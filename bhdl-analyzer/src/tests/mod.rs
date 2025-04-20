// Declare test modules within the tests/ directory

// Common utilities, not a test module itself
#[allow(dead_code)] // Allow unused items in common if tests are filtered
pub mod common;

// Test modules
mod assign_conn_types;
mod basic_analysis;
mod binary_expr;
mod bounds_checks;
mod component_refs;
mod constant_eval;
mod directionality;
mod ident_refs;
mod pin_refs;
mod type_refs; 