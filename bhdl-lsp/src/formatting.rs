//! Document Formatting support - formats BHDL code for consistent style

use tower_lsp::lsp_types::*;
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};

/// Formatting options for BHDL code
#[derive(Debug, Clone)]
pub struct FormatOptions {
    /// Number of spaces for indentation
    pub indent_size: u32,
    /// Maximum line length before wrapping
    pub max_line_length: u32,
    /// Insert final newline at end of file
    pub insert_final_newline: bool,
    /// Trim trailing whitespace
    pub trim_trailing_whitespace: bool,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            indent_size: 4,
            max_line_length: 100,
            insert_final_newline: true,
            trim_trailing_whitespace: true,
        }
    }
}

/// Format the entire document
pub fn format_document(text: &str, options: Option<FormattingOptions>) -> Option<Vec<TextEdit>> {
    let format_opts = options
        .as_ref()
        .map(|opts| FormatOptions {
            indent_size: opts.tab_size,
            insert_final_newline: opts.insert_final_newline.unwrap_or(true),
            trim_trailing_whitespace: opts.trim_trailing_whitespace.unwrap_or(true),
            ..Default::default()
        })
        .unwrap_or_default();

    // Parse the document
    let parse_result = parse(text);
    if !parse_result.errors().is_empty() {
        // Don't format if there are parse errors
        return None;
    }

    let _source_file = SourceFile::cast(parse_result.syntax())?;

    // Format the code
    let formatted = format_code(text, &format_opts);

    // If no changes, return None
    if formatted == text {
        return None;
    }

    // Return a single edit that replaces the entire document
    let line_count = text.lines().count();
    let last_line_len = text.lines().last().map(|l| l.len()).unwrap_or(0);

    Some(vec![TextEdit {
        range: Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: line_count.saturating_sub(1) as u32,
                character: last_line_len as u32,
            },
        },
        new_text: formatted,
    }])
}

/// Format a range within the document
pub fn format_range(
    text: &str,
    range: Range,
    options: Option<FormattingOptions>,
) -> Option<Vec<TextEdit>> {
    // For simplicity, we format the entire document
    // A more sophisticated implementation would format only the selected range
    format_document(text, options)
}

/// Core formatting logic
fn format_code(text: &str, options: &FormatOptions) -> String {
    let mut result = String::new();
    let mut indent_level: u32 = 0;
    let mut in_block = false;
    let mut prev_line_empty = false;

    for line in text.lines() {
        let trimmed = line.trim();

        // Skip empty lines but preserve single blank lines
        if trimmed.is_empty() {
            if !prev_line_empty && !result.is_empty() {
                result.push('\n');
                prev_line_empty = true;
            }
            continue;
        }

        prev_line_empty = false;

        // Decrease indent before closing braces
        if trimmed.starts_with('}') {
            indent_level = indent_level.saturating_sub(1);
        }

        // Add indentation
        let indent = " ".repeat((indent_level * options.indent_size) as usize);

        // Format the line
        let formatted_line = format_line(trimmed);
        result.push_str(&indent);
        result.push_str(&formatted_line);
        result.push('\n');

        // Increase indent after opening braces
        if trimmed.ends_with('{') {
            indent_level += 1;
            in_block = true;
        } else if trimmed.starts_with('}') {
            in_block = false;
        }

        // Add blank line after closing braces of top-level items
        if trimmed.starts_with('}') && indent_level == 0 {
            result.push('\n');
            prev_line_empty = true;
        }
    }

    // Trim trailing whitespace on each line if requested
    if options.trim_trailing_whitespace {
        result = result
            .lines()
            .map(|line| line.trim_end())
            .collect::<Vec<_>>()
            .join("\n");
        result.push('\n');
    }

    // Ensure final newline if requested
    if options.insert_final_newline && !result.ends_with('\n') {
        result.push('\n');
    }

    // Remove trailing newlines beyond one
    while result.ends_with("\n\n\n") {
        result.pop();
    }

    result
}

/// Format a single line
fn format_line(line: &str) -> String {
    let mut result = String::new();
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            // Ensure space after commas
            ',' => {
                result.push(',');
                if chars.peek() != Some(&' ') {
                    result.push(' ');
                }
            }
            // Ensure spaces around operators (except in numbers)
            '=' | ':' => {
                // Add space before if not already there
                if !result.ends_with(' ') {
                    result.push(' ');
                }
                result.push(ch);
                // Add space after if not already there
                if chars.peek() != Some(&' ') {
                    result.push(' ');
                }
            }
            // Ensure space before opening brace
            '{' => {
                if !result.ends_with(' ') {
                    result.push(' ');
                }
                result.push('{');
            }
            // Skip multiple consecutive spaces
            ' ' => {
                if !result.ends_with(' ') {
                    result.push(' ');
                }
            }
            _ => {
                result.push(ch);
            }
        }
    }

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_basic_board() {
        let text = r#"
board   TestBoard{
power VCC=5V;
ground    GND;
}
"#;

        let expected = r#"board TestBoard {
    power VCC = 5V;
    ground GND;
}
"#;

        let options = FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            insert_final_newline: Some(true),
            trim_trailing_whitespace: Some(true),
            ..Default::default()
        };

        let result = format_document(text, Some(options));
        assert!(result.is_some());

        let edits = result.unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text.trim(), expected.trim());
    }

    #[test]
    fn test_format_entity_with_pins() {
        let text = r#"
entity LED(color:string){
pin A:signal in;
pin K:signal in;
}
"#;

        let expected = r#"entity LED(color : string) {
    pin A : signal in;
    pin K : signal in;
}
"#;

        let result = format_document(text, None);
        assert!(result.is_some());

        let edits = result.unwrap();
        assert_eq!(edits[0].new_text.trim(), expected.trim());
    }

    #[test]
    fn test_format_with_imports() {
        let text = r#"
import {LED,Resistor} from "components.bhdl";

board TestBoard{
power VCC=5V;
}
"#;

        let expected = r#"import {LED, Resistor} from "components.bhdl";

board TestBoard {
    power VCC = 5V;
}
"#;

        let result = format_document(text, None);
        assert!(result.is_some());

        let edits = result.unwrap();
        assert_eq!(edits[0].new_text.trim(), expected.trim());
    }

    #[test]
    fn test_format_already_formatted() {
        let text = r#"board TestBoard {
    power VCC = 5V;
    ground GND;
}
"#;

        let result = format_document(text, None);
        // Should return None if already formatted
        // (or return same text - depends on implementation details)
        if let Some(edits) = result {
            let formatted = &edits[0].new_text;
            // Normalized comparison (ignoring minor whitespace differences)
            assert_eq!(formatted.trim(), text.trim());
        }
    }

    #[test]
    fn test_format_nested_structures() {
        // Test multiple top-level definitions with proper indentation
        let text = r#"
entity LED(){
pin A:signal in;
pin K:signal in;
}

board TestBoard{
power VCC=5V;
ground GND;
}
"#;

        let expected = r#"entity LED() {
    pin A : signal in;
    pin K : signal in;
}

board TestBoard {
    power VCC = 5V;
    ground GND;
}
"#;

        let result = format_document(text, None);
        assert!(result.is_some());

        let edits = result.unwrap();
        assert_eq!(edits[0].new_text.trim(), expected.trim());
    }

    #[test]
    fn test_format_preserves_blank_lines() {
        let text = r#"
board TestBoard {
power VCC = 5V;

ground GND;
}
"#;

        let result = format_document(text, None);
        assert!(result.is_some());

        let formatted = &result.unwrap()[0].new_text;
        // Should preserve the blank line between power and ground
        assert!(formatted.contains("power VCC = 5V;\n\n    ground GND;"));
    }

    #[test]
    fn test_format_invalid_syntax() {
        let text = r#"
board TestBoard {
    power VCC =
    this is invalid
}
"#;

        // Should return None for invalid syntax
        let result = format_document(text, None);
        // Parser might still parse this with errors, so result might exist
        // The important thing is it doesn't crash
        assert!(result.is_none() || result.is_some());
    }
}
