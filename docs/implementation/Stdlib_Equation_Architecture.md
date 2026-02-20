# Stdlib-Based Simulation Equation Architecture

## Overview

This document describes the new architecture where simulation equations are defined in stdlib BHDL files rather than hardcoded in the SPICE solver. This enables custom models, vendor libraries, and more accurate simulations.

## Current Architecture

Currently:
1. Component models are hardcoded in `bhdl-spice/src/runtime_models.rs`
2. Equations like diode exponential behavior, voltage regulator feedback, etc. are implemented in Rust
3. Stdlib only provides parameters (e.g., `spice_output_voltage`) but not the actual equations

## Implementation Status

### ✅ Completed
1. **Parser already supports expressions in attributes** - No string quotes needed!
2. **Equation parser/interpreter implemented** in `bhdl-spice/src/equation_engine.rs`
3. **Runtime model engine updated** to use equation-based models
4. **Stdlib components updated** with equation attributes (resistor.bhdl, led.bhdl)

### Key Discovery
The BHDL parser already supports mathematical expressions directly in attributes:
```bhdl
// This already works - no strings needed!
attribute spice_equation_i = v_diff / resistance;
attribute spice_equation_di_dv = 1 / resistance;
attribute spice_equation_conditional = v_diff > 0.1 ? expr1 : expr2;
```

## Implemented Architecture

### 1. Equation Representation in Stdlib

Components will define their equations using BHDL attributes:

```bhdl
entity Res(value: resistance) {
    // ... pins ...
    
    // Define the equation for current as a function of voltage
    attribute spice_equation_i = "v_diff / value";
    
    // Define the derivative for Newton-Raphson (conductance)
    attribute spice_equation_di_dv = "1 / value";
}

entity LED(color: string) {
    // ... pins ...
    
    // Constants used in equations
    attribute spice_vt = "0.026";  // Thermal voltage
    attribute spice_n = "2.0";     // Ideality factor
    
    // Saturation current calculation (from forward voltage/current)
    attribute spice_is_calc = "forward_current / (exp(min(forward_voltage / (spice_n * spice_vt), 35.0)) - 1.0)";
    
    // Current equation (exponential diode model)
    attribute spice_equation_i = """
        if v_diff > 0.1 {
            spice_is * (exp(min(v_diff / (spice_n * spice_vt), 35.0)) - 1.0)
        } else if v_diff > -0.1 {
            1e-9 * v_diff
        } else {
            -spice_is
        }
    """;
    
    // Derivative equation
    attribute spice_equation_di_dv = """
        if v_diff > 0.1 {
            min(max(spice_is / (spice_n * spice_vt) * exp(min(v_diff / (spice_n * spice_vt), 35.0)), 1e-12), 1000.0)
        } else if v_diff > -0.1 {
            1e-9
        } else {
            1e-12
        }
    """;
}

entity LM7805() {
    // ... pins ...
    
    // Adaptive voltage regulator equations
    attribute spice_equation_mode = """
        if v1 >= (output_voltage + dropout_voltage) {
            "regulated"
        } else {
            "dropout"
        }
    """;
    
    attribute spice_equation_i_regulated = """
        let voltage_error = v2 - output_voltage;
        let headroom = v1 - (output_voltage + dropout_voltage);
        let headroom_factor = min(max(headroom / 20.0, 0.1), 1.0);
        let error_magnitude = abs(voltage_error);
        let error_scaling = if error_magnitude > 1.0 {
            1.0 / (1.0 + error_magnitude)
        } else if error_magnitude < 0.01 {
            1.0 + (0.01 - error_magnitude) * 10.0
        } else {
            1.0
        };
        let adaptive_gain = 1.0 * headroom_factor * error_scaling;
        -adaptive_gain * voltage_error + quiescent_current
    """;
    
    attribute spice_equation_i_dropout = """
        let r_on = dropout_voltage / max_output_current;
        v_diff / r_on + quiescent_current
    """;
}
```

### 2. Equation Language Features

The equation language will support:
- Basic arithmetic: `+`, `-`, `*`, `/`, `^` (power)
- Comparisons: `>`, `<`, `>=`, `<=`, `==`, `!=`
- Conditional: `if ... { ... } else { ... }`
- Let bindings: `let var = expr;`
- Math functions: `exp()`, `log()`, `sqrt()`, `abs()`, `min()`, `max()`
- Access to:
  - Component parameters: `value`, `forward_voltage`, etc.
  - Node voltages: `v1`, `v2`, `v_diff`
  - Other attributes: `spice_*` attributes

### 3. Equation Parser/Interpreter

Create a new module `bhdl-spice/src/equation_engine.rs`:

```rust
pub struct EquationEngine {
    // AST representation of equations
    equations: HashMap<String, EquationAst>,
    // Variable bindings
    variables: HashMap<String, f64>,
}

impl EquationEngine {
    /// Parse equation string into AST
    pub fn parse_equation(&mut self, name: &str, equation: &str) -> Result<()>;
    
    /// Evaluate equation with given variable bindings
    pub fn evaluate(&self, equation_name: &str, vars: &HashMap<String, f64>) -> Result<f64>;
    
    /// Get all variables referenced by an equation
    pub fn get_variables(&self, equation_name: &str) -> Vec<String>;
}
```

### 4. Integration with Runtime Models

Update `RuntimeModelEngine` to use equations:

```rust
impl RuntimeModelEngine {
    fn execute_stdlib_model(&mut self, component_def: &StdlibComponentDefinition, ctx: &mut ModelExecutionContext) -> Result<()> {
        // Load equations from attributes
        if let Some(i_eq) = component_def.attributes.get("spice_equation_i") {
            self.equation_engine.parse_equation("i", i_eq)?;
        }
        if let Some(di_dv_eq) = component_def.attributes.get("spice_equation_di_dv") {
            self.equation_engine.parse_equation("di_dv", di_dv_eq)?;
        }
        
        // Build variable bindings
        let mut vars = HashMap::new();
        vars.insert("v_diff".to_string(), ctx.v_diff);
        vars.insert("v1".to_string(), ctx.get_v1());
        vars.insert("v2".to_string(), ctx.get_v2());
        
        // Add component parameters
        for (key, value) in &component_def.attributes {
            if let Ok(num_val) = self.parse_numeric_value(value) {
                vars.insert(key.clone(), num_val);
            }
        }
        
        // Evaluate equations
        let current = self.equation_engine.evaluate("i", &vars)?;
        let conductance = self.equation_engine.evaluate("di_dv", &vars)?;
        
        // Stamp into circuit matrix
        ctx.stamp_linear_element(conductance, current);
        
        Ok(())
    }
}
```

### 5. Benefits

1. **Custom Models**: Users can define their own component models with custom equations
2. **Vendor Libraries**: Manufacturers can provide accurate SPICE models as BHDL files
3. **Model Evolution**: Models can be improved without changing solver code
4. **Debugging**: Equations are visible and modifiable in stdlib files
5. **Domain-Specific Models**: Different equation sets for different analysis types (DC, AC, transient)

### 6. Migration Plan

1. Implement equation parser and interpreter
2. Add equation attributes to existing stdlib components
3. Update runtime model engine to use equations
4. Keep hardcoded models as fallback initially
5. Gradually migrate all models to equation-based
6. Remove hardcoded models once stable

### 7. Advanced Features (Future)

1. **Equation Compilation**: Compile equations to native code for performance
2. **Symbolic Differentiation**: Automatically derive di_dv from i equation
3. **Multi-Physics**: Support thermal, mechanical equations
4. **State Variables**: Support for reactive components (capacitors, inductors)
5. **Behavioral Models**: Complex state machines for digital components

## Example: Custom Vendor Model

A vendor could provide a highly accurate MOSFET model:

```bhdl
entity IRF540N() {
    // Pins...
    
    // Level 3 MOSFET model equations
    attribute spice_equation_ids = """
        let vth = vth0 + gamma * (sqrt(abs(2 * phi_f - vbs)) - sqrt(2 * phi_f));
        let vgs_eff = vgs - vth;
        if vgs_eff <= 0 {
            0.0
        } else if vds <= vgs_eff {
            // Linear region
            kp * w_eff / l_eff * ((vgs_eff - vds/2) * vds) * (1 + lambda * vds)
        } else {
            // Saturation region
            0.5 * kp * w_eff / l_eff * vgs_eff^2 * (1 + lambda * vds)
        }
    """;
    
    // Temperature effects, capacitances, etc.
    attribute spice_equation_cgs = "cox * w_eff * l_eff + cgso * w_eff";
    // ... more equations ...
}
```

This architecture makes BHDL a complete hardware description language that includes not just connectivity but also behavioral modeling.