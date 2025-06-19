# SPICE Models Implementation Documentation

## Overview

This document describes the sophisticated SPICE models implementation in BHDL, completed as part of the DC analysis and component modeling improvements.

## Architecture

### 1. Unified Component and SPICE Model Approach

Instead of maintaining separate BHDL component definitions and SPICE models, we integrated SPICE parameters directly into the component definitions. This provides:

- **Single source of truth**: Each component carries its complete electrical model
- **No synchronization issues**: Component behavior and SPICE model always match
- **Easier maintenance**: Update in one place affects both synthesis and analysis

Example from `bhdl-stdlib/passives/resistor.bhdl`:
```bhdl
// SPICE model parameters
attribute spice_model = "resistor";
attribute spice_resistance = params.resistance;
attribute spice_max_power = params.power_rating;
attribute spice_temp_coeff1 = params.temp_coefficient;  // ppm/°C
attribute spice_temp_coeff2 = 0;  // ppm/°C²
attribute spice_noise_coeff = 1.0;  // Standard thermal noise
attribute spice_tnom = 27;  // Nominal temperature °C
```

### 2. SPICE Model Implementation

Located in `bhdl-spice/src/models/`, the implementation includes:

#### Core Trait
```rust
pub trait SpiceModel: Send + Sync {
    fn name(&self) -> &str;
    fn model_type(&self) -> ModelType;
    fn current(&self, voltages: &[f64], temp: f64) -> f64;
    fn conductance(&self, voltages: &[f64], temp: f64) -> Vec<f64>;
    fn num_terminals(&self) -> usize;
    fn is_nonlinear(&self) -> bool;
    fn parameters(&self) -> HashMap<String, f64>;
    fn set_parameter(&mut self, name: &str, value: f64) -> Result<(), String>;
}
```

#### Implemented Models

1. **Diode Model** (`diode.rs`)
   - Full Shockley equation: I = Is * (exp(V/(n*Vt)) - 1)
   - Temperature-dependent saturation current
   - Breakdown voltage modeling
   - Junction capacitance (for future AC analysis)
   - Presets: 1N4148, 1N4007, LEDs, Schottky

2. **BJT Model** (`bjt.rs`)
   - Ebers-Moll equations for NPN/PNP
   - Forward and reverse current gains (βF, βR)
   - Early effect (output conductance)
   - Junction capacitances
   - Temperature effects
   - Presets: 2N2222, 2N2907, 2N3904, 2N3906

3. **MOSFET Model** (`mosfet.rs`)
   - Level 1 Shichman-Hodges model
   - Three regions: cutoff, linear, saturation
   - Body effect (threshold voltage modulation)
   - Channel length modulation (λ)
   - Temperature-dependent threshold voltage
   - Presets: IRF540, 2N7000, BS250

4. **Resistor Model** (`resistor.rs`)
   - Temperature coefficients (TC1, TC2)
   - Thermal noise modeling
   - Power and voltage ratings
   - Package-based parameters
   - Types: metal film, carbon film, wire wound, SMD

5. **Capacitor Model** (`capacitor.rs`)
   - ESR (Equivalent Series Resistance)
   - ESL (Equivalent Series Inductance)
   - Voltage coefficients (VC1, VC2)
   - Temperature coefficient
   - Dielectric absorption
   - Types: ceramic (X7R, C0G), electrolytic, tantalum, film

6. **Inductor Model** (`inductor.rs`)
   - Saturation modeling: L(I) = L0 / (1 + (I/Isat)²)
   - DC resistance
   - Core losses (frequency-dependent)
   - Temperature effects
   - Types: ferrite, air core, iron powder

7. **Op-Amp Model** (`opamp.rs`)
   - Simplified macro model
   - Open-loop gain and bandwidth
   - Input/output impedances
   - Supply rails and output saturation
   - Presets: LM741, TL072, LM358, OP07

### 3. Model Factory

The `SpiceModelFactory` (`model_factory.rs`) bridges BHDL components to SPICE models:

```rust
pub struct SpiceModelFactory {
    model_library: HashMap<String, String>,
}

impl SpiceModelFactory {
    // Create model from BHDL component attributes
    pub fn create_from_attributes(
        &self,
        name: &str,
        attributes: &HashMap<String, String>,
    ) -> Option<Box<dyn SpiceModel>>;
    
    // Create model from component type and parameters
    pub fn create_from_bhdl(
        &self,
        name: &str,
        bhdl_type: &str,
        parameters: &HashMap<String, f64>,
    ) -> Option<Box<dyn SpiceModel>>;
}
```

Features:
- Parses values with units (e.g., "2.2e-6", "100pF")
- Maps BHDL types to appropriate SPICE models
- Handles both generic and specific part numbers
- Extensible model library

### 4. BHDL Component Integration

Updated stdlib components to include SPICE parameters:

#### LED Example (`led.bhdl`)
```bhdl
// SPICE model parameters (LED as special diode)
attribute spice_model = "diode";
attribute spice_type = "led";
attribute spice_is = params.forward_current / exp(params.forward_voltage / (2.0 * 0.026));
attribute spice_n = 2.0;  // Emission coefficient for LEDs
attribute spice_rs = params.dynamic_resistance;
attribute spice_cjo = 10e-12;  // Junction capacitance
attribute spice_vj = params.forward_voltage;
attribute spice_tt = params.impedance.transient_response;
attribute spice_bv = params.max_reverse_voltage;
attribute spice_ibv = 10e-6;  // Breakdown current
attribute spice_tnom = 27;  // Nominal temperature
```

#### Diode Example (`diode.bhdl`)
Now includes complete SPICE parameters for various part numbers:
- 1N4148: Fast switching signal diode
- 1N4007: 1A rectifier (with 1N4001-1N4006 variants)
- 1N5819: Schottky rectifier
- BAT54: Small signal Schottky

### 5. Temperature Modeling

All models support temperature-dependent behavior:

```rust
// Thermal voltage calculation
pub fn thermal_voltage(temp_celsius: f64) -> f64 {
    let temp_kelvin = temp_celsius + 273.15;
    BOLTZMANN * temp_kelvin / ELEMENTARY_CHARGE
}

// Example: Diode saturation current temperature adjustment
fn is_temp(&self, temp: f64) -> f64 {
    let temp_k = temp + 273.15;
    let tnom_k = self.params.tnom + 273.15;
    let vt = thermal_voltage(temp);
    let vt_nom = thermal_voltage(self.params.tnom);
    
    let temp_ratio = temp_k / tnom_k;
    let vt_factor = (self.params.eg / self.params.n) * (1.0 / vt_nom - 1.0 / vt);
    self.params.is * temp_ratio.powf(self.params.xti / self.params.n) * vt_factor.exp()
}
```

### 6. Numerical Stability

Implemented safeguards against numerical issues:

```rust
// Exponential clamping to prevent overflow
pub fn clamp_exp(x: f64, max: f64) -> f64 {
    x.min(max).max(-max)
}

// Usage in diode model
let exp_arg = clamp_exp(vd_junction / (self.params.n * vt), 40.0);
is_t * (exp_arg.exp() - 1.0)
```

## Usage Examples

### 1. Creating Models from BHDL Attributes

```rust
let mut attrs = HashMap::new();
attrs.insert("spice_model".to_string(), "diode".to_string());
attrs.insert("spice_is".to_string(), "2.682e-9".to_string());
attrs.insert("spice_n".to_string(), "1.836".to_string());
// ... more parameters

let model = factory.create_from_attributes("D1", &attrs);
```

### 2. Nonlinear Analysis Integration

The models integrate with the Newton-Raphson solver:

```rust
// In nonlinear_analysis.rs
match model {
    ComponentModel::Diode { .. } => {
        let diode_model = // create from parameters
        let i = diode_model.current(&[v1, v2], temp);
        let di_dv = diode_model.conductance(&[v1, v2], temp)[0];
        // Stamp into Jacobian matrix
    }
    // ... other models
}
```

### 3. Test Coverage

Comprehensive tests in `test_integrated_spice_models.rs` verify:
- Model creation from attributes
- Temperature effects
- I-V characteristics
- Value parsing with units
- Numerical stability

## Benefits

1. **Accuracy**: Full nonlinear models instead of piecewise linear approximations
2. **Temperature Analysis**: Can simulate circuits at different operating temperatures
3. **Standards Compliance**: Uses industry-standard SPICE parameters
4. **Extensibility**: Easy to add new models or parameters
5. **Integration**: Seamless integration with BHDL component system
6. **Performance**: Efficient computation with numerical safeguards

## Future Enhancements

1. **AC Analysis**: Utilize capacitance parameters for frequency response
2. **Noise Analysis**: Implement thermal and flicker noise calculations
3. **Model Levels**: Support multiple SPICE model levels (e.g., MOSFET Level 2, 3)
4. **Subcircuits**: Support for complex component models using subcircuits
5. **Parameter Extraction**: Tools to extract SPICE parameters from datasheets

## Component Models Reference

### Resistor SPICE Attributes
- `spice_model`: "resistor"
- `spice_resistance`: Resistance value (Ω)
- `spice_temp_coeff1`: Linear temperature coefficient (ppm/°C)
- `spice_temp_coeff2`: Quadratic temperature coefficient (ppm/°C²)
- `spice_max_power`: Power rating (W)
- `spice_noise_coeff`: Noise coefficient (typically 1.0)
- `spice_tnom`: Nominal temperature (°C)

### Diode/LED SPICE Attributes
- `spice_model`: "diode"
- `spice_type`: "diode" or "led"
- `spice_is`: Saturation current (A)
- `spice_n`: Emission coefficient
- `spice_rs`: Series resistance (Ω)
- `spice_cjo`: Zero-bias junction capacitance (F)
- `spice_vj`: Junction potential (V)
- `spice_tt`: Transit time (s)
- `spice_bv`: Breakdown voltage (V)
- `spice_ibv`: Breakdown current (A)
- `spice_tnom`: Nominal temperature (°C)

### Capacitor SPICE Attributes
- `spice_model`: "capacitor"
- `spice_capacitance`: Capacitance value (F)
- `spice_esr`: Equivalent series resistance (Ω)
- `spice_esl`: Equivalent series inductance (H)
- `spice_vc1`: Linear voltage coefficient (ppm/V)
- `spice_vc2`: Quadratic voltage coefficient (ppm/V²)
- `spice_tc1`: Temperature coefficient (ppm/°C)
- `spice_voltage_rating`: Maximum voltage (V)

### Inductor SPICE Attributes
- `spice_model`: "inductor"
- `spice_inductance`: Inductance value (H)
- `spice_dcr`: DC resistance (Ω)
- `spice_isat`: Saturation current (A)
- `spice_rcore`: Core loss resistance (Ω)
- `spice_cpar`: Parasitic capacitance (F)
- `spice_tc1`: Temperature coefficient (ppm/°C)
- `spice_current_rating`: Maximum current (A)