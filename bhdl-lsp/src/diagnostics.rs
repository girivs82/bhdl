//! Convert BHDL analyzer diagnostics to LSP format

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use bhdl_analyzer::Diagnostic as BhdlDiagnostic;

/// Convert BHDL diagnostics to LSP diagnostics
pub fn convert_diagnostics(diagnostics: &[BhdlDiagnostic]) -> Vec<Diagnostic> {
    diagnostics
        .iter()
        .map(|diag| {
            // Determine severity based on message content
            let severity = if diag.message.contains("Error") || diag.message.contains("Undefined") {
                DiagnosticSeverity::ERROR
            } else if diag.message.contains("Warning") {
                DiagnosticSeverity::WARNING
            } else {
                DiagnosticSeverity::INFORMATION
            };

            Diagnostic {
                range: Range {
                    start: Position { line: 0, character: 0 },
                    end: Position { line: 0, character: 0 },
                },
                severity: Some(severity),
                code: None,
                source: Some("bhdl-analyzer".to_string()),
                message: diag.message.clone(),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        })
        .collect()
}
