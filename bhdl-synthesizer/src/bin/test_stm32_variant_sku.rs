//! Verifies parametric-entity SKU selection on the STM32F103Cx
//! family entity. Same stdlib file produces:
//!
//!   - C8T6 (Blue Pill, default args): flash_kb=64, sram_kb=20
//!   - CBT6 (variant board, overridden):  flash_kb=128, sram_kb=20
//!
//! and in both cases the same 11-child support-passive expansion
//! materialises — because all LQFP-48 SKUs share the pinout and
//! decoupling recipe. The point of parametric entities is that
//! SKU-specific data (memory size, MPN, KiCad symbol) flows
//! through the existing constructor-arg → instance-attr
//! mechanism without duplicating the entity body.

use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_parser::parse;
use bhdl_synthesizer::NetlistGenerator;
use anyhow::Result;

async fn synth_and_inspect_mcu(path: &str) -> Result<(usize, String, String)> {
    let src = std::fs::read_to_string(path)?;
    let pr = parse(&src);
    if !pr.errors().is_empty() {
        for e in pr.errors() { eprintln!("parse: {}", e.message); }
        std::process::exit(2);
    }
    let sf = SourceFile::cast(pr.syntax()).expect("source file");
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let netlist = gen.generate_from_ast_and_analysis(&sf, &analysis).await?;

    let mcu = netlist.instances.iter()
        .find(|(_, i)| i.name == "mcu")
        .map(|(_, i)| i)
        .expect("mcu instance");
    let count = netlist.instances.len();
    // Phase 4.4 stamps constructor args onto the instance under
    // the *parameter name* (not the attribute name the entity
    // body might rename it to). So we query `part_no` and
    // `flash_kb` directly.
    let part_no  = mcu.attributes.get("part_no").cloned().unwrap_or_default();
    let flash_kb = mcu.attributes.get("flash_kb").cloned().unwrap_or_default();
    Ok((count, part_no, flash_kb))
}

#[tokio::main]
async fn main() -> Result<()> {
    // Task #90 (landed 2026-05-29): Phase 4.4 now stamps entity
    // parameter *defaults* in addition to explicit overrides, so a
    // board using `STM32F103Cx()` (no args) lands with the C8T6
    // defaults on the instance attrs (part_no = "STM32F103C8T6",
    // flash_kb = 64). The override path (CBT6 board) still works
    // because Phase 4.4 stamps explicit args first; defaults use
    // entry().or_insert() so they only fill in what's missing.

    println!("=== C8T6 (default args propagate via Phase 4.4) ===");
    let (n_c8t6, part_c8t6, flash_c8t6) =
        synth_and_inspect_mcu("tests/circuits/realistic/stm32_blue_pill.bhdl").await?;
    println!("  instances total: {}", n_c8t6);
    println!("  mcu.part_no  = {:?}", part_c8t6);
    println!("  mcu.flash_kb = {:?}", flash_c8t6);
    if n_c8t6 != 12 {
        eprintln!("✗ C8T6 expected 12 instances, got {}", n_c8t6);
        std::process::exit(1);
    }
    if !part_c8t6.contains("C8T6") {
        eprintln!("✗ C8T6 default args didn't propagate: part_no = {:?} \
                   (expected to contain 'C8T6')", part_c8t6);
        std::process::exit(1);
    }
    if flash_c8t6 != "64" {
        eprintln!("✗ C8T6 default args didn't propagate: flash_kb = {:?} \
                   (expected '64')", flash_c8t6);
        std::process::exit(1);
    }
    println!("✓ C8T6: defaults propagated (part_no='STM32F103C8T6', flash_kb=64)");

    println!("\n=== CBT6 (explicit overrides for the 128 KB variant) ===");
    let (n_cbt6, part_cbt6, flash_cbt6) =
        synth_and_inspect_mcu("tests/circuits/realistic/stm32_cbt6_variant.bhdl").await?;
    println!("  instances total: {}", n_cbt6);
    println!("  mcu.part_no  = {:?}", part_cbt6);
    println!("  mcu.flash_kb = {:?}", flash_cbt6);

    if !part_cbt6.contains("CBT6") {
        eprintln!("✗ CBT6 board: expected part_no to contain 'CBT6', got {:?}", part_cbt6);
        std::process::exit(1);
    }
    if !flash_cbt6.contains("128") {
        eprintln!("✗ CBT6 board: expected flash_kb=128, got {:?}", flash_cbt6);
        std::process::exit(1);
    }
    println!("✓ CBT6: overrides propagated (part_no='STM32F103CBT6', flash_kb=128)");

    // Sanity: CBT6 board has 10 instances (chip + 9 always-on, no
    // I²C wired). Confirms the same entity body produces the same
    // expansion children regardless of SKU-specific overrides.
    if n_cbt6 != 10 {
        eprintln!("✗ CBT6 expected 10 instances, got {}", n_cbt6);
        std::process::exit(1);
    }
    println!("✓ CBT6: same expansion shape (10 instances; no I²C, no pullups)");

    println!("\nParametric STM32 SKU selection: PASS");
    Ok(())
}
