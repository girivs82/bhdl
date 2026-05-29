//! v0.8 parametric interfaces — preprocessor + end-to-end synth test.
//!
//! Three cases exercise tier-1 features:
//!   1. QSPI: SPI<lanes=4> → IO0..IO3 expanded; bundle bind fans out.
//!   2. OSPI: SPI<lanes=8> reuses the same template, distinct mangled name.
//!   3. Default form: bare `interface SPI flash;` uses lanes=1.

use bhdl_synthesizer::parametric_resolver::preprocess;

const SOURCE: &str = r#"
interface SPI<lanes: int = 1> {
    perspective master { signal SCK: out; signal CS: out; signal IO<lanes>: inout; }
    perspective slave  { signal SCK: in;  signal CS: in;  signal IO<lanes>: inout; }
}

entity QSpiFlash {
    interface SPI<lanes=4>:slave qspi;
}

entity OSpiFlash {
    interface SPI<lanes=8>:slave ospi;
}

entity LegacyFlash {
    interface SPI:slave spi;
}
"#;

fn fail(msg: &str) -> ! {
    eprintln!("✗ {}", msg);
    std::process::exit(1);
}

fn main() {
    let rewritten = match preprocess(SOURCE) {
        Ok(s) => s,
        Err(e) => fail(&format!("preprocess error: {}", e)),
    };

    println!("=== rewritten source ===");
    println!("{}", rewritten);
    println!("=== /rewritten ===");

    // Template should be deleted.
    if rewritten.contains("SPI<lanes: int") {
        fail("template `interface SPI<lanes: int = 1>` was not deleted");
    }

    // Three distinct monomorphisations should exist.
    let want_specs = ["SPI__lanes_1", "SPI__lanes_4", "SPI__lanes_8"];
    for name in want_specs.iter() {
        if !rewritten.contains(&format!("interface {}", name)) {
            fail(&format!("missing monomorphisation `interface {}`", name));
        } else {
            println!("✓ monomorphisation present: interface {}", name);
        }
    }

    // Use sites should be rewritten to mangled names.
    let want_uses = [
        ("SPI__lanes_4:slave qspi", "QSpiFlash use site"),
        ("SPI__lanes_8:slave ospi", "OSpiFlash use site"),
        ("SPI__lanes_1:slave spi",  "LegacyFlash defaults use site"),
    ];
    for (needle, label) in want_uses.iter() {
        if !rewritten.contains(needle) {
            fail(&format!("missing rewrite for {}: expected `{}`", label, needle));
        } else {
            println!("✓ {}: `{}`", label, needle);
        }
    }

    // Signal-array expansion: QSPI body must contain IO0..IO3, not IO<4>.
    let qspi_idx = rewritten.find("interface SPI__lanes_4 {").unwrap();
    let qspi_body = &rewritten[qspi_idx..];
    let qspi_end = qspi_body.find("\n}").map(|e| e + 2).unwrap_or(qspi_body.len());
    let qspi_block = &qspi_body[..qspi_end];
    for io in &["IO0", "IO1", "IO2", "IO3"] {
        if !qspi_block.contains(io) {
            fail(&format!("QSPI body missing expanded signal `{}`", io));
        }
    }
    if qspi_block.contains("<4>") || qspi_block.contains("<lanes>") {
        fail("QSPI body still contains unexpanded `<...>` width annotation");
    }
    println!("✓ QSPI body expands IO<4> → IO0..IO3");

    // OSPI body: IO0..IO7.
    let ospi_idx = rewritten.find("interface SPI__lanes_8 {").unwrap();
    let ospi_block = &rewritten[ospi_idx..];
    for io in &["IO0", "IO1", "IO2", "IO3", "IO4", "IO5", "IO6", "IO7"] {
        if !ospi_block.contains(io) {
            fail(&format!("OSPI body missing expanded signal `{}`", io));
        }
    }
    println!("✓ OSPI body expands IO<8> → IO0..IO7");

    // Now parse the rewritten source as a final check: the parser
    // should accept the monomorphised output cleanly.
    let pr = bhdl_parser::parse(&rewritten);
    if !pr.errors().is_empty() {
        eprintln!("rewritten source has parse errors:");
        for e in pr.errors().iter().take(10) {
            eprintln!("  {:?}", e);
        }
        std::process::exit(1);
    }
    println!("✓ rewritten source parses cleanly");

    // ---------- tier 2: generative loops ----------
    println!("\n=== tier 2: generate loops ===");
    let tier2_src = r#"
interface DiffPair {
    signal P: inout;
    signal N: inout;
}
interface DDR4ByteLane {
    signal DM: inout;
    interface DiffPair DQS;
}
interface DDR4<byte_lanes: int = 8> {
    signal A0: out;
    interface DiffPair CK;
    generate for i in 0..<byte_lanes> {
        interface DDR4ByteLane lane<i>;
    }
}

entity MemController {
    interface DDR4<byte_lanes=2> ddr;
}

entity SmallMem {
    interface DDR4<byte_lanes=4> ddr;
}
"#;
    let rewritten2 = preprocess(tier2_src).unwrap_or_else(|e| fail(&format!("preprocess: {}", e)));
    println!("=== rewritten ===\n{}", rewritten2);

    // The DDR4__byte_lanes_2 monomorphisation should declare lane0 and lane1.
    let mc_idx = rewritten2.find("interface DDR4__byte_lanes_2 {").unwrap_or_else(|| {
        fail("missing DDR4__byte_lanes_2 monomorphisation")
    });
    let mc_block = &rewritten2[mc_idx..];
    let mc_end = mc_block.find("\n}").map(|e| e + 2).unwrap_or(mc_block.len());
    let mc_block = &mc_block[..mc_end];
    for needed in &[
        "interface DDR4ByteLane lane0;",
        "interface DDR4ByteLane lane1;",
    ] {
        if !mc_block.contains(needed) {
            fail(&format!("DDR4__byte_lanes_2 missing `{}`", needed));
        }
    }
    if mc_block.contains("lane2") {
        fail("DDR4__byte_lanes_2 leaked an extra `lane2;` row");
    }
    if mc_block.contains("generate for") {
        fail("DDR4__byte_lanes_2 still contains an unexpanded `generate for` block");
    }
    println!("✓ DDR4<byte_lanes=2> unrolled to lane0, lane1 (no lane2, no generate residue)");

    let sm_idx = rewritten2.find("interface DDR4__byte_lanes_4 {").unwrap();
    let sm_block = &rewritten2[sm_idx..];
    let sm_end = sm_block.find("\n}").map(|e| e + 2).unwrap_or(sm_block.len());
    let sm_block = &sm_block[..sm_end];
    for k in 0..4 {
        let needle = format!("interface DDR4ByteLane lane{};", k);
        if !sm_block.contains(&needle) {
            fail(&format!("DDR4__byte_lanes_4 missing `{}`", needle));
        }
    }
    println!("✓ DDR4<byte_lanes=4> unrolled to lane0..lane3");

    // Sanity: the rewritten tier-2 source should still parse cleanly.
    let pr2 = bhdl_parser::parse(&rewritten2);
    if !pr2.errors().is_empty() {
        eprintln!("tier-2 rewritten source has parse errors:");
        for e in pr2.errors().iter().take(10) { eprintln!("  {:?}", e); }
        std::process::exit(1);
    }
    println!("✓ tier-2 rewritten source parses cleanly");

    println!("\nv0.8 parametric interfaces (tier 1 + tier 2): PASS");
}
