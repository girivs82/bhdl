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
    assert_eq!(unrun_before, 6); // 4 board faults + 1 vendor state + 1 drift

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
    assert_eq!(ran, 6); // state runs via its behavior, drift via value scaling
    // Under the mock, aliasing makes the short/open expectations hold
    // and the state's ov fires (12V everywhere) — but the DRIFT fault
    // expects UNDERVOLTAGE, which an all-12V mock can never produce:
    // exactly one honest mismatch.
    assert_eq!(mismatched, 1, "{:#?}", model.scopes[0].faults);

    let scope = &model.scopes[0];
    for f in &scope.faults {
        assert!(f.run, "{}({:?}) must have run", f.kind, f.targets);
        let expect_met = f.kind != "drift"; // see mock note above
        assert_eq!(f.expectation_met, Some(expect_met), "{}({:?})", f.kind, f.targets);
    }
    // the vendor state ran via its behavior (no 'needs the hook' note)
    let st = scope.faults.iter().find(|f| f.kind == "state").unwrap();
    assert!(st.run && st.note.is_none(), "{st:?}");
    // drift ran via value scaling
    let dr = scope.faults.iter().find(|f| f.kind == "drift").unwrap();
    assert!(dr.run && dr.note.is_none(), "{dr:?}");
    // FTTI: 1ms latency vs within 10ms = OK; vs within 500us = FAILED
    let t_ok = scope.faults.iter().find(|f| f.within.as_deref() == Some("10ms")).unwrap();
    assert_eq!(t_ok.timing_met, Some(true), "{t_ok:?}");
    let t_bad = scope.faults.iter().find(|f| f.within.as_deref() == Some("500us")).unwrap();
    assert_eq!(t_bad.timing_met, Some(false), "{t_bad:?}");
    // The alias proof: short(r_bot) merged the mid node into GND, so the
    // undervoltage effect fired from the SURVIVING net's 0 V.
    let uv = scope.faults.iter().find(|f| f.expect.contains("undervoltage")).unwrap();
    assert_eq!(uv.fired, vec!["SG_MID.undervoltage".to_string()]);

    // Gap regeneration: two gaps survive the mock — the 500µs FTTI
    // (mechanism budget 1ms regardless of mock voltages) and the drift
    // mismatch above.
    let unrun_after: Vec<_> = model.gaps.iter().filter(|g| g.class == GapClass::FaultUnrun).collect();
    assert_eq!(unrun_after.len(), 2, "{unrun_after:#?}");
    assert!(unrun_after.iter().any(|g| g.fix.contains("FTTI")));
    assert!(unrun_after.iter().any(|g| g.subject.contains("drift")));

    // ── Whole-universe campaign on the same mock: 4 parts × 2 modes
    // (+0 states). Under the all-12V mock with alias-following, the
    // classification is deterministic; measured DC exists for the
    // mechanism because the fixture declares detected_when.
    bhdl_synthesizer::fault_campaign::run_universe(&netlist, &mut model, &solve);
    // 3 generic 2-pin parts × 2 modes + DemoSense's 2 VENDOR states
    assert_eq!(model.universe.len(), 8, "{:#?}", model.universe);
    let states: Vec<_> = model.universe.iter().filter(|u| u.mode == "state").collect();
    assert_eq!(states.len(), 2);
    // vendor states carry their REAL fit shares as λ weights
    assert!(states.iter().all(|u| u.weight_fit == Some(6.0) || u.weight_fit == Some(4.0)), "{states:#?}");
    assert!(model.universe.iter().all(|u| u.ran));
    let dangerous = model.universe.iter().filter(|u| !u.fired.is_empty()).count();
    assert!(dangerous >= 2, "at least the r_bot short (aliased to GND) and more fire: {dangerous}");
    let mech = &model.scopes[0].mechanisms[0];
    assert!(mech.measured_dc.is_some(), "detected_when declared ⇒ measured DC exists: {:?}", mech.measured_note);
    assert!(mech.measured_note.as_deref().unwrap_or("").contains("basis"));
    // every universe fault carries its λ share (all parts have FIT... in
    // this test reliability did NOT run, so weights are None and the
    // basis is count — both stated)
    // generic modes have no λ here (reliability didn't run in this test)
    assert!(model.universe.iter().filter(|u| u.mode != "state").all(|u| u.weight_fit.is_none()));
    assert!(mech.measured_note.as_deref().unwrap_or("").contains("count"));

    // ── Metrics on the mock universe: weights are None (reliability did
    // not run here) ⇒ measurement INCOMPLETE ⇒ ASIL_B targets cannot
    // pass and a METRIC_MISSED gap says why. No silent normalization.
    bhdl_synthesizer::fault_campaign::compute_metrics(&mut model);
    let m = model.scopes[0].metrics.as_ref().expect("metrics computed");
    assert_eq!(m.unmeasured_faults, 6, "generic modes lack λ shares in the mock run; the 2 vendor states carry theirs");
    assert_eq!(m.pass, Some(false));
    assert!(model.gaps.iter().any(|g| g.class == GapClass::MetricMissed && g.fix.contains("unmeasured")));

    // Negative control: a solver that refuses leaves ran-without-verdict
    // gaps that say so (the fault is NOT silently classified).
    let mut model2 = build_safety_model(&netlist, &[&sf]);
    let refuse = |_: &bhdl_netlist::Netlist| -> Result<HashMap<String, f64>, String> { Err("no convergence".into()) };
    let (ran2, _) = run_declared_faults(&netlist, &mut model2, &refuse);
    assert_eq!(ran2, 6);
    let unrun2: Vec<_> = model2.gaps.iter().filter(|g| g.class == GapClass::FaultUnrun).collect();
    assert_eq!(unrun2.len(), 6);
    assert!(unrun2.iter().all(|g| g.fix.contains("without verdict")));
}
