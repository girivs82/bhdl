//! Backlog-clearing coverage: IEC 61508 route-1H rows (Type B /
//! HFT>0) and the SoC SEooC template consuming the full contract
//! vocabulary in one board.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn run_file(p: &PathBuf) -> String {
    let root = workspace_root();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    cmd.current_dir(&root).arg("-I").arg(&root).arg(p).arg("safety");
    let out = cmd.output().expect("spawn");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

#[test]
fn route_1h_type_b_hft_rows() {
    let root = workspace_root();
    let src = std::fs::read_to_string(root.join("tests/circuits/realistic/test_safety_pdn_recheck.bhdl")).unwrap();
    let dir = std::env::temp_dir().join("bhdl_route1h_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // SIL2 goal, HFT 1: SFF 75 % (band 60–90) allows exactly SIL2
    let sil = src.replace(
        r#"goal SG: ASIL_B "rail stays in window" (id="SG-PDNRC-1") {"#,
        r#"goal SG: SIL2 "rail stays in window" (id="SG-PDNRC-1", hft=1, element=B) {"#,
    );
    assert_ne!(sil, src);
    let f = dir.join("hft1.bhdl");
    std::fs::write(&f, &sil).unwrap();
    let text = run_file(&f);
    assert!(
        text.contains("SFF 75.0% · type B · HFT 1 → route 1H allows SIL2 ≥ goal SIL2"),
        "HFT1 row wrong:\n{}",
        text.lines().filter(|l| l.contains("route 1H")).collect::<Vec<_>>().join("\n")
    );
    // same SFF at HFT 0 caps at SIL1 — VIOLATED, gapped, defaults stated
    let f0 = dir.join("hft0.bhdl");
    std::fs::write(&f0, sil.replace("hft=1, element=B", "element=B")).unwrap();
    let text = run_file(&f0);
    assert!(
        text.contains("route 1H caps at SIL1 < goal SIL2") && text.contains("HFT 0 assumed"),
        "HFT0 violation wrong:\n{}",
        text.lines().filter(|l| l.contains("route 1H")).collect::<Vec<_>>().join("\n")
    );
    assert!(text.contains("caps the claimable SIL at SIL1"), "route-1H gap missing");
}

#[test]
fn soc_seooc_template_exercises_the_whole_vocabulary() {
    let root = workspace_root();
    let text = run_file(&root.join("tests/circuits/realistic/test_soc_seooc_template.bhdl"));
    // the template's AoUs dispositioned from the board (the deferred-
    // resolution fix: part AoUs are seeded AFTER block statements)
    assert!(
        text.contains(r#"soc.AOU_WDT"#) && text.contains("no watchdog fitted"),
        "AoU waiver not applied:\n{}",
        text.lines().filter(|l| l.contains("AOU")).collect::<Vec<_>>().join("\n")
    );
    // the attested split composes; the clock contract discharges; the
    // PDN honestly flags the under-decapped fixture against the
    // template's aggressive mask (a REAL finding, kept)
    assert!(text.contains("includes 250.0 FIT composed from SEooC vendor ATTESTATIONS"), "attestation");
    assert!(text.contains("machine-verified: CLOCK checks pass"), "clock discharge");
    assert!(text.contains("soc zmask"), "the template's PDN contract must reach the mask check");
}
