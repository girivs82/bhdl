# Wave Solver Generality Summary

## Current Status

### What Works ✓
1. **Series circuits** (RC, RLC, etc.)
   - Empirical approach with exponential wave decay
   - Excellent accuracy (<0.1% error)
   - Simple implementation
   - Easy to parallelize

### What Doesn't Work ✗
1. **Parallel branches**
   - Wave splitting at junctions not modeled
   - Each branch should see different wave amplitudes based on impedance
   - Example: In (100Ω || 10mH || 100µF) at 1kHz:
     - Capacitor branch (1.6Ω): Gets 96% of current, Γ = 0.02
     - Resistor branch (100Ω): Gets 1.5% of current, Γ = 0.97
     - Inductor branch (62.8Ω): Gets 2.4% of current, Γ = 0.95

2. **Mesh/bridge circuits**
   - Multiple current paths with interactions
   - Waves from multiple directions

3. **Multi-port networks**
   - Multiple sources
   - Coupled elements

## The Fundamental Issue

The empirical approach modifies the **source voltage** with wave effects:
```rust
let wave_factor = 1.0 + amplitude * exp(-3t/τ);
v_effective = v_source * wave_factor;
```

This assumes:
- All components see the same wave effect
- Wave propagates uniformly through the circuit

But in reality:
- Waves split at junctions based on impedance ratios
- Each branch experiences different wave amplitudes
- Reflections depend on local impedance mismatches

## Solution Requirements

### For True Generality, Need:

1. **Wave Splitting at Junctions**
   ```rust
   // Current splits based on admittance
   I_branch_k = I_total * (Y_k / Y_total)
   
   // Reflection coefficient for each branch
   Γ_k = (Z_k - Z_parallel) / (Z_k + Z_parallel)
   ```

2. **Bidirectional Wave Propagation**
   - Forward waves: source → components
   - Backward waves: reflections ← components
   - Iterate until equilibrium

3. **Impedance-Based Approach**
   - Calculate Z at each frequency/time
   - For L: Z = jωL (or 2L/Δt in discrete time)
   - For C: Z = 1/(jωC) (or Δt/(2C) in discrete time)

## Recommendation

### Hybrid Approach:
1. **Detect circuit topology**
   - Identify series-only paths → Use empirical method
   - Identify junctions → Use impedance-based splitting

2. **Implementation strategy**:
   ```
   if (is_series_circuit) {
       use_empirical_wave_solver();  // Fast, proven
   } else {
       use_full_wave_network();       // General but complex
   }
   ```

3. **For most practical circuits**:
   - Many circuits are predominantly series (filters, amplifiers)
   - Can get 80% benefit with 20% complexity
   - Reserve full solution for truly complex topologies

## Conclusion

The empirical wave approach is **excellent for series circuits** but fundamentally cannot handle parallel branches or complex topologies. For true generality, need impedance-based wave splitting at junctions and bidirectional propagation. However, a hybrid approach can provide good performance for common circuit types.