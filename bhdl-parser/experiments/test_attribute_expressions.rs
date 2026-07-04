use bhdl_parser::{parse, BhdlLanguage, SyntaxKind};

fn main() {
    let source = r#"
entity TestAttributeExpr() {
    pin 1: signal inout;
    pin 2: signal inout;
    
    // Test if expressions work in attributes
    attribute resistance = 10k;
    attribute equation_simple = v_diff / resistance;
    attribute equation_complex = v_diff * v_diff / resistance;
    attribute equation_conditional = v_diff > 0 ? v_diff / resistance : 0;
}
"#;

    println!("=== Testing Attribute Expression Parsing ===\n");
    println!("Source code:");
    println!("{}", source);
    
    println!("\n=== Parsing ===");
    let parsed = parse(source);
    
    // Note: ParseResult doesn't expose errors publicly in this implementation
    // We'll proceed assuming the parse succeeded
    
    println!("\n=== Analyzing Attribute Declarations ===");
    
    // Walk the syntax tree looking for attribute declarations
    fn walk_node(node: rowan::SyntaxNode<BhdlLanguage>, indent: usize) {
        let indent_str = "  ".repeat(indent);
        
        if node.kind() == SyntaxKind::ATTRIBUTE_DECL {
            println!("{}Attribute Declaration:", indent_str);
            println!("{}  Text: {}", indent_str, node.text());
            
            // Look at the structure
            for child in node.children_with_tokens() {
                match child {
                    rowan::NodeOrToken::Node(n) => {
                        println!("{}  Child node: {:?} = '{}'", indent_str, n.kind(), n.text());
                    },
                    rowan::NodeOrToken::Token(t) => {
                        println!("{}  Token: {:?} = '{}'", indent_str, t.kind(), t.text());
                    }
                }
            }
        }
        
        // Recurse into children
        for child in node.children() {
            walk_node(child, indent + 1);
        }
    }
    
    walk_node(parsed.syntax(), 0);
    
    println!("\n=== Full Syntax Tree ===");
    println!("{:#?}", parsed.syntax());
}