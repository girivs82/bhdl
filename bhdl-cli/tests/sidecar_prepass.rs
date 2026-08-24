//! Regression: board TEXT a command reads from DISK (because its board
//! is a different file than the CLI input) must get the SAME two text
//! pre-passes the input gets — supply desugaring + stage-requirement
//! resolution. Historically the safety sidecar and powertree's
//! regenerate-strip parsed raw disk text, so a board carrying a stage
//! requirement instantiation synthesized RAW and died with
//! "Undefined component type: LdoStage" (verified 2026-08-24).

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("bhdl-cli has a parent workspace dir")
        .to_path_buf()
}

fn run(dir: &std::path::Path, input: &std::path::Path, args: &[&str]) -> (bool, String) {
    let root = workspace_root();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    cmd.current_dir(dir)
        .env_remove("BHDL_LIB_PATH")
        .args(["-I", root.to_str().unwrap()])
        .arg(input);
    cmd.args(args);
    let out = cmd.output().expect("failed to spawn bhdl-cli");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

const BOARD: &str = r#"import { LdoStage } from "bhdl-stdlib/power/stages.bhdl";
import { Res } from "bhdl-stdlib/passives/resistor.bhdl";
board ScBoard {
    power V5 = 5V @ 1A;
    port V33: power out = 3.3V @ 100mA;
    ground GND;
    @V5 -> u1: LdoStage(vout=3.3V, i_max=100mA, vin=5V).VIN;
    u1.GND -> @GND; u1.VOUT -> @V33;
    @V33 -> R_LOAD: Res(33Ω, wattage=1W).1; R_LOAD.2 -> @GND;
}
"#;

/// safety sidecar: the input defines only `safety ScBoard as … { }`;
/// the board (with a stage REQUIREMENT) comes from disk via the import.
#[test]
fn safety_sidecar_board_gets_the_text_prepasses() {
    let dir = std::env::temp_dir().join("bhdl_sidecar_prepass_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("sc_board.bhdl"), BOARD).unwrap();
    std::fs::write(
        dir.join("sc_safety.bhdl"),
        r#"import { ScBoard } from "./sc_board.bhdl";
safety ScBoard as brd {
    goal SG_V33: ASIL_B "V33 stays in regulation" (id="SG-SC-1") {
        effect overvoltage = brd.R_LOAD.1 > 3.6V severity S2;
    }
}
"#,
    )
    .unwrap();
    let (_ok, text) = run(&dir, &dir.join("sc_safety.bhdl"), &["safety"]);
    assert!(
        !text.contains("Undefined component type"),
        "sidecar board synthesized RAW — the pre-passes did not run:\n{}",
        text.lines().rev().take(15).collect::<Vec<_>>().join("\n")
    );
    assert!(
        text.contains("u1: LdoStage(") && text.contains("→"),
        "no stage-resolution report for the sidecar board"
    );
    // the fixture declares no safety mechanism, so the VERDICT is FAIL
    // (exit 1) by design — what this test pins is that the analysis ran
    // to a verdict at all instead of dying at synthesis
    assert!(
        text.contains("verdict:"),
        "safety analysis did not reach a verdict:\n{}",
        text.lines().rev().take(15).collect::<Vec<_>>().join("\n")
    );
}

/// powertree regenerate: the second `--emit` strips the generated
/// region from DISK text and replans — a hand-authored stage
/// requirement OUTSIDE the region must still resolve on that path.
#[test]
fn powertree_regenerate_strip_gets_the_text_prepasses() {
    let dir = std::env::temp_dir().join("bhdl_ptregen_prepass_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let board = r#"import { LdoStage } from "bhdl-stdlib/power/stages.bhdl";
entity Loady() {
    pin 1: power in;
    pin 2: ground;
    domain VDD pins="1" v=5V i_nom=0.5A i_max=0.8A source="FIXTURE — regen probe";
}
board PtRegen {
    power VIN12 = 12V @ 3A;
    port V5: power out = 5V @ 1A;
    ground GND;
    @V5 -> ld: Loady().1; ld.2 -> @GND;
    port V33: power out = 3.3V @ 100mA;
    @V5 -> u9: LdoStage(vout=3.3V, i_max=100mA, vin=5V).VIN;
    u9.GND -> @GND; u9.VOUT -> @V33;
}
"#;
    let f = dir.join("pt_regen.bhdl");
    std::fs::write(&f, board).unwrap();
    let args = ["powertree", "--input", "VIN12", "--emit", "1"];
    let (ok, text) = run(&dir, &f, &args);
    assert!(ok && text.contains("emitted option"), "first emit failed:\n{}", text.lines().rev().take(15).collect::<Vec<_>>().join("\n"));
    // second run: the region exists → the strip path fires
    let (ok, text) = run(&dir, &f, &args);
    assert!(
        text.contains("stripped for replanning"),
        "strip path did not fire on the second emit"
    );
    assert!(
        !text.contains("Undefined component type"),
        "stripped board synthesized RAW — the pre-passes did not run on the strip path:\n{}",
        text.lines().rev().take(15).collect::<Vec<_>>().join("\n")
    );
    assert!(ok && text.contains("re-verified through the full pipeline"), "regenerate failed:\n{}", text.lines().rev().take(15).collect::<Vec<_>>().join("\n"));
}
