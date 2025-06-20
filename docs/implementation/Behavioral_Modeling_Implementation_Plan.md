# BHDL Behavioral Modeling Implementation Plan

## Overview

This document outlines the implementation strategy for adding behavioral modeling to BHDL using the unified attribute system. We're implementing only 5 new concepts to keep the language simple while enabling powerful behavioral simulation capabilities.

## The 5 New Concepts

1. **Extended Attributes** - Expressions and pin references in attributes
2. **Mutable Attributes** - Inferred from usage in `when` blocks
3. **External Model Decorator** - `@behavioral` for PLI integration
4. **Testbench Co-simulation** - `@cosim` for external test harnesses
5. **Built-in `dt` Variable** - Global timestep for time-based calculations

## Implementation Phases

### Phase 1: Extended Attribute System (Weeks 1-3)

#### 1.1 Parser Extensions

**File**: `bhdl-parser/src/grammar.rs`

```rust
// Extend attribute parsing to support expressions
pub fn parse_attribute(p: &mut Parser) -> Option<CompletedMarker> {
    let m = p.start();
    p.expect(T![attribute]);
    p.expect(T![ident]);
    p.expect(T![=]);
    
    // NEW: Parse expression instead of just literals
    parse_expression(p);
    
    p.expect(T![;]);
    Some(m.complete(p, ATTRIBUTE_DECL))
}

// Support pin references in expressions
pub fn parse_primary_expr(p: &mut Parser) -> Option<CompletedMarker> {
    match p.current() {
        T![number] => parse_number(p),
        T![string] => parse_string(p),
        T![ident] => {
            // NEW: Could be pin reference or attribute reference
            parse_identifier_or_pin_ref(p)
        }
        // ... other cases
    }
}
```

#### 1.2 AST Extensions

**File**: `bhdl-ast/src/nodes.rs`

```rust
#[derive(Debug, Clone)]
pub enum AttributeValue {
    // Existing
    Literal(Literal),
    
    // NEW
    Expression(Box<Expression>),
}

impl Attribute {
    pub fn is_expression(&self) -> bool {
        matches!(self.value, AttributeValue::Expression(_))
    }
    
    pub fn references_pins(&self) -> Vec<String> {
        // Collect all pin references in the expression
        match &self.value {
            AttributeValue::Expression(expr) => expr.collect_pin_refs(),
            _ => vec![],
        }
    }
}
```

#### 1.3 Semantic Analysis

**File**: `bhdl-analyzer/src/behavioral_analysis.rs`

```rust
pub struct BehavioralAnalyzer {
    // Track which attributes are modified
    mutable_attributes: HashSet<String>,
    // Track attribute dependencies
    attribute_deps: HashMap<String, HashSet<String>>,
}

impl BehavioralAnalyzer {
    pub fn analyze_module(&mut self, module: &Module) {
        // Analyze attribute declarations
        for attr in &module.attributes {
            if let AttributeValue::Expression(expr) = &attr.value {
                self.analyze_attribute_expression(attr.name(), expr);
            }
        }
        
        // Analyze when blocks for mutations
        for when_block in &module.when_blocks {
            self.analyze_when_block(when_block);
        }
    }
    
    fn analyze_when_block(&mut self, when_block: &WhenBlock) {
        // Find attribute modifications
        for stmt in &when_block.body {
            if let Statement::Assignment(target, _) = stmt {
                if self.is_attribute_reference(target) {
                    self.mutable_attributes.insert(target.name());
                }
            }
        }
    }
}
```

### Phase 2: Time-Based Behavioral Support (Weeks 4-5)

#### 2.1 Built-in `dt` Variable

**File**: `bhdl-analyzer/src/builtin_symbols.rs`

```rust
pub struct BuiltinSymbols {
    pub dt: Symbol,
}

impl BuiltinSymbols {
    pub fn new() -> Self {
        Self {
            dt: Symbol {
                name: "dt".to_string(),
                kind: SymbolKind::Variable,
                ty: Type::Time,
                value: Some(Value::Runtime),
                mutable: false,
                builtin: true,
            },
        }
    }
}
```

#### 2.2 When Block Enhancements

**File**: `bhdl-analyzer/src/when_analysis.rs`

```rust
pub struct WhenAnalyzer {
    pub fn analyze_when_assignments(&mut self, when_block: &WhenBlock) {
        for stmt in &when_block.body {
            match stmt {
                Statement::Assignment(target, expr) => {
                    // Support compound assignments
                    self.validate_attribute_mutation(target, expr);
                }
                Statement::CompoundAssignment(target, op, expr) => {
                    // NEW: +=, -=, *=, /=
                    self.validate_compound_assignment(target, op, expr);
                }
            }
        }
    }
}
```

### Phase 3: Behavioral Simulation Engine (Weeks 6-8)

#### 3.1 Simulation State Management

**File**: `bhdl-spice/src/behavioral_sim.rs`

```rust
pub struct BehavioralSimulator {
    // Mutable attribute states
    attribute_states: HashMap<(ModuleId, String), Value>,
    // Attribute expressions for evaluation
    attribute_exprs: HashMap<(ModuleId, String), Expression>,
    // When block conditions and actions
    when_blocks: Vec<WhenBlock>,
}

impl BehavioralSimulator {
    pub fn step(&mut self, dt: f64) {
        // 1. Evaluate all attribute expressions
        self.evaluate_attribute_expressions();
        
        // 2. Check and execute when blocks
        self.execute_when_blocks(dt);
        
        // 3. Update pin values from attributes
        self.update_pin_assignments();
    }
    
    fn evaluate_attribute_expressions(&mut self) {
        // Topological sort based on dependencies
        let sorted = self.topological_sort_attributes();
        
        for (module_id, attr_name) in sorted {
            let expr = &self.attribute_exprs[&(module_id, attr_name)];
            let value = self.evaluate_expression(expr);
            self.attribute_states.insert((module_id, attr_name), value);
        }
    }
}
```

#### 3.2 Expression Evaluator

**File**: `bhdl-spice/src/expression_eval.rs`

```rust
pub struct ExpressionEvaluator {
    pin_values: HashMap<String, f64>,
    attribute_values: HashMap<String, Value>,
    dt: f64,
}

impl ExpressionEvaluator {
    pub fn evaluate(&self, expr: &Expression) -> Value {
        match expr {
            Expression::Binary(left, op, right) => {
                let lval = self.evaluate(left);
                let rval = self.evaluate(right);
                self.apply_binary_op(lval, op, rval)
            }
            Expression::PinRef(name) => {
                Value::Number(self.pin_values[name])
            }
            Expression::AttributeRef(name) => {
                self.attribute_values[name].clone()
            }
            Expression::Identifier(name) if name == "dt" => {
                Value::Number(self.dt)
            }
            // ... other cases
        }
    }
}
```

### Phase 4: PLI Integration (Weeks 9-12)

#### 4.1 Decorator Parsing

**File**: `bhdl-parser/src/decorators.rs`

```rust
pub fn parse_decorator(p: &mut Parser) -> Option<CompletedMarker> {
    let m = p.start();
    p.expect(T![@]);
    
    let name = p.expect(T![ident]);
    
    if p.at(T!['(']) {
        parse_decorator_args(p);
    }
    
    Some(m.complete(p, DECORATOR))
}

fn parse_decorator_args(p: &mut Parser) {
    p.expect(T!['(']);
    
    while !p.at(T![')']) && !p.at(T![eof]) {
        p.expect(T![ident]); // arg name
        p.expect(T![=]);
        parse_decorator_value(p);
        
        if !p.at(T![')']) {
            p.expect(T![,]);
        }
    }
    
    p.expect(T![')']);
}
```

#### 4.2 PLI Interface Design

**File**: `bhdl-pli/src/lib.rs`

```rust
pub trait BehavioralModel: Send + Sync {
    /// Initialize the model with pin interface
    fn initialize(&mut self, pins: &PinInterface) -> Result<(), PLIError>;
    
    /// Step the model forward by dt
    fn step(&mut self, dt: f64) -> Result<(), PLIError>;
    
    /// Batch step for performance
    fn step_batch(&mut self, dt: f64, count: usize) -> Result<Vec<StateUpdate>, PLIError> {
        // Default implementation calls step() multiple times
        let mut updates = Vec::new();
        for _ in 0..count {
            self.step(dt)?;
            updates.push(self.capture_state());
        }
        Ok(updates)
    }
}

pub struct PinInterface {
    pins: HashMap<String, PinHandle>,
}

impl PinInterface {
    pub fn read(&self, pin_name: &str) -> Result<f64, PLIError> {
        self.pins.get(pin_name)
            .map(|handle| handle.read())
            .ok_or_else(|| PLIError::UnknownPin(pin_name.to_string()))
    }
    
    pub fn write(&mut self, pin_name: &str, value: f64) -> Result<(), PLIError> {
        self.pins.get_mut(pin_name)
            .map(|handle| handle.write(value))
            .ok_or_else(|| PLIError::UnknownPin(pin_name.to_string()))
    }
}
```

#### 4.3 Language Bindings

**File**: `bhdl-pli/src/python.rs`

```rust
use pyo3::prelude::*;

#[pyclass]
struct PyBehavioralModel {
    #[pyo3(get)]
    name: String,
    pins: Py<PyDict>,
}

#[pymethods]
impl PyBehavioralModel {
    fn read_pin(&self, py: Python, name: &str) -> PyResult<f64> {
        let pins = self.pins.as_ref(py);
        pins.get_item(name)?
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyKeyError, _>(name))?
            .extract::<f64>()
    }
    
    fn write_pin(&self, py: Python, name: &str, value: f64) -> PyResult<()> {
        let pins = self.pins.as_ref(py);
        pins.set_item(name, value)?;
        Ok(())
    }
}

pub struct PythonModelWrapper {
    model: PyObject,
    gil: Python<'static>,
}

impl BehavioralModel for PythonModelWrapper {
    fn step(&mut self, dt: f64) -> Result<(), PLIError> {
        Python::with_gil(|py| {
            self.model.call_method1(py, "step", (dt,))
                .map_err(|e| PLIError::ModelError(e.to_string()))?;
            Ok(())
        })
    }
}
```

### Phase 5: Integration and Testing (Weeks 13-14)

#### 5.1 Test Framework

**File**: `bhdl-analyzer/tests/behavioral_tests.rs`

```rust
#[test]
fn test_attribute_expressions() {
    let source = r#"
        module Controller {
            pin FB: analog in;
            pin PWM: digital out;
            
            attribute error = 3.3V - FB;
            attribute duty = clamp(error * 0.1, 0.0, 1.0);
            
            PWM = duty;
        }
    "#;
    
    let ast = parse(source).unwrap();
    let analyzer = BehavioralAnalyzer::new();
    let result = analyzer.analyze(&ast);
    
    assert!(result.attribute_deps["duty"].contains("error"));
    assert!(result.attribute_deps["error"].contains("FB"));
}

#[test]
fn test_mutable_attribute_detection() {
    let source = r#"
        module SoftStart {
            attribute vref = 0V;
            
            when (enable) {
                vref += 0.1V * dt;
            }
        }
    "#;
    
    let result = analyze(source);
    assert!(result.mutable_attributes.contains("vref"));
}
```

#### 5.2 Example Circuits

**File**: `tests/circuits/behavioral/thermal_led.bhdl`

```bhdl
board ThermalLEDDemo {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Temperature sensor
    sensor: LM35 {
        VCC -> .VCC;
        GND -> .GND;
    }
    
    // Behavioral LED controller
    controller: ThermalLED {
        sensor.OUT -> .TEMP;
    }
    
    // LED with current control
    VCC -> LED(red).A;
    controller.LED_DRIVE -> LED.K;
    LED.K -> GND;
}

module ThermalLED {
    pin TEMP: analog in;
    pin LED_DRIVE: current out;
    
    // Temperature conversion
    attribute temp_c = TEMP / 10mV;
    
    // Thermal derating curve
    attribute i_max = if (temp_c < 25) { 350mA }
                     else if (temp_c < 85) { 350mA - (temp_c - 25) * 2mA }
                     else { 50mA };
    
    LED_DRIVE = i_max;
}
```

## Migration Guide

### For Existing BHDL Code

1. **Static attributes remain unchanged**:
   ```bhdl
   attribute title = "My Board";  // Still works
   ```

2. **New expression attributes**:
   ```bhdl
   attribute error = target - actual;  // NEW
   ```

3. **Time-varying attributes**:
   ```bhdl
   attribute vref = 0V;
   when (ramping) {
       vref += rate * dt;  // Makes vref mutable
   }
   ```

### For Complex Behavioral Models

1. **Create Python model**:
   ```python
   class BuckController(BehavioralModel):
       def step(self, dt):
           vin = self.read_pin("VIN")
           vout = self.calculate_output(vin)
           self.write_pin("VOUT", vout)
   ```

2. **Link in BHDL**:
   ```bhdl
   module Buck {
       @behavioral(model="buck.BuckController", language="python")
   }
   ```

## Performance Considerations

### Attribute Evaluation

- Use dependency graph for efficient evaluation order
- Cache intermediate results
- Only re-evaluate changed attributes

### PLI Optimization

- Batch processing by default (1000 timesteps)
- Shared memory for large data
- Lazy pin updates (only on change)

## Testing Strategy

### Unit Tests
- Parser: Expression parsing, decorator syntax
- Analyzer: Mutability detection, dependency analysis
- Simulator: Expression evaluation, when block execution

### Integration Tests
- Simple behavioral circuits (thermal, soft-start)
- PLI integration (Python models)
- Performance benchmarks

### Example Test Circuits
1. Thermal LED derating
2. Buck converter with soft-start
3. USB PD negotiation (PLI)
4. Motor control (PLI)

## Documentation Updates

### Language Reference
- Extended attribute syntax
- When block enhancements
- Decorator reference
- Built-in variables (dt)

### User Guide
- "Getting Started with Behavioral Modeling"
- "When to Use PLI vs Pure BHDL"
- "Writing Efficient Behavioral Models"
- "Debugging Behavioral Simulations"

## Success Metrics

1. **Simplicity**: Only 5 new concepts added
2. **Performance**: < 10% overhead for simple behavioral
3. **Compatibility**: 100% backward compatible
4. **Adoption**: Clear migration path from static to behavioral

## Timeline Summary

- **Weeks 1-3**: Extended attribute system
- **Weeks 4-5**: Time-based behavioral support
- **Weeks 6-8**: Behavioral simulation engine
- **Weeks 9-12**: PLI integration
- **Weeks 13-14**: Testing and documentation

Total: 14 weeks to full behavioral modeling support with minimal language complexity.