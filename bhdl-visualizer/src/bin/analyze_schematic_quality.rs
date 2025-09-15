use std::fs;
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct ComponentPosition {
    name: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Debug)]
struct WireSegment {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    net_name: Option<String>,
}

#[derive(Debug)]
struct SchematicAnalysis {
    components: Vec<ComponentPosition>,
    wires: Vec<WireSegment>,
    issues: Vec<String>,
    score: f64,
}

fn main() {
    let svg_path = "test_generic_visualizer_output.svg";
    let svg_content = fs::read_to_string(svg_path).expect("Failed to read SVG file");
    
    let analysis = analyze_schematic(&svg_content);
    
    println!("\n=== SCHEMATIC QUALITY ANALYSIS ===\n");
    println!("Overall Score: {:.1}/100\n", analysis.score);
    
    println!("Components Found: {}", analysis.components.len());
    for comp in &analysis.components {
        println!("  {} at ({:.1}, {:.1})", comp.name, comp.x, comp.y);
    }
    
    println!("\nWire Segments: {}", analysis.wires.len());
    
    println!("\n🚨 ISSUES DETECTED:");
    for (i, issue) in analysis.issues.iter().enumerate() {
        println!("  {}. {}", i + 1, issue);
    }
    
    println!("\n📊 QUALITY METRICS:");
    let metrics = calculate_metrics(&analysis);
    println!("  • Component Overlap: {}", if metrics.has_overlaps { "❌ YES" } else { "✅ NO" });
    println!("  • Logical Flow: {}", if metrics.has_logical_flow { "✅ YES" } else { "❌ NO" });
    println!("  • Wire Cleanliness: {:.1}%", metrics.wire_cleanliness * 100.0);
    println!("  • Component Alignment: {}", if metrics.components_aligned { "✅ YES" } else { "❌ NO" });
    
    println!("\n💡 RECOMMENDATIONS:");
    generate_recommendations(&analysis, &metrics);
}

fn analyze_schematic(svg_content: &str) -> SchematicAnalysis {
    let mut components = Vec::new();
    let mut wires = Vec::new();
    let mut issues = Vec::new();
    
    // Track when we're inside a component group
    let mut inside_component = false;
    
    // Parse components from SVG
    for line in svg_content.lines() {
        if line.contains("transform=\"translate(") {
            inside_component = true;
            if let Some(comp) = parse_component(line, svg_content) {
                components.push(comp);
            }
        } else if line.trim() == "</g>" && inside_component {
            inside_component = false;
        } else if line.trim().starts_with("<line") && !inside_component {
            // Only count lines that are NOT inside component groups
            if let Some(wire) = parse_wire(line) {
                wires.push(wire);
            }
        }
    }
    
    // Check for overlapping components
    for i in 0..components.len() {
        for j in i+1..components.len() {
            if components_overlap(&components[i], &components[j]) {
                issues.push(format!(
                    "⚠️ Components {} and {} are overlapping at position ({:.1}, {:.1})",
                    components[i].name, components[j].name,
                    components[i].x, components[i].y
                ));
            }
        }
    }
    
    // Check for duplicate wires
    let mut wire_map: HashMap<String, i32> = HashMap::new();
    for wire in &wires {
        let key = format!("{},{}-{},{}", wire.x1, wire.y1, wire.x2, wire.y2);
        *wire_map.entry(key).or_insert(0) += 1;
    }
    for (wire_key, count) in wire_map.iter() {
        if *count > 1 {
            issues.push(format!("⚠️ Duplicate wire segment: {} (appears {} times)", wire_key, count));
        }
    }
    
    // Check component placement logic
    let u1_pos = components.iter().find(|c| c.name == "U1");
    let c1_pos = components.iter().find(|c| c.name == "C1");
    let c2_pos = components.iter().find(|c| c.name == "C2");
    
    if let (Some(u1), Some(c1)) = (u1_pos, c1_pos) {
        if c1.x > u1.x {
            issues.push("⚠️ Input capacitor C1 is to the RIGHT of IC U1 - should be on input side (left)".to_string());
        }
    }
    
    // Check for scattered Y positions
    let y_positions: Vec<f64> = components.iter().map(|c| c.y).collect();
    let y_variance = calculate_variance(&y_positions);
    if y_variance > 1000.0 {
        issues.push(format!("⚠️ Components scattered across multiple rows (Y variance: {:.0})", y_variance));
    }
    
    // Check for wires going through components
    for wire in &wires {
        for comp in &components {
            if wire_passes_through_component(wire, comp) {
                issues.push(format!(
                    "❌ CRITICAL: Wire ({:.0},{:.0})-({:.0},{:.0}) passes through component {} at ({:.0},{:.0})",
                    wire.x1, wire.y1, wire.x2, wire.y2, comp.name, comp.x, comp.y
                ));
            }
            
            // Check for wires overlapping IC borders (larger components)
            if comp.name.starts_with("U") && wire_overlaps_ic_border(wire, comp) {
                issues.push(format!(
                    "❌ CRITICAL: Wire ({:.0},{:.0})-({:.0},{:.0}) overlaps IC {} border at ({:.0},{:.0})",
                    wire.x1, wire.y1, wire.x2, wire.y2, comp.name, comp.x, comp.y
                ));
            }
        }
    }
    
    // Check for disconnected component pins
    check_disconnected_pins(&components, &wires, &mut issues);
    
    // Check for wire-to-pin connection gaps
    check_connection_gaps(&components, &wires, &mut issues);
    
    // Check for wire loops and redundant segments
    check_wire_loops(&wires, &mut issues);
    
    // Calculate overall score
    let score = calculate_score(&issues, &components, &wires);
    
    SchematicAnalysis {
        components,
        wires,
        issues,
        score,
    }
}

fn parse_component(line: &str, full_content: &str) -> Option<ComponentPosition> {
    // Extract transform coordinates
    let transform_start = line.find("translate(")? + 10;
    let transform_end = line.find(")")?;
    let coords = &line[transform_start..transform_end];
    let parts: Vec<&str> = coords.split(", ").collect();
    
    if parts.len() != 2 {
        return None;
    }
    
    let x: f64 = parts[0].parse().ok()?;
    let y: f64 = parts[1].parse().ok()?;
    
    // Find component name in following lines
    let line_num = full_content.lines().position(|l| l == line)?;
    let next_lines: Vec<&str> = full_content.lines().skip(line_num).take(20).collect();
    
    let mut name = String::from("Unknown");
    for next_line in next_lines {
        if next_line.contains("font-weight=\"bold\"") {
            if let Some(start) = next_line.find(">") {
                if let Some(end) = next_line.rfind("<") {
                    name = next_line[start+1..end].to_string();
                    break;
                }
            }
        }
    }
    
    Some(ComponentPosition {
        name,
        x,
        y,
        width: 40.0,  // Default assumption
        height: 20.0, // Default assumption
    })
}

fn parse_wire(line: &str) -> Option<WireSegment> {
    let x1 = extract_attr(line, "x1")?;
    let y1 = extract_attr(line, "y1")?;
    let x2 = extract_attr(line, "x2")?;
    let y2 = extract_attr(line, "y2")?;
    
    Some(WireSegment {
        x1, y1, x2, y2,
        net_name: None,
    })
}

fn extract_attr(line: &str, attr: &str) -> Option<f64> {
    let attr_with_eq = format!("{}=\"", attr);
    let start = line.find(&attr_with_eq)? + attr_with_eq.len();
    let rest = &line[start..];
    let end = rest.find("\"")?;
    rest[..end].parse().ok()
}

fn components_overlap(c1: &ComponentPosition, c2: &ComponentPosition) -> bool {
    (c1.x - c2.x).abs() < 5.0 && (c1.y - c2.y).abs() < 5.0
}

fn wire_passes_through_component(wire: &WireSegment, comp: &ComponentPosition) -> bool {
    // Check if a vertical or horizontal wire passes through component's bounding box
    // Give some margin for component size (approximated)
    // Reduced margins to be more accurate about actual component body sizes
    let comp_half_width = 10.0;  // More accurate half-width
    let comp_half_height = 10.0; // More accurate half-height
    
    let comp_left = comp.x - comp_half_width;
    let comp_right = comp.x + comp_half_width;
    let comp_top = comp.y - comp_half_height;
    let comp_bottom = comp.y + comp_half_height;
    
    // Check if wire is vertical
    if (wire.x1 - wire.x2).abs() < 0.1 {
        let wire_x = wire.x1;
        let wire_top = wire.y1.min(wire.y2);
        let wire_bottom = wire.y1.max(wire.y2);
        
        // Check if vertical wire passes through component
        if wire_x > comp_left && wire_x < comp_right {
            // Check if wire spans overlap with component vertically
            if wire_top < comp_bottom && wire_bottom > comp_top {
                return true;
            }
        }
    }
    
    // Check if wire is horizontal
    if (wire.y1 - wire.y2).abs() < 0.1 {
        let wire_y = wire.y1;
        let wire_left = wire.x1.min(wire.x2);
        let wire_right = wire.x1.max(wire.x2);
        
        // Check if horizontal wire passes through component
        if wire_y > comp_top && wire_y < comp_bottom {
            // Check if wire spans overlap with component horizontally
            if wire_left < comp_right && wire_right > comp_left {
                return true;
            }
        }
    }
    
    false
}

fn wire_overlaps_ic_border(wire: &WireSegment, ic: &ComponentPosition) -> bool {
    // ICs are larger - use actual IC dimensions (80x50 from visualizer)
    let ic_half_width = 40.0;  // IC half-width
    let ic_half_height = 25.0; // IC half-height
    
    let ic_left = ic.x - ic_half_width;
    let ic_right = ic.x + ic_half_width;
    let ic_top = ic.y - ic_half_height;
    let ic_bottom = ic.y + ic_half_height;
    
    // Check if wire runs along or very close to IC border
    let is_vertical = (wire.x1 - wire.x2).abs() < 0.1;
    let is_horizontal = (wire.y1 - wire.y2).abs() < 0.1;
    
    if is_horizontal {
        let wire_y = wire.y1;
        let wire_left = wire.x1.min(wire.x2);
        let wire_right = wire.x1.max(wire.x2);
        
        // Check if horizontal wire runs along top/bottom border of IC
        let near_top = (wire_y - ic_top).abs() < 5.0;
        let near_bottom = (wire_y - ic_bottom).abs() < 5.0;
        
        if (near_top || near_bottom) && wire_left < ic_right && wire_right > ic_left {
            return true;
        }
    }
    
    if is_vertical {
        let wire_x = wire.x1;
        let wire_top = wire.y1.min(wire.y2);
        let wire_bottom = wire.y1.max(wire.y2);
        
        // Check if vertical wire runs along left/right border of IC
        let near_left = (wire_x - ic_left).abs() < 5.0;
        let near_right = (wire_x - ic_right).abs() < 5.0;
        
        if (near_left || near_right) && wire_top < ic_bottom && wire_bottom > ic_top {
            return true;
        }
    }
    
    false
}

fn check_disconnected_pins(components: &[ComponentPosition], wires: &[WireSegment], issues: &mut Vec<String>) {
    for comp in components {
        // Check if resistors (should have 2 connections) are properly connected
        if comp.name.starts_with("R") {
            let connections_count = count_connections_near_component(comp, wires);
            if connections_count == 0 {
                issues.push(format!(
                    "❌ CRITICAL: Component {} at ({:.0},{:.0}) has no wire connections",
                    comp.name, comp.x, comp.y
                ));
            } else if connections_count == 1 {
                issues.push(format!(
                    "⚠️ WARNING: Component {} at ({:.0},{:.0}) has only one connection (should have 2)",
                    comp.name, comp.x, comp.y
                ));
            }
        }
    }
}

fn check_connection_gaps(components: &[ComponentPosition], wires: &[WireSegment], issues: &mut Vec<String>) {
    // Define expected pin positions for different component types
    for comp in components {
        let pin_positions = get_expected_pin_positions(comp);
        
        for (pin_name, pin_pos) in pin_positions {
            // Find the closest wire endpoint to this pin
            let mut min_distance = f64::MAX;
            let mut closest_wire: Option<(f64, f64)> = None;
            
            for wire in wires {
                let dist1 = ((wire.x1 - pin_pos.0).powi(2) + (wire.y1 - pin_pos.1).powi(2)).sqrt();
                let dist2 = ((wire.x2 - pin_pos.0).powi(2) + (wire.y2 - pin_pos.1).powi(2)).sqrt();
                
                if dist1 < min_distance {
                    min_distance = dist1;
                    closest_wire = Some((wire.x1, wire.y1));
                }
                if dist2 < min_distance {
                    min_distance = dist2;
                    closest_wire = Some((wire.x2, wire.y2));
                }
            }
            
            // If there's a wire close but not touching the pin, report significant gaps
            if min_distance > 8.0 && min_distance < 50.0 {  // Gap between 8-50 units (ignore small gaps)
                if let Some((wire_x, wire_y)) = closest_wire {
                    issues.push(format!(
                        "⚠️ WARNING: Wire at ({:.0},{:.0}) has {:.1}-unit gap from {} {} pin at ({:.0},{:.0})",
                        wire_x, wire_y, min_distance, comp.name, pin_name, pin_pos.0, pin_pos.1
                    ));
                }
            }
        }
    }
}

fn get_expected_pin_positions(comp: &ComponentPosition) -> Vec<(String, (f64, f64))> {
    let mut pins = Vec::new();
    
    if comp.name.starts_with("U") {
        // IC pins - match the corrected positions from the visualizer
        pins.push(("IN".to_string(), (comp.x - 40.0 - 5.0, comp.y))); // Left pin
        pins.push(("OUT".to_string(), (comp.x + 40.0 + 5.0, comp.y))); // Right pin  
        pins.push(("GND".to_string(), (comp.x, comp.y + 25.0 + 10.0))); // Bottom pin
    } else if comp.name.starts_with("R") {
        // Resistor pins - horizontal component (width = 40.0)  
        pins.push(("1".to_string(), (comp.x - 40.0/2.0 - 5.0, comp.y))); // Left pin
        pins.push(("2".to_string(), (comp.x + 40.0/2.0 + 5.0, comp.y))); // Right pin
    } else if comp.name.starts_with("C") {
        // Capacitor pins - match the updated visualizer logic
        // For input/output filters, pin 1 connects to the main routing layer (y=150)
        let routing_y = if comp.name == "C1" || comp.name == "C2" || comp.name == "C3" || comp.name == "C4" {
            // Input and output filter capacitors connect to main routing layer
            150.0
        } else {
            // Standard capacitors: adjust based on position relative to main layer
            if comp.y < 150.0 {
                comp.y + 15.0  // Above main layer: connect downward
            } else {
                comp.y - 15.0  // Below main layer: connect upward
            }
        };
        
        pins.push(("1".to_string(), (comp.x, routing_y))); // Pin 1 to routing layer
        pins.push(("2".to_string(), (comp.x, comp.y + 20.0))); // Pin 2 to ground
    } else if comp.name.starts_with("D") {
        // Diode pins - vertical component
        pins.push(("A".to_string(), (comp.x, comp.y - 15.0))); // Extended top pin (anode)
        pins.push(("K".to_string(), (comp.x, comp.y + 15.0))); // Extended bottom pin (cathode)
    } else if comp.name.starts_with("L") {
        // Inductor pins - horizontal component (width = 40.0)
        pins.push(("1".to_string(), (comp.x - 40.0/2.0 - 5.0, comp.y))); // Left pin
        pins.push(("2".to_string(), (comp.x + 40.0/2.0 + 5.0, comp.y))); // Right pin
    }
    
    pins
}

fn count_connections_near_component(comp: &ComponentPosition, wires: &[WireSegment]) -> usize {
    let mut connections = 0;
    let tolerance = 15.0; // Distance tolerance for considering a wire "connected"
    
    // For resistors, check connections at both pin ends (approximate)
    let connection_points = if comp.name.starts_with("R") {
        // Horizontal resistor - pins at left and right ends
        vec![
            (comp.x - 25.0, comp.y), // Left pin (approximate)
            (comp.x + 25.0, comp.y), // Right pin (approximate)
        ]
    } else {
        // For other components, check around center
        vec![(comp.x, comp.y)]
    };
    
    for wire in wires {
        for &(pin_x, pin_y) in &connection_points {
            // Check if either endpoint of the wire is near this pin position
            let dist1 = ((wire.x1 - pin_x).powi(2) + (wire.y1 - pin_y).powi(2)).sqrt();
            let dist2 = ((wire.x2 - pin_x).powi(2) + (wire.y2 - pin_y).powi(2)).sqrt();
            
            if dist1 < tolerance || dist2 < tolerance {
                connections += 1;
                break; // Don't count the same wire multiple times for one component
            }
        }
    }
    
    connections
}

fn check_wire_loops(wires: &[WireSegment], issues: &mut Vec<String>) {
    // Detect rectangular wire loops (like the VOUT issue mentioned)
    let mut wire_endpoints: HashMap<String, Vec<(f64, f64)>> = HashMap::new();
    
    for wire in wires {
        let key1 = format!("{:.0},{:.0}", wire.x1, wire.y1);
        let key2 = format!("{:.0},{:.0}", wire.x2, wire.y2);
        
        wire_endpoints.entry(key1).or_insert_with(Vec::new).push((wire.x2, wire.y2));
        wire_endpoints.entry(key2).or_insert_with(Vec::new).push((wire.x1, wire.y1));
    }
    
    // Look for points with more than 2 connections (potential loop centers)
    for (point_str, connections) in &wire_endpoints {
        if connections.len() > 3 {
            let coords: Vec<&str> = point_str.split(',').collect();
            if coords.len() == 2 {
                issues.push(format!(
                    "⚠️ WARNING: Potential wire loop detected at point ({},{}) with {} connections",
                    coords[0], coords[1], connections.len()
                ));
            }
        }
    }
    
    // Detect rectangular patterns (4 segments forming a rectangle)
    detect_rectangular_loops(wires, issues);
}

fn detect_rectangular_loops(wires: &[WireSegment], issues: &mut Vec<String>) {
    // Look for sets of 4 connected wires that form rectangles
    for i in 0..wires.len() {
        for j in i+1..wires.len() {
            for k in j+1..wires.len() {
                for l in k+1..wires.len() {
                    let wire_set = [&wires[i], &wires[j], &wires[k], &wires[l]];
                    if forms_rectangle(&wire_set) {
                        let center_x = (wire_set.iter().map(|w| (w.x1 + w.x2) / 2.0).sum::<f64>()) / 4.0;
                        let center_y = (wire_set.iter().map(|w| (w.y1 + w.y2) / 2.0).sum::<f64>()) / 4.0;
                        
                        issues.push(format!(
                            "⚠️ WARNING: Rectangular wire loop detected around ({:.0},{:.0})",
                            center_x, center_y
                        ));
                        break;
                    }
                }
            }
        }
    }
}

fn forms_rectangle(wires: &[&WireSegment; 4]) -> bool {
    // Collect all endpoints
    let mut points = Vec::new();
    for wire in wires {
        points.push((wire.x1, wire.y1));
        points.push((wire.x2, wire.y2));
    }
    
    // Remove duplicates (shared endpoints)
    points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.partial_cmp(&b.1).unwrap()));
    points.dedup_by(|a, b| (a.0 - b.0).abs() < 1.0 && (a.1 - b.1).abs() < 1.0);
    
    // A rectangle should have exactly 4 unique corners after deduplication
    if points.len() != 4 {
        return false;
    }
    
    // Check if points form a rectangle (opposite sides parallel and equal)
    let mut x_coords: Vec<f64> = points.iter().map(|p| p.0).collect();
    let mut y_coords: Vec<f64> = points.iter().map(|p| p.1).collect();
    
    x_coords.sort_by(|a, b| a.partial_cmp(b).unwrap());
    y_coords.sort_by(|a, b| a.partial_cmp(b).unwrap());
    
    // Should have exactly 2 unique X coordinates and 2 unique Y coordinates
    x_coords.dedup_by(|a, b| (*a - *b).abs() < 1.0);
    y_coords.dedup_by(|a, b| (*a - *b).abs() < 1.0);
    
    x_coords.len() == 2 && y_coords.len() == 2
}

fn calculate_variance(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64
}

fn calculate_score(issues: &[String], components: &[ComponentPosition], wires: &[WireSegment]) -> f64 {
    let mut score = 100.0;
    
    // Critical issues (wires through components) should have meaningful penalties
    let critical_issues = issues.iter().filter(|i| i.contains("CRITICAL")).count();
    let regular_issues = issues.len() - critical_issues;
    
    // Realistic penalty scaling
    score -= critical_issues as f64 * 15.0;  // 15 points per critical issue (significant penalty)
    score -= regular_issues as f64 * 5.0;    // 5 points per regular issue
    
    // Bonus for good circuit complexity
    if components.len() >= 8 {
        score += 10.0;
    }
    
    // Bonus for proper wire routing density
    if wires.len() >= 20 {
        score += 5.0;
    }
    
    score.max(0.0).min(100.0)
}

struct QualityMetrics {
    has_overlaps: bool,
    has_logical_flow: bool,
    wire_cleanliness: f64,
    components_aligned: bool,
}

fn calculate_metrics(analysis: &SchematicAnalysis) -> QualityMetrics {
    let has_overlaps = analysis.issues.iter().any(|i| i.contains("overlapping"));
    
    // Check if components follow left-to-right flow
    // Components should generally increase in X position (with some tolerance for stacking)
    let mut sorted_components = analysis.components.clone();
    sorted_components.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
    
    // Check if the component order makes sense (input -> processing -> output)
    let has_logical_flow = sorted_components.len() >= 3 && 
        sorted_components.first().unwrap().name.starts_with("C") && // Input caps first
        sorted_components.iter().any(|c| c.name.starts_with("U")) && // IC in middle
        sorted_components.last().unwrap().name.starts_with("R");     // Resistors at end
    
    // Check wire cleanliness (no duplicates, reasonable count)
    let duplicate_wires = analysis.issues.iter().filter(|i| i.contains("Duplicate wire")).count();
    let wire_cleanliness = 1.0 - (duplicate_wires as f64 / analysis.wires.len().max(1) as f64);
    
    // Check component alignment
    let y_positions: Vec<f64> = analysis.components.iter().map(|c| c.y).collect();
    let unique_y: std::collections::HashSet<i32> = y_positions.iter().map(|y| (*y as i32 / 10) * 10).collect();
    let components_aligned = unique_y.len() <= 3; // Should have at most 3 rows
    
    QualityMetrics {
        has_overlaps,
        has_logical_flow,
        wire_cleanliness,
        components_aligned,
    }
}

fn generate_recommendations(analysis: &SchematicAnalysis, metrics: &QualityMetrics) {
    let mut rec_num = 1;
    
    // Check for wires through components - HIGHEST PRIORITY
    let wire_through_issues = analysis.issues.iter()
        .filter(|i| i.contains("CRITICAL") && i.contains("passes through"))
        .count();
    
    if wire_through_issues > 0 {
        println!("  {}. 🚨 CRITICAL: Fix wire routing to avoid passing through components:", rec_num);
        println!("     • Route wires AROUND component boundaries, not through them");
        println!("     • Use L-shaped or Z-shaped paths to navigate around obstacles");
        println!("     • Ensure minimum clearance of 5-10 units from component edges");
        println!("     • Consider using routing channels between component rows");
        rec_num += 1;
    }
    
    if metrics.has_overlaps {
        println!("  {}. Fix overlapping components by adjusting placement algorithm", rec_num);
        rec_num += 1;
    }
    
    if !metrics.has_logical_flow {
        println!("  {}. Reorganize components to follow left-to-right signal flow:", rec_num);
        println!("     Input → Processing → Output");
        rec_num += 1;
    }
    
    if metrics.wire_cleanliness < 0.9 {
        println!("  {}. Remove duplicate wire segments in routing algorithm", rec_num);
        rec_num += 1;
    }
    
    if !metrics.components_aligned {
        println!("  {}. Align components to consistent grid rows for better readability", rec_num);
        rec_num += 1;
    }
    
    // Specific placement recommendations
    let c1_c2_same_pos = analysis.components.iter()
        .filter(|c| c.name == "C1" || c.name == "C2")
        .map(|c| (c.x, c.y))
        .collect::<Vec<_>>();
    
    if c1_c2_same_pos.len() == 2 && c1_c2_same_pos[0] == c1_c2_same_pos[1] {
        println!("  {}. Components C1 and C2 are at the same position - space them vertically", rec_num);
    }
}