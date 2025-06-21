//! Interface synthesis module
//! 
//! This module handles the synthesis of interface instances into netlists.
//! When an interface is instantiated, it generates:
//! 1. Nets for each signal in the interface
//! 2. Components for interface requirements (pullups, termination, etc.)

use anyhow::{Result, Context};
use std::collections::HashMap;
use log::{debug, info, warn};

use bhdl_analyzer::{
    symbol_table::{Symbol, SymbolKind, SymbolTable},
    types::AnalysisResult,
};
use bhdl_ast::{
    AstNode, InterfaceDef,
    interfaces::{InterfaceSignal, InterfaceRequirement, SignalDirection},
};
use bhdl_netlist::{Netlist, ModuleId, InstanceId, NetId};
use bhdl_parser::{SyntaxKind, BhdlLanguage};

use crate::NetlistGenerator;

/// Information about an interface instance
#[derive(Debug, Clone)]
pub struct InterfaceInstance {
    pub instance_name: String,
    pub interface_type: String,
    pub parameter_overrides: HashMap<String, String>,
}

/// Information about an interface signal
#[derive(Debug, Clone)]
pub struct InterfaceSignalInfo {
    pub name: String,
    pub direction: SignalDirection,
    pub is_optional: bool,
}

/// Information about an interface requirement
#[derive(Debug, Clone)]
pub struct InterfaceRequirementInfo {
    pub requirement_type: String,
    pub arguments: Vec<String>,
}

/// Resolved interface parameter value
#[derive(Debug, Clone)]
pub struct InterfaceParameter {
    pub name: String,
    pub value: String,
    pub parameter_type: String,
}

impl NetlistGenerator {
    /// Synthesize all interface instances in the design
    pub fn synthesize_interfaces(&mut self, analysis: &AnalysisResult) -> Result<()> {
        info!("Starting interface synthesis");
        
        // Find all interface instances - they may be in component inference results
        let mut interface_instances = Vec::new();
        
        // Map to track original names to generated names
        let mut original_to_generated: HashMap<String, String> = HashMap::new();
        
        // Check component inference results for interface types
        for (idx, comp) in analysis.component_inference.inferred_components.iter().enumerate() {
            // Check if the component type is actually an interface
            let is_interface = if let Some(type_symbol) = analysis.global_scope.lookup(&comp.component_type) {
                type_symbol.kind == SymbolKind::Interface
            } else {
                // Check if it's a hardcoded interface type
                self.is_hardcoded_interface_type(&comp.component_type)
            };
            
            if is_interface {
                // The generated instance name (e.g., U1)
                let generated_name = comp.instance_name.clone()
                    .unwrap_or_else(|| format!("interface_{}", idx));
                
                // Try to find the original instance name from the instance_name field
                // For interfaces, the instance_name might preserve the original name
                let original_name = generated_name.clone();
                
                info!("Found interface instance '{}' (generated: '{}') of type {}", 
                      original_name, generated_name, comp.component_type);
                
                // Store the mapping
                original_to_generated.insert(original_name.clone(), generated_name.clone());
                
                // Use parameter overrides from component inference
                let instance = InterfaceInstance {
                    instance_name: generated_name,
                    interface_type: comp.component_type.clone(),
                    parameter_overrides: comp.parameter_overrides.clone(),
                };
                interface_instances.push(instance);
            }
        }
        
        // Also check the symbol table (in case they're added there in the future)
        let symbol_instances = self.find_interface_instances(&analysis.global_scope);
        interface_instances.extend(symbol_instances);
        
        info!("Found {} interface instances to synthesize", interface_instances.len());
        
        for instance in interface_instances {
            self.synthesize_interface_instance(&instance, analysis)?;
        }
        
        // Log the interface instance mapping for debugging
        for (original, generated) in original_to_generated {
            info!("Interface instance mapping: {} -> {}", original, generated);
        }
        
        Ok(())
    }
    
    /// Find all interface instances in the symbol table
    fn find_interface_instances(&self, symbol_table: &SymbolTable) -> Vec<InterfaceInstance> {
        let mut instances = Vec::new();
        
        info!("Searching for interface instances in symbol table");
        
        for symbol in symbol_table.iter() {
            debug!("Checking symbol: {} (kind: {:?})", symbol.name, symbol.kind);
            
            if symbol.kind == SymbolKind::Instance {
                // Check if this instance is of an interface type
                if let Some(type_name) = &symbol.instance_type_name {
                    debug!("  Instance {} has type {}", symbol.name, type_name);
                    
                    // Look up the type to see if it's an interface
                    if let Some(type_symbol) = symbol_table.lookup(type_name) {
                        debug!("  Type {} has kind {:?}", type_name, type_symbol.kind);
                        
                        if type_symbol.kind == SymbolKind::Interface {
                            info!("Found interface instance: {} of type {}", symbol.name, type_name);
                            
                            let instance = InterfaceInstance {
                                instance_name: symbol.name.clone(),
                                interface_type: type_name.clone(),
                                parameter_overrides: HashMap::new(), // TODO: Extract parameter overrides
                            };
                            instances.push(instance);
                        }
                    } else {
                        debug!("  Type {} not found in symbol table", type_name);
                    }
                } else {
                    debug!("  Instance {} has no type name", symbol.name);
                }
            }
        }
        
        info!("Found {} interface instances total", instances.len());
        instances
    }
    
    /// Synthesize a single interface instance
    fn synthesize_interface_instance(
        &mut self, 
        instance: &InterfaceInstance,
        analysis: &AnalysisResult
    ) -> Result<()> {
        info!("Synthesizing interface instance: {} (type: {})", 
              instance.instance_name, instance.interface_type);
        
        // Resolve interface parameters
        let resolved_params = self.resolve_interface_parameters(instance, analysis)?;
        
        info!("Interface parameters: {:?}", resolved_params);
        
        // Check if a perspective is specified in the instance name or parameters
        let perspective = self.extract_perspective_from_instance(instance);
        
        // Get interface definition from analysis or use built-in interfaces
        let signals = if let Ok(interface_def) = self.get_interface_definition(&instance.interface_type, analysis) {
            // Use interface definition from AST
            self.extract_interface_signals_with_perspective(&interface_def, perspective.as_deref())
        } else {
            // Fall back to hardcoded interface definitions with perspective support
            self.get_hardcoded_interface_signals(&instance.interface_type, &resolved_params, perspective.as_deref())
        };
        
        let requirements = vec![];
        
        // Get the module ID for the board/module containing this interface
        // If no top-level module exists yet, create a temporary board module
        let module_id = if let Some(id) = self.netlist.top_level_module {
            id
        } else {
            // Create a temporary board module
            let board_id = self.netlist.add_module("Board".to_string(), bhdl_netlist::types::ModuleKind::Board);
            self.netlist.top_level_module = Some(board_id);
            board_id
        };
        
        // Create nets for each signal
        for signal in &signals {
            let net_name = format!("{}_{}", instance.instance_name, signal.name);
            debug!("Creating net for interface signal: {}", net_name);
            
            let net_id = self.netlist.add_net(Some(net_name.clone()));
            self.ast_to_net.insert(net_name, net_id);
        }
        
        // Create components for requirements
        for (idx, requirement) in requirements.iter().enumerate() {
            self.synthesize_interface_requirement(
                instance,
                requirement,
                idx,
                module_id,
                &signals
            )?;
        }
        
        Ok(())
    }
    
    /// Resolve interface parameters by combining defaults with overrides
    fn resolve_interface_parameters(
        &self,
        instance: &InterfaceInstance,
        _analysis: &AnalysisResult
    ) -> Result<Vec<InterfaceParameter>> {
        let mut resolved_params = Vec::new();
        
        // First, add default parameters based on interface type
        match instance.interface_type.as_str() {
            "SPI" => {
                // Default SPI parameters
                resolved_params.push(InterfaceParameter {
                    name: "width".to_string(),
                    value: "8".to_string(),
                    parameter_type: "int".to_string(),
                });
                resolved_params.push(InterfaceParameter {
                    name: "frequency".to_string(),
                    value: "1MHz".to_string(),
                    parameter_type: "frequency".to_string(),
                });
                resolved_params.push(InterfaceParameter {
                    name: "mode".to_string(),
                    value: "master".to_string(),
                    parameter_type: "string".to_string(),
                });
            },
            "UART" => {
                resolved_params.push(InterfaceParameter {
                    name: "baudrate".to_string(),
                    value: "9600".to_string(),
                    parameter_type: "int".to_string(),
                });
                resolved_params.push(InterfaceParameter {
                    name: "mode".to_string(),
                    value: "dte".to_string(), // Default to DTE (Data Terminal Equipment)
                    parameter_type: "string".to_string(),
                });
            },
            _ => {
                // No default parameters for unknown interfaces
            }
        }
        
        
        // Override with instance-specific parameters
        for (param_name, param_value) in &instance.parameter_overrides {
            if let Some(param) = resolved_params.iter_mut().find(|p| p.name == *param_name) {
                param.value = param_value.clone();
            } else {
                // Add new parameter if not in defaults
                resolved_params.push(InterfaceParameter {
                    name: param_name.clone(),
                    value: param_value.clone(),
                    parameter_type: "unknown".to_string(),
                });
            }
        }
        
        Ok(resolved_params)
    }
    
    /// Get the interface definition from the analysis result
    fn get_interface_definition(
        &self,
        interface_type: &str,
        _analysis: &AnalysisResult
    ) -> Result<InterfaceDef> {
        // For now, we can't easily access the AST from here
        // This would require storing a reference to the AST in the synthesizer
        // TODO: Implement proper AST access for interface definitions
        Err(anyhow::anyhow!("Cannot access interface definition AST - need source file reference"))
    }
    
    /// Check if a component type is a hardcoded interface type
    fn is_hardcoded_interface_type(&self, component_type: &str) -> bool {
        matches!(component_type, "SPI" | "I2C" | "UART" | "USB" | "CAN" | "Ethernet")
    }
    
    /// Extract perspective from interface instance
    fn extract_perspective_from_instance(&self, instance: &InterfaceInstance) -> Option<String> {
        // Check for "mode" parameter (since "perspective" is a keyword)
        instance.parameter_overrides.get("mode").cloned()
            .or_else(|| instance.parameter_overrides.get("perspective").cloned())
    }
    
    /// Get hardcoded interface signals with perspective support
    fn get_hardcoded_interface_signals(
        &self, 
        interface_type: &str, 
        resolved_params: &[InterfaceParameter],
        perspective: Option<&str>
    ) -> Vec<InterfaceSignalInfo> {
        match interface_type {
            "I2C" => vec![
                InterfaceSignalInfo {
                    name: "SDA".to_string(),
                    direction: SignalDirection::InOut, // Always bidirectional for I2C
                    is_optional: false,
                },
                InterfaceSignalInfo {
                    name: "SCL".to_string(),
                    direction: if perspective == Some("slave") { 
                        SignalDirection::In 
                    } else { 
                        SignalDirection::Out // Default to master
                    },
                    is_optional: false,
                },
            ],
            "SPI" => {
                // Use parameters to determine interface signals
                let _width = resolved_params.iter()
                    .find(|p| p.name == "width")
                    .map(|p| p.value.parse::<i32>().unwrap_or(8))
                    .unwrap_or(8);
                
                info!("Creating SPI interface with perspective={:?}", perspective);
                
                // SPI signal directions depend on master/slave perspective
                let is_slave = perspective == Some("slave");
                
                vec![
                    InterfaceSignalInfo {
                        name: "MOSI".to_string(),
                        direction: if is_slave { SignalDirection::In } else { SignalDirection::Out },
                        is_optional: false,
                    },
                    InterfaceSignalInfo {
                        name: "MISO".to_string(),
                        direction: if is_slave { SignalDirection::Out } else { SignalDirection::In },
                        is_optional: false,
                    },
                    InterfaceSignalInfo {
                        name: "SCK".to_string(),
                        direction: if is_slave { SignalDirection::In } else { SignalDirection::Out },
                        is_optional: false,
                    },
                    InterfaceSignalInfo {
                        name: "CS".to_string(),
                        direction: if is_slave { SignalDirection::In } else { SignalDirection::Out },
                        is_optional: true,
                    },
                ]
            },
            "UART" => {
                let is_dce = perspective == Some("dce"); // Data Circuit-terminating Equipment (modem)
                
                vec![
                    InterfaceSignalInfo {
                        name: "TX".to_string(),
                        direction: if is_dce { SignalDirection::In } else { SignalDirection::Out },
                        is_optional: false,
                    },
                    InterfaceSignalInfo {
                        name: "RX".to_string(),
                        direction: if is_dce { SignalDirection::Out } else { SignalDirection::In },
                        is_optional: false,
                    },
                ]
            },
            _ => {
                warn!("Unknown interface type: {}", interface_type);
                vec![]
            }
        }
    }
    
    /// Extract signals from an interface definition
    fn extract_interface_signals(&self, interface_def: &InterfaceDef) -> Vec<InterfaceSignalInfo> {
        // Use default perspective
        self.extract_interface_signals_with_perspective(interface_def, None)
    }
    
    /// Extract signals from an interface definition with perspective support
    fn extract_interface_signals_with_perspective(
        &self, 
        interface_def: &InterfaceDef, 
        perspective: Option<&str>
    ) -> Vec<InterfaceSignalInfo> {
        let mut signals = Vec::new();
        
        // First, try to find a specific perspective block
        if let Some(perspective_name) = perspective {
            for perspective_block in interface_def.perspectives() {
                if let Some(name) = perspective_block.name() {
                    if name.text() == perspective_name {
                        // Use signals from this perspective
                        for signal in perspective_block.signals() {
                            if let Some(name) = signal.name() {
                                let info = InterfaceSignalInfo {
                                    name: name.text().to_string(),
                                    direction: signal.direction().unwrap_or(SignalDirection::InOut),
                                    is_optional: signal.is_optional(),
                                };
                                signals.push(info);
                            }
                        }
                        return signals;
                    }
                }
            }
        }
        
        // Fallback to default signals if no perspective found
        for signal in interface_def.signals() {
            if let Some(name) = signal.name() {
                let info = InterfaceSignalInfo {
                    name: name.text().to_string(),
                    direction: signal.direction().unwrap_or(SignalDirection::InOut),
                    is_optional: signal.is_optional(),
                };
                signals.push(info);
            }
        }
        
        signals
    }
    
    /// Extract requirements from an interface definition
    fn extract_interface_requirements(&self, interface_def: &InterfaceDef) -> Vec<InterfaceRequirementInfo> {
        let mut requirements = Vec::new();
        
        for requirement in interface_def.requirements() {
            if let Some(req_type) = requirement.requirement_type() {
                let args: Vec<String> = requirement.arguments()
                    .into_iter()
                    .map(|expr| expr.syntax().text().to_string())
                    .collect();
                
                let info = InterfaceRequirementInfo {
                    requirement_type: req_type.text().to_string(),
                    arguments: args,
                };
                requirements.push(info);
            }
        }
        
        requirements
    }
    
    /// Synthesize a single interface requirement (e.g., pullup resistor)
    fn synthesize_interface_requirement(
        &mut self,
        instance: &InterfaceInstance,
        requirement: &InterfaceRequirementInfo,
        requirement_idx: usize,
        module_id: ModuleId,
        signals: &[InterfaceSignalInfo]
    ) -> Result<()> {
        match requirement.requirement_type.as_str() {
            "pullup" => {
                // Syntax: require pullup(signal_name, resistance);
                if requirement.arguments.len() >= 2 {
                    let signal_name = &requirement.arguments[0];
                    let resistance = &requirement.arguments[1];
                    
                    // Create a pullup resistor
                    let resistor_name = format!("{}_{}_pullup_{}", 
                                               instance.instance_name, signal_name, requirement_idx);
                    
                    debug!("Creating pullup resistor {} with value {}", resistor_name, resistance);
                    
                    // Create resistor module if not exists
                    let resistor_module_id = self.get_or_create_resistor_module();
                    
                    // Create resistor instance
                    let resistor_instance_id = self.netlist.add_instance(
                        resistor_name.clone(),
                        resistor_module_id
                    ).ok_or_else(|| anyhow::anyhow!("Failed to create pullup resistor instance"))?;
                    
                    // Create pin instances
                    self.netlist.create_pin_instances(resistor_instance_id)
                        .map_err(|e| anyhow::anyhow!("Failed to create pin instances for pullup resistor: {}", e))?;
                    
                    // TODO: Connect the resistor between the signal net and VCC
                    // This requires knowing which net is VCC and which is the signal
                }
            }
            "termination" => {
                // Handle termination resistors
                debug!("Termination requirement not yet implemented");
            }
            _ => {
                debug!("Unknown interface requirement type: {}", requirement.requirement_type);
            }
        }
        
        Ok(())
    }
    
    /// Get or create a resistor module
    fn get_or_create_resistor_module(&mut self) -> ModuleId {
        // Check if we already have a resistor module
        if let Some(module_id) = self.ast_to_module.get("Resistor") {
            return *module_id;
        }
        
        // Create a new resistor module
        let module_id = self.netlist.add_module(
            "Resistor".to_string(),
            bhdl_netlist::types::ModuleKind::Component
        );
        
        // Add pins to the resistor module
        self.netlist.add_pin(
            module_id,
            "1".to_string(),
            bhdl_netlist::types::PinDirection::Passive,
            bhdl_netlist::types::PinType::Passive
        );
        
        self.netlist.add_pin(
            module_id,
            "2".to_string(),
            bhdl_netlist::types::PinDirection::Passive,
            bhdl_netlist::types::PinType::Passive
        );
        
        self.ast_to_module.insert("Resistor".to_string(), module_id);
        module_id
    }
}

