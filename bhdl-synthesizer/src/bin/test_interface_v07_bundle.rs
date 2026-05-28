//! v0.7b end-to-end: bundle expansion + synthesis with the new
//! perspective-based interface declaration. Mirrors test_interface_bundle
//! but uses `perspective` blocks and `:slave` selector instead of
//! the v0.6 flat-signals + `~` form.

use bhdl_ast::SourceFile;
use bhdl_parser::parse;
use rowan::ast::AstNode;
use std::collections::BTreeMap;

const SOURCE: &str = r#"
interface SPI {
    perspective master {
        signal MOSI: out;
        signal MISO: in;
        signal SCK:  out;
    }
    perspective slave {
        signal MOSI: in;
        signal MISO: out;
        signal SCK:  in;
    }
}

entity Master {
    interface SPI spi;             // default = first-declared (master)
}

entity Slave {
    interface SPI:slave spi;       // v0.7 explicit perspective
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

    let signals = ["MOSI", "MISO", "SCK"];
    for sig in &signals {
        let mut found = false;
        for (_, conns) in &nets {
            if conns.iter().any(|c| c == &format!("mcu.spi.{}", sig))
                && conns.iter().any(|c| c == &format!("flash.spi.{}", sig))
            {
                println!("✓ {}: net joins mcu.spi.{} ↔ flash.spi.{}", sig, sig, sig);
                found = true;
                break;
            }
        }
        if !found {
            fail(&format!("no net joins mcu.spi.{} and flash.spi.{}", sig, sig));
        }
    }
    println!("\nv0.7b perspective end-to-end: PASS");
}
