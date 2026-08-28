//! DC accuracy budget (spec §7.5 addendum 11): the classic worst-case
//! analysis — reference tolerance + FB-divider tolerance composed
//! against the domain's declared tol window. The solved nominal
//! passing the static window says nothing about the vendor stack-up;
//! this is the check that does. Numbers here are pinned to the
//! datasheet-stamped block data (TPS61022: v_ref 0.6V ±2.5%, divider
//! 1%+1%), so a silent change to the composition breaks loudly.

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
    let f = dir.join("acc.bhdl");
    std::fs::write(&f, board).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    cmd.current_dir(&dir)
        .env_remove("BHDL_LIB_PATH")
        .args(["-I", root.to_str().unwrap()])
        .arg(&f)
        .args(["powertree", "--input", "VBAT", "--emit", "1"]);
    let out = cmd.output().expect("spawn bhdl-cli");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

fn board(tol: &str) -> String {
    format!(
        r#"
board AccDemo {{
    power VBAT = 3.6V @ 10A;
    port V50: power out = 5V @ 5A;
    ground GND;
    soc: AccSoc();
    @V50 -> soc.1; soc.GND -> @GND;
}}
entity AccSoc() {{
    pin 1: power in;
    pin GND: ground;
    domain VDD_MAIN pins="1" v=5V tol={tol} i_nom=200mA i_max=1A source="FIXTURE — accuracy probe";
}}
"#
    )
}

#[test]
fn dc_accuracy_budget_composes_and_gates() {
    // wide window: the composed budget is shown, WITHIN — the exact
    // arithmetic is asserted (ref 2.5% + (1−0.6/5)·(1%+1%) = 4.26%)
    let text = run(&board("5%"), "bhdl_acc_wide_test");
    let line = text
        .lines()
        .find(|l| l.contains("accuracy: 'u_v50'"))
        .unwrap_or_else(|| panic!("no accuracy line:\n{}", text.lines().rev().take(20).collect::<Vec<_>>().join("\n")));
    assert!(
        line.contains("±4.26%") && line.contains("divider 1.76%") && line.contains("within"),
        "composition wrong: {line}"
    );

    // tight window: EXCEEDS — surfaced as a designer-action finding
    // (capacitance cannot fix a setpoint, so it must NOT block emission)
    let text = run(&board("3%"), "bhdl_acc_tight_test");
    assert!(
        text.contains("ACCURACY: 'u_v50'") && text.contains("EXCEEDS the window"),
        "tight window did not violate:\n{}",
        text.lines().filter(|l| l.contains("ACCURACY")).collect::<Vec<_>>().join("\n")
    );
    assert!(
        text.contains("NOT closable by bulk") && text.contains("emitted option"),
        "violation must surface as a designer finding, not block emission:\n{}",
        text.lines().rev().take(12).collect::<Vec<_>>().join("\n")
    );
}
