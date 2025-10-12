///! Test decoupling block parsing specifically

use bhdl_parser;

fn main() {
    println!("Testing decoupling block parsing...\n");

    let code = r#"
board TestBoard {
    power_domain @VCC = 5V @ 1A {
        sources {
            reg: LM7805().OUT;
        }

        decoupling {
            near reg: 10uF @ 1;
        }
    }
}
"#;

    println!("Parsing code with decoupling block...");
    let parsed = bhdl_parser::parse(code);

    if parsed.errors().is_empty() {
        println!("✅ No parse errors");
    } else {
        println!("❌ Parse errors:");
        for error in parsed.errors() {
            println!("  - {}", error.message);
        }
    }

    // Check for DECOUPLING_BLOCK in syntax tree
    let syntax = parsed.syntax();
    let mut found_decoupling = false;
    for child in syntax.descendants() {
        if child.kind() == bhdl_parser::SyntaxKind::DECOUPLING_BLOCK {
            found_decoupling = true;
            println!("\n✅ Found DECOUPLING_BLOCK in syntax tree");
            println!("Decoupling block text: {}", child.text());
            break;
        }
    }

    if !found_decoupling {
        println!("\n❌ DECOUPLING_BLOCK not found in syntax tree");
    }
}
