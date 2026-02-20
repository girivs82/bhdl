//! Code Lens support - provides inline actionable information and metrics

use tower_lsp::lsp_types::*;
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;

/// Provide code lenses for the document
pub fn provide_code_lens(text: &str) -> Option<Vec<CodeLens>> {
    let parse_result = parse(text);
    let source_file = SourceFile::cast(parse_result.syntax())?;
    let analysis_result = analyze(&source_file);

    let mut lenses = Vec::new();

    // Add reference count lenses for modules and components
    for symbol in analysis_result.global_scope.iter() {
        use bhdl_analyzer::symbol_table::SymbolKind;

        match symbol.kind {
            SymbolKind::Entity | SymbolKind::Component => {
                // Count references to this entity/component
                let ref_count = count_references(&analysis_result, &symbol.name);

                if ref_count > 0 {
                    let range = text_range_to_lsp_range(&symbol.span);

                    lenses.push(CodeLens {
                        range,
                        command: Some(Command {
                            title: format!("{} reference{}", ref_count, if ref_count == 1 { "" } else { "s" }),
                            command: "bhdl.showReferences".to_string(),
                            arguments: None,
                        }),
                        data: None,
                    });
                }
            }
            SymbolKind::Board => {
                // Count components/entities in this board
                let component_count = count_components_in_board(&analysis_result, &symbol.name);

                if component_count > 0 {
                    let range = text_range_to_lsp_range(&symbol.span);

                    lenses.push(CodeLens {
                        range,
                        command: Some(Command {
                            title: format!("{} component{}", component_count, if component_count == 1 { "" } else { "s" }),
                            command: "bhdl.showComponents".to_string(),
                            arguments: None,
                        }),
                        data: None,
                    });
                }
            }
            _ => {}
        }
    }

    // Add pin count lenses for entities
    for symbol in analysis_result.global_scope.iter() {
        if symbol.kind == bhdl_analyzer::symbol_table::SymbolKind::Entity {
            let pin_count = count_pins_in_entity(&analysis_result, &symbol.name);

            if pin_count > 0 {
                let range = text_range_to_lsp_range(&symbol.span);

                // Check if we already have a lens at this range (reference count)
                let has_existing = lenses.iter().any(|l| l.range == range);

                if has_existing {
                    // Append to existing lens title
                    if let Some(lens) = lenses.iter_mut().find(|l| l.range == range) {
                        if let Some(ref mut cmd) = lens.command {
                            cmd.title = format!("{} | {} pin{}",
                                cmd.title,
                                pin_count,
                                if pin_count == 1 { "" } else { "s" }
                            );
                        }
                    }
                } else {
                    lenses.push(CodeLens {
                        range,
                        command: Some(Command {
                            title: format!("{} pin{}", pin_count, if pin_count == 1 { "" } else { "s" }),
                            command: "bhdl.showPins".to_string(),
                            arguments: None,
                        }),
                        data: None,
                    });
                }
            }
        }
    }

    if lenses.is_empty() {
        None
    } else {
        Some(lenses)
    }
}

/// Count references to a symbol
fn count_references(analysis: &bhdl_analyzer::AnalysisResult, name: &str) -> usize {
    let mut count = 0;

    // Count instances in global scope
    for symbol in analysis.global_scope.iter() {
        if symbol.kind == bhdl_analyzer::symbol_table::SymbolKind::Instance {
            if let Some(ref type_name) = symbol.instance_type_name {
                if type_name == name {
                    count += 1;
                }
            }
        }
    }

    // Count instances in definition scopes
    for (_node_ptr, scope) in &analysis.definition_scopes {
        for symbol in scope.iter() {
            if symbol.kind == bhdl_analyzer::symbol_table::SymbolKind::Instance {
                if let Some(ref type_name) = symbol.instance_type_name {
                    if type_name == name {
                        count += 1;
                    }
                }
            }
        }
    }

    count
}

/// Count components/entities in a board
fn count_components_in_board(analysis: &bhdl_analyzer::AnalysisResult, board_name: &str) -> usize {
    // Find the board's definition scope
    for (_node_ptr, scope) in &analysis.definition_scopes {
        if let Some(ref scope_name) = scope.scope_name {
            if scope_name == board_name {
                // Count instances in this scope
                return scope.iter()
                    .filter(|s| s.kind == bhdl_analyzer::symbol_table::SymbolKind::Instance)
                    .count();
            }
        }
    }

    0
}

/// Count pins in an entity
fn count_pins_in_entity(analysis: &bhdl_analyzer::AnalysisResult, entity_name: &str) -> usize {
    // Find the entity's definition scope
    for (_node_ptr, scope) in &analysis.definition_scopes {
        if let Some(ref scope_name) = scope.scope_name {
            if scope_name == entity_name {
                // Count pins and virtual pins in this scope
                return scope.iter()
                    .filter(|s| matches!(s.kind,
                        bhdl_analyzer::symbol_table::SymbolKind::Pin |
                        bhdl_analyzer::symbol_table::SymbolKind::VirtualPin
                    ))
                    .count();
            }
        }
    }

    0
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_lens_entity_references() {
        let text = r#"
entity LED() {
    pin A: signal in;
}

board TestBoard {
    LED();
    LED();
}
"#;

        let result = provide_code_lens(text);
        assert!(result.is_some());

        let lenses = result.unwrap();
        // Should have lens for LED entity showing reference count
        let led_lens = lenses.iter().find(|l| {
            l.command.as_ref().map(|c| c.title.contains("reference")).unwrap_or(false)
        });
        assert!(led_lens.is_some(), "Should have a lens with 'reference' in title");

        let lens = led_lens.unwrap();
        let title = &lens.command.as_ref().unwrap().title;
        // Should have at least 1 reference
        assert!(title.contains("reference"), "Title should contain 'reference', got: {}", title);
    }

    #[test]
    fn test_code_lens_board_components() {
        let text = r#"
entity LED() {
    pin A: signal in;
}

entity Resistor() {
    pin 1: signal inout;
    pin 2: signal inout;
}

board TestBoard {
    LED();
    Resistor();
}
"#;

        let result = provide_code_lens(text);
        assert!(result.is_some());

        let lenses = result.unwrap();
        // Should have lens for TestBoard showing component count
        let board_lens = lenses.iter().find(|l| {
            l.command.as_ref().map(|c| c.title.contains("component")).unwrap_or(false)
        });
        assert!(board_lens.is_some());

        let lens = board_lens.unwrap();
        let title = &lens.command.as_ref().unwrap().title;
        assert!(title.contains("2 components"));
    }

    #[test]
    fn test_code_lens_entity_pins() {
        let text = r#"
entity Regulator() {
    pin VIN: power in;
    pin VOUT: power out;
    pin GND: ground;
    pin EN: signal in;
}
"#;

        let result = provide_code_lens(text);
        assert!(result.is_some());

        let lenses = result.unwrap();
        // Should have lens showing pin count
        let pin_lens = lenses.iter().find(|l| {
            l.command.as_ref().map(|c| c.title.contains("pin")).unwrap_or(false)
        });
        assert!(pin_lens.is_some());

        let lens = pin_lens.unwrap();
        let title = &lens.command.as_ref().unwrap().title;
        assert!(title.contains("4 pins"));
    }

    #[test]
    fn test_code_lens_combined_info() {
        let text = r#"
entity LED() {
    pin A: signal in;
    pin K: signal in;
}

board TestBoard {
    LED();
}
"#;

        let result = provide_code_lens(text);
        assert!(result.is_some());

        let lenses = result.unwrap();
        // LED entity should have both reference count and pin count
        let led_lenses: Vec<_> = lenses.iter().filter(|l| {
            l.command.as_ref().map(|c|
                c.title.contains("reference") || c.title.contains("pin")
            ).unwrap_or(false)
        }).collect();

        assert!(!led_lenses.is_empty());
    }

    #[test]
    fn test_code_lens_no_references() {
        let text = r#"
entity UnusedEntity() {
    pin A: signal in;
}
"#;

        let result = provide_code_lens(text);
        // Should still have pin count lens, but no reference count
        if let Some(lenses) = result {
            let ref_lens = lenses.iter().find(|l| {
                l.command.as_ref().map(|c| c.title.contains("reference")).unwrap_or(false)
            });
            assert!(ref_lens.is_none());

            let pin_lens = lenses.iter().find(|l| {
                l.command.as_ref().map(|c| c.title.contains("pin")).unwrap_or(false)
            });
            assert!(pin_lens.is_some());
        }
    }

    #[test]
    fn test_code_lens_empty_board() {
        let text = r#"
board EmptyBoard {
}
"#;

        let result = provide_code_lens(text);
        // Empty board should not have component count lens
        if let Some(lenses) = result {
            let board_lens = lenses.iter().find(|l| {
                l.command.as_ref().map(|c| c.title.contains("component")).unwrap_or(false)
            });
            assert!(board_lens.is_none());
        }
    }
}
