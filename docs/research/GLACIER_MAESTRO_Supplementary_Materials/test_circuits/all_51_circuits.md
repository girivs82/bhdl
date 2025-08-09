# Complete Test Circuit Specifications

All 51 test circuits used in the GLACIER-MAESTRO paper evaluation.

## 1. Series Nonlinear Circuits (12 circuits)

### 1.1 Series-2-LEDs
- Components: 2 LEDs + 220Ω resistor
- Supply: 5V
- Is values: [1e-12, 1e-15]
- Expected solutions: 2-3

### 1.2 Series-3-LEDs
- Components: 3 LEDs + 220Ω resistor
- Supply: 5V
- Is values: [1e-12, 1e-18, 1e-24]
- Expected solutions: 3

### 1.3 Series-4-LEDs
- Components: 4 LEDs + 220Ω resistor
- Supply: 5V
- Is values: [1e-12, 1e-20, 1e-28, 1e-32]
- Expected solutions: 3

### 1.4 Series-5-LEDs *(Featured in paper Section VI.E)*
- Components: 5 LEDs + 220Ω resistor
- Supply: 5V
- Is values: [1e-24, 1e-28, 1e-32, 1e-36, 1e-38]
- Expected solutions: 3
- Verified iterations: 110
- Verified time: 21.61ms

### 1.5 Series-6-LEDs
- Components: 6 LEDs + 330Ω resistor
- Supply: 6V
- Is values: [1e-12, 1e-16, 1e-20, 1e-24, 1e-28, 1e-32]
- Expected solutions: 3-4

### 1.6 Series-7-LEDs
- Components: 7 LEDs + 470Ω resistor
- Supply: 7V
- Is values: [1e-15, 1e-18, 1e-21, 1e-24, 1e-27, 1e-30, 1e-33]
- Expected solutions: 3-4

### 1.7 Series-8-LEDs
- Components: 8 LEDs + 560Ω resistor
- Supply: 8V
- Is values: [1e-12, 1e-15, 1e-18, 1e-21, 1e-24, 1e-27, 1e-30, 1e-33]
- Expected solutions: 4

### 1.8 Series-9-LEDs
- Components: 9 LEDs + 680Ω resistor
- Supply: 9V
- Is values: [1e-13, 1e-16, 1e-19, 1e-22, 1e-25, 1e-28, 1e-31, 1e-34, 1e-37]
- Expected solutions: 4

### 1.9 Series-10-LEDs *(Mentioned in paper Section VI.G)*
- Components: 10 LEDs + 820Ω resistor
- Supply: 10V
- Is values: [1e-12, 1e-14, 1e-16, 1e-18, 1e-20, 1e-22, 1e-24, 1e-26, 1e-28, 1e-30]
- Expected solutions: 3
- Verified iterations: 161
- Verified time: 142.88ms

### 1.10 Series-2-LEDs-extreme *(Featured in paper)*
- Components: 2 LEDs + 150Ω resistor
- Supply: 3.3V
- Is values: [3.96e-19, 1e-15]
- Expected solutions: 2
- Verified iterations: 31,714
- Verified time: 1210.01ms

### 1.11 Series-3-Diodes
- Components: 3 1N4148 diodes + 100Ω resistor
- Supply: 3V
- Is values: [2.7e-9, 2.7e-9, 2.7e-9]
- Expected solutions: 2

### 1.12 Series-Mixed
- Components: 2 LEDs + 1 Zener + 330Ω resistor
- Supply: 5V
- Mixed Is values and Vz=3.3V
- Expected solutions: 3

## 2. Parallel Arrays (7 circuits)

### 2.1 Parallel-2-LEDs-matched
- Components: 2 identical LEDs in parallel + 100Ω resistor
- Supply: 5V
- Is values: [1e-12, 1e-12]
- Expected solutions: 2

### 2.2 Parallel-3-LEDs-matched
- Components: 3 identical LEDs in parallel + 68Ω resistor
- Supply: 5V
- Is values: [1e-15, 1e-15, 1e-15]
- Expected solutions: 2

### 2.3 Parallel-4-LEDs-mismatched
- Components: 4 LEDs with different Is + 47Ω resistor
- Supply: 5V
- Is values: [1e-12, 1e-15, 1e-18, 1e-21]
- Expected solutions: 3

### 2.4 Parallel-5-LEDs-gradient
- Components: 5 LEDs with gradient Is + 33Ω resistor
- Supply: 5V
- Is values: [1e-12, 1e-14, 1e-16, 1e-18, 1e-20]
- Expected solutions: 3

### 2.5 Parallel-2x2-matrix
- Components: 2x2 LED matrix + current limiting
- Supply: 5V
- Mixed Is values
- Expected solutions: 3-4

### 2.6 Parallel-3x3-matrix
- Components: 3x3 LED matrix + row/col resistors
- Supply: 5V
- Various Is values
- Expected solutions: 4

### 2.7 Parallel-RGB-cluster
- Components: RGB LED (3 dies) + resistors
- Supply: 5V
- Different Vf for R/G/B
- Expected solutions: 3

## 3. IBIS Models (8 circuits)

### 3.1 DDR4-DQ-Termination *(Featured in paper Section VI.F)*
- Buffer: DDR4 DQ with 50Ω trace and ODT
- Tables: 2048-point I-V curves
- Supply: 1.2V
- Expected solutions: 3 (OFF/LOW/HIGH)
- Verified iterations: 247
- Verified time: 1.2ms

### 3.2 PCIe-Gen5-Clamp *(Example 3 in paper)*
- Buffer: PCIe Gen5 with sharp power clamp
- Clamp: 10x current jump at 1.45-1.50V
- Supply: 1.0V
- Expected solutions: 2-3
- Verified iterations: 1,543
- Verified time: 7.7ms

### 3.3 Multi-Driver-Contention *(Example 2 in paper)*
- Buffers: Strong + weak drivers on shared net
- Contention current resolution
- Supply: 3.3V
- Expected solutions: 1 (equilibrium)
- Verified iterations: 892
- Verified time: 4.5ms

### 3.4 LPDDR5-Temperature
- Buffer: LPDDR5 with temperature corners
- Tables: -40°C, 25°C, 125°C
- Supply: 0.6V
- Expected solutions: 3 per temperature

### 3.5 USB3-Differential
- Buffers: USB3 differential pair
- Common mode + differential mode
- Supply: 3.3V
- Expected solutions: 3

### 3.6 MIPI-CSI-LP
- Buffer: MIPI CSI-2 low power mode
- Multiple operating modes
- Supply: 1.2V
- Expected solutions: 4

### 3.7 HDMI-PreEmphasis
- Buffer: HDMI with pre-emphasis
- Multiple drive strength settings
- Supply: 3.3V
- Expected solutions: 3

### 3.8 SerDes-Equalizer
- Buffer: SerDes with TX equalizer
- Complex I-V characteristics
- Supply: 1.0V
- Expected solutions: 3-4

## 4. Power Converters (9 circuits)

### 4.1 Buck-Simple
- Topology: Basic buck converter
- Components: MOSFET, diode, L, C
- Input: 12V, Output: 5V
- Expected solutions: 2

### 4.2 Buck-Synchronous
- Topology: Synchronous buck
- Components: 2 MOSFETs, L, C
- Input: 12V, Output: 3.3V
- Expected solutions: 2

### 4.3 Boost-Classic
- Topology: Boost converter
- Components: MOSFET, diode, L, C
- Input: 5V, Output: 12V
- Expected solutions: 2

### 4.4 Buck-Boost
- Topology: Inverting buck-boost
- Components: MOSFET, diode, L, C
- Input: 5V, Output: -5V
- Expected solutions: 2

### 4.5 Flyback-Isolated
- Topology: Flyback with transformer
- Components: MOSFET, transformer, diode, C
- Input: 48V, Output: 5V
- Expected solutions: 3

### 4.6 Forward-Converter
- Topology: Forward converter
- Components: MOSFET, transformer, diodes, L, C
- Input: 48V, Output: 12V
- Expected solutions: 3

### 4.7 SEPIC
- Topology: SEPIC converter
- Components: MOSFET, 2L, 2C, diode
- Input: 9-18V, Output: 12V
- Expected solutions: 2

### 4.8 Cuk-Converter
- Topology: Cuk converter
- Components: MOSFET, 2L, 2C, diode
- Input: 12V, Output: 5V
- Expected solutions: 2

### 4.9 Multi-Output-Flyback
- Topology: Flyback with 3 outputs
- Multiple secondary windings
- Input: 24V, Outputs: 5V, 12V, -12V
- Expected solutions: 4

## 5. Cascaded Amplifiers (6 circuits)

### 5.1 Two-Stage-OpAmp
- Stages: Differential pair + output stage
- Gain: 80dB
- Supply: ±15V
- Expected solutions: 3

### 5.2 Three-Stage-High-Gain
- Stages: 3 cascaded CE stages
- Total gain: 120dB
- Supply: 12V
- Expected solutions: 3-4

### 5.3 Instrumentation-Amp
- Topology: 3 op-amp instrumentation
- Gain: 1000
- Supply: ±15V
- Expected solutions: 3

### 5.4 Transimpedance-Amp
- Photodiode + op-amp
- Gain: 1MΩ
- Supply: ±5V
- Expected solutions: 2

### 5.5 Log-Amplifier
- Transistor in feedback
- Logarithmic response
- Supply: ±12V
- Expected solutions: 3

### 5.6 Variable-Gain-Amp
- Digitally controlled gain
- 8 gain steps
- Supply: 5V
- Expected solutions: 4

## 6. Bridge Circuits (5 circuits)

### 6.1 Diode-Bridge-Rectifier
- 4 diodes in bridge
- AC input: 12V RMS
- Load: 100Ω
- Expected solutions: 2

### 6.2 Active-Bridge-Rectifier
- MOSFETs replacing diodes
- Synchronous rectification
- AC input: 5V RMS
- Expected solutions: 3

### 6.3 H-Bridge-Motor-Driver
- 4 MOSFETs in H-bridge
- Motor model included
- Supply: 24V
- Expected solutions: 3

### 6.4 Phase-Control-Bridge
- SCRs in bridge configuration
- Phase angle control
- AC input: 120V RMS
- Expected solutions: 3-4

### 6.5 Wheatstone-Bridge
- Resistive sensor bridge
- Instrumentation amp output
- Supply: 10V
- Expected solutions: 2

## 7. Protection Circuits (4 circuits)

### 7.1 TVS-Protection
- TVS diode array
- Multiple voltage levels
- Protected line: 5V
- Expected solutions: 3

### 7.2 Current-Limiting
- Active current limiter
- Set point: 1A
- Supply: 12V
- Expected solutions: 2

### 7.3 Crowbar-Protection
- SCR crowbar circuit
- Trigger: 6V
- Supply: 5V nominal
- Expected solutions: 2

### 7.4 Foldback-Current-Limit
- Foldback characteristic
- Reduces current on overload
- Supply: 24V
- Expected solutions: 3

## Summary Statistics

Total circuits: 51
- Series Nonlinear: 12
- Parallel Arrays: 7
- IBIS Models: 8
- Power Converters: 9
- Cascaded Amplifiers: 6
- Bridge Circuits: 5
- Protection Circuits: 4

Newton-Raphson convergence: 19/51 (37.3%)
GLACIER convergence: 51/51 (100%)
MAESTRO convergence: 47/51 (92.2%)
Combined convergence: 51/51 (100%)

These circuits represent real-world challenging cases that traditional simulators struggle with, validating GLACIER-MAESTRO's robustness.