//! Safety-case capstone report (spec §2.14): one markdown document
//! for the assessor — goals, claimed-vs-measured mechanisms, the
//! measured fault universe (synthetic pdn:/boot: effects and
//! transient detection included), metrics, PDN discharge, DFA and the
//! gap register. Rendered from the model, never re-derived, and
//! byte-deterministic.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

#[test]
fn safety_case_report_renders_all_sections_deterministically() {
    let root = workspace_root();
    // the transient-detection variant: supervisor mechanism + PDN
    // domain — the report's richest inputs (measured universe with
    // pdn: effects, transient detection, discharge lines, DFA)
    let src = std::fs::read_to_string(root.join("tests/circuits/realistic/test_safety_pdn_recheck.bhdl")).unwrap();
    let with_mon = src.replace(
        r#"    goal SG: ASIL_B "rail stays in window" (id="SG-PDNRC-1") {
        effect uv = brd.r_load.1 < 10V severity S2;
    }"#,
        r#"    goal SG: ASIL_B "rail stays in window" (id="SG-PDNRC-1") {
        effect uv = brd.r_load.1 < 10V severity S2;
    }
    mechanism brd.r_load: psm(SG, detects=[uv], detected_when = brd.r_load.1 < 11.4V, dc=0.9, source="FIXTURE supervisor");"#,
    );
    assert_ne!(with_mon, src);
    let dir = std::env::temp_dir().join("bhdl_safety_report_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("sr.bhdl");
    std::fs::write(&f, with_mon).unwrap();
    let render = |out: &str| -> String {
        let rp = dir.join(out);
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_bhdl-cli"));
        cmd.current_dir(&root)
            .arg("-I")
            .arg(&root)
            .arg(&f)
            .arg("safety")
            .arg("--report")
            .arg(&rp);
        let o = cmd.output().expect("spawn");
        assert!(
            std::fs::metadata(&rp).is_ok(),
            "report not written:\n{}",
            String::from_utf8_lossy(&o.stdout)
        );
        std::fs::read_to_string(&rp).unwrap()
    };
    let md = render("a.md");
    // every section present (7 PDN included — this board has a domain)
    for sec in [
        "# Safety case — PdnFaultProbe",
        "## 1 Mission profile & scope",
        "## 2 Safety goals",
        "## 3 Safety mechanisms — claimed vs measured",
        "## 4 Parts and failure data",
        "## 5 Fault universe — measured campaign",
        "## 6 Hardware architectural metrics",
        "## 7 PDN contract — machine verification",
        "## 8 Dependent-failure analysis",
        "## 9 Assumptions of use",
        "## 10 Gap register",
    ] {
        assert!(md.contains(sec), "missing section {sec}:\n{}", md.lines().filter(|l| l.starts_with("##") || l.starts_with("# ")).collect::<Vec<_>>().join("\n"));
    }
    // the measured story is IN the document: synthetic pdn: effect,
    // transient-visible detection, the discharge line, and the DFA
    // self-monitoring finding on the fixture's shortcut
    assert!(md.contains("pdn:soc.VDD"), "synthetic effect missing");
    assert!(md.contains("(transient)"), "transient detection missing");
    assert!(md.contains("assume pdn"), "PDN discharge story missing");
    assert!(md.contains("DF-DIE") || md.contains("DEPENDENT_FAILURE"), "DFA missing");
    // no terminal color escapes leak into markdown
    assert!(!md.contains('\u{1b}'), "ANSI escapes leaked into the report");
    // byte-deterministic
    let md2 = render("b.md");
    assert_eq!(md, md2, "report is not byte-deterministic");
}
