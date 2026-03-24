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

pub mod feedback;
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

/// Run the concurrent place & route loop.
///
/// Input: a fully constructed `Board` (from semantic preprocessing).
/// Output: `PnrResult` with final placement, routes, metrics, and DRC.
pub fn place_and_route(mut board: Board, config: PnrConfig) -> Result<PnrResult> {
    let n = board.components.len();
    info!(
        "Starting P&R: {} components, {} nets, {} layers",
        n,
        board.nets.len(),
        board.layer_stack.layers.len()
    );

    // 1. Initialize placement
    placement::initialize(&mut board);

    // 2. Set up optimizer state
    let mut adam = AdamState::new(n);
    let mut monitor = ConvergenceMonitor::new(
        config.convergence.window_size,
        config.convergence.wl_tolerance,
        config.convergence.max_rollbacks,
    );

    let mut routes: Vec<Route> = board.nets.iter().map(|n| Route::empty(n.id)).collect();
    let mut grid: Option<RoutingGrid> = None;
    let mut gamma = 0.5; // LSE smoothness parameter

    // 3. Main iterative loop
    for iteration in 0..config.max_iterations {
        // Compute placement forces
        let (wl, wl_forces) = analytical::compute_wirelength(&board, gamma);

        let group_forces = grouping::compute_group_cohesion(&board);
        let thermal_forces = grouping::compute_thermal_spreading(&board, 0.1);
        let region_forces = grouping::compute_region_preference(&board);

        // Combine forces
        let mut forces = wl_forces;
        forces.accumulate(&group_forces, config.placement.lambda_group);
        forces.accumulate(&thermal_forces, config.placement.lambda_thermal);
        forces.accumulate(&region_forces, config.placement.lambda_region);

        // Add routing feedback forces (if routing has been done)
        if let Some(ref g) = grid {
            // Via penalty
            let via_grad = congestion::compute_via_penalty(&board, &routes, &board.nets);
            for (i, (vx, vy)) in via_grad.iter().enumerate() {
                forces.dx[i] += config.placement.lambda_via * vx;
                forces.dy[i] += config.placement.lambda_via * vy;
            }
            let _ = g; // congestion inflation already applied to density_inflation
        }

        // Update positions (constraint-aware)
        optimizer::adam_step(
            &mut board,
            &forces,
            &mut adam,
            &config.placement,
            &config.optimizer,
        );

        // Periodic routing feedback
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
            );

            // Apply congestion inflation
            congestion::apply_congestion_inflation(&mut board, &g, 0.3);

            grid = Some(g);

            if iteration % 100 == 0 {
                let total_vias: usize = routes.iter().map(|r| r.via_count()).sum();
                let overflow = grid.as_ref().map_or(0, |g| g.max_overflow());
                info!(
                    "Iter {}: WL={:.1}, vias={}, overflow={}",
                    iteration, wl, total_vias, overflow
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

        // Decrease gamma (tighten LSE approximation)
        if iteration > 0 && iteration % 100 == 0 {
            gamma *= 0.8;
            gamma = gamma.max(0.01);
        }
    }

    // 4. Legalization
    info!("Legalizing placement...");
    legalization::legalize(&mut board, 0.1);

    // 5. Final detailed routing
    info!("Final routing...");
    let mut final_grid = RoutingGrid::build(&board);
    let final_routes = pathfinder::pathfinder_route(
        &mut final_grid,
        &board.nets,
        &board,
        50, // more iterations for final routing
        1.0,
        1.0,
    );

    // 6. DRC
    let drc_violations = legalization::check_drc(&board, &final_routes);

    // 7. Metrics
    let hpwl = analytical::compute_hpwl(&board);
    let total_length: f64 = final_routes.iter().map(|r| r.total_length()).sum();
    let total_vias: usize = final_routes.iter().map(|r| r.via_count()).sum();
    let routed_count = final_routes.iter().filter(|r| !r.is_empty()).count();
    let total_nets = board.nets.iter().filter(|n| n.pins.len() >= 2).count();

    info!(
        "P&R complete: HPWL={:.1}mm, routed length={:.1}mm, vias={}, routability={:.0}%, DRC violations={}",
        hpwl,
        total_length,
        total_vias,
        if total_nets > 0 {
            routed_count as f64 / total_nets as f64 * 100.0
        } else {
            100.0
        },
        drc_violations.len()
    );

    Ok(PnrResult {
        board,
        routes: final_routes,
        metrics: PnrMetrics {
            hpwl_mm: hpwl,
            total_routed_length_mm: total_length,
            via_count: total_vias,
            max_congestion: final_grid.max_overflow() as f64,
            routability_pct: if total_nets > 0 {
                routed_count as f64 / total_nets as f64 * 100.0
            } else {
                100.0
            },
            iterations: config.max_iterations,
        },
        drc_violations,
    })
}
