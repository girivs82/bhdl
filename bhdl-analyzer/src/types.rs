use std::collections::HashMap;
use rowan::{TextRange};
use rowan::ast::SyntaxNodePtr;
use bhdl_parser::BhdlLanguage; // Needed for SyntaxNodePtr
use crate::symbol_table::SymbolTable; // Use crate:: to refer to local module

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

// Analysis results including scopes and diagnostics
#[derive(Debug, Default)]
pub struct AnalysisResult {
    pub global_scope: SymbolTable,
    pub definition_scopes: HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>,
    pub diagnostics: Vec<Diagnostic>,
    pub resolved_constants: ResolvedConstants,
} 