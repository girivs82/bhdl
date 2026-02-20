//! Code Actions support - provides quick fixes and refactoring suggestions

use tower_lsp::lsp_types::*;
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;

/// Provide code actions for the given range
pub fn provide_code_actions(
    text: &str,
    range: Range,
    diagnostics: Vec<Diagnostic>,
) -> Option<Vec<CodeActionOrCommand>> {
    let parse_result = parse(text);
    let source_file = SourceFile::cast(parse_result.syntax())?;
    let analysis_result = analyze(&source_file);

    let mut actions = Vec::new();

    // Add fixes for diagnostics
    for diagnostic in &diagnostics {
        if let Some(action) = create_diagnostic_fix(text, diagnostic, &analysis_result) {
            actions.push(CodeActionOrCommand::CodeAction(action));
        }
    }

    // Add refactoring suggestions
    if let Some(refactoring) = suggest_refactoring(text, range) {
        actions.push(CodeActionOrCommand::CodeAction(refactoring));
    }

    if actions.is_empty() {
        None
    } else {
        Some(actions)
    }
}

/// Create a fix for a specific diagnostic
fn create_diagnostic_fix(
    text: &str,
    diagnostic: &Diagnostic,
    _analysis: &bhdl_analyzer::AnalysisResult,
) -> Option<CodeAction> {
    // Check for common error patterns in the diagnostic message
    let message = &diagnostic.message;

    // Fix: "Net 'X' not found. Did you mean '@X'?"
    if message.contains("not found") && message.contains("Did you mean '@") {
        return Some(create_add_at_prefix_fix(text, diagnostic));
    }

    // Fix: "Missing semicolon"
    if message.contains("semicolon") || message.contains("expected ';'") {
        return Some(create_add_semicolon_fix(text, diagnostic));
    }

    // Fix: "Undefined power domain"
    if message.contains("Undefined power domain") {
        return Some(create_add_power_declaration_fix(text, diagnostic));
    }

    None
}

/// Create action to add @ prefix to net reference
fn create_add_at_prefix_fix(text: &str, diagnostic: &Diagnostic) -> CodeAction {
    let range = diagnostic.range;

    // Extract the net name from the diagnostic message
    // Message format: "Net 'VCC' not found. Did you mean '@VCC'?"
    let net_name = extract_quoted_text(&diagnostic.message);

    let edit = TextEdit {
        range,
        new_text: format!("@{}", net_name),
    };

    let mut changes = std::collections::HashMap::new();
    changes.insert(
        Url::parse("file:///current_document").unwrap(),
        vec![edit],
    );

    CodeAction {
        title: format!("Add '@' prefix to '{}'", net_name),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diagnostic.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    }
}

/// Create action to add semicolon
fn create_add_semicolon_fix(_text: &str, diagnostic: &Diagnostic) -> CodeAction {
    // Insert semicolon at the end of the range
    let mut range = diagnostic.range;
    range.start = range.end;

    let edit = TextEdit {
        range,
        new_text: ";".to_string(),
    };

    let mut changes = std::collections::HashMap::new();
    changes.insert(
        Url::parse("file:///current_document").unwrap(),
        vec![edit],
    );

    CodeAction {
        title: "Add semicolon".to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diagnostic.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    }
}

/// Create action to add power declaration
fn create_add_power_declaration_fix(_text: &str, diagnostic: &Diagnostic) -> CodeAction {
    // Extract power domain name from message
    let power_name = extract_quoted_text(&diagnostic.message);

    // Insert at the beginning of the board/entity
    let insert_position = Position { line: 1, character: 4 }; // After opening brace

    let edit = TextEdit {
        range: Range {
            start: insert_position,
            end: insert_position,
        },
        new_text: format!("power {} = 5V;\n    ", power_name),
    };

    let mut changes = std::collections::HashMap::new();
    changes.insert(
        Url::parse("file:///current_document").unwrap(),
        vec![edit],
    );

    CodeAction {
        title: format!("Add power declaration for '{}'", power_name),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diagnostic.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    }
}

/// Suggest refactoring actions
fn suggest_refactoring(_text: &str, _range: Range) -> Option<CodeAction> {
    // Future: Implement refactorings like:
    // - Extract to entity
    // - Extract to component
    // - Inline component
    // - Convert to net assignment
    None
}

/// Extract text within single quotes from a message
fn extract_quoted_text(message: &str) -> String {
    if let Some(start) = message.find('\'') {
        if let Some(end) = message[start + 1..].find('\'') {
            return message[start + 1..start + 1 + end].to_string();
        }
    }
    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_quoted_text() {
        let msg = "Net 'VCC' not found. Did you mean '@VCC'?";
        assert_eq!(extract_quoted_text(msg), "VCC");

        let msg2 = "Undefined power domain 'V3P3'";
        assert_eq!(extract_quoted_text(msg2), "V3P3");
    }

    #[test]
    fn test_add_at_prefix_action() {
        let diagnostic = Diagnostic {
            range: Range {
                start: Position { line: 5, character: 10 },
                end: Position { line: 5, character: 13 },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            code: None,
            source: Some("bhdl-analyzer".to_string()),
            message: "Net 'VCC' not found. Did you mean '@VCC'?".to_string(),
            related_information: None,
            tags: None,
            code_description: None,
            data: None,
        };

        let action = create_add_at_prefix_fix("", &diagnostic);

        assert_eq!(action.title, "Add '@' prefix to 'VCC'");
        assert_eq!(action.kind, Some(CodeActionKind::QUICKFIX));
        assert!(action.is_preferred.unwrap_or(false));

        // Check the edit
        if let Some(edit) = action.edit {
            if let Some(changes) = edit.changes {
                let edits: Vec<_> = changes.values().flatten().collect();
                assert_eq!(edits.len(), 1);
                assert_eq!(edits[0].new_text, "@VCC");
            }
        }
    }

    #[test]
    fn test_add_semicolon_action() {
        let diagnostic = Diagnostic {
            range: Range {
                start: Position { line: 5, character: 10 },
                end: Position { line: 5, character: 25 },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            code: None,
            source: Some("bhdl-parser".to_string()),
            message: "Expected ';' after statement".to_string(),
            related_information: None,
            tags: None,
            code_description: None,
            data: None,
        };

        let action = create_add_semicolon_fix("", &diagnostic);

        assert_eq!(action.title, "Add semicolon");
        assert_eq!(action.kind, Some(CodeActionKind::QUICKFIX));

        // Check that semicolon is inserted at the end
        if let Some(edit) = action.edit {
            if let Some(changes) = edit.changes {
                let edits: Vec<_> = changes.values().flatten().collect();
                assert_eq!(edits.len(), 1);
                assert_eq!(edits[0].new_text, ";");
                assert_eq!(edits[0].range.start.line, 5);
                assert_eq!(edits[0].range.start.character, 25); // At the end
            }
        }
    }

    #[test]
    fn test_provide_code_actions_empty() {
        let text = "board TestBoard {}";
        let range = Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: 0, character: 10 },
        };

        let actions = provide_code_actions(text, range, vec![]);
        assert!(actions.is_none()); // No diagnostics, no actions
    }

    #[test]
    fn test_provide_code_actions_with_diagnostic() {
        let text = "board TestBoard { net test: VCC -> output; }";
        let diagnostic = Diagnostic {
            range: Range {
                start: Position { line: 0, character: 28 },
                end: Position { line: 0, character: 31 },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            code: None,
            source: Some("bhdl-analyzer".to_string()),
            message: "Net 'VCC' not found. Did you mean '@VCC'?".to_string(),
            related_information: None,
            tags: None,
            code_description: None,
            data: None,
        };

        let actions = provide_code_actions(text, diagnostic.range, vec![diagnostic]);
        assert!(actions.is_some());

        let actions = actions.unwrap();
        assert_eq!(actions.len(), 1);

        if let CodeActionOrCommand::CodeAction(action) = &actions[0] {
            assert_eq!(action.title, "Add '@' prefix to 'VCC'");
        }
    }
}
