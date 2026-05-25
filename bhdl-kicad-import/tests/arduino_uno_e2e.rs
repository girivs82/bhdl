//! Phase F: Arduino Uno R3 end-to-end import.
//!
//! Reads the real Arduino-UNO KiCad 8 source from
//! `tests/fixtures/arduino-uno-thru-hole/`, runs the full pipeline
//! (read → resolve → extract → emit → canonical), and asserts the
//! expected structural invariants.
//!
//! This is the milestone test the plan calls for: a real
//! open-source board passing through every stage of the importer
//! without aborting, with a meaningful share of its symbols
//! resolving to real BHDL stdlib entries (the rest falling back
//! to `kicad_passthrough` cleanly).

use bhdl_kicad_import::{
    canonical_from_schematic, emit_bhdl, read_schematic, MappingRegistry,
};
use std::path::Path;

const FIXTURE_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../tests/fixtures/arduino-uno-thru-hole",
);

const STDLIB_REGISTRY_TOML: &str = include_str!(
    "../../bhdl-stdlib/kicad-symbol-mapping.toml"
);

/// Skip the test cleanly if the fixture isn't checked into the
/// repo (it's a 400 KB tree; if a contributor pulls a slim
/// checkout it should still build).
fn fixture_or_skip() -> Option<&'static Path> {
    let root = Path::new(FIXTURE_DIR);
    if root.join("Arduino UNO.kicad_sch").exists() {
        Some(root)
    } else {
        eprintln!("(skipping arduino_uno_e2e: fixture not present at {})",
            root.display());
        None
    }
}

#[test]
fn reads_arduino_uno_schematic_without_aborting() {
    let Some(root) = fixture_or_skip() else { return; };
    let sch = read_schematic(&root.join("Arduino UNO.kicad_sch"))
        .expect("Arduino UNO root sheet must parse");

    // Root sheet should have lots of components.
    let root_syms = sch.root.symbols.len();
    assert!(root_syms >= 30,
        "expected ≥30 symbols on root sheet, got {}", root_syms);

    // Hierarchical children present.
    assert!(!sch.child_sheets.is_empty(),
        "expected hierarchical sub-sheets (Power, Headers, ATMEGA328P-PU)");

    // KiCad 8 generator + modern format version.
    assert_eq!(sch.generator, "eeschema");
    assert!(sch.version >= 20231120,
        "unexpected format version {}", sch.version);

    eprintln!("Arduino UNO: root sheet {} symbols, {} wires, {} child sheets",
        root_syms, sch.root.wires.len(), sch.child_sheets.len());
}

#[test]
fn extracts_canonical_netlist_from_arduino_uno() {
    let Some(root) = fixture_or_skip() else { return; };
    let sch = read_schematic(&root.join("Arduino UNO.kicad_sch")).expect("read");

    let canon = canonical_from_schematic(&sch);

    // A real MCU board has plenty of nets — power rails alone
    // pull in dozens of pins.
    assert!(canon.len() > 20,
        "expected >20 nets, got {}", canon.len());
    assert!(canon.pin_count() > 50,
        "expected >50 total pins, got {}", canon.pin_count());

    // Power nets must exist and be heavily populated. The Uno's
    // GND net is the single most-connected net on the board.
    let gnd = canon.nets.get("GND").expect("GND net present");
    assert!(gnd.len() >= 10,
        "GND should have ≥10 pins, got {}", gnd.len());

    // +5V appears in the Uno schematic via the power flag.
    let v5 = canon.nets.get("VCC_5V").expect("VCC_5V net present");
    assert!(!v5.is_empty(), "VCC_5V should be non-empty");

    eprintln!("Arduino UNO canonical netlist: {} nets, {} pins (GND={}, VCC_5V={})",
        canon.len(), canon.pin_count(), gnd.len(), v5.len());
}

#[test]
fn emits_bhdl_for_arduino_uno() {
    let Some(root) = fixture_or_skip() else { return; };
    let sch = read_schematic(&root.join("Arduino UNO.kicad_sch")).expect("read");

    let mapping = MappingRegistry::from_toml_str(STDLIB_REGISTRY_TOML).expect("registry");
    let emitted = emit_bhdl(&sch, &mapping, "Arduino_UNO").expect("emit");

    // Board block header.
    assert!(emitted.source.contains("board Arduino_UNO {"),
        "missing board header");

    // Hierarchical sub-sheets emitted as entity blocks.
    // We don't assert exact sheet names since the file_stem
    // depends on the Sheetfile property (which the test author
    // can't control), but at least one `entity` should be
    // present given the Uno has 3 sub-sheets.
    assert!(emitted.source.contains("entity "),
        "expected at least one entity block for sub-sheets");

    // Power and ground decls.
    assert!(emitted.source.contains("ground GND;"),
        "missing ground GND decl");
    assert!(emitted.source.contains("power VCC_5V"),
        "missing VCC_5V power decl");

    // Some symbols must resolve to real stdlib entries
    // (resistor, capacitor are universally mapped).
    assert!(emitted.source.contains(": resistor("),
        "expected at least one resistor instance");
    assert!(emitted.source.contains(": capacitor("),
        "expected at least one capacitor instance");

    // Unmapped symbols fall through to kicad_passthrough — the
    // Uno uses an ATmega328, USB receptacle, etc. that aren't
    // (yet) in the stdlib mapping. That's expected.
    let passthroughs = emitted.source.matches(": kicad_passthrough(").count();
    assert!(passthroughs > 0,
        "expected at least one kicad_passthrough for unmapped ICs");

    // Warnings name the unmapped symbols (non-empty unless the
    // stdlib mapping covers literally every Uno part — not
    // currently the case).
    assert!(!emitted.warnings.is_empty(),
        "expected at least one warning for unmapped symbols");

    let lines = emitted.source.lines().count();
    eprintln!(
        "Arduino UNO emitted BHDL: {} lines, {} warnings, {} kicad_passthrough fallbacks",
        lines, emitted.warnings.len(), passthroughs,
    );
}

#[test]
fn round_trips_canonical_self_equivalent() {
    let Some(root) = fixture_or_skip() else { return; };
    let sch = read_schematic(&root.join("Arduino UNO.kicad_sch")).expect("read");

    // Two extractions of the same schematic must produce
    // byte-identical canonical netlists — proves the
    // determinism fix in `nets.rs` holds on real-world data.
    let a = canonical_from_schematic(&sch);
    let b = canonical_from_schematic(&sch);
    let rep = bhdl_kicad_import::compare(&a, &b);
    assert!(rep.is_equivalent(),
        "self-equivalence broke on real Arduino schematic:\n{}\n{:#?}",
        rep.summary(), rep.diffs);
}

#[test]
fn mapping_coverage_report() {
    // Diagnostic test: count how many of the Uno's lib_ids
    // resolve in the stdlib mapping registry. Doesn't fail; just
    // prints a coverage number so we can track stdlib accretion.
    let Some(root) = fixture_or_skip() else { return; };
    let sch = read_schematic(&root.join("Arduino UNO.kicad_sch")).expect("read");
    let mapping = MappingRegistry::from_toml_str(STDLIB_REGISTRY_TOML).expect("registry");

    let mut total = 0usize;
    let mut mapped = 0usize;
    let mut unmapped: std::collections::BTreeMap<String, usize> = Default::default();

    for sheet in std::iter::once(&sch.root).chain(sch.child_sheets.values()) {
        for sym in &sheet.symbols {
            total += 1;
            // Skip power-flag pseudosymbols from the coverage
            // count — they're handled separately and the
            // mapping treats them as `_net:*` not entities.
            if sym.lib_id.starts_with("power:") {
                mapped += 1;
                continue;
            }
            match mapping.lookup(&sym.lib_id) {
                Some(m) if !m.is_power_net() => mapped += 1,
                _ => { *unmapped.entry(sym.lib_id.clone()).or_default() += 1; }
            }
        }
    }
    let coverage_pct = 100.0 * mapped as f64 / total as f64;
    eprintln!("Arduino UNO mapping coverage: {}/{} symbols ({:.1}%)",
        mapped, total, coverage_pct);
    if !unmapped.is_empty() {
        eprintln!("Unmapped lib_ids (counts):");
        for (lib_id, n) in &unmapped {
            eprintln!("  {:4}  {}", n, lib_id);
        }
    }
}
