//! Safety rules implementations

mod current_limiting;
mod overvoltage;
mod short_circuit;

pub use current_limiting::CurrentLimitingRule;
pub use overvoltage::OvervoltageRule;
pub use short_circuit::ShortCircuitRule;

// Utility functions shared by rules

use crate::circuit::{Circuit, ComponentId, NodeId};
use std::collections::HashSet;


/// Trace path from component to power supply
pub(crate) fn trace_to_supply(
    circuit: &Circuit, 
    component_id: ComponentId
) -> Option<Vec<ComponentId>> {
    // Simple path tracing - would be more complex in real implementation
    let mut path = Vec::new();
    
    // Get starting node from component
    let component = circuit.get_component(component_id)?;
    let nodes = component.nodes();
    if nodes.is_empty() {
        return None;
    }
    
    let mut current_node = nodes[0];
    let mut visited = HashSet::new();
    
    while !circuit.is_supply_node(current_node) && visited.insert(current_node) {
        // Find components that could lead to supply
        let components = circuit.get_components_at_node(current_node);
        
        // Find the next component in the path
        let next_comp = components.iter()
            .find(|&&comp_id| !path.contains(&comp_id))
            .copied();
        
        if let Some(comp_id) = next_comp {
            path.push(comp_id);
            
            // Get the other node of this component
            if let Some(component) = circuit.get_component(comp_id) {
                let nodes = component.nodes();
                current_node = *nodes.iter()
                    .find(|&&n| n != current_node)
                    .unwrap_or(&current_node);
            } else {
                break;
            }
        } else {
            break;
        }
    }
    
    if circuit.is_supply_node(current_node) {
        Some(path)
    } else {
        None
    }
}

/// Round a resistance value to the nearest E12 series value
pub(crate) fn round_to_e12(value: f64) -> f64 {
    const E12_BASE: [f64; 12] = [
        1.0, 1.2, 1.5, 1.8, 2.2, 2.7, 3.3, 3.9, 4.7, 5.6, 6.8, 8.2
    ];
    
    if value <= 0.0 {
        return 1.0;
    }
    
    // Find the decade multiplier
    let decades = value.log10().floor();
    let multiplier = 10_f64.powf(decades);
    let normalized = value / multiplier;
    
    // Find nearest E12 value
    let nearest = E12_BASE.iter()
        .min_by(|&&a, &&b| {
            (a - normalized).abs().partial_cmp(&(b - normalized).abs()).unwrap()
        })
        .unwrap();
    
    nearest * multiplier
}

/// Get the next higher E12 series value
pub(crate) fn next_higher_e12(value: f64) -> f64 {
    const E12_BASE: [f64; 12] = [
        1.0, 1.2, 1.5, 1.8, 2.2, 2.7, 3.3, 3.9, 4.7, 5.6, 6.8, 8.2
    ];
    
    if value <= 0.0 {
        return 1.0;
    }
    
    // Find the decade multiplier
    let decades = value.log10().floor();
    let multiplier = 10_f64.powf(decades);
    let normalized = value / multiplier;
    
    // Find next higher E12 value
    let next_higher = E12_BASE.iter()
        .find(|&&v| v > normalized)
        .unwrap_or(&10.0);
    
    if *next_higher == 10.0 {
        1.0 * multiplier * 10.0
    } else {
        next_higher * multiplier
    }
}

/// Format a value in engineering notation
pub(crate) fn format_engineering(value: f64) -> String {
    if value >= 1e9 {
        format!("{:.1}G", value / 1e9)
    } else if value >= 1e6 {
        format!("{:.1}M", value / 1e6)
    } else if value >= 1e3 {
        format!("{:.1}k", value / 1e3)
    } else if value >= 1.0 {
        format!("{:.1}", value)
    } else if value >= 1e-3 {
        format!("{:.1}m", value * 1e3)
    } else if value >= 1e-6 {
        format!("{:.1}µ", value * 1e6)
    } else if value >= 1e-9 {
        format!("{:.1}n", value * 1e9)
    } else {
        format!("{:.1}p", value * 1e12)
    }
}