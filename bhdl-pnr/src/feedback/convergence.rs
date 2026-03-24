//! Convergence monitor with divergence detection and rollback.

use crate::placement::PlacementSnapshot;
use std::collections::VecDeque;

/// What to do after checking convergence.
pub enum ConvergenceAction {
    Continue,
    Converged,
    Rollback,
}

/// Tracks placement quality over iterations.
pub struct ConvergenceMonitor {
    wl_history: VecDeque<f64>,
    congestion_history: VecDeque<f64>,
    via_history: VecDeque<usize>,
    best_state: Option<PlacementSnapshot>,
    best_cost: f64,
    rollback_count: usize,
    window_size: usize,
    wl_tolerance: f64,
    max_rollbacks: usize,
    /// Minimum iteration before convergence is allowed.
    /// Prevents premature convergence before routing feedback kicks in.
    min_iterations: usize,
    routing_has_run: bool,
}

impl ConvergenceMonitor {
    pub fn new(window_size: usize, wl_tolerance: f64, max_rollbacks: usize) -> Self {
        ConvergenceMonitor {
            wl_history: VecDeque::new(),
            congestion_history: VecDeque::new(),
            via_history: VecDeque::new(),
            best_state: None,
            best_cost: f64::INFINITY,
            rollback_count: 0,
            window_size,
            wl_tolerance,
            max_rollbacks,
            min_iterations: 0,
            routing_has_run: false,
        }
    }

    /// Set minimum iterations before convergence (should be after first routing pass).
    pub fn set_min_iterations(&mut self, min: usize) {
        self.min_iterations = min;
    }

    /// Notify that routing has run at least once.
    pub fn notify_routing_done(&mut self) {
        self.routing_has_run = true;
    }

    /// Check convergence and return action.
    pub fn check(
        &mut self,
        wl: f64,
        max_overflow: usize,
        total_vias: usize,
        state: &PlacementSnapshot,
    ) -> ConvergenceAction {
        self.wl_history.push_back(wl);
        self.congestion_history.push_back(max_overflow as f64);
        self.via_history.push_back(total_vias);

        // Trim to window
        while self.wl_history.len() > self.window_size {
            self.wl_history.pop_front();
        }
        while self.congestion_history.len() > self.window_size {
            self.congestion_history.pop_front();
        }
        while self.via_history.len() > self.window_size {
            self.via_history.pop_front();
        }

        // Track best
        let cost = wl + 1000.0 * max_overflow as f64 + 10.0 * total_vias as f64;
        if cost < self.best_cost {
            self.best_cost = cost;
            self.best_state = Some(state.clone());
        }

        // Divergence detection: WL increasing over recent window
        if self.wl_history.len() >= self.window_size {
            let recent: f64 = self.wl_history.iter().rev().take(10).sum::<f64>() / 10.0;
            let earlier: f64 = self
                .wl_history
                .iter()
                .rev()
                .skip(20)
                .take(10)
                .sum::<f64>()
                / 10.0;

            if earlier > 0.0 && recent > earlier * 1.1 {
                if self.rollback_count < self.max_rollbacks {
                    self.rollback_count += 1;
                    return ConvergenceAction::Rollback;
                }
            }
        }

        // Convergence: WL stable, no overflow, AND routing has had a chance
        let past_minimum = self.wl_history.len() > self.min_iterations;
        if max_overflow == 0 && self.wl_stable() && past_minimum && self.routing_has_run {
            return ConvergenceAction::Converged;
        }

        ConvergenceAction::Continue
    }

    /// Get best state for rollback.
    pub fn best_state(&self) -> Option<&PlacementSnapshot> {
        self.best_state.as_ref()
    }

    fn wl_stable(&self) -> bool {
        if self.wl_history.len() < 20 {
            return false;
        }
        let recent: Vec<f64> = self.wl_history.iter().rev().take(20).cloned().collect();
        let mean = recent.iter().sum::<f64>() / recent.len() as f64;
        if mean == 0.0 {
            return true;
        }
        let variance = recent.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / recent.len() as f64;
        let cv = variance.sqrt() / mean; // coefficient of variation
        cv < self.wl_tolerance
    }
}
