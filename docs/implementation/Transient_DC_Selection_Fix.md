# Transient Solver DC Operating Point Selection Fix

## Issue Summary

The current transient solver in `glacier_solver.rs` uses a simplistic "maximum total power" heuristic to select the DC operating point from multiple GLACIER solutions. This can lead to non-physical or unstable initial conditions for transient simulation.

## Current Implementation

In `glacier_solver.rs` (lines 2910-2920):
```rust
// Get DC operating point
let dc_point = match initial_conditions {
    Some(ic) => ic,
    None => {
        // Run DC analysis and select solution with max power
        self.analyze()
            .and_then(|solutions| solutions.into_iter()
                .max_by(|a, b| a.3.total_power.partial_cmp(&b.3.total_power).unwrap())
                .map(|s| s.3)
                .ok_or(SpiceError::ConvergenceFailed(0)))?
    }
};
```

## Problem with Maximum Power Selection

1. **Non-Physical States**: Selecting maximum power often chooses high-dissipation states that may exceed component ratings
2. **Instability**: High-power solutions are often near unstable operating regions
3. **Component Damage**: Can select states with excessive currents through sensitive components
4. **Inefficiency**: Doesn't consider circuit efficiency or intended operating point

## Proposed Solution: MAESTRO Integration

### 1. Direct MAESTRO Integration
```rust
// Get DC operating point using MAESTRO
let dc_point = match initial_conditions {
    Some(ic) => ic,
    None => {
        // Use MAESTRO for intelligent selection
        let solutions = self.analyze()?;
        if solutions.len() > 1 {
            // Convert to production format and use MAESTRO
            let maestro_solutions = convert_to_maestro_format(solutions);
            let best = solve_with_glacier_maestro(self.circuit.clone(), self.models.clone())?;
            convert_back_to_glacier_format(best)
        } else {
            // Single solution - use it
            solutions.into_iter().next().map(|s| s.3)
                .ok_or(SpiceError::ConvergenceFailed(0))?
        }
    }
};
```

### 2. Physical Validity Scoring
If MAESTRO integration is too complex, implement a simple physical validity score:

```rust
fn select_best_dc_solution(solutions: &[(f64, f64, f64, AnalysisResult)]) -> Option<AnalysisResult> {
    solutions.iter()
        .max_by(|a, b| {
            let score_a = calculate_physical_score(&a.3);
            let score_b = calculate_physical_score(&b.3);
            score_a.partial_cmp(&score_b).unwrap()
        })
        .map(|s| s.3.clone())
}

fn calculate_physical_score(result: &AnalysisResult) -> f64 {
    let mut score = 1.0;
    
    // Prefer moderate power (penalize both too high and too low)
    let power_score = if result.total_power < 0.5 {
        1.0 - (result.total_power - 0.1).abs() / 0.4
    } else {
        0.2 / result.total_power  // Heavy penalty for high power
    };
    
    // Check current levels are reasonable
    let max_current = result.branch_currents.values()
        .map(|i| i.abs())
        .fold(0.0, f64::max);
    
    let current_score = if max_current < 0.050 {  // 50mA limit
        1.0 - (max_current - 0.020).abs() / 0.030
    } else {
        0.1  // Heavy penalty for overcurrent
    };
    
    score * power_score * current_score
}
```

## Benefits of Fix

1. **Physical Accuracy**: Selects operating points that match intended circuit behavior
2. **Component Safety**: Avoids states that could damage components
3. **Stability**: Chooses stable operating regions for reliable transient simulation
4. **Efficiency**: Prefers lower-power solutions when multiple options exist

## Implementation Steps

1. Add MAESTRO integration to `glacier_solver.rs`
2. Create conversion functions between solver formats
3. Add configuration option to enable/disable MAESTRO selection
4. Test with circuits that have multiple operating points
5. Verify transient stability from selected DC points

## Test Results

Our testing showed that GLACIER often finds multiple solutions with different characteristics:
- Solution 1: Low power, all LEDs conducting normally
- Solution 2: Medium power, some LEDs in different states  
- Solution 3: High power, potentially damaging currents

The current "max power" selection would choose Solution 3, while MAESTRO or physical scoring would correctly select Solution 1.

## Conclusion

The transient solver's DC operating point selection should be updated to use either:
1. MAESTRO's intelligent circuit-aware selection (preferred)
2. A physical validity scoring system (simpler alternative)

This ensures transient simulations start from realistic, stable operating points.