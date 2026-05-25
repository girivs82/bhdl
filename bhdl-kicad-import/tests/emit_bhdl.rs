//! Integration tests for the BHDL emitter — Phase D.
//!
//! Each test reads a small KiCad schematic, runs the full pipeline
//! (read → resolve → extract nets → emit) and checks the emitted
//! BHDL contains the expected structural elements.

use bhdl_kicad_import::{emit_bhdl, read_from_str, MappingRegistry};
use std::path::PathBuf;

const STDLIB_REGISTRY_TOML: &str = include_str!(
    "../../bhdl-stdlib/kicad-symbol-mapping.toml"
);

fn mapping() -> MappingRegistry {
    MappingRegistry::from_toml_str(STDLIB_REGISTRY_TOML).expect("registry parses")
}

/// Wrap a parsed root Sheet into a Schematic so the emitter has
/// the same shape as the real pipeline.
fn schematic_of(sheet: bhdl_kicad_import::Sheet) -> bhdl_kicad_import::Schematic {
    bhdl_kicad_import::Schematic {
        root: sheet,
        child_sheets: std::collections::HashMap::new(),
        version: 20231120,
        generator: "test".into(),
    }
}

const SIMPLE_RC: &str = r#"(kicad_sch
    (version 20231120) (generator eeschema)
    (lib_symbols
      (symbol "Device:R"
        (pin passive line (at 0 3.81 270) (length 1.27) (name "~") (number "1"))
        (pin passive line (at 0 -3.81 90) (length 1.27) (name "~") (number "2")))
      (symbol "Device:C"
        (pin passive line (at 0 3.81 270) (length 1.27) (name "~") (number "1"))
        (pin passive line (at 0 -3.81 90) (length 1.27) (name "~") (number "2"))))
    (symbol (lib_id "Device:R") (at 100 100 0) (unit 1) (in_bom yes) (on_board yes)
      (uuid "11111111-aaaa-bbbb-cccc-000000000001")
      (property "Reference" "R1" (at 0 0 0))
      (property "Value" "10k" (at 0 0 0)))
    (symbol (lib_id "Device:C") (at 100 110 0) (unit 1) (in_bom yes) (on_board yes)
      (uuid "11111111-aaaa-bbbb-cccc-000000000002")
      (property "Reference" "C1" (at 0 0 0))
      (property "Value" "100nF" (at 0 0 0)))
    (wire (pts (xy 100 96.19) (xy 100 113.81)) (uuid "w1"))
    (label "MID" (at 100 105 0) (uuid "l1")))
"#;

#[test]
fn emits_board_block_and_components() {
    let sheet = read_from_str(SIMPLE_RC, PathBuf::from("rc.kicad_sch")).expect("read");
    let sch = schematic_of(sheet);
    let out = emit_bhdl(&sch, &mapping(), "RC_Filter").expect("emit");

    assert_eq!(out.board_name, "RC_Filter");
    assert!(out.source.contains("board RC_Filter {"), "missing board header:\n{}", out.source);
    assert!(out.source.contains("R1: resistor(\"10k\");"),
        "expected R1 declaration:\n{}", out.source);
    assert!(out.source.contains("C1: capacitor(\"100nF\");"),
        "expected C1 declaration:\n{}", out.source);
}

#[test]
fn emits_label_named_net_with_at_prefix() {
    let sheet = read_from_str(SIMPLE_RC, PathBuf::from("rc.kicad_sch")).expect("read");
    let sch = schematic_of(sheet);
    let out = emit_bhdl(&sch, &mapping(), "RC_Filter").expect("emit");

    // Label "MID" sits on the wire — every pin on that net should
    // connect to @MID.
    assert!(out.source.contains("@MID -> R1.") ,
        "@MID should appear:\n{}", out.source);
    assert!(out.source.contains("@MID -> C1."),
        "@MID should reach C1:\n{}", out.source);
}

const POWER_RC: &str = r##"(kicad_sch
    (version 20231120) (generator eeschema)
    (lib_symbols
      (symbol "Device:R"
        (pin passive line (at 0 3.81 270) (length 1.27) (name "~") (number "1"))
        (pin passive line (at 0 -3.81 90) (length 1.27) (name "~") (number "2")))
      (symbol "power:+5V"
        (pin power_in line (at 0 0 90) (length 0) (name "+5V") (number "1")))
      (symbol "power:GND"
        (pin power_in line (at 0 0 270) (length 0) (name "GND") (number "1"))))
    (symbol (lib_id "Device:R") (at 50 50 0) (unit 1) (in_bom yes) (on_board yes)
      (uuid "11111111-aaaa-bbbb-cccc-000000000010")
      (property "Reference" "R1" (at 0 0 0))
      (property "Value" "1k" (at 0 0 0)))
    (symbol (lib_id "power:+5V") (at 50 46.19 0) (unit 1) (in_bom yes) (on_board yes)
      (uuid "11111111-aaaa-bbbb-cccc-000000000011")
      (property "Reference" "#PWR01" (at 0 0 0))
      (property "Value" "+5V" (at 0 0 0)))
    (symbol (lib_id "power:GND") (at 50 53.81 0) (unit 1) (in_bom yes) (on_board yes)
      (uuid "11111111-aaaa-bbbb-cccc-000000000012")
      (property "Reference" "#PWR02" (at 0 0 0))
      (property "Value" "GND" (at 0 0 0))))
"##;

#[test]
fn emits_power_and_ground_decls() {
    let sheet = read_from_str(POWER_RC, PathBuf::from("p.kicad_sch")).expect("read");
    let sch = schematic_of(sheet);
    let out = emit_bhdl(&sch, &mapping(), "PwrBoard").expect("emit");

    // Power flag → `power VCC_5V = 5V;`
    assert!(out.source.contains("power VCC_5V = 5V;"),
        "missing power decl:\n{}", out.source);
    // Ground flag → `ground GND;`
    assert!(out.source.contains("ground GND;"),
        "missing ground decl:\n{}", out.source);
    // Power references use bare name, not @-prefix. Either pin
    // may end up on either rail depending on which way the
    // symbol-relative pin offsets resolve under the IR's transform
    // — both pins should appear, each on a different rail.
    assert!(out.source.contains("VCC_5V -> R1."),
        "missing VCC connection:\n{}", out.source);
    assert!(out.source.contains("GND -> R1."),
        "missing GND connection:\n{}", out.source);
    // The power flags themselves should NOT appear as
    // component instances.
    assert!(!out.source.contains("PWR01: "), "power flag emitted as instance:\n{}", out.source);
}

#[test]
fn unmapped_symbol_falls_back_to_kicad_passthrough() {
    let src = r#"(kicad_sch
        (version 20231120) (generator eeschema)
        (lib_symbols
          (symbol "ObscureLib:Mystery_IC"
            (pin input line (at 0 0 0) (length 1) (name "A") (number "1"))))
        (symbol (lib_id "ObscureLib:Mystery_IC") (at 0 0 0) (unit 1) (in_bom yes) (on_board yes)
          (uuid "11111111-aaaa-bbbb-cccc-000000000099")
          (property "Reference" "U99" (at 0 0 0))
          (property "Value" "MysteryIC" (at 0 0 0))))
    "#;
    let sheet = read_from_str(src, PathBuf::from("m.kicad_sch")).expect("read");
    let sch = schematic_of(sheet);
    let out = emit_bhdl(&sch, &mapping(), "MysteryBoard").expect("emit");
    assert!(out.source.contains("U99: kicad_passthrough"),
        "expected passthrough:\n{}", out.source);
    assert!(!out.warnings.is_empty(), "should have warned about unmapped symbol");
    assert!(out.warnings[0].contains("Mystery_IC"));
}
