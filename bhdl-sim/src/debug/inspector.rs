//! State inspection utilities for debugging

use std::collections::HashMap;
use bhdl_netlist::{Netlist, InstanceId, NetId};
use bhdl_analyzer::expression_evaluator::RuntimeValue;
use crate::circuit::{CircuitState, PinValue, NetValue};
use crate::error::SimulationResult;

/// Result of an inspection query
#[derive(Debug, Clone)]
pub enum InspectionResult {
    /// Single value result
    Value(String),
    /// Multiple values (e.g., all pins of an instance)
    Values(HashMap<String, String>),
    /// Hierarchical result
    Hierarchy(HashMap<String, InspectionResult>),
    /// Error during inspection
    Error(String),
}

/// State inspector for examining simulation state
pub struct StateInspector<'a> {
    netlist: &'a Netlist,
    state: &'a CircuitState,
}

impl<'a> StateInspector<'a> {
    pub fn new(netlist: &'a Netlist, state: &'a CircuitState) -> Self {
        Self { netlist, state }
    }

    /// Inspect a value by path
    pub fn inspect(&self, path: &str) -> InspectionResult {
        // Parse the path
        let parts: Vec<&str> = path.split('.').collect();
        
        if parts.is_empty() {
            return InspectionResult::Error("Empty path".to_string());
        }

        match parts[0] {
            "instance" => self.inspect_instance(&parts[1..]),
            "net" => self.inspect_net(&parts[1..]),
            "attr" | "attribute" => self.inspect_attribute(&parts[1..]),
            "pin" => self.inspect_pin(&parts[1..]),
            _ => {
                // Try to find as instance name
                self.inspect_instance_by_name(parts[0], &parts[1..])
            }
        }
    }

    /// Inspect all instances
    pub fn inspect_all_instances(&self) -> InspectionResult {
        let mut results = HashMap::new();
        
        for (id, instance) in &self.netlist.instances {
            let instance_info = self.inspect_instance_details(id);
            results.insert(instance.name.clone(), instance_info);
        }
        
        InspectionResult::Hierarchy(results)
    }

    /// Inspect all nets
    pub fn inspect_all_nets(&self) -> InspectionResult {
        let mut results = HashMap::new();
        
        for (id, net) in &self.netlist.nets {
            let net_name = net.name.as_ref()
                .unwrap_or(&format!("net_{:?}", id))
                .clone();
            
            if let Some(value) = self.state.get_net(id) {
                results.insert(net_name, self.format_net_value(value));
            } else {
                results.insert(net_name, "uninitialized".to_string());
            }
        }
        
        InspectionResult::Values(results)
    }

    /// Inspect all attributes
    pub fn inspect_all_attributes(&self) -> InspectionResult {
        let mut results = HashMap::new();
        
        // Get all changed attributes
        for attr_path in self.state.changed_attributes() {
            if let Some(value) = self.state.get_attribute(attr_path) {
                results.insert(attr_path.clone(), self.format_runtime_value(value));
            }
        }
        
        InspectionResult::Values(results)
    }

    fn inspect_instance(&self, path: &[&str]) -> InspectionResult {
        if path.is_empty() {
            return self.inspect_all_instances();
        }

        // Try to parse as instance ID
        if let Ok(_id) = path[0].parse::<u32>() {
            // Create instance ID from raw value (this is a simplification)
            let instance_id = InstanceId::default(); // This won't work properly
            return self.inspect_instance_details(instance_id);
        }

        InspectionResult::Error(format!("Invalid instance ID: {}", path[0]))
    }

    fn inspect_instance_by_name(&self, name: &str, path: &[&str]) -> InspectionResult {
        // Find instance by name
        for (id, instance) in &self.netlist.instances {
            if instance.name == name {
                if path.is_empty() {
                    return self.inspect_instance_details(id);
                } else if path[0] == "pins" {
                    return self.inspect_instance_pins(id);
                } else {
                    return self.inspect_instance_pin(id, path[0]);
                }
            }
        }
        
        InspectionResult::Error(format!("Instance '{}' not found", name))
    }

    fn inspect_instance_details(&self, id: InstanceId) -> InspectionResult {
        if let Some(instance) = self.netlist.instances.get(id) {
            let mut info = HashMap::new();
            info.insert("name".to_string(), instance.name.clone());
            
            if let Some(module) = self.netlist.modules.get(instance.definition) {
                info.insert("type".to_string(), module.name.clone());
            }
            
            // Add pins
            let pins = self.inspect_instance_pins(id);
            
            InspectionResult::Hierarchy(HashMap::from([
                ("info".to_string(), InspectionResult::Values(info)),
                ("pins".to_string(), pins),
            ]))
        } else {
            InspectionResult::Error("Instance not found".to_string())
        }
    }

    fn inspect_instance_pins(&self, id: InstanceId) -> InspectionResult {
        let mut pins = HashMap::new();
        
        if let Some(instance) = self.netlist.instances.get(id) {
            if let Some(module) = self.netlist.modules.get(instance.definition) {
                for &pin_id in &module.pins {
                    if let Some(pin) = self.netlist.pins.get(pin_id) {
                        let pin_path = format!("{:?}:{}", id, pin.name);
                        if let Some(value) = self.state.get_pin(&pin_path) {
                            pins.insert(pin.name.clone(), self.format_pin_value(value));
                        } else {
                            pins.insert(pin.name.clone(), "uninitialized".to_string());
                        }
                    }
                }
            }
        }
        
        InspectionResult::Values(pins)
    }

    fn inspect_instance_pin(&self, id: InstanceId, pin_name: &str) -> InspectionResult {
        let pin_path = format!("{:?}:{}", id, pin_name);
        if let Some(value) = self.state.get_pin(&pin_path) {
            InspectionResult::Value(self.format_pin_value(value))
        } else {
            InspectionResult::Error("Pin not found or uninitialized".to_string())
        }
    }

    fn inspect_net(&self, path: &[&str]) -> InspectionResult {
        if path.is_empty() {
            return self.inspect_all_nets();
        }

        // Find net by name
        for (id, net) in &self.netlist.nets {
            if let Some(name) = &net.name {
                if name == path[0] {
                    if let Some(value) = self.state.get_net(id) {
                        return InspectionResult::Value(self.format_net_value(value));
                    } else {
                        return InspectionResult::Value("uninitialized".to_string());
                    }
                }
            }
        }

        InspectionResult::Error(format!("Net '{}' not found", path[0]))
    }

    fn inspect_attribute(&self, path: &[&str]) -> InspectionResult {
        if path.is_empty() {
            return self.inspect_all_attributes();
        }

        let attr_path = path.join(".");
        if let Some(value) = self.state.get_attribute(&attr_path) {
            InspectionResult::Value(self.format_runtime_value(value))
        } else {
            InspectionResult::Error(format!("Attribute '{}' not found", attr_path))
        }
    }

    fn inspect_pin(&self, path: &[&str]) -> InspectionResult {
        if path.len() < 2 {
            return InspectionResult::Error("Pin path requires instance and pin name".to_string());
        }

        self.inspect_instance_by_name(path[0], &[path[1]])
    }

    fn format_pin_value(&self, value: &PinValue) -> String {
        if let Some(level) = value.logic_level {
            format!("{:?} ({}V, {}Ω)", level, value.voltage, value.impedance)
        } else {
            format!("{}V @ {}mA ({}Ω)", value.voltage, value.current * 1000.0, value.impedance)
        }
    }

    fn format_net_value(&self, value: &NetValue) -> String {
        if let Some(level) = value.logic_level {
            format!("{:?} ({}V)", level, value.voltage)
        } else {
            format!("{}V @ {}mA", value.voltage, value.current * 1000.0)
        }
    }

    fn format_runtime_value(&self, value: &RuntimeValue) -> String {
        match value {
            RuntimeValue::Integer(i) => i.to_string(),
            RuntimeValue::Real(r) => format!("{:.6}", r),
            RuntimeValue::String(s) => s.clone(),
            RuntimeValue::Boolean(b) => b.to_string(),
            RuntimeValue::Array(items) => format!("[{} items]", items.len()),
            RuntimeValue::Object(fields) => format!("{{ {} fields }}", fields.len()),
        }
    }
}

/// Pretty print inspection results
pub fn format_inspection_result(result: &InspectionResult, indent: usize) -> String {
    let indent_str = "  ".repeat(indent);
    
    match result {
        InspectionResult::Value(v) => v.clone(),
        InspectionResult::Values(map) => {
            let mut lines = Vec::new();
            for (k, v) in map {
                lines.push(format!("{}{}: {}", indent_str, k, v));
            }
            lines.join("\n")
        }
        InspectionResult::Hierarchy(map) => {
            let mut lines = Vec::new();
            for (k, v) in map {
                lines.push(format!("{}{}:", indent_str, k));
                lines.push(format_inspection_result(v, indent + 1));
            }
            lines.join("\n")
        }
        InspectionResult::Error(e) => format!("{}ERROR: {}", indent_str, e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit::CircuitTopology;

    #[test]
    fn test_inspection_result_formatting() {
        let result = InspectionResult::Hierarchy(HashMap::from([
            ("info".to_string(), InspectionResult::Values(HashMap::from([
                ("name".to_string(), "cpu".to_string()),
                ("type".to_string(), "ARM_M4".to_string()),
            ]))),
            ("pins".to_string(), InspectionResult::Values(HashMap::from([
                ("VDD".to_string(), "3.3V @ 0mA (50Ω)".to_string()),
                ("CLK".to_string(), "High (3.3V, 50Ω)".to_string()),
            ]))),
        ]));

        let formatted = format_inspection_result(&result, 0);
        assert!(formatted.contains("info:"));
        assert!(formatted.contains("name: cpu"));
        assert!(formatted.contains("pins:"));
        assert!(formatted.contains("CLK: High"));
    }

    #[test]
    fn test_value_formatting() {
        let netlist = Netlist::new();
        let circuit_state = CircuitState::new(CircuitTopology {
            instance_modules: HashMap::new(),
            net_connections: HashMap::new(),
        });
        let inspector = StateInspector::new(&netlist, &circuit_state);

        // Test pin value formatting
        let pin_val = PinValue::analog(3.3);
        let formatted = inspector.format_pin_value(&pin_val);
        assert!(formatted.contains("3.3V"));

        // Test runtime value formatting
        let runtime_val = RuntimeValue::Real(3.14159);
        let formatted = inspector.format_runtime_value(&runtime_val);
        assert!(formatted.contains("3.141590"));
    }
}