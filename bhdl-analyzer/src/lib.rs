// Needed for analyze function signature and AST traversal
use bhdl_ast::SourceFile;
use rowan::ast::AstNode; // For source_file.syntax()

// Declare modules
mod types;
mod helpers;
mod symbol_table;
mod pass1;
mod pass2;
mod pass3;
mod pass4;

// Use items needed directly in the analyze function
use types::{AnalysisResult, ResolvedConstants};
use pass1::populate_global_scope_and_build_definition_scopes;
use pass2::{visit_node_pass2_references, Pass2Context};
use pass3::{visit_node_pass3_const_eval, Pass3Context};
use pass4::{visit_node_pass4_bounds_checks, Pass4Context};

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


    println!("Analysis finished. Found {} total diagnostics.", diagnostics.len());

    AnalysisResult {
        global_scope, // Move ownership
        definition_scopes, // Move ownership
        diagnostics, // Move ownership
        resolved_constants, // Move ownership
    }
}

// Remove the test module declaration as tests are now in the tests/ directory
// #[cfg(test)]
// mod tests; // Load tests from tests.rs when running cargo test

// The #[cfg(test)] mod tests { ... } block should follow here (as kept by the user)


