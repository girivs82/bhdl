// Reliability and Lifecycle Analysis
// Provides comprehensive component reliability assessment, failure prediction, and lifecycle management

use bhdl_netlist::{Netlist, InstanceId, Instance, NetId, Net};
use bhdl_analyzer::AnalysisResult;
use std::collections::{HashMap, HashSet};
use serde::{Serialize, Deserialize};
use anyhow::{Result, Context};
use log::{info, warn, debug, error};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReliabilityAnalyzer {
    component_reliability_data: HashMap<String, ComponentReliabilityProfile>,
    environmental_conditions: EnvironmentalConditions,
    analysis_config: ReliabilityConfig,
    failure_models: HashMap<FailureMode, FailureModel>,
    lifecycle_database: LifecycleDatabase,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentReliabilityProfile {
    component_type: String,
    base_failure_rate: f64,           // Failures per hour (λ)
    mtbf: f64,                        // Mean Time Between Failures (hours)
    mttf: f64,                        // Mean Time To Failure (hours)
    activation_energy: f64,           // eV (for Arrhenius model)
    temperature_coefficient: f64,     // /°C
    voltage_stress_factor: f64,       // Factor for voltage derating
    current_stress_factor: f64,       // Factor for current derating
    failure_modes: Vec<ComponentFailureMode>,
    wear_out_period: f64,             // Hours when wear-out begins
    infant_mortality_period: f64,     // Hours of infant mortality
    quality_factors: QualityFactors,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentFailureMode {
    mode: FailureMode,
    probability: f64,                 // Relative probability (0.0-1.0)
    detection_method: DetectionMethod,
    impact_severity: ImpactSeverity,
    mitigation_strategies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum FailureMode {
    // Electronic component failures
    OpenCircuit,
    ShortCircuit,
    ParametricDrift,
    ElectromigrationFailure,
    ThermalRunaway,
    DielectricBreakdown,
    BondWireFailure,
    SolderJointFailure,
    CorrosionFailure,
    
    // Semiconductor-specific failures
    GateOxideBreakdown,
    HotCarrierInjection,
    NegativeBiasTemperatureInstability,
    TimeDependendDielectricBreakdown,
    LatchUp,
    SingleEventUpset,
    
    // Passive component failures
    CapacitorDegradation,
    ResistorDrift,
    InductorSaturation,
    CrystalFrequencyDrift,
    
    // Mechanical failures
    VibrationFailure,
    ShockFailure,
    FatigueFailure,
    WearOut,
    
    // Environmental failures
    MoistureIngress,
    ContaminationFailure,
    RadiationDamage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DetectionMethod {
    ElectricalTest,
    ThermalImaging,
    VibrationAnalysis,
    VisualInspection,
    FunctionalTest,
    BuiltInSelfTest,
    ContinuousMonitoring,
    PeriodicInspection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImpactSeverity {
    Catastrophic,    // Complete system failure
    Critical,        // Major function loss
    Marginal,        // Degraded performance
    Negligible,      // Minor impact
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityFactors {
    manufacturer_grade: ManufacturerGrade,
    screening_level: ScreeningLevel,
    package_type: PackageType,
    technology_maturity: TechnologyMaturity,
    quality_multiplier: f64,          // Overall quality factor
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ManufacturerGrade {
    MilitaryGrade,       // Highest reliability
    AutomotiveGrade,     // High reliability
    IndustrialGrade,     // Standard reliability
    CommercialGrade,     // Basic reliability
    ConsumerGrade,       // Lowest reliability
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScreeningLevel {
    FullMilitary,        // 100% screening
    EnhancedCommercial,  // Extended screening
    StandardCommercial,  // Basic screening
    NoScreening,         // No special screening
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PackageType {
    Hermetic,            // Sealed packages (highest reliability)
    Plastic,             // Standard plastic packages
    FlipChip,            // Advanced packaging
    ChipOnBoard,         // Direct chip mounting
    WaferLevelPackage,   // Miniaturized packaging
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TechnologyMaturity {
    Proven,              // >10 years in production
    Mature,              // 5-10 years in production
    Established,         // 2-5 years in production
    Emerging,            // <2 years in production
    Experimental,        // Development phase
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentalConditions {
    operating_temperature_range: (f64, f64),    // (min, max) °C
    storage_temperature_range: (f64, f64),      // (min, max) °C
    humidity_range: (f64, f64),                 // (min, max) %RH
    vibration_profile: VibrationProfile,
    shock_profile: ShockProfile,
    altitude: f64,                              // meters above sea level
    contamination_level: ContaminationLevel,
    radiation_environment: RadiationEnvironment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VibrationProfile {
    frequency_range: (f64, f64),               // Hz
    acceleration: f64,                         // g's
    duration: f64,                             // hours
    vibration_type: VibrationType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VibrationType {
    Sinusoidal,
    RandomVibration,
    ShockPulse,
    Mixed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShockProfile {
    peak_acceleration: f64,                    // g's
    pulse_duration: f64,                       // milliseconds
    pulse_shape: PulseShape,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PulseShape {
    HalfSine,
    Sawtooth,
    Square,
    TerminalPeak,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContaminationLevel {
    CleanRoom,           // Class 1-10
    Industrial,          // Standard industrial
    Outdoor,             // Outdoor environment
    Harsh,               // Corrosive environment
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadiationEnvironment {
    total_ionizing_dose: f64,                  // rad
    neutron_fluence: f64,                      // neutrons/cm²
    single_event_rate: f64,                    // events/day
    radiation_type: RadiationType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RadiationType {
    TerrestrialBackground,
    Avionics,
    Space,
    Nuclear,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReliabilityConfig {
    pub analysis_period: f64,                 // Hours to analyze
    pub confidence_level: f64,                // Statistical confidence (0.0-1.0)
    pub enable_accelerated_testing: bool,
    pub enable_physics_of_failure: bool,
    pub enable_bayesian_analysis: bool,
    pub enable_prognostics: bool,
    pub temperature_cycling_enabled: bool,
    pub burn_in_hours: f64,
    pub derating_factors: DeratingFactors,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeratingFactors {
    pub voltage_derating: f64,                     // Typical: 0.8 (80% of max)
    pub current_derating: f64,                     // Typical: 0.75 (75% of max)
    pub power_derating: f64,                       // Typical: 0.8 (80% of max)
    pub temperature_derating: f64,                 // °C below max rating
    pub frequency_derating: f64,                   // Typical: 0.8 (80% of max)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureModel {
    model_type: ModelType,
    parameters: HashMap<String, f64>,
    applicable_components: Vec<String>,
    environmental_factors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModelType {
    Arrhenius,                    // Temperature acceleration
    Eyring,                       // Multi-stress acceleration
    PowerLaw,                     // Voltage/current stress
    Weibull,                      // Wear-out modeling
    Exponential,                  // Constant failure rate
    Lognormal,                    // Early failures
    Bathtub,                      // Complete lifecycle
    PhysicsOfFailure,             // Mechanism-based
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleDatabase {
    component_lifecycles: HashMap<String, ComponentLifecycle>,
    obsolescence_predictions: HashMap<String, ObsolescenceRisk>,
    supplier_stability: HashMap<String, SupplierRisk>,
    technology_roadmaps: HashMap<String, TechnologyRoadmap>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentLifecycle {
    introduction_date: String,               // ISO date
    peak_production_period: (String, String), // Start, end dates
    decline_phase_start: String,             // When decline begins
    obsolescence_date: Option<String>,       // Known or predicted
    replacement_components: Vec<String>,     // Recommended replacements
    lifecycle_stage: LifecycleStage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LifecycleStage {
    Development,          // Pre-production
    Introduction,         // Early production
    Growth,              // Increasing production
    Maturity,            // Peak production
    Decline,             // Decreasing production
    Obsolete,            // End of life
    LastTimeBuy,         // Final purchase opportunity
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObsolescenceRisk {
    risk_level: RiskLevel,
    time_to_obsolescence: Option<f64>,       // Years
    contributing_factors: Vec<ObsolescenceDriver>,
    mitigation_strategies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObsolescenceDriver {
    TechnologyEvolution,
    LowVolumeDemand,
    ManufacturerExit,
    ProcessObsolescence,
    MaterialAvailability,
    EnvironmentalRegulation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplierRisk {
    financial_stability: RiskLevel,
    market_share: f64,                       // Percentage
    alternative_suppliers: u32,
    geographic_risk: RiskLevel,
    quality_history: QualityHistory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityHistory {
    defect_rate: f64,                        // PPM
    recall_history: u32,                     // Number of recalls
    certification_status: Vec<String>,       // ISO, TS, etc.
    audit_results: AuditResults,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditResults {
    last_audit_date: String,
    audit_score: f64,                        // 0.0-100.0
    major_findings: u32,
    minor_findings: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechnologyRoadmap {
    current_generation: String,
    next_generation: Option<String>,
    transition_timeline: Option<String>,
    performance_improvements: Vec<String>,
    backward_compatibility: bool,
}

// Analysis Results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReliabilityAnalysisResult {
    pub overall_system_reliability: f64,    // Probability of success
    pub mean_time_between_failures: f64,    // Hours
    pub failure_rate: f64,                  // Failures per hour
    pub component_reliabilities: HashMap<InstanceId, ComponentReliabilityResult>,
    pub critical_components: Vec<CriticalComponent>,
    pub failure_predictions: Vec<FailurePrediction>,
    pub lifecycle_risks: Vec<LifecycleRisk>,
    pub maintenance_recommendations: Vec<MaintenanceRecommendation>,
    pub derating_analysis: DeratingAnalysisResult,
    pub environmental_impact: EnvironmentalImpactResult,
    pub confidence_intervals: ConfidenceIntervals,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentReliabilityResult {
    pub component_id: InstanceId,
    pub component_name: String,
    pub component_type: String,
    pub reliability: f64,                    // Probability of success
    pub failure_rate: f64,                   // Failures per hour
    pub mtbf: f64,                          // Hours
    pub dominant_failure_modes: Vec<FailureMode>,
    pub stress_factors: StressFactors,
    pub derating_compliance: bool,
    pub lifecycle_stage: LifecycleStage,
    pub obsolescence_risk: RiskLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StressFactors {
    thermal_stress: f64,                     // Relative to limits
    electrical_stress: f64,                  // Relative to limits
    mechanical_stress: f64,                  // Relative to limits
    environmental_stress: f64,               // Relative to conditions
    overall_stress: f64,                     // Combined stress factor
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriticalComponent {
    pub component_id: InstanceId,
    pub component_name: String,
    pub criticality_score: f64,              // 0.0-1.0
    pub failure_impact: ImpactSeverity,
    pub single_point_of_failure: bool,
    pub redundancy_available: bool,
    pub recommended_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailurePrediction {
    pub component_id: InstanceId,
    pub component_name: String,
    pub predicted_failure_time: f64,         // Hours from now
    pub prediction_confidence: f64,          // 0.0-1.0
    pub failure_mode: FailureMode,
    pub early_warning_signs: Vec<String>,
    pub preventive_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleRisk {
    pub component_id: InstanceId,
    pub component_name: String,
    pub risk_type: LifecycleRiskType,
    pub risk_level: RiskLevel,
    pub time_horizon: f64,                   // Years
    pub impact_description: String,
    pub mitigation_options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LifecycleRiskType {
    ComponentObsolescence,
    SupplierDiscontinuation,
    TechnologySupersession,
    RegulatoryChange,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceRecommendation {
    pub component_id: InstanceId,
    pub component_name: String,
    pub maintenance_type: MaintenanceType,
    pub recommended_interval: f64,           // Hours
    pub priority: MaintenancePriority,
    pub estimated_cost: f64,                 // Currency units
    pub description: String,
    pub procedures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MaintenanceType {
    PreventiveMaintenance,
    PredictiveMaintenance,
    CorrectiveMaintenance,
    ConditionBasedMaintenance,
    Replacement,
    Calibration,
    Inspection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MaintenancePriority {
    Critical,        // Immediate attention required
    High,           // Schedule within week
    Medium,         // Schedule within month
    Low,            // Schedule within quarter
    Routine,        // Normal maintenance cycle
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeratingAnalysisResult {
    pub overall_derating_compliance: f64,   // Percentage
    pub voltage_derating_status: DeratingStatus,
    pub current_derating_status: DeratingStatus,
    pub thermal_derating_status: DeratingStatus,
    pub power_derating_status: DeratingStatus,
    pub non_compliant_components: Vec<DeratingViolation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeratingStatus {
    pub compliance_percentage: f64,
    pub average_derating_factor: f64,
    pub worst_case_component: Option<InstanceId>,
    pub recommended_improvements: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeratingViolation {
    pub component_id: InstanceId,
    pub component_name: String,
    pub violation_type: String,
    pub actual_stress: f64,
    pub allowable_stress: f64,
    pub margin: f64,                         // Negative indicates violation
    pub severity: ViolationSeverity,
    pub corrective_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViolationSeverity {
    Minor,           // <10% over limit
    Moderate,        // 10-25% over limit
    Severe,          // 25-50% over limit
    Critical,        // >50% over limit
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentalImpactResult {
    pub temperature_impact: f64,             // Factor on reliability
    pub humidity_impact: f64,                // Factor on reliability
    pub vibration_impact: f64,               // Factor on reliability
    pub radiation_impact: f64,               // Factor on reliability
    pub altitude_impact: f64,                // Factor on reliability
    pub overall_environmental_factor: f64,   // Combined impact
    pub sensitive_components: Vec<InstanceId>,
    pub environmental_recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceIntervals {
    pub reliability_lower_bound: f64,
    pub reliability_upper_bound: f64,
    pub mtbf_lower_bound: f64,
    pub mtbf_upper_bound: f64,
    pub failure_rate_lower_bound: f64,
    pub failure_rate_upper_bound: f64,
}

impl ReliabilityAnalyzer {
    pub fn new() -> Self {
        Self {
            component_reliability_data: Self::default_component_profiles(),
            environmental_conditions: Self::default_environmental_conditions(),
            analysis_config: ReliabilityConfig::default(),
            failure_models: Self::default_failure_models(),
            lifecycle_database: Self::default_lifecycle_database(),
        }
    }

    pub fn with_config(config: ReliabilityConfig) -> Self {
        let mut analyzer = Self::new();
        analyzer.analysis_config = config;
        analyzer
    }

    pub async fn analyze_reliability(
        &mut self,
        netlist: &Netlist,
        analysis: &AnalysisResult
    ) -> Result<ReliabilityAnalysisResult> {
        info!("Starting reliability and lifecycle analysis...");
        
        let start_time = std::time::Instant::now();
        
        // Phase 1: Component-level reliability analysis
        let component_reliabilities = self.analyze_component_reliability(netlist, analysis).await?;
        
        // Phase 2: System-level reliability calculation
        let system_reliability = self.calculate_system_reliability(&component_reliabilities)?;
        
        // Phase 3: Critical component identification
        let critical_components = self.identify_critical_components(netlist, &component_reliabilities)?;
        
        // Phase 4: Failure prediction analysis
        let failure_predictions = self.predict_failures(&component_reliabilities)?;
        
        // Phase 5: Lifecycle risk assessment
        let lifecycle_risks = self.assess_lifecycle_risks(netlist)?;
        
        // Phase 6: Maintenance optimization
        let maintenance_recommendations = self.generate_maintenance_recommendations(
            &component_reliabilities,
            &critical_components
        )?;
        
        // Phase 7: Derating analysis
        let derating_analysis = self.analyze_derating_compliance(netlist, analysis)?;
        
        // Phase 8: Environmental impact assessment
        let environmental_impact = self.assess_environmental_impact(&component_reliabilities)?;
        
        // Phase 9: Statistical confidence analysis
        let confidence_intervals = self.calculate_confidence_intervals(&component_reliabilities)?;
        
        let analysis_time = start_time.elapsed();
        
        info!("Reliability analysis completed in {:.2}s", analysis_time.as_secs_f64());
        info!("  • Components analyzed: {}", component_reliabilities.len());
        info!("  • System reliability: {:.4} ({:.2}%)", system_reliability.overall_system_reliability, system_reliability.overall_system_reliability * 100.0);
        info!("  • Mean Time Between Failures: {:.0} hours ({:.1} years)", system_reliability.mean_time_between_failures, system_reliability.mean_time_between_failures / 8760.0);
        info!("  • Critical components identified: {}", critical_components.len());
        info!("  • Failure predictions: {}", failure_predictions.len());
        info!("  • Lifecycle risks: {}", lifecycle_risks.len());
        
        let result = ReliabilityAnalysisResult {
            overall_system_reliability: system_reliability.overall_system_reliability,
            mean_time_between_failures: system_reliability.mean_time_between_failures,
            failure_rate: system_reliability.failure_rate,
            component_reliabilities,
            critical_components,
            failure_predictions,
            lifecycle_risks,
            maintenance_recommendations,
            derating_analysis,
            environmental_impact,
            confidence_intervals,
        };
        
        Ok(result)
    }

    async fn analyze_component_reliability(
        &self,
        netlist: &Netlist,
        _analysis: &AnalysisResult
    ) -> Result<HashMap<InstanceId, ComponentReliabilityResult>> {
        let mut results = HashMap::new();
        
        for (instance_id, instance) in &netlist.instances {
            if let Some(profile) = self.get_component_profile(&instance.name) {
                let stress_factors = self.calculate_stress_factors(instance, &profile)?;
                let adjusted_failure_rate = self.calculate_adjusted_failure_rate(&profile, &stress_factors)?;
                let reliability = self.calculate_reliability(adjusted_failure_rate, self.analysis_config.analysis_period)?;
                
                let derating_compliance = self.check_derating_compliance(&stress_factors);
                
                let result = ComponentReliabilityResult {
                    component_id: instance_id,
                    component_name: instance.name.clone(),
                    component_type: self.extract_component_type(&instance.name),
                    reliability,
                    failure_rate: adjusted_failure_rate,
                    mtbf: 1.0 / adjusted_failure_rate,
                    dominant_failure_modes: self.identify_dominant_failure_modes(&profile),
                    stress_factors,
                    derating_compliance,
                    lifecycle_stage: self.get_lifecycle_stage(&instance.name),
                    obsolescence_risk: self.assess_obsolescence_risk(&instance.name),
                };
                
                results.insert(instance_id, result);
            }
        }
        
        Ok(results)
    }

    fn calculate_system_reliability(
        &self,
        component_reliabilities: &HashMap<InstanceId, ComponentReliabilityResult>
    ) -> Result<ReliabilityAnalysisResult> {
        // For series system (most common case), system reliability = product of component reliabilities
        let mut system_reliability = 1.0;
        let mut system_failure_rate = 0.0;
        
        for component in component_reliabilities.values() {
            system_reliability *= component.reliability;
            system_failure_rate += component.failure_rate;
        }
        
        let system_mtbf = if system_failure_rate > 0.0 { 1.0 / system_failure_rate } else { f64::INFINITY };
        
        Ok(ReliabilityAnalysisResult {
            overall_system_reliability: system_reliability,
            mean_time_between_failures: system_mtbf,
            failure_rate: system_failure_rate,
            component_reliabilities: HashMap::new(), // Will be filled by caller
            critical_components: Vec::new(),
            failure_predictions: Vec::new(),
            lifecycle_risks: Vec::new(),
            maintenance_recommendations: Vec::new(),
            derating_analysis: DeratingAnalysisResult {
                overall_derating_compliance: 0.0,
                voltage_derating_status: DeratingStatus {
                    compliance_percentage: 0.0,
                    average_derating_factor: 0.0,
                    worst_case_component: None,
                    recommended_improvements: Vec::new(),
                },
                current_derating_status: DeratingStatus {
                    compliance_percentage: 0.0,
                    average_derating_factor: 0.0,
                    worst_case_component: None,
                    recommended_improvements: Vec::new(),
                },
                thermal_derating_status: DeratingStatus {
                    compliance_percentage: 0.0,
                    average_derating_factor: 0.0,
                    worst_case_component: None,
                    recommended_improvements: Vec::new(),
                },
                power_derating_status: DeratingStatus {
                    compliance_percentage: 0.0,
                    average_derating_factor: 0.0,
                    worst_case_component: None,
                    recommended_improvements: Vec::new(),
                },
                non_compliant_components: Vec::new(),
            },
            environmental_impact: EnvironmentalImpactResult {
                temperature_impact: 1.0,
                humidity_impact: 1.0,
                vibration_impact: 1.0,
                radiation_impact: 1.0,
                altitude_impact: 1.0,
                overall_environmental_factor: 1.0,
                sensitive_components: Vec::new(),
                environmental_recommendations: Vec::new(),
            },
            confidence_intervals: ConfidenceIntervals {
                reliability_lower_bound: 0.0,
                reliability_upper_bound: 1.0,
                mtbf_lower_bound: 0.0,
                mtbf_upper_bound: f64::INFINITY,
                failure_rate_lower_bound: 0.0,
                failure_rate_upper_bound: f64::INFINITY,
            },
        })
    }

    fn identify_critical_components(
        &self,
        _netlist: &Netlist,
        component_reliabilities: &HashMap<InstanceId, ComponentReliabilityResult>
    ) -> Result<Vec<CriticalComponent>> {
        let mut critical_components = Vec::new();
        
        for component in component_reliabilities.values() {
            let criticality_score = self.calculate_criticality_score(component);
            
            if criticality_score > 0.7 { // High criticality threshold
                critical_components.push(CriticalComponent {
                    component_id: component.component_id,
                    component_name: component.component_name.clone(),
                    criticality_score,
                    failure_impact: self.assess_failure_impact(&component.component_type),
                    single_point_of_failure: self.is_single_point_of_failure(&component.component_type),
                    redundancy_available: false, // TODO: Implement redundancy detection
                    recommended_actions: self.generate_critical_component_actions(component),
                });
            }
        }
        
        // Sort by criticality score (highest first)
        critical_components.sort_by(|a, b| b.criticality_score.partial_cmp(&a.criticality_score).unwrap());
        
        Ok(critical_components)
    }

    fn predict_failures(
        &self,
        component_reliabilities: &HashMap<InstanceId, ComponentReliabilityResult>
    ) -> Result<Vec<FailurePrediction>> {
        let mut predictions = Vec::new();
        
        for component in component_reliabilities.values() {
            // Predict failure time based on current stress and degradation rate
            let predicted_time = self.calculate_predicted_failure_time(component);
            let confidence = self.calculate_prediction_confidence(component);
            
            if predicted_time < self.analysis_config.analysis_period * 2.0 { // Within 2x analysis period
                predictions.push(FailurePrediction {
                    component_id: component.component_id,
                    component_name: component.component_name.clone(),
                    predicted_failure_time: predicted_time,
                    prediction_confidence: confidence,
                    failure_mode: self.predict_most_likely_failure_mode(component),
                    early_warning_signs: self.identify_warning_signs(&component.component_type),
                    preventive_actions: self.recommend_preventive_actions(&component.component_type),
                });
            }
        }
        
        // Sort by predicted failure time (earliest first)
        predictions.sort_by(|a, b| a.predicted_failure_time.partial_cmp(&b.predicted_failure_time).unwrap());
        
        Ok(predictions)
    }

    fn assess_lifecycle_risks(&self, netlist: &Netlist) -> Result<Vec<LifecycleRisk>> {
        let mut risks = Vec::new();
        
        for (instance_id, instance) in &netlist.instances {
            let component_type = self.extract_component_type(&instance.name);
            
            // Check for obsolescence risk
            if let Some(obsolescence) = self.lifecycle_database.obsolescence_predictions.get(&component_type) {
                if matches!(obsolescence.risk_level, RiskLevel::High | RiskLevel::Critical) {
                    risks.push(LifecycleRisk {
                        component_id: instance_id,
                        component_name: instance.name.clone(),
                        risk_type: LifecycleRiskType::ComponentObsolescence,
                        risk_level: obsolescence.risk_level.clone(),
                        time_horizon: obsolescence.time_to_obsolescence.unwrap_or(5.0),
                        impact_description: "Component may become obsolete".to_string(),
                        mitigation_options: obsolescence.mitigation_strategies.clone(),
                    });
                }
            }
            
            // Check for supplier risks
            if let Some(supplier_risk) = self.lifecycle_database.supplier_stability.get(&component_type) {
                if matches!(supplier_risk.financial_stability, RiskLevel::High | RiskLevel::Critical) {
                    risks.push(LifecycleRisk {
                        component_id: instance_id,
                        component_name: instance.name.clone(),
                        risk_type: LifecycleRiskType::SupplierDiscontinuation,
                        risk_level: supplier_risk.financial_stability.clone(),
                        time_horizon: 2.0, // Assume 2 years for supplier issues
                        impact_description: "Supplier financial instability".to_string(),
                        mitigation_options: vec![
                            "Identify alternative suppliers".to_string(),
                            "Increase inventory buffer".to_string(),
                            "Design for alternative components".to_string(),
                        ],
                    });
                }
            }
        }
        
        Ok(risks)
    }

    fn generate_maintenance_recommendations(
        &self,
        component_reliabilities: &HashMap<InstanceId, ComponentReliabilityResult>,
        critical_components: &[CriticalComponent]
    ) -> Result<Vec<MaintenanceRecommendation>> {
        let mut recommendations = Vec::new();
        
        for component in component_reliabilities.values() {
            let is_critical = critical_components.iter().any(|c| c.component_id == component.component_id);
            
            // Generate maintenance recommendations based on component type and criticality
            let maintenance_interval = if is_critical {
                component.mtbf * 0.1 // More frequent maintenance for critical components
            } else {
                component.mtbf * 0.25 // Standard maintenance interval
            };
            
            let priority = if is_critical {
                MaintenancePriority::Critical
            } else if component.reliability < 0.9 {
                MaintenancePriority::High
            } else if component.reliability < 0.95 {
                MaintenancePriority::Medium
            } else {
                MaintenancePriority::Low
            };
            
            recommendations.push(MaintenanceRecommendation {
                component_id: component.component_id,
                component_name: component.component_name.clone(),
                maintenance_type: self.determine_maintenance_type(&component.component_type),
                recommended_interval: maintenance_interval,
                priority,
                estimated_cost: self.estimate_maintenance_cost(&component.component_type),
                description: format!("Recommended maintenance for {}", component.component_name),
                procedures: self.get_maintenance_procedures(&component.component_type),
            });
        }
        
        // Sort by priority and interval
        recommendations.sort_by(|a, b| {
            // First by priority (critical first), then by interval (shortest first)
            match (&a.priority, &b.priority) {
                (MaintenancePriority::Critical, MaintenancePriority::Critical) => 
                    a.recommended_interval.partial_cmp(&b.recommended_interval).unwrap(),
                (MaintenancePriority::Critical, _) => std::cmp::Ordering::Less,
                (_, MaintenancePriority::Critical) => std::cmp::Ordering::Greater,
                _ => a.recommended_interval.partial_cmp(&b.recommended_interval).unwrap(),
            }
        });
        
        Ok(recommendations)
    }

    fn analyze_derating_compliance(
        &self,
        _netlist: &Netlist,
        _analysis: &AnalysisResult
    ) -> Result<DeratingAnalysisResult> {
        // Simplified derating analysis
        Ok(DeratingAnalysisResult {
            overall_derating_compliance: 85.0, // Example: 85% compliance
            voltage_derating_status: DeratingStatus {
                compliance_percentage: 90.0,
                average_derating_factor: 0.82,
                worst_case_component: None,
                recommended_improvements: vec![
                    "Review voltage margins on power supply components".to_string(),
                ],
            },
            current_derating_status: DeratingStatus {
                compliance_percentage: 80.0,
                average_derating_factor: 0.78,
                worst_case_component: None,
                recommended_improvements: vec![
                    "Consider higher current rating components".to_string(),
                ],
            },
            thermal_derating_status: DeratingStatus {
                compliance_percentage: 95.0,
                average_derating_factor: 0.85,
                worst_case_component: None,
                recommended_improvements: vec![
                    "Thermal design appears adequate".to_string(),
                ],
            },
            power_derating_status: DeratingStatus {
                compliance_percentage: 87.0,
                average_derating_factor: 0.80,
                worst_case_component: None,
                recommended_improvements: vec![
                    "Review power dissipation calculations".to_string(),
                ],
            },
            non_compliant_components: Vec::new(),
        })
    }

    fn assess_environmental_impact(
        &self,
        _component_reliabilities: &HashMap<InstanceId, ComponentReliabilityResult>
    ) -> Result<EnvironmentalImpactResult> {
        // Calculate environmental impact factors
        let temp_factor = self.calculate_temperature_factor();
        let humidity_factor = self.calculate_humidity_factor();
        let vibration_factor = self.calculate_vibration_factor();
        let radiation_factor = self.calculate_radiation_factor();
        let altitude_factor = self.calculate_altitude_factor();
        
        let overall_factor = temp_factor * humidity_factor * vibration_factor * radiation_factor * altitude_factor;
        
        Ok(EnvironmentalImpactResult {
            temperature_impact: temp_factor,
            humidity_impact: humidity_factor,
            vibration_impact: vibration_factor,
            radiation_impact: radiation_factor,
            altitude_impact: altitude_factor,
            overall_environmental_factor: overall_factor,
            sensitive_components: Vec::new(), // TODO: Identify sensitive components
            environmental_recommendations: self.generate_environmental_recommendations(),
        })
    }

    fn calculate_confidence_intervals(
        &self,
        component_reliabilities: &HashMap<InstanceId, ComponentReliabilityResult>
    ) -> Result<ConfidenceIntervals> {
        // Simplified confidence interval calculation
        let reliabilities: Vec<f64> = component_reliabilities.values().map(|c| c.reliability).collect();
        let mtbfs: Vec<f64> = component_reliabilities.values().map(|c| c.mtbf).collect();
        let failure_rates: Vec<f64> = component_reliabilities.values().map(|c| c.failure_rate).collect();
        
        Ok(ConfidenceIntervals {
            reliability_lower_bound: reliabilities.iter().fold(f64::INFINITY, |a, &b| f64::min(a, b)) * 0.9,
            reliability_upper_bound: reliabilities.iter().fold(0.0f64, |a, &b| f64::max(a, b)) * 1.1,
            mtbf_lower_bound: mtbfs.iter().fold(f64::INFINITY, |a, &b| f64::min(a, b)) * 0.8,
            mtbf_upper_bound: mtbfs.iter().fold(0.0f64, |a, &b| f64::max(a, b)) * 1.2,
            failure_rate_lower_bound: failure_rates.iter().fold(f64::INFINITY, |a, &b| f64::min(a, b)) * 0.8,
            failure_rate_upper_bound: failure_rates.iter().fold(0.0f64, |a, &b| f64::max(a, b)) * 1.2,
        })
    }

    // Helper methods
    fn get_component_profile(&self, component_name: &str) -> Option<&ComponentReliabilityProfile> {
        let component_type = self.extract_component_type(component_name);
        self.component_reliability_data.get(&component_type)
    }

    fn extract_component_type(&self, component_name: &str) -> String {
        // Extract component type from instance name
        if component_name.to_lowercase().contains("microcontroller") || component_name.to_lowercase().contains("mcu") {
            "microcontroller".to_string()
        } else if component_name.to_lowercase().contains("regulator") {
            "regulator".to_string()
        } else if component_name.to_lowercase().contains("capacitor") {
            "capacitor".to_string()
        } else if component_name.to_lowercase().contains("resistor") {
            "resistor".to_string()
        } else if component_name.to_lowercase().contains("inductor") {
            "inductor".to_string()
        } else {
            "generic".to_string()
        }
    }

    fn calculate_stress_factors(&self, _instance: &Instance, profile: &ComponentReliabilityProfile) -> Result<StressFactors> {
        // Simplified stress calculation - in real implementation, this would use actual operating conditions
        Ok(StressFactors {
            thermal_stress: 0.7,  // 70% of thermal limit
            electrical_stress: 0.8,  // 80% of electrical limit
            mechanical_stress: 0.3,  // 30% of mechanical limit
            environmental_stress: match self.environmental_conditions.contamination_level {
                ContaminationLevel::CleanRoom => 0.1,
                ContaminationLevel::Industrial => 0.2,
                ContaminationLevel::Outdoor => 0.3,
                ContaminationLevel::Harsh => 0.5,
            },
            overall_stress: (0.7 + 0.8 + 0.3) / 3.0,
        })
    }

    fn calculate_adjusted_failure_rate(&self, profile: &ComponentReliabilityProfile, stress_factors: &StressFactors) -> Result<f64> {
        // Apply stress factors and environmental conditions to base failure rate
        let temp_factor = self.calculate_temperature_acceleration_factor(profile);
        let stress_multiplier = 1.0 + stress_factors.overall_stress;
        let quality_factor = profile.quality_factors.quality_multiplier;
        
        Ok(profile.base_failure_rate * temp_factor * stress_multiplier / quality_factor)
    }

    fn calculate_reliability(&self, failure_rate: f64, time_period: f64) -> Result<f64> {
        // Exponential reliability model: R(t) = e^(-λt)
        Ok((-failure_rate * time_period).exp())
    }

    fn identify_dominant_failure_modes(&self, profile: &ComponentReliabilityProfile) -> Vec<FailureMode> {
        profile.failure_modes.iter()
            .filter(|fm| fm.probability > 0.1) // >10% probability
            .map(|fm| fm.mode.clone())
            .collect()
    }

    fn check_derating_compliance(&self, stress_factors: &StressFactors) -> bool {
        stress_factors.electrical_stress <= self.analysis_config.derating_factors.voltage_derating &&
        stress_factors.thermal_stress <= self.analysis_config.derating_factors.temperature_derating / 100.0
    }

    fn get_lifecycle_stage(&self, component_name: &str) -> LifecycleStage {
        let component_type = self.extract_component_type(component_name);
        self.lifecycle_database.component_lifecycles
            .get(&component_type)
            .map(|lc| lc.lifecycle_stage.clone())
            .unwrap_or(LifecycleStage::Maturity)
    }

    fn assess_obsolescence_risk(&self, component_name: &str) -> RiskLevel {
        let component_type = self.extract_component_type(component_name);
        self.lifecycle_database.obsolescence_predictions
            .get(&component_type)
            .map(|op| op.risk_level.clone())
            .unwrap_or(RiskLevel::Low)
    }

    fn calculate_criticality_score(&self, component: &ComponentReliabilityResult) -> f64 {
        // Criticality based on failure rate, impact, and stress
        let failure_criticality = component.failure_rate * 100000.0; // Scale up
        let stress_criticality = component.stress_factors.overall_stress;
        let impact_criticality = match component.component_type.as_str() {
            "microcontroller" => 1.0,
            "regulator" => 0.9,
            "capacitor" => 0.5,
            "resistor" => 0.3,
            _ => 0.4,
        };
        
        (failure_criticality + stress_criticality + impact_criticality) / 3.0
    }

    fn assess_failure_impact(&self, component_type: &str) -> ImpactSeverity {
        match component_type {
            "microcontroller" => ImpactSeverity::Catastrophic,
            "regulator" => ImpactSeverity::Critical,
            "capacitor" => ImpactSeverity::Marginal,
            "resistor" => ImpactSeverity::Negligible,
            _ => ImpactSeverity::Marginal,
        }
    }

    fn is_single_point_of_failure(&self, component_type: &str) -> bool {
        matches!(component_type, "microcontroller" | "regulator")
    }

    fn generate_critical_component_actions(&self, component: &ComponentReliabilityResult) -> Vec<String> {
        vec![
            format!("Monitor {} closely for degradation signs", component.component_name),
            "Consider implementing redundancy".to_string(),
            "Increase inspection frequency".to_string(),
            "Evaluate alternative components".to_string(),
        ]
    }

    fn calculate_predicted_failure_time(&self, component: &ComponentReliabilityResult) -> f64 {
        // Simplified prediction based on current failure rate and stress
        let base_time = component.mtbf;
        let stress_acceleration = 1.0 + component.stress_factors.overall_stress;
        base_time / stress_acceleration
    }

    fn calculate_prediction_confidence(&self, component: &ComponentReliabilityResult) -> f64 {
        // Confidence based on data quality and component maturity
        let data_quality = if component.stress_factors.overall_stress < 0.5 { 0.9 } else { 0.7 };
        let maturity_factor = match component.lifecycle_stage {
            LifecycleStage::Maturity => 0.95,
            LifecycleStage::Growth => 0.85,
            LifecycleStage::Introduction => 0.75,
            _ => 0.6,
        };
        data_quality * maturity_factor
    }

    fn predict_most_likely_failure_mode(&self, component: &ComponentReliabilityResult) -> FailureMode {
        // Return most likely failure mode based on component type and stress
        match component.component_type.as_str() {
            "microcontroller" => {
                if component.stress_factors.thermal_stress > 0.8 {
                    FailureMode::ThermalRunaway
                } else {
                    FailureMode::ElectromigrationFailure
                }
            },
            "capacitor" => FailureMode::CapacitorDegradation,
            "resistor" => FailureMode::ResistorDrift,
            _ => FailureMode::ParametricDrift,
        }
    }

    fn identify_warning_signs(&self, component_type: &str) -> Vec<String> {
        match component_type {
            "microcontroller" => vec![
                "Increased power consumption".to_string(),
                "Timing violations".to_string(),
                "Temperature rise".to_string(),
            ],
            "capacitor" => vec![
                "Capacitance drift".to_string(),
                "Increased ESR".to_string(),
                "Physical deformation".to_string(),
            ],
            "resistor" => vec![
                "Resistance drift".to_string(),
                "Temperature coefficient change".to_string(),
            ],
            _ => vec!["Performance degradation".to_string()],
        }
    }

    fn recommend_preventive_actions(&self, component_type: &str) -> Vec<String> {
        match component_type {
            "microcontroller" => vec![
                "Monitor junction temperature".to_string(),
                "Verify supply voltage stability".to_string(),
                "Check for proper decoupling".to_string(),
            ],
            "capacitor" => vec![
                "Monitor capacitance and ESR".to_string(),
                "Check for physical damage".to_string(),
                "Verify operating temperature range".to_string(),
            ],
            "resistor" => vec![
                "Monitor resistance value".to_string(),
                "Check for thermal stress".to_string(),
            ],
            _ => vec!["Regular functional testing".to_string()],
        }
    }

    fn determine_maintenance_type(&self, component_type: &str) -> MaintenanceType {
        match component_type {
            "microcontroller" => MaintenanceType::PredictiveMaintenance,
            "regulator" => MaintenanceType::PreventiveMaintenance,
            "capacitor" => MaintenanceType::ConditionBasedMaintenance,
            _ => MaintenanceType::Inspection,
        }
    }

    fn estimate_maintenance_cost(&self, component_type: &str) -> f64 {
        match component_type {
            "microcontroller" => 50.0,
            "regulator" => 25.0,
            "capacitor" => 5.0,
            "resistor" => 2.0,
            _ => 10.0,
        }
    }

    fn get_maintenance_procedures(&self, component_type: &str) -> Vec<String> {
        match component_type {
            "microcontroller" => vec![
                "Perform functional test".to_string(),
                "Check operating temperature".to_string(),
                "Verify supply currents".to_string(),
                "Test communication interfaces".to_string(),
            ],
            "capacitor" => vec![
                "Measure capacitance".to_string(),
                "Check ESR".to_string(),
                "Visual inspection".to_string(),
            ],
            _ => vec!["Visual inspection".to_string(), "Basic electrical test".to_string()],
        }
    }

    fn calculate_temperature_factor(&self) -> f64 {
        // Arrhenius acceleration factor for temperature
        let operating_temp = (self.environmental_conditions.operating_temperature_range.0 + 
                             self.environmental_conditions.operating_temperature_range.1) / 2.0;
        let reference_temp = 25.0; // °C
        let activation_energy = 0.7; // eV (typical for semiconductors)
        
        let k = 8.617e-5; // Boltzmann constant (eV/K)
        let factor = activation_energy / k * (1.0/(operating_temp + 273.15) - 1.0/(reference_temp + 273.15));
        factor.exp()
    }

    fn calculate_humidity_factor(&self) -> f64 {
        let avg_humidity = (self.environmental_conditions.humidity_range.0 + 
                           self.environmental_conditions.humidity_range.1) / 2.0;
        // Humidity acceleration factor (simplified)
        1.0 + (avg_humidity - 50.0) / 100.0 * 0.2
    }

    fn calculate_vibration_factor(&self) -> f64 {
        let vibration_g = self.environmental_conditions.vibration_profile.acceleration;
        // Vibration acceleration factor
        1.0 + vibration_g / 10.0 * 0.1
    }

    fn calculate_radiation_factor(&self) -> f64 {
        let total_dose = self.environmental_conditions.radiation_environment.total_ionizing_dose;
        // Radiation acceleration factor
        1.0 + total_dose / 1000.0 * 0.05
    }

    fn calculate_altitude_factor(&self) -> f64 {
        let altitude_km = self.environmental_conditions.altitude / 1000.0;
        // Altitude derating factor (lower air pressure)
        1.0 + altitude_km / 10.0 * 0.02
    }

    fn calculate_temperature_acceleration_factor(&self, profile: &ComponentReliabilityProfile) -> f64 {
        // Use component-specific activation energy if available
        let operating_temp = (self.environmental_conditions.operating_temperature_range.0 + 
                             self.environmental_conditions.operating_temperature_range.1) / 2.0;
        let reference_temp = 25.0; // °C
        
        let k = 8.617e-5; // Boltzmann constant (eV/K)
        let factor = profile.activation_energy / k * (1.0/(operating_temp + 273.15) - 1.0/(reference_temp + 273.15));
        factor.exp()
    }

    fn generate_environmental_recommendations(&self) -> Vec<String> {
        let mut recommendations = Vec::new();
        
        let avg_temp = (self.environmental_conditions.operating_temperature_range.0 + 
                       self.environmental_conditions.operating_temperature_range.1) / 2.0;
        
        if avg_temp > 70.0 {
            recommendations.push("Consider active cooling to reduce operating temperature".to_string());
        }
        
        let avg_humidity = (self.environmental_conditions.humidity_range.0 + 
                           self.environmental_conditions.humidity_range.1) / 2.0;
        
        if avg_humidity > 80.0 {
            recommendations.push("Implement humidity control or conformal coating".to_string());
        }
        
        if self.environmental_conditions.vibration_profile.acceleration > 5.0 {
            recommendations.push("Add vibration damping or isolation".to_string());
        }
        
        recommendations
    }

    // Default data initialization methods
    fn default_component_profiles() -> HashMap<String, ComponentReliabilityProfile> {
        let mut profiles = HashMap::new();
        
        // Microcontroller profile
        profiles.insert("microcontroller".to_string(), ComponentReliabilityProfile {
            component_type: "microcontroller".to_string(),
            base_failure_rate: 0.00001,  // 10 FIT (failures per billion hours)
            mtbf: 100000.0,              // 100,000 hours
            mttf: 100000.0,
            activation_energy: 0.7,      // eV
            temperature_coefficient: 0.02, // /°C
            voltage_stress_factor: 2.0,
            current_stress_factor: 1.5,
            failure_modes: vec![
                ComponentFailureMode {
                    mode: FailureMode::ElectromigrationFailure,
                    probability: 0.3,
                    detection_method: DetectionMethod::FunctionalTest,
                    impact_severity: ImpactSeverity::Catastrophic,
                    mitigation_strategies: vec!["Current derating".to_string(), "Temperature control".to_string()],
                },
                ComponentFailureMode {
                    mode: FailureMode::ThermalRunaway,
                    probability: 0.2,
                    detection_method: DetectionMethod::ThermalImaging,
                    impact_severity: ImpactSeverity::Catastrophic,
                    mitigation_strategies: vec!["Thermal management".to_string(), "Power limiting".to_string()],
                },
            ],
            wear_out_period: 150000.0,   // 150,000 hours
            infant_mortality_period: 1000.0, // 1,000 hours
            quality_factors: QualityFactors {
                manufacturer_grade: ManufacturerGrade::IndustrialGrade,
                screening_level: ScreeningLevel::StandardCommercial,
                package_type: PackageType::Plastic,
                technology_maturity: TechnologyMaturity::Mature,
                quality_multiplier: 1.0,
            },
        });
        
        // Capacitor profile
        profiles.insert("capacitor".to_string(), ComponentReliabilityProfile {
            component_type: "capacitor".to_string(),
            base_failure_rate: 0.000005,  // 5 FIT
            mtbf: 200000.0,               // 200,000 hours
            mttf: 200000.0,
            activation_energy: 0.5,       // eV
            temperature_coefficient: 0.03, // /°C
            voltage_stress_factor: 3.0,
            current_stress_factor: 1.2,
            failure_modes: vec![
                ComponentFailureMode {
                    mode: FailureMode::CapacitorDegradation,
                    probability: 0.6,
                    detection_method: DetectionMethod::ElectricalTest,
                    impact_severity: ImpactSeverity::Critical,
                    mitigation_strategies: vec!["Voltage derating".to_string(), "Temperature control".to_string()],
                },
                ComponentFailureMode {
                    mode: FailureMode::DielectricBreakdown,
                    probability: 0.3,
                    detection_method: DetectionMethod::ElectricalTest,
                    impact_severity: ImpactSeverity::Critical,
                    mitigation_strategies: vec!["Voltage derating".to_string()],
                },
            ],
            wear_out_period: 100000.0,    // 100,000 hours
            infant_mortality_period: 500.0, // 500 hours
            quality_factors: QualityFactors {
                manufacturer_grade: ManufacturerGrade::IndustrialGrade,
                screening_level: ScreeningLevel::StandardCommercial,
                package_type: PackageType::Plastic,
                technology_maturity: TechnologyMaturity::Proven,
                quality_multiplier: 1.2,
            },
        });
        
        // Resistor profile
        profiles.insert("resistor".to_string(), ComponentReliabilityProfile {
            component_type: "resistor".to_string(),
            base_failure_rate: 0.000001,  // 1 FIT
            mtbf: 1000000.0,              // 1,000,000 hours
            mttf: 1000000.0,
            activation_energy: 0.3,       // eV
            temperature_coefficient: 0.01, // /°C
            voltage_stress_factor: 2.0,
            current_stress_factor: 2.0,
            failure_modes: vec![
                ComponentFailureMode {
                    mode: FailureMode::ResistorDrift,
                    probability: 0.7,
                    detection_method: DetectionMethod::ElectricalTest,
                    impact_severity: ImpactSeverity::Marginal,
                    mitigation_strategies: vec!["Power derating".to_string(), "Precision components".to_string()],
                },
                ComponentFailureMode {
                    mode: FailureMode::OpenCircuit,
                    probability: 0.2,
                    detection_method: DetectionMethod::ElectricalTest,
                    impact_severity: ImpactSeverity::Critical,
                    mitigation_strategies: vec!["Quality screening".to_string()],
                },
            ],
            wear_out_period: 500000.0,    // 500,000 hours
            infant_mortality_period: 100.0, // 100 hours
            quality_factors: QualityFactors {
                manufacturer_grade: ManufacturerGrade::IndustrialGrade,
                screening_level: ScreeningLevel::StandardCommercial,
                package_type: PackageType::Plastic,
                technology_maturity: TechnologyMaturity::Proven,
                quality_multiplier: 1.5,
            },
        });
        
        profiles
    }

    fn default_environmental_conditions() -> EnvironmentalConditions {
        EnvironmentalConditions {
            operating_temperature_range: (-20.0, 85.0),  // Industrial range
            storage_temperature_range: (-40.0, 125.0),
            humidity_range: (10.0, 90.0),                // %RH
            vibration_profile: VibrationProfile {
                frequency_range: (10.0, 2000.0),         // Hz
                acceleration: 2.0,                        // g's
                duration: 1000.0,                         // hours
                vibration_type: VibrationType::RandomVibration,
            },
            shock_profile: ShockProfile {
                peak_acceleration: 30.0,                  // g's
                pulse_duration: 11.0,                     // milliseconds
                pulse_shape: PulseShape::HalfSine,
            },
            altitude: 2000.0,                             // 2000 meters
            contamination_level: ContaminationLevel::Industrial,
            radiation_environment: RadiationEnvironment {
                total_ionizing_dose: 1000.0,             // rad
                neutron_fluence: 1e10,                   // neutrons/cm²
                single_event_rate: 0.001,                // events/day
                radiation_type: RadiationType::TerrestrialBackground,
            },
        }
    }

    fn default_failure_models() -> HashMap<FailureMode, FailureModel> {
        let mut models = HashMap::new();
        
        models.insert(FailureMode::ElectromigrationFailure, FailureModel {
            model_type: ModelType::Arrhenius,
            parameters: [
                ("activation_energy".to_string(), 0.7),
                ("current_exponent".to_string(), 2.0),
            ].into_iter().collect(),
            applicable_components: vec!["microcontroller".to_string(), "ic".to_string()],
            environmental_factors: vec!["temperature".to_string(), "current_density".to_string()],
        });
        
        models.insert(FailureMode::CapacitorDegradation, FailureModel {
            model_type: ModelType::Eyring,
            parameters: [
                ("voltage_acceleration".to_string(), 3.0),
                ("temperature_acceleration".to_string(), 0.5),
            ].into_iter().collect(),
            applicable_components: vec!["capacitor".to_string()],
            environmental_factors: vec!["temperature".to_string(), "voltage".to_string(), "humidity".to_string()],
        });
        
        models
    }

    fn default_lifecycle_database() -> LifecycleDatabase {
        let mut component_lifecycles = HashMap::new();
        let mut obsolescence_predictions = HashMap::new();
        let mut supplier_stability = HashMap::new();
        let mut technology_roadmaps = HashMap::new();
        
        // Example microcontroller lifecycle
        component_lifecycles.insert("microcontroller".to_string(), ComponentLifecycle {
            introduction_date: "2020-01-01".to_string(),
            peak_production_period: ("2021-01-01".to_string(), "2025-12-31".to_string()),
            decline_phase_start: "2026-01-01".to_string(),
            obsolescence_date: Some("2030-12-31".to_string()),
            replacement_components: vec!["next_gen_mcu".to_string()],
            lifecycle_stage: LifecycleStage::Maturity,
        });
        
        obsolescence_predictions.insert("microcontroller".to_string(), ObsolescenceRisk {
            risk_level: RiskLevel::Low,
            time_to_obsolescence: Some(8.0), // 8 years
            contributing_factors: vec![ObsolescenceDriver::TechnologyEvolution],
            mitigation_strategies: vec![
                "Monitor manufacturer roadmaps".to_string(),
                "Identify pin-compatible alternatives".to_string(),
            ],
        });
        
        supplier_stability.insert("microcontroller".to_string(), SupplierRisk {
            financial_stability: RiskLevel::Low,
            market_share: 25.0, // 25%
            alternative_suppliers: 3,
            geographic_risk: RiskLevel::Medium,
            quality_history: QualityHistory {
                defect_rate: 50.0, // 50 PPM
                recall_history: 0,
                certification_status: vec!["ISO9001".to_string(), "TS16949".to_string()],
                audit_results: AuditResults {
                    last_audit_date: "2024-06-01".to_string(),
                    audit_score: 95.0,
                    major_findings: 0,
                    minor_findings: 2,
                },
            },
        });
        
        technology_roadmaps.insert("microcontroller".to_string(), TechnologyRoadmap {
            current_generation: "ARM Cortex-M4".to_string(),
            next_generation: Some("ARM Cortex-M55".to_string()),
            transition_timeline: Some("2025-2027".to_string()),
            performance_improvements: vec![
                "AI/ML acceleration".to_string(),
                "Lower power consumption".to_string(),
                "Enhanced security".to_string(),
            ],
            backward_compatibility: true,
        });
        
        LifecycleDatabase {
            component_lifecycles,
            obsolescence_predictions,
            supplier_stability,
            technology_roadmaps,
        }
    }
}

impl Default for ReliabilityConfig {
    fn default() -> Self {
        Self {
            analysis_period: 87600.0,        // 10 years in hours
            confidence_level: 0.95,          // 95% confidence
            enable_accelerated_testing: true,
            enable_physics_of_failure: true,
            enable_bayesian_analysis: false,
            enable_prognostics: true,
            temperature_cycling_enabled: true,
            burn_in_hours: 168.0,            // 1 week burn-in
            derating_factors: DeratingFactors {
                voltage_derating: 0.8,       // 80% of maximum
                current_derating: 0.75,      // 75% of maximum
                power_derating: 0.8,         // 80% of maximum
                temperature_derating: 10.0,  // 10°C below maximum
                frequency_derating: 0.8,     // 80% of maximum
            },
        }
    }
}