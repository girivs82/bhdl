/// Check if a line segment intersects with a rectangle
pub fn line_segment_intersects_rectangle(
    x1: f64, y1: f64, x2: f64, y2: f64,          // line segment endpoints
    rect_left: f64, rect_top: f64, rect_right: f64, rect_bottom: f64  // rectangle bounds
) -> bool {
    // Check if either endpoint is inside the rectangle
    if point_in_rectangle(x1, y1, rect_left, rect_top, rect_right, rect_bottom) ||
       point_in_rectangle(x2, y2, rect_left, rect_top, rect_right, rect_bottom) {
        return true;
    }
    
    // Check if line segment intersects any of the four rectangle edges
    // Top edge
    if line_segments_intersect(x1, y1, x2, y2, rect_left, rect_top, rect_right, rect_top) {
        return true;
    }
    // Bottom edge  
    if line_segments_intersect(x1, y1, x2, y2, rect_left, rect_bottom, rect_right, rect_bottom) {
        return true;
    }
    // Left edge
    if line_segments_intersect(x1, y1, x2, y2, rect_left, rect_top, rect_left, rect_bottom) {
        return true;
    }
    // Right edge
    if line_segments_intersect(x1, y1, x2, y2, rect_right, rect_top, rect_right, rect_bottom) {
        return true;
    }
    
    false
}

/// Check if a point is inside a rectangle
pub fn point_in_rectangle(x: f64, y: f64, rect_left: f64, rect_top: f64, rect_right: f64, rect_bottom: f64) -> bool {
    x >= rect_left && x <= rect_right && y >= rect_top && y <= rect_bottom
}

/// Check if two line segments intersect
fn line_segments_intersect(x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64, x4: f64, y4: f64) -> bool {
    let denom = (x1 - x2) * (y3 - y4) - (y1 - y2) * (x3 - x4);
    if denom.abs() < 1e-10 {
        return false; // Lines are parallel
    }
    
    let t = ((x1 - x3) * (y3 - y4) - (y1 - y3) * (x3 - x4)) / denom;
    let u = -((x1 - x2) * (y1 - y3) - (y1 - y2) * (x1 - x3)) / denom;
    
    t >= 0.0 && t <= 1.0 && u >= 0.0 && u <= 1.0
}

/// Check if two rectangles intersect
pub fn rectangles_intersect(
    left1: f64, top1: f64, right1: f64, bottom1: f64,
    left2: f64, top2: f64, right2: f64, bottom2: f64
) -> bool {
    !(right1 < left2 || left1 > right2 || bottom1 < top2 || top1 > bottom2)
} 