// Main layout module - coordinates all layout functionality
pub mod types;
pub mod engine;
pub mod placement;
pub mod routing;
pub mod semantic;
pub mod utils;

// Re-export the main public API
pub use engine::LayoutEngine;
pub use types::{Point, ComponentLayout, NetLayout, BoundingBox, LayoutResult};
pub use semantic::{CircuitPattern, SemanticAnalyzer, SemanticLayoutEngine, SemanticLayoutConstraints}; 