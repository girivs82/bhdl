//! Integration between BHDL AST pin metadata and SPICE component role detection
//! 
//! This module bridges the gap between pin metadata annotations in BHDL source
//! and the SPICE component role detection system, enabling more accurate
//! role inference based on explicit functional declarations.

use std::collections::HashMap;
use bhdl_netlist::{Netlist, InstanceId};
use bhdl_common::{PinMetadata, PinFunction, AnalysisData};
use crate::circuit::ComponentId;

/// Pin metadata extracted from BHDL AST and analyzer results
#[derive(Debug, Clone)]
pub struct ExtractedPinMetadata {
    /// Map from (module_name, pin_name) to pin metadata
    pub module_pins: HashMap<(String, String), crate::pin_metadata::PinMetadata>,
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
    
    // Extract module definitions from analysis result
    for (module_name, module_def) in &analysis.module_definitions {
        // Extract pin metadata from module definition
        for (pin_name, pin_metadata) in &module_def.pins.pins {
            // Convert common pin metadata to SPICE pin metadata
            let metadata = convert_bhdl_common_pin_metadata(pin_metadata);
            module_pins.insert(
                (module_name.clone(), pin_name.clone()),
                metadata
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

/// Convert bhdl_common PinMetadata to SPICE pin metadata format
fn convert_bhdl_common_pin_metadata(common_metadata: &bhdl_common::PinMetadata) -> crate::pin_metadata::PinMetadata {
    // Extract function from common metadata
    let function = if let Some(func_str) = &common_metadata.function {
        if let Some(func) = bhdl_common::PinFunction::from_str(func_str) {
            match func {
                bhdl_common::PinFunction::PowerInput => crate::pin_metadata::PinFunction::PowerIn,
                bhdl_common::PinFunction::PowerOutput => crate::pin_metadata::PinFunction::PowerOut,
                bhdl_common::PinFunction::SwitchNode => crate::pin_metadata::PinFunction::SwitchNode,
                bhdl_common::PinFunction::FeedbackInput => crate::pin_metadata::PinFunction::Feedback,
                bhdl_common::PinFunction::Compensation => crate::pin_metadata::PinFunction::Compensation,
                bhdl_common::PinFunction::Enable => crate::pin_metadata::PinFunction::Enable,
                bhdl_common::PinFunction::CurrentSense => crate::pin_metadata::PinFunction::CurrentSense,
                bhdl_common::PinFunction::Ground => crate::pin_metadata::PinFunction::Ground,
                bhdl_common::PinFunction::Bypass => crate::pin_metadata::PinFunction::Bypass,
                _ => crate::pin_metadata::PinFunction::Unknown,
            }
        } else {
            crate::pin_metadata::PinFunction::Unknown
        }
    } else {
        crate::pin_metadata::PinFunction::Unknown
    };
    
    // Extract electrical characteristics
    let electrical = extract_electrical_data_from_common(common_metadata);
    
    // Get description from extra metadata
    let description = common_metadata.extra.get("description").cloned();
    
    crate::pin_metadata::PinMetadata {
        function,
        electrical,
        description,
    }
}

/// Extract electrical data from common pin metadata
fn extract_electrical_data_from_common(metadata: &bhdl_common::PinMetadata) -> crate::pin_metadata::PinElectricalData {
    let mut electrical = crate::pin_metadata::PinElectricalData::default();
    
    // Parse voltage range if specified
    if let Some(max_voltage) = &metadata.max_voltage {
        if let Ok(voltage) = parse_voltage(max_voltage) {
            electrical.voltage_range = Some((0.0, voltage));
        }
    }
    
    // Parse impedance if specified
    if let Some(impedance_str) = metadata.extra.get("impedance") {
        if let Ok(impedance) = parse_impedance(impedance_str) {
            electrical.impedance = Some(impedance);
        }
    }
    
    // Parse slew rate for switch nodes
    if let Some(slew_rate) = &metadata.slew_rate {
        if slew_rate == "fast" {
            electrical.dv_dt_rating = Some(100.0); // 100V/µs for fast switching
        }
    }
    
    // Parse drive strength as max current
    if let Some(drive_strength) = metadata.extra.get("drive_strength") {
        if let Ok(current) = parse_current(drive_strength) {
            electrical.max_current = Some(current);
        }
    }
    
    electrical
}

/// Convert AST pin metadata to SPICE pin metadata format
fn convert_ast_pin_metadata(pin_info: &HashMap<String, String>) -> crate::pin_metadata::PinMetadata {
    // Extract function from metadata
    let function = if let Some(func_str) = pin_info.get("function") {
        match func_str.as_str() {
            "PowerIn" => crate::pin_metadata::PinFunction::PowerIn,
            "PowerOut" => crate::pin_metadata::PinFunction::PowerOut,
            "SwitchNode" => crate::pin_metadata::PinFunction::SwitchNode,
            "Bootstrap" => crate::pin_metadata::PinFunction::Bootstrap,
            "Feedback" => crate::pin_metadata::PinFunction::Feedback,
            "Compensation" => crate::pin_metadata::PinFunction::Compensation,
            "SoftStart" => crate::pin_metadata::PinFunction::SoftStart,
            "Enable" => crate::pin_metadata::PinFunction::Enable,
            "CurrentSense" => crate::pin_metadata::PinFunction::CurrentSense,
            "Ground" => crate::pin_metadata::PinFunction::Ground,
            "Signal" => crate::pin_metadata::PinFunction::Signal,
            _ => crate::pin_metadata::PinFunction::Unknown,
        }
    } else {
        // Infer from pin type if no explicit function
        match pin_info.get("type").map(|s| s.as_str()) {
            Some("power") => {
                match pin_info.get("direction").map(|s| s.as_str()) {
                    Some("in") => crate::pin_metadata::PinFunction::PowerIn,
                    Some("out") => crate::pin_metadata::PinFunction::PowerOut,
                    _ => crate::pin_metadata::PinFunction::Unknown,
                }
            },
            Some("ground") => crate::pin_metadata::PinFunction::Ground,
            Some("signal") => crate::pin_metadata::PinFunction::Signal,
            _ => crate::pin_metadata::PinFunction::Unknown,
        }
    };
    
    // Extract electrical characteristics
    let electrical = extract_electrical_data(pin_info);
    
    // Get description
    let description = pin_info.get("description").cloned();
    
    crate::pin_metadata::PinMetadata {
        function,
        electrical,
        description,
    }
}

/// Extract electrical data from pin metadata
fn extract_electrical_data(pin_info: &HashMap<String, String>) -> crate::pin_metadata::PinElectricalData {
    let mut electrical = crate::pin_metadata::PinElectricalData::default();
    
    // Parse voltage range if specified
    if let Some(max_voltage) = pin_info.get("max_voltage") {
        if let Ok(voltage) = parse_voltage(max_voltage) {
            electrical.voltage_range = Some((0.0, voltage));
        }
    }
    
    // Parse impedance if specified
    if let Some(impedance_str) = pin_info.get("impedance") {
        if let Ok(impedance) = parse_impedance(impedance_str) {
            electrical.impedance = Some(impedance);
        }
    }
    
    // Parse slew rate for switch nodes
    if let Some(slew_rate) = pin_info.get("slew_rate") {
        if slew_rate == "fast" {
            electrical.dv_dt_rating = Some(100.0); // 100V/µs for fast switching
        }
    }
    
    // Parse drive strength as max current
    if let Some(drive_strength) = pin_info.get("drive_strength") {
        if let Ok(current) = parse_current(drive_strength) {
            electrical.max_current = Some(current);
        }
    }
    
    electrical
}

/// Parse voltage string (e.g., "30V", "5.5V")
fn parse_voltage(voltage_str: &str) -> Result<f64, ()> {
    let trimmed = voltage_str.trim_end_matches('V');
    trimmed.parse::<f64>().map_err(|_| ())
}

/// Parse impedance string (e.g., "10Mohm", "high")
fn parse_impedance(impedance_str: &str) -> Result<f64, ()> {
    match impedance_str {
        "high" => Ok(1e6), // 1MΩ for high impedance
        "low" => Ok(50.0), // 50Ω for low impedance
        s if s.ends_with("ohm") => {
            let mut value_str = s.trim_end_matches("ohm");
            let multiplier = if value_str.ends_with('M') {
                value_str = value_str.trim_end_matches('M');
                1e6
            } else if value_str.ends_with('k') {
                value_str = value_str.trim_end_matches('k');
                1e3
            } else {
                1.0
            };
            value_str.parse::<f64>()
                .map(|v| v * multiplier)
                .map_err(|_| ())
        },
        _ => Err(()),
    }
}

/// Parse current string (e.g., "50mA", "1A")
fn parse_current(current_str: &str) -> Result<f64, ()> {
    let (value_str, multiplier) = if current_str.ends_with("mA") {
        (current_str.trim_end_matches("mA"), 1e-3)
    } else if current_str.ends_with('A') {
        (current_str.trim_end_matches('A'), 1.0)
    } else {
        return Err(());
    };
    
    value_str.parse::<f64>()
        .map(|v| v * multiplier)
        .map_err(|_| ())
}

/// Update component role detector's pin database with extracted metadata
pub fn update_pin_database_from_ast(
    detector: &mut crate::ComponentRoleDetector,
    extracted: &ExtractedPinMetadata,
) {
    // Access the pin database through the detector
    for ((module_name, pin_name), metadata) in &extracted.module_pins {
        detector.pin_database.add_pin_metadata(
            module_name,
            pin_name,
            metadata.clone()
        );
    }
}

/// Get pin function for a specific component and pin
pub fn get_component_pin_function(
    component_id: ComponentId,
    pin_name: &str,
    instance_to_component: &HashMap<InstanceId, ComponentId>,
    extracted: &ExtractedPinMetadata,
) -> Option<crate::pin_metadata::PinFunction> {
    // Find the instance ID for this component
    let instance_id = instance_to_component.iter()
        .find(|(_, &comp_id)| comp_id == component_id)
        .map(|(inst_id, _)| inst_id)?;
    
    // Get the module type for this instance
    let module_type = extracted.instance_types.get(instance_id)?;
    
    // Look up the pin metadata
    let metadata = extracted.module_pins.get(&(module_type.clone(), pin_name.to_string()))?;
    
    Some(metadata.function.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_voltage_parsing() {
        assert_eq!(parse_voltage("30V"), Ok(30.0));
        assert_eq!(parse_voltage("5.5V"), Ok(5.5));
        assert_eq!(parse_voltage("0.5V"), Ok(0.5));
    }
    
    #[test]
    fn test_impedance_parsing() {
        assert_eq!(parse_impedance("10Mohm"), Ok(10e6));
        assert_eq!(parse_impedance("1kohm"), Ok(1e3));
        assert_eq!(parse_impedance("50ohm"), Ok(50.0));
        assert_eq!(parse_impedance("high"), Ok(1e6));
        assert_eq!(parse_impedance("low"), Ok(50.0));
    }
    
    #[test]
    fn test_current_parsing() {
        assert_eq!(parse_current("50mA"), Ok(0.05));
        assert_eq!(parse_current("1A"), Ok(1.0));
        assert_eq!(parse_current("100mA"), Ok(0.1));
    }
}