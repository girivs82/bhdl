//! Constraint-derived placement forces.
//!
//! Translates the geometric constraints in `Board.constraints` into
//! per-component gradient contributions (`Forces`), so the analytical
//! placer can satisfy proximity and minimize loop area alongside its
//! existing wirelength/density/cohesion terms.
//!
//! Sign convention matches the rest of the placer (`compute_wirelength`):
//! a `Forces` entry is the **cost gradient**, and `optimizer::adam_step`
//! moves each component `x -= lr · gradient` (descends). So these
//! functions return gradients, NOT descent directions — getting this
//! backwards pushes components away from their targets.
//!
//! Only translation gradients (dx, dy) are produced in v0; rotation
//! (d_theta) is left at zero. Loop area responds well to translation
//! alone (a straddling cap collapses the loop), and rotation coupling can
//! come later if the ATmega result needs it.

use std::collections::HashMap;

use crate::constraint::{Constraint, EntitySel, Hardness, PinSel};
use crate::constraint::eval::LayoutSnapshot;
use crate::types::{Board, ComponentId};

use super::Forces;

/// Index map: ComponentId → position in `board.components` (Forces index).
fn index_map(board: &Board) -> HashMap<ComponentId, usize> {
    board
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect()
}

/// The component that "owns" an entity selector (for force application).
fn owner(sel: EntitySel) -> ComponentId {
    match sel {
        EntitySel::Component(c) => c,
        EntitySel::Pin(p) => p.component,
    }
}

/// Per-constraint multiplier from its hardness. Hard constraints get a
/// large base (the external Lagrangian λ scales the whole proximity term);
/// soft constraints carry their own weight.
fn weight_of(h: &Hardness) -> f64 {
    match h {
        Hardness::Hard => 1.0,
        Hardness::Soft { weight, .. } => *weight,
    }
}

/// Proximity forces: pull the two endpoints of each `Proximity` toward
/// each other when they're farther apart than `max_mm`; push apart for
/// `KeepAway` when closer than `min_mm`. Spring-like; both endpoint
/// components receive equal-and-opposite contributions, so heavy
/// many-net components (anchored by wirelength/density) stay put while
/// light passives move.
pub fn compute_proximity_forces(board: &Board) -> Forces {
    let n = board.components.len();
    let mut f = Forces::zeros(n);
    let idx = index_map(board);

    for c in &board.constraints {
        let (a, b, target, attract, hardness) = match c {
            Constraint::Proximity { a, b, max_mm, hardness, .. } => {
                (*a, *b, *max_mm as f64, true, hardness)
            }
            Constraint::KeepAway { a, b, min_mm, hardness, .. } => {
                (*a, *b, *min_mm as f64, false, hardness)
            }
            _ => continue,
        };

        let (pa, pb) = match (board.entity_pos(a), board.entity_pos(b)) {
            (Some(pa), Some(pb)) => (pa, pb),
            _ => continue,
        };
        let dx = pb.0 - pa.0;
        let dy = pb.1 - pa.1;
        let d = (dx * dx + dy * dy).sqrt().max(1e-6);
        let (ux, uy) = (dx / d, dy / d);

        // Overshoot (positive when the constraint is violated).
        let overshoot = if attract { d - target } else { target - d };
        if overshoot <= 0.0 {
            continue;
        }
        // Quadratic-style magnitude (∝ overshoot) × hardness weight.
        let mag = weight_of(hardness) * overshoot;

        // GRADIENT (not descent dir): for attraction, moving `a` toward b
        // *reduces* cost, so the gradient on `a` points AWAY from b
        // (`-ux`); adam_step then descends (`x -= lr·grad`) → a moves
        // toward b. KeepAway is the opposite sign.
        let dir = if attract { 1.0 } else { -1.0 };
        let (gax, gay) = (-dir * mag * ux, -dir * mag * uy);

        if let Some(&ia) = idx.get(&owner(a)) {
            if board.components[ia].placement.is_free() {
                f.dx[ia] += gax;
                f.dy[ia] += gay;
            }
        }
        if let Some(&ib) = idx.get(&owner(b)) {
            if board.components[ib].placement.is_free() {
                f.dx[ib] -= gax;
                f.dy[ib] -= gay;
            }
        }
    }
    f
}

/// Loop-area forces: gradient-descent on the shoelace centroid area
/// (constraint_model_v0 §5), pushing each loop pin's owning component to
/// shrink the enclosed polygon when its area exceeds `max_mm2`.
pub fn compute_loop_area_forces(board: &Board) -> Forces {
    let n = board.components.len();
    let mut f = Forces::zeros(n);
    let idx = index_map(board);

    for c in &board.constraints {
        let (loop_pins, max_mm2, hardness) = match c {
            Constraint::LoopArea { loop_pins, max_mm2, hardness, .. } => {
                (loop_pins, *max_mm2 as f64, hardness)
            }
            _ => continue,
        };
        if loop_pins.len() < 3 {
            continue;
        }

        // Resolve loop vertex positions.
        let mut pts: Vec<(f64, f64)> = Vec::with_capacity(loop_pins.len());
        let mut ok = true;
        for p in loop_pins {
            match board.pin_abs(*p) {
                Some(xy) => pts.push(xy),
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }

        // Signed shoelace area; if within budget, no force.
        let m = pts.len();
        let mut signed = 0.0;
        for i in 0..m {
            let (x0, y0) = pts[i];
            let (x1, y1) = pts[(i + 1) % m];
            signed += x0 * y1 - x1 * y0;
        }
        signed *= 0.5;
        let area = signed.abs();
        let overshoot = area - max_mm2;
        if overshoot <= 0.0 {
            continue;
        }
        let sgn = if signed >= 0.0 { 1.0 } else { -1.0 };
        // d|A|/d(x_i) = 0.5·sgn·(y_{i+1} − y_{i−1}); d|A|/d(y_i) = 0.5·sgn·(x_{i−1} − x_{i+1}).
        // Emit the GRADIENT (scaled by weight·overshoot); adam_step
        // descends (`x -= lr·grad`), shrinking the area.
        let scale = weight_of(hardness) * overshoot;
        for i in 0..m {
            let prev = pts[(i + m - 1) % m];
            let next = pts[(i + 1) % m];
            let g_x = 0.5 * sgn * (next.1 - prev.1);
            let g_y = 0.5 * sgn * (prev.0 - next.0);
            // Map this vertex to its owning component.
            let comp = loop_pins[i].component;
            if let Some(&ci) = idx.get(&comp) {
                if board.components[ci].placement.is_free() {
                    f.dx[ci] += scale * g_x;
                    f.dy[ci] += scale * g_y;
                }
            }
        }
    }
    f
}

/// Helper for `PinSel`-typed entity positions in tests / external callers.
pub fn pin_abs(board: &Board, sel: PinSel) -> Option<(f64, f64)> {
    board.pin_abs(sel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraint::{ConstraintSource, CostShape};

    // Reuse the lowering-test board shape via a local minimal builder
    // would duplicate a lot; instead test the gradient directionality
    // directly on a hand-built two-component board.
    use crate::types::*;
    use slotmap::SlotMap;

    fn two_comp_board(constraints: Vec<Constraint>) -> (Board, ComponentId, ComponentId) {
        let mut ck: SlotMap<ComponentId, ()> = SlotMap::with_key();
        let mut pk: SlotMap<PinId, ()> = SlotMap::with_key();
        let a = ck.insert(());
        let b = ck.insert(());
        let mk = |id, name: &str, x, y| Component {
            id,
            name: name.into(),
            refdes: name.into(),
            width_mm: 1.0,
            height_mm: 1.0,
            pins: vec![],
            side: BoardSide::Top,
            group: None,
            thermal_power_w: 0.0,
            package: "p".into(),
            placement: PlacementConstraint::Free,
            x,
            y,
            theta: 0.0,
            density_inflation: 1.0,
            layout_intents: vec![],
            bbox_dx: 0.0,
            bbox_dy: 0.0,
        };
        let _ = &mut pk;
        let board = Board {
            config: BoardConfig::default(),
            layer_stack: crate::stackup::stackup_preset(StackupPreset::TwoLayer),
            components: vec![mk(a, "A", 0.0, 0.0), mk(b, "B", 10.0, 0.0)],
            nets: vec![],
            groups: vec![],
            placement_recipes: Default::default(),
            constraints,
        };
        (board, a, b)
    }

    #[test]
    fn proximity_pulls_endpoints_together() {
        let src = ConstraintSource::intent("test");
        let (board, a, b) = two_comp_board(vec![]);
        let cons = vec![Constraint::Proximity {
            a: EntitySel::Component(a),
            b: EntitySel::Component(b),
            max_mm: 2.0,
            hardness: Hardness::Soft { shape: CostShape::Quadratic, weight: 1.0 },
            source: src,
        }];
        let board = Board { constraints: cons, ..board };
        let f = compute_proximity_forces(&board);
        // Forces are GRADIENTS; adam_step descends (x -= lr·grad). A is
        // left of B and too far → to move A toward B (+x), A's gradient
        // must be NEGATIVE in x; B's gradient positive.
        assert!(f.dx[0] < 0.0, "A's gradient should be -x (descends toward B), got {}", f.dx[0]);
        assert!(f.dx[1] > 0.0, "B's gradient should be +x (descends toward A), got {}", f.dx[1]);
    }
}
