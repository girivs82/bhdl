//! Component symbol management with database integration

use std::collections::HashMap;
use anyhow::{Result, Context};
use log::{debug, warn};
use bhdl_common::{ComponentType, ComponentTypeMapper};

use bhdl_synthesizer::DatabaseComponentInstance;
use crate::types::{Point, Component};
use bhdl_netlist::InstanceId;

/// Symbol manager that integrates with the database component system
pub struct SymbolManager {
    /// Cache of parsed symbol data
    symbol_cache: HashMap<String, ParsedSymbol>,
    /// Default symbol generators for fallback
    fallback_generators: HashMap<String, Box<dyn Fn() -> ParsedSymbol + Send + Sync>>,
    /// Unified component type mapper
    type_mapper: ComponentTypeMapper,
}

/// Parsed symbol information extracted from SVG or generated programmatically
#[derive(Debug, Clone)]
pub struct ParsedSymbol {
    /// SVG content for the symbol
    pub svg_content: String,
    /// Pin positions relative to symbol center
    pub pin_positions: HashMap<String, Point>,
    /// Symbol bounding box (width, height)
    pub size: Point,
    /// Anchor point (usually center)
    pub anchor: Point,
}

impl SymbolManager {
    /// Create a new symbol manager
    pub fn new() -> Self {
        let mut manager = Self {
            symbol_cache: HashMap::new(),
            fallback_generators: HashMap::new(),
            type_mapper: ComponentTypeMapper::new(),
        };
        
        // Register fallback generators for common components
        manager.register_fallback_generators();
        manager
    }
    
    /// Register default fallback symbol generators using unified component types
    fn register_fallback_generators(&mut self) {
        // Use canonical component types for consistent mapping
        self.fallback_generators.insert(ComponentType::Resistor.as_str().to_string(), Box::new(|| {
            generate_resistor_symbol()
        }));
        
        self.fallback_generators.insert(ComponentType::Capacitor.as_str().to_string(), Box::new(|| {
            generate_capacitor_symbol()
        }));
        
        self.fallback_generators.insert(ComponentType::LED.as_str().to_string(), Box::new(|| {
            generate_led_symbol()
        }));
        
        self.fallback_generators.insert(ComponentType::VoltageRegulator.as_str().to_string(), Box::new(|| {
            generate_regulator_symbol()
        }));
        
        self.fallback_generators.insert(ComponentType::IC.as_str().to_string(), Box::new(|| {
            generate_ic_symbol()
        }));
        
        // Legacy type support for backwards compatibility
        self.fallback_generators.insert("Res".to_string(), Box::new(|| {
            generate_resistor_symbol()
        }));
        
        self.fallback_generators.insert("LM7805".to_string(), Box::new(|| {
            generate_regulator_symbol()
        }));
    }
    
    /// Get or generate symbol for a database component instance
    pub async fn get_symbol(&mut self, component_instance: &DatabaseComponentInstance) -> Result<ParsedSymbol> {
        let cache_key = format!("{}:{}", component_instance.bhdl_type, component_instance.component_name);
        
        // Check cache first
        if let Some(cached_symbol) = self.symbol_cache.get(&cache_key) {
            debug!("Symbol cache hit for {}", cache_key);
            return Ok(cached_symbol.clone());
        }
        
        // Try to parse SVG data from database
        let symbol = if !component_instance.svg_data.is_empty() {
            debug!("Parsing SVG symbol from database for {}", cache_key);
            match self.parse_svg_symbol(&component_instance.svg_data, &component_instance.pins) {
                Ok(symbol) => symbol,
                Err(e) => {
                    warn!("Failed to parse SVG symbol for {}: {}", cache_key, e);
                    self.generate_fallback_symbol(&component_instance.bhdl_type)?
                }
            }
        } else {
            debug!("No SVG data, generating fallback symbol for {} (bhdl_type: {}, instance: {})", 
                   cache_key, component_instance.bhdl_type, component_instance.instance_name);
            self.generate_fallback_symbol(&component_instance.bhdl_type)?
        };
        
        // Cache the result
        self.symbol_cache.insert(cache_key, symbol.clone());
        Ok(symbol)
    }
    
    /// Parse SVG symbol data and extract pin positions
    fn parse_svg_symbol(&self, svg_data: &str, pins: &[bhdl_components::types::PinDefinition]) -> Result<ParsedSymbol> {
        // For now, implement a simple SVG parser
        // In a full implementation, you'd use a proper SVG parsing library
        
        // Extract viewBox or use default size
        let size = self.extract_svg_size(svg_data).unwrap_or(Point::new(40.0, 20.0));
        
        // Map pins to positions based on pin definitions
        let mut pin_positions = HashMap::new();
        for (i, pin) in pins.iter().enumerate() {
            let pin_name = pin.pin_name.as_ref()
                .unwrap_or(&pin.pin_number)
                .clone();
            
            // Simple pin positioning logic - distribute around perimeter
            let pin_pos = match pins.len() {
                2 => {
                    // Two pins - left and right
                    if i == 0 {
                        Point::new(-size.x / 2.0, 0.0)
                    } else {
                        Point::new(size.x / 2.0, 0.0)
                    }
                }
                3 => {
                    // Three pins - common for TO-220 packages (like LM7805)
                    match i {
                        0 => Point::new(-size.x / 2.0, 0.0), // Input
                        1 => Point::new(0.0, size.y / 2.0),  // Ground
                        2 => Point::new(size.x / 2.0, 0.0),  // Output
                        _ => Point::new(0.0, 0.0),
                    }
                }
                _ => {
                    // Distribute around perimeter
                    let angle = 2.0 * std::f64::consts::PI * i as f64 / pins.len() as f64;
                    Point::new(
                        (size.x / 2.0) * angle.cos(),
                        (size.y / 2.0) * angle.sin()
                    )
                }
            };
            
            pin_positions.insert(pin_name, pin_pos);
        }
        
        Ok(ParsedSymbol {
            svg_content: svg_data.to_string(),
            pin_positions,
            size,
            anchor: Point::new(0.0, 0.0), // Center anchor
        })
    }
    
    /// Extract size from SVG viewBox or default dimensions
    fn extract_svg_size(&self, svg_data: &str) -> Option<Point> {
        // Simple regex-based extraction of viewBox
        if let Some(captures) = regex::Regex::new(r#"viewBox="[^"]*\s+([0-9.]+)\s+([0-9.]+)""#)
            .ok()?.captures(svg_data) 
        {
            let width: f64 = captures.get(1)?.as_str().parse().ok()?;
            let height: f64 = captures.get(2)?.as_str().parse().ok()?;
            Some(Point::new(width, height))
        } else if let Some(width_cap) = regex::Regex::new(r#"width="([0-9.]+)""#).ok()?.captures(svg_data) {
            if let Some(height_cap) = regex::Regex::new(r#"height="([0-9.]+)""#).ok()?.captures(svg_data) {
                let width: f64 = width_cap.get(1)?.as_str().parse().ok()?;
                let height: f64 = height_cap.get(1)?.as_str().parse().ok()?;
                Some(Point::new(width, height))
            } else {
                None
            }
        } else {
            None
        }
    }
    
    /// Generate fallback symbol for unknown components using unified type system
    fn generate_fallback_symbol(&self, component_type: &str) -> Result<ParsedSymbol> {
        // First, map the BHDL type to canonical type
        let canonical_type = self.type_mapper.map_bhdl_type(component_type);
        let canonical_name = canonical_type.as_str();
        
        // Try canonical component type first
        if let Some(generator) = self.fallback_generators.get(canonical_name) {
            debug!("Using canonical fallback generator for {} -> {}", component_type, canonical_name);
            return Ok(generator());
        }
        
        // Try original component type for legacy support
        if let Some(generator) = self.fallback_generators.get(component_type) {
            debug!("Using legacy fallback generator for {}", component_type);
            return Ok(generator());
        }
        
        // Generate symbol based on canonical type
        let symbol = match canonical_type {
            ComponentType::Resistor => generate_resistor_symbol(),
            ComponentType::Capacitor => generate_capacitor_symbol(),
            ComponentType::Inductor => generate_inductor_symbol(),
            ComponentType::LED => generate_led_symbol(),
            ComponentType::Diode => generate_diode_symbol(),
            ComponentType::VoltageRegulator => generate_regulator_symbol(),
            ComponentType::OpAmp | ComponentType::IC => generate_ic_symbol(),
            _ => {
                debug!("Unknown component type '{}' mapped to '{}', using generic IC symbol", 
                       component_type, canonical_name);
                generate_ic_symbol()
            }
        };
        
        Ok(symbol)
    }
    
    /// Create a visual component from a database component instance
    pub async fn create_component(
        &mut self,
        instance_id: InstanceId,
        component_instance: &DatabaseComponentInstance,
        position: Point,
        rotation: f64,
    ) -> Result<Component> {
        let symbol = self.get_symbol(component_instance).await
            .context("Failed to get symbol for component")?;
        
        let mut component = Component::new(instance_id, position)
            .with_svg(symbol.svg_content)
            .with_rotation(rotation)
            .with_size(symbol.size.x, symbol.size.y);
        
        // Set pin positions from symbol
        component.pins = symbol.pin_positions;
        
        Ok(component)
    }
    
    /// Get cached symbol count for debugging
    pub fn cache_size(&self) -> usize {
        self.symbol_cache.len()
    }
    
    /// Clear symbol cache
    pub fn clear_cache(&mut self) {
        self.symbol_cache.clear();
    }
}

impl Default for SymbolManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a resistor symbol
fn generate_resistor_symbol() -> ParsedSymbol {
    let svg = r#"
    <g>
        <rect x="-15" y="-5" width="30" height="10" fill="white" stroke="black" stroke-width="1"/>
        <line x1="-20" y1="0" x2="-15" y2="0" stroke="black" stroke-width="1"/>
        <line x1="15" y1="0" x2="20" y2="0" stroke="black" stroke-width="1"/>
    </g>"#;
    
    let mut pins = HashMap::new();
    pins.insert("1".to_string(), Point::new(-20.0, 0.0));
    pins.insert("2".to_string(), Point::new(20.0, 0.0));
    
    ParsedSymbol {
        svg_content: svg.to_string(),
        pin_positions: pins,
        size: Point::new(40.0, 10.0),
        anchor: Point::new(0.0, 0.0),
    }
}

/// Generate an inductor symbol
fn generate_inductor_symbol() -> ParsedSymbol {
    let svg = r#"
    <g>
        <path d="M -15,0 Q -10,0 -5,-5 Q 0,5 5,-5 Q 10,5 15,0" fill="none" stroke="black" stroke-width="2"/>
        <line x1="-20" y1="0" x2="-15" y2="0" stroke="black" stroke-width="1"/>
        <line x1="15" y1="0" x2="20" y2="0" stroke="black" stroke-width="1"/>
    </g>"#;
    
    let mut pins = HashMap::new();
    pins.insert("1".to_string(), Point::new(-20.0, 0.0));
    pins.insert("2".to_string(), Point::new(20.0, 0.0));
    
    ParsedSymbol {
        svg_content: svg.to_string(),
        pin_positions: pins,
        size: Point::new(40.0, 10.0),
        anchor: Point::new(0.0, 0.0),
    }
}

/// Generate a diode symbol
fn generate_diode_symbol() -> ParsedSymbol {
    let svg = r#"
    <g>
        <polygon points="-5,-8 5,0 -5,8" fill="white" stroke="black" stroke-width="1"/>
        <line x1="5" y1="-8" x2="5" y2="8" stroke="black" stroke-width="2"/>
        <line x1="-15" y1="0" x2="-5" y2="0" stroke="black" stroke-width="1"/>
        <line x1="5" y1="0" x2="15" y2="0" stroke="black" stroke-width="1"/>
    </g>"#;
    
    let mut pins = HashMap::new();
    pins.insert("anode".to_string(), Point::new(-15.0, 0.0));
    pins.insert("cathode".to_string(), Point::new(15.0, 0.0));
    
    ParsedSymbol {
        svg_content: svg.to_string(),
        pin_positions: pins,
        size: Point::new(30.0, 16.0),
        anchor: Point::new(0.0, 0.0),
    }
}

/// Generate a capacitor symbol  
fn generate_capacitor_symbol() -> ParsedSymbol {
    let svg = r#"
    <g>
        <line x1="-5" y1="-10" x2="-5" y2="10" stroke="black" stroke-width="2"/>
        <line x1="5" y1="-10" x2="5" y2="10" stroke="black" stroke-width="2"/>
        <line x1="-15" y1="0" x2="-5" y2="0" stroke="black" stroke-width="1"/>
        <line x1="5" y1="0" x2="15" y2="0" stroke="black" stroke-width="1"/>
    </g>"#;
    
    let mut pins = HashMap::new();
    pins.insert("1".to_string(), Point::new(-15.0, 0.0));
    pins.insert("2".to_string(), Point::new(15.0, 0.0));
    
    ParsedSymbol {
        svg_content: svg.to_string(),
        pin_positions: pins,
        size: Point::new(30.0, 20.0),
        anchor: Point::new(0.0, 0.0),
    }
}

/// Generate an LED symbol
fn generate_led_symbol() -> ParsedSymbol {
    let svg = r#"
    <g>
        <polygon points="-5,-8 5,0 -5,8" fill="white" stroke="black" stroke-width="1"/>
        <line x1="5" y1="-8" x2="5" y2="8" stroke="black" stroke-width="2"/>
        <line x1="-15" y1="0" x2="-5" y2="0" stroke="black" stroke-width="1"/>
        <line x1="5" y1="0" x2="15" y2="0" stroke="black" stroke-width="1"/>
        <!-- Light rays -->
        <line x1="8" y1="-5" x2="12" y2="-9" stroke="orange" stroke-width="1"/>
        <line x1="8" y1="-2" x2="12" y2="-6" stroke="orange" stroke-width="1"/>
    </g>"#;
    
    let mut pins = HashMap::new();
    pins.insert("anode".to_string(), Point::new(-15.0, 0.0));
    pins.insert("cathode".to_string(), Point::new(15.0, 0.0));
    
    ParsedSymbol {
        svg_content: svg.to_string(),
        pin_positions: pins,
        size: Point::new(30.0, 16.0),
        anchor: Point::new(0.0, 0.0),
    }
}

/// Generate a voltage regulator symbol (TO-220 package)
fn generate_regulator_symbol() -> ParsedSymbol {
    let svg = r#"
    <g>
        <rect x="-15" y="-10" width="30" height="20" fill="white" stroke="black" stroke-width="1"/>
        <text x="0" y="2" text-anchor="middle" font-size="8" fill="black">REG</text>
        <line x1="-20" y1="0" x2="-15" y2="0" stroke="black" stroke-width="1"/>
        <line x1="15" y1="0" x2="20" y2="0" stroke="black" stroke-width="1"/>
        <line x1="0" y1="10" x2="0" y2="15" stroke="black" stroke-width="1"/>
    </g>"#;
    
    let mut pins = HashMap::new();
    pins.insert("input".to_string(), Point::new(-20.0, 0.0));
    pins.insert("output".to_string(), Point::new(20.0, 0.0));
    pins.insert("ground".to_string(), Point::new(0.0, 15.0));
    
    ParsedSymbol {
        svg_content: svg.to_string(),
        pin_positions: pins,
        size: Point::new(40.0, 25.0),
        anchor: Point::new(0.0, 0.0),
    }
}

/// Generate a generic IC symbol
fn generate_ic_symbol() -> ParsedSymbol {
    let svg = r#"
    <g>
        <rect x="-20" y="-15" width="40" height="30" fill="white" stroke="black" stroke-width="1"/>
        <text x="0" y="2" text-anchor="middle" font-size="8" fill="black">IC</text>
        <line x1="-25" y1="-5" x2="-20" y2="-5" stroke="black" stroke-width="1"/>
        <line x1="-25" y1="5" x2="-20" y2="5" stroke="black" stroke-width="1"/>
        <line x1="20" y1="-5" x2="25" y2="-5" stroke="black" stroke-width="1"/>
        <line x1="20" y1="5" x2="25" y2="5" stroke="black" stroke-width="1"/>
    </g>"#;
    
    let mut pins = HashMap::new();
    pins.insert("1".to_string(), Point::new(-25.0, -5.0));
    pins.insert("2".to_string(), Point::new(-25.0, 5.0));
    pins.insert("3".to_string(), Point::new(25.0, 5.0));
    pins.insert("4".to_string(), Point::new(25.0, -5.0));
    
    ParsedSymbol {
        svg_content: svg.to_string(),
        pin_positions: pins,
        size: Point::new(50.0, 30.0),
        anchor: Point::new(0.0, 0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_components::types::{PinDefinition, ElectricalSpec};
    
    #[tokio::test]
    async fn test_symbol_manager() {
        let mut manager = SymbolManager::new();
        
        // Test fallback symbol generation
        let resistor_instance = create_test_component_instance("R1", "Resistor");
        let symbol = manager.get_symbol(&resistor_instance).await.unwrap();
        
        assert!(!symbol.svg_content.is_empty());
        assert_eq!(symbol.pin_positions.len(), 2);
        assert!(symbol.pin_positions.contains_key("1"));
        assert!(symbol.pin_positions.contains_key("2"));
    }
    
    #[tokio::test] 
    async fn test_component_creation() {
        let mut manager = SymbolManager::new();
        // Create proper InstanceId using a dummy netlist
        let mut netlist = bhdl_netlist::Netlist::new();
        let module_id = netlist.add_module("TestModule".to_string(), bhdl_netlist::ModuleKind::PhysicalComponent);
        let instance_id = netlist.add_instance("test".to_string(), module_id).unwrap();
        
        let component_instance = create_test_component_instance("U1", "LM7805");
        let position = Point::new(100.0, 50.0);
        
        let component = manager.create_component(instance_id, &component_instance, position, 0.0).await.unwrap();
        
        assert_eq!(component.instance_id, instance_id);
        assert_eq!(component.position, position);
        assert!(component.svg_data.is_some());
        assert!(!component.pins.is_empty());
    }
    
    fn create_test_component_instance(instance_name: &str, bhdl_type: &str) -> DatabaseComponentInstance {
        let pins = match bhdl_type {
            "Resistor" => vec![
                PinDefinition { 
                    pin_number: "1".to_string(), 
                    pin_name: Some("1".to_string()), 
                    electrical_type: bhdl_components::types::PinType::Passive,
                    x_position: 0.0,
                    y_position: 0.0,
                    orientation: 0,
                    length: 10.0,
                    pin_shape: bhdl_components::types::PinShape::Line,
                },
                PinDefinition { 
                    pin_number: "2".to_string(), 
                    pin_name: Some("2".to_string()), 
                    electrical_type: bhdl_components::types::PinType::Passive,
                    x_position: 0.0,
                    y_position: 0.0,
                    orientation: 0,
                    length: 10.0,
                    pin_shape: bhdl_components::types::PinShape::Line,
                },
            ],
            "LM7805" => vec![
                PinDefinition { 
                    pin_number: "1".to_string(), 
                    pin_name: Some("input".to_string()), 
                    electrical_type: bhdl_components::types::PinType::Input,
                    x_position: 0.0,
                    y_position: 0.0,
                    orientation: 0,
                    length: 10.0,
                    pin_shape: bhdl_components::types::PinShape::Line,
                },
                PinDefinition { 
                    pin_number: "2".to_string(), 
                    pin_name: Some("ground".to_string()), 
                    electrical_type: bhdl_components::types::PinType::Ground,
                    x_position: 0.0,
                    y_position: 0.0,
                    orientation: 0,
                    length: 10.0,
                    pin_shape: bhdl_components::types::PinShape::Line,
                },
                PinDefinition { 
                    pin_number: "3".to_string(), 
                    pin_name: Some("output".to_string()), 
                    electrical_type: bhdl_components::types::PinType::Output,
                    x_position: 0.0,
                    y_position: 0.0,
                    orientation: 0,
                    length: 10.0,
                    pin_shape: bhdl_components::types::PinShape::Line,
                },
            ],
            _ => vec![],
        };
        
        DatabaseComponentInstance {
            instance_name: instance_name.to_string(),
            bhdl_type: bhdl_type.to_string(),
            component_id: 1,
            component_name: format!("{}_TEST", bhdl_type),
            component_description: Some("Test component".to_string()),
            svg_data: String::new(), // Will use fallback generation
            pin_mapping: HashMap::new(),
            category: bhdl_synthesizer::component_mapping::ComponentCategory::Unknown,
            electrical_specs: vec![],
            pins,
        }
    }
}