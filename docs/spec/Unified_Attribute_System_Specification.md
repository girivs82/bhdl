> **STATUS: aspirational — not implemented (verified 2026-07-09).** The
> behavioral-attribute surface here (`when (cond) { … += … }` blocks,
> expression attributes recomputed each timestep, usage-inferred mutability)
> does not parse and is not built. Shipped attributes are static
> `attribute name = value;` bindings (main spec §6.3). Dynamic/behavioral
> modeling is tracked separately in [Behavioral_Models.md](Behavioral_Models.md)
> (also a proposal). Design intent, not current syntax.

# Unified Attribute System Technical Specification

## Abstract

This specification defines the unified attribute system for BHDL v2.1, which extends the existing `attribute` keyword to support behavioral modeling without introducing new keywords. The system enables both static metadata and dynamic behavioral expressions through a single, consistent syntax.

## 1. Attribute Categories

### 1.1 Static Attributes (Existing)
```bhdl
attribute title = "Buck Converter";
attribute version = "1.0";
attribute author = "Jane Doe";
```

**Characteristics:**
- Evaluated at compile time
- Immutable
- String or numeric literals only
- Used for metadata and documentation

### 1.2 Expression Attributes (New)
```bhdl
attribute error = vref - FB;
attribute duty = clamp(0.5 + error * 0.1, 0.1, 0.9);
attribute temp_c = (TEMP_SENSE - 0.5V) / 10mV;
```

**Characteristics:**
- Can reference pins and other attributes
- Evaluated at simulation time
- Immutable (recomputed each timestep)
- Support full expression syntax

### 1.3 Mutable Attributes (New)
```bhdl
attribute vref = 0V;  // Becomes mutable when modified

when (ENABLE && vref < 3.3V) {
    vref += 3.3V / 10ms * dt;  // Modification makes it mutable
}
```

**Characteristics:**
- Mutability inferred from usage
- Maintain state across timesteps
- Can be modified in `when` blocks
- Support compound assignment operators

## 2. Syntax Definition

### 2.1 Grammar Extensions

```ebnf
attribute_decl ::= 'attribute' IDENTIFIER ':' type_spec? '=' expression ';'

expression ::= literal
             | identifier
             | pin_reference
             | binary_expression
             | unary_expression
             | conditional_expression
             | function_call
             | parenthesized_expression

pin_reference ::= IDENTIFIER  // Resolved based on context

assignment_stmt ::= target '=' expression ';'
                  | target compound_op expression ';'

compound_op ::= '+=' | '-=' | '*=' | '/='

target ::= identifier | pin_reference
```

### 2.2 Type Inference Rules

1. **Literal Types**:
   - `3.3V` → `voltage`
   - `10mA` → `current`
   - `1.5` → `ratio` or `number`
   - `"text"` → `string`

2. **Expression Types**:
   - Binary operators preserve unit types
   - Pin references inherit pin types
   - Conditionals require matching branch types

3. **Implicit Conversions**:
   - `ratio` → PWM duty cycle
   - `voltage` → analog output
   - `boolean` → digital output

## 3. Semantic Rules

### 3.1 Mutability Detection

An attribute is considered mutable if:
1. It appears as the target of an assignment in a `when` block
2. It appears as the target of a compound assignment anywhere
3. It is explicitly declared with `var` (future extension)

```bhdl
entity Example {
    attribute static_val = 3.3V;        // Immutable
    attribute computed = a + b;         // Immutable (recomputed)
    attribute counter = 0;              // Mutable (modified below)
    
    when (condition) {
        counter += 1;  // Makes 'counter' mutable
    }
}
```

### 3.2 Dependency Analysis

The analyzer must track dependencies between attributes:

```bhdl
attribute a = pin1 + pin2;
attribute b = a * 2;
attribute c = b + a;  // Depends on both a and b
```

**Evaluation Order**: Topological sort based on dependency graph

### 3.3 Pin Assignment Rules

Attributes can be assigned to output pins:

```bhdl
pin PWM: digital out;
attribute duty = calculate_duty();

PWM = duty;  // Implicit conversion from ratio to PWM
```

**Restrictions**:
- Only output pins can be assigned from attributes
- Type must be compatible or implicitly convertible
- Assignment creates a continuous connection

## 4. Built-in Variables

### 4.1 The `dt` Variable

```bhdl
when (ramping) {
    value += rate * dt;  // dt is simulation timestep in seconds
}
```

**Properties**:
- Type: `time` (seconds)
- Scope: Global
- Value: Set by simulation engine
- Read-only

## 5. Evaluation Semantics

### 5.1 Evaluation Phases

1. **Static Evaluation** (Compile Time):
   - Static attributes with literal values
   - Constant expressions
   - Type checking

2. **Dynamic Evaluation** (Simulation Time):
   ```
   for each timestep:
       1. Read all pin values
       2. Evaluate expression attributes (dependency order)
       3. Execute when blocks
       4. Update mutable attributes
       5. Write pin assignments
   ```

### 5.2 When Block Execution

```bhdl
when (condition_expr) {
    // Statements execute if condition is true
    mutable_attr = new_value;
    mutable_attr += increment;
}
```

**Execution Rules**:
- Conditions evaluated every timestep
- Statements execute in order when condition is true
- Multiple when blocks execute in source order

## 6. Examples

### 6.1 Thermal Protection
```bhdl
entity ThermalProtection {
    pin TEMP_SENSE: analog in;
    pin ENABLE_OUT: digital out;
    
    // Convert sensor voltage to temperature
    attribute temp_c = (TEMP_SENSE - 0.5V) / 10mV;
    
    // Hysteresis logic
    attribute shutdown_temp = 125;
    attribute restart_temp = 100;
    attribute is_shutdown = false;  // Mutable
    
    when (temp_c > shutdown_temp) {
        is_shutdown = true;
    }
    
    when (temp_c < restart_temp) {
        is_shutdown = false;
    }
    
    ENABLE_OUT = !is_shutdown;
}
```

### 6.2 Soft-Start Controller
```bhdl
entity SoftStart {
    pin ENABLE: digital in;
    pin FB: analog in;
    pin COMP: analog out;
    
    // Soft-start voltage reference
    attribute vref = 0V;  // Mutable
    attribute target = 3.3V;
    attribute ramp_time = 10ms;
    
    // Control loop
    attribute error = vref - FB;
    attribute comp_voltage = clamp(2.5V + error * 10, 0.5V, 4.5V);
    
    // Ramp vref when enabled
    when (ENABLE && vref < target) {
        vref += target / ramp_time * dt;
    }
    
    when (!ENABLE) {
        vref = 0V;  // Reset on disable
    }
    
    COMP = comp_voltage;
}
```

### 6.3 PWM Generator
```bhdl
entity SimplePWM {
    pin FREQ_SET: analog in;
    pin DUTY_SET: analog in;
    pin PWM_OUT: digital out;
    
    // Configuration from analog inputs
    attribute frequency = 100kHz * FREQ_SET / 3.3V;
    attribute duty = DUTY_SET / 3.3V;
    
    // Internal counter (mutable)
    attribute counter = 0.0;
    attribute period = 1.0 / frequency;
    
    // Update counter
    when (true) {  // Always execute
        counter += dt;
        when (counter >= period) {
            counter -= period;  // Wrap around
        }
    }
    
    // Generate PWM
    attribute pwm_high = (counter / period) < duty;
    PWM_OUT = pwm_high;
}
```

## 7. Implementation Notes

### 7.1 Parser Modifications

1. Extend `parse_attribute` to accept expressions
2. Add compound assignment operators
3. Ensure `dt` is recognized as built-in identifier

### 7.2 Analyzer Requirements

1. Track attribute dependencies
2. Detect mutable attributes from usage
3. Type check all expressions
4. Validate pin assignments

### 7.3 Simulator Integration

1. Maintain attribute state table
2. Implement expression evaluator
3. Execute when blocks efficiently
4. Handle dt injection

## 8. Backward Compatibility

All existing BHDL code remains valid:
- Static attributes work unchanged
- No new keywords introduced
- Existing entities need no modifications

## 9. Future Extensions

### 9.1 Explicit Mutability (Optional)
```bhdl
attribute var counter = 0;  // Explicitly mutable
```

### 9.2 Attribute Arrays (Future)
```bhdl
attribute var samples[10] = [0; 10];  // Array of 10 zeros
```

### 9.3 Attribute Functions (Future)
```bhdl
attribute fn calculate_duty(error: voltage) -> ratio {
    return clamp(0.5 + error * 0.1, 0.1, 0.9);
}
```

## 10. Conclusion

The unified attribute system provides a clean, extensible mechanism for behavioral modeling in BHDL without adding complexity. By reusing the existing `attribute` keyword and inferring properties from usage, we maintain simplicity while enabling powerful new capabilities.