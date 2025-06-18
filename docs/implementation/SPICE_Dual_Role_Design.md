# SPICE Dual Role Design: Advisory & Generative

## Overview

The SPICE crate should operate in two modes:
1. **Advisory Mode**: When component values are specified, validate and warn about issues
2. **Generative Mode**: When component values are omitted, calculate appropriate values

## Design Principles

### 1. Explicit Intent in BHDL Syntax

```bhdl
// Generative mode - SPICE calculates the value
VCC -> r_led: Res().1 -> led: LED("red").A;

// Advisory mode - SPICE validates the value  
VCC -> r_led: Res(100).1 -> led: LED("red").A;

// Mixed mode with constraints
VCC -> r_led: Res(?, tolerance=5%, power=0.25W).1 -> led: LED("red").A;
```

### 2. Component Parameter States

```rust
pub enum ParameterState<T> {
    Specified(T),           // User provided value - validate only
    Inferred(T),           // SPICE calculated value
    Constrained {          // User provided constraints
        min: Option<T>,
        max: Option<T>,
        preferred: Option<Vec<T>>,
    },
}
```

## Implementation Requirements

### 1. Parser Changes

Update parser to handle empty parameter lists and placeholder syntax:

```rust
// In bhdl-parser/src/expressions.rs
fn parse_component_parameters(&mut self) {
    self.expect(SyntaxKind::L_PAREN);
    
    if self.peek() == Some(SyntaxKind::R_PAREN) {
        // Empty parameters - generative mode
        self.builder.start_node(SyntaxKind::PARAM_PLACEHOLDER.into());
        self.builder.finish_node();
    } else if self.peek() == Some(SyntaxKind::QUESTION) {
        // Explicit placeholder: Res(?)
        self.bump(); // consume ?
        // Parse optional constraints
        self.parse_constraints();
    } else {
        // Normal parameters
        self.parse_param_list();
    }
    
    self.expect(SyntaxKind::R_PAREN);
}
```

### 2. AST Changes

Add parameter state tracking:

```rust
// In bhdl-ast/src/common.rs
#[derive(Debug, Clone)]
pub enum ComponentParam {
    Value(Value),
    Placeholder,
    ConstrainedPlaceholder {
        constraints: Vec<Constraint>,
    },
}

#[derive(Debug, Clone)]
pub struct Constraint {
    pub name: String,
    pub value: Value,
}
```

### 3. Analyzer Enhancement

During Pass 6 (Component Inference), mark components for SPICE resolution:

```rust
// In bhdl-analyzer/src/component_inference.rs
pub struct ComponentSuggestion {
    pub component_type: String,
    pub instance_name: Option<String>,
    pub parameters: Vec<InferredParameter>,
    pub resolution_mode: ResolutionMode, // NEW
    pub reasoning: String,
    pub confidence: f64,
}

pub enum ResolutionMode {
    UserSpecified,    // Validate only
    SpiceGenerate,    // Calculate value
    SpiceConstrained, // Calculate within constraints
}
```

### 4. SPICE Integration Layer

Create a new module for SPICE-driven synthesis:

```rust
// In bhdl-analyzer/src/spice_synthesis.rs
use bhdl_spice::{Circuit, NonlinearDcAnalysis, ComponentModel};
use bhdl_netlist::Netlist;

pub struct SpiceSynthesis {
    netlist: Netlist,
    circuit: Circuit,
    unresolved_components: Vec<UnresolvedComponent>,
}

pub struct UnresolvedComponent {
    instance_id: InstanceId,
    component_type: String,
    constraints: ComponentConstraints,
    connections: Vec<NetId>,
}

impl SpiceSynthesis {
    /// Resolve all unspecified component values
    pub fn resolve_components(&mut self) -> Result<ResolutionReport> {
        let mut report = ResolutionReport::new();
        
        // Sort by dependency order
        let ordered = self.topological_sort()?;
        
        for component in ordered {
            match component.component_type.as_str() {
                "Res" => self.resolve_resistor(&component, &mut report)?,
                "Cap" => self.resolve_capacitor(&component, &mut report)?,
                "Ind" => self.resolve_inductor(&component, &mut report)?,
                _ => {
                    report.add_warning(format!(
                        "Cannot auto-resolve {} type", 
                        component.component_type
                    ));
                }
            }
        }
        
        Ok(report)
    }
    
    fn resolve_resistor(
        &mut self, 
        component: &UnresolvedComponent,
        report: &mut ResolutionReport
    ) -> Result<()> {
        // Determine circuit context
        let context = self.analyze_context(&component)?;
        
        match context {
            CircuitContext::LEDCurrentLimit { led_id, led_spec } => {
                // Calculate: R = (Vsupply - Vf) / If
                let vsupply = self.get_supply_voltage(&component)?;
                let vf = led_spec.forward_voltage;
                let if_target = led_spec.target_current;
                
                let r_calculated = (vsupply - vf) / if_target;
                let r_standard = find_nearest_e_series(r_calculated, 12);
                
                // Verify the standard value works
                let actual_current = (vsupply - vf) / r_standard;
                if actual_current > led_spec.max_current {
                    // Try next higher E-series value
                    let r_standard = find_next_higher_e_series(r_calculated, 12);
                }
                
                report.add_resolution(Resolution {
                    component: component.instance_id,
                    parameter: "value".to_string(),
                    calculated_value: r_calculated,
                    selected_value: r_standard,
                    reasoning: format!(
                        "LED current limiting: ({:.1}V - {:.1}V) / {:.3}A = {:.0}Ω",
                        vsupply, vf, if_target, r_calculated
                    ),
                });
                
                // Update circuit model
                self.circuit.set_branch_value(&component.name, r_standard);
            }
            
            CircuitContext::VoltageDivider { ratio, load } => {
                // Calculate based on ratio and load requirements
                // ...
            }
            
            CircuitContext::PullUp { logic_levels, speed } => {
                // Calculate based on logic thresholds and rise time
                // ...
            }
            
            _ => {
                report.add_error(format!(
                    "Cannot determine context for resistor {}",
                    component.name
                ));
            }
        }
        
        Ok(())
    }
}
```

### 5. Validation Mode

When values are specified, run comprehensive checks:

```rust
// In bhdl-spice/src/validation.rs
pub struct CircuitValidator {
    circuit: Circuit,
    models: HashMap<String, ComponentModel>,
    rules: ValidationRules,
}

impl CircuitValidator {
    pub fn validate(&mut self) -> ValidationReport {
        let mut report = ValidationReport::new();
        
        // Run DC analysis
        let dc_result = self.run_dc_analysis();
        
        // Check each component
        for (name, model) in &self.models {
            self.check_operating_point(name, model, &dc_result, &mut report);
            self.check_thermal_limits(name, model, &dc_result, &mut report);
            self.check_derating(name, model, &dc_result, &mut report);
        }
        
        // Check system-level constraints
        self.check_power_budget(&dc_result, &mut report);
        self.check_efficiency(&dc_result, &mut report);
        
        report
    }
    
    fn check_operating_point(
        &self,
        name: &str,
        model: &ComponentModel,
        dc_result: &AnalysisResult,
        report: &mut ValidationReport
    ) {
        match model {
            ComponentModel::LED { limits, forward_current, .. } => {
                let actual_current = self.get_branch_current(name, dc_result);
                
                if let Some(max_current) = limits.max_current {
                    if actual_current > max_current {
                        report.add_error(ValidationError {
                            component: name.to_string(),
                            error_type: ErrorType::Overcurrent,
                            message: format!(
                                "LED current {:.1}mA exceeds maximum {:.1}mA",
                                actual_current * 1000.0,
                                max_current * 1000.0
                            ),
                            severity: Severity::Critical,
                            suggestion: Some(format!(
                                "Increase series resistance to limit current"
                            )),
                        });
                    }
                }
                
                // Warn if far from optimal
                let optimal_ratio = actual_current / forward_current;
                if optimal_ratio < 0.5 || optimal_ratio > 1.5 {
                    report.add_warning(ValidationWarning {
                        component: name.to_string(),
                        message: format!(
                            "LED operating at {:.0}% of optimal current",
                            optimal_ratio * 100.0
                        ),
                    });
                }
            }
            // ... other component types
        }
    }
}
```

### 6. Integration with Analyzer

Modify the analyzer to invoke SPICE in both modes:

```rust
// In bhdl-analyzer/src/lib.rs - Pass 6
fn run_component_inference_pass(
    &mut self,
    ast: &SourceFile,
    power_context: &PowerAnalysisContext,
) -> Result<()> {
    // First pass: identify components needing resolution
    let unresolved = self.identify_unresolved_components(ast)?;
    
    if !unresolved.is_empty() {
        // Convert to SPICE circuit
        let circuit = self.build_spice_circuit()?;
        
        // Run SPICE synthesis
        let mut spice_synthesis = SpiceSynthesis::new(circuit);
        let resolution_report = spice_synthesis.resolve_components()?;
        
        // Apply resolutions
        for resolution in resolution_report.resolutions {
            self.apply_resolution(resolution)?;
        }
    }
    
    // Run validation on all components (specified + resolved)
    let mut validator = CircuitValidator::new(self.get_complete_circuit()?);
    let validation_report = validator.validate();
    
    // Convert to diagnostics
    for error in validation_report.errors {
        self.diagnostics.push(Diagnostic {
            message: error.message,
            severity: DiagnosticSeverity::Error,
            range: self.get_component_range(&error.component),
        });
    }
    
    Ok(())
}
```

## Usage Examples

### Example 1: LED Circuit with Auto-calculation

```bhdl
board AutoLED {
    power VCC = 5V @ 500mA;
    ground GND;
    
    // SPICE calculates appropriate resistor value
    VCC -> r1: Res().1 -> led: LED("red", 20mA).A;
    led.K -> GND;
}
```

SPICE would:
1. Identify `r1` has no value specified
2. Detect it's in series with an LED
3. Calculate: R = (5V - 2V) / 0.02A = 150Ω
4. Select nearest E12 value: 150Ω
5. Verify current is safe: I = 3V / 150Ω = 20mA ✓

### Example 2: Validation with Warnings

```bhdl
board ManualLED {
    power VCC = 5V @ 500mA;
    ground GND;
    
    // User specified 47Ω - too low!
    VCC -> r1: Res(47).1 -> led: LED("red", 20mA).A;
    led.K -> GND;
}
```

SPICE would:
1. Calculate actual current: I = (5V - 2V) / 47Ω = 63.8mA
2. Generate ERROR: "LED current 63.8mA exceeds maximum 30mA"
3. Suggest: "Minimum resistance needed: 100Ω"

### Example 3: Constrained Auto-calculation

```bhdl
board ConstrainedDesign {
    power VCC = 12V @ 1A;
    ground GND;
    
    // SPICE picks value within constraints
    VCC -> r1: Res(?, power=0.5W, tolerance=1%).1 -> Load(100mA);
}
```

SPICE would:
1. Calculate: R = 12V / 0.1A = 120Ω
2. Check power: P = 0.1² × 120 = 1.2W > 0.5W limit
3. Adjust for power limit: R = 0.5W / 0.1² = 50Ω max
4. Warning: "Resistor limited by power constraint, load current will be 240mA"

## Benefits

1. **Rapid Prototyping**: Designers can sketch circuits without calculating every value
2. **Safety**: Automatic detection of component stress
3. **Optimization**: SPICE can optimize for efficiency, cost, or other metrics
4. **Learning**: New designers learn proper values from SPICE suggestions
5. **Verification**: Experienced designers get automatic checking

## Implementation Priority

1. **Phase 1**: Basic resistor calculation for LEDs and voltage dividers
2. **Phase 2**: Validation and warning system
3. **Phase 3**: Capacitor sizing (decoupling, timing)
4. **Phase 4**: Complex optimization (efficiency, thermal)
5. **Phase 5**: Machine learning from validated designs