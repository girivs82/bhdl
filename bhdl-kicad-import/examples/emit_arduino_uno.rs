//! Diagnostic: run the importer against the Arduino UNO fixture
//! and print the emitted BHDL to stdout (plus a warning summary
//! to stderr). Useful for eyeballing the output during stdlib
//! accretion.
//!
//! Run from the repo root:
//!
//!     cargo run -p bhdl-kicad-import --example emit_arduino_uno
//!         > /tmp/arduino_uno.bhdl

use bhdl_kicad_import::{emit_bhdl, read_schematic, MappingRegistry};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/arduino-uno-thru-hole/Arduino UNO.kicad_sch");
    let mapping_src = include_str!("../../bhdl-stdlib/kicad-symbol-mapping.toml");
    let mapping = MappingRegistry::from_toml_str(mapping_src)?;

    let sch = read_schematic(&fixture)?;
    let out = emit_bhdl(&sch, &mapping, "Arduino_UNO")?;

    print!("{}", out.source);

    eprintln!("---");
    eprintln!("{} warnings, {} lines emitted", out.warnings.len(), out.source.lines().count());
    for w in &out.warnings {
        eprintln!("  warn: {}", w);
    }
    Ok(())
}
