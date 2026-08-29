//! Multi-supply i_max tracing (spec §2.10, the last recorded
//! increment): each domain's i_max walks up its stage chain to its
//! ROOT source — linear stages pass current, switching stages
//! input-refer through the voltage ratio and the DECLARED efficiency
//! (else an optimistic LOWER BOUND, stated) — and per-source sums
//! are judged against each rating. Hand math throughout.

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
    let f = dir.join("ms.bhdl");
    std::fs::write(&f, board).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    cmd.current_dir(&root).arg("-I").arg(&root).arg(&f).arg("safety");
    let out = cmd.output().expect("spawn");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

#[test]
fn multi_supply_tracing_attributes_and_gates() {
    let root = workspace_root();
    let src = std::fs::read_to_string(root.join("tests/circuits/realistic/test_safety_multisupply.bhdl")).unwrap();

    // healthy: boost 1A → 1×5/3.6/0.85 = 1.63A on VBAT (rated 2A);
    // LDO 0.1A passes through exactly onto V12 (rated 1A)
    let text = run(&src, "bhdl_ms_ok_test");
    assert!(
        text.contains("fed from VBAT via u1 (×5V/3.6V/η85%) — input-referred i_max 1.63A"),
        "boost attribution wrong:\n{}",
        text.lines().filter(|l| l.contains("supply")).collect::<Vec<_>>().join("\n")
    );
    assert!(
        text.contains("(linear, I passes) — input-referred i_max 0.10A"),
        "LDO must pass current through exactly:\n{}",
        text.lines().filter(|l| l.contains("supply")).collect::<Vec<_>>().join("\n")
    );
    assert!(text.contains("SOURCE VBAT rated 2A: Σ input-referred i_max 1.63A") && text.contains("→ OK"), "VBAT sum");
    assert!(text.contains("SOURCE V12 rated 1A: Σ input-referred i_max 0.10A"), "V12 sum");

    // violation: VDD_5V i_max 1.5A → 2.45A > the 2A VBAT rating
    let hot = src.replace("i_nom=200mA i_max=1A source=", "i_nom=200mA i_max=1.5A source=");
    assert_ne!(hot, src);
    let text = run(&hot, "bhdl_ms_hot_test");
    assert!(
        text.contains("Σ input-referred i_max 2.45A") && text.contains("VIOLATED"),
        "over-budget source not violated:\n{}",
        text.lines().filter(|l| l.contains("SOURCE")).collect::<Vec<_>>().join("\n")
    );
    assert!(
        text.contains("exceeds the VBAT rating 2A"),
        "capability gap missing"
    );

    // efficiency FALLBACK: with the emitter's assumption stripped,
    // the block's own declared `efficiency` (95 %) carries the chain
    // — real block data, pinned: 1×5/3.6/0.95 = 1.46A
    let bare = src.replace("    attribute u1.powertree_eff_assumed_pct = \"85\";\n", "");
    assert_ne!(bare, src);
    let text = run(&bare, "bhdl_ms_effattr_test");
    assert!(
        text.contains("η95%") && text.contains("input-referred i_max 1.4"),
        "block-declared efficiency fallback wrong:\n{}",
        text.lines().filter(|l| l.contains("supply")).collect::<Vec<_>>().join("\n")
    );

    // bound-only: a stage with NO efficiency data anywhere — ideal
    // ratio as an optimistic LOWER BOUND, ≥ in the sum, never a pass
    let raw = r#"
entity RawBoost() {
    pin VIN: power in;
    pin VOUT: power out;
    pin GND: ground;
    attribute component_class = "regulator_ic";
    attribute output_voltage  = 5V;
}

entity RbSoc() {
    pin 1: power in;
    pin GND: ground;
    domain VDD pins="1" v=5V tol=5% i_nom=200mA i_max=1A source="FIXTURE";
}

board RawBoostBoard {
    power VBAT = 3.6V @ 2A;
    port V50: power out = 5V @ 1A;
    ground GND;
    @VBAT -> u1: RawBoost().VIN;
    u1.GND -> @GND; u1.VOUT -> @V50;
    soc: RbSoc();
    @V50 -> soc.1; soc.GND -> @GND;
}

safety RawBoostBoard as brd {
    mission { ambient = 40degC; lifetime = 15000h; }
    goal SG: ASIL_B "rail holds" (id="SG-RB-1") {
        effect uv = brd.soc.1 < 4.5V severity S2;
    }
}
"#;
    let text = run(raw, "bhdl_ms_bound_test");
    assert!(
        text.contains("η UNDECLARED — ideal, lower bound"),
        "bound basis missing:\n{}",
        text.lines().filter(|l| l.contains("supply")).collect::<Vec<_>>().join("\n")
    );
    assert!(
        text.contains("Σ input-referred i_max ≥ 1.39A") && text.contains("BOUND ONLY"),
        "bound-only verdict wrong:\n{}",
        text.lines().filter(|l| l.contains("SOURCE")).collect::<Vec<_>>().join("\n")
    );
}
