# Component Model Accuracy Review

This document identifies component models in bhdl-spice that need improvement for better physical accuracy.

## 1. LED Model - CRITICAL ISSUES ⚠️

### Current Problems:
- **Fixed forward voltage assumption**: The old model in `components.rs` uses a fixed forward voltage (e.g., 2.0V for red LED) which is physically incorrect
- **Linear approximation**: Uses `forward_voltage + current * dynamic_resistance` which is too simplistic
- **Missing exponential behavior**: LEDs follow the Shockley diode equation with exponential I-V characteristics

### What Needs Fixing:
- Remove all fixed forward voltage values
- Implement proper Shockley equation: `I = Is * (exp(V/nVt) - 1)`
- Use realistic saturation currents (Is ~ 1e-12 to 1e-15 A)
- Use proper emission coefficients (n ~ 1.5-2.0 for LEDs)
- Account for series resistance (bulk resistance)

### Good Example:
The `components_v2.rs` file shows the correct approach with `LEDModelV2` that uses only physics-based parameters.

## 2. Diode Model - PARTIAL ISSUES

### Current State:
- Has optional Shockley equation parameters (`saturation_current`, `emission_coefficient`)
- Falls back to simplified linear model if parameters not provided
- Reverse bias modeling is oversimplified

### What Needs Fixing:
- Make Shockley parameters mandatory, not optional
- Improve reverse bias modeling (include breakdown voltage)
- Add junction capacitance for AC analysis
- Temperature coefficient modeling

## 3. Voltage Regulator Model - OVERSIMPLIFIED

### Current Problems:
- The adaptive gain algorithm in `runtime_models.rs` is ad-hoc, not based on actual regulator physics
- Missing internal error amplifier modeling
- No proper feedback loop dynamics
- Oversimplified dropout behavior

### What Needs Fixing:
- Model the internal error amplifier with proper gain and bandwidth
- Implement realistic feedback network
- Add proper current limiting behavior
- Model thermal shutdown and protection features

## 4. Transistor Models (BJT & MOSFET) - GOOD BUT INCOMPLETE

### BJT Model Status:
- ✅ Proper Ebers-Moll equations implemented
- ✅ Temperature effects included
- ✅ Early effect modeled
- ❌ Missing: High-current effects (Kirk effect)
- ❌ Missing: Base push-out
- ❌ Missing: Quasi-saturation region

### MOSFET Model Status:
- ✅ Level 1 Shichman-Hodges model implemented
- ✅ Body effect included
- ✅ Channel length modulation
- ❌ Missing: Short channel effects
- ❌ Missing: Velocity saturation
- ❌ Missing: DIBL (Drain-Induced Barrier Lowering)

## 5. OpAmp Model - OVERSIMPLIFIED

### Current Problems:
- Simple gain-based model without internal stages
- No frequency response modeling (despite having GBW parameter)
- No slew rate limiting implementation
- Missing input/output stage modeling

### What Needs Fixing:
- Implement proper frequency response with poles/zeros
- Add slew rate limiting based on internal compensation capacitor
- Model input and output stages separately
- Include common-mode behavior

## 6. Passive Components - MOSTLY ADEQUATE

### Resistor:
- ✅ Linear model is sufficient for DC
- ❌ Could add temperature coefficient
- ❌ Could add parasitic inductance/capacitance for high frequency

### Capacitor:
- ✅ Open circuit for DC is correct
- ❌ Missing ESR (Equivalent Series Resistance) implementation
- ❌ Missing voltage coefficient for ceramic caps

### Inductor:
- ✅ Short circuit for DC is correct
- ❌ DCR (DC Resistance) not properly used in model
- ❌ Missing saturation current effects

## Priority Recommendations:

1. **HIGHEST PRIORITY**: Fix LED model to use proper exponential physics
   - This is causing convergence issues in many circuits
   - The fixed voltage assumption leads to wrong operating points

2. **HIGH PRIORITY**: Improve voltage regulator model
   - Critical for power supply simulations
   - Current ad-hoc approach doesn't reflect real behavior

3. **MEDIUM PRIORITY**: Enhance diode model
   - Make Shockley parameters mandatory
   - Add breakdown voltage modeling

4. **LOWER PRIORITY**: Improve passive component models
   - Add temperature coefficients
   - Include parasitic effects for completeness

## Implementation Strategy:

1. Migrate all models to use physics-based parameters only (like `LEDModelV2`)
2. Remove all hardcoded "typical" values
3. Extract parameters from component datasheets or SPICE models
4. Validate against known SPICE simulators (LTspice, ngspice)
5. Add comprehensive tests comparing model outputs to expected physics

## Testing Requirements:

For each improved model:
- Test I-V curves across full operating range
- Verify temperature effects
- Check convergence in typical circuits
- Compare results with reference SPICE simulators
- Validate against manufacturer datasheets