use super::common::*;

// --- Assignment Type/Width Checks ---

#[test]
fn analyze_assign_type_mismatch_base() {
    let text = r#"
        typedef TypeA {}
        typedef TypeB {}
        board MyBoard {
            nets { net a: TypeA; net b: TypeB; }
            connections { assign a = b; }
        }
    "#;
    analyze_helper(text, true);
}

#[test]
fn analyze_assign_width_mismatch_bus_scalar() {
    let text = r#"
        board MyBoard {
            nets {
                net bus[3:0]: signal;
                net scalar: signal;
            }
            connections { assign bus = scalar; }
        }
    "#;
    analyze_helper(text, true);
}

#[test]
fn analyze_assign_width_mismatch_scalar_bus() {
    let text = r#"
        board MyBoard {
            nets {
                net scalar: signal;
                net bus[3:0]: signal;
            }
            connections { assign scalar = bus; }
        }
    "#;
    analyze_helper(text, true);
}

#[test]
fn analyze_assign_width_mismatch_bus_bus() {
    let text = r#"
        board MyBoard {
            nets { net bus4[3:0]: signal; net bus8[7:0]: signal; }
            connections { assign bus4 = bus8; }
        }
    "#;
    analyze_helper(text, true);
}

#[test]
fn analyze_assign_compatible_scalar() {
    let text = r#"
        board MyBoard {
            nets { net a: signal; net b: signal; }
            connections { assign a = b; }
        }
    "#;
    analyze_helper(text, false);
}

#[test]
fn analyze_assign_compatible_bus() {
    let text = r#"
        board MyBoard {
            nets {
                net bus_a[7:0]: signal;
                net bus_b[7:0]: signal;
            }
            connections { assign bus_a = bus_b; }
        }
    "#;
    analyze_helper(text, false);
}

// --- Connection Type/Width Checks ---

#[test]
fn analyze_conn_type_mismatch_base() {
    let text = r#"
        typedef TypeA {}
        typedef TypeB {}
        board MyBoard {
            nets { net a: TypeA; net b: TypeB; }
            connections { connect a -> b; }
        }
    "#;
    analyze_helper(text, true);
}

#[test]
fn analyze_conn_width_mismatch_bus_scalar() {
    let text = r#"
        board MyBoard {
            nets { net bus[3:0]: signal; net scalar: signal; }
            connections { connect bus -> scalar; }
        }
    "#;
    analyze_helper(text, true);
}

#[test]
fn analyze_conn_width_mismatch_scalar_bus() {
    let text = r#"
        board MyBoard {
            nets { net scalar: signal; net bus[3:0]: signal; }
            connections { connect scalar -> bus; }
        }
    "#;
    analyze_helper(text, true);
}

#[test]
fn analyze_conn_width_mismatch_bus_bus() {
    let text = r#"
        board MyBoard {
            nets { net bus4[3:0]: signal; net bus8[7:0]: signal; }
            connections { connect bus4 -> bus8; }
        }
    "#;
    analyze_helper(text, true);
}

#[test]
fn analyze_conn_compatible_scalar() {
    let text = r#"
        board MyBoard {
            nets { net a: signal; net b: signal; }
            connections { connect a -> b; }
        }
    "#;
    analyze_helper(text, false);
}

#[test]
fn analyze_conn_compatible_bus() {
    let text = r#"
        board MyBoard {
            nets {
                net bus_a[7:0]: signal;
                net bus_b[7:0]: signal;
            }
            connections { connect bus_a -> bus_b; }
        }
    "#;
    analyze_helper(text, false);
}

// --- Connection with PinRef Type/Width Checks ---

#[test]
fn analyze_conn_with_pin_ref() {
    let text = r#"
        component CompA { pins { pin p_in: signal; pin p_out: signal; } }
        board MyBoard {
            nets { net n_int: signal; }
            components { component CompA U1; }
            connections { connect n_int -> U1.p_in; connect U1.p_out -> n_int; }
        }
    "#;
    analyze_helper(text, false);
}

#[test]
fn analyze_conn_pinref_type_mismatch() {
    let text = r#"
        typedef TypeA {}
        typedef TypeB {}
        component CompA { pins { pin p: TypeA; } }
        board MyBoard {
            nets { net n: TypeB; }
            components { component CompA U1; }
            connections { connect n -> U1.p; }
        }
    "#;
    analyze_helper(text, true);
}

#[test]
fn analyze_conn_pinref_width_mismatch_bus_scalar() {
    let text = r#"
        component CompA { pins { pin p_bus[3:0]: signal; } }
        board MyBoard {
            nets { net n_scalar: signal; }
            components { component CompA U1; }
            connections { connect n_scalar -> U1.p_bus; }
        }
    "#;
    analyze_helper(text, true);
}

#[test]
fn analyze_conn_pinref_width_mismatch_bus_bus() {
    let text = r#"
        component CompA { pins { pin p_bus4[3:0]: signal; } }
        board MyBoard {
            nets { net n_bus8[7:0]: signal; }
            components { component CompA U1; }
            connections { connect n_bus8 -> U1.p_bus4; }
        }
    "#;
    analyze_helper(text, true);
}

#[test]
fn analyze_conn_pinref_compatible_bus() {
    let text = r#"
        component CompA { pins { pin p_bus8[7:0]: signal; } }
        board MyBoard {
            nets { net n_bus8[7:0]: signal; }
            components { component CompA U1; }
            connections { connect n_bus8 -> U1.p_bus8; }
        }
    "#;
    analyze_helper(text, false);
} 