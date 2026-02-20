# Pin Metadata Integration with SPICE Component Role Detection

## Overview

This document describes the integration between BHDL's pin metadata annotations and the SPICE component role detection system. This enhancement enables more accurate circuit analysis by using explicit functional declarations rather than relying on naming conventions or topology analysis alone.

## Problem Statement

Previously, the SPICE component role detector relied primarily on:
- Circuit topology analysis (what connects to what)
- Component values (e.g., 10kΩ resistor likely feedback, 10Ω likely load)
- Node naming conventions (e.g., "SW" for switch node)
- Connection patterns (e.g., capacitor between power and ground = decoupling)

While effective, this approach had limitations:
- Ambiguous cases (is a capacitor for input filtering or decoupling?)
- Reliance on naming conventions
- Difficulty distinguishing specialized pins (bootstrap, compensation, soft-start)
- False positives/negatives in complex topologies

## Solution: Pin Metadata Integration

### 1. BHDL Pin Metadata Syntax

Modules can now declare explicit pin functions using `@metadata` annotations:

```bhdl
entity BuckController() {
    pin SW: power out @metadata(
        function="SwitchNode",
        max_voltage="42V",
        slew_rate="fast"
    );
    
    pin FB: signal in @metadata(
        function="Feedback",
        impedance="high"
    );
    
    pin COMP: signal out @metadata(
        function="Compensation"
    );
}
```

### 2. Metadata Flow Through Pipeline

The metadata flows through the BHDL pipeline as follows:

1. **Parser**: Recognizes `@metadata(...)` syntax and creates AST nodes
2. **Analyzer**: Extracts metadata into entity definitions
3. **Synthesizer**: Preserves metadata in netlist generation
4. **SPICE**: Uses metadata for component role detection

### 3. Implementation Details

#### New Module: `pin_metadata_integration.rs`

Provides utilities to extract and convert pin metadata:

```rust
pub struct ExtractedPinMetadata {
    pub module_pins: HashMap<(String, String), PinMetadata>,
    pub instance_types: HashMap<InstanceId, String>,
}

pub fn extract_pin_metadata_from_analysis(
    analysis: &AnalysisResult,
    netlist: &Netlist,
) -> ExtractedPinMetadata
```

#### Enhanced ComponentRoleDetector

New constructor with AST metadata support:

```rust
pub fn with_ast_metadata(
    circuit: Circuit, 
    netlist: &Netlist, 
    instance_to_component: HashMap<InstanceId, ComponentId>,
    analysis_result: &AnalysisResult,
) -> Self
```

Enhanced connection tracking includes pin functions:

```rust
// Old: (IC ComponentId, pin name, direction, type)
// New: (IC ComponentId, pin name, direction, type, Option<PinFunction>)
component_to_ic_pins: HashMap<ComponentId, Vec<(ComponentId, String, PinDirection, PinType, Option<PinFunction>)>>
```

### 4. Pin Function Types

The system recognizes these pin functions:

- `PowerIn` - Power input pin
- `PowerOut` - Power output pin  
- `SwitchNode` - High dV/dt switching node
- `Bootstrap` - Bootstrap capacitor connection
- `Feedback` - Feedback voltage sensing
- `Compensation` - Compensation network
- `SoftStart` - Soft-start capacitor
- `Enable` - Enable/shutdown control
- `CurrentSense` - Current sensing input
- `Ground` - Ground reference
- `Signal` - General signal pin

### 5. Role Detection Improvements

The role detector now:

1. **Checks pin metadata first** before falling back to topology analysis
2. **Identifies switch nodes** via `SwitchNode` function
3. **Detects specialized capacitors** (bootstrap, soft-start, compensation)
4. **Distinguishes feedback networks** using `Feedback` function
5. **Recognizes current sense resistors** via `CurrentSense` pins

Example improvement in `is_switch_node()`:

```rust
// Check if any connected IC pin has SwitchNode function
if let Some(func) = pin_function {
    if *func == PinFunction::SwitchNode {
        return true;
    }
}
// Fall back to topology analysis only if no metadata
```

## Benefits

1. **Accuracy**: Explicit declarations eliminate ambiguity
2. **Robustness**: Less reliance on naming conventions
3. **Completeness**: Can identify specialized component roles
4. **Extensibility**: Easy to add new pin functions
5. **Documentation**: Metadata serves as inline documentation

## Example Usage

```bhdl
entity BuckConverter() {
    pin SW: power out @metadata(function="SwitchNode");
    pin BST: power @metadata(function="Bootstrap");
    pin FB: signal in @metadata(function="Feedback");
    pin COMP: signal out @metadata(function="Compensation");
    pin SS: signal @metadata(function="SoftStart");
}

// Components connected to these pins will be accurately classified:
// - Capacitor on BST pin → Bootstrap role
// - Capacitor on SS pin → SoftStart role  
// - Resistors on FB pin → FeedbackNetwork role
// - Components on SW pin → Identified as switch node components
```

## Testing

A comprehensive test (`test_pin_metadata_roles.rs`) demonstrates:
- Component role detection with and without metadata
- Improvements in classification accuracy
- Proper handling of all pin function types

## Future Enhancements

1. **Database Integration**: Extract pin metadata from KiCad symbols
2. **Automatic Inference**: Use simulation results to suggest pin functions
3. **Validation**: Verify declared functions match electrical behavior
4. **Standard Library**: Pre-defined metadata for common ICs