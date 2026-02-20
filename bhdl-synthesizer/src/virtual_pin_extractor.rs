//! Virtual Pin Extraction from AST
//! 
//! This module extracts virtual pin expansion definitions from BHDL module ASTs,
//! particularly from const declarations like TPS54331_VIRTUAL_PIN_EXPANSION.

use anyhow::{Result, Context};
use bhdl_ast::{Entity, AstNode, HasName};
use bhdl_stdlib::virtual_pins::VirtualPinComponent;
use std::collections::HashMap;
use log::{info, debug, warn};

/// Extracts virtual pin components from a module's AST
pub struct VirtualPinExtractor;

impl VirtualPinExtractor {
    /// Extract virtual pin components from a module AST
    pub fn extract_from_entity(module: &Entity) -> Option<Vec<VirtualPinComponent>> {
        let module_name = module.name()?.text().to_string();
        info!("Extracting virtual pins from module: {}", module_name);
        
        // Look for virtual pin declarations in the module
        let has_virtual_pins = Self::entity_has_virtual_pins(module);
        if !has_virtual_pins {
            debug!("Module {} has no virtual pins", module_name);
            return None;
        }
        
        // Look for expansion const (e.g., TPS54331_VIRTUAL_PIN_EXPANSION)
        let expansion_const_name = format!("{}_VIRTUAL_PIN_EXPANSION", module_name);
        
        // Try to find the const declaration
        if let Some(expansion_data) = Self::find_expansion_const(module, &expansion_const_name) {
            info!("Found virtual pin expansion const for {}", module_name);
            return Some(Self::parse_expansion_data(expansion_data));
        }
        
        // If no explicit expansion const, try to infer from module type
        Self::infer_virtual_components(&module_name)
    }
    
    /// Check if module has any virtual pins
    fn entity_has_virtual_pins(module: &Entity) -> bool {
        // Look through pin declarations for 'virtual' keyword
        for child in module.syntax().children() {
            if child.kind() == bhdl_ast::SyntaxKind::PIN_DECL {
                let text = child.text().to_string();
                if text.contains("virtual") {
                    return true;
                }
            }
        }
        false
    }
    
    /// Find expansion const declaration in module
    fn find_expansion_const(module: &Entity, const_name: &str) -> Option<String> {
        // Look for const declaration with the given name
        // This would need to parse the const value
        for child in module.syntax().children() {
            if child.kind() == bhdl_ast::SyntaxKind::PARAM_DECL {
                let text = child.text().to_string();
                if text.contains(const_name) {
                    debug!("Found expansion const: {}", const_name);
                    // For now, return the text - proper parsing would extract the value
                    return Some(text);
                }
            }
        }
        
        // Also check after the module for file-level consts
        if let Some(parent) = module.syntax().parent() {
            for sibling in parent.children() {
                if sibling.kind() == bhdl_ast::SyntaxKind::PARAM_DECL {
                    let text = sibling.text().to_string();
                    if text.contains(const_name) {
                        debug!("Found expansion const at file level: {}", const_name);
                        return Some(text);
                    }
                }
            }
        }
        
        None
    }
    
    /// Parse expansion data from const declaration
    fn parse_expansion_data(expansion_text: String) -> Vec<VirtualPinComponent> {
        let mut components = Vec::new();
        
        // This is a simplified parser - in reality we'd need to properly parse the object literal
        // For now, extract key patterns
        
        // Look for inductor definition
        if expansion_text.contains("inductor:") {
            components.push(VirtualPinComponent {
                component_type: "Inductor".to_string(),
                reference: "L1".to_string(),
                value: "15µH".to_string(), // Default for 5V
                specs: HashMap::from([
                    ("current_rating".to_string(), "4A".to_string()),
                    ("dcr_max".to_string(), "30mΩ".to_string()),
                ]),
                connection_pattern: "series".to_string(),
                formula: Some("L = (Vout × (Vin - Vout)) / (ΔI × f × Vin)".to_string()),
                placement: Some("close_to_switch_node".to_string()),
                intent: Some("energy_storage".to_string()),
            });
        }
        
        // Look for bootstrap capacitor
        if expansion_text.contains("bootstrap_cap:") {
            components.push(VirtualPinComponent {
                component_type: "Capacitor".to_string(),
                reference: "C_BOOT".to_string(),
                value: "100nF".to_string(),
                specs: HashMap::from([
                    ("voltage_rating".to_string(), "16V".to_string()),
                    ("type".to_string(), "Ceramic X7R".to_string()),
                ]),
                connection_pattern: "between_pins".to_string(),
                formula: None,
                placement: Some("close_to_ic".to_string()),
                intent: Some("bootstrap".to_string()),
            });
        }
        
        // Look for output capacitors
        if expansion_text.contains("output_caps:") {
            // Main output cap
            components.push(VirtualPinComponent {
                component_type: "Capacitor".to_string(),
                reference: "C_OUT1".to_string(),
                value: "22µF".to_string(),
                specs: HashMap::from([
                    ("voltage_rating".to_string(), "10V".to_string()),
                    ("type".to_string(), "Ceramic X7R".to_string()),
                ]),
                connection_pattern: "to_ground".to_string(),
                formula: None,
                placement: Some("close_to_output".to_string()),
                intent: Some("output_filtering".to_string()),
            });
            
            // HF bypass
            components.push(VirtualPinComponent {
                component_type: "Capacitor".to_string(),
                reference: "C_OUT2".to_string(),
                value: "100nF".to_string(),
                specs: HashMap::from([
                    ("voltage_rating".to_string(), "10V".to_string()),
                    ("type".to_string(), "Ceramic X7R".to_string()),
                ]),
                connection_pattern: "to_ground".to_string(),
                formula: None,
                placement: Some("close_to_output".to_string()),
                intent: Some("high_frequency_bypass".to_string()),
            });
        }
        
        // Look for feedback network
        if expansion_text.contains("feedback_network:") {
            // Top resistor
            components.push(VirtualPinComponent {
                component_type: "Resistor".to_string(),
                reference: "R_FB1".to_string(),
                value: "47kΩ".to_string(), // For 5V output with 0.8V ref
                specs: HashMap::from([
                    ("tolerance".to_string(), "1%".to_string()),
                    ("power".to_string(), "0.125W".to_string()),
                ]),
                connection_pattern: "voltage_divider_top".to_string(),
                formula: Some("R1 = R2 × (Vout/0.8 - 1)".to_string()),
                placement: Some("close_to_fb_pin".to_string()),
                intent: Some("feedback_divider".to_string()),
            });
            
            // Bottom resistor
            components.push(VirtualPinComponent {
                component_type: "Resistor".to_string(),
                reference: "R_FB2".to_string(),
                value: "10kΩ".to_string(),
                specs: HashMap::from([
                    ("tolerance".to_string(), "1%".to_string()),
                    ("power".to_string(), "0.125W".to_string()),
                ]),
                connection_pattern: "voltage_divider_bottom".to_string(),
                formula: None,
                placement: Some("close_to_fb_pin".to_string()),
                intent: Some("feedback_divider".to_string()),
            });
        }
        
        components
    }
    
    /// Infer virtual components based on module type
    fn infer_virtual_components(module_name: &str) -> Option<Vec<VirtualPinComponent>> {
        match module_name {
            "TPS54331" | "LM2596" => {
                // Buck converter - needs inductor, caps, feedback
                Some(Self::default_buck_converter_components())
            }
            "LM7805" | "LM317" => {
                // Linear regulator - needs caps
                Some(Self::default_linear_regulator_components())
            }
            _ => None
        }
    }
    
    /// Default components for buck converters
    fn default_buck_converter_components() -> Vec<VirtualPinComponent> {
        vec![
            VirtualPinComponent {
                component_type: "Inductor".to_string(),
                reference: "L1".to_string(),
                value: "22µH".to_string(),
                specs: HashMap::from([
                    ("current_rating".to_string(), "3A".to_string()),
                ]),
                connection_pattern: "series".to_string(),
                formula: None,
                placement: Some("close_to_switch".to_string()),
                intent: Some("energy_storage".to_string()),
            },
            VirtualPinComponent {
                component_type: "Capacitor".to_string(),
                reference: "C_OUT".to_string(),
                value: "100µF".to_string(),
                specs: HashMap::from([
                    ("voltage_rating".to_string(), "16V".to_string()),
                ]),
                connection_pattern: "to_ground".to_string(),
                formula: None,
                placement: Some("close_to_output".to_string()),
                intent: Some("output_filtering".to_string()),
            },
        ]
    }
    
    /// Default components for linear regulators
    fn default_linear_regulator_components() -> Vec<VirtualPinComponent> {
        vec![
            VirtualPinComponent {
                component_type: "Capacitor".to_string(),
                reference: "C_OUT".to_string(),
                value: "10µF".to_string(),
                specs: HashMap::from([
                    ("voltage_rating".to_string(), "16V".to_string()),
                ]),
                connection_pattern: "to_ground".to_string(),
                formula: None,
                placement: Some("close_to_output".to_string()),
                intent: Some("output_stabilization".to_string()),
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_buck_components() {
        let components = VirtualPinExtractor::default_buck_converter_components();
        assert!(!components.is_empty());
        assert!(components.iter().any(|c| c.component_type == "Inductor"));
        assert!(components.iter().any(|c| c.component_type == "Capacitor"));
    }
    
    #[test]
    fn test_default_linear_components() {
        let components = VirtualPinExtractor::default_linear_regulator_components();
        assert!(!components.is_empty());
        assert!(components.iter().any(|c| c.component_type == "Capacitor"));
    }
}