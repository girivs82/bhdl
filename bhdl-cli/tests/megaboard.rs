//! The megaboard dogfood fixture (FPGA + DDR4, full feature surface):
//! regression-pins the power-up physics the board flushed out —
//! placeholder stages ARE simulated (vout_nom discovery), direct
//! PG→EN chains gate as wired-AND, UVLO holds stages off below their
//! input floor, soft-start caps placeholder inrush, and an Off stage
//! does not hold its bank up against downstream draw on input loss.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn run(cmd: &str) -> String {
    let root = workspace_root();
    let mut c = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    c.current_dir(&root)
        .arg("-I")
        .arg(&root)
        .arg("tests/circuits/realistic/test_megaboard_fpga_ddr4.bhdl")
        .arg(cmd);
    let out = c.output().expect("spawn");
    // STDOUT only: the findings-section parsing below anchors on the
    // last "findings:" — appending stderr (build logs, survey rows)
    // after it would pollute the section
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn megaboard_powerup_ladder_is_real_and_windows_hold() {
    let text = run("powerup");
    // every rail reaches good on the timeline (REAL blocks: committed
    // TPS26630/TPS54560/TPS54302/BuckController+CSD87350 stages
    // simulate with datasheet soft-starts and EN thresholds)
    for rail in ["V_PROT GOOD", "VCCINT GOOD", "V18 GOOD", "V12Q GOOD", "VTT GOOD", "VPP GOOD"] {
        assert!(text.contains(rail), "missing {rail}:\n{}",
            text.lines().filter(|l| l.contains("GOOD") || l.contains("NEVER")).collect::<Vec<_>>().join("\n"));
    }
    let t_of = |rail: &str| -> f64 {
        text.lines()
            .find(|l| l.contains(&format!("{rail} GOOD")))
            .and_then(|l| l.trim().split("ms").next())
            .and_then(|t| t.trim().parse::<f64>().ok())
            .unwrap_or(f64::NAN)
    };
    // the DECLARED orderings realized on the timeline: CORE before AUX
    // (t_min 0.2ms), AUX before IO, VPP before VDD12, VDD12 before VTT
    // (t_min 0.1ms) — V_PROT itself goes good late (its 1.5mF bank
    // keeps charging while it already feeds the ladder; physical)
    let (tc, t18, t12q, tpp, tvtt) =
        (t_of("VCCINT"), t_of("V18"), t_of("V12Q"), t_of("VPP"), t_of("VTT"));
    assert!(tc + 0.2 <= t18, "AUX t_min after CORE: {tc} {t18}");
    assert!(t18 < t12q, "IO before AUX: {t18} {t12q}");
    assert!(tpp < t12q, "VDD12 before VPP: {tpp} {t12q}");
    assert!(t12q + 0.1 <= tvtt, "VTT t_min after VDD12: {t12q} {tvtt}");
    // no Error finding on the timeline (the findings block carries the
    // cross rows; the stage-survey near-miss crosses live before it)
    // the section header is a lone "  findings:" line; absent header
    // (a clean run omits it) = no findings. Ends at the first blank.
    let findings: Vec<&str> = match text.lines().collect::<Vec<_>>().iter().rposition(|l| l.trim() == "findings:") {
        Some(i) => text
            .lines()
            .skip(i + 1)
            .take_while(|l| !l.trim().is_empty())
            .collect(),
        None => Vec::new(),
    };
    assert!(
        !findings.iter().any(|l| l.contains('\u{2717}')),
        "error findings:\n{}",
        findings.join("\n")
    );
    assert!(!text.contains("TRUNCATED"), "sim truncated");
}

#[test]
fn megaboard_powerdown_discharges_through_draw_and_bleed() {
    let text = run("powerdown");
    // input loss: the dead front-end must NOT hold its bank — every
    // rail discharges through downstream draw + the VTT bleed
    for rail in ["V18 DOWN", "VTT DOWN", "VCCINT DOWN", "V12Q DOWN"] {
        assert!(text.contains(rail), "missing {rail}:\n{}",
            text.lines().filter(|l| l.contains("DOWN") || l.contains("✗")).collect::<Vec<_>>().join("\n"));
    }
    assert!(
        !text.contains("never discharged"),
        "discharge finding should be closed by the bleed:\n{}",
        text.lines().filter(|l| l.contains("✗")).collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn megaboard_stages_resolved_to_real_parts() {
    let text = run("synthesize");
    for (inst, block) in [
        ("u_v_prot", "Efuse_TPS26630"),
        ("u_v12q", "Buck_TPS54560"),
        ("u_v18", "Buck_TPS54302"),
        ("u_vpp", "Buck_TPS54302"),
        ("u_vtt", "Buck_TPS54302"),
        ("u_vccint", "BuckController"),
    ] {
        assert!(
            text.contains(&format!("{inst}: ")) || text.contains(block),
            "{inst} -> {block} not bound"
        );
    }
    // no stage left Generic
    assert!(!text.contains("is still the Generic"), "unresolved stage:\n{}",
        text.lines().filter(|l| l.contains("ERC032")).collect::<Vec<_>>().join("\n"));
    // SoC features on the board: the debug UART is forced OFF its
    // preferred home by the directly-wired sideband straps; PGOOD is
    // held down by the designer override; the strap inputs auto
    // pull-up; the banks are powered (no ERC035 Error)
    assert!(text.contains("pinmux: fpga.dbg"), "mux not solved");
    assert!(text.contains("alt \"IOB\""), "straps did not force the alternate:\n{}",
        text.lines().filter(|l| l.contains("pinmux")).collect::<Vec<_>>().join("\n"));
    assert!(text.contains("pull: fpga.PGOOD_IN → down (designer override)"), "PGOOD hold-down missing");
    assert!(text.contains("pull: fpga.DBG_A → up"), "strap auto pull-up missing");
    assert!(!text.contains("IO bank discipline | Error"), "bank error:\n{}",
        text.lines().filter(|l| l.contains("ERC035")).collect::<Vec<_>>().join("\n"));
}

#[test]
fn megaboard_safety_catches_the_planted_clock_violation() {
    let text = run("safety");
    // the 100MHz oscillator against the DRAM's 1GHz clock_in contract
    assert!(text.contains("dram.CK_1G") && text.contains("VIOLATED"), "clock probe not caught");
    // SEooC attestations compose
    assert!(text.contains("270.0 FIT composed from SEooC vendor ATTESTATIONS"), "attestation");
    // route 1H rows on the SIL goal
    assert!(text.contains("route 1H"), "route-1H row missing");
}
