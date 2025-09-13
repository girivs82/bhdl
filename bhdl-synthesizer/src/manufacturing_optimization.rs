// Manufacturing and Assembly Optimization (DFM/DFA)
// Analyzes designs for manufacturability and assembly considerations

use bhdl_netlist::{Netlist, Instance, InstanceId, Net, NetId};
use bhdl_analyzer::AnalysisResult;
use std::collections::{HashMap, HashSet};
use serde::{Serialize, Deserialize};
use anyhow::{Result, Context};
use log::{info, warn, debug, error};

/// Main analyzer for manufacturing and assembly optimization
#[derive(Debug, Clone)]
pub struct ManufacturingOptimizer {
    config: ManufacturingConfig,
    process_capabilities: ProcessCapabilities,
    assembly_constraints: AssemblyConstraints,
    component_library: ComponentManufacturingData,
    cost_model: ManufacturingCostModel,
}

/// Configuration for manufacturing optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManufacturingConfig {
    pub target_process: ManufacturingProcess,
    pub assembly_method: AssemblyMethod,
    pub target_volume: ProductionVolume,
    pub quality_level: QualityLevel,
    pub enable_panelization: bool,
    pub enable_testpoint_generation: bool,
    pub enable_component_consolidation: bool,
    pub enable_placement_optimization: bool,
    pub enable_routing_optimization: bool,
    pub target_yield: f64,
    pub max_board_layers: usize,
    pub preferred_component_packages: Vec<String>,
    pub avoid_component_packages: Vec<String>,
}

/// Manufacturing process type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ManufacturingProcess {
    Prototype,      // Quick-turn prototype
    SmallBatch,     // Small volume production
    MassProduction, // High volume manufacturing
    Automotive,     // Automotive grade requirements
    Medical,        // Medical device requirements
    Aerospace,      // Aerospace requirements
}

/// Assembly method
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AssemblyMethod {
    HandAssembly,
    SemiAutomatic,
    FullySMT,
    MixedTechnology,
    SelectiveSoldering,
}

/// Production volume categories
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProductionVolume {
    Prototype,      // < 10 units
    LowVolume,      // 10-100 units
    MediumVolume,   // 100-1000 units
    HighVolume,     // 1000-10000 units
    MassProduction, // > 10000 units
}

/// Quality level requirements
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum QualityLevel {
    Standard,
    HighReliability,
    Automotive,
    Medical,
    MilSpec,
}

/// Process capabilities and constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessCapabilities {
    min_trace_width: f64,      // mm
    min_trace_spacing: f64,     // mm
    min_via_size: f64,          // mm
    min_via_spacing: f64,       // mm
    min_hole_size: f64,         // mm
    min_annular_ring: f64,      // mm
    min_solder_mask_web: f64,   // mm
    min_silkscreen_width: f64,  // mm
    min_courtyard_spacing: f64, // mm
    max_aspect_ratio: f64,      // board thickness / hole diameter
    placement_accuracy: f64,    // mm
    supported_packages: HashSet<String>,
    special_processes: Vec<SpecialProcess>,
}

/// Special manufacturing processes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SpecialProcess {
    BlindVias,
    BuriedVias,
    Microvias,
    HDI,
    FlexRigid,
    MetalCore,
    ControlledImpedance,
    ViainPad,
}

/// Assembly constraints and capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssemblyConstraints {
    min_component_spacing: f64,     // mm
    min_component_size: (f64, f64), // mm (length, width)
    max_component_height: f64,      // mm
    min_pad_size: f64,              // mm
    fiducial_requirements: FiducialRequirements,
    component_orientation_rules: ComponentOrientationRules,
    wave_solder_constraints: Option<WaveSolderConstraints>,
    reflow_profile: ReflowProfile,
}

/// Fiducial marker requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiducialRequirements {
    global_fiducials: usize,
    local_fiducials_fine_pitch: bool,
    fiducial_size: f64,        // mm
    fiducial_clearance: f64,   // mm
}

/// Component orientation rules for assembly
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentOrientationRules {
    standardize_orientation: bool,
    preferred_angles: Vec<f64>, // degrees
    avoid_angles: Vec<f64>,     // degrees
}

/// Wave soldering specific constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveSolderConstraints {
    shadow_zone: f64,           // mm
    component_orientation: f64,  // degrees relative to wave direction
    min_component_spacing: f64,  // mm
}

/// Reflow soldering profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflowProfile {
    peak_temperature: f64,      // Celsius
    time_above_liquidus: f64,   // seconds
    max_heating_rate: f64,      // C/sec
    max_cooling_rate: f64,      // C/sec
}

/// Component manufacturing data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentManufacturingData {
    components: HashMap<String, ComponentManufacturingInfo>,
}

/// Manufacturing information for a component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentManufacturingInfo {
    package_type: String,
    package_dimensions: (f64, f64, f64), // mm (length, width, height)
    pad_pitch: Option<f64>,              // mm
    thermal_considerations: ThermalConsiderations,
    assembly_difficulty: AssemblyDifficulty,
    moisture_sensitivity_level: Option<MSL>,
    placement_requirements: PlacementRequirements,
    soldering_requirements: SolderingRequirements,
    availability: ComponentAvailability,
}

/// Thermal considerations for component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalConsiderations {
    requires_thermal_pad: bool,
    requires_heatsink: bool,
    max_temperature: f64,    // Celsius
    thermal_resistance: f64, // C/W
}

/// Assembly difficulty rating
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AssemblyDifficulty {
    Simple,      // Standard SMT
    Moderate,    // Fine pitch or small components
    Difficult,   // BGA, QFN, very fine pitch
    VeryDifficult, // Specialized handling required
}

/// Moisture sensitivity level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MSL {
    MSL1, // Unlimited floor life
    MSL2, // 1 year floor life
    MSL2a, // 4 weeks floor life
    MSL3, // 168 hours floor life
    MSL4, // 72 hours floor life
    MSL5, // 48 hours floor life
    MSL5a, // 24 hours floor life
    MSL6, // Time on label
}

/// Placement requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementRequirements {
    requires_manual_placement: bool,
    requires_vision_system: bool,
    placement_tolerance: f64, // mm
    rotation_tolerance: f64,  // degrees
}

/// Soldering requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolderingRequirements {
    solder_paste_type: String,
    stencil_thickness: f64,     // mm
    aperture_reduction: f64,    // percentage
    requires_selective: bool,
}

/// Component availability information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentAvailability {
    stock_status: StockStatus,
    lead_time_weeks: u32,
    minimum_order_quantity: u32,
    lifecycle_status: LifecycleStatus,
    alternative_parts: Vec<String>,
}

/// Stock status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StockStatus {
    InStock,
    LowStock,
    OutOfStock,
    Obsolete,
}

/// Component lifecycle status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LifecycleStatus {
    Active,
    NotRecommendedForNewDesigns,
    EndOfLife,
    Obsolete,
}

/// Manufacturing cost model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManufacturingCostModel {
    setup_costs: SetupCosts,
    material_costs: MaterialCosts,
    assembly_costs: AssemblyCosts,
    testing_costs: TestingCosts,
    yield_model: YieldModel,
}

/// Setup costs for manufacturing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupCosts {
    nre_cost: f64,          // Non-recurring engineering
    tooling_cost: f64,      // Stencils, fixtures, etc.
    programming_cost: f64,  // Pick-and-place programming
}

/// Material costs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialCosts {
    pcb_cost_per_sqcm: f64,
    layer_adder: f64,       // Cost per additional layer
    special_process_costs: HashMap<SpecialProcess, f64>,
}

/// Assembly costs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssemblyCosts {
    placement_cost_per_component: f64,
    solder_joint_cost: f64,
    manual_assembly_hourly_rate: f64,
    machine_time_hourly_rate: f64,
}

/// Testing costs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestingCosts {
    ict_test_cost: f64,       // In-circuit test
    functional_test_cost: f64,
    boundary_scan_cost: f64,
    aoi_cost: f64,            // Automated optical inspection
}

/// Yield model for production
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldModel {
    base_yield: f64,
    complexity_factor: f64,
    component_yield_impact: HashMap<AssemblyDifficulty, f64>,
}

/// Results of manufacturing optimization analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManufacturingAnalysisResult {
    pub dfm_score: f64,
    pub dfa_score: f64,
    pub estimated_yield: f64,
    pub estimated_cost: CostBreakdown,
    pub violations: Vec<ManufacturingViolation>,
    pub warnings: Vec<ManufacturingWarning>,
    pub suggestions: Vec<OptimizationSuggestion>,
    pub panelization: Option<PanelizationResult>,
    pub test_coverage: TestCoverageAnalysis,
    pub assembly_sequence: Vec<AssemblyStep>,
    pub critical_components: Vec<CriticalComponent>,
}

/// Cost breakdown for manufacturing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostBreakdown {
    pub material_cost: f64,
    pub assembly_cost: f64,
    pub testing_cost: f64,
    pub setup_cost_amortized: f64,
    pub total_unit_cost: f64,
    pub volume_discounts: Vec<VolumeDiscount>,
}

/// Volume discount information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeDiscount {
    pub quantity: u32,
    pub unit_cost: f64,
    pub total_cost: f64,
}

/// Manufacturing rule violation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManufacturingViolation {
    pub violation_type: ViolationType,
    pub severity: ViolationSeverity,
    pub location: String,
    pub description: String,
    pub impact: String,
    pub resolution: String,
}

/// Types of manufacturing violations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ViolationType {
    TraceWidth,
    TraceSpacing,
    ViaSize,
    HoleSize,
    ComponentSpacing,
    ComponentOrientation,
    ThermalRelief,
    SolderMask,
    Silkscreen,
    Courtyard,
    FiducialMissing,
    TestPointAccess,
}

/// Violation severity levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViolationSeverity {
    Critical,  // Will cause manufacturing failure
    Major,     // Will impact yield significantly
    Minor,     // May impact yield slightly
    Info,      // Informational only
}

/// Manufacturing warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManufacturingWarning {
    pub warning_type: String,
    pub component: String,
    pub description: String,
    pub recommendation: String,
}

/// Optimization suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationSuggestion {
    pub suggestion_type: SuggestionType,
    pub description: String,
    pub impact: OptimizationImpact,
    pub implementation_effort: ImplementationEffort,
}

/// Types of optimization suggestions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SuggestionType {
    ComponentConsolidation,
    PackageStandardization,
    OrientationAlignment,
    PanelizationOptimization,
    TestPointAddition,
    AssemblySequence,
    ThermalManagement,
    ComponentPlacement,
}

/// Impact of optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationImpact {
    pub yield_improvement: f64,
    pub cost_reduction: f64,
    pub assembly_time_reduction: f64,
    pub quality_improvement: f64,
}

/// Implementation effort level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImplementationEffort {
    Trivial,
    Low,
    Medium,
    High,
}

/// Panelization analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelizationResult {
    pub panel_size: (f64, f64),  // mm
    pub boards_per_panel: usize,
    pub utilization: f64,         // percentage
    pub panel_layout: PanelLayout,
    pub breakaway_method: BreakawayMethod,
}

/// Panel layout configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelLayout {
    pub rows: usize,
    pub columns: usize,
    pub spacing: f64,            // mm
    pub rail_width: f64,         // mm
}

/// Method for separating boards from panel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BreakawayMethod {
    VScoring,
    TabRouting,
    MouseBites,
    Combination,
}

/// Test coverage analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCoverageAnalysis {
    pub in_circuit_test_coverage: f64,
    pub boundary_scan_coverage: f64,
    pub functional_test_coverage: f64,
    pub optical_inspection_coverage: f64,
    pub test_points_required: usize,
    pub test_points_available: usize,
    pub untestable_nets: Vec<String>,
}

/// Assembly step in production
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssemblyStep {
    pub step_number: usize,
    pub process: AssemblyProcess,
    pub components: Vec<String>,
    pub time_estimate: f64,       // minutes
    pub equipment_required: String,
}

/// Assembly process type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AssemblyProcess {
    SMTPastePrinting,
    SMTPlacement,
    ReflowSoldering,
    THComponentInsertion,
    WaveSoldering,
    SelectiveSoldering,
    ManualAssembly,
    Inspection,
    Testing,
}

/// Critical component for manufacturing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriticalComponent {
    pub component_name: String,
    pub criticality_reason: CriticalityReason,
    pub special_handling: Vec<String>,
    pub risk_mitigation: Vec<String>,
}

/// Reason for component criticality
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CriticalityReason {
    HighValue,
    LongLeadTime,
    SingleSource,
    MoistureSensitive,
    FragilePackage,
    TightTolerance,
    SpecialHandling,
}

impl ManufacturingOptimizer {
    /// Create a new manufacturing optimizer with default configuration
    pub fn new() -> Self {
        Self {
            config: ManufacturingConfig::default(),
            process_capabilities: ProcessCapabilities::default(),
            assembly_constraints: AssemblyConstraints::default(),
            component_library: ComponentManufacturingData::default(),
            cost_model: ManufacturingCostModel::default(),
        }
    }

    /// Create optimizer with specific configuration
    pub fn with_config(config: ManufacturingConfig) -> Self {
        let mut optimizer = Self::new();
        optimizer.config = config;
        optimizer.update_capabilities_for_process();
        optimizer
    }

    /// Update process capabilities based on target process
    fn update_capabilities_for_process(&mut self) {
        match self.config.target_process {
            ManufacturingProcess::Prototype => {
                self.process_capabilities.min_trace_width = 0.2;
                self.process_capabilities.min_via_size = 0.3;
                self.process_capabilities.placement_accuracy = 0.1;
            }
            ManufacturingProcess::MassProduction => {
                self.process_capabilities.min_trace_width = 0.15;
                self.process_capabilities.min_via_size = 0.25;
                self.process_capabilities.placement_accuracy = 0.05;
            }
            ManufacturingProcess::Automotive | ManufacturingProcess::Medical => {
                self.process_capabilities.min_trace_width = 0.25;
                self.process_capabilities.min_via_size = 0.35;
                self.process_capabilities.placement_accuracy = 0.05;
                // More conservative for reliability
            }
            _ => {}
        }
    }

    /// Analyze manufacturing and assembly optimization
    pub async fn analyze_manufacturing(
        &mut self,
        netlist: &Netlist,
        analysis: &AnalysisResult,
    ) -> Result<ManufacturingAnalysisResult> {
        info!("Starting manufacturing and assembly optimization analysis...");

        // Phase 1: Design rule checking for manufacturing
        let violations = self.check_manufacturing_rules(netlist)?;
        info!("Found {} manufacturing violations", violations.len());

        // Phase 2: Assembly feasibility analysis
        let warnings = self.analyze_assembly_feasibility(netlist)?;
        info!("Generated {} assembly warnings", warnings.len());

        // Phase 3: Component consolidation opportunities
        let consolidation_suggestions = self.analyze_component_consolidation(netlist)?;
        
        // Phase 4: Panelization optimization
        let panelization = if self.config.enable_panelization {
            Some(self.optimize_panelization(netlist)?)
        } else {
            None
        };

        // Phase 5: Test coverage analysis
        let test_coverage = self.analyze_test_coverage(netlist)?;

        // Phase 6: Assembly sequence optimization
        let assembly_sequence = self.optimize_assembly_sequence(netlist)?;

        // Phase 7: Critical component identification
        let critical_components = self.identify_critical_components(netlist)?;

        // Phase 8: Yield estimation
        let estimated_yield = self.estimate_production_yield(netlist, &violations)?;

        // Phase 9: Cost estimation
        let estimated_cost = self.estimate_manufacturing_cost(
            netlist,
            &assembly_sequence,
            estimated_yield,
        )?;

        // Phase 10: Generate optimization suggestions
        let mut suggestions = consolidation_suggestions;
        suggestions.extend(self.generate_dfm_suggestions(netlist, &violations)?);
        suggestions.extend(self.generate_dfa_suggestions(netlist, &warnings)?);

        // Calculate overall scores
        let dfm_score = self.calculate_dfm_score(&violations);
        let dfa_score = self.calculate_dfa_score(&warnings, &assembly_sequence);

        info!("Manufacturing analysis complete:");
        info!("  DFM Score: {:.1}%", dfm_score * 100.0);
        info!("  DFA Score: {:.1}%", dfa_score * 100.0);
        info!("  Estimated Yield: {:.1}%", estimated_yield * 100.0);
        info!("  Unit Cost: ${:.2}", estimated_cost.total_unit_cost);

        Ok(ManufacturingAnalysisResult {
            dfm_score,
            dfa_score,
            estimated_yield,
            estimated_cost,
            violations,
            warnings,
            suggestions,
            panelization,
            test_coverage,
            assembly_sequence,
            critical_components,
        })
    }

    /// Check manufacturing design rules
    fn check_manufacturing_rules(&self, netlist: &Netlist) -> Result<Vec<ManufacturingViolation>> {
        let mut violations = Vec::new();

        // Check component spacing
        for (id1, inst1) in &netlist.instances {
            for (id2, inst2) in &netlist.instances {
                if id1 >= id2 {
                    continue;
                }
                
                // Simplified spacing check (would use actual placement data)
                let spacing = 1.0; // mm - placeholder
                if spacing < self.process_capabilities.min_courtyard_spacing {
                    violations.push(ManufacturingViolation {
                        violation_type: ViolationType::ComponentSpacing,
                        severity: ViolationSeverity::Major,
                        location: format!("{} to {}", inst1.name, inst2.name),
                        description: format!(
                            "Component spacing {:.2}mm below minimum {:.2}mm",
                            spacing,
                            self.process_capabilities.min_courtyard_spacing
                        ),
                        impact: "May cause assembly collisions or rework difficulty".to_string(),
                        resolution: "Increase component spacing or use smaller packages".to_string(),
                    });
                }
            }
        }

        // Check for fiducial markers
        if netlist.instances.len() > 10 {
            // Complex boards need fiducials
            let has_fiducials = false; // Placeholder - would check for actual fiducials
            if !has_fiducials {
                violations.push(ManufacturingViolation {
                    violation_type: ViolationType::FiducialMissing,
                    severity: ViolationSeverity::Major,
                    location: "PCB".to_string(),
                    description: "Missing fiducial markers for automated assembly".to_string(),
                    impact: "Reduced placement accuracy, potential assembly failures".to_string(),
                    resolution: "Add at least 2 global fiducials to PCB corners".to_string(),
                });
            }
        }

        Ok(violations)
    }

    /// Analyze assembly feasibility
    fn analyze_assembly_feasibility(&self, netlist: &Netlist) -> Result<Vec<ManufacturingWarning>> {
        let mut warnings = Vec::new();

        for (_, instance) in &netlist.instances {
            // Check for difficult packages
            if instance.name.contains("BGA") || instance.name.contains("QFN") {
                warnings.push(ManufacturingWarning {
                    warning_type: "DifficultPackage".to_string(),
                    component: instance.name.clone(),
                    description: "Component uses advanced package requiring special handling".to_string(),
                    recommendation: "Ensure X-ray inspection capability for BGA/QFN packages".to_string(),
                });
            }

            // Check for mixed technology
            if instance.name.contains("TH") {
                warnings.push(ManufacturingWarning {
                    warning_type: "MixedTechnology".to_string(),
                    component: instance.name.clone(),
                    description: "Through-hole component in SMT assembly".to_string(),
                    recommendation: "Consider SMT alternative or plan for wave/selective soldering".to_string(),
                });
            }
        }

        Ok(warnings)
    }

    /// Analyze component consolidation opportunities
    fn analyze_component_consolidation(&self, netlist: &Netlist) -> Result<Vec<OptimizationSuggestion>> {
        let mut suggestions = Vec::new();

        // Count component values
        let mut resistor_values = HashMap::new();
        let mut capacitor_values = HashMap::new();

        for (_, instance) in &netlist.instances {
            if instance.name.starts_with("R") {
                if let Some(value) = instance.attributes.get("value") {
                    *resistor_values.entry(value.clone()).or_insert(0) += 1;
                }
            } else if instance.name.starts_with("C") {
                if let Some(value) = instance.attributes.get("value") {
                    *capacitor_values.entry(value.clone()).or_insert(0) += 1;
                }
            }
        }

        // Suggest consolidation for similar values
        if resistor_values.len() > 10 {
            suggestions.push(OptimizationSuggestion {
                suggestion_type: SuggestionType::ComponentConsolidation,
                description: format!(
                    "Consolidate {} different resistor values to reduce BOM complexity",
                    resistor_values.len()
                ),
                impact: OptimizationImpact {
                    yield_improvement: 0.02,
                    cost_reduction: 0.05,
                    assembly_time_reduction: 0.1,
                    quality_improvement: 0.03,
                },
                implementation_effort: ImplementationEffort::Medium,
            });
        }

        Ok(suggestions)
    }

    /// Optimize panelization
    fn optimize_panelization(&self, netlist: &Netlist) -> Result<PanelizationResult> {
        // Simplified panelization (would use actual board dimensions)
        let board_width = 100.0;  // mm
        let board_height = 80.0;  // mm
        let panel_width = 300.0;  // mm  
        let panel_height = 250.0; // mm

        let spacing = 2.0; // mm between boards
        let rail_width = 5.0; // mm

        let usable_width = panel_width - 2.0 * rail_width;
        let usable_height = panel_height - 2.0 * rail_width;

        let cols = (((usable_width + spacing) / (board_width + spacing)) as f64).floor() as usize;
        let rows = (((usable_height + spacing) / (board_height + spacing)) as f64).floor() as usize;

        let boards_per_panel = rows * cols;
        let utilization = (boards_per_panel as f64 * board_width * board_height) 
            / (panel_width * panel_height);

        Ok(PanelizationResult {
            panel_size: (panel_width, panel_height),
            boards_per_panel,
            utilization,
            panel_layout: PanelLayout {
                rows,
                columns: cols,
                spacing,
                rail_width,
            },
            breakaway_method: BreakawayMethod::VScoring,
        })
    }

    /// Analyze test coverage
    fn analyze_test_coverage(&self, netlist: &Netlist) -> Result<TestCoverageAnalysis> {
        let total_nets = netlist.nets.len();
        let testable_nets = (total_nets as f64 * 0.85) as usize; // Simplified estimate

        Ok(TestCoverageAnalysis {
            in_circuit_test_coverage: 0.85,
            boundary_scan_coverage: 0.6,
            functional_test_coverage: 0.95,
            optical_inspection_coverage: 1.0,
            test_points_required: total_nets / 3,
            test_points_available: testable_nets / 3,
            untestable_nets: vec![], // Would identify actual untestable nets
        })
    }

    /// Optimize assembly sequence
    fn optimize_assembly_sequence(&self, netlist: &Netlist) -> Result<Vec<AssemblyStep>> {
        let mut sequence = Vec::new();

        // Step 1: Solder paste printing
        sequence.push(AssemblyStep {
            step_number: 1,
            process: AssemblyProcess::SMTPastePrinting,
            components: vec![],
            time_estimate: 0.5,
            equipment_required: "Stencil Printer".to_string(),
        });

        // Step 2: SMT placement
        let smt_components: Vec<String> = netlist.instances
            .values()
            .filter(|i| !i.name.contains("TH"))
            .map(|i| i.name.clone())
            .collect();

        sequence.push(AssemblyStep {
            step_number: 2,
            process: AssemblyProcess::SMTPlacement,
            components: smt_components.clone(),
            time_estimate: smt_components.len() as f64 * 0.02,
            equipment_required: "Pick and Place Machine".to_string(),
        });

        // Step 3: Reflow
        sequence.push(AssemblyStep {
            step_number: 3,
            process: AssemblyProcess::ReflowSoldering,
            components: smt_components,
            time_estimate: 5.0,
            equipment_required: "Reflow Oven".to_string(),
        });

        // Step 4: Inspection
        sequence.push(AssemblyStep {
            step_number: 4,
            process: AssemblyProcess::Inspection,
            components: vec![],
            time_estimate: 2.0,
            equipment_required: "AOI System".to_string(),
        });

        Ok(sequence)
    }

    /// Identify critical components
    fn identify_critical_components(&self, netlist: &Netlist) -> Result<Vec<CriticalComponent>> {
        let mut critical = Vec::new();

        for (_, instance) in &netlist.instances {
            // Check for high-value components
            if instance.name.contains("IC") || instance.name.contains("MCU") {
                critical.push(CriticalComponent {
                    component_name: instance.name.clone(),
                    criticality_reason: CriticalityReason::HighValue,
                    special_handling: vec![
                        "ESD protection required".to_string(),
                        "Moisture sensitive - bake before use".to_string(),
                    ],
                    risk_mitigation: vec![
                        "Maintain controlled storage environment".to_string(),
                        "Use ESD-safe handling procedures".to_string(),
                    ],
                });
            }
        }

        Ok(critical)
    }

    /// Estimate production yield
    fn estimate_production_yield(
        &self,
        netlist: &Netlist,
        violations: &[ManufacturingViolation],
    ) -> Result<f64> {
        let mut yield_estimate = self.cost_model.yield_model.base_yield;

        // Reduce yield for violations
        for violation in violations {
            match violation.severity {
                ViolationSeverity::Critical => yield_estimate *= 0.7,
                ViolationSeverity::Major => yield_estimate *= 0.9,
                ViolationSeverity::Minor => yield_estimate *= 0.95,
                ViolationSeverity::Info => {}
            }
        }

        // Complexity factor
        let complexity = netlist.instances.len() as f64 / 100.0;
        yield_estimate *= (1.0 - complexity * 0.01).max(0.8);

        Ok(yield_estimate)
    }

    /// Estimate manufacturing cost
    fn estimate_manufacturing_cost(
        &self,
        netlist: &Netlist,
        assembly_sequence: &[AssemblyStep],
        production_yield: f64,
    ) -> Result<CostBreakdown> {
        let component_count = netlist.instances.len();
        
        // Material costs (simplified)
        let pcb_area = 80.0; // cm² - placeholder
        let material_cost = pcb_area * self.cost_model.material_costs.pcb_cost_per_sqcm;

        // Assembly costs
        let placement_cost = component_count as f64 
            * self.cost_model.assembly_costs.placement_cost_per_component;
        
        let assembly_time: f64 = assembly_sequence.iter().map(|s| s.time_estimate).sum();
        let assembly_cost = placement_cost 
            + assembly_time * self.cost_model.assembly_costs.machine_time_hourly_rate / 60.0;

        // Testing costs
        let testing_cost = self.cost_model.testing_costs.aoi_cost
            + self.cost_model.testing_costs.functional_test_cost;

        // Setup cost amortization
        let volume = match self.config.target_volume {
            ProductionVolume::Prototype => 10,
            ProductionVolume::LowVolume => 100,
            ProductionVolume::MediumVolume => 1000,
            ProductionVolume::HighVolume => 10000,
            ProductionVolume::MassProduction => 100000,
        };
        
        let setup_cost_amortized = self.cost_model.setup_costs.nre_cost / volume as f64;

        // Adjust for yield
        let total_unit_cost = (material_cost + assembly_cost + testing_cost + setup_cost_amortized) / production_yield;

        // Volume discounts
        let volume_discounts = vec![
            VolumeDiscount {
                quantity: 100,
                unit_cost: total_unit_cost * 1.0,
                total_cost: total_unit_cost * 100.0,
            },
            VolumeDiscount {
                quantity: 1000,
                unit_cost: total_unit_cost * 0.85,
                total_cost: total_unit_cost * 0.85 * 1000.0,
            },
            VolumeDiscount {
                quantity: 10000,
                unit_cost: total_unit_cost * 0.7,
                total_cost: total_unit_cost * 0.7 * 10000.0,
            },
        ];

        Ok(CostBreakdown {
            material_cost,
            assembly_cost,
            testing_cost,
            setup_cost_amortized,
            total_unit_cost,
            volume_discounts,
        })
    }

    /// Generate DFM suggestions
    fn generate_dfm_suggestions(
        &self,
        netlist: &Netlist,
        violations: &[ManufacturingViolation],
    ) -> Result<Vec<OptimizationSuggestion>> {
        let mut suggestions = Vec::new();

        if violations.iter().any(|v| v.violation_type == ViolationType::ComponentSpacing) {
            suggestions.push(OptimizationSuggestion {
                suggestion_type: SuggestionType::ComponentPlacement,
                description: "Optimize component placement to meet spacing requirements".to_string(),
                impact: OptimizationImpact {
                    yield_improvement: 0.1,
                    cost_reduction: 0.05,
                    assembly_time_reduction: 0.0,
                    quality_improvement: 0.15,
                },
                implementation_effort: ImplementationEffort::Medium,
            });
        }

        Ok(suggestions)
    }

    /// Generate DFA suggestions
    fn generate_dfa_suggestions(
        &self,
        netlist: &Netlist,
        warnings: &[ManufacturingWarning],
    ) -> Result<Vec<OptimizationSuggestion>> {
        let mut suggestions = Vec::new();

        if warnings.iter().any(|w| w.warning_type == "MixedTechnology") {
            suggestions.push(OptimizationSuggestion {
                suggestion_type: SuggestionType::PackageStandardization,
                description: "Replace through-hole components with SMT alternatives".to_string(),
                impact: OptimizationImpact {
                    yield_improvement: 0.05,
                    cost_reduction: 0.15,
                    assembly_time_reduction: 0.25,
                    quality_improvement: 0.1,
                },
                implementation_effort: ImplementationEffort::Low,
            });
        }

        Ok(suggestions)
    }

    /// Calculate DFM score
    fn calculate_dfm_score(&self, violations: &[ManufacturingViolation]) -> f64 {
        let critical_count = violations.iter()
            .filter(|v| matches!(v.severity, ViolationSeverity::Critical))
            .count();
        let major_count = violations.iter()
            .filter(|v| matches!(v.severity, ViolationSeverity::Major))
            .count();

        let score = 1.0 - (critical_count as f64 * 0.2) - (major_count as f64 * 0.1);
        score.max(0.0)
    }

    /// Calculate DFA score
    fn calculate_dfa_score(
        &self,
        warnings: &[ManufacturingWarning],
        assembly_sequence: &[AssemblyStep],
    ) -> f64 {
        let warning_penalty = warnings.len() as f64 * 0.05;
        let sequence_efficiency = 1.0 / (assembly_sequence.len() as f64 / 5.0);
        
        (1.0 - warning_penalty) * sequence_efficiency
    }
}

impl Default for ManufacturingConfig {
    fn default() -> Self {
        Self {
            target_process: ManufacturingProcess::SmallBatch,
            assembly_method: AssemblyMethod::FullySMT,
            target_volume: ProductionVolume::MediumVolume,
            quality_level: QualityLevel::Standard,
            enable_panelization: true,
            enable_testpoint_generation: true,
            enable_component_consolidation: true,
            enable_placement_optimization: true,
            enable_routing_optimization: true,
            target_yield: 0.95,
            max_board_layers: 4,
            preferred_component_packages: vec!["0603".to_string(), "0805".to_string()],
            avoid_component_packages: vec!["0201".to_string(), "01005".to_string()],
        }
    }
}

impl Default for ProcessCapabilities {
    fn default() -> Self {
        Self {
            min_trace_width: 0.2,
            min_trace_spacing: 0.2,
            min_via_size: 0.3,
            min_via_spacing: 0.3,
            min_hole_size: 0.25,
            min_annular_ring: 0.1,
            min_solder_mask_web: 0.1,
            min_silkscreen_width: 0.15,
            min_courtyard_spacing: 0.25,
            max_aspect_ratio: 10.0,
            placement_accuracy: 0.1,
            supported_packages: HashSet::new(),
            special_processes: Vec::new(),
        }
    }
}

impl Default for AssemblyConstraints {
    fn default() -> Self {
        Self {
            min_component_spacing: 0.5,
            min_component_size: (0.6, 0.3),
            max_component_height: 15.0,
            min_pad_size: 0.2,
            fiducial_requirements: FiducialRequirements {
                global_fiducials: 2,
                local_fiducials_fine_pitch: true,
                fiducial_size: 1.0,
                fiducial_clearance: 3.0,
            },
            component_orientation_rules: ComponentOrientationRules {
                standardize_orientation: true,
                preferred_angles: vec![0.0, 90.0, 180.0, 270.0],
                avoid_angles: vec![45.0, 135.0, 225.0, 315.0],
            },
            wave_solder_constraints: None,
            reflow_profile: ReflowProfile {
                peak_temperature: 245.0,
                time_above_liquidus: 60.0,
                max_heating_rate: 3.0,
                max_cooling_rate: 6.0,
            },
        }
    }
}

impl Default for ComponentManufacturingData {
    fn default() -> Self {
        Self {
            components: HashMap::new(),
        }
    }
}

impl Default for ManufacturingCostModel {
    fn default() -> Self {
        Self {
            setup_costs: SetupCosts {
                nre_cost: 500.0,
                tooling_cost: 200.0,
                programming_cost: 100.0,
            },
            material_costs: MaterialCosts {
                pcb_cost_per_sqcm: 0.1,
                layer_adder: 0.05,
                special_process_costs: HashMap::new(),
            },
            assembly_costs: AssemblyCosts {
                placement_cost_per_component: 0.02,
                solder_joint_cost: 0.01,
                manual_assembly_hourly_rate: 50.0,
                machine_time_hourly_rate: 100.0,
            },
            testing_costs: TestingCosts {
                ict_test_cost: 5.0,
                functional_test_cost: 10.0,
                boundary_scan_cost: 3.0,
                aoi_cost: 2.0,
            },
            yield_model: YieldModel {
                base_yield: 0.98,
                complexity_factor: 0.001,
                component_yield_impact: HashMap::new(),
            },
        }
    }
}