//! Unified component type definitions for BHDL toolchain
//! 
//! This module provides canonical component type names and mappings to ensure
//! consistency across all BHDL crates (parser, analyzer, synthesizer, visualizer).

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Canonical component type names used throughout the BHDL toolchain
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComponentType {
    /// Passive resistor
    Resistor,
    /// Passive capacitor  
    Capacitor,
    /// Passive inductor
    Inductor,
    /// Light emitting diode
    LED,
    /// General purpose diode
    Diode,
    /// Bipolar junction transistor
    BJT,
    /// Field effect transistor
    FET,
    /// Operational amplifier
    OpAmp,
    /// Voltage regulator
    VoltageRegulator,
    /// Crystal oscillator
    Crystal,
    /// Connector/header
    Connector,
    /// Power supply/source
    PowerSupply,
    /// Ground reference
    Ground,
    /// Generic integrated circuit
    IC,
    /// Unknown/custom component
    Unknown(String),
}

impl ComponentType {
    /// Get the canonical string representation
    pub fn as_str(&self) -> &str {
        match self {
            ComponentType::Resistor => "Resistor",
            ComponentType::Capacitor => "Capacitor", 
            ComponentType::Inductor => "Inductor",
            ComponentType::LED => "LED",
            ComponentType::Diode => "Diode",
            ComponentType::BJT => "BJT",
            ComponentType::FET => "FET",
            ComponentType::OpAmp => "OpAmp",
            ComponentType::VoltageRegulator => "VoltageRegulator",
            ComponentType::Crystal => "Crystal",
            ComponentType::Connector => "Connector",
            ComponentType::PowerSupply => "PowerSupply",
            ComponentType::Ground => "Ground",
            ComponentType::IC => "IC",
            ComponentType::Unknown(s) => s,
        }
    }
    
    /// Get the reference designator prefix for this component type
    pub fn reference_designator_prefix(&self) -> &str {
        match self {
            ComponentType::Resistor => "R",
            ComponentType::Capacitor => "C",
            ComponentType::Inductor => "L",
            ComponentType::LED => "LED",
            ComponentType::Diode => "D",
            ComponentType::BJT | ComponentType::FET => "Q",
            ComponentType::OpAmp | ComponentType::VoltageRegulator | ComponentType::IC => "U",
            ComponentType::Crystal => "Y",
            ComponentType::Connector => "J",
            ComponentType::PowerSupply => "PS",
            ComponentType::Ground => "GND",
            ComponentType::Unknown(_) => "U",
        }
    }
    
    /// Parse a component type from a string using fuzzy matching
    pub fn from_str(s: &str) -> ComponentType {
        let s_lower = s.to_lowercase();
        
        // Direct matches first
        match s {
            "Resistor" => ComponentType::Resistor,
            "Capacitor" => ComponentType::Capacitor,
            "Inductor" => ComponentType::Inductor,
            "LED" => ComponentType::LED,
            "Diode" => ComponentType::Diode,
            "BJT" => ComponentType::BJT,
            "FET" => ComponentType::FET,
            "OpAmp" => ComponentType::OpAmp,
            "VoltageRegulator" => ComponentType::VoltageRegulator,
            "Crystal" => ComponentType::Crystal,
            "Connector" => ComponentType::Connector,
            "PowerSupply" => ComponentType::PowerSupply,
            "Ground" => ComponentType::Ground,
            "IC" => ComponentType::IC,
            _ => {
                // Fuzzy matching for common BHDL aliases and variations
                if s_lower == "res" || s_lower.contains("resistor") {
                    ComponentType::Resistor
                } else if s_lower == "cap" || s_lower.contains("capacitor") {
                    ComponentType::Capacitor
                } else if s_lower == "ind" || s_lower.contains("inductor") {
                    ComponentType::Inductor
                } else if s_lower.contains("led") {
                    ComponentType::LED
                } else if s_lower.contains("diode") {
                    ComponentType::Diode
                } else if s_lower.contains("transistor") || s_lower.contains("bjt") {
                    ComponentType::BJT
                } else if s_lower.contains("fet") || s_lower.contains("mosfet") {
                    ComponentType::FET
                } else if s_lower.contains("opamp") || s_lower.contains("amplifier") {
                    ComponentType::OpAmp
                } else if s_lower.contains("regulator") || s_lower.contains("lm78") || s_lower.contains("lm79") || s_lower == "lm7805" {
                    ComponentType::VoltageRegulator
                } else if s_lower.contains("crystal") || s_lower.contains("xtal") {
                    ComponentType::Crystal
                } else if s_lower.contains("connector") || s_lower.contains("header") {
                    ComponentType::Connector
                } else if s_lower.contains("power") || s_lower.contains("supply") || s_lower.contains("battery") {
                    ComponentType::PowerSupply
                } else if s_lower.contains("ground") || s_lower.contains("gnd") {
                    ComponentType::Ground
                } else if s_lower.contains("ic") {
                    ComponentType::IC
                } else {
                    ComponentType::Unknown(s.to_string())
                }
            }
        }
    }
    
    /// Check if this is a passive component
    pub fn is_passive(&self) -> bool {
        matches!(self, ComponentType::Resistor | ComponentType::Capacitor | ComponentType::Inductor)
    }
    
    /// Check if this is a semiconductor
    pub fn is_semiconductor(&self) -> bool {
        matches!(self, ComponentType::LED | ComponentType::Diode | ComponentType::BJT | ComponentType::FET)
    }
    
    /// Check if this is an active component
    pub fn is_active(&self) -> bool {
        matches!(self, ComponentType::OpAmp | ComponentType::VoltageRegulator | ComponentType::IC)
    }
}

impl std::fmt::Display for ComponentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<&str> for ComponentType {
    fn from(s: &str) -> Self {
        ComponentType::from_str(s)
    }
}

impl From<String> for ComponentType {
    fn from(s: String) -> Self {
        ComponentType::from_str(&s)
    }
}

/// Component type mapper for legacy string-based type systems
pub struct ComponentTypeMapper {
    /// BHDL type -> Canonical type mappings
    bhdl_mappings: HashMap<String, ComponentType>,
    /// Symbol type -> Canonical type mappings  
    symbol_mappings: HashMap<String, ComponentType>,
}

impl ComponentTypeMapper {
    /// Create a new component type mapper with default mappings
    pub fn new() -> Self {
        let mut mapper = Self {
            bhdl_mappings: HashMap::new(),
            symbol_mappings: HashMap::new(),
        };
        
        // Initialize default BHDL type mappings
        mapper.add_bhdl_mapping("Res", ComponentType::Resistor);
        mapper.add_bhdl_mapping("Resistor", ComponentType::Resistor);
        mapper.add_bhdl_mapping("Cap", ComponentType::Capacitor);
        mapper.add_bhdl_mapping("Capacitor", ComponentType::Capacitor);
        mapper.add_bhdl_mapping("Ind", ComponentType::Inductor);
        mapper.add_bhdl_mapping("Inductor", ComponentType::Inductor);
        mapper.add_bhdl_mapping("LED", ComponentType::LED);
        mapper.add_bhdl_mapping("Diode", ComponentType::Diode);
        mapper.add_bhdl_mapping("BJT", ComponentType::BJT);
        mapper.add_bhdl_mapping("FET", ComponentType::FET);
        mapper.add_bhdl_mapping("OpAmp", ComponentType::OpAmp);
        mapper.add_bhdl_mapping("LM7805", ComponentType::VoltageRegulator);
        mapper.add_bhdl_mapping("LM317", ComponentType::VoltageRegulator);
        mapper.add_bhdl_mapping("VoltageRegulator", ComponentType::VoltageRegulator);
        
        // Initialize symbol type mappings
        mapper.add_symbol_mapping("R", ComponentType::Resistor);
        mapper.add_symbol_mapping("C", ComponentType::Capacitor);
        mapper.add_symbol_mapping("L", ComponentType::Inductor);
        mapper.add_symbol_mapping("LED", ComponentType::LED);
        mapper.add_symbol_mapping("D", ComponentType::Diode);
        mapper.add_symbol_mapping("Q", ComponentType::BJT);
        mapper.add_symbol_mapping("U", ComponentType::IC);
        mapper.add_symbol_mapping("Y", ComponentType::Crystal);
        mapper.add_symbol_mapping("J", ComponentType::Connector);
        
        mapper
    }
    
    /// Add a BHDL type mapping
    pub fn add_bhdl_mapping(&mut self, bhdl_type: &str, canonical_type: ComponentType) {
        self.bhdl_mappings.insert(bhdl_type.to_string(), canonical_type);
    }
    
    /// Add a symbol type mapping
    pub fn add_symbol_mapping(&mut self, symbol_type: &str, canonical_type: ComponentType) {
        self.symbol_mappings.insert(symbol_type.to_string(), canonical_type);
    }
    
    /// Map a BHDL type to canonical type
    pub fn map_bhdl_type(&self, bhdl_type: &str) -> ComponentType {
        self.bhdl_mappings.get(bhdl_type)
            .cloned()
            .unwrap_or_else(|| ComponentType::from_str(bhdl_type))
    }
    
    /// Map a symbol type to canonical type
    pub fn map_symbol_type(&self, symbol_type: &str) -> ComponentType {
        self.symbol_mappings.get(symbol_type)
            .cloned()
            .unwrap_or_else(|| ComponentType::from_str(symbol_type))
    }
    
    /// Get reference designator prefix for a BHDL type
    pub fn get_refdes_prefix(&self, bhdl_type: &str) -> String {
        self.map_bhdl_type(bhdl_type).reference_designator_prefix().to_string()
    }
    
    /// Get canonical type name for a BHDL type
    pub fn get_canonical_name(&self, bhdl_type: &str) -> String {
        self.map_bhdl_type(bhdl_type).as_str().to_string()
    }
}

impl Default for ComponentTypeMapper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_component_type_parsing() {
        assert_eq!(ComponentType::from_str("Res"), ComponentType::Resistor);
        assert_eq!(ComponentType::from_str("LED"), ComponentType::LED);
        assert_eq!(ComponentType::from_str("LM7805"), ComponentType::VoltageRegulator);
        assert_eq!(ComponentType::from_str("capacitor"), ComponentType::Capacitor);
    }
    
    #[test]
    fn test_reference_designators() {
        assert_eq!(ComponentType::Resistor.reference_designator_prefix(), "R");
        assert_eq!(ComponentType::LED.reference_designator_prefix(), "LED");
        assert_eq!(ComponentType::VoltageRegulator.reference_designator_prefix(), "U");
        assert_eq!(ComponentType::Capacitor.reference_designator_prefix(), "C");
    }
    
    #[test]
    fn test_component_categories() {
        assert!(ComponentType::Resistor.is_passive());
        assert!(ComponentType::LED.is_semiconductor());
        assert!(ComponentType::VoltageRegulator.is_active());
        assert!(!ComponentType::LED.is_passive());
    }
    
    #[test]
    fn test_type_mapper() {
        let mapper = ComponentTypeMapper::new();
        
        assert_eq!(mapper.map_bhdl_type("Res"), ComponentType::Resistor);
        assert_eq!(mapper.map_symbol_type("R"), ComponentType::Resistor);
        assert_eq!(mapper.get_refdes_prefix("Res"), "R");
        assert_eq!(mapper.get_refdes_prefix("LED"), "LED");
    }
}