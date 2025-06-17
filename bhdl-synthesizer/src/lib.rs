//! BHDL Synthesizer - Converts semantic analysis results to netlists
//! 
//! This crate bridges the gap between the BHDL analyzer (semantic analysis)
//! and the netlist representation, preserving semantic context for intelligent
//! visualization and layout.

use anyhow::{Result, Context};
use std::collections::HashMap;
use std::path::Path;
use log::{debug, info, warn};
use bhdl_common::ComponentTypeMapper;

// Component database mapping module  
pub mod component_mapping;

// Re-export key types
pub use bhdl_analyzer::types::AnalysisResult;
pub use bhdl_netlist::{Netlist, ModuleId, InstanceId, NetId, PortId, PinId, PinInstanceId, PinInstance};
pub use bhdl_netlist::types::{ModuleKind, PortDirection, PinDirection, PinType, ConnectionPoint, Unit, Quantity, NetClass};
pub use bhdl_ast::{SourceFile, Board, Module, ComponentDef};
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
    /// Use database components instead of generic components
    pub use_database_components: bool,
    /// Path to component database
    pub database_path: Option<String>,
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
}

impl Default for NetlistConfig {
    fn default() -> Self {
        Self {
            preserve_semantic_context: true,
            include_power_domains: true,
            include_component_inference: true,
            flatten_hierarchy: false,
            use_database_components: true,
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
    // Database component mapper for real component instances
    database_mapper: Option<DatabaseComponentMapper>,
    // Component instances with database symbol references
    component_instances: Vec<DatabaseComponentInstance>,
    // Unified component type mapper
    type_mapper: ComponentTypeMapper,
}

impl NetlistGenerator {
    /// Create a new netlist generator with default configuration
    pub fn new() -> Self {
        Self::with_config(NetlistConfig::default())
    }

    /// Create a new netlist generator with custom configuration
    pub fn with_config(config: NetlistConfig) -> Self {
        Self {
            config,
            netlist: Netlist::new(),
            ast_to_module: HashMap::new(),
            ast_to_instance: HashMap::new(),
            ast_to_net: HashMap::new(),
            database_mapper: None, // Will be initialized async in generate_from_analysis
            component_instances: Vec::new(),
            type_mapper: ComponentTypeMapper::new(),
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
        info!("Starting netlist generation from analysis results");
        
        // Phase 0: Initialize database mapper if enabled
        if self.config.use_database_components && self.database_mapper.is_none() {
            self.initialize_database_mapper().await?;
        }
        
        // Phase 1: Extract board/module hierarchy from analysis
        self.extract_module_hierarchy(analysis)?;
        
        // Phase 2: Generate instances and preserve semantic context (skip if using database components)
        if !self.config.use_database_components {
            self.generate_instances_with_semantics(analysis)?;
        }
        
        // Phase 3: Generate database component instances if enabled
        if self.config.use_database_components && self.database_mapper.is_some() {
            self.generate_database_component_instances(analysis).await?;
        }
        
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

        info!("Netlist generation complete: {} modules, {} instances, {} nets, {} database components", 
              self.netlist.modules.len(), 
              self.netlist.instances.len(), 
              self.netlist.nets.len(),
              self.component_instances.len());

        Ok(std::mem::take(&mut self.netlist))
    }

    /// Extract module hierarchy from analysis results
    fn extract_module_hierarchy(&mut self, _analysis: &AnalysisResult) -> Result<()> {
        debug!("Extracting module hierarchy from analysis");
        
        // TODO: Traverse the global scope and definition scopes to extract modules
        // For now, create a basic top-level module
        let top_module_id = self.netlist.add_module(
            "top_level".to_string(), 
            ModuleKind::Board
        );
        self.netlist.top_level_module = Some(top_module_id);
        self.ast_to_module.insert("top_level".to_string(), top_module_id);

        debug!("Created top-level module: {:?}", top_module_id);
        Ok(())
    }

    /// Generate instances with semantic context preservation
    fn generate_instances_with_semantics(&mut self, analysis: &AnalysisResult) -> Result<()> {
        debug!("Generating instances with semantic context");

        // Extract component inference results for semantic context
        if self.config.include_component_inference {
            let component_context = &analysis.component_inference;
            
            // Create instances based on inferred components
            for component_suggestion in &component_context.inferred_components {
                let component_name = format!("comp_{}", component_suggestion.component_type);
                let component_type = &component_suggestion.component_type;
                let module_kind = self.map_component_type_to_module_kind(component_type);
                
                // Create module definition for this component type
                let module_id = self.netlist.add_module(
                    component_type.clone(),
                    module_kind
                );
                
                // Create instance of this component
                let instance_id = self.netlist.add_instance(
                    component_name.clone(),
                    module_id
                ).expect("Failed to add instance");
                
                self.ast_to_module.insert(component_type.clone(), module_id);
                self.ast_to_instance.insert(component_name.clone(), instance_id);
                
                debug!("Created instance '{}' of type '{}' with semantic kind {:?}", 
                       component_name, component_type, module_kind);
            }
        }

        Ok(())
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
        
        // First create power nets
        self.create_power_nets(analysis)?;
        
        // Now traverse the AST to find all connection statements
        let mut connection_count = 0;
        self.visit_connections_in_ast(ast.syntax(), &mut connection_count)?;
        
        info!("Extracted {} connections from AST", connection_count);
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
            
            // Create nets for all power domains
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
            }
        }
        Ok(())
    }
    
    /// Visit all connection statements in the AST
    fn visit_connections_in_ast(&mut self, node: &bhdl_ast::SyntaxNode<bhdl_ast::BhdlLanguage>, count: &mut usize) -> Result<()> {
        use bhdl_ast::{SyntaxKind, AstNode, ConnectionStmt};
        
        // Check if this is a connection statement
        if node.kind() == SyntaxKind::CONNECTION_STMT {
            if let Some(conn_stmt) = ConnectionStmt::cast(node.clone()) {
                self.process_connection_statement(&conn_stmt)?;
                *count += 1;
            }
        }
        
        // Recursively visit children
        for child in node.children() {
            self.visit_connections_in_ast(&child, count)?;
        }
        
        Ok(())
    }
    
    /// Process a single connection statement
    fn process_connection_statement(&mut self, conn: &bhdl_ast::ConnectionStmt) -> Result<()> {
        use bhdl_ast::AstNode;
        // Parse the connection to extract the flow of connections
        // Example: VIN -> C1: Cap(100uF, 25V).pos -> U1: LM7805(package="TO-220").IN;
        
        let conn_text = conn.syntax().text().to_string();
        info!("Processing connection: {}", conn_text.trim());
        
        // Debug: Check for D1 and C3 connections specifically
        if conn_text.contains("D1") || conn_text.contains("C3") {
            info!("  DEBUG: Found D1/C3 connection: {}", conn_text.trim());
        }
        
        // Parse the connection chain by splitting on ->
        let parts: Vec<&str> = conn_text.split("->").collect();
        
        if parts.len() < 2 {
            warn!("Invalid connection format: {}", conn_text);
            return Ok(());
        }
        
        // Track the net for this connection chain
        let mut current_net_id: Option<NetId> = None;
        
        // Process each connection endpoint
        for (i, part) in parts.iter().enumerate() {
            let endpoint = part.trim().trim_end_matches(';');
            let endpoint_info = self.parse_connection_endpoint(endpoint);
            
            info!("  Connection endpoint {}: {:?}", i, endpoint_info);
            
            match endpoint_info {
                ConnectionEndpoint::Net(net_name) => {
                    // This is a simple net reference (VCC, GND, etc.)
                    if current_net_id.is_none() {
                        current_net_id = Some(self.ensure_net_exists(&net_name));
                    }
                }
                ConnectionEndpoint::Pin(instance_name, pin_name) => {
                    // This is a component pin reference (C1.pos, U1.IN, etc.)
                    if let Some(instance_id) = self.ast_to_instance.get(&instance_name) {
                        if let Some(pin_inst_id) = self.netlist.find_pin_instance(*instance_id, &pin_name) {
                            // Connect this pin to the current net
                            let net_id = current_net_id.get_or_insert_with(|| {
                                self.netlist.add_net(None)
                            });
                            
                            self.netlist.connect(*net_id, ConnectionPoint::PinInstance(pin_inst_id))
                                .map_err(|e| anyhow::anyhow!("Failed to connect pin: {}", e))?;
                                
                            info!("    Connected {} to net {:?}", endpoint, net_id);
                            
                            // Debug: Check D1 and C3 connections
                            if instance_name.contains("D1") || instance_name.contains("C3") {
                                info!("    DEBUG: Connected {}.{} to net {:?}", instance_name, pin_name, net_id);
                            }
                        } else {
                            warn!("    Pin {} not found on instance {}", pin_name, instance_name);
                        }
                    } else {
                        warn!("    Instance {} not found", instance_name);
                    }
                }
                ConnectionEndpoint::NamedHandle(handle_name, _component_type) => {
                    // This is a named handle declaration (C1: Cap(...))
                    // The instance should already be created during component generation
                    if !self.ast_to_instance.contains_key(&handle_name) {
                        warn!("    Named handle {} not found in instances", handle_name);
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
    
    /// Parse a connection endpoint to determine its type
    fn parse_connection_endpoint(&self, endpoint: &str) -> ConnectionEndpoint {
        let trimmed = endpoint.trim();
        
        // Check for named handle declaration (C1: Cap(...))
        if let Some(colon_pos) = trimmed.find(':') {
            let handle_name = trimmed[..colon_pos].trim().to_string();
            
            // Extract component type
            let after_colon = trimmed[colon_pos + 1..].trim();
            if let Some(paren_pos) = after_colon.find('(') {
                let component_type = after_colon[..paren_pos].trim().to_string();
                
                // Check if there's also a pin after the component
                if let Some(dot_pos) = after_colon.rfind('.') {
                    let pin_name = after_colon[dot_pos + 1..].trim().to_string();
                    return ConnectionEndpoint::Pin(handle_name, pin_name);
                } else {
                    return ConnectionEndpoint::NamedHandle(handle_name, component_type);
                }
            }
        }
        
        // Check for pin reference (C1.pos)
        if let Some(dot_pos) = trimmed.find('.') {
            let instance_name = trimmed[..dot_pos].trim().to_string();
            let pin_name = trimmed[dot_pos + 1..].trim().to_string();
            return ConnectionEndpoint::Pin(instance_name, pin_name);
        }
        
        // Otherwise it's a simple net name
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

    /// Generate database component instances from BHDL component definitions
    async fn generate_database_component_instances(&mut self, analysis: &AnalysisResult) -> Result<()> {
        debug!("Generating database component instances");
        
        // Extract component instances from the inferred components in the analysis
        let inferred_components = analysis.component_inference.get_inferred_components();
        info!("Generating database components for {} inferred components", inferred_components.len());
        
        // Track component counts by type for proper reference designators
        let mut component_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        
        // Collect all component instances first
        let mut component_data = Vec::new();
        
        for (index, component_suggestion) in inferred_components.iter().enumerate() {
            let component_type = &component_suggestion.component_type;
            
            info!("Processing inferred component #{}: type='{}', reasoning='{}', instance_name={:?}", 
                  index, component_type, component_suggestion.reasoning, 
                  component_suggestion.instance_name);
            
            // Use the instance name from the suggestion if available
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
            info!("Using instance name: {} for component type: {}", instance_name, component_type);
            
            // Use the database mapper
            if let Some(mapper) = self.database_mapper.as_mut() {
                match mapper.create_component_instance(&instance_name, component_type).await {
                    Ok(component_instance) => {
                        info!("Created database component instance: {} -> {} (ID: {})", 
                              component_instance.instance_name,
                              component_instance.component_name,
                              component_instance.component_id);
                        
                        component_data.push((instance_name, component_type.clone(), component_instance));
                    }
                    Err(e) => {
                        warn!("Failed to create database component instance for {}: {}", instance_name, e);
                        // Continue with other components
                    }
                }
            }
        }
        
        // Now process all the collected component data
        for (instance_name, component_type, component_instance) in component_data {
            // Create netlist module for this database component
            let module_id = self.netlist.add_module(
                component_instance.component_name.clone(),
                component_instance.get_module_kind()
            );
            
            // Add pins to the module based on component type
            self.add_pins_for_component(&instance_name, &component_type, module_id)?;
            
            // Create netlist instance
            let netlist_instance_id = self.netlist.add_instance(
                instance_name.clone(),
                module_id
            ).expect("Failed to add database component instance to netlist");
            
            // Create pin instances for this component instance
            self.netlist.create_pin_instances(netlist_instance_id)
                .map_err(|e| anyhow::anyhow!("Failed to create pin instances: {}", e))?;
            
            // Store mappings
            self.ast_to_module.insert(component_instance.component_name.clone(), module_id);
            self.ast_to_instance.insert(component_instance.instance_name.clone(), netlist_instance_id);
            
            // Store component instance for reference
            self.component_instances.push(component_instance);
        }
        
        info!("Generated {} database component instances", self.component_instances.len());
        Ok(())
    }

    /// Get the generated netlist (consumes the generator)
    pub fn into_netlist(self) -> Netlist {
        self.netlist
    }

    /// Get a reference to the current netlist
    pub fn netlist(&self) -> &Netlist {
        &self.netlist
    }

    /// Get the generated database component instances
    pub fn get_component_instances(&self) -> &[DatabaseComponentInstance] {
        &self.component_instances
    }

    /// Get database mapper statistics
    pub async fn get_database_stats(&self) -> Option<DatabaseMapperStats> {
        if let Some(mapper) = &self.database_mapper {
            Some(mapper.get_stats().await)
        } else {
            None
        }
    }

    /// Check if database component mapping is enabled and working
    pub fn is_database_enabled(&self) -> bool {
        self.config.use_database_components && self.database_mapper.is_some()
    }
    
    /// Add pins to a component module based on its type
    fn add_pins_for_component(&mut self, _instance_name: &str, component_type: &str, module_id: ModuleId) -> Result<()> {
        let component_lower = component_type.to_lowercase();
        
        // Determine pins based on component type
        if component_lower.contains("resistor") || component_type == "Res" {
            // Resistor has two passive pins
            self.netlist.add_pin(module_id, "1".to_string(), PinDirection::Passive, PinType::Passive);
            self.netlist.add_pin(module_id, "2".to_string(), PinDirection::Passive, PinType::Passive);
        } else if component_lower.contains("capacitor") || component_type == "Cap" {
            // Capacitor has pos and neg pins
            self.netlist.add_pin(module_id, "pos".to_string(), PinDirection::Passive, PinType::Passive);
            self.netlist.add_pin(module_id, "neg".to_string(), PinDirection::Passive, PinType::Passive);
        } else if component_lower.contains("led") || component_type == "LED" {
            // LED has anode and cathode
            self.netlist.add_pin(module_id, "A".to_string(), PinDirection::In, PinType::Signal);
            self.netlist.add_pin(module_id, "K".to_string(), PinDirection::Out, PinType::Signal);
        } else if component_type == "LM7805" || component_type.starts_with("LM78") {
            // Linear regulator pins
            self.netlist.add_pin(module_id, "IN".to_string(), PinDirection::Power, PinType::Power);
            self.netlist.add_pin(module_id, "GND".to_string(), PinDirection::Ground, PinType::Ground);
            self.netlist.add_pin(module_id, "OUT".to_string(), PinDirection::Power, PinType::Power);
        } else {
            // Generic component - add at least two pins
            self.netlist.add_pin(module_id, "1".to_string(), PinDirection::InOut, PinType::Signal);
            self.netlist.add_pin(module_id, "2".to_string(), PinDirection::InOut, PinType::Signal);
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
        warn!("Analysis produced {} diagnostics", analysis.diagnostics.len());
    }

    // Phase 2: Generate netlist with custom configuration
    let mut generator = NetlistGenerator::with_config(config);
    let netlist = generator.generate_from_analysis(&analysis).await
        .context("Failed to generate netlist with custom config")?;

    info!("Successfully generated custom netlist");
    Ok(netlist)
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