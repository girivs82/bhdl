// Declare modules
pub mod drawing;
pub mod symbols;
pub mod layout;

// Re-export public API
pub use drawing::visualize_netlist;

// Removed old add function and inline tests
