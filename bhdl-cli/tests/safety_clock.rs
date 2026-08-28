//! Clock-input contract (spec §2.15): drift = declared arithmetic,
//! the edge = MEASURED against the extracted net (single-pole model —
//! exact for one pole), jitter = declared/structural only. Pins the
//! hand math (1ns driver ⊕ 2.2·50Ω·15pF = 1.93ns vs 3ns), the
//! discharge of `assume clock_in(...)`, the violated arms, and the
//! missing-driver-data UNCHECKED arm.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn run(board: &str, dirname: &str) -> String {
    let root = workspace_root();
    let dir = std::env::temp_dir().join(dirname);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("ck.bhdl");
    std::fs::write(&f, board).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    cmd.current_dir(&root).arg("-I").arg(&root).arg(&f).arg("safety");
    let out = cmd.output().expect("spawn");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

#[test]
fn clock_contract_measured_edge_drift_and_discharge() {
    let root = workspace_root();
    let src = std::fs::read_to_string(root.join("tests/circuits/realistic/test_safety_clock.bhdl")).unwrap();

    // healthy: drift 30 ≤ 50 ppm, edge 1.93 ns ≤ 3 ns — discharged
    let text = run(&src, "bhdl_clock_ok_test");
    assert!(
        text.contains("drift: source 'osc' 25.000 MHz (offset 0.0 ppm) + stability 30 ppm = 30.0 ppm vs budget 50 ppm"),
        "drift arithmetic wrong:\n{}",
        text.lines().filter(|l| l.contains("drift")).collect::<Vec<_>>().join("\n")
    );
    assert!(
        text.contains("2.2·50Ω·15.0pF = 1.93ns"),
        "measured edge wrong:\n{}",
        text.lines().filter(|l| l.contains("edge")).collect::<Vec<_>>().join("\n")
    );
    assert!(
        text.contains("satisfied_by machine-verified: CLOCK checks pass"),
        "assumption not discharged"
    );
    assert!(text.contains("jitter: declared/structural only"), "jitter scope not stated");

    // violated edge: a slow driver behind 200 Ω → 6.68 ns > 3 ns; the
    // assumption must stay OPEN with the AouViolated gap
    let slow = src.replace("attribute r_out     = 50Ω;", "attribute r_out     = 200Ω;");
    assert_ne!(slow, src);
    let text = run(&slow, "bhdl_clock_slow_test");
    assert!(
        text.contains("vs rise_max 3.00ns → VIOLATED"),
        "slow edge not violated:\n{}",
        text.lines().filter(|l| l.contains("edge")).collect::<Vec<_>>().join("\n")
    );
    assert!(
        !text.contains("satisfied_by machine-verified: CLOCK checks pass"),
        "violated contract must not discharge"
    );
    assert!(text.contains("clock contract soc.CLK_MAIN violated"), "gap missing");

    // drifted source: 100 ppm > the 50 ppm budget
    let drifty = src.replace("attribute ppm       = 30;", "attribute ppm       = 100;");
    assert_ne!(drifty, src);
    let text = run(&drifty, "bhdl_clock_drift_test");
    assert!(
        text.contains("100.0 ppm vs budget 50 ppm → VIOLATED"),
        "drift not violated:\n{}",
        text.lines().filter(|l| l.contains("drift")).collect::<Vec<_>>().join("\n")
    );

    // driver missing r_out: the edge is UNCHECKED by name, never a pass
    let bare = src.replace("    attribute r_out     = 50Ω;     // FIXTURE: driver output impedance\n", "");
    assert_ne!(bare, src);
    let text = run(&bare, "bhdl_clock_bare_test");
    assert!(
        text.contains("the driver lacks r_out — UNCHECKED"),
        "missing driver data not stated:\n{}",
        text.lines().filter(|l| l.contains("edge")).collect::<Vec<_>>().join("\n")
    );
}
