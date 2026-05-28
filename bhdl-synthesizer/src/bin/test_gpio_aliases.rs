//! Verifies v0.9a function aliases.
//!
//! Board uses `mcu.gpio11`, `mcu.gpio12`, `mcu.gpio13`, `mcu.adc0`,
//! `mcu.reset` — every reference goes through the chip's
//! `aliases { }` block, which maps each logical name to a physical
//! port pin (PB3/PB4/PB5/PC0/PC6). The synthesizer should resolve
//! the alias and connect the corresponding physical pin, so the
//! emitted netlist shows the *physical* pin names in the
//! connections (since that's what landed via find_pin_instance).

use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_parser::parse;
use bhdl_synthesizer::NetlistGenerator;
use anyhow::Result;
use std::collections::BTreeMap;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let src = std::fs::read_to_string(
        "tests/circuits/realistic/atmega328p_gpio_aliases.bhdl")?;
    let pr = parse(&src);
    if !pr.errors().is_empty() {
        for e in pr.errors() { eprintln!("parse: {}", e.message); }
        std::process::exit(2);
    }
    let sf = SourceFile::cast(pr.syntax()).expect("source file");
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let netlist = gen.generate_from_ast_and_analysis(&sf, &analysis).await?;

    // Collect (net_name → sorted [refdes.pin]) the same way the
    // interface tests do, so we can see which physical pins
    // landed on which board net.
    let mut nets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (net_id, net) in &netlist.nets {
        let name = net.name.clone().unwrap_or_else(|| format!("Net_{:?}", net_id));
        let mut conns: Vec<String> = Vec::new();
        for (_pi_id, pi) in &netlist.pin_instances {
            if pi.net != Some(net_id) { continue; }
            let inst = netlist.instances.get(pi.instance);
            let pin = netlist.pins.get(pi.pin_def);
            if let (Some(inst), Some(pin)) = (inst, pin) {
                conns.push(format!("{}.{}", inst.name, pin.name));
            }
        }
        conns.sort();
        if !conns.is_empty() { nets.insert(name, conns); }
    }

    println!("Nets ({} total):", nets.len());
    for (n, conns) in &nets {
        println!("  {}: {:?}", n, conns);
    }

    // Each board-level net we declared should contain the
    // *physical* pin that the alias resolves to.
    let expected = [
        ("MOSI_NET",  "mcu.PB3"),  // gpio11 → PB3
        ("MISO_NET",  "mcu.PB4"),  // gpio12 → PB4
        ("SCK_NET",   "mcu.PB5"),  // gpio13 → PB5
        ("ADC_IN",    "mcu.PC0"),  // adc0   → PC0
        ("RESET_NET", "mcu.PC6"),  // reset  → PC6
    ];
    for (net_name, expected_pin) in &expected {
        let net = nets.iter()
            .find(|(name, _)| name.as_str() == *net_name)
            .map(|(_, conns)| conns);
        match net {
            Some(conns) => {
                if !conns.iter().any(|p| p == expected_pin) {
                    eprintln!("✗ net @{} expected to contain {} (from alias resolution), got {:?}",
                              net_name, expected_pin, conns);
                    std::process::exit(1);
                }
                println!("✓ @{}: contains {} (alias correctly resolved)", net_name, expected_pin);
            }
            None => {
                eprintln!("✗ net @{} not found in netlist", net_name);
                std::process::exit(1);
            }
        }
    }

    println!("\nFunction aliases (v0.9a): PASS");
    Ok(())
}
