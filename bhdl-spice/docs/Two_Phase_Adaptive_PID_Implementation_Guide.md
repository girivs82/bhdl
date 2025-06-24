# Two-Phase Adaptive PID Implementation Guide

## Overview

This guide provides comprehensive documentation for the Two-Phase Adaptive PID solver implementation, which achieves sub-1% accuracy (0.15% average error) while maintaining the key advantage of complete genericity - no circuit knowledge required.

## Core Concepts

### 1. Two-Phase Strategy

The solver operates in two distinct phases:

**Phase 1: Rapid Progress (0% to ~90%)**
- Goal: Quickly approach the solution
- PID Gains: Kp=2.0, Ki=0.4, Kd=0.01
- Target Error: 1e-11
- Max Ramp Rate: 0.2 (20% per step)
- Behavior: Aggressive ramping with moderate precision

**Phase 2: Precision Refinement (90% to 100%)**
- Goal: Ultra-precise convergence
- PID Gains: Kp=1.0, Ki=0.2, Kd=0.02
- Target Error: 1e-15
- Max Ramp Rate: 0.05-0.1 (adaptive)
- Behavior: Careful refinement with high precision

### 2. Adaptive PID Control

The PID controller adapts its gains based on the logarithmic gradient (device sensitivity):

```rust
log_gradient = d(ln(I))/dV ≈ 1/Vt for diodes
```

This gradient indicates how sensitive the device is to voltage changes:
- Low gradient (< 2.0): Device is insensitive (e.g., high Vt diode)
- Normal gradient (10-30): Standard sensitivity
- High gradient (> 30): Very sensitive device

### 3. Phase Switching Logic

The solver switches from Phase 1 to Phase 2 when BOTH conditions are met:
1. Ramp factor > 0.9 (reached 90% of target)
2. Convergence error < 1e-10

This ensures the solver only enters precision mode when it's close to the solution.

## Implementation Details

### PID Gain Adaptation Rules

| Log Gradient | Device Type | Kp Multiplier | Ki Multiplier | Kd Multiplier |
|-------------|-------------|---------------|---------------|---------------|
| < 2.0 | Very low sensitivity | 2.0x | 3.0x | 0.5x |
| 2.0 - 10.0 | Low sensitivity | 1.5x | 2.0x | 0.7x |
| 10.0 - 30.0 | Normal sensitivity | 1.0x | 1.0x | 1.0x |
| > 30.0 | High sensitivity | 0.8x | 0.7x | 1.2x |

### Ramp Rate Control

The ramp rate is controlled by the PID output:

```rust
error_ratio = ln(error / target_error)
pid_output = pid.update(error_ratio, dt)
rate_multiplier = exp(-pid_output * 0.1)
ramp_rate *= rate_multiplier
```

### Bounds on Ramp Rate

**Phase 1:**
- Minimum: 1e-4 (0.01% per step)
- Maximum: 0.2 (20% per step)

**Phase 2:**
- Minimum: 1e-6 to 1e-5 (gradient-dependent)
- Maximum: 0.05 to 0.1 (gradient-dependent)

### Final Convergence Push

After reaching 100% ramp, the solver performs up to 20 additional iterations:

```rust
for pass in 0..20 {
    let (converged, iters, error) = self.solve_to_convergence();
    if error < 1e-16 || (pass > 10 && error < 1e-15) {
        break;
    }
}
```

## Performance Characteristics

### Accuracy vs Speed Trade-off

| Implementation | Average Error | Average Time | Use Case |
|----------------|---------------|--------------|-----------|
| Original Adaptive | 3.55% | 21.5ms | Rapid prototyping |
| Two-Phase PID | 0.15% | 355.7ms | Precision applications |

### Convergence Behavior

1. **Phase 1 (0-90%)**: ~100ms, reaches within 1% of solution
2. **Phase 2 (90-100%)**: ~250ms, refines to 0.15% accuracy
3. **Final push**: ~5ms, ensures ultra-precision

### Memory Usage

- History tracking: Minimal (last voltage/current only)
- PID state: 8 doubles per controller
- No extensive history buffers needed

## Circuit Compatibility

### Successfully Tested On:

1. **Simple Circuits**
   - Single diode with various Vt (0.026V to 0.050V)
   - Wide voltage range (0.05V to 10V)
   - Wide resistance range (10Ω to 2kΩ)

2. **Complex Topologies**
   - Series diodes (2-10 in series)
   - Parallel diodes (current sharing)
   - Bridge rectifiers (4 diodes)
   - Voltage multipliers
   - Multiple voltage sources (OR-ing, series)

3. **Extreme Cases**
   - Very high current (0.1Ω, large Is)
   - Near zero current (1MΩ, 10mV)
   - Temperature variations (-40°C to 125°C)

### Performance on Complex Circuits

| Circuit Type | Convergence Time | Iterations | Success |
|--------------|------------------|------------|---------|
| Single diode | 200-500ms | 50k-200k | 100% |
| 2-3 series diodes | 800-1600ms | 150k-250k | 100% |
| Bridge rectifier | 600ms | 120k | 100% |
| 10 series diodes | 9.2s | 265k | 100% |

## Usage Guidelines

### When to Use Two-Phase PID

✅ **Ideal for:**
- Applications requiring < 1% error
- Complex circuits where Newton might fail
- Working with tabulated data (IBIS models)
- Research requiring consistent results
- Educational environments (transparent operation)

❌ **Not ideal for:**
- Real-time simulation (> 300ms per solution)
- Simple circuits where 3-5% error is acceptable
- Large-scale circuit simulation (too slow)

### Best Practices

1. **Initial Setup**
   - Ensure proper circuit connectivity
   - Initialize all nodes to 0V
   - Set reasonable component values

2. **Convergence Monitoring**
   - Watch for phase transitions
   - Monitor log gradient evolution
   - Check final error achievement

3. **Performance Optimization**
   - For simple circuits, consider using original adaptive version
   - For known good circuits, can reduce final push iterations
   - Can adjust phase switching thresholds if needed

## Troubleshooting

### Common Issues and Solutions

1. **Slow Convergence**
   - Check for extremely high/low component values
   - Verify circuit connectivity
   - Consider if circuit has multiple solutions

2. **Phase 2 Never Reached**
   - Circuit may be too complex for 90% threshold
   - Try reducing phase switch threshold to 0.85
   - Check if error threshold (1e-10) is too strict

3. **Oscillating Convergence**
   - Increase derivative gain (Kd) in PID
   - Reduce maximum ramp rate
   - Check for numerical overflow in components

## Code Structure

### Key Components

1. **AdaptivePIDController**
   - Manages proportional, integral, derivative control
   - Adapts gains based on log gradient
   - Tracks integral and last error

2. **TwoPhasePIDSolver**
   - Main solver class
   - Manages phase transitions
   - Implements ramping strategy

3. **Element Trait**
   - Generic interface for circuit components
   - Supports resistors, voltage sources, diodes
   - Extensible for new component types

### Extension Points

To add new component types:

1. Implement the `Element` trait
2. Define `current_at_voltage()` and `conductance_at_voltage()`
3. Set `is_nonlinear()` appropriately
4. No changes needed to solver logic!

## Mathematical Foundation

### Logarithmic Gradient

For exponential devices:
```
I = Is * (exp(V/Vt) - 1)
d(ln(I))/dV = 1/Vt
```

This gradient is device-independent for a given temperature, making it ideal for generic analysis.

### PID Control Theory

The PID controller minimizes the error between actual and target convergence:
```
u(t) = Kp*e(t) + Ki*∫e(t)dt + Kd*de/dt
```

Where:
- e(t) = ln(actual_error / target_error)
- u(t) = ramp rate adjustment

### Newton-Raphson Core

Each convergence step uses Newton-Raphson:
```
x[n+1] = x[n] - J^(-1) * F(x[n])
```

With damping factor 0.6-0.8 for stability.

## Comparison with Other Approaches

### vs Original Adaptive Threshold

| Aspect | Original | Two-Phase PID |
|--------|----------|---------------|
| Complexity | Simple threshold-based | PID with phase control |
| Accuracy | 3.55% | 0.15% |
| Speed | 21.5ms | 355.7ms |
| Robustness | Good | Excellent |
| Parameter tuning | Minimal | None needed |

### vs Newton-Raphson

| Aspect | Newton-Raphson | Two-Phase PID |
|--------|----------------|---------------|
| Circuit knowledge | Required | Not required |
| Initial guess | Critical | Not needed |
| Convergence basins | Multiple possible | Consistent via ramping |
| IBIS compatibility | No (needs conversion) | Yes (direct) |
| Speed | 0.6ms | 355.7ms |
| Accuracy | 0.31% | 0.15% |

## Future Enhancements

### Potential Improvements

1. **Adaptive Phase Switching**
   - Learn optimal switch point from circuit
   - Could reduce total time by 10-20%

2. **Parallel Device Handling**
   - Track multiple nonlinear devices
   - Adapt based on most sensitive device

3. **Predictive Ramping**
   - Use early convergence pattern to predict final ramp
   - Skip intermediate steps when safe

4. **Hybrid Approach**
   - Start with Two-Phase PID
   - Switch to Newton for final refinement
   - Best of both worlds

## References

1. "Adaptive Logarithmic Gradient Circuit Solver" - Original research paper
2. PID control theory - Åström & Hägglund
3. Newton-Raphson methods in circuit simulation - Nagel (SPICE)
4. Numerical methods for stiff ODEs - Gear

## Appendix: Complete Test Results

Full test results on 22 circuits are available in:
- `test_solver_comprehensive.rs` - Test implementation
- Research paper Appendix C - Detailed results

The solver achieved 100% convergence on all test circuits with an average error of 0.15%.