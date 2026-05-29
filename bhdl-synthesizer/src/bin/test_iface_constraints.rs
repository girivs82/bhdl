//! v0.8 interface constraints — end-to-end synth test.
//!
//! Verifies that a DDR4-shaped interface stack (DiffPair + DDR4ByteLane
//! + DDR4) has its `constraints { }` blocks propagated as attributes
//! on the materialised pins, with:
//!   - bundle-self `*` on DiffPair attaching to both `.P` and `.N`
//!     pins of every DiffPair instantiation (CK, lane0.DQS, lane1.DQS);
//!   - per-signal properties on DDR4ByteLane attaching to ddr.laneX.DQk;
//!   - dotted wildcard `CK.*` on DDR4 attaching to ddr.CK.{P,N};
//!   - cross-bundle relation `CK -> laneN.DQS: skew_max 100ps`
//!     producing relation-attributes from each ddr.CK.* to each
//!     ddr.laneN.DQS.*.

use bhdl_ast::{AstNode, SourceFile};
use bhdl_parser::parse;
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
    interface DiffPair DQS;
    signal DM: inout;
    constraints {
        DQ0, DQ1, DQ2, DQ3, DQ4, DQ5, DQ6, DQ7: single_ended 40ohm, signal_class DATA;
    }
}

interface DDR4 {
    signal A0: out;
    signal A1: out;
    signal CS: out;
    interface DiffPair CK;
    interface DDR4ByteLane lane0;
    interface DDR4ByteLane lane1;
    constraints {
        CK.*:            signal_class CLOCK, max_freq 1600MHz;
        A0, A1:          single_ended 50ohm, signal_class ADDR;
        CK -> lane0.DQS: skew_max 100ps;
        CK -> lane1.DQS: skew_max 100ps;
    }
}

entity MemController {
    interface DDR4 ddr;
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
    let pr = parse(SOURCE);
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

    // Find the MemController module to read constraint attributes from.
    let mc_inst = netlist
        .instances
        .iter()
        .find(|(_, i)| i.name == "mc")
        .map(|(_, i)| i.definition);
    let module_id = mc_inst.expect("mc instance");
    let module = netlist.modules.get(module_id).expect("mc module");

    // Dump the constraint attributes for inspection.
    let mut const_attrs: Vec<(&String, &String)> = module
        .attributes
        .iter()
        .filter(|(k, _)| {
            k.starts_with(INTERFACE_CONSTRAINT_ATTR_PREFIX)
                || k.starts_with(INTERFACE_CONSTRAINT_REL_ATTR_PREFIX)
        })
        .collect();
    const_attrs.sort_by(|a, b| a.0.cmp(b.0));
    println!("Constraint attributes on `mc` ({}):", const_attrs.len());
    for (k, v) in &const_attrs {
        println!("  {} = {}", k, v);
    }

    let lookup = |key: &str| -> Option<String> { module.attributes.get(key).cloned() };
    let want_props = [
        // DiffPair `*: differential 100ohm` propagates to ddr.CK.P and ddr.CK.N
        ("intf_const__ddr.CK.P__differential", "100ohm"),
        ("intf_const__ddr.CK.N__differential", "100ohm"),
        // …and to lane0/lane1 DQS pairs.
        ("intf_const__ddr.lane0.DQS.P__differential", "100ohm"),
        ("intf_const__ddr.lane0.DQS.N__differential", "100ohm"),
        ("intf_const__ddr.lane1.DQS.P__differential", "100ohm"),
        ("intf_const__ddr.lane1.DQS.N__differential", "100ohm"),
        // DDR4ByteLane per-signal properties.
        ("intf_const__ddr.lane0.DQ0__single_ended", "40ohm"),
        ("intf_const__ddr.lane0.DQ7__signal_class", "DATA"),
        ("intf_const__ddr.lane1.DQ3__single_ended", "40ohm"),
        // DDR4 outer: dotted wildcard CK.*
        ("intf_const__ddr.CK.P__signal_class", "CLOCK"),
        ("intf_const__ddr.CK.N__max_freq", "1600MHz"),
        // DDR4 outer: per-signal on A0/A1
        ("intf_const__ddr.A0__single_ended", "50ohm"),
        ("intf_const__ddr.A1__signal_class", "ADDR"),
    ];

    for (k, v) in want_props.iter() {
        match lookup(k) {
            Some(actual) if actual == *v => println!("✓ {} = {}", k, v),
            Some(actual) => fail(&format!("expected `{}` = `{}`, got `{}`", k, v, actual)),
            None => fail(&format!("missing attribute `{}` (expected `{}`)", k, v)),
        }
    }

    // Pairwise relation: DiffPair `P -> N: length_match 1ps` on each pair.
    let want_rels = [
        ("intf_const_rel__ddr.CK.P__ddr.CK.N__length_match", "1ps"),
        ("intf_const_rel__ddr.lane0.DQS.P__ddr.lane0.DQS.N__length_match", "1ps"),
        ("intf_const_rel__ddr.lane1.DQS.P__ddr.lane1.DQS.N__length_match", "1ps"),
    ];
    for (k, v) in want_rels.iter() {
        match lookup(k) {
            Some(actual) if actual == *v => println!("✓ {} = {}", k, v),
            Some(actual) => fail(&format!("expected `{}` = `{}`, got `{}`", k, v, actual)),
            None => fail(&format!("missing relation attribute `{}`", k)),
        }
    }

    // Cross-bundle: CK -> lane0.DQS: skew_max 100ps
    // Should cross-product CK.{P,N} × lane0.DQS.{P,N} = 4 entries per lane.
    let mut skew_count = 0usize;
    for (k, _v) in module.attributes.iter() {
        if k.starts_with("intf_const_rel__ddr.CK.")
            && k.contains("__ddr.lane0.DQS.")
            && k.ends_with("__skew_max")
        {
            skew_count += 1;
        }
    }
    if skew_count != 4 {
        fail(&format!(
            "expected 4 CK→lane0.DQS skew_max relations (2×2 cross-product), got {}",
            skew_count));
    }
    println!("✓ CK → lane0.DQS skew_max: 4 cross-product relations");

    println!("\nv0.8 interface constraints (tier 1): PASS");
}
