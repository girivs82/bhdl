//! Intent recipes: one lowering function per `LayoutIntent` kind.
//!
//! Each recipe takes the self-component (the one carrying the intent),
//! the typed intent, and the `LoweringContext`, and emits zero or more
//! `Constraint`s per the "Lowers to" section of `intent_vocabulary_v0.md`
//! §4. A recipe that can't resolve a referenced pin drops that constraint
//! and records a diagnostic (warn-and-degrade).

use bhdl_common::intent::vocabulary::LayoutIntent;

use crate::constraint::{
    Constraint, ConstraintSource, CostShape, EntitySel, Hardness, LayerHintKind,
};
use crate::types::ComponentId;

use super::resolve::{LoweringContext, ResolveError};

/// Output of lowering one component's intents: emitted constraints plus
/// any resolution diagnostics (non-fatal).
#[derive(Default)]
pub struct RecipeOutput {
    pub constraints: Vec<Constraint>,
    pub diagnostics: Vec<String>,
}

impl RecipeOutput {
    fn warn(&mut self, kind: &str, err: ResolveError) {
        self.diagnostics
            .push(format!("intent `{kind}` dropped a constraint: {err}"));
    }
}

const RECIPE_VERSION: &str = "0";

fn src(kind: &str) -> ConstraintSource {
    ConstraintSource {
        file: String::new(),
        line: None,
        intent_kind: kind.into(),
        recipe_version: RECIPE_VERSION.into(),
    }
}

fn soft_quadratic(weight: f64) -> Hardness {
    Hardness::Soft { shape: CostShape::Quadratic, weight }
}

fn soft_linear(weight: f64) -> Hardness {
    Hardness::Soft { shape: CostShape::Linear, weight }
}

/// Lower a single component's intents into constraints.
pub fn lower_component_intents(
    self_id: ComponentId,
    intents: &[LayoutIntent],
    ctx: &LoweringContext,
) -> RecipeOutput {
    let mut out = RecipeOutput::default();
    for intent in intents {
        lower_one(self_id, intent, ctx, &mut out);
    }
    out
}

fn lower_one(
    self_id: ComponentId,
    intent: &LayoutIntent,
    ctx: &LoweringContext,
    out: &mut RecipeOutput,
) {
    let kind = intent.kind_name();
    match intent {
        // ── high_freq_bypass ─────────────────────────────────────────
        LayoutIntent::HighFreqBypass {
            rail,
            return_pin,
            loop_area_max_mm2,
            proximity_max_mm,
        } => {
            let rail_pin = match ctx.resolve_pin(self_id, rail) {
                Ok(p) => p,
                Err(e) => return out.warn(kind, e),
            };
            let ret_pin = match ctx.resolve_pin(self_id, return_pin) {
                Ok(p) => p,
                Err(e) => return out.warn(kind, e),
            };
            // Proximity: cap adjacent to the rail pin's component (hard).
            out.constraints.push(Constraint::Proximity {
                a: EntitySel::Component(self_id),
                b: EntitySel::Pin(rail_pin),
                max_mm: *proximity_max_mm,
                hardness: Hardness::Hard,
                source: src(kind),
            });
            // Loop area: rail → cap.1 → cap.2 → return (soft, quadratic).
            if let (Some(c1), Some(c2)) =
                (ctx.self_pin(self_id, 0), ctx.self_pin(self_id, 1))
            {
                out.constraints.push(Constraint::LoopArea {
                    loop_pins: vec![rail_pin, c1, c2, ret_pin],
                    max_mm2: *loop_area_max_mm2,
                    hardness: soft_quadratic(4.0),
                    source: src(kind),
                });
            }
            // Layer hint: adjacent to a ground plane (soft, linear).
            out.constraints.push(Constraint::LayerHint {
                component: self_id,
                hint: LayerHintKind::AdjacentToGroundPlane,
                hardness: soft_linear(1.0),
                source: src(kind),
            });
        }

        // ── bulk_reservoir ───────────────────────────────────────────
        LayoutIntent::BulkReservoir { rail, return_pin: _, proximity_max_mm } => {
            let rail_pin = match ctx.resolve_pin(self_id, rail) {
                Ok(p) => p,
                Err(e) => return out.warn(kind, e),
            };
            out.constraints.push(Constraint::Proximity {
                a: EntitySel::Component(self_id),
                b: EntitySel::Pin(rail_pin),
                max_mm: *proximity_max_mm,
                hardness: soft_linear(1.0),
                source: src(kind),
            });
        }

        // ── analog_ref_filter ────────────────────────────────────────
        LayoutIntent::AnalogRefFilter { ref_pin, return_pin, proximity_max_mm } => {
            let rp = match ctx.resolve_pin(self_id, ref_pin) {
                Ok(p) => p,
                Err(e) => return out.warn(kind, e),
            };
            let ret = match ctx.resolve_pin(self_id, return_pin) {
                Ok(p) => p,
                Err(e) => return out.warn(kind, e),
            };
            out.constraints.push(Constraint::Proximity {
                a: EntitySel::Component(self_id),
                b: EntitySel::Pin(rp),
                max_mm: *proximity_max_mm,
                hardness: Hardness::Hard,
                source: src(kind),
            });
            if let (Some(c1), Some(c2)) =
                (ctx.self_pin(self_id, 0), ctx.self_pin(self_id, 1))
            {
                out.constraints.push(Constraint::LoopArea {
                    loop_pins: vec![rp, c1, c2, ret],
                    max_mm2: 2.0,
                    hardness: soft_quadratic(4.0),
                    source: src(kind),
                });
            }
            // KeepAway-from-switching deferred until switching nets are
            // tagged by other intents (vocab §4.1 note).
        }

        // ── crystal_load_cap ─────────────────────────────────────────
        LayoutIntent::CrystalLoadCap {
            xtal_pin,
            return_pin: _,
            partner,
            proximity_max_mm,
        } => {
            let xp = match ctx.resolve_pin(self_id, xtal_pin) {
                Ok(p) => p,
                Err(e) => return out.warn(kind, e),
            };
            out.constraints.push(Constraint::Proximity {
                a: EntitySel::Component(self_id),
                b: EntitySel::Pin(xp),
                max_mm: *proximity_max_mm,
                hardness: Hardness::Hard,
                source: src(kind),
            });
            // Symmetric trace length to the partner cap is a length-match
            // group (router-side); represent partner proximity for now.
            if let Ok(partner_id) = ctx.resolve_component(partner) {
                out.constraints.push(Constraint::Proximity {
                    a: EntitySel::Component(self_id),
                    b: EntitySel::Component(partner_id),
                    max_mm: 6.0,
                    hardness: soft_linear(0.5),
                    source: src(kind),
                });
            }
        }

        // ── switching_input_filter ───────────────────────────────────
        LayoutIntent::SwitchingInputFilter {
            rail,
            return_pin,
            loop_area_max_mm2,
            switch_node_keepaway_mm: _,
        } => {
            let rail_pin = match ctx.resolve_pin(self_id, rail) {
                Ok(p) => p,
                Err(e) => return out.warn(kind, e),
            };
            let ret_pin = match ctx.resolve_pin(self_id, return_pin) {
                Ok(p) => p,
                Err(e) => return out.warn(kind, e),
            };
            out.constraints.push(Constraint::Proximity {
                a: EntitySel::Component(self_id),
                b: EntitySel::Pin(rail_pin),
                max_mm: 3.0,
                hardness: Hardness::Hard,
                source: src(kind),
            });
            if let (Some(c1), Some(c2)) =
                (ctx.self_pin(self_id, 0), ctx.self_pin(self_id, 1))
            {
                // The hot loop is EMC-critical → hard.
                out.constraints.push(Constraint::LoopArea {
                    loop_pins: vec![rail_pin, c1, c2, ret_pin],
                    max_mm2: *loop_area_max_mm2,
                    hardness: Hardness::Hard,
                    source: src(kind),
                });
            }
        }

        // ── feedback_divider ─────────────────────────────────────────
        LayoutIntent::FeedbackDivider {
            sense_node: _,
            fb_pin,
            keepaway_from,
            keepaway_min_mm,
        } => {
            if let Ok(fb) = ctx.resolve_pin(self_id, fb_pin) {
                out.constraints.push(Constraint::Proximity {
                    a: EntitySel::Component(self_id),
                    b: EntitySel::Pin(fb),
                    max_mm: 5.0,
                    hardness: soft_linear(1.0),
                    source: src(kind),
                });
            }
            if let Ok(sw) = ctx.resolve_pin(self_id, keepaway_from) {
                out.constraints.push(Constraint::KeepAway {
                    a: EntitySel::Component(self_id),
                    b: EntitySel::Pin(sw),
                    min_mm: *keepaway_min_mm,
                    hardness: soft_quadratic(2.0),
                    source: src(kind),
                });
            }
        }

        // ── snubber ──────────────────────────────────────────────────
        LayoutIntent::Snubber { across } => {
            let a = ctx.resolve_pin(self_id, &across.0);
            let b = ctx.resolve_pin(self_id, &across.1);
            if let (Ok(a), Ok(b)) = (a, b) {
                if let (Some(c1), Some(c2)) =
                    (ctx.self_pin(self_id, 0), ctx.self_pin(self_id, 1))
                {
                    out.constraints.push(Constraint::LoopArea {
                        loop_pins: vec![a, c1, c2, b],
                        max_mm2: 1.5,
                        hardness: soft_quadratic(3.0),
                        source: src(kind),
                    });
                }
            }
        }

        // ── series_termination ───────────────────────────────────────
        LayoutIntent::SeriesTermination { driver, line: _ } => {
            if let Ok(d) = ctx.resolve_pin(self_id, driver) {
                out.constraints.push(Constraint::Proximity {
                    a: EntitySel::Component(self_id),
                    b: EntitySel::Pin(d),
                    max_mm: 3.0,
                    hardness: Hardness::Hard,
                    source: src(kind),
                });
            }
        }

        // ── gate_resistor ────────────────────────────────────────────
        LayoutIntent::GateResistor { driver: _, gate } => {
            if let Ok(g) = ctx.resolve_pin(self_id, gate) {
                out.constraints.push(Constraint::Proximity {
                    a: EntitySel::Component(self_id),
                    b: EntitySel::Pin(g),
                    max_mm: 3.0,
                    hardness: Hardness::Hard,
                    source: src(kind),
                });
            }
        }

        // ── pullup / pulldown ────────────────────────────────────────
        // Geometrically unconstrained; the value is net classification,
        // which is a net-tag concern handled elsewhere. No geometry.
        LayoutIntent::Pullup { .. } | LayoutIntent::Pulldown { .. } => {}

        // ── current_sense ────────────────────────────────────────────
        // Kelvin topology → router-side Topology constraint (deferred to
        // the router integration); standard → no placement geometry.
        LayoutIntent::CurrentSense { .. } => {}
    }
}
