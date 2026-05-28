//! v0.4 interface check: an MCU with `interface SPI spi { MOSI=PB3; ... }`
//! wired via `mcu.spi -> flash.spi` should produce nets containing the
//! MCU's **physical pins** (PB3, PB4, …), not synthetic `mcu.spi.MOSI`
//! pins. The peripheral (unbound `interface SPI:slave spi;`) still uses the
//! `flash.spi.MOSI` form. Bundle expansion bridges the two.

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

entity SomeMCU {
    pin PB2: signal inout;
    pin PB3: signal inout;
    pin PB4: signal inout;
    pin PB5: signal inout;
    interface SPI spi {
        MOSI = PB3;
        MISO = PB4;
        SCK  = PB5;
    }
}

entity Flash {
    interface SPI:slave spi;
}

board Demo {
    mcu:   SomeMCU();
    flash: Flash();
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
        &sf,
        &analysis,
        &mut netlist,
        None,
    )
    .expect("synthesis succeeded");

    let mut nets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (net_id, net) in &netlist.nets {
        let name = net.name.clone().unwrap_or_else(|| format!("Net_{:?}", net_id));
        let mut connections: Vec<String> = Vec::new();
        for (_pi_id, pi) in &netlist.pin_instances {
            if pi.net != Some(net_id) { continue; }
            let inst = netlist.instances.get(pi.instance);
            let pin  = netlist.pins.get(pi.pin_def);
            if let (Some(inst), Some(pin)) = (inst, pin) {
                connections.push(format!("{}.{}", inst.name, pin.name));
            }
        }
        connections.sort();
        if !connections.is_empty() {
            nets.insert(name, connections);
        }
    }

    println!("Synthesised {} net(s):", nets.len());
    for (name, conns) in &nets { println!("  {}: {:?}", name, conns); }

    // After bundle expansion + alias resolution, each SPI signal
    // should land on a net joining `mcu.PB?` (the *bound physical
    // pin*, not `mcu.spi.SIG`) with `flash.spi.SIG` (the unbound
    // form on the peripheral side).
    let expected_pairs = [
        ("MOSI", "mcu.PB3", "flash.spi.MOSI"),
        ("MISO", "mcu.PB4", "flash.spi.MISO"),
        ("SCK",  "mcu.PB5", "flash.spi.SCK"),
    ];
    let mut hits = 0usize;
    for (sig, mcu_pin, flash_pin) in &expected_pairs {
        let mut found = false;
        for (_, conns) in &nets {
            if conns.iter().any(|c| c == mcu_pin)
                && conns.iter().any(|c| c == flash_pin)
            {
                println!("✓ {}: net joins {} and {}", sig, mcu_pin, flash_pin);
                found = true;
                hits += 1;
                break;
            }
        }
        if !found {
            fail(&format!("no net joins {} and {} for signal {}", mcu_pin, flash_pin, sig));
        }
    }
    if hits != expected_pairs.len() {
        fail(&format!("expected {} signal pairings, found {}", expected_pairs.len(), hits));
    }

    // Also confirm: `mcu.spi.MOSI` should NOT appear in the netlist
    // — the binding aliased it to the physical pin.
    for (_, conns) in &nets {
        for c in conns {
            if c.starts_with("mcu.spi.") {
                fail(&format!("MCU side leaked an interface-signal pin name into the netlist: {}", c));
            }
        }
    }
    println!("✓ no `mcu.spi.*` pins leaked into the netlist (alias resolution worked)");

    println!("\nv0.4 bound-interface end-to-end: PASS");
}
