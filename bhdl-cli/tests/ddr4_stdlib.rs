//! The DDR4 interface stack, re-landed. bhdl-stdlib/interfaces/
//! ddr4.bhdl was deleted in the stdlib consolidation (bfaa4eda) and
//! never restored, so DDR4_SDRAM_x8's `interface DDR4Data dat;` /
//! `interface DDR4Ca:sdram ca;` fields silently resolved no
//! definition. The file is back, imported by the entity (imports are
//! transitive), and BOTH import paths now run the parametric
//! pre-parse rewrite — the reason the file "failed to parse" through
//! imports and got dropped in the first place.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn run(args: &[&str]) -> String {
    let root = workspace_root();
    let mut c = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    c.current_dir(&root).arg("-I").arg(&root);
    for a in args {
        c.arg(a);
    }
    let out = c.output().expect("spawn");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

#[test]
fn sdram_interface_fields_materialize_pins() {
    let text = run(&["tests/circuits/realistic/ddr4_board.bhdl", "synthesize"]);
    // the import chain is clean — no raw-parse spam from the
    // parametric interface template
    assert!(!text.contains("Error loading import"), "interface import failed:\n{}",
        text.lines().filter(|l| l.contains("import")).take(6).collect::<Vec<_>>().join("\n"));
    assert!(!text.contains("Parse errors in"), "parametric preprocess missing on an import path");
    // dat.* / ca.* leaves exist with real perspective directions
    // (ca is the sdram side → inputs; ERC006 flags the unwired bus)
    assert!(text.contains("u1.ca.CK_t (input)"), "ca.CK_t not materialized as an sdram-side input:\n{}",
        text.lines().filter(|l| l.contains("ca.CK_t")).take(4).collect::<Vec<_>>().join("\n"));
    assert!(text.contains("dat.DQ0") || text.contains("u1.dat.DQ0"), "dat.DQ0 leaf missing");
    // the datasheet expansion still lands
    assert!(text.contains("u1_R_zq"), "ZQ calibration resistor missing");
}

#[test]
fn frozen_record_carries_no_template_stubs() {
    // The analyzer registers entity DEFINITIONS as instance-like
    // symbols, minting a `Res: Res` template stub — marked and
    // filtered everywhere. The as-fabbed freeze record must not
    // carry the phantom part.
    let root = workspace_root();
    let _ = run(&["--no-elaborate", "tests/circuits/realistic/ddr4_board.bhdl", "freeze"]);
    let frozen = root.join("tests/circuits/realistic/ddr4_board.frozen.json");
    let j = std::fs::read_to_string(&frozen).expect("frozen record");
    let _ = std::fs::remove_file(&frozen);
    assert!(j.contains("u1_R_zq"), "real part missing from freeze");
    // "Res" appears legitimately as rt0's component TYPE — the stub
    // manifests as an instance whose NAME/refdes equals the type.
    let v: serde_json::Value = serde_json::from_str(&j).expect("frozen json");
    let comps = v["components"]
        .as_array()
        .or_else(|| v["instances"].as_array())
        .expect("component list");
    let names: Vec<&str> = comps
        .iter()
        .filter_map(|c| c["refdes"].as_str().or_else(|| c["name"].as_str()))
        .collect();
    assert!(
        !names.contains(&"Res") && !names.contains(&"Cap"),
        "definition-template stub leaked into the as-fabbed record: {names:?}"
    );
}
