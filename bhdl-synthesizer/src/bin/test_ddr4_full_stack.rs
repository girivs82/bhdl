//! v0.8 DDR4 full-stack integration test.
//!
//! Exercises every interface feature together for a realistic DDR4
//! memory controller:
//!   - #85 hierarchical sub-interfaces (DiffPair inside DDR4ByteLane)
//!   - #84 parametric interfaces + tier-2 generate loops
//!         (`DDR4<byte_lanes=N>` unrolling laneK fields)
//!   - #86 interface constraints (DiffPair / DDR4ByteLane / DDR4)
//!
//! Verifies that the chosen `byte_lanes=4` SKU produces:
//!   - leaf pins for CK.{P,N}, A0..A1, and lane0..3 each with
//!     DQS.{P,N}, DM
//!   - constraint attributes propagated through every level
//!     (differential 100Ω on every DQS pair, single-ended 40Ω on
//!      every DQ-class pin, etc.)
//!   - cross-bundle relations to each laneN.DQS pair

use bhdl_ast::{AstNode, SourceFile};
use bhdl_parser::parse;
use bhdl_synthesizer::parametric_resolver;
use bhdl_synthesizer::hierarchical_connectivity::{
    INTERFACE_CONSTRAINT_ATTR_PREFIX, INTERFACE_CONSTRAINT_REL_ATTR_PREFIX,
};

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
    signal DM:  inout;
    interface DiffPair DQS;
    constraints {
        DQ0, DQ1, DQ2, DQ3, DQ4, DQ5, DQ6, DQ7: single_ended 40ohm, signal_class DATA;
        DM: single_ended 40ohm, signal_class DM;
        // Bit swizzle freedom: within a byte lane the strobe latches
        // all data lines together, so the router may permute DQ0..DQ7
        // + DM however it likes. No new BHDL machinery needed — the
        // property name `swizzle_within_byte` (distinct from
        // `swizzle_across_bytes` declared at the DDR4 outer level)
        // flows to every materialised DQ leaf as its own attribute,
        // so both freedoms coexist on the same pin without a
        // last-write-wins collision.
        DQ0, DQ1, DQ2, DQ3, DQ4, DQ5, DQ6, DQ7, DM: swizzle_within_byte true;
    }
}

interface DDR4<byte_lanes: int = 8> {
    signal A0: out;
    signal A1: out;
    signal CS: out;
    interface DiffPair CK;
    generate for i in 0..<byte_lanes> {
        interface DDR4ByteLane lane<i>;
    }
    constraints {
        CK.*:               signal_class CLOCK, max_freq 1600MHz;
        A0, A1, CS:         single_ended 50ohm, signal_class ADDR;
        // Byte swizzle freedom: byte lanes train independently, so
        // the router may reorder lane0..laneN-1 as a whole. Wildcard
        // `lane*` matches every leaf under any laneK. Distinct
        // property name from `swizzle_within_byte` lets both
        // freedoms coexist on the same pin.
        lane*:              swizzle_across_bytes true;
    }
}

entity MemController {
    interface DDR4<byte_lanes=4> ddr;
}

board TestBoard {
    power VCC = 1.2V @ 1A;
    ground GND;
    mc: MemController();
}
"#;

fn fail(msg: &str) -> ! {
    eprintln!("✗ {}", msg);
    std::process::exit(1);
}

fn main() {
    // Run the parametric resolver (handles `<param>` substitution and
    // `generate for` unrolling), then parse + extract connectivity.
    let rewritten = match parametric_resolver::preprocess(SOURCE) {
        Ok(s) => s,
        Err(e) => fail(&format!("preprocess: {}", e)),
    };
    let pr = parse(&rewritten);
    if !pr.errors().is_empty() {
        eprintln!("parse errors:");
        for e in pr.errors().iter().take(20) { eprintln!("  {:?}", e); }
        std::process::exit(1);
    }
    let sf = SourceFile::cast(pr.syntax()).expect("source file");
    let analysis = bhdl_analyzer::analyze(&sf);
    let mut netlist = bhdl_netlist::Netlist::new();
    bhdl_synthesizer::hierarchical_connectivity::extract_hierarchical_connectivity(
        &sf, &analysis, &mut netlist, None,
    ).expect("synthesis succeeded");

    let mc_module_id = netlist
        .instances
        .iter()
        .find(|(_, i)| i.name == "mc")
        .map(|(_, i)| i.definition)
        .unwrap_or_else(|| fail("mc instance not found"));
    let module = netlist.modules.get(mc_module_id).expect("mc module");

    // Pin set.
    let mut pin_names: Vec<String> = module
        .pins
        .iter()
        .filter_map(|pid| netlist.pins.get(*pid).map(|p| p.name.clone()))
        .collect();
    pin_names.sort();
    println!("Materialised pins on mc ({}):", pin_names.len());
    for p in &pin_names { println!("  - {}", p); }

    // Expected leaf pins: CK.{P,N}, A0, A1, CS, and per-lane (0..3)
    // DQ0..DQ7 + DM + DQS.{P,N} = (8 + 1 + 2) × 4 = 44 lane pins.
    // Total = 3 (addr/cmd) + 2 (CK pair) + 44 = 49.
    let expected = [
        "ddr.A0", "ddr.A1", "ddr.CS",
        "ddr.CK.P", "ddr.CK.N",
        "ddr.lane0.DQ0", "ddr.lane0.DQ7", "ddr.lane0.DM",
        "ddr.lane0.DQS.P", "ddr.lane0.DQS.N",
        "ddr.lane3.DQ0", "ddr.lane3.DQ7", "ddr.lane3.DM",
        "ddr.lane3.DQS.P", "ddr.lane3.DQS.N",
    ];
    for e in expected.iter() {
        if !pin_names.iter().any(|n| n == e) {
            fail(&format!("missing expected pin `{}`", e));
        }
    }
    // No lane4 leak.
    if pin_names.iter().any(|n| n.starts_with("ddr.lane4.")) {
        fail("`ddr.lane4.*` leaked — generate loop over-unrolled");
    }
    println!("✓ all 4 byte-lanes materialised (lane0..lane3, no lane4)");

    // Constraint attribute checks.
    let need_attr = |k: &str, v: &str| {
        match module.attributes.get(k) {
            Some(actual) if actual == v => println!("✓ {} = {}", k, v),
            Some(actual) => fail(&format!("expected `{}` = `{}`, got `{}`", k, v, actual)),
            None => fail(&format!("missing attribute `{}` (expected `{}`)", k, v)),
        }
    };

    // DiffPair's `*: differential 100ohm` fires for CK + every laneN.DQS.
    for lane in 0..4 {
        for leaf in &["P", "N"] {
            need_attr(
                &format!("{}ddr.lane{}.DQS.{}__differential", INTERFACE_CONSTRAINT_ATTR_PREFIX, lane, leaf),
                "100ohm",
            );
        }
    }
    need_attr(&format!("{}ddr.CK.P__differential", INTERFACE_CONSTRAINT_ATTR_PREFIX), "100ohm");
    need_attr(&format!("{}ddr.CK.N__differential", INTERFACE_CONSTRAINT_ATTR_PREFIX), "100ohm");

    // DDR4ByteLane's per-DQ properties for every lane.
    for lane in 0..4 {
        for dq in 0..8 {
            need_attr(
                &format!("{}ddr.lane{}.DQ{}__single_ended", INTERFACE_CONSTRAINT_ATTR_PREFIX, lane, dq),
                "40ohm",
            );
        }
        // DM single-ended 40ohm too.
        need_attr(
            &format!("{}ddr.lane{}.DM__single_ended", INTERFACE_CONSTRAINT_ATTR_PREFIX, lane),
            "40ohm",
        );
    }

    // DDR4 outer: `CK.*` dotted wildcard.
    need_attr(&format!("{}ddr.CK.P__signal_class", INTERFACE_CONSTRAINT_ATTR_PREFIX), "CLOCK");
    need_attr(&format!("{}ddr.CK.N__max_freq", INTERFACE_CONSTRAINT_ATTR_PREFIX), "1600MHz");

    // DDR4 outer: ADDR list.
    need_attr(&format!("{}ddr.A0__single_ended", INTERFACE_CONSTRAINT_ATTR_PREFIX), "50ohm");
    need_attr(&format!("{}ddr.A1__signal_class", INTERFACE_CONSTRAINT_ATTR_PREFIX), "ADDR");
    need_attr(&format!("{}ddr.CS__signal_class", INTERFACE_CONSTRAINT_ATTR_PREFIX), "ADDR");

    // DiffPair relation `P -> N: length_match 1ps` fires per pair.
    for lane in 0..4 {
        need_attr(
            &format!("{}ddr.lane{}.DQS.P__ddr.lane{}.DQS.N__length_match",
                     INTERFACE_CONSTRAINT_REL_ATTR_PREFIX, lane, lane),
            "1ps",
        );
    }
    need_attr(
        &format!("{}ddr.CK.P__ddr.CK.N__length_match", INTERFACE_CONSTRAINT_REL_ATTR_PREFIX),
        "1ps",
    );

    // Swizzle freedoms — declared protocol-level permission for the
    // router to permute signals. Zero new code landed for this; we
    // just verify the constraint property names propagate to the
    // expected leaves. Distinct names (`swizzle_within_byte` from
    // DDR4ByteLane vs `swizzle_across_bytes` from DDR4 outer) let
    // both freedoms coexist on the same DQ pin under tier-1
    // single-valued attribute storage.
    for lane in 0..4 {
        for dq in 0..8 {
            // Inner: bit swizzle within this byte.
            need_attr(
                &format!("{}ddr.lane{}.DQ{}__swizzle_within_byte", INTERFACE_CONSTRAINT_ATTR_PREFIX, lane, dq),
                "true",
            );
            // Outer: byte lanes themselves can swap.
            need_attr(
                &format!("{}ddr.lane{}.DQ{}__swizzle_across_bytes", INTERFACE_CONSTRAINT_ATTR_PREFIX, lane, dq),
                "true",
            );
        }
        need_attr(
            &format!("{}ddr.lane{}.DM__swizzle_within_byte", INTERFACE_CONSTRAINT_ATTR_PREFIX, lane),
            "true",
        );
        // The outer `lane*` wildcard reaches DQS pair too.
        need_attr(
            &format!("{}ddr.lane{}.DQS.P__swizzle_across_bytes", INTERFACE_CONSTRAINT_ATTR_PREFIX, lane),
            "true",
        );
    }
    println!("✓ swizzle freedoms (within_byte + across_bytes) propagated to every relevant leaf");

    println!("\nv0.8 DDR4 full-stack (parametric + generate + hierarchical + constraints + swizzle): PASS");
}
