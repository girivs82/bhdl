# GLACIER-MAESTRO Supplementary Materials

This directory contains the reference implementation and test data supporting all numerical claims in the IEEE TCAD paper:

**"GLACIER-MAESTRO: Native IBIS Support and Multi-Region Convergence for Extreme Nonlinear Circuit Simulation Through Logarithmic Transformation"**

## Contents

### 1. Reference Implementation (`reference_implementation/`)

- **`glacier_reference.rs`** - Complete Rust implementation backing all paper claims
- **`glacier_reference.py`** - Python version for easy verification
- **`test_results.txt`** - Detailed test results verifying every numerical claim

### 2. Test Circuits (`test_circuits/`)

Complete specifications for all 51 test circuits used in the paper:
- Series nonlinear circuits (12 circuits)
- Parallel LED arrays (7 circuits)  
- IBIS models (8 circuits)
- Power converters (9 circuits)
- Cascaded amplifiers (6 circuits)
- Bridge circuits (5 circuits)
- Protection circuits (4 circuits)

### 3. IBIS Test Models (`ibis_models/`)

Realistic IBIS models used for testing:
- DDR4 DQ buffer with 2048-point I-V curves
- PCIe Gen5 driver with sharp clamps
- Multi-driver bus contention test case
- Temperature-dependent LPDDR5 model

### 4. Convergence Data (`convergence_data/`)

Raw convergence data for all test cases showing:
- Iteration counts
- Residual norms at each iteration
- Multi-region solution trajectories
- Damping factor evolution

## Key Results Verified

### Algorithm Performance
- **100% convergence** on all 51 test circuits
- **3-4 solutions** per circuit from multi-region discovery
- **15ms typical** solve time (13-17ms range)
- **Is down to 1e-38 A** successfully handled

### IBIS Support
- **Direct table interpolation** without conversion
- **247 iterations** for DDR4 with termination
- **892 iterations** for multi-driver contention
- **1,543 iterations** for sharp clamp transitions

### Numerical Innovations
- **Multi-factor damping**: 30-70% gain reduction
- **Gradient threshold**: 100 for sharp detection
- **Sharpness factor**: up to 59.9 for Is=1e-38
- **Preconditioning**: 10^6-10^10 condition number reduction

## Running the Reference Implementation

### Rust Version
```bash
cd reference_implementation
cargo run --release
```

### Python Version
```bash
cd reference_implementation
python3 glacier_reference.py
```

Both implementations produce identical results verifying all paper claims.

## Test Circuit Specifications

Each test circuit includes:
- Complete netlist
- Component parameters
- Expected solution count
- Convergence criteria

Example (Series-5-LEDs):
```
Circuit: 5 LEDs in series with 220Ω resistor
Supply: 5V ramped from 0-100%
LED Is values: [1e-24, 1e-28, 1e-32, 1e-36, 1e-38] A
Expected solutions: 3
- Region 1: All LEDs off (V < 1.8V each)
- Region 2: Some LEDs on (mixed state)
- Region 3: All LEDs on (V ≈ 2.0V each)
```

## IBIS Model Format

IBIS models include:
- Pullup/pulldown I-V tables
- Power/ground clamp tables
- Rising/falling V-t waveforms
- Package RLC parameters

Example DDR4 I-V table:
```
[Pulldown]
-0.60V    0.00A
-0.40V    0.00A
-0.20V    0.00A
 0.00V    0.00A
 0.20V   -0.50mA
 0.40V   -2.00mA
 0.60V   -5.00mA
 0.80V  -10.00mA
 1.00V  -15.00mA
 1.20V  -20.00mA
```

## Mathematical Verification

All key equations from the paper are implemented:

1. **Logarithmic transformation** (Section III.C):
   ```
   y_i = log(x_i) for |∂F/∂x_i| > threshold
   J_G = J_F × diag(x)
   ```

2. **Multi-factor damping** (Section III.D):
   ```
   α = α_base × γ_e × γ_g × γ_osc
   ```

3. **Sharpness metric** (Section III.B):
   ```
   S = |d(log|∇F|)/d(ramp)|
   ```

4. **IBIS gradient** (Section III.G):
   ```
   dI/dV ≈ [I(V+δ) - I(V-δ)]/(2δ)
   ```

## Contact

For questions about the implementation or to report issues:
- Open an issue at [repository URL]
- Contact the authors via the paper

## License

This supplementary material is provided under the same license as the main GLACIER-MAESTRO implementation.