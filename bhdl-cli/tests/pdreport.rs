//! The professional PD report (`bhdl pdreport`, spec §9): every
//! section present, curves as inline SVG, findings carried verbatim.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

#[test]
fn pdreport_produces_all_sections() {
    let root = workspace_root();
    let dir = std::env::temp_dir().join("bhdl_pdreport_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let board = r#"
import { BuckBoost_TPS63020 } from "bhdl-stdlib/power/tps63020.bhdl";
import { Res } from "bhdl-stdlib/passives/resistor.bhdl";
import { Cap } from "bhdl-stdlib/passives/capacitor.bhdl";
entity RepSoc() {
    pin 1: power in;
    pin GND: ground;
    domain VDD pins="1" v=5V i_nom=200mA step=1A rise=10us dur=100us droop_max=3% source="FIXTURE — pdreport probe";
}
board RepBoard {
    power VBAT = 3.6V @ 8A;
    port V50: power out = 5V @ 1A;
    ground GND;
    @VBAT -> u1: BuckBoost_TPS63020(v_out=5V, i_out_max=1A, v_in=3.6V, v_in_min=3.0V, v_in_max=4.2V).VIN;
    u1.GND -> @GND; u1.VOUT -> @V50;
    @V50 -> C_bulk: Cap(220µF).1; C_bulk.2 -> @GND;
    soc: RepSoc();
    @V50 -> soc.1; soc.GND -> @GND;
}
"#;
    let f = dir.join("rep.bhdl");
    std::fs::write(&f, board).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    cmd.current_dir(&dir)
        .env_remove("BHDL_LIB_PATH")
        .args(["-I", root.to_str().unwrap()])
        .arg(&f)
        .args(["pdreport"]);
    let out = cmd.output().expect("spawn bhdl-cli");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let md = std::fs::read_to_string(dir.join("rep.pd.md")).expect("report written");
    for section in [
        "# Power-Delivery Report",
        "## 1. Power topology",
        "## 2. Requirement resolution",
        "## 3. Sizing",
        "## 4. Simulated curves",
        "## 5. Power-up timeline",
        "## 6. Power-down and sleep",
        "## 7.5 Stress sign-off",
        "## 8. Final PDN sanity",
        "<svg xmlns=",
        "BuckBoost_TPS63020",
        "θJA=41.8degC/W",
    ] {
        assert!(md.contains(section), "missing {section:?} in report:\n{}", &md[..md.len().min(2000)]);
    }
    // the load-step section carries the domain's declared trapezoid
    assert!(md.contains("soc.VDD") && md.contains("Self-droop"), "load-step table missing");
    // curves: at least power-up + input-loss captured
    assert!(md.matches("<svg").count() >= 2, "expected ≥2 curve charts");
}
