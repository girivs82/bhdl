//! Pattern-driven schematic layout that mimics human drawing conventions
//! 
//! Each circuit pattern (voltage regulator, amplifier, etc.) has specific
//! layout rules that match how humans typically draw these circuits.

use std::collections::HashMap;
use anyhow::{Result, Context};
use log::{debug, info};

use bhdl_netlist::{Netlist, InstanceId, NetId, NetClass};
use bhdl_analyzer::types::AnalysisResult;
use crate::types::{Point, BoundingBox, Component};

/// Circuit patterns that have specific layout conventions
#[derive(Debug, Clone, PartialEq)]
pub enum CircuitPattern {
    VoltageRegulator,
    Amplifier,
    PowerSupply,
    DigitalLogic,
    MixedSignal,
    Generic,
}

/// Layout conventions for a specific pattern
pub trait PatternLayout {
    /// Identify components by their role in the circuit
    fn classify_components(&self, netlist: &Netlist, analysis: &AnalysisResult) -> ComponentRoles;
    
    /// Position components according to pattern conventions
    fn position_components(&self, roles: &ComponentRoles, netlist: &Netlist) -> HashMap<InstanceId, Point>;
    
    /// Define power rail positions
    fn get_power_rails(&self, netlist: &Netlist) -> PowerRails;
    
    /// Get preferred component orientations
    fn get_orientations(&self, roles: &ComponentRoles) -> HashMap<InstanceId, Orientation>;
}

/// Component roles in a circuit
#[derive(Debug, Default)]
pub struct ComponentRoles {
    pub power_input: Vec<InstanceId>,
    pub power_output: Vec<InstanceId>,
    pub regulators: Vec<InstanceId>,
    pub input_filters: Vec<InstanceId>,
    pub output_filters: Vec<InstanceId>,
    pub feedback: Vec<InstanceId>,
    pub indicators: Vec<InstanceId>,
    pub current_limiting: Vec<InstanceId>,
    pub protection: Vec<InstanceId>,
    pub generic: Vec<InstanceId>,
}

/// Power rail definitions
#[derive(Debug)]
pub struct PowerRails {
    pub vcc_y: f64,
    pub gnd_y: f64,
    pub vin_y: Option<f64>,
    pub other_rails: HashMap<String, f64>,
}

/// Component orientation
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Orientation {
    Horizontal,
    Vertical,
    DiagonalUp,
    DiagonalDown,
}

/// Voltage regulator pattern layout
pub struct VoltageRegulatorLayout {
    spacing: f64,
    rail_offset: f64,
}

impl VoltageRegulatorLayout {
    pub fn new() -> Self {
        Self {
            spacing: 100.0,
            rail_offset: 50.0,
        }
    }
}

impl PatternLayout for VoltageRegulatorLayout {
    fn classify_components(&self, netlist: &Netlist, analysis: &AnalysisResult) -> ComponentRoles {
        let mut roles = ComponentRoles::default();
        
        // Analyze each instance
        for (id, instance) in &netlist.instances {
            if let Some(module) = netlist.modules.get(instance.definition) {
                let name = &instance.name;
                let module_name = &module.name;
                
                // Classify based on name and type
                if module_name.contains("7805") || module_name.contains("7812") || 
                   module_name.contains("LM") || module_name.contains("regulator") {
                    roles.regulators.push(id);
                } else if name.starts_with("C") {
                    // Determine if input or output cap based on connections
                    if is_connected_to_input(&id, netlist, analysis) {
                        roles.input_filters.push(id);
                    } else {
                        roles.output_filters.push(id);
                    }
                } else if name.starts_with("R") {
                    roles.current_limiting.push(id);
                } else if name.starts_with("D") || module_name.contains("LED") {
                    roles.indicators.push(id);
                } else {
                    roles.generic.push(id);
                }
            }
        }
        
        debug!("Component roles: {} regulators, {} input filters, {} output filters, {} indicators",
               roles.regulators.len(), roles.input_filters.len(), 
               roles.output_filters.len(), roles.indicators.len());
        
        roles
    }
    
    fn position_components(&self, roles: &ComponentRoles, netlist: &Netlist) -> HashMap<InstanceId, Point> {
        let mut positions = HashMap::new();
        let mut x = self.spacing;
        
        // Center Y position for main components
        let center_y = 200.0;
        
        // 1. Input capacitors on the left (vertical)
        for (i, id) in roles.input_filters.iter().enumerate() {
            positions.insert(*id, Point {
                x: x + i as f64 * 50.0,
                y: center_y,
            });
        }
        x += roles.input_filters.len() as f64 * 50.0 + self.spacing;
        
        // 2. Regulator in the center
        if let Some(reg_id) = roles.regulators.first() {
            positions.insert(*reg_id, Point { x, y: center_y });
            x += 150.0; // Regulator width
        }
        x += self.spacing;
        
        // 3. Output capacitors (vertical)
        for (i, id) in roles.output_filters.iter().enumerate() {
            positions.insert(*id, Point {
                x: x + i as f64 * 50.0,
                y: center_y,
            });
        }
        x += roles.output_filters.len() as f64 * 50.0 + self.spacing;
        
        // 4. Current limiting resistor and LED (far right)
        if let Some(r_id) = roles.current_limiting.first() {
            positions.insert(*r_id, Point { x, y: center_y - 50.0 });
        }
        
        if let Some(led_id) = roles.indicators.first() {
            positions.insert(*led_id, Point { x, y: center_y + 50.0 });
        }
        
        positions
    }
    
    fn get_power_rails(&self, netlist: &Netlist) -> PowerRails {
        // Standard positions for voltage regulator
        PowerRails {
            vcc_y: 50.0,
            gnd_y: 350.0,
            vin_y: Some(50.0), // Same height as VCC but on the left
            other_rails: HashMap::new(),
        }
    }
    
    fn get_orientations(&self, roles: &ComponentRoles) -> HashMap<InstanceId, Orientation> {
        let mut orientations = HashMap::new();
        
        // All capacitors are vertical in voltage regulator circuits
        for id in roles.input_filters.iter().chain(roles.output_filters.iter()) {
            orientations.insert(*id, Orientation::Vertical);
        }
        
        // Resistors are vertical when in series with LEDs
        for id in &roles.current_limiting {
            orientations.insert(*id, Orientation::Vertical);
        }
        
        // LEDs are vertical
        for id in &roles.indicators {
            orientations.insert(*id, Orientation::Vertical);
        }
        
        orientations
    }
}

/// Helper function to determine if a component is connected to input power
fn is_connected_to_input(
    instance_id: &InstanceId,
    netlist: &Netlist,
    analysis: &AnalysisResult,
) -> bool {
    // Check if this component is connected to VIN or input power domain
    for (net_id, net) in &netlist.nets {
        if let Some(name) = &net.name {
            if name.contains("VIN") || name.contains("INPUT") {
                // Check if this instance is connected to this net
                for conn in &net.connections {
                    if let bhdl_netlist::ConnectionPoint::PinInstance(pin_id) = conn {
                        if let Some(pin_inst) = netlist.pin_instances.get(*pin_id) {
                            if pin_inst.instance == *instance_id {
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

/// Pattern detector that analyzes the circuit and determines its type
pub struct PatternDetector;

impl PatternDetector {
    pub fn detect_pattern(
        netlist: &Netlist,
        analysis: &AnalysisResult,
    ) -> CircuitPattern {
        // Check for voltage regulator indicators
        let has_multiple_power_domains = analysis.power_analysis.domains.len() > 2;
        let has_regulator = netlist.instances.values().any(|inst| {
            if let Some(module) = netlist.modules.get(inst.definition) {
                module.name.contains("78") || module.name.contains("LM") || 
                module.name.contains("regulator")
            } else {
                false
            }
        });
        
        if has_multiple_power_domains && has_regulator {
            debug!("Detected voltage regulator pattern");
            return CircuitPattern::VoltageRegulator;
        }
        
        // Check for amplifier patterns
        let has_opamp = netlist.instances.values().any(|inst| {
            if let Some(module) = netlist.modules.get(inst.definition) {
                module.name.contains("amp") || module.name.contains("TL") || 
                module.name.contains("NE5532")
            } else {
                false
            }
        });
        
        if has_opamp {
            debug!("Detected amplifier pattern");
            return CircuitPattern::Amplifier;
        }
        
        // Default to generic
        debug!("No specific pattern detected, using generic layout");
        CircuitPattern::Generic
    }
}

/// Get the appropriate layout engine for a circuit pattern
pub fn get_pattern_layout(pattern: CircuitPattern) -> Box<dyn PatternLayout> {
    match pattern {
        CircuitPattern::VoltageRegulator => Box::new(VoltageRegulatorLayout::new()),
        // Add other patterns as we implement them
        _ => Box::new(GenericLayout::new()),
    }
}

/// Generic layout for unrecognized patterns
struct GenericLayout {
    spacing: f64,
}

impl GenericLayout {
    fn new() -> Self {
        Self { spacing: 100.0 }
    }
}

impl PatternLayout for GenericLayout {
    fn classify_components(&self, netlist: &Netlist, _analysis: &AnalysisResult) -> ComponentRoles {
        let mut roles = ComponentRoles::default();
        // Put everything in generic category
        for id in netlist.instances.keys() {
            roles.generic.push(id);
        }
        roles
    }
    
    fn position_components(&self, roles: &ComponentRoles, _netlist: &Netlist) -> HashMap<InstanceId, Point> {
        let mut positions = HashMap::new();
        // Simple grid layout
        for (i, &id) in roles.generic.iter().enumerate() {
            let row = i / 4;
            let col = i % 4;
            positions.insert(id, Point {
                x: 100.0 + col as f64 * self.spacing,
                y: 100.0 + row as f64 * self.spacing,
            });
        }
        positions
    }
    
    fn get_power_rails(&self, _netlist: &Netlist) -> PowerRails {
        PowerRails {
            vcc_y: 50.0,
            gnd_y: 400.0,
            vin_y: None,
            other_rails: HashMap::new(),
        }
    }
    
    fn get_orientations(&self, roles: &ComponentRoles) -> HashMap<InstanceId, Orientation> {
        // Default to horizontal for all
        roles.generic.iter()
            .map(|&id| (id, Orientation::Horizontal))
            .collect()
    }
}