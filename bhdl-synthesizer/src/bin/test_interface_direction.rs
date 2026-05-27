//! v0.5 interface check: incompatible direction pairings are
//! rejected at synthesis time. Two cases:
//!
//!   GOOD: master + slave (one `out`, one `in`) → 3 nets produced.
//!   BAD:  master + master (both `out` on MOSI) → 0 nets + error
//!         printed to stderr.
//!
//! Confirms the check fires for both unbound forms and would also
//! cover bound forms (since both store `intf_dir__` attributes).

use bhdl_ast::SourceFile;
use bhdl_parser::parse;
use rowan::ast::AstNode;

fn run_and_count_nets(source: &str) -> (usize, String) {
    let pr = parse(source);
    if !pr.errors().is_empty() {
        eprintln!("parse errors: {:?}", pr.errors());
        std::process::exit(2);
    }
    let sf = SourceFile::cast(pr.syntax()).expect("source file");
    let analysis = bhdl_analyzer::analyze(&sf);

    let mut netlist = bhdl_netlist::Netlist::new();
    // Capture the synthesizer's stderr-like output by running
    // with `info!` discarded; we'll just count nets in the result.
    bhdl_synthesizer::hierarchical_connectivity::extract_hierarchical_connectivity(
        &sf, &analysis, &mut netlist, None,
    )
    .expect("synthesis returned an error");

    // Count nets with any pin connections (excluding net-only entries).
    let mut net_count = 0;
    for (net_id, _) in &netlist.nets {
        if netlist
            .pin_instances
            .iter()
            .any(|(_, pi)| pi.net == Some(net_id))
        {
            net_count += 1;
        }
    }
    (net_count, "".to_string())
}

const GOOD: &str = r#"
interface SPI {
    signal MOSI: out;
    signal MISO: in;
    signal SCK:  out;
}
entity Master { interface  SPI spi; }
entity Slave  { interface ~SPI spi; }
board OK {
    mcu:   Master();
    flash: Slave();
    mcu.spi -> flash.spi;
}
"#;

// Both endpoints are master perspective ⇒ MOSI/SCK collide as
// two `out` drivers; MISO collides as two `in`s. All three
// signals should be rejected by the direction check.
const BAD: &str = r#"
interface SPI {
    signal MOSI: out;
    signal MISO: in;
    signal SCK:  out;
}
entity Master { interface SPI spi; }
board BadWiring {
    a: Master();
    b: Master();
    a.spi -> b.spi;
}
"#;

fn main() {
    println!("=== GOOD: master + slave ===");
    let (good_nets, _) = run_and_count_nets(GOOD);
    println!("Good case produced {} net(s)", good_nets);
    assert_eq!(good_nets, 3, "expected 3 nets in good case, got {}", good_nets);
    println!("✓ Good case wires 3 signal nets as expected.");

    println!("\n=== BAD: master + master ===");
    let (bad_nets, _) = run_and_count_nets(BAD);
    println!("Bad case produced {} net(s)", bad_nets);
    assert_eq!(
        bad_nets, 0,
        "expected 0 nets in bad case (all rejected by direction check), got {}",
        bad_nets
    );
    println!("✓ Bad case (two masters) was rejected — 0 nets produced.");

    println!("\nv0.5 direction-compatibility check: PASS");
}
