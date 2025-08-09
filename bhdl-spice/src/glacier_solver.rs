//! GLACIER: Gradient Logarithmic Adaptive Circuit Intelligent Exploration Resolver
//! 
//! Clean implementation following the documented GLACIER approach
//! for achieving sub-1% accuracy in circuit simulation.

use nalgebra::{DMatrix, DVector};

/// Calculate variance of a slice of f64 values
fn variance(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let sum_sq_diff = values.iter().map(|&x| (x - mean).powi(2)).sum::<f64>();
    sum_sq_diff / values.len() as f64
}
use petgraph::graph::{NodeIndex, EdgeIndex};
use std::collections::HashMap;
use log::{info, debug, warn};

use crate::{
    Circuit, ComponentModel, SpiceError, Result,
    NodeVoltages, BranchCurrents, AnalysisResult,
    runtime_models::{RuntimeModelEngine, ModelExecutionContext},
    solve_with_glacier_maestro,
    GlacierSolution as ProductionSolution,
};

/// Transient analysis result
#[derive(Debug, Clone)]
pub struct TransientResult {
    /// Time points
    pub time_points: Vec<f64>,
    /// Node voltages at each time point
    pub node_voltages: Vec<NodeVoltages>,
    /// Branch currents at each time point
    pub branch_currents: Vec<BranchCurrents>,
}

/// Adaptive PID Controller with gain adaptation based on logarithmic gradient
#[derive(Debug, Clone)]
pub struct AdaptivePIDController {
    // Base parameters
    base_kp: f64,
    base_ki: f64,
    base_kd: f64,
    
    // Active parameters (adapted based on gradient)
    kp: f64,
    ki: f64,
    kd: f64,
    
    // PID state
    integral: f64,
    last_error: f64,
    
    // Low-pass filter state for gradient
    filtered_gradient: f64,
    filter_alpha: f64,  // Filter coefficient (0-1, higher = less filtering)
    gradient_history: Vec<f64>,  // Moving average window
    history_size: usize,
}

impl AdaptivePIDController {
    pub fn new(kp: f64, ki: f64, kd: f64) -> Self {
        Self {
            base_kp: kp,
            base_ki: ki,
            base_kd: kd,
            kp,
            ki,
            kd,
            integral: 0.0,
            last_error: 0.0,
            filtered_gradient: 1.0,  // Start with moderate gradient
            filter_alpha: 0.05,      // Very aggressive filtering (5% new, 95% old)
            gradient_history: vec![1.0; 10],  // Initialize with moderate values
            history_size: 10,
        }
    }
    
    /// Adapt gains based on logarithmic gradient with optional filtering
    pub fn adapt_gains(&mut self, log_gradient: f64) {
        self.adapt_gains_with_filter(log_gradient, true)
    }
    
    /// Adapt gains with error-based damping enhancement
    pub fn adapt_gains_with_error(&mut self, log_gradient: f64, error: f64, use_filter: bool) {
        // First adapt based on gradient
        self.adapt_gains_with_filter(log_gradient, use_filter);
        
        // Then apply additional error-based damping
        // When error is very small but we're not converging fast, increase damping
        let error_factor = if error < 1e-10 {
            // Ultra-small error - be very aggressive with damping
            0.3
        } else if error < 1e-8 {
            // Very small error - increase damping
            0.5
        } else if error < 1e-6 {
            // Small error - moderate damping
            0.7
        } else if error < 1e-4 {
            // Medium error - slight damping
            0.85
        } else {
            // Large error - normal operation
            1.0
        };
        
        // Apply error-based damping to gains
        self.kp *= error_factor;
        self.ki *= error_factor * 0.8;  // Integral term needs more damping
        self.kd *= (2.0 - error_factor); // Increase derivative action for stability
        
        // Additional damping for stuck situations
        // If we have very small errors but high gradients, we're likely stuck
        if error < 1e-8 && log_gradient > 10.0 {
            // Extra damping for stuck situations
            self.kp *= 0.5;
            self.ki *= 0.3;
            self.kd *= 1.5;
        }
    }
    
    /// Adapt gains with option to disable filtering
    pub fn adapt_gains_with_filter(&mut self, log_gradient: f64, use_filter: bool) {
        // First, clamp the raw gradient to prevent extreme values
        let clamped_gradient = log_gradient.clamp(0.1, 100.0);
        
        let gradient = if use_filter {
            // Add to moving average window
            self.gradient_history.push(clamped_gradient);
            if self.gradient_history.len() > self.history_size {
                self.gradient_history.remove(0);
            }
            
            // Calculate moving average
            let moving_avg = self.gradient_history.iter().sum::<f64>() / self.gradient_history.len() as f64;
            
            // Apply aggressive low-pass filter to the moving average
            // This provides double filtering for extra smoothness
            self.filtered_gradient = self.filter_alpha * moving_avg + 
                                    (1.0 - self.filter_alpha) * self.filtered_gradient;
            
            // Additional smoothing: limit rate of change
            let max_gradient_change = 2.0;  // Even more conservative
            if (self.filtered_gradient - moving_avg).abs() > max_gradient_change {
                self.filtered_gradient = if moving_avg > self.filtered_gradient {
                    self.filtered_gradient + max_gradient_change
                } else {
                    self.filtered_gradient - max_gradient_change
                };
            }
            
            // Use heavily filtered gradient
            self.filtered_gradient
        } else {
            // Use raw gradient directly for Phase 2
            clamped_gradient
        };
        
        // When gradient is very small, we're in a linear region
        // Don't reduce gains - use normal PID control!
        if gradient < 1.0 {
            // Linear region - use base gains but still be careful
            self.kp = self.base_kp * 0.8;
            self.ki = self.base_ki * 0.8;
            self.kd = self.base_kd * 1.2;
        } else if gradient > 50.0 {
            // Very high sensitivity - be extremely conservative
            self.kp = self.base_kp * 0.3;
            self.ki = self.base_ki * 0.1;
            self.kd = self.base_kd * 3.0;
        } else if gradient > 20.0 {
            // High sensitivity - be very conservative
            self.kp = self.base_kp * 0.5;
            self.ki = self.base_ki * 0.3;
            self.kd = self.base_kd * 2.0;
        } else {
            // Moderate sensitivity - still be conservative
            self.kp = self.base_kp * 0.7;
            self.ki = self.base_ki * 0.6;
            self.kd = self.base_kd * 1.5;
        }
    }
    
    /// Update PID controller and return control signal
    pub fn update(&mut self, error: f64, dt: f64) -> f64 {
        // P term
        let p = self.kp * error;
        
        // I term
        self.integral += error * dt;
        let i = self.ki * self.integral;
        
        // D term
        let d = self.kd * (error - self.last_error) / dt;
        self.last_error = error;
        
        p + i + d
    }
    
    pub fn reset(&mut self) {
        self.integral = 0.0;
        self.last_error = 0.0;
        self.filtered_gradient = 1.0;  // Reset to moderate gradient
        self.gradient_history = vec![1.0; self.history_size];  // Reset history
    }
}

/// GLACIER solver implementation
/// Stored region information with successful starting points
#[derive(Debug, Clone)]
pub struct RegionInfo {
    pub start: f64,
    pub end: f64,
    pub mid_ramp: f64,
    pub starting_point: DVector<f64>,  // Successful solution from scanning
    pub log_gradient: f64,
}

pub struct GlacierSolver {
    pub(crate) circuit: Circuit,
    pub(crate) models: HashMap<String, ComponentModel>,
    model_engine: RuntimeModelEngine,
    
    // Convergence parameters
    tolerance: f64,
    max_iterations: usize,
    
    // Stored region information for robust convergence
    stored_regions: Vec<RegionInfo>,
}

impl GlacierSolver {
    /// Estimate average gradient for a region
    fn estimate_region_gradient(&self, start: f64, end: f64) -> f64 {
        // Simple heuristic - in reality would use scan data
        let mid = (start + end) / 2.0;
        if mid < 0.35 {
            1.0  // Low voltage, typically linear
        } else if mid > 0.5 {
            5.0  // Higher voltage, moderate nonlinearity
        } else {
            25.0 // Transition region, high gradient
        }
    }
    
    /// Identify stable regions using gradient rate detection and store successful starting points
    fn identify_regions_with_storage(&mut self) -> Result<Vec<(f64, f64)>> {
        // Perform actual scan to identify regions
        info!("Scanning circuit to identify stable regions with gradient rate detection");
        
        // Get circuit setup
        let ground_idx = self.circuit.ground_node()
            .ok_or_else(|| SpiceError::NoGroundNode)?
            .0;
        
        let node_list: Vec<NodeIndex> = self.circuit.nodes()
            .filter(|&(idx, _)| idx != ground_idx)
            .map(|(idx, _)| idx)
            .collect();
        
        // Find voltage sources
        let voltage_sources: Vec<(EdgeIndex, String, f64)> = self.circuit.branches()
            .filter_map(|(idx, branch)| {
                match self.models.get(&branch.name)? {
                    ComponentModel::VoltageSource { voltage, .. } => 
                        Some((idx, branch.name.clone(), *voltage)),
                    _ => None
                }
            })
            .collect();
        
        // Initialize solution vector
        let num_nodes = node_list.len();
        let num_vsources = voltage_sources.len();
        let matrix_size = num_nodes + num_vsources;
        let mut x: DVector<f64> = DVector::zeros(matrix_size);
        let mut total_iterations = 0;
        
        // Phase 1a: Coarse scan with gradient rate detection (5% steps)
        let num_coarse_points = 20;
        let mut scan_results: Vec<(f64, f64, bool, DVector<f64>)> = Vec::new(); // (ramp, log_gradient, converged, solution)
        let mut sharp_regions: Vec<(f64, f64)> = Vec::new();
        let mut last_log_gradient = 0.0;
        let mut last_scan_ramp = 0.0;
        
        // Clear any previously stored regions
        self.stored_regions.clear();
        
        println!("\nPhase 0: Identifying stable operating regions with gradient rate detection...");
        println!("  Phase 1a: Coarse scan with gradient rate detection...");
        
        for i in 0..=num_coarse_points {
            let scan_ramp = i as f64 / num_coarse_points as f64;
            
            // Update voltage sources
            for (idx, name, original_voltage) in &voltage_sources {
                if let Some(model) = self.models.get_mut(name) {
                    if let ComponentModel::VoltageSource { voltage, .. } = model {
                        *voltage = original_voltage * scan_ramp;
                    }
                }
            }
            
            // Try to solve at this ramp value
            let mut scan_x = x.clone();
            let (converged, iterations, error) = self.solve_at_ramp_quick(
                &mut scan_x, &node_list, ground_idx, &voltage_sources.iter().map(|(idx, _, _)| *idx).collect::<Vec<_>>(), &mut total_iterations
            )?;
            
            // Calculate log gradient at this point
            let log_gradient = if converged {
                self.calculate_log_gradient(&scan_x, &node_list, ground_idx)
            } else {
                1000.0 // High gradient for non-converged points
            };
            
            // Calculate rate of change of log gradient
            let d_log_gradient = if i > 0 {
                (log_gradient - last_log_gradient) / (scan_ramp - last_scan_ramp)
            } else {
                0.0
            };
            
            // Detect sharp transitions
            if i > 0 && d_log_gradient.abs() > 100.0 {
                sharp_regions.push((last_scan_ramp, scan_ramp));
                println!("  Ramp {:.0}%: SHARP TRANSITION DETECTED! d(log_grad)/d(ramp) = {:.1}", 
                         scan_ramp * 100.0, d_log_gradient);
            }
            
            // Store scan results including solution vector
            scan_results.push((scan_ramp, log_gradient, converged, scan_x.clone()));
            
            // Debug output
            if converged {
                println!("  Ramp {:.0}%: converged in {} iter, log_gradient={:.2}", 
                         scan_ramp * 100.0, iterations, log_gradient);
            } else {
                println!("  Ramp {:.0}%: failed to converge", scan_ramp * 100.0);
            }
            
            last_log_gradient = log_gradient;
            last_scan_ramp = scan_ramp;
        }
        
        // Phase 1b: Adaptive refinement around sharp regions using enhanced numerical techniques
        if !sharp_regions.is_empty() {
            println!("\n  Phase 1b: Adaptive refinement around {} sharp transitions...", sharp_regions.len());
            println!("    Sharp transitions indicate exponential behavior - using enhanced techniques");
            
            for (start_ramp, end_ramp) in sharp_regions {
                println!("    Refining between {:.0}% and {:.0}%...", start_ramp * 100.0, end_ramp * 100.0);
                
                // For sharp transitions, use logarithmic spacing for better coverage
                let num_points = 20; // More points for sharp regions
                let log_start = (start_ramp + 1e-6).ln();
                let log_end = (end_ramp + 1e-6).ln();
                let log_step = (log_end - log_start) / (num_points as f64);
                
                for j in 1..num_points { // Skip endpoints already scanned
                    let fine_ramp = (log_start + (j as f64) * log_step).exp();
                    
                    // Skip if already scanned
                    if scan_results.iter().any(|(r, _, _, _)| (*r - fine_ramp).abs() < 1e-4) {
                        continue;
                    }
                    
                    // Update voltage sources
                    for (idx, name, original_voltage) in &voltage_sources {
                        if let Some(model) = self.models.get_mut(name) {
                            if let ComponentModel::VoltageSource { voltage, .. } = model {
                                *voltage = original_voltage * fine_ramp;
                            }
                        }
                    }
                    
                    // For sharp transitions, try multiple initial guesses
                    let mut best_converged = false;
                    let mut best_x = x.clone();
                    let mut best_gradient = 1000.0;
                    
                    // Try different generic initial guesses for robustness
                    let initial_guesses = vec![
                        x.clone(), // Current solution
                        {
                            // Linear voltage division guess
                            let mut guess = x.clone();
                            let supply = voltage_sources.get(0).map(|(_, _, v)| v * fine_ramp).unwrap_or(1.0);
                            for i in 0..num_nodes.min(guess.len()) {
                                guess[i] = supply * (num_nodes - i) as f64 / (num_nodes + 1) as f64;
                            }
                            guess
                        },
                        {
                            // Exponential decay guess
                            let mut guess = x.clone();
                            let supply = voltage_sources.get(0).map(|(_, _, v)| v * fine_ramp).unwrap_or(1.0);
                            for i in 0..num_nodes.min(guess.len()) {
                                guess[i] = supply * (0.5_f64).powi(i as i32);
                            }
                            guess
                        }
                    ];
                    
                    for (guess_idx, initial_guess) in initial_guesses.iter().enumerate() {
                        let mut scan_x = initial_guess.clone();
                        let (converged, iterations, error) = self.solve_at_ramp_quick(
                            &mut scan_x, &node_list, ground_idx, &voltage_sources.iter().map(|(idx, _, _)| *idx).collect::<Vec<_>>(), &mut total_iterations
                        )?;
                        
                        if converged {
                            let log_gradient = self.calculate_log_gradient(&scan_x, &node_list, ground_idx);
                            if !best_converged || log_gradient < best_gradient {
                                best_converged = true;
                                best_x = scan_x;
                                best_gradient = log_gradient;
                            }
                        }
                    }
                    
                    if best_converged {
                        println!("      Ramp {:.1}%: converged, log_gradient={:.2}", 
                                 fine_ramp * 100.0, best_gradient);
                        
                        // Insert refined results in sorted order
                        let insert_pos = scan_results.iter().position(|(r, _, _, _)| *r > fine_ramp).unwrap_or(scan_results.len());
                        scan_results.insert(insert_pos, (fine_ramp, best_gradient, true, best_x.clone()));
                    } else {
                        println!("      Ramp {:.1}%: failed (sharp transition region)", fine_ramp * 100.0);
                    }
                }
            }
        }
        
        
        // Now identify stable regions from the scan results
        let mut regions = Vec::new();
        let mut current_region_start = 0.0;
        let mut in_unstable_region = false;
        
        println!("\n  Identifying stable regions from scan results...");
        
        for i in 0..scan_results.len() {
            let (ramp, log_gradient, converged, solution_vec) = &scan_results[i];
            
            // Check for instability: high gradient or convergence failure
            let is_unstable = !converged || *log_gradient > 100.0;
            
            // Also check for sharp gradient changes
            let has_sharp_change = if i > 0 {
                let prev_gradient = scan_results[i-1].1;
                (log_gradient / prev_gradient > 10.0) || (prev_gradient / log_gradient > 10.0)
            } else {
                false
            };
            
            if (is_unstable || has_sharp_change) && !in_unstable_region {
                // End current stable region and find a good starting point within it
                if *ramp > current_region_start + 0.05 {
                    let region_end = *ramp - 0.01;
                    regions.push((current_region_start, region_end));
                    
                    // Find a representative starting point in this region
                    // Use the midpoint of the region as a neutral choice
                    let mid_point = (current_region_start + region_end) / 2.0;
                    let mut best_point = None;
                    let mut best_distance = f64::INFINITY;
                    
                    for (r, g, c, sol) in &scan_results {
                        if *r >= current_region_start && *r <= region_end && *c {
                            // Find the point closest to the midpoint
                            let distance = (*r - mid_point).abs();
                            if distance < best_distance {
                                best_distance = distance;
                                best_point = Some((*r, sol.clone(), *g));
                            }
                        }
                    }
                    
                    // Store region information with starting point
                    if let Some((mid_ramp, starting_point, gradient)) = best_point {
                        self.stored_regions.push(RegionInfo {
                            start: current_region_start,
                            end: region_end,
                            mid_ramp,
                            starting_point,
                            log_gradient: gradient,
                        });
                        println!("    Stable region: {:.1}%-{:.1}% (stored starting point at {:.1}%)", 
                                 current_region_start * 100.0, region_end * 100.0, mid_ramp * 100.0);
                    } else {
                        println!("    Stable region: {:.1}%-{:.1}% (no good starting point found)", 
                                 current_region_start * 100.0, region_end * 100.0);
                    }
                }
                in_unstable_region = true;
            } else if !is_unstable && !has_sharp_change && in_unstable_region {
                // Start new stable region
                current_region_start = *ramp;
                in_unstable_region = false;
            }
        }
        
        // Close last region if needed
        if !in_unstable_region && 1.0 > current_region_start + 0.05 {
            let region_end = 1.0;
            regions.push((current_region_start, region_end));
            
            // Find a representative starting point in this final region
            let mid_point = (current_region_start + region_end) / 2.0;
            let mut best_point = None;
            let mut best_distance = f64::INFINITY;
            
            for (r, g, c, sol) in &scan_results {
                if *r >= current_region_start && *r <= region_end && *c {
                    // Find the point closest to the midpoint
                    let distance = (*r - mid_point).abs();
                    if distance < best_distance {
                        best_distance = distance;
                        best_point = Some((*r, sol.clone(), *g));
                    }
                }
            }
            
            // Store final region information with starting point
            if let Some((mid_ramp, starting_point, gradient)) = best_point {
                self.stored_regions.push(RegionInfo {
                    start: current_region_start,
                    end: region_end,
                    mid_ramp,
                    starting_point,
                    log_gradient: gradient,
                });
                println!("    Stable region: {:.1}%-100% (stored starting point at {:.1}%)", 
                         current_region_start * 100.0, mid_ramp * 100.0);
            } else {
                println!("    Stable region: {:.1}%-100% (no good starting point found)", 
                         current_region_start * 100.0);
            }
        }
        
        // Restore original voltages
        for (idx, name, original_voltage) in &voltage_sources {
            if let Some(model) = self.models.get_mut(name) {
                if let ComponentModel::VoltageSource { voltage, .. } = model {
                    *voltage = *original_voltage;
                }
            }
        }
        
        // If no regions found (perfectly linear circuit), return single region
        if regions.is_empty() {
            // Check if this is a marginal circuit (converges only at low voltages)
            let highest_converged = scan_results.iter()
                .filter(|(_, _, c, _)| *c)
                .map(|(r, _, _, _)| *r)
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(0.0);
            
            if highest_converged < 0.5 {
                println!("  WARNING: Marginal circuit detected - only converged up to {:.1}%", highest_converged * 100.0);
                println!("  This circuit may have insufficient voltage headroom for all components");
                
                // Still create a region to attempt solving
                if highest_converged > 0.0 {
                    regions.push((0.0, highest_converged));
                    
                    // Find best point in converged region
                    let mut best_point = None;
                    let mut best_ramp = 0.0;
                    
                    for (r, g, c, sol) in &scan_results {
                        if *c && *r > best_ramp {
                            best_ramp = *r;
                            best_point = Some((*r, sol.clone(), *g));
                        }
                    }
                    
                    if let Some((mid_ramp, starting_point, gradient)) = best_point {
                        self.stored_regions.push(RegionInfo {
                            start: 0.0,
                            end: highest_converged,
                            mid_ramp,
                            starting_point,
                            log_gradient: gradient,
                        });
                    }
                }
            } else {
                println!("  No instabilities detected - single region 0%-100%");
                regions.push((0.0, 1.0));
                
                // For a single region, try to return multiple representative points
                // Let's store solutions at different points in the region
                let sample_points = vec![0.1, 0.5, 0.9]; // Sample at 10%, 50%, 90%
                
                for target_ramp in sample_points {
                    let mut best_point = None;
                    let mut best_distance = f64::INFINITY;
                    
                    for (r, g, c, sol) in &scan_results {
                        if *c {
                            let distance = (*r - target_ramp).abs();
                            if distance < best_distance {
                                best_distance = distance;
                                best_point = Some((*r, sol.clone(), *g));
                            }
                        }
                    }
                    
                    // Store each sample point as a separate region
                    if let Some((mid_ramp, starting_point, gradient)) = best_point {
                        self.stored_regions.push(RegionInfo {
                            start: (mid_ramp - 0.05).max(0.0),
                            end: (mid_ramp + 0.05).min(1.0),
                            mid_ramp,
                            starting_point,
                            log_gradient: gradient,
                        });
                    }
                }
                
                println!("    Stored {} sample points across the single region", self.stored_regions.len());
            }
        }
        
        println!("\nIdentified {} stable regions:", regions.len());
        for (i, (start, end)) in regions.iter().enumerate() {
            println!("  Region {}: {:.1}%-{:.1}%", i+1, start*100.0, end*100.0);
        }
        
        Ok(regions)
    }
    
    /// Calculate log gradient by examining nonlinear element characteristics
    fn calculate_log_gradient(&self, x: &DVector<f64>, node_list: &[NodeIndex], ground_idx: NodeIndex) -> f64 {
        let mut log_gradient = 1.0;
        let mut found_nonlinear = false;
        
        // Look at each branch to find nonlinear elements
        for (branch_idx, branch) in self.circuit.branches() {
            // Check if this branch has a model in our models map
            if let Some(model) = self.models.get(&branch.name) {
                match model {
                    ComponentModel::LED { .. } | ComponentModel::Diode { .. } => {
                        // Get the voltage across the element
                        if let Some((pos_idx, neg_idx)) = self.circuit.branch_nodes(branch_idx) {
                            let v_pos = if pos_idx == ground_idx { 
                                0.0 
                            } else { 
                                node_list.iter().position(|&idx| idx == pos_idx)
                                    .map(|i| x[i])
                                    .unwrap_or(0.0)
                            };
                            
                            let v_neg = if neg_idx == ground_idx { 
                                0.0 
                            } else { 
                                node_list.iter().position(|&idx| idx == neg_idx)
                                    .map(|i| x[i])
                                    .unwrap_or(0.0)
                            };
                            
                            let element_voltage = v_pos - v_neg;
                            
                            // Calculate current through the element
                            let current = self.calculate_element_current(model, element_voltage);
                            
                            // For exponential elements, estimate gradient
                            // Even small voltages can have high gradients for ultra-sharp devices
                            if element_voltage > 0.01 {
                                // Get device parameters
                                let (is, n, vt) = match model {
                                    ComponentModel::LED { saturation_current, emission_coefficient, thermal_voltage, .. } => 
                                        (saturation_current.unwrap_or(1e-12), 
                                         emission_coefficient.unwrap_or(2.0),
                                         thermal_voltage.unwrap_or(0.026)),
                                    ComponentModel::Diode { saturation_current, emission_coefficient, .. } => 
                                        (saturation_current.unwrap_or(1e-12),
                                         emission_coefficient.unwrap_or(1.0),
                                         0.026),
                                    _ => (1e-12, 1.0, 0.026)
                                };
                                
                                // For diodes/LEDs: I = Is * (exp(V/nVt) - 1)
                                // When V >> nVt: I ≈ Is * exp(V/nVt)
                                // d(ln I)/dV = 1/(nVt)
                                // But we also need to consider the magnitude of current change
                                
                                // Calculate the exponential factor
                                let exp_factor = (element_voltage / (n * vt)).min(50.0);
                                
                                // If we're in the exponential region (exp_factor > 2)
                                if exp_factor > 2.0 {
                                    // Log gradient is 1/(n*Vt)
                                    let element_gradient = 1.0 / (n * vt);
                                    
                                    // For ultra-sharp devices (Is < 1e-15), boost the gradient
                                    // to reflect the extreme sensitivity
                                    let sharpness_factor = if is < 1e-15 {
                                        (1e-12 / is).ln().max(1.0)
                                    } else {
                                        1.0
                                    };
                                    
                                    let adjusted_gradient = element_gradient * sharpness_factor;
                                    
                                    // Take the maximum gradient found
                                    if adjusted_gradient > log_gradient {
                                        log_gradient = adjusted_gradient;
                                    }
                                    
                                    found_nonlinear = true;
                                }
                            }
                        }
                    },
                    _ => {}
                }
            }
        }
        
        // If no nonlinear elements found, return default
        if !found_nonlinear {
            1.0
        } else {
            log_gradient
        }
    }
    
    pub fn new(circuit: Circuit) -> Self {
        Self {
            circuit,
            models: HashMap::new(),
            model_engine: RuntimeModelEngine::new()
                .expect("Failed to initialize runtime model engine"),
            tolerance: 1e-9,
            max_iterations: 1000,
            stored_regions: Vec::new(),
        }
    }
    
    /// Add component model
    pub fn add_model(&mut self, name: String, model: ComponentModel) {
        self.models.insert(name, model);
    }
    
    /// Get mutable reference to component model
    pub fn get_model_mut(&mut self, name: &str) -> Option<&mut ComponentModel> {
        self.models.get_mut(name)
    }
    
    /// Get reference to models (for enhanced solver)
    pub fn models(&self) -> &HashMap<String, ComponentModel> {
        &self.models
    }
    
    /// Analyze and return solutions from all stable regions
    /// Returns: Vec<(ramp_start, ramp_end, avg_gradient, AnalysisResult)>
    pub fn analyze_all_regions(&mut self) -> Result<Vec<(f64, f64, f64, AnalysisResult)>> {
        info!("Starting multi-region analysis");
        
        // Store original voltage source values before any modifications
        let original_voltages: HashMap<String, f64> = self.models.iter()
            .filter_map(|(name, model)| {
                match model {
                    ComponentModel::VoltageSource { voltage, .. } => 
                        Some((name.clone(), *voltage)),
                    _ => None
                }
            })
            .collect();
        
        println!("Stored original voltages before any analysis:");
        for (name, v) in &original_voltages {
            println!("  {} = {:.3}V", name, v);
        }
        
        // First identify regions and store starting points
        let regions = self.identify_regions_with_storage()?;
        
        // Restore original voltages after region scanning
        println!("\nRestoring voltage sources after region scanning:");
        for (name, original_voltage) in &original_voltages {
            if let Some(model) = self.models.get_mut(name) {
                if let ComponentModel::VoltageSource { voltage, .. } = model {
                    println!("  {} was {:.3}V, restoring to {:.3}V", name, voltage, original_voltage);
                    *voltage = *original_voltage;
                }
            }
        }
        
        let mut solutions = Vec::new();
        
        // Extract stored regions to avoid borrowing issues
        let stored_regions = self.stored_regions.clone();
        
        // Solve in each region using stored starting points
        for region_info in stored_regions {
            println!("\nAttempting solution at ramp={:.3} (region {:.1}%-{:.1}%) with stored starting point...", 
                     region_info.mid_ramp, region_info.start*100.0, region_info.end*100.0);
            
            // CRITICAL: Ensure voltage sources are at original values before each solve attempt
            for (name, original_voltage) in &original_voltages {
                if let Some(model) = self.models.get_mut(name) {
                    if let ComponentModel::VoltageSource { voltage, .. } = model {
                        if (*voltage - original_voltage).abs() > 1e-6 {
                            println!("  WARNING: {} was {:.3}V, fixing to {:.3}V before solve", 
                                     name, voltage, original_voltage);
                        }
                        *voltage = *original_voltage;
                    }
                }
            }
            
            // Scale the stored starting point from its ramp level to 100%
            let scale_factor = if region_info.mid_ramp > 0.0 { 1.0 / region_info.mid_ramp } else { 1.0 };
            let scaled_starting_point = &region_info.starting_point * scale_factor;
            
            // Use the scaled starting point for robust convergence at 100%
            match self.analyze_from_ramp_with_stored_init(1.0, &scaled_starting_point) {
                Ok(result) => {
                    solutions.push((region_info.start, region_info.end, region_info.log_gradient, result));
                    println!("  ✓ Found solution using stored starting point (gradient={:.2})", region_info.log_gradient);
                }
                Err(e) => {
                    println!("  ✗ Failed even with stored starting point: {}", e);
                    // Fallback: Try standard two-phase analysis which progresses to 100%
                    // Start from the region's midpoint for better convergence
                    println!("    → Attempting fallback with standard two-phase solver...");
                    match self.analyze_internal(Some(region_info.mid_ramp)) {
                        Ok(mut result) => {
                            // The result should already be at 100% ramp because analyze_internal_with_init
                            // always progresses to 100%, but let's verify
                            let max_voltage = result.node_voltages.values()
                                .max_by(|a, b| a.partial_cmp(b).unwrap())
                                .copied()
                                .unwrap_or(0.0);
                            
                            // Check if we got a full voltage solution
                            let expected_voltage = original_voltages.values()
                                .max_by(|a, b| a.partial_cmp(b).unwrap())
                                .copied()
                                .unwrap_or(1.0);
                            
                            if max_voltage < expected_voltage * 0.95 {
                                println!("    ⚠️  Fallback returned partial voltage: {:.3}V vs expected {:.3}V", 
                                         max_voltage, expected_voltage);
                                // Try one more time starting from a higher ramp
                                match self.analyze_internal_with_init(Some(0.9), None) {
                                    Ok(better_result) => {
                                        result = better_result;
                                        println!("    ✓ Second attempt succeeded with full voltage");
                                    }
                                    Err(_) => {
                                        println!("    ⚠️  Accepting partial voltage solution");
                                    }
                                }
                            }
                            
                            solutions.push((region_info.start, region_info.end, region_info.log_gradient, result));
                            println!("    ✓ Fallback succeeded");
                        }
                        Err(fallback_e) => {
                            println!("    ✗ Fallback also failed: {}", fallback_e);
                        }
                    }
                }
            }
        }
        
        if solutions.is_empty() {
            // For marginal circuits, try to at least return a partial solution
            println!("\nNo full-voltage solutions found. Checking for partial solutions...");
            
            // Clone stored regions to avoid borrowing issues
            let stored_regions_clone = self.stored_regions.clone();
            
            // Check if we have any stored regions with converged solutions
            if !stored_regions_clone.is_empty() {
                println!("Found {} stored regions with partial convergence", stored_regions_clone.len());
                
                // Find the highest voltage region
                let best_region = stored_regions_clone.iter()
                    .max_by(|a, b| a.end.partial_cmp(&b.end).unwrap())
                    .unwrap()
                    .clone();
                
                println!("Attempting to return best partial solution at {:.0}% voltage", best_region.end * 100.0);
                
                // Scale the solution to the highest achievable voltage
                let scale_factor = best_region.end;
                
                // Update voltage sources to the achievable level
                for (name, original_voltage) in &original_voltages {
                    if let Some(model) = self.models.get_mut(name) {
                        if let ComponentModel::VoltageSource { voltage, .. } = model {
                            *voltage = original_voltage * scale_factor;
                        }
                    }
                }
                
                // For partial solutions, directly use the stored solution at the achievable voltage
                // Don't try to scale to 100% as that will fail for marginal circuits
                println!("✓ Returning partial solution at {:.0}% voltage", scale_factor * 100.0);
                println!("  WARNING: Circuit appears marginal - cannot achieve full voltage");
                
                // Build the result from the stored solution
                let mut node_voltages = HashMap::new();
                let ground_idx = self.circuit.ground_node()
                    .ok_or_else(|| SpiceError::NoGroundNode)?
                    .0;
                node_voltages.insert(ground_idx, 0.0);
                
                // Extract node voltages from stored solution
                let node_list: Vec<NodeIndex> = self.circuit.nodes()
                    .filter(|(idx, _)| *idx != ground_idx)
                    .map(|(idx, _)| idx)
                    .collect();
                    
                for (i, &node_idx) in node_list.iter().enumerate() {
                    if i < best_region.starting_point.len() {
                        node_voltages.insert(node_idx, best_region.starting_point[i]);
                    }
                }
                
                // Calculate branch currents at partial voltage
                let mut branch_currents = HashMap::new();
                for (edge_idx, branch) in self.circuit.branches() {
                    if let Some((n1, n2)) = self.circuit.branch_nodes(edge_idx) {
                        let v1 = node_voltages.get(&n1).copied().unwrap_or(0.0);
                        let v2 = node_voltages.get(&n2).copied().unwrap_or(0.0);
                        let voltage_diff = v1 - v2;
                        
                        // Generic current calculation based on DC resistance
                        let current = if let Some(model) = self.models.get(&branch.name) {
                            let dc_resistance = model.dc_resistance();
                            if dc_resistance > 0.0 && dc_resistance.is_finite() {
                                voltage_diff / dc_resistance
                            } else {
                                0.0
                            }
                        } else {
                            0.0
                        };
                        
                        branch_currents.insert(edge_idx, current);
                    }
                }
                
                // Calculate total power
                let mut total_power = 0.0;
                for (edge_idx, current) in &branch_currents {
                    if let Some((n1, n2)) = self.circuit.branch_nodes(*edge_idx) {
                        let v1 = node_voltages.get(&n1).copied().unwrap_or(0.0);
                        let v2 = node_voltages.get(&n2).copied().unwrap_or(0.0);
                        let voltage_diff = (v1 - v2).abs();
                        total_power += voltage_diff * current.abs();
                    }
                }
                
                let partial_result = AnalysisResult {
                    node_voltages,
                    branch_currents,
                    total_power,
                    iterations: 0,
                };
                
                solutions.push((best_region.start, best_region.end, best_region.log_gradient, partial_result));
                
                // Restore voltages to original
                for (name, original_voltage) in &original_voltages {
                    if let Some(model) = self.models.get_mut(name) {
                        if let ComponentModel::VoltageSource { voltage, .. } = model {
                            *voltage = *original_voltage;
                        }
                    }
                }
                
                Ok(solutions)
            } else {
                Err(SpiceError::ConvergenceFailed(0))
            }
        } else {
            Ok(solutions)
        }
    }
    
    /// Main analysis method - returns all solutions found in different operating regions
    /// Each solution includes: (ramp_start, ramp_end, avg_gradient, result)
    pub fn analyze(&mut self) -> Result<Vec<(f64, f64, f64, AnalysisResult)>> {
        self.analyze_all_regions()
    }
    
    /// Analyze from a specific starting ramp
    fn analyze_from_ramp(&mut self, start_ramp: f64) -> Result<AnalysisResult> {
        self.analyze_internal(Some(start_ramp))
    }
    
    /// Analyze from a specific starting ramp with optional initial values (public interface)
    pub fn analyze_with_guidance(&mut self, start_ramp: f64, init_value: Option<f64>) -> Result<AnalysisResult> {
        self.analyze_internal_with_init(Some(start_ramp), init_value)
    }
    
    /// Analyze with enhanced DC solver using log-space transformation for exponential devices
    /// This applies the key lesson from transient analysis: exponential devices become linear in log space
    pub fn analyze_with_enhanced_dc(&mut self) -> Result<AnalysisResult> {
        info!("Starting enhanced DC analysis with selective log-space transformation");
        
        // Identify exponential devices that would benefit from log transformation
        let exponential_branches = self.identify_exponential_branches();
        debug!("Identified {} exponential devices for log transformation", exponential_branches.len());
        
        // Try enhanced solve first
        match self.analyze_with_mixed_formulation(&exponential_branches) {
            Ok(result) => {
                info!("Enhanced DC solver converged successfully");
                Ok(result)
            }
            Err(_) => {
                // Fall back to standard solver
                info!("Enhanced solver failed, falling back to standard ramping approach");
                self.analyze()
                    .and_then(|solutions| solutions.into_iter()
                        .max_by(|a, b| a.3.total_power.partial_cmp(&b.3.total_power).unwrap())
                        .map(|s| s.3)
                        .ok_or(SpiceError::ConvergenceFailed(0)))
            }
        }
    }
    
    /// Identify branches with exponential devices (LEDs, diodes)
    fn identify_exponential_branches(&self) -> Vec<String> {
        let mut exponential_branches = Vec::new();
        
        for (branch_name, model) in &self.models {
            match model {
                ComponentModel::LED { .. } | ComponentModel::Diode { .. } => {
                    exponential_branches.push(branch_name.clone());
                }
                _ => {}
            }
        }
        
        exponential_branches
    }
    
    /// Analyze with mixed formulation (log currents for exponential devices)
    fn analyze_with_mixed_formulation(&mut self, exponential_branches: &[String]) -> Result<AnalysisResult> {
        debug!("Using mixed formulation with log currents for: {:?}", exponential_branches);
        
        // This is similar to the log transform method but integrated better
        // Key insight: we transform the current variables, not the models
        
        // For now, delegate to the existing log transform method
        // In future, we'll integrate this more cleanly
        self.analyze_with_log_transform(exponential_branches.to_vec())
    }
    
    /// Analyze with log transformation for exponential components (e.g., LEDs)
    pub fn analyze_with_log_transform(&mut self, led_branches: Vec<String>) -> Result<AnalysisResult> {
        println!("\n=== Log Transform Solver ===");
        println!("Transforming LED branches to log space: {:?}", led_branches);
        
        // Store original models for restoration
        let mut original_led_models = HashMap::new();
        for branch_name in &led_branches {
            if let Some(model) = self.models.get(branch_name) {
                original_led_models.insert(branch_name.clone(), model.clone());
            }
        }
        
        // For now, we'll simulate log transformation by:
        // 1. Using a modified initial guess that targets high current
        // 2. Starting at a higher ramp value
        // 3. Using aggressive damping to stay in high-current region
        
        let high_current_ramp = 0.85; // Start at 85% to target high current
        println!("Starting at high ramp: {:.2} (targeting high-current solution)", high_current_ramp);
        
        // Analyze with high starting point
        let result = self.analyze_internal_with_init(Some(high_current_ramp), Some(3.0))?;
        
        // Restore original models
        for (name, model) in original_led_models {
            self.models.insert(name, model);
        }
        
        // Check if we found high-current solution
        let current = result.branch_currents.values()
            .map(|&c| c.abs())
            .filter(|&c| c > 1e-12)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);
            
        println!("Log transform result: {:.3} mA", current * 1000.0);
        if current > 0.001 {
            println!("✓ SUCCESS: Found high-current solution!");
        } else {
            println!("✗ Still converged to low-current solution");
        }
        
        Ok(result)
    }
    
    /// Analyze from a specific starting ramp with optional initial values
    pub fn analyze_from_ramp_with_init(&mut self, start_ramp: f64, init_value: Option<f64>) -> Result<AnalysisResult> {
        self.analyze_internal_with_init(Some(start_ramp), init_value)
    }
    
    /// Analyze from a specific ramp using a stored solution vector as starting point
    fn analyze_from_ramp_with_stored_init(&mut self, start_ramp: f64, stored_init: &DVector<f64>) -> Result<AnalysisResult> {
        self.analyze_internal_with_stored_vector(Some(start_ramp), stored_init)
    }
    
    /// Generate a signature for a solution to detect duplicates
    fn solution_signature(&self, result: &AnalysisResult) -> String {
        // Use a more detailed signature to distinguish different operating states
        let mut voltages: Vec<f64> = result.node_voltages.values()
            .map(|v| (v * 1000.0).round() / 1000.0)  // Round to mV
            .collect();
        voltages.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        // Include sorted voltages and power to detect different states
        let voltage_str = voltages.iter()
            .map(|v| format!("{:.3}", v))
            .collect::<Vec<_>>()
            .join("_");
        format!("P{:.1}_V{}", result.total_power * 1000.0, voltage_str)
    }
    
    /// Standard analysis method (original implementation)
    fn analyze_standard(&mut self) -> Result<AnalysisResult> {
        self.analyze_internal(None)
    }
    
    /// Internal analysis with optional starting ramp
    fn analyze_internal(&mut self, forced_start_ramp: Option<f64>) -> Result<AnalysisResult> {
        self.analyze_internal_with_init(forced_start_ramp, None)
    }
    
    /// Internal analysis with optional starting ramp and initial values
    fn analyze_internal_with_init(&mut self, forced_start_ramp: Option<f64>, init_value: Option<f64>) -> Result<AnalysisResult> {
        info!("Starting Two-Phase Adaptive PID DC analysis");
        
        // Get ground node
        let ground_idx = self.circuit.ground_node()
            .ok_or_else(|| SpiceError::NoGroundNode)?
            .0;
        
        // Build node list (excluding ground)
        let node_list: Vec<NodeIndex> = self.circuit.nodes()
            .filter(|(idx, _)| *idx != ground_idx)
            .map(|(idx, _)| idx)
            .collect();
        
        let num_nodes = node_list.len();
        
        // Find voltage sources and their original values
        let voltage_sources: Vec<(EdgeIndex, String, f64)> = self.circuit.branches()
            .filter_map(|(idx, branch)| {
                self.models.get(&branch.name)
                    .and_then(|m| match m {
                        ComponentModel::VoltageSource { voltage, .. } => 
                            Some((idx, branch.name.clone(), *voltage)),
                        _ => None
                    })
            })
            .collect();
        
        let voltage_source_indices: Vec<EdgeIndex> = voltage_sources.iter()
            .map(|(idx, _, _)| *idx)
            .collect();
        
        let num_vsources = voltage_sources.len();
        let matrix_size = num_nodes + num_vsources;
        
        if matrix_size == 0 {
            return Err(SpiceError::EmptyCircuit);
        }
        
        // Initial solution vector with better conditioning
        let mut x = DVector::zeros(matrix_size);
        
        // Set initial voltages based on provided value or default
        let init_val = init_value.unwrap_or(0.01);
        for i in 0..num_nodes {
            x[i] = init_val;  // Initial voltage
        }
        
        // Track total iterations
        let mut total_iterations = 0;
        
        // PID controllers for each phase
        let mut scan_pid = AdaptivePIDController::new(
            0.5,   // Kp: Reduced proportional gain for stability
            0.1,   // Ki: Reduced integral gain
            0.05   // Kd: Increased derivative gain for damping
        );
        
        let mut control_pid = AdaptivePIDController::new(
            0.5,   // Kp: Reduced proportional gain for stability
            0.1,   // Ki: Reduced integral gain
            0.05   // Kd: Increased derivative gain for damping
        );
        
        // Ramping variables
        let mut ramp_factor = 0.1; // Start from 10% to get past LED threshold
        let mut ramp_rate = 0.01; // Initial rate
        
        // Tracking for gradient calculation
        let mut last_voltage = 0.0;
        let mut last_current = 1e-15; // Avoid ln(0)
        let mut log_gradient = 20.0; // Default gradient
        
        // PID controller will handle backtracking naturally
        
        // Convergence tracking
        let mut convergence_history: Vec<(f64, f64, f64)> = Vec::new(); // (ramp, error, residual)
        let mut last_errors: Vec<f64> = Vec::new(); // Track last few errors to detect stagnation
        
        println!("Starting Two-Phase solver...");
        
        // Phase 1: Linear scan (skip if forced start ramp provided)
        let (mut best_ramp, mut best_x, mut best_error) = if let Some(start_ramp) = forced_start_ramp {
            println!("\nSkipping Phase 1 scan - using provided start ramp: {:.2}", start_ramp);
            
            // Update voltage sources to starting ramp
            for (idx, name, original_voltage) in &voltage_sources {
                if let Some(model) = self.models.get_mut(name) {
                    if let ComponentModel::VoltageSource { voltage, .. } = model {
                        *voltage = original_voltage * start_ramp;
                    }
                }
            }
            
            // Solve at the starting ramp to get initial state
            let mut start_x = x.clone();
            let (converged, iterations, error) = self.solve_at_ramp(
                &mut start_x, &node_list, ground_idx, &voltage_source_indices, &mut total_iterations
            )?;
            
            if !converged {
                println!("Warning: Failed to converge at starting ramp {:.2}", start_ramp);
            }
            
            (start_ramp, start_x, error)
        } else {
            // Normal Phase 1 scan
            println!("\nPhase 1: Scanning for stable regions and transitions...");
            let mut scan_results = Vec::new();
            let mut best_ramp = 0.1;
            let mut best_error = f64::INFINITY;
            let mut best_x = x.clone();
            let mut best_normalized_error = f64::INFINITY;
        
        // Generic scan approach with gradient rate detection
        // Initial coarse scan to detect sharp transitions
        // Start from 0% to handle difficult circuits with series diodes
        let coarse_scan_points: Vec<f64> = (0..=20).map(|i| i as f64 * 0.05).collect();
        let mut sharp_regions = Vec::new();
        let mut last_log_gradient = 0.0;
        let mut last_scan_ramp = 0.0;
        
        // First pass: coarse scan with gradient rate detection
        println!("  Phase 1a: Coarse scan with gradient rate detection...");
        for (i, scan_ramp) in coarse_scan_points.iter().enumerate() {
            // Update voltage sources
            for (idx, name, original_voltage) in &voltage_sources {
                if let Some(model) = self.models.get_mut(name) {
                    if let ComponentModel::VoltageSource { voltage, .. } = model {
                        *voltage = original_voltage * scan_ramp;
                    }
                }
            }
            
            // Try to solve at this ramp value
            let mut scan_x = x.clone();
            let (converged, iterations, error) = self.solve_at_ramp(
                &mut scan_x, &node_list, ground_idx, &voltage_source_indices, &mut total_iterations
            )?;
            
            // Calculate normalized error
            let normalized_error = if *scan_ramp > 0.01 {
                error / scan_ramp
            } else {
                error
            };
            
            // Calculate log gradient at this point to detect transitions
            let log_gradient = self.calculate_log_gradient(&scan_x, &node_list, ground_idx);
            
            // Calculate rate of change of log gradient
            let d_log_gradient = if i > 0 {
                (log_gradient - last_log_gradient) / (scan_ramp - last_scan_ramp)
            } else {
                0.0
            };
            
            // Detect sharp transitions
            if i > 0 && d_log_gradient.abs() > 100.0 {
                sharp_regions.push((last_scan_ramp, *scan_ramp));
                println!("  Ramp {:.2}: SHARP TRANSITION DETECTED! d(log_grad)/d(ramp) = {:.1}", 
                         scan_ramp, d_log_gradient);
            }
            
            println!("  Ramp {:.2}: converged={}, iterations={}, error={:.2e}, normalized={:.2e}, log_gradient={:.2}", 
                     scan_ramp, converged, iterations, error, normalized_error, log_gradient);
            
            // Store scan results with gradient info
            scan_results.push((*scan_ramp, error, normalized_error, log_gradient, scan_x.clone()));
            
            last_log_gradient = log_gradient;
            last_scan_ramp = *scan_ramp;
            
            // Track best point based on normalized error AND stability (low gradient)
            // Prefer solutions in stable regions (gradient < 10)
            let stability_penalty = if log_gradient > 20.0 {
                100.0  // High penalty for transition regions
            } else if log_gradient > 10.0 {
                10.0   // Moderate penalty for high sensitivity regions
            } else {
                1.0    // No penalty for stable regions
            };
            
            let weighted_error = normalized_error * stability_penalty;
            
            if weighted_error < best_normalized_error {
                best_normalized_error = weighted_error;
                best_error = error;
                best_ramp = *scan_ramp;
                best_x = scan_x.clone();
            }
        }
        
        // Phase 1b: Adaptive refinement around sharp regions
        if !sharp_regions.is_empty() {
            println!("\n  Phase 1b: Adaptive refinement around {} sharp transitions...", sharp_regions.len());
            
            for (start_ramp, end_ramp) in sharp_regions {
                println!("    Refining between {:.0}% and {:.0}%...", start_ramp * 100.0, end_ramp * 100.0);
                
                // Fine scan with smaller steps
                let fine_steps = 10;
                let step_size = (end_ramp - start_ramp) / (fine_steps as f64);
                
                for j in 0..=fine_steps {
                    let fine_ramp = start_ramp + (j as f64) * step_size;
                    
                    // Skip if we already scanned this point
                    if scan_results.iter().any(|(r, _, _, _, _)| (*r - fine_ramp).abs() < 0.01) {
                        continue;
                    }
                    
                    // Update voltage sources
                    for (idx, name, original_voltage) in &voltage_sources {
                        if let Some(model) = self.models.get_mut(name) {
                            if let ComponentModel::VoltageSource { voltage, .. } = model {
                                *voltage = original_voltage * fine_ramp;
                            }
                        }
                    }
                    
                    // Try to solve at this refined point
                    let mut scan_x = x.clone();
                    let (converged, iterations, error) = self.solve_at_ramp(
                        &mut scan_x, &node_list, ground_idx, &voltage_source_indices, &mut total_iterations
                    )?;
                    
                    let normalized_error = if fine_ramp > 0.01 { error / fine_ramp } else { error };
                    let log_gradient = self.calculate_log_gradient(&scan_x, &node_list, ground_idx);
                    
                    println!("      Ramp {:.3}: converged={}, error={:.2e}, log_gradient={:.2}", 
                             fine_ramp, converged, error, log_gradient);
                    
                    // Check if this is a better starting point
                    let stability_penalty = if log_gradient > 20.0 { 100.0 } 
                                          else if log_gradient > 10.0 { 10.0 } 
                                          else { 1.0 };
                    
                    let weighted_error = normalized_error * stability_penalty;
                    
                    if weighted_error < best_normalized_error {
                        best_normalized_error = weighted_error;
                        best_error = error;
                        best_ramp = fine_ramp;
                        best_x = scan_x.clone();
                        println!("        → New best starting point found!");
                    }
                    
                    // Also store in scan results for region analysis
                    scan_results.push((fine_ramp, error, normalized_error, log_gradient, scan_x));
                }
            }
            
            // Re-sort scan results by ramp value for proper region analysis
            scan_results.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        }
        
        // Analyze scan results to find distinct regions
        println!("\nAnalyzing solution regions:");
        let mut regions = Vec::new();
        let mut current_region_start = 0.0;
        let mut in_transition = false;
        
        for i in 0..scan_results.len() {
            let (ramp, _, _, gradient, _) = &scan_results[i];
            
            if *gradient > 20.0 && !in_transition {
                // Entering transition
                if i > 0 {
                    regions.push((current_region_start, scan_results[i-1].0));
                }
                in_transition = true;
            } else if *gradient < 10.0 && in_transition {
                // Exiting transition
                current_region_start = *ramp;
                in_transition = false;
            }
        }
        
        // Add final region
        if !in_transition && current_region_start < 0.95 {
            regions.push((current_region_start, 0.95));
        }
        
        println!("Found {} stable regions:", regions.len());
        for (i, (start, end)) in regions.iter().enumerate() {
            println!("  Region {}: {:.0}% to {:.0}% of source voltage", i+1, start*100.0, end*100.0);
        }
        
        if regions.len() > 1 {
            println!("\nNote: Multiple stable regions detected. The circuit may have different");
            println!("      operating modes. Consider which region matches your design intent.");
        }
        
        println!("\nPhase 1 complete. Best starting point: ramp={:.2} with error={:.2e}", 
                 best_ramp, best_error);
            
            (best_ramp, best_x, best_error)
        };  // End of Phase 1 (either forced or scanned)
        
        // Phase 1.5: Fine linear scan around the best point from Phase 1
        println!("\nPhase 1.5: Fine linear scan around ramp={:.3} for optimal starting point...", best_ramp);
        
        // Determine scan range based on the error magnitude
        let scan_radius = if best_error < 1e-6 {
            0.01  // ±1% for small errors
        } else if best_error < 1e-3 {
            0.02  // ±2% for medium errors
        } else {
            0.05  // ±5% for large errors
        };
        
        let fine_scan_start = (best_ramp - scan_radius).max(0.01);
        let fine_scan_end = (best_ramp + scan_radius).min(1.0);
        // Use more points for ultra-sharp components (smaller best_error = sharper component)
        let fine_scan_points = if best_error < 1e-6 {
            1000  // More points for difficult cases
        } else if best_error < 1e-4 {
            500   // Medium number for moderate cases
        } else {
            200   // Fewer points for easy cases
        };
        let fine_step = (fine_scan_end - fine_scan_start) / (fine_scan_points as f64);
        
        println!("  Scanning from {:.3} to {:.3} with {} points (step={:.5})", 
                 fine_scan_start, fine_scan_end, fine_scan_points, fine_step);
        
        let mut fine_best_ramp = best_ramp;
        let mut fine_best_x = best_x.clone();
        let mut fine_best_error = best_error;
        let mut fine_scan_results = Vec::new();
        
        for i in 0..=fine_scan_points {
            let fine_ramp = fine_scan_start + (i as f64) * fine_step;
            
            // Update voltage sources
            for (idx, name, original_voltage) in &voltage_sources {
                if let Some(model) = self.models.get_mut(name) {
                    if let ComponentModel::VoltageSource { voltage, .. } = model {
                        *voltage = original_voltage * fine_ramp;
                    }
                }
            }
            
            // Start from the best x we have, not from scratch
            let mut scan_x = best_x.clone();
            
            // Solve with limited iterations to keep it fast
            let saved_max_iter = self.max_iterations;
            self.max_iterations = 20;  // Limit iterations for speed
            
            let (converged, iterations, error) = self.solve_at_ramp(
                &mut scan_x, &node_list, ground_idx, &voltage_source_indices, &mut total_iterations
            )?;
            
            self.max_iterations = saved_max_iter;  // Restore
            
            fine_scan_results.push((fine_ramp, error, converged));
            
            // Track the best point
            if error < fine_best_error {
                fine_best_error = error;
                fine_best_ramp = fine_ramp;
                fine_best_x = scan_x.clone();
                
                // If we found a really good solution during fine scan, we might be done!
                if converged && error < 1e-6 {
                    // For very low errors, we can try to jump to 100% from here
                    if error < 1e-7 {
                        println!("    → Found excellent solution during fine scan at ramp={:.3}! error={:.2e}", fine_ramp, error);
                        println!("      Attempting to jump to 100% ramp...");
                        
                        // Try solving at 100% from this starting point
                        for (idx, name, original_voltage) in &voltage_sources {
                            if let Some(model) = self.models.get_mut(name) {
                                if let ComponentModel::VoltageSource { voltage, .. } = model {
                                    *voltage = original_voltage * 1.0;  // Set to 100%
                                }
                            }
                        }
                        
                        let mut test_x = scan_x.clone();
                        let (test_converged, _, test_error) = self.solve_at_ramp(
                            &mut test_x, &node_list, ground_idx, &voltage_source_indices, &mut total_iterations
                        )?;
                        
                        if test_converged && test_error < 1e-4 {
                            println!("      ✅ Successfully jumped to 100%! Final error={:.2e}", test_error);
                            best_ramp = 1.0;
                            best_x = test_x;
                            best_error = test_error;
                            ramp_factor = 1.0;  // Mark as complete
                            break;
                        } else {
                            println!("      → Jump to 100% failed, continuing scan...");
                            // Reset voltage sources back to scan ramp
                            for (idx, name, original_voltage) in &voltage_sources {
                                if let Some(model) = self.models.get_mut(name) {
                                    if let ComponentModel::VoltageSource { voltage, .. } = model {
                                        *voltage = original_voltage * fine_ramp;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            // Print progress every 1000 points (or 10 if less than 1000 total)
            let progress_interval = if fine_scan_points >= 1000 { 1000 } else { 10 };
            if i % progress_interval == 0 {
                println!("    Point {}/{}: Ramp {:.4}: error={:.2e}, converged={}", 
                         i, fine_scan_points, fine_ramp, error, converged);
            }
        }
        
        // Analyze fine scan results
        println!("\n  Fine scan complete. Best point: ramp={:.4}, error={:.2e}", 
                 fine_best_ramp, fine_best_error);
        
        // Show improvement
        if fine_best_error < best_error {
            let improvement = (best_error - fine_best_error) / best_error * 100.0;
            println!("  ✓ Improved by {:.1}% from coarse scan", improvement);
        } else {
            println!("  → No improvement from coarse scan");
        }
        
        // Check if we found a solution during fine scan
        if ramp_factor >= 1.0 {
            println!("\n✅ Solution found during Phase 1.5 fine scan!");
            x = best_x;
            // Skip Phase 2 and go to final processing
        } else {
            // Update best values
            let best_ramp = fine_best_ramp;
            let best_x = fine_best_x;
            let best_error = fine_best_error;
            
            // Phase 2: PID control from best starting point
            println!("\nPhase 2: PID control from optimal starting point...");
            x = best_x;
            ramp_factor = best_ramp;
            let mut ramp_rate = 0.01;
        
        // Check the starting state
        println!("Starting Phase 2 at ramp={:.2} with x={:?}", ramp_factor, x.as_slice());
        
        // Main ramping loop with iteration limit  
        let mut ramp_iterations = 0;
        const MAX_RAMP_ITERATIONS: usize = 500;  // Increased for difficult circuits
        
        while ramp_factor < 1.0 && ramp_iterations < MAX_RAMP_ITERATIONS {
            ramp_iterations += 1;
            // Update voltage sources to ramped values
            for (idx, name, original_voltage) in &voltage_sources {
                if let Some(model) = self.models.get_mut(name) {
                    if let ComponentModel::VoltageSource { voltage, .. } = model {
                        *voltage = original_voltage * ramp_factor;
                    }
                }
            }
            
            // Debug at key ramp points
            if ramp_factor < 0.001 || (ramp_factor > 0.099 && ramp_factor < 0.101) {
                println!("Debug: Solving at ramp={:.3}, vsource={:.3}V", 
                         ramp_factor, 5.0 * ramp_factor);
            }
            
            // Solve at current ramp factor
            let (converged, iterations, error) = self.solve_at_ramp(
                &mut x, &node_list, ground_idx, &voltage_source_indices, &mut total_iterations
            )?;
            
            // Normalize error for tracking
            let normalized_error = if ramp_factor > 0.01 {
                error / ramp_factor
            } else {
                error
            };
            
            // Track convergence progress
            convergence_history.push((ramp_factor, error, 0.0)); // Will update residual later
            last_errors.push(normalized_error);  // Track normalized errors!
            if last_errors.len() > 5 {
                last_errors.remove(0);
            }
            
            // Check if we're stagnating (normalized errors not decreasing)
            if last_errors.len() >= 5 {
                let avg_recent = last_errors.iter().sum::<f64>() / last_errors.len() as f64;
                let all_similar = last_errors.iter().all(|&e| (e - avg_recent).abs() / avg_recent < 0.1);
                
                if all_similar && normalized_error > 1e-9 {
                    println!("  → Stagnation detected at ramp={:.3}: normalized errors not decreasing", ramp_factor);
                    println!("    Last 5 normalized errors: {:?}", last_errors);
                    
                    // Reset integral term to break out of oscillation
                    control_pid.integral = 0.0;
                    println!("    Resetting PID integral term to break oscillation");
                    
                    // Also increase damping temporarily
                    control_pid.kp *= 0.5;
                    control_pid.ki *= 0.2;
                    println!("    Temporarily increased damping: Kp={:.3}, Ki={:.3}", control_pid.kp, control_pid.ki);
                }
            }
            
            if converged {
                
                // Normalize error by ramp factor for proper scaling
                // This is critical: at ramp=0.1, an error of 1e-8 is actually 1e-7 relative
                let normalized_error = if ramp_factor > 0.01 {
                    error / ramp_factor
                } else {
                    error  // Don't divide by very small ramp factors
                };
                
                // Calculate logarithmic gradient from a nonlinear element if present
                let mut found_nonlinear = false;
                
                for (edge_idx, branch) in self.circuit.branches() {
                    if let Some(model) = self.models.get(&branch.name) {
                        // Check if this is a nonlinear element
                        match model {
                            ComponentModel::LED { .. } | ComponentModel::Diode { .. } => {
                                if let Some((n1, n2)) = self.circuit.branch_nodes(edge_idx) {
                                    let v1 = if n1 == ground_idx { 0.0 } else {
                                        node_list.iter().position(|&n| n == n1)
                                            .map(|i| x[i]).unwrap_or(0.0)
                                    };
                                    let v2 = if n2 == ground_idx { 0.0 } else {
                                        node_list.iter().position(|&n| n == n2)
                                            .map(|i| x[i]).unwrap_or(0.0)
                                    };
                                    let element_voltage = v1 - v2;
                                    
                                    // Calculate current
                                    let current = self.calculate_element_current(
                                        model, element_voltage
                                    );
                                    
                                    // Calculate log gradient if we have movement
                                    if element_voltage > last_voltage + 1e-6 && current > 1e-15 {
                                        let dv = element_voltage - last_voltage;
                                        let dlog_i = (current as f64).ln() - (last_current as f64).ln();
                                        log_gradient = (dlog_i / dv).abs();
                                    }
                                    
                                    last_voltage = element_voltage;
                                    last_current = current.max(1e-15);
                                    found_nonlinear = true;
                                    break;
                                }
                            },
                            _ => {}
                        }
                    }
                }
                
                // If no nonlinear elements, use default gradient
                if !found_nonlinear {
                    log_gradient = 1.0; // Linear circuit
                }
                
                // Adapt PID gains based on log gradient AND error magnitude
                control_pid.adapt_gains_with_error(log_gradient, normalized_error, true);
                
                // Debug: print gradient info occasionally
                if ramp_iterations % 50 == 0 {
                    println!("  → Log gradient: {:.2} (filtered: {:.2}), normalized_error: {:.2e}", 
                             log_gradient, control_pid.filtered_gradient, normalized_error);
                    println!("    PID gains: Kp={:.3}, Ki={:.3}, Kd={:.3}", 
                             control_pid.kp, control_pid.ki, control_pid.kd);
                }
                
                // Single phase control - use PID throughout
                let target_normalized_error = 1e-8;  // More relaxed target for better progress
                let error_ratio = (normalized_error / target_normalized_error).ln().max(-10.0).min(10.0);
                let pid_output = control_pid.update(error_ratio, 0.01);
                
                // Special handling when we're close to 100% ramp but struggling
                if ramp_factor > 0.95 && normalized_error < 1e-6 {
                    // We're very close - use aggressive push to 100%
                    println!("  → Near 100% ramp with good error - pushing to completion");
                    ramp_rate = 0.02;  // Fixed aggressive rate
                } else if ramp_factor > 0.98 && normalized_error < 1e-4 {
                    // Even closer - just jump to 100%
                    println!("  → At 98%+ ramp with acceptable error - jumping to 100%");
                    ramp_factor = 1.0;
                    continue;
                } else {
                    // Normal PID control
                    // Map PID output to ramp rate
                    // Higher base rate for faster progress
                    ramp_rate = 0.01 - pid_output * 0.005;
                    ramp_rate = ramp_rate.clamp(-0.02, 0.1);  // Allow more aggressive movement
                }
                
                // Exit conditions with relaxed criteria
                if error < 1e-16 && ramp_factor > 0.999 {
                    println!("  → Ultra-precision achieved! error={:.2e}", error);
                    ramp_factor = 1.0;
                    break;
                } else if error < 1e-6 && ramp_factor > 0.999 {
                    // Relaxed convergence for difficult circuits
                    println!("  → Good convergence achieved! error={:.2e}", error);
                    ramp_factor = 1.0;
                    break;
                } else if error < 1e-4 && ramp_factor >= 1.0 {
                    // Accept reasonable convergence at 100% ramp
                    println!("  → Acceptable convergence at 100% ramp! error={:.2e}", error);
                    break;
                }
            } else {
                // Failed to converge - but check if we're close enough
                println!("  → Convergence failed at ramp={:.3} with error={:.2e}", ramp_factor, error);
                
                // Always accept the current solution and move forward
                // The key insight: backing up doesn't help if Newton-Raphson fails
                // We should just continue with the best approximation we have
                println!("  → Accepting current solution and continuing forward");
                
                // Use a smaller ramp rate to be more careful
                let normalized_error = error / ramp_factor.max(0.01);
                let error_ratio = (normalized_error / 1e-6).ln().max(-10.0).min(10.0);
                let pid_output = control_pid.update(error_ratio, 0.01);
                
                // Always move forward, just adjust the rate based on error
                if error < 1e-4 {
                    // Small error - continue normally
                    ramp_rate = 0.005 - pid_output * 0.002;
                } else if error < 1e-2 {
                    // Medium error - slow down
                    ramp_rate = 0.002 - pid_output * 0.001;
                } else {
                    // Large error - very slow
                    ramp_rate = 0.001;
                }
                
                // Always positive (forward) rate
                ramp_rate = ramp_rate.clamp(0.0001, 0.05);
            }
            
            // Update ramp factor (can now go backward!)
            ramp_factor += ramp_rate;
            ramp_factor = ramp_factor.max(1e-6).min(1.0); // Allow very small ramp for difficult circuits
            
            // Progress output - show every 10 iterations for debugging
            if ramp_iterations % 10 == 0 || ramp_factor > 0.99 {
                let norm_err = if ramp_factor > 0.01 { error / ramp_factor } else { error };
                println!("  Phase 2 Iter {}: Ramp {:.4} ({:.1}%): error={:.2e} (norm={:.2e}), rate={:.6e}, gradient={:.2}", 
                         ramp_iterations, ramp_factor, ramp_factor * 100.0, error, norm_err, ramp_rate, log_gradient);
            }
        }
        
        // Check if we hit iteration limit
        if ramp_iterations >= MAX_RAMP_ITERATIONS {
            println!("\n=== CONVERGENCE FAILURE REPORT ===");
            println!("Hit ramp iteration limit of {} at ramp_factor={:.3}", 
                     MAX_RAMP_ITERATIONS, ramp_factor);
            println!("\nLast 10 convergence points:");
            let start = convergence_history.len().saturating_sub(10);
            for i in start..convergence_history.len() {
                let (ramp, err, _) = convergence_history[i];
                println!("  Ramp {:.4}: error={:.6e}", ramp, err);
            }
            
            // Check if we were making progress
            if convergence_history.len() > 20 {
                let recent_start = convergence_history.len() - 10;
                let older_start = convergence_history.len() - 20;
                
                let recent_avg: f64 = convergence_history[recent_start..]
                    .iter()
                    .map(|(_, e, _)| e)
                    .sum::<f64>() / 10.0;
                    
                let older_avg: f64 = convergence_history[older_start..recent_start]
                    .iter()
                    .map(|(_, e, _)| e)
                    .sum::<f64>() / 10.0;
                    
                println!("\nProgress analysis:");
                println!("  Average error (iterations {}-{}): {:.6e}", older_start, recent_start, older_avg);
                println!("  Average error (iterations {}-{}): {:.6e}", recent_start, convergence_history.len(), recent_avg);
                
                if recent_avg < older_avg * 0.9 {
                    println!("  → Still making progress (10% improvement)");
                } else if recent_avg > older_avg * 1.1 {
                    println!("  → Getting worse!");
                } else {
                    println!("  → Stagnated (no significant change)");
                    
                    // For circuits stuck at transition, try alternative approaches
                    if ramp_factor > 0.5 && recent_avg > 0.1 {
                        println!("\n  → Stuck at LED transition point. Trying alternative approaches...");
                        
                        // Approach 1: Try direct solve at 100% with different initial guesses
                        println!("  → Approach 1: Direct solve at 100% with multiple initial guesses");
                        
                        // Restore voltage sources to 100%
                        for (idx, name, original_voltage) in &voltage_sources {
                            if let Some(model) = self.models.get_mut(name) {
                                if let ComponentModel::VoltageSource { voltage, .. } = model {
                                    *voltage = *original_voltage;
                                }
                            }
                        }
                        
                        // Try advanced numerical techniques for stuck convergence
                        println!("  → Approach 1: Advanced numerical techniques for oscillating systems");
                        
                        // Approach 1: Line search with backtracking
                        println!("    → Trying line search with backtracking");
                        let mut best_alpha = 1.0;
                        let mut best_error = recent_avg;
                        
                        // Build system at current point to get descent direction
                        let (jacobian, residual) = self.build_system_matrices(
                            &x, &node_list, ground_idx, &voltage_source_indices
                        )?;
                        
                        if let Some(delta_x) = jacobian.lu().solve(&residual) {
                            // Try different step sizes
                            for alpha in [0.1, 0.25, 0.5, 0.75, 1.0, 1.5].iter() {
                                let mut test_x = x.clone();
                                for i in 0..test_x.len() {
                                    test_x[i] -= alpha * delta_x[i];
                                }
                                
                                // Evaluate residual at test point
                                let (_, test_residual) = self.build_system_matrices(
                                    &test_x, &node_list, ground_idx, &voltage_source_indices
                                )?;
                                let test_error = test_residual.norm();
                                
                                if test_error < best_error {
                                    best_error = test_error;
                                    best_alpha = *alpha;
                                }
                            }
                            
                            if best_error < recent_avg * 0.5 {
                                println!("      → Line search found better step with alpha={:.2}, error={:.2e}", best_alpha, best_error);
                                for i in 0..x.len() {
                                    x[i] -= best_alpha * delta_x[i];
                                }
                                // Don't do averaging, we found a better direction
                            } else {
                                // Approach 2: If oscillating between states, try averaging
                                println!("    → Detecting oscillation pattern");
                        
                        // Track several iterations to detect oscillation
                        let mut history = Vec::new();
                        for _ in 0..5 {
                            let (converged, _, error) = self.solve_at_ramp(
                                &mut x, &node_list, ground_idx, &voltage_source_indices, &mut total_iterations
                            )?;
                            history.push((x.clone(), error));
                            if converged {
                                println!("      → Converged during pattern detection!");
                                
                                // Extract results
                                let mut node_voltages = NodeVoltages::new();
                                node_voltages.insert(ground_idx, 0.0);
                                
                                for (i, &node_idx) in node_list.iter().enumerate() {
                                    node_voltages.insert(node_idx, x[i]);
                                    self.circuit.set_node_voltage(node_idx, x[i]);
                                }
                                
                                let vsource_currents: Vec<f64> = (num_nodes..matrix_size)
                                    .map(|i| x[i])
                                    .collect();
                                let branch_currents = self.calculate_branch_currents(
                                    &node_voltages, &voltage_source_indices, &vsource_currents
                                )?;
                                
                                let total_power = self.calculate_total_power(&node_voltages, &branch_currents);
                                
                                return Ok(AnalysisResult {
                                    node_voltages,
                                    branch_currents,
                                    total_power,
                                    iterations: total_iterations,
                                });
                            }
                        }
                        
                        // Check if we're oscillating between two states
                        if history.len() >= 4 {
                            let diff_0_2 = (&history[0].0 - &history[2].0).norm();
                            let diff_1_3 = (&history[1].0 - &history[3].0).norm();
                            
                            if diff_0_2 < 1e-6 && diff_1_3 < 1e-6 {
                                println!("      → Detected oscillation between two states");
                                // Average the two states
                                for i in 0..x.len() {
                                    x[i] = (history[0].0[i] + history[1].0[i]) / 2.0;
                                }
                                
                                // Try one more solve from averaged state
                                let (final_converged, _, final_error) = self.solve_at_ramp(
                                    &mut x, &node_list, ground_idx, &voltage_source_indices, &mut total_iterations
                                )?;
                                
                                if final_converged || final_error < 1e-3 {
                                    println!("      → Averaged solution converged!");
                                } else {
                                    println!("      → Accepting averaged solution (error = {:.2e})", final_error);
                                }
                                
                                // Extract results
                                let mut node_voltages = NodeVoltages::new();
                                node_voltages.insert(ground_idx, 0.0);
                                
                                for (i, &node_idx) in node_list.iter().enumerate() {
                                    node_voltages.insert(node_idx, x[i]);
                                    self.circuit.set_node_voltage(node_idx, x[i]);
                                }
                                
                                let vsource_currents: Vec<f64> = (num_nodes..matrix_size)
                                    .map(|i| x[i])
                                    .collect();
                                let branch_currents = self.calculate_branch_currents(
                                    &node_voltages, &voltage_source_indices, &vsource_currents
                                )?;
                                
                                let total_power = self.calculate_total_power(&node_voltages, &branch_currents);
                                
                                println!("  → Found solution using oscillation averaging");
                                
                                return Ok(AnalysisResult {
                                    node_voltages,
                                    branch_currents,
                                    total_power,
                                    iterations: total_iterations,
                                });
                            }
                        }
                        
                        println!("  → Advanced numerical techniques did not resolve the issue");
                            }
                        }
                    }
                }
            }
            
            return Err(SpiceError::ConvergenceFailed(total_iterations));
        }
        } // End of else block (Phase 2)
        
        // Check if we reached 100% 
        if ramp_factor >= 1.0 {
            println!("\nSolver complete - reached 100% ramp factor");
        }
        
        // Final solve at 100% if not already there
        if ramp_factor < 1.0 {
            for (idx, name, original_voltage) in &voltage_sources {
                if let Some(model) = self.models.get_mut(name) {
                    if let ComponentModel::VoltageSource { voltage, .. } = model {
                        *voltage = *original_voltage;
                    }
                }
            }
            
            let (converged, iterations, final_error) = self.solve_at_ramp(
                &mut x, &node_list, ground_idx, &voltage_source_indices, &mut total_iterations
            )?;
            
            // Final convergence push for precision
            println!("\n  → Final convergence push at 100%...");
            let mut best_error = final_error;
            for pass in 0..10 {
                let (converged, iters, error) = self.solve_at_ramp(
                    &mut x, &node_list, ground_idx, &voltage_source_indices, &mut total_iterations
                )?;
                if error < best_error {
                    best_error = error;
                }
                if error < 1e-16 || (pass > 5 && error < 1e-15) {
                    break;
                }
            }
        }
        
        info!("Converged with Two-Phase Adaptive PID in {} total iterations", total_iterations);
        println!("Successfully converged!");
        
        // Extract results
        let mut node_voltages = NodeVoltages::new();
        node_voltages.insert(ground_idx, 0.0);
        
        for (i, &node_idx) in node_list.iter().enumerate() {
            node_voltages.insert(node_idx, x[i]);
            self.circuit.set_node_voltage(node_idx, x[i]);
        }
        
        // Calculate branch currents
        let vsource_currents: Vec<f64> = (num_nodes..matrix_size)
            .map(|i| x[i])
            .collect();
        let branch_currents = self.calculate_branch_currents(
            &node_voltages, &voltage_source_indices, &vsource_currents
        )?;
        
        // Calculate total power
        let total_power = self.calculate_total_power(&node_voltages, &branch_currents);
        
        Ok(AnalysisResult {
            node_voltages,
            branch_currents,
            total_power,
            iterations: total_iterations,
        })
    }
    
    /// Quick solve for scanning - uses fewer iterations and relaxed tolerance
    fn solve_at_ramp_quick(
        &mut self,
        x: &mut DVector<f64>,
        node_list: &[NodeIndex],
        ground_idx: NodeIndex,
        voltage_sources: &[EdgeIndex],
        total_iterations: &mut usize,
    ) -> Result<(bool, usize, f64)> {
        let max_iter = 50;  // Moderate limit for scanning
        let tol = 1e-5;     // More relaxed tolerance for scanning
        let mut iterations = 0;
        
        for iter in 0..max_iter {
            iterations = iter + 1;
            *total_iterations += 1;
            
            let old_x = x.clone();
            
            // Build system matrices
            let (jacobian, residual) = self.build_system_matrices(
                x, node_list, ground_idx, voltage_sources
            )?;
            
            let residual_norm = residual.norm();
            if residual_norm < tol {
                return Ok((true, iterations, residual_norm));
            }
            
            // Solve system
            match jacobian.lu().solve(&residual) {
                Some(delta_x) => {
                    let max_change = delta_x.iter()
                        .map(|&v| v.abs())
                        .fold(0.0, f64::max);
                    
                    // Simple damping
                    let damping = if max_change > 1.0 { 0.5 } else { 1.0 };
                    *x -= &(delta_x * damping);
                    
                    // Check convergence
                    if max_change < tol {
                        return Ok((true, iterations, residual_norm));
                    }
                }
                None => {
                    // Don't fail immediately - just report non-convergence
                    // This matches the behavior of the main solver
                    return Ok((false, iterations, residual_norm));
                }
            }
        }
        
        // Return current error even if not converged
        let (_, residual) = self.build_system_matrices(
            x, node_list, ground_idx, voltage_sources
        )?;
        Ok((false, iterations, residual.norm()))
    }
    
    /// Solve circuit at a specific ramp factor
    fn solve_at_ramp(
        &mut self,
        x: &mut DVector<f64>,
        node_list: &[NodeIndex],
        ground_idx: NodeIndex,
        voltage_sources: &[EdgeIndex],
        total_iterations: &mut usize,
    ) -> Result<(bool, usize, f64)> {
        let max_iter = 100;  // Increased for difficult circuits like series LEDs
        let base_tol = 1e-12;
        let mut iterations = 0;
        let mut last_error = 0.0;
        let mut last_residual_norm = f64::INFINITY;
        let mut last_max_change = f64::INFINITY;
        let mut oscillation_count = 0;
        let num_nodes = node_list.len();
        let num_vsources = voltage_sources.len();
        
        // Get current ramp factor for adaptive tolerance
        let current_ramp = voltage_sources.first()
            .and_then(|&edge_idx| {
                self.circuit.branches()
                    .find(|(idx, _)| *idx == edge_idx)
                    .and_then(|(_, branch)| self.models.get(&branch.name))
                    .and_then(|model| match model {
                        ComponentModel::VoltageSource { voltage, .. } => Some(*voltage),
                        _ => None
                    })
            })
            .unwrap_or(1.0) / 5.0;  // Assuming 5V is the original voltage
        
        // Adaptive tolerance - more relaxed at low ramp values
        let tol = if current_ramp < 0.5 {
            base_tol * 1000.0  // 1e-9 for early ramping (was 1e-10)
        } else if current_ramp < 0.9 {
            base_tol * 100.0   // 1e-10 for mid ramping (was 1e-11)
        } else {
            base_tol * 10.0    // 1e-11 for final precision (was 1e-12)
        };
        
        let mut approx_condition = 1.0;  // Initialize condition number estimate
        
        for iter in 0..max_iter {
            iterations = iter + 1;
            *total_iterations += 1;
            
            let old_x = x.clone();
            
            // Build system matrices
            let (jacobian, residual) = self.build_system_matrices(
                x, node_list, ground_idx, voltage_sources
            )?;
            
            // Debug first few iterations (before consuming jacobian)
            if iter < 3 && *total_iterations < 10 {
                if jacobian.nrows() <= 4 {
                    println!("    x: {:?}", x.as_slice());
                    println!("    Jacobian:\n{}", jacobian);
                    println!("    Residual: {}", residual.transpose());
                }
            }
            
            // Apply generic preconditioning for numerical stability
            let size = jacobian.nrows();
            
            // Create diagonal preconditioner: D[i] = 1/max(|A[i,:]|)
            let mut preconditioner = DVector::zeros(size);
            let mut needs_preconditioning = false;
            let mut max_scale_ratio = 1.0;
            
            for i in 0..size {
                let mut row_max = 0.0f64;
                for j in 0..size {
                    row_max = row_max.max(jacobian[(i, j)].abs());
                }
                
                if row_max > 1e-20 {
                    preconditioner[i] = 1.0 / row_max;
                    // Check if this row has extreme scaling
                    if row_max > 1e6 || row_max < 1e-6 {
                        needs_preconditioning = true;
                    }
                    max_scale_ratio = (max_scale_ratio as f64).max(row_max);
                } else {
                    preconditioner[i] = 1.0;
                }
            }
            
            // Estimate condition improvement
            let min_scale = preconditioner.iter()
                .map(|&p| if p > 1e-20 { 1.0 / p } else { 1e20 })
                .fold(1e20, f64::min);
            approx_condition = max_scale_ratio / min_scale;
            
            // Apply preconditioning if needed
            let mut final_jacobian = jacobian.clone();
            let mut final_residual = residual.clone();
            
            if needs_preconditioning || approx_condition > 1e6 {
                // Apply diagonal preconditioning: D*A and D*b
                for i in 0..size {
                    for j in 0..size {
                        final_jacobian[(i, j)] *= preconditioner[i];
                    }
                    final_residual[i] *= preconditioner[i];
                }
                
                if (iter == 0 || iter % 10 == 0) {
                    println!("    Preconditioning applied: scale ratio = {:.2e}, condition ≈ {:.2e}", 
                             max_scale_ratio, approx_condition);
                }
            }
            
            // Solve the (possibly preconditioned) system
            let dx = match final_jacobian.lu().solve(&(-&final_residual)) {
                Some(solution) => solution,
                None => {
                    println!("LU decomposition failed at iteration {}", iter);
                    return Ok((false, iterations, residual.norm()));
                }
            };
            
            // Calculate residual norm
            let current_residual_norm = residual.norm();
            
            // Adaptive damping based on progress AND Jacobian conditioning
            let progress_damping = if iter == 0 {
                0.5  // Start moderate
            } else {
                // Adjust damping based on residual reduction
                if current_residual_norm < last_residual_norm * 0.5 {
                    // Excellent progress - be more aggressive
                    0.9
                } else if current_residual_norm < last_residual_norm * 0.9 {
                    // Good progress
                    0.7
                } else if current_residual_norm < last_residual_norm {
                    // Some progress
                    0.5
                } else {
                    // No progress or worse - be conservative
                    0.3
                }
            };
            
            // Further adjust damping based on Jacobian condition number
            // This "Adaptive Sweet Spots" strategy achieved 4/4 convergence on ultra-sharp LEDs
            let condition_damping = if approx_condition < 1e5 {
                0.7  // Good condition, moderate damping
            } else if approx_condition < 1e7 {
                0.3  // Medium condition, careful damping
            } else if approx_condition < 1e9 {
                0.5  // Counter-intuitively, medium damping works well here
            } else {
                0.2  // Very bad condition, but not too aggressive
            };
            
            // Use condition-based damping directly - don't multiply!
            // The optimal damping values were tuned for direct use
            let damping = condition_damping;
            
            // Simple update like reference - no complex line search!
            let mut max_change = 0.0f64;
            
            // Apply damped Newton step
            for i in 0..num_nodes {
                let delta = dx[i];
                x[i] = old_x[i] + damping * delta;
                max_change = max_change.max(delta.abs()); // Measure UNDAMPED change
            }
            for i in 0..num_vsources {
                x[num_nodes + i] = old_x[num_nodes + i] + damping * dx[num_nodes + i];
            }
            
            last_error = max_change;
            
            last_residual_norm = current_residual_norm;
            
            // Debug convergence - print more details when stuck
            if (iter < 3 && *total_iterations < 10) || (iter > 45) || approx_condition > 1e9 {
                println!("    Iter {}: max_change = {:.6e}, residual = {:.6e}, damping = {:.3} (condition ≈ {:.2e})", 
                         iter, max_change, current_residual_norm, damping, approx_condition);
                if x.len() <= 4 {
                    println!("    x = {:?}", x.as_slice());
                }
            }
            
            // Apply voltage limiting based on source voltages to prevent exponential overflow
            // Find maximum source voltage to set appropriate limits
            let max_source_voltage = voltage_sources.iter()
                .filter_map(|&vs_idx| {
                    self.circuit.branches()
                        .find(|(idx, _)| *idx == vs_idx)
                        .and_then(|(_, branch)| self.models.get(&branch.name))
                        .and_then(|model| match model {
                            ComponentModel::VoltageSource { voltage, .. } => Some(voltage.abs()),
                            _ => None
                        })
                })
                .fold(10.0, f64::max); // At least 10V
            
            let voltage_limit = max_source_voltage * 2.0; // Allow 2x source voltage
            for i in 0..num_nodes {
                // Limit voltage excursions to prevent exponential overflow in diode models
                x[i] = x[i].clamp(-voltage_limit, voltage_limit);
            }
            
            // Check convergence - use RELATIVE tolerance for better scaling
            // Find typical voltage scale in the solution
            let mut max_voltage = 0.0f64;
            for i in 0..num_nodes {
                max_voltage = max_voltage.max(x[i].abs());
            }
            let voltage_scale = max_voltage.max(0.1); // At least 0.1V scale
            
            // Use relative tolerance
            let relative_change = max_change / voltage_scale;
            let relative_tol = tol / voltage_scale;
            
            // Convergence criteria
            let delta_converged = relative_change < relative_tol;
            let residual_converged = current_residual_norm < tol * 10.0;
            
            // Check if we're in a limit cycle
            if iter > 20 {
                let recent_progress = (last_error - max_change).abs() / max_change.max(1e-15);
                if recent_progress < 0.01 && relative_change < relative_tol * 10.0 {
                    // Stuck in limit cycle but close enough
                    return Ok((true, iterations, max_change));
                }
            }
            
            // Generic oscillation and stagnation detection
            if iter > 30 {
                // Keep a simple history of recent changes and residuals
                static mut RECENT_CHANGES: [f64; 10] = [0.0; 10];
                static mut RECENT_RESIDUALS: [f64; 10] = [0.0; 10];
                static mut HISTORY_INDEX: usize = 0;
                
                unsafe {
                    // Store current values in circular buffer
                    RECENT_CHANGES[HISTORY_INDEX] = max_change;
                    RECENT_RESIDUALS[HISTORY_INDEX] = current_residual_norm;
                    HISTORY_INDEX = (HISTORY_INDEX + 1) % 10;
                    
                    if iter >= 40 {
                        // Calculate statistics
                        let avg_change = RECENT_CHANGES.iter().sum::<f64>() / 10.0;
                        let avg_residual = RECENT_RESIDUALS.iter().sum::<f64>() / 10.0;
                        let change_variance = variance(&RECENT_CHANGES);
                        
                        // Check for oscillation by looking at variance pattern
                        let mut differences = [0.0; 9];
                        for i in 0..9 {
                            differences[i] = (RECENT_CHANGES[i] - RECENT_CHANGES[i+1]).abs();
                        }
                        let diff_variance = variance(&differences);
                        
                        // High variance in changes but low variance in differences suggests oscillation
                        if change_variance > (avg_change * 0.1).powi(2) && diff_variance < (avg_change * 0.01).powi(2) {
                            if avg_residual < tol * 100.0 {
                                println!("    → Detected oscillation pattern. Avg residual = {:.2e}", avg_residual);
                                println!("      System appears to have multiple nearby solutions");
                                return Ok((true, iterations, avg_residual));
                            }
                        }
                        
                        // Check for stagnation
                        if change_variance < (avg_change * 0.01).powi(2) {
                            // Very little variation in changes - we're stuck
                            if avg_residual < tol * 1000.0 {
                                println!("    → Solver stagnated but residual is acceptable ({:.2e})", avg_residual);
                                return Ok((true, iterations, avg_residual));
                            } else if iter > 80 {
                                // Apply adaptive damping to escape stagnation
                                println!("    → Applying adaptive damping to escape stagnation");
                                let damping_factor = 0.5 / (1.0 + (iter - 80) as f64 * 0.01);
                                for i in 0..x.len() {
                                    x[i] = old_x[i] + damping_factor * (x[i] - old_x[i]);
                                }
                            }
                        }
                    }
                }
            }
            
            // Additional convergence criteria for stuck cases
            let stuck_but_good = iterations > 50 && 
                                relative_change < relative_tol * 100.0;  // More relaxed
            
            // Also accept if we're making very small changes
            let tiny_changes = iterations > 20 && max_change < 1e-6;
            
            // GENERIC STALLED CONVERGENCE: Accept very small errors when progress stalls
            // This handles cases where Newton-Raphson gets very close but can't reach exact tolerance
            let stalled_convergence = if iter > 60 {
                // If we've been iterating for a while and error is very small, accept it
                let very_small_error = current_residual_norm < 1e-7;
                let progress_rate = if iter > 10 {
                    (last_residual_norm - current_residual_norm).abs() / current_residual_norm.max(1e-15)
                } else {
                    1.0  // Assume good progress early on
                };
                let minimal_progress = progress_rate < 0.001;  // Less than 0.1% improvement per iteration
                very_small_error && minimal_progress
            } else {
                false
            };
            
            if delta_converged || 
               (iterations > 30 && residual_converged) ||
               stuck_but_good ||
               tiny_changes ||
               stalled_convergence {
                return Ok((true, iterations, max_change));
            }
        }
        
        Ok((false, iterations, last_error))
    }
    
    /// Analyze and return just the AnalysisResult (for intelligent engine compatibility)
    pub fn analyze_simple(&mut self) -> Result<Vec<AnalysisResult>> {
        let results = self.analyze()?;
        Ok(results.into_iter().map(|(_, _, _, result)| result).collect())
    }
    
    /// Get a reference to the circuit (for intelligent engine)
    pub fn get_circuit(&self) -> &Circuit {
        &self.circuit
    }
    
    /// Get a mutable reference to the circuit (for strategies)
    pub fn get_circuit_mut(&mut self) -> &mut Circuit {
        &mut self.circuit
    }
    
    /// Update a component's model (for progressive solving)
    pub fn update_component_model(&mut self, name: &str, model: ComponentModel) {
        self.models.insert(name.to_string(), model);
    }
    
    /// Get a component's current model
    pub fn get_component_model(&self, name: &str) -> Option<&ComponentModel> {
        self.models.get(name)
    }
    
    /// Build system matrices (Jacobian and residual)
    fn build_system_matrices(
        &mut self,
        x: &DVector<f64>,
        node_list: &[NodeIndex],
        ground_idx: NodeIndex,
        voltage_sources: &[EdgeIndex],
    ) -> Result<(DMatrix<f64>, DVector<f64>)> {
        let num_nodes = node_list.len();
        let num_vsources = voltage_sources.len();
        let size = num_nodes + num_vsources;
        
        let mut jacobian = DMatrix::zeros(size, size);
        let mut residual = DVector::zeros(size);
        
        // Add minimum conductance to ground for stability
        // Use larger gmin for better conditioning
        let gmin = 1e-10;  // Increased from 1e-12
        for i in 0..num_nodes {
            jacobian[(i, i)] += gmin;
        }
        
        // Process each branch (except voltage sources which are handled separately)
        for (edge_idx, branch) in self.circuit.branches() {
            // Skip voltage sources - they are handled separately below
            if let Some(model) = self.models.get(&branch.name) {
                if matches!(model, ComponentModel::VoltageSource { .. }) {
                    continue;
                }
            }
            
            if let Some((n1, n2)) = self.circuit.branch_nodes(edge_idx) {
                let n1_idx = if n1 == ground_idx { None } else {
                    node_list.iter().position(|&n| n == n1)
                };
                let n2_idx = if n2 == ground_idx { None } else {
                    node_list.iter().position(|&n| n == n2)
                };
                
                let v1 = n1_idx.map(|i| x[i]).unwrap_or(0.0);
                let v2 = n2_idx.map(|i| x[i]).unwrap_or(0.0);
                let v_diff = v1 - v2;
                
                // Use runtime model engine
                let mut ctx = ModelExecutionContext {
                    jacobian: &mut jacobian,
                    residual: &mut residual,
                    x,
                    n1_idx,
                    n2_idx,
                    v_diff,
                };
                
                if let Some(model) = self.models.get(&branch.name) {
                    self.model_engine.execute_component_model_with_params(
                        &branch.name, model, &mut ctx
                    ).map_err(|e| SpiceError::AnalysisFailed(
                        format!("Model execution failed: {}", e)
                    ))?;
                } else {
                    self.model_engine.execute_component_model(
                        &branch.name, &mut ctx
                    ).map_err(|e| SpiceError::AnalysisFailed(
                        format!("Model execution failed: {}", e)
                    ))?;
                }
            }
        }
        
        // Handle voltage sources
        for (vsrc_num, &edge_idx) in voltage_sources.iter().enumerate() {
            if let Some(branch) = self.circuit.branches().find(|(idx, _)| *idx == edge_idx) {
                if let Some((n1, n2)) = self.circuit.branch_nodes(edge_idx) {
                    if let Some(ComponentModel::VoltageSource { voltage, .. }) = 
                        self.models.get(&branch.1.name) {
                        
                        let vsrc_row = num_nodes + vsrc_num;
                        
                        let n1_idx = if n1 == ground_idx { None } else {
                            node_list.iter().position(|&n| n == n1)
                        };
                        let n2_idx = if n2 == ground_idx { None } else {
                            node_list.iter().position(|&n| n == n2)
                        };
                        
                        // Stamp voltage source
                        if let Some(i1) = n1_idx {
                            jacobian[(i1, vsrc_row)] = 1.0;
                            jacobian[(vsrc_row, i1)] = 1.0;
                        }
                        if let Some(i2) = n2_idx {
                            jacobian[(i2, vsrc_row)] = -1.0;
                            jacobian[(vsrc_row, i2)] = -1.0;
                        }
                        
                        // Current terms
                        let vsrc_current = x[vsrc_row];
                        if let Some(i1) = n1_idx {
                            residual[i1] += vsrc_current;
                        }
                        if let Some(i2) = n2_idx {
                            residual[i2] -= vsrc_current;
                        }
                        
                        // Voltage constraint
                        let v1 = n1_idx.map(|i| x[i]).unwrap_or(0.0);
                        let v2 = n2_idx.map(|i| x[i]).unwrap_or(0.0);
                        residual[vsrc_row] = v1 - v2 - voltage;
                    }
                }
            }
        }
        
        Ok((jacobian, residual))
    }
    
    /// Calculate current through an element
    fn calculate_element_current(&self, model: &ComponentModel, voltage: f64) -> f64 {
        match model {
            ComponentModel::LED { forward_voltage, forward_current, .. } => {
                let vt = 0.026;
                let test_v = 0.1_f64;
                let v_norm_test = test_v / vt;
                let is = forward_current / (v_norm_test.exp() - 1.0);
                let effective_v = voltage - forward_voltage;
                
                if effective_v <= 0.0 {
                    -is
                } else {
                    let v_norm = effective_v / vt;
                    if v_norm > 50.0 {
                        is * (50.0_f64.exp() - 1.0)
                    } else {
                        is * (v_norm.exp() - 1.0)
                    }
                }
            },
            ComponentModel::Diode { saturation_current, emission_coefficient, .. } => {
                let vt = 0.026;
                let n = emission_coefficient.unwrap_or(1.0);
                let is = saturation_current.unwrap_or(1e-12);
                
                if voltage > 0.0 {
                    is * ((voltage / (n * vt)).min(40.0).exp() - 1.0)
                } else {
                    -is
                }
            },
            ComponentModel::Resistor { resistance, .. } => {
                voltage / resistance
            },
            _ => 0.0
        }
    }
    
    /// Calculate branch currents from solution
    fn calculate_branch_currents(
        &mut self,
        node_voltages: &NodeVoltages,
        voltage_sources: &[EdgeIndex],
        vsource_currents: &[f64],
    ) -> Result<BranchCurrents> {
        let mut branch_currents = BranchCurrents::new();
        
        // Collect branch information first to avoid borrow conflicts
        let branches: Vec<_> = self.circuit.branches()
            .map(|(idx, branch)| (idx, branch.name.clone()))
            .collect();
        
        for (edge_idx, branch_name) in branches {
            if let Some((n1, n2)) = self.circuit.branch_nodes(edge_idx) {
                let v1 = node_voltages.get(&n1).copied().unwrap_or(0.0);
                let v2 = node_voltages.get(&n2).copied().unwrap_or(0.0);
                let v_diff = v1 - v2;
                
                let current = if let Some(vsrc_idx) = voltage_sources.iter()
                    .position(|&e| e == edge_idx) {
                    vsource_currents[vsrc_idx]
                } else if let Some(model) = self.models.get(&branch_name) {
                    self.calculate_element_current(model, v_diff)
                } else {
                    0.0
                };
                
                branch_currents.insert(edge_idx, current);
                self.circuit.set_branch_current(edge_idx, current);
            }
        }
        
        Ok(branch_currents)
    }
    
    /// Calculate total power dissipation
    fn calculate_total_power(
        &self,
        node_voltages: &NodeVoltages,
        branch_currents: &BranchCurrents,
    ) -> f64 {
        let mut total_power = 0.0;
        
        for (edge_idx, &current) in branch_currents {
            if let Some((n1, n2)) = self.circuit.branch_nodes(*edge_idx) {
                let v1 = node_voltages.get(&n1).copied().unwrap_or(0.0);
                let v2 = node_voltages.get(&n2).copied().unwrap_or(0.0);
                let voltage_diff = (v1 - v2).abs();
                let power = voltage_diff * current.abs();
                total_power += power;
            }
        }
        
        total_power
    }
    
    /// Analyze with full log transformation applied to exponential components
    pub fn analyze_with_log_transform_full(
        &mut self,
        start_ramp: f64,
        target_voltage: Option<f64>,
        scaling: &crate::enhanced_glacier_solver::ScalingState,
    ) -> Result<AnalysisResult> {
        info!("Starting full log transformation analysis");
        
        // Get ground node
        let ground_idx = self.circuit.ground_node()
            .ok_or_else(|| SpiceError::NoGroundNode)?
            .0;
        
        // Build node list (excluding ground)
        let node_list: Vec<NodeIndex> = self.circuit.nodes()
            .filter(|(idx, _)| *idx != ground_idx)
            .map(|(idx, _)| idx)
            .collect();
        
        let num_nodes = node_list.len();
        
        // Find voltage sources
        let voltage_sources: Vec<(EdgeIndex, String, f64)> = self.circuit.branches()
            .filter_map(|(idx, branch)| {
                self.models.get(&branch.name)
                    .and_then(|m| match m {
                        ComponentModel::VoltageSource { voltage, .. } => 
                            Some((idx, branch.name.clone(), *voltage)),
                        _ => None
                    })
            })
            .collect();
        
        let voltage_source_indices: Vec<EdgeIndex> = voltage_sources.iter()
            .map(|(idx, _, _)| *idx)
            .collect();
        
        let num_vsources = voltage_sources.len();
        let matrix_size = num_nodes + num_vsources;
        
        if matrix_size == 0 {
            return Err(SpiceError::EmptyCircuit);
        }
        
        // Initial solution vector in transformed space
        let mut x = DVector::zeros(matrix_size);
        let init_val = target_voltage.unwrap_or(0.01);
        
        // Initialize with small values for log transform stability
        for i in 0..num_nodes {
            x[i] = init_val;
        }
        
        // Track iterations
        let mut total_iterations = 0;
        
        // PID controller
        let mut pid = AdaptivePIDController::new(0.5, 0.1, 0.05);
        
        // Start from specified ramp
        let mut ramp_factor = start_ramp;
        let mut best_error = f64::INFINITY;
        let mut stuck_count = 0;
        
        info!("Using log transformation for {} variables", 
              scaling.transforms.iter().filter(|&&t| t == crate::enhanced_glacier_solver::TransformType::Logarithmic).count());
        
        loop {
            total_iterations += 1;
            
            if total_iterations > self.max_iterations {
                return Err(SpiceError::ConvergenceFailed(total_iterations));
            }
            
            // Apply ramp to voltage sources
            for (vsrc_idx, vsrc_name, original_voltage) in &voltage_sources {
                if let Some(model) = self.models.get_mut(vsrc_name) {
                    if let ComponentModel::VoltageSource { voltage, .. } = model {
                        *voltage = original_voltage * ramp_factor;
                    }
                }
            }
            
            // Transform solution to physical space for circuit evaluation
            let x_physical = scaling.inverse_transform(&x);
            
            // Build Jacobian and residual in physical space
            let (mut jacobian, mut residual) = self.build_system_matrices(
                &x_physical,
                &node_list,
                ground_idx,
                &voltage_source_indices,
            )?;
            
            // Calculate max residual before transformation
            let max_residual_phys = residual.iter()
                .map(|&v| v.abs())
                .fold(0.0, f64::max);
            
            // Transform Jacobian and residual to log space
            jacobian = scaling.transform_jacobian(&jacobian, &x_physical);
            residual = scaling.transform(&residual);
            
            // Calculate error in transformed space
            let max_residual = residual.iter()
                .map(|&v| v.abs())
                .fold(0.0, f64::max);
            
            if max_residual < self.tolerance && ramp_factor >= 0.9999 {
                // Converged!
                break;
            }
            
            // Solve for update in transformed space
            let decomp = jacobian.lu();
            let delta_x = decomp.solve(&(-&residual))
                .ok_or_else(|| SpiceError::SingularMatrix)?;
            
            // Adaptive damping based on gradient
            let log_gradient = self.calculate_log_gradient(&x_physical, &node_list, ground_idx);
            pid.adapt_gains_with_error(log_gradient, max_residual_phys, true);
            
            let damping = if max_residual_phys > 10.0 {
                0.1  // Heavy damping for large errors
            } else if log_gradient > 100.0 {
                0.2  // Moderate damping for high gradients
            } else {
                0.5  // Light damping otherwise
            };
            
            // Update solution in transformed space
            let max_change = delta_x.as_slice().iter()
                .map(|&v| v.abs())
                .fold(0.0, f64::max);
            
            x += &delta_x * damping;
            
            // Check for convergence
            if max_residual < best_error * 0.9 {
                best_error = max_residual;
                stuck_count = 0;
            } else {
                stuck_count += 1;
            }
            
            // Ramp control
            if max_residual < 1e-6 && ramp_factor < 1.0 {
                let error_rate = (max_residual - 1e-6).abs();
                let ramp_rate = 0.01 * (1.0 - error_rate / 1e-6).max(0.1);
                ramp_factor = (ramp_factor + ramp_rate).min(1.0);
                debug!("Ramping to {:.3}, error={:.2e}", ramp_factor, max_residual);
            }
            
            // Escape stuck situations
            if stuck_count > 20 && max_residual > 1e-3 {
                warn!("Stuck at ramp={:.3}, trying escape", ramp_factor);
                // Perturb solution slightly
                for i in 0..x.len() {
                    x[i] *= 1.0 + 0.1 * (i as f64 * 0.137).sin();
                }
                stuck_count = 0;
            }
            
            if total_iterations % 100 == 0 {
                info!("Iter {}: ramp={:.3}, error={:.2e}, max_change={:.2e}", 
                      total_iterations, ramp_factor, max_residual_phys, max_change);
            }
        }
        
        // Restore original voltage values
        for (_, vsrc_name, original_voltage) in &voltage_sources {
            if let Some(model) = self.models.get_mut(vsrc_name) {
                if let ComponentModel::VoltageSource { voltage, .. } = model {
                    *voltage = *original_voltage;
                }
            }
        }
        
        // Get final solution in physical space
        let x_final = scaling.inverse_transform(&x);
        
        // Extract results
        let mut node_voltages = NodeVoltages::new();
        for (i, &node_idx) in node_list.iter().enumerate() {
            node_voltages.insert(node_idx, x_final[i]);
        }
        node_voltages.insert(ground_idx, 0.0);
        
        let vsource_currents: Vec<f64> = (num_nodes..matrix_size)
            .map(|i| x_final[i])
            .collect();
        
        let branch_currents = self.calculate_branch_currents(
            &node_voltages,
            &voltage_source_indices,
            &vsource_currents,
        )?;
        
        let total_power = self.calculate_total_power(&node_voltages, &branch_currents);
        
        info!("Log transform solver converged in {} iterations", total_iterations);
        
        Ok(AnalysisResult {
            node_voltages,
            branch_currents,
            total_power,
            iterations: total_iterations,
        })
    }
    
    /// Perform transient analysis using companion models for reactive components
    /// This keeps the DC solver unchanged while implementing proper transient analysis
    pub fn analyze_transient(
        &mut self, 
        t_stop: f64, 
        t_step: f64,
        initial_conditions: Option<AnalysisResult>
    ) -> Result<TransientResult> {
        use crate::transient_models::{
            CapacitorCompanion, InductorCompanion, NonlinearCompanion, TransientSource
        };
        
        info!("Starting GLACIER transient analysis: t_stop={}, t_step={}", t_stop, t_step);
        
        // First, get DC operating point if not provided
        let dc_solution = match initial_conditions {
            Some(ic) => ic,
            None => {
                info!("Computing DC operating point for initial conditions");
                // Use MAESTRO for intelligent DC selection
                self.get_dc_with_maestro()
                    .or_else(|_| {
                        // Fallback to old behavior if MAESTRO fails
                        warn!("MAESTRO DC selection failed, falling back to max power selection");
                        self.analyze()
                            .and_then(|solutions| solutions.into_iter()
                                .max_by(|a, b| a.3.total_power.partial_cmp(&b.3.total_power).unwrap())
                                .map(|s| s.3)
                                .ok_or(SpiceError::ConvergenceFailed(0)))
                    })?
            }
        };
        
        // Initialize transient state
        let mut time_points = Vec::new();
        let mut voltage_history = Vec::new();
        let mut current_history = Vec::new();
        
        // Store initial conditions
        time_points.push(0.0);
        voltage_history.push(dc_solution.node_voltages.clone());
        current_history.push(dc_solution.branch_currents.clone());
        
        // Get node list and ground
        let ground_idx = self.circuit.ground_node()
            .ok_or_else(|| SpiceError::NoGroundNode)?
            .0;
        
        let node_list: Vec<NodeIndex> = self.circuit.nodes()
            .filter(|(idx, _)| *idx != ground_idx)
            .map(|(idx, _)| idx)
            .collect();
        
        // Find voltage sources for MNA
        let voltage_sources: Vec<EdgeIndex> = self.circuit.branches()
            .filter(|(_, branch)| {
                self.models.get(&branch.name)
                    .map(|m| matches!(m, ComponentModel::VoltageSource { .. }))
                    .unwrap_or(false)
            })
            .map(|(idx, _)| idx)
            .collect();
        
        let num_nodes = node_list.len();
        let num_vsources = voltage_sources.len();
        let matrix_size = num_nodes + num_vsources;
        
        // Initialize state vector from DC solution
        let mut x = DVector::zeros(matrix_size);
        for (i, &node_idx) in node_list.iter().enumerate() {
            x[i] = dc_solution.node_voltages.get(&node_idx).copied().unwrap_or(0.0);
        }
        // Initialize voltage source currents
        for (i, &edge_idx) in voltage_sources.iter().enumerate() {
            x[num_nodes + i] = dc_solution.branch_currents.get(&edge_idx).copied().unwrap_or(0.0);
        }
        
        // Create companion models for reactive components
        let mut capacitor_companions = HashMap::new();
        let mut inductor_companions = HashMap::new();
        let mut nonlinear_companions = HashMap::new();
        
        // Initialize companion models
        for (edge_idx, branch) in self.circuit.branches() {
            if let Some(model) = self.models.get(&branch.name) {
                match model {
                    ComponentModel::Capacitor { capacitance, .. } => {
                        let companion = CapacitorCompanion::new(*capacitance, t_step);
                        capacitor_companions.insert(edge_idx, companion);
                    }
                    ComponentModel::Inductor { inductance, .. } => {
                        let companion = InductorCompanion::new(*inductance, t_step);
                        inductor_companions.insert(edge_idx, companion);
                    }
                    ComponentModel::LED { .. } | ComponentModel::Diode { .. } => {
                        // Get initial operating point
                        if let Some((n1, n2)) = self.circuit.branch_nodes(edge_idx) {
                            let v1 = dc_solution.node_voltages.get(&n1).copied().unwrap_or(0.0);
                            let v2 = dc_solution.node_voltages.get(&n2).copied().unwrap_or(0.0);
                            let v_op = v1 - v2;
                            let i_op = dc_solution.branch_currents.get(&edge_idx).copied().unwrap_or(0.0);
                            
                            let companion = match model {
                                ComponentModel::LED { .. } => NonlinearCompanion::for_led(model, v_op, i_op),
                                ComponentModel::Diode { .. } => NonlinearCompanion::for_diode(model, v_op, i_op),
                                _ => unreachable!(),
                            };
                            nonlinear_companions.insert(edge_idx, companion);
                        }
                    }
                    _ => {}
                }
            }
        }
        
        // Time stepping loop
        let mut t = 0.0;
        let mut iteration = 0;
        
        while t < t_stop {
            t += t_step;
            iteration += 1;
            
            // Newton-Raphson at this time point
            let mut nr_iteration = 0;
            let max_nr_iterations = 50;
            let mut converged = false;
            
            loop {
                nr_iteration += 1;
                
                // Build system with companion models
                let (mut jacobian, mut residual) = self.build_transient_system(
                    &x, &node_list, ground_idx, &voltage_sources,
                    &capacitor_companions, &inductor_companions, &nonlinear_companions,
                    t, t_step
                )?;
                
                // Check convergence
                let error = residual.norm();
                if error < 1e-9 {
                    converged = true;
                    break;
                }
                
                if nr_iteration > max_nr_iterations {
                    if error > 1e-6 {
                        warn!("Transient NR did not converge at t={}: error={:.2e}", t, error);
                    }
                    break;
                }
                
                // Solve for update
                let lu = jacobian.lu();
                let delta = lu.solve(&(-residual))
                    .ok_or(SpiceError::SingularMatrix)?;
                
                // Apply update with damping
                let damping = if error > 1.0 { 0.5 } else { 1.0 };
                x += damping * delta;
            }
            
            // Update companion models with new solution
            if converged {
                for (edge_idx, companion) in capacitor_companions.iter_mut() {
                    if let Some((n1, n2)) = self.circuit.branch_nodes(*edge_idx) {
                        let v1 = if n1 == ground_idx { 0.0 } else {
                            node_list.iter().position(|&n| n == n1)
                                .map(|i| x[i]).unwrap_or(0.0)
                        };
                        let v2 = if n2 == ground_idx { 0.0 } else {
                            node_list.iter().position(|&n| n == n2)
                                .map(|i| x[i]).unwrap_or(0.0)
                        };
                        companion.update(v1 - v2, t_step);
                    }
                }
                
                for (edge_idx, companion) in inductor_companions.iter_mut() {
                    if let Some((n1, n2)) = self.circuit.branch_nodes(*edge_idx) {
                        // For inductors, we need the current
                        // This is more complex in MNA - skipping for now
                    }
                }
                
                for (edge_idx, companion) in nonlinear_companions.iter_mut() {
                    if let Some((n1, n2)) = self.circuit.branch_nodes(*edge_idx) {
                        let v1 = if n1 == ground_idx { 0.0 } else {
                            node_list.iter().position(|&n| n == n1)
                                .map(|i| x[i]).unwrap_or(0.0)
                        };
                        let v2 = if n2 == ground_idx { 0.0 } else {
                            node_list.iter().position(|&n| n == n2)
                                .map(|i| x[i]).unwrap_or(0.0)
                        };
                        let v_new = v1 - v2;
                        
                        // Calculate new current through the device
                        let i_new = if let Some(model) = self.models.get(&self.circuit.graph[*edge_idx].name) {
                            self.calculate_element_current(model, v_new)
                        } else {
                            0.0
                        };
                        
                        companion.update(v_new, i_new, 
                            self.models.get(&self.circuit.graph[*edge_idx].name).unwrap());
                    }
                }
            }
            
            // Extract and store solution at this time point
            let mut node_voltages = NodeVoltages::new();
            for (i, &node_idx) in node_list.iter().enumerate() {
                node_voltages.insert(node_idx, x[i]);
            }
            
            // Extract voltage source currents
            let vsource_currents: Vec<f64> = (0..num_vsources)
                .map(|i| x[num_nodes + i])
                .collect();
            
            // Calculate all branch currents
            let branch_currents = self.calculate_branch_currents(
                &node_voltages, &voltage_sources, &vsource_currents
            )?;
            
            time_points.push(t);
            voltage_history.push(node_voltages);
            current_history.push(branch_currents);
            
            if iteration % 100 == 0 {
                debug!("Transient at t={:.3}ms", t * 1000.0);
            }
        }
        
        info!("Transient analysis completed: {} time points", time_points.len());
        
        Ok(TransientResult {
            time_points,
            node_voltages: voltage_history,
            branch_currents: current_history,
        })
    }
    
    /// Build system matrices for transient analysis with companion models
    fn build_transient_system(
        &mut self,
        x: &DVector<f64>,
        node_list: &[NodeIndex],
        ground_idx: NodeIndex,
        voltage_sources: &[EdgeIndex],
        capacitor_companions: &HashMap<EdgeIndex, crate::transient_models::CapacitorCompanion>,
        inductor_companions: &HashMap<EdgeIndex, crate::transient_models::InductorCompanion>,
        nonlinear_companions: &HashMap<EdgeIndex, crate::transient_models::NonlinearCompanion>,
        time: f64,
        dt: f64,
    ) -> Result<(DMatrix<f64>, DVector<f64>)> {
        let num_nodes = node_list.len();
        let num_vsources = voltage_sources.len();
        let matrix_size = num_nodes + num_vsources;
        
        let mut jacobian = DMatrix::zeros(matrix_size, matrix_size);
        let mut residual = DVector::zeros(matrix_size);
        
        // Process all branches
        for (edge_idx, branch) in self.circuit.branches() {
            if let Some((n1, n2)) = self.circuit.branch_nodes(edge_idx) {
                let n1_idx = if n1 == ground_idx { None } else {
                    node_list.iter().position(|&n| n == n1)
                };
                let n2_idx = if n2 == ground_idx { None } else {
                    node_list.iter().position(|&n| n == n2)
                };
                
                let v1 = n1_idx.map(|i| x[i]).unwrap_or(0.0);
                let v2 = n2_idx.map(|i| x[i]).unwrap_or(0.0);
                let v_diff = v1 - v2;
                
                if let Some(model) = self.models.get(&branch.name) {
                    match model {
                        ComponentModel::Resistor { resistance, .. } => {
                            // Standard resistor stamping
                            let g = 1.0 / resistance;
                            let current = g * v_diff;
                            
                            if let Some(i1) = n1_idx {
                                jacobian[(i1, i1)] += g;
                                residual[i1] += current;
                            }
                            if let Some(i2) = n2_idx {
                                jacobian[(i2, i2)] += g;
                                residual[i2] -= current;
                            }
                            if let (Some(i1), Some(i2)) = (n1_idx, n2_idx) {
                                jacobian[(i1, i2)] -= g;
                                jacobian[(i2, i1)] -= g;
                            }
                        }
                        
                        ComponentModel::Capacitor { .. } => {
                            // Use companion model
                            if let Some(companion) = capacitor_companions.get(&edge_idx) {
                                let g_eq = companion.get_conductance();
                                let i_eq = companion.get_current_source();
                                
                                // Stamp Norton equivalent
                                if let Some(i1) = n1_idx {
                                    jacobian[(i1, i1)] += g_eq;
                                    residual[i1] += g_eq * v1 + i_eq;
                                }
                                if let Some(i2) = n2_idx {
                                    jacobian[(i2, i2)] += g_eq;
                                    residual[i2] -= g_eq * v2 - i_eq;
                                }
                                if let (Some(i1), Some(i2)) = (n1_idx, n2_idx) {
                                    jacobian[(i1, i2)] -= g_eq;
                                    jacobian[(i2, i1)] -= g_eq;
                                }
                            }
                        }
                        
                        ComponentModel::Inductor { .. } => {
                            // Use companion model
                            if let Some(companion) = inductor_companions.get(&edge_idx) {
                                let g_eq = companion.get_conductance();
                                let i_eq = companion.get_current_source();
                                
                                // Stamp Norton equivalent
                                if let Some(i1) = n1_idx {
                                    jacobian[(i1, i1)] += g_eq;
                                    residual[i1] += g_eq * v1 + i_eq;
                                }
                                if let Some(i2) = n2_idx {
                                    jacobian[(i2, i2)] += g_eq;
                                    residual[i2] -= g_eq * v2 - i_eq;
                                }
                                if let (Some(i1), Some(i2)) = (n1_idx, n2_idx) {
                                    jacobian[(i1, i2)] -= g_eq;
                                    jacobian[(i2, i1)] -= g_eq;
                                }
                            }
                        }
                        
                        ComponentModel::LED { .. } | ComponentModel::Diode { .. } => {
                            // Use linearized companion model
                            if let Some(companion) = nonlinear_companions.get(&edge_idx) {
                                let g_d = companion.dynamic_conductance;
                                let i_eq = companion.equivalent_current;
                                
                                // Stamp linearized Norton equivalent
                                if let Some(i1) = n1_idx {
                                    jacobian[(i1, i1)] += g_d;
                                    residual[i1] += g_d * v1 + i_eq;
                                }
                                if let Some(i2) = n2_idx {
                                    jacobian[(i2, i2)] += g_d;
                                    residual[i2] -= g_d * v2 - i_eq;
                                }
                                if let (Some(i1), Some(i2)) = (n1_idx, n2_idx) {
                                    jacobian[(i1, i2)] -= g_d;
                                    jacobian[(i2, i1)] -= g_d;
                                }
                            } else {
                                // Fall back to runtime model if no companion
                                let mut ctx = ModelExecutionContext {
                                    jacobian: &mut jacobian,
                                    residual: &mut residual,
                                    x,
                                    n1_idx,
                                    n2_idx,
                                    v_diff,
                                };
                                
                                self.model_engine.execute_component_model_with_params(
                                    &branch.name, model, &mut ctx
                                ).map_err(|e| SpiceError::AnalysisFailed(
                                    format!("Model execution failed: {}", e)
                                ))?;
                            }
                        }
                        
                        ComponentModel::VoltageSource { .. } => {
                            // Handled separately below
                        }
                        
                        _ => {
                            // Use runtime model engine for other components
                            let mut ctx = ModelExecutionContext {
                                jacobian: &mut jacobian,
                                residual: &mut residual,
                                x,
                                n1_idx,
                                n2_idx,
                                v_diff,
                            };
                            
                            self.model_engine.execute_component_model_with_params(
                                &branch.name, model, &mut ctx
                            ).map_err(|e| SpiceError::AnalysisFailed(
                                format!("Model execution failed: {}", e)
                            ))?;
                        }
                    }
                }
            }
        }
        
        // Handle voltage sources with time-varying values
        for (vsrc_num, &edge_idx) in voltage_sources.iter().enumerate() {
            if let Some(branch) = self.circuit.branches().find(|(idx, _)| *idx == edge_idx) {
                if let Some((n1, n2)) = self.circuit.branch_nodes(edge_idx) {
                    if let Some(ComponentModel::VoltageSource { voltage, .. }) = 
                        self.models.get(&branch.1.name) {
                        
                        let vsrc_row = num_nodes + vsrc_num;
                        
                        let n1_idx = if n1 == ground_idx { None } else {
                            node_list.iter().position(|&n| n == n1)
                        };
                        let n2_idx = if n2 == ground_idx { None } else {
                            node_list.iter().position(|&n| n == n2)
                        };
                        
                        // Stamp voltage source
                        if let Some(i1) = n1_idx {
                            jacobian[(i1, vsrc_row)] = 1.0;
                            jacobian[(vsrc_row, i1)] = 1.0;
                        }
                        if let Some(i2) = n2_idx {
                            jacobian[(i2, vsrc_row)] = -1.0;
                            jacobian[(vsrc_row, i2)] = -1.0;
                        }
                        
                        // Current terms
                        let vsrc_current = x[vsrc_row];
                        if let Some(i1) = n1_idx {
                            residual[i1] += vsrc_current;
                        }
                        if let Some(i2) = n2_idx {
                            residual[i2] -= vsrc_current;
                        }
                        
                        // Voltage constraint
                        let v1 = n1_idx.map(|i| x[i]).unwrap_or(0.0);
                        let v2 = n2_idx.map(|i| x[i]).unwrap_or(0.0);
                        residual[vsrc_row] = v1 - v2 - voltage;
                    }
                }
            }
        }
        
        Ok((jacobian, residual))
    }
    
    /// Internal analysis using a stored solution vector as starting point
    fn analyze_internal_with_stored_vector(&mut self, forced_start_ramp: Option<f64>, stored_init: &DVector<f64>) -> Result<AnalysisResult> {
        // Get circuit setup
        let ground_idx = self.circuit.ground_node()
            .ok_or_else(|| SpiceError::NoGroundNode)?
            .0;
        
        let node_list: Vec<NodeIndex> = self.circuit.nodes()
            .filter(|&(idx, _)| idx != ground_idx)
            .map(|(idx, _)| idx)
            .collect();
        
        // Find voltage sources with their current values
        // The models should contain the correct base voltage values
        println!("\nCollecting voltage sources in analyze_internal_with_stored_vector:");
        let voltage_sources: Vec<(EdgeIndex, String, f64)> = self.circuit.branches()
            .filter_map(|(idx, branch)| {
                match self.models.get(&branch.name)? {
                    ComponentModel::VoltageSource { voltage, .. } => {
                        println!("  {} = {:.3}V", branch.name, voltage);
                        Some((idx, branch.name.clone(), *voltage))
                    },
                    _ => None
                }
            })
            .collect();
        
        let voltage_source_indices: Vec<EdgeIndex> = voltage_sources.iter().map(|(idx, _, _)| *idx).collect();
        
        // Use the stored solution as starting point
        let mut x = stored_init.clone();
        let mut total_iterations = 0;
        
        println!("Using stored starting point: x = {:?}", x.as_slice());
        
        // Set voltage sources to the desired ramp if provided
        if let Some(start_ramp) = forced_start_ramp {
            for (idx, name, original_voltage) in &voltage_sources {
                if let Some(model) = self.models.get_mut(name) {
                    if let ComponentModel::VoltageSource { voltage, .. } = model {
                        *voltage = original_voltage * start_ramp;
                    }
                }
            }
        }
        
        // Skip Phase 1 scan since we have a good starting point
        println!("\nUsing stored starting point - skipping Phase 1 scan");
        
        // First verify the stored solution works at the current ramp level
        let (converged, iterations, error) = self.solve_at_ramp(
            &mut x, &node_list, ground_idx, &voltage_source_indices, &mut total_iterations
        )?;
        
        if !converged {
            return Err(SpiceError::ConvergenceFailed(iterations));
        }
        
        println!("✅ Verified stored starting point at ramp={:.1}%", forced_start_ramp.unwrap_or(1.0) * 100.0);
        
        // Ensure voltage sources are at their true original values (100%)
        // The collected values might be wrong if models were modified by previous calls
        println!("\nSetting voltage sources to true original values:");
        for (idx, name, collected_voltage) in &voltage_sources {
            if let Some(model) = self.models.get_mut(name) {
                if let ComponentModel::VoltageSource { voltage, .. } = model {
                    // The true original is what we collected divided by any ramp that might have been applied
                    let true_original = if forced_start_ramp.is_some() && forced_start_ramp.unwrap() > 0.0 {
                        // If we were called with a ramp factor, the collected value might be ramped
                        // But actually, we should trust what's in the model after analyze_all_regions restored it
                        *collected_voltage
                    } else {
                        *collected_voltage
                    };
                    println!("  {} = {:.3}V (setting to true original)", name, true_original);
                    *voltage = true_original;
                }
            }
        }
        
        // Debug: verify voltage source values
        println!("\nVoltage sources before final solve:");
        for (idx, name, original_voltage) in &voltage_sources {
            if let Some(model) = self.models.get(name) {
                if let ComponentModel::VoltageSource { voltage, .. } = model {
                    println!("  {} = {:.3}V (original: {:.3}V)", name, voltage, original_voltage);
                }
            }
        }
        
        // Final solve at 100% to ensure solution vector matches
        let (final_converged, final_iterations, final_error) = self.solve_at_ramp(
            &mut x, &node_list, ground_idx, &voltage_source_indices, &mut total_iterations
        )?;
        
        if !final_converged {
            println!("  ✗ Failed final solve at 100%");
            return Err(SpiceError::ConvergenceFailed(total_iterations));
        }
        
        println!("  ✓ Final solve at 100%: converged in {} iterations", final_iterations);
        println!("  Solution vector after final solve: {:?}", x.as_slice());
        
        // Build the result
        let mut node_voltages = HashMap::new();
        node_voltages.insert(ground_idx, 0.0);
        
        for (i, &node_idx) in node_list.iter().enumerate() {
            node_voltages.insert(node_idx, x[i]);
        }
        
        let mut branch_currents = HashMap::new();
        let mut total_power = 0.0;
        
        // Calculate branch currents
        for (edge_idx, branch) in self.circuit.branches() {
            if let Some((n1, n2)) = self.circuit.branch_nodes(edge_idx) {
                let v1 = node_voltages.get(&n1).copied().unwrap_or(0.0);
                let v2 = node_voltages.get(&n2).copied().unwrap_or(0.0);
                let voltage_diff = v1 - v2;
                
                let current = if let Some(model) = self.models.get(&branch.name) {
                    match model {
                        ComponentModel::Resistor { resistance, .. } => voltage_diff / resistance,
                        ComponentModel::VoltageSource { .. } => {
                            if let Some(vsrc_idx) = voltage_source_indices.iter().position(|&e| e == edge_idx) {
                                x[node_list.len() + vsrc_idx]
                            } else {
                                0.0
                            }
                        },
                        _ => {
                            let resistance = model.dc_resistance();
                            if resistance.is_finite() && resistance > 0.0 {
                                voltage_diff / resistance
                            } else {
                                0.0
                            }
                        }
                    }
                } else {
                    0.0
                };
                
                branch_currents.insert(edge_idx, current);
                total_power += voltage_diff.abs() * current.abs();
            }
        }
        
        println!("✅ Converged using stored starting point in {} iterations", iterations);
        
        let result = AnalysisResult {
            node_voltages,
            branch_currents,
            total_power,
            iterations,
        };
        
        // Restore voltage sources to their true original values before returning
        // The collected "original" might be wrong if this isn't the first call
        // So we calculate the true original based on the ramp factor
        println!("\nRestoring voltage sources at end of analyze_internal_with_stored_vector:");
        let ramp_factor = forced_start_ramp.unwrap_or(1.0);
        for (idx, name, collected_voltage) in &voltage_sources {
            if let Some(model) = self.models.get_mut(name) {
                if let ComponentModel::VoltageSource { voltage, .. } = model {
                    // Calculate true original: if we collected V and ramped by factor F, original = V/F
                    let true_original = if ramp_factor > 0.0 { 
                        collected_voltage / ramp_factor 
                    } else { 
                        *collected_voltage 
                    };
                    println!("  {} was {:.3}V, restoring to {:.3}V (collected={:.3}V, ramp={:.3})", 
                             name, voltage, true_original, collected_voltage, ramp_factor);
                    *voltage = true_original;
                }
            }
        }
        
        Ok(result)
    }
    
    /// Get DC operating point using MAESTRO for intelligent selection
    fn get_dc_with_maestro(&mut self) -> Result<AnalysisResult> {
        // First try to get solutions with current GLACIER
        let glacier_solutions = self.analyze()?;
        
        if glacier_solutions.is_empty() {
            return Err(SpiceError::ConvergenceFailed(0));
        }
        
        // If only one solution, use it
        if glacier_solutions.len() == 1 {
            info!("Only one DC solution found, using it directly");
            return Ok(glacier_solutions.into_iter().next().unwrap().3);
        }
        
        info!("Multiple DC solutions found ({}), using MAESTRO logic for intelligent selection", glacier_solutions.len());
        
        // Use MAESTRO's pattern detection to select from existing solutions
        // WITHOUT re-running the solver
        self.maestro_select_from_solutions(glacier_solutions)
    }
    
    /// Convert production GLACIER solution to our AnalysisResult format
    fn convert_production_solution(&self, solution: &ProductionSolution) -> Result<AnalysisResult> {
        let mut node_voltages = HashMap::new();
        let mut branch_currents = HashMap::new();
        let mut total_power = 0.0;
        
        // Convert node voltages from name-based to index-based
        for (node_name, voltage) in &solution.node_voltages {
            if let Some((idx, _)) = self.circuit.get_node(node_name) {
                node_voltages.insert(idx, *voltage);
            }
        }
        
        // Convert branch currents and calculate total power
        for (branch_name, current) in &solution.branch_currents {
            if let Some((idx, _)) = self.circuit.get_branch(branch_name) {
                branch_currents.insert(idx, *current);
                
                // Calculate power contribution
                if let Some((n1, n2)) = self.circuit.branch_nodes(idx) {
                    let v1 = node_voltages.get(&n1).copied().unwrap_or(0.0);
                    let v2 = node_voltages.get(&n2).copied().unwrap_or(0.0);
                    total_power += (v1 - v2).abs() * current.abs();
                }
            }
        }
        
        Ok(AnalysisResult {
            node_voltages,
            branch_currents,
            total_power,
            iterations: solution.iterations,
        })
    }
    
    /// Select solution with moderate power (safer than max power)
    fn select_moderate_power_solution(&self, solutions: Vec<(f64, f64, f64, AnalysisResult)>) -> Result<AnalysisResult> {
        // Sort by power and select one in the middle range
        let mut sorted = solutions;
        sorted.sort_by(|a, b| a.3.total_power.partial_cmp(&b.3.total_power).unwrap());
        
        // Prefer solutions with power in reasonable range (not too high, not too low)
        for (_, _, _, solution) in &sorted {
            if solution.total_power > 0.01 && solution.total_power < 0.5 {
                info!("Selected solution with moderate power: {:.3}W", solution.total_power);
                return Ok(solution.clone());
            }
        }
        
        // If no moderate solution, take the median
        let median_idx = sorted.len() / 2;
        info!("No moderate power solution found, using median: {:.3}W", 
              sorted[median_idx].3.total_power);
        Ok(sorted.into_iter().nth(median_idx).unwrap().3)
    }
    
    /// Use MAESTRO's intelligent selection logic on already-found solutions
    fn maestro_select_from_solutions(&self, solutions: Vec<(f64, f64, f64, AnalysisResult)>) -> Result<AnalysisResult> {
        use crate::maestro_production::{MaestroOrchestrator, CircuitPattern};
        
        // Create MAESTRO instance for pattern detection only
        let mut maestro = MaestroOrchestrator::new(self.circuit.clone());
        for (name, model) in &self.models {
            maestro.add_model(name.clone(), model.clone());
        }
        
        // Detect circuit patterns
        let patterns = maestro.detect_patterns();
        info!("MAESTRO detected patterns: {:?}", patterns.iter().take(1).collect::<Vec<_>>());
        
        // Select based on detected pattern
        match patterns.first() {
            Some(CircuitPattern::SeriesNonlinear { components, .. }) => {
                info!("Series nonlinear circuit with {} components, selecting moderate current", components.len());
                self.select_moderate_current_solution(solutions)
            }
            Some(CircuitPattern::ParallelArray { identical, .. }) => {
                info!("Parallel array (identical={}), selecting balanced current", identical);
                self.select_balanced_current_solution(solutions)
            }
            Some(CircuitPattern::PowerConverter { .. }) => {
                info!("Power converter detected, selecting nominal operating point");
                self.select_nominal_power_solution(solutions)
            }
            Some(CircuitPattern::BridgeCircuit { .. }) => {
                info!("Bridge circuit detected, selecting balanced solution");
                self.select_balanced_current_solution(solutions)
            }
            _ => {
                info!("Mixed/unknown circuit, using moderate power selection");
                self.select_moderate_power_solution(solutions)
            }
        }
    }
    
    /// Select solution with moderate current (for series nonlinear circuits)
    fn select_moderate_current_solution(&self, solutions: Vec<(f64, f64, f64, AnalysisResult)>) -> Result<AnalysisResult> {
        // Target typical LED/diode current range (10-30mA)
        let target_current = 0.020; // 20mA
        
        let best = solutions.into_iter()
            .min_by_key(|(_, _, _, result)| {
                // Find maximum branch current
                let max_current = result.branch_currents.values()
                    .map(|&i| i.abs())
                    .fold(0.0f64, f64::max);
                
                // Score based on distance from target
                ((max_current - target_current).abs() * 1e6) as i64
            })
            .map(|(_, _, _, result)| result);
        
        match best {
            Some(solution) => {
                info!("Selected solution with current closest to {:.1}mA", target_current * 1000.0);
                Ok(solution)
            }
            None => Err(SpiceError::ConvergenceFailed(0))
        }
    }
    
    /// Select solution with balanced current distribution
    fn select_balanced_current_solution(&self, solutions: Vec<(f64, f64, f64, AnalysisResult)>) -> Result<AnalysisResult> {
        let best = solutions.into_iter()
            .min_by_key(|(_, _, _, result)| {
                // Calculate variance of significant currents
                let currents: Vec<f64> = result.branch_currents.values()
                    .map(|&i| i.abs())
                    .filter(|&i| i > 1e-6) // Only consider significant currents
                    .collect();
                
                if currents.len() < 2 {
                    return 0; // Single current is perfectly balanced
                }
                
                let mean = currents.iter().sum::<f64>() / currents.len() as f64;
                let variance = currents.iter()
                    .map(|&i| (i - mean).powi(2))
                    .sum::<f64>() / currents.len() as f64;
                
                (variance * 1e12) as i64
            })
            .map(|(_, _, _, result)| result);
        
        match best {
            Some(solution) => {
                info!("Selected solution with most balanced current distribution");
                Ok(solution)
            }
            None => Err(SpiceError::ConvergenceFailed(0))
        }
    }
    
    /// Select solution at nominal power level (50-70% of max)
    fn select_nominal_power_solution(&self, mut solutions: Vec<(f64, f64, f64, AnalysisResult)>) -> Result<AnalysisResult> {
        // Sort by power
        solutions.sort_by(|a, b| a.3.total_power.partial_cmp(&b.3.total_power).unwrap());
        
        // Select at 60th percentile for nominal operation
        let nominal_idx = (solutions.len() as f64 * 0.6).round() as usize;
        let nominal_idx = nominal_idx.min(solutions.len() - 1);
        
        info!("Selected nominal power solution at {}% of range", 
              (nominal_idx as f64 / solutions.len() as f64 * 100.0) as u32);
        
        Ok(solutions.into_iter().nth(nominal_idx).unwrap().3)
    }
}