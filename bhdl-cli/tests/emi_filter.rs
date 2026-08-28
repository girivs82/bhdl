//! EMI input filter synthesis (spec §7.5 addendum 12):
//! `requirements { emi_filter: "<A>dB[, l=<H>]" }` sizes a damped LC
//! at the slowest bound f_sw, caps from the characterized shortlist
//! (voltage-gated), interposed on a new V_FILT rail, with the
//! Middlebrook interaction machine-checked. Compliance itself is a
//! measurement — the synthesis says so.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn run(reqs: &str, dirname: &str) -> (String, String) {
    let root = workspace_root();
    let dir = std::env::temp_dir().join(dirname);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let board = format!(
        r#"
board EmiDemo {{
    requirements {{ {reqs} decap_lib: "{}/tests/circuits/realistic/decap_lib_fixture.bhdl"; }}
    power VBAT = 3.6V @ 10A;
    port V50: power out = 5V @ 5A;
    ground GND;
    soc: EmiSoc();
    @V50 -> soc.1; soc.GND -> @GND;
}}
entity EmiSoc() {{
    pin 1: power in;
    pin GND: ground;
    domain VDD_MAIN pins="1" v=5V i_nom=200mA i_max=1A source="FIXTURE — EMI probe";
}}
"#,
        root.display()
    );
    let f = dir.join("emi.bhdl");
    std::fs::write(&f, board).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    cmd.current_dir(&dir)
        .env_remove("BHDL_LIB_PATH")
        .args(["-I", root.to_str().unwrap()])
        .arg(&f)
        .args(["powertree", "--input", "VBAT", "--emit", "1"]);
    let out = cmd.output().expect("spawn bhdl-cli");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let emitted = std::fs::read_to_string(&f).unwrap_or_default();
    (text, emitted)
}

#[test]
fn emi_filter_sizes_interposes_and_checks_middlebrook() {
    // default arm: cap picked to clear the customary 6 dB Middlebrook
    // margin BY CONSTRUCTION; filter emitted and the tree re-fed
    let (text, emitted) = run(r#"emi_filter: "40dB";"#, "bhdl_emi_default_test");
    assert!(
        text.contains("emi filter: target 40dB") && text.contains("f_c 100.0kHz"),
        "sizing line wrong:\n{}",
        text.lines().filter(|l| l.contains("emi filter")).collect::<Vec<_>>().join("\n")
    );
    let mb = text
        .lines()
        .find(|l| l.contains("Middlebrook"))
        .unwrap_or_else(|| panic!("no Middlebrook line:\n{text}"));
    assert!(!mb.contains("margin -"), "default pick must clear the margin: {mb}");
    assert!(emitted.contains("power V_FILT"), "no filter rail:\n{emitted}");
    assert!(emitted.contains("l_emi: Ind("), "no filter inductor:\n{emitted}");
    assert!(emitted.contains("@V_FILT -> u_v50.VIN;"), "tree not re-fed from V_FILT:\n{emitted}");
    assert!(
        emitted.contains("r_demi: Res(") && emitted.contains("c_demi: Cap("),
        "damping branch missing:\n{emitted}"
    );
    // compliance is a measurement — the synthesis must say so
    assert!(text.contains("CISPR compliance is a MEASUREMENT"), "measurement hand-off missing");

    // declared-L arm: the designer's inductor, smallest single part
    // covering the needed C (over-attenuation is the safe direction)
    let (text, emitted) = run(r#"emi_filter: "40dB, l=4.7µH";"#, "bhdl_emi_declared_test");
    assert!(
        text.contains("L = 4.70µH (declared") && text.contains("1× DecapMid10u"),
        "declared-L pick wrong:\n{}",
        text.lines().filter(|l| l.contains("emi filter")).collect::<Vec<_>>().join("\n")
    );
    assert!(emitted.contains("Ind(4.70µH"), "declared L not emitted:\n{emitted}");

    // a deliberately hostile declared L: high characteristic impedance
    // → Middlebrook VIOLATED, surfaced as a designer-action finding
    let (text, _) = run(r#"emi_filter: "40dB, l=47µH";"#, "bhdl_emi_violation_test");
    assert!(
        text.contains("EMI: Middlebrook VIOLATED") && text.contains("NOT closable by bulk"),
        "violation not surfaced:\n{}",
        text.lines().filter(|l| l.contains("EMI") || l.contains("Middlebrook")).collect::<Vec<_>>().join("\n")
    );

    // no shortlist: the filter cap must be characterized — stated gap,
    // no filter emitted
    let root = workspace_root();
    let dir = std::env::temp_dir().join("bhdl_emi_nolib_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let board = r#"
board EmiNoLib {
    requirements { emi_filter: "40dB"; }
    power VBAT = 3.6V @ 10A;
    port V50: power out = 5V @ 5A;
    ground GND;
    soc: NlSoc();
    @V50 -> soc.1; soc.GND -> @GND;
}
entity NlSoc() {
    pin 1: power in;
    pin GND: ground;
    domain VDD_MAIN pins="1" v=5V i_nom=200mA i_max=1A source="FIXTURE";
}
"#;
    let f = dir.join("nolib.bhdl");
    std::fs::write(&f, board).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    cmd.current_dir(&dir)
        .env_remove("BHDL_LIB_PATH")
        .args(["-I", root.to_str().unwrap()])
        .arg(&f)
        .args(["powertree", "--input", "VBAT", "--emit", "1"]);
    let out = cmd.output().expect("spawn");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let emitted = std::fs::read_to_string(&f).unwrap_or_default();
    assert!(
        text.contains("NO project decap_lib") && text.contains("filter NOT emitted"),
        "missing-shortlist gap not stated:\n{}",
        text.lines().filter(|l| l.contains("emi")).collect::<Vec<_>>().join("\n")
    );
    assert!(!emitted.contains("l_emi"), "filter emitted without characterized caps:\n{emitted}");
}
