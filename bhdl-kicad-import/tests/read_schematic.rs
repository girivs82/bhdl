//! Integration tests for [`bhdl_kicad_import::read_from_str`].
//!
//! Fixtures are inline raw-string KiCad schematics — small enough
//! to read in the test, large enough to exercise real constructs.

use bhdl_kicad_import::*;
use std::path::PathBuf;

/// The smallest valid KiCad 6+ schematic: one resistor with two
/// pins, no nets, no labels. Exercises: header parsing, version
/// detection, lib_symbol with pin definitions, one schematic
/// symbol instance with properties (Reference + Value), pin
/// position computation.
#[test]
fn reads_one_resistor_schematic() {
    let src = r#"
        (kicad_sch
            (version 20231120)
            (generator eeschema)
            (uuid 11111111-1111-1111-1111-111111111111)
            (paper "A4")
            (lib_symbols
                (symbol "Device:R"
                    (pin passive line (at 0 3.81 270) (length 1.27)
                        (name "~" (effects (font (size 1.27 1.27))))
                        (number "1" (effects (font (size 1.27 1.27)))))
                    (pin passive line (at 0 -3.81 90) (length 1.27)
                        (name "~" (effects (font (size 1.27 1.27))))
                        (number "2" (effects (font (size 1.27 1.27)))))
                )
            )
            (symbol
                (lib_id "Device:R")
                (at 100 50 0)
                (unit 1)
                (in_bom yes)
                (on_board yes)
                (uuid 22222222-2222-2222-2222-222222222222)
                (property "Reference" "R1" (at 102 48 0))
                (property "Value" "10k" (at 102 52 0))
                (property "Footprint" "Resistor_SMD:R_0603_1608Metric" (at 100 50 0) (effects (font (size 1.27 1.27)) hide))
                (property "Datasheet" "~" (at 100 50 0) (effects (font (size 1.27 1.27)) hide))
            )
        )
    "#;
    let sheet = read_from_str(src, PathBuf::from("test.kicad_sch")).expect("parse");
    assert_eq!(sheet.lib_symbols.len(), 1);
    assert_eq!(sheet.lib_symbols[0].lib_id, "Device:R");
    assert_eq!(sheet.lib_symbols[0].pins.len(), 2);

    assert_eq!(sheet.symbols.len(), 1);
    let r1 = &sheet.symbols[0];
    assert_eq!(r1.lib_id, "Device:R");
    assert_eq!(r1.reference(), Some("R1"));
    assert_eq!(r1.value(), Some("10k"));
    assert_eq!(r1.footprint(), Some("Resistor_SMD:R_0603_1608Metric"));
    assert_eq!(r1.pin_positions.len(), 2);
    // Pin 1 is at relative (0, 3.81); symbol at (100, 50) with rot 0
    // → absolute (100, 53.81). Pin 2 at relative (0, -3.81) → (100, 46.19).
    let p1 = r1.pin_positions.iter().find(|p| p.pin_number == "1").unwrap();
    assert!((p1.at.0 - 100.0).abs() < 1e-6);
    assert!((p1.at.1 - 53.81).abs() < 1e-6);
}

/// Two resistors connected by a wire. Tests wire parsing.
#[test]
fn reads_wire_between_resistors() {
    let src = r#"
        (kicad_sch
            (version 20231120)
            (generator eeschema)
            (uuid abcd1234-0000-0000-0000-000000000001)
            (paper "A4")
            (lib_symbols
                (symbol "Device:R"
                    (pin passive line (at 0 3.81 270) (length 1.27)
                        (name "~") (number "1"))
                    (pin passive line (at 0 -3.81 90) (length 1.27)
                        (name "~") (number "2"))))
            (wire (pts (xy 100 50) (xy 110 50)) (uuid wire-uuid-1))
            (symbol
                (lib_id "Device:R")
                (at 100 50 0)
                (unit 1)
                (uuid aaaa-aaaa)
                (property "Reference" "R1" (at 102 48 0))
                (property "Value" "1k" (at 102 52 0)))
            (symbol
                (lib_id "Device:R")
                (at 110 50 0)
                (unit 1)
                (uuid bbbb-bbbb)
                (property "Reference" "R2" (at 112 48 0))
                (property "Value" "2k" (at 112 52 0)))
        )
    "#;
    let sheet = read_from_str(src, PathBuf::from("test.kicad_sch")).expect("parse");
    assert_eq!(sheet.symbols.len(), 2);
    assert_eq!(sheet.wires.len(), 1);
    assert_eq!(sheet.wires[0].start, (100.0, 50.0));
    assert_eq!(sheet.wires[0].end,   (110.0, 50.0));
}

/// Power flags are reclassified — they're schematic-level symbol
/// instances from the `power` library that we project to
/// [`PowerSymbol`] instead of [`SchematicSymbol`].
#[test]
fn classifies_power_flags() {
    let src = r##"
        (kicad_sch
            (version 20231120)
            (generator eeschema)
            (uuid pwr-uuid)
            (paper "A4")
            (lib_symbols
                (symbol "power:+5V"
                    (pin power_in line (at 0 0 90) (length 0)
                        (name "+5V") (number "1")))
                (symbol "power:GND"
                    (pin power_in line (at 0 0 270) (length 0)
                        (name "GND") (number "1")))
                (symbol "power:+3V3"
                    (pin power_in line (at 0 0 90) (length 0)
                        (name "+3V3") (number "1"))))
            (symbol
                (lib_id "power:+5V")
                (at 50 25 0)
                (unit 1)
                (uuid p1)
                (property "Reference" "#PWR1" (at 50 23 0))
                (property "Value" "+5V" (at 50 27 0)))
            (symbol
                (lib_id "power:GND")
                (at 50 75 0)
                (unit 1)
                (uuid p2)
                (property "Reference" "#PWR2" (at 50 77 0))
                (property "Value" "GND" (at 50 73 0)))
            (symbol
                (lib_id "power:+3V3")
                (at 75 25 0)
                (unit 1)
                (uuid p3)
                (property "Reference" "#PWR3" (at 75 23 0))
                (property "Value" "+3V3" (at 75 27 0)))
        )
    "##;
    let sheet = read_from_str(src, PathBuf::from("test.kicad_sch")).expect("parse");
    assert_eq!(sheet.symbols.len(), 0, "power flags should not be in symbols");
    assert_eq!(sheet.power_symbols.len(), 3);
    let v5 = sheet.power_symbols.iter().find(|p| p.label == "+5V").expect("+5V");
    assert_eq!(v5.category, PowerCategory::Power);
    assert_eq!(v5.voltage, Some(5.0));
    let gnd = sheet.power_symbols.iter().find(|p| p.label == "GND").expect("GND");
    assert_eq!(gnd.category, PowerCategory::Ground);
    let v3v3 = sheet.power_symbols.iter().find(|p| p.label == "+3V3").expect("+3V3");
    assert_eq!(v3v3.category, PowerCategory::Power);
    assert_eq!(v3v3.voltage, Some(3.3));
}

/// Labels (local, global, hierarchical) all parse and end up in
/// their respective vecs.
#[test]
fn reads_labels_and_junctions() {
    let src = r#"
        (kicad_sch
            (version 20231120)
            (generator eeschema)
            (uuid lbl-uuid)
            (paper "A4")
            (label "DATA_CLK" (at 50 25 0) (uuid l1))
            (global_label "RESET" (shape bidirectional) (at 60 30 0) (uuid g1))
            (hierarchical_label "PWR_GOOD" (shape output) (at 70 35 0) (uuid h1))
            (junction (at 55 25 0) (uuid j1))
            (no_connect (at 65 30 0) (uuid nc1))
        )
    "#;
    let sheet = read_from_str(src, PathBuf::from("test.kicad_sch")).expect("parse");
    assert_eq!(sheet.labels.len(), 1);
    assert_eq!(sheet.labels[0].text, "DATA_CLK");
    assert_eq!(sheet.global_labels.len(), 1);
    assert_eq!(sheet.global_labels[0].text, "RESET");
    assert_eq!(sheet.global_labels[0].shape, GlobalLabelShape::Bidirectional);
    assert_eq!(sheet.hierarchical_labels.len(), 1);
    assert_eq!(sheet.hierarchical_labels[0].text, "PWR_GOOD");
    assert_eq!(sheet.hierarchical_labels[0].shape, GlobalLabelShape::Output);
    assert_eq!(sheet.junctions.len(), 1);
    assert_eq!(sheet.no_connects.len(), 1);
}

/// A sheet-reference (hierarchical sheet symbol on the parent).
/// Tests the `(sheet ...)` block with its property fields (Sheetname,
/// Sheetfile) plus sheet pins.
#[test]
fn reads_hierarchical_sheet_reference() {
    let src = r#"
        (kicad_sch
            (version 20231120)
            (generator eeschema)
            (uuid root-uuid)
            (paper "A4")
            (sheet
                (at 100 50)
                (size 40 30)
                (uuid child-uuid)
                (property "Sheetname" "Power_Supply" (at 100 48 0))
                (property "Sheetfile" "power.kicad_sch" (at 100 52 0))
                (pin "VIN" input (at 100 60 180) (uuid pin1))
                (pin "VOUT" output (at 140 60 0) (uuid pin2))
                (pin "GND" passive (at 100 70 180) (uuid pin3))
            )
        )
    "#;
    let sheet = read_from_str(src, PathBuf::from("root.kicad_sch")).expect("parse");
    assert_eq!(sheet.sheet_refs.len(), 1);
    let sr = &sheet.sheet_refs[0];
    assert_eq!(sr.name, "Power_Supply");
    assert_eq!(sr.file_path, PathBuf::from("power.kicad_sch"));
    assert_eq!(sr.size, (40.0, 30.0));
    assert_eq!(sr.pins.len(), 3);
    assert_eq!(sr.pins[0].name, "VIN");
    assert_eq!(sr.pins[0].shape, GlobalLabelShape::Input);
    assert_eq!(sr.pins[1].name, "VOUT");
    assert_eq!(sr.pins[1].shape, GlobalLabelShape::Output);
}

/// DNP (do-not-populate) flag — KiCad 7+ first-class marker on
/// schematic symbols.
#[test]
fn detects_dnp_flag() {
    let src = r#"
        (kicad_sch
            (version 20231120)
            (generator eeschema)
            (uuid dnp-test)
            (paper "A4")
            (lib_symbols
                (symbol "Device:R"
                    (pin passive line (at 0 3.81 270) (length 1.27)
                        (name "~") (number "1"))
                    (pin passive line (at 0 -3.81 90) (length 1.27)
                        (name "~") (number "2"))))
            (symbol
                (lib_id "Device:R")
                (at 100 50 0)
                (unit 1)
                (in_bom yes)
                (on_board yes)
                (dnp yes)
                (uuid r1-uuid)
                (property "Reference" "R1" (at 102 48 0))
                (property "Value" "10k" (at 102 52 0)))
        )
    "#;
    let sheet = read_from_str(src, PathBuf::from("test.kicad_sch")).expect("parse");
    assert_eq!(sheet.symbols.len(), 1);
    assert!(sheet.symbols[0].dnp);
}

/// Title block extraction.
#[test]
fn reads_title_block() {
    let src = r#"
        (kicad_sch
            (version 20231120)
            (generator eeschema)
            (uuid tb-uuid)
            (paper "A4")
            (title_block
                (title "Arduino Uno R3")
                (date "2024-01-15")
                (rev "3")
                (company "Arduino LLC")
                (comment 1 "Open Hardware design")
                (comment 2 "https://www.arduino.cc")
            )
        )
    "#;
    let sheet = read_from_str(src, PathBuf::from("test.kicad_sch")).expect("parse");
    let tb = sheet.title_block.expect("title block");
    assert_eq!(tb.title, Some("Arduino Uno R3".into()));
    assert_eq!(tb.date, Some("2024-01-15".into()));
    assert_eq!(tb.rev, Some("3".into()));
    assert_eq!(tb.company, Some("Arduino LLC".into()));
    assert_eq!(tb.comments.len(), 2);
}

/// A board that's missing a `kicad_sch` top wrapper is not a
/// schematic — should error out clearly.
#[test]
fn errors_on_non_schematic_input() {
    let src = "(some_other_thing (version 1))";
    let result = read_from_str(src, PathBuf::from("test.kicad_sch"));
    assert!(matches!(result, Err(ReadError::NotASchematic)));
}

/// Old KiCad 5 schematics (version < 20211014) are explicitly
/// unsupported with a clear error.
#[test]
fn errors_on_kicad_5_format_version() {
    let src = r#"
        (kicad_sch
            (version 20200214)
            (generator eeschema)
            (paper "A4"))
    "#;
    let result = read_from_str(src, PathBuf::from("test.kicad_sch"));
    assert!(matches!(result, Err(ReadError::UnsupportedVersion { version: 20200214 })));
}
