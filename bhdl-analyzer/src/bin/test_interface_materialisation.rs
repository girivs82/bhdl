//! v0.2 interface materialisation check: given an entity that
//! declares `interface SPI spi;` and `interface ~SPI slave;`, the
//! analyzer should produce dot-qualified Pin symbols with the
//! correct directions on both sides.

use bhdl_analyzer::analyze;
use bhdl_analyzer::symbol_table::{PortDirectionKind, SymbolKind};
use bhdl_ast::SourceFile;
use bhdl_parser::parse;
use rowan::ast::AstNode;

// Two SPI definitions:
//   SPI_LEGACY — flat v0.6 form, used with `~` for back-compat.
//   SPI_V07    — v0.7 perspective form, used with `:slave`.
// Both sides should materialise the same 4 pins with the same
// directions. This proves the v0.7 path agrees with the legacy path.
const SOURCE: &str = r#"
interface SPI_LEGACY {
    signal MOSI: out;
    signal MISO: in;
    signal SCK:  out;
    signal CS:   out optional;
}
interface SPI_V07 {
    perspective master {
        signal MOSI: out;
        signal MISO: in;
        signal SCK:  out;
        signal CS:   out optional;
    }
    perspective slave {
        signal MOSI: in;
        signal MISO: out;
        signal SCK:  in;
        signal CS:   in optional;
    }
}

entity Master      { interface SPI_LEGACY spi; }
entity Slave       { interface ~SPI_LEGACY spi; }
entity MasterV07   { interface SPI_V07 spi; }
entity SlaveV07    { interface SPI_V07:slave spi; }
"#;

fn fail(msg: &str) -> ! {
    eprintln!("✗ {}", msg);
    std::process::exit(1);
}

fn main() {
    let pr = parse(SOURCE);
    assert!(pr.errors().is_empty(), "parse errors: {:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).expect("source file");
    let result = analyze(&sf);

    // Find all four entity scopes.
    let mut scopes: std::collections::HashMap<&'static str, _> = Default::default();
    for entry in result.scope_registry.iter() {
        if entry.kind != bhdl_analyzer::scope_registry::ScopeKind::Entity { continue; }
        let name = entry.table.scope_name.clone();
        for key in ["Master", "Slave", "MasterV07", "SlaveV07"] {
            if name.as_deref() == Some(key) {
                scopes.insert(key, entry.id);
            }
        }
    }
    for key in ["Master", "Slave", "MasterV07", "SlaveV07"] {
        if !scopes.contains_key(key) {
            fail(&format!("{} entity scope not found", key));
        }
    }

    let probes = [
        // Legacy ~SPI form (v0.6, still accepted):
        ("Master", scopes["Master"], "spi.MOSI", PortDirectionKind::Out),
        ("Master", scopes["Master"], "spi.MISO", PortDirectionKind::In),
        ("Master", scopes["Master"], "spi.SCK",  PortDirectionKind::Out),
        ("Master", scopes["Master"], "spi.CS",   PortDirectionKind::Out),
        ("Slave",  scopes["Slave"],  "spi.MOSI", PortDirectionKind::In),
        ("Slave",  scopes["Slave"],  "spi.MISO", PortDirectionKind::Out),
        ("Slave",  scopes["Slave"],  "spi.SCK",  PortDirectionKind::In),
        ("Slave",  scopes["Slave"],  "spi.CS",   PortDirectionKind::In),
        // v0.7 explicit-perspective form:
        ("MasterV07", scopes["MasterV07"], "spi.MOSI", PortDirectionKind::Out),
        ("MasterV07", scopes["MasterV07"], "spi.MISO", PortDirectionKind::In),
        ("MasterV07", scopes["MasterV07"], "spi.SCK",  PortDirectionKind::Out),
        ("MasterV07", scopes["MasterV07"], "spi.CS",   PortDirectionKind::Out),
        ("SlaveV07",  scopes["SlaveV07"],  "spi.MOSI", PortDirectionKind::In),
        ("SlaveV07",  scopes["SlaveV07"],  "spi.MISO", PortDirectionKind::Out),
        ("SlaveV07",  scopes["SlaveV07"],  "spi.SCK",  PortDirectionKind::In),
        ("SlaveV07",  scopes["SlaveV07"],  "spi.CS",   PortDirectionKind::In),
    ];

    for (entity, scope_id, pin_name, expected) in probes {
        let scope = result.scope_registry.get(scope_id);
        let sym = match scope.table.lookup(pin_name) {
            Some(s) => s,
            None => fail(&format!("{}::{} not found", entity, pin_name)),
        };
        if sym.kind != SymbolKind::Pin {
            fail(&format!("{}::{} should be Pin, got {:?}", entity, pin_name, sym.kind));
        }
        if sym.direction != Some(expected) {
            fail(&format!(
                "{}::{} direction = {:?}, expected {:?}",
                entity, pin_name, sym.direction, expected
            ));
        }
        println!("✓ {} :: {} ({:?})", entity, pin_name, expected);
    }

    println!("\n{} pins materialised correctly across master + slave.", probes.len());
}
