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
pub mod hierarchical_refdes;

// Interface synthesis
pub mod interface_synthesis;

// Passive component calculation engine
pub mod passive_component_calculator;

// Package selection engine
pub mod package_selector;

// GLACIER-driven component physical selection
pub mod glacier_physical_selection;

// Import loader for handling BHDL imports
pub mod import_loader;

// Import preprocessor for pre-processing imports before analysis
pub mod import_preprocessor;

// Synthesis knowledge parser and storage
pub mod synthesis_knowledge;

// Virtual pin extraction from AST
pub mod virtual_pin_extractor;

// Virtual pin expansion (post-synthesis wiring of inductor/diode/cap)
pub mod virtual_pin_expander;

// Ripple-aware multi-tier capacitor bank computation
pub mod ripple_calculator;

// Intent attribute stamper (bridges FlowTracker intents → netlist attributes)
pub mod intent_attribute_stamper;

// Input capacitor bank physics computation
pub mod input_cap_calculator;

// Post-GLACIER input capacitor sizing pass
pub mod input_cap_sizer;

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
    /// Flatten hierarchical designs to top-level
    pub flatten_hierarchy: bool,
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

/// Represents different types of connection endpoints
#[derive(Debug, Clone)]
enum ConnectionEndpoint {
    /// Simple net name (VCC, GND, etc.)
    Net(String),
    /// Component pin (instance.pin)
    Pin(String, String), // (instance_name, pin_name)
    /// Named handle declaration (C1: Cap(...))
    NamedHandle(String, String), // (handle_name, component_type)
    /// Net assignment with component (net_name: Component(...).pin)
    NetAssignment(String, String, String), // (net_name, component_type, pin_name),
    /// Net reference with @ prefix (@NETNAME)
    NetRef(String), // net_name without @ prefix
}

impl Default for NetlistConfig {
    fn default() -> Self {
        Self {
            preserve_semantic_context: true,
            include_power_domains: true,
            include_component_inference: true,
            flatten_hierarchy: false,
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
        }
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
    // Mapping from net assignment handles to instance IDs
    // When we see "protected_vin: TVSDiode(15V).K", we map "protected_vin" -> TVSDiode instance
    net_assignment_handles: HashMap<String, InstanceId>,
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
    pub fn with_config(config: NetlistConfig) -> Self {
        
        // Initialize import loader with current directory as base
        let import_loader = ImportLoader::new(".");
        
        Self {
            config,
            netlist: Netlist::new(),
            ast_to_module: HashMap::new(),
            ast_to_instance: HashMap::new(),
            ast_to_net: HashMap::new(),
            net_assignment_handles: HashMap::new(),
            database_mapper: None, // Will be initialized async in generate_from_analysis
            component_instances: Vec::new(),
            type_mapper: ComponentTypeMapper::new(),
            import_loader,
            import_preprocessor: None,
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

    /// Extract virtual pin components from an entity AST node
    fn extract_virtual_pins_from_entity(&self, entity: &bhdl_ast::Entity) -> Option<Vec<bhdl_stdlib::virtual_pins::VirtualPinComponent>> {
        use crate::virtual_pin_extractor::VirtualPinExtractor;

        // Use the new virtual pin extractor to parse the entity
        VirtualPinExtractor::extract_from_entity(entity)
    }
    
    /// Process virtual pin components extracted from AST
    fn process_virtual_pin_components(
        &mut self, 
        virtual_components: &[bhdl_stdlib::virtual_pins::VirtualPinComponent], 
        instance_name: &str, 
        component_type: &str
    ) -> Result<()> {
        info!("Processing {} virtual pin components for {} ({})", 
              virtual_components.len(), instance_name, component_type);
        
        // Convert VirtualPinComponent to CalculatedComponent for compatibility
        for component in virtual_components {
            let calc_component = crate::component_calculator::CalculatedComponent {
                component_type: match component.component_type.as_str() {
                    "Inductor" => crate::component_calculator::ComponentType::Inductor,
                    "Capacitor" => crate::component_calculator::ComponentType::Capacitor,
                    "Resistor" => crate::component_calculator::ComponentType::Resistor,
                    "Diode" => crate::component_calculator::ComponentType::Diode,
                    "LED" => crate::component_calculator::ComponentType::LED,
                    _ => crate::component_calculator::ComponentType::Capacitor, // Default
                },
                reference: component.reference.clone(),
                value: component.value.clone(),
                rating: component.specs.get("voltage_rating")
                    .or_else(|| component.specs.get("current_rating"))
                    .or_else(|| component.specs.get("power_rating"))
                    .unwrap_or(&"".to_string()).clone(),
                package: component.specs.get("package")
                    .unwrap_or(&"0805".to_string()).clone(),
                purpose: format!("Virtual pin component for {}", component_type),
                calculation: "Extracted from module virtual pins".to_string(),
                placement: "Near parent component".to_string(),
                intent: match component.component_type.as_str() {
                    "Capacitor" => crate::component_calculator::ComponentIntent::InputFiltering,
                    "Resistor" => crate::component_calculator::ComponentIntent::CurrentLimiting,
                    "Inductor" => crate::component_calculator::ComponentIntent::EnergyStorage,
                    _ => crate::component_calculator::ComponentIntent::Decoupling,
                },
            };
            
            // Add the component to the netlist
            self.add_virtual_pin_component(instance_name, &calc_component)?;
        }
        
        Ok(())
    }
    
    /// Add a virtual pin component to the netlist
    fn add_virtual_pin_component(&mut self, parent_instance: &str, component: &crate::component_calculator::CalculatedComponent) -> Result<()> {
        // Generate a unique reference for the supporting component
        let supporting_ref = format!("{}_{}", parent_instance, component.reference);
        
        // Create a module for the supporting component type
        let module_id = self.netlist.add_module(
            format!("{:?}", component.component_type),
            bhdl_netlist::types::ModuleKind::Component
        );
        
        // Create an instance of the supporting component
        let instance_id = self.netlist.add_instance(
            supporting_ref.clone(),
            module_id
        );
        
        // Track the supporting component instance
        if let Some(instance_id) = instance_id {
            self.supporting_component_instances.insert(supporting_ref, instance_id);
            info!("Added virtual pin component: {} ({:?})", component.reference, component.component_type);
        } else {
            warn!("Failed to create instance for virtual pin component: {}", component.reference);
        }
        
        Ok(())
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
            println!("DEBUG: Attempting to initialize database mapper");
            if let Err(e) = self.initialize_database_mapper().await {
                warn!("Failed to initialize database mapper: {}", e);
                println!("DEBUG: Database mapper initialization failed: {}", e);
                // Continue without database mapper - will use fallback
            } else {
                println!("DEBUG: Database mapper initialized successfully");
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
            println!("DEBUG: Using database component mapper for instance generation");
            println!("DEBUG: About to call generate_database_component_instances");
            let result = self.generate_database_component_instances(analysis, ast).await;
            println!("DEBUG: Returned from generate_database_component_instances with result: {:?}", result.is_ok());
            result?;
        } else {
            // Fallback to semantic instance generation if database unavailable
            println!("DEBUG: Using semantic instance generation (no database mapper)");
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

        // Final phase: Populate power symbol database entries for visualization
        self.populate_power_symbol_components(analysis).await?;

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
        
        // Use flat extraction for now if hierarchy is to be flattened
        if self.config.flatten_hierarchy {
            // Create a basic top-level module for flat designs
            let top_module_id = self.netlist.add_module(
                "top_level".to_string(), 
                ModuleKind::Board
            );
            self.netlist.top_level_module = Some(top_module_id);
            self.ast_to_module.insert("top_level".to_string(), top_module_id);
            debug!("Created top-level module for flat design: {:?}", top_module_id);
        } else {
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
                    println!("DEBUG: Calling populate_instance_attributes for instance '{}' (id: {:?})", instance_name, instance_id);
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
        for (_node_ptr, scope) in &analysis.definition_scopes {
            for (name, symbol) in scope.get_symbols() {
                all_symbols.insert(name.clone(), symbol.clone());
            }
        }
        
        for (name, symbol) in &all_symbols {
            if matches!(symbol.kind, bhdl_analyzer::symbol_table::SymbolKind::Instance) {
                debug!("Processing component instance: {} of kind {:?}", name, symbol.kind);

                // Extract component type from the symbol's instance_type_name
                if let Some(ref type_name) = symbol.instance_type_name {
                    debug!("Component {} is of type: {}", name, type_name);

                    // Create module for the component type if it doesn't exist
                    let module_id = self.get_or_create_module(type_name, ModuleKind::Component)?;
                    
                    // Create instance of the component using correct API
                    let instance_id = self.netlist.add_instance(name.clone(), module_id);
                    
                    if let Some(instance_id) = instance_id {
                        debug!("Created component instance: {} -> {:?}", name, instance_id);

                        // Add pins for the component based on database or default pins
                        if let Err(e) = self.add_pins_for_component(name, type_name, module_id) {
                            warn!("Failed to add pins for component {}: {}", name, e);
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
        
        // Phase 2: Automatic Supporting Component Generation
        // For power management ICs like TPS54331, automatically generate supporting components
        println!("🔧 MAIN SYNTHESIZER: Starting automatic supporting component generation");
        info!("AUTOMATIC COMPONENT GENERATION: Starting automatic supporting component generation");
        self.generate_automatic_supporting_components(&all_symbols, analysis, ast).await?;
        
        Ok(())
    }

    /// Generate automatic supporting components from BHDL synthesis knowledge
    async fn generate_automatic_supporting_components(&mut self, all_symbols: &HashMap<String, bhdl_analyzer::symbol_table::Symbol>, analysis: &AnalysisResult, ast: Option<&SourceFile>) -> Result<()> {
        debug!("Starting automatic supporting component generation for {} symbols", all_symbols.len());
        
        // Process ALL components that have virtual pins defined in BHDL
        for (name, symbol) in all_symbols {
            if matches!(symbol.kind, bhdl_analyzer::symbol_table::SymbolKind::Instance) {
                if let Some(ref type_name) = symbol.instance_type_name {
                    // Check if the component is defined in the analyzer's symbol table (from imports)
                    let has_entity_definition = analysis.global_scope.lookup(type_name)
                        .map(|s| s.kind == bhdl_analyzer::symbol_table::SymbolKind::Entity)
                        .unwrap_or(false);

                    if has_entity_definition {
                        println!("✅ SYNTHESIZER: Found entity definition for {} in symbol table", type_name);
                        info!("Component {} found in analyzer symbol table", type_name);
                        
                        // Try to extract component information directly from the AST node in the symbol table
                        let mut virtual_components_from_ast: Option<Vec<bhdl_stdlib::virtual_pins::VirtualPinComponent>> = None;
                        
                        if let Some(module_symbol) = analysis.global_scope.lookup(type_name) {
                            if let Some(syntax_node_ptr) = &module_symbol.definition_node_ptr {
                                // Try to resolve the AST node - first from main file, then from imports
                                let mut resolved_node = None;
                                
                                // First try the main AST file
                                if let Some(ast_root) = ast {
                                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                        syntax_node_ptr.to_node(&ast_root.syntax())
                                    })) {
                                        Ok(node) => resolved_node = Some(node),
                                        Err(_) => {
                                            debug!("Node not in main AST, checking imported files");
                                        }
                                    }
                                }
                                
                                // If not found in main file, try imported files
                                if resolved_node.is_none() {
                                    for (path, imported_ast) in self.import_loader.loaded_source_files() {
                                        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                            syntax_node_ptr.to_node(&imported_ast.syntax())
                                        })) {
                                            Ok(node) => {
                                                info!("Found {} in imported file: {}", type_name, path);
                                                resolved_node = Some(node);
                                                break;
                                            }
                                            Err(_) => continue,
                                        }
                                    }
                                }
                                
                                // Extract virtual pins if we found the node
                                if let Some(syntax_node) = resolved_node {
                                    if let Some(entity_node) = bhdl_ast::Entity::cast(syntax_node) {
                                        info!("Successfully resolved {} AST node from symbol table", type_name);
                                        virtual_components_from_ast = self.extract_virtual_pins_from_entity(&entity_node);
                                    }
                                } else {
                                    warn!("Could not resolve AST node for {} - may need to load more imports", type_name);
                                }
                            }
                        }
                        
                        // Process virtual components from AST if available
                        if let Some(ast_components) = virtual_components_from_ast {
                            info!("Using AST-extracted virtual pins for {} - {} components", type_name, ast_components.len());
                            self.process_virtual_pin_components(&ast_components, name, type_name)?;
                        } else {
                            // AST-based extraction is the only supported method now
                            info!("No virtual pins extracted for {} - component may not define virtual pins", type_name);
                        }
                    } else {
                        debug!("Component {} ({}) has no virtual pins in BHDL", name, type_name);
                    }
                }
            }
        }
        
        Ok(())
    }

    // REMOVED: is_power_management_ic() - We should not hardcode component types
    // All component knowledge should come from BHDL files

    /// Extract power specifications from analysis results for a given IC
    async fn extract_power_specifications(&self, analysis: &AnalysisResult, ic_type: &str) -> Result<Option<PowerSupplySpec>> {
        // Extract power domain information from the analysis
        // Look for power declarations like "power VIN = 24V @ 3A;" and "power VOUT = 12V @ 2.5A;"
        
        debug!("Extracting power specifications for IC: {}", ic_type);
        
        let mut input_voltage = None;
        let mut output_voltage = None;
        let mut output_current = None;
        
        // Search through all symbols for power domains (both global scope and definition scopes)
        let mut all_symbols = analysis.global_scope.get_symbols().clone();
        
        // Add symbols from all definition scopes (power domains are stored in board definition scope)
        for (_node_ptr, scope) in &analysis.definition_scopes {
            for (name, symbol) in scope.get_symbols() {
                all_symbols.insert(name.clone(), symbol.clone());
            }
            
            // Check if this scope has nets separately (nets have their own namespace)
            for (name, symbol) in scope.get_nets() {
                all_symbols.insert(name.clone(), symbol.clone());
            }
        }
        
        debug!("Searching for power domains in {} symbols", all_symbols.len());
        
        for (name, symbol) in &all_symbols {
            if matches!(symbol.kind, bhdl_analyzer::symbol_table::SymbolKind::Net) {
                if let Some(ref net_attribute) = symbol.net_attributes {
                    // Check for input power domain (VIN, VCC, etc.)
                    let name_upper = name.to_uppercase();
                    if name_upper.contains("VIN") || name_upper.contains("INPUT") {
                        if let Some(voltage) = net_attribute.voltage() {
                            input_voltage = Some(voltage);
                            debug!("Found input voltage: {}V", voltage);
                        }
                    }
                    // Check for output power domain (VOUT, etc.)
                    else if name_upper.contains("VOUT") || name_upper.contains("OUTPUT") {
                        if let Some(voltage) = net_attribute.voltage() {
                            output_voltage = Some(voltage);
                            debug!("Found output voltage: {}V", voltage);
                        }
                        if let Some(current) = net_attribute.max_current() {
                            output_current = Some(current);
                            debug!("Found output current: {}A", current);
                        }
                    }
                }
            }
        }
        
        // Create power specification if we have the required information
        if let (Some(v_in), Some(v_out), Some(i_out)) = (input_voltage, output_voltage, output_current) {
            let power_spec = PowerSupplySpec {
                input_voltage: v_in,
                output_voltage: v_out,
                output_current: i_out,
                switching_frequency: self.get_default_switching_frequency(ic_type),
                ripple_spec: 0.100,      // 100mVpp (conservative spec)
                transient_spec: 100.0,   // 100µs (conservative spec)
                efficiency_target: self.get_default_efficiency(ic_type),
            };
            
            Ok(Some(power_spec))
        } else {
            debug!("Insufficient power domain information - VIN: {:?}, VOUT: {:?}, IOUT: {:?}", 
                   input_voltage, output_voltage, output_current);
            Ok(None)
        }
    }

    /// Get default switching frequency for a given IC type
    fn get_default_switching_frequency(&self, ic_type: &str) -> f64 {
        let type_lower = ic_type.to_lowercase();
        
        if type_lower.contains("tps543") {
            400_000.0  // 400kHz for TPS54331
        } else if type_lower.contains("lm2596") {
            150_000.0  // 150kHz for LM2596
        } else {
            500_000.0  // 500kHz default for modern switchers
        }
    }

    /// Get default efficiency for a given IC type
    fn get_default_efficiency(&self, ic_type: &str) -> f64 {
        let type_lower = ic_type.to_lowercase();
        
        if type_lower.contains("tps543") {
            0.91  // 91% for TPS54331
        } else if type_lower.contains("lm2596") {
            0.85  // 85% for LM2596 
        } else if type_lower.contains("7805") || type_lower.contains("lm317") {
            0.60  // 60% for linear regulators (poor efficiency)
        } else {
            0.88  // 88% default for modern switchers
        }
    }

    /// Add a calculated component to the netlist
    fn add_calculated_component_to_netlist(&mut self, component: &crate::component_calculator::CalculatedComponent, ic_name: &str) -> Result<()> {
        // Create module for the component type
        let component_type = format!("{:?}", component.component_type); // Convert enum to string
        let module_id = self.get_or_create_module(&component_type, ModuleKind::Component)?;
        
        // Create instance with calculated reference designator
        let instance_name = format!("{}_{}", ic_name, component.reference); // e.g., "U1_C1"
        
        if let Some(instance_id) = self.netlist.add_instance(instance_name.clone(), module_id) {
            debug!("Created calculated component: {} -> {:?} ({})", instance_name, instance_id, component.value);
            
            // Store the instance ID for later connection generation
            self.supporting_component_instances.insert(instance_name.clone(), instance_id);
            
            // Add component metadata as annotations
            // The visualizer can read these annotations to understand component values and purposes
            // TODO: Add proper metadata storage to netlist when available
            
        } else {
            warn!("Failed to create instance for calculated component: {}", instance_name);
        }
        
        Ok(())
    }

    /// Generate connections for supporting components from virtual pin expansion
    fn generate_supporting_component_connections(&mut self, analysis: &AnalysisResult) -> Result<()> {
        if self.supporting_component_instances.is_empty() {
            info!("No supporting components to connect");
            return Ok(()); // No supporting components to connect
        }
        
        info!("Generating connections for {} supporting components", self.supporting_component_instances.len());
        for (name, id) in &self.supporting_component_instances {
            info!("  Supporting component: {} -> {:?}", name, id);
        }
        
        // Group components by the IC they belong to.
        // Prefer vpin_parent attribute; fall back to name prefix for legacy components.
        let mut components_by_ic: HashMap<String, Vec<(String, InstanceId)>> = HashMap::new();
        for (name, id) in &self.supporting_component_instances {
            let ic_prefix = self.netlist.instances.get(*id)
                .and_then(|inst| inst.attributes.get("vpin_parent").cloned())
                .or_else(|| name.split('_').next().map(|s| s.to_string()));
            if let Some(prefix) = ic_prefix {
                components_by_ic.entry(prefix)
                    .or_insert_with(Vec::new)
                    .push((name.clone(), *id));
            }
        }

        // Create connections for each IC's supporting components
        for (ic_name, components) in components_by_ic {
            info!("Creating connections for IC {} supporting components ({} components)", ic_name, components.len());

            // Find relevant nets - we need SW, VOUT, GND, FB nets
            let sw_net = self.find_or_create_net(&format!("{}_SW", ic_name), NetClass::Signal);
            let vout_net = self.find_or_create_net("VOUT", NetClass::Power(5.0)); // TODO: Get actual voltage
            let gnd_net = self.find_or_create_net("GND", NetClass::Ground);
            let fb_net = self.find_or_create_net(&format!("{}_FB", ic_name), NetClass::Signal);

            for (comp_name, comp_id) in components {
                // Get the module for this component to create pins
                let module_id = if let Some(inst) = self.netlist.instances.get(comp_id) {
                    inst.definition
                } else {
                    continue;
                };

                // Determine component role from attributes, then fall back to name parsing.
                let (comp_class, comp_role) = {
                    let inst = self.netlist.instances.get(comp_id);
                    let class = inst.and_then(|i| i.attributes.get("component_class").cloned());
                    let role = inst.and_then(|i| i.attributes.get("vpin_role").cloned());
                    (class.unwrap_or_default(), role.unwrap_or_default())
                };

                // Classify by component_class + vpin_role, fall back to name heuristic
                let is_inductor = comp_class == "inductor" || comp_name.starts_with("L");
                let is_capacitor = comp_class == "capacitor" || comp_name.starts_with("C");
                let is_resistor = comp_class == "resistor" || comp_name.starts_with("R");
                let is_diode = comp_class == "diode" || comp_name.starts_with("D");

                if is_inductor && comp_role != "shunt" {
                    // Inductor (series): SW -> IN, OUT -> VOUT
                    info!("Connecting inductor {}", comp_name);
                    let pin1 = self.get_or_create_pin(module_id, "IN", PinDirection::In);
                    let pin2 = self.get_or_create_pin(module_id, "OUT", PinDirection::Out);

                    let pin_inst1 = self.netlist.pin_instances.insert_with_key(|id| PinInstance {
                        id,
                        pin_def: pin1,
                        instance: comp_id,
                        net: Some(sw_net),
                        connection_name: None,
                    });
                    let pin_inst2 = self.netlist.pin_instances.insert_with_key(|id| PinInstance {
                        id,
                        pin_def: pin2,
                        instance: comp_id,
                        net: Some(vout_net),
                        connection_name: None,
                    });

                    self.netlist.connect(sw_net, ConnectionPoint::PinInstance(pin_inst1)).ok();
                    self.netlist.connect(vout_net, ConnectionPoint::PinInstance(pin_inst2)).ok();
                } else if is_capacitor {
                    // Capacitor: VOUT -> pin 1, pin 2 -> GND
                    info!("Connecting capacitor {}", comp_name);
                    let pin1 = self.get_or_create_pin(module_id, "1", PinDirection::In);
                    let pin2 = self.get_or_create_pin(module_id, "2", PinDirection::In);

                    let pin_inst1 = self.netlist.pin_instances.insert_with_key(|id| PinInstance {
                        id,
                        pin_def: pin1,
                        instance: comp_id,
                        net: Some(vout_net),
                        connection_name: None,
                    });
                    let pin_inst2 = self.netlist.pin_instances.insert_with_key(|id| PinInstance {
                        id,
                        pin_def: pin2,
                        instance: comp_id,
                        net: Some(gnd_net),
                        connection_name: None,
                    });

                    self.netlist.connect(vout_net, ConnectionPoint::PinInstance(pin_inst1)).ok();
                    self.netlist.connect(gnd_net, ConnectionPoint::PinInstance(pin_inst2)).ok();
                } else if is_resistor {
                    // Resistor: default VOUT -> pin 1, pin 2 -> GND
                    // (feedback divider wiring would need more context)
                    info!("Connecting resistor {}", comp_name);
                    let pin1 = self.get_or_create_pin(module_id, "1", PinDirection::In);
                    let pin2 = self.get_or_create_pin(module_id, "2", PinDirection::Out);

                    let pin_inst1 = self.netlist.pin_instances.insert_with_key(|id| PinInstance {
                        id,
                        pin_def: pin1,
                        instance: comp_id,
                        net: Some(vout_net),
                        connection_name: None,
                    });
                    let pin_inst2 = self.netlist.pin_instances.insert_with_key(|id| PinInstance {
                        id,
                        pin_def: pin2,
                        instance: comp_id,
                        net: Some(gnd_net),
                        connection_name: None,
                    });

                    self.netlist.connect(vout_net, ConnectionPoint::PinInstance(pin_inst1)).ok();
                    self.netlist.connect(gnd_net, ConnectionPoint::PinInstance(pin_inst2)).ok();
                } else if is_diode {
                    // Diode: GND -> D.A (anode), D.K (cathode) -> SW
                    info!("Connecting diode {}", comp_name);
                    let pin_a = self.get_or_create_pin(module_id, "A", PinDirection::In);
                    let pin_k = self.get_or_create_pin(module_id, "K", PinDirection::Out);

                    let pin_inst_a = self.netlist.pin_instances.insert_with_key(|id| PinInstance {
                        id,
                        pin_def: pin_a,
                        instance: comp_id,
                        net: Some(gnd_net),
                        connection_name: None,
                    });
                    let pin_inst_k = self.netlist.pin_instances.insert_with_key(|id| PinInstance {
                        id,
                        pin_def: pin_k,
                        instance: comp_id,
                        net: Some(sw_net),
                        connection_name: None,
                    });

                    self.netlist.connect(gnd_net, ConnectionPoint::PinInstance(pin_inst_a)).ok();
                    self.netlist.connect(sw_net, ConnectionPoint::PinInstance(pin_inst_k)).ok();
                } else {
                    info!("Unknown supporting component type for {} (class={}, role={})", comp_name, comp_class, comp_role);
                }
            }
        }
        
        Ok(())
    }
    
    /// Find or create a net with the given name and class
    fn find_or_create_net(&mut self, name: &str, net_class: NetClass) -> NetId {
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
    
    /// Get or create a pin for a module
    fn get_or_create_pin(&mut self, module_id: ModuleId, pin_name: &str, direction: PinDirection) -> PinId {
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
        });
        
        // Add pin to module
        if let Some(module) = self.netlist.modules.get_mut(module_id) {
            module.pins.push(pin_id);
        }
        
        pin_id
    }
    
    /// Get or create a module definition for a given component type
    fn get_or_create_module(&mut self, component_type: &str, kind: ModuleKind) -> Result<ModuleId> {
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
    fn extract_connectivity_from_ast(&mut self, ast: &SourceFile, analysis: &AnalysisResult) -> Result<()> {
        use bhdl_ast::{AstNode, SyntaxKind, ConnectionStmt};
        
        info!("Extracting connectivity from AST");
        
        // Always create power nets first - they're needed by both extraction methods
        self.create_power_nets(analysis)?;
        
        if !self.config.flatten_hierarchy {
            // Use hierarchical connectivity extraction
            info!("Using hierarchical connectivity extraction");
            hierarchical_connectivity::extract_hierarchical_connectivity(ast, analysis, &mut self.netlist, self.import_preprocessor.as_ref())?;
        } else {
            // Use flat extraction for backward compatibility
            info!("Using flat connectivity extraction");
            
            // Now traverse the AST to find all connection statements
            let mut connection_count = 0;
            self.visit_connections_in_ast(ast.syntax(), &mut connection_count, analysis)?;
            
            info!("Extracted {} connections from AST", connection_count);
        }
        
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
            
            // Create nets and component instances for all power domains
            for (domain_name, domain_info) in &power_context.domains {
                // Determine the appropriate NetClass based on domain type
                let net_class = if domain_name.contains("GND") || domain_info.voltage == 0.0 {
                    NetClass::Ground
                } else {
                    NetClass::Power(domain_info.voltage)
                };
                
                let net_id = self.find_or_create_net(domain_name, net_class.clone());
                self.ast_to_net.insert(domain_name.clone(), net_id);
                
                debug!("Created power net '{}' with voltage {:?} and class {:?}", 
                       domain_name, domain_info.voltage, net_class);
                
                // NEW: Create component instances for power and ground
                if domain_name.contains("GND") || domain_info.voltage == 0.0 {
                    // Create Ground component instance using KiCad GND symbol
                    let symbol_name = "GND".to_string();
                    let module_id = self.netlist.add_module(
                        symbol_name.clone(),
                        ModuleKind::PhysicalComponent
                    );

                    // Add pin to the module (ground symbols have pin "1")
                    let pin_id = self.netlist.add_pin(
                        module_id,
                        "1".to_string(),
                        PinDirection::InOut,
                        PinType::Ground
                    ).ok_or_else(|| anyhow::anyhow!("Failed to add GND pin"))?;
                    
                    // Create instance
                    let instance_id = self.netlist.add_instance(
                        domain_name.clone(),
                        module_id
                    ).ok_or_else(|| anyhow::anyhow!("Failed to add Ground instance"))?;
                    
                    // Create pin instances for the ground component
                    let pin_instances = self.netlist.create_pin_instances(instance_id)
                        .map_err(|e| anyhow::anyhow!("Failed to create pin instances: {}", e))?;
                    
                    // Connect the first pin instance to the net
                    if let Some(&pin_inst_id) = pin_instances.first() {
                        self.netlist.connect(net_id, ConnectionPoint::PinInstance(pin_inst_id))
                            .map_err(|e| anyhow::anyhow!("Failed to connect ground: {}", e))?;
                    }
                    
                    self.ast_to_instance.insert(domain_name.clone(), instance_id);

                    info!("Created Ground component instance '{}' connected to net", domain_name);
                } else {
                    // Create Power component instance using voltage-appropriate symbol
                    let symbol_name = match domain_info.voltage.round() as i32 {
                        5 => "+5V".to_string(),
                        12 => "+12V".to_string(),
                        3 => "+3V3".to_string(),
                        24 => "+24V".to_string(),
                        _ => format!("+{}V", domain_info.voltage.round() as i32),
                    };

                    let module_id = self.netlist.add_module(
                        symbol_name.clone(),
                        ModuleKind::PhysicalComponent
                    );

                    // Add pin to the module (power symbols have pin "1")
                    let pin_id = self.netlist.add_pin(
                        module_id,
                        "1".to_string(),
                        PinDirection::Out,
                        PinType::Power
                    ).ok_or_else(|| anyhow::anyhow!("Failed to add power pin"))?;
                    
                    // Create instance
                    let instance_id = self.netlist.add_instance(
                        domain_name.clone(),
                        module_id
                    ).ok_or_else(|| anyhow::anyhow!("Failed to add Power instance"))?;
                    
                    // Create pin instances for the power component
                    let pin_instances = self.netlist.create_pin_instances(instance_id)
                        .map_err(|e| anyhow::anyhow!("Failed to create pin instances: {}", e))?;
                    
                    // Connect the first pin instance to the net
                    if let Some(&pin_inst_id) = pin_instances.first() {
                        self.netlist.connect(net_id, ConnectionPoint::PinInstance(pin_inst_id))
                            .map_err(|e| anyhow::anyhow!("Failed to connect power: {}", e))?;
                    }
                    
                    self.ast_to_instance.insert(domain_name.clone(), instance_id);

                    info!("Created Power component instance '{}' ({}V @ {}A) connected to net",
                          domain_name, domain_info.voltage, domain_info.max_current);
                }
            }
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

        for (domain_name, domain_info) in &power_context.domains {
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

    /// Visit all connection statements in the AST
    fn visit_connections_in_ast(&mut self, node: &bhdl_ast::SyntaxNode<bhdl_ast::BhdlLanguage>, count: &mut usize, analysis: &AnalysisResult) -> Result<()> {
        use bhdl_ast::{SyntaxKind, AstNode, ConnectionStmt};
        
        // Check if this is a connection statement
        if node.kind() == SyntaxKind::CONNECTION_STMT {
            if let Some(conn_stmt) = ConnectionStmt::cast(node.clone()) {
                // First pass: identify and map component handles
                self.identify_component_handles(&conn_stmt)?;
                // Second pass: process connections
                self.process_connection_statement(&conn_stmt, analysis)?;
                *count += 1;
            }
        }
        
        // Recursively visit children
        for child in node.children() {
            self.visit_connections_in_ast(&child, count, analysis)?;
        }
        
        Ok(())
    }
    
    /// Identify component handles in a connection statement (first pass)
    fn identify_component_handles(&mut self, conn: &bhdl_ast::ConnectionStmt) -> Result<()> {
        use bhdl_ast::AstNode;
        let conn_text = conn.syntax().text().to_string();
        let parts: Vec<&str> = conn_text.split("->").collect();
        
        for part in parts {
            let endpoint = part.trim().trim_end_matches(';');
            
            // Check for net assignment pattern (handle: Component(...).pin)
            if let Some(colon_pos) = endpoint.find(':') {
                let handle_name = endpoint[..colon_pos].trim();
                
                // Extract component type
                let after_colon = endpoint[colon_pos + 1..].trim();
                if let Some(paren_pos) = after_colon.find('(') {
                    let component_type = after_colon[..paren_pos].trim();
                    
                    // Find the matching component instance by type
                    // This should map the handle to the instance that was created during component generation
                    for (idx, comp) in self.component_instances.iter().enumerate() {
                        if comp.bhdl_type == component_type && 
                           !self.net_assignment_handles.values().any(|&id| {
                               self.ast_to_instance.get(&comp.instance_name)
                                   .map(|&inst_id| inst_id == id)
                                   .unwrap_or(false)
                           }) {
                            // Found unmapped component of the right type
                            if let Some(&inst_id) = self.ast_to_instance.get(&comp.instance_name) {
                                self.net_assignment_handles.insert(handle_name.to_string(), inst_id);
                                info!("Mapped handle '{}' to instance {} ({})", handle_name, comp.instance_name, component_type);
                                break;
                            }
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Process a single connection statement
    fn process_connection_statement(&mut self, conn: &bhdl_ast::ConnectionStmt, analysis: &AnalysisResult) -> Result<()> {
        use bhdl_ast::AstNode;
        // Parse the connection to extract the flow of connections
        // Example: VIN -> C1: Cap(100uF, 25V).pos -> U1: LM7805(package="TO-220").IN;
        // Example with net naming: VIN @RAW-> fuse: Fuse(1A).1
        
        let conn_text = conn.syntax().text().to_string();
        info!("Processing connection: {}", conn_text.trim());
        
        // Debug: Check for D1 and C3 connections specifically
        if conn_text.contains("D1") || conn_text.contains("C3") {
            info!("  DEBUG: Found D1/C3 connection: {}", conn_text.trim());
        }
        
        // Parse the connection chain by splitting on -> but handle @NETNAME-> specially
        let parts = self.parse_connection_chain(&conn_text);
        
        info!("  Parsed connection into {} parts:", parts.len());
        for (i, part) in parts.iter().enumerate() {
            info!("    Part {}: '{}'", i, part);
        }
        
        if parts.len() < 2 {
            warn!("Invalid connection format: {}", conn_text);
            return Ok(());
        }
        
        // Track the net for this connection chain
        let mut current_net_id: Option<NetId> = None;
        
        // First pass: identify if we're connecting to an existing net
        let mut target_net_name: Option<String> = None;
        for part in &parts {
            let endpoint = part.trim().trim_end_matches(';');
            if let ConnectionEndpoint::Net(net_name) = self.parse_connection_endpoint(endpoint) {
                target_net_name = Some(net_name);
                break;
            }
        }
        
        // If we found a target net, use it as the current net
        if let Some(net_name) = target_net_name {
            current_net_id = Some(self.ensure_net_exists(&net_name));
            info!("  Using existing net '{}' for connection", net_name);
        }
        
        // Process each connection endpoint
        for (i, part) in parts.iter().enumerate() {
            let endpoint = part.trim().trim_end_matches(';');
            let endpoint_info = self.parse_connection_endpoint(endpoint);
            
            info!("  Connection endpoint {}: {:?}", i, endpoint_info);
            
            match endpoint_info {
                ConnectionEndpoint::Net(net_name) => {
                    // This is a simple net reference (VCC, GND, etc.)
                    // Skip if we already set this as current net in the first pass
                    if current_net_id.is_none() {
                        current_net_id = Some(self.ensure_net_exists(&net_name));
                    }
                }
                ConnectionEndpoint::NetRef(net_name) => {
                    // This is a net reference with @ prefix (@NETNAME)
                    // Connect to the existing net or create it if it doesn't exist
                    let net_id = self.ensure_net_exists(&net_name);
                    current_net_id = Some(net_id);
                    info!("    Connected to net reference @{} (net {:?})", net_name, net_id);
                }
                ConnectionEndpoint::Pin(instance_name, pin_name) => {
                    // This is a component pin reference (C1.pos, U1.IN, fuse.2, etc.)
                    // or an interface signal reference (i2c_bus.SDA)
                    
                    // First check if this might be an interface signal reference
                    // The pattern instance_name.pin_name might be an interface signal
                    // Check the analysis result to see if instance_name is an interface
                    let mut is_interface_signal = false;
                    
                    // Look for any net that ends with _<signal_name> and starts with a valid instance prefix
                    let mut found_interface_net = false;
                    for (net_id, net) in self.netlist.nets.iter() {
                        if let Some(net_name) = &net.name {
                            // Check if this net ends with our signal name
                            if net_name.ends_with(&format!("_{}", pin_name)) {
                                // Extract the prefix before _<signal>
                                let prefix_end = net_name.len() - pin_name.len() - 1;
                                if prefix_end > 0 {
                                    let prefix = &net_name[..prefix_end];
                                    // Check if this prefix matches a known instance (like U1, U2, etc.)
                                    // Interface instances are typically generated with U<number> names
                                    if prefix.starts_with("U") && prefix[1..].chars().all(|c| c.is_digit(10)) {
                                        // This looks like an interface signal net
                                        info!("    Found interface signal net: {} for {}.{}", net_name, instance_name, pin_name);
                                        current_net_id = Some(net_id);
                                        found_interface_net = true;
                                        is_interface_signal = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    
                    if found_interface_net {
                        // We've already set the current_net_id, continue to next endpoint
                        continue;
                    }
                    
                    // Not an interface signal, check for regular component pin
                    let instance_id = if let Some(&handle_id) = self.net_assignment_handles.get(&instance_name) {
                        info!("    Found net assignment handle '{}' -> instance {:?}", instance_name, handle_id);
                        Some(handle_id)
                    } else {
                        self.ast_to_instance.get(&instance_name).copied()
                    };
                    
                    if let Some(inst_id) = instance_id {
                        if let Some(pin_inst_id) = self.netlist.find_pin_instance(inst_id, &pin_name) {
                            // Connect this pin to the current net
                            let net_id = current_net_id.get_or_insert_with(|| {
                                self.netlist.add_net(None)
                            });
                            
                            self.netlist.connect(*net_id, ConnectionPoint::PinInstance(pin_inst_id))
                                .map_err(|e| anyhow::anyhow!("Failed to connect pin: {}", e))?;
                                
                            info!("    Connected {} to net {:?}", endpoint, net_id);
                        } else {
                            warn!("    Pin {} not found on instance {}", pin_name, instance_name);
                            // Try common pin name alternatives
                            let alt_pins = match pin_name.as_str() {
                                "1" => vec!["IN", "VI", "VIN", "+", "pos"],
                                "2" => vec!["OUT", "VO", "VOUT", "-", "neg", "GND"],  
                                "3" => vec!["GND", "COM"],
                                "pos" => vec!["+", "1"],
                                "neg" => vec!["-", "2", "GND"],
                                "IN" => vec!["1", "VI", "VIN"],
                                "OUT" => vec!["2", "3", "VO", "VOUT"],
                                "GND" => vec!["2", "3", "COM", "-"],
                                "K" => vec!["cathode", "2", "-"],
                                "A" => vec!["anode", "1", "+"],
                                _ => vec![]
                            };
                            for alt in alt_pins {
                                if let Some(pin_inst_id) = self.netlist.find_pin_instance(inst_id, alt) {
                                    info!("    Found pin using alternative name '{}' instead of '{}'", alt, pin_name);
                                    let net_id = current_net_id.get_or_insert_with(|| {
                                        self.netlist.add_net(None)
                                    });
                                    self.netlist.connect(*net_id, ConnectionPoint::PinInstance(pin_inst_id))
                                        .map_err(|e| anyhow::anyhow!("Failed to connect pin: {}", e))?;
                                    break;
                                }
                            }
                        }
                    } else {
                        warn!("    Instance {} not found (checked both regular instances and net assignment handles)", instance_name);
                        warn!("    Available instances:");
                        for (name, _) in &self.ast_to_instance {
                            warn!("      - {}", name);
                        }
                        warn!("    Available net assignment handles:");
                        for (handle, _) in &self.net_assignment_handles {
                            warn!("      - {}", handle);
                        }
                    }
                }
                ConnectionEndpoint::NamedHandle(handle_name, _component_type) => {
                    // This is a named handle declaration (C1: Cap(...))
                    // The instance should already be created during component generation
                    if !self.ast_to_instance.contains_key(&handle_name) {
                        warn!("    Named handle {} not found in instances", handle_name);
                    }
                }
                ConnectionEndpoint::NetAssignment(handle_name, component_type, pin_name) => {
                    // This is inline component instantiation (handle: Component(...).pin)
                    // Example: r1: Res(330Ω).1
                    // This creates a component instance with handle, NOT a net
                    
                    info!("    Processing inline component instantiation: {} = {}(...).{}", handle_name, component_type, pin_name);
                    
                    // Do NOT create a net with the handle name!
                    // The net is the connection itself, not the handle
                    
                    // Check if this handle already has an instance
                    let inst_id = if let Some(&existing_id) = self.net_assignment_handles.get(&handle_name) {
                        info!("    Found pre-mapped handle '{}' -> instance {:?}", handle_name, existing_id);
                        existing_id
                    } else {
                        // Need to create the component instance inline
                        info!("    Creating inline component instance for handle '{}'", handle_name);
                        
                        // Create the component instance
                        // TODO: This should use database components if available
                        let module_id = self.netlist.add_module(
                            component_type.to_string(),
                            self.map_component_type_to_module_kind(&component_type)
                        );
                        
                        // Add pins for this component type
                        if let Err(e) = self.add_pins_for_component(&handle_name, &component_type, module_id) {
                            warn!("Failed to add pins for {}: {}", component_type, e);
                        }
                        
                        // Create the instance
                        let new_inst_id = self.netlist.add_instance(
                            handle_name.to_string(),
                            module_id
                        );
                        
                        if let Some(inst_id) = new_inst_id {
                            // Create pin instances for this component
                            self.netlist.create_pin_instances(inst_id)
                                .map_err(|e| anyhow::anyhow!("Failed to create pin instances: {}", e))?;
                            
                            // Transfer component parameters from analyzer to instance attributes
                            self.populate_instance_attributes(inst_id, &handle_name, analysis);
                            
                            // Store mappings
                            self.ast_to_instance.insert(handle_name.to_string(), inst_id);
                            self.net_assignment_handles.insert(handle_name.to_string(), inst_id);
                            
                            info!("    Created instance {:?} for handle '{}'", inst_id, handle_name);
                            inst_id
                        } else {
                            warn!("    Failed to create instance for handle '{}'", handle_name);
                            continue;
                        }
                    };
                    
                    // Find the pin on this instance
                    if let Some(pin_inst_id) = self.netlist.find_pin_instance(inst_id, &pin_name) {
                            // Connect this pin to the current net
                            let net_id = current_net_id.get_or_insert_with(|| {
                                self.netlist.add_net(None)
                            });
                            
                            self.netlist.connect(*net_id, ConnectionPoint::PinInstance(pin_inst_id))
                                .map_err(|e| anyhow::anyhow!("Failed to connect pin: {}", e))?;
                            
                            info!("    Connected {}({}).{} to net {:?}", component_type, "...", pin_name, net_id);
                    } else {
                            warn!("    Pin {} not found on {} instance", pin_name, component_type);
                            // Try common pin name alternatives
                            let alt_pins = match pin_name.as_str() {
                                "1" => vec!["IN", "VI", "VIN", "+", "pos"],
                                "2" => vec!["OUT", "VO", "VOUT", "-", "neg", "GND"],
                                "3" => vec!["GND", "COM"],
                                _ => vec![]
                            };
                            for alt in alt_pins {
                                if let Some(pin_inst_id) = self.netlist.find_pin_instance(inst_id, alt) {
                                    info!("    Found pin using alternative name '{}' instead of '{}'", alt, pin_name);
                                    
                                    let net_id = current_net_id.get_or_insert_with(|| {
                                        self.netlist.add_net(None)
                                    });
                                    
                                    self.netlist.connect(*net_id, ConnectionPoint::PinInstance(pin_inst_id))
                                        .map_err(|e| anyhow::anyhow!("Failed to connect pin: {}", e))?;
                                    break;
                                }
                            }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Clean connection endpoint by removing trailing semicolons and extracting net/pin name
    fn clean_connection_endpoint(&self, endpoint: &str) -> String {
        // Remove trailing semicolon
        let clean = endpoint.trim_end_matches(';').trim();
        
        // Extract just the net/pin identifier
        // Examples:
        // "C1: Cap(100uF, 25V).pos" -> "C1.pos"
        // "U1: LM7805(package=\"TO-220\").IN" -> "U1.IN"
        // "VCC" -> "VCC"
        
        if let Some(colon_pos) = clean.find(':') {
            // This is a named handle like "C1: Cap(...).pos"
            let name = clean[..colon_pos].trim();
            
            // Find the pin after the component instantiation
            if let Some(dot_pos) = clean.rfind('.') {
                let pin = &clean[dot_pos + 1..];
                return format!("{}.{}", name, pin);
            } else {
                return name.to_string();
            }
        }
        
        // Simple identifier or component.pin
        clean.to_string()
    }
    
    /// Ensure a net exists for the given name
    fn ensure_net_exists(&mut self, net_name: &str) -> NetId {
        if let Some(&net_id) = self.ast_to_net.get(net_name) {
            net_id
        } else {
            let net_id = self.netlist.add_net(Some(net_name.to_string()));
            self.ast_to_net.insert(net_name.to_string(), net_id);
            debug!("Created net '{}'", net_name);
            net_id
        }
    }
    
    /// Parse a connection chain handling @NETNAME-> syntax
    fn parse_connection_chain(&self, conn_text: &str) -> Vec<String> {
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut i = 0;
        let chars: Vec<char> = conn_text.chars().collect();
        
        while i < chars.len() {
            if i + 1 < chars.len() && chars[i] == '-' && chars[i + 1] == '>' {
                // Found ->
                // Look back to see if we have @NETNAME pattern
                let trimmed = current.trim();
                if let Some(space_pos) = trimmed.rfind(' ') {
                    let after_space = &trimmed[space_pos + 1..];
                    if after_space.starts_with('@') {
                        // This is "something @NETNAME" pattern
                        // Split at the space before @
                        let before_at = trimmed[..space_pos].trim();
                        if !before_at.is_empty() {
                            parts.push(before_at.to_string());
                        }
                        // Keep @NETNAME-> together
                        parts.push(format!("{}{}", after_space, "->"));
                        current.clear();
                        i += 2; // Skip past ->
                        continue;
                    }
                }
                
                // Normal -> split
                if !current.trim().is_empty() {
                    parts.push(current.trim().to_string());
                }
                current.clear();
                i += 2; // Skip past ->
            } else {
                current.push(chars[i]);
                i += 1;
            }
        }
        
        // Don't forget the last part
        if !current.trim().is_empty() {
            parts.push(current.trim().trim_end_matches(';').to_string());
        }
        
        parts
    }
    
    /// Parse a connection endpoint to determine its type
    fn parse_connection_endpoint(&self, endpoint: &str) -> ConnectionEndpoint {
        let trimmed = endpoint.trim();
        
        // Check for net creation/reference with @ prefix
        // @NETNAME-> creates and references a net
        // @NETNAME references an existing net
        if trimmed.ends_with("->") && trimmed.contains('@') {
            // This is @NETNAME-> pattern (net creation)
            let without_arrow = trimmed.trim_end_matches("->");
            if let Some(at_pos) = without_arrow.rfind('@') {
                let net_name = without_arrow[at_pos + 1..].trim().to_string();
                return ConnectionEndpoint::NetRef(net_name);
            }
        } else if trimmed.starts_with('@') {
            // This is just @NETNAME (net reference)
            let net_name = trimmed[1..].to_string(); // Remove @ prefix
            return ConnectionEndpoint::NetRef(net_name);
        }
        
        // Check for net assignment pattern (handle: Component(...).pin)
        // This creates a component instance with a handle, NOT a net
        if let Some(colon_pos) = trimmed.find(':') {
            let handle_name = trimmed[..colon_pos].trim().to_string();
            
            // Extract component type and pin
            let after_colon = trimmed[colon_pos + 1..].trim();
            if let Some(paren_pos) = after_colon.find('(') {
                let component_type = after_colon[..paren_pos].trim().to_string();
                
                // Check if there's a pin after the component
                if let Some(dot_pos) = after_colon.rfind('.') {
                    let pin_name = after_colon[dot_pos + 1..].trim().to_string();
                    // This is a component instantiation with handle and pin reference
                    return ConnectionEndpoint::NetAssignment(handle_name, component_type, pin_name);
                } else {
                    // This is a named handle without pin (shouldn't happen in valid syntax)
                    return ConnectionEndpoint::NamedHandle(handle_name, component_type);
                }
            }
        }
        
        // Check for pin reference (handle.pin)
        if let Some(dot_pos) = trimmed.find('.') {
            let instance_name = trimmed[..dot_pos].trim().to_string();
            let pin_name = trimmed[dot_pos + 1..].trim().to_string();
            return ConnectionEndpoint::Pin(instance_name, pin_name);
        }
        
        // Otherwise it's a simple net name (VCC, GND, etc.)
        ConnectionEndpoint::Net(trimmed.to_string())
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

        // Get or create GND net for capacitor connections
        let gnd_net_id = if let Some(&net_id) = self.ast_to_net.get("GND") {
            net_id
        } else {
            let net_id = self.netlist.add_net_with_class(
                Some("GND".to_string()),
                bhdl_netlist::types::NetClass::Ground
            );
            self.ast_to_net.insert("GND".to_string(), net_id);
            net_id
        };

        // Process expanded power distribution connections
        for connection in &expansion.connections {
            info!("  Power connection: @{} -> {}.{}",
                  connection.source_net, connection.component, connection.pin);

            // Get or create the source net
            let source_net_id = if let Some(&net_id) = self.ast_to_net.get(&connection.source_net) {
                net_id
            } else {
                // Create the net with appropriate class
                let net_id = self.netlist.add_net_with_class(
                    Some(connection.source_net.clone()),
                    bhdl_netlist::types::NetClass::Power(3.3) // Default voltage, should be from domain spec
                );
                self.ast_to_net.insert(connection.source_net.clone(), net_id);
                net_id
            };

            // Get the component instance
            if let Some(&instance_id) = self.ast_to_instance.get(&connection.component) {
                // Find the pin instance for this component's pin
                if let Some(pin_inst_id) = self.netlist.find_pin_instance(instance_id, &connection.pin) {
                    // Connect the power net to this pin instance
                    if let Err(e) = self.netlist.connect(source_net_id, ConnectionPoint::PinInstance(pin_inst_id)) {
                        debug!("  Failed to connect power to {}.{}: {}", connection.component, connection.pin, e);
                    } else {
                        debug!("  Connected: @{} -> {}.{}", connection.source_net, connection.component, connection.pin);
                    }
                } else {
                    debug!("  Pin instance not found for {}.{}", connection.component, connection.pin);
                }
            } else {
                debug!("  Component instance not yet created: {}", connection.component);
            }
        }

        // Create capacitor module definition with proper pins if not exists
        let cap_module_id = if let Some(&module_id) = self.ast_to_module.get("Capacitor") {
            module_id
        } else {
            // Create the Capacitor module with + and - pins
            let module_id = self.netlist.add_module(
                "Capacitor".to_string(),
                bhdl_netlist::types::ModuleKind::Component
            );

            // Add pins to the capacitor module
            self.netlist.add_pin(
                module_id,
                "+".to_string(),
                bhdl_netlist::types::PinDirection::InOut,
                bhdl_netlist::types::PinType::Power
            );
            self.netlist.add_pin(
                module_id,
                "-".to_string(),
                bhdl_netlist::types::PinDirection::InOut,
                bhdl_netlist::types::PinType::Ground
            );

            self.ast_to_module.insert("Capacitor".to_string(), module_id);
            debug!("Created Capacitor module with + and - pins");
            module_id
        };

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

                    // Find the power net to connect to
                    // Assumption: All decoupling caps connect to the same power domain that generated them
                    // In a real implementation, we'd need to track which domain each cap belongs to
                    let power_net_id = if let Some(first_conn) = expansion.connections.first() {
                        self.ast_to_net.get(&first_conn.source_net).copied()
                    } else {
                        None
                    };

                    if let Some(power_net) = power_net_id {
                        // Connect capacitor + pin to power net
                        if let Some(plus_pin_inst) = self.netlist.find_pin_instance(inst_id, "+") {
                            if let Err(e) = self.netlist.connect(power_net, ConnectionPoint::PinInstance(plus_pin_inst)) {
                                debug!("  Failed to connect {} + pin to power: {}", cap.instance_name, e);
                            } else {
                                debug!("  Connected {} + pin to power net", cap.instance_name);
                            }
                        }

                        // Connect capacitor - pin to GND
                        if let Some(minus_pin_inst) = self.netlist.find_pin_instance(inst_id, "-") {
                            if let Err(e) = self.netlist.connect(gnd_net_id, ConnectionPoint::PinInstance(minus_pin_inst)) {
                                debug!("  Failed to connect {} - pin to GND: {}", cap.instance_name, e);
                            } else {
                                debug!("  Connected {} - pin to GND", cap.instance_name);
                            }
                        }
                    } else {
                        debug!("  No power net found for capacitor connections");
                    }

                    // Store capacitor value as instance attribute
                    if let Some(instance) = self.netlist.instances.get_mut(inst_id) {
                        instance.attributes.insert("value".to_string(), cap.value.clone());
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
        for (node_ptr, scope) in &analysis.definition_scopes {
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
        for (_, scope) in &analysis.definition_scopes {
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
    
    /// Add pins to a component module based on its type using stdlib definitions
    fn add_pins_for_component(&mut self, instance_name: &str, component_type: &str, module_id: ModuleId) -> Result<()> {
        debug!("add_pins_for_component called for component_type: {} (from lib.rs)", component_type);
        debug!("import_preprocessor is_some: {}", self.import_preprocessor.is_some());
        
        // First check if this component was imported via preprocessor
        let (pin_definitions, has_virtual_pins) = if let Some(ref preprocessor) = self.import_preprocessor {
            if let Some(entity) = preprocessor.get_imported_entity(component_type) {
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
                        } else if pin_text.contains("signal in") {
                            (bhdl_netlist::types::PinDirection::In, bhdl_netlist::types::PinType::Signal)
                        } else if pin_text.contains("signal out") {
                            (bhdl_netlist::types::PinDirection::Out, bhdl_netlist::types::PinType::Signal)
                        } else if pin_text.contains("signal inout") {
                            (bhdl_netlist::types::PinDirection::InOut, bhdl_netlist::types::PinType::Signal)
                        } else {
                            (bhdl_netlist::types::PinDirection::Passive, bhdl_netlist::types::PinType::Passive)
                        };
                        
                        pins.push(bhdl_stdlib::StdlibPinDefinition {
                            name: name.text().to_string(),
                            direction,
                            pin_type,
                            is_virtual,
                        });
                    }
                }
                
                let has_virtual = pins.iter().any(|p| p.is_virtual);
                (pins, has_virtual)
            } else {
                // No preprocessor available - use empty pin definitions
                warn!("No import preprocessor available for component {} - cannot extract pins", component_type);
                (Vec::new(), false)
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
                    } else if pin_text.contains("signal in") {
                        (bhdl_netlist::types::PinDirection::In, bhdl_netlist::types::PinType::Signal)
                    } else if pin_text.contains("signal out") {
                        (bhdl_netlist::types::PinDirection::Out, bhdl_netlist::types::PinType::Signal)
                    } else if pin_text.contains("signal inout") {
                        (bhdl_netlist::types::PinDirection::InOut, bhdl_netlist::types::PinType::Signal)
                    } else {
                        (bhdl_netlist::types::PinDirection::Passive, bhdl_netlist::types::PinType::Passive)
                    };
                    
                    pins.push(bhdl_stdlib::StdlibPinDefinition {
                        name: name.text().to_string(),
                        direction,
                        pin_type,
                        is_virtual,
                    });
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
            info!("Component '{}' has virtual pins that need expansion", component_type);
            
            // Check if we have synthesis knowledge for this component
            // Virtual pin expansion is now handled by AST-based automatic component generation
            info!("Virtual pin expansion will be handled by AST-based automatic component generation");
            // The virtual pin expansion happens in generate_automatic_supporting_components
            // which extracts virtual pins directly from AST nodes in the symbol table
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
            
            self.netlist.add_pin(
                module_id, 
                pin_def.name.clone(), 
                pin_def.direction, 
                pin_def.pin_type
            );
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
    
    /// Run component compatibility analysis on the generated netlist  
    async fn run_compatibility_analysis(&self, analysis: &AnalysisResult) -> Result<()> {
        use crate::component_compatibility::ComponentCompatibilityAnalyzer;
        use std::path::Path;
        
        info!("Running component compatibility analysis on {} components", self.netlist.instances.len());
        
        // Initialize the compatibility analyzer (try to use database if available)
        let analyzer = if let Some(ref db_path) = self.config.database_path {
            match ComponentCompatibilityAnalyzer::with_database(Path::new(db_path)).await {
                Ok(analyzer) => {
                    info!("Using real component database for compatibility analysis");
                    analyzer
                },
                Err(e) => {
                    warn!("Failed to connect to component database: {}. Using mock data.", e);
                    ComponentCompatibilityAnalyzer::new()
                }
            }
        } else {
            info!("No database path configured. Using mock compatibility data.");
            ComponentCompatibilityAnalyzer::new()
        };
        
        // Run the compatibility analysis
        match analyzer.analyze_compatibility(&self.netlist, analysis) {
            Ok(report) => {
                // Report compatibility results to the user
                info!("=== Component Compatibility Analysis Results ===");
                info!("Overall compatibility score: {:.1}%", report.overall_compatibility_score * 100.0);
                
                // Report power domain analysis
                if !report.power_domain_analysis.is_empty() {
                    info!("Power Domain Analysis:");
                    for (i, domain) in report.power_domain_analysis.iter().enumerate() {
                        info!("  {}. Domain '{}' ({:.1}V) - {} components, {:.1}A capacity", 
                              i + 1, domain.domain_name, domain.nominal_voltage, 
                              domain.connected_components.len(), domain.max_current);
                        
                        if !domain.compatibility_issues.is_empty() {
                            warn!("     {} compatibility issues found", domain.compatibility_issues.len());
                            for issue in &domain.compatibility_issues {
                                warn!("     - {}: {}", issue.title, issue.description);
                            }
                        }
                    }
                }
                
                // Report thermal analysis
                if !report.thermal_analysis.is_empty() {
                    info!("Thermal Analysis:");
                    for (i, zone) in report.thermal_analysis.iter().enumerate() {
                        info!("  {}. Zone '{}' - {:.2}W dissipation, max {:.1}°C", 
                              i + 1, zone.thermal_zone, zone.total_power_dissipation, zone.max_junction_temp);
                    }
                }
                
                // Report critical issues
                if !report.critical_issues.is_empty() {
                    warn!("Critical compatibility issues found:");
                    for issue in &report.critical_issues {
                        warn!("  - {}: {}", issue.title, issue.description);
                        warn!("    Recommended action: {}", issue.recommended_action);
                    }
                } else {
                    info!("No critical compatibility issues detected");
                }
                
                // Report optimization opportunities
                if !report.optimization_opportunities.is_empty() {
                    info!("Optimization opportunities identified:");
                    for opportunity in &report.optimization_opportunities {
                        info!("  - {}: {}", opportunity.title, opportunity.description);
                    }
                }
                
                info!("Component compatibility analysis completed successfully");
            },
            Err(e) => {
                warn!("Component compatibility analysis failed: {}", e);
                // Don't fail the entire synthesis - just log the warning
            }
        }
        
        Ok(())
    }
    
    /// Run design pattern recognition on the generated netlist
    fn run_pattern_recognition(&mut self, analysis: &AnalysisResult) -> Result<()> {
        use crate::design_pattern_recognition::DesignPatternRecognizer;
        
        info!("Running design pattern recognition on {} components", self.netlist.instances.len());
        
        let mut recognizer = DesignPatternRecognizer::new();
        
        // Recognize patterns in the netlist
        match recognizer.recognize_patterns(&self.netlist, analysis) {
            Ok(report) => {
                info!("Recognized {} circuit patterns", report.recognized_patterns.len());
                for pattern in &report.recognized_patterns {
                    info!("  - {} (confidence: {:.1}%)", pattern.pattern_name, pattern.confidence_score * 100.0);
                    if !pattern.matched_components.is_empty() {
                        info!("    Components: {} instances", pattern.matched_components.len());
                    }
                }
            },
            Err(e) => {
                warn!("Pattern recognition failed: {}", e);
            }
        }
        
        Ok(())
    }
    
    /// Run cross-component optimization
    fn run_cross_component_optimization(&mut self, analysis: &AnalysisResult) -> Result<()> {
        use crate::cross_component_optimization::CrossComponentOptimizer;
        
        info!("Running cross-component optimization");
        
        let mut optimizer = CrossComponentOptimizer::new();
        
        // Analyze coordination opportunities  
        // Note: Using empty behavioral models array as we don't have them yet
        let behavioral_models = Vec::new();
        match optimizer.analyze_coordination_opportunities(&self.netlist, &behavioral_models) {
            Ok(plan) => {
                info!("Found coordination plan with {} participants", plan.total_participants);
                
                // Execute coordinated optimization if there are participants
                if plan.total_participants > 0 {
                    // Create initial design parameters (would come from simulation in full implementation)
                    let initial_params = bhdl_simulation::DesignParameters::new();
                    match optimizer.execute_coordinated_optimization(&mut self.netlist, &initial_params) {
                        Ok(result) => {
                            info!("Cross-component optimization completed:");
                            info!("  - {} optimization phases executed", result.phase_results.len());
                            info!("  - Objectives met: {}", result.objectives_met);
                            
                            // Log phase results
                            for phase in &result.phase_results {
                                let total_objectives = phase.objectives_achieved.len();
                                info!("    Phase '{}': {} participants, {} objectives achieved",
                                      phase.phase_name, phase.participants_optimized, total_objectives);
                            }
                        },
                        Err(e) => {
                            warn!("Cross-component optimization execution failed: {}", e);
                        }
                    }
                }
            },
            Err(e) => {
                warn!("Cross-component opportunity analysis failed: {}", e);
            }
        }
        
        Ok(())
    }
    
    /// Run design rule checking
    fn run_design_rule_check(&self, analysis: &AnalysisResult) -> Result<()> {
        use crate::design_rule_checker::{DesignRuleChecker, IndustryStandard};
        
        info!("Running design rule check on netlist");
        
        // Use IPC-2221 as default standard
        let mut checker = DesignRuleChecker::new(IndustryStandard::IPC2221);
        let report = checker.run_checks(&self.netlist, analysis);
        
        info!("DRC Results:");
        info!("  - Rules checked: {}", report.rules_checked);
        info!("  - Pass rate: {:.1}%", report.pass_rate);
        
        if report.critical_count > 0 {
            error!("  - {} CRITICAL violations found!", report.critical_count);
        }
        if report.error_count > 0 {
            error!("  - {} ERROR violations found!", report.error_count);
        }
        if report.warning_count > 0 {
            warn!("  - {} WARNING violations found", report.warning_count);
        }
        if report.info_count > 0 {
            info!("  - {} INFO messages", report.info_count);
        }
        
        if report.manufacturing_ready {
            info!("✅ Design is MANUFACTURING READY");
        } else {
            warn!("❌ Design is NOT manufacturing ready - fix critical and error violations");
        }
        
        Ok(())
    }
    
    /// Run ML-based component selection optimization
    fn run_ml_component_selection(&mut self, analysis: &AnalysisResult) -> Result<()> {
        use crate::ml_component_selection::{
            MLComponentSelector, ComponentRequirements, ComponentCategory,
            EnvironmentalConditions, DesignContext
        };
        
        info!("Running ML-based component selection optimization");
        
        let ml_selector = MLComponentSelector::new();
        let mut optimization_count = 0;
        let mut total_components = 0;
        
        // Process each component instance for potential optimization
        for (instance_id, instance) in &self.netlist.instances {
            total_components += 1;
            
            // Determine component category
            let category = self.determine_component_category(&instance.name);
            
            // Extract requirements from instance and analysis
            let requirements = self.extract_component_requirements(
                instance,
                &category,
                analysis
            )?;
            
            // Create design context
            let context = DesignContext {
                application_type: "General Purpose".to_string(),
                production_volume: 1000,
                target_cost: 100.0,
                regulatory_requirements: vec![],
            };
            
            // Run ML selection
            match ml_selector.select_component(&requirements, &context) {
                Ok(prediction) => {
                    if !prediction.recommended_components.is_empty() {
                        let best = &prediction.recommended_components[0];
                        
                        // Only suggest optimization if confidence is high
                        if best.score > 0.8 {
                            info!("  ML recommends {} for {} (score: {:.2})",
                                  best.part_number, instance.name, best.score);
                            
                            // Store recommendation for later application
                            // In production, would update the netlist or generate report
                            optimization_count += 1;
                            
                            // Log reasons
                            for reason in &best.reasons {
                                debug!("    - {}", reason);
                            }
                        }
                    }
                },
                Err(e) => {
                    debug!("ML selection failed for {}: {}", instance.name, e);
                }
            }
        }
        
        info!("ML component selection completed:");
        info!("  - {} components analyzed", total_components);
        info!("  - {} optimization opportunities found", optimization_count);
        
        if optimization_count > 0 {
            let optimization_rate = (optimization_count as f64 / total_components as f64) * 100.0;
            info!("  - Optimization potential: {:.1}%", optimization_rate);
        }
        
        Ok(())
    }
    
    /// Determine component category from instance name/type
    fn determine_component_category(&self, name: &str) -> crate::ml_component_selection::ComponentCategory {
        use crate::ml_component_selection::ComponentCategory;
        
        let name_lower = name.to_lowercase();
        
        if name_lower.contains("res") || name_lower.starts_with('r') {
            ComponentCategory::Resistor
        } else if name_lower.contains("cap") || name_lower.starts_with('c') {
            ComponentCategory::Capacitor
        } else if name_lower.contains("ind") || name_lower.starts_with('l') {
            ComponentCategory::Inductor
        } else if name_lower.contains("diode") || name_lower.starts_with('d') {
            ComponentCategory::Diode
        } else if name_lower.starts_with('q') || name_lower.contains("trans") {
            ComponentCategory::Transistor
        } else if name_lower.starts_with('u') {
            ComponentCategory::IC
        } else if name_lower.starts_with('j') || name_lower.contains("conn") {
            ComponentCategory::Connector
        } else if name_lower.starts_with('y') || name_lower.contains("xtal") {
            ComponentCategory::Crystal
        } else {
            ComponentCategory::IC // Default
        }
    }
    
    /// Extract component requirements from instance and analysis
    fn extract_component_requirements(
        &self,
        instance: &bhdl_netlist::Instance,
        category: &crate::ml_component_selection::ComponentCategory,
        analysis: &AnalysisResult,
    ) -> Result<crate::ml_component_selection::ComponentRequirements> {
        use crate::ml_component_selection::{ComponentRequirements, ComponentCategory, EnvironmentalConditions};
        use std::collections::HashMap;
        
        let mut electrical_specs = HashMap::new();
        
        // Extract electrical specifications from instance attributes
        for (key, value) in &instance.attributes {
            if let Ok(num_value) = value.parse::<f64>() {
                electrical_specs.insert(key.clone(), num_value);
            }
        }
        
        // Add default specs based on category
        match category {
            ComponentCategory::Resistor => {
                electrical_specs.entry("power_rating".to_string()).or_insert(0.25);
                electrical_specs.entry("tolerance".to_string()).or_insert(5.0);
            },
            ComponentCategory::Capacitor => {
                electrical_specs.entry("voltage_rating".to_string()).or_insert(16.0);
                electrical_specs.entry("tolerance".to_string()).or_insert(10.0);
            },
            _ => {}
        }
        
        Ok(ComponentRequirements {
            component_type: category.clone(),
            electrical_specs,
            environmental_conditions: EnvironmentalConditions {
                temperature_range: (-40.0, 85.0),
                humidity_range: (0.0, 95.0),
                vibration_level: "Standard".to_string(),
                altitude_max: 3000.0,
                chemical_exposure: vec![],
            },
            cost_target: None,
            size_constraints: None,
            reliability_requirements: None,
        })
    }
    
    /// Run thermal simulation and analysis
    fn run_thermal_simulation(&mut self, analysis: &AnalysisResult) -> Result<()> {
        use crate::thermal_simulation::{ThermalSimulator, AmbientConditions, BoardThermalProperties};
        use std::collections::HashMap;
        
        info!("Running thermal simulation on {} components", self.netlist.instances.len());
        
        let mut simulator = ThermalSimulator::new();
        
        // Set up simulation environment
        let ambient = AmbientConditions {
            temperature: 25.0,  // °C - typical room temperature
            humidity: 50.0,     // %RH
            pressure: 101.3,    // kPa - sea level
            altitude: 0.0,      // m
            enclosure_properties: None,
        };
        simulator.set_ambient_conditions(ambient);
        
        // Set up board properties (would come from PCB design in production)
        let board_props = BoardThermalProperties::default();
        simulator.set_board_properties(board_props);
        
        // Extract component list and load thermal models
        let component_names: Vec<String> = self.netlist.instances.keys()
            .map(|id| self.netlist.instances[id].name.clone())
            .collect();
        
        simulator.load_component_models(&component_names)?;
        
        // Estimate power dissipation for each component
        let power_map = self.estimate_component_power_dissipation(analysis)?;
        
        info!("Power dissipation estimates:");
        for (name, power) in &power_map {
            debug!("  {}: {:.3}W", name, power);
        }
        
        // Run thermal simulation
        match simulator.simulate(&power_map) {
            Ok(results) => {
                info!("Thermal simulation results:");
                info!("  - Components analyzed: {}", results.component_temperatures.len());
                info!("  - Thermal violations: {}", results.thermal_violations.len());
                info!("  - Hot spots identified: {}", results.hot_spots.len());
                
                // Report component temperatures
                for (name, temp) in &results.component_temperatures {
                    let status = if temp.thermal_margin > 10.0 {
                        "✓ OK"
                    } else if temp.thermal_margin > 0.0 {
                        "⚠ WARM"
                    } else {
                        "❌ HOT"
                    };
                    
                    info!("    {}: {:.1}°C junction, {:.1}°C margin {}",
                          name, temp.junction_temperature, temp.thermal_margin, status);
                }
                
                // Report violations
                if !results.thermal_violations.is_empty() {
                    warn!("Thermal violations detected:");
                    for violation in &results.thermal_violations {
                        warn!("  - {}: {:.1}°C exceeds {:.1}°C limit ({:?})",
                              violation.component_name,
                              violation.actual_value,
                              violation.limit_value,
                              violation.severity);
                    }
                }
                
                // Report hot spots
                if !results.hot_spots.is_empty() {
                    warn!("Hot spots detected:");
                    for hot_spot in &results.hot_spots {
                        warn!("  - {:.1}°C at ({:.1}, {:.1}) mm - {} - {:?}",
                              hot_spot.temperature,
                              hot_spot.position.0,
                              hot_spot.position.1,
                              hot_spot.root_cause,
                              hot_spot.severity);
                    }
                }
                
                // Show cooling recommendations
                if !results.cooling_recommendations.is_empty() {
                    info!("Cooling recommendations:");
                    for rec in &results.cooling_recommendations {
                        info!("  - {:?}: {} ({:.1}°C improvement, {:?} cost)",
                              rec.solution_type,
                              rec.description,
                              rec.estimated_improvement,
                              rec.implementation_cost);
                    }
                }
                
                // Show derating recommendations
                if !results.power_derating_recommendations.is_empty() {
                    info!("Power derating recommendations:");
                    for rec in &results.power_derating_recommendations {
                        info!("  - {}: Reduce to {:.2}W ({:.0}% derating)",
                              rec.component_name,
                              rec.recommended_power,
                              (1.0 - rec.derating_factor) * 100.0);
                    }
                }
                
                // Generate thermal report
                match simulator.export_thermal_report(&results) {
                    Ok(report) => {
                        debug!("Thermal analysis report:\n{}", report);
                    },
                    Err(e) => {
                        warn!("Failed to generate thermal report: {}", e);
                    }
                }
            },
            Err(e) => {
                error!("Thermal simulation failed: {}", e);
                return Err(e);
            }
        }
        
        Ok(())
    }
    
    /// Estimate power dissipation for components
    fn estimate_component_power_dissipation(
        &self,
        analysis: &AnalysisResult,
    ) -> Result<HashMap<String, f64>> {
        let mut power_map = HashMap::new();
        
        // Extract power information from component instances
        for (_, instance) in &self.netlist.instances {
            let mut power = 0.0;
            
            // Check for explicit power attributes
            if let Some(power_str) = instance.attributes.get("power") {
                if let Ok(parsed_power) = power_str.parse::<f64>() {
                    power = parsed_power;
                }
            }
            
            // Estimate based on component type if no explicit power
            if power == 0.0 {
                power = self.estimate_power_by_type(&instance.name);
            }
            
            // Add power domain contributions
            power += self.estimate_power_from_domains(&instance.name, analysis);
            
            power_map.insert(instance.name.clone(), power);
        }
        
        Ok(power_map)
    }
    
    /// Estimate power by component type
    fn estimate_power_by_type(&self, component_name: &str) -> f64 {
        let name_lower = component_name.to_lowercase();
        
        if name_lower.starts_with('r') {
            0.001 // 1mW typical resistor
        } else if name_lower.starts_with('c') {
            0.0001 // 0.1mW typical capacitor
        } else if name_lower.starts_with('l') {
            0.0001 // 0.1mW typical inductor
        } else if name_lower.starts_with('d') {
            0.01 // 10mW typical diode
        } else if name_lower.starts_with('q') {
            0.1 // 100mW typical transistor
        } else if name_lower.starts_with('u') {
            0.25 // 250mW typical IC
        } else if name_lower.contains("led") {
            0.02 // 20mW typical LED
        } else {
            0.1 // 100mW default
        }
    }
    
    /// Estimate power contribution from power domains
    fn estimate_power_from_domains(&self, _component_name: &str, analysis: &AnalysisResult) -> f64 {
        // Check if component is connected to high-power domains
        // This would require connection analysis in production
        
        // For now, add small contribution based on power domain voltage
        let mut domain_power = 0.0;
        
        for (_domain_name, symbol) in analysis.global_scope.get_symbols() {
            if let Some(net_attr) = &symbol.net_attributes {
                if let Some(voltage) = net_attr.voltage() {
                    // Check for current in the net attributes (simplified)
                    // In production, would have proper current extraction method
                    if voltage > 3.0 {
                        domain_power += voltage * 0.01 * 0.01; // Very small estimated contribution
                    }
                }
            }
        }
        
        domain_power.min(1.0f64) // Cap at 1W
    }
    
    /// Run cost optimization with supplier data integration
    async fn run_cost_optimization(&mut self, analysis: &AnalysisResult) -> Result<()> {
        use crate::cost_optimization::{CostOptimizer, CostOptimizationConfig, SupplierClient, RateLimit, BackoffStrategy};
        use std::time::Duration;
        
        info!("Running cost optimization on {} components", self.netlist.instances.len());
        
        // Initialize cost optimizer if not already initialized
        if self.cost_optimizer.is_none() {
            let mut config = CostOptimizationConfig::default();
            config.enable_real_time_pricing = true;
            config.cache_pricing_hours = 4;
            config.parallel_supplier_queries = 5;
            config.include_shipping_costs = true;
            config.optimization_iterations = 30;
            
            let mut optimizer = CostOptimizer::with_config(config);
            
            // Add default suppliers (in production, these would come from configuration)
            let digikey = SupplierClient {
                supplier_name: "DigiKey".to_string(),
                api_endpoint: "https://api.digikey.com/v1/".to_string(),
                api_key: None, // Would be loaded from environment in production
                rate_limit: RateLimit {
                    requests_per_minute: 60,
                    burst_allowance: 10,
                    backoff_strategy: BackoffStrategy::Exponential(Duration::from_secs(1)),
                },
                availability_check: true,
                real_time_pricing: true,
                bulk_discount_support: true,
                lead_time_data: true,
            };
            
            let mouser = SupplierClient {
                supplier_name: "Mouser".to_string(),
                api_endpoint: "https://api.mouser.com/v1/".to_string(),
                api_key: None,
                rate_limit: RateLimit {
                    requests_per_minute: 100,
                    burst_allowance: 20,
                    backoff_strategy: BackoffStrategy::Linear(Duration::from_millis(500)),
                },
                availability_check: true,
                real_time_pricing: true,
                bulk_discount_support: true,
                lead_time_data: true,
            };
            
            let arrow = SupplierClient {
                supplier_name: "Arrow".to_string(),
                api_endpoint: "https://api.arrow.com/v1/".to_string(),
                api_key: None,
                rate_limit: RateLimit {
                    requests_per_minute: 30,
                    burst_allowance: 5,
                    backoff_strategy: BackoffStrategy::Fixed(Duration::from_secs(2)),
                },
                availability_check: true,
                real_time_pricing: false, // Batch pricing updates
                bulk_discount_support: true,
                lead_time_data: true,
            };
            
            // Add suppliers to optimizer
            optimizer.add_supplier(digikey).await.context("Failed to add DigiKey supplier")?;
            optimizer.add_supplier(mouser).await.context("Failed to add Mouser supplier")?;
            optimizer.add_supplier(arrow).await.context("Failed to add Arrow supplier")?;
            
            self.cost_optimizer = Some(optimizer);
            info!("Cost optimizer initialized with 3 suppliers");
        }
        
        // Run cost optimization
        if let Some(optimizer) = &mut self.cost_optimizer {
            match optimizer.optimize_component_costs(&self.netlist, analysis).await {
                Ok(results) => {
                    info!("Cost optimization results:");
                    info!("  - Original total cost: ${:.2}", results.original_cost.total);
                    info!("  - Optimized total cost: ${:.2}", results.optimized_cost.total);
                    info!("  - Total cost savings: ${:.2} ({:.1}%)", 
                          results.cost_savings, results.savings_percentage);
                    
                    // Report component-level savings
                    let significant_savings: Vec<_> = results.component_recommendations.iter()
                        .filter(|(_, rec)| rec.cost_change.abs() > 0.10) // Show savings > $0.10
                        .collect();
                    
                    if !significant_savings.is_empty() {
                        info!("  - Components with significant cost changes:");
                        for (instance_id, recommendation) in significant_savings.iter().take(10) {
                            let component_name = self.netlist.instances.get(**instance_id)
                                .map(|inst| inst.name.as_str())
                                .unwrap_or("unknown");
                            
                            let change_sign = if recommendation.cost_change >= 0.0 { "+" } else { "" };
                            info!("    • {}: {}{:.2} ({:.1}%) -> {}",
                                  component_name,
                                  change_sign,
                                  recommendation.cost_change,
                                  recommendation.cost_change_percentage,
                                  recommendation.recommended_component);
                        }
                        
                        if significant_savings.len() > 10 {
                            info!("    ... and {} more components with cost changes",
                                  significant_savings.len() - 10);
                        }
                    }
                    
                    // Report supplier consolidation
                    info!("  - Supplier optimization:");
                    info!("    • Suppliers: {} → {}",
                          results.supplier_consolidation.original_supplier_count,
                          results.supplier_consolidation.optimized_supplier_count);
                    info!("    • Consolidation savings: ${:.2}",
                          results.supplier_consolidation.consolidation_savings);
                    info!("    • Volume discount achieved: ${:.2}",
                          results.supplier_consolidation.volume_discount_achieved);
                    
                    // Report lifecycle risks
                    if !results.lifecycle_risks.is_empty() {
                        warn!("  - Lifecycle risks identified: {}", results.lifecycle_risks.len());
                        let high_risks = results.lifecycle_risks.iter()
                            .filter(|r| matches!(r.risk_level, crate::cost_optimization::RiskLevel::High | crate::cost_optimization::RiskLevel::Critical))
                            .count();
                        
                        if high_risks > 0 {
                            warn!("    • High/Critical risks: {} components", high_risks);
                        }
                    }
                    
                    // Show key findings and recommendations
                    if !results.optimization_summary.key_findings.is_empty() {
                        info!("  - Key findings:");
                        for finding in &results.optimization_summary.key_findings {
                            info!("    • {}", finding);
                        }
                    }
                    
                    if !results.optimization_summary.recommendations.is_empty() {
                        info!("  - Recommendations:");
                        for recommendation in &results.optimization_summary.recommendations {
                            info!("    • {}", recommendation);
                        }
                    }
                    
                    // Report optimization performance
                    info!("  - Optimization performance:");
                    info!("    • Iterations: {} (converged: {})",
                          results.optimization_summary.iterations_performed,
                          results.optimization_summary.convergence_achieved);
                    info!("    • Components analyzed: {}",
                          results.optimization_summary.components_analyzed);
                    info!("    • Alternatives evaluated: {}",
                          results.optimization_summary.alternatives_evaluated);
                    info!("    • Supplier queries: {}",
                          results.optimization_summary.supplier_queries_made);
                    info!("    • Time: {:.2}s",
                          results.optimization_summary.optimization_time_seconds);
                    
                    // Store results for later use (e.g., BOM generation)
                    // In production, this would be stored in the netlist or a separate structure
                    debug!("Cost optimization data available for BOM generation and procurement");
                },
                Err(e) => {
                    error!("Cost optimization failed: {}", e);
                    warn!("Continuing synthesis without cost optimization");
                    // Don't fail the entire synthesis due to cost optimization failure
                }
            }
        } else {
            error!("Cost optimizer not initialized - this should not happen");
            return Err(anyhow::anyhow!("Cost optimizer initialization failed"));
        }
        
        Ok(())
    }
    
    /// Run EMI/EMC (Electromagnetic Interference/Electromagnetic Compatibility) analysis
    async fn run_emi_emc_analysis(&mut self, analysis: &AnalysisResult) -> Result<()> {
        use crate::emi_emc_analysis::{EMIEMCAnalyzer, EMIEMCConfig, EmissionStandard, ImmunityStandard};
        
        info!("Running EMI/EMC analysis on {} components", self.netlist.instances.len());
        
        // Initialize EMI/EMC analyzer if not already initialized
        if self.emi_emc_analyzer.is_none() {
            let mut config = EMIEMCConfig::default();
            
            // Configure analysis parameters
            config.target_standards = vec![
                EmissionStandard::CISPR22,   // Information Technology Equipment
                EmissionStandard::FCC15,     // US FCC Part 15
                EmissionStandard::IEC61000,  // General EMC Standard
            ];
            
            config.immunity_standards = vec![
                ImmunityStandard::IEC61000_4_2, // ESD Immunity
                ImmunityStandard::IEC61000_4_3, // Radiated RF Immunity
                ImmunityStandard::IEC61000_4_4, // Electrical Fast Transient
                ImmunityStandard::IEC61000_4_6, // Conducted RF
            ];
            
            config.frequency_range = (9_000.0, 1_000_000_000.0); // 9 kHz to 1 GHz
            config.analysis_resolution = 100_000.0; // 100 kHz resolution
            config.enable_prediction = true;
            config.enable_mitigation_suggestions = true;
            config.include_crosstalk_analysis = true;
            config.include_power_integrity = true;
            config.safety_margin = 6.0; // 6 dB safety margin
            
            let analyzer = EMIEMCAnalyzer::with_config(config);
            self.emi_emc_analyzer = Some(analyzer);
            
            info!("EMI/EMC analyzer initialized with {} emission standards and {} immunity standards",
                  3, 4);
        }
        
        // Run EMI/EMC analysis
        if let Some(analyzer) = &mut self.emi_emc_analyzer {
            match analyzer.analyze_emi_emc(&self.netlist, analysis).await {
                Ok(results) => {
                    info!("EMI/EMC analysis results:");
                    
                    // Report emission compliance
                    info!("  - Emission Compliance:");
                    info!("    • Conducted emissions: {:?}", results.emission_compliance.conducted_emissions.overall_status);
                    info!("    • Radiated emissions: {:?}", results.emission_compliance.radiated_emissions.overall_status);
                    info!("    • Harmonic emissions: {:?}", results.emission_compliance.harmonic_emissions.overall_status);
                    info!("    • Emission hotspots found: {}", results.emission_compliance.emission_hotspots.len());
                    
                    // Report worst-case margins
                    let worst_conducted_margin = results.emission_compliance.conducted_emissions.worst_case_margin;
                    let worst_radiated_margin = results.emission_compliance.radiated_emissions.worst_case_margin;
                    
                    info!("    • Worst-case conducted margin: {:.1} dB", worst_conducted_margin);
                    info!("    • Worst-case radiated margin: {:.1} dB", worst_radiated_margin);
                    
                    if worst_conducted_margin < 0.0 || worst_radiated_margin < 0.0 {
                        warn!("    ⚠ Emission limits exceeded - mitigation required");
                    }
                    
                    // Report emission hotspots
                    if !results.emission_compliance.emission_hotspots.is_empty() {
                        info!("  - Emission Hotspots (top 5):");
                        for (i, hotspot) in results.emission_compliance.emission_hotspots.iter().take(5).enumerate() {
                            let component_name = self.netlist.instances.get(hotspot.component_id)
                                .map(|inst| inst.name.as_str())
                                .unwrap_or("unknown");
                            
                            info!("    {}. {} at {:.1} MHz: {:.1} dBμV ({:.1}% contribution)",
                                  i + 1,
                                  component_name,
                                  hotspot.emission_frequency / 1_000_000.0,
                                  hotspot.emission_level,
                                  hotspot.contribution_percentage);
                        }
                        
                        if results.emission_compliance.emission_hotspots.len() > 5 {
                            info!("    ... and {} more hotspots",
                                  results.emission_compliance.emission_hotspots.len() - 5);
                        }
                    }
                    
                    // Report immunity assessment
                    info!("  - Immunity Assessment:");
                    info!("    • Overall immunity level: {:.1} dBμV/m", 
                          results.immunity_assessment.susceptibility_analysis.overall_immunity_level);
                    info!("    • Protection effectiveness: {:.1}%", 
                          results.immunity_assessment.susceptibility_analysis.protection_effectiveness);
                    info!("    • Vulnerable components: {}", 
                          results.immunity_assessment.vulnerable_components.len());
                    
                    // Report vulnerable components
                    let high_risk_components = results.immunity_assessment.vulnerable_components.iter()
                        .filter(|v| matches!(v.risk_level, crate::emi_emc_analysis::RiskLevel::High | crate::emi_emc_analysis::RiskLevel::Critical))
                        .count();
                    
                    if high_risk_components > 0 {
                        warn!("    ⚠ {} components at high/critical risk for interference", high_risk_components);
                        
                        info!("    • High-risk components:");
                        for vulnerable in results.immunity_assessment.vulnerable_components.iter()
                            .filter(|v| matches!(v.risk_level, crate::emi_emc_analysis::RiskLevel::High | crate::emi_emc_analysis::RiskLevel::Critical))
                            .take(3) {
                            
                            info!("      - {} at {:.1} MHz (threshold: {:.1} dBμV/m)",
                                  vulnerable.component_name,
                                  vulnerable.susceptible_frequency / 1_000_000.0,
                                  vulnerable.immunity_threshold);
                        }
                    }
                    
                    // Report interference analysis
                    info!("  - Interference Analysis:");
                    info!("    • Internal interference sources: {}", 
                          results.interference_analysis.internal_interference.len());
                    info!("    • Crosstalk pairs analyzed: {}", 
                          results.interference_analysis.crosstalk_analysis.near_end_crosstalk.len());
                    info!("    • Power integrity issues: {}", 
                          results.interference_analysis.power_integrity_issues.len());
                    
                    let severe_interference = results.interference_analysis.internal_interference.iter()
                        .filter(|i| matches!(i.impact_severity, crate::emi_emc_analysis::ImpactSeverity::Severe | crate::emi_emc_analysis::ImpactSeverity::Critical))
                        .count();
                    
                    if severe_interference > 0 {
                        warn!("    ⚠ {} severe/critical interference issues detected", severe_interference);
                    }
                    
                    // Report worst-case crosstalk
                    if results.interference_analysis.crosstalk_analysis.worst_case_crosstalk > -30.0 {
                        warn!("    ⚠ Excessive crosstalk detected: {:.1} dB", 
                              results.interference_analysis.crosstalk_analysis.worst_case_crosstalk);
                    }
                    
                    // Report mitigation recommendations
                    info!("  - Mitigation Recommendations:");
                    info!("    • Total recommendations: {}", results.mitigation_recommendations.len());
                    
                    let critical_recs = results.mitigation_recommendations.iter()
                        .filter(|r| matches!(r.priority, crate::emi_emc_analysis::MitigationPriority::Critical))
                        .count();
                    
                    let high_recs = results.mitigation_recommendations.iter()
                        .filter(|r| matches!(r.priority, crate::emi_emc_analysis::MitigationPriority::High))
                        .count();
                    
                    info!("    • Critical priority: {}", critical_recs);
                    info!("    • High priority: {}", high_recs);
                    
                    if critical_recs > 0 || high_recs > 0 {
                        info!("    • Top recommendations:");
                        for (i, rec) in results.mitigation_recommendations.iter()
                            .filter(|r| matches!(r.priority, crate::emi_emc_analysis::MitigationPriority::Critical | crate::emi_emc_analysis::MitigationPriority::High))
                            .take(5)
                            .enumerate() {
                            
                            info!("      {}. [{}] {} - Effectiveness: {:.0}%",
                                  i + 1,
                                  match rec.priority {
                                      crate::emi_emc_analysis::MitigationPriority::Critical => "CRITICAL",
                                      crate::emi_emc_analysis::MitigationPriority::High => "HIGH",
                                      _ => "MEDIUM",
                                  },
                                  rec.description,
                                  rec.effectiveness * 100.0);
                        }
                    }
                    
                    // Report compliance summary
                    info!("  - Compliance Summary:");
                    info!("    • Overall compliance: {:?}", results.compliance_summary.overall_compliance);
                    info!("    • Standards passed: {}", results.compliance_summary.standards_passed.len());
                    info!("    • Standards failed: {}", results.compliance_summary.standards_failed.len());
                    info!("    • Estimated fix cost: {:?}", results.compliance_summary.estimated_fix_cost);
                    
                    // Report analysis performance
                    info!("  - Analysis Performance:");
                    info!("    • Components analyzed: {}", results.analysis_summary.components_analyzed);
                    info!("    • Nets analyzed: {}", results.analysis_summary.nets_analyzed);
                    info!("    • Frequencies analyzed: {}", results.analysis_summary.frequencies_analyzed);
                    info!("    • Analysis time: {:.2}s", results.analysis_summary.analysis_time_seconds);
                    info!("    • Prediction confidence: {:.1}%", results.analysis_summary.prediction_confidence * 100.0);
                    
                    // Summary status
                    match results.compliance_summary.overall_compliance {
                        crate::emi_emc_analysis::ComplianceLevel::Pass => {
                            info!("✅ EMI/EMC analysis: PASS - Circuit meets all EMC requirements");
                        },
                        crate::emi_emc_analysis::ComplianceLevel::PassWithMargin(_) => {
                            info!("✅ EMI/EMC analysis: PASS WITH MARGIN - Circuit exceeds EMC requirements");
                        },
                        crate::emi_emc_analysis::ComplianceLevel::Marginal(_) => {
                            warn!("⚠️ EMI/EMC analysis: MARGINAL - Circuit meets requirements but with limited margin");
                        },
                        crate::emi_emc_analysis::ComplianceLevel::Fail(_) => {
                            error!("❌ EMI/EMC analysis: FAIL - Circuit does not meet EMC requirements");
                        },
                    }
                    
                    // Store results for later use (e.g., compliance reporting)
                    debug!("EMI/EMC analysis data available for compliance documentation and design optimization");
                },
                Err(e) => {
                    error!("EMI/EMC analysis failed: {}", e);
                    warn!("Continuing synthesis without EMI/EMC analysis");
                    // Don't fail the entire synthesis due to EMI/EMC analysis failure
                }
            }
        } else {
            error!("EMI/EMC analyzer not initialized - this should not happen");
            return Err(anyhow::anyhow!("EMI/EMC analyzer initialization failed"));
        }
        
        Ok(())
    }

    /// Run reliability and lifecycle analysis
    async fn run_reliability_analysis(&mut self, analysis: &AnalysisResult) -> Result<()> {
        use crate::reliability_analysis::{ReliabilityAnalyzer, ReliabilityConfig};
        
        info!("Running reliability and lifecycle analysis on {} components", self.netlist.instances.len());
        
        // Initialize reliability analyzer if not already initialized
        if self.reliability_analyzer.is_none() {
            let mut config = ReliabilityConfig::default();
            
            // Configure analysis parameters
            config.analysis_period = 87600.0;  // 10 years in hours
            config.confidence_level = 0.95;    // 95% confidence
            config.enable_accelerated_testing = true;
            config.enable_physics_of_failure = true;
            config.enable_bayesian_analysis = false;
            config.enable_prognostics = true;
            config.temperature_cycling_enabled = true;
            config.burn_in_hours = 168.0;      // 1 week burn-in
            
            // Configure derating factors for conservative design
            config.derating_factors.voltage_derating = 0.8;  // 80% of maximum
            config.derating_factors.current_derating = 0.75; // 75% of maximum
            config.derating_factors.power_derating = 0.8;    // 80% of maximum
            config.derating_factors.temperature_derating = 10.0; // 10°C below maximum
            config.derating_factors.frequency_derating = 0.8; // 80% of maximum
            
            let analysis_period = config.analysis_period;
            let confidence_level = config.confidence_level;
            
            let analyzer = ReliabilityAnalyzer::with_config(config);
            self.reliability_analyzer = Some(analyzer);
            
            info!("Reliability analyzer initialized for {:.0}-year analysis with {}% confidence",
                  analysis_period / 8760.0, confidence_level * 100.0);
        }
        
        // Run reliability analysis
        if let Some(analyzer) = &mut self.reliability_analyzer {
            match analyzer.analyze_reliability(&self.netlist, analysis).await {
                Ok(results) => {
                    info!("Reliability analysis results:");
                    
                    // Report system-level reliability
                    info!("  - System Reliability:");
                    info!("    • Overall reliability: {:.4} ({:.2}%)", 
                          results.overall_system_reliability, 
                          results.overall_system_reliability * 100.0);
                    info!("    • Mean Time Between Failures: {:.0} hours ({:.1} years)", 
                          results.mean_time_between_failures, 
                          results.mean_time_between_failures / 8760.0);
                    info!("    • System failure rate: {:.2e} failures/hour", results.failure_rate);
                    
                    // Report component reliability summary
                    info!("  - Component Reliability:");
                    info!("    • Total components analyzed: {}", results.component_reliabilities.len());
                    
                    let low_reliability_count = results.component_reliabilities.values()
                        .filter(|c| c.reliability < 0.9)
                        .count();
                    
                    if low_reliability_count > 0 {
                        warn!("    ⚠ {} components with reliability < 90%", low_reliability_count);
                        
                        // Show worst components
                        let mut worst_components: Vec<_> = results.component_reliabilities.values().collect();
                        worst_components.sort_by(|a, b| a.reliability.partial_cmp(&b.reliability).unwrap());
                        
                        info!("    • Lowest reliability components:");
                        for (i, component) in worst_components.iter().take(3).enumerate() {
                            info!("      {}. {}: {:.3} ({:.1}% reliability)", 
                                  i + 1, 
                                  component.component_name, 
                                  component.reliability,
                                  component.reliability * 100.0);
                        }
                    }
                    
                    // Report critical components
                    info!("  - Critical Components:");
                    info!("    • Critical components identified: {}", results.critical_components.len());
                    
                    let single_point_failures = results.critical_components.iter()
                        .filter(|c| c.single_point_of_failure)
                        .count();
                    
                    if single_point_failures > 0 {
                        warn!("    ⚠ {} single points of failure detected", single_point_failures);
                    }
                    
                    // Show top critical components
                    if !results.critical_components.is_empty() {
                        info!("    • Top critical components:");
                        for (i, component) in results.critical_components.iter().take(5).enumerate() {
                            let impact_str = match component.failure_impact {
                                crate::reliability_analysis::ImpactSeverity::Catastrophic => "CATASTROPHIC",
                                crate::reliability_analysis::ImpactSeverity::Critical => "CRITICAL",
                                crate::reliability_analysis::ImpactSeverity::Marginal => "MARGINAL",
                                crate::reliability_analysis::ImpactSeverity::Negligible => "NEGLIGIBLE",
                            };
                            
                            info!("      {}. {} (Score: {:.2}, Impact: {}{})", 
                                  i + 1, 
                                  component.component_name,
                                  component.criticality_score,
                                  impact_str,
                                  if component.single_point_of_failure { ", SPOF" } else { "" });
                        }
                    }
                    
                    // Report failure predictions
                    info!("  - Failure Predictions:");
                    info!("    • Predicted failures in analysis period: {}", results.failure_predictions.len());
                    
                    let near_term_failures = results.failure_predictions.iter()
                        .filter(|f| f.predicted_failure_time < 8760.0) // Within 1 year
                        .count();
                    
                    if near_term_failures > 0 {
                        warn!("    ⚠ {} components predicted to fail within 1 year", near_term_failures);
                        
                        info!("    • Near-term failure predictions:");
                        for (i, prediction) in results.failure_predictions.iter()
                            .filter(|f| f.predicted_failure_time < 8760.0)
                            .take(3)
                            .enumerate() {
                            
                            info!("      {}. {} in {:.0} hours ({:.1} months, confidence: {:.0}%)",
                                  i + 1,
                                  prediction.component_name,
                                  prediction.predicted_failure_time,
                                  prediction.predicted_failure_time / (8760.0 / 12.0),
                                  prediction.prediction_confidence * 100.0);
                        }
                    }
                    
                    // Report lifecycle risks
                    info!("  - Lifecycle Risks:");
                    info!("    • Total lifecycle risks identified: {}", results.lifecycle_risks.len());
                    
                    let high_lifecycle_risks = results.lifecycle_risks.iter()
                        .filter(|r| matches!(r.risk_level, crate::reliability_analysis::RiskLevel::High | crate::reliability_analysis::RiskLevel::Critical))
                        .count();
                    
                    if high_lifecycle_risks > 0 {
                        warn!("    ⚠ {} high/critical lifecycle risks", high_lifecycle_risks);
                        
                        info!("    • High-priority lifecycle risks:");
                        for (i, risk) in results.lifecycle_risks.iter()
                            .filter(|r| matches!(r.risk_level, crate::reliability_analysis::RiskLevel::High | crate::reliability_analysis::RiskLevel::Critical))
                            .take(3)
                            .enumerate() {
                            
                            let risk_type_str = match risk.risk_type {
                                crate::reliability_analysis::LifecycleRiskType::ComponentObsolescence => "Obsolescence",
                                crate::reliability_analysis::LifecycleRiskType::SupplierDiscontinuation => "Supplier Risk",
                                crate::reliability_analysis::LifecycleRiskType::TechnologySupersession => "Technology",
                                crate::reliability_analysis::LifecycleRiskType::RegulatoryChange => "Regulatory",
                            };
                            
                            info!("      {}. {} - {} ({:.1} years): {}",
                                  i + 1,
                                  risk.component_name,
                                  risk_type_str,
                                  risk.time_horizon,
                                  risk.impact_description);
                        }
                    }
                    
                    // Report maintenance recommendations
                    info!("  - Maintenance Recommendations:");
                    info!("    • Total maintenance items: {}", results.maintenance_recommendations.len());
                    
                    let critical_maintenance = results.maintenance_recommendations.iter()
                        .filter(|m| matches!(m.priority, crate::reliability_analysis::MaintenancePriority::Critical))
                        .count();
                    
                    let high_maintenance = results.maintenance_recommendations.iter()
                        .filter(|m| matches!(m.priority, crate::reliability_analysis::MaintenancePriority::High))
                        .count();
                    
                    info!("    • Critical priority: {}", critical_maintenance);
                    info!("    • High priority: {}", high_maintenance);
                    
                    if critical_maintenance > 0 || high_maintenance > 0 {
                        info!("    • Priority maintenance items:");
                        for (i, maintenance) in results.maintenance_recommendations.iter()
                            .filter(|m| matches!(m.priority, crate::reliability_analysis::MaintenancePriority::Critical | crate::reliability_analysis::MaintenancePriority::High))
                            .take(5)
                            .enumerate() {
                            
                            let priority_str = match maintenance.priority {
                                crate::reliability_analysis::MaintenancePriority::Critical => "CRITICAL",
                                crate::reliability_analysis::MaintenancePriority::High => "HIGH",
                                _ => "MEDIUM",
                            };
                            
                            info!("      {}. [{}] {} - Every {:.0} hours ({:.1} months)",
                                  i + 1,
                                  priority_str,
                                  maintenance.component_name,
                                  maintenance.recommended_interval,
                                  maintenance.recommended_interval / (8760.0 / 12.0));
                        }
                    }
                    
                    // Report derating analysis
                    info!("  - Derating Analysis:");
                    info!("    • Overall derating compliance: {:.1}%", 
                          results.derating_analysis.overall_derating_compliance);
                    info!("    • Voltage derating: {:.1}% compliant", 
                          results.derating_analysis.voltage_derating_status.compliance_percentage);
                    info!("    • Current derating: {:.1}% compliant", 
                          results.derating_analysis.current_derating_status.compliance_percentage);
                    info!("    • Thermal derating: {:.1}% compliant", 
                          results.derating_analysis.thermal_derating_status.compliance_percentage);
                    
                    if results.derating_analysis.overall_derating_compliance < 90.0 {
                        warn!("    ⚠ Derating compliance below 90% - review component stress levels");
                    }
                    
                    // Report environmental impact
                    info!("  - Environmental Impact:");
                    info!("    • Temperature impact factor: {:.2}", results.environmental_impact.temperature_impact);
                    info!("    • Humidity impact factor: {:.2}", results.environmental_impact.humidity_impact);
                    info!("    • Vibration impact factor: {:.2}", results.environmental_impact.vibration_impact);
                    info!("    • Overall environmental factor: {:.2}", results.environmental_impact.overall_environmental_factor);
                    
                    if results.environmental_impact.overall_environmental_factor > 1.5 {
                        warn!("    ⚠ High environmental stress factor - consider environmental mitigation");
                    }
                    
                    // Report confidence intervals
                    info!("  - Statistical Confidence:");
                    info!("    • Reliability range: {:.3} - {:.3}", 
                          results.confidence_intervals.reliability_lower_bound,
                          results.confidence_intervals.reliability_upper_bound);
                    info!("    • MTBF range: {:.0} - {:.0} hours", 
                          results.confidence_intervals.mtbf_lower_bound,
                          results.confidence_intervals.mtbf_upper_bound);
                    
                    // Summary assessment
                    if results.overall_system_reliability > 0.95 {
                        info!("✅ Reliability analysis: EXCELLENT - System meets high reliability standards");
                    } else if results.overall_system_reliability > 0.90 {
                        info!("✅ Reliability analysis: GOOD - System meets reliability requirements");
                    } else if results.overall_system_reliability > 0.80 {
                        warn!("⚠️ Reliability analysis: MARGINAL - System reliability could be improved");
                    } else {
                        error!("❌ Reliability analysis: POOR - System reliability needs significant improvement");
                    }
                    
                    // Store results for later use (e.g., maintenance planning, lifecycle management)
                    debug!("Reliability analysis data available for maintenance planning and lifecycle management");
                },
                Err(e) => {
                    error!("Reliability analysis failed: {}", e);
                    warn!("Continuing synthesis without reliability analysis");
                    // Don't fail the entire synthesis due to reliability analysis failure
                }
            }
        } else {
            error!("Reliability analyzer not initialized - this should not happen");
            return Err(anyhow::anyhow!("Reliability analyzer initialization failed"));
        }
        
        Ok(())
    }
    
    /// Run predictive analytics and machine learning integration
    async fn run_predictive_analysis(&mut self, analysis: &AnalysisResult) -> Result<()> {
        use crate::predictive_analytics::{PredictiveAnalyzer, PredictiveConfig};
        
        info!("Running predictive analytics and machine learning integration on {} components", self.netlist.instances.len());
        
        // Initialize predictive analyzer if not already initialized
        if self.predictive_analyzer.is_none() {
            let mut config = PredictiveConfig::default();
            
            // Enable key ML models
            config.enabled_models.insert(crate::predictive_analytics::ModelType::ComponentSelection);
            config.enabled_models.insert(crate::predictive_analytics::ModelType::PerformancePrediction);
            config.enabled_models.insert(crate::predictive_analytics::ModelType::DesignCompletion);
            config.enabled_models.insert(crate::predictive_analytics::ModelType::AnomalyDetection);
            config.enabled_models.insert(crate::predictive_analytics::ModelType::ParameterTuning);
            config.enabled_models.insert(crate::predictive_analytics::ModelType::ThermalPrediction);
            config.enabled_models.insert(crate::predictive_analytics::ModelType::EMIPrediction);
            config.enabled_models.insert(crate::predictive_analytics::ModelType::ReliabilityPrediction);
            
            // Configure prediction parameters
            config.prediction_confidence_threshold = 0.8;
            config.max_prediction_time_ms = 30000; // 30 seconds max
            config.enable_explainable_ai = true;
            config.enable_uncertainty_quantification = true;
            config.enable_online_learning = false; // Off by default for stability
            
            let analyzer = PredictiveAnalyzer::with_config(config);
            self.predictive_analyzer = Some(analyzer);
            
            info!("Predictive analyzer initialized with ML algorithms: Random Forest, Gradient Boosting, SVM, Ensemble Methods");
        }
        
        // Run predictive analysis
        if let Some(analyzer) = &mut self.predictive_analyzer {
            match analyzer.analyze_predictive_insights(&self.netlist, analysis).await {
                Ok(results) => {
                    info!("Predictive analytics completed successfully:");
                    info!("  - Component recommendations: {}", results.component_recommendations.len());
                    info!("  - Performance predictions: {}", results.performance_predictions.len());
                    info!("  - Design completion suggestions: {}", results.design_completion_suggestions.len());
                    info!("  - Optimization opportunities: {}", results.optimization_opportunities.len());
                    info!("  - Risk assessments: {}", results.risk_assessments.len());
                    info!("  - Design pattern matches: {}", results.design_pattern_matches.len());
                    info!("  - Anomalies detected: {}", results.anomaly_detections.len());
                    
                    if results.component_recommendations.len() + results.optimization_opportunities.len() > 5 {
                        info!("✅ Predictive analysis: EXCELLENT - Multiple insights generated for design optimization");
                    } else if results.component_recommendations.len() + results.optimization_opportunities.len() > 2 {
                        info!("✅ Predictive analysis: GOOD - Several insights generated for improvement");
                    } else {
                        info!("✅ Predictive analysis: BASIC - Limited insights available with current data");
                    }
                    
                    // Store results for ML model training and future predictions
                    debug!("Predictive analytics data available for ML model improvement and future predictions");
                },
                Err(e) => {
                    error!("Predictive analytics failed: {}", e);
                    warn!("Continuing synthesis without predictive analytics");
                    // Don't fail the entire synthesis due to predictive analytics failure
                }
            }
        } else {
            error!("Predictive analyzer not initialized - this should not happen");
            return Err(anyhow::anyhow!("Predictive analyzer initialization failed"));
        }
        
        Ok(())
    }
    
    /// Run manufacturing and assembly optimization (DFM/DFA)
    async fn run_manufacturing_optimization(&mut self, analysis: &AnalysisResult) -> Result<()> {
        use crate::manufacturing_optimization::{ManufacturingOptimizer, ManufacturingConfig};
        
        info!("Running manufacturing and assembly optimization on {} components", self.netlist.instances.len());
        
        // Initialize manufacturing optimizer if not already initialized
        if self.manufacturing_optimizer.is_none() {
            let mut config = ManufacturingConfig::default();
            
            // Configure based on production intent
            config.target_process = crate::manufacturing_optimization::ManufacturingProcess::SmallBatch;
            config.assembly_method = crate::manufacturing_optimization::AssemblyMethod::FullySMT;
            config.target_volume = crate::manufacturing_optimization::ProductionVolume::MediumVolume;
            config.quality_level = crate::manufacturing_optimization::QualityLevel::Standard;
            
            // Enable optimization features
            config.enable_panelization = true;
            config.enable_testpoint_generation = true;
            config.enable_component_consolidation = true;
            config.enable_placement_optimization = true;
            config.enable_routing_optimization = true;
            
            // Set targets
            config.target_yield = 0.95;
            config.max_board_layers = 4;
            
            let optimizer = ManufacturingOptimizer::with_config(config);
            self.manufacturing_optimizer = Some(optimizer);
            
            info!("Manufacturing optimizer initialized for small batch SMT production");
        }
        
        // Run manufacturing analysis
        if let Some(optimizer) = &mut self.manufacturing_optimizer {
            match optimizer.analyze_manufacturing(&self.netlist, analysis).await {
                Ok(results) => {
                    info!("Manufacturing optimization results:");
                    info!("  - DFM Score: {:.1}%", results.dfm_score * 100.0);
                    info!("  - DFA Score: {:.1}%", results.dfa_score * 100.0);
                    info!("  - Estimated Yield: {:.1}%", results.estimated_yield * 100.0);
                    info!("  - Unit Cost: ${:.2}", results.estimated_cost.total_unit_cost);
                    info!("  - Violations: {}", results.violations.len());
                    info!("  - Warnings: {}", results.warnings.len());
                    info!("  - Optimization Suggestions: {}", results.suggestions.len());
                    
                    // Report critical violations
                    let critical_violations = results.violations.iter()
                        .filter(|v| matches!(v.severity, crate::manufacturing_optimization::ViolationSeverity::Critical))
                        .count();
                    
                    if critical_violations > 0 {
                        error!("  ⚠ {} critical manufacturing violations found - design changes required", critical_violations);
                        for violation in results.violations.iter()
                            .filter(|v| matches!(v.severity, crate::manufacturing_optimization::ViolationSeverity::Critical))
                            .take(3) {
                            error!("    - {}: {}", violation.location, violation.description);
                        }
                    }
                    
                    // Report panelization if enabled
                    if let Some(panel) = &results.panelization {
                        info!("  - Panelization: {}x{} boards per panel, {:.1}% utilization",
                              panel.panel_layout.rows,
                              panel.panel_layout.columns,
                              panel.utilization * 100.0);
                    }
                    
                    // Report test coverage
                    info!("  - Test Coverage:");
                    info!("    • ICT: {:.1}%", results.test_coverage.in_circuit_test_coverage * 100.0);
                    info!("    • Boundary Scan: {:.1}%", results.test_coverage.boundary_scan_coverage * 100.0);
                    info!("    • Functional: {:.1}%", results.test_coverage.functional_test_coverage * 100.0);
                    
                    // Report assembly sequence
                    info!("  - Assembly Steps: {}", results.assembly_sequence.len());
                    let total_time: f64 = results.assembly_sequence.iter()
                        .map(|s| s.time_estimate)
                        .sum();
                    info!("    • Total assembly time: {:.1} minutes", total_time);
                    
                    // Report critical components
                    if !results.critical_components.is_empty() {
                        info!("  - Critical Components: {}", results.critical_components.len());
                        for component in results.critical_components.iter().take(3) {
                            info!("    • {}: {:?}", component.component_name, component.criticality_reason);
                        }
                    }
                    
                    // Overall assessment
                    if results.dfm_score > 0.9 && results.dfa_score > 0.9 {
                        info!("✅ Manufacturing optimization: EXCELLENT - Design ready for production");
                    } else if results.dfm_score > 0.8 && results.dfa_score > 0.8 {
                        info!("✅ Manufacturing optimization: GOOD - Minor improvements recommended");
                    } else if results.dfm_score > 0.7 && results.dfa_score > 0.7 {
                        warn!("⚠️ Manufacturing optimization: MODERATE - Several improvements needed");
                    } else {
                        error!("❌ Manufacturing optimization: POOR - Significant redesign recommended");
                    }
                    
                    // Store results for production planning
                    debug!("Manufacturing analysis data available for production planning and cost estimation");
                },
                Err(e) => {
                    error!("Manufacturing optimization failed: {}", e);
                    warn!("Continuing synthesis without manufacturing optimization");
                    // Don't fail the entire synthesis due to manufacturing optimization failure
                }
            }
        } else {
            error!("Manufacturing optimizer not initialized - this should not happen");
            return Err(anyhow::anyhow!("Manufacturing optimizer initialization failed"));
        }
        
        Ok(())
    }
}

impl Default for NetlistGenerator {
    fn default() -> Self {
        Self::new()
    }
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
    println!("DEBUG: populate_instance_attributes called!");
    println!("DEBUG: Transferring parameters for instance {:?} (handle: {})", instance_id, handle_name);
    
    let inference_components = analysis.component_inference.get_inferred_components();
    println!("DEBUG: Total inferred components: {}", inference_components.len());
    
    for (idx, component) in inference_components.iter().enumerate() {
        println!("DEBUG: Component {}: type={}, instance_name={:?}, params={}",
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
                println!("DEBUG: Found inference data for handle '{}', component type: {}", handle_name, component.component_type);
                
                // Extract parameters from the component suggestion
                for param in &component.parameters {
                    let param_value = match &param.value {
                        ParameterValue::Resistance(r) => r.to_string(),
                        ParameterValue::Capacitance(c) => c.to_string(),
                        ParameterValue::Voltage(v) => v.to_string(),
                        ParameterValue::Current(i) => i.to_string(),
                        ParameterValue::String(s) => s.clone(),
                        ParameterValue::Real(r) => r.to_string(),
                        ParameterValue::Integer(i) => i.to_string(),
                        _ => continue, // Skip unsupported parameter types
                    };
                    
                    println!("DEBUG: Setting parameter '{}' = '{}' for instance {:?}", param.name, param_value, instance_id);
                    
                    // Store the parameter in the netlist instance attributes
                    if let Some(instance) = netlist.instances.get_mut(instance_id) {
                        instance.attributes.insert(param.name.clone(), param_value);
                        println!("DEBUG: Successfully stored parameter in netlist instance");
                    } else {
                        println!("DEBUG: Failed to find instance {:?} in netlist", instance_id);
                    }
                }
                
                return; // Found the component, we're done
            }
        }
    }
    
    println!("DEBUG: No component inference data found for handle '{}' (instance {:?})", handle_name, instance_id);
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
        assert!(!config.flatten_hierarchy);
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