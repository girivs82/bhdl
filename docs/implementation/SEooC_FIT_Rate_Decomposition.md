# SEooC FIT Rate Decomposition for Component Libraries
## Handling Vendor-Provided Aggregate Safety Data

### Overview

Many semiconductor vendors provide **SEooC (Safety Element out of Context)** FIT rates as aggregate values rather than detailed failure mode breakdowns. These typically include:

- **Permanent Die Failures**: Functional failures of the silicon die
- **Package Failures**: Bond wire failures, pin connectivity issues
- **Transient Failures**: Single event upsets, soft errors (sometimes)

The BHDL component library system decomposes these aggregate rates into specific failure modes that can be analyzed by the safety synthesizer.

---

## Vendor Data Format

### Typical SEooC Documentation

```
Component: LTC2954-3.3 Voltage Supervisor
Safety Manual: LTC2954-SM Rev B

Failure Rate Data (at 55°C, FIT):
├── Permanent Die Failures: 12 FIT
├── Package Failures: 8 FIT  
├── Transient Failures: 2 FIT
└── Total: 22 FIT

Notes:
- Die failures include all functional silicon failures
- Package failures include bond wire and pin connectivity
- Transient failures are SEU/latchup (recoverable)
- Data based on FIDES 2009 methodology
- Qualification: AEC-Q100 Grade 1
```

### Limited Failure Mode Information

Vendors typically don't provide detailed breakdowns like:
- "Comparator stuck high: 3 FIT"  
- "Reference drift: 2 FIT"
- "Output driver failure: 4 FIT"

Instead, they group everything into broad categories for liability and IP protection reasons.

---

## BHDL Decomposition Strategy

### 1. Die Functional Failures

Map the **permanent die FIT rate** to functional failure modes based on component architecture:

```bhdl
// File: bhdl-stdlib/src/voltage_regulators/supervisors/ltc2954.bhdl
module LTC2954 {
    // Vendor SEooC data
    seooc_data {
        vendor: "Analog Devices";
        document: "LTC2954-SM Rev B";
        permanent_die_failures: 12FIT;
        package_failures: 8FIT;
        transient_failures: 2FIT;
        total_fit_rate: 22FIT;
        test_conditions: "55C, FIDES 2009 methodology";
    }
    
    // Decompose die failures into functional failure modes
    failure_modes {
        // Permanent die failures - decomposed from 12 FIT total
        die_functional: {
            // Primary functional failure - most of the die FIT budget
            rate: 12FIT;
            description: "Silicon die functional failure";
            failure_mechanism: "Internal logic failure, reference drift, or comparator malfunction";
            
            // Decomposition based on typical voltage supervisor architecture
            subfailure_modes: {
                comparator_stuck_high: {
                    rate: 4FIT;  // ~33% of die failures
                    description: "Voltage comparator stuck at high output";
                    observable_symptom: "No fault indication during undervoltage";
                    detectable_by_self_test: true;
                    self_test_coverage: 85%;
                },
                
                comparator_stuck_low: {
                    rate: 3FIT;  // ~25% of die failures  
                    description: "Voltage comparator stuck at low output";
                    observable_symptom: "Continuous fault indication";
                    detectable_by_self_test: true;
                    self_test_coverage: 90%;
                },
                
                reference_drift: {
                    rate: 3FIT;  // ~25% of die failures
                    description: "Internal voltage reference degradation";
                    observable_symptom: "Threshold voltage drift beyond specification";
                    detectable_by_self_test: true;
                    self_test_coverage: 95%;
                },
                
                internal_logic_failure: {
                    rate: 2FIT;  // ~17% of die failures
                    description: "Digital control logic malfunction";
                    observable_symptom: "Erratic or no switching behavior";
                    detectable_by_self_test: true;
                    self_test_coverage: 80%;
                }
            };
            
            // Decomposition rationale
            decomposition_basis: {
                method: "Functional block analysis of typical voltage supervisor architecture";
                reference: "IEEE 1413.1 - Semiconductor device reliability analysis";
                assumptions: [
                    "Comparator accounts for largest portion due to analog sensitivity",
                    "Reference circuit is second most critical for accuracy",
                    "Digital logic has lowest failure rate due to process maturity"
                ];
                confidence: 0.7;  // Engineering estimate based on architecture
            }
        },
        
        // Package failures - decomposed from 8 FIT total
        package_connectivity: {
            rate: 8FIT;
            description: "Package and interconnect failures";
            failure_mechanism: "Bond wire failure, package crack, or pin connectivity loss";
            
            subfailure_modes: {
                pin_to_pin_short: {
                    rate: 2FIT;  // ~25% of package failures
                    description: "Short circuit between adjacent pins";
                    affected_pins: "Any two pins";
                    observable_symptom: "Unexpected electrical connectivity";
                    detectable_externally: true;  // Circuit behavior changes
                },
                
                pin_short_to_ground: {
                    rate: 2FIT;  // ~25% of package failures
                    description: "Pin shorted to package ground";
                    affected_pins: "Any signal pin to substrate";
                    observable_symptom: "Pin stuck at ground potential";
                    detectable_externally: true;
                },
                
                pin_short_to_vcc: {
                    rate: 1.5FIT;  // ~19% of package failures
                    description: "Pin shorted to power supply";
                    affected_pins: "Any signal pin to VCC";
                    observable_symptom: "Pin stuck at supply voltage";
                    detectable_externally: true;
                },
                
                pin_open_circuit: {
                    rate: 2FIT;  // ~25% of package failures
                    description: "Loss of pin connectivity";
                    affected_pins: "Any pin";
                    observable_symptom: "No electrical connection to pin";
                    detectable_externally: true;
                },
                
                bond_wire_failure: {
                    rate: 0.5FIT;  // ~6% of package failures
                    description: "Internal bond wire break";
                    affected_pins: "Random pin";
                    observable_symptom: "Pin becomes non-functional";
                    detectable_externally: true;
                }
            };
            
            decomposition_basis: {
                method: "Package failure mode distribution from reliability studies";
                reference: "JEDEC JEP148 - Reliability qualification of semiconductor devices";
                pin_count_factor: 8;  // LTC2954 has 8 pins
                confidence: 0.8;  // Well-established package failure distributions
            }
        },
        
        // Transient failures - usually small portion
        transient_upsets: {
            rate: 2FIT;
            description: "Single event upsets and soft errors";
            failure_mechanism: "Cosmic ray or alpha particle induced state change";
            
            subfailure_modes: {
                single_event_upset: {
                    rate: 1.5FIT;  // ~75% of transient failures
                    description: "Temporary logic state upset";
                    observable_symptom: "Brief incorrect output, self-recovering";
                    duration: "Microseconds to milliseconds";
                    detectable_externally: false;  // Too brief to detect
                },
                
                latchup: {
                    rate: 0.5FIT;  // ~25% of transient failures
                    description: "Parasitic thyristor activation";
                    observable_symptom: "High current consumption, possible permanent damage";
                    recovery_method: "Power cycle required";
                    detectable_externally: true;  // Current monitoring can detect
                }
            };
            
            decomposition_basis: {
                method: "Single event effect modeling";
                reference: "JESD89A - Measurement and reporting of alpha particle and terrestrial cosmic ray-induced soft errors";
                technology_node: "Mature CMOS process (>180nm)";
                confidence: 0.6;  // Less mature modeling for transients
            }
        }
    };
    
    // Self-diagnostic capabilities (component-specific)
    self_diagnostic_capabilities {
        test_method: internal_reference_comparison;
        detectable_failure_types: [
            comparator_stuck_high,
            comparator_stuck_low, 
            reference_drift,
            internal_logic_failure
        ];
        
        // Coverage against die failures only (package failures not detectable by BIST)
        die_failure_coverage: 87%;  // Weighted average of subfailure mode coverage
        package_failure_coverage: 0%;  // Self-test cannot detect package issues
        transient_failure_coverage: 0%;  // SEU too brief to catch
        
        overall_self_test_coverage: 47%;  // (12FIT * 87%) / 22FIT total
    };
}
```

### 2. Package Failure Decomposition Rules

Standard decomposition for **package failures** based on pin count and package type:

```bhdl
// Generic package failure decomposition
package_failure_decomposition {
    // Distribution percentages based on reliability studies
    failure_distribution: {
        pin_to_pin_short: 25%;        // Adjacent pin shorts most common
        pin_short_to_ground: 25%;     // Substrate shorts
        pin_short_to_vcc: 20%;        // Power rail shorts  
        pin_open_circuit: 25%;        // Bond wire/lead failures
        internal_bond_wire: 5%;       // Internal interconnect
    };
    
    // Pin-specific failure rates
    calculate_pin_failures(total_package_fit: fit_rate, pin_count: number) -> PinFailureRates {
        base_rate_per_pin = total_package_fit / pin_count;
        
        return PinFailureRates {
            any_pin_to_adjacent: base_rate_per_pin * 0.25,
            any_pin_to_ground: base_rate_per_pin * 0.25,
            any_pin_to_vcc: base_rate_per_pin * 0.20,
            any_pin_open: base_rate_per_pin * 0.25,
            bond_wire_any_pin: base_rate_per_pin * 0.05,
        }
    }
    
    // Package-specific adjustments
    package_type_factors: {
        QFN: 1.0;          // Baseline
        SOIC: 0.8;          // More robust leads
        BGA: 1.5;          // More complex interconnect
        TSSOP: 1.2;        // Fine pitch increases risk
        TO220: 0.6;        // Very robust package
    };
}
```

### 3. Microcontroller Example with Large FIT Budget

```bhdl
// File: bhdl-stdlib/src/microcontrollers/stm32f103.bhdl
module STM32F103 {
    seooc_data {
        vendor: "STMicroelectronics";
        document: "STM32F103-SM Rev 3.1";
        permanent_die_failures: 45FIT;  // Complex SoC
        package_failures: 25FIT;        // 64-pin LQFP
        transient_failures: 8FIT;       // Larger memory arrays
        total_fit_rate: 78FIT;
    }
    
    failure_modes {
        // Die failures decomposed by functional block
        die_functional: {
            rate: 45FIT;
            
            subfailure_modes: {
                cpu_core_failure: {
                    rate: 15FIT;  // ~33% - most critical block
                    description: "ARM Cortex-M3 core malfunction";
                    observable_symptoms: ["no_code_execution", "incorrect_calculations", "hang_state"];
                    detectable_by: software_watchdog;
                },
                
                memory_failure: {
                    rate: 12FIT;  // ~27% - large area, many transistors
                    description: "Flash or SRAM memory failure"; 
                    observable_symptoms: ["data_corruption", "program_execution_errors"];
                    detectable_by: memory_test_patterns;
                },
                
                peripheral_failure: {
                    rate: 10FIT;  // ~22% - GPIO, timers, ADC, etc.
                    description: "Peripheral block malfunction";
                    observable_symptoms: ["gpio_stuck", "timer_not_counting", "adc_incorrect"];
                    detectable_by: peripheral_self_test;
                },
                
                clock_pll_failure: {
                    rate: 5FIT;   // ~11% - analog blocks more sensitive
                    description: "Clock generation or PLL failure";
                    observable_symptoms: ["no_clock", "wrong_frequency", "clock_jitter"];
                    detectable_by: clock_monitoring;
                },
                
                power_management_failure: {
                    rate: 3FIT;   // ~7% - voltage regulators, brown-out
                    description: "Internal power management failure";
                    observable_symptoms: ["voltage_regulation_loss", "brown_out_false_trigger"];
                    detectable_by: voltage_monitoring;
                }
            };
            
            decomposition_basis: {
                method: "Functional block area and complexity analysis";
                reference: "ARM Cortex-M3 reliability analysis + STM32 peripheral complexity";
                confidence: 0.6;  // Complex SoC decomposition less certain
            }
        },
        
        // Package failures for 64-pin LQFP
        package_connectivity: {
            rate: 25FIT;
            pin_count: 64;
            package_type: "LQFP64";
            
            // Use standard decomposition formula
            subfailure_modes: auto_generate_from_pin_count(25FIT, 64);
        },
        
        transient_upsets: {
            rate: 8FIT;  // Higher due to memory arrays
            
            subfailure_modes: {
                memory_seu: {
                    rate: 6FIT;   // ~75% - memory most susceptible
                    description: "Single event upset in memory";
                    affected_area: "Flash or SRAM arrays";
                    detectable_by: ECC_if_available;
                },
                
                register_seu: {
                    rate: 1.5FIT; // ~19% - processor registers
                    description: "Single event upset in CPU registers";
                    affected_area: "Register file";
                    detectable_by: software_cross_checking;
                },
                
                latchup: {
                    rate: 0.5FIT; // ~6% - less common in modern process
                    description: "Parasitic thyristor activation";
                    recovery: power_cycle_required;
                }
            }
        }
    };
    
    // Built-in diagnostic capabilities
    self_diagnostic_capabilities {
        cpu_diagnostics: {
            watchdog_timer: available;
            instruction_test: march_test_capability;
            register_test: walking_ones_pattern;
        };
        
        memory_diagnostics: {
            flash_crc: available;
            ram_test: march_algorithms;
            ecc: not_available;  // Depends on specific variant
        };
        
        peripheral_diagnostics: {
            gpio_loopback: available;
            timer_cross_check: available;
            adc_self_test: reference_voltage_test;
        };
        
        // Coverage calculation
        die_failure_coverage: 65%;     // Software diagnostics can catch most issues
        package_failure_coverage: 15%; // Some pin failures detectable by I/O tests
        transient_failure_coverage: 30%; // Software cross-checking helps
        
        overall_self_test_coverage: 52%; // Weighted average
    };
}
```

---

## Tool Integration

### Automatic Decomposition

```rust
// Tool automatically decomposes vendor SEooC data
pub struct SeoocDecomposer {
    package_rules: PackageFailureRules,
    die_architecture_models: DieArchitectureModels,
}

impl SeoocDecomposer {
    pub fn decompose_vendor_fit_data(&self, seooc_data: &SeoocData) -> DetailedFailureModes {
        let mut failure_modes = Vec::new();
        
        // Decompose die failures based on component architecture
        let die_failures = self.decompose_die_failures(
            seooc_data.permanent_die_failures,
            seooc_data.component_type
        );
        failure_modes.extend(die_failures);
        
        // Decompose package failures based on pin count and package type
        let package_failures = self.package_rules.decompose_package_failures(
            seooc_data.package_failures,
            seooc_data.pin_count,
            seooc_data.package_type
        );
        failure_modes.extend(package_failures);
        
        // Handle transient failures
        let transient_failures = self.decompose_transient_failures(
            seooc_data.transient_failures,
            seooc_data.technology_node
        );
        failure_modes.extend(transient_failures);
        
        DetailedFailureModes {
            modes: failure_modes,
            total_fit: seooc_data.total_fit_rate,
            decomposition_confidence: self.calculate_confidence(&seooc_data),
            basis: seooc_data.decomposition_method.clone(),
        }
    }
    
    fn decompose_die_failures(&self, total_die_fit: FitRate, component_type: ComponentType) -> Vec<FailureMode> {
        match component_type {
            ComponentType::VoltageRegulator => {
                vec![
                    FailureMode {
                        name: "regulation_failure".to_string(),
                        rate: total_die_fit * 0.4,  // 40% - main function
                        description: "Loss of voltage regulation".to_string(),
                    },
                    FailureMode {
                        name: "protection_failure".to_string(), 
                        rate: total_die_fit * 0.3,  // 30% - protection circuits
                        description: "Overcurrent or thermal protection failure".to_string(),
                    },
                    FailureMode {
                        name: "reference_drift".to_string(),
                        rate: total_die_fit * 0.2,  // 20% - voltage reference
                        description: "Internal voltage reference degradation".to_string(),
                    },
                    FailureMode {
                        name: "control_logic_failure".to_string(),
                        rate: total_die_fit * 0.1,  // 10% - digital control
                        description: "Control logic malfunction".to_string(),
                    }
                ]
            },
            
            ComponentType::VoltageSupervisor => {
                vec![
                    FailureMode {
                        name: "comparator_stuck_high".to_string(),
                        rate: total_die_fit * 0.35,
                        description: "Voltage comparator stuck at high output".to_string(),
                    },
                    FailureMode {
                        name: "comparator_stuck_low".to_string(),
                        rate: total_die_fit * 0.25,
                        description: "Voltage comparator stuck at low output".to_string(),
                    },
                    FailureMode {
                        name: "reference_drift".to_string(),
                        rate: total_die_fit * 0.25,
                        description: "Internal voltage reference degradation".to_string(),
                    },
                    FailureMode {
                        name: "internal_logic_failure".to_string(),
                        rate: total_die_fit * 0.15,
                        description: "Digital control logic malfunction".to_string(),
                    }
                ]
            },
            
            ComponentType::Microcontroller => {
                // More complex decomposition based on architectural blocks
                self.decompose_microcontroller_failures(total_die_fit)
            },
            
            _ => {
                // Generic decomposition for unknown component types
                vec![
                    FailureMode {
                        name: "die_functional_failure".to_string(),
                        rate: total_die_fit,
                        description: "Silicon die functional failure".to_string(),
                    }
                ]
            }
        }
    }
}
```

### Safety Analysis Integration

```bhdl
// Tool uses decomposed failure modes for circuit analysis
safety_analysis_with_seooc {
    component: power_monitor: LTC2954;
    seooc_data: extracted_from_vendor_manual;
    
    // Tool automatically decomposes 12 FIT die failures
    decomposed_die_failures: {
        comparator_stuck_high: 4FIT;
        comparator_stuck_low: 3FIT;
        reference_drift: 3FIT;
        internal_logic_failure: 2FIT;
    };
    
    // Tool analyzes each failure mode in circuit context
    context_analysis: {
        circuit_role: "5V output monitoring for automotive BCM";
        downstream_asil: ASIL_B;
        protection_target: "ECU overvoltage protection";
        
        failure_effects: {
            comparator_stuck_high: {
                local_effect: "No fault indication during real overvoltage";
                system_effect: "Undetected ECU damage from overvoltage";
                severity: 9;  // Critical - defeats safety mechanism
                detectable_by_self_test: true;
                coverage: 85%;
            },
            
            comparator_stuck_low: {
                local_effect: "Continuous false fault indication";
                system_effect: "Nuisance system shutdown";
                severity: 4;  // Low - safe but inconvenient
                detectable_by_self_test: true;
                coverage: 90%;
            }
            
            // ... other failure modes analyzed in context
        }
    };
    
    // Final safety metrics using decomposed data
    calculated_metrics: {
        spfm: 89.3%;  // Based on actual failure mode breakdown
        lfm: 91.7%;   // Self-test coverage against specific modes
        pmhf: 2.1FIT; // Residual risk from undetected portions
    };
}
```

---

## Benefits of SEooC Decomposition

### 1. **Works with Real Vendor Data**
- **Standard format**: Most vendors provide SEooC FIT rates
- **Liability protection**: Vendors don't need to reveal detailed failure mechanisms
- **Qualification aligned**: Matches existing automotive qualification processes

### 2. **Engineering-Based Breakdown** 
- **Architecture-driven**: Decomposition based on component functional blocks
- **Confidence tracking**: Engineering estimates with documented assumptions
- **Updateable**: Can refine decomposition as more data becomes available

### 3. **Tool Integration**
- **Automatic processing**: Tool decomposes vendor data into usable failure modes
- **Context analysis**: Each failure mode analyzed in actual circuit application
- **Traceability**: Clear path from vendor SEooC data to final safety metrics

### 4. **Practical Implementation**
- **No vendor cooperation needed**: Works with existing safety manuals
- **Standard methodology**: Consistent decomposition rules across components
- **Scalable approach**: Handles simple supervisors to complex microcontrollers

This approach enables **realistic safety analysis** using **actually available vendor data** while maintaining the **component library architecture** and **context-sensitive effect analysis** that makes the BHDL safety system effective.