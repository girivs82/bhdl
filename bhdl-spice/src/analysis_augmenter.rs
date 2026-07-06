//! Augment netlist with SPICE analysis data
//! 
//! This module adds SPICE-specific analysis data to the unified AnalysisData structure,
//! avoiding the need for complex netlist-to-circuit conversions.

use std::collections::HashMap;
use anyhow::{Result, Context};
use log::{debug, info, warn};

use bhdl_common::{AnalysisData, InstanceAnalysisData, ElectricalParams};
use bhdl_netlist::{Netlist, InstanceId, ModuleKind};
use crate::{
    ComponentModelExtractor, ExtractedModel,
    component_registry::ComponentRegistry,
    model_extractor::ModelSource,
    NetlistToSpiceConverter,
    ComponentRoleDetector,
};

/// Augments netlist and analysis data with SPICE-specific information
pub struct SpiceAnalysisAugmenter {
    model_extractor: ComponentModelExtractor,
    component_registry: ComponentRegistry,
}

impl SpiceAnalysisAugmenter {
    pub fn new() -> Self {
        Self {
            model_extractor: ComponentModelExtractor::new(),
            component_registry: ComponentRegistry::new(),
        }
    }
    
    /// Augment the analysis data with SPICE-specific information
    pub fn augment(
        &mut self,
        netlist: &Netlist,
        analysis_data: &mut AnalysisData,
    ) -> Result<()> {
        info!("Augmenting {} instances with SPICE analysis data", netlist.instances.len());
        
        // Process each instance
        for (instance_id, instance) in &netlist.instances {
            let module = netlist.modules.get(instance.definition)
                .ok_or_else(|| anyhow::anyhow!("Module not found for instance {}", instance.name))?;
            
            // Skip non-physical components
            match module.kind {
                ModuleKind::PhysicalComponent | ModuleKind::Component => {},
                ModuleKind::Interface if is_physical_interface(&module.name) => {},
                _ => {
                    debug!("Skipping non-physical module: {}", instance.name);
                    continue;
                }
            }
            
            // Get or create instance analysis data
            let instance_analysis = analysis_data.instance_analysis
                .entry(instance.name.clone())
                .or_insert_with(InstanceAnalysisData::default);
            
            // Determine SPICE component type. `component_class` lives on the
            // entity definition; instance attributes override it if present.
            let mut merged_attributes = module.attributes.clone();
            merged_attributes.extend(instance.attributes.clone());
            let spice_type = self.determine_spice_type(&module.name, &merged_attributes)?;
            instance_analysis.spice_type = Some(spice_type.clone());
            
            // Extract electrical parameters
            if let Ok(extracted_model) = self.extract_model_for_instance(
                &instance.name,
                module,
                &instance.attributes,
            ) {
                instance_analysis.electrical_params = Some(convert_to_electrical_params(&extracted_model));
            }
            
            debug!("Augmented instance {} with SPICE type: {}", instance.name, spice_type);
        }
        
        // Step 2: Run component role detection
        self.detect_component_roles(netlist, analysis_data)?;
        
        info!("Successfully augmented {} instances", analysis_data.instance_analysis.len());
        Ok(())
    }
    
    /// Detect component roles using topology analysis
    fn detect_component_roles(
        &mut self,
        netlist: &Netlist,
        analysis_data: &mut AnalysisData,
    ) -> Result<()> {
        info!("Running component role detection...");
        
        // Convert netlist to SPICE circuit
        let mut converter = NetlistToSpiceConverter::new();
        let circuit = converter.convert(netlist)?;
        
        // Build instance to component mapping
        let mut instance_mapping = HashMap::new();
        let mut component_to_instance = HashMap::new();
        
        // Map instances to SPICE components
        for (comp_id, component) in circuit.branches() {
            // Find corresponding instance by name
            for (inst_id, instance) in &netlist.instances {
                if instance.name == component.name() {
                    instance_mapping.insert(inst_id, comp_id);
                    component_to_instance.insert(comp_id, instance.name.clone());
                    break;
                }
            }
        }
        
        // Create role detector with AST metadata from unified model
        let detector = ComponentRoleDetector::with_ast_metadata(
            circuit,
            netlist,
            instance_mapping,
        );
        
        // Detect all component roles
        let roles = detector.detect_all_roles();
        
        // Update instance analysis data with detected roles
        for (comp_id, role) in roles {
            if let Some(instance_name) = component_to_instance.get(&comp_id) {
                if let Some(instance_analysis) = analysis_data.instance_analysis.get_mut(instance_name) {
                    instance_analysis.component_role = Some(format!("{:?}", role));
                    debug!("Instance {} detected role: {:?}", instance_name, role);
                }
            }
        }
        
        info!("Component role detection complete");
        Ok(())
    }
    
    fn determine_spice_type(
        &self,
        module_name: &str,
        attributes: &HashMap<String, String>,
    ) -> Result<String> {
        // Use component registry to determine type
        if let Some(component_type) = self.component_registry.get_component_type(module_name, attributes) {
            // Convert ComponentType enum to string
            let type_str = match component_type {
                crate::components::ComponentType::Resistor => "resistor",
                crate::components::ComponentType::Capacitor => "capacitor",
                crate::components::ComponentType::Inductor => "inductor",
                crate::components::ComponentType::Diode => "diode",
                crate::components::ComponentType::LED => "led",
                crate::components::ComponentType::BJT => "bjt",
                crate::components::ComponentType::MOSFET => "mosfet",
                crate::components::ComponentType::VoltageSource => "voltage_source",
                crate::components::ComponentType::CurrentSource => "current_source",
                crate::components::ComponentType::OpAmp => "opamp",
                crate::components::ComponentType::VoltageRegulator => "voltage_regulator",
                crate::components::ComponentType::Triode => "triode",
                crate::components::ComponentType::Other(ref s) => s,
            };
            return Ok(type_str.to_string());
        }
        
        // If not found in registry, return unknown
        warn!("Component type not found for module '{}', using 'unknown'", module_name);
        Ok("unknown".to_string())
    }
    
    fn extract_model_for_instance(
        &mut self,
        instance_name: &str,
        module: &bhdl_netlist::ModuleDefinition,
        attributes: &HashMap<String, String>,
    ) -> Result<ExtractedModel> {
        // Build data map from attributes
        let mut data = HashMap::new();
        data.insert("name".to_string(), instance_name.to_string());
        data.insert("type".to_string(), module.name.clone());
        
        // Add instance attributes
        for (key, value) in attributes {
            data.insert(key.clone(), value.clone());
        }
        
        // Add module attributes
        for (key, value) in &module.attributes {
            data.insert(key.clone(), value.clone());
        }
        
        // Use model extractor to properly extract the model
        self.model_extractor.extract_from_data(data)
    }
}

fn is_physical_interface(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("test") || 
    lower.contains("point") ||
    lower.contains("connector") ||
    lower.contains("pin") ||
    lower.contains("header")
}

fn convert_to_electrical_params(model: &ExtractedModel) -> ElectricalParams {
    let mut params = ElectricalParams {
        value: None,
        tolerance: None,
        power_rating: None,
        voltage_rating: None,
        current_rating: None,
        extra: HashMap::new(),
    };
    
    // Extract common electrical parameters from model parameters
    if let Some(&value) = model.parameters.get("value") {
        params.value = Some(value);
    }
    if let Some(&tolerance) = model.parameters.get("tolerance") {
        params.tolerance = Some(tolerance);
    }
    if let Some(&power) = model.parameters.get("power_rating") {
        params.power_rating = Some(power);
    }
    if let Some(&voltage) = model.parameters.get("voltage_rating") {
        params.voltage_rating = Some(voltage);
    }
    if let Some(&current) = model.parameters.get("current_rating") {
        params.current_rating = Some(current);
    }
    
    // Add all other parameters to extra
    for (key, &value) in &model.parameters {
        if !["value", "tolerance", "power_rating", "voltage_rating", "current_rating"].contains(&key.as_str()) {
            params.extra.insert(key.clone(), value);
        }
    }
    
    params
}