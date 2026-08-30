//! Real STM32F103 remap tables through the mux solver. The stdlib
//! entities carry the F1's AFIO remap register states as `alt`
//! groups transcribed from RM0008 Rev 9 Tables 43–47; the fixture
//! board straps USART1's reset home so the solver must walk the
//! same cascade a firmware engineer reads off those tables.

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

#[test]
fn f103_remap_cascade_matches_rm0008() {
    let text = run_file("tests/circuits/realistic/stm32_remap_solver.bhdl");
    let mux_lines = || text.lines().filter(|l| l.contains("pinmux")).collect::<Vec<_>>().join("\n");
    // straps hold PA9/PA10 → USART1_REMAP=1 (RM0008 Table 45: PB6/PB7)
    assert!(text.contains(r#"pinmux: mcu.uart1 → alt "REMAP""#), "uart1 choice:\n{}", mux_lines());
    // …which takes I²C1's reset home → I2C1_REMAP=1 (Table 46: PB8/PB9)
    assert!(text.contains(r#"pinmux: mcu.i2c1 → alt "REMAP""#), "i2c1 choice:\n{}", mux_lines());
    assert!(text.contains("Connected mcu.PB8"), "I2C SCL not on the remap home PB8");
    assert!(text.contains("Connected mcu.PB9"), "I2C SDA not on the remap home PB9");
    // internal pulls resolve logical direction THROUGH the chosen
    // remap: PB6 serves uart1.TX (logically OUT) — never auto-pulled
    assert!(!text.contains("pull: mcu.PB6 → up"), "UART TX pad auto-pulled despite serving an OUT signal");
    // the I²C nets carry external 4.7k pull-ups → internal off
    assert!(!text.contains("pull: mcu.PB8 → up"), "internal pull configured despite external I2C pull-up");
    assert!(!text.contains("falling back to the FIRST declared alternate"),
        "fallback fired — the solver missed an instance");

    // The PULL PROGRAM reaches firmware through the mux header: the
    // strap inputs are pull-less input-only nets → configured UP
    // (idle-high; the F103 entity declares no IO banks, so the 40k
    // is stated but not materialised into the DC solve — the info
    // line says so), and the pad serving uart1.TX stays OFF.
    let root = workspace_root();
    let hdr = std::env::temp_dir().join("bhdl_stm32_af_mux.h");
    let mut c = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    c.current_dir(&root)
        .arg("-I").arg(&root)
        .arg("tests/circuits/realistic/stm32_remap_solver.bhdl")
        .arg("doc").arg("--mux-header").arg(&hdr);
    let out = c.output().expect("spawn doc");
    assert!(out.status.success(), "doc --mux-header failed");
    let h = std::fs::read_to_string(&hdr).unwrap();
    assert!(h.contains("#define BHDL_MUX_MCU_UART1_ALT \"REMAP\""), "header lacks the AFIO choice:\n{h}");
    assert!(h.contains("#define BHDL_MUX_MCU_I2C1_SCL_PIN PB8"), "header lacks the remapped SCL pin");
    assert!(h.contains("#define BHDL_MUX_MCU_PA9_PULL UP"), "strap input not configured idle-high:\n{}",
        h.lines().filter(|l| l.contains("PA9")).collect::<Vec<_>>().join("\n"));
    assert!(h.contains("#define BHDL_MUX_MCU_PB6_PULL OFF"), "uart TX pad pull not OFF");
}

#[test]
fn f103_default_homes_stay_default() {
    // The Blue Pill board wires PB6/PB7 directly (no interface
    // fields) — the new alt tables must not disturb plain pin
    // wiring or the datasheet expansion network.
    let text = run_file("tests/circuits/realistic/stm32_blue_pill.bhdl");
    assert!(!text.contains("Error:"), "blue pill broke:\n{}",
        text.lines().rev().take(6).collect::<Vec<_>>().join("\n"));
    assert!(!text.contains("pinmux: mcu."), "solver ran with no wired alt fields:\n{}",
        text.lines().filter(|l| l.contains("pinmux")).collect::<Vec<_>>().join("\n"));
}
