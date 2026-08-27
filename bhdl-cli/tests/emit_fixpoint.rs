//! The --emit convergence loop (spec §7.4): stages + auto-decouple +
//! synthesized sequencing chains + bulk sized by the power-up /
//! interaction fixpoint, written to disk once, converged. Findings not
//! closable by capacitance stay OPEN and say so (designer action) —
//! never silently absorbed.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

#[test]
fn emit_converges_chains_and_bulk() {
    let root = workspace_root();
    let dir = std::env::temp_dir().join("bhdl_emit_fixpoint_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let board = r#"
entity LoopSoc() {
    pin 1: power in;
    pin 2: power in;
    pin GND: ground;
    domain VDD_MAIN pins="1" v=5V i_nom=200mA i_max=1A slot=1 step=4.5A rise=10us dur=200us droop_max=2% source="FIXTURE — loop probe";
    domain VDD_AUX pins="2" v=3.3V i_nom=100mA i_max=0.3A slot=2 source="FIXTURE — loop probe";
}
board AutoLoop {
    power VBAT = 3.6V @ 10A;
    port V50: power out = 5V @ 5A;
    port V33: power out = 3.3V @ 1A;
    ground GND;
    soc: LoopSoc();
    @V50 -> soc.1; @V33 -> soc.2; soc.GND -> @GND;
}
"#;
    let f = dir.join("autoloop.bhdl");
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
    assert!(text.contains("chain wire(s) synthesized"), "no chains:\n{}", text.lines().rev().take(20).collect::<Vec<_>>().join("\n"));
    assert!(text.contains("fixpoint iteration"), "fixpoint never ran:\n{}", text.lines().rev().take(20).collect::<Vec<_>>().join("\n"));
    assert!(text.contains("re-verified through the full pipeline"), "not verified:\n{}", text.lines().rev().take(20).collect::<Vec<_>>().join("\n"));
    let emitted = std::fs::read_to_string(&f).unwrap();
    assert!(emitted.contains("seqbulk_v50"), "no bulk emitted:\n{emitted}");
    assert!(emitted.contains("seqr_u_v33") || emitted.contains(".PG -> "), "no chain emitted:\n{emitted}");
    // the Generic placeholder's unverifiable threshold stays an OPEN,
    // stated finding — never silently absorbed
    assert!(text.contains("NOT closable by bulk"), "the open finding was absorbed:\n{}", text.lines().rev().take(20).collect::<Vec<_>>().join("\n"));
}

/// The envelope-aware search (§7.5): the fixpoint stays INSIDE the
/// stage's datasheet stability envelope — and when the droop still
/// fails at the clamped ceiling, the feasible interval is provably
/// EMPTY and the finding says so with both numbers and the remedies.
#[test]
fn emit_fixpoint_empty_interval_is_named() {
    let root = workspace_root();
    let dir = std::env::temp_dir().join("bhdl_emit_empty_interval_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // the 900 µs burst needs more bulk than the TPS61022's 1000 µF
    // effective envelope admits
    let board = r#"
entity BigSoc() {
    pin 1: power in;
    pin GND: ground;
    domain VDD_MAIN pins="1" v=5V i_nom=200mA i_max=1A step=4.5A rise=10us dur=900us droop_max=2% source="FIXTURE — empty-interval probe";
}
board BigBoard {
    power VBAT = 3.6V @ 10A;
    port V50: power out = 5V @ 5A;
    ground GND;
    soc: BigSoc();
    @V50 -> soc.1; soc.GND -> @GND;
}
"#;
    let f = dir.join("big.bhdl");
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
    assert!(
        text.contains("feasible interval is EMPTY") && text.contains("stability envelope caps the rail"),
        "empty interval not named:\n{}",
        text.lines().rev().take(20).collect::<Vec<_>>().join("\n")
    );
    // and the search never exceeded the clamp (no runaway bulk value)
    assert!(!text.contains("1408µF") && !text.contains("2816µF"), "search escaped the envelope:\n{text}");
}

/// Shortlist bulk (§7.5 addendum 4): with a project `decap_lib`, the
/// fixpoint's bulk is STACKED FROM the characterized library — every
/// capacitor on the rail becomes a shortlisted, orderable, curve-aware
/// part — instead of a bare farad value.
#[test]
fn emit_bulk_from_characterized_shortlist() {
    let root = workspace_root();
    let dir = std::env::temp_dir().join("bhdl_emit_shortlist_bulk_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let board = format!(r#"
board ShortlistBulk {{
    requirements {{ decap_lib: "{}/tests/circuits/realistic/decap_lib_fixture.bhdl"; }}
    power VBAT = 3.6V @ 10A;
    port V50: power out = 5V @ 5A;
    ground GND;
    soc: SlSoc();
    @V50 -> soc.1; soc.GND -> @GND;
}}
entity SlSoc() {{
    pin 1: power in;
    pin GND: ground;
    domain VDD_MAIN pins="1" v=5V i_nom=200mA i_max=1A step=4.5A rise=10us dur=200us droop_max=3% source="FIXTURE — shortlist bulk probe";
}}
"#, root.display());
    let f = dir.join("sl.bhdl");
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
    assert!(text.contains("bulk source: shortlist part DecapBulk100u"), "shortlist not used:\n{}", text.lines().rev().take(15).collect::<Vec<_>>().join("\n"));
    let emitted = std::fs::read_to_string(&f).unwrap();
    assert!(emitted.contains("seqbulk_v50_1: DecapBulk100u()"), "no stacked library bulk:\n{emitted}");
    assert!(emitted.contains("import { DecapBulk100u }"), "library import missing");
    // the emitted bank passed the stability envelope + timeline — no
    // STABILITY finding printed
    assert!(!text.contains("STABILITY:"), "{}", text.lines().rev().take(15).collect::<Vec<_>>().join("\n"));
    // §7.5 addendum 6: block-internal application caps are
    // characterized from the SAME shortlist (smallest candidate ≥ the
    // datasheet minimum), so the RESONANCE UNCHECKED note is CLOSED —
    // both asserted, so the closure can never go vacuous
    assert!(
        text.contains("characterize u_v50_C_out") && text.contains("from the shortlist"),
        "block cap not characterized from the shortlist:\n{}",
        text.lines().filter(|l| l.contains("characterize")).collect::<Vec<_>>().join("\n")
    );
    assert!(
        !text.contains("RESONANCE UNCHECKED"),
        "resonance gap still open:\n{}",
        text.lines().filter(|l| l.contains("RESONANCE")).collect::<Vec<_>>().join("\n")
    );
}

/// N+1 redundancy knob (`requirements { pdn_redundancy: "n+1"; }`):
/// every bulk stack carries one extra part so ANY single capacitor
/// open leaves the proven-sufficient count. With a shortlist that is
/// k+1 library parts; the bare-Cap fallback (asserted here — it is
/// deterministic) emits TWO full-size caps, each alone sufficient.
#[test]
fn emit_bulk_n_plus_1_redundancy() {
    let root = workspace_root();
    let dir = std::env::temp_dir().join("bhdl_emit_nplus1_bulk_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let board = r#"
board NPlusOneBulk {
    requirements { pdn_redundancy: "n+1"; }
    power VBAT = 3.6V @ 10A;
    port V50: power out = 5V @ 5A;
    ground GND;
    soc: SlSoc();
    @V50 -> soc.1; soc.GND -> @GND;
}
entity SlSoc() {
    pin 1: power in;
    pin GND: ground;
    domain VDD_MAIN pins="1" v=5V i_nom=200mA i_max=1A step=4.5A rise=10us dur=200us droop_max=3% source="FIXTURE — N+1 bulk probe";
}
"#;
    let f = dir.join("n1.bhdl");
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
    assert!(
        text.contains("pdn_redundancy: n+1"),
        "knob not stated:\n{}",
        text.lines().rev().take(15).collect::<Vec<_>>().join("\n")
    );
    let emitted = std::fs::read_to_string(&f).unwrap();
    // bare-Cap fallback under n+1: two full-size caps, never one
    let n1 = emitted.contains("seqbulk_v50_1: Cap(");
    let n2 = emitted.contains("seqbulk_v50_2: Cap(");
    assert!(n1 && n2, "expected two full-size bulk caps under n+1:\n{emitted}");
    // and both are the SAME size (each survivor subset must be >= C)
    let size_of = |k: usize| -> Option<String> {
        emitted
            .lines()
            .find(|l| l.contains(&format!("seqbulk_v50_{k}: Cap(")))
            .and_then(|l| l.split("Cap(").nth(1))
            .and_then(|r| r.split(')').next())
            .map(str::to_string)
    };
    assert_eq!(size_of(1), size_of(2), "redundant caps must be identical:\n{emitted}");
}
