use super::common::*;

#[test]
fn analyze_binary_expr_add_scalars_ok() {
    let text = r#"
        board MyBoard {
            nets { net a: signal; net b: signal; net c: signal; }
            connections { assign c = a + b; }
        }
    "#;
    analyze_helper(text, false);
}

#[test]
fn analyze_binary_expr_and_buses_ok() {
    let text = r#"
        board MyBoard {
            nets { net bus_a[7:0]: signal; net bus_b[7:0]: signal; net bus_c[7:0]: signal; } // Corrected syntax
            connections { assign bus_c = bus_a & bus_b; }
        }
    "#;
    analyze_helper(text, false);
}

#[test]
fn analyze_binary_expr_add_width_mismatch_in_expr() {
    let text = r#"
        board MyBoard {
            nets {
                net scalar: signal;
                net bus[3:0]: signal; // Corrected syntax
                net result: signal;
            }
            connections { assign result = scalar + bus; }
        }
    "#;
    analyze_helper(text, true);
}

#[test]
fn analyze_binary_expr_and_width_mismatch_in_expr() {
    let text = r#"
        board MyBoard {
            nets {
                net bus4[3:0]: signal; // Corrected syntax
                net bus8[7:0]: signal; // Corrected syntax
                net result[7:0]: signal; // Corrected syntax
            }
            connections { assign result = bus4 & bus8; }
        }
    "#;
    analyze_helper(text, true);
}

#[test]
fn analyze_binary_expr_ok_assign_width_mismatch() {
    let text = r#"
        board MyBoard {
            nets { net a: signal; net b: signal; net result_bus[3:0]: signal; } // Corrected syntax
            connections { assign result_bus = a + b; }
        }
    "#;
    analyze_helper(text, true);
} 