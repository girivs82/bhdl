//! Call Hierarchy support - shows entity/component instantiation relationships

use tower_lsp::lsp_types::*;
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;

/// Prepare call hierarchy item at the given position
pub fn prepare_call_hierarchy(
    text: &str,
    position: Position,
) -> Option<Vec<CallHierarchyItem>> {
    let parse_result = parse(text);
    let source_file = SourceFile::cast(parse_result.syntax())?;
    let analysis_result = analyze(&source_file);

    // Convert position to offset
    let offset = position_to_offset(text, position)?;

    // Find the symbol at the cursor position
    let symbol = find_symbol_at_position(&analysis_result, offset)?;

    // Create call hierarchy item
    let item = create_call_hierarchy_item(&symbol, text)?;

    Some(vec![item])
}

/// Find incoming calls (who instantiates this entity/component)
pub fn incoming_calls(
    text: &str,
    item: &CallHierarchyItem,
) -> Option<Vec<CallHierarchyIncomingCall>> {
    let parse_result = parse(text);
    let source_file = SourceFile::cast(parse_result.syntax())?;
    let analysis_result = analyze(&source_file);

    let mut incoming = Vec::new();

    // Get the name of the item we're looking for instantiations of
    let item_name = &item.name;

    // Search through all symbols for instances of this entity/component
    for symbol in analysis_result.global_scope.iter() {
        if let Some(instance_type) = &symbol.instance_type_name {
            if instance_type == item_name {
                // This is an instantiation of our item
                if let Some(call_item) = create_call_hierarchy_item(symbol, text) {
                    let from_ranges = vec![call_item.selection_range];
                    incoming.push(CallHierarchyIncomingCall {
                        from: call_item,
                        from_ranges,
                    });
                }
            }
        }
    }

    // Also check definition scopes
    for (_node_ptr, scope) in &analysis_result.definition_scopes {
        for symbol in scope.iter() {
            if let Some(instance_type) = &symbol.instance_type_name {
                if instance_type == item_name {
                    if let Some(call_item) = create_call_hierarchy_item(symbol, text) {
                        let from_ranges = vec![call_item.selection_range];
                        incoming.push(CallHierarchyIncomingCall {
                            from: call_item,
                            from_ranges,
                        });
                    }
                }
            }
        }
    }

    if incoming.is_empty() {
        None
    } else {
        Some(incoming)
    }
}

/// Find outgoing calls (what entities/components does this instantiate)
pub fn outgoing_calls(
    text: &str,
    item: &CallHierarchyItem,
) -> Option<Vec<CallHierarchyOutgoingCall>> {
    let parse_result = parse(text);
    let source_file = SourceFile::cast(parse_result.syntax())?;
    let analysis_result = analyze(&source_file);

    let mut outgoing = Vec::new();

    // Find the definition scope for this item
    // Look through the definition scopes to find instances within this item
    for (_node_ptr, scope) in &analysis_result.definition_scopes {
        // Check if this scope belongs to our item
        if let Some(scope_name) = &scope.scope_name {
            if scope_name == &item.name {
                // Found our scope, now find all instances within it
                for symbol in scope.iter() {
                    if let Some(instance_type) = &symbol.instance_type_name {
                        // This is an instantiation - find its definition
                        if let Some(target_symbol) = analysis_result.global_scope.lookup(instance_type) {
                            if let Some(call_item) = create_call_hierarchy_item(target_symbol, text) {
                                let from_ranges = vec![text_range_to_lsp_range(&symbol.span)];
                                outgoing.push(CallHierarchyOutgoingCall {
                                    to: call_item,
                                    from_ranges,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    if outgoing.is_empty() {
        None
    } else {
        Some(outgoing)
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

/// Create a CallHierarchyItem from a symbol
fn create_call_hierarchy_item(
    symbol: &bhdl_analyzer::symbol_table::Symbol,
    text: &str,
) -> Option<CallHierarchyItem> {
    use bhdl_analyzer::symbol_table::SymbolKind as ASymbolKind;

    let kind = match symbol.kind {
        ASymbolKind::Entity => SymbolKind::FUNCTION,
        ASymbolKind::Component => SymbolKind::CLASS,
        ASymbolKind::Instance => SymbolKind::OBJECT,
        _ => return None, // Only interested in entities, components, and instances
    };

    let range = text_range_to_lsp_range(&symbol.span);

    // Use a placeholder URI - this will be replaced by the handler
    let uri = Url::parse("file:///current_document").unwrap();

    Some(CallHierarchyItem {
        name: symbol.name.clone(),
        kind,
        tags: None,
        detail: symbol.instance_type_name.clone(),
        uri,
        range,
        selection_range: range,
        data: None,
    })
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
    fn test_prepare_call_hierarchy() {
        let text = r#"
entity Regulator() {
    pin IN: power in;
}

board TestBoard {
    Regulator();
}
"#;

        // Position on "Regulator" entity definition (line 1)
        let position = Position { line: 1, character: 8 };

        let result = prepare_call_hierarchy(text, position);
        assert!(result.is_some());

        let items = result.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Regulator");
        assert_eq!(items[0].kind, SymbolKind::FUNCTION);
    }

    #[test]
    fn test_incoming_calls() {
        let text = r#"
entity Regulator() {
    pin IN: power in;
}

board TestBoard {
    Regulator();
}
"#;

        // Prepare item for Regulator
        let position = Position { line: 1, character: 8 };
        let items = prepare_call_hierarchy(text, position).unwrap();
        let regulator_item = &items[0];

        // Get incoming calls (who instantiates Regulator)
        let incoming = incoming_calls(text, regulator_item);
        assert!(incoming.is_some());

        let calls = incoming.unwrap();
        // Should find the instantiation in TestBoard
        assert!(!calls.is_empty());
    }

    #[test]
    fn test_outgoing_calls() {
        let text = r#"
entity LED() {
    pin A: signal in;
}

entity Regulator() {
    LED();
}
"#;

        // Prepare item for Regulator (line 5 is where "entity Regulator()" starts)
        let position = Position { line: 5, character: 8 };
        let items = prepare_call_hierarchy(text, position).unwrap();
        let regulator_item = &items[0];

        // Get outgoing calls (what Regulator instantiates)
        let outgoing = outgoing_calls(text, regulator_item);
        assert!(outgoing.is_some());

        let calls = outgoing.unwrap();
        // Should find LED instantiation
        assert!(!calls.is_empty());
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
    fn test_no_calls() {
        let text = r#"
entity Regulator() {
    pin IN: power in;
}
"#;

        // Prepare item
        let position = Position { line: 1, character: 8 };
        let items = prepare_call_hierarchy(text, position).unwrap();
        let regulator_item = &items[0];

        // No incoming calls (nothing instantiates it)
        let incoming = incoming_calls(text, regulator_item);
        assert!(incoming.is_none());

        // No outgoing calls (it doesn't instantiate anything)
        let outgoing = outgoing_calls(text, regulator_item);
        assert!(outgoing.is_none());
    }
}
