# Component Model Extraction

## Overview

The Component Model Extraction system bridges multiple sources of component information to create accurate SPICE models. It unifies data from the BHDL analyzer's symbol table, component database, stdlib definitions, and user specifications.

## Architecture

### Model Sources

1. **Symbol Table** (High Confidence: 90%)
   - Data from BHDL analyzer semantic analysis
   - Type information, values, and attributes
   - Example: `Res(4.7kΩ, power=0.25W)`

2. **Component Database** (Very High Confidence: 95%)
   - KiCad symbol library data
   - Manufacturer specifications
   - Pin mappings and electrical characteristics

3. **BHDL Stdlib** (Very High Confidence: 95%)
   - Standard component definitions
   - Pre-defined parameters and models
   - Validated electrical characteristics

4. **User-Defined** (Full Confidence: 100%)
   - Explicit SPICE attributes in BHDL
   - Custom model parameters
   - Override default behaviors

5. **Context Inference** (Low Confidence: 30%)
   - Based on circuit connections
   - Neighboring components
   - Common patterns (e.g., LED + resistor)

### Extraction Process

```rust
let mut extractor = ComponentModelExtractor::new();

// 1. Try symbol table first (from analyzer)
let model = extractor.extract_from_symbol_table("R1", &symbol_data)?;

// 2. Fall back to database
let model = extractor.extract_from_database(component_id, db_path).await?;

// 3. Check stdlib definitions
let model = extractor.extract_from_stdlib("LED", &stdlib_data)?;

// 4. User overrides everything
let model = extractor.extract_from_user_attributes("D1", &attrs)?;

// 5. Last resort: inference
let model = extractor.infer_from_context("C1", &connections, &nearby)?;
```

## Features

### Smart Unit Parsing

The system correctly parses electrical units with SI prefixes:

```
"4.7k" → 4700.0
"100nF" → 100e-9
"2.2μH" → 2.2e-6
"1N4148" → Diode model lookup
```

### Parameter Mapping

Component-specific parameter extraction:

```rust
// Resistor
"value" → "resistance"
"power" → "power_rating"

// Capacitor
"value" → "capacitance"
"voltage" → "voltage_rating"

// LED
"color" → Vf calculation
"max_current" → "forward_current"
```

### SPICE Model Creation

Extracted models are converted to full SPICE models:

```rust
let spice_model = extractor.create_spice_model(&extracted)?;
// Returns Box<dyn SpiceModel> ready for simulation
```

## Example Usage

### From Symbol Table
```rust
let mut symbol_data = HashMap::new();
symbol_data.insert("component_type".to_string(), "resistor".to_string());
symbol_data.insert("value".to_string(), "10k".to_string());
symbol_data.insert("power".to_string(), "0.5W".to_string());

let model = extractor.extract_from_symbol_table("R1", &symbol_data)?;
// Creates ResistorModel with R=10kΩ, P=0.5W
```

### From User Attributes
```rust
let mut attrs = HashMap::new();
attrs.insert("spice_model".to_string(), "diode".to_string());
attrs.insert("spice_is".to_string(), "1e-15".to_string());
attrs.insert("spice_n".to_string(), "1.8".to_string());
attrs.insert("spice_vj".to_string(), "0.7".to_string());

let model = extractor.extract_from_user_attributes("D1", &attrs)?;
// Creates DiodeModel with custom Shockley parameters
```

### Context Inference
```rust
let connections = vec!["VCC".to_string(), "LED1.A".to_string()];
let nearby = vec!["LED1".to_string()];

let model = extractor.infer_from_context("R1", &connections, &nearby)?;
// Infers current-limiting resistor (confidence: 30%)
```

## Integration Points

### With Analyzer
- Receives symbol table data
- Uses resolved constants
- Leverages type information

### With Database
- Queries component library
- Retrieves manufacturer data
- Maps pins to functions

### With SPICE Engine
- Provides models for simulation
- Ensures parameter accuracy
- Enables behavioral modeling

## Benefits

1. **Unified Interface**: Single API for all model sources
2. **Confidence Tracking**: Know reliability of extracted data
3. **Fallback Chain**: Multiple sources ensure model availability
4. **Type Safety**: Strong typing prevents parameter mismatches
5. **Extensibility**: Easy to add new sources or model types

## Future Enhancements

1. **Machine Learning**: Learn component patterns from usage
2. **Datasheet Import**: Extract parameters from PDF datasheets
3. **Online Database**: Query component databases via API
4. **Model Validation**: Compare extracted vs. measured behavior
5. **Parameter Optimization**: Tune models to match measurements