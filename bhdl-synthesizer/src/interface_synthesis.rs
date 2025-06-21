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
    AstNode, HasName, InterfaceDef,
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

impl NetlistGenerator {
    /// Synthesize all interface instances in the design
    pub fn synthesize_interfaces(&mut self, analysis: &AnalysisResult) -> Result<()> {
        info!("Starting interface synthesis");
        
        // Find all interface instances - they may be in component inference results
        let mut interface_instances = Vec::new();
        
        // Check component inference results for interface types
        for (idx, comp) in analysis.component_inference.inferred_components.iter().enumerate() {
            // Check if the component type is actually an interface
            if let Some(type_symbol) = analysis.global_scope.lookup(&comp.component_type) {
                if type_symbol.kind == SymbolKind::Interface {
                    // Use the instance name from component inference
                    // In the future, we should preserve the original instance name from the AST
                    let instance_name = comp.instance_name.clone()
                        .unwrap_or_else(|| format!("interface_{}", idx));
                    
                    info!("Found interface instance in component inference: {} of type {}", 
                          instance_name, comp.component_type);
                    
                    let instance = InterfaceInstance {
                        instance_name,
                        interface_type: comp.component_type.clone(),
                        parameter_overrides: HashMap::new(),
                    };
                    interface_instances.push(instance);
                }
            }
        }
        
        // Also check the symbol table (in case they're added there in the future)
        let symbol_instances = self.find_interface_instances(&analysis.global_scope);
        interface_instances.extend(symbol_instances);
        
        info!("Found {} interface instances to synthesize", interface_instances.len());
        
        for instance in interface_instances {
            self.synthesize_interface_instance(&instance, analysis)?;
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
        
        // For now, use hardcoded interface definitions until we can access the AST
        let signals = match instance.interface_type.as_str() {
            "I2C" => vec![
                InterfaceSignalInfo {
                    name: "SDA".to_string(),
                    direction: SignalDirection::InOut,
                    is_optional: false,
                },
                InterfaceSignalInfo {
                    name: "SCL".to_string(),
                    direction: SignalDirection::Out,
                    is_optional: false,
                },
            ],
            "UART" => vec![
                InterfaceSignalInfo {
                    name: "TX".to_string(),
                    direction: SignalDirection::Out,
                    is_optional: false,
                },
                InterfaceSignalInfo {
                    name: "RX".to_string(),
                    direction: SignalDirection::In,
                    is_optional: false,
                },
            ],
            _ => {
                warn!("Unknown interface type: {}", instance.interface_type);
                vec![]
            }
        };
        let requirements = vec![];
        
        // Get the module ID for the board/module containing this interface
        // For now, assume it's in the top-level module
        let module_id = self.netlist.top_level_module
            .ok_or_else(|| anyhow::anyhow!("No top-level module found"))?;
        
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
    
    /// Get the interface definition from the analysis result
    fn get_interface_definition(
        &self,
        interface_type: &str,
        analysis: &AnalysisResult
    ) -> Result<InterfaceDef> {
        // Look up the interface in the global scope
        let interface_symbol = analysis.global_scope.lookup(interface_type)
            .ok_or_else(|| anyhow::anyhow!("Interface type '{}' not found", interface_type))?;
        
        // Get the definition node
        let def_node_ptr = interface_symbol.definition_node_ptr.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Interface '{}' has no definition node", interface_type))?;
        
        // Get the actual syntax node from the source file
        // This is a bit tricky - we need access to the source file root
        // For now, we'll return an error - this needs to be improved
        Err(anyhow::anyhow!("Cannot access interface definition AST - need source file reference"))
    }
    
    /// Extract signals from an interface definition
    fn extract_interface_signals(&self, interface_def: &InterfaceDef) -> Vec<InterfaceSignalInfo> {
        let mut signals = Vec::new();
        
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

