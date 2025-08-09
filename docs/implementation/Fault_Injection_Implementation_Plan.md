# Fault Injection Implementation Plan

## Overview

This document outlines the implementation plan for comprehensive fault injection capabilities in BHDL testbenches, integrated with the safety analysis module for cascade failure detection and protection validation.

## Architecture

### 1. Fault Injection Engine

```rust
// bhdl-testbench/src/fault_injection/mod.rs
pub struct FaultInjectionEngine {
    scenarios: HashMap<String, FaultScenario>,
    active_faults: Vec<ActiveFault>,
    safety_analyzer: SafetyAnalysisEngine,
    cascade_detector: CascadeFailureDetector,
}

pub struct FaultScenario {
    pub name: String,
    pub description: String,
    pub trigger: FaultTrigger,
    pub faults: Vec<FaultDefinition>,
    pub expected_behavior: Option<ExpectedBehavior>,
}

pub enum FaultTrigger {
    AtTime(f64),
    WhenCondition(Box<dyn Fn(&SignalValues) -> bool>),
    Progressive { start_time: f64, end_time: f64 },
    Probabilistic { probability: f64, interval: f64 },
}

pub enum FaultDefinition {
    // Component parameter override
    ComponentOverride {
        instance: String,
        parameter: ComponentParameter,
        fault_value: FaultValue,
    },
    
    // Connection faults
    ConnectionFault {
        from: String,
        to: String,
        fault_type: ConnectionFaultType,
    },
    
    // Model replacement
    ModelReplacement {
        instance: String,
        fault_model: FaultModel,
    },
}

pub enum ComponentParameter {
    Resistance,
    Capacitance,
    ForwardVoltage,
    Beta,
    LeakageCurrent,
    Custom(String),
}

pub enum FaultValue {
    Absolute(f64),
    Relative(f64),  // Multiplier
    Short,          // ~0 ohms
    Open,           // ~1e12 ohms
    Drift(f64),     // Percentage drift
}

pub enum FaultModel {
    ShortCircuit,
    OpenCircuit,
    PartialShort(f64),  // Resistance
    LeakyCapacitor { leakage: f64 },
    FailedTransistor { mode: TransistorFailure },
    DegradedDiode { forward_voltage: f64, reverse_current: f64 },
}
```

### 2. Cascade Failure Detector

```rust
// bhdl-spice/src/safety/cascade_detector.rs
pub struct CascadeFailureDetector {
    circuit: Circuit,
    component_models: HashMap<String, ComponentModel>,
    failure_propagation_rules: Vec<PropagationRule>,
}

impl CascadeFailureDetector {
    pub fn analyze_fault_impact(
        &self,
        initial_fault: &FaultDefinition,
        dc_result: &AnalysisResult,
    ) -> CascadeAnalysisResult {
        let mut affected_components = HashSet::new();
        let mut failure_chain = Vec::new();
        let mut work_queue = VecDeque::new();
        
        // Start with the initial fault
        work_queue.push_back(initial_fault.clone());
        
        while let Some(fault) = work_queue.pop_front() {
            // 1. Calculate stress on connected components
            let stressed_components = self.find_stressed_components(&fault, dc_result);
            
            // 2. Check if any exceed their limits
            for (component_id, stress) in stressed_components {
                if self.exceeds_limits(&component_id, &stress) {
                    let secondary_fault = self.predict_failure_mode(&component_id, &stress);
                    
                    if !affected_components.contains(&component_id) {
                        affected_components.insert(component_id.clone());
                        failure_chain.push(FailureEvent {
                            component: component_id,
                            failure_mode: secondary_fault.clone(),
                            stress_level: stress,
                            time_to_failure: self.estimate_time_to_failure(&stress),
                        });
                        
                        // Add to queue for further propagation
                        work_queue.push_back(secondary_fault);
                    }
                }
            }
        }
        
        CascadeAnalysisResult {
            initial_fault: initial_fault.clone(),
            failure_chain,
            affected_components,
            total_damage_estimate: self.calculate_total_damage(&affected_components),
            recommended_protections: self.suggest_protections(&failure_chain),
        }
    }
    
    fn find_stressed_components(
        &self,
        fault: &FaultDefinition,
        dc_result: &AnalysisResult,
    ) -> HashMap<ComponentId, StressLevel> {
        let mut stressed = HashMap::new();
        
        match fault {
            FaultDefinition::ComponentOverride { instance, parameter, fault_value } => {
                // Example: If R1 shorts, check current through connected components
                if matches!(fault_value, FaultValue::Short) {
                    // Find components in series that will see increased current
                    let current_path = self.trace_current_path(instance);
                    for component in current_path {
                        let new_current = self.calculate_fault_current(&component, dc_result);
                        stressed.insert(component.id, StressLevel::Current(new_current));
                    }
                }
            }
            // ... other fault types
        }
        
        stressed
    }
}

pub struct StressLevel {
    pub electrical: ElectricalStress,
    pub thermal: Option<ThermalStress>,
    pub mechanical: Option<MechanicalStress>,
}

pub struct ElectricalStress {
    pub voltage: f64,
    pub current: f64,
    pub power: f64,
    pub dv_dt: Option<f64>,
    pub di_dt: Option<f64>,
}
```

### 3. Testbench Integration

```rust
// bhdl-testbench/src/coordinator.rs
impl TestbenchRunner {
    pub fn run_with_faults(&mut self, fault_scenarios: Vec<String>) -> Result<FaultTestResults> {
        let mut results = FaultTestResults::new();
        
        for scenario_name in fault_scenarios {
            println!("Running fault scenario: {}", scenario_name);
            
            // 1. Reset circuit to nominal state
            self.reset_to_nominal()?;
            
            // 2. Run baseline simulation
            let baseline = self.run_baseline_simulation()?;
            
            // 3. Inject fault
            let scenario = self.fault_engine.get_scenario(&scenario_name)?;
            self.inject_fault(scenario)?;
            
            // 4. Run faulted simulation
            let faulted = self.run_faulted_simulation()?;
            
            // 5. Run safety analysis
            let safety_result = self.analyze_safety_with_fault(scenario)?;
            
            // 6. Compare and record results
            results.add_scenario_result(ScenarioResult {
                scenario: scenario_name,
                baseline,
                faulted,
                safety_analysis: safety_result,
                cascade_failures: self.detect_cascade_failures(scenario)?,
                protection_effectiveness: self.evaluate_protections(scenario)?,
            });
        }
        
        Ok(results)
    }
    
    fn inject_fault(&mut self, scenario: &FaultScenario) -> Result<()> {
        match &scenario.trigger {
            FaultTrigger::AtTime(time) => {
                self.schedule_fault_at_time(*time, &scenario.faults)?;
            }
            FaultTrigger::WhenCondition(condition) => {
                self.watch_for_condition(condition, &scenario.faults)?;
            }
            FaultTrigger::Progressive { start_time, end_time } => {
                self.schedule_progressive_fault(*start_time, *end_time, &scenario.faults)?;
            }
            FaultTrigger::Probabilistic { probability, interval } => {
                self.setup_probabilistic_fault(*probability, *interval, &scenario.faults)?;
            }
        }
        Ok(())
    }
}
```

### 4. SPICE Solver Enhancement

```rust
// bhdl-spice/src/adaptive_solver.rs
impl AdaptiveCircuitSolver {
    /// Apply fault to component model
    pub fn apply_component_fault(&mut self, instance: &str, fault: &FaultValue) -> Result<()> {
        if let Some(model) = self.models.get_mut(instance) {
            match (model, fault) {
                (ComponentModel::Resistor { resistance, .. }, FaultValue::Short) => {
                    *resistance = 1e-3;  // 1 milliohm
                }
                (ComponentModel::Resistor { resistance, .. }, FaultValue::Open) => {
                    *resistance = 1e12;  // 1 teraohm
                }
                (ComponentModel::Resistor { resistance, .. }, FaultValue::Drift(pct)) => {
                    *resistance *= 1.0 + (pct / 100.0);
                }
                (ComponentModel::Capacitor { capacitance, esr, .. }, FaultValue::Short) => {
                    *capacitance = 1e6;  // Very large capacitance
                    *esr = Some(1e-3);   // Very low ESR
                }
                (ComponentModel::LED { forward_voltage, .. }, FaultValue::Open) => {
                    // Change to high resistance diode model
                    *self.models.get_mut(instance).unwrap() = ComponentModel::Resistor {
                        resistance: 1e12,
                        tolerance: 0.0,
                        limits: Default::default(),
                    };
                }
                // ... other combinations
            }
            Ok(())
        } else {
            Err(SpiceError::ComponentNotFound(instance.to_string()))
        }
    }
}
```

## Implementation Phases

### Phase 1: Basic Fault Injection (Week 1-2)
1. **Fault Definition Language**
   - Parse fault scenarios in testbench
   - Support basic parameter overrides
   - Time-based triggering

2. **SPICE Integration**
   - Modify component models at runtime
   - Handle open/short circuits
   - Parameter drift simulation

3. **Basic Testing**
   - LED circuit with resistor faults
   - Capacitor failures in filters
   - Simple short circuit scenarios

### Phase 2: Safety Integration (Week 3-4)
1. **Connect to Safety Module**
   - Run safety analysis on faulted circuits
   - Generate safety reports for each scenario
   - Track protection effectiveness

2. **Cascade Detection**
   - Implement basic propagation rules
   - Identify stressed components
   - Predict secondary failures

3. **Protection Validation**
   - Test TVS diode clamping
   - Verify fuse response times
   - Validate current limiting

### Phase 3: Advanced Features (Week 5-6)
1. **Progressive Faults**
   - Gradual parameter degradation
   - Temperature-dependent failures
   - Aging simulation

2. **Probabilistic Faults**
   - Random failure injection
   - Statistical analysis
   - Monte Carlo with faults

3. **Reporting**
   - Fault tree diagrams
   - FMEA tables
   - Protection recommendations

## Example Usage

### Simple Fault Test
```bhdl
testbench TB_LED_Fault for LEDCircuit {
    faults {
        scenario "resistor_short" {
            at time: 10ms {
                override R1.resistance = short;
            }
            
            expect {
                safety_violation: overcurrent at LED1;
                max_current: < 100mA;  // Protection should limit
            }
        }
    }
    
    verify {
        assert LED1.current < 100mA always message "Overcurrent protection failed";
    }
}
```

### Cascade Failure Test
```bhdl
testbench TB_PowerSupply_Cascade for PowerSupply {
    faults {
        scenario "mosfet_short" {
            at time: 50ms {
                override Q1.drain_source = short;
            }
            
            cascade_analysis {
                track: [L1.saturation, D1.reverse_breakdown, C1.overvoltage];
                time_window: 10ms;
            }
        }
    }
    
    safety_report {
        include: cascade_failures;
        format: detailed;
    }
}
```

### Protection Validation
```bhdl
testbench TB_Protection_Validation for ProtectedCircuit {
    faults {
        scenario "voltage_spike" {
            at @VIN > 20V {
                // Spike already injected via stimulus
            }
            
            verify_protection {
                tvs_clamps: within 100ns;
                max_voltage: @VOUT < 6V;
                no_damage_to: [U1, Q1];
            }
        }
    }
}
```

## Integration Points

### 1. Parser Updates
- Add `faults` block to testbench grammar
- Support fault definition syntax
- Parse safety expectations

### 2. Synthesizer
- Maintain fault injection points
- Track protection components
- Generate safety metadata

### 3. Safety Module
- Enhance cascade detection
- Add fault-specific rules
- Generate FMEA reports

### 4. Visualization
- Show fault propagation paths
- Highlight stressed components
- Display protection effectiveness

## Benefits

1. **Design Validation**: Verify circuit behavior under fault conditions
2. **Safety Assurance**: Validate protection mechanisms work correctly
3. **Robustness Testing**: Identify weak points before production
4. **Documentation**: Automatic generation of safety analysis reports
5. **Compliance**: Meet safety standards requirements (IEC 61508, ISO 26262)

## Next Steps

1. Review and refine the specification
2. Implement Phase 1 basic fault injection
3. Create test cases for common fault scenarios
4. Integrate with existing safety module
5. Develop cascade failure detection algorithms

This comprehensive fault injection system will make BHDL a powerful tool for safety-critical circuit design and validation.