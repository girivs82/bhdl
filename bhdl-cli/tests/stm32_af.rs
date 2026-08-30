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

fn run_src(src: &str, name: &str) -> String {
    let dir = std::env::temp_dir().join("bhdl_stm32_af_test");
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join(name);
    std::fs::write(&f, src).unwrap();
    run_file(f.to_str().unwrap())
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
    // the strap inputs are pull-less input-only nets → idle-high,
    // and with the VDDIO bank declared the 40k MATERIALISES into
    // the DC solve (it was previously stated-only)
    assert!(text.contains("pull: mcu.PA9 → up"), "strap idle-high pull missing:\n{}",
        text.lines().filter(|l| l.contains("pull:")).collect::<Vec<_>>().join("\n"));
    assert!(text.contains("40000Ω modelled into the solve"), "internal pull not materialised against the bank rail");
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

#[test]
fn f103_ft_pins_gate_erc004() {
    // DS Doc ID 13587 Rev 12 Table 5: PB10 is FT (5V tolerant),
    // PA1 is a strict 3.3V pad. A 5V driver on the FT pad is
    // designed-in and silent; on the strict pad ERC004 refuses.
    let src = r#"
import { STM32F103Cx } from "bhdl-stdlib/actives/stm32f103cx.bhdl";
import { Cap } from "bhdl-stdlib/passives/capacitor.bhdl";
import { Res } from "bhdl-stdlib/passives/resistor.bhdl";

entity FiveVoltLogic() {
    pin VCC: power in;
    pin Y0:  signal out;
    pin Y1:  signal out;
    pin GND: ground;
    attribute component_class = "ic";
    attribute part_number = "FIXTURE-5V-LOGIC";
}

board FtProbe {
    power VDD = 3.3V @ 100mA;
    power V5  = 5V @ 100mA;
    ground GND;

    mcu: STM32F103Cx();
    lg:  FiveVoltLogic();

    @VDD -> mcu.VDD_1;
    @VDD -> mcu.VDD_2;
    @VDD -> mcu.VDD_3;
    @VDD -> mcu.VDDA;
    mcu.VSS_1 -> @GND;
    mcu.VSS_2 -> @GND;
    mcu.VSS_3 -> @GND;
    mcu.VSSA  -> @GND;
    mcu.VBAT  -> @VDD;
    @V5 -> lg.VCC;
    lg.GND -> @GND;

    lg.Y0 -> mcu.PB10;   // FT pad
    lg.Y1 -> mcu.PA1;    // strict pad
}
"#;
    let text = run_src(src, "ft_probe.bhdl");
    let erc = || text.lines().filter(|l| l.contains("ERC004")).collect::<Vec<_>>().join("\n");
    assert!(
        text.contains("net 'auto_lg_Y1'") && text.contains("mcu.PA1 @3.30V (bank)"),
        "5V into the strict pad not refused:\n{}", erc()
    );
    assert!(!erc().contains("auto_lg_Y0"), "5V into the FT pad falsely refused:\n{}", erc());
    // the F103 supply pins are DECLARED power/ground (no
    // wired-to-a-rail heuristic — one side of a pull-up touches the
    // rail too), so the bank-coverage advisory has nothing to nag
    assert!(!text.contains("belongs to NO declared IO bank"),
        "bank-coverage advisory nagged a correctly-typed entity:\n{}",
        text.lines().filter(|l| l.contains("ERC035")).collect::<Vec<_>>().join("\n"));
}

#[test]
fn signal_typed_rail_pin_is_an_error() {
    // The declaration is the truth: a domain whose rail pin is
    // declared `signal` is a modelling ERROR in the entity — never
    // papered over by looking at what net the pin touches.
    let src = r#"
entity BadRail() {
    pin VCC: signal inout;
    pin IO1: signal inout;
    pin GND: ground;
    attribute component_class = "ic";
    attribute part_number = "FIXTURE-BADRAIL";
    domain MAIN pins="VCC" v=3.3V i_nom=10mA i_max=50mA
        io_pins="IO1" source="FIXTURE";
}
entity RailPeer() {
    pin A: signal in;
    pin GND: ground;
    attribute component_class = "ic";
    attribute part_number = "FIXTURE-RAILPEER";
}
board RailProbe {
    power VDD = 3.3V @ 100mA;
    ground GND;
    u1: BadRail();
    p1: RailPeer();
    @VDD -> u1.VCC;
    u1.GND -> @GND;
    p1.GND -> @GND;
    u1.IO1 -> p1.A;
}
"#;
    let text = run_src(src, "bad_rail.bhdl");
    assert!(
        text.contains("'VCC' is declared `signal` — a bank's supply must be a power/ground pin"),
        "signal-typed rail pin not refused:\n{}",
        text.lines().filter(|l| l.contains("ERC035")).collect::<Vec<_>>().join("\n")
    );
    assert!(text.contains("IO bank discipline | Error"), "not an Error severity");
}
