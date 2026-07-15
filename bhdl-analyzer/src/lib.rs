// Needed for analyze function signature and AST traversal
use bhdl_ast::SourceFile;
use rowan::ast::AstNode; // For source_file.syntax()

// Declare modules
pub mod types;
mod helpers;
pub mod symbol_table;
pub mod scope_registry;
pub mod hierarchical_symbol_table;
pub use hierarchical_symbol_table::definition_scopes_sorted;
pub mod net_attributes;
pub mod part_family;
pub mod catalog_scan;
pub mod value_snap;
pub mod plugin;
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
pub mod sku_bom;

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
    let (mut scope_registry, alias_specializations, imported_expansion_recipes, imported_symbol_definitions, imported_layout_definitions, imported_placement_recipes, imported_design_recipes, imported_stress_recipes, imported_model_recipes, imported_entity_attr_index, imported_entity_param_index, imported_entity_value_domains, imported_entity_attr_param_refs) = pass1::build_scope_registry_with_base(source_file, base_path);
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

    // Build the global entity constructor-parameter-name index (imported
    // files + main file), the order-independent companion to
    // entity_attribute_index. The synthesizer's expansion interpreter
    // uses it to resolve attribute values that reference a child entity's
    // own parameter into the instantiation argument. Built BEFORE Pass 6
    // so component inference can bind positional constructor args to the
    // entity's declared parameter names (ElectrolyticCap(100µF, 25V) →
    // value/voltage, not value/param_1).
    let mut entity_param_names = imported_entity_param_index.clone();
    for (name, params) in extract_entity_param_names(source_file) {
        entity_param_names.insert(name, params);
    }

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
    log::debug!("Available power domains for component inference:");
    let mut domains_sorted: Vec<_> = power_context.domains.iter().collect();
    domains_sorted.sort_by_key(|(name, _)| name.as_str());
    for (name, domain) in domains_sorted {
        log::debug!("  - {}: {}V @ {}A", name, domain.voltage, domain.max_current);
    }
    log::debug!("Component domain assignments:");
    let mut assignments_sorted: Vec<_> = power_context.component_domains.iter().collect();
    assignments_sorted.sort();
    for (comp, domain) in assignments_sorted {
        log::debug!("  - {} -> {}", comp, domain);
    }
    
    analyze_components_for_inference(source_file.syntax(), &mut component_inference, &power_context, &entity_param_names);
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
    // Stage 6 cross-file: thread pass1's accumulated cross-file
    // entity-attribute index through main-file recipe extraction. A
    // board (main file) that instantiates a stage entity from an
    // imported stdlib file needs the *stage's* expansion children's
    // attributes — which come from yet-another imported file — to be
    // resolved here. (Cross-file resolution for purely imported
    // expansion recipes is already handled inside pass1's per-import
    // pass.)
    let main_file_recipes = extract_expansion_recipes_with_overlay(
        source_file,
        &imported_entity_attr_index,
    );
    expansion_recipes.extend(main_file_recipes);
    let expansion_count = expansion_recipes.len();

    // Build the global entity attribute index (imported files + main
    // file). This is the order-independent source-of-truth for entity
    // attribute defaults, threaded through AnalysisResult so the
    // synthesizer's expansion interpreter can late-bind attributes
    // onto leaf instances when the order-dependent extraction-time
    // overlay missed them.
    let mut entity_attribute_index = imported_entity_attr_index.clone();
    for (name, attrs) in extract_entity_attribute_index(source_file) {
        entity_attribute_index.insert(name, attrs);
    }

    // Per-entity attribute→param bare-reference linkage (imported files +
    // main file), the substitution anchor the default-resolved attribute
    // index erases.
    let mut entity_attr_param_refs = imported_entity_attr_param_refs.clone();
    for (name, refs) in extract_entity_attr_param_refs(source_file) {
        entity_attr_param_refs.insert(name, refs);
    }

    // Per-entity parameter value domains (`where <param> in (...)`),
    // imported files + main file.
    let mut entity_value_domains = imported_entity_value_domains.clone();
    for (name, doms) in extract_entity_value_domains(source_file) {
        entity_value_domains.insert(name, doms);
    }

    // Reject constructor args that bind to no declared parameter, and
    // values outside a parameter's declared allowed set. Both used to pass
    // silently; emitted as Error-severity diagnostics the CLI refuses to
    // build on.
    for diag in validate_constructor_args(source_file, &entity_param_names, &entity_value_domains) {
        diagnostics.push(diag);
    }

    // Extract design recipes from the main source file. (Imported-file
    // Merge vendor `design { }` recipes: imported files (loaded by pass1)
    // overlaid with the main file's blocks.
    let mut design_recipes = imported_design_recipes;
    for (entity, by_intent) in extract_design_recipes(source_file) {
        design_recipes.entry(entity).or_default().extend(by_intent);
    }

    // Extract stress recipes (simulation { stress { } }, §4) from the main
    // source file. Import-merge (for stdlib-defined entities) is threaded
    // through pass1 in a later stage; today's targets declare the block in the
    // board file alongside the instance.
    // Imported entities first (stdlib parts carrying the blocks), then the main
    // source file overlaid on top (a board-file entity of the same name wins).
    let mut stress_recipes = imported_stress_recipes;
    stress_recipes.extend(extract_stress_recipes(source_file));
    // Extract model recipes (simulation { model { } }, §5) from the main source.
    let mut model_recipes = imported_model_recipes;
    model_recipes.extend(extract_model_recipes(source_file));

    // Extract board-level SKU variants from the main source file.
    // Variants are board-local (a `variant` block can only patch
    // instances declared in the surrounding board), so we don't
    // pass1-merge them across imports — a board can't reference
    // another file's instances anyway.
    let variants = extract_variant_blocks(source_file);

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
        design_recipes, // Move ownership (Pass 8.5)
        stress_recipes, // Move ownership (§4 stress surface)
        model_recipes, // Move ownership (§5 model surface)
        variants,
        entity_attribute_index,
        entity_param_names,
        entity_attr_param_refs,
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
    entity_param_names: &std::collections::HashMap<String, Vec<String>>,
) {



    // Walk through the syntax tree looking for component instantiations
    visit_node_for_component_inference(syntax, component_inference, power_context, entity_param_names);
}

/// Visit nodes for component inference
fn visit_node_for_component_inference(
    node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
    component_inference: &mut ComponentInferenceContext,
    power_context: &PowerAnalysisContext,
    entity_param_names: &std::collections::HashMap<String, Vec<String>>,
) {
    use bhdl_ast::SyntaxKind;
    use component_inference::CircuitContext;

    match node.kind() {
        bhdl_ast::SyntaxKind::FLOW_EXPR => {
            // Process flow expressions which contain inline component instantiations in v2.0
            use bhdl_ast::flow::{FlowExpr, FlowElement};
            
            log::debug!("Found FLOW_EXPR node");
            if let Some(flow_expr) = FlowExpr::cast(node.clone()) {
                log::debug!("Successfully cast to FlowExpr");
                // Process each element in the flow expression
                for element in flow_expr.elements() {
                    match &element {
                        FlowElement::ComponentInstantiation(comp_inst) => {
                            log::debug!("Found ComponentInstantiation in flow");
                            process_component_instantiation_v2(&comp_inst, component_inference, power_context, entity_param_names);
                        }
                        FlowElement::Identifier(token) => {
                            log::debug!("Found Identifier in flow: {}", token.text());
                        }
                        FlowElement::ConditionalExpr(_) => {
                            log::debug!("Found ConditionalExpr in flow");
                        }
                    }
                }
            } else {
                log::debug!("Failed to cast FLOW_EXPR to FlowExpr");
            }
        }
        bhdl_ast::SyntaxKind::CONNECTION_STMT => {
            // Process connection statements which may contain inline component instantiations
            // e.g., VCC -> R1(10k).1 -> LED1(red).A -> GND;
            let _stmt_text = node.to_string();
            
            // Look for inline component instantiations in the connection
            // Pattern: ComponentType(params) or name: ComponentType(params)
            process_connection_for_components(node, component_inference, power_context, entity_param_names);
            
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
                    log::debug!("Processing inline component: {}", component_type);
                    
                    // Process as v2.0 inline instantiation
                    use bhdl_ast::flow::ComponentInstantiation;
                    if let Some(comp_inst) = ComponentInstantiation::cast(node.clone()) {
                        process_component_instantiation_v2(&comp_inst, component_inference, power_context, entity_param_names);
                        return;
                    }
                }
            }
            
            // Handle ComponentInst from common module (not flow module)
            use bhdl_ast::ComponentInst;
            
            if let Some(comp_inst) = ComponentInst::cast(node.clone()) {
                log::debug!("Processing ComponentInst (common module)");
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
                log::debug!("COMPONENT_INST '{}' with extracted instance name: {:?}", component_type, instance_name);
                
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
                let mut explicit_params = std::collections::BTreeMap::new();
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
                    log::debug!("Component '{}' using supply voltage: {}V", component_type, voltage);
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
                    log::debug!("No suggestion returned for '{}'", component_type);
                }
            }
        }
        _ => {}
    }

    // Recursively visit children
    for child in node.children() {
        visit_node_for_component_inference(&child, component_inference, power_context, entity_param_names);
    }
}

/// Process component instantiation from v2.0 flow syntax
fn process_component_instantiation_v2(
    comp_inst: &bhdl_ast::flow::ComponentInstantiation,
    component_inference: &mut ComponentInferenceContext,
    power_context: &PowerAnalysisContext,
    entity_param_names: &std::collections::HashMap<String, Vec<String>>,
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
            log::debug!("Component {} has placeholder parameters - marking for SPICE resolution", component_type);
            
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
        log::debug!("Extracting normal parameters for {}", component_type);
        // Only extract normal parameters if there's no placeholder.
        // Positional parameters bind to the entity's DECLARED parameter
        // names in order (ElectrolyticCap(100µF, 25V) → value, voltage) —
        // the entity_param_names index carries every imported + same-file
        // entity's ordered param list. The hardcoded passive signatures
        // remain as a fallback for types the index doesn't know (naming
        // EVERY positional "value" silently dropped the second one:
        // Res(10k, 1%) lost its tolerance, Cap(22uF, 35V) its voltage
        // rating; an unbound name like "param_1" left the entity's
        // attribute expressions referencing that param unevaluated).
        let declared_params = entity_param_names.get(&component_type);
        let mut positional_idx = 0usize;
        for param_assign in comp_inst.parameter_assignments() {
            if let Some(value) = param_assign.value() {
                let param_name = param_assign.name()
                    .map(|n| n.text().to_string())
                    .unwrap_or_else(|| {
                        let sig: &[&str] = match component_type.as_str() {
                            "Res" | "Resistor" => &["value", "tolerance"],
                            "Cap" | "Capacitor" => &["value", "voltage"],
                            "Ind" | "Inductor" => &["value", "rated_current"],
                            "LED" => &["color"],
                            "Fuse" => &["current_rating"],
                            "TVSDiode" => &["voltage_rating"],
                            _ => &["value"],
                        };
                        let name = declared_params
                            .and_then(|d| d.get(positional_idx))
                            .cloned()
                            .or_else(|| sig.get(positional_idx).copied().map(str::to_string))
                            .unwrap_or_else(|| format!("param_{positional_idx}"));
                        positional_idx += 1;
                        name
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
    log::debug!("Extracted instance name from context: {:?}", extracted_name);
    
    let instance_name = extracted_name.unwrap_or_else(|| {
            // If no explicit name found, generate one based on component type
            static mut COMPONENT_COUNTER: usize = 0;
            let instance_id = unsafe {
                COMPONENT_COUNTER += 1;
                COMPONENT_COUNTER
            };
            format!("{}{}", get_refdes_prefix(&component_type), instance_id)
        });
    
    log::debug!("Processing v2.0 component instantiation: {} (type: {}) with {} parameters", 
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
        log::debug!("Component {} has {} extracted parameters", component_type, extracted_params.len());
        let mut explicit_params_map = std::collections::BTreeMap::new();
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
            log::debug!("Adding param '{}' = '{}'", param.name, value_str);
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
        
        log::debug!("Added unresolved component {} for SPICE resolution", instance_name);
    } else {
        // Normal inference for components with specified values
        if let Some(mut suggestion) = component_inference.infer_component_parameters(
            &component_type, &requirements, &circuit_context
        ) {
            // Set the instance name
            suggestion.instance_name = Some(instance_name.clone());
            
            // Add parameter overrides for interfaces
            suggestion.parameter_overrides = parameter_overrides.clone();
            
            // Add user-specified parameters to the suggestion. A
            // user-specified param REPLACES a same-named inferred default —
            // the skip-if-present form silently kept the entity default
            // (tolerance 5%) over the instantiation's explicit
            // tolerance=1%.
            for param in extracted_params {
                if let Some(existing) = suggestion
                    .parameters
                    .iter_mut()
                    .find(|p| p.name == param.name)
                {
                    *existing = param;
                } else {
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
    
    log::debug!("Processing component type: {}", component_type);
    
    // Check if this component has placeholder parameters (for SPICE generation)
    let mut has_placeholder = false;
    let mut placeholder_constraints = Vec::new();
    
    if let Some(param_block) = comp_inst.param_assign_block() {
        log::debug!("Component {} has param block", component_type);
        if param_block.has_placeholder() {
            has_placeholder = true;
            log::debug!("Component {} has placeholder parameters - marking for SPICE resolution", component_type);
            
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
            log::debug!("Extracting parameters from param list for {}", component_type);
            for param in param_list.params() {
                if let (Some(name), Some(value)) = (param.name(), param.value()) {
                    let param_name = name.text().to_string();
                    let param_value = value.syntax().text().to_string().trim_matches('"').to_string();
                    log::debug!("Found param '{}' = '{}'", param_name, param_value);
                    
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
            log::debug!("Extracting parameters from param block for {}", component_type);
            for param_assign in param_block.assignments() {
                if let Some(value) = param_assign.value() {
                    // Get parameter name, or use empty string for positional parameters
                    let param_name = param_assign.name()
                        .map(|n| n.text().to_string())
                        .unwrap_or_else(|| String::new());
                    
                    let param_value = value.syntax().text().to_string();
                    log::debug!("Found param '{}' = '{}'", param_name, param_value);
                    
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
    
    log::debug!("Component instance name: {}", instance_name);
    
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
        log::debug!("Component {} has {} extracted parameters", component_type, extracted_params.len());
        let mut explicit_params_map = std::collections::BTreeMap::new();
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
            log::debug!("Adding param '{}' = '{}'", param.name, value_str);
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
        
        log::debug!("Added unresolved component {} for SPICE resolution", instance_name);
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
    entity_param_names: &std::collections::HashMap<String, Vec<String>>,
) {
    use bhdl_ast::flow::{FlowExpr, FlowElement};
    
    log::debug!("process_connection_for_components called for: {}", node.text());
    
    // Look for flow expressions within the connection statement
    for child in node.children() {
        if let Some(flow_expr) = FlowExpr::cast(child.clone()) {
            log::debug!("Found FlowExpr in connection");
            // Process each element in the flow expression
            for element in flow_expr.elements() {
                if let FlowElement::ComponentInstantiation(comp_inst) = element {
                    log::debug!("Found ComponentInstantiation in flow");
                    process_component_instantiation_v2(&comp_inst, component_inference, power_context, entity_param_names);
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
    
    log::debug!("extract_instance_name_from_context called for node: {}", node.text());
    
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
                        log::debug!("Extracted instance name from context: {}", handle);
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
    
    log::debug!("No instance name found in context");
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
    extract_expansion_recipes_with_overlay(
        source_file,
        &std::collections::HashMap::new(),
    )
}

/// Walk a source file's entities and extract their attribute defaults as
/// an `(entity_name → {attr → value})` map. Used by Stage 6's
/// device-discovery wiring: pass1 accumulates these across all imported
/// files and the main file, then threads the combined map back into
/// `extract_expansion_recipes_with_overlay` so cross-file references
/// (a stage entity in one file instantiating a tube device defined in
/// another) carry their callee's attributes through to expansion time.
pub fn extract_entity_attribute_index(
    source_file: &SourceFile,
) -> std::collections::HashMap<String, std::collections::HashMap<String, String>> {
    use bhdl_ast::{Entity, HasName};
    use rowan::ast::AstNode;
    let mut idx = std::collections::HashMap::new();
    for item in source_file.items() {
        if let Some(entity) = Entity::cast(item.syntax().clone()) {
            if let Some(name_token) = entity.name() {
                // Raw values, then overlay the resolvable param/const
                // references with the param's default (`attribute f_sw = f_sw`
                // → "570kHz") — same per-key unwrap_or(raw) contract as the
                // component-library loader and module propagation. Without
                // this, a stress block's `self.f_sw` on an entity that
                // declares the attribute as a param ref got the literal text
                // "f_sw", was dropped by the numeric filter, and the whole
                // recipe silently didn't apply. Un-defaulted refs (`value`)
                // and generic refs stay raw for the downstream per-instance
                // substitution passes.
                let mut attrs = crate::attribute_extraction::extract_module_attributes(&entity);
                let resolved =
                    crate::attribute_extraction::extract_module_attributes_resolved(&entity);
                for (k, v) in attrs.iter_mut() {
                    if let Some(r) = resolved.get(k) {
                        *v = r.clone();
                    }
                }
                if !attrs.is_empty() {
                    idx.insert(name_token.text().to_string(), attrs);
                }
            }
        }
    }
    idx
}

/// Walk a source file's entities and extract their ordered constructor-
/// parameter names as an `(entity_name → [param_name, …])` map. Mirrors
/// [`extract_entity_attribute_index`]; pass1 accumulates these across all
/// imported files and threads the combined map into
/// `extract_expansion_recipes_with_overlay`. The recipe extractor uses it
/// to resolve attribute values that are bare references to a child
/// entity's own parameter (e.g. `entity Cap(value: capacitance) {
/// attribute capacitance = value; }`) into the positional argument
/// supplied at the instantiation site — so the leaf instance carries
/// `capacitance = "100nF"` rather than the literal placeholder `"value"`.
pub fn extract_entity_param_names(
    source_file: &SourceFile,
) -> std::collections::HashMap<String, Vec<String>> {
    use bhdl_ast::{Entity, HasName};
    use rowan::ast::AstNode;
    let mut idx = std::collections::HashMap::new();
    for item in source_file.items() {
        if let Some(entity) = Entity::cast(item.syntax().clone()) {
            if let Some(name_token) = entity.name() {
                let mut names = Vec::new();
                if let Some(param_list) = entity.param_list() {
                    for param_def in param_list.param_defs() {
                        if let Some(n) = param_def.name() {
                            names.push(n.text().to_string());
                        }
                    }
                }
                if !names.is_empty() {
                    idx.insert(name_token.text().to_string(), names);
                }
            }
        }
    }

    // Same-file simple aliases (`alias Resistor = Res;`) inherit their
    // target's parameter list, so an instantiation through the alias
    // (`Capacitor(value, voltage)`) validates against the real entity's
    // parameters. Type-arg aliases (`alias LM7805 = Reg<5V>`) are handled
    // by monomorphization and skipped here.
    for node in source_file.syntax().descendants() {
        if node.kind() != bhdl_ast::SyntaxKind::ALIAS {
            continue;
        }
        if node
            .descendants()
            .any(|n| n.kind() == bhdl_ast::SyntaxKind::TYPE_ARGS)
        {
            continue;
        }
        let idents: Vec<String> = node
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == bhdl_ast::SyntaxKind::IDENT)
            .map(|t| t.text().to_string())
            .collect();
        if let [alias_name, target] = idents.as_slice() {
            if let Some(params) = idx.get(target).cloned() {
                idx.entry(alias_name.clone()).or_insert(params);
            }
        }
    }
    idx
}

/// Extract per-entity parameter value domains from `where <param> in
/// (<literal>, ...)` membership constraints — the allowed-value set for a
/// string/enum parameter. Returns entity name → param name → allowed
/// values (quotes trimmed). Same shape and import-merge path as
/// [`extract_entity_param_names`], so an instantiation validates its
/// argument values against the imported entity's declared domain.
pub fn extract_entity_value_domains(
    source_file: &SourceFile,
) -> std::collections::HashMap<String, std::collections::HashMap<String, Vec<String>>> {
    use bhdl_ast::{Entity, HasName};
    use rowan::ast::AstNode;
    let mut idx = std::collections::HashMap::new();
    for item in source_file.items() {
        let Some(entity) = Entity::cast(item.syntax().clone()) else { continue };
        let Some(name_token) = entity.name() else { continue };
        let mut domains: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for m in entity
            .syntax()
            .descendants()
            .filter(|n| n.kind() == bhdl_ast::SyntaxKind::MEMBERSHIP_CONSTRAINT)
        {
            // First IDENT = the parameter; STRING/NUMBER/IDENT tokens after
            // the `in (` are the allowed values.
            let toks: Vec<_> = m
                .children_with_tokens()
                .filter_map(|e| e.into_token())
                .filter(|t| {
                    matches!(
                        t.kind(),
                        bhdl_ast::SyntaxKind::IDENT
                            | bhdl_ast::SyntaxKind::STRING
                            | bhdl_ast::SyntaxKind::NUMBER
                    )
                })
                .collect();
            let Some((param_tok, value_toks)) = toks.split_first() else { continue };
            let param = param_tok.text().to_string();
            let values: Vec<String> = value_toks
                .iter()
                .map(|t| t.text().trim_matches('"').to_string())
                .collect();
            if !values.is_empty() {
                domains.insert(param, values);
            }
        }
        if !domains.is_empty() {
            idx.insert(name_token.text().to_string(), domains);
        }
    }
    idx
}

/// Per-entity map of attribute → the constructor PARAM it bare-references
/// (`attribute part_number = part_no;` → {"part_number": "part_no"}).
/// The attribute index resolves defaulted param-refs at extraction time
/// (stress blocks need real numbers), which erases the bare-reference
/// anchor the expansion-child substitution keys on — so entities whose
/// params ALL have defaults (MOSFET) could never receive threaded
/// constructor args. This parallel index records the linkage so the
/// expansion interpreter can overwrite the attr when an explicit arg is
/// supplied. Same import-merged contract as `entity_param_names`.
pub fn extract_entity_attr_param_refs(
    source_file: &SourceFile,
) -> std::collections::HashMap<String, std::collections::HashMap<String, String>> {
    use bhdl_ast::{Entity, HasName};
    use rowan::ast::AstNode;
    let mut idx = std::collections::HashMap::new();
    for item in source_file.items() {
        let Some(entity) = Entity::cast(item.syntax().clone()) else { continue };
        let Some(name_token) = entity.name() else { continue };
        let refs = crate::attribute_extraction::extract_module_attribute_param_refs(&entity);
        if !refs.is_empty() {
            idx.insert(name_token.text().to_string(), refs);
        }
    }
    idx
}

/// Character edit distance (for "did you mean" parameter suggestions).
fn arg_edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Reject constructor arguments that bind to no declared parameter — a
/// named arg whose name is not a parameter, or a positional arg beyond the
/// parameter count. Such args used to pass through as dead instance
/// attributes nothing reads, silently swallowing design intent (a
/// `Res(2.5Ω, wattage=10W)` whose 10 W never reached the part). Only
/// entities whose parameters are known (indexed from this file or an
/// import, aliases resolved) are checked; an unknown type is left to
/// symbol resolution rather than guessed at.
pub fn validate_constructor_args(
    source_file: &SourceFile,
    entity_param_names: &std::collections::HashMap<String, Vec<String>>,
    value_domains: &std::collections::HashMap<
        String,
        std::collections::HashMap<String, Vec<String>>,
    >,
) -> Vec<types::Diagnostic> {
    use bhdl_ast::{ComponentInst, HasName};
    use rowan::ast::AstNode;
    let mut out = Vec::new();
    for node in source_file.syntax().descendants() {
        let Some(inst) = ComponentInst::cast(node.clone()) else { continue };
        let Some(type_tok) = inst.component_type_name() else { continue };
        let entity = type_tok.text().to_string();
        let Some(params) = entity_param_names.get(&entity) else { continue };
        let Some(block) = inst.param_assign_block() else { continue };
        let domains = value_domains.get(&entity);

        // A value against its parameter's declared allowed set (`where
        // <param> in (...)`), if one exists. `channel = "P"` on a MOSFET
        // whose `where channel in ("nmos", "pmos")` rejects "P" — the name
        // binds fine, the value is out of domain.
        let value_diag = |param: &str, assign: &bhdl_ast::ParamAssign| -> Option<types::Diagnostic> {
            let allowed = domains?.get(param)?;
            let raw = assign.value()?.syntax().text().to_string();
            let val = raw.trim().trim_matches('"').to_string();
            if allowed.iter().any(|a| a == &val) {
                return None;
            }
            Some(types::Diagnostic::with_kind(
                bhdl_common::DiagnosticKind::ParameterValueNotAllowed {
                    param: param.to_string(),
                    entity: entity.clone(),
                    value: val.clone(),
                    allowed: allowed.clone(),
                },
                format!(
                    "'{entity}' parameter '{param}' = \"{val}\" is not one of its \
                     allowed values ({}) — declared by `where {param} in (...)`",
                    allowed
                        .iter()
                        .map(|a| format!("\"{a}\""))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                assign.syntax().text_range(),
            ))
        };

        let mut positional = 0usize;
        for assign in block.assignments() {
            match assign.name() {
                Some(name_tok) => {
                    let arg = name_tok.text().to_string();
                    if params.iter().any(|p| p == &arg) {
                        if let Some(d) = value_diag(&arg, &assign) {
                            out.push(d);
                        }
                        continue;
                    }
                    // Toolchain-reserved attribute namespace: the supply
                    // desugarer stamps `supply_*` / `i_supply` metadata that
                    // sign-off, ERC016, and the part chooser read back off
                    // the instance, and the expansion interpreter stamps
                    // `expansion_*` / `vpin_parent` provenance. These are
                    // deliberate machine-authored attribute passthrough, not
                    // user parameters, and never appear on an entity's
                    // declared list.
                    if arg.starts_with("supply_")
                        || arg == "i_supply"
                        || arg.starts_with("expansion_")
                        || arg == "vpin_parent"
                        // Simulation directives (IBIS buffer states etc.):
                        // stamped as instance attributes, consumed by the
                        // SPICE converter — same machine-passthrough class
                        // as supply_*.
                        || arg.starts_with("ibis_")
                        // Value-derivation directives: `derive_rule` marks
                        // the written value as a SEED the toolchain refines
                        // by simulation (bhdl-cli value_deriver); companions
                        // like derive_i2c_khz parameterize the rule.
                        || arg.starts_with("derive_")
                    {
                        continue;
                    }
                    let suggestions: Vec<String> = params
                        .iter()
                        .filter(|p| arg_edit_distance(p, &arg) <= 2)
                        .cloned()
                        .collect();
                    let did_you_mean = if suggestions.is_empty() {
                        String::new()
                    } else {
                        format!(" — did you mean {}?", suggestions.join(" / "))
                    };
                    let params_list = if params.is_empty() {
                        "no parameters".to_string()
                    } else {
                        params.join(", ")
                    };
                    out.push(
                        types::Diagnostic::with_kind(
                            bhdl_common::DiagnosticKind::UnknownConstructorArg {
                                arg: arg.clone(),
                                entity: entity.clone(),
                                suggestions,
                            },
                            format!(
                                "'{entity}' has no parameter '{arg}'{did_you_mean} \
                                 (declared: {params_list}). An unrecognized argument \
                                 is not a free annotation — it never reaches the part; \
                                 add the parameter to the entity or remove the argument",
                            ),
                            assign.syntax().text_range(),
                        ),
                    );
                }
                None => {
                    positional += 1;
                    if positional > params.len() {
                        out.push(
                            types::Diagnostic::with_kind(
                                bhdl_common::DiagnosticKind::UnknownConstructorArg {
                                    arg: format!("#{positional}"),
                                    entity: entity.clone(),
                                    suggestions: Vec::new(),
                                },
                                format!(
                                    "'{entity}' takes {} parameter(s) but a {}th \
                                     positional argument was supplied (declared: {})",
                                    params.len(),
                                    positional,
                                    if params.is_empty() {
                                        "none".to_string()
                                    } else {
                                        params.join(", ")
                                    },
                                ),
                                assign.syntax().text_range(),
                            ),
                        );
                    } else if let Some(param) = params.get(positional - 1) {
                        // Positional value against the param at this slot's
                        // domain — the SKU aliases pass channel positionally.
                        if let Some(d) = value_diag(param, &assign) {
                            out.push(d);
                        }
                    }
                }
            }
        }
    }
    out
}

/// Substitute attribute values that are bare references to one of the
/// child entity's constructor parameters with the corresponding
/// positional argument expression from the instantiation.
///
/// Fixes the case where e.g. `entity Cap(value: capacitance) { attribute
/// capacitance = value; }` would otherwise stamp the literal parameter
/// *name* `"value"` onto the leaf instance instead of the actual
/// argument (e.g. `"100nF"`). Param↔arg matching is positional; a
/// reference to a parameter for which no positional argument was supplied
/// (it relied on a default) is left untouched.
pub fn substitute_value_params(
    attributes: &mut std::collections::HashMap<String, String>,
    param_names: &[String],
    args: &[String],
) {
    if param_names.is_empty() || args.is_empty() {
        return;
    }
    for value in attributes.values_mut() {
        if let Some(idx) = param_names.iter().position(|p| p == value.trim()) {
            if let Some(arg) = args.get(idx) {
                *value = arg.trim().to_string();
            }
        }
    }
}

/// Like [`extract_expansion_recipes`] but also consults a caller-supplied
/// `overlay` map of entity attribute defaults for cross-file references.
/// In-file entities still take precedence (an entity redefined locally
/// would mask an imported one of the same name).
pub fn extract_expansion_recipes_with_overlay(
    source_file: &SourceFile,
    overlay: &std::collections::HashMap<String, std::collections::HashMap<String, String>>,
) -> std::collections::HashMap<String, bhdl_common::ExpansionRecipe> {
    use bhdl_common::{ExpansionRecipe, ExpansionInstance, ExpansionConnection, ExpansionEndpoint};
    use bhdl_ast::{Entity, HasName, SyntaxKind};
    use rowan::ast::AstNode;

    // Stage 6: pre-build a per-file index of (entity name → attribute
    // defaults), then overlay it on top of the caller's cross-file map.
    // When we create an ExpansionInstance referring to an entity, the
    // local index is checked first, then the overlay — so an in-file
    // redefinition wins over an imported one of the same name.
    let mut local_entity_attrs = overlay.clone();
    for (name, attrs) in extract_entity_attribute_index(source_file) {
        local_entity_attrs.insert(name, attrs);
    }

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
                        &local_entity_attrs,
                    );
                }

                // Extract standalone component declarations carrying P&R
                // layout intents (`C_vcc: Cap(100nF) for high_freq_bypass(...)`).
                // These parse as COMPONENT_INST nodes (not CONNECTION_STMT),
                // so the flow extractor above misses them; their wiring is
                // written separately and references the instance declared
                // here. Skip names already created by an inline flow form.
                {
                    use bhdl_ast::SyntaxKind;
                    use rowan::ast::AstNode;
                    for comp_node in expansion_block.syntax().children()
                        .filter(|n| n.kind() == SyntaxKind::COMPONENT_INST)
                    {
                        if let Some(inst) = parse_expansion_component_inst(
                            &comp_node, &local_entity_attrs,
                        ) {
                            if !recipe.instances.iter().any(|i| i.name == inst.name) {
                                recipe.instances.push(inst);
                            } else if let Some(existing) = recipe.instances.iter_mut()
                                .find(|i| i.name == inst.name)
                            {
                                // Inline flow already created it; just merge
                                // in the layout intents from the decl form.
                                if existing.layout_intents.is_empty() {
                                    existing.layout_intents = inst.layout_intents;
                                }
                            }
                        }
                    }
                }

                // Extract socket-pairing statements (`socket <held> in <socket>;`).
                // Stored on the recipe; the synthesizer reads them after
                // expansion and stamps `socketed_in = "<socket>"` on the
                // held instance's attributes so downstream consumers
                // (PnR, KiCad export) can suppress its footprint. Both
                // children stay on the BOM as separate orderable SKUs.
                for stmt in expansion_block.syntax().children()
                    .filter(|n| n.kind() == SyntaxKind::EXPANSION_SOCKET_STMT)
                {
                    let idents: Vec<String> = stmt.children_with_tokens()
                        .filter_map(|el| el.into_token())
                        .filter(|t| t.kind() == SyntaxKind::IDENT)
                        .map(|t| t.text().to_string())
                        .collect();
                    if idents.len() == 2 {
                        recipe.socket_pairings.insert(idents[0].clone(), idents[1].clone());
                    }
                }

                if !recipe.instances.is_empty()
                    || !recipe.connections.is_empty()
                    || !recipe.socket_pairings.is_empty()
                {
                    println!("  Extracted expansion recipe for '{}': {} instances, {} connections, {} internal nets, {} param defaults, {} socket pairings",
                        entity_name, recipe.instances.len(), recipe.connections.len(), recipe.internal_nets.len(), recipe.param_defaults.len(), recipe.socket_pairings.len());
                    recipes.insert(entity_name, recipe);
                }
            }
        }
    }

    recipes
}

/// Walk a source file's boards and pull each `variant <Name> { ... }`
/// block into a [`bhdl_common::variant::Variant`]. Returns a map of
/// board-name → variant-name → variant.
///
/// V0.1 surface (see `docs/spec/Board_SKU_Variants.md` §2.2):
/// the body of a variant block contains zero or more of these
/// statement forms:
///
/// - `dnp <instance>;`               (do-not-populate)
/// - `<instance>.value = <expr>;`    (value override; field must be `value`)
///
/// Anything else parses (the parser accepts any IDENT for the field
/// name to leave room for v0.2 extensions like `.mpn`) but the
/// analyzer rejects it here with a diagnostic.
pub fn extract_variant_blocks(
    source_file: &SourceFile,
) -> std::collections::HashMap<String, std::collections::HashMap<String, bhdl_common::variant::Variant>> {
    use bhdl_common::variant::Variant;
    use bhdl_ast::{Board, HasName, SyntaxKind};
    use rowan::ast::AstNode;

    let mut all: std::collections::HashMap<String, std::collections::HashMap<String, Variant>>
        = std::collections::HashMap::new();

    for item in source_file.items() {
        let board = match Board::cast(item.syntax().clone()) { Some(b) => b, None => continue };
        let board_name = board.name().map(|t| t.text().to_string()).unwrap_or_default();
        if board_name.is_empty() { continue; }

        for variant_node in board.syntax().children()
            .filter(|n| n.kind() == SyntaxKind::VARIANT_BLOCK)
        {
            // Variant name = the first IDENT token after VARIANT_KW.
            let variant_name = {
                let mut after_kw = false;
                let mut name = None;
                for el in variant_node.children_with_tokens() {
                    if let Some(t) = el.as_token() {
                        if t.kind() == SyntaxKind::VARIANT_KW { after_kw = true; continue; }
                        if after_kw && t.kind() == SyntaxKind::IDENT {
                            name = Some(t.text().to_string());
                            break;
                        }
                    }
                }
                match name { Some(n) => n, None => continue }
            };

            let mut v = Variant::new(variant_name.clone());

            for stmt in variant_node.children() {
                match stmt.kind() {
                    SyntaxKind::VARIANT_DNP_STMT => {
                        if let Some(inst) = first_token_text(&stmt, SyntaxKind::IDENT) {
                            v.dnp.insert(inst);
                        }
                    }
                    SyntaxKind::VARIANT_VALUE_OVERRIDE => {
                        // Body shape: IDENT '.' IDENT '=' EXPR ';'
                        // The first IDENT is the instance name, the
                        // second is the field name (must be "value"
                        // for v0.1).
                        let mut idents: Vec<String> = stmt.children_with_tokens()
                            .filter_map(|el| el.into_token())
                            .filter(|t| t.kind() == SyntaxKind::IDENT)
                            .map(|t| t.text().to_string())
                            .collect();
                        if idents.len() < 2 { continue; }
                        let field = idents.pop().unwrap();
                        let inst  = idents.pop().unwrap();
                        if field != "value" {
                            println!("  WARN: variant '{}' in board '{}' tries to override \
                                      `{}.{}` — only `.value` is supported in v0.1; ignored",
                                     variant_name, board_name, inst, field);
                            continue;
                        }
                        if let Some(expr) = text_between(&stmt, SyntaxKind::EQ, SyntaxKind::SEMI) {
                            v.value_overrides.insert(inst, expr);
                        }
                    }
                    _ => {}
                }
            }

            if v.is_empty() {
                println!("  Variant '{}'.'{}' is empty (base design unchanged).",
                         board_name, variant_name);
            } else {
                println!("  Extracted variant '{}'.'{}': {} value override(s), {} DNP",
                         board_name, variant_name,
                         v.value_overrides.len(), v.dnp.len());
            }
            all.entry(board_name.clone()).or_default()
                .insert(variant_name, v);
        }
    }

    all
}

/// Walk a source file's entities and pull each `design for <intent> { … }`
/// block into a [`DesignRecipe`].
///
/// Expressions are stored as raw source text — the evaluator (stage 3)
/// re-parses them. This keeps the extraction step language-agnostic and
/// avoids embedding expression semantics in `bhdl-common`.
pub fn extract_design_recipes(
    source_file: &SourceFile,
) -> std::collections::HashMap<String, std::collections::HashMap<String, bhdl_common::design::DesignRecipe>> {
    use bhdl_common::design::{DesignRecipe, DesignStatement};
    use bhdl_ast::{Entity, HasName, SyntaxKind};
    use rowan::ast::AstNode;

    let mut all: std::collections::HashMap<String, std::collections::HashMap<String, DesignRecipe>>
        = std::collections::HashMap::new();

    for item in source_file.items() {
        let entity = match Entity::cast(item.syntax().clone()) {
            Some(e) => e,
            None => continue,
        };
        let entity_name = entity.name().map(|t| t.text().to_string()).unwrap_or_default();
        if entity_name.is_empty() { continue; }

        for design_node in entity.syntax().children()
            .filter(|n| n.kind() == SyntaxKind::DESIGN_BLOCK)
        {
            // Intent name = the first IDENT token after FOR_KW. If no
            // FOR_KW is present, this is a plain `design { }` block
            // (spec §5.2 scenario A) — the recipe runs unconditionally
            // for every instance of the entity. We store these under
            // the sentinel name "<plain>" so the same HashMap shape
            // can hold both forms.
            let intent_name = {
                let mut after_for = false;
                let mut name = None;
                for el in design_node.children_with_tokens() {
                    if let Some(t) = el.as_token() {
                        if t.kind() == SyntaxKind::FOR_KW { after_for = true; continue; }
                        if after_for && t.kind() == SyntaxKind::IDENT {
                            name = Some(t.text().to_string());
                            break;
                        }
                    }
                }
                name.unwrap_or_else(|| "<plain>".to_string())
            };

            let mut recipe = DesignRecipe::new(entity_name.clone(), intent_name.clone());
            // Two collection passes over the design block's children:
            // (1) declarative statements (Stages 1-4 surface), (2) the
            // Stage-5 foreign-language body hook. Mutual exclusion is
            // checked after extraction so we can produce a clear error
            // rather than silently dropping one side.
            let mut inputs_decl  = Vec::new();
            let mut outputs_decl = Vec::new();
            let mut body_hook    = None;
            for child in design_node.children() {
                if let Some(s) = extract_design_statement(&child) {
                    recipe.statements.push(s);
                    continue;
                }
                match child.kind() {
                    SyntaxKind::DESIGN_INPUTS_DECL  => inputs_decl  = extract_design_name_list(&child),
                    SyntaxKind::DESIGN_OUTPUTS_DECL => outputs_decl = extract_design_name_list(&child),
                    SyntaxKind::DESIGN_BODY_HOOK    => body_hook    = extract_design_body_hook(&child),
                    _ => {}
                }
            }

            if let Some((language, source)) = body_hook {
                if !recipe.statements.is_empty() {
                    println!("  WARN: design recipe for '{entity_name}'.'{intent_name}' mixes \
                              declarative statements with a `body` hook — the hook wins, \
                              declarative statements ignored.");
                    recipe.statements.clear();
                }
                recipe.body = Some(bhdl_common::design::DesignBody {
                    language,
                    inputs:  inputs_decl,
                    outputs: outputs_decl,
                    source,
                });
            }

            if recipe.has_statements() || recipe.has_body() {
                if recipe.has_body() {
                    let b = recipe.body.as_ref().unwrap();
                    println!("  Extracted design recipe for '{entity_name}'.'{intent_name}': \
                              body hook ({}, {} bytes, {} input(s), {} output(s))",
                              b.language, b.source.len(), b.inputs.len(), b.outputs.len());
                } else {
                    println!("  Extracted design recipe for '{entity_name}'.'{intent_name}': \
                              {} statement(s)", recipe.statements.len());
                }
                all.entry(entity_name.clone()).or_default()
                    .insert(intent_name, recipe);
            }
        }
    }

    all
}

/// Extract one [`StressRecipe`] per entity that declares a
/// `simulation { stress { } }` block (Vendor_Simulation_Blocks.md §4).
/// Keyed by entity name. Entities without a stress block are absent from the
/// map (sign-off then uses the hardcoded reference model). The reserved
/// `model { }` sub-block (§5) is ignored here.
pub fn extract_stress_recipes(
    source_file: &SourceFile,
) -> std::collections::HashMap<String, bhdl_common::stress::StressRecipe> {
    use bhdl_common::stress::StressRecipe;
    use bhdl_ast::{Entity, HasName, SyntaxKind};
    use rowan::ast::AstNode;

    let mut all: std::collections::HashMap<String, StressRecipe> =
        std::collections::HashMap::new();

    for item in source_file.items() {
        let entity = match Entity::cast(item.syntax().clone()) {
            Some(e) => e,
            None => continue,
        };
        let entity_name = entity.name().map(|t| t.text().to_string()).unwrap_or_default();
        if entity_name.is_empty() { continue; }

        // entity → SIM_BLOCK → { STRESS_BLOCK statements, CHECK_BLOCK requires }.
        // One recipe per entity carries both vendor-model surfaces: §4 stress
        // and the T2/ERC025 part-carried checks (docs/spec/ERC.md).
        let mut recipe = StressRecipe::new(entity_name.clone());
        for sim_node in entity.syntax().children()
            .filter(|n| n.kind() == SyntaxKind::SIM_BLOCK)
        {
            for stress_node in sim_node.children()
                .filter(|n| n.kind() == SyntaxKind::STRESS_BLOCK)
            {
                for child in stress_node.children() {
                    if let Some(s) = extract_stress_statement(&child) {
                        recipe.statements.push(s);
                    }
                }
            }
            for check_node in sim_node.children()
                .filter(|n| n.kind() == SyntaxKind::CHECK_BLOCK)
            {
                for child in check_node.children() {
                    if child.kind() != SyntaxKind::DESIGN_REQUIRE_STMT { continue; }
                    let Some(condition) =
                        text_between(&child, SyntaxKind::REQUIRE_KW, SyntaxKind::ELSE_KW)
                    else { continue };
                    let Some(raw) = first_token_text(&child, SyntaxKind::STRING)
                    else { continue };
                    recipe.checks.push(bhdl_common::stress::CheckRequire {
                        condition,
                        message: raw.trim_matches('"').to_string(),
                    });
                }
            }
        }
        if recipe.has_content() {
            println!("  Extracted stress recipe for '{entity_name}': {} statement(s), {} check(s)",
                     recipe.statements.len(), recipe.checks.len());
            all.insert(entity_name.clone(), recipe);
        }
    }

    all
}

/// Extract one [`ModelRecipe`] per entity that declares a
/// `simulation { model { } }` block (Vendor_Simulation_Blocks.md §5). Keyed by
/// entity name; only the primitive-composition `node <net> <role> = <expr>;`
/// statements are captured (builtin/vendor forms are skipped). Entities without
/// a model block are absent (the converter then uses its hardcoded decomposition).
pub fn extract_model_recipes(
    source_file: &SourceFile,
) -> std::collections::HashMap<String, bhdl_common::model::ModelRecipe> {
    use bhdl_common::model::{ModelNode, ModelRecipe, ModelRole};
    use bhdl_ast::{Entity, HasName, SyntaxKind};
    use rowan::ast::AstNode;

    let mut all: std::collections::HashMap<String, ModelRecipe> =
        std::collections::HashMap::new();

    for item in source_file.items() {
        let entity = match Entity::cast(item.syntax().clone()) {
            Some(e) => e,
            None => continue,
        };
        let entity_name = entity.name().map(|t| t.text().to_string()).unwrap_or_default();
        if entity_name.is_empty() { continue; }

        for sim_node in entity.syntax().children()
            .filter(|n| n.kind() == SyntaxKind::SIM_BLOCK)
        {
            for model_node in sim_node.children()
                .filter(|n| n.kind() == SyntaxKind::MODEL_BLOCK)
            {
                let mut recipe = ModelRecipe::new(entity_name.clone());
                for stmt in model_node.children()
                    .filter(|n| n.kind() == SyntaxKind::MODEL_NODE_STMT)
                {
                    // node <net> <role> = <expr>;  — the leading `node` is itself
                    // a (contextual-keyword) IDENT token, so skip it: the next two
                    // direct IDENT tokens are the net and the role.
                    let idents: Vec<String> = stmt.children_with_tokens()
                        .filter_map(|el| el.into_token())
                        .filter(|t| t.kind() == SyntaxKind::IDENT)
                        .map(|t| t.text().to_string())
                        .skip(1)
                        .take(2)
                        .collect();
                    let (Some(net), Some(role_str)) = (idents.first(), idents.get(1)) else { continue };
                    let role = match role_str.as_str() {
                        "source" => ModelRole::Source,
                        "draws" => ModelRole::Draws,
                        _ => continue, // unknown role keyword — skip
                    };
                    let Some(expr) = text_between(&stmt, SyntaxKind::EQ, SyntaxKind::SEMI) else { continue };
                    recipe.nodes.push(ModelNode { net: net.clone(), role, expr });
                }
                // Vendor IBIS reference (§5 form #1):
                // `ibis "path" component "NAME" [corner c] [map { P = sig; }];`
                for stmt in model_node.children()
                    .filter(|n| n.kind() == SyntaxKind::MODEL_IBIS_STMT)
                {
                    let strings: Vec<String> = stmt.children_with_tokens()
                        .filter_map(|el| el.into_token())
                        .filter(|t| t.kind() == SyntaxKind::STRING)
                        .map(|t| t.text().trim_matches('"').to_string())
                        .collect();
                    let idents: Vec<String> = stmt.children_with_tokens()
                        .filter_map(|el| el.into_token())
                        .filter(|t| t.kind() == SyntaxKind::IDENT)
                        .map(|t| t.text().to_string())
                        .collect();
                    let Some(path) = strings.first() else { continue };
                    let component = strings.get(1).cloned().unwrap_or_default();
                    // corner = the ident following "corner", if any.
                    let corner = idents.iter().position(|t| t == "corner")
                        .and_then(|i| idents.get(i + 1))
                        .cloned()
                        .unwrap_or_default();
                    // map { PIN = sig; … } — pairs of idents around EQ inside
                    // the brace run after "map".
                    let mut pin_map = Vec::new();
                    if let Some(mi) = idents.iter().position(|t| t == "map") {
                        let pairs = &idents[mi + 1..];
                        for w in pairs.chunks(2) {
                            if let [a, b] = w {
                                pin_map.push((a.clone(), b.clone()));
                            }
                        }
                    }
                    recipe.ibis.push(bhdl_common::model::IbisRef {
                        path: path.clone(),
                        component,
                        corner,
                        pin_map,
                    });
                }
                if recipe.has_nodes() || !recipe.ibis.is_empty() {
                    println!("  Extracted model recipe for '{entity_name}': {} node(s){}",
                             recipe.nodes.len(),
                             if recipe.ibis.is_empty() { "" } else { " + ibis ref(s)" });
                    all.insert(entity_name.clone(), recipe);
                }
            }
        }
    }

    all
}

/// Extract one statement of a `stress { }` block. `const`/`require` share the
/// design-block node kinds (PARAM_DECL / DESIGN_REQUIRE_STMT); the stress
/// assignment is the dotted-LHS STRESS_ASSIGNMENT.
fn extract_stress_statement(
    node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
) -> Option<bhdl_common::stress::StressStatement> {
    use bhdl_common::stress::StressStatement;
    use bhdl_ast::SyntaxKind;
    match node.kind() {
        SyntaxKind::PARAM_DECL => {
            // const NAME = EXPR;
            let name = first_token_text(node, SyntaxKind::IDENT)?;
            let expr = text_between(node, SyntaxKind::EQ, SyntaxKind::SEMI)?;
            Some(StressStatement::Let { name, expr })
        }
        SyntaxKind::DESIGN_REQUIRE_STMT => {
            // require EXPR else "MSG";
            let condition = text_between(node, SyntaxKind::REQUIRE_KW, SyntaxKind::ELSE_KW)?;
            let raw = first_token_text(node, SyntaxKind::STRING)?;
            let message = raw.trim_matches('"').to_string();
            Some(StressStatement::Require { condition, message })
        }
        SyntaxKind::STRESS_ASSIGNMENT => {
            // CHILD.AXIS = EXPR;  — the LHS is the first two IDENT tokens
            // (separated by DOT), the RHS is the text between EQ and SEMI.
            let idents: Vec<String> = node.children_with_tokens()
                .filter_map(|el| el.into_token())
                .filter(|t| t.kind() == SyntaxKind::IDENT)
                .map(|t| t.text().to_string())
                .take(2)
                .collect();
            let child_name = idents.first()?.clone();
            let axis = idents.get(1)?.clone();
            let expr = text_between(node, SyntaxKind::EQ, SyntaxKind::SEMI)?;
            Some(StressStatement::Assign { child_name, axis, expr })
        }
        _ => None,
    }
}

/// Collect IDENT names from a DESIGN_INPUTS_DECL or DESIGN_OUTPUTS_DECL
/// node. Names appear in source order; semicolons are dropped.
fn extract_design_name_list(
    node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
) -> Vec<String> {
    use bhdl_ast::SyntaxKind;
    let mut names = Vec::new();
    // Skip the leading IDENT (the `inputs`/`outputs` keyword itself —
    // tracked as IDENT because it's contextual) and the L_BRACE.
    let mut seen_brace = false;
    for el in node.children_with_tokens() {
        if let Some(t) = el.as_token() {
            match t.kind() {
                SyntaxKind::L_BRACE => seen_brace = true,
                SyntaxKind::IDENT if seen_brace => names.push(t.text().to_string()),
                _ => {}
            }
        }
    }
    names
}

/// Extract the `(language, source)` pair from a DESIGN_BODY_HOOK node.
/// The body's RAW_STRING token is unwrapped of its `r#"..."#` delimiters
/// here so the evaluator receives the script source verbatim.
fn extract_design_body_hook(
    node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
) -> Option<(String, String)> {
    use bhdl_ast::SyntaxKind;
    let mut after_body = false;
    let mut language = None;
    let mut raw = None;
    for el in node.children_with_tokens() {
        if let Some(t) = el.as_token() {
            match t.kind() {
                SyntaxKind::BODY_KW => after_body = true,
                SyntaxKind::IDENT if after_body && language.is_none() => {
                    language = Some(t.text().to_string());
                }
                SyntaxKind::RAW_STRING => {
                    raw = Some(t.text().to_string());
                    break;
                }
                _ => {}
            }
        }
    }
    let language = language?;
    let raw = raw?;
    // Strip the `r#"..."#` delimiters. Opening: `r` + n hashes + `"`.
    // Closing: `"` + n hashes. The body lexer guarantees the literal is
    // well-formed, so we can locate the boundaries by counting hashes.
    let bytes = raw.as_bytes();
    if bytes.first() != Some(&b'r') { return None; }
    let mut i = 1;
    while bytes.get(i) == Some(&b'#') { i += 1; }
    let n_hashes = i - 1;
    if bytes.get(i) != Some(&b'"') { return None; }
    let body_start = i + 1;
    // The close is `"` + n_hashes hashes — those are the last 1+n_hashes
    // bytes of the literal.
    let body_end = raw.len().saturating_sub(1 + n_hashes);
    if body_end < body_start { return None; }
    let source = raw[body_start..body_end].to_string();
    Some((language, source))
}

/// Translate a single statement node inside a `design { }` block into a
/// structured [`DesignStatement`]. Returns `None` for non-statement children
/// (whitespace, error nodes the parser recovered into).
fn extract_design_statement(
    node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
) -> Option<bhdl_common::design::DesignStatement> {
    use bhdl_common::design::DesignStatement;
    use bhdl_ast::SyntaxKind;
    match node.kind() {
        SyntaxKind::PARAM_DECL => {
            // const NAME = EXPR;
            let name = first_token_text(node, SyntaxKind::IDENT)?;
            let expr = text_between(node, SyntaxKind::EQ, SyntaxKind::SEMI)?;
            Some(DesignStatement::Let { name, expr })
        }
        SyntaxKind::DESIGN_REQUIRE_STMT => {
            // require EXPR else "MSG";
            let condition = text_between(node, SyntaxKind::REQUIRE_KW, SyntaxKind::ELSE_KW)?;
            let raw = first_token_text(node, SyntaxKind::STRING)?;
            let message = raw.trim_matches('"').to_string();
            Some(DesignStatement::Require { condition, message })
        }
        SyntaxKind::DESIGN_ASSIGNMENT => {
            // CHILD = EXPR;
            let child_name = first_token_text(node, SyntaxKind::IDENT)?;
            let expr = text_between(node, SyntaxKind::EQ, SyntaxKind::SEMI)?;
            Some(DesignStatement::Assign { child_name, expr })
        }
        _ => None,
    }
}

/// Return the text of the first token of the given kind under `node`.
fn first_token_text(
    node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
    kind: bhdl_ast::SyntaxKind,
) -> Option<String> {
    node.children_with_tokens()
        .filter_map(|el| el.into_token())
        .find(|t| t.kind() == kind)
        .map(|t| t.text().to_string())
}

/// Return the concatenated text of all elements *between* the first token
/// of kind `start` and the first token of kind `end` (after `start`).
/// Trims whitespace. None when either anchor is missing.
fn text_between(
    node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
    start: bhdl_ast::SyntaxKind,
    end: bhdl_ast::SyntaxKind,
) -> Option<String> {
    let elements: Vec<_> = node.children_with_tokens().collect();
    let start_idx = elements.iter()
        .position(|el| el.as_token().map(|t| t.kind() == start).unwrap_or(false))?;
    let end_idx = elements.iter().enumerate()
        .skip(start_idx + 1)
        .find(|(_, el)| el.as_token().map(|t| t.kind() == end).unwrap_or(false))
        .map(|(i, _)| i)?;
    let mut text = String::new();
    for el in &elements[start_idx + 1 .. end_idx] {
        match el {
            rowan::NodeOrToken::Node(n) => text.push_str(&n.text().to_string()),
            rowan::NodeOrToken::Token(t) => text.push_str(t.text()),
        }
    }
    Some(text.trim().to_string())
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
    local_entity_attrs: &std::collections::HashMap<String, std::collections::HashMap<String, String>>,
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
            local_entity_attrs,
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
    local_entity_attrs: &std::collections::HashMap<String, std::collections::HashMap<String, String>>,
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
                    // Stage 6: pre-populate with the called entity's
                    // attribute defaults (looked up in the in-file index
                    // built at the top of extract_expansion_recipes).
                    // Missing → empty map; an explicit instance-level
                    // attribute override would land here at a later
                    // extraction stage (not yet implemented).
                    attributes: local_entity_attrs.get(comp_type)
                        .cloned()
                        .unwrap_or_default(),
                    // Inline-flow instantiations carry no intent clause;
                    // intents come via the standalone-decl form, merged in
                    // the COMPONENT_INST pass. (See parse_expansion_component_inst.)
                    layout_intents: Vec::new(),
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

/// Extract a standalone component declaration inside an `expansion { }`
/// block — the P&R-intent form:
///
/// ```text
/// C_vcc: Cap(100nF) for high_freq_bypass(rail: VCC, return: GND1, loop_area_max: 1.5mm2);
/// ```
///
/// This parses as a COMPONENT_INST node (not a CONNECTION_STMT), so it
/// isn't picked up by the flow-chain extractor. The wiring is written
/// separately (`VCC -> C_vcc.1; C_vcc.2 -> GND1;`) and references the
/// instance this declares. Returns the `ExpansionInstance` (with any
/// `for INTENT(...)` lowered to typed `LayoutIntent`s), or `None` if the
/// node doesn't have the `name: Type(...)` shape.
fn parse_expansion_component_inst(
    node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
    local_entity_attrs: &std::collections::HashMap<String, std::collections::HashMap<String, String>>,
) -> Option<bhdl_common::ExpansionInstance> {
    use bhdl_ast::SyntaxKind;

    // COMPONENT_INST tokens: IDENT(name) COLON IDENT(type) PARAM_LIST(...) [INTENT_CLAUSE].
    // Pull the first two IDENT tokens at the node's top level for name + type.
    let idents: Vec<String> = node
        .children_with_tokens()
        .filter_map(|el| el.into_token())
        .filter(|t| t.kind() == SyntaxKind::IDENT)
        .map(|t| t.text().to_string())
        .collect();
    if idents.len() < 2 {
        return None;
    }
    let name = idents[0].clone();
    let component_type = idents[1].clone();

    // Params: text inside the PARAM_LIST, comma-split (matches the
    // flow-form extractor's convention of raw param-expression text).
    let params: Vec<String> = node
        .children()
        .find(|n| n.kind() == SyntaxKind::PARAM_LIST)
        .map(|pl| {
            let t = pl.text().to_string();
            let t = t.trim().trim_start_matches('(').trim_end_matches(')').trim();
            if t.is_empty() {
                Vec::new()
            } else {
                t.split(',').map(|s| s.trim().to_string()).collect()
            }
        })
        .unwrap_or_default();

    // Layout intents from any INTENT_CLAUSE → INTENT_CALL child.
    let mut layout_intents = Vec::new();
    for clause in node.children().filter(|n| n.kind() == SyntaxKind::INTENT_CLAUSE) {
        if let Some(call) = clause.children().find(|n| n.kind() == SyntaxKind::INTENT_CALL) {
            if let Some(intent) = lower_layout_intent(&call) {
                layout_intents.push(intent);
            }
        }
    }

    Some(bhdl_common::ExpansionInstance {
        name,
        component_type,
        params,
        attributes: local_entity_attrs.get(&idents[1]).cloned().unwrap_or_default(),
        layout_intents,
    })
}

/// Lower an `INTENT_CALL` syntax node to a typed
/// [`bhdl_common::intent::vocabulary::LayoutIntent`].
///
/// Reads the intent name + named parameters and constructs the matching
/// variant. Pin-valued params become `PinRef::HostPin` (resolved later,
/// at P&R lowering time — handshake §8.3). Distance/area params are
/// parsed to `f32`; when omitted, the vocabulary `defaults` fill in.
/// Returns `None` for an unrecognized intent name (warn-and-degrade —
/// the synth side simply doesn't attach it; P&R never sees a bad value).
///
/// v0: the three decoupling/power-integrity kinds needed for the ATmega
/// milestone are lowered. The remaining vocabulary kinds parse but lower
/// to `None` until their stdlib use appears (the match arm is the single
/// extension point).
fn lower_layout_intent(
    call: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
) -> Option<bhdl_common::intent::vocabulary::LayoutIntent> {
    use bhdl_common::intent::vocabulary::{defaults, LayoutIntent, PinRef};
    use bhdl_ast::SyntaxKind;

    // Intent name = first IDENT token directly under INTENT_CALL.
    let name = call
        .children_with_tokens()
        .filter_map(|el| el.into_token())
        .find(|t| t.kind() == SyntaxKind::IDENT)?
        .text()
        .to_string();

    // Collect named params: name → raw value text (trimmed).
    let mut named: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Some(params) = call.children().find(|n| n.kind() == SyntaxKind::INTENT_PARAMS) {
        for np in params.children().filter(|n| n.kind() == SyntaxKind::INTENT_NAMED_PARAM) {
            // INTENT_NAMED_PARAM: IDENT COLON <value-expr>. The key is the
            // first IDENT; the value is everything after the colon.
            let key = np
                .children_with_tokens()
                .filter_map(|el| el.into_token())
                .find(|t| t.kind() == SyntaxKind::IDENT)
                .map(|t| t.text().to_string());
            let full = np.text().to_string();
            let value = full.split_once(':').map(|(_, v)| v.trim().to_string());
            if let (Some(k), Some(v)) = (key, value) {
                named.insert(k, v);
            }
        }
    }

    // Helpers.
    let pin = |k: &str| -> Option<PinRef> {
        named.get(k).map(|s| PinRef::HostPin(s.trim().to_string()))
    };
    // Parse the leading numeric portion of a value like "1.5mm2" / "2mm".
    let num = |k: &str| -> Option<f32> {
        named.get(k).and_then(|s| {
            let digits: String = s
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
                .collect();
            digits.parse::<f32>().ok()
        })
    };

    match name.as_str() {
        "high_freq_bypass" => Some(LayoutIntent::HighFreqBypass {
            rail: pin("rail")?,
            return_pin: pin("return")?,
            loop_area_max_mm2: num("loop_area_max")?,
            proximity_max_mm: num("proximity_max")
                .unwrap_or(defaults::HIGH_FREQ_BYPASS_PROXIMITY_MM),
        }),
        "bulk_reservoir" => Some(LayoutIntent::BulkReservoir {
            rail: pin("rail")?,
            return_pin: pin("return")?,
            proximity_max_mm: num("proximity_max")
                .unwrap_or(defaults::BULK_RESERVOIR_PROXIMITY_MM),
        }),
        "analog_ref_filter" => Some(LayoutIntent::AnalogRefFilter {
            ref_pin: pin("ref_pin")?,
            return_pin: pin("return")?,
            proximity_max_mm: num("proximity_max")
                .unwrap_or(defaults::ANALOG_REF_FILTER_PROXIMITY_MM),
        }),
        "switching_input_filter" => Some(LayoutIntent::SwitchingInputFilter {
            rail: pin("rail")?,
            return_pin: pin("return")?,
            loop_area_max_mm2: num("loop_area_max")?,
            switch_node_keepaway_mm: num("switch_node_keepaway")
                .unwrap_or(defaults::SWITCHING_INPUT_FILTER_KEEPAWAY_MM),
        }),
        other => {
            // Recognized-by-vocabulary but not yet lowered here, or an
            // unknown kind. Warn-and-degrade: attach nothing.
            log::warn!(
                "expansion intent `{}` not lowered (no v0 analyzer recipe yet); skipping",
                other
            );
            None
        }
    }
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
        let layer_stackup = layout_def.layer_stackup();
        let places = layout_def.places();
        let region_places = layout_def.region_places();
        let (outline_rect, outline_polygon) = match layout_def.outline() {
            Some(bhdl_ast::LayoutOutline::Rect { w, h }) => (Some((w, h)), None),
            Some(bhdl_ast::LayoutOutline::Polygon(pts)) => (None, Some(pts)),
            None => (None, None),
        };
        let mounting_holes = layout_def.mounting_holes();
        let keepouts = layout_def.keepouts();
        let mech_check = layout_def.mech_check();

        definitions.insert(entity_name.clone(), LayoutDefinition {
            entity_name,
            package,
            layer_stackup,
            places,
            region_places,
            outline_rect,
            outline_polygon,
            mounting_holes,
            keepouts,
            mech_check,
        });
    }

    definitions
}

// Remove the test module declaration as tests are now in the tests/ directory
// #[cfg(test)]
// mod tests; // Load tests from tests.rs when running cargo test

// The #[cfg(test)] mod tests { ... } block should follow here (as kept by the user)


