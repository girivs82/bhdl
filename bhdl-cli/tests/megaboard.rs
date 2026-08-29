//! The megaboard dogfood fixture (FPGA + DDR4, full feature surface):
//! regression-pins the power-up physics the board flushed out —
//! placeholder stages ARE simulated (vout_nom discovery), direct
//! PG→EN chains gate as wired-AND, UVLO holds stages off below their
//! input floor, soft-start caps placeholder inrush, and an Off stage
//! does not hold its bank up against downstream draw on input loss.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn run(cmd: &str) -> String {
    let root = workspace_root();
    let mut c = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    c.current_dir(&root)
        .arg("-I")
        .arg(&root)
        .arg("tests/circuits/realistic/test_megaboard_fpga_ddr4.bhdl")
        .arg(cmd);
    let out = c.output().expect("spawn");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

#[test]
fn megaboard_powerup_ladder_is_real_and_windows_hold() {
    let text = run("powerup");
    // the ladder exists on the timeline (placeholder stages simulate)
    for rail in ["V_PROT GOOD", "VCCINT GOOD", "V18 GOOD", "V12Q GOOD"] {
        assert!(text.contains(rail), "missing {rail}:\n{}",
            text.lines().filter(|l| l.contains("GOOD") || l.contains("NEVER")).collect::<Vec<_>>().join("\n"));
    }
    // sequencing order realized: V_PROT before VCCINT before V18 before V12Q
    let t_of = |rail: &str| -> f64 {
        text.lines()
            .find(|l| l.contains(&format!("{rail} GOOD")))
            .and_then(|l| l.trim().split("ms").next())
            .and_then(|t| t.trim().parse::<f64>().ok())
            .unwrap_or(f64::NAN)
    };
    let (tp, tc, t18, t12q) = (t_of("V_PROT"), t_of("VCCINT"), t_of("V18"), t_of("V12Q"));
    assert!(tp < tc && tc < t18 && t18 < t12q, "ladder out of order: {tp} {tc} {t18} {t12q}");
    // no Error finding on the timeline (the findings block carries the
    // cross rows; the stage-survey near-miss crosses live before it)
    let findings: Vec<&str> = text.split("findings:").last().unwrap_or("").lines().collect();
    assert!(
        !findings.iter().any(|l| l.contains('\u{2717}')),
        "error findings:\n{}",
        findings.join("\n")
    );
    assert!(!text.contains("TRUNCATED"), "sim truncated");
}

#[test]
fn megaboard_powerdown_discharges_through_draw_and_bleed() {
    let text = run("powerdown");
    // input loss: the dead front-end must NOT hold its bank — every
    // rail discharges through downstream draw + the VTT bleed
    for rail in ["V18 DOWN", "VTT DOWN", "VCCINT DOWN", "V12Q DOWN"] {
        assert!(text.contains(rail), "missing {rail}:\n{}",
            text.lines().filter(|l| l.contains("DOWN") || l.contains("✗")).collect::<Vec<_>>().join("\n"));
    }
    assert!(
        !text.contains("never discharged"),
        "discharge finding should be closed by the bleed:\n{}",
        text.lines().filter(|l| l.contains("✗")).collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn megaboard_safety_catches_the_planted_clock_violation() {
    let text = run("safety");
    // the 100MHz oscillator against the DRAM's 1GHz clock_in contract
    assert!(text.contains("dram.CK_1G") && text.contains("VIOLATED"), "clock probe not caught");
    // SEooC attestations compose
    assert!(text.contains("270.0 FIT composed from SEooC vendor ATTESTATIONS"), "attestation");
    // route 1H rows on the SIL goal
    assert!(text.contains("route 1H"), "route-1H row missing");
}
