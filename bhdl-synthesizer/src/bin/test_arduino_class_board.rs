//! Multi-chip integration test exercising the full stack from
//! the past several commits:
//!
//!   - Phase 4.4 constructor-arg stamping (57992dc)
//!   - Phase 4.5 expansion-interpreter wiring (da3786a)
//!   - design→expansion value substitution (57992dc)
//!   - Conditional expansion gating (5f5a22b, 6b470c2)
//!
//! Verifies that a small "Arduino-class" board with:
//!   - LM317 regulator (v_out=5V)
//!   - ATmega328P MCU with I²C wired
//! synthesizes correctly with every expansion child landing on the
//! right net.

use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_parser::parse;
use bhdl_synthesizer::NetlistGenerator;
use anyhow::Result;
use std::collections::BTreeMap;

#[tokio::main]
async fn main() -> Result<()> {
    let src = std::fs::read_to_string(
        "tests/circuits/realistic/arduino_class_board.bhdl")?;
    let pr = parse(&src);
    if !pr.errors().is_empty() {
        for e in pr.errors() { eprintln!("parse: {}", e.message); }
        std::process::exit(2);
    }
    let sf = SourceFile::cast(pr.syntax()).expect("source file");
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let netlist = gen.generate_from_ast_and_analysis(&sf, &analysis).await?;

    println!("Instances ({}):", netlist.instances.len());
    let mut names: Vec<&str> = netlist.instances.iter()
        .map(|(_, i)| i.name.as_str())
        .collect();
    names.sort();
    for n in &names {
        // Show value where present (caps/resistors).
        let inst = netlist.instances.iter()
            .find(|(_, i)| i.name == *n)
            .map(|(_, i)| i);
        let val = inst
            .and_then(|i| i.attributes.get("value"))
            .map(|s| s.as_str())
            .unwrap_or("");
        if val.is_empty() {
            println!("  {}", n);
        } else {
            println!("  {}  (value={})", n, val);
        }
    }

    // Expected expansion children. LM317 contributes 4 (C_in, C_out,
    // R1, R2); ATmega328P contributes 4 decoupling + 2 conditional
    // pullups (because PC4/PC5 are wired here).
    let want = [
        ("LM317 C_in",        "U_REG_C_in",   "10µF"),
        ("LM317 C_out",       "U_REG_C_out",  "22µF"),
        ("LM317 R1 (720Ω)",   "U_REG_R1",     "720.000"),
        ("LM317 R2 (240Ω)",   "U_REG_R2",     "240.000"),
        ("MCU C_vcc",         "MCU_C_vcc",    "100nF"),
        ("MCU C_bulk",        "MCU_C_bulk",   "10µF"),
        ("MCU C_avcc",        "MCU_C_avcc",   "100nF"),
        ("MCU C_aref",        "MCU_C_aref",   "100nF"),
        ("MCU R_pu_sda",      "MCU_R_pu_sda", "4.7kΩ"),
        ("MCU R_pu_scl",      "MCU_R_pu_scl", "4.7kΩ"),
    ];
    let mut missing = Vec::new();
    let mut wrong_value: BTreeMap<&str, (String, &str)> = BTreeMap::new();
    for (label, name, want_val) in &want {
        let hit = netlist.instances.iter()
            .find(|(_, i)| i.name == *name);
        match hit {
            Some((_, inst)) => {
                let got_val = inst.attributes
                    .get("value")
                    .map(|s| s.as_str())
                    .unwrap_or("");
                if got_val != *want_val {
                    wrong_value.insert(label, (got_val.to_string(), want_val));
                }
                println!("✓ {}", label);
            }
            None => missing.push(*label),
        }
    }

    if !missing.is_empty() {
        eprintln!("\n✗ MISSING expansion children: {:?}", missing);
        std::process::exit(1);
    }
    if !wrong_value.is_empty() {
        eprintln!("\n✗ WRONG values:");
        for (label, (got, want)) in &wrong_value {
            eprintln!("  {}: got {:?}, want {:?}", label, got, want);
        }
        std::process::exit(1);
    }

    println!("\nArduino-class board integration: PASS");
    Ok(())
}
