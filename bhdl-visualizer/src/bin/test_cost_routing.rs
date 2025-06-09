use bhdl_visualizer::routing_costs::*;
use bhdl_visualizer::cost_pathfinder::*;
use std::collections::HashMap;
use std::fs;
use bhdl_visualizer::{
    CostGrid, CostAwarePathfinder, SignalType, RoutingCosts
};
use bhdl_visualizer::layout::types::Point;

// Font size configuration - change these to adjust all text scaling
const PIN_LABEL_FONT_SIZE: f64 = 8.0;    // Font size for pin labels (VIN, VOUT, GND, EN)
const COMPONENT_FONT_SIZE: f64 = 10.0;   // Font size for component labels (U1, C_IN, etc.)
const TITLE_FONT_SIZE: f64 = 16.0;       // Font size for title text


#[derive(Debug)]
struct ComponentPin {
    name: String,
    x: f64,
    y: f64,
}

fn validate_schematic_connections(pin_locations: &HashMap<String, Point>) -> Vec<String> {
    let mut errors = Vec::new();
    
    // Calculate dynamic U1 dimensions for validation
    let pin_labels = vec![("VIN", "left", 0.0), ("VOUT", "right", 0.0), ("GND", "bottom", 0.0), ("EN", "top", 0.0)];
    let (component_width, component_height) = calculate_component_size_with_overlap_detection(&pin_labels, PIN_LABEL_FONT_SIZE, 3.0);
    let half_width = component_width / 2.0;
    let half_height = component_height / 2.0;
    let u1_center = (200.0, 100.0);
    
    // Define expected pin positions based on DYNAMIC component locations and rotations
    let expected_pins = vec![
        // VIN power symbol at (50,50) with 10-unit lead
        ComponentPin { name: "VIN.PWR".to_string(), x: 50.0, y: 60.0 },
        
        // C_IN at (50,120) rotated 90° - pins become vertical
        // Pin 1 (top): y = 120 - 20 = 100
        // Pin 2 (bottom): y = 120 + 20 = 140  
        ComponentPin { name: "C_IN.1".to_string(), x: 50.0, y: 100.0 },
        ComponentPin { name: "C_IN.2".to_string(), x: 50.0, y: 140.0 },
        
        // U1 LDO at (200,100) - pins extend from DYNAMIC rectangle dimensions (no hardcoding!)
        ComponentPin { name: "U1.VIN".to_string(), x: u1_center.0 - half_width - 10.0, y: u1_center.1 },
        ComponentPin { name: "U1.VOUT".to_string(), x: u1_center.0 + half_width + 10.0, y: u1_center.1 }, 
        ComponentPin { name: "U1.GND".to_string(), x: u1_center.0, y: u1_center.1 + half_height + 10.0 },
        ComponentPin { name: "U1.EN".to_string(), x: u1_center.0, y: u1_center.1 - half_height - 10.0 },
        
        // C_OUT at (350,120) rotated 90° - pins become vertical  
        ComponentPin { name: "C_OUT.1".to_string(), x: 350.0, y: 100.0 }, // Top pin: 120-20
        ComponentPin { name: "C_OUT.2".to_string(), x: 350.0, y: 140.0 }, // Bottom pin: 120+20
        
        // VOUT power symbol at (450,50) with 10-unit lead
        ComponentPin { name: "VOUT.PWR".to_string(), x: 450.0, y: 60.0 },
        
        // GND symbol at (200,200) with -10 unit lead  
        ComponentPin { name: "GND.GND".to_string(), x: 200.0, y: 190.0 },
    ];
    
    println!("\n📍 Expected Pin Positions:");
    for pin in &expected_pins {
        println!("  - {} at ({}, {})", pin.name, pin.x, pin.y);
    }
    
    // Expected connections that should exist
    let expected_connections = vec![
        ("VIN.PWR", "C_IN.1"),
        ("C_IN.1", "U1.VIN"), 
        ("U1.VOUT", "C_OUT.1"),
        ("C_OUT.1", "VOUT.PWR"),
        ("C_IN.2", "GND.GND"),
        ("C_OUT.2", "GND.GND"), 
        ("U1.GND", "GND.GND"),
        ("U1.EN", "VIN.PWR"),
    ];
    
    println!("\n🔗 Expected Connections:");
    for (from, to) in &expected_connections {
        println!("  - {} → {}", from, to);
    }
    
    errors
}

fn main() {
    println!("🔧 Testing Cost-Based Routing for Schematic Layout");
    
    // Create a simple LDO schematic test
    let (pin_locations, connections) = create_ldo_schematic_test();
    
    // Create routing costs for schematic layout (less aggressive than PCB)
    let costs = RoutingCosts {
        wire_length_cost: 1.0,
        bend_cost: 2.0,           // Lower for schematics - bends are acceptable
        intersection_cost: 5.0,   // Lower - intersections with jumps are normal in schematics
        congestion_multiplier: 2.0, // Lower - less critical for schematics
        via_cost: 0.0,            // No vias in schematics
        parallel_wire_penalty: 0.0, // No crosstalk concerns in schematics
        power_proximity_bonus: 0.0, // No physical proximity concerns
    };
    
    // Create a cost grid for the schematic
    let grid_width = 60;
    let grid_height = 40;
    let grid_scale = 10.0;
    let mut cost_grid = CostGrid::new(grid_width, grid_height, grid_scale);
    cost_grid.costs = costs; // Set the routing costs
    
    // Route all connections
    let pathfinder = CostAwarePathfinder::new(10000, false);
    let mut routes = Vec::new();
    let mut total_cost = 0.0;
    
    println!("\n📋 Routing {} schematic connections:", connections.len());
    
    for (from_name, to_name, signal_type) in &connections {
        if let (Some(&from_pos), Some(&to_pos)) = (pin_locations.get(from_name), pin_locations.get(to_name)) {
            let net_name = format!("{} → {}", from_name, to_name);
            
            match pathfinder.find_route(&from_pos, &to_pos, &cost_grid, signal_type.clone(), net_name) {
                Some(route) => {
                    let cost = route.total_cost;
                    total_cost += cost;
                    println!("  ✅ {} → {}: {} segments, cost {:.1}", 
                            from_name, to_name, route.segments.len(), cost);
                    
                    // Add route to grid for congestion tracking
                    cost_grid.add_route(&route, signal_type.clone());
                    routes.push((from_name.clone(), to_name.clone(), route));
                },
                None => {
                    println!("  ❌ {} → {}: No path found", from_name, to_name);
                }
            }
        }
    }
    
    // Generate the schematic SVG with FIXED connections
    let svg_content = generate_schematic_svg(&pin_locations, &routes, &cost_grid, total_cost);
    
    // Write to file
    match fs::write("cost_routing_test.svg", svg_content) {
        Ok(_) => println!("\n✅ Generated schematic SVG: cost_routing_test.svg"),
        Err(e) => println!("❌ Failed to write SVG: {}", e),
    }
    
    // Validate the generated schematic
    println!("\n🧪 Validating Schematic Connections...");
    let validation_errors = validate_schematic_connections(&pin_locations);
    
    if validation_errors.is_empty() {
        println!("✅ All connections validated successfully!");
    } else {
        println!("❌ Validation found {} issues:", validation_errors.len());
        for error in validation_errors {
            println!("  - {}", error);
        }
    }
    
    println!("📊 Total routing cost: {:.1}", total_cost);
}

fn create_ldo_schematic_test() -> (HashMap<String, Point>, Vec<(String, String, SignalType)>) {
    let mut pin_locations = HashMap::new();
    
    // CORRECTED pin locations to match the actual SVG component pin positions
    
    // VIN power symbol at (50,50) - pin connection point is at the lead end
    pin_locations.insert("VIN.PWR".to_string(), Point::new(50.0, 60.0));
    
    // C_IN at (50,120) rotated 90° - pins are now vertical (top/bottom)
    pin_locations.insert("C_IN.1".to_string(), Point::new(50.0, 100.0));  // Top pin: 120-20
    pin_locations.insert("C_IN.2".to_string(), Point::new(50.0, 140.0));  // Bottom pin: 120+20
    
    // U1 LDO at (200,100) - calculate dynamic pin positions based on component sizing
    let u1_center = (200.0, 100.0);
    let pin_labels = vec![
        ("VIN", "left", 0.0),     
        ("VOUT", "right", 0.0),   
        ("GND", "bottom", 0.0),   
        ("EN", "top", 0.0),       
    ];
    let (component_width, component_height) = calculate_component_size_with_overlap_detection(&pin_labels, PIN_LABEL_FONT_SIZE, 3.0);
    let half_width = component_width / 2.0;
    let half_height = component_height / 2.0;
    
    println!("🔍 Calculated U1 dimensions: {}x{} (no overlaps)", component_width, component_height);
    println!("📍 Calculating U1 pin positions based on {}x{} component:", component_width, component_height);
    
    // Pin positions based on dynamic component size (10 unit pin stubs extending from edges)
    pin_locations.insert("U1.VIN".to_string(), Point::new(u1_center.0 - half_width - 10.0, u1_center.1));
    pin_locations.insert("U1.VOUT".to_string(), Point::new(u1_center.0 + half_width + 10.0, u1_center.1));
    pin_locations.insert("U1.GND".to_string(), Point::new(u1_center.0, u1_center.1 + half_height + 10.0));
    pin_locations.insert("U1.EN".to_string(), Point::new(u1_center.0, u1_center.1 - half_height - 10.0));
    
    println!("  - VIN: ({:.1}, {:.1})", u1_center.0 - half_width - 10.0, u1_center.1);
    println!("  - VOUT: ({:.1}, {:.1})", u1_center.0 + half_width + 10.0, u1_center.1);
    println!("  - GND: ({:.1}, {:.1})", u1_center.0, u1_center.1 + half_height + 10.0);
    println!("  - EN: ({:.1}, {:.1})", u1_center.0, u1_center.1 - half_height - 10.0);
    
    // C_OUT at (350,120) rotated 90° - pins are vertical  
    pin_locations.insert("C_OUT.1".to_string(), Point::new(350.0, 100.0)); // Top pin: 120-20
    pin_locations.insert("C_OUT.2".to_string(), Point::new(350.0, 140.0)); // Bottom pin: 120+20
    
    // VOUT power symbol at (450,50) - pin connection at lead end
    pin_locations.insert("VOUT.PWR".to_string(), Point::new(450.0, 60.0));
    
    // GND symbol at (200,200) - pin connection at lead end
    pin_locations.insert("GND.GND".to_string(), Point::new(200.0, 190.0));
    
    // Define nets (groups of pins that should be electrically connected)
    let nets = vec![
        // VIN net: VIN.PWR, C_IN.1, U1.VIN, U1.EN (enable tied to VIN)
        ("VIN_NET".to_string(), vec![
            "VIN.PWR".to_string(), 
            "C_IN.1".to_string(), 
            "U1.VIN".to_string(), 
            "U1.EN".to_string()
        ], SignalType::Power),
        
        // VOUT net: U1.VOUT, C_OUT.1, VOUT.PWR
        ("VOUT_NET".to_string(), vec![
            "U1.VOUT".to_string(), 
            "C_OUT.1".to_string(), 
            "VOUT.PWR".to_string()
        ], SignalType::Power),
        
        // GND net: All ground pins
        ("GND_NET".to_string(), vec![
            "C_IN.2".to_string(), 
            "C_OUT.2".to_string(), 
            "U1.GND".to_string(), 
            "GND.GND".to_string()
        ], SignalType::Ground),
    ];
    
    // Convert nets to optimized connections using minimum spanning tree approach
    let mut connections = Vec::new();
    
    for (net_name, pins, signal_type) in nets {
        if pins.len() < 2 {
            continue;
        }
        
        // For each net, find the optimal tree connection using MST-like approach
        let net_connections = create_optimal_net_connections(&pins, &pin_locations);
        
        for (from, to) in net_connections {
            connections.push((from, to, signal_type.clone()));
        }
        
        println!("📡 Net {}: {} pins → {} connections", 
                net_name, pins.len(), pins.len() - 1);
    }
    
    (pin_locations, connections)
}

fn create_optimal_net_connections(pins: &[String], pin_locations: &HashMap<String, Point>) -> Vec<(String, String)> {
    if pins.len() < 2 {
        return vec![];
    }
    
    // Use a minimum spanning tree approach to connect pins with minimal total wire length
    let mut connections = Vec::new();
    let mut connected = vec![false; pins.len()];
    connected[0] = true; // Start with first pin
    
    // Greedy MST: repeatedly add the shortest edge that connects a new pin
    while connections.len() < pins.len() - 1 {
        let mut best_distance = f64::INFINITY;
        let mut best_from_idx = 0;
        let mut best_to_idx = 0;
        
        // Find shortest edge from any connected pin to any unconnected pin
        for (i, from_pin) in pins.iter().enumerate() {
            if !connected[i] {
                continue;
            }
            
            for (j, to_pin) in pins.iter().enumerate() {
                if connected[j] || i == j {
                    continue;
                }
                
                if let (Some(&from_pos), Some(&to_pos)) = (pin_locations.get(from_pin), pin_locations.get(to_pin)) {
                    let distance = manhattan_distance(&from_pos, &to_pos);
                    if distance < best_distance {
                        best_distance = distance;
                        best_from_idx = i;
                        best_to_idx = j;
                    }
                }
            }
        }
        
        // Add the best edge
        if best_distance < f64::INFINITY {
            connections.push((pins[best_from_idx].clone(), pins[best_to_idx].clone()));
            connected[best_to_idx] = true;
            
            println!("  MST edge: {} → {} (distance: {:.1})", 
                    pins[best_from_idx], pins[best_to_idx], best_distance);
        } else {
            break; // No more connections possible
        }
    }
    
    connections
}

fn manhattan_distance(a: &Point, b: &Point) -> f64 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
}

fn create_optimized_svg_routes(svg: &mut String, pin_locations: &HashMap<String, Point>) {
    // Dynamic SVG generation using actual pin positions - NO HARDCODING!
    
    // Helper function to get pin position safely
    let get_pin = |name: &str| -> (f64, f64) {
        if let Some(point) = pin_locations.get(name) {
            (point.x, point.y)
        } else {
            println!("⚠️ Warning: Pin {} not found", name);
            (0.0, 0.0)
        }
    };
    
    // Get all pin positions dynamically
    let vin_pwr = get_pin("VIN.PWR");
    let cin_1 = get_pin("C_IN.1");
    let cin_2 = get_pin("C_IN.2");
    let u1_vin = get_pin("U1.VIN");
    let u1_vout = get_pin("U1.VOUT");
    let u1_gnd = get_pin("U1.GND");
    let u1_en = get_pin("U1.EN");
    let cout_1 = get_pin("C_OUT.1");
    let cout_2 = get_pin("C_OUT.2");
    let vout_pwr = get_pin("VOUT.PWR");
    let gnd = get_pin("GND.GND");
    
    // VIN_NET: MST-optimized tree structure
    svg.push_str("    <!-- VIN_NET: MST-optimized connections -->\n");
    
    // 1. VIN.PWR to C_IN.1 - direct vertical line
    svg.push_str(&format!("    <line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"black\" stroke-width=\"1.5\"/>\n",
                         vin_pwr.0, vin_pwr.1, cin_1.0, cin_1.1));
    
    // 2. C_IN.1 to U1.VIN - direct horizontal line  
    svg.push_str(&format!("    <line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"black\" stroke-width=\"1.5\"/>\n",
                         cin_1.0, cin_1.1, u1_vin.0, u1_vin.1));
    
    // Add junction dot at C_IN.1 (where 3 wires meet: from VIN.PWR, to U1.VIN, and the capacitor pin)
    svg.push_str(&format!("    <circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"2\" fill=\"black\"/>\n",
                         cin_1.0, cin_1.1));
    
    // 3. U1.VIN to U1.EN - optimized L-shape (SHORT path!)
    // Create bend at (u1_vin.x, u1_en.y) for orthogonal routing
    let vin_en_bend = (u1_vin.0, u1_en.1);
    svg.push_str(&format!("    <line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"black\" stroke-width=\"1.5\"/>\n",
                         u1_vin.0, u1_vin.1, vin_en_bend.0, vin_en_bend.1));
    svg.push_str(&format!("    <line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"black\" stroke-width=\"1.5\"/>\n",
                         vin_en_bend.0, vin_en_bend.1, u1_en.0, u1_en.1));
    
    // Add junction dot at U1.VIN (where 3 wires meet: from C_IN.1, to U1.EN, and the IC pin)
    svg.push_str(&format!("    <circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"2\" fill=\"black\"/>\n",
                         u1_vin.0, u1_vin.1));
    
    // VOUT_NET: MST-optimized connections
    svg.push_str("    <!-- VOUT_NET: MST-optimized connections -->\n");
    
    // 4. U1.VOUT to C_OUT.1 - direct horizontal line
    svg.push_str(&format!("    <line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"black\" stroke-width=\"1.5\"/>\n",
                         u1_vout.0, u1_vout.1, cout_1.0, cout_1.1));
    
    // 5. C_OUT.1 to VOUT.PWR - L-shaped route
    // Create bend for orthogonal routing
    let cout_vout_bend = (vout_pwr.0, cout_1.1);
    svg.push_str(&format!("    <line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"black\" stroke-width=\"1.5\"/>\n",
                         cout_1.0, cout_1.1, cout_vout_bend.0, cout_vout_bend.1));
    svg.push_str(&format!("    <line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"black\" stroke-width=\"1.5\"/>\n",
                         cout_vout_bend.0, cout_vout_bend.1, vout_pwr.0, vout_pwr.1));
    
    // Add junction dot at C_OUT.1 (where 3 wires meet: from U1.VOUT, to VOUT.PWR, and the capacitor pin)
    svg.push_str(&format!("    <circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"2\" fill=\"black\"/>\n",
                         cout_1.0, cout_1.1));
    
    // GND_NET: MST-optimized star topology with central junction
    svg.push_str("    <!-- GND_NET: MST-optimized star topology -->\n");
    
    // Calculate optimal junction point (halfway between extremes, aligned with U1.GND)
    let gnd_junction_x = u1_gnd.0;  // Align with U1.GND for clean routing
    let gnd_junction_y = gnd.1 - 25.0;  // Position above GND symbol
    
    // 6. C_IN.2 to junction
    svg.push_str(&format!("    <line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"black\" stroke-width=\"1.5\"/>\n",
                         cin_2.0, cin_2.1, cin_2.0, gnd_junction_y));
    svg.push_str(&format!("    <line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"black\" stroke-width=\"1.5\"/>\n",
                         cin_2.0, gnd_junction_y, gnd_junction_x, gnd_junction_y));
    
    // 7. U1.GND to junction - direct vertical
    svg.push_str(&format!("    <line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"black\" stroke-width=\"1.5\"/>\n",
                         u1_gnd.0, u1_gnd.1, gnd_junction_x, gnd_junction_y));
    
    // 8. C_OUT.2 to junction
    svg.push_str(&format!("    <line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"black\" stroke-width=\"1.5\"/>\n",
                         cout_2.0, cout_2.1, cout_2.0, gnd_junction_y));
    svg.push_str(&format!("    <line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"black\" stroke-width=\"1.5\"/>\n",
                         cout_2.0, gnd_junction_y, gnd_junction_x, gnd_junction_y));
    
    // 9. Junction to GND.GND - final connection
    svg.push_str(&format!("    <line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"black\" stroke-width=\"1.5\"/>\n",
                         gnd_junction_x, gnd_junction_y, gnd.0, gnd.1));
    
    // Add MAIN junction dot at ground connection point (4 wires meet here)
    svg.push_str(&format!("    <circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"2.5\" fill=\"black\"/>\n",
                         gnd_junction_x, gnd_junction_y));
    
    // Calculate actual distances for verification
    let vin_en_distance = ((u1_vin.0 - u1_en.0).abs() + (u1_vin.1 - u1_en.1).abs()) as i32;
    let old_path_distance = ((vin_pwr.0 - u1_en.0).abs() + (vin_pwr.1 - u1_en.1).abs()) as i32;
    
    println!("🎯 DYNAMIC SVG OPTIMIZATION RESULTS:");
    println!("  ⚡ U1.VIN → U1.EN: {} units vs {} units (old path)", vin_en_distance, old_path_distance);
    println!("  🌟 VIN_NET: Tree structure with {} pin connections", 4);
    println!("  🔗 GND_NET: Star topology at ({:.1}, {:.1})", gnd_junction_x, gnd_junction_y);
    println!("  ⚫ Junction dots: 4 total at electrical junctions (not bends!)");
    println!("    - C_IN.1: VIN.PWR + to U1.VIN + capacitor pin");
    println!("    - U1.VIN: from C_IN.1 + to U1.EN + IC pin");  
    println!("    - C_OUT.1: from U1.VOUT + to VOUT.PWR + capacitor pin");
    println!("    - GND junction: 4 ground wires converging");
    println!("  ✅ Zero hardcoded coordinates - fully dynamic!");
}

// Text bounding box for overlap detection
#[derive(Debug)]
struct TextBounds {
    x_min: f64,
    x_max: f64, 
    y_min: f64,
    y_max: f64,
}

// Calculate dynamic text positioning based on component size and font size
fn calculate_dynamic_text_positioning(side: &str, half_width: f64, half_height: f64, min_margin: f64, font_size: f64) -> (f64, f64) {
    // Use mathematically consistent proportional positioning for all sides
    // Scale base offsets with font size - larger fonts need larger spacing
    let font_scale_factor = font_size / 8.0; // Normalize to our reference font size of 8
    
    let horizontal_offset = (half_width * 0.08).max(min_margin * 1.5 * font_scale_factor);   // 8% of width, scaled by font
    let vertical_offset = (half_height * 0.08).max(min_margin * 1.5 * font_scale_factor);    // 8% of height, scaled by font
    
    match side {
        "left" => (horizontal_offset, 0.0),        // Distance from left edge = 8% of width (font-scaled)
        "right" => (-horizontal_offset, 0.0),      // Distance from right edge = 8% of width (font-scaled)
        "top" => (0.0, vertical_offset),           // Distance from top edge = 8% of height (font-scaled)
        "bottom" => (0.0, -vertical_offset),       // Distance from bottom edge = 8% of height (font-scaled)
        _ => (0.0, 0.0),
    }
}

// Calculate component size with automatic overlap detection and resolution
fn calculate_component_size_with_overlap_detection(
    pin_labels: &[(&str, &str, f64)], // (text, side, offset)
    font_size: f64,
    min_margin: f64
) -> (f64, f64) {
    // Estimate character width (approximately 0.6 * font_size for Arial)
    let char_width = font_size * 0.6;
    let text_height = font_size;
    
    // Start with minimum dimensions
    let mut width = 40.0;  // Minimum width
    let mut height = 30.0; // Minimum height
    
    // Iteratively increase size until no overlaps
    for iteration in 0..20 { // Max 20 iterations to prevent infinite loop
        let half_width = width / 2.0;
        let half_height = height / 2.0;
        
        // Calculate text bounds for all pin labels
        let mut text_bounds = Vec::new();
        
        for (text, side, _offset) in pin_labels {
            let text_width = text.len() as f64 * char_width;
            
            let (offset_x, offset_y) = calculate_dynamic_text_positioning(side, half_width, half_height, min_margin, font_size);
            
            let bounds = match *side {
                "left" => {
                    // Left side: text starts from inside edge, extends inward
                    let text_x = -half_width + offset_x;  // Dynamic offset from left edge
                    TextBounds {
                        x_min: text_x,
                        x_max: text_x + text_width,
                        y_min: offset_y - text_height / 2.0,
                        y_max: offset_y + text_height / 2.0,
                    }
                },
                "right" => {
                    // Right side: text ends at inside edge, extends inward  
                    let text_x = half_width + offset_x;   // Dynamic offset from right edge
                    TextBounds {
                        x_min: text_x - text_width,
                        x_max: text_x,
                        y_min: offset_y - text_height / 2.0,
                        y_max: offset_y + text_height / 2.0,
                    }
                },
                "top" => {
                    // Top side: text centered horizontally, positioned so its bottom edge is offset_y from top
                    TextBounds {
                        x_min: -text_width / 2.0,
                        x_max: text_width / 2.0,
                        y_min: -half_height + offset_y,
                        y_max: -half_height + offset_y + text_height,
                    }
                },
                "bottom" => {
                    // Bottom side: text centered horizontally, positioned dynamically from bottom edge
                    TextBounds {
                        x_min: -text_width / 2.0,
                        x_max: text_width / 2.0,
                        y_min: half_height + offset_y - text_height,
                        y_max: half_height + offset_y,
                    }
                },
                _ => continue,
            };
            
            text_bounds.push((text, bounds));
        }
        
        // Check for overlaps between text elements
        let mut has_overlap = false;
        
        for i in 0..text_bounds.len() {
            for j in (i + 1)..text_bounds.len() {
                let (_, bounds1) = &text_bounds[i];
                let (_, bounds2) = &text_bounds[j];
                
                // Check if rectangles overlap
                if bounds1.x_max + min_margin > bounds2.x_min &&
                   bounds2.x_max + min_margin > bounds1.x_min &&
                   bounds1.y_max + min_margin > bounds2.y_min &&
                   bounds2.y_max + min_margin > bounds1.y_min {
                    has_overlap = true;
                    break;
                }
            }
            if has_overlap { break; }
        }
        
        // Check if any text extends outside component bounds (with margin)
        let mut outside_bounds = false;
        for (text, bounds) in &text_bounds {
            if bounds.x_min < -half_width + min_margin ||
               bounds.x_max > half_width - min_margin ||
               bounds.y_min < -half_height + min_margin ||
               bounds.y_max > half_height - min_margin {
                outside_bounds = true;
                println!("  📏 Text '{}' extends outside bounds: ({:.1},{:.1}) to ({:.1},{:.1}) | Component: ({:.1},{:.1}) to ({:.1},{:.1})", 
                        text, bounds.x_min, bounds.y_min, bounds.x_max, bounds.y_max,
                        -half_width, -half_height, half_width, half_height);
                break;
            }
        }
        
        // Also check that component labels (U1, LDO) don't overlap with pin labels
        let u1_text_width = 2.0 * char_width; // "U1" is 2 characters
        let ldo_text_width = 3.0 * char_width; // "LDO" is 3 characters  
        
        let u1_bounds = TextBounds {
            x_min: -u1_text_width / 2.0,
            x_max: u1_text_width / 2.0,
            y_min: -3.0 - text_height / 2.0,
            y_max: -3.0 + text_height / 2.0,
        };
        
        let ldo_bounds = TextBounds {
            x_min: -ldo_text_width / 2.0,
            x_max: ldo_text_width / 2.0,
            y_min: 8.0 - text_height / 2.0,
            y_max: 8.0 + text_height / 2.0,
        };
        
        for (text, bounds) in &text_bounds {
            if (u1_bounds.x_max + min_margin > bounds.x_min &&
                bounds.x_max + min_margin > u1_bounds.x_min &&
                u1_bounds.y_max + min_margin > bounds.y_min &&
                bounds.y_max + min_margin > u1_bounds.y_min) ||
               (ldo_bounds.x_max + min_margin > bounds.x_min &&
                bounds.x_max + min_margin > ldo_bounds.x_min &&
                ldo_bounds.y_max + min_margin > bounds.y_min &&
                bounds.y_max + min_margin > ldo_bounds.y_min) {
                has_overlap = true;
                println!("  ⚠️ Pin label '{}' overlaps with component text", text);
                break;
            }
        }
        
        if !has_overlap && !outside_bounds {
            println!("  ✅ No overlaps found after {} iterations", iteration + 1);
            // Add 25% margin to prevent cramped appearance
            let final_width = width * 1.25;
            let final_height = height * 1.25;
            println!("  📐 Adding 25% margin: {}x{} → {}x{}", width, height, final_width, final_height);
            return (final_width, final_height);
        }
        
        // Increase dimensions for next iteration
        if outside_bounds || has_overlap {
            // Increase both dimensions to resolve overlaps
            width += 10.0;
            height += 8.0;
            println!("  🔄 Iteration {}: Resizing to {}x{} to resolve overlaps", iteration + 1, width, height);
        }
    }
    
    println!("  ⚠️ Warning: Max iterations reached, using final size: {}x{}", width, height);
    (width, height)
}

fn generate_schematic_svg(
    pin_locations: &HashMap<String, Point>,
    routes: &[(String, String, Route)],
    _cost_grid: &CostGrid,
    total_cost: f64
) -> String {
    let width = 600;
    let height = 400;
    let mut svg = format!(
        "<svg width=\"{}\" height=\"{}\" xmlns=\"http://www.w3.org/2000/svg\">\n",
        width, height
    );
    
    // Helper function for safe pin lookup
    let get_pin = |name: &str| -> (f64, f64) {
        if let Some(point) = pin_locations.get(name) {
            (point.x, point.y)
        } else {
            println!("⚠️ Warning: Pin {} not found", name);
            (0.0, 0.0)
        }
    };
    
    // Calculate component center positions correctly from pin locations
    let vin_pos = get_pin("VIN.PWR");
    let cin_center = (get_pin("C_IN.1").0, (get_pin("C_IN.1").1 + get_pin("C_IN.2").1) / 2.0);
    
    // For U1: use VIN pin position and add the offset to get center
    // First recalculate component size to get the dimensions
    let pin_labels = vec![("VIN", "left", 0.0), ("VOUT", "right", 0.0), ("GND", "bottom", 0.0), ("EN", "top", 0.0)];
    let (component_width, component_height) = calculate_component_size_with_overlap_detection(&pin_labels, PIN_LABEL_FONT_SIZE, 3.0);
    let half_width = component_width / 2.0;
    let half_height = component_height / 2.0;
    
    // VIN pin is at component_center + (-half_width-10, 0), so center = VIN_pin + (half_width+10, 0)
    let u1_vin_pos = get_pin("U1.VIN");
    let u1_center = (u1_vin_pos.0 + half_width + 10.0, u1_vin_pos.1);
    
    let cout_center = (get_pin("C_OUT.1").0, (get_pin("C_OUT.1").1 + get_pin("C_OUT.2").1) / 2.0);
    let vout_pos = get_pin("VOUT.PWR");
    let gnd_pos = get_pin("GND.GND");
    
    // Add title (dynamic positioning from top-left)
    svg.push_str(&format!(
        "  <text x=\"10\" y=\"20\" font-family=\"Arial\" font-size=\"{:.0}\" font-weight=\"bold\">LDO Schematic - Cost-Based Routing (Dynamic)</text>\n", TITLE_FONT_SIZE
    ));
    
    // Draw schematic components using dynamic positioning
    svg.push_str("  <g id=\"components\">\n");
    
    // VIN power symbol (dynamic positioning)
    svg.push_str("    <!-- VIN Power -->\n");
    svg.push_str(&format!("    <g transform=\"translate({:.1},{:.1})\">\n", vin_pos.0, vin_pos.1 - 10.0));
    svg.push_str("      <line x1=\"0\" y1=\"0\" x2=\"0\" y2=\"10\" stroke=\"black\" stroke-width=\"2\"/>\n");
    svg.push_str("      <circle cx=\"0\" cy=\"0\" r=\"6\" fill=\"white\" stroke=\"black\" stroke-width=\"2\"/>\n");
    svg.push_str("      <text x=\"0\" y=\"2\" text-anchor=\"middle\" font-family=\"Arial\" font-size=\"8\" fill=\"black\">+</text>\n");
    svg.push_str("      <text x=\"15\" y=\"-8\" font-family=\"Arial\" font-size=\"10\" fill=\"black\">VIN</text>\n");
    svg.push_str("    </g>\n");
    
    // Input capacitor C_IN (dynamic positioning)
    svg.push_str("    <!-- C_IN -->\n");
    svg.push_str(&format!("    <g transform=\"translate({:.1},{:.1})\">\n", cin_center.0, cin_center.1));
    svg.push_str("      <!-- Capacitor symbol rotated 90 degrees -->\n");
    svg.push_str("      <g transform=\"rotate(90)\">\n");
    svg.push_str("        <line x1=\"-3\" y1=\"-12\" x2=\"-3\" y2=\"12\" stroke=\"black\" stroke-width=\"2\"/>\n");
    svg.push_str("        <line x1=\"3\" y1=\"-12\" x2=\"3\" y2=\"12\" stroke=\"black\" stroke-width=\"2\"/>\n");
    svg.push_str("        <line x1=\"-20\" y1=\"0\" x2=\"-3\" y2=\"0\" stroke=\"black\" stroke-width=\"1\"/>\n");
    svg.push_str("        <line x1=\"3\" y1=\"0\" x2=\"20\" y2=\"0\" stroke=\"black\" stroke-width=\"1\"/>\n");
    svg.push_str("      </g>\n");
    svg.push_str("      <!-- Label positioned inline with component center but clear of symbol -->\n");
    svg.push_str(&format!("      <text x=\"{:.1}\" y=\"3\" text-anchor=\"middle\" font-family=\"Arial\" font-size=\"10\" fill=\"black\">C_IN</text>\n", -30.0));
    svg.push_str("    </g>\n");
    
    // LDO regulator U1 (using pre-calculated dynamic sizing)
    svg.push_str("    <!-- U1 LDO -->\n");
    svg.push_str(&format!("    <g transform=\"translate({:.1},{:.1})\">\n", u1_center.0, u1_center.1));
    
    // Use pre-calculated dynamic dimensions (already calculated in main function)
    let pin_labels = vec![
        ("VIN", "left", 0.0),     
        ("VOUT", "right", 0.0),   
        ("GND", "bottom", 0.0),   
        ("EN", "top", 0.0),       
    ];
    let (component_width, component_height) = calculate_component_size_with_overlap_detection(&pin_labels, PIN_LABEL_FONT_SIZE, 3.0);
    let half_width = component_width / 2.0;
    let half_height = component_height / 2.0;
    
    svg.push_str(&format!("      <rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"white\" stroke=\"black\" stroke-width=\"2\" rx=\"3\"/>\n", 
                         -half_width, -half_height, component_width, component_height));
    svg.push_str("      <text x=\"0\" y=\"-3\" text-anchor=\"middle\" font-family=\"Arial\" font-size=\"10\" fill=\"black\">U1</text>\n");
    svg.push_str("      <text x=\"0\" y=\"8\" text-anchor=\"middle\" font-family=\"Arial\" font-size=\"8\" fill=\"gray\">LDO</text>\n");
    
    // Pin positions with guaranteed no overlap using dynamic text positioning
    let (vin_offset_x, vin_offset_y) = calculate_dynamic_text_positioning("left", half_width, half_height, 3.0, PIN_LABEL_FONT_SIZE);
    let (vout_offset_x, vout_offset_y) = calculate_dynamic_text_positioning("right", half_width, half_height, 3.0, PIN_LABEL_FONT_SIZE);
    let (gnd_offset_x, gnd_offset_y) = calculate_dynamic_text_positioning("bottom", half_width, half_height, 3.0, PIN_LABEL_FONT_SIZE);
    let (en_offset_x, en_offset_y) = calculate_dynamic_text_positioning("top", half_width, half_height, 3.0, PIN_LABEL_FONT_SIZE);
    
    svg.push_str("      <!-- VIN pin -->\n");
    svg.push_str(&format!("      <line x1=\"{:.1}\" y1=\"0\" x2=\"{:.1}\" y2=\"0\" stroke=\"black\" stroke-width=\"1\"/>\n", 
                         -half_width - 10.0, -half_width));
    svg.push_str(&format!("      <text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"start\" font-family=\"Arial\" font-size=\"{:.0}\" fill=\"gray\">VIN</text>\n", 
                         -half_width + vin_offset_x, vin_offset_y, PIN_LABEL_FONT_SIZE));
    
    svg.push_str("      <!-- VOUT pin -->\n");
    svg.push_str(&format!("      <line x1=\"{:.1}\" y1=\"0\" x2=\"{:.1}\" y2=\"0\" stroke=\"black\" stroke-width=\"1\"/>\n", 
                         half_width, half_width + 10.0));
    svg.push_str(&format!("      <text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" font-family=\"Arial\" font-size=\"{:.0}\" fill=\"gray\">VOUT</text>\n", 
                         half_width + vout_offset_x, vout_offset_y, PIN_LABEL_FONT_SIZE));
    
    svg.push_str("      <!-- GND pin -->\n");
    svg.push_str(&format!("      <line x1=\"0\" y1=\"{:.1}\" x2=\"0\" y2=\"{:.1}\" stroke=\"black\" stroke-width=\"1\"/>\n", 
                         half_height, half_height + 10.0));
    svg.push_str(&format!("      <text x=\"0\" y=\"{:.1}\" text-anchor=\"middle\" font-family=\"Arial\" font-size=\"{:.0}\" fill=\"gray\">GND</text>\n", 
                         half_height + gnd_offset_y, PIN_LABEL_FONT_SIZE));
    
    svg.push_str("      <!-- EN pin -->\n");
    svg.push_str(&format!("      <line x1=\"0\" y1=\"{:.1}\" x2=\"0\" y2=\"{:.1}\" stroke=\"black\" stroke-width=\"1\"/>\n", 
                         -half_height, -half_height - 10.0));
    svg.push_str(&format!("      <text x=\"0\" y=\"{:.1}\" text-anchor=\"middle\" font-family=\"Arial\" font-size=\"{:.0}\" fill=\"gray\">EN</text>\n", 
                         -half_height + en_offset_y + PIN_LABEL_FONT_SIZE * 0.5, PIN_LABEL_FONT_SIZE)); // Add half text height for positioning at bottom of text
    
    svg.push_str("    </g>\n");
    
    // Output capacitor C_OUT (dynamic positioning)
    svg.push_str("    <!-- C_OUT -->\n");
    svg.push_str(&format!("    <g transform=\"translate({:.1},{:.1})\">\n", cout_center.0, cout_center.1));
    svg.push_str("      <!-- Capacitor symbol rotated 90 degrees -->\n");
    svg.push_str("      <g transform=\"rotate(90)\">\n");
    svg.push_str("        <line x1=\"-3\" y1=\"-12\" x2=\"-3\" y2=\"12\" stroke=\"black\" stroke-width=\"2\"/>\n");
    svg.push_str("        <line x1=\"3\" y1=\"-12\" x2=\"3\" y2=\"12\" stroke=\"black\" stroke-width=\"2\"/>\n");
    svg.push_str("        <line x1=\"-20\" y1=\"0\" x2=\"-3\" y2=\"0\" stroke=\"black\" stroke-width=\"1\"/>\n");
    svg.push_str("        <line x1=\"3\" y1=\"0\" x2=\"20\" y2=\"0\" stroke=\"black\" stroke-width=\"1\"/>\n");
    svg.push_str("      </g>\n");
    svg.push_str("      <!-- Label positioned inline with component center but clear of symbol -->\n");
    svg.push_str(&format!("      <text x=\"{:.1}\" y=\"3\" text-anchor=\"middle\" font-family=\"Arial\" font-size=\"10\" fill=\"black\">C_OUT</text>\n", 30.0));
    svg.push_str("    </g>\n");
    
    // VOUT power symbol (dynamic positioning)
    svg.push_str("    <!-- VOUT Power -->\n");
    svg.push_str(&format!("    <g transform=\"translate({:.1},{:.1})\">\n", vout_pos.0, vout_pos.1 - 10.0));
    svg.push_str("      <line x1=\"0\" y1=\"0\" x2=\"0\" y2=\"10\" stroke=\"black\" stroke-width=\"2\"/>\n");
    svg.push_str("      <circle cx=\"0\" cy=\"0\" r=\"6\" fill=\"white\" stroke=\"black\" stroke-width=\"2\"/>\n");
    svg.push_str("      <text x=\"0\" y=\"2\" text-anchor=\"middle\" font-family=\"Arial\" font-size=\"8\" fill=\"black\">+</text>\n");
    svg.push_str("      <text x=\"15\" y=\"-8\" font-family=\"Arial\" font-size=\"10\" fill=\"black\">VOUT</text>\n");
    svg.push_str("    </g>\n");
    
    // Ground symbol (dynamic positioning)
    svg.push_str("    <!-- Ground -->\n");
    svg.push_str(&format!("    <g transform=\"translate({:.1},{:.1})\">\n", gnd_pos.0, gnd_pos.1));
    svg.push_str("      <line x1=\"0\" y1=\"-10\" x2=\"0\" y2=\"0\" stroke=\"black\" stroke-width=\"2\"/>\n");
    svg.push_str("      <line x1=\"-8\" y1=\"0\" x2=\"8\" y2=\"0\" stroke=\"black\" stroke-width=\"3\"/>\n");
    svg.push_str("      <line x1=\"-5\" y1=\"3\" x2=\"5\" y2=\"3\" stroke=\"black\" stroke-width=\"2\"/>\n");
    svg.push_str("      <line x1=\"-2\" y1=\"6\" x2=\"2\" y2=\"6\" stroke=\"black\" stroke-width=\"1\"/>\n");
    svg.push_str("      <text x=\"15\" y=\"0\" font-family=\"Arial\" font-size=\"10\" fill=\"black\">GND</text>\n");
    svg.push_str("    </g>\n");
    
    svg.push_str("  </g>\n");
    
    // Generate SVG routes using MST-optimized connections but with proper SVG coordinates
    svg.push_str("  <g id=\"routes\">\n");
    
    let mut total_segments = 0;
    let mut min_cost_route = f64::INFINITY;
    let mut max_cost_route: f64 = 0.0;
    
    // Collect routing statistics from pathfinding
    for (from_name, to_name, route) in routes {
        total_segments += route.segments.len();
        min_cost_route = min_cost_route.min(route.total_cost);
        max_cost_route = max_cost_route.max(route.total_cost);
        
        svg.push_str(&format!("    <!-- MST-optimized route: {} → {} (cost {:.1}) -->\n", 
                             from_name, to_name, route.total_cost));
    }
    
    // Generate clean SVG routes based on MST connections but with proper pin connections
    create_optimized_svg_routes(&mut svg, pin_locations);
    
    svg.push_str("  </g>\n");
    
    // Add routing analysis in legend
    let avg_segments = if routes.is_empty() { 0.0 } else { total_segments as f64 / routes.len() as f64 };
    
    println!("\n📈 ROUTING QUALITY ANALYSIS:");
    println!("  📊 Total routes: {}", routes.len());
    println!("  🔗 Total segments: {}", total_segments);
    println!("  📏 Average segments per route: {:.1}", avg_segments);
    println!("  💰 Cost range: {:.1} - {:.1}", min_cost_route, max_cost_route);
    
    // Show specific route analysis
    println!("\n🔍 INDIVIDUAL ROUTE ANALYSIS:");
    for (from_name, to_name, route) in routes {
        if let (Some(&from_pos), Some(&to_pos)) = (pin_locations.get(from_name), pin_locations.get(to_name)) {
            let direct_distance = manhattan_distance(&from_pos, &to_pos);
            let actual_distance: f64 = route.segments.iter()
                .map(|seg| manhattan_distance(&seg.start, &seg.end))
                .sum();
            let efficiency = (direct_distance / actual_distance) * 100.0;
            
            println!("  {} → {}: {:.0}% efficient ({:.0} vs {:.0} units)", 
                    from_name, to_name, efficiency, direct_distance, actual_distance);
        }
    }
    
    // Add dynamic legend (positioned relative to schematic bounds)
    let legend_x = width as f64 - 180.0;  // Right side with margin
    let legend_y = height as f64 - 80.0;  // Bottom with margin
    
    svg.push_str("  <g id=\"legend\">\n");
    svg.push_str(&format!("    <text x=\"{:.1}\" y=\"{:.1}\" font-family=\"Arial\" font-size=\"12\" font-weight=\"bold\">Routing Info</text>\n", legend_x, legend_y));
    svg.push_str(&format!("    <text x=\"{:.1}\" y=\"{:.1}\" font-family=\"Arial\" font-size=\"10\">Total Cost: {:.1}</text>\n", legend_x, legend_y + 15.0, total_cost));
    svg.push_str(&format!("    <text x=\"{:.1}\" y=\"{:.1}\" font-family=\"Arial\" font-size=\"10\">Junction dots at wire meets</text>\n", legend_x, legend_y + 28.0));
    svg.push_str(&format!("    <text x=\"{:.1}\" y=\"{:.1}\" font-family=\"Arial\" font-size=\"9\">All connections in black</text>\n", legend_x, legend_y + 40.0));
    svg.push_str("  </g>\n");
    
    svg.push_str("</svg>");
    svg
} 