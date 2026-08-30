//! ERC034 swizzle discipline: the realised interface permutation is
//! reconciled against the declared swizzle freedoms. The legal fixture
//! (byte swap + bit reversal, both declared) reports the as-built
//! table as Info with zero errors; the violations fixture trips every
//! class — CA cross-wire, DQS polarity swap, byte split, and a lane
//! swap without the across-bytes grant.
//!
//! Also pins the CLI plumbing this increment flushed: the v0.8
//! parametric resolver (interface params + generate-loop unrolling —
//! the swizzle permutation tables) previously ran only in
//! `synthesize_from_source` and the test binaries; the main CLI path
//! never saw it, so these fixtures could not even parse.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn run(fixture: &str) -> String {
    let root = workspace_root();
    let mut c = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
    c.current_dir(&root).arg("-I").arg(&root).arg(fixture).arg("synthesize");
    let out = c.output().expect("spawn");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

fn erc034_lines(text: &str) -> Vec<&str> {
    let mut v: Vec<&str> = text.lines().filter(|l| l.contains("ERC034") && l.starts_with('|')).collect();
    v.sort();
    v.dedup();
    v
}

#[test]
fn legal_swizzle_reports_as_built_table_no_errors() {
    let text = run("tests/circuits/realistic/test_ddr_swizzle.bhdl");
    let rows = erc034_lines(&text);
    assert!(
        !rows.iter().any(|l| l.contains("| Error |")),
        "legal permutation flagged:\n{}",
        rows.join("\n")
    );
    let info = rows
        .iter()
        .find(|l| l.contains("as-built swizzle"))
        .unwrap_or_else(|| panic!("no as-built Info row:\n{}", rows.join("\n")));
    // the recovered permutation: lane swap + the DQ0↔DQ3 bit reversal
    assert!(info.contains("ddr.lane0→ddr.lane1"), "lane map missing: {info}");
    assert!(info.contains("ddr.lane0.DQ0↔ddr.lane1.DQ3"), "bit swap missing: {info}");
}

#[test]
fn violations_fixture_trips_every_class() {
    let text = run("tests/circuits/erc/erc034_swizzle_violations.bhdl");
    let rows = erc034_lines(&text);
    let errors: Vec<&&str> = rows.iter().filter(|l| l.contains("| Error |")).collect();
    let has = |needle: &str| errors.iter().any(|l| l.contains(needle));
    // 1. CA cross-wire (no freedom declared)
    assert!(has("declares no swizzle freedom"), "CA cross-wire missed:\n{}", rows.join("\n"));
    // 2. DQS polarity swap (non-member relative-path mismatch)
    assert!(has("strobe polarity / non-member"), "polarity swap missed:\n{}", rows.join("\n"));
    // 3. byte split
    assert!(has("splits across"), "byte split missed:\n{}", rows.join("\n"));
    // 4. lane swap without across-bytes grant
    assert!(has("does not declare swizzle_across_bytes"), "across gate missed:\n{}", rows.join("\n"));
}
