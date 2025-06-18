# Electrical Safety Analysis Implementation

## Overview

The BHDL electrical safety analysis system provides comprehensive safety checking for circuits by analyzing actual electrical conditions against component ratings. The system is fully generic and data-driven, using real component specifications rather than hardcoded values.

## Key Design Principles

1. **Data-Driven**: All component limits come from actual component models, not hardcoded values
2. **Analysis-Based**: Safety checks are based on actual DC analysis results (currents, voltages) rather than circuit topology assumptions
3. **Generic**: Works for any component type - resistors, capacitors, ICs, LEDs, etc.
4. **Severity-Based**: Uses a 4-level severity system (Info, Warning, Error, Critical)
5. **Actionable**: Provides specific fixes for violations when possible

## Architecture

### Safety Analysis Flow

```
BHDL Circuit → DC Analysis → Safety Analysis → Violations & Fixes
     ↓              ↓              ↓
Component      Actual         Check against
Models         V & I          Limits
```

### Core Components

1. **Safety Engine** (`bhdl-spice/src/safety/engine.rs`)
   - Orchestrates safety rules
   - Manages configuration (derating factors, enabled rules)
   - Generates safety analysis results

2. **Safety Rules** (`bhdl-spice/src/safety/rules/`)
   - `CurrentLimitingRule`: Checks if components exceed current ratings
   - `OvervoltageRule`: Checks if components exceed voltage ratings
   - `ShortCircuitRule`: Detects abnormally high currents indicating shorts

3. **Circuit Integration** (`bhdl-spice/src/circuit.rs`)
   - `Branch` struct enhanced with `limits: Option<ElectricalLimits>`
   - Limits populated from component models during DC analysis setup

4. **DC Analysis Integration** (`bhdl-spice/src/analysis.rs`)
   - `add_model()` propagates component limits to circuit branches
   - Analysis results provide actual currents and voltages for safety checks

## Implementation Details

### Component Limits Propagation

When adding a component model to DC analysis:

```rust
pub fn add_model(&mut self, component_name: String, model: ComponentModel) {
    // Extract limits from model
    let limits = model.limits();
    
    // Only set if we have actual limits (not DEFAULT_LIMITS)
    if limits.max_voltage.is_some() || limits.max_current.is_some() || limits.max_power.is_some() {
        self.circuit.set_branch_limits(&component_name, limits.clone());
    }
    
    self.models.insert(component_name, model);
}
```

### Safety Rule Implementation

Each safety rule implements the `SafetyRule` trait:

```rust
pub trait SafetyRule: Send + Sync {
    fn name(&self) -> &str;
    fn default_severity(&self) -> Severity;
    fn priority(&self) -> u32;
    fn check(&self, circuit: &Circuit, dc_result: Option<&AnalysisResult>) -> Vec<SafetyViolation>;
    fn can_auto_fix(&self) -> bool;
    fn suggest_fix(&self, violation: &SafetyViolation, circuit: &Circuit) -> Option<CircuitModification>;
}
```

Rules only run when DC analysis results are available:

```rust
fn check(&self, circuit: &Circuit, dc_result: Option<&AnalysisResult>) -> Vec<SafetyViolation> {
    let mut violations = Vec::new();
    
    // Without DC analysis results, we can't check safety
    if dc_result.is_none() {
        return violations;
    }
    
    let result = dc_result.unwrap();
    
    // Check each component against its limits...
}
```

### Derating Factors

The system applies conservative derating factors:

```rust
pub struct DeratingFactors {
    pub voltage: f64,      // 0.8 = use 80% of max
    pub current: f64,      // 0.7 = use 70% of max  
    pub power: f64,        // 0.5 = use 50% of max
    pub temperature: f64,  // 0.8 = use 80% of max
}
```

## Integration with BHDL Pipeline

### Analyzer Integration (Pass 8)

Add safety analysis as the final pass in the analyzer:

```rust
// In bhdl-analyzer/src/lib.rs
pub fn analyze(ast: &SourceFile) -> AnalysisResult {
    // ... existing passes ...
    
    // Pass 7: Synthesize netlist
    let netlist = synthesizer.generate(&ast, &symbol_table)?;
    
    // Pass 8: Electrical Safety Analysis
    if let Some(safety_violations) = run_safety_analysis(&netlist, &analysis_result) {
        diagnostics.extend(safety_violations);
    }
    
    // Return results
    AnalysisResult {
        diagnostics,
        symbol_table,
        netlist,
        // ...
    }
}

fn run_safety_analysis(
    netlist: &Netlist, 
    analysis: &AnalysisResult
) -> Option<Vec<Diagnostic>> {
    // Convert netlist to SPICE circuit
    let circuit = Circuit::from_netlist(netlist)?;
    
    // Run DC analysis
    let mut dc_analysis = DcAnalysis::new(circuit);
    
    // Add component models from symbol table or component database
    for (instance_id, instance) in &netlist.instances {
        if let Some(model) = get_component_model(instance) {
            dc_analysis.add_model(instance.name.clone(), model);
        }
    }
    
    // Run DC analysis
    let dc_result = dc_analysis.analyze().ok()?;
    
    // Run safety analysis
    let config = SafetyConfig::default();
    let engine = SafetyAnalysisEngine::new(config);
    let safety_result = engine.analyze(dc_analysis.circuit(), Some(&dc_result));
    
    // Convert violations to diagnostics
    Some(violations_to_diagnostics(safety_result.violations))
}
```

### Example Usage

```rust
// Test program showing the complete flow
use bhdl_spice::{Circuit, DcAnalysis, ComponentModel, SafetyAnalysisEngine, SafetyConfig};

// Create circuit
let mut circuit = Circuit::new();
circuit.add_branch("D1", "VCC", "GND", "LED", 0.0, None);

// Set up DC analysis with component models
let mut dc = DcAnalysis::new(circuit);
dc.add_model("D1", ComponentModel::LED {
    color: "red".to_string(),
    forward_voltage: 2.0,
    forward_current: 0.02,
    dynamic_resistance: 10.0,
    limits: ElectricalLimits {
        max_current: Some(0.030),  // 30mA absolute max
        max_voltage: Some(3.3),
        max_power: Some(0.1),
        ..Default::default()
    },
});

// Run DC analysis
let dc_result = dc.analyze()?;

// Run safety analysis on the circuit with limits
let engine = SafetyAnalysisEngine::new(SafetyConfig::default());
let safety_result = engine.analyze(dc.circuit(), Some(&dc_result));

// Check violations
for violation in &safety_result.violations {
    println!("[{}] {}", violation.severity, violation.message);
}
```

## Safety Violations

### Violation Structure

```rust
pub struct SafetyViolation {
    pub rule_name: String,
    pub severity: Severity,
    pub location: CircuitLocation,
    pub message: String,
    pub technical_details: String,
    pub user_impact: String,
    pub estimated_damage: Option<DamageEstimate>,
}
```

### Example Violations

1. **Overcurrent (Critical)**
   ```
   LED 'D1' current 500.0mA exceeds absolute maximum 30.0mA
   Technical: Measured: 0.500A, Max: 0.030A, Overcurrent ratio: 16.7x
   Impact: Component will fail immediately
   Damage: Overcurrent failure - 10ms to failure
   ```

2. **Derating Warning**
   ```
   LED 'D1' current 21.7mA exceeds 70% derating limit of 21.0mA
   Technical: Measured: 0.022A, Derated max: 0.021A
   Impact: Reduced component lifetime and reliability
   ```

## Circuit Modifications

The system can suggest automatic fixes:

```rust
pub enum CircuitModification {
    InsertComponent {
        component_type: ComponentType,
        value: ComponentValue,
        from_node: NodeId,
        to_node: NodeId,
        new_node: Option<String>,
        reason: String,
    },
    ModifyComponentValue {
        instance: ComponentId,
        new_value: ComponentValue,
        old_value: ComponentValue,
        reason: String,
    },
    AddProtectionCircuit {
        protection_type: ProtectionType,
        target: ProtectionTarget,
        specifications: HashMap<String, f64>,
        reason: String,
    },
}
```

## Testing

### Unit Tests

```rust
#[test]
fn test_led_overcurrent_detection() {
    // Create circuit with LED directly connected to 5V
    let circuit = create_dangerous_led_circuit();
    
    // Run DC analysis
    let dc_result = run_dc_analysis(&circuit);
    
    // Run safety analysis
    let violations = safety_engine.analyze(&circuit, Some(&dc_result)).violations;
    
    // Should detect overcurrent
    assert!(violations.iter().any(|v| {
        v.severity == Severity::Critical &&
        v.message.contains("exceeds absolute maximum")
    }));
}
```

### Integration Test

See `bhdl-spice/src/bin/test_safety_with_dc.rs` for a complete example.

## Benefits

1. **Prevents Component Damage**: Catches overcurrent/overvoltage before fabrication
2. **Ensures Reliability**: Derating warnings improve long-term reliability
3. **Cost Savings**: Prevents expensive component failures
4. **Design Validation**: Verifies design meets all electrical constraints
5. **Automatic Fixes**: Can suggest current limiting resistors, protection diodes, etc.

## Future Enhancements

1. **Thermal Analysis**: Check power dissipation and thermal limits
2. **Transient Analysis**: Check for voltage spikes and inrush currents
3. **EMI/EMC Checks**: Verify emissions and susceptibility
4. **Component Matching**: Suggest optimal components from database
5. **Multi-Domain**: Extend to mechanical stress, vibration limits, etc.