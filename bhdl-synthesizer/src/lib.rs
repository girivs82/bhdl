//! BHDL Synthesizer - Converts semantic analysis results to netlists
//! 
//! This crate bridges the gap between the BHDL analyzer (semantic analysis)
//! and the netlist representation, preserving semantic context for intelligent
//! visualization and layout.

use anyhow::{Result, Context};
use std::collections::HashMap;
use std::path::Path;
use log::{debug, info, warn, error};
use bhdl_common::ComponentTypeMapper;
use bhdl_common::pin_metadata::{ModulePinMetadata, PinMetadata, PinDirection as CommonPinDirection, PinType as CommonPinType};
// StdlibReader removed - now using AST-based component extraction
use crate::import_loader::ImportLoader;
use crate::import_preprocessor::ImportPreprocessor;
use crate::synthesis_knowledge::{SynthesisKnowledge, SynthesisKnowledgeEngine};
use crate::intent_aware_generator::IntentAwareGenerator;
use crate::component_calculator::{ComponentCalculator, PowerSupplySpec};
use crate::behavioral_model_extractor::BehavioralModelExtractor;


// Component database mapping module  
pub mod component_mapping;

// Hierarchical connectivity extraction
pub mod hierarchical_connectivity;

// Entity variant management
pub mod entity_variants;

// Hierarchical reference designator generation
pub mod refdes_alloc;

// Interface synthesis
pub mod interface_synthesis;

// Passive component calculation engine
pub mod passive_component_calculator;

// Package selection engine
pub mod package_selector;

// GLACIER-driven component physical selection
pub mod glacier_physical_selection;
// Power-supply synthesis: the `supply` statement desugar (S1).
// docs/spec/Power_Supply_Synthesis.md.
pub mod supply_synthesis;
// Electrical rule checks — the real DRC content (driver conflicts,
// diff-pair polarity, TX/RX crossing, voltage domains, I2C pull-ups).
pub mod erc;

// T3 org-policy ERC plugins (BHDL_ERC_PLUGINS, JSON over stdio).
pub mod erc_plugin;

// Simulation-refined margin & sign-off report (spec: Simulation_Margin_Signoff.md).
pub mod signoff;

// Import loader for handling BHDL imports
pub mod abstract_resolver;

pub mod parametric_resolver;

pub mod import_loader;
pub mod freeze;

// Import preprocessor for pre-processing imports before analysis
pub mod import_preprocessor;

// Synthesis knowledge parser and storage
pub mod synthesis_knowledge;

// Virtual pin extraction from AST

// Virtual pin expansion (post-synthesis wiring of inductor/diode/cap)
pub mod virtual_pin_expander;

// Ripple-aware multi-tier capacitor bank computation
pub mod ripple_calculator;

// Intent attribute stamper (bridges FlowTracker intents → netlist attributes)
pub mod intent_attribute_stamper;

// Generic expansion interpreter (replaces attribute-driven virtual_pin_expander)
pub mod expansion_interpreter;

// Evaluator for vendor-authored `design { }` blocks (stage 3 of the
// vendor-extensibility surface — see docs/spec/Vendor_Design_Blocks.md).
pub mod design_evaluator;
pub mod stress_evaluator;
pub mod model_evaluator;
pub mod variant_apply;

// Input capacitor bank physics computation
pub mod input_cap_calculator;

// Post-GLACIER input capacitor sizing pass
pub mod input_cap_sizer;

// Post-GLACIER output capacitor sizing pass
pub mod output_cap_sizer;

// Intent-aware component generator
pub mod intent_aware_generator;

// Component value calculation engine
pub mod component_calculator;

// Simulation-driven synthesis optimization
pub mod simulation_driven;

// Behavioral model extraction from components
pub mod behavioral_model_extractor;

// Cross-component optimization coordination
pub mod cross_component_optimization;

// Design pattern recognition engine
pub mod design_pattern_recognition;

// Component compatibility analysis
pub mod component_compatibility;

// Design rule checking (DRC)
pub mod design_rule_checker;

// ML-based component selection optimization
pub mod ml_component_selection;

// Thermal simulation integration
pub mod thermal_simulation;

// Cost optimization with supplier data
pub mod cost_optimization;

// EMI/EMC (Electromagnetic Interference/Electromagnetic Compatibility) analysis
pub mod emi_emc_analysis;
pub mod reliability_analysis;

// Predictive analytics and machine learning integration
pub mod predictive_analytics;

// Manufacturing and assembly optimization (DFM/DFA)
pub mod manufacturing_optimization;

// Intent hint processor - applies synthesis hints to guide component selection
pub mod intent_hint_processor;

// Analysis-runner methods (`run_compatibility_analysis`,
// `run_emi_emc_analysis`, `run_reliability_analysis`, …) split
// out of `lib.rs` on 2026-05-26. They're impl methods on
// `NetlistGenerator` declared `pub(crate)`; the file contains a
// second `impl NetlistGenerator { ... }` block. Touching this
// file no longer invalidates the main pipeline's compile unit.
mod analysis_runners;

// Power-supply IC supporting-component synthesis (extracted
// 2026-05-26 alongside `analysis_runners`). Contiguous 289-LOC
// cluster — handles the buck / LDO / charge-pump pipeline that
// sizes input/output caps, inductors, feedback dividers, etc.
// against the source's `power VIN = X V` declarations.
mod power_supply_synthesis;

// Re-export key types
pub use bhdl_analyzer::types::AnalysisResult;
pub use bhdl_analyzer::component_inference::ParameterValue;
pub use bhdl_netlist::{Netlist, ModuleId, InstanceId, NetId, PortId, PinId, PinInstanceId, PinInstance, Pin};
pub use bhdl_netlist::types::{ModuleKind, PortDirection, PinDirection, PinType, ConnectionPoint, Unit, Quantity, NetClass};
pub use bhdl_ast::{SourceFile, Board, Entity, ComponentDef, HasName, AstNode, PinDecl};
pub use component_mapping::{DatabaseComponentMapper, DatabaseComponentInstance, DatabaseMapperStats};
pub use intent_hint_processor::{
    IntentHintProcessor, ComponentRecommendation, ComponentPreference,
    OptimizationPriority, ValidationResult,
};

/// Configuration for netlist generation
#[derive(Debug, Clone)]
pub struct NetlistConfig {
    /// Generate semantic annotations for visualization
    pub preserve_semantic_context: bool,
    /// Include power domain information
    pub include_power_domains: bool,
    /// Generate component inference annotations
    pub include_component_inference: bool,
    /// Path to component database
    pub database_path: Option<String>,
    /// Enable simulation-driven optimization
    pub enable_simulation_optimization: bool,
    /// Enable component compatibility analysis
    pub enable_compatibility_analysis: bool,
    /// Enable design pattern recognition
    pub enable_pattern_recognition: bool,
    /// Enable cross-component optimization
    pub enable_cross_optimization: bool,
    /// Enable design rule checking
    pub enable_design_rule_check: bool,
    /// Enable ML-based component selection
    pub enable_ml_selection: bool,
    /// Enable thermal simulation and analysis
    pub enable_thermal_simulation: bool,
    /// Enable cost optimization with supplier data
    pub enable_cost_optimization: bool,
    /// Enable EMI/EMC (Electromagnetic Interference/Electromagnetic Compatibility) analysis
    pub enable_emi_emc_analysis: bool,
    /// Enable reliability and lifecycle analysis
    pub enable_reliability_analysis: bool,
    /// Enable predictive analytics and machine learning integration
    pub enable_predictive_analytics: bool,
    /// Enable manufacturing and assembly optimization (DFM/DFA)
    pub enable_manufacturing_optimization: bool,
    /// Persistent refdes sidecar (`<board>.bhdl.refdes`) for phase 12.7
    /// allocation. None = allocate in-memory only (tests).
    pub refdes_lut_path: Option<std::path::PathBuf>,
}

/// Database integration statistics
#[derive(Debug, Clone)]
pub struct DatabaseStats {
    pub component_mappings: u32,
    pub cached_components: u32,
    pub cached_svg_symbols: u32,
    pub component_cache_hit_rate: f64,
    pub symbol_cache_hit_rate: f64,
}

impl Default for NetlistConfig {
    fn default() -> Self {
        Self {
            preserve_semantic_context: true,
            include_power_domains: true,
            include_component_inference: true,
            database_path: Some("/Users/girivs/src/bhdl-new/components.db".to_string()),
            enable_simulation_optimization: false,
            enable_compatibility_analysis: true,
            enable_pattern_recognition: true,
            enable_cross_optimization: true,
            enable_design_rule_check: true,
            enable_ml_selection: false, // Off by default since it requires training data
            enable_thermal_simulation: false, // Off by default for performance
            enable_cost_optimization: false, // Off by default, requires supplier API setup
            enable_emi_emc_analysis: false,  // Off by default for performance
            enable_reliability_analysis: false, // Off by default for performance
            enable_predictive_analytics: false, // Off by default for performance
            enable_manufacturing_optimization: false, // Off by default for performance
            refdes_lut_path: None,
        }
    }
}

/// Process-global Cargo-style library resolver, set once at startup by
/// the CLI from the project `bhdl.toml` + `-I`/`$BHDL_LIB_PATH`. The
/// library search configuration is an invocation-level setting (like
/// Cargo's, derived once from manifest + env + flags), so every
/// `NetlistGenerator::new()` in this process adopts it without
/// threading it through call sites. Unset in library/test contexts →
/// legacy literal-path import resolution. See
/// docs/spec/Library_Resolution.md.
static GLOBAL_LIBRARY_RESOLVER: std::sync::OnceLock<bhdl_common::library::LibraryResolver> =
    std::sync::OnceLock::new();

/// Install the process-global library resolver. Idempotent — first call
/// wins (subsequent calls are ignored), matching the once-at-startup
/// contract. Call before any synthesis runs.
pub fn set_global_library_resolver(resolver: bhdl_common::library::LibraryResolver) {
    let _ = GLOBAL_LIBRARY_RESOLVER.set(resolver);
}

pub fn global_library_resolver() -> Option<bhdl_common::library::LibraryResolver> {
    GLOBAL_LIBRARY_RESOLVER.get().cloned()
}

/// Recursively collect every `*.bhdl` file under `dir` into `out`.
fn collect_bhdl_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_bhdl_files(&p, out);
        } else if p.extension().and_then(|e| e.to_str()) == Some("bhdl") {
            out.push(p);
        }
    }
}

/// Merge one stdlib attribute into a slot, preferring a concrete value over a
/// still-unresolved dotted reference. When the same entity name is defined in
/// more than one stdlib file (e.g. `TPS54302` in both `tps54302.bhdl` and
/// `tps54302_simple.bhdl`), this keeps the usable number rather than letting
/// file order decide — a `BUCK_PARAMS.switching_frequency` that failed to
/// resolve never masks a sibling's plain `500kHz`.
fn merge_stdlib_attr(slot: &mut HashMap<String, String>, k: String, v: String) {
    // A value that is a dotted identifier path (starts with a letter, only
    // identifier chars + dots) is an unresolved const/field reference. A
    // number like `0.05` starts with a digit, so it is never mis-flagged.
    fn looks_unresolved(s: &str) -> bool {
        let s = s.trim();
        s.contains('.')
            && s.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false)
            && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.')
    }
    match slot.get(&k) {
        None => {
            slot.insert(k, v);
        }
        Some(existing) if looks_unresolved(existing) && !looks_unresolved(&v) => {
            slot.insert(k, v);
        }
        Some(_) => {}
    }
}

/// Main netlist generator that converts analyzer results to netlists
pub struct NetlistGenerator {
    config: NetlistConfig,
    netlist: Netlist,
    // Mapping from AST nodes to netlist elements
    ast_to_module: HashMap<String, ModuleId>,
    ast_to_instance: HashMap<String, InstanceId>,
    ast_to_net: HashMap<String, NetId>,
    // Database component mapper for real component instances
    database_mapper: Option<DatabaseComponentMapper>,
    // Component instances with database symbol references
    component_instances: Vec<DatabaseComponentInstance>,
    // Unified component type mapper
    type_mapper: ComponentTypeMapper,
    // Import loader for processing BHDL imports (legacy)
    import_loader: ImportLoader,
    // Import preprocessor for pre-processed imports
    import_preprocessor: Option<ImportPreprocessor>,
    // Same-file entity definitions (board-local `entity X { … }` blocks),
    // captured at synthesis start so add_pins_for_component can read their
    // DECLARED pin directions — previously local entities silently fell to
    // "default pins" and every direction-dependent check (ERC) saw junk.
    local_entities: HashMap<String, bhdl_ast::Entity>,
    // Component calculator for automatic supporting component generation
    component_calculator: ComponentCalculator,
    // Supporting component instances for virtual pin expansion
    supporting_component_instances: HashMap<String, InstanceId>,
    // Cost optimizer for supplier data integration
    cost_optimizer: Option<crate::cost_optimization::CostOptimizer>,
    // EMI/EMC analyzer for electromagnetic compatibility analysis
    emi_emc_analyzer: Option<crate::emi_emc_analysis::EMIEMCAnalyzer>,
    // Reliability analyzer for component reliability and lifecycle analysis
    reliability_analyzer: Option<crate::reliability_analysis::ReliabilityAnalyzer>,
    // Predictive analytics for machine learning integration
    predictive_analyzer: Option<crate::predictive_analytics::PredictiveAnalyzer>,
    // Manufacturing optimizer for DFM/DFA analysis
    manufacturing_optimizer: Option<crate::manufacturing_optimization::ManufacturingOptimizer>,
}

impl NetlistGenerator {
    /// Create a new netlist generator with default configuration
    pub fn new() -> Self {
        Self::with_config(NetlistConfig::default())
    }

    /// Create a new netlist generator with custom configuration
    /// Point phase 12.7 refdes allocation at the board's persistent
    /// sidecar (`<board>.bhdl.refdes`). Callers with a source path set
    /// this before generating; without it allocation is in-memory only.
    pub fn set_refdes_lut_path(&mut self, path: std::path::PathBuf) {
        self.config.refdes_lut_path = Some(path);
    }

    pub fn with_config(config: NetlistConfig) -> Self {
        
        // Initialize import loader with current directory as base.
        // If the process installed a global library resolver (set once
        // by the CLI from `bhdl.toml` + `-I`/`$BHDL_LIB_PATH`), adopt it
        // so namespaced imports resolve against declared libraries.
        // Unset → legacy literal-path behaviour (back-compat for tests
        // and stdlib-only boards). See docs/spec/Library_Resolution.md.
        let mut import_loader = ImportLoader::new(".");
        if let Some(resolver) = global_library_resolver() {
            import_loader.set_resolver(resolver);
        }

        Self {
            config,
            netlist: Netlist::new(),
            ast_to_module: HashMap::new(),
            ast_to_instance: HashMap::new(),
            ast_to_net: HashMap::new(),
            database_mapper: None, // Will be initialized async in generate_from_analysis
            component_instances: Vec::new(),
            type_mapper: ComponentTypeMapper::new(),
            import_loader,
            import_preprocessor: None,
            local_entities: HashMap::new(),
            component_calculator: ComponentCalculator::new(),
            supporting_component_instances: HashMap::new(),
            cost_optimizer: None, // Will be initialized when cost optimization is enabled
            emi_emc_analyzer: None, // Will be initialized when EMI/EMC analysis is enabled
            reliability_analyzer: None, // Will be initialized when reliability analysis is enabled
            predictive_analyzer: None, // Will be initialized when predictive analytics is enabled
            manufacturing_optimizer: None, // Will be initialized when manufacturing optimization is enabled
        }
    }

    /// Set the import preprocessor for this generator
    pub fn set_import_preprocessor(&mut self, preprocessor: ImportPreprocessor) {
        self.import_preprocessor = Some(preprocessor);
    }
    
    /// Install a Cargo-style library resolver so namespaced imports
    /// (`<lib>/<path>.bhdl`) resolve against a project `bhdl.toml` +
    /// the `-I`/`$BHDL_LIB_PATH` search path. See
    /// `docs/spec/Library_Resolution.md`.
    pub fn set_library_resolver(&mut self, resolver: bhdl_common::library::LibraryResolver) {
        self.import_loader.set_resolver(resolver);
    }

    /// Set the source file path for proper import resolution
    pub fn set_source_file_path(&mut self, path: impl AsRef<Path>) {
        // Extract the directory from the file path to use as the base for relative imports
        if let Some(parent) = path.as_ref().parent() {
            let base_path = parent.to_string_lossy().to_string();
            info!("Setting import base path to: {}", base_path);
            self.import_loader.set_base_path(base_path);
        } else {
            warn!("Could not extract parent directory from source file path: {:?}", path.as_ref());
        }
    }

    /// Generate netlist from analysis results (backwards compatibility)
    pub async fn generate_from_analysis(&mut self, analysis: &AnalysisResult) -> Result<Netlist> {
        warn!("generate_from_analysis is deprecated - connectivity extraction will be limited");
        // Create a dummy AST for backwards compatibility
        // In real usage, should use generate_from_ast_and_analysis
        self.generate_from_ast_and_analysis_internal(None, analysis).await
    }
    
    /// Generate netlist from AST and analysis results with semantic context preservation
    pub async fn generate_from_ast_and_analysis(&mut self, ast: &SourceFile, analysis: &AnalysisResult) -> Result<Netlist> {
        self.generate_from_ast_and_analysis_internal(Some(ast), analysis).await
    }
    
    /// Internal implementation that handles both cases
    async fn generate_from_ast_and_analysis_internal(&mut self, ast: Option<&SourceFile>, analysis: &AnalysisResult) -> Result<Netlist> {
        // Capture same-file entity definitions for pin-direction resolution
        // (see `local_entities` field doc).
        if let Some(src) = ast {
            for e in src.entities() {
                if let Some(n) = bhdl_ast::HasName::name(&e) {
                    self.local_entities.insert(n.text().to_string(), e.clone());
                }
            }
        }
        info!("Starting netlist generation from analysis results");
        
        // Phase 0a: Process imports if AST is available - do this FIRST
        if let Some(ast) = ast {
            info!("Processing imports from source file");
            if let Err(e) = self.import_loader.process_imports(ast) {
                warn!("Failed to process some imports: {}", e);
            }

            // Build import preprocessor from loaded entities so that
            // hierarchical_connectivity can find imported entity definitions
            // (needed for attribute extraction and pin definitions)
            if self.import_preprocessor.is_none() && !self.import_loader.loaded_entities().is_empty() {
                let mut preprocessor = ImportPreprocessor::new(".");
                preprocessor.preprocess_imports(ast).ok();
                self.import_preprocessor = Some(preprocessor);
                info!("Created import preprocessor with {} entities", self.import_loader.loaded_entities().len());
            }
        }

        // The analyzer has already processed imports and populated the global symbol table
        // We can now check for component definitions directly in the symbol table
        info!("Using analyzer's symbol table with {} symbols", analysis.global_scope.get_symbols().len());
        
        // Check for undefined components after processing imports
        if !analysis.diagnostics.is_empty() {
            let undefined_components: Vec<_> = analysis.diagnostics.iter()
                .filter(|d| d.message.contains("Undefined component type"))
                .collect();
            
            if !undefined_components.is_empty() {
                error!("Cannot synthesize circuit with undefined components:");
                for diagnostic in &undefined_components {
                    error!("  - {}", diagnostic.message);
                }
                return Err(anyhow::anyhow!(
                    "Circuit has {} undefined component(s). Please import required components before synthesis.",
                    undefined_components.len()
                ));
            }
        }
        
        // Phase 0b: Initialize database mapper if needed
        if self.database_mapper.is_none() && self.config.include_component_inference && self.config.database_path.is_some() {
            log::debug!("Attempting to initialize database mapper");
            if let Err(e) = self.initialize_database_mapper().await {
                warn!("Failed to initialize database mapper: {}", e);
                log::debug!("Database mapper initialization failed: {}", e);
                // Continue without database mapper - will use fallback
            } else {
                log::debug!("Database mapper initialized successfully");
            }
        }
        
        // Phase 1: Extract board/module hierarchy from analysis
        // Always extract to ensure top-level module is created
        info!("Phase 1: Extracting module hierarchy...");
        self.extract_module_hierarchy(analysis)?;
        info!("Phase 1 complete: {} modules created", self.netlist.modules.len());
        
        // Phase 2: Generate database component instances if mapper is available
        info!("Phase 2: Generating component instances...");
        if self.database_mapper.is_some() {
            log::debug!("Using database component mapper for instance generation");
            log::debug!("About to call generate_database_component_instances");
            let result = self.generate_database_component_instances(analysis, ast).await;
            log::debug!("Returned from generate_database_component_instances with result: {:?}", result.is_ok());
            result?;
        } else {
            // Fallback to semantic instance generation if database unavailable
            log::debug!("Using semantic instance generation (no database mapper)");
            self.generate_instances_with_semantics(analysis)?;
        }
        info!("Phase 2 complete: {} instances created", self.netlist.instances.len());
        
        // Phase 2.5: Generate connections for supporting components (virtual pin expansion)
        self.generate_supporting_component_connections(analysis)?;

        // Phase 2.7: Process power domain expansion (Phase 1: Scalability)
        info!("Phase 2.7: Processing power domain expansion...");
        self.process_power_domain_expansion(analysis)?;
        info!("Phase 2.7 complete: Power domain expansion processed");

        // Phase 3: Synthesize interface instances BEFORE connectivity extraction
        // This ensures interface signal nets exist before connections are processed
        self.synthesize_interfaces(analysis)?;
        
        // Phase 4: Extract connectivity and create nets
        if let Some(ast) = ast {
            self.extract_connectivity_from_ast(ast, analysis)?;
        } else {
            self.extract_connectivity_limited(analysis)?;
        }

        // Phase 4.1: Materialise board-level boundary ports (ports doctrine:
        // power pins are not magic — every board-level external connection is
        // a top-level Port object port-mapped to the named net). Both the
        // explicit `port` form and the power/ground sugar arrive here through
        // the same BoardPortInfo records.
        self.create_board_ports(analysis)?;

        // Phase 4.4: Stamp constructor arguments onto instances.
        // A board like `U1: LM317(v_out=5V);` carries the arg
        // `v_out=5V` on the COMPONENT_INST AST node, but the
        // multi-path instance-creation code (database mapper,
        // hierarchical extractor, component-inference) doesn't
        // consistently transfer those args onto the netlist
        // instance's attribute map — so by the time Phase 4.5
        // runs the `design { }` evaluator, `self.v_out` is
        // unset and the recipe rejects. Walk every COMPONENT_INST
        // in the AST and merge its constructor args into the
        // matching netlist instance.
        if let Some(ast) = ast {
            self.stamp_constructor_args_on_instances(
                ast,
                &analysis.monomorphization.alias_specializations,
                &analysis.entity_param_names,
            );
        }

        // Phase 4.5: Apply entity `expansion { }` blocks (virtual-pin
        // auto-expansion). Each instance whose entity declares ≥1
        // `virtual` pin AND whose entity has an `expansion { }` block
        // gets the block's child instances materialised, with parent-
        // pin references resolved to whatever the board wired them to.
        //
        // This was previously dead code — the interpreter existed but
        // nothing in the synthesis pipeline called it, so stdlib
        // entities like LM317/ATmega328P with `expansion { }` blocks
        // never actually expanded. Wired in 2026-05-28.
        if !analysis.expansion_recipes.is_empty() {
            info!(
                "Phase 4.5: running expansion interpreter ({} recipe(s) available)",
                analysis.expansion_recipes.len()
            );
            let results = crate::expansion_interpreter::expand_entity_instances_with_designs(
                &mut self.netlist,
                &analysis.expansion_recipes,
                &analysis.design_recipes,
                &analysis.entity_attribute_index,
                &analysis.entity_param_names,
                &analysis.entity_attr_param_refs,
            );
            info!(
                "Phase 4.5: expansion produced {} expanded instance(s)",
                results.len()
            );
        }

        // Phase 4.6: Stamp entity `attribute` declarations onto every
        // instance, making them FIRST-CLASS on the netlist. Entity attributes
        // (component_class, switching_frequency, topology, output_current, …)
        // were otherwise dropped for entities without an expansion/design
        // block, leaving the instance attribute-less — which left the SPICE
        // converter unable to recognise the device and the sign-off ripple
        // model unable to recover the operating point. This pass runs after
        // all instances exist; an attribute already present is not overwritten.
        self.stamp_entity_attributes_on_instances();

        // Phase 5: Apply semantic annotations for visualization
        if self.config.preserve_semantic_context {
            self.apply_semantic_annotations(analysis)?;
        }

        // Phase 6: Include power domain information
        if self.config.include_power_domains {
            self.include_power_domain_info(analysis)?;
        }

        // Phase 7: Include component inference results
        if self.config.include_component_inference {
            self.include_component_inference_info(analysis)?;
        }
        
        // Phase 8: Populate analysis data in netlist (unified model)
        self.populate_analysis_data(analysis)?;
        
        // Phase 9: Run simulation-driven optimization if enabled
        if self.config.enable_simulation_optimization {
            self.run_simulation_optimization(ast, analysis).await?;
        }

        // Phase 10: Run design pattern recognition
        if self.config.enable_pattern_recognition {
            info!("Starting design pattern recognition phase...");
            self.run_pattern_recognition(analysis)?;
            info!("Design pattern recognition phase completed");
        }
        
        // Phase 11: Run cross-component optimization
        if self.config.enable_cross_optimization {
            info!("Starting cross-component optimization phase...");
            self.run_cross_component_optimization(analysis)?;
            info!("Cross-component optimization phase completed");
        }
        
        // Phase 12: Run component compatibility analysis
        if self.config.enable_compatibility_analysis {
            info!("Starting component compatibility analysis phase...");
            self.run_compatibility_analysis(analysis).await?;
            info!("Component compatibility analysis phase completed");
        }
        
        // Phase 12.5: Stamp intent attributes onto instances BEFORE the DRC
        // phase — intent-aware rules (ERC022) read `intent_name` /
        // `intent_<param>` from instance attributes. The CLI's post-
        // generation stamping call remains for consumers of its own netlist
        // copies; re-stamping the same values is idempotent.
        if let Some(ft) = analysis.flow_tracker.as_ref() {
            crate::intent_attribute_stamper::stamp_intent_attributes(&mut self.netlist, ft);
        }

        // Phase 12.7: allocate reference designators. After every phase
        // that mints instances (expansion 4.5, entity attrs 4.6), before
        // DRC so ERC plugin summaries carry real designators. Instances
        // minted post-generation (CLI cap-bank sizers) get theirs from the
        // CLI re-invoking assign_refdes — idempotent, LUT-stable.
        crate::refdes_alloc::assign_refdes(
            &mut self.netlist,
            self.config.refdes_lut_path.as_deref(),
        );

        // Phase 13: Run design rule checking (DRC)
        if self.config.enable_design_rule_check {
            info!("Starting design rule checking phase...");
            self.run_design_rule_check(analysis)?;
            info!("Design rule checking phase completed");
        }
        
        // Phase 14: Run ML-based component selection optimization
        if self.config.enable_ml_selection {
            info!("Starting ML-based component selection phase...");
            self.run_ml_component_selection(analysis)?;
            info!("ML component selection phase completed");
        }
        
        // Phase 15: Run thermal simulation and analysis
        if self.config.enable_thermal_simulation {
            info!("Starting thermal simulation phase...");
            self.run_thermal_simulation(analysis)?;
            info!("Thermal simulation phase completed");
        }
        
        // Phase 16: Run cost optimization with supplier data
        if self.config.enable_cost_optimization {
            info!("Starting cost optimization phase...");
            self.run_cost_optimization(analysis).await?;
            info!("Cost optimization phase completed");
        }
        
        // Phase 17: Run EMI/EMC analysis
        if self.config.enable_emi_emc_analysis {
            info!("Starting EMI/EMC analysis phase...");
            self.run_emi_emc_analysis(analysis).await?;
            info!("EMI/EMC analysis phase completed");
        }
        
        // Phase 18: Run reliability and lifecycle analysis
        if self.config.enable_reliability_analysis {
            info!("Starting reliability and lifecycle analysis phase...");
            self.run_reliability_analysis(analysis).await?;
            info!("Reliability and lifecycle analysis phase completed");
        }
        
        // Phase 19: Run predictive analytics and machine learning integration
        if self.config.enable_predictive_analytics {
            info!("Starting predictive analytics and machine learning integration phase...");
            self.run_predictive_analysis(analysis).await?;
            info!("Predictive analytics and machine learning integration phase completed");
        }
        
        // Phase 20: Run manufacturing and assembly optimization (DFM/DFA)
        if self.config.enable_manufacturing_optimization {
            info!("Starting manufacturing and assembly optimization phase...");
            self.run_manufacturing_optimization(analysis).await?;
            info!("Manufacturing and assembly optimization phase completed");
        }

        // Populate database components for ALL netlist instances (before power symbols)
        self.populate_all_netlist_components().await?;

        // Power/ground source declarations are net-name annotations,
        // not components. The schematic-visualization layer should
        // render them as label-glyphs on the named nets, not as
        // pseudo-component instances. `populate_power_symbol_components`
        // is kept around in case the visualizer wants to opt back in
        // later but is no longer invoked on the netlist path. See
        // Phase J of the KiCad import work for the motivation.
        // self.populate_power_symbol_components(analysis).await?;

        info!("Netlist generation complete: {} modules, {} instances, {} nets, {} database components",
              self.netlist.modules.len(),
              self.netlist.instances.len(),
              self.netlist.nets.len(),
              self.component_instances.len());

        Ok(std::mem::take(&mut self.netlist))
    }

    /// Extract module hierarchy from analysis results
    fn extract_module_hierarchy(&mut self, analysis: &AnalysisResult) -> Result<()> {
        debug!("Extracting module hierarchy from analysis");

        // Extract module definitions from global scope
        for symbol in analysis.global_scope.iter() {
            if matches!(symbol.kind, bhdl_analyzer::symbol_table::SymbolKind::Entity |
                                   bhdl_analyzer::symbol_table::SymbolKind::Board) {
                let module_kind = if matches!(symbol.kind, bhdl_analyzer::symbol_table::SymbolKind::Board) {
                    ModuleKind::Board
                } else {
                    ModuleKind::Module
                };

                let module_id = self.netlist.add_module(
                    symbol.name.clone(),
                    module_kind
                );

                self.ast_to_module.insert(symbol.name.clone(), module_id);

                if module_kind == ModuleKind::Board {
                    self.netlist.top_level_module = Some(module_id);
                }

                debug!("Created module '{}' with kind {:?}", symbol.name, module_kind);
            }
        }

        Ok(())
    }

    /// Generate instances with semantic context preservation
    fn generate_instances_with_semantics(&mut self, analysis: &AnalysisResult) -> Result<()> {
        debug!("Generating instances with semantic context");

        // Track component counts by type for proper reference designators
        let mut component_counts: HashMap<String, usize> = HashMap::new();

        // Extract component inference results for semantic context
        if self.config.include_component_inference {
            let component_context = &analysis.component_inference;
            
            // Create instances based on inferred components
            debug!("Processing {} inferred components", component_context.inferred_components.len());
            for component_suggestion in &component_context.inferred_components {
                let component_type = &component_suggestion.component_type;
                debug!("Processing inferred component: {}", component_type);
                
                // Check if this is an interface type
                if self.is_interface_type(component_type, analysis) {
                    // Process as interface instance
                    let instance_name = component_suggestion.instance_name.as_ref()
                        .unwrap_or(&component_type).clone();
                    
                    self.process_interface_instance(&instance_name, component_type, analysis)?;
                    debug!("Processed interface instance '{}' of type '{}'", instance_name, component_type);
                } else {
                    // Normal component processing
                    let module_kind = self.map_component_type_to_module_kind(component_type);
                    
                    // Generate proper reference designator
                    let instance_name = if let Some(ref name) = component_suggestion.instance_name {
                        name.clone()
                    } else {
                        // Generate proper reference designator using unified type mapper
                        let refdes_prefix = self.type_mapper.get_refdes_prefix(component_type);
                        
                        // Increment count for this component type
                        let count = component_counts.entry(refdes_prefix.clone()).or_insert(0);
                        *count += 1;
                        
                        format!("{}{}", refdes_prefix, count)
                    };
                    
                    // Create module definition for this component type
                    let module_id = self.netlist.add_module(
                        component_type.clone(),
                        module_kind
                    );
                    
                    // Add pins to the module based on component type
                    debug!("About to call add_pins_for_component for {} ({})", instance_name, component_type);
                    self.add_pins_for_component(&instance_name, component_type, module_id)?;
                    
                    // Create instance of this component
                    let instance_id = self.netlist.add_instance(
                        instance_name.clone(),
                        module_id
                    ).expect("Failed to add instance");

                    // Propagate module-level attributes (including component_class) to instance
                    if let Some(module) = self.netlist.modules.get(module_id) {
                        let module_attrs = module.attributes.clone();
                        if let Some(instance) = self.netlist.instances.get_mut(instance_id) {
                            for (key, value) in &module_attrs {
                                if !instance.attributes.contains_key(key) {
                                    instance.attributes.insert(key.clone(), value.clone());
                                }
                            }
                        }
                    }

                    // Create pin instances for this component instance
                    self.netlist.create_pin_instances(instance_id)
                        .map_err(|e| anyhow::anyhow!("Failed to create pin instances: {}", e))?;

                    // Transfer component parameters from analyzer to instance attributes
                    log::debug!("Calling populate_instance_attributes for instance '{}' (id: {:?})", instance_name, instance_id);
                    populate_instance_attributes(&mut self.netlist, instance_id, &instance_name, analysis);

                    self.ast_to_module.insert(component_type.clone(), module_id);
                    self.ast_to_instance.insert(instance_name.clone(), instance_id);

                    debug!("Created instance '{}' of type '{}' with semantic kind {:?}",
                           instance_name, component_type, module_kind);
                }
            }
        }

        Ok(())
    }

    /// Generate component instances from AST using database component mapper
    async fn generate_database_component_instances(&mut self, analysis: &AnalysisResult, ast: Option<&SourceFile>) -> Result<()> {
        println!("🔧 MAIN SYNTHESIZER: Entering generate_database_component_instances");
        info!("ENTERING generate_database_component_instances");
        debug!("Generating component instances from AST using database mapper");
        
        // We need to process the actual component instances from the AST
        // These are the components like "U1: TPS54331()" that appear in the BHDL source
        
        // For now, we need to extract component instances from the analysis
        // This is where the actual BHDL component instantiations should be processed
        
        // Extract component instances from both global scope and definition scopes
        debug!("Total symbols in global scope: {}", analysis.global_scope.get_symbols().len());
        debug!("Total definition scopes: {}", analysis.definition_scopes.len());
        
        // Check both global scope and definition scopes  
        let mut all_symbols = analysis.global_scope.get_symbols().clone();
        
        // Add symbols from all definition scopes
        for (_node_ptr, scope) in bhdl_analyzer::definition_scopes_sorted(&analysis.definition_scopes) {
            for (name, symbol) in scope.get_symbols() {
                all_symbols.insert(name.clone(), symbol.clone());
            }
        }
        
        // Iterate name-sorted: all_symbols is a HashMap, and this loop's order
        // decides module/instance/pin creation order — hash order here made
        // the whole downstream netlist (and every report derived from it)
        // nondeterministic run-to-run.
        let mut sorted_symbols: Vec<_> = all_symbols.iter().collect();
        sorted_symbols.sort_by_key(|(name, _)| name.as_str());
        for (name, symbol) in sorted_symbols {
            if matches!(symbol.kind, bhdl_analyzer::symbol_table::SymbolKind::Instance) {
                debug!("Processing component instance: {} of kind {:?}", name, symbol.kind);

                // Extract component type from the symbol's instance_type_name
                if let Some(ref type_name) = symbol.instance_type_name {
                    debug!("Component {} is of type: {}", name, type_name);

                    // Interface instances (`i2c_bus: I2C();`) are synthesized
                    // as signal nets by interface_synthesis.rs, never as
                    // component instances — creating one here puts a phantom
                    // part in the netlist. Mirrors the is_interface_type skip
                    // in generate_instances_with_semantics.
                    if self.is_interface_type(type_name, analysis) {
                        debug!("Skipping interface instance {} of type {}", name, type_name);
                        continue;
                    }

                    // Create module for the component type if it doesn't exist
                    let module_id = self.get_or_create_module(type_name, ModuleKind::Component)?;
                    
                    // Create instance of the component using correct API
                    let instance_id = self.netlist.add_instance(name.clone(), module_id);
                    
                    if let Some(instance_id) = instance_id {
                        debug!("Created component instance: {} -> {:?}", name, instance_id);

                        // Register the instance by its board handle so later
                        // phases (power-domain distribution lowering, Phase
                        // 2.7) can resolve it — the semantic path does this
                        // but this path historically didn't, leaving every
                        // power_domain distribution{} pin floating.
                        self.ast_to_instance.insert(name.clone(), instance_id);

                        // Add pins for the component based on database or default pins.
                        // `get_or_create_module` hands back a *shared* module per
                        // component type, so only the first instance of a given
                        // type should populate the pin definitions — otherwise the
                        // pins (and, below, the pin instances) get duplicated.
                        let module_needs_pins = self.netlist.modules.get(module_id)
                            .map(|m| m.pins.is_empty())
                            .unwrap_or(false);
                        if module_needs_pins {
                            if let Err(e) = self.add_pins_for_component(name, type_name, module_id) {
                                warn!("Failed to add pins for component {}: {}", name, e);
                            }
                        }

                        // Materialize pin instances for this component instance.
                        // `add_pins_for_component` only populates the module's pin
                        // *definitions*; without this step the instance has zero
                        // pin instances, so later phases (connectivity resolution,
                        // expansion-block interpretation) cannot resolve any pin on
                        // it. This mirrors `generate_instances_with_semantics`.
                        // Skip module type definitions (name == type_name) — those
                        // are library templates, not real circuit instances.
                        if name != type_name {
                            if let Err(e) = self.netlist.create_pin_instances(instance_id) {
                                warn!("Failed to create pin instances for component {}: {}", name, e);
                            }
                        }

                        // Try to match this instance to a database component for visualization
                        // BUT: Skip module type definitions (where name == type_name like "Cap" == "Cap")
                        // These are library types, not actual component instances
                        if name != type_name {
                            if let Some(ref mut mapper) = self.database_mapper {
                                match mapper.create_component_instance(name, type_name).await {
                                    Ok(component_instance) => {
                                        debug!("Matched component {} to database component: {}", name, component_instance.component_name);
                                        self.component_instances.push(component_instance);
                                    }
                                    Err(e) => {
                                        debug!("Could not match component {} (type: {}) to database: {}", name, type_name, e);
                                    }
                                }
                            }
                        } else {
                            debug!("Skipping database component for module type definition: {}", name);
                        }
                    } else {
                        warn!("Failed to create instance for component: {}", name);
                    }
                } else {
                    debug!("Component {} has no type name, skipping", name);
                }
            }
        }
        
        Ok(())
    }

    // REMOVED: is_power_management_ic() - We should not hardcode component types
    // All component knowledge should come from BHDL files
    /// Find or create a net with the given name and class.
    ///
    /// `pub(crate)` because `power_supply_synthesis` (split out
    /// 2026-05-26) needs it when wiring up supporting components.
    pub(crate) fn find_or_create_net(&mut self, name: &str, net_class: NetClass) -> NetId {
        // First check if the net already exists
        for (id, net) in &self.netlist.nets {
            if let Some(ref net_name) = net.name {
                if net_name == name {
                    return id;
                }
            }
        }
        
        // Create new net
        self.netlist.add_net_with_class(Some(name.to_string()), net_class)
    }
    
    /// Get or create a pin for a module. `pub(crate)` for
    /// `power_supply_synthesis` (split 2026-05-26).
    pub(crate) fn get_or_create_pin(&mut self, module_id: ModuleId, pin_name: &str, direction: PinDirection) -> PinId {
        // Check if pin already exists for this module
        if let Some(module) = self.netlist.modules.get(module_id) {
            for &pin_id in &module.pins {
                if let Some(pin) = self.netlist.pins.get(pin_id) {
                    if pin.name == pin_name {
                        return pin_id;
                    }
                }
            }
        }
        
        // Create new pin
        let pin_id = self.netlist.pins.insert_with_key(|id| Pin {
            id,
            name: pin_name.to_string(),
            direction,
            pin_type: PinType::Signal,
            module: module_id,
            description: None,
            is_virtual: false,
        });
        
        // Add pin to module
        if let Some(module) = self.netlist.modules.get_mut(module_id) {
            module.pins.push(pin_id);
        }
        
        pin_id
    }
    
    /// Get or create a module definition for a given component
    /// type. `pub(crate)` for `power_supply_synthesis` (split
    /// 2026-05-26).
    pub(crate) fn get_or_create_module(&mut self, component_type: &str, kind: ModuleKind) -> Result<ModuleId> {
        // Check if module already exists
        for (id, module) in &self.netlist.modules {
            if module.name == component_type {
                return Ok(id);
            }
        }
        
        // Create new module using correct API
        let module_id = self.netlist.add_module(component_type.to_string(), kind);
        
        debug!("Created new module: {} -> {:?}", component_type, module_id);
        Ok(module_id)
    }

    /// Map component type string to semantic ModuleKind for visualization
    fn map_component_type_to_module_kind(&self, component_type: &str) -> ModuleKind {
        let component_lower = component_type.to_lowercase();
        
        if component_lower.contains("regulator") || component_lower.contains("ldo") {
            ModuleKind::Component // Power regulator will be detected by semantic analyzer
        } else if component_lower.contains("opamp") || component_lower.contains("amplifier") {
            ModuleKind::Component // Op-amp will be detected by semantic analyzer
        } else if component_lower.contains("mcu") || component_lower.contains("microcontroller") {
            ModuleKind::Component // MCU will be detected by semantic analyzer
        } else if component_lower.contains("resistor") || component_lower.contains("capacitor") {
            ModuleKind::PhysicalComponent
        } else {
            ModuleKind::Component
        }
    }

    /// Extract connectivity from AST and create nets
    /// Recover the refdes of an inline-flow component instantiation
    /// (`… -> refdes: Entity(args).pin`). The parser wraps only the entity
    /// IDENT in the `COMPONENT_INST` node; the refdes is the `IDENT :`
    /// binding immediately preceding it (siblings in the same parent).
    /// Walks backward over siblings, skipping trivia, expecting `:` then
    /// the refdes IDENT/NUMBER. Returns `None` for an anonymous inline use
    /// (no `refdes:` binding) or any other preceding shape.
    fn preceding_flow_refdes(
        node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
    ) -> Option<String> {
        use bhdl_ast::SyntaxKind;
        let mut el = node.prev_sibling_or_token();
        let mut seen_colon = false;
        while let Some(e) = el {
            match e.kind() {
                SyntaxKind::WHITESPACE | SyntaxKind::COMMENT => {}
                SyntaxKind::COLON if !seen_colon => seen_colon = true,
                // The refdes in a flow part is wrapped in an `IDENT_REF`
                // node (`… -> buck : Entity(...)`), not a bare token.
                SyntaxKind::IDENT_REF if seen_colon => {
                    return e.as_node().and_then(|n| {
                        n.children_with_tokens()
                            .filter_map(|x| x.into_token())
                            .find(|t| t.kind() == SyntaxKind::IDENT)
                            .map(|t| t.text().to_string())
                    });
                }
                // Defensive: also accept a bare IDENT/NUMBER token refdes.
                SyntaxKind::IDENT | SyntaxKind::NUMBER if seen_colon => {
                    return e.as_token().map(|t| t.text().to_string());
                }
                _ => return None,
            }
            el = e.prev_sibling_or_token();
        }
        None
    }

    /// Walk every `COMPONENT_INST` node in the AST and merge its
    /// constructor arguments (`name=value` pairs inside the `(...)`)
    /// into the matching netlist instance's `attributes` map.
    ///
    /// Why: multiple instance-creation paths (database mapper,
    /// hierarchical entity walk, component-inference fallback) each
    /// own part of the attribute story but none consistently
    /// transfers the user's constructor args onto the netlist
    /// instance. The `design { }` evaluator at Phase 4.5 then can't
    /// see `self.v_out` for entities like LM317 that take a
    /// runtime arg. A single unified merge pass right before
    /// expansion guarantees those args are visible.
    fn stamp_constructor_args_on_instances(
        &mut self,
        ast: &SourceFile,
        alias_specs: &[bhdl_analyzer::passes::monomorphization::AliasSpecialization],
        entity_param_names: &HashMap<String, Vec<String>>,
    ) {
        // Helper: strip surrounding double-quotes from a string-literal
        // constructor arg value. The AST text() returns the literal
        // source token including its quotes (`"STM32F103C8T6"`), but
        // downstream consumers (BOM walker, KiCad export, comparators)
        // expect the bare string. Numeric / unit / identifier values
        // pass through unchanged.
        fn unquote_string_literal(s: &str) -> String {
            let s = s.trim();
            if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
                s[1..s.len() - 1].to_string()
            } else {
                s.to_string()
            }
        }
        use bhdl_ast::SyntaxKind;
        use bhdl_ast::common::ComponentInst;
        use rowan::ast::AstNode;

        // Build an index of entity_name → list of (param_name, default_value_text)
        // from the entire AST + imported entities. Used in the second
        // pass below to stamp DEFAULTS on instances that didn't pass
        // an explicit override (task #90).
        let mut entity_param_defaults: HashMap<String, Vec<(String, String)>> = HashMap::new();

        // Helper: extract (param_name, default_value) pairs from an
        // entity-def AST node. Returned as an owned Vec — caller
        // decides where to store.
        fn extract_param_defaults(
            entity_node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
        ) -> Vec<(String, String)> {
            let mut defaults: Vec<(String, String)> = Vec::new();
            for param_node in entity_node.children() {
                if param_node.kind() != SyntaxKind::PARAM_LIST { continue; }
                for entity_param in param_node.children() {
                    if entity_param.kind() != SyntaxKind::PARAM_DECL { continue; }
                    let pname = entity_param.children_with_tokens()
                        .filter_map(|el| el.into_token())
                        .find(|t| t.kind() == SyntaxKind::IDENT)
                        .map(|t| t.text().to_string());
                    let Some(pname) = pname else { continue; };
                    // Default value: walk children for text after EQ.
                    let mut saw_eq = false;
                    let mut default_text = String::new();
                    for el in entity_param.children_with_tokens() {
                        match el {
                            rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::EQ => {
                                saw_eq = true;
                            }
                            rowan::NodeOrToken::Node(n) if saw_eq => {
                                default_text.push_str(n.text().to_string().trim());
                            }
                            rowan::NodeOrToken::Token(t) if saw_eq
                                && t.kind() != SyntaxKind::WHITESPACE
                                && t.kind() != SyntaxKind::COMMA
                                && t.kind() != SyntaxKind::R_PAREN =>
                            {
                                default_text.push_str(t.text());
                            }
                            _ => {}
                        }
                    }
                    let default_text = unquote_string_literal(default_text.trim());
                    if !default_text.is_empty() {
                        defaults.push((pname, default_text));
                    }
                }
            }
            defaults
        }

        // (a) Local entity defs in the same file.
        for node in ast.syntax().descendants() {
            if node.kind() != SyntaxKind::ENTITY_DEF { continue; }
            let name = node.children_with_tokens()
                .filter_map(|el| el.into_token())
                .find(|t| t.kind() == SyntaxKind::IDENT)
                .map(|t| t.text().to_string());
            let Some(entity_name) = name else { continue; };
            let defaults = extract_param_defaults(&node);
            if !defaults.is_empty() {
                entity_param_defaults.insert(entity_name, defaults);
            }
        }

        let mut stamped_explicit = 0usize;
        let mut stamped_defaults = 0usize;
        for node in ast.syntax().descendants() {
            if node.kind() != SyntaxKind::COMPONENT_INST { continue; }
            let Some(comp_inst) = ComponentInst::cast(node) else { continue; };

            // The user-supplied refdes (`U1` in `U1: LM317(...)`).
            // The entity-type IDENT is the second.
            let idents: Vec<String> = comp_inst.syntax()
                .children_with_tokens()
                .filter_map(|el| el.into_token())
                .filter(|t| t.kind() == SyntaxKind::IDENT)
                .map(|t| t.text().to_string())
                .collect();
            if idents.is_empty() { continue; }
            // Two COMPONENT_INST shapes carry different ident layouts:
            //   - Standalone decl `refdes: Entity(args);` — the node holds
            //     BOTH idents (refdes, entity).
            //   - Inline-flow `… -> refdes: Entity(args).pin` — the parser
            //     (bhdl-parser expressions.rs) wraps only the ENTITY ident;
            //     the refdes is the `IDENT :` binding immediately preceding
            //     the node. Without recovering it, `idents[0]` is the
            //     entity, the instance lookup by name fails, and the
            //     instance never gets its constructor defaults stamped — so
            //     its `design { }` block fails (`self.f_sw = 'f_sw'`) and
            //     emits placeholder values. (Bug A.)
            // Discriminate the two shapes by whether the COMPONENT_INST
            // node itself contains the `:` binding (verified against the
            // parser's AST):
            //   - Standalone `refdes: Entity(...)` — COLON is INSIDE, so
            //     idents = [refdes, entity].
            //   - Inline-flow `… -> refdes: Entity(...).pin` — no COLON
            //     inside; idents = [entity, pin], and the refdes is the
            //     preceding `IDENT_REF :` sibling.
            // Ident *count* can't distinguish them (both are 2), so key off
            // the COLON.
            let has_inner_colon = comp_inst
                .syntax()
                .children_with_tokens()
                .any(|e| e.kind() == SyntaxKind::COLON);
            let (inst_name, entity_type) = if has_inner_colon {
                (idents[0].clone(), idents.get(1).cloned())
            } else {
                match Self::preceding_flow_refdes(comp_inst.syntax()) {
                    Some(refdes) => (refdes, idents.first().cloned()),
                    // Anonymous inline use (no `refdes:` binding, e.g.
                    // `VIN -> Cap(10µF).1`) — nothing to stamp by name.
                    None => continue,
                }
            };
            let inst_name = &inst_name;

            // Find the matching netlist instance by name.
            let inst_id = self.netlist.instances.iter()
                .find(|(_, inst)| inst.name == *inst_name)
                .map(|(id, _)| id);
            let Some(inst_id) = inst_id else { continue; };

            // Resolve constructor-argument aliases (`alias TPS54331_3V3 =
            // TPS54331(3.3V)`): if the instance's type is such an alias,
            // the *effective* entity for default lookup is the alias
            // target, and the alias's positional args bind to the
            // target's params. The alias args sit between board-explicit
            // overrides (highest) and entity defaults (lowest), so a
            // board can still override an SKU alias's value at the call
            // site. `alias_specs` carries both generic `<…>` and
            // constructor `(…)` aliases uniformly (the parser records both
            // in the same TYPE_ARGS node); for a generic alias the target
            // is a generic entity and this positional bind is harmless
            // (its params are bound by monomorphization instead).
            let alias = entity_type.as_ref()
                .and_then(|et| alias_specs.iter().find(|a| &a.alias_name == et));
            let effective_type = alias
                .map(|a| a.target_entity.clone())
                .or_else(|| entity_type.clone());

            // (1) Stamp explicit constructor args from the `PARAM_LIST`.
            // Constructor args (`v_out=5V`, `tolerance=1%`, …) live
            // under a `PARAM_LIST` node — not `PARAM_ASSIGN_BLOCK`,
            // which is a different grammar shape.
            if let Some(param_list) = comp_inst.param_list() {
                for assign in param_list.params() {
                    if let (Some(name_tok), Some(value_expr)) =
                        (bhdl_ast::HasName::name(&assign), assign.value())
                    {
                        let key = name_tok.text().to_string();
                        let val = unquote_string_literal(
                            value_expr.syntax().text().to_string().trim());
                        if let Some(inst) = self.netlist.instances.get_mut(inst_id) {
                            inst.attributes.entry(key).or_insert(val);
                            stamped_explicit += 1;
                        }
                    }
                }
            }

            // (1a) Inline-flow instances (`… -> r: Res(x, k=v).pin`) carry
            // their constructor args under PARAM_ASSIGN_BLOCK, not
            // PARAM_LIST (parser `parse_component_parameters`). Without this,
            // named-arg overrides on the flow-style instantiation that
            // recipes/expansions use silently vanished. Stamp the *named*
            // assignments here too — `HasName::name()` only matches a direct
            // IDENT token, so positional args (the value, an IDENT_REF/expr
            // child node) are correctly skipped and still bind via component
            // inference. `or_insert` keeps any PARAM_LIST stamp above.
            if let Some(block) = comp_inst.param_assign_block() {
                for assign in block.assignments() {
                    if let (Some(name_tok), Some(value_expr)) =
                        (bhdl_ast::HasName::name(&assign), assign.value())
                    {
                        let key = name_tok.text().to_string();
                        let val = unquote_string_literal(
                            value_expr.syntax().text().to_string().trim());
                        if let Some(inst) = self.netlist.instances.get_mut(inst_id) {
                            inst.attributes.entry(key).or_insert(val);
                            stamped_explicit += 1;
                        }
                    }
                }
            }

            // (1b) Stamp constructor-arg-alias positional args, bound to
            // the target entity's params by position. `or_insert` keeps
            // any board-explicit override stamped in (1).
            if let Some(a) = alias {
                if let Some(pnames) = entity_param_names.get(&a.target_entity) {
                    for (i, arg) in a.type_arg_texts.iter().enumerate() {
                        let Some(pname) = pnames.get(i) else { break; };
                        let val = unquote_string_literal(arg.trim());
                        if let Some(inst) = self.netlist.instances.get_mut(inst_id) {
                            inst.attributes.entry(pname.clone()).or_insert(val);
                        }
                    }
                }
            }

            // (2) Task #90: stamp entity-parameter DEFAULTS for any
            // params the user didn't explicitly override.
            //
            // Source for defaults: same-file entity defs already
            // collected above; for imported entities, do an on-demand
            // collection from the imported AST. For an alias instance the
            // effective entity is the alias target, not the alias name.
            if let Some(ref etype) = effective_type {
                // Lazily collect imported entity's defaults if not
                // already in the index.
                if !entity_param_defaults.contains_key(etype) {
                    if let Some(ref pp) = self.import_preprocessor {
                        if let Some(imported) = pp.get_imported_entity(etype) {
                            let defaults = extract_param_defaults(imported.syntax());
                            if !defaults.is_empty() {
                                entity_param_defaults.insert(etype.clone(), defaults);
                            }
                        }
                    }
                }
                if let Some(defaults) = entity_param_defaults.get(etype).cloned() {
                    if let Some(inst) = self.netlist.instances.get_mut(inst_id) {
                        for (pname, pdefault) in defaults {
                            // entry().or_insert() skips when the user
                            // already passed an override (the
                            // explicit-args pass above stamped them).
                            let pre_exists = inst.attributes.contains_key(&pname);
                            inst.attributes.entry(pname.clone()).or_insert(pdefault);
                            if !pre_exists {
                                stamped_defaults += 1;
                            }
                        }
                    }
                }
            }
        }
        info!("Phase 4.4: stamped {} explicit arg(s) + {} default(s) onto instances",
              stamped_explicit, stamped_defaults);
    }

    fn extract_connectivity_from_ast(&mut self, ast: &SourceFile, analysis: &AnalysisResult) -> Result<()> {
        info!("Extracting connectivity from AST");
        
        // Always create power nets first
        self.create_power_nets(analysis)?;

        // Use hierarchical connectivity extraction (AST-based)
        info!("Using hierarchical connectivity extraction");
        hierarchical_connectivity::extract_hierarchical_connectivity(ast, analysis, &mut self.netlist, self.import_preprocessor.as_ref())?;

        Ok(())
    }
    
    /// Limited connectivity extraction without AST
    fn extract_connectivity_limited(&mut self, analysis: &AnalysisResult) -> Result<()> {
        warn!("Limited connectivity extraction - no AST available");
        
        // Create power nets only
        self.create_power_nets(analysis)?;
        
        Ok(())
    }
    
    /// Create power nets from analysis
    fn create_power_nets(&mut self, analysis: &AnalysisResult) -> Result<()> {
        if self.config.include_power_domains {
            let power_context = &analysis.power_analysis;

            // Create one Net per source-declared power/ground domain.
            //
            // `power VCC_5V = 5V;` / `ground GND;` source declarations
            // are net-name annotations: they assign a name + voltage
            // type to a net, they do NOT materialise a component.
            // Power physically enters the board through a connector
            // pin (USB VBUS, barrel jack tip) or is produced by a
            // regulator pin (LDO VOUT, buck VOUT); those connector/
            // regulator entities own their own pins. The named net is
            // just the wire those pins terminate on. See discussion
            // documented in Phase J of the KiCad import work.
            // Name-sorted: domains is a HashMap, and this loop fixes the
            // power nets' creation (NetId) order for the whole netlist.
            let mut domains_sorted: Vec<_> = power_context.domains.iter().collect();
            domains_sorted.sort_by_key(|(name, _)| name.as_str());
            for (domain_name, domain_info) in domains_sorted {
                let net_class = if domain_name.contains("GND") || domain_info.voltage == 0.0 {
                    NetClass::Ground
                } else {
                    // Carry the source-declared per-rail load budget (`@ I`) onto
                    // the net. `None` when undeclared — sign-off then reports the
                    // i_out-dependent checks UNCHECKED rather than substituting the
                    // regulator's rated output current as a proxy (Real-Data).
                    NetClass::Power { voltage: domain_info.voltage, current: domain_info.declared_current }
                };

                let net_id = self.find_or_create_net(domain_name, net_class.clone());
                self.ast_to_net.insert(domain_name.clone(), net_id);

                debug!("Created power net '{}' with voltage {:?} and class {:?}",
                       domain_name, domain_info.voltage, net_class);
            }
        }
        Ok(())
    }

    /// Materialise the board's boundary ports as netlist Port objects.
    ///
    /// Each BoardPortInfo (explicit `port` decl or desugared power/ground
    /// decl) becomes a Port on the top-level module with `net` pointing at
    /// the named boundary net. The net itself is created exactly as
    /// `create_power_nets` would (same NetClass) — the Port is the honest
    /// boundary object layered on top, not a second lowering path.
    fn create_board_ports(&mut self, analysis: &AnalysisResult) -> Result<()> {
        use bhdl_analyzer::power_analysis::{BoardPortDir, BoardPortKind};

        let Some(top_module) = self.netlist.top_level_module else {
            // No board (library file) — nothing has a boundary.
            return Ok(());
        };

        for port in &analysis.power_analysis.board_ports {
            let net_class = match port.kind {
                BoardPortKind::Power => NetClass::Power {
                    voltage: port.voltage.unwrap_or(0.0),
                    current: port.current,
                },
                BoardPortKind::Ground => NetClass::Ground,
                BoardPortKind::Signal => NetClass::Signal,
            };
            let net_id = self.find_or_create_net(&port.name, net_class);
            self.ast_to_net.entry(port.name.clone()).or_insert(net_id);

            let direction = match port.direction {
                BoardPortDir::In => PortDirection::Input,
                BoardPortDir::Out => PortDirection::Output,
                BoardPortDir::InOut => PortDirection::InOut,
            };
            let Some(port_id) =
                self.netlist.add_port(top_module, port.name.clone(), direction, None)
            else {
                warn!("Could not add board port '{}' to top-level module", port.name);
                continue;
            };
            if let Some(p) = self.netlist.ports.get_mut(port_id) {
                p.net = Some(net_id);
            }
            debug!(
                "Created board port '{}' ({:?} {:?}, explicit={}) -> net {:?}",
                port.name, port.kind, port.direction, port.explicit, net_id
            );
        }
        Ok(())
    }

    /// Populate database components for all netlist instances
    async fn populate_all_netlist_components(&mut self) -> Result<()> {
        if self.database_mapper.is_none() {
            return Ok(()); // No database, skip
        }

        // Track which instances already have database components
        let existing_instances: std::collections::HashSet<String> =
            self.component_instances.iter()
                .map(|c| c.instance_name.clone())
                .collect();

        debug!("Populating database components for netlist instances (already have {} components)", existing_instances.len());

        // Iterate over all netlist instances
        for (instance_id, instance) in &self.netlist.instances {
            // Skip if already have a database component for this instance
            if existing_instances.contains(&instance.name) {
                debug!("Skipping instance '{}' - already has database component", instance.name);
                continue;
            }

            // Get module definition to find component type
            let module_def = match self.netlist.get_module(instance.definition) {
                Some(def) => def,
                None => {
                    debug!("WARNING: Module definition not found for instance {}, skipping", instance.name);
                    continue;
                }
            };

            // Skip module types where instance name == module name (like "Cap" == "Cap")
            if instance.name == module_def.name {
                debug!("Skipping module type definition: {}", instance.name);
                continue;
            }

            // Try to create database component instance
            if let Some(ref mut mapper) = self.database_mapper {
                match mapper.create_component_instance(&instance.name, &module_def.name).await {
                    Ok(component_instance) => {
                        debug!("Created database component for instance '{}' (type: {})", instance.name, module_def.name);
                        self.component_instances.push(component_instance);
                    }
                    Err(e) => {
                        debug!("Could not create database component for instance '{}' (type: {}): {}",
                               instance.name, module_def.name, e);
                    }
                }
            }
        }

        info!("Populated database components: {} total", self.component_instances.len());
        Ok(())
    }

    /// Populate power symbol database entries for visualization
    async fn populate_power_symbol_components(&mut self, analysis: &AnalysisResult) -> Result<()> {
        if self.database_mapper.is_none() {
            return Ok(()); // No database, skip
        }

        let power_context = &analysis.power_analysis;

        // Name-sorted for deterministic component_instances order.
        let mut domains_sorted: Vec<_> = power_context.domains.iter().collect();
        domains_sorted.sort_by_key(|(name, _)| name.as_str());
        for (domain_name, domain_info) in domains_sorted {
            // Determine symbol name
            let symbol_name = if domain_name.contains("GND") || domain_info.voltage == 0.0 {
                "GND".to_string()
            } else {
                match domain_info.voltage.round() as i32 {
                    5 => "+5V".to_string(),
                    12 => "+12V".to_string(),
                    3 => "+3V3".to_string(),
                    24 => "+24V".to_string(),
                    _ => format!("+{}V", domain_info.voltage.round() as i32),
                }
            };

            // Create database component instance for this power symbol using direct database lookup
            if let Some(ref mut mapper) = self.database_mapper {
                match mapper.create_component_instance_direct(domain_name, &symbol_name).await {
                    Ok(component_instance) => {
                        info!("Added power symbol '{}' ({}) to component instances", domain_name, symbol_name);
                        self.component_instances.push(component_instance);
                    }
                    Err(e) => {
                        debug!("Could not add power symbol {} ({}): {}", domain_name, symbol_name, e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Apply semantic annotations for intelligent visualization
    fn apply_semantic_annotations(&mut self, _analysis: &AnalysisResult) -> Result<()> {
        debug!("Applying semantic annotations for visualization");

        // The semantic context is preserved through:
        // 1. ModuleKind enum values that map to CircuitPattern detection
        // 2. Meaningful instance and net names
        // 3. Power domain information
        // 4. Component inference results

        // The visualizer's SemanticAnalyzer will detect patterns like:
        // - PowerRegulator: instances with "regulator" in module name + connected caps
        // - OpAmpStage: instances with "opamp" in module name + surrounding components  
        // - MicrocontrollerCore: instances with "mcu" in module name + power/crystal components
        // - PowerDistribution: nets with power-related names (VCC, VDD, GND)

        info!("Semantic annotations applied - visualizer can now detect circuit patterns");
        Ok(())
    }

    /// Include power domain information from analysis
    /// Process power domain expansion results (Phase 1: Scalability)
    fn process_power_domain_expansion(&mut self, analysis: &AnalysisResult) -> Result<()> {
        info!("Processing power domain expansion (Phase 2: Scalability Integration)");

        let expansion = &analysis.power_domain_expansion;

        // Determine the board ground net name. Exactly one `ground X;`
        // declaration makes the choice unambiguous; otherwise fall back to
        // "GND" for decoupling caps and skip the automatic load ground tie.
        let unambiguous_ground = expansion.ground_nets.len() == 1;
        let gnd_name = expansion.ground_nets.first().map(|s| s.as_str()).unwrap_or("GND");
        if expansion.ground_nets.len() > 1 {
            warn!("Multiple ground nets declared ({}); load ground pins will not be tied automatically",
                  expansion.ground_nets.join(", "));
        }

        // Get or create the ground net for capacitor and load connections
        let gnd_net_id = if let Some(&net_id) = self.ast_to_net.get(gnd_name) {
            net_id
        } else {
            let net_id = self.netlist.add_net_with_class(
                Some(gnd_name.to_string()),
                bhdl_netlist::types::NetClass::Ground
            );
            self.ast_to_net.insert(gnd_name.to_string(), net_id);
            net_id
        };

        // Lower sources{} entries first (the rail drivers), then the
        // distribution{} entries (the loads). Both connect a named instance
        // pin to the domain rail net; unresolvable entries are hard
        // warnings — never silently dropped (Real-Data policy).
        let mut connected_load_instances: Vec<bhdl_netlist::types::InstanceId> = Vec::new();
        let all_connections = expansion.source_connections.iter().map(|c| (c, true))
            .chain(expansion.connections.iter().map(|c| (c, false)));
        for (connection, is_source) in all_connections {
            info!("  Power {}: @{} -> {}.{}",
                  if is_source { "source" } else { "connection" },
                  connection.source_net, connection.component, connection.pin);

            // Get or create the rail net, using the parsed domain spec
            let source_net_id = if let Some(&net_id) = self.ast_to_net.get(&connection.source_net) {
                net_id
            } else {
                let rail = expansion.rails.iter().find(|r| r.name == connection.source_net);
                let net_id = self.netlist.add_net_with_class(
                    Some(connection.source_net.clone()),
                    bhdl_netlist::types::NetClass::Power {
                        voltage: rail.and_then(|r| r.voltage).unwrap_or(3.3),
                        current: rail.and_then(|r| r.current),
                    }
                );
                self.ast_to_net.insert(connection.source_net.clone(), net_id);
                net_id
            };

            // Resolve the component instance: prefer the handle map, fall
            // back to a name scan (instance-creation paths differ in which
            // maps they populate).
            let instance_id = self.ast_to_instance.get(&connection.component).copied()
                .or_else(|| self.netlist.instances.iter()
                    .find(|(_, inst)| inst.name == connection.component)
                    .map(|(id, _)| id));

            let Some(instance_id) = instance_id else {
                warn!("Power domain @{}: unresolved entry '{}.{}' — no instance named '{}'; pin left unconnected",
                      connection.source_net, connection.component, connection.pin, connection.component);
                continue;
            };

            let Some(pin_inst_id) = self.netlist.find_pin_instance(instance_id, &connection.pin) else {
                warn!("Power domain @{}: unresolved entry '{}.{}' — instance '{}' has no pin '{}'; pin left unconnected",
                      connection.source_net, connection.component, connection.pin,
                      connection.component, connection.pin);
                continue;
            };

            if let Err(e) = self.netlist.connect(source_net_id, ConnectionPoint::PinInstance(pin_inst_id)) {
                warn!("Power domain @{}: failed to connect {}.{}: {}",
                      connection.source_net, connection.component, connection.pin, e);
            } else {
                debug!("  Connected: @{} -> {}.{}", connection.source_net, connection.component, connection.pin);
                if !is_source && !connected_load_instances.contains(&instance_id) {
                    connected_load_instances.push(instance_id);
                }
            }
        }

        // Tie ground pins of connected loads to the board ground net when
        // the choice is unambiguous (exactly one `ground` declaration).
        // Only untied pins are touched — explicit wiring wins.
        if unambiguous_ground {
            for instance_id in connected_load_instances {
                let ground_pin_names: Vec<String> = self.netlist.instances.get(instance_id)
                    .and_then(|inst| self.netlist.modules.get(inst.definition))
                    .map(|module| module.pins.iter()
                        .filter_map(|&pin_id| self.netlist.pins.get(pin_id))
                        .filter(|pin| pin.pin_type == bhdl_netlist::types::PinType::Ground)
                        .map(|pin| pin.name.clone())
                        .collect())
                    .unwrap_or_default();

                for pin_name in ground_pin_names {
                    let Some(pin_inst_id) = self.netlist.find_pin_instance(instance_id, &pin_name) else { continue };
                    let already_tied = self.netlist.pin_instances.get(pin_inst_id)
                        .map(|pi| pi.net.is_some())
                        .unwrap_or(true);
                    if already_tied {
                        continue;
                    }
                    if let Err(e) = self.netlist.connect(gnd_net_id, ConnectionPoint::PinInstance(pin_inst_id)) {
                        warn!("Power domain ground tie: failed to connect ground pin '{}': {}", pin_name, e);
                    } else {
                        debug!("  Ground tie: {} -> {}", pin_name, gnd_name);
                    }
                }
            }
        }

        // Create capacitor module definition with proper pins if not exists
        let cap_module_id = if let Some(&module_id) = self.ast_to_module.get("Capacitor") {
            module_id
        } else {
            let module_id = self.netlist.add_module(
                "Capacitor".to_string(),
                bhdl_netlist::types::ModuleKind::Component
            );
            self.ast_to_module.insert("Capacitor".to_string(), module_id);
            debug!("Created Capacitor module");
            module_id
        };

        // The module may pre-exist from an import with no pin definitions
        // registered yet (the stdlib Capacitor uses pins "1"/"2") — an
        // instance of a pin-less module gets zero pin instances and can
        // never be connected. Ensure two terminals exist.
        let cap_module_pinless = self.netlist.modules.get(cap_module_id)
            .map(|m| m.pins.is_empty())
            .unwrap_or(false);
        if cap_module_pinless {
            self.netlist.add_pin(
                cap_module_id,
                "1".to_string(),
                bhdl_netlist::types::PinDirection::InOut,
                bhdl_netlist::types::PinType::Power
            );
            self.netlist.add_pin(
                cap_module_id,
                "2".to_string(),
                bhdl_netlist::types::PinDirection::InOut,
                bhdl_netlist::types::PinType::Ground
            );
            debug!("Added terminal pins 1/2 to pin-less Capacitor module");
        }

        // Resolve the module's terminal pin names once: positive prefers
        // +/1/pos, negative prefers -/2/neg, falling back to declared order.
        let cap_pin_names: Vec<String> = self.netlist.modules.get(cap_module_id)
            .map(|m| m.pins.iter()
                .filter_map(|&pid| self.netlist.pins.get(pid))
                .map(|p| p.name.clone())
                .collect())
            .unwrap_or_default();
        let pick = |prefs: &[&str], fallback: usize| -> Option<String> {
            prefs.iter()
                .find_map(|p| cap_pin_names.iter().find(|n| n == p))
                .or_else(|| cap_pin_names.get(fallback))
                .cloned()
        };
        let cap_pos_pin = pick(&["+", "1", "pos"], 0);
        let cap_neg_pin = pick(&["-", "2", "neg"], 1);

        // Process decoupling capacitors
        for cap in &expansion.decoupling_caps {
            info!("  Decoupling capacitor: {} = {}", cap.instance_name, cap.value);

            // Create an instance of the capacitor
            if let Some(inst_id) = self.netlist.add_instance(
                cap.instance_name.clone(),
                cap_module_id
            ) {
                self.ast_to_instance.insert(cap.instance_name.clone(), inst_id);

                // Create pin instances for this capacitor
                if let Ok(pin_instances) = self.netlist.create_pin_instances(inst_id) {
                    debug!("  Created {} pin instances for {}", pin_instances.len(), cap.instance_name);

                    // Each cap decouples the rail of the domain that
                    // generated it (DecouplingCapacitor::domain).
                    let power_net_id = if let Some(&net_id) = self.ast_to_net.get(&cap.domain) {
                        Some(net_id)
                    } else {
                        let rail = expansion.rails.iter().find(|r| r.name == cap.domain);
                        let net_id = self.netlist.add_net_with_class(
                            Some(cap.domain.clone()),
                            bhdl_netlist::types::NetClass::Power {
                                voltage: rail.and_then(|r| r.voltage).unwrap_or(3.3),
                                current: rail.and_then(|r| r.current),
                            }
                        );
                        self.ast_to_net.insert(cap.domain.clone(), net_id);
                        Some(net_id)
                    };

                    if let Some(power_net) = power_net_id {
                        // Connect the positive terminal to the rail
                        if let Some(plus_pin_inst) = cap_pos_pin.as_ref()
                            .and_then(|p| self.netlist.find_pin_instance(inst_id, p)) {
                            if let Err(e) = self.netlist.connect(power_net, ConnectionPoint::PinInstance(plus_pin_inst)) {
                                warn!("  Failed to connect {} positive pin to @{}: {}", cap.instance_name, cap.domain, e);
                            } else {
                                debug!("  Connected {} positive pin to @{}", cap.instance_name, cap.domain);
                            }
                        } else {
                            warn!("  Decoupling cap {} has no positive terminal pin; left unconnected", cap.instance_name);
                        }

                        // Connect the negative terminal to ground
                        if let Some(minus_pin_inst) = cap_neg_pin.as_ref()
                            .and_then(|p| self.netlist.find_pin_instance(inst_id, p)) {
                            if let Err(e) = self.netlist.connect(gnd_net_id, ConnectionPoint::PinInstance(minus_pin_inst)) {
                                warn!("  Failed to connect {} negative pin to {}: {}", cap.instance_name, gnd_name, e);
                            } else {
                                debug!("  Connected {} negative pin to {}", cap.instance_name, gnd_name);
                            }
                        } else {
                            warn!("  Decoupling cap {} has no negative terminal pin; left unconnected", cap.instance_name);
                        }
                    }

                    // Store capacitor value as instance attribute
                    if let Some(instance) = self.netlist.instances.get_mut(inst_id) {
                        instance.attributes.insert("value".to_string(), cap.value.clone());
                        instance.attributes.insert("component_class".to_string(), "capacitor".to_string());
                        if let Some(ref near_comp) = cap.near_component {
                            instance.attributes.insert("placement".to_string(), format!("near {}", near_comp));
                            info!("    Placement: near {}", near_comp);
                        } else if cap.is_distributed {
                            instance.attributes.insert("placement".to_string(), "distributed".to_string());
                            info!("    Placement: distributed");
                        }
                    }
                } else {
                    warn!("  Failed to create pin instances for {}", cap.instance_name);
                }
            } else {
                warn!("Failed to create capacitor instance: {}", cap.instance_name);
            }
        }

        info!("Power domain expansion complete: {} connections, {} decoupling caps",
              expansion.connections.len(), expansion.decoupling_caps.len());

        Ok(())
    }

    fn include_power_domain_info(&mut self, analysis: &AnalysisResult) -> Result<()> {
        debug!("Including power domain information");

        let power_context = &analysis.power_analysis;
        
        // Add metadata about power domains to netlist
        // This information helps the visualizer understand power relationships
        for (domain_name, domain_info) in &power_context.domains {
            debug!("Power domain '{}': voltage={:?}, current={:?}", 
                   domain_name, domain_info.voltage, domain_info.max_current);
        }

        // The power sequencing information is also valuable for semantic layout
        let power_sequencer = &analysis.power_sequencing;
        debug!("Power sequence has {} steps", power_sequencer.startup_sequence.len());

        Ok(())
    }

    /// Include component inference information for enhanced semantics
    fn include_component_inference_info(&mut self, analysis: &AnalysisResult) -> Result<()> {
        debug!("Including component inference information");

        let component_context = &analysis.component_inference;
        
        debug!("Component inference found {} inferred components", 
               component_context.inferred_components.len());
        
        // The inferred components provide semantic context that helps
        // the visualizer understand circuit function and apply appropriate layout

        Ok(())
    }

    /// Initialize database component mapper
    async fn initialize_database_mapper(&mut self) -> Result<()> {
        info!("🔧 Initializing database component mapper");
        
        let database_path = self.config.database_path.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Database components enabled but no database path specified"))?;
        
        let db_path = Path::new(database_path);
        
        match DatabaseComponentMapper::new(db_path).await {
            Ok(mut mapper) => {
                // Ensure components are imported
                if let Err(e) = mapper.ensure_components_imported().await {
                    warn!("Failed to ensure components imported: {}", e);
                }
                
                // Preload common components for better performance
                if let Err(e) = mapper.preload_common_components().await {
                    warn!("Failed to preload common components: {}", e);
                }
                
                let stats = mapper.get_stats().await;
                info!("✅ Database mapper initialized: {} mappings, {} cached components, {:.1}% cache hit rate", 
                      stats.component_mappings, stats.cached_components, stats.component_cache_hit_rate * 100.0);
                      
                self.database_mapper = Some(mapper);
                Ok(())
            }
            Err(e) => {
                warn!("❌ Failed to initialize database mapper: {}", e);
                warn!("Falling back to generic component generation");
                Err(e)
            }
        }
    }

    
    /// Populate analysis data in the netlist for unified model approach
    /// This allows metadata to flow through to SPICE without conversion
    fn populate_analysis_data(&mut self, analysis: &AnalysisResult) -> Result<()> {
        use bhdl_common::analysis_interface::{AnalysisData, ModuleDefinitionInfo, SymbolInfo, SymbolType};
        use bhdl_analyzer::symbol_table::{SymbolKind, PortDirectionKind};
        
        info!("Populating analysis data in netlist for unified model");
        
        let mut analysis_data = AnalysisData::new();
        
        // Extract symbol information from global scope
        let global_symbols = analysis.global_scope.get_symbols();
        let global_nets = analysis.global_scope.get_nets();
        
        debug!("Extracting symbols from global scope: {} symbols, {} nets", 
               global_symbols.len(), global_nets.len());
        
        // Convert analyzer symbols to common symbol format
        for (name, symbol) in global_symbols {
            let symbol_type = match symbol.kind {
                SymbolKind::Entity => SymbolType::Module,
                SymbolKind::Instance => SymbolType::Instance,
                SymbolKind::Component => SymbolType::Module, // Components are module definitions
                SymbolKind::Board => SymbolType::Module,     // Boards are top-level modules
                SymbolKind::Parameter => SymbolType::Constant,
                _ => continue, // Skip other symbol types for now
            };
            
            let symbol_info = SymbolInfo {
                symbol_type,
                module_type: symbol.instance_type_name.clone(),
                parameters: HashMap::new(), // TODO: Extract parameter information
            };
            
            analysis_data.symbol_data.insert(name.clone(), symbol_info);
            
            // If this is a module definition, create module definition info
            if matches!(symbol.kind, SymbolKind::Entity | SymbolKind::Component | SymbolKind::Board) {
                let module_info = ModuleDefinitionInfo {
                    name: name.clone(),
                    pins: self.extract_module_pins(name, analysis)?,
                    parameters: HashMap::new(), // TODO: Extract parameters from definition
                };
                
                analysis_data.module_definitions.insert(name.clone(), module_info);
            }
        }
        
        // Add net symbols to symbol data
        for (name, symbol) in global_nets {
            let symbol_info = SymbolInfo {
                symbol_type: SymbolType::Net,
                module_type: None,
                parameters: HashMap::new(),
            };
            
            analysis_data.symbol_data.insert(name.clone(), symbol_info);
        }
        
        // Extract symbols from definition scopes (modules, components, etc.)
        for (node_ptr, scope) in bhdl_analyzer::definition_scopes_sorted(&analysis.definition_scopes) {
            debug!("Processing definition scope for node: {:?}", node_ptr);
            
            // Get the scope name to identify the module
            if let Some(scope_name) = &scope.scope_name {
                let scope_symbols = scope.get_symbols();
                let scope_nets = scope.get_nets();
                
                debug!("Definition scope '{}': {} symbols, {} nets", 
                       scope_name, scope_symbols.len(), scope_nets.len());
                
                // Create module definition with pins from this scope
                let module_info = ModuleDefinitionInfo {
                    name: scope_name.clone(),
                    pins: self.extract_pins_from_scope(scope)?,
                    parameters: HashMap::new(), // TODO: Extract parameters
                };
                
                analysis_data.module_definitions.insert(scope_name.clone(), module_info);
                
                // Add scope symbols to global symbol data with qualified names
                for (name, symbol) in scope_symbols {
                    let qualified_name = format!("{}::{}", scope_name, name);
                    let symbol_type = match symbol.kind {
                        SymbolKind::Instance => SymbolType::Instance,
                        SymbolKind::Parameter => SymbolType::Constant,
                        SymbolKind::Pin => continue, // Pins are handled separately in module definitions
                        _ => continue,
                    };
                    
                    let symbol_info = SymbolInfo {
                        symbol_type,
                        module_type: symbol.instance_type_name.clone(),
                        parameters: HashMap::new(),
                    };
                    
                    analysis_data.symbol_data.insert(qualified_name, symbol_info);
                }
            }
        }
        
        // Set the analysis data in the netlist
        self.netlist.set_analysis_data(analysis_data);
        
        info!("Analysis data populated: {} module definitions, {} symbol entries", 
              self.netlist.get_analysis_data().unwrap().module_definitions.len(),
              self.netlist.get_analysis_data().unwrap().symbol_data.len());
        
        Ok(())
    }
    
    /// Extract module pin information for a module from analysis results
    fn extract_module_pins(&self, module_name: &str, analysis: &AnalysisResult) -> Result<ModulePinMetadata> {
        // Look for the module in definition scopes
        for (_, scope) in bhdl_analyzer::definition_scopes_sorted(&analysis.definition_scopes) {
            if scope.scope_name.as_deref() == Some(module_name) {
                return self.extract_pins_from_scope(scope);
            }
        }
        
        // If not found in definition scopes, create empty pin metadata
        Ok(ModulePinMetadata::new())
    }
    
    /// Extract pin metadata from a symbol table scope
    fn extract_pins_from_scope(&self, scope: &bhdl_analyzer::symbol_table::SymbolTable) -> Result<ModulePinMetadata> {
        use bhdl_analyzer::symbol_table::{SymbolKind, PortDirectionKind};
        
        let mut pin_metadata = ModulePinMetadata::new();
        
        // Look for pin symbols in the scope
        for (name, symbol) in scope.get_symbols() {
            if symbol.kind == SymbolKind::Pin {
                let direction = match symbol.direction {
                    Some(PortDirectionKind::In) => CommonPinDirection::Input,
                    Some(PortDirectionKind::Out) => CommonPinDirection::Output,
                    Some(PortDirectionKind::InOut) => CommonPinDirection::Bidirectional,
                    None => CommonPinDirection::Bidirectional, // Default if not specified
                };
                
                // Infer pin type from name and direction (simplified heuristic)
                let pin_type = self.infer_pin_type(name, direction);
                
                let pin_info = PinMetadata {
                    direction,
                    pin_type,
                    function: None, // Will be populated from stdlib definitions
                    electrical: bhdl_common::pin_metadata::PinElectricalData::default(),
                    electrical_specs: HashMap::new(),
                    documentation: None,
                };
                
                pin_metadata.add_pin(name.clone(), pin_info);
            }
        }
        
        Ok(pin_metadata)
    }
    
    /// Infer pin type from name and direction using simple heuristics
    fn infer_pin_type(&self, pin_name: &str, direction: CommonPinDirection) -> CommonPinType {
        
        let name_lower = pin_name.to_lowercase();
        
        // Power pins
        if name_lower.contains("vcc") || name_lower.contains("vdd") || 
           name_lower.contains("vin") || name_lower.contains("vout") ||
           name_lower == "out" && direction == CommonPinDirection::Output {
            return CommonPinType::Power;
        }
        
        // Ground pins
        if name_lower.contains("gnd") || name_lower.contains("vss") ||
           name_lower.contains("ground") {
            return CommonPinType::Ground;
        }
        
        // Clock pins
        if name_lower.contains("clk") || name_lower.contains("clock") {
            return CommonPinType::Clock;
        }
        
        // Reset pins
        if name_lower.contains("rst") || name_lower.contains("reset") {
            return CommonPinType::Reset;
        }
        
        // Enable pins
        if name_lower.contains("en") || name_lower.contains("enable") {
            return CommonPinType::Enable;
        }
        
        // Default to signal
        CommonPinType::Signal
    }
    
    /// Stamp each instance's entity `attribute` declarations onto the
    /// instance, making entity attributes first-class on the netlist (Phase
    /// 4.6). Resolves the imported entity for each instance's module name and
    /// copies its `attribute` decls (component_class, switching_frequency,
    /// topology, …) onto the instance, never overwriting an existing key.
    fn stamp_entity_attributes_on_instances(&mut self) {
        // Step 1: (instance, entity-name) for every instance — borrows netlist.
        let id_names: Vec<(InstanceId, String)> = self
            .netlist
            .instances
            .iter()
            .filter_map(|(id, inst)| {
                let module = self.netlist.modules.get(inst.definition)?;
                Some((id, module.name.clone()))
            })
            .collect();
        // Step 2: resolve each entity's attributes. First try the import
        // loader (explicitly-imported entities). For entities the loader
        // can't supply — a bare `TPS54302()` used WITHOUT an `import`, which
        // resolves only against a same-file stub that carries no attributes —
        // fall back to scanning the bundled stdlib by entity name, so the
        // stdlib part's real attributes (component_class, switching_frequency,
        // topology, output_current, …) still reach the instance.
        //
        // Both paths use the *resolved* extractor so attribute values that are
        // bare references to the entity's own consts/params (`f_sw = f_sw`,
        // `switching_frequency = BUCK_PARAMS.switching_frequency`) land as the
        // concrete number the sign-off ripple model needs, not the literal
        // reference text.
        use bhdl_analyzer::attribute_extraction::extract_module_attributes_resolved as extract_attrs;

        // Which entity names need the stdlib fallback (loader has no attrs)?
        // Entity AST source: the import loader for imported entities, the
        // generator's own local_entities capture for SAME-FILE entities —
        // without the latter, a same-file IC got no component_class and
        // downstream consumers (PnR is_power_symbol, BOM class filter)
        // silently dropped it from the board.
        let entity_ast = |name: &str| -> Option<&bhdl_ast::Entity> {
            self.import_loader
                .get_entity(name)
                .or_else(|| self.local_entities.get(name))
        };
        let fallback_names: std::collections::HashSet<String> = id_names
            .iter()
            .filter(|(_, name)| {
                entity_ast(name)
                    .map(|e| extract_attrs(e).is_empty())
                    .unwrap_or(true)
            })
            .map(|(_, name)| name.clone())
            .collect();
        let stdlib_attrs = Self::stdlib_entity_attribute_index(&fallback_names);

        let updates: Vec<(InstanceId, std::collections::HashMap<String, String>, std::collections::HashMap<String, Vec<String>>)> = id_names
            .into_iter()
            .filter_map(|(id, name)| {
                // The instance's already-stamped constructor args (Phase 4.4)
                // override the entity's param defaults, so a `v_out`-referencing
                // attribute resolves to the board's `LM317(v_out=9V)` rather
                // than the entity default.
                let inst_args: std::collections::HashMap<String, String> = self
                    .netlist
                    .instances
                    .get(id)
                    .map(|i| i.attributes.clone())
                    .unwrap_or_default();
                // Stale-placeholder forms: instance creation copies the
                // module's attributes, which carry either the raw reference
                // text for param refs with no default (`voltage_rating =
                // voltage`) or the entity-DEFAULT resolution for defaulted
                // params (`current_rating = rated_current` → "1A" before the
                // instance's `Ind(8.2uH, 6A)` arg is known). Collect both
                // forms per key so the apply step below can recognise a
                // value that is still just the entity-level copy and replace
                // it with the per-instance resolution; a value matching
                // neither is a genuine per-instance choice and is kept.
                let stale_attrs: std::collections::HashMap<String, Vec<String>> = entity_ast(&name)
                    .map(|e| {
                        let mut stale: std::collections::HashMap<String, Vec<String>> =
                            std::collections::HashMap::new();
                        for (k, v) in bhdl_analyzer::attribute_extraction::extract_module_attributes(e) {
                            stale.entry(k).or_default().push(v);
                        }
                        for (k, v) in
                            bhdl_analyzer::attribute_extraction::extract_module_attributes_resolved(e)
                        {
                            stale.entry(k).or_default().push(v);
                        }
                        stale
                    })
                    .unwrap_or_default();
                let mut attrs = entity_ast(&name)
                    .map(|e| bhdl_analyzer::attribute_extraction::extract_module_attributes_resolved_with(e, &inst_args))
                    .filter(|a| !a.is_empty())
                    .or_else(|| stdlib_attrs.get(&name).cloned())?;
                // Alias specialization (`alias LM7805 = LinearRegulator<5V>;`):
                // bind the alias's generic arguments to the target entity's
                // generic parameters and substitute attribute values that
                // reference them (`attribute output_voltage = V_OUT` → `5V`).
                // Without this the literal text "V_OUT" was stamped, which the
                // regulator decomposition can't parse as a voltage.
                if let (Some(args), Some(entity)) = (
                    self.import_loader.get_alias_generic_args(&name),
                    self.import_loader.get_entity(&name),
                ) {
                    let generic_params =
                        bhdl_analyzer::attribute_extraction::extract_generic_param_info(entity);
                    if !generic_params.is_empty() {
                        bhdl_analyzer::attribute_extraction::substitute_generic_attr_refs(
                            &mut attrs,
                            &generic_params,
                            args,
                        );
                    }
                }
                if attrs.is_empty() {
                    None
                } else {
                    Some((id, attrs, stale_attrs))
                }
            })
            .collect();
        // Step 3: apply — mutates instances. An attribute the instance
        // already carries is kept — UNLESS its value is still one of the
        // entity-level copies stamped at instance creation before the ctor
        // args were known (the raw unresolved reference text
        // `voltage_rating = "voltage"`, or the entity default
        // `current_rating = "1A"`) and we now have a real per-instance
        // resolution: positional args bound to declared param names
        // (ElectrolyticCap(100µF, 25V) → voltage=25V, Ind(8.2uH, 6A) →
        // rated_current=6A) only land on the instance after creation, so
        // the stale placeholder must yield to the resolved value here.
        let (mut total, mut touched) = (0usize, 0usize);
        for (id, attrs, stale_attrs) in updates {
            if let Some(inst) = self.netlist.instances.get_mut(id) {
                let before = total;
                for (k, v) in attrs {
                    match inst.attributes.get(&k) {
                        None => {
                            inst.attributes.insert(k, v);
                            total += 1;
                        }
                        Some(existing)
                            if *existing != v
                                && stale_attrs
                                    .get(&k)
                                    .is_some_and(|forms| forms.iter().any(|f| f == existing)) =>
                        {
                            inst.attributes.insert(k, v);
                            total += 1;
                        }
                        Some(_) => {}
                    }
                }
                if total > before {
                    touched += 1;
                }
            }
        }
        if total > 0 {
            info!("Phase 4.6: stamped {total} entity attribute(s) across {touched} instance(s)");
        }
    }

    /// Build an (entity name → resolved attributes) index by scanning the
    /// bundled stdlib for the requested entity names. This is the Phase 4.6
    /// fallback for BARE (non-imported) parts: a circuit may use `TPS54302()`
    /// with only a same-file pin stub (no attributes) and no `import`, so the
    /// import loader has nothing — but the real part lives in the stdlib.
    /// Returns an empty map when no names are requested or the stdlib can't be
    /// located (an installed CLI run from an unrelated cwd), in which case the
    /// instance simply keeps whatever attributes it already had.
    fn stdlib_entity_attribute_index(
        names: &std::collections::HashSet<String>,
    ) -> HashMap<String, HashMap<String, String>> {
        use bhdl_analyzer::attribute_extraction::extract_module_attributes_resolved;
        let mut out: HashMap<String, HashMap<String, String>> = HashMap::new();
        if names.is_empty() {
            return out;
        }
        // Prefer the user's installed resolver; otherwise a discovery resolver
        // rooted at the bundled stdlib (located through the shared import
        // search order — input dir, -I roots, $BHDL_LIB_PATH, cwd).
        let Some(resolver) = global_library_resolver().or_else(|| {
            let stdlib = bhdl_common::import_search::locate_dir("bhdl-stdlib")?;
            bhdl_common::library::LibraryResolver::new(None, &[], None, Some(stdlib)).ok()
        }) else {
            return out;
        };
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        for root in resolver.library_roots() {
            collect_bhdl_files(&root, &mut files);
        }
        files.sort(); // deterministic merge order
        for path in files {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let parse = bhdl_parser::parse(&text);
            let Some(sf) = SourceFile::cast(parse.syntax()) else {
                continue;
            };
            for entity in sf.entities() {
                let Some(ename) = entity.name().map(|t| t.text().to_string()) else {
                    continue;
                };
                if !names.contains(&ename) {
                    continue;
                }
                let attrs = extract_module_attributes_resolved(&entity);
                if attrs.is_empty() {
                    continue;
                }
                let slot = out.entry(ename).or_default();
                for (k, v) in attrs {
                    merge_stdlib_attr(slot, k, v);
                }
            }
        }
        out
    }

    /// Add pins to a component module based on its type using stdlib definitions
    fn add_pins_for_component(&mut self, instance_name: &str, component_type: &str, module_id: ModuleId) -> Result<()> {
        debug!("add_pins_for_component called for component_type: {} (from lib.rs)", component_type);
        debug!("import_preprocessor is_some: {}", self.import_preprocessor.is_some());
        
        // Stamp v0.9 entity aliases (`aliases { gpio0 = PB0; ... }`)
        // onto the module's attributes BEFORE we touch pin definitions —
        // resolve_field_binding_alias checks these attrs to translate
        // `mcu.gpio0` to `mcu.PB0` at connection-resolution time.
        if let Some(ref preprocessor) = self.import_preprocessor {
            if let Some(entity) = preprocessor.get_imported_entity(component_type) {
                use bhdl_ast::SyntaxKind;
                use rowan::ast::AstNode;
                let prefix = crate::hierarchical_connectivity::ENTITY_ALIAS_ATTR_PREFIX;
                let mut stamped = 0usize;
                for node in entity.syntax().descendants() {
                    if node.kind() != SyntaxKind::ENTITY_ALIASES_BLOCK { continue; }
                    for mapping in node.children() {
                        if mapping.kind() != SyntaxKind::ENTITY_ALIAS_MAPPING { continue; }
                        let idents: Vec<String> = mapping.children_with_tokens()
                            .filter_map(|el| el.into_token())
                            .filter(|t| t.kind() == SyntaxKind::IDENT)
                            .map(|t| t.text().to_string())
                            .collect();
                        if idents.len() < 2 { continue; }
                        if let Some(m) = self.netlist.modules.get_mut(module_id) {
                            m.attributes.insert(
                                format!("{}{}", prefix, idents[0]),
                                idents[1].clone(),
                            );
                            stamped += 1;
                        }
                    }
                }
                if stamped > 0 {
                    info!("Stamped {} v0.9 alias(es) on module '{}'",
                          stamped, component_type);
                }
            }
        }

        // First check if this component was imported via preprocessor
        // ROOT-CAUSE NOTE (connectivity bug family, docs/spec/ERC.md): this
        // chain previously nested the preprocessor lookup — when a
        // preprocessor EXISTED but the type wasn't imported (any board with
        // one import plus a same-file entity), the inner miss returned EMPTY
        // pins instead of cascading, hollowing out the whole netlist.
        let preprocessed_entity = self
            .import_preprocessor
            .as_ref()
            .and_then(|p| p.get_imported_entity(component_type))
            .cloned();
        let (pin_definitions, has_virtual_pins) = if let Some(entity) = preprocessed_entity.as_ref() {
            {
                println!("SYNTHESIZER: Using preprocessed imported entity definition for '{}'", component_type);

                // Extract pins from the imported entity
                let mut pins = Vec::new();
                for pin in entity.pins() {
                    if let Some(name) = pin.name() {
                        let pin_text = pin.syntax().text().to_string();
                        let is_virtual = pin_text.contains("virtual");
                        
                        // Parse direction and type from pin declaration
                        let (direction, pin_type) = if pin_text.contains("power in") {
                            (bhdl_netlist::types::PinDirection::Power, bhdl_netlist::types::PinType::Power)
                        } else if pin_text.contains("power out") {
                            (bhdl_netlist::types::PinDirection::Out, bhdl_netlist::types::PinType::Power)
                        } else if pin_text.contains("ground") {
                            (bhdl_netlist::types::PinDirection::Ground, bhdl_netlist::types::PinType::Ground)
                        } else if pin_text.contains("signal inout") {
                            // inout before in/out: "signal in" is a substring
                            // of "signal inout" — the old order mapped every
                            // imported inout pin (SDA/SCL, buses) to In.
                            (bhdl_netlist::types::PinDirection::InOut, bhdl_netlist::types::PinType::Signal)
                        } else if pin_text.contains("signal out") {
                            (bhdl_netlist::types::PinDirection::Out, bhdl_netlist::types::PinType::Signal)
                        } else if pin_text.contains("signal in") {
                            (bhdl_netlist::types::PinDirection::In, bhdl_netlist::types::PinType::Signal)
                        } else {
                            (bhdl_netlist::types::PinDirection::Passive, bhdl_netlist::types::PinType::Passive)
                        };
                        
                        // Literal bus pin (`pin VCCO[4]` / `pin D[7:0]`):
                        // expand to indexed pin definitions so indexed
                        // references (`inst.VCCO[0]`) resolve.
                        for pin_name in crate::hierarchical_connectivity::expand_bus_pin_names(&pin, name.text()) {
                            pins.push(bhdl_stdlib::StdlibPinDefinition {
                                name: pin_name,
                                direction,
                                pin_type,
                                is_virtual,
                            });
                        }
                    }
                }

                let has_virtual = pins.iter().any(|p| p.is_virtual);
                (pins, has_virtual)
            }
        } else if let Some(entity) = self.import_loader.get_entity(component_type) {
            // Legacy path: check import_loader if no preprocessor
            println!("SYNTHESIZER: Using legacy import_loader for '{}'", component_type);

            // Extract pins from the imported entity
            let mut pins = Vec::new();
            for pin in entity.pins() {
                if let Some(name) = pin.name() {
                    let pin_text = pin.syntax().text().to_string();
                    let is_virtual = pin_text.contains("virtual");
                    
                    // Parse direction and type from pin declaration
                    let (direction, pin_type) = if pin_text.contains("power in") {
                        (bhdl_netlist::types::PinDirection::Power, bhdl_netlist::types::PinType::Power)
                    } else if pin_text.contains("power out") {
                        (bhdl_netlist::types::PinDirection::Out, bhdl_netlist::types::PinType::Power)
                    } else if pin_text.contains("ground") {
                        (bhdl_netlist::types::PinDirection::Ground, bhdl_netlist::types::PinType::Ground)
                    } else if pin_text.contains("signal inout") {
                        // inout before in/out: "signal in" is a substring of
                        // "signal inout" — the old order mapped every inout
                        // pin to In (latent since the imported-entity path
                        // was written; surfaced by the ERC direction checks).
                        (bhdl_netlist::types::PinDirection::InOut, bhdl_netlist::types::PinType::Signal)
                    } else if pin_text.contains("signal out") {
                        (bhdl_netlist::types::PinDirection::Out, bhdl_netlist::types::PinType::Signal)
                    } else if pin_text.contains("signal in") {
                        (bhdl_netlist::types::PinDirection::In, bhdl_netlist::types::PinType::Signal)
                    } else {
                        (bhdl_netlist::types::PinDirection::Passive, bhdl_netlist::types::PinType::Passive)
                    };
                    
                    // Literal bus pin: expand to indexed pin definitions.
                    for pin_name in crate::hierarchical_connectivity::expand_bus_pin_names(&pin, name.text()) {
                        pins.push(bhdl_stdlib::StdlibPinDefinition {
                            name: pin_name,
                            direction,
                            pin_type,
                            is_virtual,
                        });
                    }
                }
            }

            let has_virtual = pins.iter().any(|p| p.is_virtual);
            (pins, has_virtual)
        } else if let Some(entity) = self.local_entities.get(component_type).cloned() {
            // Same-file entity definition: read the DECLARED pin directions
            // (`pin TX: signal out;`) exactly like the imported-entity path.
            let mut pins = Vec::new();
            for pin in entity.pins() {
                if let Some(name) = pin.name() {
                    let pin_text = pin.syntax().text().to_string();
                    let is_virtual = pin_text.contains("virtual");
                    let (direction, pin_type) = if pin_text.contains("power in") {
                        (bhdl_netlist::types::PinDirection::Power, bhdl_netlist::types::PinType::Power)
                    } else if pin_text.contains("power out") {
                        (bhdl_netlist::types::PinDirection::Out, bhdl_netlist::types::PinType::Power)
                    } else if pin_text.contains("ground") {
                        (bhdl_netlist::types::PinDirection::Ground, bhdl_netlist::types::PinType::Ground)
                    } else if pin_text.contains("signal inout") {
                        // NOTE: inout MUST be tested before in/out — the
                        // substring "signal in" matches inside "signal inout".
                        (bhdl_netlist::types::PinDirection::InOut, bhdl_netlist::types::PinType::Signal)
                    } else if pin_text.contains("signal out") {
                        (bhdl_netlist::types::PinDirection::Out, bhdl_netlist::types::PinType::Signal)
                    } else if pin_text.contains("signal in") {
                        (bhdl_netlist::types::PinDirection::In, bhdl_netlist::types::PinType::Signal)
                    } else {
                        (bhdl_netlist::types::PinDirection::Passive, bhdl_netlist::types::PinType::Passive)
                    };
                    // Literal bus pin: expand to indexed pin definitions.
                    for pin_name in crate::hierarchical_connectivity::expand_bus_pin_names(&pin, name.text()) {
                        pins.push(bhdl_stdlib::StdlibPinDefinition {
                            name: pin_name,
                            direction,
                            pin_type,
                            is_virtual,
                        });
                    }
                }
            }
            let has_virtual = pins.iter().any(|p| p.is_virtual);
            (pins, has_virtual)
        } else {
            // No component information available - create default pins
            warn!("No pin information available for component {} - using default pins", component_type);
            (Vec::new(), false)
        };
        
        debug!("Adding {} pins for component type '{}' to module", 
               pin_definitions.len(), component_type);
        
        if has_virtual_pins {
            // The pin is materialised normally; the entity's `expansion { }`
            // block is applied post-synthesis by `expansion_interpreter`.
            info!("Component '{}' has a virtual pin — expansion handled by its \
                   expansion {{ }} block", component_type);
        }
        
        // Add each pin to the netlist module
        for pin_def in pin_definitions {
            if pin_def.is_virtual {
                debug!("  Adding VIRTUAL pin '{}' to {}", pin_def.name, component_type);
                // In a full implementation, we would:
                // 1. Look up the expansion rules for this virtual pin
                // 2. Generate the required components
                // 3. Create the connections
                // For now, we still add the pin but mark it for expansion
            }
            
            let pin_id = self.netlist.add_pin(
                module_id,
                pin_def.name.clone(),
                pin_def.direction,
                pin_def.pin_type
            );
            if pin_def.is_virtual {
                if let Some(pid) = pin_id {
                    if let Some(pin) = self.netlist.pins.get_mut(pid) {
                        pin.is_virtual = true;
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Backwards compatibility method for synthesizing from AST and analysis
    /// This matches the expected interface from existing test code
    pub async fn synthesize(&mut self, ast: &SourceFile, analysis: &AnalysisResult) -> Result<Netlist> {
        self.generate_from_ast_and_analysis(ast, analysis).await
    }
    
    /// Check if a component type is actually an interface
    pub fn is_interface_type(&self, component_type: &str, analysis: &AnalysisResult) -> bool {
        if let Some(symbol) = analysis.global_scope.lookup(component_type) {
            symbol.kind == bhdl_analyzer::symbol_table::SymbolKind::Interface
        } else {
            false
        }
    }
    
    /// Process an interface instance during component synthesis
    pub fn process_interface_instance(
        &mut self,
        instance_name: &str,
        interface_type: &str,
        analysis: &AnalysisResult
    ) -> Result<()> {
        debug!("Processing interface instance {} of type {}", instance_name, interface_type);
        
        // For now, just create the nets
        // In the future, we need to get the interface definition and create all signals
        
        // Create basic I2C interface nets as an example
        if interface_type == "I2C" {
            let sda_net_name = format!("{}_SDA", instance_name);
            let scl_net_name = format!("{}_SCL", instance_name);
            
            let sda_net_id = self.netlist.add_net(Some(sda_net_name.clone()));
            let scl_net_id = self.netlist.add_net(Some(scl_net_name.clone()));
            
            self.ast_to_net.insert(sda_net_name, sda_net_id);
            self.ast_to_net.insert(scl_net_name, scl_net_id);
            
            info!("Created I2C interface nets for {}", instance_name);
        }
        
        // Create basic UART interface nets
        if interface_type == "UART" {
            let tx_net_name = format!("{}_TX", instance_name);
            let rx_net_name = format!("{}_RX", instance_name);
            
            let tx_net_id = self.netlist.add_net(Some(tx_net_name.clone()));
            let rx_net_id = self.netlist.add_net(Some(rx_net_name.clone()));
            
            self.ast_to_net.insert(tx_net_name, tx_net_id);
            self.ast_to_net.insert(rx_net_name, rx_net_id);
            
            info!("Created UART interface nets for {}", instance_name);
        }
        
        Ok(())
    }

    /// Transfer component parameters from analyzer to netlist instance attributes
    pub fn populate_instance_attributes(&mut self, instance_id: InstanceId, handle_name: &str, analysis: &AnalysisResult) {
        populate_instance_attributes(&mut self.netlist, instance_id, handle_name, analysis);
    }

    /// Run simulation-driven optimization on the netlist
    async fn run_simulation_optimization(&mut self, ast: Option<&SourceFile>, analysis: &AnalysisResult) -> Result<()> {
        info!("Starting simulation-driven optimization");
        
        // Step 1: Extract behavioral models from components in the netlist
        let behavioral_models = self.extract_behavioral_models_from_components(ast, analysis)?;
        
        if behavioral_models.is_empty() {
            warn!("No behavioral models found in components - skipping optimization");
            return Ok(());
        }
        
        info!("Found {} behavioral models for optimization", behavioral_models.len());
        
        // Step 2: Create simulation engine and run optimization
        let mut sim_optimizer = simulation_driven::SimulationDrivenSynthesizer::new();
        
        // Step 3: Set up design requirements from analysis
        let requirements = self.create_design_requirements(analysis)?;
        
        // Step 4: Run optimization on the netlist (pass the extracted models)
        match sim_optimizer.optimize_netlist(&mut self.netlist, &requirements, Some(behavioral_models)) {
            Ok(report) => {
                info!("Simulation optimization complete:");
                info!("  Models used: {}", report.models_found);
                info!("  Success: {}", report.optimization_successful);
                
                if !report.final_metrics.is_empty() {
                    info!("  Final metrics:");
                    for (metric, value) in &report.final_metrics {
                        info!("    {}: {:.3}", metric, value);
                    }
                }
            }
            Err(e) => {
                warn!("Simulation optimization failed: {}", e);
                // Continue with unoptimized netlist
            }
        }
        
        Ok(())
    }
    
    /// Extract behavioral models from component definitions
    fn extract_behavioral_models_from_components(
        &self,
        ast: Option<&SourceFile>, 
        analysis: &AnalysisResult
    ) -> Result<Vec<bhdl_simulation::ModelMetadata>> {
        // Use the behavioral model extractor
        let mut extractor = BehavioralModelExtractor::new();
        
        // Extract from AST if available
        if let Some(ast) = ast {
            if let Err(e) = extractor.extract_from_ast(ast, analysis) {
                warn!("Failed to extract behavioral models: {}", e);
            }
        }
        
        Ok(extractor.get_models().to_vec())
    }
    
    /// Create design requirements from analysis results and component behavioral models
    fn create_design_requirements(&self, analysis: &AnalysisResult) -> Result<simulation_driven::DesignRequirements> {
        let mut requirements = simulation_driven::DesignRequirements::default();
        
        // Extract requirements from component behavioral models
        // These should come from @optimization_strategy annotations in the components
        for (name, symbol) in analysis.global_scope.get_symbols() {
            // Check if this is a module/component with optimization requirements
            if symbol.kind == bhdl_analyzer::symbol_table::SymbolKind::Entity {
                // TODO: Extract from @optimization_strategy annotation
                // For now, look for known power converter types
                if name.contains("Buck") || name.contains("Boost") || name.contains("PowerSupply") {
                    // These values should come from the component's behavioral model
                    // e.g., @optimization_strategy { target_efficiency: 0.92, min_phase_margin: 60 }
                    debug!("Component {} should provide its own optimization requirements", name);
                }
            }
            
            // Extract voltage/current requirements from power domains
            if let Some(net_attr) = &symbol.net_attributes {
                if let Some(voltage) = net_attr.voltage() {
                    if name.contains("OUT") {
                        // Ripple requirement should also come from component specs
                        // e.g., @component_knowledge { max_ripple_percent: 1.0 }
                        requirements.max_output_ripple = Some(voltage * 0.01); // Default 1% if not specified
                    }
                }
            }
        }
        
        // Default optimization goals if not specified by components
        // These should be overridden by component-specific requirements
        requirements.minimize_cost = true;
        requirements.minimize_size = true;
        
        // DON'T hardcode these - they should come from behavioral models
        // requirements.target_efficiency = Some(0.90);  // WRONG - should come from component
        // requirements.min_phase_margin = Some(60.0);   // WRONG - should come from component
        
        warn!("Design requirements should be extracted from component behavioral models, not hardcoded");
        
        Ok(requirements)
    }
    
    /// Check if database integration is enabled
    pub fn is_database_enabled(&self) -> bool {
        self.database_mapper.is_some()
    }

    /// Get database statistics if database is enabled
    pub async fn get_database_stats(&self) -> Option<DatabaseStats> {
        if let Some(ref mapper) = self.database_mapper {
            Some(DatabaseStats {
                component_mappings: self.component_instances.len() as u32,
                cached_components: mapper.get_cached_component_count().unwrap_or(0),
                cached_svg_symbols: mapper.get_cached_symbol_count().unwrap_or(0),
                component_cache_hit_rate: mapper.get_component_cache_hit_rate(),
                symbol_cache_hit_rate: mapper.get_symbol_cache_hit_rate(),
            })
        } else {
            None
        }
    }

    /// Get all component instances with database mappings
    pub fn get_component_instances(&self) -> &Vec<DatabaseComponentInstance> {
        &self.component_instances
    }
}

impl Default for NetlistGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// One-stop entry point that takes raw BHDL source text and runs
/// the full pipeline:
///   1. v0.9b abstract-entity preprocessor
///      (`abstract_resolver::preprocess`) — strips `abstract entity`
///      blocks, rewrites `mcu: ABSTRACT()` → `mcu: ChosenSKU()`,
///      rewrites `mcu.alias` → `mcu.<pin_map[alias]>`.
///   2. Parser → AST.
///   3. Analyzer.
///   4. NetlistGenerator pipeline (constructor-arg stamping,
///      expansion interpreter, conditional gating, etc.).
///
/// Returns the rewritten source alongside the netlist so callers
/// can debug / display what the parser actually saw. If the input
/// has no abstract-entity declarations, the rewritten source equals
/// the input.
pub async fn synthesize_from_source(source: &str) -> Result<(String, Netlist)> {
    use bhdl_ast::AstNode;
    // v0.8: monomorphise parametric interfaces (`interface SPI<lanes=4> ...`)
    // before the abstract resolver, so abstract-entity bodies that use
    // parametric interfaces work cleanly downstream.
    let source = crate::parametric_resolver::preprocess(source)?;
    let (rewritten, resolutions) =
        crate::abstract_resolver::preprocess_with_resolutions(&source)?;
    let pr = bhdl_parser::parse(&rewritten);
    if !pr.errors().is_empty() {
        let errs: Vec<String> = pr.errors().iter().take(5)
            .map(|e| format!("{:?}", e)).collect();
        return Err(anyhow::anyhow!(
            "Parse errors in (possibly rewritten) source: {}", errs.join("; ")));
    }
    let sf = SourceFile::cast(pr.syntax())
        .ok_or_else(|| anyhow::anyhow!("Could not cast to SourceFile"))?;
    let analysis = bhdl_analyzer::analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let mut netlist = gen.generate_from_ast_and_analysis(&sf, &analysis).await?;

    // Stamp the v0.9b resolution choice on each abstract-resolved
    // instance. BOM walkers, KiCad export, comparators, and SPICE
    // exporters can read these to know:
    //   - which abstract entity the user wrote (`abstract_origin`)
    //   - which concrete SKU the resolver picked (`selected_sku`)
    // …without re-running the preprocessor.
    for (inst_name, resolution) in &resolutions {
        for (_id, inst) in netlist.instances.iter_mut() {
            if inst.name == *inst_name {
                inst.attributes.insert(
                    "abstract_origin".to_string(),
                    resolution.abstract_entity.clone());
                inst.attributes.insert(
                    "selected_sku".to_string(),
                    resolution.concrete_sku.clone());
            }
        }
    }

    Ok((rewritten, netlist))
}

/// Generate netlist from BHDL source with full semantic context
pub async fn generate_netlist_from_source(source_file: &SourceFile) -> Result<Netlist> {
    info!("Generating netlist from BHDL source with semantic context");
    
    // Phase 1: Perform semantic analysis
    let analysis = bhdl_analyzer::analyze(source_file);
    
    if !analysis.diagnostics.is_empty() {
        // Check if any diagnostics are about undefined components
        let has_undefined_components = analysis.diagnostics.iter()
            .any(|d| d.message.contains("Undefined component"));
        
        if has_undefined_components {
            error!("Analysis found undefined components - cannot continue synthesis");
            for diagnostic in &analysis.diagnostics {
                if diagnostic.message.contains("Undefined component") {
                    error!("  {}", diagnostic.message);
                }
            }
            return Err(anyhow::anyhow!("Cannot synthesize circuit with undefined components. Please import required components."));
        }
        
        // Warn about other diagnostics
        warn!("Analysis produced {} diagnostics", analysis.diagnostics.len());
        for diagnostic in &analysis.diagnostics {
            warn!("  {}", diagnostic.message);
        }
    }

    // Phase 2: Generate netlist with semantic context preservation
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_analysis(&analysis).await
        .context("Failed to generate netlist from analysis results")?;

    info!("Successfully generated semantic-aware netlist");
    Ok(netlist)
}

/// Generate netlist with custom configuration
pub async fn generate_netlist_with_config(
    source_file: &SourceFile, 
    config: NetlistConfig
) -> Result<Netlist> {
    info!("Generating netlist from BHDL source with custom config");
    
    // Phase 1: Perform semantic analysis
    let analysis = bhdl_analyzer::analyze(source_file);
    
    if !analysis.diagnostics.is_empty() {
        // Check for critical errors
        let has_critical_errors = analysis.diagnostics.iter()
            .any(|d| d.message.contains("Undefined component") || 
                     d.message.contains("Cannot resolve"));
        
        if has_critical_errors {
            error!("Analysis found critical errors - cannot continue synthesis");
            for diagnostic in &analysis.diagnostics {
                if diagnostic.message.contains("Undefined") || diagnostic.message.contains("Cannot resolve") {
                    error!("  {}", diagnostic.message);
                }
            }
            return Err(anyhow::anyhow!("Cannot synthesize circuit with undefined components"));
        }
        
        warn!("Analysis produced {} diagnostics", analysis.diagnostics.len());
    }

    // Phase 2: Generate netlist with custom configuration
    let mut generator = NetlistGenerator::with_config(config);
    let netlist = generator.generate_from_analysis(&analysis).await
        .context("Failed to generate netlist with custom config")?;

    info!("Successfully generated custom netlist");
    Ok(netlist)
}

/// Transfer component parameters from analyzer to netlist instance attributes
pub fn populate_instance_attributes(
    netlist: &mut Netlist,
    instance_id: InstanceId, 
    handle_name: &str, 
    analysis: &AnalysisResult
) {
    log::debug!("populate_instance_attributes called!");
    log::debug!("Transferring parameters for instance {:?} (handle: {})", instance_id, handle_name);
    
    let inference_components = analysis.component_inference.get_inferred_components();
    log::debug!("Total inferred components: {}", inference_components.len());
    
    for (idx, component) in inference_components.iter().enumerate() {
        log::debug!("Component {}: type={}, instance_name={:?}, params={}",
                 idx, component.component_type, component.instance_name, component.parameters.len());
    }
    
    // 🎯 NEW: Check if unified simulation data is available for enhanced component selection
    let has_simulation_data = analysis.simulation_data.simulation_metadata.engines_used.len() > 0;
    if has_simulation_data {
        println!("🔬 SIMULATION INTEGRATION: Unified simulation data available - using simulation-based component selection");
        
        // Apply simulation-based component calculations
        apply_simulation_based_component_selection(netlist, instance_id, handle_name, analysis);
    } else {
        println!("⚠️  SIMULATION INTEGRATION: No simulation data - using basic component parameters");
    }
    
    // 🎯 Intent-Aware Synthesis Integration
    // Get the component type for this instance to check synthesis knowledge
    let component_type = if let Some(instance) = netlist.instances.get(instance_id) {
        if let Some(module) = netlist.modules.get(instance.definition) {
            Some(module.name.clone())
        } else {
            None
        }
    } else {
        None
    };
    
    if let Some(comp_type) = component_type {
        debug!("Checking synthesis knowledge in stdlib for component type: {}", comp_type);
        
        // TODO: Load and apply synthesis knowledge from stdlib BHDL files
        // This would:
        // 1. Parse the TPS54331_SYNTHESIS const from the BHDL file
        // 2. Apply virtual pin expansions
        // 3. Add calculated components with proper intents
        
        info!("Synthesis knowledge integration for {} pending full stdlib parser implementation", comp_type);
    }
    
    // Look up the component in the analyzer's component inference data
    for component in inference_components {
        if let Some(instance_name) = &component.instance_name {
            if instance_name == handle_name {
                log::debug!("Found inference data for handle '{}', component type: {}", handle_name, component.component_type);
                
                // Extract parameters from the component suggestion.
                // Sort first: the Vec inherits HashMap iteration order
                // from several inference sources, and stamping order
                // decides which duplicate wins (override vs or_insert) —
                // unsorted, netlists and layouts were nondeterministic.
                let mut params_sorted: Vec<_> = component.parameters.iter().collect();
                params_sorted.sort_by(|a, b| {
                    a.name
                        .cmp(&b.name)
                        .then(
                            b.confidence
                                .partial_cmp(&a.confidence)
                                .unwrap_or(std::cmp::Ordering::Equal),
                        )
                        .then_with(|| format!("{:?}", a.value).cmp(&format!("{:?}", b.value)))
                });
                for param in params_sorted {
                    let param_value = match &param.value {
                        ParameterValue::Resistance(r) => r.to_string(),
                        ParameterValue::Capacitance(c) => c.to_string(),
                        ParameterValue::Voltage(v) => v.to_string(),
                        ParameterValue::Current(i) => i.to_string(),
                        // Strip the literal string quotes the source form
                        // carries — the ctor-arg path (Phase 4.4) stamps
                        // unquoted values, and attribute consumers
                        // (erc_waive clause match, expansion_parent
                        // stress-designator mapping) compare against bare
                        // text. Quoted-vs-bare split the two paths' values
                        // for the same attribute.
                        ParameterValue::String(s) => s.trim().trim_matches('"').to_string(),
                        ParameterValue::Real(r) => r.to_string(),
                        ParameterValue::Integer(i) => i.to_string(),
                        _ => continue, // Skip unsupported parameter types
                    };
                    
                    log::debug!("Setting parameter '{}' = '{}' for instance {:?}", param.name, param_value, instance_id);
                    
                    // Store the parameter in the netlist instance attributes.
                    // Two provenance classes, two policies:
                    //  - USER-SPECIFIED params (confidence == 1.0, extracted
                    //    from the instantiation's own arg list) OVERRIDE —
                    //    inline instantiations never pass through Phase 4.4,
                    //    so this is the only stamping their explicit args
                    //    get; an or_insert here let an earlier-stamped
                    //    entity default (5%) beat the user's tolerance=1%.
                    //    (When Phase 4.4 DID stamp the same ctor arg, the
                    //    values are identical — the override is a no-op.)
                    //  - Inference DEFAULTS keep or_insert: they must never
                    //    clobber a design-block or ctor-stamped choice.
                    if let Some(instance) = netlist.instances.get_mut(instance_id) {
                        if (param.confidence - 1.0).abs() < f64::EPSILON {
                            instance.attributes.insert(param.name.clone(), param_value);
                        } else {
                            instance.attributes.entry(param.name.clone()).or_insert(param_value);
                        }
                        log::debug!("Successfully stored parameter in netlist instance");
                    } else {
                        log::debug!("Failed to find instance {:?} in netlist", instance_id);
                    }
                }
                
                return; // Found the component, we're done
            }
        }
    }
    
    log::debug!("No component inference data found for handle '{}' (instance {:?})", handle_name, instance_id);
}

/// 🔬 NEW FUNCTION: Apply simulation-based component selection using unified simulation results
/// This is where the magic happens - simulation data drives component specifications!
fn apply_simulation_based_component_selection(
    netlist: &mut Netlist,
    instance_id: InstanceId,
    component_name: &str,
    analysis: &AnalysisResult,
) {
    use crate::passive_component_calculator::PassiveComponentCalculator;
    
    println!("🎯 Applying simulation-based component selection for: {}", component_name);
    
    // Initialize the passive component calculator
    let calculator = PassiveComponentCalculator::new();
    
    // Get component type to determine calculation method
    let component_type = if let Some(instance) = netlist.instances.get(instance_id) {
        if let Some(module) = netlist.modules.get(instance.definition) {
            module.name.to_lowercase()
        } else {
            component_name.to_lowercase()
        }
    } else {
        component_name.to_lowercase()
    };
    
    println!("🔍 Component type detected: {}", component_type);
    
    // Apply simulation-based calculations based on component type
    if component_type.contains("res") || component_type.contains("resistor") {
        apply_simulation_based_resistor_spec(netlist, instance_id, component_name, analysis, &calculator);
    } else if component_type.contains("cap") || component_type.contains("capacitor") {
        apply_simulation_based_capacitor_spec(netlist, instance_id, component_name, analysis, &calculator);
    } else {
        // For other components, apply general simulation-derived attributes
        apply_general_simulation_attributes(netlist, instance_id, component_name, analysis);
    }
    
    // Always apply safety and thermal derating factors
    apply_derating_factors(netlist, instance_id, component_name, analysis);
}

/// Apply simulation-based resistor specifications
fn apply_simulation_based_resistor_spec(
    netlist: &mut Netlist,
    instance_id: InstanceId,
    component_name: &str,
    analysis: &AnalysisResult,
    calculator: &crate::passive_component_calculator::PassiveComponentCalculator,
) {
    println!("🔧 Calculating simulation-based resistor specifications for: {}", component_name);
    
    // Use the simulation-based calculation method
    match calculator.calculate_resistor_spec_from_simulation(component_name, analysis, None) {
        Ok((power_rating, voltage_rating, optimal_resistance)) => {
            println!("✅ SIMULATION SUCCESS: Resistor specs calculated from real simulation data");
            println!("   - Optimal resistance: {:.1}Ω", optimal_resistance);
            println!("   - Power rating: {}", power_rating);
            println!("   - Voltage rating: {}", voltage_rating);
            
            // Store simulation-derived specifications in netlist attributes
            if let Some(instance) = netlist.instances.get_mut(instance_id) {
                instance.attributes.insert("sim_resistance".to_string(), format!("{:.1}", optimal_resistance));
                instance.attributes.insert("sim_power_rating".to_string(), power_rating.to_string());
                instance.attributes.insert("sim_voltage_rating".to_string(), voltage_rating.to_string());
                instance.attributes.insert("calculation_method".to_string(), "simulation_based".to_string());
                
                // Extract actual operating conditions from simulation
                if let Some(actual_current) = analysis.simulation_data.get_operating_current(component_name) {
                    instance.attributes.insert("sim_operating_current".to_string(), format!("{:.3}A", actual_current));
                }
                if let Some(actual_voltage) = analysis.simulation_data.get_operating_voltage(component_name) {
                    instance.attributes.insert("sim_operating_voltage".to_string(), format!("{:.2}V", actual_voltage));
                }
                if let Some(actual_power) = analysis.simulation_data.get_power_dissipation(component_name) {
                    instance.attributes.insert("sim_power_dissipation".to_string(), format!("{:.3}W", actual_power));
                }
                
                println!("✅ Simulation-derived resistor attributes stored in netlist");
            }
        }
        Err(e) => {
            println!("⚠️  Failed to calculate simulation-based resistor spec: {}", e);
            println!("   Falling back to basic parameter extraction");
        }
    }
}

/// Apply simulation-based capacitor specifications
fn apply_simulation_based_capacitor_spec(
    netlist: &mut Netlist,
    instance_id: InstanceId,
    component_name: &str,
    analysis: &AnalysisResult,
    calculator: &crate::passive_component_calculator::PassiveComponentCalculator,
) {
    println!("🔧 Calculating simulation-based capacitor specifications for: {}", component_name);
    
    // Use the simulation-based calculation method
    match calculator.calculate_capacitor_spec_from_simulation(component_name, analysis, None) {
        Ok((voltage_rating, dielectric, max_esr)) => {
            println!("✅ SIMULATION SUCCESS: Capacitor specs calculated from real simulation data");
            println!("   - Voltage rating: {}", voltage_rating);
            println!("   - Dielectric: {:?}", dielectric);
            println!("   - Max ESR: {:.3}Ω", max_esr);
            
            // Store simulation-derived specifications in netlist attributes
            if let Some(instance) = netlist.instances.get_mut(instance_id) {
                instance.attributes.insert("sim_voltage_rating".to_string(), voltage_rating.to_string());
                instance.attributes.insert("sim_dielectric".to_string(), format!("{:?}", dielectric));
                instance.attributes.insert("sim_max_esr".to_string(), format!("{:.3}", max_esr));
                instance.attributes.insert("calculation_method".to_string(), "simulation_based".to_string());
                
                // Extract actual operating conditions from simulation
                if let Some(actual_voltage) = analysis.simulation_data.get_operating_voltage(component_name) {
                    instance.attributes.insert("sim_operating_voltage".to_string(), format!("{:.2}V", actual_voltage));
                }
                
                // Add thermal analysis results if available
                if let Some(ref thermal) = analysis.simulation_data.thermal_analysis {
                    if let Some(operating_temp) = thermal.component_temperatures.get(component_name) {
                        instance.attributes.insert("sim_operating_temperature".to_string(), format!("{:.1}°C", operating_temp));
                        
                        // Thermal derating factor
                        if let Some(thermal_derating) = thermal.thermal_derating_factors.get(component_name) {
                            instance.attributes.insert("sim_thermal_derating".to_string(), format!("{:.2}", thermal_derating));
                        }
                    }
                }
                
                println!("✅ Simulation-derived capacitor attributes stored in netlist");
            }
        }
        Err(e) => {
            println!("⚠️  Failed to calculate simulation-based capacitor spec: {}", e);
            println!("   Falling back to basic parameter extraction");
        }
    }
}

/// Apply general simulation-derived attributes for any component type
fn apply_general_simulation_attributes(
    netlist: &mut Netlist,
    instance_id: InstanceId,
    component_name: &str,
    analysis: &AnalysisResult,
) {
    println!("🔧 Applying general simulation attributes for: {}", component_name);
    
    if let Some(instance) = netlist.instances.get_mut(instance_id) {
        // Add operating conditions from DC analysis
        if let Some(ref dc_analysis) = analysis.simulation_data.dc_analysis {
            if let Some(voltage) = dc_analysis.node_voltages.get(component_name) {
                instance.attributes.insert("sim_node_voltage".to_string(), format!("{:.3}V", voltage));
            }
            if let Some(current) = dc_analysis.branch_currents.get(component_name) {
                instance.attributes.insert("sim_branch_current".to_string(), format!("{:.3}A", current));
            }
            if let Some(power) = dc_analysis.power_dissipation.get(component_name) {
                instance.attributes.insert("sim_power_dissipation".to_string(), format!("{:.3}W", power));
            }
            if let Some(temp) = dc_analysis.operating_temperatures.get(component_name) {
                instance.attributes.insert("sim_operating_temperature".to_string(), format!("{:.1}°C", temp));
            }
        }
        
        // Mark as simulation-enhanced
        instance.attributes.insert("simulation_enhanced".to_string(), "true".to_string());
        
        println!("✅ General simulation attributes applied");
    }
}

/// Apply derating factors from electrical safety and thermal analysis
fn apply_derating_factors(
    netlist: &mut Netlist,
    instance_id: InstanceId,
    component_name: &str,
    analysis: &AnalysisResult,
) {
    println!("🛡️  Applying safety and thermal derating factors for: {}", component_name);
    
    if let Some(instance) = netlist.instances.get_mut(instance_id) {
        // Apply electrical safety derating
        if let Some(ref electrical_safety) = analysis.simulation_data.electrical_safety {
            if let Some(stress_analysis) = electrical_safety.component_stress.get(component_name) {
                println!("   - Electrical stress analysis found");
                
                // Store stress ratios
                instance.attributes.insert("stress_voltage_ratio".to_string(), format!("{:.2}", stress_analysis.voltage_stress_ratio));
                instance.attributes.insert("stress_current_ratio".to_string(), format!("{:.2}", stress_analysis.current_stress_ratio));
                instance.attributes.insert("stress_power_ratio".to_string(), format!("{:.2}", stress_analysis.power_stress_ratio));
                instance.attributes.insert("stress_thermal_ratio".to_string(), format!("{:.2}", stress_analysis.thermal_stress_ratio));
                
                // Store stress flags
                if stress_analysis.has_voltage_stress || stress_analysis.has_current_stress || stress_analysis.has_thermal_stress {
                    instance.attributes.insert("has_stress_issues".to_string(), "true".to_string());
                    
                    let mut stress_types = Vec::new();
                    if stress_analysis.has_voltage_stress { stress_types.push("voltage"); }
                    if stress_analysis.has_current_stress { stress_types.push("current"); }
                    if stress_analysis.has_thermal_stress { stress_types.push("thermal"); }
                    
                    instance.attributes.insert("stress_types".to_string(), stress_types.join(","));
                    
                    println!("   ⚠️  Stress issues detected: {}", stress_types.join(", "));
                }
                
                // Store derating recommendations
                if !stress_analysis.derating_recommendations.is_empty() {
                    let mut recommendations = Vec::new();
                    for rec in &stress_analysis.derating_recommendations {
                        recommendations.push(format!("{}:{:.2}->{:.2}", rec.parameter, rec.current_value, rec.recommended_value));
                    }
                    instance.attributes.insert("derating_recommendations".to_string(), recommendations.join(";"));
                    
                    println!("   📋 Derating recommendations: {}", recommendations.len());
                }
            }
        }
        
        // Apply comprehensive derating factor
        let comprehensive_derating = analysis.simulation_data.get_derating_factor(component_name);
        if comprehensive_derating < 1.0 {
            instance.attributes.insert("comprehensive_derating_factor".to_string(), format!("{:.3}", comprehensive_derating));
            instance.attributes.insert("additional_derating_needed".to_string(), format!("{:.1}%", (1.0 - comprehensive_derating) * 100.0));
            
            println!("   🎯 Comprehensive derating factor: {:.1}% (additional {:.1}% derating required)", 
                     comprehensive_derating * 100.0, (1.0 - comprehensive_derating) * 100.0);
        }
        
        println!("✅ Safety and thermal derating factors applied");
    }
}

/// Compatibility alias for NetlistGenerator
/// This provides backwards compatibility for existing code
pub type Synthesizer = NetlistGenerator;

/*
/// Load synthesis knowledge for a component from the stdlib
fn load_synthesis_knowledge_for_component(component_type: &str) -> Result<SynthesisKnowledge> {
    // Implementation commented out for compilation - see test_intent_aware_synthesis.rs
    Err(anyhow::anyhow!("Not implemented"))
}

/// Extract parameters from a netlist instance for synthesis calculations
fn extract_instance_parameters(netlist: &Netlist, instance_id: InstanceId) -> HashMap<String, String> {
    HashMap::new()
}

/// Hardcoded TPS54331 synthesis knowledge for demonstration
fn create_tps54331_synthesis_knowledge() -> SynthesisKnowledge {
    // Implementation moved to test_intent_aware_synthesis.rs
    
    let mut virtual_pin_expansions = HashMap::new();
    
    // VOUT pin expansion - this is where the magic happens
    let vout_expansion = VirtualPinExpansion {
        pin_name: "VOUT".to_string(),
        components: vec![
            SynthesisComponent {
                reference_designator: "L1".to_string(),
                component_type: "Inductor".to_string(), 
                value: ComponentValue::Calculated {
                    formula: "L = (Vout × (Vin - Vout)) / (ΔI × f × Vin)".to_string(),
                    context: HashMap::new(),
                },
                intent: "power_filtering(ripple_target: 30%, efficiency_priority: high)".to_string(),
                placement_constraints: None,
            },
            SynthesisComponent {
                reference_designator: "C_BOOT".to_string(),
                component_type: "Cap".to_string(),
                value: ComponentValue::Fixed("100nF".to_string()),
                intent: "bootstrap_timing(rise_time: 50ns, hold_time: 2µs, switching_freq: 570kHz)".to_string(),
                placement_constraints: None,
            },
            SynthesisComponent {
                reference_designator: "C_OUT1".to_string(),
                component_type: "Cap".to_string(),
                value: ComponentValue::Fixed("22µF".to_string()),
                intent: "power_decoupling(esr_target: low, ripple_reduction: 80%)".to_string(),
                placement_constraints: None,
            },
            SynthesisComponent {
                reference_designator: "C_OUT2".to_string(),
                component_type: "Cap".to_string(),
                value: ComponentValue::Fixed("22µF".to_string()),
                intent: "power_decoupling(esr_target: low, ripple_reduction: 80%)".to_string(),
                placement_constraints: None,
            },
            SynthesisComponent {
                reference_designator: "R_FB1".to_string(),
                component_type: "Res".to_string(),
                value: ComponentValue::Calculated {
                    formula: "R1 = R2 × (Vout/0.8 - 1)".to_string(),
                    context: HashMap::new(),
                },
                intent: "feedback_control(target_voltage: vout, accuracy: 1%)".to_string(),
                placement_constraints: None,
            },
            SynthesisComponent {
                reference_designator: "R_FB2".to_string(),
                component_type: "Res".to_string(),
                value: ComponentValue::Fixed("10kΩ".to_string()),
                intent: "feedback_control(target_voltage: vout, accuracy: 1%)".to_string(),
                placement_constraints: None,
            },
        ],
        connections: vec![
            SynthesisConnection {
                from: "TPS54331.SW".to_string(),
                to: "L1.1".to_string(),
                connection_type: "power".to_string(),
            },
            SynthesisConnection {
                from: "L1.2".to_string(),
                to: "VOUT".to_string(),
                connection_type: "power".to_string(),
            },
            SynthesisConnection {
                from: "TPS54331.BOOT".to_string(),
                to: "C_BOOT.1".to_string(),
                connection_type: "signal".to_string(),
            },
            SynthesisConnection {
                from: "C_BOOT.2".to_string(),
                to: "TPS54331.SW".to_string(),
                connection_type: "signal".to_string(),
            },
        ],
        intents: HashMap::new(),
    };
    
    virtual_pin_expansions.insert("VOUT".to_string(), vout_expansion);
    
    // Mandatory components - these are required regardless of virtual pins
    let mandatory_components = vec![
        MandatoryComponent {
            reference_designator: "C_IN".to_string(),
            component_type: "Cap".to_string(),
            value: ComponentValue::Fixed("100µF".to_string()),
            connection: "VIN -> C_IN.1, C_IN.2 -> GND".to_string(),
            intent: "power_filtering(frequency: switching, stabilization: input_voltage)".to_string(),
        },
        MandatoryComponent {
            reference_designator: "D_CATCH".to_string(),
            component_type: "SS34".to_string(),
            value: ComponentValue::Fixed("SS34".to_string()),
            connection: "GND -> D_CATCH.A, D_CATCH.K -> SW".to_string(),
            intent: "input_protection(reverse_current: block, efficiency_loss: minimize)".to_string(),
        },
    ];
    
    SynthesisKnowledge {
        component_name: "TPS54331".to_string(),
        virtual_pin_expansions,
        mandatory_components,
        calculation_formulas: HashMap::new(),
        connection_requirements: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_netlist_generator_creation() {
        let generator = NetlistGenerator::new();
        assert_eq!(generator.netlist.modules.len(), 0);
        assert_eq!(generator.netlist.instances.len(), 0);
        assert_eq!(generator.netlist.nets.len(), 0);
    }

    #[test]
    fn test_netlist_config_default() {
        let config = NetlistConfig::default();
        assert!(config.preserve_semantic_context);
        assert!(config.include_power_domains);
        assert!(config.include_component_inference);
    }

    #[test]
    fn test_component_type_mapping() {
        let generator = NetlistGenerator::new();
        
        assert_eq!(
            generator.map_component_type_to_module_kind("voltage_regulator"),
            ModuleKind::Component
        );
        
        assert_eq!(
            generator.map_component_type_to_module_kind("resistor"),
            ModuleKind::PhysicalComponent
        );
        
        assert_eq!(
            generator.map_component_type_to_module_kind("opamp"),
            ModuleKind::Component
        );
    }
}

*/
