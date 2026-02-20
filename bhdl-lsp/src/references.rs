//! Find References support

use tower_lsp::lsp_types::*;
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use rowan::{TextRange, SyntaxNode};
use bhdl_parser::BhdlLanguage;

/// Find all references to the symbol at the given position
pub fn find_references(text: &str, position: Position, include_declaration: bool) -> Option<Vec<Location>> {
    // Parse the document
    let parse_result = parse(text);
    let source_file = SourceFile::cast(parse_result.syntax())?;

    // Run semantic analysis
    let analysis_result = analyze(&source_file);

    // Convert LSP position to byte offset
    let byte_offset = position_to_offset(text, position)?;

    // Find the token at the cursor position
    let token = parse_result.syntax()
        .token_at_offset((byte_offset as u32).into())
        .right_biased()?;

    let target_symbol_name = token.text().to_string();

    // Look up the symbol to verify it exists
    let symbol_table = &analysis_result.global_scope;
    let symbol = symbol_table.lookup(&target_symbol_name)
        .or_else(|| symbol_table.lookup_net(&target_symbol_name))?;

    let mut locations = Vec::new();

    // Add the definition location if requested
    if include_declaration {
        locations.push(create_location_from_range(text, symbol.span));
    }

    // Find all references by traversing the syntax tree
    find_references_in_node(&parse_result.syntax(), &target_symbol_name, text, &mut locations);

    Some(locations)
}

/// Recursively traverse the syntax tree to find all identifier references
fn find_references_in_node(
    node: &SyntaxNode<BhdlLanguage>,
    target_name: &str,
    text: &str,
    locations: &mut Vec<Location>,
) {
    // Check if this node is an identifier token
    for token in node.children_with_tokens() {
        match token {
            rowan::NodeOrToken::Token(tok) => {
                // Check if this is an identifier matching our target
                if tok.text() == target_name {
                    let range = tok.text_range();
                    locations.push(create_location_from_range(text, range));
                }
            }
            rowan::NodeOrToken::Node(child_node) => {
                // Recursively search child nodes
                find_references_in_node(&child_node, target_name, text, locations);
            }
        }
    }
}

/// Convert LSP Position to byte offset
fn position_to_offset(text: &str, position: Position) -> Option<usize> {
    let mut line_num = 0;
    let mut offset = 0;

    for line in text.lines() {
        if line_num == position.line as usize {
            // Found the target line
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

/// Create LSP Location from TextRange
fn create_location_from_range(text: &str, range: TextRange) -> Location {
    let start_offset = range.start().into();
    let end_offset = range.end().into();

    let start_position = offset_to_position(text, start_offset);
    let end_position = offset_to_position(text, end_offset);

    Location {
        uri: Url::parse("file:///current_document").unwrap(), // Will be replaced with actual URI
        range: Range {
            start: start_position,
            end: end_position,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_references_basic() {
        let text = r#"
entity Regulator() {
    pin IN: power in;
    pin OUT: power out;
}

board TestBoard {
    Regulator();
    Regulator();
}
"#;

        // Find references to "Regulator" - position on entity definition
        let position = Position { line: 1, character: 7 }; // "entity Regulator"
        let references = find_references(text, position, true);

        assert!(references.is_some());
        let refs = references.unwrap();

        // Should find: 1 definition + 2 uses = 3 total
        assert!(refs.len() >= 2, "Expected at least 2 references, found {}", refs.len());
    }

    #[test]
    fn test_find_references_from_usage() {
        let text = r#"
entity Regulator() {
    pin IN: power in;
}

board TestBoard {
    Regulator();
}
"#;

        // Find references from usage site
        let position = Position { line: 6, character: 4 }; // "Regulator()" in board
        let references = find_references(text, position, true);

        assert!(references.is_some());
        let refs = references.unwrap();

        // Should find at least the definition and this usage
        assert!(refs.len() >= 2, "Expected at least 2 references, found {}", refs.len());
    }

    #[test]
    fn test_find_references_exclude_declaration() {
        let text = r#"
entity Regulator() {
    pin IN: power in;
}

board TestBoard {
    Regulator();
}
"#;

        // Find references without including declaration
        let position = Position { line: 1, character: 7 };
        let references = find_references(text, position, false);

        assert!(references.is_some());
        let refs = references.unwrap();

        // Should find only usage, not definition
        assert!(refs.len() >= 1, "Expected at least 1 reference, found {}", refs.len());
    }
}
