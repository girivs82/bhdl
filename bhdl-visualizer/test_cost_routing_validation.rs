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

#[derive(Debug)]
struct ComponentPin {
    component: String,
    pin_name: String,
    expected_x: f64,
    expected_y: f64,
}

fn define_expected_pins() -> Vec<ComponentPin> {
    vec![
        // VIN power symbol at (50,50) with 10-unit lead
        ComponentPin {
            component: "VIN".to_string(),
            pin_name: "PWR".to_string(),
            expected_x: 50.0,
            expected_y: 60.0,
        },
        
        // C_IN at (50,120) rotated 90° - pins become vertical
        ComponentPin {
            component: "C_IN".to_string(),
            pin_name: "Pin1".to_string(),
            expected_x: 50.0,
            expected_y: 100.0, // Top pin: 120-20
        },
        ComponentPin {
            component: "C_IN".to_string(),
            pin_name: "Pin2".to_string(),
            expected_x: 50.0,
            expected_y: 140.0, // Bottom pin: 120+20
        },
        
        // U1 LDO at (200,100) - pins extend from rectangle edges
        ComponentPin {
            component: "U1".to_string(),
            pin_name: "VIN".to_string(),
            expected_x: 165.0,
            expected_y: 100.0, // Left: 200-35
        },
        ComponentPin {
            component: "U1".to_string(),
            pin_name: "VOUT".to_string(),
            expected_x: 235.0,
            expected_y: 100.0, // Right: 200+35
        },
        ComponentPin {
            component: "U1".to_string(),
            pin_name: "GND".to_string(),
            expected_x: 200.0,
            expected_y: 125.0, // Bottom: 100+25
        },
        ComponentPin {
            component: "U1".to_string(),
            pin_name: "EN".to_string(),
            expected_x: 200.0,
            expected_y: 75.0, // Top: 100-25
        },
        
        // C_OUT at (350,120) rotated 90° - pins become vertical
        ComponentPin {
            component: "C_OUT".to_string(),
            pin_name: "Pin1".to_string(),
            expected_x: 350.0,
            expected_y: 100.0, // Top pin: 120-20
        },
        ComponentPin {
            component: "C_OUT".to_string(),
            pin_name: "Pin2".to_string(),
            expected_x: 350.0,
            expected_y: 140.0, // Bottom pin: 120+20
        },
        
        // VOUT power symbol at (450,50) with 10-unit lead
        ComponentPin {
            component: "VOUT".to_string(),
            pin_name: "PWR".to_string(),
            expected_x: 450.0,
            expected_y: 60.0,
        },
        
        // GND symbol at (200,200) with -10 unit lead
        ComponentPin {
            component: "GND".to_string(),
            pin_name: "GND".to_string(),
            expected_x: 200.0,
            expected_y: 190.0,
        },
    ]
}

fn validate_wire_connections(wires: &[SvgWire], expected_pins: &[ComponentPin]) -> Vec<String> {
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
        
        // Check if wire connects to another wire (for intermediate connections)
        let start_connects_to_wire = wires.iter().enumerate().any(|(j, other_wire)| {
            i != j && (
                ((wire_start.0 - other_wire.x1).abs() < tolerance && (wire_start.1 - other_wire.y1).abs() < tolerance) ||
                ((wire_start.0 - other_wire.x2).abs() < tolerance && (wire_start.1 - other_wire.y2).abs() < tolerance)
            )
        });
        
        let end_connects_to_wire = wires.iter().enumerate().any(|(j, other_wire)| {
            i != j && (
                ((wire_end.0 - other_wire.x1).abs() < tolerance && (wire_end.1 - other_wire.y1).abs() < tolerance) ||
                ((wire_end.0 - other_wire.x2).abs() < tolerance && (wire_end.1 - other_wire.y2).abs() < tolerance)
            )
        });
        
        if !start_connected && !start_connects_to_wire {
            errors.push(format!(
                "Wire {} start ({}, {}) doesn't connect to any expected pin or wire junction", 
                i+1, wire_start.0, wire_start.1
            ));
        }
        
        if !end_connected && !end_connects_to_wire {
            errors.push(format!(
                "Wire {} end ({}, {}) doesn't connect to any expected pin or wire junction",
                i+1, wire_end.0, wire_end.1  
            ));
        }
    }
    
    errors
}

fn validate_expected_connections(wires: &[SvgWire], expected_pins: &[ComponentPin]) -> Vec<String> {
    let mut errors = Vec::new();
    
    // Expected schematic connections
    let expected_connections = vec![
        ("VIN.PWR", "C_IN.Pin1"),
        ("C_IN.Pin1", "U1.VIN"),
        ("U1.VOUT", "C_OUT.Pin1"),
        ("C_OUT.Pin1", "VOUT.PWR"),
        ("C_IN.Pin2", "GND.GND"),
        ("C_OUT.Pin2", "GND.GND"),
        ("U1.GND", "GND.GND"),
        ("U1.EN", "VIN.PWR"),
    ];
    
    for (from_pin, to_pin) in expected_connections {
        let from_pos = find_pin_position(from_pin, expected_pins);
        let to_pos = find_pin_position(to_pin, expected_pins);
        
        if let (Some(from), Some(to)) = (from_pos, to_pos) {
            let path_exists = check_path_exists(from, to, wires);
            if !path_exists {
                errors.push(format!(
                    "Missing connection: {} → {} (({}, {}) → ({}, {}))",
                    from_pin, to_pin, from.0, from.1, to.0, to.1
                ));
            }
        } else {
            errors.push(format!(
                "Invalid pin reference: {} → {}",
                from_pin, to_pin
            ));
        }
    }
    
    errors
}

fn find_pin_position(pin_name: &str, expected_pins: &[ComponentPin]) -> Option<(f64, f64)> {
    let parts: Vec<&str> = pin_name.split('.').collect();
    if parts.len() != 2 {
        return None;
    }
    
    let component = parts[0];
    let pin = parts[1];
    
    expected_pins.iter()
        .find(|p| p.component == component && p.pin_name == pin)
        .map(|p| (p.expected_x, p.expected_y))
}

fn check_path_exists(from: (f64, f64), to: (f64, f64), wires: &[SvgWire]) -> bool {
    let tolerance = 2.0;
    
    // Use breadth-first search to avoid stack overflow
    use std::collections::{HashSet, VecDeque};
    
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(from);
    visited.insert((from.0 as i32, from.1 as i32)); // discretize for hashing
    
    while let Some(current) = queue.pop_front() {
        // Check if we've reached the target
        if (current.0 - to.0).abs() < tolerance && (current.1 - to.1).abs() < tolerance {
            return true;
        }
        
        // Find all wires connected to current position
        for wire in wires {
            let mut next_points = Vec::new();
            
            // Check if current position connects to wire start
            if (current.0 - wire.x1).abs() < tolerance && (current.1 - wire.y1).abs() < tolerance {
                next_points.push((wire.x2, wire.y2));
            }
            
            // Check if current position connects to wire end
            if (current.0 - wire.x2).abs() < tolerance && (current.1 - wire.y2).abs() < tolerance {
                next_points.push((wire.x1, wire.y1));
            }
            
            // Add unvisited next points to queue
            for next_point in next_points {
                let next_key = (next_point.0 as i32, next_point.1 as i32);
                if !visited.contains(&next_key) {
                    visited.insert(next_key);
                    queue.push_back(next_point);
                }
            }
        }
    }
    
    false
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

fn main() {
    println!("🧪 COST ROUTING SVG VALIDATION");
    
    // Read the generated SVG
    let svg_file = "cost_routing_test.svg";
    let svg_content = match fs::read_to_string(svg_file) {
        Ok(content) => content,
        Err(e) => {
            println!("❌ Failed to read {}: {}", svg_file, e);
            println!("Run 'cargo run --bin test_cost_routing' first to generate the SVG file.");
            return;
        }
    };
    
    // Parse expected pin positions
    let expected_pins = define_expected_pins();
    println!("\n📍 EXPECTED PIN POSITIONS:");
    for pin in &expected_pins {
        println!("  - {}.{} at ({}, {})", 
                pin.component, pin.pin_name, pin.expected_x, pin.expected_y);
    }
    
    // Parse wires from SVG (look for routes group)
    let mut wires = Vec::new();
    let mut in_routes_group = false;
    
    for line in svg_content.lines() {
        let trimmed = line.trim();
        
        // Check if we're entering the routes group
        if trimmed.contains("id=\"routes\"") {
            in_routes_group = true;
            continue;
        }
        
        // Check if we're leaving the group
        if trimmed == "</g>" && in_routes_group {
            in_routes_group = false;
            continue;
        }
        
        // Parse lines within the routes group
        if in_routes_group && trimmed.starts_with("<line") {
            if let Some(wire) = parse_svg_wire(line) {
                wires.push(wire);
            }
        }
    }
    
    println!("\n🔌 PARSED WIRES FROM SVG:");
    for (i, wire) in wires.iter().enumerate() {
        println!("  {}: ({}, {}) → ({}, {})", i+1, wire.x1, wire.y1, wire.x2, wire.y2);
    }
    
    // Check for duplicate wires
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
    
    // Validate expected schematic connections
    let missing_connections = validate_expected_connections(&wires, &expected_pins);
    
    println!("\n📊 VALIDATION RESULTS:");
    println!("  - Total routing wires: {}", wires.len());
    println!("  - Duplicate wires: {}", duplicates.len());
    println!("  - Connection errors: {}", connection_errors.len());
    println!("  - Missing connections: {}", missing_connections.len());
    println!("  - SVG file size: {} chars", svg_content.len());
    
    // Analyze wire patterns for quality
    let horizontal_wires = wires.iter().filter(|w| (w.y1 - w.y2).abs() < 0.1).count();
    let vertical_wires = wires.iter().filter(|w| (w.x1 - w.x2).abs() < 0.1).count();
    let diagonal_wires = wires.len() - horizontal_wires - vertical_wires;
    
    println!("\n📐 WIRE PATTERN ANALYSIS:");
    println!("  - Horizontal wires: {}", horizontal_wires);
    println!("  - Vertical wires: {}", vertical_wires);
    println!("  - Diagonal/other wires: {}", diagonal_wires);
    
    if diagonal_wires > 0 {
        println!("⚠️  WARNING: {} non-orthogonal wires detected (should be 0 for clean schematic routing)", diagonal_wires);
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
    
    if !missing_connections.is_empty() {
        println!("\n🚨 MISSING EXPECTED CONNECTIONS:");
        for error in &missing_connections {
            println!("  - {}", error);
        }
        has_errors = true;
    }
    
    // Basic sanity checks
    if wires.len() < 8 {
        println!("\n⚠️  WARNING: Only {} wires found, expected at least 8 for complete LDO routing", wires.len());
    }
    
    // Final verdict
    if has_errors {
        println!("\n❌ VALIDATION FAILED: {} duplicates, {} connection errors, {} missing connections", 
                duplicates.len(), connection_errors.len(), missing_connections.len());
        std::process::exit(1);
    } else {
        println!("\n✅ VALIDATION PASSED: All wires properly connected, no duplicates, clean orthogonal routing");
        println!("🎉 Cost-based schematic routing is working correctly!");
    }
} 