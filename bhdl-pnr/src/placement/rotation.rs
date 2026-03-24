//! Continuous rotation support.
//!
//! Unlike Cypress's Gumbel-Softmax over {0°, 90°, 180°, 270°},
//! rotation θ is a continuous variable optimized with gradient descent.

use crate::types::*;

/// Snap rotation to nearest multiple of `snap_degrees` if it improves or
/// doesn't significantly degrade wirelength.
pub fn snap_rotations(
    board: &mut Board,
    snap_degrees: f64,
    wl_tolerance: f64,
    wl_fn: impl Fn(&Board) -> f64,
) {
    let original_wl = wl_fn(board);

    for i in 0..board.components.len() {
        if board.components[i].placement.is_fixed() {
            continue;
        }

        let orig_theta = board.components[i].theta;
        let deg = orig_theta.to_degrees().rem_euclid(360.0);
        let snapped_deg = (deg / snap_degrees).round() * snap_degrees;
        let snapped_theta = snapped_deg.to_radians();

        // Snap in place and evaluate
        board.components[i].theta = snapped_theta;
        let snapped_wl = wl_fn(board);

        if snapped_wl > original_wl * (1.0 + wl_tolerance) {
            board.components[i].theta = orig_theta; // revert
        }
    }
}

/// Normalize rotation to [0, 2π).
pub fn normalize_theta(theta: f64) -> f64 {
    theta.rem_euclid(std::f64::consts::TAU)
}
