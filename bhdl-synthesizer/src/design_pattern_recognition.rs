// Design Pattern Recognition Engine
// Automatically identifies common circuit patterns and applies design knowledge

use anyhow::{Result, Context};
use bhdl_netlist::{Netlist, InstanceId, NetId, ModuleId};
use bhdl_analyzer::AnalysisResult;
use std::collections::{HashMap, HashSet};
use log::{info, debug, warn};

/// Design pattern recognition engine
pub struct DesignPatternRecognizer {
    /// Known circuit patterns
    patterns: Vec<CircuitPattern>,
    
    /// Pattern matching results
    recognized_patterns: Vec<RecognizedPattern>,
    
    /// Pattern-specific design rules
    pattern_rules: HashMap<String, Vec<DesignRule>>,
    
    /// Component role inference
    component_roles: HashMap<InstanceId, ComponentRole>,
}

impl DesignPatternRecognizer {
    pub fn new() -> Self {
        let mut recognizer = Self {
            patterns: Vec::new(),
            recognized_patterns: Vec::new(),
            pattern_rules: HashMap::new(),
            component_roles: HashMap::new(),
        };
        
        // Initialize with common circuit patterns
        recognizer.initialize_standard_patterns();
        recognizer
    }
    
    /// Recognize patterns in the given netlist
    pub fn recognize_patterns(
        &mut self,
        netlist: &Netlist,
        analysis: &AnalysisResult,
    ) -> Result<PatternRecognitionReport> {
        info!("Starting design pattern recognition");
        
        // Clear previous results
        self.recognized_patterns.clear();
        self.component_roles.clear();
        
        // Phase 1: Analyze circuit topology
        let topology = self.analyze_circuit_topology(netlist)?;
        
        // Phase 2: Infer component roles
        self.infer_component_roles(netlist, &topology, analysis)?;
        
        // Phase 3: Recognize circuit patterns
        for pattern in &self.patterns.clone() {
            if let Some(recognized) = self.match_pattern(pattern, netlist, &topology)? {
                info!("Recognized pattern: {}", recognized.pattern_name);
                self.recognized_patterns.push(recognized);
            }
        }
        
        // Phase 4: Generate design recommendations
        let recommendations = self.generate_design_recommendations(netlist)?;
        
        // Phase 5: Create comprehensive report
        let report = PatternRecognitionReport {
            topology_analysis: topology,
            component_roles: self.component_roles.clone(),
            recognized_patterns: self.recognized_patterns.clone(),
            design_recommendations: recommendations,
            pattern_coverage: self.calculate_pattern_coverage(netlist),
        };
        
        info!("Pattern recognition complete: {} patterns found", 
              self.recognized_patterns.len());
        
        Ok(report)
    }
    
    /// Initialize standard circuit patterns
    fn initialize_standard_patterns(&mut self) {
        // Power supply patterns
        self.add_linear_regulator_pattern();
        self.add_switching_regulator_pattern();
        self.add_voltage_divider_pattern();
        self.add_current_limiting_pattern();
        
        // Filter patterns
        self.add_rc_lowpass_pattern();
        self.add_decoupling_pattern();
        self.add_emc_filter_pattern();
        
        // Protection patterns
        self.add_overvoltage_protection_pattern();
        self.add_esd_protection_pattern();
        self.add_thermal_protection_pattern();
        
        // Amplifier patterns
        self.add_opamp_non_inverting_pattern();
        self.add_opamp_inverting_pattern();
        self.add_differential_amplifier_pattern();
        
        // Digital patterns
        self.add_pullup_pulldown_pattern();
        self.add_crystal_oscillator_pattern();
        self.add_reset_circuit_pattern();
        
        info!("Initialized {} standard circuit patterns", self.patterns.len());
    }
    
    /// Add linear regulator pattern
    fn add_linear_regulator_pattern(&mut self) {
        let pattern = CircuitPattern {
            name: "linear_regulator".to_string(),
            description: "Linear voltage regulator with input/output filtering".to_string(),
            pattern_type: PatternType::PowerSupply,
            required_components: vec![
                ComponentMatcher {
                    role: ComponentRole::VoltageRegulator,
                    component_types: vec!["LM7805".to_string(), "AMS1117".to_string(), "LM317".to_string()],
                    min_count: 1,
                    max_count: 1,
                }
            ],
            optional_components: vec![
                ComponentMatcher {
                    role: ComponentRole::InputFilter,
                    component_types: vec!["Cap".to_string()],
                    min_count: 0,
                    max_count: 2,
                },
                ComponentMatcher {
                    role: ComponentRole::OutputFilter,
                    component_types: vec!["Cap".to_string()],
                    min_count: 0,
                    max_count: 2,
                },
            ],
            connectivity_rules: vec![
                ConnectivityRule {
                    description: "Input power flows to regulator VIN".to_string(),
                    source_role: ComponentRole::PowerSource,
                    target_role: ComponentRole::VoltageRegulator,
                    connection_type: ConnectionType::Power,
                },
                ConnectivityRule {
                    description: "Regulator VOUT provides regulated power".to_string(),
                    source_role: ComponentRole::VoltageRegulator,
                    target_role: ComponentRole::Load,
                    connection_type: ConnectionType::Power,
                },
            ],
            design_knowledge: vec![
                "Input capacitor: 10-100µF for stability".to_string(),
                "Output capacitor: 1-10µF for transient response".to_string(),
                "Thermal derating required above 1W dissipation".to_string(),
                "Dropout voltage affects efficiency significantly".to_string(),
            ],
        };
        
        self.patterns.push(pattern);
        
        // Add pattern-specific design rules
        self.pattern_rules.insert("linear_regulator".to_string(), vec![
            DesignRule {
                name: "thermal_management".to_string(),
                description: "Linear regulators require thermal management above 1W".to_string(),
                rule_type: RuleType::Warning,
                condition: "power_dissipation > 1.0".to_string(),
                recommendation: "Add heatsink or consider switching regulator".to_string(),
            },
            DesignRule {
                name: "input_filtering".to_string(),
                description: "Input filtering improves regulation and stability".to_string(),
                rule_type: RuleType::Recommendation,
                condition: "input_capacitor < 10e-6".to_string(),
                recommendation: "Add 10-100µF input capacitor".to_string(),
            },
        ]);
    }
    
    /// Add switching regulator pattern
    fn add_switching_regulator_pattern(&mut self) {
        let pattern = CircuitPattern {
            name: "switching_regulator".to_string(),
            description: "Switching voltage regulator with feedback network".to_string(),
            pattern_type: PatternType::PowerSupply,
            required_components: vec![
                ComponentMatcher {
                    role: ComponentRole::SwitchingController,
                    component_types: vec!["TPS54331".to_string(), "LM2596".to_string()],
                    min_count: 1,
                    max_count: 1,
                },
                ComponentMatcher {
                    role: ComponentRole::Inductor,
                    component_types: vec!["Ind".to_string()],
                    min_count: 1,
                    max_count: 1,
                },
            ],
            optional_components: vec![
                ComponentMatcher {
                    role: ComponentRole::FeedbackNetwork,
                    component_types: vec!["Res".to_string()],
                    min_count: 0,
                    max_count: 4,
                },
                ComponentMatcher {
                    role: ComponentRole::CompensationNetwork,
                    component_types: vec!["Cap".to_string(), "Res".to_string()],
                    min_count: 0,
                    max_count: 3,
                },
            ],
            connectivity_rules: vec![
                ConnectivityRule {
                    description: "Inductor in series with switch output".to_string(),
                    source_role: ComponentRole::SwitchingController,
                    target_role: ComponentRole::Inductor,
                    connection_type: ConnectionType::Signal,
                },
                ConnectivityRule {
                    description: "Feedback network from output to FB pin".to_string(),
                    source_role: ComponentRole::FeedbackNetwork,
                    target_role: ComponentRole::SwitchingController,
                    connection_type: ConnectionType::Feedback,
                },
            ],
            design_knowledge: vec![
                "Inductor value: L = Vout*(Vin-Vout)/(0.3*Iout*fsw*Vin)".to_string(),
                "Output capacitor: Low ESR required for ripple reduction".to_string(),
                "Feedback network sets output voltage: Vout = Vref*(1 + R1/R2)".to_string(),
                "Compensation network critical for stability".to_string(),
            ],
        };
        
        self.patterns.push(pattern);
    }
    
    /// Add voltage divider pattern
    fn add_voltage_divider_pattern(&mut self) {
        let pattern = CircuitPattern {
            name: "voltage_divider".to_string(),
            description: "Resistive voltage divider for reference or sensing".to_string(),
            pattern_type: PatternType::SignalConditioning,
            required_components: vec![
                ComponentMatcher {
                    role: ComponentRole::DividerResistor,
                    component_types: vec!["Res".to_string()],
                    min_count: 2,
                    max_count: 2,
                },
            ],
            optional_components: vec![],
            connectivity_rules: vec![
                ConnectivityRule {
                    description: "Resistors in series between supply and ground".to_string(),
                    source_role: ComponentRole::DividerResistor,
                    target_role: ComponentRole::DividerResistor,
                    connection_type: ConnectionType::Signal,
                },
            ],
            design_knowledge: vec![
                "Vout = Vin * R2 / (R1 + R2)".to_string(),
                "Loading effect: Consider input impedance of following stage".to_string(),
                "Power consumption: P = Vin² / (R1 + R2)".to_string(),
                "Tolerance: Output tolerance ≈ sqrt(tol1² + tol2²)".to_string(),
            ],
        };
        
        self.patterns.push(pattern);
    }
    
    /// Add current limiting pattern
    fn add_current_limiting_pattern(&mut self) {
        let pattern = CircuitPattern {
            name: "current_limiting".to_string(),
            description: "Series resistor for current limiting".to_string(),
            pattern_type: PatternType::Protection,
            required_components: vec![
                ComponentMatcher {
                    role: ComponentRole::SeriesResistor,
                    component_types: vec!["Res".to_string()],
                    min_count: 1,
                    max_count: 1,
                },
            ],
            optional_components: vec![],
            connectivity_rules: vec![
                ConnectivityRule {
                    description: "Resistor in series with load".to_string(),
                    source_role: ComponentRole::SeriesResistor,
                    target_role: ComponentRole::Load,
                    connection_type: ConnectionType::Signal,
                },
            ],
            design_knowledge: vec![
                "R = (Vsupply - Vload) / Idesired".to_string(),
                "Power dissipation: P = I² * R".to_string(),
                "Use 150% power rating minimum for safety".to_string(),
                "Consider temperature coefficient effects".to_string(),
            ],
        };
        
        self.patterns.push(pattern);
    }
    
    /// Add RC lowpass filter pattern
    fn add_rc_lowpass_pattern(&mut self) {
        let pattern = CircuitPattern {
            name: "rc_lowpass_filter".to_string(),
            description: "RC lowpass filter for noise suppression".to_string(),
            pattern_type: PatternType::Filter,
            required_components: vec![
                ComponentMatcher {
                    role: ComponentRole::FilterResistor,
                    component_types: vec!["Res".to_string()],
                    min_count: 1,
                    max_count: 1,
                },
                ComponentMatcher {
                    role: ComponentRole::FilterCapacitor,
                    component_types: vec!["Cap".to_string()],
                    min_count: 1,
                    max_count: 1,
                },
            ],
            optional_components: vec![],
            connectivity_rules: vec![
                ConnectivityRule {
                    description: "RC in series-parallel configuration".to_string(),
                    source_role: ComponentRole::FilterResistor,
                    target_role: ComponentRole::FilterCapacitor,
                    connection_type: ConnectionType::Signal,
                },
            ],
            design_knowledge: vec![
                "Cutoff frequency: fc = 1 / (2π * R * C)".to_string(),
                "Attenuation: -20dB/decade above cutoff".to_string(),
                "Phase shift: -45° at cutoff frequency".to_string(),
                "Loading: Consider source and load impedances".to_string(),
            ],
        };
        
        self.patterns.push(pattern);
    }
    
    /// Add decoupling capacitor pattern
    fn add_decoupling_pattern(&mut self) {
        let pattern = CircuitPattern {
            name: "power_decoupling".to_string(),
            description: "Power supply decoupling capacitors".to_string(),
            pattern_type: PatternType::PowerIntegrity,
            required_components: vec![
                ComponentMatcher {
                    role: ComponentRole::DecouplingCapacitor,
                    component_types: vec!["Cap".to_string()],
                    min_count: 1,
                    max_count: 10,
                },
            ],
            optional_components: vec![],
            connectivity_rules: vec![
                ConnectivityRule {
                    description: "Capacitor between power and ground".to_string(),
                    source_role: ComponentRole::DecouplingCapacitor,
                    target_role: ComponentRole::PowerSource,
                    connection_type: ConnectionType::Power,
                },
            ],
            design_knowledge: vec![
                "Multiple values: 100nF + 10µF for broadband decoupling".to_string(),
                "Placement: As close as possible to IC power pins".to_string(),
                "ESR/ESL: Low values critical for high-frequency performance".to_string(),
                "Self-resonant frequency should exceed switching frequency".to_string(),
            ],
        };
        
        self.patterns.push(pattern);
    }
    
    /// Add EMC filter pattern
    fn add_emc_filter_pattern(&mut self) {
        let pattern = CircuitPattern {
            name: "emc_filter".to_string(),
            description: "EMC/EMI filtering network".to_string(),
            pattern_type: PatternType::Filter,
            required_components: vec![
                ComponentMatcher {
                    role: ComponentRole::EMCInductor,
                    component_types: vec!["Ind".to_string(), "Ferrite".to_string()],
                    min_count: 1,
                    max_count: 2,
                },
                ComponentMatcher {
                    role: ComponentRole::EMCCapacitor,
                    component_types: vec!["Cap".to_string()],
                    min_count: 1,
                    max_count: 4,
                },
            ],
            optional_components: vec![],
            connectivity_rules: vec![
                ConnectivityRule {
                    description: "LC filter configuration".to_string(),
                    source_role: ComponentRole::EMCInductor,
                    target_role: ComponentRole::EMCCapacitor,
                    connection_type: ConnectionType::Signal,
                },
            ],
            design_knowledge: vec![
                "Common mode: Ferrite beads for high-frequency suppression".to_string(),
                "Differential mode: LC filter with appropriate cutoff".to_string(),
                "X2/Y2 capacitors for safety-critical applications".to_string(),
                "Impedance matching important at cable interfaces".to_string(),
            ],
        };
        
        self.patterns.push(pattern);
    }
    
    /// Add overvoltage protection pattern
    fn add_overvoltage_protection_pattern(&mut self) {
        let pattern = CircuitPattern {
            name: "overvoltage_protection".to_string(),
            description: "Overvoltage protection using TVS diodes".to_string(),
            pattern_type: PatternType::Protection,
            required_components: vec![
                ComponentMatcher {
                    role: ComponentRole::ProtectionDiode,
                    component_types: vec!["TVSDiode".to_string(), "Zener".to_string()],
                    min_count: 1,
                    max_count: 2,
                },
            ],
            optional_components: vec![
                ComponentMatcher {
                    role: ComponentRole::SeriesResistor,
                    component_types: vec!["Res".to_string(), "Fuse".to_string()],
                    min_count: 0,
                    max_count: 1,
                },
            ],
            connectivity_rules: vec![
                ConnectivityRule {
                    description: "TVS diode across protected line".to_string(),
                    source_role: ComponentRole::ProtectionDiode,
                    target_role: ComponentRole::PowerSource,
                    connection_type: ConnectionType::Protection,
                },
            ],
            design_knowledge: vec![
                "Clamp voltage must be below IC damage threshold".to_string(),
                "Peak pulse power rating must exceed expected transients".to_string(),
                "Response time: Sub-nanosecond for fast transients".to_string(),
                "Consider series resistance for current limiting".to_string(),
            ],
        };
        
        self.patterns.push(pattern);
    }
    
    /// Add ESD protection pattern  
    fn add_esd_protection_pattern(&mut self) {
        let pattern = CircuitPattern {
            name: "esd_protection".to_string(),
            description: "ESD protection for I/O interfaces".to_string(),
            pattern_type: PatternType::Protection,
            required_components: vec![
                ComponentMatcher {
                    role: ComponentRole::ESDProtection,
                    component_types: vec!["ESDDiode".to_string(), "TVSDiode".to_string()],
                    min_count: 1,
                    max_count: 4,
                },
            ],
            optional_components: vec![
                ComponentMatcher {
                    role: ComponentRole::SeriesResistor,
                    component_types: vec!["Res".to_string()],
                    min_count: 0,
                    max_count: 1,
                },
            ],
            connectivity_rules: vec![
                ConnectivityRule {
                    description: "ESD diodes on I/O lines".to_string(),
                    source_role: ComponentRole::ESDProtection,
                    target_role: ComponentRole::IOInterface,
                    connection_type: ConnectionType::Protection,
                },
            ],
            design_knowledge: vec![
                "Bidirectional protection for I/O lines".to_string(),
                "Low clamping voltage to protect sensitive ICs".to_string(),
                "Low capacitance for high-speed signals".to_string(),
                "IEC 61000-4-2 compliance for commercial products".to_string(),
            ],
        };
        
        self.patterns.push(pattern);
    }
    
    /// Add thermal protection pattern
    fn add_thermal_protection_pattern(&mut self) {
        let pattern = CircuitPattern {
            name: "thermal_protection".to_string(),
            description: "Thermal protection and monitoring".to_string(),
            pattern_type: PatternType::Protection,
            required_components: vec![
                ComponentMatcher {
                    role: ComponentRole::TemperatureSensor,
                    component_types: vec!["Thermistor".to_string(), "TempSensor".to_string()],
                    min_count: 1,
                    max_count: 2,
                },
            ],
            optional_components: vec![
                ComponentMatcher {
                    role: ComponentRole::ThermalSwitch,
                    component_types: vec!["ThermalSwitch".to_string()],
                    min_count: 0,
                    max_count: 1,
                },
            ],
            connectivity_rules: vec![
                ConnectivityRule {
                    description: "Temperature sensor monitors hot components".to_string(),
                    source_role: ComponentRole::TemperatureSensor,
                    target_role: ComponentRole::HeatSource,
                    connection_type: ConnectionType::Sensing,
                },
            ],
            design_knowledge: vec![
                "NTC thermistors for temperature monitoring".to_string(),
                "Thermal switches for automatic shutdown".to_string(),
                "Placement critical for accurate sensing".to_string(),
                "Consider thermal time constants".to_string(),
            ],
        };
        
        self.patterns.push(pattern);
    }
    
    /// Add non-inverting op-amp pattern
    fn add_opamp_non_inverting_pattern(&mut self) {
        let pattern = CircuitPattern {
            name: "opamp_non_inverting".to_string(),
            description: "Non-inverting operational amplifier".to_string(),
            pattern_type: PatternType::Amplifier,
            required_components: vec![
                ComponentMatcher {
                    role: ComponentRole::OpAmp,
                    component_types: vec!["OpAmp".to_string(), "LM358".to_string()],
                    min_count: 1,
                    max_count: 1,
                },
                ComponentMatcher {
                    role: ComponentRole::FeedbackResistor,
                    component_types: vec!["Res".to_string()],
                    min_count: 1,
                    max_count: 2,
                },
            ],
            optional_components: vec![],
            connectivity_rules: vec![
                ConnectivityRule {
                    description: "Feedback from output to inverting input".to_string(),
                    source_role: ComponentRole::OpAmp,
                    target_role: ComponentRole::FeedbackResistor,
                    connection_type: ConnectionType::Feedback,
                },
            ],
            design_knowledge: vec![
                "Gain: G = 1 + Rf/Rin".to_string(),
                "High input impedance, low output impedance".to_string(),
                "Non-inverting input determines output phase".to_string(),
                "Bandwidth decreases with increasing gain".to_string(),
            ],
        };
        
        self.patterns.push(pattern);
    }
    
    /// Add inverting op-amp pattern
    fn add_opamp_inverting_pattern(&mut self) {
        let pattern = CircuitPattern {
            name: "opamp_inverting".to_string(),
            description: "Inverting operational amplifier".to_string(),
            pattern_type: PatternType::Amplifier,
            required_components: vec![
                ComponentMatcher {
                    role: ComponentRole::OpAmp,
                    component_types: vec!["OpAmp".to_string()],
                    min_count: 1,
                    max_count: 1,
                },
                ComponentMatcher {
                    role: ComponentRole::InputResistor,
                    component_types: vec!["Res".to_string()],
                    min_count: 1,
                    max_count: 1,
                },
                ComponentMatcher {
                    role: ComponentRole::FeedbackResistor,
                    component_types: vec!["Res".to_string()],
                    min_count: 1,
                    max_count: 1,
                },
            ],
            optional_components: vec![],
            connectivity_rules: vec![
                ConnectivityRule {
                    description: "Input through series resistor to inverting input".to_string(),
                    source_role: ComponentRole::InputResistor,
                    target_role: ComponentRole::OpAmp,
                    connection_type: ConnectionType::Signal,
                },
                ConnectivityRule {
                    description: "Feedback from output to inverting input".to_string(),
                    source_role: ComponentRole::OpAmp,
                    target_role: ComponentRole::FeedbackResistor,
                    connection_type: ConnectionType::Feedback,
                },
            ],
            design_knowledge: vec![
                "Gain: G = -Rf/Rin".to_string(),
                "Virtual ground at inverting input".to_string(),
                "Input impedance equals input resistor".to_string(),
                "Output inverted with respect to input".to_string(),
            ],
        };
        
        self.patterns.push(pattern);
    }
    
    /// Add differential amplifier pattern
    fn add_differential_amplifier_pattern(&mut self) {
        let pattern = CircuitPattern {
            name: "differential_amplifier".to_string(),
            description: "Differential amplifier configuration".to_string(),
            pattern_type: PatternType::Amplifier,
            required_components: vec![
                ComponentMatcher {
                    role: ComponentRole::OpAmp,
                    component_types: vec!["OpAmp".to_string()],
                    min_count: 1,
                    max_count: 1,
                },
                ComponentMatcher {
                    role: ComponentRole::MatchedResistor,
                    component_types: vec!["Res".to_string()],
                    min_count: 4,
                    max_count: 4,
                },
            ],
            optional_components: vec![],
            connectivity_rules: vec![
                ConnectivityRule {
                    description: "Matched resistor network for CMRR".to_string(),
                    source_role: ComponentRole::MatchedResistor,
                    target_role: ComponentRole::OpAmp,
                    connection_type: ConnectionType::Signal,
                },
            ],
            design_knowledge: vec![
                "Gain: G = R2/R1 (when R3=R1 and R4=R2)".to_string(),
                "CMRR depends on resistor matching accuracy".to_string(),
                "Use 0.1% tolerance resistors for high CMRR".to_string(),
                "Temperature tracking critical for drift performance".to_string(),
            ],
        };
        
        self.patterns.push(pattern);
    }
    
    /// Add pull-up/pull-down pattern
    fn add_pullup_pulldown_pattern(&mut self) {
        let pattern = CircuitPattern {
            name: "pullup_pulldown".to_string(),
            description: "Pull-up or pull-down resistor for digital signals".to_string(),
            pattern_type: PatternType::DigitalInterface,
            required_components: vec![
                ComponentMatcher {
                    role: ComponentRole::PullResistor,
                    component_types: vec!["Res".to_string()],
                    min_count: 1,
                    max_count: 1,
                },
            ],
            optional_components: vec![],
            connectivity_rules: vec![
                ConnectivityRule {
                    description: "Resistor between signal and supply/ground".to_string(),
                    source_role: ComponentRole::PullResistor,
                    target_role: ComponentRole::DigitalIO,
                    connection_type: ConnectionType::Signal,
                },
            ],
            design_knowledge: vec![
                "Pull-up: R = (Vcc - Vol) / Iol".to_string(),
                "Pull-down: R = Voh / Ioh".to_string(),
                "Typical values: 4.7kΩ to 10kΩ for 3.3V logic".to_string(),
                "Lower resistance = faster switching, higher power".to_string(),
            ],
        };
        
        self.patterns.push(pattern);
    }
    
    /// Add crystal oscillator pattern
    fn add_crystal_oscillator_pattern(&mut self) {
        let pattern = CircuitPattern {
            name: "crystal_oscillator".to_string(),
            description: "Crystal oscillator with load capacitors".to_string(),
            pattern_type: PatternType::Clock,
            required_components: vec![
                ComponentMatcher {
                    role: ComponentRole::Crystal,
                    component_types: vec!["Crystal".to_string(), "XTAL".to_string()],
                    min_count: 1,
                    max_count: 1,
                },
                ComponentMatcher {
                    role: ComponentRole::LoadCapacitor,
                    component_types: vec!["Cap".to_string()],
                    min_count: 2,
                    max_count: 2,
                },
            ],
            optional_components: vec![
                ComponentMatcher {
                    role: ComponentRole::FeedbackResistor,
                    component_types: vec!["Res".to_string()],
                    min_count: 0,
                    max_count: 1,
                },
            ],
            connectivity_rules: vec![
                ConnectivityRule {
                    description: "Crystal between oscillator pins".to_string(),
                    source_role: ComponentRole::Crystal,
                    target_role: ComponentRole::Oscillator,
                    connection_type: ConnectionType::Clock,
                },
                ConnectivityRule {
                    description: "Load capacitors from each pin to ground".to_string(),
                    source_role: ComponentRole::LoadCapacitor,
                    target_role: ComponentRole::Crystal,
                    connection_type: ConnectionType::Signal,
                },
            ],
            design_knowledge: vec![
                "Load capacitance: CL = (C1 × C2)/(C1 + C2) + Cstray".to_string(),
                "Typical values: 12-22pF for most crystals".to_string(),
                "ESR affects startup reliability".to_string(),
                "Layout: Keep traces short, avoid switching signals".to_string(),
            ],
        };
        
        self.patterns.push(pattern);
    }
    
    /// Add reset circuit pattern
    fn add_reset_circuit_pattern(&mut self) {
        let pattern = CircuitPattern {
            name: "reset_circuit".to_string(),
            description: "Power-on reset circuit".to_string(),
            pattern_type: PatternType::DigitalInterface,
            required_components: vec![
                ComponentMatcher {
                    role: ComponentRole::ResetCapacitor,
                    component_types: vec!["Cap".to_string()],
                    min_count: 1,
                    max_count: 1,
                },
                ComponentMatcher {
                    role: ComponentRole::ResetResistor,
                    component_types: vec!["Res".to_string()],
                    min_count: 1,
                    max_count: 1,
                },
            ],
            optional_components: vec![
                ComponentMatcher {
                    role: ComponentRole::ResetButton,
                    component_types: vec!["Switch".to_string()],
                    min_count: 0,
                    max_count: 1,
                },
            ],
            connectivity_rules: vec![
                ConnectivityRule {
                    description: "RC network to reset pin".to_string(),
                    source_role: ComponentRole::ResetCapacitor,
                    target_role: ComponentRole::ResetInput,
                    connection_type: ConnectionType::Signal,
                },
            ],
            design_knowledge: vec![
                "Reset time: T = R × C × ln(Vdd/(Vdd - Vreset))".to_string(),
                "Typical values: 10kΩ and 100nF for ~1ms reset".to_string(),
                "Ensure reset time exceeds power supply rise time".to_string(),
                "Manual reset button pulls reset low".to_string(),
            ],
        };
        
        self.patterns.push(pattern);
    }
    
    /// Analyze circuit topology
    fn analyze_circuit_topology(&self, netlist: &Netlist) -> Result<CircuitTopology> {
        let mut topology = CircuitTopology {
            total_components: netlist.instances.len(),
            total_nets: netlist.nets.len(),
            power_domains: Vec::new(),
            signal_groups: Vec::new(),
            connectivity_matrix: HashMap::new(),
            component_clusters: Vec::new(),
        };
        
        // Analyze power domains
        topology.power_domains = self.identify_power_domains(netlist)?;
        
        // Group signals by function
        topology.signal_groups = self.group_signals_by_function(netlist)?;
        
        // Build connectivity matrix
        topology.connectivity_matrix = self.build_connectivity_matrix(netlist)?;
        
        // Identify component clusters
        topology.component_clusters = self.identify_component_clusters(netlist)?;
        
        info!("Topology analysis: {} components, {} nets, {} power domains", 
              topology.total_components, topology.total_nets, topology.power_domains.len());
        
        Ok(topology)
    }
    
    /// Identify power domains in the circuit
    fn identify_power_domains(&self, netlist: &Netlist) -> Result<Vec<PowerDomain>> {
        let mut domains = Vec::new();
        
        // Look for power nets (VCC, VDD, etc.)
        for (net_id, net) in &netlist.nets {
            if let Some(ref name) = net.name {
                if self.is_power_net(name) {
                    let domain = PowerDomain {
                        name: name.clone(),
                        net_id,
                        voltage_level: self.estimate_voltage_level(name),
                        connected_components: self.find_components_on_net(netlist, net_id),
                    };
                    domains.push(domain);
                }
            }
        }
        
        Ok(domains)
    }
    
    /// Check if a net name indicates a power net
    fn is_power_net(&self, name: &str) -> bool {
        let power_indicators = ["VCC", "VDD", "VIN", "VOUT", "V+", "V-", "+", "-", 
                               "GND", "VSS", "GROUND", "AGND", "DGND"];
        power_indicators.iter().any(|&indicator| 
            name.to_uppercase().contains(indicator))
    }
    
    /// Estimate voltage level from net name
    fn estimate_voltage_level(&self, name: &str) -> Option<f64> {
        let name_upper = name.to_uppercase();
        
        // Look for voltage indicators
        if name_upper.contains("3V3") || name_upper.contains("3.3V") {
            Some(3.3)
        } else if name_upper.contains("5V") {
            Some(5.0)
        } else if name_upper.contains("12V") {
            Some(12.0)
        } else if name_upper.contains("24V") {
            Some(24.0)
        } else if name_upper.contains("1V8") || name_upper.contains("1.8V") {
            Some(1.8)
        } else if name_upper.contains("GND") || name_upper.contains("VSS") {
            Some(0.0)
        } else {
            None
        }
    }
    
    /// Find all components connected to a net
    fn find_components_on_net(&self, netlist: &Netlist, net_id: NetId) -> Vec<InstanceId> {
        let mut components = Vec::new();
        
        if let Some(net) = netlist.nets.get(net_id) {
            for connection in &net.connections {
                if let Some(instance_id) = self.extract_instance_from_connection(connection) {
                    if !components.contains(&instance_id) {
                        components.push(instance_id);
                    }
                }
            }
        }
        
        components
    }
    
    /// Extract instance ID from connection point
    fn extract_instance_from_connection(&self, connection: &bhdl_netlist::ConnectionPoint) -> Option<InstanceId> {
        match connection {
            bhdl_netlist::ConnectionPoint::InstancePort(instance_id, _) => Some(*instance_id),
            bhdl_netlist::ConnectionPoint::InstancePin(instance_id, _) => Some(*instance_id),
            bhdl_netlist::ConnectionPoint::PinInstance(pin_instance_id) => {
                // Would need to look up the parent instance from pin instance
                // Simplified for now
                None
            },
            _ => None,
        }
    }
    
    /// Group signals by function (power, data, clock, etc.)
    fn group_signals_by_function(&self, netlist: &Netlist) -> Result<Vec<SignalGroup>> {
        let mut groups = Vec::new();
        
        // Power signals
        let power_nets: Vec<_> = netlist.nets.iter()
            .filter_map(|(id, net)| {
                if let Some(ref name) = net.name {
                    if self.is_power_net(name) {
                        Some((id, name.clone()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();
        
        if !power_nets.is_empty() {
            groups.push(SignalGroup {
                group_type: SignalType::Power,
                nets: power_nets,
                characteristics: "Power distribution and regulation".to_string(),
            });
        }
        
        // Clock signals  
        let clock_nets: Vec<_> = netlist.nets.iter()
            .filter_map(|(id, net)| {
                if let Some(ref name) = net.name {
                    if self.is_clock_net(name) {
                        Some((id, name.clone()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();
        
        if !clock_nets.is_empty() {
            groups.push(SignalGroup {
                group_type: SignalType::Clock,
                nets: clock_nets,
                characteristics: "Timing and synchronization".to_string(),
            });
        }
        
        // Reset signals
        let reset_nets: Vec<_> = netlist.nets.iter()
            .filter_map(|(id, net)| {
                if let Some(ref name) = net.name {
                    if self.is_reset_net(name) {
                        Some((id, name.clone()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();
        
        if !reset_nets.is_empty() {
            groups.push(SignalGroup {
                group_type: SignalType::Reset,
                nets: reset_nets,
                characteristics: "System initialization and control".to_string(),
            });
        }
        
        Ok(groups)
    }
    
    /// Check if net name indicates clock signal
    fn is_clock_net(&self, name: &str) -> bool {
        let clock_indicators = ["CLK", "CLOCK", "OSC", "XTAL"];
        clock_indicators.iter().any(|&indicator| 
            name.to_uppercase().contains(indicator))
    }
    
    /// Check if net name indicates reset signal  
    fn is_reset_net(&self, name: &str) -> bool {
        let reset_indicators = ["RST", "RESET", "nRST", "RESETn"];
        reset_indicators.iter().any(|&indicator| 
            name.to_uppercase().contains(indicator))
    }
    
    /// Build connectivity matrix between components
    fn build_connectivity_matrix(&self, netlist: &Netlist) -> Result<HashMap<(InstanceId, InstanceId), usize>> {
        let mut matrix = HashMap::new();
        
        // For each net, increment connection count between all component pairs
        for (_net_id, net) in &netlist.nets {
            let components = self.get_connected_instances(&net.connections);
            
            // Count connections between each pair of components
            for (i, &comp1) in components.iter().enumerate() {
                for &comp2 in components.iter().skip(i + 1) {
                    let key = if comp1 < comp2 { (comp1, comp2) } else { (comp2, comp1) };
                    *matrix.entry(key).or_insert(0) += 1;
                }
            }
        }
        
        Ok(matrix)
    }
    
    /// Get instance IDs from connection points
    fn get_connected_instances(&self, connections: &[bhdl_netlist::ConnectionPoint]) -> Vec<InstanceId> {
        connections.iter()
            .filter_map(|conn| self.extract_instance_from_connection(conn))
            .collect()
    }
    
    /// Identify clusters of highly connected components
    fn identify_component_clusters(&self, netlist: &Netlist) -> Result<Vec<ComponentCluster>> {
        let mut clusters = Vec::new();
        
        // Simple clustering: group components that share multiple nets
        let connectivity = self.build_connectivity_matrix(netlist)?;
        let mut processed = HashSet::new();
        
        for (instance_id, _instance) in &netlist.instances {
            if processed.contains(&instance_id) {
                continue;
            }
            
            let mut cluster_components = vec![instance_id];
            processed.insert(instance_id);
            
            // Find highly connected components (threshold: 2+ shared nets)
            for (&(comp1, comp2), &count) in &connectivity {
                if count >= 2 {
                    if comp1 == instance_id && !processed.contains(&comp2) {
                        cluster_components.push(comp2);
                        processed.insert(comp2);
                    } else if comp2 == instance_id && !processed.contains(&comp1) {
                        cluster_components.push(comp1);
                        processed.insert(comp1);
                    }
                }
            }
            
            if cluster_components.len() > 1 {
                clusters.push(ComponentCluster {
                    components: cluster_components.clone(),
                    cluster_type: self.infer_cluster_type(&cluster_components, netlist),
                    description: format!("Cluster of {} components", cluster_components.len()),
                });
            }
        }
        
        Ok(clusters)
    }
    
    /// Infer cluster type from component types
    fn infer_cluster_type(&self, components: &[InstanceId], netlist: &Netlist) -> ClusterType {
        let component_types: Vec<_> = components.iter()
            .filter_map(|&id| netlist.instances.get(id))
            .map(|instance| instance.name.as_str())
            .collect();
        
        // Simple heuristics for cluster classification
        if component_types.iter().any(|name| name.contains("7805") || name.contains("LM") || name.contains("TPS")) {
            ClusterType::PowerManagement
        } else if component_types.iter().any(|name| name.contains("OpAmp") || name.contains("LM358")) {
            ClusterType::Analog
        } else if component_types.iter().any(|name| name.contains("Crystal") || name.contains("XTAL")) {
            ClusterType::Clock
        } else if component_types.iter().any(|name| name.contains("Cap") && name.contains("Res")) {
            ClusterType::Filter
        } else {
            ClusterType::Unknown
        }
    }
    
    /// Infer component roles based on connectivity and type
    fn infer_component_roles(
        &mut self, 
        netlist: &Netlist, 
        topology: &CircuitTopology,
        analysis: &AnalysisResult,
    ) -> Result<()> {
        for (instance_id, instance) in &netlist.instances {
            let role = self.determine_component_role(instance_id, instance, netlist, topology, analysis)?;
            self.component_roles.insert(instance_id, role);
        }
        
        info!("Inferred roles for {} components", self.component_roles.len());
        Ok(())
    }
    
    /// Determine role of a specific component
    fn determine_component_role(
        &self,
        instance_id: InstanceId,
        instance: &bhdl_netlist::Instance,
        netlist: &Netlist,
        topology: &CircuitTopology,
        _analysis: &AnalysisResult,
    ) -> Result<ComponentRole> {
        // Role inference based on component type and connectivity
        let component_type = &instance.name;
        
        // Direct type mapping
        if component_type.contains("7805") || component_type.contains("1117") {
            return Ok(ComponentRole::VoltageRegulator);
        }
        
        if component_type.contains("TPS") || component_type.contains("LM259") {
            return Ok(ComponentRole::SwitchingController);
        }
        
        if component_type.contains("TVS") {
            return Ok(ComponentRole::ProtectionDiode);
        }
        
        if component_type.contains("OpAmp") {
            return Ok(ComponentRole::OpAmp);
        }
        
        if component_type.contains("Crystal") || component_type.contains("XTAL") {
            return Ok(ComponentRole::Crystal);
        }
        
        // Context-based inference for generic components
        if component_type.contains("Res") {
            return Ok(self.infer_resistor_role(instance_id, netlist, topology));
        }
        
        if component_type.contains("Cap") {
            return Ok(self.infer_capacitor_role(instance_id, netlist, topology));
        }
        
        if component_type.contains("Ind") {
            return Ok(ComponentRole::Inductor);
        }
        
        // Default role
        Ok(ComponentRole::Unknown)
    }
    
    /// Infer resistor role based on connectivity
    fn infer_resistor_role(&self, instance_id: InstanceId, netlist: &Netlist, topology: &CircuitTopology) -> ComponentRole {
        // Check if resistor is in a power domain (likely current limiting)
        for domain in &topology.power_domains {
            if domain.connected_components.contains(&instance_id) {
                return ComponentRole::SeriesResistor;
            }
        }
        
        // Check if part of feedback network (connected to amplifier)
        let connected_components = self.find_connected_components(instance_id, netlist);
        for comp_id in connected_components {
            if let Some(instance) = netlist.instances.get(comp_id) {
                if instance.name.contains("OpAmp") || instance.name.contains("TPS") {
                    return ComponentRole::FeedbackResistor;
                }
            }
        }
        
        // Check if pull-up/pull-down (connected to power rail and I/O)
        // Simplified check - would need more sophisticated analysis
        
        // Default to generic resistor
        ComponentRole::DividerResistor
    }
    
    /// Infer capacitor role based on connectivity
    fn infer_capacitor_role(&self, instance_id: InstanceId, netlist: &Netlist, topology: &CircuitTopology) -> ComponentRole {
        // Check if connected to power domain (likely decoupling)
        for domain in &topology.power_domains {
            if domain.connected_components.contains(&instance_id) {
                return ComponentRole::DecouplingCapacitor;
            }
        }
        
        // Check if part of filter network
        let connected_components = self.find_connected_components(instance_id, netlist);
        let has_resistor = connected_components.iter()
            .any(|&comp_id| {
                netlist.instances.get(comp_id)
                    .map(|inst| inst.name.contains("Res"))
                    .unwrap_or(false)
            });
        
        if has_resistor {
            return ComponentRole::FilterCapacitor;
        }
        
        // Check if connected to crystal (load capacitor)
        let has_crystal = connected_components.iter()
            .any(|&comp_id| {
                netlist.instances.get(comp_id)
                    .map(|inst| inst.name.contains("Crystal") || inst.name.contains("XTAL"))
                    .unwrap_or(false)
            });
        
        if has_crystal {
            return ComponentRole::LoadCapacitor;
        }
        
        // Default to generic capacitor
        ComponentRole::FilterCapacitor
    }
    
    /// Find components directly connected to the given component
    fn find_connected_components(&self, instance_id: InstanceId, netlist: &Netlist) -> Vec<InstanceId> {
        let mut connected = Vec::new();
        
        // Find all nets this component is connected to
        for (_net_id, net) in &netlist.nets {
            let mut this_component_connected = false;
            let mut other_components = Vec::new();
            
            for connection in &net.connections {
                if let Some(comp_id) = self.extract_instance_from_connection(connection) {
                    if comp_id == instance_id {
                        this_component_connected = true;
                    } else {
                        other_components.push(comp_id);
                    }
                }
            }
            
            if this_component_connected {
                connected.extend(other_components);
            }
        }
        
        connected.sort();
        connected.dedup();
        connected
    }
    
    /// Match a circuit pattern against the netlist
    fn match_pattern(
        &self,
        pattern: &CircuitPattern,
        netlist: &Netlist,
        topology: &CircuitTopology,
    ) -> Result<Option<RecognizedPattern>> {
        // Check if required components are present with correct roles
        let mut component_matches = HashMap::new();
        
        for required_matcher in &pattern.required_components {
            let matching_components = self.find_components_with_role(&required_matcher.role);
            
            if matching_components.len() < required_matcher.min_count {
                return Ok(None); // Pattern doesn't match
            }
            
            if matching_components.len() > required_matcher.max_count {
                // Too many components - could still match with subset
            }
            
            component_matches.insert(required_matcher.role.clone(), matching_components);
        }
        
        // Check connectivity rules
        for rule in &pattern.connectivity_rules {
            if !self.verify_connectivity_rule(rule, &component_matches, netlist)? {
                return Ok(None); // Connectivity doesn't match
            }
        }
        
        // Calculate confidence score
        let confidence = self.calculate_pattern_confidence(pattern, &component_matches, topology);
        
        let recognized = RecognizedPattern {
            pattern_name: pattern.name.clone(),
            pattern_type: pattern.pattern_type.clone(),
            confidence_score: confidence,
            matched_components: component_matches.into_iter()
                .flat_map(|(_, comps)| comps)
                .collect(),
            design_insights: pattern.design_knowledge.clone(),
            applicable_rules: self.pattern_rules.get(&pattern.name).cloned().unwrap_or_default(),
        };
        
        Ok(Some(recognized))
    }
    
    /// Find components with specific role
    fn find_components_with_role(&self, role: &ComponentRole) -> Vec<InstanceId> {
        self.component_roles.iter()
            .filter(|(_, comp_role)| *comp_role == role)
            .map(|(id, _)| *id)
            .collect()
    }
    
    /// Verify connectivity rule between components
    fn verify_connectivity_rule(
        &self,
        rule: &ConnectivityRule,
        component_matches: &HashMap<ComponentRole, Vec<InstanceId>>,
        netlist: &Netlist,
    ) -> Result<bool> {
        let source_components = component_matches.get(&rule.source_role);
        let target_components = component_matches.get(&rule.target_role);
        
        if let (Some(sources), Some(targets)) = (source_components, target_components) {
            // Check if any source is connected to any target
            for &source_id in sources {
                for &target_id in targets {
                    if self.are_components_connected(source_id, target_id, netlist) {
                        return Ok(true);
                    }
                }
            }
        }
        
        Ok(false)
    }
    
    /// Check if two components are connected
    fn are_components_connected(&self, comp1: InstanceId, comp2: InstanceId, netlist: &Netlist) -> bool {
        // Check if components share any nets
        for (_net_id, net) in &netlist.nets {
            let mut comp1_connected = false;
            let mut comp2_connected = false;
            
            for connection in &net.connections {
                if let Some(comp_id) = self.extract_instance_from_connection(connection) {
                    if comp_id == comp1 {
                        comp1_connected = true;
                    }
                    if comp_id == comp2 {
                        comp2_connected = true;
                    }
                }
            }
            
            if comp1_connected && comp2_connected {
                return true;
            }
        }
        
        false
    }
    
    /// Calculate pattern confidence score
    fn calculate_pattern_confidence(
        &self,
        pattern: &CircuitPattern,
        component_matches: &HashMap<ComponentRole, Vec<InstanceId>>,
        _topology: &CircuitTopology,
    ) -> f64 {
        let mut score: f64 = 1.0;
        
        // Reduce score for missing optional components
        for optional_matcher in &pattern.optional_components {
            if let Some(matches) = component_matches.get(&optional_matcher.role) {
                if matches.is_empty() {
                    score *= 0.9; // Slight penalty for missing optional components
                }
            } else {
                score *= 0.9;
            }
        }
        
        // Boost score for exact component counts
        for required_matcher in &pattern.required_components {
            if let Some(matches) = component_matches.get(&required_matcher.role) {
                let count = matches.len();
                if count == required_matcher.min_count {
                    score *= 1.1; // Boost for exact match
                } else if count > required_matcher.max_count {
                    score *= 0.8; // Penalty for too many components
                }
            }
        }
        
        score.min(1.0)
    }
    
    /// Generate design recommendations based on recognized patterns
    fn generate_design_recommendations(&self, netlist: &Netlist) -> Result<Vec<DesignRecommendation>> {
        let mut recommendations = Vec::new();
        
        for recognized in &self.recognized_patterns {
            // Apply pattern-specific rules
            for rule in &recognized.applicable_rules {
                let recommendation = self.evaluate_design_rule(rule, &recognized.matched_components, netlist)?;
                if let Some(rec) = recommendation {
                    recommendations.push(rec);
                }
            }
            
            // Generate pattern-specific recommendations
            recommendations.extend(self.generate_pattern_recommendations(recognized, netlist)?);
        }
        
        Ok(recommendations)
    }
    
    /// Evaluate a specific design rule
    fn evaluate_design_rule(
        &self,
        rule: &DesignRule,
        components: &[InstanceId],
        netlist: &Netlist,
    ) -> Result<Option<DesignRecommendation>> {
        // Simplified rule evaluation - would need expression parser for full implementation
        let should_apply = self.evaluate_rule_condition(&rule.condition, components, netlist)?;
        
        if should_apply {
            return Ok(Some(DesignRecommendation {
                category: match rule.rule_type {
                    RuleType::Error => RecommendationCategory::Error,
                    RuleType::Warning => RecommendationCategory::Warning,
                    RuleType::Info => RecommendationCategory::Info,
                    RuleType::Recommendation => RecommendationCategory::Suggestion,
                },
                title: rule.name.clone(),
                description: rule.description.clone(),
                recommendation: rule.recommendation.clone(),
                affected_components: components.to_vec(),
                priority: match rule.rule_type {
                    RuleType::Error => 1,
                    RuleType::Warning => 2,
                    RuleType::Recommendation => 3,
                    RuleType::Info => 4,
                },
            }));
        }
        
        Ok(None)
    }
    
    /// Evaluate rule condition (simplified)
    fn evaluate_rule_condition(
        &self,
        condition: &str,
        _components: &[InstanceId],
        _netlist: &Netlist,
    ) -> Result<bool> {
        // Simplified condition evaluation
        // In a full implementation, this would parse and evaluate expressions
        
        if condition.contains("power_dissipation > 1.0") {
            // Would calculate actual power dissipation
            return Ok(false); // Placeholder
        }
        
        if condition.contains("input_capacitor < 10e-6") {
            // Would check actual capacitor values
            return Ok(true); // Placeholder
        }
        
        Ok(false)
    }
    
    /// Generate pattern-specific recommendations
    fn generate_pattern_recommendations(
        &self,
        pattern: &RecognizedPattern,
        _netlist: &Netlist,
    ) -> Result<Vec<DesignRecommendation>> {
        let mut recommendations = Vec::new();
        
        match pattern.pattern_name.as_str() {
            "linear_regulator" => {
                recommendations.push(DesignRecommendation {
                    category: RecommendationCategory::Suggestion,
                    title: "Linear Regulator Optimization".to_string(),
                    description: "Consider efficiency and thermal management".to_string(),
                    recommendation: "For high current loads, consider switching regulator for better efficiency".to_string(),
                    affected_components: pattern.matched_components.clone(),
                    priority: 3,
                });
            },
            "switching_regulator" => {
                recommendations.push(DesignRecommendation {
                    category: RecommendationCategory::Info,
                    title: "Switching Regulator Layout".to_string(),
                    description: "Layout considerations for switching regulators".to_string(),
                    recommendation: "Keep switching node traces short, use solid ground plane, place input/output caps close".to_string(),
                    affected_components: pattern.matched_components.clone(),
                    priority: 2,
                });
            },
            "crystal_oscillator" => {
                recommendations.push(DesignRecommendation {
                    category: RecommendationCategory::Warning,
                    title: "Crystal Oscillator Layout".to_string(),
                    description: "Crystal layout affects startup reliability".to_string(),
                    recommendation: "Keep crystal traces short, add guard ring, avoid switching signals nearby".to_string(),
                    affected_components: pattern.matched_components.clone(),
                    priority: 2,
                });
            },
            _ => {
                // Generic recommendations based on pattern type
                match pattern.pattern_type {
                    PatternType::PowerSupply => {
                        recommendations.push(DesignRecommendation {
                            category: RecommendationCategory::Info,
                            title: "Power Supply Design".to_string(),
                            description: "General power supply considerations".to_string(),
                            recommendation: "Ensure adequate filtering and protection".to_string(),
                            affected_components: pattern.matched_components.clone(),
                            priority: 3,
                        });
                    },
                    _ => {}
                }
            }
        }
        
        Ok(recommendations)
    }
    
    /// Calculate what percentage of the circuit is covered by recognized patterns
    fn calculate_pattern_coverage(&self, netlist: &Netlist) -> f64 {
        let total_components = netlist.instances.len();
        if total_components == 0 {
            return 0.0;
        }
        
        let covered_components: HashSet<_> = self.recognized_patterns.iter()
            .flat_map(|pattern| &pattern.matched_components)
            .collect();
        
        covered_components.len() as f64 / total_components as f64
    }
}

// Data structures for design pattern recognition

#[derive(Debug, Clone)]
pub struct CircuitPattern {
    pub name: String,
    pub description: String,
    pub pattern_type: PatternType,
    pub required_components: Vec<ComponentMatcher>,
    pub optional_components: Vec<ComponentMatcher>,
    pub connectivity_rules: Vec<ConnectivityRule>,
    pub design_knowledge: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PatternType {
    PowerSupply,
    Filter,
    Amplifier,
    DigitalInterface,
    Protection,
    Clock,
    SignalConditioning,
    PowerIntegrity,
}

#[derive(Debug, Clone)]
pub struct ComponentMatcher {
    pub role: ComponentRole,
    pub component_types: Vec<String>,
    pub min_count: usize,
    pub max_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ComponentRole {
    // Power management
    VoltageRegulator,
    SwitchingController,
    CurrentLimiter,
    PowerSource,
    Load,
    
    // Passive components
    InputFilter,
    OutputFilter,
    DecouplingCapacitor,
    FilterCapacitor,
    FilterResistor,
    LoadCapacitor,
    Inductor,
    
    // Resistor roles
    DividerResistor,
    FeedbackResistor,
    InputResistor,
    PullResistor,
    MatchedResistor,
    SeriesResistor,
    
    // Protection
    ProtectionDiode,
    ESDProtection,
    ThermalSwitch,
    
    // Amplifiers
    OpAmp,
    
    // Digital
    DigitalIO,
    Crystal,
    Oscillator,
    ResetInput,
    ResetCapacitor,
    ResetResistor,
    ResetButton,
    
    // Sensors
    TemperatureSensor,
    
    // EMC
    EMCInductor,
    EMCCapacitor,
    
    // Interfaces
    IOInterface,
    
    // Heat management
    HeatSource,
    
    // Network analysis
    FeedbackNetwork,
    CompensationNetwork,
    
    Unknown,
}

#[derive(Debug, Clone)]
pub struct ConnectivityRule {
    pub description: String,
    pub source_role: ComponentRole,
    pub target_role: ComponentRole,
    pub connection_type: ConnectionType,
}

#[derive(Debug, Clone)]
pub enum ConnectionType {
    Power,
    Signal,
    Feedback,
    Protection,
    Clock,
    Sensing,
}

#[derive(Debug, Clone)]
pub struct DesignRule {
    pub name: String,
    pub description: String,
    pub rule_type: RuleType,
    pub condition: String,
    pub recommendation: String,
}

#[derive(Debug, Clone)]
pub enum RuleType {
    Error,
    Warning,
    Info,
    Recommendation,
}

#[derive(Debug, Clone)]
pub struct RecognizedPattern {
    pub pattern_name: String,
    pub pattern_type: PatternType,
    pub confidence_score: f64,
    pub matched_components: Vec<InstanceId>,
    pub design_insights: Vec<String>,
    pub applicable_rules: Vec<DesignRule>,
}

#[derive(Debug, Clone)]
pub struct CircuitTopology {
    pub total_components: usize,
    pub total_nets: usize,
    pub power_domains: Vec<PowerDomain>,
    pub signal_groups: Vec<SignalGroup>,
    pub connectivity_matrix: HashMap<(InstanceId, InstanceId), usize>,
    pub component_clusters: Vec<ComponentCluster>,
}

#[derive(Debug, Clone)]
pub struct PowerDomain {
    pub name: String,
    pub net_id: NetId,
    pub voltage_level: Option<f64>,
    pub connected_components: Vec<InstanceId>,
}

#[derive(Debug, Clone)]
pub struct SignalGroup {
    pub group_type: SignalType,
    pub nets: Vec<(NetId, String)>,
    pub characteristics: String,
}

#[derive(Debug, Clone)]
pub enum SignalType {
    Power,
    Clock,
    Reset,
    Data,
    Control,
    Analog,
}

#[derive(Debug, Clone)]
pub struct ComponentCluster {
    pub components: Vec<InstanceId>,
    pub cluster_type: ClusterType,
    pub description: String,
}

#[derive(Debug, Clone)]
pub enum ClusterType {
    PowerManagement,
    Analog,
    Digital,
    Filter,
    Clock,
    Protection,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct DesignRecommendation {
    pub category: RecommendationCategory,
    pub title: String,
    pub description: String,
    pub recommendation: String,
    pub affected_components: Vec<InstanceId>,
    pub priority: u8, // 1 = highest priority
}

#[derive(Debug, Clone)]
pub enum RecommendationCategory {
    Error,
    Warning,
    Info,
    Suggestion,
}

#[derive(Debug, Clone)]
pub struct PatternRecognitionReport {
    pub topology_analysis: CircuitTopology,
    pub component_roles: HashMap<InstanceId, ComponentRole>,
    pub recognized_patterns: Vec<RecognizedPattern>,
    pub design_recommendations: Vec<DesignRecommendation>,
    pub pattern_coverage: f64,
}