//! Hierarchical connectivity extraction for BHDL synthesizer
//!
//! This module handles the extraction of connectivity information from
//! hierarchical entity designs, including:
//! - Entity definitions and their internal connections
//! - Entity instances and port mappings
//! - Hierarchical net resolution
//! - SPICE subcircuit generation

use anyhow::Result;
use std::collections::HashMap;
use bhdl_ast::{AstNode, SyntaxKind, Entity, EntityInst, ConnectionStmt, HasName, BinaryExpr};
use bhdl_analyzer::types::AnalysisResult;
use bhdl_netlist::{Netlist, ModuleId, InstanceId, NetId, ConnectionPoint};
use bhdl_parser::BhdlLanguage;
use rowan::SyntaxNode;
use log::{debug, info, warn};
use crate::entity_variants::EntityVariantManager;
use crate::populate_instance_attributes;

/// Context for hierarchical connectivity extraction
pub struct HierarchicalContext {
    /// Stack of module contexts (for nested modules)
    module_stack: Vec<ModuleContext>,
    /// Map from module name to ModuleId
    module_name_to_id: HashMap<String, ModuleId>,
    /// Map from hierarchical instance path to InstanceId
    instance_path_to_id: HashMap<String, InstanceId>,
    /// Current hierarchical path (e.g., "board.controller.pwm")
    current_path: Vec<String>,
    /// Entity variant manager for deduplication
    variant_manager: EntityVariantManager,
    /// Component counters for reference designator generation
    component_counters: HashMap<String, usize>,
    /// Map from interface instance names to their generated instance names
    /// e.g., "i2c_bus" -> "U1" (from component inference)
    interface_instance_mapping: HashMap<String, String>,
}

/// Context for a single module being processed
struct ModuleContext {
    /// Module name
    name: String,
    /// Module ID in the netlist
    module_id: ModuleId,
    /// Local nets within this module
    local_nets: HashMap<String, NetId>,
    /// Port connections (port name to net)
    port_connections: HashMap<String, NetId>,
}

impl HierarchicalContext {
    pub fn new() -> Self {
        Self {
            module_stack: Vec::new(),
            module_name_to_id: HashMap::new(),
            instance_path_to_id: HashMap::new(),
            current_path: Vec::new(),
            variant_manager: EntityVariantManager::new(),
            component_counters: HashMap::new(),
            interface_instance_mapping: HashMap::new(),
        }
    }
    
    /// Enter a module scope
    pub fn push_module(&mut self, name: String, module_id: ModuleId) {
        self.current_path.push(name.clone());
        self.module_stack.push(ModuleContext {
            name,
            module_id,
            local_nets: HashMap::new(),
            port_connections: HashMap::new(),
        });
    }
    
    /// Exit current module scope
    pub fn pop_module(&mut self) {
        self.module_stack.pop();
        self.current_path.pop();
    }
    
    /// Get current module context
    pub fn current_module(&self) -> Option<&ModuleContext> {
        self.module_stack.last()
    }
    
    /// Get current module context (mutable)
    pub fn current_module_mut(&mut self) -> Option<&mut ModuleContext> {
        self.module_stack.last_mut()
    }
    
    /// Get current hierarchical path as string
    pub fn current_path_string(&self) -> String {
        self.current_path.join(".")
    }
    
    /// Check if a net is a power or ground net based on netlist net classes
    fn is_power_or_ground_net(&self, net_name: &str, netlist: &Netlist) -> bool {
        for (_net_id, net) in netlist.nets.iter() {
            if let Some(existing_name) = &net.name {
                if existing_name == net_name {
                    // Check if this net has a power or ground class
                    use bhdl_netlist::NetClass;
                    match &net.net_class {
                        NetClass::Power(_) | NetClass::Ground => return true,
                        _ => {}
                    }
                }
            }
        }
        false
    }
    
    /// Resolve a net name in the current context
    pub fn resolve_net(&mut self, net_name: &str, netlist: &mut Netlist) -> Result<NetId> {
        // First check if this is an interface signal reference (e.g., i2c_bus.SDA)
        if net_name.contains('.') {
            let parts: Vec<&str> = net_name.split('.').collect();
            if parts.len() == 2 {
                let interface_name = parts[0];
                let signal_name = parts[1];
                
                // Look for any existing net that ends with _<signal_name>
                // This will catch interface nets created by interface synthesis
                for (net_id, net) in netlist.nets.iter() {
                    if let Some(existing_name) = &net.name {
                        // Check if this ends with the signal name (e.g., U1_SDA matches SDA)
                        if existing_name.ends_with(&format!("_{}", signal_name)) {
                            // Additional check: make sure the prefix looks like an interface instance
                            let prefix_end = existing_name.len() - signal_name.len() - 1;
                            if prefix_end > 0 {
                                let prefix = &existing_name[..prefix_end];
                                // Interface instances are typically named U1, U2, etc.
                                if prefix.starts_with("U") && prefix[1..].chars().all(|c| c.is_digit(10)) {
                                    info!("Resolved interface signal {} to existing net {}", net_name, existing_name);
                                    return Ok(net_id);
                                }
                            }
                        }
                    }
                }
                
                // If no interface net found, check for exact match
                for (net_id, net) in netlist.nets.iter() {
                    if let Some(existing_name) = &net.name {
                        if existing_name == net_name {
                            return Ok(net_id);
                        }
                    }
                }
                
                // No existing net found - DON'T create a new one with the interface signal name
                // Instead, create it with a temporary name and let interface synthesis handle it
                warn!("Interface signal {} referenced before interface synthesis - creating placeholder net", net_name);
            }
        }
        
        // Check if we're in a module context
        let in_module = self.current_module().is_some();
        println!("DEBUG: resolve_net('{}'), in_module: {}", net_name, in_module);
        
        if in_module {
            // Check if net already exists in local context
            if let Some(module_ctx) = self.current_module() {
                if let Some(&net_id) = module_ctx.local_nets.get(net_name) {
                    return Ok(net_id);
                }
            }
            
            // Create new net with hierarchical name
            let hier_net_name = if self.current_path.len() > 1 {
                format!("{}.{}", self.current_path_string(), net_name)
            } else {
                net_name.to_string()
            };
            
            let net_id = netlist.add_net(Some(hier_net_name));
            
            // Now update the module context
            if let Some(module_ctx) = self.current_module_mut() {
                module_ctx.local_nets.insert(net_name.to_string(), net_id);
            }
            
            Ok(net_id)
        } else {
            // Top level net - check if it already exists (e.g., power/ground nets)
            println!("DEBUG: Looking for existing net '{}'", net_name);
            for (net_id, net) in netlist.nets.iter() {
                if let Some(existing_name) = &net.name {
                    println!("DEBUG: Checking net {:?} with name '{}'", net_id, existing_name);
                    if existing_name == net_name {
                        println!("DEBUG: Found existing top-level net: {} -> {:?}", net_name, net_id);
                        return Ok(net_id);
                    }
                }
            }
            
            // Check if this should be a power/ground net but wasn't found
            // This can happen if the power declaration hasn't been processed yet
            if self.is_power_or_ground_net(net_name, netlist) {
                warn!("Power/ground net {} referenced but not found - this shouldn't happen", net_name);
            }
            
            // Create new net if not found
            let net_id = netlist.add_net(Some(net_name.to_string()));
            debug!("Created new top-level net: {} -> {:?}", net_name, net_id);
            Ok(net_id)
        }
    }
}

/// Extract hierarchical connectivity from AST
pub fn extract_hierarchical_connectivity(
    ast: &bhdl_ast::SourceFile,
    analysis: &AnalysisResult,
    netlist: &mut Netlist,
    import_preprocessor: Option<&crate::import_preprocessor::ImportPreprocessor>,
) -> Result<()> {
    println!("=== STARTING hierarchical connectivity extraction from AST ===");
    info!("=== STARTING hierarchical connectivity extraction from AST ===");
    
    let mut context = HierarchicalContext::new();
    
    // Build interface instance mapping from component inference results
    build_interface_instance_mapping(&mut context, analysis);
    
    // First pass: Create module definitions
    println!("=== Phase 1: Creating module definitions ===");
    info!("=== Phase 1: Creating module definitions ===");
    create_module_definitions(ast, analysis, netlist, &mut context, import_preprocessor)?;
    
    // Second pass: Process module instances and connections
    println!("=== Phase 2: Processing module hierarchy ===");
    info!("=== Phase 2: Processing module hierarchy ===");
    process_module_hierarchy(ast, analysis, netlist, &mut context, import_preprocessor)?;
    
    println!("=== COMPLETED hierarchical connectivity extraction ===");
    info!("=== COMPLETED hierarchical connectivity extraction ===");
    Ok(())
}

/// Build mapping from original interface instance names to generated names
fn build_interface_instance_mapping(context: &mut HierarchicalContext, analysis: &AnalysisResult) {
    // Component inference generates names like U1, U2 for interface instances
    // We need to map from the original names (like i2c_bus) to these generated names
    
    // First, find all interface types in the symbol table
    let mut interface_types = std::collections::HashSet::new();
    for symbol in analysis.global_scope.iter() {
        if symbol.kind == bhdl_analyzer::symbol_table::SymbolKind::Interface {
            interface_types.insert(symbol.name.clone());
        }
    }
    
    // Then check component inference results for interface instances
    for comp in &analysis.component_inference.inferred_components {
        if interface_types.contains(&comp.component_type) {
            // This is an interface instance
            // The component inference may have lost the original instance name
            // For now, we'll try to reconstruct it from the instance_name field
            if let Some(ref inst_name) = comp.instance_name {
                // inst_name might be like "U1" (generated)
                // We need to find the original name from the AST
                // This is a limitation - we should preserve the original name better
                debug!("Found interface instance {} of type {} in component inference", 
                      inst_name, comp.component_type);
            }
        }
    }
}

/// Create module definitions from AST
fn create_module_definitions(
    ast: &bhdl_ast::SourceFile,
    _analysis: &AnalysisResult,
    netlist: &mut Netlist,
    context: &mut HierarchicalContext,
    _import_preprocessor: Option<&crate::import_preprocessor::ImportPreprocessor>,
) -> Result<()> {
    use bhdl_ast::source_file::Item;
    use bhdl_netlist::types::ModuleKind;
    
    debug!("Creating module definitions");
    
    for item in ast.items() {
        match item {
            Item::Entity(entity) => {
                // Register entity definition with variant manager
                context.variant_manager.register_entity_definition(&entity);

                // Create a module entry for this entity AND register
                // its name → module_id so the second pass's
                // `process_module_hierarchy` can walk into the
                // body. Previously this was deferred to "on-demand
                // when instances are processed," but that path
                // never fires for entity *definitions* (only for
                // their instances), so entity bodies were skipped
                // entirely — and COMPONENT_INST lines inside a
                // child-sheet `entity { ... }` block produced no
                // netlist instances. Bug surfaced by the KiCad
                // importer's Arduino UNO round-trip (96 pin
                // instances, only 24 wired).
                if let Some(name) = entity.name() {
                    let entity_name = name.text().to_string();
                    let module_id = netlist.add_module(
                        entity_name.clone(),
                        ModuleKind::Module
                    );
                    context.module_name_to_id.insert(entity_name.clone(), module_id);
                    debug!("Created entity module: {}", entity_name);
                }
            }
            Item::Board(board) => {
                if let Some(name) = board.name() {
                    let board_name = name.text().to_string();
                    let module_id = netlist.add_module(
                        board_name.clone(),
                        ModuleKind::Board
                    );
                    
                    context.module_name_to_id.insert(board_name.clone(), module_id);
                    netlist.top_level_module = Some(module_id);
                    
                    debug!("Created board definition: {}", board_name);
                }
            }
            _ => {}
        }
    }
    
    Ok(())
}

/// Process module hierarchy and connections
fn process_module_hierarchy(
    ast: &bhdl_ast::SourceFile,
    analysis: &AnalysisResult,
    netlist: &mut Netlist,
    context: &mut HierarchicalContext,
    import_preprocessor: Option<&crate::import_preprocessor::ImportPreprocessor>,
) -> Result<()> {
    use bhdl_ast::source_file::Item;
    
    debug!("Processing module hierarchy");
    
    for item in ast.items() {
        match item {
            Item::Entity(entity) => {
                if let Some(name) = entity.name() {
                    let entity_name = name.text().to_string();
                    if let Some(&module_id) = context.module_name_to_id.get(&entity_name) {
                        context.push_module(entity_name, module_id);
                        process_entity_body(&entity, analysis, netlist, context, import_preprocessor)?;
                        context.pop_module();
                    }
                }
            }
            Item::Board(board) => {
                if let Some(name) = board.name() {
                    let board_name = name.text().to_string();
                    if let Some(&_module_id) = context.module_name_to_id.get(&board_name) {
                        // Don't push board as module context - boards use global nets
                        debug!("Processing board {} at top level", board_name);
                        process_board_body(&board, analysis, netlist, context, import_preprocessor)?;
                    }
                }
            }
            _ => {}
        }
    }
    
    Ok(())
}

/// Process entity body for instances and connections
fn process_entity_body(
    entity: &Entity,
    analysis: &AnalysisResult,
    netlist: &mut Netlist,
    context: &mut HierarchicalContext,
    import_preprocessor: Option<&crate::import_preprocessor::ImportPreprocessor>,
) -> Result<()> {
    // Process entity instances
    for entity_inst in entity.entity_instances() {
        process_entity_instance(&entity_inst, netlist, context, analysis, import_preprocessor)?;
    }

    // Process connections, flow statements, and component
    // instances inside the entity body. Component instances are
    // the `R1: Resistor("1k");` form (COMPONENT_INST) — the
    // semicolon-terminated single-line declaration that appears
    // wherever a passive or simple IC is used. Without this
    // branch the entity body's components are skipped entirely
    // and their pin_instances never get wired to internal nets.
    // (Bug surfaced by the KiCad importer's Arduino UNO
    // round-trip: 96 pin_instances created but only 24 wired,
    // the rest sitting on R/C/D instances inside subsheet
    // entity bodies.) Mirrors the COMPONENT_INST handling in
    // `process_board_body` below.
    for child in entity.syntax().children() {
        match child.kind() {
            SyntaxKind::COMPONENT_INST => {
                if let Some(comp_inst) = bhdl_ast::common::ComponentInst::cast(child.clone()) {
                    create_component_instance(&comp_inst, netlist, context, analysis, import_preprocessor)?;
                }
            }
            SyntaxKind::CONNECTION_STMT => {
                // Bare connection: process as flow (creates instances + wires in one pass)
                process_connection_stmt_as_flow(&child, netlist, context, analysis, import_preprocessor)?;
            }
            SyntaxKind::NET_FLOW_STMT => {
                debug!("Found NET_FLOW_STMT in module");
                process_net_flow_statement(&child, netlist, context, analysis, import_preprocessor)?;
            }
            _ => {}
        }
    }

    Ok(())
}

/// Process board body for instances and connections
fn process_board_body(
    board: &bhdl_ast::Board,
    analysis: &AnalysisResult,
    netlist: &mut Netlist,
    context: &mut HierarchicalContext,
    import_preprocessor: Option<&crate::import_preprocessor::ImportPreprocessor>,
) -> Result<()> {
    info!("=== Processing board body ===");
    
    // Process entity instances
    for entity_inst in board.entity_instances() {
        process_entity_instance(&entity_inst, netlist, context, analysis, import_preprocessor)?;
    }
    
    // Process connections and extract component instances
    println!("=== Scanning board children for connections ===");
    info!("=== Scanning board children for connections ===");
    for child in board.syntax().children() {
        println!("Board child kind: {:?}, text preview: '{}'", child.kind(), 
              child.text().to_string().chars().take(50).collect::<String>());
        info!("Board child kind: {:?}, text preview: '{}'", child.kind(), 
              child.text().to_string().chars().take(50).collect::<String>());
        match child.kind() {
            SyntaxKind::COMPONENT_INST => {
                // Process component instances directly
                if let Some(comp_inst) = bhdl_ast::common::ComponentInst::cast(child) {
                    println!("Processing COMPONENT_INST: {}", comp_inst.syntax().text());
                    info!("Processing COMPONENT_INST: {}", comp_inst.syntax().text());
                    create_component_instance(&comp_inst, netlist, context, analysis, import_preprocessor)?;
                }
            }
            SyntaxKind::CONNECTION_STMT => {
                // Bare connection like: VCC -> r1: Res(330).1 -> r1.2 -> led1: LED("red").A;
                // Process like a NET_FLOW_STMT but without a declared net name
                process_connection_stmt_as_flow(&child, netlist, context, analysis, import_preprocessor)?;
            }
            SyntaxKind::NET_FLOW_STMT => {
                // Handle net flow statements like: net led_circuit: @VCC -> R1: Res(330).1 -> ...
                println!("Found NET_FLOW_STMT in board - processing it!");
                info!("Found NET_FLOW_STMT in board");
                process_net_flow_statement(&child, netlist, context, analysis, import_preprocessor)?;
            }
            _ => {}
        }
    }

    Ok(())
}

/// Process an entity instance
fn process_entity_instance(
    entity_inst: &EntityInst,
    netlist: &mut Netlist,
    context: &mut HierarchicalContext,
    analysis: &AnalysisResult,
    import_preprocessor: Option<&crate::import_preprocessor::ImportPreprocessor>,
) -> Result<()> {
    let instance_name = entity_inst.name()
        .map(|t| t.text().to_string())
        .ok_or_else(|| anyhow::anyhow!("Entity instance missing name"))?;

    let entity_type = entity_inst.entity_type()
        .map(|t| t.text().to_string())
        .ok_or_else(|| anyhow::anyhow!("Entity instance missing type"))?;

    // Get or create entity variant based on parameters
    let module_id = context.variant_manager.get_or_create_variant(
        entity_inst,
        netlist,
        analysis
    )?;
    
    // Create instance
    let instance_id = netlist.add_instance(instance_name.clone(), module_id)
        .ok_or_else(|| anyhow::anyhow!("Failed to add instance"))?;

    // Propagate module-level attributes (including component_class) to instance
    if let Some(module) = netlist.modules.get(module_id) {
        let module_attrs = module.attributes.clone();
        if let Some(instance) = netlist.instances.get_mut(instance_id) {
            for (key, value) in &module_attrs {
                if !instance.attributes.contains_key(key) {
                    instance.attributes.insert(key.clone(), value.clone());
                }
            }
        }
    }

    // Store instance path
    let instance_path = if context.current_path.is_empty() {
        instance_name.clone()
    } else {
        format!("{}.{}", context.current_path_string(), instance_name)
    };
    context.instance_path_to_id.insert(instance_path.clone(), instance_id);
    
    // Create pin instances
    netlist.create_pin_instances(instance_id)
        .map_err(|e| anyhow::anyhow!("Failed to create pin instances: {}", e))?;
    
    // Transfer component parameters from analyzer to instance
    populate_instance_attributes(netlist, instance_id, &instance_name, analysis);
    
    // Process port mappings
    for port_mapping in entity_inst.port_mappings() {
        process_port_mapping(&port_mapping, instance_id, netlist, context)?;
    }

    debug!("Created entity instance: {} of type {}", instance_name, entity_type);

    // Now process the entity's internal components
    // First, check if we have the entity definition
    let has_entity_def = context.variant_manager.find_entity_definition(&entity_type).is_some();

    if has_entity_def {
        // Push the instance context
        context.push_module(instance_name.clone(), module_id);

        // Clone the entity definition to avoid borrow issues
        let entity_def = context.variant_manager.find_entity_definition(&entity_type)
            .unwrap()
            .clone();

        process_entity_body(&entity_def, analysis, netlist, context, import_preprocessor)?;

        // Pop the instance context
        context.pop_module();
    }
    
    Ok(())
}

/// Process a port mapping
fn process_port_mapping(
    port_mapping: &bhdl_ast::hierarchical::PortMapping,
    instance_id: InstanceId,
    netlist: &mut Netlist,
    context: &mut HierarchicalContext,
) -> Result<()> {
    // Get the pin name from the port mapping
    let pin_name = port_mapping.pin_ref()
        .and_then(|p| p.name())
        .map(|n| n.text().to_string())
        .ok_or_else(|| anyhow::anyhow!("Port mapping missing pin name"))?;
    
    // Get the target net name from the connection target
    let target_net_name = port_mapping.connection_target()
        .map(|t| t.syntax().text().to_string().trim().to_string())
        .ok_or_else(|| anyhow::anyhow!("Port mapping missing connection target"))?;
    
    // Find the pin instance for this pin
    let pin_inst_id = netlist.find_pin_instance(instance_id, &pin_name)
        .ok_or_else(|| anyhow::anyhow!("Pin instance not found for pin {} on instance {:?}", pin_name, instance_id))?;
    
    // Resolve the net in current context
    let net_id = context.resolve_net(&target_net_name, netlist)?;
    
    // Connect the pin instance to the net
    netlist.connect(net_id, ConnectionPoint::PinInstance(pin_inst_id))
        .map_err(|e| anyhow::anyhow!("Failed to connect pin to net: {}", e))?;
    
    debug!("Connected {}:{} to net {}", 
           netlist.instances.get(instance_id).map(|i| &i.name).unwrap_or(&"<unknown>".to_string()),
           pin_name, 
           target_net_name);
    
    Ok(())
}

/// Process a connection statement within a module
fn process_connection_in_module(
    conn_stmt: &ConnectionStmt,
    netlist: &mut Netlist,
    context: &mut HierarchicalContext,
) -> Result<()> {
    // Extract connection endpoints from the binary expression
    if let Some(expr_node) = conn_stmt.expr() {
        if let Some(binary_expr) = BinaryExpr::cast(expr_node) {
            // Get left and right sides of the connection
            let left_text = binary_expr.lhs()
                .map(|e| e.syntax().text().to_string().trim().to_string())
                .unwrap_or_default();
            let right_text = binary_expr.rhs()
                .map(|e| e.syntax().text().to_string().trim().to_string())
                .unwrap_or_default();
            
            if left_text.is_empty() || right_text.is_empty() {
                warn!("Connection statement has empty endpoints");
                return Ok(());
            }
            
            // Check what type of connection this is by looking at the operator
            let operator = get_binary_operator(&binary_expr);
            
            match operator.as_str() {
                "<=>" => {
                    // Interface-to-interface connection
                    info!("Processing interface-to-interface connection: {} <=> {}", left_text, right_text);
                    process_interface_to_interface_connection(&left_text, &right_text, netlist, context)?;
                }
                "->" | "<->" => {
                    // Regular pin-to-pin or net connections
                    // Component instances are already created by the synthesizer,
                    // so we just need to process the connections
                    
                    // Now process the actual connections
                    // Parse the full connection text to handle chains like A -> B -> C
                    let conn_text = conn_stmt.syntax().text().to_string();
                    let parts = parse_connection_chain(&conn_text);
                    
                    debug!("Processing connection with {} parts", parts.len());
                    
                    // Track the current net for this connection chain
                    let mut current_net_id: Option<NetId> = None;

                    // FIRST PASS: Scan all endpoints to find existing net references
                    // This prevents creating new nets when we should connect to existing ones
                    for part in &parts {
                        let endpoint = part.trim().trim_end_matches(';');

                        // Check for @ prefixed net reference
                        if endpoint.starts_with('@') {
                            let net_name = &endpoint[1..];
                            println!("DEBUG: First pass found net reference @{}", net_name);
                            let net_id = context.resolve_net(net_name, netlist)?;
                            current_net_id = Some(net_id);
                            break; // Found a net, stop scanning
                        }

                        // Check for simple identifiers that might be power/ground nets
                        // These are endpoints without dots (not pin references) and without @ prefix
                        if !endpoint.contains('.') && !endpoint.contains(':') {
                            // This might be a simple net name like GND, VIN, VOUT
                            // Try to resolve it - if it exists, use it
                            if let Ok(net_id) = context.resolve_net(endpoint, netlist) {
                                println!("DEBUG: First pass found existing net '{}'", endpoint);
                                current_net_id = Some(net_id);
                                break; // Found a net, stop scanning
                            }
                        }
                    }
                    
                    // Process each endpoint
                    for (i, part) in parts.iter().enumerate() {
                        let endpoint = part.trim().trim_end_matches(';');
                        println!("DEBUG: Processing connection endpoint {}: '{}'", i, endpoint);
                        
                        // Parse the endpoint to determine its type
                        if endpoint.starts_with('@') {
                            // Net reference like @VCC or @GND
                            let net_name = &endpoint[1..];
                            
                            // Skip if we already processed this net reference
                            if parts.len() == 2 && i == 1 && current_net_id.is_some() {
                                println!("DEBUG: Skipping already processed net reference @{}", net_name);
                                continue;
                            }
                            
                            println!("DEBUG: Processing net reference @{}", net_name);
                            let net_id = context.resolve_net(net_name, netlist)?;
                            current_net_id = Some(net_id);
                            println!("DEBUG: Net reference: {} -> {:?}", net_name, net_id);
                        } else if let Some(dot_pos) = endpoint.rfind('.') {
                            // Component pin reference like R1.2 or LED1.cathode
                            // Also handles inline assignment like R1: Res(330).1
                            let before_dot = &endpoint[..dot_pos];
                            let pin_name = &endpoint[dot_pos + 1..];
                            
                            // Extract the instance handle from inline assignment syntax
                            let inst_name = if let Some(colon_pos) = before_dot.find(':') {
                                // Inline assignment: extract handle before colon
                                before_dot[..colon_pos].trim()
                            } else {
                                // Simple reference: use the whole string
                                before_dot
                            };
                            
                            // Find the instance. Use the context-aware
                            // lookup so a bare `S1` reference inside an
                            // entity body resolves to the entity-scoped
                            // `ATMEGA328P_PU.S1` instance when the
                            // bare-name one doesn't exist. See
                            // `find_instance_by_name_in_context` docs.
                            if let Some(inst_id) = find_instance_by_name_in_context(netlist, context, inst_name) {
                                // Find the pin on this instance
                                if let Some(pin_inst_id) = netlist.find_pin_instance(inst_id, pin_name) {
                                    // Connect this pin to the current net
                                    let net_id = current_net_id.get_or_insert_with(|| {
                                        // Create an unnamed net for this connection
                                        println!("DEBUG: Creating unnamed net for connection");
                                        netlist.add_net(None)
                                    });
                                    
                                    if let Err(e) = netlist.connect(*net_id, ConnectionPoint::PinInstance(pin_inst_id)) {
                                        warn!("Failed to connect {}.{}: {}", inst_name, pin_name, e);
                                    } else {
                                        println!("DEBUG: Connected {}.{} to net {:?}", inst_name, pin_name, net_id);
                                    }
                                } else {
                                    warn!("    Pin {} not found on instance {}", pin_name, inst_name);
                                }
                            } else {
                                warn!("    Instance {} not found", inst_name);
                            }
                        } else {
                            // Simple identifier - could be a net name
                            let net_id = context.resolve_net(endpoint, netlist)?;
                            current_net_id = Some(net_id);
                            debug!("    Net name: {} -> {:?}", endpoint, net_id);
                        }
                    }
                    
                    debug!("Processed connection: {} {} {}", left_text, operator, right_text);
                }
                _ => {
                    warn!("Unknown connection operator: {}", operator);
                }
            }
        }
    }
    
    Ok(())
}

/// Extract component instances from a connection statement
fn extract_component_instances_from_connection(
    conn_stmt: &ConnectionStmt,
    netlist: &mut Netlist,
    context: &mut HierarchicalContext,
    analysis: &AnalysisResult,
    import_preprocessor: Option<&crate::import_preprocessor::ImportPreprocessor>,
) -> Result<()> {
    // Extract the connection expression
    if let Some(expr_node) = conn_stmt.expr() {
        // Walk the expression tree to find component instantiations
        extract_components_from_node(&expr_node, netlist, context, analysis, import_preprocessor)?;
    }
    
    Ok(())
}

/// Recursively extract component instances from an AST node
fn extract_components_from_node(
    node: &SyntaxNode<BhdlLanguage>,
    netlist: &mut Netlist,
    context: &mut HierarchicalContext,
    analysis: &AnalysisResult,
    import_preprocessor: Option<&crate::import_preprocessor::ImportPreprocessor>,
) -> Result<()> {
    use bhdl_ast::SyntaxKind;
    
    // Check if this node is a component instantiation
    if node.kind() == SyntaxKind::COMPONENT_INST {
        if let Some(comp_inst) = bhdl_ast::common::ComponentInst::cast(node.clone()) {
            create_component_instance(&comp_inst, netlist, context, analysis, import_preprocessor)?;
        }
    }
    
    // Recursively process children
    for child in node.children() {
        extract_components_from_node(&child, netlist, context, analysis, import_preprocessor)?;
    }
    
    Ok(())
}

/// Create a component instance in the current module context
fn create_component_instance(
    comp_inst: &bhdl_ast::common::ComponentInst,
    netlist: &mut Netlist,
    context: &mut HierarchicalContext,
    analysis: &AnalysisResult,
    import_preprocessor: Option<&crate::import_preprocessor::ImportPreprocessor>,
) -> Result<()> {
    use bhdl_netlist::ModuleKind;
    
    // Get component type
    let component_type = comp_inst.component_type_name()
        .map(|t| t.text().to_string())
        .ok_or_else(|| anyhow::anyhow!("Component instance missing type"))?;

    // If the user wrote `<name>: <Type>(...)`, prefer that name. The instance
    // was (or will be) created by the inference-driven path under this name;
    // generating a fresh `U<n>` here would duplicate it as a disconnected
    // child in the netlist.
    let user_supplied_name = extract_user_instance_name(comp_inst);

    let local_name = if let Some(name) = user_supplied_name {
        name
    } else {
        // Anonymous (flow) syntax: generate a hierarchical refdes.
        let base_refdes = get_component_refdes_prefix(&component_type);
        let counter_key = if context.current_path.is_empty() {
            base_refdes.clone()
        } else {
            format!("{}.{}", context.current_path_string(), base_refdes)
        };
        let counter = context.component_counters
            .entry(counter_key)
            .or_insert(0);
        *counter += 1;
        format!("{}{}", base_refdes, counter)
    };
    // For hierarchical names, skip the board prefix if it's just "TestBoard" or similar
    let instance_name = if context.current_path.is_empty() {
        local_name.clone()
    } else if context.current_path.len() == 1 && context.current_path[0].ends_with("Board") {
        // Skip board prefix for cleaner names
        local_name.clone()
    } else {
        // For deeper hierarchies, include the path but skip the board
        let path_without_board = if context.current_path.len() > 1 && context.current_path[0].ends_with("Board") {
            context.current_path[1..].join(".")
        } else {
            context.current_path_string()
        };
        format!("{}.{}", path_without_board, local_name)
    };
    
    // Check if an instance with the same name already exists.
    //
    // We accept TWO match shapes:
    //   - Full hierarchical match (`instance_name` ==
    //     `ATMEGA328P_PU.S1`): another path inside this same
    //     hierarchical context already created it.
    //   - Bare-name match (`local_name` == existing instance name
    //     == `S1` at board level): the earlier
    //     `generate_database_component_instances` pass walked
    //     analyzer-registered Instance symbols and created `S1`
    //     unprefixed. KiCad's annotation rule guarantees refdes
    //     uniqueness across the entire board (`S1` only ever
    //     appears once across all sheets), so a bare-name
    //     collision genuinely refers to the SAME conceptual
    //     component — we should re-use it, not duplicate.
    //
    // The bare-name branch fixes the non-deterministic
    // `find_instance_by_name` behavior in the Arduino UNO
    // round-trip: before this fix, both `S1` and
    // `ATMEGA328P_PU.S1` existed, and bare-name lookup hit one
    // or the other depending on SlotMap iteration. After this
    // fix, only `S1` exists; lookup is deterministic.
    for (_inst_id, instance) in &netlist.instances {
        if instance.name == instance_name || instance.name == local_name {
            debug!("Component instance '{}' already exists (matched on {}), skipping creation",
                instance_name,
                if instance.name == instance_name { "full path" } else { "bare name" });
            return Ok(());
        }
    }
    
    // Create or get the component module
    let module_id = get_or_create_component_module(&component_type, netlist, context, analysis, import_preprocessor)?;

    // Create the instance
    let instance_id = netlist.add_instance(instance_name.clone(), module_id)
        .ok_or_else(|| anyhow::anyhow!("Failed to add component instance"))?;

    // Propagate component_class from module definition to instance
    if let Some(module) = netlist.modules.get(module_id) {
        let module_attrs = module.attributes.clone();
        if let Some(instance) = netlist.instances.get_mut(instance_id) {
            for (key, value) in &module_attrs {
                if !instance.attributes.contains_key(key) {
                    instance.attributes.insert(key.clone(), value.clone());
                }
            }
        }
    }

    // Create pin instances
    netlist.create_pin_instances(instance_id)
        .map_err(|e| anyhow::anyhow!("Failed to create pin instances: {}", e))?;

    // Transfer component parameters from analyzer to instance
    populate_instance_attributes(netlist, instance_id, &instance_name, analysis);

    debug!("Created component instance: {} of type {}", instance_name, component_type);

    Ok(())
}

/// Get or create a module for a component type
fn get_or_create_component_module(
    component_type: &str,
    netlist: &mut Netlist,
    context: &HierarchicalContext,
    analysis: &AnalysisResult,
    import_preprocessor: Option<&crate::import_preprocessor::ImportPreprocessor>,
) -> Result<ModuleId> {
    use bhdl_netlist::ModuleKind;

    debug!("get_or_create_component_module called for: {}", component_type);

    // Check if module already exists
    for (module_id, module) in &netlist.modules {
        if module.name == component_type && module.kind == ModuleKind::PhysicalComponent {
            debug!("Found existing module for {}: {:?}", component_type, module_id);
            return Ok(module_id);
        }
    }

    // Create new component module
    debug!("Creating new module for: {}", component_type);
    let module_id = netlist.add_module(component_type.to_string(), ModuleKind::PhysicalComponent);

    // Determine the actual entity to look up for attributes and pins.
    // If this component_type is an alias specialization, look up the original generic entity.
    let original_entity_name = analysis.monomorphization.alias_specializations.iter()
        .find(|a| a.alias_name == component_type)
        .map(|a| a.target_entity.clone());

    // Extract entity attributes (including component_class) from AST definition
    // and store them on the ModuleDefinition.
    // Try original generic entity name first, then fall back to alias name.
    let lookup_name = original_entity_name.as_deref().unwrap_or(component_type);
    let entity_ast = context.variant_manager.find_entity_definition(lookup_name)
        .cloned()
        .or_else(|| import_preprocessor.and_then(|pp| pp.get_imported_entity(lookup_name).cloned()))
        .or_else(|| {
            // Fall back to looking up by alias name (import_loader stores under alias name)
            if lookup_name != component_type {
                context.variant_manager.find_entity_definition(component_type)
                    .cloned()
                    .or_else(|| import_preprocessor.and_then(|pp| pp.get_imported_entity(component_type).cloned()))
            } else {
                None
            }
        });
    if let Some(ref entity) = entity_ast {
        let mut entity_attrs = bhdl_analyzer::attribute_extraction::extract_module_attributes(entity);

        // If this is an alias specialization, substitute generic param references
        // in attribute values with concrete values (e.g., "V_OUT" → "5")
        if let Some(alias_spec) = analysis.monomorphization.alias_specializations.iter()
            .find(|a| a.alias_name == component_type)
        {
            if !alias_spec.concrete_params.is_empty() {
                bhdl_analyzer::attribute_extraction::substitute_generic_params(
                    &mut entity_attrs,
                    &alias_spec.concrete_params,
                );
                debug!("Substituted generic params for alias '{}': {:?}",
                       component_type, entity_attrs);
            }
        }

        if !entity_attrs.is_empty() {
            if let Some(module) = netlist.modules.get_mut(module_id) {
                module.attributes = entity_attrs;
                debug!("Propagated {} attributes from entity '{}' to module definition",
                       module.attributes.len(), component_type);
            }
        }
    }

    // Look up pin specialization info for this component type (excluded pins, bus sizes)
    let specialized_module = analysis.monomorphization.alias_specializations.iter()
        .find(|a| a.alias_name == component_type)
        .and_then(|alias| {
            let key = bhdl_analyzer::passes::monomorphization::SpecializationKey {
                module_name: alias.target_entity.clone(),
                params: alias.concrete_params.iter()
                    .map(|(k, v)| (k.clone(), bhdl_analyzer::passes::monomorphization::ConstValueKey::from_const_value(v)))
                    .collect(),
            };
            analysis.monomorphization.specializations.get(&key)
        })
        .or_else(|| analysis.monomorphization.get_by_mangled_name(component_type));

    // Add standard pins based on component type.
    // Determine the best name for pin lookup: try original entity name first,
    // then alias name. The import preprocessor stores entities under the
    // alias name (e.g., "LM7805"), not the generic entity name ("LinearRegulator").
    let pin_lookup_name = if lookup_name != component_type {
        // Check if the original entity name is available in the preprocessor
        let found_original = import_preprocessor
            .and_then(|pp| pp.get_imported_entity(lookup_name))
            .is_some()
            || context.variant_manager.find_entity_definition(lookup_name).is_some();
        if found_original {
            lookup_name
        } else {
            component_type
        }
    } else {
        lookup_name
    };
    add_component_pins(pin_lookup_name, module_id, netlist, context, import_preprocessor, specialized_module)?;

    Ok(module_id)
}

/// Add pins to a component module based on its type.
/// If `specialized_module` is provided, applies pin specialization:
/// - Excludes pins whose `when` condition evaluated to false
/// - Expands parameterized bus pins to concrete sizes
fn add_component_pins(
    component_type: &str,
    module_id: ModuleId,
    netlist: &mut Netlist,
    context: &HierarchicalContext,
    import_preprocessor: Option<&crate::import_preprocessor::ImportPreprocessor>,
    specialized_module: Option<&bhdl_analyzer::passes::monomorphization::SpecializedModule>,
) -> Result<()> {
    use bhdl_netlist::{PinDirection, PinType, PortDirection};

    debug!("add_component_pins called for component_type: {}", component_type);
    debug!("import_preprocessor is_some: {}", import_preprocessor.is_some());

    // Helper closure: add a single pin (with bus expansion if needed) to the netlist
    let add_pin_to_netlist = |pin: &bhdl_ast::common::PinDecl,
                              module_id: ModuleId,
                              netlist: &mut Netlist,
                              specialized: Option<&bhdl_analyzer::passes::monomorphization::SpecializedModule>| {
        let pin_name = match pin.name() {
            Some(n) => n,
            None => return,
        };
        let pin_name_str = pin_name.text().to_string();

        // Check if this pin is excluded by a `when` condition
        if let Some(spec) = specialized {
            if spec.excluded_pins.contains(&pin_name_str) {
                debug!("Skipping excluded pin '{}' for specialized component", pin_name_str);
                return;
            }
        }

        // Convert pin direction from AST to netlist types
        let direction_str = pin.direction().map(|t| t.text().to_string());
        let (pin_direction, port_direction) = match direction_str.as_deref() {
            Some("in") => (PinDirection::In, PortDirection::Input),
            Some("out") => (PinDirection::Out, PortDirection::Output),
            Some("inout") => (PinDirection::InOut, PortDirection::InOut),
            _ => (PinDirection::InOut, PortDirection::InOut),
        };

        // Convert pin type from AST to netlist types
        let pin_type_str = pin.pin_type().map(|t| t.text().to_string());
        let pin_type = match pin_type_str.as_deref() {
            Some("power") => PinType::Power,
            Some("ground") => PinType::Ground,
            Some("signal") => PinType::Signal,
            _ => PinType::Signal,
        };

        // Check if this pin has a resolved bus size from generics
        if let Some(spec) = specialized {
            if let Some(&bus_size) = spec.resolved_bus_sizes.get(&pin_name_str) {
                // Expand parameterized bus pin into indexed pins
                for i in 0..bus_size {
                    let indexed_name = format!("{}[{}]", pin_name_str, i);
                    debug!("Adding expanded bus pin '{}' for component '{}'", indexed_name, component_type);
                    netlist.add_port(module_id, indexed_name.clone(), port_direction, None);
                    netlist.add_pin(module_id, indexed_name, pin_direction, pin_type);
                }
                return;
            }
        }

        debug!("Adding pin '{}' for component '{}'", pin_name_str, component_type);
        netlist.add_port(module_id, pin_name_str.clone(), port_direction, None);
        let pin_id = netlist.add_pin(module_id, pin_name_str, pin_direction, pin_type);
        // Propagate virtual flag from AST pin declaration
        if pin.is_virtual() {
            if let Some(pid) = pin_id {
                if let Some(p) = netlist.pins.get_mut(pid) {
                    p.is_virtual = true;
                }
            }
        }
    };

    // First check if this component is in the variant_manager (same-file entities)
    if let Some(entity_ast) = context.variant_manager.find_entity_definition(component_type) {
        debug!("Adding pins for locally-defined component: {}", component_type);

        let pins: Vec<_> = entity_ast.pins().collect();
        debug!("Total pins found in {}: {}", component_type, pins.len());
        for pin in &pins {
            add_pin_to_netlist(pin, module_id, netlist, specialized_module);
        }
        return Ok(());
    }

    // Next check if this component is in the imported modules
    if let Some(preprocessor) = import_preprocessor {
        debug!("Checking preprocessor for component: {}", component_type);
        if let Some(entity_ast) = preprocessor.get_imported_entity(component_type) {
            debug!("Adding pins for imported component: {}", component_type);

            let pins: Vec<_> = entity_ast.pins().collect();
            debug!("Total pins found in {}: {}", component_type, pins.len());
            for pin in &pins {
                add_pin_to_netlist(pin, module_id, netlist, specialized_module);
            }
            return Ok(());
        }
    }
    
    // Fallback to hardcoded patterns for standard components
    match component_type.to_lowercase().as_str() {
        "res" | "resistor" => {
            // Add two passive pins
            netlist.add_port(module_id, "1".to_string(), PortDirection::InOut, None);
            netlist.add_port(module_id, "2".to_string(), PortDirection::InOut, None);
            netlist.add_pin(module_id, "1".to_string(), PinDirection::Passive, PinType::Passive);
            netlist.add_pin(module_id, "2".to_string(), PinDirection::Passive, PinType::Passive);
        }
        "cap" | "capacitor" => {
            // Add two passive pins
            netlist.add_port(module_id, "1".to_string(), PortDirection::InOut, None);
            netlist.add_port(module_id, "2".to_string(), PortDirection::InOut, None);
            netlist.add_pin(module_id, "1".to_string(), PinDirection::Passive, PinType::Passive);
            netlist.add_pin(module_id, "2".to_string(), PinDirection::Passive, PinType::Passive);
        }
        "led" => {
            // Add anode and cathode
            netlist.add_port(module_id, "A".to_string(), PortDirection::InOut, None);
            netlist.add_port(module_id, "K".to_string(), PortDirection::InOut, None);
            netlist.add_pin(module_id, "A".to_string(), PinDirection::In, PinType::Signal);
            netlist.add_pin(module_id, "K".to_string(), PinDirection::Out, PinType::Signal);
        }
        _ => {
            debug!("Using default pins for unknown component type: {}", component_type);
            // Default: add two generic pins
            netlist.add_port(module_id, "1".to_string(), PortDirection::InOut, None);
            netlist.add_port(module_id, "2".to_string(), PortDirection::InOut, None);
            netlist.add_pin(module_id, "1".to_string(), PinDirection::InOut, PinType::Signal);
            netlist.add_pin(module_id, "2".to_string(), PinDirection::InOut, PinType::Signal);
        }
    }
    
    Ok(())
}

/// Get the binary operator from a binary expression
fn get_binary_operator(binary_expr: &BinaryExpr) -> String {
    // Look for the operator token in the binary expression
    for token in binary_expr.syntax().children_with_tokens() {
        if let Some(token) = token.as_token() {
            match token.kind() {
                SyntaxKind::ARROW => return "->".to_string(),
                SyntaxKind::BI_ARROW => return "<->".to_string(),
                SyntaxKind::INTERFACE_OP => return "<=>".to_string(),
                SyntaxKind::LEFT_ARROW => return "<-".to_string(),
                _ => continue,
            }
        }
    }
    "unknown".to_string()
}

/// Process interface-to-interface connection (A <=> B)
/// This merges the signal nets from both interfaces
fn process_interface_to_interface_connection(
    left_interface: &str,
    right_interface: &str, 
    netlist: &mut Netlist,
    context: &mut HierarchicalContext,
) -> Result<()> {
    info!("Merging interface signals between {} and {}", left_interface, right_interface);
    
    // Find interface signal nets for both interfaces
    let mut left_nets = Vec::new();
    let mut right_nets = Vec::new();
    
    for (net_id, net) in netlist.nets.iter() {
        if let Some(net_name) = &net.name {
            // Check if this net belongs to the left interface
            if net_name.starts_with("U") && net_name.contains("_") {
                // Extract signal name (everything after _)
                if let Some(underscore_pos) = net_name.rfind('_') {
                    let signal_name = &net_name[underscore_pos + 1..];
                    let instance_prefix = &net_name[..underscore_pos];
                    
                    // We need to map the original interface names to the generated names
                    // For now, collect all interface nets and match by signal name
                    left_nets.push((net_id, signal_name.to_string(), net_name.clone()));
                }
            }
        }
    }
    
    // Copy the vector to avoid borrow issues
    right_nets = left_nets.clone();
    
    // Group nets by signal name
    let mut signal_groups: std::collections::HashMap<String, Vec<(NetId, String)>> = std::collections::HashMap::new();
    
    for (net_id, signal_name, net_name) in &left_nets {
        signal_groups.entry(signal_name.clone())
            .or_insert_with(Vec::new)
            .push((*net_id, net_name.clone()));
    }
    
    // For each signal, merge the nets if there are multiple
    for (signal_name, nets) in signal_groups {
        if nets.len() > 1 {
            info!("Merging {} nets for signal {}", nets.len(), signal_name);
            
            // Use the first net as the target and merge others into it
            let target_net_id = nets[0].0;
            
            for i in 1..nets.len() {
                let source_net_id = nets[i].0;
                merge_nets(target_net_id, source_net_id, netlist)?;
            }
        }
    }
    
    Ok(())
}

/// Merge two nets - move all connections from source to target and remove source
fn merge_nets(target_net_id: NetId, source_net_id: NetId, netlist: &mut Netlist) -> Result<()> {
    // Get the connections from the source net
    let source_connections = if let Some(source_net) = netlist.nets.get(source_net_id) {
        source_net.connections.clone()
    } else {
        return Ok(());
    };
    
    // Move connections to target net
    if let Some(target_net) = netlist.nets.get_mut(target_net_id) {
        for connection in source_connections {
            if !target_net.connections.contains(&connection) {
                target_net.connections.push(connection);
            }
        }
    }
    
    // Remove the source net
    netlist.nets.remove(source_net_id);
    
    Ok(())
}

/// Return the user-supplied instance name from `<name>: <Type>(...)` syntax,
/// or `None` for anonymous flow syntax like `Res(330).1`.
fn extract_user_instance_name(
    comp_inst: &bhdl_ast::common::ComponentInst,
) -> Option<String> {
    use bhdl_ast::SyntaxKind;
    let mut first_ident: Option<String> = None;
    for element in comp_inst.syntax().children_with_tokens() {
        let Some(token) = element.into_token() else { continue };
        match token.kind() {
            SyntaxKind::IDENT => {
                if first_ident.is_none() {
                    first_ident = Some(token.text().to_string());
                }
            }
            SyntaxKind::COLON => {
                // Colon follows the first IDENT → that IDENT is the user's name.
                return first_ident;
            }
            _ => {}
        }
    }
    None
}

/// Get reference designator prefix for a component type
fn get_component_refdes_prefix(component_type: &str) -> String {
    match component_type.to_lowercase().as_str() {
        "res" | "resistor" => "R",
        "cap" | "capacitor" => "C",
        "ind" | "inductor" => "L",
        "diode" => "D",
        "led" => "D",
        "transistor" | "bjt" | "fet" | "mosfet" => "Q",
        "ic" | "opamp" | "regulator" => "U",
        "connector" => "J",
        "switch" => "SW",
        "crystal" | "xtal" => "Y",
        "transformer" => "T",
        "fuse" => "F",
        _ => "U",
    }.to_string()
}

/// Generate SPICE subcircuits for hierarchical modules
pub fn generate_spice_subcircuits(
    netlist: &Netlist,
    _analysis: &AnalysisResult,
) -> Result<String> {
    let mut spice_output = String::new();
    
    // Generate subcircuit for each module
    for (_module_id, module_def) in &netlist.modules {
        if matches!(module_def.kind, bhdl_netlist::types::ModuleKind::Module) {
            spice_output.push_str(&format!("\n* Subcircuit: {}\n", module_def.name));
            spice_output.push_str(&format!(".SUBCKT {}", module_def.name));
            
            // Add pins by looking up port names from the ports slotmap
            for &port_id in &module_def.ports {
                if let Some(port) = netlist.ports.get(port_id) {
                    spice_output.push_str(&format!(" {}", port.name));
                }
            }
            spice_output.push_str("\n");
            
            // TODO: Add internal components and connections
            
            spice_output.push_str(&format!(".ENDS {}\n", module_def.name));
        }
    }
    
    Ok(spice_output)
}

/// Process a net flow statement
fn process_net_flow_statement(
    node: &SyntaxNode<BhdlLanguage>,
    netlist: &mut Netlist,
    context: &mut HierarchicalContext,
    analysis: &AnalysisResult,
    import_preprocessor: Option<&crate::import_preprocessor::ImportPreprocessor>,
) -> Result<()> {
    println!("=== Processing NET_FLOW_STMT ===");
    info!("Processing NET_FLOW_STMT");
    println!("Full NET_FLOW_STMT text: '{}'", node.text());
    info!("Full NET_FLOW_STMT text: '{}'", node.text());
    
    // Extract the net name and flow expression
    let mut net_name: Option<String> = None;
    let mut flow_text: Option<String> = None;
    
    // Debug: Show all children
    println!("NET_FLOW_STMT children:");
    info!("NET_FLOW_STMT children:");
    for (i, child) in node.children_with_tokens().enumerate() {
        if let Some(token) = child.as_token() {
            println!("  Child {} (token): kind={:?}, text='{}'", i, token.kind(), token.text());
            info!("  Child {} (token): kind={:?}, text='{}'", i, token.kind(), token.text());
        } else if let Some(node) = child.as_node() {
            println!("  Child {} (node): kind={:?}, text='{}'", i, node.kind(), node.text());
            info!("  Child {} (node): kind={:?}, text='{}'", i, node.kind(), node.text());
        }
    }
    
    // Find the net name (before the colon)
    for child in node.children_with_tokens() {
        if let Some(token) = child.as_token() {
            match token.kind() {
                SyntaxKind::NET_KW => {
                    // Skip the 'net' keyword
                    continue;
                }
                SyntaxKind::IDENT => {
                    if net_name.is_none() {
                        net_name = Some(token.text().to_string());
                        info!("Found net name: {}", token.text());
                    }
                }
                _ => {}
            }
        }
    }
    
    // Find the flow expression (after the colon)
    let full_text = node.text().to_string();
    if let Some(colon_pos) = full_text.find(':') {
        let after_colon = &full_text[colon_pos + 1..];
        // Remove trailing semicolon and 'for' clause if present
        let flow_end = after_colon.find(" for ").unwrap_or_else(|| {
            after_colon.find(';').unwrap_or(after_colon.len())
        });
        flow_text = Some(after_colon[..flow_end].trim().to_string());
        println!("Found flow expression: {}", flow_text.as_ref().unwrap());
        info!("Found flow expression: {}", flow_text.as_ref().unwrap());
    }
    
    if let (Some(net_name), Some(flow_text)) = (net_name, flow_text) {
        // Parse the flow expression into parts
        let parts = parse_connection_chain(&flow_text);
        println!("Parsed {} parts from flow expression", parts.len());
        info!("Parsed {} parts from flow expression", parts.len());

        // Create or resolve the named net from the NET_FLOW_STMT declaration
        let declared_net_id = context.resolve_net(&net_name, netlist)?;
        println!("Resolved declared net '{}' to {:?}", net_name, declared_net_id);
        info!("Resolved declared net '{}' to {:?}", net_name, declared_net_id);

        // Process the flow chain starting from the declared net
        process_flow_parts(&parts, Some(declared_net_id), netlist, context, analysis, import_preprocessor)?;

    } else {
        warn!("Failed to extract net name or flow expression from NET_FLOW_STMT");
    }

    Ok(())
}

/// Process a bare CONNECTION_STMT (no `net` keyword) as a flow statement.
/// e.g., `VCC -> r1: Res(330).1 -> r1.2 -> led1: LED("red").A;`
fn process_connection_stmt_as_flow(
    node: &SyntaxNode<BhdlLanguage>,
    netlist: &mut Netlist,
    context: &mut HierarchicalContext,
    analysis: &AnalysisResult,
    import_preprocessor: Option<&crate::import_preprocessor::ImportPreprocessor>,
) -> Result<()> {
    let full_text = node.text().to_string();
    println!("=== Processing CONNECTION_STMT as flow ===");
    info!("Processing CONNECTION_STMT as flow: '{}'", full_text);

    // Strip trailing semicolon and any `for` clause
    let text = full_text.trim();
    let text = text.strip_suffix(';').unwrap_or(text);
    let flow_end = text.find(" for ").unwrap_or(text.len());
    let flow_text = text[..flow_end].trim();

    if flow_text.is_empty() {
        return Ok(());
    }

    let parts = parse_connection_chain(flow_text);
    println!("Parsed {} parts from bare connection", parts.len());
    info!("Parsed {} parts from bare connection", parts.len());

    // Process with no initial net — the first element in the chain will establish it
    process_flow_parts(&parts, None, netlist, context, analysis, import_preprocessor)?;

    Ok(())
}

/// Shared flow processing logic used by both NET_FLOW_STMT and CONNECTION_STMT handlers.
///
/// Walks through flow parts left-to-right, creating inline instances and connecting pins.
/// The key invariant: after processing each part, `last_net_id` holds the net that the
/// NEXT element should connect to. For inline instantiations (`r1: Res(330).1`), we connect
/// the specified pin to `last_net_id` and then set `last_net_id = None` — the next part
/// (typically a pin-ref like `r1.2`) will create a fresh intermediate net.
fn process_flow_parts(
    parts: &[String],
    initial_net_id: Option<NetId>,
    netlist: &mut Netlist,
    context: &mut HierarchicalContext,
    analysis: &AnalysisResult,
    import_preprocessor: Option<&crate::import_preprocessor::ImportPreprocessor>,
) -> Result<()> {
    let mut last_net_id: Option<NetId> = initial_net_id;
    let mut last_was_component_pin = false;

    for (i, part) in parts.iter().enumerate() {
        let endpoint = part.trim();
        println!("\nProcessing flow part {}: '{}'", i, endpoint);
        info!("Processing flow part {}: '{}'", i, endpoint);

        if endpoint.starts_with('@') {
            // Net reference like @VCC or @GND
            let ref_net_name = &endpoint[1..];
            let ref_net_id = context.resolve_net(ref_net_name, netlist)?;
            println!("  Net reference: @{} (id: {:?})", ref_net_name, ref_net_id);
            info!("  Net reference: @{} (id: {:?})", ref_net_name, ref_net_id);

            // If the previous part left a component pin dangling (last_net_id == None,
            // last_was_component_pin == true), connect that pin to this net.
            if last_was_component_pin && last_net_id.is_none() && i > 0 {
                connect_previous_pin_to_net(parts, i, netlist, context, ref_net_id)?;
            }

            // Merge if there's an existing intermediate net
            if let Some(prev_net_id) = last_net_id {
                if prev_net_id != ref_net_id {
                    merge_nets(ref_net_id, prev_net_id, netlist)?;
                }
            }

            last_net_id = Some(ref_net_id);
            last_was_component_pin = false;

        } else if endpoint.contains(':') && endpoint.contains('(') {
            // Inline component instantiation like "r1: Res(330).1"
            if let Some(colon_pos) = endpoint.find(':') {
                let instance_name = endpoint[..colon_pos].trim();
                let after_colon = &endpoint[colon_pos + 1..].trim();

                if let Some(paren_pos) = after_colon.find('(') {
                    let component_type = after_colon[..paren_pos].trim();

                    let pin_name = if let Some(dot_pos) = endpoint.rfind('.') {
                        endpoint[dot_pos + 1..].trim()
                    } else {
                        "1"
                    };

                    println!("  Inline component: {} = {}(...).{}", instance_name, component_type, pin_name);
                    info!("  Inline component: {} = {}(...).{}", instance_name, component_type, pin_name);

                    let inst_id = create_inline_component_instance(
                        instance_name,
                        component_type,
                        endpoint,
                        netlist,
                        context,
                        analysis,
                        import_preprocessor
                    )?;

                    println!("  Created instance {:?} for {}", inst_id, instance_name);

                    if let Some(net_id) = last_net_id {
                        connect_pin_to_net(netlist, inst_id, pin_name, net_id,
                            &format!("{}.{}", instance_name, pin_name), "previous net")?;
                    } else {
                        println!("  Warning: No previous net to connect to for {}.{}", instance_name, pin_name);
                    }

                    // After an inline instantiation the specified pin is connected;
                    // the *other* pin (e.g. r1.2) will be referenced explicitly later
                    // and will need a new intermediate net.
                    last_was_component_pin = true;
                    last_net_id = None;
                }
            }
        } else if let Some(dot_pos) = endpoint.find('.') {
            // Component pin reference like "r1.2" or "led1.K"
            let instance_name = &endpoint[..dot_pos];
            let pin_name = &endpoint[dot_pos + 1..];

            println!("  Component pin: {}.{}", instance_name, pin_name);
            info!("  Component pin: {}.{}", instance_name, pin_name);

            // Context-aware: prefer module-qualified instance when
            // we're inside a sub-module's body.
            if let Some(inst_id) = find_instance_by_name_in_context(netlist, context, instance_name) {
                let net_id = if let Some(existing_net_id) = last_net_id {
                    // Use the existing net from the previous element
                    existing_net_id
                } else if last_was_component_pin {
                    // Previous was an inline instantiation (last_net_id = None).
                    // Create a fresh intermediate net for this new pin.
                    // Do NOT re-connect the previous pin — it was already connected
                    // to the correct net during its inline instantiation step.
                    let net_name = format!("net_{}_{}",
                        if i > 0 { parts[i-1].trim() } else { "start" },
                        endpoint
                    ).replace(".", "_").replace(":", "_");
                    let new_net_id = netlist.add_net(Some(net_name.clone()));
                    println!("  Created intermediate net '{}' ({:?})", net_name, new_net_id);
                    new_net_id
                } else {
                    // First element in the chain is a pin reference (e.g. `led1.K -> GND`).
                    // Create a temporary auto-net; it will be merged with the real net
                    // when the next element (e.g. GND) is resolved.
                    let net_name = format!("auto_{}", endpoint).replace(".", "_");
                    let new_net_id = netlist.add_net(Some(net_name.clone()));
                    println!("  Created auto-net '{}' ({:?}) for first-element pin", net_name, new_net_id);
                    new_net_id
                };

                connect_pin_to_net(netlist, inst_id, pin_name, net_id,
                    &format!("{}.{}", instance_name, pin_name), "net")?;

                last_net_id = Some(net_id);
                last_was_component_pin = true;
            } else {
                println!("  Instance {} not found", instance_name);
                warn!("  Instance {} not found", instance_name);
            }
        } else {
            // Simple identifier — power/ground net name like VCC, GND (without @ prefix)
            println!("  Simple identifier: '{}'", endpoint);
            info!("  Simple identifier: '{}'", endpoint);

            match context.resolve_net(endpoint, netlist) {
                Ok(resolved_net_id) => {
                    // If there's an existing intermediate/auto net, merge it into the resolved net
                    if let Some(prev_net_id) = last_net_id {
                        if prev_net_id != resolved_net_id {
                            println!("  Merging previous net {:?} with resolved net {:?}", prev_net_id, resolved_net_id);
                            info!("  Merging previous net {:?} with resolved net {:?}", prev_net_id, resolved_net_id);
                            merge_nets(resolved_net_id, prev_net_id, netlist)?;
                        }
                    }

                    // If previous was an inline instantiation with dangling pin, connect it
                    if last_was_component_pin && last_net_id.is_none() && i > 0 {
                        connect_previous_pin_to_net(parts, i, netlist, context, resolved_net_id)?;
                    }

                    last_net_id = Some(resolved_net_id);
                    last_was_component_pin = false;
                    println!("  Resolved '{}' to net {:?}", endpoint, resolved_net_id);
                    info!("  Resolved '{}' to net {:?}", endpoint, resolved_net_id);
                }
                Err(e) => {
                    println!("  Warning: Could not resolve '{}' as net: {}", endpoint, e);
                    warn!("  Could not resolve '{}' as net: {}", endpoint, e);
                }
            }
        }
    }

    Ok(())
}

/// Helper: look back at parts[i-1] and connect its pin to the given net.
/// Used when an inline instantiation leaves a dangling component pin.
///
/// Takes a `&HierarchicalContext` so the instance lookup is
/// module-scope-aware — `S1.1` from inside `ATMEGA328P_PU` body
/// resolves to `ATMEGA328P_PU.S1` when no bare `S1` exists.
fn connect_previous_pin_to_net(
    parts: &[String],
    i: usize,
    netlist: &mut Netlist,
    context: &HierarchicalContext,
    net_id: NetId,
) -> Result<()> {
    if i == 0 { return Ok(()); }
    let prev_part = parts[i-1].trim();
    if let Some(prev_dot) = prev_part.rfind('.') {
        let prev_inst_name = if prev_part.contains(':') {
            prev_part.split(':').next().unwrap_or("").trim()
        } else {
            &prev_part[..prev_dot]
        };
        let prev_pin = &prev_part[prev_dot + 1..];
        if let Some(prev_inst_id) = find_instance_by_name_in_context(netlist, context, prev_inst_name) {
            connect_pin_to_net(netlist, prev_inst_id, prev_pin, net_id,
                &format!("{}.{}", prev_inst_name, prev_pin), "resolved net")?;
        }
    }
    Ok(())
}

/// Helper: connect a pin to a net, trying alternative pin names if needed
fn connect_pin_to_net(
    netlist: &mut Netlist,
    inst_id: InstanceId,
    pin_name: &str,
    net_id: NetId,
    desc: &str,
    target_desc: &str,
) -> Result<()> {
    if let Some(pin_inst_id) = netlist.find_pin_instance(inst_id, pin_name) {
        netlist.connect(net_id, ConnectionPoint::PinInstance(pin_inst_id))
            .map_err(|e| anyhow::anyhow!("Failed to connect pin: {}", e))?;
        println!("  Connected {} to {}", desc, target_desc);
        info!("  Connected {} to {}", desc, target_desc);
    } else {
        let alt_pins = match pin_name {
            "cathode" | "K" => vec!["K", "2", "-"],
            "anode" | "A" => vec!["A", "1", "+"],
            _ => vec![]
        };
        let mut connected = false;
        for alt in alt_pins {
            if let Some(pin_inst_id) = netlist.find_pin_instance(inst_id, alt) {
                netlist.connect(net_id, ConnectionPoint::PinInstance(pin_inst_id))
                    .map_err(|e| anyhow::anyhow!("Failed to connect pin: {}", e))?;
                println!("  Connected {} to {} (using pin '{}')", desc, target_desc, alt);
                info!("  Connected {} to {} (using pin '{}')", desc, target_desc, alt);
                connected = true;
                break;
            }
        }
        if !connected {
            warn!("  Could not find pin '{}' on instance", pin_name);
        }
    }
    Ok(())
}

/// Create an inline component instance from a flow statement
fn create_inline_component_instance(
    instance_name: &str,
    component_type: &str,
    full_text: &str,
    netlist: &mut Netlist,
    context: &mut HierarchicalContext,
    analysis: &AnalysisResult,
    import_preprocessor: Option<&crate::import_preprocessor::ImportPreprocessor>,
) -> Result<InstanceId> {
    // Check if instance already exists
    if let Some(inst_id) = find_instance_by_name(netlist, instance_name) {
        debug!("Component instance '{}' already exists", instance_name);
        return Ok(inst_id);
    }
    
    // Create or get the component module
    let module_id = get_or_create_component_module(component_type, netlist, context, analysis, import_preprocessor)?;

    // Create the instance
    let instance_id = netlist.add_instance(instance_name.to_string(), module_id)
        .ok_or_else(|| anyhow::anyhow!("Failed to add component instance"))?;

    // Propagate module-level attributes (including component_class) to instance
    if let Some(module) = netlist.modules.get(module_id) {
        let module_attrs = module.attributes.clone();
        if let Some(instance) = netlist.instances.get_mut(instance_id) {
            for (key, value) in &module_attrs {
                if !instance.attributes.contains_key(key) {
                    instance.attributes.insert(key.clone(), value.clone());
                }
            }
        }
    }

    // Create pin instances
    netlist.create_pin_instances(instance_id)
        .map_err(|e| anyhow::anyhow!("Failed to create pin instances: {}", e))?;

    // Extract and apply component parameters
    if let Some(paren_start) = full_text.find('(') {
        if let Some(paren_end) = full_text.find(')') {
            let params_text = &full_text[paren_start + 1..paren_end];
            // For simple single-value components like Res(330), the value is the parameter
            if !params_text.contains('=') && !params_text.contains(',') {
                // Single value parameter
                if let Some(instance) = netlist.instances.get_mut(instance_id) {
                    instance.attributes.insert("value".to_string(), params_text.trim().to_string());
                }
            }
            // TODO: Parse more complex parameter lists
        }
    }
    
    // Transfer component parameters from analyzer if available
    populate_instance_attributes(netlist, instance_id, instance_name, analysis);
    
    info!("Created inline component instance: {} of type {}", instance_name, component_type);
    
    Ok(instance_id)
}

/// Find an instance by name. Naive first-match scan over the
/// instances SlotMap. Use this only when no module context is
/// available (rare in practice — most callers are inside a
/// processing function that has `&HierarchicalContext`).
fn find_instance_by_name(netlist: &Netlist, name: &str) -> Option<InstanceId> {
    for (inst_id, instance) in &netlist.instances {
        if instance.name == name {
            return Some(inst_id);
        }
    }
    None
}

/// Context-aware instance lookup. Prefers an instance whose name
/// matches the current module's path-prefixed form (e.g.
/// `ATMEGA328P_PU.S1` when called from inside ATMEGA328P_PU's
/// body looking for `S1`), falling back to the bare name if no
/// path-prefixed match exists.
///
/// Why: the synthesizer's two instance-creation paths produce
/// instances under different naming conventions. Path 1
/// (`generate_database_component_instances`, driven by the
/// analyzer's symbol table) creates instances under their bare
/// scope-local name (`S1`). Path 2 (`create_component_instance`
/// inside `process_entity_body`, AST-driven) creates instances
/// under hierarchical names (`ATMEGA328P_PU.S1`).
///
/// In Arduino UNO specifically, S1 (the reset switch) is the
/// ONLY sub-module instance that path 1 doesn't pre-create —
/// because its type (`kicad_passthrough`) has no Pass 6
/// inference suggestion. So at lookup time:
///   - Other components have BOTH bare-name (path 1) and
///     path-prefixed (path 2) instances. With the dedup fix in
///     `create_component_instance`, only the bare-name one
///     survives; bare-name lookup finds it.
///   - S1 has ONLY the path-prefixed instance
///     (`ATMEGA328P_PU.S1`). Bare-name lookup for `S1` returns
///     None. Connections to `S1.1`/`S1.2`/etc. silently fail
///     to wire.
///
/// This function tries the path-prefixed form first so the
/// S1-like case resolves. The naive `find_instance_by_name` is
/// kept for callers that genuinely don't have context.
fn find_instance_by_name_in_context(
    netlist: &Netlist,
    context: &HierarchicalContext,
    name: &str,
) -> Option<InstanceId> {
    // If we're inside a module, try `<path>.<name>` first.
    if !context.current_path.is_empty() {
        // Skip a board prefix the same way `create_component_instance`
        // does, so the lookup matches the actual stored instance
        // name shape.
        let path_without_board: String =
            if context.current_path.len() > 1
                && context.current_path[0].ends_with("Board")
            {
                context.current_path[1..].join(".")
            } else if context.current_path.len() == 1
                && context.current_path[0].ends_with("Board")
            {
                String::new()  // bare-name case
            } else {
                context.current_path.join(".")
            };
        if !path_without_board.is_empty() {
            let qualified = format!("{}.{}", path_without_board, name);
            if let Some(inst_id) = find_instance_by_name(netlist, &qualified) {
                return Some(inst_id);
            }
        }
    }
    // Fall back to bare-name lookup.
    find_instance_by_name(netlist, name)
}

/// Parse a connection chain string into individual parts
fn parse_connection_chain(conn_text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current_part = String::new();
    let mut in_parens = 0;
    let mut chars = conn_text.chars().peekable();
    
    while let Some(ch) = chars.next() {
        match ch {
            '(' => {
                in_parens += 1;
                current_part.push(ch);
            }
            ')' => {
                in_parens -= 1;
                current_part.push(ch);
            }
            '-' if in_parens == 0 => {
                // Check if this is part of an arrow
                if chars.peek() == Some(&'>') {
                    // End current part and skip the arrow
                    if !current_part.trim().is_empty() {
                        parts.push(current_part.trim().to_string());
                    }
                    current_part.clear();
                    chars.next(); // Skip '>'
                } else {
                    current_part.push(ch);
                }
            }
            '<' if in_parens == 0 => {
                // Check if this is part of a bidirectional arrow
                if chars.peek() == Some(&'-') {
                    if !current_part.trim().is_empty() {
                        parts.push(current_part.trim().to_string());
                    }
                    current_part.clear();
                    chars.next(); // Skip '-'
                    if chars.peek() == Some(&'>') {
                        chars.next(); // Skip '>'
                    }
                } else {
                    current_part.push(ch);
                }
            }
            _ => {
                current_part.push(ch);
            }
        }
    }
    
    // Add the last part
    if !current_part.trim().is_empty() {
        parts.push(current_part.trim().to_string());
    }
    
    parts
}