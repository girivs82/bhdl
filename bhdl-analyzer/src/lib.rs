// Needed for analyze function signature and AST traversal
use bhdl_ast::SourceFile;
use rowan::ast::AstNode; // For source_file.syntax()

// Declare modules
pub mod types;
mod helpers;
pub mod symbol_table;
pub mod scope_registry;
pub mod hierarchical_symbol_table;
pub mod net_attributes;
mod pass1;
mod pass2;
mod pass3;
mod pass4;
pub mod power_analysis;
pub mod component_inference;
pub mod power_sequencing;
pub mod component_library;
pub mod attribute_extraction;
pub mod spice_extraction;
pub mod spice_integration;
pub mod spice_synthesis;
pub mod analysis_data_conversion;
pub mod attribute_analysis;
pub mod builtin_variables;
pub mod expression_evaluator;
pub mod flow_tracking;
pub mod unified_simulation;
pub mod documentation;

// Safety analysis module
pub mod passes;

// Use items needed directly in the analyze function
use pass2::{visit_node_pass2_references, Pass2Context};
use pass3::{visit_node_pass3_const_eval, Pass3Context};
use pass4::{visit_node_pass4_bounds_checks, Pass4Context};
use power_analysis::{analyze_power_domains, PowerAnalysisContext};
use component_inference::ComponentInferenceContext;
use power_sequencing::PowerSequenceGenerator;
use attribute_analysis::AttributeAnalyzer;
use flow_tracking::FlowTracker;
use bhdl_common::IntentRegistry;
use bhdl_stdlib::intents as stdlib_intents;

// Re-export key types for public use
pub use types::{AnalysisResult, Diagnostic, ResolvedConstants};

// Main analysis function
pub fn analyze(source_file: &SourceFile) -> AnalysisResult {
    analyze_with_base_path(source_file, std::path::Path::new("."))
}

// Analysis function with base path for import resolution
pub fn analyze_with_base_path(source_file: &SourceFile, base_path: &std::path::Path) -> AnalysisResult {
    println!("Starting analysis...");

    // Result accumulator
    let mut diagnostics = Vec::new();
    // Initialize resolved_constants using the type alias from the types module
    let mut resolved_constants = ResolvedConstants::new();

    // Pass 1: Build scope registry with base path for imports
    let (mut scope_registry, alias_specializations, imported_expansion_recipes, imported_symbol_definitions, imported_layout_definitions, imported_placement_recipes) = pass1::build_scope_registry_with_base(source_file, base_path);
    // Extract legacy data structures for backward compatibility with existing passes
    let global_scope = scope_registry.extract_global_scope();
    let definition_scopes = scope_registry.extract_definition_scopes();
    println!("Analyzer: Pass 1 complete. Global symbols: {}, Definition scopes: {}, Total scopes: {}",
             global_scope.get_symbols().len(),
             definition_scopes.len(),
             scope_registry.len());

    // Pass 1.25: Build Early Component Instance Registry (Phase 2: Scalability)
    println!("Analyzer: Starting Pass 1.25 - Component Instance Registry...");
    let instance_registry = passes::build_instance_registry(source_file);
    println!("Analyzer: Pass 1.25 complete. Registered {} component instances", instance_registry.len());

    // Pass 1.5: Power Domain Expansion (Phase 1: Scalability)
    println!("Analyzer: Starting Pass 1.5 - Power Domain Expansion...");
    let power_domain_expansion = passes::expand_power_domains(source_file, &instance_registry);
    let connections_count = power_domain_expansion.connections.len();
    let decoupling_caps_count = power_domain_expansion.decoupling_caps.len();
    let expansion_diags_count = power_domain_expansion.diagnostics.len();
    println!("Analyzer: Pass 1.5 complete. Connections expanded: {}, Decoupling capacitors generated: {}, Diagnostics: {}",
             connections_count, decoupling_caps_count, expansion_diags_count);

    // Add power domain expansion diagnostics to the accumulator
    diagnostics.extend(power_domain_expansion.diagnostics.clone());

    // Create built-in variable manager for behavioral modeling
    let builtin_manager = builtin_variables::BuiltinVariableManager::new();

    // Pass 2: Reference Checks
    println!("Analyzer: Starting Pass 2 - References & Basic Types...");
    let mut pass2_context = Pass2Context::new(&scope_registry, source_file.syntax(), &mut diagnostics, &builtin_manager);
    visit_node_pass2_references(source_file.syntax(), &mut pass2_context);
    println!("Analyzer: Pass 2 complete. Diagnostics found so far: {}", diagnostics.len());

    // Pass 3: Constant Evaluation (moved before Pass 2.5 so monomorphization has resolved constants)
    println!("Analyzer: Starting Pass 3 - Constant Evaluation...");
    let diag_count_before_pass3 = diagnostics.len();
    let mut pass3_context = Pass3Context::new(
        &scope_registry,
        source_file.syntax(),
        &mut resolved_constants,
        &mut diagnostics,
    );
    visit_node_pass3_const_eval(source_file.syntax(), &mut pass3_context);
    let pass3_diag_count = diagnostics.len() - diag_count_before_pass3;
    println!("Analyzer: Pass 3 complete. Constants evaluated: {}, Diagnostics added in pass: {}",
             resolved_constants.len(), pass3_diag_count);

    // Pass 2.5: Monomorphization (after Pass 3 so type args like 5V are resolved)
    println!("Analyzer: Starting Pass 2.5 - Monomorphization...");
    let mono_result = passes::run_monomorphization(&scope_registry, &resolved_constants, alias_specializations);
    if !mono_result.specializations.is_empty() {
        passes::register_specializations(&mut scope_registry, &mono_result);
    }
    diagnostics.extend(mono_result.diagnostics.clone());
    println!("Analyzer: Pass 2.5 complete. Specializations: {}, Iterations: {}",
             mono_result.specializations.len(), mono_result.iterations);


    // Pass 4: Bounds Checks
    println!("Analyzer: Starting Pass 4 - Bounds Checks...");
    // Pass diagnostics vec mutably
    let diag_count_before_pass4 = diagnostics.len(); // Get length BEFORE creating context
    let mut pass4_context = Pass4Context::new(
        &scope_registry,
        &resolved_constants, // Pass constants immutably
        &mut diagnostics, // Pass cumulative diagnostics vec
    );
    visit_node_pass4_bounds_checks(source_file.syntax(), &mut pass4_context);
    let pass4_diag_count = diagnostics.len() - diag_count_before_pass4;
    println!("Analyzer: Pass 4 complete. Diagnostics added in pass: {}", pass4_diag_count);

    // Pass 5: Power Analysis
    println!("Analyzer: Starting Pass 5 - Power Analysis...");
    let power_context = analyze_power_domains(source_file.syntax(), &global_scope, &definition_scopes);
    let power_errors_count = power_context.errors.len();
    let power_warnings_count = power_context.warnings.len();
    println!("Analyzer: Pass 5 complete. Power domains analyzed: {}, Level shifters: {}, Errors: {}, Warnings: {}", 
             power_context.domains.len(), 
             power_context.level_shifted_signals.len(),
             power_errors_count,
             power_warnings_count);

    // Pass 6: Component Inference
    println!("Analyzer: Starting Pass 6 - Component Inference...");
    let mut component_inference = ComponentInferenceContext::new();
    
    // Initialize module resolver for component library support
    let mut resolver = component_library::ModuleResolver::new();
    // Initialize with standard library
    if resolver.init_stdlib().is_ok() {
        component_inference.set_module_resolver(resolver);
        println!("Analyzer: Component library resolver initialized");
    } else {
        println!("Analyzer: Warning - Could not initialize component library");
    }
    
    // Debug: Print available power domains
    println!("DEBUG: Available power domains for component inference:");
    for (name, domain) in &power_context.domains {
        println!("  - {}: {}V @ {}A", name, domain.voltage, domain.max_current);
    }
    println!("DEBUG: Component domain assignments:");
    for (comp, domain) in &power_context.component_domains {
        println!("  - {} -> {}", comp, domain);
    }
    
    analyze_components_for_inference(source_file.syntax(), &mut component_inference, &power_context);
    let inferred_components_count = component_inference.get_inferred_components().len();
    let inference_warnings_count = component_inference.warnings.len();
    println!("Analyzer: Pass 6 complete. Components inferred: {}, Warnings: {}", 
             inferred_components_count, inference_warnings_count);

    // Pass 6.5: SPICE Synthesis (for components with placeholder values)
    println!("Analyzer: Starting Pass 6.5 - SPICE Synthesis...");
    let unresolved_count = component_inference.get_unresolved_components().len();
    if unresolved_count > 0 {
        println!("Analyzer: Found {} components needing SPICE resolution", unresolved_count);
        
        // Create SPICE synthesis engine
        let mut spice_synthesis = spice_synthesis::SpiceSynthesis::new();
        
        // Add unresolved components
        for unresolved in component_inference.get_unresolved_components() {
            spice_synthesis.add_component(unresolved.clone());
        }
        
        // Resolve component values
        match spice_synthesis.resolve_components() {
            Ok(resolution_report) => {
                println!("Analyzer: SPICE resolved {} component values", resolution_report.resolutions.len());
                
                // Convert resolutions to component suggestions
                let suggestions = resolution_report.to_component_suggestions();
                for suggestion in suggestions {
                    component_inference.add_component_suggestion(suggestion);
                }
                
                // Add any resolution errors to diagnostics
                for error in &resolution_report.errors {
                    diagnostics.push(types::Diagnostic::new(
                        format!("SPICE Resolution Error: {}", error),
                        rowan::TextRange::empty(rowan::TextSize::from(0)),
                    ));
                }
                
                // Add any resolution warnings to diagnostics
                for warning in &resolution_report.warnings {
                    diagnostics.push(types::Diagnostic::new(
                        format!("SPICE Resolution Warning: {}", warning),
                        rowan::TextRange::empty(rowan::TextSize::from(0)),
                    ));
                }
            }
            Err(e) => {
                println!("Analyzer: SPICE resolution failed: {}", e);
                diagnostics.push(types::Diagnostic::new(
                    format!("SPICE Resolution Failed: {}", e),
                    rowan::TextRange::empty(rowan::TextSize::from(0)),
                ));
            }
        }
    } else {
        println!("Analyzer: No components need SPICE resolution");
    }

    // Pass 7: Power Sequencing
    println!("Analyzer: Starting Pass 7 - Power Sequencing...");
    let mut power_sequencing = PowerSequenceGenerator::new();
    generate_power_sequences(&mut power_sequencing, &power_context);
    let startup_steps = power_sequencing.startup_sequence.len();
    let shutdown_steps = power_sequencing.shutdown_sequence.len();
    let sequencing_warnings = power_sequencing.warnings.len();
    println!("Analyzer: Pass 7 complete. Startup steps: {}, Shutdown steps: {}, Warnings: {}", 
             startup_steps, shutdown_steps, sequencing_warnings);
    
    // Pass 8: Attribute Analysis
    println!("Analyzer: Starting Pass 8 - Attribute Analysis...");
    let mut attribute_analyzer = AttributeAnalyzer::new();
    let attribute_analysis = attribute_analyzer.analyze(source_file.syntax());
    let attribute_count = attribute_analysis.attributes.len();
    let circular_deps_count = attribute_analysis.circular_dependencies.len();
    println!("Analyzer: Pass 8 complete. Attributes found: {}, Circular dependencies: {}", 
             attribute_count, circular_deps_count);
    
    // Add diagnostics for circular attribute dependencies
    for cycle in &attribute_analysis.circular_dependencies {
        diagnostics.push(types::Diagnostic::new(
            format!("Circular attribute dependency: {}", cycle.join(" -> ")),
            rowan::TextRange::empty(rowan::TextSize::from(0)),
        ));
    }

    // Pass 8.5: Expansion Recipe + Symbol/Layout Extraction
    // Merge recipes from imported files (Pass 1) with recipes from main source file
    println!("Analyzer: Starting Pass 8.5 - Expansion Recipe + Symbol/Layout Extraction...");
    let mut expansion_recipes = imported_expansion_recipes;
    let main_file_recipes = extract_expansion_recipes(source_file);
    expansion_recipes.extend(main_file_recipes);
    let expansion_count = expansion_recipes.len();

    // Extract symbol and layout definitions (from imported files + main file)
    let mut symbol_definitions = imported_symbol_definitions;
    let main_file_symbols = extract_symbol_definitions(source_file);
    symbol_definitions.extend(main_file_symbols);

    let mut layout_definitions = imported_layout_definitions;
    let main_file_layouts = extract_layout_definitions(source_file);
    layout_definitions.extend(main_file_layouts);

    // Merge placement recipes from imported files + main source file
    let mut placement_recipes = imported_placement_recipes;
    let main_file_placements = extract_placement_recipes(source_file);
    placement_recipes.extend(main_file_placements);

    println!("Analyzer: Pass 8.5 complete. Expansion recipes: {}, Placement recipes: {}, Symbol defs: {}, Layout defs: {}",
        expansion_count, placement_recipes.len(), symbol_definitions.len(), layout_definitions.len());

    // Pass 9: Flow Tracking and Intent Resolution
    println!("Analyzer: Starting Pass 9 - Flow Tracking and Intent Resolution...");
    let mut intent_registry = IntentRegistry::new();
    stdlib_intents::register_stdlib_intents(&mut intent_registry);
    let mut flow_tracker = FlowTracker::new(intent_registry);
    
    // Process boards to find flow paths with intents
    let mut flow_tracker_opt = None;
    for item in source_file.items() {
        if let Some(board) = bhdl_ast::Board::cast(item.syntax().clone()) {
            let flow_diagnostics = flow_tracker.analyze_board_with_scopes(&board, &global_scope, &definition_scopes);
            diagnostics.extend(flow_diagnostics);
            flow_tracker_opt = Some(flow_tracker);
            break; // Process first board only for now
        }
    }
    
    if let Some(ref mut tracker) = flow_tracker_opt {
        // Analyze virtual pins and create intent-driven flows
        let virtual_pin_diagnostics = tracker.analyze_virtual_pins(&global_scope, &definition_scopes);
        diagnostics.extend(virtual_pin_diagnostics);
        
        // Propagate intents hierarchically through module instances
        tracker.propagate_hierarchical_intents(&global_scope, &definition_scopes);
        
        let flow_paths_count = tracker.get_flow_paths().len();
        let required_sim_mode = tracker.get_required_sim_mode();
        println!("Analyzer: Pass 9 complete. Flow paths tracked: {}, Required simulation mode: {:?}", 
                 flow_paths_count, required_sim_mode);
    } else {
        println!("Analyzer: Pass 9 complete. No boards found to track flows.");
    }

    // Pass 10: Unified Simulation (Run once, extract all data)
    println!("Analyzer: Starting Pass 10 - Unified Simulation...");
    let mut simulation_data = types::UnifiedSimulationData::new();
    
    // Only run simulation if we have components to analyze
    if inferred_components_count > 0 {
        use unified_simulation::UnifiedSimulationOrchestrator;
        let orchestrator = UnifiedSimulationOrchestrator::new();
        
        // Create a placeholder netlist for simulation if none exists
        let placeholder_netlist = bhdl_netlist::Netlist::new();
        
        match orchestrator.run_unified_simulation(&placeholder_netlist, &component_inference) {
            Ok(sim_data) => {
                simulation_data = sim_data;
                let engines_count = simulation_data.simulation_metadata.engines_used.len();
                let confidence = simulation_data.simulation_metadata.simulation_accuracy.confidence_level * 100.0;
                println!("Analyzer: Pass 10 complete. Simulation engines: {}, Confidence: {:.1}%", 
                         engines_count, confidence);
                         
                // Add simulation warnings to diagnostics
                for warning in &simulation_data.simulation_metadata.warnings {
                    diagnostics.push(types::Diagnostic::new(
                        format!("Simulation Warning: {}", warning),
                        rowan::TextRange::empty(rowan::TextSize::from(0)),
                    ));
                }
            }
            Err(e) => {
                println!("Analyzer: Pass 10 failed. Simulation error: {}", e);
                diagnostics.push(types::Diagnostic::new(
                    format!("Unified Simulation Failed: {}", e),
                    rowan::TextRange::empty(rowan::TextSize::from(0)),
                ));
            }
        }
    } else {
        println!("Analyzer: Pass 10 skipped. No components found for simulation.");
    }

    // Pass 11: Safety Analysis (ISO 26262 compliance tracking)
    println!("Analyzer: Starting Pass 11 - Safety Analysis...");
    let safety_analysis = passes::analyze_safety(&source_file);
    let safety_reqs_count = safety_analysis.requirements.len();
    let safety_coverage = safety_analysis.coverage.coverage_percentage;
    println!("Analyzer: Pass 11 complete. Safety requirements: {}, Coverage: {:.1}%", 
             safety_reqs_count, safety_coverage);
    
    // Add safety analysis diagnostics
    for safety_diag in &safety_analysis.diagnostics {
        diagnostics.push(safety_diag.clone());
    }

    // Convert power analysis errors to diagnostics
    for error in &power_context.errors {
        diagnostics.push(types::Diagnostic::new(
            format!("Power Analysis: {}", error),
            rowan::TextRange::empty(rowan::TextSize::from(0)),
        ));
    }

    // Convert power analysis warnings to diagnostics
    for warning in &power_context.warnings {
        diagnostics.push(types::Diagnostic::new(
            format!("Power Warning: {}", warning),
            rowan::TextRange::empty(rowan::TextSize::from(0)),
        ));
    }

    // Convert component inference warnings to diagnostics
    for warning in &component_inference.warnings {
        diagnostics.push(types::Diagnostic::new(
            format!("Component Inference: {}", warning),
            rowan::TextRange::empty(rowan::TextSize::from(0)),
        ));
    }

    // Convert power sequencing warnings to diagnostics
    for warning in &power_sequencing.warnings {
        diagnostics.push(types::Diagnostic::new(
            format!("Power Sequencing: {}", warning),
            rowan::TextRange::empty(rowan::TextSize::from(0)),
        ));
    }


    println!("Analysis finished. Found {} total diagnostics.", diagnostics.len());

    AnalysisResult {
        global_scope, // Move ownership
        definition_scopes, // Move ownership
        scope_registry, // Move ownership
        diagnostics, // Move ownership
        resolved_constants, // Move ownership
        power_analysis: power_context, // Move ownership
        component_inference, // Move ownership
        power_sequencing, // Move ownership
        netlist: None, // Move ownership
        attribute_analysis, // Move ownership
        flow_tracker: flow_tracker_opt, // Move ownership
        safety_analysis, // Move ownership
        simulation_data, // Move ownership
        instance_registry, // Move ownership (Phase 2: Pass 1.25)
        power_domain_expansion, // Move ownership (Phase 1: Scalability)
        monomorphization: mono_result, // Move ownership (Pass 2.5)
        expansion_recipes, // Move ownership (Pass 8.5)
        placement_recipes, // Move ownership (Pass 8.5)
        symbol_definitions, // Move ownership (Pass 8.5)
        layout_definitions, // Move ownership (Pass 8.5)
    }
}


/// Analyze components for inference based on circuit context
fn analyze_components_for_inference(
    syntax: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
    component_inference: &mut ComponentInferenceContext,
    power_context: &PowerAnalysisContext,
) {
    
    

    // Walk through the syntax tree looking for component instantiations
    visit_node_for_component_inference(syntax, component_inference, power_context);
}

/// Visit nodes for component inference
fn visit_node_for_component_inference(
    node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
    component_inference: &mut ComponentInferenceContext,
    power_context: &PowerAnalysisContext,
) {
    use bhdl_ast::SyntaxKind;
    use component_inference::CircuitContext;

    match node.kind() {
        bhdl_ast::SyntaxKind::FLOW_EXPR => {
            // Process flow expressions which contain inline component instantiations in v2.0
            use bhdl_ast::flow::{FlowExpr, FlowElement};
            
            println!("DEBUG: Found FLOW_EXPR node");
            if let Some(flow_expr) = FlowExpr::cast(node.clone()) {
                println!("DEBUG: Successfully cast to FlowExpr");
                // Process each element in the flow expression
                for element in flow_expr.elements() {
                    match &element {
                        FlowElement::ComponentInstantiation(comp_inst) => {
                            println!("DEBUG: Found ComponentInstantiation in flow");
                            process_component_instantiation_v2(&comp_inst, component_inference, power_context);
                        }
                        FlowElement::Identifier(token) => {
                            println!("DEBUG: Found Identifier in flow: {}", token.text());
                        }
                        FlowElement::ConditionalExpr(_) => {
                            println!("DEBUG: Found ConditionalExpr in flow");
                        }
                    }
                }
            } else {
                println!("DEBUG: Failed to cast FLOW_EXPR to FlowExpr");
            }
        }
        bhdl_ast::SyntaxKind::CONNECTION_STMT => {
            // Process connection statements which may contain inline component instantiations
            // e.g., VCC -> R1(10k).1 -> LED1(red).A -> GND;
            let _stmt_text = node.to_string();
            
            // Look for inline component instantiations in the connection
            // Pattern: ComponentType(params) or name: ComponentType(params)
            process_connection_for_components(node, component_inference, power_context);
            
            // Don't recursively call visit_node_for_component_inference here
            // The normal traversal will handle visiting child nodes
        }
        bhdl_ast::SyntaxKind::COMPONENT_INST => {
            // Check if this COMPONENT_INST is inside a CONNECTION_STMT
            // These are v2.0 inline instantiations like Res(10k).1
            let mut current = node.clone();
            let mut in_connection = false;
            while let Some(parent) = current.parent() {
                if parent.kind() == bhdl_ast::SyntaxKind::CONNECTION_STMT {
                    in_connection = true;
                    break;
                }
                current = parent;
            }
            
            if in_connection {
                // This is a v2.0 inline instantiation - extract component type directly
                // For Res(10k).1, the first IDENT token is "Res"
                let component_type = node.children_with_tokens()
                    .find_map(|child| {
                        if let rowan::NodeOrToken::Token(token) = child {
                            if token.kind() == bhdl_ast::SyntaxKind::IDENT {
                                Some(token.text().to_string())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    });
                
                if let Some(component_type) = component_type {
                    println!("DEBUG: Processing inline component: {}", component_type);
                    
                    // Process as v2.0 inline instantiation
                    use bhdl_ast::flow::ComponentInstantiation;
                    if let Some(comp_inst) = ComponentInstantiation::cast(node.clone()) {
                        process_component_instantiation_v2(&comp_inst, component_inference, power_context);
                        return;
                    }
                }
            }
            
            // Handle ComponentInst from common module (not flow module)
            use bhdl_ast::ComponentInst;
            
            if let Some(comp_inst) = ComponentInst::cast(node.clone()) {
                println!("DEBUG: Processing ComponentInst (common module)");
                process_component_inst_common(&comp_inst, component_inference, power_context);
                return; // Don't process children since we handled it
            }
            
            // Old code path - kept as fallback
            // Find the first non-whitespace token to get the component type
            let component_type = node.children_with_tokens()
                .find_map(|child| {
                    if let rowan::NodeOrToken::Token(token) = child {
                        if token.kind() == bhdl_ast::SyntaxKind::IDENT {
                            Some(token.text().to_string())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                });
            
            if let Some(component_type) = component_type {
                // Try to extract instance name from connection context
                let instance_name = extract_instance_name_from_context(node);
                println!("DEBUG: COMPONENT_INST '{}' with extracted instance name: {:?}", component_type, instance_name);
                
                // Try to get component instance name and its assigned power domain
                let supply_voltage = if let Some(inst_name) = get_component_instance_name(node) {
                    // Check if this component has been assigned to a power domain
                    power_context.component_domains.get(&inst_name)
                        .and_then(|domain_name| power_context.domains.get(domain_name))
                        .map(|domain| domain.voltage)
                        .or_else(|| {
                            // If not found by instance name, check by node text which might contain the component identifier
                            let node_text = node.to_string();
                            power_context.domains.values()
                                .find(|domain| node_text.contains(&domain.name))
                                .map(|domain| domain.voltage)
                        })
                        .or_else(|| {
                            // Look for VCC domain as default supply
                            power_context.domains.get("VCC")
                                .map(|domain| domain.voltage)
                        })
                        .or(Some(5.0)) // Default to 5V if no power domain found
                } else {
                    power_context.domains.get("VCC")
                        .map(|domain| domain.voltage)
                        .or(Some(5.0)) // Default to 5V
                };
                
                // Create requirements based on context and actual power domain
                let requirements = component_inference::CircuitRequirements {
                    supply_voltage,
                    load_current: None,
                    required_current: None,
                    frequency: None,
                    max_power: None,
                    temperature_range: None,
                    tolerance: None,
                    package_constraint: None,
                };

                // Determine circuit context
                let mut circuit_context = CircuitContext::default();
                
                // Extract explicit parameters from the component instantiation
                // Look for PARAM_ASSIGN_BLOCK which contains the parameters
                let mut explicit_params = std::collections::HashMap::new();
                if let Some(param_block) = node.children().find(|n| n.kind() == bhdl_ast::SyntaxKind::PARAM_ASSIGN_BLOCK) {
                    // Extract parameters from the block
                    for param_node in param_block.children() {
                        if param_node.kind() == SyntaxKind::PARAM_ASSIGN {
                            // For simple values like Res(330Ω), there's usually just a VALUE node
                            if let Some(value_node) = param_node.children().find(|n| n.kind() == bhdl_ast::SyntaxKind::VALUE) {
                                let value_text = value_node.text().to_string();
                                explicit_params.insert("value".to_string(), value_text);
                            }
                        }
                    }
                }
                
                if !explicit_params.is_empty() {
                    circuit_context.explicit_params = Some(explicit_params);
                }
                
                // Check if this is near an LED by looking at the connection context
                if let Some(connection_stmt) = node.ancestors().find(|n| n.kind() == bhdl_ast::SyntaxKind::CONNECTION_STMT) {
                    let connection_text = connection_stmt.to_string();
                    if connection_text.contains("LED") {
                        circuit_context.has_led_in_series = true;
                        // Try to extract LED color
                        if connection_text.contains("red") {
                            circuit_context.led_color = Some("red".to_string());
                        } else if connection_text.contains("green") {
                            circuit_context.led_color = Some("green".to_string());
                        } else if connection_text.contains("blue") {
                            circuit_context.led_color = Some("blue".to_string());
                        } else {
                            circuit_context.led_color = Some("red".to_string()); // Default
                        }
                    }
                }
                
                // Check for pull-up context - only if explicitly mentioned
                if node.to_string().contains("pull") {
                    circuit_context.is_pullup = true;
                }
                
                // Check for decoupling context using power analysis
                // A capacitor connected to a power domain is likely decoupling
                if component_type == "Cap" || component_type == "ElectrolyticCap" {
                    // Check if this component is connected to any power domain
                    // by looking at the connection context in the parent nodes
                    if let Some(connection_stmt) = node.ancestors().find(|n| n.kind() == bhdl_ast::SyntaxKind::CONNECTION_STMT) {
                        // Extract connected net names from the connection
                        let connection_text = connection_stmt.to_string();
                        
                        // Check if any power domain name appears in the connection
                        for (domain_name, _) in &power_context.domains {
                            if connection_text.contains(domain_name) {
                                circuit_context.is_decoupling = true;
                                break;
                            }
                        }
                    }
                }

                // Debug: Print voltage being used
                if let Some(voltage) = requirements.supply_voltage {
                    println!("DEBUG: Component '{}' using supply voltage: {}V", component_type, voltage);
                }
                
                // Infer component parameters
                if let Some(mut suggestion) = component_inference.infer_component_parameters(
                    &component_type, &requirements, &circuit_context
                ) {
                    // Set the instance name if extracted
                    if let Some(name) = instance_name {
                        suggestion.instance_name = Some(name);
                    }
                    component_inference.add_inferred_component(suggestion);
                } else {
                    println!("DEBUG: No suggestion returned for '{}'", component_type);
                }
            }
        }
        _ => {}
    }

    // Recursively visit children
    for child in node.children() {
        visit_node_for_component_inference(&child, component_inference, power_context);
    }
}

/// Process component instantiation from v2.0 flow syntax
fn process_component_instantiation_v2(
    comp_inst: &bhdl_ast::flow::ComponentInstantiation,
    component_inference: &mut ComponentInferenceContext,
    power_context: &PowerAnalysisContext,
) {
    use component_inference::{CircuitContext, ParameterValue, InferredParameter};
    use bhdl_ast::HasName;
    
    // Extract component type from the instantiation
    let component_type = comp_inst.component_type()
        .map(|t| t.text().to_string())
        .unwrap_or_else(|| "Unknown".to_string());
    
    // Check if this component has placeholder parameters (for SPICE generation)
    let mut has_placeholder = false;
    let mut placeholder_constraints = Vec::new();
    
    if let Some(param_block) = comp_inst.parameters() {
        if param_block.has_placeholder() {
            has_placeholder = true;
            println!("DEBUG: Component {} has placeholder parameters - marking for SPICE resolution", component_type);
            
            // Extract any constraints from the placeholder
            if let Some(placeholder) = param_block.placeholder() {
                for constraint in placeholder.constraints() {
                    if let (Some(name), Some(value)) = (constraint.name(), constraint.value()) {
                        placeholder_constraints.push((name.text().to_string(), value.syntax().text().to_string()));
                    }
                }
            }
        }
    }
    
    // Extract parameters from the instantiation
    let mut extracted_params = Vec::new();
    let mut parameter_overrides = std::collections::HashMap::new();
    if !has_placeholder {
        println!("DEBUG: Extracting normal parameters for {}", component_type);
        // Only extract normal parameters if there's no placeholder
        for param_assign in comp_inst.parameter_assignments() {
            if let Some(value) = param_assign.value() {
                // Get parameter name, or infer it for positional parameters based on component type
                let param_name = param_assign.name()
                    .map(|n| n.text().to_string())
                    .unwrap_or_else(|| {
                        // For positional parameters, infer name based on component type
                        match component_type.as_str() {
                            "Res" | "Resistor" => "value".to_string(),
                            "Cap" | "Capacitor" => "value".to_string(),
                            "LED" => "color".to_string(),
                            "Fuse" => "current_rating".to_string(),
                            "TVSDiode" => "voltage_rating".to_string(),
                            _ => "value".to_string(), // Default fallback
                        }
                    });
                    
                let param_value = value.syntax().text().to_string();
                
                // Store in parameter_overrides for interfaces
                parameter_overrides.insert(param_name.clone(), param_value.clone().trim_matches('"').to_string());
                
                // Parse parameter value based on name or content
                let parsed_value = if param_name == "package" {
                    ParameterValue::String(param_value.trim_matches('"').to_string())
                } else if param_value.chars().all(|c| c.is_digit(10) || c == '.') {
                    ParameterValue::Real(param_value.parse().unwrap_or(0.0))
                } else {
                    ParameterValue::String(param_value)
                };
                
                extracted_params.push(InferredParameter {
                    name: param_name,
                    value: parsed_value,
                    confidence: 1.0,  // User-specified parameters have 100% confidence
                    reasoning: "User-specified parameter".to_string(),
                });
            }
        }
    }
    
    // Try to extract instance name from the connection context
    // Look for patterns like "D1: LED(red)" in the parent connection statement
    let extracted_name = extract_instance_name_from_context(comp_inst.syntax());
    println!("DEBUG: Extracted instance name from context: {:?}", extracted_name);
    
    let instance_name = extracted_name.unwrap_or_else(|| {
            // If no explicit name found, generate one based on component type
            static mut COMPONENT_COUNTER: usize = 0;
            let instance_id = unsafe {
                COMPONENT_COUNTER += 1;
                COMPONENT_COUNTER
            };
            format!("{}{}", get_refdes_prefix(&component_type), instance_id)
        });
    
    println!("DEBUG: Processing v2.0 component instantiation: {} (type: {}) with {} parameters", 
             instance_name, component_type, extracted_params.len());
    
    // Get supply voltage from power context
    // First check if component is assigned to a domain
    let supply_voltage = power_context.component_domains.get(&instance_name)
        .and_then(|domain_name| power_context.domains.get(domain_name))
        .map(|domain| domain.voltage)
        .or_else(|| {
            // If not found, look for VCC domain as default supply
            power_context.domains.get("VCC")
                .map(|domain| domain.voltage)
        })
        .or(Some(5.0)); // Default to 5V
    
    // Check for package constraint in extracted parameters
    let package_constraint = extracted_params.iter()
        .find(|p| p.name == "package")
        .and_then(|p| match &p.value {
            ParameterValue::String(s) => Some(s.clone()),
            _ => None
        });
    
    // Create requirements
    let requirements = component_inference::CircuitRequirements {
        supply_voltage,
        load_current: None,
        required_current: None,
        frequency: None,
        max_power: None,
        temperature_range: None,
        tolerance: None,
        package_constraint,
    };
    
    // Determine circuit context by looking at the parent connection
    let mut circuit_context = CircuitContext::default();
    
    // Add explicit parameters to the circuit context
    if !extracted_params.is_empty() {
        println!("DEBUG: Component {} has {} extracted parameters", component_type, extracted_params.len());
        let mut explicit_params_map = std::collections::HashMap::new();
        for param in &extracted_params {
            let value_str = match &param.value {
                ParameterValue::Real(v) => v.to_string(),
                ParameterValue::String(s) => s.clone(),
                ParameterValue::Resistance(r) => format!("{}Ω", r),
                ParameterValue::Capacitance(c) => format!("{}F", c),
                ParameterValue::Inductance(l) => format!("{}H", l),
                ParameterValue::Voltage(v) => format!("{}V", v),
                ParameterValue::Current(i) => format!("{}A", i),
                ParameterValue::Power(p) => format!("{}W", p),
                ParameterValue::Frequency(f) => format!("{}Hz", f),
                ParameterValue::Integer(i) => i.to_string(),
                ParameterValue::Boolean(b) => b.to_string(),
            };
            println!("DEBUG: Adding param '{}' = '{}'", param.name, value_str);
            explicit_params_map.insert(param.name.clone(), value_str);
        }
        circuit_context.explicit_params = Some(explicit_params_map);
    }
    
    // Check for LED context
    if let Some(connection_stmt) = comp_inst.syntax().ancestors().find(|n| n.kind() == bhdl_ast::SyntaxKind::CONNECTION_STMT) {
        let connection_text = connection_stmt.to_string();
        if connection_text.contains("LED") {
            circuit_context.has_led_in_series = true;
            // Try to extract LED color from parameters first
            if let Some(color_param) = extracted_params.iter().find(|p| p.name == "color") {
                if let ParameterValue::String(color) = &color_param.value {
                    circuit_context.led_color = Some(color.clone());
                }
            } else {
                // Fallback to text search
                if connection_text.contains("red") {
                    circuit_context.led_color = Some("red".to_string());
                } else if connection_text.contains("green") {
                    circuit_context.led_color = Some("green".to_string());
                } else if connection_text.contains("blue") {
                    circuit_context.led_color = Some("blue".to_string());
                }
            }
        }
    }
    
    // Handle components differently based on whether they have placeholders
    if has_placeholder {
        // Component has placeholder parameters - mark for SPICE resolution
        use spice_synthesis::{UnresolvedComponent, ComponentConstraints, CircuitContext as SpiceContext, LEDSpec};
        
        // Create an unresolved component entry
        let mut constraints = ComponentConstraints::default();
        
        // Add any placeholder constraints
        for (name, value) in &placeholder_constraints {
            match name.as_str() {
                "rating" | "power" => {
                    // Parse power rating constraint
                    if let Ok(power) = parse_power_value(&value) {
                        constraints.power_rating = Some(power);
                    }
                }
                "tolerance" => {
                    // Parse tolerance constraint
                    if let Ok(tol) = parse_percentage_value(&value) {
                        constraints.tolerance = Some(tol);
                    }
                }
                _ => {}
            }
        }
        
        // Determine SPICE circuit context
        let spice_context = if circuit_context.has_led_in_series {
            // LED current limiting context
            SpiceContext::LEDCurrentLimit {
                led_name: format!("LED_{}", instance_name),
                led_spec: LEDSpec {
                    color: circuit_context.led_color.clone().unwrap_or_else(|| "red".to_string()),
                    forward_voltage: get_led_forward_voltage(&circuit_context.led_color),
                    target_current: 0.020, // 20mA default
                    max_current: 0.030,    // 30mA max
                },
                supply_voltage: supply_voltage.unwrap_or(5.0),
            }
        } else {
            SpiceContext::Unknown
        };
        
        // Mark this component as needing SPICE resolution
        component_inference.add_unresolved_component(UnresolvedComponent {
            instance_name: instance_name.clone(),
            component_type: component_type.clone(),
            ast_node: comp_inst.syntax().clone(),
            is_value_specified: false,
            specified_value: None,
            constraints,
            circuit_context: spice_context,
        });
        
        println!("DEBUG: Added unresolved component {} for SPICE resolution", instance_name);
    } else {
        // Normal inference for components with specified values
        if let Some(mut suggestion) = component_inference.infer_component_parameters(
            &component_type, &requirements, &circuit_context
        ) {
            // Set the instance name
            suggestion.instance_name = Some(instance_name.clone());
            
            // Add parameter overrides for interfaces
            suggestion.parameter_overrides = parameter_overrides.clone();
            
            // Add user-specified parameters to the suggestion
            for param in extracted_params {
                // Don't duplicate parameters that were already inferred
                if !suggestion.parameters.iter().any(|p| p.name == param.name) {
                    suggestion.parameters.push(param);
                }
            }
            
            component_inference.add_inferred_component(suggestion);
        }
    }
}

/// Process ComponentInst from common module (v2.0 inline syntax)
fn process_component_inst_common(
    comp_inst: &bhdl_ast::ComponentInst,
    component_inference: &mut ComponentInferenceContext,
    power_context: &PowerAnalysisContext,
) {
    use component_inference::{CircuitContext, ParameterValue, InferredParameter};
    use bhdl_ast::HasName;
    
    // Extract component type
    let component_type = comp_inst.component_type_name()
        .map(|t| t.text().to_string())
        .unwrap_or_else(|| "Unknown".to_string());
    
    println!("DEBUG: Processing component type: {}", component_type);
    
    // Check if this component has placeholder parameters (for SPICE generation)
    let mut has_placeholder = false;
    let mut placeholder_constraints = Vec::new();
    
    if let Some(param_block) = comp_inst.param_assign_block() {
        println!("DEBUG: Component {} has param block", component_type);
        if param_block.has_placeholder() {
            has_placeholder = true;
            println!("DEBUG: Component {} has placeholder parameters - marking for SPICE resolution", component_type);
            
            // Extract any constraints from the placeholder
            if let Some(placeholder) = param_block.placeholder() {
                for constraint in placeholder.constraints() {
                    if let (Some(name), Some(value)) = (constraint.name(), constraint.value()) {
                        placeholder_constraints.push((name.text().to_string(), value.syntax().text().to_string()));
                    }
                }
            }
        }
    }
    
    // Extract normal parameters if not a placeholder
    let mut extracted_params = Vec::new();
    let mut parameter_overrides = std::collections::HashMap::new();
    
    if !has_placeholder {
        // First check for param_list (used by interfaces)
        if let Some(param_list) = comp_inst.param_list() {
            println!("DEBUG: Extracting parameters from param list for {}", component_type);
            for param in param_list.params() {
                if let (Some(name), Some(value)) = (param.name(), param.value()) {
                    let param_name = name.text().to_string();
                    let param_value = value.syntax().text().to_string().trim_matches('"').to_string();
                    println!("DEBUG: Found param '{}' = '{}'", param_name, param_value);
                    
                    // Store in parameter_overrides for interfaces
                    parameter_overrides.insert(param_name.clone(), param_value.clone());
                    
                    // Also create InferredParameter for compatibility
                    extracted_params.push(InferredParameter {
                        name: param_name,
                        value: ParameterValue::String(param_value),
                        confidence: 1.0,
                        reasoning: "User-specified parameter".to_string(),
                    });
                }
            }
        }
        // Then check for param_assign_block (used by components)
        else if let Some(param_block) = comp_inst.param_assign_block() {
            println!("DEBUG: Extracting parameters from param block for {}", component_type);
            for param_assign in param_block.assignments() {
                if let Some(value) = param_assign.value() {
                    // Get parameter name, or use empty string for positional parameters
                    let param_name = param_assign.name()
                        .map(|n| n.text().to_string())
                        .unwrap_or_else(|| String::new());
                    
                    let param_value = value.syntax().text().to_string();
                    println!("DEBUG: Found param '{}' = '{}'", param_name, param_value);
                    
                    // Parse parameter value based on name or content
                    let parsed_value = if param_name == "package" {
                        ParameterValue::String(param_value.trim_matches('"').to_string())
                    } else if param_value.chars().all(|c| c.is_digit(10) || c == '.') {
                        ParameterValue::Real(param_value.parse().unwrap_or(0.0))
                    } else {
                        ParameterValue::String(param_value)
                    };
                    
                    extracted_params.push(InferredParameter {
                        name: param_name,
                        value: parsed_value,
                        confidence: 1.0,
                        reasoning: "User-specified parameter".to_string(),
                    });
                }
            }
        }
    }
    
    // Extract instance name
    let extracted_name = extract_instance_name_from_context(comp_inst.syntax());
    let instance_name = extracted_name.unwrap_or_else(|| {
        static mut COMPONENT_COUNTER: usize = 0;
        let instance_id = unsafe {
            COMPONENT_COUNTER += 1;
            COMPONENT_COUNTER
        };
        format!("{}{}", get_refdes_prefix(&component_type), instance_id)
    });
    
    println!("DEBUG: Component instance name: {}", instance_name);
    
    // Get supply voltage from power context
    // First check if component is assigned to a domain
    let supply_voltage = power_context.component_domains.get(&instance_name)
        .and_then(|domain_name| power_context.domains.get(domain_name))
        .map(|domain| domain.voltage)
        .or_else(|| {
            // If not found, look for VCC domain as default supply
            power_context.domains.get("VCC")
                .map(|domain| domain.voltage)
        })
        .or(Some(5.0)); // Default to 5V
    
    // Create requirements
    let requirements = component_inference::CircuitRequirements {
        supply_voltage,
        load_current: None,
        required_current: None,
        frequency: None,
        max_power: None,
        temperature_range: None,
        tolerance: None,
        package_constraint: None,
    };
    
    // Determine circuit context
    let mut circuit_context = CircuitContext::default();
    
    // Add explicit parameters to the circuit context
    if !extracted_params.is_empty() {
        println!("DEBUG: Component {} has {} extracted parameters", component_type, extracted_params.len());
        let mut explicit_params_map = std::collections::HashMap::new();
        for param in &extracted_params {
            let value_str = match &param.value {
                ParameterValue::Real(v) => v.to_string(),
                ParameterValue::String(s) => s.clone(),
                ParameterValue::Resistance(r) => format!("{}Ω", r),
                ParameterValue::Capacitance(c) => format!("{}F", c),
                ParameterValue::Inductance(l) => format!("{}H", l),
                ParameterValue::Voltage(v) => format!("{}V", v),
                ParameterValue::Current(i) => format!("{}A", i),
                ParameterValue::Power(p) => format!("{}W", p),
                ParameterValue::Frequency(f) => format!("{}Hz", f),
                ParameterValue::Integer(i) => i.to_string(),
                ParameterValue::Boolean(b) => b.to_string(),
            };
            println!("DEBUG: Adding param '{}' = '{}'", param.name, value_str);
            explicit_params_map.insert(param.name.clone(), value_str);
        }
        circuit_context.explicit_params = Some(explicit_params_map);
    }
    
    // Check for LED context
    if let Some(connection_stmt) = comp_inst.syntax().ancestors().find(|n| n.kind() == bhdl_ast::SyntaxKind::CONNECTION_STMT) {
        let connection_text = connection_stmt.to_string();
        if connection_text.contains("LED") {
            circuit_context.has_led_in_series = true;
            if connection_text.contains("red") {
                circuit_context.led_color = Some("red".to_string());
            } else if connection_text.contains("green") {
                circuit_context.led_color = Some("green".to_string());
            } else if connection_text.contains("blue") {
                circuit_context.led_color = Some("blue".to_string());
            }
        }
    }
    
    // Handle components based on placeholder status
    if has_placeholder {
        // Component has placeholder parameters - mark for SPICE resolution
        use spice_synthesis::{UnresolvedComponent, ComponentConstraints, CircuitContext as SpiceContext, LEDSpec};
        
        // Create constraints
        let mut constraints = ComponentConstraints::default();
        
        // Add any placeholder constraints
        for (name, value) in &placeholder_constraints {
            match name.as_str() {
                "rating" | "power" => {
                    if let Ok(power) = parse_power_value(&value) {
                        constraints.power_rating = Some(power);
                    }
                }
                "tolerance" => {
                    if let Ok(tol) = parse_percentage_value(&value) {
                        constraints.tolerance = Some(tol);
                    }
                }
                _ => {}
            }
        }
        
        // Determine SPICE circuit context
        let spice_context = if circuit_context.has_led_in_series {
            SpiceContext::LEDCurrentLimit {
                led_name: format!("LED_{}", instance_name),
                led_spec: LEDSpec {
                    color: circuit_context.led_color.clone().unwrap_or_else(|| "red".to_string()),
                    forward_voltage: get_led_forward_voltage(&circuit_context.led_color),
                    target_current: 0.020, // 20mA default
                    max_current: 0.030,    // 30mA max
                },
                supply_voltage: supply_voltage.unwrap_or(5.0),
            }
        } else {
            SpiceContext::Unknown
        };
        
        // Mark this component as needing SPICE resolution
        component_inference.add_unresolved_component(UnresolvedComponent {
            instance_name: instance_name.clone(),
            component_type: component_type.clone(),
            ast_node: comp_inst.syntax().clone(),
            is_value_specified: false,
            specified_value: None,
            constraints,
            circuit_context: spice_context,
        });
        
        println!("DEBUG: Added unresolved component {} for SPICE resolution", instance_name);
    } else {
        // Normal inference for components with specified values
        if let Some(mut suggestion) = component_inference.infer_component_parameters(
            &component_type, &requirements, &circuit_context
        ) {
            suggestion.instance_name = Some(instance_name.clone());
            
            // Add parameter overrides for interfaces
            suggestion.parameter_overrides = parameter_overrides.clone();
            
            // Add user-specified parameters
            for param in extracted_params {
                if !suggestion.parameters.iter().any(|p| p.name == param.name) {
                    suggestion.parameters.push(param);
                }
            }
            
            component_inference.add_inferred_component(suggestion);
        }
    }
}

/// Process connection statement for inline component instantiations
fn process_connection_for_components(
    node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
    component_inference: &mut ComponentInferenceContext,
    power_context: &PowerAnalysisContext,
) {
    use bhdl_ast::flow::{FlowExpr, FlowElement};
    
    println!("DEBUG: process_connection_for_components called for: {}", node.text());
    
    // Look for flow expressions within the connection statement
    for child in node.children() {
        if let Some(flow_expr) = FlowExpr::cast(child.clone()) {
            println!("DEBUG: Found FlowExpr in connection");
            // Process each element in the flow expression
            for element in flow_expr.elements() {
                if let FlowElement::ComponentInstantiation(comp_inst) = element {
                    println!("DEBUG: Found ComponentInstantiation in flow");
                    process_component_instantiation_v2(&comp_inst, component_inference, power_context);
                }
            }
        }
    }
}

/// Parse power value from string (e.g., "0.25W" -> 0.25)
fn parse_power_value(value: &str) -> Result<f64, std::num::ParseFloatError> {
    let value = value.trim();
    let numeric_part = value.trim_end_matches(|c: char| c.is_alphabetic());
    numeric_part.parse()
}

/// Parse percentage value from string (e.g., "5%" -> 0.05)
fn parse_percentage_value(value: &str) -> Result<f64, std::num::ParseFloatError> {
    let value = value.trim();
    let numeric_part = value.trim_end_matches('%');
    numeric_part.parse::<f64>().map(|v| v / 100.0)
}

/// Get LED forward voltage based on color
fn get_led_forward_voltage(color: &Option<String>) -> f64 {
    match color.as_ref().map(|s| s.as_str()) {
        Some("red") => 2.0,
        Some("green") => 2.2,
        Some("blue") => 3.0,
        Some("white") => 3.3,
        _ => 2.0, // Default to red
    }
}

/// Get reference designator prefix for a component type
fn get_refdes_prefix(component_type: &str) -> &'static str {
    match component_type {
        "Res" | "Resistor" => "R",
        "Cap" | "Capacitor" => "C", 
        "LED" => "LED",
        "Diode" => "D",
        "L" | "Inductor" => "L",
        _ => {
            // For ICs and other components, default to "U"
            // This includes part numbers like LM7805, NE555, etc.
            "U"
        }
    }
}

/// Extract instance name from connection context by looking for named handle patterns
fn extract_instance_name_from_context(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>) -> Option<String> {
    use bhdl_ast::SyntaxKind;
    
    println!("DEBUG: extract_instance_name_from_context called for node: {}", node.text());
    
    // In BHDL v2.0, the pattern "name: Component(...)" creates a component handle
    // According to the spec section 5.5:
    // - Component handles use `:` syntax and create ONLY component references
    // - Example: "r1: Res(10kΩ)" creates component with handle "r1"
    // - Example: "LED1: LED(red)" creates component with handle "LED1"
    
    // Look for the handle name before the colon in the parent context
    // We need to traverse up to find the containing connection or assignment
    let mut current = Some(node.clone());
    
    while let Some(parent) = current {
        // Check if this node or its parent contains the inline assignment pattern
        let text = parent.text().to_string();
        
        // Look for pattern "handle: ComponentType" in the text
        if let Some(colon_pos) = text.find(':') {
            // Make sure this is a component instantiation (has parentheses after colon)
            let after_colon = &text[colon_pos + 1..].trim_start();
            if after_colon.contains('(') {
                // Extract the handle name before the colon
                let before_colon = &text[..colon_pos].trim_end();
                // The handle might be preceded by other text (like "->"), so take the last identifier
                if let Some(handle) = before_colon.split_whitespace().last() {
                    // Clean up the handle (remove any leading arrows or operators)
                    let handle = handle.trim_start_matches("->").trim();
                    if !handle.is_empty() && handle.chars().all(|c| c.is_alphanumeric() || c == '_') {
                        println!("DEBUG: Extracted instance name from context: {}", handle);
                        return Some(handle.to_string());
                    }
                }
            }
        }
        
        // Move up to parent
        current = parent.parent();
        
        // Don't go too far up the tree
        if parent.kind() == SyntaxKind::CONNECTION_STMT || 
           parent.kind() == SyntaxKind::FLOW_STMT ||
           parent.kind() == SyntaxKind::BOARD_DEF {
            break;
        }
    }
    
    println!("DEBUG: No instance name found in context");
    None
}

/// Helper function to extract component instance name from COMPONENT_INST node
fn get_component_instance_name(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>) -> Option<String> {
    
    // Look for the instance name in the AST structure
    // Format is typically: ComponentType(params).pin or ComponentType(params)
    // We want to extract the full instance identifier
    
    // For now, just generate a name from the component type and its position
    // This could be enhanced to look for actual instance names in the future
    node.children_with_tokens()
        .find_map(|child| {
            if let rowan::NodeOrToken::Token(token) = child {
                if token.kind() == bhdl_ast::SyntaxKind::IDENT {
                    Some(format!("{}_{}", token.text(), node.index()))
                } else {
                    None
                }
            } else {
                None
            }
        })
}

/// Generate power sequences based on power analysis
fn generate_power_sequences(
    power_sequencing: &mut PowerSequenceGenerator,
    power_context: &PowerAnalysisContext,
) {
    use power_sequencing::PowerDomain as SeqPowerDomain;

    // Convert power analysis domains to sequencing domains
    for (name, power_domain) in &power_context.domains {
        let seq_domain = SeqPowerDomain {
            name: name.clone(),
            voltage: power_domain.voltage,
            max_current: power_domain.max_current,
            enable_signal: power_domain.enable_signal.clone(),
            good_signal: None, // Could be enhanced later
            dependencies: power_domain.dependencies.clone(),
            startup_delay_ms: power_domain.startup_delay_ms,
            shutdown_delay_ms: 5.0, // Default shutdown delay
            ramp_rate_v_per_ms: None, // Could be enhanced later
            sequence_priority: power_domain.sequence_priority,
            critical: name.contains("VCC") || name.contains("USB"), // Basic criticality heuristic
        };
        
        power_sequencing.add_domain(seq_domain);
    }

    // Generate the sequences
    if let Err(error) = power_sequencing.generate_sequences() {
        power_sequencing.warnings.push(format!("Sequence generation error: {}", error));
    }
}

/// Extract expansion recipes from all entity definitions in the source file.
///
/// Walks the AST looking for entity definitions that contain `expansion { }`
/// blocks. For each one, parses the expansion body into a structured
/// `ExpansionRecipe` suitable for the synthesizer's expansion interpreter.
pub fn extract_expansion_recipes(
    source_file: &SourceFile,
) -> std::collections::HashMap<String, bhdl_common::ExpansionRecipe> {
    use bhdl_common::{ExpansionRecipe, ExpansionInstance, ExpansionConnection, ExpansionEndpoint};
    use bhdl_ast::{Entity, HasName, SyntaxKind};
    use rowan::ast::AstNode;

    let mut recipes = std::collections::HashMap::new();

    // Walk all top-level items looking for entity definitions
    for item in source_file.items() {
        if let Some(entity) = Entity::cast(item.syntax().clone()) {
            if let Some(expansion_block) = entity.expansion_block() {
                let entity_name = entity.name()
                    .map(|t| t.text().to_string())
                    .unwrap_or_default();

                if entity_name.is_empty() {
                    continue;
                }

                let mut recipe = ExpansionRecipe::new(entity_name.clone());

                // Extract entity parameter defaults (e.g., l_value = 33µH)
                if let Some(param_list) = entity.param_list() {
                    for param_def in param_list.param_defs() {
                        if let Some(name) = param_def.name() {
                            let param_name = name.text().to_string();
                            if let Some(default_val) = param_def.default_value() {
                                let val_text = default_val.syntax().text().to_string().trim().to_string();
                                recipe.param_defaults.insert(param_name, val_text);
                            }
                        }
                    }
                }

                // Extract pin type and direction info for schematic placement hints
                for pin_def in entity.pins() {
                    if let Some(pin_name) = pin_def.name() {
                        let name = pin_name.text().to_string();
                        let pin_type = pin_def.pin_type()
                            .map(|t| t.text().to_string())
                            .unwrap_or_else(|| "signal".to_string());
                        let direction = pin_def.direction()
                            .map(|t| t.text().to_string())
                            .unwrap_or_else(|| "inout".to_string());
                        recipe.pin_info.insert(name, (pin_type, direction));
                    }
                }

                // Extract internal net declarations
                recipe.internal_nets = expansion_block.internal_nets();

                // Extract instances and connections from connection statements
                for conn_stmt in expansion_block.connection_stmts() {
                    parse_expansion_connection_stmt(
                        conn_stmt.syntax(),
                        &mut recipe,
                        &entity,
                    );
                }

                if !recipe.instances.is_empty() || !recipe.connections.is_empty() {
                    println!("  Extracted expansion recipe for '{}': {} instances, {} connections, {} internal nets, {} param defaults",
                        entity_name, recipe.instances.len(), recipe.connections.len(), recipe.internal_nets.len(), recipe.param_defaults.len());
                    recipes.insert(entity_name, recipe);
                }
            }
        }
    }

    recipes
}

/// Parse a single CONNECTION_STMT inside an expansion block into instances and connections.
///
/// Handles flow chains like: `VOUT -> L: Ind(l_value).1 -> L.2 -> sw;`
/// This creates:
///   - Instance: L of type Ind with param l_value
///   - Connections: ParentPin("VOUT") → InstancePin("L", "1"),
///                  InstancePin("L", "2") → InternalNet("sw")
fn parse_expansion_connection_stmt(
    node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
    recipe: &mut bhdl_common::ExpansionRecipe,
    entity: &bhdl_ast::Entity,
) {
    use bhdl_common::{ExpansionInstance, ExpansionConnection, ExpansionEndpoint};
    use bhdl_ast::SyntaxKind;

    // Collect all known entity pin names
    let entity_pins: Vec<String> = entity.pins()
        .filter_map(|p| {
            use bhdl_ast::HasName;
            p.name().map(|t| t.text().to_string())
        })
        .collect();

    // Collect known instance names to avoid duplicates
    let known_instance_names: std::collections::HashSet<String> = recipe.instances.iter()
        .map(|i| i.name.clone())
        .collect();

    // Walk through the flow chain elements in order.
    // The chain is a sequence of endpoints separated by ARROW tokens.
    // Each endpoint can be:
    //   - A bare identifier (entity pin or internal net)
    //   - handle: Type(params).pin  (inline instantiation)
    //   - handle.pin  (reference to already-instantiated child)
    let text = node.text().to_string();
    let text = text.trim().trim_end_matches(';').trim();

    // Split by " -> " to get the chain elements
    let elements: Vec<&str> = text.split("->").map(|s| s.trim()).collect();

    // Track last endpoint for chaining
    let mut prev_endpoint: Option<ExpansionEndpoint> = None;

    for element in &elements {
        let element = element.trim();
        if element.is_empty() {
            continue;
        }

        // Check for "handle: Type(params).pin" pattern
        let (endpoint, maybe_instance) = parse_expansion_element(
            element,
            &entity_pins,
            &recipe.internal_nets,
        );

        // Register the instance if it's new
        if let Some(inst) = maybe_instance {
            if !known_instance_names.contains(&inst.name)
                && !recipe.instances.iter().any(|i| i.name == inst.name)
            {
                recipe.instances.push(inst);
            }
        }

        // Create connection from previous endpoint to this one
        if let (Some(from), Some(ref to)) = (&prev_endpoint, &endpoint) {
            recipe.connections.push(ExpansionConnection {
                from: from.clone(),
                to: to.clone(),
            });
        }

        prev_endpoint = endpoint;
    }
}

/// Parse a single element in an expansion flow chain.
/// Returns the endpoint and optionally a new instance to create.
fn parse_expansion_element(
    text: &str,
    entity_pins: &[String],
    internal_nets: &[String],
) -> (Option<bhdl_common::ExpansionEndpoint>, Option<bhdl_common::ExpansionInstance>) {
    use bhdl_common::{ExpansionEndpoint, ExpansionInstance};

    let text = text.trim();

    // Check for "handle: Type(params).pin" pattern (inline instantiation)
    if let Some(colon_pos) = text.find(':') {
        let handle = text[..colon_pos].trim();
        let rest = text[colon_pos + 1..].trim();

        // Parse "Type(params).pin"
        if let Some(paren_start) = rest.find('(') {
            let comp_type = rest[..paren_start].trim();
            if let Some(paren_end) = rest.find(')') {
                let params_str = rest[paren_start + 1..paren_end].trim();
                let params: Vec<String> = if params_str.is_empty() {
                    Vec::new()
                } else {
                    params_str.split(',').map(|s| s.trim().to_string()).collect()
                };

                // Check for ".pin" after the params
                let after_paren = rest[paren_end + 1..].trim();
                let pin = if after_paren.starts_with('.') {
                    Some(after_paren[1..].trim().to_string())
                } else {
                    None
                };

                let instance = ExpansionInstance {
                    name: handle.to_string(),
                    component_type: comp_type.to_string(),
                    params,
                    attributes: std::collections::HashMap::new(),
                };

                let endpoint = pin.map(|p| ExpansionEndpoint::InstancePin(handle.to_string(), p));
                return (endpoint, Some(instance));
            }
        }
    }

    // Check for "instance.pin" pattern (reference to existing child)
    if let Some(dot_pos) = text.find('.') {
        let left = text[..dot_pos].trim();
        let pin = text[dot_pos + 1..].trim();

        // If left matches an entity pin, this is "ParentPin.something" — unlikely but handle
        if entity_pins.contains(&left.to_string()) {
            return (Some(ExpansionEndpoint::ParentPin(left.to_string())), None);
        }

        return (
            Some(ExpansionEndpoint::InstancePin(left.to_string(), pin.to_string())),
            None,
        );
    }

    // Bare identifier — check if it's an entity pin or internal net
    if entity_pins.contains(&text.to_string()) {
        return (Some(ExpansionEndpoint::ParentPin(text.to_string())), None);
    }

    if internal_nets.contains(&text.to_string()) {
        return (Some(ExpansionEndpoint::InternalNet(text.to_string())), None);
    }

    // Unknown — treat as internal net (could be a net reference like @GND)
    // or an entity pin not yet seen
    // For robustness, try stripping @ prefix
    if text.starts_with('@') {
        let net_name = &text[1..];
        return (Some(ExpansionEndpoint::ParentPin(net_name.to_string())), None);
    }

    // Default: assume it's an entity pin (e.g., "GND", "VIN")
    (Some(ExpansionEndpoint::ParentPin(text.to_string())), None)
}

/// Extract placement recipes from all entity definitions in the source file.
///
/// Walks the AST looking for entity definitions that contain `placement { }`
/// blocks. For each one, parses the placement body into a structured
/// `PlacementRecipe` suitable for the PnR engine.
pub fn extract_placement_recipes(
    source_file: &SourceFile,
) -> std::collections::HashMap<String, bhdl_common::PlacementRecipe> {
    use bhdl_common::{PlacementRecipe, ChildPosition};
    use bhdl_ast::{Entity, HasName};
    use rowan::ast::AstNode;

    let mut recipes = std::collections::HashMap::new();

    // Walk all top-level items looking for entity definitions
    for item in source_file.items() {
        if let Some(entity) = Entity::cast(item.syntax().clone()) {
            if let Some(placement_block) = entity.placement_block() {
                let entity_name = entity.name()
                    .map(|t| t.text().to_string())
                    .unwrap_or_default();

                if entity_name.is_empty() {
                    continue;
                }

                let mut recipe = PlacementRecipe::new(entity_name.clone());
                recipe.reference = placement_block.reference_text();

                for item in placement_block.placement_items() {
                    if let Some(name) = item.component_name() {
                        if let Some((x, y)) = item.coordinates() {
                            let rotation = item.rotation_deg().unwrap_or(0.0);
                            recipe.positions.push(ChildPosition {
                                name,
                                dx_mm: x,
                                dy_mm: y,
                                rotation_deg: rotation,
                            });
                        }
                    }
                }

                if !recipe.positions.is_empty() || recipe.reference.is_some() {
                    println!("  Extracted placement recipe for '{}': {} positions, ref: {:?}",
                        entity_name, recipe.positions.len(), recipe.reference);
                    recipes.insert(entity_name, recipe);
                }
            }
        }
    }

    recipes
}

/// Extract symbol definitions from a parsed source file.
///
/// Walks the AST looking for `symbol EntityName { ... }` top-level items
/// and converts them into `SymbolDefinition` data structures.
pub fn extract_symbol_definitions(
    source_file: &SourceFile,
) -> std::collections::HashMap<String, bhdl_common::SymbolDefinition> {
    use bhdl_ast::{SymbolDef, HasName};
    use bhdl_common::symbol::{SymbolDefinition, SymbolSide, PinSide, SideEntry};

    let mut definitions = std::collections::HashMap::new();

    for sym_def in source_file.symbols() {
        let entity_name = sym_def.name()
            .map(|t| t.text().to_string())
            .unwrap_or_default();

        if entity_name.is_empty() {
            continue;
        }

        let body_hint = sym_def.body_hint();

        let mut sides = Vec::new();
        for ast_side in sym_def.sides() {
            let side_name = match ast_side.side() {
                Some(s) => s,
                None => continue,
            };
            let pin_side = match PinSide::from_str(&side_name) {
                Some(s) => s,
                None => continue,
            };

            let mut entries = Vec::new();

            // Collect bare pins
            for pin in ast_side.pins() {
                entries.push(SideEntry::Pin { name: pin });
            }

            // Collect groups
            for group in ast_side.groups() {
                let label = group.label().unwrap_or_default();
                let pins = group.pins();
                entries.push(SideEntry::Group { label, pins });
            }

            sides.push(SymbolSide { side: pin_side, entries });
        }

        definitions.insert(entity_name.clone(), SymbolDefinition {
            entity_name,
            body_hint,
            sides,
        });
    }

    definitions
}

/// Extract layout definitions from a parsed source file.
///
/// Walks the AST looking for `layout EntityName { ... }` top-level items
/// and converts them into `LayoutDefinition` data structures.
pub fn extract_layout_definitions(
    source_file: &SourceFile,
) -> std::collections::HashMap<String, bhdl_common::LayoutDefinition> {
    use bhdl_ast::{LayoutDef, HasName};
    use bhdl_common::layout_meta::LayoutDefinition;

    let mut definitions = std::collections::HashMap::new();

    for layout_def in source_file.layouts() {
        let entity_name = layout_def.name()
            .map(|t| t.text().to_string())
            .unwrap_or_default();

        if entity_name.is_empty() {
            continue;
        }

        let package = layout_def.package();

        definitions.insert(entity_name.clone(), LayoutDefinition {
            entity_name,
            package,
        });
    }

    definitions
}

// Remove the test module declaration as tests are now in the tests/ directory
// #[cfg(test)]
// mod tests; // Load tests from tests.rs when running cargo test

// The #[cfg(test)] mod tests { ... } block should follow here (as kept by the user)


