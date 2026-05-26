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
pub mod lib_resolver;
pub mod symbol_mapping;
pub mod nets;
pub mod emitter;
pub mod canonical;

pub use ir::*;
pub use reader::{read_schematic, read_from_str, ReadError};
pub use sexpr::{Sexpr, ParseError};
pub use lib_resolver::{LibraryResolver, SymLibTableEntry,
    parse_sym_lib_table_str, parse_sym_lib_table_file,
    parse_kicad_sym_str, parse_kicad_sym_file};
pub use symbol_mapping::{MappingRegistry, SymbolMapping, MappingError};
pub use nets::{extract_nets, Net, NetList, NetPin};
pub use emitter::{emit_bhdl, emit_bhdl_with_options, EmittedBhdl, EmitError, EmitOptions};
pub use canonical::{
    canonical_from_schematic, canonical_from_schematic_with_mapping,
    parse_kicad_net_file, compare,
    CanonicalNetlist, PinRef, NetDiff, EquivalenceReport,
};
