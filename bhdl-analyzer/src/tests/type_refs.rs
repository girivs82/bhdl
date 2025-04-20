use super::common::*;

#[test]
fn analyze_defined_type_ref() {
    let text = r#"
        typedef MyCustomSig { width = 4; }
        board MyBoard {
            nets { net data: MyCustomSig; }
        }
    "#;
    analyze_helper(text, false);
}

#[test]
fn analyze_undefined_type_ref() {
    let text = r#"
        board MyBoard {
            nets { net data: UnknownType; }
        }
    "#;
    analyze_helper(text, true); // Expect error
    // Optionally check the specific error message if analyze_helper is adapted
}

#[test]
fn analyze_non_type_as_type_ref() {
    let text = r#"
        module NotAType {}
        board MyBoard {
            nets { net data: NotAType; }
        }
    "#;
    analyze_helper(text, true); // Expect error
} 