//! Netlist to SPICE Circuit Converter
//! 
//! This module provides enhanced conversion from BHDL netlists to SPICE circuits,
//! leveraging the component model extraction system for accurate models.

use std::collections::HashMap;
use anyhow::{Result, Context};
use log::{debug, info, warn};

use crate::{
    Circuit, ComponentModelExtractor, ExtractedModel,
    model_factory::SpiceModelFactory,
    models::SpiceModel,
};
use bhdl_netlist::{
    Netlist, NetId, InstanceId,
    ConnectionPoint, ModuleKind, ModuleDefinition,
};

/// Enhanced netlist converter with proper SPICE model creation
pub struct NetlistToSpiceConverter {
    /// Component model extractor
    model_extractor: ComponentModelExtractor,
    /// Model factory for creating SPICE models
    model_factory: SpiceModelFactory,
    /// Cached models for instances
    instance_models: HashMap<InstanceId, Box<dyn SpiceModel>>,
    /// Symbol table data from analyzer (if available)
    symbol_table: HashMap<String, HashMap<String, String>>,
}

impl NetlistToSpiceConverter {
    /// Create new converter
    pub fn new() -> Self {
        Self {
            model_extractor: ComponentModelExtractor::new(),
            model_factory: SpiceModelFactory::new(),
            instance_models: HashMap::new(),
            symbol_table: HashMap::new(),
        }
    }
    
    /// Set symbol table data from analyzer
    pub fn set_symbol_table(&mut self, symbol_table: HashMap<String, HashMap<String, String>>) {
        self.symbol_table = symbol_table;
    }
    
    /// Convert BHDL netlist to SPICE circuit with proper models
    pub fn convert(&mut self, netlist: &Netlist) -> Result<Circuit> {
        let mut circuit = Circuit::new();
        
        info!("Converting netlist to SPICE circuit with {} instances", netlist.instances.len());
        
        // Step 1: Add all nets as nodes
        self.add_nets_as_nodes(&mut circuit, netlist)?;
        
        // Step 2: Process each instance and create proper SPICE models
        for (instance_id, instance) in &netlist.instances {
            match self.process_instance(&mut circuit, netlist, instance_id, instance) {
                Ok(_) => info!("Successfully processed instance: {}", instance.name),
                Err(e) => {
                    warn!("Failed to process instance {}: {}", instance.name, e);
                    // For debugging, let's see what's happening
                    eprintln!("ERROR processing {}: {}", instance.name, e);
                }
            }
        }
        
        info!("Created SPICE circuit with {} nodes and {} components", 
             circuit.nodes().count(), circuit.branches().count());
        
        Ok(circuit)
    }
    
    /// Add all nets as circuit nodes
    fn add_nets_as_nodes(&self, circuit: &mut Circuit, netlist: &Netlist) -> Result<()> {
        for (net_id, net) in &netlist.nets {
            let name = net.name.clone()
                .unwrap_or_else(|| format!("net_{:?}", net_id));
            circuit.add_node(name, Some(net_id));
        }
        Ok(())
    }
    
    /// Process a single instance and create SPICE model
    fn process_instance(
        &mut self,
        circuit: &mut Circuit,
        netlist: &Netlist,
        instance_id: InstanceId,
        instance: &bhdl_netlist::Instance,
    ) -> Result<()> {
        let module = netlist.modules.get(instance.definition)
            .ok_or_else(|| anyhow::anyhow!("Module not found for instance {}", instance.name))?;
        
        info!("Processing instance {} with module kind {:?}", 
              instance.name, module.kind);
        
        // Extract model based on available information
        let extracted_model = self.extract_model_for_instance(
            &instance.name,
            module,
            &instance.attributes,
        )?;
        
        // Create SPICE model
        let spice_model = self.model_extractor.create_spice_model(&extracted_model)?;
        
        // Get connected nets for this instance
        let connected_nets = self.get_connected_nets(netlist, instance_id)?;
        
        info!("Instance {} has {} connected nets", instance.name, connected_nets.len());
        
        // Handle different component types
        match module.kind {
            ModuleKind::PhysicalComponent => {
                self.add_physical_component(
                    circuit,
                    netlist,
                    &instance.name,
                    instance_id,
                    &connected_nets,
                    extracted_model,
                )?;
            }
            ModuleKind::Module | ModuleKind::Component => {
                // For now, skip logical modules
                debug!("Skipping logical module: {}", instance.name);
            }
            _ => {
                debug!("Skipping module kind {:?} for {}", module.kind, instance.name);
            }
        }
        
        // Cache the model
        self.instance_models.insert(instance_id, spice_model);
        
        Ok(())
    }
    
    /// Extract model for an instance
    fn extract_model_for_instance(
        &mut self,
        instance_name: &str,
        module: &ModuleDefinition,
        attributes: &HashMap<String, String>,
    ) -> Result<ExtractedModel> {
        // First try symbol table if available
        if let Some(symbol_data) = self.symbol_table.get(instance_name) {
            if let Ok(model) = self.model_extractor.extract_from_symbol_table(instance_name, symbol_data) {
                return Ok(model);
            }
        }
        
        // Then try user attributes
        if !attributes.is_empty() {
            if let Ok(model) = self.model_extractor.extract_from_user_attributes(instance_name, attributes) {
                return Ok(model);
            }
        }
        
        // Extract from module information
        let mut module_attrs = HashMap::new();
        module_attrs.insert("component_type".to_string(), self.infer_component_type(&module.name));
        
        // Add module parameters
        for (key, value) in &module.attributes {
            module_attrs.insert(key.clone(), value.clone());
        }
        
        // Try to extract value from module name (e.g., "Res_10k")
        if let Some(value) = self.extract_value_from_name(&module.name) {
            module_attrs.insert("value".to_string(), value);
        }
        
        self.model_extractor.extract_from_symbol_table(instance_name, &module_attrs)
            .or_else(|_| {
                // Last resort: context inference
                let connections = vec![]; // TODO: Get actual connections
                let nearby = vec![]; // TODO: Get nearby components
                self.model_extractor.infer_from_context(instance_name, &connections, &nearby)
            })
    }
    
    /// Get nets connected to an instance
    fn get_connected_nets(
        &self,
        netlist: &Netlist,
        instance_id: InstanceId,
    ) -> Result<Vec<(NetId, String)>> {
        let mut connected_nets = Vec::new();
        
        // Better approach: look at pin instances for this instance
        for (pin_inst_id, pin_inst) in &netlist.pin_instances {
            if pin_inst.instance == instance_id {
                if let Some(net_id) = pin_inst.net {
                    if let Some(net) = netlist.nets.get(net_id) {
                        let net_name = net.name.clone()
                            .unwrap_or_else(|| format!("net_{:?}", net_id));
                        
                        // Add the net if not already present
                        if !connected_nets.iter().any(|(id, _)| *id == net_id) {
                            connected_nets.push((net_id, net_name));
                        }
                    }
                }
            }
        }
        
        debug!("Instance {:?} has {} connected nets", instance_id, connected_nets.len());
        
        Ok(connected_nets)
    }
    
    /// Add a physical component to the circuit
    fn add_physical_component(
        &self,
        circuit: &mut Circuit,
        netlist: &Netlist,
        instance_name: &str,
        instance_id: InstanceId,
        connected_nets: &[(NetId, String)],
        extracted_model: ExtractedModel,
    ) -> Result<()> {
        // For 2-terminal components
        if connected_nets.len() >= 2 {
            let node1 = &connected_nets[0].1;
            let node2 = &connected_nets[1].1;
            
            // Get primary value from extracted model
            let value = self.get_primary_value(&extracted_model);
            
            info!("Adding component {} ({:?}): {} -> {}, value={}",
                  instance_name, extracted_model.component_type, node1, node2, value);
            
            circuit.add_branch(
                instance_name.to_string(),
                node1,
                node2,
                format!("{:?}", extracted_model.component_type),
                value,
                Some(instance_id),
            );
        } else if connected_nets.len() == 1 {
            // Single-pin components (like test points)
            debug!("Single-pin component {}: connected to {}", 
                   instance_name, connected_nets[0].1);
        } else {
            warn!("Component {} has no connections", instance_name);
        }
        
        Ok(())
    }
    
    /// Infer component type from module name
    fn infer_component_type(&self, module_name: &str) -> String {
        let lower = module_name.to_lowercase();
        if lower.contains("voltage") && lower.contains("source") {
            "voltage_source".to_string()
        } else if lower.contains("res") || lower.starts_with('r') {
            "resistor".to_string()
        } else if lower.contains("cap") || lower.starts_with('c') {
            "capacitor".to_string()
        } else if lower.contains("ind") || lower.starts_with('l') {
            "inductor".to_string()
        } else if lower.contains("led") {
            "led".to_string()
        } else if lower.contains("diode") || lower.starts_with('d') {
            "diode".to_string()
        } else if lower.starts_with('v') {
            "voltage_source".to_string()
        } else {
            module_name.to_string()
        }
    }
    
    /// Extract value from component name (e.g., "Res_10k" -> "10k")
    fn extract_value_from_name(&self, name: &str) -> Option<String> {
        // Look for patterns like _10k, _100n, etc.
        if let Some(underscore_pos) = name.rfind('_') {
            let value_part = &name[underscore_pos + 1..];
            // Check if it looks like a value
            if value_part.chars().next()?.is_numeric() {
                return Some(value_part.to_string());
            }
        }
        
        // Look for embedded values like "R10k" or "C100n"
        if name.len() > 1 {
            let without_prefix = &name[1..];
            if without_prefix.chars().next().map(|c| c.is_numeric()).unwrap_or(false) {
                return Some(without_prefix.to_string());
            }
        }
        
        None
    }
    
    /// Get primary value from extracted model
    fn get_primary_value(&self, model: &ExtractedModel) -> f64 {
        use crate::components::ComponentType;
        
        let value = match model.component_type {
            ComponentType::Resistor => {
                model.parameters.get("resistance").copied().unwrap_or(1e3)
            }
            ComponentType::Capacitor => {
                model.parameters.get("capacitance").copied().unwrap_or(1e-6)
            }
            ComponentType::Inductor => {
                model.parameters.get("inductance").copied().unwrap_or(1e-6)
            }
            ComponentType::VoltageSource => {
                model.parameters.get("voltage").copied().unwrap_or(5.0)
            }
            _ => 1.0,
        };
        
        debug!("Primary value for {:?}: {}", model.component_type, value);
        value
    }
}

impl Default for NetlistToSpiceConverter {
    fn default() -> Self {
        Self::new()
    }
}

/// Enhanced Circuit::from_netlist using the converter
impl Circuit {
    /// Create SPICE circuit from BHDL netlist with proper models
    pub fn from_netlist_with_models(netlist: &Netlist) -> Result<Self> {
        let mut converter = NetlistToSpiceConverter::new();
        converter.convert(netlist)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_value_extraction() {
        let converter = NetlistToSpiceConverter::new();
        
        assert_eq!(converter.extract_value_from_name("Res_10k"), Some("10k".to_string()));
        assert_eq!(converter.extract_value_from_name("Cap_100nF"), Some("100nF".to_string()));
        assert_eq!(converter.extract_value_from_name("R10k"), Some("10k".to_string()));
        assert_eq!(converter.extract_value_from_name("C100n"), Some("100n".to_string()));
        assert_eq!(converter.extract_value_from_name("LED"), None);
    }
    
    #[test]
    fn test_component_type_inference() {
        let converter = NetlistToSpiceConverter::new();
        
        assert_eq!(converter.infer_component_type("Resistor"), "resistor");
        assert_eq!(converter.infer_component_type("Res_10k"), "resistor");
        assert_eq!(converter.infer_component_type("Cap_100n"), "capacitor");
        assert_eq!(converter.infer_component_type("LED_Red"), "led");
        assert_eq!(converter.infer_component_type("Diode_1N4148"), "diode");
    }
}