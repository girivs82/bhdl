# BHDL Functional Safety Extension - Complete Design Document

## Executive Summary

This document presents a comprehensive design for integrating ISO 26262 functional safety as a first-class feature in BHDL (Board Hardware Description Language). The design maintains BHDL's minimalist philosophy while enabling complete safety lifecycle support through clear separation of concerns between board designers and safety engineers.

## Table of Contents
1. [Motivation and Goals](#1-motivation-and-goals)
2. [Core Design Principles](#2-core-design-principles)
3. [Architecture Overview](#3-architecture-overview)
4. [Language Extensions](#4-language-extensions)
5. [Workflow and Separation of Concerns](#5-workflow-and-separation-of-concerns)
6. [Automatic Safety Metrics Calculation](#6-automatic-safety-metrics-calculation)
7. [FMEA/FMEDA Generation System](#7-fmeafmeda-generation-system)
8. [Implementation Architecture](#8-implementation-architecture)
9. [Complete Examples](#9-complete-examples)
10. [Tool Integration](#10-tool-integration)
11. [Migration and Adoption](#11-migration-and-adoption)
12. [Key Innovations Summary](#12-key-innovations-summary)

---

## 1. Motivation and Goals

### 1.1 The Problem
- Functional safety (ISO 26262) requires rigorous analysis of hardware designs
- Board designers focus on electrical correctness and functionality
- Safety engineers focus on failure modes, diagnostic coverage, and safety metrics
- Current tools force awkward coupling between these domains
- Safety analysis is often done in spreadsheets, disconnected from actual design

### 1.2 Design Goals
- **Separation of Concerns**: Board designers and safety engineers work independently
- **No New Keywords**: Reuse existing BHDL constructs (`entity`, `requirements`, `attributes`)
- **Automatic Analysis**: Calculate safety metrics (SPFM, LFM, PMHF) automatically
- **Bidirectional Flow**: Safety requirements → board implementation → safety validation
- **ISO 26262 Compliance**: Support all ASIL levels and required metrics
- **Progressive Adoption**: Safety features are optional, don't affect existing designs

---

## 2. Core Design Principles

### 2.1 Minimal Language Extension
- Use existing `entity` construct for all components (no `safety_part` keyword)
- Use `attributes` to mark safety properties (PSM, LSM, ASIL levels)
- Use unified `requirements` block for all requirement types
- Introduce only one new construct: `safety_entity` for safety analysis overlay

### 2.2 Clear Domain Separation
```
Board Designer Domain          Safety Engineer Domain
├── Circuits                   ├── Safety Analysis
├── Components                 ├── Failure Modes
├── Connections                ├── Safety Mechanisms
└── Measurements               └── Coverage Metrics
```

### 2.3 Virtual Mechanisms for Gap Analysis
Safety engineers can specify "virtual" components that don't exist yet, generating requirements for board designers.

---

## 3. Architecture Overview

### 3.1 Two-Layer Architecture

```bhdl
// Layer 1: Physical Design (Board Designer)
board PowerSupply {
    // Pure electrical design
    @VCC -> monitor: VoltageMonitor().VIN;
    monitor.FAULT -> mcu.GPIO1;
}

// Layer 2: Safety Overlay (Safety Engineer)
safety_module PowerSupplySafety {
    analyzes board PowerSupply;
    
    // Maps safety requirements to physical components
    safety_function VoltageProtection {
        primary_mechanism {
            uses = PowerSupply.monitor;
            coverage = 90%;
        }
    }
}
```

### 3.2 Data Flow

```
1. Safety Engineer defines safety_module with requirements
2. Virtual mechanisms generate requirements for Board Designer
3. Board Designer implements physical design
4. Safety Analyzer validates implementation
5. Metrics calculated automatically from actual design
```

---

## 4. Language Extensions

### 4.1 Unified Requirements Block

```bhdl
requirements {
    // Single construct, differentiated by type attribute
    REQ_001: requirement {
        type = safety;           // safety | functional | performance | component
        asil = ASIL_B;          // Only for safety requirements
        description = "Monitor shall detect overvoltage";
        allocated_to = VoltageMonitor;  // Module name
        
        // Board-verifiable constraints
        constraints {
            detection_range = 0..6V;
            response_time < 100us;
            accuracy = ±2%;
        }
        
        // Safety targets (calculated by tools)
        safety_targets {
            diagnostic_coverage >= 90%;
            latent_fault_metric >= 60%;
        }
    }
}
```

### 4.2 Module Safety Attributes

```bhdl
entity VoltageMonitor {
    // Standard entity with safety attributes
    attribute safety_mechanism = "primary";  // PSM
    attribute asil = ASIL_B;
    attribute diagnostic_coverage = 90%;
    
    // Failure modes for FMEDA
    attribute failure_modes = {
        stuck_high: { rate: 20FIT, detectable: true },
        stuck_low: { rate: 20FIT, detectable: true },
        drift: { rate: 10FIT, detectable: false }
    };
    
    // Regular entity interface
    pin VIN: signal in;
    pin FAULT: signal out;
    pin TEST: signal in optional;
}
```

### 4.3 Safety Module (Analysis Overlay)

```bhdl
safety_module PowerSafety {
    // References existing board
    analyzes board PowerSupply;
    
    // Safety functions map to board components
    safety_function VoltageProtection {
        asil = ASIL_B;
        
        // Primary Safety Mechanism
        primary_mechanism {
            uses = PowerSupply.voltage_monitor;  // Actual component
            coverage = 90%;
            failure_rate = 50FIT;
        }
        
        // Latent fault Safety Mechanism
        latent_mechanism {
            uses = PowerSupply.self_test;  // Or virtual if missing
            test_interval = 100ms;
            coverage_of_psm = 85%;
        }
    }
    
    // Virtual mechanism (doesn't exist yet)
    safety_function CurrentProtection {
        primary_mechanism {
            virtual CurrentMonitor {
                // These become requirements for board designer
                detection_range = 0..2A;
                response_time < 1ms;
                accuracy = ±5%;
            }
        }
    }
}
```

---

## 5. Workflow and Separation of Concerns

### 5.1 Board Designer Workflow

```bhdl
// Step 1: Design the circuit
board MyBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Focus on electrical design
    @VCC -> reg: VoltageRegulator().IN;
    reg.OUT -> @3V3;
    
    // Add monitoring (functional need, not safety-driven)
    @3V3 -> monitor: VoltageMonitor().VIN;
    monitor.FAULT -> mcu.GPIO;
}

// Step 2: Implement requirements from safety
requirements {
    // Auto-generated from safety analysis
    REQ_GEN_001: requirement {
        type = component;
        description = "Add current monitoring";
        
        // Board designer can verify these
        constraints {
            range = 0..2A;          // ✓ Measurable
            response_time < 1ms;    // ✓ Measurable
            accuracy = ±5%;         // ✓ Measurable
        }
    }
}

// Step 3: Validate implementation
validation {
    REQ_GEN_001: {
        component = current_sensor;
        measured_range = 0..2.5A;    // Exceeds requirement
        measured_response = 800us;    // Meets requirement
        measured_accuracy = ±3%;      // Exceeds requirement
        status = SATISFIED;
    }
}
```

### 5.2 Safety Engineer Workflow

```bhdl
// Step 1: Analyze board for safety
safety_module BoardSafety {
    analyzes board MyBoard;
    
    // Step 2: Define safety functions
    safety_function PowerMonitoring {
        asil = ASIL_B;
        
        // Map to existing components
        primary_mechanism {
            uses = MyBoard.monitor;
            // Tool calculates coverage from component properties
        }
        
        // Identify gaps
        latent_mechanism {
            virtual SelfTest {  // Doesn't exist
                test_interval = 100ms;
                coverage_target = 85%;
            }
        }
    }
    
    // Step 3: Review calculated metrics
    calculated_metrics {
        spfm = 92%;  // Calculated by tool
        lfm = 65%;   // Calculated by tool
        pmhf = 45FIT; // Calculated by tool
    }
}
```

### 5.3 Bidirectional Communication

```bhdl
// Safety → Board: Requirements
gap_analysis {
    missing: SelfTest mechanism;
    generates: REQ_ADD_SELFTEST;
}

// Board → Safety: Constraints
board_constraints {
    max_components = 100;  // Space limit
    generates: "Consider time-redundancy instead of dual monitors";
}

// Safety → Board: Alternative
safety_alternative {
    instead_of: dual_monitors;
    propose: single_monitor with faster self_test;
}
```

---

## 6. Automatic Safety Metrics Calculation

### 6.1 FMEDA Data Model

```bhdl
// Component provides base failure data
entity CurrentSensor {
    attribute failure_data = {
        lambda_total: 50FIT,
        failure_modes: [
            { mode: "open", rate: 15FIT, safe: false },
            { mode: "short", rate: 15FIT, safe: false },
            { mode: "drift", rate: 18FIT, safe: false },
            { mode: "noise", rate: 2FIT, safe: true }
        ]
    };
    
    attribute self_test = {
        available: true,
        detects: ["open", "short"],  // Can't detect drift
        interval: 100ms
    };
}
```

### 6.2 Safety Metrics Calculation by bhdl-safety

```rust
// bhdl-safety crate implementation
pub struct SafetyAnalyzer {
    pub fn calculate_metrics(&self, board: &Board, safety_module: &SafetyModule) -> Metrics {
        // Single Point Fault Metric (SPFM)
        let spfm = self.calculate_spfm(board, safety_module);
        
        // Latent Fault Metric (LFM)
        let lfm = self.calculate_lfm(board, safety_module);
        
        // Probabilistic Metric for random Hardware Failures (PMHF)
        let pmhf = self.calculate_pmhf(board, safety_module);
        
        Metrics { spfm, lfm, pmhf }
    }
    
    fn calculate_spfm(&self, board: &Board, safety: &SafetyModule) -> f64 {
        // SPFM = Σ(λ_SPF_detected + λ_safe) / Σ(λ_total)
        
        let mut lambda_spf_detected = 0.0;
        let mut lambda_safe = 0.0;
        let mut lambda_total = 0.0;
        
        for component in board.safety_relevant_components() {
            let failure_data = component.get_failure_data();
            lambda_total += failure_data.lambda_total;
            
            // Check if component is covered by PSM
            if let Some(psm) = safety.get_psm_for(component) {
                let coverage = psm.coverage;
                lambda_spf_detected += failure_data.lambda_dangerous * coverage;
            }
            
            lambda_safe += failure_data.lambda_safe;
        }
        
        (lambda_spf_detected + lambda_safe) / lambda_total * 100.0
    }
    
    fn calculate_lfm(&self, board: &Board, safety: &SafetyModule) -> f64 {
        // LFM = Σ(λ_RF_detected_by_PSM + λ_MPF_detected + λ_safe) / Σ(λ_total)
        
        let mut lambda_detected = 0.0;
        let mut lambda_latent_detected = 0.0;
        let mut lambda_safe = 0.0;
        let mut lambda_total = 0.0;
        
        for component in board.safety_relevant_components() {
            let failure_data = component.get_failure_data();
            lambda_total += failure_data.lambda_total;
            lambda_safe += failure_data.lambda_safe;
            
            // PSM coverage
            if let Some(psm) = safety.get_psm_for(component) {
                lambda_detected += failure_data.lambda_dangerous * psm.coverage;
                
                // LSM coverage of residual faults
                if let Some(lsm) = safety.get_lsm_for(psm) {
                    let residual = failure_data.lambda_dangerous * (1.0 - psm.coverage);
                    lambda_latent_detected += residual * lsm.coverage;
                }
            }
        }
        
        (lambda_detected + lambda_latent_detected + lambda_safe) / lambda_total * 100.0
    }
}
```

### 6.3 Automatic Coverage Calculation

```bhdl
// Safety engineer specifies target
safety_module Analysis {
    safety_function Protection {
        primary_mechanism {
            uses = Board.monitor;
            coverage = ?;  // Let tool calculate
        }
    }
}

// Tool calculates actual coverage
calculated_coverage {
    component = Board.monitor;
    failure_modes = component.failure_data.modes;
    detectable = filter(modes, monitor.self_test.detects);
    coverage = sum(detectable.rates) / sum(all.rates);
    result = 85%;  // Calculated automatically
}
```

### 6.4 Gap Detection and Reporting

```bhdl
// Tool generates gap report
safety_gaps {
    GAP_001: {
        requirement: "ASIL_B requires SPFM >= 90%";
        calculated: "SPFM = 87%";
        gap: "3% shortage";
        
        suggestions: [
            "Add redundant monitor (increases SPFM by ~5%)",
            "Add self-test to existing monitor (increases by ~3%)",
            "Use higher-reliability component (reduces λ_dangerous)"
        ];
    }
    
    GAP_002: {
        requirement: "ASIL_B requires LFM >= 60%";
        calculated: "LFM = 55%";
        gap: "5% shortage";
        
        suggestions: [
            "Add power-on self-test (increases LFM by ~8%)",
            "Reduce test interval to 50ms (increases by ~5%)"
        ];
    }
}
```

---

## 7. FMEA/FMEDA Generation System

### 7.1 Three-Tier Failure Data Architecture

The system uses three levels of failure data sources, with automatic application based on component type and context:

```bhdl
// TIER 1: Built-in IEC 62380 Models (for passives)
// Automatically applied to resistors, capacitors, inductors
builtin_failure_models {
    resistor: {
        source: "IEC 62380 TR";
        failure_modes: [
            { mode: "open", distribution: 40% },
            { mode: "short", distribution: 30% },
            { mode: "drift_high", distribution: 20% },
            { mode: "drift_low", distribution: 10% }
        ];
        // FIT calculation based on IEC 62380 formula
        lambda_base = f(power_rating, temperature, voltage_stress);
    }
    
    capacitor: {
        source: "IEC 62380 TR";
        failure_modes: [
            { mode: "open", distribution: 60% },
            { mode: "short", distribution: 25% },
            { mode: "loss_of_capacitance", distribution: 10% },
            { mode: "increased_esr", distribution: 5% }
        ];
        // Different formulas for ceramic, electrolytic, film
        lambda_base = f(type, voltage_rating, temperature, ripple_current);
    }
    
    inductor: {
        source: "IEC 62380 TR";
        failure_modes: [
            { mode: "open", distribution: 45% },
            { mode: "short_turn", distribution: 30% },
            { mode: "saturation", distribution: 15% },
            { mode: "increased_dcr", distribution: 10% }
        ];
        lambda_base = f(inductance, current_rating, temperature, core_material);
    }
}

// TIER 2: Component Library Definitions
// In bhdl-stdlib or vendor libraries
entity LM7805 {
    attribute failure_data = {
        source: "STMicroelectronics datasheet";
        lambda_total: 30FIT;  // At 25°C, nominal load
        failure_modes: [
            { mode: "no_output", rate: 10FIT },
            { mode: "low_output", rate: 8FIT },
            { mode: "high_output", rate: 7FIT },
            { mode: "oscillation", rate: 5FIT }
        ];
        environmental_factors: {
            temperature: { 85C: 2.5x, 125C: 5x },
            voltage_stress: { 80%: 1.5x, 90%: 2x }
        };
    };
}

// TIER 3: Project-Specific Overrides (in safety_module)
safety_module PowerSafety {
    failure_data_overrides {
        // Override specific component instances
        PowerSupply.vreg {  // Using handle, not RefDes!
            source: "Field return data from previous project";
            lambda_total: 45FIT;  // Higher than datasheet
            additional_modes: [
                { mode: "thermal_shutdown_stuck", rate: 5FIT }
            ];
        }
        
        // Provide data for custom components
        PowerSupply.custom_asic {
            source: "Internal reliability testing";
            lambda_total: 150FIT;
            failure_modes: [
                { mode: "logic_stuck_high", rate: 50FIT },
                { mode: "logic_stuck_low", rate: 50FIT },
                { mode: "timing_violation", rate: 30FIT },
                { mode: "power_domain_failure", rate: 20FIT }
            ];
        }
    }
}
```

### 7.2 Context-Aware Failure Effects Definition

Failure effects are context-dependent and defined by safety engineers based on how components are used in the specific circuit:

```bhdl
safety_module PowerSupplySafety {
    analyzes board PowerSupply;
    
    // FAILURE EFFECTS DEFINITIONS - Context-specific
    failure_effects {
        // Using component handles for stable references
        PowerSupply.vreg {  // LM7805 voltage regulator
            context: "Provides 5V to MCU and sensors";
            
            failure_modes {
                "no_output" {
                    local_effect: "0V on 5V rail";
                    system_effect: "Complete system shutdown - safe state";
                    safety_impact: "Safe failure - system cannot operate";
                    severity: 2;  // Low - fails safe
                }
                
                "high_output" {
                    local_effect: "7-12V on 5V rail (unregulated input)";
                    system_effect: "MCU and sensor damage, possible fire hazard";
                    safety_impact: "Dangerous - can damage downstream components";
                    severity: 9;  // Critical
                    
                    affected_components: [
                        PowerSupply.mcu,
                        PowerSupply.sensor1,
                        PowerSupply.sensor2
                    ];
                }
                
                "low_output" {
                    local_effect: "3-4V on 5V rail";
                    system_effect: "MCU brown-out, sensors give incorrect readings";
                    safety_impact: "Latent fault - system operates incorrectly";
                    severity: 7;  // High
                }
            }
        }
        
        // Group similar components
        group bypass_capacitors {
            components: [input_cap, output_cap, mcu_bypass];  // Handles
            context: "Power supply filtering and decoupling";
            
            common_failure_modes {
                "open" {
                    local_effect: "Loss of local bypassing/filtering";
                    system_effect: "Increased noise on power rail";
                    safety_impact: "Degraded EMC performance";
                    severity: 4;  // Low to Moderate
                }
                
                "short" {
                    local_effect: "Power rail shorted to ground";
                    system_effect: "Power supply shutdown or current limit";
                    safety_impact: "Safe failure if protection present";
                    severity: 2;  // Low - fails safe
                }
            }
        }
    }
    
    // HIERARCHICAL EFFECT DEFINITION
    subsystem_effects {
        power_subsystem {
            includes: [vreg, current_monitor, protection_diode];
            
            failure_scenarios {
                "loss_of_regulation" {
                    cause: "vreg.no_output OR vreg.low_output";
                    subsystem_effect: "Unstable power to all consumers";
                    mitigation: "Backup power path or safe shutdown";
                }
            }
        }
    }
    
    system_effects {
        application: "Electric Power Steering";
        
        critical_functions {
            "assist_torque" {
                failure_impact: "Driver requires more effort";
                acceptable_degradation: "50% assist loss for < 10 seconds";
            }
        }
    }
}
```

### 7.3 Automatic Failure Effect Generation with Intent

The tool leverages its understanding of circuit topology, component roles, simulation capabilities, and designer intent to automatically generate most failure effects:

```bhdl
// The 'for' intent keyword provides invaluable context
board PowerSupply {
    // Intent makes the purpose crystal clear
    net input_filtering: @VCC -> input_cap: Cap(100uF) -> GND
        for noise_filtering(ripple < 50mV, frequency > 100kHz);
    
    net bulk_storage: @VCC -> bulk_cap: Cap(470uF) -> GND
        for energy_storage(holdup_time: 10ms);
    
    net mcu_decoupling: @3V3 -> mcu_bypass: Cap(100nF) -> GND
        for decoupling();  // Tool finds MCU from topology
}

// Tool automatically generates precise failure effects
generated_failure_effects {
    input_cap {
        detected_intent: "noise_filtering(ripple < 50mV, frequency > 100kHz)";
        
        failure_modes {
            "open" {
                // Tool knows EXACTLY what this cap is for
                local_effect: "Loss of high-frequency filtering above 100kHz";
                
                // Tool simulates and measures against intent
                measured_impact: "Ripple increases to 180mV at 150kHz (VIOLATES INTENT)";
                
                // Tool traces downstream impact
                system_effect: "Switching noise couples into analog measurements";
                
                severity: 7;  // High - intent violation
                
                // Generated by simulation, not guesswork
                confidence: 0.92;
            }
        }
    }
    
    bulk_cap {
        detected_intent: "energy_storage(holdup_time: 10ms)";
        
        failure_modes {
            "open" {
                // Tool simulates power interruption
                local_effect: "No bulk storage, holdup time drops to 0.5ms";
                system_effect: "System resets on 5ms power glitches (INTENT VIOLATED)";
                operational_impact: "Unexpected resets during power transitions";
                severity: 8;  // Critical - explicit requirement violated
            }
        }
    }
    
    mcu_bypass {
        detected_intent: "decoupling()";
        associated_ic: "mcu (determined by 3mm proximity)";  // Auto-detected
        
        failure_modes {
            "open" {
                local_effect: "MCU loses high-frequency decoupling";
                // Tool looked up MCU's requirements automatically
                system_effect: "MCU clock jitter exceeds spec by 300%";
                severity: 6;  // Moderate - degraded operation
            }
        }
    }
}
```

### 7.4 Smart Pattern Recognition

The tool recognizes common circuit patterns and applies domain-specific failure analysis:

```bhdl
pattern_based_analysis {
    // Pattern: Buck converter detected
    buck_converter_1 {
        detected_topology: "Buck converter with synchronous rectification";
        components: {
            high_side: mosfet1,
            low_side: mosfet2,
            inductor: l1,
            output_cap: cout,
            feedback: [rfb1, rfb2]
        };
        
        // Tool applies buck-specific failure analysis
        automatic_failure_effects {
            mosfet1 {
                "short" {
                    effect: "Output = Input (12V on 3.3V rail) - CATASTROPHIC";
                    physics: "No switching, inductor becomes wire";
                    simulated: true;
                }
            }
            
            l1 {
                "saturated" {
                    effect: "Lost regulation, thermal stress";
                    physics: "Inductor becomes resistor, no energy storage";
                    simulated: "Temperature rise 45°C in 2 seconds";
                }
            }
        }
    }
}
```

### 7.5 FMEDA Generation with Quantitative Analysis

```bhdl
generated_fmeda {
    safety_goal: "Prevent overcurrent to critical components";
    asil: ASIL_B;
    
    // Component analysis with handles
    component: PowerSupply.vreg {
        handle: "vreg";  // Stable identifier
        refdes: "U1";    // Generated during synthesis
        type: "LM7805";
        
        failure_analysis: [
            {
                // From IEC 62380 or component library
                failure_mode: "high_output";
                failure_rate: 7FIT;
                
                // From safety engineer or automatic generation
                local_effect: "7-12V on 5V rail";
                system_effect: "MCU and sensor damage";
                
                // From safety analysis
                severity: 9;
                occurrence: 2;  // Based on FIT rate
                detection: 3;   // Monitor present
                rpn: 54;       // S × O × D
                
                // From safety mechanisms
                diagnostic_coverage: 90%;
                residual_risk: 0.7FIT;
            }
        ];
    }
    
    // Roll up to safety metrics
    calculated_metrics {
        spfm: 91.2%;  // Meets ASIL B (>90%)
        lfm: 67.8%;   // Meets ASIL B (>60%)
        pmhf: 12FIT;  // Meets ASIL B (<100FIT)
        
        compliance: {
            spfm: "PASS",
            lfm: "PASS",
            pmhf: "PASS"
        }
    }
}
```

---

## 8. Implementation Architecture

### 7.1 New Crate: bhdl-safety

```rust
// bhdl-safety/src/lib.rs
pub mod analyzer;
pub mod metrics;
pub mod requirements;
pub mod virtual_mechanisms;
pub mod iso26262;

use bhdl_analyzer::AnalysisResult;
use bhdl_netlist::Netlist;

pub struct SafetyAnalyzer {
    safety_modules: Vec<SafetyModule>,
    requirements: RequirementDatabase,
    failure_database: FailureModeDatabase,
}

impl SafetyAnalyzer {
    pub fn analyze(
        &self,
        board: &Board,
        safety_module: &SafetyModule,
        netlist: &Netlist,
    ) -> SafetyAnalysisResult {
        // Map safety functions to physical components
        let mappings = self.map_safety_to_physical(safety_module, board);
        
        // Identify virtual mechanisms (gaps)
        let gaps = self.find_virtual_mechanisms(safety_module);
        
        // Generate requirements from gaps
        let generated_reqs = self.generate_requirements(gaps);
        
        // Calculate safety metrics
        let metrics = self.calculate_metrics(board, mappings);
        
        // Validate against ISO 26262
        let validation = self.validate_iso26262(metrics, safety_module.asil);
        
        SafetyAnalysisResult {
            mappings,
            gaps,
            generated_requirements: generated_reqs,
            metrics,
            validation,
            suggestions: self.generate_suggestions(metrics, validation),
        }
    }
}
```

### 7.2 Integration with Existing Pipeline

```
Current Pipeline:
Parser → AST → Analyzer (8 passes) → Synthesizer → Netlist

Extended Pipeline:
Parser → AST → Analyzer (8 passes) → Synthesizer → Netlist
                                          ↓
                                   Safety Analyzer (Pass 9)
                                          ↓
                                   ┌──────────────┐
                                   │ FMEDA Report │
                                   │ Safety Case  │
                                   │ Gap Analysis │
                                   └──────────────┘
```

### 7.3 Data Structures

```rust
// Core safety data structures
pub struct SafetyModule {
    pub name: String,
    pub analyzes_board: String,
    pub safety_functions: Vec<SafetyFunction>,
    pub requirements: Vec<Requirement>,
}

pub struct SafetyFunction {
    pub name: String,
    pub asil: ASILLevel,
    pub primary_mechanisms: Vec<Mechanism>,
    pub latent_mechanisms: Vec<Mechanism>,
}

pub struct Mechanism {
    pub mechanism_type: MechanismType,  // Primary, Latent
    pub uses: MechanismTarget,          // Physical or Virtual
    pub coverage: Option<f64>,          // May be calculated
    pub properties: HashMap<String, Value>,
}

pub enum MechanismTarget {
    Physical(String),  // References board component
    Virtual(VirtualComponent),  // Doesn't exist yet
}

pub struct VirtualComponent {
    pub name: String,
    pub properties: HashMap<String, Value>,
    pub generates_requirements: Vec<Requirement>,
}

pub struct SafetyMetrics {
    pub spfm: f64,  // Single Point Fault Metric
    pub lfm: f64,   // Latent Fault Metric
    pub pmhf: f64,  // Probabilistic Metric for Hardware Failures
    pub diagnostic_coverage: HashMap<String, f64>,
}
```

---

## 9. Complete Examples

### 9.1 Simple LED with Safety

```bhdl
// Board designer's view
board SimpleLED {
    power VCC = 5V @ 100mA;
    ground GND;
    
    @VCC -> r1: Res(330Ω).1 -> led: LED(red).A;
    led.K -> @GND;
}

// Safety engineer's view
safety_module SimpleLEDSafety {
    analyzes board SimpleLED;
    
    safety_function OvercurrentProtection {
        asil = QM;  // Low safety relevance
        
        // Current limiting resistor acts as safety mechanism
        primary_mechanism {
            uses = SimpleLED.r1;
            function = "current_limiting";
            coverage = 100%;  // Passive protection
        }
    }
    
    calculated_metrics {
        // Tool calculates
        spfm = 100%;  // Resistor always limits current
        lfm = 100%;   // No latent faults in passive component
        pmhf = 0.5FIT; // Very low failure rate
    }
}
```

### 9.2 Automotive Power Supply

```bhdl
// board_design/automotive_psu.bhdl
board AutomotivePSU {
    power VIN = 12V @ 3A { attribute source = "automotive_battery"; }
    power VCC = 5V @ 2A;
    ground GND;
    
    // Input protection
    @VIN -> tvs: TVSDiode(40V).K;
    tvs.A -> @GND;
    
    // Voltage regulation
    @VIN -> reg: VoltageRegulator(5V, 2A) {
        attribute part_number = "LM2596";
        attribute efficiency = 85%;
    };
    reg.OUT -> @VCC;
    reg.GND -> @GND;
    
    // Output monitoring
    @VCC -> vmon: VoltageMonitor {
        attribute threshold = 5.5V;
        attribute response_time = 100us;
    };
    vmon.FAULT -> mcu.GPIO_FAULT;
    
    // Self-test capability
    mcu.GPIO_TEST -> vmon.TEST;
    vmon.TEST_RESULT -> mcu.GPIO_TEST_RESULT;
}

// safety_analysis/automotive_psu_safety.bhdl
safety_module AutomotivePSUSafety {
    analyzes board AutomotivePSU;
    
    requirements {
        SG_001: requirement {
            type = safety_goal;
            asil = ASIL_B;
            description = "Prevent damage from power supply faults";
            hazard = "Overvoltage damages safety-critical components";
        }
        
        FSR_001: requirement {
            type = safety;
            derived_from = SG_001;
            description = "Detect and mitigate overvoltage";
            allocated_to = OutputProtection;
        }
    }
    
    safety_function OutputProtection {
        asil = ASIL_B;
        implements = FSR_001;
        
        // Primary mechanism - voltage monitor
        primary_mechanism {
            uses = AutomotivePSU.vmon;
            coverage = ?;  // Tool calculates: 92%
            
            failure_modes_covered = [
                "regulator_overvoltage",
                "reference_drift_high",
                "feedback_open"
            ];
        }
        
        // Latent mechanism - self-test
        latent_mechanism {
            uses = AutomotivePSU.SelfTestSequence;
            test_interval = 100ms;
            coverage_of_psm = ?;  // Tool calculates: 85%
            
            detects_psm_faults = [
                "monitor_stuck_low",
                "monitor_no_response",
                "threshold_drift"
            ];
        }
    }
    
    safety_function InputProtection {
        asil = ASIL_B;
        
        primary_mechanism {
            uses = AutomotivePSU.tvs;
            coverage = 99%;  // TVS diodes very reliable
            failure_rate = 10FIT;
        }
        
        // No LSM needed for passive protection
    }
    
    // Tool calculates these
    calculated_metrics {
        spfm = 91.5%;  // Meets ASIL B (>90%)
        lfm = 65.2%;   // Meets ASIL B (>60%)
        pmhf = 42FIT;  // Meets ASIL B (<100FIT)
        
        breakdown {
            vmon_contribution: {
                lambda_dangerous: 50FIT;
                detected_by_psm: 46FIT;
                detected_by_lsm: 3.4FIT;
                residual: 0.6FIT;
            }
            
            tvs_contribution: {
                lambda_dangerous: 10FIT;
                detected: 9.9FIT;
                residual: 0.1FIT;
            }
        }
    }
}

// validation/psu_validation.bhdl
validation PSUValidation {
    board = AutomotivePSU;
    safety = AutomotivePSUSafety;
    
    // All physical mechanisms exist
    assert all(safety.mechanisms where !virtual).exist_in(board);
    
    // Metrics meet requirements
    assert safety.calculated_metrics.spfm >= 90%;
    assert safety.calculated_metrics.lfm >= 60%;
    assert safety.calculated_metrics.pmhf < 100FIT;
    
    status = VALIDATED;
}
```

### 9.3 Gap Analysis Example

```bhdl
// Initial board without current monitoring
board InitialDesign {
    power VCC = 5V @ 2A;
    @VCC -> load;
}

// Safety analysis identifies gap
safety_module SafetyAnalysis {
    analyzes board InitialDesign;
    
    safety_function OvercurrentProtection {
        asil = ASIL_B;
        
        primary_mechanism {
            // This doesn't exist!
            virtual CurrentMonitor {
                detection_range = 0..2.5A;
                response_time < 1ms;
                accuracy = ±5%;
            }
        }
    }
}

// Generated requirements (automatic)
requirements {
    REQ_GEN_001: requirement {
        type = component;
        source = "SafetyAnalysis.OvercurrentProtection.CurrentMonitor";
        description = "Add current monitoring capability";
        
        // From virtual component attributes
        constraints {
            detection_range = 0..2.5A;
            response_time < 1ms;
            accuracy = ±5%;
        }
        
        assigned_to = board_designer;
        priority = safety_critical;
        asil = ASIL_B;
    }
}

// Board designer updates design
board UpdatedDesign {
    power VCC = 5V @ 2A;
    
    // Added based on requirement
    @VCC -> isense: CurrentSensor {
        attribute range = 0..3A;
        attribute response = 500us;
        attribute accuracy = ±3%;
    };
    isense.OUT -> @VCC_MONITORED;
    isense.FAULT -> mcu.OVERCURRENT;
    
    @VCC_MONITORED -> load;
}

// Safety validates update
safety_module SafetyValidation {
    analyzes board UpdatedDesign;
    
    safety_function OvercurrentProtection {
        primary_mechanism {
            uses = UpdatedDesign.isense;  // Now exists!
            coverage = 94%;  // Calculated
        }
    }
    
    validation {
        REQ_GEN_001 = SATISFIED;
    }
}
```

---

## 10. Tool Integration

### 9.1 Command Line Interface

```bash
# Analyze safety
bhdl safety analyze --board design.bhdl --safety safety.bhdl

# Generate gap report
bhdl safety gaps --board design.bhdl --safety safety.bhdl --output gaps.md

# Calculate metrics
bhdl safety metrics --board design.bhdl --safety safety.bhdl

# Generate requirements from virtual mechanisms
bhdl safety gen-reqs --safety safety.bhdl --output requirements.bhdl

# Validate implementation
bhdl safety validate --board design.bhdl --requirements reqs.bhdl

# Generate ISO 26262 documentation
bhdl safety iso26262 --board design.bhdl --safety safety.bhdl --asil B
```

### 9.2 IDE Integration

```typescript
// VS Code extension features
interface SafetyFeatures {
    // Real-time safety metrics in status bar
    showMetrics(): { spfm: number, lfm: number, pmhf: number };
    
    // Highlight components by safety relevance
    highlightSafetyComponents(): void;
    
    // Show gaps as problems
    showGaps(): Diagnostic[];
    
    // Quick fixes for gaps
    provideQuickFixes(): CodeAction[];
    
    // Generate safety overlay template
    generateSafetyModule(): string;
}
```

### 9.3 CI/CD Integration

```yaml
# GitHub Actions workflow
name: Safety Analysis
on: [push, pull_request]

jobs:
  safety-check:
    steps:
      - uses: actions/checkout@v2
      
      - name: Install BHDL
        run: cargo install bhdl-cli
      
      - name: Run Safety Analysis
        run: bhdl safety analyze --board src/board.bhdl --safety safety/analysis.bhdl
      
      - name: Check Metrics
        run: |
          bhdl safety metrics --board src/board.bhdl --safety safety/analysis.bhdl
          # Fails if metrics don't meet ASIL requirements
      
      - name: Generate Report
        run: bhdl safety report --format html --output safety-report.html
      
      - name: Upload Report
        uses: actions/upload-artifact@v2
        with:
          name: safety-report
          path: safety-report.html
```

---

## 11. Migration and Adoption

### 10.1 Adoption Strategy

#### Phase 1: Board Design as Usual
- Teams continue designing boards normally
- No safety keywords or concepts required
- Full backward compatibility

#### Phase 2: Safety Overlay
- Safety engineers create `safety_module` overlays
- Analyze existing designs for safety
- Generate gap reports

#### Phase 3: Requirement Integration
- Virtual mechanisms generate requirements
- Board designers implement requirements
- Automatic validation

#### Phase 4: Full Integration
- Bidirectional flow established
- Automatic metrics calculation
- ISO 26262 documentation generation

### 10.2 Training Path

```bhdl
// Level 1: Board Designer
// - No safety knowledge required
// - Focus on implementing requirements
// - Validate measurable constraints

// Level 2: Safety Analyst  
// - Create safety overlays
// - Define safety functions
// - Review calculated metrics

// Level 3: Safety Architect
// - Design safety concepts
// - Allocate ASIL levels
// - Define safety mechanisms
```

### 10.3 Tool Rollout

1. **Pilot Project**: Single board with safety overlay
2. **Department Adoption**: One team uses for all boards
3. **Company Rollout**: Standardize across organization
4. **Supply Chain**: Share safety entities with suppliers

---

## Summary

This design provides:

1. **Clean Separation**: Board designers and safety engineers work independently
2. **No New Keywords**: Uses existing BHDL constructs with attributes
3. **Automatic Analysis**: Safety metrics calculated by tools
4. **ISO 26262 Compliance**: Full support for automotive functional safety
5. **Progressive Adoption**: Can be added to existing designs incrementally
6. **Bidirectional Flow**: Requirements → Implementation → Validation
7. **Gap Analysis**: Identifies missing safety mechanisms automatically
8. **Complete Traceability**: From safety goals to implementation

The key innovation is the `safety_module` overlay that analyzes boards without modifying them, maintaining separation of concerns while enabling comprehensive safety analysis.

---

## 12. Key Innovations Summary

### 12.1 Revolutionary Features

1. **Automatic FMEA/FMEDA Generation**: The tool understands circuit physics and can automatically generate 90% of failure effects through simulation and pattern recognition.

2. **Intent-Driven Safety Analysis**: The `for` keyword provides semantic clarity that transforms component-level analysis into intent-level verification.

3. **Three-Tier Failure Data System**:
   - IEC 62380 formulas for passives (automatic)
   - Component library data from vendors
   - Project-specific overrides by safety engineers

4. **Context-Aware Effect Generation**: The tool combines topology analysis, SPICE simulation, functional simulation, and intent to generate precise failure effects.

5. **Stable Component Handles**: Uses designer-defined handles instead of generated reference designators, making safety analysis refactor-proof.

6. **Separation of Concerns**: Board designers focus on circuits, safety engineers on safety requirements, tool bridges the gap automatically.

### 12.2 Workflow Efficiency Gains

- **Before**: Safety engineer manually analyzes hundreds of components
- **After**: Safety engineer defines high-level safety functions, tool handles component details

- **Before**: FMEA in disconnected spreadsheets  
- **After**: FMEA generated directly from actual design with simulation results

- **Before**: Design changes require complete re-analysis
- **After**: Automatic update of safety analysis when design changes

### 12.3 Technical Advantages

1. **Physics-Based**: Uses actual electrical simulation, not just rules
2. **Pattern-Aware**: Recognizes common circuits (buck converters, filters, etc.)
3. **Intent-Preserving**: Failure effects tied to violated intents
4. **Traceable**: Every effect linked to simulation results
5. **Quantitative**: Real FIT rates and coverage percentages
6. **Standards-Compliant**: Follows ISO 26262 and IEC 62380 methodologies

This makes BHDL not just a hardware description language with safety features, but a revolutionary platform that fundamentally transforms how functional safety analysis is performed in hardware design.