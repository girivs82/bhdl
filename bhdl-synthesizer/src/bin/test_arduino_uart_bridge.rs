//! Cross-chip UART smoke test: the v0.7 perspective + wires{}
//! payoff for the actual Arduino topology.
//!
//! On a real Arduino Uno, the host ATmega328P talks to the
//! ATmega16U2 USB-serial bridge over a single UART line pair.
//! The stdlib now declares both chips' UART interfaces with
//! perspectives baked in: 328p's `uart` defaults to `dte`, and
//! the 16U2 (sitting between USB and the host MCU) is wired as
//! `dce` at the board level. A single `host.uart -> bridge.uart`
//! statement should produce two nets cross-pairing TX↔RX.
//!
//! This isn't an Arduino-Uno round-trip (which is exercised by
//! bhdl-kicad-import). It's the minimal proof that the stdlib's
//! new MCU interface declarations actually wire correctly at
//! synth time.

use bhdl_ast::SourceFile;
use bhdl_parser::parse;
use rowan::ast::AstNode;
use std::collections::BTreeMap;

const SOURCE: &str = r#"
// Cross-name UART with explicit perspectives.
interface UART {
    perspective dte { signal TX: out; signal RX: in; }
    perspective dce { signal TX: out; signal RX: in; }
    wires {
        dte.TX <-> dce.RX;
        dte.RX <-> dce.TX;
    }
}

// Host MCU (acts as dte by default).
entity HostMCU {
    pin PD0: signal inout;   // RXD
    pin PD1: signal inout;   // TXD
    interface UART uart {
        TX = PD1;
        RX = PD0;
    }
}

// USB-serial bridge (explicitly :dce, matching ATmega16U2 / FT232RL
// stdlib declarations).
entity Bridge {
    pin TXD: signal inout;
    pin RXD: signal inout;
    interface UART:dce uart {
        TX = TXD;
        RX = RXD;
    }
}

board ArduinoLikeUart {
    host:   HostMCU();
    bridge: Bridge();
    host.uart -> bridge.uart;
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

    // The cross-pairing the stdlib's wires{} block decrees:
    //   host.PD1 (TX) ↔ bridge.RXD (chip-in)
    //   host.PD0 (RX) ↔ bridge.TXD (chip-out)
    let expected = [
        ("host TX → bridge RX", "host.PD1", "bridge.RXD"),
        ("host RX ← bridge TX", "host.PD0", "bridge.TXD"),
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
    println!("\nCross-chip UART (dte ↔ dce) via stdlib MCU interfaces: PASS");
}
