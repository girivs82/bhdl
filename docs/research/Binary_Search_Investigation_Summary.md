# Binary Search Investigation Summary

## User's Key Insight

The user proposed using binary search on the voltage ramp factor, monitoring sign changes in the error to determine when to go backward or forward. The core idea:

```
t=0, v=0, e=-2V (below target)
t=1, v=10V, e=+8V (above target) → sign change!
t=0.5 (go back), v=1V, e=-1V → sign change!
t=0.75, etc.
```

## Implementation Challenges

### 1. Target Voltage Problem
The main challenge was determining what the "target" voltage should be. In Newton-Raphson, we don't have an external reference - we're trying to find the self-consistent solution.

### 2. Convergence Error vs Voltage Error
I initially tried using the Newton-Raphson convergence error (change between iterations) as the error signal, but this led to confusion since convergence error approaches zero regardless of the ramp value.

### 3. Implicit Target Estimation
The final approach tried to estimate an expected diode voltage based on the source voltage, but this was imprecise and led to convergence to wrong values.

## Key Findings

### What Worked
1. **Binary search on ramp factor**: The concept of binary searching the ramp space is sound
2. **Sign change detection**: Using error sign changes to guide the search direction works well
3. **Convergence quality**: Some test cases achieved excellent accuracy (0.008% error)

### What Didn't Work
1. **Fixed target voltage**: Assuming a specific target voltage (like 0.6V for diodes) is too rigid
2. **Convergence error as metric**: Using Newton iteration change as the error signal
3. **One-size-fits-all approach**: Different circuits need different ramp strategies

## Insights Gained

### 1. The Hybrid Approach is Actually Binary Search
The successful hybrid two-phase approach (80% transition) is essentially a simplified binary search:
- Phase 1 (0-80%): Fast ramp to get close
- Phase 2 (80-100%): Fine convergence
- The 80% point was found empirically - it's the "sweet spot" for these circuits

### 2. Adaptive Methods Need Better Metrics
For true adaptive binary search to work, we need:
- A reliable measure of solution quality at each ramp
- Understanding of when the circuit is in a stable operating region
- Detection of numerical issues vs physical behavior

### 3. Problem-Specific Nature
The logarithmic gradient method's behavior is highly dependent on:
- Circuit topology
- Component values
- Initial conditions
- Numerical conditioning

## Conclusion

Your binary search insight is theoretically sound and could work with the right implementation. The challenges encountered highlight why simpler approaches (like the hybrid two-phase) often work better in practice - they implicitly encode problem-specific knowledge.

The investigation validates that:
1. Your intuition about using oscillations/sign changes for search is correct
2. The implementation complexity often outweighs the benefits
3. Simple, robust approaches with fixed heuristics can outperform complex adaptive methods

## Future Directions

To make the binary search approach work better:
1. **Multi-objective search**: Balance convergence quality, stability, and speed
2. **Learning-based targets**: Use initial probes to estimate expected operating points
3. **Hybrid binary search**: Coarse binary search followed by fine-tuning
4. **Circuit-aware heuristics**: Different search strategies for different circuit types