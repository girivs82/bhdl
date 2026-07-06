//! BHDL Standard Library Rust Interface
//! 
//! This module provides a Rust interface to read and parse BHDL stdlib component definitions
//! to extract pin information and other metadata.

use std::collections::HashMap;
use std::path::Path;
use std::fs;
use anyhow::{Result, Context};
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode, Entity, PinDecl, HasName, AttributeDecl};
use bhdl_ast::expr::Expr;
use bhdl_netlist::types::{PinDirection, PinType};
use log::{debug, info, warn};

pub mod intents;
pub mod virtual_pins;

use virtual_pins::{ComponentVirtualPins, VirtualPinDefinition, VirtualPinComponent};

/// Represents a pin definition extracted from a BHDL entity
#[derive(Debug, Clone)]
pub struct StdlibPinDefinition {
    pub name: String,
    pub direction: PinDirection,
    pub pin_type: PinType,
    pub is_virtual: bool,  // Whether this is a virtual pin that needs expansion
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
        self.load_component_file("passives/diode.bhdl")?;

        // Load optoelectronic components
        self.load_component_file("optoelectronic/led.bhdl")?;

        // Load protection components (TVSDiode + Fuse)
        self.load_component_file("protection/tvs.bhdl")?;

        // Load active components
        self.load_component_file("actives/custom_diode.bhdl")?;

        // Load regulators
        self.load_component_file("power/lm7805.bhdl")?;

        // Load switching regulators
        self.load_component_file("power/tps54331.bhdl")?;
        self.load_component_file("components/power/switching_regulators/LM2596.bhdl")?;

        // Load connectors
        self.load_component_file("connectors/testpoint.bhdl")?;

        // Load power components
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
        
        // Extract entity definitions
        for item in source_file.items() {
            if let Some(entity) = Entity::cast(item.syntax().clone()) {
                if let Some(component_def) = self.extract_component_definition(&entity) {
                    // Store the component definition
                    self.component_cache.insert(component_def.module_name.clone(), component_def.clone());

                    // Also handle entity aliases (e.g., alias Capacitor = Cap;)
                    // This is a simplified approach - in reality we'd need to parse alias syntax
                }
            }
        }
        
        Ok(())
    }

    /// Extract component definition from an entity AST node
    fn extract_component_definition(&self, entity: &Entity) -> Option<StdlibComponentDefinition> {
        let module_name = entity.name()?.text().to_string();
        let mut pins = Vec::new();
        let mut attributes = HashMap::new();
        
        // Extract attributes from the entity
        for attr in entity.attributes() {
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
        for pin in entity.pins() {
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
        
        // Check if this is a virtual pin
        let decl_text = pin_decl.syntax().text().to_string();
        let is_virtual = decl_text.contains("virtual");
        
        // Parse direction from pin declaration
        // The actual parsing would need to handle the BHDL syntax like "signal inout", "power in", etc.
        let (direction, pin_type) = self.parse_pin_type_and_direction(pin_decl)?;
        
        Some(StdlibPinDefinition {
            name,
            direction,
            pin_type,
            is_virtual,
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
                        is_virtual: false,
                    },
                    StdlibPinDefinition {
                        name: "2".to_string(),
                        direction: PinDirection::InOut,
                        pin_type: PinType::Signal,
                        is_virtual: false,
                    },
                ]
            })
    }
    
    /// Check if a component has virtual pins
    pub fn has_virtual_pins(&self, component_type: &str) -> bool {
        self.get_component(component_type)
            .map(|def| def.pins.iter().any(|p| p.is_virtual))
            .unwrap_or(false)
    }
    
    /// Get synthesis knowledge for a component from its BHDL file
    /// This extracts the _SYNTHESIS const if it exists
    pub fn get_synthesis_knowledge(&self, component_type: &str) -> Option<String> {
        // For now, return a placeholder - full implementation would parse the BHDL file
        // and extract the const TPS54331_SYNTHESIS or similar
        // debug!("Loading synthesis knowledge for component: {}", component_type);
        
        // TODO: Parse the BHDL file and extract synthesis knowledge
        // This would look for patterns like:
        // const TPS54331_SYNTHESIS = { ... }
        // const TPS54331_VIRTUAL_PIN_EXPANSION = { ... }
        
        None
    }
    
    /// Parse virtual pin expansion from BHDL const definition
    fn parse_virtual_pin_expansion(&self, bhdl_content: &str, component_type: &str) -> Option<ComponentVirtualPins> {
        // Look for the VIRTUAL_PIN_EXPANSION const
        let const_name = format!("{}_VIRTUAL_PIN_EXPANSION", component_type.to_uppercase());
        
        // Find the const definition
        let const_start = bhdl_content.find(&format!("const {} = {{", const_name))?;
        let content_after_start = &bhdl_content[const_start..];
        
        // Find the matching closing brace
        let mut brace_count = 0;
        let mut const_end = 0;
        let mut in_const = false;
        
        for (i, ch) in content_after_start.char_indices() {
            if ch == '{' {
                brace_count += 1;
                in_const = true;
            } else if ch == '}' {
                brace_count -= 1;
                if brace_count == 0 && in_const {
                    const_end = i + 1;
                    break;
                }
            }
        }
        
        if const_end == 0 {
            return None;
        }
        
        let const_content = &content_after_start[..const_end];
        
        // Parse the VOUT_expansion section
        self.parse_vout_expansion(const_content, component_type)
    }
    
    /// Parse VOUT expansion details from const content
    fn parse_vout_expansion(&self, const_content: &str, component_type: &str) -> Option<ComponentVirtualPins> {
        let mut virtual_pins = ComponentVirtualPins::new(component_type.to_string());
        
        // Look for VOUT_expansion section
        if !const_content.contains("VOUT_expansion:") {
            return None;
        }
        
        let mut vout_components = Vec::new();
        
        // Parse inductor if present
        if let Some(inductor) = self.parse_component_section(const_content, "inductor:") {
            vout_components.push(inductor);
        }
        
        // Parse bootstrap capacitor
        if let Some(bootstrap) = self.parse_component_section(const_content, "bootstrap_cap:") {
            vout_components.push(bootstrap);
        }
        
        // Parse output capacitors
        if const_content.contains("output_caps:") {
            // This is an array, need special handling
            if let Some(caps) = self.parse_output_caps_array(const_content) {
                vout_components.extend(caps);
            }
        }
        
        // Parse feedback network
        if const_content.contains("feedback_network:") {
            if let Some(feedback) = self.parse_feedback_network(const_content) {
                vout_components.extend(feedback);
            }
        }
        
        // Parse compensation network
        if const_content.contains("compensation:") {
            if let Some(comp) = self.parse_compensation_network(const_content) {
                vout_components.extend(comp);
            }
        }
        
        // Parse soft-start capacitor
        if let Some(soft_start) = self.parse_component_section(const_content, "soft_start:") {
            vout_components.push(soft_start);
        }
        
        if !vout_components.is_empty() {
            let vout_def = VirtualPinDefinition {
                pin_name: "VOUT".to_string(),
                description: "Virtual output - expands to complete buck converter output stage".to_string(),
                supporting_components: vout_components,
                metadata: HashMap::new(),
            };
            
            virtual_pins.add_virtual_pin("VOUT".to_string(), vout_def);
            Some(virtual_pins)
        } else {
            None
        }
    }
    
    /// Parse a single component section
    fn parse_component_section(&self, content: &str, section_name: &str) -> Option<VirtualPinComponent> {
        let section_start = content.find(section_name)?;
        let section_content = &content[section_start..];
        
        // Extract connection pattern
        let connection = self.extract_field_value(section_content, "connection:");
        let value = self.extract_field_value(section_content, "value:").or_else(|| 
            self.extract_field_value(section_content, "value_formula:"));
        
        // Determine component type from section name
        let component_type = if section_name.contains("inductor") {
            "Inductor".to_string()
        } else if section_name.contains("bootstrap") || section_name.contains("cap") {
            "Capacitor".to_string()
        } else if section_name.contains("r_") {
            "Resistor".to_string()
        } else {
            return None;
        };
        
        // Generate reference based on component type
        let reference = if component_type == "Inductor" {
            "L1".to_string()
        } else if section_name.contains("bootstrap") {
            "C_BOOT".to_string()
        } else if section_name.contains("soft_start") {
            "C_SS".to_string()
        } else {
            section_name.trim_end_matches(':').to_uppercase()
        };
        
        Some(VirtualPinComponent {
            reference,
            component_type,
            connection_pattern: connection.unwrap_or_default(),
            value: value.unwrap_or("calculated".to_string()),
            formula: self.extract_field_value(section_content, "value_formula:"),
            specs: HashMap::new(),
            placement: self.extract_field_value(section_content, "placement:"),
            intent: self.extract_field_value(section_content, "purpose:").or_else(||
                self.extract_field_value(section_content, "intent:")),
        })
    }
    
    /// Extract field value from BHDL const content
    fn extract_field_value(&self, content: &str, field_name: &str) -> Option<String> {
        let field_start = content.find(field_name)?;
        let after_field = &content[field_start + field_name.len()..];
        
        // Skip whitespace
        let value_start = after_field.trim_start();
        
        // Find the end of the value (comma or closing brace)
        if value_start.starts_with('"') {
            // String value
            let end_quote = value_start[1..].find('"')?;
            Some(value_start[1..end_quote + 1].to_string())
        } else {
            // Non-string value, find comma or newline
            let end_pos = value_start.find(',').or_else(|| value_start.find('\n'))?;
            Some(value_start[..end_pos].trim().to_string())
        }
    }
    
    /// Parse output capacitors array
    fn parse_output_caps_array(&self, content: &str) -> Option<Vec<VirtualPinComponent>> {
        let mut caps = Vec::new();
        
        // Simple heuristic: look for multiple capacitor definitions
        let caps_start = content.find("output_caps:")?;
        let caps_section = &content[caps_start..];
        
        // For now, create standard output caps based on common patterns
        caps.push(VirtualPinComponent {
            reference: "C_OUT1".to_string(),
            component_type: "Capacitor".to_string(),
            connection_pattern: "VOUT -> self.1; self.2 -> GND".to_string(),
            value: "22µF".to_string(),
            formula: None,
            specs: HashMap::from([
                ("voltage_rating".to_string(), "VOUT × 2".to_string()),
                ("type".to_string(), "Ceramic X7R".to_string()),
            ]),
            placement: Some("close_to_output".to_string()),
            intent: Some("output_filtering".to_string()),
        });
        
        caps.push(VirtualPinComponent {
            reference: "C_OUT2".to_string(),
            component_type: "Capacitor".to_string(),
            connection_pattern: "VOUT -> self.1; self.2 -> GND".to_string(),
            value: "100nF".to_string(),
            formula: None,
            specs: HashMap::from([
                ("voltage_rating".to_string(), "VOUT × 2".to_string()),
                ("type".to_string(), "Ceramic X7R".to_string()),
            ]),
            placement: Some("close_to_output".to_string()),
            intent: Some("high_frequency_bypass".to_string()),
        });
        
        Some(caps)
    }
    
    /// Parse feedback network components
    fn parse_feedback_network(&self, content: &str) -> Option<Vec<VirtualPinComponent>> {
        let mut components = Vec::new();
        
        // R_FB1 (top)
        components.push(VirtualPinComponent {
            reference: "R_FB1".to_string(),
            component_type: "Resistor".to_string(),
            connection_pattern: "VOUT -> self.1; self.2 -> FB".to_string(),
            value: "calculated".to_string(),
            formula: Some("R2 × (Vout/0.8 - 1)".to_string()),
            specs: HashMap::from([
                ("tolerance".to_string(), "1%".to_string()),
            ]),
            placement: Some("close_to_ic".to_string()),
            intent: Some("feedback_control".to_string()),
        });
        
        // R_FB2 (bottom)
        components.push(VirtualPinComponent {
            reference: "R_FB2".to_string(),
            component_type: "Resistor".to_string(),
            connection_pattern: "FB -> self.1; self.2 -> GND".to_string(),
            value: "10kΩ".to_string(),
            formula: None,
            specs: HashMap::from([
                ("tolerance".to_string(), "1%".to_string()),
            ]),
            placement: Some("close_to_ic".to_string()),
            intent: Some("feedback_reference".to_string()),
        });
        
        Some(components)
    }
    
    /// Parse compensation network components
    fn parse_compensation_network(&self, content: &str) -> Option<Vec<VirtualPinComponent>> {
        let mut components = Vec::new();
        
        components.push(VirtualPinComponent {
            reference: "R_COMP".to_string(),
            component_type: "Resistor".to_string(),
            connection_pattern: "COMP -> self.1; self.2 -> C_COMP_NODE".to_string(),
            value: "10kΩ".to_string(),
            formula: None,
            specs: HashMap::new(),
            placement: Some("close_to_ic".to_string()),
            intent: Some("compensation".to_string()),
        });
        
        components.push(VirtualPinComponent {
            reference: "C_COMP1".to_string(),
            component_type: "Capacitor".to_string(),
            connection_pattern: "C_COMP_NODE -> self.1; self.2 -> GND".to_string(),
            value: "4.7nF".to_string(),
            formula: None,
            specs: HashMap::new(),
            placement: Some("close_to_ic".to_string()),
            intent: Some("compensation".to_string()),
        });
        
        components.push(VirtualPinComponent {
            reference: "C_COMP2".to_string(),
            component_type: "Capacitor".to_string(),
            connection_pattern: "COMP -> self.1; self.2 -> GND".to_string(),
            value: "47pF".to_string(),
            formula: None,
            specs: HashMap::new(),
            placement: Some("close_to_ic".to_string()),
            intent: Some("compensation".to_string()),
        });
        
        Some(components)
    }
    
    /// Get virtual pin supporting components for a component type
    pub fn get_virtual_pin_components(&self, component_type: &str) -> Option<ComponentVirtualPins> {
        // First try to parse from actual BHDL file
        if let Some(virtual_pins) = self.try_parse_virtual_pins_from_bhdl(component_type) {
            return Some(virtual_pins);
        }
        
        // Fallback to hardcoded for now (will be removed once all components have BHDL definitions)
        let mut virtual_pins = ComponentVirtualPins::new(component_type.to_string());
        
        // Hardcoded for TPS54331 as an example
        if component_type == "TPS54331" {
            let mut vout_components = Vec::new();
            
            // Add inductor
            vout_components.push(VirtualPinComponent {
                reference: "L1".to_string(),
                component_type: "Inductor".to_string(),
                connection_pattern: "SW -> self.1; self.2 -> VOUT".to_string(),
                value: "22µH".to_string(),
                formula: None,
                specs: HashMap::from([
                    ("current_rating".to_string(), "2A".to_string()),
                    ("dcr_max".to_string(), "50mΩ".to_string()),
                ]),
                placement: Some("close_to_ic".to_string()),
                intent: Some("energy_storage(frequency: 570kHz)".to_string()),
            });
            
            // Add output capacitors
            vout_components.push(VirtualPinComponent {
                reference: "C_OUT1".to_string(),
                component_type: "Capacitor".to_string(),
                connection_pattern: "VOUT -> self.1; self.2 -> GND".to_string(),
                value: "22µF".to_string(),
                formula: None,
                specs: HashMap::from([
                    ("voltage_rating".to_string(), "10V".to_string()),
                    ("dielectric".to_string(), "ceramic".to_string()),
                ]),
                placement: Some("close_to_output".to_string()),
                intent: Some("output_filtering(esr_target: low)".to_string()),
            });
            
            // Add feedback resistors
            vout_components.push(VirtualPinComponent {
                reference: "R_FB1".to_string(),
                component_type: "Resistor".to_string(),
                connection_pattern: "VOUT -> self.1; self.2 -> FB".to_string(),
                value: "calculated".to_string(),
                formula: Some("R2 × (Vout/0.8 - 1)".to_string()),
                specs: HashMap::from([
                    ("tolerance".to_string(), "1%".to_string()),
                    ("power_rating".to_string(), "100mW".to_string()),
                ]),
                placement: Some("close_to_ic".to_string()),
                intent: Some("feedback_control(target_voltage: vout)".to_string()),
            });
            
            let vout_def = VirtualPinDefinition {
                pin_name: "VOUT".to_string(),
                description: "Virtual output - expands to inductor, capacitors, and feedback network".to_string(),
                supporting_components: vout_components,
                metadata: HashMap::new(),
            };
            
            virtual_pins.add_virtual_pin("VOUT".to_string(), vout_def);
        }
        
        // Hardcoded for LM7805
        else if component_type == "LM7805" {
            let mut vout_components = Vec::new();
            
            vout_components.push(VirtualPinComponent {
                reference: "C_OUT".to_string(),
                component_type: "Capacitor".to_string(),
                connection_pattern: "VOUT -> self.1; self.2 -> GND".to_string(),
                value: "100µF".to_string(),
                formula: None,
                specs: HashMap::from([
                    ("voltage_rating".to_string(), "10V".to_string()),
                ]),
                placement: Some("close_to_ic".to_string()),
                intent: Some("output_stabilization(ripple_reduction: 60dB)".to_string()),
            });
            
            vout_components.push(VirtualPinComponent {
                reference: "C_BYPASS".to_string(),
                component_type: "Capacitor".to_string(),
                connection_pattern: "VOUT -> self.1; self.2 -> GND".to_string(),
                value: "100nF".to_string(),
                formula: None,
                specs: HashMap::from([
                    ("voltage_rating".to_string(), "10V".to_string()),
                ]),
                placement: Some("very_close_to_ic".to_string()),
                intent: Some("high_frequency_bypass(frequency: 1MHz)".to_string()),
            });
            
            let vout_def = VirtualPinDefinition {
                pin_name: "VOUT".to_string(),
                description: "Virtual output - expands to output capacitors".to_string(),
                supporting_components: vout_components,
                metadata: HashMap::new(),
            };
            
            virtual_pins.add_virtual_pin("VOUT".to_string(), vout_def);
        }
        
        if virtual_pins.virtual_pins.is_empty() {
            None
        } else {
            Some(virtual_pins)
        }
    }
    
    /// Try to parse virtual pins from actual BHDL file
    fn try_parse_virtual_pins_from_bhdl(&self, component_type: &str) -> Option<ComponentVirtualPins> {
        // Search for the component BHDL file in the stdlib directory
        // This is more generic - it will find any component file
        let component_file = self.find_component_bhdl_file(component_type)?;
        
        // Read and parse the BHDL file
        if let Ok(content) = fs::read_to_string(&component_file) {
            return self.parse_virtual_pin_expansion(&content, component_type);
        }
        
        None
    }
    
    /// Find a component's BHDL file in the stdlib directory
    fn find_component_bhdl_file(&self, component_type: &str) -> Option<std::path::PathBuf> {
        use std::fs;
        
        // Common locations to search for component files
        let search_paths = vec![
            format!("components/power/switching_regulators/{}.bhdl", component_type),
            format!("components/power/linear_regulators/{}.bhdl", component_type),
            format!("components/power/protection/{}.bhdl", component_type),
            format!("components/passives/resistors/{}.bhdl", component_type),
            format!("components/passives/capacitors/{}.bhdl", component_type),
            format!("components/passives/inductors/{}.bhdl", component_type),
            format!("components/connectors/{}.bhdl", component_type),
            format!("components/mcu/{}.bhdl", component_type),
            format!("passives/{}.bhdl", component_type.to_lowercase()),
            format!("actives/{}.bhdl", component_type.to_lowercase()),
            format!("regulators/{}.bhdl", component_type.to_lowercase()),
            format!("power/{}.bhdl", component_type.to_lowercase()),
            format!("connectors/{}.bhdl", component_type.to_lowercase()),
        ];
        
        for relative_path in search_paths {
            let full_path = Path::new(&self.stdlib_path).join(&relative_path);
            if full_path.exists() {
                debug!("Found component file for {}: {}", component_type, full_path.display());
                return Some(full_path);
            }
        }
        
        // Try case-insensitive search as a fallback
        let type_lower = component_type.to_lowercase();
        if type_lower != component_type {
            return self.find_component_bhdl_file(&type_lower);
        }
        
        debug!("No BHDL file found for component type: {}", component_type);
        None
    }
}

/// Get the default stdlib path.
///
/// Prefers `bhdl-stdlib` relative to the current directory (deployed layouts,
/// running from the workspace root), falling back to this crate's source
/// directory so test binaries — which cargo runs with the package dir as CWD —
/// resolve the stdlib too.
pub fn get_default_stdlib_path() -> String {
    let relative = std::path::Path::new("bhdl-stdlib");
    if relative.is_dir() {
        return "bhdl-stdlib".to_string();
    }
    env!("CARGO_MANIFEST_DIR").to_string()
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