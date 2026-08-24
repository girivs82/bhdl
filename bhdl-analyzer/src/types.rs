use std::collections::{HashMap, HashSet};
use rowan::{TextRange};
use rowan::ast::SyntaxNodePtr;
use bhdl_parser::BhdlLanguage; // Needed for SyntaxNodePtr
use crate::symbol_table::SymbolTable; // Use crate:: to refer to local module
use crate::power_analysis::PowerAnalysisContext;
use crate::component_inference::ComponentInferenceContext;
use crate::attribute_analysis::AttributeAnalysisResult;
use crate::flow_tracking::FlowTracker;
use crate::passes::SafetyAnalysisResult;

/// Unified simulation results from all analysis engines
/// This contains all simulation data needed by subsequent phases to avoid multiple simulation runs
#[derive(Debug, Clone)]
pub struct UnifiedSimulationData {
    /// DC operating point analysis results
    pub dc_analysis: Option<DcSimulationResults>,
    
    /// Electrical safety violations from bhdl-spice
    pub electrical_safety: Option<ElectricalSafetyResults>,
    
    /// Thermal analysis results
    pub thermal_analysis: Option<ThermalSimulationResults>,
    
    /// AC frequency response (when implemented)
    pub ac_analysis: Option<AcSimulationResults>,
    
    /// Transient analysis (when implemented)  
    pub transient_analysis: Option<TransientSimulationResults>,
    
    /// Simulation metadata
    pub simulation_metadata: SimulationMetadata,
}

/// DC operating point simulation results
#[derive(Debug, Clone)]
pub struct DcSimulationResults {
    /// Node voltages across the circuit
    pub node_voltages: HashMap<String, f64>,
    
    /// Branch currents through components
    pub branch_currents: HashMap<String, f64>,
    
    /// Power dissipation per component
    pub power_dissipation: HashMap<String, f64>,
    
    /// Component operating temperatures
    pub operating_temperatures: HashMap<String, f64>,
    
    /// Convergence information
    pub converged: bool,
    pub iterations: usize,
    pub final_residual: f64,
}

/// Electrical safety analysis results from SPICE engine
#[derive(Debug, Clone)]
pub struct ElectricalSafetyResults {
    /// Component stress analysis
    pub component_stress: HashMap<String, ComponentStressAnalysis>,
    
    /// Current density violations
    pub current_density_violations: Vec<CurrentDensityViolation>,
    
    /// Voltage stress violations  
    pub voltage_stress_violations: Vec<VoltageStressViolation>,
    
    /// Thermal stress violations
    pub thermal_stress_violations: Vec<ThermalStressViolation>,
    
    /// Overall safety summary
    pub safety_summary: ElectricalSafetySummary,
}

/// Component stress analysis details
#[derive(Debug, Clone)]
pub struct ComponentStressAnalysis {
    pub component_name: String,
    pub voltage_stress_ratio: f64,    // Operating voltage / Max voltage
    pub current_stress_ratio: f64,    // Operating current / Max current  
    pub power_stress_ratio: f64,      // Operating power / Max power
    pub thermal_stress_ratio: f64,    // Operating temp / Max temp
    pub has_voltage_stress: bool,
    pub has_current_stress: bool,
    pub has_thermal_stress: bool,
    pub derating_recommendations: Vec<DeratingRecommendation>,
}

/// Derating recommendations for components
#[derive(Debug, Clone)]
pub struct DeratingRecommendation {
    pub parameter: String,           // "power_rating", "voltage_rating", etc.
    pub current_value: f64,
    pub recommended_value: f64,
    pub derating_factor: f64,
    pub reason: String,
}

/// Current density violations
#[derive(Debug, Clone)]
pub struct CurrentDensityViolation {
    pub location: String,
    pub current: f64,
    pub max_safe_current: f64,
    pub severity: SafetyViolationSeverity,
}

/// Voltage stress violations
#[derive(Debug, Clone)]
pub struct VoltageStressViolation {
    pub component: String,
    pub applied_voltage: f64,
    pub max_voltage: f64,
    pub stress_ratio: f64,
    pub severity: SafetyViolationSeverity,
}

/// Thermal stress violations
#[derive(Debug, Clone)]
pub struct ThermalStressViolation {
    pub component: String,
    pub operating_temperature: f64,
    pub max_temperature: f64,
    pub thermal_derating_required: bool,
    pub severity: SafetyViolationSeverity,
}

/// Safety violation severity levels
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum SafetyViolationSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Electrical safety summary
#[derive(Debug, Clone)]
pub struct ElectricalSafetySummary {
    pub total_violations: usize,
    pub critical_violations: usize,
    pub components_needing_derating: Vec<String>,
    pub estimated_reliability_impact: f64, // 0.0 to 1.0
}

/// Thermal simulation results
#[derive(Debug, Clone)]
pub struct ThermalSimulationResults {
    pub component_temperatures: HashMap<String, f64>,
    pub hot_spots: Vec<HotSpot>,
    pub thermal_derating_factors: HashMap<String, f64>,
    pub ambient_temperature: f64,
}

/// Hot spot identification
#[derive(Debug, Clone)]
pub struct HotSpot {
    pub location: String,
    pub temperature: f64,
    pub components_affected: Vec<String>,
    pub cooling_required: bool,
}

/// AC frequency response results (future)
#[derive(Debug, Clone)]
pub struct AcSimulationResults {
    pub frequency_points: Vec<f64>,
    pub magnitude_response: HashMap<String, Vec<f64>>,
    pub phase_response: HashMap<String, Vec<f64>>,
    pub bandwidth: HashMap<String, f64>,
    pub stability_margins: HashMap<String, StabilityMargin>,
}

/// Stability margin analysis
#[derive(Debug, Clone)]
pub struct StabilityMargin {
    pub gain_margin_db: f64,
    pub phase_margin_deg: f64,
    pub is_stable: bool,
}

/// Transient simulation results (future)
#[derive(Debug, Clone)]
pub struct TransientSimulationResults {
    pub time_points: Vec<f64>,
    pub node_voltages_vs_time: HashMap<String, Vec<f64>>,
    pub ripple_currents: HashMap<String, f64>,
    pub settling_times: HashMap<String, f64>,
    pub peak_currents: HashMap<String, f64>,
}

/// Simulation metadata and performance info
#[derive(Debug, Clone)]
pub struct SimulationMetadata {
    pub simulation_time_ms: f64,
    pub engines_used: Vec<String>,
    pub simulation_accuracy: SimulationAccuracy,
    pub warnings: Vec<String>,
    pub timestamp: std::time::SystemTime,
}

/// Simulation accuracy assessment
#[derive(Debug, Clone)]
pub struct SimulationAccuracy {
    pub convergence_quality: f64,    // 0.0 to 1.0
    pub model_fidelity: f64,         // 0.0 to 1.0  
    pub confidence_level: f64,       // 0.0 to 1.0
    pub limitations: Vec<String>,
}

impl Default for UnifiedSimulationData {
    fn default() -> Self {
        Self {
            dc_analysis: None,
            electrical_safety: None,
            thermal_analysis: None,
            ac_analysis: None,
            transient_analysis: None,
            simulation_metadata: SimulationMetadata {
                simulation_time_ms: 0.0,
                engines_used: Vec::new(),
                simulation_accuracy: SimulationAccuracy {
                    convergence_quality: 0.0,
                    model_fidelity: 0.0,
                    confidence_level: 0.0,
                    limitations: Vec::new(),
                },
                warnings: Vec::new(),
                timestamp: std::time::SystemTime::now(),
            },
        }
    }
}

impl UnifiedSimulationData {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Check if this component has any safety violations
    pub fn has_safety_violations(&self, component_name: &str) -> bool {
        if let Some(ref safety) = self.electrical_safety {
            safety.component_stress.get(component_name)
                .map(|stress| stress.has_voltage_stress || stress.has_current_stress || stress.has_thermal_stress)
                .unwrap_or(false)
        } else {
            false
        }
    }
    
    /// Get derating factor for a component based on all safety analysis
    pub fn get_derating_factor(&self, component_name: &str) -> f64 {
        let mut base_derating = 1.0;
        
        if let Some(ref safety) = self.electrical_safety {
            if let Some(stress) = safety.component_stress.get(component_name) {
                // Apply additional derating based on stress analysis
                if stress.has_voltage_stress {
                    base_derating *= 0.8; // 20% additional derating
                }
                if stress.has_current_stress {
                    base_derating *= 0.9; // 10% additional derating  
                }
                if stress.has_thermal_stress {
                    base_derating *= 0.7; // 30% additional derating
                }
            }
        }
        
        // Apply thermal derating if available
        if let Some(ref thermal) = self.thermal_analysis {
            if let Some(thermal_factor) = thermal.thermal_derating_factors.get(component_name) {
                base_derating *= thermal_factor;
            }
        }
        
        base_derating
    }
    
    /// Get actual operating voltage for a component
    pub fn get_operating_voltage(&self, component_name: &str) -> Option<f64> {
        self.dc_analysis.as_ref()
            .and_then(|dc| dc.node_voltages.get(component_name))
            .copied()
    }
    
    /// Get actual operating current for a component
    pub fn get_operating_current(&self, component_name: &str) -> Option<f64> {
        self.dc_analysis.as_ref()
            .and_then(|dc| dc.branch_currents.get(component_name))
            .copied()
    }
    
    /// Get actual power dissipation for a component
    pub fn get_power_dissipation(&self, component_name: &str) -> Option<f64> {
        self.dc_analysis.as_ref()
            .and_then(|dc| dc.power_dissipation.get(component_name))
            .copied()
    }
}

// Represents resolved type information for checking
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTypeInfo {
    pub base_type_name: String,
    // Represents width: None for scalar, Some((high, low)) for bus
    pub bounds: Option<(i64, i64)>,
}

impl ResolvedTypeInfo {
    // Helper to get width (number of bits)
    pub fn width(&self) -> Option<u64> {
        self.bounds.map(|(h, l)| (h - l).abs() as u64 + 1)
    }
}

/// Structured diagnostic message with optional typed classification.
///
/// All diagnostics have a `message` and `range`. The structured fields
/// (`kind`, `severity`, `code`, `hints`, `related`) are optional for
/// backward compatibility — existing code that only sets `message`/`range`
/// continues to work. New code should populate the structured fields.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub message: String,
    pub range: TextRange,
    /// Typed diagnostic category (defaults to Unclassified for legacy diagnostics).
    pub kind: bhdl_common::DiagnosticKind,
    /// Severity level.
    pub severity: bhdl_common::Severity,
    /// Machine-readable error code (e.g., "E0100"). Auto-derived from `kind`.
    pub code: String,
    /// Hints with optional suggested fixes.
    pub hints: Vec<bhdl_common::DiagnosticHint>,
    /// Related source locations.
    pub related: Vec<bhdl_common::RelatedInfo>,
}

impl Diagnostic {
    /// Create a simple diagnostic with just a message and range (backward compat).
    pub fn new(message: String, range: TextRange) -> Self {
        Self {
            message,
            range,
            kind: bhdl_common::DiagnosticKind::Unclassified,
            severity: bhdl_common::Severity::Error,
            code: "E0000".to_string(),
            hints: Vec::new(),
            related: Vec::new(),
        }
    }

    /// Create a structured diagnostic with a specific kind.
    pub fn with_kind(kind: bhdl_common::DiagnosticKind, message: String, range: TextRange) -> Self {
        let code = kind.error_code().to_string();
        Self {
            message,
            range,
            kind,
            severity: bhdl_common::Severity::Error,
            code,
            hints: Vec::new(),
            related: Vec::new(),
        }
    }

    /// Set severity and return self (builder pattern).
    pub fn with_severity(mut self, severity: bhdl_common::Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Add a hint and return self (builder pattern).
    pub fn with_hint(mut self, message: impl Into<String>) -> Self {
        self.hints.push(bhdl_common::DiagnosticHint {
            message: message.into(),
            fix: None,
        });
        self
    }

    /// Add related info and return self (builder pattern).
    pub fn with_related(mut self, message: impl Into<String>) -> Self {
        self.related.push(bhdl_common::RelatedInfo {
            message: message.into(),
        });
        self
    }
}

// Type alias for the map storing results of constant evaluation.
// ConstValue supports integers, floats, booleans, strings, and physical quantities.
pub type ResolvedConstants = HashMap<SyntaxNodePtr<BhdlLanguage>, bhdl_common::ConstValue>;

// Source location information for diagnostics
#[derive(Debug, Clone, PartialEq)]
pub struct SourceLocation {
    pub line: u32,
    pub column: u32,
    pub file: Option<String>,
}

impl SourceLocation {
    pub fn new(line: u32, column: u32) -> Self {
        Self {
            line,
            column,
            file: None,
        }
    }
    
    pub fn with_file(line: u32, column: u32, file: String) -> Self {
        Self {
            line,
            column,
            file: Some(file),
        }
    }
    
    pub fn unknown() -> Self {
        Self {
            line: 0,
            column: 0,
            file: None,
        }
    }
}

// Analysis results including scopes and diagnostics
pub struct AnalysisResult {
    pub global_scope: SymbolTable,
    pub definition_scopes: HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>,
    /// Arena-based scope registry with parent-chain lookup.
    /// During migration, `global_scope` and `definition_scopes` are extracted
    /// from this registry for backward compatibility.
    pub scope_registry: crate::scope_registry::ScopeRegistry,
    pub diagnostics: Vec<Diagnostic>,
    pub resolved_constants: ResolvedConstants,
    pub power_analysis: PowerAnalysisContext,
    pub component_inference: ComponentInferenceContext,
    pub netlist: Option<bhdl_netlist::Netlist>,
    pub attribute_analysis: AttributeAnalysisResult,
    pub flow_tracker: Option<FlowTracker>,
    pub safety_analysis: SafetyAnalysisResult,
    /// Unified simulation data from all engines (run once, used by all phases)
    pub simulation_data: UnifiedSimulationData,
    /// Component instance registry (Phase 2: Pass 1.25)
    pub instance_registry: crate::passes::InstanceRegistry,
    /// Power domain expansion results (Phase 1: Scalability)
    pub power_domain_expansion: crate::passes::PowerDomainExpansion,
    /// Monomorphization results (Pass 2.5: generic specialization)
    pub monomorphization: crate::passes::MonomorphizationResult,
    /// Expansion recipes extracted from entity `expansion { }` blocks
    pub expansion_recipes: HashMap<String, bhdl_common::ExpansionRecipe>,
    /// Design recipes extracted from entity `design for <intent> { }` blocks.
    /// Keyed by entity name → intent name → recipe. When the synthesizer
    /// runs the intent-driven designer for an instance, it consults this
    /// map first and falls back to the Rust reference designer on miss.
    pub design_recipes: HashMap<String, HashMap<String, bhdl_common::design::DesignRecipe>>,
    /// Stress recipes extracted from entity `simulation { stress { } }` blocks
    /// (Vendor_Simulation_Blocks.md §4). Keyed by entity name. When sign-off
    /// computes per-part stress for a switcher, it evaluates this recipe for
    /// per-child overrides, falling back to the hardcoded reference ripple
    /// model when the entity declares no block.
    pub stress_recipes: HashMap<String, bhdl_common::stress::StressRecipe>,
    /// Model recipes extracted from entity `simulation { model { } }` blocks
    /// (Vendor_Simulation_Blocks.md §5). Keyed by entity name. The SPICE
    /// converter consults this when stamping a device, using the entity's
    /// authored `node source/draws` branches in place of its hardcoded
    /// decomposition (fallback when absent).
    pub model_recipes: HashMap<String, bhdl_common::model::ModelRecipe>,
    /// Board-level SKU variants extracted from `variant <Name> { }`
    /// blocks. Keyed by board name → variant name → patch set.
    /// Empty when no variant blocks are declared (existing boards
    /// keep working as a single implicit "default" SKU).
    pub variants: HashMap<String, HashMap<String, bhdl_common::variant::Variant>>,
    /// Global entity attribute index: per-entity flat attribute
    /// defaults gathered from every imported file and the main file.
    /// Used by the synthesizer's expansion interpreter to late-bind
    /// entity attributes onto leaf instances when the analyzer's
    /// recipe-extraction overlay missed them (the overlay is order-
    /// dependent across imports; this index isn't, so it's the
    /// reliable fallback). See pass1::build_scope_registry_with_base.
    pub entity_attribute_index: HashMap<String, HashMap<String, String>>,
    /// Per-entity ordered constructor-parameter names, gathered from
    /// every imported file and the main file (order-independent, like
    /// `entity_attribute_index`). The synthesizer's expansion
    /// interpreter uses it to resolve attribute values that are bare
    /// references to a child entity's own parameter (e.g. `attribute
    /// capacitance = value;`) into the argument supplied at the
    /// instantiation site, instead of leaking the literal param name.
    pub entity_param_names: HashMap<String, Vec<String>>,
    /// Per-entity attribute → constructor-param bare-reference linkage
    /// (see `extract_entity_attr_param_refs`): lets expansion-child
    /// substitution thread explicit args into attrs whose defaulted
    /// param-refs were resolved away at extraction time.
    pub entity_attr_param_refs: HashMap<String, HashMap<String, String>>,
    /// Placement recipes extracted from entity `placement { }` blocks
    pub placement_recipes: HashMap<String, bhdl_common::PlacementRecipe>,
    /// Symbol definitions extracted from `symbol EntityName { }` blocks
    pub symbol_definitions: HashMap<String, bhdl_common::SymbolDefinition>,
    /// Layout definitions extracted from `layout EntityName { }` blocks
    pub layout_definitions: HashMap<String, bhdl_common::LayoutDefinition>,
}

impl Default for AnalysisResult {
    fn default() -> Self {
        Self {
            global_scope: SymbolTable::default(),
            definition_scopes: HashMap::new(),
            scope_registry: crate::scope_registry::ScopeRegistry::new(),
            diagnostics: Vec::new(),
            resolved_constants: HashMap::new(),
            power_analysis: PowerAnalysisContext::new(),
            component_inference: ComponentInferenceContext::new(),
            netlist: None,
            attribute_analysis: AttributeAnalysisResult {
                attributes: HashMap::new(),
                dependencies: HashMap::new(),
                evaluation_order: Vec::new(),
                circular_dependencies: Vec::new(),
                mutable_attributes: HashSet::new(),
            },
            flow_tracker: None,
            safety_analysis: SafetyAnalysisResult::default(),
            simulation_data: UnifiedSimulationData::default(),
            instance_registry: crate::passes::InstanceRegistry::new(),
            power_domain_expansion: crate::passes::PowerDomainExpansion::new(),
            monomorphization: crate::passes::MonomorphizationResult::new(),
            expansion_recipes: HashMap::new(),
            design_recipes: HashMap::new(),
            stress_recipes: HashMap::new(),
            model_recipes: HashMap::new(),
            variants: HashMap::new(),
            entity_attribute_index: HashMap::new(),
            entity_param_names: HashMap::new(),
            entity_attr_param_refs: HashMap::new(),
            placement_recipes: HashMap::new(),
            symbol_definitions: HashMap::new(),
            layout_definitions: HashMap::new(),
        }
    }
} 