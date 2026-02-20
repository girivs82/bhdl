//! Document Highlight support - highlights all occurrences of symbol under cursor

use tower_lsp::lsp_types::*;
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;

/// Provide document highlights for the symbol at the given position
pub fn provide_document_highlight(
    text: &str,
    position: Position,
) -> Option<Vec<DocumentHighlight>> {
    let parse_result = parse(text);
    let source_file = SourceFile::cast(parse_result.syntax())?;
    let analysis_result = analyze(&source_file);

    // Convert position to offset
    let offset = position_to_offset(text, position)?;

    // Find the symbol at the cursor position
    let symbol = find_symbol_at_position(&analysis_result, offset)?;
    let symbol_name = &symbol.name;

    let mut highlights = Vec::new();

    // Add the definition/declaration itself as a Write highlight
    let def_range = text_range_to_lsp_range(&symbol.span);
    highlights.push(DocumentHighlight {
        range: def_range,
        kind: Some(DocumentHighlightKind::WRITE),
    });

    // Find all references to this symbol in global scope
    for other_symbol in analysis_result.global_scope.iter() {
        // Check if name matches OR if this is an instance of the symbol type
        let is_match = other_symbol.name == *symbol_name ||
            (other_symbol.kind == bhdl_analyzer::symbol_table::SymbolKind::Instance &&
             other_symbol.instance_type_name.as_ref() == Some(symbol_name));

        if is_match && other_symbol.span != symbol.span {
            let range = text_range_to_lsp_range(&other_symbol.span);

            // Determine highlight kind based on symbol type
            let kind = match other_symbol.kind {
                bhdl_analyzer::symbol_table::SymbolKind::Instance => {
                    // Instances are "read" operations (using the entity/component)
                    Some(DocumentHighlightKind::READ)
                },
                _ => {
                    // Other occurrences are typically declarations/definitions
                    Some(DocumentHighlightKind::TEXT)
                }
            };

            highlights.push(DocumentHighlight { range, kind });
        }
    }

    // Find all references in definition scopes
    for (_node_ptr, scope) in &analysis_result.definition_scopes {
        for other_symbol in scope.iter() {
            // Check if name matches OR if this is an instance of the symbol type
            let is_match = other_symbol.name == *symbol_name ||
                (other_symbol.kind == bhdl_analyzer::symbol_table::SymbolKind::Instance &&
                 other_symbol.instance_type_name.as_ref() == Some(symbol_name));

            if is_match && other_symbol.span != symbol.span {
                let range = text_range_to_lsp_range(&other_symbol.span);

                let kind = match other_symbol.kind {
                    bhdl_analyzer::symbol_table::SymbolKind::Instance => {
                        Some(DocumentHighlightKind::READ)
                    },
                    _ => {
                        Some(DocumentHighlightKind::TEXT)
                    }
                };

                highlights.push(DocumentHighlight { range, kind });
            }
        }
    }

    if highlights.is_empty() {
        None
    } else {
        Some(highlights)
    }
}

/// Find symbol at the given offset
fn find_symbol_at_position(
    analysis: &bhdl_analyzer::AnalysisResult,
    offset: usize,
) -> Option<&bhdl_analyzer::symbol_table::Symbol> {
    // Search in global scope
    for symbol in analysis.global_scope.iter() {
        let start: usize = symbol.span.start().into();
        let end: usize = symbol.span.end().into();
        if offset >= start && offset <= end {
            return Some(symbol);
        }
    }

    // Search in definition scopes
    for (_node_ptr, scope) in &analysis.definition_scopes {
        for symbol in scope.iter() {
            let start: usize = symbol.span.start().into();
            let end: usize = symbol.span.end().into();
            if offset >= start && offset <= end {
                return Some(symbol);
            }
        }
    }

    None
}

/// Convert rowan TextRange to LSP Range
fn text_range_to_lsp_range(text_range: &rowan::TextRange) -> Range {
    let start: usize = text_range.start().into();
    let end: usize = text_range.end().into();

    Range {
        start: Position {
            line: 0,
            character: start as u32,
        },
        end: Position {
            line: 0,
            character: end as u32,
        },
    }
}

/// Convert LSP Position to byte offset
fn position_to_offset(text: &str, position: Position) -> Option<usize> {
    let mut line_num = 0;
    let mut offset = 0;

    for line in text.lines() {
        if line_num == position.line as usize {
            let char_offset = position.character as usize;
            if char_offset <= line.len() {
                return Some(offset + char_offset);
            } else {
                return None;
            }
        }
        line_num += 1;
        offset += line.len() + 1; // +1 for newline
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_highlight_entity() {
        let text = r#"
entity Regulator() {
    pin IN: power in;
}

board TestBoard {
    Regulator();
}
"#;

        // Position on "Regulator" entity definition
        let position = Position { line: 1, character: 8 };

        let result = provide_document_highlight(text, position);
        assert!(result.is_some());

        let highlights = result.unwrap();
        // Should have at least 2: definition and instantiation
        assert!(highlights.len() >= 2);

        // First highlight should be the definition (WRITE)
        assert_eq!(highlights[0].kind, Some(DocumentHighlightKind::WRITE));
    }

    #[test]
    fn test_document_highlight_power_domain() {
        let text = r#"
board TestBoard {
    power VCC = 5V;
    ground GND;

    net test: @VCC -> output;
}
"#;

        // Position on "VCC" declaration
        let position = Position { line: 2, character: 11 };

        let result = provide_document_highlight(text, position);
        assert!(result.is_some());

        let highlights = result.unwrap();
        // Should have at least the declaration
        assert!(!highlights.is_empty());
    }

    #[test]
    fn test_document_highlight_no_matches() {
        let text = r#"
entity UniqueEntity() {
    pin A: signal in;
}
"#;

        // Position on "A" which is only used once
        let position = Position { line: 2, character: 9 };

        let result = provide_document_highlight(text, position);
        // Should still return the definition itself
        assert!(result.is_some());

        let highlights = result.unwrap();
        assert_eq!(highlights.len(), 1);
    }

    #[test]
    fn test_document_highlight_multiple_uses() {
        let text = r#"
entity LED() {
    pin A: signal in;
}

board TestBoard {
    LED();
    LED();
    LED();
}
"#;

        // Position on "LED" entity definition
        let position = Position { line: 1, character: 8 };

        let result = provide_document_highlight(text, position);
        assert!(result.is_some());

        let highlights = result.unwrap();
        // Should have at least 2: 1 definition + at least 1 instantiation
        assert!(highlights.len() >= 2, "Expected at least 2 highlights, got {}", highlights.len());

        // First should be definition (WRITE)
        assert_eq!(highlights[0].kind, Some(DocumentHighlightKind::WRITE));

        // Check that we have at least one READ highlight (instantiation)
        let has_read = highlights.iter().any(|h| h.kind == Some(DocumentHighlightKind::READ));
        assert!(has_read, "Should have at least one READ highlight for instance");
    }

    #[test]
    fn test_position_to_offset() {
        let text = "line1\nline2\nline3";

        assert_eq!(position_to_offset(text, Position { line: 0, character: 0 }), Some(0));
        assert_eq!(position_to_offset(text, Position { line: 0, character: 5 }), Some(5));
        assert_eq!(position_to_offset(text, Position { line: 1, character: 0 }), Some(6));
        assert_eq!(position_to_offset(text, Position { line: 2, character: 2 }), Some(14));
    }

    #[test]
    fn test_document_highlight_invalid_position() {
        let text = r#"
board TestBoard {
    power VCC = 5V;
}
"#;

        // Position way beyond the end of the document
        let position = Position { line: 100, character: 0 };

        let result = provide_document_highlight(text, position);
        // Should return None if no symbol at position
        assert!(result.is_none());
    }
}
