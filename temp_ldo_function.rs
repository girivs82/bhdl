/// Helper function to calculate LDO pin locations with proper pin positioning
/// VIN: left side, VOUT: right side, GND: bottom, EN: top
pub fn calculate_ldo_pin_locations(box_width: f64, box_height: f64, pins: &[Pin]) -> HashMap<String, Point> {
    let mut locations = HashMap::new();
    let half_width = box_width / 2.0;
    let half_height = box_height / 2.0;
    
    for pin in pins {
        let point = match pin.name.to_uppercase().as_str() {
            "VIN" => Point::new(-half_width, 0.0),     // Left side
            "VOUT" => Point::new(half_width, 0.0),     // Right side
            "GND" => Point::new(0.0, half_height),     // Bottom
            "EN" => Point::new(0.0, -half_height),     // Top
            _ => Point::new(-half_width, 0.0),         // Default to left side
        };
        locations.insert(pin.name.clone(), point);
    }
    
    locations
} 