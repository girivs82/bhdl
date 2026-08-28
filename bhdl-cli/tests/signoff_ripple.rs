//! Capacitor ripple-current sign-off (spec §7.5 addendum 8) on the
//! TPS54331 demo board — and, inseparably, the composed-parent fix:
//! vendor `stress { }` recipes that reference expansion children
//! (`L_out.value`, `C_out.i_ripple_applied`) were silently SKIPPED on
//! composed design blocks, because the child map matched only the S4
//! path's `expansion_parent` attribute while composition stamps
//! `composed_parent`. Every "(stress block)" assertion here is a
//! canary for that whole path.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

#[test]
fn ripple_current_rows_and_composed_stress_path() {
    let root = workspace_root();
    let fixture = root.join("tests/circuits/realistic/buck_converter_tps54331.bhdl");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    cmd.current_dir(&root).arg(&fixture).arg("report");
    let out = cmd.output().expect("spawn bhdl-cli");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // the composed-block vendor stress path fires at all (canary):
    // the block's own ripple-voltage and inductor-peak assignments
    // reach the sign-off table as "(stress block)" rows
    assert!(
        text.contains("ΔV=") && text.contains("(stress block)"),
        "vendor stress recipe did not apply to the composed block:\n{}",
        text.lines().filter(|l| l.contains("U1_C_out")).collect::<Vec<_>>().join("\n")
    );
    assert!(
        text.contains("I_pk=") && text.contains("P_pass="),
        "inductor-peak / pass-dissipation stress rows missing:\n{}",
        text.lines().filter(|l| l.contains("stress")).collect::<Vec<_>>().join("\n")
    );
    // the new axis: per-cap RMS switching-ripple rows, with the
    // buck forms — input cap chops the load (≈1A here), output cap
    // carries only the inductor ripple (an order of magnitude less)
    let irms: Vec<&str> = text.lines().filter(|l| l.contains("| Irms |")).collect();
    assert!(
        irms.iter().any(|l| l.contains("U1_C_in") && l.contains("I_rms=0.9")),
        "input-cap ripple row wrong or missing: {irms:?}"
    );
    assert!(
        irms.iter().any(|l| l.contains("U1_C_out") && l.contains("I_rms=0.1")),
        "output-cap ripple row wrong or missing: {irms:?}"
    );
    // bare Caps declare no ripple_current rating — the axis reports
    // UNCHECKED (an undeclared rating is not infinite), never a pass
    assert!(
        text.contains("axis Irms") && text.contains("Unchecked axis"),
        "missing UNCHECKED statement for unrated ripple rows:\n{}",
        text.lines().filter(|l| l.contains("Irms")).collect::<Vec<_>>().join("\n")
    );
}
