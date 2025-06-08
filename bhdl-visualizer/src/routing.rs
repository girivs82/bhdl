use std::collections::HashMap;
use crate::layout::types::Point;

/// Main routing function that creates smart orthogonal wires between connected pins
pub fn create_smart_routing_for_connections(
    pin_locations: &HashMap<String, Point>,
    connections: &[(String, String)],
) -> Vec<String> {
    let component_centers = extract_component_centers(pin_locations);
    create_smart_routing_for_connections_with_obstacles(pin_locations, connections, &[])
}

/// Enhanced routing function that considers component obstacles during routing
pub fn create_smart_routing_for_connections_with_obstacles(
    pin_locations: &HashMap<String, Point>,
    connections: &[(String, String)],
    component_obstacles: &[(Point, f64, f64)], // (center, width, height)
) -> Vec<String> {
    let mut routing_lines = Vec::new();
    
    for (from_pin, to_pin) in connections {
        if let (Some(from_pos), Some(to_pos)) = (pin_locations.get(from_pin), pin_locations.get(to_pin)) {
            let routing_svg = create_smart_orthogonal_wire_with_explicit_obstacles(from_pos, to_pos, component_obstacles);
            routing_lines.push(routing_svg);
        }
    }
    
    routing_lines
}

/// Extract component centers from pin locations for obstacle avoidance
fn extract_component_centers(pin_locations: &HashMap<String, Point>) -> Vec<Point> {
    let mut centers = Vec::new();
    let mut component_pins: HashMap<String, Vec<Point>> = HashMap::new();
    
    for (pin_name, pin_pos) in pin_locations {
        if let Some(component_part) = pin_name.split('.').next() {
            component_pins.entry(component_part.to_string())
                          .or_insert_with(Vec::new)
                          .push(*pin_pos);
        }
    }
    
    for (_component, pins) in component_pins {
        if !pins.is_empty() {
            let center_x = pins.iter().map(|p| p.x).sum::<f64>() / pins.len() as f64;
            let center_y = pins.iter().map(|p| p.y).sum::<f64>() / pins.len() as f64;
            centers.push(Point::new(center_x, center_y));
        }
    }
    
    centers
}

/// Create smart orthogonal wire routing with explicit obstacle avoidance
fn create_smart_orthogonal_wire_with_explicit_obstacles(from: &Point, to: &Point, obstacles: &[(Point, f64, f64)]) -> String {
    let dx = (to.x - from.x).abs();
    let dy = (to.y - from.y).abs();
    
    if dy < 5.0 {
        // Same Y level - check if horizontal line would hit obstacles before drawing straight line
        if would_horizontal_line_hit_explicit_obstacles(from, to, obstacles) {
            // Horizontal line would hit obstacle - route around with minimal offset for small obstacles  
            let small_offset = 15.0; // Sufficient offset to clear capacitor height
            let intermediate_y = if from.y > 150.0 { from.y + small_offset } else { from.y - small_offset };

            format!(
                "  <path d=\"M {} {} L {} {} L {} {} L {} {}\" fill=\"none\" stroke=\"black\" stroke-width=\"1.5\"/>",
                from.x, from.y,         // Start point
                from.x, intermediate_y, // Vertical away from obstacles
                to.x, intermediate_y,   // Horizontal to target X  
                to.x, to.y              // Vertical to target Y
            )
        } else {
            // Clear horizontal path - use straight line
            format!(
                "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"black\" stroke-width=\"1.5\"/>",
                from.x, from.y, to.x, to.y
            )
        }
    } else if dx < 5.0 {
        // Same X level - check for vertical obstacles
        if would_vertical_line_hit_explicit_obstacles(from, to, obstacles) {
            // Vertical line would hit obstacle - route around 
            let small_offset = 15.0; // Minimal offset for tiny obstacles
            let intermediate_x = if from.x > 400.0 { from.x + small_offset } else { from.x - small_offset };

            format!(
                "  <path d=\"M {} {} L {} {} L {} {} L {} {}\" fill=\"none\" stroke=\"black\" stroke-width=\"1.5\"/>",
                from.x, from.y,         // Start point
                intermediate_x, from.y, // Horizontal away from obstacles
                intermediate_x, to.y,   // Vertical to target Y
                to.x, to.y              // Horizontal to target X
            )
        } else {
            // Clear vertical path - use straight line
            format!(
                "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"black\" stroke-width=\"1.5\"/>",
                from.x, from.y, to.x, to.y
            )
        }
    } else {
        // Different X and Y - need L-shape routing
        // Try vertical-first L-shape (preferred)
        if !would_l_shape_hit_explicit_obstacles(from, to, obstacles) {
            format!(
                "  <path d=\"M {} {} L {} {} L {} {}\" fill=\"none\" stroke=\"black\" stroke-width=\"1.5\"/>",
                from.x, from.y, from.x, to.y, to.x, to.y
            )
        } else {
            // L-shape would hit obstacles - need to route around
            let clearance = 25.0; // Smaller clearance for better routing
            
            
            // Choose better routing strategy based on obstacle positions
            if from.y < to.y {
                // Going down - route above then down
                let intermediate_y = from.y - clearance;
                format!(
                    "  <path d=\"M {} {} L {} {} L {} {} L {} {}\" fill=\"none\" stroke=\"black\" stroke-width=\"1.5\"/>",
                    from.x, from.y, from.x, intermediate_y, to.x, intermediate_y, to.x, to.y
                )
            } else {
                // Going up - route below then up
                let intermediate_y = from.y + clearance;
                format!(
                    "  <path d=\"M {} {} L {} {} L {} {} L {} {}\" fill=\"none\" stroke=\"black\" stroke-width=\"1.5\"/>",
                    from.x, from.y, from.x, intermediate_y, to.x, intermediate_y, to.x, to.y
                )
            }
        }
    }
}

/// Legacy routing function for backwards compatibility
fn create_smart_orthogonal_wire_with_obstacles(from: &Point, to: &Point, obstacles: &[Point]) -> String {
    let dx = (to.x - from.x).abs();
    let dy = (to.y - from.y).abs();
    
    // Component dimensions (assuming uniform sizing for legacy function)
    let comp_width = 6.0;
    let comp_height = 24.0;
    
    if dy < 5.0 {
        // Same Y level - check if horizontal line would hit obstacles  
        if would_horizontal_line_hit_obstacle(from, to, obstacles, comp_width, comp_height) {
            // Horizontal line would hit obstacle - route around with vertical offset
            let small_offset = 15.0; // Minimal offset for tiny capacitor plates
            let intermediate_y = if from.y > 150.0 { from.y + small_offset } else { from.y - small_offset };
            format!(
                "  <path d=\"M {} {} L {} {} L {} {} L {} {}\" fill=\"none\" stroke=\"black\" stroke-width=\"1.5\"/>",
                from.x, from.y,         // Start point
                from.x, intermediate_y, // Vertical away from obstacles
                to.x, intermediate_y,   // Horizontal to target X  
                to.x, to.y              // Vertical to target Y
            )
        } else {
            // Clear horizontal path
            format!(
                "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"black\" stroke-width=\"1.5\"/>",
                from.x, from.y, to.x, to.y
            )
        }
    } else if dx < 5.0 {
        // Same X level - should be a straight vertical line, but check for obstacles
        format!(
            "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"black\" stroke-width=\"1.5\"/>",
            from.x, from.y, to.x, to.y
        )
    } else {
        // Different X and Y - need L-shape routing
        // Check if standard L-shape would hit obstacles
        if !would_l_shape_hit_obstacle(from, to, obstacles, comp_width, comp_height) {
            // Standard vertical-first L-shape
            format!(
                "  <path d=\"M {} {} L {} {} L {} {}\" fill=\"none\" stroke=\"black\" stroke-width=\"1.5\"/>",
                from.x, from.y, from.x, to.y, to.x, to.y
            )
        } else {
            // L-shape hits obstacles - route around
            let clearance = 80.0;
            let intermediate_y = if from.y > 150.0 { from.y + clearance } else { from.y - clearance };
            format!(
                "  <path d=\"M {} {} L {} {} L {} {} L {} {}\" fill=\"none\" stroke=\"black\" stroke-width=\"1.5\"/>",
                from.x, from.y,         // Start point
                from.x, intermediate_y, // Vertical away from obstacles
                to.x, intermediate_y,   // Horizontal to target X  
                to.x, to.y              // Vertical to target Y
            )
        }
    }
}

/// Check if horizontal line would hit explicit obstacles
fn would_horizontal_line_hit_explicit_obstacles(from: &Point, to: &Point, obstacles: &[(Point, f64, f64)]) -> bool {
    let start_x = from.x.min(to.x);
    let end_x = from.x.max(to.x);
    let line_y = from.y;
    
    for &(obstacle_center, comp_width, comp_height) in obstacles {
        let rect_left = obstacle_center.x - comp_width / 2.0;
        let rect_right = obstacle_center.x + comp_width / 2.0;
        let rect_top = obstacle_center.y - comp_height / 2.0;
        let rect_bottom = obstacle_center.y + comp_height / 2.0;
        
        if crate::geometry::line_segment_intersects_rectangle(
            start_x, line_y, end_x, line_y,
            rect_left, rect_top, rect_right, rect_bottom
        ) {
            return true;
        }
    }
    false
}

/// Check if vertical line would hit explicit obstacles
fn would_vertical_line_hit_explicit_obstacles(from: &Point, to: &Point, obstacles: &[(Point, f64, f64)]) -> bool {
    let start_y = from.y.min(to.y);
    let end_y = from.y.max(to.y);
    let line_x = from.x;
    
    for &(obstacle_center, comp_width, comp_height) in obstacles {
        let rect_left = obstacle_center.x - comp_width / 2.0;
        let rect_right = obstacle_center.x + comp_width / 2.0;
        let rect_top = obstacle_center.y - comp_height / 2.0;
        let rect_bottom = obstacle_center.y + comp_height / 2.0;
        
        if crate::geometry::line_segment_intersects_rectangle(
            line_x, start_y, line_x, end_y,
            rect_left, rect_top, rect_right, rect_bottom
        ) {
            return true;
        }
    }
    false
}

/// Check if L-shape routing would hit explicit obstacles
fn would_l_shape_hit_explicit_obstacles(from: &Point, to: &Point, obstacles: &[(Point, f64, f64)]) -> bool {
    // For vertical-first L-shape: from -> (from.x, to.y) -> to
    let horizontal_from = Point::new(from.x, to.y);
    let horizontal_to = Point::new(to.x, to.y);
    let vertical_from = Point::new(from.x, from.y);
    let vertical_to = Point::new(from.x, to.y);
    
    // Check if either segment hits obstacles
    if would_horizontal_line_hit_explicit_obstacles(&horizontal_from, &horizontal_to, obstacles) {
        return true;
    }
    
    if would_vertical_line_hit_explicit_obstacles(&vertical_from, &vertical_to, obstacles) {
        return true;
    }
    
    false
}

/// Legacy obstacle detection for horizontal lines
fn would_horizontal_line_hit_obstacle(from: &Point, to: &Point, obstacles: &[Point], comp_width: f64, comp_height: f64) -> bool {
    let start_x = from.x.min(to.x);
    let end_x = from.x.max(to.x);
    let line_y = from.y;
    
    for obstacle_center in obstacles {
        let rect_left = obstacle_center.x - comp_width / 2.0;
        let rect_right = obstacle_center.x + comp_width / 2.0;
        let rect_top = obstacle_center.y - comp_height / 2.0;
        let rect_bottom = obstacle_center.y + comp_height / 2.0;
        
        if crate::geometry::line_segment_intersects_rectangle(
            start_x, line_y, end_x, line_y,
            rect_left, rect_top, rect_right, rect_bottom
        ) {
            return true;
        }
    }
    false
}

/// Legacy obstacle detection for L-shape routing
fn would_l_shape_hit_obstacle(from: &Point, to: &Point, obstacles: &[Point], comp_width: f64, comp_height: f64) -> bool {
    // Check both segments of the L-shape: vertical then horizontal
    let intermediate = Point::new(from.x, to.y);
    
    // Check vertical segment
    let v_start_y = from.y.min(intermediate.y);
    let v_end_y = from.y.max(intermediate.y);
    
    // Check horizontal segment  
    let h_start_x = intermediate.x.min(to.x);
    let h_end_x = intermediate.x.max(to.x);
    
    for obstacle_center in obstacles {
        let rect_left = obstacle_center.x - comp_width / 2.0;
        let rect_right = obstacle_center.x + comp_width / 2.0;
        let rect_top = obstacle_center.y - comp_height / 2.0;
        let rect_bottom = obstacle_center.y + comp_height / 2.0;
        
        // Check if vertical segment intersects
        if crate::geometry::line_segment_intersects_rectangle(
            from.x, v_start_y, from.x, v_end_y,
            rect_left, rect_top, rect_right, rect_bottom
        ) {
            return true;
        }
        
        // Check if horizontal segment intersects
        if crate::geometry::line_segment_intersects_rectangle(
            h_start_x, intermediate.y, h_end_x, intermediate.y,
            rect_left, rect_top, rect_right, rect_bottom
        ) {
            return true;
        }
    }
    false
} 