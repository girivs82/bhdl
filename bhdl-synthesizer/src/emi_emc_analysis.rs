// EMI/EMC (Electromagnetic Interference/Electromagnetic Compatibility) Analysis
// Analyzes circuits for electromagnetic interference and compatibility issues

use bhdl_netlist::{Netlist, InstanceId, Instance, NetId, Net};
use bhdl_analyzer::AnalysisResult;
use std::collections::{HashMap, HashSet};
use serde::{Serialize, Deserialize};
use anyhow::{Result, Context};
use log::{info, warn, debug, error};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EMIEMCAnalyzer {
    frequency_bands: Vec<FrequencyBand>,
    emission_standards: HashMap<EmissionStandard, EmissionLimits>,
    immunity_standards: HashMap<ImmunityStandard, ImmunityRequirements>,
    component_characteristics: HashMap<String, ComponentEMIProfile>,
    board_characteristics: BoardEMIProfile,
    analysis_config: EMIEMCConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrequencyBand {
    name: String,
    start_frequency: f64, // Hz
    end_frequency: f64,   // Hz
    primary_concerns: Vec<EMIConcern>,
    typical_sources: Vec<String>,
    mitigation_techniques: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EMIConcern {
    ConductedEmissions,
    RadiatedEmissions,
    ConductedSusceptibility,
    RadiatedSusceptibility,
    ElectrostaticDischarge,
    PowerLineDisturbances,
    MagneticFields,
    ElectricFields,
    Harmonics,
    Flicker,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EmissionStandard {
    CISPR22,    // Information Technology Equipment
    CISPR25,    // Automotive
    CISPR32,    // Multimedia Equipment
    FCC15,      // US Federal Communications Commission
    ETSI301489, // European Telecommunications Standards Institute
    IEC61000,   // General EMC Standard
    MILSTD461,  // Military Standard
    DO160,      // Aviation
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmissionLimits {
    conducted_limits: HashMap<String, f64>, // Frequency (Hz) -> Limit (dBμV)
    radiated_limits: HashMap<String, f64>,  // Frequency (Hz) -> Limit (dBμV/m)
    harmonics_limits: Option<HarmonicsLimits>,
    flicker_limits: Option<FlickerLimits>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarmonicsLimits {
    fundamental_frequency: f64,
    max_harmonic_order: u32,
    limits_by_order: HashMap<u32, f64>, // Harmonic order -> Limit (% of fundamental)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlickerLimits {
    short_term_severity: f64, // Pst
    long_term_severity: f64,  // Plt
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ImmunityStandard {
    IEC61000_4_2, // Electrostatic Discharge
    IEC61000_4_3, // Radiated RF Electromagnetic Field
    IEC61000_4_4, // Electrical Fast Transient/Burst
    IEC61000_4_5, // Surge Immunity
    IEC61000_4_6, // Conducted RF
    IEC61000_4_8, // Power Frequency Magnetic Field
    IEC61000_4_11, // Voltage Dips and Interruptions
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImmunityRequirements {
    test_levels: HashMap<String, f64>, // Frequency (Hz) -> Test Level
    performance_criteria: Vec<PerformanceCriteria>,
    protection_requirements: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PerformanceCriteria {
    CriterionA, // Normal performance within limits
    CriterionB, // Temporary loss of function, self-recoverable
    CriterionC, // Temporary loss of function, manual recovery required
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentEMIProfile {
    component_type: String,
    emission_characteristics: EmissionProfile,
    susceptibility_characteristics: SusceptibilityProfile,
    shielding_effectiveness: Option<f64>, // dB
    bandwidth: Option<f64>,               // Hz
    slew_rate: Option<f64>,              // V/s
    switching_frequency: Option<f64>,     // Hz
    current_consumption: Option<f64>,     // A
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmissionProfile {
    peak_emissions: HashMap<String, f64>, // Frequency (Hz) -> Emission Level (dBμV)
    broadband_noise_floor: f64,         // dBμV
    harmonic_content: Option<f64>,      // % of fundamental
    rise_time: Option<f64>,             // ns
    edge_rate: Option<f64>,             // V/ns
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SusceptibilityProfile {
    susceptible_frequencies: HashMap<String, f64>, // Frequency (Hz) -> Threshold (dBμV)
    immunity_level: f64,                         // dBμV/m
    bandwidth_sensitivity: Option<f64>,          // Hz
    input_impedance: Option<f64>,               // Ω
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardEMIProfile {
    board_size: (f64, f64),           // Length x Width (mm)
    layer_count: u32,
    ground_plane_coverage: f64,       // Percentage
    power_plane_coverage: f64,        // Percentage
    trace_impedance: f64,             // Ω
    via_count: u32,
    clock_frequencies: Vec<f64>,      // Hz
    power_consumption: f64,           // W
    enclosure_shielding: Option<f64>, // dB
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EMIEMCConfig {
    pub target_standards: Vec<EmissionStandard>,
    pub immunity_standards: Vec<ImmunityStandard>,
    pub frequency_range: (f64, f64), // Start, End Hz
    pub analysis_resolution: f64,     // Hz
    pub enable_prediction: bool,
    pub enable_mitigation_suggestions: bool,
    pub include_crosstalk_analysis: bool,
    pub include_power_integrity: bool,
    pub safety_margin: f64, // dB
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EMIEMCAnalysisResult {
    pub emission_compliance: EmissionComplianceResult,
    pub immunity_assessment: ImmunityAssessmentResult,
    pub interference_analysis: InterferenceAnalysisResult,
    pub mitigation_recommendations: Vec<MitigationRecommendation>,
    pub compliance_summary: ComplianceSummary,
    pub analysis_summary: AnalysisSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmissionComplianceResult {
    pub standards_tested: Vec<EmissionStandard>,
    pub conducted_emissions: ComplianceStatus,
    pub radiated_emissions: ComplianceStatus,
    pub harmonic_emissions: ComplianceStatus,
    pub emission_hotspots: Vec<EmissionHotspot>,
    pub margin_analysis: Vec<MarginAnalysis>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceStatus {
    pub overall_status: ComplianceLevel,
    pub frequency_violations: Vec<FrequencyViolation>,
    pub worst_case_margin: f64, // dB
    pub compliance_percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComplianceLevel {
    Pass,
    PassWithMargin(f64), // dB margin
    Marginal(f64),       // dB over limit
    Fail(f64),           // dB over limit
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrequencyViolation {
    pub frequency: f64,      // Hz
    pub predicted_level: f64, // dBμV or dBμV/m
    pub limit: f64,          // dBμV or dBμV/m
    pub margin: f64,         // dB (negative = violation)
    pub source_components: Vec<InstanceId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmissionHotspot {
    pub component_id: InstanceId,
    pub component_name: String,
    pub emission_frequency: f64, // Hz
    pub emission_level: f64,     // dBμV or dBμV/m
    pub contribution_percentage: f64,
    pub hotspot_type: HotspotType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HotspotType {
    DigitalSwitching,
    ClockSignal,
    PowerSwitching,
    HighSpeedData,
    Oscillator,
    PowerSupplyNoise,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarginAnalysis {
    pub frequency: f64,
    pub margin: f64,           // dB
    pub margin_type: MarginType,
    pub confidence: f64,       // 0.0 to 1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarginType {
    Conducted,
    Radiated,
    Harmonic,
    Immunity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImmunityAssessmentResult {
    pub standards_assessed: Vec<ImmunityStandard>,
    pub susceptibility_analysis: SusceptibilityAnalysis,
    pub vulnerable_components: Vec<VulnerableComponent>,
    pub immunity_gaps: Vec<ImmunityGap>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SusceptibilityAnalysis {
    pub overall_immunity_level: f64, // dBμV/m
    pub frequency_response: HashMap<String, f64>, // Frequency -> Susceptibility
    pub critical_frequencies: Vec<f64>,
    pub protection_effectiveness: f64, // Percentage
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnerableComponent {
    pub component_id: InstanceId,
    pub component_name: String,
    pub vulnerability_type: VulnerabilityType,
    pub susceptible_frequency: f64, // Hz
    pub immunity_threshold: f64,    // dBμV/m
    pub risk_level: RiskLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VulnerabilityType {
    AnalogCircuits,
    HighGainAmplifiers,
    PrecisionReferences,
    ClockGenerators,
    DataConverters,
    CommunicationInterfaces,
    PowerManagement,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImmunityGap {
    pub frequency_range: (f64, f64), // Hz
    pub required_immunity: f64,      // dBμV/m
    pub predicted_immunity: f64,     // dBμV/m
    pub gap_magnitude: f64,          // dB
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterferenceAnalysisResult {
    pub internal_interference: Vec<InterferenceSource>,
    pub crosstalk_analysis: CrosstalkAnalysis,
    pub power_integrity_issues: Vec<PowerIntegrityIssue>,
    pub signal_integrity_concerns: Vec<SignalIntegrityConcern>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterferenceSource {
    pub source_component: InstanceId,
    pub victim_component: InstanceId,
    pub interference_frequency: f64, // Hz
    pub interference_level: f64,     // dBμV
    pub coupling_mechanism: CouplingMechanism,
    pub impact_severity: ImpactSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CouplingMechanism {
    ConductiveCoupling,
    CapacitiveCoupling,
    InductiveCoupling,
    RadiativeCoupling,
    CommonImpedance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImpactSeverity {
    Negligible,
    Minor,
    Moderate,
    Severe,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrosstalkAnalysis {
    pub near_end_crosstalk: Vec<CrosstalkPair>,
    pub far_end_crosstalk: Vec<CrosstalkPair>,
    pub worst_case_crosstalk: f64, // dB
    pub acceptable_crosstalk_threshold: f64, // dB
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrosstalkPair {
    pub aggressor_net: NetId,
    pub victim_net: NetId,
    pub crosstalk_level: f64, // dB
    pub frequency: f64,       // Hz
    pub coupling_length: f64, // mm
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerIntegrityIssue {
    pub power_net: NetId,
    pub issue_type: PowerIntegrityIssueType,
    pub frequency: f64,     // Hz
    pub impedance: f64,     // Ω
    pub voltage_ripple: f64, // V
    pub affected_components: Vec<InstanceId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PowerIntegrityIssueType {
    PowerSupplyNoise,
    GroundBounce,
    VoltageDropout,
    ImpedanceResonance,
    DecouplingInadequate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalIntegrityConcern {
    pub signal_net: NetId,
    pub concern_type: SignalIntegrityType,
    pub frequency: f64,       // Hz
    pub signal_degradation: f64, // dB
    pub timing_impact: Option<f64>, // ps
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignalIntegrityType {
    Overshoot,
    Undershoot,
    Ringing,
    Jitter,
    Reflection,
    AttenuationExcessive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MitigationRecommendation {
    pub recommendation_id: String,
    pub priority: MitigationPriority,
    pub recommendation_type: MitigationType,
    pub description: String,
    pub implementation_cost: CostEstimate,
    pub effectiveness: f64, // 0.0 to 1.0
    pub affected_components: Vec<InstanceId>,
    pub implementation_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MitigationPriority {
    Critical,  // Must fix for compliance
    High,      // Should fix for robust design
    Medium,    // Recommended for best practice
    Low,       // Optional improvement
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MitigationType {
    // Circuit Design Changes
    FilteringEnhancement,
    ShieldingImprovement,
    GroundingOptimization,
    DecouplingCapacitors,
    
    // Layout Changes
    ComponentPlacement,
    TraceRouting,
    LayerStackup,
    ViaPlacement,
    
    // Component Selection
    LowerEmissionComponents,
    HigherImmunityComponents,
    FilterComponents,
    ShieldingComponents,
    
    // System Level
    EnclosureShielding,
    CableShielding,
    FerriteCores,
    IsolationBarriers,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CostEstimate {
    Free,           // No additional cost
    Low,            // < $10
    Medium,         // $10-$100
    High,           // $100-$1000
    VeryHigh,       // > $1000
    DesignChange,   // Requires design revision
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceSummary {
    pub overall_compliance: ComplianceLevel,
    pub standards_passed: Vec<EmissionStandard>,
    pub standards_failed: Vec<EmissionStandard>,
    pub critical_issues: u32,
    pub high_priority_issues: u32,
    pub total_issues: u32,
    pub estimated_fix_cost: CostEstimate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisSummary {
    pub components_analyzed: u32,
    pub nets_analyzed: u32,
    pub frequencies_analyzed: u32,
    pub analysis_time_seconds: f64,
    pub prediction_confidence: f64, // 0.0 to 1.0
    pub recommendations_generated: u32,
}

impl EMIEMCAnalyzer {
    pub fn new() -> Self {
        Self {
            frequency_bands: Self::default_frequency_bands(),
            emission_standards: Self::default_emission_standards(),
            immunity_standards: Self::default_immunity_standards(),
            component_characteristics: HashMap::new(),
            board_characteristics: BoardEMIProfile::default(),
            analysis_config: EMIEMCConfig::default(),
        }
    }

    pub fn with_config(config: EMIEMCConfig) -> Self {
        let mut analyzer = Self::new();
        analyzer.analysis_config = config;
        analyzer
    }

    fn default_frequency_bands() -> Vec<FrequencyBand> {
        vec![
            FrequencyBand {
                name: "LF - Low Frequency".to_string(),
                start_frequency: 9_000.0,      // 9 kHz
                end_frequency: 150_000.0,      // 150 kHz
                primary_concerns: vec![EMIConcern::ConductedEmissions, EMIConcern::Harmonics],
                typical_sources: vec!["Power supplies".to_string(), "Motor drives".to_string()],
                mitigation_techniques: vec!["Line filters".to_string(), "Common mode chokes".to_string()],
            },
            FrequencyBand {
                name: "MF - Medium Frequency".to_string(),
                start_frequency: 150_000.0,    // 150 kHz
                end_frequency: 30_000_000.0,   // 30 MHz
                primary_concerns: vec![EMIConcern::ConductedEmissions, EMIConcern::RadiatedEmissions],
                typical_sources: vec!["Digital switching".to_string(), "Clock signals".to_string()],
                mitigation_techniques: vec!["Ferrite beads".to_string(), "Ground planes".to_string()],
            },
            FrequencyBand {
                name: "HF - High Frequency".to_string(),
                start_frequency: 30_000_000.0,  // 30 MHz
                end_frequency: 1_000_000_000.0, // 1 GHz
                primary_concerns: vec![EMIConcern::RadiatedEmissions, EMIConcern::RadiatedSusceptibility],
                typical_sources: vec!["High-speed digital".to_string(), "Oscillators".to_string()],
                mitigation_techniques: vec!["Shielding".to_string(), "Careful routing".to_string()],
            },
            FrequencyBand {
                name: "VHF - Very High Frequency".to_string(),
                start_frequency: 1_000_000_000.0,  // 1 GHz
                end_frequency: 18_000_000_000.0,   // 18 GHz
                primary_concerns: vec![EMIConcern::RadiatedEmissions, EMIConcern::RadiatedSusceptibility],
                typical_sources: vec!["RF circuits".to_string(), "High-speed serdes".to_string()],
                mitigation_techniques: vec!["Advanced shielding".to_string(), "Impedance control".to_string()],
            },
        ]
    }

    fn default_emission_standards() -> HashMap<EmissionStandard, EmissionLimits> {
        let mut standards = HashMap::new();
        
        // CISPR 22 Class B (Residential use)
        let mut cispr22_conducted = HashMap::new();
        cispr22_conducted.insert("150000".to_string(), 66.0);    // 150 kHz - 66 dBμV
        cispr22_conducted.insert("500000".to_string(), 56.0);    // 500 kHz - 56 dBμV
        cispr22_conducted.insert("30000000".to_string(), 56.0); // 30 MHz - 56 dBμV
        
        let mut cispr22_radiated = HashMap::new();
        cispr22_radiated.insert("30000000".to_string(), 40.0);  // 30 MHz - 40 dBμV/m
        cispr22_radiated.insert("230000000".to_string(), 40.0); // 230 MHz - 40 dBμV/m
        cispr22_radiated.insert("1000000000".to_string(), 47.0); // 1 GHz - 47 dBμV/m
        
        standards.insert(EmissionStandard::CISPR22, EmissionLimits {
            conducted_limits: cispr22_conducted,
            radiated_limits: cispr22_radiated,
            harmonics_limits: Some(HarmonicsLimits {
                fundamental_frequency: 50.0, // 50 Hz
                max_harmonic_order: 40,
                limits_by_order: (2..=40).map(|n| (n, 3.0 / n as f64)).collect(),
            }),
            flicker_limits: Some(FlickerLimits {
                short_term_severity: 1.0,
                long_term_severity: 0.65,
            }),
        });
        
        // FCC Part 15 Class B
        let mut fcc15_conducted = HashMap::new();
        fcc15_conducted.insert("150000".to_string(), 66.0);    // 150 kHz - 66 dBμV
        fcc15_conducted.insert("500000".to_string(), 56.0);    // 500 kHz - 56 dBμV
        fcc15_conducted.insert("30000000".to_string(), 56.0); // 30 MHz - 56 dBμV
        
        let mut fcc15_radiated = HashMap::new();
        fcc15_radiated.insert("30000000".to_string(), 40.0);   // 30 MHz - 40 dBμV/m
        fcc15_radiated.insert("88000000".to_string(), 40.0);   // 88 MHz - 40 dBμV/m
        fcc15_radiated.insert("216000000".to_string(), 40.0);  // 216 MHz - 40 dBμV/m
        fcc15_radiated.insert("960000000".to_string(), 40.0);  // 960 MHz - 40 dBμV/m
        fcc15_radiated.insert("1000000000".to_string(), 47.0); // 1 GHz - 47 dBμV/m
        
        standards.insert(EmissionStandard::FCC15, EmissionLimits {
            conducted_limits: fcc15_conducted,
            radiated_limits: fcc15_radiated,
            harmonics_limits: None,
            flicker_limits: None,
        });
        
        standards
    }

    fn default_immunity_standards() -> HashMap<ImmunityStandard, ImmunityRequirements> {
        let mut standards = HashMap::new();
        
        // IEC 61000-4-3 Radiated RF Immunity
        let mut rf_immunity = HashMap::new();
        rf_immunity.insert("80000000".to_string(), 3.0);   // 80 MHz - 3 V/m
        rf_immunity.insert("1000000000".to_string(), 3.0); // 1 GHz - 3 V/m
        rf_immunity.insert("2700000000".to_string(), 1.0); // 2.7 GHz - 1 V/m
        
        standards.insert(ImmunityStandard::IEC61000_4_3, ImmunityRequirements {
            test_levels: rf_immunity,
            performance_criteria: vec![PerformanceCriteria::CriterionA],
            protection_requirements: vec![
                "Equipment shall continue to operate as intended".to_string(),
                "No degradation of performance below specified limits".to_string(),
            ],
        });
        
        // IEC 61000-4-2 ESD Immunity
        let mut esd_immunity = HashMap::new();
        esd_immunity.insert("contact".to_string(), 4000.0); // Contact discharge - 4 kV
        esd_immunity.insert("air".to_string(), 8000.0); // Air discharge - 8 kV
        
        standards.insert(ImmunityStandard::IEC61000_4_2, ImmunityRequirements {
            test_levels: esd_immunity,
            performance_criteria: vec![PerformanceCriteria::CriterionB],
            protection_requirements: vec![
                "Temporary loss of function acceptable".to_string(),
                "Automatic recovery required".to_string(),
            ],
        });
        
        standards
    }

    pub async fn analyze_emi_emc(
        &mut self,
        netlist: &Netlist,
        analysis: &AnalysisResult
    ) -> Result<EMIEMCAnalysisResult> {
        let start_time = std::time::Instant::now();
        info!("Starting EMI/EMC analysis for {} components", netlist.instances.len());

        // Phase 1: Load component EMI/EMC characteristics
        self.load_component_characteristics(netlist)?;
        
        // Phase 2: Analyze emissions
        let emission_compliance = self.analyze_emissions(netlist, analysis)
            .context("Failed to analyze emissions")?;
        
        // Phase 3: Assess immunity
        let immunity_assessment = self.assess_immunity(netlist, analysis)
            .context("Failed to assess immunity")?;
        
        // Phase 4: Analyze internal interference
        let interference_analysis = self.analyze_interference(netlist, analysis)
            .context("Failed to analyze interference")?;
        
        // Phase 5: Generate mitigation recommendations
        let mitigation_recommendations = self.generate_mitigation_recommendations(
            &emission_compliance,
            &immunity_assessment,
            &interference_analysis,
            netlist
        ).context("Failed to generate mitigation recommendations")?;
        
        // Phase 6: Create compliance summary
        let compliance_summary = self.create_compliance_summary(
            &emission_compliance,
            &immunity_assessment,
            &mitigation_recommendations
        );
        
        let analysis_time = start_time.elapsed().as_secs_f64();
        
        let result = EMIEMCAnalysisResult {
            emission_compliance,
            immunity_assessment,
            interference_analysis,
            mitigation_recommendations: mitigation_recommendations.clone(),
            compliance_summary,
            analysis_summary: AnalysisSummary {
                components_analyzed: netlist.instances.len() as u32,
                nets_analyzed: netlist.nets.len() as u32,
                frequencies_analyzed: self.calculate_analysis_points(),
                analysis_time_seconds: analysis_time,
                prediction_confidence: self.calculate_prediction_confidence(netlist),
                recommendations_generated: mitigation_recommendations.len() as u32,
            },
        };

        info!("EMI/EMC analysis completed in {:.2}s", analysis_time);
        info!("Found {} compliance issues", result.compliance_summary.total_issues);
        info!("Generated {} mitigation recommendations", result.analysis_summary.recommendations_generated);

        Ok(result)
    }

    fn load_component_characteristics(&mut self, netlist: &Netlist) -> Result<()> {
        info!("Loading EMI/EMC characteristics for {} components", netlist.instances.len());
        
        for (instance_id, instance) in &netlist.instances {
            let characteristics = self.get_component_emi_profile(&instance.name);
            self.component_characteristics.insert(instance.name.clone(), characteristics);
        }
        
        debug!("Loaded characteristics for {} component types", self.component_characteristics.len());
        Ok(())
    }

    fn get_component_emi_profile(&self, component_type: &str) -> ComponentEMIProfile {
        // In a real implementation, this would load from a component database
        // For now, we'll generate realistic profiles based on component type
        match component_type.to_lowercase().as_str() {
            s if s.contains("microcontroller") || s.contains("mcu") => ComponentEMIProfile {
                component_type: component_type.to_string(),
                emission_characteristics: EmissionProfile {
                    peak_emissions: [
                        ("8000000".to_string(), 45.0),
                        ("16000000".to_string(), 42.0),
                        ("24000000".to_string(), 38.0),
                    ].into_iter().collect(),
                    broadband_noise_floor: 25.0,
                    harmonic_content: Some(15.0),
                    rise_time: Some(2.0), // 2 ns
                    edge_rate: Some(1.5), // 1.5 V/ns
                },
                susceptibility_characteristics: SusceptibilityProfile {
                    susceptible_frequencies: [
                        ("100000000".to_string(), 120.0),
                        ("1000000000".to_string(), 100.0),
                    ].into_iter().collect(),
                    immunity_level: 3.0,
                    bandwidth_sensitivity: Some(10_000_000.0), // 10 MHz
                    input_impedance: Some(1_000_000.0), // 1 MΩ
                },
                shielding_effectiveness: None,
                bandwidth: Some(20_000_000.0), // 20 MHz
                slew_rate: Some(100_000_000.0), // 100 V/s
                switching_frequency: Some(8_000_000.0), // 8 MHz
                current_consumption: Some(0.050), // 50 mA
            },
            s if s.contains("regulator") || s.contains("pmic") => ComponentEMIProfile {
                component_type: component_type.to_string(),
                emission_characteristics: EmissionProfile {
                    peak_emissions: [
                        ("500000".to_string(), 50.0),
                        ("1000000".to_string(), 45.0),
                        ("2000000".to_string(), 40.0),
                    ].into_iter().collect(),
                    broadband_noise_floor: 30.0,
                    harmonic_content: Some(20.0),
                    rise_time: Some(10.0), // 10 ns
                    edge_rate: Some(0.5),  // 0.5 V/ns
                },
                susceptibility_characteristics: SusceptibilityProfile {
                    susceptible_frequencies: [
                        ("1000000".to_string(), 80.0),
                        ("10000000".to_string(), 90.0),
                    ].into_iter().collect(),
                    immunity_level: 10.0,
                    bandwidth_sensitivity: Some(1_000_000.0), // 1 MHz
                    input_impedance: Some(50.0), // 50 Ω
                },
                shielding_effectiveness: None,
                bandwidth: Some(1_000_000.0), // 1 MHz
                slew_rate: Some(10_000_000.0), // 10 V/s
                switching_frequency: Some(500_000.0), // 500 kHz
                current_consumption: Some(2.0), // 2 A
            },
            s if s.contains("oscillator") || s.contains("crystal") => ComponentEMIProfile {
                component_type: component_type.to_string(),
                emission_characteristics: EmissionProfile {
                    peak_emissions: [
                        ("16000000".to_string(), 40.0),
                        ("32000000".to_string(), 35.0),
                        ("48000000".to_string(), 30.0),
                    ].into_iter().collect(),
                    broadband_noise_floor: 15.0,
                    harmonic_content: Some(10.0),
                    rise_time: Some(1.0), // 1 ns
                    edge_rate: Some(2.0), // 2 V/ns
                },
                susceptibility_characteristics: SusceptibilityProfile {
                    susceptible_frequencies: [
                        ("15000000".to_string(), 100.0),
                        ("17000000".to_string(), 95.0),
                    ].into_iter().collect(),
                    immunity_level: 1.0, // Very susceptible
                    bandwidth_sensitivity: Some(100_000.0), // 100 kHz
                    input_impedance: Some(1_000_000.0), // 1 MΩ
                },
                shielding_effectiveness: None,
                bandwidth: Some(100_000.0), // 100 kHz
                slew_rate: Some(500_000_000.0), // 500 V/s
                switching_frequency: Some(16_000_000.0), // 16 MHz
                current_consumption: Some(0.001), // 1 mA
            },
            s if s.contains("opamp") || s.contains("amplifier") => ComponentEMIProfile {
                component_type: component_type.to_string(),
                emission_characteristics: EmissionProfile {
                    peak_emissions: [
                        ("1000000".to_string(), 25.0),
                        ("10000000".to_string(), 20.0),
                    ].into_iter().collect(),
                    broadband_noise_floor: 10.0,
                    harmonic_content: Some(5.0),
                    rise_time: Some(50.0), // 50 ns
                    edge_rate: Some(0.1),  // 0.1 V/ns
                },
                susceptibility_characteristics: SusceptibilityProfile {
                    susceptible_frequencies: [
                        ("100000000".to_string(), 60.0),
                        ("1000000000".to_string(), 80.0),
                    ].into_iter().collect(),
                    immunity_level: 0.5, // Very susceptible to interference
                    bandwidth_sensitivity: Some(100_000_000.0), // 100 MHz
                    input_impedance: Some(1_000_000_000.0), // 1 GΩ
                },
                shielding_effectiveness: None,
                bandwidth: Some(100_000_000.0), // 100 MHz
                slew_rate: Some(1_000_000.0), // 1 V/s
                switching_frequency: None,
                current_consumption: Some(0.005), // 5 mA
            },
            _ => ComponentEMIProfile {
                component_type: component_type.to_string(),
                emission_characteristics: EmissionProfile {
                    peak_emissions: HashMap::new(),
                    broadband_noise_floor: 20.0,
                    harmonic_content: None,
                    rise_time: None,
                    edge_rate: None,
                },
                susceptibility_characteristics: SusceptibilityProfile {
                    susceptible_frequencies: HashMap::new(),
                    immunity_level: 5.0,
                    bandwidth_sensitivity: None,
                    input_impedance: None,
                },
                shielding_effectiveness: None,
                bandwidth: None,
                slew_rate: None,
                switching_frequency: None,
                current_consumption: Some(0.001), // 1 mA default
            },
        }
    }

    fn analyze_emissions(&self, netlist: &Netlist, _analysis: &AnalysisResult) -> Result<EmissionComplianceResult> {
        info!("Analyzing emissions compliance for {} standards", self.analysis_config.target_standards.len());
        
        let mut emission_hotspots = Vec::new();
        let mut frequency_violations = Vec::new();
        let mut margin_analysis = Vec::new();
        
        // Analyze each component for emission hotspots
        for (instance_id, instance) in &netlist.instances {
            if let Some(characteristics) = self.component_characteristics.get(&instance.name) {
                for (frequency_str, emission_level) in &characteristics.emission_characteristics.peak_emissions {
                    if let Ok(frequency) = frequency_str.parse::<f64>() {
                        emission_hotspots.push(EmissionHotspot {
                            component_id: instance_id,
                            component_name: instance.name.clone(),
                            emission_frequency: frequency,
                            emission_level: *emission_level,
                            contribution_percentage: self.calculate_contribution_percentage(*emission_level),
                            hotspot_type: self.classify_hotspot_type(&instance.name, frequency),
                        });
                    }
                }
            }
        }
        
        // Check compliance against standards
        for standard in &self.analysis_config.target_standards {
            if let Some(limits) = self.emission_standards.get(standard) {
                self.check_emission_compliance(&limits, &emission_hotspots, &mut frequency_violations, &mut margin_analysis);
            }
        }
        
        let conducted_status = self.calculate_compliance_status(&frequency_violations, "conducted");
        let radiated_status = self.calculate_compliance_status(&frequency_violations, "radiated");
        let harmonic_status = self.calculate_harmonic_compliance(&emission_hotspots);
        
        Ok(EmissionComplianceResult {
            standards_tested: self.analysis_config.target_standards.clone(),
            conducted_emissions: conducted_status,
            radiated_emissions: radiated_status,
            harmonic_emissions: harmonic_status,
            emission_hotspots,
            margin_analysis,
        })
    }

    fn assess_immunity(&self, netlist: &Netlist, _analysis: &AnalysisResult) -> Result<ImmunityAssessmentResult> {
        info!("Assessing immunity for {} standards", self.analysis_config.immunity_standards.len());
        
        let mut vulnerable_components = Vec::new();
        let mut immunity_gaps = Vec::new();
        
        // Analyze each component for susceptibility
        for (instance_id, instance) in &netlist.instances {
            if let Some(characteristics) = self.component_characteristics.get(&instance.name) {
                for (frequency_str, threshold) in &characteristics.susceptibility_characteristics.susceptible_frequencies {
                    if let Ok(frequency) = frequency_str.parse::<f64>() {
                        let vulnerability_type = self.classify_vulnerability_type(&instance.name);
                        let risk_level = self.assess_risk_level(*threshold, characteristics.susceptibility_characteristics.immunity_level);
                        
                        vulnerable_components.push(VulnerableComponent {
                            component_id: instance_id,
                            component_name: instance.name.clone(),
                            vulnerability_type,
                            susceptible_frequency: frequency,
                            immunity_threshold: *threshold,
                            risk_level,
                        });
                    }
                }
            }
        }
        
        // Calculate overall immunity level
        let overall_immunity = self.calculate_overall_immunity_level(&vulnerable_components);
        
        // Build frequency response
        let frequency_response = self.build_immunity_frequency_response(&vulnerable_components);
        
        // Identify critical frequencies
        let critical_frequencies = self.identify_critical_frequencies(&frequency_response);
        
        // Calculate protection effectiveness
        let protection_effectiveness = self.calculate_protection_effectiveness(&vulnerable_components);
        
        Ok(ImmunityAssessmentResult {
            standards_assessed: self.analysis_config.immunity_standards.clone(),
            susceptibility_analysis: SusceptibilityAnalysis {
                overall_immunity_level: overall_immunity,
                frequency_response,
                critical_frequencies,
                protection_effectiveness,
            },
            vulnerable_components,
            immunity_gaps,
        })
    }

    fn analyze_interference(&self, netlist: &Netlist, _analysis: &AnalysisResult) -> Result<InterferenceAnalysisResult> {
        info!("Analyzing internal interference and signal integrity");
        
        let mut internal_interference = Vec::new();
        let mut crosstalk_pairs = Vec::new();
        let mut power_integrity_issues = Vec::new();
        let mut signal_integrity_concerns = Vec::new();
        
        // Analyze component-to-component interference
        for (source_id, source_instance) in &netlist.instances {
            for (victim_id, victim_instance) in &netlist.instances {
                if source_id != victim_id {
                    if let (Some(source_char), Some(victim_char)) = (
                        self.component_characteristics.get(&source_instance.name),
                        self.component_characteristics.get(&victim_instance.name)
                    ) {
                        // Check for potential interference
                        for (source_freq_str, source_level) in &source_char.emission_characteristics.peak_emissions {
                            for (victim_freq_str, victim_threshold) in &victim_char.susceptibility_characteristics.susceptible_frequencies {
                                if let (Ok(source_freq), Ok(victim_freq)) = (
                                    source_freq_str.parse::<f64>(),
                                    victim_freq_str.parse::<f64>()
                                ) {
                                    if self.frequencies_interfere(source_freq, victim_freq) {
                                        let coupling = self.estimate_coupling_mechanism(source_instance, victim_instance);
                                        let impact = self.assess_interference_impact(*source_level, *victim_threshold);
                                        
                                        internal_interference.push(InterferenceSource {
                                            source_component: source_id,
                                            victim_component: victim_id,
                                            interference_frequency: source_freq,
                                            interference_level: *source_level,
                                            coupling_mechanism: coupling,
                                            impact_severity: impact,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Analyze crosstalk between nets
        if self.analysis_config.include_crosstalk_analysis {
            for (net1_id, _net1) in &netlist.nets {
                for (net2_id, _net2) in &netlist.nets {
                    if net1_id != net2_id {
                        let crosstalk_level = self.calculate_crosstalk(net1_id, net2_id, netlist);
                        if crosstalk_level > -30.0 { // Threshold: -30 dB
                            crosstalk_pairs.push(CrosstalkPair {
                                aggressor_net: net1_id,
                                victim_net: net2_id,
                                crosstalk_level,
                                frequency: 100_000_000.0, // 100 MHz typical
                                coupling_length: 10.0,    // 10 mm typical
                            });
                        }
                    }
                }
            }
        }
        
        // Analyze power integrity
        if self.analysis_config.include_power_integrity {
            for (net_id, net) in &netlist.nets {
                if self.is_power_net(net) {
                    let impedance_issues = self.analyze_power_net_impedance(net_id, net);
                    power_integrity_issues.extend(impedance_issues);
                }
            }
        }
        
        Ok(InterferenceAnalysisResult {
            internal_interference,
            crosstalk_analysis: CrosstalkAnalysis {
                near_end_crosstalk: crosstalk_pairs.clone(),
                far_end_crosstalk: crosstalk_pairs,
                worst_case_crosstalk: -20.0, // dB
                acceptable_crosstalk_threshold: -30.0, // dB
            },
            power_integrity_issues,
            signal_integrity_concerns,
        })
    }

    fn generate_mitigation_recommendations(
        &self,
        emission_compliance: &EmissionComplianceResult,
        immunity_assessment: &ImmunityAssessmentResult,
        interference_analysis: &InterferenceAnalysisResult,
        netlist: &Netlist
    ) -> Result<Vec<MitigationRecommendation>> {
        let mut recommendations = Vec::new();
        let mut recommendation_id = 1;
        
        // Generate recommendations for emission violations
        for violation in &emission_compliance.conducted_emissions.frequency_violations {
            recommendations.push(MitigationRecommendation {
                recommendation_id: format!("EMI-{:03}", recommendation_id),
                priority: if violation.margin < -6.0 { MitigationPriority::Critical } else { MitigationPriority::High },
                recommendation_type: MitigationType::FilteringEnhancement,
                description: format!(
                    "Add conducted emission filtering at {:.1} MHz (violation: {:.1} dB)",
                    violation.frequency / 1_000_000.0,
                    violation.margin.abs()
                ),
                implementation_cost: CostEstimate::Medium,
                effectiveness: 0.8,
                affected_components: violation.source_components.clone(),
                implementation_notes: vec![
                    "Consider adding common mode choke".to_string(),
                    "Add X and Y capacitors at power input".to_string(),
                    "Ensure proper grounding of filter".to_string(),
                ],
            });
            recommendation_id += 1;
        }
        
        // Generate recommendations for radiated emissions
        for violation in &emission_compliance.radiated_emissions.frequency_violations {
            recommendations.push(MitigationRecommendation {
                recommendation_id: format!("RAD-{:03}", recommendation_id),
                priority: if violation.margin < -6.0 { MitigationPriority::Critical } else { MitigationPriority::High },
                recommendation_type: MitigationType::ShieldingImprovement,
                description: format!(
                    "Add shielding for radiated emission at {:.1} MHz (violation: {:.1} dB)",
                    violation.frequency / 1_000_000.0,
                    violation.margin.abs()
                ),
                implementation_cost: CostEstimate::High,
                effectiveness: 0.9,
                affected_components: violation.source_components.clone(),
                implementation_notes: vec![
                    "Consider board-level shielding can".to_string(),
                    "Improve ground plane coverage".to_string(),
                    "Reduce loop areas in critical traces".to_string(),
                ],
            });
            recommendation_id += 1;
        }
        
        // Generate recommendations for vulnerable components
        for vulnerable in &immunity_assessment.vulnerable_components {
            if matches!(vulnerable.risk_level, RiskLevel::High | RiskLevel::Critical) {
                recommendations.push(MitigationRecommendation {
                    recommendation_id: format!("IMM-{:03}", recommendation_id),
                    priority: match vulnerable.risk_level {
                        RiskLevel::Critical => MitigationPriority::Critical,
                        RiskLevel::High => MitigationPriority::High,
                        _ => MitigationPriority::Medium,
                    },
                    recommendation_type: MitigationType::FilteringEnhancement,
                    description: format!(
                        "Improve immunity for {} at {:.1} MHz",
                        vulnerable.component_name,
                        vulnerable.susceptible_frequency / 1_000_000.0
                    ),
                    implementation_cost: CostEstimate::Low,
                    effectiveness: 0.7,
                    affected_components: vec![vulnerable.component_id],
                    implementation_notes: vec![
                        "Add input filtering".to_string(),
                        "Improve power supply decoupling".to_string(),
                        "Consider shielded components".to_string(),
                    ],
                });
                recommendation_id += 1;
            }
        }
        
        // Generate recommendations for interference issues
        for interference in &interference_analysis.internal_interference {
            if matches!(interference.impact_severity, ImpactSeverity::Severe | ImpactSeverity::Critical) {
                recommendations.push(MitigationRecommendation {
                    recommendation_id: format!("INT-{:03}", recommendation_id),
                    priority: match interference.impact_severity {
                        ImpactSeverity::Critical => MitigationPriority::Critical,
                        ImpactSeverity::Severe => MitigationPriority::High,
                        _ => MitigationPriority::Medium,
                    },
                    recommendation_type: MitigationType::ComponentPlacement,
                    description: format!(
                        "Reduce interference between components at {:.1} MHz",
                        interference.interference_frequency / 1_000_000.0
                    ),
                    implementation_cost: CostEstimate::DesignChange,
                    effectiveness: 0.8,
                    affected_components: vec![interference.source_component, interference.victim_component],
                    implementation_notes: vec![
                        "Increase physical separation".to_string(),
                        "Add shielding between components".to_string(),
                        "Improve grounding and filtering".to_string(),
                    ],
                });
                recommendation_id += 1;
            }
        }
        
        info!("Generated {} mitigation recommendations", recommendations.len());
        Ok(recommendations)
    }

    fn create_compliance_summary(
        &self,
        emission_compliance: &EmissionComplianceResult,
        immunity_assessment: &ImmunityAssessmentResult,
        mitigation_recommendations: &[MitigationRecommendation]
    ) -> ComplianceSummary {
        let critical_issues = mitigation_recommendations.iter()
            .filter(|r| matches!(r.priority, MitigationPriority::Critical))
            .count() as u32;
        
        let high_priority_issues = mitigation_recommendations.iter()
            .filter(|r| matches!(r.priority, MitigationPriority::High))
            .count() as u32;
        
        let overall_compliance = if critical_issues > 0 {
            ComplianceLevel::Fail(6.0) // Assume 6 dB worst case
        } else if high_priority_issues > 0 {
            ComplianceLevel::Marginal(3.0) // Assume 3 dB margin
        } else {
            ComplianceLevel::PassWithMargin(6.0) // Good margin
        };
        
        let estimated_fix_cost = if critical_issues > 2 {
            CostEstimate::VeryHigh
        } else if critical_issues > 0 || high_priority_issues > 3 {
            CostEstimate::High
        } else if high_priority_issues > 0 {
            CostEstimate::Medium
        } else {
            CostEstimate::Low
        };
        
        ComplianceSummary {
            overall_compliance,
            standards_passed: vec![], // Would be calculated based on actual compliance
            standards_failed: emission_compliance.standards_tested.clone(),
            critical_issues,
            high_priority_issues,
            total_issues: mitigation_recommendations.len() as u32,
            estimated_fix_cost,
        }
    }

    // Helper methods for analysis
    fn calculate_analysis_points(&self) -> u32 {
        let (start_freq, end_freq) = self.analysis_config.frequency_range;
        let resolution = self.analysis_config.analysis_resolution;
        ((end_freq - start_freq) / resolution) as u32
    }

    fn calculate_prediction_confidence(&self, netlist: &Netlist) -> f64 {
        // Confidence based on component characterization completeness
        let characterized_components = self.component_characteristics.len();
        let total_components = netlist.instances.len();
        
        if total_components == 0 {
            0.0
        } else {
            (characterized_components as f64 / total_components as f64) * 0.8 + 0.2
        }
    }

    fn calculate_contribution_percentage(&self, emission_level: f64) -> f64 {
        // Simplified contribution calculation
        (emission_level / 100.0) * 100.0
    }

    fn classify_hotspot_type(&self, component_name: &str, frequency: f64) -> HotspotType {
        match component_name.to_lowercase().as_str() {
            s if s.contains("microcontroller") || s.contains("mcu") => {
                if frequency > 10_000_000.0 {
                    HotspotType::DigitalSwitching
                } else {
                    HotspotType::ClockSignal
                }
            },
            s if s.contains("regulator") || s.contains("pmic") => HotspotType::PowerSwitching,
            s if s.contains("oscillator") || s.contains("crystal") => HotspotType::Oscillator,
            s if s.contains("serdes") || s.contains("ethernet") => HotspotType::HighSpeedData,
            _ => HotspotType::PowerSupplyNoise,
        }
    }

    fn check_emission_compliance(
        &self,
        limits: &EmissionLimits,
        hotspots: &[EmissionHotspot],
        frequency_violations: &mut Vec<FrequencyViolation>,
        margin_analysis: &mut Vec<MarginAnalysis>
    ) {
        for hotspot in hotspots {
            // Check conducted emissions
            if let Some(limit) = self.interpolate_limit(&limits.conducted_limits, hotspot.emission_frequency) {
                let margin = limit - hotspot.emission_level;
                
                margin_analysis.push(MarginAnalysis {
                    frequency: hotspot.emission_frequency,
                    margin,
                    margin_type: MarginType::Conducted,
                    confidence: 0.8,
                });
                
                if margin < 0.0 {
                    frequency_violations.push(FrequencyViolation {
                        frequency: hotspot.emission_frequency,
                        predicted_level: hotspot.emission_level,
                        limit,
                        margin,
                        source_components: vec![hotspot.component_id],
                    });
                }
            }
            
            // Check radiated emissions
            if let Some(limit) = self.interpolate_limit(&limits.radiated_limits, hotspot.emission_frequency) {
                let margin = limit - hotspot.emission_level;
                
                margin_analysis.push(MarginAnalysis {
                    frequency: hotspot.emission_frequency,
                    margin,
                    margin_type: MarginType::Radiated,
                    confidence: 0.7, // Lower confidence for radiated predictions
                });
                
                if margin < 0.0 {
                    frequency_violations.push(FrequencyViolation {
                        frequency: hotspot.emission_frequency,
                        predicted_level: hotspot.emission_level,
                        limit,
                        margin,
                        source_components: vec![hotspot.component_id],
                    });
                }
            }
        }
    }

    fn interpolate_limit(&self, limits: &HashMap<String, f64>, frequency: f64) -> Option<f64> {
        if limits.is_empty() {
            return None;
        }
        
        // Convert frequency to string and try exact match first
        let freq_str = frequency.to_string();
        if let Some(&limit) = limits.get(&freq_str) {
            return Some(limit);
        }
        
        // Parse all frequencies and find closest matches
        let mut freq_limits: Vec<(f64, f64)> = limits.iter()
            .filter_map(|(freq_str, &limit)| {
                freq_str.parse::<f64>().ok().map(|freq| (freq, limit))
            })
            .collect();
        
        if freq_limits.is_empty() {
            return None;
        }
        
        freq_limits.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        
        // Find the two closest frequency points
        let mut below = None;
        let mut above = None;
        
        for (freq, limit) in freq_limits {
            if freq <= frequency {
                below = Some((freq, limit));
            } else if above.is_none() {
                above = Some((freq, limit));
                break;
            }
        }
        
        match (below, above) {
            (Some((f1, l1)), Some((f2, l2))) => {
                // Linear interpolation
                let ratio = (frequency - f1) / (f2 - f1);
                Some(l1 + ratio * (l2 - l1))
            },
            (Some((_, limit)), None) => Some(limit), // Use last limit
            (None, Some((_, limit))) => Some(limit), // Use first limit
            (None, None) => None,
        }
    }

    fn calculate_compliance_status(&self, violations: &[FrequencyViolation], emission_type: &str) -> ComplianceStatus {
        let type_violations: Vec<_> = violations.iter()
            .filter(|v| {
                // Simple filtering based on frequency range
                match emission_type {
                    "conducted" => v.frequency < 30_000_000.0,
                    "radiated" => v.frequency >= 30_000_000.0,
                    _ => true,
                }
            })
            .collect();
        
        let worst_case_margin = type_violations.iter()
            .map(|v| v.margin)
            .fold(0.0f64, |acc, margin| acc.min(margin));
        
        let total_points = 100; // Assume 100 frequency points checked
        let violation_points = type_violations.len();
        let compliance_percentage = ((total_points - violation_points) as f64 / total_points as f64) * 100.0;
        
        let overall_status = if worst_case_margin < -6.0 {
            ComplianceLevel::Fail(worst_case_margin.abs())
        } else if worst_case_margin < 0.0 {
            ComplianceLevel::Marginal(worst_case_margin.abs())
        } else if worst_case_margin > 6.0 {
            ComplianceLevel::PassWithMargin(worst_case_margin)
        } else {
            ComplianceLevel::Pass
        };
        
        ComplianceStatus {
            overall_status,
            frequency_violations: type_violations.into_iter().cloned().collect(),
            worst_case_margin,
            compliance_percentage,
        }
    }

    fn calculate_harmonic_compliance(&self, _hotspots: &[EmissionHotspot]) -> ComplianceStatus {
        // Simplified harmonic compliance check
        ComplianceStatus {
            overall_status: ComplianceLevel::Pass,
            frequency_violations: vec![],
            worst_case_margin: 6.0,
            compliance_percentage: 100.0,
        }
    }

    fn classify_vulnerability_type(&self, component_name: &str) -> VulnerabilityType {
        match component_name.to_lowercase().as_str() {
            s if s.contains("opamp") || s.contains("amplifier") => VulnerabilityType::HighGainAmplifiers,
            s if s.contains("adc") || s.contains("dac") => VulnerabilityType::DataConverters,
            s if s.contains("oscillator") || s.contains("pll") => VulnerabilityType::ClockGenerators,
            s if s.contains("reference") || s.contains("vref") => VulnerabilityType::PrecisionReferences,
            s if s.contains("uart") || s.contains("spi") || s.contains("i2c") => VulnerabilityType::CommunicationInterfaces,
            s if s.contains("regulator") || s.contains("pmic") => VulnerabilityType::PowerManagement,
            _ => VulnerabilityType::AnalogCircuits,
        }
    }

    fn assess_risk_level(&self, threshold: f64, immunity_level: f64) -> RiskLevel {
        let margin = immunity_level - threshold;
        
        if margin < -10.0 {
            RiskLevel::Critical
        } else if margin < -5.0 {
            RiskLevel::High
        } else if margin < 0.0 {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        }
    }

    fn calculate_overall_immunity_level(&self, vulnerable_components: &[VulnerableComponent]) -> f64 {
        if vulnerable_components.is_empty() {
            return 10.0; // Default high immunity
        }
        
        let sum: f64 = vulnerable_components.iter()
            .map(|v| v.immunity_threshold)
            .sum();
        
        sum / vulnerable_components.len() as f64
    }

    fn build_immunity_frequency_response(&self, vulnerable_components: &[VulnerableComponent]) -> HashMap<String, f64> {
        let mut response = HashMap::new();
        
        for component in vulnerable_components {
            response.insert(component.susceptible_frequency.to_string(), component.immunity_threshold);
        }
        
        response
    }

    fn identify_critical_frequencies(&self, frequency_response: &HashMap<String, f64>) -> Vec<f64> {
        frequency_response.iter()
            .filter(|(_, &threshold)| threshold < 5.0) // Critical if threshold < 5 dBμV/m
            .filter_map(|(freq_str, _)| freq_str.parse::<f64>().ok())
            .collect()
    }

    fn calculate_protection_effectiveness(&self, vulnerable_components: &[VulnerableComponent]) -> f64 {
        if vulnerable_components.is_empty() {
            return 100.0;
        }
        
        let protected_count = vulnerable_components.iter()
            .filter(|v| matches!(v.risk_level, RiskLevel::Low | RiskLevel::Medium))
            .count();
        
        (protected_count as f64 / vulnerable_components.len() as f64) * 100.0
    }

    fn frequencies_interfere(&self, source_freq: f64, victim_freq: f64) -> bool {
        let tolerance = 0.1; // 10% frequency tolerance
        (source_freq - victim_freq).abs() / victim_freq < tolerance
    }

    fn estimate_coupling_mechanism(&self, _source: &Instance, _victim: &Instance) -> CouplingMechanism {
        // Simplified coupling estimation - would use physical layout in real implementation
        CouplingMechanism::ConductiveCoupling
    }

    fn assess_interference_impact(&self, source_level: f64, victim_threshold: f64) -> ImpactSeverity {
        let interference_margin = source_level - victim_threshold;
        
        if interference_margin > 20.0 {
            ImpactSeverity::Critical
        } else if interference_margin > 10.0 {
            ImpactSeverity::Severe
        } else if interference_margin > 5.0 {
            ImpactSeverity::Moderate
        } else if interference_margin > 0.0 {
            ImpactSeverity::Minor
        } else {
            ImpactSeverity::Negligible
        }
    }

    fn calculate_crosstalk(&self, _net1: NetId, _net2: NetId, _netlist: &Netlist) -> f64 {
        // Simplified crosstalk calculation - would use actual trace geometry
        -25.0 + (rand::random::<f64>() * 10.0) // -25 to -15 dB
    }

    fn is_power_net(&self, net: &Net) -> bool {
        if let Some(name) = &net.name {
            let name_lower = name.to_lowercase();
            name_lower.contains("vcc") || name_lower.contains("vdd") || name_lower.contains("power") || name_lower.contains("gnd")
        } else {
            false
        }
    }

    fn analyze_power_net_impedance(&self, net_id: NetId, _net: &Net) -> Vec<PowerIntegrityIssue> {
        // Simplified power integrity analysis
        vec![
            PowerIntegrityIssue {
                power_net: net_id,
                issue_type: PowerIntegrityIssueType::PowerSupplyNoise,
                frequency: 1_000_000.0, // 1 MHz
                impedance: 0.1,         // 0.1 Ω
                voltage_ripple: 0.05,   // 50 mV
                affected_components: vec![],
            }
        ]
    }
}

impl Default for BoardEMIProfile {
    fn default() -> Self {
        Self {
            board_size: (100.0, 80.0), // 100mm x 80mm
            layer_count: 4,
            ground_plane_coverage: 90.0, // 90%
            power_plane_coverage: 80.0,  // 80%
            trace_impedance: 50.0,       // 50 Ω
            via_count: 200,
            clock_frequencies: vec![8_000_000.0, 16_000_000.0], // 8 MHz, 16 MHz
            power_consumption: 5.0,      // 5 W
            enclosure_shielding: Some(40.0), // 40 dB
        }
    }
}

impl Default for EMIEMCConfig {
    fn default() -> Self {
        Self {
            target_standards: vec![EmissionStandard::CISPR22, EmissionStandard::FCC15],
            immunity_standards: vec![ImmunityStandard::IEC61000_4_2, ImmunityStandard::IEC61000_4_3],
            frequency_range: (9_000.0, 1_000_000_000.0), // 9 kHz to 1 GHz
            analysis_resolution: 100_000.0, // 100 kHz
            enable_prediction: true,
            enable_mitigation_suggestions: true,
            include_crosstalk_analysis: true,
            include_power_integrity: true,
            safety_margin: 6.0, // 6 dB
        }
    }
}