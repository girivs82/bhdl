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


// Component database mapping module  
pub mod component_mapping;

// Hierarchical connectivity extraction
pub mod hierarchical_connectivity;

// Module variant management
pub mod module_variants;

// Hierarchical reference designator generation
pub mod hierarchical_refdes;

// Interface synthesis
pub mod interface_synthesis;

// Passive component calculation engine
pub mod passive_component_calculator;

// Package selection engine  
pub mod package_selector;

// Import loader for handling BHDL imports
pub mod import_loader;

// Import preprocessor for pre-processing imports before analysis
pub mod import_preprocessor;

// Synthesis knowledge parser and storage
pub mod synthesis_knowledge;

// Virtual pin extraction from AST
pub mod virtual_pin_extractor;

// Intent-aware component generator
pub mod intent_aware_generator;

// Component value calculation engine
pub mod component_calculator;

// Re-export key types
pub use bhdl_analyzer::types::AnalysisResult;
pub use bhdl_analyzer::component_inference::ParameterValue;
pub use bhdl_netlist::{Netlist, ModuleId, InstanceId, NetId, PortId, PinId, PinInstanceId, PinInstance, Pin};
pub use bhdl_netlist::types::{ModuleKind, PortDirection, PinDirection, PinType, ConnectionPoint, Unit, Quantity, NetClass};
pub use bhdl_ast::{SourceFile, Board, Module, ComponentDef, HasName, AstNode, PinDecl};
pub use component_mapping::{DatabaseComponentMapper, DatabaseComponentInstance, DatabaseMapperStats};

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
    // When we see "protected_vin: TVSDiode(15V).1", we map "protected_vin" -> TVSDiode instance
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

    /// Extract virtual pin components from a module AST node
    fn extract_virtual_pins_from_module(&self, module: &bhdl_ast::Module) -> Option<Vec<bhdl_stdlib::virtual_pins::VirtualPinComponent>> {
        use crate::virtual_pin_extractor::VirtualPinExtractor;
        
        // Use the new virtual pin extractor to parse the module
        VirtualPinExtractor::extract_from_module(module)
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
        }
        
        // The analyzer has already processed imports and populated the global symbol table
        // We can now check for component definitions directly in the symbol table
        info!("Using analyzer's symbol table with {} symbols", analysis.global_scope.get_symbols().len());
        
        // Check for undefined components after processing imports
        if !analysis.diagnostics.is_empty() {
            let undefined_components: Vec<_> = analysis.diagnostics.iter()
                .filter(|d| d.message.contains("Undefined component"))
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
        self.extract_module_hierarchy(analysis)?;
        
        // Phase 2: Generate database component instances if mapper is available
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
        
        // Phase 2.5: Generate connections for supporting components (virtual pin expansion)
        self.generate_supporting_component_connections(analysis)?;
        
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
                if matches!(symbol.kind, bhdl_analyzer::symbol_table::SymbolKind::Module | 
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
                    let has_module_definition = analysis.global_scope.lookup(type_name)
                        .map(|s| s.kind == bhdl_analyzer::symbol_table::SymbolKind::Module)
                        .unwrap_or(false);
                    
                    if has_module_definition {
                        println!("✅ SYNTHESIZER: Found module definition for {} in symbol table", type_name);
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
                                    if let Some(module_node) = bhdl_ast::Module::cast(syntax_node) {
                                        info!("Successfully resolved {} AST node from symbol table", type_name);
                                        virtual_components_from_ast = self.extract_virtual_pins_from_module(&module_node);
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
        
        // Group components by IC they belong to
        let mut components_by_ic: HashMap<String, Vec<(String, InstanceId)>> = HashMap::new();
        for (name, id) in &self.supporting_component_instances {
            if let Some(ic_prefix) = name.split('_').next() {
                components_by_ic.entry(ic_prefix.to_string())
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
                
                // Parse component type and number from name (e.g., "U1_L1" -> "L", "1")
                let comp_parts: Vec<&str> = comp_name.split('_').collect();
                if comp_parts.len() < 2 {
                    continue;
                }
                
                let comp_type_str = comp_parts[1];
                let comp_type = comp_type_str.chars().next().unwrap_or('?');
                
                // Create pins if they don't exist and connect based on component type
                match comp_type {
                    'L' if comp_type_str == "L1" => {
                        // Inductor: SW -> L1.1, L1.2 -> VOUT
                        info!("Connecting inductor {}", comp_name);
                        let pin1 = self.get_or_create_pin(module_id, "1", PinDirection::In);
                        let pin2 = self.get_or_create_pin(module_id, "2", PinDirection::Out);
                        
                        // Create pin instances and connect
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
                        
                        // Add connections to nets
                        self.netlist.connect(sw_net, ConnectionPoint::PinInstance(pin_inst1)).ok();
                        self.netlist.connect(vout_net, ConnectionPoint::PinInstance(pin_inst2)).ok();
                    },
                    'C' => {
                        // Capacitor: Typically VOUT -> C.1, C.2 -> GND
                        info!("Connecting capacitor {}", comp_name);
                        let pin1 = self.get_or_create_pin(module_id, "1", PinDirection::In);
                        let pin2 = self.get_or_create_pin(module_id, "2", PinDirection::In);
                        
                        // Determine which nets to connect based on capacitor position
                        let (net1, net2) = if comp_name.contains("C1") || comp_name.contains("C2") {
                            // Bootstrap or output capacitors
                            (vout_net, gnd_net)
                        } else {
                            // Other capacitors default to VOUT/GND
                            (vout_net, gnd_net)
                        };
                        
                        let pin_inst1 = self.netlist.pin_instances.insert_with_key(|id| PinInstance {
                            id,
                            pin_def: pin1,
                            instance: comp_id,
                            net: Some(net1),
                            connection_name: None,
                        });
                        let pin_inst2 = self.netlist.pin_instances.insert_with_key(|id| PinInstance {
                            id,
                            pin_def: pin2,
                            instance: comp_id,
                            net: Some(net2),
                            connection_name: None,
                        });
                        
                        self.netlist.connect(net1, ConnectionPoint::PinInstance(pin_inst1)).ok();
                        self.netlist.connect(net2, ConnectionPoint::PinInstance(pin_inst2)).ok();
                    },
                    'R' => {
                        // Resistor: Feedback divider typically VOUT -> R1.1, R1.2 -> FB, FB -> R2.1, R2.2 -> GND
                        info!("Connecting resistor {}", comp_name);
                        let pin1 = self.get_or_create_pin(module_id, "1", PinDirection::In);
                        let pin2 = self.get_or_create_pin(module_id, "2", PinDirection::Out);
                        
                        let (net1, net2) = if comp_name.contains("R1") {
                            // Top feedback resistor: VOUT -> R1.1, R1.2 -> FB
                            (vout_net, fb_net)
                        } else if comp_name.contains("R2") {
                            // Bottom feedback resistor: FB -> R2.1, R2.2 -> GND
                            (fb_net, gnd_net)
                        } else {
                            // Other resistors default
                            (vout_net, gnd_net)
                        };
                        
                        let pin_inst1 = self.netlist.pin_instances.insert_with_key(|id| PinInstance {
                            id,
                            pin_def: pin1,
                            instance: comp_id,
                            net: Some(net1),
                            connection_name: None,
                        });
                        let pin_inst2 = self.netlist.pin_instances.insert_with_key(|id| PinInstance {
                            id,
                            pin_def: pin2,
                            instance: comp_id,
                            net: Some(net2),
                            connection_name: None,
                        });
                        
                        self.netlist.connect(net1, ConnectionPoint::PinInstance(pin_inst1)).ok();
                        self.netlist.connect(net2, ConnectionPoint::PinInstance(pin_inst2)).ok();
                    },
                    'D' if comp_type_str == "D1" => {
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
                    },
                    _ => {
                        info!("Unknown component type for {} (type={}, char={})", comp_name, comp_type_str, comp_type);
                    }
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
                
                let net_id = self.netlist.add_net_with_class(Some(domain_name.clone()), net_class.clone());
                self.ast_to_net.insert(domain_name.clone(), net_id);
                
                debug!("Created power net '{}' with voltage {:?} and class {:?}", 
                       domain_name, domain_info.voltage, net_class);
                
                // NEW: Create component instances for power and ground
                if domain_name.contains("GND") || domain_info.voltage == 0.0 {
                    // Create Ground component instance
                    let module_id = self.netlist.add_module(
                        "Ground".to_string(),
                        ModuleKind::PhysicalComponent
                    );
                    
                    // Add GND pin to the module
                    let pin_id = self.netlist.add_pin(
                        module_id,
                        "GND".to_string(),
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
                    // Create Power component instance
                    let module_id = self.netlist.add_module(
                        "Power".to_string(),
                        ModuleKind::PhysicalComponent
                    );
                    
                    // Add OUT pin to the module
                    let pin_id = self.netlist.add_pin(
                        module_id,
                        "OUT".to_string(),
                        PinDirection::Out,
                        PinType::Power
                    ).ok_or_else(|| anyhow::anyhow!("Failed to add OUT pin"))?;
                    
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
                SymbolKind::Module => SymbolType::Module,
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
            if matches!(symbol.kind, SymbolKind::Module | SymbolKind::Component | SymbolKind::Board) {
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
            if let Some(module) = preprocessor.get_imported_module(component_type) {
                println!("SYNTHESIZER: Using preprocessed imported module definition for '{}'", component_type);
                
                // Extract pins from the imported module
                let mut pins = Vec::new();
                for pin in module.pins() {
                    if let Some(name) = pin.name() {
                        let pin_text = pin.syntax().text().to_string();
                        let is_virtual = pin_text.contains("virtual");
                        
                        // Parse direction and type from pin declaration
                        let (direction, pin_type) = if pin_text.contains("power in") {
                            (bhdl_netlist::types::PinDirection::Power, bhdl_netlist::types::PinType::Power)
                        } else if pin_text.contains("power out") {
                            (bhdl_netlist::types::PinDirection::Power, bhdl_netlist::types::PinType::Power)
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
        } else if let Some(module) = self.import_loader.get_module(component_type) {
            // Legacy path: check import_loader if no preprocessor
            println!("SYNTHESIZER: Using legacy import_loader for '{}'", component_type);
            
            // Extract pins from the imported module
            let mut pins = Vec::new();
            for pin in module.pins() {
                if let Some(name) = pin.name() {
                    let pin_text = pin.syntax().text().to_string();
                    let is_virtual = pin_text.contains("virtual");
                    
                    // Parse direction and type from pin declaration
                    let (direction, pin_type) = if pin_text.contains("power in") {
                        (bhdl_netlist::types::PinDirection::Power, bhdl_netlist::types::PinType::Power)
                    } else if pin_text.contains("power out") {
                        (bhdl_netlist::types::PinDirection::Power, bhdl_netlist::types::PinType::Power)
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