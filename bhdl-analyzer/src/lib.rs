use bhdl_parser::{parse, syntax::SyntaxKind};
use rowan::{SyntaxNode, WalkEvent};
use bhdl_parser::syntax::BhdlLanguage; // Import the language type

mod symbol_table;
use symbol_table::{Symbol, SymbolKind, SymbolTable};

// Placeholder for analysis results or diagnostics
#[derive(Debug, Default)] // Added Default derive
pub struct AnalysisResult {
    pub symbol_table: SymbolTable,
    // Add diagnostics later
}

// Helper to get the first IDENT token's text within a node
fn get_identifier_name(node: &SyntaxNode<BhdlLanguage>) -> Option<String> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| token.kind() == SyntaxKind::IDENT)
        .map(|token| token.text().to_string())
}

// Function to build the top-level symbol table
fn build_symbol_table(root_node: &SyntaxNode<BhdlLanguage>) -> SymbolTable {
    let mut table = SymbolTable::default();

    // Iterate through top-level children of SOURCE_FILE
    for element in root_node.children() {
        let (name_opt, kind_opt) = match element.kind() {
            SyntaxKind::BOARD_DEF => (get_identifier_name(&element), Some(SymbolKind::Board)),
            SyntaxKind::MODULE_DEF => (get_identifier_name(&element), Some(SymbolKind::Module)),
            SyntaxKind::COMPONENT_DEF => (get_identifier_name(&element), Some(SymbolKind::Component)),
            SyntaxKind::INTERFACE_DEF => (get_identifier_name(&element), Some(SymbolKind::Interface)),
            SyntaxKind::TYPEDEF_DEF => (get_identifier_name(&element), Some(SymbolKind::Typedef)),
            _ => (None, None), // Ignore other top-level items like imports for now
        };

        if let (Some(name), Some(kind)) = (name_opt, kind_opt) {
            if !name.is_empty() {
                table.insert(Symbol {
                    name,
                    kind,
                    // TODO: Add span
                });
            }
        }
    }
    table
}

// Main analysis entry point
pub fn analyze(text: &str) -> AnalysisResult {
    println!("Analyzing input text...");
    let parse_result = parse(text);
    let root_node = parse_result.syntax();

    println!("Parsed successfully. Root node kind: {:?}", root_node.kind());

    // Print parse errors, if any
    let errors = parse_result.errors();
    if !errors.is_empty() {
        eprintln!("Parse Errors:");
        for error in errors {
            eprintln!("- {}", error.message);
        }
        // Early return on parse errors? Or continue analysis?
        // For now, continue but return an empty symbol table perhaps.
        return AnalysisResult::default(); // Return default if parse errors exist
    }

    // Build symbol table
    let symbol_table = build_symbol_table(&root_node);
    println!("Built symbol table: {:#?}", symbol_table);

    // TODO: Continue analysis
    // - Traverse deeper into definitions
    // - Build scoped symbol tables
    // - Name resolution
    // - Type checking

    AnalysisResult {
        symbol_table,
        // Add diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_parser::syntax::SyntaxKind;

    #[test]
    fn analyze_minimal_board() {
        let input = "board Foo { }";
        let result = analyze(input);
        assert!(result.symbol_table.lookup("Foo").is_some());
        assert_eq!(result.symbol_table.lookup("Foo").unwrap().kind, SymbolKind::Board);
    }

    #[test]
    fn analyze_multiple_defs() {
        let input = r#"
            board MyBoard {}
            component MyComp {}
            interface MyIntf {}
            typedef MyType { p=1; }
            module MyMod {}
        "#;
        let result = analyze(input);
        assert!(result.symbol_table.lookup("MyBoard").is_some());
        assert_eq!(result.symbol_table.lookup("MyBoard").unwrap().kind, SymbolKind::Board);
        assert!(result.symbol_table.lookup("MyComp").is_some());
        assert_eq!(result.symbol_table.lookup("MyComp").unwrap().kind, SymbolKind::Component);
        assert!(result.symbol_table.lookup("MyIntf").is_some());
        assert_eq!(result.symbol_table.lookup("MyIntf").unwrap().kind, SymbolKind::Interface);
        assert!(result.symbol_table.lookup("MyType").is_some());
        assert_eq!(result.symbol_table.lookup("MyType").unwrap().kind, SymbolKind::Typedef);
        assert!(result.symbol_table.lookup("MyMod").is_some());
        assert_eq!(result.symbol_table.lookup("MyMod").unwrap().kind, SymbolKind::Module);
    }

    #[test]
    fn analyze_board_with_errors() {
        let input = "board Foo { junk }";
        let result = analyze(input);
        // Analyze returns default/empty symbol table on parse errors
        assert!(result.symbol_table.lookup("Foo").is_none()); 
    }
}
