//! BHDL Standard Library Rust Interface
//! 
//! This module provides a Rust interface to read and parse BHDL stdlib component definitions
//! to extract pin information and other metadata.

use std::collections::HashMap;
use std::path::Path;
use std::fs;
use anyhow::{Result, Context};
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode, Module, PinDecl, HasName, AttributeDecl};
use bhdl_ast::expr::Expr;
use bhdl_netlist::types::{PinDirection, PinType};

pub mod intents;

/// Represents a pin definition extracted from a BHDL module
#[derive(Debug, Clone)]
pub struct StdlibPinDefinition {
    pub name: String,
    pub direction: PinDirection,
    pub pin_type: PinType,
}

/// Represents a component definition from the stdlib
#[derive(Debug, Clone)]
pub struct StdlibComponentDefinition {
    pub module_name: String,
    pub pins: Vec<StdlibPinDefinition>,
    pub attributes: HashMap<String, String>,
}

/// BHDL Standard Library reader
pub struct StdlibReader {
    /// Path to the bhdl-stdlib directory
    stdlib_path: String,
    /// Cache of parsed component definitions
    component_cache: HashMap<String, StdlibComponentDefinition>,
}

impl StdlibReader {
    /// Create a new stdlib reader
    pub fn new(stdlib_path: impl Into<String>) -> Self {
        Self {
            stdlib_path: stdlib_path.into(),
            component_cache: HashMap::new(),
        }
    }

    /// Load all component definitions from the stdlib
    pub fn load_all_components(&mut self) -> Result<()> {
        // Load passive components
        self.load_component_file("passives/resistor.bhdl")?;
        self.load_component_file("passives/capacitor.bhdl")?;
        self.load_component_file("passives/led.bhdl")?;
        self.load_component_file("passives/diode.bhdl")?;
        self.load_component_file("passives/fuse.bhdl")?;
        self.load_component_file("passives/tvs_diode.bhdl")?;
        
        // Load regulators
        self.load_component_file("regulators/lm7805.bhdl")?;
        
        // Load connectors
        self.load_component_file("connectors/testpoint.bhdl")?;
        
        // Load power components
        self.load_component_file("power/power.bhdl")?;
        self.load_component_file("power/ground.bhdl")?;
        
        Ok(())
    }

    /// Load a single component file
    fn load_component_file(&mut self, relative_path: &str) -> Result<()> {
        let full_path = Path::new(&self.stdlib_path).join(relative_path);
        let content = fs::read_to_string(&full_path)
            .with_context(|| format!("Failed to read {}", full_path.display()))?;
        
        // Parse the BHDL file
        let parse_result = parse(&content);
        let syntax_node = parse_result.syntax();
        let source_file = SourceFile::cast(syntax_node)
            .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
        
        // Extract module definitions
        for item in source_file.items() {
            if let Some(module) = Module::cast(item.syntax().clone()) {
                if let Some(component_def) = self.extract_component_definition(&module) {
                    // Store the component definition
                    self.component_cache.insert(component_def.module_name.clone(), component_def.clone());
                    
                    // Also handle module aliases (e.g., module Capacitor = Cap;)
                    // This is a simplified approach - in reality we'd need to parse alias syntax
                }
            }
        }
        
        Ok(())
    }

    /// Extract component definition from a module AST node
    fn extract_component_definition(&self, module: &Module) -> Option<StdlibComponentDefinition> {
        let module_name = module.name()?.text().to_string();
        let mut pins = Vec::new();
        let mut attributes = HashMap::new();
        
        // Extract attributes from the module
        for attr in module.attributes() {
            if let Some(name) = attr.name() {
                let attr_name = name.text().to_string();
                if let Some(value) = attr.value() {
                    // Convert the expression to a string representation
                    let attr_value = self.expr_to_string(&value);
                    attributes.insert(attr_name, attr_value);
                }
            }
        }
        
        // Now we can properly use the AST to get pins
        for pin in module.pins() {
            if let Some(pin_def) = self.extract_pin_definition(&pin) {
                pins.push(pin_def);
            }
        }
        
        Some(StdlibComponentDefinition {
            module_name,
            pins,
            attributes,
        })
    }

    /// Extract pin definition from a pin declaration
    fn extract_pin_definition(&self, pin_decl: &PinDecl) -> Option<StdlibPinDefinition> {
        let name = pin_decl.name()?.text().to_string();
        
        // Parse direction from pin declaration
        // The actual parsing would need to handle the BHDL syntax like "signal inout", "power in", etc.
        let (direction, pin_type) = self.parse_pin_type_and_direction(pin_decl)?;
        
        Some(StdlibPinDefinition {
            name,
            direction,
            pin_type,
        })
    }

    /// Parse pin type and direction from pin declaration
    fn parse_pin_type_and_direction(&self, pin_decl: &PinDecl) -> Option<(PinDirection, PinType)> {
        // This is a simplified version - real implementation would parse the actual syntax
        let decl_text = pin_decl.syntax().text().to_string();
        
        if decl_text.contains("power in") {
            Some((PinDirection::Power, PinType::Power))
        } else if decl_text.contains("power out") {
            Some((PinDirection::Power, PinType::Power))
        } else if decl_text.contains("ground") {
            Some((PinDirection::Ground, PinType::Ground))
        } else if decl_text.contains("signal in") {
            Some((PinDirection::In, PinType::Signal))
        } else if decl_text.contains("signal out") {
            Some((PinDirection::Out, PinType::Signal))
        } else if decl_text.contains("signal inout") {
            Some((PinDirection::InOut, PinType::Signal))
        } else {
            // Default for passive components
            Some((PinDirection::Passive, PinType::Passive))
        }
    }
    
    /// Convert an expression AST node to a string representation
    fn expr_to_string(&self, expr: &Expr) -> String {
        // For now, just use the syntax text - this preserves the original source
        expr.syntax().text().to_string()
    }

    /// Get component definition by name
    pub fn get_component(&self, name: &str) -> Option<&StdlibComponentDefinition> {
        // Try exact match first
        if let Some(def) = self.component_cache.get(name) {
            return Some(def);
        }
        
        // Try case-insensitive match
        let name_lower = name.to_lowercase();
        for (cached_name, def) in &self.component_cache {
            if cached_name.to_lowercase() == name_lower {
                return Some(def);
            }
        }
        
        // Try common aliases
        match name {
            "Resistor" => self.component_cache.get("Res"),
            "Capacitor" => self.component_cache.get("Cap"),
            "PWR" => self.component_cache.get("Power"),
            "GND" => self.component_cache.get("Ground"),
            _ => None,
        }
    }

    /// Get pins for a component type
    pub fn get_component_pins(&self, component_type: &str) -> Vec<StdlibPinDefinition> {
        self.get_component(component_type)
            .map(|def| def.pins.clone())
            .unwrap_or_else(|| {
                // Default fallback for unknown components
                vec![
                    StdlibPinDefinition {
                        name: "1".to_string(),
                        direction: PinDirection::InOut,
                        pin_type: PinType::Signal,
                    },
                    StdlibPinDefinition {
                        name: "2".to_string(),
                        direction: PinDirection::InOut,
                        pin_type: PinType::Signal,
                    },
                ]
            })
    }
}

/// Get the default stdlib path relative to the project root
pub fn get_default_stdlib_path() -> String {
    // This assumes we're running from the project root
    "bhdl-stdlib".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stdlib_reader_creation() {
        let reader = StdlibReader::new("bhdl-stdlib");
        assert!(reader.component_cache.is_empty());
    }
}