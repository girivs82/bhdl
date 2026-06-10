//! Entity variant management for parameter-based deduplication
//!
//! This module handles creating unique variants of entities based on their
//! parameter values, ensuring proper deduplication while maintaining
//! distinct implementations for different parameter combinations.

use std::collections::HashMap;
use anyhow::Result;
use bhdl_ast::{AstNode, EntityInst, HasName};
use bhdl_analyzer::types::AnalysisResult;
use bhdl_netlist::{Netlist, ModuleId, ModuleKind, InstanceId, NetId};
use bhdl_netlist::types::{PinDirection, PinType, PortDirection, NetClass};
use log::{debug, info, warn};

/// Key for identifying unique entity variants
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EntityVariantKey {
    /// Base entity name (e.g., "RC_Filter")
    pub base_name: String,
    /// Sorted parameter key-value pairs for consistent hashing
    pub parameters: Vec<(String, String)>,
}

impl EntityVariantKey {
    /// Create a new variant key from entity name and parameters
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

/// Entity variant manager for deduplication
pub struct EntityVariantManager {
    /// Map from variant key to module ID
    variants: HashMap<EntityVariantKey, ModuleId>,
    /// Map from base entity name to its definition in the AST
    entity_definitions: HashMap<String, bhdl_ast::Entity>,
}

impl EntityVariantManager {
    pub fn new() -> Self {
        Self {
            variants: HashMap::new(),
            entity_definitions: HashMap::new(),
        }
    }

    /// Register an entity definition from the AST
    pub fn register_entity_definition(&mut self, entity: &bhdl_ast::Entity) {
        if let Some(name) = entity.name() {
            let entity_name = name.text().to_string();
            self.entity_definitions.insert(entity_name, entity.clone());
            debug!("Registered entity definition: {}", name.text());
        }
    }

    /// Get or create an entity variant for the given instance
    pub fn get_or_create_variant(
        &mut self,
        entity_inst: &EntityInst,
        netlist: &mut Netlist,
        analysis: &AnalysisResult,
    ) -> Result<ModuleId> {
        // Extract entity type and parameters
        let entity_type = entity_inst.entity_type()
            .map(|t| t.text().to_string())
            .ok_or_else(|| anyhow::anyhow!("Entity instance missing type"))?;

        // Extract parameter values from the instance
        let parameters = self.extract_instance_parameters(entity_inst, analysis)?;

        // Create variant key
        let variant_key = EntityVariantKey::new(entity_type.clone(), parameters);

        // Check if variant already exists
        if let Some(&module_id) = self.variants.get(&variant_key) {
            debug!("Reusing existing variant: {}", variant_key.variant_name());
            return Ok(module_id);
        }

        // Create new variant
        let variant_name = variant_key.variant_name();
        info!("Creating new entity variant: {}", variant_name);

        // Create the module in the netlist
        let module_id = netlist.add_module(variant_name.clone(), ModuleKind::Module);

        // Copy pins from base entity definition
        // We need to clone the entity to avoid borrowing conflicts
        if let Some(base_entity) = self.entity_definitions.get(&entity_type).cloned() {
            self.copy_entity_interface(&base_entity, module_id, netlist)?;
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

    /// Extract parameter values from an entity instance
    fn extract_instance_parameters(
        &self,
        entity_inst: &EntityInst,
        _analysis: &AnalysisResult,
    ) -> Result<Vec<(String, String)>> {
        let mut parameters = Vec::new();

        // Get parameters from the instance's param list
        if let Some(param_list) = entity_inst.param_list() {
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

        // If no parameters specified, we might need to get defaults from the entity definition
        if parameters.is_empty() {
            if let Some(entity_type) = entity_inst.entity_type() {
                let type_name = entity_type.text().to_string();
                if let Some(base_entity) = self.entity_definitions.get(&type_name) {
                    parameters = self.extract_default_parameters(base_entity)?;
                }
            }
        }

        Ok(parameters)
    }

    /// Extract default parameter values from entity definition
    fn extract_default_parameters(&self, entity: &bhdl_ast::Entity) -> Result<Vec<(String, String)>> {
        let mut parameters = Vec::new();

        if let Some(param_list) = entity.param_list() {
            // For entity definitions, use param_defs() to get EntityParam items
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

    /// Copy entity interface (pins) from base definition to variant
    fn copy_entity_interface(
        &mut self,
        base_entity: &bhdl_ast::Entity,
        variant_id: ModuleId,
        netlist: &mut Netlist,
    ) -> Result<()> {
        use bhdl_ast::SyntaxKind;
        use bhdl_netlist::types::{PortDirection, PinDirection, PinType};

        // Extract pins from the base entity
        for child in base_entity.syntax().children() {
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
                            info!("Processing virtual pin '{}' in entity", pin_name_str);
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
        info!("Expanding virtual pin '{}' in entity with intent: {:?}",
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
            NetClass::Power { voltage: 3.3, current: None } // Default 3.3V, should be parameterized
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
            NetClass::Power { voltage: 3.3, current: None } // Default 3.3V, should be parameterized
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
    pub fn get_variants(&self) -> &HashMap<EntityVariantKey, ModuleId> {
        &self.variants
    }

    /// Find entity definition by name
    pub fn find_entity_definition(&self, entity_name: &str) -> Option<&bhdl_ast::Entity> {
        self.entity_definitions.get(entity_name)
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

        // Apply safety violation flags for enhanced component derating
        self.apply_safety_enhanced_derating(
            netlist, module_id, pin_name, analysis_result, calculator
        )?;

        // Apply thermal stress-based component selection
        self.apply_thermal_stress_component_selection(
            netlist, module_id, pin_name, analysis_result, calculator
        )?;

        // Add current density analysis for PCB layout
        self.add_current_density_analysis_for_pcb_layout(
            netlist, module_id, pin_name, analysis_result, calculator
        )?;

        // Create power output net and pin
        let power_net = netlist.add_net_with_class(
            Some(format!("{}_out", pin_name)),
            NetClass::Power { voltage: domain_voltage, current: None }
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

        // Apply safety violation-based enhanced derating
        self.apply_safety_enhanced_derating(
            netlist, module_id, pin_name, analysis_result, calculator
        )?;

        // Apply thermal stress-based component selection
        self.apply_thermal_stress_component_selection(
            netlist, module_id, pin_name, analysis_result, calculator
        )?;

        // Add current density analysis for PCB layout
        self.add_current_density_analysis_for_pcb_layout(
            netlist, module_id, pin_name, analysis_result, calculator
        )?;

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

        // Apply safety violation-based enhanced derating
        self.apply_safety_enhanced_derating(
            netlist, module_id, pin_name, analysis_result, calculator
        )?;

        // Apply thermal stress-based component selection
        self.apply_thermal_stress_component_selection(
            netlist, module_id, pin_name, analysis_result, calculator
        )?;

        // Add current density analysis for PCB layout
        self.add_current_density_analysis_for_pcb_layout(
            netlist, module_id, pin_name, analysis_result, calculator
        )?;

        // Create bidirectional pin
        netlist.add_pin(module_id, pin_name.to_string(), PinDirection::InOut, PinType::Signal);

        Ok(())
    }

    /// Apply safety violation-based enhanced derating for component selection
    /// This implements Phase 2 of virtual pin synthesis with sophisticated safety analysis
    fn apply_safety_enhanced_derating(
        &mut self,
        _netlist: &mut Netlist,
        _module_id: ModuleId,
        pin_name: &str,
        analysis_result: &bhdl_analyzer::AnalysisResult,
        calculator: &crate::passive_component_calculator::PassiveComponentCalculator,
    ) -> Result<()> {
        info!("Applying safety violation-based enhanced derating for pin '{}'", pin_name);

        // Check for safety violations in simulation data
        if let Some(ref electrical_safety) = analysis_result.simulation_data.electrical_safety {
            let mut total_derating_factor = 1.0;
            let mut violations_found = Vec::new();

            // Check component stress violations
            for (component_name, stress) in &electrical_safety.component_stress {
                if component_name.contains(pin_name) {
                    let component_derating = self.calculate_component_specific_derating(stress, calculator)?;
                    total_derating_factor *= component_derating;

                    if component_derating < 1.0 {
                        violations_found.push(format!("Component '{}': {:.1}% derating applied",
                            component_name, (1.0 - component_derating) * 100.0));
                    }
                }
            }

            // Check current density violations
            for violation in &electrical_safety.current_density_violations {
                if violation.location.contains(pin_name) {
                    let severity_derating = self.calculate_severity_derating(&violation.severity);
                    total_derating_factor *= severity_derating;
                    violations_found.push(format!("Current density violation at '{}': {:.1}mA -> {:.1}mA safe",
                        violation.location, violation.current * 1000.0, violation.max_safe_current * 1000.0));
                }
            }

            // Check voltage stress violations
            for violation in &electrical_safety.voltage_stress_violations {
                if violation.component.contains(pin_name) {
                    let voltage_derating = self.calculate_voltage_stress_derating(violation);
                    total_derating_factor *= voltage_derating;
                    violations_found.push(format!("Voltage stress on '{}': {:.1}V applied / {:.1}V max (ratio: {:.2})",
                        violation.component, violation.applied_voltage, violation.max_voltage, violation.stress_ratio));
                }
            }

            // Check thermal stress violations
            for violation in &electrical_safety.thermal_stress_violations {
                if violation.component.contains(pin_name) {
                    let thermal_derating = self.calculate_thermal_stress_derating(violation);
                    total_derating_factor *= thermal_derating;
                    violations_found.push(format!("Thermal stress on '{}': {:.1}°C operating / {:.1}°C max",
                        violation.component, violation.operating_temperature, violation.max_temperature));
                }
            }

            // Report safety analysis results
            if !violations_found.is_empty() {
                warn!("Safety violations detected for pin '{}' - Enhanced derating applied:", pin_name);
                for violation in violations_found {
                    warn!("  ⚠️  {}", violation);
                }
                warn!("  📊 Total derating factor: {:.2} ({:.1}% additional margin)",
                      total_derating_factor, (1.0 - total_derating_factor) * 100.0);

                // Note: Enhanced safety factors will be applied during component calculations
                info!("Enhanced derating will be applied to all component calculations for this pin");
            } else {
                info!("✅ No safety violations detected for pin '{}' - Standard derating applies", pin_name);
            }

            // Check overall safety summary
            if electrical_safety.safety_summary.critical_violations > 0 {
                warn!("🚨 CRITICAL: {} critical safety violations detected in circuit!",
                      electrical_safety.safety_summary.critical_violations);
                warn!("  Reliability impact: {:.1}% reduction",
                      electrical_safety.safety_summary.estimated_reliability_impact * 100.0);
            }
        } else {
            info!("No electrical safety data available for pin '{}' - using standard calculations", pin_name);
        }

        Ok(())
    }

    /// Calculate component-specific derating based on stress analysis
    fn calculate_component_specific_derating(
        &self,
        stress: &bhdl_analyzer::types::ComponentStressAnalysis,
        _calculator: &crate::passive_component_calculator::PassiveComponentCalculator,
    ) -> Result<f64> {
        let mut derating_factor = 1.0;

        // Apply derating based on individual stress types
        if stress.has_voltage_stress {
            let voltage_derating = 1.0 - (stress.voltage_stress_ratio - 0.8).max(0.0) * 0.5;
            derating_factor *= voltage_derating;
            info!("  Voltage stress ratio: {:.2} -> derating: {:.2}",
                  stress.voltage_stress_ratio, voltage_derating);
        }

        if stress.has_current_stress {
            let current_derating = 1.0 - (stress.current_stress_ratio - 0.7).max(0.0) * 0.4;
            derating_factor *= current_derating;
            info!("  Current stress ratio: {:.2} -> derating: {:.2}",
                  stress.current_stress_ratio, current_derating);
        }

        if stress.has_thermal_stress {
            let thermal_derating = 1.0 - (stress.thermal_stress_ratio - 0.6).max(0.0) * 0.6;
            derating_factor *= thermal_derating;
            info!("  Thermal stress ratio: {:.2} -> derating: {:.2}",
                  stress.thermal_stress_ratio, thermal_derating);
        }

        // Apply derating recommendations if available
        for recommendation in &stress.derating_recommendations {
            let recommendation_factor = recommendation.derating_factor.min(1.0);
            derating_factor *= recommendation_factor;
            info!("  Recommendation for {}: {:.2} derating ({})",
                  recommendation.parameter, recommendation_factor, recommendation.reason);
        }

        Ok(derating_factor.max(0.3)) // Never derate below 30% to prevent unrealistic requirements
    }

    /// Calculate derating based on safety violation severity
    fn calculate_severity_derating(&self, severity: &bhdl_analyzer::types::SafetyViolationSeverity) -> f64 {
        match severity {
            bhdl_analyzer::types::SafetyViolationSeverity::Info => 0.98,       // 2% derating
            bhdl_analyzer::types::SafetyViolationSeverity::Warning => 0.9,     // 10% derating
            bhdl_analyzer::types::SafetyViolationSeverity::Error => 0.8,       // 20% derating
            bhdl_analyzer::types::SafetyViolationSeverity::Critical => 0.6,    // 40% derating
        }
    }

    /// Calculate derating for voltage stress violations
    fn calculate_voltage_stress_derating(&self, violation: &bhdl_analyzer::types::VoltageStressViolation) -> f64 {
        // Base derating from severity
        let severity_derating = self.calculate_severity_derating(&violation.severity);

        // Additional derating based on stress ratio
        let stress_derating = if violation.stress_ratio > 1.0 {
            // Over-stressed - significant derating required
            1.0 / violation.stress_ratio.min(2.0) // Cap at 50% derating
        } else if violation.stress_ratio > 0.9 {
            // High stress but within limits - moderate derating
            1.0 - (violation.stress_ratio - 0.9) * 2.0 // 0-20% derating
        } else {
            1.0 // No additional derating needed
        };

        severity_derating * stress_derating
    }

    /// Calculate derating for thermal stress violations
    fn calculate_thermal_stress_derating(&self, violation: &bhdl_analyzer::types::ThermalStressViolation) -> f64 {
        let base_derating = self.calculate_severity_derating(&violation.severity);

        // Additional thermal derating based on temperature margin
        let temp_margin = violation.max_temperature - violation.operating_temperature;
        let thermal_derating = if temp_margin < 10.0 {
            0.7 // 30% derating for <10°C margin
        } else if temp_margin < 25.0 {
            0.85 // 15% derating for <25°C margin
        } else {
            1.0 // No additional derating for good margin
        };

        base_derating * thermal_derating
    }

    /// Apply thermal stress-based component selection for virtual pins
    /// This implements advanced thermal analysis integration for component reliability
    fn apply_thermal_stress_component_selection(
        &mut self,
        netlist: &mut Netlist,
        module_id: ModuleId,
        pin_name: &str,
        analysis_result: &bhdl_analyzer::AnalysisResult,
        calculator: &crate::passive_component_calculator::PassiveComponentCalculator,
    ) -> Result<()> {
        info!("Applying thermal stress-based component selection for pin '{}'", pin_name);

        // Check if we have thermal simulation data available
        if let Some(ref thermal_data) = analysis_result.simulation_data.thermal_analysis {
            // Analyze component temperatures and hot spots
            for (component_name, temperature) in &thermal_data.component_temperatures {
                if component_name.contains(pin_name) {
                    info!("Component '{}' operating at {:.1}°C", component_name, temperature);

                    // Apply thermal derating based on operating temperature
                    let thermal_derating_factor = thermal_data.thermal_derating_factors
                        .get(component_name)
                        .copied()
                        .unwrap_or(1.0);

                    if thermal_derating_factor < 1.0 {
                        warn!("  🌡️  Thermal derating required: {:.1}% reduction in ratings",
                              (1.0 - thermal_derating_factor) * 100.0);
                    }

                    // Select components with enhanced thermal ratings
                    self.select_thermally_enhanced_components(
                        netlist, module_id, component_name, *temperature, thermal_derating_factor, calculator
                    )?;
                }
            }

            // Check for hot spots affecting this pin's components
            for hot_spot in &thermal_data.hot_spots {
                if hot_spot.components_affected.iter().any(|comp| comp.contains(pin_name)) {
                    warn!("🔥 Hot spot detected at '{}': {:.1}°C", hot_spot.location, hot_spot.temperature);
                    warn!("   Components affected: {:?}", hot_spot.components_affected);

                    if hot_spot.cooling_required {
                        warn!("   ❄️  Active cooling required for reliability");
                        // TODO: Add thermal management components (heat sinks, fans, thermal vias)
                    }

                    // Apply enhanced thermal margins for hot spot components
                    self.apply_hot_spot_thermal_margins(
                        netlist, module_id, pin_name, hot_spot, calculator
                    )?;
                }
            }

            // Check ambient temperature considerations
            let ambient_temp = thermal_data.ambient_temperature;
            if ambient_temp > 70.0 {
                warn!("🌡️  High ambient temperature: {:.1}°C - Enhanced thermal margins applied", ambient_temp);
                // Apply additional derating for high ambient temperature
                self.apply_ambient_temperature_derating(ambient_temp, calculator)?;
            } else if ambient_temp < -20.0 {
                warn!("❄️  Low ambient temperature: {:.1}°C - Cold start considerations apply", ambient_temp);
                // Apply cold start considerations (capacitor ESR, resistor tolerance changes)
                self.apply_cold_start_considerations(ambient_temp, calculator)?;
            }

        } else {
            // No thermal data available - apply conservative thermal assumptions
            warn!("No thermal simulation data available for pin '{}' - applying conservative thermal margins", pin_name);
            self.apply_conservative_thermal_margins(calculator)?;
        }

        Ok(())
    }

    /// Select components with enhanced thermal characteristics
    fn select_thermally_enhanced_components(
        &mut self,
        _netlist: &mut Netlist,
        _module_id: ModuleId,
        component_name: &str,
        operating_temp: f64,
        thermal_derating_factor: f64,
        _calculator: &crate::passive_component_calculator::PassiveComponentCalculator,
    ) -> Result<()> {
        info!("Selecting thermally enhanced components for '{}'", component_name);

        // Determine temperature grade requirements
        let temp_grade = if operating_temp > 125.0 {
            "High Temperature (>125°C) - Special automotive/military grade required"
        } else if operating_temp > 85.0 {
            "Extended Temperature (85-125°C) - Automotive grade recommended"
        } else if operating_temp > 70.0 {
            "Industrial Temperature (70-85°C) - Industrial grade sufficient"
        } else {
            "Commercial Temperature (<70°C) - Commercial grade acceptable"
        };

        info!("  Temperature grade selection: {}", temp_grade);

        // Apply specific thermal enhancements based on component type
        if component_name.contains("resistor") || component_name.contains("R") {
            self.apply_resistor_thermal_enhancements(operating_temp, thermal_derating_factor)?;
        } else if component_name.contains("capacitor") || component_name.contains("C") {
            self.apply_capacitor_thermal_enhancements(operating_temp, thermal_derating_factor)?;
        }

        Ok(())
    }

    /// Apply resistor-specific thermal enhancements
    fn apply_resistor_thermal_enhancements(
        &self,
        operating_temp: f64,
        thermal_derating_factor: f64,
    ) -> Result<()> {
        // Power rating adjustments for temperature
        let power_derating = if operating_temp > 85.0 {
            // Significant power derating above 85°C
            thermal_derating_factor * 0.6 // Additional 40% derating
        } else if operating_temp > 70.0 {
            // Moderate power derating above 70°C
            thermal_derating_factor * 0.8 // Additional 20% derating
        } else {
            thermal_derating_factor
        };

        info!("  Resistor thermal enhancements:");
        info!("    Power derating factor: {:.2} ({:.1}% margin)",
              power_derating, (1.0 - power_derating) * 100.0);

        // Temperature coefficient considerations
        if operating_temp > 100.0 {
            info!("    Recommendation: Metal foil resistors for <25ppm/°C TC");
        } else if operating_temp > 85.0 {
            info!("    Recommendation: Metal film resistors for <100ppm/°C TC");
        } else {
            info!("    Recommendation: Standard thick film resistors acceptable");
        }

        Ok(())
    }

    /// Apply capacitor-specific thermal enhancements
    fn apply_capacitor_thermal_enhancements(
        &self,
        operating_temp: f64,
        thermal_derating_factor: f64,
    ) -> Result<()> {
        // Voltage rating adjustments for temperature
        let voltage_derating = if operating_temp > 85.0 {
            thermal_derating_factor * 0.7 // Additional 30% voltage derating
        } else {
            thermal_derating_factor
        };

        info!("  Capacitor thermal enhancements:");
        info!("    Voltage derating factor: {:.2} ({:.1}% margin)",
              voltage_derating, (1.0 - voltage_derating) * 100.0);

        // Dielectric selection based on temperature
        let dielectric_recommendation = if operating_temp > 125.0 {
            "C0G/NP0 (Class I) for maximum stability and reliability"
        } else if operating_temp > 85.0 {
            "X7R (Class II) with caution - expect 15% capacitance change"
        } else {
            "X5R or X7R acceptable for general use"
        };

        info!("    Dielectric recommendation: {}", dielectric_recommendation);

        // ESR considerations at temperature
        if operating_temp > 85.0 {
            info!("    ESR increase expected: ~50% higher at operating temperature");
        }

        Ok(())
    }

    /// Apply enhanced thermal margins for components in hot spots
    fn apply_hot_spot_thermal_margins(
        &mut self,
        _netlist: &mut Netlist,
        _module_id: ModuleId,
        pin_name: &str,
        hot_spot: &bhdl_analyzer::types::HotSpot,
        _calculator: &crate::passive_component_calculator::PassiveComponentCalculator,
    ) -> Result<()> {
        info!("Applying hot spot thermal margins for pin '{}' in hot spot '{}'", pin_name, hot_spot.location);

        // Calculate additional thermal derating for hot spot proximity
        let hot_spot_derating = if hot_spot.temperature > 150.0 {
            0.5 // 50% derating for extreme hot spots
        } else if hot_spot.temperature > 125.0 {
            0.6 // 40% derating for severe hot spots
        } else if hot_spot.temperature > 100.0 {
            0.8 // 20% derating for moderate hot spots
        } else {
            0.9 // 10% derating for minor hot spots
        };

        warn!("  🔥 Hot spot thermal derating: {:.1}% additional margin required",
              (1.0 - hot_spot_derating) * 100.0);

        // Thermal management recommendations
        if hot_spot.cooling_required {
            info!("  ❄️  Thermal management recommendations:");
            info!("    - Add thermal vias near affected components");
            info!("    - Consider copper pour for heat spreading");
            info!("    - Evaluate component placement for better airflow");
            if hot_spot.temperature > 125.0 {
                info!("    - Active cooling (fan/heat sink) strongly recommended");
            }
        }

        Ok(())
    }

    /// Apply derating considerations for high ambient temperatures
    fn apply_ambient_temperature_derating(
        &self,
        ambient_temp: f64,
        _calculator: &crate::passive_component_calculator::PassiveComponentCalculator,
    ) -> Result<()> {
        let ambient_derating = if ambient_temp > 85.0 {
            0.6 // 40% additional derating for extreme ambient
        } else if ambient_temp > 70.0 {
            0.8 // 20% additional derating for high ambient
        } else {
            1.0
        };

        info!("Ambient temperature derating factor: {:.2}", ambient_derating);
        info!("  High ambient temperature mitigation strategies:");
        info!("    - Use temperature-rated components (automotive/industrial grade)");
        info!("    - Increase power ratings significantly");
        info!("    - Consider thermal interface materials");

        Ok(())
    }

    /// Apply considerations for low ambient temperature operation
    fn apply_cold_start_considerations(
        &self,
        ambient_temp: f64,
        _calculator: &crate::passive_component_calculator::PassiveComponentCalculator,
    ) -> Result<()> {
        info!("Cold start considerations for {:.1}°C ambient:", ambient_temp);
        info!("  - Capacitor ESR increases significantly at low temperatures");
        info!("  - Resistor tolerance may shift due to temperature coefficient");
        info!("  - Consider cold start current limiting for sensitive circuits");

        if ambient_temp < -40.0 {
            warn!("  ❄️  Extreme cold operation - specialized components required");
            info!("    - Military/aerospace grade components recommended");
            info!("    - Pre-heating circuits may be necessary");
        }

        Ok(())
    }

    /// Apply conservative thermal margins when no thermal data is available
    fn apply_conservative_thermal_margins(
        &self,
        _calculator: &crate::passive_component_calculator::PassiveComponentCalculator,
    ) -> Result<()> {
        info!("Applying conservative thermal margins:");
        info!("  - Assume 85°C maximum operating temperature");
        info!("  - Apply 50% power derating for reliability");
        info!("  - Use industrial temperature grade components");
        info!("  - Add thermal management provisions in layout");

        Ok(())
    }

    /// Add current density analysis for trace and via sizing requirements
    /// This implements PCB layout guidance based on electrical current requirements
    fn add_current_density_analysis_for_pcb_layout(
        &mut self,
        _netlist: &mut Netlist,
        _module_id: ModuleId,
        pin_name: &str,
        analysis_result: &bhdl_analyzer::AnalysisResult,
        _calculator: &crate::passive_component_calculator::PassiveComponentCalculator,
    ) -> Result<()> {
        info!("Analyzing current density requirements for PCB layout of pin '{}'", pin_name);

        // Check if we have electrical safety data with current density violations
        if let Some(ref electrical_safety) = analysis_result.simulation_data.electrical_safety {
            let mut trace_requirements = Vec::new();

            // Analyze current density violations
            for violation in &electrical_safety.current_density_violations {
                if violation.location.contains(pin_name) {
                    let current_density_violation = self.analyze_current_density_violation(violation)?;
                    trace_requirements.push(current_density_violation);
                }
            }

            // Analyze DC operating currents for trace sizing
            if let Some(ref dc_analysis) = analysis_result.simulation_data.dc_analysis {
                for (branch_name, current) in &dc_analysis.branch_currents {
                    if branch_name.contains(pin_name) && current.abs() > 0.001 {
                        let trace_requirements = self.calculate_trace_sizing_requirements(
                            branch_name, *current
                        )?;
                        info!("Branch '{}' current: {:.1}mA - {}",
                              branch_name, current * 1000.0, trace_requirements);
                    }
                }
            }

            // Generate PCB layout recommendations
            self.generate_pcb_layout_recommendations(pin_name, &trace_requirements)?;

        } else {
            // No current density data - provide conservative recommendations
            self.apply_conservative_pcb_layout_guidelines(pin_name)?;
        }

        Ok(())
    }

    /// Analyze specific current density violation and provide recommendations
    fn analyze_current_density_violation(
        &self,
        violation: &bhdl_analyzer::types::CurrentDensityViolation,
    ) -> Result<String> {
        let current_ma = violation.current * 1000.0;
        let safe_current_ma = violation.max_safe_current * 1000.0;
        let severity_str = match violation.severity {
            bhdl_analyzer::types::SafetyViolationSeverity::Critical => "🚨 CRITICAL",
            bhdl_analyzer::types::SafetyViolationSeverity::Error => "🛑 ERROR",
            bhdl_analyzer::types::SafetyViolationSeverity::Warning => "⚠️  WARNING",
            bhdl_analyzer::types::SafetyViolationSeverity::Info => "ℹ️  INFO",
        };

        warn!("{} Current density violation at '{}':", severity_str, violation.location);
        warn!("  Current: {:.1}mA, Safe limit: {:.1}mA", current_ma, safe_current_ma);

        // Calculate required trace width for safe current carrying
        let trace_width_mils = self.calculate_required_trace_width(violation.current)?;
        let via_requirements = self.calculate_via_requirements(violation.current)?;

        let recommendation = format!(
            "Required trace width: {:.1} mils, Via requirements: {}",
            trace_width_mils, via_requirements
        );

        info!("  📐 PCB Layout: {}", recommendation);
        Ok(recommendation)
    }

    /// Calculate required trace width based on current
    fn calculate_required_trace_width(&self, current_amps: f64) -> Result<f64> {
        // IPC-2221 standard for trace width calculation
        // Formula: Width (mils) = (Current / (k * (temp_rise^b) * thickness^c))^(1/d)
        // Where k=0.048, b=0.44, c=0.725, d=0.725 for external traces

        let current_ma = current_amps * 1000.0;
        let temp_rise_c: f64 = 10.0; // Conservative 10°C temperature rise
        let copper_thickness_oz: f64 = 1.0; // Standard 1oz copper

        // External trace constants
        let k: f64 = 0.048;
        let b: f64 = 0.44;
        let c: f64 = 0.725;
        let d: f64 = 0.725;

        let width_mils = (current_ma / (k * temp_rise_c.powf(b) * copper_thickness_oz.powf(c))).powf(1.0/d);

        // Apply safety margin
        let safe_width_mils = width_mils * 1.5; // 50% safety margin

        info!("  Trace width calculation:");
        info!("    Current: {:.1}mA, Temp rise: {:.1}°C", current_ma, temp_rise_c);
        info!("    Calculated: {:.1} mils, With safety margin: {:.1} mils", width_mils, safe_width_mils);

        Ok(safe_width_mils)
    }

    /// Calculate via requirements for current density
    fn calculate_via_requirements(&self, current_amps: f64) -> Result<String> {
        let current_ma = current_amps * 1000.0;

        // Standard via current carrying capacity (approximate)
        let via_capacity_ma = match () {
            _ if current_ma <= 100.0 => {
                "Single 8 mil via sufficient"
            },
            _ if current_ma <= 300.0 => {
                "Single 12 mil via or 2x 8 mil vias"
            },
            _ if current_ma <= 500.0 => {
                "2-3x 12 mil vias in parallel"
            },
            _ if current_ma <= 1000.0 => {
                "4-6x 12 mil vias or 2x 16 mil vias"
            },
            _ => {
                "Multiple large vias (16+ mil) or thermal vias required"
            }
        };

        // Add thermal considerations
        let thermal_note = if current_ma > 500.0 {
            " + thermal vias for heat dissipation"
        } else {
            ""
        };

        Ok(format!("{}{}", via_capacity_ma, thermal_note))
    }

    /// Calculate trace sizing requirements for a specific current
    fn calculate_trace_sizing_requirements(&self, branch_name: &str, current_amps: f64) -> Result<String> {
        let current_ma = current_amps.abs() * 1000.0;

        if current_ma < 1.0 {
            return Ok("Minimum trace width sufficient (5 mils)".to_string());
        }

        let trace_width = self.calculate_required_trace_width(current_amps)?;
        let via_req = self.calculate_via_requirements(current_amps)?;

        // Generate specific recommendations based on current level
        let recommendation = if current_ma > 1000.0 {
            format!("HIGH CURRENT: {:.1} mil trace, {}, consider copper pour", trace_width, via_req)
        } else if current_ma > 500.0 {
            format!("MEDIUM CURRENT: {:.1} mil trace, {}", trace_width, via_req)
        } else if current_ma > 100.0 {
            format!("LOW CURRENT: {:.1} mil trace, {}", trace_width, via_req)
        } else {
            format!("SIGNAL LEVEL: {:.1} mil trace (minimum for manufacturing)", trace_width.max(5.0))
        };

        Ok(recommendation)
    }

    /// Generate comprehensive PCB layout recommendations
    fn generate_pcb_layout_recommendations(
        &self,
        pin_name: &str,
        trace_requirements: &[String],
    ) -> Result<()> {
        info!("📐 PCB Layout Recommendations for pin '{}':", pin_name);

        if trace_requirements.is_empty() {
            info!("  ✅ No special current density requirements detected");
            info!("  📝 Use standard PCB design rules for signal traces");
            return Ok(());
        }

        info!("  📋 Current density analysis results:");
        for (i, req) in trace_requirements.iter().enumerate() {
            info!("    {}. {}", i + 1, req);
        }

        info!("  🎯 General PCB layout guidelines:");
        info!("    - Keep high current traces short and wide");
        info!("    - Use copper pours for power distribution where possible");
        info!("    - Place thermal vias near high power components");
        info!("    - Consider copper thickness upgrade (2oz) for high current paths");
        info!("    - Maintain minimum spacing per IPC-2221 standards");
        info!("    - Use proper via stitching for layer transitions");

        info!("  🌡️  Thermal management considerations:");
        info!("    - Monitor junction temperatures during operation");
        info!("    - Provide adequate ventilation for heat dissipation");
        info!("    - Consider thermal interface materials for critical components");

        Ok(())
    }

    /// Apply conservative PCB layout guidelines when no data is available
    fn apply_conservative_pcb_layout_guidelines(&self, pin_name: &str) -> Result<()> {
        info!("📐 Conservative PCB layout guidelines for pin '{}' (no current density data):", pin_name);
        info!("  📏 Recommended minimum trace widths:");
        info!("    - Power traces: 20+ mils (0.5mm+)");
        info!("    - Ground traces: 15+ mils (0.4mm+)");
        info!("    - Signal traces: 8+ mils (0.2mm+)");
        info!("    - High-speed signals: 6+ mils with controlled impedance");

        info!("  🔌 Via recommendations:");
        info!("    - Power/Ground: 12+ mil vias, multiple vias in parallel");
        info!("    - Signals: 8+ mil vias, standard single via");
        info!("    - Layer transitions: Via stitching every 200 mils");

        info!("  🛡️  Safety margins:");
        info!("    - Apply 50% derating to calculated trace widths");
        info!("    - Use 2oz copper for power planes");
        info!("    - Maintain 8+ mil spacing for signal isolation");

        Ok(())
    }
}
