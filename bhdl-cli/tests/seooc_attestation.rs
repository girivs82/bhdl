//! SEooC λ attestation (spec §2.7): a black box the campaign cannot
//! simulate composes into the board metrics ONLY through the vendor's
//! own attested FMEDA split — lambda + spfm + lfm, all three, cited.
//! Pins the hand math (λ=120, SPFM 92%, LFM 70% ⇒ residual 9.6 FIT,
//! latent 33.1 FIT), the classification-only accounting of the
//! attested part's board-side rows, and the partial-attestation gap.

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
    let f = dir.join("sx.bhdl");
    std::fs::write(&f, board).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    cmd.current_dir(&root).arg("-I").arg(&root).arg(&f).arg("safety");
    let out = cmd.output().expect("spawn");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

fn board(seooc_line: &str) -> String {
    let src = std::fs::read_to_string(workspace_root().join("tests/circuits/realistic/test_safety_dfa.bhdl")).unwrap();
    let out = src
        .replace(
            "board DfaDemo {",
            &format!(
                r#"entity SeoocBox() {{
    pin 1: signal inout;
    pin 2: ground;
    attribute component_class = "asic";
    safety {{
        {seooc_line}
    }}
}}

board DfaDemo {{"#
            ),
        )
        .replace(
            "    @V33A -> mon_top: Res(10kΩ).1;",
            "    @V33A -> box: SeoocBox().1; box.2 -> @GND;\n    @V33A -> mon_top: Res(10kΩ).1;",
        );
    assert_ne!(out, src, "fixture shape changed — update the replaces");
    out
}

#[test]
fn full_attestation_composes_with_hand_math() {
    let text = run(
        &board(r#"seooc lambda=120 spfm=0.92 lfm=0.7 source="FIXTURE vendor FMEDA — attested split";"#),
        "bhdl_seooc_full_test",
    );
    // the part row states the attestation
    assert!(
        text.contains("ATTESTED SPFM=92% LFM=70%"),
        "attestation not shown:\n{}",
        text.lines().filter(|l| l.contains("seooc")).collect::<Vec<_>>().join("\n")
    );
    // hand math: residual = 120·(1−0.92) = 9.6; latent = 120·0.92·(1−0.7) = 33.1
    let metrics = text
        .lines()
        .find(|l| l.contains("metrics [board]"))
        .unwrap_or_else(|| panic!("no metrics line:\n{text}"));
    assert!(
        metrics.contains("λ_residual=9.6") && metrics.contains("λ_latent=33.1"),
        "attested split wrong: {metrics}"
    );
    assert!(
        text.contains("includes 120.0 FIT composed from SEooC vendor ATTESTATIONS"),
        "attested share not stated"
    );
    // the attested part's board-side rows are classification-only —
    // stated on the row, and NOT counted as unmeasured (the remaining
    // unmeasured count comes from the two no-data LDO dies only)
    assert!(
        text.contains("classification-only"),
        "board-side accounting not stated"
    );
}

#[test]
fn partial_attestation_is_a_named_gap() {
    let text = run(
        &board(r#"seooc lambda=120 spfm=0.92 source="FIXTURE vendor FMEDA — lfm withheld";"#),
        "bhdl_seooc_partial_test",
    );
    assert!(
        text.contains("SEooC attestation incomplete") && text.contains("declare lambda, spfm AND lfm"),
        "partial attestation not gapped:\n{}",
        text.lines().filter(|l| l.contains("attestation") || l.contains("seooc")).collect::<Vec<_>>().join("\n")
    );
    // nothing composed: the metrics must NOT contain the attested share
    assert!(
        !text.contains("composed from SEooC vendor ATTESTATIONS"),
        "partial attestation must not compose"
    );
}
