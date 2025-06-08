use std::collections::HashMap;
use crate::layout::types::Point;

/// Component symbol generation functionality
pub fn generate_component_symbol(instance_name: &str, module_name: &str, x: f64, y: f64) -> (String, HashMap<String, Point>) {
    generate_component_symbol_with_rotation(instance_name, module_name, x, y, 0.0)
}

/// Component symbol generation functionality with rotation support
pub fn generate_component_symbol_with_rotation(instance_name: &str, module_name: &str, x: f64, y: f64, rotation: f64) -> (String, HashMap<String, Point>) {
    let mut pin_locations = HashMap::new();
    
    let svg_content = match module_name {
        "VoltageRegulator" => {
            // Enhanced LDO pins placed outside component bounds
            pin_locations.insert("VIN".to_string(), Point::new(-35.0, 0.0));    // Extended outside left
            pin_locations.insert("VOUT".to_string(), Point::new(35.0, 0.0));   // Extended outside right  
            pin_locations.insert("GND".to_string(), Point::new(0.0, 25.0));    // Extended outside bottom
            pin_locations.insert("EN".to_string(), Point::new(0.0, -25.0));    // Extended outside top
            
            format!(
                "  <g id=\"{}\" transform=\"translate({},{}) rotate({})\">\\n\\\
                     <rect x=\"-25\" y=\"-15\" width=\"50\" height=\"30\" fill=\"white\" stroke=\"black\" stroke-width=\"2\" rx=\"3\"/>\\n\\\
                     <text x=\"0\" y=\"-5\" text-anchor=\"middle\" font-family=\"Arial\" font-size=\"10\" fill=\"black\">{}</text>\\n\\\
                     <text x=\"0\" y=\"8\" text-anchor=\"middle\" font-family=\"Arial\" font-size=\"8\" fill=\"gray\">LDO</text>\\n\\\
                     <!-- VIN pin -->\\n\\\
                     <line x1=\"-35\" y1=\"0\" x2=\"-25\" y2=\"0\" stroke=\"black\" stroke-width=\"1\"/>\\n\\\
                     <text x=\"-30\" y=\"-3\" text-anchor=\"middle\" font-family=\"Arial\" font-size=\"8\" fill=\"gray\">VIN</text>\\n\\\
                     <!-- VOUT pin -->\\n\\\
                     <line x1=\"25\" y1=\"0\" x2=\"35\" y2=\"0\" stroke=\"black\" stroke-width=\"1\"/>\\n\\\
                     <text x=\"30\" y=\"-3\" text-anchor=\"middle\" font-family=\"Arial\" font-size=\"8\" fill=\"gray\">VOUT</text>\\n\\\
                     <!-- GND pin -->\\n\\\
                     <line x1=\"0\" y1=\"15\" x2=\"0\" y2=\"25\" stroke=\"black\" stroke-width=\"1\"/>\\n\\\
                     <text x=\"5\" y=\"20\" text-anchor=\"start\" font-family=\"Arial\" font-size=\"8\" fill=\"gray\">GND</text>\\n\\\
                     <!-- EN pin -->\\n\\\
                     <line x1=\"0\" y1=\"-15\" x2=\"0\" y2=\"-25\" stroke=\"black\" stroke-width=\"1\"/>\\n\\\
                     <text x=\"5\" y=\"-18\" text-anchor=\"start\" font-family=\"Arial\" font-size=\"8\" fill=\"gray\">EN</text>\\n\\\
                  </g>",
                instance_name, x, y, rotation, instance_name
            )
        },
        "Capacitor" => {
            // Capacitor: pin 1 at left, pin 2 at right (standard orientation)
            // Rotation will handle visual orientation automatically
            pin_locations.insert("1".to_string(), Point::new(-20.0, 0.0));  // Left pin
            pin_locations.insert("2".to_string(), Point::new(20.0, 0.0));   // Right pin
            
            format!(
                "  <g id=\"{}\" transform=\"translate({},{}) rotate({})\">\\n\\\
                     <line x1=\"-3\" y1=\"-12\" x2=\"-3\" y2=\"12\" stroke=\"black\" stroke-width=\"2\"/>\\n\\\
                     <line x1=\"3\" y1=\"-12\" x2=\"3\" y2=\"12\" stroke=\"black\" stroke-width=\"2\"/>\\n\\\
                     <line x1=\"-20\" y1=\"0\" x2=\"-3\" y2=\"0\" stroke=\"black\" stroke-width=\"1\"/>\\n\\\
                     <line x1=\"3\" y1=\"0\" x2=\"20\" y2=\"0\" stroke=\"black\" stroke-width=\"1\"/>\\n\\\
                     <text x=\"8\" y=\"0\" text-anchor=\"start\" font-family=\"Arial\" font-size=\"10\" fill=\"black\">{}</text>\\n\\\
                  </g>",
                instance_name, x, y, rotation, instance_name
            )
        },
        "Ground" => {
            pin_locations.insert("GND".to_string(), Point::new(0.0, -10.0));
            
            format!(
                "  <g id=\"{}\" transform=\"translate({},{}) rotate({})\">\\n\\\
                     <line x1=\"0\" y1=\"-10\" x2=\"0\" y2=\"0\" stroke=\"black\" stroke-width=\"2\"/>\\n\\\
                     <line x1=\"-8\" y1=\"0\" x2=\"8\" y2=\"0\" stroke=\"black\" stroke-width=\"3\"/>\\n\\\
                     <line x1=\"-5\" y1=\"3\" x2=\"5\" y2=\"3\" stroke=\"black\" stroke-width=\"2\"/>\\n\\\
                     <line x1=\"-2\" y1=\"6\" x2=\"2\" y2=\"6\" stroke=\"black\" stroke-width=\"1\"/>\\n\\\
                     <text x=\"12\" y=\"0\" text-anchor=\"start\" font-family=\"Arial\" font-size=\"10\" fill=\"black\">{}</text>\\n\\\
                  </g>",
                instance_name, x, y, rotation, instance_name
            )
        },
        "Power" => {
            pin_locations.insert("PWR".to_string(), Point::new(0.0, 10.0));
            
            format!(
                "  <g id=\"{}\" transform=\"translate({},{}) rotate({})\">\\n\\\
                     <line x1=\"0\" y1=\"0\" x2=\"0\" y2=\"10\" stroke=\"black\" stroke-width=\"2\"/>\\n\\\
                     <circle cx=\"0\" cy=\"0\" r=\"6\" fill=\"white\" stroke=\"black\" stroke-width=\"2\"/>\\n\\\
                     <text x=\"0\" y=\"-2\" text-anchor=\"middle\" font-family=\"Arial\" font-size=\"8\" fill=\"black\">+</text>\\n\\\
                     <text x=\"12\" y=\"0\" text-anchor=\"start\" font-family=\"Arial\" font-size=\"10\" fill=\"black\">{}</text>\\n\\\
                  </g>",
                instance_name, x, y, rotation, instance_name
            )
        },
        _ => {
            pin_locations.insert("1".to_string(), Point::new(-10.0, 0.0));
            pin_locations.insert("2".to_string(), Point::new(10.0, 0.0));
            
            format!(
                "  <g id=\"{}\" transform=\"translate({},{}) rotate({})\">\\n\\\
                     <rect x=\"-10\" y=\"-5\" width=\"20\" height=\"10\" fill=\"lightgray\" stroke=\"black\" stroke-width=\"1\"/>\\n\\\
                     <line x1=\"-15\" y1=\"0\" x2=\"-10\" y2=\"0\" stroke=\"black\" stroke-width=\"1\"/>\\n\\\
                     <line x1=\"10\" y1=\"0\" x2=\"15\" y2=\"0\" stroke=\"black\" stroke-width=\"1\"/>\\n\\\
                     <text x=\"0\" y=\"2\" text-anchor=\"middle\" font-family=\"Arial\" font-size=\"8\" fill=\"black\">{}</text>\\n\\\
                  </g>",
                instance_name, x, y, rotation, instance_name
            )
        }
    };
    
    (svg_content, pin_locations)
} 