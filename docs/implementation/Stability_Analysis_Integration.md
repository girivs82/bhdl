# Power Converter Stability Analysis - AC Integration

## Overview

The BHDL stability analysis system now integrates with actual AC frequency response analysis to provide real measurements of power converter stability. This enables accurate detection of stability issues including phase/gain margins, impedance interactions, and resonance cascades.

## Key Features

### 1. Loop Stability Analysis
- **AC-based measurements**: Uses actual frequency response from `SimulationEngine`
- **Phase margin calculation**: Interpolates phase at gain crossover frequency
- **Gain margin calculation**: Interpolates gain at phase crossover frequency
- **Nyquist stability**: Counts encirclements of critical point
- **Bandwidth measurement**: Finds -3dB frequency

### 2. Impedance Analysis
- **Input impedance**: Models negative resistance characteristic of switching converters
- **Output impedance**: Includes effect of control loop within bandwidth
- **Middlebrook criterion**: Verifies |Zsource/Zload| < 0.5 for cascade stability
- **Realistic modeling**: Includes parasitics (ESR, DCR) and control effects

### 3. Resonance Detection
- **Automatic peak detection**: Finds local maxima in impedance profiles
- **Q factor calculation**: Measures sharpness using -3dB bandwidth method
- **Damping assessment**: Classifies from critically damped to poorly damped
- **Multiple resonance types**: LC filter, parasitic, and control loop resonances

### 4. Cascade Analysis
- **Multi-converter systems**: Analyzes impedance interactions between stages
- **Beat frequency detection**: Identifies potential audible noise issues
- **Stability margin calculation**: Minimum margin across all interactions
- **Automated recommendations**: Suggests damping networks, filter adjustments

## Implementation Details

### Loop Gain Measurement
```rust
// AC analysis from compensation to feedback node
let ac_result = engine.run_ac_analysis(
    comp_node,
    fb_node,
    1.0,      // 1 Hz start
    10e6,     // 10 MHz stop
    20,       // 20 points per decade
)?;

// Extract loop gain and calculate margins
let loop_gains: Vec<Complex<f64>> = ac_result.transfer_function;
let phase_margin = 180.0 + phase_at_crossover;
```

### Impedance Modeling
```rust
// Output impedance includes control loop effect
let loop_gain = 1000.0 / (1.0 + (f / 10e3).powi(2));
let z_out = z_filter / (1.0 + loop_gain);

// Input impedance includes negative resistance
let r_neg = -10.0 / (1.0 + (f / 100.0).powi(2));
```

### Resonance Detection Algorithm
1. Convert impedance magnitude to dB scale
2. Find local maxima in frequency response
3. Check prominence (peak height above baseline)
4. Calculate Q factor from -3dB bandwidth
5. Classify damping level based on Q

## Usage Example

```rust
// Create and analyze a buck converter
let circuit = create_realistic_buck(...);
let mut analyzer = PowerConverterStabilityAnalyzer::new(circuit);

// Register converter nodes
analyzer.add_converter("Buck", ConverterNodes {
    input: vin_node,
    output: vout_node,
    feedback: Some(fb_node),
    compensation: Some(comp_node),
    ground: gnd_node,
});

// Run analysis
let result = analyzer.analyze_stability("Buck")?;

// Check results
println!("Phase Margin: {:.1}°", result.loop_stability.phase_margin_deg);
println!("Resonances: {:?}", result.resonances);
```

## Stability Criteria

### Loop Stability
- **Phase Margin**: > 45° (recommended), > 30° (minimum)
- **Gain Margin**: > 10 dB (recommended), > 6 dB (minimum)
- **Crossover Frequency**: < fsw/10 (1/10 of switching frequency)

### Impedance Requirements
- **Middlebrook Criterion**: |Zout_source/Zin_load| < 0.5
- **Resonance Q Factor**: < 2 for resonances within control bandwidth
- **Output Impedance**: Low enough for load transient requirements

### Cascade Stability
- **Impedance Ratio**: Each stage should satisfy Middlebrook criterion
- **Beat Frequencies**: Avoid audible range (20Hz - 20kHz)
- **Stability Margin**: > 6dB minimum for all interactions

## Current Limitations

1. **Loop breaking**: Currently assumes ideal loop breaking at compensation node
2. **Component models**: Uses simplified behavioral models for some components
3. **Large signal**: Only analyzes small-signal stability
4. **Temperature**: Does not include temperature effects

## Future Enhancements

1. **Automated loop breaking**: Intelligent selection of injection point
2. **Monte Carlo analysis**: Stability with component tolerances
3. **Time domain verification**: Large signal step response
4. **Thermal modeling**: Temperature-dependent stability