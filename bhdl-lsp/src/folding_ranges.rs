//! Folding Ranges support - provides code folding for collapsible regions

use tower_lsp::lsp_types::*;
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile, Board, Entity, ComponentDef, InterfaceDef};
use rowan::TextRange;

/// Provide folding ranges for the document
pub fn provide_folding_ranges(text: &str) -> Option<Vec<FoldingRange>> {
    let parse_result = parse(text);
    let source_file = SourceFile::cast(parse_result.syntax())?;

    let mut ranges = Vec::new();

    // Traverse AST and collect foldable regions
    collect_folding_ranges(&source_file.syntax().clone(), text, &mut ranges);

    if ranges.is_empty() {
        None
    } else {
        Some(ranges)
    }
}

/// Collect folding ranges from AST nodes
fn collect_folding_ranges(
    node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
    text: &str,
    ranges: &mut Vec<FoldingRange>,
) {
    // Check for board declarations
    if let Some(board) = Board::cast(node.clone()) {
        if let Some(range) = create_folding_range_for_block(&board.syntax().text_range(), text) {
            ranges.push(range);
        }
    }

    // Check for entity declarations
    if let Some(entity) = Entity::cast(node.clone()) {
        if let Some(range) = create_folding_range_for_block(&entity.syntax().text_range(), text) {
            ranges.push(range);
        }
    }

    // Check for component declarations
    if let Some(component) = ComponentDef::cast(node.clone()) {
        if let Some(range) = create_folding_range_for_block(&component.syntax().text_range(), text) {
            ranges.push(range);
        }
    }

    // Check for interface declarations
    if let Some(interface) = InterfaceDef::cast(node.clone()) {
        if let Some(range) = create_folding_range_for_block(&interface.syntax().text_range(), text) {
            ranges.push(range);
        }
    }

    // Recurse into children
    for child in node.children() {
        collect_folding_ranges(&child, text, ranges);
    }
}

/// Create a folding range for a block (between opening and closing braces)
fn create_folding_range_for_block(text_range: &TextRange, text: &str) -> Option<FoldingRange> {
    let start_offset: usize = text_range.start().into();
    let end_offset: usize = text_range.end().into();

    // Find the opening brace
    let block_text = &text[start_offset..end_offset];
    let opening_brace = block_text.find('{')?;
    let closing_brace = block_text.rfind('}')?;

    if closing_brace <= opening_brace {
        return None;
    }

    // Convert to line numbers
    let start_line = offset_to_line(text, start_offset + opening_brace);
    let end_line = offset_to_line(text, start_offset + closing_brace);

    // Only create folding range if there's more than one line
    if end_line <= start_line {
        return None;
    }

    Some(FoldingRange {
        start_line: start_line as u32,
        start_character: None,
        end_line: end_line as u32,
        end_character: None,
        kind: Some(FoldingRangeKind::Region),
        collapsed_text: None,
    })
}

/// Convert byte offset to line number
fn offset_to_line(text: &str, offset: usize) -> usize {
    let mut line = 0;
    let mut current_offset = 0;

    for ch in text.chars() {
        if current_offset >= offset {
            break;
        }

        if ch == '\n' {
            line += 1;
        }
        current_offset += ch.len_utf8();
    }

    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_folding_ranges_board() {
        let text = r#"
board TestBoard {
    power VCC = 5V;
    ground GND;
}
"#;

        let ranges = provide_folding_ranges(text);
        assert!(ranges.is_some());

        let ranges = ranges.unwrap();
        assert!(!ranges.is_empty());

        // Should have a folding range for the board
        let board_range = &ranges[0];
        assert_eq!(board_range.start_line, 1); // Line with opening brace
        assert_eq!(board_range.end_line, 4);   // Line with closing brace
        assert_eq!(board_range.kind, Some(FoldingRangeKind::Region));
    }

    #[test]
    fn test_folding_ranges_entity() {
        let text = r#"
entity Regulator() {
    pin IN: power in;
    pin OUT: power out;
}
"#;

        let ranges = provide_folding_ranges(text);
        assert!(ranges.is_some());

        let ranges = ranges.unwrap();
        assert_eq!(ranges.len(), 1);

        let entity_range = &ranges[0];
        assert_eq!(entity_range.start_line, 1);
        assert_eq!(entity_range.end_line, 4);
    }

    #[test]
    fn test_folding_ranges_multiple() {
        let text = r#"
board TestBoard {
    power VCC = 5V;
}

entity Regulator() {
    pin IN: power in;
}
"#;

        let ranges = provide_folding_ranges(text);
        assert!(ranges.is_some());

        let ranges = ranges.unwrap();
        assert_eq!(ranges.len(), 2);

        // Board range
        assert_eq!(ranges[0].start_line, 1);
        assert_eq!(ranges[0].end_line, 3);

        // Entity range
        assert_eq!(ranges[1].start_line, 5);
        assert_eq!(ranges[1].end_line, 7);
    }

    #[test]
    fn test_folding_ranges_nested() {
        let text = r#"
board TestBoard {
    entity InnerEntity() {
        pin A: signal in;
    }
}
"#;

        let ranges = provide_folding_ranges(text);
        assert!(ranges.is_some());

        let ranges = ranges.unwrap();
        // Should have at least the board folding range
        // Note: nested entities inside boards may not be represented as Entity AST nodes
        assert!(!ranges.is_empty());

        // Board range (outer) should exist
        let board_range = ranges.iter().find(|r| r.start_line == 1).unwrap();
        // The end line should be where the closing brace is
        assert!(board_range.end_line >= 4 && board_range.end_line <= 5);

        // If we have 2 ranges, the second should be the inner entity
        if ranges.len() == 2 {
            let entity_range = ranges.iter().find(|r| r.start_line == 2).unwrap();
            assert!(entity_range.end_line >= 3 && entity_range.end_line <= 4);
        }
    }

    #[test]
    fn test_folding_ranges_single_line() {
        // Single-line blocks should not create folding ranges
        let text = "board TestBoard { }";

        let ranges = provide_folding_ranges(text);
        // Might be None or empty depending on implementation
        if let Some(ranges) = ranges {
            assert!(ranges.is_empty() || ranges[0].start_line == ranges[0].end_line);
        }
    }

    #[test]
    fn test_offset_to_line() {
        let text = "line1\nline2\nline3";

        assert_eq!(offset_to_line(text, 0), 0);  // Start of line1
        assert_eq!(offset_to_line(text, 5), 0);  // End of line1 (before \n)
        assert_eq!(offset_to_line(text, 6), 1);  // Start of line2
        assert_eq!(offset_to_line(text, 12), 2); // Start of line3
    }

    #[test]
    fn test_folding_ranges_empty() {
        let text = "";
        let ranges = provide_folding_ranges(text);
        assert!(ranges.is_none());
    }
}
