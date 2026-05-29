//! v0.8 hierarchical sub-interfaces — synthesizer-side smoke test.
//!
//! Verifies that an entity declaring `interface DualUART duart;`,
//! where `DualUART { interface UartChannel ch0; interface UartChannel
//! ch1; }`, materialises pins at the leaf level
//! (`duart.ch0.TX`, `duart.ch0.RX`, `duart.ch1.TX`, `duart.ch1.RX`),
//! and that a bundle-to-bundle binding fans those leaves out into
//! pairwise nets crossing the cross-name xwire (dte.TX ↔ dce.RX).

use bhdl_ast::{AstNode, SourceFile};
use bhdl_parser::parse;
use std::collections::BTreeMap;

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

entity HostMCU {
    interface DualUART:dte duart;
}

entity Peripheral {
    interface DualUART:dce duart;
}

board TestBoard {
    power VCC = 3.3V @ 1A;
    ground GND;

    mcu: HostMCU();
    per: Peripheral();

    mcu.duart -> per.duart;
}
"#;

fn fail(msg: &str) -> ! {
    eprintln!("✗ {}", msg);
    std::process::exit(1);
}

fn main() {
    let pr = parse(SOURCE);
    if !pr.errors().is_empty() {
        fail(&format!("parse errors: {:?}", pr.errors()));
    }
    let sf = SourceFile::cast(pr.syntax()).expect("source file");
    let analysis = bhdl_analyzer::analyze(&sf);
    let mut netlist = bhdl_netlist::Netlist::new();
    bhdl_synthesizer::hierarchical_connectivity::extract_hierarchical_connectivity(
        &sf, &analysis, &mut netlist, None,
    ).expect("synthesis succeeded");

    // 1) leaf-pin materialisation
    let mut pins_per_instance: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (_pi_id, pi) in &netlist.pin_instances {
        let inst = match netlist.instances.get(pi.instance) { Some(i) => i, None => continue };
        let pin  = match netlist.pins.get(pi.pin_def)      { Some(p) => p, None => continue };
        pins_per_instance.entry(inst.name.clone()).or_default().push(pin.name.clone());
    }
    for v in pins_per_instance.values_mut() { v.sort(); v.dedup(); }
    println!("Pin instances per board instance:");
    for (i, ps) in &pins_per_instance {
        println!("  {}: {:?}", i, ps);
    }

    let want_leaves = [
        ("mcu", "duart.ch0.TX"),
        ("mcu", "duart.ch0.RX"),
        ("mcu", "duart.ch1.TX"),
        ("mcu", "duart.ch1.RX"),
        ("per", "duart.ch0.TX"),
        ("per", "duart.ch0.RX"),
        ("per", "duart.ch1.TX"),
        ("per", "duart.ch1.RX"),
    ];
    for (inst, pin) in want_leaves.iter() {
        let have = pins_per_instance
            .get(*inst)
            .map(|v| v.iter().any(|p| p == pin))
            .unwrap_or(false);
        if !have {
            fail(&format!("missing pin {}.{}", inst, pin));
        }
    }
    println!("✓ all 8 leaf pins materialised");

    // 2) bundle-fanout: nets joining cross-name pairs.
    let mut nets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (net_id, net) in &netlist.nets {
        let name = net.name.clone().unwrap_or_else(|| format!("Net_{:?}", net_id));
        let mut conns: Vec<String> = Vec::new();
        for (_pi_id, pi) in &netlist.pin_instances {
            if pi.net != Some(net_id) { continue; }
            let inst = netlist.instances.get(pi.instance);
            let pin  = netlist.pins.get(pi.pin_def);
            if let (Some(inst), Some(pin)) = (inst, pin) {
                conns.push(format!("{}.{}", inst.name, pin.name));
            }
        }
        conns.sort();
        if !conns.is_empty() { nets.insert(name, conns); }
    }
    println!("\nSynthesised {} net(s):", nets.len());
    for (n, c) in &nets { println!("  {}: {:?}", n, c); }

    // dte.TX -> dce.RX, dte.RX -> dce.TX, across ch0 and ch1 → 4 expected joins.
    let pairs = [
        ("mcu.duart.ch0.TX", "per.duart.ch0.RX"),
        ("mcu.duart.ch0.RX", "per.duart.ch0.TX"),
        ("mcu.duart.ch1.TX", "per.duart.ch1.RX"),
        ("mcu.duart.ch1.RX", "per.duart.ch1.TX"),
    ];
    for (a, b) in pairs.iter() {
        let joined = nets.values().any(|conns| {
            conns.iter().any(|c| c == a) && conns.iter().any(|c| c == b)
        });
        if !joined {
            fail(&format!("no net joins {} ↔ {}", a, b));
        }
        println!("✓ joined: {} ↔ {}", a, b);
    }

    println!("\nv0.8 sub-interface synth: PASS");
}
