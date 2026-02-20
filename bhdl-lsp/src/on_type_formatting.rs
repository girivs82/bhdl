//! On Type Formatting - automatic formatting as you type

use tower_lsp::lsp_types::*;
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};

/// Provide on-type formatting
pub fn on_type_formatting(
    text: &str,
    position: Position,
    ch: &str,
    options: FormattingOptions,
) -> Option<Vec<TextEdit>> {
    match ch {
        "\n" => handle_newline(text, position, &options),
        "}" => handle_closing_brace(text, position, &options),
        ";" => handle_semicolon(text, position, &options),
        _ => None,
    }
}

/// Handle newline - auto-indent the new line
fn handle_newline(
    text: &str,
    position: Position,
    options: &FormattingOptions,
) -> Option<Vec<TextEdit>> {
    // Split text into lines, preserving empty lines from trailing newlines
    let mut lines: Vec<&str> = text.split('\n').collect();

    // Handle trailing newline case
    if text.ends_with('\n') && lines.last() == Some(&"") {
        // Keep the empty line
    }

    let line_idx = position.line as usize;

    if line_idx == 0 || line_idx >= lines.len() {
        return None;
    }

    // Get the previous line
    let prev_line = lines[line_idx - 1];

    // Calculate indentation for the new line
    let indent = calculate_indent(prev_line, options);

    if indent.is_empty() {
        return None;
    }

    // Insert indentation at the beginning of the current line
    Some(vec![TextEdit {
        range: Range {
            start: Position {
                line: position.line,
                character: 0,
            },
            end: Position {
                line: position.line,
                character: 0,
            },
        },
        new_text: indent,
    }])
}

/// Calculate indentation for a new line based on previous line
fn calculate_indent(prev_line: &str, options: &FormattingOptions) -> String {
    let base_indent = get_line_indentation(prev_line);
    let indent_size = options.tab_size as usize;
    let trimmed = prev_line.trim();

    // If previous line ends with {, increase indent
    if trimmed.ends_with('{') {
        format!("{}{}", base_indent, " ".repeat(indent_size))
    } else {
        base_indent
    }
}

/// Get the indentation of a line
fn get_line_indentation(line: &str) -> String {
    line.chars()
        .take_while(|c| c.is_whitespace())
        .collect()
}

/// Handle closing brace - auto-dedent to match opening brace
fn handle_closing_brace(
    text: &str,
    position: Position,
    options: &FormattingOptions,
) -> Option<Vec<TextEdit>> {
    let lines: Vec<&str> = text.split('\n').collect();
    let line_idx = position.line as usize;

    if line_idx >= lines.len() {
        return None;
    }

    let current_line = lines[line_idx];
    let current_indent = get_line_indentation(current_line);

    // Calculate correct indentation by finding matching opening brace
    let target_indent = calculate_closing_brace_indent(text, position, options)?;

    // If indentation is already correct, no changes needed
    if current_indent == target_indent {
        return None;
    }

    // Replace the current indentation with the correct one
    Some(vec![TextEdit {
        range: Range {
            start: Position {
                line: position.line,
                character: 0,
            },
            end: Position {
                line: position.line,
                character: current_indent.len() as u32,
            },
        },
        new_text: target_indent,
    }])
}

/// Calculate the correct indentation for a closing brace
fn calculate_closing_brace_indent(
    text: &str,
    position: Position,
    _options: &FormattingOptions,
) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let line_idx = position.line as usize;

    // Count braces to find the matching level
    // Start at -1 because we need to skip the closing brace we just typed
    let mut brace_count = -1;

    // Work backwards to find matching opening brace
    for i in (0..=line_idx).rev() {
        let line = lines[i];
        let trimmed = line.trim();

        // Count braces in this line (in reverse order since we're going backwards)
        for ch in trimmed.chars().rev() {
            if ch == '}' {
                brace_count += 1;
            } else if ch == '{' {
                if brace_count == 0 {
                    // Found matching opening brace
                    return Some(get_line_indentation(line));
                }
                brace_count -= 1;
            }
        }
    }

    // If no matching brace found, use no indentation
    Some(String::new())
}

/// Handle semicolon - format the current line
fn handle_semicolon(
    text: &str,
    position: Position,
    _options: &FormattingOptions,
) -> Option<Vec<TextEdit>> {
    let lines: Vec<&str> = text.split('\n').collect();
    let line_idx = position.line as usize;

    if line_idx >= lines.len() {
        return None;
    }

    let current_line = lines[line_idx];
    let current_indent = get_line_indentation(current_line);
    let trimmed = current_line.trim();

    // Format the line content (add spaces around operators, etc.)
    let formatted = format_line_content(trimmed);

    // If the line hasn't changed, no edits needed
    let new_line = format!("{}{}", current_indent, formatted);
    if new_line == current_line {
        return None;
    }

    // Replace the entire line with formatted version
    Some(vec![TextEdit {
        range: Range {
            start: Position {
                line: position.line,
                character: 0,
            },
            end: Position {
                line: position.line,
                character: current_line.len() as u32,
            },
        },
        new_text: new_line,
    }])
}

/// Format a line's content (similar to formatting.rs but for a single line)
fn format_line_content(line: &str) -> String {
    let mut result = String::new();
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            ',' => {
                result.push(',');
                if chars.peek() != Some(&' ') {
                    result.push(' ');
                }
            }
            '=' | ':' => {
                if !result.ends_with(' ') {
                    result.push(' ');
                }
                result.push(ch);
                if chars.peek() != Some(&' ') {
                    result.push(' ');
                }
            }
            '{' => {
                if !result.ends_with(' ') {
                    result.push(' ');
                }
                result.push('{');
            }
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

    fn default_options() -> FormattingOptions {
        FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            insert_final_newline: None,
            trim_trailing_whitespace: None,
            ..Default::default()
        }
    }

    #[test]
    fn test_newline_after_opening_brace() {
        let text = "board TestBoard {\n";
        let position = Position {
            line: 1,
            character: 0,
        };

        let result = on_type_formatting(text, position, "\n", default_options());
        assert!(result.is_some());

        let edits = result.unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "    "); // 4 spaces
    }

    #[test]
    fn test_newline_after_regular_line() {
        let text = "    power VCC = 5V;\n";
        let position = Position {
            line: 1,
            character: 0,
        };

        let result = on_type_formatting(text, position, "\n", default_options());
        assert!(result.is_some());

        let edits = result.unwrap();
        assert_eq!(edits[0].new_text, "    "); // Same indent as previous line
    }

    #[test]
    fn test_closing_brace_dedent() {
        let text = "board TestBoard {\n    power VCC = 5V;\n    }";
        let position = Position {
            line: 2,
            character: 5, // After the }
        };

        let result = on_type_formatting(text, position, "}", default_options());
        assert!(result.is_some());

        let edits = result.unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, ""); // Remove indentation
    }

    #[test]
    fn test_closing_brace_already_correct() {
        let text = "board TestBoard {\n    power VCC = 5V;\n}";
        let position = Position {
            line: 2,
            character: 1,
        };

        let result = on_type_formatting(text, position, "}", default_options());
        // No changes needed - already correctly indented
        assert!(result.is_none());
    }

    #[test]
    fn test_semicolon_formats_line() {
        let text = "    power VCC=5V;";
        let position = Position {
            line: 0,
            character: 16,
        };

        let result = on_type_formatting(text, position, ";", default_options());
        assert!(result.is_some());

        let edits = result.unwrap();
        assert!(edits[0].new_text.contains("VCC = 5V")); // Spaces around =
    }

    #[test]
    fn test_semicolon_no_changes_needed() {
        let text = "    power VCC = 5V;";
        let position = Position {
            line: 0,
            character: 19,
        };

        let result = on_type_formatting(text, position, ";", default_options());
        // Already formatted correctly
        assert!(result.is_none());
    }

    #[test]
    fn test_nested_braces() {
        let text = r#"board TestBoard {
    entity Regulator() {
        pin IN: power in;
        }
}"#;
        let position = Position {
            line: 3,
            character: 9,
        };

        let result = on_type_formatting(text, position, "}", default_options());
        assert!(result.is_some());

        let edits = result.unwrap();
        // Should dedent to match entity line (4 spaces)
        assert_eq!(edits[0].new_text, "    ");
    }

    #[test]
    fn test_unsupported_character() {
        let text = "board TestBoard";
        let position = Position {
            line: 0,
            character: 5,
        };

        let result = on_type_formatting(text, position, "a", default_options());
        assert!(result.is_none());
    }
}
