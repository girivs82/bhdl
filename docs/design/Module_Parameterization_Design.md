# Module Parameterization Design

## Overview

Module parameterization allows modules to adapt their behavior, structure, and connections based on parameters passed during instantiation. This is essential for creating flexible, reusable library modules.

## Parameterization Features

### 1. Value Parameters
```bhdl
// Module definition with parameters
module BuckConverter(
    vout: voltage = 3.3V,
    fsw: frequency = 500kHz,
    imax: current = 2A
) {
    pin VIN: power in;
    pin VOUT: power out;
    pin EN: digital in;
    
    // Use parameters in calculations
    attribute inductor_value = (VIN - vout) * vout / (VIN * fsw * imax * 0.3);
    
    // Component selection based on parameters
    L1: Inductor(inductor_value) { ... }
    
    // Conditional component inclusion
    when (imax > 1A) {
        // Parallel MOSFETs for high current
        Q1: MOSFET(IRLZ44N) { ... }
        Q2: MOSFET(IRLZ44N) { ... }
    } else {
        Q1: MOSFET(2N7002) { ... }
    }
}

// Instantiation with custom parameters
buck_5v: BuckConverter(vout=5V, imax=3A) { ... }
buck_3v3: BuckConverter() { ... }  // Uses defaults
```

### 2. Type Parameters
```bhdl
// Generic module with type parameters
module PowerStage<ControllerType, SwitchType>(
    vout: voltage,
    topology: string = "buck"
) {
    controller: ControllerType {
        // Controller-specific connections
    }
    
    switch: SwitchType {
        // Switch connections
    }
}

// Instantiation with specific types
stage: PowerStage<TPS54302, IRLZ44N>(vout=3.3V) { ... }
```

### 3. Conditional Structure
```bhdl
module FlexibleRegulator(
    vout: voltage,
    imax: current,
    efficiency_priority: bool = true
) {
    pin VIN: power in;
    pin VOUT: power out;
    
    // Choose topology based on requirements
    when (efficiency_priority && (VIN - vout) > 5V) {
        // Use switching regulator for efficiency
        reg: BuckConverter(vout=vout, imax=imax) {
            VIN -> .VIN;
            .VOUT -> VOUT;
        }
    } else {
        // Use LDO for simplicity/low noise
        reg: LDO_Regulator(vout=vout) {
            VIN -> .VIN;
            .VOUT -> VOUT;
        }
    }
}
```

### 4. Array/Generate Parameters
```bhdl
module ParallelRegulators(
    count: int = 2,
    vout: voltage = 3.3V,
    current_per_phase: current = 10A
) {
    pin VIN: power in;
    pin VOUT: power out;
    pin EN: digital in;
    
    // Generate parallel phases
    generate for i in 0..count {
        phase[i]: BuckPhase(
            vout=vout,
            imax=current_per_phase,
            phase_shift=360deg/count * i
        ) {
            VIN -> .VIN;
            .VOUT -> VOUT;  // All phases connect to same output
            .EN -> EN;
            .SYNC -> sync_bus;
        }
    }
    
    // Current balancing
    balancer: CurrentBalancer(phases=count) {
        generate for i in 0..count {
            phase[i].ISENSE -> .SENSE[i];
            .BALANCE[i] -> phase[i].IREF;
        }
    }
}
```

### 5. Computed Pins
```bhdl
module FlexibleInterface(
    data_width: int = 8,
    has_parity: bool = false,
    differential: bool = false
) {
    // Data pins array
    pin DATA[data_width]: signal inout;
    
    // Optional parity pin
    when (has_parity) {
        pin PARITY: signal inout;
    }
    
    // Differential pairs
    when (differential) {
        pin DATA_N[data_width]: signal inout;
        
        // Differential termination
        generate for i in 0..data_width {
            R_term[i]: Res(100) {
                DATA[i] -> .1;
                DATA_N[i] -> .2;
            }
        }
    }
}
```

## Implementation Updates

### 1. Parser Extensions

```rust
// In grammar.rs
fn parse_module_params(p: &mut Parser) {
    if !p.at(T!['(']) {
        return;
    }
    
    p.expect(T!['(']);
    while !p.at(T![')']) && !p.at(EOF) {
        parse_param_decl(p);  // name: type = default
        
        if !p.at(T![')']) {
            p.expect(T![,]);
        }
    }
    p.expect(T![')']);
}

fn parse_conditional_block(p: &mut Parser) {
    p.expect(T![when]);
    p.expect(T!['(']);
    parse_expression(p);  // Condition
    p.expect(T![')']);
    parse_block(p);       // Body
}

fn parse_generate_block(p: &mut Parser) {
    p.expect(T![generate]);
    if p.at(T![for]) {
        parse_generate_for(p);
    } else if p.at(T![if]) {
        parse_generate_if(p);
    }
}
```

### 2. AST Additions

```rust
// New AST nodes
#[derive(Debug, Clone)]
pub struct ModuleParam {
    pub name: String,
    pub param_type: TypeRef,
    pub default_value: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct ConditionalBlock {
    pub condition: Expr,
    pub body: Vec<ModuleStatement>,
}

#[derive(Debug, Clone)]
pub struct GenerateBlock {
    pub kind: GenerateKind,
    pub body: Vec<ModuleStatement>,
}

#[derive(Debug, Clone)]
pub enum GenerateKind {
    For {
        var: String,
        range: RangeExpr,
    },
    If {
        condition: Expr,
    },
}

// Extend Module
impl Module {
    pub fn params(&self) -> impl Iterator<Item = ModuleParam> { ... }
    pub fn conditional_blocks(&self) -> impl Iterator<Item = ConditionalBlock> { ... }
    pub fn generate_blocks(&self) -> impl Iterator<Item = GenerateBlock> { ... }
}
```

### 3. Analyzer Updates

```rust
// Parameter evaluation context
pub struct ParameterContext {
    values: HashMap<String, Value>,
}

// Module instantiation with parameters
pub struct ModuleInstantiation {
    module_def: Arc<Module>,
    param_values: HashMap<String, Value>,
    evaluated_body: Vec<EvaluatedStatement>,
}

impl Analyzer {
    fn instantiate_module(
        &mut self,
        module: &Module,
        params: &HashMap<String, Expr>,
    ) -> Result<ModuleInstantiation> {
        // 1. Evaluate parameter expressions
        let param_ctx = self.evaluate_params(module, params)?;
        
        // 2. Evaluate conditional blocks
        let body = self.evaluate_conditional_structure(module, &param_ctx)?;
        
        // 3. Expand generate blocks
        let expanded = self.expand_generates(body, &param_ctx)?;
        
        Ok(ModuleInstantiation {
            module_def: Arc::new(module.clone()),
            param_values: param_ctx.values,
            evaluated_body: expanded,
        })
    }
}
```

### 4. Synthesizer Updates

```rust
impl NetlistBuilder {
    fn synthesize_module_instance(
        &mut self,
        instance_name: &str,
        instantiation: &ModuleInstantiation,
    ) {
        // Use evaluated body instead of original module body
        for stmt in &instantiation.evaluated_body {
            match stmt {
                EvaluatedStatement::Instance(inst) => {
                    self.add_instance(inst);
                }
                EvaluatedStatement::Connection(conn) => {
                    self.add_connection(conn);
                }
                // ... other cases
            }
        }
    }
}
```

## Advanced Features

### 1. Parameter Constraints
```bhdl
module SafeBuck(
    vout: voltage = 3.3V where vout > 0.8V && vout < 5.5V,
    fsw: frequency = 500kHz where fsw >= 100kHz && fsw <= 2MHz
) {
    // Analyzer validates constraints at instantiation
}
```

### 2. Computed Types
```bhdl
module AdaptiveFilter(
    order: int = 2,
    topology: string = "butterworth"
) {
    // Component type depends on parameter
    attribute cap_type = order > 4 ? "C0G" : "X7R";
    
    generate for i in 0..order {
        C[i]: Cap(compute_value(i), type=cap_type) { ... }
    }
}
```

### 3. Parameter Inheritance
```bhdl
module ComplexSystem(
    base_voltage: voltage = 3.3V,
    optimization: string = "power"
) {
    // Child modules inherit parent parameters
    subsystem1: PowerSection(
        vcore = base_voltage * 0.8,
        optimization = optimization  // Inherited
    ) { ... }
}
```

### 4. Static Assertions
```bhdl
module VerifiedBuck(vout: voltage, vin: voltage) {
    // Compile-time checks
    static_assert(vin > vout * 1.2, "Input voltage too low for buck topology");
    static_assert(vout >= 0.6V, "Output below reference voltage");
}
```

## Examples

### Parameterized LED Driver
```bhdl
module LEDDriver(
    num_channels: int = 4,
    max_current: current = 350mA,
    dimming: bool = true
) {
    pin VIN: power in;
    pin LED_P[num_channels]: current out;
    pin LED_N[num_channels]: current in;
    
    when (dimming) {
        pin PWM[num_channels]: digital in;
    }
    
    generate for i in 0..num_channels {
        driver[i]: ConstantCurrentSink(imax=max_current) {
            .OUT -> LED_N[i];
            when (dimming) {
                PWM[i] -> .DIM;
            }
        }
        
        // LED connection
        LED_P[i] -> LED[i]: LED(white).A;
        LED[i].K -> LED_N[i];
    }
}

// Usage
rgb_driver: LEDDriver(num_channels=3, dimming=true) { ... }
white_driver: LEDDriver(num_channels=1, max_current=700mA) { ... }
```

### Configurable Filter
```bhdl
module EMIFilter(
    topology: string = "pi",
    cutoff: frequency = 1MHz,
    impedance: resistance = 50
) {
    pin IN: signal in;
    pin OUT: signal out;
    pin GND: ground;
    
    // Calculate component values
    attribute L_value = impedance / (2 * π * cutoff);
    attribute C_value = 1 / (2 * π * cutoff * impedance);
    
    when (topology == "pi") {
        // Pi filter
        IN -> C1: Cap(C_value).1 -> GND;
        IN -> L1: Inductor(L_value).1;
        L1.2 -> OUT;
        OUT -> C2: Cap(C_value).1 -> GND;
    } else when (topology == "T") {
        // T filter  
        IN -> L1: Inductor(L_value/2).1;
        L1.2 -> mid;
        mid -> C1: Cap(C_value).1 -> GND;
        mid -> L2: Inductor(L_value/2).1;
        L2.2 -> OUT;
    }
}
```

## Benefits

1. **True Reusability**: One module definition, many configurations
2. **Design Space Exploration**: Easy to try different parameters
3. **Validation**: Constraints ensure valid configurations
4. **Library Development**: Create comprehensive component libraries
5. **Reduced Duplication**: No need for multiple similar modules

## Testing Strategy

1. **Parameter Evaluation**: Test default values, overrides, expressions
2. **Conditional Structure**: Test when blocks with various conditions
3. **Generate Expansion**: Test loops and conditional generation
4. **Constraint Validation**: Test parameter constraints
5. **Nested Parameters**: Test parameter passing to child modules