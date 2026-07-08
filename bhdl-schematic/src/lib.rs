//! BHDL Schematic Viewer — Rust extraction layer + HTML/Canvas renderer.
//!
//! This crate provides two public functions:
//! - `extract_schematic_data()` — converts a BHDL `Netlist` to a JSON-serializable `SchematicData`

pub mod types;
pub mod extract;
pub mod refdes;
pub mod sub_layout;
pub mod v4;

pub use types::*;
pub use extract::extract_schematic_data;
pub use refdes::{RefDesLut, category_to_prefix};
pub use sub_layout::{compute_expansion_sub_schematic, compute_cap_bank_sub_schematic, CapBankMember};
