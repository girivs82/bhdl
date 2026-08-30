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
fn io_banks_gate_voltage_and_power() {
    let root = workspace_root();
    let src = std::fs::read_to_string(
        root.join("tests/circuits/realistic/test_pinmux_alternates.bhdl"),
    )
    .unwrap();

    // 1. the shipped fixture is bank-clean (both banks powered at the
    //    peer's level) — no ERC035, no ERC004
    let text = run_file("tests/circuits/realistic/test_pinmux_alternates.bhdl");
    assert!(!text.contains("ERC035 IO bank discipline | Error"), "clean fixture flagged");

    // 2. unpower BANK_B: using PB6/PB7 (the I2C home) = dead silicon
    let unpowered = src.replace("    @VDD -> mcu.VDDIO_B;
", "");
    assert_ne!(unpowered, src);
    let text = run_src(&unpowered, "bank_unpowered.bhdl");
    assert!(
        text.contains("IO bank 'BANK_B' is UNPOWERED"),
        "unpowered bank not refused:
{}",
        text.lines().filter(|l| l.contains("ERC035")).collect::<Vec<_>>().join("
")
    );

    // 3. power BANK_B from a 1.8V rail while the peer runs at 3.3V:
    //    ERC004 judges the banked pins at the BANK rail's net voltage
    let mismatched = src
        .replace(
            "    power VDD = 3.3V @ 1A;",
            "    power VDD = 3.3V @ 1A;
    power VDD18 = 1.8V @ 100mA;",
        )
        .replace("    @VDD -> mcu.VDDIO_A;", "    @VDD18 -> mcu.VDDIO_A;")
        .replace(
            "    pin SDA: signal inout;",
            "    pin VCC: power in;
    pin SDA: signal inout;",
        )
        .replace("    peer.GND -> @GND;", "    peer.GND -> @GND;
    @VDD -> peer.VCC;");
    assert_ne!(mismatched, src);
    let text = run_src(&mismatched, "bank_mismatch.bhdl");
    assert!(
        text.contains("(bank)") && text.contains("ERC004"),
        "bank-level ERC004 missing:
{}",
        text.lines().filter(|l| l.contains("ERC004")).collect::<Vec<_>>().join("
")
    );
}

#[test]
fn mux_header_carries_the_full_pin_story() {
    let root = workspace_root();
    let dir = std::env::temp_dir().join("bhdl_pinmux_hdr_test");
    std::fs::create_dir_all(&dir).unwrap();
    let hdr = dir.join("mux.h");
    let md = dir.join("pd.md");
    let mut c = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    c.current_dir(&root)
        .arg("-I")
        .arg(&root)
        .arg("tests/circuits/realistic/test_pinmux_alternates.bhdl")
        .arg("doc")
        .arg("--mux-header")
        .arg(&hdr)
        .arg("-o")
        .arg(&md);
    let out = c.output().expect("spawn");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let h = std::fs::read_to_string(&hdr).unwrap();
    // the solved alternates
    assert!(h.contains("#define BHDL_MUX_MCU_UART1_ALT \"AF_B\""), "{h}");
    assert!(h.contains("#define BHDL_MUX_MCU_UART1_TX_PIN PA9"), "{h}");
    assert!(h.contains("#define BHDL_MUX_MCU_I2C1_SDA_PIN PB7"), "{h}");
    // fixed bindings ride along — the chip's whole pin story
    assert!(h.contains("#define BHDL_MUX_MCU_SPI1_SCK_PIN PA5 /* fixed */"), "{h}");
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
