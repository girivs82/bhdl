use super::common::*;

#[test]
fn analyze_ident_ref_in_assign_ok() {
    let text = r#"
        board MyBoard {
            nets { net a: signal; net b: signal; }
            connections { assign a = b; }
        }
    "#;
    analyze_helper(text, false);
}

#[test]
fn analyze_ident_ref_in_assign_fail() {
    let text = r#"
        board MyBoard {
            nets { net a: signal; }
            connections { assign a = b; } // b is undefined
        }
    "#;
    analyze_helper(text, true);
}

#[test]
fn analyze_ident_ref_in_param_default_fail() {
    let text = r#"
        board MyBoard {
            parameters { parameter VALUE = WIDTH; } // WIDTH is undefined
        }
    "#;
    analyze_helper(text, true);
} 