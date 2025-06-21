//! Hierarchical connectivity extraction for BHDL synthesizer
//! 
//! This module handles the extraction of connectivity information from
//! hierarchical module designs, including:
//! - Module definitions and their internal connections
//! - Module instances and port mappings
//! - Hierarchical net resolution
//! - SPICE subcircuit generation

use anyhow::Result;
use std::collections::HashMap;
use bhdl_ast::{AstNode, SyntaxKind, Module, ModuleInst, ConnectionStmt, HasName, BinaryExpr};
use bhdl_analyzer::types::AnalysisResult;
use bhdl_netlist::{Netlist, ModuleId, InstanceId, NetId, ConnectionPoint};
use bhdl_parser::BhdlLanguage;
use rowan::SyntaxNode;
use log::{debug, info, warn};
use crate::module_variants::ModuleVariantManager;

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
    /// Module variant manager for deduplication
    variant_manager: ModuleVariantManager,
    /// Component counters for reference designator generation
    component_counters: HashMap<String, usize>,
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
            variant_manager: ModuleVariantManager::new(),
            component_counters: HashMap::new(),
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
    
    /// Resolve a net name in the current context
    pub fn resolve_net(&mut self, net_name: &str, netlist: &mut Netlist) -> Result<NetId> {
        // Check if we're in a module context
        let in_module = self.current_module().is_some();
        
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
            // Top level net
            let net_id = netlist.add_net(Some(net_name.to_string()));
            Ok(net_id)
        }
    }
}

/// Extract hierarchical connectivity from AST
pub fn extract_hierarchical_connectivity(
    ast: &bhdl_ast::SourceFile,
    analysis: &AnalysisResult,
    netlist: &mut Netlist,
) -> Result<()> {
    info!("Extracting hierarchical connectivity from AST");
    
    let mut context = HierarchicalContext::new();
    
    // First pass: Create module definitions
    create_module_definitions(ast, analysis, netlist, &mut context)?;
    
    // Second pass: Process module instances and connections
    process_module_hierarchy(ast, analysis, netlist, &mut context)?;
    
    info!("Hierarchical connectivity extraction complete");
    Ok(())
}

/// Create module definitions from AST
fn create_module_definitions(
    ast: &bhdl_ast::SourceFile,
    _analysis: &AnalysisResult,
    netlist: &mut Netlist,
    context: &mut HierarchicalContext,
) -> Result<()> {
    use bhdl_ast::source_file::Item;
    use bhdl_netlist::types::ModuleKind;
    
    debug!("Creating module definitions");
    
    for item in ast.items() {
        match item {
            Item::Module(module) => {
                // Register module definition with variant manager
                context.variant_manager.register_module_definition(&module);
                
                // Note: We don't create module definitions here anymore
                // They will be created on-demand when instances are processed
                if let Some(name) = module.name() {
                    debug!("Registered module definition: {}", name.text());
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
) -> Result<()> {
    use bhdl_ast::source_file::Item;
    
    debug!("Processing module hierarchy");
    
    for item in ast.items() {
        match item {
            Item::Module(module) => {
                if let Some(name) = module.name() {
                    let module_name = name.text().to_string();
                    if let Some(&module_id) = context.module_name_to_id.get(&module_name) {
                        context.push_module(module_name, module_id);
                        process_module_body(&module, analysis, netlist, context)?;
                        context.pop_module();
                    }
                }
            }
            Item::Board(board) => {
                if let Some(name) = board.name() {
                    let board_name = name.text().to_string();
                    if let Some(&module_id) = context.module_name_to_id.get(&board_name) {
                        context.push_module(board_name, module_id);
                        process_board_body(&board, analysis, netlist, context)?;
                        context.pop_module();
                    }
                }
            }
            _ => {}
        }
    }
    
    Ok(())
}

/// Process module body for instances and connections
fn process_module_body(
    module: &Module,
    analysis: &AnalysisResult,
    netlist: &mut Netlist,
    context: &mut HierarchicalContext,
) -> Result<()> {
    // Process module instances
    for module_inst in module.module_instances() {
        process_module_instance(&module_inst, netlist, context, analysis)?;
    }
    
    // Process connections and extract component instances
    for child in module.syntax().children() {
        if child.kind() == SyntaxKind::CONNECTION_STMT {
            if let Some(conn_stmt) = ConnectionStmt::cast(child) {
                // Process the connection
                process_connection_in_module(&conn_stmt, netlist, context)?;
                
                // Extract and create component instances from the connection
                extract_component_instances_from_connection(&conn_stmt, netlist, context)?;
            }
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
) -> Result<()> {
    // Process module instances
    for module_inst in board.module_instances() {
        process_module_instance(&module_inst, netlist, context, analysis)?;
    }
    
    // Process connections and extract component instances
    for child in board.syntax().children() {
        if child.kind() == SyntaxKind::CONNECTION_STMT {
            if let Some(conn_stmt) = ConnectionStmt::cast(child) {
                // Process the connection
                process_connection_in_module(&conn_stmt, netlist, context)?;
                
                // Extract and create component instances from the connection
                extract_component_instances_from_connection(&conn_stmt, netlist, context)?;
            }
        }
    }
    
    Ok(())
}

/// Process a module instance
fn process_module_instance(
    module_inst: &ModuleInst,
    netlist: &mut Netlist,
    context: &mut HierarchicalContext,
    analysis: &AnalysisResult,
) -> Result<()> {
    let instance_name = module_inst.name()
        .map(|t| t.text().to_string())
        .ok_or_else(|| anyhow::anyhow!("Module instance missing name"))?;
    
    let module_type = module_inst.module_type()
        .map(|t| t.text().to_string())
        .ok_or_else(|| anyhow::anyhow!("Module instance missing type"))?;
    
    // Get or create module variant based on parameters
    let module_id = context.variant_manager.get_or_create_variant(
        module_inst,
        netlist,
        analysis
    )?;
    
    // Create instance
    let instance_id = netlist.add_instance(instance_name.clone(), module_id)
        .ok_or_else(|| anyhow::anyhow!("Failed to add instance"))?;
    
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
    
    // Process port mappings
    for port_mapping in module_inst.port_mappings() {
        process_port_mapping(&port_mapping, instance_id, netlist, context)?;
    }
    
    debug!("Created module instance: {} of type {}", instance_name, module_type);
    
    // Now process the module's internal components
    // First, check if we have the module definition
    let has_module_def = context.variant_manager.find_module_definition(&module_type).is_some();
    
    if has_module_def {
        // Push the instance context
        context.push_module(instance_name.clone(), module_id);
        
        // Clone the module definition to avoid borrow issues
        let module_def = context.variant_manager.find_module_definition(&module_type)
            .unwrap()
            .clone();
        
        process_module_body(&module_def, analysis, netlist, context)?;
        
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
        
        // For now, create nets for both endpoints if they don't exist
        let left_net = context.resolve_net(&left_text, netlist)?;
        let right_net = context.resolve_net(&right_text, netlist)?;
        
        // If they're different nets, we should merge them
        // For now, just log it
        if left_net != right_net {
            debug!("Connection creates alias between nets {} and {}", left_text, right_text);
        }
        
        debug!("Processed connection: {} -> {}", left_text, right_text);
        }
    }
    
    Ok(())
}

/// Extract component instances from a connection statement
fn extract_component_instances_from_connection(
    conn_stmt: &ConnectionStmt,
    netlist: &mut Netlist,
    context: &mut HierarchicalContext,
) -> Result<()> {
    // Extract the connection expression
    if let Some(expr_node) = conn_stmt.expr() {
        // Walk the expression tree to find component instantiations
        extract_components_from_node(&expr_node, netlist, context)?;
    }
    
    Ok(())
}

/// Recursively extract component instances from an AST node
fn extract_components_from_node(
    node: &SyntaxNode<BhdlLanguage>,
    netlist: &mut Netlist,
    context: &mut HierarchicalContext,
) -> Result<()> {
    use bhdl_ast::SyntaxKind;
    
    // Check if this node is a component instantiation
    if node.kind() == SyntaxKind::COMPONENT_INST {
        if let Some(comp_inst) = bhdl_ast::common::ComponentInst::cast(node.clone()) {
            create_component_instance(&comp_inst, netlist, context)?;
        }
    }
    
    // Recursively process children
    for child in node.children() {
        extract_components_from_node(&child, netlist, context)?;
    }
    
    Ok(())
}

/// Create a component instance in the current module context
fn create_component_instance(
    comp_inst: &bhdl_ast::common::ComponentInst,
    netlist: &mut Netlist,
    context: &mut HierarchicalContext,
) -> Result<()> {
    use bhdl_netlist::ModuleKind;
    
    // Get component type
    let component_type = comp_inst.component_type_name()
        .map(|t| t.text().to_string())
        .ok_or_else(|| anyhow::anyhow!("Component instance missing type"))?;
    
    // Generate hierarchical reference designator
    let base_refdes = get_component_refdes_prefix(&component_type);
    let counter_key = if context.current_path.is_empty() {
        base_refdes.clone()
    } else {
        format!("{}.{}", context.current_path_string(), base_refdes)
    };
    
    // Get or increment counter for this component type in this context
    let counter = context.component_counters
        .entry(counter_key)
        .or_insert(0);
    *counter += 1;
    
    // Generate instance name
    let local_name = format!("{}{}", base_refdes, counter);
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
    
    // Create or get the component module
    let module_id = get_or_create_component_module(&component_type, netlist)?;
    
    // Create the instance
    let instance_id = netlist.add_instance(instance_name.clone(), module_id)
        .ok_or_else(|| anyhow::anyhow!("Failed to add component instance"))?;
    
    // Create pin instances
    netlist.create_pin_instances(instance_id)
        .map_err(|e| anyhow::anyhow!("Failed to create pin instances: {}", e))?;
    
    debug!("Created component instance: {} of type {}", instance_name, component_type);
    
    Ok(())
}

/// Get or create a module for a component type
fn get_or_create_component_module(
    component_type: &str,
    netlist: &mut Netlist,
) -> Result<ModuleId> {
    use bhdl_netlist::ModuleKind;
    
    // Check if module already exists
    for (module_id, module) in &netlist.modules {
        if module.name == component_type && module.kind == ModuleKind::PhysicalComponent {
            return Ok(module_id);
        }
    }
    
    // Create new component module
    let module_id = netlist.add_module(component_type.to_string(), ModuleKind::PhysicalComponent);
    
    // Add standard pins based on component type
    add_component_pins(component_type, module_id, netlist)?;
    
    Ok(module_id)
}

/// Add pins to a component module based on its type
fn add_component_pins(
    component_type: &str,
    module_id: ModuleId,
    netlist: &mut Netlist,
) -> Result<()> {
    use bhdl_netlist::{PinDirection, PinType, PortDirection};
    
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
            // Default: add two generic pins
            netlist.add_port(module_id, "1".to_string(), PortDirection::InOut, None);
            netlist.add_port(module_id, "2".to_string(), PortDirection::InOut, None);
            netlist.add_pin(module_id, "1".to_string(), PinDirection::InOut, PinType::Signal);
            netlist.add_pin(module_id, "2".to_string(), PinDirection::InOut, PinType::Signal);
        }
    }
    
    Ok(())
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