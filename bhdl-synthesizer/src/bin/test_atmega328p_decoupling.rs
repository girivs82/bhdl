//! Verify auto-expansion of the ATmega328P decoupling network.
//!
//! Mirrors the pattern in `test_lm317_virtual_pins.rs`: load a
//! board file that explicitly imports the stdlib entity, run it
//! through the full synthesizer pipeline (including import
//! loader → analyzer → expansion interpreter), and check the
//! expansion children land in `netlist.instances`.

use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_parser::parse;
use bhdl_synthesizer::NetlistGenerator;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let test_file = "tests/circuits/realistic/atmega328p_decoupling.bhdl";
    let src = std::fs::read_to_string(test_file)?;
    let pr = parse(&src);
    if !pr.errors().is_empty() {
        for e in pr.errors() { eprintln!("parse: {}", e.message); }
        std::process::exit(2);
    }
    let sf = SourceFile::cast(pr.syntax()).expect("source file");
    let analysis = analyze(&sf);
    eprintln!("Analyzer diagnostics: {}", analysis.diagnostics.len());

    let mut gen = NetlistGenerator::new();
    let netlist = gen.generate_from_ast_and_analysis(&sf, &analysis).await?;

    println!("Instances ({}):", netlist.instances.len());
    for (_id, inst) in &netlist.instances {
        println!("  {}", inst.name);
    }

    let want = ["C_vcc", "C_bulk", "C_avcc", "C_aref"];
    let mut missing = Vec::new();
    for n in &want {
        let hit = netlist.instances.iter().any(|(_, inst)| {
            inst.name == *n
                || inst.name.ends_with(&format!("_{}", n))
                || inst.name.contains(&format!("_{}_", n))
                || inst.name.contains(n)
        });
        if hit { println!("✓ found {}", n); } else { missing.push(*n); }
    }

    if !missing.is_empty() {
        eprintln!("\n✗ MISSING expansion children: {:?}", missing);
        std::process::exit(1);
    }
    println!("\nATmega328P decoupling auto-expansion: PASS");
    Ok(())
}
