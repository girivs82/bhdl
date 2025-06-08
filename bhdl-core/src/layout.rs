use crate::core::PinId;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub struct InstanceLayout {
    pub center: Point,
    pub pin_locations: HashMap<PinId, Point>,
}

#[derive(Debug)]
struct PlacementNode {
    instance_id: InstanceId,
    module_id: ModuleId,
    center: Point,
    velocity: Vector,
    size: Size,
    is_fixed: bool,
    relative_pin_locations: HashMap<String, Point>,
}

impl PlacementNode {
    fn new(instance_id: InstanceId, module_id: ModuleId, center: Point, size: Size, is_fixed: bool) -> Self {
        PlacementNode {
            instance_id,
            module_id,
            center,
            velocity: Vector::new(0.0, 0.0),
            size,
            is_fixed,
            relative_pin_locations: HashMap::new(),
        }
    }
}

impl Layout {
    pub fn new(module_id: ModuleId) -> Self {
        Layout {
            module_id,
            instance_layouts: HashMap::new(),
            connection_routes: Vec::new(),
        }
    }

    pub fn add_instance_layout(&mut self, instance_id: InstanceId, layout: InstanceLayout) {
        self.instance_layouts.insert(instance_id, layout);
    }

    pub fn get_instance_layout(&self, instance_id: InstanceId) -> Option<&InstanceLayout> {
        self.instance_layouts.get(&instance_id)
    }

    pub fn add_connection_route(&mut self, route: Vec<Point>) {
        self.connection_routes.push(route);
    }

    pub fn get_instance_layouts(&self) -> &HashMap<InstanceId, InstanceLayout> {
        &self.instance_layouts
    }

    pub fn get_connection_routes(&self) -> &Vec<Vec<Point>> {
        &self.connection_routes
    }
} 