// Needed for analyze function signature and AST traversal
use bhdl_ast::SourceFile;
use rowan::ast::AstNode; // For source_file.syntax()

// Declare modules
pub mod types;
mod helpers;
mod symbol_table;
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

// Use items needed directly in the analyze function
use types::{AnalysisResult, ResolvedConstants};
use pass1::populate_global_scope_and_build_definition_scopes;
use pass2::{visit_node_pass2_references, Pass2Context};
use pass3::{visit_node_pass3_const_eval, Pass3Context};
use pass4::{visit_node_pass4_bounds_checks, Pass4Context};
use power_analysis::{analyze_power_domains, PowerAnalysisContext};
use component_inference::ComponentInferenceContext;
use power_sequencing::PowerSequenceGenerator;

// Main analysis function
pub fn analyze(source_file: &SourceFile) -> AnalysisResult {
    println!("Starting analysis...");

    // Result accumulator
    let mut diagnostics = Vec::new();
    // Initialize resolved_constants using the type alias from the types module
    let mut resolved_constants = ResolvedConstants::new();

    // Pass 1: Build scopes
    let (global_scope, definition_scopes) = populate_global_scope_and_build_definition_scopes(source_file);
    println!("Analyzer: Pass 1 complete. Global symbols: {}, Definition scopes: {}",
             global_scope.children.len(), // Assuming SymbolTable has a len() method or similar -> USE children.len()
             definition_scopes.len());

    // Pass 2: Reference Checks
    println!("Analyzer: Starting Pass 2 - References & Basic Types...");
    let mut pass2_context = Pass2Context::new(&global_scope, &definition_scopes, source_file.syntax(), &mut diagnostics);
    visit_node_pass2_references(source_file.syntax(), &mut pass2_context);
    println!("Analyzer: Pass 2 complete. Diagnostics found so far: {}", diagnostics.len());


    // Pass 3: Constant Evaluation
    println!("Analyzer: Starting Pass 3 - Constant Evaluation...");
    // Pass diagnostics vec and resolved_constants map mutably
    let diag_count_before_pass3 = diagnostics.len(); // Get length BEFORE creating context
    let mut pass3_context = Pass3Context::new(
        &global_scope,
        &definition_scopes,
        source_file.syntax(),
        &mut resolved_constants,
        &mut diagnostics, // Pass cumulative diagnostics vec
    );
    visit_node_pass3_const_eval(source_file.syntax(), &mut pass3_context);
    let pass3_diag_count = diagnostics.len() - diag_count_before_pass3;
    println!("Analyzer: Pass 3 complete. Constants evaluated: {}, Diagnostics added in pass: {}",
             resolved_constants.len(), pass3_diag_count);


    // Pass 4: Bounds Checks
    println!("Analyzer: Starting Pass 4 - Bounds Checks...");
    // Pass diagnostics vec mutably
    let diag_count_before_pass4 = diagnostics.len(); // Get length BEFORE creating context
    let mut pass4_context = Pass4Context::new(
        &global_scope,
        &definition_scopes,
        &resolved_constants, // Pass constants immutably
        &mut diagnostics, // Pass cumulative diagnostics vec
    );
    visit_node_pass4_bounds_checks(source_file.syntax(), &mut pass4_context);
    let pass4_diag_count = diagnostics.len() - diag_count_before_pass4;
    println!("Analyzer: Pass 4 complete. Diagnostics added in pass: {}", pass4_diag_count);

    // Pass 5: Power Analysis
    println!("Analyzer: Starting Pass 5 - Power Analysis...");
    let power_context = analyze_power_domains(source_file.syntax());
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

    // Pass 7: Power Sequencing
    println!("Analyzer: Starting Pass 7 - Power Sequencing...");
    let mut power_sequencing = PowerSequenceGenerator::new();
    generate_power_sequences(&mut power_sequencing, &power_context);
    let startup_steps = power_sequencing.startup_sequence.len();
    let shutdown_steps = power_sequencing.shutdown_sequence.len();
    let sequencing_warnings = power_sequencing.warnings.len();
    println!("Analyzer: Pass 7 complete. Startup steps: {}, Shutdown steps: {}, Warnings: {}", 
             startup_steps, shutdown_steps, sequencing_warnings);

    // Convert power analysis errors to diagnostics
    for error in &power_context.errors {
        diagnostics.push(types::Diagnostic {
            message: format!("Power Analysis: {}", error),
            range: rowan::TextRange::empty(rowan::TextSize::from(0)),
        });
    }

    // Convert power analysis warnings to diagnostics
    for warning in &power_context.warnings {
        diagnostics.push(types::Diagnostic {
            message: format!("Power Warning: {}", warning),
            range: rowan::TextRange::empty(rowan::TextSize::from(0)),
        });
    }

    // Convert component inference warnings to diagnostics
    for warning in &component_inference.warnings {
        diagnostics.push(types::Diagnostic {
            message: format!("Component Inference: {}", warning),
            range: rowan::TextRange::empty(rowan::TextSize::from(0)),
        });
    }

    // Convert power sequencing warnings to diagnostics
    for warning in &power_sequencing.warnings {
        diagnostics.push(types::Diagnostic {
            message: format!("Power Sequencing: {}", warning),
            range: rowan::TextRange::empty(rowan::TextSize::from(0)),
        });
    }

    println!("Analysis finished. Found {} total diagnostics.", diagnostics.len());

    AnalysisResult {
        global_scope, // Move ownership
        definition_scopes, // Move ownership
        diagnostics, // Move ownership
        resolved_constants, // Move ownership
        power_analysis: power_context, // Move ownership
        component_inference, // Move ownership
        power_sequencing, // Move ownership
    }
}

/// Analyze components for inference based on circuit context
fn analyze_components_for_inference(
    syntax: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
    component_inference: &mut ComponentInferenceContext,
    power_context: &PowerAnalysisContext,
) {
    use bhdl_ast::SyntaxKind;
    use component_inference::{CircuitRequirements, CircuitContext};

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
    use component_inference::{CircuitRequirements, CircuitContext};

    match node.kind() {
        SyntaxKind::FLOW_EXPR => {
            // Process flow expressions which contain inline component instantiations in v2.0
            use bhdl_ast::flow::{FlowExpr, FlowElement, ComponentInstantiation};
            
            if let Some(flow_expr) = FlowExpr::cast(node.clone()) {
                // Process each element in the flow expression
                for element in flow_expr.elements() {
                    if let FlowElement::ComponentInstantiation(comp_inst) = element {
                        process_component_instantiation_v2(&comp_inst, component_inference, power_context);
                    }
                }
            }
        }
        SyntaxKind::CONNECTION_STMT => {
            // Process connection statements which may contain inline component instantiations
            // e.g., VCC -> R1(10k).1 -> LED1(red).A -> GND;
            let stmt_text = node.to_string();
            
            // Look for inline component instantiations in the connection
            // Pattern: ComponentType(params) or name: ComponentType(params)
            process_connection_for_components(node, component_inference, power_context);
        }
        SyntaxKind::COMPONENT_INST => {
            // Analyze component instantiation for inference opportunities
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
                        .or(Some(3.3)) // Default to 3.3V if no power domain found
                } else {
                    Some(3.3) // Default to 3.3V
                };
                
                // Create requirements based on context and actual power domain
                let requirements = CircuitRequirements {
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
                
                // Check if this is near an LED by looking at the connection context
                if let Some(connection_stmt) = node.ancestors().find(|n| n.kind() == SyntaxKind::CONNECTION_STMT) {
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
                    if let Some(connection_stmt) = node.ancestors().find(|n| n.kind() == SyntaxKind::CONNECTION_STMT) {
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
    use component_inference::{CircuitRequirements, CircuitContext, ParameterValue, InferredParameter};
    use bhdl_ast::HasName;
    
    // Extract component type from the instantiation
    let component_type = comp_inst.component_type()
        .map(|t| t.text().to_string())
        .unwrap_or_else(|| "Unknown".to_string());
    
    // Extract parameters from the instantiation
    let mut extracted_params = Vec::new();
    for param_assign in comp_inst.parameter_assignments() {
        if let (Some(name), Some(value)) = (param_assign.name(), param_assign.value()) {
            let param_name = name.text().to_string();
            let param_value = value.syntax().text().to_string();
            
            // Parse parameter value based on name
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
    let supply_voltage = power_context.component_domains.get(&instance_name)
        .and_then(|domain_name| power_context.domains.get(domain_name))
        .map(|domain| domain.voltage)
        .or(Some(3.3)); // Default to 3.3V
    
    // Check for package constraint in extracted parameters
    let package_constraint = extracted_params.iter()
        .find(|p| p.name == "package")
        .and_then(|p| match &p.value {
            ParameterValue::String(s) => Some(s.clone()),
            _ => None
        });
    
    // Create requirements
    let requirements = CircuitRequirements {
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
    
    // Infer component parameters
    if let Some(mut suggestion) = component_inference.infer_component_parameters(
        &component_type, &requirements, &circuit_context
    ) {
        // Set the instance name
        suggestion.instance_name = Some(instance_name.clone());
        
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

/// Process connection statement for inline component instantiations
fn process_connection_for_components(
    node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
    component_inference: &mut ComponentInferenceContext,
    power_context: &PowerAnalysisContext,
) {
    use bhdl_ast::flow::{FlowExpr, FlowElement, ComponentInstantiation};
    
    // Look for flow expressions within the connection statement
    for child in node.children() {
        if let Some(flow_expr) = FlowExpr::cast(child.clone()) {
            // Process each element in the flow expression
            for element in flow_expr.elements() {
                if let FlowElement::ComponentInstantiation(comp_inst) = element {
                    process_component_instantiation_v2(&comp_inst, component_inference, power_context);
                }
            }
        }
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
    
    // In BHDL v2.0, instance names are auto-generated by the toolchain
    // The pattern "name: Component(...)" creates a net, not an instance name
    // For example: "feedback_r1: Res(10k).1" creates:
    // - A net named "feedback_r1" 
    // - A resistor instance with auto-generated refdes like "R1"
    
    // Return None to let the synthesizer generate proper reference designators
    None
}

/// Helper function to extract component instance name from COMPONENT_INST node
fn get_component_instance_name(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>) -> Option<String> {
    use bhdl_ast::SyntaxKind;
    
    // Look for the instance name in the AST structure
    // Format is typically: ComponentType(params).pin or ComponentType(params)
    // We want to extract the full instance identifier
    
    // For now, just generate a name from the component type and its position
    // This could be enhanced to look for actual instance names in the future
    node.children_with_tokens()
        .find_map(|child| {
            if let rowan::NodeOrToken::Token(token) = child {
                if token.kind() == SyntaxKind::IDENT {
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

// Remove the test module declaration as tests are now in the tests/ directory
// #[cfg(test)]
// mod tests; // Load tests from tests.rs when running cargo test

// The #[cfg(test)] mod tests { ... } block should follow here (as kept by the user)


