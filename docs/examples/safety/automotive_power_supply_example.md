# Automotive Power Supply Safety Example
## 12V Battery to 5V Regulation with Multi-Phase Safety Analysis

### Overview

This example demonstrates the complete multi-phase safety architecture using a realistic automotive power supply that converts 12V battery power to regulated 5V for electronic control units (ECUs).

**Application Context**: Automotive body control module (BCM) power supply
**Safety Level**: ASIL B (moderate automotive safety requirements)
**Key Requirements**: Reliable 5V power for safety-related functions

---

## Phase 1: System-Level Safety Analysis (Day 1)

### System-Level Safety Definition

```bhdl
// File: safety/automotive_psu_system.bhdl
// Created: Day 1, before any board design exists
// Author: Safety Engineer

system_safety AutomotivePowerSystemSafety {
    application_context {
        domain: automotive;
        system: body_control_module;
        environment: under_hood;
        temperature_range: -40C..85C;
        asil_target: ASIL_B;
    }
    
    // Functional decomposition without implementation details
    functional_blocks {
        power_input: BatteryInterface {
            voltage_nominal: 12V;
            voltage_range: 9V..16V;  // Automotive battery variation
            transient_tolerance: 6V..24V;  // Jump start, load dump
            max_current: 5A;
            source_impedance: 50mΩ;  // Battery + wiring
            
            // Functional requirements, no implementation
            protection_required: [reverse_polarity, overvoltage, undervoltage];
            emc_class: Class_3;  // Automotive EMC
        }
        
        primary_regulation: VoltageRegulation {
            input: power_input;
            output_voltage: 5V ± 2%;  // ±100mV tolerance
            max_current: 3A;
            efficiency_target: >80%;  // Thermal management
            
            // Safety-critical requirements
            max_output_ripple: 50mV_rms;
            load_regulation: <1%;
            line_regulation: <0.5%;
            
            // Transient response for load steps
            settling_time: <100µs;
            overshoot: <5%;
        }
        
        output_distribution: PowerDistribution {
            input: primary_regulation;
            load_types: [microcontrollers, sensors, actuators];
            load_profiles: [continuous, switched, pulsed];
            
            // Distribution requirements
            impedance_budget: <10mΩ;  // Low voltage drop
            current_sharing: balanced;
        }
        
        critical_loads: SafetyCriticalSystems {
            power_source: output_distribution;
            asil_level: ASIL_B;
            
            // Critical load requirements  
            power_interruption_tolerance: 1ms;  // Before shutdown
            voltage_tolerance: 5V ± 3%;  // ±150mV
            noise_immunity: 100mV;  // EMC requirement
        }
    }
    
    // Hazard analysis and safety goals
    hazard_analysis {
        H1: hazard {
            description: "Loss of power to safety-critical ECU functions";
            potential_cause: "Power supply failure";
            hazardous_event: "Safety system unavailable when needed";
            severity: S2;  // Moderate injury potential
            exposure: E4;  // Very high (normal driving)
            controllability: C2;  // Normally controllable
            asil: ASIL_B;  // S2 × E4 × C2 = ASIL B
        }
        
        H2: hazard {
            description: "Overvoltage damage to ECU";
            potential_cause: "Regulation failure";
            hazardous_event: "ECU malfunction or damage";
            severity: S2;
            exposure: E4;
            controllability: C2;
            asil: ASIL_B;
        }
    }
    
    // Safety goals derived from hazard analysis
    safety_goals {
        SG1: safety_goal {
            id: "SG_PSU_001";
            description: "Prevent loss of power to safety-critical systems";
            derived_from: H1;
            asil: ASIL_B;
            safe_state: "Controlled shutdown with fault indication";
        }
        
        SG2: safety_goal {
            description: "Prevent overvoltage damage to downstream systems";
            derived_from: H2;
            asil: ASIL_B;
            safe_state: "Power shutdown with fault isolation";
        }
    }
    
    // Functional safety requirements (no implementation details)
    functional_safety_requirements {
        FSR1: safety_function {
            id: "FSR_PSU_001";
            implements: SG1;
            description: "Detect and respond to power supply failures";
            asil: ASIL_B;
            
            // Allocated to functional blocks
            primary_mechanism {
                allocated_to: primary_regulation;
                function_type: output_monitoring;
                coverage_target: 90%;  // ASIL B requirement
                response_time: <100µs;  // Before damage occurs
            }
            
            latent_mechanism {
                allocated_to: primary_regulation;
                function_type: periodic_self_test;
                test_interval: 100ms;  // Detect dormant faults
                coverage_target: 60%;  // ASIL B LFM requirement
            }
        }
        
        FSR2: safety_function {
            id: "FSR_PSU_002";
            implements: SG2;
            description: "Overvoltage protection";
            asil: ASIL_B;
            
            primary_mechanism {
                allocated_to: primary_regulation;
                function_type: overvoltage_detection;
                threshold: 5.5V;  // 10% overvoltage limit
                response_time: <10µs;  // Fast protection
                coverage_target: 95%;  // High coverage needed
            }
        }
    }
    
    // Generated requirements for board designer
    generated_requirements {
        // These are created automatically from safety analysis
        REQ_PSU_001: requirement {
            type: functional;
            source: FSR1.primary_mechanism;
            description: "Primary regulator shall monitor output voltage";
            allocated_to: primary_regulation;
            
            // Testable, measurable requirements
            constraints {
                monitoring_range: 0V..6V;
                accuracy: ±1%;
                response_time: <100µs;
                fault_output: logic_signal;
            }
        }
        
        REQ_PSU_002: requirement {
            type: functional;
            source: FSR1.latent_mechanism;
            description: "Primary regulator shall provide self-test capability";
            allocated_to: primary_regulation;
            
            constraints {
                test_method: built_in_self_test;
                test_interval: ≤100ms;
                test_signal: dedicated_pin;
                result_indication: logic_signal;
            }
        }
        
        REQ_PSU_003: requirement {
            type: functional;
            source: FSR2.primary_mechanism;
            description: "Primary regulator shall detect overvoltage";
            allocated_to: primary_regulation;
            
            constraints {
                overvoltage_threshold: 5.5V ± 2%;
                detection_time: <10µs;
                protection_action: immediate_shutdown;
                recovery: manual_reset_required;
            }
        }
        
        // Electrical performance requirements
        REQ_PSU_004: requirement {
            type: performance;
            description: "Output voltage regulation";
            constraints {
                nominal: 5.0V;
                tolerance: ±2%;  // ±100mV
                load_regulation: <1%;
                line_regulation: <0.5%;
                ripple: <50mV_rms;
            }
        }
        
        REQ_PSU_005: requirement {
            type: performance;
            description: "Efficiency and thermal";
            constraints {
                efficiency: >80%;
                max_case_temperature: 85C;
                thermal_derating: 70%;
            }
        }
    }
}
```

### Benefits of Phase 1 Analysis

**For Safety Engineer:**
- Productive from day 1, no waiting for board design
- Focus on hazards, safety goals, and functional requirements
- Establishes ASIL allocations and coverage targets
- Creates stable foundation that survives board design changes

**For Board Designer:**
- Clear functional requirements with measurable constraints
- Safety requirements integrated from start, not retrofitted
- Freedom to choose implementation (linear, switching, etc.)
- Testable specifications for validation

---

## Phase 2: Board Design with Real-Time Safety Analysis

### Board Implementation

```bhdl
// File: board/automotive_psu_board.bhdl
// Created: After system requirements defined
// Author: Board Designer

board AutomotivePowerSupply {
    // Metadata linking to system requirements
    attribute implements = "AutomotivePowerSystemSafety";
    attribute asil = ASIL_B;
    attribute application = "automotive_bcm";
    
    // Power domain declarations
    power VBATT = 12V @ 5A {
        attribute source = automotive_battery;
        attribute voltage_range = 9V..16V;
        attribute transient_range = 6V..24V;
    }
    power VCC_5V = 5V @ 3A {
        attribute regulated = true;
        attribute tolerance = ±2%;
        attribute asil = ASIL_B;
    }
    ground PGND;
    ground AGND;
    
    // Input protection and filtering
    @VBATT @PROTECTED-> fuse: AutomotiveFuse(7.5A) {
        // Intent drives safety analysis
        for overcurrent_protection(max_current: 7.5A, response_time: <1s);
    };
    
    fuse.out @FUSED-> tvs: TVSDiode(28V) {
        for transient_protection(max_voltage: 28V, response_time: <1ns);
    };
    tvs.A -> @PGND;
    
    // Input filtering - intent clarifies purpose
    @FUSED -> input_bulk: ElectrolyticCap(220µF, 25V) {
        for energy_storage(holdup_time: 10ms, load_current: 3A);
    } -> @PGND;
    
    @FUSED -> input_hf: CeramicCap(100nF, 25V) {
        for noise_filtering(frequency_range: 1MHz..100MHz, impedance: <1Ω);
    } -> @PGND;
    
    // Primary regulation - board designer chooses implementation
    @FUSED -> regulator: LM2596(5V, 3A) {  // Switching regulator choice
        // Component parameters for safety analysis
        attribute switching_frequency = 150kHz;
        attribute efficiency = 85%;
        attribute thermal_resistance = 15C/W;
        
        // Safety-relevant attributes
        attribute failure_modes = {
            no_switching: { rate: 15FIT, safe: false },
            overvoltage: { rate: 8FIT, safe: false },
            thermal_shutdown: { rate: 12FIT, safe: true },
            oscillation: { rate: 10FIT, safe: false }
        };
        
        attribute built_in_protection = [
            thermal_shutdown,
            current_limit,
            undervoltage_lockout
        ];
    };
    
    // Switching regulator external components
    regulator.SW -> inductor: PowerInductor(47µH, 4A) {
        for energy_transfer(ripple_current: <1A, saturation_margin: 50%);
    };
    
    inductor.2 @SW_NODE-> schottky: SchottkyDiode(40V, 3A) {
        for synchronous_rectification(forward_drop: <0.3V, recovery_time: <50ns);
    };
    schottky.cathode -> @PGND;
    
    regulator.FB @FB_SENSE<- feedback_divider: VoltageDivider(10kΩ, 2kΩ) {
        for voltage_sensing(accuracy: ±0.5%, stability: high_impedance);
    };
    feedback_divider.low -> @PGND;
    
    // Output filtering and regulation
    @SW_NODE @RECT-> output_bulk: ElectrolyticCap(470µF, 10V) {
        for output_filtering(ripple_reduction: 90%, esr: <50mΩ);
    };
    output_bulk.- -> @PGND;
    
    @RECT -> output_ceramic: CeramicCap(10µF, 10V) {
        for high_frequency_filtering(esr: <5mΩ, resonance: >1MHz);
    } -> @PGND;
    
    // Regulated output with monitoring
    @RECT @VCC_5V-> voltage_monitor: VoltageMonitor {
        // Component implements safety requirements  
        attribute monitoring_range = 0V..6V;
        attribute accuracy = ±1%;
        attribute response_time = 50µs;  // Meets <100µs requirement
        attribute overvoltage_threshold = 5.5V;
        attribute fault_output = active_low;
        
        // Self-test capability for latent fault detection
        attribute self_test = {
            method: internal_reference_test;
            test_duration: 10ms;
            test_interval_capability: 1ms..1s;
            coverage: 85%;  // Meets >60% requirement
        };
    };
    
    // Connect monitoring signals
    voltage_monitor.fault -> mcu_interface: HeaderPin(fault_signal);
    voltage_monitor.test_enable <- test_control: HeaderPin(test_input);
    voltage_monitor.test_result -> test_result: HeaderPin(test_output);
    
    // Output distribution
    @VCC_5V -> output_connector: AutomotiveConnector(5_pin) {
        for power_distribution(contact_resistance: <5mΩ, current_rating: 3A);
    };
    
    // Test points for validation
    @PROTECTED -> tp_input: TestPoint();
    @VCC_5V -> tp_output: TestPoint();
    @FB_SENSE -> tp_feedback: TestPoint();
    @PGND -> tp_ground: TestPoint();
    
    // Board-level constraints for layout
    constrain placement {
        // Keep switching elements close to minimize loop area
        group switching_loop: [regulator, inductor, schottky] {
            max_distance: 5mm;
            priority: high;
        }
        
        // Separate analog feedback from switching noise
        isolation feedback_path: feedback_divider {
            clearance_from: switching_loop;
            min_distance: 10mm;
        }
    }
    
    constrain routing {
        // Low impedance power connections
        net @VCC_5V {
            min_width: 1mm;
            via_stitching: every_5mm;
            copper_weight: 2oz;
        }
        
        // Controlled impedance for feedback
        net @FB_SENSE {
            impedance: 50Ω ± 10%;
            guard_traces: @AGND;
            layer: internal;
        }
    }
    
    // Environmental specifications
    attribute operating_conditions = {
        temperature: -40C..85C;
        humidity: 10%..95%;
        vibration: automotive_grade;
        emc_compliance: CISPR_25_Class_3;
    };
}
```

### Real-Time Safety Analysis Results

As soon as the board designer adds each component, the safety analysis tool generates contextual analysis:

```bhdl
// Auto-generated as components are added to the board
real_time_safety_analysis {
    board: AutomotivePowerSupply;
    analysis_timestamp: 2024-01-15T10:30:00Z;
    analysis_context: automotive_bcm_ASIL_B;
    
    // Component analysis with full context
    component_analysis {
        LM2596_regulator: {
            handle: "regulator";  // Stable reference
            component_type: "Buck_Switching_Regulator";
            safety_criticality: HIGH;  // Powers ASIL B systems
            
            // Context-aware failure analysis
            contextual_analysis {
                circuit_topology: buck_converter;
                output_load: safety_critical_ecu;
                downstream_asil: ASIL_B;
                power_budget: 15W;  // 5V × 3A
                thermal_environment: under_hood_automotive;
                
                // Intent-based analysis from 'for' keywords
                detected_intents: [
                    voltage_regulation(5V, ±2%),
                    efficiency_target(85%),
                    thermal_management(85C_max)
                ];
            }
            
            failure_modes {
                no_switching: {
                    base_rate: 15FIT;  // From component datasheet
                    local_effect: "No output voltage, 0V on 5V rail";
                    
                    // Tool simulates downstream impact
                    system_effect: "Complete power loss to ASIL B ECU functions";
                    operational_impact: "Safety system unavailable";
                    hazard_contribution: "Maps to hazard H1 - Power loss";
                    
                    // Contextual severity assessment
                    severity: 8;  // High - safety system unavailable
                    detection_by_psm: 99%;  // Voltage monitor detects 0V
                    residual_risk: 0.15FIT;  // 15FIT × 1%
                    
                    // Tool-generated insight
                    failure_physics: "Control IC failure, no switching activity";
                    diagnostic_signature: "Output voltage below 1V";
                }
                
                overvoltage_runaway: {
                    base_rate: 8FIT;
                    local_effect: "Unregulated output, up to 12V on 5V rail";
                    
                    // Critical failure - tool identifies through simulation
                    system_effect: "Overvoltage damage to downstream ECUs";
                    operational_impact: "ECU malfunction or permanent damage";
                    hazard_contribution: "Maps to hazard H2 - Overvoltage damage";
                    
                    severity: 9;  // Critical - component damage
                    detection_by_psm: 95%;  // Voltage monitor at 5.5V threshold
                    response_time: 50µs;  // Monitor response time
                    residual_risk: 0.4FIT;  // 8FIT × 5%
                    
                    failure_physics: "Feedback loop failure, no regulation";
                    diagnostic_signature: "Output voltage above 5.5V";
                    protection_action: "Immediate shutdown by voltage monitor";
                }
                
                oscillation: {
                    base_rate: 10FIT;
                    local_effect: "High-frequency noise on output";
                    
                    // Tool analyzes noise impact through circuit simulation
                    system_effect: "EMC violations, potential ECU malfunction";
                    operational_impact: "Intermittent system errors";
                    hazard_contribution: "Contributes to H1 through system instability";
                    
                    severity: 6;  // Moderate - operational degradation
                    detection_by_psm: 30%;  // Difficult to detect with DC monitoring
                    latent_fault_potential: HIGH;  // May go undetected
                    
                    failure_physics: "Compensation network failure, instability";
                    diagnostic_signature: "High-frequency ripple >200mV";
                }
            }
            
            // Safety mechanism effectiveness analysis
            safety_mechanism_analysis {
                primary_mechanism: {
                    component: voltage_monitor;
                    function: output_voltage_monitoring;
                    
                    // Tool calculates actual coverage
                    calculated_coverage: {
                        no_switching: 99%;  // Easily detects 0V
                        overvoltage_runaway: 95%;  // Threshold at 5.5V
                        oscillation: 30%;  // DC monitor misses AC issues
                        weighted_average: 92%;  // Exceeds 90% ASIL B requirement
                    }
                    
                    response_analysis: {
                        detection_time: 50µs;  // From component spec
                        shutdown_time: 10µs;   // Internal protection
                        total_response: 60µs;  // Meets <100µs requirement
                    }
                }
                
                latent_mechanism: {
                    component: voltage_monitor;
                    function: periodic_self_test;
                    
                    calculated_coverage: {
                        monitor_stuck_fault: 90%;  // Reference test detects
                        threshold_drift: 80%;      // Calibration check
                        comparator_failure: 85%;   // Built-in test
                        weighted_average: 87%;     // Exceeds 60% ASIL B requirement
                    }
                    
                    test_analysis: {
                        test_interval: 100ms;  // Meets ≤100ms requirement
                        test_duration: 10ms;   // Brief interruption
                        self_diagnostic: true; // No external stimulus needed
                    }
                }
            }
        }
        
        // Additional component analyses...
        TVS_diode_protection: {
            handle: "tvs";
            safety_criticality: MEDIUM;  // Input protection
            
            contextual_analysis {
                circuit_topology: input_protection;
                protection_target: switching_regulator_input;
                transient_environment: automotive_electrical_system;
                
                detected_intents: [
                    transient_protection(28V_max, <1ns_response)
                ];
            }
            
            failure_modes {
                short_circuit: {
                    base_rate: 5FIT;
                    local_effect: "Input fuse opens, no power";
                    system_effect: "Complete system shutdown";
                    severity: 7;  // High but safe failure
                    safe_failure: true;  // Fails to safe state
                }
                
                open_circuit: {
                    base_rate: 2FIT;  
                    local_effect: "No transient protection";
                    system_effect: "Regulator vulnerable to load dump";
                    severity: 8;  // High - latent dangerous failure
                    latent_fault: true;  // Only detected during transient
                }
            }
        }
        
        voltage_monitor: {
            handle: "voltage_monitor";
            safety_criticality: CRITICAL;  // Implements safety function
            
            contextual_analysis {
                safety_function: FSR_PSU_001;  // Maps to safety requirements
                asil_allocation: ASIL_B;
                coverage_target: 90%;  // SPFM requirement
                
                detected_intents: [
                    overvoltage_protection(5.5V_threshold, <100µs_response),
                    self_test_capability(100ms_interval)
                ];
            }
            
            failure_modes {
                stuck_low_fault: {
                    base_rate: 20FIT;
                    local_effect: "False fault indication";
                    system_effect: "Nuisance shutdown of power supply";
                    severity: 4;  // Low - safe but inconvenient
                    safe_failure: true;
                    detection_by_lsm: 90%;  // Self-test detects stuck fault
                }
                
                stuck_high_fault: {
                    base_rate: 20FIT;
                    local_effect: "No fault indication during overvoltage";
                    system_effect: "Overvoltage damage to ECU not prevented";
                    severity: 9;  // Critical - safety function lost
                    dangerous_failure: true;
                    detection_by_lsm: 85%;  // Self-test with reference check
                }
                
                threshold_drift: {
                    base_rate: 15FIT;
                    local_effect: "Incorrect trip threshold";
                    system_effect: "Late or early fault detection";
                    severity: 6;  // Moderate degradation
                    detection_by_lsm: 80%;  // Calibration check
                }
            }
        }
    }
    
    // System-level analysis
    system_level_analysis {
        safety_function_FSR1: {
            implementation_mapping: {
                primary_mechanism: voltage_monitor.output_monitoring;
                latent_mechanism: voltage_monitor.self_test;
            }
            
            calculated_metrics: {
                spfm: 91.5%;  // Weighted average of PSM coverage
                lfm: 68.2%;   // Includes LSM effectiveness  
                pmhf: 42FIT;  // Residual risk from all components
                
                meets_asil_b: true;  // SPFM>90%, LFM>60%, PMHF<100FIT
            }
            
            gap_analysis: {
                coverage_gaps: [];  // No gaps found
                metric_compliance: PASS;
                recommendations: [
                    "Consider reducing self-test interval to 50ms for higher LFM",
                    "Add redundant monitoring for ASIL C applications"
                ];
            }
        }
    }
}
```

---

## Phase 3: Complete FMEA/FMEDA Generation

### Automatic FMEA Generation

```bhdl
// Auto-generated comprehensive FMEA
generated_fmea {
    project: "Automotive BCM Power Supply";
    board: AutomotivePowerSupply;
    safety_analysis: AutomotivePowerSystemSafety;
    asil_level: ASIL_B;
    analysis_date: 2024-01-15;
    
    fmea_header {
        system_description: "12V to 5V switching power supply for automotive BCM";
        design_responsibility: "Hardware Team";
        safety_responsibility: "Functional Safety Team";
        review_status: "Preliminary";
    }
    
    // Component-by-component FMEA entries
    fmea_entries: [
        {
            item_number: 1;
            component: "U1 - LM2596 Switching Regulator";
            handle: "regulator";  // Stable reference
            function: "Convert 12V battery power to regulated 5V";
            
            failure_mode: "No switching (control IC failure)";
            failure_rate: 15FIT;
            
            local_effect: "Zero output voltage on 5V rail";
            next_level_effect: "Complete power loss to ECU functions";
            system_effect: "Safety-critical systems unavailable";
            
            current_controls: {
                design: "Voltage monitor with fault output";
                process: "Component qualification testing";
                verification: "Power-on self-test verification";
            }
            
            severity: 8;  // High - safety impact
            occurrence: 2;  // Low frequency (15FIT)
            detection: 1;   // Always detected by voltage monitor
            rpn: 16;       // S×O×D = 8×2×1
            
            safety_analysis: {
                classification: "Single Point Fault";
                asil_contribution: ASIL_B;
                diagnostic_coverage: 99%;
                residual_risk: 0.15FIT;
            }
            
            recommended_actions: [
                "Verify voltage monitor response time <100µs",
                "Add power-good LED for visual indication",
                "Consider input fuse coordination"
            ];
        },
        
        {
            item_number: 2;
            component: "U1 - LM2596 Switching Regulator";
            handle: "regulator";
            function: "Convert 12V battery power to regulated 5V";
            
            failure_mode: "Overvoltage runaway (feedback failure)";
            failure_rate: 8FIT;
            
            local_effect: "Unregulated 12V appears on 5V output";
            next_level_effect: "Overvoltage stress on downstream ECUs";
            system_effect: "Potential ECU damage and safety system failure";
            
            current_controls: {
                design: "Voltage monitor with 5.5V threshold, immediate shutdown";
                process: "Design review for feedback stability";
                verification: "Overvoltage injection testing";
            }
            
            severity: 9;  // Critical - component damage potential
            occurrence: 2;  // Low frequency (8FIT)
            detection: 1;   // Detected by overvoltage monitor
            rpn: 18;       // S×O×D = 9×2×1
            
            safety_analysis: {
                classification: "Single Point Fault";
                asil_contribution: ASIL_B;
                diagnostic_coverage: 95%;
                residual_risk: 0.4FIT;
                response_time: 60µs;  // Detection + shutdown
            }
            
            recommended_actions: [
                "Verify overvoltage threshold accuracy ±2%",
                "Test shutdown response under all load conditions",
                "Consider crowbar protection for faster response"
            ];
        },
        
        {
            item_number: 3;
            component: "U2 - Voltage Monitor";
            handle: "voltage_monitor";
            function: "Monitor 5V output and provide fault detection";
            
            failure_mode: "Stuck high output (no fault indication)";
            failure_rate: 20FIT;
            
            local_effect: "No fault signal during actual overvoltage condition";
            next_level_effect: "Overvoltage protection not activated";
            system_effect: "ECU damage from undetected overvoltage";
            
            current_controls: {
                design: "Built-in self-test with reference check";
                process: "Monitor IC qualification and incoming test";
                verification: "Periodic self-test validation";
            }
            
            severity: 9;   // Critical - safety function failure
            occurrence: 2;  // Low frequency (20FIT)
            detection: 2;   // Detected by self-test (85% coverage)
            rpn: 36;       // S×O×D = 9×2×2
            
            safety_analysis: {
                classification: "Latent Fault";
                asil_contribution: ASIL_B;
                diagnostic_coverage: 85%;  // Via self-test
                residual_risk: 3FIT;
                test_interval: 100ms;
            }
            
            recommended_actions: [
                "Implement self-test every 100ms as designed",
                "Add external test stimulus capability",
                "Consider dual monitor for redundancy"
            ];
        },
        
        {
            item_number: 4;
            component: "D1 - TVS Diode";
            handle: "tvs";
            function: "Protect against input voltage transients";
            
            failure_mode: "Open circuit (no clamping)";
            failure_rate: 2FIT;
            
            local_effect: "No transient voltage protection";
            next_level_effect: "Switching regulator vulnerable to load dump";
            system_effect: "Possible regulator damage during transient events";
            
            current_controls: {
                design: "Automotive-grade TVS with margin";
                process: "Incoming inspection and qualification";
                verification: "Load dump testing per ISO 16750";
            }
            
            severity: 6;   // Moderate - latent failure
            occurrence: 1;  // Very low (2FIT)
            detection: 4;   // Only detected during transient event
            rpn: 24;       // S×O×D = 6×1×4
            
            safety_analysis: {
                classification: "Latent Fault";
                asil_contribution: QM;  // Input protection not safety-critical
                diagnostic_coverage: 0%; // No active monitoring
                residual_risk: 2FIT;
            }
            
            recommended_actions: [
                "Consider adding TVS health monitoring",
                "Verify load dump test coverage",
                "Add redundant protection if required"
            ];
        }
    ];
    
    // FMEDA summary for ISO 26262
    fmeda_summary {
        total_components_analyzed: 12;
        safety_relevant_components: 8;
        
        failure_rate_summary: {
            lambda_total: 247FIT;
            lambda_safe: 89FIT;
            lambda_dangerous_detected: 145FIT;
            lambda_dangerous_undetected: 13FIT;
        }
        
        calculated_metrics: {
            spfm: 91.5%;  // (λ_safe + λ_dangerous_detected) / λ_total
            lfm: 68.2%;   // Includes latent fault detection
            pmhf: 42FIT;  // λ_dangerous_undetected + λ_rf
            
            asil_b_compliance: {
                spfm_requirement: 90%;  // PASS (91.5% > 90%)
                lfm_requirement: 60%;   // PASS (68.2% > 60%)
                pmhf_requirement: 100FIT; // PASS (42FIT < 100FIT)
                overall_status: COMPLIANT;
            }
        }
        
        safety_mechanism_effectiveness: {
            primary_mechanisms: {
                voltage_monitor: {
                    coverage: 92%;
                    response_time: 60µs;
                    confidence: 0.95;
                }
            }
            
            latent_mechanisms: {
                self_test: {
                    coverage: 87%;
                    test_interval: 100ms;
                    confidence: 0.90;
                }
            }
        }
    }
}
```

---

## Tool Analysis Capabilities Demonstrated

### 1. Context-Sensitive Analysis
- **Same component, different effects**: LM2596 failure has different severity based on what it powers
- **Intent-driven effects**: Failure analysis tied to violated design intents
- **Topology awareness**: Buck converter patterns recognized and analyzed appropriately

### 2. Physics-Based Failure Effects  
- **Electrical simulation**: Tool calculates actual voltage/current during failures
- **Thermal analysis**: Component heating and derating effects
- **EMC impact**: Oscillation modes and noise coupling effects

### 3. Safety Metrics Calculation
- **Automatic SPFM/LFM/PMHF**: Calculated from actual component data and coverage
- **Coverage analysis**: Real diagnostic capability assessment
- **Gap identification**: Automatic detection of safety requirement shortfalls

### 4. Stable References
- **Component handles**: `regulator`, `voltage_monitor` survive board layout changes
- **Refactor resilience**: Safety analysis remains valid through design iterations
- **Traceability**: Clear mapping from safety requirements to implementation

This example shows how the multi-phase architecture enables productive parallel work while maintaining comprehensive, accurate safety analysis throughout the design process.

<function_calls>
<invoke name="TodoWrite">
<parameter name="todos">[{"content": "Create comprehensive power supply example", "status": "completed", "activeForm": "Creating comprehensive power supply example"}, {"content": "Design FMEA/FMEDA extraction process", "status": "completed", "activeForm": "Designing FMEA/FMEDA extraction process"}, {"content": "Define automatic failure effect generation", "status": "completed", "activeForm": "Defining automatic failure effect generation"}]