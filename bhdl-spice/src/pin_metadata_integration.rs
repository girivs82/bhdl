//! Integration between BHDL AST pin metadata and SPICE component role detection
//! 
//! This module bridges the gap between pin metadata annotations in BHDL source
//! and the SPICE component role detection system, enabling more accurate
//! role inference based on explicit functional declarations.

use std::collections::HashMap;
use bhdl_netlist::{Netlist, InstanceId};
use bhdl_common::pin_metadata::{PinMetadata, PinFunction};
use bhdl_common::analysis_interface::AnalysisData;
use crate::circuit::ComponentId;

/// Pin metadata extracted from BHDL AST and analyzer results
#[derive(Debug, Clone)]
pub struct ExtractedPinMetadata {
    /// Map from (module_name, pin_name) to pin metadata
    pub module_pins: HashMap<(String, String), PinMetadata>,
    /// Map from instance ID to its module type
    pub instance_types: HashMap<InstanceId, String>,
}

/// Extract pin metadata from analysis results and netlist
pub fn extract_pin_metadata_from_analysis(
    analysis: &AnalysisData,
    netlist: &Netlist,
) -> ExtractedPinMetadata {
    let mut module_pins = HashMap::new();
    let mut instance_types = HashMap::new();
    
    // Extract module definitions from analysis result - no conversion needed!
    for (module_name, module_def) in &analysis.module_definitions {
        // Extract pin metadata from module definition
        for (pin_name, pin_metadata) in &module_def.pins.pins {
            module_pins.insert(
                (module_name.clone(), pin_name.clone()),
                pin_metadata.clone()
            );
        }
    }
    
    // Map instances to their module types
    for (instance_id, instance) in &netlist.instances {
        if let Some(module) = netlist.modules.get(instance.definition) {
            instance_types.insert(instance_id, module.name.clone());
        }
    }
    
    ExtractedPinMetadata {
        module_pins,
        instance_types,
    }
}

/// Apply extracted pin metadata to component role detection
pub fn apply_pin_metadata_to_detector(
    metadata: &ExtractedPinMetadata,
    instance_to_component: &HashMap<InstanceId, ComponentId>,
    detector: &mut crate::extended_analysis::component_role_detector::ComponentRoleDetector,
) {
    // For each component, find its pin functions
    for (instance_id, component_id) in instance_to_component {
        if let Some(module_type) = metadata.instance_types.get(instance_id) {
            // Look up all pins for this module type
            for ((mod_name, pin_name), pin_metadata) in &metadata.module_pins {
                if mod_name == module_type {
                    // Apply pin metadata to the detector's database
                    detector.pin_database.add_pin_metadata(
                        module_type,
                        pin_name,
                        pin_metadata.clone()
                    );
                }
            }
        }
    }
}