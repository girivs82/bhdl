//! Layout definition data structures for PCB footprint metadata.
//!
//! Currently only stores package name; will be extended with
//! pad geometry and thermal relief data in later phases.

use serde::{Serialize, Deserialize};

/// Layout definition for an entity (PCB footprint metadata).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutDefinition {
    pub entity_name: String,
    pub package: Option<String>,
    /// Board-level layer count from `layer_stackup N;` — a declared
    /// stackup is an INPUT to PnR, not something routing discovers.
    pub layer_stackup: Option<usize>,
    /// MECHANICAL CONTRACT — chassis-locked truth PnR works within:
    /// (handle, x, y, rot_deg) locked part positions.
    pub places: Vec<(String, f64, f64, f64)>,
    /// (handle, x0, y0, x1, y1) region-constrained parts (thermal
    /// bosses: part must sit WITHIN the zone, position otherwise free).
    pub region_places: Vec<(String, f64, f64, f64, f64)>,
    /// Declared outline: Rect(w,h) as (w, h, empty) or Polygon points.
    pub outline_rect: Option<(f64, f64)>,
    pub outline_polygon: Option<Vec<(f64, f64)>>,
    /// (x, y, drill, keepout) plated-free mounting holes.
    pub mounting_holes: Vec<(f64, f64, f64, f64)>,
    /// (x0, y0, x1, y1) rectangular keepouts (chassis bosses etc.).
    pub keepouts: Vec<(f64, f64, f64, f64)>,
}
