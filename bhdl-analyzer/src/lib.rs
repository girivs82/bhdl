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
        SyntaxKind::COMPONENT_INST => {
            // Analyze component instantiation for inference opportunities
            if let Some(ident_token) = node.first_token() {
                let component_type = ident_token.text();
                
                // Create requirements based on context
                let requirements = CircuitRequirements {
                    supply_voltage: Some(3.3), // Default to 3.3V
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
                
                // Check if this is near an LED
                if node.to_string().contains("LED") || 
                   node.parent().map_or(false, |p| p.to_string().contains("LED")) {
                    circuit_context.has_led_in_series = true;
                    circuit_context.led_color = Some("red".to_string());
                }
                
                // Check for pull-up context
                if node.to_string().contains("pull") || component_type == "Res" {
                    circuit_context.is_pullup = true;
                }
                
                // Check for decoupling context
                if component_type == "Cap" && node.to_string().contains("VCC") {
                    circuit_context.is_decoupling = true;
                }

                // Infer component parameters
                if let Some(suggestion) = component_inference.infer_component_parameters(
                    component_type, &requirements, &circuit_context
                ) {
                    component_inference.add_inferred_component(suggestion);
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


