use super::common::*;

#[test]
fn analyze_net_ref_index_out_of_bounds_low() {
    let text = r#"
        board MyBoard {
            nets { net bus[7:0]: signal; } // Corrected syntax
            connections { assign bus[-1] = 1b'0; }
        }
    "#;
    analyze_helper(text, true);
}

#[test]
fn analyze_net_ref_index_out_of_bounds_high() {
    let text = r#"
        board MyBoard {
            nets { net bus[7:0]: signal; } // Corrected syntax
            connections { assign bus[8] = 1b'0; }
        }
    "#;
    analyze_helper(text, true);
}

#[test]
fn analyze_net_ref_index_out_of_bounds_low_reversed() {
    let text = r#"
        board MyBoard {
            nets { net bus[0:7]: signal; } // Corrected syntax
            connections { assign bus[-1] = 1b'0; }
        }
    "#;
    analyze_helper(text, true);
}

#[test]
fn analyze_net_ref_index_out_of_bounds_high_reversed() {
    let text = r#"
        board MyBoard {
            nets { net bus[0:7]: signal; } // Corrected syntax
            connections { assign bus[8] = 1b'0; }
        }
    "#;
    analyze_helper(text, true);
} 