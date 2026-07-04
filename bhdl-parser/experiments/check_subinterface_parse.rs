//! v0.8 hierarchical sub-interfaces — parser-level smoke test.
//!
//! Verifies the grammar accepts `interface SubName field;` inside
//! an interface body, so a real-world hierarchical interface like
//! RGMII can be declared:
//!
//!     interface UartChannel { perspective dte {...} ... }
//!     interface DualUART {
//!         interface UartChannel ch0;
//!         interface UartChannel ch1;
//!     }
//!
//! The synthesizer-side recursion (materialising
//! `duart.ch0.TX` / `duart.ch1.TX` etc. on instances) is a
//! follow-up (task #85 stage 2); this test only verifies the
//! grammar.

use bhdl_parser::parse;
use std::process::exit;

const SOURCE: &str = r#"
interface UartChannel {
    perspective dte { signal TX: out; signal RX: in; }
    perspective dce { signal TX: out; signal RX: in; }
    wires { dte.TX <-> dce.RX; dte.RX <-> dce.TX; }
}

interface DualUART {
    interface UartChannel ch0;
    interface UartChannel ch1;
}

// More realistic: RGMII has tx + rx sub-bundles each carrying
// clock, control, and data lines. Without array signals (a
// future addition) we expand the data lines manually as D0..D3.
interface MIIChannelTx {
    perspective phy {
        signal CLK:  out;
        signal CTL:  out;
        signal D0:   out;
        signal D1:   out;
        signal D2:   out;
        signal D3:   out;
    }
    perspective mac {
        signal CLK:  in;
        signal CTL:  in;
        signal D0:   in;
        signal D1:   in;
        signal D2:   in;
        signal D3:   in;
    }
}
interface MIIChannelRx {
    perspective phy {
        signal CLK:  in;
        signal CTL:  in;
        signal D0:   in;
        signal D1:   in;
        signal D2:   in;
        signal D3:   in;
    }
    perspective mac {
        signal CLK:  out;
        signal CTL:  out;
        signal D0:   out;
        signal D1:   out;
        signal D2:   out;
        signal D3:   out;
    }
}
interface RGMII {
    interface MIIChannelTx tx;
    interface MIIChannelRx rx;
}
"#;

fn main() {
    let r = parse(SOURCE);
    if !r.errors().is_empty() {
        eprintln!("✗ parse failed:");
        for e in r.errors().iter().take(10) {
            eprintln!("    {:?}", e);
        }
        exit(1);
    }
    println!("✓ hierarchical interfaces parse cleanly");
    println!("  - DualUART (2 sub-fields of UartChannel)");
    println!("  - RGMII (tx: MIIChannelTx, rx: MIIChannelRx)");
    println!("\nv0.8 sub-interface parser: PASS");
}
