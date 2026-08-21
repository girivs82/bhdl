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
    let (ran, mismatched) = run_declared_faults(&netlist, &mut model, &solve, None);
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
    bhdl_synthesizer::fault_campaign::run_universe(&netlist, &mut model, &solve, &HashMap::new(), None);
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
    let (ran2, _) = run_declared_faults(&netlist, &mut model2, &refuse, None);
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
    bhdl_synthesizer::fault_campaign::run_universe(&netlist, &mut model, &solve, &HashMap::new(), None);
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
    bhdl_synthesizer::fault_campaign::run_universe(&netlist, &mut model2, &solve, &geo, None);
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
    run_declared_faults(&netlist, &mut model, &solve, None);
    bhdl_synthesizer::fault_campaign::run_universe(&netlist, &mut model, &solve, &HashMap::new(), None);
    bhdl_synthesizer::fault_campaign::compute_metrics(&mut model);
    let csvs = bhdl_synthesizer::fault_campaign::export_fmeda(&model);

    // worksheet: header + one row per universe fault (14 in the mock)
    // + the stated inter-part-bridge exclusion row
    let wlines: Vec<&str> = csvs.worksheet.trim_end().lines().collect();
    assert_eq!(wlines.len(), 1 + model.universe.len() + 1, "{}", csvs.worksheet);
    assert!(
        wlines.last().unwrap().contains("EXCLUDED") && wlines.last().unwrap().contains("short_inter_part"),
        "{}", csvs.worksheet
    );
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

/// Transient FTTI: with a time-domain engine, the timing verdict
/// COMPOSES the measured BOARD-path settle time of detected_when with
/// the mechanism's declared CHIP-INTERNAL latency + interval — the
/// solve sees the board up to the detector's pin; the chip inside is a
/// black box whose reaction time is exactly what the declared latency
/// models. The mock trace ramps every net 0→12V over the run:
/// `brd.sense.1 > 4V` settles at 0.4× the duration (= 0.8× `within`).
#[tokio::test]
async fn transient_ftti_measurement_supersedes_declared_latency() {
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
    // Mock transient: 11 samples, every non-GND net ramps 0→12V.
    let tran = |faulted: &bhdl_netlist::Netlist,
                duration: f64,
                _drives: &[bhdl_synthesizer::fault_campaign::PinDrive]|
     -> Result<(Vec<f64>, HashMap<String, Vec<f64>>), String> {
        let times: Vec<f64> = (0..=10).map(|k| duration * k as f64 / 10.0).collect();
        let traces: HashMap<String, Vec<f64>> = faulted
            .nets
            .iter()
            .filter_map(|(_, n)| n.name.clone())
            .map(|name| {
                let series: Vec<f64> = (0..=10)
                    .map(|k| if name == "GND" { 0.0 } else { 12.0 * k as f64 / 10.0 })
                    .collect();
                (name, series)
            })
            .collect();
        Ok((times, traces))
    };
    run_declared_faults(&netlist, &mut model, &solve, Some(&tran));
    let scope = &model.scopes[0];
    // 10ms budget: measured board settle 8ms (0.4 × 2×within) + 1ms
    // declared chip latency = 9ms ≤ 10ms → OK
    let t_ok = scope.faults.iter().find(|f| f.within.as_deref() == Some("10ms")).unwrap();
    assert_eq!(t_ok.timing_met, Some(true), "{t_ok:?}");
    assert!(t_ok.note.as_deref().unwrap_or("").contains("FTTI MEASURED"), "{t_ok:?}");
    assert!(t_ok.note.as_deref().unwrap_or("").contains("chip-internal"), "{t_ok:?}");
    // 500µs budget: measured board settle 0.4ms + 1ms chip latency =
    // 1.4ms > 0.5ms → FAILED — the chip-internal term the solve cannot
    // see is composed in, never dropped
    let t_500 = scope.faults.iter().find(|f| f.within.as_deref() == Some("500us")).unwrap();
    assert_eq!(t_500.timing_met, Some(false), "{t_500:?}");
    assert!(t_500.note.as_deref().unwrap_or("").contains("chip-internal"), "{t_500:?}");
    // A refusing engine falls back to the declared budget, stated.
    let mut model2 = build_safety_model(&netlist, &[&sf]);
    let no_tran = |_: &bhdl_netlist::Netlist, _: f64, _: &[bhdl_synthesizer::fault_campaign::PinDrive]| -> Result<(Vec<f64>, HashMap<String, Vec<f64>>), String> {
        Err("no transient engine".into())
    };
    run_declared_faults(&netlist, &mut model2, &solve, Some(&no_tran));
    let t2 = model2.scopes[0].faults.iter().find(|f| f.within.as_deref() == Some("500us")).unwrap();
    assert_eq!(t2.timing_met, Some(false), "declared 1ms budget > 500µs: {t2:?}");
    assert!(t2.note.as_deref().unwrap_or("").contains("declared budget used"), "{t2:?}");
}

/// Transient pin-disturbance states: a pulse-symptom behavior is
/// classified over the TRACE (the endpoint is healthy by construction),
/// a multi-pin vector is ONE fault with one λ, and detection may be the
/// measured external crossing or the vendor's declared internal path.
#[tokio::test]
async fn transient_pulse_states_classify_over_the_trace() {
    let src = r#"
entity Drv() {
    pin 1: signal inout;
    pin 2: signal inout;
    attribute component_class = "resistor";
    attribute resistance = 10MΩ;
    safety {
        failure_state glitch fit=2 of 10 source="FIXTURE" behavior="pulse(1, 0V, 20us); pulse(2, 0V, 20us)";
        failure_state quiet  fit=1 of 10 source="FIXTURE" behavior="pulse(1, 0V, 20us)" detected_internally="5us";
    }
}

board PulseDemo {
    power V12 = 12V @ 1A;
    ground GND;
    @V12 -> r1: Res(1kΩ).1; r1.2 -> d: Drv().1;
    d.2 -> r2: Res(10kΩ).1; r2.2 -> @GND;
}

safety PulseDemo as g {
    goal SG: ASIL_B "node floor" (id="SG-P-1") {
        effect uv = g.r1.2 < 8V severity S2;
    }
    mechanism g.r2: psm(SG, detects=[uv], detected_when = g.r2.1 < 6V, latency=10us, dc=0.9, source="FIXTURE");
    fault state(g.d, "glitch") expect SG.uv detected_by g.r2 within 100us;
    fault state(g.d, "quiet")  expect SG.uv within 100us;
}
"#;
    let src = format!("import {{ Res }} from \"bhdl-stdlib/passives/resistor.bhdl\";\n{src}");
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let pr = parse(&src);
    assert!(pr.errors().is_empty(), "parse: {:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let netlist = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let mut model = build_safety_model(&netlist, &[&sf]);
    assert!(model.errors.is_empty(), "{:#?}", model.errors);
    let solve = |faulted: &bhdl_netlist::Netlist| -> Result<HashMap<String, f64>, String> {
        Ok(faulted
            .nets
            .iter()
            .filter_map(|(_, n)| n.name.clone())
            .map(|name| (name.clone(), if name == "GND" { 0.0 } else { 12.0 }))
            .collect())
    };
    // Mock transient: nets DRIVEN by the fault dip to their drive level
    // for the first 3 of 11 samples; everything else holds 12V (GND 0).
    // The dip makes the uv effect and the detection predicate true
    // mid-trace and false at the endpoint — endpoint classification
    // would call this SAFE.
    let tran = |faulted: &bhdl_netlist::Netlist,
                duration: f64,
                drives: &[bhdl_synthesizer::fault_campaign::PinDrive]|
     -> Result<(Vec<f64>, HashMap<String, Vec<f64>>), String> {
        let times: Vec<f64> = (0..=10).map(|k| duration * k as f64 / 10.0).collect();
        let traces = faulted
            .nets
            .iter()
            .filter_map(|(_, n)| n.name.clone())
            .map(|name| {
                let driven = drives.iter().find(|d| d.net == name);
                let series: Vec<f64> = (0..=10)
                    .map(|k| {
                        if name == "GND" {
                            0.0
                        } else if driven.is_some() && k <= 3 {
                            driven.unwrap().level_v
                        } else {
                            12.0
                        }
                    })
                    .collect();
                (name, series)
            })
            .collect();
        Ok((times, traces))
    };
    run_declared_faults(&netlist, &mut model, &solve, Some(&tran));
    let faults = &model.scopes[0].faults;
    // glitch: pin2's drive collapses the monitored net → uv fires AND
    // the external predicate crosses (measured); expectation + timing OK
    let fg = faults.iter().find(|f| f.targets.iter().any(|t| t.contains("glitch"))).unwrap();
    assert_eq!(fg.expectation_met, Some(true), "{fg:?}");
    assert_eq!(fg.timing_met, Some(true), "{fg:?}");
    assert!(fg.note.as_deref().unwrap_or("").contains("crossed at"), "{fg:?}");
    assert!(fg.note.as_deref().unwrap_or("").contains("2 pin drive(s)"), "{fg:?}");
    // quiet: only pin1 driven — the monitored net never dips, so the
    // external monitor is BLIND; the uv effect still fires (r1.2 IS the
    // driven net) and the vendor's internal 5µs path carries the timing
    let fq = faults.iter().find(|f| f.targets.iter().any(|t| t.contains("quiet"))).unwrap();
    assert_eq!(fq.expectation_met, Some(true), "{fq:?}");
    assert_eq!(fq.timing_met, Some(true), "{fq:?}");
    assert!(fq.note.as_deref().unwrap_or("").contains("INTERNAL detection"), "{fq:?}");
    assert!(!fq.note.as_deref().unwrap_or("").contains("crossed at"), "external monitor must be blind: {fq:?}");
    // universe: state rows classified over the trace, one λ each
    bhdl_synthesizer::fault_campaign::run_universe(&netlist, &mut model, &solve, &HashMap::new(), Some(&tran));
    let states: Vec<_> = model.universe.iter().filter(|u| u.mode == "state").collect();
    assert_eq!(states.len(), 2, "{states:#?}");
    assert!(states.iter().all(|u| u.ran && !u.fired.is_empty()), "{states:#?}");
    let uq = states.iter().find(|u| u.targets.iter().any(|t| t.contains("quiet"))).unwrap();
    assert!(uq.detected.iter().any(|d| d.contains("internal")), "{uq:#?}");
    assert!(uq.weight_fit == Some(1.0), "one fault, one λ: {uq:#?}");
    // no engine ⇒ honest not-run, stated
    let mut model2 = build_safety_model(&netlist, &[&sf]);
    run_declared_faults(&netlist, &mut model2, &solve, None);
    let f2 = &model2.scopes[0].faults[0];
    assert!(!f2.run && f2.note.as_deref().unwrap_or("").contains("time-domain engine"), "{f2:?}");
}

/// Latent probe over TRANSIENT dangerous faults: a benign+silent DC
/// fault on the monitor part (dormant damage) blinds the glitch's
/// external detection — the classic unprotected-transient scenario.
/// The mock transient couples the monitor's flag net to the driven net
/// only while the monitor part is INTACT; opening it kills the
/// coupling. An internally-detected transient is NOT blindable by a
/// board fault and must not convict.
#[tokio::test]
async fn latent_probe_convicts_dormant_monitor_fault_blinding_a_transient() {
    let src = r#"
entity Drv() {
    pin 1: signal inout;
    pin 2: signal inout;
    attribute component_class = "resistor";
    attribute resistance = 10MΩ;
    safety {
        failure_state glitch fit=2 of 10 source="FIXTURE" behavior="pulse(1, 0V, 20us)";
        failure_state quiet  fit=1 of 10 source="FIXTURE" behavior="pulse(1, 0V, 20us)" detected_internally="5us";
    }
}

board LatentDemo {
    power V12 = 12V @ 1A;
    ground GND;
    @V12 -> r1: Res(1kΩ).1; r1.2 -> d: Drv().1;
    d.2 -> r2: Res(10kΩ).1; r2.2 -> @GND;
    r1.2 -> m: Res(10kΩ).1; m.2 -> rm2: Res(10kΩ).1; rm2.2 -> @GND;
}

safety LatentDemo as g {
    goal SG: ASIL_B "node floor" (id="SG-L-1") {
        effect uv = g.r1.2 < 8V severity S2;
    }
    mechanism g.m: psm(SG, detects=[uv], detected_when = g.m.2 < 6V, latency=10us, dc=0.9, source="FIXTURE");
}
"#;
    let src = format!("import {{ Res }} from \"bhdl-stdlib/passives/resistor.bhdl\";\n{src}");
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let pr = parse(&src);
    assert!(pr.errors().is_empty(), "parse: {:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let netlist = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let mut model = build_safety_model(&netlist, &[&sf]);
    assert!(model.errors.is_empty(), "{:#?}", model.errors);
    let solve = |faulted: &bhdl_netlist::Netlist| -> Result<HashMap<String, f64>, String> {
        Ok(faulted
            .nets
            .iter()
            .filter_map(|(_, n)| n.name.clone())
            .map(|name| (name.clone(), if name == "GND" { 0.0 } else { 12.0 }))
            .collect())
    };
    // Mock transient: driven nets dip 12→0 for the first 3 of 11
    // samples; the flag net (nets of instance `m`) mirrors the dip ONLY
    // while m is intact in the faulted netlist — a removed monitor
    // cannot couple.
    let tran = |faulted: &bhdl_netlist::Netlist,
                duration: f64,
                drives: &[bhdl_synthesizer::fault_campaign::PinDrive]|
     -> Result<(Vec<f64>, HashMap<String, Vec<f64>>), String> {
        let m_alive = faulted.instances.iter().any(|(_, i)| i.name == "m");
        let m_nets: Vec<String> = faulted
            .pin_instances
            .values()
            .filter(|pi| {
                faulted
                    .instances
                    .get(pi.instance)
                    .map(|i| i.name == "m")
                    .unwrap_or(false)
            })
            .filter_map(|pi| pi.net.and_then(|nid| faulted.nets.get(nid)).and_then(|n| n.name.clone()))
            .collect();
        let times: Vec<f64> = (0..=10).map(|k| duration * k as f64 / 10.0).collect();
        let traces = faulted
            .nets
            .iter()
            .filter_map(|(_, n)| n.name.clone())
            .map(|name| {
                let driven = drives.iter().any(|d| d.net == name);
                let coupled = m_alive && m_nets.contains(&name);
                let series: Vec<f64> = (0..=10)
                    .map(|k| {
                        if name == "GND" {
                            0.0
                        } else if (driven || coupled) && k <= 3 {
                            0.0
                        } else {
                            12.0
                        }
                    })
                    .collect();
                (name, series)
            })
            .collect();
        Ok((times, traces))
    };
    bhdl_synthesizer::fault_campaign::run_universe(&netlist, &mut model, &solve, &HashMap::new(), Some(&tran));
    // base classification: glitch is dangerous + EXTERNALLY detected
    // (flag couples while m intact); quiet detected internally too
    let ug = model.universe.iter().find(|u| u.targets.iter().any(|t| t.contains("glitch"))).unwrap();
    assert!(!ug.fired.is_empty() && ug.detected.iter().any(|d| d == "g.m"), "{ug:#?}");
    // m's open is benign+silent alone (flat 12V mock) → candidate; the
    // probe co-injects it with the TRANSIENT glitch: coupling gone,
    // effect still fires → LATENT, exposed by the glitch's λ share (2.0)
    let um = model.universe.iter().find(|u| u.part == "m" && u.mode == "open").unwrap();
    assert!(um.latent, "dormant monitor open must be convicted: {um:#?}");
    assert!(
        um.note.as_deref().unwrap_or("").contains("TRANSIENT"),
        "{um:#?}"
    );
    // exposure == glitch share ONLY (2.0): the internally-detected
    // 'quiet' state is NOT blindable by a board fault and must not
    // contribute
    assert!((um.latent_exposed_fit - 2.0).abs() < 1e-9, "exposed = glitch λ only: {um:#?}");
    // pulse states are never latent CANDIDATES (a transient cannot be
    // dormant)
    assert!(model.universe.iter().filter(|u| u.mode == "state").all(|u| !u.latent));
}

/// Elaboration emitter on a REAL synthesized fixture: ctor args
/// reconstruct from resolved attributes, connectivity emits as anchor
/// arrows. (Round-trip gating lands with the CLI command.)
#[tokio::test]
async fn elaborate_emits_reconstructed_ctors_for_fixture() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let src = std::fs::read_to_string(ws.join("tests/circuits/realistic/test_safety_fault_campaign.bhdl")).unwrap();
    let pr = parse(&src);
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let stdlib_src = std::fs::read_to_string(ws.join("bhdl-stdlib/passives/resistor.bhdl")).unwrap();
    let pr2 = parse(&stdlib_src);
    let sf2 = SourceFile::cast(pr2.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let netlist = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let ctors = bhdl_synthesizer::elaborate::extract_ctors(&[&sf, &sf2]);
    let preamble = bhdl_synthesizer::elaborate::extract_preamble(&src, &sf);
    let out = bhdl_synthesizer::elaborate::emit_elaborated_with_preamble(
        &netlist, "test_safety_fault_campaign.bhdl", &ctors, &preamble,
    );
    println!("{out}");
    // stdlib Res instances reconstruct value (+tolerance default fills)
    assert!(out.contains("r_top: Res(") && out.contains("r_bot: Res("), "{out}");
    // the fixture-local DemoSense entity reconstructs too
    assert!(out.contains("sense: DemoSense("), "{out}");
    // connectivity present as arrows
    assert!(out.contains(" -> "), "{out}");
    // board wrapper + power/ground declarations + rail anchors — the
    // single-pin V12 net must NOT drop
    assert!(out.contains("board FaultDemo {"), "{out}");
    assert!(out.contains("power V12 = 12V @ 1A;"), "{out}");
    assert!(out.contains("ground GND;"), "{out}");
    assert!(out.contains("@V12 -> r_top.1;"), "{out}");
    assert!(out.contains("r_bot.2 -> @GND;"), "{out}");
    // preamble carries the original imports and local entity defs
    // verbatim — the elaborated file is SELF-CONTAINED
    assert!(out.contains("import { Res } from \"bhdl-stdlib/passives/resistor.bhdl\";"), "{out}");
    assert!(out.contains("entity DemoSense"), "{out}");
    // ...but never a second copy of the board block
    assert_eq!(out.matches("board FaultDemo").count(), 1, "{out}");
    // pre-round-trip smoke: the emitted text PARSES as bhdl
    let reparse = parse(&out);
    assert!(reparse.errors().is_empty(), "elaborated output must parse: {:?}\n{out}", reparse.errors());
}

/// Decap synthesis (arc c): the board declares NO decoupling — the
/// synthesizer derives it from the entity's Z(f) mask, with N+1
/// margin and verified single-open robustness. Reproduces the full
/// in-process double-generate the elaborate pipeline runs.
#[tokio::test]
async fn decap_synthesis_double_generate_partitions() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let src = std::fs::read_to_string(ws.join("tests/circuits/realistic/test_decap_synthesis.bhdl")).unwrap();
    let pr = parse(&src);
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n1 = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synth 1");
    let decs1: Vec<_> = n1.instances.iter().filter(|(_, i)| i.name.contains("_dec")).map(|(_, i)| i.name.clone()).collect();
    // ≥2 chosen by the greedy loop + ≥1 N+1 margin (10µF is non-bulk
    // under the default bulk_over=10µF strict-greater rule)
    assert!(decs1.len() >= 3, "decaps minted incl. margin: {decs1:?}");
    // every minted decap carries its provenance + the library path
    for (_, i) in n1.instances.iter().filter(|(_, i)| i.name.contains("_dec")) {
        assert_eq!(i.attributes.get("decap_origin").map(String::as_str), Some("decouple soc.VDD"));
        assert!(i.attributes.get("decap_lib").is_some());
        // solver contract: numeric esr/esl (a pretty "5nH" would stamp IDEAL)
        assert!(i.attributes.get("esr").unwrap().parse::<f64>().is_ok());
        assert!(i.attributes.get("esl").unwrap().parse::<f64>().is_ok());
    }
    // second generate IN-PROCESS on the ELABORATED text (what the
    // round-trip gate actually re-synthesizes)
    let ctors = bhdl_synthesizer::elaborate::extract_ctors(&[&sf]);
    let src2 = bhdl_synthesizer::elaborate::emit_elaborated_with_preamble(
        &n1, "test_decap_synthesis.bhdl", &ctors,
        &bhdl_synthesizer::elaborate::extract_preamble(&src, &sf),
    );
    // the CLI injects the decap library import (decap_lib attribute) —
    // replicate it
    let src2 = format!(
        "import {{ DecapMid10u, DecapBulk100u, DecapHf100n }} from \"./tests/circuits/realistic/decap_lib_fixture.bhdl\";\n\n{src2}"
    );

    let pr2 = parse(&src2);
    assert!(pr2.errors().is_empty(), "elaborated parses: {:?}", pr2.errors());
    let sf2 = SourceFile::cast(pr2.syntax()).unwrap();
    let analysis2 = analyze(&sf2);
    let mut gen2 = NetlistGenerator::new();
    let n2 = gen2.generate_from_ast_and_analysis(&sf2, &analysis2).await.expect("synth 2");
    let nets = |n: &bhdl_netlist::Netlist| -> Vec<Vec<String>> {
        let mut v: Vec<Vec<String>> = n.nets.iter().map(|(id, _)| {
            let mut e: Vec<String> = n.pin_instances.values().filter(|pi| pi.net == Some(id))
                .filter_map(|pi| Some(format!("{}.{}", n.instances.get(pi.instance)?.name, n.pins.get(pi.pin_def)?.name)))
                .collect();
            e.sort(); e
        }).filter(|e| !e.is_empty()).collect();
        v.sort(); v
    };
    // the synthesis report is archived in the netlist's analysis data
    // (the CLI report section reads it from there)
    let reps = &n1.get_analysis_data().expect("analysis data").decap_reports;
    assert_eq!(reps.len(), 1, "{reps:#?}");
    let r = &reps[0];
    assert_eq!(r.target, "soc.VDD");
    assert_eq!(r.steps.len(), 2, "{r:#?}");
    assert_eq!(r.margin_added.len(), 1);
    assert_eq!(r.opens_verified, 3);
    assert!(r.final_ratio <= 1.0 && r.final_ratio > 0.0);
    assert!(r.candidates_skipped.iter().any(|c| c.contains("DecapNoEsl")), "{r:#?}");
    assert!((r.z_margin_pct - 20.0).abs() < 1e-9);

    // non-vacuous: the ground net must be a real merged net (load,
    // soc and all decaps), not per-pin fragments
    let p1 = nets(&n1);
    assert!(
        p1.iter().any(|e| e.len() >= 5 && e.iter().any(|x| x == "soc.2")),
        "healthy merged ground net expected: {p1:#?}"
    );
    assert_eq!(p1, nets(&n2), "elaborated re-synthesis, same process, SAME partition");
    // scoped-attribute consumption: the elaborated text carries
    // provenance as REAL `attribute inst.key = "v";` statements and
    // phase 4.45 applies them back — the re-synthesized decaps carry
    // their idempotency key as an attribute, not just a name shape
    for (_, i) in n2.instances.iter().filter(|(_, i)| i.name.contains("_dec")) {
        assert_eq!(
            i.attributes.get("decap_origin").map(String::as_str),
            Some("decouple soc.VDD"),
            "restored on {}: {:?}", i.name, i.attributes
        );
    }
}

/// Infeasibility is a HARD error naming the physics: a mask below the
/// declared PDN-budget floor cannot be met by ANY capacitor.
#[tokio::test]
async fn decap_synthesis_infeasible_budget_floor_is_hard_error() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    // budget L=10nH → ωL at 50MHz ≈ 3.1Ω, far above the 100mΩ mask.
    let src = r#"
import { Res } from "bhdl-stdlib/passives/resistor.bhdl";

entity Soc2() {
    pin 1: signal inout;
    pin 2: signal inout;
    attribute component_class = "resistor";
    attribute resistance = 10MΩ;
    attribute kicad_symbol = "Device:R";
    domain VDD pins="1" v=12V
        zmask="100kHz:100m 50MHz:100m"
        pdn_r=1m pdn_l=10n
        source="FIXTURE";
}

board BadPdn {
    power V12 = 12V @ 5A;
    ground GND;
    @V12 -> r_load: Res(12Ω).1; r_load.2 -> @GND;
    @V12 -> soc: Soc2().1;
    soc.2 -> @GND;
    decouple soc.VDD from "tests/circuits/realistic/decap_lib_fixture.bhdl";
}
"#;
    let pr = parse(src);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let err = gen
        .generate_from_ast_and_analysis(&sf, &analysis)
        .await
        .expect_err("must be a hard synthesis error");
    let msg = format!("{err:#}");
    assert!(msg.contains("INFEASIBLE"), "{msg}");
    assert!(msg.contains("budget alone"), "names the physics: {msg}");
}

/// Power-tree load harvesting on a FUNCTION-FIRST partial board:
/// rails declared but undriven, loads from entity domain contracts,
/// noise targets picked up, phantom stubs filtered.
#[tokio::test]
async fn powertree_harvests_loads_from_partial_board() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let src = std::fs::read_to_string(ws.join("tests/circuits/realistic/test_powertree_loads.bhdl")).unwrap();
    let pr = parse(&src);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let netlist = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let h = bhdl_synthesizer::powertree::harvest_loads(&netlist, &sf);

    // three real loads — the definition stubs must NOT harvest
    assert_eq!(h.loads.len(), 3, "{:#?}", h.loads);
    assert!(h.unwired.is_empty(), "{:#?}", h.unwired);

    let rail = |n: &str| h.rails.iter().find(|r| r.net == n).unwrap();
    assert_eq!(h.rails.len(), 4);
    let v33 = rail("V3V3");
    assert!((v33.i_nom_total_a.unwrap() - 0.4).abs() < 1e-9);
    let v1 = rail("V1V0");
    assert!((v1.i_nom_total_a.unwrap() - 2.0).abs() < 1e-9);
    assert!((v1.i_max_total_a.unwrap() - 4.0).abs() < 1e-9);
    assert!(v1.noise_uvrms.is_none()); // no target declared — absent stays absent
    assert!(!v1.driven);
    let v18 = rail("V1V8");
    assert!((v18.i_nom_total_a.unwrap() - 0.05).abs() < 1e-9);
    // noise=100uV parsed to µVrms
    assert!((v18.noise_uvrms.unwrap() - 100.0).abs() < 1e-9, "{v18:#?}");
    let vin = rail("VIN");
    assert_eq!(vin.declared_budget_a, Some(3.0));
    assert!(vin.loads.is_empty());
    // every rail undriven — the whole tree is the worklist
    assert!(h.rails.iter().all(|r| !r.driven));
}

/// Option calculator: donor-rail selection (feed the noise LDO from a
/// nearby existing rail — the decision variable is DISSIPATION, not
/// headroom volts) and intermediate minting only when no donor is
/// feasible.
#[tokio::test]
async fn powertree_options_donor_and_intermediate() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    use bhdl_synthesizer::powertree::{harvest_loads, propose_trees, Topology};

    // fixture: V3V3 exists → the V1V8 LDO must feed from it, resized
    let src = std::fs::read_to_string(ws.join("tests/circuits/realistic/test_powertree_loads.bhdl")).unwrap();
    let pr = parse(&src);
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let netlist = gen.generate_from_ast_and_analysis(&sf, &analysis).await.unwrap();
    let h = harvest_loads(&netlist, &sf);
    let opts = propose_trees(&h, "VIN").expect("options");
    assert!(!opts.is_empty());
    for o in &opts {
        let ldo = o.stages.iter().find(|s| s.topology == Topology::Ldo).expect("noise rail gets an LDO");
        assert_eq!(ldo.from, "V3V3", "donor rail, not a minted intermediate: {o:#?}");
        assert!(!o.stages.iter().any(|s| s.to.starts_with("V_INT")), "{o:#?}");
        // donor resized: V3V3 buck carries IO + LDO draw
        let donor = o.stages.iter().find(|s| s.to == "V3V3").unwrap();
        assert!((donor.i_nom_a - 0.45).abs() < 1e-9, "{donor:#?}");
        assert!(donor.serves.iter().any(|x| x == "V1V8"));
        // energy books balance: p_in = p_load + dissipation
        assert!((o.p_in_w - o.p_load_w - o.p_diss_w).abs() < 1e-6, "{o:#?}");
        // LDO efficiency is physics, stated as such
        assert!(ldo.eff_basis.contains("physics"));
        assert!((ldo.eff_pct - 1.8 / 3.3 * 100.0).abs() < 1e-6);
        // relative cost: V1V0 buck (rate 4A → 6) + V3V3 buck (0.68A →
        // 4) + LDO (0.08A → 1) = 11 units. The donor decision is what
        // the cost function prices: a minted intermediate would ADD a
        // buck's 4 units for zero requirement gain.
        assert!((o.cost_units - 11.0).abs() < 1e-9, "{o:#?}");
    }
    // options come sorted by relative cost, dissipation breaking ties
    assert!(opts.windows(2).all(|w| w[0].cost_units <= w[1].cost_units + 1e-9));

    // no donor in reach → a minimal-headroom intermediate is minted
    let src2 = r#"
entity Pll2() {
    pin 1: power in;
    pin 2: ground;
    attribute component_class = "ic";
    domain VDDA pins="1" v=1.8V i_nom=50mA i_max=80mA noise=100uV source="FIXTURE";
}
board OnlyNoise {
    power VIN = 12V @ 1A;
    power V1V8 = 1.8V;
    ground GND;
    @V1V8 -> pll: Pll2().1;
    pll.2 -> @GND;
}
"#;
    let pr2 = parse(src2);
    assert!(pr2.errors().is_empty(), "{:?}", pr2.errors());
    let sf2 = SourceFile::cast(pr2.syntax()).unwrap();
    let analysis2 = analyze(&sf2);
    let mut gen2 = NetlistGenerator::new();
    let n2 = gen2.generate_from_ast_and_analysis(&sf2, &analysis2).await.unwrap();
    let h2 = harvest_loads(&n2, &sf2);
    let opts2 = propose_trees(&h2, "VIN").expect("options");
    for o in &opts2 {
        // direct LDO from 12V would dissipate (12-1.8)*0.05 = 0.51W —
        // over the package bound — so the intermediate MUST appear at
        // minimal headroom 1.8+0.5 = 2.3V
        let int_buck = o.stages.iter().find(|s| s.to.starts_with("V_INT")).expect("intermediate minted: {o:#?}");
        assert!((int_buck.vout - 2.3).abs() < 1e-9, "{int_buck:#?}");
        let ldo = o.stages.iter().find(|s| s.topology == Topology::Ldo).unwrap();
        assert!((ldo.p_diss_w - 0.5 * 0.05).abs() < 1e-9, "minimal headroom heat: {ldo:#?}");
        // the forced intermediate is PRICED: buck 4 + LDO 1 = 5 units —
        // visibly more than the 1-unit donor-fed LDO it replaces
        assert!((o.cost_units - 5.0).abs() < 1e-9, "{o:#?}");
    }
}

/// Integrated-buck → controller+external-stages crossover is a
/// THERMAL estimate, not a current rule: same 6A/7A load goes
/// external at 3.3V (2.2W loss, 1.54W in-package > 1.5W bound) but
/// stays integrated at 1.0V (0.67W loss); 25A exceeds integrated FETs
/// outright. Each class carries its own cost band.
#[tokio::test]
async fn powertree_external_stage_crossover_is_thermal() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    use bhdl_synthesizer::powertree::{harvest_loads, propose_trees, Topology};
    let src = r#"
entity BigSoc() {
    pin 1: power in;
    pin 2: power in;
    pin 3: power in;
    pin 4: ground;
    attribute component_class = "ic";
    domain VCORE pins="1" v=0.9V i_nom=20A i_max=25A source="FIXTURE";
    domain VIO   pins="2" v=3.3V i_nom=6A  i_max=7A  source="FIXTURE";
    domain VMEM  pins="3" v=1.0V i_nom=5A  i_max=6A  source="FIXTURE";
}
board HighCurrent {
    power VIN = 12V @ 10A;
    power VCORE = 0.9V;
    power V3V3 = 3.3V;
    power V1V0 = 1.0V;
    ground GND;
    @VCORE -> soc: BigSoc().1;
    @V3V3 -> soc.2;
    @V1V0 -> soc.3;
    soc.4 -> @GND;
}
"#;
    let pr = parse(src);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.unwrap();
    let h = harvest_loads(&n, &sf);
    let opts = propose_trees(&h, "VIN").expect("options");
    for o in &opts {
        let by_rail = |net: &str| o.stages.iter().find(|s| s.to == net).unwrap();
        // 25A rating: integrated FETs out of reach
        let core = by_rail("VCORE");
        assert_eq!(core.topology, Topology::BuckExternal, "{core:#?}");
        assert!(core.eff_basis.contains("phase(s)") && core.eff_basis.contains("derated"), "{core:#?}");
        // 3.3V @ 6A: thermal crossover (in-package share over bound)
        let io = by_rail("V3V3");
        assert_eq!(io.topology, Topology::BuckExternal, "same current, higher Vout → more loss → external: {io:#?}");
        // 1.0V @ 5A/6A: low loss AND derated rating (6/0.8 = 7.5A)
        // still within the 8A integrated ceiling → integrated
        let mem = by_rail("V1V0");
        assert_eq!(mem.topology, Topology::Buck, "{mem:#?}");
        assert!((mem.required_rating_a - 7.5).abs() < 1e-9, "derating down the chain: {mem:#?}");
        // phase-based cost: 25A → 2 phases (20A/phase design point,
        // derated for FIT) → ctrl 4 + 2·1 + 2·5 = 16; 7A ext → 1
        // phase → 10 (a mild premium over the 9-unit integrated buck
        // the thermal crossover disqualified); integrated 7A = 9.
        assert_eq!(core.phases, 2, "{core:#?}");
        assert!((core.cost_units - 16.0).abs() < 1e-9);
        assert_eq!(io.phases, 1);
        assert!((io.cost_units - 10.0).abs() < 1e-9);
        // cost keys on the REQUIRED RATING (7.5A > 5A band → 9 units):
        // the part you must buy, not the current you draw
        assert!((mem.cost_units - 9.0).abs() < 1e-9);
        // every stage carries the derated acceptance rating
        for st in &o.stages {
            assert!((st.required_rating_a - st.i_max_a / 0.8).abs() < 1e-9, "{st:#?}");
        }
        assert_eq!(o.ext_buck_count, 2);
    }

    // modern-SoC scale: a 180A core rail is a 9-phase design and the
    // cost scales with the phases (and the board area they proxy)
    let src2 = r#"
entity MonsterSoc() {
    pin 1: power in;
    pin 2: ground;
    attribute component_class = "ic";
    domain VCORE pins="1" v=0.85V i_nom=150A i_max=180A source="FIXTURE";
}
board Monster {
    power VIN = 12V @ 20A;
    power VCORE = 0.85V;
    ground GND;
    @VCORE -> soc: MonsterSoc().1;
    soc.2 -> @GND;
}
"#;
    let pr2 = parse(src2);
    let sf2 = SourceFile::cast(pr2.syntax()).unwrap();
    let analysis2 = analyze(&sf2);
    let mut gen2 = NetlistGenerator::new();
    let n2 = gen2.generate_from_ast_and_analysis(&sf2, &analysis2).await.unwrap();
    let h2 = harvest_loads(&n2, &sf2);
    let opts2 = propose_trees(&h2, "VIN").expect("options");
    for o in &opts2 {
        let core = o.stages.iter().find(|s| s.to == "VCORE").unwrap();
        assert_eq!(core.topology, Topology::BuckExternal);
        assert_eq!(core.phases, 9, "{core:#?}");
        // ctrl 4 + 9·1 + 9·5 = 58
        assert!((core.cost_units - 58.0).abs() < 1e-9, "{core:#?}");
        // per-phase dissipation stays civil: 0.85V·150A, eff 86%
        // (90 band − 4pt ratio penalty at 14:1) → ~20.8W over 9
        // phases ≈ 2.3W/phase
        assert!((core.eff_pct - 86.0).abs() < 1e-9, "{core:#?}");
        assert!(core.p_diss_w / core.phases as f64 <= 2.5, "{core:#?}");
        // the intermediate-rail COMBINATION was evaluated, chosen or
        // not — the note proves the chain arithmetic ran
        if o.label == "max-efficiency" {
            assert!(
                o.notes.iter().any(|n| n.contains("bulk round") && n.contains("swept")),
                "{:#?}", o.notes
            );
        }
    }
}

/// The intermediate voltage is SWEPT, not a menu — and at 48V input
/// the arithmetic genuinely flips: direct 48→0.85V (56:1) pays the
/// deep-ratio penalty (75%), while bulk→VRM composes to ~77.4%, so
/// the two-stage chain WINS and the sweep finds the best bulk voltage
/// (ties prefer higher V: less bulk current). At 12V input the same
/// sweep says direct — both answers from the same model.
#[tokio::test]
async fn powertree_bulk_sweep_flips_at_48v() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    use bhdl_synthesizer::powertree::{harvest_loads, propose_trees, Topology};
    let src = r#"
entity Soc48() {
    pin 1: power in;
    pin 2: ground;
    pin 3: power in;
    attribute component_class = "ic";
    domain VCORE pins="1" v=0.85V i_nom=150A i_max=180A source="FIXTURE";
    domain VIO pins="3" v=3.3V i_nom=2A i_max=3A source="FIXTURE";
}
board FortyEight {
    power VIN = 48V @ 8A;
    power VCORE = 0.85V;
    power V3V3 = 3.3V;
    ground GND;
    @VCORE -> soc: Soc48().1;
    @V3V3 -> soc.3;
    soc.2 -> @GND;
}
"#;
    let pr = parse(src);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.unwrap();
    let h = harvest_loads(&n, &sf);
    let opts = propose_trees(&h, "VIN").expect("options");
    let eff_opt = opts.iter().find(|o| o.label == "max-efficiency").expect("efficiency option");
    // bulk chosen, with the sweep note carrying the arithmetic
    assert!(
        eff_opt.notes.iter().any(|nn| nn.contains("swept") && nn.contains("CHOSEN")),
        "{:#?}", eff_opt.notes
    );
    let bulk = eff_opt.stages.iter().find(|s| s.to.starts_with("V_BULK")).expect("bulk stage");
    // the sweep's optimum: both stages inside the ≤10:1 free-ish zone —
    // bulk ≥ 4.8V (48:10) and downstream ≤ 8.5V (0.85×10); ties prefer
    // the higher voltage → 8.5V
    assert!((bulk.vout - 8.5).abs() < 1e-9, "{bulk:#?}");
    let vrm = eff_opt.stages.iter().find(|s| s.to == "VCORE").unwrap();
    assert_eq!(vrm.from, bulk.to);
    assert_eq!(vrm.topology, Topology::BuckExternal);
    // chain beats direct: composed eff ≈ 88% × 88% = 77.4% > 75%
    assert!(eff_opt.eff_pct > 75.0, "{eff_opt:#?}");
    // MULTI-ROUND: after committing the 8.5V bulk, round 2 re-swept
    // the REMAINING rails (V3V3) for a second, different voltage and
    // reported its verdict — each distribution level earns its place
    // or the note says why not
    assert!(
        eff_opt.notes.iter().any(|nn| nn.contains("bulk round 2")),
        "{:#?}", eff_opt.notes
    );
    // V3V3 assignment is whatever the arithmetic said — but it must be
    // fed from ONE of: VIN direct or a bulk, never dangling
    let vio = eff_opt.stages.iter().find(|s| s.to == "V3V3").unwrap();
    assert!(vio.from == "VIN" || vio.from.starts_with("V_BULK"), "{vio:#?}");
}

/// Prereg policy: every rail sits behind the protected front end —
/// except always-on loads, which hang DIRECT off the input (stated):
/// they must live when the front end is off or faulted.
#[tokio::test]
async fn powertree_prereg_with_always_on_bypass() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    use bhdl_synthesizer::powertree::{harvest_loads, propose_trees_with_policy, Topology};
    let src = r#"
entity Mcu() {
    pin 1: power in;
    pin 2: power in;
    pin 3: ground;
    attribute component_class = "ic";
    domain VDD pins="1" v=3.3V i_nom=0.5A i_max=0.8A source="FIXTURE";
    domain VSTBY pins="2" v=3.3V i_nom=10mA i_max=20mA always_on=true source="FIXTURE";
}
board Protected {
    power VIN = 12V @ 2A;
    power V3V3 = 3.3V;
    power VSTBY = 3.3V;
    ground GND;
    @V3V3 -> mcu: Mcu().1;
    @VSTBY -> mcu.2;
    mcu.3 -> @GND;
}
"#;
    let pr = parse(src);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.unwrap();
    let h = harvest_loads(&n, &sf);
    // the always_on flag survives harvest
    assert!(h.rails.iter().find(|r| r.net == "VSTBY").unwrap().always_on);
    assert!(!h.rails.iter().find(|r| r.net == "V3V3").unwrap().always_on);

    let opts = propose_trees_with_policy(&h, "VIN", Some("OV/UV + load dump")).expect("options");
    for o in &opts {
        // the front end exists, carries the reason, and is rated+derated
        let prot = o.stages.iter().find(|s| s.topology == Topology::Prereg).expect("prereg stage");
        assert_eq!(prot.from, "VIN");
        assert!(prot.eff_basis.contains("OV/UV + load dump"), "{prot:#?}");
        assert!((prot.required_rating_a - prot.i_max_a / 0.8).abs() < 1e-9);
        // the ordinary rail sits BEHIND the protection
        let v33 = o.stages.iter().find(|s| s.to == "V3V3").unwrap();
        assert_eq!(v33.from, "V_PROT", "{v33:#?}");
        assert!((v33.vin - 11.9).abs() < 1e-9);
        // the always-on rail hangs DIRECT off the input, stated
        let stby = o.stages.iter().find(|s| s.to == "VSTBY").unwrap();
        assert_eq!(stby.from, "VIN", "AO bypass: {stby:#?}");
        assert!(
            o.notes.iter().any(|nn| nn.contains("always-on") && nn.contains("VSTBY")),
            "{:#?}", o.notes
        );
        // energy books still balance through the protection chain
        assert!((o.p_in_w - o.p_load_w - o.p_diss_w).abs() < 1e-6, "{o:#?}");
    }
}
