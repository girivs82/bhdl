//! Increment 2 of the SoC-features arc: `bhdl layout --propose-swizzle`
//! chooses the crossing-minimizing LEGAL permutation on the placed
//! board and emits it into the marked swizzle region (powertree-emit
//! ownership: the tool only ever rewrites its own region).
//!
//! The fixture pins both chips with the memory rotated 180°, so the
//! identity pairing is crossed by construction. Asserted end to end:
//! the proposal reports a crossing reduction to zero, rewrites the
//! region, the emitted permutation passes ERC034 (as-built Info, no
//! Errors), and a second run is a no-change fixpoint.
//!
//! Deliberately NOT the full PnR suite: one tiny pinned board, one
//! trial, capped iterations — the heavy layout-oracle sweep stays in
//! the dedicated PnR phase.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn run(args: &[&str]) -> String {
    let root = workspace_root();
    let mut c = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    c.current_dir(&root).arg("-I").arg(&root).args(args);
    let out = c.output().expect("spawn");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

#[test]
fn propose_swizzle_emits_verifies_and_reaches_fixpoint() {
    let root = workspace_root();
    let dir = std::env::temp_dir().join("bhdl_swz_propose_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("swzp.bhdl");
    std::fs::copy(
        root.join("tests/circuits/realistic/test_swizzle_propose.bhdl"),
        &f,
    )
    .unwrap();
    let pcb = dir.join("swzp.kicad_pcb");

    let layout_args = |f: &PathBuf, pcb: &PathBuf| -> Vec<String> {
        vec![
            f.to_str().unwrap().into(),
            "layout".into(),
            "--propose-swizzle".into(),
            "-t".into(),
            "1".into(),
            "--max-iterations".into(),
            "150".into(),
            "--seed".into(),
            "42".into(),
            "-o".into(),
            pcb.to_str().unwrap().into(),
        ]
    };

    // 1. propose + emit
    let args: Vec<String> = layout_args(&f, &pcb);
    let text = run(&args.iter().map(String::as_str).collect::<Vec<_>>());
    assert!(
        text.contains("crossings") && text.contains("→ 0"),
        "no crossing reduction reported:\n{}",
        text.lines().filter(|l| l.contains("wirelength") || l.contains("optimal")).collect::<Vec<_>>().join("\n")
    );
    assert!(text.contains("emitted the swizzle region"), "region not emitted");
    let emitted = std::fs::read_to_string(&f).unwrap();
    assert!(emitted.contains("BEGIN GENERATED SWIZZLE"), "markers lost");
    // the pinned-180° geometry forces the lane swap
    assert!(
        emitted.contains("mc.ddr.lane0.DQ0 -> mem.ddr.lane1."),
        "lane swap missing from region:\n{emitted}"
    );

    // 2. the emitted permutation passes ERC034 (Info, no Errors)
    let text = run(&[f.to_str().unwrap(), "synthesize"]);
    let rows: Vec<&str> = text.lines().filter(|l| l.contains("ERC034") && l.starts_with('|')).collect();
    assert!(
        !rows.iter().any(|l| l.contains("| Error |")),
        "emitted permutation flagged:\n{}",
        rows.join("\n")
    );
    assert!(
        rows.iter().any(|l| l.contains("as-built swizzle")),
        "as-built Info table missing:\n{}",
        rows.join("\n")
    );

    // 3. fixpoint: a second run proposes no change
    let args: Vec<String> = layout_args(&f, &pcb);
    let text = run(&args.iter().map(String::as_str).collect::<Vec<_>>());
    assert!(
        text.contains("already optimal"),
        "second run not a fixpoint:\n{}",
        text.lines().filter(|l| l.contains("wirelength") || l.contains("optimal") || l.contains("emitted")).collect::<Vec<_>>().join("\n")
    );
    let after = std::fs::read_to_string(&f).unwrap();
    assert_eq!(emitted, after, "second run rewrote the file");
}
