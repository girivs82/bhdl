use std::collections::HashMap;
use rowan::{TextRange};
use rowan::ast::SyntaxNodePtr;
use bhdl_parser::BhdlLanguage; // Needed for SyntaxNodePtr
use crate::symbol_table::SymbolTable; // Use crate:: to refer to local module
use crate::power_analysis::PowerAnalysisContext;
use crate::component_inference::ComponentInferenceContext;
use crate::power_sequencing::PowerSequenceGenerator;

// Represents resolved type information for checking
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTypeInfo {
    pub base_type_name: String,
    // Represents width: None for scalar, Some((high, low)) for bus
    pub bounds: Option<(i64, i64)>,
}

impl ResolvedTypeInfo {
    // Helper to get width (number of bits)
    pub fn width(&self) -> Option<u64> {
        self.bounds.map(|(h, l)| (h - l).abs() as u64 + 1)
    }
}

// Represents a diagnostic message (error, warning)
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub message: String,
    pub range: TextRange, // Position in the source text
}

// Type alias for the map storing results of constant evaluation
pub type ResolvedConstants = HashMap<SyntaxNodePtr<BhdlLanguage>, i64>;

// Source location information for diagnostics
#[derive(Debug, Clone, PartialEq)]
pub struct SourceLocation {
    pub line: u32,
    pub column: u32,
    pub file: Option<String>,
}

impl SourceLocation {
    pub fn new(line: u32, column: u32) -> Self {
        Self {
            line,
            column,
            file: None,
        }
    }
    
    pub fn with_file(line: u32, column: u32, file: String) -> Self {
        Self {
            line,
            column,
            file: Some(file),
        }
    }
    
    pub fn unknown() -> Self {
        Self {
            line: 0,
            column: 0,
            file: None,
        }
    }
}

// Analysis results including scopes and diagnostics
#[derive(Debug)]
pub struct AnalysisResult {
    pub global_scope: SymbolTable,
    pub definition_scopes: HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>,
    pub diagnostics: Vec<Diagnostic>,
    pub resolved_constants: ResolvedConstants,
    pub power_analysis: PowerAnalysisContext,
    pub component_inference: ComponentInferenceContext,
    pub power_sequencing: PowerSequenceGenerator,
}

impl Default for AnalysisResult {
    fn default() -> Self {
        Self {
            global_scope: SymbolTable::default(),
            definition_scopes: HashMap::new(),
            diagnostics: Vec::new(),
            resolved_constants: HashMap::new(),
            power_analysis: PowerAnalysisContext::new(),
            component_inference: ComponentInferenceContext::new(),
            power_sequencing: PowerSequenceGenerator::new(),
        }
    }
} 