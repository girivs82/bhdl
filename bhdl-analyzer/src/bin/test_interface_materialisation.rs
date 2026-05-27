//! v0.2 interface materialisation check: given an entity that
//! declares `interface SPI spi;` and `interface ~SPI slave;`, the
//! analyzer should produce dot-qualified Pin symbols with the
//! correct directions on both sides.

use bhdl_analyzer::analyze;
use bhdl_analyzer::symbol_table::{PortDirectionKind, SymbolKind};
use bhdl_ast::SourceFile;
use bhdl_parser::parse;
use rowan::ast::AstNode;

const SOURCE: &str = r#"
interface SPI {
    signal MOSI: out;
    signal MISO: in;
    signal SCK:  out;
    signal CS:   out optional;
}

entity Master {
    interface SPI spi;
}

entity Slave {
    interface ~SPI spi;
}
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

    // Find both entity scopes.
    let mut master_scope_id = None;
    let mut slave_scope_id = None;
    for entry in result.scope_registry.iter() {
        if entry.kind != bhdl_analyzer::scope_registry::ScopeKind::Entity { continue; }
        let name = entry.table.scope_name.clone();
        match name.as_deref() {
            Some("Master") => master_scope_id = Some(entry.id),
            Some("Slave")  => slave_scope_id  = Some(entry.id),
            _ => {}
        }
    }
    let master_id = master_scope_id.unwrap_or_else(|| fail("Master entity scope not found"));
    let slave_id  = slave_scope_id.unwrap_or_else(|| fail("Slave entity scope not found"));

    let probes = [
        ("Master", master_id, "spi.MOSI", PortDirectionKind::Out),
        ("Master", master_id, "spi.MISO", PortDirectionKind::In),
        ("Master", master_id, "spi.SCK",  PortDirectionKind::Out),
        ("Master", master_id, "spi.CS",   PortDirectionKind::Out),
        // ~SPI flips out↔in:
        ("Slave",  slave_id, "spi.MOSI", PortDirectionKind::In),
        ("Slave",  slave_id, "spi.MISO", PortDirectionKind::Out),
        ("Slave",  slave_id, "spi.SCK",  PortDirectionKind::In),
        ("Slave",  slave_id, "spi.CS",   PortDirectionKind::In),
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
