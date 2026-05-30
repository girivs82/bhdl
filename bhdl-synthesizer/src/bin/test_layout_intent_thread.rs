//! P&R layout-intent end-to-end thread test (handshake step 4 + 5).
//!
//! Synthesizes the annotated ATmega328P decoupling fixture and verifies
//! that the `for INTENT(...)` clauses on the chip's expansion-block
//! decoupling caps survive parse → analyzer lowering → Phase 4.5
//! materialization, landing as typed `LayoutIntent` values on the
//! corresponding netlist `Instance`s. This is exactly what bhdl-pnr's
//! `semantic.rs` reads (no string-lift boundary).

use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_parser::parse;
use bhdl_synthesizer::NetlistGenerator;
use bhdl_common::intent::vocabulary::{LayoutIntent, PinRef};
use anyhow::Result;

fn fail(msg: &str) -> ! {
    eprintln!("✗ {}", msg);
    std::process::exit(1);
}

#[tokio::main]
async fn main() -> Result<()> {
    let src = std::fs::read_to_string("tests/circuits/realistic/atmega328p_i2c_used.bhdl")
        .or_else(|_| std::fs::read_to_string("../tests/circuits/realistic/atmega328p_i2c_used.bhdl"))?;
    let pr = parse(&src);
    if !pr.errors().is_empty() {
        for e in pr.errors() { eprintln!("parse: {}", e.message); }
        std::process::exit(2);
    }
    let sf = SourceFile::cast(pr.syntax()).expect("source file");
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let netlist = gen.generate_from_ast_and_analysis(&sf, &analysis).await?;

    // Index materialized instances by name → their layout intents.
    let intents_of = |name: &str| -> Vec<LayoutIntent> {
        netlist.instances.iter()
            .find(|(_, i)| i.name == name)
            .map(|(_, i)| i.layout_intents.clone())
            .unwrap_or_default()
    };

    // The expansion children are named `<parent>_<child>` = `mcu_C_vcc` etc.
    let host = |p: &str| PinRef::HostPin(p.to_string());

    // C_vcc → high_freq_bypass(rail: VCC, return: GND1, loop_area_max: 1.5)
    match intents_of("mcu_C_vcc").as_slice() {
        [LayoutIntent::HighFreqBypass { rail, return_pin, loop_area_max_mm2, proximity_max_mm }]
            if *rail == host("VCC") && *return_pin == host("GND1")
                && (*loop_area_max_mm2 - 1.5).abs() < 1e-4
                && (*proximity_max_mm - 2.0).abs() < 1e-4 =>
        {
            println!("✓ mcu_C_vcc: HighFreqBypass(VCC→GND1, loop≤1.5mm², prox 2mm default)");
        }
        other => fail(&format!("mcu_C_vcc intents wrong: {:?}", other)),
    }

    // C_bulk → bulk_reservoir(rail: VCC, return: GND1, proximity_max: 10mm)
    match intents_of("mcu_C_bulk").as_slice() {
        [LayoutIntent::BulkReservoir { rail, return_pin, proximity_max_mm }]
            if *rail == host("VCC") && *return_pin == host("GND1")
                && (*proximity_max_mm - 10.0).abs() < 1e-4 =>
        {
            println!("✓ mcu_C_bulk: BulkReservoir(VCC→GND1, prox 10mm)");
        }
        other => fail(&format!("mcu_C_bulk intents wrong: {:?}", other)),
    }

    // C_avcc → high_freq_bypass(rail: AVCC, return: GND2, loop_area_max: 1.5)
    match intents_of("mcu_C_avcc").as_slice() {
        [LayoutIntent::HighFreqBypass { rail, return_pin, .. }]
            if *rail == host("AVCC") && *return_pin == host("GND2") =>
        {
            println!("✓ mcu_C_avcc: HighFreqBypass(AVCC→GND2)");
        }
        other => fail(&format!("mcu_C_avcc intents wrong: {:?}", other)),
    }

    // C_aref → analog_ref_filter(ref_pin: AREF, return: GND2, prox 3mm default)
    match intents_of("mcu_C_aref").as_slice() {
        [LayoutIntent::AnalogRefFilter { ref_pin, return_pin, proximity_max_mm }]
            if *ref_pin == host("AREF") && *return_pin == host("GND2")
                && (*proximity_max_mm - 3.0).abs() < 1e-4 =>
        {
            println!("✓ mcu_C_aref: AnalogRefFilter(AREF→GND2, prox 3mm default)");
        }
        other => fail(&format!("mcu_C_aref intents wrong: {:?}", other)),
    }

    // The I²C pullups carry no intent (none annotated) — confirm empty,
    // so we're not spuriously attaching.
    for r in &["mcu_R_pu_sda", "mcu_R_pu_scl"] {
        if !intents_of(r).is_empty() {
            fail(&format!("{} should carry no layout intents", r));
        }
    }
    println!("✓ I²C pullups carry no layout intents (as authored)");

    println!("\nP&R layout-intent thread (parse → lower → materialize): PASS");
    Ok(())
}
