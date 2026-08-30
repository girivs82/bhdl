//! Pinmux alternates (SoC arc increment 3): entities declare
//! `alt "AFn" { SIG = PIN; ... }` groups (the vendor's alternate-
//! function table); the per-instance assignment solver picks one
//! alternate per WIRED field so no physical pin serves two roles —
//! deterministic, override-able, and a hard error with the full
//! candidate survey when the wired set cannot coexist.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn run_file(path: &str) -> String {
    let root = workspace_root();
    let mut c = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    c.current_dir(&root).arg("-I").arg(&root).arg(path).arg("synthesize");
    let out = c.output().expect("spawn");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

fn run_src(src: &str, name: &str) -> String {
    let dir = std::env::temp_dir().join("bhdl_pinmux_test");
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join(name);
    std::fs::write(&f, src).unwrap();
    run_file(f.to_str().unwrap())
}

#[test]
fn solver_moves_uart_off_the_i2c_pins() {
    let text = run_file("tests/circuits/realistic/test_pinmux_alternates.bhdl");
    // i2c1 has one home; uart1 must yield its preferred AF_A
    assert!(text.contains(r#"pinmux: mcu.i2c1 → alt "AF_A""#), "i2c choice:\n{}",
        text.lines().filter(|l| l.contains("pinmux")).collect::<Vec<_>>().join("\n"));
    assert!(text.contains(r#"pinmux: mcu.uart1 → alt "AF_B""#), "uart choice:\n{}",
        text.lines().filter(|l| l.contains("pinmux")).collect::<Vec<_>>().join("\n"));
    // no unsolved-reference fallback fired
    assert!(!text.contains("falling back to the FIRST declared alternate"),
        "fallback fired — the solver missed an instance");
    // the uart connection landed on the AF_B physical pin
    assert!(text.contains("Connected mcu.PA9"), "uart TX not on PA9");
}

#[test]
fn override_forces_the_alternate_and_conflicts_survey_loudly() {
    let root = workspace_root();
    let src = std::fs::read_to_string(
        root.join("tests/circuits/realistic/test_pinmux_alternates.bhdl"),
    )
    .unwrap();

    // 1. designer override to the ALREADY-FREE alternate: honored + labelled
    let forced = src.replace(
        "    mcu.uart1.TX -> peer.RX;",
        "    attribute mcu.mux__uart1 = \"AF_B\";\n    mcu.uart1.TX -> peer.RX;",
    );
    assert_ne!(forced, src);
    let text = run_src(&forced, "override_ok.bhdl");
    assert!(
        text.contains(r#"pinmux: mcu.uart1 → alt "AF_B" (designer override)"#),
        "override not honored/labelled:\n{}",
        text.lines().filter(|l| l.contains("pinmux")).collect::<Vec<_>>().join("\n")
    );

    // 2. override forcing uart onto the I2C pins: unsatisfiable — the
    //    error carries the survey naming the blockers
    let clash = src.replace(
        "    mcu.uart1.TX -> peer.RX;",
        "    attribute mcu.mux__uart1 = \"AF_A\";\n    mcu.uart1.TX -> peer.RX;",
    );
    let text = run_src(&clash, "override_clash.bhdl");
    assert!(
        text.contains("pinmux: no assignment satisfies 'mcu'"),
        "unsat not raised:\n{}",
        text.lines().rev().take(20).collect::<Vec<_>>().join("\n")
    );
    assert!(
        text.contains("BLOCKED") && text.contains("claimed by"),
        "survey missing blockers:\n{}",
        text.lines().filter(|l| l.contains("alt") || l.contains("BLOCKED")).collect::<Vec<_>>().join("\n")
    );
}
