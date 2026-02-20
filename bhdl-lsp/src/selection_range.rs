//! Selection Range support - provides smart selection expansion based on AST structure

use tower_lsp::lsp_types::*;
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use rowan::TextRange;

/// Provide selection ranges for smart selection expansion
pub fn provide_selection_range(
    text: &str,
    positions: Vec<Position>,
) -> Option<Vec<SelectionRange>> {
    let parse_result = parse(text);
    let source_file = SourceFile::cast(parse_result.syntax())?;

    let mut ranges = Vec::new();

    for position in positions {
        if let Some(range) = get_selection_range_at_position(text, &source_file, position) {
            ranges.push(range);
        }
    }

    if ranges.is_empty() {
        None
    } else {
        Some(ranges)
    }
}

/// Get selection range at a specific position
fn get_selection_range_at_position(
    text: &str,
    source_file: &SourceFile,
    position: Position,
) -> Option<SelectionRange> {
    let offset = position_to_offset(text, position)?;

    // Find the token at the cursor position
    let token = source_file
        .syntax()
        .token_at_offset(rowan::TextSize::from(offset as u32))
        .right_biased()?;

    // Build selection range chain from token up to root
    let mut current_node: Option<rowan::SyntaxNode<bhdl_parser::BhdlLanguage>> = Some(token.parent()?);
    let mut parent_range: Option<Box<SelectionRange>> = None;

    // Start with the token itself
    let token_range = text_range_to_lsp_range(&token.text_range());
    let mut result = SelectionRange {
        range: token_range,
        parent: None,
    };

    // Build chain of parent nodes
    while let Some(node) = current_node {
        let node_range = text_range_to_lsp_range(&node.text_range());

        // Skip if this range is the same as the previous one (no expansion)
        if parent_range.is_none() || is_meaningful_expansion(&result.range, &node_range) {
            parent_range = Some(Box::new(SelectionRange {
                range: node_range,
                parent: parent_range,
            }));
        }

        current_node = node.parent();
    }

    // Return the chain with the token range at the innermost level
    result.parent = parent_range;
    Some(result)
}

/// Check if expanding from inner to outer is meaningful
fn is_meaningful_expansion(inner: &Range, outer: &Range) -> bool {
    // Outer should actually be larger than inner
    if outer.start.line < inner.start.line
        || (outer.start.line == inner.start.line && outer.start.character < inner.start.character)
    {
        return true;
    }
    if outer.end.line > inner.end.line
        || (outer.end.line == inner.end.line && outer.end.character > inner.end.character)
    {
        return true;
    }
    false
}

/// Convert rowan TextRange to LSP Range
fn text_range_to_lsp_range(text_range: &TextRange) -> Range {
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
    fn test_selection_range_basic() {
        let text = r#"
board TestBoard {
    power VCC = 5V;
}
"#;

        // Position on "VCC" identifier
        let position = Position { line: 2, character: 11 };

        let result = provide_selection_range(text, vec![position]);
        assert!(result.is_some());

        let ranges = result.unwrap();
        assert_eq!(ranges.len(), 1);

        // Should have at least the identifier and its parent nodes
        let range = &ranges[0];
        assert!(range.parent.is_some());
    }

    #[test]
    fn test_selection_range_multiple_positions() {
        let text = r#"
board TestBoard {
    power VCC = 5V;
    ground GND;
}
"#;

        let positions = vec![
            Position { line: 2, character: 11 }, // VCC
            Position { line: 3, character: 12 }, // GND
        ];

        let result = provide_selection_range(text, positions);
        assert!(result.is_some());

        let ranges = result.unwrap();
        assert_eq!(ranges.len(), 2);
    }

    #[test]
    fn test_selection_range_entity() {
        let text = r#"
entity Regulator() {
    pin IN: power in;
    pin OUT: power out;
}
"#;

        // Position on "Regulator"
        let position = Position { line: 1, character: 8 };

        let result = provide_selection_range(text, vec![position]);
        assert!(result.is_some());

        let ranges = result.unwrap();
        assert_eq!(ranges.len(), 1);

        // Should build a chain of parent nodes
        let mut current = &ranges[0];
        let mut depth = 0;
        while let Some(ref parent) = current.parent {
            depth += 1;
            current = parent;
        }

        // Should have multiple levels of nesting
        assert!(depth > 0);
    }

    #[test]
    fn test_selection_range_empty_text() {
        let text = "";
        let result = provide_selection_range(text, vec![Position { line: 0, character: 0 }]);
        assert!(result.is_none());
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
    fn test_is_meaningful_expansion() {
        let inner = Range {
            start: Position { line: 1, character: 5 },
            end: Position { line: 1, character: 10 },
        };

        let outer_larger = Range {
            start: Position { line: 1, character: 0 },
            end: Position { line: 1, character: 20 },
        };

        let outer_same = Range {
            start: Position { line: 1, character: 5 },
            end: Position { line: 1, character: 10 },
        };

        assert!(is_meaningful_expansion(&inner, &outer_larger));
        assert!(!is_meaningful_expansion(&inner, &outer_same));
    }
}
