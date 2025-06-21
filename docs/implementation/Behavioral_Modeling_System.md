# BHDL Behavioral Modeling System

## Overview

The BHDL behavioral modeling system enables designers to specify time-dependent circuit behavior, mathematical relationships between signals, and conditional logic within their hardware descriptions. This system forms the foundation for behavioral simulation capabilities in BHDL v2.0.

## Architecture

### 1. Extended Attribute System

The core of behavioral modeling is the extended attribute system that supports expressions:

```bhdl
board behavioral_example {
    // Static attributes (literals)
    attribute vcc_nominal = 5.0;
    attribute threshold = 3.3;
    
    // Expression attributes (computed)
    attribute margin = vcc_nominal - threshold;
    attribute percentage = (margin / vcc_nominal) * 100;
    
    // Time-dependent attributes using built-ins
    attribute sample_time = dt * 1000;  // Convert to milliseconds
    attribute phase = 2 * pi * frequency * t;
}
```

#### Key Components:

- **AttributeDecl AST Node** (`bhdl-ast/src/attributes.rs`)
  - Represents attribute declarations with expression support
  - Methods for identifying expression vs literal attributes
  - Reference extraction for dependency analysis

- **Extended Expression AST** (`bhdl-ast/src/expr.rs`)
  - Enhanced Expr enum with PinRef variant
  - Support for all arithmetic, logical, and comparison operators
  - Function call expressions for mathematical functions

### 2. Dependency Analysis System

The dependency analyzer tracks relationships between attributes to ensure correct evaluation order:

```rust
// bhdl-analyzer/src/attribute_analysis.rs
pub struct AttributeAnalyzer {
    attributes: HashMap<String, AttributeInfo>,
    dependencies: HashMap<String, HashSet<String>>,
    mutable_attributes: HashSet<String>,
}
```

#### Features:

- **Dependency Tracking**: Automatically extracts which attributes reference others
- **Circular Dependency Detection**: Uses DFS to detect and report circular references
- **Topological Sort**: Determines safe evaluation order for attributes
- **Mutable Attribute Detection**: Identifies attributes modified in when blocks

### 3. Behavioral Constructs

#### When Blocks

Conditional behavior based on runtime conditions:

```bhdl
when (voltage > threshold) {
    attribute led_state = 1;
    attribute current_limit += 0.001;  // Increment operator
}

when (temperature > max_temp) {
    attribute shutdown = true;
    attribute error_count = error_count + 1;
}
```

#### AST Support (`bhdl-ast/src/behavioral.rs`):

- **WhenBlock**: Represents conditional blocks with condition expressions
- **AttributeAssignment**: Handles assignments within when blocks
- **BehavioralStmt**: Enum for different behavioral statement types

### 4. Built-in Variables

The system provides built-in variables for simulation:

| Variable | Type | Description | Constant |
|----------|------|-------------|----------|
| `dt` | Real | Simulation time step (seconds) | Yes |
| `t` | Real | Current simulation time (seconds) | No |
| `pi` | Real | Mathematical constant π | Yes |
| `e` | Real | Mathematical constant e | Yes |

#### Implementation (`bhdl-analyzer/src/builtin_variables.rs`):

```rust
pub struct BuiltinVariableManager {
    variables: HashMap<String, BuiltinVariable>,
}

pub struct SimulationContext {
    pub current_time: f64,
    pub time_step: f64,
    pub custom_values: HashMap<String, f64>,
}
```

### 5. Expression Evaluator

The runtime expression evaluator computes attribute values during simulation:

```rust
// bhdl-analyzer/src/expression_evaluator.rs
pub enum RuntimeValue {
    Integer(i64),
    Real(f64),
    Boolean(bool),
    String(String),
}

pub struct EvaluationContext<'a> {
    pub attributes: HashMap<String, RuntimeValue>,
    pub pins: HashMap<String, RuntimeValue>,
    pub simulation: &'a SimulationContext,
}
```

#### Supported Operations:

- **Arithmetic**: +, -, *, /, %
- **Comparison**: ==, !=, <, >, <=, >=
- **Logical**: &&, ||, !
- **Ternary**: condition ? true_expr : false_expr
- **Functions**: sin, cos, tan, sqrt, abs, pow, log, exp, min, max, floor, ceil, round

### 6. Parser Integration

The parser has been extended to support behavioral constructs:

- **Attribute declarations** with expression values
- **When blocks** with condition parsing
- **Increment/decrement operators** (+=, -=)
- **Built-in variable recognition**

#### Whitespace Fix

A critical fix was implemented to ensure identifier references don't capture leading whitespace:

```rust
// bhdl-ast/src/common.rs
impl IdentRef {
    pub fn token(&self) -> Option<SyntaxToken<BhdlLanguage>> { 
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT)
    }
}
```

## Usage Examples

### 1. RC Circuit Time Constant

```bhdl
board rc_circuit {
    attribute resistance = 10e3;  // 10kΩ
    attribute capacitance = 100e-9;  // 100nF
    attribute tau = resistance * capacitance;  // Time constant
    
    // Voltage as function of time
    attribute v_cap = 5.0 * (1 - exp(-t / tau));
}
```

### 2. PWM Duty Cycle Calculator

```bhdl
board pwm_controller {
    attribute period = 0.001;  // 1ms period
    attribute frequency = 1 / period;
    
    attribute time_high = 0;
    attribute time_low = 0;
    attribute edge_count = 0;
    
    when (pwm_signal > 2.5) {
        attribute time_high += dt;
    }
    
    when (pwm_signal <= 2.5) {
        attribute time_low += dt;
    }
    
    when (edge_count > 100) {
        attribute duty_cycle = time_high / (time_high + time_low) * 100;
        attribute edge_count = 0;
        attribute time_high = 0;
        attribute time_low = 0;
    }
}
```

### 3. Temperature-Dependent Behavior

```bhdl
board thermal_protection {
    attribute temp_celsius = 25;
    attribute temp_kelvin = temp_celsius + 273.15;
    
    // Semiconductor junction voltage temperature coefficient
    attribute vbe_25c = 0.7;  // Base-emitter voltage at 25°C
    attribute temp_coeff = -2.1e-3;  // -2.1mV/°C
    attribute vbe = vbe_25c + temp_coeff * (temp_celsius - 25);
    
    // Thermal shutdown
    attribute max_temp = 125;
    attribute shutdown = false;
    
    when (temp_celsius > max_temp) {
        attribute shutdown = true;
    }
}
```

## Implementation Details

### Semantic Analysis Pipeline

1. **Pass 1**: Build symbol tables and collect attribute declarations
2. **Pass 2**: Resolve references with built-in variable support
3. **Attribute Analysis**: 
   - Extract dependencies
   - Detect circular references
   - Determine evaluation order
   - Identify mutable attributes

### Evaluation Process

1. **Static Attributes**: Evaluated once at initialization
2. **Expression Attributes**: Re-evaluated when dependencies change
3. **When Blocks**: Evaluated each simulation step
4. **Mutable Attributes**: Updated by when block assignments

### Type System Integration

The behavioral system integrates with BHDL's type system:

- Attributes can have explicit types: `attribute voltage: real = 3.3;`
- Type inference from expressions
- Type checking for assignments and operations

## Testing

Comprehensive tests have been implemented:

1. **Basic Expression Evaluation** (`test_behavioral_attributes.rs`)
   - Arithmetic operations
   - Built-in variables
   - Function calls

2. **Dependency Analysis** (`test_attr_dependencies.rs`)
   - Circular dependency detection
   - Topological sort verification
   - Reference extraction

3. **Mutable Attributes** (`test_mutable_detection.rs`)
   - When block parsing
   - Increment/decrement operators
   - State tracking

4. **Whitespace Handling** (`test_identifier_whitespace.rs`)
   - Clean identifier extraction
   - Proper dependency tracking

## Future Enhancements

1. **Event System**: Support for edge-triggered events
2. **Time Functions**: Additional time-based functions (delays, timers)
3. **State Machines**: Formal state machine constructs
4. **Assertions**: Runtime assertion checking
5. **Analog Behavior**: Continuous-time differential equations

## API Reference

### Key Types

```rust
// Attribute information
pub struct AttributeInfo {
    pub name: String,
    pub attribute_type: AttributeType,
    pub dependencies: AttributeDependency,
    pub is_mutable: bool,
    pub decl: AttributeDecl,
}

// Attribute types
pub enum AttributeType {
    Static(String),           // Literal value
    Expression(Vec<String>),  // Expression with dependencies
    Mutable,                  // Modified in when blocks
}

// Runtime values
pub enum RuntimeValue {
    Integer(i64),
    Real(f64),
    Boolean(bool),
    String(String),
}
```

### Key Functions

```rust
// Analyze attributes in a syntax tree
pub fn analyze(&mut self, root: &SyntaxNode<BhdlLanguage>) -> AttributeAnalysisResult

// Evaluate an expression
pub fn evaluate(expr: &Expr, context: &EvaluationContext) -> Result<RuntimeValue, EvaluationError>

// Check if variable is built-in
pub fn is_builtin(&self, name: &str) -> bool

// Get built-in variable value
pub fn get_builtin_value(&self, name: &str) -> Option<f64>
```

## Conclusion

The BHDL behavioral modeling system provides a powerful foundation for describing time-dependent circuit behavior. With expression attributes, conditional logic, and built-in simulation variables, designers can create sophisticated behavioral models that accurately represent circuit operation during simulation.

The system is designed to be extensible, with clean separation between parsing, analysis, and evaluation phases. This architecture enables future enhancements while maintaining compatibility with existing behavioral descriptions.