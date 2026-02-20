//! Pass 9: Safety Analysis
//! 
//! This pass analyzes safety requirements and compliance declarations,
//! collecting traceability information and calculating safety metrics.

use bhdl_ast::{AstNode, Board, HasName, HasSatisfies, SatisfiesSpec, SourceFile};
use crate::types::Diagnostic;
use std::collections::HashMap;

/// Safety requirement information
#[derive(Debug, Clone)]
pub struct SafetyRequirement {
    pub id: String,
    pub satisfaction: SafetyCompliance,
    pub source_location: Option<String>,
}

/// How a requirement is satisfied
#[derive(Debug, Clone)]
pub enum SafetyCompliance {
    /// Satisfied via a specific component
    ViaComponent { component: String },
    /// Satisfied with detailed evidence
    WithDetails { details: HashMap<String, String> },
    /// Not satisfied (requirement exists but no compliance declared)
    NotSatisfied,
}

/// Safety analysis results
#[derive(Debug, Clone, Default)]
pub struct SafetyAnalysisResult {
    /// All safety requirements found
    pub requirements: HashMap<String, SafetyRequirement>,
    /// Traceability matrix: requirement ID -> implementing components
    pub traceability: HashMap<String, Vec<String>>,
    /// Coverage metrics
    pub coverage: SafetyCoverage,
    /// Safety-related diagnostics
    pub diagnostics: Vec<Diagnostic>,
}

/// Safety coverage metrics
#[derive(Debug, Clone, Default)]
pub struct SafetyCoverage {
    pub total_requirements: usize,
    pub satisfied_requirements: usize,
    pub coverage_percentage: f64,
    pub unsatisfied_requirements: Vec<String>,
}

/// Perform safety analysis on the AST
pub fn analyze_safety(source_file: &SourceFile) -> SafetyAnalysisResult {
    let mut result = SafetyAnalysisResult::default();
    let mut diagnostics = Vec::new();

    // Extract board from AST
    if let Some(board) = extract_board(source_file) {
        // Analyze safety compliance
        analyze_safety_compliance(&board, &mut result, &mut diagnostics);
        
        // Calculate coverage metrics
        calculate_coverage_metrics(&mut result);
        
        // Check for missing requirements
        check_missing_requirements(&result, &mut diagnostics);
    }

    result.diagnostics = diagnostics;
    result
}

/// Extract the board from the AST
fn extract_board(ast: &bhdl_ast::SourceFile) -> Option<Board> {
    for item in ast.items() {
        if let Some(board) = Board::cast(item.syntax().clone()) {
            return Some(board);
        }
    }
    None
}

/// Analyze safety compliance declarations in the board
fn analyze_safety_compliance(
    board: &Board,
    result: &mut SafetyAnalysisResult,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Check if board has satisfies block
    if let Some(satisfies_block) = board.satisfies_block() {
        // Process each satisfaction item
        for item in satisfies_block.items() {
            if let Some(req_id) = item.requirement_id() {
                let req_id_str = req_id.text().to_string();
                
                let compliance = match item.satisfaction() {
                    Some(SatisfiesSpec::Via(via)) => {
                        let component = via.component_path_string();
                        
                        // Add to traceability matrix
                        result.traceability
                            .entry(req_id_str.clone())
                            .or_insert_with(Vec::new)
                            .push(component.clone());
                        
                        SafetyCompliance::ViaComponent { component }
                    }
                    Some(SatisfiesSpec::Details(details)) => {
                        let fields = details.fields()
                            .into_iter()
                            .collect::<HashMap<_, _>>();
                        
                        // Validate required fields for detailed compliance
                        if !fields.contains_key("implementation") {
                            diagnostics.push(Diagnostic::new(
                                format!(
                                    "Safety requirement {} lacks implementation description",
                                    req_id_str
                                ),
                                rowan::TextRange::empty(rowan::TextSize::from(0)),
                            ));
                        }
                        
                        SafetyCompliance::WithDetails { details: fields }
                    }
                    None => {
                        diagnostics.push(Diagnostic::new(
                            format!(
                                "Safety requirement {} declared but not satisfied",
                                req_id_str
                            ),
                            rowan::TextRange::empty(rowan::TextSize::from(0)),
                        ));
                        SafetyCompliance::NotSatisfied
                    }
                };
                
                result.requirements.insert(
                    req_id_str.clone(),
                    SafetyRequirement {
                        id: req_id_str,
                        satisfaction: compliance,
                        source_location: board.name().map(|n| n.text().to_string()),
                    },
                );
            }
        }
    } else {
        // Check if this is a safety-critical board (has certain components)
        if is_safety_critical_board(board) {
            diagnostics.push(Diagnostic::new(
                "Safety-critical board lacks satisfies block".to_string(),
                rowan::TextRange::empty(rowan::TextSize::from(0)),
            ));
        }
    }
}

/// Calculate coverage metrics
fn calculate_coverage_metrics(result: &mut SafetyAnalysisResult) {
    let total = result.requirements.len();
    let satisfied = result.requirements
        .values()
        .filter(|req| !matches!(req.satisfaction, SafetyCompliance::NotSatisfied))
        .count();
    
    let mut unsatisfied = Vec::new();
    for (id, req) in &result.requirements {
        if matches!(req.satisfaction, SafetyCompliance::NotSatisfied) {
            unsatisfied.push(id.clone());
        }
    }
    
    result.coverage = SafetyCoverage {
        total_requirements: total,
        satisfied_requirements: satisfied,
        coverage_percentage: if total > 0 {
            (satisfied as f64 / total as f64) * 100.0
        } else {
            100.0
        },
        unsatisfied_requirements: unsatisfied,
    };
}

/// Check for missing requirements based on project configuration
fn check_missing_requirements(
    result: &SafetyAnalysisResult,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // In a real implementation, this would check against a requirements database
    // or project configuration file to ensure all required safety requirements
    // are addressed
    
    // For now, just check coverage
    if result.coverage.coverage_percentage < 100.0 && !result.requirements.is_empty() {
        diagnostics.push(Diagnostic::new(
            format!(
                "Safety requirement coverage is {:.1}% ({}/{} satisfied)",
                result.coverage.coverage_percentage,
                result.coverage.satisfied_requirements,
                result.coverage.total_requirements
            ),
            rowan::TextRange::empty(rowan::TextSize::from(0)),
        ));
    }
}

/// Determine if a board appears to be safety-critical based on its components
fn is_safety_critical_board(board: &Board) -> bool {
    // Look for components that suggest safety criticality
    // This is a heuristic - in practice this would be configured
    if let Some(name) = board.name() {
        let name_str = name.text().to_lowercase();
        if name_str.contains("safety") || 
           name_str.contains("monitor") ||
           name_str.contains("protection") ||
           name_str.contains("bcm") ||  // Body Control Module
           name_str.contains("ecu") {    // Electronic Control Unit
            return true;
        }
    }
    
    // Could also check for specific component types like:
    // - Voltage monitors
    // - Protection diodes  
    // - Redundant components
    // But that would require access to the component instances
    
    false
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safety_coverage_calculation() {
        let mut result = SafetyAnalysisResult::default();
        
        // Add some test requirements
        result.requirements.insert(
            "REQ_001".to_string(),
            SafetyRequirement {
                id: "REQ_001".to_string(),
                satisfaction: SafetyCompliance::ViaComponent {
                    component: "monitor".to_string(),
                },
                source_location: None,
            },
        );
        
        result.requirements.insert(
            "REQ_002".to_string(),
            SafetyRequirement {
                id: "REQ_002".to_string(),
                satisfaction: SafetyCompliance::NotSatisfied,
                source_location: None,
            },
        );
        
        calculate_coverage_metrics(&mut result);
        
        assert_eq!(result.coverage.total_requirements, 2);
        assert_eq!(result.coverage.satisfied_requirements, 1);
        assert_eq!(result.coverage.coverage_percentage, 50.0);
        assert_eq!(result.coverage.unsatisfied_requirements, vec!["REQ_002"]);
    }
}