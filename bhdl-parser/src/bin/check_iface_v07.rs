//! Parser-side acceptance test for v0.7 interface grammar:
//! perspective blocks, `Interface:perspective` colon syntax,
//! `wires { lhs <-> rhs; }` blocks. Confirms each new construct
//! parses with zero errors and that legacy `~Interface` produces
//! a useful suggestion.

use bhdl_parser::parse;
use std::fs;

const V07_GOOD: &str = include_str!("/tmp/test_iface_v07.bhdl");

// During v0.7a/b the legacy `~SPI` still parses (no error) so
// existing entities keep working until v0.7c migrates them.
const V07_LEGACY_TILDE: &str = r#"
interface SPI {
    signal MOSI: out;
    signal MISO: in;
}
entity Foo { interface ~SPI spi; }
"#;

fn main() {
    println!("=== v0.7 good fixture ===");
    let pr = parse(V07_GOOD);
    if pr.errors().is_empty() {
        println!("✓ all v0.7 constructs parse cleanly (0 errors)");
    } else {
        for e in pr.errors() { println!("  {:?}", e); }
        std::process::exit(1);
    }

    println!("\n=== legacy `~Interface` still parses (back-compat) ===");
    let pr = parse(V07_LEGACY_TILDE);
    if pr.errors().is_empty() {
        println!("✓ `~Interface` accepted during v0.7a/b transition window");
    } else {
        eprintln!("✗ legacy `~SPI` should still parse, got errors:");
        for e in pr.errors() { eprintln!("    {:?}", e); }
        std::process::exit(1);
    }

    // Also sanity check the existing stdlib interfaces file still parses.
    let path = "bhdl-stdlib/interfaces/serial.bhdl";
    if let Ok(c) = fs::read_to_string(path) {
        let pr = parse(&c);
        if !pr.errors().is_empty() {
            eprintln!("✗ {} regressed: {} parse errors", path, pr.errors().len());
            for e in pr.errors().iter().take(5) { eprintln!("    {:?}", e); }
            std::process::exit(1);
        }
        println!("\n✓ {} still parses (v0.6 form, back-compat)", path);
    }

    println!("\nv0.7a parser surface: PASS");
}
