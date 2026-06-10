//! Workspace Symbols support - search for symbols across the entire project

use tower_lsp::lsp_types::*;
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;

/// Provide workspace-wide symbol search
pub fn provide_workspace_symbols(
    query: &str,
    documents: &[(Url, String)], // List of (uri, text) for all open documents
) -> Option<Vec<SymbolInformation>> {
    let mut symbols = Vec::new();

    // Search through all documents
    for (uri, text) in documents {
        if let Some(doc_symbols) = extract_symbols_from_document(text, uri) {
            // Filter by query (case-insensitive fuzzy match)
            for symbol in doc_symbols {
                if matches_query(&symbol.name, query) {
                    symbols.push(symbol);
                }
            }
        }
    }

    if symbols.is_empty() {
        None
    } else {
        // Sort by name for consistent ordering
        symbols.sort_by(|a, b| a.name.cmp(&b.name));
        Some(symbols)
    }
}

/// Extract all symbols from a document
fn extract_symbols_from_document(text: &str, uri: &Url) -> Option<Vec<SymbolInformation>> {
    let parse_result = parse(text);
    let source_file = SourceFile::cast(parse_result.syntax())?;
    let analysis_result = analyze(&source_file);

    let mut symbols = Vec::new();

    // Extract from global scope
    for symbol in analysis_result.global_scope.iter() {
        let symbol_info = create_symbol_information(symbol, uri)?;
        symbols.push(symbol_info);
    }

    // Extract from definition scopes
    for (_node_ptr, scope) in &analysis_result.definition_scopes {
        for symbol in scope.iter() {
            let symbol_info = create_symbol_information(symbol, uri)?;
            symbols.push(symbol_info);
        }
    }

    if symbols.is_empty() {
        None
    } else {
        Some(symbols)
    }
}

/// Create LSP SymbolInformation from analyzer Symbol
#[allow(deprecated)]
fn create_symbol_information(
    symbol: &bhdl_analyzer::symbol_table::Symbol,
    uri: &Url,
) -> Option<SymbolInformation> {
    use bhdl_analyzer::symbol_table::SymbolKind as ASymbolKind;

    let kind = match symbol.kind {
        ASymbolKind::Board => SymbolKind::CLASS,
        ASymbolKind::Entity => SymbolKind::MODULE,
        ASymbolKind::Component => SymbolKind::STRUCT,
        ASymbolKind::PartFamily => SymbolKind::CLASS, // v0.2 catalog family (groups part candidates for an entity)
        ASymbolKind::Interface => SymbolKind::INTERFACE,
        ASymbolKind::Net => SymbolKind::VARIABLE,
        ASymbolKind::Pin => SymbolKind::FIELD,
        ASymbolKind::VirtualPin => SymbolKind::FIELD,
        ASymbolKind::Parameter => SymbolKind::CONSTANT,
        ASymbolKind::Typedef => SymbolKind::TYPE_PARAMETER,
        ASymbolKind::Instance => SymbolKind::OBJECT,
        ASymbolKind::Enum => SymbolKind::ENUM,
        ASymbolKind::EnumVariant => SymbolKind::ENUM_MEMBER,
        ASymbolKind::Trait => SymbolKind::INTERFACE,
    };

    // Convert TextRange to LSP Range
    let range = text_range_to_lsp_range(&symbol.span);

    Some(SymbolInformation {
        name: symbol.name.clone(),
        kind,
        tags: None,
        deprecated: None,
        location: Location {
            uri: uri.clone(),
            range,
        },
        container_name: None, // Could add parent scope name here
    })
}

/// Convert rowan TextRange to LSP Range
fn text_range_to_lsp_range(text_range: &rowan::TextRange) -> Range {
    // Simple conversion - assumes single line for now
    // In production, would need proper line/column tracking
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

/// Check if symbol name matches query (fuzzy, case-insensitive)
fn matches_query(name: &str, query: &str) -> bool {
    if query.is_empty() {
        return true; // Empty query matches everything
    }

    let name_lower = name.to_lowercase();
    let query_lower = query.to_lowercase();

    // Simple fuzzy matching: check if all characters in query appear in order
    let mut query_chars = query_lower.chars().peekable();
    let mut name_chars = name_lower.chars();

    while let Some(&query_char) = query_chars.peek() {
        match name_chars.find(|&c| c == query_char) {
            Some(_) => {
                query_chars.next();
            }
            None => return false,
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_query() {
        // Exact match
        assert!(matches_query("TestBoard", "TestBoard"));

        // Case insensitive
        assert!(matches_query("TestBoard", "testboard"));
        assert!(matches_query("TestBoard", "TESTBOARD"));

        // Fuzzy match - characters in order
        assert!(matches_query("TestBoard", "TB"));
        assert!(matches_query("TestBoard", "TstBrd"));
        assert!(matches_query("TestBoard", "test"));
        assert!(matches_query("TestBoard", "Board"));

        // No match - characters not in order
        assert!(!matches_query("TestBoard", "BT"));

        // No match - character not present
        assert!(!matches_query("TestBoard", "TestBoardX"));
        assert!(!matches_query("TestBoard", "xyz"));

        // Empty query matches everything
        assert!(matches_query("TestBoard", ""));
        assert!(matches_query("", ""));
    }

    #[test]
    fn test_workspace_symbols_empty() {
        let documents = vec![];
        let result = provide_workspace_symbols("test", &documents);
        assert!(result.is_none());
    }

    #[test]
    fn test_workspace_symbols_single_document() {
        let text = r#"
board TestBoard {
    power VCC = 5V;
}

entity Regulator() {
    pin IN: power in;
}
"#;

        let uri = Url::parse("file:///test.bhdl").unwrap();
        let documents = vec![(uri.clone(), text.to_string())];

        let result = provide_workspace_symbols("", &documents);
        assert!(result.is_some());

        let symbols = result.unwrap();
        assert!(!symbols.is_empty());

        // Should find TestBoard and Regulator
        let names: Vec<String> = symbols.iter().map(|s| s.name.clone()).collect();
        assert!(names.iter().any(|n| n == "TestBoard"));
        assert!(names.iter().any(|n| n == "Regulator"));
    }

    #[test]
    fn test_workspace_symbols_fuzzy_match() {
        let text = r#"
board TestBoard {}
board ProductionBoard {}
entity Regulator() {}
"#;

        let uri = Url::parse("file:///test.bhdl").unwrap();
        let documents = vec![(uri.clone(), text.to_string())];

        // Query for "Board" should match both boards
        let result = provide_workspace_symbols("Board", &documents);
        assert!(result.is_some());

        let symbols = result.unwrap();
        let names: Vec<String> = symbols.iter().map(|s| s.name.clone()).collect();

        assert!(names.iter().any(|n| n == "TestBoard"));
        assert!(names.iter().any(|n| n == "ProductionBoard"));
        assert!(!names.iter().any(|n| n == "Regulator")); // Should not match
    }

    #[test]
    fn test_workspace_symbols_multiple_documents() {
        let doc1 = "board Board1 {}";
        let doc2 = "entity Entity2() {}";

        let uri1 = Url::parse("file:///file1.bhdl").unwrap();
        let uri2 = Url::parse("file:///file2.bhdl").unwrap();

        let documents = vec![
            (uri1.clone(), doc1.to_string()),
            (uri2.clone(), doc2.to_string()),
        ];

        let result = provide_workspace_symbols("", &documents);
        assert!(result.is_some());

        let symbols = result.unwrap();
        assert!(symbols.len() >= 2);

        // Should find symbols from both files
        assert!(symbols.iter().any(|s| s.name == "Board1"));
        assert!(symbols.iter().any(|s| s.name == "Entity2"));

        // Check URIs are correct
        let board1_symbol = symbols.iter().find(|s| s.name == "Board1").unwrap();
        assert_eq!(board1_symbol.location.uri, uri1);

        let entity2_symbol = symbols.iter().find(|s| s.name == "Entity2").unwrap();
        assert_eq!(entity2_symbol.location.uri, uri2);
    }
}
