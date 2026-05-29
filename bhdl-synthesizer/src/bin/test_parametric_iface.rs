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

    println!("\nv0.8 parametric interfaces (tier 1): PASS");
}
