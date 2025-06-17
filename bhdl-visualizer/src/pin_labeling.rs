//! Intelligent pin labeling system for circuit visualization

use crate::types::{Point, BoundingBox};

/// Pin labeling configuration
#[derive(Debug, Clone)]
pub struct PinLabelConfig {
    /// Minimum font size for pin labels
    pub min_font_size: f64,
    /// Maximum font size for pin labels
    pub max_font_size: f64,
    /// Preferred font size for pin numbers
    pub pin_number_size: f64,
    /// Preferred font size for pin names
    pub pin_name_size: f64,
    /// Offset from pin for numbers
    pub number_offset: f64,
    /// Offset from symbol for names
    pub name_offset: f64,
    /// Whether to show pin numbers
    pub show_numbers: bool,
    /// Whether to show pin names
    pub show_names: bool,
    /// Auto-hide overlapping labels
    pub auto_hide_overlaps: bool,
}

impl Default for PinLabelConfig {
    fn default() -> Self {
        Self {
            min_font_size: 4.0,
            max_font_size: 8.0,
            pin_number_size: 5.0,
            pin_name_size: 6.0,
            number_offset: 3.0,
            name_offset: 20.0,  // Increased distance from symbol
            show_numbers: false,  // Only show numbers for complex ICs
            show_names: true,     // Show descriptive names
            auto_hide_overlaps: true,
        }
    }
}

/// Pin direction for label positioning
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PinDirection {
    Left,
    Right,
    Top,
    Bottom,
}

impl PinDirection {
    /// Determine pin direction based on pin position relative to symbol center
    pub fn from_positions(pin_pos: Point, symbol_center: Point) -> Self {
        let dx = pin_pos.x - symbol_center.x;
        let dy = pin_pos.y - symbol_center.y;
        
        if dx.abs() > dy.abs() {
            if dx > 0.0 { PinDirection::Right } else { PinDirection::Left }
        } else {
            if dy > 0.0 { PinDirection::Bottom } else { PinDirection::Top }
        }
    }
    
    /// Get text anchor for this direction
    pub fn text_anchor(&self) -> &'static str {
        match self {
            PinDirection::Left => "end",
            PinDirection::Right => "start",
            PinDirection::Top => "middle",
            PinDirection::Bottom => "middle",
        }
    }
    
    /// Get rotation angle for vertical text
    pub fn rotation(&self) -> f64 {
        match self {
            PinDirection::Top | PinDirection::Bottom => -90.0,
            _ => 0.0,
        }
    }
}

/// Pin label positioning calculator
pub struct PinLabelPositioner {
    config: PinLabelConfig,
}

impl PinLabelPositioner {
    pub fn new(config: PinLabelConfig) -> Self {
        Self { config }
    }
    
    /// Calculate positions for pin number and name
    pub fn calculate_label_positions(
        &self,
        pin_pos: Point,
        pin_name: &str,
        pin_number: Option<&str>,
        symbol_bounds: &BoundingBox,
        pin_direction: PinDirection,
    ) -> PinLabelLayout {
        let mut layout = PinLabelLayout {
            number_pos: None,
            number_size: self.config.pin_number_size,
            number_anchor: "middle",
            name_pos: None,
            name_size: self.config.pin_name_size,
            name_anchor: pin_direction.text_anchor(),
            show_number: false,
            show_name: false,
        };
        
        // Calculate pin number position (close to pin)
        if let Some(num) = pin_number {
            if self.config.show_numbers && !num.is_empty() {
                let offset = self.config.number_offset;
                layout.number_pos = Some(self.calculate_number_position(pin_pos, pin_direction, offset));
                layout.show_number = true;
            }
        }
        
        // Calculate pin name position (outside symbol bounds)
        if self.config.show_names && !pin_name.is_empty() && pin_name != "~" {
            let (name_pos, name_size) = self.calculate_name_position(
                pin_pos,
                pin_name,
                symbol_bounds,
                pin_direction,
            );
            layout.name_pos = Some(name_pos);
            layout.name_size = name_size;
            layout.show_name = true;
        }
        
        layout
    }
    
    /// Calculate position for pin number (close to pin)
    fn calculate_number_position(&self, pin_pos: Point, direction: PinDirection, offset: f64) -> Point {
        match direction {
            PinDirection::Left => pin_pos.translate(-offset, 0.0),
            PinDirection::Right => pin_pos.translate(offset, 0.0),
            PinDirection::Top => pin_pos.translate(0.0, -offset),
            PinDirection::Bottom => pin_pos.translate(0.0, offset),
        }
    }
    
    /// Calculate position and size for pin name (outside symbol)
    fn calculate_name_position(
        &self,
        pin_pos: Point,
        pin_name: &str,
        symbol_bounds: &BoundingBox,
        direction: PinDirection,
    ) -> (Point, f64) {
        // Calculate base offset from symbol edge
        let symbol_edge = match direction {
            PinDirection::Left => symbol_bounds.min_x,
            PinDirection::Right => symbol_bounds.max_x,
            PinDirection::Top => symbol_bounds.min_y,
            PinDirection::Bottom => symbol_bounds.max_y,
        };
        
        // Position name outside symbol with appropriate clearance
        let clearance = self.config.name_offset;
        let name_pos = match direction {
            PinDirection::Left => Point::new(symbol_edge - clearance, pin_pos.y),
            PinDirection::Right => Point::new(symbol_edge + clearance, pin_pos.y),
            PinDirection::Top => Point::new(pin_pos.x, symbol_edge - clearance),
            PinDirection::Bottom => Point::new(pin_pos.x, symbol_edge + clearance),
        };
        
        // Calculate adaptive text size based on available space
        let text_width = pin_name.len() as f64 * self.config.pin_name_size * 0.6; // Approximate
        let available_space = match direction {
            PinDirection::Left | PinDirection::Right => 30.0, // Horizontal space estimate
            PinDirection::Top | PinDirection::Bottom => 20.0, // Vertical space estimate
        };
        
        let size_factor = (available_space / text_width).min(1.0).max(0.5);
        let final_size = (self.config.pin_name_size * size_factor)
            .max(self.config.min_font_size)
            .min(self.config.max_font_size);
        
        (name_pos, final_size)
    }
}

/// Layout result for pin labels
#[derive(Debug, Clone)]
pub struct PinLabelLayout {
    pub number_pos: Option<Point>,
    pub number_size: f64,
    pub number_anchor: &'static str,
    pub name_pos: Option<Point>,
    pub name_size: f64,
    pub name_anchor: &'static str,
    pub show_number: bool,
    pub show_name: bool,
}

/// Detect overlapping labels in a collection
pub fn detect_label_overlaps(labels: &[LabelInfo]) -> Vec<(usize, usize)> {
    let mut overlaps = Vec::new();
    
    for (i, label1) in labels.iter().enumerate() {
        for (j, label2) in labels.iter().enumerate().skip(i + 1) {
            if labels_overlap(label1, label2) {
                overlaps.push((i, j));
            }
        }
    }
    
    overlaps
}

/// Check if two labels overlap
fn labels_overlap(label1: &LabelInfo, label2: &LabelInfo) -> bool {
    let padding = 2.0; // Small padding between labels
    
    let l1_left = label1.position.x - label1.width / 2.0 - padding;
    let l1_right = label1.position.x + label1.width / 2.0 + padding;
    let l1_top = label1.position.y - label1.height / 2.0 - padding;
    let l1_bottom = label1.position.y + label1.height / 2.0 + padding;
    
    let l2_left = label2.position.x - label2.width / 2.0 - padding;
    let l2_right = label2.position.x + label2.width / 2.0 + padding;
    let l2_top = label2.position.y - label2.height / 2.0 - padding;
    let l2_bottom = label2.position.y + label2.height / 2.0 + padding;
    
    !(l1_right < l2_left || l2_right < l1_left || l1_bottom < l2_top || l2_bottom < l1_top)
}

/// Information about a label for overlap detection
#[derive(Debug, Clone)]
pub struct LabelInfo {
    pub position: Point,
    pub width: f64,
    pub height: f64,
    pub priority: i32, // Higher priority labels are kept when overlapping
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pin_direction_detection() {
        let center = Point::new(0.0, 0.0);
        
        assert_eq!(PinDirection::from_positions(Point::new(10.0, 0.0), center), PinDirection::Right);
        assert_eq!(PinDirection::from_positions(Point::new(-10.0, 0.0), center), PinDirection::Left);
        assert_eq!(PinDirection::from_positions(Point::new(0.0, 10.0), center), PinDirection::Bottom);
        assert_eq!(PinDirection::from_positions(Point::new(0.0, -10.0), center), PinDirection::Top);
    }
    
    #[test]
    fn test_label_overlap_detection() {
        let labels = vec![
            LabelInfo {
                position: Point::new(0.0, 0.0),
                width: 10.0,
                height: 5.0,
                priority: 1,
            },
            LabelInfo {
                position: Point::new(8.0, 0.0),
                width: 10.0,
                height: 5.0,
                priority: 1,
            },
            LabelInfo {
                position: Point::new(20.0, 0.0),
                width: 10.0,
                height: 5.0,
                priority: 1,
            },
        ];
        
        let overlaps = detect_label_overlaps(&labels);
        assert_eq!(overlaps.len(), 1);
        assert_eq!(overlaps[0], (0, 1));
    }
}