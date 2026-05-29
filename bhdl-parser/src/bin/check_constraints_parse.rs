//! v0.8 constraints — parser-level smoke test.
//!
//! Confirms the grammar accepts `constraints { ... }` blocks inside
//! interface bodies and tolerates the full DDR4 use case: per-signal
//! properties, pairwise relations, bundle-self `*` targets, and
//! dotted/wildcard target paths.

use bhdl_parser::parse;
use std::process::exit;

const SOURCE: &str = r#"
interface DiffPair {
    signal P: inout;
    signal N: inout;
    constraints {
        *:       differential 100ohm;
        P -> N:  length_match 1ps;
    }
}

interface DDR4ByteLane {
    signal DQ0: inout;
    signal DQ1: inout;
    signal DQ2: inout;
    signal DQ3: inout;
    signal DQ4: inout;
    signal DQ5: inout;
    signal DQ6: inout;
    signal DQ7: inout;
    interface DiffPair DQS;
    signal DM: inout;
    constraints {
        DQ0, DQ1, DQ2, DQ3, DQ4, DQ5, DQ6, DQ7: single_ended 40ohm, signal_class DATA, length_match 10ps within_group;
    }
}

interface DDR4 {
    signal A0: out;
    signal A1: out;
    signal CS: out;
    signal RAS: out;
    interface DiffPair CK;
    interface DDR4ByteLane lane0;
    interface DDR4ByteLane lane1;
    constraints {
        CK.*:               signal_class CLOCK, max_freq 1600MHz;
        A0, A1:             single_ended 50ohm, signal_class ADDR;
        CS, RAS:            single_ended 50ohm, signal_class CMD;
        CK -> lane0.DQS:    skew_max 100ps;
        CK -> lane1.DQS:    skew_max 100ps;
    }
}
"#;

fn main() {
    let r = parse(SOURCE);
    if !r.errors().is_empty() {
        eprintln!("✗ parse failed:");
        for e in r.errors().iter().take(20) {
            eprintln!("    {:?}", e);
        }
        exit(1);
    }
    println!("✓ DDR4-style constraints blocks parse cleanly");
    println!("  - DiffPair: bundle-self `*` target + pairwise `P -> N` skew");
    println!("  - DDR4ByteLane: multi-signal target list, multi-property RHS");
    println!("  - DDR4: dotted `CK.*` + cross-bundle `CK -> laneN.DQS` skew");
    println!("\nv0.8 constraints parser: PASS");
}
