//! Adam optimizer with constraint-aware position updates.

use crate::placement::Forces;
use crate::types::*;

/// Per-component Adam optimizer state.
pub struct AdamState {
    pub m_x: Vec<f64>,
    pub m_y: Vec<f64>,
    pub m_theta: Vec<f64>,
    pub v_x: Vec<f64>,
    pub v_y: Vec<f64>,
    pub v_theta: Vec<f64>,
    pub t: usize,
}

impl AdamState {
    pub fn new(n: usize) -> Self {
        AdamState {
            m_x: vec![0.0; n],
            m_y: vec![0.0; n],
            m_theta: vec![0.0; n],
            v_x: vec![0.0; n],
            v_y: vec![0.0; n],
            v_theta: vec![0.0; n],
            t: 0,
        }
    }
}

/// Apply one Adam update step, respecting placement constraints.
/// `frozen` optionally marks components that have been progressively frozen.
pub fn adam_step(
    board: &mut Board,
    forces: &Forces,
    state: &mut AdamState,
    config: &PlacementConfig,
    opt: &OptimizerConfig,
    frozen: Option<&[bool]>,
) {
    state.t += 1;
    let t = state.t as f64;
    let bc1 = 1.0 - opt.beta1.powi(state.t as i32);
    let bc2 = 1.0 - opt.beta2.powi(state.t as i32);
    let _ = t; // used via bc1/bc2

    let board_w = board.config.outline.width();
    let board_h = board.config.outline.height();
    let ec = board.config.edge_clearance_mm;

    for (i, comp) in board.components.iter_mut().enumerate() {
        // Skip progressively frozen components
        if frozen.map_or(false, |f| f.get(i).copied().unwrap_or(false)) {
            continue;
        }

        match &comp.placement {
            PlacementConstraint::Fixed { .. } => {
                // Completely frozen — skip
                continue;
            }
            PlacementConstraint::FixedPosition { .. } => {
                // Only theta updates
                let step = adam_scalar(
                    forces.d_theta[i],
                    &mut state.m_theta[i],
                    &mut state.v_theta[i],
                    opt.beta1,
                    opt.beta2,
                    opt.epsilon,
                    bc1,
                    bc2,
                );
                comp.theta -= config.rotation_lr * step;
            }
            PlacementConstraint::Edge { edge, .. } => {
                // Free axis along edge + theta
                match edge {
                    BoardEdge::Left | BoardEdge::Right => {
                        // x is frozen, y is free
                        let step_y = adam_scalar(
                            forces.dy[i],
                            &mut state.m_y[i],
                            &mut state.v_y[i],
                            opt.beta1,
                            opt.beta2,
                            opt.epsilon,
                            bc1,
                            bc2,
                        );
                        comp.y -= config.position_lr * step_y;
                        comp.y = comp.y.clamp(ec, board_h - ec);
                    }
                    BoardEdge::Top | BoardEdge::Bottom => {
                        // y is frozen, x is free
                        let step_x = adam_scalar(
                            forces.dx[i],
                            &mut state.m_x[i],
                            &mut state.v_x[i],
                            opt.beta1,
                            opt.beta2,
                            opt.epsilon,
                            bc1,
                            bc2,
                        );
                        comp.x -= config.position_lr * step_x;
                        comp.x = comp.x.clamp(ec, board_w - ec);
                    }
                }
                // Theta always updates for edge components
                let step_t = adam_scalar(
                    forces.d_theta[i],
                    &mut state.m_theta[i],
                    &mut state.v_theta[i],
                    opt.beta1,
                    opt.beta2,
                    opt.epsilon,
                    bc1,
                    bc2,
                );
                comp.theta -= config.rotation_lr * step_t;
            }
            PlacementConstraint::Free | PlacementConstraint::PreferRegion { .. } => {
                // Full update
                let step_x = adam_scalar(
                    forces.dx[i],
                    &mut state.m_x[i],
                    &mut state.v_x[i],
                    opt.beta1,
                    opt.beta2,
                    opt.epsilon,
                    bc1,
                    bc2,
                );
                let step_y = adam_scalar(
                    forces.dy[i],
                    &mut state.m_y[i],
                    &mut state.v_y[i],
                    opt.beta1,
                    opt.beta2,
                    opt.epsilon,
                    bc1,
                    bc2,
                );
                let step_t = adam_scalar(
                    forces.d_theta[i],
                    &mut state.m_theta[i],
                    &mut state.v_theta[i],
                    opt.beta1,
                    opt.beta2,
                    opt.epsilon,
                    bc1,
                    bc2,
                );

                comp.x -= config.position_lr * step_x;
                comp.y -= config.position_lr * step_y;
                comp.theta -= config.rotation_lr * step_t;

                // Clamp to board (account for component half-width so edges stay inside)
                let hw = comp.width_mm / 2.0;
                let hh = comp.height_mm / 2.0;
                comp.x = comp.x.clamp(ec + hw, board_w - ec - hw);
                comp.y = comp.y.clamp(ec + hh, board_h - ec - hh);
            }
        }
    }
}

/// Single-variable Adam update. Returns the bias-corrected step.
fn adam_scalar(
    grad: f64,
    m: &mut f64,
    v: &mut f64,
    beta1: f64,
    beta2: f64,
    epsilon: f64,
    bc1: f64,
    bc2: f64,
) -> f64 {
    *m = beta1 * *m + (1.0 - beta1) * grad;
    *v = beta2 * *v + (1.0 - beta2) * grad * grad;
    let m_hat = *m / bc1;
    let v_hat = *v / bc2;
    m_hat / (v_hat.sqrt() + epsilon)
}
