//! Board SKU variants — analyzer-extracted patch sets.
//!
//! See `docs/spec/Board_SKU_Variants.md`. A board may declare
//! `variant <Name> { ... }` blocks; each block is a *patch* on the
//! base design's literals: per-instance value overrides and DNP
//! flags. The analyzer extracts one [`Variant`] per declared
//! variant; the synthesizer applies the patches in the user-selected
//! SKU before BOM walk / SPICE export.
//!
//! v0.1 surface: DNP + value override only.

use std::collections::{HashMap, HashSet};

/// A single board variant — one shipping SKU's worth of patches.
#[derive(Debug, Clone)]
pub struct Variant {
    /// Variant name as declared in `variant <Name> { ... }`.
    pub name: String,
    /// Instance-name → new value (as raw source text, re-parsed by
    /// the synthesizer when applying). Mirrors the design-recipe
    /// extraction shape (raw text re-parsed at evaluation time).
    pub value_overrides: HashMap<String, String>,
    /// Instances flagged DNP for this variant. The instance stays
    /// in the netlist but is marked so the BOM walker / pick-place
    /// export skip it.
    pub dnp: HashSet<String>,
}

impl Variant {
    pub fn new(name: String) -> Self {
        Self {
            name,
            value_overrides: HashMap::new(),
            dnp: HashSet::new(),
        }
    }

    /// True if this variant has no patches — i.e. it produces the
    /// base design unchanged. Useful for the analyzer's diagnostic
    /// "this variant is empty; did you mean to remove it?"
    pub fn is_empty(&self) -> bool {
        self.value_overrides.is_empty() && self.dnp.is_empty()
    }
}
