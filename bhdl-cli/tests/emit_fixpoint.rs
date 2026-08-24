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
