//! Go to Definition support

use tower_lsp::lsp_types::*;
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use rowan::TextRange;

/// Find the definition location for the symbol at the given position
pub fn find_definition(text: &str, position: Position) -> Option<Location> {
    // Parse the document
    let parse_result = parse(text);
    let source_file = SourceFile::cast(parse_result.syntax())?;

    // Run semantic analysis to get symbol table
    let analysis_result = analyze(&source_file);

    // Convert LSP position to byte offset
    let byte_offset = position_to_offset(text, position)?;

    // Find the token at the cursor position
    let token = parse_result.syntax()
        .token_at_offset((byte_offset as u32).into())
        .right_biased()?;

    let token_text = token.text();

    // Look up the symbol in the symbol table
    let symbol_table = &analysis_result.global_scope;

    // Try to find regular symbol (entity, component, etc.)
    if let Some(symbol) = symbol_table.lookup(token_text) {
        return Some(create_location_from_range(text, symbol.span));
    }

    // Try to find net definition
    if let Some(net_symbol) = symbol_table.lookup_net(token_text) {
        return Some(create_location_from_range(text, net_symbol.span));
    }

    None
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
    fn test_position_to_offset() {
        let text = "line 1\nline 2\nline 3";

        // Start of first line
        assert_eq!(position_to_offset(text, Position { line: 0, character: 0 }), Some(0));

        // Start of second line
        assert_eq!(position_to_offset(text, Position { line: 1, character: 0 }), Some(7));

        // Middle of second line
        assert_eq!(position_to_offset(text, Position { line: 1, character: 3 }), Some(10));
    }

    #[test]
    fn test_offset_to_position() {
        let text = "line 1\nline 2\nline 3";

        // Start of first line
        assert_eq!(offset_to_position(text, 0), Position { line: 0, character: 0 });

        // Start of second line
        assert_eq!(offset_to_position(text, 7), Position { line: 1, character: 0 });

        // Middle of second line
        assert_eq!(offset_to_position(text, 10), Position { line: 1, character: 3 });
    }

    #[test]
    fn test_find_entity_definition() {
        let text = r#"
entity Regulator() {
    pin IN: power in;
    pin OUT: power out;
}

board TestBoard {
    Regulator();
}
"#;

        // Cursor on "Regulator" in board (should find entity definition)
        let position = Position { line: 7, character: 4 }; // "Regulator()"
        let location = find_definition(text, position);

        // Should find the entity definition on line 1
        assert!(location.is_some());
        if let Some(loc) = location {
            assert_eq!(loc.range.start.line, 1);
        }
    }
}
