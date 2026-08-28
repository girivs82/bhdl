//! Front-end synthesis + inrush report (spec §7.5 addendum 10): the
//! board-text `requirements { front_end: "..." }` plans a protected
//! front end for every non-always-on rail, the recognized axis tokens
//! flow into the emitted PreregStage REQUIREMENT (so the acceptance
//! gates verify them against real protection blocks), and the inrush
//! report states what bounds each bank's plug-in charge — or names
//! the gap.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

#[test]
fn front_end_requirement_plans_axes_and_inrush() {
    let root = workspace_root();
    let dir = std::env::temp_dir().join("bhdl_front_end_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let board = r#"
board FrontEndDemo {
    requirements { front_end: "reverse_polarity, ov_trip=9V, uv_trip=2.7V"; source_r: "100m"; }
    power VBAT = 3.6V @ 10A;
    port V50: power out = 5V @ 5A;
    ground GND;
    soc: FeSoc();
    @V50 -> soc.1; soc.GND -> @GND;
}
entity FeSoc() {
    pin 1: power in;
    pin GND: ground;
    domain VDD_MAIN pins="1" v=5V i_nom=200mA i_max=1A source="FIXTURE — front-end probe";
}
"#;
    let f = dir.join("fe.bhdl");
    std::fs::write(&f, board).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    cmd.current_dir(&dir)
        .env_remove("BHDL_LIB_PATH")
        .args(["-I", root.to_str().unwrap()])
        .arg(&f)
        .args(["powertree", "--input", "VBAT", "--emit", "1"]);
    let out = cmd.output().expect("spawn bhdl-cli");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("front_end requirement: protected front end planned"),
        "requirement did not trigger planning:\n{}",
        text.lines().rev().take(15).collect::<Vec<_>>().join("\n")
    );
    let emitted = std::fs::read_to_string(&f).unwrap();
    // the protected rail exists and the downstream stage feeds from it
    assert!(emitted.contains("power V_PROT"), "no protected rail:\n{emitted}");
    assert!(emitted.contains("@V_PROT -> u_v50.VIN;"), "stage not re-fed from the protected rail:\n{emitted}");
    // the axis tokens became REQUIREMENT arguments (acceptance-gated)
    let req = emitted
        .lines()
        .find(|l| l.contains("u_v_prot: PreregStage("))
        .unwrap_or_else(|| panic!("no prereg requirement:\n{emitted}"));
    assert!(
        req.contains("reverse_polarity=true") && req.contains("ov_trip=9V") && req.contains("uv_trip=2.7V"),
        "protection axes missing from the requirement: {req}"
    );
    // inrush: the bank behind the front end has no declared current
    // limit — a NAMED gap with the remedy, never a silent pass
    assert!(
        text.contains("inrush:") && text.contains("declares NO current limit") && text.contains("i_lim=<A>"),
        "inrush gap not stated:\n{}",
        text.lines().filter(|l| l.contains("inrush")).collect::<Vec<_>>().join("\n")
    );
    // the soft-start hand-off is stated, not re-estimated
    assert!(
        text.contains("soft-start-limited — verified by the power-up timeline"),
        "downstream hand-off missing"
    );
}
