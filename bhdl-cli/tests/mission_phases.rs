//! Mission-profile phases (spec §2.8): the machinery shipped long ago
//! and had ZERO exercise — a path that runs is not a path that tests.
//! Pins: named-profile resolution from the stdlib file, the
//! time-weighted λ over powered phases (hand math in the basis
//! string), the inline-phase form, the sum≠1 refusal, and the
//! capstone report's phase table.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn run(board: &str, dirname: &str, report: Option<&str>) -> (String, Option<String>) {
    let root = workspace_root();
    let dir = std::env::temp_dir().join(dirname);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("mp.bhdl");
    std::fs::write(&f, board).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    cmd.current_dir(&root).arg("-I").arg(&root).arg(&f).arg("safety");
    let rp = report.map(|r| dir.join(r));
    if let Some(rp) = &rp {
        cmd.arg("--report").arg(rp);
    }
    let out = cmd.output().expect("spawn");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (text, rp.map(|p| std::fs::read_to_string(p).unwrap_or_default()))
}

fn base_board(mission: &str) -> String {
    let src = std::fs::read_to_string(workspace_root().join("tests/circuits/realistic/test_safety_dfa.bhdl")).unwrap();
    let out = src.replace("mission { ambient = 40degC; lifetime = 15000h; }", mission);
    assert_ne!(out, src, "fixture mission line changed — update the replace");
    out
}

#[test]
fn named_profile_weighted_lambda_and_report() {
    let (text, md) = run(
        &base_board("mission { profile = passenger_compartment; lifetime = 15000h; }"),
        "bhdl_mission_profile_test",
        Some("mp.safety.md"),
    );
    // resolution from the stdlib file, source printed
    assert!(
        text.contains("mission profile passenger_compartment") && text.contains("mission_profiles.toml"),
        "profile not resolved:\n{}",
        text.lines().filter(|l| l.contains("profile")).collect::<Vec<_>>().join("\n")
    );
    // the weighted composition IS the basis: unpowered parked phase
    // contributes nothing, powered share divides (operating basis) —
    // per-phase λs visible so the math is checkable by hand
    let basis = text
        .lines()
        .find(|l| l.contains("Σ[parked 90%@23°C off"))
        .unwrap_or_else(|| panic!("no weighted basis line:\n{text}"));
    assert!(
        basis.contains("/ powered 10%") && basis.contains("operating basis"),
        "basis wrong: {basis}"
    );
    // the report renders the phase table + weighted-mean ambient
    let md = md.unwrap();
    assert!(md.contains("time-weighted mean of profile `passenger_compartment`"), "report ambient line");
    assert!(
        md.contains("| parked | 90.0% | 23 °C | no") && md.contains("| hot_soak | 0.5% | 85 °C | yes |"),
        "report phase table:\n{}",
        md.lines().filter(|l| l.starts_with('|')).take(8).collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn inline_phases_and_sum_refusal() {
    // inline phases: same engine path, no profile file involved
    let (text, _) = run(
        &base_board(
            "mission { lifetime = 15000h; phase idle { time = 50%; ambient = 25degC; } phase run { time = 50%; ambient = 60degC; } }",
        ),
        "bhdl_mission_inline_test",
        None,
    );
    assert!(
        text.contains("Σ[idle 50%@25°C") && text.contains("run 50%@60°C"),
        "inline phases not weighted:\n{}",
        text.lines().filter(|l| l.contains("Σ[")).take(2).collect::<Vec<_>>().join("\n")
    );
    // a histogram that does not cover the life is REFUSED, never scaled
    let (text, _) = run(
        &base_board("mission { lifetime = 15000h; phase run { time = 60%; ambient = 60degC; } }"),
        "bhdl_mission_sum_test",
        None,
    );
    assert!(
        text.contains("phases sum to 0.600, not 1.0"),
        "bad histogram not refused:\n{}",
        text.lines().filter(|l| l.contains("sum")).collect::<Vec<_>>().join("\n")
    );
}
