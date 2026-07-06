//! Schematic V4 — idiom composition, not graph layout
//! (docs/spec/Schematic_V4.md). Layout lives in Rust: this module turns a
//! netlist into an electrical PLAN (rails, stage backbones, shunts,
//! feedback loops, residue) that the composer lays onto a grid and the SVG
//! renderer draws. The classifier never guesses — anything it cannot
//! idiomize lands in `residue` and is drawn in the honest fallback grid
//! and counted in the render's absence ledger.

pub mod classify;
pub mod svg;

pub use svg::render_sheet_svg;
pub use classify::{classify_sheet, SheetPlan, StagePlan, BackboneElem, Shunt, LoopChain, Strap, LoadPlan};
