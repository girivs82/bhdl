# GLACIER Transient Analysis Extension
## Technical Specification v1.0

*June 2025*

## 1. Overview

This document specifies the transient analysis extensions for GLACIER (Gradient Logarithmic Adaptive Circuit Intelligent Exploration Resolver). The focus is on extending the core logarithmic transformation and gradient-aware techniques from DC to time-domain analysis while maintaining numerical robustness.

### Key Innovation Summary

**The Problem**: In transient analysis, time derivatives of exponential devices (di/dt) still contain exponentials, defeating the purpose of logarithmic transformation.

**Our Solution**: Instead of transforming derivatives, we:
1. Keep exponential currents as log variables (w = log(i))
2. Build companion models that relate w and v linearly
3. Use scaled matrix assembly to avoid computing large exponentials
4. Solve the system where exponential relationships become linear

This is fundamentally different from prior log-scaling approaches that only transform variables - we transform the entire numerical framework to avoid exponentials throughout the computation.

## 2. Core GLACIER Transient Innovations

### 2.0 Key Insight: Avoiding Exponentials in Transient

The fundamental challenge in transient analysis is that time derivatives of exponential devices still contain exponentials. For a diode/LED:

```
i = Is * (exp(v/Vt) - 1)
di/dt = (Is/Vt) * exp(v/Vt) * dv/dt  ← Still has exp()!
```

Simply taking log(di/dt) doesn't help because we still need to compute the exponential.

**Our Solution**: Work entirely in log space for BOTH the variables AND the system construction:

1. **Variables in log space**: w = log(i) for currents
2. **Companion models in log space**: Store log(G) and log(I)
3. **System assembly with scaling**: Avoid exponentiating large values
4. **Direct updates in log space**: w_new = w_old + Δw (no exp needed)

This is fundamentally different from just log-scaling variables - we transform the entire numerical framework.

### 2.1 Time-Domain Logarithmic Transformation

#### Mathematical Foundation

For time-varying exponential relationships in semiconductor devices:
```
i(t) = Is * (exp(v(t)/Vt) - 1)
```

The time derivative in original space:
```
di/dt = (Is/Vt) * exp(v(t)/Vt) * dv/dt
```

**Problem**: Taking d(log(i))/dt still contains exponentials and doesn't help.

**Key Innovation**: Transform both the variable AND its derivative to log space.

Let `w = log(i)` and `z = log(di/dt)` when di/dt > 0.

For the companion model, instead of:
```
i_new = i_old + (di/dt) * dt
```

We use:
```
log(i_new) = log(i_old + (di/dt) * dt)
           = log(i_old * (1 + (di/dt)*dt/i_old))
           ≈ log(i_old) + log(1 + (di/dt)*dt/i_old)
           ≈ w_old + (di/dt)*dt/i_old
```

For large currents where i >> Is:
```
i ≈ Is * exp(v/Vt)
log(i) ≈ log(Is) + v/Vt
```

So the companion model becomes linear in log space:
```
w_new = w_old + (v_new - v_old)/(Vt * dt) * dt
      = w_old + (v_new - v_old)/Vt
```

**This is the key**: In log space, the exponential device becomes a linear conductor!

#### Implementation

```rust
pub struct LogarithmicTimeDerivative {
    vt: f64,  // Thermal voltage
}

impl LogarithmicTimeDerivative {
    pub fn compute_log_space_update(&self, w_old: f64, v_old: f64, 
                                   v_new: f64, dt: f64) -> f64 {
        // w = log(i), update directly in log space
        // For exponential device: log(i) ≈ log(Is) + v/Vt
        
        if v_old > 4.0 * self.vt {
            // Strong forward bias - linear in log space
            w_old + (v_new - v_old) / self.vt
        } else {
            // Near threshold - use careful update
            let i_old = w_old.exp();
            let di_dv = i_old / self.vt;  // Simplified derivative
            let di = di_dv * (v_new - v_old);
            
            // Logarithmic update
            if (i_old + di * dt) > 0.0 {
                (i_old + di * dt).ln()
            } else {
                w_old - 10.0  // Large negative log for zero current
            }
        }
    }
}
```

### 2.2 Companion Models in Log Space

The real innovation is how we handle reactive components (capacitors/inductors) with logarithmic currents:

#### Capacitor Companion in Log Space

For a capacitor with current i = C * dv/dt:

```rust
pub struct LogCapacitorCompanion {
    capacitance: f64,
}

impl LogCapacitorCompanion {
    pub fn build_log_companion(&self, v_old: f64, w_old: f64, dt: f64) -> (f64, f64) {
        // w = log(i) for the capacitor current
        // In normal space: i_new = C * (v_new - v_old) / dt
        // In log space: w_new = log(C * (v_new - v_old) / dt)
        
        // For the companion model G*v + I = 0:
        // We need to express this in terms of log currents
        
        // Key insight: For small dt, capacitor current can be large
        // So we work with log(G) and log(I) directly
        
        let log_g = (self.capacitance / dt).ln();
        let log_i_history = w_old;  // Previous log current
        
        // Return coefficients for log-space equation
        (log_g, log_i_history)
    }
}
```

#### The Log-Space MNA System

Instead of solving:
```
[G][V] + [I] = 0
```

We solve:
```
[exp(log_G)] * [V] + [exp(log_I)] = 0
```

But we construct and solve it carefully to avoid exponential overflow:

```rust
pub struct LogSpaceMNA {
    // Store conductances and currents in log form
    log_conductances: SparseMatrix<f64>,
    log_currents: Vector<f64>,
}

impl LogSpaceMNA {
    pub fn solve(&self) -> Result<Vector<f64>> {
        // Key: We don't exponentiate everything at once
        // Instead, we use relative scaling
        
        let max_log_g = self.log_conductances.max();
        let scaled_g = self.log_conductances.map(|g| (g - max_log_g).exp());
        let scaled_i = self.log_currents.map(|i| (i - max_log_g).exp());
        
        // Now solve the scaled system
        scaled_g.solve(&scaled_i)
    }
}
```

### 2.3 Practical Example: LED with Capacitor

Consider a simple circuit: LED with parallel capacitor, switching on.

Traditional approach faces these equations:
```
i_led = 1e-30 * (exp(v/0.026) - 1)    // Exponential!
i_cap = C * dv/dt                      // Time derivative
i_total = i_led + i_cap = I_source     // KCL
```

GLACIER's log-space approach:
```
w_led = log(i_led)                     // LED current in log space
w_led ≈ log(1e-30) + v/0.026          // LINEAR in v!
i_cap = C * dv/dt                      // Capacitor stays linear

// Modified KCL:
exp(w_led) + i_cap = I_source
```

But we solve it smartly:
```
During Newton iteration:
Jacobian for w_led equation:
∂w_led/∂v = 1/0.026                    // Constant! No exponential!

Jacobian for KCL (after scaling):
∂(scaled_KCL)/∂v = exp(w_led - w_max)/0.026 + C/dt
                   ^^^^^^^^^^^^^^^^^ This stays near 1.0
```

The key insight: By keeping currents in log space and using smart scaling, we NEVER compute large exponentials during the Newton iteration!

### 2.3 Adaptive Timestep with Logarithmic Scaling

```rust
pub struct LogarithmicTimestepController {
    dt_min: f64,
    dt_max: f64,
    sharpness_threshold: f64,
    
    pub fn compute_timestep(&self, gradient: &TemporalGradient) -> f64 {
        // Logarithmic scaling based on sharpness
        if gradient.sharpness > 100.0 {
            // Ultra-sharp: minimum timestep
            self.dt_min
        } else if gradient.sharpness > 10.0 {
            // Sharp: logarithmic interpolation
            let log_factor = (gradient.sharpness.log10() - 1.0) / 2.0;
            self.dt_min * (10.0_f64).powf(1.0 - log_factor)
        } else if gradient.sharpness > 1.0 {
            // Moderate: linear interpolation in log space
            let factor = (gradient.sharpness - 1.0) / 9.0;
            self.dt_min * (self.dt_max / self.dt_min).powf(1.0 - factor)
        } else {
            // Smooth: maximum timestep
            self.dt_max
        }
    }
    
    pub fn validate_timestep(&self, dt: f64, error: f64) -> f64 {
        // Error-based adjustment
        if error > 1e-3 {
            dt * 0.5  // Reduce if error too large
        } else if error < 1e-6 {
            dt * 1.5  // Increase if error very small
        } else {
            dt
        }
    }
}
```

### 2.4 Enhanced PID Control for Transient

Extend the adaptive PID controller for time-domain:

```rust
pub struct TransientPIDController {
    // Spatial PID (from DC)
    spatial_gains: PIDGains,
    
    // Temporal PID
    temporal_gains: PIDGains,
    
    // Combined control
    pub fn compute_damping(&self, spatial_error: f64, temporal_error: f64) -> f64 {
        // Spatial damping (existing)
        let spatial_damping = self.spatial_pid(spatial_error);
        
        // Temporal damping
        let temporal_damping = self.temporal_pid(temporal_error);
        
        // Combined with priority on stability
        spatial_damping.min(temporal_damping)
    }
    
    fn temporal_pid(&self, error: f64) -> f64 {
        // Aggressive damping for time-domain stability
        if error > 1e-2 {
            0.1  // 10% for large errors
        } else if error > 1e-4 {
            0.3  // 30% for moderate
        } else {
            0.7  // 70% for small
        }
    }
}
```

### 2.5 Integration Method Selection

GLACIER-specific integration methods optimized for logarithmic transformation:

```rust
pub enum GlacierIntegration {
    LogarithmicBackwardEuler,    // Most stable
    LogarithmicTrapezoidal,      // Balanced
    AdaptiveLogBDF,              // Variable order
}

impl GlacierIntegration {
    pub fn select_method(&self, sharpness: f64, stiffness: f64) -> Self {
        if sharpness > 50.0 || stiffness > 1000.0 {
            // Ultra-stiff or sharp: maximum stability
            GlacierIntegration::LogarithmicBackwardEuler
        } else if stiffness > 100.0 {
            // Moderately stiff: adaptive order
            GlacierIntegration::AdaptiveLogBDF
        } else {
            // Normal: good accuracy
            GlacierIntegration::LogarithmicTrapezoidal
        }
    }
}
```

## 3. Companion Model Transformations

### 3.1 Logarithmic Capacitor Companion

```rust
pub struct LogarithmicCapacitorCompanion {
    capacitance: f64,
    
    pub fn build_companion(&self, v_prev: f64, i_prev: f64, dt: f64, 
                          method: &GlacierIntegration) -> (f64, f64) {
        match method {
            GlacierIntegration::LogarithmicBackwardEuler => {
                // i = C * (v - v_prev) / dt
                // In log space: handle near-zero carefully
                let g_eq = self.capacitance / dt;
                let i_eq = -g_eq * v_prev;
                
                // Transform for logarithmic variables
                if v_prev.abs() > 1e-20 {
                    let g_eq_log = g_eq * v_prev.abs();
                    (g_eq_log, i_eq)
                } else {
                    (g_eq, i_eq)
                }
            },
            GlacierIntegration::LogarithmicTrapezoidal => {
                // Trapezoidal with log transformation
                let g_eq = 2.0 * self.capacitance / dt;
                let i_eq = -g_eq * v_prev - 2.0 * i_prev;
                
                // Enhanced stability near zero
                self.stabilize_companion(g_eq, i_eq, v_prev)
            },
            _ => unimplemented!()
        }
    }
}
```

### 3.2 Logarithmic Inductor Companion

```rust
pub struct LogarithmicInductorCompanion {
    inductance: f64,
    
    pub fn build_companion(&self, v_prev: f64, i_prev: f64, dt: f64,
                          method: &GlacierIntegration) -> (f64, f64) {
        // v = L * di/dt
        // Rearranged: i = (1/L) * integral(v)
        
        match method {
            GlacierIntegration::LogarithmicBackwardEuler => {
                let r_eq = self.inductance / dt;
                let v_eq = r_eq * i_prev;
                
                // Transform for current in log space
                if i_prev.abs() > 1e-20 {
                    let r_eq_log = r_eq / i_prev.abs();
                    (1.0 / r_eq_log, v_eq)
                } else {
                    (1.0 / r_eq, v_eq)
                }
            },
            _ => unimplemented!()
        }
    }
}
```

## 4. Transient-Specific Numerical Enhancements

### 4.1 Jacobian Augmentation for Time Derivatives

```rust
impl TransientJacobian {
    pub fn augment_for_time(&mut self, dt: f64, method: &GlacierIntegration) {
        // Add time derivative contributions
        for (i, var) in self.variables.iter().enumerate() {
            if var.is_logarithmic {
                // Special handling for log variables
                let derivative_term = self.compute_log_time_derivative(var, dt);
                self.matrix[(i, i)] += derivative_term;
            } else {
                // Standard time derivative
                let derivative_term = self.compute_time_derivative(var, dt, method);
                self.matrix[(i, i)] += derivative_term;
            }
        }
    }
}
```

### 4.2 Convergence Criteria for Transient

```rust
pub struct TransientConvergenceCriteria {
    voltage_tol: f64,
    current_tol: f64,
    charge_tol: f64,
    
    pub fn check_convergence(&self, x_new: &[f64], x_old: &[f64], 
                            charges: &[f64]) -> bool {
        // Voltage convergence
        let v_converged = self.check_voltage_convergence(x_new, x_old);
        
        // Current convergence (through companion models)
        let i_converged = self.check_current_convergence(x_new, x_old);
        
        // Charge conservation
        let q_converged = self.check_charge_conservation(charges);
        
        v_converged && i_converged && q_converged
    }
}
```

## 5. GPU Acceleration for GLACIER Transient

### 5.1 Parallel Gradient Computation

```cuda
__global__ void glacier_temporal_gradient_kernel(
    double* states,      // Current state vector
    double* history,     // State history buffer
    double* gradients,   // Output gradients
    int num_variables,
    int history_depth,
    double dt
) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx < num_variables) {
        // Compute spatial gradient
        double spatial_grad = compute_spatial_gradient_gpu(states, idx);
        
        // Compute temporal gradient from history
        double temporal_grad = 0.0;
        if (history_depth >= 2) {
            double v_curr = states[idx];
            double v_prev = history[(history_depth-1) * num_variables + idx];
            double v_prev2 = history[(history_depth-2) * num_variables + idx];
            
            // Second-order approximation
            temporal_grad = (1.5*v_curr - 2.0*v_prev + 0.5*v_prev2) / dt;
        }
        
        // Combined gradient with log scaling
        gradients[idx] = log_scale_gradient(spatial_grad, temporal_grad);
    }
}
```

### 5.2 Batched Timestep Computation

```cuda
__global__ void glacier_timestep_kernel(
    double* gradients,
    double* timesteps,
    int num_points,
    double dt_min,
    double dt_max
) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx < num_points) {
        double sharpness = gradients[idx];
        
        // Logarithmic timestep scaling
        if (sharpness > 100.0) {
            timesteps[idx] = dt_min;
        } else if (sharpness > 10.0) {
            double log_factor = (log10(sharpness) - 1.0) / 2.0;
            timesteps[idx] = dt_min * pow(10.0, 1.0 - log_factor);
        } else {
            timesteps[idx] = dt_max;
        }
    }
}
```

## 6. Test Cases for GLACIER Transient

### 6.1 Exponential Transient Response
```spice
* Test exponential handling in time domain
V1 IN 0 PULSE(0 5 0 1n 1n 10u 20u)
D1 IN OUT EXTREME_LED
R1 OUT 0 1k

.model EXTREME_LED D (IS=1e-38 N=2.0 RS=10)
.tran 0.01u 50u

* Expected: GLACIER handles startup without overflow
```

### 6.2 Sharp Switching Transient
```spice
* Ultra-fast switching to test gradient detection
V1 CTRL 0 PULSE(0 5 0 100p 100p 1u 2u)
* [MOSFET switch with LED load]

* Tests Phase 0 temporal gradient prediction
```

## 7. Success Metrics for GLACIER Transient

1. **Convergence Rate**: >95% on LED transient benchmarks
2. **Numerical Stability**: No overflow for Is < 1e-35
3. **Timestep Efficiency**: 10x fewer steps than fixed timestep
4. **Accuracy**: <0.1% error vs. reference solution
5. **GPU Scaling**: Linear speedup to 10,000 variables

---

*This specification focuses solely on GLACIER numerical innovations for transient analysis*