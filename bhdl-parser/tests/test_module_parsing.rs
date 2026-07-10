use bhdl_parser;

#[test]
fn test_entity_definition_parsing() {
    // Current v2 entity grammar (matches bhdl-stdlib/passives/capacitor.bhdl):
    // per-pin `pin` declarations and `attribute` statements.
    let entity_code = r#"
entity Cap(value: capacitance) {
    pin 1: signal inout;
    pin 2: signal inout;

    attribute component_class = "capacitor";
    attribute capacitance = value;
}
"#;

    let parsed = bhdl_parser::parse(entity_code);
    assert!(parsed.errors().is_empty(), "Parse errors: {:?}", parsed.errors());

    let syntax = parsed.syntax();
    println!("Root kind: {:?}", syntax.kind());

    // Look for ENTITY_DEF
    let mut found_entity = false;
    for child in syntax.children() {
        if child.kind() == bhdl_parser::SyntaxKind::ENTITY_DEF {
            found_entity = true;
            println!("Found ENTITY_DEF!");

            // Find the entity name
            for gc in child.children_with_tokens() {
                if let rowan::NodeOrToken::Token(t) = gc {
                    if t.kind() == bhdl_parser::SyntaxKind::IDENT {
                        println!("Entity name: {}", t.text());
                        assert_eq!(t.text(), "Cap");
                        break;
                    }
                }
            }
        }
    }

    assert!(found_entity, "ENTITY_DEF not found in parsed syntax tree");
}
