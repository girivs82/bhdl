//! DFA (spec §2.13) on the two-LDO fixture: the supervisor powered
//! from the rail it supervises is the CLASSIC dependent failure —
//! DF-SUPPLY strong, a DEPENDENT_FAILURE gap. Moving one wire (the
//! divider's feed to the independent rail) downgrades it to the
//! input-only informational note — the same walk, the opposite
//! verdict, so the finding tracks the topology, not the wiring style.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn run_safety(board: &str, dirname: &str) -> String {
    let root = workspace_root();
    let dir = std::env::temp_dir().join(dirname);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("dfa.bhdl");
    std::fs::write(&f, board).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    cmd.current_dir(&root).arg("-I").arg(&root).arg(&f).arg("safety");
    let out = cmd.output().expect("spawn bhdl-cli");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

#[test]
fn dfa_supply_sharing_strong_vs_input_only_info() {
    let root = workspace_root();
    let src = std::fs::read_to_string(root.join("tests/circuits/realistic/test_safety_dfa.bhdl")).unwrap();

    // dependent: supervisor fed from the supervised rail
    let text = run_safety(&src, "bhdl_dfa_dependent_test");
    assert!(
        text.contains("[DF-SUPPLY]") && text.contains("V33A") && text.contains("blinds its detection"),
        "DF-SUPPLY not found:\n{}",
        text.lines().filter(|l| l.contains("DF-")).collect::<Vec<_>>().join("\n")
    );
    assert!(
        text.contains("DEPENDENT_FAILURE"),
        "strong finding must land in the gap report:\n{}",
        text.lines().filter(|l| l.contains("DEPENDENT")).collect::<Vec<_>>().join("\n")
    );

    // independent: ONE wire moved — the divider feeds from V33B
    let indep = src.replace(
        "@V33A -> mon_top: Res(10kΩ).1;",
        "@V33B -> mon_top: Res(10kΩ).1;",
    );
    assert_ne!(indep, src, "fixture shape changed — update the replace");
    let text = run_safety(&indep, "bhdl_dfa_independent_test");
    assert!(
        !text.contains("DEPENDENT_FAILURE"),
        "independent supply must NOT gap:\n{}",
        text.lines().filter(|l| l.contains("DF-") || l.contains("DEPENDENT")).collect::<Vec<_>>().join("\n")
    );
    assert!(
        text.contains("share supply only at the board input") && text.contains("VBAT"),
        "input-only sharing must be the stated informational note:\n{}",
        text.lines().filter(|l| l.contains("DF-SUPPLY")).collect::<Vec<_>>().join("\n")
    );
}
