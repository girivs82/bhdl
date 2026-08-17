//! Concurrent Semantically-Aware PCB Place & Route Engine
//!
//! Co-optimizes component placement and trace routing in a unified iterative
//! loop. Unlike sequential approaches (place-then-route) or proxy-based methods
//! (Cypress's net crossing), this system runs actual PathFinder routing on a
//! coarse 3D grid during placement iterations, feeding real congestion and
//! via-count data back as placement forces.
//!
//! ## Architecture
//!
//! ```text
//! BHDL Pipeline → Semantic Preprocessor → Placement + Routing Loop → Output
//!
//! Placement forces: wirelength (LSE) + density (FFT) + group cohesion
//!                   + thermal spreading + congestion inflation + via penalty
//!
//! Routing: PathFinder negotiated congestion on coarse 3D grid
//!          (component-pitch cells, ~3000-7000 cells, runs in ms)
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use bhdl_pnr::{place_and_route, types::PnrConfig};
//!
//! let result = place_and_route(board, PnrConfig::default())?;
//! let kicad_pcb = bhdl_pnr::output::kicad::export_kicad_pcb(&result.board, &result.routes);
//! ```

pub mod det;
pub mod priors;
pub mod constraint;
pub mod geom;
use geom::{segment_point_too_close, segments_too_close};
pub mod feedback;
pub mod footprint;
pub mod intent;
pub mod ipc7351;
pub mod legalization;
pub mod output;
pub mod placement;
pub mod routing;
pub mod semantic;
pub mod stackup;
pub mod types;

use anyhow::Result;
use feedback::congestion;
use feedback::convergence::{ConvergenceAction, ConvergenceMonitor};
use log::{debug, info};
use placement::analytical;
use placement::grouping;
use placement::optimizer::{self, AdamState};
use routing::grid::RoutingGrid;
use routing::pathfinder;
use types::*;

/// P5 stage 2 — the MEASURED reward a trial is scored on: total
/// worst-couple crosstalk noise (mV) summed across signal nets,
/// computable only where the board carries a solved IBIS edge.
/// Real-Data policy: no measured edge anywhere → None, and trial
/// selection stays exactly the legality/connectivity/HPWL chain
/// (byte-identical). Measured edges present but no couple in
/// reach → Some(0.0): the copper genuinely measures quiet.
fn measured_noise_mv(result: &PnrResult) -> Option<f64> {
    if !result.board.nets.iter().any(|n| n.edge_swing_v.is_some()) {
        return None;
    }
    let mut total = 0.0;
    for (vi, n) in result.board.nets.iter().enumerate() {
        if !matches!(n.net_class, PnrNetClass::Signal) || n.plane_layer.is_some() {
            continue;
        }
        if let Some((mv, ..)) =
            routing::extract::crosstalk_worst_mv(&result.board, &result.routes, vi)
        {
            total += mv;
        }
    }
    Some(total)
}

/// Trial dominance: legality first (residual pad overlaps), then
/// connected sinks, then — ONLY on boards with a measured IBIS edge
/// (P5 stage 2) — lower total measured crosstalk noise, then HPWL.
fn trial_dominated(result: &PnrResult, best: &PnrResult, has_measured: bool) -> bool {
    let r_over = legalization::residual_pad_overlaps(&result.board);
    let b_over = legalization::residual_pad_overlaps(&best.board);
    if r_over != b_over {
        return r_over > b_over;
    }
    let r_conn = result.metrics.connected_sinks;
    let b_conn = best.metrics.connected_sinks;
    if r_conn != b_conn {
        return r_conn < b_conn;
    }
    // Pour defects (stranded islands, un-dropped plane pads) are
    // connectivity the sink counter can't see — same currency, same
    // rank. Always 0 without the pour experiment (byte-identical).
    let r_pd = result.metrics.pour_defects;
    let b_pd = best.metrics.pour_defects;
    if r_pd != b_pd {
        return r_pd > b_pd;
    }
    // SHIPPED DRC, priced. Since ee681c3 the verdict is re-taken on
    // final copper with a copper-aware check_drc, so this is what the
    // oracle will see — and until now it did not enter the currency
    // at all. A trial could reconnect one sink and ship four dangling
    // chains and DOMINATE a clean board (measured: span-only
    // amputation gated behind cheap_amputation moved no-post seed 42
    // from 0v/0unc to 1v/4unc, because sinks rank first and the
    // debris ranked nowhere). Byte-neutral wherever every trial ships
    // 0 — the whole corpus and ecc83 today.
    let r_drc = result.drc_violations.len();
    let b_drc = best.drc_violations.len();
    if r_drc != b_drc {
        return r_drc > b_drc;
    }
    if has_measured {
        let rn = measured_noise_mv(result).unwrap_or(0.0);
        let bn = measured_noise_mv(best).unwrap_or(0.0);
        if (rn - bn).abs() > 1e-9 {
            return rn > bn;
        }
    }
    // DETOUR ENVELOPE (fidelity boards only): equally-connected,
    // equally-quiet trials compete on routing quality against the
    // scale-free professional bar (p90 first — the tail is what
    // separates us from the hand-routed demos — then p50). Gated on
    // fidelity mode so every other board keeps the exact
    // legality/connectivity/HPWL chain (byte-identical).
    if result.board.config.route_bias.is_some()
        || result.board.config.design_track_width_mm.is_some()
    {
        let (r90, b90) = (result.metrics.detour_p90, best.metrics.detour_p90);
        if (r90 - b90).abs() > 1e-6 {
            return r90 > b90;
        }
        let (r50, b50) = (result.metrics.detour_p50, best.metrics.detour_p50);
        if (r50 - b50).abs() > 1e-6 {
            return r50 > b50;
        }
    }
    result.metrics.hpwl_mm >= best.metrics.hpwl_mm
}


/// Run one tier's trials CONCURRENTLY — each trial is an independent
/// pipeline over its own board clone + per-trial seed, so results in
/// TRIAL ORDER feed the caller's unchanged fold/tie-breaks/early
/// breaks and the winner is byte-identical to the serial run (an
/// early break just discards precomputed extras). All engine scratch
/// state is thread_local; the only process-wide mutables are metric
/// atomics. Escape hatch: BHDL_PNR_SERIAL_TRIALS=1.
fn run_tier_trials(
    board: &Board,
    config: &PnrConfig,
    trials: usize,
    base_seed: u64,
    prep: &(dyn Fn(&mut Board) + Sync),
) -> Vec<Result<PnrResult>> {
    if trials <= 1 || std::env::var("BHDL_PNR_SERIAL_TRIALS").is_ok() {
        return (0..trials)
            .map(|t| {
                let mut tb = board.clone();
                prep(&mut tb);
                place_and_route(tb, config.clone(), base_seed.wrapping_add(t as u64))
            })
            .collect();
    }
    std::thread::scope(|sc| {
        let handles: Vec<_> = (0..trials)
            .map(|t| {
                let mut tb = board.clone();
                prep(&mut tb);
                let cfg = config.clone();
                sc.spawn(move || {
                    place_and_route(tb, cfg, base_seed.wrapping_add(t as u64))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("trial thread panicked"))
            .collect()
    })
}

/// Run SEVERAL tiers' trials at once, bounded by the machine.
///
/// The tier ladder is a fallback chain — each tier runs only when the
/// best result so far is still imperfect — and it was executed as a
/// chain of barriers: three trials wide, five tiers deep, so a 14-core
/// machine sat at 2.66x average parallelism (measured: real 436s,
/// user 1160s, exactly three busy worker threads).
///
/// The tiers in this chain are INDEPENDENT: each prep mutates only
/// config knobs on a fresh clone of the ORIGINAL board, and none of
/// them reads a previous tier's result. So they can all be computed up
/// front and thrown away unused, which is what the caller's early
/// break already does to surplus trials within a tier. The fold, the
/// tie-breaks and the winner are untouched — only the moment of
/// computation moves.
///
/// Jobs are pulled from one counter by a fixed pool, so concurrency is
/// bounded by cores rather than by tiers x trials, and each result
/// lands in its own slot: the returned per-tier vectors are in trial
/// order regardless of completion order. Escape hatches:
/// BHDL_PNR_SERIAL_TRIALS=1 (fully serial), BHDL_PNR_NO_SPECULATION=1
/// (tier-at-a-time, trials still parallel), BHDL_PNR_MAX_TRIAL_THREADS
/// to cap the pool when memory matters more than time.
fn run_tiers_speculative(
    board: &Board,
    config: &PnrConfig,
    trials: usize,
    base_seed: u64,
    preps: &[&(dyn Fn(&mut Board) + Sync)],
) -> Vec<Vec<Result<PnrResult>>> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let ntier = preps.len();
    if trials == 0 || ntier == 0 {
        return (0..ntier).map(|_| Vec::new()).collect();
    }
    if trials == 1
        || std::env::var("BHDL_PNR_SERIAL_TRIALS").is_ok()
        || std::env::var("BHDL_PNR_NO_SPECULATION").is_ok()
    {
        return preps
            .iter()
            .map(|p| run_tier_trials(board, config, trials, base_seed, *p))
            .collect();
    }
    let total = ntier * trials;
    let cap = std::env::var("BHDL_PNR_MAX_TRIAL_THREADS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
        });
    let workers = total.min(cap).max(1);
    let next = AtomicUsize::new(0);
    let out: std::sync::Mutex<Vec<(usize, Result<PnrResult>)>> =
        std::sync::Mutex::new(Vec::with_capacity(total));
    std::thread::scope(|sc| {
        for _ in 0..workers {
            sc.spawn(|| loop {
                let job = next.fetch_add(1, Ordering::SeqCst);
                if job >= total {
                    break;
                }
                let (ti, tr) = (job / trials, job % trials);
                let mut tb = board.clone();
                (preps[ti])(&mut tb);
                let r = place_and_route(
                    tb,
                    config.clone(),
                    base_seed.wrapping_add(tr as u64),
                );
                out.lock().expect("trial results poisoned").push((job, r));
            });
        }
    });
    let mut done = out.into_inner().expect("trial results poisoned");
    done.sort_by_key(|(j, _)| *j);
    let mut tiers: Vec<Vec<Result<PnrResult>>> =
        (0..ntier).map(|_| Vec::with_capacity(trials)).collect();
    for (j, r) in done {
        tiers[j / trials].push(r);
    }
    tiers
}

/// Run multiple placement+routing trials with different initializations,
/// return the best result (highest routability, then lowest HPWL; on
/// boards with measured IBIS edges, lower measured crosstalk breaks
/// the tie between equally-connected trials — P5 stage 2).
pub fn place_and_route_best_of(
    board: Board,
    config: PnrConfig,
    trials: usize,
    base_seed: u64,
) -> Result<PnrResult> {
    let mut best: Option<PnrResult> = None;
    // P5 stage 2 switch: with a solved edge on the board the trials
    // compete on MEASURED physics too, so a perfect trial no longer
    // short-circuits the loop — later trials may measure quieter.
    // Boards without measured data keep the early break, preserving
    // byte-identity.
    let has_measured = board.nets.iter().any(|n| n.edge_swing_v.is_some());

    // The five tiers below are a FALLBACK CHAIN, not a dependency
    // chain: each prep touches only config knobs on a clone of this
    // same board, and none reads a previous tier's result. Computing
    // them together turns five barriers into one and lets the pool
    // fill the machine; a tier that never gets consumed is discarded
    // exactly as surplus trials already are. Only the noise-feedback
    // tier further down genuinely depends on `best`, so it stays put.
    let p_base: &(dyn Fn(&mut Board) + Sync) = &|_b: &mut Board| {};
    let p_si: &(dyn Fn(&mut Board) + Sync) =
        &|b: &mut Board| b.config.si_return_cost = true;
    let p_fan: &(dyn Fn(&mut Board) + Sync) =
        &|b: &mut Board| b.config.fanout_first = true;
    let p_amp: &(dyn Fn(&mut Board) + Sync) =
        &|b: &mut Board| b.config.cheap_amputation = true;
    let p_esc: &(dyn Fn(&mut Board) + Sync) = &|b: &mut Board| {
        b.config.escape_demand = 2.0;
        b.config.fanout_first = true;
    };
    let mut spec = run_tiers_speculative(
        &board,
        &config,
        trials,
        base_seed,
        &[p_base, p_si, p_fan, p_amp, p_esc],
    );
    let tier = std::mem::take(&mut spec[0]);
    for (trial, result) in tier.into_iter().enumerate() {
        info!("=== Trial {}/{} ===", trial + 1, trials);
        let result = result?;

        // PLACEMENT LEGALITY FIRST: a trial shipping a residual
        // pad-box overlap grades as clearance + mask-bridge
        // violations no routing quality can buy back — it can never
        // beat an overlap-free trial (uno s99: an illegal-placement
        // trial out-scored a legal one on connected sinks and shipped
        // 7 violations). Then more CONNECTED SINKS (counting
        // non-empty routes let a trial shipping one surviving branch
        // and 19 stranded pads tie a fully-connected one), then
        // measured noise where it exists, then lower HPWL.
        let r_over = legalization::residual_pad_overlaps(&result.board);
        let dominated = best
            .as_ref()
            .map_or(false, |b| trial_dominated(&result, b, has_measured));

        if !dominated {
            match measured_noise_mv(&result) {
                Some(mv) if has_measured => info!(
                    "Trial {} is new best: {} connected sink(s), HPWL={:.1}mm, measured noise {mv:.1}mV",
                    trial + 1, result.metrics.connected_sinks, result.metrics.hpwl_mm
                ),
                _ => info!(
                    "Trial {} is new best: {} connected sink(s), HPWL={:.1}mm",
                    trial + 1, result.metrics.connected_sinks, result.metrics.hpwl_mm
                ),
            }
            let total_sinks: usize = result
                .board
                .nets
                .iter()
                .filter(|n| n.pins.len() >= 2)
                .map(|n| n.pins.len())
                .sum();
            let perfect = result.metrics.connected_sinks >= total_sinks
                && result.metrics.pour_defects == 0
                && result.drc_violations.is_empty()
                && r_over == 0;
            best = Some(result);
            if perfect && !has_measured {
                info!(
                    "Trial {} fully connected with no DRC — skipping remaining trials",
                    trial + 1
                );
                break;
            }
        }
    }

    // KNOB-AWARE TIER: when the best off-tier trial is imperfect, try
    // the same seeds with the P4 return-path cost ON — measured on the
    // uno it lifts whole seeds to perfect (s13/s99 0v/3unc -> 0/0)
    // while costing others their perfection; running it only as a
    // FALLBACK keeps every already-perfect board byte-identical and
    // lets dominance pick per board. (The clean A/B behind this:
    // knob-on 4 seeds 0v unc 1/1/0/0 vs knob-off 0/0/3/3.)
    let best_imperfect = best.as_ref().map_or(true, |b| {
        let total_sinks: usize = b
            .board
            .nets
            .iter()
            .filter(|n| n.pins.len() >= 2)
            .map(|n| n.pins.len())
            .sum();
        b.metrics.connected_sinks < total_sinks
            || b.metrics.pour_defects > 0
            || !b.drc_violations.is_empty()
            || legalization::residual_pad_overlaps(&b.board) > 0
    });
    if best_imperfect {
        let tier = std::mem::take(&mut spec[1]);
        for (trial, result) in tier.into_iter().enumerate() {
            info!("=== SI-cost trial {}/{} ===", trial + 1, trials);
            let result = result?;
            let r_over = legalization::residual_pad_overlaps(&result.board);
            let dominated = best
                .as_ref()
                .map_or(false, |b| trial_dominated(&result, b, has_measured));
            if !dominated {
                info!(
                    "SI-cost trial {} is new best: {} connected sink(s), HPWL={:.1}mm",
                    trial + 1, result.metrics.connected_sinks, result.metrics.hpwl_mm
                );
                let total_sinks: usize = result
                    .board
                    .nets
                    .iter()
                    .filter(|n| n.pins.len() >= 2)
                    .map(|n| n.pins.len())
                    .sum();
                let perfect = result.metrics.connected_sinks >= total_sinks
                    && result.metrics.pour_defects == 0
                    && result.drc_violations.is_empty()
                    && r_over == 0;
                best = Some(result);
                if perfect {
                    break;
                }
            }
        }
    }
    // FANOUT-FIRST TIER: still imperfect after the SI tier → retry
    // with plane-pad fanout claimed BEFORE signal routing. Measured
    // both ways on the uno: it CURES the walled-in-QFP-ground-pad
    // class (s99's UGND, dead through every completion rung) but the
    // global perturbation cost a previously-perfect seed a sink —
    // so, like the SI knob, it only ever runs as a fallback where
    // dominance can police it per board.
    let still_imperfect = best.as_ref().map_or(true, |b| {
        let total_sinks: usize = b
            .board
            .nets
            .iter()
            .filter(|n| n.pins.len() >= 2)
            .map(|n| n.pins.len())
            .sum();
        b.metrics.connected_sinks < total_sinks
            || b.metrics.pour_defects > 0
            || !b.drc_violations.is_empty()
            || legalization::residual_pad_overlaps(&b.board) > 0
    });
    if still_imperfect {
        let tier = std::mem::take(&mut spec[2]);
        for (trial, result) in tier.into_iter().enumerate() {
            info!("=== Fanout-first trial {}/{} ===", trial + 1, trials);
            let result = result?;
            let dominated = best
                .as_ref()
                .map_or(false, |b| trial_dominated(&result, b, has_measured));
            if !dominated {
                info!(
                    "Fanout-first trial {} is new best: {} connected sink(s), HPWL={:.1}mm",
                    trial + 1,
                    result.metrics.connected_sinks,
                    result.metrics.hpwl_mm
                );
                let total_sinks: usize = result
                    .board
                    .nets
                    .iter()
                    .filter(|n| n.pins.len() >= 2)
                    .map(|n| n.pins.len())
                    .sum();
                let r_over = legalization::residual_pad_overlaps(&result.board);
                let perfect = result.metrics.connected_sinks >= total_sinks
                    && result.metrics.pour_defects == 0
                    && result.drc_violations.is_empty()
                    && r_over == 0;
                best = Some(result);
                if perfect {
                    break;
                }
            }
        }
    }

    // CHEAP-AMPUTATION TIER: still imperfect → retry with the
    // validator choosing amputees by REPAIR COST instead of net
    // weight. Cures the whole-net-in-disguise rip class (ecc83
    // strict: one bad corner cost K2 26/27 segments and recovery was
    // walled by the survivor) but shifts every later rip — measured
    // both fixing and breaking boards, so it only runs where
    // dominance can police it.
    let still_imperfect2 = best.as_ref().map_or(true, |b| {
        let total_sinks: usize = b
            .board
            .nets
            .iter()
            .filter(|n| n.pins.len() >= 2)
            .map(|n| n.pins.len())
            .sum();
        b.metrics.connected_sinks < total_sinks
            || b.metrics.pour_defects > 0
            || !b.drc_violations.is_empty()
            || legalization::residual_pad_overlaps(&b.board) > 0
    });
    if still_imperfect2 {
        let tier = std::mem::take(&mut spec[3]);
        for (trial, result) in tier.into_iter().enumerate() {
            info!("=== Cheap-amputation trial {}/{} ===", trial + 1, trials);
            let result = result?;
            let dominated = best
                .as_ref()
                .map_or(false, |b| trial_dominated(&result, b, has_measured));
            if !dominated {
                info!(
                    "Cheap-amputation trial {} is new best: {} connected sink(s), HPWL={:.1}mm",
                    trial + 1,
                    result.metrics.connected_sinks,
                    result.metrics.hpwl_mm
                );
                let total_sinks: usize = result
                    .board
                    .nets
                    .iter()
                    .filter(|n| n.pins.len() >= 2)
                    .map(|n| n.pins.len())
                    .sum();
                let r_over = legalization::residual_pad_overlaps(&result.board);
                let perfect = result.metrics.connected_sinks >= total_sinks
                    && result.metrics.pour_defects == 0
                    && result.drc_violations.is_empty()
                    && r_over == 0;
                best = Some(result);
                if perfect {
                    break;
                }
            }
        }
    }

    // ESCAPE-DEMAND TIER: still imperfect after the fanout tier →
    // re-place with IC pin rows projecting fanout-corridor demand
    // into the density map (scale 2.0) AND fanout-first pre-drops.
    // The two heavy levers COMPOSE: the aisle gives fanout-first the
    // corridor room, the pre-drops give the plane pads their vias
    // before signals fill it — measured, this exact combination is
    // what cleared s99's conserved congestion debt to 0/0 (the
    // seed's first perfect ever; escape alone and fanout alone both
    // measured 1unc, scale 1.0 insufficient). Global perturbation,
    // so like every heavy lever it only runs where dominance can
    // police it.
    let pre_escape_imperfect = best.as_ref().map_or(true, |b| {
        let total_sinks: usize = b
            .board
            .nets
            .iter()
            .filter(|n| n.pins.len() >= 2)
            .map(|n| n.pins.len())
            .sum();
        b.metrics.connected_sinks < total_sinks
            || b.metrics.pour_defects > 0
            || !b.drc_violations.is_empty()
            || legalization::residual_pad_overlaps(&b.board) > 0
    });
    if pre_escape_imperfect {
        let tier = std::mem::take(&mut spec[4]);
        for (trial, result) in tier.into_iter().enumerate() {
            info!("=== Escape-demand trial {}/{} ===", trial + 1, trials);
            let result = result?;
            let dominated = best
                .as_ref()
                .map_or(false, |b| trial_dominated(&result, b, has_measured));
            if !dominated {
                info!(
                    "Escape-demand trial {} is new best: {} connected sink(s), HPWL={:.1}mm",
                    trial + 1,
                    result.metrics.connected_sinks,
                    result.metrics.hpwl_mm
                );
                let total_sinks: usize = result
                    .board
                    .nets
                    .iter()
                    .filter(|n| n.pins.len() >= 2)
                    .map(|n| n.pins.len())
                    .sum();
                let r_over = legalization::residual_pad_overlaps(&result.board);
                let perfect = result.metrics.connected_sinks >= total_sinks
                    && result.metrics.pour_defects == 0
                    && result.drc_violations.is_empty()
                    && r_over == 0;
                best = Some(result);
                if perfect {
                    break;
                }
            }
        }
    }

    // P5 STAGE 3 — measured rewards feed PLACEMENT: when a declared
    // noise budget FAILS on measured numbers, invert the coupling
    // model into the separation the budget demands —
    //   k_b = 0.25/(1+(s/h)^2)  →  s = h·sqrt(0.25·swing/budget − 1)
    // — and re-run the trials with soft KeepAways holding the
    // victim's components that far from the aggressor's. The
    // constraint is a lever, not the judge: acceptance stays with
    // trial_dominated, so a feedback trial ships only when legality
    // and connectivity hold and the copper MEASURES quieter.
    if has_measured {
        let mut fb: Vec<crate::constraint::Constraint> = Vec::new();
        if let Some(b) = &best {
            use crate::constraint::{Constraint, ConstraintSource, CostShape, EntitySel, Hardness};
            for c in &b.board.constraints {
                let Constraint::NoiseBudget { net, max_mv, .. } = c else {
                    continue;
                };
                let Some(vi) = b.board.nets.iter().position(|n| n.id == *net) else {
                    continue;
                };
                let Some((mv, ai, _, h, sw)) =
                    routing::extract::crosstalk_worst_mv(&b.board, &b.routes, vi)
                else {
                    continue;
                };
                if mv <= *max_mv as f64 {
                    continue;
                }
                let kb_needed = *max_mv as f64 / 1000.0 / sw;
                if kb_needed <= 0.0 || kb_needed >= 0.25 {
                    continue;
                }
                // 1.5× headroom over the physics floor: the KeepAway
                // acts on COMPONENT centers while the budget governs
                // TRACE gaps, and traces sag toward their pads (1.25×
                // measured 6.6mm of trace gap where 6.8mm was needed —
                // the discrepancy eats more than a quarter). The margin
                // is a lever setting, not a claim — acceptance stays
                // with the re-measurement.
                let s_needed = 1.5 * h * (0.25 / kb_needed - 1.0).sqrt();
                info!(
                    "P5 stage 3: noise budget {} FAILING measured ({mv:.1}mV > {max_mv}mV vs {}) — budget demands {s_needed:.2}mm separation (h={h:.2}mm, swing {sw:.2}V), re-placing",
                    b.board.nets[vi].name, b.board.nets[ai].name
                );
                let src = ConstraintSource {
                    file: String::new(),
                    line: None,
                    intent_kind: "p5_noise_feedback".into(),
                    recipe_version: "0".into(),
                };
                let vcomps: std::collections::BTreeSet<ComponentId> =
                    b.board.nets[vi].pins.iter().map(|&(c, _)| c).collect();
                let acomps: std::collections::BTreeSet<ComponentId> =
                    b.board.nets[ai].pins.iter().map(|&(c, _)| c).collect();
                for &cv in &vcomps {
                    for &ca in &acomps {
                        if cv == ca {
                            continue;
                        }
                        // Weight 12: this spring must beat the shared-
                        // rail wirelength basin that CAUSED the couple
                        // (weight-4 feedback moved 1 of 3 trials).
                        fb.push(Constraint::KeepAway {
                            a: EntitySel::Component(cv),
                            b: EntitySel::Component(ca),
                            min_mm: s_needed as f32,
                            hardness: Hardness::Soft {
                                shape: CostShape::Quadratic,
                                weight: 12.0,
                            },
                            source: src.clone(),
                        });
                    }
                }
            }
        }
        if !fb.is_empty() {
            let tier = run_tier_trials(&board, &config, trials, base_seed, &|b| {
                b.constraints.extend(fb.iter().cloned());
            });
            for (trial, result) in tier.into_iter().enumerate() {
                info!("=== Noise-feedback trial {}/{} ===", trial + 1, trials);
                let result = result?;
                let dominated = best
                    .as_ref()
                    .map_or(false, |b| trial_dominated(&result, b, true));
                if !dominated {
                    info!(
                        "Noise-feedback trial {} is new best: {} connected sink(s), measured noise {:.1}mV",
                        trial + 1,
                        result.metrics.connected_sinks,
                        measured_noise_mv(&result).unwrap_or(0.0)
                    );
                    best = Some(result);
                }
            }
        }
    }

    // WINNER-ONLY mirror dump: the per-trial tail runs many times and
    // its files hold whichever trial finished last — never diff those
    // against the export (that mistake produced a phantom
    // mirror-vs-export divergence spanning half the board).
    if let Ok(dir) = std::env::var("BHDL_PNR_DUMP_FINAL_MIRROR") {
        if let Some(res) = &best {
            use std::io::Write;
            for ni in 0..res.board.nets.len() {
                if res.board.nets[ni].plane_layer.is_none() {
                    continue;
                }
                let Some(polys) =
                    output::kicad::emission_fill_polys(&res.board, &res.routes, ni)
                else {
                    continue;
                };
                if let Ok(mut f) = std::fs::File::create(format!(
                    "{dir}/final_mirror_{}.txt",
                    res.board.nets[ni].name.replace('/', "_")
                )) {
                    for p in &polys {
                        for (x, y) in p {
                            let _ = writeln!(f, "{x} {y}");
                        }
                        let _ = writeln!(f, "---");
                    }
                }
            }
        }
    }
    best.ok_or_else(|| anyhow::anyhow!("No trials completed"))
}

/// Run the concurrent place & route loop.
///
/// Input: a fully constructed `Board` (from semantic preprocessing).
/// Output: `PnrResult` with final placement, routes, metrics, and DRC.
/// Debug tracer: BHDL_PNR_TRACE_NEAR=x,y,r prints every segment/via of
/// every net within r mm of (x,y) at each tagged tail phase. Diagnoses
/// "who moved my copper" between rungs without a debugger.
fn trace_near(tag: &str, board: &Board, routes: &[Route]) {
    let Ok(v) = std::env::var("BHDL_PNR_TRACE_NEAR") else { return };
    let mut it = v.split(',').filter_map(|t| t.trim().parse::<f64>().ok());
    let (Some(x), Some(y), Some(r)) = (it.next(), it.next(), it.next()) else { return };
    for (ni, rt) in routes.iter().enumerate() {
        let name = board.nets.get(ni).map(|n| n.name.as_str()).unwrap_or("?");
        for sg in &rt.segments {
            if (sg.start.0 - x).hypot(sg.start.1 - y) <= r || (sg.end.0 - x).hypot(sg.end.1 - y) <= r {
                info!("[trace {tag}] '{name}' seg L{} ({:.2},{:.2})->({:.2},{:.2})", sg.layer, sg.start.0, sg.start.1, sg.end.0, sg.end.1);
            }
        }
        for vv in &rt.vias {
            if (vv.x - x).hypot(vv.y - y) <= r {
                info!("[trace {tag}] '{name}' via ({:.2},{:.2})", vv.x, vv.y);
            }
        }
    }
}

pub fn place_and_route(mut board: Board, config: PnrConfig, seed: u64) -> Result<PnrResult> {
    // Residual emission-model strandings the island-bridge pass
    // (5.998) could not close — added to pour_defects so trial
    // selection prefers placements where every bridge landed.
    let mut pour_bridge_residual = 0usize;
    // INVARIANT REPAIR: every pin stamped `pin.net = Some(n)` must
    // appear in that net's pins list. The exporter writes pad nets
    // from pin.net, so a pad missing from net.pins is INVISIBLE to
    // every pin-driven pass (drops, rescue, unreached counts) while
    // KiCad still expects it connected — U4's AVCC shipped stranded
    // with zero pipeline log lines because nothing ever iterated it.
    {
        let mut repaired = 0usize;
        for comp in &board.components {
            for pin in &comp.pins {
                let Some(nid) = pin.net else { continue };
                if pin.unplaced {
                    continue;
                }
                if let Some(net) = board.nets.iter_mut().find(|n| n.id == nid) {
                    if !net
                        .pins
                        .iter()
                        .any(|&(c, p)| c == comp.id && p == pin.pin_id)
                    {
                        net.pins.push((comp.id, pin.pin_id));
                        repaired += 1;
                    }
                }
            }
        }
        if repaired > 0 {
            info!(
                "net-pin reconcile: {repaired} pad(s) stamped on a net but missing from its pin list"
            );
        }
    }
    if log::log_enabled!(log::Level::Debug) {
        let mut fp: u64 = 0xcbf29ce484222325;
        let mut mix = |b: &[u8]| {
            for &x in b {
                fp ^= x as u64;
                fp = fp.wrapping_mul(0x100000001b3);
            }
        };
        for c in &board.components {
            mix(c.name.as_bytes());
            mix(&c.x.to_bits().to_le_bytes());
            mix(&c.y.to_bits().to_le_bytes());
            for p in &c.pins {
                mix(p.name.as_bytes());
                mix(format!("{:?}", p.net).as_bytes());
            }
        }
        for n in &board.nets {
            mix(n.name.as_bytes());
            mix(&n.required_trace_width_mm.to_bits().to_le_bytes());
        }
        log::debug!("pnr input fingerprint: {fp:016x}");
    }
    let n = board.components.len();
    info!(
        "Starting P&R: {} components, {} nets, {} layers",
        n,
        board.nets.len(),
        board.layer_stack.layers.len()
    );

    // 0. Detect constraint contradictions before placement (§9). v0 logs
    //    them prominently and proceeds; hard-fail-on-conflict is gated
    //    behind a future board layout_policy.
    if !board.constraints.is_empty() {
        let conflicts = constraint::conflicts::detect_conflicts(&board.constraints);
        if !conflicts.is_empty() {
            let (errors, warnings) =
                constraint::conflicts::count_by_severity(&conflicts);
            info!(
                "constraint conflicts: {} error(s), {} warning(s)",
                errors, warnings
            );
            for c in &conflicts {
                match c.severity {
                    constraint::conflicts::Severity::Error => {
                        log::error!("{}", c.describe())
                    }
                    constraint::conflicts::Severity::Warning => {
                        log::warn!("{}", c.describe())
                    }
                }
            }
        }
    }

    // 1. Initialize placement (block-based with datasheet patterns)
    let recipes = board.placement_recipes.clone();
    placement::initialize(&mut board, seed, &recipes);
    let anchor_idx = placement::find_anchor(&board);

    // 1.5 MIRROR CLASSES (fidelity mode): repeated-entity siblings —
    // each suffix class holds one free counterpart per sibling group
    // plus that group's FIXED-member anchor. The mirror force below
    // attracts every counterpart toward (anchor + family-mean offset)
    // DURING the solve, so ONE channel layout emerges that fits every
    // column (the post-placement stamp teleport measured worse:
    // copper 3244 -> 4730mm — coherence must come from the optimizer,
    // not a paste-over).
    // MIRROR CONSTRAINT — LANDED DORMANT (BHDL_PNR_MIRROR=1). Three
    // formulations measured on the mixer, none beat baseline:
    //   post-placement stamp: teleport breaks coherence (3244->4730mm)
    //   mirror force k=2 + init-stamp + snap: solve tears the stamp
    //     apart (snap residual 109mm)
    //   k=30: residual 105mm — k-INSENSITIVE, Adam normalizes
    //     gradients so force magnitude cannot cross obstacle basins.
    // Next formulation = RIGID-BODY group moves: freeze sibling
    // groups' internal offsets from the init-stamp and move each
    // group by its AVERAGED force (an optimizer mode, not a force
    // term); skip detailed-refine for rigid members.
    let mirror_classes: Vec<Vec<(usize, (f64, f64))>> = if std::env::var("BHDL_PNR_MIRROR")
        .is_ok()
        && (board.config.route_bias.is_some()
            || board.config.design_track_width_mm.is_some())
    {
        sibling_suffix_classes(&board)
    } else {
        Vec::new()
    };
    // ROUTE-STAMP bookkeeping: a certified channel solve carries its
    // copper forward to routing time. Recorded at init-stamp; consumed
    // after placement settles (blocks translate rigidly, so the copper
    // translates with them — verified, never assumed).
    struct StampedChannel {
        /// Reference group's parent component indices (group order).
        ref_members: Vec<usize>,
        /// Their positions at stamp time (board coords) — the frame
        /// the routes were certified in.
        ref_stamp_pos: Vec<(f64, f64)>,
        /// All sibling groups, reference at [0], member-parallel.
        groups: Vec<Vec<usize>>,
        /// Expected block delta per group vs the reference frame (the
        /// anchor translation) — the frame each sibling's copper is
        /// licensed in.
        expect: Vec<(f64, f64)>,
        /// Whether the group was frozen (edge-safe); unfrozen groups
        /// stay on the rigid machinery and never stamp.
        frozen: Vec<bool>,
        /// Certified copper in stamp-time board coords, parent NetIds.
        routes: Vec<(NetId, Route)>,
    }
    let mut stamped_channels: Vec<StampedChannel> = Vec::new();
    // RIGID-BODY GROUP MOVES (BHDL_PNR_RIGID=1, fidelity): the third
    // formulation of the channel-coherence experiment — freeze each
    // sibling group's internal geometry from the init-stamp and move
    // the group as ONE body (averaged forces + per-iteration snap).
    // The force formulation was basin-blind (105mm residual,
    // k-insensitive); rigidity makes divergence impossible instead of
    // penalized. (leader, members(ci, dx, dy from leader)) per group.
    let rigid_groups: Vec<(usize, Vec<(usize, f64, f64)>)> = if std::env::var(
        "BHDL_PNR_RIGID",
    )
    .is_ok()
        && (board.config.route_bias.is_some()
            || board.config.design_track_width_mm.is_some())
    {
        let stamped = stamp_sibling_groups(&mut board);
        let classes = sibling_suffix_classes(&board);
        // Regroup suffix classes by sibling: class[k] = one member per
        // sibling in sorted-group order, so column k across classes =
        // sibling k's members.
        let mut groups: Vec<Vec<usize>> = Vec::new();
        if let Some(first) = classes.first() {
            groups = vec![Vec::new(); first.len()];
            for class in &classes {
                if class.len() != groups.len() {
                    continue;
                }
                for (k, &(ci, _)) in class.iter().enumerate() {
                    groups[k].push(ci);
                }
            }
        }
        // COLUMN-FEASIBLE STRIP SYNTHESIS: the init-stamp froze the
        // analytical-init blob, which is wider than the inter-anchor
        // pitch — no legalizer can fit four copies of the wrong shape
        // (measured trail 144v -> 22v, residual = shape). Rigid
        // internals are FROZEN, so the shape must be right at birth:
        // synthesize the reference channel as a single-file vertical
        // strip at its anchor x (the demo's channel-strip idiom),
        // skipping the y-intervals its column's FIXED parts occupy,
        // then stamp the strip to every sibling.
        let anchors: Vec<(f64, f64)> = classes
            .first()
            .map(|c| c.iter().map(|&(_, a)| a).collect())
            .unwrap_or_default();
        if !groups.is_empty() && anchors.len() == groups.len() {
            // Column half-width from the tightest anchor separation
            // on the dominant axis.
            let xs: Vec<f64> = anchors.iter().map(|a| a.0).collect();
            let ys: Vec<f64> = anchors.iter().map(|a| a.1).collect();
            let spread = |v: &Vec<f64>| {
                v.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
                    - v.iter().cloned().fold(f64::INFINITY, f64::min)
            };
            let x_major = spread(&xs) >= spread(&ys);
            let coords = if x_major { &xs } else { &ys };
            let mut sorted = coords.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let min_sep = sorted
                .windows(2)
                .map(|w| w[1] - w[0])
                .fold(f64::INFINITY, f64::min);
            let half_w = (min_sep / 2.0 - 1.0).max(2.0);
            let (ax, ay) = anchors[0];
            let _ = ay;
            // Fixed parts whose envelope intersects the reference
            // column: their cross-axis intervals are unplaceable.
            let axis_lo = if x_major { ax - half_w } else { anchors[0].1 - half_w };
            let axis_hi = if x_major { ax + half_w } else { anchors[0].1 + half_w };
            let mut blocked: Vec<(f64, f64)> = Vec::new();
            for c in &board.components {
                if !c.placement.is_fixed() {
                    continue;
                }
                let (cx, cy, hw, hh) = c.envelope();
                let (on_axis, cross, cross_h) = if x_major {
                    (cx, cy, hh)
                } else {
                    (cy, cx, hw)
                };
                let on_h = if x_major { hw } else { hh };
                if on_axis + on_h > axis_lo && on_axis - on_h < axis_hi {
                    blocked.push((cross - cross_h - 0.75, cross + cross_h + 0.75));
                }
            }
            blocked.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            // Stack the reference group's free members single-file
            // along the cross axis, ordered by their solved cross
            // coordinate (keeps net affinity), skipping blocked
            // intervals.
            let cross_extent = if x_major {
                board.config.outline.height()
            } else {
                board.config.outline.width()
            };
            let mut order: Vec<usize> = groups[0].clone();
            order.sort_by(|&a, &b| {
                let ka = if x_major { board.components[a].y } else { board.components[a].x };
                let kb = if x_major { board.components[b].y } else { board.components[b].x };
                ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
            });
            // PER-COLUMN CERTIFICATES (settled formulation): EVERY
            // sibling column becomes its own mini-board — free
            // members + that column's OWN fixed parts + boundary
            // pads for the nets that leave — solved with the full
            // engine and accepted only when it ROUTES clean.
            // Translating the reference certificate to siblings was
            // measured unlicensed: columns have DIFFERENT foreign
            // obstacle patterns (ch1's band holds ch2's jack, ch4's
            // none) — 4 shorts + 5 unc. Certified columns FREEZE at
            // their solve and stamp their own copper; the region is
            // clamped to the board margin so certified placements
            // are edge-legal BY CONSTRUCTION (the old post-hoc
            // edge clamp is gone). REGION WIDENING per column: the
            // margin against neighbor anchors shrinks 1.0 -> 0.25mm
            // when the inner pad ladder exhausts.
            let ec = board.config.edge_clearance_mm;
            let mut col_cert: Vec<bool> = vec![false; groups.len()];
            let mut ref_cert_routes: Vec<(NetId, Route)> = Vec::new();
            for k in 0..groups.len() {
                let (akx, aky) = anchors[k];
                // TRANSLATE-VERIFY first (uniformity doctrine): the
                // demo ships IDENTICAL strips. Try the reference
                // certificate translated to this column; geometry
                // verification decides per column, independent solve
                // stays as the fallback.
                if k > 0 && col_cert[0] && !ref_cert_routes.is_empty() {
                    let delta =
                        (anchors[k].0 - anchors[0].0, anchors[k].1 - anchors[0].1);
                    if let Some(routes) = try_channel_transfer(
                        &mut board,
                        &groups[0],
                        &groups[k],
                        delta,
                        &ref_cert_routes,
                    ) {
                        for &ci in &groups[k] {
                            let c = &mut board.components[ci];
                            c.placement = PlacementConstraint::Fixed {
                                x: c.x,
                                y: c.y,
                                theta: c.theta,
                            };
                        }
                        info!(
                            "channel transfer: column {k} = translated reference certificate ({} route(s) verified)",
                            routes.len()
                        );
                        stamped_channels.push(StampedChannel {
                            ref_members: groups[k].clone(),
                            ref_stamp_pos: groups[k]
                                .iter()
                                .map(|&ci| {
                                    (board.components[ci].x, board.components[ci].y)
                                })
                                .collect(),
                            groups: vec![groups[k].clone()],
                            expect: vec![(0.0, 0.0)],
                            frozen: vec![true],
                            routes,
                        });
                        col_cert[k] = true;
                        continue;
                    }
                }
                let mut certified = None;
                for (tier, margin) in [1.0f64, 0.25].iter().enumerate() {
                    let hw = (min_sep / 2.0 - margin).max(2.0);
                    let region = if x_major {
                        let x0 = (akx - hw).max(ec);
                        let x1 =
                            (akx + hw).min(board.config.outline.width() - ec);
                        (x0, ec, x1 - x0, cross_extent - 2.0 * ec)
                    } else {
                        let y0 = (aky - hw).max(ec);
                        let y1 =
                            (aky + hw).min(board.config.outline.height() - ec);
                        (ec, y0, cross_extent - 2.0 * ec, y1 - y0)
                    };
                    certified = solve_channel_miniboard(
                        &board,
                        &groups[k],
                        region,
                        &config,
                        seed ^ 0x5eed_c0de
                            ^ ((tier as u64) << 32)
                            ^ ((k as u64) << 40),
                    );
                    if certified.is_some() {
                        if tier > 0 {
                            info!(
                                "channel mini-solve: column {k} certified after region widening (margin {margin}mm)"
                            );
                        }
                        break;
                    }
                }
                if let Some(cert) = certified {
                    for &(ci, x, y, theta) in &cert.placements {
                        board.components[ci].x = x;
                        board.components[ci].y = y;
                        board.components[ci].theta = theta;
                    }
                    // Freeze the certificate in its own frame.
                    for &ci in &groups[k] {
                        let c = &mut board.components[ci];
                        c.placement = PlacementConstraint::Fixed {
                            x: c.x,
                            y: c.y,
                            theta: c.theta,
                        };
                    }
                    if k == 0 {
                        ref_cert_routes = cert.routes.clone();
                    }
                    stamped_channels.push(StampedChannel {
                        ref_members: groups[k].clone(),
                        ref_stamp_pos: groups[k]
                            .iter()
                            .map(|&ci| {
                                (board.components[ci].x, board.components[ci].y)
                            })
                            .collect(),
                        groups: vec![groups[k].clone()],
                        expect: vec![(0.0, 0.0)],
                        frozen: vec![true],
                        routes: cert.routes,
                    });
                    col_cert[k] = true;
                }
            }
            info!(
                "channel certificates: {}/{} column(s) certified+frozen",
                col_cert.iter().filter(|&&c| c).count(),
                col_cert.len()
            );
            let solved = col_cert[0];

            // MULTI-FILE (fallback): a 21-part single file (~90mm) overflows the
            // cross space left beside the pot rows (measured: tail
            // piled at the board edge, 86v/76unc). Fit as many files
            // as the column width allows and fill them in parallel.
            let max_w = order
                .iter()
                .map(|&ci| {
                    let (_, _, hw, hh) = board.components[ci].envelope();
                    2.0 * if x_major { hw } else { hh }
                })
                .fold(0.0f64, f64::max);
            let file_pitch = max_w + 1.0;
            let n_files = ((2.0 * half_w / file_pitch).floor() as usize).clamp(1, 3);
            let mut cursors = vec![board.config.edge_clearance_mm + 2.0; n_files];
            for &ci in order.iter().filter(|_| !solved) {
                let (_, _, hw, hh) = board.components[ci].envelope();
                let ch = if x_major { hh } else { hw };
                // Shortest file first (keeps files balanced).
                let f = (0..n_files)
                    .min_by(|&a, &b| cursors[a].partial_cmp(&cursors[b]).unwrap())
                    .unwrap();
                loop {
                    let (lo, hi) = (cursors[f], cursors[f] + 2.0 * ch);
                    if let Some(&(_, b1)) = blocked
                        .iter()
                        .find(|&&(b0, b1)| hi > b0 && lo < b1)
                    {
                        cursors[f] = b1;
                        continue;
                    }
                    break;
                }
                let center = (cursors[f] + ch).min(cross_extent - 2.0);
                let file_off =
                    (f as f64 - (n_files as f64 - 1.0) / 2.0) * file_pitch;
                if x_major {
                    board.components[ci].x = ax + file_off;
                    board.components[ci].y = center;
                } else {
                    board.components[ci].x = center;
                    board.components[ci].y = anchors[0].1 + file_off;
                }
                cursors[f] += 2.0 * ch + 1.0;
            }
            // UNCERTIFIED siblings fall back to the anchor-translated
            // copy of the reference layout (mobile — they stay on the
            // rigid machinery; certified columns are already frozen
            // in their own frames and must not be overwritten).
            for k in 1..groups.len() {
                if col_cert[k] {
                    continue;
                }
                let (tx, ty) = (anchors[k].0 - ax, anchors[k].1 - anchors[0].1);
                for (m, &rci) in groups[0].iter().enumerate() {
                    let sci = groups[k][m];
                    board.components[sci].x = board.components[rci].x + tx;
                    board.components[sci].y = board.components[rci].y + ty;
                    board.components[sci].theta = board.components[rci].theta;
                }
            }
        }
        let out: Vec<(usize, Vec<(usize, f64, f64)>)> = groups
            .into_iter()
            // A frozen (certified) family needs no rigid machinery —
            // every member is Fixed; rigid moves would fight the
            // certificate.
            .filter(|g| {
                g.len() >= 2
                    && !g.iter().all(|&ci| board.components[ci].placement.is_fixed())
            })
            .map(|g| {
                let leader = g[0];
                let (lx, ly) = (board.components[leader].x, board.components[leader].y);
                let members = g
                    .into_iter()
                    .map(|ci| {
                        (ci, board.components[ci].x - lx, board.components[ci].y - ly)
                    })
                    .collect();
                (leader, members)
            })
            .collect();
        info!(
            "rigid groups: {} sibling block(s), {} part(s) init-stamped",
            out.len(),
            stamped
        );
        out
    } else {
        Vec::new()
    };

    if !mirror_classes.is_empty() {
        // INIT-STAMP: replicate the reference sibling's INITIAL layout
        // before any optimization — at init there is no coherence to
        // break (the post-placement stamp teleport measured worse for
        // exactly that reason), and it makes the family-mean offset
        // meaningful from iteration 0. The mirror force then HOLDS
        // counterparts together while the optimizer negotiates one
        // channel layout that fits every column.
        let stamped = stamp_sibling_groups(&mut board);
        info!(
            "mirror constraint: {} counterpart class(es), {} part(s) init-stamped",
            mirror_classes.len(),
            stamped
        );
    }

    // 2. Set up optimizer state + progressive freezer
    let mut adam = AdamState::new(n);
    let mut freezer = placement::ProgressiveFreezer::new(n);
    let mut monitor = ConvergenceMonitor::new(
        config.convergence.window_size,
        config.convergence.wl_tolerance,
        config.convergence.max_rollbacks,
    );
    // Don't converge until routing has run at least once
    monitor.set_min_iterations(config.routing_schedule.first_route_iter + 50);

    let mut routes: Vec<Route> = board.nets.iter().map(|n| Route::empty(n.id)).collect();
    let mut grid: Option<RoutingGrid> = None;
    let mut gamma = 2.0; // Start moderately smooth, anneal down to 0.1
    let mut lambda_density = 0.0; // Auto-calibrated on first iteration

    // 3. Main iterative loop
    for iteration in 0..config.max_iterations {
        // Compute placement forces
        let (wl, wl_forces) = analytical::compute_wirelength(&board, gamma);

        let (density_forces, density_overflow) =
            placement::density::compute_density_forces(&board);
        let group_forces = grouping::compute_group_cohesion(&board);
        let power_forces = grouping::compute_power_domain_cohesion(&board);
        let thermal_forces = grouping::compute_thermal_spreading(&board, 0.1);
        let region_forces = grouping::compute_region_preference(&board);

        // Auto-calibrate density weight on first iteration (ePlace approach):
        // Set λ_D so that ||∇_WL|| ≈ ||λ_D · ∇_D||
        if iteration == 0 {
            let wl_norm: f64 = wl_forces.dx.iter().zip(wl_forces.dy.iter())
                .map(|(x, y)| x * x + y * y)
                .sum::<f64>()
                .sqrt();
            let d_norm: f64 = density_forces.dx.iter().zip(density_forces.dy.iter())
                .map(|(x, y)| x * x + y * y)
                .sum::<f64>()
                .sqrt();
            lambda_density = if d_norm > 1e-10 {
                config.placement.lambda_density * (wl_norm / d_norm)
            } else {
                config.placement.lambda_density
            };
            info!("Auto-calibrated λ_density = {:.4} (WL_norm={:.1}, D_norm={:.1})",
                lambda_density, wl_norm, d_norm);
        }

        // Gradually increase density weight (ePlace: lambda grows ~1.05× per iter)
        if iteration > 0 && iteration % 10 == 0 {
            lambda_density *= 1.02;
        }

        // Combine forces with phase-dependent weights
        let group_weight = if iteration >= config.routing_schedule.fine_start_iter {
            let decay = 1.0 - ((iteration - config.routing_schedule.fine_start_iter) as f64
                / (config.max_iterations - config.routing_schedule.fine_start_iter).max(1) as f64)
                .min(0.8);
            config.placement.lambda_group * decay
        } else {
            config.placement.lambda_group
        };

        let mut forces = wl_forces;
        forces.accumulate(&density_forces, lambda_density);
        forces.accumulate(&group_forces, group_weight);
        // Power-domain cohesion at a fraction of group weight: enough
        // to regionalize rails (split-plane separability), never enough
        // to beat signal wirelength.
        forces.accumulate(&power_forces, group_weight * 0.3);
        forces.accumulate(&thermal_forces, config.placement.lambda_thermal);
        forces.accumulate(&region_forces, config.placement.lambda_region);

        // Intent-derived constraint forces (proximity, loop area). Only
        // computed when the board actually carries constraints, so
        // un-annotated boards pay nothing. The proximity term ramps like
        // density so hard proximity tightens as placement settles
        // (Lagrangian-style); loop area uses a steady soft weight.
        if !board.constraints.is_empty() {
            let prox_forces = placement::intent_forces::compute_proximity_forces(&board);
            let loop_forces = placement::intent_forces::compute_loop_area_forces(&board);
            // Ramp proximity weight from 0.5× to ~2× over the run.
            let ramp = 0.5 + 1.5 * (iteration as f64 / config.max_iterations.max(1) as f64);
            forces.accumulate(&prox_forces, config.placement.lambda_proximity * ramp);
            forces.accumulate(&loop_forces, config.placement.lambda_loop_area);
        }

        // MIRROR FORCE: counterparts pull toward their family-mean
        // offset (ramped like proximity — tightens as placement
        // settles). Fixed members are anchors, never pulled.
        if !mirror_classes.is_empty() {
            let ramp = 0.5 + 1.5 * (iteration as f64 / config.max_iterations.max(1) as f64);
            // Strong: the mirror must OUTVOTE per-column wirelength
            // preferences or the solve tears the init-stamp apart
            // (k=2 measured a 109mm snap residual — pure noise
            // against density/HPWL).
            let k = 30.0 * ramp;
            for class in &mirror_classes {
                let n_c = class.len() as f64;
                let (mut mx, mut my) = (0.0, 0.0);
                for &(ci, (ax, ay)) in class {
                    mx += board.components[ci].x - ax;
                    my += board.components[ci].y - ay;
                }
                mx /= n_c;
                my /= n_c;
                for &(ci, (ax, ay)) in class {
                    forces.dx[ci] += k * ((ax + mx) - board.components[ci].x);
                    forces.dy[ci] += k * ((ay + my) - board.components[ci].y);
                }
            }
        }

        // Add routing feedback forces (ramp up after routing starts)
        if grid.is_some() {
            // λ_C and λ_V ramp: start small, grow linearly over iterations
            // This matches the proposal: routing feedback grows stronger as
            // placement refines and routing data becomes more meaningful.
            let routing_progress = if iteration > config.routing_schedule.first_route_iter {
                ((iteration - config.routing_schedule.first_route_iter) as f64
                    / (config.max_iterations - config.routing_schedule.first_route_iter).max(1) as f64)
                    .min(1.0)
            } else {
                0.0
            };
            let lambda_c = config.placement.lambda_congestion.max(0.1) * routing_progress;
            let lambda_v = config.placement.lambda_via.max(0.5) * routing_progress;

            // Via penalty: push connected components toward net centroid
            let via_grad = congestion::compute_via_penalty(&board, &routes, &board.nets);
            for (i, (vx, vy)) in via_grad.iter().enumerate() {
                forces.dx[i] += lambda_v * vx;
                forces.dy[i] += lambda_v * vy;
            }

            // Congestion inflation was already applied to density_inflation
            // in the routing step — the density force picks it up automatically.
            let _ = lambda_c; // used implicitly through density_inflation
        }

        // Log force magnitudes periodically
        if iteration % 50 == 0 {
            let wl_mag: f64 = forces.dx.iter().zip(forces.dy.iter())
                .map(|(x, y)| (x * x + y * y).sqrt())
                .sum::<f64>() / n as f64;
            let avg_theta: f64 = board.components.iter()
                .map(|c| c.theta.to_degrees().abs())
                .sum::<f64>() / n as f64;
            let theta_force: f64 = forces.d_theta.iter()
                .map(|t| t.abs())
                .sum::<f64>() / n as f64;
            info!(
                "Iter {}: WL={:.1}, density_ovf={:.3}, force={:.4}, avg_θ={:.1}°, θ_force={:.4}",
                iteration, wl, density_overflow, wl_mag, avg_theta, theta_force
            );
        }

        // RIGID: each sibling block feels ONE force — the mean over
        // its members (identical forces + identical Adam state =
        // identical trajectories; the snap below catches everything
        // else that touches positions).
        for (_, members) in &rigid_groups {
            let n_m = members.len() as f64;
            let fx = members.iter().map(|&(ci, ..)| forces.dx[ci]).sum::<f64>() / n_m;
            let fy = members.iter().map(|&(ci, ..)| forces.dy[ci]).sum::<f64>() / n_m;
            for &(ci, ..) in members {
                forces.dx[ci] = fx;
                forces.dy[ci] = fy;
            }
        }

        // Update positions (constraint-aware, skip frozen components)
        optimizer::adam_step(
            &mut board,
            &forces,
            &mut adam,
            &config.placement,
            &config.optimizer,
            Some(&freezer.frozen),
        );

        // Direct overlap resolution every iteration (mini-legalization)
        // This ensures overlapping components get pushed apart immediately,
        // not just through gradient descent which Adam normalizes away.
        for i in 0..n {
            for j in (i + 1)..n {
                // A component is immovable for overlap displacement if it is
                // either progressively frozen or constrained to a Fixed
                // position — shoving a Fixed component violates its invariant
                // (caught by the legalization debug_assert downstream).
                let immovable_i = freezer.is_frozen(i) || board.components[i].placement.is_fixed();
                let immovable_j = freezer.is_frozen(j) || board.components[j].placement.is_fixed();
                if immovable_i && immovable_j { continue; }
                let (cxi, cyi, hwi, hhi) = board.components[i].envelope();
                let (cxj, cyj, hwj, hhj) = board.components[j].envelope();
                let dx = cxj - cxi;
                let dy = cyj - cyi;
                // Min envelope separation = sum of half-extents plus
                // both components' courtyard excess (IPC keepout). At nominal
                // density (0.25/side) this is the prior hardcoded +0.5.
                let cy = 2.0 * board.config.courtyard_excess_mm;
                let min_dx = hwi + hwj + cy;
                let min_dy = hhi + hhj + cy;
                if dx.abs() < min_dx && dy.abs() < min_dy {
                    // Push proportional to overlap — larger overlaps get stronger push
                    let overlap = ((min_dx - dx.abs()) + (min_dy - dy.abs())) / 2.0;
                    let push = (overlap * 0.5).max(0.2); // at least 0.2mm, scales with overlap
                    let sign_x = if dx >= 0.0 { 1.0 } else { -1.0 };
                    let sign_y = if dy >= 0.0 { 1.0 } else { -1.0 };
                    if !immovable_i && !immovable_j {
                        if (min_dx - dx.abs()) < (min_dy - dy.abs()) {
                            board.components[i].x -= sign_x * push;
                            board.components[j].x += sign_x * push;
                        } else {
                            board.components[i].y -= sign_y * push;
                            board.components[j].y += sign_y * push;
                        }
                    } else if !immovable_j {
                        if (min_dx - dx.abs()) < (min_dy - dy.abs()) {
                            board.components[j].x += sign_x * push * 2.0;
                        } else {
                            board.components[j].y += sign_y * push * 2.0;
                        }
                    } else if !immovable_i {
                        if (min_dx - dx.abs()) < (min_dy - dy.abs()) {
                            board.components[i].x -= sign_x * push * 2.0;
                        } else {
                            board.components[i].y -= sign_y * push * 2.0;
                        }
                    }
                }
            }
        }

        // RIGID: re-freeze internal geometry after anything that
        // moved members individually (mini-legalization above).
        for (leader, members) in &rigid_groups {
            let (lx, ly) = (board.components[*leader].x, board.components[*leader].y);
            for &(ci, dx, dy) in members {
                board.components[ci].x = lx + dx;
                board.components[ci].y = ly + dy;
            }
        }

        // Progressive freezing: lock components that have stabilized
        let newly_frozen = freezer.update(&board, anchor_idx);
        if newly_frozen > 0 && iteration % 50 == 0 {
            info!("Iter {}: froze {} components ({} total frozen)",
                iteration, newly_frozen, freezer.frozen_count());
        }

        // Periodic routing feedback (tiered schedule from proposal §5.1)
        if config.routing_schedule.should_route(iteration) {
            let pf_iters = config.routing_schedule.pathfinder_iterations(iteration);

            let mut g = RoutingGrid::build(&board);
            routes = pathfinder::pathfinder_route(
                &mut g,
                &board.nets,
                &board,
                pf_iters,
                0.5,
                1.0,
                false, // no vias during placement feedback (single-layer)
            );

            // Apply congestion inflation
            congestion::apply_congestion_inflation(&mut board, &g, 0.3);

            grid = Some(g);
            monitor.notify_routing_done();

            if iteration % 100 == 0 {
                let total_vias: usize = routes.iter().map(|r| r.via_count()).sum();
                let overflow = grid.as_ref().map_or(0, |g| g.max_overflow());
                info!(
                    "Iter {}: WL={:.1}, density_overflow={:.3}, vias={}, routing_overflow={}",
                    iteration, wl, density_overflow, total_vias, overflow
                );
            }
        }

        // Convergence check
        let snap = placement::snapshot(&board);
        let total_vias: usize = routes.iter().map(|r| r.via_count()).sum();
        let overflow = grid.as_ref().map_or(0, |g| g.max_overflow());

        match monitor.check(wl, overflow, total_vias, &snap) {
            ConvergenceAction::Converged => {
                info!("Converged at iteration {}", iteration);
                break;
            }
            ConvergenceAction::Rollback => {
                if let Some(best) = monitor.best_state() {
                    info!("Divergence detected at iter {}, rolling back", iteration);
                    placement::restore(&mut board, best);
                }
            }
            ConvergenceAction::Continue => {}
        }

        // Anneal gamma: tighten LSE wirelength approximation over time
        // ePlace approach: gamma shrinks as density overflow decreases
        if iteration > 0 && iteration % 20 == 0 {
            gamma *= 0.9;
            gamma = gamma.max(0.1);
        }

        // After iter 400: loosen group cohesion to allow routing-driven spreading
        // (proposal §5.1: "Full forces, decreasing λ_G")
        if iteration == config.routing_schedule.fine_start_iter {
            info!("Iter {}: entering fine routing phase, loosening group cohesion", iteration);
        }
    }

    // 3.9 CHANNEL STAMPING — LANDED DORMANT (BHDL_PNR_STAMP=1). The
    // mixer demo solves one channel and stamps it x4 (its 4 worst
    // nets are IDENTICAL 2.5013x); ours re-solves each channel
    // (tre_t across ch1-4 measured 1.35/1.34/4.97/1.34). This
    // post-placement translate MEASURED WORSE (mixer total copper
    // 3244 -> 4730mm, family spread 1.23 -> 5.31 mean): teleporting
    // after the placer optimized globally breaks coherence, and the
    // reference channel's layout does not fit the edge column (parts
    // land off-board, the legalizer scatters). The real lever is a
    // MIRROR CONSTRAINT inside the optimizer — counterpart offsets
    // attracted across siblings during the solve, so ONE channel
    // layout emerges that fits every column. Kept for that arc.
    if std::env::var("BHDL_PNR_STAMP").is_ok() {
        let stamped = stamp_sibling_groups(&mut board);
        if stamped > 0 {
            info!("channel stamping: {stamped} part(s) aligned to their reference sibling");
        }
    }

    // 3.95 MIRROR SNAP: once converged, counterparts sit NEAR their
    // family-mean offset — snap them exactly onto it (sub-mm, coherent
    // displacements, nothing like the stamp teleport) so sibling
    // channels ship with identical internal geometry.
    if !mirror_classes.is_empty() {
        let mut snapped = 0usize;
        let mut max_disp = 0.0f64;
        for class in &mirror_classes {
            let n_c = class.len() as f64;
            let (mut mx, mut my) = (0.0, 0.0);
            for &(ci, (ax, ay)) in class {
                mx += board.components[ci].x - ax;
                my += board.components[ci].y - ay;
            }
            mx /= n_c;
            my /= n_c;
            for &(ci, (ax, ay)) in class {
                let (nx, ny) = (ax + mx, ay + my);
                let c = &mut board.components[ci];
                let d = (c.x - nx).hypot(c.y - ny);
                if d > 1e-9 {
                    max_disp = max_disp.max(d);
                    c.x = nx;
                    c.y = ny;
                    snapped += 1;
                }
            }
        }
        if snapped > 0 {
            info!("mirror snap: {snapped} counterpart(s) onto the family-mean offset");
        }
        let _ = &max_disp;
        info!("mirror snap: max displacement {max_disp:.2}mm");
    }

    // RIGID: final snap, then BLOCK-AWARE legalization — the
    // legalizer alternates with mean-displacement re-coherence so
    // block overlaps resolve as whole-block translations (Fixed
    // members were unresolvable: mixer 144v/41unc). Members are held
    // Fixed only through detailed refine (below).
    for (leader, members) in &rigid_groups {
        let (lx, ly) = (board.components[*leader].x, board.components[*leader].y);
        for &(ci, dx, dy) in members {
            board.components[ci].x = lx + dx;
            board.components[ci].y = ly + dy;
        }
    }

    // 4. Legalization
    info!("Legalizing placement...");
    if rigid_groups.is_empty() {
        legalization::legalize(&mut board, 0.1);
    } else {
        let blocks: Vec<Vec<(usize, f64, f64)>> =
            rigid_groups.iter().map(|(_, m)| m.clone()).collect();
        legalization::legalize_with_blocks(&mut board, 0.1, &blocks);
    }
    // Refine must not move block members individually.
    let mut rigid_restore: Vec<(usize, PlacementConstraint)> = Vec::new();
    for (_, members) in &rigid_groups {
        for &(ci, ..) in members {
            let c = &board.components[ci];
            rigid_restore.push((ci, c.placement.clone()));
            let (x, y, theta) = (c.x, c.y, c.theta);
            board.components[ci].placement = PlacementConstraint::Fixed { x, y, theta };
        }
    }

    // 4.5 Detailed placement: greedy swap/rotate HPWL refinement on the
    // legal placement. Every accepted move is legality-checked, so no
    // re-legalization is needed.
    let (wl0, wl1) = placement::detailed::refine(&mut board, 4);
    if wl1 < wl0 - 1e-9 {
        info!("Detailed placement: HPWL {:.1} -> {:.1}mm ({:.1}%)",
            wl0, wl1, (wl0 - wl1) / wl0 * 100.0);
    }
    for (ci, orig) in rigid_restore {
        board.components[ci].placement = orig;
    }

    // 4.7 Split-plane regions: several rails can share one Power layer;
    // each gets a BAND along the axis of larger pin spread, boundaries
    // at midpoints between rail centroids (computable only now — pin
    // positions needed placement). Bands shrink 0.25mm per inner side
    // so adjacent fills keep the 0.3mm zone clearance plus margin.
    assign_plane_regions(&mut board);

    // 4.99. Resolve PENDING pour regions: a declared non-ground pour
    // covers its consumers' PIN CLOUD — computable only now that
    // placement is final (semantic time had free parts at init
    // garbage). Bbox + 3mm, clamped inside the edge clearance.
    {
        let bw = board.config.outline.width();
        let bh = board.config.outline.height();
        let m = board.config.edge_clearance_mm + 0.05;
        let comp_pos: crate::det::HashMap<ComponentId, usize> = board
            .components
            .iter()
            .enumerate()
            .map(|(i, c)| (c.id, i))
            .collect();
        for i in 0..board.nets.len() {
            if !board.nets[i].pour_region_pending {
                continue;
            }
            let mut x0 = f64::INFINITY;
            let mut y0 = f64::INFINITY;
            let mut x1 = f64::NEG_INFINITY;
            let mut y1 = f64::NEG_INFINITY;
            for &(cid, pid) in &board.nets[i].pins {
                let Some(&ci) = comp_pos.get(&cid) else { continue };
                let comp = &board.components[ci];
                let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pid)
                else {
                    continue;
                };
                let (co, sn) = (comp.theta.cos(), comp.theta.sin());
                let gx = comp.x + pin.dx * co - pin.dy * sn;
                let gy = comp.y + pin.dx * sn + pin.dy * co;
                x0 = x0.min(gx);
                y0 = y0.min(gy);
                x1 = x1.max(gx);
                y1 = y1.max(gy);
            }
            board.nets[i].pour_region_pending = false;
            if !x0.is_finite() || x1 - x0 < 1.0 || y1 - y0 < 1.0 {
                log::warn!(
                    "pour region for '{}': degenerate pin cloud ({} pins, bbox {:.2}x{:.2} at ({:.1},{:.1})) — pour dropped",
                    board.nets[i].name,
                    board.nets[i].pins.len(),
                    x1 - x0,
                    y1 - y0,
                    x0,
                    y0
                );
                board.nets[i].plane_layer = None;
                continue;
            }
            let region = (
                (x0 - 3.0).max(m),
                (y0 - 3.0).max(m),
                (x1 + 3.0).min(bw - m),
                (y1 + 3.0).min(bh - m),
            );
            board.nets[i].plane_region = Some(region);
            // PLACEMENT-AWARE SHAPE: a scattered consumer cloud must
            // not blanket the board with pour priority (the free-
            // placement vbias bbox covered 80x70mm, its claim/apron
            // swallowed foreign SMD pockets, and a walled GND pad
            // shipped unconnected). Cluster the pins (single-linkage,
            // 6mm) and connect cluster boxes with L-corridors along
            // an MST — the DEMO's own band is exactly this shape when
            // the consumers sit in a row. One cluster = the bbox,
            // byte-for-byte today's behavior.
            let pts: Vec<(f64, f64)> = {
                let mut v = Vec::new();
                for &(cid, pid) in &board.nets[i].pins {
                    let Some(&ci) = comp_pos.get(&cid) else { continue };
                    let comp = &board.components[ci];
                    let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pid)
                    else {
                        continue;
                    };
                    let (co, sn) = (comp.theta.cos(), comp.theta.sin());
                    v.push((
                        comp.x + pin.dx * co - pin.dy * sn,
                        comp.y + pin.dx * sn + pin.dy * co,
                    ));
                }
                v
            };
            let mut cluster = (0..pts.len()).collect::<Vec<usize>>();
            fn find(c: &mut Vec<usize>, mut a: usize) -> usize {
                while c[a] != a {
                    c[a] = c[c[a]];
                    a = c[a];
                }
                a
            }
            for a in 0..pts.len() {
                for b in (a + 1)..pts.len() {
                    if (pts[a].0 - pts[b].0).hypot(pts[a].1 - pts[b].1) <= 6.0 {
                        let (ra, rb) = (find(&mut cluster, a), find(&mut cluster, b));
                        cluster[ra] = rb;
                    }
                }
            }
            let mut boxes: Vec<(f64, f64, f64, f64)> = Vec::new();
            let mut roots: Vec<usize> = Vec::new();
            for a in 0..pts.len() {
                let r = find(&mut cluster, a);
                let bi = match roots.iter().position(|&x| x == r) {
                    Some(k) => k,
                    None => {
                        roots.push(r);
                        boxes.push((
                            f64::INFINITY,
                            f64::INFINITY,
                            f64::NEG_INFINITY,
                            f64::NEG_INFINITY,
                        ));
                        boxes.len() - 1
                    }
                };
                boxes[bi].0 = boxes[bi].0.min(pts[a].0);
                boxes[bi].1 = boxes[bi].1.min(pts[a].1);
                boxes[bi].2 = boxes[bi].2.max(pts[a].0);
                boxes[bi].3 = boxes[bi].3.max(pts[a].1);
            }
            if boxes.len() > 1 {
                let clamp = |r: (f64, f64, f64, f64)| {
                    (
                        r.0.max(m),
                        r.1.max(m),
                        r.2.min(bw - m),
                        r.3.min(bh - m),
                    )
                };
                let mut rects: Vec<(f64, f64, f64, f64)> = boxes
                    .iter()
                    .map(|&(a, b, c, d)| clamp((a - 3.0, b - 3.0, c + 3.0, d + 3.0)))
                    .collect();
                // MST over cluster centers (Prim), L-corridors 6mm wide.
                let ctr: Vec<(f64, f64)> = boxes
                    .iter()
                    .map(|&(a, b, c, d)| ((a + c) / 2.0, (b + d) / 2.0))
                    .collect();
                let mut joined = vec![false; ctr.len()];
                joined[0] = true;
                for _ in 1..ctr.len() {
                    let mut best: Option<(usize, usize, f64)> = None;
                    for a in 0..ctr.len() {
                        if !joined[a] {
                            continue;
                        }
                        for b in 0..ctr.len() {
                            if joined[b] {
                                continue;
                            }
                            let d = (ctr[a].0 - ctr[b].0).hypot(ctr[a].1 - ctr[b].1);
                            if best.map_or(true, |(.., bd)| d < bd) {
                                best = Some((a, b, d));
                            }
                        }
                    }
                    let Some((a, b, _)) = best else { break };
                    joined[b] = true;
                    let hw = 3.0;
                    let (ax, ay) = ctr[a];
                    let (bx, by) = ctr[b];
                    rects.push(clamp((
                        ax.min(bx) - hw,
                        ay - hw,
                        ax.max(bx) + hw,
                        ay + hw,
                    )));
                    rects.push(clamp((
                        bx - hw,
                        ay.min(by) - hw,
                        bx + hw,
                        ay.max(by) + hw,
                    )));
                }
                // AREA GATE: the shape earns its keep only when the
                // bbox is mostly empty space (a scattered cloud whose
                // blanket would swallow foreign pockets). A dense row
                // — the demo's own band — keeps the blanket: its
                // pocket machinery (drops, rescue) was built for it.
                let (bx0, by0, bx1, by1) = region;
                let (gw, gh) = (
                    (((bx1 - bx0) / 1.0).ceil() as usize).max(1),
                    (((by1 - by0) / 1.0).ceil() as usize).max(1),
                );
                let mut covered = 0usize;
                for gy in 0..gh {
                    for gx in 0..gw {
                        let x = bx0 + (gx as f64 + 0.5) * (bx1 - bx0) / gw as f64;
                        let y = by0 + (gy as f64 + 0.5) * (by1 - by0) / gh as f64;
                        if rects
                            .iter()
                            .any(|&(x0, y0, x1, y1)| {
                                x >= x0 && x <= x1 && y >= y0 && y <= y1
                            })
                        {
                            covered += 1;
                        }
                    }
                }
                let ratio = covered as f64 / (gw * gh) as f64;
                if ratio >= 0.5 {
                    info!(
                        "pour region: '{}' pin cloud -> ({:.1},{:.1})-({:.1},{:.1}) ({} cluster(s), union {:.0}% of bbox — blanket kept)",
                        board.nets[i].name, region.0, region.1, region.2, region.3,
                        boxes.len(), ratio * 100.0
                    );
                    continue;
                }
                info!(
                    "pour region: '{}' {} pin cluster(s) -> {} rect(s) (union {:.0}% of bbox), bbox ({:.1},{:.1})-({:.1},{:.1})",
                    board.nets[i].name,
                    boxes.len(),
                    rects.len(),
                    ratio * 100.0,
                    region.0,
                    region.1,
                    region.2,
                    region.3
                );
                if std::env::var("BHDL_PNR_PROBE").is_ok() {
                    for r in &rects {
                        info!(
                            "[probe] region rect ({:.2},{:.2})-({:.2},{:.2})",
                            r.0, r.1, r.2, r.3
                        );
                    }
                }
                board.nets[i].plane_region_rects = rects;
            } else {
                info!(
                    "pour region: '{}' pin cloud -> ({:.1},{:.1})-({:.1},{:.1})",
                    board.nets[i].name, region.0, region.1, region.2, region.3
                );
            }
        }
    }

    // 5. Final routing — two-pass strategy (route like a human)
    //    Pass 1: single-layer routing (no vias) — maximize what can be routed flat
    //    Pass 2: remaining unrouted nets get vias to escape to other layers
    info!("Final routing pass 1 (single-layer, no vias)...");
    // Plane-assigned nets don't route as trees: their copper is the
    // emitted zone FILL; surface pads get via drops after routing.
    let mut routing_nets: Vec<PnrNet> = board
        .nets
        .iter()
        .map(|n| {
            if n.plane_layer.is_some() {
                // A pour is an OPTIMIZATION, not a connectivity
                // guarantee: a drilled pad tucked inside its own
                // package's pin ring (the hard-net-first pocket
                // predicate) has structurally poor pour reach — the
                // fill must thread the ring gaps and every signal
                // escape fences it further (measured: the ecc83
                // valve's H9/GND pad stranded on every seed; the
                // hand-routed demo serves that pad with a TRACK
                // claimed early). Signal-layer pours therefore keep
                // their POCKET pads as ROUTED sinks — paired with the
                // nearest same-net pad as the tree's far anchor — and
                // the pour serves everything else; same-net contact
                // merges track and fill. Decided from geometry alone.
                let on_signal = n
                    .plane_layer
                    .and_then(|pl| board.layer_stack.layers.get(pl))
                    .map(|l| l.kind == crate::types::LayerKind::Signal)
                    .unwrap_or(false);
                let mut keep: Vec<(ComponentId, PinId)> = Vec::new();
                if on_signal {
                    let pos_of = |cid: ComponentId, pid: PinId| -> Option<(f64, f64)> {
                        let comp = board.components.iter().find(|c| c.id == cid)?;
                        let pin = comp.pins.iter().find(|p| p.pin_id == pid)?;
                        let (co, sn) = (comp.theta.cos(), comp.theta.sin());
                        Some((
                            comp.x + pin.dx * co - pin.dy * sn,
                            comp.y + pin.dx * sn + pin.dy * co,
                        ))
                    };
                    // Pocketed = pad on a >=5-pin package that the
                    // fill must thread the package's own pin punches
                    // to reach: a DRILLED pad always (barrels punch
                    // every layer), and an SMD pad when the pour is
                    // ON its own face — same-side SMD neighbors punch
                    // the fill too, and a mid-row GND pad sits inside
                    // their chain-merged void (mixer anti-bias: every
                    // opamp V_MINUS stranded exactly so).
                    let last_cu = board.layer_stack.layers.len() - 1;
                    let pockets: Vec<(ComponentId, PinId)> = n
                        .pins
                        .iter()
                        .copied()
                        .filter(|&(cid, pid)| {
                            board.components.iter().any(|c| {
                                let surf = match c.side {
                                    BoardSide::Top => 0,
                                    BoardSide::Bottom => last_cu,
                                };
                                c.id == cid
                                    && c.pins.len() >= 5
                                    && c.pins.iter().any(|p| {
                                        p.pin_id == pid
                                            && p.pad.as_ref().map_or(false, |pd| {
                                                pd.drill_mm.is_some()
                                                    || Some(surf) == n.plane_layer
                                            })
                                    })
                            })
                        })
                        .collect();
                    for &(cid, pid) in &pockets {
                        keep.push((cid, pid));
                        // Nearest same-net pad on ANOTHER component =
                        // the routed tree's far anchor (pour-reachable
                        // open copper).
                        if let Some(pp) = pos_of(cid, pid) {
                            if let Some(&far) = n
                                .pins
                                .iter()
                                .filter(|&&(c2, _)| c2 != cid)
                                .min_by(|&&(c2, p2), &&(c3, p3)| {
                                    let d2 = pos_of(c2, p2)
                                        .map(|q| (q.0 - pp.0).hypot(q.1 - pp.1))
                                        .unwrap_or(f64::MAX);
                                    let d3 = pos_of(c3, p3)
                                        .map(|q| (q.0 - pp.0).hypot(q.1 - pp.1))
                                        .unwrap_or(f64::MAX);
                                    d2.partial_cmp(&d3)
                                        .unwrap_or(std::cmp::Ordering::Equal)
                                })
                            {
                                if !keep.contains(&far) {
                                    keep.push(far);
                                }
                            }
                        }
                    }
                }
                if keep.len() >= 2 {
                    info!(
                        "pour/track split: '{}' keeps {} pocket sink(s) as ROUTED (pour serves the rest)",
                        n.name,
                        keep.len()
                    );
                }
                if keep.len() >= 2 {
                    // plane_layer cleared on the ROUTING clone only —
                    // pathfinder would skip a plane net; the pour
                    // machinery reads board.nets, which is untouched.
                    PnrNet { pins: keep, plane_layer: None, ..n.clone() }
                } else {
                    PnrNet { pins: Vec::new(), ..n.clone() }
                }
            } else {
                n.clone()
            }
        })
        .collect();
    let mut final_grid = RoutingGrid::build(&board);

    // FANOUT-FIRST for IC plane pads: claim each SMD plane-pad's
    // stub+via while the board is EMPTY. Completion-time drops
    // compete with every signal escape for corridor space — the uno
    // s99 endgame: UGND mid-row in the QFP with its only outlet
    // crossed by the VCC rail, every siting rung dead (ring, routed
    // dijkstra, victim rip — whole-rail rebuilds strand 5-23 sinks).
    // Real fanout runs BEFORE routing for exactly this reason.
    // Sites are exact-kernel checked (pads are the only copper at
    // this stage); the drops are blocked in the pass-1 grid so
    // signals route around them, and the completion pass's
    // live-drop guard adopts them instead of double-dropping.
    let mut fanout_drops: Vec<(usize, Route)> = Vec::new();
    if board.config.fanout_first || std::env::var("BHDL_PNR_FANOUT_FIRST").is_ok() {
        let empty: Vec<Route> =
            board.nets.iter().map(|nn| Route::empty(nn.id)).collect();
        let nl = board.layer_stack.layers.len();
        let via_r = board.layer_stack.via.pad_mm / 2.0;
        let bw = board.config.outline.width();
        let bh = board.config.outline.height();
        let edge = board.config.edge_clearance_mm + via_r;
        let punch_gap = 2.0 * (via_r + 0.35) + 0.15;
        let mut new_vias: Vec<(f64, f64)> = Vec::new();
        let comp_idx: crate::det::HashMap<ComponentId, usize> = board
            .components
            .iter()
            .enumerate()
            .map(|(k, c)| (c.id, k))
            .collect();
        for i in 0..board.nets.len() {
            let net = &board.nets[i];
            if net.plane_layer.is_none() {
                continue;
            }
            let share = stackup::trace_width_for_current(
                stackup::current_for_trace_width(net.required_trace_width_mm)
                    / net.pins.len().max(1) as f64,
                1.0,
                10.0,
            )
            .max(0.3)
            .min(net.required_trace_width_mm);
            let merged = output::kicad::merge_holes(output::kicad::plane_foreign_holes(
                &board, &empty, net.id,
            ));
            let cidx = geom::ClearanceIndex::build(&board, &empty, Some(net.id));
            let region = net.plane_region;
            let mut drop_route = Route::empty(net.id);
            for &(comp_id, pin_id) in &net.pins {
                let Some(&ci) = comp_idx.get(&comp_id) else { continue };
                let comp = &board.components[ci];
                // ICs only: their pin rows are what gets walled in;
                // passives' pads stay reachable for the completion
                // pass, and fewer pre-commitments = less perturbation.
                if comp.pins.len() < 8 {
                    continue;
                }
                let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pin_id) else {
                    continue;
                };
                if pin.unplaced {
                    continue;
                }
                let cos_t = comp.theta.cos();
                let sin_t = comp.theta.sin();
                let px = comp.x + pin.dx * cos_t - pin.dy * sin_t;
                let py = comp.y + pin.dx * sin_t + pin.dy * cos_t;
                if pin.pad.as_ref().and_then(|p| p.drill_mm).is_some() {
                    // THT barrels pierce the plane in-region (same
                    // rule as the completion pass).
                    let in_region = match region {
                        None => true,
                        Some((rx0, ry0, rx1, ry1)) => {
                            px > rx0 + 0.05
                                && px < rx1 - 0.05
                                && py > ry0 + 0.05
                                && py < ry1 - 0.05
                        }
                    };
                    if in_region {
                        continue;
                    }
                }
                let stub_layer = match comp.side {
                    BoardSide::Top => 0,
                    BoardSide::Bottom => nl - 1,
                };
                // POUR-SIDE pads live ON the fill's layer: they are
                // fill anchors (contact merges them; island stitch
                // bridges stranding) — a stub+via here leaves the
                // via's far end on a bare signal layer, which ships
                // as via_dangling (dbl_sided 2v).
                if Some(stub_layer) == net.plane_layer {
                    continue;
                }
                // AISLE DISCIPLINE: a pre-drop consumes a ~1.4mm
                // punch swath — parked in a NEIGHBOR's escape lane it
                // strands that pad instead (s99: the first greedy
                // siting cured UGND and displaced the knot onto D_N
                // two pads over). Prefer the pad's OWN lane: angles
                // sorted by closeness to its outward normal.
                let onorm = {
                    let (ox, oy) = (px - comp.x, py - comp.y);
                    let l = ox.hypot(oy).max(1e-6);
                    (ox / l, oy / l)
                };
                let mut angles: Vec<f64> = (0..8)
                    .map(|k| k as f64 * std::f64::consts::FRAC_PI_4)
                    .collect();
                angles.sort_by(|a, b| {
                    let da = -(a.cos() * onorm.0 + a.sin() * onorm.1);
                    let db = -(b.cos() * onorm.0 + b.sin() * onorm.1);
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                });
                let mut placed: Option<(f64, f64)> = None;
                'rings: for ring in 0..10 {
                    let r = 0.6 + ring as f64 * 0.35;
                    for &ang in &angles {
                        let (x, y) = (px + r * ang.cos(), py + r * ang.sin());
                        if x < edge || y < edge || x > bw - edge || y > bh - edge {
                            continue;
                        }
                        if let Some((rx0, ry0, rx1, ry1)) = region {
                            if x - via_r < rx0 + 0.05
                                || y - via_r < ry0 + 0.05
                                || x + via_r > rx1 - 0.05
                                || y + via_r > ry1 - 0.05
                            {
                                continue;
                            }
                        }
                        // Foreign plane fills: the barrel must be
                        // cleanly punchable (same rule as completion).
                        let punch = via_r + 0.35;
                        let mut straddles = false;
                        for other in board.nets.iter().filter(|n| n.id != net.id) {
                            if other.plane_layer.is_none() {
                                continue;
                            }
                            let (zx0, zy0, zx1, zy1) = match other.plane_region {
                                Some((x0, y0, x1, y1)) => (
                                    x0.max(edge),
                                    y0.max(edge),
                                    x1.min(bw - edge),
                                    y1.min(bh - edge),
                                ),
                                None => (edge, edge, bw - edge, bh - edge),
                            };
                            let intersects = x > zx0 - punch
                                && x < zx1 + punch
                                && y > zy0 - punch
                                && y < zy1 + punch;
                            let interior = x > zx0 + punch
                                && x < zx1 - punch
                                && y > zy0 + punch
                                && y < zy1 - punch;
                            if intersects && !interior {
                                straddles = true;
                                break;
                            }
                        }
                        if straddles {
                            continue;
                        }
                        if output::kicad::plane_swallows(
                            &board, &merged, x, y, via_r, region,
                        ) {
                            continue;
                        }
                        if new_vias
                            .iter()
                            .any(|&(vx, vy)| (x - vx).hypot(y - vy) < punch_gap)
                        {
                            continue;
                        }
                        if cidx.via_conflict(x, y, via_r, net.id).is_some()
                            || cidx
                                .first_conflict((px, py), (x, y), share, stub_layer, net.id)
                                .is_some()
                        {
                            continue;
                        }
                        placed = Some((x, y));
                        break 'rings;
                    }
                }
                if let Some((vx, vy)) = placed {
                    let seg_start = drop_route.segments.len();
                    let via_start = drop_route.vias.len();
                    drop_route.segments.push(RouteSegment {
                        layer: stub_layer,
                        start: (px, py),
                        end: (vx, vy),
                        width_mm: share,
                    });
                    drop_route.path_spans.push((seg_start, 1));
                    drop_route.path_parents.push(None);
                    drop_route.vias.push(RouteVia {
                        x: vx,
                        y: vy,
                        from_layer: 0,
                        to_layer: nl - 1,
                    });
                    drop_route.via_spans.push((via_start, 1));
                    new_vias.push((vx, vy));
                }
            }
            if !drop_route.segments.is_empty() {
                fanout_drops.push((i, drop_route));
            }
        }
        let total: usize = fanout_drops.iter().map(|(_, r)| r.vias.len()).sum();
        if total > 0 {
            info!("fanout-first: {total} plane drop(s) pre-sited for IC pads");
        }
    }
    for (_, r) in &fanout_drops {
        pathfinder::block_route_geometry(&mut final_grid, r, &board);
    }

    // ROUTE STAMPING: certified channel copper re-enters the top-level
    // solve as pre-routes. Per sibling group: verify the rigid block
    // survived placement as a PURE TRANSLATION of the stamp frame
    // (deformed → skip, honest fallback), map reference nets to
    // sibling nets through member-parallel pin order, translate the
    // copper, and REVALIDATE pad-exact against the parent board —
    // a net stamps only when EVERY one of its parent pins is touched
    // by the translated copper (leaving nets fail this and route
    // normally; so does any net whose fixed environment doesn't
    // translate with the block, e.g. off-pitch jacks).
    let mut stamped_pre: Vec<(usize, Route)> = Vec::new();
    if !stamped_channels.is_empty() {
        let net_pos: crate::det::HashMap<NetId, usize> = board
            .nets
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id, i))
            .collect();
        let comp_pos: crate::det::HashMap<ComponentId, usize> = board
            .components
            .iter()
            .enumerate()
            .map(|(i, c)| (c.id, i))
            .collect();
        let mut taken: crate::det::HashSet<usize> = Default::default();
        let (mut n_deform, mut n_unmap, mut n_miss) = (0usize, 0usize, 0usize);
        let mut n_partial = 0usize;
        for ch in &stamped_channels {
            let hits = ch
                .routes
                .iter()
                .filter(|(nid, _)| net_pos.contains_key(nid))
                .count();
            debug!(
                "  route-stamp: channel with {} route(s), {} resolve to parent nets, {} group(s)",
                ch.routes.len(),
                hits,
                ch.groups.len()
            );
            for (k, grp) in ch.groups.iter().enumerate() {
                if !ch.frozen[k] {
                    continue; // stayed on the rigid machinery, no certificate
                }
                let deltas: Vec<(f64, f64)> = grp
                    .iter()
                    .zip(&ch.ref_stamp_pos)
                    .map(|(&ci, &(sx, sy))| {
                        (board.components[ci].x - sx, board.components[ci].y - sy)
                    })
                    .collect();
                if deltas.is_empty() {
                    continue;
                }
                let n = deltas.len() as f64;
                let (dx, dy) = (
                    deltas.iter().map(|d| d.0).sum::<f64>() / n,
                    deltas.iter().map(|d| d.1).sum::<f64>() / n,
                );
                let dev = deltas
                    .iter()
                    .map(|d| (d.0 - dx).abs().max((d.1 - dy).abs()))
                    .fold(0.0f64, f64::max);
                if dev > 0.1 {
                    n_deform += 1;
                    debug!(
                        "  route-stamp: group {k} deformed (max member deviation {dev:.2}mm) — skipped"
                    );
                    continue;
                }
                // Copper is only certified in its own column FRAME —
                // the reference frame for k=0, the anchor translation
                // for siblings. A block that drifted off its frame
                // keeps pure-member nets pad-consistent but lands the
                // copper in UNCERTIFIED territory (measured: 17
                // shorts from 17mm-drifted stamps).
                let (ex, ey) = ch.expect[k];
                if (dx - ex).abs() > 0.25 || (dy - ey).abs() > 0.25 {
                    n_deform += 1;
                    debug!(
                        "  route-stamp: group {k} at delta ({dx:.2},{dy:.2}) vs licensed frame ({ex:.2},{ey:.2}) — skipped"
                    );
                    continue;
                }
                for (nid, route) in &ch.routes {
                    let Some(&ri) = net_pos.get(nid) else { continue };
                    // Map the reference net to this sibling's net via
                    // member-parallel pin order (k=0 maps to itself).
                    let target = if k == 0 {
                        Some(ri)
                    } else {
                        let mut mapped: Option<usize> = None;
                        let mut ok = true;
                        for &(cid, pid) in &board.nets[ri].pins {
                            let Some(m) = ch
                                .ref_members
                                .iter()
                                .position(|&mi| board.components[mi].id == cid)
                            else {
                                continue; // pin on a fixed/foreign part
                            };
                            let refc = &board.components[ch.ref_members[m]];
                            let Some(j) =
                                refc.pins.iter().position(|p| p.pin_id == pid)
                            else {
                                ok = false;
                                break;
                            };
                            let sib = &board.components[grp[m]];
                            let Some(snid) = sib.pins.get(j).and_then(|p| p.net)
                            else {
                                ok = false;
                                break;
                            };
                            let Some(&si) = net_pos.get(&snid) else {
                                ok = false;
                                break;
                            };
                            if mapped.map_or(false, |mm| mm != si) {
                                ok = false;
                                break;
                            }
                            mapped = Some(si);
                        }
                        if ok { mapped } else { None }
                    };
                    let Some(ti) = target else {
                        n_unmap += 1;
                        continue;
                    };
                    let tnet = &board.nets[ti];
                    if tnet.pins.len() < 2
                        || tnet.plane_layer.is_some()
                        || tnet.is_plane_connected(&board.layer_stack)
                    {
                        continue;
                    }
                    // A net already stamped by another channel MERGES
                    // its copper in (rails cross every channel — each
                    // column's certified GND/VCC tree becomes an
                    // ISLAND of one Route; completion treats non-main
                    // components' pads as unreached and stitches the
                    // islands through them).
                    let merge_into: Option<usize> = if taken.contains(&ti) {
                        stamped_pre.iter().position(|(i, _)| *i == ti)
                    } else {
                        None
                    };
                    let mut t = route.clone();
                    t.net_id = tnet.id;
                    for sg in t.segments.iter_mut() {
                        sg.start.0 += dx;
                        sg.start.1 += dy;
                        sg.end.0 += dx;
                        sg.end.1 += dy;
                    }
                    for v in t.vias.iter_mut() {
                        v.x += dx;
                        v.y += dy;
                    }
                    // Pad-exact revalidation on the PARENT board.
                    let touched = tnet
                        .pins
                        .iter()
                        .filter(|&&(cid, pid)| {
                            let Some(&ci) = comp_pos.get(&cid) else {
                                return false;
                            };
                            let comp = &board.components[ci];
                            let Some(pin) =
                                comp.pins.iter().find(|p| p.pin_id == pid)
                            else {
                                return false;
                            };
                            let (c, sn) = (comp.theta.cos(), comp.theta.sin());
                            let px = comp.x + pin.dx * c - pin.dy * sn;
                            let py = comp.y + pin.dx * sn + pin.dy * c;
                            let half = pin
                                .pad
                                .as_ref()
                                .map(|p| p.width_mm.min(p.height_mm) / 2.0)
                                .unwrap_or(0.4);
                            t.segments.iter().any(|sg| {
                                geom::point_segment_dist((px, py), sg.start, sg.end)
                                    < sg.width_mm / 2.0 + half - 0.001
                            })
                        })
                        .count();
                    // Full stamp, partial stamp (leaving net keeps its
                    // certified in-column copper; completion bridges
                    // from the exit stub instead of the top-level
                    // router re-deriving the whole net around the
                    // frozen blocks — the F.Cu sprawl), or miss.
                    if touched < 2 {
                        n_miss += 1;
                        debug!(
                            "  route-stamp miss: '{}' ({touched}/{} pins, group {k}, delta {dx:.2},{dy:.2})",
                            tnet.name,
                            tnet.pins.len()
                        );
                        continue;
                    }
                    if touched < tnet.pins.len() {
                        n_partial += 1;
                    }
                    if let Some(mi) = merge_into {
                        let dst = &mut stamped_pre[mi].1;
                        let seg_off = dst.segments.len();
                        let via_off = dst.vias.len();
                        let span_off = dst.path_spans.len();
                        dst.segments.extend(t.segments);
                        dst.vias.extend(t.vias);
                        dst.path_spans
                            .extend(t.path_spans.iter().map(|&(s, l)| (s + seg_off, l)));
                        dst.path_parents
                            .extend(t.path_parents.iter().map(|p| p.map(|x| x + span_off)));
                        dst.via_spans
                            .extend(t.via_spans.iter().map(|&(s, l)| (s + via_off, l)));
                    } else {
                        taken.insert(ti);
                        stamped_pre.push((ti, t));
                    }
                }
            }
        }
        if !stamped_pre.is_empty() || n_deform + n_unmap + n_miss > 0 {
            info!(
                "route stamping: {} net(s) pre-routed from certified channel copper ({} partial, {} group(s) deformed, {} unmapped, {} pad-miss)",
                stamped_pre.len(),
                n_partial,
                n_deform,
                n_unmap,
                n_miss
            );
        }
        for (i, r) in &stamped_pre {
            pathfinder::block_route_geometry(&mut final_grid, r, &board);
            routing_nets[*i].pins = Vec::new();
        }
    }

    let mut final_routes = pathfinder::pathfinder_route(
        &mut final_grid,
        &routing_nets,
        &board,
        100,
        1.0,
        1.0,
        false, // no vias
    );
    for (i, r) in fanout_drops {
        final_routes[i] = r;
    }
    let stamped_idx: Vec<usize> = stamped_pre.iter().map(|(i, _)| *i).collect();
    for (i, r) in stamped_pre {
        final_routes[i] = r;
    }

    if std::env::var("BHDL_PNR_DEBUG_CLEARANCE").is_ok() {
        debug_check_foreign_pads(&board, &final_routes, "after-pass1");
    }
    if std::env::var("BHDL_PNR_DEBUG_STAGES").is_ok() {
        for (i, n) in board.nets.iter().enumerate() {
            if n.pins.len() < 2 || n.plane_layer.is_some() {
                continue;
            }
            let r = &final_routes[i];
            let unr = pathfinder::unreached_sink_count(n, &board, r);
            debug!(
                "  [pass1] '{}': {} seg(s), {} unreached",
                n.name,
                r.segments.len(),
                unr
            );
        }
    }
    let routed_pass1 = final_routes.iter().filter(|r| !r.is_empty()).count();
    let needs_via: Vec<usize> = final_routes.iter().enumerate()
        .filter(|(i, r)| {
            r.is_empty()
                && board.nets.get(*i).map_or(false, |n| n.pins.len() >= 2)
                && !board.nets.get(*i).map_or(false, |n| n.is_plane_connected(&board.layer_stack))
        })
        .map(|(i, _)| i)
        .collect();

    if !needs_via.is_empty() {
        info!("Final routing pass 2 (with vias for {} remaining nets)...", needs_via.len());

        // Build a fresh grid; reduce capacity where pass 1 routes exist
        let mut via_grid = RoutingGrid::build(&board);
        for route in &final_routes {
            if !route.is_empty() {
                // Full geometric footprint of the pass-1 route — path
                // cells AND diagonal companions. Reducing only the path
                // cells let a pass-2 net route the OPPOSITE diagonal
                // through a pass-1 diagonal's square: a physical X
                // between the two passes that neither pass could see.
                // BLOCK, don't zero-capacity: capacity-0 cells are
                // exempt from present-cost and overflow accounting (the
                // pin-terminal rule), so pass-2 routed straight through
                // pass-1 copper at zero cost, invisibly — the recurring
                // "VCC diagonal through auto_* vertical" crossing. A
                // blocked cell is truly unroutable (own terminals still
                // enter via the sink exception).
                for cell in pathfinder::route_cells(&via_grid, route) {
                    let c = via_grid.get_mut(cell);
                    c.blocked = true;
                    c.hard = true;
                }
            }
        }

        // Route ONLY the unrouted nets with a separate PathFinder run.
        // Power nets get higher weight so they route first (they benefit most
        // from B.Cu — wider traces, many pins spread across the board).
        let filtered_nets: Vec<PnrNet> = board.nets.iter().enumerate()
            .map(|(i, net)| {
                if needs_via.contains(&i) {
                    let mut n = net.clone();
                    // Boost power net priority in pass 2
                    if matches!(n.net_class, PnrNetClass::Power { .. }) {
                        n.weight = 10.0; // route power first
                    }
                    n
                } else {
                    PnrNet {
                        pins: Vec::new(),
                        ..net.clone()
                    }
                }
            })
            .collect();

        let via_routes = pathfinder::pathfinder_route(
            &mut via_grid,
            &filtered_nets,
            &board,
            100,
            1.0,
            1.0,
            true, // vias allowed
        );

        let mut pass2_routed = 0;
        for &idx in &needs_via {
            if !via_routes[idx].is_empty() {
                final_routes[idx] = via_routes[idx].clone();
                pass2_routed += 1;
            }
        }
        info!("Pass 2 routed {} additional nets with vias", pass2_routed);
        final_grid = via_grid;
    }

    info!("Routing complete: {} pass1, {} pass2",
        routed_pass1, final_routes.iter().filter(|r| !r.is_empty()).count() - routed_pass1);

    // 5.4. COMPLETION: pass 1 is via-less and pass 2 only takes nets
    // with NO copper — a net that reached some sinks on the pin layer
    // but has a sink unreachable without vias (a back-side pad) stays
    // incomplete forever, and recovery only touches RIPPED nets.
    // Extend every incomplete non-plane net with vias allowed.
    completion_pass(&board, &mut final_routes);

    // 5.45. Prune UNUSED exit stubs on stamped nets: a certified
    // boundary stub completion didn't adopt ends in bare copper (the
    // oracle's track_dangling). Iteratively drop segments with a FREE
    // end — an endpoint touching no same-net pad, no via, and no
    // other segment of the route. Pruning invalidates span indices,
    // so a pruned route's span structure is cleared (validator
    // semantics: empty spans = rip-whole on damage).
    {
        let comp_pos: crate::det::HashMap<ComponentId, usize> = board
            .components
            .iter()
            .enumerate()
            .map(|(i, c)| (c.id, i))
            .collect();
        let mut pruned_total = 0usize;
        for &i in &stamped_idx {
            let net = &board.nets[i];
            let pads: Vec<(f64, f64, f64)> = net
                .pins
                .iter()
                .filter_map(|&(cid, pid)| {
                    let &ci = comp_pos.get(&cid)?;
                    let comp = &board.components[ci];
                    let pin = comp.pins.iter().find(|p| p.pin_id == pid)?;
                    let (c, sn) = (comp.theta.cos(), comp.theta.sin());
                    let half = pin
                        .pad
                        .as_ref()
                        .map(|p| p.width_mm.min(p.height_mm) / 2.0)
                        .unwrap_or(0.4);
                    Some((
                        comp.x + pin.dx * c - pin.dy * sn,
                        comp.y + pin.dx * sn + pin.dy * c,
                        half,
                    ))
                })
                .collect();
            let r = &mut final_routes[i];
            let mut pruned_here = 0usize;
            loop {
                let mut drop: Option<usize> = None;
                'seg: for (si, sg) in r.segments.iter().enumerate() {
                    for &pt in &[sg.start, sg.end] {
                        let on_pad = pads.iter().any(|&(px, py, half)| {
                            (px - pt.0).hypot(py - pt.1) < half + sg.width_mm / 2.0
                        });
                        if on_pad {
                            continue;
                        }
                        let on_via = r
                            .vias
                            .iter()
                            .any(|v| (v.x - pt.0).hypot(v.y - pt.1) < 0.4);
                        if on_via {
                            continue;
                        }
                        let attached = r.segments.iter().enumerate().any(|(sj, s2)| {
                            sj != si
                                && geom::point_segment_dist(pt, s2.start, s2.end)
                                    < s2.width_mm / 2.0 + 0.01
                        });
                        if !attached {
                            drop = Some(si);
                            break 'seg;
                        }
                    }
                }
                match drop {
                    Some(si) => {
                        r.segments.remove(si);
                        pruned_here += 1;
                    }
                    None => break,
                }
            }
            if pruned_here > 0 {
                r.path_spans.clear();
                r.path_parents.clear();
                r.via_spans.clear();
                pruned_total += pruned_here;
            }
        }
        if pruned_total > 0 {
            info!("route stamping: {pruned_total} unused exit-stub segment(s) pruned");
        }
    }

    // 5.5. Via drops for plane-assigned nets: through-hole barrels
    // pierce their plane directly; each SURFACE pad gets a short stub
    // to a legally-sited via. A pad with no legal site within reach
    // stays honestly unconnected.
    {
        let dropped = plane_drop_pass(&board, &mut final_routes);
        if dropped > 0 {
            info!("plane via drops: {} surface pad(s) connected", dropped);
        }
    }

    // 5.9. Geometric validation with RECOVERY: a ripped net is not
    // abandoned — it gets rerouted (vias allowed) on a grid where all
    // surviving copper's footprint is blocked, then re-validated. Up to
    // three rounds; whatever still can't route legally stays unrouted
    // (honest) rather than shipping illegal copper.
    {
        let mut round = 0;
        let mut banned_vias: Vec<(usize, (f64, f64))> = Vec::new();
        let mut banned_dangles: Vec<(usize, (f64, f64))> = Vec::new();
        // Nets whose extension the commit gate stripped: nothing
        // illegal shipped, so the validator won't re-queue them — they
        // still deserve their retry round (with the new site bans).
        let mut gate_retry: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
        loop {
            if std::env::var("BHDL_PNR_DEBUG_PLANES").is_ok() {
                for (i, n) in board.nets.iter().enumerate() {
                    if n.plane_layer.is_some() {
                        log::info!(
                            "[planes] pre-validate '{}': {} segs, {} vias, {} spans",
                            n.name,
                            final_routes[i].segments.len(),
                            final_routes[i].vias.len(),
                            final_routes[i].path_spans.len()
                        );
                    }
                }
            }
            let mut ripped = validate_and_rip(
                &board,
                &mut final_routes,
                &mut banned_vias,
                &mut banned_dangles,
            );
            for &i in &gate_retry {
                if !ripped.contains(&i) {
                    ripped.push(i);
                }
            }
            gate_retry.clear();
            if ripped.is_empty() || round >= 3 {
                break;
            }
            round += 1;
            info!(
                "geometric recovery round {round}: rerouting {} ripped net(s) with vias",
                ripped.len()
            );
            // Partial (amputated) routes EXTEND from their surviving
            // tree; only fully-ripped nets reroute from scratch.
            // Plane-assigned nets are NOT tree-recovered: their
            // connectivity is pad → drop via → plane fill, which the
            // extension's tree model can't see — it "reclaimed" plane
            // pins with 69 segments of tree copper and orphaned drop
            // vias. Validator amputation of their offending copper
            // stands; the plane carries the rest.
            let ripped: Vec<usize> = ripped
                .into_iter()
                .filter(|&i| board.nets[i].plane_layer.is_none())
                .collect();
            // Topology-shaped nets rebuild WHOLE through the topology
            // router — extension/greedy would regrow a Steiner tree
            // and silently lose the declared star/chain (the stub
            // grading catches it; recovery should not cause it).
            let (topo, rest): (Vec<usize>, Vec<usize>) =
                ripped.iter().partition(|&&i| has_shape_topology(&board, i));
            for &i in &topo {
                let mut ext_grid = RoutingGrid::build(&board);
                for (j, route) in final_routes.iter().enumerate() {
                    if j != i && !route.is_empty() {
                        pathfinder::block_route_geometry(&mut ext_grid, route, &board);
                    }
                }
                let attract = pair_attract(&board, &final_routes, &ext_grid, i);
                let rebuilt = pathfinder::route_single_net(
                    &ext_grid,
                    &board.nets[i],
                    &board,
                    true,
                    attract.as_ref(),
                );
                if !rebuilt.is_empty() {
                    // Shape topologies are all-or-nothing: stripping a
                    // hop breaks the declared chain — gate WHOLE.
                    let mut rebuilt = rebuilt;
                    let mut bans = Vec::new();
                    let total = rebuilt.path_spans.len();
                    let kept = exact_commit_strip(
                        &board, &final_routes, i, &mut rebuilt, 0, &mut bans,
                    );
                    for pt in bans {
                        banned_dangles.push((i, pt));
                    }
                    if kept == total {
                        info!(
                            "recovery: topology rebuild of '{}' ({} span(s))",
                            board.nets[i].name,
                            rebuilt.path_spans.len()
                        );
                        final_routes[i] = rebuilt;
                    } else {
                        debug!(
                            "commit gate: topology rebuild of '{}' rejected ({} of {} spans illegal)",
                            board.nets[i].name, total - kept, total
                        );
                        gate_retry.insert(i);
                    }
                }
            }
            let (partial, whole): (Vec<usize>, Vec<usize>) =
                rest.iter().partition(|&&i| !final_routes[i].is_empty());
            for &i in &partial {
                let mut ext_grid = RoutingGrid::build(&board);
                for (j, route) in final_routes.iter().enumerate() {
                    if j != i && !route.is_empty() {
                        pathfinder::block_route_geometry(&mut ext_grid, route, &board);
                    }
                }
                let mut route = final_routes[i].clone();
                let banned: Vec<(f64, f64)> = banned_vias
                    .iter()
                    .filter(|(k, _)| *k == i)
                    .map(|&(_, xy)| xy)
                    .collect();
                let dangles: Vec<(f64, f64)> = banned_dangles
                    .iter()
                    .filter(|(k, _)| *k == i)
                    .map(|&(_, xy)| xy)
                    .collect();
                let attract = pair_attract(&board, &final_routes, &ext_grid, i);
                let from_span = route.path_spans.len();
                let got = pathfinder::extend_route(
                    &mut ext_grid, &board.nets[i], &board, &mut route, 1.0, 1.0, &banned,
                    &dangles, false, attract.as_ref(),
                );
                if got > 0 {
                    // Span-level gate: illegal new branches are stripped
                    // (their sites banned for the next round); clean
                    // branches commit.
                    let mut bans = Vec::new();
                    let kept = exact_commit_strip(
                        &board, &final_routes, i, &mut route, from_span, &mut bans,
                    );
                    if !bans.is_empty() {
                        gate_retry.insert(i);
                    }
                    for pt in bans {
                        banned_dangles.push((i, pt));
                    }
                    if kept > 0 {
                        info!(
                            "recovery: extended '{}' ({kept} legal branch(es) of {got} sink(s) reached)",
                            board.nets[i].name
                        );
                        final_routes[i] = route;
                    }
                }
            }
            // Whole-ripped nets recover SEQUENTIALLY, fat nets first:
            // jointly renegotiating several power nets on a mostly
            // hard-blocked grid just makes them contend for the last
            // free corridor until all overflow-rip. One at a time, each
            // gets the full remaining freedom, and its copper commits
            // (hard) before the next tries.
            let mut whole_sorted = whole.clone();
            whole_sorted.sort_by(|&a, &b| {
                let wa = board.nets[a].required_trace_width_mm;
                let wb = board.nets[b].required_trace_width_mm;
                wb.partial_cmp(&wa).unwrap_or(std::cmp::Ordering::Equal).then(a.cmp(&b))
            });
            // One grid for the whole round: every net in `whole` is
            // EMPTY (nothing of its own to exclude), so the blocked set
            // is identical — rebuilding the grid per net was the uno
            // board's 10-minute recovery. Copper committed by earlier
            // nets in the round is blocked incrementally.
            let mut rec_grid = if whole_sorted.is_empty() {
                None
            } else {
                let mut g = RoutingGrid::build(&board);
                for route in final_routes.iter() {
                    if !route.is_empty() {
                        pathfinder::block_route_geometry(&mut g, route, &board);
                    }
                }
                Some(g)
            };
            for &i in &whole_sorted {
                // Sequential greedy per net on ONE shared round grid:
                // negotiation is pointless when everything else is
                // hard-blocked and nets commit one at a time — and the
                // per-net grid rebuild + 100-iteration negotiated
                // reroute was the uno board's 10-minute recovery.
                // Widths are leaf shares (whole-rail IPC width is
                // unroutable and under-clears; per-pin flow analysis
                // is the width-honesty follow-up).
                let rec_grid = rec_grid.as_mut().unwrap();
                let mut fresh = Route::empty(final_routes[i].net_id);
                let banned_v: Vec<(f64, f64)> = banned_vias
                    .iter()
                    .filter(|(k, _)| *k == i)
                    .map(|&(_, xy)| xy)
                    .collect();
                let banned_d: Vec<(f64, f64)> = banned_dangles
                    .iter()
                    .filter(|(k, _)| *k == i)
                    .map(|&(_, xy)| xy)
                    .collect();
                let attract = pair_attract(&board, &final_routes, rec_grid, i);
                let got = pathfinder::extend_route(
                    rec_grid, &board.nets[i], &board, &mut fresh, 1.0, 1.0,
                    &banned_v, &banned_d, false, attract.as_ref(),
                );
                if got > 0 {
                    let mut bans = Vec::new();
                    let kept = exact_commit_strip(
                        &board, &final_routes, i, &mut fresh, 0, &mut bans,
                    );
                    if !bans.is_empty() {
                        gate_retry.insert(i);
                    }
                    for pt in bans {
                        banned_dangles.push((i, pt));
                    }
                    if kept > 0 {
                        info!(
                            "recovery: greedy reroute of '{}' reached {got} sink(s) ({kept} legal branch(es))",
                            board.nets[i].name
                        );
                        final_routes[i] = fresh;
                        pathfinder::block_route_geometry(rec_grid, &final_routes[i], &board);
                    }
                }
            }
            // Exact ladder for whatever the grid passes left behind
            // this round (escape / via-hop / cross-under / shove).
            for &i in &ripped {
                if board.nets[i].plane_layer.is_none()
                    && pathfinder::unreached_sink_count(
                        &board.nets[i],
                        &board,
                        &final_routes[i],
                    ) > 0
                {
                    offgrid_escape(&board, &mut final_routes, i);
                }
            }
        }
    }

    // 5.92. Plane-drop REPAIR: recovery's new copper can have made an
    // original drop site illegal (validator amputates the stub), and
    // plane nets are excluded from tree recovery — re-run the drop pass
    // so those pads get a fresh drop sited around the post-recovery
    // copper. Idempotent: pads with a live drop are skipped.
    {
        let repaired = plane_drop_pass(&board, &mut final_routes);
        if repaired > 0 {
            info!(
                "plane drop repair: {repaired} pad(s) re-dropped after recovery"
            );
            // Guarantee round for the repair: only the new drops have
            // changed since the recovery loop's last validate, so any
            // amputation here is a mis-sited drop — removed, honest
            // unconnected (the miter guarantee only validates when
            // corners were actually cut, so it cannot backstop this).
            let mut bv: Vec<(usize, (f64, f64))> = Vec::new();
            let mut bd: Vec<(usize, (f64, f64))> = Vec::new();
            validate_and_rip(&board, &mut final_routes, &mut bv, &mut bd);
        }
    }

    // 5.93. POUR ISLAND STITCH: a signal-layer pour shares its layer
    // with routed copper, and the emission's voids can sever the fill
    // into islands whose anchors then ship disconnected (the pour
    // A/B's 7-unc fragmentation family). Flood-fill the SAME raster
    // the emission fractures, and when anchors span islands, bridge
    // each stranded island to the main one with a same-net stitch
    // track on the pour layer (exact-kernel checked against foreign
    // copper; same-net contact merges it with both islands). No legal
    // stitch → honest warn, unc stands.
    {
        let stitched = pour_island_stitch(&board, &mut final_routes);
        if stitched > 0 {
            info!("pour island stitch: {stitched} bridge(s) added");
            let mut bv: Vec<(usize, (f64, f64))> = Vec::new();
            let mut bd: Vec<(usize, (f64, f64))> = Vec::new();
            validate_and_rip(&board, &mut final_routes, &mut bv, &mut bd);
        }
    }

    // 5.93. Completion RE-RUN: the recovery loop's final validate can
    // dangle-TRIM copper without counting as a rip (rips are what gate
    // another round), leaving a sink disconnected with no repair.
    // Completion is cheap when nothing is missing; guarantee-validate
    // anything it adds.
    {
        let extended = completion_pass(&board, &mut final_routes);
        if extended > 0 {
            let mut bv: Vec<(usize, (f64, f64))> = Vec::new();
            let mut bd: Vec<(usize, (f64, f64))> = Vec::new();
            validate_and_rip(&board, &mut final_routes, &mut bv, &mut bd);
        }
    }

    // 5.94. MEANDERS: length-match skew above the limit gets serpentine
    // length added to the SHORT member — one meander at a time, each
    // wrapped in a full-snapshot transaction: the validator judges the
    // new copper, and any objection restores the whole board (the skew
    // FAIL then stands honestly in the sign-off).
    {
        // Fixpoint: short collinear runs bound how much one application
        // can add — repeat until the skew is inside the limit or a
        // round makes no progress.
        let mut total = 0;
        for _ in 0..8 {
            let meandered = meander_pass(&board, &mut final_routes);
            if meandered == 0 {
                break;
            }
            total += meandered;
        }
        if total > 0 {
            info!("meander pass: {total} application(s) for skew");
        }
    }

    // 5.95. 45° corner mitering (verified post-pass) + one final
    // guarantee round: any miter the local check mis-judged is
    // amputated/trimmed by the validator like all copper.
    {
        // TRANSACTIONAL: elegance never costs connectivity. If the
        // guarantee round amputates anything on a mitered net (a case
        // the local check missed), that net reverts to its pre-miter
        // route wholesale — validated copper we already had.
        let saved = final_routes.clone();
        // Fidelity-tier polish: staircases are the ortho discipline's
        // grid artifact, so the pass runs where that discipline does.
        // Ungated it measured 7v/1unc on non-fidelity corpus boards —
        // the oracle sees conflict families the kernel pre-check
        // doesn't; the transactional guard only covers what the
        // INTERNAL validator objects to.
        let fidelity = board.config.route_bias.is_some()
            || board.config.design_track_width_mm.is_some();
        let staired = if !fidelity || std::env::var("BHDL_PNR_NO_MITER").is_ok() {
            0
        } else {
            staircase_pass(&board, &mut final_routes)
        };
        if staired > 0 {
            info!("staircase pass: {staired} chain(s) canonicalized");
        }
        let mitered = if std::env::var("BHDL_PNR_NO_MITER").is_ok() {
            0
        } else {
            miter_pass(&board, &mut final_routes)
        };
        if mitered + staired > 0 {
            info!("45° miter pass: {} corners cut", mitered);
            let post: Vec<usize> =
                final_routes.iter().map(|r| r.segments.len()).collect();
            let mut bv: Vec<(usize, (f64, f64))> = Vec::new();
            let mut bd: Vec<(usize, (f64, f64))> = Vec::new();
            let _ = validate_and_rip(&board, &mut final_routes, &mut bv, &mut bd);
            let mut reverted = 0;
            for i in 0..final_routes.len() {
                if final_routes[i].segments.len() < post[i] {
                    final_routes[i] = saved[i].clone();
                    reverted += 1;
                }
            }
            if reverted > 0 {
                info!("45° miter pass: {reverted} net(s) reverted (guarantee round objected)");
            }
        }
    }

    // 6. DRC
    if std::env::var("BHDL_PNR_DEBUG_CLEARANCE").is_ok() {
        debug_check_foreign_pads(&board, &final_routes, "final");
    }
    let mut drc_violations = legalization::check_drc(&board, &final_routes);

    // 7. Metrics
    let hpwl = analytical::compute_hpwl(&board);
    let total_length: f64 = final_routes.iter().map(|r| r.total_length()).sum();
    let total_vias: usize = final_routes.iter().map(|r| r.via_count()).sum();
    let routed_count = final_routes.iter().filter(|r| !r.is_empty()).count();
    let plane_nets = board.nets.iter()
        .filter(|n| n.pins.len() >= 2 && n.is_plane_connected(&board.layer_stack))
        .count();
    let total_nets = board.nets.iter()
        .filter(|n| n.pins.len() >= 2 && !n.is_plane_connected(&board.layer_stack))
        .count();

    let routability = if total_nets > 0 {
        routed_count as f64 / total_nets as f64 * 100.0
    } else {
        100.0
    };

    info!(
        "P&R complete: HPWL={:.1}mm, routed={:.1}mm, vias={}, routability={:.0}% ({}/{} signal, {} plane), DRC={}",
        hpwl, total_length, total_vias, routability,
        routed_count, total_nets, plane_nets, drc_violations.len()
    );


    // 5.97. M2 OFF-GRID ESCAPE — the LAST routing pass, after every
    // validate: the 2-layer tail's sinks die when final validates
    // amputate grid-built extensions with no repair stage after. The
    // continuous router connects each still-unreached pad straight to
    // tree copper (direct / L / sampled-Z), every leg exact-checked
    // against the ClearanceIndex — legal BY CONSTRUCTION, the first
    // copper that ships without a validator pass (M3 preview; the
    // KiCad oracle still grades the result).
    {
        let mut escaped = 0usize;
        for i in 0..board.nets.len() {
            if board.nets[i].plane_layer.is_some() {
                continue;
            }
            if pathfinder::unreached_sink_count(&board.nets[i], &board, &final_routes[i]) > 0
            {
                escaped += offgrid_escape(&board, &mut final_routes, i);
            }
        }
        if escaped > 0 {
            info!("off-grid escape: {escaped} sink(s) connected exactly");
        }
    }

    via_anchor_check(&board, &final_routes, "post-5.97");
    // 5.99. PART NUDGE — placement relief with routing feedback for
    // whatever the exact ladder still can't reach.
    {
        let nudged = part_nudge_pass(&mut board, &mut final_routes);
        if nudged > 0 {
            info!("part nudge: {nudged} sink(s) recovered by moving neighbors");
        }
    }

    via_anchor_check(&board, &final_routes, "post-nudge");
    // 5.995. PLANE SURFACE RESCUE — drop-less plane pads join their
    // net over surface copper (the ladder skips plane nets; a pad
    // outside its split-region band had no mechanism at all).
    {
        let rescued = plane_surface_rescue(&board, &mut final_routes);
        if rescued > 0 {
            info!("plane surface rescue: {rescued} pad(s) joined");
        }
    }

    via_anchor_check(&board, &final_routes, "post-rescue");
    probe_dangling_vias(&board, &final_routes, "post-5.995-rescue");
    // 5.996. FINAL ORPHAN SWEEP (after every copper-moving pass): copper-moving passes (shove, miter,
    // strip) can strand short parentless fragments after the last
    // validate — the oracle reads them as track_dangling. KiCad
    // endpoint-graph anchor semantics: an endpoint is anchored ON
    // another segment's centerline, INSIDE a same-net pad, or ON a
    // via barrel — lateral width-overlap never rescues a dangle.
    {
        let mut pruned = 0usize;
        for i in 0..board.nets.len() {
            // Plane nets INCLUDED: their fragments dangle the same
            // way (a 0.10mm VIN_12V orphan shipped as
            // track_dangling), and the anchor rules — own pad, via,
            // junction — are the same physics for them.
            if final_routes[i].is_empty() {
                continue;
            }
            let mut pour_net_sweep = false;
            let mut own_pads: Vec<(f64, f64, f64, f64)> = Vec::new();
            for comp in &board.components {
                let cos_t = comp.theta.cos();
                let sin_t = comp.theta.sin();
                let quarter = ((comp.theta / std::f64::consts::FRAC_PI_2).round() as i64)
                    .rem_euclid(2);
                for pin in &comp.pins {
                    if pin.net != Some(board.nets[i].id) || pin.unplaced {
                        continue;
                    }
                    let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
                    let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;
                    let (pw, ph) = match &pin.pad {
                        Some(p) => (p.width_mm, p.height_mm),
                        None => (0.5, 0.5),
                    };
                    let (pw, ph) = if quarter == 1 { (ph, pw) } else { (pw, ph) };
                    own_pads.push((gx, gy, pw / 2.0, ph / 2.0));
                }
            }
            // LAYER-AWARE pad list: a THT barrel (layer None) anchors
            // on any copper layer, an SMD pad only on its surface —
            // KiCad's actual pad-connect semantics. own_pads above is
            // layer-blind and stays that way for the passes tuned on
            // it; passes judging KiCad dangle-ness use this one (uno
            // s7: an In3 track tip inside an F.Cu SMD pad's XY rect
            // was called "anchored" and shipped as track_dangling).
            let last_cu = board.layer_stack.layers.len().saturating_sub(1);
            let mut pads_l: Vec<(f64, f64, f64, f64, Option<usize>)> = Vec::new();
            for comp in &board.components {
                let cos_t = comp.theta.cos();
                let sin_t = comp.theta.sin();
                let quarter = ((comp.theta / std::f64::consts::FRAC_PI_2).round() as i64)
                    .rem_euclid(2);
                let surf = match comp.side {
                    BoardSide::Top => 0usize,
                    BoardSide::Bottom => last_cu,
                };
                for pin in &comp.pins {
                    if pin.net != Some(board.nets[i].id) || pin.unplaced {
                        continue;
                    }
                    let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
                    let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;
                    let (pw, ph, tht) = match &pin.pad {
                        Some(p) => (p.width_mm, p.height_mm, p.drill_mm.is_some()),
                        None => (0.5, 0.5, false),
                    };
                    let (pw, ph) = if quarter == 1 { (ph, pw) } else { (pw, ph) };
                    pads_l.push((
                        gx,
                        gy,
                        pw / 2.0,
                        ph / 2.0,
                        if tht { None } else { Some(surf) },
                    ));
                }
            }
            if std::env::var("BHDL_PNR_DEBUG_NETS")
                .map(|v| board.nets[i].name.contains(&v))
                .unwrap_or(false)
            {
                let r = &final_routes[i];
                for (si, &(ps, pl)) in r.path_spans.iter().enumerate() {
                    log::info!("[sweep] '{}' span {si} pl={pl}", board.nets[i].name);
                    for sg in &r.segments[ps..ps + pl] {
                        log::info!(
                            "[sweep]   l{} ({:.3},{:.3})->({:.3},{:.3})",
                            sg.layer, sg.start.0, sg.start.1, sg.end.0, sg.end.1
                        );
                    }
                    if let Some(&(vs, vl)) = r.via_spans.get(si) {
                        for v in r.vias.iter().skip(vs).take(vl) {
                            log::info!("[sweep]   via ({:.3},{:.3})", v.x, v.y);
                        }
                    }
                }
            }
            // NEAR-DUPLICATE VIA REPAIR: two same-net vias closer
            // than the drill rule (seed-7: 0.10mm apart, placed by
            // different passes that cannot see each other's commits)
            // — remove the later one; segments anchored at its center
            // stay connected through the survivor's barrel (0.3mm
            // radius covers the 0.1mm offset). via_spans bookkeeping
            // shifts with the removal.
            {
                // Removal is only safe when the survivor's BARREL
                // covers the removed center (segments anchored there
                // stay connected): d well inside via_r. Wider pairs
                // (0.25..hole_gap) have no safe ship-time repair —
                // removing one strands its segments (measured:
                // track/via_dangling regressions).
                let safe_merge = (board.layer_stack.via.pad_mm / 2.0 - 0.05).max(0.1);
                loop {
                    let r = &final_routes[i];
                    let mut dup: Option<usize> = None;
                    'find_dup: for a in 0..r.vias.len() {
                        for b in (a + 1)..r.vias.len() {
                            let d = (r.vias[a].x - r.vias[b].x)
                                .hypot(r.vias[a].y - r.vias[b].y);
                            if d < safe_merge {
                                dup = Some(b);
                                break 'find_dup;
                            }
                        }
                    }
                    let Some(b) = dup else { break };
                    let r = &mut final_routes[i];
                    r.vias.remove(b);
                    for (vs, vl) in r.via_spans.iter_mut() {
                        if *vs <= b && b < *vs + *vl {
                            *vl -= 1;
                        } else if *vs > b {
                            *vs -= 1;
                        }
                    }
                }
                // WIDE PAIR + BRIDGE: pairs past the barrel-cover
                // bound but still inside the drill rule (uno s13:
                // 0.525mm, hole_to_hole) — removing one strands its
                // copper, so bridge each of its layers to the
                // survivor. A bridge along the center line whose
                // half-width h satisfies sqrt((d/2)^2 + h^2) <= via_r
                // stays inside the union of the two already-legal via
                // pads, so it cannot create a new clearance conflict.
                let via_r = board.layer_stack.via.pad_mm / 2.0;
                let wide_thresh = board.layer_stack.via.drill_mm + 0.27;
                loop {
                    let r = &final_routes[i];
                    let mut dup: Option<(usize, usize)> = None;
                    'find_wide: for a in 0..r.vias.len() {
                        for b in (a + 1)..r.vias.len() {
                            let d = (r.vias[a].x - r.vias[b].x)
                                .hypot(r.vias[a].y - r.vias[b].y);
                            let allowed_w =
                                2.0 * (via_r * via_r - (d / 2.0) * (d / 2.0)).max(0.0).sqrt()
                                    - 0.01;
                            if d >= safe_merge && d < wide_thresh && allowed_w >= 0.15 {
                                dup = Some((a, b));
                                break 'find_wide;
                            }
                        }
                    }
                    let Some((a, b)) = dup else { break };
                    let r = &mut final_routes[i];
                    let (ax, ay) = (r.vias[a].x, r.vias[a].y);
                    let (bx, by) = (r.vias[b].x, r.vias[b].y);
                    let d = (ax - bx).hypot(ay - by);
                    let allowed_w = (2.0
                        * (via_r * via_r - (d / 2.0) * (d / 2.0)).max(0.0).sqrt()
                        - 0.01)
                        .min(0.25);
                    let mut layers: Vec<usize> = Vec::new();
                    for sg in &r.segments {
                        for e in [sg.start, sg.end] {
                            if (e.0 - bx).hypot(e.1 - by) <= via_r
                                && !layers.contains(&sg.layer)
                            {
                                layers.push(sg.layer);
                            }
                        }
                    }
                    r.vias.remove(b);
                    for (vs, vl) in r.via_spans.iter_mut() {
                        if *vs <= b && b < *vs + *vl {
                            *vl -= 1;
                        } else if *vs > b {
                            *vs -= 1;
                        }
                    }
                    for layer in layers {
                        let ps = r.segments.len();
                        r.segments.push(RouteSegment {
                            start: (bx, by),
                            end: (ax, ay),
                            layer,
                            width_mm: allowed_w,
                        });
                        r.path_spans.push((ps, 1));
                        r.via_spans.push((r.vias.len(), 0));
                    }
                }
            }
            // ENDPOINT WELD: sub-micron drift between consecutive
            // polyline endpoints (measured 0.0003mm) breaks KiCad's
            // endpoint graph — the segments LOOK joined at any print
            // precision but dangle. Snap near-coincident neighbors to
            // exact equality before judging anchors.
            {
                let r = &mut final_routes[i];
                for &(ps, pl) in &r.path_spans {
                    for k in ps + 1..ps + pl {
                        let prev_end = r.segments[k - 1].end;
                        let d = (r.segments[k].start.0 - prev_end.0)
                            .hypot(r.segments[k].start.1 - prev_end.1);
                        if d > 0.0 && d < 1e-3 {
                            r.segments[k].start = prev_end;
                        }
                    }
                }
                // Cross-span: endpoints within 1e-3 of another span's
                // endpoint weld onto it; endpoints within 1e-3 of
                // another segment's CENTERLINE weld onto the exact
                // projection — an attach point computed against a
                // segment later reshaped (collinear collapse within
                // tolerance) sits ~1e-5 off the new line, and KiCad's
                // endpoint graph is nm-exact (measured: a 0.103mm
                // escape leg dangling at a point visually ON the
                // tree's diagonal).
                let snapshot: Vec<((f64, f64), (f64, f64))> =
                    r.segments.iter().map(|sg| (sg.start, sg.end)).collect();
                for sg in r.segments.iter_mut() {
                    for &(a, b) in &snapshot {
                        for target in [a, b] {
                            for pt in [&mut sg.start, &mut sg.end] {
                                let d = (pt.0 - target.0).hypot(pt.1 - target.1);
                                if d > 0.0 && d < 1e-3 {
                                    *pt = target;
                                }
                            }
                        }
                    }
                }
                for sk in 0..r.segments.len() {
                    for end in [0usize, 1] {
                        let pt = if end == 0 {
                            r.segments[sk].start
                        } else {
                            r.segments[sk].end
                        };
                        let mut best: Option<((f64, f64), f64)> = None;
                        for (oj, &(a, b)) in snapshot.iter().enumerate() {
                            if oj == sk {
                                continue;
                            }
                            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                            let l2 = dx * dx + dy * dy;
                            if l2 <= 1e-12 {
                                continue;
                            }
                            let t = (((pt.0 - a.0) * dx + (pt.1 - a.1) * dy) / l2)
                                .clamp(0.0, 1.0);
                            let q = (a.0 + t * dx, a.1 + t * dy);
                            let d = (pt.0 - q.0).hypot(pt.1 - q.1);
                            if d > 0.0
                                && d < 1e-3
                                && best.map_or(true, |(_, bd)| d < bd)
                            {
                                best = Some((q, d));
                            }
                        }
                        if let Some((q, _)) = best {
                            if end == 0 {
                                r.segments[sk].start = q;
                            } else {
                                r.segments[sk].end = q;
                            }
                        }
                    }
                }
            }
            // T-SPLIT: KiCad's endpoint graph does NOT connect an
            // endpoint landing in the INTERIOR of another track — a
            // mathematically exact on-centerline landing (measured
            // dist 0.0e0) still dangles. Split the host segment at
            // every such landing so the T becomes a real junction.
            loop {
                let r = &final_routes[i];
                let mut split: Option<(usize, (f64, f64))> = None;
                'find_t: for sg in &r.segments {
                    for pt in [sg.start, sg.end] {
                        for (sk, host) in r.segments.iter().enumerate() {
                            let (dx, dy) = (host.end.0 - host.start.0, host.end.1 - host.start.1);
                            let l2 = dx * dx + dy * dy;
                            if l2 <= 1e-12 {
                                continue;
                            }
                            let t = ((pt.0 - host.start.0) * dx + (pt.1 - host.start.1) * dy) / l2;
                            if t <= 1e-6 || t >= 1.0 - 1e-6 {
                                continue; // at host's endpoint already
                            }
                            let q = (host.start.0 + t * dx, host.start.1 + t * dy);
                            if (pt.0 - q.0).hypot(pt.1 - q.1) < 1e-6
                                && (pt.0 - host.start.0).hypot(pt.1 - host.start.1) > 1e-6
                                && (pt.0 - host.end.0).hypot(pt.1 - host.end.1) > 1e-6
                            {
                                split = Some((sk, pt));
                                break 'find_t;
                            }
                        }
                    }
                }
                let Some((sk, pt)) = split else { break };
                let r = &mut final_routes[i];
                let host = r.segments[sk].clone();
                r.segments[sk].end = pt;
                r.segments.insert(
                    sk + 1,
                    RouteSegment {
                        layer: host.layer,
                        start: pt,
                        end: host.end,
                        width_mm: host.width_mm,
                    },
                );
                for (si, (qs, ql)) in r.path_spans.iter_mut().enumerate() {
                    let _ = si;
                    if *qs <= sk && sk < *qs + *ql {
                        *ql += 1;
                    } else if *qs > sk {
                        *qs += 1;
                    }
                }
            }
            loop {
                let r = &final_routes[i];
                // SIGNAL-POUR nets: validator amputations routinely
                // leave LONG free-ended spurs (a mid-piece under-
                // clearing a pin row is removed, stranding a 3-4mm
                // tail that ships as track_dangling + an island).
                // A spur with a genuinely free end is dead copper at
                // ANY length — lift the caps for these nets only;
                // ordinary nets keep the conservative bounds.
                let pour_net = board.nets[i]
                    .plane_layer
                    .and_then(|pl| board.layer_stack.layers.get(pl))
                    .map(|l| l.kind == crate::types::LayerKind::Signal)
                    .unwrap_or(false);
                // Segment-level trim for REGIONED pours AND signal
                // nets — both carry small routes (pocket stubs, a
                // few dozen segments), while a board-wide ground's
                // thousands make the O(n^2)-per-removal scan
                // intractable (its spur classes are covered by the
                // span machinery). Signal nets need it too: rebuild
                // churn leaves dead-end tips the pour gate never
                // served (7 track_dangling at the mixer's seed-7
                // board, warning severity — invisible to the old
                // severity-error-only checks).
                pour_net_sweep = (pour_net
                    && board.nets[i].plane_region.is_some())
                    || board.nets[i].plane_layer.is_none();
                let (cap_pl, cap_len) =
                    if pour_net { (16usize, 20.0f64) } else { (4, 2.2) };
                let mut drop_span: Option<usize> = None;
                'stubs: for (si, &(ps, pl)) in r.path_spans.iter().enumerate() {
                    if pl == 0 || pl > cap_pl {
                        continue;
                    }
                    let len: f64 = r.segments[ps..ps + pl]
                        .iter()
                        .map(|sg| (sg.end.0 - sg.start.0).hypot(sg.end.1 - sg.start.1))
                        .sum();
                    if len > cap_len {
                        continue;
                    }
                    let via_r = board.layer_stack.via.pad_mm / 2.0;
                    // KiCad flags a track with ANY unconnected end —
                    // a one-end-anchored spur dangles just the same.
                    let ends = [r.segments[ps].start, r.segments[ps + pl - 1].end];
                    let all_anchored = ends.iter().all(|&e| {
                        // ALL other copper anchors, not just
                        // span-REGISTERED copper: rescue joins, trim
                        // T-split halves, and amputation leftovers
                        // carry no span bookkeeping, and a via hop's
                        // 17mm B-leg landing on such a host was
                        // judged tip-unanchored and eaten (seed-7
                        // via_dangling pair).
                        r.segments
                            .iter()
                            .enumerate()
                            .any(|(k, sg)| {
                                (k < ps || k >= ps + pl)
                                    && geom::point_segment_dist(e, sg.start, sg.end)
                                        <= 0.05
                            })
                            || r
                                .vias
                                .iter()
                                .any(|v| (v.x - e.0).hypot(v.y - e.1) <= via_r)
                            || own_pads.iter().any(|&(cx, cy, hx, hy)| {
                                (e.0 - cx).abs() <= hx && (e.1 - cy).abs() <= hy
                            })
                            // Interior of the span's own polyline (a
                            // zig-zag's middle joints anchor its ends).
                            || r.segments[ps..ps + pl].iter().any(|sg| {
                                geom::point_segment_dist(e, sg.start, sg.end) <= 0.05
                                    && (e.0 - sg.start.0).hypot(e.1 - sg.start.1) > 1e-6
                                    && (e.0 - sg.end.0).hypot(e.1 - sg.end.1) > 1e-6
                            })
                    });
                    if all_anchored {
                        continue 'stubs;
                    }
                    drop_span = Some(si);
                    break;
                }
                match drop_span {
                    Some(si) => {
                        let mut d = vec![false; final_routes[i].path_spans.len()];
                        d[si] = true;
                        strip_route_spans(&mut final_routes[i], &d);
                        pruned += 1;
                    }
                    None => break,
                }
            }
            // POUR-NET SEGMENT-LEVEL free-end trim: validator
            // amputations clear span bookkeeping, so their leftover
            // spurs are invisible to the span-based loop above — but
            // a segment endpoint anchored on NOTHING (no pad, no via,
            // no other segment's centerline) dangles per KiCad
            // regardless. Iteratively eat such tips (the amputated
            // 3.3mm vbias spur family).
            if pour_net_sweep {
                if std::env::var("BHDL_PNR_PROBE").is_ok() {
                    log::info!(
                        "[probe] pour-net trim: '{}' {} seg(s)",
                        board.nets[i].name,
                        final_routes[i].segments.len()
                    );
                }
                loop {
                    let r = &final_routes[i];
                    let via_r = board.layer_stack.via.pad_mm / 2.0;
                    let mut drop: Option<usize> = None;
                    'segs: for (sk, sg) in r.segments.iter().enumerate() {
                        for &e in &[sg.start, sg.end] {
                            // SAME-LAYER segments only — a B.Cu leg
                            // under an F.Cu tip is no anchor (cross-
                            // layer joins need the via test below).
                            let anchored = r
                                .segments
                                .iter()
                                .enumerate()
                                .any(|(sj, s2)| {
                                    sj != sk
                                        && s2.layer == sg.layer
                                        && geom::point_segment_dist(
                                            e, s2.start, s2.end,
                                        ) <= 0.05
                                })
                                || r.vias.iter().any(|v| {
                                    (v.x - e.0).hypot(v.y - e.1) <= via_r
                                })
                                || own_pads.iter().any(|&(cx, cy, hx, hy)| {
                                    (e.0 - cx).abs() <= hx
                                        && (e.1 - cy).abs() <= hy
                                });
                            if !anchored {
                                drop = Some(sk);
                                break 'segs;
                            }
                        }
                    }
                    match drop {
                        Some(sk) => {
                            let r = &mut final_routes[i];
                            if std::env::var("BHDL_PNR_PROBE").is_ok() {
                                let sg = &r.segments[sk];
                                log::info!(
                                    "[probe] tip-trim DROP '{}' l{} ({:.2},{:.2})->({:.2},{:.2})",
                                    board.nets[i].name, sg.layer,
                                    sg.start.0, sg.start.1, sg.end.0, sg.end.1
                                );
                            }
                            r.segments.remove(sk);
                            // span bookkeeping is already stale for
                            // these nets — clear it (rip-whole
                            // semantics on later damage).
                            for (qs, ql) in r.path_spans.iter_mut() {
                                if *qs > sk {
                                    *qs -= 1;
                                } else if sk < *qs + *ql {
                                    *ql = ql.saturating_sub(1);
                                }
                            }
                            pruned += 1;
                        }
                        None => break,
                    }
                }
            }

            // COVERED DUPLICATES: an END segment whose whole body lies
            // inside another same-net same-layer COLLINEAR segment is
            // duplicate copper — KiCad keeps it as a separate track
            // whose free tip dangles (the uno 0.6mm spur inside a
            // parallel run). Deleting it leaves the span ending at a
            // true junction.
            loop {
                let r = &final_routes[i];
                let mut hit: Option<(usize, bool)> = None; // (span, from_back)
                'covered: for (si, &(ps, pl)) in r.path_spans.iter().enumerate() {
                    if pl == 0 {
                        continue;
                    }
                    for (&at, &from_back) in [(ps, false), (ps + pl - 1, true)].iter()
                        .map(|(a, b)| (a, b))
                    {
                        let sg = &r.segments[at];
                        let d1 = (sg.end.0 - sg.start.0, sg.end.1 - sg.start.1);
                        if d1.0.hypot(d1.1) < 1e-9 {
                            continue;
                        }
                        let via_r = board.layer_stack.via.pad_mm / 2.0;
                        let carries_via = r.vias.iter().any(|v| {
                            (v.x - sg.start.0).hypot(v.y - sg.start.1) <= via_r
                                || (v.x - sg.end.0).hypot(v.y - sg.end.1) <= via_r
                        });
                        let covered = !carries_via
                            && r.segments.iter().enumerate().any(|(sk, o)| {
                            if sk == at || o.layer != sg.layer {
                                return false;
                            }
                            let d2 = (o.end.0 - o.start.0, o.end.1 - o.start.1);
                            (d1.0 * d2.1 - d1.1 * d2.0).abs() < 1e-6
                                && geom::point_segment_dist(sg.start, o.start, o.end) <= 0.02
                                && geom::point_segment_dist(sg.end, o.start, o.end) <= 0.02
                        });
                        if covered {
                            hit = Some((si, from_back));
                            break 'covered;
                        }
                    }
                }
                let Some((si, from_back)) = hit else { break };
                let (ps, pl) = final_routes[i].path_spans[si];
                if pl == 1 {
                    let mut d = vec![false; final_routes[i].path_spans.len()];
                    d[si] = true;
                    strip_route_spans(&mut final_routes[i], &d);
                } else {
                    let r = &mut final_routes[i];
                    let at = if from_back { ps + pl - 1 } else { ps };
                    r.segments.remove(at);
                    r.path_spans[si].1 -= 1;
                    for (qs, _) in r.path_spans.iter_mut() {
                        if *qs > at {
                            *qs -= 1;
                        }
                    }
                }
                pruned += 1;
            }
            // OUT-AND-BACK SPURS: consecutive COLLINEAR segments in
            // opposite directions retrace copper — the overhang past
            // the turnaround is a spur whose tip dangles at a MIDDLE
            // vertex, invisible to end-based trimming (partial
            // retraces too: out 1.2mm, back 0.6mm). Collapse the pair
            // to a.start -> b.end; the path stays continuous, the
            // spur tip goes.
            loop {
                let r = &final_routes[i];
                let mut hit: Option<(usize, usize)> = None; // (span, first seg)
                'outback: for (si, &(ps, pl)) in r.path_spans.iter().enumerate() {
                    if pl < 2 {
                        continue;
                    }
                    for k in ps..ps + pl - 1 {
                        let a = &r.segments[k];
                        let b = &r.segments[k + 1];
                        if a.layer != b.layer {
                            continue;
                        }
                        let da = (a.end.0 - a.start.0, a.end.1 - a.start.1);
                        let db = (b.end.0 - b.start.0, b.end.1 - b.start.1);
                        let la = da.0.hypot(da.1);
                        let lb = db.0.hypot(db.1);
                        if la < 1e-9 || lb < 1e-9 {
                            continue;
                        }
                        let cross = da.0 * db.1 - da.1 * db.0;
                        let dot = da.0 * db.0 + da.1 * db.1;
                        // Collinear retrace (exact), or a HAIRPIN —
                        // out-and-back at a shallow angle (< ~25°):
                        // the two arms' copper union tapers to a
                        // sub-min-feature spike at the tip, KiCad's
                        // copper_sliver (measured: a 3.2mm V at 10.5°
                        // shipped 2 slivers under si_cost routing).
                        // Hairpin collapse REROUTES copper (the chord
                        // deviates from the arms) — exact-gated below.
                        let sin_ang = cross.abs() / (la * lb);
                        let collinear = cross.abs() < 1e-6 && dot < 0.0;
                        let hairpin = !collinear
                            && dot < 0.0
                            && sin_ang < 0.42
                            && la.min(lb) > 0.5
                            // Fidelity mode: the collapse chord must
                            // read as hand routing (H/V/45) — it was
                            // the source of the long arbitrary-angle
                            // runs the comparison review flagged.
                            && (!(board.config.route_bias.is_some()
                                || board.config.design_track_width_mm.is_some())
                                || {
                                    let (cs, ce) = (a.start, b.end);
                                    let (dx, dy) =
                                        ((ce.0 - cs.0).abs(), (ce.1 - cs.1).abs());
                                    dx < 1e-9 || dy < 1e-9 || (dx - dy).abs() < 1e-9
                                });
                        if collinear || hairpin {
                            // A via AT the retrace tip rides that
                            // copper — collapsing orphans it (final
                            // sweep was the pass stranding maze vias:
                            // via-check clean post-rescue, dangling
                            // at final).
                            let tip = a.end;
                            let via_r = board.layer_stack.via.pad_mm / 2.0;
                            if r.vias.iter().any(|v| {
                                (v.x - tip.0).hypot(v.y - tip.1) <= via_r
                            }) {
                                continue;
                            }
                            if hairpin {
                                // The chord a.start -> b.end is NEW
                                // copper — commit only when the exact
                                // kernel clears it.
                                let idx = geom::ClearanceIndex::build(
                                    &board,
                                    &final_routes,
                                    Some(final_routes[i].net_id),
                                );
                                let r2 = &final_routes[i];
                                let (aa, bb) =
                                    (r2.segments[k].clone(), r2.segments[k + 1].clone());
                                if idx
                                    .first_conflict(
                                        aa.start,
                                        bb.end,
                                        aa.width_mm,
                                        aa.layer,
                                        r2.net_id,
                                    )
                                    .is_some()
                                {
                                    continue;
                                }
                            }
                            hit = Some((si, k));
                            break 'outback;
                        }
                    }
                }
                let Some((si, k)) = hit else { break };
                let r = &mut final_routes[i];
                let a_start = r.segments[k].start;
                let b_end = r.segments[k + 1].end;
                if (a_start.0 - b_end.0).hypot(a_start.1 - b_end.1) < 1e-6 {
                    // Full retrace: both segments vanish.
                    r.segments.drain(k..k + 2);
                    r.path_spans[si].1 -= 2;
                    for (qs, _) in r.path_spans.iter_mut() {
                        if *qs > k {
                            *qs -= 2;
                        }
                    }
                } else {
                    // Partial: one straight segment a.start -> b.end.
                    r.segments[k].end = b_end;
                    r.segments.remove(k + 1);
                    r.path_spans[si].1 -= 1;
                    for (qs, _) in r.path_spans.iter_mut() {
                        if *qs > k {
                            *qs -= 1;
                        }
                    }
                }
                pruned += 1;
            }
            // CROSS-SPAN HAIRPIN STITCH: two same-net arms sharing a
            // tip ACROSS span boundaries (an attach branch doubling
            // back beside the through route at < ~25°) taper their
            // copper union to a sub-min-feature spike where the
            // capsules separate — KiCad's copper_sliver (the s42
            // si_cost latent: a 10.5° V at (42.83,29.80)). The
            // consecutive-pair collapse above cannot see these. The
            // repair ADDS copper: one exact-gated chord across the
            // arms at 1.2mm from the tip blunts the union outline;
            // nothing is removed, connectivity only gains.
            {
                let mut stitches: Vec<(RouteSegment, usize)> = Vec::new(); // (chord, layer unused)
                {
                    let r = &final_routes[i];
                    let nseg = r.segments.len();
                    let span_of = |k: usize| -> usize {
                        r.path_spans
                            .iter()
                            .position(|&(ps, pl)| k >= ps && k < ps + pl)
                            .unwrap_or(usize::MAX)
                    };
                    for k1 in 0..nseg {
                        for k2 in (k1 + 1)..nseg {
                            let (s1, s2) = (&r.segments[k1], &r.segments[k2]);
                            if s1.layer != s2.layer {
                                continue;
                            }
                            // consecutive-in-span pairs were handled above
                            if span_of(k1) == span_of(k2) && k2 == k1 + 1 {
                                continue;
                            }
                            let mut tip: Option<(f64, f64)> = None;
                            for pa in [s1.start, s1.end] {
                                for pb in [s2.start, s2.end] {
                                    if (pa.0 - pb.0).hypot(pa.1 - pb.1) < 1e-6 {
                                        tip = Some(pa);
                                    }
                                }
                            }
                            let Some(tip) = tip else { continue };
                            let away = |sg: &RouteSegment| -> ((f64, f64), f64) {
                                let (o, e) = if (sg.start.0 - tip.0).hypot(sg.start.1 - tip.1)
                                    < 1e-6
                                {
                                    (sg.start, sg.end)
                                } else {
                                    (sg.end, sg.start)
                                };
                                let v = (e.0 - o.0, e.1 - o.1);
                                let l = v.0.hypot(v.1);
                                ((v.0 / l.max(1e-12), v.1 / l.max(1e-12)), l)
                            };
                            let (u1, l1) = away(s1);
                            let (u2, l2) = away(s2);
                            if l1 < 1.4 || l2 < 1.4 {
                                continue;
                            }
                            let cosang = u1.0 * u2.0 + u1.1 * u2.1;
                            if cosang < (25.0_f64).to_radians().cos() || cosang > 1.0 - 1e-9
                            {
                                continue; // not a shallow V (or same direction overlap)
                            }
                            let d = 1.2;
                            let pa = (tip.0 + u1.0 * d, tip.1 + u1.1 * d);
                            let pb = (tip.0 + u2.0 * d, tip.1 + u2.1 * d);
                            // Already stitched here?
                            let dup = r.segments.iter().any(|sg| {
                                ((sg.start.0 - pa.0).hypot(sg.start.1 - pa.1) < 1e-6
                                    && (sg.end.0 - pb.0).hypot(sg.end.1 - pb.1) < 1e-6)
                                    || ((sg.start.0 - pb.0).hypot(sg.start.1 - pb.1) < 1e-6
                                        && (sg.end.0 - pa.0).hypot(sg.end.1 - pa.1) < 1e-6)
                            }) || stitches.iter().any(|(sg, _)| {
                                (sg.start.0 - pa.0).hypot(sg.start.1 - pa.1) < 1e-6
                            });
                            if dup {
                                continue;
                            }
                            stitches.push((
                                RouteSegment {
                                    layer: s1.layer,
                                    start: pa,
                                    end: pb,
                                    width_mm: s1.width_mm.max(s2.width_mm),
                                },
                                0,
                            ));
                        }
                    }
                }
                if !stitches.is_empty() {
                    let idx =
                        geom::ClearanceIndex::build(&board, &final_routes, Some(final_routes[i].net_id));
                    let net_id = final_routes[i].net_id;
                    for (sg, _) in stitches {
                        if idx
                            .first_conflict(sg.start, sg.end, sg.width_mm, sg.layer, net_id)
                            .is_some()
                        {
                            continue; // foreign copper in the wedge — leave it
                        }
                        // The chord ends land in host-segment INTERIORS
                        // — KiCad's endpoint graph is end-to-end only,
                        // so each host must be T-SPLIT at the landing
                        // (the un-split version shipped the chord as a
                        // track_dangling pair).
                        let r = &mut final_routes[i];
                        for pt in [sg.start, sg.end] {
                            let host = r.segments.iter().position(|h| {
                                h.layer == sg.layer
                                    && geom::point_segment_dist(pt, h.start, h.end) < 1e-6
                                    && (pt.0 - h.start.0).hypot(pt.1 - h.start.1) > 1e-6
                                    && (pt.0 - h.end.0).hypot(pt.1 - h.end.1) > 1e-6
                            });
                            if let Some(hk) = host {
                                let old_end = r.segments[hk].end;
                                r.segments[hk].end = pt;
                                r.segments.insert(
                                    hk + 1,
                                    RouteSegment {
                                        layer: sg.layer,
                                        start: pt,
                                        end: old_end,
                                        width_mm: r.segments[hk].width_mm,
                                    },
                                );
                                for (qs, ql) in r.path_spans.iter_mut() {
                                    if *qs <= hk && hk < *qs + *ql {
                                        *ql += 1;
                                    } else if *qs > hk {
                                        *qs += 1;
                                    }
                                }
                            }
                        }
                        let seg_start = r.segments.len();
                        let via_start = r.vias.len();
                        info!(
                            "hairpin stitch: '{}' blunted a V at ({:.2},{:.2})",
                            board.nets[i].name, sg.start.0, sg.start.1
                        );
                        r.segments.push(sg);
                        r.path_spans.push((seg_start, 1));
                        r.path_parents.push(None);
                        r.via_spans.push((via_start, 0));
                        pruned += 1;
                    }
                }
            }
            // TAIL TRIM: a long span whose END dangles (post-validator
            // passes can amputate what it attached to) — walk inward
            // one segment at a time until a junction/anchor, exactly
            // the validator's dangle-trim semantics.
            let via_r = board.layer_stack.via.pad_mm / 2.0;
            for _outer in 0..8 {
            let mut via_removed = false;
            for _ in 0..64 {
                let r = &final_routes[i];
                // LAYER-AWARE + COLLINEAR-AWARE: a segment crossing
                // on another layer anchors nothing without a via, and
                // an endpoint landing in the INTERIOR of a COLLINEAR
                // track is a merged-run interior, not a junction —
                // KiCad merges collinear same-net tracks, so the run's
                // interior can't terminate anything (the recorded
                // "lateral width-overlap never rescues" lesson's
                // collinear sibling). End-to-end contact and true
                // T-junctions still anchor.
                let anchored = |e: (f64, f64), layer: usize, dir: (f64, f64), skip: usize| -> bool {
                    r.segments.iter().enumerate().any(|(sk, sg)| {
                        if sk == skip
                            || sg.layer != layer
                            || geom::point_segment_dist(e, sg.start, sg.end) > 0.05
                        {
                            return false;
                        }
                        let _ = dir;
                        true
                    }) || r.vias.iter().any(|v| (v.x - e.0).hypot(v.y - e.1) <= via_r)
                        // Layer-aware: an SMD pad only anchors a track
                        // ON its surface layer (THT = any layer).
                        || pads_l.iter().any(|&(cx, cy, hx, hy, pl_)| {
                            pl_.map_or(true, |l| l == layer)
                                && (e.0 - cx).abs() <= hx
                                && (e.1 - cy).abs() <= hy
                        })
                };
                let mut cut: Option<(usize, bool)> = None; // (span, from_back)
                'scan: for (si, &(ps, pl)) in r.path_spans.iter().enumerate() {
                    if pl == 0 {
                        continue;
                    }
                    let front = r.segments[ps].clone();
                    let back = r.segments[ps + pl - 1].clone();
                    let seg_len = |sg: &RouteSegment| {
                        (sg.end.0 - sg.start.0).hypot(sg.end.1 - sg.start.1)
                    };
                    if std::env::var("BHDL_PNR_DEBUG_NETS")
                        .map(|v| board.nets[i].name.contains(&v))
                        .unwrap_or(false)
                    {

                    }
                    let fdir = (front.end.0 - front.start.0, front.end.1 - front.start.1);
                    let bdir = (back.end.0 - back.start.0, back.end.1 - back.start.1);
                    // Length cap only for PLANE nets, whose track ends
                    // may terminate inside a fill this segment scan
                    // cannot see. Signal-net dangles have no such
                    // rescue — a 1.76mm stranded spur leg (uno s7,
                    // free MCU placement) sat just past the old
                    // universal 1.5 cap and shipped as track_dangling.
                    let cap = if board.nets[i].plane_layer.is_some() {
                        1.5
                    } else {
                        f64::INFINITY
                    };
                    if seg_len(&front) <= cap && !anchored(front.start, front.layer, fdir, ps) {
                        cut = Some((si, false));
                        break 'scan;
                    }
                    if seg_len(&back) <= cap && !anchored(back.end, back.layer, bdir, ps + pl - 1) {
                        cut = Some((si, true));
                        break 'scan;
                    }
                }
                let Some((si, from_back)) = cut else { break };
                let (ps, pl) = final_routes[i].path_spans[si];
                if pl == 1 {
                    let mut d = vec![false; final_routes[i].path_spans.len()];
                    d[si] = true;
                    strip_route_spans(&mut final_routes[i], &d);
                } else {
                    let r = &mut final_routes[i];
                    let at = if from_back { ps + pl - 1 } else { ps };
                    r.segments.remove(at);
                    r.path_spans[si].1 -= 1;
                    for (qs, _) in r.path_spans.iter_mut() {
                        if *qs > at {
                            *qs -= 1;
                        }
                    }
                }
                pruned += 1;
            }
            // USELESS-VIA CLEANUP: the trim can strip a via's only
            // segment on some layer. If every remaining copper
            // contact sits on ONE layer, the via performs no layer
            // change — KiCad grades it via_dangling. Deleting it is
            // connectivity-neutral (coincident same-layer endpoints
            // stay joined by the endpoint graph), so delete it and
            // re-run the trim for any stub it was holding up. Plane
            // nets exempt (their vias bond to fills this segment scan
            // can't see); vias inside an own pad exempt (a THT barrel
            // spans layers on its own). Layer credit uses full
            // track-body distance, not just endpoints — a via sitting
            // mid-segment is still connected there.
            if board.nets[i].plane_layer.is_none() {
                loop {
                    let r = &final_routes[i];
                    let mut doomed: Option<usize> = None;
                    'find_useless: for (vi, v) in r.vias.iter().enumerate() {
                        if own_pads.iter().any(|&(cx, cy, hx, hy)| {
                            (v.x - cx).abs() <= hx && (v.y - cy).abs() <= hy
                        }) {
                            continue;
                        }
                        let mut first: Option<usize> = None;
                        let mut multi = false;
                        for sg in &r.segments {
                            if geom::point_segment_dist((v.x, v.y), sg.start, sg.end)
                                <= via_r + sg.width_mm / 2.0
                            {
                                match first {
                                    None => first = Some(sg.layer),
                                    Some(l) if l != sg.layer => multi = true,
                                    _ => {}
                                }
                            }
                        }
                        if !multi {
                            doomed = Some(vi);
                            break 'find_useless;
                        }
                    }
                    let Some(vi) = doomed else { break };
                    let r = &mut final_routes[i];
                    r.vias.remove(vi);
                    for (vs, vl) in r.via_spans.iter_mut() {
                        if *vs <= vi && vi < *vs + *vl {
                            *vl -= 1;
                        } else if *vs > vi {
                            *vs -= 1;
                        }
                    }
                    via_removed = true;
                    pruned += 1;
                }
            }
            // EXACT-TWIN DEDUP: two spans that traversed the same
            // column (one up, one down) leave segment-for-segment
            // duplicates. The covered-dup primitive skipped them while
            // a via anchored the shared tip; once the via cleanup
            // deletes that via, the twins are pure copper noise and
            // KiCad grades one of each pair track_dangling. Remove the
            // later twin; the trim then walks the surviving chain back
            // to its junction.
            {
                let r = &mut final_routes[i];
                let mut doomed: Vec<usize> = Vec::new();
                for a in 0..r.segments.len() {
                    if doomed.contains(&a) {
                        continue;
                    }
                    for b in (a + 1)..r.segments.len() {
                        if doomed.contains(&b) {
                            continue;
                        }
                        let (sa, sb) = (&r.segments[a], &r.segments[b]);
                        if sa.layer != sb.layer {
                            continue;
                        }
                        let same = |p: (f64, f64), q: (f64, f64)| {
                            (p.0 - q.0).abs() < 1e-6 && (p.1 - q.1).abs() < 1e-6
                        };
                        if (same(sa.start, sb.start) && same(sa.end, sb.end))
                            || (same(sa.start, sb.end) && same(sa.end, sb.start))
                        {
                            doomed.push(b);
                        }
                    }
                }
                if !doomed.is_empty() {
                    doomed.sort_by_key(|&x| std::cmp::Reverse(x));
                    for &b in &doomed {
                        r.segments.remove(b);
                        for (ps, pl) in r.path_spans.iter_mut() {
                            if *ps <= b && b < *ps + *pl {
                                *pl -= 1;
                            } else if *ps > b {
                                *ps -= 1;
                            }
                        }
                        pruned += 1;
                    }
                    via_removed = true; // rerun the trim on the survivors
                }
            }
            if !via_removed {
                break;
            }
            }
            // ORPHAN ISLANDS: fragments that anchor EACH OTHER pass
            // every per-end test yet the group as a whole touches no
            // pad — copper attached to nothing (uno s7 free-MCU: a
            // 0.2375mm + 0.30mm RESET pair 0.04mm apart). Judge
            // connectivity at the COMPONENT level: spans connect via
            // same-layer endpoint/body contact or a shared via;
            // components with no own-pad contact are deleted whole.
            // Plane nets exempt — their components bond to fills this
            // scan cannot see.
            if board.nets[i].plane_layer.is_none() {
                let r = &final_routes[i];
                let ns = r.path_spans.len();
                if ns > 0 {
                    let mut comp: Vec<usize> = (0..ns).collect();
                    fn find(c: &mut Vec<usize>, x: usize) -> usize {
                        let mut x = x;
                        while c[x] != x {
                            c[x] = c[c[x]];
                            x = c[x];
                        }
                        x
                    }
                    // EXACT contact, matching KiCad's nm-exact
                    // endpoint graph: the weld + T-split passes have
                    // already normalized every true junction to exact
                    // coordinates, so anything still 0.001-0.05 apart
                    // is NOT connected on the shipped board — a
                    // tolerant test here would bridge near-miss
                    // fragments onto the padded tree and hide them.
                    let touches = |&(ps, pl): &(usize, usize),
                                   &(qs, ql): &(usize, usize)|
                     -> bool {
                        for a in &r.segments[ps..ps + pl] {
                            for b in &r.segments[qs..qs + ql] {
                                if a.layer != b.layer {
                                    continue;
                                }
                                for e in [a.start, a.end] {
                                    if geom::point_segment_dist(e, b.start, b.end) <= 1e-6 {
                                        return true;
                                    }
                                }
                                for e in [b.start, b.end] {
                                    if geom::point_segment_dist(e, a.start, a.end) <= 1e-6 {
                                        return true;
                                    }
                                }
                            }
                        }
                        false
                    };
                    let via_touches = |&(ps, pl): &(usize, usize), v: &RouteVia| -> bool {
                        r.segments[ps..ps + pl].iter().any(|sg| {
                            geom::point_segment_dist((v.x, v.y), sg.start, sg.end)
                                <= via_r + sg.width_mm / 2.0
                        })
                    };
                    for a in 0..ns {
                        for b in (a + 1)..ns {
                            if find(&mut comp, a) != find(&mut comp, b)
                                && (touches(&r.path_spans[a], &r.path_spans[b])
                                    || r.vias.iter().any(|v| {
                                        via_touches(&r.path_spans[a], v)
                                            && via_touches(&r.path_spans[b], v)
                                    }))
                            {
                                let (ra, rb) = (find(&mut comp, a), find(&mut comp, b));
                                comp[ra] = rb;
                            }
                        }
                    }
                    let mut root_has_pad = vec![false; ns];
                    for a in 0..ns {
                        let (ps, pl) = r.path_spans[a];
                        let hit = r.segments[ps..ps + pl].iter().any(|sg| {
                            [sg.start, sg.end].iter().any(|e| {
                                pads_l.iter().any(|&(cx, cy, hx, hy, layer)| {
                                    layer.map_or(true, |l| l == sg.layer)
                                        && (e.0 - cx).abs() <= hx
                                        && (e.1 - cy).abs() <= hy
                                })
                            })
                        });
                        if hit {
                            let ra = find(&mut comp, a);
                            root_has_pad[ra] = true;
                        }
                    }
                    let doomed: Vec<bool> = (0..ns)
                        .map(|a| {
                            let ra = find(&mut comp, a);
                            !root_has_pad[ra] && r.path_spans[a].1 > 0
                        })
                        .collect();
                    if doomed.iter().any(|&d| d) {
                        pruned += doomed.iter().filter(|&&d| d).count();
                        strip_route_spans(&mut final_routes[i], &doomed);
                    }
                }
            }
            // SECOND free-end trim: the covered-duplicates and
            // component passes above can remove the very copper that
            // anchored a spur tip during the first trim (measured:
            // the 3.3mm vbias spur's tip sat on a duplicate that the
            // covered pass then deleted).
            // POUR-NET SEGMENT-LEVEL free-end trim: validator
            // amputations clear span bookkeeping, so their leftover
            // spurs are invisible to the span-based loop above — but
            // a segment endpoint anchored on NOTHING (no pad, no via,
            // no other segment's centerline) dangles per KiCad
            // regardless. Iteratively eat such tips (the amputated
            // 3.3mm vbias spur family).
            if pour_net_sweep {
                if std::env::var("BHDL_PNR_PROBE").is_ok() {
                    log::info!(
                        "[probe] pour-net trim: '{}' {} seg(s)",
                        board.nets[i].name,
                        final_routes[i].segments.len()
                    );
                }
                loop {
                    let r = &final_routes[i];
                    let via_r = board.layer_stack.via.pad_mm / 2.0;
                    let mut drop: Option<usize> = None;
                    'segs: for (sk, sg) in r.segments.iter().enumerate() {
                        for &e in &[sg.start, sg.end] {
                            // SAME-LAYER segments only — a B.Cu leg
                            // under an F.Cu tip is no anchor (cross-
                            // layer joins need the via test below).
                            let anchored = r
                                .segments
                                .iter()
                                .enumerate()
                                .any(|(sj, s2)| {
                                    sj != sk
                                        && s2.layer == sg.layer
                                        && geom::point_segment_dist(
                                            e, s2.start, s2.end,
                                        ) <= 0.05
                                })
                                || r.vias.iter().any(|v| {
                                    (v.x - e.0).hypot(v.y - e.1) <= via_r
                                })
                                || own_pads.iter().any(|&(cx, cy, hx, hy)| {
                                    (e.0 - cx).abs() <= hx
                                        && (e.1 - cy).abs() <= hy
                                });
                            if !anchored {
                                drop = Some(sk);
                                break 'segs;
                            }
                        }
                    }
                    match drop {
                        Some(sk) => {
                            let r = &mut final_routes[i];
                            if std::env::var("BHDL_PNR_PROBE").is_ok() {
                                let sg = &r.segments[sk];
                                log::info!(
                                    "[probe] tip-trim DROP '{}' l{} ({:.2},{:.2})->({:.2},{:.2})",
                                    board.nets[i].name, sg.layer,
                                    sg.start.0, sg.start.1, sg.end.0, sg.end.1
                                );
                            }
                            r.segments.remove(sk);
                            // span bookkeeping is already stale for
                            // these nets — clear it (rip-whole
                            // semantics on later damage).
                            for (qs, ql) in r.path_spans.iter_mut() {
                                if *qs > sk {
                                    *qs -= 1;
                                } else if sk < *qs + *ql {
                                    *ql = ql.saturating_sub(1);
                                }
                            }
                            pruned += 1;
                        }
                        None => break,
                    }
                }
            }
        }
        if pruned > 0 {
            info!("final orphan sweep: {pruned} stranded fragment(s) pruned");

    probe_dangling_vias(&board, &final_routes, "post-5.996-sweep");
    // 5.997. POST-SWEEP PLANE RESCUE: the sweeps above run AFTER the
    // surface rescue, so a pad whose serving copper they trimmed
    // (amputation spurs, covered duplicates) ends the pipeline
    // unserved with no repair. One more idempotent rescue round —
    // pads with live copper skip — closes exactly that window.
    {
        let rescued = plane_surface_rescue(&board, &mut final_routes);
        if rescued > 0 {
            info!("post-sweep plane rescue: {rescued} pad(s) joined");
            // Its joins can leave short overshoot tails — trim them
            // with the same free-end rule before sign-off.
            let trimmed = pour_net_free_end_trim(&board, &mut final_routes);
            if trimmed > 0 {
                info!("post-rescue trim: {trimmed} spur segment(s)");
            }
        }
    }

    probe_dangling_vias(&board, &final_routes, "pre-5.9985");
    // 5.9985. REGION FOLLOWS THE NET'S OWN COPPER: a shaped pour
    // region comes from PIN clusters at 4.99 — before routing — so
    // trunks that wander between clusters end up outside it, and
    // copper the old blanket fill flooded over ships as orphan
    // groups (rigid C4: a 6mm staircase remnant dangled). Extend
    // the union with corridor rects along the net's own plane-layer
    // segments and vias so the fill serves every piece of its own
    // copper.
    {
        let bw = board.config.outline.width();
        let bh = board.config.outline.height();
        let m = board.config.edge_clearance_mm + 0.05;
        for ni in 0..board.nets.len() {
            if board.nets[ni].plane_region_rects.is_empty() {
                continue;
            }
            let Some(pl) = board.nets[ni].plane_layer else { continue };
            let contains = |rects: &[(f64, f64, f64, f64)], x: f64, y: f64| {
                rects
                    .iter()
                    .any(|&(x0, y0, x1, y1)| x >= x0 && x <= x1 && y >= y0 && y <= y1)
            };
            let clamp = |(x0, y0, x1, y1): (f64, f64, f64, f64)| {
                (x0.max(m), y0.max(m), x1.min(bw - m), y1.min(bh - m))
            };
            let mut add: Vec<(f64, f64, f64, f64)> = Vec::new();
            let mut cur: Option<(f64, f64, f64, f64)> = None;
            for sg in &final_routes[ni].segments {
                if sg.layer != pl {
                    continue;
                }
                let rects = &board.nets[ni].plane_region_rects;
                if contains(rects, sg.start.0, sg.start.1)
                    && contains(rects, sg.end.0, sg.end.1)
                {
                    continue;
                }
                let sb = (
                    sg.start.0.min(sg.end.0),
                    sg.start.1.min(sg.end.1),
                    sg.start.0.max(sg.end.0),
                    sg.start.1.max(sg.end.1),
                );
                cur = Some(match cur {
                    None => sb,
                    Some(c) => {
                        let merged = (
                            c.0.min(sb.0),
                            c.1.min(sb.1),
                            c.2.max(sb.2),
                            c.3.max(sb.3),
                        );
                        if merged.2 - merged.0 > 15.0 && merged.3 - merged.1 > 15.0 {
                            add.push(clamp((c.0 - 3.0, c.1 - 3.0, c.2 + 3.0, c.3 + 3.0)));
                            sb
                        } else {
                            merged
                        }
                    }
                });
            }
            if let Some(c) = cur {
                add.push(clamp((c.0 - 3.0, c.1 - 3.0, c.2 + 3.0, c.3 + 3.0)));
            }
            for v in &final_routes[ni].vias {
                if !contains(&board.nets[ni].plane_region_rects, v.x, v.y) {
                    add.push(clamp((v.x - 3.0, v.y - 3.0, v.x + 3.0, v.y + 3.0)));
                }
            }
            if !add.is_empty() {
                info!(
                    "pour region: '{}' +{} own-copper corridor rect(s)",
                    board.nets[ni].name,
                    add.len()
                );
                board.nets[ni].plane_region_rects.extend(add);
            }
        }
    }

    // 5.9987. Routed signal nets shipped in more than one piece —
    // rejoin them on KiCad's own grouping terms.
    {
        let repaired = signal_net_continuity_repair(&board, &mut final_routes);
        if repaired > 0 {
            info!("continuity repair: {repaired} split net(s) rejoined");
        }
    }

    // 5.99855. EXACT-DUPLICATE segments: bridge/joint re-commits can
    // stack a segment on itself; the pair then self-anchors through
    // the tip-trim's centerline test (each anchors the other at
    // distance zero) while KiCad sees one dangling stub (7
    // track_dangling at the mixer's seed-7 board). Pure redundancy —
    // remove.
    {
        let mut deduped = 0usize;
        for ni in 0..board.nets.len() {
            let r = &mut final_routes[ni];
            let mut k = 0usize;
            while k < r.segments.len() {
                let dup = (0..k).any(|j| {
                    let (a, b) = (&r.segments[j], &r.segments[k]);
                    a.layer == b.layer
                        && (a.width_mm - b.width_mm).abs() < 1e-6
                        && (((a.start.0 - b.start.0).hypot(a.start.1 - b.start.1) < 1e-6
                            && (a.end.0 - b.end.0).hypot(a.end.1 - b.end.1) < 1e-6)
                            || ((a.start.0 - b.end.0).hypot(a.start.1 - b.end.1) < 1e-6
                                && (a.end.0 - b.start.0).hypot(a.end.1 - b.start.1)
                                    < 1e-6))
                });
                if dup {
                    r.segments.remove(k);
                    for (qs, ql) in r.path_spans.iter_mut() {
                        if *qs > k {
                            *qs -= 1;
                        } else if k < *qs + *ql {
                            *ql = ql.saturating_sub(1);
                        }
                    }
                    deduped += 1;
                } else {
                    k += 1;
                }
            }
        }
        if deduped > 0 {
            info!("segment dedup: {deduped} exact duplicate(s) removed");
        }
    }

    // 5.9986. Orphan chains judged on the shipped copper — after the
    // region followed placement + own-copper corridors, whatever
    // still touches nothing is metal KiCad will group alone.
    {
        let swept = pour_orphan_chain_sweep(&board, &mut final_routes);
        if swept > 0 {
            info!("pour orphan sweep: {swept} segment(s) removed");
        }
    }

    let _t_bridge = std::time::Instant::now();
    // 5.998. EMISSION-MODEL ISLAND BRIDGE: detect islands on the SAME
    // copper the file will ship (emission_fill_polys mirrors the
    // writer — rings, spokes, backfill claims, anchored fracture) and
    // route a bridge from each stranded island's pad to the main
    // fill. The stitcher's optimistic raster missed exactly these
    // (the free-path C35/U20 ground pair), and patching ITS raster
    // regressed other paths.
    {
        // RIP-tier budget for the whole pass: victim rebuilds carry
        // RoutingGrid builds — unbounded, one free run took 4h.
        let mut rip_budget = 3usize;
        // ORDERING FIXPOINT: victim rebuilds and later bridges add
        // copper that can sever EARLIER bridges (measured: a bridged
        // pad's mid-path shipped as two dangling fragments). A
        // second round re-judges strandedness on the
        // post-everything emission model.
        for _round in 0..3 {
        let mut bridged = 0usize;
        let mut round_residual = 0usize;
        for ni in 0..board.nets.len() {
            let Some(polys) =
                output::kicad::emission_fill_polys(&board, &final_routes, ni)
            else {
                continue;
            };
            if polys.is_empty() {
                continue;
            }
            let net_id = board.nets[ni].id;
            let layer = board.nets[ni].plane_layer.unwrap_or(0);
            let pip = |pt: (f64, f64), poly: &[(f64, f64)]| -> bool {
                let (x, y) = pt;
                let mut inside = false;
                let mm = poly.len();
                for k in 0..mm {
                    let (x1, y1) = poly[k];
                    let (x2, y2) = poly[(k + 1) % mm];
                    if (y1 > y) != (y2 > y)
                        && x < (x2 - x1) * (y - y1) / (y2 - y1) + x1
                    {
                        inside = !inside;
                    }
                }
                inside
            };
            // Same-net pads with their fill poly (if any).
            let pour_side = if layer == 0 {
                BoardSide::Top
            } else {
                BoardSide::Bottom
            };
            let mut pad_polys: Vec<((f64, f64), usize)> = Vec::new();
            // SMD pads on the pour face that map to NO fill poly at
            // all — the fill retreated from their whole area, so
            // they hang on bare tracks (or nothing). The fill model
            // can't see track connectivity; these get a BFS over the
            // net's actual segments before any bridge fires.
            let mut stranded: Vec<(f64, f64)> = Vec::new();
            for comp in &board.components {
                let (co, sn) = (comp.theta.cos(), comp.theta.sin());
                for pin in &comp.pins {
                    if pin.net != Some(net_id) || pin.unplaced {
                        continue;
                    }
                    let Some(pad) = &pin.pad else { continue };
                    if pad.drill_mm.is_none() && comp.side != pour_side {
                        continue;
                    }
                    let gx = comp.x + pin.dx * co - pin.dy * sn;
                    let gy = comp.y + pin.dx * sn + pin.dy * co;
                    // Nearest poly containing OR within 1.3mm of a
                    // vertex (the pad sits inside its relief ring —
                    // its island's fill surrounds, not contains, the
                    // center).
                    let mut best: Option<(usize, f64)> = None;
                    for (pi2, poly) in polys.iter().enumerate() {
                        if pip((gx, gy), poly) {
                            best = Some((pi2, 0.0));
                            break;
                        }
                        // Full-resolution distance for SMALL polys
                        // (islands); big polys — the main fill —
                        // only get the containment test above (a pad
                        // near the main fill isn't stranded anyway).
                        // 1.6mm covers the relief ring + gap.
                        if poly.len() > 5000 {
                            continue;
                        }
                        let d = poly
                            .iter()
                            .map(|&(vx, vy)| (vx - gx).hypot(vy - gy))
                            .fold(f64::INFINITY, f64::min);
                        if d < 1.6 && best.map_or(true, |(_, bd)| d < bd) {
                            best = Some((pi2, d));
                        }
                    }
                    if let Some((pi2, _)) = best {
                        pad_polys.push(((gx, gy), pi2));
                    } else if pad.drill_mm.is_none() {
                        // THT pads reach the other face's fill through
                        // their own reliefs — only SMD pads on this
                        // face can be truly fill-orphaned.
                        stranded.push((gx, gy));
                    }
                }
            }
            if pad_polys.is_empty() && stranded.is_empty() {
                continue;
            }
            let mut counts = vec![0usize; polys.len()];
            for &(_, pi2) in &pad_polys {
                counts[pi2] += 1;
            }
            // Main poly: the one with the most pads (area proxy).
            let main = counts
                .iter()
                .enumerate()
                .max_by_key(|&(pi2, &c)| (c, polys[pi2].len()))
                .map(|(pi2, _)| pi2)
                .unwrap_or(0);
            let width = board
                .config
                .min_trace_width_mm
                .max(0.15)
                .min(board.nets[ni].required_trace_width_mm);
            for island in 0..polys.len() {
                if island == main || counts[island] == 0 {
                    continue;
                }
                let mut done = false;
                let mut served = false;
                for &(src, pi2) in &pad_polys {
                    if pi2 != island || done || served {
                        continue;
                    }
                    // An earlier ROUND already via-dropped this pad:
                    // the surface model still shows an island (the
                    // via serves it through the other face);
                    // re-dropping stacked coincident vias (oracle:
                    // holes_co_located). Served is NOT a new bridge —
                    // counting it as one made the ordering fixpoint
                    // run all its rounds on every trial.
                    if final_routes[ni]
                        .vias
                        .iter()
                        .any(|v| (v.x - src.0).hypot(v.y - src.1) <= 2.0)
                    {
                        served = true;
                        continue;
                    }
                    done = island_bridge_pad(
                        &board,
                        &mut final_routes,
                        ni,
                        src,
                        &polys[main],
                        layer,
                        width,
                        false,
                        &mut rip_budget,
                    );
                }
                if done {
                    bridged += 1;
                } else if !served {
                    round_residual += 1;
                    log::warn!(
                        "island bridge: '{}' island with {} pad(s) — no legal bridge (honest)",
                        board.nets[ni].name,
                        counts[island]
                    );
                }
            }
            // Fill-orphaned pads: SMD pads the emission model maps to
            // NO poly at all. The fill model can't see tracks, so a
            // pad here may still be grounded through routed copper —
            // BFS the net's actual segments first, and bridge only
            // chains that provably reach nothing (re-checked per pad:
            // an earlier bridge grounds the rest of its track group).
            if !stranded.is_empty() && board.nets[ni].plane_region.is_none() {
                let mut grounded_pads: Vec<(f64, f64)> = pad_polys
                    .iter()
                    .map(|&(p, _)| p)
                    .collect();
                for comp in &board.components {
                    let (co, sn) = (comp.theta.cos(), comp.theta.sin());
                    for pin in &comp.pins {
                        if pin.net != Some(net_id) || pin.unplaced {
                            continue;
                        }
                        let Some(pad) = &pin.pad else { continue };
                        if pad.drill_mm.is_some() {
                            grounded_pads.push((
                                comp.x + pin.dx * co - pin.dy * sn,
                                comp.y + pin.dx * sn + pin.dy * co,
                            ));
                        }
                    }
                }
                for &src in &stranded {
                    let Some(chain) = pad_track_grounded(
                        &final_routes[ni],
                        src,
                        &polys,
                        layer,
                        &grounded_pads,
                    ) else {
                        continue;
                    };
                    // The pad itself may be walled in — try every
                    // point of its copper chain, nearest-to-fill
                    // first.
                    let dist_to_fill = |p: (f64, f64)| -> f64 {
                        polys[main]
                            .iter()
                            .step_by(4)
                            .map(|&(vx, vy)| (vx - p.0).hypot(vy - p.1))
                            .fold(f64::INFINITY, f64::min)
                    };
                    let mut sources: Vec<(f64, f64)> = std::iter::once(src)
                        .chain(chain.into_iter())
                        .collect();
                    sources.sort_by(|a, b| {
                        dist_to_fill(*a)
                            .partial_cmp(&dist_to_fill(*b))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    sources.truncate(8);
                    let mut done = false;
                    if final_routes[ni]
                        .vias
                        .iter()
                        .any(|v| (v.x - src.0).hypot(v.y - src.1) <= 2.0)
                    {
                        continue; // served by an earlier round's via
                    }
                    for &s2 in &sources {
                        if island_bridge_pad(
                            &board,
                            &mut final_routes,
                            ni,
                            s2,
                            &polys[main],
                            layer,
                            width,
                            true,
                            &mut rip_budget,
                        ) {
                            bridged += 1;
                            done = true;
                            break;
                        }
                    }
                    if !done {
                        round_residual += 1;
                        log::warn!(
                            "island bridge: '{}' fill-orphaned pad ({:.1},{:.1}) — no legal bridge (honest)",
                            board.nets[ni].name, src.0, src.1
                        );
                    }
                }
            }
        }
        if bridged > 0 {
            let trimmed = pour_net_free_end_trim(&board, &mut final_routes);
            if trimmed > 0 {
                info!("post-bridge trim: {trimmed} spur segment(s)");
            }
            // A RIP-bridged victim was rebuilt AFTER the continuity
            // pass ran — give rebuilt nets the same split check.
            let repaired = signal_net_continuity_repair(&board, &mut final_routes);
            if repaired > 0 {
                info!("post-bridge continuity repair: {repaired} split net(s) rejoined");
            }
            // Bridges land on the CURRENT emission model; the final
            // fill can still withdraw (fixpoint). Mid-bridge pieces
            // whose fill left are orphan copper — and GND never gets
            // the regioned free-end trim. Sweep on the post-bridge
            // model.
            let swept = pour_orphan_chain_sweep(&board, &mut final_routes);
            if swept > 0 {
                info!("post-bridge orphan sweep: {swept} segment(s) removed");
            }
        }
        if std::env::var("BHDL_PNR_TIMING").is_ok() {
            eprintln!(
                "[timing] bridge round done at {:.1}s (emission calls {}, {}ms cum)",
                _t_bridge.elapsed().as_secs_f64(),
                output::kicad::EMISSION_CALLS.load(std::sync::atomic::Ordering::Relaxed),
                output::kicad::EMISSION_MS.load(std::sync::atomic::Ordering::Relaxed),
            );
            {
                use std::sync::atomic::Ordering::Relaxed as R;
                eprintln!(
                    "[timing]   fill phases: grid {}ms, spoke {}ms (of which morph {}ms), label+trace {}ms, cells {}M",
                    output::kicad::FILL_MS_GRID.load(R),
                    output::kicad::FILL_MS_SPOKE.load(R),
                    output::kicad::FILL_MS_MORPH.load(R),
                    output::kicad::FILL_MS_LABEL.load(R),
                    output::kicad::FILL_CELLS.load(R) / 1_000_000,
                );
            }
        }
        pour_bridge_residual = round_residual;
        // Re-scan whenever ANY bridge landed: this round's own
        // mutations (later bridges, victim rebuilds, sweeps) can
        // sever an EARLIER bridge — the residual count above was
        // taken before them (measured: a "bridged" pad shipped
        // stranded with its stub, invisible to the single-round
        // gate).
        if bridged == 0 {
            break;
        }
        }
    }
        }
        for i in 0..board.nets.len() {
            if std::env::var("BHDL_PNR_DEBUG_NETS")
                .map(|v| board.nets[i].name.contains(&v))
                .unwrap_or(false)
            {
                let r = &final_routes[i];
                for (si, &(ps, pl)) in r.path_spans.iter().enumerate() {
                    log::info!("[sweep2] '{}' span {si} pl={pl}", board.nets[i].name);
                    for sg in &r.segments[ps..ps + pl] {
                        log::info!(
                            "[sweep2]   l{} ({:.4},{:.4})->({:.4},{:.4})",
                            sg.layer, sg.start.0, sg.start.1, sg.end.0, sg.end.1
                        );
                    }
                }
                for v in &r.vias {
                    log::info!("[sweep2]   via ({:.4},{:.4})", v.x, v.y);
                }
            }
        }
    }



    // 5.997. SINGLE-LAYER VIA SWEEP: rips and dangle-trims can take a
    // via's far-layer copper without taking the via — it then bridges
    // nothing and ships as the oracle's via_dangling (buck s99: an
    // F.Cu track junction sat on a via whose B.Cu leg was long gone).
    // A via touching copper on <2 layers is electrically inert, so
    // removal can never disconnect. Plane nets are EXCLUDED: their
    // drops connect to fill copper the segment scan can't see.
    {
        let via_r = board.layer_stack.via.pad_mm / 2.0;
        let last_cu = board.layer_stack.layers.len().saturating_sub(1);
        let mut trimmed = 0usize;
        for i in 0..board.nets.len() {
            if board.nets[i].plane_layer.is_some() || final_routes[i].is_empty() {
                continue;
            }
            // Same-net pads: THT anchors every layer, SMD its surface.
            let mut pads: Vec<(f64, f64, f64, f64, Option<usize>)> = Vec::new();
            for comp in &board.components {
                let cos_t = comp.theta.cos();
                let sin_t = comp.theta.sin();
                let quarter = ((comp.theta / std::f64::consts::FRAC_PI_2).round()
                    as i64)
                    .rem_euclid(2);
                let surf = match comp.side {
                    BoardSide::Top => 0usize,
                    BoardSide::Bottom => last_cu,
                };
                for pin in &comp.pins {
                    if pin.net != Some(board.nets[i].id) || pin.unplaced {
                        continue;
                    }
                    let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
                    let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;
                    let (pw, ph, tht) = match &pin.pad {
                        Some(p) => (p.width_mm, p.height_mm, p.drill_mm.is_some()),
                        None => (0.5, 0.5, false),
                    };
                    let (pw, ph) = if quarter == 1 { (ph, pw) } else { (pw, ph) };
                    pads.push((
                        gx,
                        gy,
                        pw / 2.0,
                        ph / 2.0,
                        if tht { None } else { Some(surf) },
                    ));
                }
            }
            let segs = final_routes[i].segments.clone();
            final_routes[i].vias.retain(|v| {
                let mut layers: std::collections::BTreeSet<usize> =
                    std::collections::BTreeSet::new();
                for sg in &segs {
                    for e in [sg.start, sg.end] {
                        if (e.0 - v.x).hypot(e.1 - v.y) <= via_r {
                            layers.insert(sg.layer);
                        }
                    }
                }
                for &(gx, gy, hx, hy, layer) in &pads {
                    if (v.x - gx).abs() < hx + via_r && (v.y - gy).abs() < hy + via_r
                    {
                        match layer {
                            None => return true, // THT barrel: all layers
                            Some(l) => {
                                layers.insert(l);
                            }
                        }
                    }
                }
                if layers.len() >= 2 {
                    true
                } else {
                    trimmed += 1;
                    false
                }
            });
        }
        if trimmed > 0 {
            info!("single-layer via sweep: {trimmed} inert via(s) removed");
        }
    }

    via_anchor_check(&board, &final_routes, "final");

    let connected_sinks = pathfinder::count_connected_sinks(&board, &final_routes);

    // ── Constraint sign-off (constraint synthesis v1) ──
    // Constraints promise, routed copper delivers: a `target vs
    // achieved` row per net/signal constraint, honest FAILs included.
    // Report-only — an unmet electrical constraint is a design review
    // item, not illegal copper.
    {
        use crate::constraint::Constraint;
        let idx_of = |nid: NetId| board.nets.iter().position(|n| n.id == nid);
        let mut rows: Vec<String> = Vec::new();
        // Worst matched-group delay spread — feeds the DDR bin
        // UI-context row (report-only) when the board carries an
        // SDRAM entity with a speed bin.
        let mut worst_group_spread_ps: Option<f64> = None;
        for c in &board.constraints {
            match c {
                Constraint::DiffPair { p_net, n_net, length_match_mm, length_match_ps, spacing_mm, .. } => {
                    let (Some(pi), Some(ni)) = (idx_of(*p_net), idx_of(*n_net)) else {
                        continue;
                    };
                    let lp = routing::measure::net_routed_length(&final_routes[pi]);
                    let ln = routing::measure::net_routed_length(&final_routes[ni]);
                    let skew = (lp - ln).abs();
                    let gap = *spacing_mm as f64 + 3.0 * 0.3;
                    let coupled = routing::measure::coupled_fraction(
                        &final_routes[pi],
                        &final_routes[ni],
                        gap,
                    );
                    // The pair-lowering default is 0.1mm; a user-declared
                    // `length_match` on the same two nets arrives as a
                    // LengthMatchGroup — that tolerance is the designer's
                    // word and wins. A budget DECLARED IN TIME grades
                    // routed DELAY (per-layer stackup velocity): a
                    // millimeter of outer microstrip is not a millimeter
                    // of inner stripline.
                    let covering = board.constraints.iter().find_map(|c2| match c2 {
                        Constraint::LengthMatchGroup {
                            nets, tolerance_mm, tolerance_ps, ..
                        } if nets.contains(p_net) && nets.contains(n_net) => {
                            Some((*tolerance_mm, *tolerance_ps))
                        }
                        _ => None,
                    });
                    // A DERIVED time budget on the pair itself (IBIS
                    // measured edge → t_rise/10) grades delay too,
                    // unless a declared LengthMatchGroup covers it.
                    let effective_ps = covering
                        .and_then(|(_, ps)| ps)
                        .or(*length_match_ps);
                    match (covering, effective_ps) {
                        (_, Some(limit_ps)) => {
                            let dp = routing::measure::net_routed_delay_ps(
                                &final_routes[pi],
                                &board.layer_stack,
                            );
                            let dn = routing::measure::net_routed_delay_ps(
                                &final_routes[ni],
                                &board.layer_stack,
                            );
                            let skew_ps = (dp - dn).abs();
                            let ok = skew_ps <= limit_ps as f64 && lp > 0.0 && ln > 0.0;
                            rows.push(format!(
                                "diff-pair {} / {}: P={dp:.1}ps N={dn:.1}ps skew={skew_ps:.2}ps (limit {limit_ps}ps) coupled={:.0}% — {}",
                                board.nets[pi].name,
                                board.nets[ni].name,
                                coupled * 100.0,
                                if ok { "PASS" } else { "FAIL" }
                            ));
                        }
                        _ => {
                            let limit =
                                covering.map(|(mm, _)| mm).unwrap_or(*length_match_mm);
                            let ok = skew <= limit as f64 && lp > 0.0 && ln > 0.0;
                            rows.push(format!(
                                "diff-pair {} / {}: P={lp:.2}mm N={ln:.2}mm skew={skew:.3}mm (limit {limit}mm) coupled={:.0}% — {}",
                                board.nets[pi].name,
                                board.nets[ni].name,
                                coupled * 100.0,
                                if ok { "PASS" } else { "FAIL" }
                            ));
                        }
                    }
                }
                Constraint::LengthMatchGroup { nets, tolerance_mm, tolerance_ps, .. } => {
                    let idxs: Vec<usize> =
                        nets.iter().filter_map(|nid| idx_of(*nid)).collect();
                    if idxs.len() < 2 {
                        continue;
                    }
                    if let Some(limit_ps) = tolerance_ps {
                        // Declared in TIME: grade routed delay.
                        let delays: Vec<f64> = idxs
                            .iter()
                            .map(|&i| {
                                routing::measure::net_routed_delay_ps(
                                    &final_routes[i],
                                    &board.layer_stack,
                                )
                            })
                            .collect();
                        let min = delays.iter().cloned().fold(f64::INFINITY, f64::min);
                        let max = delays.iter().cloned().fold(0.0_f64, f64::max);
                        let spread = max - min;
                        let ok = spread <= *limit_ps as f64 && min > 0.0;
                        if min > 0.0 {
                            worst_group_spread_ps = Some(
                                worst_group_spread_ps.unwrap_or(0.0).max(spread),
                            );
                        }
                        rows.push(format!(
                            "delay-match ({} nets): spread={spread:.2}ps (tolerance {limit_ps}ps) — {}",
                            idxs.len(),
                            if ok { "PASS" } else { "FAIL" }
                        ));
                    } else {
                        let lens: Vec<f64> = idxs
                            .iter()
                            .map(|&i| {
                                routing::measure::net_routed_length(&final_routes[i])
                            })
                            .collect();
                        let min = lens.iter().cloned().fold(f64::INFINITY, f64::min);
                        let max = lens.iter().cloned().fold(0.0_f64, f64::max);
                        let spread = max - min;
                        let ok = spread <= *tolerance_mm as f64 && min > 0.0;
                        // mm-declared groups still contribute delay
                        // spread to the DDR bin context row (the UI is
                        // a time; grade the routed delay-domain spread
                        // through the stackup velocities).
                        if board.ddr_bin.is_some() && min > 0.0 {
                            let delays: Vec<f64> = idxs
                                .iter()
                                .map(|&i| {
                                    routing::measure::net_routed_delay_ps(
                                        &final_routes[i],
                                        &board.layer_stack,
                                    )
                                })
                                .collect();
                            let dmin =
                                delays.iter().cloned().fold(f64::INFINITY, f64::min);
                            let dmax = delays.iter().cloned().fold(0.0_f64, f64::max);
                            if dmin > 0.0 {
                                worst_group_spread_ps = Some(
                                    worst_group_spread_ps
                                        .unwrap_or(0.0)
                                        .max(dmax - dmin),
                                );
                            }
                        }
                        rows.push(format!(
                            "length-match ({} nets): spread={spread:.3}mm (tolerance {tolerance_mm}mm) — {}",
                            idxs.len(),
                            if ok { "PASS" } else { "FAIL" }
                        ));
                    }
                }
                Constraint::NoiseBudget { net, max_mv, .. } => {
                    let Some(i) = idx_of(*net) else { continue };
                    match routing::extract::crosstalk_worst_mv(&board, &final_routes, i) {
                        Some((mv, ai, mm, _, _)) => {
                            let ok = mv <= *max_mv as f64;
                            rows.push(format!(
                                "noise budget {}: measured worst {mv:.1}mV (vs {}, {mm:.1}mm coupled) budget {max_mv}mV — {}",
                                board.nets[i].name,
                                board.nets[ai].name,
                                if ok { "PASS" } else { "FAIL" }
                            ));
                        }
                        // None means either NOTHING was measured (no
                        // solved edge → ungradable, absence ledger) or
                        // the copper genuinely sits beyond coupling
                        // reach (5h) of every measured aggressor —
                        // which is a PASS earned by separation, not a
                        // data gap.
                        None if board.nets.iter().any(|n| n.edge_swing_v.is_some()) => {
                            rows.push(format!(
                                "noise budget {}: measured aggressor edge present, no couple within coupling reach (5·h) — PASS by separation (budget {max_mv}mV)",
                                board.nets[i].name
                            ))
                        }
                        None => rows.push(format!(
                            "noise budget {}: declared {max_mv}mV but no measured aggressor edge — ungradable, see absence ledger",
                            board.nets[i].name
                        )),
                    }
                }
                Constraint::RailDrop { net, max_mv, .. } => {
                    let Some(i) = idx_of(*net) else { continue };
                    match routing::extract::ir_drop_mv_of(&board, &final_routes, i) {
                        Some(mv) => {
                            let ok = mv <= *max_mv as f64;
                            rows.push(format!(
                                "rail drop {}: measured {mv:.1}mV budget {max_mv}mV — {}",
                                board.nets[i].name,
                                if ok { "PASS" } else { "FAIL" }
                            ));
                        }
                        None => rows.push(format!(
                            "rail drop {}: declared {max_mv}mV but no solved current (or plane rail) — ungradable, see absence ledger",
                            board.nets[i].name
                        )),
                    }
                }
                Constraint::Impedance { net, target_ohms, tolerance_pct, .. } => {
                    let Some(i) = idx_of(*net) else { continue };
                    let min_w = final_routes[i]
                        .segments
                        .iter()
                        .map(|sg| sg.width_mm)
                        .fold(f64::INFINITY, f64::min);
                    if !min_w.is_finite() {
                        continue;
                    }
                    // P3: grade the ROUTED copper's impedance — every
                    // segment's Z0 from its own layer's stackup model
                    // (microstrip outer / stripline inner), worst
                    // deviation vs target. A diff-net member's
                    // single-ended target is Zdiff/2, matching the
                    // width-floor convention.
                    // A DIFF member grades on the COUPLED model at the
                    // pair's designed gap (grading Zdiff/2 single-ended
                    // flags the coupled design point as a false FAIL).
                    let pair_gap = board.constraints.iter().find_map(|c2| match c2 {
                        Constraint::DiffPair { p_net, n_net, spacing_mm, .. }
                            if p_net == net || n_net == net =>
                        {
                            Some(*spacing_mm as f64)
                        }
                        _ => None,
                    });
                    let z_target = *target_ohms as f64;
                    let mut worst: Option<(f64, usize)> = None; // (z, layer)
                    for sg in &final_routes[i].segments {
                        let z = match pair_gap {
                            Some(gap) => routing::measure::layer_zdiff(
                                &board.layer_stack,
                                sg.layer,
                                sg.width_mm,
                                gap,
                            ),
                            None => routing::measure::layer_z0(
                                &board.layer_stack,
                                sg.layer,
                                sg.width_mm,
                            ),
                        };
                        if let Some(z) = z {
                            if worst
                                .map_or(true, |(wz, _)| (z - z_target).abs() > (wz - z_target).abs())
                            {
                                worst = Some((z, sg.layer));
                            }
                        }
                    }
                    match worst {
                        Some((z, l)) => {
                            let dev_pct = (z - z_target).abs() / z_target * 100.0;
                            let ok = dev_pct <= *tolerance_pct as f64;
                            let kind = if pair_gap.is_some() { "Zdiff" } else { "Z0" };
                            rows.push(format!(
                                "impedance {}: target {target_ohms}Ω, routed worst {kind} {z:.1}Ω on layer {l} ({dev_pct:.1}% dev, tol {tolerance_pct}%) min width {min_w:.2}mm — {}",
                                board.nets[i].name,
                                if ok { "PASS" } else { "FAIL" }
                            ));
                            // P3 derived-rules table: the width this
                            // stackup demands for the target on EVERY
                            // signal layer (microstrip / stripline per
                            // dispatch) — the provenance a reviewer
                            // checks the routed geometry against.
                            let z_w = if pair_gap.is_some() {
                                z_target / 2.0
                            } else {
                                z_target
                            };
                            let cells: Vec<String> = board
                                .layer_stack
                                .signal_layer_indices()
                                .into_iter()
                                .map(|sl| {
                                    match routing::measure::layer_width_for(
                                        &board.layer_stack,
                                        sl,
                                        z_w,
                                    ) {
                                        Some(w) => format!("L{sl}={w:.2}mm"),
                                        None => format!("L{sl}=unreachable"),
                                    }
                                })
                                .collect();
                            rows.push(format!(
                                "impedance {} width table (Z0 {z_w:.0}Ω): {}",
                                board.nets[i].name,
                                cells.join(" ")
                            ));
                        }
                        None => rows.push(format!(
                            "impedance {}: target {target_ohms}Ω, routed min width {min_w:.2}mm (floor {:.2}mm) — no stackup dielectrics, Z0 ungraded",
                            board.nets[i].name, board.nets[i].required_trace_width_mm
                        )),
                    }
                }
                Constraint::Topology { net, kind, stub_max_mm, .. } => {
                    let Some(i) = idx_of(*net) else { continue };
                    if final_routes[i].is_empty() {
                        continue;
                    }
                    // Stub grading (fly-by budgets): measure every pin's
                    // dead-end branch off the trunk.
                    let stub_note = if let Some(limit) = stub_max_mm {
                        let comp_idx: crate::det::HashMap<ComponentId, usize> =
                            board
                                .components
                                .iter()
                                .enumerate()
                                .map(|(k, c)| (c.id, k))
                                .collect();
                        let pads: Vec<(f64, f64, f64)> = board.nets[i]
                            .pins
                            .iter()
                            .filter_map(|&(cid, pid)| {
                                let comp = &board.components[*comp_idx.get(&cid)?];
                                let pin = comp.pins.iter().find(|p| p.pin_id == pid)?;
                                let cos_t = comp.theta.cos();
                                let sin_t = comp.theta.sin();
                                Some((
                                    comp.x + pin.dx * cos_t - pin.dy * sin_t,
                                    comp.y + pin.dx * sin_t + pin.dy * cos_t,
                                    pin.pad
                                        .as_ref()
                                        .map(|p| p.width_mm.min(p.height_mm) / 2.0)
                                        .unwrap_or(0.25),
                                ))
                            })
                            .collect();
                        let max_stub = pads
                            .iter()
                            .enumerate()
                            .map(|(k, &(px, py, h))| {
                                let others: Vec<(f64, f64, f64)> = pads
                                    .iter()
                                    .enumerate()
                                    .filter(|&(j, _)| j != k)
                                    .map(|(_, &p)| p)
                                    .collect();
                                routing::measure::pin_stub_length(
                                    &final_routes[i],
                                    (px, py),
                                    h,
                                    &others,
                                )
                            })
                            .fold(0.0_f64, f64::max);
                        let ok = max_stub <= *limit as f64;
                        format!(
                            ", max stub {max_stub:.2}mm (limit {limit}mm) — {}",
                            if ok { "PASS" } else { "FAIL" }
                        )
                    } else {
                        String::new()
                    };
                    rows.push(format!(
                        "topology {:?} on '{}': constructed ({} span(s)){stub_note}",
                        kind,
                        board.nets[i].name,
                        final_routes[i].path_spans.len()
                    ));
                }
                Constraint::LayerRule { net, bind, .. } => {
                    let Some(i) = idx_of(*net) else { continue };
                    let mut used: Vec<usize> = final_routes[i]
                        .segments
                        .iter()
                        .map(|sg| sg.layer)
                        .collect();
                    used.sort_unstable();
                    used.dedup();
                    let ok = board.nets[i]
                        .allowed_layers
                        .as_ref()
                        .map(|a| used.iter().all(|l| a.contains(l)))
                        .unwrap_or(true);
                    rows.push(format!(
                        "layer rule {:?} on '{}': layers used {:?} — {}",
                        bind,
                        board.nets[i].name,
                        used,
                        if ok { "PASS" } else { "FAIL" }
                    ));
                }
                _ => {}
            }
        }
        // P4 — POST-ROUTE EXTRACTION: what the routed copper DOES,
        // measured from its geometry. Crosstalk couples, IR drop on
        // routed power traces, return-path void crossings — the
        // re-simulation inputs and the reviewer's noise ledger.
        rows.extend(routing::extract::crosstalk_rows(&board, &final_routes, 5));
        rows.extend(routing::extract::ir_rows(&board, &final_routes));
        rows.extend(routing::extract::return_path_rows(&board, &final_routes, 5));
        // DDR speed-bin CONTEXT: the measured matched-group spread,
        // restated as a fraction of the bin's unit interval
        // (UI = tCK/2 — DDR transfers on both clock edges). Report-
        // only: the declared constraint tolerances above keep gating;
        // this row just anchors "spread=Xps" to the interface the
        // silicon actually runs (Micron Table 1, carried on the
        // SDRAM entity).
        if let (Some((bin, tck_ns)), Some(spread_ps)) =
            (&board.ddr_bin, worst_group_spread_ps)
        {
            let ui_ps = tck_ns * 1000.0 / 2.0;
            rows.push(format!(
                "DDR4 bin {bin} (tCK {tck_ns}ns, UI {ui_ps:.0}ps): worst matched-group spread {spread_ps:.1}ps = {:.2}% of UI (context only — declared tolerances gate)",
                spread_ps / ui_ps * 100.0
            ));
        }
        // Both ends of a link can carry the same constraint (each
        // instance's module holds its side's attrs, resolving to the
        // same nets) — one row per distinct fact.
        let mut seen = std::collections::BTreeSet::new();
        rows.retain(|r| seen.insert(r.clone()));
        if !rows.is_empty() {
            info!("── Constraint sign-off ──");
            for r in &rows {
                info!("  {r}");
            }
            // Also to stdout: the sign-off is a deliverable, like the
            // supply report.
            println!("\n  Constraint sign-off:");
            for r in &rows {
                println!("    {r}");
            }
        }
    }

    {
        let pruned = signal_free_end_trim(&board, &mut final_routes);
        if pruned > 0 {
            info!("post-everything free-end trim: {pruned} segment(s) removed");
        }
    }
    {
        // AFTER the trim, not before: the trim removes segments and
        // the fill reshapes around the removal. And ITERATED to a
        // fixpoint, because the pass's own joins reshape the fill
        // too — each net's verdicts are judged against a polys
        // snapshot, and a join added later (same net or a neighbour)
        // moves copper and voids alike. Measured: U2.V_MINUS on the
        // jack board's seed 99 probed fill_served=true inside the
        // pass and sat in NO fragment on the shipped board. A pass
        // that mutates what it judges must re-judge until nothing
        // moves; joins strictly decrease the unserved set, so this
        // terminates (3 rounds is a backstop, not a tuning knob).
        let st = pour_severed_component_stitch(&board, &mut final_routes);
        if st > 0 {
            info!("severed-component stitch: {st} via(s)");
        }
        for _ in 0..3 {
            let n = pour_unserved_pad_route(&board, &mut final_routes);
            if n == 0 {
                break;
            }
            info!("unserved-pad route: {n} pad(s) joined");
            trace_near("after-unserved", &board, &final_routes);
        }
    }
    {
        // RE-JUDGE THE FINAL COPPER. The DRC above ran ~2300 lines and
        // half a dozen repair rungs ago; everything the trim, stitch
        // and unserved-route fixpoint committed since was invisible to
        // the verdict — the engine reported "DRC violations: 0" on a
        // board KiCad graded with 3 track shorts (0.3mm vbias stubs
        // across a ch1_b_out track, mixer true-geometry seed 42). A
        // verdict on copper you then keep editing is not a verdict;
        // and the trial currency must price the late rungs' damage or
        // dominance will keep picking winners that ship it.
        {
            let j = pour_split_group_join(&board, &mut final_routes);
            if j > 0 {
                info!("split-group join: {j} group(s) joined");
            }
            trace_near("after-split-join", &board, &final_routes);
        }
        // DANGLING-VIA PRUNE against the emitted fills: a via whose
        // barrel meets same-net copper on at most ONE layer connects
        // nothing across layers — it is dead drill (measured: a GND
        // via at (45.8,43.1) whose far end landed in a void shipped
        // as seed 99's via_dangling). Removing such a via cannot
        // disconnect anything, by construction: whatever touches it
        // on the one live layer stays touching itself. Judged, like
        // everything now, on the copper actually being shipped.
        {
            let (_, fills) =
                output::kicad::export_kicad_pcb_with_fills(&board, &final_routes);
            let via_r = board.layer_stack.via.pad_mm / 2.0;
            let pip = |poly: &[(f64, f64)], x: f64, y: f64| -> bool {
                let mut ins = false;
                let m = poly.len();
                for k in 0..m {
                    let (x1, y1) = poly[k];
                    let (x2, y2) = poly[(k + 1) % m];
                    if (y1 > y) != (y2 > y)
                        && x < (x2 - x1) * (y - y1) / (y2 - y1) + x1
                    {
                        ins = !ins;
                    }
                }
                ins
            };
            for ni in 0..board.nets.len() {
                if final_routes[ni].vias.is_empty() {
                    continue;
                }
                let net_id = board.nets[ni].id;
                let keep: Vec<bool> = final_routes[ni]
                    .vias
                    .iter()
                    .map(|v| {
                        let mut layers: crate::det::HashSet<usize> = Default::default();
                        for sg in &final_routes[ni].segments {
                            if (sg.start.0 - v.x).hypot(sg.start.1 - v.y)
                                <= via_r + sg.width_mm / 2.0
                                || (sg.end.0 - v.x).hypot(sg.end.1 - v.y)
                                    <= via_r + sg.width_mm / 2.0
                            {
                                layers.insert(sg.layer);
                            }
                        }
                        for z in fills.zones.iter().filter(|z| z.net_id == net_id) {
                            if layers.contains(&z.layer) {
                                continue;
                            }
                            if z.polys.iter().any(|p| pip(p, v.x, v.y)) {
                                layers.insert(z.layer);
                            }
                        }
                        layers.len() >= 2
                    })
                    .collect();
                let dropped = keep.iter().filter(|&&k| !k).count();
                if dropped > 0 {
                    info!(
                        "dangling-via prune: '{}' {} via(s) meet copper on <=1 layer — removed",
                        board.nets[ni].name, dropped
                    );
                    let mut it = keep.iter();
                    final_routes[ni].vias.retain(|_| *it.next().unwrap_or(&true));
                }
            }
        }
        trace_near("after-via-prune", &board, &final_routes);
        // SHORTING-STUB AMPUTATION before the verdict: a cross-net
        // SHORT whose party is a tiny stub is repair debris — cut the
        // stub, then let the unserved-pad pass re-serve anything that
        // just lost its copper (measured: three 0.3mm vbias stubs
        // across a ch1_b_out track — no logged rung near them; they
        // predate the late joins and nothing downstream re-judged
        // them until now).
        for _round in 0..3 {
            let conflicts = legalization::copper_conflicts(&board, &final_routes);
            let mut cut: Vec<(usize, usize)> = Vec::new();
            for &(na, sa, nb, sb, _d, short) in &conflicts {
                if !short {
                    continue;
                }
                for (ni, si) in [(na, sa), (nb, sb)] {
                    let sg = &final_routes[ni].segments[si];
                    let len = (sg.end.0 - sg.start.0).hypot(sg.end.1 - sg.start.1);
                    if len <= 0.6 && !cut.contains(&(ni, si)) {
                        cut.push((ni, si));
                    }
                }
            }
            if cut.is_empty() {
                break;
            }
            cut.sort_unstable_by(|a, b| b.cmp(a));
            for (ni, si) in cut {
                let sg = final_routes[ni].segments[si].clone();
                info!(
                    "shorting-stub amputation: '{}' {:.2}mm stub at ({:.2},{:.2}) cut",
                    board.nets[ni].name,
                    (sg.end.0 - sg.start.0).hypot(sg.end.1 - sg.start.1),
                    sg.start.0,
                    sg.start.1
                );
                let r = &mut final_routes[ni];
                r.segments.remove(si);
                for (qs, ql) in r.path_spans.iter_mut() {
                    if *qs > si {
                        *qs -= 1;
                    } else if si < *qs + *ql {
                        *ql = ql.saturating_sub(1);
                    }
                }
            }
            // Anything the amputation stranded gets the honest ladder.
            for _ in 0..2 {
                if pour_unserved_pad_route(&board, &mut final_routes) == 0 {
                    break;
                }
            }
        }
        {
            let c = signal_seam_close(&board, &mut final_routes);
            if c > 0 {
                info!("seam close: {c} sliver(s) added");
            }
        }
        trace_near("after-seam-close", &board, &final_routes);
        let recheck = legalization::check_drc(&board, &final_routes);
        if recheck.len() != drc_violations.len() {
            info!(
                "final-copper DRC recheck: {} -> {} violation(s) after late repair rungs",
                drc_violations.len(),
                recheck.len()
            );
        }
        drc_violations = recheck;
    }
    if let Ok(dir) = std::env::var("BHDL_PNR_DUMP_FINAL_MIRROR") {
        use std::io::Write;
        for ni in 0..board.nets.len() {
            if board.nets[ni].plane_layer.is_none() {
                continue;
            }
            let Some(polys) = output::kicad::emission_fill_polys(&board, &final_routes, ni)
            else {
                continue;
            };
            if let Ok(mut f) = std::fs::File::create(format!(
                "{dir}/final_mirror_{}.txt",
                board.nets[ni].name.replace('/', "_")
            )) {
                for p in &polys {
                    for (x, y) in p {
                        let _ = writeln!(f, "{x} {y}");
                    }
                    let _ = writeln!(f, "---");
                }
            }
        }
    }
    probe_dangling_vias(&board, &final_routes, "pre-metrics");
    let pour_defects = pour_defect_count(&board, &final_routes) + pour_bridge_residual;
    if pour_defects > 0 {
        info!("pour defects (trial currency): {pour_defects}");
    }

    let (detour_p50, detour_p90, detour_nets) = detour_stats(&board, &final_routes);
    if detour_nets > 0 {
        let verdict = if detour_p50 <= 1.3 && detour_p90 <= 1.9 {
            "PASS"
        } else {
            "WATCH"
        };
        info!(
            "detour envelope: p50={detour_p50:.2} p90={detour_p90:.2} over {detour_nets} net(s) — {verdict} (pro bar 1.30/1.90, 14-board demo survey)"
        );
    }

    Ok(PnrResult {
        board,
        routes: final_routes,
        metrics: PnrMetrics {
            connected_sinks,
            hpwl_mm: hpwl,
            total_routed_length_mm: total_length,
            via_count: total_vias,
            max_congestion: final_grid.max_overflow() as f64,
            routability_pct: if total_nets > 0 {
                routability
            } else {
                100.0
            },
            iterations: config.max_iterations,
            pour_defects,
            detour_p50,
            detour_p90,
            detour_nets,
        },
        drc_violations,
    })
}

/// DETOUR ENVELOPE: per-net routed-length / pad-MST ratio percentiles
/// over the final copper. The pad MST (Euclidean, Prim) is the
/// placement-implied lower bound; the ratio measures pure routing
/// quality independent of scale — the demo survey found professionals
/// hold p50 1.05-1.28 / p90 1.36-1.85 on EVERY board from 15fp to
/// 1508fp/12-layer. Read-only.
fn detour_stats(board: &Board, final_routes: &[Route]) -> (f64, f64, usize) {
    let comp_idx: crate::det::HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();
    let mut ratios: Vec<f64> = Vec::new();
    for (i, net) in board.nets.iter().enumerate() {
        if net.pins.len() < 2 || net.plane_layer.is_some() {
            continue;
        }
        let routed: f64 = final_routes
            .get(i)
            .map(|r| {
                r.segments
                    .iter()
                    .map(|s| (s.end.0 - s.start.0).hypot(s.end.1 - s.start.1))
                    .sum()
            })
            .unwrap_or(0.0);
        if routed < 0.5 {
            continue;
        }
        let mut pts: Vec<(f64, f64)> = Vec::new();
        for &(cid, pid) in &net.pins {
            let Some(&ci) = comp_idx.get(&cid) else { continue };
            let comp = &board.components[ci];
            let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pid) else { continue };
            if pin.unplaced {
                continue;
            }
            let (c, s) = (comp.theta.cos(), comp.theta.sin());
            pts.push((
                comp.x + pin.dx * c - pin.dy * s,
                comp.y + pin.dx * s + pin.dy * c,
            ));
        }
        // Big fanout nets (headers, rails) get O(n^2) Prim — cap like
        // the survey did; beyond it the MST bound is dominated by the
        // pour/trunk anyway.
        if pts.len() < 2 || pts.len() > 60 {
            continue;
        }
        let n = pts.len();
        let mut in_t = vec![false; n];
        in_t[0] = true;
        let mut count = 1;
        let mut mst = 0.0;
        while count < n {
            let mut best = (f64::INFINITY, 0);
            for a in 0..n {
                if !in_t[a] {
                    continue;
                }
                for b in 0..n {
                    if in_t[b] {
                        continue;
                    }
                    let d = (pts[a].0 - pts[b].0).hypot(pts[a].1 - pts[b].1);
                    if d < best.0 {
                        best = (d, b);
                    }
                }
            }
            mst += best.0;
            in_t[best.1] = true;
            count += 1;
        }
        if mst > 1.0 {
            ratios.push(routed / mst);
        }
    }
    if ratios.is_empty() {
        return (0.0, 0.0, 0);
    }
    ratios.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pick = |q: f64| ratios[((q * ratios.len() as f64) as usize).min(ratios.len() - 1)];
    (pick(0.5), pick(0.9), ratios.len())
}

/// Count the pour failures the sink counter can't see — the trial
/// scorer's blind spot the pour saga exposed: a trial reported "fully
/// connected" while the oracle found stranded pour islands and
/// un-dropped plane pads (drops/stitch run AFTER routing, so
/// connected_sinks never prices their failures, and best-of never
/// selects against them). Read-only, measured on the FINAL copper.
/// Boards without a signal-layer pour count 0 → selection unchanged.
fn pour_defect_count(board: &Board, final_routes: &[Route]) -> usize {
    let mut defects = 0usize;
    let n_layers = board.layer_stack.layers.len();
    for (ni, net) in board.nets.iter().enumerate() {
        let Some(pl) = net.plane_layer else { continue };
        if board.layer_stack.layers.get(pl).map(|l| l.kind)
            != Some(crate::types::LayerKind::Signal)
        {
            continue;
        }
        // (d) SEVERED live components on the EMISSION model — the
        // raster in (a) is optimistic about clearances and measured
        // blind to a real 3224-vertex severed assembly that KiCad
        // reported as a zone island. Counting it here lets the trial
        // machinery do what no local rung could: prefer a routing
        // whose pour never fractured in the first place.
        if let Some(polys) = output::kicad::emission_fill_polys(board, final_routes, ni) {
            defects += pour_severed_fragments(board, final_routes, ni, &polys).len();
            // (d) DANGLING TRACK ENDS on the pour net — the oracle's
            // track_dangling. The fill is already in hand here, so
            // this is the one place the test is affordable inside the
            // per-trial currency (check_drc exempts pour nets for
            // exactly that reason). A pour-side end is free if it
            // touches no same-net track on its layer, no via, no
            // same-net pad copper on that layer, AND is not inside the
            // emitted fill on the pour layer. Measured: a span-only
            // amputation tier won seed 99 on connected sinks while
            // shipping four 0.3mm vbias stubs the currency did not
            // see; a better-connected board with more debris must not
            // dominate a clean one.
            let via_r = board.layer_stack.via.pad_mm / 2.0;
            let pl = board.nets[ni].plane_layer.unwrap_or(0);
            let net_id = board.nets[ni].id;
            let last = board.layer_stack.layers.len().saturating_sub(1);
            let mut pads: Vec<(f64, f64, f64, f64, Option<usize>)> = Vec::new();
            for comp in &board.components {
                let (co, sn) = (comp.theta.cos(), comp.theta.sin());
                let quarter =
                    ((comp.theta / std::f64::consts::FRAC_PI_2).round() as i64).rem_euclid(2);
                let face = match comp.side {
                    BoardSide::Top => 0usize,
                    BoardSide::Bottom => last,
                };
                for p in comp.pins.iter().filter(|p| !p.unplaced && p.net == Some(net_id)) {
                    let gx = comp.x + p.dx * co - p.dy * sn;
                    let gy = comp.y + p.dx * sn + p.dy * co;
                    let (pw, ph) = p
                        .pad
                        .as_ref()
                        .map(|pd| (pd.width_mm, pd.height_mm))
                        .unwrap_or((0.5, 0.5));
                    let (hx, hy) = if quarter == 1 { (ph / 2.0, pw / 2.0) } else { (pw / 2.0, ph / 2.0) };
                    let tht = p.pad.as_ref().map_or(false, |pd| pd.drill_mm.is_some());
                    pads.push((gx, gy, hx, hy, if tht { None } else { Some(face) }));
                }
            }
            let inside = |x: f64, y: f64| -> bool {
                polys.iter().any(|poly| {
                    let mut ins = false;
                    let m = poly.len();
                    for k in 0..m {
                        let (x1, y1) = poly[k];
                        let (x2, y2) = poly[(k + 1) % m];
                        if (y1 > y) != (y2 > y) && x < (x2 - x1) * (y - y1) / (y2 - y1) + x1 {
                            ins = !ins;
                        }
                    }
                    ins
                })
            };
            let r = &final_routes[ni];
            for (si, sg) in r.segments.iter().enumerate() {
                for &e in &[sg.start, sg.end] {
                    let on_track = r.segments.iter().enumerate().any(|(sj, s2)| {
                        sj != si
                            && s2.layer == sg.layer
                            && geom::point_segment_dist(e, s2.start, s2.end)
                                <= (s2.width_mm + sg.width_mm) / 2.0 - 0.001
                    });
                    if on_track {
                        continue;
                    }
                    if r.vias.iter().any(|v| (v.x - e.0).hypot(v.y - e.1) <= via_r + sg.width_mm / 2.0) {
                        continue;
                    }
                    if pads.iter().any(|&(cx, cy, hx, hy, layer)| {
                        layer.map_or(true, |l| l == sg.layer)
                            && (e.0 - cx).abs() <= hx + sg.width_mm / 2.0
                            && (e.1 - cy).abs() <= hy + sg.width_mm / 2.0
                    }) {
                        continue;
                    }
                    if sg.layer == pl && inside(e.0, e.1) {
                        continue;
                    }
                    defects += 1;
                }
            }
        }
        // (a) Stranded islands: anchors spread over >1 raster label
        // after all stitching (a routed bridge merges labels because
        // the raster is rebuilt from the final routes).
        if let Some(raster) = output::kicad::pour_raster(board, final_routes, ni) {
            if raster.n_labels > 1 {
                let anchors = output::kicad::plane_anchor_points(board, final_routes, ni);
                let labels: std::collections::BTreeSet<u32> = anchors
                    .iter()
                    .map(|&(ax, ay)| raster.label_at(ax, ay))
                    .filter(|&l| l != 0)
                    .collect();
                defects += labels.len().saturating_sub(1);
            }
            // (c) DANGLING drops: a via whose pour-layer end lands in
            // a void (label 0 — e.g. an opposite-side SMD pad's punch)
            // and has no same-net pour-layer track either — connected
            // on one layer only, the oracle's via_dangling.
            for v in &final_routes[ni].vias {
                if raster.label_at(v.x, v.y) != 0 {
                    continue;
                }
                let tracked = final_routes[ni].segments.iter().any(|sg| {
                    sg.layer == pl
                        && ((v.x - sg.start.0).hypot(v.y - sg.start.1) < 0.01
                            || (v.x - sg.end.0).hypot(v.y - sg.end.1) < 0.01)
                });
                if !tracked {
                    defects += 1;
                }
            }
        }
        // (b) Off-side SMD pads with no live drop (stub on the pad's
        // layer touching pad copper with a via at its far end — the
        // drop pass's own liveness test).
        let comp_idx: crate::det::HashMap<ComponentId, usize> = board
            .components
            .iter()
            .enumerate()
            .map(|(k, c)| (c.id, k))
            .collect();
        for &(comp_id, pin_id) in &net.pins {
            let Some(&ci) = comp_idx.get(&comp_id) else { continue };
            let comp = &board.components[ci];
            let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pin_id) else {
                continue;
            };
            if pin.unplaced || pin.pad.as_ref().and_then(|p| p.drill_mm).is_some() {
                continue;
            }
            let pad_layer = match comp.side {
                BoardSide::Top => 0,
                BoardSide::Bottom => n_layers - 1,
            };
            if pad_layer == pl {
                continue; // pour-side pad contacts the fill directly
            }
            let cos_t = comp.theta.cos();
            let sin_t = comp.theta.sin();
            let px = comp.x + pin.dx * cos_t - pin.dy * sin_t;
            let py = comp.y + pin.dx * sin_t + pin.dy * cos_t;
            let half = pin
                .pad
                .as_ref()
                .map(|p| p.width_mm.min(p.height_mm) / 2.0)
                .unwrap_or(0.25);
            let has_live_drop = final_routes[ni].segments.iter().any(|sg| {
                if sg.layer != pad_layer {
                    return false;
                }
                if segment_point_too_close(sg.start, sg.end, (px, py), sg.width_mm / 2.0 + half - 0.001) {
                    final_routes[ni].vias.iter().any(|v| {
                        (v.x - sg.start.0).hypot(v.y - sg.start.1) < 0.01
                            || (v.x - sg.end.0).hypot(v.y - sg.end.1) < 0.01
                    })
                } else {
                    false
                }
            });
            if !has_live_drop {
                defects += 1;
            }
        }
    }
    defects
}

/// Env-gated diagnostic: sample every route segment against every
/// foreign pad rect (+spacing) and report intrusions with full context.
fn debug_check_foreign_pads(board: &Board, routes: &[Route], tag: &str) {
    for route in routes {
        for seg in &route.segments {
            for comp in &board.components {
                let cos_t = comp.theta.cos();
                let sin_t = comp.theta.sin();
                for pin in &comp.pins {
                    if pin.unplaced || pin.net == Some(route.net_id) {
                        continue;
                    }
                    let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
                    let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;
                    let (pw, ph) = pin.pad.as_ref().map(|p| (p.width_mm, p.height_mm)).unwrap_or((0.5, 0.5)); // matches exporter fallback
                    let (hx, hy) = (pw / 2.0 + 0.15, ph / 2.0 + 0.15);
                    for i in 0..=10 {
                        let t = i as f64 / 10.0;
                        let x = seg.start.0 + t * (seg.end.0 - seg.start.0);
                        let y = seg.start.1 + t * (seg.end.1 - seg.start.1);
                        if (x - gx).abs() < hx && (y - gy).abs() < hy {
                            log::warn!(
                                "CLEARANCE[{tag}] net route seg {:?}->{:?} intrudes {}.{} pad at ({gx:.2},{gy:.2}) net {:?} (route net {:?})",
                                seg.start, seg.end, comp.refdes, pin.name, pin.net, route.net_id
                            );
                            break;
                        }
                    }
                }
            }
        }
    }
}

/// Geometric final validation + rip (the shipping guarantee): no two
/// different-net copper items (track-track or track-PAD) on one layer
/// may intersect or under-clear. The cell model has deliberate blind
/// spots (blocked pad-halo cells exempt from overflow), so the last
/// word is geometry. Returns the indices of nets ripped this call.
/// 45° corner mitering: replace right-angle corners with diagonal
/// cuts wherever the EXACT geometry stays legal — the length and
/// elegance Manhattan-only routing leaves on the table, recovered as
/// a verified post-pass (routing WITH diagonals is unsafe at pitch
/// cells: parallel adjacent diagonals sit 0.212mm apart). Every miter
/// is validated against foreign copper before applying, and the
/// final validate_and_rip round remains the shipping guarantee.
/// STAIRCASE CANONICALIZATION: a monotone chain that alternates
/// between two compass directions (the grid's rendering of an
/// off-45 diagonal — measured: K2 on the ecc83 descending in three
/// 1.2mm 45° steps interleaved with verticals) is replaced by its
/// TWO maximal legs (diagonal first, then straight — or the reverse
/// when the first order conflicts), and a single-direction chain of
/// grid-cell stubs merges into one segment. Same H/V/45 discipline,
/// same endpoints, fewer bends — the hand-router's canonical form.
/// Legality is exact-kernel checked against foreign copper; the
/// caller wraps the pass in the miter block's transactional
/// save/validate/revert.
fn staircase_pass(board: &Board, final_routes: &mut [Route]) -> usize {
    let mut collapsed = 0usize;
    let dir8 = |a: (f64, f64), b: (f64, f64)| -> Option<(i8, i8)> {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = dx.hypot(dy);
        if len < 1e-6 {
            return None;
        }
        let (ux, uy) = (dx / len, dy / len);
        let sx = if ux > 0.4 {
            1
        } else if ux < -0.4 {
            -1
        } else {
            0
        };
        let sy = if uy > 0.4 {
            1
        } else if uy < -0.4 {
            -1
        } else {
            0
        };
        // Reject non-H/V/45 (a 26-degree leg snaps to a wrong dir).
        let (ex, ey) = (sx as f64, sy as f64);
        let elen = ex.hypot(ey);
        if elen < 0.5 || (ux * ex + uy * ey) / elen < 0.999 {
            return None;
        }
        Some((sx, sy))
    };
    // Orientation-agnostic span polyline: segments within a span may
    // be stored source-ward OR sink-ward (measured: the K2 chain is
    // reverse-chained and a forward-only walk never even sees it).
    let close = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).hypot(a.1 - b.1) < 1e-4;
    let span_polyline = |segs: &[RouteSegment]| -> Option<Vec<(f64, f64)>> {
        let n = segs.len();
        if n == 0 {
            return None;
        }
        let (l0, w0) = (segs[0].layer, segs[0].width_mm);
        if segs
            .iter()
            .any(|s| s.layer != l0 || (s.width_mm - w0).abs() > 1e-6)
        {
            return None;
        }
        if n == 1 {
            return Some(vec![segs[0].start, segs[0].end]);
        }
        let (a, b) = (&segs[0], &segs[1]);
        let (p0, p1) = if close(a.end, b.start) || close(a.end, b.end) {
            (a.start, a.end)
        } else if close(a.start, b.start) || close(a.start, b.end) {
            (a.end, a.start)
        } else {
            return None;
        };
        let mut pts = vec![p0, p1];
        for s in &segs[1..] {
            let cur = *pts.last().unwrap();
            if close(s.start, cur) {
                pts.push(s.end);
            } else if close(s.end, cur) {
                pts.push(s.start);
            } else {
                return None;
            }
        }
        Some(pts)
    };
    for ni in 0..final_routes.len() {
        let net_id = final_routes[ni].net_id;
        if final_routes[ni].segments.is_empty() {
            continue;
        }
        let mut cidx: Option<geom::ClearanceIndex> = None;
        let n_spans = final_routes[ni].path_spans.len();
        for sp in 0..n_spans {
            let mut scan_start = 0usize;
            'rescan: loop {
                let (s0, sl) = final_routes[ni].path_spans[sp];
                if sl < 2 {
                    break;
                }
                let segs = &final_routes[ni].segments;
                let Some(pts_all) = span_polyline(&segs[s0..s0 + sl]) else {
                    break 'rescan;
                };
                // Find the first EDGE window [e0, e1] using <=2 dirs
                // where a collapse would shed at least one segment.
                let edge_dir: Vec<Option<(i8, i8)>> = pts_all
                    .windows(2)
                    .map(|w| dir8(w[0], w[1]))
                    .collect();
                let mut found: Option<(usize, usize, Vec<(i8, i8)>)> = None;
                let mut e0 = scan_start.min(sl.saturating_sub(1));
                while e0 + 1 < sl {
                    let Some(d0) = edge_dir[e0] else {
                        e0 += 1;
                        continue;
                    };
                    let mut dirs = vec![d0];
                    let mut e1 = e0;
                    while e1 + 1 < sl {
                        let Some(d) = edge_dir[e1 + 1] else { break };
                        let mut set = dirs.clone();
                        if !set.contains(&d) {
                            set.push(d);
                        }
                        if set.len() > 2 {
                            break;
                        }
                        dirs = set;
                        e1 += 1;
                    }
                    let n_win = e1 - e0 + 1;
                    let worthwhile = (dirs.len() == 1 && n_win >= 2)
                        || (dirs.len() == 2 && n_win >= 3);
                    if worthwhile {
                        found = Some((e0, e1, dirs));
                        break;
                    }
                    e0 += 1;
                }
                let Some((e0, e1_max, _)) = found else { break 'rescan };
                let segs = &final_routes[ni].segments;
                let layer = segs[s0 + e0].layer;
                let width = segs[s0 + e0].width_mm;
                let idx = cidx.get_or_insert_with(|| {
                    geom::ClearanceIndex::build(board, final_routes_view(final_routes), Some(net_id))
                });
                // BACK-OFF: the maximal two-dir window often spans
                // half the board (its two-leg chord slashes occupied
                // space — never legal). Shrink from the right until a
                // sub-window collapses; the greedy break here left
                // the ACTUAL staircases untouched (measured: K2's
                // window swallowed the long exit runs and failed).
                let mut applied: Option<(usize, usize, Vec<RouteSegment>)> = None;
                let mut e1 = e1_max;
                'shrink: while e1 > e0 {
                    let mut dirs: Vec<(i8, i8)> = Vec::new();
                    for e in e0..=e1 {
                        if let Some(d) = edge_dir[e] {
                            if !dirs.contains(&d) {
                                dirs.push(d);
                            }
                        }
                    }
                    let n_win = e1 - e0 + 1;
                    let worthwhile = (dirs.len() == 1 && n_win >= 2)
                        || (dirs.len() == 2 && n_win >= 3);
                    if !worthwhile {
                        e1 -= 1;
                        continue;
                    }
                    let a0 = pts_all[e0];
                    let b1 = pts_all[e1 + 1];
                    let mut run: crate::det::HashMap<(i8, i8), (f64, f64)> =
                        crate::det::HashMap::default();
                    for e in e0..=e1 {
                        if let Some(d) = edge_dir[e] {
                            let en = run.entry(d).or_insert((0.0, 0.0));
                            en.0 += pts_all[e + 1].0 - pts_all[e].0;
                            en.1 += pts_all[e + 1].1 - pts_all[e].1;
                        }
                    }
                    let legs: Vec<(f64, f64)> =
                        dirs.iter().map(|d| run[d]).collect();
                    let orders: Vec<Vec<(f64, f64)>> = if legs.len() == 1 {
                        vec![legs.clone()]
                    } else {
                        vec![legs.clone(), legs.iter().rev().copied().collect()]
                    };
                    'orders: for ord in &orders {
                        let mut pts = vec![a0];
                        for &(vx, vy) in ord {
                            let last = *pts.last().unwrap();
                            pts.push((last.0 + vx, last.1 + vy));
                        }
                        let end = *pts.last().unwrap();
                        if (end.0 - b1.0).hypot(end.1 - b1.1) > 1e-4 {
                            continue;
                        }
                        for w in pts.windows(2) {
                            // Probe 0.2mm FAT: a hair-thin clearance
                            // passes the kernel but not the guarantee
                            // validator (measured 1.51mm vs the 1.6mm
                            // limit — chosen, objected, reverted).
                            if idx
                                .first_conflict(
                                    w[0], w[1], width + 0.2, layer, net_id,
                                )
                                .is_some()
                            {
                                continue 'orders;
                            }
                        }
                        let newsegs: Vec<RouteSegment> = pts
                            .windows(2)
                            .filter(|w| {
                                (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1) > 1e-6
                            })
                            .map(|w| RouteSegment {
                                layer,
                                start: w[0],
                                end: w[1],
                                width_mm: width,
                            })
                            .collect();
                        if (e1 - e0 + 1) > newsegs.len() {
                            applied = Some((s0 + e0, s0 + e1, newsegs));
                            break 'shrink;
                        }
                        break;
                    }
                    e1 -= 1;
                }
                let Some((i0, i1, newsegs)) = applied else {
                    // Nothing legal at this start — scan further along.
                    scan_start = e0 + 1;
                    continue 'rescan;
                };
                let removed = (i1 - i0 + 1) - newsegs.len();
                let r = &mut final_routes[ni];
                r.segments.splice(i0..=i1, newsegs);
                r.path_spans[sp].1 -= removed;
                for (q, spq) in r.path_spans.iter_mut().enumerate() {
                    if q != sp && spq.0 > i0 {
                        spq.0 -= removed;
                    }
                }
                cidx = None; // own copper changed — rebuild lazily
                collapsed += 1;
                scan_start = 0; // indices shifted — rescan the span
                continue 'rescan;
            }
        }
    }
    collapsed
}

/// Identity view helper: ClearanceIndex::build takes &[Route].
fn final_routes_view(r: &mut [Route]) -> &[Route] {
    r
}

fn miter_pass(board: &Board, final_routes: &mut [Route]) -> usize {
    let clearance = board.config.min_spacing_mm;
    let bw = board.config.outline.width();
    let bh = board.config.outline.height();
    let edge = board.config.edge_clearance_mm;
    let via_r = board.layer_stack.via.pad_mm / 2.0;

    // Foreign-copper snapshot (pads incl. NC).
    struct PadObs {
        net: Option<NetId>,
        layer_top: bool,
        layer_bot: bool,
        cx: f64,
        cy: f64,
        hx: f64,
        hy: f64,
    }
    let n_layers = board.layer_stack.layers.len();
    let mut pads: Vec<PadObs> = Vec::new();
    for comp in &board.components {
        let cos_t = comp.theta.cos();
        let sin_t = comp.theta.sin();
        let quarter =
            ((comp.theta / std::f64::consts::FRAC_PI_2).round() as i64).rem_euclid(2);
        for pin in &comp.pins {
            if pin.unplaced {
                continue;
            }
            let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
            let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;
            let (pw, ph, thru) = match &pin.pad {
                Some(p) => (p.width_mm, p.height_mm, p.drill_mm.is_some()),
                None => (0.5, 0.5, false),
            };
            let (pw, ph) = if quarter == 1 { (ph, pw) } else { (pw, ph) };
            pads.push(PadObs {
                net: pin.net,
                layer_top: thru || matches!(comp.side, BoardSide::Top),
                layer_bot: thru || matches!(comp.side, BoardSide::Bottom),
                cx: gx,
                cy: gy,
                hx: pw / 2.0,
                hy: ph / 2.0,
            });
        }
    }
    let pad_on_layer = |p: &PadObs, layer: usize| -> bool {
        // THT barrels (top && bot) exist on EVERY copper layer — the
        // surface-only test left inner-layer miters blind to them (an
        // In2 corner cut grazed a PTH annulus at 0.075mm, shipped as
        // the s99 clearance latent). Same rule as first_conflict.
        (p.layer_top && p.layer_bot)
            || (layer == 0 && p.layer_top)
            || (layer == n_layers - 1 && p.layer_bot)
    };

    // Snapshot foreign segments/vias per net lazily is O(n²); boards are
    // small — clone the pre-pass geometry once and validate against it
    // (miters only SHRINK copper, so validating against the original is
    // conservative-safe).
    let originals: Vec<Route> = final_routes.to_vec();

    let mut mitered = 0usize;
    for i in 0..final_routes.len() {
        let net_id = final_routes[i].net_id;
        let r = &mut final_routes[i];
        for sp in 0..r.path_spans.len() {
            let (ps, pl) = r.path_spans[sp];
            if pl < 2 {
                continue;
            }
            // Rebuild this span's segments with miters.
            let mut out: Vec<RouteSegment> = Vec::with_capacity(pl + 4);
            out.push(r.segments[ps].clone());
            for k in 1..pl {
                let b = r.segments[ps + k].clone();
                let a = out.last().unwrap().clone();
                // Corner: a.end == b.start, same layer/width, perpendicular.
                let corner = (a.end.0 - b.start.0).abs() < 1e-9
                    && (a.end.1 - b.start.1).abs() < 1e-9
                    && a.layer == b.layer
                    && (a.width_mm - b.width_mm).abs() < 1e-9
                    && {
                        let da = (a.end.0 - a.start.0, a.end.1 - a.start.1);
                        let db = (b.end.0 - b.start.0, b.end.1 - b.start.1);
                        (da.0 * db.0 + da.1 * db.1).abs() < 1e-9
                    };
                if !corner {
                    out.push(b);
                    continue;
                }
                // Never miter a JUNCTION corner: a child span or a via
                // attaches at the corner point, and cutting the corner
                // removes the very copper it anchors to (the guarantee
                // round then amputates the child — a +1 unc on nearly
                // every mitering board until this check).
                let corner_pt = a.end;
                let is_junction = originals[i].segments.iter().enumerate().any(
                    |(si2, sg)| {
                        (si2 < ps || si2 >= ps + pl)
                            && (((sg.start.0 - corner_pt.0).abs() < 1e-6
                                && (sg.start.1 - corner_pt.1).abs() < 1e-6)
                                || ((sg.end.0 - corner_pt.0).abs() < 1e-6
                                    && (sg.end.1 - corner_pt.1).abs() < 1e-6))
                    },
                ) || originals[i].vias.iter().any(|v| {
                    (v.x - corner_pt.0).abs() < 1e-6 && (v.y - corner_pt.1).abs() < 1e-6
                });
                if is_junction {
                    out.push(b);
                    continue;
                }
                let la = (a.end.0 - a.start.0).hypot(a.end.1 - a.start.1);
                let lb = (b.end.0 - b.start.0).hypot(b.end.1 - b.start.1);
                let d = (la.min(lb) * 0.5).min(0.9);
                if d < 0.15 {
                    out.push(b);
                    continue;
                }
                let ua = ((a.end.0 - a.start.0) / la, (a.end.1 - a.start.1) / la);
                let ub = ((b.end.0 - b.start.0) / lb, (b.end.1 - b.start.1) / lb);
                let p1 = (a.end.0 - ua.0 * d, a.end.1 - ua.1 * d);
                let p2 = (b.start.0 + ub.0 * d, b.start.1 + ub.1 * d);
                // Exact-committed spans (escapes / cross-unders) attach
                // MID-SEGMENT: an anchor anywhere on the cut portions
                // (last d of leg a, first d of leg b) — not just at the
                // corner point — dangles if the miter removes it.
                let anchor_in_cut = {
                    let on_cut = |pt: (f64, f64)| -> bool {
                        geom::point_segment_dist(pt, p1, a.end) < a.width_mm / 2.0 + 1e-6
                            || geom::point_segment_dist(pt, b.start, p2)
                                < b.width_mm / 2.0 + 1e-6
                    };
                    originals[i].segments.iter().enumerate().any(|(si2, sg)| {
                        (si2 < ps || si2 >= ps + pl)
                            && (on_cut(sg.start) || on_cut(sg.end))
                    }) || originals[i].vias.iter().any(|v| on_cut((v.x, v.y)))
                };
                if anchor_in_cut {
                    out.push(b);
                    continue;
                }
                // Validate the diagonal against FOREIGN copper + edge.
                let w = a.width_mm;
                let m = w / 2.0;
                let inside = p1.0.min(p2.0) - m > edge
                    && p1.1.min(p2.1) - m > edge
                    && p1.0.max(p2.0) + m < bw - edge
                    && p1.1.max(p2.1) + m < bh - edge;
                let mut ok = inside;
                if ok {
                    'chk: for (j, other) in originals.iter().enumerate() {
                        let foreign = board.nets.get(j).map(|x| x.id) != Some(net_id);
                        if !foreign {
                            continue;
                        }
                        for sg in &other.segments {
                            if sg.layer != a.layer {
                                continue;
                            }
                            if segments_too_close(
                                p1,
                                p2,
                                sg.start,
                                sg.end,
                                m + sg.width_mm / 2.0 + clearance,
                            ) {
                                ok = false;
                                break 'chk;
                            }
                        }
                        for v in &other.vias {
                            if segment_point_too_close(
                                p1,
                                p2,
                                (v.x, v.y),
                                m + via_r + clearance,
                            ) {
                                ok = false;
                                break 'chk;
                            }
                        }
                    }
                }
                if ok {
                    for p in &pads {
                        if p.net == Some(net_id) || !pad_on_layer(p, a.layer) {
                            continue;
                        }
                        // sample the diagonal vs pad rect
                        for t in 0..=6 {
                            let tt = t as f64 / 6.0;
                            let x = p1.0 + tt * (p2.0 - p1.0);
                            let y = p1.1 + tt * (p2.1 - p1.1);
                            if (x - p.cx).abs() < p.hx + m + clearance
                                && (y - p.cy).abs() < p.hy + m + clearance
                            {
                                ok = false;
                                break;
                            }
                        }
                        if !ok {
                            break;
                        }
                    }
                }
                if !ok {
                    out.push(b);
                    continue;
                }
                // Apply: shorten a, insert diagonal, shorten b.
                let last = out.last_mut().unwrap();
                last.end = p1;
                out.push(RouteSegment {
                    layer: a.layer,
                    start: p1,
                    end: p2,
                    width_mm: w,
                });
                let mut nb = b;
                nb.start = p2;
                out.push(nb);
                mitered += 1;
            }
            // Splice the rebuilt span back, shifting later spans.
            let delta = out.len() as i64 - pl as i64;
            r.segments.splice(ps..ps + pl, out);
            r.path_spans[sp].1 = (pl as i64 + delta) as usize;
            for (qi, q) in r.path_spans.iter_mut().enumerate() {
                if qi != sp && q.0 > ps {
                    q.0 = (q.0 as i64 + delta) as usize;
                }
            }
        }
    }
    mitered
}

/// Compute split-plane band regions for Power layers shared by
/// multiple rails. Single-net planes (and Ground planes) keep
/// plane_region = None (whole layer).
fn assign_plane_regions(board: &mut Board) {
    use std::collections::BTreeMap;
    // Polygon boards never band-split: plane assignment already limits
    // them to one rail per Power layer, and a rectangular band on a
    // concave outline can be disconnected.
    if matches!(board.config.outline, BoardOutline::Polygon(_)) {
        return;
    }
    let bw = board.config.outline.width();
    let bh = board.config.outline.height();
    let comp_idx: crate::det::HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();
    // Group plane nets by layer. DECLARED-POUR nets are exempt from
    // band arbitration: their coexistence on a shared face is handled
    // by geometric subtraction (the pin-cloud region carved out of
    // the ground fill) — the band splitter saw GND+vbias on F.Cu,
    // failed its separability gate on the board-wide ground, and
    // silently cleared the vbias pour.
    let mut by_layer: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, n) in board.nets.iter().enumerate() {
        if n.pour_region_pending || n.plane_region.is_some() {
            continue;
        }
        if board
            .layer_stack
            .layers
            .get(n.plane_layer.unwrap_or(usize::MAX))
            .map(|l| l.kind == crate::types::LayerKind::Signal)
            .unwrap_or(false)
        {
            continue; // signal-layer pours coexist by subtraction
        }
        if let Some(l) = n.plane_layer {
            by_layer.entry(l).or_default().push(i);
        }
    }
    for (layer, nets_on) in by_layer {
        if nets_on.len() < 2 {
            continue; // whole-layer plane
        }
        // Rail centroids from pin positions.
        let mut cents: Vec<(usize, f64, f64)> = Vec::new();
        for &ni in &nets_on {
            let (mut sx, mut sy, mut n) = (0.0, 0.0, 0usize);
            for &(cid, pid) in &board.nets[ni].pins {
                let Some(&ci) = comp_idx.get(&cid) else { continue };
                let comp = &board.components[ci];
                let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pid) else {
                    continue;
                };
                let cos_t = comp.theta.cos();
                let sin_t = comp.theta.sin();
                sx += comp.x + pin.dx * cos_t - pin.dy * sin_t;
                sy += comp.y + pin.dx * sin_t + pin.dy * cos_t;
                n += 1;
            }
            if n > 0 {
                cents.push((ni, sx / n as f64, sy / n as f64));
            } else {
                cents.push((ni, bw / 2.0, bh / 2.0));
            }
        }
        // Axis of larger centroid spread.
        let spread = |vals: Vec<f64>| -> f64 {
            let mn = vals.iter().cloned().fold(f64::INFINITY, f64::min);
            let mx = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            mx - mn
        };
        let x_spread = spread(cents.iter().map(|c| c.1).collect());
        let y_spread = spread(cents.iter().map(|c| c.2).collect());
        let use_x = x_spread >= y_spread;
        cents.sort_by(|a, b| {
            let (ka, kb) = if use_x { (a.1, b.1) } else { (a.2, b.2) };
            ka.partial_cmp(&kb)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| board.nets[a.0].name.cmp(&board.nets[b.0].name))
        });
        // Band boundaries at centroid midpoints.
        let full = if use_x { bw } else { bh };
        let mut bounds = vec![0.0];
        for w in cents.windows(2) {
            let (a, b) = if use_x { (w[0].1, w[1].1) } else { (w[0].2, w[1].2) };
            bounds.push((a + b) / 2.0);
        }
        bounds.push(full);
        // SEPARABILITY GATE: bands only help when each rail's pins
        // actually live in its band. A scattered rail (VCC everywhere)
        // forced into a band strands its far pins — worse than letting
        // it route as copper. Require ≥60% of every rail's pins within
        // its band (±2mm slack); otherwise keep only the fattest rail
        // on the whole layer and release the rest to the router.
        let mut regions: Vec<(usize, (f64, f64, f64, f64))> = Vec::new();
        for (k, &(ni, _, _)) in cents.iter().enumerate() {
            let lo = bounds[k] + if k == 0 { 0.0 } else { 0.25 };
            let hi = bounds[k + 1] - if k + 1 == cents.len() { 0.0 } else { 0.25 };
            let region = if use_x {
                (lo, 0.0, hi, bh)
            } else {
                (0.0, lo, bw, hi)
            };
            regions.push((ni, region));
        }
        let mut separable = true;
        'gate: for &(ni, (rx0, ry0, rx1, ry1)) in &regions {
            let (mut inside, mut total) = (0usize, 0usize);
            for &(cid, pid) in &board.nets[ni].pins {
                let Some(&ci) = comp_idx.get(&cid) else { continue };
                let comp = &board.components[ci];
                let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pid) else {
                    continue;
                };
                let cos_t = comp.theta.cos();
                let sin_t = comp.theta.sin();
                let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
                let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;
                total += 1;
                if gx > rx0 - 2.0 && gx < rx1 + 2.0 && gy > ry0 - 2.0 && gy < ry1 + 2.0
                {
                    inside += 1;
                }
            }
            if total > 0 && (inside as f64) < 0.6 * total as f64 {
                info!(
                    "split plane {layer}: NOT separable ('{}': {inside}/{total} pins \
                     in band) — fattest rail keeps the whole layer",
                    board.nets[ni].name
                );
                separable = false;
                break 'gate;
            }
        }
        if separable {
            for &(ni, region) in &regions {
                board.nets[ni].plane_region = Some(region);
                info!(
                    "split plane {}: '{}' region ({:.1},{:.1})-({:.1},{:.1})",
                    layer, board.nets[ni].name, region.0, region.1, region.2, region.3
                );
            }
        } else {
            // Fattest (first assigned = widest) keeps the layer; others
            // go back to the router.
            let keep = nets_on
                .iter()
                .copied()
                .max_by(|&a, &b| {
                    board.nets[a]
                        .required_trace_width_mm
                        .partial_cmp(&board.nets[b].required_trace_width_mm)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| board.nets[b].name.cmp(&board.nets[a].name))
                })
                .unwrap();
            for &ni in &nets_on {
                if ni != keep {
                    board.nets[ni].plane_layer = None;
                }
            }
        }
    }
}

/// Connect plane-assigned nets' SURFACE pads to their plane with a
/// stub + via. Sites are searched deterministically on rings around the
/// pad and checked geometrically against ALL existing copper (foreign
/// pads, tracks, vias, board edge). Returns the number of pads dropped.
/// Drop vias for plane-assigned nets + swallow verification, as one
/// idempotent pass: pads with a live drop are skipped, pads without one
/// get a freshly-sited drop, and any drop whose plane contact was eaten
/// by merged fill punches is removed (honest unconnected). Runs at 5.5
/// and AGAIN after geometric recovery — plane nets are excluded from
/// tree recovery, so re-siting here is the only repair path when the
/// validator amputates a drop that later copper made illegal.
/// Extend every incomplete non-plane net (unreached sinks, vias
/// allowed) on a grid where all other committed copper is blocked.
/// Cheap when nothing is missing (grid-free pre-filter). Returns the
/// number of sinks reclaimed.
/// Diff-pair partner attraction for post-negotiation extensions
/// (recovery, completion): the partner's routed corridor, when it
/// exists. Without this, a pair member re-routed after validation
/// abandons the coupled run (and cheap empty-layer vias).
/// Nets whose Topology constraint declares a SHAPE (star / chain):
/// tree extension cannot preserve it — recovery rebuilds these whole.
fn has_shape_topology(board: &Board, i: usize) -> bool {
    use crate::constraint::{Constraint, TopoKind};
    let id = board.nets[i].id;
    board.constraints.iter().any(|c| {
        matches!(
            c,
            Constraint::Topology { net, kind, .. }
                if *net == id
                    && matches!(
                        kind,
                        TopoKind::Star | TopoKind::DaisyChain | TopoKind::FlyBy
                    )
        )
    })
}

fn pair_attract(
    board: &Board,
    final_routes: &[Route],
    grid: &routing::grid::RoutingGrid,
    i: usize,
) -> Option<std::collections::BTreeSet<routing::grid::CellCoord>> {
    use crate::constraint::Constraint;
    let my_id = board.nets[i].id;
    let partner_id = board.constraints.iter().find_map(|c| match c {
        Constraint::DiffPair { p_net, n_net, .. } if *p_net == my_id => Some(*n_net),
        Constraint::DiffPair { p_net, n_net, .. } if *n_net == my_id => Some(*p_net),
        _ => None,
    })?;
    let pi = board.nets.iter().position(|n| n.id == partner_id)?;
    if final_routes[pi].is_empty() {
        return None;
    }
    Some(pathfinder::build_attract_set(grid, &final_routes[pi]))
}

/// Remove the `doomed` spans from a route: drain their segments and
/// vias, shift the survivors' span starts, remap parent indices.
/// (Same surgery the validator's subtree amputation performs.)
/// Debug (BHDL_PNR_VIA_CHECK): KiCad's via rule — segment endpoints
/// on >=2 distinct layers within the barrel, else dangling.
fn via_anchor_check(board: &Board, routes: &[Route], tag: &str) {
    if let Ok(t) = std::env::var("BHDL_PNR_VIA_NEAR") {
        if let Some((tx, ty)) = t.split_once(',').and_then(|(a, b)| {
            Some((a.trim().parse::<f64>().ok()?, b.trim().parse::<f64>().ok()?))
        }) {
            for (ri, r) in routes.iter().enumerate() {
                for v in &r.vias {
                    if (v.x - tx).hypot(v.y - ty) < 1.0 {
                        log::warn!(
                            "[via-near {tag}] net '{}' via ({:.3},{:.3})",
                            board.nets[ri].name, v.x, v.y
                        );
                    }
                }
            }
        }
    }
    if std::env::var("BHDL_PNR_VIA_CHECK").is_err() {
        return;
    }
    let via_r = board.layer_stack.via.pad_mm / 2.0;
    for (ri, r) in routes.iter().enumerate() {
        if board.nets[ri].plane_layer.is_some() {
            continue;
        }
        for (vi, v) in r.vias.iter().enumerate() {
            let mut layers: std::collections::BTreeSet<usize> =
                std::collections::BTreeSet::new();
            for sg in &r.segments {
                for e in [sg.start, sg.end] {
                    if (e.0 - v.x).hypot(e.1 - v.y) <= via_r {
                        layers.insert(sg.layer);
                    }
                }
            }
            if layers.len() < 2 {
                log::warn!(
                    "[via-check {tag}] net '{}' via {vi} ({:.2},{:.2}) layers={:?}",
                    board.nets[ri].name, v.x, v.y, layers
                );
            }
        }
    }
}

fn strip_route_spans(r: &mut Route, doomed: &[bool]) {
    let n = r.path_spans.len();
    let mut order: Vec<usize> = (0..n).filter(|&i| doomed[i]).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(r.path_spans[i].0));
    for &i in &order {
        let (ps, pl) = r.path_spans[i];
        r.segments.drain(ps..ps + pl);
        for j in 0..n {
            if !doomed[j] && r.path_spans[j].0 > ps {
                r.path_spans[j].0 -= pl;
            }
        }
    }
    let mut vorder: Vec<usize> =
        (0..n).filter(|&i| doomed[i] && i < r.via_spans.len()).collect();
    vorder.sort_by_key(|&i| std::cmp::Reverse(r.via_spans[i].0));
    for &i in &vorder {
        let (vs, vl) = r.via_spans[i];
        if vl > 0 && vs + vl <= r.vias.len() {
            r.vias.drain(vs..vs + vl);
            for j in 0..n {
                if !doomed[j] && j < r.via_spans.len() && r.via_spans[j].0 > vs {
                    r.via_spans[j].0 -= vl;
                }
            }
        }
    }
    let mut remap = vec![usize::MAX; n];
    let mut next = 0usize;
    for i in 0..n {
        if !doomed[i] {
            remap[i] = next;
            next += 1;
        }
    }
    let spans: Vec<(usize, usize)> =
        (0..n).filter(|&i| !doomed[i]).map(|i| r.path_spans[i]).collect();
    let parents: Vec<Option<usize>> = (0..n)
        .filter(|&i| !doomed[i])
        .map(|i| {
            r.path_parents.get(i).copied().flatten().and_then(|pp| {
                if doomed[pp] { None } else { Some(remap[pp]) }
            })
        })
        .collect();
    let vspans: Vec<(usize, usize)> = (0..n)
        .filter(|&i| !doomed[i])
        .map(|i| r.via_spans.get(i).copied().unwrap_or((0, 0)))
        .collect();
    r.path_spans = spans;
    r.path_parents = parents;
    r.via_spans = vspans;
}

/// M3: span-level commit gate. Every NEW span (index >= `from_span`)
/// of a route mutation is checked against the exact clearance index;
/// offending spans — and their new-span descendants — are STRIPPED
/// before the mutation enters the board, at the same granularity the
/// validator's subtree amputation would have used after the fact.
/// Clean spans keep their sinks: partial progress survives, illegal
/// copper never ships. Conflict points are appended to `bans`.
/// Returns the number of new spans kept.
fn exact_commit_strip(
    board: &Board,
    final_routes: &[Route],
    i: usize,
    candidate: &mut Route,
    from_span: usize,
    bans: &mut Vec<(f64, f64)>,
) -> usize {
    let net = &board.nets[i];
    let n = candidate.path_spans.len();
    if from_span >= n {
        return 0;
    }
    let idx = geom::ClearanceIndex::build(board, final_routes, Some(net.id));
    let via_r = board.layer_stack.via.pad_mm / 2.0;
    let mut doomed = vec![false; n];
    for si in from_span..n {
        let (ps, pl) = candidate.path_spans[si];
        'span: for sg in &candidate.segments[ps..ps + pl] {
            if let Some(c) =
                idx.first_conflict(sg.start, sg.end, sg.width_mm, sg.layer, net.id)
            {
                debug!(
                    "commit gate: '{}' span {si} conflicts ({:?}) at ({:.2},{:.2})-({:.2},{:.2})",
                    net.name, c, sg.start.0, sg.start.1, sg.end.0, sg.end.1
                );
                bans.push((
                    (sg.start.0 + sg.end.0) / 2.0,
                    (sg.start.1 + sg.end.1) / 2.0,
                ));
                doomed[si] = true;
                break 'span;
            }
        }
        if !doomed[si] {
            if let Some(&(vs, vl)) = candidate.via_spans.get(si) {
                if vl > 0 && vs + vl <= candidate.vias.len() {
                    for v in &candidate.vias[vs..vs + vl] {
                        if idx.via_conflict(v.x, v.y, via_r, net.id).is_some() {
                            bans.push((v.x, v.y));
                            doomed[si] = true;
                            break;
                        }
                    }
                }
            }
        }
    }
    // Descendants of a doomed new span are stranded — strip them too.
    loop {
        let mut grew = false;
        for si in from_span..n {
            if !doomed[si] {
                if let Some(Some(pp)) = candidate.path_parents.get(si) {
                    if doomed[*pp] {
                        doomed[si] = true;
                        grew = true;
                    }
                }
            }
        }
        if !grew {
            break;
        }
    }
    let cut = doomed.iter().filter(|d| **d).count();
    if cut > 0 {
        strip_route_spans(candidate, &doomed);
        // Prune stubs the cut orphaned (same rule as the validator's
        // post-amputation prune): a short parentless span anchored to
        // nothing — no other surviving span's copper, no via, no
        // own-net pad — is dangling copper the oracle flags and the
        // retry extension trips over.
        let mut own_pads: Vec<(f64, f64, f64, f64)> = Vec::new();
        for comp in &board.components {
            let cos_t = comp.theta.cos();
            let sin_t = comp.theta.sin();
            let quarter = ((comp.theta / std::f64::consts::FRAC_PI_2).round() as i64)
                .rem_euclid(2);
            for pin in &comp.pins {
                if pin.net != Some(net.id) || pin.unplaced {
                    continue;
                }
                let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
                let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;
                let (pw, ph) = match &pin.pad {
                    Some(p) => (p.width_mm, p.height_mm),
                    None => (0.5, 0.5),
                };
                let (pw, ph) = if quarter == 1 { (ph, pw) } else { (pw, ph) };
                own_pads.push((gx, gy, pw / 2.0, ph / 2.0));
            }
        }
        loop {
            let r = &*candidate;
            let mut drop_span: Option<usize> = None;
            'stubs: for (si, &(ps, pl)) in r.path_spans.iter().enumerate() {
                if pl == 0 || pl > 2 {
                    continue;
                }
                if r.path_parents.get(si).copied().flatten().is_some() {
                    continue;
                }
                let len: f64 = r.segments[ps..ps + pl]
                    .iter()
                    .map(|sg| (sg.end.0 - sg.start.0).hypot(sg.end.1 - sg.start.1))
                    .sum();
                if len > 1.0 {
                    continue;
                }
                let ends = [r.segments[ps].start, r.segments[ps + pl - 1].end];
                for e in ends {
                    let w = r.segments[ps].width_mm / 2.0;
                    let anchored = r
                        .path_spans
                        .iter()
                        .enumerate()
                        .filter(|&(sj, _)| sj != si)
                        .any(|(_, &(qs, ql))| {
                            r.segments[qs..qs + ql].iter().any(|sg| {
                                geom::point_segment_dist(e, sg.start, sg.end)
                                    <= sg.width_mm / 2.0 + w
                            })
                        })
                        || r.vias.iter().any(|v| (v.x - e.0).hypot(v.y - e.1) <= 0.5)
                        || own_pads.iter().any(|&(cx, cy, hx, hy)| {
                            (e.0 - cx).abs() <= hx && (e.1 - cy).abs() <= hy
                        });
                    if anchored {
                        continue 'stubs;
                    }
                }
                drop_span = Some(si);
                break;
            }
            match drop_span {
                Some(si) => {
                    let mut d = vec![false; candidate.path_spans.len()];
                    d[si] = true;
                    strip_route_spans(candidate, &d);
                }
                None => break,
            }
        }
    }
    (n - from_span) - cut
}

fn completion_pass(board: &Board, final_routes: &mut Vec<Route>) -> usize {
    let mut total = 0usize;
    for i in 0..board.nets.len() {
        if board.nets[i].is_plane_connected(&board.layer_stack) {
            continue;
        }
        if final_routes[i].is_empty() {
            continue; // pass 2 already tried from scratch
        }
        if pathfinder::unreached_sink_count(&board.nets[i], &board, &final_routes[i]) == 0 {
            continue;
        }
        let mut ext_grid = RoutingGrid::build(board);
        for (j, route) in final_routes.iter().enumerate() {
            if j != i && !route.is_empty() {
                pathfinder::block_route_geometry(&mut ext_grid, route, board);
            }
        }
        let attract = pair_attract(board, final_routes, &ext_grid, i);
        if has_shape_topology(board, i) {
            // Shape topologies complete by WHOLE rebuild.
            let rebuilt = pathfinder::route_single_net(
                &ext_grid,
                &board.nets[i],
                board,
                true,
                attract.as_ref(),
            );
            let whole_legal = {
                let mut probe = rebuilt.clone();
                let mut bans = Vec::new();
                let total = probe.path_spans.len();
                exact_commit_strip(board, final_routes, i, &mut probe, 0, &mut bans)
                    == total
            };
            if whole_legal
                && pathfinder::unreached_sink_count(&board.nets[i], board, &rebuilt)
                    < pathfinder::unreached_sink_count(&board.nets[i], board, &final_routes[i])
            {
                info!(
                    "completion: topology rebuild of '{}'",
                    board.nets[i].name
                );
                final_routes[i] = rebuilt;
                total += 1;
            }
            continue;
        }
        let mut route = final_routes[i].clone();
        let from_span = route.path_spans.len();
        let got = pathfinder::extend_route(
            &mut ext_grid, &board.nets[i], board, &mut route, 1.0, 1.0, &[], &[], false,
            attract.as_ref(),
        );
        if got > 0 {
            let mut bans = Vec::new();
            let kept =
                exact_commit_strip(board, final_routes, i, &mut route, from_span, &mut bans);
            if kept > 0 {
                info!(
                    "completion: extended '{}' ({kept} legal branch(es), {got} sink(s) reached)",
                    board.nets[i].name
                );
                final_routes[i] = route;
                total += got;
            }
        }
        // TARGETED RIP-UP-AND-REROUTE (single victim): a sink still
        // walled in after extension is usually fenced by ONE other
        // net's copper. Rip the cheapest nearby candidate, extend
        // ourselves through the freed corridor, re-route the victim
        // from scratch on what remains — accepted only when the
        // board's total unreached strictly drops; otherwise both
        // revert. (NOT push-and-shove: the victim is rebuilt, not
        // geometrically deformed — true shove needs the continuous-
        // geometry kernel parked as P1.)
        if pathfinder::unreached_sink_count(&board.nets[i], board, &final_routes[i]) > 0 {
            total += shove_one_blocker(board, final_routes, i);
        }
        // The exact ladder (escape / via-hop / cross-under / shove)
        // used to run only as the final 5.97 pass; running it here
        // lets later passes see the copper and the validator (now
        // exact on pads too) polices it like everything else.
        if pathfinder::unreached_sink_count(&board.nets[i], board, &final_routes[i]) > 0 {
            total += offgrid_escape(board, final_routes, i);
        }
        // VICTIM RIP + EXACT-LADDER RETRY: shove_one_blocker's grid
        // extend cannot thread sub-grid corridors, and the exact
        // ladder cannot cross a fence a single foreign net pins in
        // place (uno s5: D_P behind one crossing jog). Rip the
        // blocker wholesale, rerun the full exact ladder through the
        // freed corridor, rebuild the victim, strict-total-win or
        // revert both.
        if pathfinder::unreached_sink_count(&board.nets[i], board, &final_routes[i]) > 0 {
            total += rip_and_exact_retry(board, final_routes, i);
        }
    }
    total
}

/// See the call site: single-victim rip with EXACT-ladder retry for
/// sinks the grid-based shove_one_blocker cannot free.
fn rip_and_exact_retry(board: &Board, final_routes: &mut Vec<Route>, i: usize) -> usize {
    let comp_idx: crate::det::HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(k, c)| (c.id, k))
        .collect();
    let n_layers = board.layer_stack.layers.len();
    use crate::routing::pathfinder::route_components;
    // Unreached pad positions: no touching copper on the pad's
    // surface layer, or stranded on an island holding no second pad.
    let mut pads: Vec<((f64, f64), usize, Option<usize>)> = Vec::new();
    let mut comp_pads: crate::det::HashMap<usize, usize> = crate::det::HashMap::default();
    {
        let r = &final_routes[i];
        let comps = route_components(r);
        for &(cid, pid) in &board.nets[i].pins {
            let Some(&ci) = comp_idx.get(&cid) else { continue };
            let comp = &board.components[ci];
            let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pid) else { continue };
            if pin.unplaced {
                continue;
            }
            let cos_t = comp.theta.cos();
            let sin_t = comp.theta.sin();
            let px = comp.x + pin.dx * cos_t - pin.dy * sin_t;
            let py = comp.y + pin.dx * sin_t + pin.dy * cos_t;
            let half = pin
                .pad
                .as_ref()
                .map(|p| p.width_mm.min(p.height_mm) / 2.0)
                .unwrap_or(0.25);
            let layer = match comp.side {
                BoardSide::Top => 0,
                BoardSide::Bottom => n_layers - 1,
            };
            let pad_comp: Option<usize> = r
                .segments
                .iter()
                .enumerate()
                .find(|(_, sg)| {
                    sg.layer == layer
                        && geom::point_segment_dist((px, py), sg.start, sg.end)
                            < sg.width_mm / 2.0 + half - 0.001
                })
                .map(|(si, _)| comps[si]);
            if let Some(pc) = pad_comp {
                *comp_pads.entry(pc).or_insert(0) += 1;
            }
            pads.push(((px, py), layer, pad_comp));
        }
    }
    let targets: Vec<((f64, f64), usize, Option<usize>)> = pads
        .into_iter()
        .filter(|(_, _, pc)| match pc {
            None => true,
            Some(c) => comp_pads.get(c).copied().unwrap_or(0) < 2,
        })
        .collect();
    if targets.is_empty() {
        return 0;
    }
    let mut before_i =
        pathfinder::unreached_sink_count(&board.nets[i], board, &final_routes[i]);
    let mut gained = 0usize;
    let net_id = board.nets[i].id;
    let width = board
        .config
        .min_trace_width_mm
        .max(0.15)
        .min(board.nets[i].required_trace_width_mm);
    for ((px, py), layer, pad_comp) in targets.into_iter().take(4) {
        if before_i == 0 {
            break;
        }
        // Attach candidates on OTHER components' same-layer copper.
        let mut attach: Vec<((f64, f64), f64)> = {
            let r = &final_routes[i];
            let comps = route_components(r);
            r.segments
                .iter()
                .enumerate()
                .filter(|(si, sg)| sg.layer == layer && Some(comps[*si]) != pad_comp)
                .map(|(_, sg)| {
                    let (dx, dy) = (sg.end.0 - sg.start.0, sg.end.1 - sg.start.1);
                    let l2 = dx * dx + dy * dy;
                    let t = if l2 <= 1e-12 {
                        0.0
                    } else {
                        (((px - sg.start.0) * dx + (py - sg.start.1) * dy) / l2)
                            .clamp(0.0, 1.0)
                    };
                    let q = (sg.start.0 + t * dx, sg.start.1 + t * dy);
                    (q, (px - q.0).hypot(py - q.1))
                })
                .collect()
        };
        attach.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        attach.truncate(4);
        let mut victims: Vec<usize> = Vec::new();
        {
            let idx = geom::ClearanceIndex::build(board, final_routes, Some(net_id));
            for &(q, _) in &attach {
                if let Some(geom::Conflict::Track { net: vn, .. }) =
                    geom::escape_blocker(&idx, (px, py), q, width, layer, net_id)
                {
                    if let Some(vj) = board.nets.iter().position(|n| n.id == vn) {
                        if vj != i
                            && board.nets[vj].plane_layer.is_none()
                            && !final_routes[vj].is_empty()
                            && !victims.contains(&vj)
                        {
                            victims.push(vj);
                        }
                    }
                }
            }
        }
        for &vj in victims.iter().take(2) {
            let snap_v = final_routes[vj].clone();
            let snap_i = final_routes[i].clone();
            final_routes[vj] = Route::empty(snap_v.net_id);
            offgrid_escape(board, final_routes, i);
            let i_after =
                pathfinder::unreached_sink_count(&board.nets[i], board, &final_routes[i]);
            if i_after >= before_i {
                final_routes[vj] = snap_v;
                final_routes[i] = snap_i;
                continue;
            }
            // Rebuild the victim on the board that carries the join.
            let mut jgrid = RoutingGrid::build(board);
            for (k, r) in final_routes.iter().enumerate() {
                if k != vj && !r.is_empty() {
                    pathfinder::block_route_geometry(&mut jgrid, r, board);
                }
            }
            let mut fresh = Route::empty(snap_v.net_id);
            pathfinder::extend_route(
                &mut jgrid, &board.nets[vj], board, &mut fresh, 1.0, 1.0, &[], &[], false,
                None,
            );
            {
                let mut bans = Vec::new();
                let mut trial_board: Vec<Route> = final_routes.clone();
                trial_board[vj] = Route::empty(snap_v.net_id);
                exact_commit_strip(board, &trial_board, vj, &mut fresh, 0, &mut bans);
            }
            final_routes[vj] = fresh;
            if pathfinder::unreached_sink_count(&board.nets[vj], board, &final_routes[vj])
                > 0
            {
                offgrid_escape(board, final_routes, vj);
            }
            let v_before = pathfinder::unreached_sink_count(&board.nets[vj], board, &snap_v);
            let v_after = pathfinder::unreached_sink_count(
                &board.nets[vj], board, &final_routes[vj],
            );
            if i_after + v_after < before_i + v_before {
                info!(
                    "completion: exact-ladder retry connected '{}' after ripping '{}' (victim unreached {v_before} -> {v_after})",
                    board.nets[i].name, board.nets[vj].name
                );
                gained += before_i - i_after;
                before_i = i_after;
                break;
            }
            final_routes[vj] = snap_v;
            final_routes[i] = snap_i;
        }
    }
    gained
}

/// M4: true push-and-shove, v1. Geometrically DEFORM a blocking
/// foreign track — a lateral bump around the escape corridor, every
/// new segment exactly gated — instead of ripping and rebuilding its
/// net. Connectivity is preserved by construction (the detour keeps
/// the segment's endpoints); the victim's net stays whole. On
/// success the deformation is committed to `final_routes` and the
/// pre-shove route is pushed onto `snapshots` for the caller to
/// revert if the escape still fails.
fn try_shove_track(
    board: &Board,
    final_routes: &mut [Route],
    i: usize,
    blocker: &geom::Conflict,
    from: (f64, f64),
    to: (f64, f64),
    escape_width: f64,
    snapshots: &mut Vec<(usize, Route)>,
) -> bool {
    let geom::Conflict::Track { net: bnet, layer, a, b } = *blocker else {
        return false;
    };
    let Some(j) = board.nets.iter().position(|n| n.id == bnet) else {
        return false;
    };
    if j == i || board.nets[j].plane_layer.is_some() {
        return false;
    }
    let close = |p: (f64, f64), q: (f64, f64)| (p.0 - q.0).hypot(p.1 - q.1) < 1e-6;
    let Some(sj) = final_routes[j].segments.iter().position(|sg| {
        sg.layer == layer
            && ((close(sg.start, a) && close(sg.end, b))
                || (close(sg.start, b) && close(sg.end, a)))
    }) else {
        return false;
    };
    // Grid routes are chains of pitch-length pieces: bumping one
    // 0.3mm piece can't open a corridor. Expand to the maximal
    // COLLINEAR RUN containing the blocker (within its span) and
    // deform the run as one unit.
    let (span_lo, span_hi) = {
        let r = &final_routes[j];
        match r
            .path_spans
            .iter()
            .find(|&&(ps, pl)| ps <= sj && sj < ps + pl)
        {
            Some(&(ps, pl)) => (ps, ps + pl - 1),
            None => (sj, sj),
        }
    };
    let r = &final_routes[j];
    let dir_of = |sg: &RouteSegment| -> (f64, f64) {
        let l = (sg.end.0 - sg.start.0).hypot(sg.end.1 - sg.start.1);
        if l < 1e-9 {
            (0.0, 0.0)
        } else {
            ((sg.end.0 - sg.start.0) / l, (sg.end.1 - sg.start.1) / l)
        }
    };
    let d0 = dir_of(&r.segments[sj]);
    let collinear = |sg: &RouteSegment| -> bool {
        let d = dir_of(sg);
        (d.0 * d0.1 - d.1 * d0.0).abs() < 1e-6 && (d.0 * d0.0 + d.1 * d0.1) > 0.0
    };
    let wj = r.segments[sj].width_mm;
    let mut k0 = sj;
    while k0 > span_lo
        && close(r.segments[k0 - 1].end, r.segments[k0].start)
        && r.segments[k0 - 1].layer == layer
        && (r.segments[k0 - 1].width_mm - wj).abs() < 1e-9
        && collinear(&r.segments[k0 - 1])
    {
        k0 -= 1;
    }
    let mut k1 = sj;
    while k1 < span_hi
        && close(r.segments[k1].end, r.segments[k1 + 1].start)
        && r.segments[k1 + 1].layer == layer
        && (r.segments[k1 + 1].width_mm - wj).abs() < 1e-9
        && collinear(&r.segments[k1 + 1])
    {
        k1 += 1;
    }
    let (sa, sb) = (r.segments[k0].start, r.segments[k1].end);
    let run_len = (sb.0 - sa.0).hypot(sb.1 - sa.1);
    if run_len < 0.2 {
        return false;
    }
    // A blocker that properly CROSSES the escape chord separates the
    // endpoints on this layer — no lateral push un-crosses it (the
    // bump just ping-pongs). Only parallel squeezes are shovable.
    // (In via-siting mode from == to and there is no chord.)
    if (from.0 - to.0).hypot(from.1 - to.1) > 1e-9 {
        let orient = |p: (f64, f64), q: (f64, f64), r: (f64, f64)| -> f64 {
            (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0)
        };
        let (o1, o2) = (orient(sa, sb, from), orient(sa, sb, to));
        let (o3, o4) = (orient(from, to, sa), orient(from, to, sb));
        if o1 * o2 < 0.0 && o3 * o4 < 0.0 {
            return false;
        }
    }
    let u = ((sb.0 - sa.0) / run_len, (sb.1 - sa.1) / run_len);
    let nv = (-u.1, u.0);
    let mut t_c = run_len / 2.0;
    let mut best = f64::MAX;
    for k in 0..=64 {
        let t = run_len * k as f64 / 64.0;
        let p = (sa.0 + u.0 * t, sa.1 + u.1 * t);
        let d = geom::point_segment_dist(p, from, to);
        if d < best {
            best = d;
            t_c = t;
        }
    }
    let c = (sa.0 + u.0 * t_c, sa.1 + u.1 * t_c);
    let em = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
    let mut side = (nv.0 * (em.0 - c.0) + nv.1 * (em.1 - c.1)).signum();
    if side == 0.0 {
        side = 1.0;
    }
    let spacing = board.config.min_spacing_mm;
    // Entanglement guard: same-net branches or vias anchored ON the
    // run's INTERIOR would be disconnected by the detour. The run's
    // own endpoints stay put and are exempt.
    let anchored_inside = |p: (f64, f64)| -> bool {
        if close(p, sa) || close(p, sb) {
            return false;
        }
        geom::point_segment_dist(p, sa, sb) < wj / 2.0 + 0.05
    };
    for (sk, sg) in r.segments.iter().enumerate() {
        if (sk < k0 || sk > k1) && (anchored_inside(sg.start) || anchored_inside(sg.end)) {
            return false;
        }
    }
    if r.vias.iter().any(|v| anchored_inside((v.x, v.y))) {
        return false;
    }
    for delta in [
        escape_width / 2.0 + wj / 2.0 + 2.0 * spacing + 0.05,
        escape_width / 2.0 + wj / 2.0 + 2.0 * spacing + 0.3,
        escape_width / 2.0 + wj / 2.0 + 2.0 * spacing + 0.7,
    ] {
        let lm = delta + wj / 2.0 + escape_width / 2.0 + spacing + 0.2;
        let t1 = (t_c - lm).max(0.0);
        let t2 = (t_c + lm).min(run_len);
        let off = (nv.0 * delta * -side, nv.1 * delta * -side);
        let q1 = (sa.0 + u.0 * t1, sa.1 + u.1 * t1);
        let q2 = (sa.0 + u.0 * t2, sa.1 + u.1 * t2);
        let mut poly = vec![sa];
        if t1 > 1e-6 {
            poly.push(q1);
        }
        poly.push((q1.0 + off.0, q1.1 + off.1));
        poly.push((q2.0 + off.0, q2.1 + off.1));
        if t2 < run_len - 1e-6 {
            poly.push(q2);
        }
        poly.push(sb);
        let idx_j = geom::ClearanceIndex::build(board, final_routes, Some(bnet));
        let legal = poly.windows(2).all(|w| {
            (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1) < 1e-9
                || idx_j
                    .first_conflict(w[0], w[1], wj, layer, bnet)
                    .is_none()
        });
        if !legal {
            continue;
        }
        let new_segs: Vec<RouteSegment> = poly
            .windows(2)
            .filter(|w| (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1) > 1e-9)
            .map(|w| RouteSegment {
                layer,
                start: w[0],
                end: w[1],
                width_mm: wj,
            })
            .collect();
        let m = new_segs.len();
        let removed = k1 - k0 + 1;
        snapshots.push((j, final_routes[j].clone()));
        let rj = &mut final_routes[j];
        rj.segments.splice(k0..k1 + 1, new_segs);
        for (ps, pl) in rj.path_spans.iter_mut() {
            if *ps <= k0 && k0 < *ps + *pl {
                *pl = *pl + m - removed;
            } else if *ps > k0 {
                *ps = *ps + m - removed;
            }
        }
        info!(
            "shove: bumped a '{}' run ({removed} piece(s)) by {delta:.2}mm to open the '{}' escape",
            board.nets[j].name, board.nets[i].name
        );
        return true;
    }
    false
}

/// Find a legal via site ringing `center`, shoving foreign tracks
/// aside (exactly-gated) when a site is blocked only by copper that
/// can move. Successful shoves stay committed and their snapshots
/// accumulate in `snapshots` for the caller to revert on failure.
/// One bridge attempt for a fill-stranded pad: same-layer escape to
/// the nearest main-poly vertices first, then a VIA drop into the
/// other face's fill (dual-face pours — the ground plane is right
/// underneath). The via site must survive the swallow test on that
/// face's hole model, and the stub keeps the via pad-anchored on this
/// face so both ends carry copper.
fn island_bridge_pad(
    board: &Board,
    final_routes: &mut Vec<Route>,
    ni: usize,
    src: (f64, f64),
    main_poly: &[(f64, f64)],
    layer: usize,
    width: f64,
    far: bool,
    rip_budget: &mut usize,
) -> bool {
    let net_id = board.nets[ni].id;

    let pip_main = |x: f64, y: f64| -> bool {
        let mut inside = false;
        let m = main_poly.len();
        for k in 0..m {
            let (x1, y1) = main_poly[k];
            let (x2, y2) = main_poly[(k + 1) % m];
            if (y1 > y) != (y2 > y) && x < (x2 - x1) * (y - y1) / (y2 - y1) + x1 {
                inside = !inside;
            }
        }
        inside
    };
    // A fill vertex sits exactly ON the clearance boundary of whatever
    // carved it — landing a track end there is knife-edge illegal.
    // Nudge each candidate INTO the fill (perpendicular to the local
    // edge, direction chosen by the containment test) so the bridge
    // lands on solid copper.
    let n_verts = main_poly.len();
    let mut targets: Vec<((f64, f64), f64)> = main_poly
        .iter()
        .enumerate()
        .step_by(4)
        .filter_map(|(vi, &(vx, vy))| {
            let d = (vx - src.0).hypot(vy - src.1);
            if d >= 18.0 {
                return None;
            }
            let (px, py) = main_poly[(vi + n_verts - 1) % n_verts];
            let (nx2, ny2) = main_poly[(vi + 1) % n_verts];
            let (ex, ey) = (nx2 - px, ny2 - py);
            let el = ex.hypot(ey).max(1e-9);
            let (mut ox, mut oy) = (-ey / el * 0.4, ex / el * 0.4);
            if !pip_main(vx + ox, vy + oy) {
                (ox, oy) = (-ox, -oy);
            }
            let (tx, ty) = (vx + ox, vy + oy);
            if pip_main(tx, ty) {
                Some(((tx, ty), d))
            } else {
                Some(((vx, vy), d))
            }
        })
        .collect();
    targets.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    targets.truncate(12);
    let idx = geom::ClearanceIndex::build(board, final_routes, Some(net_id));
    for &(q, _) in &targets {
        let Some(path) = geom::route_escape(&idx, src, q, width, layer, net_id) else {
            continue;
        };
        if !path_respects_courtyards(board, &path) {
            continue;
        }
        commit_escape(
            &mut final_routes[ni],
            &path,
            layer,
            width,
            None,
            &board.nets[ni].name,
        );
        info!(
            "island bridge: '{}' island pad ({:.1},{:.1}) bridged to main fill",
            board.nets[ni].name, src.0, src.1
        );
        return true;
    }
    // Straight escapes can't thread the congestion that made the fill
    // retreat here in the first place — maze-tunnel (exact A*) the
    // nearest targets before giving up the layer.
    for &(q, _) in targets.iter().take(4) {
        let Some(path) = geom::route_tunnel(&idx, src, q, width, layer, net_id) else {
            continue;
        };
        if !path_respects_courtyards(board, &path) {
            continue;
        }
        commit_escape(
            &mut final_routes[ni],
            &path,
            layer,
            width,
            None,
            &board.nets[ni].name,
        );
        info!(
            "island bridge: '{}' island pad ({:.1},{:.1}) maze-bridged to main fill",
            board.nets[ni].name, src.0, src.1
        );
        return true;
    }
    let n_layers = board.layer_stack.layers.len();
    if n_layers >= 2 && board.nets[ni].plane_region.is_none() {
        let other = if layer == 0 { n_layers - 1 } else { 0 };
        let other_sig = board
            .layer_stack
            .layers
            .get(other)
            .map(|l| l.kind == crate::types::LayerKind::Signal)
            .unwrap_or(false);
        if other_sig {
            let via_r = board.layer_stack.via.pad_mm / 2.0;
            let merged_b = output::kicad::merge_holes(output::kicad::plane_foreign_holes_on(
                board,
                final_routes,
                net_id,
                Some(other),
                true,
            ));
            let mut vsnaps: Vec<(usize, Route)> = Vec::new();
            // Sealed pockets need room made, not found: allow several
            // neighbor-track shoves (snapshot-rolled-back on failure).
            let mut budget = 6usize;
            match claim_via_site(
                board,
                final_routes,
                ni,
                src,
                via_r,
                None,
                &mut vsnaps,
                &mut budget,
            ) {
                Some((vx, vy))
                    if !output::kicad::plane_swallows(board, &merged_b, vx, vy, via_r, None)
                        && !final_routes.iter().any(|r| {
                            r.vias.iter().any(|v| {
                                let d = (v.x - vx).hypot(v.y - vy);
                                d < board.layer_stack.via.drill_mm + 0.25
                            })
                        }) =>
                {
                    let idx = geom::ClearanceIndex::build(board, final_routes, Some(net_id));
                    if let Some(path) =
                        geom::route_escape(&idx, src, (vx, vy), width, layer, net_id)
                    {
                        commit_escape(
                            &mut final_routes[ni],
                            &path,
                            layer,
                            width,
                            Some(RouteVia {
                                x: vx,
                                y: vy,
                                from_layer: layer,
                                to_layer: other,
                            }),
                            &board.nets[ni].name,
                        );
                        info!(
                            "island bridge: '{}' island pad ({:.1},{:.1}) VIA-dropped to the {} fill",
                            board.nets[ni].name,
                            src.0,
                            src.1,
                            board
                                .layer_stack
                                .layers
                                .get(other)
                                .map(|l| l.name.as_str())
                                .unwrap_or("other")
                        );
                        return true;
                    }
                    for (jj, old) in vsnaps.drain(..).rev() {
                        final_routes[jj] = old;
                    }
                }
                _ => {
                    for (jj, old) in vsnaps.drain(..).rev() {
                        final_routes[jj] = old;
                    }
                }
            }
            // Extended via search (stranded pads only): the near ring
            // above tops out at 2.0mm — inside a sealed pocket (band
            // area, bottom-bias congestion) that's never enough.
            // Sites must land in the other face's MAIN plane fragment
            // — the secondary fill is island-removed to fragments
            // that touch same-net copper, so a via in a sliver
            // between tracks survives as an anchored island that
            // connects to nothing (measured: 11 dangling vias).
            // Approximate main-fragment membership by a coarse flood
            // fill over B free space (merged foreign holes as walls)
            // seeded from same-net THT barrels and existing vias.
            if !far {
                return false;
            }
            let cell = 0.3f64;
            let bw = board.config.outline.width();
            let bh = board.config.outline.height();
            let nx = (bw / cell).ceil() as usize + 2;
            let nyc = (bh / cell).ceil() as usize + 2;
            let mut open = vec![true; nx * nyc];
            for &(hx, hy, hr) in &merged_b {
                let rr2 = hr + 0.15;
                let x0 = (((hx - rr2) / cell).floor().max(0.0)) as usize;
                let x1 = ((((hx + rr2) / cell).ceil()) as usize).min(nx - 1);
                let y0 = (((hy - rr2) / cell).floor().max(0.0)) as usize;
                let y1 = ((((hy + rr2) / cell).ceil()) as usize).min(nyc - 1);
                for gx in x0..=x1 {
                    for gy in y0..=y1 {
                        let (cx, cy) = (gx as f64 * cell, gy as f64 * cell);
                        if (cx - hx).hypot(cy - hy) <= rr2 {
                            open[gy * nx + gx] = false;
                        }
                    }
                }
            }
            let mut region = vec![false; nx * nyc];
            let mut q: Vec<(usize, usize)> = Vec::new();
            let mut seed = |x: f64, y: f64, open: &[bool], region: &mut [bool], q: &mut Vec<(usize, usize)>| {
                let gx = (x / cell).round().max(0.0) as usize;
                let gy = (y / cell).round().max(0.0) as usize;
                if gx < nx && gy < nyc && open[gy * nx + gx] && !region[gy * nx + gx] {
                    region[gy * nx + gx] = true;
                    q.push((gx, gy));
                }
            };
            for comp in &board.components {
                let (co, sn) = (comp.theta.cos(), comp.theta.sin());
                for pin in &comp.pins {
                    if pin.net != Some(net_id) || pin.unplaced {
                        continue;
                    }
                    let Some(pad) = &pin.pad else { continue };
                    if pad.drill_mm.is_some() {
                        seed(
                            comp.x + pin.dx * co - pin.dy * sn,
                            comp.y + pin.dx * sn + pin.dy * co,
                            &open,
                            &mut region,
                            &mut q,
                        );
                    }
                }
            }
            for v in &final_routes[ni].vias {
                seed(v.x, v.y, &open, &mut region, &mut q);
            }
            while let Some((gx, gy)) = q.pop() {
                for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                    let (ax, ay) = (gx as i64 + dx, gy as i64 + dy);
                    if ax < 0 || ay < 0 || ax as usize >= nx || ay as usize >= nyc {
                        continue;
                    }
                    let ii = ay as usize * nx + ax as usize;
                    if open[ii] && !region[ii] {
                        region[ii] = true;
                        q.push((ax as usize, ay as usize));
                    }
                }
            }
            let idx2 = geom::ClearanceIndex::build(board, final_routes, Some(net_id));
            let mut cands: Vec<((f64, f64), f64)> = Vec::new();
            // Ring 0 = via-in-pad (r=0): a fully boxed-in pad's last
            // legal exit is straight down. DRC-legal; validated by
            // the same conflict/swallow/fragment tests.
            for ring in -1i32..19 {
                let rr = if ring < 0 {
                    0.0
                } else {
                    0.6 + ring as f64 * 0.45
                };
                for k in 0..12 {
                    if ring < 0 && k > 0 {
                        break;
                    }
                    let ang = k as f64 * std::f64::consts::PI / 6.0;
                    let (vx, vy) = (src.0 + rr * ang.cos(), src.1 + rr * ang.sin());
                    let gx = (vx / cell).round().max(0.0) as usize;
                    let gy = (vy / cell).round().max(0.0) as usize;
                    if gx >= nx || gy >= nyc || !region[gy * nx + gx] {
                        continue;
                    }
                    if output::kicad::plane_swallows(board, &merged_b, vx, vy, via_r, None)
                    {
                        continue;
                    }
                    if idx2.via_conflict(vx, vy, via_r, net_id).is_some() {
                        continue;
                    }
                    if final_routes.iter().any(|r| {
                        r.vias.iter().any(|v| {
                            let d = (v.x - vx).hypot(v.y - vy);
                            d < board.layer_stack.via.drill_mm + 0.25
                        })
                    }) {
                        continue;
                    }
                    cands.push(((vx, vy), rr));
                }
            }
            let far_cands = cands;
            for &((vx, vy), rr) in far_cands.iter().take(12) {
                let path = if rr <= 0.01 {
                    // via-in-pad: the span just needs real copper —
                    // a micro-stub on the pad's own footprint.
                    [(0.25f64, 0.0f64), (-0.25, 0.0), (0.0, 0.25), (0.0, -0.25)]
                        .iter()
                        .find_map(|&(dx, dy)| {
                            geom::route_escape(
                                &idx2,
                                src,
                                (src.0 + dx, src.1 + dy),
                                width,
                                layer,
                                net_id,
                            )
                        })
                } else {
                    geom::route_escape(&idx2, src, (vx, vy), width, layer, net_id)
                        .or_else(|| {
                            geom::route_tunnel(&idx2, src, (vx, vy), width, layer, net_id)
                        })
                };
                let Some(path) = path else { continue };
                if !path_respects_courtyards(board, &path) {
                    continue;
                }
                commit_escape(
                    &mut final_routes[ni],
                    &path,
                    layer,
                    width,
                    Some(RouteVia {
                        x: vx,
                        y: vy,
                        from_layer: layer,
                        to_layer: other,
                    }),
                    &board.nets[ni].name,
                );
                info!(
                    "island bridge: '{}' island pad ({:.1},{:.1}) FAR-via-dropped at ({vx:.1},{vy:.1}) to the {} fill",
                    board.nets[ni].name,
                    src.0,
                    src.1,
                    board
                        .layer_stack
                        .layers
                        .get(other)
                        .map(|l| l.name.as_str())
                        .unwrap_or("other")
                );
                return true;
            }
        }
    }
    // VICTIM RIP + BRIDGE RETRY — the router's own hammer, last tier:
    // every one-shot lever above measured illegal, so the pocket is
    // fenced by ROUTED foreign copper. Rip the fencing signal net
    // wholesale, land the bridge through the freed corridor, rebuild
    // the victim on the new board (grid + exact-gated commit + the
    // exact ladder), strict victim-no-worse-or-revert — the same
    // contract as completion's rip_and_exact_retry.
    if far && *rip_budget > 0 {
        *rip_budget -= 1;
        let snap_i = final_routes[ni].clone();
        'lines: for &(q, _) in targets.iter().take(1) {
            // NEGOTIATION LOOP along one target line: rip each new
            // blocker as it surfaces (a 15mm corridor is fenced by
            // SEVERAL nets — one rip just exposes the next), up to 4
            // victims, until the bridge lands.
            let mut snaps: Vec<(usize, Route)> = Vec::new();
            let mut landed = false;
            for _round in 0..4 {
                let idx4 =
                    geom::ClearanceIndex::build(board, final_routes, Some(net_id));
                let path = geom::route_escape(&idx4, src, q, width, layer, net_id)
                    .or_else(|| geom::route_tunnel(&idx4, src, q, width, layer, net_id));
                if let Some(path) = path {
                    if path_respects_courtyards(board, &path) {
                        commit_escape(
                            &mut final_routes[ni],
                            &path,
                            layer,
                            width,
                            None,
                            &board.nets[ni].name,
                        );
                        landed = true;
                    }
                    break;
                }
                if snaps.len() >= 3 {
                    break;
                }
                let Some(geom::Conflict::Track { net: vn, .. }) =
                    geom::escape_blocker(&idx4, src, q, width, layer, net_id)
                else {
                    break;
                };
                let Some(vj) = board.nets.iter().position(|n| n.id == vn) else {
                    break;
                };
                if board.nets[vj].plane_layer.is_some()
                    || final_routes[vj].is_empty()
                    || snaps.iter().any(|&(k, _)| k == vj)
                {
                    break;
                }
                snaps.push((vj, final_routes[vj].clone()));
                final_routes[vj] = Route::empty(final_routes[vj].net_id);
            }
            if !landed {
                for (jj, old) in snaps.drain(..).rev() {
                    final_routes[jj] = old;
                }
                final_routes[ni] = snap_i.clone();
                continue 'lines;
            }
            // Rebuild every victim on the board that carries the
            // bridge; ALL must come back no worse than before, or
            // everything reverts.
            let mut ok = true;
            for &(vj, ref snap_v) in &snaps {
                let v_before =
                    pathfinder::unreached_sink_count(&board.nets[vj], board, snap_v);
                let g_before = net_pad_group_count(board, snap_v, vj);
                let mut jgrid = RoutingGrid::build(board);
                for (k, r) in final_routes.iter().enumerate() {
                    if k != vj && !r.is_empty() {
                        pathfinder::block_route_geometry(&mut jgrid, r, board);
                    }
                }
                let mut fresh = Route::empty(snap_v.net_id);
                pathfinder::extend_route(
                    &mut jgrid, &board.nets[vj], board, &mut fresh, 1.0, 1.0, &[], &[],
                    false, None,
                );
                {
                    let mut bans = Vec::new();
                    let mut trial_board: Vec<Route> = final_routes.clone();
                    trial_board[vj] = Route::empty(snap_v.net_id);
                    exact_commit_strip(board, &trial_board, vj, &mut fresh, 0, &mut bans);
                }
                final_routes[vj] = fresh;
                if pathfinder::unreached_sink_count(
                    &board.nets[vj],
                    board,
                    &final_routes[vj],
                ) > 0
                {
                    offgrid_escape(board, final_routes, vj);
                }
                let v_after = pathfinder::unreached_sink_count(
                    &board.nets[vj],
                    board,
                    &final_routes[vj],
                );
                // KiCad-grounded acceptance: the sink counter read a
                // gutted 1-segment rebuild as complete — the pads
                // must land in no MORE connectivity groups than the
                // snapshot's.
                let g_after = net_pad_group_count(board, &final_routes[vj], vj);
                if v_after > v_before || g_after > g_before {
                    ok = false;
                    break;
                }
            }
            if ok {
                info!(
                    "island bridge: '{}' pad ({:.1},{:.1}) RIP-bridged ({} victim(s) rebuilt)",
                    board.nets[ni].name,
                    src.0,
                    src.1,
                    snaps.len()
                );
                return true;
            }
            for (jj, old) in snaps.drain(..).rev() {
                final_routes[jj] = old;
            }
            final_routes[ni] = snap_i.clone();
        }
    }
    false
}

/// Is a fill-orphaned pad already grounded through routed copper?
/// BFS over the net's segments from the pad (endpoint adjacency on a
/// 0.01mm grid, layer-aware): grounded if the chain touches ANY fill
/// poly of the net (fill floods over same-net tracks, so contact is
/// tested by dense sampling ALONG each segment, not just endpoints —
/// endpoint-only tests miss it and produced mass false "stranded"),
/// reaches a via (the other face's fill takes it), or touches a THT /
/// fill-mapped pad. A sample budget bounds the walk; exhausting it
/// counts as GROUNDED — on doubt, never bridge. Returns None when
/// grounded; when stranded, returns the chain's segment endpoints —
/// the pad may be walled in while some point along its copper still
/// has a clear shot at the fill.
fn pad_track_grounded(
    route: &Route,
    src: (f64, f64),
    polys: &[Vec<(f64, f64)>],
    layer: usize,
    grounded_pads: &[(f64, f64)],
) -> Option<Vec<(f64, f64)>> {
    let key = |x: f64, y: f64| ((x / 0.01).round() as i64, (y / 0.01).round() as i64);
    let near = |a: (f64, f64), b: (f64, f64), r: f64| (a.0 - b.0).hypot(a.1 - b.1) <= r;
    let bboxes: Vec<(f64, f64, f64, f64)> = polys
        .iter()
        .map(|p| {
            p.iter().fold(
                (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
                |(x0, y0, x1, y1), &(x, y)| (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
            )
        })
        .collect();
    let in_fill = |pt: (f64, f64)| -> bool {
        let (x, y) = pt;
        for (pi, poly) in polys.iter().enumerate() {
            let (x0, y0, x1, y1) = bboxes[pi];
            if x < x0 || x > x1 || y < y0 || y > y1 {
                continue;
            }
            let mut inside = false;
            let m = poly.len();
            for k in 0..m {
                let (px1, py1) = poly[k];
                let (px2, py2) = poly[(k + 1) % m];
                if (py1 > y) != (py2 > y) && x < (px2 - px1) * (y - py1) / (py2 - py1) + px1 {
                    inside = !inside;
                }
            }
            if inside {
                return true;
            }
        }
        false
    };
    let mut grid: crate::det::HashMap<(i64, i64, usize), Vec<usize>> = Default::default();
    for (i, sg) in route.segments.iter().enumerate() {
        for &(x, y) in &[sg.start, sg.end] {
            let (kx, ky) = key(x, y);
            grid.entry((kx, ky, sg.layer)).or_default().push(i);
        }
    }
    let mut stack: Vec<usize> = route
        .segments
        .iter()
        .enumerate()
        .filter(|(_, sg)| {
            sg.layer == layer && (near(sg.start, src, 0.8) || near(sg.end, src, 0.8))
        })
        .map(|(i, _)| i)
        .collect();
    let mut seen: crate::det::HashSet<usize> = stack.iter().copied().collect();
    let mut budget = 20_000usize;
    let mut endpoints: Vec<(f64, f64)> = Vec::new();
    while let Some(i) = stack.pop() {
        let sg = &route.segments[i];
        // Fill contact anywhere along the copper, 0.3mm steps.
        let len = (sg.end.0 - sg.start.0).hypot(sg.end.1 - sg.start.1);
        let steps = ((len / 0.3).ceil() as usize).max(1);
        for s in 0..=steps {
            let t = s as f64 / steps as f64;
            let pt = (
                sg.start.0 + (sg.end.0 - sg.start.0) * t,
                sg.start.1 + (sg.end.1 - sg.start.1) * t,
            );
            if in_fill(pt) {
                return None;
            }
            budget = budget.saturating_sub(1);
            if budget == 0 {
                return None;
            }
        }
        for &pt in &[sg.start, sg.end] {
            if route.vias.iter().any(|v| near((v.x, v.y), pt, 0.05)) {
                return None;
            }
            if grounded_pads.iter().any(|&p| near(p, pt, 0.8)) {
                return None;
            }
            if sg.layer == layer && !endpoints.iter().any(|&e| near(e, pt, 0.05)) {
                endpoints.push(pt);
            }
            let (kx, ky) = key(pt.0, pt.1);
            for dx in -1..=1i64 {
                for dy in -1..=1i64 {
                    let Some(cands) = grid.get(&(kx + dx, ky + dy, sg.layer)) else {
                        continue;
                    };
                    for &j in cands {
                        if seen.contains(&j) {
                            continue;
                        }
                        let o = &route.segments[j];
                        if near(o.start, pt, 0.011) || near(o.end, pt, 0.011) {
                            seen.insert(j);
                            stack.push(j);
                        }
                    }
                }
            }
        }
    }
    Some(endpoints)
}

fn claim_via_site(
    board: &Board,
    final_routes: &mut [Route],
    i: usize,
    center: (f64, f64),
    via_r: f64,
    avoid: Option<(f64, f64)>,
    snapshots: &mut Vec<(usize, Route)>,
    shove_budget: &mut usize,
) -> Option<(f64, f64)> {
    let net_id = board.nets[i].id;
    let hole_gap = board.layer_stack.via.drill_mm + 0.25;
    for ring in 0..5 {
        let rr = 0.6 + ring as f64 * 0.35;
        for k in 0..8 {
            let ang = k as f64 * std::f64::consts::FRAC_PI_4;
            let (vx, vy) = (center.0 + rr * ang.cos(), center.1 + rr * ang.sin());
            if let Some(av) = avoid {
                if (vx - av.0).hypot(vy - av.1)
                    < hole_gap.max(2.0 * via_r + board.config.min_spacing_mm)
                {
                    continue;
                }
            }
            let mark = snapshots.len();
            let mut ok = false;
            for _ in 0..3 {
                let idx =
                    geom::ClearanceIndex::build(board, final_routes, Some(net_id));
                match idx.via_conflict(vx, vy, via_r, net_id) {
                    None => {
                        ok = true;
                        break;
                    }
                    Some(c @ geom::Conflict::Track { .. }) if *shove_budget > 0 => {
                        if !try_shove_track(
                            board,
                            final_routes,
                            i,
                            &c,
                            (vx, vy),
                            (vx, vy),
                            2.0 * via_r,
                            snapshots,
                        ) {
                            break;
                        }
                        *shove_budget -= 1;
                    }
                    Some(_) => break,
                }
            }
            if ok {
                return Some((vx, vy));
            }
            while snapshots.len() > mark {
                let (jj, old) = snapshots.pop().unwrap();
                final_routes[jj] = old;
            }
        }
    }
    None
}

/// Connect each unreached pad of net `i` to its nearest tree copper
/// with geom::route_escape. Returns sinks gained.
/// 5.995 PLANE SURFACE RESCUE: a plane net's pad with no legal drop
/// site (out of its split-region band, or walled in) has NO
/// mechanism at all — the escape ladder skips plane nets. Connect it
/// on its own surface layer to the nearest same-net copper (another
/// pad's drop stub or via) with the exact router. The plane carries
/// it from there.
/// FANOUT DISCIPLINE for plane-net rescue legs: F.Cu inside an IC's
/// courtyard belongs to that IC's own fanout. A rescue path may dip
/// into a courtyard near its endpoints (a pad fans out through its
/// own body bubble; an attach lands on copper beside another pad)
/// but may not TRANSIT a body interior — the measured failure is a
/// VCC Z-leg cutting through the QFN32 body and boxing UGND with no
/// via site left anywhere (uno free-MCU s42/s7). ICs only (>=8
/// pins); crossing an 0603's courtyard is harmless and common.
fn path_respects_courtyards(board: &Board, path: &[(f64, f64)]) -> bool {
    if path.len() < 2 {
        return true;
    }
    let src = path[0];
    let dst = *path.last().unwrap();
    let mut pts: Vec<(f64, f64)> = Vec::new();
    for w in path.windows(2) {
        pts.push(w[0]);
        pts.push(((w[0].0 + w[1].0) / 2.0, (w[0].1 + w[1].1) / 2.0));
    }
    pts.push(dst);
    for comp in &board.components {
        if comp.pins.len() < 8 {
            continue;
        }
        let quarter =
            ((comp.theta / std::f64::consts::FRAC_PI_2).round() as i64).rem_euclid(2);
        let (w, h) = if quarter == 1 {
            (comp.height_mm, comp.width_mm)
        } else {
            (comp.width_mm, comp.height_mm)
        };
        let (hx, hy) = (w / 2.0 - 0.3, h / 2.0 - 0.3);
        if hx <= 0.0 || hy <= 0.0 {
            continue;
        }
        for &p in &pts {
            if (p.0 - comp.x).abs() < hx && (p.1 - comp.y).abs() < hy {
                let near_end = (p.0 - src.0).hypot(p.1 - src.1) < 2.0
                    || (p.0 - dst.0).hypot(p.1 - dst.1) < 1.0;
                if !near_end {
                    debug!(
                        "courtyard discipline: rejected leg ({:.2},{:.2})->({:.2},{:.2}) transiting '{}' at ({:.2},{:.2})",
                        src.0, src.1, dst.0, dst.1, comp.refdes, p.0, p.1
                    );
                    return false;
                }
            }
        }
    }
    true
}


/// Free-end trim for REGIONED-pour nets: iteratively remove segments
/// with a genuinely unanchored endpoint (no same-layer segment
/// centerline, no via, no own pad). Validator amputations and rescue
/// stubs leave such spurs with cleared span bookkeeping, invisible
/// to the span-based sweep. Small routes only (pocket stubs + hops),
/// so the quadratic scan is cheap.
/// KiCad-grounded connectivity: the number of GROUPS the net's pads
/// land in (segments joined by genuine copper overlap on a layer,
/// vias across layers; a pad touching no copper is its own group).
/// 1 = electrically whole. The victim-rebuild acceptance test —
/// unreached_sink_count read a gutted 1-segment net as complete
/// (ch2_vtap shipped as a 0.13mm sliver).
fn net_pad_group_count(board: &Board, route: &Route, ni: usize) -> usize {
    let via_r = board.layer_stack.via.pad_mm / 2.0;
    let segs = &route.segments;
    let n = segs.len();
    let mut par: Vec<usize> = (0..n).collect();
    fn find(par: &mut Vec<usize>, mut a: usize) -> usize {
        while par[a] != a {
            par[a] = par[par[a]];
            a = par[a];
        }
        a
    }
    for a in 0..n {
        for b in (a + 1)..n {
            if segs[a].layer != segs[b].layer {
                continue;
            }
            let tol = (segs[a].width_mm + segs[b].width_mm) / 2.0 - 1e-3;
            let touch = geom::segment_point_too_close(segs[b].start, segs[b].end, segs[a].start, tol)
                || geom::segment_point_too_close(segs[b].start, segs[b].end, segs[a].end, tol)
                || geom::segment_point_too_close(segs[a].start, segs[a].end, segs[b].start, tol)
                || geom::segment_point_too_close(segs[a].start, segs[a].end, segs[b].end, tol);
            if touch {
                let (ra, rb) = (find(&mut par, a), find(&mut par, b));
                par[ra] = rb;
            }
        }
    }
    for v in &route.vias {
        let mut first: Option<usize> = None;
        for (i, sg) in segs.iter().enumerate() {
            if (sg.start.0 - v.x).hypot(sg.start.1 - v.y) <= via_r + 1e-3
                || (sg.end.0 - v.x).hypot(sg.end.1 - v.y) <= via_r + 1e-3
            {
                match first {
                    None => first = Some(i),
                    Some(f) => {
                        let (ra, rb) = (find(&mut par, f), find(&mut par, i));
                        par[ra] = rb;
                    }
                }
            }
        }
    }
    let comp_pos: crate::det::HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(k, c)| (c.id, k))
        .collect();
    let mut groups: Vec<Option<usize>> = Vec::new();
    for &(cid, pid) in &board.nets[ni].pins {
        let Some(&ci) = comp_pos.get(&cid) else { continue };
        let comp = &board.components[ci];
        let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pid) else {
            continue;
        };
        if pin.unplaced {
            continue;
        }
        let (co, sn) = (comp.theta.cos(), comp.theta.sin());
        let gx = comp.x + pin.dx * co - pin.dy * sn;
        let gy = comp.y + pin.dx * sn + pin.dy * co;
        let half = pin
            .pad
            .as_ref()
            .map(|p| p.width_mm.max(p.height_mm) / 2.0)
            .unwrap_or(0.4);
        let mut g: Option<usize> = None;
        for (i, sg) in segs.iter().enumerate() {
            if geom::segment_point_too_close(
                sg.start,
                sg.end,
                (gx, gy),
                half + sg.width_mm / 2.0 - 1e-3,
            ) {
                g = Some(find(&mut par, i));
                break;
            }
        }
        groups.push(g);
    }
    let mut distinct: Vec<Option<usize>> = Vec::new();
    let mut count = 0usize;
    for g in groups {
        match g {
            None => count += 1, // bare pad = its own group
            Some(r) => {
                if !distinct.contains(&Some(r)) {
                    distinct.push(Some(r));
                    count += 1;
                }
            }
        }
    }
    count.max(1)
}



/// Split the segment whose INTERIOR carries point q into two halves
/// ending exactly at q — a route landing mid-span is electrically
/// connected, but every dangling/stub judge (KiCad's and ours)
/// demands a REAL junction (the d37c674 T-split lesson). Committed
/// joins must split their host AT COMMIT TIME: the 5.996 span trim
/// runs before the later T-splitting passes and ate a via hop's
/// 17mm B-leg as an unanchored stub span.
fn t_split_host_at(route: &mut Route, q: (f64, f64), layer: usize) {
    for k in 0..route.segments.len() {
        let sg = &route.segments[k];
        if sg.layer != layer {
            continue;
        }
        if geom::point_segment_dist(q, sg.start, sg.end) > 0.01 {
            continue;
        }
        if (q.0 - sg.start.0).hypot(q.1 - sg.start.1) < 0.01
            || (q.0 - sg.end.0).hypot(q.1 - sg.end.1) < 0.01
        {
            return; // already lands on a junction
        }
        let (start, end, w, l) = (sg.start, sg.end, sg.width_mm, sg.layer);
        route.segments[k] = RouteSegment {
            layer: l,
            start,
            end: q,
            width_mm: w,
        };
        route.segments.push(RouteSegment {
            layer: l,
            start: q,
            end,
            width_mm: w,
        });
        return;
    }
}


/// TRULY-FINAL free-end trim for SIGNAL nets. The 5.996 "final"
/// orphan sweep is not last: the 5.997 post-sweep plane rescue runs
/// after it and its Stage-1 shove DEFORMS foreign copper — a lateral
/// bump whose return rung is orphaned when a later shove displaces
/// the run the rung aimed at (seed-7: two 0.5mm rungs dead-ending in
/// space, the shipped track_dangling pair). Rather than teach every
/// deformer to clean up after the ones that follow it, sweep once at
/// the true end: a segment endpoint anchored on NOTHING — no pad, no
/// via, no other segment's centerline — dangles per KiCad, whoever
/// left it. Signal nets only: a board-wide ground's thousands of
/// segments make the per-removal rescan intractable, and its classes
/// are covered by the span machinery.

/// A routed leg whose end stops a hair short of the same-net track it
/// was meant to T into is a router seam, not a stub — KiCad grades it
/// track_dangling and the free-end trim would CUT a live leg. Close
/// the seam with the missing sliver instead.
///
/// The target model is the whole thing (a first cut snapped an end to
/// its own colinear neighbour and shipped 2v):
///   * candidates exclude the segment itself AND any segment sharing
///     the free endpoint — those are the same chain, not a target;
///   * the sliver must land on the target's INTERIOR (a genuine T),
///     never at its endpoint (that is a corner of the same chain);
///   * the sliver must be at least half a width long (else the end was
///     already touching) and at most 2.5 widths (else it is not a
///     seam, it is a gap the router meant).
/// Layer-aware pad anchors, same as the trim.
fn signal_seam_close(board: &Board, final_routes: &mut Vec<Route>) -> usize {
    let via_r = board.layer_stack.via.pad_mm / 2.0;
    let mut closed = 0usize;
    for i in 0..board.nets.len() {
        if board.nets[i].plane_layer.is_some() || final_routes[i].segments.is_empty() {
            continue;
        }
        let mut own_pads: Vec<(f64, f64, f64, f64, bool, usize)> = Vec::new();
        for comp in &board.components {
            let (co, sn) = (comp.theta.cos(), comp.theta.sin());
            let quarter =
                ((comp.theta / std::f64::consts::FRAC_PI_2).round() as i64).rem_euclid(2);
            for pin in &comp.pins {
                if pin.net != Some(board.nets[i].id) || pin.unplaced {
                    continue;
                }
                let gx = comp.x + pin.dx * co - pin.dy * sn;
                let gy = comp.y + pin.dx * sn + pin.dy * co;
                let (pw, ph) = match &pin.pad {
                    Some(pd) => (pd.width_mm, pd.height_mm),
                    None => (0.5, 0.5),
                };
                let (pw, ph) = if quarter == 1 { (ph, pw) } else { (pw, ph) };
                let tht = pin.pad.as_ref().map_or(false, |pd| pd.drill_mm.is_some());
                let face = match comp.side {
                    BoardSide::Top => 0usize,
                    BoardSide::Bottom => board.layer_stack.layers.len().saturating_sub(1),
                };
                own_pads.push((gx, gy, pw / 2.0, ph / 2.0, tht, face));
            }
        }
        let mut adds: Vec<RouteSegment> = Vec::new();
        {
            let r = &final_routes[i];
            for (sk, sg) in r.segments.iter().enumerate() {
                for &e in &[sg.start, sg.end] {
                    let shares = |s2: &RouteSegment| {
                        (s2.start.0 - e.0).hypot(s2.start.1 - e.1) <= 0.011
                            || (s2.end.0 - e.0).hypot(s2.end.1 - e.1) <= 0.011
                    };
                    let anchored = r.segments.iter().enumerate().any(|(sj, s2)| {
                        sj != sk
                            && s2.layer == sg.layer
                            && !shares(s2)
                            && geom::point_segment_dist(e, s2.start, s2.end) <= 0.05
                    }) || r.segments.iter().enumerate().any(|(sj, s2)| {
                        // Continuation of the same chain: not free.
                        sj != sk && s2.layer == sg.layer && shares(s2)
                    }) || r
                        .vias
                        .iter()
                        .any(|v| (v.x - e.0).hypot(v.y - e.1) <= via_r)
                        || own_pads.iter().any(|&(cx, cy, hx, hy, tht, face)| {
                            (tht || face == sg.layer)
                                && (e.0 - cx).abs() <= hx
                                && (e.1 - cy).abs() <= hy
                        });
                    if anchored {
                        continue;
                    }
                    let mut best: Option<((f64, f64), f64)> = None;
                    for (sj, s2) in r.segments.iter().enumerate() {
                        if sj == sk || s2.layer != sg.layer || shares(s2) {
                            continue;
                        }
                        let (ax, ay) = s2.start;
                        let (bx, by) = s2.end;
                        let (dx, dy) = (bx - ax, by - ay);
                        let l2 = dx * dx + dy * dy;
                        if l2 <= 1e-12 {
                            continue;
                        }
                        let t = ((e.0 - ax) * dx + (e.1 - ay) * dy) / l2;
                        // A genuine T lands on the target's INTERIOR — or
                        // on a JUNCTION where the target continues into
                        // another segment (copper runs straight through
                        // it). A lone chain END is a corner of the same
                        // chain and never a target. Measured: the seam
                        // sat exactly above the junction of two
                        // collinear track pieces; interior-only rejected
                        // it.
                        let tc = t.clamp(0.0, 1.0);
                        let q = (ax + tc * dx, ay + tc * dy);
                        if t <= 0.05 || t >= 0.95 {
                            let junction = r.segments.iter().enumerate().any(|(sm, s3)| {
                                sm != sj
                                    && sm != sk
                                    && s3.layer == sg.layer
                                    && ((s3.start.0 - q.0).hypot(s3.start.1 - q.1) <= 0.011
                                        || (s3.end.0 - q.0).hypot(s3.end.1 - q.1) <= 0.011)
                            });
                            if !junction {
                                continue;
                            }
                        }
                        let d = (q.0 - e.0).hypot(q.1 - e.1);
                        if d >= sg.width_mm * 0.5 && d <= sg.width_mm * 2.5 + 0.1 {
                            if best.map_or(true, |(_, bd)| d < bd) {
                                best = Some((q, d));
                            }
                        }
                    }
                    if let Some((q, _)) = best {
                        adds.push(RouteSegment {
                            layer: sg.layer,
                            start: e,
                            end: q,
                            width_mm: sg.width_mm,
                        });
                    }
                }
            }
        }
        if !adds.is_empty() {
            let route = &mut final_routes[i];
            for a in adds {
                info!(
                    "seam close: '{}' l{} ({:.2},{:.2}) -> ({:.2},{:.2})",
                    board.nets[i].name, a.layer, a.start.0, a.start.1, a.end.0, a.end.1
                );
                let seg_start = route.segments.len();
                route.segments.push(a);
                route.path_spans.push((seg_start, 1));
                route.path_parents.push(None);
                closed += 1;
            }
        }
    }
    closed
}

fn signal_free_end_trim(board: &Board, final_routes: &mut [Route]) -> usize {
    let via_r = board.layer_stack.via.pad_mm / 2.0;
    let mut pruned = 0usize;
    for i in 0..board.nets.len() {
        if board.nets[i].plane_layer.is_some() || final_routes[i].segments.is_empty() {
            continue;
        }
        let mut own_pads: Vec<(f64, f64, f64, f64, bool, usize)> = Vec::new();
        for comp in &board.components {
            let (co, sn) = (comp.theta.cos(), comp.theta.sin());
            let quarter =
                ((comp.theta / std::f64::consts::FRAC_PI_2).round() as i64).rem_euclid(2);
            for pin in &comp.pins {
                if pin.net != Some(board.nets[i].id) || pin.unplaced {
                    continue;
                }
                let gx = comp.x + pin.dx * co - pin.dy * sn;
                let gy = comp.y + pin.dx * sn + pin.dy * co;
                let (pw, ph) = match &pin.pad {
                    Some(pd) => (pd.width_mm, pd.height_mm),
                    None => (0.5, 0.5),
                };
                let (pw, ph) = if quarter == 1 { (ph, pw) } else { (pw, ph) };
                // WHICH LAYERS this pad's copper is on: a THT pad is on
                // every layer, an SMD pad only on its component's face.
                // The anchor test was layer-blind and called a B.Cu
                // free end "anchored" by an F.Cu-only SMD pad it can
                // never touch (measured: U25.IN_A_N, 1.85mm wide, over
                // a B.Cu leg start 0.30mm shy of its T — shipped as
                // track_dangling on the rigid mixer, and the near-miss
                // close never fired because the end read "anchored").
                let tht = pin.pad.as_ref().map_or(false, |pd| pd.drill_mm.is_some());
                let face = match comp.side {
                    BoardSide::Top => 0usize,
                    BoardSide::Bottom => board.layer_stack.layers.len().saturating_sub(1),
                };
                own_pads.push((gx, gy, pw / 2.0, ph / 2.0, tht, face));
            }
        }
        loop {
            let r = &final_routes[i];
            let mut drop: Option<usize> = None;
            'segs: for (sk, sg) in r.segments.iter().enumerate() {
                for &e in &[sg.start, sg.end] {
                    let anchored = r.segments.iter().enumerate().any(|(sj, s2)| {
                        sj != sk
                            && s2.layer == sg.layer
                            && geom::point_segment_dist(e, s2.start, s2.end) <= 0.05
                    }) || r
                        .vias
                        .iter()
                        .any(|v| (v.x - e.0).hypot(v.y - e.1) <= via_r)
                        || own_pads.iter().any(|&(cx, cy, hx, hy, tht, face)| {
                            (tht || face == sg.layer)
                                && (e.0 - cx).abs() <= hx
                                && (e.1 - cy).abs() <= hy
                        });
                    if !anchored {
                        drop = Some(sk);
                        break 'segs;
                    }
                }
            }
            match drop {
                Some(sk) => {
                    let r = &mut final_routes[i];
                    r.segments.remove(sk);
                    for (qs, ql) in r.path_spans.iter_mut() {
                        if *qs > sk {
                            *qs -= 1;
                        } else if sk < *qs + *ql {
                            *ql = ql.saturating_sub(1);
                        }
                    }
                    pruned += 1;
                }
                None => break,
            }
        }
    }
    pruned
}

/// PROBE census: vias with no segment contact on one of their
/// layers — the shipping via_dangling shape. Log-only.
fn probe_dangling_vias(board: &Board, final_routes: &[Route], tag: &str) {
    if std::env::var("BHDL_PNR_PROBE").is_err() {
        return;
    }
    let via_r = board.layer_stack.via.pad_mm / 2.0;
    for (ni, r) in final_routes.iter().enumerate() {
        for v in &r.vias {
            let touch = |layer: usize| {
                r.segments.iter().any(|sg| {
                    sg.layer == layer
                        && ((sg.start.0 - v.x).hypot(sg.start.1 - v.y) <= via_r + 0.05
                            || (sg.end.0 - v.x).hypot(sg.end.1 - v.y) <= via_r + 0.05)
                })
            };
            let f = touch(v.from_layer);
            let b = touch(v.to_layer);
            if !f || !b {
                log::info!(
                    "[probe] census {tag}: '{}' via ({:.3},{:.3}) contact from={} to={}",
                    board.nets.get(ni).map(|n| n.name.as_str()).unwrap_or("?"),
                    v.x, v.y, f, b
                );
            }
        }
    }
}

/// SIGNAL-NET CONTINUITY REPAIR: mirror KiCad's connectivity grouping
/// per ROUTED net — segments joined by geometric overlap (same layer)
/// and by vias across layers, pads joined by contact — and when a
/// net's pads land in MORE THAN ONE group, route the gap between the
/// two nearest group endpoints. A validator amputation with stale
/// span bookkeeping can strand half a routed tree while the sink
/// counter still reads complete (ch3_a_out shipped in two pieces,
/// 3.7mm apart, invisible to every existing check).
/// A pour is an OPTIMIZATION, not a connectivity guarantee — and
/// signal_net_continuity_repair skips every net that HAS a plane_layer
/// (`plane_layer.is_some() => continue`), so a pour net's pad that the
/// fill never reaches had no pass looking after it at all.
///
/// Measured on the mixer at seed 7: vbias pad U20.IN_B_P at
/// (23.88,90.74) sits outside all three of its net's F.Cu fill
/// fragments with its nearest same-net track 4.825mm away, while its
/// sibling 4.95mm along is track-connected at 0.000. The engine was
/// honest about it — "pour defects (trial currency): 5", all five
/// tiers run — it simply had no rung left to try.
///
/// Serve exactly those pads: NEITHER fill-contained NOR touching
/// copper, routed to the nearest same-net copper with the ordinary
/// escape/maze ladder. Runs last, on the copper actually being
/// shipped, so it judges the real fill rather than an early model.
/// Join a live-but-severed piece of pour to the main body with ONE
/// stitching via, where the other face's main plane runs beneath it.
///
/// The pad-level ladders cannot do this: measured (jack board, seed
/// 99), a 3224-vertex F.Cu GND fragment with two pads, four tracks
/// and a B.Cu partner was severed from the main pour, and every pad
/// inside it failed "no legal bridge" — each pad's LOCAL neighbourhood
/// is sealed, which is HOW the piece got severed. But the component
/// itself sits on top of the whole-board B.Cu plane: one via anywhere
/// inside the fragment where the far plane covers joins everything.
///
/// Connectivity-gated, not shape-gated: a fragment that already
/// reaches the main body through tracks or vias (the ecc83 organic
/// split serves fragments by TRACK on purpose) gets no via. Only a
/// component that provably reaches nothing is stitched.
/// Which fill fragments of this pour net fail to reach the main body
/// through ANY copper — the emission-model ground truth the raster
/// currency cannot see (measured: a 3224-vertex severed assembly
/// registered zero raster defects while KiCad reported the island).
fn pour_severed_fragments(
    board: &Board,
    final_routes: &[Route],
    ni: usize,
    polys: &[Vec<(f64, f64)>],
) -> Vec<usize> {
    let via_r = board.layer_stack.via.pad_mm / 2.0;
    let Some(pl) = board.nets[ni].plane_layer else {
        return Vec::new();
    };
    let np = polys.len();
    if np < 2 {
        return Vec::new();
    }
    let Some(main_pi) = polys
        .iter()
        .enumerate()
        .max_by_key(|(_, p)| p.len())
        .map(|(i, _)| i)
    else {
        return Vec::new();
    };
    let inside = |poly: &[(f64, f64)], x: f64, y: f64| -> bool {
        let mut ins = false;
        let m = poly.len();
        for k in 0..m {
            let (x1, y1) = poly[k];
            let (x2, y2) = poly[(k + 1) % m];
            if (y1 > y) != (y2 > y) && x < (x2 - x1) * (y - y1) / (y2 - y1) + x1 {
                ins = !ins;
            }
        }
        ins
    };
    let segs = &final_routes[ni].segments;
    let vias: Vec<(f64, f64)> =
        final_routes[ni].vias.iter().map(|v| (v.x, v.y)).collect();
    let (nseg, nv) = (segs.len(), vias.len());
    let mut dsu: Vec<usize> = (0..nseg + nv + np).collect();
    for i in 0..nseg {
        for j in (i + 1)..nseg {
            if segs[i].layer != segs[j].layer {
                continue;
            }
            let close =
                |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).hypot(a.1 - b.1) <= 0.011;
            if close(segs[i].start, segs[j].start)
                || close(segs[i].start, segs[j].end)
                || close(segs[i].end, segs[j].start)
                || close(segs[i].end, segs[j].end)
                || geom::segment_point_too_close(
                    segs[j].start,
                    segs[j].end,
                    segs[i].start,
                    (segs[i].width_mm + segs[j].width_mm) / 2.0 + 0.01,
                )
                || geom::segment_point_too_close(
                    segs[j].start,
                    segs[j].end,
                    segs[i].end,
                    (segs[i].width_mm + segs[j].width_mm) / 2.0 + 0.01,
                )
            {
                dsu_union(&mut dsu, i, j);
            }
        }
    }
    for (vi, &(vx, vy)) in vias.iter().enumerate() {
        for (i, sg) in segs.iter().enumerate() {
            if (sg.start.0 - vx).hypot(sg.start.1 - vy) <= via_r + 0.05
                || (sg.end.0 - vx).hypot(sg.end.1 - vy) <= via_r + 0.05
                || geom::segment_point_too_close(
                    sg.start,
                    sg.end,
                    (vx, vy),
                    via_r + sg.width_mm / 2.0,
                )
            {
                dsu_union(&mut dsu, nseg + vi, i);
            }
        }
        for (pi, poly) in polys.iter().enumerate() {
            if inside(poly, vx, vy) {
                dsu_union(&mut dsu, nseg + vi, nseg + nv + pi);
            }
        }
    }
    for (i, sg) in segs.iter().enumerate() {
        if sg.layer != pl {
            continue;
        }
        let len = (sg.end.0 - sg.start.0).hypot(sg.end.1 - sg.start.1);
        let steps = ((len / 0.3).ceil() as usize).max(1);
        for st in 0..=steps {
            let t = st as f64 / steps as f64;
            let (x, y) = (
                sg.start.0 + (sg.end.0 - sg.start.0) * t,
                sg.start.1 + (sg.end.1 - sg.start.1) * t,
            );
            for (pi, poly) in polys.iter().enumerate() {
                if inside(poly, x, y) {
                    dsu_union(&mut dsu, i, nseg + nv + pi);
                }
            }
        }
    }
    let main_root = dsu_find(&mut dsu, nseg + nv + main_pi);
    (0..np)
        .filter(|&pi| pi != main_pi && dsu_find(&mut dsu, nseg + nv + pi) != main_root)
        .collect()
}

fn pour_severed_component_stitch(board: &Board, final_routes: &mut Vec<Route>) -> usize {
    let n_layers = board.layer_stack.layers.len();
    if n_layers < 2 {
        return 0;
    }
    let via_r = board.layer_stack.via.pad_mm / 2.0;
    let mut stitched = 0usize;
    for ni in 0..board.nets.len() {
        let Some(pl) = board.nets[ni].plane_layer else {
            continue;
        };
        let other = if pl == 0 { n_layers - 1 } else { 0 };
        let other_sig = board
            .layer_stack
            .layers
            .get(other)
            .map(|l| l.kind == crate::types::LayerKind::Signal)
            .unwrap_or(false);
        if !other_sig {
            continue;
        }
        let net_id = board.nets[ni].id;
        let Some(polys) = output::kicad::emission_fill_polys(board, final_routes, ni)
        else {
            continue;
        };
        if polys.len() < 2 {
            continue;
        }
        let Some(main_pi) = polys
            .iter()
            .enumerate()
            .max_by_key(|(_, p)| p.len())
            .map(|(i, _)| i)
        else {
            continue;
        };
        let inside = |poly: &[(f64, f64)], x: f64, y: f64| -> bool {
            let mut ins = false;
            let m = poly.len();
            for k in 0..m {
                let (x1, y1) = poly[k];
                let (x2, y2) = poly[(k + 1) % m];
                if (y1 > y) != (y2 > y) && x < (x2 - x1) * (y - y1) / (y2 - y1) + x1 {
                    ins = !ins;
                }
            }
            ins
        };
        let severed = pour_severed_fragments(board, final_routes, ni, &polys);
        if severed.is_empty() {
            continue;
        }
        // Other-face model: the EMITTED secondary fill, not the hole
        // model. plane_swallows answers "would the plane cover this
        // point", which is OPTIMISTIC about what island removal keeps
        // — the first stitch attempt used it and dropped its via into
        // the severed assembly's OWN B.Cu partner, joining the island
        // to itself (and this project has made that exact mistake
        // before; it is in the ledger). Only membership in the far
        // face's MAIN emitted fragment proves the via lands on copper
        // that goes somewhere. Computed lazily — the full emission is
        // ~1s per fill and severed fragments are rare.
        let mut far_main: Option<Option<Vec<(f64, f64)>>> = None;
        for pi in severed {
            let fm = far_main
                .get_or_insert_with(|| {
                    let (_, fills) =
                        output::kicad::export_kicad_pcb_with_fills(board, final_routes);
                    fills
                        .zones
                        .iter()
                        .filter(|z| z.net_id == net_id && z.layer == other)
                        .flat_map(|z| z.polys.iter())
                        .max_by_key(|p| p.len())
                        .cloned()
                })
                .clone();
            let Some(far_poly) = fm else {
                log::warn!(
                    "severed-component stitch: '{}' has no emitted {} fill — cannot stitch (honest)",
                    board.nets[ni].name,
                    board
                        .layer_stack
                        .layers
                        .get(other)
                        .map(|l| l.name.as_str())
                        .unwrap_or("far")
                );
                break;
            };
            // First legal site scanning the fragment row-major: the
            // via disc fully inside this fragment, landing in the
            // other face's main plane, drill-rule clear of every
            // existing hole. Deterministic by construction.
            let (mut fx0, mut fy0, mut fx1, mut fy1) =
                (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
            for &(x, y) in &polys[pi] {
                fx0 = fx0.min(x);
                fy0 = fy0.min(y);
                fx1 = fx1.max(x);
                fy1 = fy1.max(y);
            }
            let mut placed = false;
            let step = 0.5f64;
            let margin = via_r + 0.15;
            let mut y = fy0 + margin;
            'scan: while y <= fy1 - margin {
                let mut x = fx0 + margin;
                while x <= fx1 - margin {
                    let disc_ok = (0..8).all(|k| {
                        let a = k as f64 * std::f64::consts::FRAC_PI_4;
                        inside(&polys[pi], x + margin * a.cos(), y + margin * a.sin())
                    });
                    let far_ok = disc_ok
                        && (0..8).all(|k| {
                            let a = k as f64 * std::f64::consts::FRAC_PI_4;
                            inside(
                                &far_poly,
                                x + margin * a.cos(),
                                y + margin * a.sin(),
                            )
                        });
                    if far_ok
                        && !final_routes.iter().any(|r| {
                            r.vias.iter().any(|v| {
                                (v.x - x).hypot(v.y - y)
                                    < board.layer_stack.via.drill_mm + 0.25
                            })
                        })
                        && !board.components.iter().any(|c| {
                            let (co, sn) = (c.theta.cos(), c.theta.sin());
                            c.pins.iter().any(|pin| {
                                pin.pad
                                    .as_ref()
                                    .and_then(|pd| pd.drill_mm)
                                    .map_or(false, |d| {
                                        let gx = c.x + pin.dx * co - pin.dy * sn;
                                        let gy = c.y + pin.dx * sn + pin.dy * co;
                                        (gx - x).hypot(gy - y)
                                            < (d + board.layer_stack.via.drill_mm) / 2.0
                                                + 0.25
                                    })
                            })
                        })
                    {
                        final_routes[ni].vias.push(RouteVia {
                            x,
                            y,
                            from_layer: pl.min(other),
                            to_layer: pl.max(other),
                        });
                        info!(
                            "severed-component stitch: '{}' fragment ({} verts) joined to the {} plane by a via at ({x:.2},{y:.2})",
                            board.nets[ni].name,
                            polys[pi].len(),
                            board
                                .layer_stack
                                .layers
                                .get(other)
                                .map(|l| l.name.as_str())
                                .unwrap_or("other")
                        );
                        stitched += 1;
                        placed = true;
                        break 'scan;
                    }
                    x += step;
                }
                y += step;
            }
            if !placed {
                // PERIMETER escape: no far-plane coverage means a via
                // cannot fix this piece — but the pad-local searches
                // that failed here started from a handful of sealed
                // pads, and the fragment's own boundary offers
                // hundreds of departure points. Try a same-face track
                // from perimeter vertices to the nearest main-fill
                // vertices; a track touching both fills merges them
                // (same-net contact is connection).
                let main_poly = &polys[main_pi];
                // Candidate pairs: subsampled perimeter vertices x
                // their nearest main vertices, tried nearest-first.
                let mut pairs: Vec<((f64, f64), (f64, f64), f64)> = Vec::new();
                let stride = (polys[pi].len() / 48).max(1);
                for (k, &(sx, sy)) in polys[pi].iter().enumerate() {
                    if k % stride != 0 {
                        continue;
                    }
                    let mut best: Option<((f64, f64), f64)> = None;
                    for &(mx, my) in main_poly.iter() {
                        let d = (mx - sx).hypot(my - sy);
                        if best.map_or(true, |(_, bd)| d < bd) {
                            best = Some(((mx, my), d));
                        }
                    }
                    if let Some((m, d)) = best {
                        if d <= 20.0 {
                            pairs.push(((sx, sy), m, d));
                        }
                    }
                }
                pairs.sort_by(|a, b| {
                    a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal)
                });
                let width = board.config.min_trace_width_mm;
                let idx = geom::ClearanceIndex::build(board, final_routes, Some(net_id));
                let mut joined_track = false;
                for &(src, dst, d) in pairs.iter().take(24) {
                    let path = geom::route_escape(&idx, src, dst, width, pl, net_id)
                        .or_else(|| {
                            geom::route_tunnel(&idx, src, dst, width, pl, net_id)
                        });
                    if let Some(path) = path {
                        let route = &mut final_routes[ni];
                        let seg_start = route.segments.len();
                        for w in path.windows(2) {
                            if (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1) > 1e-9 {
                                route.segments.push(RouteSegment {
                                    layer: pl,
                                    start: w[0],
                                    end: w[1],
                                    width_mm: width,
                                });
                            }
                        }
                        if route.segments.len() > seg_start {
                            route
                                .path_spans
                                .push((seg_start, route.segments.len() - seg_start));
                            route.path_parents.push(None);
                            info!(
                                "severed-component stitch: '{}' fragment ({} verts) joined to main by a {d:.2}mm perimeter track",
                                board.nets[ni].name,
                                polys[pi].len()
                            );
                            stitched += 1;
                            joined_track = true;
                            break;
                        }
                    }
                }
                if !joined_track {
                    log::warn!(
                        "severed-component stitch: '{}' fragment ({} verts, bbox {fx0:.1},{fy0:.1}-{fx1:.1},{fy1:.1}) — no via site, no perimeter route (honest)",
                        board.nets[ni].name,
                        polys[pi].len()
                    );
                }
            }
        }
    }
    stitched
}


/// Join every pad-holding group of a pour net to its main body.
///
/// The witness rule (main fill or >=2 distinct pads) is a REMOVAL
/// criterion: copper failing it is dead and gets swept. It is the
/// wrong criterion for JOINING — measured on the true-geometry mixer,
/// seed 99: a pocket-sink pad at (43.68,27.91), spoke-served by its
/// own fill fragment plus a 0.35mm stub, formed a one-pad component
/// disconnected from main. One pad makes a component worth
/// CONNECTING, not worth deleting; a first cut of this pass required
/// two and silently skipped the exact group it existed to fix.
///
/// Components are built with the witness DSU on final copper
/// (segments + vias + fill fragments); every component holding at
/// least one pad witness and not holding the main fragment is routed
/// to main. The nearest point-pair usually straddles the very blocker
/// that split the fill (a 0.81mm gap refused: the gap IS the foreign
/// clearance), so several pairs are ranked and the multi-layer maze
/// is the last resort — it dives under the blocker.
fn pour_split_group_join(board: &Board, final_routes: &mut Vec<Route>) -> usize {
    let via_r = board.layer_stack.via.pad_mm / 2.0;
    let mut joined = 0usize;
    for ni in 0..board.nets.len() {
        let Some(pl) = board.nets[ni].plane_layer else { continue };
        let Some(polys) = output::kicad::emission_fill_polys(board, final_routes, ni)
        else {
            continue;
        };
        if polys.is_empty() {
            continue;
        }
        let net_id = board.nets[ni].id;
        let segs = final_routes[ni].segments.clone();
        let vias: Vec<(f64, f64)> =
            final_routes[ni].vias.iter().map(|v| (v.x, v.y)).collect();
        let (n, nv, np) = (segs.len(), vias.len(), polys.len());
        let bboxes: Vec<(f64, f64, f64, f64)> = polys
            .iter()
            .map(|p| {
                p.iter().fold(
                    (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
                    |(x0, y0, x1, y1), &(x, y)| (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
                )
            })
            .collect();
        let fill_hit = |x: f64, y: f64| -> Option<usize> {
            for (pi, poly) in polys.iter().enumerate() {
                let (x0, y0, x1, y1) = bboxes[pi];
                if x < x0 || x > x1 || y < y0 || y > y1 {
                    continue;
                }
                let mut ins = false;
                let m = poly.len();
                for k in 0..m {
                    let (px1, py1) = poly[k];
                    let (px2, py2) = poly[(k + 1) % m];
                    if (py1 > y) != (py2 > y)
                        && x < (px2 - px1) * (y - py1) / (py2 - py1) + px1
                    {
                        ins = !ins;
                    }
                }
                if ins {
                    return Some(pi);
                }
            }
            None
        };
        let mut dsu: Vec<usize> = (0..n + nv + np).collect();
        for i in 0..n {
            for j in (i + 1)..n {
                if segs[i].layer != segs[j].layer {
                    continue;
                }
                let (a, b) = (&segs[i], &segs[j]);
                let close = [a.start, a.end].iter().any(|&pt| {
                    (b.start.0 - pt.0).hypot(b.start.1 - pt.1) <= 0.011
                        || (b.end.0 - pt.0).hypot(b.end.1 - pt.1) <= 0.011
                        || geom::segment_point_too_close(
                            b.start,
                            b.end,
                            pt,
                            (a.width_mm + b.width_mm) / 2.0 + 0.01,
                        )
                }) || [b.start, b.end].iter().any(|&pt| {
                    geom::segment_point_too_close(
                        a.start,
                        a.end,
                        pt,
                        (a.width_mm + b.width_mm) / 2.0 + 0.01,
                    )
                });
                if close {
                    dsu_union(&mut dsu, i, j);
                }
            }
        }
        for (vi, &(vx, vy)) in vias.iter().enumerate() {
            for (i, sg) in segs.iter().enumerate() {
                if (sg.start.0 - vx).hypot(sg.start.1 - vy) <= via_r + sg.width_mm / 2.0
                    || (sg.end.0 - vx).hypot(sg.end.1 - vy) <= via_r + sg.width_mm / 2.0
                    || geom::segment_point_too_close(
                        sg.start,
                        sg.end,
                        (vx, vy),
                        via_r + sg.width_mm / 2.0,
                    )
                {
                    dsu_union(&mut dsu, n + vi, i);
                }
            }
            if let Some(pi) = fill_hit(vx, vy) {
                dsu_union(&mut dsu, n + vi, n + nv + pi);
            }
        }
        for (i, sg) in segs.iter().enumerate() {
            if sg.layer != pl {
                continue;
            }
            let len = (sg.end.0 - sg.start.0).hypot(sg.end.1 - sg.start.1);
            let steps = ((len / 0.3).ceil() as usize).max(1);
            for st in 0..=steps {
                let t = st as f64 / steps as f64;
                if let Some(pi) = fill_hit(
                    sg.start.0 + (sg.end.0 - sg.start.0) * t,
                    sg.start.1 + (sg.end.1 - sg.start.1) * t,
                ) {
                    dsu_union(&mut dsu, i, n + nv + pi);
                }
            }
            // A track whose END sits ON the fill boundary is connected
            // (its half-width overlaps the copper) but a point-in-
            // polygon test on a vertex is a coin flip — the ML rungs
            // target boundary VERTICES by construction. Sample a ring
            // of half-width around each endpoint.
            let hw = sg.width_mm / 2.0 - 0.01;
            for &(ex, ey) in &[sg.start, sg.end] {
                for q in 0..8 {
                    let a = q as f64 * std::f64::consts::FRAC_PI_4;
                    if let Some(pi) = fill_hit(ex + hw * a.cos(), ey + hw * a.sin()) {
                        dsu_union(&mut dsu, i, n + nv + pi);
                        break;
                    }
                }
            }
        }
        // Pad witnesses: through segment endpoints on the pad face, or
        // through the fragment the pad's relief spokes feed (walk the
        // spoke directions — the pad centre itself sits in the void).
        let mut root_pads: crate::det::HashMap<usize, usize> = Default::default();
        let mut main_root_opt: Option<usize> = None;
        if let Some(main_pi) = polys
            .iter()
            .enumerate()
            .max_by_key(|(_, p)| p.len())
            .map(|(i, _)| i)
        {
            main_root_opt = Some(dsu_find(&mut dsu, n + nv + main_pi));
        }
        for comp in &board.components {
            let (co, sn) = (comp.theta.cos(), comp.theta.sin());
            let quarter =
                ((comp.theta / std::f64::consts::FRAC_PI_2).round() as i64).rem_euclid(2);
            for pin in &comp.pins {
                if pin.net != Some(net_id) || pin.unplaced {
                    continue;
                }
                let gx = comp.x + pin.dx * co - pin.dy * sn;
                let gy = comp.y + pin.dx * sn + pin.dy * co;
                let (pw, ph) = pin
                    .pad
                    .as_ref()
                    .map(|p| (p.width_mm, p.height_mm))
                    .unwrap_or((0.5, 0.5));
                let (hx, hy) = if quarter == 1 {
                    (ph / 2.0, pw / 2.0)
                } else {
                    (pw / 2.0, ph / 2.0)
                };
                let mut roots: crate::det::HashSet<usize> = Default::default();
                for (i, sg) in segs.iter().enumerate() {
                    for &pt in &[sg.start, sg.end] {
                        if (pt.0 - gx).abs() <= hx + 0.05 && (pt.1 - gy).abs() <= hy + 0.05
                        {
                            roots.insert(dsu_find(&mut dsu, i));
                        }
                    }
                }
                // ON OR INSIDE the pad edge — the sweep's calibrated
                // test. An outward ray-walk jumps the relief gap and
                // claims the pad onto a NEIGHBOUR fragment (measured
                // twice now: first in the witness sweep's history, and
                // again here — the walk invented stray pad-groups on
                // clean boards, broke the ecc83 byte-guard and put 1v
                // on seeds 42/7 via spurious joins). A spoke tip
                // overlaps the pad; a void contour does not.
                if let Some(pi) = fill_hit(gx, gy) {
                    roots.insert(dsu_find(&mut dsu, n + nv + pi));
                }
                for pi in 0..np {
                    if polys[pi].iter().any(|&(vx, vy)| {
                        (vx - gx).abs() <= hx + 0.001 && (vy - gy).abs() <= hy + 0.001
                    }) {
                        roots.insert(dsu_find(&mut dsu, n + nv + pi));
                    }
                }
                // A pad IS copper: it BRIDGES everything it touches.
                // Witness-only pads left a truth gap — ecc83's pocket
                // chain reaches the pour THROUGH its far-anchor pad,
                // and with no pad nodes the chain graded stranded and
                // drew a spurious join that broke the byte-guard.
                let mut it = roots.iter();
                if let Some(&first) = it.next() {
                    for &r in it {
                        dsu_union(&mut dsu, first, r);
                    }
                    let merged = dsu_find(&mut dsu, first);
                    *root_pads.entry(merged).or_insert(0) += 1;
                }
            }
        }
        // Re-canonicalise witness counts after pad bridging: earlier
        // entries may now share a root.
        let mut canon: crate::det::HashMap<usize, usize> = Default::default();
        for (r, c) in root_pads.drain() {
            *canon.entry(dsu_find(&mut dsu, r)).or_insert(0) += c;
        }
        let root_pads = canon;
        let Some(main_root) = main_root_opt.map(|r| dsu_find(&mut dsu, r)) else {
            continue;
        };
        let strays: Vec<usize> = root_pads
            .keys()
            .copied()
            .filter(|&r| r != main_root)
            .collect();
        if strays.is_empty() {
            continue;
        }
        // Join endpoints must be copper ON THE PLANE LAYER: both the
        // single-layer maze and the ML maze start on `pl`. A stray
        // group's B.Cu-only endpoint used to be offered as a start and
        // the F.Cu track then began in air (measured: C4's ML route on
        // the mixer, F->via->B->via->F, was graded a stray — its F.Cu
        // end sat exactly on a fill VERTEX — and the join drew a 14mm
        // F.Cu track from its B.Cu bend, shipped as track_dangling).
        let points_of = |dsu: &mut Vec<usize>, root: usize| -> Vec<(f64, f64)> {
            let mut out = Vec::new();
            for (i, sg) in segs.iter().enumerate() {
                if sg.layer == pl && dsu_find(dsu, i) == root {
                    out.push(sg.start);
                    out.push(sg.end);
                }
            }
            for pi in 0..np {
                if dsu_find(dsu, n + nv + pi) == root {
                    let stride = (polys[pi].len() / 64).max(1);
                    for (k, &v) in polys[pi].iter().enumerate() {
                        if k % stride == 0 {
                            out.push(v);
                        }
                    }
                }
            }
            out
        };
        let main_pts = points_of(&mut dsu, main_root);
        for stray in strays {
            let stray_pts = points_of(&mut dsu, stray);
            if stray_pts.is_empty() || main_pts.is_empty() {
                continue;
            }
            let mut pairs: Vec<((f64, f64), (f64, f64), f64)> = Vec::new();
            for &sp in &stray_pts {
                let mut bm: Option<((f64, f64), f64)> = None;
                for &mp in &main_pts {
                    let dd = (sp.0 - mp.0).hypot(sp.1 - mp.1);
                    if bm.map_or(true, |(_, bd)| dd < bd) {
                        bm = Some((mp, dd));
                    }
                }
                if let Some((mp, dd)) = bm {
                    if dd <= 25.0 {
                        pairs.push((sp, mp, dd));
                    }
                }
            }
            pairs.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
            pairs.truncate(8);
            if pairs.is_empty() {
                continue;
            }
            let width = board.config.min_trace_width_mm;
            let idx = geom::ClearanceIndex::build(board, final_routes, Some(net_id));
            let mut done = false;
            for &(sp, mp, dd) in &pairs {
                let path2 = geom::route_escape(&idx, sp, mp, width, pl, net_id)
                    .or_else(|| geom::route_tunnel(&idx, sp, mp, width, pl, net_id));
                if let Some(path) = path2 {
                    let route = &mut final_routes[ni];
                    let seg_start = route.segments.len();
                    for w in path.windows(2) {
                        if (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1) > 1e-9 {
                            route.segments.push(RouteSegment {
                                layer: pl,
                                start: w[0],
                                end: w[1],
                                width_mm: width,
                            });
                        }
                    }
                    if route.segments.len() > seg_start {
                        route
                            .path_spans
                            .push((seg_start, route.segments.len() - seg_start));
                        route.path_parents.push(None);
                        joined += 1;
                        info!(
                            "split-group join: '{}' pad group joined to main ({dd:.2}mm)",
                            board.nets[ni].name
                        );
                        done = true;
                        break;
                    }
                }
            }
            if !done {
                let signal_layers: Vec<usize> = board
                    .layer_stack
                    .layers
                    .iter()
                    .enumerate()
                    .filter(|(_, l)| l.kind == crate::types::LayerKind::Signal)
                    .map(|(k, _)| k)
                    .collect();
                // Drill-rule validation: every layer-switch point must
                // clear EVERY existing hole (any net's via, any THT
                // barrel) by drill+0.25 — the oracle's hole_to_hole.
                // Committing raw maze output put two GND vias 0.206mm
                // apart (measured, seeds 42/7).
                let drill = board.layer_stack.via.drill_mm;
                let mut holes_all: Vec<(f64, f64)> = Vec::new();
                for r in final_routes.iter() {
                    for v in &r.vias {
                        holes_all.push((v.x, v.y));
                    }
                }
                for comp in &board.components {
                    let (co2, sn2) = (comp.theta.cos(), comp.theta.sin());
                    for pin in &comp.pins {
                        if pin.pad.as_ref().and_then(|p| p.drill_mm).is_some() {
                            holes_all.push((
                                comp.x + pin.dx * co2 - pin.dy * sn2,
                                comp.y + pin.dx * sn2 + pin.dy * co2,
                            ));
                        }
                    }
                }
                let way_legal = |way: &Vec<(f64, f64, usize)>| -> bool {
                    let mut switches: Vec<(f64, f64)> = Vec::new();
                    for w in way.windows(2) {
                        if w[0].2 != w[1].2 {
                            switches.push((w[0].0, w[0].1));
                        }
                    }
                    for (i, &(x, y)) in switches.iter().enumerate() {
                        if holes_all
                            .iter()
                            .any(|&(hx, hy)| (hx - x).hypot(hy - y) < drill + 0.25)
                        {
                            return false;
                        }
                        if switches[i + 1..]
                            .iter()
                            .any(|&(x2, y2)| (x2 - x).hypot(y2 - y) < drill + 0.25)
                        {
                            return false;
                        }
                    }
                    true
                };
                for &(sp, mp, dd) in pairs.iter().take(4) {
                    let Some(way) = geom::route_tunnel_ml(
                        &idx,
                        sp,
                        pl,
                        mp,
                        pl,
                        width,
                        via_r,
                        &signal_layers,
                        net_id,
                        12.0,
                    )
                    .filter(&way_legal) else {
                        continue;
                    };
                    let route = &mut final_routes[ni];
                    let seg_start = route.segments.len();
                    for w in way.windows(2) {
                        let (a, b) = (w[0], w[1]);
                        if a.2 == b.2 {
                            if (a.0 - b.0).hypot(a.1 - b.1) > 1e-9 {
                                route.segments.push(RouteSegment {
                                    layer: a.2,
                                    start: (a.0, a.1),
                                    end: (b.0, b.1),
                                    width_mm: width,
                                });
                            }
                        } else if !route
                            .vias
                            .iter()
                            .any(|v| (v.x - a.0).hypot(v.y - a.1) < 1e-6)
                        {
                            route.vias.push(RouteVia {
                                x: a.0,
                                y: a.1,
                                from_layer: 0,
                                to_layer: board.layer_stack.layers.len() - 1,
                            });
                        }
                    }
                    if route.segments.len() > seg_start {
                        route
                            .path_spans
                            .push((seg_start, route.segments.len() - seg_start));
                        route.path_parents.push(None);
                        joined += 1;
                        info!(
                            "split-group join: '{}' pad group ML-joined to main ({dd:.2}mm)",
                            board.nets[ni].name
                        );
                        done = true;
                        break;
                    }
                }
            }
            if !done {
                log::warn!(
                    "split-group join: '{}' pad group stranded from main — no legal route (honest)",
                    board.nets[ni].name
                );
            }
        }
    }
    joined
}

fn pour_unserved_pad_route(board: &Board, final_routes: &mut Vec<Route>) -> usize {
    let mut joined = 0usize;
    // Phase 1 is read-only: the fill mirror borrows final_routes, so
    // collect every repair before touching anything.
    struct Job {
        ni: usize,
        src: (f64, f64),
        dst: (f64, f64),
        width: f64,
        layer: usize,
    }
    let mut jobs: Vec<Job> = Vec::new();
    for ni in 0..board.nets.len() {
        let Some(layer) = board.nets[ni].plane_layer else {
            continue;
        };
        if board.nets[ni].pins.len() < 2 {
            continue;
        }
        let net_id = board.nets[ni].id;
        let polys = output::kicad::emission_fill_polys(board, final_routes, ni)
            .unwrap_or_default();
        // Fill-served means served by fill that GOES SOMEWHERE. The
        // any-fragment test skipped pads whose only fill contact was
        // a severed scrap (measured on the true-geometry mixer:
        // C8.1/C6.2 sat in a GND fragment that reached nothing —
        // "served", skipped, shipped as the board's zone-split pair).
        // Membership in severed copper is not service — the same rule
        // as the witness sweep and the stitch referee.
        let severed: crate::det::HashSet<usize> =
            pour_severed_fragments(board, final_routes, ni, &polys)
                .into_iter()
                .collect();
        let inside = |poly: &[(f64, f64)], x: f64, y: f64| -> bool {
            let mut ins = false;
            let m = poly.len();
            for k in 0..m {
                let (x1, y1) = poly[k];
                let (x2, y2) = poly[(k + 1) % m];
                if (y1 > y) != (y2 > y) && x < (x2 - x1) * (y - y1) / (y2 - y1) + x1 {
                    ins = !ins;
                }
            }
            ins
        };
        for comp in &board.components {
            let (co, sn) = (comp.theta.cos(), comp.theta.sin());
            let quarter =
                ((comp.theta / std::f64::consts::FRAC_PI_2).round() as i64).rem_euclid(2);
            for pin in &comp.pins {
                if pin.net != Some(net_id) || pin.unplaced {
                    continue;
                }
                let gx = comp.x + pin.dx * co - pin.dy * sn;
                let gy = comp.y + pin.dx * sn + pin.dy * co;
                let (pw, ph) = match &pin.pad {
                    Some(p) => (p.width_mm, p.height_mm),
                    None => (0.5, 0.5),
                };
                let (hx, hy) = if quarter == 1 {
                    (ph / 2.0, pw / 2.0)
                } else {
                    (pw / 2.0, ph / 2.0)
                };
                // Served by the pour? Sample the pad face, because a
                // thermally relieved pad's CENTRE sits in the void.
                let mut fill_served = false;
                'fs: for dx in [-0.6, -0.3, 0.0, 0.3, 0.6] {
                    for dy in [-0.6, -0.3, 0.0, 0.3, 0.6] {
                        let (sx, sy) = (gx + dx * hx, gy + dy * hy);
                        if polys
                            .iter()
                            .enumerate()
                            .any(|(pi, p)| !severed.contains(&pi) && inside(p, sx, sy))
                        {
                            fill_served = true;
                            break 'fs;
                        }
                    }
                }
                // TEMPORARY probe-at: name the verdict for one pad.
                let probe_here = std::env::var("BHDL_PNR_PROBE_AT")
                    .ok()
                    .and_then(|v| {
                        let (a, b) = v.split_once(',')?;
                        Some((a.parse::<f64>().ok()?, b.parse::<f64>().ok()?))
                    })
                    .map_or(false, |(px, py)| {
                        (gx - px).abs() < 0.5 && (gy - py).abs() < 0.5
                    });
                if probe_here {
                    log::info!(
                        "[probe-at] pad ({gx:.2},{gy:.2}) fill_served={fill_served} frags={}",
                        polys.len()
                    );
                }
                if fill_served {
                    continue;
                }
                // Served by a track?
                let touched = final_routes[ni].segments.iter().any(|sg| {
                    geom::segment_point_too_close(
                        sg.start,
                        sg.end,
                        (gx, gy),
                        hx.max(hy) + sg.width_mm / 2.0,
                    )
                });
                if probe_here && touched {
                    for sg in final_routes[ni].segments.iter() {
                        if geom::segment_point_too_close(
                            sg.start,
                            sg.end,
                            (gx, gy),
                            hx.max(hy) + sg.width_mm / 2.0,
                        ) {
                            log::info!(
                                "[probe-at] touched by l{} ({:.2},{:.2})->({:.2},{:.2})",
                                sg.layer, sg.start.0, sg.start.1, sg.end.0, sg.end.1
                            );
                        }
                    }
                }
                if touched {
                    continue;
                }
                // Nearest same-net copper: a point on one of this
                // net's own segments, or a vertex of its fill.
                let mut best: Option<((f64, f64), f64, usize, f64)> = None;
                for sg in &final_routes[ni].segments {
                    // Only copper on the pad's own layer is a target
                    // for the single-layer first rung; a nearer B.Cu
                    // stub used to win the search and the pad was
                    // then DROPPED by the layer guard below — never a
                    // job, never a rung, silently unserved (measured:
                    // C6.2 on the mixer, F.Cu main 7mm away, a GND
                    // stub 6.9mm away on B.Cu; probe said
                    // fill_served=false, no log line ever named it).
                    if sg.layer != layer {
                        continue;
                    }
                    let (ax, ay) = sg.start;
                    let (bx, by) = sg.end;
                    let (dx, dy) = (bx - ax, by - ay);
                    let l2 = dx * dx + dy * dy;
                    let t = if l2 <= 0.0 {
                        0.0
                    } else {
                        (((gx - ax) * dx + (gy - ay) * dy) / l2).clamp(0.0, 1.0)
                    };
                    let q = (ax + t * dx, ay + t * dy);
                    let d = (q.0 - gx).hypot(q.1 - gy);
                    if best.map_or(true, |(.., bd)| d < bd) {
                        best = Some((q, sg.width_mm, sg.layer, d));
                    }
                }
                // Nearest LIVE fill copper only: a severed scrap is
                // not service (same rule as the fill_served test above
                // and the witness sweep) — targeting one made the
                // first rung "join" a pad to its own thermal disc.
                for (pi, poly) in polys.iter().enumerate() {
                    if severed.contains(&pi) {
                        continue;
                    }
                    for &(vx, vy) in poly {
                        let d = (vx - gx).hypot(vy - gy);
                        if best.map_or(true, |(.., bd)| d < bd) {
                            best = Some((
                                (vx, vy),
                                board.config.min_trace_width_mm,
                                layer,
                                d,
                            ));
                        }
                    }
                }
                let Some((dst, width, dlayer, d)) = best else {
                    continue;
                };
                // Same cap the continuity repair uses: past this, a
                // "repair" is really a reroute and belongs upstream.
                if d > 20.0 || dlayer != layer {
                    continue;
                }
                jobs.push(Job {
                    ni,
                    src: (gx, gy),
                    dst,
                    width: width.max(board.config.min_trace_width_mm),
                    layer,
                });
            }
        }
    }
    for job in jobs {
        let net_id = board.nets[job.ni].id;
        let idx = geom::ClearanceIndex::build(board, final_routes, Some(net_id));
        let path = geom::route_escape(&idx, job.src, job.dst, job.width, job.layer, net_id)
            .or_else(|| {
                geom::route_tunnel(&idx, job.src, job.dst, job.width, job.layer, net_id)
            });
        drop(idx);
        let Some(path) = path else {
            // Single-layer escape/tunnel exhausted. Escalate to the
            // island bridge's full ladder — near via, far via,
            // via-in-pad, rip-negotiation — the same rungs an
            // island pad gets, because this pad IS one, just found
            // later. Measured: U2.V_MINUS on the jack board's seed
            // 99, 17mm from the nearest same-net copper across a
            // dense face with a whole-board B.Cu plane underneath;
            // the single-layer maze failed six times where a via
            // drop is the canonical fix.
            let main_poly = output::kicad::emission_fill_polys(board, final_routes, job.ni)
                .and_then(|ps| ps.into_iter().max_by_key(|p| p.len()));
            let mut rip_budget = 1usize;
            if let Some(mp) = main_poly {
                // VERIFY-AND-REVERT around the ladder. Its via rungs
                // validate with plane_swallows — the optimistic hole
                // model — and a drop can land on far copper that
                // island removal strands (measured: U2.V_MINUS's
                // rescue via at (68.78,50.80) shipped as the board's
                // last dangling pair). The emitted far main fragment
                // is the only honest referee; a "join" that fails it
                // never happened.
                let snapshot: Vec<Route> = final_routes.clone();
                let vias_before = final_routes[job.ni].vias.len();
                if island_bridge_pad(
                    board,
                    final_routes,
                    job.ni,
                    job.src,
                    &mp,
                    job.layer,
                    job.width,
                    true,
                    &mut rip_budget,
                ) {
                    let new_vias: Vec<(f64, f64)> = final_routes[job.ni].vias
                        [vias_before..]
                        .iter()
                        .map(|v| (v.x, v.y))
                        .collect();
                    let verified = if new_vias.is_empty() {
                        true // track-only join: same-face copper contact
                    } else {
                        let net_id = board.nets[job.ni].id;
                        let n_layers = board.layer_stack.layers.len();
                        let far = if job.layer == 0 { n_layers - 1 } else { 0 };
                        let (_, fills) = output::kicad::export_kicad_pcb_with_fills(
                            board,
                            final_routes,
                        );
                        let far_main = fills
                            .zones
                            .iter()
                            .filter(|z| z.net_id == net_id && z.layer == far)
                            .flat_map(|z| z.polys.iter())
                            .max_by_key(|p| p.len());
                        match far_main {
                            Some(fp) => new_vias.iter().all(|&(vx, vy)| {
                                let mut ins = false;
                                let m = fp.len();
                                for k in 0..m {
                                    let (x1, y1) = fp[k];
                                    let (x2, y2) = fp[(k + 1) % m];
                                    if (y1 > vy) != (y2 > vy)
                                        && vx < (x2 - x1) * (vy - y1) / (y2 - y1) + x1
                                    {
                                        ins = !ins;
                                    }
                                }
                                ins
                            }),
                            None => false,
                        }
                    };
                    if verified {
                        joined += 1;
                        info!(
                            "unserved pad: '{}' pad ({:.2},{:.2}) joined via the bridge ladder",
                            board.nets[job.ni].name, job.src.0, job.src.1
                        );
                        continue;
                    }
                    *final_routes = snapshot;
                    log::warn!(
                        "unserved pad: '{}' pad ({:.2},{:.2}) ladder join FAILED far-main verification — reverted (honest)",
                        board.nets[job.ni].name, job.src.0, job.src.1
                    );
                }
                // Last rung: OUR OWN via search with the honest
                // referee up front. The ladder commits the first
                // plane_swallows-blessed site; here every candidate
                // must sit inside the EMITTED far main before a track
                // is even attempted, so what commits is verified by
                // construction.
                let net_id = board.nets[job.ni].id;
                let n_layers = board.layer_stack.layers.len();
                let far = if job.layer == 0 { n_layers - 1 } else { 0 };
                let far_sig = board
                    .layer_stack
                    .layers
                    .get(far)
                    .map(|l| l.kind == crate::types::LayerKind::Signal)
                    .unwrap_or(false);
                if far_sig {
                    let (_, fills) =
                        output::kicad::export_kicad_pcb_with_fills(board, final_routes);
                    let far_main = fills
                        .zones
                        .iter()
                        .filter(|z| z.net_id == net_id && z.layer == far)
                        .flat_map(|z| z.polys.iter())
                        .max_by_key(|p| p.len())
                        .cloned();
                    if let Some(fp) = far_main {
                        let inside = |x: f64, y: f64| -> bool {
                            let mut ins = false;
                            let m = fp.len();
                            for k in 0..m {
                                let (x1, y1) = fp[k];
                                let (x2, y2) = fp[(k + 1) % m];
                                if (y1 > y) != (y2 > y)
                                    && x < (x2 - x1) * (y - y1) / (y2 - y1) + x1
                                {
                                    ins = !ins;
                                }
                            }
                            ins
                        };
                        let via_r = board.layer_stack.via.pad_mm / 2.0;
                        let margin = via_r + 0.15;
                        let idx = geom::ClearanceIndex::build(
                            board,
                            final_routes,
                            Some(net_id),
                        );
                        let mut done = false;
                        'rings: for ring in 0..24 {
                            let rr = 0.6 + ring as f64 * 0.45;
                            for k in 0..12 {
                                let ang = k as f64 * std::f64::consts::PI / 6.0;
                                let (vx, vy) =
                                    (job.src.0 + rr * ang.cos(), job.src.1 + rr * ang.sin());
                                let far_ok = (0..8).all(|q| {
                                    let a = q as f64 * std::f64::consts::FRAC_PI_4;
                                    inside(
                                        vx + margin * a.cos(),
                                        vy + margin * a.sin(),
                                    )
                                });
                                if !far_ok
                                    || idx.via_conflict(vx, vy, via_r, net_id).is_some()
                                    || final_routes.iter().any(|r| {
                                        r.vias.iter().any(|v| {
                                            (v.x - vx).hypot(v.y - vy)
                                                < board.layer_stack.via.drill_mm + 0.25
                                        })
                                    })
                                {
                                    continue;
                                }
                                let Some(path) = geom::route_escape(
                                    &idx, job.src, (vx, vy), job.width, job.layer, net_id,
                                )
                                .or_else(|| {
                                    geom::route_tunnel(
                                        &idx, job.src, (vx, vy), job.width, job.layer,
                                        net_id,
                                    )
                                }) else {
                                    continue;
                                };
                                let route = &mut final_routes[job.ni];
                                let seg_start = route.segments.len();
                                for w in path.windows(2) {
                                    if (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1) > 1e-9 {
                                        route.segments.push(RouteSegment {
                                            layer: job.layer,
                                            start: w[0],
                                            end: w[1],
                                            width_mm: job.width,
                                        });
                                    }
                                }
                                route.vias.push(RouteVia {
                                    x: vx,
                                    y: vy,
                                    from_layer: job.layer.min(far),
                                    to_layer: job.layer.max(far),
                                });
                                route
                                    .path_spans
                                    .push((seg_start, route.segments.len() - seg_start));
                                route.path_parents.push(None);
                                joined += 1;
                                info!(
                                    "unserved pad: '{}' pad ({:.2},{:.2}) via-dropped to VERIFIED far main at ({vx:.2},{vy:.2})",
                                    board.nets[job.ni].name, job.src.0, job.src.1
                                );
                                done = true;
                                break 'rings;
                            }
                        }
                        if done {
                            continue;
                        }
                        // TRUE last rung: MULTI-LAYER maze to the far
                        // main itself. The verified via search needs a
                        // single-layer path to a ring site; in a dense
                        // pocket none exists (measured: C8.1 at
                        // (80.53,74.65) failed every rung on every
                        // round — no fill, no track within 2mm, the
                        // fractured pot-grid region walls it in on its
                        // own face). The ML maze may dive under the
                        // wall; every layer switch is drill-checked by
                        // the maze itself, and the destination is a
                        // far-main VERTEX, so the landing is on real
                        // main copper by construction.
                        let signal_layers: Vec<usize> = board
                            .layer_stack
                            .layers
                            .iter()
                            .enumerate()
                            .filter(|(_, l)| {
                                l.kind == crate::types::LayerKind::Signal
                            })
                            .map(|(k, _)| k)
                            .collect();
                        // Targets: vertices of the far main AND of the
                        // pad's OWN-layer main. The maze is multi-layer
                        // either way; restricting the destination to
                        // the far face left a pad whose own-face main
                        // was 8.6mm away unserved because the far main
                        // was 24.8mm away (measured: C4 on the mixer
                        // with demo-true jacks — F.Cu main at 8.6mm,
                        // B.Cu main at 24.8mm, "no legal route").
                        let own_main = fills
                            .zones
                            .iter()
                            .filter(|z| z.net_id == net_id && z.layer == job.layer)
                            .flat_map(|z| z.polys.iter())
                            .max_by_key(|p| p.len())
                            .cloned();
                        let mut verts: Vec<((f64, f64), usize, f64)> = fp
                            .iter()
                            .map(|&(vx, vy)| {
                                (
                                    (vx, vy),
                                    far,
                                    (vx - job.src.0).hypot(vy - job.src.1),
                                )
                            })
                            .collect();
                        if let Some(om) = own_main.as_ref() {
                            verts.extend(om.iter().map(|&(vx, vy)| {
                                (
                                    (vx, vy),
                                    job.layer,
                                    (vx - job.src.0).hypot(vy - job.src.1),
                                )
                            }));
                        }
                        verts.sort_by(|a, b| {
                            a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal)
                        });
                        let idx2 = geom::ClearanceIndex::build(
                            board,
                            final_routes,
                            Some(net_id),
                        );
                        let mut ml_done = false;
                        for &(dst, dst_layer, d) in verts.iter().take(6) {
                            if d > 20.0 {
                                break;
                            }
                            let Some(way) = geom::route_tunnel_ml(
                                &idx2,
                                job.src,
                                job.layer,
                                dst,
                                dst_layer,
                                job.width,
                                via_r,
                                &signal_layers,
                                net_id,
                                12.0,
                            ) else {
                                continue;
                            };
                            let route = &mut final_routes[job.ni];
                            let seg_start = route.segments.len();
                            for w in way.windows(2) {
                                let (a, b) = (w[0], w[1]);
                                if a.2 == b.2 {
                                    if (a.0 - b.0).hypot(a.1 - b.1) > 1e-9 {
                                        route.segments.push(RouteSegment {
                                            layer: a.2,
                                            start: (a.0, a.1),
                                            end: (b.0, b.1),
                                            width_mm: job.width,
                                        });
                                    }
                                } else {
                                    let dup = route.vias.iter().any(|v| {
                                        (v.x - a.0).hypot(v.y - a.1) < 1e-6
                                    });
                                    if !dup {
                                        route.vias.push(RouteVia {
                                            x: a.0,
                                            y: a.1,
                                            from_layer: job.layer.min(far),
                                            to_layer: job.layer.max(far),
                                        });
                                    }
                                }
                            }
                            if route.segments.len() > seg_start {
                                route.path_spans.push((
                                    seg_start,
                                    route.segments.len() - seg_start,
                                ));
                                route.path_parents.push(None);
                                joined += 1;
                                info!(
                                    "unserved pad: '{}' pad ({:.2},{:.2}) ML-mazed to far main {d:.2}mm away",
                                    board.nets[job.ni].name, job.src.0, job.src.1
                                );
                                ml_done = true;
                            }
                            break;
                        }
                        if ml_done {
                            continue;
                        }
                    }
                }
            }
            log::warn!(
                "unserved pad: '{}' pad ({:.2},{:.2}) reaches neither fill nor track — no legal route (honest)",
                board.nets[job.ni].name, job.src.0, job.src.1
            );
            continue;
        };
        let route = &mut final_routes[job.ni];
        let seg_start = route.segments.len();
        for w in path.windows(2) {
            if (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1) > 1e-9 {
                route.segments.push(RouteSegment {
                    layer: job.layer,
                    start: w[0],
                    end: w[1],
                    width_mm: job.width,
                });
            }
        }
        if route.segments.len() > seg_start {
            route
                .path_spans
                .push((seg_start, route.segments.len() - seg_start));
            route.path_parents.push(None);
            joined += 1;
            info!(
                "unserved pad: '{}' pad ({:.2},{:.2}) joined to same-net copper {:.2}mm away",
                board.nets[job.ni].name,
                job.src.0,
                job.src.1,
                (job.dst.0 - job.src.0).hypot(job.dst.1 - job.src.1)
            );
        }
    }
    joined
}

fn signal_net_continuity_repair(board: &Board, final_routes: &mut Vec<Route>) -> usize {
    let via_r = board.layer_stack.via.pad_mm / 2.0;
    let mut repaired = 0usize;
    for ni in 0..board.nets.len() {
        if board.nets[ni].plane_layer.is_some()
            || board.nets[ni].pins.len() < 2
            || final_routes[ni].segments.is_empty()
        {
            continue;
        }
        let net_id = board.nets[ni].id;
        let segs: Vec<RouteSegment> = final_routes[ni].segments.clone();
        let n = segs.len();
        // Union-find over segments: same-layer geometric contact
        // (endpoint against the other's centerline within combined
        // half-widths) + via bridges across layers.
        let mut par: Vec<usize> = (0..n).collect();
        fn find(par: &mut Vec<usize>, mut a: usize) -> usize {
            while par[a] != a {
                par[a] = par[par[a]];
                a = par[a];
            }
            a
        }
        for a in 0..n {
            for b in (a + 1)..n {
                if segs[a].layer != segs[b].layer {
                    continue;
                }
                // GENUINE overlap only: KiCad connects on copper
                // overlap; a +grace here merged a 0.13mm sliver into
                // its pad group and the padless test never fired.
                let tol = (segs[a].width_mm + segs[b].width_mm) / 2.0 - 1e-3;
                let touch = geom::segment_point_too_close(
                    segs[b].start,
                    segs[b].end,
                    segs[a].start,
                    tol,
                ) || geom::segment_point_too_close(
                    segs[b].start,
                    segs[b].end,
                    segs[a].end,
                    tol,
                ) || geom::segment_point_too_close(
                    segs[a].start,
                    segs[a].end,
                    segs[b].start,
                    tol,
                ) || geom::segment_point_too_close(
                    segs[a].start,
                    segs[a].end,
                    segs[b].end,
                    tol,
                );
                if touch {
                    let (ra, rb) = (find(&mut par, a), find(&mut par, b));
                    par[ra] = rb;
                }
            }
        }
        for v in final_routes[ni].vias.clone() {
            let mut first: Option<usize> = None;
            for (i, sg) in segs.iter().enumerate() {
                if (sg.start.0 - v.x).hypot(sg.start.1 - v.y) <= via_r + 1e-3
                    || (sg.end.0 - v.x).hypot(sg.end.1 - v.y) <= via_r + 1e-3
                {
                    match first {
                        None => first = Some(i),
                        Some(f) => {
                            let (ra, rb) = (find(&mut par, f), find(&mut par, i));
                            par[ra] = rb;
                        }
                    }
                }
            }
        }
        // Pads -> groups (pad bbox contact with a segment endpoint,
        // pad layer respected via component side for SMD).
        let comp_pos: crate::det::HashMap<ComponentId, usize> = board
            .components
            .iter()
            .enumerate()
            .map(|(k, c)| (c.id, k))
            .collect();
        let mut pad_group: Vec<((f64, f64), Option<usize>)> = Vec::new();
        for &(cid, pid) in &board.nets[ni].pins {
            let Some(&ci) = comp_pos.get(&cid) else { continue };
            let comp = &board.components[ci];
            let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pid) else {
                continue;
            };
            if pin.unplaced {
                continue;
            }
            let (co, sn) = (comp.theta.cos(), comp.theta.sin());
            let gx = comp.x + pin.dx * co - pin.dy * sn;
            let gy = comp.y + pin.dx * sn + pin.dy * co;
            let half = pin
                .pad
                .as_ref()
                .map(|p| p.width_mm.max(p.height_mm) / 2.0)
                .unwrap_or(0.4);
            let mut g: Option<usize> = None;
            for (i, sg) in segs.iter().enumerate() {
                if geom::segment_point_too_close(
                    sg.start,
                    sg.end,
                    (gx, gy),
                    half + sg.width_mm / 2.0 - 1e-3,
                ) {
                    g = Some(find(&mut par, i));
                    break;
                }
            }
            pad_group.push(((gx, gy), g));
        }
        let mut groups: Vec<usize> = pad_group.iter().filter_map(|&(_, g)| g).collect();
        groups.sort_unstable();
        groups.dedup();
        // Padless slivers: a group carrying NO pad is orphan copper
        // (rip/rebuild leftovers — a 0.13mm ch2_vtap tail shipped as
        // its own KiCad group). The union-find above already merged
        // T-contacts and via bridges, so "no pad in the whole group"
        // is exact. Delete them.
        let roots: Vec<usize> = (0..n).map(|i| find(&mut par, i)).collect();
        let mut sliver_drop: Vec<usize> =
            (0..n).filter(|&i| !groups.contains(&roots[i])).collect();
        if !sliver_drop.is_empty() {
            sliver_drop.sort_unstable_by(|a, b| b.cmp(a));
            let n_drop = sliver_drop.len();
            for sk in sliver_drop {
                let r = &mut final_routes[ni];
                r.segments.remove(sk);
                for (qs, ql) in r.path_spans.iter_mut() {
                    if *qs > sk {
                        *qs -= 1;
                    } else if sk < *qs + *ql {
                        *ql = ql.saturating_sub(1);
                    }
                }
            }
            info!(
                "continuity repair: '{}' {} padless sliver segment(s) removed",
                board.nets[ni].name, n_drop
            );
            repaired += 1;
            // Segment indexing is stale for the bridging step — this
            // net gets re-judged on the pass's next invocation.
            continue;
        }
        // BARE pads count as their own group — a gutted net (one
        // 0.13mm sliver, both pads copper-less) is split even though
        // it has fewer than two pad-BEARING groups.
        let bare = pad_group.iter().filter(|&&(_, g)| g.is_none()).count();
        if groups.len() + bare < 2 {
            continue;
        }
        // Main group = most pads (a pad-bearing group wins over bare
        // pads); bridge every other pad-group AND every bare pad to
        // it — nearest endpoint pair, same layer first, maze
        // fallback. A fully-bare net routes pad-to-pad.
        let count_of = |g: usize, pg: &[((f64, f64), Option<usize>)]| {
            pg.iter().filter(|&&(_, x)| x == Some(g)).count()
        };
        let main: Option<usize> =
            groups.iter().copied().max_by_key(|&g| count_of(g, &pad_group));
        // Sources to join: other pad-bearing groups + bare pads.
        enum Join {
            Group(usize),
            Bare((f64, f64)),
        }
        let mut joins: Vec<Join> = groups
            .iter()
            .filter(|&&g| Some(g) != main)
            .map(|&g| Join::Group(g))
            .collect();
        for &(p, g) in &pad_group {
            if g.is_none() {
                joins.push(Join::Bare(p));
            }
        }
        // Anchor set of MAIN: its segments' endpoints, or (fully bare
        // net) the first bare pad — which then must not also be a
        // join source.
        let main_pts: Vec<((f64, f64), usize)> = match main {
            Some(mg) => segs
                .iter()
                .enumerate()
                .filter(|(j, _)| find(&mut par, *j) == mg)
                .flat_map(|(_, sb)| [(sb.start, sb.layer), (sb.end, sb.layer)])
                .collect(),
            None => {
                let Some(&(p0, _)) = pad_group.first() else { continue };
                if let Some(pos) = joins.iter().position(
                    |j| matches!(j, Join::Bare(p) if (p.0 - p0.0).hypot(p.1 - p0.1) < 1e-6),
                ) {
                    joins.remove(pos);
                }
                vec![(p0, 0usize)]
            }
        };
        for join in joins {
            let mut best: Option<((f64, f64), (f64, f64), usize, f64)> = None;
            match join {
                Join::Group(g) => {
                    for (i, sa) in segs.iter().enumerate() {
                        if find(&mut par, i) != g {
                            continue;
                        }
                        for &(pb, bl) in &main_pts {
                            if bl != sa.layer {
                                continue;
                            }
                            for &pa in &[sa.start, sa.end] {
                                let d = (pa.0 - pb.0).hypot(pa.1 - pb.1);
                                if best.map_or(true, |(.., bd)| d < bd) {
                                    best = Some((pa, pb, sa.layer, d));
                                }
                            }
                        }
                    }
                }
                Join::Bare(p) => {
                    for &(pb, bl) in &main_pts {
                        let d = (p.0 - pb.0).hypot(p.1 - pb.1);
                        if best.map_or(true, |(.., bd)| d < bd) {
                            best = Some((p, pb, bl, d));
                        }
                    }
                }
            }
            let Some((pa, pb, layer, d)) = best else { continue };
            if d > 20.0 {
                continue;
            }
            let width = segs
                .iter()
                .map(|s| s.width_mm)
                .fold(f64::INFINITY, f64::min)
                .max(board.config.min_trace_width_mm);
            let idx = geom::ClearanceIndex::build(board, final_routes, Some(net_id));
            let path = geom::route_escape(&idx, pa, pb, width, layer, net_id)
                .or_else(|| geom::route_tunnel(&idx, pa, pb, width, layer, net_id));
            let Some(path) = path else {
                // MULTI-LAYER maze joint: a pad fence on this face
                // is unrippable — but the other face is often open
                // (ch2_vtap: C22's pocket sealed on F.Cu by the pot
                // grid, empty underneath).
                {
                    let signal_layers: Vec<usize> = board
                        .layer_stack
                        .layers
                        .iter()
                        .enumerate()
                        .filter(|(_, l)| l.kind == crate::types::LayerKind::Signal)
                        .map(|(k, _)| k)
                        .collect();
                    let via_r2 = board.layer_stack.via.pad_mm / 2.0;
                    let cidx =
                        geom::ClearanceIndex::build(board, final_routes, Some(net_id));
                    // Drill-rule validation: every layer-switch point
                    // must clear EVERY existing hole (any net's via,
                    // any THT barrel) by drill+0.25 — the oracle's
                    // hole_to_hole / holes_co_located (measured: 6
                    // violations at the sweep's default seed).
                    let hole_gap = board.layer_stack.via.drill_mm + 0.25;
                    let mut all_holes: Vec<(f64, f64)> = Vec::new();
                    for r in final_routes.iter() {
                        for v in &r.vias {
                            all_holes.push((v.x, v.y));
                        }
                    }
                    for comp in &board.components {
                        let (co, sn) = (comp.theta.cos(), comp.theta.sin());
                        for pin in &comp.pins {
                            if pin.unplaced {
                                continue;
                            }
                            if pin.pad.as_ref().and_then(|pd| pd.drill_mm).is_some() {
                                all_holes.push((
                                    comp.x + pin.dx * co - pin.dy * sn,
                                    comp.y + pin.dx * sn + pin.dy * co,
                                ));
                            }
                        }
                    }
                    let way_legal = |way: &Vec<(f64, f64, usize)>| -> bool {
                        way.windows(2).all(|w| {
                            w[0].2 == w[1].2
                                || !all_holes.iter().any(|&(hx, hy)| {
                                    let d = (hx - w[0].0).hypot(hy - w[0].1);
                                    d > 1e-6 && d < hole_gap
                                })
                        })
                    };
                    // Nearest target failed the drill rule at some
                    // seeds — try the next-nearest main anchors too.
                    let mut cands: Vec<(f64, f64)> = vec![pb];
                    {
                        let mut extra: Vec<((f64, f64), f64)> = main_pts
                            .iter()
                            .filter(|&&(_, l)| l == layer)
                            .map(|&(p, _)| (p, (p.0 - pa.0).hypot(p.1 - pa.1)))
                            .collect();
                        extra.sort_by(|a, b| {
                            a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)
                        });
                        for (p, _) in extra.into_iter().take(4) {
                            if !cands
                                .iter()
                                .any(|&c| (c.0 - p.0).hypot(c.1 - p.1) < 1e-6)
                            {
                                cands.push(p);
                            }
                        }
                    }
                    if let Some(way) = cands.iter().find_map(|&pb2| {
                        geom::route_tunnel_ml(
                            &cidx,
                            pa,
                            layer,
                            pb2,
                            layer,
                            width,
                            via_r2,
                            &signal_layers,
                            net_id,
                            12.0,
                        )
                        .filter(&way_legal)
                    }) {
                        let route = &mut final_routes[ni];
                        let seg_start = route.segments.len();
                        let via_start = route.vias.len();
                        let n_l = board.layer_stack.layers.len() - 1;
                        for w in way.windows(2) {
                            let (a, b) = (w[0], w[1]);
                            if a.2 == b.2 {
                                if (a.0 - b.0).hypot(a.1 - b.1) > 1e-9 {
                                    route.segments.push(RouteSegment {
                                        layer: a.2,
                                        start: (a.0, a.1),
                                        end: (b.0, b.1),
                                        width_mm: width,
                                    });
                                }
                            } else {
                                let dup = route
                                    .vias
                                    .iter()
                                    .any(|v| (v.x - a.0).hypot(v.y - a.1) < 1e-6);
                                if !dup {
                                    route.vias.push(RouteVia {
                                        x: a.0,
                                        y: a.1,
                                        from_layer: 0,
                                        to_layer: n_l,
                                    });
                                }
                            }
                        }
                        let n_vias = route.vias.len() - via_start;
                        route
                            .path_spans
                            .push((seg_start, route.segments.len() - seg_start));
                        route.path_parents.push(None);
                        route.via_spans.push((via_start, n_vias));
                        info!(
                            "continuity repair: '{}' ML-joined at ({:.1},{:.1})->({:.1},{:.1}) ({n_vias} via(s))",
                            board.nets[ni].name, pa.0, pa.1, pb.0, pb.1
                        );
                        repaired += 1;
                        continue;
                    }
                }
                // RIP-NEGOTIATION joint first (same contract as the
                // island bridge's last tier): the gap is fenced by
                // routed foreign copper — rip each blocker as it
                // surfaces, land the joint, rebuild every victim,
                // all-no-worse-or-revert.
                {
                    let mut snaps: Vec<(usize, Route)> = Vec::new();
                    let snap_self = final_routes[ni].clone();
                    let mut landed = false;
                    for _round in 0..4 {
                        let idx2 =
                            geom::ClearanceIndex::build(board, final_routes, Some(net_id));
                        let jp = geom::route_escape(&idx2, pa, pb, width, layer, net_id)
                            .or_else(|| {
                                geom::route_tunnel(&idx2, pa, pb, width, layer, net_id)
                            });
                        if let Some(jp) = jp {
                            if path_respects_courtyards(board, &jp) {
                                commit_escape(
                                    &mut final_routes[ni],
                                    &jp,
                                    layer,
                                    width,
                                    None,
                                    &board.nets[ni].name,
                                );
                                landed = true;
                            }
                            break;
                        }
                        if snaps.len() >= 3 {
                            break;
                        }
                        let Some(geom::Conflict::Track { net: vn, .. }) =
                            geom::escape_blocker(&idx2, pa, pb, width, layer, net_id)
                        else {
                            break;
                        };
                        let Some(vj) = board.nets.iter().position(|n| n.id == vn) else {
                            break;
                        };
                        if vj == ni
                            || board.nets[vj].plane_layer.is_some()
                            || final_routes[vj].is_empty()
                            || snaps.iter().any(|&(k, _)| k == vj)
                        {
                            break;
                        }
                        snaps.push((vj, final_routes[vj].clone()));
                        final_routes[vj] = Route::empty(final_routes[vj].net_id);
                    }
                    if landed {
                        let mut ok = true;
                        for &(vj, ref snap_v) in &snaps {
                            let vb =
                                pathfinder::unreached_sink_count(&board.nets[vj], board, snap_v);
                            let gb = net_pad_group_count(board, snap_v, vj);
                            let mut jgrid = RoutingGrid::build(board);
                            for (k, r) in final_routes.iter().enumerate() {
                                if k != vj && !r.is_empty() {
                                    pathfinder::block_route_geometry(&mut jgrid, r, board);
                                }
                            }
                            let mut fresh = Route::empty(snap_v.net_id);
                            pathfinder::extend_route(
                                &mut jgrid, &board.nets[vj], board, &mut fresh, 1.0, 1.0,
                                &[], &[], false, None,
                            );
                            {
                                let mut bans = Vec::new();
                                let mut trial_board: Vec<Route> = final_routes.clone();
                                trial_board[vj] = Route::empty(snap_v.net_id);
                                exact_commit_strip(
                                    board, &trial_board, vj, &mut fresh, 0, &mut bans,
                                );
                            }
                            final_routes[vj] = fresh;
                            if pathfinder::unreached_sink_count(
                                &board.nets[vj],
                                board,
                                &final_routes[vj],
                            ) > 0
                            {
                                offgrid_escape(board, final_routes, vj);
                            }
                            let va = pathfinder::unreached_sink_count(
                                &board.nets[vj],
                                board,
                                &final_routes[vj],
                            );
                            let ga = net_pad_group_count(board, &final_routes[vj], vj);
                            if va > vb || ga > gb {
                                ok = false;
                                break;
                            }
                        }
                        if ok {
                            info!(
                                "continuity repair: '{}' RIP-joined at ({:.1},{:.1})->({:.1},{:.1}) ({} victim(s) rebuilt)",
                                board.nets[ni].name, pa.0, pa.1, pb.0, pb.1, snaps.len()
                            );
                            repaired += 1;
                            continue;
                        }
                    }
                    for (jj, old) in snaps.drain(..).rev() {
                        final_routes[jj] = old;
                    }
                    final_routes[ni] = snap_self;
                }
                // No legal joint — REBUILD the whole net with the
                // late grid machinery (a gutted net is basically
                // unrouted; the escape/maze joint is the wrong tool
                // for a 4mm gap through congestion). Accept only a
                // strictly better pad grouping.
                let snap = final_routes[ni].clone();
                let g_before = net_pad_group_count(board, &snap, ni);
                let mut jgrid = RoutingGrid::build(board);
                for (k, r) in final_routes.iter().enumerate() {
                    if k != ni && !r.is_empty() {
                        pathfinder::block_route_geometry(&mut jgrid, r, board);
                    }
                }
                let mut fresh = Route::empty(snap.net_id);
                pathfinder::extend_route(
                    &mut jgrid, &board.nets[ni], board, &mut fresh, 1.0, 1.0, &[], &[],
                    false, None,
                );
                {
                    let mut bans = Vec::new();
                    let mut trial_board: Vec<Route> = final_routes.clone();
                    trial_board[ni] = Route::empty(snap.net_id);
                    exact_commit_strip(board, &trial_board, ni, &mut fresh, 0, &mut bans);
                }
                final_routes[ni] = fresh;
                if pathfinder::unreached_sink_count(&board.nets[ni], board, &final_routes[ni])
                    > 0
                {
                    offgrid_escape(board, final_routes, ni);
                }
                let g_after = net_pad_group_count(board, &final_routes[ni], ni);
                if g_after < g_before {
                    info!(
                        "continuity repair: '{}' REBUILT ({} -> {} pad group(s))",
                        board.nets[ni].name, g_before, g_after
                    );
                    repaired += 1;
                } else {
                    final_routes[ni] = snap;
                    log::warn!(
                        "continuity repair: '{}' split at ({:.1},{:.1})<->({:.1},{:.1}) — no legal joint, rebuild no better (honest)",
                        board.nets[ni].name, pa.0, pa.1, pb.0, pb.1
                    );
                }
                continue;
            };
            if !path_respects_courtyards(board, &path) {
                continue;
            }
            commit_escape(
                &mut final_routes[ni],
                &path,
                layer,
                width,
                None,
                &board.nets[ni].name,
            );
            info!(
                "continuity repair: '{}' rejoined at ({:.1},{:.1})->({:.1},{:.1}) ({d:.1}mm gap)",
                board.nets[ni].name, pa.0, pa.1, pb.0, pb.1
            );
            repaired += 1;
        }
    }
    repaired
}

/// EMISSION-MODEL ORPHAN-CHAIN SWEEP: a pour net's routed chain that
/// touches no pad, no via, and no fill of the SHIPPED copper is
/// orphan metal — KiCad groups it alone and reports it unconnected
/// (rigid C4: a 6mm staircase remnant the blanket fill used to flood
/// over dangled once the region followed placement). Deleting it is
/// the honest move: by definition nothing connects through it.
/// Mid-span contact with non-group copper (an unsplit T) keeps the
/// chain — endpoint adjacency alone must never justify deletion.
fn dsu_find(p: &mut [usize], mut a: usize) -> usize {
    while p[a] != a {
        p[a] = p[p[a]];
        a = p[a];
    }
    a
}

fn dsu_union(p: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (dsu_find(p, a), dsu_find(p, b));
    if ra != rb {
        p[ra] = rb;
    }
}

fn pour_orphan_chain_sweep(board: &Board, final_routes: &mut [Route]) -> usize {
    let via_r = board.layer_stack.via.pad_mm / 2.0;
    let mut swept = 0usize;
    for ni in 0..board.nets.len() {
        let pour = board.nets[ni]
            .plane_layer
            .and_then(|pl| board.layer_stack.layers.get(pl))
            .map(|l| l.kind == crate::types::LayerKind::Signal)
            .unwrap_or(false);
        // Whole-layer pours are back in scope: the rip/rebuild tier
        // leaves genuinely dangling GND track groups (measured:
        // 6mm+1.9mm pair). All four keep-tests (pad, via, T-contact,
        // fill) stand — the earlier defaults regression is re-gated
        // by them plus the T-merge; defaults re-verified per ship.
        if !pour || final_routes[ni].segments.is_empty() {
            continue;
        }
        let Some(polys) = output::kicad::emission_fill_polys(board, final_routes, ni)
        else {
            continue;
        };
        if let Ok(dir) = std::env::var("BHDL_PNR_DUMP_MIRROR") {
            use std::io::Write;
            if let Ok(mut f) = std::fs::File::create(format!(
                "{dir}/mirror_{}.txt",
                board.nets[ni].name.replace('/', "_")
            )) {
                for p in &polys {
                    for (x, y) in p {
                        let _ = writeln!(f, "{x} {y}");
                    }
                    let _ = writeln!(f, "---");
                }
            }
        }
        let mut own_pads: Vec<(f64, f64, f64, f64)> = Vec::new();
        for comp in &board.components {
            let (co, sn) = (comp.theta.cos(), comp.theta.sin());
            let quarter =
                ((comp.theta / std::f64::consts::FRAC_PI_2).round() as i64).rem_euclid(2);
            for pin in &comp.pins {
                if pin.net != Some(board.nets[ni].id) || pin.unplaced {
                    continue;
                }
                let gx = comp.x + pin.dx * co - pin.dy * sn;
                let gy = comp.y + pin.dx * sn + pin.dy * co;
                let (pw, ph) = match &pin.pad {
                    Some(p) => (p.width_mm, p.height_mm),
                    None => (0.5, 0.5),
                };
                let (pw, ph) = if quarter == 1 { (ph, pw) } else { (pw, ph) };
                own_pads.push((gx, gy, pw / 2.0, ph / 2.0));
            }
        }
        let bboxes: Vec<(f64, f64, f64, f64)> = polys
            .iter()
            .map(|p| {
                p.iter().fold(
                    (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
                    |(x0, y0, x1, y1), &(x, y)| {
                        (x0.min(x), y0.min(y), x1.max(x), y1.max(y))
                    },
                )
            })
            .collect();
        // Fragment-RESOLVING membership: which piece of fill covers
        // this point. The identity matters — "touches some same-net
        // fill" is not connectivity once the fill is in pieces.
        let fill_hit = |x: f64, y: f64| -> Option<usize> {
            for (pi, poly) in polys.iter().enumerate() {
                let (x0, y0, x1, y1) = bboxes[pi];
                if x < x0 || x > x1 || y < y0 || y > y1 {
                    continue;
                }
                let mut inside = false;
                let mm = poly.len();
                for k in 0..mm {
                    let (px1, py1) = poly[k];
                    let (px2, py2) = poly[(k + 1) % mm];
                    if (py1 > y) != (py2 > y)
                        && x < (px2 - px1) * (y - py1) / (py2 - py1) + px1
                    {
                        inside = !inside;
                    }
                }
                if inside {
                    return Some(pi);
                }
            }
            None
        };
        if polys.is_empty() {
            continue;
        }
        // ONE union-find whose nodes are SEGMENTS, VIAS and FILL
        // FRAGMENTS. The sweep used to group segments only and read a
        // via as a self-evident anchor; that is wrong in both
        // directions. Measured: an F.Cu fill fragment holding no pad,
        // tied by a single via to an equally stranded B.Cu fragment,
        // shipped as a KiCad zone island — the pad-driven island
        // bridge could not see it (no pad to bridge from) and this
        // sweep kept its stub alive because the stub touched "some"
        // fill. Deleting the stub alone would not have helped either:
        // the via still anchored the fragment. A via is a NODE, not a
        // verdict, and copper is grounded only when its whole
        // component reaches a pad or the main pour body.
        let segs = final_routes[ni].segments.clone();
        let vias: Vec<(f64, f64)> =
            final_routes[ni].vias.iter().map(|v| (v.x, v.y)).collect();
        let n = segs.len();
        let (nv, np) = (vias.len(), polys.len());
        let mut dsu: Vec<usize> = (0..n + nv + np).collect();
        // Spatial index: walk each segment rather than filling its
        // bbox, so a long diagonal costs its length and not its area.
        let mut sgrid: crate::det::HashMap<(i64, i64, usize), Vec<usize>> =
            Default::default();
        for (i, sg) in segs.iter().enumerate() {
            let len = (sg.end.0 - sg.start.0).hypot(sg.end.1 - sg.start.1);
            let steps = ((len / 0.5).ceil() as usize).max(1);
            let mut last = (i64::MIN, i64::MIN);
            for s in 0..=steps {
                let t = s as f64 / steps as f64;
                let x = sg.start.0 + (sg.end.0 - sg.start.0) * t;
                let y = sg.start.1 + (sg.end.1 - sg.start.1) * t;
                let c = (x.floor() as i64, y.floor() as i64);
                if c == last {
                    continue;
                }
                last = c;
                sgrid.entry((c.0, c.1, sg.layer)).or_default().push(i);
            }
        }
        let near = |x: f64, y: f64, layer: usize| -> Vec<usize> {
            let (cx, cy) = (x.floor() as i64, y.floor() as i64);
            let mut out: Vec<usize> = Vec::new();
            for dx in -1..=1i64 {
                for dy in -1..=1i64 {
                    if let Some(c) = sgrid.get(&(cx + dx, cy + dy, layer)) {
                        out.extend_from_slice(c);
                    }
                }
            }
            out.sort_unstable();
            out.dedup();
            out
        };
        // seg <-> seg: a shared endpoint, or an endpoint resting
        // mid-span on same-layer copper (the unsplit T).
        for (i, sg) in segs.iter().enumerate() {
            for &pt in &[sg.start, sg.end] {
                for j in near(pt.0, pt.1, sg.layer) {
                    if j == i {
                        continue;
                    }
                    let o = &segs[j];
                    if (o.start.0 - pt.0).hypot(o.start.1 - pt.1) <= 0.011
                        || (o.end.0 - pt.0).hypot(o.end.1 - pt.1) <= 0.011
                        || geom::segment_point_too_close(
                            o.start,
                            o.end,
                            pt,
                            (o.width_mm + sg.width_mm) / 2.0 + 0.01,
                        )
                    {
                        dsu_union(&mut dsu, i, j);
                    }
                }
            }
        }
        // via <-> seg on ANY layer: that is what a via is for. This
        // replaces the old "a via joins track layers, so keep the
        // chain" special case with the structural fact.
        for (vi, &(vx, vy)) in vias.iter().enumerate() {
            for l in 0..board.layer_stack.layers.len() {
                for j in near(vx, vy, l) {
                    let o = &segs[j];
                    if (o.start.0 - vx).hypot(o.start.1 - vy) <= via_r + 0.05
                        || (o.end.0 - vx).hypot(o.end.1 - vy) <= via_r + 0.05
                        || geom::segment_point_too_close(
                            o.start,
                            o.end,
                            (vx, vy),
                            via_r + o.width_mm / 2.0,
                        )
                    {
                        dsu_union(&mut dsu, n + vi, j);
                    }
                }
            }
        }
        // fill fragment <-> copper. Only pour-layer tracks can meet
        // the primary-face fill; a via crosses every layer.
        let pl = board.nets[ni].plane_layer.unwrap_or(0);
        for (i, sg) in segs.iter().enumerate() {
            if sg.layer != pl {
                continue;
            }
            let len = (sg.end.0 - sg.start.0).hypot(sg.end.1 - sg.start.1);
            let steps = ((len / 0.3).ceil() as usize).max(1);
            for s in 0..=steps {
                let t = s as f64 / steps as f64;
                if let Some(pi) = fill_hit(
                    sg.start.0 + (sg.end.0 - sg.start.0) * t,
                    sg.start.1 + (sg.end.1 - sg.start.1) * t,
                ) {
                    dsu_union(&mut dsu, i, n + nv + pi);
                }
            }
        }
        for (vi, &(vx, vy)) in vias.iter().enumerate() {
            if let Some(pi) = fill_hit(vx, vy) {
                dsu_union(&mut dsu, n + vi, n + nv + pi);
            }
        }
        // Grounding: the main pour body is the ONE true source. A pad
        // is NOT — it is a WITNESS, counted per component. Measured
        // (the DC-jack island): a real GND pad fed a 0.6mm stub into a
        // via-in-pad whose fragment pair reached nothing else. "A pad
        // grounds its fragment" was locally true and globally wrong —
        // the whole assembly floated, and KiCad reported zone<->zone
        // precisely because the pad WAS connected to something. But a
        // track chain joining TWO pads with no fill contact is real
        // copper and must survive. So: a component is live if it
        // contains the main fill, OR at least two DISTINCT pads. One
        // pad alone anchors nothing — its copper is swept, and the
        // bare pad falls through to pour_unserved_pad_route, which
        // routes it to real copper instead.
        let mut grounded = vec![false; n + nv + np];
        if let Some(main_pi) = polys
            .iter()
            .enumerate()
            .max_by_key(|(_, p)| p.len())
            .map(|(i, _)| i)
        {
            grounded[n + nv + main_pi] = true;
        }
        // Per-root distinct-pad witnesses (one pad counted once no
        // matter how many segments, vias or fragments it touches).
        let mut pad_roots: Vec<crate::det::HashSet<usize>> =
            vec![Default::default(); own_pads.len()];
        for (pad_i, &(cx, cy, hx, hy)) in own_pads.iter().enumerate() {
            // A pad grounds the fragment its COPPER reaches, which is
            // not simply the fragment under its centre: a thermally
            // relieved pad sits in a void and meets the fill only at
            // its spoke tips. Those tips are fill VERTICES sitting on
            // the pad boundary — test for them, rather than casting
            // rays outward, which happily jumps the relief gap and
            // grounds a fragment the pad never touches.
            if let Some(pi) = fill_hit(cx, cy) {
                let r = dsu_find(&mut dsu, n + nv + pi);
                pad_roots[pad_i].insert(r);
            }
            // ON OR INSIDE the pad edge, not merely near it. The void
            // contour around a relieved pad hugs the pad at clearance
            // distance, so any tolerance wider than the clearance
            // grounds a fragment that never touches the pad — measured:
            // a 158-vertex GND island next to the DC jack was declared
            // grounded by a 0.15mm box and shipped as a zone island. A
            // spoke tip overlaps the pad; a void contour does not.
            for pi in 0..np {
                if polys[pi].iter().any(|&(vx, vy)| {
                    (vx - cx).abs() <= hx + 0.001 && (vy - cy).abs() <= hy + 0.001
                }) {
                    let r = dsu_find(&mut dsu, n + nv + pi);
                    pad_roots[pad_i].insert(r);
                }
            }
            for (i, sg) in segs.iter().enumerate() {
                for &pt in &[sg.start, sg.end] {
                    if (pt.0 - cx).abs() <= hx + 0.05 && (pt.1 - cy).abs() <= hy + 0.05 {
                        let r = dsu_find(&mut dsu, i);
                        pad_roots[pad_i].insert(r);
                    }
                }
            }
            for (vi, &(vx, vy)) in vias.iter().enumerate() {
                if (vx - cx).abs() <= hx + 0.05 && (vy - cy).abs() <= hy + 0.05 {
                    let r = dsu_find(&mut dsu, n + vi);
                    pad_roots[pad_i].insert(r);
                }
            }
        }
        let mut root_ok: crate::det::HashSet<usize> = Default::default();
        for k in 0..n + nv + np {
            if grounded[k] {
                let r = dsu_find(&mut dsu, k);
                root_ok.insert(r);
            }
        }
        // Two distinct pad witnesses make a component live even with
        // no fill member at all (a plain routed pad-to-pad chain).
        let mut root_pad_count: crate::det::HashMap<usize, usize> = Default::default();
        for pr in &pad_roots {
            for &r in pr.iter() {
                *root_pad_count.entry(r).or_insert(0) += 1;
            }
        }
        for (&r, &c) in root_pad_count.iter() {
            if c >= 2 {
                root_ok.insert(r);
            }
        }
        if std::env::var("BHDL_PNR_PROBE").is_ok() && np > 1 {
            for pi in 0..np {
                let (x0, y0, x1, y1) = bboxes[pi];
                log::info!(
                    "[probe] frag '{}' #{pi} {} verts bbox {x0:.1},{y0:.1}-{x1:.1},{y1:.1} grounded={} root={}",
                    board.nets[ni].name, polys[pi].len(),
                    grounded[n + nv + pi],
                    root_ok.contains(&dsu_find(&mut dsu, n + nv + pi))
                );
            }
        }
        let keep_seg: Vec<bool> = (0..n)
            .map(|i| root_ok.contains(&dsu_find(&mut dsu, i)))
            .collect();
        let keep_via: Vec<bool> = (0..nv)
            .map(|vi| root_ok.contains(&dsu_find(&mut dsu, n + vi)))
            .collect();
        let mut drop: Vec<usize> = (0..n).filter(|&i| !keep_seg[i]).collect();
        if std::env::var("BHDL_PNR_PROBE").is_ok() {
            for &i in &drop {
                let sg = &segs[i];
                log::info!(
                    "[probe] orphan-sweep DROP '{}' l{} ({:.2},{:.2})->({:.2},{:.2})",
                    board.nets[ni].name, sg.layer,
                    sg.start.0, sg.start.1, sg.end.0, sg.end.1
                );
            }
            for (vi, &(vx, vy)) in vias.iter().enumerate() {
                if !keep_via[vi] {
                    log::info!(
                        "[probe] orphan-sweep DROP-VIA '{}' ({vx:.2},{vy:.2})",
                        board.nets[ni].name
                    );
                }
            }
        }
        drop.sort_unstable_by(|a, b| b.cmp(a));
        for sk in drop {
            let r = &mut final_routes[ni];
            r.segments.remove(sk);
            for (qs, ql) in r.path_spans.iter_mut() {
                if *qs > sk {
                    *qs -= 1;
                } else if sk < *qs + *ql {
                    *ql = ql.saturating_sub(1);
                }
            }
            swept += 1;
        }
        // A via whose whole component reaches nothing is dead copper —
        // and, left behind, it re-anchors the very fragment the sweep
        // just tried to strand.
        let mut vk = keep_via.iter();
        final_routes[ni].vias.retain(|_| *vk.next().unwrap_or(&true));
    }
    swept
}

fn pour_net_free_end_trim(board: &Board, final_routes: &mut [Route]) -> usize {
    let via_r = board.layer_stack.via.pad_mm / 2.0;
    let mut pruned = 0usize;
    for i in 0..board.nets.len() {
        let pour = board.nets[i]
            .plane_layer
            .and_then(|pl| board.layer_stack.layers.get(pl))
            .map(|l| l.kind == crate::types::LayerKind::Signal)
            .unwrap_or(false)
            && board.nets[i].plane_region.is_some();
        if !pour || final_routes[i].segments.is_empty() {
            continue;
        }
        let mut own_pads: Vec<(f64, f64, f64, f64)> = Vec::new();
        for comp in &board.components {
            let (co, sn) = (comp.theta.cos(), comp.theta.sin());
            let quarter = ((comp.theta / std::f64::consts::FRAC_PI_2).round()
                as i64)
                .rem_euclid(2);
            for pin in &comp.pins {
                if pin.net != Some(board.nets[i].id) || pin.unplaced {
                    continue;
                }
                let gx = comp.x + pin.dx * co - pin.dy * sn;
                let gy = comp.y + pin.dx * sn + pin.dy * co;
                let (pw, ph) = match &pin.pad {
                    Some(p) => (p.width_mm, p.height_mm),
                    None => (0.5, 0.5),
                };
                let (pw, ph) = if quarter == 1 { (ph, pw) } else { (pw, ph) };
                own_pads.push((gx, gy, pw / 2.0, ph / 2.0));
            }
        }
        // T-SPLIT first: a rescue join landing mid-span on its target
        // is electrically connected but KiCad's dangling test demands
        // a REAL junction — split the host segment at every such
        // landing (the sweep's own splitter runs before the
        // post-sweep rescue, so its joins arrive unsplit).
        loop {
            let r = &final_routes[i];
            let mut split: Option<(usize, (f64, f64))> = None;
            'find_t: for sg in &r.segments {
                for pt in [sg.start, sg.end] {
                    for (sk, host) in r.segments.iter().enumerate() {
                        if host.layer != sg.layer {
                            continue;
                        }
                        let (dx, dy) =
                            (host.end.0 - host.start.0, host.end.1 - host.start.1);
                        let l2 = dx * dx + dy * dy;
                        if l2 <= 1e-12 {
                            continue;
                        }
                        let t = ((pt.0 - host.start.0) * dx
                            + (pt.1 - host.start.1) * dy)
                            / l2;
                        if t <= 1e-6 || t >= 1.0 - 1e-6 {
                            continue;
                        }
                        let q = (host.start.0 + t * dx, host.start.1 + t * dy);
                        if (pt.0 - q.0).hypot(pt.1 - q.1) < 0.01
                            && (pt.0 - host.start.0).hypot(pt.1 - host.start.1)
                                > 1e-6
                            && (pt.0 - host.end.0).hypot(pt.1 - host.end.1) > 1e-6
                        {
                            split = Some((sk, pt));
                            break 'find_t;
                        }
                    }
                }
            }
            let Some((sk, pt)) = split else { break };
            let r = &mut final_routes[i];
            let host = r.segments[sk].clone();
            r.segments[sk].end = pt;
            r.segments.insert(
                sk + 1,
                RouteSegment {
                    layer: host.layer,
                    start: pt,
                    end: host.end,
                    width_mm: host.width_mm,
                },
            );
            for (qs, ql) in r.path_spans.iter_mut() {
                if *qs <= sk && sk < *qs + *ql {
                    *ql += 1;
                } else if *qs > sk {
                    *qs += 1;
                }
            }
        }
        loop {
            let r = &final_routes[i];
            let mut drop: Option<usize> = None;
            'segs: for (sk, sg) in r.segments.iter().enumerate() {
                for &e in &[sg.start, sg.end] {
                    let anchored = r
                        .segments
                        .iter()
                        .enumerate()
                        .any(|(sj, s2)| {
                            sj != sk
                                && s2.layer == sg.layer
                                && geom::point_segment_dist(e, s2.start, s2.end)
                                    <= 0.01
                        })
                        || r.vias
                            .iter()
                            .any(|v| (v.x - e.0).hypot(v.y - e.1) <= via_r)
                        || own_pads.iter().any(|&(cx, cy, hx, hy)| {
                            (e.0 - cx).abs() <= hx && (e.1 - cy).abs() <= hy
                        });
                    if !anchored {
                        drop = Some(sk);
                        break 'segs;
                    }
                }
            }
            if drop.is_none() && std::env::var("BHDL_PNR_PROBE").is_ok() {
                for (sk, sg) in r.segments.iter().enumerate() {
                    for &e in &[sg.start, sg.end] {
                        if (e.0 - 49.296).hypot(e.1 - 39.45) < 0.2 {
                            let seg_a = r.segments.iter().enumerate().find(|(sj, s2)| {
                                *sj != sk
                                    && s2.layer == sg.layer
                                    && geom::point_segment_dist(e, s2.start, s2.end) <= 0.01
                            });
                            log::info!(
                                "[probe] final-trim tip ({:.3},{:.3}) anchored: seg={:?} via={} pad={}",
                                e.0, e.1,
                                seg_a.map(|(_, s2)| (s2.start, s2.end, s2.layer)),
                                r.vias.iter().any(|v| (v.x - e.0).hypot(v.y - e.1) <= via_r),
                                own_pads.iter().any(|&(cx, cy, hx, hy)| (e.0 - cx).abs() <= hx && (e.1 - cy).abs() <= hy)
                            );
                        }
                    }
                }
            }
            match drop {
                Some(sk) => {
                    let r = &mut final_routes[i];
                    if std::env::var("BHDL_PNR_PROBE").is_ok() {
                        let sg = &r.segments[sk];
                        log::info!(
                            "[probe] free-end-trim DROP '{}' l{} ({:.2},{:.2})->({:.2},{:.2})",
                            board.nets[i].name, sg.layer,
                            sg.start.0, sg.start.1, sg.end.0, sg.end.1
                        );
                    }
                    r.segments.remove(sk);
                    for (qs, ql) in r.path_spans.iter_mut() {
                        if *qs > sk {
                            *qs -= 1;
                        } else if sk < *qs + *ql {
                            *ql = ql.saturating_sub(1);
                        }
                    }
                    pruned += 1;
                }
                None => break,
            }
        }
    }
    pruned
}

fn plane_surface_rescue(board: &Board, final_routes: &mut Vec<Route>) -> usize {
    debug!(
        "surface rescue pass: {} plane net(s)",
        board.nets.iter().filter(|n| n.plane_layer.is_some()).count()
    );
    let comp_idx: crate::det::HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(k, c)| (c.id, k))
        .collect();
    let n_layers = board.layer_stack.layers.len();
    let mut rescued = 0usize;
    for i in 0..board.nets.len() {
        if board.nets[i].plane_layer.is_none() {
            continue;
        }
        let net_id = board.nets[i].id;
        // A rescue stub carries ONE pin's draw — neck it down like the
        // drop machinery's leaf share. A rail-width (0.6mm) stub
        // physically cannot leave a 0.5-pitch TQFP pad row (AVCC sat
        // stranded while 14 maze rescues connected roomier pads).
        let width = board
            .config
            .min_trace_width_mm
            .max(0.15)
            .min(board.nets[i].required_trace_width_mm);
        // MAROON pre-pass (pour-side pads, one raster per net): a pad
        // can pass the local swallow test while its fill pocket is an
        // ISLAND walled off by foreign-track voids — connectivity
        // demands the pocket reach a via/barrel/routed-track anchor.
        let maroon: crate::det::HashMap<(ComponentId, PinId), bool> = {
            let mut pts: Vec<((ComponentId, PinId), (f64, f64))> = Vec::new();
            if let Some(pl) = board.nets[i].plane_layer.filter(|&pl| {
                board.layer_stack.layers.get(pl).map(|l| l.kind)
                    == Some(crate::types::LayerKind::Signal)
            }) {
                for &(cid, pid) in &board.nets[i].pins {
                    let Some(&ci) = comp_idx.get(&cid) else { continue };
                    let comp = &board.components[ci];
                    let surf = match comp.side {
                        BoardSide::Top => 0,
                        BoardSide::Bottom => n_layers - 1,
                    };
                    if surf != pl {
                        continue;
                    }
                    let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pid) else {
                        continue;
                    };
                    if pin.unplaced
                        || pin.pad.as_ref().and_then(|p| p.drill_mm).is_some()
                    {
                        continue;
                    }
                    let (co, sn) = (comp.theta.cos(), comp.theta.sin());
                    pts.push((
                        (cid, pid),
                        (
                            comp.x + pin.dx * co - pin.dy * sn,
                            comp.y + pin.dx * sn + pin.dy * co,
                        ),
                    ));
                }
                if !pts.is_empty() {
                    let flags = output::kicad::plane_pads_marooned(
                        board,
                        final_routes,
                        net_id,
                        pl,
                        &pts.iter().map(|p| p.1).collect::<Vec<_>>(),
                    );
                    pts.iter()
                        .zip(flags)
                        .map(|(&(k, _), f)| (k, f))
                        .collect()
                } else {
                    Default::default()
                }
            } else {
                Default::default()
            }
        };
        // Pads not touching ANY same-net copper.
        let mut todo: Vec<((f64, f64), usize, Option<usize>, bool)> = Vec::new();
        for &(cid, pid) in &board.nets[i].pins {
            let Some(&ci) = comp_idx.get(&cid) else { continue };
            let comp = &board.components[ci];
            let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pid) else { continue };
            if pin.unplaced {
                continue;
            }
            let cos_t = comp.theta.cos();
            let sin_t = comp.theta.sin();
            let px = comp.x + pin.dx * cos_t - pin.dy * sin_t;
            let py = comp.y + pin.dx * sin_t + pin.dy * cos_t;
            // THT pads pierce the plane directly — but only INSIDE
            // their rail's region (see plane_via_drops).
            if pin.pad.as_ref().map(|p| p.drill_mm.is_some()).unwrap_or(false) {
                let in_region = match board.nets[i].plane_region {
                    None => true,
                    Some((rx0, ry0, rx1, ry1)) => {
                        px > rx0 + 0.05
                            && px < rx1 - 0.05
                            && py > ry0 + 0.05
                            && py < ry1 - 0.05
                    }
                };
                if in_region {
                    continue;
                }
            }
            // POUR-SIDE pads live ON the fill's layer: connected by
            // contact/relief, so the island-has-via test below is
            // wrong for them (it "rescued" fill-connected pads with a
            // stub + via whose far end is bare — the mixer anti-bias
            // 4× via_dangling family). Their stranded test is fill
            // REACH: a pad inside a merged foreign punch (a void
            // pocket in the dense region) gets no relief and needs a
            // surface join; one outside is a fill anchor — skip.
            let pour_side = {
                let surf = match comp.side {
                    BoardSide::Top => 0,
                    BoardSide::Bottom => n_layers - 1,
                };
                Some(surf) == board.nets[i].plane_layer
            };
            let half = pin
                .pad
                .as_ref()
                .map(|p| p.width_mm.min(p.height_mm) / 2.0)
                .unwrap_or(0.25);
            let r = &final_routes[i];
            // "Touches copper" is not "reaches the plane": a drop
            // stub whose via was swallowed leaves the pad on a DEAD
            // ISLAND (the supply_tree 1.65mm fragment). The pad's
            // copper component must contain a VIA.
            use crate::routing::pathfinder::route_components;
            let comps = route_components(r);
            let via_r_pl = board.layer_stack.via.pad_mm / 2.0;
            let pin_layer = match comp.side {
                BoardSide::Top => 0,
                BoardSide::Bottom => n_layers - 1,
            };
            let pad_comp: Option<usize> = r
                .segments
                .iter()
                .enumerate()
                .find(|(_, sg)| {
                    sg.layer == pin_layer
                        && geom::point_segment_dist((px, py), sg.start, sg.end)
                            < sg.width_mm / 2.0 + half - 0.001
                })
                .map(|(si, _)| comps[si]);
            // A via only counts when it actually REACHES the plane:
            // one swallowed by a fill punch (or outside the rail's
            // region) is bare barrel in a hole — AVCC sat "connected"
            // behind exactly such a via while KiCad saw nothing.
            let merged = output::kicad::merge_holes(output::kicad::plane_foreign_holes(
                board,
                final_routes,
                net_id,
            ));
            let region = board.nets[i].plane_region;
            let island_has_via = pad_comp.map_or(false, |pc| {
                r.vias.iter().any(|v| {
                    !output::kicad::plane_swallows(board, &merged, v.x, v.y, via_r_pl, region)
                        && r.segments.iter().enumerate().any(|(si, sg)| {
                            comps[si] == pc
                                && geom::point_segment_dist((v.x, v.y), sg.start, sg.end)
                                    < sg.width_mm / 2.0 + via_r_pl
                        })
                })
            });
            let stranded = if pour_side {
                output::kicad::plane_swallows(board, &merged, px, py, half, region)
                    || maroon.get(&(cid, pid)).copied().unwrap_or(false)
            } else {
                pad_comp.is_none() || !island_has_via
            };
            if stranded {
                let layer = match comp.side {
                    BoardSide::Top => 0,
                    BoardSide::Bottom => n_layers - 1,
                };
                debug!(
                    "surface rescue queue: '{}' pad ({px:.2},{py:.2}) island={:?} has_via={island_has_via} pour_side={pour_side}",
                    board.nets[i].name, pad_comp
                );
                todo.push(((px, py), layer, pad_comp, pour_side));
            }
        }
        for ((px, py), layer, pad_comp, pour_side) in todo {
            // Candidates: projections onto same-net same-layer copper
            // + drop via centers — EXCLUDING the pad's own dead
            // island (attaching to it connects nothing).
            let r = &final_routes[i];
            use crate::routing::pathfinder::route_components;
            let comps = route_components(r);
            let mut attach: Vec<((f64, f64), f64)> = r
                .segments
                .iter()
                .enumerate()
                .filter(|(si, sg)| sg.layer == layer && Some(comps[*si]) != pad_comp)
                .map(|(_, sg)| {
                    let (dx, dy) = (sg.end.0 - sg.start.0, sg.end.1 - sg.start.1);
                    let l2 = dx * dx + dy * dy;
                    let t = if l2 <= 1e-12 {
                        0.0
                    } else {
                        (((px - sg.start.0) * dx + (py - sg.start.1) * dy) / l2)
                            .clamp(0.0, 1.0)
                    };
                    let q = (sg.start.0 + t * dx, sg.start.1 + t * dy);
                    (q, (px - q.0).hypot(py - q.1))
                })
                .collect();
            for v in &r.vias {
                attach.push((((v.x, v.y)), (px - v.x).hypot(py - v.y)));
            }
            attach.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            attach.truncate(12);
            // Sources: the pad center, plus the dead island's segment
            // endpoints (the stub's far end often has open space the
            // pad itself lacks; overlap-connectivity carries the pad).
            let mut sources: Vec<(f64, f64)> = vec![(px, py)];
            if let Some(pc) = pad_comp {
                for (si, sg) in r.segments.iter().enumerate() {
                    if comps[si] == pc && sg.layer == layer {
                        sources.push(sg.start);
                        sources.push(sg.end);
                    }
                }
            }
            sources.dedup_by(|a, b| (a.0 - b.0).hypot(a.1 - b.1) < 1e-6);
            sources.truncate(10);
            let mut connected = false;
            // Exact route, then shove-assisted retry.
            let mut snaps: Vec<(usize, Route)> = Vec::new();
            'try_attach: for &src in &sources {
                for &(q, _) in &attach {
                for _round in 0..3 {
                    let idx = geom::ClearanceIndex::build(board, final_routes, Some(net_id));
                    if let Some(path) =
                        geom::route_escape(&idx, src, q, width, layer, net_id)
                    {
                        if !path_respects_courtyards(board, &path) {
                            break; // next attach candidate — shoving won't help our own rule
                        }
                        commit_escape(
                            &mut final_routes[i],
                            &path,
                            layer,
                            width,
                            None,
                            &board.nets[i].name,
                        );
                        info!(
                            "plane surface rescue: '{}' pad at ({px:.2},{py:.2}) joined by surface copper",
                            board.nets[i].name
                        );
                        connected = true;
                        break 'try_attach;
                    }
                    let Some(bl) =
                        geom::escape_blocker(&idx, src, q, width, layer, net_id)
                    else {
                        break;
                    };
                    if !try_shove_track(
                        board, final_routes, i, &bl, src, q, width, &mut snaps,
                    ) {
                        break;
                    }
                }
                }
            }
            // Stage 1.2 — MAZE TUNNEL (same layer): the shapes and
            // shoves above cannot thread a QFP interior — a 0.5mm-
            // pitch pad row is impassable sideways, but the footprint
            // body behind it is open copper and the corner gaps lead
            // out (uno free-MCU: UGND walled in by its own pin row
            // while GND copper sat 1.5mm away). One exact A* per
            // attach candidate, nearest first.
            if !connected {
                for (jj, old) in snaps.drain(..).rev() {
                    final_routes[jj] = old;
                }
                let cidx = geom::ClearanceIndex::build(board, final_routes, Some(net_id));
                'tunnel: for &src in &sources {
                    for &(q, _) in attach.iter().take(4) {
                        if let Some(path) =
                            geom::route_tunnel(&cidx, src, q, width, layer, net_id)
                        {
                            if !path_respects_courtyards(board, &path) {
                                continue;
                            }
                            commit_escape(
                                &mut final_routes[i],
                                &path,
                                layer,
                                width,
                                None,
                                &board.nets[i].name,
                            );
                            info!(
                                "plane surface rescue: '{}' pad at ({px:.2},{py:.2}) joined by maze tunnel",
                                board.nets[i].name
                            );
                            connected = true;
                            rescued += 1;
                            break 'tunnel;
                        }
                    }
                }
                if connected {
                    continue;
                }
            }
            // Stage 1.5 — VICTIM RIP: the blocker crosses the pad
            // column and every shape/shove fails (uno s5: a 0.3mm
            // foreign jog crossing exactly between two same-net USB
            // pads fenced the whole gap). Rip the blocking net
            // wholesale, make the exact join through the freed
            // corridor, rebuild the victim on the updated board
            // (grid extend + exact strip + ladder top-up), and accept
            // only a strict total win — the join lands AND the victim
            // ends no worse than it started. Plane pads never get the
            // completion pass's shove_one_blocker, so this is their
            // only rip path.
            if !connected {
                for (jj, old) in snaps.drain(..).rev() {
                    final_routes[jj] = old;
                }
                let mut victims: Vec<usize> = Vec::new();
                {
                    let idx = geom::ClearanceIndex::build(board, final_routes, Some(net_id));
                    for &(q, _) in attach.iter().take(6) {
                        if let Some(geom::Conflict::Track { net: vn, .. }) =
                            geom::escape_blocker(&idx, (px, py), q, width, layer, net_id)
                        {
                            if let Some(vj) = board.nets.iter().position(|n| n.id == vn) {
                                if board.nets[vj].plane_layer.is_none()
                                    && !final_routes[vj].is_empty()
                                    && !victims.contains(&vj)
                                {
                                    victims.push(vj);
                                }
                            }
                        }
                    }
                }
                'victim: for &vj in victims.iter().take(3) {
                    let snap_v = final_routes[vj].clone();
                    let snap_g = final_routes[i].clone();
                    final_routes[vj] = Route::empty(snap_v.net_id);
                    let mut joined = false;
                    'join: for &src in &sources {
                        for &(q, _) in &attach {
                            let idx = geom::ClearanceIndex::build(
                                board,
                                final_routes,
                                Some(net_id),
                            );
                            if let Some(path) =
                                geom::route_escape(&idx, src, q, width, layer, net_id)
                            {
                                if !path_respects_courtyards(board, &path) {
                                    continue;
                                }
                                commit_escape(
                                    &mut final_routes[i],
                                    &path,
                                    layer,
                                    width,
                                    None,
                                    &board.nets[i].name,
                                );
                                joined = true;
                                break 'join;
                            }
                        }
                    }
                    if !joined {
                        final_routes[vj] = snap_v;
                        continue;
                    }
                    // Rebuild the victim from scratch on the board
                    // that now carries the join.
                    let mut jgrid = RoutingGrid::build(board);
                    for (k, r) in final_routes.iter().enumerate() {
                        if k != vj && !r.is_empty() {
                            pathfinder::block_route_geometry(&mut jgrid, r, board);
                        }
                    }
                    let mut fresh = Route::empty(snap_v.net_id);
                    pathfinder::extend_route(
                        &mut jgrid, &board.nets[vj], board, &mut fresh, 1.0, 1.0, &[], &[],
                        false, None,
                    );
                    {
                        let mut bans = Vec::new();
                        let mut trial_board: Vec<Route> = final_routes.clone();
                        trial_board[vj] = Route::empty(snap_v.net_id);
                        exact_commit_strip(board, &trial_board, vj, &mut fresh, 0, &mut bans);
                    }
                    final_routes[vj] = fresh;
                    if pathfinder::unreached_sink_count(
                        &board.nets[vj], board, &final_routes[vj],
                    ) > 0
                    {
                        offgrid_escape(board, final_routes, vj);
                    }
                    let v_before =
                        pathfinder::unreached_sink_count(&board.nets[vj], board, &snap_v);
                    let v_after = pathfinder::unreached_sink_count(
                        &board.nets[vj], board, &final_routes[vj],
                    );
                    if v_after <= v_before {
                        info!(
                            "plane surface rescue: '{}' pad at ({px:.2},{py:.2}) joined after ripping '{}' (victim unreached {v_before} -> {v_after})",
                            board.nets[i].name, board.nets[vj].name
                        );
                        connected = true;
                        break 'victim;
                    }
                    final_routes[vj] = snap_v;
                    final_routes[i] = snap_g;
                }
            }
            // Stage 2 — EXACT ROUTED DROP: every surface path to
            // existing copper is fenced; make NEW plane contact
            // instead. Ring-search a legal via site (exact barrel +
            // punchability + swallow + region rules), exact-route the
            // stub, commit stub + via. The grid-based routed-drop
            // fallback failed exactly here (grid fully blocked); the
            // continuous router threads what the grid cannot.
            // Stage 1.7 — VIA HOP (pour-side pads): every same-layer
            // join is fenced, but same-net copper often runs on the
            // OTHER layer right past the pocket (probe evidence: the
            // stranded vbias pads had a clean same-x candidate 10mm
            // up that no same-layer join could thread). One TRACK
            // via: a stub on the pad's layer to a claimed site, then
            // an exact run on the other layer to same-net copper
            // there. Both via ends carry REAL segments — structurally
            // immune to via_dangling (unlike plane drops, no fill
            // dependence on the far side).
            if !connected && pour_side && n_layers >= 2 {
                for (jj, old) in snaps.drain(..).rev() {
                    final_routes[jj] = old;
                }
                let other = if layer == 0 { n_layers - 1 } else { 0 };
                let via_r = board.layer_stack.via.pad_mm / 2.0;
                // Attach candidates on the OTHER layer: same-net
                // segment projections and via centers.
                let r = &final_routes[i];
                let mut attach_b: Vec<((f64, f64), f64)> = r
                    .segments
                    .iter()
                    .filter(|sg| sg.layer == other)
                    .map(|sg| {
                        let (dx, dy) =
                            (sg.end.0 - sg.start.0, sg.end.1 - sg.start.1);
                        let l2 = dx * dx + dy * dy;
                        let t = if l2 <= 1e-12 {
                            0.0
                        } else {
                            (((px - sg.start.0) * dx + (py - sg.start.1) * dy)
                                / l2)
                                .clamp(0.0, 1.0)
                        };
                        let q = (sg.start.0 + t * dx, sg.start.1 + t * dy);
                        (q, (px - q.0).hypot(py - q.1))
                    })
                    .collect();
                for v in &r.vias {
                    // A via with no copper on the TARGET layer is a
                    // dead landing — hopping to it ships
                    // via_dangling, not connectivity.
                    let live = r.segments.iter().any(|sg| {
                        sg.layer == other
                            && ((sg.start.0 - v.x).hypot(sg.start.1 - v.y)
                                <= via_r + 0.05
                                || (sg.end.0 - v.x).hypot(sg.end.1 - v.y)
                                    <= via_r + 0.05)
                    });
                    if live {
                        attach_b.push(((v.x, v.y), (px - v.x).hypot(py - v.y)));
                    }
                }
                attach_b.sort_by(|a, b| {
                    a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)
                });
                attach_b.truncate(8);
                if !attach_b.is_empty() {
                    'viahop: for &src in &sources {
                        let mut vsnaps: Vec<(usize, Route)> = Vec::new();
                        let mut budget = 2usize;
                        let Some((vx, vy)) = claim_via_site(
                            board,
                            final_routes,
                            i,
                            src,
                            via_r,
                            None,
                            &mut vsnaps,
                            &mut budget,
                        ) else {
                            for (jj, old) in vsnaps.drain(..).rev() {
                                final_routes[jj] = old;
                            }
                            continue;
                        };
                        let idx = geom::ClearanceIndex::build(
                            board,
                            final_routes,
                            Some(net_id),
                        );
                        let Some(fpath) =
                            geom::route_escape(&idx, src, (vx, vy), width, layer, net_id)
                        else {
                            for (jj, old) in vsnaps.drain(..).rev() {
                                final_routes[jj] = old;
                            }
                            continue;
                        };
                        if !path_respects_courtyards(board, &fpath) {
                            for (jj, old) in vsnaps.drain(..).rev() {
                                final_routes[jj] = old;
                            }
                            continue;
                        }
                        let mut done = false;
                        for &(q, _) in &attach_b {
                            // A target at the claimed site itself
                            // yields a DEGENERATE B-leg: commit_escape
                            // silently pushes nothing while the
                            // F-side commit already carried the via —
                            // the exact anatomy of the seed-7
                            // via_dangling pair.
                            if (q.0 - vx).hypot(q.1 - vy) < via_r * 2.0 {
                                continue;
                            }
                            let Some(bpath) = geom::route_escape(
                                &idx,
                                (vx, vy),
                                q,
                                width,
                                other,
                                net_id,
                            ) else {
                                continue;
                            };
                            let blen: f64 = bpath
                                .windows(2)
                                .map(|w| (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1))
                                .sum();
                            if blen < 0.05 {
                                continue;
                            }
                            commit_escape(
                                &mut final_routes[i],
                                &fpath,
                                layer,
                                width,
                                Some(RouteVia {
                                    x: vx,
                                    y: vy,
                                    from_layer: layer,
                                    to_layer: other,
                                }),
                                &board.nets[i].name,
                            );
                            commit_escape(
                                &mut final_routes[i],
                                &bpath,
                                other,
                                width,
                                None,
                                &board.nets[i].name,
                            );
                            t_split_host_at(&mut final_routes[i], q, other);
                            if std::env::var("BHDL_PNR_PROBE").is_ok() {
                                log::info!(
                                    "[probe] hop internals: fpath {} pt(s), bpath {} pt(s), blen {blen:.3}, via ({vx:.3},{vy:.3}), q ({:.3},{:.3}), segs now {}",
                                    fpath.len(), bpath.len(), q.0, q.1,
                                    final_routes[i].segments.len()
                                );
                            }
                            info!(
                                "plane surface rescue: '{}' pad at ({px:.2},{py:.2}) joined by VIA HOP to the other layer",
                                board.nets[i].name
                            );
                            connected = true;
                            done = true;
                            break;
                        }
                        if done {
                            break 'viahop;
                        }
                        for (jj, old) in vsnaps.drain(..).rev() {
                            final_routes[jj] = old;
                        }
                    }
                }
                if connected {
                    rescued += 1;
                }
                continue;
            }
            if !connected {
                // Stage 1's failed shove deformations must not ship —
                // they opened corridors nothing uses (a stranded BOOT
                // bump shipped as track_dangling).
                for (jj, old) in snaps.drain(..).rev() {
                    final_routes[jj] = old;
                }
                let via_r = board.layer_stack.via.pad_mm / 2.0;
                let region = board.nets[i].plane_region;
                let merged = output::kicad::merge_holes(output::kicad::plane_foreign_holes(
                    board, final_routes, net_id,
                ));
                let mut budget = 4usize;
                'site: for ring in 0..24 {
                    let rr = 0.6 + ring as f64 * 0.35;
                    for k in 0..12 {
                        let ang = k as f64 * 2.0 * std::f64::consts::PI / 12.0;
                        let (vx, vy) = (px + rr * ang.cos(), py + rr * ang.sin());
                        if let Some((rx0, ry0, rx1, ry1)) = region {
                            if vx - via_r < rx0 + 0.05
                                || vy - via_r < ry0 + 0.05
                                || vx + via_r > rx1 - 0.05
                                || vy + via_r > ry1 - 0.05
                            {
                                continue;
                            }
                        }
                        if output::kicad::plane_swallows(
                            board, &merged, vx, vy, via_r, region,
                        ) {
                            continue;
                        }
                        let site_mark = snaps.len();
                        let mut site_ok = false;
                        for _ in 0..2 {
                            let idx = geom::ClearanceIndex::build(
                                board,
                                final_routes,
                                Some(net_id),
                            );
                            match idx.via_conflict(vx, vy, via_r, net_id) {
                                None => {
                                    site_ok = true;
                                    break;
                                }
                                Some(c @ geom::Conflict::Track { .. }) if budget > 0 => {
                                    if !try_shove_track(
                                        board, final_routes, i, &c, (vx, vy), (vx, vy),
                                        2.0 * via_r, &mut snaps,
                                    ) {
                                        break;
                                    }
                                    budget -= 1;
                                }
                                Some(_) => break,
                            }
                        }
                        if !site_ok {
                            while snaps.len() > site_mark {
                                let (jj, old) = snaps.pop().unwrap();
                                final_routes[jj] = old;
                            }
                            continue;
                        }
                        let idx =
                            geom::ClearanceIndex::build(board, final_routes, Some(net_id));
                        let Some(path) =
                            geom::route_escape(&idx, (px, py), (vx, vy), width, layer, net_id)
                        else {
                            while snaps.len() > site_mark {
                                let (jj, old) = snaps.pop().unwrap();
                                final_routes[jj] = old;
                            }
                            continue;
                        };
                        let route = &mut final_routes[i];
                        let seg_start = route.segments.len();
                        let via_start = route.vias.len();
                        for w in path.windows(2) {
                            if (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1) > 1e-9 {
                                route.segments.push(RouteSegment {
                                    layer,
                                    start: w[0],
                                    end: w[1],
                                    width_mm: width,
                                });
                            }
                        }
                        route.vias.push(RouteVia {
                            x: vx,
                            y: vy,
                            from_layer: 0,
                            to_layer: board.layer_stack.layers.len() - 1,
                        });
                        route.path_spans.push((seg_start, route.segments.len() - seg_start));
                        route.path_parents.push(None);
                        route.via_spans.push((via_start, 1));
                        info!(
                            "plane surface rescue: '{}' pad at ({px:.2},{py:.2}) got an exact routed drop at ({vx:.2},{vy:.2})",
                            board.nets[i].name
                        );
                        connected = true;
                        break 'site;
                    }
                }
            }
            // Stage 3 — MULTI-LAYER MAZE: dense pin fields can fence
            // both the surface paths AND every nearby via site; the
            // (x, y, layer) search wanders to wherever a via fits and
            // reaches same-net copper on ANY signal layer.
            if !connected {
                let via_r = board.layer_stack.via.pad_mm / 2.0;
                // Strict route_bias: rungs stay inside the net's layer mask.
                let signal_layers: Vec<usize> = board
                    .layer_stack
                    .signal_layer_indices()
                    .into_iter()
                    .filter(|l| {
                        board.nets[i]
                            .allowed_layers
                            .as_ref()
                            .map_or(true, |a| a.contains(l))
                    })
                    .collect();
                let r = &final_routes[i];
                use crate::routing::pathfinder::route_components;
                let comps3 = route_components(r);
                let mut targets: Vec<((f64, f64), usize, f64)> = r
                    .segments
                    .iter()
                    .enumerate()
                    .filter(|(si, sg)| {
                        Some(comps3[*si]) != pad_comp && signal_layers.contains(&sg.layer)
                    })
                    .map(|(_, sg)| {
                        let (dx, dy) = (sg.end.0 - sg.start.0, sg.end.1 - sg.start.1);
                        let l2n = dx * dx + dy * dy;
                        let t = if l2n <= 1e-12 {
                            0.0
                        } else {
                            (((px - sg.start.0) * dx + (py - sg.start.1) * dy) / l2n)
                                .clamp(0.0, 1.0)
                        };
                        let q = (sg.start.0 + t * dx, sg.start.1 + t * dy);
                        (q, sg.layer, (px - q.0).hypot(py - q.1))
                    })
                    .collect();
                targets.sort_by(|a, b| {
                    a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal)
                });
                targets.truncate(8);
                let idx = geom::ClearanceIndex::build(board, final_routes, Some(net_id));
                for &(q, ql, _) in &targets {
                    let Some(way) = geom::route_tunnel_ml(
                        &idx, (px, py), layer, q, ql, width, via_r, &signal_layers, net_id,
                        4.0,
                    )
                    .or_else(|| {
                        geom::route_tunnel_ml(
                            &idx, (px, py), layer, q, ql, width, via_r, &signal_layers,
                            net_id, 12.0,
                        )
                    }) else {
                        continue;
                    };
                    let hole_gap = board.layer_stack.via.drill_mm + 0.25;
                    let own_conflict = way.windows(2).any(|w| {
                        w[0].2 != w[1].2
                            && final_routes[i].vias.iter().any(|v| {
                                let d = (v.x - w[0].0).hypot(v.y - w[0].1);
                                d > 1e-6 && d < hole_gap
                            })
                    });
                    if own_conflict {
                        continue;
                    }
                    let route = &mut final_routes[i];
                    let seg_start = route.segments.len();
                    let via_start = route.vias.len();
                    let n_l = board.layer_stack.layers.len() - 1;
                    for w in way.windows(2) {
                        let (a, b) = (w[0], w[1]);
                        if a.2 == b.2 {
                            if (a.0 - b.0).hypot(a.1 - b.1) > 1e-9 {
                                route.segments.push(RouteSegment {
                                    layer: a.2,
                                    start: (a.0, a.1),
                                    end: (b.0, b.1),
                                    width_mm: width,
                                });
                            }
                        } else if !route
                            .vias
                            .iter()
                            .any(|v| (v.x - a.0).hypot(v.y - a.1) < 1e-6)
                        {
                            route.vias.push(RouteVia {
                                x: a.0,
                                y: a.1,
                                from_layer: 0,
                                to_layer: n_l,
                            });
                        }
                    }
                    let n_vias = route.vias.len() - via_start;
                    route.path_spans.push((seg_start, route.segments.len() - seg_start));
                    route.path_parents.push(None);
                    route.via_spans.push((via_start, n_vias));
                    info!(
                        "plane surface rescue: '{}' pad at ({px:.2},{py:.2}) joined by multi-layer maze ({n_vias} via(s))",
                        board.nets[i].name
                    );
                    connected = true;
                    break;
                }
            }
            if connected {
                rescued += 1;
            } else {
                debug!(
                    "surface rescue FAILED: '{}' pad ({px:.2},{py:.2}), {} attach candidate(s)",
                    board.nets[i].name,
                    attach.len()
                );
                if log::log_enabled!(log::Level::Debug) {
                    let idx = geom::ClearanceIndex::build(board, final_routes, Some(net_id));
                    for &(q, d) in attach.iter().take(4) {
                        let bl = geom::escape_blocker(&idx, (px, py), q, width, layer, net_id);
                        debug!(
                            "  attach ({:.2},{:.2}) d={d:.2}: blocker {:?}",
                            q.0, q.1, bl
                        );
                    }
                    let merged = output::kicad::merge_holes(output::kicad::plane_foreign_holes(
                        board, final_routes, net_id,
                    ));
                    for &(hx, hy, hr) in &merged {
                        if (hx - px).hypot(hy - py) < hr + 9.0 && hr > 1.0 {
                            debug!("  big merged hole: ({hx:.2},{hy:.2}) r={hr:.2}");
                        }
                    }
                }
                for (jj, old) in snaps.into_iter().rev() {
                    final_routes[jj] = old;
                }
            }
        }
    }
    rescued
}

/// Total unreached sinks over non-plane nets (the accept metric for
/// feedback-driven repair passes).
fn total_unreached(board: &Board, final_routes: &[Route]) -> usize {
    board
        .nets
        .iter()
        .enumerate()
        .filter(|(_, n)| n.plane_layer.is_none())
        .map(|(i, n)| pathfinder::unreached_sink_count(n, board, &final_routes[i]))
        .sum()
}

/// 5.99 PART NUDGE — placement-side relief with routing feedback.
/// A pad still unreached after the whole exact ladder is usually
/// walled in by a NEIGHBOR PART's pin field (via room is the binding
/// constraint on dense boards). Static placement halos measured
/// WORSE (basin lottery); this is the targeted version: move one
/// small free neighbor a step away from the stuck pad, rip only the
/// nets that touch it (plane pads lose their drop spans; the drop
/// pass re-sites them), re-route greedily under the exact commit
/// gate, re-run the ladder — and accept ONLY a strict drop in total
/// unreached sinks. Everything else reverts wholesale.
/// MIRROR CLASSES: suffix classes across sibling functional groups
/// (repeated entity instances). Each class = one FREE counterpart per
/// sibling, paired with its group's FIXED-member centroid anchor.
/// Families need >=2 siblings and every sibling needs >=1 fixed
/// member (the anchors must be solve-constant).
fn sibling_suffix_classes(board: &Board) -> Vec<Vec<(usize, (f64, f64))>> {
    use crate::det::HashMap;
    let comp_pos: HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();
    let mut families: HashMap<Vec<String>, Vec<(String, HashMap<String, usize>)>> =
        HashMap::default();
    for g in &board.groups {
        let prefix = format!("{}_", g.name);
        let mut by_suffix: HashMap<String, usize> = HashMap::default();
        let mut ok = true;
        for &mid in &g.members {
            let Some(&ci) = comp_pos.get(&mid) else { ok = false; break };
            let name = &board.components[ci].name;
            if name == &g.name {
                continue;
            }
            let Some(suf) = name.strip_prefix(&prefix) else { ok = false; break };
            if by_suffix.insert(suf.to_string(), ci).is_some() {
                ok = false;
                break;
            }
        }
        if !ok || by_suffix.len() < 2 {
            continue;
        }
        let mut key: Vec<String> = by_suffix.keys().cloned().collect();
        key.sort();
        families.entry(key).or_default().push((g.name.clone(), by_suffix));
    }
    let mut classes: Vec<Vec<(usize, (f64, f64))>> = Vec::new();
    for (key, mut sibs) in families {
        if sibs.len() < 2 {
            continue;
        }
        sibs.sort_by(|a, b| a.0.cmp(&b.0));
        // Per-sibling anchor from FIXED members only.
        let anchors: Vec<Option<(f64, f64)>> = sibs
            .iter()
            .map(|(_, m)| {
                let pts: Vec<(f64, f64)> = m
                    .values()
                    .filter(|&&ci| board.components[ci].placement.is_fixed())
                    .map(|&ci| (board.components[ci].x, board.components[ci].y))
                    .collect();
                if pts.is_empty() {
                    None
                } else {
                    let n = pts.len() as f64;
                    Some((
                        pts.iter().map(|p| p.0).sum::<f64>() / n,
                        pts.iter().map(|p| p.1).sum::<f64>() / n,
                    ))
                }
            })
            .collect();
        if anchors.iter().any(|a| a.is_none()) {
            continue;
        }
        for suf in &key {
            let mut class: Vec<(usize, (f64, f64))> = Vec::new();
            let mut all_free = true;
            for ((_, m), a) in sibs.iter().zip(&anchors) {
                let Some(&ci) = m.get(suf) else { all_free = false; break };
                if board.components[ci].placement.is_fixed() {
                    all_free = false; // fixed counterparts ARE the anchors
                    break;
                }
                class.push((ci, a.unwrap()));
            }
            if all_free && class.len() == sibs.len() {
                classes.push(class);
            }
        }
    }
    classes
}

/// CHANNEL MINI-BOARD SOLVE: extract ONE sibling channel as its own
/// board — the group's free members (Free), every fixed component
/// whose envelope intersects the column region (Fixed, e.g. the
/// pinned pots and jack), and a virtual boundary pad on the region
/// edge for each net that leaves the channel (IN/BUS/VCC/VBIAS/GND
/// see realistic exits) — then solve it with the FULL engine and
/// certify routability (connected sinks == pins, no DRC). The
/// certified placement is what gets stamped; a stacking heuristic
/// can be placement-legal yet unroutable (the strip-synthesis
/// lesson). Region-local coordinates; caller translates back.
/// NEGOTIATION: certification failures retry with wider exit-pad
/// separation and a fresh seed — moving the virtual boundary pads is
/// exactly the degree of freedom a hierarchical block owns.
/// Segments that WOULD be amputated if the branch containing
/// `seg_idx` were cut (the span-subtree closure the validator's
/// amputation performs) — its repair-cost estimate for choosing WHICH
/// of two offenders to rip. Routes without span structure rip whole:
/// cost = every segment.
fn amputation_cost(route: &Route, seg_idx: usize) -> usize {
    let Some((s, _)) = route
        .path_spans
        .iter()
        .copied()
        .find(|(ps, pl)| seg_idx >= *ps && seg_idx < *ps + *pl)
    else {
        return route.segments.len();
    };
    let root = route
        .path_spans
        .iter()
        .position(|&(ps, _)| ps == s)
        .unwrap_or(0);
    let n = route.path_spans.len();
    let mut doomed = vec![false; n];
    doomed[root] = true;
    loop {
        let mut grew = false;
        for i in 0..n {
            if !doomed[i] {
                if let Some(Some(pp)) = route.path_parents.get(i) {
                    if doomed[*pp] {
                        doomed[i] = true;
                        grew = true;
                    }
                }
            }
        }
        if !grew {
            break;
        }
    }
    (0..n)
        .filter(|&i| doomed[i])
        .map(|i| route.path_spans[i].1)
        .sum()
}

/// TRANSLATE-VERIFY (uniformity doctrine): the hand-routed demos ship
/// IDENTICAL channel strips; independent per-column solves are
/// correct but non-uniform. Try the reference column's certificate on
/// a sibling: apply anchor-translated placements, map each reference
/// net to its sibling (member-parallel pin order), translate the
/// copper, and verify — conflict-free against the parent board's pads
/// at EXACT widths (relations to the sibling's own members replicate
/// the certified reference frame; only the column's FIXED environment
/// differs) and pad-exact on every sibling member pin. Any failure
/// restores positions and returns None → the caller falls back to an
/// independent solve. Organic: geometry decides, per column.
fn try_channel_transfer(
    board: &mut Board,
    ref_members: &[usize],
    sib_members: &[usize],
    delta: (f64, f64),
    ref_routes: &[(NetId, Route)],
) -> Option<Vec<(NetId, Route)>> {
    let (tx, ty) = delta;
    let prior: Vec<(usize, f64, f64, f64)> = sib_members
        .iter()
        .map(|&ci| {
            let c = &board.components[ci];
            (ci, c.x, c.y, c.theta)
        })
        .collect();
    for (m, &sci) in sib_members.iter().enumerate() {
        let rci = ref_members[m];
        let (rx, ry, rt) = (
            board.components[rci].x,
            board.components[rci].y,
            board.components[rci].theta,
        );
        let c = &mut board.components[sci];
        c.x = rx + tx;
        c.y = ry + ty;
        c.theta = rt;
    }
    let restore = |board: &mut Board| {
        for &(ci, x, y, t) in &prior {
            let c = &mut board.components[ci];
            c.x = x;
            c.y = y;
            c.theta = t;
        }
    };
    let empty: Vec<Route> = board.nets.iter().map(|n| Route::empty(n.id)).collect();
    let net_pos: crate::det::HashMap<NetId, usize> = board
        .nets
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id, i))
        .collect();
    let via_r = board.layer_stack.via.pad_mm / 2.0;
    // Probe kernel = the MINI's world: members + FIXED environment.
    // Free non-participant parts sit at meaningless init positions
    // (the master section false-failed transfers); making them
    // invisible matches the solved-certificate semantics exactly —
    // a per-conflict free-pad filter measured 3 violations (unsound:
    // it also admitted real conflicts).
    let probe = {
        let mut b = board.clone();
        let keep: crate::det::HashSet<usize> = ref_members
            .iter()
            .chain(sib_members.iter())
            .copied()
            .collect();
        for (ci, c) in b.components.iter_mut().enumerate() {
            if !keep.contains(&ci) && !c.placement.is_fixed() {
                c.pins.clear();
            }
        }
        b
    };
    let cidx = geom::ClearanceIndex::build(&probe, &empty, None);
    let sib_ids: Vec<ComponentId> =
        sib_members.iter().map(|&ci| board.components[ci].id).collect();
    let mut out: Vec<(NetId, Route)> = Vec::new();
    for (nid, route) in ref_routes {
        let Some(&ri) = net_pos.get(nid) else {
            restore(board);
            return None;
        };
        let mut mapped: Option<usize> = None;
        for &(cid, pid) in &board.nets[ri].pins {
            let Some(m) = ref_members
                .iter()
                .position(|&mi| board.components[mi].id == cid)
            else {
                continue; // pin on a fixed/foreign part
            };
            let refc = &board.components[ref_members[m]];
            let Some(j) = refc.pins.iter().position(|p| p.pin_id == pid) else {
                restore(board);
                return None;
            };
            let sib = &board.components[sib_members[m]];
            let Some(snid) = sib.pins.get(j).and_then(|p| p.net) else {
                restore(board);
                return None;
            };
            let Some(&si) = net_pos.get(&snid) else {
                restore(board);
                return None;
            };
            if mapped.map_or(false, |mm| mm != si) {
                restore(board);
                return None;
            }
            mapped = Some(si);
        }
        let Some(si) = mapped else { continue }; // pure-fixed net: no transfer
        let snet_id = board.nets[si].id;
        let mut t = route.clone();
        t.net_id = snet_id;
        for sg in t.segments.iter_mut() {
            sg.start.0 += tx;
            sg.start.1 += ty;
            sg.end.0 += tx;
            sg.end.1 += ty;
        }
        for v in t.vias.iter_mut() {
            v.x += tx;
            v.y += ty;
        }
        // DIFFERENTIAL verification: the kernel probe is stricter
        // than the oracle's margins, and the certificate already
        // proved its near-margin relations acceptable (measured: a
        // congruent foreign-jack pad flagged on every transfer). A
        // conflict is only DISQUALIFYING when the congruent query in
        // the REFERENCE frame is clean — i.e. the translation created
        // a NEW relation. Reference-replicated relations pass; the
        // stamp-time revalidation, validator, and oracle still gate
        // the final copper.
        for (sg, rsg) in t.segments.iter().zip(route.segments.iter()) {
            if cidx
                .first_conflict(sg.start, sg.end, sg.width_mm, sg.layer, snet_id)
                .is_some()
            {
                let ref_hit = cidx
                    .first_conflict(
                        rsg.start,
                        rsg.end,
                        rsg.width_mm,
                        rsg.layer,
                        *nid,
                    )
                    .is_some();
                if !ref_hit {
                    debug!(
                        "  channel transfer: '{}' seg ({:.1},{:.1})-({:.1},{:.1}) NEW conflict — fallback",
                        board.nets[si].name,
                        sg.start.0,
                        sg.start.1,
                        sg.end.0,
                        sg.end.1
                    );
                    restore(board);
                    return None;
                }
            }
        }
        for (v, rv) in t.vias.iter().zip(route.vias.iter()) {
            if cidx.via_conflict(v.x, v.y, via_r, snet_id).is_some()
                && cidx.via_conflict(rv.x, rv.y, via_r, *nid).is_none()
            {
                restore(board);
                return None;
            }
        }
        // Pad-exact on the sibling's own member pins of this net.
        let touched_all = board.nets[si].pins.iter().all(|&(cid, pid)| {
            if !sib_ids.contains(&cid) {
                return true; // outside pins: the stamping revalidation gates them
            }
            let comp = board
                .components
                .iter()
                .find(|c| c.id == cid)
                .expect("sibling member");
            let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pid) else {
                return false;
            };
            let (co, sn) = (comp.theta.cos(), comp.theta.sin());
            let px = comp.x + pin.dx * co - pin.dy * sn;
            let py = comp.y + pin.dx * sn + pin.dy * co;
            let half = pin
                .pad
                .as_ref()
                .map(|p| p.width_mm.min(p.height_mm) / 2.0)
                .unwrap_or(0.4);
            t.segments.iter().any(|sg| {
                geom::point_segment_dist((px, py), sg.start, sg.end)
                    < sg.width_mm / 2.0 + half - 0.001
            })
        });
        if !touched_all {
            debug!(
                "  channel transfer: '{}' translated copper misses a member pad",
                board.nets[si].name
            );
            restore(board);
            return None;
        }
        out.push((snet_id, t));
    }
    Some(out)
}

/// A certified channel solve, in BOARD coordinates: the reference
/// members' placements AND the certified copper. Parent NetIds are
/// preserved (mini nets are clones of parent nets).
struct CertifiedChannel {
    placements: Vec<(usize, f64, f64, f64)>,
    routes: Vec<(NetId, Route)>,
}

fn solve_channel_miniboard(
    board: &Board,
    free_members: &[usize],
    region: (f64, f64, f64, f64),
    config: &PnrConfig,
    seed: u64,
) -> Option<CertifiedChannel> {
    for (attempt, pad_sep) in [3.0f64, 5.0, 8.0].iter().enumerate() {
        let got = solve_channel_miniboard_attempt(
            board,
            free_members,
            region,
            config,
            seed.wrapping_add(attempt as u64),
            *pad_sep,
        );
        if got.is_some() {
            if attempt > 0 {
                info!("channel mini-solve: certified on negotiation attempt {}", attempt + 1);
            }
            return got;
        }
    }
    None
}

fn solve_channel_miniboard_attempt(
    board: &Board,
    free_members: &[usize],
    region: (f64, f64, f64, f64), // x0, y0, w, h
    config: &PnrConfig,
    seed: u64,
    pad_sep: f64,
) -> Option<CertifiedChannel> {
    use crate::det::{HashMap, HashSet};
    let (rx0, ry0, rw, rh) = region;
    let member_set: HashSet<usize> = free_members.iter().copied().collect();
    let mut comps: Vec<Component> = Vec::new();
    let mut included: HashSet<ComponentId> = HashSet::default();
    // Free members, region-local.
    for &ci in free_members {
        let mut c = board.components[ci].clone();
        c.x -= rx0;
        c.y -= ry0;
        c.placement = PlacementConstraint::Free;
        c.group = None;
        included.insert(c.id);
        comps.push(c);
    }
    // Fixed parts intersecting the region.
    for (ci, c) in board.components.iter().enumerate() {
        if member_set.contains(&ci) || !c.placement.is_fixed() {
            continue;
        }
        let (cx, cy, hw, hh) = c.envelope();
        if cx + hw > rx0 && cx - hw < rx0 + rw && cy + hh > ry0 && cy - hh < ry0 + rh {
            let mut m = c.clone();
            m.x -= rx0;
            m.y -= ry0;
            m.placement = PlacementConstraint::Fixed { x: m.x, y: m.y, theta: m.theta };
            m.group = None;
            included.insert(m.id);
            comps.push(m);
        }
    }
    // Nets: keep pins on included comps; nets that also leave the
    // channel get a boundary pad on the nearest vertical region edge.
    // KEY COLLISION GUARD: a fresh SlotMap mints (index 0, version 1)
    // — IDENTICAL to the original board's first-inserted keys. A
    // boundary pad's ComponentId collided with a real component
    // (measured: '__edge_GND' resolved as a pin of auto_J_CH1_1 —
    // the invariant 65/70 residual + the Spacing were ID corruption,
    // not geometry). Pre-fill the mints past the existing key space.
    let mut key_mint: slotmap::SlotMap<ComponentId, ()> = slotmap::SlotMap::with_key();
    for _ in 0..board.components.len() + 64 {
        let _ = key_mint.insert(());
    }
    let mut pin_mint: slotmap::SlotMap<PinId, ()> = slotmap::SlotMap::with_key();
    let total_board_pins: usize = board.components.iter().map(|c| c.pins.len()).sum();
    for _ in 0..total_board_pins + 64 {
        let _ = pin_mint.insert(());
    }
    let pos_of: HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();
    // STRADDLER PADS: a fixed part intersecting the region edge keeps
    // pads OUTSIDE the mini outline — the router can never reach them
    // (measured: J_CH1 GND pads at local x=-6.3..-1.3 counted
    // mandatory and missed in EVERY attempt). Such pads are external:
    // stripped from the mini netlist, and their net exits through a
    // boundary pad projected from the actual crossing point.
    let mut outside_pins: HashSet<(ComponentId, PinId)> = HashSet::default();
    let mut outside_pos: HashMap<(ComponentId, PinId), (f64, f64)> = HashMap::default();
    for c in comps.iter() {
        if !c.placement.is_fixed() {
            continue;
        }
        let (co, sn) = (c.theta.cos(), c.theta.sin());
        for pin in &c.pins {
            let px = c.x + pin.dx * co - pin.dy * sn;
            let py = c.y + pin.dx * sn + pin.dy * co;
            if px < 0.0 || px > rw || py < 0.0 || py > rh {
                outside_pins.insert((c.id, pin.pin_id));
                outside_pos.insert((c.id, pin.pin_id), (px + rx0, py + ry0));
            }
        }
    }
    // A net belongs to the CHANNEL only if it touches a FREE member.
    // Foreign fixed parts inside the region (a neighbor channel's
    // jack living in this column band) are OBSTACLES, not netlist —
    // their nets dragged 2-sink unroutables into every attempt.
    let free_ids_set: crate::det::HashSet<ComponentId> =
        free_members.iter().map(|&ci| board.components[ci].id).collect();
    let eligible: crate::det::HashSet<NetId> = board
        .nets
        .iter()
        .filter(|n| n.pins.iter().any(|(cid, _)| free_ids_set.contains(cid)))
        .map(|n| n.id)
        .collect();
    for c in comps.iter_mut() {
        if c.placement.is_fixed() {
            for pin in c.pins.iter_mut() {
                if pin.net.map_or(false, |nid| !eligible.contains(&nid))
                    || outside_pins.contains(&(c.id, pin.pin_id))
                {
                    pin.net = None; // obstacle copper, no net demand
                }
            }
        }
    }
    let mut optional_pads: crate::det::HashSet<ComponentId> =
        crate::det::HashSet::default();
    let mut nets: Vec<PnrNet> = Vec::new();
    // BOUNDARY-PAD NEGOTIATION pass 1: each leaving net's exit pad
    // goes where its OUTSIDE-pin centroid projects onto the nearer
    // vertical edge (v1's even spacing put exits nowhere near where
    // nets wanted to leave — certification refused 63/91). Pads on
    // one edge are then collision-swept to `pad_sep` spacing; the
    // caller widens pad_sep across negotiation attempts.
    struct Leaving {
        net: NetId,
        left: bool,
        proj: f64,
        slot: f64,
        /// Rail exits beyond the first are OPTIONS the tree may use,
        /// never obligations — counting them as sinks failed
        /// certification on unused exits.
        optional: bool,
    }
    let mut leaving: Vec<Leaving> = Vec::new();
    for net in &board.nets {
        if !eligible.contains(&net.id) {
            continue;
        }
        let inside_pins: Vec<usize> = net
            .pins
            .iter()
            .filter(|(cid, pid)| {
                included.contains(cid) && !outside_pins.contains(&(*cid, *pid))
            })
            .filter_map(|(cid, _)| pos_of.get(cid).copied())
            .collect();
        if inside_pins.is_empty() || net.pins.len() == inside_pins.len() {
            continue;
        }
        let (mut ox, mut oy, mut on) = (0.0f64, 0.0f64, 0usize);
        for &(cid, pid) in &net.pins {
            if included.contains(&cid) && !outside_pins.contains(&(cid, pid)) {
                continue;
            }
            // Straddler pads know their exact crossing geometry —
            // better exit projection than the owning part's centroid.
            if let Some(&(px, py)) = outside_pos.get(&(cid, pid)) {
                ox += px;
                oy += py;
                on += 1;
            } else if let Some(&pi) = pos_of.get(&cid) {
                ox += board.components[pi].x;
                oy += board.components[pi].y;
                on += 1;
            }
        }
        if on == 0 {
            continue;
        }
        let (ox, oy) = (ox / on as f64, oy / on as f64);
        // RAIL EXITS: a board-crossing rail leaves a real block
        // everywhere along its edge — funneling a whole GND tree
        // through ONE exit was the last certification blocker
        // (65/71, attempt-invariant). Rails get k exits at the
        // y-quantiles of their INSIDE pins, alternating edges;
        // signals keep the single projected exit.
        let is_rail = matches!(
            net.net_class,
            PnrNetClass::Power { .. } | PnrNetClass::Ground
        );
        if is_rail && inside_pins.len() >= 3 {
            let k = ((inside_pins.len() as f64 / 3.0).ceil() as usize).clamp(2, 4);
            let mut ys: Vec<f64> =
                inside_pins.iter().map(|&pi| board.components[pi].y).collect();
            ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
            for j in 0..k {
                let q = (j as f64 + 0.5) / k as f64;
                let y = ys[((q * ys.len() as f64) as usize).min(ys.len() - 1)];
                leaving.push(Leaving {
                    net: net.id,
                    left: j % 2 == 0,
                    proj: (y - ry0).clamp(2.0, rh - 2.0),
                    slot: 0.0,
                    optional: j > 0,
                });
            }
        } else {
            leaving.push(Leaving {
                net: net.id,
                left: ox < rx0 + rw / 2.0,
                proj: (oy - ry0).clamp(2.0, rh - 2.0),
                slot: 0.0,
                optional: false,
            });
        }
    }
    // Collision sweep per edge (deterministic: sort by projection,
    // stable on insertion order). Fixed parts hugging an edge BLOCK
    // their y-interval: a straddling jack sits ON the exit rail, and
    // an exit pad dropped at its row was the last hard Spacing
    // (measured: J3 at y=27.8, exit slot at (rw-2, 27.8)).
    let edge_block = |is_left: bool| -> Vec<(f64, f64)> {
        let mut iv: Vec<(f64, f64)> = Vec::new();
        for c in comps.iter() {
            if !c.placement.is_fixed() {
                continue;
            }
            let (cx, cy, hw, hh) = c.envelope();
            let near = if is_left { cx - hw < 4.0 } else { cx + hw > rw - 4.0 };
            if near {
                iv.push((cy - hh - 1.5, cy + hh + 1.5));
            }
        }
        iv.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        iv
    };
    for is_left in [true, false] {
        let blocked = edge_block(is_left);
        let mut idxs: Vec<usize> = (0..leaving.len())
            .filter(|&i| leaving[i].left == is_left)
            .collect();
        idxs.sort_by(|&a, &b| {
            leaving[a]
                .proj
                .partial_cmp(&leaving[b].proj)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut cursor = 2.0f64;
        for &i in &idxs {
            let mut want = leaving[i].proj.max(cursor);
            for &(b0, b1) in &blocked {
                if want > b0 && want < b1 {
                    want = b1;
                }
            }
            let slot = want.min(rh - 2.0);
            leaving[i].slot = slot;
            cursor = slot + pad_sep;
        }
        // BACKWARD RELAX: when the forward cursor runs past the rail
        // end, every remaining exit clamps onto rh-2 — the SAME point
        // (measured: __edge_ch4_ni and __edge_ch4_gleg coincident at
        // y=108, a self-inflicted Spacing that failed ch4's
        // certification on every attempt). Sweep back from the top
        // pushing earlier pads down to restore pad_sep.
        let mut cursor = rh - 2.0;
        for &i in idxs.iter().rev() {
            let mut slot = leaving[i].slot.min(cursor);
            for &(b0, b1) in blocked.iter().rev() {
                if slot > b0 && slot < b1 {
                    slot = b0;
                }
            }
            leaving[i].slot = slot.max(2.0);
            cursor = leaving[i].slot - pad_sep;
        }
    }
    for net in &board.nets {
        if !eligible.contains(&net.id) {
            continue;
        }
        let inside: Vec<(ComponentId, PinId)> = net
            .pins
            .iter()
            .copied()
            .filter(|(cid, pid)| {
                included.contains(cid) && !outside_pins.contains(&(*cid, *pid))
            })
            .collect();
        if inside.is_empty() {
            continue;
        }
        let leaves = net.pins.len() > inside.len();
        if inside.len() < 2 && !leaves {
            continue;
        }
        let mut n = net.clone();
        n.pins = inside;
        n.plane_layer = None; // no pours in the mini-solve
        n.plane_region = None;
        n.allowed_layers = None; // strict mask rebuilt from mini config if needed
        for l in leaving.iter().filter(|l| l.net == net.id) {
            let bx = if l.left { 2.0 } else { rw - 2.0 };
            let cid = key_mint.insert(());
            let pid = pin_mint.insert(());
            if l.optional {
                optional_pads.insert(cid);
            }
            let by = l.slot;
            comps.push(Component {
                id: cid,
                name: format!("__edge_{}", n.name),
                refdes: String::new(),
                width_mm: 1.7,
                height_mm: 1.7,
                bbox_dx: 0.0,
                bbox_dy: 0.0,
                pins: vec![PinPosition {
                    pin_id: pid,
                    name: "1".into(),
                    dx: 0.0,
                    dy: 0.0,
                    net: Some(n.id),
                    pad: Some(PadGeom {
                        width_mm: 1.7,
                        height_mm: 1.7,
                        shape: PadShapeKind::Circle,
                        drill_slot_mm: None,
                        drill_mm: Some(1.0),
                    }),
                    unplaced: false,
                }],
                side: BoardSide::Top,
                group: None,
                thermal_power_w: 0.0,
                solved_current_a: None,
                package: "__edge".into(),
                placement: PlacementConstraint::Fixed { x: bx, y: by, theta: 0.0 },
                x: bx,
                y: by,
                theta: 0.0,
                density_inflation: 1.0,
                layout_intents: Vec::new(),
            });
            included.insert(cid);
            n.pins.push((cid, pid));
        }
        if n.pins.len() >= 2 {
            nets.push(n);
        }
    }
    let mut mini_cfg = config.clone();
    mini_cfg.board.outline = BoardOutline::Rectangle { width_mm: rw, height_mm: rh };
    mini_cfg.board.fixed_placements = Vec::new();
    mini_cfg.board.mounting_holes = Vec::new();
    mini_cfg.board.keepout_zones = Vec::new();
    mini_cfg.board.cutouts = Vec::new();
    mini_cfg.board.placement_regions = Vec::new();
    // Small board, cheap iterations — a bigger budget beats an early
    // stall (the mini is the certification oracle; spend here).
    mini_cfg.max_iterations = mini_cfg.max_iterations.min(700);
    let mini = Board {
        config: mini_cfg.board.clone(),
        layer_stack: board.layer_stack.clone(),
        components: comps,
        nets,
        groups: Vec::new(),
        placement_recipes: Default::default(),
        constraints: Vec::new(),
        ddr_bin: None,
    };
    let free_ids: Vec<ComponentId> =
        free_members.iter().map(|&ci| board.components[ci].id).collect();
    let result = place_and_route(mini, mini_cfg, seed).ok()?;
    // CERTIFICATION, pad-exact: every MANDATORY pad geometrically
    // touched by its net's copper (optional rail exits skipped), and
    // no DRC beyond UnroutedNet entries attributable to those
    // optional exits.
    let rcomp: crate::det::HashMap<ComponentId, usize> = result
        .board
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();
    let mut missed = 0usize;
    let mut mandatory = 0usize;
    for (ni, n) in result.board.nets.iter().enumerate() {
        if n.pins.len() < 2 {
            continue;
        }
        let route = result.routes.get(ni);
        for &(cid, pid) in &n.pins {
            if optional_pads.contains(&cid) {
                continue;
            }
            mandatory += 1;
            let Some(&ci) = rcomp.get(&cid) else { missed += 1; continue };
            let comp = &result.board.components[ci];
            let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pid) else {
                missed += 1;
                continue;
            };
            let (c, sn) = (comp.theta.cos(), comp.theta.sin());
            let px = comp.x + pin.dx * c - pin.dy * sn;
            let py = comp.y + pin.dx * sn + pin.dy * c;
            let half = pin
                .pad
                .as_ref()
                .map(|p| p.width_mm.min(p.height_mm) / 2.0)
                .unwrap_or(0.4);
            let touched = route.map_or(false, |r| {
                r.segments.iter().any(|sg| {
                    geom::point_segment_dist((px, py), sg.start, sg.end)
                        < sg.width_mm / 2.0 + half - 0.001
                })
            });
            if !touched {
                missed += 1;
                debug!(
                    "  mini-miss: '{}' pad '{}' at ({px:.1},{py:.1})",
                    n.name, comp.name
                );
            }
        }
    }
    let hard_drc = result
        .drc_violations
        .iter()
        .filter(|v| !matches!(v.kind, DrcViolationKind::UnroutedNet))
        .count();
    if hard_drc > 0 && std::env::var("BHDL_PNR_PROBE").is_ok() {
        for v in result
            .drc_violations
            .iter()
            .filter(|v| !matches!(v.kind, DrcViolationKind::UnroutedNet))
            .take(8)
        {
            log::info!("[probe] mini-solve hard drc: {v:?}");
        }
    }
    let certified = missed == 0 && hard_drc == 0;
    info!(
        "channel mini-solve: {}/{} mandatory pads, {} hard drc — {}",
        mandatory - missed,
        mandatory,
        hard_drc,
        if certified { "CERTIFIED" } else { "not routable" }
    );
    if !certified {
        for (i, n) in result.board.nets.iter().enumerate() {
            if n.pins.len() < 2 {
                continue;
            }
            let unreached = pathfinder::unreached_sink_count(
                n,
                &result.board,
                result.routes.get(i).unwrap_or(&Route::empty(n.id)),
            );
            if unreached > 0 {
                debug!("  mini-solve unreached: '{}' {} sink(s)", n.name, unreached);
            }
        }
        for v in result.drc_violations.iter().take(3) {
            debug!(
                "  mini-solve drc: {:?} at ({:.2},{:.2}) — {}",
                v.kind, v.location.0, v.location.1, v.description
            );
        }
    }
    if !certified {
        return None;
    }
    let id_pos: HashMap<ComponentId, usize> = result
        .board
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();
    // Certified copper, translated to board coords. Boundary-pad
    // stubs stay in (they're real copper of LEAVING nets — those
    // nets fail the caller's all-pins-touched revalidation and route
    // normally at top level; interior nets never had exit pads).
    let mut routes: Vec<(NetId, Route)> = Vec::new();
    for (i, n) in result.board.nets.iter().enumerate() {
        let Some(r) = result.routes.get(i) else { continue };
        if r.is_empty() {
            continue;
        }
        let mut t = r.clone();
        for sg in t.segments.iter_mut() {
            sg.start.0 += rx0;
            sg.start.1 += ry0;
            sg.end.0 += rx0;
            sg.end.1 += ry0;
        }
        for v in t.vias.iter_mut() {
            v.x += rx0;
            v.y += ry0;
        }
        routes.push((n.id, t));
    }
    Some(CertifiedChannel {
        placements: free_members
            .iter()
            .zip(&free_ids)
            .filter_map(|(&ci, id)| {
                let &mi = id_pos.get(id)?;
                let m = &result.board.components[mi];
                Some((ci, m.x + rx0, m.y + ry0, m.theta))
            })
            .collect(),
        routes,
    })
}

/// CHANNEL STAMPING: find families of sibling functional groups
/// (identical member-suffix multisets — repeated entity instances),
/// pick the lexicographically-first group as the reference, and move
/// every FREE sibling member to the reference counterpart's position
/// plus the sibling's translation (derived from FIXED counterpart
/// pairs — the pinned pots/jacks of the mechanical contract; groups
/// with no fixed members use member centroids). Returns parts moved.
fn stamp_sibling_groups(board: &mut Board) -> usize {
    use crate::det::HashMap;
    let comp_pos: HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();
    // Group -> (suffix -> component index), keyed by suffix multiset.
    let mut families: HashMap<Vec<String>, Vec<(String, HashMap<String, usize>)>> =
        HashMap::default();
    for g in &board.groups {
        let prefix = format!("{}_", g.name);
        let mut by_suffix: HashMap<String, usize> = HashMap::default();
        let mut ok = true;
        for &mid in &g.members {
            let Some(&ci) = comp_pos.get(&mid) else { ok = false; break };
            let name = &board.components[ci].name;
            if name == &g.name {
                continue; // the parent pseudo-component itself
            }
            let Some(suf) = name.strip_prefix(&prefix) else { ok = false; break };
            if by_suffix.insert(suf.to_string(), ci).is_some() {
                ok = false; // duplicate suffix — ambiguous
                break;
            }
        }
        if !ok || by_suffix.len() < 2 {
            continue;
        }
        let mut key: Vec<String> = by_suffix.keys().cloned().collect();
        key.sort();
        families.entry(key).or_default().push((g.name.clone(), by_suffix));
    }
    let mut moved = 0usize;
    for (_, mut sibs) in families {
        if sibs.len() < 2 {
            continue;
        }
        sibs.sort_by(|a, b| a.0.cmp(&b.0));
        let (ref_name, ref_map) = sibs[0].clone();
        // Snapshot positions/fixedness BEFORE mutating.
        let pos: Vec<(f64, f64, f64, BoardSide, bool)> = board
            .components
            .iter()
            .map(|c| (c.x, c.y, c.theta, c.side, c.placement.is_fixed()))
            .collect();
        // Reference anchor = centroid of its FIXED members (fall back
        // to all members).
        let anchor = |m: &HashMap<String, usize>,
                      fixed_only: bool,
                      pos: &Vec<(f64, f64, f64, BoardSide, bool)>|
         -> Option<(f64, f64)> {
            let pts: Vec<(f64, f64)> = m
                .values()
                .filter(|&&ci| !fixed_only || pos[ci].4)
                .map(|&ci| (pos[ci].0, pos[ci].1))
                .collect();
            if pts.is_empty() {
                return None;
            }
            let n = pts.len() as f64;
            Some((
                pts.iter().map(|p| p.0).sum::<f64>() / n,
                pts.iter().map(|p| p.1).sum::<f64>() / n,
            ))
        };
        let use_fixed = ref_map.values().any(|&ci| pos[ci].4);
        let Some(ra) = anchor(&ref_map, use_fixed, &pos) else { continue };
        for (sib_name, sib_map) in &sibs[1..] {
            let Some(sa) = anchor(sib_map, use_fixed, &pos) else { continue };
            let (tx, ty) = (sa.0 - ra.0, sa.1 - ra.1);
            for (suf, &sci) in sib_map {
                if pos[sci].4 {
                    continue; // the mechanical contract wins
                }
                let Some(&rci) = ref_map.get(suf) else { continue };
                let (nx, ny) = (pos[rci].0 + tx, pos[rci].1 + ty);
                let (theta, side) = (pos[rci].2, pos[rci].3);
                let c = &mut board.components[sci];
                if (c.x - nx).hypot(c.y - ny) > 1e-9
                    || (c.theta - theta).abs() > 1e-9
                {
                    c.x = nx;
                    c.y = ny;
                    c.theta = theta;
                    c.side = side;
                    moved += 1;
                }
            }
            log::debug!(
                "channel stamping: '{sib_name}' aligned to '{ref_name}' (t = {tx:.2},{ty:.2})"
            );
        }
    }
    moved
}

fn part_nudge_pass(board: &mut Board, final_routes: &mut Vec<Route>) -> usize {
    let mut before = total_unreached(board, final_routes);
    if before == 0 {
        return 0;
    }
    let start = before;
    // Stuck pads: same detection as the escape ladder's target scan.
    let comp_idx: crate::det::HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(k, c)| (c.id, k))
        .collect();
    let mut stuck: Vec<(usize, (f64, f64))> = Vec::new();
    for (ni, net) in board.nets.iter().enumerate() {
        if net.plane_layer.is_some() || stuck.len() >= 4 {
            continue;
        }
        if pathfinder::unreached_sink_count(net, board, &final_routes[ni]) == 0 {
            continue;
        }
        use crate::routing::pathfinder::route_components;
        let route = &final_routes[ni];
        if route.is_empty() {
            continue;
        }
        let comps = route_components(route);
        let tree = {
            let mut pop: crate::det::HashMap<usize, usize> =
                crate::det::HashMap::default();
            for &c in &comps {
                *pop.entry(c).or_insert(0) += 1;
            }
            pop.into_iter()
                .max_by_key(|&(c, n)| (n, std::cmp::Reverse(c)))
                .map(|(c, _)| c)
        };
        for &(cid, pid) in &net.pins {
            let Some(&ci) = comp_idx.get(&cid) else { continue };
            let comp = &board.components[ci];
            let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pid) else { continue };
            if pin.unplaced {
                continue;
            }
            let cos_t = comp.theta.cos();
            let sin_t = comp.theta.sin();
            let px = comp.x + pin.dx * cos_t - pin.dy * sin_t;
            let py = comp.y + pin.dx * sin_t + pin.dy * cos_t;
            let half = pin
                .pad
                .as_ref()
                .map(|p| p.width_mm.min(p.height_mm) / 2.0)
                .unwrap_or(0.25);
            let pin_layer = match comp.side {
                BoardSide::Top => 0,
                BoardSide::Bottom => board.layer_stack.layers.len() - 1,
            };
            let thru = pin.pad.as_ref().map(|p| p.drill_mm.is_some()).unwrap_or(false);
            let touched = route.segments.iter().enumerate().any(|(si, sg)| {
                Some(comps[si]) == tree
                    && (thru || sg.layer == pin_layer)
                    && geom::point_segment_dist((px, py), sg.start, sg.end)
                        < sg.width_mm / 2.0 + half - 0.001
            });
            if !touched {
                stuck.push((ni, (px, py)));
                break;
            }
        }
    }
    let mut gained = 0usize;
    for (ni, (px, py)) in stuck {
        // Small, free neighbors nearest the stuck pad.
        let mut cands: Vec<(usize, f64)> = board
            .components
            .iter()
            .enumerate()
            .filter(|(k, c)| {
                c.placement.is_free()
                    && c.pins.len() <= 10
                    && !c.pins.iter().any(|p| {
                        p.net == Some(board.nets[ni].id)
                            && {
                                let cos_t = c.theta.cos();
                                let sin_t = c.theta.sin();
                                let gx = c.x + p.dx * cos_t - p.dy * sin_t;
                                let gy = c.y + p.dx * sin_t + p.dy * cos_t;
                                (gx - px).hypot(gy - py) < 1e-6
                            }
                    })
                    && {
                        let (cx, cy, hw, hh) = board.components[*k].envelope();
                        let nx = px.clamp(cx - hw, cx + hw);
                        let ny = py.clamp(cy - hh, cy + hh);
                        (px - nx).hypot(py - ny) < 3.0
                    }
            })
            .map(|(k, c)| (k, (c.x - px).hypot(c.y - py)))
            .collect();
        cands.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        cands.truncate(2);
        // SELF-NUDGE: when the fence is the stuck pad's own
        // neighborhood of PADS (a crystal against a TQFP pin row), no
        // neighbor move opens the corridor — move the stuck component
        // itself. Tried AFTER neighbor moves so existing outcomes are
        // preserved; the identical rip/rebuild/strict-win trial
        // machinery polices the move. (Landed only after drop siting
        // moved onto the exact kernel — the first landing shipped
        // vias through the PadObs snapshot's blind spots.)
        // Trial = (dx, dy, dtheta): neighbors translate away; the
        // self candidate also tries quarter ROTATIONS in place (a
        // crystal's stuck pad often needs to FACE the TQFP row —
        // translation alone cannot reorient it).
        let mut cand_dirs: Vec<(usize, Vec<(f64, f64, f64)>)> = cands
            .iter()
            .map(|&(k, _)| {
                let c = &board.components[k];
                let (dx, dy) = (c.x - px, c.y - py);
                let l = dx.hypot(dy).max(1e-6);
                (k, vec![(dx / l, dy / l, 0.0)])
            })
            .collect();
        if let Some(sk) = board.components.iter().position(|c| {
            c.placement.is_free()
                && c.pins.len() <= 10
                && c.pins.iter().any(|p| {
                    let cos_t = c.theta.cos();
                    let sin_t = c.theta.sin();
                    let gx = c.x + p.dx * cos_t - p.dy * sin_t;
                    let gy = c.y + p.dx * sin_t + p.dy * cos_t;
                    (gx - px).hypot(gy - py) < 1e-6
                })
        }) {
            cand_dirs.push((
                sk,
                vec![
                    (1.0, 0.0, 0.0),
                    (-1.0, 0.0, 0.0),
                    (0.0, 1.0, 0.0),
                    (0.0, -1.0, 0.0),
                    (0.0, 0.0, std::f64::consts::FRAC_PI_2),
                    (0.0, 0.0, -std::f64::consts::FRAC_PI_2),
                ],
            ));
        }
        'cand: for (k, dirs) in cand_dirs {
            for away in dirs {
            for step in [0.6f64, 1.2] {
                // A rotation trial is position-invariant — one step
                // suffices.
                if away.2 != 0.0 && step > 0.6 {
                    continue;
                }
                let (ox, oy) = (board.components[k].x, board.components[k].y);
                let otheta = board.components[k].theta;
                let (nx, ny) = (ox + away.0 * step, oy + away.1 * step);
                if away.2 != 0.0 {
                    board.components[k].theta = otheta + away.2;
                }
                if !crate::legalization::position_legal(board, k, nx, ny) {
                    board.components[k].theta = otheta;
                    continue;
                }
                // position_legal checks component ENVELOPES only — the
                // moved part's pads can land ON another net's existing
                // copper (seed-13: a nudged 1206's GND pad shorted
                // SCK's tracks, 10 shorting_items). Check every pad at
                // the trial position against foreign segments/vias.
                {
                    let c = &board.components[k];
                    let cos_t = c.theta.cos();
                    let sin_t = c.theta.sin();
                    let quarter = ((c.theta / std::f64::consts::FRAC_PI_2).round() as i64)
                        .rem_euclid(2);
                    let clearance = board.config.min_spacing_mm;
                    let via_r = board.layer_stack.via.pad_mm / 2.0;
                    let mut pad_hits_copper = false;
                    'pads: for pin in &c.pins {
                        if pin.unplaced {
                            continue;
                        }
                        let gx = nx + pin.dx * cos_t - pin.dy * sin_t;
                        let gy = ny + pin.dx * sin_t + pin.dy * cos_t;
                        let (pw, ph) = match &pin.pad {
                            Some(p) => (p.width_mm, p.height_mm),
                            None => (0.5, 0.5),
                        };
                        let (pw, ph) = if quarter == 1 { (ph, pw) } else { (pw, ph) };
                        let (hx, hy) = (pw / 2.0, ph / 2.0);
                        let thru =
                            pin.pad.as_ref().map(|p| p.drill_mm.is_some()).unwrap_or(false);
                        let pad_layer = match c.side {
                            BoardSide::Top => 0,
                            BoardSide::Bottom => board.layer_stack.layers.len() - 1,
                        };
                        for (rj, r) in final_routes.iter().enumerate() {
                            if pin.net == Some(board.nets[rj].id) {
                                continue;
                            }
                            for sg in &r.segments {
                                if !thru && sg.layer != pad_layer {
                                    continue;
                                }
                                if geom::segment_rect_dist(
                                    sg.start,
                                    sg.end,
                                    gx - hx,
                                    gy - hy,
                                    gx + hx,
                                    gy + hy,
                                ) < sg.width_mm / 2.0 + clearance - 1e-6
                                {
                                    pad_hits_copper = true;
                                    break 'pads;
                                }
                            }
                            for v in &r.vias {
                                let cx = v.x.clamp(gx - hx, gx + hx);
                                let cy = v.y.clamp(gy - hy, gy + hy);
                                if (v.x - cx).hypot(v.y - cy)
                                    < via_r + clearance - 1e-6
                                {
                                    pad_hits_copper = true;
                                    break 'pads;
                                }
                            }
                        }
                    }
                    if pad_hits_copper {
                        board.components[k].theta = otheta;
                        continue;
                    }
                }
                // Snapshot everything the trial can touch.
                let snap_routes = final_routes.clone();
                board.components[k].x = nx;
                board.components[k].y = ny;
                // Old pad positions (for plane drop-span stripping).
                let old_pads: Vec<(f64, f64)> = {
                    let c = &board.components[k];
                    let cos_t = c.theta.cos();
                    let sin_t = c.theta.sin();
                    c.pins
                        .iter()
                        .map(|p| {
                            (
                                ox + p.dx * cos_t - p.dy * sin_t,
                                oy + p.dx * sin_t + p.dy * cos_t,
                            )
                        })
                        .collect()
                };
                let cid = board.components[k].id;
                let mut affected: Vec<usize> = Vec::new();
                for (mi, net) in board.nets.iter().enumerate() {
                    if !net.pins.iter().any(|&(c, _)| c == cid) {
                        continue;
                    }
                    if net.plane_layer.is_some() {
                        // Strip only the moved pads' drop spans; the
                        // drop pass re-sites them at the new spot.
                        let r = &final_routes[mi];
                        let mut doomed = vec![false; r.path_spans.len()];
                        for (si, &(ps, pl)) in r.path_spans.iter().enumerate() {
                            let hit = r.segments[ps..ps + pl].iter().any(|sg| {
                                old_pads.iter().any(|&(gx, gy)| {
                                    (sg.start.0 - gx).hypot(sg.start.1 - gy) < 0.8
                                        || (sg.end.0 - gx).hypot(sg.end.1 - gy) < 0.8
                                })
                            });
                            if hit {
                                doomed[si] = true;
                            }
                        }
                        if doomed.iter().any(|d| *d) {
                            strip_route_spans(&mut final_routes[mi], &doomed);
                        }
                    } else {
                        final_routes[mi] = Route::empty(net.id);
                        affected.push(mi);
                    }
                }
                // Greedy reroute of the moved part's signal nets under
                // the exact commit gate, fat-first.
                affected.sort_by(|&a, &b| {
                    board.nets[b]
                        .required_trace_width_mm
                        .partial_cmp(&board.nets[a].required_trace_width_mm)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                for &mi in &affected {
                    let mut grid = RoutingGrid::build(board);
                    for (j, r) in final_routes.iter().enumerate() {
                        if j != mi && !r.is_empty() {
                            pathfinder::block_route_geometry(&mut grid, r, board);
                        }
                    }
                    let mut fresh = Route::empty(board.nets[mi].id);
                    let got = pathfinder::extend_route(
                        &mut grid, &board.nets[mi], board, &mut fresh, 1.0, 1.0, &[], &[],
                        false, None,
                    );
                    if got > 0 {
                        let mut bans = Vec::new();
                        if exact_commit_strip(board, final_routes, mi, &mut fresh, 0, &mut bans)
                            > 0
                        {
                            final_routes[mi] = fresh;
                        }
                    }
                }
                // Ladder for the stuck net + anything still short, then
                // re-drop moved plane pads.
                for &mi in affected.iter().chain(std::iter::once(&ni)) {
                    if pathfinder::unreached_sink_count(&board.nets[mi], board, &final_routes[mi])
                        > 0
                    {
                        offgrid_escape(board, final_routes, mi);
                    }
                }
                plane_drop_pass(board, final_routes);
                let after = total_unreached(board, final_routes);
                if after < before {
                    info!(
                        "part nudge: moved '{}' {step:.1}mm off the '{}' corridor (unreached {before} -> {after})",
                        board.components[k].refdes, board.nets[ni].name
                    );
                    gained += before - after;
                    before = after;
                    break 'cand;
                }
                // Strict-win only: revert wholesale.
                debug!(
                    "nudge trial FAILED: '{}' dir ({:.1},{:.1},{:.2}) step {step:.1} for '{}': unreached {before} -> {after}",
                    board.components[k].refdes, away.0, away.1, away.2, board.nets[ni].name
                );
                board.components[k].x = ox;
                board.components[k].y = oy;
                board.components[k].theta = otheta;
                *final_routes = snap_routes;
            }
            }
        }
        if before == 0 {
            break;
        }
    }
    let _ = start;
    gained
}

/// Bootstrap a WHOLE-NET failure: an empty route has no tree copper
/// for the escape ladder to attach to, so connect the net's first
/// pad pair directly — same-layer exact route, then shove-assisted,
/// then via-hop (mixed layers) / cross-under (same layer, crossing
/// fence). Returns true when a first span was committed.
pub fn bootstrap_empty_route(board: &Board, final_routes: &mut Vec<Route>, i: usize) -> bool {
    let net = &board.nets[i];
    // Same leaf-share taper as the main ladder (see offgrid_escape):
    // one pin's approach carries one pin's current.
    let width = match net.net_class {
        PnrNetClass::Power { .. } | PnrNetClass::Ground => {
            (net.required_trace_width_mm / (net.pins.len().max(1) as f64))
                .max(board.config.min_trace_width_mm)
                .min(net.required_trace_width_mm)
        }
        _ => net.required_trace_width_mm,
    };
    let n_layers = board.layer_stack.layers.len();
    let comp_idx: crate::det::HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(k, c)| (c.id, k))
        .collect();
    let mut pads: Vec<((f64, f64), usize, bool)> = Vec::new(); // pos, layer, thru
    for &(cid, pid) in &net.pins {
        let Some(&ci) = comp_idx.get(&cid) else { continue };
        let comp = &board.components[ci];
        let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pid) else { continue };
        if pin.unplaced {
            continue;
        }
        let cos_t = comp.theta.cos();
        let sin_t = comp.theta.sin();
        let px = comp.x + pin.dx * cos_t - pin.dy * sin_t;
        let py = comp.y + pin.dx * sin_t + pin.dy * cos_t;
        let layer = match comp.side {
            BoardSide::Top => 0,
            BoardSide::Bottom => n_layers - 1,
        };
        let thru = pin.pad.as_ref().map(|p| p.drill_mm.is_some()).unwrap_or(false);
        // route_bias: a THT pad works on the preferred side (mirrors
        // the grid router's terminal seeding) — the side-based layer
        // put ladder stubs on F.Cu under `route_bias bottom`.
        let layer = match (board.config.route_bias.as_deref(), thru) {
            (Some("bottom"), true) => n_layers - 1,
            (Some("top"), true) => 0,
            _ => layer,
        };
        pads.push(((px, py), layer, thru));
    }
    if pads.len() < 2 {
        return false;
    }
    let (seed, seed_layer, seed_thru) = pads[0];
    let mut targets: Vec<((f64, f64), usize, bool)> = pads[1..].to_vec();
    targets.sort_by(|a, b| {
        let da = (a.0 .0 - seed.0).hypot(a.0 .1 - seed.1);
        let db = (b.0 .0 - seed.0).hypot(b.0 .1 - seed.1);
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    });
    let via_r = board.layer_stack.via.pad_mm / 2.0;
    let hole_gap = board.layer_stack.via.drill_mm + 0.25;
    let net_id = net.id;
    for &(tgt, tgt_layer, tgt_thru) in targets.iter().take(3) {
        // Layers the pair can legally meet on: THT pads reach every
        // layer; SMD pads only their side.
        let common: Option<usize> = if seed_layer == tgt_layer {
            Some(seed_layer)
        } else if seed_thru {
            Some(tgt_layer)
        } else if tgt_thru {
            Some(seed_layer)
        } else {
            None
        };
        // 1) direct exact route on a common layer, with shoves.
        if let Some(l) = common {
            let mut snaps: Vec<(usize, Route)> = Vec::new();
            for _round in 0..3 {
                let idx = geom::ClearanceIndex::build(board, final_routes, Some(net_id));
                if let Some(path) = geom::route_escape(&idx, seed, tgt, width, l, net_id) {
                    commit_escape(&mut final_routes[i], &path, l, width, None, &net.name);
                    info!(
                        "bootstrap: whole-net '{}' seeded pad-to-pad on layer {l}",
                        net.name
                    );
                    return true;
                }
                let Some(bl) = geom::escape_blocker(&idx, seed, tgt, width, l, net_id)
                else {
                    break;
                };
                if !try_shove_track(
                    board, final_routes, i, &bl, seed, tgt, width, &mut snaps,
                ) {
                    break;
                }
            }
            for (jj, old) in snaps.into_iter().rev() {
                final_routes[jj] = old;
            }
        }
        // 2) tunnel: via(s) + far-layer leg. Same-layer pair =
        // cross-under (two vias); mixed pair = single-via hop.
        let mut budget = 6usize;
        // Strict route_bias: tunnels stay inside the net's layer mask.
        let tunnel_layers: Vec<usize> = board
            .layer_stack
            .signal_layer_indices()
            .into_iter()
            .filter(|l| net.allowed_layers.as_ref().map_or(true, |a| a.contains(l)))
            .collect();
        for l2 in tunnel_layers {
            if Some(l2) == common {
                continue;
            }
            let mut snaps: Vec<(usize, Route)> = Vec::new();
            // Seed side: SMD seed on another layer needs a via.
            let (v1, seed_leg) = if seed_thru || seed_layer == l2 {
                (None, None)
            } else {
                match claim_via_site(
                    board, final_routes, i, seed, via_r, None, &mut snaps, &mut budget,
                ) {
                    Some(v) => (Some(v), Some((seed, v, seed_layer))),
                    None => {
                        for (jj, old) in snaps.into_iter().rev() {
                            final_routes[jj] = old;
                        }
                        continue;
                    }
                }
            };
            let (v2, tgt_leg) = if tgt_thru || tgt_layer == l2 {
                (None, None)
            } else {
                match claim_via_site(
                    board, final_routes, i, tgt, via_r, v1, &mut snaps, &mut budget,
                ) {
                    Some(v) => (Some(v), Some((v, tgt, tgt_layer))),
                    None => {
                        for (jj, old) in snaps.into_iter().rev() {
                            final_routes[jj] = old;
                        }
                        continue;
                    }
                }
            };
            if let (Some(a), Some(b)) = (v1, v2) {
                if (a.0 - b.0).hypot(a.1 - b.1)
                    < hole_gap.max(2.0 * via_r + board.config.min_spacing_mm)
                {
                    for (jj, old) in snaps.into_iter().rev() {
                        final_routes[jj] = old;
                    }
                    continue;
                }
            }
            let idx = geom::ClearanceIndex::build(board, final_routes, Some(net_id));
            let t_from = v1.unwrap_or(seed);
            let t_to = v2.unwrap_or(tgt);
            let legs_ok = seed_leg
                .map(|(a, b, l)| idx.first_conflict(a, b, width, l, net_id).is_none())
                .unwrap_or(true)
                && tgt_leg
                    .map(|(a, b, l)| idx.first_conflict(a, b, width, l, net_id).is_none())
                    .unwrap_or(true);
            let tunnel = if legs_ok {
                geom::route_escape(&idx, t_from, t_to, width, l2, net_id)
            } else {
                None
            };
            let Some(path) = tunnel else {
                for (jj, old) in snaps.into_iter().rev() {
                    final_routes[jj] = old;
                }
                continue;
            };
            let route = &mut final_routes[i];
            let seg_start = route.segments.len();
            let via_start = route.vias.len();
            if let Some((a, b, l)) = seed_leg {
                route.segments.push(RouteSegment { layer: l, start: a, end: b, width_mm: width });
            }
            for w in path.windows(2) {
                if (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1) > 1e-9 {
                    route.segments.push(RouteSegment {
                        layer: l2,
                        start: w[0],
                        end: w[1],
                        width_mm: width,
                    });
                }
            }
            if let Some((a, b, l)) = tgt_leg {
                route.segments.push(RouteSegment { layer: l, start: a, end: b, width_mm: width });
            }
            let n_l = board.layer_stack.layers.len() - 1;
            let mut n_vias = 0usize;
            if let Some(v) = v1 {
                route.vias.push(RouteVia { x: v.0, y: v.1, from_layer: 0, to_layer: n_l });
                n_vias += 1;
            }
            if let Some(v) = v2 {
                route.vias.push(RouteVia { x: v.0, y: v.1, from_layer: 0, to_layer: n_l });
                n_vias += 1;
            }
            route.path_spans.push((seg_start, route.segments.len() - seg_start));
            route.path_parents.push(None);
            route.via_spans.push((via_start, n_vias));
            info!(
                "bootstrap: whole-net '{}' seeded through layer {l2} ({} via(s))",
                net.name, n_vias
            );
            return true;
        }
    }
    // 3) LONG-HAUL EXACT MAZE: the shapes above are short-range
    // (direct / L / via-hop a few mm) — a whole-net rip whose pads
    // face each other ACROSS an obstacle field (the ecc83 SRPP ring
    // link at THT-era widths) needs the wandering (x, y, layer) maze
    // with a board-sized window. Legality is exact per-edge, so the
    // seeded span ships validator-free like every ladder commit.
    {
        let signal_layers: Vec<usize> = {
            let all = board.layer_stack.signal_layer_indices();
            match &net.allowed_layers {
                Some(a) => all.into_iter().filter(|l| a.contains(l)).collect(),
                None => all,
            }
        };
        if !signal_layers.is_empty() {
            // A pad whose side-layer is masked off (strict bias) or
            // drilled starts on a legal layer instead.
            let eff = |l: usize, thru: bool| -> usize {
                if signal_layers.contains(&l) {
                    l
                } else if thru {
                    *signal_layers.last().unwrap()
                } else {
                    l
                }
            };
            let board_dim = board
                .config
                .outline
                .width()
                .max(board.config.outline.height());
            for &(tgt, tgt_layer, tgt_thru) in targets.iter().take(3) {
                let sl = eff(seed_layer, seed_thru);
                let tl = eff(tgt_layer, tgt_thru);
                if !signal_layers.contains(&sl) || !signal_layers.contains(&tl) {
                    continue;
                }
                let idx = geom::ClearanceIndex::build(board, final_routes, Some(net_id));
                // 25mm window first (cheap, usually enough), then the
                // full board — the maze's node cap arbitrates cost.
                let way = geom::route_tunnel_ml(
                    &idx, seed, sl, tgt, tl, width, via_r, &signal_layers, net_id, 25.0,
                )
                .or_else(|| {
                    geom::route_tunnel_ml(
                        &idx, seed, sl, tgt, tl, width, via_r, &signal_layers, net_id,
                        board_dim,
                    )
                });
                let Some(way) = way else { continue };
                let route = &mut final_routes[i];
                let seg_start = route.segments.len();
                let via_start = route.vias.len();
                let n_l = board.layer_stack.layers.len() - 1;
                for w in way.windows(2) {
                    let (a, b) = (w[0], w[1]);
                    if a.2 == b.2 {
                        if (a.0 - b.0).hypot(a.1 - b.1) > 1e-9 {
                            route.segments.push(RouteSegment {
                                layer: a.2,
                                start: (a.0, a.1),
                                end: (b.0, b.1),
                                width_mm: width,
                            });
                        }
                    } else {
                        let dup = route
                            .vias
                            .iter()
                            .any(|v| (v.x - a.0).hypot(v.y - a.1) < 1e-6);
                        if !dup {
                            route.vias.push(RouteVia {
                                x: a.0,
                                y: a.1,
                                from_layer: 0,
                                to_layer: n_l,
                            });
                        }
                    }
                }
                let n_vias = route.vias.len() - via_start;
                route.path_spans.push((seg_start, route.segments.len() - seg_start));
                route.path_parents.push(None);
                route.via_spans.push((via_start, n_vias));
                info!(
                    "bootstrap: whole-net '{}' seeded by LONG-HAUL maze ({} via(s))",
                    net.name, n_vias
                );
                return true;
            }
        }
    }
    false
}

fn offgrid_escape(board: &Board, final_routes: &mut Vec<Route>, i: usize) -> usize {
    use crate::routing::pathfinder::route_components;
    let net = &board.nets[i];
    // Power/Ground rails taper to LEAF SHARES everywhere else in the
    // pipeline; the ladder must too — a whole-rail IPC width (2.03mm
    // for uno's VCC_5V) can never leave a 0.5-pitch pad row, so AVCC
    // sat stranded while every escape stage failed at rail width.
    // One pin's approach carries one pin's current.
    let width = match net.net_class {
        PnrNetClass::Power { .. } | PnrNetClass::Ground => {
            (net.required_trace_width_mm / (net.pins.len().max(1) as f64))
                .max(board.config.min_trace_width_mm)
                .min(net.required_trace_width_mm)
        }
        _ => net.required_trace_width_mm,
    };
    // Fanout discipline applies to PLANE nets' surface legs: they
    // never need long surface transits (the plane is their highway),
    // so a leg transiting an IC body interior only steals that IC's
    // fanout room (see path_respects_courtyards).
    let courtyard_guard = net.plane_layer.is_some();
    let comp_idx: crate::det::HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(k, c)| (c.id, k))
        .collect();
    let n_layers = board.layer_stack.layers.len();
    let mut gained = 0usize;
    // Pads the whole ladder failed for THIS invocation: skip them and
    // keep trying the rest (an early return abandoned every pad after
    // the first stuck one).
    let mut failed: Vec<(f64, f64)> = Vec::new();
    // Bounded: every legitimate pass either connects a pad (max
    // pins) or marks one failed (max pins). Anything beyond that is
    // a connect/break CYCLE — a later commit re-orphaning an earlier
    // pad re-queues it forever (measured: the real-outline uno wedged
    // a whole trial inside this loop, one ClearanceIndex build per
    // spin). Deterministic cap; normal boards never approach it.
    for _pass in 0..net.pins.len() * 2 + 4 {
        let route = &final_routes[i];
        if route.is_empty() {
            // Whole-net failure: seed a first pad-to-pad span, then
            // the normal per-pad ladder takes over.
            if !bootstrap_empty_route(board, final_routes, i) {
                return gained;
            }
            gained += 1;
            continue;
        }
        let comps = route_components(route);
        let tree = {
            let mut pop: crate::det::HashMap<usize, usize> =
                crate::det::HashMap::default();
            for &c in &comps {
                *pop.entry(c).or_insert(0) += 1;
            }
            pop.into_iter()
                .max_by_key(|&(c, n)| (n, std::cmp::Reverse(c)))
                .map(|(c, _)| c)
        };
        // First unreached pad.
        // (pad, layer, thru, component center — for the radial-escape rung)
        let mut target: Option<((f64, f64), usize, bool, (f64, f64))> = None;
        for &(cid, pid) in &net.pins {
            let Some(&ci) = comp_idx.get(&cid) else { continue };
            let comp = &board.components[ci];
            let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pid) else { continue };
            if pin.unplaced {
                continue;
            }
            let cos_t = comp.theta.cos();
            let sin_t = comp.theta.sin();
            let px = comp.x + pin.dx * cos_t - pin.dy * sin_t;
            let py = comp.y + pin.dx * sin_t + pin.dy * cos_t;
            let half = pin
                .pad
                .as_ref()
                .map(|p| p.width_mm.min(p.height_mm) / 2.0)
                .unwrap_or(0.25);
            let pin_layer = match comp.side {
                BoardSide::Top => 0,
                BoardSide::Bottom => n_layers - 1,
            };
            let thru = pin.pad.as_ref().map(|p| p.drill_mm.is_some()).unwrap_or(false);
            // route_bias: a THT pad works on the preferred side
            // (mirrors the grid router's terminal seeding) — the
            // side-based layer routed via-hop stubs and same-layer
            // attaches on F.Cu under `route_bias bottom`.
            let pin_layer = match (board.config.route_bias.as_deref(), thru) {
                (Some("bottom"), true) => n_layers - 1,
                (Some("top"), true) => 0,
                _ => pin_layer,
            };
            // LAYER-AWARE: copper on another layer does not reach an
            // SMD pad (an inner run passing under an F.Cu pad counted
            // as touched — the pad was never repaired).
            let touched = route.segments.iter().enumerate().any(|(si, sg)| {
                Some(comps[si]) == tree
                    && (thru || sg.layer == pin_layer)
                    && geom::point_segment_dist((px, py), sg.start, sg.end)
                        < sg.width_mm / 2.0 + half - 0.001
            });
            if !touched
                && !failed
                    .iter()
                    .any(|f| (f.0 - px).hypot(f.1 - py) < 1e-6)
            {
                target = Some(((px, py), pin_layer, thru, (comp.x, comp.y)));
                break;
            }
        }
        let Some(((px, py), layer, thru, (ccx, ccy))) = target else { return gained };
        // Candidate attach points: projections onto same-layer tree
        // segments, nearest first, top 5.
        let mut attach: Vec<((f64, f64), f64)> = route
            .segments
            .iter()
            .enumerate()
            .filter(|(si, sg)| Some(comps[*si]) == tree && sg.layer == layer)
            .map(|(_, sg)| {
                let (dx, dy) = (sg.end.0 - sg.start.0, sg.end.1 - sg.start.1);
                let l2 = dx * dx + dy * dy;
                let t = if l2 <= 1e-12 {
                    0.0
                } else {
                    (((px - sg.start.0) * dx + (py - sg.start.1) * dy) / l2)
                        .clamp(0.0, 1.0)
                };
                let q = (sg.start.0 + t * dx, sg.start.1 + t * dy);
                let d = (px - q.0).hypot(py - q.1);
                (q, d)
            })
            .collect();
        {
            // Tree VIA centers as attach candidates too: a pad whose
            // only nearby same-net copper is a via barrel had no
            // candidates at all ("Pad VIN | Via [VIN_12V]" items).
            let route = &final_routes[i];
            for v in &route.vias {
                let touches_tree = route.segments.iter().enumerate().any(|(si, sg)| {
                    Some(comps[si]) == tree
                        && (geom::point_segment_dist((v.x, v.y), sg.start, sg.end)
                            < sg.width_mm / 2.0 + 0.05)
                });
                if touches_tree {
                    attach.push(((v.x, v.y), (px - v.x).hypot(py - v.y)));
                }
            }
        }
        attach.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        attach.truncate(5);
        // NO early return on an empty same-layer attach set: under
        // route_bias the ENTIRE tree can sit on the far side. The
        // any-layer rung (THT pads), the M2b via-hop (SMD pads) and
        // the mazes below all build their own cross-layer candidate
        // sets — the old `return gained` here abandoned every biased
        // off-side pin before those rungs ever ran (ecc83 3 unc /
        // mixer 14-of-17 unc at demo design rules, all this exit).
        let cidx = geom::ClearanceIndex::build(board, final_routes, Some(net.id));
        let mut connected = false;
        // Same-layer attempts first.
        for &(q, _) in &attach {
            if let Some(path) = geom::route_escape(&cidx, (px, py), q, width, layer, net.id)
            {
                if courtyard_guard && !path_respects_courtyards(board, &path) {
                    continue;
                }
                commit_escape(&mut final_routes[i], &path, layer, width, None, &net.name);
                gained += 1;
                connected = true;
                break;
            }
        }
        // THT ANY-LAYER ATTACH: a drilled pad pierces every layer —
        // route to it directly on ANY signal layer, no via at the pin
        // (the drill rule correctly refuses via sites inside the hole
        // ring, so a via-hop cannot serve a header pin). Real boards
        // reach edge headers from the back side exactly like this.
        if !connected && thru {
            // Strict route_bias: rungs stay inside the net's layer mask.
            let signal_layers: Vec<usize> = board
                .layer_stack
                .signal_layer_indices()
                .into_iter()
                .filter(|l| net.allowed_layers.as_ref().map_or(true, |a| a.contains(l)))
                .collect();
            'tht: for &l2 in signal_layers.iter().filter(|&&l| l != layer) {
                let route = &final_routes[i];
                let mut attach2: Vec<((f64, f64), f64)> = route
                    .segments
                    .iter()
                    .enumerate()
                    .filter(|(si, sg)| Some(comps[*si]) == tree && sg.layer == l2)
                    .map(|(_, sg)| {
                        let (dx, dy) = (sg.end.0 - sg.start.0, sg.end.1 - sg.start.1);
                        let l2n = dx * dx + dy * dy;
                        let t = if l2n <= 1e-12 {
                            0.0
                        } else {
                            (((px - sg.start.0) * dx + (py - sg.start.1) * dy) / l2n)
                                .clamp(0.0, 1.0)
                        };
                        let q = (sg.start.0 + t * dx, sg.start.1 + t * dy);
                        (q, (px - q.0).hypot(py - q.1))
                    })
                    .collect();
                attach2.sort_by(|a, b| {
                    a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)
                });
                attach2.truncate(4);
                for &(q, _) in &attach2 {
                    if let Some(path) =
                        geom::route_escape(&cidx, (px, py), q, width, l2, net.id)
                    {
                        if courtyard_guard && !path_respects_courtyards(board, &path) {
                            continue;
                        }
                        commit_escape(
                            &mut final_routes[i],
                            &path,
                            l2,
                            width,
                            None,
                            &net.name,
                        );
                        info!(
                            "completion: THT any-layer attach '{}' pad ({px:.2},{py:.2}) on layer {l2}",
                            net.name
                        );
                        gained += 1;
                        connected = true;
                        break 'tht;
                    }
                }
            }
        }
        // M2b VIA HOP: the pad's own layer may be fenced while the
        // other side is open. Site a via near the pad (exact barrel +
        // drill-rule check), stub to it on the pad's layer, then run
        // the continuous router on the far layer to same-net tree
        // copper THERE.
        if !connected {
            let via_r = board.layer_stack.via.pad_mm / 2.0;
            // Strict route_bias: rungs stay inside the net's layer mask.
            let signal_layers: Vec<usize> = board
                .layer_stack
                .signal_layer_indices()
                .into_iter()
                .filter(|l| net.allowed_layers.as_ref().map_or(true, |a| a.contains(l)))
                .collect();
            'hop: for &l2 in signal_layers.iter().filter(|&&l| l != layer) {
                // Attach candidates on the far layer.
                let route = &final_routes[i];
                let comps2 = route_components(route);
                let tree2 = {
                    let mut pop: crate::det::HashMap<usize, usize> =
                        crate::det::HashMap::default();
                    for &c in &comps2 {
                        *pop.entry(c).or_insert(0) += 1;
                    }
                    pop.into_iter()
                        .max_by_key(|&(c, n)| (n, std::cmp::Reverse(c)))
                        .map(|(c, _)| c)
                };
                let mut attach2: Vec<((f64, f64), f64)> = route
                    .segments
                    .iter()
                    .enumerate()
                    .filter(|(si, sg)| Some(comps2[*si]) == tree2 && sg.layer == l2)
                    .map(|(_, sg)| {
                        let (dx, dy) = (sg.end.0 - sg.start.0, sg.end.1 - sg.start.1);
                        let l2n = dx * dx + dy * dy;
                        let t = if l2n <= 1e-12 {
                            0.0
                        } else {
                            (((px - sg.start.0) * dx + (py - sg.start.1) * dy) / l2n)
                                .clamp(0.0, 1.0)
                        };
                        let q = (sg.start.0 + t * dx, sg.start.1 + t * dy);
                        ((q.0, q.1), (px - q.0).hypot(py - q.1))
                    })
                    .collect();
                attach2.sort_by(|a, b| {
                    a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)
                });
                attach2.truncate(4);
                if attach2.is_empty() {
                    continue;
                }
                // Via sites ring the pad.
                let mut shove_budget = 6usize;
                for ring in 0..6 {
                    let rr = 0.6 + ring as f64 * 0.35;
                    for k in 0..8 {
                        let ang = k as f64 * std::f64::consts::FRAC_PI_4;
                        let (vx, vy) = (px + rr * ang.cos(), py + rr * ang.sin());
                        // M4: a site blocked ONLY by a track is not
                        // dead — shove the track aside (exactly-gated
                        // bump away from the via barrel) and re-check.
                        let mut site_snapshots: Vec<(usize, Route)> = Vec::new();
                        let mut site_ok =
                            cidx.via_conflict(vx, vy, via_r, net.id).is_none();
                        if !site_ok && shove_budget > 0 {
                            for _ in 0..2 {
                                let idx_now = geom::ClearanceIndex::build(
                                    board,
                                    final_routes,
                                    Some(net.id),
                                );
                                match idx_now.via_conflict(vx, vy, via_r, net.id) {
                                    None => {
                                        site_ok = true;
                                        break;
                                    }
                                    Some(c @ geom::Conflict::Track { .. }) => {
                                        if shove_budget == 0
                                            || !try_shove_track(
                                                board,
                                                final_routes,
                                                i,
                                                &c,
                                                (vx, vy),
                                                (vx, vy),
                                                2.0 * via_r,
                                                &mut site_snapshots,
                                            )
                                        {
                                            break;
                                        }
                                        shove_budget -= 1;
                                    }
                                    Some(_) => break,
                                }
                            }
                        }
                        if !site_ok {
                            for (jj, old) in site_snapshots.into_iter().rev() {
                                final_routes[jj] = old;
                            }
                            continue;
                        }
                        let cidx = geom::ClearanceIndex::build(
                            board,
                            final_routes,
                            Some(net.id),
                        );
                        // Stub pad→via on the pad's layer.
                        if cidx
                            .first_conflict((px, py), (vx, vy), width, layer, net.id)
                            .is_some()
                        {
                            for (jj, old) in site_snapshots.into_iter().rev() {
                                final_routes[jj] = old;
                            }
                            continue;
                        }
                        let mut hop_done = false;
                        for &(q, _) in &attach2 {
                            if let Some(path) = geom::route_escape(
                                &cidx,
                                (vx, vy),
                                q,
                                width,
                                l2,
                                net.id,
                            ) {
                                // Commit: stub (pad layer) + via + far path.
                                let route = &mut final_routes[i];
                                let seg_start = route.segments.len();
                                let via_start = route.vias.len();
                                route.segments.push(RouteSegment {
                                    layer,
                                    start: (px, py),
                                    end: (vx, vy),
                                    width_mm: width,
                                });
                                for w in path.windows(2) {
                                    if (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1) > 1e-9 {
                                        route.segments.push(RouteSegment {
                                            layer: l2,
                                            start: w[0],
                                            end: w[1],
                                            width_mm: width,
                                        });
                                    }
                                }
                                route.vias.push(RouteVia {
                                    x: vx,
                                    y: vy,
                                    from_layer: 0,
                                    to_layer: board.layer_stack.layers.len() - 1,
                                });
                                route
                                    .path_spans
                                    .push((seg_start, route.segments.len() - seg_start));
                                route.path_parents.push(None);
                                route.via_spans.push((via_start, 1));
                                info!(
                                    "completion: OFF-GRID via-hop connected a '{}' pad at ({px:.2},{py:.2}) via ({vx:.2},{vy:.2})",
                                    net.name
                                );
                                gained += 1;
                                connected = true;
                                hop_done = true;
                                break 'hop;
                            }
                        }
                        if !hop_done {
                            for (jj, old) in site_snapshots.into_iter().rev() {
                                final_routes[jj] = old;
                            }
                        }
                    }
                }
            }
        }
        // M4 CROSS-UNDER: the fence CROSSES the corridor, so no
        // same-layer move helps and the tree has no far-layer copper
        // to hop to — tunnel beneath it: pad -> via -> far-layer leg
        // -> via -> back up to the tree. Both via sites are claimed
        // with shove assistance (foreign tracks bumped aside, every
        // move exactly gated).
        if !connected {
            let via_r = board.layer_stack.via.pad_mm / 2.0;
            let hole_gap = board.layer_stack.via.drill_mm + 0.25;
            // Strict route_bias: rungs stay inside the net's layer mask.
            let signal_layers: Vec<usize> = board
                .layer_stack
                .signal_layer_indices()
                .into_iter()
                .filter(|l| net.allowed_layers.as_ref().map_or(true, |a| a.contains(l)))
                .collect();
            let mut budget = 8usize;
            'under: for &l2 in signal_layers.iter().filter(|&&l| l != layer) {
                for &(q, _) in attach.iter().take(3) {
                    let mut snaps: Vec<(usize, Route)> = Vec::new();
                    let Some(v1) = claim_via_site(
                        board, final_routes, i, (px, py), via_r, None, &mut snaps,
                        &mut budget,
                    ) else {
                        debug!(
                            "cross-under: '{}' pad ({px:.2},{py:.2}): no v1 site",
                            net.name
                        );
                        for (jj, old) in snaps.into_iter().rev() {
                            final_routes[jj] = old;
                        }
                        continue;
                    };
                    let Some(v2) = claim_via_site(
                        board, final_routes, i, q, via_r, Some(v1), &mut snaps,
                        &mut budget,
                    ) else {
                        debug!(
                            "cross-under: '{}' attach ({:.2},{:.2}): no v2 site",
                            net.name, q.0, q.1
                        );
                        for (jj, old) in snaps.into_iter().rev() {
                            final_routes[jj] = old;
                        }
                        continue;
                    };
                    if (v1.0 - v2.0).hypot(v1.1 - v2.1)
                        < hole_gap.max(2.0 * via_r + board.config.min_spacing_mm)
                    {
                        for (jj, old) in snaps.into_iter().rev() {
                            final_routes[jj] = old;
                        }
                        continue;
                    }
                    let idx =
                        geom::ClearanceIndex::build(board, final_routes, Some(net.id));
                    // Stub pad->v1 and leg v2->q run through the FULL
                    // escape router (bends allowed — the straight-line
                    // check killed reachable sites: measured
                    // stub_ok=false with an L-shaped stub available);
                    // tunnel v1->v2 on the far layer. All exact.
                    let stub = geom::route_escape(&idx, (px, py), v1, width, layer, net.id);
                    let leg = if stub.is_some() {
                        geom::route_escape(&idx, v2, q, width, layer, net.id)
                    } else {
                        None
                    };
                    let tunnel = if stub.is_some() && leg.is_some() {
                        geom::route_escape(&idx, v1, v2, width, l2, net.id).or_else(|| {
                            // Maze-shaped last rung: fixed shapes can't
                            // thread long detours (measured 21.7mm).
                            geom::route_tunnel(&idx, v1, v2, width, l2, net.id)
                        })
                    } else {
                        None
                    };
                    let (Some(stub), Some(leg), Some(path)) = (stub, leg, tunnel) else {
                        debug!(
                            "cross-under: '{}' v1 ({:.2},{:.2}) v2 ({:.2},{:.2}): stub/leg/tunnel FAILED",
                            net.name, v1.0, v1.1, v2.0, v2.1
                        );
                        for (jj, old) in snaps.into_iter().rev() {
                            final_routes[jj] = old;
                        }
                        continue;
                    };
                    let route = &mut final_routes[i];
                    let seg_start = route.segments.len();
                    let via_start = route.vias.len();
                    for w in stub.windows(2) {
                        if (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1) > 1e-9 {
                            route.segments.push(RouteSegment {
                                layer,
                                start: w[0],
                                end: w[1],
                                width_mm: width,
                            });
                        }
                    }
                    for w in path.windows(2) {
                        if (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1) > 1e-9 {
                            route.segments.push(RouteSegment {
                                layer: l2,
                                start: w[0],
                                end: w[1],
                                width_mm: width,
                            });
                        }
                    }
                    for w in leg.windows(2) {
                        if (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1) > 1e-9 {
                            route.segments.push(RouteSegment {
                                layer,
                                start: w[0],
                                end: w[1],
                                width_mm: width,
                            });
                        }
                    }
                    let n_l = board.layer_stack.layers.len() - 1;
                    // Coincident own via = reuse (a re-cross-under of
                    // the same pad lands the same site: oracle
                    // holes_co_located); push only new barrels.
                    // Near-coincident same-net pairs are repaired at
                    // ship time by the final sweep (commit-time
                    // rejection here perturbed the whole seed-42
                    // evolution — via_dangling regression).
                    let mut pushed = 0usize;
                    for v in [v1, v2] {
                        if !route.vias.iter().any(|e| (e.x - v.0).hypot(e.y - v.1) < 1e-6) {
                            route.vias.push(RouteVia {
                                x: v.0,
                                y: v.1,
                                from_layer: 0,
                                to_layer: n_l,
                            });
                            pushed += 1;
                        }
                    }
                    route.path_spans.push((seg_start, route.segments.len() - seg_start));
                    route.path_parents.push(None);
                    route.via_spans.push((via_start, pushed));
                    info!(
                        "completion: OFF-GRID cross-under connected a '{}' pad at ({px:.2},{py:.2}) under the fence via ({:.2},{:.2})/({:.2},{:.2})",
                        net.name, v1.0, v1.1, v2.0, v2.1
                    );
                    gained += 1;
                    connected = true;
                    break 'under;
                }
            }
        }
        // MULTI-LAYER MAZE: the final rung. A pad with NO via room
        // nearby and NO single-layer corridor can still connect —
        // the (x, y, layer) search wanders to wherever a via fits,
        // dives, tunnels, resurfaces. Attach targets may sit on ANY
        // signal layer (tree segments, any component == tree).
        if !connected {
            let via_r = board.layer_stack.via.pad_mm / 2.0;
            // Strict route_bias: rungs stay inside the net's layer mask.
            let signal_layers: Vec<usize> = board
                .layer_stack
                .signal_layer_indices()
                .into_iter()
                .filter(|l| net.allowed_layers.as_ref().map_or(true, |a| a.contains(l)))
                .collect();
            // Attach candidates across all signal layers.
            let route = &final_routes[i];
            let mut attach_ml: Vec<((f64, f64), usize, f64)> = route
                .segments
                .iter()
                .enumerate()
                .filter(|(si, sg)| {
                    Some(comps[*si]) == tree && signal_layers.contains(&sg.layer)
                })
                .map(|(_, sg)| {
                    let (dx, dy) = (sg.end.0 - sg.start.0, sg.end.1 - sg.start.1);
                    let l2n = dx * dx + dy * dy;
                    let t = if l2n <= 1e-12 {
                        0.0
                    } else {
                        (((px - sg.start.0) * dx + (py - sg.start.1) * dy) / l2n)
                            .clamp(0.0, 1.0)
                    };
                    let q = (sg.start.0 + t * dx, sg.start.1 + t * dy);
                    (q, sg.layer, (px - q.0).hypot(py - q.1))
                })
                .collect();
            attach_ml.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
            // Candidate-set experiments all measured WORSE (open-exit
            // filtering of the nearest-3 reshuffled 3 clean seeds;
            // an open-exit fallback tier still degraded s13 3->5):
            // the residual knots are EVOLUTION-COUPLED — any new
            // mid-completion success cascades. Diagnosis lives in the
            // BHDL_PNR_ML_PROBE entry-edge probe (geom.rs).
            attach_ml.truncate(3);
            let cidx5 = geom::ClearanceIndex::build(board, final_routes, Some(net.id));
            for &(q, ql, _) in &attach_ml {
                // Normal wander first (preserves prior evolution
                // exactly); the WIDE retry fires only where the
                // normal region finds nothing — a 22mm connection
                // whose only corridor swings far around the chip
                // needs more than ±4mm of lateral room, but granting
                // it everywhere reshuffled the whole board (1→4 unc).
                let Some(way) = geom::route_tunnel_ml(
                    &cidx5, (px, py), layer, q, ql, width, via_r, &signal_layers, net.id,
                    4.0,
                )
                .or_else(|| {
                    geom::route_tunnel_ml(
                        &cidx5, (px, py), layer, q, ql, width, via_r, &signal_layers,
                        net.id, 12.0,
                    )
                })
                .or_else(|| {
                    // LONG-HAUL tier: fires only where both bounded
                    // windows found nothing (evolution-preserving) —
                    // at THT-era track widths whole regions wall off
                    // and the detour swings board-scale. The maze's
                    // node cap arbitrates cost on big boards.
                    let dim = board
                        .config
                        .outline
                        .width()
                        .max(board.config.outline.height());
                    geom::route_tunnel_ml(
                        &cidx5, (px, py), layer, q, ql, width, via_r, &signal_layers,
                        net.id, dim,
                    )
                })
                .or_else(|| {
                    // RADIAL ESCAPE (fidelity, THT): ring-package pads
                    // exit AWAY from the package center — the one
                    // corridor out of a lattice-sealed pocket (the
                    // demo's hand idiom; the pad-7 probe showed entry
                    // 8/8 open yet the search dead at ~206 nodes: no
                    // lattice node sits ON the radial line). Stub
                    // outward along the H/V/45-snapped radial
                    // (exact-checked), then maze from OUTSIDE the
                    // pocket; the stub ships as the way's first leg.
                    if !cidx5.ortho || !thru {
                        return None;
                    }
                    let (vx, vy) = (px - ccx, py - ccy);
                    if vx.hypot(vy) < 1e-6 {
                        return None;
                    }
                    let step = std::f64::consts::FRAC_PI_4;
                    let snapped = (vy.atan2(vx) / step).round() * step;
                    let (dx, dy) = (snapped.cos(), snapped.sin());
                    let dim = board
                        .config
                        .outline
                        .width()
                        .max(board.config.outline.height());
                    for l in [2.0f64, 3.0, 4.5, 6.0] {
                        let pout = (px + dx * l, py + dy * l);
                        if let Some(c) =
                            cidx5.first_conflict((px, py), pout, width, layer, net.id)
                        {
                            debug!(
                                "radial: '{}' stub ({px:.2},{py:.2})+{l:.1} blocked by {c:?}",
                                net.name
                            );
                            continue;
                        }
                        for m in [12.0, dim] {
                            if let Some(mut way) = geom::route_tunnel_ml(
                                &cidx5, pout, layer, q, ql, width, via_r,
                                &signal_layers, net.id, m,
                            ) {
                                way.insert(0, (px, py, layer));
                                let _ = m;
                                info!(
                                    "completion: RADIAL escape freed a '{}' pad at ({px:.2},{py:.2}) (stub {l:.1}mm)",
                                    net.name
                                );
                                return Some(way);
                            }
                        }
                    }
                    None
                }) else {
                    debug!(
                        "ml-maze: '{}' ({px:.2},{py:.2})l{layer} -> ({:.2},{:.2})l{ql} found no corridor",
                        net.name, q.0, q.1
                    );
                    continue;
                };
                // OWN-net vias are invisible to the index (skip_net),
                // but drill rules bind same-net too: a second maze
                // path for this net can land a via on (or within hole
                // gap of) one it placed earlier — oracle:
                // holes_co_located. Coincident = reuse (skip push);
                // near-but-not-coincident = reject this path.
                let hole_gap = board.layer_stack.via.drill_mm + 0.25;
                let own_conflict = way.windows(2).any(|w| {
                    w[0].2 != w[1].2
                        && final_routes[i].vias.iter().any(|v| {
                            let d = (v.x - w[0].0).hypot(v.y - w[0].1);
                            d > 1e-6 && d < hole_gap
                        })
                });
                if own_conflict {
                    continue;
                }
                let route = &mut final_routes[i];
                let seg_start = route.segments.len();
                let via_start = route.vias.len();
                let n_l = board.layer_stack.layers.len() - 1;
                for w in way.windows(2) {
                    let (a, b) = (w[0], w[1]);
                    if a.2 == b.2 {
                        if (a.0 - b.0).hypot(a.1 - b.1) > 1e-9 {
                            route.segments.push(RouteSegment {
                                layer: a.2,
                                start: (a.0, a.1),
                                end: (b.0, b.1),
                                width_mm: width,
                            });
                        }
                    } else {
                        // A chained switch (l0->l2->l4 at one lattice
                        // point) needs ONE full-stack via, not two
                        // stacked (oracle: holes_co_located).
                        let dup = route
                            .vias
                            .iter()
                            .any(|v| (v.x - a.0).hypot(v.y - a.1) < 1e-6);
                        if !dup {
                            route.vias.push(RouteVia {
                                x: a.0,
                                y: a.1,
                                from_layer: 0,
                                to_layer: n_l,
                            });
                        }
                    }
                }
                let n_vias = route.vias.len() - via_start;
                route.path_spans.push((seg_start, route.segments.len() - seg_start));
                route.path_parents.push(None);
                route.via_spans.push((via_start, n_vias));
                info!(
                    "completion: OFF-GRID multi-layer maze connected a '{}' pad at ({px:.2},{py:.2}) ({n_vias} via(s))",
                    net.name
                );
                gained += 1;
                connected = true;
                break;
            }
            // RADIAL PAD-PAIR: the missing link can be pad-to-pad
            // between TWO lattice-sealed ring pockets (ecc83 G2:
            // valve pad 1 <-> pad 7 — the demo's horseshoe with a
            // radial stub at EACH end). Stub both pads outward along
            // their H/V/45-snapped radials (exact-checked), maze
            // between the two escape points, ship stubs + way as one
            // span.
            if !connected && cidx5.ortho && thru {
                let step45 = std::f64::consts::FRAC_PI_4;
                let snap_dir = |vx: f64, vy: f64| -> Option<(f64, f64)> {
                    if vx.hypot(vy) < 1e-6 {
                        return None;
                    }
                    let a = (vy.atan2(vx) / step45).round() * step45;
                    Some((a.cos(), a.sin()))
                };
                // Other same-net THT pads not reached by the tree.
                let mut peers: Vec<((f64, f64), (f64, f64))> = Vec::new(); // (pad, center)
                for &(cid2, pid2) in &net.pins {
                    let Some(&ci2) = comp_idx.get(&cid2) else { continue };
                    let comp2 = &board.components[ci2];
                    let Some(pin2) = comp2.pins.iter().find(|p| p.pin_id == pid2) else {
                        continue;
                    };
                    if pin2.unplaced
                        || pin2.pad.as_ref().and_then(|p| p.drill_mm).is_none()
                    {
                        continue;
                    }
                    let cos2 = comp2.theta.cos();
                    let sin2 = comp2.theta.sin();
                    let qx = comp2.x + pin2.dx * cos2 - pin2.dy * sin2;
                    let qy = comp2.y + pin2.dx * sin2 + pin2.dy * cos2;
                    if (qx - px).hypot(qy - py) < 1e-6 {
                        continue;
                    }
                    peers.push(((qx, qy), (comp2.x, comp2.y)));
                }
                let dim = board
                    .config
                    .outline
                    .width()
                    .max(board.config.outline.height());
                'pair: for &((qx, qy), (qcx, qcy)) in &peers {
                    let Some((dax, day)) = snap_dir(px - ccx, py - ccy) else { continue };
                    let Some((dbx, dby)) = snap_dir(qx - qcx, qy - qcy) else { continue };
                    for la in [2.0f64, 3.0, 4.5] {
                        let pa = (px + dax * la, py + day * la);
                        if cidx5
                            .first_conflict((px, py), pa, width, layer, net.id)
                            .is_some()
                        {
                            continue;
                        }
                        for lb in [2.0f64, 3.0, 4.5] {
                            let pb = (qx + dbx * lb, qy + dby * lb);
                            if cidx5
                                .first_conflict((qx, qy), pb, width, layer, net.id)
                                .is_some()
                            {
                                continue;
                            }
                            let way = geom::route_tunnel_ml(
                                &cidx5, pa, layer, pb, layer, width, via_r,
                                &signal_layers, net.id, 12.0,
                            )
                            .or_else(|| {
                                geom::route_tunnel_ml(
                                    &cidx5, pa, layer, pb, layer, width, via_r,
                                    &signal_layers, net.id, dim,
                                )
                            });
                            let Some(mut way) = way else { continue };
                            way.insert(0, (px, py, layer));
                            way.push((qx, qy, layer));
                            let route = &mut final_routes[i];
                            let seg_start = route.segments.len();
                            let via_start = route.vias.len();
                            let n_l = board.layer_stack.layers.len() - 1;
                            for w in way.windows(2) {
                                let (a, b) = (w[0], w[1]);
                                if a.2 == b.2 {
                                    if (a.0 - b.0).hypot(a.1 - b.1) > 1e-9 {
                                        route.segments.push(RouteSegment {
                                            layer: a.2,
                                            start: (a.0, a.1),
                                            end: (b.0, b.1),
                                            width_mm: width,
                                        });
                                    }
                                } else {
                                    let dup = route.vias.iter().any(|v| {
                                        (v.x - a.0).hypot(v.y - a.1) < 1e-6
                                    });
                                    if !dup {
                                        route.vias.push(RouteVia {
                                            x: a.0,
                                            y: a.1,
                                            from_layer: 0,
                                            to_layer: n_l,
                                        });
                                    }
                                }
                            }
                            let n_vias = route.vias.len() - via_start;
                            route
                                .path_spans
                                .push((seg_start, route.segments.len() - seg_start));
                            route.path_parents.push(None);
                            route.via_spans.push((via_start, n_vias));
                            info!(
                                "completion: RADIAL PAD-PAIR joined '{}' ({px:.2},{py:.2})<->({qx:.2},{qy:.2}) ({n_vias} via(s))",
                                net.name
                            );
                            gained += 1;
                            connected = true;
                            break 'pair;
                        }
                    }
                }
            }
            // MAZE RIP LADDER: the entry-edge probe showed the last
            // knots' endpoints sealed by ONE net's copper threading a
            // pin row. Name the rippable Track blockers on the pad's
            // and top attaches' entry edges, rip one, retry the maze
            // through the freed corridor, rebuild the victim (grid
            // extend + exact strip only — NO recursive ladder), and
            // accept ONLY when the board's total unreached strictly
            // drops; anything else reverts both nets. The strict-win
            // gate is what the measured-worse candidate-set
            // experiments lacked.
            if !connected {
                let mut victims: Vec<usize> = Vec::new();
                {
                    let mut probe_pts: Vec<((f64, f64), usize)> =
                        vec![((px, py), layer)];
                    for &(q, ql, _) in attach_ml.iter().take(2) {
                        probe_pts.push((q, ql));
                    }
                    for (p, l) in probe_pts {
                        for d in [
                            (0.35, 0.0),
                            (-0.35, 0.0),
                            (0.0, 0.35),
                            (0.0, -0.35),
                            (0.25, 0.25),
                            (0.25, -0.25),
                            (-0.25, 0.25),
                            (-0.25, -0.25),
                        ] {
                            if let Some(geom::Conflict::Track { net: vn, .. }) = cidx5
                                .first_conflict(
                                    p,
                                    (p.0 + d.0, p.1 + d.1),
                                    width,
                                    l,
                                    net.id,
                                )
                            {
                                if let Some(vj) =
                                    board.nets.iter().position(|n| n.id == vn)
                                {
                                    if board.nets[vj].plane_layer.is_none()
                                        && vj != i
                                        && !final_routes[vj].is_empty()
                                        && !victims.contains(&vj)
                                    {
                                        victims.push(vj);
                                    }
                                }
                            }
                        }
                    }
                }
                let before_total = total_unreached(board, final_routes);
                'rip: for &vj in victims.iter().take(2) {
                    let snap_v = final_routes[vj].clone();
                    let snap_i = final_routes[i].clone();
                    final_routes[vj] = Route::empty(snap_v.net_id);
                    let cidx6 =
                        geom::ClearanceIndex::build(board, final_routes, Some(net.id));
                    let mut found: Option<Vec<(f64, f64, usize)>> = None;
                    'retry: for &(q, ql, _) in &attach_ml {
                        for m in [4.0, 12.0] {
                            if let Some(way) = geom::route_tunnel_ml(
                                &cidx6, (px, py), layer, q, ql, width, via_r,
                                &signal_layers, net.id, m,
                            ) {
                                found = Some(way);
                                break 'retry;
                            }
                        }
                    }
                    let Some(way) = found else {
                        final_routes[vj] = snap_v;
                        continue 'rip;
                    };
                    let hole_gap = board.layer_stack.via.drill_mm + 0.25;
                    let own_conflict = way.windows(2).any(|w| {
                        w[0].2 != w[1].2
                            && final_routes[i].vias.iter().any(|v| {
                                let d = (v.x - w[0].0).hypot(v.y - w[0].1);
                                d > 1e-6 && d < hole_gap
                            })
                    });
                    if own_conflict {
                        final_routes[vj] = snap_v;
                        continue 'rip;
                    }
                    {
                        let route = &mut final_routes[i];
                        let seg_start = route.segments.len();
                        let via_start = route.vias.len();
                        let n_l = board.layer_stack.layers.len() - 1;
                        for w in way.windows(2) {
                            let (a, b) = (w[0], w[1]);
                            if a.2 == b.2 {
                                if (a.0 - b.0).hypot(a.1 - b.1) > 1e-9 {
                                    route.segments.push(RouteSegment {
                                        layer: a.2,
                                        start: (a.0, a.1),
                                        end: (b.0, b.1),
                                        width_mm: width,
                                    });
                                }
                            } else if !route
                                .vias
                                .iter()
                                .any(|v| (v.x - a.0).hypot(v.y - a.1) < 1e-6)
                            {
                                route.vias.push(RouteVia {
                                    x: a.0,
                                    y: a.1,
                                    from_layer: 0,
                                    to_layer: n_l,
                                });
                            }
                        }
                        let n_vias = route.vias.len() - via_start;
                        route
                            .path_spans
                            .push((seg_start, route.segments.len() - seg_start));
                        route.path_parents.push(None);
                        route.via_spans.push((via_start, n_vias));
                    }
                    // Rebuild the victim on the board carrying the join.
                    let mut jgrid = RoutingGrid::build(board);
                    for (m2, r_) in final_routes.iter().enumerate() {
                        if m2 != vj && !r_.is_empty() {
                            pathfinder::block_route_geometry(&mut jgrid, r_, board);
                        }
                    }
                    let mut fresh = Route::empty(snap_v.net_id);
                    pathfinder::extend_route(
                        &mut jgrid, &board.nets[vj], board, &mut fresh, 1.0, 1.0, &[],
                        &[], false, None,
                    );
                    {
                        let mut bans = Vec::new();
                        exact_commit_strip(board, final_routes, vj, &mut fresh, 0, &mut bans);
                    }
                    final_routes[vj] = fresh;
                    let after_total = total_unreached(board, final_routes);
                    if after_total < before_total {
                        info!(
                            "completion: MAZE RIP connected a '{}' pad at ({px:.2},{py:.2}) after ripping '{}' (board unreached {before_total} -> {after_total})",
                            net.name, board.nets[vj].name
                        );
                        gained += 1;
                        connected = true;
                        break 'rip;
                    }
                    final_routes[i] = snap_i;
                    final_routes[vj] = snap_v;
                }
            }
        }
        // FRAGMENT SOURCES: the pad's entry stub often survives an
        // amputation that killed the tree path — the pad's own
        // little component. Its far endpoints can sit in open space
        // the fenced pad center can't reach; escape from THEM (the
        // pad connects through the fragment by copper overlap).
        if !connected {
            let route = &final_routes[i];
            let comps2 = route_components(route);
            let pad_comp: Option<usize> = route
                .segments
                .iter()
                .enumerate()
                .find(|(_, sg)| {
                    sg.layer == layer
                        && geom::point_segment_dist((px, py), sg.start, sg.end)
                            < sg.width_mm / 2.0 + 0.25
                })
                .map(|(si, _)| comps2[si]);
            if let Some(pc) = pad_comp {
                if Some(pc) != tree {
                    let mut sources: Vec<(f64, f64)> = Vec::new();
                    for (si, sg) in route.segments.iter().enumerate() {
                        if comps2[si] == pc && sg.layer == layer {
                            sources.push(sg.start);
                            sources.push(sg.end);
                        }
                    }
                    sources.sort_by(|a, b| {
                        let da = attach
                            .first()
                            .map(|&(q, _)| (a.0 - q.0).hypot(a.1 - q.1))
                            .unwrap_or(0.0);
                        let db = attach
                            .first()
                            .map(|&(q, _)| (b.0 - q.0).hypot(b.1 - q.1))
                            .unwrap_or(0.0);
                        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                    });
                    sources.dedup_by(|a, b| (a.0 - b.0).hypot(a.1 - b.1) < 1e-6);
                    sources.truncate(4);
                    let cidx3 =
                        geom::ClearanceIndex::build(board, final_routes, Some(net.id));
                    'frag: for &src in &sources {
                        for &(q, _) in attach.iter().take(3) {
                            if let Some(path) = geom::route_escape(
                                &cidx3, src, q, width, layer, net.id,
                            ) {
                                if courtyard_guard
                                    && !path_respects_courtyards(board, &path)
                                {
                                    continue;
                                }
                                commit_escape(
                                    &mut final_routes[i], &path, layer, width, None,
                                    &net.name,
                                );
                                info!(
                                    "completion: OFF-GRID fragment-end escape joined a '{}' island",
                                    net.name
                                );
                                gained += 1;
                                connected = true;
                                break 'frag;
                            }
                        }
                    }
                }
            }
        }
        // MAZE TUNNEL (same layer): the shapes + shoves above are
        // fast; when they all miss, one exact A* attempt at the
        // nearest attach point threads whatever corridor exists.
        if !connected {
            if let Some(&(q, _)) = attach.first() {
                let cidx4 = geom::ClearanceIndex::build(board, final_routes, Some(net.id));
                if let Some(path) = geom::route_tunnel(&cidx4, (px, py), q, width, layer, net.id)
                    .filter(|p| !courtyard_guard || path_respects_courtyards(board, p))
                {
                    commit_escape(&mut final_routes[i], &path, layer, width, None, &net.name);
                    info!(
                        "completion: OFF-GRID maze tunnel connected a '{}' pad at ({px:.2},{py:.2})",
                        net.name
                    );
                    gained += 1;
                    connected = true;
                }
            }
        }
        // M4 TRUE SHOVE: same-layer escape blocked by ONE foreign
        // track — deform it (exactly-gated lateral bump away from the
        // corridor) and retry. Rip-free: the neighbor's net stays
        // whole, only its geometry moves.
        if !connected {
            let mut snapshots: Vec<(usize, Route)> = Vec::new();
            'shove: for &(q, _) in attach.iter().take(4) {
                let undo_mark = snapshots.len();
                for _round in 0..5 {
                    let cidx2 =
                        geom::ClearanceIndex::build(board, final_routes, Some(net.id));
                    if let Some(path) =
                        geom::route_escape(&cidx2, (px, py), q, width, layer, net.id)
                    {
                        if courtyard_guard && !path_respects_courtyards(board, &path) {
                            break; // our own rule, not a blocker — next candidate
                        }
                        commit_escape(&mut final_routes[i], &path, layer, width, None, &net.name);
                        gained += 1;
                        connected = true;
                        break 'shove;
                    }
                    let bl = geom::escape_blocker(&cidx2, (px, py), q, width, layer, net.id);
                    debug!(
                        "shove probe: '{}' pad ({px:.2},{py:.2}) -> ({:.2},{:.2}): {:?}",
                        net.name, q.0, q.1, bl
                    );
                    let Some(bl) = bl else {
                        break;
                    };
                    if !try_shove_track(
                        board,
                        final_routes,
                        i,
                        &bl,
                        (px, py),
                        q,
                        width,
                        &mut snapshots,
                    ) {
                        break;
                    }
                }
                // This attach point failed: undo its deformations.
                while snapshots.len() > undo_mark {
                    let (j, old) = snapshots.pop().unwrap();
                    final_routes[j] = old;
                }
            }
            if !connected {
                for (j, old) in snapshots.into_iter().rev() {
                    final_routes[j] = old;
                }
            }
        }
        if !connected {
            failed.push((px, py));
        }
    }
    gained
}

/// Append an exact-routed escape polyline as one span.
fn commit_escape(
    route: &mut Route,
    path: &[(f64, f64)],
    layer: usize,
    width: f64,
    via: Option<RouteVia>,
    net_name: &str,
) {
    let seg_start = route.segments.len();
    let via_start = route.vias.len();
    for w in path.windows(2) {
        if (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1) > 1e-9 {
            route.segments.push(RouteSegment {
                layer,
                start: w[0],
                end: w[1],
                width_mm: width,
            });
        }
    }
    let n_new = route.segments.len() - seg_start;
    if n_new == 0 {
        return;
    }
    let n_vias = if let Some(v) = via {
        route.vias.push(v);
        1
    } else {
        0
    };
    route.path_spans.push((seg_start, n_new));
    route.path_parents.push(None);
    route.via_spans.push((via_start, n_vias));
    info!(
        "completion: OFF-GRID escape connected a '{net_name}' pad ({n_new} seg(s))"
    );
}

/// Targeted single-victim rip-up-and-reroute for the completion pass.
/// Returns sinks gained.
fn shove_one_blocker(
    board: &Board,
    final_routes: &mut Vec<Route>,
    i: usize,
) -> usize {
    // Unreached pad positions of net i (targets to unblock).
    let comp_idx: crate::det::HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(k, c)| (c.id, k))
        .collect();
    let pads: Vec<(f64, f64)> = board.nets[i]
        .pins
        .iter()
        .filter_map(|&(cid, pid)| {
            let comp = &board.components[*comp_idx.get(&cid)?];
            let pin = comp.pins.iter().find(|p| p.pin_id == pid)?;
            let cos_t = comp.theta.cos();
            let sin_t = comp.theta.sin();
            Some((
                comp.x + pin.dx * cos_t - pin.dy * sin_t,
                comp.y + pin.dx * sin_t + pin.dy * cos_t,
            ))
        })
        .collect();
    // Candidate blockers: routed, non-plane, not us, copper within 3mm
    // of any of our pads; cheapest routed length first; cap 6.
    let mut candidates: Vec<(usize, f64)> = Vec::new();
    for (j, r) in final_routes.iter().enumerate() {
        if j == i || r.is_empty() || board.nets[j].plane_layer.is_some() {
            continue;
        }
        let near = r.segments.iter().any(|sg| {
            pads.iter().any(|&(px, py)| {
                segment_point_too_close(sg.start, sg.end, (px, py), 3.0)
            })
        });
        if near {
            candidates.push((j, routing::measure::net_routed_length(r)));
        }
    }
    candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    candidates.truncate(6);

    let before_i = pathfinder::unreached_sink_count(&board.nets[i], board, &final_routes[i]);
    for (j, _) in candidates {
        let snap_i = final_routes[i].clone();
        let snap_j = final_routes[j].clone();
        // Grid with everything blocked EXCEPT nets i and j.
        let mut grid = RoutingGrid::build(board);
        for (k, r) in final_routes.iter().enumerate() {
            if k != i && k != j && !r.is_empty() {
                pathfinder::block_route_geometry(&mut grid, r, board);
            }
        }
        // Extend us with the blocker's copper absent.
        let mut mine = final_routes[i].clone();
        let mine_span = mine.path_spans.len();
        let got = pathfinder::extend_route(
            &mut grid, &board.nets[i], board, &mut mine, 1.0, 1.0, &[], &[], false, None,
        );
        if got == 0 {
            continue; // j wasn't the fence
        }
        // Gate vs everything except the ripped blocker j (its copper
        // is absent from the trial board by construction).
        {
            let mut trial_board: Vec<Route> = final_routes.clone();
            trial_board[j] = Route::empty(final_routes[j].net_id);
            let mut bans = Vec::new();
            if exact_commit_strip(board, &trial_board, i, &mut mine, mine_span, &mut bans)
                == 0
            {
                continue;
            }
        }
        final_routes[i] = mine;
        // Re-route the blocker on the updated board.
        let mut jgrid = RoutingGrid::build(board);
        for (k, r) in final_routes.iter().enumerate() {
            if k != j && !r.is_empty() {
                pathfinder::block_route_geometry(&mut jgrid, r, board);
            }
        }
        let mut fresh = Route::empty(final_routes[j].net_id);
        pathfinder::extend_route(
            &mut jgrid, &board.nets[j], board, &mut fresh, 1.0, 1.0, &[], &[], false, None,
        );
        {
            let mut bans = Vec::new();
            let mut trial_board: Vec<Route> = final_routes.clone();
            trial_board[j] = Route::empty(final_routes[j].net_id);
            exact_commit_strip(board, &trial_board, j, &mut fresh, 0, &mut bans);
        }
        let j_unreached_before =
            pathfinder::unreached_sink_count(&board.nets[j], board, &snap_j);
        let j_unreached_after =
            pathfinder::unreached_sink_count(&board.nets[j], board, &fresh);
        let i_unreached_after =
            pathfinder::unreached_sink_count(&board.nets[i], board, &final_routes[i]);
        // Accept only a strict total win.
        if i_unreached_after + j_unreached_after < before_i + j_unreached_before {
            final_routes[j] = fresh;
            info!(
                "completion shove: '{}' freed by re-routing '{}' (unreached {} → {})",
                board.nets[i].name,
                board.nets[j].name,
                before_i + j_unreached_before,
                i_unreached_after + j_unreached_after
            );
            return before_i.saturating_sub(i_unreached_after);
        }
        final_routes[i] = snap_i;
        final_routes[j] = snap_j;
    }
    0
}

/// Add serpentine length to the short member of each skew-violating
/// pair / length-match group. Returns the number of nets meandered.
fn meander_pass(board: &Board, final_routes: &mut Vec<Route>) -> usize {
    use crate::constraint::Constraint;
    let idx_of = |nid: NetId| board.nets.iter().position(|n| n.id == nid);
    // Collect (short_idx, long_idx, needed_mm) work items.
    let mut jobs: Vec<(usize, f64)> = Vec::new();
    let mut seen: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
    for c in &board.constraints {
        let (nets, tol): (Vec<usize>, f64) = match c {
            Constraint::DiffPair { p_net, n_net, length_match_mm, .. } => {
                let limit = board
                    .constraints
                    .iter()
                    .find_map(|c2| match c2 {
                        Constraint::LengthMatchGroup { nets, tolerance_mm, .. }
                            if nets.contains(p_net) && nets.contains(n_net) =>
                        {
                            Some(*tolerance_mm)
                        }
                        _ => None,
                    })
                    .unwrap_or(*length_match_mm);
                (
                    [*p_net, *n_net].iter().filter_map(|n| idx_of(*n)).collect(),
                    limit as f64,
                )
            }
            Constraint::LengthMatchGroup { nets, tolerance_mm, .. } => (
                nets.iter().filter_map(|n| idx_of(*n)).collect(),
                *tolerance_mm as f64,
            ),
            _ => continue,
        };
        if nets.len() < 2 {
            continue;
        }
        let lens: Vec<f64> = nets
            .iter()
            .map(|&i| routing::measure::net_routed_length(&final_routes[i]))
            .collect();
        let max = lens.iter().cloned().fold(0.0_f64, f64::max);
        if lens.iter().any(|&l| l <= 0.0) {
            continue; // unrouted member — skew is not the problem
        }
        for (k, &i) in nets.iter().enumerate() {
            let short = lens[k];
            if max - short > tol && seen.insert(i) {
                // Aim for mid-band, not the edge.
                jobs.push((i, (max - short) - tol * 0.5));
            }
        }
    }
    let mut done = 0usize;
    for (i, needed) in jobs {
        if board.nets[i].plane_layer.is_some() {
            continue;
        }
        let mut ok = false;
        // ONE exact-geometry index per job: other nets' copper is
        // stable across attempts (P1 kernel M1 — replaces this file's
        // hand-rolled copy of the clearance predicates).
        let cidx = geom::ClearanceIndex::build(board, final_routes, Some(board.nets[i].id));
        // A bump field toward the coupled partner collides with its
        // copper, and tight corridors reject deep bumps — walk
        // (run × depth × side) until the LOCAL clearance check accepts.
        // (A full validate_and_rip here is wrong: the validator is
        // stricter than KiCad in spots and would rip PRE-EXISTING
        // shipped-legal copper, poisoning the transaction.)
        'attempts: for run_rank in 0..64 {
        for depth in [1.2, 0.9, 0.6, 0.45, 0.3] {
        // side 0.0 = ALTERNATING serpentine (odd bumps up, even bumps
        // down): fits tight corridors where a one-sided bump field
        // needs the full depth clear on a single side.
        for side in [1.0, -1.0, 0.0] {
            let snapshot = final_routes[i].clone();
            let Some((s0, sn)) =
                apply_meander(board, &mut final_routes[i], needed, side, depth, run_rank)
            else {
                continue 'attempts; // this run has no room; try the next
            };
            if std::env::var("BHDL_PNR_DEBUG_NETS").is_ok() {
                log::warn!(
                    "meander attempt '{}' run_rank={run_rank} side={side} depth={depth}: spliced {sn} segs at {s0}",
                    board.nets[i].name
                );
                for sg in &final_routes[i].segments[s0..s0 + sn] {
                    log::warn!(
                        "   new ({:.2},{:.2})-({:.2},{:.2}) L{}",
                        sg.start.0, sg.start.1, sg.end.0, sg.end.1, sg.layer
                    );
                }
            }
            let clear = final_routes[i].segments[s0..s0 + sn].iter().all(|sg| {
                cidx.first_conflict(sg.start, sg.end, sg.width_mm, sg.layer, board.nets[i].id)
                    .is_none()
            });
            if clear {
                ok = true;
                break 'attempts;
            }
            final_routes[i] = snapshot;
        }
        }
        }
        if ok {
            done += 1;
        } else {
            log::info!(
                "meander on '{}' rejected on every run/side/depth — skew FAIL stands",
                board.nets[i].name
            );
        }
    }
    done
}


/// Replace part of the longest straight tree segment with a square-wave
/// serpentine adding ~`needed` mm. Returns false when no segment offers
/// enough room (honest FAIL).
fn apply_meander(
    board: &Board,
    route: &mut Route,
    needed: f64,
    side: f64,
    depth: f64,
    run_rank: usize,
) -> Option<(usize, usize)> {
    let comps = pathfinder::route_components(route);
    let tree = {
        let mut pop: crate::det::HashMap<usize, usize> = crate::det::HashMap::default();
        for &c in &comps {
            *pop.entry(c).or_insert(0) += 1;
        }
        pop.into_iter()
            .max_by_key(|&(c, n)| (n, std::cmp::Reverse(c)))
            .map(|(c, _)| c)
    };
    // Longest COLLINEAR RUN of consecutive tree segments (grid routing
    // emits 1-cell pieces — a single segment is never long enough),
    // kept within one path span so the splice bookkeeping stays sane.
    let span_of = |si: usize| -> Option<usize> {
        route
            .path_spans
            .iter()
            .position(|&(ps, pl)| si >= ps && si < ps + pl)
    };
    let mut runs: Vec<(usize, usize, f64)> = Vec::new(); // (start, end incl, len)
    let mut si = 0;
    while si < route.segments.len() {
        let sg = &route.segments[si];
        let axis_x = (sg.end.1 - sg.start.1).abs() < 1e-9;
        let axis_y = (sg.end.0 - sg.start.0).abs() < 1e-9;
        if Some(comps[si]) != tree || (!axis_x && !axis_y) {
            si += 1;
            continue;
        }
        // signum(0.0) is +1.0 in Rust — naive signum pairs gave a
        // horizontal segment direction (1,1), merging it with a
        // following DIAGONAL into one "run" (the serpentine then bent
        // diagonally and hit everything).
        let dnorm = |dx: f64, dy: f64| -> (i8, i8) {
            let f = |v: f64| {
                if v > 1e-9 {
                    1
                } else if v < -1e-9 {
                    -1
                } else {
                    0
                }
            };
            (f(dx), f(dy))
        };
        let dir = dnorm(sg.end.0 - sg.start.0, sg.end.1 - sg.start.1);
        let sp = span_of(si);
        let mut sj = si;
        while sj + 1 < route.segments.len() {
            let nx = &route.segments[sj + 1];
            let chained = (route.segments[sj].end.0 - nx.start.0).abs() < 1e-9
                && (route.segments[sj].end.1 - nx.start.1).abs() < 1e-9;
            let same_dir = dnorm(nx.end.0 - nx.start.0, nx.end.1 - nx.start.1) == dir;
            if !chained
                || !same_dir
                || nx.layer != sg.layer
                || Some(comps[sj + 1]) != tree
                || span_of(sj + 1) != sp
            {
                break;
            }
            sj += 1;
        }
        let run_len = (route.segments[sj].end.0 - route.segments[si].start.0)
            .hypot(route.segments[sj].end.1 - route.segments[si].start.1);
        runs.push((si, sj, run_len));
        si = sj + 1;
    }
    runs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    let Some(&(si, sj, seg_len)) = runs.get(run_rank) else { return None };
    let sg = RouteSegment {
        layer: route.segments[si].layer,
        start: route.segments[si].start,
        end: route.segments[sj].end,
        width_mm: route.segments[si].width_mm,
    };
    let pitch = 0.3;
    let spacing = board.config.min_spacing_mm;
    let w = sg.width_mm;
    // Bump geometry: depth d adds 2d per bump; bump pitch along the
    // segment = 2 cells (rise/run) + one cell gap.
    let d: f64 = depth.min(needed / 2.0).max(pitch);
    // Each bump consumes exactly 2·(w+spacing) of run (rise-advance-
    // fall-advance) — the old +pitch overestimate halved capacity.
    let bump_pitch = 2.0 * (w + spacing);
    let margin = pitch;
    let n_fit = ((seg_len - 2.0 * margin) / bump_pitch).floor() as usize;
    let n_need = (needed / (2.0 * d)).ceil() as usize;
    let n = n_fit.min(n_need);
    if n == 0 {
        return None;
    }
    let (ux, uy) = (
        (sg.end.0 - sg.start.0).signum() * if (sg.end.0 - sg.start.0).abs() > 1e-9 { 1.0 } else { 0.0 },
        (sg.end.1 - sg.start.1).signum() * if (sg.end.1 - sg.start.1).abs() > 1e-9 { 1.0 } else { 0.0 },
    );
    // side ±1 = one-sided bump field; side 0 = alternating (odd bumps
    // one way, even bumps the other).
    let alternating = side.abs() < 1e-9;
    let base_side = if alternating { 1.0 } else { side };
    let (nx, ny) = (-uy * base_side, ux * base_side);
    // Build the serpentine polyline from sg.start to sg.end.
    let mut pts: Vec<(f64, f64)> = vec![sg.start];
    let mut pos = sg.start;
    let mut adv = |pts: &mut Vec<(f64, f64)>, pos: &mut (f64, f64), dx: f64, dy: f64| {
        *pos = (pos.0 + dx, pos.1 + dy);
        pts.push(*pos);
    };
    adv(&mut pts, &mut pos, ux * margin, uy * margin);
    for k in 0..n {
        let flip = if alternating && k % 2 == 1 { -1.0 } else { 1.0 };
        adv(&mut pts, &mut pos, nx * d * flip, ny * d * flip);
        adv(&mut pts, &mut pos, ux * (w + spacing), uy * (w + spacing));
        adv(&mut pts, &mut pos, -nx * d * flip, -ny * d * flip);
        adv(&mut pts, &mut pos, ux * (w + spacing), uy * (w + spacing));
    }
    pts.push(sg.end);
    // Splice: replace segments[si] with the polyline, fixing span
    // bookkeeping (that span grows; later spans shift).
    let new_segs: Vec<RouteSegment> = pts
        .windows(2)
        .filter(|w2| (w2[0].0 - w2[1].0).hypot(w2[0].1 - w2[1].1) > 1e-9)
        .map(|w2| RouteSegment {
            layer: sg.layer,
            start: w2[0],
            end: w2[1],
            width_mm: w,
        })
        .collect();
    let replaced = sj - si + 1;
    let n_new = new_segs.len();
    let added = n_new as i64 - replaced as i64;
    route.segments.splice(si..sj + 1, new_segs);
    for span in route.path_spans.iter_mut() {
        if si >= span.0 && si < span.0 + span.1 {
            span.1 = (span.1 as i64 + added) as usize;
        } else if span.0 > si {
            span.0 = (span.0 as i64 + added) as usize;
        }
    }
    Some((si, n_new))
}

/// Bridge stranded pour islands (see the 5.93 call site). For each
/// signal-layer pour net: raster the fill exactly as emission will,
/// label islands, map every same-net anchor (via / THT pad / pour-
/// side SMD pad) to its island, and stitch each anchored non-main
/// island to the main one with a straight (or L) same-net track on
/// the pour layer. Candidate stitch targets are the K nearest main-
/// island cells to the stranded anchor; legality is the exact kernel
/// vs foreign copper only (same-net contact at both ends is the
/// join). Returns the number of bridges added.
fn pour_island_stitch(board: &Board, final_routes: &mut [Route]) -> usize {
    let mut stitched = 0usize;
    for ni in 0..board.nets.len() {
        let Some(pl) = board.nets[ni].plane_layer else { continue };
        if board.layer_stack.layers.get(pl).map(|l| l.kind)
            != Some(crate::types::LayerKind::Signal)
        {
            continue;
        }
        let Some(raster) = output::kicad::pour_raster(board, final_routes, ni) else {
            continue;
        };
        if raster.n_labels <= 1 {
            continue;
        }
        let anchors = output::kicad::plane_anchor_points(board, final_routes, ni);
        let mut per_label: std::collections::BTreeMap<u32, Vec<(f64, f64)>> =
            std::collections::BTreeMap::new();
        for &(ax, ay) in &anchors {
            let l = raster.label_at(ax, ay);
            if l != 0 {
                per_label.entry(l).or_default().push((ax, ay));
            }
        }
        if per_label.len() <= 1 {
            continue;
        }
        // Main island = most anchors (ties: lowest label — deterministic).
        let main = per_label
            .iter()
            .max_by_key(|(l, v)| (v.len(), std::cmp::Reverse(**l)))
            .map(|(l, _)| *l)
            .unwrap();
        let net_id = board.nets[ni].id;
        let width = board.nets[ni].required_trace_width_mm.clamp(0.3, 0.5);
        let cidx = geom::ClearanceIndex::build(board, final_routes, Some(net_id));
        // Same-net PIN anchors with identity — the router fallback
        // routes pin-to-pin when the flat stitch is fenced.
        let pour_side = if pl == 0 { BoardSide::Top } else { BoardSide::Bottom };
        let mut pin_anchors: Vec<((f64, f64), (ComponentId, PinId))> = Vec::new();
        // EVERY same-net pin (any side, any pad kind) — the routed
        // fallback's stranded endpoint can be a top-side SMD pad that
        // reaches the island only through its drop stub+via.
        let mut all_pins: Vec<((f64, f64), (ComponentId, PinId))> = Vec::new();
        for comp in &board.components {
            let cos_t = comp.theta.cos();
            let sin_t = comp.theta.sin();
            for pin in &comp.pins {
                if pin.net != Some(net_id) {
                    continue;
                }
                let Some(pad) = &pin.pad else { continue };
                let pos = (
                    comp.x + pin.dx * cos_t - pin.dy * sin_t,
                    comp.y + pin.dx * sin_t + pin.dy * cos_t,
                );
                all_pins.push((pos, (comp.id, pin.pin_id)));
                if pad.drill_mm.is_some() || comp.side == pour_side {
                    pin_anchors.push((pos, (comp.id, pin.pin_id)));
                }
            }
        }
        // Union-find over labels so islands merged by an earlier
        // stitch this round count as main.
        let mut joined: std::collections::BTreeSet<u32> =
            std::collections::BTreeSet::new();
        joined.insert(main);
        for (&l, island_anchors) in &per_label {
            if joined.contains(&l) {
                continue;
            }
            let &(ax, ay) = &island_anchors[0];
            // K nearest main-island cells to the stranded anchor.
            let mut cands: Vec<(f64, (f64, f64))> = Vec::new();
            for r in 0..raster.rows {
                for c in 0..raster.cols {
                    if raster.label[r * raster.cols + c] == main {
                        let p = raster.cell_center(r, c);
                        cands.push(((p.0 - ax).hypot(p.1 - ay), p));
                    }
                }
            }
            cands.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            cands.truncate(16);
            let mut done = false;
            for &(_, (bx, by)) in &cands {
                // Straight stitch, then the two L-bends.
                let paths: [Vec<((f64, f64), (f64, f64))>; 3] = [
                    vec![((ax, ay), (bx, by))],
                    vec![((ax, ay), (bx, ay)), ((bx, ay), (bx, by))],
                    vec![((ax, ay), (ax, by)), ((ax, by), (bx, by))],
                ];
                for legs in &paths {
                    if legs
                        .iter()
                        .all(|&(a, b)| cidx.first_conflict(a, b, width, pl, net_id).is_none())
                    {
                        for &(a, b) in legs {
                            final_routes[ni].segments.push(crate::types::RouteSegment {
                                layer: pl,
                                start: a,
                                end: b,
                                width_mm: width,
                            });
                        }
                        info!(
                            "pour island stitch: '{}' island bridged ({:.1},{:.1})→({:.1},{:.1})",
                            board.nets[ni].name, ax, ay, bx, by
                        );
                        joined.insert(l);
                        stitched += 1;
                        done = true;
                        break;
                    }
                }
                if done {
                    break;
                }
            }
            if !done {
                // ROUTER FALLBACK: the island is fenced on the pour
                // layer (foreign tracks encircle it — the line_buffer
                // power-header case). Route its PIN anchor to a same-
                // net pin in the main island with the real router
                // (vias + escapes), exact-commit-stripped, and MERGE
                // the copper into the pour net's route.
                let stranded_pin = pin_anchors
                    .iter()
                    .find(|(p, _)| raster.label_at(p.0, p.1) == l)
                    .map(|(_, id)| *id)
                    // A pin-less stranded island (a drop VIA island):
                    // route from the pad that drop SERVES — the same-
                    // net pin within stub reach of the island's via
                    // anchor. The pad already connects to the island
                    // through its stub+via, so bridging the pad to the
                    // main island joins the clusters.
                    .or_else(|| {
                        island_anchors.iter().find_map(|&(vx, vy)| {
                            all_pins
                                .iter()
                                .filter(|(p, _)| (p.0 - vx).hypot(p.1 - vy) < 1.5)
                                .min_by(|(a, _), (b, _)| {
                                    let da = (a.0 - vx).hypot(a.1 - vy);
                                    let db = (b.0 - vx).hypot(b.1 - vy);
                                    da.partial_cmp(&db)
                                        .unwrap_or(std::cmp::Ordering::Equal)
                                })
                                .map(|(_, id)| *id)
                        })
                    });
                let target_pin = pin_anchors
                    .iter()
                    .find(|(p, _)| raster.label_at(p.0, p.1) == main)
                    .map(|(_, id)| *id);
                if let (Some(sp), Some(tp)) = (stranded_pin, target_pin) {
                    let synth = PnrNet {
                        pins: vec![sp, tp],
                        plane_layer: None,
                        ..board.nets[ni].clone()
                    };
                    let mut grid = RoutingGrid::build(board);
                    for (j, route) in final_routes.iter().enumerate() {
                        if j != ni && !route.is_empty() {
                            pathfinder::block_route_geometry(&mut grid, route, board);
                        }
                    }
                    let mut rebuilt =
                        pathfinder::route_single_net(&grid, &synth, board, true, None);
                    if !rebuilt.is_empty() {
                        let mut bans: Vec<(f64, f64)> = Vec::new();
                        let kept = exact_commit_strip(
                            board, final_routes, ni, &mut rebuilt, 0, &mut bans,
                        );
                        // A PARTIAL bridge is worse than none: merged
                        // half-copper becomes fresh litter and the
                        // island stays stranded. Accept only when the
                        // stripped route still connects BOTH endpoints.
                        if kept > 0
                            && pathfinder::unreached_sink_count(&synth, board, &rebuilt)
                                == 0
                        {
                            final_routes[ni].segments.extend(rebuilt.segments);
                            final_routes[ni].vias.extend(rebuilt.vias);
                            info!(
                                "pour island stitch: '{}' fenced island near ({:.1},{:.1}) bridged by ROUTED fallback ({kept} branch(es))",
                                board.nets[ni].name, ax, ay
                            );
                            joined.insert(l);
                            stitched += 1;
                            done = true;
                        }
                    }
                }
            }
            if !done {
                log::warn!(
                    "pour island stitch: no legal bridge for a '{}' island near ({:.1},{:.1}) — unconnected stands (honest)",
                    board.nets[ni].name, ax, ay
                );
            }
        }
    }
    stitched
}

fn plane_drop_pass(board: &Board, final_routes: &mut [Route]) -> usize {
    // FIXPOINT: siting anticipates the fill's hole punches, but the
    // authoritative swallow test runs against the hole set AFTER all
    // drops land — a drop can be sited legally and still end up
    // swallowed. Ban each removed site and re-site until no drop is
    // removed (bounded; each iteration shrinks the candidate space).
    let mut banned: Vec<(f64, f64)> = Vec::new();
    let mut total = 0usize;
    for _ in 0..4 {
        let before = banned.len();
        let dropped = plane_drop_iteration(board, final_routes, &mut banned);
        total += dropped;
        if banned.len() == before {
            break; // no drop removed — stable
        }
    }
    total
}

/// One drop + swallow-verify iteration. Removed drop sites are appended
/// to `banned`; returns how many drops were newly placed (before
/// removal).
fn plane_drop_iteration(
    board: &Board,
    final_routes: &mut [Route],
    banned: &mut Vec<(f64, f64)>,
) -> usize {
    let dropped = plane_via_drops(board, final_routes, banned);
    // Verify every drop still TOUCHES its plane after hole merging:
    // punches around clustered foreign barrels chain-merge into
    // circles bigger than any siting gap anticipates, and a drop
    // fully inside one has no plane contact (ships as
    // via_dangling). Remove swallowed drops — honest unconnected.
    let via_r = board.layer_stack.via.pad_mm / 2.0;
    for i in 0..board.nets.len() {
        if board.nets[i].plane_layer.is_none() {
            continue;
        }
        let holes = output::kicad::merge_holes(output::kicad::plane_foreign_holes(
            board,
            final_routes,
            board.nets[i].id,
        ));
        let r = &mut final_routes[i];
        let mut vi = 0;
        while vi < r.vias.len() {
            let v = &r.vias[vi];
            let swallowed = output::kicad::plane_swallows(
                board,
                &holes,
                v.x,
                v.y,
                via_r,
                board.nets[i].plane_region,
            );
            if swallowed {
                banned.push((v.x, v.y));
                log::warn!(
                    "plane via drop at ({:.2},{:.2}) on '{}' swallowed by a merged \
                     fill punch — removed (site banned, will re-site)",
                    v.x, v.y, board.nets[i].name
                );
                // Drop spans are (1 segment, 1 via) each, appended in
                // order: find the span whose via range contains vi.
                if let Some(sp) = r
                    .via_spans
                    .iter()
                    .position(|&(vs, vl)| vl > 0 && vi >= vs && vi < vs + vl)
                {
                    let (ps, pl) = r.path_spans[sp];
                    r.segments.drain(ps..ps + pl);
                    r.vias.remove(vi);
                    r.path_spans.remove(sp);
                    r.path_parents.remove(sp);
                    r.via_spans.remove(sp);
                    for q in r.path_spans.iter_mut() {
                        if q.0 > ps {
                            q.0 -= pl;
                        }
                    }
                    for q in r.via_spans.iter_mut() {
                        if q.0 > vi {
                            q.0 -= 1;
                        }
                    }
                } else {
                    r.vias.remove(vi);
                }
            } else {
                vi += 1;
            }
        }
        // ORPHAN-STUB PRUNE (unspanned drop copper): the else-arm
        // above removes a swallowed via that carries no span
        // bookkeeping (fanout-first / victim-rip / routed-fallback
        // drops append raw copper), leaving its stub behind — pad-
        // anchored litter with NO plane contact that ships as the
        // "Track ↔ Via" unconnected pair AND blocks the fixpoint
        // from re-dropping the pad. Fragment the UNSPANNED TAIL
        // (indices past all span coverage, so span bookkeeping is
        // untouched) by endpoint adjacency; keep a fragment only if
        // it still reaches the plane: a surviving via, copper ON the
        // plane layer (stitch/fallback merging with the fill), or a
        // same-net THT barrel. Everything else is dead — removed, so
        // the pad reads un-served and the next iteration re-sites.
        let r = &mut final_routes[i];
        let covered = r
            .path_spans
            .iter()
            .map(|&(ps, pl)| ps + pl)
            .max()
            .unwrap_or(0);
        if r.segments.len() > covered {
            let pl_layer = board.nets[i].plane_layer.unwrap();
            let tail: Vec<usize> = (covered..r.segments.len()).collect();
            // Union-find fragments over shared endpoints (1µm tol).
            let n = tail.len();
            let mut comp: Vec<usize> = (0..n).collect();
            fn find(c: &mut Vec<usize>, a: usize) -> usize {
                let mut a = a;
                while c[a] != a {
                    c[a] = c[c[a]];
                    a = c[a];
                }
                a
            }
            let close = |a: (f64, f64), b: (f64, f64)| {
                (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
            };
            for x in 0..n {
                for y in x + 1..n {
                    let (sx, sy) = (&r.segments[tail[x]], &r.segments[tail[y]]);
                    if close(sx.start, sy.start)
                        || close(sx.start, sy.end)
                        || close(sx.end, sy.start)
                        || close(sx.end, sy.end)
                    {
                        let (ra, rb) = (find(&mut comp, x), find(&mut comp, y));
                        if ra != rb {
                            comp[ra] = rb;
                        }
                    }
                }
            }
            let mut root_alive = vec![false; n];
            let tht: Vec<(f64, f64)> = board
                .components
                .iter()
                .flat_map(|c2| {
                    let (ct, st) = (c2.theta.cos(), c2.theta.sin());
                    c2.pins
                        .iter()
                        .filter(|p| {
                            p.net == Some(board.nets[i].id)
                                && p.pad.as_ref().and_then(|pd| pd.drill_mm).is_some()
                        })
                        .map(move |p| {
                            (
                                c2.x + p.dx * ct - p.dy * st,
                                c2.y + p.dx * st + p.dy * ct,
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
            for x in 0..n {
                let sg = &r.segments[tail[x]];
                let alive = sg.layer == pl_layer
                    || r.vias.iter().any(|v| {
                        close(sg.start, (v.x, v.y)) || close(sg.end, (v.x, v.y))
                    })
                    || tht.iter().any(|&(tx, ty)| {
                        [sg.start, sg.end]
                            .iter()
                            .any(|e| (e.0 - tx).hypot(e.1 - ty) < 0.7)
                    });
                if alive {
                    let rx = find(&mut comp, x);
                    root_alive[rx] = true;
                }
            }
            let mut doomed: Vec<usize> = (0..n)
                .filter(|&x| {
                    let rx = find(&mut comp, x);
                    !root_alive[rx]
                })
                .map(|x| tail[x])
                .collect();
            if !doomed.is_empty() {
                log::info!(
                    "plane drop: pruned {} orphan stub segment(s) on '{}' (dead drop litter)",
                    doomed.len(),
                    board.nets[i].name
                );
                doomed.sort_unstable_by(|a, b| b.cmp(a));
                for di in doomed {
                    r.segments.remove(di);
                }
            }
        }
    }
    dropped
}

fn plane_via_drops(
    board: &Board,
    final_routes: &mut [Route],
    banned_sites: &[(f64, f64)],
) -> usize {
    let via_r = board.layer_stack.via.pad_mm / 2.0;
    let drill = board.layer_stack.via.drill_mm;
    let clearance = board.config.min_spacing_mm;
    let bw = board.config.outline.width();
    let bh = board.config.outline.height();
    let edge = board.config.edge_clearance_mm + via_r;
    let n_layers = board.layer_stack.layers.len();

    // Snapshot foreign-copper geometry once.
    struct PadObs {
        net: Option<NetId>,
        cx: f64,
        cy: f64,
        hx: f64,
        hy: f64,
        drill_r: f64,
    }
    let mut pads: Vec<PadObs> = Vec::new();
    for comp in &board.components {
        let cos_t = comp.theta.cos();
        let sin_t = comp.theta.sin();
        let quarter =
            ((comp.theta / std::f64::consts::FRAC_PI_2).round() as i64).rem_euclid(2);
        for pin in &comp.pins {
            if pin.unplaced {
                continue;
            }
            let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
            let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;
            let (pw, ph, dr) = match &pin.pad {
                Some(p) => (p.width_mm, p.height_mm, p.drill_mm.unwrap_or(0.0) / 2.0),
                None => (0.5, 0.5, 0.0), // matches exporter fallback
            };
            let (pw, ph) = if quarter == 1 { (ph, pw) } else { (pw, ph) };
            pads.push(PadObs {
                net: pin.net,
                cx: gx,
                cy: gy,
                hx: pw / 2.0,
                hy: ph / 2.0,
                drill_r: dr,
            });
        }
    }

    let mut new_vias: Vec<(f64, f64)> = Vec::new();
    let mut dropped = 0usize;
    for i in 0..board.nets.len() {
        let net = &board.nets[i];
        if net.plane_layer.is_none() {
            continue;
        }
        let comp_idx: crate::det::HashMap<ComponentId, usize> = board
            .components
            .iter()
            .enumerate()
            .map(|(k, c)| (c.id, k))
            .collect();
        let share = stackup::trace_width_for_current(
            stackup::current_for_trace_width(net.required_trace_width_mm)
                / net.pins.len().max(1) as f64,
            1.0,
            10.0,
        )
        .max(0.3)
        .min(net.required_trace_width_mm);
        // Merged fill punches for THIS net's plane: a drop sited inside
        // one has no plane contact (the swallow verifier would remove
        // it right after) — reject such sites during siting instead of
        // churning site-then-remove.
        let merged = output::kicad::merge_holes(output::kicad::plane_foreign_holes(
            board,
            final_routes,
            net.id,
        ));
        // Exact-clearance index for STUB legality (P1 kernel M1b —
        // replaces this file's second hand-rolled predicate copy).
        // Rebuilt per net: earlier nets' drops in this pass are
        // foreign copper the index must see.
        let cidx = geom::ClearanceIndex::build(board, final_routes, Some(net.id));
        for &(comp_id, pin_id) in &net.pins {
            let Some(&ci) = comp_idx.get(&comp_id) else { continue };
            let comp = &board.components[ci];
            let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pin_id) else {
                continue;
            };
            if pin.unplaced {
                continue;
            }
            let cos_t = comp.theta.cos();
            let sin_t = comp.theta.sin();
            let px = comp.x + pin.dx * cos_t - pin.dy * sin_t;
            let py = comp.y + pin.dx * sin_t + pin.dy * cos_t;
            if pin.pad.as_ref().and_then(|p| p.drill_mm).is_some() {
                // A through-hole barrel pierces the plane — but only
                // where its rail actually HAS copper. With a split-
                // plane REGION, a THT pad outside its rail's band
                // pierces bare dielectric (band fixture: a mixed-rail
                // header pin in the neighbor band shipped as zone-
                // unconnected); out-of-region THT pads take the
                // stub+via path like SMD pads.
                let in_region = match net.plane_region {
                    None => true,
                    Some((rx0, ry0, rx1, ry1)) => {
                        px > rx0 + 0.05
                            && px < rx1 - 0.05
                            && py > ry0 + 0.05
                            && py < ry1 - 0.05
                    }
                };
                if in_region {
                    continue;
                }
            }
            let stub_layer = match comp.side {
                BoardSide::Top => 0,
                BoardSide::Bottom => n_layers - 1,
            };
            // POUR-SIDE pads live ON the fill's layer: they are fill
            // anchors (contact merges them; island stitch bridges
            // stranding) — a stub+via here leaves the via's far end
            // on a bare signal layer, which ships as via_dangling
            // (the dbl_sided 2v family). Power/Ground plane layers
            // are inner and never a pad's own layer, so this only
            // fires for the signal-layer pour.
            if Some(stub_layer) == net.plane_layer {
                continue;
            }
            // Repair-pass guard: skip pads that still have a LIVE drop
            // (a stub on the pad's layer touching pad copper whose far
            // end carries a via). The validator can amputate a drop
            // that later copper made illegal — plane nets are excluded
            // from tree recovery, so a re-run of this pass is their
            // only repair path, and it must not double-drop the pads
            // that kept theirs.
            let half = pin
                .pad
                .as_ref()
                .map(|p| p.width_mm.min(p.height_mm) / 2.0)
                .unwrap_or(0.25);
            let has_live_drop = final_routes[i].segments.iter().any(|sg| {
                if sg.layer != stub_layer {
                    return false;
                }
                if segment_point_too_close(sg.start, sg.end, (px, py), sg.width_mm / 2.0 + half - 0.001) {
                    final_routes[i].vias.iter().any(|v| {
                        (v.x - sg.start.0).hypot(v.y - sg.start.1) < 0.01
                            || (v.x - sg.end.0).hypot(v.y - sg.end.1) < 0.01
                    })
                } else {
                    false
                }
            });
            if has_live_drop {
                if std::env::var("BHDL_PNR_PROBE").is_ok() && pin.name == "AVCC" {
                    log::info!(
                        "[probe] drop pass: pad AVCC ({px:.2},{py:.2}) net '{}' has_live_drop=true",
                        net.name
                    );
                }
                continue;
            }
            if std::env::var("BHDL_PNR_PROBE").is_ok() && pin.name == "AVCC" {
                log::info!(
                    "[probe] drop pass: pad AVCC ({px:.2},{py:.2}) net '{}' NEEDS drop",
                    net.name
                );
            }
            let region = net.plane_region;
            let site_ok = |x: f64, y: f64| -> bool {
                if x < edge || y < edge || x > bw - edge || y > bh - edge {
                    return false;
                }
                // Split-plane: the via must land INSIDE the rail's
                // region (plus via radius margin) or it has no plane.
                if let Some((rx0, ry0, rx1, ry1)) = region {
                    if x - via_r < rx0 + 0.05
                        || y - via_r < ry0 + 0.05
                        || x + via_r > rx1 - 0.05
                        || y + via_r > ry1 - 0.05
                    {
                        return false;
                    }
                }
                // FOREIGN plane fills: the barrel must be punchable —
                // a punch straddling a foreign fill boundary is never
                // punched (same rule as via_conflict / dijkstra).
                let punch = via_r + 0.35;
                for other in board.nets.iter().filter(|n| n.id != net.id) {
                    if other.plane_layer.is_none() {
                        continue;
                    }
                    let (zx0, zy0, zx1, zy1) = match other.plane_region {
                        Some((x0, y0, x1, y1)) => (
                            x0.max(edge),
                            y0.max(edge),
                            x1.min(bw - edge),
                            y1.min(bh - edge),
                        ),
                        None => (edge, edge, bw - edge, bh - edge),
                    };
                    let intersects = x > zx0 - punch
                        && x < zx1 + punch
                        && y > zy0 - punch
                        && y < zy1 + punch;
                    let interior = x > zx0 + punch
                        && x < zx1 - punch
                        && y > zy0 + punch
                        && y < zy1 - punch;
                    if intersects && !interior {
                        return false;
                    }
                }
                if output::kicad::plane_swallows(board, &merged, x, y, via_r, region) {
                    return false;
                }
                // Sites whose drop a previous fixpoint iteration removed
                // (swallow-verified against the POST-drop hole set, which
                // siting cannot fully anticipate) — ban a pitch-sized disc
                // so the retry actually moves.
                if banned_sites.iter().any(|&(bx, by)| (x - bx).hypot(y - by) < 0.26) {
                    return false;
                }
                // ONE-TRUTH barrel gate: the PadObs box test below is
                // a snapshot approximation that shipped a via 0.375mm
                // from a foreign pad (uno s42 during nudge trials) —
                // the exact kernel's via_conflict (roundrect pads,
                // current copper) is authoritative. The index is
                // bucketed, so this is a local query, cheap even in
                // the routed fallback's sink enumeration.
                if cidx.via_conflict(x, y, via_r, net.id).is_some() {
                    return false;
                }
                for p in &pads {
                    let same = p.net == Some(net.id);
                    if !same {
                        let m = (via_r + clearance).max(drill / 2.0 + 0.25);
                        if (x - p.cx).abs() < p.hx + m && (y - p.cy).abs() < p.hy + m {
                            return false;
                        }
                    }
                    if p.drill_r > 0.0
                        && (x - p.cx).hypot(y - p.cy)
                            < (p.drill_r + drill / 2.0 + 0.25)
                                .max(if p.net == Some(net.id) {
                                    0.0
                                } else {
                                    // foreign barrel punch + own punch
                                    (p.hx.max(p.hy) + 0.35) + (via_r + 0.35) + 0.1
                                })
                    {
                        return false;
                    }
                }
                // Keep plane drops a full PUNCH DIAMETER away from
                // foreign barrels: the fill punches holes around
                // foreign barrels (radius ~via_r + zone clearance), and
                // overlapping punches MERGE into bigger circles — a
                // drop sited inside a merged punch loses its plane
                // contact (shipped as via_dangling).
                let punch_gap = 2.0 * (via_r + 0.35) + 0.15;
                for r in final_routes.iter() {
                    for sg in &r.segments {
                        let m = via_r + sg.width_mm / 2.0 + clearance;
                        if segment_point_too_close(sg.start, sg.end, (x, y), m) {
                            return false;
                        }
                    }
                    for v in &r.vias {
                        if (x - v.x).hypot(y - v.y) < punch_gap {
                            return false;
                        }
                    }
                }
                for &(vx, vy) in &new_vias {
                    // Same-net drops don't punch each other, but drops
                    // of DIFFERENT plane nets do — keep the full gap.
                    if (x - vx).hypot(y - vy) < punch_gap {
                        return false;
                    }
                }
                // The STUB pad→via must also be legal, not just the
                // via site: the repair pass can need multi-mm stubs
                // (region projection, ring 10), and an unvalidated
                // stub plows straight through whatever recovery routed
                // in between (shipped as shorting_items on the fpga
                // board). One exact-geometry query (P1 kernel).
                if cidx
                    .first_conflict((px, py), (x, y), share, stub_layer, net.id)
                    .is_some()
                {
                    return false;
                }
                true
            };
            // Ring search around the pad; for split planes ALSO around
            // the pad's projection into the region (a pad outside its
            // rail's band needs a longer stub to a legal site).
            let mut anchors: Vec<(f64, f64)> = vec![(px, py)];
            if let Some((rx0, ry0, rx1, ry1)) = region {
                let qx = px.clamp(rx0 + via_r + 0.1, rx1 - via_r - 0.1);
                let qy = py.clamp(ry0 + via_r + 0.1, ry1 - via_r - 0.1);
                if (qx - px).abs() > 1e-9 || (qy - py).abs() > 1e-9 {
                    anchors.push((qx, qy));
                }
            }
            let mut placed_at: Option<(f64, f64)> = None;
            'rings: for ring in 0..10 {
                let r = 0.6 + ring as f64 * 0.35;
                for &(ax, ay) in &anchors {
                    for k in 0..8 {
                        let ang = k as f64 * std::f64::consts::FRAC_PI_4;
                        let (x, y) = (ax + r * ang.cos(), ay + r * ang.sin());
                        if site_ok(x, y) {
                            placed_at = Some((x, y));
                            break 'rings;
                        }
                    }
                }
            }
            // Straight-stub siting failed: fall back to a ROUTED drop —
            // dijkstra on the pad's layer to any cell passing site_ok,
            // on a grid where all committed copper is blocked. This is
            // the post-recovery repair path (the original drop's site
            // became illegal and the neighborhood is now congested).
            let routed: Option<(Vec<RouteSegment>, (f64, f64))> =
                if placed_at.is_none() {
                    let mut fb_grid = routing::grid::RoutingGrid::build(board);
                    for r in final_routes.iter() {
                        if !r.is_empty() {
                            pathfinder::block_route_geometry(&mut fb_grid, r, board);
                        }
                    }
                    pathfinder::routed_plane_drop(
                        &fb_grid,
                        net,
                        board,
                        (px, py),
                        (comp.x, comp.y),
                        stub_layer,
                        share,
                        &site_ok,
                    )
                } else {
                    None
                };
            let (stub_segs, vx, vy) = match (placed_at, routed) {
                (Some((vx, vy)), _) => (
                    vec![RouteSegment {
                        layer: stub_layer,
                        start: (px, py),
                        end: (vx, vy),
                        width_mm: share,
                    }],
                    vx,
                    vy,
                ),
                (None, Some((segs, (vx, vy)))) => {
                    log::info!(
                        "plane via drop: routed fallback for pad '{}' of '{}' \
                         (net '{}') — {} segment(s) to ({vx:.2},{vy:.2})",
                        pin.name, comp.refdes, net.name, segs.len()
                    );
                    (segs, vx, vy)
                }
                (None, None) => {
                    // Terminal rung — DROP-SITE VICTIM RIP: every site
                    // in reach is blocked, but some only by SIGNAL
                    // tracks (uno free-MCU: UGND boxed in by its own
                    // VCC ring on F.Cu while two inner-layer signals
                    // crossed under the box and killed every barrel
                    // site). Rip the signal(s), place the drop, rebuild
                    // them on the updated board; accept only when no
                    // victim ends worse.
                    if drop_site_victim_rip(
                        board,
                        final_routes,
                        i,
                        (px, py),
                        stub_layer,
                        share,
                        &mut new_vias,
                    ) {
                        dropped += 1;
                        continue;
                    }
                    log::warn!(
                        "plane via drop: no legal site near pad '{}' of '{}' (net '{}') — pad stays unconnected",
                        pin.name, comp.refdes, net.name
                    );
                    continue;
                }
            };
            // EXACT COMMIT GATE: the straight-stub site test only
            // checks pad->via clearance; the ROUTED fallback's dijkstra
            // uses the grid, blind to sub-grid pad corners (a diagonal
            // drop stub grazed a TQFP pad at 0.138mm vs 0.15 — oracle
            // clearance). Re-check every stub segment against the exact
            // index before committing; illegal drops stay honestly
            // unconnected.
            {
                let idx = geom::ClearanceIndex::build(board, final_routes, Some(net.id));
                if stub_segs.iter().any(|sg| {
                    idx.first_conflict(sg.start, sg.end, sg.width_mm, sg.layer, net.id)
                        .is_some()
                }) {
                    continue;
                }
                // The VIA BARREL too — a siting path handed this
                // commit a coordinate site_ok never saw (measured: a
                // via 0.375mm from a foreign pad shipped through the
                // routed fallback while the same call's site_ok
                // rejected that exact spot). The commit gate is the
                // one chokepoint every drop passes.
                if idx.via_conflict(vx, vy, via_r, net.id).is_some() {
                    continue;
                }
            }
            let route = &mut final_routes[i];
            let seg_start = route.segments.len();
            let via_start = route.vias.len();
            let n_segs = stub_segs.len();
            route.segments.extend(stub_segs);
            route.vias.push(RouteVia {
                x: vx,
                y: vy,
                from_layer: 0,
                to_layer: n_layers - 1,
            });
            route.path_spans.push((seg_start, n_segs));
            route.path_parents.push(None);
            route.via_spans.push((via_start, 1));
            new_vias.push((vx, vy));
            dropped += 1;
        }
    }
    dropped
}

/// Terminal rung of the drop ladder (see the call site in
/// plane_via_drops): scan ring sites again, classifying blockers —
/// a site blocked ONLY by 1-2 signal nets' tracks is a rip
/// candidate. Rip them, verify the site on the exact kernel, commit
/// the drop, rebuild the victims on the updated board; revert
/// everything unless no victim ends with more unreached sinks than
/// it started with.
fn drop_site_victim_rip(
    board: &Board,
    final_routes: &mut [Route],
    i: usize,
    (px, py): (f64, f64),
    stub_layer: usize,
    share: f64,
    new_vias: &mut Vec<(f64, f64)>,
) -> bool {
    let net_id = board.nets[i].id;
    let via_r = board.layer_stack.via.pad_mm / 2.0;
    let drill = board.layer_stack.via.drill_mm;
    let clearance = board.config.min_spacing_mm;
    let region = board.nets[i].plane_region;
    let n_layers = board.layer_stack.layers.len();
    let punch_gap = 2.0 * (via_r + 0.35) + 0.15;
    let bw = board.config.outline.width();
    let bh = board.config.outline.height();
    let edge = board.config.edge_clearance_mm + via_r;
    struct PadObs {
        net: Option<NetId>,
        cx: f64,
        cy: f64,
        hx: f64,
        hy: f64,
        drill_r: f64,
    }
    let mut pads: Vec<PadObs> = Vec::new();
    for comp in &board.components {
        let cos_t = comp.theta.cos();
        let sin_t = comp.theta.sin();
        let quarter =
            ((comp.theta / std::f64::consts::FRAC_PI_2).round() as i64).rem_euclid(2);
        for pin in &comp.pins {
            if pin.unplaced {
                continue;
            }
            let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
            let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;
            let (pw, ph, dr) = match &pin.pad {
                Some(p) => (p.width_mm, p.height_mm, p.drill_mm.unwrap_or(0.0) / 2.0),
                None => (0.5, 0.5, 0.0),
            };
            let (pw, ph) = if quarter == 1 { (ph, pw) } else { (pw, ph) };
            pads.push(PadObs {
                net: pin.net,
                cx: gx,
                cy: gy,
                hx: pw / 2.0,
                hy: ph / 2.0,
                drill_r: dr,
            });
        }
    }
    let merged = output::kicad::merge_holes(output::kicad::plane_foreign_holes(
        board,
        final_routes,
        net_id,
    ));
    // (site, soft victims, ring radius) — soft = every conflict is a
    // non-plane track; anything touching pads, vias, plane copper, or
    // our own net stays hard.
    let mut cands: Vec<((f64, f64), Vec<usize>, f64)> = Vec::new();
    for ring in 0..10 {
        let r = 0.6 + ring as f64 * 0.35;
        for k in 0..8 {
            let ang = k as f64 * std::f64::consts::FRAC_PI_4;
            let (x, y) = (px + r * ang.cos(), py + r * ang.sin());
            if x < edge || y < edge || x > bw - edge || y > bh - edge {
                continue;
            }
            if let Some((rx0, ry0, rx1, ry1)) = region {
                if x < rx0 + via_r + 0.1
                    || x > rx1 - via_r - 0.1
                    || y < ry0 + via_r + 0.1
                    || y > ry1 - via_r - 0.1
                {
                    continue;
                }
            }
            if output::kicad::plane_swallows(board, &merged, x, y, via_r, region) {
                if std::env::var("BHDL_DROP_DEBUG").is_ok() {
                    log::info!("drop-rip cand ({x:.2},{y:.2}) r={r:.2}: plane-swallowed");
                }
                continue;
            }
            let mut hard = false;
            for p in &pads {
                let same = p.net == Some(net_id);
                if !same {
                    let m = (via_r + clearance).max(drill / 2.0 + 0.25);
                    if (x - p.cx).abs() < p.hx + m && (y - p.cy).abs() < p.hy + m {
                        hard = true;
                        break;
                    }
                }
                if p.drill_r > 0.0
                    && (x - p.cx).hypot(y - p.cy)
                        < (p.drill_r + drill / 2.0 + 0.25).max(if same {
                            0.0
                        } else {
                            (p.hx.max(p.hy) + 0.35) + (via_r + 0.35) + 0.1
                        })
                {
                    hard = true;
                    break;
                }
            }
            if hard {
                if std::env::var("BHDL_DROP_DEBUG").is_ok() {
                    log::info!("drop-rip cand ({x:.2},{y:.2}) r={r:.2}: hard by PAD");
                }
                continue;
            }
            let mut soft: Vec<usize> = Vec::new();
            'routes: for (j, r_) in final_routes.iter().enumerate() {
                for v in &r_.vias {
                    if (x - v.x).hypot(y - v.y) < punch_gap {
                        // SIGNAL vias are rippable too: ripping net j
                        // takes its via along, and the rebuild may
                        // dive elsewhere. Escape vias wall a ~1.4mm
                        // swath each — with them hard, a mid-row QFP
                        // ground pad's whole corridor can be untriable
                        // (uno s99 UGND: every ring site sat inside
                        // some escape via's punch gap). Plane nets and
                        // our own drops stay hard.
                        if j == i || board.nets[j].plane_layer.is_some() {
                            hard = true;
                            break 'routes;
                        }
                        if !soft.contains(&j) {
                            soft.push(j);
                        }
                        continue;
                    }
                }
                for sg in &r_.segments {
                    let m = via_r + sg.width_mm / 2.0 + clearance;
                    if geom::segment_point_too_close(sg.start, sg.end, (x, y), m) {
                        if j == i || board.nets[j].plane_layer.is_some() {
                            hard = true;
                            break 'routes;
                        }
                        if !soft.contains(&j) {
                            soft.push(j);
                        }
                    }
                }
            }
            // STUB victims: the pad→site stub crosses copper the SITE
            // scan never sees — this is exactly why a mid-row QFP
            // ground pad stayed walled while 174 "free" sites (soft
            // empty, hard false) were being skipped: every free site
            // was unreachable THROUGH the escape field. Foreign pads
            // on the stub are unrippable (hard); signal tracks and
            // vias join the victim list like site conflicts do.
            if !hard {
                for p in &pads {
                    if p.net == Some(net_id) {
                        continue;
                    }
                    if geom::segment_rect_dist(
                        (px, py),
                        (x, y),
                        p.cx - p.hx,
                        p.cy - p.hy,
                        p.cx + p.hx,
                        p.cy + p.hy,
                    ) < share / 2.0 + clearance
                    {
                        hard = true;
                        break;
                    }
                }
            }
            if !hard {
                'stub: for (j, r_) in final_routes.iter().enumerate() {
                    if j == i {
                        // Same-net copper on the stub is a JOIN, not a
                        // conflict (the commit gate excludes own-net).
                        continue;
                    }
                    for sg in &r_.segments {
                        if sg.layer != stub_layer {
                            continue;
                        }
                        let m = share / 2.0 + sg.width_mm / 2.0 + clearance;
                        if geom::segments_too_close((px, py), (x, y), sg.start, sg.end, m)
                        {
                            if board.nets[j].plane_layer.is_some() {
                                hard = true;
                                break 'stub;
                            }
                            if !soft.contains(&j) {
                                soft.push(j);
                            }
                        }
                    }
                    for v in &r_.vias {
                        let m = share / 2.0 + via_r + clearance;
                        if geom::point_segment_dist((v.x, v.y), (px, py), (x, y)) < m {
                            if board.nets[j].plane_layer.is_some() {
                                hard = true;
                                break 'stub;
                            }
                            if !soft.contains(&j) {
                                soft.push(j);
                            }
                        }
                    }
                }
            }
            if std::env::var("BHDL_DROP_DEBUG").is_ok() {
                log::info!(
                    "drop-rip cand ({x:.2},{y:.2}) r={r:.2}: hard={hard} soft={:?}",
                    soft.iter().map(|&j| board.nets[j].name.clone()).collect::<Vec<_>>()
                );
            }
            if hard || soft.is_empty() || soft.len() > 2 {
                continue;
            }
            cands.push(((x, y), soft, r));
        }
    }
    // Rebuild risk is proportional to the COPPER being ripped, not
    // the victim count: one crossing of the VCC rail (hundreds of
    // segments, whole-net rebuild always strands a sink) is a far
    // worse bet than two 2-pin signal stubs — sorting by count alone
    // burned all attempts on the rail while the winnable candidates
    // sat just past the cutoff.
    cands.sort_by(|a, b| {
        let cost = |v: &Vec<usize>| -> usize {
            v.iter().map(|&j| final_routes[j].segments.len()).sum()
        };
        (cost(&a.1), a.1.len(), a.2)
            .partial_cmp(&(cost(&b.1), b.1.len(), b.2))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    'site: for (site, victims, _) in cands.into_iter().take(8) {
        let (vx, vy) = site;
        if std::env::var("BHDL_DROP_DEBUG").is_ok() {
            log::info!(
                "drop-rip TRY ({vx:.2},{vy:.2}) for pad ({px:.2},{py:.2}) victims={:?}",
                victims.iter().map(|&j| board.nets[j].name.clone()).collect::<Vec<_>>()
            );
        }
        let snaps: Vec<(usize, Route)> = victims
            .iter()
            .map(|&vj| (vj, final_routes[vj].clone()))
            .collect();
        for &vj in &victims {
            final_routes[vj] = Route::empty(final_routes[vj].net_id);
        }
        let drop_snap = final_routes[i].clone();
        {
            let idx = geom::ClearanceIndex::build(board, final_routes, Some(net_id));
            let vc = idx.via_conflict(vx, vy, via_r, net_id);
            let sc = idx.first_conflict((px, py), (vx, vy), share, stub_layer, net_id);
            if vc.is_some() || sc.is_some() {
                if std::env::var("BHDL_DROP_DEBUG").is_ok() {
                    log::info!(
                        "drop-rip TRY ({vx:.2},{vy:.2}): COMMIT-GATE reject (via_conflict={} stub_conflict={})",
                        vc.is_some(), sc.is_some()
                    );
                }
                for (vj, old) in snaps {
                    final_routes[vj] = old;
                }
                continue 'site;
            }
        }
        {
            let route = &mut final_routes[i];
            let seg_start = route.segments.len();
            let via_start = route.vias.len();
            route.segments.push(RouteSegment {
                layer: stub_layer,
                start: (px, py),
                end: (vx, vy),
                width_mm: share,
            });
            route.path_spans.push((seg_start, 1));
            route.path_parents.push(None);
            route.vias.push(RouteVia {
                x: vx,
                y: vy,
                from_layer: 0,
                to_layer: n_layers - 1,
            });
            route.via_spans.push((via_start, 1));
        }
        let mut all_ok = true;
        for (k, &vj) in victims.iter().enumerate() {
            let before =
                pathfinder::unreached_sink_count(&board.nets[vj], board, &snaps[k].1);
            let mut jgrid = RoutingGrid::build(board);
            for (m, r_) in final_routes.iter().enumerate() {
                if m != vj && !r_.is_empty() {
                    pathfinder::block_route_geometry(&mut jgrid, r_, board);
                }
            }
            let mut fresh = Route::empty(board.nets[vj].id);
            pathfinder::extend_route(
                &mut jgrid, &board.nets[vj], board, &mut fresh, 1.0, 1.0, &[], &[],
                false, None,
            );
            {
                let mut bans = Vec::new();
                exact_commit_strip(board, final_routes, vj, &mut fresh, 0, &mut bans);
            }
            final_routes[vj] = fresh;
            let after = pathfinder::unreached_sink_count(
                &board.nets[vj],
                board,
                &final_routes[vj],
            );
            if after > before {
                if std::env::var("BHDL_DROP_DEBUG").is_ok() {
                    log::info!(
                        "drop-rip TRY ({vx:.2},{vy:.2}): victim '{}' rebuild WORSE ({before} -> {after})",
                        board.nets[vj].name
                    );
                }
                all_ok = false;
                break;
            }
        }
        if all_ok {
            new_vias.push((vx, vy));
            log::info!(
                "plane via drop: victim-rip sited a '{}' drop at ({vx:.2},{vy:.2}) after ripping {} signal net(s)",
                board.nets[i].name,
                victims.len()
            );
            return true;
        }
        final_routes[i] = drop_snap;
        for (vj, old) in snaps {
            final_routes[vj] = old;
        }
    }
    false
}

fn validate_and_rip(
    board: &Board,
    final_routes: &mut [Route],
    banned_vias: &mut Vec<(usize, (f64, f64))>,
    banned_dangles: &mut Vec<(usize, (f64, f64))>,
) -> Vec<usize> {
    let mut ripped_nets: Vec<usize> = Vec::new();

        let clearance = board.config.min_spacing_mm;

        // Foreign-copper obstacles per layer: every pad's rotated rect
        // (real P0 geometry) tagged with its net — a track must keep
        // clearance from every pad that is not its own net. This was
        // the validator's blind spot on dense boards (~200
        // shorting_items per board, all track-vs-PAD).
        struct PadRect {
            net: Option<NetId>,
            layer_top: bool,
            layer_bot: bool,
            cx: f64,
            cy: f64,
            hx: f64,
            hy: f64,
            drill_r: f64,
            corner_r: f64,
        }
        let mut pad_rects: Vec<PadRect> = Vec::new();
        let n_layers = board.layer_stack.layers.len();
        for comp in &board.components {
            let cos_t = comp.theta.cos();
            let sin_t = comp.theta.sin();
            let quarter =
                ((comp.theta / std::f64::consts::FRAC_PI_2).round() as i64).rem_euclid(2);
            for pin in &comp.pins {
                if pin.unplaced {
                    continue; // no copper emitted for this pin
                }
                let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
                let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;
                let (pw, ph, thru, drill_r, corner_r) = match &pin.pad {
                    Some(p) => {
                        let m = p.width_mm.min(p.height_mm);
                        // Same shape rule the exporter emits.
                        let r = match p.shape {
                            crate::types::PadShapeKind::RoundRect => 0.25 * m,
                            crate::types::PadShapeKind::Oval
                            | crate::types::PadShapeKind::Circle => m / 2.0,
                            crate::types::PadShapeKind::Rect => 0.0,
                        };
                        (
                            p.width_mm,
                            p.height_mm,
                            p.drill_mm.is_some(),
                            p.drill_mm.unwrap_or(0.0) / 2.0,
                            r,
                        )
                    }
                    // 0.5 matches the EXPORTER's fallback pad — the validator
                    // modeling 0.8 while the file ships 0.5 let stubs pass
                    // as pad-anchored that KiCad sees dangling.
                    None => (0.5, 0.5, false, 0.0, 0.0),
                };
                let (pw, ph) = if quarter == 1 { (ph, pw) } else { (pw, ph) };
                let on_top = thru || matches!(comp.side, BoardSide::Top);
                let on_bot = thru || matches!(comp.side, BoardSide::Bottom);
                pad_rects.push(PadRect {
                    net: pin.net,
                    layer_top: on_top,
                    layer_bot: on_bot,
                    cx: gx,
                    cy: gy,
                    hx: pw / 2.0,
                    hy: ph / 2.0,
                    drill_r,
                    corner_r,
                });
            }
        }
        let pad_on_layer = |p: &PadRect, layer: usize| -> bool {
            (layer == 0 && p.layer_top) || (layer == n_layers - 1 && p.layer_bot)
        };
        // Exact segment-to-pad distance on the EXPORTED shape: a
        // roundrect/oval is the Minkowski sum of the corner-inset rect
        // and a disc, so distance = dist(seg, inset rect) − corner_r.
        // (Replaces a 9-sample box test that was stricter than KiCad
        // at corners and could miss grazes between samples.)
        let seg_hits_rect = |a: (f64, f64), b: (f64, f64), p: &PadRect, gap: f64| -> bool {
            let rc = p.corner_r.min(p.hx).min(p.hy);
            // Inset half-extents once, so the rect can never invert by
            // 1 ulp when rc == hx/hy (same epsilon as the clamp sites).
            let dx = (p.hx - rc).max(0.0);
            let dy = (p.hy - rc).max(0.0);
            geom::segment_rect_dist(
                a,
                b,
                p.cx - dx,
                p.cy - dy,
                p.cx + dx,
                p.cy + dy,
            ) - rc
                < gap - 1e-6
        };

        let mut ripped = 0usize;
        let mut whole_rip_mode = false;
        loop {
            let mut offender: Option<(usize, Option<usize>)> = None;
            // (net, span index, trim_from_end, segment count) — dangling
            // tails are TRIMMED back to the last junction, not amputated:
            // a 0.15mm tail on a trunk span must not take 17 branches
            // down with it.
            let mut trim: Option<(usize, usize, bool, usize)> = None;
            'scan: for i in 0..final_routes.len() {
                if final_routes[i].is_empty() {
                    continue;
                }
                let net_id = final_routes[i].net_id;
                // Track vs foreign PAD.
                for (si, sa) in final_routes[i].segments.iter().enumerate() {
                    let gap = sa.width_mm / 2.0 + clearance;
                    for p in &pad_rects {
                        if p.net == Some(net_id) || !pad_on_layer(p, sa.layer) {
                            continue;
                        }
                        if seg_hits_rect(sa.start, sa.end, p, gap) {
                            log::debug!(
                                "validator: track-vs-pad offender net '{}' seg {si} \
                                 ({:.2},{:.2})-({:.2},{:.2}) w={:.2} vs pad at ({:.2},{:.2}) hx={:.2} hy={:.2}",
                                board.nets[i].name, sa.start.0, sa.start.1, sa.end.0, sa.end.1,
                                sa.width_mm, p.cx, p.cy, p.hx, p.hy
                            );
                            // Ban the offending copper's site: grid-
                            // pitch quantization lets extensions place
                            // this exact segment again and again — the
                            // amputate/rebuild ping-pong never converges
                            // without marking the spot.
                            banned_dangles.push((
                                i,
                                (
                                    (sa.start.0 + sa.end.0) / 2.0,
                                    (sa.start.1 + sa.end.1) / 2.0,
                                ),
                            ));
                            offender = Some((i, Some(si)));
                            break 'scan;
                        }
                    }
                }
                // Copper vs board edge: the edge band in the grid has
                // owner exceptions (pads may legally sit near the edge),
                // and recovery extensions can thread through them — the
                // oracle reports copper_edge_clearance. Board outline is
                // the (0,0)..(width,height) rect.
                {
                    let edge = board.config.edge_clearance_mm;
                    let bw = board.config.outline.width();
                    let bh = board.config.outline.height();
                    let poly = match &board.config.outline {
                        BoardOutline::Polygon(pts) => Some(pts.clone()),
                        _ => None,
                    };
                    let edge_bad = |p: (f64, f64), m: f64| -> bool {
                        if let Some(pts) = &poly {
                            !board.config.outline.contains(p.0, p.1)
                                || crate::routing::grid::polygon_edge_distance(
                                    pts, p.0, p.1,
                                ) < m
                        } else {
                            p.0 < m || p.1 < m || p.0 > bw - m || p.1 > bh - m
                        }
                    };
                    let mut hit: Option<usize> = None;
                    if bw > 0.0 && bh > 0.0 {
                        for (si, sg) in final_routes[i].segments.iter().enumerate() {
                            let m = edge + sg.width_mm / 2.0;
                            if [sg.start, sg.end].iter().any(|p| edge_bad(*p, m)) {
                                hit = Some(si);
                                break;
                            }
                            // Interior cutouts, SEGMENT-exact: the
                            // grid band gates cell centers, but a
                            // mitered diagonal sweeping a cutout
                            // CORNER passes the center check while
                            // its copper dips inside the band
                            // (oracle copper_edge_clearance vs the
                            // Edge.Cuts rect on test_poly_dense).
                            if board.config.cutouts.iter().any(|&(x0, y0, x1, y1)| {
                                geom::segment_rect_dist(sg.start, sg.end, x0, y0, x1, y1)
                                    < m
                            }) {
                                hit = Some(si);
                                break;
                            }
                        }
                    }
                    if hit.is_none() && bw > 0.0 && bh > 0.0 {
                        let vm = edge + board.layer_stack.via.pad_mm / 2.0;
                        if let Some(vi) = final_routes[i].vias.iter().position(|v| {
                            edge_bad((v.x, v.y), vm)
                        }) {
                            let r = &final_routes[i];
                            hit = r
                                .via_spans
                                .iter()
                                .position(|&(vs, vl)| vl > 0 && vi >= vs && vi < vs + vl)
                                .and_then(|sp| r.path_spans.get(sp).map(|&(ps, _)| ps));
                            if hit.is_none() {
                                hit = Some(0);
                            }
                        }
                    }
                    if let Some(si) = hit {
                        log::debug!(
                            "validator: copper too close to board edge on net '{}'",
                            board.nets[i].name
                        );
                        offender = Some((i, Some(si)));
                        break 'scan;
                    }
                }
                // Dangling track ends: an endpoint must touch same-net
                // copper — another segment (incl. mid-segment T-joints),
                // a via, or a pad of this net. The oracle reports
                // orphans as track_dangling; amputation bookkeeping can
                // miss cases, so the validator is the guarantee.
                {
                    let rt = &final_routes[i];
                    let mut dangle: Option<(usize, (f64, f64))> = None;
                    'ends: for (si, sa) in rt.segments.iter().enumerate() {
                        for pt in [sa.start, sa.end] {
                            // KiCad's track_dangling is ENDPOINT-GRAPH
                            // semantics: lateral width-overlap does NOT
                            // rescue a stray end (self-copper of the
                            // merged run never counts). An endpoint is
                            // anchored only if it lies ON another
                            // segment (co-endpoint or T-interior),
                            // INSIDE a same-net pad, or ON a via.
                            let own_half = sa.width_mm / 2.0;
                            let _ = own_half;
                            let via_r = board.layer_stack.via.pad_mm / 2.0;
                            let mut touched = rt.segments.iter().enumerate().any(|(sj, sb)| {
                                sj != si
                                    && sb.layer == sa.layer
                                    && segment_point_too_close(sb.start, sb.end, pt, 0.01)
                            });
                            if !touched {
                                touched = rt.vias.iter().any(|v| {
                                    (v.x - pt.0).hypot(v.y - pt.1) < via_r + 0.01
                                });
                            }
                            if !touched {
                                touched = pad_rects.iter().any(|p| {
                                    p.net == Some(net_id)
                                        && pad_on_layer(p, sa.layer)
                                        && (pt.0 - p.cx).abs() < p.hx + 0.01
                                        && (pt.1 - p.cy).abs() < p.hy + 0.01
                                });
                            }
                            if !touched {
                                dangle = Some((si, pt));
                                break 'ends;
                            }
                        }
                    }
                    if let Some((si, pt)) = dangle {
                        log::debug!(
                            "validator: dangling track end on net '{}' at ({:.2},{:.2})",
                            board.nets[i].name, pt.0, pt.1
                        );
                        // Trim the tail: walk inward from the dangling
                        // end until the exposed endpoint touches other
                        // copper (junction, via, or pad). Only if the
                        // WHOLE span is tail do we fall back to subtree
                        // amputation (and ban the site so extension
                        // recovery doesn't rebuild the same dangler).
                        let span_idx = rt
                            .path_spans
                            .iter()
                            .position(|&(ps, pl)| pl > 0 && si >= ps && si < ps + pl);
                        let mut planned: Option<(usize, bool, usize)> = None;
                        if let Some(sp) = span_idx {
                            let (ps, pl) = rt.path_spans[sp];
                            let at_end = si == ps + pl - 1
                                && (rt.segments[si].end.0 - pt.0).abs() < 1e-6
                                && (rt.segments[si].end.1 - pt.1).abs() < 1e-6;
                            let at_start = si == ps
                                && (rt.segments[si].start.0 - pt.0).abs() < 1e-6
                                && (rt.segments[si].start.1 - pt.1).abs() < 1e-6;
                            if at_end || at_start {
                                let own_half = rt.segments[si].width_mm / 2.0;
                                let via_r = board.layer_stack.via.pad_mm / 2.0;
                                let touched_at =
                                    |q: (f64, f64), layer: usize, removed: &dyn Fn(usize) -> bool| {
                                        rt.segments.iter().enumerate().any(|(sj, sb)| {
                                            !removed(sj)
                                                && sb.layer == layer
                                                && segment_point_too_close(
                                                    sb.start,
                                                    sb.end,
                                                    q,
                                                    sb.width_mm / 2.0 + own_half - 0.001,
                                                )
                                        }) || rt.vias.iter().any(|v| {
                                            (v.x - q.0).hypot(v.y - q.1)
                                                < via_r + own_half - 0.001
                                        }) || pad_rects.iter().any(|p| {
                                            p.net == Some(net_id)
                                                && pad_on_layer(p, layer)
                                                && (q.0 - p.cx).abs() < p.hx + own_half - 0.001
                                                && (q.1 - p.cy).abs() < p.hy + own_half - 0.001
                                        })
                                    };
                                let mut count = 0usize;
                                while count < pl {
                                    count += 1;
                                    if count == pl {
                                        break; // whole span is tail
                                    }
                                    let (q, layer, removed_from): (_, _, usize) = if at_end {
                                        let k = ps + pl - count;
                                        (rt.segments[k].start, rt.segments[k].layer, k)
                                    } else {
                                        let k = ps + count - 1;
                                        (rt.segments[k].end, rt.segments[k].layer, k)
                                    };
                                    let removed = |sj: usize| {
                                        if at_end {
                                            sj >= removed_from
                                        } else {
                                            sj < ps + count
                                        }
                                    };
                                    if touched_at(q, layer, &removed) {
                                        break;
                                    }
                                }
                                if count < pl {
                                    planned = Some((sp, at_end, count));
                                }
                            }
                        }
                        match planned {
                            Some((sp, at_end, count)) => {
                                trim = Some((i, sp, at_end, count));
                            }
                            None => {
                                banned_dangles.push((i, pt));
                                offender = Some((i, Some(si)));
                            }
                        }
                        break 'scan;
                    }
                }
                // VIA vs foreign copper: the barrel needs via_pad/2 +
                // clearance; the hole needs drill/2 + the board hole
                // clearance (KiCad default 0.25). A via touches every
                // layer, so any foreign pad or track qualifies. The
                // offender maps to the via's span so amputation takes
                // the branch (and its vias) together.
                let via_r = board.layer_stack.via.pad_mm / 2.0;
                let hole_margin = board.layer_stack.via.drill_mm / 2.0 + 0.25;
                let via_span_seg = |r: &Route, vi: usize| -> Option<usize> {
                    r.via_spans
                        .iter()
                        .position(|&(vs, vl)| vl > 0 && vi >= vs && vi < vs + vl)
                        .and_then(|s| r.path_spans.get(s).map(|&(ps, _)| ps))
                };
                for (vi, v) in final_routes[i].vias.iter().enumerate() {
                    let pad_margin = (via_r + clearance).max(hole_margin);
                    let mut bad = false;
                    let mut why = "";
                    // Same-route drilled-hole spacing: two of this net's
                    // own vias too close is hole_to_hole just the same.
                    let hole_gap = board.layer_stack.via.drill_mm + 0.25;
                    for (vj, vb) in final_routes[i].vias.iter().enumerate() {
                        if vj < vi && (v.x - vb.x).hypot(v.y - vb.y) < hole_gap {
                            bad = true;
                            why = "same-net-hole";
                            break;
                        }
                    }
                    for p in &pad_rects {
                        // hole_to_hole is a DRILL rule and binds
                        // SAME-NET pairs too (a GND drop via beside a
                        // GND header pin hole) — check before the
                        // same-net skip.
                        if p.drill_r > 0.0
                            && (v.x - p.cx).hypot(v.y - p.cy)
                                < p.drill_r + board.layer_stack.via.drill_mm / 2.0 + 0.25
                        {
                            bad = true;
                            why = "tht-hole";
                            break;
                        }
                        if p.net == Some(net_id) {
                            continue;
                        }
                        {
                            // Exact roundrect distance (inset rect +
                            // disc) — the old Chebyshev box was
                            // stricter than KiCad at pad corners and
                            // amputated gate-legal cross-under vias in
                            // an endless re-commit loop.
                            let rc = p.corner_r.min(p.hx).min(p.hy);
                            // Inset half-extents computed ONCE: writing the
                            // bounds as (cx-hx)+rc / (cx+hx)-rc differs by
                            // 1 ulp when rc == hx (fine-pitch pads whose
                            // corner radius saturates) and clamp panics on
                            // min > max by ~1e-15.
                            let dx = (p.hx - rc).max(0.0);
                            let dy = (p.hy - rc).max(0.0);
                            let nx = v.x.clamp(p.cx - dx, p.cx + dx);
                            let ny = v.y.clamp(p.cy - dy, p.cy + dy);
                            if (v.x - nx).hypot(v.y - ny) - rc < pad_margin - 1e-6 {
                                bad = true;
                                why = "foreign-pad";
                                break;
                            }
                        }
                        // hole_to_hole: drill-to-drill spacing for THT
                        // pads (board setup default 0.25mm).
                        if p.drill_r > 0.0
                            && (v.x - p.cx).hypot(v.y - p.cy)
                                < p.drill_r + board.layer_stack.via.drill_mm / 2.0 + 0.25
                        {
                            bad = true;
                            why = "tht-hole";
                            break;
                        }
                    }
                    if !bad {
                        'tracks: for (j, other) in final_routes.iter().enumerate() {
                            if j == i || other.is_empty() {
                                continue;
                            }
                            for sb in &other.segments {
                                let margin =
                                    (via_r + sb.width_mm / 2.0 + clearance).max(hole_margin);
                                if segment_point_too_close(sb.start, sb.end, (v.x, v.y), margin)
                                {
                                    bad = true;
                                    why = "foreign-track";
                                    break 'tracks;
                                }
                            }
                            for vb in &other.vias {
                                if (v.x - vb.x).hypot(v.y - vb.y)
                                    < (2.0 * via_r + clearance).max(hole_margin + via_r)
                                {
                                    bad = true;
                                    why = "foreign-via";
                                    break 'tracks;
                                }
                            }
                        }
                    }
                    // via_dangling — KiCad's rule: a via is dangling
                    // when connected on AT MOST ONE layer. The old
                    // check demanded copper specifically on the via's
                    // from/to layers, which amputated KiCad-legal
                    // full-stack vias carrying an l0<->l2 transition
                    // (cross-under / multi-layer maze) in an endless
                    // re-commit ping-pong. Count DISTINCT touched
                    // layers across everything the barrel spans; a
                    // same-net THT pad touches every layer at once.
                    // Plane-assigned nets are exempt: their via
                    // pierces the emitted zone fill (copper the
                    // oracle sees but this validator doesn't model).
                    if !bad && board.nets[i].plane_layer.is_none() {
                        let lo = v.from_layer.min(v.to_layer);
                        let hi = v.from_layer.max(v.to_layer);
                        let mut touched_layers: std::collections::BTreeSet<usize> =
                            std::collections::BTreeSet::new();
                        for sg in &final_routes[i].segments {
                            if sg.layer >= lo
                                && sg.layer <= hi
                                && segment_point_too_close(
                                    sg.start,
                                    sg.end,
                                    (v.x, v.y),
                                    sg.width_mm / 2.0 + via_r - 0.001,
                                )
                            {
                                touched_layers.insert(sg.layer);
                            }
                        }
                        let tht_pad = pad_rects.iter().any(|p| {
                            p.net == Some(net_id)
                                && p.drill_r > 0.0
                                && (v.x - p.cx).abs() < p.hx
                                && (v.y - p.cy).abs() < p.hy
                        });
                        if !tht_pad && touched_layers.len() < 2 {
                            bad = true;
                            why = "via-dangling";
                        }
                    }
                    if bad {
                        log::debug!(
                            "validator: via offender [{}] net '{}' at ({:.2},{:.2}) layers {}-{}; near segs: {:?}; near vias: {:?}",
                            why, board.nets[i].name, v.x, v.y, v.from_layer, v.to_layer,
                            final_routes[i].segments.iter()
                                .filter(|sg| segment_point_too_close(sg.start, sg.end, (v.x, v.y), 0.5))
                                .map(|sg| (sg.layer, sg.start, sg.end))
                                .collect::<Vec<_>>(),
                            final_routes[i].vias.iter()
                                .filter(|vb| (vb.x - v.x).hypot(vb.y - v.y) < 0.5)
                                .map(|vb| (vb.x, vb.y, vb.from_layer, vb.to_layer))
                                .collect::<Vec<_>>()
                        );
                        // Remember the site: extension recovery must
                        // not re-place the exact via the validator
                        // just amputated, or the loop ping-pongs until
                        // the round cap and ships the sinks unrouted.
                        banned_vias.push((i, (v.x, v.y)));
                        offender = Some((i, via_span_seg(&final_routes[i], vi)));
                        break 'scan;
                    }
                }
                // Track vs foreign track.
                for j in (i + 1)..final_routes.len() {
                    if final_routes[j].is_empty() {
                        continue;
                    }
                    for (sai, sa) in final_routes[i].segments.iter().enumerate() {
                        for (sbi, sb) in final_routes[j].segments.iter().enumerate() {
                            if sa.layer != sb.layer {
                                continue;
                            }
                            let min_gap =
                                sa.width_mm / 2.0 + sb.width_mm / 2.0 + clearance;
                            if segments_too_close(
                                sa.start, sa.end, sb.start, sb.end, min_gap,
                            ) {
                                let wi = board.nets.get(i).map(|n| n.weight).unwrap_or(1.0);
                                let wj = board.nets.get(j).map(|n| n.weight).unwrap_or(1.0);
                                // Amputee choice: net WEIGHT by default
                                // (lighter loses). Under the
                                // cheap_amputation trial knob: repair
                                // COST (smaller span subtree loses,
                                // weight breaks ties). The knob variant
                                // cures the whole-net-in-disguise class
                                // (ecc83 strict: one bad corner cost K2
                                // 26/27 segments, recovery walled) but
                                // shifts the rip cascade — measured both
                                // fixing and breaking boards, so
                                // dominance polices it per board.
                                // Milder gates (4x-asymmetry, 80%
                                // whole-net) measured NO effect: the win
                                // comes from early-round divergence, not
                                // the big amputation itself.
                                let rip_j = if board.config.cheap_amputation {
                                    let ci =
                                        amputation_cost(&final_routes[i], sai);
                                    let cj =
                                        amputation_cost(&final_routes[j], sbi);
                                    if ci != cj { cj < ci } else { wj <= wi }
                                } else {
                                    wj <= wi
                                };
                                log::debug!(
                                    "validator: track-vs-track offender '{}' seg ({:.2},{:.2})-({:.2},{:.2}) w={:.2} vs '{}' seg ({:.2},{:.2})-({:.2},{:.2}) w={:.2}",
                                    board.nets[i].name, sa.start.0, sa.start.1, sa.end.0, sa.end.1, sa.width_mm,
                                    board.nets[j].name, sb.start.0, sb.start.1, sb.end.0, sb.end.1, sb.width_mm
                                );
                                // Ban the ripped side's site (same
                                // ping-pong argument as track-vs-pad).
                                let (bi, bseg) = if rip_j {
                                    (j, sb)
                                } else {
                                    (i, sa)
                                };
                                banned_dangles.push((
                                    bi,
                                    (
                                        (bseg.start.0 + bseg.end.0) / 2.0,
                                        (bseg.start.1 + bseg.end.1) / 2.0,
                                    ),
                                ));
                                offender = Some(if rip_j {
                                    (j, Some(sbi))
                                } else {
                                    (i, Some(sai))
                                });
                                break 'scan;
                            }
                        }
                    }
                }
            }
            if let Some((k, sp, at_end, count)) = trim {
                let r = &mut final_routes[k];
                let (ps, pl) = r.path_spans[sp];
                let range = if at_end {
                    (ps + pl - count)..(ps + pl)
                } else {
                    ps..(ps + count)
                };
                let drain_start = range.start;
                log::debug!(
                    "validator: trimming {count} dangling tail segment(s) \
                     from net '{}' span {sp}",
                    board.nets.get(k).map(|n| n.name.as_str()).unwrap_or("")
                );
                r.segments.drain(range);
                r.path_spans[sp].1 -= count;
                if !at_end {
                    // span keeps its start index; nothing to do — the
                    // drained prefix shifts the remainder into place.
                }
                for (qi, spq) in r.path_spans.iter_mut().enumerate() {
                    if qi != sp && spq.0 > drain_start {
                        spq.0 -= count;
                    }
                }
                ripped += 1;
                if ripped > board.nets.len() * 8 {
                    whole_rip_mode = true;
                }
                continue;
            }
            match offender {
                Some((k, seg_idx)) => {
                    let seg_idx = if whole_rip_mode { None } else { seg_idx };
                    let name = board.nets.get(k).map(|n| n.name.clone()).unwrap_or_default();
                    // AMPUTATE the offending Steiner branch when the
                    // route carries path structure: one bad segment must
                    // cost ONE sink, not the whole 37-pin power tree.
                    let span = seg_idx.and_then(|si| {
                        final_routes[k]
                            .path_spans
                            .iter()
                            .copied()
                            .find(|(s, l)| si >= *s && si < *s + *l)
                    });
                    match span {
                        Some((s, _l)) => {
                            // Amputate the branch AND its whole subtree:
                            // children attach mid-path, and cutting only
                            // the parent strands them as dangling copper.
                            let r = &mut final_routes[k];
                            let root = r
                                .path_spans
                                .iter()
                                .position(|&(ps, _)| ps == s)
                                .unwrap_or(0);
                            let n = r.path_spans.len();
                            let mut doomed = vec![false; n];
                            doomed[root] = true;
                            loop {
                                let mut grew = false;
                                for i in 0..n {
                                    if !doomed[i] {
                                        if let Some(Some(pp)) =
                                            r.path_parents.get(i)
                                        {
                                            if doomed[*pp] {
                                                doomed[i] = true;
                                                grew = true;
                                            }
                                        }
                                    }
                                }
                                if !grew {
                                    break;
                                }
                            }
                            let cut: usize = (0..n)
                                .filter(|&i| doomed[i])
                                .map(|i| r.path_spans[i].1)
                                .sum();
                            log::warn!(
                                "geometric validation: amputating a subtree of '{name}' \
                                 ({} branch(es), {cut} segment(s)) — unrouted sinks beat \
                                 illegal copper",
                                doomed.iter().filter(|d| **d).count()
                            );
                            // Rebuild segments/spans/parents without the
                            // doomed spans (high-to-low keeps indices
                            // valid during drain).
                            let mut order: Vec<usize> =
                                (0..n).filter(|&i| doomed[i]).collect();
                            order.sort_by_key(|&i| std::cmp::Reverse(r.path_spans[i].0));
                            for &i in &order {
                                let (ps, pl) = r.path_spans[i];
                                r.segments.drain(ps..ps + pl);
                                for j in 0..n {
                                    if !doomed[j] && r.path_spans[j].0 > ps {
                                        r.path_spans[j].0 -= pl;
                                    }
                                }
                            }
                            // Vias travel with their branch (orphaned
                            // vias read as via_dangling to the oracle).
                            let mut vorder: Vec<usize> = (0..n)
                                .filter(|&i| doomed[i] && i < r.via_spans.len())
                                .collect();
                            vorder.sort_by_key(|&i| std::cmp::Reverse(r.via_spans[i].0));
                            for &i in &vorder {
                                let (vs, vl) = r.via_spans[i];
                                if vl > 0 && vs + vl <= r.vias.len() {
                                    r.vias.drain(vs..vs + vl);
                                    for j in 0..n {
                                        if !doomed[j]
                                            && j < r.via_spans.len()
                                            && r.via_spans[j].0 > vs
                                        {
                                            r.via_spans[j].0 -= vl;
                                        }
                                    }
                                }
                            }
                            // Compact spans/parents, remapping parent
                            // indices to the surviving order.
                            let mut remap = vec![usize::MAX; n];
                            let mut next = 0usize;
                            for i in 0..n {
                                if !doomed[i] {
                                    remap[i] = next;
                                    next += 1;
                                }
                            }
                            let spans: Vec<(usize, usize)> = (0..n)
                                .filter(|&i| !doomed[i])
                                .map(|i| r.path_spans[i])
                                .collect();
                            let parents: Vec<Option<usize>> = (0..n)
                                .filter(|&i| !doomed[i])
                                .map(|i| {
                                    r.path_parents.get(i).copied().flatten().and_then(
                                        |pp| {
                                            if doomed[pp] {
                                                None
                                            } else {
                                                Some(remap[pp])
                                            }
                                        },
                                    )
                                })
                                .collect();
                            let vspans: Vec<(usize, usize)> = (0..n)
                                .filter(|&i| !doomed[i])
                                .map(|i| r.via_spans.get(i).copied().unwrap_or((0, 0)))
                                .collect();
                            r.path_spans = spans;
                            r.path_parents = parents;
                            r.via_spans = vspans;
                            // Prune stubs orphaned by the cut: a
                            // 1-segment parentless span (pad-escape
                            // stub) whose start no longer coincides
                            // with any surviving copper endpoint is
                            // dangling — grid segments always start/end
                            // on cell centers, so an endpoint match is
                            // exact.
                            loop {
                                let mut drop_span: Option<usize> = None;
                                let via_r = board.layer_stack.via.pad_mm / 2.0;
                                'stubs: for (si, &(ps, pl)) in
                                    r.path_spans.iter().enumerate()
                                {
                                    if pl != 1 || r.path_parents.get(si).copied().flatten().is_some() {
                                        continue;
                                    }
                                    // A stub is anchored if EITHER endpoint
                                    // touches other copper: another span's
                                    // segment (width-aware), a via, or an
                                    // own-net pad. Endpoint-vs-segment-
                                    // endpoint matching alone pruned every
                                    // plane drop stub (pad on one end, via
                                    // on the other) the moment any span of
                                    // the net was amputated.
                                    let sg0 = &r.segments[ps];
                                    let own_half = sg0.width_mm / 2.0;
                                    for pt in [sg0.start, sg0.end] {
                                        for (qi, &(qs, ql)) in
                                            r.path_spans.iter().enumerate()
                                        {
                                            if qi == si {
                                                continue;
                                            }
                                            for seg in &r.segments[qs..qs + ql] {
                                                if seg.layer == sg0.layer
                                                    && segment_point_too_close(
                                                        seg.start,
                                                        seg.end,
                                                        pt,
                                                        seg.width_mm / 2.0 + own_half
                                                            - 0.001,
                                                    )
                                                {
                                                    continue 'stubs;
                                                }
                                            }
                                        }
                                        if r.vias.iter().any(|v| {
                                            (v.x - pt.0).hypot(v.y - pt.1)
                                                < via_r + own_half - 0.001
                                        }) {
                                            continue 'stubs;
                                        }
                                        if pad_rects.iter().any(|p| {
                                            p.net == Some(r.net_id)
                                                && (pt.0 - p.cx).abs()
                                                    < p.hx + own_half - 0.001
                                                && (pt.1 - p.cy).abs()
                                                    < p.hy + own_half - 0.001
                                        }) {
                                            continue 'stubs;
                                        }
                                    }
                                    drop_span = Some(si);
                                    break;
                                }
                                let Some(si) = drop_span else { break };
                                let (ps, pl) = r.path_spans[si];
                                r.segments.drain(ps..ps + pl);
                                r.path_spans.remove(si);
                                r.path_parents.remove(si);
                                if si < r.via_spans.len() {
                                    r.via_spans.remove(si);
                                }
                                for sp in r.path_spans.iter_mut() {
                                    if sp.0 > ps {
                                        sp.0 -= pl;
                                    }
                                }
                                for pparent in r.path_parents.iter_mut() {
                                    if let Some(pp) = pparent {
                                        if *pp > si {
                                            *pp -= 1;
                                        }
                                    }
                                }
                            }
                            // Queue for recovery: the cut sinks get an
                            // EXTENSION attempt (vias allowed) from the
                            // surviving tree.
                            if !ripped_nets.contains(&k) {
                                ripped_nets.push(k);
                            }
                        }
                        None => {
                            log::warn!(
                                "geometric validation: ripping net '{name}' — its copper \
                                 intersects or under-clears foreign copper; unrouted \
                                 beats illegal"
                            );
                            final_routes[k] = Route::empty(final_routes[k].net_id);
                            ripped_nets.push(k);
                        }
                    }
                    ripped += 1;
                    if ripped > board.nets.len() * 8 {
                        // Cap tripped: amputation isn't converging.
                        // Switch to whole-rip mode — every further
                        // offender loses its whole net. Unrouted beats
                        // illegal is a hard guarantee, not a budget;
                        // stopping here used to SHIP the remaining
                        // violations.
                        log::warn!(
                            "geometric validation: rip cap exhausted — \
                             whole-ripping remaining offender nets"
                        );
                        whole_rip_mode = true;
                    }
                }
                None => break,
            }
        }
    
    // Self-check (debug builds of truth): re-run the dangle scan on the
    // final state — a survivor here means an arm above exited without
    // consuming its offender.
    if std::env::var("BHDL_PNR_DEBUG_PLANES").is_ok() {
        for (i, rt) in final_routes.iter().enumerate() {
            for (si, sa) in rt.segments.iter().enumerate() {
                let own_half = sa.width_mm / 2.0;
                for pt in [sa.start, sa.end] {
                    let touched = rt.segments.iter().enumerate().any(|(sj, sb)| {
                        sj != si
                            && sb.layer == sa.layer
                            && segment_point_too_close(
                                sb.start, sb.end, pt,
                                sb.width_mm / 2.0 + own_half - 0.001,
                            )
                    }) || rt.vias.iter().any(|v| {
                        (v.x - pt.0).hypot(v.y - pt.1) < 0.3 + own_half
                    });
                    if !touched {
                        log::warn!(
                            "[self-check] net '{}' endpoint ({:.2},{:.2}) untouched at validator exit",
                            board.nets.get(i).map(|n| n.name.as_str()).unwrap_or(""),
                            pt.0, pt.1
                        );
                    }
                }
            }
        }
    }
    ripped_nets
}
