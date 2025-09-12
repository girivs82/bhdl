/// Virtual pin support for BHDL components
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

/// Represents a supporting component defined in a virtual pin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualPinComponent {
    /// Component reference (e.g., "C_OUT", "R1", "D1")
    pub reference: String,
    
    /// Component type (e.g., "Capacitor", "Resistor", "Diode")
    pub component_type: String,
    
    /// Connection pattern (e.g., "VOUT -> self.1; self.2 -> GND")
    pub connection_pattern: String,
    
    /// Component value (e.g., "100µF", "240Ω", "calculated")
    pub value: String,
    
    /// Optional value calculation formula
    pub formula: Option<String>,
    
    /// Component specifications
    pub specs: HashMap<String, String>,
    
    /// Placement hint (e.g., "close_to_ic", "very_close_to_ic")
    pub placement: Option<String>,
    
    /// Design intent (e.g., "output_stabilization(ripple_reduction: 60dB)")
    pub intent: Option<String>,
}

/// Represents a virtual pin definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualPinDefinition {
    /// Pin name (e.g., "VOUT", "GND")
    pub pin_name: String,
    
    /// Description of the virtual pin
    pub description: String,
    
    /// Supporting components that implement this virtual pin
    pub supporting_components: Vec<VirtualPinComponent>,
    
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Collection of virtual pins for a component
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComponentVirtualPins {
    /// Component name (e.g., "TPS54331", "LM7805")
    pub component_name: String,
    
    /// Map of pin name to virtual pin definition
    pub virtual_pins: HashMap<String, VirtualPinDefinition>,
}

impl ComponentVirtualPins {
    /// Create a new component virtual pins collection
    pub fn new(component_name: String) -> Self {
        Self {
            component_name,
            virtual_pins: HashMap::new(),
        }
    }
    
    /// Add a virtual pin definition
    pub fn add_virtual_pin(&mut self, pin_name: String, definition: VirtualPinDefinition) {
        self.virtual_pins.insert(pin_name, definition);
    }
    
    /// Get all supporting components for all virtual pins
    pub fn get_all_supporting_components(&self) -> Vec<VirtualPinComponent> {
        self.virtual_pins
            .values()
            .flat_map(|vp| vp.supporting_components.iter().cloned())
            .collect()
    }
    
    /// Get supporting components for a specific virtual pin
    pub fn get_supporting_components(&self, pin_name: &str) -> Option<Vec<VirtualPinComponent>> {
        self.virtual_pins
            .get(pin_name)
            .map(|vp| vp.supporting_components.clone())
    }
}