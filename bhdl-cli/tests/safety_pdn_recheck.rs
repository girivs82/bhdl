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
