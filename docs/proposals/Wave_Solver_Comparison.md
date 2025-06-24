# Wave Solver Approach Comparison

## Current Empirical Approach vs. Proposed Generic Solution

### Current: Empirical Wave Decay (Series Only)

```
   [Source] -----> [R] -----> [L] -----> [C] -----> [GND]
      |
      v
   Apply wave factor: V_eff = V * (1 + 0.1 * e^(-3t/τ))
```

**How it works:**
- Wave effect applied at source
- Propagates uniformly through series chain
- All components see same wave modification
- ✅ Perfect for series circuits
- ❌ Fails at parallel junctions

### Problem: Parallel Junction

```
                    ┌─> [R2] ─┐
   [Source] -> [R1] ┤         ├─> [GND]
                    └─> [L]  ─┘
```

**Why it fails:**
- R2 sees Z = 100Ω
- L sees Z = jωL (frequency dependent)
- Current should split based on impedance
- But empirical approach applies same wave to both!

### Proposed: Wave Digital Network

```
   [Source]  ══════> [R1] ══════> Junction ══════> [R2]
    Port 1    Wave    Port 2      Adaptor   Wave    Port 3
                                     ║       Wave
                                     ║       Port 4
                                     v
                                    [L]
```

**How it works:**

1. **Each connection is a wave channel** (═══)
   - Forward wave (incident) →
   - Backward wave (reflected) ←
   - Port impedance Rp

2. **Elements scatter waves**
   ```
   Resistor: b1 = Γ * a1, b2 = τ * a1
   where Γ = (R - Rp)/(R + Rp)
   ```

3. **Junctions split/combine waves**
   ```
   Parallel: Current splits by admittance
   I_R2 = I_total * (1/R2) / (1/R2 + 1/Z_L)
   I_L = I_total * (1/Z_L) / (1/R2 + 1/Z_L)
   ```

4. **Empirical decay applied locally**
   ```
   Each element has local decay
   Not global from source
   ```

## Visual Example: RC Circuit Comparison

### Empirical (Current)
```
Time=0:    [5V] -> [1kΩ] -> [1µF] -> [GND]
           V=5V    V=5V      V=0V

Time=1µs:  [5V] -> [1kΩ] -> [1µF] -> [GND]
           V=5.5V* V=2.75V   V=2.75V
           (*wave factor applied)
```

### Wave Digital (Proposed)
```
Time=0:    [5V] ═══> [1kΩ] ═══> [1µF] ═══> [GND]
           a=2.5V    a=2.5V     a=0V
           b=0V      b=0V       b=2.5V

Time=1µs:  [5V] ═══> [1kΩ] ═══> [1µF] ═══> [GND]
           a=2.5V    a=1.25V    a=0.6V
           b=1.25V   b=0.6V     b=0.6V
           V=3.75V   V=1.85V    V=1.2V
```

## Key Differences

| Aspect | Empirical | Wave Digital |
|--------|-----------|--------------|
| **Scope** | Series only | Any topology |
| **Wave Model** | Global factor | Local scattering |
| **Junctions** | Not supported | Proper splitting |
| **Impedance** | Fixed | Adaptive |
| **Parallelization** | Limited | Full |
| **Stability** | Good for series | Guaranteed |

## Implementation Complexity

### Empirical (50 lines)
```rust
let decay = exp(-3t/τ);
let factor = 1 + 0.1 * decay;
v_effective = v_source * factor;
// Apply circuit equations
```

### Wave Digital (500+ lines)
```rust
// For each element
let gamma = (Z - Rp) / (Z + Rp);
reflected = gamma * incident;

// For each junction
v_junction = sum(2 * a_i / R_i) / sum(1/R_i);
b_i = v_junction - a_i;

// Iterate to convergence
```

## When to Use Each

### Use Empirical When:
- Circuit is purely series
- Need quick results
- Simplicity is priority

### Use Wave Digital When:
- General topology required
- Parallel/mesh circuits
- Need guaranteed stability
- Full parallelization needed

## Migration Path

1. **Keep empirical for series detection**
   ```rust
   if is_series_only(circuit) {
       use_empirical_solver();  // Fast & accurate
   } else {
       use_wave_digital();      // General
   }
   ```

2. **Hybrid approach**
   - Detect series subcircuits
   - Use empirical within series sections
   - Use wave digital at junctions

3. **Gradual adoption**
   - Start with basic elements
   - Add junction types incrementally
   - Validate against SPICE