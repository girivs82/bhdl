use serde::{Serialize, Deserialize};
use std::fmt;

// Removed old placeholder add function
// pub fn add(left: u64, right: u64) -> u64 {
//     left + right
// }

// Removed old inline tests
// #[cfg(test)]
// mod tests {
//     use super::*;
// 
//     #[test]
//     fn it_works() {
//         let result = add(2, 2);
//         assert_eq!(result, 4);
//     }
// }

// Removed old definitions (moved to separate files)
// #[derive(Debug, Default, Serialize, Deserialize)]
// pub struct Netlist { ... }

// #[derive(Debug, Serialize, Deserialize)]
// pub struct ModuleDefinition { ... }

// #[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Copy)]
// pub enum ModuleKind { ... }

// #[derive(Debug, Serialize, Deserialize)]
// pub struct Instance { ... }

// #[derive(Debug, Serialize, Deserialize)]
// pub struct Port { ... }

// #[derive(Debug, Serialize, Deserialize)]
// pub struct Pin { ... }

// #[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Copy)]
// pub enum PortDirection { ... }

// #[derive(Debug, Serialize, Deserialize)]
// pub struct Net { ... }

// #[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Copy)]
// pub enum ConnectionPoint { ... }

// impl Netlist { // Old impl
//     pub fn new() -> Self {
//         Self::default()
//     }
//     // ... other methods ...
// }

// #[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
// pub struct ComponentInstance { ... }

// #[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
// pub struct PinRef { ... }

// #[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
// pub struct Netlist { // Very old definition
//     pub top_design_name: String,
//     pub instances: Vec<ComponentInstance>,
//     pub nets: Vec<Net>,
// }

// impl Netlist { // Impl for very old definition
//     pub fn new(top_design_name: String) -> Self {
//         Netlist {
//             top_design_name,
//             instances: Vec::new(),
//             nets: Vec::new(),
//         }
//     }
// }

// #[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
// pub enum Unit { ... }

// impl fmt::Display for Unit { ... }

// #[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
// pub struct Quantity { ... }

// impl fmt::Display for Quantity { ... }


// Declare modules
pub mod types;
pub mod definition;
pub mod instance;
pub mod portpin;
pub mod net;
pub mod netlist;

// Re-export key types and IDs from the types module
pub use types::{*}; // Continue re-exporting everything from types

pub use definition::ModuleDefinition;
pub use instance::Instance;
pub use portpin::{Port, Pin};
pub use net::Net;
pub use netlist::Netlist;

// Load tests from the tests/ directory
#[cfg(test)]
mod tests;
