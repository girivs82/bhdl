# Hierarchical Modules with Parameterization - Complete Implementation Checklist

## Overview

This checklist combines hierarchical module support with full parameterization capabilities.

## Phase 1: Parser & AST Extensions

### Grammar Updates (`bhdl-parser/src/grammar.rs`)

#### Module Definition
- [ ] Parse module parameter list after module name
  - [ ] `module Name(param: type = default, ...)`
  - [ ] Support type annotations
  - [ ] Support default values
  - [ ] Handle trailing commas

#### Conditional Structures
- [ ] Add `when` block parsing for modules
  - [ ] `when (condition) { ... }`
  - [ ] `when (cond1) { ... } else when (cond2) { ... }`
  - [ ] Support in module body context

#### Generate Constructs  
- [ ] Add `generate` block parsing
  - [ ] `generate for var in start..end { ... }`
  - [ ] `generate if (condition) { ... }`
  - [ ] Array indexing: `component[i]`

#### Instance Parameters
- [ ] Parse parameters in instantiation
  - [ ] `inst: Module(param1=value1, param2=value2)`
  - [ ] Support positional and named parameters

### AST Nodes (`bhdl-ast/src/`)

#### New Node Types
- [ ] `ModuleParam` struct
  ```rust
  pub struct ModuleParam {
      name: String,
      param_type: Option<TypeRef>,
      default_value: Option<Expr>,
  }
  ```

- [ ] `ConditionalBlock` struct
  ```rust
  pub struct ConditionalBlock {
      condition: Expr,
      body: Vec<ModuleItem>,
      else_clause: Option<Box<ConditionalBlock>>,
  }
  ```

- [ ] `GenerateBlock` struct
  ```rust
  pub struct GenerateBlock {
      kind: GenerateKind,
      body: Vec<ModuleItem>,
  }
  ```

#### Module Extensions
- [ ] Add to `Module` impl:
  - [ ] `params() -> impl Iterator<Item = ModuleParam>`
  - [ ] `conditional_blocks() -> impl Iterator<Item = ConditionalBlock>`
  - [ ] `generate_blocks() -> impl Iterator<Item = GenerateBlock>`

## Phase 2: Analyzer Enhancements

### Parameter System (`bhdl-analyzer/src/parameters.rs`)

- [ ] Create `ParameterContext` struct
  ```rust
  pub struct ParameterContext {
      values: HashMap<String, Value>,
      parent: Option<Arc<ParameterContext>>,
  }
  ```

- [ ] Implement parameter evaluation
  - [ ] Type checking
  - [ ] Default value evaluation
  - [ ] Constraint validation
  - [ ] Expression evaluation with parameters

### Module Instantiation (`bhdl-analyzer/src/instantiation.rs`)

- [ ] Create `ModuleInstantiator` struct
- [ ] Implement instantiation pipeline:
  1. [ ] Validate provided parameters
  2. [ ] Apply defaults for missing parameters
  3. [ ] Create parameter context
  4. [ ] Evaluate conditional blocks
  5. [ ] Expand generate blocks
  6. [ ] Produce instantiated module body

### Conditional Evaluation

- [ ] Implement `ConditionalEvaluator`
  - [ ] Evaluate when conditions
  - [ ] Select active branches
  - [ ] Handle else-if chains
  - [ ] Type check conditions (must be boolean)

### Generate Expansion

- [ ] Implement `GenerateExpander`
  - [ ] Handle for loops with ranges
  - [ ] Create indexed instances
  - [ ] Handle conditional generation
  - [ ] Validate loop bounds

## Phase 3: Synthesis Updates

### Parameterized Synthesis (`bhdl-synthesizer/src/`)

#### Module Cache
- [ ] Create `InstantiatedModuleCache`
  - [ ] Key: (module_name, parameter_values)
  - [ ] Value: Evaluated module body
  - [ ] Prevent re-evaluation of same configuration

#### Instance Generation
- [ ] Update `synthesize_module_instance()`:
  - [ ] Look up or create instantiated module
  - [ ] Use evaluated body instead of template
  - [ ] Handle generated instance arrays
  - [ ] Map parameters to component values

#### Array Handling
- [ ] Support indexed instances
  - [ ] `phase[0]`, `phase[1]`, etc.
  - [ ] Generate unique names in netlist
  - [ ] Preserve array structure for layout

## Phase 4: Advanced Features

### Parameter Constraints
- [ ] Add constraint syntax parsing
  - [ ] `param: type = default where constraint`
- [ ] Implement constraint checker
- [ ] Report clear errors for violations

### Computed Attributes
- [ ] Allow attributes to use parameters
- [ ] Support conditional attributes
- [ ] Lazy evaluation of attribute expressions

### Static Assertions
- [ ] Add `static_assert` parsing
- [ ] Evaluate at analysis time
- [ ] Report errors with context

## Test Suite

### Parser Tests

```bhdl
// test_module_with_params
module Configurable(width: int = 8, signed: bool = false) {
    pin DATA[width]: signal inout;
    when (signed) {
        pin SIGN: signal out;
    }
}

// test_conditional_components
module Adaptive(use_external: bool) {
    when (use_external) {
        osc: ExternalOsc { ... }
    } else {
        osc: InternalOsc { ... }
    }
}

// test_generate_array
module Parallel(n: int = 4) {
    generate for i in 0..n {
        amp[i]: Amplifier { 
            IN -> .input;
            .output -> OUT[i];
        }
    }
}
```

### Analyzer Tests

```rust
#[test]
fn test_parameter_evaluation() {
    // Test default values
    // Test parameter overrides  
    // Test expression evaluation
    // Test type checking
}

#[test]
fn test_conditional_evaluation() {
    // Test when blocks
    // Test nested conditions
    // Test else-if chains
}

#[test]
fn test_generate_expansion() {
    // Test for loops
    // Test array generation
    // Test nested generates
}
```

### End-to-End Tests

```bhdl
// Parameterized power supply
board TestBoard {
    // Different configurations of same module
    buck_5v: BuckConverter(vout=5V, imax=3A) { ... }
    buck_3v3: BuckConverter(vout=3.3V) { ... }
    buck_1v2: BuckConverter(vout=1.2V, fsw=1MHz) { ... }
}

// Should produce different component values for each instance
```

## Implementation Order

1. **Week 1**: Basic parameterization
   - Module parameters (parse & AST)
   - Parameter evaluation
   - Simple instantiation

2. **Week 2**: Conditional structures
   - When blocks
   - Conditional evaluation
   - Component selection

3. **Week 3**: Generate constructs
   - For loops
   - Array instances
   - Generate expansion

4. **Week 4**: Advanced features
   - Constraints
   - Static assertions
   - Computed types

## Success Criteria

- [ ] Can define modules with typed parameters
- [ ] Can instantiate with custom parameter values
- [ ] Conditional blocks work correctly
- [ ] Generate creates proper arrays
- [ ] Parameter constraints are validated
- [ ] Same module with different params → different netlists
- [ ] Performance acceptable for deep hierarchies
- [ ] Clear error messages for parameter issues

## Common Pitfalls to Avoid

1. **Parameter evaluation order** - Use dependency graph
2. **Infinite recursion** - Detect circular parameter dependencies
3. **Generate explosion** - Limit loop bounds
4. **Cache invalidation** - Track parameter changes
5. **Error reporting** - Maintain parameter context in errors

## Example: Fully Parameterized Module

```bhdl
module UniversalFilter(
    order: int = 2 where order >= 1 && order <= 8,
    topology: string = "butterworth",
    cutoff: frequency = 1kHz,
    type: string = "lowpass" where type in ["lowpass", "highpass", "bandpass"]
) {
    pin IN: signal in;
    pin OUT: signal out;
    pin GND: ground;
    
    // Validate parameter combinations
    static_assert(
        !(type == "bandpass" && order < 2),
        "Bandpass requires order >= 2"
    );
    
    // Component values depend on parameters
    attribute pole_freq = cutoff / sqrt(pow(2, 1.0/order) - 1);
    
    // Structure depends on filter type
    when (type == "lowpass") {
        generate for i in 0..order {
            stage[i]: LowpassStage(
                freq = pole_freq * pole_factor(i, order, topology)
            ) {
                when (i == 0) { IN -> .input; }
                else { stage[i-1].output -> .input; }
                
                when (i == order-1) { .output -> OUT; }
            }
        }
    } else when (type == "highpass") {
        // Different topology for highpass
        // ...
    }
}
```

This comprehensive parameterization system will make BHDL modules as flexible as software functions!