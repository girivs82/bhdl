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
use bhdl_common::pin_metadata::{ModulePinMetadata, PinMetadata, PinDirection as CommonPinDirection, PinType as CommonPinType};
use bhdl_stdlib::{StdlibReader, get_default_stdlib_path};


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
    // BHDL stdlib reader for component definitions
    stdlib_reader: StdlibReader,
}

impl NetlistGenerator {
    /// Create a new netlist generator with default configuration
    pub fn new() -> Self {
        Self::with_config(NetlistConfig::default())
    }

    /// Create a new netlist generator with custom configuration
    pub fn with_config(config: NetlistConfig) -> Self {
        // Initialize stdlib reader and load all components
        let mut stdlib_reader = StdlibReader::new(get_default_stdlib_path());
        if let Err(e) = stdlib_reader.load_all_components() {
            warn!("Failed to load stdlib components: {}", e);
        }
        
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
            stdlib_reader,
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
        
        // Phase 0: Initialize database mapper if needed
        if self.database_mapper.is_none() && self.config.include_component_inference && self.config.database_path.is_some() {
            if let Err(e) = self.initialize_database_mapper().await {
                warn!("Failed to initialize database mapper: {}", e);
                // Continue without database mapper - will use fallback
            }
        }
        
        // Phase 1: Extract board/module hierarchy from analysis
        // Always extract to ensure top-level module is created
        self.extract_module_hierarchy(analysis)?;
        
        // Phase 2: Generate database component instances if mapper is available
        if self.database_mapper.is_some() {
            self.generate_database_component_instances(analysis).await?;
        } else {
            // Fallback to semantic instance generation if database unavailable
            self.generate_instances_with_semantics(analysis)?;
        }
        
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
            for component_suggestion in &component_context.inferred_components {
                let component_type = &component_suggestion.component_type;
                
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
                    self.add_pins_for_component(&instance_name, component_type, module_id)?;
                    
                    // Create instance of this component
                    let instance_id = self.netlist.add_instance(
                        instance_name.clone(),
                        module_id
                    ).expect("Failed to add instance");
                    
                    // Create pin instances for this component instance
                    self.netlist.create_pin_instances(instance_id)
                        .map_err(|e| anyhow::anyhow!("Failed to create pin instances: {}", e))?;
                    
                    self.ast_to_module.insert(component_type.clone(), module_id);
                    self.ast_to_instance.insert(instance_name.clone(), instance_id);
                    
                    debug!("Created instance '{}' of type '{}' with semantic kind {:?}", 
                           instance_name, component_type, module_kind);
                }
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
        
        if !self.config.flatten_hierarchy {
            // Use hierarchical connectivity extraction
            info!("Using hierarchical connectivity extraction");
            hierarchical_connectivity::extract_hierarchical_connectivity(ast, analysis, &mut self.netlist)?;
        } else {
            // Use flat extraction for backward compatibility
            info!("Using flat connectivity extraction");
            
            // First create power nets
            self.create_power_nets(analysis)?;
            
            // Now traverse the AST to find all connection statements
            let mut connection_count = 0;
            self.visit_connections_in_ast(ast.syntax(), &mut connection_count)?;
            
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
    fn visit_connections_in_ast(&mut self, node: &bhdl_ast::SyntaxNode<bhdl_ast::BhdlLanguage>, count: &mut usize) -> Result<()> {
        use bhdl_ast::{SyntaxKind, AstNode, ConnectionStmt};
        
        // Check if this is a connection statement
        if node.kind() == SyntaxKind::CONNECTION_STMT {
            if let Some(conn_stmt) = ConnectionStmt::cast(node.clone()) {
                // First pass: identify and map component handles
                self.identify_component_handles(&conn_stmt)?;
                // Second pass: process connections
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
    fn process_connection_statement(&mut self, conn: &bhdl_ast::ConnectionStmt) -> Result<()> {
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
                // Debug: show all available mappings
                info!("Available component mappings:");
                for (key, mapping) in mapper.get_all_mappings() {
                    info!("  '{}' -> '{}' (category: {:?})", key, mapping.component_name, mapping.category);
                }
                
                match mapper.create_component_instance(&instance_name, component_type).await {
                    Ok(component_instance) => {
                        info!("Created database component instance: {} -> {} (ID: {})", 
                              component_instance.instance_name,
                              component_instance.component_name,
                              component_instance.component_id);
                        
                        component_data.push((instance_name, component_type.clone(), component_instance));
                    }
                    Err(e) => {
                        warn!("Failed to create database component instance for {} (type: '{}'): {}", instance_name, component_type, e);
                        // Continue with other components
                    }
                }
            }
        }
        
        // Now process all the collected component data
        for (instance_name, component_type, component_instance) in component_data {
            // Create netlist module for this database component
            // IMPORTANT: Use BHDL type (e.g., "Res") not database name (e.g., "R")
            let module_id = self.netlist.add_module(
                component_type.clone(),  // Use BHDL type instead of database component name
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
            
            // Store mappings - use BHDL type for module mapping
            self.ast_to_module.insert(component_type.clone(), module_id);
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
        self.database_mapper.is_some()
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
    fn add_pins_for_component(&mut self, _instance_name: &str, component_type: &str, module_id: ModuleId) -> Result<()> {
        // Get pin definitions from the stdlib reader
        let pin_definitions = self.stdlib_reader.get_component_pins(component_type);
        
        debug!("Adding {} pins for component type '{}' to module", 
               pin_definitions.len(), component_type);
        
        // Add each pin to the netlist module
        for pin_def in pin_definitions {
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

/// Compatibility alias for NetlistGenerator
/// This provides backwards compatibility for existing code
pub type Synthesizer = NetlistGenerator;

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