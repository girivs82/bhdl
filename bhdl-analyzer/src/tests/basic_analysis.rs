use super::common::*;
use crate::analyze;
use crate::symbol_table::SymbolKind;

#[test]
fn analyze_minimal_board() {
    let text = "board MyBoard {}";
    analyze_helper(text, false);
}

#[test]
fn analyze_multiple_defs() {
    let text = r#"
        board Board1 {}
        module ModA {
            parameters { parameter P1 = 5; }
        }
        component CompX {
            pins { pin p1: signal; }
        }
        interface IfaceY {
            pins { pin p2: signal; }
        }
        typedef MySig { width = 8; }
    "#;
    analyze_helper(text, false);
}

#[test]
fn analyze_nested_scopes() {
    let text = r#"
        board OuterBoard {
            parameters { parameter P_OUT = 10; }
            nets { net n_out: signal; }
            components { component InnerMod M1 { P_INNER = P_OUT * 2; }; }
        }
        module InnerMod {
            parameters { parameter P_INNER: integer; }
            ports { port p_in: SigIntf; }
            nets { net n_in: signal; }
            connections { connect p_in -> n_in; }
        }
        interface SigIntf { pins { pin p: signal; } }
    "#;
    analyze_helper(text, true);
} 