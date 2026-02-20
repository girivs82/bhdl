//! Rename Symbol support

use tower_lsp::lsp_types::*;
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use rowan::{TextRange, SyntaxNode};
use bhdl_parser::BhdlLanguage;

/// Prepare for rename - validate that rename is possible at this location
pub fn prepare_rename(text: &str, position: Position) -> Option<PrepareRenameResponse> {
    // Parse and analyze
    let parse_result = parse(text);
    let source_file = SourceFile::cast(parse_result.syntax())?;
    let analysis_result = analyze(&source_file);

    // Convert position to offset
    let byte_offset = position_to_offset(text, position)?;

    // Find token at position
    let token = parse_result.syntax()
        .token_at_offset((byte_offset as u32).into())
        .right_biased()?;

    let symbol_name = token.text().to_string();

    // Verify the symbol exists in the symbol table
    let symbol_table = &analysis_result.global_scope;
    let _symbol = symbol_table.lookup(&symbol_name)
        .or_else(|| symbol_table.lookup_net(&symbol_name))?;

    // Create the range for the current symbol
    let range = text_range_to_lsp_range(text, token.text_range());

    // Return the range and default text (current symbol name)
    Some(PrepareRenameResponse::RangeWithPlaceholder {
        range,
        placeholder: symbol_name,
    })
}

/// Perform the actual rename operation
pub fn rename_symbol(
    text: &str,
    position: Position,
    new_name: &str,
) -> Option<WorkspaceEdit> {
    // Validate new name is a valid identifier
    if !is_valid_identifier(new_name) {
        return None;
    }

    // Parse and analyze
    let parse_result = parse(text);
    let source_file = SourceFile::cast(parse_result.syntax())?;
    let analysis_result = analyze(&source_file);

    // Convert position to offset
    let byte_offset = position_to_offset(text, position)?;

    // Find token at position
    let token = parse_result.syntax()
        .token_at_offset((byte_offset as u32).into())
        .right_biased()?;

    let old_name = token.text().to_string();

    // Verify the symbol exists
    let symbol_table = &analysis_result.global_scope;
    let _symbol = symbol_table.lookup(&old_name)
        .or_else(|| symbol_table.lookup_net(&old_name))?;

    // Check for naming conflicts
    if symbol_table.lookup(new_name).is_some() || symbol_table.lookup_net(new_name).is_some() {
        // Name already exists - conflict
        return None;
    }

    // Find all occurrences of the symbol
    let mut text_edits = Vec::new();
    find_rename_locations(&parse_result.syntax(), &old_name, text, new_name, &mut text_edits);

    // Create workspace edit
    let mut changes = std::collections::HashMap::new();
    let uri = Url::parse("file:///current_document").unwrap(); // Will be replaced with actual URI
    changes.insert(uri, text_edits);

    Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}

/// Find all locations where the symbol should be renamed
fn find_rename_locations(
    node: &SyntaxNode<BhdlLanguage>,
    old_name: &str,
    text: &str,
    new_name: &str,
    edits: &mut Vec<TextEdit>,
) {
    for token in node.children_with_tokens() {
        match token {
            rowan::NodeOrToken::Token(tok) => {
                if tok.text() == old_name {
                    let range = text_range_to_lsp_range(text, tok.text_range());
                    edits.push(TextEdit {
                        range,
                        new_text: new_name.to_string(),
                    });
                }
            }
            rowan::NodeOrToken::Node(child_node) => {
                find_rename_locations(&child_node, old_name, text, new_name, edits);
            }
        }
    }
}

/// Validate that a string is a valid BHDL identifier
fn is_valid_identifier(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    // Must start with letter or underscore
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !first.is_alphabetic() && first != '_' {
        return false;
    }

    // Rest must be alphanumeric or underscore
    for ch in chars {
        if !ch.is_alphanumeric() && ch != '_' {
            return false;
        }
    }

    // Check against BHDL keywords
    matches!(name,
        "board" | "entity" | "component" | "interface" | "power" | "ground" |
        "net" | "pin" | "in" | "out" | "inout" | "for" | "generate" | "if" |
        "const" | "param" | "import" | "from" | "alias" | "when" | "satisfies"
    ).then(|| false).unwrap_or(true)
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
        offset += line.len() + 1;
    }

    None
}

/// Convert byte offset to LSP Position
fn offset_to_position(text: &str, offset: usize) -> Position {
    let mut line = 0;
    let mut character = 0;
    let mut current_offset = 0;

    for ch in text.chars() {
        if current_offset >= offset {
            break;
        }

        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
        current_offset += ch.len_utf8();
    }

    Position { line, character }
}

/// Convert TextRange to LSP Range
fn text_range_to_lsp_range(text: &str, range: TextRange) -> Range {
    let start_offset: usize = range.start().into();
    let end_offset: usize = range.end().into();

    let start_position = offset_to_position(text, start_offset);
    let end_position = offset_to_position(text, end_offset);

    Range {
        start: start_position,
        end: end_position,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_identifier() {
        // Valid identifiers
        assert!(is_valid_identifier("Regulator"));
        assert!(is_valid_identifier("my_entity"));
        assert!(is_valid_identifier("_private"));
        assert!(is_valid_identifier("Module123"));

        // Invalid identifiers
        assert!(!is_valid_identifier("123entity")); // starts with number
        assert!(!is_valid_identifier("my-entity")); // contains hyphen
        assert!(!is_valid_identifier("my.entity")); // contains dot
        assert!(!is_valid_identifier("")); // empty

        // Keywords should be invalid
        assert!(!is_valid_identifier("board"));
        assert!(!is_valid_identifier("entity"));
        assert!(!is_valid_identifier("power"));
    }

    #[test]
    fn test_prepare_rename() {
        let text = r#"
entity Regulator() {
    pin IN: power in;
}

board TestBoard {
    Regulator();
}
"#;

        // Prepare rename on entity definition
        let position = Position { line: 1, character: 7 }; // "entity Regulator"
        let response = prepare_rename(text, position);

        assert!(response.is_some());
        if let Some(PrepareRenameResponse::RangeWithPlaceholder { placeholder, .. }) = response {
            assert_eq!(placeholder, "Regulator");
        }
    }

    #[test]
    fn test_rename_symbol() {
        let text = r#"
entity Regulator() {
    pin IN: power in;
}

board TestBoard {
    Regulator();
    Regulator();
}
"#;

        // Rename "Regulator" to "VoltageRegulator"
        let position = Position { line: 1, character: 7 };
        let workspace_edit = rename_symbol(text, position, "VoltageRegulator");

        assert!(workspace_edit.is_some());
        let edit = workspace_edit.unwrap();

        // Should have changes
        assert!(edit.changes.is_some());
        let changes = edit.changes.unwrap();

        // Should have at least one URI with edits
        assert!(!changes.is_empty());

        // Should have multiple edits (definition + 2 uses = 3 total)
        let edits = changes.values().next().unwrap();
        assert!(edits.len() >= 3, "Expected at least 3 edits, found {}", edits.len());

        // All edits should replace with "VoltageRegulator"
        for edit in edits {
            assert_eq!(edit.new_text, "VoltageRegulator");
        }
    }

    #[test]
    fn test_rename_conflict_detection() {
        let text = r#"
entity Regulator() {
    pin IN: power in;
}

entity PowerSupply() {
    pin IN: power in;
}

board TestBoard {
    Regulator();
}
"#;

        // Try to rename "Regulator" to "PowerSupply" (should fail - conflict)
        let position = Position { line: 1, character: 7 };
        let workspace_edit = rename_symbol(text, position, "PowerSupply");

        // Should return None due to naming conflict
        assert!(workspace_edit.is_none());
    }

    #[test]
    fn test_rename_invalid_identifier() {
        let text = r#"
entity Regulator() {
    pin IN: power in;
}
"#;

        // Try to rename to invalid identifier
        let position = Position { line: 1, character: 7 };

        // Should fail for various invalid names
        assert!(rename_symbol(text, position, "123Invalid").is_none());
        assert!(rename_symbol(text, position, "my-entity").is_none());
        assert!(rename_symbol(text, position, "entity").is_none()); // keyword
        assert!(rename_symbol(text, position, "").is_none()); // empty
    }
}
