//! DDR4 stdlib validation test.
//!
//! Loads the real stdlib files (interfaces/ddr4.bhdl +
//! actives/ddr4_sdram.bhdl), composes a board with a memory
//! controller (DDR4<byte_lanes=2>) and an x8 SDRAM chip, and runs
//! the proven preprocess → parse → analyze → extract_hierarchical_
//! connectivity path (the same one test_ddr4_full_stack uses) to
//! verify the stdlib content actually:
//!
//!   1. monomorphises DDR4<byte_lanes=2> + unrolls its generate loop
//!   2. materialises the full hierarchical pin set on both the
//!      controller (ddr.ca.*, ddr.lane0/1.*) and the SDRAM chip
//!      (dat.*, ca.*, plus explicit power/ZQ pins)
//!   3. propagates the DDR4 protocol constraints (impedance, signal
//!      class, swizzle freedom) to the materialised leaves
//!   4. has a well-formed expansion block (the analyzer extracts the
//!      6-instance ZQ/decoupling recipe — confirmed via the analysis
//!      result).
//!
//! NOTE: the *full* synthesize_from_source pipeline (which would also
//! run the expansion interpreter to instantiate the ZQ resistor +
//! decoupling caps) does not yet compose cleanly with `import` +
//! parametric preprocessing — imported passive entities leak as
//! instances and hierarchical modules flatten to empty. That
//! integration gap is tracked separately; the expansion *interpreter*
//! itself is proven by the stm32/atmega decoupling tests. Here we
//! assert the recipe is extracted, not that children are instantiated.

use bhdl_ast::{AstNode, SourceFile};
use bhdl_synthesizer::parametric_resolver;
use bhdl_synthesizer::hierarchical_connectivity::INTERFACE_CONSTRAINT_ATTR_PREFIX;
use bhdl_synthesizer::synthesize_from_source;
use bhdl_parser::parse;

fn fail(msg: &str) -> ! {
    eprintln!("✗ {}", msg);
    std::process::exit(1);
}

#[tokio::main]
async fn main() {
    let iface = std::fs::read_to_string("bhdl-stdlib/interfaces/ddr4.bhdl")
        .or_else(|_| std::fs::read_to_string("../bhdl-stdlib/interfaces/ddr4.bhdl"))
        .unwrap_or_else(|e| fail(&format!("read ddr4.bhdl: {}", e)));
    let sdram = std::fs::read_to_string("bhdl-stdlib/actives/ddr4_sdram.bhdl")
        .or_else(|_| std::fs::read_to_string("../bhdl-stdlib/actives/ddr4_sdram.bhdl"))
        .unwrap_or_else(|e| fail(&format!("read ddr4_sdram.bhdl: {}", e)));

    let board = r#"
entity MemController {
    interface DDR4<byte_lanes=2> ddr;
}

board DDR4TestBoard {
    power VDD = 1.2V @ 2A;
    ground GND;
    mc: MemController();
    u1: DDR4_SDRAM_x8();
}
"#;

    let source = format!("{}\n{}\n{}", iface, sdram, board);

    // 1. Parametric monomorphisation + generate unroll.
    let rewritten = match parametric_resolver::preprocess(&source) {
        Ok(s) => s,
        Err(e) => fail(&format!("preprocess: {}", e)),
    };
    for needed in &["interface DDR4__byte_lanes_2", "interface DDR4ByteLane lane"] {
        // (the second won't appear — DDR4 uses DDR4Data lanes — just check the first)
        let _ = needed;
    }
    if !rewritten.contains("interface DDR4__byte_lanes_2") {
        fail("monomorphisation `DDR4__byte_lanes_2` not produced");
    }
    if rewritten.contains("generate for") {
        fail("rewritten source still contains an unexpanded generate loop");
    }
    println!("✓ DDR4<byte_lanes=2> monomorphised + generate unrolled");

    let pr = parse(&rewritten);
    if !pr.errors().is_empty() {
        eprintln!("parse errors:");
        for e in pr.errors().iter().take(20) { eprintln!("  {:?}", e); }
        std::process::exit(1);
    }
    let sf = SourceFile::cast(pr.syntax()).expect("source file");
    let analysis = bhdl_analyzer::analyze(&sf);

    // 4. Expansion recipe well-formed: the analyzer extracts it.
    let recipe_count = analysis.expansion_recipes.len();
    if recipe_count == 0 {
        fail("analyzer extracted no expansion recipe for DDR4_SDRAM_x8");
    }
    println!("✓ expansion recipe extracted for DDR4_SDRAM_x8 ({} recipe(s))", recipe_count);

    let mut netlist = bhdl_netlist::Netlist::new();
    bhdl_synthesizer::hierarchical_connectivity::extract_hierarchical_connectivity(
        &sf, &analysis, &mut netlist, None,
    ).expect("synthesis succeeded");

    let pins_of = |inst_name: &str| -> Vec<String> {
        let def = netlist.instances.iter()
            .find(|(_, i)| i.name == inst_name)
            .map(|(_, i)| i.definition);
        match def.and_then(|d| netlist.modules.get(d)) {
            Some(m) => m.pins.iter().filter_map(|pid| netlist.pins.get(*pid).map(|p| p.name.clone())).collect(),
            None => Vec::new(),
        }
    };
    let attr = |inst_name: &str, key: &str| -> Option<String> {
        let def = netlist.instances.iter()
            .find(|(_, i)| i.name == inst_name)
            .map(|(_, i)| i.definition);
        def.and_then(|d| netlist.modules.get(d)).and_then(|m| m.attributes.get(key).cloned())
    };

    // 2a. Controller pins (parametric + generate + hierarchical).
    let mc_pins = pins_of("mc");
    let want_mc = [
        "ddr.ca.CK_t", "ddr.ca.CK_c", "ddr.ca.A0", "ddr.ca.CS_n", "ddr.ca.ALERT_n",
        "ddr.lane0.DQ0", "ddr.lane0.DQ7", "ddr.lane0.DM",
        "ddr.lane0.DQS.P", "ddr.lane0.DQS.N",
        "ddr.lane1.DQ0", "ddr.lane1.DQ7",
        "ddr.lane1.DQS.P", "ddr.lane1.DQS.N",
    ];
    for w in want_mc.iter() {
        if !mc_pins.iter().any(|p| p == w) {
            fail(&format!("controller missing pin `{}` (has {} pins)", w, mc_pins.len()));
        }
    }
    if mc_pins.iter().any(|p| p.starts_with("ddr.lane2.")) {
        fail("controller leaked lane2 (byte_lanes=2 should stop at lane1)");
    }
    println!("✓ controller: ca + lane0 + lane1 materialised ({} pins), no lane2", mc_pins.len());

    // 2b. SDRAM chip pins (unbound interface fields + explicit power/ZQ).
    let u1_pins = pins_of("u1");
    for w in &["dat.DQ0", "dat.DQ7", "dat.DM", "dat.DQS.P", "dat.DQS.N",
               "ca.CK_t", "ca.A0", "ca.CS_n", "ca.ALERT_n",
               "VDD", "VDDQ", "VPP", "VREFCA", "ZQ", "VSS", "VSSQ"] {
        if !u1_pins.iter().any(|p| p == w) {
            fail(&format!("SDRAM u1 missing pin `{}` (has {} pins)", w, u1_pins.len()));
        }
    }
    println!("✓ SDRAM u1: dat.* + ca.* + power/ZQ materialised ({} pins)", u1_pins.len());

    // 3. Constraint propagation on both controller and SDRAM leaves.
    let want_attrs = [
        ("mc", "intf_const__ddr.lane0.DQ0__single_ended", "34ohm"),
        ("mc", "intf_const__ddr.lane0.DQ0__signal_class", "DATA"),
        ("mc", "intf_const__ddr.lane0.DQS.P__differential", "80ohm"),
        ("mc", "intf_const__ddr.ca.CK_t__differential", "100ohm"),
        ("mc", "intf_const__ddr.ca.CK_t__signal_class", "CLOCK"),
        ("mc", "intf_const__ddr.ca.A0__signal_class", "ADDR"),
        ("mc", "intf_const__ddr.lane0.DQ0__swizzle_within_byte", "true"),
        ("mc", "intf_const__ddr.lane0.DQ0__swizzle_across_bytes", "true"),
        ("mc", "intf_const__ddr.ca.A0__topology", "fly_by"),
        // SDRAM side carries the same protocol constraints on its own leaves.
        ("u1", "intf_const__dat.DQ0__single_ended", "34ohm"),
        ("u1", "intf_const__dat.DQS.P__differential", "80ohm"),
        ("u1", "intf_const__ca.CK_t__signal_class", "CLOCK"),
    ];
    for (inst, key, val) in want_attrs.iter() {
        match attr(inst, key) {
            Some(a) if a == *val => println!("✓ {}::{} = {}", inst, key, val),
            Some(a) => fail(&format!("{}::{} = `{}`, expected `{}`", inst, key, a, val)),
            None => {
                let def = netlist.instances.iter().find(|(_, i)| i.name == *inst).map(|(_, i)| i.definition);
                let mut sample: Vec<String> = def.and_then(|d| netlist.modules.get(d))
                    .map(|m| m.attributes.keys()
                        .filter(|k| k.starts_with(INTERFACE_CONSTRAINT_ATTR_PREFIX))
                        .cloned().collect())
                    .unwrap_or_default();
                sample.sort();
                eprintln!("missing {}::{}. {} const attrs present, e.g.:", inst, key, sample.len());
                for s in sample.iter().take(12) { eprintln!("   {}", s); }
                std::process::exit(1);
            }
        }
    }

    // ── 5. Real usage through the FULL pipeline ──────────────────
    // A board that *imports* the SDRAM stdlib entity, synthesized via
    // synthesize_from_source (parametric → abstract → parse → analyze
    // → generate, including the Phase 4.5 expansion interpreter). This
    // is how a user actually consumes the part. All six datasheet
    // support components — including the ZQ calibration resistor, VPP
    // pump decoupling, and VREFCA bypass that the conditional gating
    // previously suppressed — must materialise.
    let board_src = std::fs::read_to_string("tests/circuits/realistic/ddr4_board.bhdl")
        .or_else(|_| std::fs::read_to_string("../tests/circuits/realistic/ddr4_board.bhdl"))
        .unwrap_or_else(|e| fail(&format!("read ddr4_board.bhdl: {}", e)));
    let (_t, full_nl) = match synthesize_from_source(&board_src).await {
        Ok(x) => x,
        Err(e) => fail(&format!("full-pipeline synthesis failed: {}", e)),
    };
    let inst_names: Vec<String> = full_nl.instances.iter().map(|(_, i)| i.name.clone()).collect();
    let want_children = [
        "u1_R_zq",    // 240Ω ZQ calibration — was suppressed pre-fix
        "u1_C_vpp",   // VPP pump decoupling — was suppressed pre-fix
        "u1_C_vref",  // VREFCA bypass — was suppressed pre-fix
        "u1_C_vdd",   // core decoupling
        "u1_C_bulk",  // bulk reservoir
        "u1_C_vddq",  // I/O decoupling
    ];
    for c in want_children.iter() {
        if !inst_names.iter().any(|n| n == c) {
            let mut sorted = inst_names.clone(); sorted.sort();
            fail(&format!("imported full-pipeline missing expansion child `{}`. Instances: {:?}", c, sorted));
        }
    }
    // Definition-template stubs (`Res: Res`) are a DELIBERATE
    // analyzer artifact — but each must be template-marked and
    // unconnected (is_template_stub) so every consumer (elaborate,
    // freeze, powertree, safety) can filter it. A bare Cap/Res that
    // is NOT a marked stub is a leak.
    let unmarked_leak = full_nl.instances.iter().find(|(id, i)| {
        (i.name == "Cap" || i.name == "Res")
            && !bhdl_synthesizer::is_template_stub(&full_nl, *id)
    });
    if let Some((_, i)) = unmarked_leak {
        let mut sorted = inst_names.clone(); sorted.sort();
        fail(&format!("imported full-pipeline leaked `{}` as a real (non-template-stub) instance. Instances: {:?}", i.name, sorted));
    }
    println!("✓ imported SDRAM through full synthesize_from_source: all 6 datasheet support components materialise (ZQ + VPP + VREFCA + 3× decoupling), no leaked passives");

    println!("\n✓ DDR4 stdlib validation: PASS");
}
