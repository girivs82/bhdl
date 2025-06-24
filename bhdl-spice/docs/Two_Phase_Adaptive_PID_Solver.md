# Two-Phase Adaptive PID Logarithmic Gradient Solver

## Overview

This document describes the implementation of a highly accurate (< 1% error) logarithmic gradient solver for nonlinear circuit simulation. The solver achieves 0.15% average error (23.8x better than the reference paper's 3.55%) while maintaining reasonable performance (355.7ms average).

## Key Innovation: Two-Phase Strategy

The solver uses a two-phase approach to balance speed and accuracy:

1. **Phase 1 (Rapid Progress)**: Quickly ramp to ~90% using moderately aggressive PID control
2. **Phase 2 (Precision)**: Switch to precision mode with adaptive gains based on device sensitivity

## Implementation Details

### Core Algorithm

The solver implements source ramping with logarithmic gradient tracking:

```rust
// Track logarithmic gradient (sensitivity)
let dv = element_voltage - last_voltage;
let dlog_i = current.ln() - last_current.ln();
log_gradient = (dlog_i / dv).abs();
```

This gradient represents the device sensitivity: `d(ln(I))/dV ≈ 1/Vt` for diodes.

### Adaptive PID Controller

The PID controller adapts its gains based on the logarithmic gradient:

```rust
struct AdaptivePIDController {
    base_kp: f64,  // Base proportional gain
    base_ki: f64,  // Base integral gain
    base_kd: f64,  // Base derivative gain
    kp: f64,       // Active proportional gain
    ki: f64,       // Active integral gain
    kd: f64,       // Active derivative gain
    integral: f64,
    last_error: f64,
}
```

### Phase 1 Configuration

- **Base gains**: Kp=2.0, Ki=0.4, Kd=0.01
- **Initial ramp rate**: 0.1
- **Target error**: 1e-11
- **Max ramp rate**: 0.2
- **Min ramp rate**: 1e-4

### Phase 2 Configuration

- **Base gains**: Kp=1.0, Ki=0.2, Kd=0.02
- **Initial ramp rate**: 0.02 (when switching)
- **Target error**: 1e-15
- **Max ramp rate**: 0.05-0.1 (depends on gradient)
- **Min ramp rate**: 1e-6 to 1e-5 (depends on gradient)

### Gain Adaptation Rules

Based on logarithmic gradient (device sensitivity):

| Gradient Range | Classification | Kp Multiplier | Ki Multiplier | Kd Multiplier | Example |
|----------------|----------------|---------------|---------------|---------------|---------|
| < 2.0 | Very low sensitivity | 2.0x | 3.0x | 0.5x | High Vt diode |
| 2.0 - 10.0 | Low sensitivity | 1.5x | 2.0x | 0.7x | Moderate Vt |
| 10.0 - 30.0 | Normal sensitivity | 1.0x | 1.0x | 1.0x | Standard diode |
| > 30.0 | High sensitivity | 0.8x | 0.7x | 1.2x | Low Vt/high current |

### Phase Switching Logic

The solver switches from Phase 1 to Phase 2 when:
- Ramp factor > 0.9 (90% progress) AND
- Convergence error < 1e-10

```rust
if phase == 1 && ramp_factor > 0.9 && error < 1e-10 {
    println!("  → Switching to precision phase at ramp={:.3}", ramp_factor);
    phase = 2;
    // Reset PID with precision parameters
    pid = AdaptivePIDController::new(1.0, 0.2, 0.02);
    ramp_rate = 0.02; // Slow down for precision
}
```

### PID Control Law

The PID controls the ramp rate based on convergence error:

```rust
let target_error = if phase == 1 { 1e-11 } else { 1e-15 };
let error_ratio = (error / target_error).ln().max(-10.0).min(10.0);
let pid_output = pid.update(error_ratio, 0.01);
let rate_multiplier = (-pid_output * 0.1).exp();
ramp_rate *= rate_multiplier;
```

### Exit Conditions

The solver exits the ramping loop when:
- Ultra-precision achieved: error < 1e-16 AND ramp_factor > 0.999
- Or: ramp_factor reaches 1.0

### Final Convergence Push

After reaching 100% ramp, the solver performs up to 20 additional Newton-Raphson iterations to ensure maximum precision:

```rust
for pass in 0..20 {
    let (converged, iters, error) = self.solve_to_convergence(&mut total_iterations);
    if error < best_error {
        best_error = error;
    }
    if error < 1e-16 || (pass > 10 && error < 1e-15) {
        break;
    }
}
```

## Performance Characteristics

### Test Results (7 standard test cases)

| Test Case | Vs | Rs | Is | Vt | Error | Time | Iterations |
|-----------|----|----|----|----|-------|------|------------|
| Baseline | 1.0V | 100Ω | 1e-12 | 0.026V | 0.000% | ~500ms | ~140k |
| High Vt | 1.0V | 100Ω | 1e-12 | 0.050V | 0.001% | ~300ms | ~80k |
| Low Current | 0.1V | 1kΩ | 1e-12 | 0.026V | 0.208% | ~10ms | ~3k |
| High Voltage | 5.0V | 100Ω | 1e-12 | 0.026V | 0.000% | ~500ms | ~140k |
| Low Resistance | 1.0V | 10Ω | 1e-12 | 0.026V | 0.000% | ~400ms | ~110k |
| Extreme Low | 0.05V | 2kΩ | 1e-12 | 0.026V | 0.840% | ~10ms | ~2k |
| High Current | 10.0V | 50Ω | 1e-12 | 0.026V | 0.000% | ~500ms | ~140k |

**Average**: 0.15% error, 355.7ms

### Comparison with Reference

- **Paper**: 3.55% error, 21.5ms
- **Our solver**: 0.15% error, 355.7ms
- **Accuracy improvement**: 23.8x better
- **Speed trade-off**: 16.5x slower

## Key Success Factors

1. **Two-phase approach**: Balances speed in initial ramping with precision in final convergence
2. **Adaptive gains**: Adjusts PID parameters based on device sensitivity (log gradient)
3. **No early exit**: Continues refining until ultra-high precision achieved
4. **Extended final push**: Multiple passes at 100% ensure best possible accuracy
5. **Appropriate damping**: 0.5-0.85 damping factor in Newton-Raphson iterations

## Implementation File

The complete implementation is in: `/Users/girivs/src/bhdl-new/bhdl-spice/src/bin/simple_pid_ramping.rs`

## Usage

```rust
let mut solver = PIDRampingSolver::new(num_nodes);

// Add elements
let vs_idx = solver.add_element(Box::new(VoltageSource::new(voltage)));
let r_idx = solver.add_element(Box::new(Resistor::new(resistance)));
let d_idx = solver.add_element(Box::new(Diode::new(is, vt)));

// Connect elements
solver.connect(vs_idx, 1, 0);  // VS between nodes 1 and 0
solver.connect(r_idx, 1, 2);   // R between nodes 1 and 2
solver.connect(d_idx, 2, 0);   // D between nodes 2 and 0

// Solve
let (voltages, time_ms, iterations) = solver.solve_with_pid();
```

## Future Optimizations

1. **Parallel ramping**: For circuits with multiple independent nonlinear devices
2. **Adaptive phase switching**: Learn optimal switching point from circuit characteristics
3. **Gradient prediction**: Use ML to predict sensitivity before ramping
4. **Multi-rate ramping**: Different ramp rates for different voltage sources