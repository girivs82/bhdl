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

pub mod constraint;
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
use log::info;
use placement::analytical;
use placement::grouping;
use placement::optimizer::{self, AdamState};
use routing::grid::RoutingGrid;
use routing::pathfinder;
use types::*;

/// Run multiple placement+routing trials with different initializations,
/// return the best result (highest routability, then lowest HPWL).
pub fn place_and_route_best_of(
    board: Board,
    config: PnrConfig,
    trials: usize,
    base_seed: u64,
) -> Result<PnrResult> {
    let mut best: Option<PnrResult> = None;

    for trial in 0..trials {
        info!("=== Trial {}/{} ===", trial + 1, trials);
        let trial_board = board.clone();
        let result = place_and_route(trial_board, config.clone(), base_seed.wrapping_add(trial as u64))?;

        let dominated = best.as_ref().map_or(false, |b| {
            // Better = more CONNECTED SINKS, then lower HPWL. Counting
            // non-empty routes let a trial shipping one surviving branch
            // and 19 stranded pads tie a fully-connected one.
            let b_conn = b.metrics.connected_sinks;
            let r_conn = result.metrics.connected_sinks;
            r_conn < b_conn
                || (r_conn == b_conn && result.metrics.hpwl_mm >= b.metrics.hpwl_mm)
        });

        if !dominated {
            info!(
                "Trial {} is new best: {} connected sink(s), HPWL={:.1}mm",
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
                && result.drc_violations.is_empty();
            best = Some(result);
            if perfect {
                info!(
                    "Trial {} fully connected with no DRC — skipping remaining trials",
                    trial + 1
                );
                break;
            }
        }
    }

    best.ok_or_else(|| anyhow::anyhow!("No trials completed"))
}

/// Run the concurrent place & route loop.
///
/// Input: a fully constructed `Board` (from semantic preprocessing).
/// Output: `PnrResult` with final placement, routes, metrics, and DRC.
pub fn place_and_route(mut board: Board, config: PnrConfig, seed: u64) -> Result<PnrResult> {
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

    // 4. Legalization
    info!("Legalizing placement...");
    legalization::legalize(&mut board, 0.1);

    // 4.5 Detailed placement: greedy swap/rotate HPWL refinement on the
    // legal placement. Every accepted move is legality-checked, so no
    // re-legalization is needed.
    let (wl0, wl1) = placement::detailed::refine(&mut board, 4);
    if wl1 < wl0 - 1e-9 {
        info!("Detailed placement: HPWL {:.1} -> {:.1}mm ({:.1}%)",
            wl0, wl1, (wl0 - wl1) / wl0 * 100.0);
    }

    // 4.7 Split-plane regions: several rails can share one Power layer;
    // each gets a BAND along the axis of larger pin spread, boundaries
    // at midpoints between rail centroids (computable only now — pin
    // positions needed placement). Bands shrink 0.25mm per inner side
    // so adjacent fills keep the 0.3mm zone clearance plus margin.
    assign_plane_regions(&mut board);

    // 5. Final routing — two-pass strategy (route like a human)
    //    Pass 1: single-layer routing (no vias) — maximize what can be routed flat
    //    Pass 2: remaining unrouted nets get vias to escape to other layers
    info!("Final routing pass 1 (single-layer, no vias)...");
    // Plane-assigned nets don't route as trees: their copper is the
    // emitted zone FILL; surface pads get via drops after routing.
    let routing_nets: Vec<PnrNet> = board
        .nets
        .iter()
        .map(|n| {
            if n.plane_layer.is_some() {
                PnrNet { pins: Vec::new(), ..n.clone() }
            } else {
                n.clone()
            }
        })
        .collect();
    let mut final_grid = RoutingGrid::build(&board);
    let mut final_routes = pathfinder::pathfinder_route(
        &mut final_grid,
        &routing_nets,
        &board,
        100,
        1.0,
        1.0,
        false, // no vias
    );

    if std::env::var("BHDL_PNR_DEBUG_CLEARANCE").is_ok() {
        debug_check_foreign_pads(&board, &final_routes, "after-pass1");
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
            let ripped = validate_and_rip(
                &board,
                &mut final_routes,
                &mut banned_vias,
                &mut banned_dangles,
            );
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
            let (partial, whole): (Vec<usize>, Vec<usize>) =
                ripped.iter().partition(|&&i| !final_routes[i].is_empty());
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
                let got = pathfinder::extend_route(
                    &mut ext_grid, &board.nets[i], &board, &mut route, 1.0, 1.0, &banned,
                    &dangles, false,
                );
                if got > 0 {
                    info!(
                        "recovery: extended '{}' to {got} previously-cut sink(s)",
                        board.nets[i].name
                    );
                    final_routes[i] = route;
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
                let got = pathfinder::extend_route(
                    rec_grid, &board.nets[i], &board, &mut fresh, 1.0, 1.0,
                    &banned_v, &banned_d, false,
                );
                if got > 0 {
                    info!(
                        "recovery: greedy reroute of '{}' reached {got} sink(s)",
                        board.nets[i].name
                    );
                    final_routes[i] = fresh;
                    pathfinder::block_route_geometry(rec_grid, &final_routes[i], &board);
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

    // 5.95. 45° corner mitering (verified post-pass) + one final
    // guarantee round: any miter the local check mis-judged is
    // amputated/trimmed by the validator like all copper.
    {
        // TRANSACTIONAL: elegance never costs connectivity. If the
        // guarantee round amputates anything on a mitered net (a case
        // the local check missed), that net reverts to its pre-miter
        // route wholesale — validated copper we already had.
        let saved = final_routes.clone();
        let mitered = miter_pass(&board, &mut final_routes);
        if mitered > 0 {
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
    let drc_violations = legalization::check_drc(&board, &final_routes);

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

    let connected_sinks = pathfinder::count_connected_sinks(&board, &final_routes);
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
        },
        drc_violations,
    })
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

/// True when segment AB and segment CD (as center-lines) intersect or
/// pass within `min_gap` of each other.
fn segments_too_close(
    a: (f64, f64),
    b: (f64, f64),
    c: (f64, f64),
    d: (f64, f64),
    min_gap: f64,
) -> bool {
    fn orient(p: (f64, f64), q: (f64, f64), r: (f64, f64)) -> f64 {
        (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0)
    }
    let (o1, o2) = (orient(a, b, c), orient(a, b, d));
    let (o3, o4) = (orient(c, d, a), orient(c, d, b));
    if o1 * o2 < 0.0 && o3 * o4 < 0.0 {
        return true;
    }
    fn seg_pt(p: (f64, f64), q: (f64, f64), r: (f64, f64)) -> f64 {
        let (dx, dy) = (q.0 - p.0, q.1 - p.1);
        let l2 = dx * dx + dy * dy;
        let t = if l2 == 0.0 {
            0.0
        } else {
            (((r.0 - p.0) * dx + (r.1 - p.1) * dy) / l2).clamp(0.0, 1.0)
        };
        let (nx, ny) = (p.0 + t * dx, p.1 + t * dy);
        ((r.0 - nx).powi(2) + (r.1 - ny).powi(2)).sqrt()
    }
    seg_pt(a, b, c).min(seg_pt(a, b, d)).min(seg_pt(c, d, a)).min(seg_pt(c, d, b)) < min_gap
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
        (layer == 0 && p.layer_top) || (layer == n_layers - 1 && p.layer_bot)
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
    let comp_idx: std::collections::HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();
    // Group plane nets by layer.
    let mut by_layer: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, n) in board.nets.iter().enumerate() {
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
        let comp_idx: std::collections::HashMap<ComponentId, usize> = board
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
        for &(comp_id, pin_id) in &net.pins {
            let Some(&ci) = comp_idx.get(&comp_id) else { continue };
            let comp = &board.components[ci];
            let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pin_id) else {
                continue;
            };
            if pin.unplaced {
                continue;
            }
            if pin.pad.as_ref().and_then(|p| p.drill_mm).is_some() {
                continue; // through-hole barrel pierces the plane
            }
            let cos_t = comp.theta.cos();
            let sin_t = comp.theta.sin();
            let px = comp.x + pin.dx * cos_t - pin.dy * sin_t;
            let py = comp.y + pin.dx * sin_t + pin.dy * cos_t;
            let stub_layer = match comp.side {
                BoardSide::Top => 0,
                BoardSide::Bottom => n_layers - 1,
            };
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
                continue;
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
                // The STUB pad→via must also be legal, not just the via
                // site: the repair pass can need multi-mm stubs (region
                // projection, ring 10), and an unvalidated stub plows
                // straight through whatever recovery routed in between
                // (shipped as shorting_items on the fpga board).
                let stub_a = (px, py);
                let stub_b = (x, y);
                for (k, r) in final_routes.iter().enumerate() {
                    if k == i {
                        continue; // own copper may touch its own stub
                    }
                    for sg in &r.segments {
                        if sg.layer != stub_layer {
                            continue;
                        }
                        if segments_too_close(
                            stub_a,
                            stub_b,
                            sg.start,
                            sg.end,
                            share / 2.0 + sg.width_mm / 2.0 + clearance,
                        ) {
                            return false;
                        }
                    }
                    for v in &r.vias {
                        if segment_point_too_close(
                            stub_a,
                            stub_b,
                            (v.x, v.y),
                            share / 2.0 + via_r + clearance,
                        ) {
                            return false;
                        }
                    }
                }
                for p in &pads {
                    if p.net == Some(net.id) {
                        continue;
                    }
                    // Pad rect approximated by its four edges.
                    let (x0, y0, x1, y1) =
                        (p.cx - p.hx, p.cy - p.hy, p.cx + p.hx, p.cy + p.hy);
                    let edges = [
                        ((x0, y0), (x1, y0)),
                        ((x1, y0), (x1, y1)),
                        ((x1, y1), (x0, y1)),
                        ((x0, y1), (x0, y0)),
                    ];
                    if edges.iter().any(|&(a, b)| {
                        segments_too_close(stub_a, stub_b, a, b, share / 2.0 + clearance)
                    }) {
                        return false;
                    }
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
                    log::warn!(
                        "plane via drop: no legal site near pad '{}' of '{}' (net '{}') — pad stays unconnected",
                        pin.name, comp.refdes, net.name
                    );
                    continue;
                }
            };
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

fn segment_point_too_close(a: (f64, f64), b: (f64, f64), p: (f64, f64), gap: f64) -> bool {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= 1e-12 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    let (cx, cy) = (a.0 + t * dx, a.1 + t * dy);
    (p.0 - cx).hypot(p.1 - cy) < gap
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
                let (pw, ph, thru, drill_r) = match &pin.pad {
                    Some(p) => (
                        p.width_mm,
                        p.height_mm,
                        p.drill_mm.is_some(),
                        p.drill_mm.unwrap_or(0.0) / 2.0,
                    ),
                    // 0.5 matches the EXPORTER's fallback pad — the validator
                    // modeling 0.8 while the file ships 0.5 let stubs pass
                    // as pad-anchored that KiCad sees dangling.
                    None => (0.5, 0.5, false, 0.0),
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
                });
            }
        }
        let pad_on_layer = |p: &PadRect, layer: usize| -> bool {
            (layer == 0 && p.layer_top) || (layer == n_layers - 1 && p.layer_bot)
        };
        // Min distance from segment AB to an axis-aligned rect ≈ distance
        // to the rect's center clamped by extents: sample-based (rects
        // are small vs segments; 9 samples along the segment suffice at
        // 0.3mm cells).
        let seg_hits_rect = |a: (f64, f64), b: (f64, f64), p: &PadRect, gap: f64| -> bool {
            for i in 0..=8 {
                let t = i as f64 / 8.0;
                let x = a.0 + t * (b.0 - a.0);
                let y = a.1 + t * (b.1 - a.1);
                if (x - p.cx).abs() < p.hx + gap && (y - p.cy).abs() < p.hy + gap {
                    return true;
                }
            }
            false
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
                        if p.net == Some(net_id) {
                            continue;
                        }
                        if (v.x - p.cx).abs() < p.hx + pad_margin
                            && (v.y - p.cy).abs() < p.hy + pad_margin
                        {
                            bad = true;
                            why = "foreign-pad";
                            break;
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
                    // via_dangling: the via must land on copper on BOTH
                    // layers it spans — a segment endpoint at its center
                    // on that layer, or a THT pad of its own net.
                    // Plane-assigned nets are exempt: their via pierces
                    // the emitted zone fill (copper the oracle sees but
                    // this validator doesn't model).
                    if !bad && board.nets[i].plane_layer.is_none() {
                        for check_layer in [v.from_layer, v.to_layer] {
                            // Width-aware copper overlap (a via inside
                            // a wide trunk's width is connected even off
                            // the centerline), incl. mid-segment T-joints
                            // (collinear runs merge at emission).
                            let mut touched = final_routes[i].segments.iter().any(|sg| {
                                sg.layer == check_layer
                                    && segment_point_too_close(
                                        sg.start,
                                        sg.end,
                                        (v.x, v.y),
                                        sg.width_mm / 2.0 + via_r - 0.001,
                                    )
                            });
                            if !touched {
                                touched = pad_rects.iter().any(|p| {
                                    p.net == Some(net_id)
                                        && p.drill_r > 0.0
                                        && (v.x - p.cx).abs() < p.hx
                                        && (v.y - p.cy).abs() < p.hy
                                });
                            }
                            if !touched {
                                bad = true;
                                why = "via-dangling";
                                break;
                            }
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
                                // Amputate the offending BRANCH of the
                                // lighter net, not its whole tree — the
                                // recovery loop then extends it back with
                                // vias. Whole-net rips threw away good
                                // copper and the from-scratch reroute
                                // often failed where an extension
                                // succeeds.
                                log::debug!(
                                    "validator: track-vs-track offender '{}' seg ({:.2},{:.2})-({:.2},{:.2}) w={:.2} vs '{}' seg ({:.2},{:.2})-({:.2},{:.2}) w={:.2}",
                                    board.nets[i].name, sa.start.0, sa.start.1, sa.end.0, sa.end.1, sa.width_mm,
                                    board.nets[j].name, sb.start.0, sb.start.1, sb.end.0, sb.end.1, sb.width_mm
                                );
                                offender = Some(if wj <= wi {
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
