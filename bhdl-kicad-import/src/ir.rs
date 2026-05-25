//! Typed IR for a KiCad 6+ schematic.
//!
//! The S-expression parser in [`crate::sexpr`] produces a generic
//! tree; this module is the KiCad-specific shape we project that
//! tree onto. Every construct we need to translate to BHDL has a
//! Rust type here.
//!
//! The IR is deliberately *flat* per sheet — wires, labels,
//! symbols, etc. are sibling Vecs rather than a hierarchical
//! tree. The cross-cutting structure (which pins are on which
//! net, which sheets are children of which) is computed by
//! later phases (see `net_extraction.rs` in Phase C).

use std::collections::HashMap;
use std::path::PathBuf;

/// A full schematic — possibly hierarchical. The top sheet plus a
/// map of child sheets keyed by their on-disk path.
#[derive(Debug, Clone)]
pub struct Schematic {
    /// The top-level sheet (the `.kicad_sch` file the user
    /// imported).
    pub root: Sheet,
    /// Children referenced by hierarchical-sheet symbols in the
    /// root or in other children. Keyed by their path *relative
    /// to the root's directory* so the data is portable.
    pub child_sheets: HashMap<PathBuf, Sheet>,
    /// The format version reported by the file (e.g. 20231120).
    /// Used to gate format-version-specific handling.
    pub version: u32,
    /// The tool that generated the file ("eeschema" usually).
    pub generator: String,
}

/// One sheet's worth of schematic content.
#[derive(Debug, Clone, Default)]
pub struct Sheet {
    /// Path of the sheet file relative to the project root (for the
    /// root sheet, the file name; for children, the relative path).
    pub path: PathBuf,
    /// UUID of this sheet (KiCad assigns a stable UUID per sheet).
    pub uuid: String,
    /// The sheet's title-block metadata (title, date, rev, etc.) —
    /// optional, mostly informational.
    pub title_block: Option<TitleBlock>,
    /// Library symbol definitions used in this sheet. Embedded in
    /// the schematic file by KiCad to make it self-contained even
    /// if the user moves libraries around.
    pub lib_symbols: Vec<LibSymbol>,
    /// Component instances (resistors, ICs, etc.).
    pub symbols: Vec<SchematicSymbol>,
    /// Wires between points.
    pub wires: Vec<Wire>,
    /// Explicit junctions (where wires merge).
    pub junctions: Vec<Junction>,
    /// `no_connect` markers.
    pub no_connects: Vec<NoConnect>,
    /// Local labels.
    pub labels: Vec<Label>,
    /// Global labels (span all sheets).
    pub global_labels: Vec<GlobalLabel>,
    /// Hierarchical labels (cross sheet boundaries).
    pub hierarchical_labels: Vec<HierarchicalLabel>,
    /// Power flag symbols (+5V, GND, etc.).
    pub power_symbols: Vec<PowerSymbol>,
    /// References to child sheets (hierarchical instantiations).
    pub sheet_refs: Vec<SheetRef>,
}

/// Title block metadata. Mostly informational; we extract it for
/// completeness and to populate BHDL board comments.
#[derive(Debug, Clone, Default)]
pub struct TitleBlock {
    pub title: Option<String>,
    pub date: Option<String>,
    pub rev: Option<String>,
    pub company: Option<String>,
    pub comments: Vec<String>,
}

/// A library symbol declaration embedded in the schematic.
/// Carries the pin list and electrical types we need for net
/// topology. The full graphical content (lines, rectangles,
/// arcs) is not modelled — purely a netlist concern.
#[derive(Debug, Clone)]
pub struct LibSymbol {
    /// Library reference: `Device:R`, `MCU_ST_STM32F4:STM32F411RETx`.
    pub lib_id: String,
    /// Pin definitions for this symbol. Multi-unit ICs have all
    /// pins listed; the per-unit assignment is on each pin via
    /// [`Pin::unit_index`].
    pub pins: Vec<LibPin>,
    /// Number of distinct units this symbol covers (1 for a normal
    /// single-unit component, 2+ for a multi-gate IC).
    pub unit_count: u32,
    /// Custom properties at the library level (rare but possible).
    pub properties: HashMap<String, String>,
}

/// Pin definition inside a library symbol.
#[derive(Debug, Clone)]
pub struct LibPin {
    /// Pin number as it appears in the symbol — usually "1", "2",
    /// "VCC", "GND", etc. KiCad sometimes uses bareword names.
    pub number: String,
    /// Human-readable name (`VDD`, `RESET`, `~`).  The `~` is the
    /// KiCad convention for "no name".
    pub name: String,
    /// Electrical type: input, output, bidirectional, power_in,
    /// power_out, passive, …
    pub electrical_type: PinElectricalType,
    /// Which unit of a multi-unit symbol this pin belongs to.
    /// 1-indexed; 0 means "common to all units" (KiCad convention
    /// for shared power pins).
    pub unit_index: u32,
    /// Position relative to the symbol origin. Used by the net-
    /// topology pass to compute the absolute pin position once
    /// the symbol instance is placed.
    pub at: (f64, f64, f64), // (x, y, rotation_degrees)
}

/// KiCad's pin electrical types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinElectricalType {
    Input,
    Output,
    Bidirectional,
    Tristate,
    Passive,
    Free,
    Unspecified,
    PowerIn,
    PowerOut,
    OpenCollector,
    OpenEmitter,
    NoConnect,
}

impl PinElectricalType {
    /// Parse the KiCad symbol form: `input`, `output`, `passive`,
    /// `power_in`, etc.
    pub fn from_kicad(s: &str) -> Self {
        match s {
            "input"           => Self::Input,
            "output"          => Self::Output,
            "bidirectional"   => Self::Bidirectional,
            "tri_state"       => Self::Tristate,
            "passive"         => Self::Passive,
            "free"            => Self::Free,
            "unspecified"     => Self::Unspecified,
            "power_in"        => Self::PowerIn,
            "power_out"       => Self::PowerOut,
            "open_collector"  => Self::OpenCollector,
            "open_emitter"    => Self::OpenEmitter,
            "no_connect"      => Self::NoConnect,
            _                 => Self::Unspecified,
        }
    }
}

/// An instance of a library symbol placed on the schematic.
#[derive(Debug, Clone)]
pub struct SchematicSymbol {
    /// Reference to the library symbol: `Device:R`, etc.
    pub lib_id: String,
    /// Stable UUID assigned by KiCad.
    pub uuid: String,
    /// Position on the schematic page: (x, y, rotation).
    pub at: (f64, f64, f64),
    /// Mirror state: "x", "y", or None.
    pub mirror: Option<String>,
    /// Which unit this instance is (1-indexed; for multi-gate ICs,
    /// each gate is a separate symbol instance with the same
    /// reference designator but a different unit index).
    pub unit: u32,
    /// Schematic properties: Reference, Value, Footprint, Datasheet,
    /// plus any custom fields. Stored as a map for flexibility;
    /// the well-known ones get pulled out by accessor methods.
    pub properties: HashMap<String, SymbolProperty>,
    /// Pin instances with their per-instance positions (computed
    /// after applying the symbol's `at` transform to each lib pin's
    /// relative position). The net-extraction pass uses these.
    pub pin_positions: Vec<PinPosition>,
    /// `in_bom` flag (KiCad lets users exclude symbols from the BOM
    /// without deleting them).
    pub in_bom: bool,
    /// `on_board` flag (the symbol is on the PCB but possibly
    /// virtual on the schematic).
    pub on_board: bool,
    /// `dnp` flag (KiCad 7+ adds a first-class do-not-populate
    /// marker; KiCad 6 uses custom fields).
    pub dnp: bool,
}

impl SchematicSymbol {
    /// Look up the value of a well-known property like "Reference"
    /// or "Value". Returns the value string, or None if absent.
    pub fn property(&self, name: &str) -> Option<&str> {
        self.properties.get(name).map(|p| p.value.as_str())
    }

    pub fn reference(&self) -> Option<&str> { self.property("Reference") }
    pub fn value(&self)     -> Option<&str> { self.property("Value") }
    pub fn footprint(&self) -> Option<&str> { self.property("Footprint") }
    pub fn datasheet(&self) -> Option<&str> { self.property("Datasheet") }
}

/// One property field on a schematic symbol. We keep the full
/// shape because some fields (visibility, position, font) matter
/// when emitting back to KiCad someday.
#[derive(Debug, Clone)]
pub struct SymbolProperty {
    pub value: String,
    pub at: Option<(f64, f64, f64)>,
    pub hidden: bool,
}

/// A pin's resolved absolute position on the schematic.
#[derive(Debug, Clone)]
pub struct PinPosition {
    pub pin_number: String,
    pub pin_name: String,
    pub electrical_type: PinElectricalType,
    pub at: (f64, f64),     // (x, y) in schematic coordinates
}

/// A wire segment. Two endpoints; the net-extraction pass joins
/// segments that share endpoints into nets.
#[derive(Debug, Clone)]
pub struct Wire {
    pub start: (f64, f64),
    pub end: (f64, f64),
    pub uuid: String,
}

/// An explicit junction (wires connect here even though they cross).
#[derive(Debug, Clone)]
pub struct Junction {
    pub at: (f64, f64),
    pub uuid: String,
}

/// A `no_connect` marker (pin intentionally unconnected).
#[derive(Debug, Clone)]
pub struct NoConnect {
    pub at: (f64, f64),
    pub uuid: String,
}

/// A local label (`(label "BUS_CLK" (at ...) ...)`).
#[derive(Debug, Clone)]
pub struct Label {
    pub text: String,
    pub at: (f64, f64, f64),
    pub uuid: String,
}

/// A global label (`(global_label "RESET" ...)`). Spans all sheets.
#[derive(Debug, Clone)]
pub struct GlobalLabel {
    pub text: String,
    pub at: (f64, f64, f64),
    pub shape: GlobalLabelShape,
    pub uuid: String,
}

/// A hierarchical label (`(hierarchical_label "SHEET_IN" ...)`).
/// Connects to the matching sheet pin on the parent.
#[derive(Debug, Clone)]
pub struct HierarchicalLabel {
    pub text: String,
    pub at: (f64, f64, f64),
    pub shape: HierarchicalLabelShape,
    pub uuid: String,
}

/// Shape annotation on global labels: input, output, bidi, tri-state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalLabelShape {
    Input, Output, Bidirectional, Tristate, Passive,
}

/// Shape annotation on hierarchical labels. Same set as global.
pub type HierarchicalLabelShape = GlobalLabelShape;

/// A power-flag symbol instance. These are schematic symbols from
/// the `power` library (`+5V`, `GND`, `+3V3`, `VCC`, `VBUS`, etc.).
/// We model them separately from regular `SchematicSymbol` because
/// they map to BHDL `power`/`ground` declarations, not to entity
/// instances.
#[derive(Debug, Clone)]
pub struct PowerSymbol {
    /// The label as drawn on the schematic: `+5V`, `GND`, `+3V3`, …
    pub label: String,
    /// Position on the schematic.
    pub at: (f64, f64, f64),
    /// Power category (auto-detected from label).
    pub category: PowerCategory,
    /// Inferred voltage value when the label encodes one (`+5V` → 5V,
    /// `+3V3` → 3.3V). None for grounds and ambiguous names.
    pub voltage: Option<f64>,
    pub uuid: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerCategory {
    Power,  // +5V, +3V3, VCC, VBUS, VBAT, ...
    Ground, // GND, GNDA, GNDD, AGND, DGND, ...
    Other,  // unrecognised label
}

/// A reference to a child sheet — the `(sheet ...)` construct in
/// the parent schematic.
#[derive(Debug, Clone)]
pub struct SheetRef {
    /// On-disk path of the child sheet's `.kicad_sch` file
    /// (relative to the parent).
    pub file_path: PathBuf,
    /// The display name of the sheet ("Power Supply", "MCU").
    pub name: String,
    /// Position of the sheet symbol on the parent schematic.
    pub at: (f64, f64),
    /// Width × height of the sheet symbol.
    pub size: (f64, f64),
    /// Pins on the sheet symbol (parent-side connections to
    /// the child's hierarchical labels).
    pub pins: Vec<SheetPin>,
    /// Stable UUID.
    pub uuid: String,
}

/// A pin on a sheet symbol (parent side).
#[derive(Debug, Clone)]
pub struct SheetPin {
    /// Name matching a hierarchical label inside the child.
    pub name: String,
    /// Direction (input, output, bidi) — must match the child's
    /// hierarchical label shape.
    pub shape: HierarchicalLabelShape,
    /// Position on the parent schematic.
    pub at: (f64, f64, f64),
    pub uuid: String,
}
