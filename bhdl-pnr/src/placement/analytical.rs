//! Analytical wirelength computation using log-sum-exp (LSE) smooth HPWL.
//!
//! Pin positions depend on continuous rotation:
//!   x_k = x_c + dx_k * cos(θ) - dy_k * sin(θ)
//!   y_k = y_c + dx_k * sin(θ) + dy_k * cos(θ)
//!
//! Gradients ∂WL/∂x, ∂WL/∂y, ∂WL/∂θ computed analytically.

use crate::placement::Forces;
use crate::types::*;

/// Compute weighted wirelength and its gradient.
///
/// Returns (total_wl, forces) where forces contain per-component gradients.
pub fn compute_wirelength(
    board: &Board,
    gamma: f64,
) -> (f64, Forces) {
    let n = board.components.len();
    let mut forces = Forces::zeros(n);
    let mut total_wl = 0.0;

    // Build component index lookup
    let comp_idx: std::collections::HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();

    for net in &board.nets {
        if net.pins.len() < 2 {
            continue;
        }

        // Compute pin positions in global coordinates
        let mut pin_xs = Vec::with_capacity(net.pins.len());
        let mut pin_ys = Vec::with_capacity(net.pins.len());
        let mut pin_comp_indices = Vec::with_capacity(net.pins.len());
        let mut pin_local_offsets = Vec::with_capacity(net.pins.len());

        for &(comp_id, pin_id) in &net.pins {
            let Some(&ci) = comp_idx.get(&comp_id) else {
                continue;
            };
            let comp = &board.components[ci];
            let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pin_id) else {
                continue;
            };

            let cos_t = comp.theta.cos();
            let sin_t = comp.theta.sin();
            let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
            let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;

            pin_xs.push(gx);
            pin_ys.push(gy);
            pin_comp_indices.push(ci);
            pin_local_offsets.push((pin.dx, pin.dy));
        }

        if pin_xs.len() < 2 {
            continue;
        }

        // LSE wirelength for this net
        let inv_gamma = 1.0 / gamma;

        // WL_x = γ * (log Σ exp(x_k/γ) + log Σ exp(-x_k/γ))
        let (wl_x, grad_x_pos, grad_x_neg) =
            lse_wirelength_1d(&pin_xs, gamma, inv_gamma);
        let (wl_y, grad_y_pos, grad_y_neg) =
            lse_wirelength_1d(&pin_ys, gamma, inv_gamma);

        let net_wl = wl_x + wl_y;
        total_wl += net.weight * net_wl;

        // Distribute gradients to components
        for k in 0..pin_xs.len() {
            let ci = pin_comp_indices[k];
            let (dx_local, dy_local) = pin_local_offsets[k];
            let comp = &board.components[ci];

            let dwl_dxk = grad_x_pos[k] + grad_x_neg[k];
            let dwl_dyk = grad_y_pos[k] + grad_y_neg[k];

            // ∂x_k/∂x_c = 1, ∂y_k/∂y_c = 1
            forces.dx[ci] += net.weight * dwl_dxk;
            forces.dy[ci] += net.weight * dwl_dyk;

            // ∂x_k/∂θ = -dx_local * sin(θ) - dy_local * cos(θ)
            // ∂y_k/∂θ =  dx_local * cos(θ) - dy_local * sin(θ)
            let cos_t = comp.theta.cos();
            let sin_t = comp.theta.sin();
            let dxk_dtheta = -dx_local * sin_t - dy_local * cos_t;
            let dyk_dtheta = dx_local * cos_t - dy_local * sin_t;

            forces.d_theta[ci] +=
                net.weight * (dwl_dxk * dxk_dtheta + dwl_dyk * dyk_dtheta);
        }
    }

    (total_wl, forces)
}

/// 1D LSE wirelength: γ·log(Σ exp(x_k/γ)) + γ·log(Σ exp(-x_k/γ))
///
/// Returns (wl, grad_pos, grad_neg) where grad[k] = ∂WL/∂x_k contribution.
fn lse_wirelength_1d(
    coords: &[f64],
    gamma: f64,
    inv_gamma: f64,
) -> (f64, Vec<f64>, Vec<f64>) {
    // Numerically stable: subtract max before exp
    let max_val = coords.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min_val = coords.iter().cloned().fold(f64::INFINITY, f64::min);

    // Positive term: γ·log(Σ exp(x_k/γ))
    let exps_pos: Vec<f64> = coords.iter().map(|&x| ((x - max_val) * inv_gamma).exp()).collect();
    let sum_pos: f64 = exps_pos.iter().sum();
    let wl_pos = gamma * sum_pos.ln() + max_val;

    // Negative term: γ·log(Σ exp(-x_k/γ))
    let exps_neg: Vec<f64> = coords
        .iter()
        .map(|&x| ((-x + min_val) * inv_gamma).exp())
        .collect();
    let sum_neg: f64 = exps_neg.iter().sum();
    let wl_neg = gamma * sum_neg.ln() - min_val;

    // Gradients: ∂/∂x_k of γ·log(Σ exp(x_k/γ)) = exp(x_k/γ) / Σ exp(x_j/γ)
    let grad_pos: Vec<f64> = exps_pos.iter().map(|e| e / sum_pos).collect();
    let grad_neg: Vec<f64> = exps_neg.iter().map(|e| -e / sum_neg).collect();

    (wl_pos + wl_neg, grad_pos, grad_neg)
}

/// Compute the HPWL (half-perimeter wirelength) for metrics.
pub fn compute_hpwl(board: &Board) -> f64 {
    let comp_idx: std::collections::HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();

    let mut total = 0.0;

    for net in &board.nets {
        if net.pins.len() < 2 {
            continue;
        }

        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;

        for &(comp_id, pin_id) in &net.pins {
            let Some(&ci) = comp_idx.get(&comp_id) else {
                continue;
            };
            let comp = &board.components[ci];
            let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pin_id) else {
                continue;
            };

            let cos_t = comp.theta.cos();
            let sin_t = comp.theta.sin();
            let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
            let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;

            min_x = min_x.min(gx);
            max_x = max_x.max(gx);
            min_y = min_y.min(gy);
            max_y = max_y.max(gy);
        }

        total += (max_x - min_x) + (max_y - min_y);
    }

    total
}
