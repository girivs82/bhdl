//! Constraint evaluation against a layout snapshot.
//!
//! A `LayoutSnapshot` exposes just enough geometry for constraints to
//! score themselves: component poses and absolute pin positions, plus a
//! flag for whether routes exist yet. `Board` implements it directly, so
//! placement-time evaluation runs against the live board with no copying.
//!
//! Routing-dependent constraints return `Eval::Unknown` on a
//! placement-only snapshot (`has_routes() == false`) and are simply
//! skipped by the placer cost loop — no penalty, no crash.

use crate::constraint::{Constraint, CostShape, Eval, Hardness, PinSel};
use crate::types::{Board, ComponentId};

use super::EntitySel;

/// Geometry accessor for constraint evaluation.
pub trait LayoutSnapshot {
    /// Component pose `(x, y, theta)` in mm/radians, if the component exists.
    fn component_pose(&self, c: ComponentId) -> Option<(f64, f64, f64)>;
    /// Absolute position of a pin (component pose applied to the pin offset).
    fn pin_abs(&self, sel: PinSel) -> Option<(f64, f64)>;
    /// Whether routed traces are available in this snapshot.
    fn has_routes(&self) -> bool {
        false
    }

    /// Absolute position of an `EntitySel` — a component's center, or a pin.
    fn entity_pos(&self, sel: EntitySel) -> Option<(f64, f64)> {
        match sel {
            EntitySel::Component(c) => self.component_pose(c).map(|(x, y, _)| (x, y)),
            EntitySel::Pin(p) => self.pin_abs(p),
        }
    }
}

impl LayoutSnapshot for Board {
    fn component_pose(&self, c: ComponentId) -> Option<(f64, f64, f64)> {
        self.components
            .iter()
            .find(|comp| comp.id == c)
            .map(|comp| (comp.x, comp.y, comp.theta))
    }

    fn pin_abs(&self, sel: PinSel) -> Option<(f64, f64)> {
        let comp = self.components.iter().find(|c| c.id == sel.component)?;
        let pin = comp.pins.iter().find(|p| p.pin_id == sel.pin)?;
        // Rotate the pin offset by the component's theta, then translate.
        let (cos_t, sin_t) = (comp.theta.cos(), comp.theta.sin());
        let rx = pin.dx * cos_t - pin.dy * sin_t;
        let ry = pin.dx * sin_t + pin.dy * cos_t;
        Some((comp.x + rx, comp.y + ry))
    }
}

/// Euclidean distance between two points.
fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    (dx * dx + dy * dy).sqrt()
}

/// Shoelace polygon area on a sequence of points (absolute value).
///
/// This is the pre-routing loop-area approximation (constraint_model_v0
/// §5): cheap, O(N), differentiable in pin positions, and monotonically
/// related to the achievable routed loop area. For a 4-pin bypass loop
/// it collapses toward 0 as the cap is placed between rail and return —
/// the correct optimum.
pub fn shoelace_area(points: &[(f64, f64)]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }
    let mut acc = 0.0;
    for i in 0..points.len() {
        let (x0, y0) = points[i];
        let (x1, y1) = points[(i + 1) % points.len()];
        acc += x0 * y1 - x1 * y0;
    }
    0.5 * acc.abs()
}

/// Satisfaction tolerance in mm. A constraint violated by less than this
/// is treated as satisfied — sub-micron overshoot is physically
/// meaningless on a PCB, and equilibria that sit exactly at a bound
/// (proximity force → 0 at `max_mm`) would otherwise flap between
/// satisfied/violated on floating-point noise.
pub const SATISFACTION_EPS_MM: f64 = 1e-3;

/// Apply a hardness to a non-negative `error`, producing an `Eval`.
fn score(hardness: &Hardness, error: f64) -> Eval {
    if error <= SATISFACTION_EPS_MM {
        return Eval::Satisfied;
    }
    match hardness {
        Hardness::Hard => Eval::Violated {
            // Hard constraints report a quadratic raw cost; the placer
            // multiplies it by the ramping Lagrangian λ.
            cost: error * error,
            slack: error as f32,
        },
        Hardness::Soft { shape, weight } => {
            let c = weight * shape.cost(error);
            Eval::SoftCost(c)
        }
    }
}

impl Constraint {
    /// Evaluate this constraint against a snapshot.
    pub fn eval(&self, snap: &dyn LayoutSnapshot) -> Eval {
        match self {
            // Router/report-side gates — no placement-time evaluation.
            Constraint::NoiseBudget { .. } | Constraint::RailDrop { .. } => Eval::Unknown,
            Constraint::Proximity { a, b, max_mm, hardness, .. } => {
                let (pa, pb) = match (snap.entity_pos(*a), snap.entity_pos(*b)) {
                    (Some(pa), Some(pb)) => (pa, pb),
                    _ => return Eval::Unknown,
                };
                let error = dist(pa, pb) - *max_mm as f64;
                score(hardness, error)
            }

            Constraint::KeepAway { a, b, min_mm, hardness, .. } => {
                let (pa, pb) = match (snap.entity_pos(*a), snap.entity_pos(*b)) {
                    (Some(pa), Some(pb)) => (pa, pb),
                    _ => return Eval::Unknown,
                };
                // Error is how far *inside* the keep-away radius we are.
                let error = *min_mm as f64 - dist(pa, pb);
                score(hardness, error)
            }

            Constraint::LoopArea { loop_pins, max_mm2, hardness, .. } => {
                let mut pts = Vec::with_capacity(loop_pins.len());
                for p in loop_pins {
                    match snap.pin_abs(*p) {
                        Some(xy) => pts.push(xy),
                        None => return Eval::Unknown,
                    }
                }
                let area = shoelace_area(&pts);
                let error = area - *max_mm2 as f64;
                score(hardness, error)
            }

            Constraint::LayerHint { .. } => {
                // Layer assignment isn't modeled in the 2D placement
                // snapshot; consumed by the router/layer-assignment side.
                Eval::Unknown
            }

            // Routing-dependent variants: not evaluable on placement-only
            // snapshots. Returning Unknown means "skip, no penalty."
            // SwizzleGroup is a freedom grant and SignalClass a tag — both
            // are consumed by the router, never scored here.
            Constraint::TraceLength { .. }
            | Constraint::DiffPair { .. }
            | Constraint::LengthMatchGroup { .. }
            | Constraint::Impedance { .. }
            | Constraint::Topology { .. }
            | Constraint::SwizzleGroup { .. }
            | Constraint::SignalClass { .. }
            | Constraint::LayerRule { .. } => {
                let _ = snap.has_routes();
                Eval::Unknown
            }
        }
    }
}

/// Evaluate every constraint and fold into a total soft cost plus a list
/// of hard violations (with their sources and slack). The placer adds the
/// hard-violation cost under a ramping Lagrangian λ.
pub struct EvalSummary {
    pub soft_cost: f64,
    pub hard_cost: f64,
    pub hard_violations: Vec<(super::ConstraintSource, f32)>,
    pub unknown: usize,
}

pub fn eval_all(board: &Board) -> EvalSummary {
    let mut soft_cost = 0.0;
    let mut hard_cost = 0.0;
    let mut hard_violations = Vec::new();
    let mut unknown = 0;

    for c in &board.constraints {
        match c.eval(board) {
            Eval::Satisfied => {}
            Eval::SoftCost(x) => soft_cost += x,
            Eval::Violated { cost, slack } => {
                if c.hardness().is_hard() {
                    hard_cost += cost;
                    hard_violations.push((c.source().clone(), slack));
                } else {
                    soft_cost += cost;
                }
            }
            Eval::Unknown => unknown += 1,
        }
    }

    EvalSummary { soft_cost, hard_cost, hard_violations, unknown }
}

/// Sanity helper used by the placer: a quadratic cost shape is the default
/// for new soft geometric constraints.
pub const DEFAULT_GEOMETRIC_SHAPE: CostShape = CostShape::Quadratic;
