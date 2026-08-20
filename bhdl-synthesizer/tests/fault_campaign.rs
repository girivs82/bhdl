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
    bhdl_synthesizer::fault_campaign::run_universe(&netlist, &mut model, &solve, &HashMap::new());
    // 3 generic 2-pin parts × 4 modes (short/open + drift_high/drift_low
    // — value-carrying parts get parametric drift probed at the declared
    // tolerance edge and the labelled 0.5×/2× convention point) +
    // DemoSense's 2 VENDOR states (behavioral parts run the vendor's
    // states, never generic drift)
    assert_eq!(model.universe.len(), 14, "{:#?}", model.universe);
    let drifts: Vec<_> = model.universe.iter().filter(|u| u.mode.starts_with("drift_")).collect();
    assert_eq!(drifts.len(), 6);
    // every drift row's note labels the convention probe as convention
    assert!(
        drifts.iter().all(|u| u.note.as_deref().map(|n| n.contains("convention")).unwrap_or(false)),
        "{drifts:#?}"
    );
    // stdlib Res declares tolerance=5% — the real-data probe is named
    assert!(drifts.iter().all(|u| u.note.as_deref().unwrap_or("").contains("declared tolerance")), "{drifts:#?}");
    // under the flat 12V mock every drift is dangerous (ov fires) and
    // detected (sense reads 12V) — the sweep reports full coverage
    assert!(
        drifts.iter().all(|u| u.ran && !u.fired.is_empty() && !u.detected.is_empty()),
        "{drifts:#?}"
    );
    assert!(
        drifts.iter().all(|u| u.note.as_deref().unwrap_or("").contains("detected throughout")),
        "{drifts:#?}"
    );
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
    assert_eq!(m.unmeasured_faults, 12, "generic modes (incl. drift) lack λ shares in the mock run; the 2 vendor states carry theirs");
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

/// Multi-pin universe: adjacent-pin bridge faults. With no geometry the
/// bridges are the ORDERING approximation (consecutive definition-order
/// pins, labelled); with a caller-supplied geometric adjacency map the
/// bridges are exactly the geometric pairs, labelled as such. Weight is
/// None (no safety data on the part) — the bridge rows still run.
#[tokio::test]
async fn universe_adjacent_pin_bridges_geometry_vs_ordering() {
    let src = r#"
entity Quad() {
    pin 1: signal inout;
    pin 2: signal inout;
    pin 3: signal inout;
    pin 4: signal inout;
    attribute component_class = "ic";
}

board GeoDemo {
    power V5 = 5V @ 1A;
    ground GND;
    @V5 -> u1: Quad().1;
    u1.2 -> @GND;
    u1.3 -> @GND;
    u1.4 -> @GND;
}

safety GeoDemo as g {
    goal SG: ASIL_B "rail" (id="SG-GEO-1") {
        effect ov = g.u1.1 > 8V severity S2;
    }
}
"#;
    let pr = parse(src);
    assert!(pr.errors().is_empty(), "parse: {:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let netlist = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let solve = |faulted: &bhdl_netlist::Netlist| -> Result<HashMap<String, f64>, String> {
        Ok(faulted
            .nets
            .iter()
            .filter_map(|(_, n)| n.name.clone())
            .map(|name| (name.clone(), if name == "GND" { 0.0 } else { 5.0 }))
            .collect())
    };

    // No geometry → ordering fallback: 4 opens + 3 consecutive bridges.
    let mut model = build_safety_model(&netlist, &[&sf]);
    bhdl_synthesizer::fault_campaign::run_universe(&netlist, &mut model, &solve, &HashMap::new());
    let bridges: Vec<_> = model.universe.iter().filter(|u| u.mode == "short_adjacent").collect();
    assert_eq!(bridges.len(), 3, "{:#?}", model.universe);
    assert!(
        bridges.iter().all(|u| u.note.as_deref().map(|n| n.contains("ordering-adjacency")).unwrap_or(false)),
        "fallback must be labelled: {bridges:#?}"
    );
    assert_eq!(model.universe.iter().filter(|u| u.mode == "open_pin").count(), 4);
    assert!(bridges.iter().all(|u| u.ran), "{bridges:#?}");

    // Geometric map → exactly those pairs, labelled geometric. The map
    // deliberately DROPS the 2-3 consecutive pair (opposite sides on
    // the package) — the ordering bug this feature kills.
    let mut geo: HashMap<String, Vec<(String, String)>> = HashMap::new();
    geo.insert("u1".into(), vec![("1".into(), "2".into()), ("3".into(), "4".into())]);
    let mut model2 = build_safety_model(&netlist, &[&sf]);
    bhdl_synthesizer::fault_campaign::run_universe(&netlist, &mut model2, &solve, &geo);
    let bridges2: Vec<_> = model2.universe.iter().filter(|u| u.mode == "short_adjacent").collect();
    assert_eq!(bridges2.len(), 2, "{:#?}", model2.universe);
    assert!(
        bridges2.iter().all(|u| u.note.as_deref().map(|n| n.contains("geometric adjacency")).unwrap_or(false)),
        "{bridges2:#?}"
    );
    let has_pair = |a: &str, b: &str| {
        bridges2.iter().any(|u| u.targets == vec![format!("u1.{a}"), format!("u1.{b}")])
    };
    assert!(has_pair("1", "2") && has_pair("3", "4"), "{bridges2:#?}");
    assert!(!has_pair("2", "3"), "2-3 must be dropped by geometry: {bridges2:#?}");
}

/// FMEDA export: the worksheet serializes what the campaign MEASURED —
/// no field computed at export time, empty cells where data does not
/// exist. Runs on the FaultDemo fixture after a full mock campaign.
#[tokio::test]
async fn fmeda_export_serializes_the_measured_model() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let src = std::fs::read_to_string(ws.join("tests/circuits/realistic/test_safety_fault_campaign.bhdl")).unwrap();
    let pr = parse(&src);
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let netlist = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let mut model = build_safety_model(&netlist, &[&sf]);
    let solve = |faulted: &bhdl_netlist::Netlist| -> Result<HashMap<String, f64>, String> {
        Ok(faulted
            .nets
            .iter()
            .filter_map(|(_, n)| n.name.clone())
            .map(|name| (name.clone(), if name == "GND" { 0.0 } else { 12.0 }))
            .collect())
    };
    run_declared_faults(&netlist, &mut model, &solve);
    bhdl_synthesizer::fault_campaign::run_universe(&netlist, &mut model, &solve, &HashMap::new());
    bhdl_synthesizer::fault_campaign::compute_metrics(&mut model);
    let csvs = bhdl_synthesizer::fault_campaign::export_fmeda(&model);

    // worksheet: header + one row per universe fault (14 in the mock)
    let wlines: Vec<&str> = csvs.worksheet.trim_end().lines().collect();
    assert_eq!(wlines.len(), 1 + model.universe.len(), "{}", csvs.worksheet);
    assert!(wlines[0].starts_with("scope,part,entity,part_lambda_fit"));
    // vendor states carry their real λ shares; classification column present
    assert!(csvs.worksheet.contains("state") && csvs.worksheet.contains("6.0000"), "{}", csvs.worksheet);
    // under the flat mock sense_stuck is RESIDUAL (ov fires everywhere,
    // its own short kills the detection read) — LATENT rows appear only
    // under a real solve; both classifications serialize
    assert!(csvs.worksheet.contains("RESIDUAL"), "{}", csvs.worksheet);
    assert!(csvs.worksheet.contains("DETECTED_DANGEROUS"), "{}", csvs.worksheet);
    // CSV escaping: notes contain commas → quoted cells, and every data
    // row parses back to the header's column count
    let ncols = wlines[0].split(',').count();
    for line in &wlines[1..] {
        let mut cols = 0usize;
        let mut in_q = false;
        for c in line.chars() {
            match c {
                '"' => in_q = !in_q,
                ',' if !in_q => cols += 1,
                _ => {}
            }
        }
        assert_eq!(cols + 1, ncols, "row column count: {line}");
    }
    // mechanisms: claimed vs measured side by side
    assert!(csvs.mechanisms.lines().next().unwrap().contains("claimed_dc,dc_source,measured_dc"));
    assert!(csvs.mechanisms.contains("brd.sense"), "{}", csvs.mechanisms);
    // metrics: one row for the board scope, incomplete ⇒ pass=false
    let mlines: Vec<&str> = csvs.metrics.trim_end().lines().collect();
    assert_eq!(mlines.len(), 2, "{}", csvs.metrics);
    assert!(mlines[1].ends_with("false"), "{}", csvs.metrics);
}
