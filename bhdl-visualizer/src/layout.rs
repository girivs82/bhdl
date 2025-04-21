use std::collections::HashMap;
use bhdl_netlist::{InstanceId, PinId};

// Simple point struct for coordinates
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Point { x, y }
    }
}

/// Stores the calculated layout information for a single component instance.
#[derive(Debug, Clone)]
pub struct InstanceLayout {
    /// The center coordinates of the instance symbol.
    pub center: Point,
    /// The calculated absolute coordinates of each pin, mapping PinId to Point.
    /// Using PinId assumes we primarily connect to physical pins in the final schematic.
    /// We might need to handle PortId connections later, especially for hierarchical blocks.
    pub pin_locations: HashMap<PinId, Point>,
    // Could add bounding box, rotation, etc. later
}

/// Represents the overall layout, mapping instance IDs to their specific layout info.
#[derive(Debug, Default)]
pub struct Layout {
    pub instances: HashMap<InstanceId, InstanceLayout>,
    // Could add overall dimensions, grid info, etc. later
}

impl Layout {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_instance(&mut self, id: InstanceId, layout: InstanceLayout) {
        self.instances.insert(id, layout);
    }

    pub fn get_instance_layout(&self, id: InstanceId) -> Option<&InstanceLayout> {
        self.instances.get(&id)
    }

    /// Helper to get the absolute coordinates of a specific pin on a specific instance.
    pub fn get_pin_location(&self, instance_id: InstanceId, pin_id: PinId) -> Option<Point> {
        self.instances.get(&instance_id)
            .and_then(|layout| layout.pin_locations.get(&pin_id))
            .copied() // Copy the Point
    }
}
