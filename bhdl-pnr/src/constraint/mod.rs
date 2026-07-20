//! Typed layout-constraint catalog — the central IR between intent
//! producers (expansion intent + interface constraints) and the
//! placement/routing consumers.
//!
//! See `bhdl-pnr/docs/constraint_model_v0.md` for the full design. This
//! module implements the v0 subset needed for the ATmega-decoupling
//! milestone (geometric/placement constraints), with the electrical/
//! routing variants stubbed in the enum so the catalog is complete and
//! the router side can fill `eval` later.
//!
//! Design points honored here:
//!   - Typed catalog (no stringly constraints).
//!   - Hard vs. soft is a per-instance choice (`Hardness`).
//!   - Constraints reference *resolved* P&R entities (`ComponentId` /
//!     `NetId` / `PinSel`), so `eval` can run directly against a laid-out
//!     `Board`. Name→id resolution happens in the lowering pass.
//!   - Every constraint carries `ConstraintSource` provenance.
//!   - Loop area uses the shoelace centroid approximation (§5 of the
//!     doc) so it can participate in placement cost before any wires.

pub mod conflicts;
pub mod eval;

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

use crate::types::{ComponentId, NetId, PinId};

/// A resolved pin selector: a specific pin on a specific component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinSel {
    pub component: ComponentId,
    pub pin: PinId,
}

/// A resolved reference to either a whole component or a specific pin.
/// Lets `Proximity` mean "place these two components near each other" or
/// "place this component near that specific pin."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntitySel {
    Component(ComponentId),
    Pin(PinSel),
}

/// Layer-placement preference (mirrors `bhdl_common::intent::vocabulary::LayerHint`,
/// kept separate so the constraint catalog is self-contained).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayerHintKind {
    Any,
    Top,
    Bottom,
    Inner,
    AdjacentToGroundPlane,
}

/// Cost shape for a soft constraint: how the per-unit-overshoot penalty
/// grows with the constraint-specific error.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CostShape {
    /// cost = w · max(0, error)
    Linear,
    /// cost = w · max(0, error)²  — default for geometric softness
    /// (smooth gradient for the analytical placer).
    Quadratic,
    /// 0 inside `slack`, linear ramp outside.
    Hinge { slack: f32 },
    /// cost = w · (exp(k · error) − 1)
    Exponential { k: f32 },
}

impl CostShape {
    /// Evaluate the (unweighted) cost for a given non-negative error.
    pub fn cost(&self, error: f64) -> f64 {
        let e = error.max(0.0);
        match self {
            CostShape::Linear => e,
            CostShape::Quadratic => e * e,
            CostShape::Hinge { slack } => (e - *slack as f64).max(0.0),
            CostShape::Exponential { k } => ((*k as f64 * e).exp() - 1.0).max(0.0),
        }
    }
}

/// Hardness of a constraint instance.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Hardness {
    /// Must be satisfied. Accumulated into a Lagrangian penalty whose
    /// multiplier ramps over the placement schedule.
    Hard,
    /// Balanced against other costs via a shaped, weighted penalty.
    Soft { shape: CostShape, weight: f64 },
}

impl Hardness {
    pub fn is_hard(&self) -> bool {
        matches!(self, Hardness::Hard)
    }
}

/// Where a constraint came from — for diagnostics, debugging the recipe
/// engine, and conflict resolution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstraintSource {
    /// bhdl source file the intent/interface-constraint lives in.
    pub file: String,
    /// 1-based line, if known.
    pub line: Option<u32>,
    /// The intent kind name (`high_freq_bypass`) or `interface:<prop>`.
    pub intent_kind: String,
    /// Vocabulary / recipe version that emitted this constraint.
    pub recipe_version: String,
}

impl ConstraintSource {
    /// Convenience constructor for an intent-derived constraint.
    pub fn intent(kind: impl Into<String>) -> Self {
        ConstraintSource {
            file: String::new(),
            line: None,
            intent_kind: kind.into(),
            recipe_version: "0".into(),
        }
    }
}

/// Result of evaluating one constraint against a layout snapshot.
#[derive(Debug, Clone, PartialEq)]
pub enum Eval {
    /// Satisfied; no cost.
    Satisfied,
    /// Soft constraint contributing a (weighted) cost.
    SoftCost(f64),
    /// Violated. `cost` is the (weighted) penalty; `slack` is the signed
    /// overshoot (positive = how far past the bound).
    Violated { cost: f64, slack: f32 },
    /// Not enough information in this snapshot to evaluate (e.g. a
    /// routing constraint evaluated on a placement-only snapshot).
    Unknown,
}

/// The typed constraint catalog (v0).
///
/// Geometric/placement variants (§3.1) are implemented for `eval`.
/// Electrical/routing variants (§3.2) and net tags (§3.3) are present so
/// the catalog is complete; their `eval` returns `Unknown` until the
/// router side fills them in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Constraint {
    // ── §3.1 Geometric / placement ───────────────────────────────────
    /// Keep `a` within `max_mm` of `b`.
    Proximity {
        a: EntitySel,
        b: EntitySel,
        max_mm: f32,
        hardness: Hardness,
        source: ConstraintSource,
    },
    /// Keep `a` at least `min_mm` from `b`.
    KeepAway {
        a: EntitySel,
        b: EntitySel,
        min_mm: f32,
        hardness: Hardness,
        source: ConstraintSource,
    },
    /// Soft layer-placement preference for a component.
    LayerHint {
        component: ComponentId,
        hint: LayerHintKind,
        hardness: Hardness,
        source: ConstraintSource,
    },

    // ── §3.2 Electrical / routing ─────────────────────────────────────
    /// Bound the enclosed area of a current loop through the listed pins.
    /// Pre-routing: shoelace centroid approximation (§5). Post-routing:
    /// recomputed from trace geometry (router side, later).
    LoopArea {
        loop_pins: Vec<PinSel>,
        max_mm2: f32,
        hardness: Hardness,
        source: ConstraintSource,
    },
    /// Bound the routed trace length between two pins. (`eval` deferred to
    /// the router; returns `Unknown` on placement-only snapshots.)
    TraceLength {
        from: PinSel,
        to: PinSel,
        max_mm: f32,
        hardness: Hardness,
        source: ConstraintSource,
    },
    /// Differential pair. (Router-side.)
    /// P4 stage 4: declared crosstalk noise budget — the sign-off
    /// gates the MEASURED coupled noise (k_b x measured swing).
    NoiseBudget {
        net: NetId,
        max_mv: f32,
        source: ConstraintSource,
    },
    /// P4 stage 4: declared IR-drop budget — the sign-off gates
    /// R x solved current on the routed rail.
    RailDrop {
        net: NetId,
        max_mv: f32,
        source: ConstraintSource,
    },
    DiffPair {
        p_net: NetId,
        n_net: NetId,
        spacing_mm: f32,
        length_match_mm: f32,
        /// Set when the budget was DECLARED in time — the sign-off then
        /// grades routed DELAY (per-layer velocity from the stackup)
        /// instead of length.
        length_match_ps: Option<f32>,
        source: ConstraintSource,
    },
    /// Length-match group. (Router-side.)
    LengthMatchGroup {
        nets: Vec<NetId>,
        tolerance_mm: f32,
        /// Declared-in-time budget — see DiffPair::length_match_ps.
        tolerance_ps: Option<f32>,
        hardness: Hardness,
        source: ConstraintSource,
    },
    /// Target impedance on a net. v0: lowers to a trace-width floor;
    /// real impedance routing needs the v1 stackup model. (Router-side.)
    Impedance {
        net: NetId,
        target_ohms: f32,
        tolerance_pct: f32,
        source: ConstraintSource,
    },
    /// Routing topology for a net (star/daisy-chain/fly-by/T). From a
    /// protocol's `topology` interface constraint, or `current_sense`
    /// Kelvin. (Router-side — tree-construction hint.)
    Topology {
        net: NetId,
        kind: TopoKind,
        root: Option<PinSel>,
        stub_max_mm: Option<f32>,
        source: ConstraintSource,
    },
    /// A grant of routing *freedom*: the router may permute the
    /// endpoint→net assignment among `members` (DDR swizzle). Not a
    /// restriction — see `constraint_model_v0.md` §3.2a / §5a.2. Inert if
    /// the board fixed the permutation. (Router-side — assignment input.)
    SwizzleGroup {
        members: Vec<NetId>,
        scope: SwizzleScope,
        source: ConstraintSource,
    },
    /// Net classification tag (signal class + optional max frequency).
    /// Not a cost — metadata other rules/the router dispatch on
    /// (`constraint_model_v0.md` §3.3). From a protocol's `signal_class` /
    /// `max_freq` interface constraints.
    SignalClass {
        net: NetId,
        class: String,
        max_freq_hz: Option<f64>,
        source: ConstraintSource,
    },
    /// Net restricted to specific copper layers (`layer top;` etc.).
    /// (Router-side — enforced in the grid walk.)
    LayerRule {
        net: NetId,
        bind: LayerBind,
        source: ConstraintSource,
    },
}

/// Which copper layers a `LayerRule` binds a net to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayerBind {
    Top,
    Bottom,
    Outer,
    Inner,
}

/// Routing-topology kind for `Constraint::Topology`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TopoKind {
    Star,
    DaisyChain,
    FlyBy,
    T,
}

/// Scope of a `SwizzleGroup`'s permutation freedom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SwizzleScope {
    /// Permute leaf signals within one bundle (e.g. DQ0..DQ3+DM in a byte lane).
    WithinGroup,
    /// Permute whole bundles as units (e.g. reorder byte lanes).
    AcrossGroups,
}

impl Constraint {
    pub fn source(&self) -> &ConstraintSource {
        match self {
            Constraint::Proximity { source, .. }
            | Constraint::KeepAway { source, .. }
            | Constraint::LayerHint { source, .. }
            | Constraint::LoopArea { source, .. }
            | Constraint::TraceLength { source, .. }
            | Constraint::DiffPair { source, .. }
            | Constraint::NoiseBudget { source, .. }
            | Constraint::RailDrop { source, .. }
            | Constraint::LengthMatchGroup { source, .. }
            | Constraint::Impedance { source, .. }
            | Constraint::Topology { source, .. }
            | Constraint::SwizzleGroup { source, .. }
            | Constraint::SignalClass { source, .. }
            | Constraint::LayerRule { source, .. } => source,
        }
    }

    pub fn hardness(&self) -> Hardness {
        match self {
            Constraint::Proximity { hardness, .. }
            | Constraint::KeepAway { hardness, .. }
            | Constraint::LayerHint { hardness, .. }
            | Constraint::LoopArea { hardness, .. }
            | Constraint::TraceLength { hardness, .. }
            | Constraint::LengthMatchGroup { hardness, .. } => *hardness,
            // Router-side variants without a placement hardness default to Hard.
            Constraint::DiffPair { .. }
            | Constraint::Impedance { .. }
            | Constraint::Topology { .. }
            | Constraint::NoiseBudget { .. }
            | Constraint::RailDrop { .. }
            | Constraint::LayerRule { .. } => Hardness::Hard,
            // Freedom grant + classification tag carry no hardness.
            Constraint::SwizzleGroup { .. } | Constraint::SignalClass { .. } => {
                Hardness::Soft { shape: CostShape::Linear, weight: 0.0 }
            }
        }
    }

    /// A short, stable label for diagnostics.
    pub fn kind_label(&self) -> &'static str {
        match self {
            Constraint::Proximity { .. } => "Proximity",
            Constraint::KeepAway { .. } => "KeepAway",
            Constraint::LayerHint { .. } => "LayerHint",
            Constraint::LoopArea { .. } => "LoopArea",
            Constraint::TraceLength { .. } => "TraceLength",
            Constraint::DiffPair { .. } => "DiffPair",
            Constraint::LengthMatchGroup { .. } => "LengthMatchGroup",
            Constraint::Impedance { .. } => "Impedance",
            Constraint::Topology { .. } => "Topology",
            Constraint::SwizzleGroup { .. } => "SwizzleGroup",
            Constraint::SignalClass { .. } => "SignalClass",
            Constraint::LayerRule { .. } => "LayerRule",
            Constraint::NoiseBudget { .. } => "NoiseBudget",
            Constraint::RailDrop { .. } => "RailDrop",
        }
    }
}
