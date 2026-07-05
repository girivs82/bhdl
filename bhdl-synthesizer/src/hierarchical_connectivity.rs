//! Hierarchical connectivity extraction for BHDL synthesizer
//!
//! This module handles the extraction of connectivity information from
//! hierarchical entity designs, including:
//! - Entity definitions and their internal connections
//! - Entity instances and port mappings
//! - Hierarchical net resolution
//! - SPICE subcircuit generation

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use bhdl_ast::{AstNode, SyntaxKind, Entity, EntityInst, ConnectionStmt, HasName, BinaryExpr};
use bhdl_analyzer::types::AnalysisResult;
use bhdl_netlist::{Netlist, ModuleId, InstanceId, NetId, ConnectionPoint};
use bhdl_parser::BhdlLanguage;
use rowan::SyntaxNode;
use log::{debug, error, info, warn};
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
                        NetClass::Power { .. } | NetClass::Ground => return true,
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

    // Third pass: detect interface-field pin conflicts (v0.6).
    // When an entity declares multiple bindings sharing a physical
    // pin (PB3 = SPI.MOSI AND PB3 = ICSP.MOSI), a board using more
    // than one of the conflicting fields would route the same
    // pin to two roles. Reject early with a clear diagnostic.
    println!("=== Phase 3: Interface-field conflict detection ===");
    info!("=== Phase 3: Interface-field conflict detection ===");
    detect_interface_field_conflicts(ast, netlist)?;

    println!("=== COMPLETED hierarchical connectivity extraction ===");
    info!("=== COMPLETED hierarchical connectivity extraction ===");
    Ok(())
}

/// v0.6: walk the board(s) for connection statements, harvest
/// `(instance, field)` pairs used in any connection, then for each
/// instance check whether the used fields' pin bindings overlap.
/// Two used fields claiming the same physical pin is a conflict.
fn detect_interface_field_conflicts(
    ast: &bhdl_ast::SourceFile,
    netlist: &Netlist,
) -> Result<()> {
    use bhdl_parser::SyntaxKind;
    use rowan::ast::AstNode;

    // Map from instance name → set of interface-field names used.
    let mut used_fields: HashMap<String, HashSet<String>> = HashMap::new();

    for item in ast.items() {
        let bhdl_ast::Item::Board(board) = item else { continue; };
        // Walk every CONNECTION_STMT in the board body.
        for stmt in board.syntax().descendants() {
            if stmt.kind() != SyntaxKind::CONNECTION_STMT { continue; }
            let text = stmt.text().to_string();
            // Extract every `IDENT.IDENT(.IDENT)?` pattern.
            harvest_field_references(&text, &mut used_fields);
        }
    }

    // For each instance with at least one used field, look up its
    // module's bindings and detect overlaps.
    let mut conflicts = Vec::new();
    for (inst_name, fields) in &used_fields {
        if fields.len() < 2 { continue; }

        // Resolve the instance to its module.
        let Some((_, inst)) = netlist
            .instances
            .iter()
            .find(|(_, i)| &i.name == inst_name)
        else { continue; };
        let Some(module) = netlist.modules.get(inst.definition) else { continue; };

        // For each used field, collect (signal → physical pin) bindings.
        // physical_pin → set of (field, signal) that map to it.
        let mut pin_claims: HashMap<String, Vec<(String, String)>> = HashMap::new();
        for (attr_key, phys_pin) in &module.attributes {
            let Some(dotted) = attr_key.strip_prefix(INTERFACE_FIELD_BINDING_ATTR_PREFIX)
            else { continue; };
            let Some(dot_pos) = dotted.find('.') else { continue; };
            let field = &dotted[..dot_pos];
            let signal = &dotted[dot_pos + 1..];
            if !fields.contains(field) { continue; }
            pin_claims
                .entry(phys_pin.clone())
                .or_default()
                .push((field.to_string(), signal.to_string()));
        }

        for (pin, claimants) in pin_claims {
            if claimants.len() < 2 { continue; }
            // Reduce to distinct field names.
            let mut distinct_fields: Vec<&str> = claimants.iter().map(|(f, _)| f.as_str()).collect();
            distinct_fields.sort();
            distinct_fields.dedup();
            if distinct_fields.len() < 2 { continue; }
            let fields_label = distinct_fields.iter().map(|f| format!("`{}`", f)).collect::<Vec<_>>().join(" and ");
            let detail = claimants
                .iter()
                .map(|(f, s)| format!("{}.{}", f, s))
                .collect::<Vec<_>>()
                .join(", ");
            conflicts.push(format!(
                "pin `{}.{}` is claimed by multiple interface fields ({}). The board \
                 uses each of them: {}. One physical pin can only serve one role at \
                 a time — pick one interface to wire on this instance.",
                inst_name, pin, fields_label, detail,
            ));
        }
    }

    if !conflicts.is_empty() {
        for msg in &conflicts {
            error!("{}", msg);
            eprintln!("error: {}", msg);
        }
    }
    Ok(())
}

/// Scrape `instance.field(.signal)?` references out of a connection
/// statement's text. Quick-and-dirty: split on whitespace and
/// connection operators, look at each token, take the leading
/// `IDENT.IDENT` pair.
fn harvest_field_references(text: &str, used: &mut HashMap<String, HashSet<String>>) {
    for raw in text.split(|c: char| c.is_whitespace() || matches!(c, '-' | '>' | '<' | ';' | ',' | '(' | ')')) {
        let s = raw.trim();
        if s.is_empty() { continue; }
        let mut parts = s.split('.');
        let inst = match parts.next() { Some(s) if !s.is_empty() => s, _ => continue };
        let field = match parts.next() { Some(s) if !s.is_empty() => s, _ => continue };
        // Skip pure identifiers that don't look like instance refs
        // (no caret on what `instance` even is — but the second token
        // must look like an identifier, not e.g. an attribute or
        // operator). Filter aggressively.
        if !inst.chars().all(|c| c.is_alphanumeric() || c == '_') { continue; }
        if !field.chars().all(|c| c.is_alphanumeric() || c == '_') { continue; }
        used.entry(inst.to_string()).or_default().insert(field.to_string());
    }
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
                process_connection_stmt_as_flow(&child, netlist, context, analysis, import_preprocessor)?;
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
                    // Walk the AST to handle chains like A -> B -> C
                    let parts = extract_flow_chain_ast(conn_stmt.syntax());
                    
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
        // Resolve attribute values that reference a constructor param with a
        // default — `attribute tolerance = tolerance` → 5%,
        // `attribute voltage_rating = voltage` → 50V — so the module (and every
        // instance that copies its attributes) carries the real value, not the
        // literal reference text. Storing the raw token poisoned the
        // part-selection grade gate, which read a resistor's tolerance as the
        // bogus string "tolerance". Un-defaulted param refs (`value`) and
        // generic refs (`V_OUT`, substituted by the alias pass below) have no
        // resolved entry and fall back to the raw text via the per-key
        // unwrap_or — same contract as the component-library loader.
        let resolved =
            bhdl_analyzer::attribute_extraction::extract_module_attributes_resolved(entity);
        let mut entity_attrs: std::collections::HashMap<String, String> =
            bhdl_analyzer::attribute_extraction::extract_module_attributes(entity)
                .into_iter()
                .map(|(k, raw)| {
                    let v = resolved.get(&k).cloned().unwrap_or(raw);
                    (k, v)
                })
                .collect();

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
        // `power in` / `ground` pins carry their electrical ROLE as the
        // netlist direction — the add_pins_for_component convention every
        // power-aware ERC rule keys on (instance_rails, ERC007 unpowered,
        // ERC016 rail budget). Mapping only the in/out token here left
        // inline-instantiated parts' VIN as plain In and made those rules
        // blind to them.
        let pin_direction = match (pin_type, direction_str.as_deref()) {
            // `power out` (a regulator's SW/VOUT) DRIVES — it keeps Out;
            // only `power in` is a supply pin (the ERC007/ERC016 sense).
            (PinType::Power, Some("out")) => pin_direction,
            (PinType::Power, _) => PinDirection::Power,
            (PinType::Ground, _) => PinDirection::Ground,
            _ => pin_direction,
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
        // Also materialise interface-field signals as `field.signal`
        // pins on this module (v0.3 interfaces).
        add_interface_field_pins(&entity_ast, module_id, netlist, context, import_preprocessor);
        add_entity_aliases(&entity_ast, module_id, netlist);
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
            add_interface_field_pins(&entity_ast, module_id, netlist, context, import_preprocessor);
            add_entity_aliases(&entity_ast, module_id, netlist);
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
            // Passive conduction path — same fix as expansion_interpreter's
            // component_type_pins: A/K as In/Out made ERC001 read a diode
            // cathode as a push-pull driver.
            netlist.add_pin(module_id, "A".to_string(), PinDirection::InOut, PinType::Passive);
            netlist.add_pin(module_id, "K".to_string(), PinDirection::InOut, PinType::Passive);
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

    // Update every pin_instance that referenced the source net to point
    // at the target. Without this fix, pin instances retain stale
    // `net` pointers to the (now-removed) source net, and downstream
    // net→pin queries miss them entirely. The bug only surfaced once
    // a board file wrote a connection in pin-first/net-second order
    // (e.g. `mcu.GND1 -> @GND`), since that path goes through an
    // auto-net + merge. Net-first/pin-second order
    // (`@GND -> mcu.GND1`) avoids the merge and worked all along.
    let mut moved = 0usize;
    for (_pi_id, pi) in netlist.pin_instances.iter_mut() {
        if pi.net == Some(source_net_id) {
            pi.net = Some(target_net_id);
            moved += 1;
        }
    }
    if moved > 0 {
        debug!("merge_nets: rewrote {} pin_instance(s) from {:?} → {:?}",
            moved, source_net_id, target_net_id);
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

    let parts = extract_flow_chain_ast(node);
    println!("Parsed {} parts from bare connection", parts.len());
    info!("Parsed {} parts from bare connection", parts.len());

    // Bundle expansion (v0.3 interfaces): if every part is an
    // `instance.field` reference where the field is an interface
    // field (i.e. the instance has pins `field.X`, `field.Y`, …
    // but no pin literally named `field`), expand the single
    // chain into one parallel chain per signal. So
    //   MCU.spi -> FLASH.spi
    // becomes
    //   MCU.spi.MOSI -> FLASH.spi.MOSI
    //   MCU.spi.MISO -> FLASH.spi.MISO
    //   MCU.spi.SCK  -> FLASH.spi.SCK
    //   MCU.spi.CS   -> FLASH.spi.CS
    //
    // Mixed bundle/single-pin chains aren't expanded (we leave
    // those to the operator-by-operator form, which still works).
    let chains = expand_interface_bundle_chain(&parts, netlist, context);

    for chain in chains {
        // v0.5: direction-compatibility check for chains that are
        // pure interface-signal connections. Walks the endpoints,
        // looks up the logical interface direction on each side,
        // and rejects clearly-incompatible pairings (two `out`
        // endpoints driving the same line, etc.). Non-interface
        // chains skip the check.
        if let Err(e) = check_chain_directions(&chain, netlist, context) {
            error!("{}", e);
            eprintln!("error: {}", e);
            // Bail on this chain — refusing to silently produce
            // a netlist with a multi-driver short.
            continue;
        }
        // Process with no initial net — the first element in the chain will establish it
        process_flow_parts(&chain, None, netlist, context, analysis, import_preprocessor)?;
    }

    Ok(())
}

/// Walk a chain of `inst.field.signal` endpoints; if both endpoints
/// declare a logical interface-signal direction (`intf_dir__` attr)
/// and the directions clash, return an error.
///
/// Rules:
///   out × out  → error (two drivers)
///   in  × in   → error (no driver on the net)
///   inout × _  → ok (bidirectional pin tolerates either side)
///   _ × inout  → ok
///   out × in   → ok (the intended case)
///   in × out   → ok
///
/// Endpoints that don't carry an `intf_dir__` attribute are
/// non-interface pins and skip the check.
fn check_chain_directions(
    chain: &[String],
    netlist: &Netlist,
    context: &HierarchicalContext,
) -> std::result::Result<(), String> {
    // Collect (endpoint, logical_dir) for endpoints that have one.
    let mut info: Vec<(String, String)> = Vec::new();
    for endpoint in chain {
        let endpoint = endpoint.trim();
        let Some(dot) = endpoint.find('.') else { continue; };
        let instance_name = &endpoint[..dot];
        let dotted = &endpoint[dot + 1..];
        if !dotted.contains('.') { continue; } // not a `field.signal` shape

        let Some(inst_id) = find_instance_by_name_in_context(netlist, context, instance_name)
        else { continue; };
        let Some(inst) = netlist.instances.get(inst_id) else { continue; };
        let Some(module) = netlist.modules.get(inst.definition) else { continue; };
        let key = format!("{}{}", INTERFACE_FIELD_DIRECTION_ATTR_PREFIX, dotted);
        if let Some(dir) = module.attributes.get(&key) {
            info.push((endpoint.to_string(), dir.clone()));
        }
    }
    if info.len() < 2 { return Ok(()); }

    // Pairwise check among the typed endpoints.
    for i in 0..info.len() {
        for j in (i + 1)..info.len() {
            let (a_name, a_dir) = (&info[i].0, info[i].1.as_str());
            let (b_name, b_dir) = (&info[j].0, info[j].1.as_str());
            if a_dir == "inout" || b_dir == "inout" { continue; }
            if a_dir == "out" && b_dir == "out" {
                return Err(format!(
                    "incompatible directions on connection: `{}` and `{}` are both `out` \
                     (two drivers would short on the same net). Did you forget \
                     a `:slave` (or other opposing perspective) selector on one side?",
                    a_name, b_name
                ));
            }
            if a_dir == "in" && b_dir == "in" {
                return Err(format!(
                    "incompatible directions on connection: `{}` and `{}` are both `in` \
                     (no driver on the net). Did you forget a `:slave` (or other \
                     opposing perspective) selector on one side?",
                    a_name, b_name
                ));
            }
        }
    }
    Ok(())
}

/// Walk an entity's INTERFACE_FIELD_DECL children and add a Pin
/// to `module_id` for each signal in the referenced interface,
/// named `field.signal_name`. Applies direction reversal for the
/// `~InterfaceName` form.
///
/// This is the synthesiser-side counterpart to pass1's symbol
/// materialisation (bhdl-analyzer/src/pass1.rs); both must produce
/// the same pin set so connection resolution sees `field.signal`
/// in both the symbol table and the netlist.
/// Prefix used on `module.attributes` keys to store interface-field
/// pin bindings. `intf_bind__spi.MOSI = "PB3"` means the dotted
/// pin name `spi.MOSI` is an alias for the physical pin `PB3` on
/// this module. Encoded into attributes (rather than a dedicated
/// netlist field) so we don't have to thread additional mutable
/// state through the existing pin-population call chain.
pub(crate) const INTERFACE_FIELD_BINDING_ATTR_PREFIX: &str = "intf_bind__";

/// Prefix for storing the *logical* interface-signal direction on
/// the module's attributes. `intf_dir__spi.MOSI = "out"` means
/// signal MOSI on the field `spi` is declared as `out` (from this
/// entity's perspective, after any `~` reversal). Used by the
/// connection direction-compatibility check (v0.5) so that bound
/// interface signals (whose underlying physical pin is typically
/// `inout`) still report the logical direction the interface
/// claims.
pub(crate) const INTERFACE_FIELD_DIRECTION_ATTR_PREFIX: &str = "intf_dir__";

/// Prefix for storing the cross-perspective signal-name mapping
/// implied by an interface's `wires { }` block. Stored per-field
/// on the module's attributes: `intf_xwire__<field>__<my_signal>`
/// → `<other_perspective's_signal>`. Used by bundle expansion to
/// pair signals across perspectives whose names cross (UART:
/// dte.TX <-> dce.RX). Absent when `wires { }` was omitted (the
/// SPI/I²C same-name default).
pub(crate) const INTERFACE_FIELD_XWIRE_ATTR_PREFIX: &str = "intf_xwire__";

/// Prefix for entity-level function aliases (v0.9).
/// `alias__gpio0 = "PB0"` on a module means the logical pin name
/// `gpio0` is an alias for the physical pin `PB0`. Synthesizer
/// pin-lookup checks this prefix when a referenced pin name
/// isn't directly defined on the module — letting board authors
/// write `mcu.gpio0` instead of `mcu.PB0`. Parallel mechanism to
/// the interface-field bindings above but without the dotted
/// `field.signal` namespacing.
pub(crate) const ENTITY_ALIAS_ATTR_PREFIX: &str = "alias__";

fn add_interface_field_pins(
    entity: &bhdl_ast::Entity,
    module_id: ModuleId,
    netlist: &mut Netlist,
    context: &HierarchicalContext,
    import_preprocessor: Option<&crate::import_preprocessor::ImportPreprocessor>,
) {
    use bhdl_netlist::{PinDirection, PinType, PortDirection};
    use bhdl_parser::SyntaxKind;
    use rowan::ast::AstNode;

    for field_node in entity
        .syntax()
        .children()
        .filter(|n| n.kind() == SyntaxKind::INTERFACE_FIELD_DECL)
    {
        // Extract `~`, type name, perspective selector (v0.7),
        // and field name from the field decl.
        let mut reversed = false;
        let mut type_name: Option<String> = None;
        let mut perspective_name: Option<String> = None;
        let mut field_name: Option<String> = None;
        let tokens: Vec<_> = field_node
            .children_with_tokens()
            .filter_map(|el| el.into_token())
            .filter(|t| matches!(
                t.kind(),
                SyntaxKind::TILDE | SyntaxKind::COLON | SyntaxKind::IDENT
            ))
            .collect();
        let mut i = 0;
        while i < tokens.len() {
            match tokens[i].kind() {
                SyntaxKind::TILDE => { reversed = true; i += 1; }
                SyntaxKind::IDENT if type_name.is_none() => {
                    type_name = Some(tokens[i].text().to_string());
                    i += 1;
                }
                SyntaxKind::COLON if perspective_name.is_none() => {
                    i += 1;
                    if i < tokens.len() && tokens[i].kind() == SyntaxKind::IDENT {
                        perspective_name = Some(tokens[i].text().to_string());
                        i += 1;
                    }
                }
                SyntaxKind::IDENT if field_name.is_none() => {
                    field_name = Some(tokens[i].text().to_string());
                    i += 1;
                }
                _ => i += 1,
            }
        }
        let (Some(type_name), Some(field_name)) = (type_name, field_name) else { continue; };

        // v0.4 pinmux: collect per-signal bindings from the optional
        // `{ SIG = PIN; ... }` block. When bindings are present, the
        // field aliases existing pins instead of materialising new
        // ones.
        let mut bindings: HashMap<String, String> = HashMap::new();
        for child in field_node.children() {
            if child.kind() != SyntaxKind::INTERFACE_FIELD_BINDINGS { continue; }
            for binding in child.children().filter(|n| n.kind() == SyntaxKind::INTERFACE_FIELD_BINDING) {
                let idents: Vec<String> = binding
                    .children_with_tokens()
                    .filter_map(|el| el.into_token())
                    .filter(|t| t.kind() == SyntaxKind::IDENT || t.kind() == SyntaxKind::NUMBER)
                    .map(|t| t.text().to_string())
                    .collect();
                if idents.len() >= 2 {
                    bindings.insert(idents[0].clone(), idents[1].clone());
                }
            }
        }

        // Look up the interface definition in the variant manager
        // first (same-file), then in imports.
        let iface_node = context
            .variant_manager
            .find_entity_definition(&type_name)
            .map(|e| e.syntax().clone())
            .or_else(|| {
                import_preprocessor
                    .and_then(|p| p.get_imported_entity(&type_name).map(|e| e.syntax().clone()))
            });
        // Also fall back to scanning the entity's own source file
        // for an INTERFACE_DEF with the matching name. Interfaces
        // aren't entities; the variant_manager / preprocessor APIs
        // index entities only.
        let iface_node = iface_node.or_else(|| {
            let mut root = entity.syntax().clone();
            while let Some(p) = root.parent() { root = p; }
            root.descendants()
                .find(|n| {
                    if n.kind() != SyntaxKind::INTERFACE_DEF { return false; }
                    n.children_with_tokens()
                        .filter_map(|el| el.into_token())
                        .filter(|t| t.kind() == SyntaxKind::IDENT)
                        .next()
                        .map(|t| t.text() == type_name)
                        .unwrap_or(false)
                })
        });

        let Some(iface_node) = iface_node else { continue; };

        // v0.7: resolve the perspective selector to a list of
        // INTERFACE_SIGNAL nodes + whether directions need
        // flipping (only true for legacy `~` with no explicit
        // perspectives).
        let (signal_nodes, flip_for_legacy) = synth_resolve_perspective_signals(
            &iface_node,
            perspective_name.as_deref(),
            reversed,
        );

        // v0.7c: harvest cross-perspective signal mappings from the
        // interface's optional `wires { }` block. For each entry
        // `lhs_persp.lhs_sig <-> rhs_persp.rhs_sig`, if our
        // perspective is on one side, record the other-side signal
        // name we pair with. Used by bundle expansion to wire
        // cross-name protocols (UART) correctly.
        let our_perspective = match (&perspective_name, reversed) {
            (Some(name), _) => Some(name.clone()),
            (None, true) => {
                // Legacy `~`: resolve to second-declared perspective name
                // (if any) so the xwire lookup keys still work.
                synth_nth_perspective_name(&iface_node, 1)
            }
            (None, false) => synth_nth_perspective_name(&iface_node, 0),
        };
        let xwires_for_field: HashMap<String, String> =
            synth_harvest_xwires(&iface_node, our_perspective.as_deref());

        for sig_node in signal_nodes {
            // Pull signal name + direction.
            let sig_name_tok = sig_node
                .children_with_tokens()
                .filter_map(|el| el.into_token())
                .find(|t| t.kind() == SyntaxKind::IDENT);
            let Some(sig_name_tok) = sig_name_tok else { continue; };
            let sig_name = sig_name_tok.text().to_string();

            let dir_tok = sig_node
                .children_with_tokens()
                .filter_map(|el| el.into_token())
                .find(|t| matches!(
                    t.kind(),
                    SyntaxKind::IN_KW | SyntaxKind::OUT_KW | SyntaxKind::INOUT_KW
                ));
            let mut pin_direction = match dir_tok.as_ref().map(|t| t.kind()) {
                Some(SyntaxKind::IN_KW) => PinDirection::In,
                Some(SyntaxKind::OUT_KW) => PinDirection::Out,
                Some(SyntaxKind::INOUT_KW) => PinDirection::InOut,
                _ => PinDirection::InOut,
            };
            let mut port_direction = match dir_tok.as_ref().map(|t| t.kind()) {
                Some(SyntaxKind::IN_KW) => PortDirection::Input,
                Some(SyntaxKind::OUT_KW) => PortDirection::Output,
                Some(SyntaxKind::INOUT_KW) => PortDirection::InOut,
                _ => PortDirection::InOut,
            };
            if flip_for_legacy {
                pin_direction = match pin_direction {
                    PinDirection::In => PinDirection::Out,
                    PinDirection::Out => PinDirection::In,
                    PinDirection::InOut => PinDirection::InOut,
                    other => other,
                };
                port_direction = match port_direction {
                    PortDirection::Input => PortDirection::Output,
                    PortDirection::Output => PortDirection::Input,
                    other => other,
                };
            }

            let pin_name = format!("{}.{}", field_name, sig_name);

            // Record the *logical* interface-signal direction for
            // direction-compatibility checking. We store it on the
            // module's attributes whether or not the field is
            // bound, so both forms participate in the check.
            let dir_str = match pin_direction {
                PinDirection::In  => "in",
                PinDirection::Out => "out",
                PinDirection::InOut => "inout",
                _ => "unknown",
            };
            if let Some(module) = netlist.modules.get_mut(module_id) {
                let dir_key = format!(
                    "{}{}", INTERFACE_FIELD_DIRECTION_ATTR_PREFIX, pin_name,
                );
                module.attributes.insert(dir_key, dir_str.to_string());

                // Cross-wire mapping for this signal, if the interface
                // declares one. Bundle expansion reads this to wire
                // cross-name protocols.
                if let Some(other_side_sig) = xwires_for_field.get(&sig_name) {
                    let xwire_key = format!(
                        "{}{}__{}",
                        INTERFACE_FIELD_XWIRE_ATTR_PREFIX, field_name, sig_name,
                    );
                    module.attributes.insert(xwire_key, other_side_sig.clone());
                }
            }

            if let Some(target_pin) = bindings.get(&sig_name) {
                // Bound: don't create a new pin. Record the alias on
                // the module's attribute map under a reserved prefix
                // so connection processing can resolve `spi.MOSI` to
                // the canonical physical pin (`PB3`).
                if let Some(module) = netlist.modules.get_mut(module_id) {
                    let key = format!(
                        "{}{}",
                        INTERFACE_FIELD_BINDING_ATTR_PREFIX, pin_name,
                    );
                    module.attributes.insert(key, target_pin.clone());
                }
            } else {
                netlist.add_port(module_id, pin_name.clone(), port_direction, None);
                netlist.add_pin(module_id, pin_name, pin_direction, PinType::Signal);
            }
        }

        // v0.8 hierarchical sub-interfaces: this interface may itself
        // declare `interface SubName subField;` rows. Recursively
        // materialise their pins under the dotted prefix
        // `field_name.sub_field_name.*`. Sub-fields inherit our
        // perspective + legacy-flip state (e.g., DualUART:dte → all
        // sub-channels resolved as `dte`).
        add_sub_interface_field_pins(
            &iface_node,
            module_id,
            netlist,
            context,
            import_preprocessor,
            entity,
            &field_name,
            perspective_name.as_deref(),
            reversed,
        );

        // v0.8 constraints: walk this interface's `constraints { }`
        // block (if any) and attach each property to the materialised
        // pins under the dotted prefix `field_name.…`. Sub-interface
        // constraints are applied during recursion above, so by the
        // time we reach here all leaf pins this constraint block can
        // reference already exist in `module.pins`.
        apply_iface_constraints(&iface_node, module_id, netlist, &field_name, &type_name);
    }
}

/// v0.8 hierarchical sub-interfaces: recursively materialise pins for
/// any `interface SubName subField;` rows declared inside `iface_node`'s
/// body, under the dotted prefix `parent_prefix.sub_field`.
///
/// Sub-interface fields inherit the *parent's* perspective + `~`
/// reversal so a `DualUART:dte` selector flows to both `ch0` and `ch1`
/// as `dte`. Pinmux bindings, `wires { }` cross-name pairing, and
/// explicit per-sub-field perspective selectors are NOT supported at
/// the nested level in this slice; they remain leaf-only features and
/// would need a richer recursion if hierarchical interfaces grow them
/// later.
#[allow(clippy::too_many_arguments)]
fn add_sub_interface_field_pins(
    iface_node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
    module_id: ModuleId,
    netlist: &mut Netlist,
    context: &HierarchicalContext,
    import_preprocessor: Option<&crate::import_preprocessor::ImportPreprocessor>,
    entity: &bhdl_ast::Entity,
    parent_prefix: &str,
    parent_perspective: Option<&str>,
    parent_reversed: bool,
) {
    use bhdl_netlist::{PinDirection, PinType, PortDirection};
    use bhdl_parser::SyntaxKind;

    for field_node in iface_node
        .children()
        .filter(|n| n.kind() == SyntaxKind::INTERFACE_FIELD_DECL)
    {
        // Same token-walk as the parent loop: `[~] TypeName [: persp] fieldName`.
        let mut reversed = false;
        let mut type_name: Option<String> = None;
        let mut perspective_name: Option<String> = None;
        let mut field_name: Option<String> = None;
        let tokens: Vec<_> = field_node
            .children_with_tokens()
            .filter_map(|el| el.into_token())
            .filter(|t| matches!(
                t.kind(),
                SyntaxKind::TILDE | SyntaxKind::COLON | SyntaxKind::IDENT
            ))
            .collect();
        let mut i = 0;
        while i < tokens.len() {
            match tokens[i].kind() {
                SyntaxKind::TILDE => { reversed = true; i += 1; }
                SyntaxKind::IDENT if type_name.is_none() => {
                    type_name = Some(tokens[i].text().to_string());
                    i += 1;
                }
                SyntaxKind::COLON if perspective_name.is_none() => {
                    i += 1;
                    if i < tokens.len() && tokens[i].kind() == SyntaxKind::IDENT {
                        perspective_name = Some(tokens[i].text().to_string());
                        i += 1;
                    }
                }
                SyntaxKind::IDENT if field_name.is_none() => {
                    field_name = Some(tokens[i].text().to_string());
                    i += 1;
                }
                _ => i += 1,
            }
        }
        let (Some(type_name), Some(field_name)) = (type_name, field_name) else { continue; };

        // Inherit parent's perspective + reversed when this sub-field
        // didn't declare its own. The combined `reversed` xors so that
        // `~` at either level toggles direction.
        let effective_perspective = perspective_name.or_else(|| parent_perspective.map(|s| s.to_string()));
        let effective_reversed = reversed ^ parent_reversed;

        // Resolve sub-interface definition via the same three-step chain.
        let sub_iface_node = context
            .variant_manager
            .find_entity_definition(&type_name)
            .map(|e| e.syntax().clone())
            .or_else(|| {
                import_preprocessor
                    .and_then(|p| p.get_imported_entity(&type_name).map(|e| e.syntax().clone()))
            })
            .or_else(|| {
                let mut root = entity.syntax().clone();
                while let Some(p) = root.parent() { root = p; }
                root.descendants().find(|n| {
                    if n.kind() != SyntaxKind::INTERFACE_DEF { return false; }
                    n.children_with_tokens()
                        .filter_map(|el| el.into_token())
                        .filter(|t| t.kind() == SyntaxKind::IDENT)
                        .next()
                        .map(|t| t.text() == type_name)
                        .unwrap_or(false)
                })
            });
        let Some(sub_iface_node) = sub_iface_node else { continue; };

        let (signal_nodes, flip_for_legacy) = synth_resolve_perspective_signals(
            &sub_iface_node,
            effective_perspective.as_deref(),
            effective_reversed,
        );

        let nested_prefix = format!("{}.{}", parent_prefix, field_name);

        // Cross-wire harvest from the sub-interface (e.g.,
        // UartChannel's `wires { dte.TX <-> dce.RX; }`). The bundle
        // expander's `translate_via_xwire` looks up xwires under the
        // *outer* field name (the user-written `mcu.duart`) using the
        // remaining nested path as part of the signal key. We split
        // `nested_prefix` at the first dot to get
        // `outer_field` + `rest_path`, then store one entry per
        // sub-interface signal whose name remaps.
        let (outer_field, rest_path) = match nested_prefix.find('.') {
            Some(p) => (&nested_prefix[..p], &nested_prefix[p + 1..]),
            None => (nested_prefix.as_str(), ""),
        };
        let nested_xwires: HashMap<String, String> =
            synth_harvest_xwires(&sub_iface_node, effective_perspective.as_deref());
        if let Some(module) = netlist.modules.get_mut(module_id) {
            for (lhs_sig, rhs_sig) in &nested_xwires {
                let key_sig = if rest_path.is_empty() {
                    lhs_sig.clone()
                } else {
                    format!("{}.{}", rest_path, lhs_sig)
                };
                let val_sig = if rest_path.is_empty() {
                    rhs_sig.clone()
                } else {
                    format!("{}.{}", rest_path, rhs_sig)
                };
                let key = format!(
                    "{}{}__{}",
                    INTERFACE_FIELD_XWIRE_ATTR_PREFIX, outer_field, key_sig,
                );
                module.attributes.insert(key, val_sig);
            }
        }

        for sig_node in signal_nodes {
            let sig_name_tok = sig_node
                .children_with_tokens()
                .filter_map(|el| el.into_token())
                .find(|t| t.kind() == SyntaxKind::IDENT);
            let Some(sig_name_tok) = sig_name_tok else { continue; };
            let sig_name = sig_name_tok.text().to_string();

            let dir_tok = sig_node
                .children_with_tokens()
                .filter_map(|el| el.into_token())
                .find(|t| matches!(
                    t.kind(),
                    SyntaxKind::IN_KW | SyntaxKind::OUT_KW | SyntaxKind::INOUT_KW
                ));
            let mut pin_direction = match dir_tok.as_ref().map(|t| t.kind()) {
                Some(SyntaxKind::IN_KW) => PinDirection::In,
                Some(SyntaxKind::OUT_KW) => PinDirection::Out,
                Some(SyntaxKind::INOUT_KW) => PinDirection::InOut,
                _ => PinDirection::InOut,
            };
            let mut port_direction = match dir_tok.as_ref().map(|t| t.kind()) {
                Some(SyntaxKind::IN_KW) => PortDirection::Input,
                Some(SyntaxKind::OUT_KW) => PortDirection::Output,
                Some(SyntaxKind::INOUT_KW) => PortDirection::InOut,
                _ => PortDirection::InOut,
            };
            if flip_for_legacy {
                pin_direction = match pin_direction {
                    PinDirection::In => PinDirection::Out,
                    PinDirection::Out => PinDirection::In,
                    PinDirection::InOut => PinDirection::InOut,
                    other => other,
                };
                port_direction = match port_direction {
                    PortDirection::Input => PortDirection::Output,
                    PortDirection::Output => PortDirection::Input,
                    other => other,
                };
            }

            let pin_name = format!("{}.{}", nested_prefix, sig_name);
            let dir_str = match pin_direction {
                PinDirection::In => "in",
                PinDirection::Out => "out",
                PinDirection::InOut => "inout",
                _ => "unknown",
            };
            if let Some(module) = netlist.modules.get_mut(module_id) {
                let dir_key = format!(
                    "{}{}", INTERFACE_FIELD_DIRECTION_ATTR_PREFIX, pin_name,
                );
                module.attributes.insert(dir_key, dir_str.to_string());
            }
            netlist.add_port(module_id, pin_name.clone(), port_direction, None);
            netlist.add_pin(module_id, pin_name, pin_direction, PinType::Signal);
        }

        // Tail recursion: deeper nesting (e.g., RGMII { tx { phy { ... } } })
        // is uncommon today but cheap to support.
        add_sub_interface_field_pins(
            &sub_iface_node,
            module_id,
            netlist,
            context,
            import_preprocessor,
            entity,
            &nested_prefix,
            effective_perspective.as_deref(),
            effective_reversed,
        );

        // v0.8 constraints on the sub-interface itself (e.g., DiffPair's
        // `*: differential 100ohm`) get attached to the leaves
        // materialised under `nested_prefix.*`.
        apply_iface_constraints(&sub_iface_node, module_id, netlist, &nested_prefix, &type_name);
    }
}

/// v0.8 constraints — attribute-key prefixes.
///
/// Per-pin properties are stored as
///   `intf_const__<dotted_pin_name>__<prop_name>` → `<value_text>`
/// Cross-pin relations are stored as
///   `intf_const_rel__<from_pin>__<to_pin>__<prop_name>` → `<value_text>`
///
/// Downstream consumers (PCB router/DRC, future SI analysers, BOM
/// walkers wanting termination-rail info) iterate the module's
/// attributes and read by prefix.
pub const INTERFACE_CONSTRAINT_ATTR_PREFIX: &str = "intf_const__";
pub const INTERFACE_CONSTRAINT_REL_ATTR_PREFIX: &str = "intf_const_rel__";

/// Walk an interface's `constraints { }` block (if present) and
/// attach each statement's properties to the module attributes,
/// keyed by the materialised pin path under `prefix`.
///
/// Target syntax (in tier 1):
///   - `*`            — every leaf pin under `prefix.*`
///   - `IDENT`        — `prefix.IDENT`
///   - `IDENT.IDENT…` — fully qualified path (dotted) under prefix
///   - `IDENT*`       — wildcard suffix; matches every leaf whose
///                       leaf-segment starts with `IDENT`
///   - `IDENT.*`      — every leaf under `prefix.IDENT.*`
///
/// Relations (`A -> B: prop`) cross-product the LHS-resolved set
/// with the RHS-resolved set.
///
/// # Tier-2 (task #96): multi-value storage + override precedence + provenance
///
/// A `(pin, prop)` slot may be targeted by more than one statement — most
/// commonly a broad wildcard (`*: single_ended 40ohm`) plus a specific
/// override (`DQ0: single_ended 50ohm`). The earlier implementation blindly
/// `insert`ed, so the last writer silently won and the disagreement was
/// invisible downstream. We now:
///
/// - accumulate every contributor (value + tier + source line + the
///   declaring interface type name) per attribute key;
/// - resolve the **winning** value by override precedence —
///   [`ConstraintTier::Specific`] (explicit pin) beats
///   [`ConstraintTier::Interface`] (wildcard), ties broken last-writer —
///   and store it in the primary `intf_const__*` attribute exactly as
///   before (backward compatible);
/// - emit the full contributor map once per module under
///   [`INTERFACE_CONSTRAINT_PROVENANCE_ATTR`] (a single JSON attribute) so
///   the P&R session can render traceable diagnostics and detect
///   same-tier contradictions. Cross-net contradictions remain P&R's to
///   detect post-net-merge (handshake §10).
///
/// `scope` is the declaring interface's type name (e.g. `"DDR4Data"`),
/// recorded in each provenance entry alongside the source line.
fn apply_iface_constraints(
    iface_node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
    module_id: ModuleId,
    netlist: &mut Netlist,
    prefix: &str,
    scope: &str,
) {
    use bhdl_parser::SyntaxKind;
    use bhdl_common::constraint_provenance::{
        ConstraintProvenance, ConstraintProvenanceMap, INTERFACE_CONSTRAINT_PROVENANCE_ATTR,
    };

    // Snapshot the materialised pin set ONCE so wildcard expansion
    // doesn't see partial state if anything reorders mid-loop.
    let module_pins: Vec<String> = match netlist.modules.get(module_id) {
        Some(m) => m
            .pins
            .iter()
            .filter_map(|pid| netlist.pins.get(*pid).map(|p| p.name.clone()))
            .collect(),
        None => return,
    };

    // Accumulate contributors per primary attribute key across all
    // statements in this interface's constraint block(s).
    let mut acc: std::collections::HashMap<String, Vec<ConstraintProvenance>> =
        std::collections::HashMap::new();

    for cb in iface_node
        .children()
        .filter(|n| n.kind() == SyntaxKind::CONSTRAINTS_BLOCK)
    {
        for stmt in cb.children().filter(|n| n.kind() == SyntaxKind::CONSTRAINT_STMT) {
            let lhs_text = stmt
                .children()
                .find(|n| n.kind() == SyntaxKind::CONSTRAINT_LHS)
                .map(|n| n.text().to_string())
                .unwrap_or_default();
            let rhs_text = stmt
                .children()
                .find(|n| n.kind() == SyntaxKind::CONSTRAINT_RHS)
                .map(|n| n.text().to_string());
            let props_text = stmt
                .children()
                .find(|n| n.kind() == SyntaxKind::CONSTRAINT_PROPS)
                .map(|n| n.text().to_string())
                .unwrap_or_default();

            let props = parse_constraint_props(&props_text);
            if props.is_empty() { continue; }

            let line = constraint_stmt_line(&stmt);

            match rhs_text {
                Some(rhs) => {
                    // Relation: cross-product per-target so each endpoint
                    // pair carries its own tier (Specific only when both
                    // endpoints are explicit).
                    for fraw in lhs_text.split(',') {
                        let ft = fraw.trim();
                        if ft.is_empty() { continue; }
                        let ftier = constraint_target_tier(ft);
                        let froms = resolve_one_target(ft, prefix, &module_pins);
                        for rraw in rhs.split(',') {
                            let rt = rraw.trim();
                            if rt.is_empty() { continue; }
                            let rtier = constraint_target_tier(rt);
                            let tier = ftier.min(rtier);
                            let tos = resolve_one_target(rt, prefix, &module_pins);
                            for from in &froms {
                                for to in &tos {
                                    for (k, v) in &props {
                                        let key = format!(
                                            "{}{}__{}__{}",
                                            INTERFACE_CONSTRAINT_REL_ATTR_PREFIX,
                                            from, to, k,
                                        );
                                        acc.entry(key).or_default().push(
                                            ConstraintProvenance::new(
                                                v.clone(), line, tier, scope,
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                None => {
                    for raw in lhs_text.split(',') {
                        let t = raw.trim();
                        if t.is_empty() { continue; }
                        let tier = constraint_target_tier(t);
                        let pins = resolve_one_target(t, prefix, &module_pins);
                        for pin in &pins {
                            for (k, v) in &props {
                                let key = format!(
                                    "{}{}__{}",
                                    INTERFACE_CONSTRAINT_ATTR_PREFIX, pin, k,
                                );
                                acc.entry(key).or_default().push(
                                    ConstraintProvenance::new(v.clone(), line, tier, scope),
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    if acc.is_empty() { return; }

    if let Some(module) = netlist.modules.get_mut(module_id) {
        // Merge into any provenance map a prior call (another interface
        // field on this same module) already wrote.
        let mut prov_map: ConstraintProvenanceMap = module
            .attributes
            .get(INTERFACE_CONSTRAINT_PROVENANCE_ATTR)
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        for (key, entries) in acc {
            // Primary attribute = the override winner (back-compat shape).
            if let Some(win) = ConstraintProvenance::winner(&entries) {
                module.attributes.insert(key.clone(), win.value.clone());
            }
            // Surface a within-module same-tier contradiction at emit time
            // (two equally-authoritative statements disagreeing on one
            // (pin, prop) — distinct from an intentional specific-over-
            // wildcard override). This is the synth-owned half of the
            // conflict split; cross-net, post-net-merge contradictions are
            // P&R's §9 pass (handshake §10/§11/§13). We warn rather than
            // fail: the override winner is still well-defined (last
            // writer), so the build proceeds, but the disagreement is
            // logged with each side's origin.
            if ConstraintProvenance::has_same_tier_conflict(&entries) {
                let detail = entries
                    .iter()
                    .map(|e| {
                        let loc = e
                            .line
                            .map(|l| format!("{}:{}", e.scope, l))
                            .unwrap_or_else(|| e.scope.clone());
                        format!("{} @ {} ({})", e.value, loc, e.tier.as_str())
                    })
                    .collect::<Vec<_>>()
                    .join(" vs ");
                let winner_val = ConstraintProvenance::winner(&entries)
                    .map(|w| w.value.as_str())
                    .unwrap_or("<none>");
                warn!(
                    "interface constraint conflict on `{}`: {} — using `{}` \
                     (last writer); make the intended value the more-specific \
                     target to silence this",
                    key, detail, winner_val,
                );
            }
            prov_map.entry(key).or_default().extend(entries);
        }

        if let Ok(json) = serde_json::to_string(&prov_map) {
            module
                .attributes
                .insert(INTERFACE_CONSTRAINT_PROVENANCE_ATTR.to_string(), json);
        }
    }
}

/// 1-based source line of a constraint statement within its file, derived
/// from the syntax tree (count newlines before the node's start offset).
/// `None` if the offset can't be sliced (shouldn't happen for real nodes).
/// Best-effort: when the interface was source-text-monomorphised (a
/// parametric interface), the line is relative to the preprocessed text.
fn constraint_stmt_line(
    stmt: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
) -> Option<u32> {
    let mut root = stmt.clone();
    while let Some(p) = root.parent() {
        root = p;
    }
    let src = root.text().to_string();
    let off = usize::from(stmt.text_range().start());
    let prefix = src.get(..off)?;
    Some(prefix.matches('\n').count() as u32 + 1)
}

/// Override tier of a raw constraint target token: a wildcard target
/// (`*`, `DQ*`, `CK.*`) is the broad [`ConstraintTier::Interface`]; an
/// explicit pin/bundle name is the narrower [`ConstraintTier::Specific`]
/// and overrides a wildcard on the same `(pin, prop)`.
fn constraint_target_tier(raw: &str) -> bhdl_common::constraint_provenance::ConstraintTier {
    use bhdl_common::constraint_provenance::ConstraintTier;
    if raw.contains('*') {
        ConstraintTier::Interface
    } else {
        ConstraintTier::Specific
    }
}

/// Resolve a comma-separated target list (LHS or RHS text) into a
/// concrete pin-name set, all under the given `prefix`. Wildcards
/// match against `module_pins` (materialised pin names that start
/// with `prefix.`).
fn resolve_constraint_targets(
    text: &str,
    prefix: &str,
    module_pins: &[String],
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.split(',') {
        let target = raw.trim();
        if target.is_empty() { continue; }
        let resolved = resolve_one_target(target, prefix, module_pins);
        for r in resolved {
            if !out.contains(&r) { out.push(r); }
        }
    }
    out
}

fn resolve_one_target(target: &str, prefix: &str, module_pins: &[String]) -> Vec<String> {
    // `*` alone — every leaf under prefix.*
    if target == "*" {
        let p = format!("{}.", prefix);
        return module_pins.iter()
            .filter(|n| n.starts_with(&p))
            .cloned()
            .collect();
    }

    // Trailing wildcard: `DQ*` or `lane*.DQS` or `CK.*`
    if let Some(stripped) = target.strip_suffix(".*") {
        let p = format!("{}.{}.", prefix, stripped);
        return module_pins.iter()
            .filter(|n| n.starts_with(&p))
            .cloned()
            .collect();
    }
    if target.ends_with('*') {
        let stem = &target[..target.len() - 1]; // strip trailing `*`
        // For something like `DQ*` we want every pin whose leaf
        // segment (after the prefix) starts with `DQ`. For
        // `lane*.DQS` we'd want a multi-segment wildcard; that
        // shape isn't supported in tier 1 — bail to no-match.
        if stem.contains('*') || stem.contains('.') && !stem.ends_with('.') {
            // Mixed wildcard / dotted form — tier-2 work.
            return Vec::new();
        }
        let p = format!("{}.{}", prefix, stem);
        return module_pins.iter()
            .filter(|n| n.starts_with(&p))
            .cloned()
            .collect();
    }

    // Plain dotted path: prefix.IDENT[.IDENT…]
    let full = format!("{}.{}", prefix, target);
    if module_pins.iter().any(|n| n == &full) {
        return vec![full];
    }
    // Could be a sub-bundle reference (no leaf with that exact name
    // but leaves below it). Match by prefix.
    let p = format!("{}.", full);
    let matches: Vec<String> = module_pins
        .iter()
        .filter(|n| n.starts_with(&p))
        .cloned()
        .collect();
    matches
}

/// Parse a property list like `single_ended 40ohm, signal_class DATA`
/// into (name, value-text) pairs. The name is the first token; the
/// value is everything after it up to the next comma.
fn parse_constraint_props(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for chunk in text.split(',') {
        let s = chunk.trim();
        if s.is_empty() { continue; }
        let (name, value) = match s.find(|c: char| c.is_whitespace()) {
            Some(p) => (s[..p].to_string(), s[p..].trim().to_string()),
            None => (s.to_string(), String::new()),
        };
        if name.is_empty() { continue; }
        out.push((name, value));
    }
    out
}

/// Name of the n-th declared perspective on this interface (0-indexed).
/// `None` if the interface declares fewer than n+1 perspectives.
fn synth_nth_perspective_name(
    iface: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
    n: usize,
) -> Option<String> {
    use bhdl_parser::SyntaxKind;
    let mut idx = 0;
    for child in iface.children() {
        if child.kind() != SyntaxKind::INTERFACE_PERSPECTIVE { continue; }
        if idx == n {
            return child
                .children_with_tokens()
                .filter_map(|el| el.into_token())
                .find(|t| t.kind() == SyntaxKind::IDENT)
                .map(|t| t.text().to_string());
        }
        idx += 1;
    }
    None
}

/// Harvest the cross-perspective signal mappings from an interface's
/// optional `wires { }` block, restricted to the side `my_perspective`.
/// Returns a map `my_signal_name → other_perspective_signal_name`.
///
/// Each `<lhs_persp.lhs_sig> <-> <rhs_persp.rhs_sig>` entry contributes:
///   - if `lhs_persp == my_perspective`: lhs_sig → rhs_sig
///   - if `rhs_persp == my_perspective`: rhs_sig → lhs_sig
/// Both directions handled so the lookup works regardless of which
/// side of the `<->` operator the user wrote first.
fn synth_harvest_xwires(
    iface: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
    my_perspective: Option<&str>,
) -> HashMap<String, String> {
    use bhdl_parser::SyntaxKind;
    let mut out = HashMap::new();
    let Some(mine) = my_perspective else { return out; };

    for child in iface.children() {
        if child.kind() != SyntaxKind::INTERFACE_WIRES_BLOCK { continue; }
        for mapping in child.children().filter(|n| n.kind() == SyntaxKind::INTERFACE_WIRE_MAPPING) {
            let idents: Vec<String> = mapping
                .children_with_tokens()
                .filter_map(|el| el.into_token())
                .filter(|t| t.kind() == SyntaxKind::IDENT)
                .map(|t| t.text().to_string())
                .collect();
            if idents.len() != 4 { continue; }
            let (lhs_persp, lhs_sig, rhs_persp, rhs_sig) =
                (&idents[0], &idents[1], &idents[2], &idents[3]);
            if lhs_persp == mine {
                out.insert(lhs_sig.clone(), rhs_sig.clone());
            } else if rhs_persp == mine {
                out.insert(rhs_sig.clone(), lhs_sig.clone());
            }
        }
    }
    out
}

/// v0.7: pick the INTERFACE_SIGNAL nodes corresponding to the
/// requested perspective.
///
/// Same logic as the analyzer's `resolve_perspective_signals`:
///   1. explicit selector → that perspective.
///   2. legacy `~`: second-declared perspective if any, else
///      top-level signals with directions flipped.
///   3. no selector: first-declared perspective if any, else
///      top-level signals (v0.6 single-implicit-perspective form).
///
/// Returns `(signals, flip_for_legacy)`. `flip_for_legacy=true`
/// only in case (2)'s fallback branch.
fn synth_resolve_perspective_signals(
    iface: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
    perspective: Option<&str>,
    legacy_reversed: bool,
) -> (Vec<rowan::SyntaxNode<bhdl_parser::BhdlLanguage>>, bool) {
    use bhdl_parser::SyntaxKind;

    let perspectives: Vec<_> = iface
        .children()
        .filter(|n| n.kind() == SyntaxKind::INTERFACE_PERSPECTIVE)
        .collect();

    if let Some(name) = perspective {
        for p in &perspectives {
            let p_name = p
                .children_with_tokens()
                .filter_map(|el| el.into_token())
                .find(|t| t.kind() == SyntaxKind::IDENT)
                .map(|t| t.text().to_string());
            if p_name.as_deref() == Some(name) {
                let sigs = p
                    .children()
                    .filter(|n| n.kind() == SyntaxKind::INTERFACE_SIGNAL)
                    .collect();
                return (sigs, false);
            }
        }
        // Fall through if name didn't match.
    }

    if legacy_reversed {
        if perspectives.len() >= 2 {
            let sigs = perspectives[1]
                .children()
                .filter(|n| n.kind() == SyntaxKind::INTERFACE_SIGNAL)
                .collect();
            return (sigs, false);
        }
        let sigs = iface
            .children()
            .filter(|n| n.kind() == SyntaxKind::INTERFACE_SIGNAL)
            .collect();
        return (sigs, true);
    }

    if !perspectives.is_empty() {
        let sigs = perspectives[0]
            .children()
            .filter(|n| n.kind() == SyntaxKind::INTERFACE_SIGNAL)
            .collect();
        return (sigs, false);
    }

    let sigs = iface
        .children()
        .filter(|n| n.kind() == SyntaxKind::INTERFACE_SIGNAL)
        .collect();
    (sigs, false)
}

/// Look up an interface-field pin-binding alias on the instance's
/// module. Returns the canonical pin name if the dotted reference
/// is an alias; `None` if it's a regular pin or no binding exists.
fn resolve_field_binding_alias<'a>(
    netlist: &'a Netlist,
    inst_id: InstanceId,
    pin_name: &str,
) -> Option<&'a str> {
    let inst = netlist.instances.get(inst_id)?;
    let module = netlist.modules.get(inst.definition)?;
    // First try the v0.7 interface-field-binding form: `intf_bind__spi.MOSI`.
    let key = format!("{}{}", INTERFACE_FIELD_BINDING_ATTR_PREFIX, pin_name);
    if let Some(physical) = module.attributes.get(&key) {
        return Some(physical.as_str());
    }
    // Then try the v0.9 entity-alias form: `alias__gpio0`.
    let key = format!("{}{}", ENTITY_ALIAS_ATTR_PREFIX, pin_name);
    module.attributes.get(&key).map(|s| s.as_str())
}

/// Walk an entity's AST looking for `aliases { gpio0 = PB0; … }`
/// blocks and stamp each mapping onto the module's attributes
/// using the `alias__<name>` prefix. Called once per module
/// during creation, parallel to `add_interface_field_pins`.
fn add_entity_aliases(
    entity_ast: &Entity,
    module_id: ModuleId,
    netlist: &mut Netlist,
) {
    use bhdl_ast::SyntaxKind;
    use rowan::ast::AstNode;

    let mut count = 0;

    // Find all ENTITY_ALIASES_BLOCK nodes inside the entity body.
    for node in entity_ast.syntax().descendants() {
        if node.kind() != SyntaxKind::ENTITY_ALIASES_BLOCK { continue; }
        for mapping in node.children() {
            if mapping.kind() != SyntaxKind::ENTITY_ALIAS_MAPPING { continue; }
            // Each mapping has two IDENT tokens: alias_name, then physical pin name.
            let idents: Vec<String> = mapping.children_with_tokens()
                .filter_map(|el| el.into_token())
                .filter(|t| t.kind() == SyntaxKind::IDENT)
                .map(|t| t.text().to_string())
                .collect();
            if idents.len() < 2 { continue; }
            let alias_name = &idents[0];
            let physical_name = &idents[1];
            if let Some(module) = netlist.modules.get_mut(module_id) {
                module.attributes.insert(
                    format!("{}{}", ENTITY_ALIAS_ATTR_PREFIX, alias_name),
                    physical_name.clone(),
                );
                count += 1;
            }
        }
    }
    if count > 0 {
        info!("add_entity_aliases: stamped {} alias(es) on module {:?}", count, module_id);
    }
}

/// If `parts` is a pure bundle-bundle chain (every endpoint is
/// `inst.field` where `field` is an interface field on `inst`),
/// expand into one chain per signal sorted alphabetically. Otherwise
/// return `[parts]` unchanged.
fn expand_interface_bundle_chain(
    parts: &[String],
    netlist: &Netlist,
    context: &HierarchicalContext,
) -> Vec<Vec<String>> {
    // First pass: gather (instance_name, field_name, signal_set) per
    // candidate part. Bail out the moment any part doesn't look like
    // a bundle ref.
    let mut bundle_info: Vec<(String, String, Vec<String>)> = Vec::with_capacity(parts.len());
    for part in parts {
        let endpoint = part.trim();
        let Some(dot) = endpoint.find('.') else {
            return vec![parts.to_vec()];
        };
        let instance_name = &endpoint[..dot];
        let field_name = &endpoint[dot + 1..];

        // Skip if endpoint already has a nested dot (`A.spi.MOSI`)
        // — that's the explicit per-signal form, not a bundle.
        if field_name.contains('.') {
            return vec![parts.to_vec()];
        }

        // Resolve the instance.
        let Some(inst_id) = find_instance_by_name_in_context(netlist, context, instance_name)
        else {
            return vec![parts.to_vec()];
        };
        let Some(inst) = netlist.instances.get(inst_id) else {
            return vec![parts.to_vec()];
        };
        let Some(module) = netlist.modules.get(inst.definition) else {
            return vec![parts.to_vec()];
        };

        // Collect this instance's pin names — both real pins and
        // interface-field bindings (the latter live as attributes
        // under the INTERFACE_FIELD_BINDING_ATTR_PREFIX).
        let mut pin_names: Vec<String> = module
            .pins
            .iter()
            .filter_map(|pid| netlist.pins.get(*pid).map(|p| p.name.clone()))
            .collect();
        for (k, _) in &module.attributes {
            if let Some(dotted) = k.strip_prefix(INTERFACE_FIELD_BINDING_ATTR_PREFIX) {
                pin_names.push(dotted.to_string());
            }
        }

        // If a pin is named exactly `field_name`, this is a single
        // pin reference, not a bundle.
        if pin_names.iter().any(|n| n == field_name) {
            return vec![parts.to_vec()];
        }

        // Find sibling signals: pin names that look like `field.X`.
        let prefix = format!("{}.", field_name);
        let mut signals: Vec<String> = pin_names
            .iter()
            .filter_map(|n| n.strip_prefix(&prefix).map(|s| s.to_string()))
            .collect();
        if signals.is_empty() {
            // No siblings either — this is just an unresolved reference;
            // hand off to process_flow_parts as-is.
            return vec![parts.to_vec()];
        }
        signals.sort();
        signals.dedup();
        bundle_info.push((instance_name.to_string(), field_name.to_string(), signals));
    }

    // v0.7c: signal sets may differ across endpoints when an
    // interface's `wires { }` block declares cross-name pairings
    // (UART dte.TX ↔ dce.RX). We pick the driver perspective —
    // the first endpoint — and translate signal names for every
    // subsequent endpoint via that endpoint's `intf_xwire__field__sig`
    // attribute, falling back to the same name (the SPI/I2C
    // default).
    let driver_signals = bundle_info[0].2.clone();

    let mut chains = Vec::with_capacity(driver_signals.len());
    for sig in &driver_signals {
        let mut chain = Vec::with_capacity(bundle_info.len());
        // Driver endpoint uses the signal as-is.
        chain.push(format!("{}.{}.{}", bundle_info[0].0, bundle_info[0].1, sig));
        // Subsequent endpoints translate via their own xwire map.
        for (inst, field, other_sigs) in &bundle_info[1..] {
            let translated = translate_via_xwire(netlist, context, inst, field, sig);
            // The translated name must exist on the other endpoint's
            // pin/binding set — otherwise we're trying to pair with a
            // signal the other side doesn't carry. Bail to the
            // unexpanded form in that case (the connection processor
            // will then surface a clear "no pin" error).
            if !other_sigs.iter().any(|s| s == &translated) {
                return vec![parts.to_vec()];
            }
            chain.push(format!("{}.{}.{}", inst, field, translated));
        }
        chains.push(chain);
    }
    chains
}

/// Translate `sig` (a signal name on the driver endpoint) into the
/// corresponding signal name on `instance.field` per its module's
/// recorded cross-wire mapping. Returns `sig` unchanged when no
/// mapping is recorded — the by-name default that's correct for
/// SPI / I²C / USB.
fn translate_via_xwire(
    netlist: &Netlist,
    context: &HierarchicalContext,
    instance_name: &str,
    field_name: &str,
    sig: &str,
) -> String {
    let Some(inst_id) = find_instance_by_name_in_context(netlist, context, instance_name)
    else { return sig.to_string(); };
    let Some(inst) = netlist.instances.get(inst_id) else { return sig.to_string(); };
    let Some(module) = netlist.modules.get(inst.definition) else { return sig.to_string(); };
    let key = format!(
        "{}{}__{}",
        INTERFACE_FIELD_XWIRE_ATTR_PREFIX, field_name, sig,
    );
    module.attributes.get(&key).cloned().unwrap_or_else(|| sig.to_string())
}

/// Shared flow processing logic for CONNECTION_STMT handlers.
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

                    // `->` uniformly MERGES its two endpoints: keep `last_net_id`
                    // on the net we just connected the named pin to, so a
                    // following endpoint joins the SAME net (no threading
                    // *through* the component). `inst.a -> inst.b` therefore
                    // ties the two pins — a deliberate gate→source / diode-
                    // connect tie, or, for a 2-pin passive, a short GLACIER
                    // flags. A SERIES element uses the explicit split form
                    // (`… -> inst.a; inst.b -> …`), landing the second pin on a
                    // fresh net in the next statement.
                    last_was_component_pin = true;
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
    // v0.4 interface-field binding alias resolution: if `pin_name`
    // is a dotted form (`spi.MOSI`) that was registered as an alias
    // for a physical pin (`PB3`), translate it before looking up
    // the pin instance.
    let pin_name = resolve_field_binding_alias(netlist, inst_id, pin_name)
        .map(|s| s.to_string())
        .unwrap_or_else(|| pin_name.to_string());
    let pin_name = pin_name.as_str();

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

/// Extract the ordered flow endpoints of a connection / net-flow statement
/// directly from the syntax tree, splitting at the lexer's `ARROW` /
/// `BI_ARROW` tokens rather than re-scanning the statement's source text.
///
/// This replaces the former char-level `parse_connection_chain` scanner
/// *and* the ad-hoc `find(':')` / `find(" for ")` / `strip_suffix(';')`
/// text munging at the call sites — both are forms of re-parsing already-
/// parsed structure. Walking tokens is robust to anything the lexer already
/// disambiguates (arrows inside string arguments, nested parens, etc.):
///   - accumulation stops at a `for` / `where` clause keyword or the
///     terminating `;`, so intent / where clauses never leak into the last
///     endpoint.
///
/// Endpoints are returned with their original inner spacing trimmed, e.g.
/// `["@VIN", "c_in: Cap(22uF).1"]` — the same shape `process_flow_parts`
/// already consumes.
fn extract_flow_chain_ast(node: &SyntaxNode<BhdlLanguage>) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    // CONNECTION_STMT has no statement prefix to skip.
    let mut prefix_done = true;

    let flush = |parts: &mut Vec<String>, current: &mut String| {
        let trimmed = current.trim();
        if !trimmed.is_empty() {
            parts.push(trimmed.to_string());
        }
        current.clear();
    };

    // Pre-order traversal yields tokens in source order, including those
    // nested in `BINARY_EXPR` / inline `COMPONENT_INST` children, so a
    // left- or right-nested arrow chain flattens to the correct sequence.
    for element in node.descendants_with_tokens() {
        let token = match element.as_token() {
            Some(t) => t,
            None => continue,
        };
        let kind = token.kind();
        if !prefix_done {
            if kind == SyntaxKind::COLON {
                prefix_done = true;
            }
            continue;
        }
        match kind {
            SyntaxKind::ARROW | SyntaxKind::BI_ARROW => flush(&mut parts, &mut current),
            SyntaxKind::FOR_KW | SyntaxKind::WHERE_KW | SyntaxKind::SEMI => break,
            _ => current.push_str(token.text()),
        }
    }
    flush(&mut parts, &mut current);
    parts
}