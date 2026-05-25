//! KiCad 6+ schematic reader.
//!
//! Phase A of the KiCad-to-BHDL translator pipeline (see
//! `docs/plan/KiCad_Import_And_Stdlib_Accretion.md`).
//!
//! Public entry points:
//!
//! - [`read_schematic`] — Read a `.kicad_sch` file from disk,
//!   follow hierarchical sheet references, return the full
//!   [`Schematic`].
//! - [`read_from_str`] — Parse a single sheet from a source
//!   string (used by tests).
//!
//! See [`Schematic`] and [`Sheet`] for the IR shape this produces.
//! The output IR is consumed by later phases (B–D in the plan):
//! library-symbol resolution, net topology extraction, BHDL
//! emission.
//!
//! # Example
//!
//! ```no_run
//! use bhdl_kicad_import::read_schematic;
//! use std::path::Path;
//!
//! let schematic = read_schematic(Path::new("arduino_uno.kicad_sch"))?;
//! println!("Read {} symbols, {} wires, {} children",
//!     schematic.root.symbols.len(),
//!     schematic.root.wires.len(),
//!     schematic.child_sheets.len());
//! # Ok::<(), bhdl_kicad_import::ReadError>(())
//! ```

pub mod sexpr;
pub mod ir;
pub mod reader;

pub use ir::*;
pub use reader::{read_schematic, read_from_str, ReadError};
pub use sexpr::{Sexpr, ParseError};
