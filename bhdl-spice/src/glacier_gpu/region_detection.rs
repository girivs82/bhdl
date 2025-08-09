//! GPU-based gradient region detection for GLACIER solver
//! 
//! Implements the same gradient-based region detection as the CPU solver
//! but adapted for f32 precision with auto-scaling.

use crate::glacier_gpu::gpu_data::Phase0Result;
use log::{info, debug};

/// Region information detected from Phase 0 scan
#[derive(Debug, Clone)]
pub struct GpuRegion {
    pub start: f32,
    pub end: f32,
    pub representative_ramp: f32,
    pub log_gradient: f32,
    pub converged: bool,
}

/// Detect regions from Phase 0 results using gradient analysis
/// This matches the CPU solver's approach but accounts for f32 precision
pub fn detect_gradient_regions(phase0_results: &[Phase0Result]) -> Vec<GpuRegion> {
    info!("GPU gradient-based region detection on {} points", phase0_results.len());
    
    if phase0_results.is_empty() {
        return vec![];
    }
    
    // First pass: Calculate log gradients for each converged point
    let mut scan_data: Vec<(f32, f32, bool)> = Vec::new(); // (ramp, log_gradient, converged)
    
    for i in 0..phase0_results.len() {
        let result = &phase0_results[i];
        let converged = result.converged != 0 && result.iterations > 0;
        
        if converged {
            // Estimate log gradient from neighboring points
            let log_gradient = calculate_log_gradient_at_point(phase0_results, i);
            scan_data.push((result.ramp, log_gradient, true));
        } else {
            scan_data.push((result.ramp, 1000.0, false)); // High gradient for non-converged
        }
    }
    
    // Second pass: Identify sharp transitions and unstable regions
    let mut sharp_transitions = Vec::new();
    let gradient_threshold = 100.0_f32; // Threshold for sharp transition
    let gradient_change_threshold = 10.0_f32; // Threshold for rate of change
    
    for i in 1..scan_data.len() {
        let (ramp, gradient, converged) = scan_data[i];
        let (prev_ramp, prev_gradient, prev_converged) = scan_data[i-1];
        
        if converged && prev_converged {
            // Check for sharp gradient change
            let gradient_ratio = if prev_gradient > 0.0 {
                gradient / prev_gradient
            } else {
                1.0
            };
            
            let inverse_ratio = if gradient > 0.0 {
                prev_gradient / gradient
            } else {
                1.0
            };
            
            // Detect sharp transitions
            if gradient > gradient_threshold || 
               gradient_ratio > gradient_change_threshold || 
               inverse_ratio > gradient_change_threshold {
                sharp_transitions.push((prev_ramp, ramp));
                debug!("Sharp transition detected between {:.1}% and {:.1}%", 
                       prev_ramp * 100.0, ramp * 100.0);
            }
        }
    }
    
    // Third pass: Build stable regions
    let mut regions = Vec::new();
    let mut current_region_start = 0.0_f32;
    let mut in_unstable_region = false;
    
    for i in 0..scan_data.len() {
        let (ramp, gradient, converged) = scan_data[i];
        
        // Check if this point is in an unstable region
        let is_unstable = !converged || gradient > gradient_threshold;
        
        // Check if we're at a transition boundary
        let at_transition = sharp_transitions.iter()
            .any(|(start, end)| ramp >= *start && ramp <= *end);
        
        if (is_unstable || at_transition) && !in_unstable_region {
            // End current stable region
            if ramp > current_region_start + 0.05 {
                let region_end = (ramp - 0.01).max(current_region_start);
                
                // Find best representative point in the region
                let mut best_ramp = (current_region_start + region_end) / 2.0;
                let mut best_gradient = gradient_threshold;
                
                for j in 0..scan_data.len() {
                    let (r, g, c) = scan_data[j];
                    if c && r >= current_region_start && r <= region_end && g < best_gradient {
                        best_ramp = r;
                        best_gradient = g;
                    }
                }
                
                regions.push(GpuRegion {
                    start: current_region_start,
                    end: region_end,
                    representative_ramp: best_ramp,
                    log_gradient: best_gradient,
                    converged: true,
                });
                
                info!("GPU Region {}: [{:.1}%-{:.1}%], gradient={:.1}", 
                      regions.len(), current_region_start * 100.0, region_end * 100.0, best_gradient);
            }
            in_unstable_region = true;
        } else if !is_unstable && !at_transition && in_unstable_region {
            // Start new stable region
            current_region_start = ramp;
            in_unstable_region = false;
        }
    }
    
    // Handle final region
    if !in_unstable_region && 1.0 > current_region_start + 0.05 {
        let mut best_ramp = (current_region_start + 1.0) / 2.0;
        let mut best_gradient = gradient_threshold;
        
        for (r, g, c) in &scan_data {
            if *c && *r >= current_region_start && *g < best_gradient {
                best_ramp = *r;
                best_gradient = *g;
            }
        }
        
        regions.push(GpuRegion {
            start: current_region_start,
            end: 1.0,
            representative_ramp: best_ramp,
            log_gradient: best_gradient,
            converged: true,
        });
        
        info!("GPU Region {}: [{:.1}%-100%], gradient={:.1}", 
              regions.len(), current_region_start * 100.0, best_gradient);
    }
    
    // If no regions found, create a single region (linear circuit)
    if regions.is_empty() {
        let all_converged: Vec<_> = scan_data.iter()
            .filter(|(_, _, c)| *c)
            .collect();
            
        if !all_converged.is_empty() {
            let avg_gradient = all_converged.iter()
                .map(|(_, g, _)| *g)
                .sum::<f32>() / all_converged.len() as f32;
                
            regions.push(GpuRegion {
                start: 0.0,
                end: 1.0,
                representative_ramp: 0.5,
                log_gradient: avg_gradient,
                converged: true,
            });
            
            info!("GPU single region detected (linear circuit), avg gradient={:.1}", avg_gradient);
        }
    }
    
    regions
}

/// Calculate log gradient at a specific point by examining neighboring solutions
fn calculate_log_gradient_at_point(results: &[Phase0Result], index: usize) -> f32 {
    // For GPU, we estimate gradient from the convergence behavior
    // Since we don't have access to individual component currents in Phase0Result,
    // we use the error behavior and iteration count as proxies
    
    let result = &results[index];
    
    // Base gradient estimate from iteration count
    // More iterations typically means sharper gradients
    let iteration_factor = (result.iterations as f32 / 10.0).min(10.0).max(1.0);
    
    // Error magnitude also indicates gradient
    // Smaller errors with more iterations suggest sharp transitions
    let error_factor = if result.error > 0.0 {
        (1.0 / result.error.log10().abs()).min(10.0).max(1.0)
    } else {
        1.0
    };
    
    // Look at neighboring points for rate of change
    let mut neighbor_factor = 1.0;
    
    if index > 0 && index < results.len() - 1 {
        let prev = &results[index - 1];
        let next = &results[index + 1];
        
        if prev.converged != 0 && next.converged != 0 {
            // Calculate approximate derivative of error
            let ramp_diff = next.ramp - prev.ramp;
            if ramp_diff > 0.0 {
                let error_diff = (next.error - prev.error).abs();
                let error_derivative = error_diff / ramp_diff;
                
                // High derivative suggests sharp transition
                neighbor_factor = (error_derivative * 100.0).min(10.0).max(1.0);
            }
        }
    }
    
    // Combine factors to estimate log gradient
    // This is a heuristic that approximates the CPU's component-based calculation
    let estimated_gradient = iteration_factor * error_factor * neighbor_factor;
    
    // Clamp to reasonable range
    estimated_gradient.min(1000.0).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_single_region_detection() {
        // Test with a simple linear circuit (all points converge easily)
        let mut results = vec![];
        for i in 0..20 {
            results.push(Phase0Result {
                ramp: i as f32 * 0.05,
                converged: 1,
                iterations: 5,
                error: 1e-6,
            });
        }
        
        let regions = detect_gradient_regions(&results);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].start, 0.0);
        assert_eq!(regions[0].end, 1.0);
    }
    
    #[test]
    fn test_sharp_transition_detection() {
        // Test with a sharp transition in the middle
        let mut results = vec![];
        
        // First stable region
        for i in 0..10 {
            results.push(Phase0Result {
                ramp: i as f32 * 0.05,
                converged: 1,
                iterations: 5,
                error: 1e-6,
            });
        }
        
        // Sharp transition region
        for i in 10..12 {
            results.push(Phase0Result {
                ramp: i as f32 * 0.05,
                converged: 1,
                iterations: 100, // High iteration count
                error: 1e-3,     // Higher error
            });
        }
        
        // Second stable region
        for i in 12..20 {
            results.push(Phase0Result {
                ramp: i as f32 * 0.05,
                converged: 1,
                iterations: 8,
                error: 1e-6,
            });
        }
        
        let regions = detect_gradient_regions(&results);
        assert!(regions.len() >= 2); // Should detect at least 2 regions
    }
}