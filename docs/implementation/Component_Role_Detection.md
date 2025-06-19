# Component Role Detection Implementation

## Overview

The BHDL SPICE component role detection system analyzes electronic circuits to automatically identify the functional role of each component (e.g., input protection, filtering, load, etc.). This document describes the implementation and recent improvements.

## Key Components

### 1. Circuit Representation
- **Unified Structure**: Single `Branch` struct in `circuit.rs` represents components
- **No Duplication**: Removed separate Component struct to avoid confusion
- **Flexible Models**: Optional SPICE models can be attached when needed

### 2. Component Value Extraction
The system extracts actual component values from the BHDL analysis results:
- Resistances (Ω)
- Capacitances (F) 
- Inductances (H)
- Voltages (V)

Values are properly formatted for display (e.g., 100nF, 330Ω, 1.0V).

### 3. IC Detection
ICs are detected based on:
- Generic types: "VoltageRegulator", "OpAmp", "Comparator", etc.
- Specific part numbers: "LM7805", "LM317", "TPS54360", etc.
- Future: Component database classification

### 4. Role Classification

#### Protection Components
- **Fuse**: Input protection
- **TVSDiode**: Input protection  
- **PTC**: Input protection

#### Capacitors
Classification based on:
1. Special functions (bootstrap, soft-start, compensation)
2. Connection to IC pins (input filter, output stabilization)
3. Topology analysis (connected to protection devices → input filter)
4. Value-based heuristics (≥10µF output → stabilization, <10µF → decoupling)

#### Resistors
- Very low value (<1Ω): Current sense
- In feedback path: Feedback network
- Connected to enable pin: UVLO/enable divider
- With LED: Current limiting (Load)

#### Other Components
- **LED**: Load (especially with series resistor)
- **Inductor**: Power inductor, EMI filter, or compensation
- **Diode**: Catch diode, rectifier, or protection

### 5. Topology Analysis

The system analyzes circuit topology to determine relationships:
- `is_connected_to_protection_device()`: Checks if component shares nodes with fuses/TVS
- `is_connected_to_load()`: Checks connection to LEDs or low-value resistors
- `has_series_resistor()`: Validates LED has current limiting
- `is_connected_to_ic_input/output()`: Checks IC pin connections

## Example Analysis

For a 7805 linear regulator circuit:
```
Input Stage:
  - Fuse (1A) - Input protection
  - TVS Diode (15V) - Input protection
  
Power Stage:
  - LM7805 - Voltage regulator (IC)
  
Output Stage:  
  - Capacitors (100nF) - Decoupling
  - LED (green) - Load indicator
  - Resistor (330Ω) - Current limiting
```

## Future Enhancements

1. **Pin Metadata Integration**: Use component pin functions (already implemented) for more accurate detection
2. **Component Database**: Query real component data instead of heuristics
3. **Machine Learning**: Train on known circuits for pattern recognition
4. **Thermal Analysis**: Consider power dissipation in role assignment

## Testing

Run the component role detection test:
```bash
cargo run -p bhdl-synthesizer --bin test_buck_regulator_spice_roles
```

This analyzes a realistic voltage regulator circuit and displays:
- Component identification by functional role
- Grouped by circuit stage (input, power, control, output)
- Success rate percentage