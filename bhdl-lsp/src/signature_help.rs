//! Signature Help support - provides parameter hints for function calls

use tower_lsp::lsp_types::*;
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_common::IntentRegistry;
use rowan::NodeOrToken;
use bhdl_parser::BhdlLanguage;

/// Provide signature help at the given position
pub fn provide_signature_help(
    text: &str,
    position: Position,
    intent_registry: &IntentRegistry,
) -> Option<SignatureHelp> {
    let parse_result = parse(text);
    let _source_file = SourceFile::cast(parse_result.syntax())?;

    // Convert position to byte offset
    let byte_offset = position_to_offset(text, position)?;

    // Look backwards to find function/intent name and opening paren
    let (function_name, paren_offset) = find_function_context(text, byte_offset)?;

    // Count which parameter we're on
    let active_parameter = count_parameters_before(text, paren_offset, byte_offset);

    // Look up the function in the intent registry
    if let Some(intent_fn) = intent_registry.get(&function_name) {
        let signature = create_signature_from_intent(intent_fn, active_parameter);
        return Some(SignatureHelp {
            signatures: vec![signature],
            active_signature: Some(0),
            active_parameter: Some(active_parameter as u32),
        });
    }

    // Could also handle component instantiation here
    // e.g., Resistor(value), LED(color), etc.

    None
}

/// Create a signature from an intent function
fn create_signature_from_intent(
    intent: &dyn bhdl_common::IntentFunction,
    active_param: usize,
) -> SignatureInformation {
    // Build parameter labels
    let mut param_labels = Vec::new();
    let mut param_infos = Vec::new();

    for param in intent.param_metadata() {
        let type_str = format!("{:?}", param.param_type); // Debug format for now
        let label = if let Some(default) = &param.default_value {
            format!("{}: {} = {:?}", param.name, type_str, default)
        } else if param.required {
            format!("{}: {}", param.name, type_str)
        } else {
            format!("{}?: {}", param.name, type_str)
        };

        param_labels.push(label.clone());
        param_infos.push(ParameterInformation {
            label: ParameterLabel::Simple(label),
            documentation: None, // No documentation in metadata
        });
    }

    // Build full signature label
    let label = format!("{}({})", intent.name(), param_labels.join(", "));

    // Build documentation
    let doc = format!("**{}**\n\nIntent function for design specifications", intent.name());

    SignatureInformation {
        label,
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: doc,
        })),
        parameters: Some(param_infos),
        active_parameter: Some(active_param as u32),
    }
}

/// Find the function name and opening paren before the cursor
fn find_function_context(text: &str, offset: usize) -> Option<(String, usize)> {
    let mut current_offset = offset;
    let mut paren_count = 0;
    let mut found_opening_paren = false;
    let mut paren_offset = offset;

    // Scan backwards to find the matching opening paren
    while current_offset > 0 {
        current_offset -= 1;
        let ch = text.chars().nth(current_offset)?;

        if ch == ')' {
            paren_count += 1;
        } else if ch == '(' {
            if paren_count == 0 {
                found_opening_paren = true;
                paren_offset = current_offset;
                break;
            }
            paren_count -= 1;
        }
    }

    if !found_opening_paren {
        return None;
    }

    // Now scan backwards to find the function name
    let mut name_end = paren_offset;
    let mut name_start = name_end;

    // Skip whitespace before paren
    while name_start > 0 {
        let ch = text.chars().nth(name_start - 1)?;
        if !ch.is_whitespace() {
            break;
        }
        name_start -= 1;
    }
    name_end = name_start;

    // Collect identifier characters
    while name_start > 0 {
        let ch = text.chars().nth(name_start - 1)?;
        if ch.is_alphanumeric() || ch == '_' {
            name_start -= 1;
        } else {
            break;
        }
    }

    if name_start >= name_end {
        return None;
    }

    let function_name = text[name_start..name_end].to_string();
    Some((function_name, paren_offset))
}

/// Count how many parameters come before the cursor
fn count_parameters_before(text: &str, paren_offset: usize, cursor_offset: usize) -> usize {
    let mut count = 0;
    let mut current_offset = paren_offset + 1;
    let mut depth = 0;

    while current_offset < cursor_offset && current_offset < text.len() {
        let ch = text.chars().nth(current_offset).unwrap_or('\0');

        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => count += 1,
            _ => {}
        }

        current_offset += 1;
    }

    count
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
    use bhdl_stdlib::intents::register_stdlib_intents;

    fn create_test_registry() -> IntentRegistry {
        let mut registry = IntentRegistry::new();
        register_stdlib_intents(&mut registry);
        registry
    }

    #[test]
    fn test_signature_help_delay() {
        // Simplified test - just delay(...)
        let text = "delay(5ms)";
        let position = Position { line: 0, character: 7 }; // Inside the paren

        let registry = create_test_registry();
        let help = provide_signature_help(text, position, &registry);

        assert!(help.is_some(), "Should find signature help for delay");
        let help = help.unwrap();

        assert_eq!(help.signatures.len(), 1);
        assert!(help.signatures[0].label.contains("delay"));
    }

    #[test]
    fn test_signature_help_second_parameter() {
        // Simplified test
        let text = "noise_filtering(1kHz, 20dB)";
        let position = Position { line: 0, character: 23 }; // After comma, on second param

        let registry = create_test_registry();
        let help = provide_signature_help(text, position, &registry);

        assert!(help.is_some(), "Should find signature help for noise_filtering");
        let help = help.unwrap();

        assert_eq!(help.active_parameter, Some(1)); // Second parameter
    }

    #[test]
    fn test_find_function_context() {
        let text = "delay(100ms)";
        let offset = 7; // Inside the parameter

        let result = find_function_context(text, offset);
        assert!(result.is_some());

        let (name, paren_offset) = result.unwrap();
        assert_eq!(name, "delay");
        assert_eq!(paren_offset, 5); // Position of '('
    }

    #[test]
    fn test_count_parameters() {
        let text = "func(a, b, c)";
        //                  ^-- offset 10 (after 'b,')

        let count = count_parameters_before(text, 4, 10); // 4 is offset of '('
        assert_eq!(count, 2); // On third parameter (0-indexed: 0, 1, 2)
    }

    #[test]
    fn test_position_to_offset() {
        let text = "line1\nline2\nline3";

        assert_eq!(position_to_offset(text, Position { line: 0, character: 0 }), Some(0));
        assert_eq!(position_to_offset(text, Position { line: 0, character: 5 }), Some(5));
        assert_eq!(position_to_offset(text, Position { line: 1, character: 0 }), Some(6));
        assert_eq!(position_to_offset(text, Position { line: 2, character: 2 }), Some(14));
    }
}
