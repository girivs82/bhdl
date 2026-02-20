# Failure Modes vs. Failure Effects Architecture
## Clean Separation Between Component Libraries and Safety Analysis

### Architecture Principle

**Component Libraries** contain **failure modes** (generic ways components can fail) while the **Safety Synthesizer** determines **failure effects** (what happens in the specific circuit context) through circuit analysis and simulation.

This separation enables:
1. **Reusable component models** - Same failure modes apply across all circuit applications
2. **Context-sensitive analysis** - Effects calculated based on actual circuit topology and intent
3. **Maintainable libraries** - Component models don't need circuit-specific information
4. **Accurate analysis** - Effects determined by simulation, not generic assumptions

---

## Component Library Structure (Revised)

### Failure Modes Only - No Effects

```bhdl
// File: bhdl-stdlib/src/voltage_regulators/supervisors/ltc2954.bhdl
entity LTC2954 {
    // Electrical and behavioral models (same as before)
    electrical_model { /* ... */ }
    behavioral_model { /* ... */ }
    
    // ONLY failure modes - NO effects
    failure_modes {
        stuck_fault_low: { 
            rate: 20FIT,
            description: "FAULT_N output stuck at logic low",
            failure_mechanism: "Output driver transistor stuck on",
            detectable_by_self_test: true,
            self_test_coverage: 90%
        },
        
        stuck_fault_high: { 
            rate: 18FIT,
            description: "FAULT_N output stuck at logic high", 
            failure_mechanism: "Output driver transistor stuck off or open",
            detectable_by_self_test: true,
            self_test_coverage: 85%
        },
        
        threshold_drift: { 
            rate: 15FIT,
            description: "Monitoring threshold accuracy degradation",
            failure_mechanism: "Reference voltage drift or resistor aging",
            detectable_by_self_test: true,
            self_test_coverage: 80%
        },
        
        comparator_failure: { 
            rate: 12FIT,
            description: "Internal comparator malfunction",
            failure_mechanism: "Comparator stuck or oscillating",
            detectable_by_self_test: true,
            self_test_coverage: 90%
        },
        
        reference_drift: { 
            rate: 8FIT,
            description: "Internal voltage reference degradation",
            failure_mechanism: "Bandgap reference aging",
            detectable_by_self_test: true,
            self_test_coverage: 95%
        }
    };
    
    // Diagnostic capabilities (what the component can detect about itself)
    self_diagnostic_capabilities {
        test_method: internal_reference_comparison;
        test_interval_range: 1ms..1s;
        detectable_failure_types: [stuck_outputs, threshold_drift, reference_failure, comparator_failure];
        coverage_basis: "Fault injection testing with 150 test vectors";
        overall_test_coverage: 87%;  // Weighted average
    };
    
    // NO failure effects - these are determined by circuit context
}
```

### Switching Regulator - Modes Only

```bhdl  
// File: bhdl-stdlib/src/voltage_regulators/switching_regulators/lm2596.bhdl
entity LM2596(output_voltage: voltage = 5V, max_current: current = 3A) {
    // Electrical and behavioral models for simulation
    electrical_model { /* switching parameters, control loop, etc. */ }
    behavioral_model { /* PWM control, protection, startup */ }
    ripple_model { /* performance calculations */ }
    
    // ONLY failure modes - NO circuit-specific effects
    failure_modes {
        no_switching: { 
            rate: 15FIT,
            description: "PWM controller failure, no switching activity",
            failure_mechanism: "Control IC internal failure or clock generator fault",
            observable_symptoms: ["0V output", "no_switching_node_activity"],
            detectable_externally: true  // Easy to detect with voltage monitoring
        },
        
        overvoltage_runaway: { 
            rate: 8FIT,
            description: "Loss of regulation, output follows input",
            failure_mechanism: "Feedback network failure or control loop instability",
            observable_symptoms: ["output_voltage > regulation_spec", "uncontrolled_duty_cycle"],
            detectable_externally: true  // Detectable with overvoltage monitoring
        },
        
        oscillation: { 
            rate: 10FIT,
            description: "Control loop instability causing high-frequency oscillation",
            failure_mechanism: "Compensation network failure or loop instability",
            observable_symptoms: ["excessive_switching_frequency", "high_output_ripple"],
            detectable_externally: false  // DC monitoring won't detect
        },
        
        current_limit_failure: { 
            rate: 6FIT,
            description: "Overcurrent protection circuit malfunction",
            failure_mechanism: "Current sensing resistor or protection logic failure",
            observable_symptoms: ["no_current_limiting", "thermal_stress_during_overload"],
            detectable_externally: false  // Only during overload condition
        },
        
        thermal_shutdown_failure: { 
            rate: 4FIT,
            description: "Thermal protection circuit malfunction",
            failure_mechanism: "Temperature sensor or shutdown logic failure", 
            observable_symptoms: ["no_thermal_protection", "operation_beyond_safe_temp"],
            detectable_externally: false  // Only during overtemperature
        }
    };
    
    // What can be observed externally (for effect analysis)
    observable_parameters {
        output_voltage: continuously_measurable;
        switching_activity: measurable_with_scope;
        efficiency: calculable_from_power_measurement;
        output_ripple: measurable_with_ac_coupling;
        case_temperature: measurable_with_thermal_sensor;
    };
    
    // NO failure effects specified - determined by circuit context
}
```

---

## Safety Synthesizer - Context-Driven Effect Analysis

### Circuit Context Analysis

```rust
// Safety synthesizer determines effects through circuit analysis
pub struct SafetySynthesizer {
    circuit_analyzer: CircuitAnalyzer,
    simulator: CircuitSimulator,
    intent_analyzer: IntentAnalyzer,
}

impl SafetySynthesizer {
    pub fn analyze_component_failure_effects(
        &self,
        component: &Component,
        failure_mode: &FailureMode,
        circuit: &Circuit
    ) -> FailureEffectAnalysis {
        
        // Analyze circuit context around this component
        let context = self.circuit_analyzer.analyze_component_context(component, circuit);
        
        // Determine what this component powers/controls
        let downstream_components = circuit.get_downstream_components(component);
        let power_domains = circuit.get_power_domains_supplied_by(component);
        let control_signals = circuit.get_signals_from_component(component);
        
        // Extract design intent for this component
        let design_intent = self.intent_analyzer.get_component_intent(component);
        
        // Simulate failure mode in actual circuit
        let failure_simulation = self.simulator.simulate_failure_mode(
            component, 
            failure_mode, 
            circuit
        );
        
        // Determine effects based on context and simulation
        FailureEffectAnalysis {
            local_effect: self.determine_local_effect(failure_mode, failure_simulation),
            next_level_effect: self.analyze_downstream_impact(downstream_components, failure_simulation),
            system_effect: self.analyze_system_level_impact(power_domains, control_signals, failure_simulation),
            end_effect: self.analyze_safety_impact(context.asil_level, system_effect),
            
            // Context-specific severity
            severity: self.calculate_severity_from_context(system_effect, context),
            
            // Intent violation analysis
            intent_violation: self.check_intent_violation(design_intent, failure_simulation),
        }
    }
    
    fn determine_local_effect(&self, failure_mode: &FailureMode, simulation: &FailureSimulation) -> String {
        match failure_mode.description.as_str() {
            "PWM controller failure, no switching activity" => {
                format!("0V output on {}V rail (expected {}V)", 
                    simulation.measured_output_voltage,
                    simulation.expected_output_voltage)
            },
            "Loss of regulation, output follows input" => {
                format!("Unregulated output: {}V on {}V rail ({}% overvoltage)", 
                    simulation.measured_output_voltage,
                    simulation.expected_output_voltage, 
                    simulation.overvoltage_percentage)
            },
            "FAULT_N output stuck at logic low" => {
                "Continuous fault indication regardless of actual power supply status".to_string()
            },
            _ => failure_mode.description.clone()
        }
    }
    
    fn analyze_downstream_impact(&self, downstream: &[Component], simulation: &FailureSimulation) -> String {
        let mut impacts = Vec::new();
        
        for component in downstream {
            match component.component_type.as_str() {
                "microcontroller" | "ecu" => {
                    if simulation.measured_output_voltage < component.min_operating_voltage {
                        impacts.push(format!("{} will brown-out or reset", component.name));
                    }
                    if simulation.measured_output_voltage > component.max_operating_voltage {
                        impacts.push(format!("{} may be damaged by overvoltage", component.name));
                    }
                },
                "sensor" => {
                    impacts.push(format!("{} power supply out of specification", component.name));
                },
                "led" => {
                    if simulation.measured_output_voltage == 0.0 {
                        impacts.push(format!("{} will not illuminate", component.name));
                    } else if simulation.overvoltage_percentage > 20.0 {
                        impacts.push(format!("{} may be damaged by overcurrent", component.name));
                    }
                },
                _ => {}
            }
        }
        
        if impacts.is_empty() {
            "No immediate downstream component impact".to_string()
        } else {
            impacts.join("; ")
        }
    }
    
    fn analyze_system_level_impact(&self, power_domains: &[PowerDomain], control_signals: &[Signal], simulation: &FailureSimulation) -> String {
        let mut system_impacts = Vec::new();
        
        // Analyze power domain impacts
        for domain in power_domains {
            match domain.asil_level {
                ASILLevel::ASIL_B | ASILLevel::ASIL_C | ASILLevel::ASIL_D => {
                    if simulation.power_lost {
                        system_impacts.push(format!("Safety-critical {} domain lost", domain.name));
                    } else if simulation.overvoltage_percentage > 10.0 {
                        system_impacts.push(format!("Safety-critical {} domain overvoltage", domain.name));
                    }
                },
                ASILLevel::QM => {
                    if simulation.power_lost {
                        system_impacts.push(format!("Non-critical {} domain lost", domain.name));
                    }
                },
                _ => {}
            }
        }
        
        // Analyze control signal impacts
        for signal in control_signals {
            if simulation.signal_lost(&signal.name) {
                system_impacts.push(format!("Control signal {} lost", signal.name));
            }
        }
        
        if system_impacts.is_empty() {
            "Localized impact, no system-level effects".to_string()
        } else {
            system_impacts.join("; ")
        }
    }
    
    fn check_intent_violation(&self, intent: &ComponentIntent, simulation: &FailureSimulation) -> Option<IntentViolation> {
        match intent {
            ComponentIntent::VoltageRegulation { target_voltage, tolerance } => {
                let voltage_error = (simulation.measured_output_voltage - target_voltage).abs();
                if voltage_error > tolerance {
                    Some(IntentViolation {
                        intent_type: "voltage_regulation".to_string(),
                        expected: format!("{}V ± {}V", target_voltage, tolerance),
                        actual: format!("{}V", simulation.measured_output_voltage),
                        violation_severity: if voltage_error > tolerance * 2.0 { "CRITICAL" } else { "MODERATE" }.to_string()
                    })
                } else {
                    None
                }
            },
            ComponentIntent::NoiseFiltering { max_ripple } => {
                if simulation.measured_ripple > *max_ripple {
                    Some(IntentViolation {
                        intent_type: "noise_filtering".to_string(),
                        expected: format!("ripple < {}mV", max_ripple.as_millivolts()),
                        actual: format!("ripple = {}mV", simulation.measured_ripple.as_millivolts()),
                        violation_severity: "MODERATE".to_string()
                    })
                } else {
                    None
                }
            },
            // ... other intent types
            _ => None
        }
    }
}
```

### Example: Context-Sensitive Effect Generation

```bhdl
// Same LM2596, different contexts, different effects
context_sensitive_analysis {
    // Context 1: Powers critical automotive ECU
    automotive_ecu_power_supply: {
        component: lm2596_regulator;
        failure_mode: "no_switching";
        
        circuit_context: {
            downstream_components: [airbag_ecu, brake_controller, engine_management];
            power_domain: safety_critical_5v;
            asil_level: ASIL_B;
            design_intent: power_supply_for_safety_systems;
        }
        
        // Tool-generated effects based on context
        generated_effects: {
            local_effect: "0V output on safety-critical 5V rail (expected 5.0V ± 2%)";
            next_level_effect: "Airbag ECU, brake controller, and engine management lose power";
            system_effect: "Multiple safety systems unavailable simultaneously";
            end_effect: "Vehicle safety functions disabled - potential accident risk";
            severity: 9;  // Critical - multiple safety systems affected
            
            intent_violation: {
                intent: "power_supply_for_safety_systems";
                violation: "Complete failure to provide power";
                impact: "CRITICAL - defeats primary function";
            }
        }
    }
    
    // Context 2: Powers non-critical LED indicators  
    led_driver_power_supply: {
        component: lm2596_regulator;  // Same component
        failure_mode: "no_switching";  // Same failure mode
        
        circuit_context: {
            downstream_components: [status_leds, indicator_lights];
            power_domain: non_critical_5v;
            asil_level: QM;
            design_intent: led_driver_power;
        }
        
        // Tool-generated effects - completely different
        generated_effects: {
            local_effect: "0V output on LED driver 5V rail (expected 5.0V ± 5%)";
            next_level_effect: "Status LEDs and indicator lights will not illuminate";  
            system_effect: "Loss of visual status indication";
            end_effect: "Reduced user feedback, no safety impact";
            severity: 3;  // Low - cosmetic issue only
            
            intent_violation: {
                intent: "led_driver_power";
                violation: "No power to LED array";
                impact: "MODERATE - defeats secondary function";
            }
        }
    }
    
    // Context 3: Powers sensor array
    sensor_power_supply: {
        component: lm2596_regulator;  // Same component  
        failure_mode: "overvoltage_runaway";  // Different failure mode
        
        circuit_context: {
            downstream_components: [pressure_sensors, temperature_sensors, position_sensors];
            power_domain: sensor_5v;
            asil_level: ASIL_B;  
            design_intent: precision_sensor_power;
        }
        
        generated_effects: {
            local_effect: "Unregulated 12V output on sensor 5V rail (140% overvoltage)";
            next_level_effect: "Pressure, temperature, and position sensors damaged by overvoltage";
            system_effect: "Loss of critical sensor data for vehicle control"; 
            end_effect: "Vehicle control systems operating with degraded sensor input";
            severity: 8;  // High - sensor damage affects control systems
            
            intent_violation: {
                intent: "precision_sensor_power(tolerance: ±2%)";
                violation: "140% overvoltage vs ±2% specification";
                impact: "CRITICAL - destroys components requiring precision";
            }
        }
    }
}
```

---

## Component Database Integration

### Passive Components with Generic Failure Modes

```bhdl
// File: bhdl-stdlib/src/passives/capacitors/electrolytic_capacitor.bhdl
entity ElectrolyticCap(value: capacitance, voltage_rating: voltage, esr: resistance = auto) {
    electrical_model {
        capacitance: value;
        voltage_rating: voltage_rating;
        esr: if esr == auto { calculate_typical_esr(value, voltage_rating) } else { esr };
        temperature_coefficient: -1%_per_10C;  // Typical for aluminum electrolytic
        ripple_current_rating: calculate_ripple_rating(value, voltage_rating);
    }
    
    behavioral_model {
        aging_model: {
            capacitance_loss: 1%_per_1000_hours at 85C;
            esr_increase: 2%_per_1000_hours at 85C;
            leakage_increase: exponential_with_temperature;
        }
        
        failure_mechanisms: {
            electrolyte_dry_out: temperature_and_time_dependent;
            dielectric_breakdown: voltage_stress_dependent;
            connection_corrosion: environmental_stress_dependent;
        }
    }
    
    // Generic failure modes - effects determined by circuit context
    failure_modes {
        open_circuit: {
            rate: calculate_from_IEC_62380(capacitor_type: electrolytic, stress_factors),
            description: "Loss of capacitance, effectively open circuit",
            failure_mechanism: "Electrolyte dry-out or internal connection failure",
            observable_symptoms: ["no_capacitive_effect", "increased_esr", "circuit_resonance_shift"]
        },
        
        short_circuit: {
            rate: calculate_from_IEC_62380(capacitor_type: electrolytic, stress_factors), 
            description: "Dielectric breakdown, low resistance path",
            failure_mechanism: "Dielectric material breakdown under voltage stress",
            observable_symptoms: ["low_resistance_path", "overcurrent", "possible_fuse_opening"]
        },
        
        capacitance_loss: {
            rate: calculate_from_IEC_62380(capacitor_type: electrolytic, stress_factors),
            description: "Gradual reduction in capacitance value",
            failure_mechanism: "Electrolyte aging and dry-out",
            observable_symptoms: ["reduced_filtering", "increased_ripple", "frequency_response_shift"]
        },
        
        esr_increase: {
            rate: calculate_from_IEC_62380(capacitor_type: electrolytic, stress_factors),
            description: "Equivalent series resistance increases",
            failure_mechanism: "Connection degradation or electrolyte aging", 
            observable_symptoms: ["reduced_filtering_effectiveness", "increased_power_dissipation", "heating"]
        }
    };
    
    // IEC 62380 failure rate calculation
    iec_62380_model {
        base_failure_rate: function_of(capacitor_construction, voltage_rating, capacitance);
        stress_factors: {
            voltage_stress: (applied_voltage / voltage_rating)^2;
            temperature_stress: arrhenius_model(junction_temperature);
            ripple_current_stress: (ripple_current / ripple_current_rating)^2;
        };
        
        total_failure_rate: base_failure_rate * product(stress_factors);
    }
}
```

### Tool Determines Effects Based on Circuit Role

```bhdl
// Tool analyzes same capacitor failure in different contexts
capacitor_failure_effect_analysis {
    // Same failure mode, different circuit roles
    failure_mode: "open_circuit";
    component: output_bulk_cap: ElectrolyticCap(470µF, 10V);
    
    // Context 1: Input filtering capacitor
    input_filter_context: {
        circuit_role: input_energy_storage;
        design_intent: holdup_time(10ms, load_current: 3A);
        circuit_position: between_input_and_regulator;
        
        analyzed_effects: {
            local_effect: "Loss of input energy storage and filtering";
            immediate_impact: "Reduced holdup time from 10ms to 0.5ms";
            system_effect: "Power supply resets during brief input interruptions";
            severity: 7;  // High - violates holdup requirement
            
            intent_violation: {
                intent: "holdup_time(10ms)";
                actual: "0.5ms holdup";
                violation_severity: "CRITICAL - 95% reduction in holdup capability";
            }
        }
    }
    
    // Context 2: Output filtering capacitor
    output_filter_context: {
        circuit_role: output_ripple_reduction;
        design_intent: noise_filtering(ripple < 50mV);
        circuit_position: regulator_output_to_load;
        
        analyzed_effects: {
            local_effect: "Loss of output bulk filtering";
            immediate_impact: "Output ripple increases from 30mV to 180mV";
            system_effect: "Loads experience increased power supply noise";
            severity: 6;  // Moderate - degrades but doesn't kill system
            
            intent_violation: {
                intent: "noise_filtering(ripple < 50mV)";
                actual: "180mV ripple";
                violation_severity: "CRITICAL - 260% above specification";
            }
        }
    }
    
    // Context 3: Decoupling capacitor  
    decoupling_context: {
        circuit_role: local_decoupling;
        design_intent: decoupling(target_ic: microcontroller);
        circuit_position: adjacent_to_mcu_power_pins;
        
        analyzed_effects: {
            local_effect: "Loss of local charge storage for MCU";
            immediate_impact: "MCU supply impedance increases at switching frequencies";
            system_effect: "MCU clock jitter and possible reset during load transients"; 
            severity: 8;  // High - affects system reliability
            
            intent_violation: {
                intent: "decoupling(target_ic: microcontroller)";
                actual: "No local charge storage";
                violation_severity: "HIGH - defeats decoupling function";
            }
        }
    }
}
```

---

## Benefits of Separation

### 1. **Reusable Component Models**
- **Same failure modes** apply whether LM2596 powers LEDs or safety systems
- **Library maintenance** doesn't require knowledge of every possible application
- **Component vendors** can provide standard models without circuit-specific data

### 2. **Accurate Context Analysis**  
- **Real circuit simulation** determines actual failure impacts
- **Intent-driven analysis** shows violations of design purpose
- **Downstream component analysis** traces failure propagation through circuit

### 3. **Maintainable Architecture**
- **Clear separation** between component physics and circuit application
- **Standard libraries** work across all project types
- **Custom components** only need failure modes, not application-specific effects

### 4. **Comprehensive Analysis**
- **Same component models** used for functional and safety simulation
- **Context-sensitive effects** reflect actual circuit behavior
- **Intent violation detection** shows design requirement impacts

This architecture provides **maximum reusability** of component models while enabling **highly accurate, context-specific safety analysis** through intelligent circuit synthesis and simulation.