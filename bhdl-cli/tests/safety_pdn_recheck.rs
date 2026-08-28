//! End-to-end `bhdl safety` on the PDN-recheck fixture
//! (Functional_Safety.md §2.11): a rail behind a REAL inductor, so
//! the decap bank carries the load-step transient — losing the bulk
//! cap is a DC no-op that defeats the droop contract. Guards two
//! separate regressions:
//!   1. the recheck itself: cap open/drift faults must fire the
//!      synthetic `pdn:` effect and classify RESIDUAL;
//!   2. the faulted-solve COMPLETE node map: the solver closure once
//!      returned the RENDERER's net_voltages (inductor-bridged nets
//!      unioned, internal side deleted), so effect predicates on the
//!      rail errored "no solved voltage" — invisible on drift rows,
//!      a note on open rows. The mock-solver unit test structurally
//!      cannot catch that; this spawned-binary run can.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

#[test]
fn safety_pdn_recheck_fires_and_dc_map_is_complete() {
    let root = workspace_root();
    let fixture = root.join("tests/circuits/realistic/test_safety_pdn_recheck.bhdl");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    cmd.current_dir(&root).arg(&fixture).arg("safety");
    let out = cmd.output().expect("spawn bhdl-cli");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // healthy board discharges the consumed contract
    assert!(
        text.contains("satisfied_by machine-verified: PDN checks pass"),
        "healthy PDN checks did not discharge the assumption:\n{}",
        text.lines().rev().take(30).collect::<Vec<_>>().join("\n")
    );
    // ...and the stale validation-time gap does not resurface
    assert!(
        !text.contains("ASSUMPTION_OPEN"),
        "discharged assumption still reported as an open gap:\n{text}"
    );
    // the recheck: bulk open fires the synthetic pdn effect, residual
    let open_row = text
        .lines()
        .find(|l| l.contains("c_bulk open(c_bulk)"))
        .unwrap_or_else(|| panic!("no c_bulk open row:\n{text}"));
    assert!(open_row.contains("pdn:soc.VDD"), "{open_row}");
    assert!(open_row.contains("RESIDUAL"), "{open_row}");
    assert!(open_row.contains("PDN contract VIOLATED"), "{open_row}");
    // the complete-map fix: no effect predicate lost its net voltage
    assert!(
        !text.contains("no solved voltage"),
        "faulted DC map is incomplete again (renderer net_voltages?):\n{}",
        text.lines().filter(|l| l.contains("no solved voltage")).collect::<Vec<_>>().join("\n")
    );
}

/// Load-release overshoot (spec addendum 9): the droop trace holds the
/// release edge — the energy the step pulled through the feed
/// inductance dumps into the bank when the load lets go. Healthy
/// fixture: bounded by the declared tol window and PASSES (the line
/// must exist — reading the top of the trace is the whole point); a
/// variant declaring overshoot_max=0.5% must VIOLATE, leave the pdn
/// assumption OPEN, and carry the AouViolated gap naming the remedy.
#[test]
fn load_release_overshoot_bound_and_violation() {
    let root = workspace_root();
    let src = std::fs::read_to_string(root.join("tests/circuits/realistic/test_safety_pdn_recheck.bhdl")).unwrap();
    // healthy: tol-window fallback, passes
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    cmd.current_dir(&root)
        .arg(root.join("tests/circuits/realistic/test_safety_pdn_recheck.bhdl"))
        .arg("safety");
    let out = cmd.output().expect("spawn");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let rel = text.lines().find(|l| l.contains("release: overshoot")).unwrap_or_else(|| panic!("no release line:\n{text}"));
    assert!(rel.contains("(tol window)") && rel.contains("OK"), "healthy release must pass on the tol window: {rel}");

    // tight bound: violates, assumption stays OPEN
    let dir = std::env::temp_dir().join("bhdl_overshoot_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let tight = src.replace("droop_max=4%", "droop_max=4% overshoot_max=0.5%");
    assert_ne!(tight, src, "fixture shape changed — update the replace");
    let f = dir.join("over.bhdl");
    std::fs::write(&f, tight).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    cmd.current_dir(&root).arg("-I").arg(&root).arg(&f).arg("safety");
    let out = cmd.output().expect("spawn");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(
        text.contains("load-release overshoot") && text.contains("VIOLATED"),
        "tight overshoot bound did not violate:\n{}",
        text.lines().filter(|l| l.contains("release")).collect::<Vec<_>>().join("\n")
    );
    assert!(
        !text.contains("satisfied_by machine-verified: PDN checks pass"),
        "assumption must NOT discharge under an overshoot violation"
    );
}

/// Fault-at-boot, end to end (spec §2.12) on the PG-chained cascade:
/// the RC-enable cap open is a DC no-op (the settled solve models no
/// enable gating) yet V33 never starts — the campaign must classify
/// it boot-dangerous; and the R_del short is a pure ORDERING
/// violation (slot 2 good before slot 1 completes), a hazard class
/// only the timeline can see.
#[test]
fn fault_at_boot_campaign_classifies_startup_breakers() {
    let root = workspace_root();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    cmd.current_dir(&root)
        .arg(root.join("tests/circuits/realistic/test_safety_boot.bhdl"))
        .arg("safety");
    let out = cmd.output().expect("spawn");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let row = |needle: &str| -> &str {
        text.lines().find(|l| l.contains(needle)).unwrap_or_else(|| panic!("no row {needle}:\n{}", text.lines().filter(|l| l.contains("boot:")).collect::<Vec<_>>().join("\n")))
    };
    // enable-path cap SHORT grounds EN: DC-benign, boot-dangerous
    let cdel = row("C_del short(C_del.1, C_del.2)");
    assert!(cdel.contains("boot:soc.VDD_AUX") && cdel.contains("RESIDUAL"), "{cdel}");
    assert!(cdel.contains("dangerous at start-up"), "{cdel}");
    // enable-path cap OPEN only removes the DELAY: the PG still
    // drives EN, the rail starts promptly and IN ORDER — benign for
    // this contract (the physics review that fixed the engine: a
    // mode-based PG starved the algebraic EN re-evaluation and called
    // the rail NEVER-good; PG is voltage-based now)
    assert!(
        !text.lines().any(|l| l.contains("C_del open(C_del)") && l.contains("boot:")),
        "C_del open must be boot-benign:\n{}",
        text.lines().filter(|l| l.contains("C_del open")).collect::<Vec<_>>().join("\n")
    );
    // R_del short: the ORDERING violation (slot 2 before slot 1)
    let rdel = row("R_del short(R_del.1, R_del.2)");
    assert!(rdel.contains("boot:soc") && rdel.contains("before slot 1"), "{rdel}");
}
