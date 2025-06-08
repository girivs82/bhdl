use crate::layout::types::Point;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Checks if a point lies inside a rectangle defined by its center, dimensions, and rotation angle.
pub fn is_point_inside_rotated_rect(
    point: Point,
    rect_center: Point,
    rect_width: f64,
    rect_height: f64,
    rect_angle_rad: f64,
) -> bool {
    // Translate point relative to the rectangle's center
    let translated_point = point - rect_center;

    // Rotate the translated point backwards by the rectangle's angle
    let cos_a = rect_angle_rad.cos();
    let sin_a = rect_angle_rad.sin();
    // Note: Use inverse rotation (negative angle) -> cos(-a)=cos(a), sin(-a)=-sin(a)
    let rotated_back_x = translated_point.x * cos_a + translated_point.y * sin_a;
    let rotated_back_y = -translated_point.x * sin_a + translated_point.y * cos_a;

    // Check if the rotated-back point is within the axis-aligned bounds
    let half_width = rect_width / 2.0;
    let half_height = rect_height / 2.0;

    rotated_back_x >= -half_width
        && rotated_back_x <= half_width
        && rotated_back_y >= -half_height
        && rotated_back_y <= half_height
}

pub fn manhattan_distance(p1: Point, p2: Point) -> f64 {
    (p1.x - p2.x).abs() + (p1.y - p2.y).abs()
}

pub fn simplify_world_path(path: &[Point]) -> Vec<Point> {
    if path.len() <= 2 {
        return path.to_vec();
    }

    let mut simplified = Vec::new();
    simplified.push(path[0]);

    let mut i = 1;
    while i < path.len() - 1 {
        let prev = simplified.last().unwrap();
        let curr = &path[i];
        let next = &path[i + 1];

        // Calculate direction vectors
        let dir1 = Point::new(curr.x - prev.x, curr.y - prev.y);
        let dir2 = Point::new(next.x - curr.x, next.y - curr.y);

        // Check if directions are the same (collinear)
        // Use cross product to check if vectors are parallel
        let cross = dir1.x * dir2.y - dir1.y * dir2.x;

        if cross.abs() > 1e-10 {
            // Not collinear, keep this point
            simplified.push(*curr);
        }
        // If collinear, skip this point

        i += 1;
    }

    simplified.push(path[path.len() - 1]);
    simplified
}

/// Helper struct for Prim's Algorithm
#[derive(Copy, Clone, PartialEq)]
pub struct PrimEdge {
    pub weight: f64,
    pub to_node: usize,    // Index into the list of points
    pub from_node: usize,  // Index of the node this edge connects from in the MST
}

impl PartialOrd for PrimEdge {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // Reverse comparison for min-heap behavior in BinaryHeap (which is a max-heap by default)
        other.weight.partial_cmp(&self.weight)
    }
}

impl Ord for PrimEdge {
    fn cmp(&self, other: &Self) -> Ordering {
        // Use total_cmp for f64 which handles NaN properly
        other.weight.total_cmp(&self.weight)
    }
}

impl Eq for PrimEdge {}

pub fn find_mst_prim(points: &[Point]) -> Vec<(usize, usize)> {
    if points.len() <= 1 {
        return Vec::new();
    }

    let n = points.len();
    let mut in_mst = vec![false; n];
    let mut edges = Vec::new();
    let mut heap = BinaryHeap::new();

    // Start with node 0
    in_mst[0] = true;

    // Add all edges from node 0 to the heap
    for i in 1..n {
        let weight = manhattan_distance(points[0], points[i]);
        heap.push(PrimEdge {
            weight,
            to_node: i,
            from_node: 0,
        });
    }

    // Build MST
    while let Some(edge) = heap.pop() {
        if in_mst[edge.to_node] {
            continue; // Already in MST
        }

        // Add this edge to MST
        edges.push((edge.from_node, edge.to_node));
        in_mst[edge.to_node] = true;

        // Add all edges from the new node to nodes not in MST
        for i in 0..n {
            if !in_mst[i] {
                let weight = manhattan_distance(points[edge.to_node], points[i]);
                heap.push(PrimEdge {
                    weight,
                    to_node: i,
                    from_node: edge.to_node,
                });
            }
        }
    }

    edges
} 