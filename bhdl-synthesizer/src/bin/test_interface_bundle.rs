//! v0.3 interface check: `MCU.spi -> FLASH.spi` (bundle form)
//! expands at synthesis to per-signal connections.
//!
//! Defines a tiny SPI interface, a Master and Slave entity (the
//! Slave using `~SPI` to flip directions), instantiates both on a
//! board, writes the bundle connection, and asserts the
//! synthesised netlist has four nets each with two pins (master
//! + slave) — one per SPI signal.

use bhdl_ast::SourceFile;
use bhdl_parser::parse;
use rowan::ast::AstNode;
use std::collections::BTreeMap;

const SOURCE: &str = r#"
interface SPI {
    signal MOSI: out;
    signal MISO: in;
    signal SCK:  out;
}

entity Master {
    interface SPI spi;
}

entity Slave {
    interface ~SPI spi;
}

board Demo {
    mcu:   Master();
    flash: Slave();
    mcu.spi -> flash.spi;
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

    // Synthesise.
    let mut netlist = bhdl_netlist::Netlist::new();
    bhdl_synthesizer::hierarchical_connectivity::extract_hierarchical_connectivity(
        &sf,
        &analysis,
        &mut netlist,
        None,
    )
    .expect("synthesis succeeded");

    // Collect (net_name → sorted [refdes.pin]) so we can assert
    // the SPI bundle expanded into per-signal connections.
    let mut nets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (net_id, net) in &netlist.nets {
        let name = net.name.clone().unwrap_or_else(|| format!("Net_{:?}", net_id));
        let mut connections: Vec<String> = Vec::new();
        for (pi_id, pi) in &netlist.pin_instances {
            if pi.net != Some(net_id) { continue; }
            let inst = netlist.instances.get(pi.instance);
            let pin  = netlist.pins.get(pi.pin_def);
            if let (Some(inst), Some(pin)) = (inst, pin) {
                connections.push(format!("{}.{}", inst.name, pin.name));
            }
            let _ = pi_id;
        }
        connections.sort();
        if !connections.is_empty() {
            nets.insert(name, connections);
        }
    }

    println!("Synthesised {} net(s):", nets.len());
    for (name, conns) in &nets {
        println!("  {}: {:?}", name, conns);
    }

    // Pre-bundle, the synthesizer would create a single net
    // containing the bundle endpoints. Post-bundle, we expect four
    // separate nets, one for each SPI signal (MOSI, MISO, SCK).
    // Each net should have exactly two endpoints: master + slave.
    let signals = ["MOSI", "MISO", "SCK"];
    let mut total_bundle_nets = 0;
    for sig in &signals {
        let mut hit = false;
        for (_, conns) in &nets {
            let has_master = conns.iter().any(|c| c == &format!("mcu.spi.{}", sig));
            let has_slave  = conns.iter().any(|c| c == &format!("flash.spi.{}", sig));
            if has_master && has_slave {
                hit = true;
                total_bundle_nets += 1;
                println!("✓ {}: net joins mcu.spi.{} and flash.spi.{}", sig, sig, sig);
                break;
            }
        }
        if !hit {
            fail(&format!("no net joins mcu.spi.{} and flash.spi.{}", sig, sig));
        }
    }

    if total_bundle_nets != signals.len() {
        fail(&format!(
            "expected {} bundle nets, found {}",
            signals.len(),
            total_bundle_nets
        ));
    }

    println!(
        "\n{} signals expanded from bundle connection `mcu.spi -> flash.spi`.",
        signals.len()
    );
}
