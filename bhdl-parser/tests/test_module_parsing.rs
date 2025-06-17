use bhdl_parser;

#[test]
fn test_module_definition_parsing() {
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
    assert!(parsed.errors().is_empty(), "Parse errors: {:?}", parsed.errors());
    
    let syntax = parsed.syntax();
    println!("Root kind: {:?}", syntax.kind());
    
    // Look for MODULE_DEF
    let mut found_module = false;
    for child in syntax.children() {
        if child.kind() == bhdl_parser::SyntaxKind::MODULE_DEF {
            found_module = true;
            println!("Found MODULE_DEF!");
            
            // Find the module name
            for gc in child.children_with_tokens() {
                if let rowan::NodeOrToken::Token(t) = gc {
                    if t.kind() == bhdl_parser::SyntaxKind::IDENT {
                        println!("Module name: {}", t.text());
                        assert_eq!(t.text(), "Cap");
                        break;
                    }
                }
            }
        }
    }
    
    assert!(found_module, "MODULE_DEF not found in parsed syntax tree");
}