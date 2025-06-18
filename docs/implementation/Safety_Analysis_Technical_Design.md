# Electrical Safety Analysis - Technical Design

## Core Data Structures

### 1. Safety Rule Framework

```rust
// bhdl-spice/src/safety/mod.rs

use std::collections::HashMap;
use crate::{Circuit, Node, Component, AnalysisResult};

/// Severity levels for safety violations
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,       // Suggestion for improvement
    Warning,    // Potential issue, but not immediate danger
    Error,      // Likely to cause problems
    Critical,   // Will cause component damage or safety hazard
}

/// Location in circuit where violation occurs
#[derive(Debug, Clone)]
pub struct CircuitLocation {
    pub nodes: Vec<NodeId>,
    pub components: Vec<InstanceId>,
    pub nets: Vec<String>,
    pub description: String,
}

/// A safety violation found in the circuit
#[derive(Debug)]
pub struct SafetyViolation {
    pub rule_name: String,
    pub severity: Severity,
    pub location: CircuitLocation,
    pub message: String,
    pub technical_details: String,
    pub user_impact: String,
    pub estimated_damage: Option<DamageEstimate>,
}

#[derive(Debug)]
pub struct DamageEstimate {
    pub failure_mode: String,
    pub time_to_failure: Option<Duration>,
    pub affected_components: Vec<String>,
    pub estimated_cost: Option<f64>,
}

/// Base trait for all safety rules
pub trait SafetyRule: Send + Sync {
    /// Unique name for this rule
    fn name(&self) -> &str;
    
    /// Default severity if violations are found
    fn default_severity(&self) -> Severity;
    
    /// Check the circuit for violations of this rule
    fn check(&self, circuit: &Circuit, dc_result: Option<&AnalysisResult>) -> Vec<SafetyViolation>;
    
    /// Whether this rule can suggest automatic fixes
    fn can_auto_fix(&self) -> bool { false }
    
    /// Suggest fixes for violations
    fn suggest_fix(&self, violation: &SafetyViolation, circuit: &Circuit) -> Option<CircuitModification> {
        None
    }
    
    /// Priority for rule execution (higher = earlier)
    fn priority(&self) -> u32 { 100 }
}
```

### 2. Circuit Modifications

```rust
// bhdl-spice/src/safety/modifications.rs

/// Represents a modification to fix a safety issue
#[derive(Debug, Clone)]
pub enum CircuitModification {
    /// Insert a new component between two nodes
    InsertComponent {
        component_type: ComponentType,
        value: ComponentValue,
        from_node: NodeId,
        to_node: NodeId,
        new_node: Option<String>, // Optional intermediate node name
        reason: String,
    },
    
    /// Modify an existing component's value
    ModifyComponentValue {
        instance: InstanceId,
        new_value: ComponentValue,
        old_value: ComponentValue,
        reason: String,
    },
    
    /// Add a protection circuit
    AddProtectionCircuit {
        protection_type: ProtectionType,
        target: ProtectionTarget,
        specifications: HashMap<String, f64>,
        reason: String,
    },
    
    /// Split a node to insert protection
    SplitNode {
        original_node: NodeId,
        new_node_name: String,
        insert_between: ComponentType,
        reason: String,
    },
    
    /// Add parallel component (e.g., decoupling cap)
    AddParallelComponent {
        component_type: ComponentType,
        value: ComponentValue,
        node1: NodeId,
        node2: NodeId,
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub enum ComponentType {
    Resistor,
    Capacitor,
    Inductor,
    Diode,
    TVSDidoe,
    Fuse,
    PTC,
    GasDischareTube,
    Custom(String),
}

#[derive(Debug, Clone)]
pub enum ProtectionType {
    OvercurrentProtection,
    OvervoltageProtection,
    ReverseVoltageProtection,
    ESDProtection,
    InrushLimiting,
    FlybackDiode,
}
```

### 3. Specific Safety Rules

```rust
// bhdl-spice/src/safety/rules/current_limiting.rs

pub struct CurrentLimitingRule {
    derating_factor: f64,  // e.g., 0.7 for 70% derating
}

impl SafetyRule for CurrentLimitingRule {
    fn name(&self) -> &str { "Current Limiting Check" }
    
    fn default_severity(&self) -> Severity { Severity::Critical }
    
    fn check(&self, circuit: &Circuit, dc_result: Option<&AnalysisResult>) -> Vec<SafetyViolation> {
        let mut violations = Vec::new();
        
        // Check each component
        for (id, component) in circuit.components() {
            if let Some(max_current) = component.max_current() {
                // Get current through component from DC analysis
                if let Some(result) = dc_result {
                    if let Some(&current) = result.get_current(id) {
                        let derated_max = max_current * self.derating_factor;
                        
                        if current.abs() > max_current {
                            violations.push(SafetyViolation {
                                rule_name: self.name().to_string(),
                                severity: Severity::Critical,
                                location: CircuitLocation {
                                    components: vec![id],
                                    nodes: component.nodes().to_vec(),
                                    nets: vec![],
                                    description: format!("Component {}", component.name()),
                                },
                                message: format!(
                                    "{} current {:.1}mA exceeds maximum {:.1}mA",
                                    component.name(),
                                    current.abs() * 1000.0,
                                    max_current * 1000.0
                                ),
                                technical_details: format!(
                                    "Current: {:.3}A, Max: {:.3}A, Ratio: {:.1}x",
                                    current.abs(),
                                    max_current,
                                    current.abs() / max_current
                                ),
                                user_impact: "Component will be destroyed immediately".to_string(),
                                estimated_damage: Some(DamageEstimate {
                                    failure_mode: "Overcurrent burnout".to_string(),
                                    time_to_failure: Some(Duration::from_millis(10)),
                                    affected_components: vec![component.name().to_string()],
                                    estimated_cost: component.cost(),
                                }),
                            });
                        } else if current.abs() > derated_max {
                            violations.push(SafetyViolation {
                                rule_name: self.name().to_string(),
                                severity: Severity::Warning,
                                location: CircuitLocation {
                                    components: vec![id],
                                    nodes: component.nodes().to_vec(),
                                    nets: vec![],
                                    description: format!("Component {}", component.name()),
                                },
                                message: format!(
                                    "{} current {:.1}mA exceeds derated maximum {:.1}mA",
                                    component.name(),
                                    current.abs() * 1000.0,
                                    derated_max * 1000.0
                                ),
                                technical_details: format!(
                                    "Current: {:.3}A, Derated Max: {:.3}A ({}% derating)",
                                    current.abs(),
                                    derated_max,
                                    (self.derating_factor * 100.0) as u32
                                ),
                                user_impact: "Reduced component lifetime and reliability".to_string(),
                                estimated_damage: None,
                            });
                        }
                    }
                }
                
                // Also check for missing current limiting
                if component.needs_current_limiting() && !has_current_limiting(circuit, id) {
                    violations.push(create_missing_current_limit_violation(component, id));
                }
            }
        }
        
        violations
    }
    
    fn can_auto_fix(&self) -> bool { true }
    
    fn suggest_fix(&self, violation: &SafetyViolation, circuit: &Circuit) -> Option<CircuitModification> {
        // Calculate appropriate current limiting resistor
        if violation.message.contains("current limiting") {
            // Extract component info and calculate resistor
            Some(calculate_current_limiting_resistor(violation, circuit))
        } else {
            None
        }
    }
}
```

### 4. Safety Analysis Engine

```rust
// bhdl-spice/src/safety/engine.rs

pub struct SafetyAnalysisEngine {
    rules: Vec<Box<dyn SafetyRule>>,
    config: SafetyConfig,
}

#[derive(Debug, Clone)]
pub struct SafetyConfig {
    pub enabled: bool,
    pub auto_fix: bool,
    pub severity_threshold: Severity,
    pub derating_factors: DeratingFactors,
    pub excluded_rules: HashSet<String>,
    pub custom_limits: HashMap<String, f64>,
}

#[derive(Debug, Clone)]
pub struct DeratingFactors {
    pub voltage: f64,      // 0.8 = use 80% of max
    pub current: f64,      // 0.7 = use 70% of max  
    pub power: f64,        // 0.5 = use 50% of max
    pub temperature: f64,  // 0.8 = use 80% of max
}

impl SafetyAnalysisEngine {
    pub fn new(config: SafetyConfig) -> Self {
        let mut rules: Vec<Box<dyn SafetyRule>> = vec![
            Box::new(CurrentLimitingRule::new(config.derating_factors.current)),
            Box::new(VoltageRatingRule::new(config.derating_factors.voltage)),
            Box::new(PowerDissipationRule::new(config.derating_factors.power)),
            Box::new(ProtectionCircuitRule::new()),
            Box::new(DecouplingCapacitorRule::new()),
            Box::new(PullResistorRule::new()),
            Box::new(GateProtectionRule::new()),
            Box::new(InductiveLoadRule::new()),
        ];
        
        // Sort by priority
        rules.sort_by_key(|r| std::cmp::Reverse(r.priority()));
        
        Self { rules, config }
    }
    
    pub fn analyze(&self, circuit: &Circuit, dc_result: Option<&AnalysisResult>) -> SafetyAnalysisResult {
        let mut all_violations = Vec::new();
        let mut suggested_fixes = Vec::new();
        
        // Run each rule
        for rule in &self.rules {
            if self.config.excluded_rules.contains(rule.name()) {
                continue;
            }
            
            let violations = rule.check(circuit, dc_result);
            
            // Generate fixes for violations above threshold
            for violation in &violations {
                if violation.severity >= self.config.severity_threshold {
                    if let Some(fix) = rule.suggest_fix(violation, circuit) {
                        suggested_fixes.push((violation.clone(), fix));
                    }
                }
            }
            
            all_violations.extend(violations);
        }
        
        // Sort violations by severity
        all_violations.sort_by_key(|v| std::cmp::Reverse(v.severity));
        
        SafetyAnalysisResult {
            violations: all_violations,
            suggested_fixes,
            summary: generate_summary(&all_violations),
        }
    }
}
```

### 5. Integration with Analyzer

```rust
// bhdl-analyzer/src/lib.rs (additions)

// Pass 8: Electrical Safety Analysis
println!("Analyzer: Starting Pass 8 - Electrical Safety Analysis...");

// Create safety analysis engine
let safety_config = SafetyConfig::default(); // Or load from user config
let safety_engine = SafetyAnalysisEngine::new(safety_config);

// Get the circuit from synthesizer results
if let Some(circuit) = build_circuit_from_netlist(&netlist) {
    // Run DC analysis first if needed
    let dc_result = if safety_engine.needs_dc_analysis() {
        run_dc_analysis(&circuit)?
    } else {
        None
    };
    
    // Run safety analysis
    let safety_result = safety_engine.analyze(&circuit, dc_result.as_ref());
    
    // Convert violations to diagnostics
    for violation in &safety_result.violations {
        let diagnostic = Diagnostic {
            message: format!("{}: {}", violation.severity, violation.message),
            severity: map_severity(violation.severity),
            range: find_ast_range(&violation.location),
            fix_hint: violation.suggested_fix.as_ref().map(|f| f.description()),
        };
        diagnostics.push(diagnostic);
    }
    
    // Store safety analysis results
    analysis_result.safety_analysis = Some(safety_result);
}

println!("Analyzer: Pass 8 complete. Safety violations: {}", 
    safety_result.violations.len());
```

## Example Implementation Details

### LED Current Limiting Detection

```rust
fn has_current_limiting(circuit: &Circuit, component_id: InstanceId) -> bool {
    // Check if component has any series resistance
    let component = circuit.get_component(component_id);
    
    // Trace from component pins to find series elements
    for pin in component.pins() {
        if let Some(path) = trace_to_supply(circuit, pin) {
            // Check if path has resistive elements
            let has_resistance = path.iter().any(|elem| {
                matches!(elem.component_type(), ComponentType::Resistor) ||
                elem.has_significant_resistance()
            });
            
            if !has_resistance {
                return false;
            }
        }
    }
    
    true
}

fn calculate_current_limiting_resistor(
    violation: &SafetyViolation,
    circuit: &Circuit
) -> CircuitModification {
    // Extract component and operating conditions
    let component_id = &violation.location.components[0];
    let component = circuit.get_component(*component_id);
    
    // Get supply voltage
    let supply_voltage = circuit.get_supply_voltage();
    
    // Calculate required resistance
    let required_resistance = match component.component_type() {
        ComponentType::LED(color) => {
            let vf = get_led_forward_voltage(color);
            let target_current = 0.015; // 15mA typical
            (supply_voltage - vf) / target_current
        }
        _ => {
            // Generic calculation
            let max_current = component.max_current().unwrap_or(0.1);
            supply_voltage / (max_current * 0.7) // 70% derating
        }
    };
    
    // Round to E12 series
    let resistance = round_to_e12(required_resistance);
    
    CircuitModification::InsertComponent {
        component_type: ComponentType::Resistor,
        value: ComponentValue::Resistance(resistance),
        from_node: violation.location.nodes[0],
        to_node: violation.location.nodes[1],
        new_node: Some(format!("{}_protected", component.name())),
        reason: format!(
            "Add {}Ω current limiting resistor for {}",
            resistance, component.name()
        ),
    }
}
```

## Testing Strategy

### 1. Unit Tests for Each Rule
```rust
#[test]
fn test_current_limiting_rule() {
    let mut circuit = Circuit::new();
    
    // Add LED without resistor
    circuit.add_component("LED1", ComponentType::LED("red"), &["VCC", "GND"]);
    
    let rule = CurrentLimitingRule::new(0.7);
    let violations = rule.check(&circuit, None);
    
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].severity, Severity::Critical);
    assert!(violations[0].message.contains("current limiting"));
}
```

### 2. Integration Tests
```rust
#[test]
fn test_complete_safety_analysis() {
    let bhdl_source = r#"
        board TestBoard {
            power VCC = 5V @ 1A;
            ground GND;
            
            // Dangerous: LED without resistor
            VCC -> LED1: LED(red).A;
            LED1.K -> GND;
            
            // Safe: LED with resistor
            VCC -> R1: Res(330).1 -> LED2: LED(green).A;
            LED2.K -> GND;
        }
    "#;
    
    let analysis_result = analyze_with_safety(bhdl_source);
    
    assert!(analysis_result.has_safety_violations());
    assert_eq!(analysis_result.critical_violations().len(), 1);
}
```

## Configuration File Format

```toml
# .bhdl-safety.toml

[safety]
enabled = true
auto_fix = false
severity_threshold = "warning"

[safety.derating]
voltage = 0.8
current = 0.7
power = 0.5
temperature = 0.8

[safety.rules.current_limiting]
enabled = true
led_current = 0.015  # 15mA default for LEDs

[safety.rules.voltage_rating]
enabled = true
include_transients = true

[safety.custom_limits]
"MCU.max_current" = 0.2  # 200mA max for MCU
"LED.max_pulse_current" = 0.1  # 100mA pulse

[safety.exclude]
rules = []
components = ["TEST_*"]  # Ignore test points
```