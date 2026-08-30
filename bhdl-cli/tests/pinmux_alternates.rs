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
fn internal_pulls_configure_from_connections() {
    let text = run_file("tests/circuits/realistic/test_pinmux_alternates.bhdl");
    // the open-drain IRQ gets the internal pull-up (idle-high, stated)
    assert!(
        text.contains("pull: mcu.PA4 → up"),
        "IRQ pull missing:
{}",
        text.lines().filter(|l| l.contains("pull:")).collect::<Vec<_>>().join("
")
    );
    // the UART TX pin serves a peripheral OUTPUT — no pull; the I2C
    // pins have EXTERNAL pulls — internal off. Neither prints (off is
    // silent) and neither materialises a resistor.
    assert!(!text.contains("pull: mcu.PA9"), "TX pin pulled");
    assert!(!text.contains("pull: mcu.PB6"), "ext-pulled pin pulled");
    assert!(text.contains("pullcfg_mcu_pa4"), "pull resistor not materialised");
    assert!(!text.contains("pullcfg_mcu_pa9"), "spurious pull resistor");
}

#[test]
fn pull_header_and_midpoint_contradiction() {
    let root = workspace_root();
    // header: the firmware pull program rides with the mux commitments
    let dir = std::env::temp_dir().join("bhdl_pull_hdr_test");
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
    assert!(c.output().expect("spawn").status.success());
    let h = std::fs::read_to_string(&hdr).unwrap();
    assert!(h.contains("#define BHDL_MUX_MCU_PA4_PULL UP"), "{h}");
    assert!(h.contains("#define BHDL_MUX_MCU_PB6_PULL OFF"), "{h}");

    // midpoint contradiction: board pull-DOWN + designer-forced
    // internal pull-up, equal strengths — NO pull-specific logic
    // exists; the DC solve computes the divider and ERC036 refuses
    // the ambiguous band
    let src = std::fs::read_to_string(
        root.join("tests/circuits/realistic/test_pinmux_alternates.bhdl"),
    )
    .unwrap();
    let clash = src.replace(
        "    peer.INT -> mcu.PA4;",
        "    peer.INT -> mcu.PA4;
    attribute mcu.pull__PA4 = \"up\";
    @GND -> rp_int: Res(40kΩ, tolerance = 1%).1; rp_int.2 -> mcu.PA4;",
    );
    assert_ne!(clash, src);
    let f = dir.join("pull_clash.bhdl");
    std::fs::write(&f, &clash).unwrap();
    let mut c = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    c.current_dir(&root).arg("-I").arg(&root).arg(&f).arg("report");
    let out = c.output().expect("spawn");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("ERC036") && text.contains("ambiguous 30–70% band"),
        "midpoint not refused:
{}",
        text.lines().filter(|l| l.contains("ERC036") || l.contains("PA4")).collect::<Vec<_>>().join("
")
    );
    assert!(text.contains("50%"), "not at midpoint:
{}",
        text.lines().filter(|l| l.contains("ERC036")).collect::<Vec<_>>().join("
"));
}

#[test]
fn open_drain_pullup_tiers() {
    let root = workspace_root();
    let src = std::fs::read_to_string(
        root.join("tests/circuits/realistic/test_pinmux_alternates.bhdl"),
    )
    .unwrap();

    // baseline: peer.INT (declared open-drain) point-to-point with the
    // MCU's configured internal pull-up — SILENT (a single-OD net
    // accepts an internal PU)
    let text = run_file("tests/circuits/realistic/test_pinmux_alternates.bhdl");
    assert!(!text.contains("ERC037"), "clean fixture flagged:\n{}",
        text.lines().filter(|l| l.contains("ERC037")).collect::<Vec<_>>().join("\n"));

    // 1. remove the MCU's pull capability on PA4 → single OD pin with
    //    NO pull-up anywhere → Warning "can never go high"
    let bare = src.replace("pins=PA4,PA9,PA10,PB6,PB7", "pins=PA9,PA10,PB6,PB7");
    assert_ne!(bare, src);
    let text = run_src(&bare, "od_bare.bhdl");
    assert!(
        text.contains("ERC037") && text.contains("NO pull-up anywhere"),
        "single-OD bare not warned:\n{}",
        text.lines().filter(|l| l.contains("ERC037")).collect::<Vec<_>>().join("\n")
    );

    // 2. wire a SECOND open-drain device onto the line (wired-AND):
    //    the internal PU stays configured but is NOT sufficient
    let wired_and = src.replace(
        "    peer.INT -> mcu.PA4;",
        "    peer.INT -> mcu.PA4;\n    peer2: MuxPeer();\n    peer2.GND -> @GND;\n    peer2.INT -> mcu.PA4;",
    );
    assert_ne!(wired_and, src);
    let text = run_src(&wired_and, "od_wired_and.bhdl");
    assert!(
        text.contains("wire-ANDs") && text.contains("NOT sufficient"),
        "wired-AND with internal PU not warned:\n{}",
        text.lines().filter(|l| l.contains("ERC037")).collect::<Vec<_>>().join("\n")
    );

    // 3. ONE external pull-up serves the whole wired-AND (dedupe) —
    //    silent again
    let pulled = wired_and.replace(
        "    peer.INT -> mcu.PA4;",
        "    peer.INT -> mcu.PA4;\n    @VDD -> rp_irq: Res(10kΩ, tolerance = 1%).1; rp_irq.2 -> mcu.PA4;",
    );
    let text = run_src(&pulled, "od_pulled.bhdl");
    assert!(
        !text.contains("ERC037"),
        "external pull-up not honored:\n{}",
        text.lines().filter(|l| l.contains("ERC037")).collect::<Vec<_>>().join("\n")
    );
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
