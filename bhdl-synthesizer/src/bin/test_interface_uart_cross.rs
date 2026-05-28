//! v0.7c: UART cross-name wiring via `wires { dte.TX <-> dce.RX; }`.
//!
//! The whole point of perspectives + wires{} is to make this work:
//! the master's TX wire is the slave's RX wire, and vice versa.
//! A board author writes `mcu.uart -> bridge.uart` once and the
//! synthesizer cross-pairs the signals according to the interface's
//! wires{} declaration.

use bhdl_ast::SourceFile;
use bhdl_parser::parse;
use rowan::ast::AstNode;
use std::collections::BTreeMap;

const SOURCE: &str = r#"
interface UART {
    perspective dte {
        signal TX: out;
        signal RX: in;
    }
    perspective dce {
        signal TX: out;
        signal RX: in;
    }
    wires {
        dte.TX <-> dce.RX;
        dte.RX <-> dce.TX;
    }
}

entity MCU    { interface UART       uart; }   // default = dte
entity Bridge { interface UART:dce   uart; }

board Demo {
    mcu:    MCU();
    bridge: Bridge();
    mcu.uart -> bridge.uart;
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

    let mut nets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (net_id, net) in &netlist.nets {
        let name = net.name.clone().unwrap_or_else(|| format!("Net_{:?}", net_id));
        let mut connections: Vec<String> = Vec::new();
        for (_pi_id, pi) in &netlist.pin_instances {
            if pi.net != Some(net_id) { continue; }
            let inst = netlist.instances.get(pi.instance);
            let pin = netlist.pins.get(pi.pin_def);
            if let (Some(inst), Some(pin)) = (inst, pin) {
                connections.push(format!("{}.{}", inst.name, pin.name));
            }
        }
        connections.sort();
        if !connections.is_empty() { nets.insert(name, connections); }
    }

    println!("Synthesised {} net(s):", nets.len());
    for (n, c) in &nets { println!("  {}: {:?}", n, c); }

    // The wire pairings — what the wires{} block decreed:
    //   mcu.uart.TX (out) ↔ bridge.uart.RX (in)
    //   mcu.uart.RX (in)  ↔ bridge.uart.TX (out)
    let expected = [
        ("MCU.TX → bridge.RX", "mcu.uart.TX", "bridge.uart.RX"),
        ("MCU.RX ← bridge.TX", "mcu.uart.RX", "bridge.uart.TX"),
    ];
    for (label, a, b) in &expected {
        let mut found = false;
        for (_, conns) in &nets {
            if conns.iter().any(|c| c == a) && conns.iter().any(|c| c == b) {
                println!("✓ {}: net joins {} ↔ {}", label, a, b);
                found = true;
                break;
            }
        }
        if !found {
            fail(&format!("no net joins {} and {} for {}", a, b, label));
        }
    }
    println!("\nv0.7c UART cross-name wiring: PASS");
}
