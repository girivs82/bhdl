//! Typed P&R layout-intent vocabulary v0.
//!
//! Each `LayoutIntent` variant carries datasheet-rooted design intent
//! that lowers (in `bhdl-pnr`) to geometric/electrical layout
//! constraints — proximity, loop area, layer hints, keep-aways, net
//! tags. The variants mirror `bhdl-pnr/docs/intent_vocabulary_v0.md`
//! §4 exactly; that doc is the contract.
//!
//! Source-syntax form: `for INTENT(named_param: value, ...)` attached
//! to a component instantiation inside an `expansion { }` block (or, at
//! board level, to a `@net`). The parser produces these from the
//! `INTENT_CALL` syntax node; the analyzer attaches the resulting typed
//! value to the materialized netlist instance.
//!
//! Distinct from the simulation-lifecycle `IntentCall`/`IntentResult`
//! in the parent module: different intent kinds, different consumer
//! (P&R placement/routing vs. the simulation engine).
//!
//! ## Scope split (per the contract docs)
//!
//! This vocabulary covers the **support-passive placement** half:
//! where a chip's expansion-born support components sit (decoupling,
//! crystal load caps, termination, snubbers, …). It deliberately does
//! NOT cover signal-net routing properties (impedance, length match,
//! diff pairs, skew, swizzle) — those come from the shipped v0.8
//! interface `constraints { }` mechanism (`docs/spec/Interfaces.md`
//! §13) and reach P&R as `intf_const__*` / `intf_const_rel__*` module
//! attributes. Earlier vocabulary drafts carried `diff_pair` /
//! `length_match_group` intents; they were dropped to avoid two
//! mechanisms expressing the same thing (vocab doc §4.6).
//!
//! ## Versioning
//!
//! v0 = the variants below. Adding a variant is a minor bump; both the
//! synth side (parser/analyzer) and the P&R side (recipe) extend. P&R
//! warns-and-degrades on unknown variants (never fails the build), so
//! the two sides can evolve without lockstep. Changing a variant's
//! field shape is breaking (deprecation cycle).

use serde::{Deserialize, Serialize};

/// Reference to a pin, used in intent parameters.
///
/// Inside an `expansion { }` block, intents reference the host
/// entity's own pins by name (`HostPin`). At board level, intents may
/// reference a pin on a specific board component (`BoardPin`).
/// Resolution to a flat board net happens in the P&R recipe engine at
/// lowering time (string-now, resolve-later) — the parser does not
/// validate the name against the host pin map, so a typo surfaces as a
/// clear lowering error rather than a parse error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PinRef {
    /// Reference by pin name on the host entity (e.g. `"VCC"`,
    /// `"GND1"`). Resolved against the host's own pin map.
    HostPin(String),
    /// Reference to a pin on a board-level component
    /// (e.g. `mcu.VCC` → `{ component: "mcu", pin: "VCC" }`).
    BoardPin { component: String, pin: String },
}

/// Reference to a sibling component within the same expansion
/// (e.g. the partner load cap of a crystal pair). Resolved at lowering
/// time, like `PinRef`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentRef(pub String);

/// Reference to a board-level net (board-scope intents only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetRef(pub String);

/// Layer-placement preference (soft hint).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayerHint {
    Any,
    Top,
    Bottom,
    Inner,
    AdjacentToGroundPlane,
}

/// Sense-resistor topology (for `current_sense`). Distinct from the
/// routing `Topology` (Star/DaisyChain/FlyBy/T) used by the constraint
/// model — this names the *measurement* wiring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SenseTopology {
    /// 4-wire Kelvin sense — force/sense separated at the shunt pads.
    Kelvin,
    /// 2-wire standard sense.
    Standard,
}

/// The typed P&R intent vocabulary (v0).
///
/// Variants and their parameters mirror `intent_vocabulary_v0.md` §4.
/// Optional placement-distance parameters carry the doc's default in
/// their doc-comment; the parser fills the default when the source
/// omits the named argument (the default policy lives stdlib-side via
/// these signatures, not in the P&R recipe).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LayoutIntent {
    // ── §4.1 Decoupling / power-integrity ────────────────────────
    /// Small ceramic cap placed adjacent to one supply pin with the
    /// lowest-inductance return through a specific ground pin.
    HighFreqBypass {
        rail: PinRef,
        return_pin: PinRef,
        loop_area_max_mm2: f32,
        /// default 2.0 mm
        proximity_max_mm: f32,
    },
    /// Larger reservoir cap; placement less critical than a bypass.
    BulkReservoir {
        rail: PinRef,
        return_pin: PinRef,
        /// default 10.0 mm
        proximity_max_mm: f32,
    },
    /// Quiet placement near an analog reference pin, returning through
    /// analog ground; kept away from switching/digital nets.
    AnalogRefFilter {
        ref_pin: PinRef,
        return_pin: PinRef,
        /// default 3.0 mm
        proximity_max_mm: f32,
    },

    // ── §4.2 Clocking ─────────────────────────────────────────────
    /// One of a symmetric pair of crystal load caps; length-matched to
    /// its partner, short trace to the crystal pin.
    CrystalLoadCap {
        xtal_pin: PinRef,
        return_pin: PinRef,
        partner: ComponentRef,
        /// default 3.0 mm
        proximity_max_mm: f32,
    },

    // ── §4.3 Switching power ──────────────────────────────────────
    /// Cin of a switching regulator — drives the EMC-critical hot loop.
    SwitchingInputFilter {
        rail: PinRef,
        return_pin: PinRef,
        loop_area_max_mm2: f32,
        /// default 2.0 mm
        switch_node_keepaway_mm: f32,
    },
    /// Feedback-divider resistors; sensitive tap→FB trace, route short
    /// and away from the switch node.
    FeedbackDivider {
        sense_node: PinRef,
        fb_pin: PinRef,
        keepaway_from: PinRef,
        /// default 3.0 mm
        keepaway_min_mm: f32,
    },
    /// RC/RD snubber across a switching node; minimize the loop between
    /// its two nodes.
    Snubber { across: (PinRef, PinRef) },

    // ── §4.4 Signal conditioning ──────────────────────────────────
    /// Source-terminated resistor on a fast signal; sits immediately
    /// adjacent to the driver.
    SeriesTermination { driver: PinRef, line: NetRef },
    /// Series gate resistor on a MOSFET; placed near the FET (the long
    /// trace tolerates the high-impedance side).
    GateResistor { driver: PinRef, gate: PinRef },
    /// Discrete pull-up. Geometrically unconstrained; the value is net
    /// classification (the net is tagged pulled).
    Pullup { signal: PinRef, rail: PinRef },
    /// Discrete pull-down. As `Pullup`, toward a return.
    Pulldown { signal: PinRef, return_pin: PinRef },

    // ── §4.5 Measurement ──────────────────────────────────────────
    /// Shunt resistor in a current path. Kelvin requires 4-wire sense.
    CurrentSense {
        across: (PinRef, PinRef),
        topology: SenseTopology,
    },
}

impl LayoutIntent {
    /// The intent kind's source-syntax name (the `for <name>(...)`
    /// keyword). Stable identifier used for provenance and for the
    /// parser ↔ vocabulary dispatch.
    pub fn kind_name(&self) -> &'static str {
        match self {
            LayoutIntent::HighFreqBypass { .. } => "high_freq_bypass",
            LayoutIntent::BulkReservoir { .. } => "bulk_reservoir",
            LayoutIntent::AnalogRefFilter { .. } => "analog_ref_filter",
            LayoutIntent::CrystalLoadCap { .. } => "crystal_load_cap",
            LayoutIntent::SwitchingInputFilter { .. } => "switching_input_filter",
            LayoutIntent::FeedbackDivider { .. } => "feedback_divider",
            LayoutIntent::Snubber { .. } => "snubber",
            LayoutIntent::SeriesTermination { .. } => "series_termination",
            LayoutIntent::GateResistor { .. } => "gate_resistor",
            LayoutIntent::Pullup { .. } => "pullup",
            LayoutIntent::Pulldown { .. } => "pulldown",
            LayoutIntent::CurrentSense { .. } => "current_sense",
        }
    }
}

/// Default placement distances (mm) from the vocabulary doc §4. The
/// parser applies these when the named argument is absent at the
/// source site, keeping the default policy stdlib-side.
pub mod defaults {
    pub const HIGH_FREQ_BYPASS_PROXIMITY_MM: f32 = 2.0;
    pub const BULK_RESERVOIR_PROXIMITY_MM: f32 = 10.0;
    pub const ANALOG_REF_FILTER_PROXIMITY_MM: f32 = 3.0;
    pub const CRYSTAL_LOAD_CAP_PROXIMITY_MM: f32 = 3.0;
    pub const SWITCHING_INPUT_FILTER_KEEPAWAY_MM: f32 = 2.0;
    pub const FEEDBACK_DIVIDER_KEEPAWAY_MM: f32 = 3.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_names_match_vocab_doc() {
        let i = LayoutIntent::HighFreqBypass {
            rail: PinRef::HostPin("VCC".into()),
            return_pin: PinRef::HostPin("GND1".into()),
            loop_area_max_mm2: 1.5,
            proximity_max_mm: defaults::HIGH_FREQ_BYPASS_PROXIMITY_MM,
        };
        assert_eq!(i.kind_name(), "high_freq_bypass");
    }

    #[test]
    fn round_trips_through_json() {
        let i = LayoutIntent::CurrentSense {
            across: (
                PinRef::BoardPin { component: "shunt".into(), pin: "1".into() },
                PinRef::HostPin("ISNS".into()),
            ),
            topology: SenseTopology::Kelvin,
        };
        let json = serde_json::to_string(&i).unwrap();
        let back: LayoutIntent = serde_json::from_str(&json).unwrap();
        assert_eq!(i, back);
    }
}
