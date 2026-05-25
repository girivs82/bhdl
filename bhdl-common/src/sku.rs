//! First-class SKU (Stock Keeping Unit) attribute convention.
//!
//! A `.bhdl` entity is now the single source of truth for *both* the
//! circuit topology AND the manufacturing inputs needed to actually
//! order the part. Before this module, attributes like
//! `manufacturer` / `part_number` / `footprint` were used ad-hoc by
//! some consumers (PnR, KiCad export) and ignored by others (BOM
//! generator, supplier picker), with no canonical naming convention.
//!
//! This module defines the canonical names and what they mean. Both
//! producers (stdlib entities, vendor entities, board overrides) and
//! consumers (BOM walker, supplier-layer pre-check, KiCad export) read
//! and write through these constants, so a misspelled attribute fails
//! loudly at compile time instead of silently disappearing.
//!
//! ## Two kinds of entity, two granularities of SKU
//!
//! BHDL has two shapes of device-shaped entity:
//!
//! - **Concrete part entities** — e.g. `NPN_2N3904`, `LM358_DIP8`.
//!   These name *one specific orderable part*. They should declare a
//!   full SKU: manufacturer + MPN + package + ideally distributor PNs.
//!
//! - **Abstract type entities** — e.g. `Res`, `Cap`, `Ind`, `Triode`.
//!   These name a *category*. The user supplies a numeric value at
//!   instantiation (`Res(575)`) and the supplier picker chooses a
//!   concrete part to satisfy it. These entities declare the *shape*
//!   attributes (`category = "resistor"`, optional `default_package =
//!   "0603"`, etc.) but not an MPN. A board-level instance can
//!   override with an explicit `attribute mpn = ...` to pin a
//!   specific part instead of letting the picker choose.
//!
//! Vendor design recipes (Stages 1-6) sit in between: they produce a
//! *value* the picker then resolves. So `Itail_Rk` gets `value =
//! "575.313"`, the picker reads `category = "resistor"` from the
//! `Res` entity and chooses (for example) a 576 Ω 1 % 0603 thick-film
//! reel; the BOM lists THAT part, not "575 Ω generic".
//!
//! ## The canonical attribute names
//!
//! See the `attr` module below. The convention is: lowercase,
//! snake_case, no namespacing (the attribute itself is the key).
//! Distributor SKUs follow the `<distributor>_pn` pattern.

/// Canonical SKU attribute names. Producers and consumers both reach
/// for these constants rather than typing string literals, so a
/// misspelling fails to compile.
pub mod attr {
    // ─── Identity ─────────────────────────────────────────────────

    /// Manufacturer name as the manufacturer themselves spell it
    /// (e.g. "ON Semiconductor", "Yageo", "Murata"). Required on
    /// concrete part entities.
    pub const MANUFACTURER: &str = "manufacturer";

    /// Manufacturer part number — the canonical orderable identifier.
    /// On concrete part entities this is the MFR's full datasheet
    /// part number (e.g. "MMBT3904LT1G", not just "2N3904").
    /// Required on concrete part entities.
    pub const MPN: &str = "mpn";

    /// Generic part-number string used historically by the stdlib —
    /// often a family name like "2N3904" or "6SN7" rather than the
    /// orderable MPN. Kept as an attribute for SPICE / datasheet
    /// lookup but consumers preferring orderable identifiers should
    /// read [`MPN`] first.
    pub const PART_NUMBER: &str = "part_number";

    /// Physical package / case the part comes in (e.g. "TO-92",
    /// "SOT-23", "0603", "QFN-32"). On concrete entities this is the
    /// actual package; on abstract types this is the *default*
    /// package used when the user doesn't override at instantiation.
    ///
    /// The attribute key is `physical_package` rather than the more
    /// obvious `package` because `package` is a reserved BHDL keyword
    /// (used by the layout-block grammar). Vendors who type
    /// `attribute package = ...;` get a parse error at the lexer
    /// stage; the canonical `physical_package` avoids the collision
    /// and is what every SKU consumer reads.
    pub const PACKAGE: &str = "physical_package";

    /// KiCad footprint library reference (e.g.
    /// "Package_TO_SOT_THT:TO-92_Inline"). Consumed by the
    /// KiCad-export path.
    pub const FOOTPRINT: &str = "footprint";

    /// KiCad symbol library reference (e.g. "Device:Q_NPN_BCE").
    /// Consumed by the schematic export.
    pub const KICAD_SYMBOL: &str = "kicad_symbol";

    /// Datasheet URL.
    pub const DATASHEET: &str = "datasheet";

    // ─── Passive characteristics ─────────────────────────────────

    /// Tolerance as a percentage string (e.g. "1%", "5%", "0.1%").
    /// Drives the supplier picker's part-selection band.
    pub const TOLERANCE: &str = "tolerance";

    /// Voltage rating (e.g. "16V", "50V"). Required minimum for
    /// caps; safety floor for the picker.
    pub const VOLTAGE_RATING: &str = "voltage_rating";

    /// Temperature coefficient (e.g. "X7R", "C0G/NP0", "100ppm").
    pub const TEMP_COEFF: &str = "temp_coeff";

    /// Power rating (e.g. "1/4W", "1W") for resistors.
    pub const POWER_RATING: &str = "power_rating";

    // ─── Distributor SKUs ────────────────────────────────────────

    /// DigiKey distributor part number — if known and pre-pinned, the
    /// supplier layer uses this directly instead of querying.
    pub const DIGIKEY_PN: &str = "digikey_pn";

    /// Mouser distributor part number.
    pub const MOUSER_PN: &str = "mouser_pn";

    /// LCSC (JLCPCB-affiliated) distributor part number — useful for
    /// JLC assembly preorders.
    pub const LCSC_PN: &str = "lcsc_pn";

    /// Arrow distributor part number.
    pub const ARROW_PN: &str = "arrow_pn";

    /// Nexar internal identifier (Octopart MPN match).
    pub const NEXAR_PN: &str = "nexar_pn";

    // ─── Category / role ─────────────────────────────────────────

    /// Top-level component class (already used by SPICE +
    /// device-family discovery). Examples: "resistor", "capacitor",
    /// "inductor", "diode", "triode", "bjt", "mosfet", "ic_opamp".
    pub const COMPONENT_CLASS: &str = "component_class";

    /// Reference-designator prefix to use in the BOM (e.g. "R", "C",
    /// "U", "Q"). Inferred from `component_class` when not given;
    /// the attribute lets vendors override (e.g. "FL" for fuses).
    pub const REFDES_PREFIX: &str = "refdes_prefix";

    /// All canonical SKU attribute names, useful for "walk every
    /// attribute and produce a BOM column" code.
    pub const ALL: &[&str] = &[
        MANUFACTURER, MPN, PART_NUMBER, PACKAGE, FOOTPRINT, KICAD_SYMBOL, DATASHEET,
        TOLERANCE, VOLTAGE_RATING, TEMP_COEFF, POWER_RATING,
        DIGIKEY_PN, MOUSER_PN, LCSC_PN, ARROW_PN, NEXAR_PN,
        COMPONENT_CLASS, REFDES_PREFIX,
    ];
}

/// The subset of SKU attributes that, if all populated on an
/// instance, give the BOM walker enough information to produce an
/// order-ready row (manufacturer + MPN + package). Consumers can
/// check `[required_for_order_ready]` for missing data.
pub fn required_for_order_ready() -> &'static [&'static str] {
    &[attr::MANUFACTURER, attr::MPN, attr::PACKAGE]
}

/// Pick a reference-designator prefix for a component_class value
/// using the EDA-standard convention. Vendors who want a custom
/// prefix declare [`attr::REFDES_PREFIX`] on their entity and the
/// BOM walker reads that first.
pub fn refdes_prefix_for_class(component_class: &str) -> &'static str {
    match component_class {
        "resistor"                                => "R",
        "capacitor"                               => "C",
        "inductor"                                => "L",
        "diode" | "led" | "tvs_diode"             => "D",
        "fuse"                                    => "F",
        "transistor" | "bjt" | "mosfet" | "jfet" |
        "triode" | "tetrode" | "pentode"          => "Q",
        "ic_opamp" | "ic_comparator" | "ic_regulator" |
        "ic_logic" | "ic_mcu" | "ic_dsp" |
        "switching_regulator"                     => "U",
        "crystal" | "oscillator"                  => "Y",
        "connector" | "header"                    => "J",
        "test_point"                              => "TP",
        "switch" | "relay"                        => "SW",
        _                                         => "U",
    }
}
