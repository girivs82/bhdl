//! Document Symbols support - provides outline/structure view

use tower_lsp::lsp_types::*;
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile, Board, Entity, ComponentDef, PinDecl, PowerDecl, GroundDecl, HasName, V2FlowStmt};
use rowan::TextRange;

/// Provide document symbols for outline view
pub fn provide_document_symbols(text: &str) -> Option<DocumentSymbolResponse> {
    // Parse the document
    let parse_result = parse(text);
    let source_file = SourceFile::cast(parse_result.syntax())?;

    let mut symbols = Vec::new();

    // Process all top-level items
    for item in source_file.items() {
        if let Some(board) = Board::cast(item.syntax().clone()) {
            if let Some(symbol) = process_board(&board, text) {
                symbols.push(symbol);
            }
        } else if let Some(entity) = Entity::cast(item.syntax().clone()) {
            if let Some(symbol) = process_entity(&entity, text) {
                symbols.push(symbol);
            }
        } else if let Some(component) = ComponentDef::cast(item.syntax().clone()) {
            if let Some(symbol) = process_component(&component, text) {
                symbols.push(symbol);
            }
        }
    }

    if symbols.is_empty() {
        None
    } else {
        Some(DocumentSymbolResponse::Nested(symbols))
    }
}

/// Process a board definition
fn process_board(board: &Board, text: &str) -> Option<DocumentSymbol> {
    let name = board.name()?.text().to_string();
    let range = text_range_to_lsp_range(text, board.syntax().text_range());
    let selection_range = board.name()
        .map(|n| text_range_to_lsp_range(text, n.text_range()))
        .unwrap_or(range);

    let mut children = Vec::new();

    // Add power declarations
    for power in board.power_decls() {
        if let Some(child) = process_power_decl(&power, text) {
            children.push(child);
        }
    }

    // Add ground declarations
    for ground in board.ground_decls() {
        if let Some(child) = process_ground_decl(&ground, text) {
            children.push(child);
        }
    }

    // Add flow statements (nets in v2.0)
    for flow in board.flow_statements() {
        if let Some(child) = process_flow_stmt(&flow, text) {
            children.push(child);
        }
    }

    #[allow(deprecated)]
    Some(DocumentSymbol {
        name,
        detail: Some("board".to_string()),
        kind: SymbolKind::CLASS,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: if children.is_empty() { None } else { Some(children) },
    })
}

/// Process an entity definition
fn process_entity(entity: &Entity, text: &str) -> Option<DocumentSymbol> {
    let name = entity.name()?.text().to_string();
    let range = text_range_to_lsp_range(text, entity.syntax().text_range());
    let selection_range = entity.name()
        .map(|n| text_range_to_lsp_range(text, n.text_range()))
        .unwrap_or(range);

    // TODO: Extract pin declarations from entity body
    // For now, just show entity without children

    #[allow(deprecated)]
    Some(DocumentSymbol {
        name,
        detail: Some("entity".to_string()),
        kind: SymbolKind::MODULE,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: None,
    })
}

/// Process a component definition
fn process_component(component: &ComponentDef, text: &str) -> Option<DocumentSymbol> {
    let name = component.name()?.text().to_string();
    let range = text_range_to_lsp_range(text, component.syntax().text_range());
    let selection_range = component.name()
        .map(|n| text_range_to_lsp_range(text, n.text_range()))
        .unwrap_or(range);

    // TODO: Extract pin declarations from component body
    // For now, just show component without children

    #[allow(deprecated)]
    Some(DocumentSymbol {
        name,
        detail: Some("component".to_string()),
        kind: SymbolKind::CLASS,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: None,
    })
}

/// Process a pin declaration
fn process_pin_decl(pin: &PinDecl, text: &str) -> Option<DocumentSymbol> {
    let name = pin.name()?.text().to_string();
    let range = text_range_to_lsp_range(text, pin.syntax().text_range());
    let selection_range = pin.name()
        .map(|n| text_range_to_lsp_range(text, n.text_range()))
        .unwrap_or(range);

    // Get pin type and direction for detail
    let pin_type = pin.pin_type()
        .map(|t| t.text().to_string())
        .unwrap_or_else(|| "signal".to_string());

    let direction = pin.direction()
        .map(|d| d.text().to_string())
        .unwrap_or_else(|| "".to_string());

    let detail = if direction.is_empty() {
        pin_type
    } else {
        format!("{} {}", pin_type, direction)
    };

    #[allow(deprecated)]
    Some(DocumentSymbol {
        name,
        detail: Some(detail),
        kind: SymbolKind::PROPERTY,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: None,
    })
}

/// Process a flow statement (net in v2.0)
fn process_flow_stmt(flow: &V2FlowStmt, text: &str) -> Option<DocumentSymbol> {
    // Flow statements in v2.0 can have optional net names like:
    // net signal: input -> output;
    // For now, try to extract the name if present
    let name = flow.name()?.text().to_string();
    let range = text_range_to_lsp_range(text, flow.syntax().text_range());
    let selection_range = flow.name()
        .map(|n| text_range_to_lsp_range(text, n.text_range()))
        .unwrap_or(range);

    #[allow(deprecated)]
    Some(DocumentSymbol {
        name,
        detail: Some("net".to_string()),
        kind: SymbolKind::VARIABLE,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: None,
    })
}

/// Process a power declaration
fn process_power_decl(power: &PowerDecl, text: &str) -> Option<DocumentSymbol> {
    let name = power.name()?.text().to_string();
    let range = text_range_to_lsp_range(text, power.syntax().text_range());
    let selection_range = power.name()
        .map(|n| text_range_to_lsp_range(text, n.text_range()))
        .unwrap_or(range);

    // Get voltage if specified
    let detail = power.voltage()
        .map(|v| format!("power: {}", v))
        .unwrap_or_else(|| "power".to_string());

    #[allow(deprecated)]
    Some(DocumentSymbol {
        name,
        detail: Some(detail),
        kind: SymbolKind::CONSTANT,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: None,
    })
}

/// Process a ground declaration
fn process_ground_decl(ground: &GroundDecl, text: &str) -> Option<DocumentSymbol> {
    let name = ground.name()?.text().to_string();
    let range = text_range_to_lsp_range(text, ground.syntax().text_range());
    let selection_range = ground.name()
        .map(|n| text_range_to_lsp_range(text, n.text_range()))
        .unwrap_or(range);

    #[allow(deprecated)]
    Some(DocumentSymbol {
        name,
        detail: Some("ground".to_string()),
        kind: SymbolKind::CONSTANT,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: None,
    })
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
    fn test_board_symbols() {
        let text = r#"
board TestBoard {
    power VCC = 5V;
    ground GND;
}
"#;

        let symbols = provide_document_symbols(text);
        assert!(symbols.is_some());

        if let Some(DocumentSymbolResponse::Nested(syms)) = symbols {
            assert_eq!(syms.len(), 1);

            let board = &syms[0];
            assert_eq!(board.name, "TestBoard");
            assert_eq!(board.kind, SymbolKind::CLASS);
            assert!(board.children.is_some());

            let children = board.children.as_ref().unwrap();
            assert_eq!(children.len(), 2); // VCC, GND

            // Check power
            assert_eq!(children[0].name, "VCC");
            assert_eq!(children[0].kind, SymbolKind::CONSTANT);

            // Check ground
            assert_eq!(children[1].name, "GND");
            assert_eq!(children[1].kind, SymbolKind::CONSTANT);
        }
    }

    #[test]
    fn test_entity_symbols() {
        let text = r#"
entity Regulator() {
    pin IN: power in;
    pin OUT: power out;
    pin EN: signal in;
}
"#;

        let symbols = provide_document_symbols(text);
        assert!(symbols.is_some());

        if let Some(DocumentSymbolResponse::Nested(syms)) = symbols {
            assert_eq!(syms.len(), 1);

            let entity = &syms[0];
            assert_eq!(entity.name, "Regulator");
            assert_eq!(entity.kind, SymbolKind::MODULE);
            // Simplified version doesn't enumerate children
            assert!(entity.children.is_none());
        }
    }

    #[test]
    fn test_multiple_top_level_symbols() {
        let text = r#"
entity Regulator() {
    pin IN: power in;
}

entity Filter() {
    pin IN: signal in;
}

board TestBoard {
    power VCC = 5V;
}
"#;

        let symbols = provide_document_symbols(text);
        assert!(symbols.is_some());

        if let Some(DocumentSymbolResponse::Nested(syms)) = symbols {
            assert_eq!(syms.len(), 3); // 2 entities + 1 board

            assert_eq!(syms[0].name, "Regulator");
            assert_eq!(syms[0].kind, SymbolKind::MODULE);

            assert_eq!(syms[1].name, "Filter");
            assert_eq!(syms[1].kind, SymbolKind::MODULE);

            assert_eq!(syms[2].name, "TestBoard");
            assert_eq!(syms[2].kind, SymbolKind::CLASS);
        }
    }

    #[test]
    fn test_empty_file() {
        let text = "";
        let symbols = provide_document_symbols(text);
        assert!(symbols.is_none());
    }

    #[test]
    #[ignore] // TODO: Investigate v2.0 component syntax support
    fn test_component_symbols() {
        let text = r#"
component Resistor(value: resistance) {
    pin 1: signal inout;
    pin 2: signal inout;
}
"#;

        let symbols = provide_document_symbols(text);
        assert!(symbols.is_some());

        if let Some(DocumentSymbolResponse::Nested(syms)) = symbols {
            assert_eq!(syms.len(), 1);

            let component = &syms[0];
            assert_eq!(component.name, "Resistor");
            assert_eq!(component.kind, SymbolKind::CLASS);
            // Simplified version doesn't enumerate children
            assert!(component.children.is_none());
        }
    }
}
