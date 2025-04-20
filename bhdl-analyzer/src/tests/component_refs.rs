use super::common::*;

#[test]
fn analyze_defined_component_type() {
    let text = r#"
        module MyModule {}
        board MyBoard {
            components { component MyModule U1; } // Corrected syntax
        }
    "#;
    analyze_helper(text, false);
}

#[test]
fn analyze_undefined_component_type() {
    let text = r#"
        board MyBoard {
            components { component UnknownModule U1; } // Corrected syntax
        }
    "#;
    analyze_helper(text, true);
}

#[test]
fn analyze_non_component_as_component_type() {
    let text = r#"
        typedef NotAComponent {}
        board MyBoard {
            components { component NotAComponent U1; } // Corrected syntax
        }
    "#;
    analyze_helper(text, true);
} 