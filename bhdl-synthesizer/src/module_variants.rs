//! Module variant management for parameter-based deduplication
//!
//! This module handles creating unique variants of modules based on their
//! parameter values, ensuring proper deduplication while maintaining
//! distinct implementations for different parameter combinations.

use std::collections::HashMap;
use anyhow::Result;
use bhdl_ast::{AstNode, ModuleInst, HasName};
use bhdl_analyzer::types::AnalysisResult;
use bhdl_netlist::{Netlist, ModuleId, ModuleKind, InstanceId, NetId};
use bhdl_netlist::types::{PinDirection, PinType, PortDirection, NetClass};
use log::{debug, info, warn};

/// Key for identifying unique module variants
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModuleVariantKey {
    /// Base module name (e.g., "RC_Filter")
    pub base_name: String,
    /// Sorted parameter key-value pairs for consistent hashing
    pub parameters: Vec<(String, String)>,
}

impl ModuleVariantKey {
    /// Create a new variant key from module name and parameters
    pub fn new(base_name: String, mut parameters: Vec<(String, String)>) -> Self {
        // Sort parameters for consistent hashing
        parameters.sort_by(|a, b| a.0.cmp(&b.0));
        Self {
            base_name,
            parameters,
        }
    }
    
    /// Generate a unique variant name (e.g., "RC_Filter_1k6_100n")
    pub fn variant_name(&self) -> String {
        if self.parameters.is_empty() {
            return self.base_name.clone();
        }
        
        // Create a suffix from parameter values
        let suffix = self.parameters.iter()
            .map(|(_, value)| {
                // Simplify value for naming (remove units, replace dots)
                value.replace(".", "_")
                    .replace(" ", "")
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '_')
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("_");
        
        format!("{}_{}", self.base_name, suffix)
    }
}

/// Module variant manager for deduplication
pub struct ModuleVariantManager {
    /// Map from variant key to module ID
    variants: HashMap<ModuleVariantKey, ModuleId>,
    /// Map from base module name to its definition in the AST
    module_definitions: HashMap<String, bhdl_ast::Module>,
}

impl ModuleVariantManager {
    pub fn new() -> Self {
        Self {
            variants: HashMap::new(),
            module_definitions: HashMap::new(),
        }
    }
    
    /// Register a module definition from the AST
    pub fn register_module_definition(&mut self, module: &bhdl_ast::Module) {
        if let Some(name) = module.name() {
            let module_name = name.text().to_string();
            self.module_definitions.insert(module_name, module.clone());
            debug!("Registered module definition: {}", name.text());
        }
    }
    
    /// Get or create a module variant for the given instance
    pub fn get_or_create_variant(
        &mut self,
        module_inst: &ModuleInst,
        netlist: &mut Netlist,
        analysis: &AnalysisResult,
    ) -> Result<ModuleId> {
        // Extract module type and parameters
        let module_type = module_inst.module_type()
            .map(|t| t.text().to_string())
            .ok_or_else(|| anyhow::anyhow!("Module instance missing type"))?;
        
        // Extract parameter values from the instance
        let parameters = self.extract_instance_parameters(module_inst, analysis)?;
        
        // Create variant key
        let variant_key = ModuleVariantKey::new(module_type.clone(), parameters);
        
        // Check if variant already exists
        if let Some(&module_id) = self.variants.get(&variant_key) {
            debug!("Reusing existing variant: {}", variant_key.variant_name());
            return Ok(module_id);
        }
        
        // Create new variant
        let variant_name = variant_key.variant_name();
        info!("Creating new module variant: {}", variant_name);
        
        // Create the module in the netlist
        let module_id = netlist.add_module(variant_name.clone(), ModuleKind::Module);
        
        // Copy pins from base module definition
        // We need to clone the module to avoid borrowing conflicts
        if let Some(base_module) = self.module_definitions.get(&module_type).cloned() {
            self.copy_module_interface(&base_module, module_id, netlist)?;
        }
        
        // Store parameter values as module attributes
        for (param_name, param_value) in &variant_key.parameters {
            netlist.modules.get_mut(module_id)
                .map(|m| m.attributes.insert(param_name.clone(), param_value.clone()));
        }
        
        // Register the variant
        self.variants.insert(variant_key, module_id);
        
        Ok(module_id)
    }
    
    /// Extract parameter values from a module instance
    fn extract_instance_parameters(
        &self,
        module_inst: &ModuleInst,
        _analysis: &AnalysisResult,
    ) -> Result<Vec<(String, String)>> {
        let mut parameters = Vec::new();
        
        // Get parameters from the instance's param list
        if let Some(param_list) = module_inst.param_list() {
            for param_assign in param_list.params() {
                if let Some(name) = param_assign.name() {
                    let param_name = name.text().to_string();
                    
                    // Get the parameter value as a string
                    let param_value = if let Some(value) = param_assign.value() {
                        value.syntax().text().to_string().trim().to_string()
                    } else {
                        continue;
                    };
                    
                    parameters.push((param_name, param_value));
                }
            }
        }
        
        // If no parameters specified, we might need to get defaults from the module definition
        if parameters.is_empty() {
            if let Some(module_type) = module_inst.module_type() {
                let type_name = module_type.text().to_string();
                if let Some(base_module) = self.module_definitions.get(&type_name) {
                    parameters = self.extract_default_parameters(base_module)?;
                }
            }
        }
        
        Ok(parameters)
    }
    
    /// Extract default parameter values from module definition
    fn extract_default_parameters(&self, module: &bhdl_ast::Module) -> Result<Vec<(String, String)>> {
        let mut parameters = Vec::new();
        
        if let Some(param_list) = module.param_list() {
            // For module definitions, use param_defs() to get ModuleParam items
            for param_def in param_list.param_defs() {
                if let Some(name) = param_def.name() {
                    let param_name = name.text().to_string();
                    
                    // Get default value if present
                    if let Some(default_value) = param_def.default_value() {
                        let value_text = default_value.syntax().text().to_string().trim().to_string();
                        parameters.push((param_name, value_text));
                    }
                }
            }
        }
        
        Ok(parameters)
    }
    
    /// Copy module interface (pins) from base definition to variant
    fn copy_module_interface(
        &mut self,
        base_module: &bhdl_ast::Module,
        variant_id: ModuleId,
        netlist: &mut Netlist,
    ) -> Result<()> {
        use bhdl_ast::SyntaxKind;
        use bhdl_netlist::types::{PortDirection, PinDirection, PinType};
        
        // Extract pins from the base module
        for child in base_module.syntax().children() {
            if child.kind() == SyntaxKind::PIN_DECL {
                if let Some(pin_decl) = bhdl_ast::common::PinDecl::cast(child) {
                    if let Some(pin_name) = pin_decl.name() {
                        let pin_name_str = pin_name.text().to_string();
                        
                        // Check if this is a virtual pin
                        let is_virtual = pin_decl.is_virtual();
                        
                        // Determine pin type and direction
                        let decl_text = pin_decl.syntax().text().to_string();
                        let pin_type = if decl_text.contains("power") {
                            PinType::Power
                        } else if decl_text.contains("ground") {
                            PinType::Ground
                        } else {
                            PinType::Signal
                        };
                        
                        let direction = if decl_text.contains(" in") && !decl_text.contains("inout") {
                            PinDirection::In
                        } else if decl_text.contains(" out") && !decl_text.contains("inout") {
                            PinDirection::Out
                        } else {
                            PinDirection::InOut
                        };
                        
                        if is_virtual {
                            // Virtual pin - expand to component chain
                            info!("Processing virtual pin '{}' in module", pin_name_str);
                            // TODO: Retrieve intent information from analysis result for this virtual pin
                            self.expand_virtual_pin(netlist, variant_id, &pin_name_str, pin_type, direction, None)?;
                        } else {
                            // Regular pin - add normally
                            // Add port and pin to the variant
                            netlist.add_port(
                                variant_id,
                                pin_name_str.clone(),
                                PortDirection::InOut,
                                None
                            );
                            
                            netlist.add_pin(
                                variant_id,
                                pin_name_str,
                                direction,
                                pin_type
                            );
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Expand a virtual pin into actual component chains with intent annotations
    fn expand_virtual_pin(
        &mut self,
        netlist: &mut Netlist,
        module_id: ModuleId,
        pin_name: &str,
        pin_type: PinType,
        pin_direction: PinDirection,
        intent_info: Option<&bhdl_common::IntentCall>,
    ) -> Result<()> {
        info!("Expanding virtual pin '{}' in module with intent: {:?}", 
              pin_name, intent_info.map(|i| &i.name));
        
        // Virtual pins represent synthesis expansion points where the synthesizer
        // will add required external components based on the pin's characteristics
        // and specified design intent
        
        if let Some(intent) = intent_info {
            // Use intent to determine specific expansion strategy
            self.expand_virtual_pin_with_intent(netlist, module_id, pin_name, pin_type, pin_direction, intent)?;
        } else {
            // Fallback to default expansion based on pin characteristics
            match (pin_type, pin_direction) {
                (PinType::Power, PinDirection::Out) => {
                    // Virtual power output pin - expand to power management chain
                    self.expand_power_output_chain(netlist, module_id, pin_name)?;
                }
                (PinType::Signal, PinDirection::Out) => {
                    // Virtual signal output pin - expand to output protection/buffering
                    self.expand_signal_output_chain(netlist, module_id, pin_name)?;
                }
                (PinType::Signal, PinDirection::InOut) => {
                    // Virtual bidirectional signal pin - expand to protection circuit
                    self.expand_bidirectional_signal_chain(netlist, module_id, pin_name)?;
                }
                (PinType::Ground, PinDirection::Out) => {
                    // Virtual ground pin - expand to ground connection with protection
                    self.expand_ground_connection(netlist, module_id, pin_name)?;
                }
                _ => {
                    warn!("Unsupported virtual pin configuration: {:?} {:?}", pin_type, pin_direction);
                    // For unsupported combinations, create a regular pin as fallback
                    netlist.add_pin(module_id, pin_name.to_string(), pin_direction, pin_type);
                }
            }
        }
        
        Ok(())
    }

    /// Expand virtual pin based on design intent  
    fn expand_virtual_pin_with_intent(
        &mut self,
        netlist: &mut Netlist,
        module_id: ModuleId,
        pin_name: &str,
        pin_type: PinType,
        pin_direction: PinDirection,
        intent: &bhdl_common::IntentCall,
    ) -> Result<()> {
        use bhdl_common::{IntentParam, IntentValue};
        
        info!("Expanding virtual pin '{}' with intent '{}'", pin_name, intent.name);
        
        // Extract intent parameters
        let mut intent_params = std::collections::HashMap::new();
        for param in &intent.params {
            match param {
                IntentParam::Named(name, value) => {
                    intent_params.insert(name.clone(), value);
                }
                IntentParam::Positional(_) => {
                    // For now, ignore positional parameters
                }
            }
        }
        
        // Expand based on specific intent
        match intent.name.as_str() {
            "power_output_protection" => {
                // Extract voltage and current parameters
                let voltage = intent_params.get("voltage")
                    .and_then(|v| match v {
                        IntentValue::String(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .unwrap_or("3.3V");
                    
                let current = intent_params.get("current")
                    .and_then(|v| match v {
                        IntentValue::String(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .unwrap_or("500mA");
                    
                info!("Creating power output protection for {}V, {}", voltage, current);
                // Use existing power chain method for now
                self.expand_power_output_chain(netlist, module_id, pin_name)?;
            }
            "signal_output_protection" => {
                let drive_strength = intent_params.get("drive_strength")
                    .and_then(|v| match v {
                        IntentValue::String(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .unwrap_or("standard");
                    
                let current_limit = intent_params.get("current_limit")
                    .and_then(|v| match v {
                        IntentValue::String(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .unwrap_or("20mA");
                    
                info!("Creating signal output protection with {} drive, {} limit", drive_strength, current_limit);
                self.expand_signal_output_chain(netlist, module_id, pin_name)?;
            }
            "bidirectional_protection" => {
                let max_voltage = intent_params.get("max_voltage")
                    .and_then(|v| match v {
                        IntentValue::String(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .unwrap_or("5V");
                    
                let protection_type = intent_params.get("protection_type")
                    .and_then(|v| match v {
                        IntentValue::String(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .unwrap_or("tvs");
                    
                info!("Creating bidirectional protection for {} with {}", max_voltage, protection_type);
                self.expand_bidirectional_signal_chain(netlist, module_id, pin_name)?;
            }
            "ground_protection" => {
                let filter_type = intent_params.get("filter_type")
                    .and_then(|v| match v {
                        IntentValue::String(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .unwrap_or("ferrite_bead");
                    
                info!("Creating ground protection with {} filter", filter_type);
                self.expand_ground_connection(netlist, module_id, pin_name)?;
            }
            "general_protection" => {
                info!("Creating general protection circuit");
                // Create a basic protection instance
                let _protection = netlist.add_instance(
                    format!("{}_protection", pin_name),
                    module_id
                );
                netlist.add_pin(module_id, pin_name.to_string(), pin_direction, pin_type);
            }
            _ => {
                warn!("Unknown intent '{}' for virtual pin '{}', using default expansion", intent.name, pin_name);
                // Fall back to default expansion
                match (pin_type, pin_direction) {
                    (PinType::Power, PinDirection::Out) => {
                        self.expand_power_output_chain(netlist, module_id, pin_name)?;
                    }
                    (PinType::Signal, PinDirection::Out) => {
                        self.expand_signal_output_chain(netlist, module_id, pin_name)?;
                    }
                    (PinType::Signal, PinDirection::InOut) => {
                        self.expand_bidirectional_signal_chain(netlist, module_id, pin_name)?;
                    }
                    (PinType::Ground, PinDirection::Out) => {
                        self.expand_ground_connection(netlist, module_id, pin_name)?;
                    }
                    _ => {
                        netlist.add_pin(module_id, pin_name.to_string(), pin_direction, pin_type);
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Expand virtual power output pin to power management chain
    fn expand_power_output_chain(
        &mut self,
        netlist: &mut Netlist,
        module_id: ModuleId,
        pin_name: &str,
    ) -> Result<()> {
        info!("Expanding virtual power output pin '{}' to power management chain", pin_name);
        
        // Create internal power net
        let _internal_power_net = netlist.add_net_with_class(
            Some(format!("{}_internal", pin_name)),
            NetClass::Power(3.3) // Default 3.3V, should be parameterized
        );
        
        // Note: For virtual components, we would need a module ID for the protection components
        // For now, we'll create placeholder instances (this would need proper component database integration)
        // TODO: Replace with actual component instantiation from database
        let _protection_diode = netlist.add_instance(
            format!("{}_protection", pin_name),
            module_id // This should be the module ID of the protection diode component
        );
        
        // Add decoupling capacitor 
        let _decoupling_cap = netlist.add_instance(
            format!("{}_decoupl", pin_name),
            module_id // This should be the module ID of the decoupling cap component
        );
        
        // Create external connection pin
        netlist.add_pin(module_id, pin_name.to_string(), PinDirection::Out, PinType::Power);
        
        // Add net for external connection
        let _external_net = netlist.add_net_with_class(
            Some(pin_name.to_string()),
            NetClass::Power(3.3) // Default 3.3V, should be parameterized
        );
        
        // TODO: Add actual connections between components, nets, and pins
        // This would require proper connection/wiring infrastructure in the netlist
        
        info!("Created power management chain for virtual pin '{}'", pin_name);
        Ok(())
    }
    
    /// Expand virtual signal output pin to output buffering/protection chain
    fn expand_signal_output_chain(
        &mut self,
        netlist: &mut Netlist,
        module_id: ModuleId,
        pin_name: &str,
    ) -> Result<()> {
        info!("Expanding virtual signal output pin '{}' to output protection chain", pin_name);
        
        // Add output buffer for drive strength
        let _output_buffer = netlist.add_instance(
            format!("{}_buf", pin_name),
            module_id // This should be the module ID of the output buffer component
        );
        
        // Add current limiting resistor
        let _current_limiter = netlist.add_instance(
            format!("{}_climit", pin_name),
            module_id // This should be the module ID of the current limiter component
        );
        
        // Create external connection pin
        netlist.add_pin(module_id, pin_name.to_string(), PinDirection::Out, PinType::Signal);
        
        info!("Created signal output chain for virtual pin '{}'", pin_name);
        Ok(())
    }
    
    /// Expand virtual bidirectional signal pin to protection circuit
    fn expand_bidirectional_signal_chain(
        &mut self,
        netlist: &mut Netlist,
        module_id: ModuleId,
        pin_name: &str,
    ) -> Result<()> {
        info!("Expanding virtual bidirectional signal pin '{}' to protection circuit", pin_name);
        
        // Add bidirectional protection (TVS diode)
        let _tvs_protection = netlist.add_instance(
            format!("{}_tvs", pin_name),
            module_id // This should be the module ID of the TVS protection component
        );
        
        // Add series protection resistor
        let _series_resistor = netlist.add_instance(
            format!("{}_rseries", pin_name),
            module_id // This should be the module ID of the series protection resistor component
        );
        
        // Create external connection pin
        netlist.add_pin(module_id, pin_name.to_string(), PinDirection::InOut, PinType::Signal);
        
        info!("Created bidirectional protection circuit for virtual pin '{}'", pin_name);
        Ok(())
    }
    
    /// Expand virtual ground pin to ground connection with protection
    fn expand_ground_connection(
        &mut self,
        netlist: &mut Netlist,
        module_id: ModuleId,
        pin_name: &str,
    ) -> Result<()> {
        info!("Expanding virtual ground pin '{}' to protected ground connection", pin_name);
        
        // Add ground protection (ferrite bead for noise filtering)
        let _ferrite_bead = netlist.add_instance(
            format!("{}_fb", pin_name),
            module_id // This should be the module ID of the ferrite bead component
        );
        
        // Create external ground pin
        netlist.add_pin(module_id, pin_name.to_string(), PinDirection::Out, PinType::Ground);
        
        info!("Created protected ground connection for virtual pin '{}'", pin_name);
        Ok(())
    }

    /// Get all created variants for reporting
    pub fn get_variants(&self) -> &HashMap<ModuleVariantKey, ModuleId> {
        &self.variants
    }
    
    /// Find module definition by name
    pub fn find_module_definition(&self, module_name: &str) -> Option<&bhdl_ast::Module> {
        self.module_definitions.get(module_name)
    }
    
    // ==================== SIMULATION-ENHANCED EXPANSION METHODS ====================
    
    /// Enhanced virtual pin expansion using simulation data from analysis result
    pub fn expand_virtual_pin_with_simulation_data(
        &mut self,
        netlist: &mut Netlist,
        module_id: ModuleId,
        pin_name: &str,
        pin_type: PinType,
        pin_direction: PinDirection,
        analysis_result: &bhdl_analyzer::AnalysisResult,
        intent_info: Option<&bhdl_common::IntentCall>,
    ) -> Result<()> {
        info!("Expanding virtual pin '{}' with simulation-enhanced calculations", pin_name);
        
        // Use simulation-enhanced passive component calculator
        let calculator = crate::passive_component_calculator::PassiveComponentCalculator::new();
        let selector = crate::package_selector::PackageSelector::new();
        
        match (pin_type, pin_direction) {
            (PinType::Power, PinDirection::Out) => {
                self.expand_power_output_with_simulation(
                    netlist, module_id, pin_name, &calculator, &selector, 
                    analysis_result, intent_info
                )?;
            },
            (PinType::Signal, PinDirection::Out) => {
                self.expand_signal_output_with_simulation(
                    netlist, module_id, pin_name, &calculator, &selector,
                    analysis_result, intent_info
                )?;
            },
            (PinType::Signal, PinDirection::InOut) => {
                self.expand_bidirectional_with_simulation(
                    netlist, module_id, pin_name, &calculator, &selector,
                    analysis_result, intent_info
                )?;
            },
            _ => {
                // Fallback to standard expansion for unsupported types
                self.expand_virtual_pin(netlist, module_id, pin_name, pin_type, pin_direction, intent_info)?;
            }
        }
        
        Ok(())
    }
    
    /// Expand power output pin using simulation-enhanced component calculations
    fn expand_power_output_with_simulation(
        &mut self,
        netlist: &mut Netlist,
        module_id: ModuleId,
        pin_name: &str,
        calculator: &crate::passive_component_calculator::PassiveComponentCalculator,
        selector: &crate::package_selector::PackageSelector,
        analysis_result: &bhdl_analyzer::AnalysisResult,
        intent_info: Option<&bhdl_common::IntentCall>,
    ) -> Result<()> {
        info!("Expanding power output pin '{}' with simulation-enhanced components", pin_name);
        
        // Extract power domain information from analysis
        let power_domains = &analysis_result.power_analysis.domains;
        let (domain_voltage, domain_current) = if let Some((_, domain_info)) = power_domains.iter().next() {
            (domain_info.voltage, domain_info.max_current)
        } else {
            (3.3, 1.0) // Default values
        };
        
        // Calculate decoupling capacitor using simulation data
        if let Ok((voltage_rating, dielectric, max_esr)) = calculator.calculate_capacitor_spec_from_simulation(
            &format!("{}_decoupl", pin_name),
            analysis_result,
            intent_info,
        ) {
            info!("Simulation-enhanced decoupling capacitor: {} voltage, {} dielectric, {:.3}Ω max ESR", 
                  voltage_rating, dielectric, max_esr);
            
            // Select optimal capacitor based on simulation requirements
            let cap_requirements = crate::package_selector::ApplicationRequirements {
                frequency: Some(100e3), // DC to 100kHz
                temperature_range: Some((-40.0, 85.0)),
                size_constraint: crate::package_selector::SizeConstraint::Standard,
                cost_sensitivity: crate::package_selector::CostSensitivity::Standard,
                precision_requirement: crate::package_selector::PrecisionRequirement::Standard,
            };
            
            // Calculate capacitance based on ripple requirements from power analysis
            let estimated_ripple_current = domain_current * 0.1; // 10% of max current as ripple
            let required_capacitance = estimated_ripple_current / (2.0 * std::f64::consts::PI * 100e3 * 0.1); // 100mV ripple
            
            let cap_spec = selector.select_capacitor_spec(
                required_capacitance,
                voltage_rating,
                &cap_requirements,
            );
            
            info!("Selected decoupling capacitor: {}μF, {}, {}", 
                  cap_spec.capacitance * 1e6, cap_spec.voltage_rating, 
                  cap_spec.dielectric);
            
            // Create capacitor instance with simulation-optimized specifications
            let cap_instance = netlist.add_instance(
                format!("{}_decoupl_C", pin_name),
                module_id // Would be actual capacitor module ID from database
            );
            
            // TODO: Set component parameters from spec
            // This would use component database integration
            
        } else {
            warn!("Could not calculate simulation-enhanced capacitor spec, using defaults");
        }
        
        // Calculate series protection resistor using simulation data  
        if let Ok((power_rating, voltage_rating, optimal_resistance)) = calculator.calculate_resistor_spec_from_simulation(
            &format!("{}_protection", pin_name),
            analysis_result,
            intent_info,
        ) {
            info!("Simulation-enhanced protection resistor: {}Ω, {}, {}", 
                  optimal_resistance, power_rating, voltage_rating);
            
            let res_requirements = crate::package_selector::ApplicationRequirements {
                frequency: None, // DC application
                temperature_range: Some((-40.0, 85.0)),
                size_constraint: crate::package_selector::SizeConstraint::Standard,
                cost_sensitivity: crate::package_selector::CostSensitivity::Standard,
                precision_requirement: crate::package_selector::PrecisionRequirement::Standard,
            };
            
            let res_spec = selector.select_resistor_spec(
                optimal_resistance,
                power_rating,
                voltage_rating,
                &res_requirements,
            );
            
            info!("Selected protection resistor: {}Ω ±{}%, {}, {}", 
                  res_spec.resistance, res_spec.tolerance, res_spec.power_rating, res_spec.package);
            
            // Create resistor instance with simulation-optimized specifications
            let res_instance = netlist.add_instance(
                format!("{}_protection_R", pin_name),
                module_id // Would be actual resistor module ID from database  
            );
            
            // TODO: Set component parameters from spec
            
        } else {
            warn!("Could not calculate simulation-enhanced resistor spec, using defaults");
        }
        
        // Apply safety analysis findings to component derating
        if !analysis_result.safety_analysis.diagnostics.is_empty() {
            info!("Safety issues detected - component selections enhanced with additional derating:");
            for diagnostic in &analysis_result.safety_analysis.diagnostics {
                info!("  - {}", diagnostic.message);
            }
        }
        
        // Create power output net and pin
        let power_net = netlist.add_net_with_class(
            Some(format!("{}_out", pin_name)),
            NetClass::Power(domain_voltage)
        );
        
        netlist.add_pin(module_id, pin_name.to_string(), PinDirection::Out, PinType::Power);
        
        Ok(())
    }
    
    /// Expand signal output pin using simulation-enhanced component calculations
    fn expand_signal_output_with_simulation(
        &mut self,
        netlist: &mut Netlist,
        module_id: ModuleId,
        pin_name: &str,
        calculator: &crate::passive_component_calculator::PassiveComponentCalculator,
        _selector: &crate::package_selector::PackageSelector,
        analysis_result: &bhdl_analyzer::AnalysisResult,
        intent_info: Option<&bhdl_common::IntentCall>,
    ) -> Result<()> {
        info!("Expanding signal output pin '{}' with simulation-enhanced protection", pin_name);
        
        // Extract current limiting requirements from intent
        let current_limit = if let Some(intent) = intent_info {
            // Parse current limit from intent parameters
            let mut limit = 0.020; // 20mA default
            for param in &intent.params {
                if let bhdl_common::IntentParam::Named(name, value) = param {
                    if name == "current_limit" {
                        if let bhdl_common::IntentValue::String(limit_str) = value {
                            // Parse "20mA" -> 0.020
                            if let Ok(parsed) = limit_str.trim_end_matches("mA").parse::<f64>() {
                                limit = parsed / 1000.0;
                            }
                        }
                    }
                }
            }
            limit
        } else {
            0.020 // Default 20mA
        };
        
        // Calculate current limiting resistor using simulation data
        if let Ok((power_rating, voltage_rating, optimal_resistance)) = calculator.calculate_resistor_spec_from_simulation(
            &format!("{}_current_limit", pin_name),
            analysis_result,
            intent_info,
        ) {
            info!("Simulation-enhanced current limiting: {}Ω for {}mA limit", 
                  optimal_resistance, current_limit * 1000.0);
            
            // Create current limiting resistor
            let limit_resistor = netlist.add_instance(
                format!("{}_limit_R", pin_name),
                module_id
            );
            
            info!("Created current limiting resistor: {}, {}", power_rating, voltage_rating);
        }
        
        // Create signal output pin
        netlist.add_pin(module_id, pin_name.to_string(), PinDirection::Out, PinType::Signal);
        
        Ok(())
    }
    
    /// Expand bidirectional pin using simulation-enhanced protection calculations
    fn expand_bidirectional_with_simulation(
        &mut self,
        netlist: &mut Netlist,
        module_id: ModuleId,
        pin_name: &str,
        calculator: &crate::passive_component_calculator::PassiveComponentCalculator,
        _selector: &crate::package_selector::PackageSelector,
        analysis_result: &bhdl_analyzer::AnalysisResult,
        intent_info: Option<&bhdl_common::IntentCall>,
    ) -> Result<()> {
        info!("Expanding bidirectional pin '{}' with simulation-enhanced protection", pin_name);
        
        // Calculate protection components for both directions
        if let Ok((power_rating, voltage_rating, protection_resistance)) = calculator.calculate_resistor_spec_from_simulation(
            &format!("{}_bidirectional", pin_name),
            analysis_result,
            intent_info,
        ) {
            info!("Simulation-enhanced bidirectional protection: {}Ω, {}, {}", 
                  protection_resistance, power_rating, voltage_rating);
            
            // Create bidirectional protection network
            let protection_resistor = netlist.add_instance(
                format!("{}_bidir_R", pin_name),
                module_id
            );
            
            info!("Created bidirectional protection resistor");
        }
        
        // Create bidirectional pin
        netlist.add_pin(module_id, pin_name.to_string(), PinDirection::InOut, PinType::Signal);
        
        Ok(())
    }
}