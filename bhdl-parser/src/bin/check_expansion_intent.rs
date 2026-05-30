//! Parser regression guard: `for INTENT(...)` layout-intent clause on a
//! component-declaration inside an `expansion { }` block (P&R handshake
//! step 4, parser half). Covers the named-param form with an `mm2` area
//! unit literal — the canonical ASCII area syntax (handshake §8.3).

use bhdl_parser::{parse, SyntaxKind};
use rowan::ast::AstNode;

const SRC: &str = r#"
entity Widget {
    pin VCC: signal inout virtual;
    pin GND1: signal inout;
    expansion {
        C_vcc: Cap(100nF) for high_freq_bypass(rail: VCC, return: GND1, loop_area_max: 1.5mm2);
        VCC -> C_vcc.1; C_vcc.2 -> GND1;
    }
}
"#;

fn main() {
    let pr = parse(SRC);
    if !pr.errors().is_empty() {
        eprintln!("✗ parse errors:");
        for e in pr.errors().iter().take(10) {
            eprintln!("    {}", e.message);
        }
        std::process::exit(1);
    }
    let mut clauses = 0;
    let mut calls = 0;
    for n in pr.syntax().descendants() {
        match n.kind() {
            SyntaxKind::INTENT_CLAUSE => clauses += 1,
            SyntaxKind::INTENT_CALL => calls += 1,
            _ => {}
        }
    }
    if clauses != 1 || calls != 1 {
        eprintln!("✗ expected 1 INTENT_CLAUSE + 1 INTENT_CALL, got {} / {}", clauses, calls);
        std::process::exit(1);
    }
    println!("✓ expansion-block `for INTENT(...)` clause parses (named params + mm2 area unit)");
    println!("parser intent-clause guard: PASS");
}
