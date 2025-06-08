use std::collections::HashMap;
use std::ops::{Add, Sub, Mul, Div, AddAssign, DivAssign};
use bhdl_netlist::{InstanceId, PinId, NetId};

// Point struct (using f64)
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }
    
    pub fn magnitude_sq(&self) -> f64 {
        self.x * self.x + self.y * self.y
    }
    
    pub fn magnitude(&self) -> f64 {
        self.magnitude_sq().sqrt()
    }
    
    pub fn normalize(&self) -> Self {
        let mag = self.magnitude();
        if mag == 0.0 {
            Point::default()
        } else {
            *self / mag
        }
    }
}

impl Add for Point {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Point {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl Sub for Point {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Point {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

impl Mul<f64> for Point {
    type Output = Self;
    fn mul(self, scalar: f64) -> Self {
        Point {
            x: self.x * scalar,
            y: self.y * scalar,
        }
    }
}

impl Mul<Point> for f64 {
    type Output = Point;
    fn mul(self, point: Point) -> Point {
        point * self
    }
}

impl Div<f64> for Point {
    type Output = Self;
    fn div(self, scalar: f64) -> Self {
        if scalar == 0.0 {
            return Point::default();
        }
        Point {
            x: self.x / scalar,
            y: self.y / scalar,
        }
    }
}

impl AddAssign for Point {
    fn add_assign(&mut self, other: Self) {
        *self = Self {
            x: self.x + other.x,
            y: self.y + other.y,
        };
    }
}

impl DivAssign<f64> for Point {
    fn div_assign(&mut self, scalar: f64) {
        if scalar == 0.0 {
            self.x = 0.0;
            self.y = 0.0;
        } else {
            self.x /= scalar;
            self.y /= scalar;
        }
    }
}

#[derive(Debug, Clone)]
pub struct ComponentLayout {
    pub center_x: f64,
    pub center_y: f64,
    pub rotation: f64,
    pub relative_pin_locations: HashMap<PinId, Point>,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Default)]
pub struct NetLayout {
    pub segments: Vec<(Point, Point)>,
}

#[derive(Debug, Clone)]
pub struct BoundingBox {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl BoundingBox {
    pub fn new() -> Self {
        BoundingBox {
            min_x: f64::INFINITY,
            min_y: f64::INFINITY,
            max_x: f64::NEG_INFINITY,
            max_y: f64::NEG_INFINITY,
        }
    }

    pub fn update(&mut self, x: f64, y: f64) {
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }

    pub fn update_with_component(&mut self, layout: &ComponentLayout) {
        let w = layout.width / 2.0;
        let h = layout.height / 2.0;
        let cx = layout.center_x;
        let cy = layout.center_y;
        let angle = layout.rotation.to_radians();
        let cos_a = angle.cos();
        let sin_a = angle.sin();
        let corners = [
            Point::new(-w, -h),
            Point::new(w, -h),
            Point::new(w, h),
            Point::new(-w, h),
        ];
        for corner in corners {
            let rotated_x = cx + corner.x * cos_a - corner.y * sin_a;
            let rotated_y = cy + corner.x * sin_a + corner.y * cos_a;
            self.update(rotated_x, rotated_y);
        }
        for (_, pin_rel_pos) in &layout.relative_pin_locations {
            let rotated_pin_x = cx + pin_rel_pos.x * cos_a - pin_rel_pos.y * sin_a;
            let rotated_pin_y = cy + pin_rel_pos.x * sin_a + pin_rel_pos.y * cos_a;
            self.update(rotated_pin_x, rotated_pin_y);
        }
    }

    pub fn update_with_net(&mut self, net_layout: &NetLayout) {
        for (p1, p2) in &net_layout.segments {
            self.update(p1.x, p1.y);
            self.update(p2.x, p2.y);
        }
    }

    pub fn add_padding(&mut self, padding: f64) {
        if self.min_x.is_finite() && self.max_x.is_finite() {
            self.min_x -= padding;
            self.max_x += padding;
        }
        if self.min_y.is_finite() && self.max_y.is_finite() {
            self.min_y -= padding;
            self.max_y += padding;
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlacementNode {
    pub id: InstanceId,
    pub position: Point,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
    pub radius: f64,
    pub net_force: Point,
}

pub struct LayoutResult {
    pub component_positions: HashMap<InstanceId, Point>,
} 