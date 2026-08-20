//! Integration test for the Phase-3 fault campaign
//! (docs/spec/Functional_Safety.md §2.5) on the FaultDemo fixture, with
//! a MOCK solver (net → volts) so no spice runs here: the mutation
//! machinery, effect-predicate evaluation, classification and gap
//! regeneration are exercised; the real GLACIER solve is exercised by
//! `bhdl-cli safety` on the same fixture.

use std::collections::HashMap;

use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_common::safety::GapClass;
use bhdl_parser::parse;
use bhdl_synthesizer::fault_campaign::run_declared_faults;
use bhdl_synthesizer::safety_model::build_safety_model;
use bhdl_synthesizer::NetlistGenerator;

#[tokio::test]
async fn declared_faults_run_classify_and_regenerate_gaps() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let src = std::fs::read_to_string(ws.join("tests/circuits/realistic/test_safety_fault_campaign.bhdl")).unwrap();
    let pr = parse(&src);
    assert!(pr.errors().is_empty(), "parse: {:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let netlist = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let mut model = build_safety_model(&netlist, &[&sf]);
    assert!(model.errors.is_empty(), "errors: {:#?}", model.errors);
    let unrun_before = model.gaps.iter().filter(|g| g.class == GapClass::FaultUnrun).count();
    assert_eq!(unrun_before, 4);

    // Mock solver: every net at 12 V except GND. The short faults are
    // still classified PHYSICALLY under this mock, because the net-alias
    // machinery follows the merge: short(r_bot) merges the mid node into
    // the surviving GND rail, so the `brd.r_bot.1` predicate reads 0 V
    // and UNDERVOLTAGE fires — proving resolution against the healthy
    // connectivity + alias translation, not the mock's flat 12 V.
    let solve = |faulted: &bhdl_netlist::Netlist| -> Result<HashMap<String, f64>, String> {
        Ok(faulted
            .nets
            .iter()
            .filter_map(|(_, n)| n.name.clone())
            .map(|name| {
                let v = if name == "GND" { 0.0 } else { 12.0 };
                (name, v)
            })
            .collect())
    };
    let (ran, mismatched) = run_declared_faults(&netlist, &mut model, &solve);
    assert_eq!(ran, 4);
    assert_eq!(mismatched, 0, "under this mock every expectation holds (aliasing makes the shorts physical)");

    let scope = &model.scopes[0];
    for f in &scope.faults {
        assert!(f.run, "{}({:?}) must have run", f.kind, f.targets);
        assert_eq!(f.expectation_met, Some(true));
    }
    // The alias proof: short(r_bot) merged the mid node into GND, so the
    // undervoltage effect fired from the SURVIVING net's 0 V.
    let uv = scope.faults.iter().find(|f| f.expect.contains("undervoltage")).unwrap();
    assert_eq!(uv.fired, vec!["SG_MID.undervoltage".to_string()]);

    // Gap regeneration: all 4 placeholders cleared.
    let unrun_after: Vec<_> = model.gaps.iter().filter(|g| g.class == GapClass::FaultUnrun).collect();
    assert!(unrun_after.is_empty(), "{unrun_after:#?}");

    // ── Whole-universe campaign on the same mock: 4 parts × 2 modes
    // (+0 states). Under the all-12V mock with alias-following, the
    // classification is deterministic; measured DC exists for the
    // mechanism because the fixture declares detected_when.
    bhdl_synthesizer::fault_campaign::run_universe(&netlist, &mut model, &solve);
    assert_eq!(model.universe.len(), 8, "{:#?}", model.universe);
    assert!(model.universe.iter().all(|u| u.ran));
    let dangerous = model.universe.iter().filter(|u| !u.fired.is_empty()).count();
    assert!(dangerous >= 2, "at least the r_bot short (aliased to GND) and more fire: {dangerous}");
    let mech = &model.scopes[0].mechanisms[0];
    assert!(mech.measured_dc.is_some(), "detected_when declared ⇒ measured DC exists: {:?}", mech.measured_note);
    assert!(mech.measured_note.as_deref().unwrap_or("").contains("basis"));
    // every universe fault carries its λ share (all parts have FIT... in
    // this test reliability did NOT run, so weights are None and the
    // basis is count — both stated)
    assert!(model.universe.iter().all(|u| u.weight_fit.is_none()));
    assert!(mech.measured_note.as_deref().unwrap_or("").contains("count"));

    // Negative control: a solver that refuses leaves ran-without-verdict
    // gaps that say so (the fault is NOT silently classified).
    let mut model2 = build_safety_model(&netlist, &[&sf]);
    let refuse = |_: &bhdl_netlist::Netlist| -> Result<HashMap<String, f64>, String> { Err("no convergence".into()) };
    let (ran2, _) = run_declared_faults(&netlist, &mut model2, &refuse);
    assert_eq!(ran2, 4);
    let unrun2: Vec<_> = model2.gaps.iter().filter(|g| g.class == GapClass::FaultUnrun).collect();
    assert_eq!(unrun2.len(), 4);
    assert!(unrun2.iter().all(|g| g.fix.contains("without verdict")));
}
