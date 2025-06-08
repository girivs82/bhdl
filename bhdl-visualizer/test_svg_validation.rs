use std::fs;

#[derive(Debug, PartialEq, Clone)]
struct SvgWire {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
}

fn parse_svg_wire(line: &str) -> Option<SvgWire> {
    let x1 = extract_attribute(line, "x1=")?;
    let y1 = extract_attribute(line, "y1=")?;
    let x2 = extract_attribute(line, "x2=")?;
    let y2 = extract_attribute(line, "y2=")?;
    
    Some(SvgWire { x1, y1, x2, y2 })
}

fn extract_attribute(line: &str, attr: &str) -> Option<f64> {
    if let Some(start) = line.find(attr) {
        let start = start + attr.len();
        if line.chars().nth(start) == Some('"') {
            let start = start + 1;
            if let Some(end) = line[start..].find('"') {
                return line[start..start + end].parse().ok();
            }
        }
    }
    None
}

fn wires_equal(w1: &SvgWire, w2: &SvgWire) -> bool {
    let tolerance = 0.1;
    
    // Check both directions
    let forward = (w1.x1 - w2.x1).abs() < tolerance &&
                 (w1.y1 - w2.y1).abs() < tolerance &&
                 (w1.x2 - w2.x2).abs() < tolerance &&
                 (w1.y2 - w2.y2).abs() < tolerance;
                 
    let reverse = (w1.x1 - w2.x2).abs() < tolerance &&
                 (w1.y1 - w2.y2).abs() < tolerance &&
                 (w1.x2 - w2.x1).abs() < tolerance &&
                 (w1.y2 - w2.y1).abs() < tolerance;
                 
    forward || reverse
}

#[derive(Debug)]
struct Component {
    name: String,
    x: f64,
    y: f64,
    rotation: f64,
    component_type: String,
}

#[derive(Debug)]
struct ExpectedPin {
    component: String,
    pin_name: String,
    expected_x: f64,
    expected_y: f64,
}

fn parse_components(svg_content: &str) -> Vec<Component> {
    let mut components = Vec::new();
    let lines: Vec<&str> = svg_content.lines().collect();
    
    for (i, line) in lines.iter().enumerate() {
        if line.trim().starts_with("<g transform=\"translate(") {
            if let Some((x, y, rotation)) = parse_transform(line) {
                // Look for component name and type in subsequent lines
                let mut name = "Unknown".to_string();
                let mut component_type = "Unknown".to_string();
                
                for j in (i+1)..(i+15).min(lines.len()) {
                    let text_line = lines[j];
                    if text_line.contains("<text") && text_line.contains(">") && text_line.contains("</text>") {
                        if let Some(text_content) = extract_text_from_line(text_line) {
                            if !text_content.is_empty() && text_content.len() < 20 {
                                name = text_content;
                                break;
                            }
                        }
                    }
                }
                
                // Determine component type from SVG structure
                for j in (i+1)..(i+20).min(lines.len()) {
                    let structure_line = lines[j];
                    if structure_line.contains("VoltageRegulator") {
                        component_type = "VoltageRegulator".to_string();
                        break;
                    } else if structure_line.contains("x1=\"-12\"") && structure_line.contains("x2=\"-2\"") {
                        component_type = "Capacitor".to_string();
                        break;
                    } else if structure_line.contains("path d=\"M-5,-4 L5,-4 L0,-19 z\"") {
                        component_type = "Power".to_string();
                        break;
                    } else if structure_line.contains("x1=\"-10\" x2=\"10\"") && structure_line.contains("y1=\"4\" y2=\"4\"") {
                        component_type = "Ground".to_string();
                        break;
                    }
                }
                
                components.push(Component { name, x, y, rotation, component_type });
            }
        }
    }
    
    components
}

fn parse_transform(line: &str) -> Option<(f64, f64, f64)> {
    if let Some(start) = line.find("translate(") {
        let start = start + 10;
        if let Some(end) = line[start..].find(')') {
            let coords = &line[start..start + end];
            let parts: Vec<&str> = coords.split_whitespace().collect();
            if parts.len() >= 2 {
                if let (Ok(x), Ok(y)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                    // Extract rotation if present
                    let rotation = if line.contains("rotate(") {
                        extract_rotation_angle(line).unwrap_or(0.0)
                    } else {
                        0.0
                    };
                    return Some((x, y, rotation));
                }
            }
        }
    }
    None
}

fn extract_rotation_angle(line: &str) -> Option<f64> {
    if let Some(start) = line.find("rotate(") {
        let start = start + 7;
        if let Some(end) = line[start..].find(' ') {
            return line[start..start + end].parse().ok();
        }
    }
    None
}

fn extract_text_from_line(line: &str) -> Option<String> {
    if let Some(start) = line.find('>') {
        if let Some(end) = line[start + 1..].find('<') {
            let content = &line[start + 1..start + 1 + end];
            return Some(content.trim().to_string());
        }
    }
    None
}

fn calculate_expected_pins(components: &[Component]) -> Vec<ExpectedPin> {
    let mut expected_pins = Vec::new();
    
    for comp in components {
        match comp.component_type.as_str() {
            "VoltageRegulator" => {
                // LDO pins based on the SVG structure: VIN(left), VOUT(right), GND(bottom)
                expected_pins.push(ExpectedPin {
                    component: comp.name.clone(),
                    pin_name: "VIN".to_string(),
                    expected_x: comp.x - 30.0,
                    expected_y: comp.y,
                });
                expected_pins.push(ExpectedPin {
                    component: comp.name.clone(), 
                    pin_name: "VOUT".to_string(),
                    expected_x: comp.x + 30.0,
                    expected_y: comp.y,
                });
                expected_pins.push(ExpectedPin {
                    component: comp.name.clone(),
                    pin_name: "GND".to_string(), 
                    expected_x: comp.x,
                    expected_y: comp.y + 40.0,
                });
            },
            "Capacitor" => {
                // Capacitor rotated 90°: pins at top and bottom
                if comp.rotation == 90.0 {
                    expected_pins.push(ExpectedPin {
                        component: comp.name.clone(),
                        pin_name: "Pin1".to_string(),
                        expected_x: comp.x,
                        expected_y: comp.y - 12.0, // Top pin
                    });
                    expected_pins.push(ExpectedPin {
                        component: comp.name.clone(),
                        pin_name: "Pin2".to_string(), 
                        expected_x: comp.x,
                        expected_y: comp.y + 12.0, // Bottom pin
                    });
                } else {
                    expected_pins.push(ExpectedPin {
                        component: comp.name.clone(),
                        pin_name: "Pin1".to_string(),
                        expected_x: comp.x - 12.0,
                        expected_y: comp.y,
                    });
                    expected_pins.push(ExpectedPin {
                        component: comp.name.clone(),
                        pin_name: "Pin2".to_string(),
                        expected_x: comp.x + 12.0,
                        expected_y: comp.y,
                    });
                }
            },
            "Power" => {
                expected_pins.push(ExpectedPin {
                    component: comp.name.clone(),
                    pin_name: "PWR".to_string(),
                    expected_x: comp.x,
                    expected_y: comp.y, // Pin at center for power symbols
                });
            },
            "Ground" => {
                expected_pins.push(ExpectedPin {
                    component: comp.name.clone(),
                    pin_name: "GND".to_string(),
                    expected_x: comp.x,
                    expected_y: comp.y, // Pin at center for ground symbols  
                });
            },
            _ => {}
        }
    }
    
    expected_pins
}

fn validate_wire_connections(wires: &[SvgWire], expected_pins: &[ExpectedPin]) -> Vec<String> {
    let mut errors = Vec::new();
    let tolerance = 2.0; // Allow some tolerance for connection points
    
    for (i, wire) in wires.iter().enumerate() {
        let wire_start = (wire.x1, wire.y1);
        let wire_end = (wire.x2, wire.y2);
        
        // Check if wire start connects to an expected pin
        let start_connected = expected_pins.iter().any(|pin| {
            (wire_start.0 - pin.expected_x).abs() < tolerance &&
            (wire_start.1 - pin.expected_y).abs() < tolerance
        });
        
        // Check if wire end connects to an expected pin  
        let end_connected = expected_pins.iter().any(|pin| {
            (wire_end.0 - pin.expected_x).abs() < tolerance &&
            (wire_end.1 - pin.expected_y).abs() < tolerance
        });
        
        if !start_connected {
            errors.push(format!(
                "Wire {} start ({}, {}) doesn't connect to any expected pin", 
                i+1, wire_start.0, wire_start.1
            ));
        }
        
        if !end_connected {
            errors.push(format!(
                "Wire {} end ({}, {}) doesn't connect to any expected pin",
                i+1, wire_end.0, wire_end.1  
            ));
        }
    }
    
    errors
}

fn main() {
    println!("🧪 ENHANCED SVG VALIDATION WITH PIN VERIFICATION");
    
    // Read the most recently generated SVG
    let svg_file = "complete_ldo_schematic.svg";
    let svg_content = match fs::read_to_string(svg_file) {
        Ok(content) => content,
        Err(e) => {
            println!("❌ Failed to read {}: {}", svg_file, e);
            println!("Run a test first to generate the SVG file.");
            return;
        }
    };
    
    // Parse components from SVG
    let components = parse_components(&svg_content);
    println!("\n🏗️  COMPONENTS FOUND:");
    for comp in &components {
        println!("  - {} ({}) at ({}, {}) rotation: {}°", 
                comp.name, comp.component_type, comp.x, comp.y, comp.rotation);
    }
    
    // Calculate expected pin positions
    let expected_pins = calculate_expected_pins(&components);
    println!("\n📍 EXPECTED PIN POSITIONS:");
    for pin in &expected_pins {
        println!("  - {}.{} at ({}, {})", 
                pin.component, pin.pin_name, pin.expected_x, pin.expected_y);
    }
    
    // Parse wires from SVG
    let mut wires = Vec::new();
    let mut in_blue_group = false;
    
    for line in svg_content.lines() {
        let trimmed = line.trim();
        
        // Check if we're entering a blue stroke group
        if trimmed.starts_with("<g") && (trimmed.contains("stroke=\"blue\"") || trimmed.contains("id=\"nets\"")) {
            in_blue_group = true;
            continue;
        }
        
        // Check if we're leaving the group
        if trimmed == "</g>" && in_blue_group {
            in_blue_group = false;
            continue;
        }
        
        // Parse lines within the blue group
        if in_blue_group && trimmed.starts_with("<line") {
            if let Some(wire) = parse_svg_wire(line) {
                wires.push(wire);
            }
        }
    }
    
    // Check for duplicates
    let mut duplicates = Vec::new();
    for i in 0..wires.len() {
        for j in (i + 1)..wires.len() {
            if wires_equal(&wires[i], &wires[j]) {
                duplicates.push(format!("Wire {}: ({}, {}) → ({}, {})", i+1, wires[i].x1, wires[i].y1, wires[i].x2, wires[i].y2));
            }
        }
    }
    
    // Validate wire connections to pins
    let connection_errors = validate_wire_connections(&wires, &expected_pins);
    
    println!("\n📊 SVG Analysis Results:");
    println!("  - Total routing wires: {}", wires.len());
    println!("  - Duplicate wires: {}", duplicates.len());
    println!("  - Connection errors: {}", connection_errors.len());
    println!("  - SVG file size: {} chars", svg_content.len());
    
    println!("\n🔌 ALL ROUTING WIRES:");
    for (i, wire) in wires.iter().enumerate() {
        println!("  {}: ({}, {}) → ({}, {})", i+1, wire.x1, wire.y1, wire.x2, wire.y2);
    }
    
    // Report all errors
    let mut has_errors = false;
    
    if !duplicates.is_empty() {
        println!("\n🚨 DUPLICATE WIRES DETECTED:");
        for dup in &duplicates {
            println!("  - {}", dup);
        }
        has_errors = true;
    }
    
    if !connection_errors.is_empty() {
        println!("\n🚨 WIRE CONNECTION ERRORS:");
        for error in &connection_errors {
            println!("  - {}", error);
        }
        has_errors = true;
    }
    
    // Basic sanity checks
    if wires.len() < 6 {
        println!("\n⚠️  WARNING: Only {} wires found, expected at least 6 for complete LDO routing", wires.len());
    }
    
    // Analyze wire patterns for quality
    let horizontal_wires = wires.iter().filter(|w| (w.y1 - w.y2).abs() < 0.1).count();
    let vertical_wires = wires.iter().filter(|w| (w.x1 - w.x2).abs() < 0.1).count();
    let diagonal_wires = wires.len() - horizontal_wires - vertical_wires;
    
    println!("\n📐 WIRE ANALYSIS:");
    println!("  - Horizontal wires: {}", horizontal_wires);
    println!("  - Vertical wires: {}", vertical_wires);
    println!("  - Diagonal/other wires: {}", diagonal_wires);
    
    if diagonal_wires > 0 {
        println!("⚠️  WARNING: {} non-orthogonal wires detected (should be 0 for clean routing)", diagonal_wires);
    }
    
    // Final verdict
    if has_errors {
        println!("\n❌ VALIDATION FAILED: {} duplicate wires, {} connection errors", 
                duplicates.len(), connection_errors.len());
        std::process::exit(1);
    } else {
        println!("\n✅ VALIDATION PASSED: No errors detected!");
    }
    
    println!("\n🎯 Enhanced SVG validation complete!");
} 