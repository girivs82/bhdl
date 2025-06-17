use bhdl_parser;

fn main() {
    let module_code = r#"
module Cap(value: capacitance) {
    pins {
        +: passive;
        -: passive;
    }
    
    @component_class = "capacitor";
}
"#;

    let parsed = bhdl_parser::parse(module_code);
    println!("Parse errors: {:?}", parsed.errors());
    
    let syntax = parsed.syntax();
    println!("Root kind: {:?}", syntax.kind());
    
    // Print all children
    for child in syntax.children() {
        println!("Child: {:?} - {:?}", child.kind(), child.text());
    }
    
    // Look for MODULE_DEF
    for child in syntax.children() {
        if child.kind() == bhdl_parser::SyntaxKind::MODULE_DEF {
            println!("\nFound MODULE_DEF!");
            for gc in child.children_with_tokens() {
                match gc {
                    rowan::NodeOrToken::Node(n) => {
                        println!("  Node: {:?}", n.kind());
                    }
                    rowan::NodeOrToken::Token(t) => {
                        println!("  Token: {:?} = '{}'", t.kind(), t.text());
                    }
                }
            }
        }
    }
}