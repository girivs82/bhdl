use bhdl_parser::{parse, SyntaxKind};

fn main() {
    // Test simple via clause
    let via_test = r#"
board TestBoard {
    voltage_monitor: VoltageMonitor();
    
    satisfies {
        REQ_001: via voltage_monitor;
    }
}
"#;

    println!("Testing 'via' clause...");
    let result = parse(via_test);
    
    if result.errors().is_empty() {
        println!("✓ Via clause parsed successfully!");
        print_satisfies_nodes(&result.syntax(), 0);
    } else {
        println!("✗ Parse errors:");
        for error in result.errors() {
            println!("  {:?}", error);
        }
    }
}

fn print_satisfies_nodes(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, depth: usize) {
    let indent = "  ".repeat(depth);
    let kind: SyntaxKind = node.kind().into();
    
    if matches!(kind, 
        SyntaxKind::SATISFIES_BLOCK | 
        SyntaxKind::SATISFIES_ITEM | 
        SyntaxKind::SATISFIES_VIA |
        SyntaxKind::IDENT |
        SyntaxKind::VIA_KW
    ) {
        println!("{}{:?}: \"{}\"", indent, kind, node.text());
    }
    
    for child in node.children() {
        print_satisfies_nodes(&child, depth + 1);
    }
}