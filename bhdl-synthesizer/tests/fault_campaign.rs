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
    // ctor signatures from the board AND its stdlib imports (as the
    // CLI's transitive-import extraction does) — else Res emits bare
    let res_src = std::fs::read_to_string(ws.join("bhdl-stdlib/passives/resistor.bhdl")).unwrap();
    let res_pr = parse(&res_src);
    let res_sf = SourceFile::cast(res_pr.syntax()).unwrap();
    let ctors = bhdl_synthesizer::elaborate::extract_ctors(&[&sf, &res_sf]);
    let src2 = bhdl_synthesizer::elaborate::emit_elaborated_with_preamble(
        &n1, "test_decap_synthesis.bhdl", &ctors,
        &bhdl_synthesizer::elaborate::extract_preamble(&src, &sf),
    );
    assert!(src2.contains("r_load: Res(12Ω"), "emission carries positional args: {src2}");
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
    // ATTRIBUTE FIDELITY on re-synthesis: the elaborated text restates
    // `r_load: Res(12Ω, 5%, 0.25W);` as a STANDALONE positional
    // instantiation — the value must bind to the resistance attribute
    // exactly as the inline form did (this was a silent gap: the
    // positional args bound only in connection context)
    let (_, r2) = n2.instances.iter().find(|(_, i)| i.name == "r_load").expect("r_load re-synthesized");
    assert_eq!(
        r2.attributes.get("resistance").map(String::as_str),
        Some("12Ω"),
        "positional ctor args must bind on standalone instantiation: {:#?}",
        r2.attributes
    );
    assert_eq!(r2.attributes.get("tolerance").map(String::as_str), Some("5%"), "{:#?}", r2.attributes);
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

    // ── the controller+external stage EMITS a BuckExtStage requirement
    //    with its phase count; the resolver lists the BuckController
    //    TEMPLATE (never auto-bound) and leaves the stage a placeholder ──
    let region = bhdl_synthesizer::powertree::emit_power_region(eff_opt, "GND");
    let line = region.lines().find(|l| l.contains("u_vcore:")).expect("vcore stage emitted").to_string();
    assert!(line.contains("BuckExtStage(") && line.contains(&format!("phases={}", vrm.phases)), "{line}");
    let emitted = bhdl_synthesizer::powertree::splice_power_region(&src, &region).unwrap();
    let r = bhdl_synthesizer::stage_resolution::resolve_stages(&emitted, ws.join("bhdl-stdlib").as_path(), &[]).unwrap().unwrap();
    let vcore = r.resolutions.iter().find(|x| x.instance == "u_vcore").unwrap();
    assert!(vcore.bound.is_none(), "{}", bhdl_synthesizer::stage_resolution::render_report(vcore));
    let tmpl = vcore.candidates.iter().find(|c| c.block == "BuckController").expect("template listed");
    assert!(tmpl.template && !tmpl.passes());
    assert!(tmpl.gates.iter().any(|g| g.0 == "phases" && !g.2), "{tmpl:#?}");
    assert!(r.source.contains("u_vcore: GenericBuckExt("), "{}", r.source);
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
        // the front end EMITS as a PreregStage requirement (resolvable)
        let region = bhdl_synthesizer::powertree::emit_power_region(o, "GND");
        let line = region.lines().find(|l| l.contains("u_v_prot:")).expect("prereg stage emitted").to_string();
        assert!(line.contains("PreregStage(") && !line.contains("GenericPrereg"), "{line}");
        assert!(
            o.notes.iter().any(|nn| nn.contains("always-on") && nn.contains("VSTBY")),
            "{:#?}", o.notes
        );
        // energy books still balance through the protection chain
        assert!((o.p_in_w - o.p_load_w - o.p_diss_w).abs() < 1e-6, "{o:#?}");
    }
}

/// --emit closes the loop: the emitted region (generic placeholders +
/// wiring + assumption attributes) makes every load rail DRIVEN — the
/// worklist empties, and the scoped assumption attributes land on the
/// instances as the acceptance contract for the real parts.
#[tokio::test]
async fn powertree_emit_closes_the_loop() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    use bhdl_synthesizer::powertree::{
        emit_power_region, harvest_loads, propose_trees, splice_power_region, strip_power_region,
    };
    let src = std::fs::read_to_string(ws.join("tests/circuits/realistic/test_powertree_loads.bhdl")).unwrap();
    let pr = parse(&src);
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let netlist = gen.generate_from_ast_and_analysis(&sf, &analysis).await.unwrap();
    let h = harvest_loads(&netlist, &sf);
    let opts = propose_trees(&h, "VIN").unwrap();

    let region = emit_power_region(&opts[0], "GND");
    let emitted = resolve_emitted(ws, &splice_power_region(&src, &region).unwrap());

    // the emitted board parses and synthesizes
    let pr2 = parse(&emitted);
    assert!(pr2.errors().is_empty(), "{:?}", pr2.errors());
    let sf2 = SourceFile::cast(pr2.syntax()).unwrap();
    let analysis2 = analyze(&sf2);
    let mut gen2 = NetlistGenerator::new();
    let n2 = gen2.generate_from_ast_and_analysis(&sf2, &analysis2).await.expect("emitted board synthesizes");

    // every load rail is now DRIVEN — the worklist is empty
    let h2 = harvest_loads(&n2, &sf2);
    for r in h2.rails.iter().filter(|r| !r.loads.is_empty()) {
        assert!(r.driven, "emitted tree must drive {}: {r:#?}", r.net);
    }
    // the assumption attributes landed on the placeholder instances
    // (scoped-attribute consumption) — the acceptance contract is ON
    // the netlist, not just in text
    let (_, u) = n2.instances.iter().find(|(_, i)| i.name == "u_v1v8").expect("LDO stage");
    assert_eq!(u.attributes.get("powertree_eff_assumed_pct").map(String::as_str), Some("54.5"), "{:#?}", u.attributes);
    assert!(u.attributes.contains_key("powertree_noise_assumed_uvrms"));
    // the LDO requirement RESOLVED (Ldo_LP2985 covers 1.8V / 80mA /
    // 3.3V in / 30µV): the block is bound and its silicon materialised;
    // the requirement stays live on the instance
    assert_eq!(u.attributes.get("stage_bound").map(String::as_str), Some("Ldo_LP2985"), "{:#?}", u.attributes);
    assert!(n2.instances.iter().any(|(_, i)| i.name == "u_v1v8_u"), "LP2985 silicon inside the block");
    // the 4A buck has no covering block yet → placeholder, contract attrs present
    let (_, b) = n2.instances.iter().find(|(_, i)| i.name == "u_v1v0").expect("buck placeholder");
    assert_eq!(b.attributes.get("stage_binding").map(String::as_str), Some("unresolved"), "{:#?}", b.attributes);
    assert!(b.attributes.contains_key("i_rating"), "{:#?}", b.attributes);

    // strip → byte-identical replanning source
    let stripped = strip_power_region(&emitted).expect("region present");
    // (import line remains — harmless; the board body is restored)
    assert!(!stripped.contains("BEGIN GENERATED"));
    assert!(stripped.contains("@V1V0 -> soc: SocCore().1;"));
}

/// THE REGULATOR PIN CONTRACT, machine-enforced: every entity in
/// bhdl-stdlib/power that regulates (has a VOUT) must expose
/// VIN / VOUT / GND — the same 3-pin logical surface as the Generic*
/// placeholders — so a power-tree swap is a rename, wiring untouched.
/// Physical pins beyond these are the entity's internal business,
/// served by its virtual-pin expansion (the datasheet application
/// circuit), never by board wiring the generic couldn't have known.
#[test]
fn stdlib_regulators_honor_the_pin_contract() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut checked = 0;
    for entry in std::fs::read_dir(ws.join("bhdl-stdlib/power")).unwrap().flatten() {
        let path = entry.path();
        if path.extension().and_then(|x| x.to_str()) != Some("bhdl") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let pr = parse(&text);
        assert!(pr.errors().is_empty(), "{}: {:?}", path.display(), pr.errors());
        let sf = SourceFile::cast(pr.syntax()).unwrap();
        for ent in sf.entities() {
            use bhdl_ast::HasName;
            let Some(name) = ent.name().map(|t| t.text().to_string()) else { continue };
            // The contract is the BOARD-FACING surface: `as part` silicon
            // (LM317 has no GND pin at all) is wrapped by its block.
            if ent.declared_kind().as_deref() == Some("part") {
                continue;
            }
            let pins: Vec<String> = ent
                .syntax()
                .descendants()
                .filter(|n| n.kind() == bhdl_parser::SyntaxKind::PIN_DECL)
                .filter_map(|n| {
                    n.children_with_tokens()
                        .filter_map(|e| e.into_token())
                        .find(|t| t.kind() == bhdl_parser::SyntaxKind::IDENT)
                        .map(|t| t.text().to_string())
                })
                .collect();
            if !pins.iter().any(|p| p == "VOUT") {
                continue; // not a regulator surface (Ground etc.)
            }
            checked += 1;
            assert!(
                pins.iter().any(|p| p == "VIN"),
                "{name} ({}): regulator contract requires pin VIN (has {pins:?})",
                path.display()
            );
            assert!(
                pins.iter().any(|p| p == "GND"),
                "{name} ({}): regulator contract requires pin GND (has {pins:?})",
                path.display()
            );
        }
    }
    assert!(checked >= 10, "expected the stdlib regulator population, checked {checked}");
}

/// Resolve the emitted requirement instantiations exactly as the CLI
/// does before synthesis (no lock, no override).
fn resolve_emitted(ws: &std::path::Path, text: &str) -> String {
    bhdl_synthesizer::stage_resolution::resolve_stages(text, ws.join("bhdl-stdlib").as_path(), &[])
        .expect("resolver")
        .expect("requirements present")
        .source
}

/// Swap one emitted stage line (`    <inst>: …;`) for a hand-written
/// instantiation and add its import after the emitter's import block —
/// the "commit a real part" edit, independent of how the emitter
/// spells the requirement.
fn swap_stage(text: &str, inst: &str, new_inst_stmt: &str, import_line: &str) -> String {
    let prefix = format!("    {inst}: ");
    let swapped: String = text
        .lines()
        .map(|l| if l.starts_with(&prefix) { format!("    {inst}: {new_inst_stmt}") } else { l.to_string() })
        .collect::<Vec<_>>()
        .join("\n");
    let imp = bhdl_synthesizer::powertree::EMIT_IMPORT;
    assert!(swapped.contains(imp), "emitter import block present");
    swapped.replacen(imp, &format!("{imp}\n{import_line}"), 1)
}

/// Swap-by-rename, END TO END: emit the power tree with generics,
/// then rename the LDO placeholder to a REAL part (XC6206P182 —
/// contract pins, virtual VOUT, datasheet expansion) and
/// re-synthesize. The wiring survives untouched, the real part's
/// expansion mints its application circuit, and the rails stay
/// driven.
#[tokio::test]
async fn powertree_swap_by_rename_to_real_part() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    use bhdl_synthesizer::powertree::{emit_power_region, harvest_loads, propose_trees, splice_power_region};
    let src = std::fs::read_to_string(ws.join("tests/circuits/realistic/test_powertree_loads.bhdl")).unwrap();
    let pr = parse(&src);
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let netlist = gen.generate_from_ast_and_analysis(&sf, &analysis).await.unwrap();
    let h = harvest_loads(&netlist, &sf);
    let opts = propose_trees(&h, "VIN").unwrap();
    let emitted = resolve_emitted(ws, &splice_power_region(&src, &emit_power_region(&opts[0], "GND")).unwrap());

    // THE SWAP: one line — rename the generic to the real part (the
    // 1.8V fixed XC6206 SKU). Ctor args change with it (the real
    // part's voltage is its SKU); the WIRING lines are untouched.
    let swapped = swap_stage(&emitted, "u_v1v8", "XC6206P182();", "import { XC6206P182 } from \"bhdl-stdlib/power/xc6206.bhdl\";");
    assert_ne!(swapped, emitted, "the rename must have applied");

    let pr2 = parse(&swapped);
    assert!(pr2.errors().is_empty(), "{:?}", pr2.errors());
    let sf2 = SourceFile::cast(pr2.syntax()).unwrap();
    let analysis2 = analyze(&sf2);
    let mut gen2 = NetlistGenerator::new();
    let n2 = gen2.generate_from_ast_and_analysis(&sf2, &analysis2).await.expect("swapped board synthesizes");

    // the real part's datasheet expansion minted its application
    // circuit onto the SAME wiring the generic used
    assert!(
        n2.instances.iter().any(|(_, i)| i.name == "u_v1v8_C_out"),
        "XC6206 expansion mints C_out: {:?}",
        n2.instances.iter().map(|(_, i)| i.name.clone()).collect::<Vec<_>>()
    );
    // rails stay driven — the tree is intact after the swap
    let h2 = harvest_loads(&n2, &sf2);
    for r in h2.rails.iter().filter(|r| !r.loads.is_empty()) {
        assert!(r.driven, "swap must not undrive {}: {r:#?}", r.net);
    }

    // ── ERC032: the acceptance CONTRACT is enforced ──
    let v = bhdl_synthesizer::erc::check_powertree_acceptance(&n2, &analysis2);
    // the committed XC6206 (0.2A rated) meets the 0.125A derated
    // requirement — no Error against u_v1v8
    assert!(
        !v.iter().any(|x| x.severity == bhdl_synthesizer::design_rule_checker::ViolationSeverity::Error
            && x.description.contains("u_v1v8")),
        "{v:#?}"
    );
    // the still-generic bucks report as placeholders (Info) — a
    // planned tree is visible, never silent
    assert!(
        v.iter().any(|x| x.severity == bhdl_synthesizer::design_rule_checker::ViolationSeverity::Info
            && x.description.contains("placeholder")),
        "{v:#?}"
    );

    // under-rated rename = Error: fake the same swap onto a 0.2A LDO
    // with a 5A requirement
    let shrunk = swap_stage(&emitted, "u_v1v0", "XC6206P332();", "import { XC6206P332 } from \"bhdl-stdlib/power/xc6206.bhdl\";");
    assert_ne!(shrunk, emitted);
    let pr3 = parse(&shrunk);
    assert!(pr3.errors().is_empty(), "{:?}", pr3.errors());
    let sf3 = SourceFile::cast(pr3.syntax()).unwrap();
    let analysis3 = analyze(&sf3);
    let mut gen3 = NetlistGenerator::new();
    let n3 = gen3.generate_from_ast_and_analysis(&sf3, &analysis3).await.expect("shrunk board synthesizes");
    let v3 = bhdl_synthesizer::erc::check_powertree_acceptance(&n3, &analysis3);
    assert!(
        v3.iter().any(|x| x.severity == bhdl_synthesizer::design_rule_checker::ViolationSeverity::Error
            && x.description.contains("u_v1v0")
            && x.description.contains("silently shrank")),
        "under-rated rename must be an Error: {v3:#?}"
    );

    // ── noise convention: the 1.8V stage assumed the low-noise LDO
    // class (30µVrms). Committing LP2985 (datasheet 30µVrms) passes;
    // committing a 78xx-class part (40µVrms) is the noise Error. ──
    let quiet = swap_stage(&emitted, "u_v1v8", "Ldo_LP2985(v_out=1.8V, i_out_max=80mA, v_in=3.3V);", "import { Ldo_LP2985 } from \"bhdl-stdlib/power/lp2985.bhdl\";");
    let sfq = SourceFile::cast(parse(&quiet).syntax()).unwrap();
    let aq = analyze(&sfq);
    let mut gq = NetlistGenerator::new();
    let nq = gq.generate_from_ast_and_analysis(&sfq, &aq).await.expect("LP2985 board synthesizes");
    let vq = bhdl_synthesizer::erc::check_powertree_acceptance(&nq, &aq);
    assert!(
        !vq.iter().any(|x| x.description.contains("u_v1v8") && x.description.contains("output noise")),
        "30µVrms part meets the 30µVrms assumption: {vq:#?}"
    );
    let loud = swap_stage(&emitted, "u_v1v8", "Ldo_LM7805(v_out=5V, i_out_max=80mA, v_in=12V);", "import { Ldo_LM7805 } from \"bhdl-stdlib/power/lm7805.bhdl\";");
    let sfl = SourceFile::cast(parse(&loud).syntax()).unwrap();
    let al = analyze(&sfl);
    let mut gl = NetlistGenerator::new();
    let nl = gl.generate_from_ast_and_analysis(&sfl, &al).await.expect("LM7805 board synthesizes");
    let vl = bhdl_synthesizer::erc::check_powertree_acceptance(&nl, &al);
    assert!(
        vl.iter().any(|x| x.severity == bhdl_synthesizer::design_rule_checker::ViolationSeverity::Error
            && x.description.contains("u_v1v8")
            && x.description.contains("output noise")),
        "40µVrms part against a 30µVrms assumption must be the noise Error: {vl:#?}"
    );
}

/// Partness (`entity X as part|design`): the declared bit — a design
/// block's instantiation mints no physical self-part (module kind
/// DesignBlock; BOM and the safety parts table exclude it via their
/// existing kind whitelists), only its children are physical. A
/// design block declaring package identity is a contradiction and a
/// hard error.
#[tokio::test]
async fn entity_partness_as_design_and_as_part() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    use bhdl_netlist::types::ModuleKind;

    let src = r#"
import { Res } from "bhdl-stdlib/passives/resistor.bhdl";

// designer composition: no package identity, children are the copper
entity DividerBlock(r_top: resistance = 10kΩ, r_bot: resistance = 10kΩ) as design {
    pin VIN: power in;
    pin VOUT: signal out virtual;
    pin GND: ground;
    expansion {
        VIN -> R_top: Res(r_top).1; R_top.2 -> VOUT;
        VOUT -> R_bot: Res(r_bot).1; R_bot.2 -> GND;
    }
}

entity LoadPart() as part {
    pin 1: signal in;
    pin 2: ground;
    attribute component_class = "resistor";
    attribute resistance = 100kΩ;
    attribute kicad_symbol = "Device:R";
}

board PartnessDemo {
    power V5 = 5V @ 1A;
    ground GND;
    @V5 -> div: DividerBlock().VIN;
    div.GND -> @GND;
    div.VOUT -> load: LoadPart().1;
    load.2 -> @GND;
}
"#;
    let pr = parse(src);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    // AST surface
    use bhdl_ast::HasName;
    let kinds: std::collections::HashMap<String, Option<String>> = sf
        .entities()
        .filter_map(|e| e.name().map(|n| (n.text().to_string(), e.declared_kind())))
        .collect();
    assert_eq!(kinds["DividerBlock"], Some("design".to_string()));
    assert_eq!(kinds["LoadPart"], Some("part".to_string()));

    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    // module kinds carry the declaration
    let kind_of = |name: &str| n.modules.iter().find(|(_, m)| m.name == name).map(|(_, m)| m.kind);
    assert_eq!(kind_of("DividerBlock"), Some(ModuleKind::DesignBlock));
    assert_eq!(kind_of("LoadPart"), Some(ModuleKind::PhysicalComponent));
    // the design block's instance exists as a binding skeleton, its
    // children exist as physical parts
    assert!(n.instances.iter().any(|(_, i)| i.name == "div"));
    assert!(n.instances.iter().any(|(_, i)| i.name == "div_R_top"), "{:?}",
        n.instances.iter().map(|(_, i)| i.name.clone()).collect::<Vec<_>>());
    // the safety parts view excludes the design block, includes children
    let model = build_safety_model(&n, &[&sf]);
    let part_names: Vec<&str> = model.parts.iter().map(|p| p.instance.as_str()).collect();
    assert!(!part_names.contains(&"div"), "design block must not be a part: {part_names:?}");
    assert!(part_names.contains(&"div_R_top"), "{part_names:?}");

    // contradiction: as design + package identity = hard error
    let bad = r#"
entity BadBlock() as design {
    pin 1: signal in;
    attribute kicad_symbol = "Device:R";
}
board B {
    ground GND;
    b: BadBlock();
    b.1 -> @GND;
}
"#;
    let pr2 = parse(bad);
    assert!(pr2.errors().is_empty(), "{:?}", pr2.errors());
    let sf2 = SourceFile::cast(pr2.syntax()).unwrap();
    let analysis2 = analyze(&sf2);
    let mut gen2 = NetlistGenerator::new();
    let err = gen2.generate_from_ast_and_analysis(&sf2, &analysis2).await.expect_err("contradiction");
    assert!(format!("{err:#}").contains("no body to solder"), "{err:#}");
}

/// Definition-template stubs are MARKED at creation (`template=true`)
/// and judged by the one helper — the five consumer sites no longer
/// each re-derive the name-shape heuristic.
#[tokio::test]
async fn template_stubs_are_marked_at_creation() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let src = std::fs::read_to_string(ws.join("tests/circuits/realistic/test_safety_fault_campaign.bhdl")).unwrap();
    let pr = parse(&src);
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.unwrap();
    // the DemoSense definition stub carries the explicit marker
    let (stub_id, stub) = n
        .instances
        .iter()
        .find(|(_, i)| i.name == "DemoSense")
        .expect("definition stub exists");
    assert_eq!(stub.attributes.get("template").map(String::as_str), Some("true"), "{:#?}", stub.attributes);
    assert!(bhdl_synthesizer::is_template_stub(&n, stub_id));
    // a real, connected instance is never a stub
    let (real_id, _) = n.instances.iter().find(|(_, i)| i.name == "sense").unwrap();
    assert!(!bhdl_synthesizer::is_template_stub(&n, real_id));
}

/// Hierarchical accessors: `div.R_top.2` on the BOARD reaches the
/// composition child whose flat identity is `div_R_top` — the dotted
/// form is the language surface, the mangled name stays the netlist
/// identity. (The safety model's NetView::resolve already walked
/// dotted paths; connectivity now matches.)
#[tokio::test]
async fn hierarchical_accessor_reaches_composition_children() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let src = r#"
import { Res } from "bhdl-stdlib/passives/resistor.bhdl";

entity DividerBlock(r_top: resistance = 10kΩ, r_bot: resistance = 10kΩ) as design {
    pin VIN: power in;
    pin VOUT: signal out virtual;
    pin GND: ground;
    expansion {
        VIN -> R_top: Res(r_top).1; R_top.2 -> VOUT;
        VOUT -> R_bot: Res(r_bot).1; R_bot.2 -> GND;
    }
}

board AccessorDemo {
    power V5 = 5V @ 1A;
    ground GND;
    @V5 -> div: DividerBlock().VIN;
    div.GND -> @GND;

    // the accessor: reach INSIDE the composition — probe the divider
    // midpoint through the child, not through a board-side net name
    div.R_top.2 -> tp: Res(1MΩ).1;
    tp.2 -> @GND;
}
"#;
    let pr = parse(src);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");

    // net of a named instance pin
    let pin_net = |inst: &str, pin: &str| -> Option<bhdl_netlist::types::NetId> {
        n.pin_instances.values().find_map(|pi| {
            let i = n.instances.get(pi.instance)?;
            if i.name != inst { return None; }
            let p = n.pins.get(pi.pin_def)?;
            if p.name != pin { return None; }
            pi.net
        })
    };
    let child_net = pin_net("div_R_top", "2").expect("child pin wired");
    let probe_net = pin_net("tp", "1").expect("probe wired");
    assert_eq!(child_net, probe_net, "accessor must land the probe on the child's net");
}

/// Plain-body composition: an `entity … as design` composes by PLAIN
/// body statements — FIRM children (composed_parent marker, no
/// takeover/gating), nesting via the fixpoint (a block instantiating
/// a block), and a self-reachable entity is a hard cycle error.
#[tokio::test]
async fn plain_body_composition_firm_nested_and_cyclic() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let src = r#"
import { Res } from "bhdl-stdlib/passives/resistor.bhdl";

entity Inner(r: resistance = 1kΩ) as design {
    pin A: signal in;
    pin B: signal out virtual;
    A -> r1: Res(r).1;
    r1.2 -> B;
}

entity Outer() as design {
    pin VIN: power in;
    pin VOUT: signal out virtual;
    pin GND: ground;
    VIN -> stage: Inner(2kΩ).A;
    stage.B -> VOUT;
    VOUT -> r_load: Res(10kΩ).1;
    r_load.2 -> GND;
}

board Composed {
    power V5 = 5V @ 1A;
    ground GND;
    @V5 -> blk: Outer().VIN;
    blk.GND -> @GND;
}
"#;
    let pr = parse(src);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    assert!(analysis.expansion_recipes.get("Outer").map(|r| r.firm).unwrap_or(false), "Outer recipe is FIRM");
    assert!(analysis.expansion_recipes.get("Inner").map(|r| r.firm).unwrap_or(false));
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let names: Vec<String> = n.instances.iter().map(|(_, i)| i.name.clone()).collect();
    // round 1: Outer's firm children; round 2: the nested Inner's child
    assert!(names.iter().any(|x| x == "blk_stage"), "{names:?}");
    assert!(names.iter().any(|x| x == "blk_r_load"), "{names:?}");
    assert!(names.iter().any(|x| x == "blk_stage_r1"), "NESTED composition child: {names:?}");
    // definition-scope symbol instances are TEMPLATES — they never
    // expand into board-level junk (no bare stage_r1)
    assert!(!names.iter().any(|x| x == "stage_r1"), "{names:?}");
    // firm children carry composed_parent, never expansion_parent
    let (_, child) = n.instances.iter().find(|(_, i)| i.name == "blk_r_load").unwrap();
    assert_eq!(child.attributes.get("composed_parent").map(String::as_str), Some("blk"), "{:#?}", child.attributes);
    assert!(!child.attributes.contains_key("expansion_parent"));

    // cycle: an entity reachable from itself is a hard error
    let cyc = r#"
entity Loop() as design {
    pin A: signal in;
    A -> again: Loop().A;
}
board Cyc {
    ground GND;
    l: Loop();
    l.A -> @GND;
}
"#;
    let pr2 = parse(cyc);
    assert!(pr2.errors().is_empty(), "{:?}", pr2.errors());
    let sf2 = SourceFile::cast(pr2.syntax()).unwrap();
    let analysis2 = analyze(&sf2);
    let mut gen2 = NetlistGenerator::new();
    let err = gen2.generate_from_ast_and_analysis(&sf2, &analysis2).await.expect_err("cycle must error");
    assert!(format!("{err:#}").contains("cycle"), "{err:#}");
}

/// Drift check — the spreadsheet's silent-rot problem as a gate: the
/// emitted stages carry their sizing; when the board's loads evolve
/// past them (current growth, or a new noise-sensitive load on a buck
/// rail), the drift check names the stage and the arithmetic.
#[tokio::test]
async fn powertree_drift_check_catches_evolved_loads() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    use bhdl_synthesizer::powertree::{check_drift, emit_power_region, harvest_loads, propose_trees, splice_power_region};
    let src = std::fs::read_to_string(ws.join("tests/circuits/realistic/test_powertree_loads.bhdl")).unwrap();
    let pr = parse(&src);
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let netlist = gen.generate_from_ast_and_analysis(&sf, &analysis).await.unwrap();
    let h = harvest_loads(&netlist, &sf);
    let opts = propose_trees(&h, "VIN").unwrap();
    let emitted = resolve_emitted(ws, &splice_power_region(&src, &emit_power_region(&opts[0], "GND")).unwrap());

    let build = |text: &str| {
        let pr = parse(text);
        assert!(pr.errors().is_empty(), "{:?}", pr.errors());
        SourceFile::cast(pr.syntax()).unwrap()
    };

    // fresh emit: the plan covers the board — NO drift
    let sf2 = build(&emitted);
    let analysis2 = analyze(&sf2);
    let mut gen2 = NetlistGenerator::new();
    let n2 = gen2.generate_from_ast_and_analysis(&sf2, &analysis2).await.unwrap();
    assert!(check_drift(&n2, &sf2).is_empty(), "{:#?}", check_drift(&n2, &sf2));

    // current growth: core i_max 4A → 9A outgrows the 5A-sized stage
    let grown = emitted.replace("i_nom=2A i_max=4A", "i_nom=6A i_max=9A");
    assert_ne!(grown, emitted);
    let sf3 = build(&grown);
    let analysis3 = analyze(&sf3);
    let mut gen3 = NetlistGenerator::new();
    let n3 = gen3.generate_from_ast_and_analysis(&sf3, &analysis3).await.unwrap();
    let d3 = check_drift(&n3, &sf3);
    assert!(
        d3.iter().any(|f| f.stage == "u_v1v0" && f.kind == "rating" && f.detail.contains("OUTGREW")),
        "{d3:#?}"
    );

    // noise drift: the core load grows a 50µVrms requirement — a buck
    // rail assuming 500µVrms output can no longer serve it
    let quieter = emitted.replace(
        "domain VDD_CORE pins=\"1\" v=1.0V tol=3% i_nom=2A i_max=4A",
        "domain VDD_CORE pins=\"1\" v=1.0V tol=3% i_nom=2A i_max=4A noise=50uV",
    );
    assert_ne!(quieter, emitted);
    let sf4 = build(&quieter);
    let analysis4 = analyze(&sf4);
    let mut gen4 = NetlistGenerator::new();
    let n4 = gen4.generate_from_ast_and_analysis(&sf4, &analysis4).await.unwrap();
    let d4 = check_drift(&n4, &sf4);
    assert!(
        d4.iter().any(|f| f.stage == "u_v1v0" && f.kind == "noise" && f.detail.contains("post-regulation")),
        "{d4:#?}"
    );
}


/// Always-on noise rail under a prereg policy with no always-on donor
/// in reach: the tree mints an ALWAYS-ON intermediate fed direct from
/// the input (never behind the protected front end), at minimal
/// headroom, feeding the AO LDO — what used to be the stated hard gap.
#[tokio::test]
async fn powertree_always_on_noise_rail_gets_ao_intermediate() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    use bhdl_synthesizer::powertree::{harvest_loads, propose_trees_with_policy, Topology};
    // RTC rail: always-on, noise-sensitive, and at 60mA a direct 12V LDO
    // dissipates (12-1.8)·0.06 = 0.61W > the 0.5W bound → needs an
    // intermediate; the only other rail (3.3V core) sits behind the
    // front end, so it can NOT be the donor.
    let src = r#"
entity Mcu() {
    pin 1: power in;
    pin 2: power in;
    pin 3: ground;
    attribute component_class = "ic";
    domain VDD pins="1" v=3.3V i_nom=0.5A i_max=0.8A source="FIXTURE";
    domain VRTC pins="2" v=1.8V i_nom=50mA i_max=60mA noise=100uV always_on=true source="FIXTURE";
}
board AoNoise {
    power VIN = 12V @ 2A;
    power V3V3 = 3.3V;
    power VRTC = 1.8V;
    ground GND;
    @V3V3 -> mcu: Mcu().1;
    @VRTC -> mcu.2;
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
    let opts = propose_trees_with_policy(&h, "VIN", Some("OV/UV")).expect("no longer a stated gap");
    for o in &opts {
        // the AO intermediate exists, fed DIRECT from VIN at minimal headroom
        let ao_int = o.stages.iter().find(|s| s.to.starts_with("V_INT_AO_")).expect("AO intermediate: {o:#?}");
        assert_eq!(ao_int.from, "VIN", "AO intermediate must bypass the front end: {ao_int:#?}");
        assert!((ao_int.vout - 2.3).abs() < 1e-9, "{ao_int:#?}");
        // the AO LDO hangs off it
        let rtc = o.stages.iter().find(|s| s.to == "VRTC").unwrap();
        assert_eq!(rtc.topology, Topology::Ldo);
        assert_eq!(rtc.from, ao_int.to);
        // the ordinary rail is still behind the protection
        let v33 = o.stages.iter().find(|s| s.to == "V3V3").unwrap();
        assert_eq!(v33.from, "V_PROT");
        // the protection stage does NOT carry the AO path's current
        let prot = o.stages.iter().find(|s| s.topology == Topology::Prereg).unwrap();
        assert!(!prot.serves.iter().any(|x| x.contains("VRTC")));
        // stated in the notes
        assert!(o.notes.iter().any(|nn| nn.contains("always-on intermediate") && nn.contains("DIRECT")), "{:#?}", o.notes);
        // books balance through both paths
        assert!((o.p_in_w - o.p_load_w - o.p_diss_w).abs() < 1e-6, "{o:#?}");
    }
}

/// Two-layer library model, first increment (Requirements_And_Resolution
/// §5.1): `TPS54331 as part` (vendor truth) + `Buck_TPS54331 as design`
/// (the reviewed subcircuit). A board instantiating the BLOCK gets the
/// part + sized application circuit; the block carries no BOM line; the
/// silicon's internals are reachable through the accessor; and the
/// block's validity envelope (3A × 0.8 derating) is a hard error.
#[tokio::test]
async fn tps54331_part_block_split() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let board = |i_max: &str| format!(r#"
import {{ Buck_TPS54331 }} from "bhdl-stdlib/power/tps54331.bhdl";
import {{ Res }} from "bhdl-stdlib/passives/resistor.bhdl";
board SplitDemo {{
    power VIN = 12V @ 3A;
    port V5: power out = 5V @ 2A;
    ground GND;
    @VIN -> U1: Buck_TPS54331(v_out=5V, i_out_max={i_max}).VIN;
    U1.GND -> @GND; U1.EN -> @VIN;
    U1.VOUT -> @V5;
    @V5 -> R_LOAD: Res(2.5Ω, wattage=10W).1; R_LOAD.2 -> @GND;
    U1.u.SW -> tp: Res(1MΩ).1; tp.2 -> @GND;
}}
"#);
    let pr = parse(&board("2A"));
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");

    let kind_of = |name: &str| {
        let i = n.instances.values().find(|i| i.name == name).unwrap_or_else(|| panic!("instance {name}"));
        n.modules.get(i.definition).map(|m| m.kind.clone()).unwrap()
    };
    assert_eq!(kind_of("U1"), bhdl_netlist::types::ModuleKind::DesignBlock, "the block is a design block");
    assert_ne!(kind_of("U1_u"), bhdl_netlist::types::ModuleKind::DesignBlock, "U1.u is the silicon (a part)");
    for child in ["U1_L_out", "U1_C_out", "U1_C_in", "U1_D_catch", "U1_R_top", "U1_R_bot", "U1_C_boot"] {
        assert!(n.instances.values().any(|i| i.name == child), "block child {child} materialised");
    }
    let pin_net = |inst: &str, pin: &str| -> Option<bhdl_netlist::types::NetId> {
        n.pin_instances.values().find_map(|pi| {
            let i = n.instances.get(pi.instance)?;
            if i.name != inst { return None; }
            let p = n.pins.get(pi.pin_def)?;
            if p.name != pin { return None; }
            pi.net
        })
    };
    assert_eq!(pin_net("U1_u", "SW"), pin_net("tp", "1"), "accessor U1.u.SW reaches the part's switch node");
    assert_eq!(pin_net("U1_u", "SW"), pin_net("U1_L_out", "1"), "inductor hangs on the part's SW");
    assert_eq!(pin_net("U1_L_out", "2"), pin_net("R_LOAD", "1"), "virtual VOUT lands the load on the inductor output");

    // envelope: 2.6A > 2.4A (3A rating × 0.8 derate) is a hard error
    let pr = parse(&board("2.6A"));
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let r = gen.generate_from_ast_and_analysis(&sf, &analysis).await;
    let err = match r { Ok(_) => panic!("envelope violation must fail synthesis"), Err(e) => format!("{e:#}") };
    assert!(err.contains("envelope"), "envelope message surfaced: {err}");
}

/// Requirement interfaces end to end (Requirements_And_Resolution §3,
/// increment 2): `u1: BuckStage(...)` resolves to `Buck_TPS54331` by
/// trial-instantiation; an out-of-envelope requirement is UNRESOLVED
/// with every gate stated and becomes the Generic placeholder; an
/// `LdoStage` has no implementing block yet (stated, not faked); a
/// `resolve` override that fails its gates is a hard error; the lock
/// binding is tried first and re-resolved loudly when stale; and an
/// unresolved trait instantiation can never reach synthesis silently.
#[tokio::test]
async fn stage_requirements_resolve_lock_override_and_refuse() {
    use bhdl_synthesizer::stage_resolution::resolve_stages;
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let stdlib = ws.join("bhdl-stdlib");
    let board = |args: &str, extra: &str| format!(r#"
import {{ BuckStage, LdoStage }} from "bhdl-stdlib/power/stages.bhdl";
import {{ Res }} from "bhdl-stdlib/passives/resistor.bhdl";
board ReqDemo {{
    power VIN = 12V @ 3A;
    port V5: power out = 5V @ 2A;
    ground GND;
    @VIN -> u1: BuckStage({args}).VIN; // BuckStage(in a comment — µ≤ multibyte)
    u1.GND -> @GND; u1.VOUT -> @V5;
    @V5 -> R_LOAD: Res(2.5Ω, wattage=10W).1; R_LOAD.2 -> @GND;
    {extra}
}}
"#);

    // 1. survey resolves
    let r = resolve_stages(&board("vout=5V, i_max=2A, vin=12V", ""), &stdlib, &[]).unwrap().unwrap();
    assert_eq!(r.resolutions.len(), 1);
    let u1 = &r.resolutions[0];
    // two 3A blocks cover this (TPS54302, TPS54331): no cost data → least
    // over-rating tie-break, ties by library order — stated in the note
    let b = u1.bound.clone().expect("bound");
    assert!(b == "Buck_TPS54302" || b == "Buck_TPS54331", "{}", bhdl_synthesizer::stage_resolution::render_report(u1));
    assert_eq!(u1.basis, "survey");
    // ranking basis is ALWAYS stated: real catalogue price when the
    // supplier provider priced every survivor, else least over-rating
    assert!(u1.notes.iter().any(|n| n.contains("ranked by catalogue price") || n.contains("least over-rated")), "{:?}", u1.notes);
    if u1.notes.iter().any(|n| n.contains("ranked by catalogue price")) {
        let b = u1.bound.as_deref().unwrap();
        let chosen = u1.candidates.iter().find(|c| c.block == b).unwrap();
        assert!(u1.candidates.iter().filter(|c| c.passes()).all(|c| c.ic_price.unwrap() >= chosen.ic_price.unwrap()), "cheapest silicon wins");
    }
    assert!(r.source.contains(&format!("u1: {b}(v_out=5V, i_out_max=2A, v_in=12V)")), "{}", r.source);
    assert!(r.source.contains(&format!("import {{ {b} }} from \"bhdl-stdlib/power/")), "{}", r.source);
    assert!(r.source.contains("attribute u1.stage_requirement = \"vout=5V, i_max=2A, vin=12V\";"), "{}", r.source);
    // the resolved text synthesizes and the block composes
    let pr = parse(&r.source);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    assert!(n.instances.values().any(|i| i.name == "u1_u"), "the silicon materialised");
    assert!(n.instances.values().any(|i| i.name == "u1_L_out"));

    // 2. out of envelope + noise requirement: every gate stated, placeholder
    let r = resolve_stages(&board("vout=5V, i_max=2.6A, vin=12V, noise=50mV", ""), &stdlib, &[]).unwrap().unwrap();
    let u1 = &r.resolutions[0];
    assert!(u1.bound.is_none());
    assert_eq!(u1.basis, "unresolved");
    let c = u1.candidates.iter().find(|c| c.block == "Buck_TPS54331").unwrap();
    assert!(!c.passes());
    let fails = c.failures().join("\n");
    assert!(fails.contains("envelope") && fails.contains("2.4A"), "{fails}");
    assert!(fails.contains("i_max") && fails.contains("3.250A"), "{fails}");
    assert!(fails.contains("noise") && fails.contains("UNCHECKED"), "{fails}");
    assert!(r.source.contains("u1: GenericBuck(vin=12V, vout=5V, rated=2.6A)"), "{}", r.source);
    assert!(r.source.contains("attribute u1.powertree_rating_required_a = \"3.2500\";"), "{}", r.source);

    // 3. LDO: Ldo_LP2985 resolves inside its envelope (SKU voltage,
    //    ≤120mA, 30µV noise); outside it the near-miss is stated and the
    //    placeholder is emitted
    let ldo = board("vout=5V, i_max=2A, vin=12V", "@V5 -> u2: LdoStage(vout=3.3V, i_max=100mA, vin=5V, noise=30uV).VIN; u2.GND -> @GND;");
    let r = resolve_stages(&ldo, &stdlib, &[]).unwrap().unwrap();
    let u2 = r.resolutions.iter().find(|x| x.instance == "u2").unwrap();
    assert_eq!(u2.bound.as_deref(), Some("Ldo_LP2985"), "{}", bhdl_synthesizer::stage_resolution::render_report(u2));
    assert!(r.source.contains("u2: Ldo_LP2985(v_out=3.3V, i_out_max=100mA, v_in=5V)"), "{}", r.source);
    let ldo = board("vout=5V, i_max=2A, vin=12V", "@V5 -> u2: LdoStage(vout=3.6V, i_max=200mA, vin=5V).VIN; u2.GND -> @GND;");
    let r = resolve_stages(&ldo, &stdlib, &[]).unwrap().unwrap();
    let u2 = r.resolutions.iter().find(|x| x.instance == "u2").unwrap();
    assert!(u2.bound.is_none());
    let fails = u2.candidates.iter().find(|c| c.block == "Ldo_LP2985").unwrap().failures().join("\n");
    assert!(fails.contains("SKU voltage") || fails.contains("120mA"), "{fails}");
    assert!(r.source.contains("u2: GenericLdo(vin=5V, vout=3.6V, rated=200mA)"), "{}", r.source);

    // 4. override: accepted when it passes; hard error when it fails / is unknown
    let r = resolve_stages(&board("vout=5V, i_max=2A, vin=12V", "resolve u1 = Buck_TPS54331;"), &stdlib, &[]).unwrap().unwrap();
    assert_eq!(r.resolutions[0].basis, "override");
    assert!(!r.source.contains("\n    resolve u1"), "override statement consumed: {}", r.source);
    let e = resolve_stages(&board("vout=5V, i_max=2.6A, vin=12V", "resolve u1 = Buck_TPS54331;"), &stdlib, &[]).unwrap_err();
    assert!(format!("{e:#}").contains("does not meet the requirement"), "{e:#}");
    let e = resolve_stages(&board("vout=5V, i_max=2A, vin=12V", "resolve u1 = Buck_NoSuch;"), &stdlib, &[]).unwrap_err();
    assert!(format!("{e:#}").contains("no `impl BuckStage for Buck_NoSuch`"), "{e:#}");

    // 4b. BuckExtStage: the BuckController TEMPLATE is listed, never
    //     auto-bound; an override WITH the designer's power-stage args
    //     commits it (UNCHECKED vin range stated, not blocking), the args
    //     reach the FET children, and the board round-trips
    let ext = |args: &str, extra: &str| format!(r#"
import {{ BuckExtStage }} from "bhdl-stdlib/power/stages.bhdl";
import {{ Res }} from "bhdl-stdlib/passives/resistor.bhdl";
board ExtDemo {{
    power VIN = 24V @ 10A;
    port V5: power out = 5V @ 8A;
    ground GND;
    @VIN -> u1: BuckExtStage({args}).VIN;
    u1.GND -> @GND; u1.VOUT -> @V5;
    @V5 -> R_LOAD: Res(0.625Ω, wattage=50W).1; R_LOAD.2 -> @GND;
    {extra}
}}
"#);
    let r = resolve_stages(&ext("vout=5V, i_max=8A, vin=24V", ""), &stdlib, &[]).unwrap().unwrap();
    assert!(r.resolutions[0].bound.is_none());
    assert!(r.resolutions[0].candidates.iter().any(|c| c.block == "BuckController" && c.template));
    let ovr = "resolve u1 = BuckController(hs_fet=\"BSC0902NS\", ls_fet=\"BSC0902NS\", fet_rds_on=2.3mΩ, fet_vds_max=30V, fet_id_max=30A, fet_vgs_th=2.4V, fet_p_rating=42W); u1.EN -> @VIN;";
    let r = resolve_stages(&ext("vout=5V, i_max=8A, vin=24V", ovr), &stdlib, &[]).unwrap().unwrap();
    assert_eq!(r.resolutions[0].basis, "override");
    assert!(r.resolutions[0].notes.iter().any(|n| n.contains("TEMPLATE")), "{:?}", r.resolutions[0].notes);
    assert!(r.resolutions[0].notes.iter().any(|n| n.contains("UNCHECKED promise")), "{:?}", r.resolutions[0].notes);
    assert!(r.source.contains("hs_fet=\"BSC0902NS\""), "{}", r.source);
    assert!(r.source.contains("u1.EN -> @VIN;"), "the statement sharing the override's line survives: {}", r.source);
    {
        let pr = parse(&r.source);
        assert!(pr.errors().is_empty(), "{:?}", pr.errors());
        let sf = SourceFile::cast(pr.syntax()).unwrap();
        let analysis = analyze(&sf);
        let mut gen = NetlistGenerator::new();
        let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
        let fet = n.instances.values().find(|i| i.name == "u1_M_hs").expect("high-side FET minted");
        assert_eq!(fet.attributes.get("part_number").map(String::as_str), Some("BSC0902NS"), "{:#?}", fet.attributes);
        assert_eq!(fet.attributes.get("rds_on").map(String::as_str), Some("2.3mΩ"), "{:#?}", fet.attributes);
    }
    // a multi-phase requirement cannot be committed to the single-phase template
    let e = resolve_stages(&ext("vout=5V, i_max=60A, vin=24V, phases=3", ovr), &stdlib, &[]).unwrap_err();
    assert!(format!("{e:#}").contains("phases: supports 1 phase(s) ≥ required 3"), "{e:#}");

    // 4c. temperature: the TI bucks are JUNCTION-rated (no ambient range in
    //     SLVS839H / SLVSDG6C) and declare θ_JA + T_J,max, so an ambient
    //     requirement is met THERMALLY at the requirement's dissipation:
    //     0.5 A at 60 °C → T_J ≈ 96 / 86 °C ≤ 150 passes; 2 A at 85 °C on the
    //     JEDEC board at 125 °C ambient exceeds T_J,max. AP63205 is
    //     AMBIENT-rated (DS41326: TA −40…85 °C) and passes 60 °C directly.
    let r = resolve_stages(&board("vout=5V, i_max=0.5A, vin=12V, temp_max=60degC", ""), &stdlib, &[]).unwrap().unwrap();
    assert!(r.resolutions[0].bound.is_some(), "{}", bhdl_synthesizer::stage_resolution::render_report(&r.resolutions[0]));
    let c331 = r.resolutions[0].candidates.iter().find(|c| c.block == "Buck_TPS54331").unwrap();
    let tg = c331.gates.iter().find(|g| g.0 == "temp_max").unwrap();
    // the IC's own physics loss (I²·R_ds·D + V·I·f_sw·t_sw + V·Iq), not
    // the whole-stage η estimate: 0.119 W at 0.5 A
    assert!(tg.2 && tg.1.contains("thermal: T_J = 60°C + 0.119W × 116.3°C/W = 73.8°C"), "{tg:?}");
    let c205 = r.resolutions[0].candidates.iter().find(|c| c.block == "Buck_AP63205").unwrap();
    assert!(c205.gates.iter().any(|g| g.0 == "temp_max" && g.2 && g.1.contains("temp_max 85°C ≥ required 60°C")), "{c205:#?}");
    // 2 A at 85 °C PASSES with the IC's own physics loss (~0.46 W → 139 °C);
    // at 125 °C ambient the same loss exceeds T_J,max — refused
    let r = resolve_stages(&board("vout=5V, i_max=2A, vin=12V, temp_max=125degC", ""), &stdlib, &[]).unwrap().unwrap();
    assert!(r.resolutions[0].bound.is_none(), "2 A at 125 °C exceeds T_J,max on the JEDEC board: {}", bhdl_synthesizer::stage_resolution::render_report(&r.resolutions[0]));
    let c331 = r.resolutions[0].candidates.iter().find(|c| c.block == "Buck_TPS54331").unwrap();
    assert!(c331.gates.iter().any(|g| g.0 == "temp_max" && !g.2 && g.1.contains("≤ T_J,max 150°C")), "{c331:#?}");
    let r = resolve_stages(&board("vout=5V, i_max=2A, vin=12V, qual=\"AEC-Q100\"", ""), &stdlib, &[]).unwrap().unwrap();
    assert!(r.resolutions[0].bound.is_none());
    assert!(r.resolutions[0].candidates.iter().all(|c| c.unchecked.contains(&"qual".to_string())), "no stdlib part declares a qualification");

    // 5. lock: tried first; stale lock re-resolved loudly
    let lock = vec![bhdl_common::library::LockedStage {
        board: "ReqDemo".into(),
        instance: "u1".into(),
        trait_name: "BuckStage".into(),
        requirement: "vout=5V, i_max=1A, vin=12V".into(),
        block: "Buck_TPS54331".into(),
    }];
    let r = resolve_stages(&board("vout=5V, i_max=2A, vin=12V", ""), &stdlib, &lock).unwrap().unwrap();
    assert_eq!(r.resolutions[0].basis, "lock");
    assert!(r.resolutions[0].notes.iter().any(|n| n.contains("requirement changed since lock")), "{:?}", r.resolutions[0].notes);
    let r = resolve_stages(&board("vout=5V, i_max=2.6A, vin=12V", ""), &stdlib, &lock).unwrap().unwrap();
    assert_eq!(r.resolutions[0].basis, "unresolved");
    assert!(r.resolutions[0].notes.iter().any(|n| n.contains("no longer meets")), "{:?}", r.resolutions[0].notes);
    // a different board's lock entry is not this board's
    let other = vec![bhdl_common::library::LockedStage { board: "Other".into(), ..lock[0].clone() }];
    let r = resolve_stages(&board("vout=5V, i_max=2A, vin=12V", ""), &stdlib, &other).unwrap().unwrap();
    assert_eq!(r.resolutions[0].basis, "survey");

    // 6. an unresolved requirement can never synthesize silently
    let raw = board("vout=5V, i_max=2A, vin=12V", "");
    let pr = parse(&raw);
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let err = gen.generate_from_ast_and_analysis(&sf, &analysis).await.err().expect("must refuse");
    assert!(format!("{err:#}").contains("BuckStage"), "{err:#}");
}

/// The per-build trace matrix (Requirements_And_Resolution §4, increment
/// 3): rows are DERIVED from constructs with verifiers — stage
/// requirements, rail budgets, vendor domain contracts, part-carried
/// checks — each ending in evidence; "no verifier ran" is UNVERIFIED,
/// never a pass; explicit ids via `attribute x.requirement_id`;
/// `satisfies { ID: via inst; }` links land on the row, dangling ones are
/// findings.
#[tokio::test]
async fn trace_matrix_derives_rows_with_evidence_and_links() {
    use bhdl_synthesizer::trace_matrix::{build_trace_matrix, TraceStatus};
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    use bhdl_synthesizer::powertree::{emit_power_region, harvest_loads, propose_trees, splice_power_region};
    let src = std::fs::read_to_string(ws.join("tests/circuits/realistic/test_powertree_loads.bhdl")).unwrap();
    let pr = parse(&src);
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let netlist = gen.generate_from_ast_and_analysis(&sf, &analysis).await.unwrap();
    let h = harvest_loads(&netlist, &sf);
    let opts = propose_trees(&h, "VIN").unwrap();
    let emitted = resolve_emitted(ws, &splice_power_region(&src, &emit_power_region(&opts[0], "GND")).unwrap());
    // explicit id + satisfies links (one valid, one dangling requirement, one dangling element)
    // (the generated region ends the board body — append right after it)
    let end_marker = bhdl_synthesizer::powertree::EMIT_END;
    let at = emitted.find(end_marker).expect("emit marker") + end_marker.len();
    let mut emitted = emitted;
    emitted.insert_str(
        at,
        "\n    attribute u_v3v3.requirement_id = \"PWR_3V3\";\n    satisfies {\n        PWR_3V3: via u_v3v3_u;\n        NOPE_1: via u_v3v3;\n        PWR_3V3: via ghost;\n    }",
    );
    let pr = parse(&emitted);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let mut checker = bhdl_synthesizer::design_rule_checker::DesignRuleChecker::new(
        bhdl_synthesizer::design_rule_checker::IndustryStandard::IPC2221,
    );
    let drc = checker.run_checks(&n, &analysis);
    let safety = bhdl_synthesizer::safety_model::build_safety_model(&n, &[&sf]);
    let m = build_trace_matrix(&n, &analysis, &sf, &drc.violations, Some(&safety), false);
    let row = |id: &str| m.rows.iter().find(|r| r.id == id).unwrap_or_else(|| panic!("row {id}: {:#?}", m.rows.iter().map(|r| &r.id).collect::<Vec<_>>()));

    // stage requirements: unresolved / resolved-with-implementers
    assert_eq!(row("PowertreeDemo.u_v1v0").status, TraceStatus::Unresolved);
    let ldo = row("PowertreeDemo.u_v1v8");
    assert!(ldo.implemented_by.iter().any(|i| i == "u_v1v8 : Ldo_LP2985"), "{ldo:#?}");
    assert!(ldo.implemented_by.iter().any(|i| i == "u_v1v8_u"), "{ldo:#?}");
    // explicit id replaces the derived one; the satisfies link landed
    assert!(m.rows.iter().all(|r| r.id != "PowertreeDemo.u_v3v3"));
    let buck = row("PWR_3V3");
    assert!(buck.implemented_by.iter().any(|i| i == "u_v3v3_u (declared)"), "{buck:#?}");
    // a promise the part does not declare = UNVERIFIED, never a pass
    assert_eq!(buck.status, TraceStatus::Unverified, "{buck:#?}");
    assert!(buck.evidence.contains("UNCHECKED"), "{buck:#?}");
    // vendor domain contracts with no decouple = UNVERIFIED, once per real instance (no template-stub duplicates)
    let domains: Vec<_> = m.rows.iter().filter(|r| r.kind == "vendor domain contract").collect();
    assert_eq!(domains.len(), 3, "{:#?}", domains.iter().map(|r| &r.id).collect::<Vec<_>>());
    assert!(domains.iter().all(|r| r.status == TraceStatus::Unverified && r.evidence.contains("no `decouple`")));
    // part-carried check (LP2985 EN rule) verified on the netlist
    let chk = row("PowertreeDemo.u_v1v8_u.check[0]");
    assert_eq!(chk.status, TraceStatus::Verified, "{chk:#?}");
    assert_eq!(chk.verifier, "ERC025");
    // rails: every declared rail has a row with the ERC028 verdict
    assert!(m.rows.iter().any(|r| r.id == "PowertreeDemo.rail.VIN" && r.status == TraceStatus::Verified));
    // dangling links are findings
    assert!(m.findings.iter().any(|f| f.starts_with("satisfies NOPE_1")), "{:#?}", m.findings);
    assert!(m.findings.iter().any(|f| f.contains("'ghost' is not an instance")), "{:#?}", m.findings);
    assert!(!m.clean());
    let md = bhdl_synthesizer::trace_matrix::render_markdown(&m);
    assert!(md.contains("| PWR_3V3 |") && md.contains("### Findings"), "{md}");
}

/// `where` envelope spelling + `generate if (wired(PIN))` gating
/// (Requirements_And_Resolution §2.2 / §2.4): the where clause lowers to
/// `require`s that BOTH the resolver's trial-instantiation and synthesis
/// evaluate; an optional contract pin's then/else branches fire by the
/// board's wiring, the unwired case is not a floating input, and the
/// elaborated board round-trips either way.
#[tokio::test]
async fn where_envelope_and_wired_gating() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let board = |i_max: &str, en: &str| format!(r#"
import {{ Cap }} from "bhdl-stdlib/passives/capacitor.bhdl";
entity Blk(v_in: voltage = 5V, i_out_max: current = 100mA) as design where v_in <= 16V, i_out_max <= 120mA {{
    pin VIN: power in;
    pin VOUT: power out virtual;
    pin EN: signal in;
    pin GND: ground;
    VIN -> C_in: Cap(1uF).1; C_in.2 -> GND;
    VIN -> VOUT;
    generate if (wired(EN)) {{
        EN -> C_en: Cap(10nF).1; C_en.2 -> GND;
    }} else {{
        VIN -> C_tie: Cap(1nF).1; C_tie.2 -> GND;
    }}
}}
board GateDemo {{
    power V5 = 5V @ 1A;
    ground GND;
    @V5 -> b: Blk(i_out_max={i_max}).VIN; b.GND -> @GND;
    {en}
}}
"#);
    let synth = |src: String| async move {
        let pr = parse(&src);
        assert!(pr.errors().is_empty(), "{:?}", pr.errors());
        let sf = SourceFile::cast(pr.syntax()).unwrap();
        let analysis = analyze(&sf);
        let mut gen = NetlistGenerator::new();
        (gen.generate_from_ast_and_analysis(&sf, &analysis).await, analysis)
    };
    // where → design recipe requires (front of the plain recipe)
    let (n, analysis) = synth(board("100mA", "")).await;
    let n = n.expect("inside the envelope synthesizes");
    let recipe = &analysis.design_recipes["Blk"]["<plain>"];
    assert!(matches!(&recipe.statements[0], bhdl_common::design::DesignStatement::Require { condition, message } if condition == "self.v_in <= 16V" && message.contains("where v_in <= 16V")), "{:#?}", recipe.statements);
    // unwired EN → else-branch child, then-branch child absent; no ERC006 on the gated pin
    let names: Vec<String> = n.instances.values().map(|i| i.name.clone()).collect();
    assert!(names.contains(&"b_C_tie".to_string()) && !names.contains(&"b_C_en".to_string()), "{names:?}");
    let b = n.instances.values().find(|i| i.name == "b").unwrap();
    assert_eq!(b.attributes.get("gated_pins").map(String::as_str), Some("EN"));
    let v = bhdl_synthesizer::erc::check_unconnected_pins_real(&n, &analysis);
    assert!(!v.iter().any(|x| x.rule_id == "ERC006" && x.description.contains("b.EN")), "{v:#?}");
    // wired EN → then-branch
    let (n, _) = synth(board("100mA", "@V5 -> b.EN;")).await;
    let names: Vec<String> = n.unwrap().instances.values().map(|i| i.name.clone()).collect();
    assert!(names.contains(&"b_C_en".to_string()) && !names.contains(&"b_C_tie".to_string()), "{names:?}");
    // outside the envelope = hard synthesis error quoting the clause
    let (r, _) = synth(board("200mA", "")).await;
    let e = format!("{:#}", r.err().expect("envelope must refuse"));
    assert!(e.contains("Blk envelope: where i_out_max <= 120mA"), "{e}");
    // the resolver sees the same envelope (trial-instantiation)
    let text = board("100mA", "");
    let v = bhdl_synthesizer::stage_resolution::trial_envelope(&text, "Blk", &[("i_out_max".into(), "200mA".into())]).expect("has a design recipe");
    assert!(v.unwrap_err().contains("where i_out_max <= 120mA"));
}

/// ONE acceptance predicate (Requirements_And_Resolution §3): what the
/// resolver decides before binding and what ERC032 decides on the
/// flattened circuit are the same function over the same gates. Bind a
/// block the resolver accepts → ERC032 clean; bind (by override) a block
/// whose noise promise is UNDECLARED against a noise requirement →
/// resolver marks it UNCHECKED and ERC032 reports exactly that gate as
/// an UNCHECKED Info, never an Error and never silence.
#[tokio::test]
async fn resolver_and_erc032_share_one_acceptance_predicate() {
    use bhdl_synthesizer::stage_resolution::resolve_stages;
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let stdlib = ws.join("bhdl-stdlib");
    let board = |args: &str, extra: &str| format!(r#"
import {{ LdoStage }} from "bhdl-stdlib/power/stages.bhdl";
import {{ Res }} from "bhdl-stdlib/passives/resistor.bhdl";
board OnePred {{
    power V5 = 5V @ 1A;
    port V3V3: power out = 3.3V @ 100mA;
    ground GND;
    @V5 -> u2: LdoStage({args}).VIN;
    u2.GND -> @GND; u2.VOUT -> @V3V3;
    @V3V3 -> R_LOAD: Res(33Ω, wattage=1W).1; R_LOAD.2 -> @GND;
    {extra}
}}
"#);
    async fn erc(src: String) -> Vec<bhdl_synthesizer::design_rule_checker::DRCViolation> {
        let pr = parse(&src);
        assert!(pr.errors().is_empty(), "{:?}", pr.errors());
        let sf = SourceFile::cast(pr.syntax()).unwrap();
        let analysis = analyze(&sf);
        let mut gen = NetlistGenerator::new();
        let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
        bhdl_synthesizer::erc::check_powertree_acceptance(&n, &analysis)
    }
    // accepted by the resolver (LP2985 declares 30µV) → ERC032 clean on u2
    let r = resolve_stages(&board("vout=3.3V, i_max=100mA, vin=5V, noise=30uV", ""), &stdlib, &[]).unwrap().unwrap();
    assert_eq!(r.resolutions[0].bound.as_deref(), Some("Ldo_LP2985"));
    let v = erc(r.source.clone()).await;
    assert!(!v.iter().any(|x| x.description.contains("'u2'")), "{v:#?}");
    // override onto XC6206 (no output_noise declared): resolver UNCHECKED → ERC032 UNCHECKED Info on the SAME gate
    let r = resolve_stages(&board("vout=3.3V, i_max=100mA, vin=5V, noise=30uV", "resolve u2 = Ldo_XC6206;"), &stdlib, &[]).unwrap().unwrap();
    assert_eq!(r.resolutions[0].basis, "override");
    let c = r.resolutions[0].candidates.iter().find(|c| c.block == "Ldo_XC6206").unwrap();
    assert!(c.unchecked.contains(&"noise".to_string()), "{c:#?}");
    let v = erc(r.source.clone()).await;
    let noise: Vec<_> = v.iter().filter(|x| x.description.contains("'u2'") && x.description.contains("noise")).collect();
    assert_eq!(noise.len(), 1, "{v:#?}");
    assert_eq!(noise[0].severity, bhdl_synthesizer::design_rule_checker::ViolationSeverity::Info);
    assert!(noise[0].description.contains("UNCHECKED"), "{}", noise[0].description);
    assert!(!v.iter().any(|x| x.description.contains("'u2'") && x.severity == bhdl_synthesizer::design_rule_checker::ViolationSeverity::Error), "{v:#?}");
}

/// HSR evidence + HSI contracts in the trace matrix (Requirements_And_
/// Resolution §4): a supplied campaign model turns safety-goal rows into
/// measured verdicts — a detected effect that failed its FTTI is
/// VIOLATED (the campaign keeps the `FaultUnrun` class; the fault record
/// has the truth), an unrun fault is UNVERIFIED naming the fault,
/// unattributed gaps are findings; an `hsi NAME { … }` contract is
/// verified on the netlist for wiring / pin direction / source supply
/// level, and `latency_max` is stated UNVERIFIED (no verifier).
#[tokio::test]
async fn trace_hsr_evidence_and_hsi_contracts() {
    use bhdl_synthesizer::trace_matrix::{build_trace_matrix, TraceStatus};
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let src = std::fs::read_to_string(ws.join("tests/circuits/realistic/test_safety_supervised_reg.bhdl")).unwrap();
    let src = src.replacen(
        "    rail_b.nFAULT -> r_flag_b: Res(10kΩ).1; r_flag_b.2 -> @GND;\n",
        "    rail_b.nFAULT -> r_flag_b: Res(10kΩ).1; r_flag_b.2 -> @GND;\n    hsi HSI_FAULT_A { signal: r_flag_a.1; direction: input; level: 5V; active: low; source: rail_a.nFAULT; latency_max: 10ms; owner: \"fw/safety_monitor\"; }\n    hsi HSI_BAD { signal: r_flag_a.1; direction: output; level: 3.3V; source: rail_b.nFAULT; }\n    hsi HSI_GOOD { signal: r_flag_b.1; direction: input; level: 5V; source: rail_b.nFAULT; }\n    hsi HSI_TIMED { signal: r_flag_a.1; direction: input; level: 5V; source: rail_a.nFAULT; latency_max: 10ms; fw_latency: 2ms; }\n    hsi HSI_SLOW { signal: r_flag_a.1; direction: input; level: 5V; source: rail_a.nFAULT; latency_max: 1ms; fw_latency: 2ms; }\n",
        1,
    )
    // the rail-A monitor declares its response latency (FIXTURE value)
    .replacen(
        "mechanism dut.mon: psm(SG_OV, detects=[overvoltage, silent_ov], dc=0.90,",
        "mechanism dut.mon: psm(SG_OV, detects=[overvoltage, silent_ov], dc=0.90, latency=25us,",
        1,
    );
    let pr = parse(&src);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let mut checker = bhdl_synthesizer::design_rule_checker::DesignRuleChecker::new(bhdl_synthesizer::design_rule_checker::IndustryStandard::IPC2221);
    let drc = checker.run_checks(&n, &analysis);
    // the safety library the fixture imports (goals / assumptions live there)
    let lib_srcs: Vec<SourceFile> = src
        .lines()
        .filter_map(|l| l.trim().strip_prefix("import").and_then(|r| r.split('"').nth(1)).map(str::to_string))
        .filter_map(|rel| std::fs::read_to_string(ws.join(&rel)).ok())
        .filter_map(|t| SourceFile::cast(parse(&t).syntax()))
        .collect();
    let mut srcs: Vec<&SourceFile> = vec![&sf];
    srcs.extend(lib_srcs.iter());
    let mut model = bhdl_synthesizer::safety_model::build_safety_model(&n, &srcs);

    // ── HSI ──
    let m = build_trace_matrix(&n, &analysis, &sf, &drc.violations, Some(&model), false);
    let row = |m: &bhdl_synthesizer::trace_matrix::TraceMatrix, id: &str| m.rows.iter().find(|r| r.id == id).cloned().unwrap_or_else(|| panic!("row {id}: {:?}", m.rows.iter().map(|r| &r.id).collect::<Vec<_>>()));
    let good = row(&m, "HSI_GOOD");
    assert_eq!(good.status, TraceStatus::Verified, "{good:#?}");
    assert!(good.evidence.contains("ok wiring") && good.evidence.contains("ok direction") && good.evidence.contains("ok level: source supply rail 5.00V"), "{good:#?}");
    // latency: the hardware share is DERIVED (driver's declared latency +
    // RC edge on the net); fw_latency is a declared contract term
    let a = row(&m, "HSI_FAULT_A");
    assert_eq!(a.status, TraceStatus::Verified, "{a:#?}");
    assert!(a.evidence.contains("ok latency: hw 0.025ms (mechanism rail_a_mon latency=25us"), "{a:#?}");
    assert!(a.evidence.contains("no fw_latency declared"), "{a:#?}");
    let t = row(&m, "HSI_TIMED");
    assert_eq!(t.status, TraceStatus::Verified, "{t:#?}");
    assert!(t.evidence.contains("+ fw 2ms (declared contract term, not measured) = 2.025ms ≤ 10ms"), "{t:#?}");
    let slow = row(&m, "HSI_SLOW");
    assert_eq!(slow.status, TraceStatus::Violated, "{slow:#?}");
    assert!(slow.evidence.contains("NOK latency") && slow.evidence.contains("≤ 1ms"), "{slow:#?}");

    assert!(a.implemented_by.iter().any(|i| i == "fw: fw/safety_monitor"), "{a:#?}");
    let bad = row(&m, "HSI_BAD");
    assert_eq!(bad.status, TraceStatus::Violated, "{bad:#?}");
    assert!(bad.evidence.contains("NOK wiring") && bad.evidence.contains("NOK level: source supply rail 5.00V vs declared 3.30V"), "{bad:#?}");

    // ── HSR evidence: fabricate campaign records on the resolved model (the
    //    campaign itself is `bhdl safety`; here the matrix's reading of its
    //    output is what is under test) ──
    let goal_path = model.scopes.iter().flat_map(|s| s.goals.iter().map(|g| g.path.clone())).find(|p| p.ends_with("rail_a.SG_OV")).unwrap_or_else(|| panic!("rail_a.SG_OV not in {:?} (errors {:?})", model.scopes.iter().map(|s| (s.path.clone(), s.goals.iter().map(|g| g.path.clone()).collect::<Vec<_>>())).collect::<Vec<_>>(), model.errors));
    model.universe.push(bhdl_common::safety::UniverseFault { scope: "rail_a".into(), part: "rail_a_r_fb_top".into(), mode: "open".into(), targets: vec![], ran: true, fired: vec!["overvoltage".into()], detected: vec!["rail_a_mon".into()], false_alarm: false, latent: false, latent_exposed_fit: 0.0, weight_fit: None, note: None });
    model.gaps.retain(|g| !g.goal.starts_with(&goal_path));
    model.gaps.push(bhdl_common::safety::Gap { class: bhdl_common::safety::GapClass::FaultUnrun, goal: format!("{goal_path}.overvoltage"), subject: "open(rail_a_r_fb_top)".into(), fix: "campaign ran: overvoltage fired and is detected, but the FTTI check FAILED".into() });
    for s in model.scopes.iter_mut() {
        for f in s.faults.iter_mut() {
            if f.kind == "open" && f.targets.iter().any(|t| t == "rail_a_r_fb_top") { f.run = true; f.expectation_met = Some(true); f.timing_met = Some(false); }
        }
    }
    let m = build_trace_matrix(&n, &analysis, &sf, &drc.violations, Some(&model), true);
    let ov = m.rows.iter().find(|r| r.kind == "safety goal" && r.stated_by.contains("rail_a.SG_OV")).expect("SG_OV row");
    assert_eq!(ov.status, TraceStatus::Violated, "ran + FTTI failed is a violation: {ov:#?}");
    assert!(ov.evidence.contains("FTTI check FAILED"), "{ov:#?}");
    let uv = m.rows.iter().find(|r| r.kind == "safety goal" && r.stated_by.contains("rail_a.SG_UV")).expect("SG_UV row");
    assert_eq!(uv.status, TraceStatus::Unverified, "{uv:#?}");
    assert!(uv.evidence.contains("FaultUnrun") || uv.evidence.contains("campaign evidence incomplete"), "{uv:#?}");
    assert!(m.findings.iter().any(|f| f.starts_with("safety gap (no goal row)")), "{:#?}", m.findings);
}

/// ERC032's fix text: every placeholder (Buck / LDO / BuckExt / Prereg)
/// is an unresolved REQUIREMENT — the fix names the interface to impl and
/// the `resolve` form; never "rename the Generic*".
#[tokio::test]
async fn erc032_fix_text_per_placeholder() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let src = r#"
import { GenericBuck, GenericPrereg } from "bhdl-stdlib/power/generic_regulators.bhdl";
import { Res } from "bhdl-stdlib/passives/resistor.bhdl";
board Fix {
    power VIN = 24V @ 3A;
    power V_PROT = 24V @ 3A;
    port V5: power out = 5V @ 2A;
    ground GND;
    @VIN -> pre: GenericPrereg(vin=24V, vout=24V, rated=3A).VIN; pre.VOUT -> @V_PROT; pre.GND -> @GND;
    attribute pre.powertree_rating_required_a = "3.750";
    @V_PROT -> u1: GenericBuck(vin=24V, vout=5V, rated=2.5A).VIN; u1.VOUT -> @V5; u1.GND -> @GND;
    attribute u1.stage_trait = "BuckStage"; attribute u1.stage_requirement = "vout=5V, i_max=2A, vin=24V";
    attribute u1.stage_bound = ""; attribute u1.stage_binding = "unresolved";
    attribute u1.powertree_rating_required_a = "2.500";
    @V5 -> r: Res(2.5Ω, wattage=10W).1; r.2 -> @GND;
}
"#;
    let pr = parse(src);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let v = bhdl_synthesizer::erc::check_powertree_acceptance(&n, &analysis);
    let pre = v.iter().find(|x| x.description.contains("'pre'")).expect("prereg placeholder finding");
    assert!(pre.fix_suggestion.contains("`impl`s PreregStage") && pre.fix_suggestion.contains("resolve pre = <Block>"), "{}", pre.fix_suggestion);
    assert!(!pre.fix_suggestion.contains("renam"), "{}", pre.fix_suggestion);
    let u1 = v.iter().find(|x| x.description.contains("'u1'")).expect("buck placeholder finding");
    assert!(u1.fix_suggestion.contains("`impl`s BuckStage") && u1.fix_suggestion.contains("resolve u1 = <Block>"), "{}", u1.fix_suggestion);
    assert!(!u1.fix_suggestion.contains("renam"), "a requirement is not committed by renaming: {}", u1.fix_suggestion);
}

/// PreregStage (the last tree stage to get an interface): the tree emits
/// `PreregStage(...)` for its protected front end; `PassiveFrontEnd`
/// (fuse + TVS, real parts) resolves it, promising the fuse rating, a
/// passive OV clamp and an input range BY CONSTRUCTION; protection
/// functions it does not provide (active cutoff, UV lockout, reverse
/// polarity) are UNCHECKED against it — a requirement stating one stays
/// unresolved with that said.
#[tokio::test]
async fn prereg_stage_interface_and_passive_front_end() {
    use bhdl_synthesizer::stage_resolution::resolve_stages;
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let stdlib = ws.join("bhdl-stdlib");
    let board = |args: &str| format!(r#"
import {{ PreregStage }} from "bhdl-stdlib/power/stages.bhdl";
import {{ Res }} from "bhdl-stdlib/passives/resistor.bhdl";
board PreDemo {{
    power VIN = 12V @ 3A;
    port V_PROT: power out = 12V @ 1A;
    ground GND;
    @VIN -> fe: PreregStage({args}).VIN;
    fe.GND -> @GND; fe.VOUT -> @V_PROT;
    @V_PROT -> R_LOAD: Res(6Ω, wattage=50W).1; R_LOAD.2 -> @GND;
}}
"#);
    let r = resolve_stages(&board("vout=12V, i_max=2A, vin=12V, ov_clamp=30V"), &stdlib, &[]).unwrap().unwrap();
    assert_eq!(r.resolutions[0].bound.as_deref(), Some("PassiveFrontEnd"), "{}", bhdl_synthesizer::stage_resolution::render_report(&r.resolutions[0]));
    let c = &r.resolutions[0].candidates[0];
    assert!(c.gates.iter().any(|g| g.0 == "ov_clamp" && g.2 && g.1.contains("24.0V")), "{c:#?}");
    // the bound block synthesizes: fuse + TVS on the protected line
    let pr = parse(&r.source);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    for child in ["fe_F", "fe_D_tvs"] {
        assert!(n.instances.values().any(|i| i.name == child), "{child}");
    }
    // too much load for the default 3A fuse: the where-envelope refuses (override with i_rating)
    let r = resolve_stages(&board("vout=12V, i_max=2.6A, vin=12V"), &stdlib, &[]).unwrap().unwrap();
    assert!(r.resolutions[0].bound.is_none());
    assert!(r.resolutions[0].candidates[0].failures().iter().any(|f| f.contains("envelope") && f.contains("i_load <= 0.8 * i_rating")), "{:?}", r.resolutions[0].candidates[0].failures());
    // protection the passive block does not provide: UNCHECKED against it,
    // unresolved (the eFuse is a TEMPLATE — listed, never auto-bound)
    for extra in ["reverse_polarity=\"true\"", "ov_trip=30V", "uv_trip=8V"] {
        let r = resolve_stages(&board(&format!("vout=12V, i_max=1A, vin=12V, {extra}")), &stdlib, &[]).unwrap().unwrap();
        assert!(r.resolutions[0].bound.is_none(), "{extra}");
        let gate = extra.split('=').next().unwrap();
        let passive = r.resolutions[0].candidates.iter().find(|c| c.block == "PassiveFrontEnd").unwrap();
        assert!(passive.unchecked.contains(&gate.to_string()), "{extra}: {passive:#?}");
        let efuse = r.resolutions[0].candidates.iter().find(|c| c.block == "Efuse_TPS2660").unwrap();
        assert!(efuse.template && efuse.gates.iter().any(|g| g.0 == gate && g.2), "{extra}: {efuse:#?}");
        assert!(r.source.contains("fe: GenericPrereg("), "{}", r.source);
    }
    // the eFuse TEMPLATE commits by override carrying r_ilim (the datasheet
    // axis this library lacks); the requirement's trip points size the
    // OVP / UVLO dividers against the 1.2 V threshold; the stamped
    // requirement text carries no nested quotes
    let src = board("vout=12V, i_max=1A, vin=12V, ov_trip=30V, uv_trip=8V, reverse_polarity=\"true\"")
        .replacen("fe.GND -> @GND;", "fe.GND -> @GND; resolve fe = Efuse_TPS2660(r_ilim=16.5kΩ);", 1);
    let r = resolve_stages(&src, &stdlib, &[]).unwrap().unwrap();
    assert_eq!(r.resolutions[0].bound.as_deref(), Some("Efuse_TPS2660"));
    assert_eq!(r.resolutions[0].basis, "override");
    assert!(r.source.contains("reverse_polarity=true\""), "{}", r.source);
    let pr = parse(&r.source);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let val = |name: &str| n.instances.values().find(|i| i.name == name).unwrap_or_else(|| panic!("{name}")).attributes.get("value").cloned().unwrap_or_default();
    let ohms = |v: String| bhdl_synthesizer::stage_acceptance::parse_si(&v).unwrap();
    assert!((ohms(val("fe_R_ov_top")) - 240e3).abs() / 240e3 < 0.03, "OVP top {}", val("fe_R_ov_top"));
    assert!((ohms(val("fe_R_uv_top")) - 56.67e3).abs() / 56.67e3 < 0.03, "UVLO top {}", val("fe_R_uv_top"));
    assert!((ohms(val("fe_R_ilim")) - 16.5e3).abs() < 1.0, "{}", val("fe_R_ilim"));
    // ERC032 on the committed stage: same predicate, all declared → clean
    let v = bhdl_synthesizer::erc::check_powertree_acceptance(&n, &analysis);
    assert!(!v.iter().any(|x| x.description.contains("'fe'") && x.severity != bhdl_synthesizer::design_rule_checker::ViolationSeverity::Info), "{v:#?}");
    assert!(!v.iter().any(|x| x.description.contains("'fe'") && x.description.contains("UNCHECKED")), "{v:#?}");

    // the ideal-diode controller TEMPLATE: reverse polarity + AEC-Q100
    // promised by the controller; the current capability is the FET the
    // override supplies (BuckController idiom); no ov/uv trip
    let src = board("vout=12V, i_max=8A, vin=12V, reverse_polarity=\"true\", qual=\"AEC-Q100\"")
        .replacen("fe.GND -> @GND;", "fe.GND -> @GND; resolve fe = IdealDiode_LM74700(fet=\"BSC0902NS\", fet_vds_max=30V, fet_id_max=30A, fet_rds_on=2.3mΩ, fet_vgs_th=2.4V, fet_p_rating=42W);", 1);
    let r = resolve_stages(&src, &stdlib, &[]).unwrap().unwrap();
    assert_eq!(r.resolutions[0].bound.as_deref(), Some("IdealDiode_LM74700"), "{}", bhdl_synthesizer::stage_resolution::render_report(&r.resolutions[0]));
    let c = r.resolutions[0].candidates.iter().find(|c| c.block == "IdealDiode_LM74700").unwrap();
    assert!(c.template && c.gates.iter().any(|g| g.0 == "i_max" && g.2 && g.1.contains("30.000A")), "{c:#?}");
    assert!(c.gates.iter().any(|g| g.0 == "qual" && g.2), "{c:#?}");
    let pr = parse(&r.source);
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let fet = n.instances.values().find(|i| i.name == "fe_M").expect("pass FET");
    assert_eq!(fet.attributes.get("part_number").map(String::as_str), Some("BSC0902NS"), "{:#?}", fet.attributes);
    // a requirement asking for an active cutoff: UNCHECKED against the ideal diode
    let r = resolve_stages(&board("vout=12V, i_max=1A, vin=12V, ov_trip=30V"), &stdlib, &[]).unwrap().unwrap();
    let c = r.resolutions[0].candidates.iter().find(|c| c.block == "IdealDiode_LM74700").unwrap();
    assert!(c.unchecked.contains(&"ov_trip".to_string()), "{c:#?}");
}

/// Project-wide filters, ASIL capability, and the thermal path
/// (Requirements_And_Resolution §3): a board-level `requirements { … }`
/// merges into every stage requirement (stage's own key wins); `asil`
/// gates against `asil_capable`; an ambient temp_max requirement is met
/// THERMALLY by a junction-rated block declaring theta_ja + tj_max at the
/// requirement's dissipation, and refused when the junction would exceed
/// its rating; the trace matrix derives the ASIL a stage must meet from
/// the safety goal on the rail it drives and flags an unstated one.
#[tokio::test]
async fn project_requirements_asil_and_thermal_path() {
    use bhdl_synthesizer::stage_resolution::resolve_stages;
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let stdlib = ws.join("bhdl-stdlib");
    // a junction-rated FIXTURE block with θJA (one attribute per statement, several per line)
    let lib = r#"
import { Cap } from "bhdl-stdlib/passives/capacitor.bhdl";
entity HotLdoIc(v_out: voltage = 3.3V) as part {
    pin VIN: power in; pin VOUT: power out; pin GND: ground;
    attribute component_class = "regulator_ic"; attribute part_number = "FIXTURE-LDO";
    attribute output_current = 0.5A; attribute vin_min = 2.5V; attribute vin_max = 16V;
    attribute i_quiescent = 1mA; attribute tj_max = 125degC; attribute theta_ja = 60degC/W;
    attribute asil_capable = "B";
}
entity Ldo_Hot(v_out: voltage = 3.3V, i_out_max: current = 100mA, v_in: voltage = 5V) as design {
    pin VIN: power in; pin VOUT: power out virtual; pin GND: ground;
    attribute component_class = "ldo"; attribute output_voltage = v_out; attribute v_in = v_in; attribute i_out_max = i_out_max;
    attribute output_current = 0.5A; attribute vin_min = 2.5V; attribute vin_max = 16V; attribute i_quiescent = 1mA;
    attribute tj_max = 125degC; attribute theta_ja = 60degC/W; attribute asil_capable = "B";
    design { const need = self.v_out + 0.3; require self.v_in >= need else "dropout"; }
    u: HotLdoIc(v_out); VIN -> u.VIN; u.GND -> GND; u.VOUT -> VOUT;
    VOUT -> C_out: Cap(1uF).1; C_out.2 -> GND;
}
impl LdoStage for Ldo_Hot { const vout = v_out; const i_max = i_out_max; const vin = v_in; }
"#;
    let board = |project: &str, args: &str, extra: &str| format!(r#"
import {{ LdoStage }} from "bhdl-stdlib/power/stages.bhdl";
import {{ Res }} from "bhdl-stdlib/passives/resistor.bhdl";
{lib}
board Proj {{
    {project}
    power V12 = 12V @ 1A;
    // budget matches the largest requirement the test states — the
    // understatement check (ERC032) fires when i_max < the rail budget
    port V3V3: power out = 3.3V @ 100mA;
    ground GND;
    @V12 -> u2: LdoStage({args}).VIN;
    u2.GND -> @GND; u2.VOUT -> @V3V3;
    {extra}
    @V3V3 -> R_LOAD: Res(8.25Ω, wattage=2W).1; R_LOAD.2 -> @GND;
}}
"#);
    // 1. project-wide filters land on the stage; the stage's own key wins
    let r = resolve_stages(&board("requirements { asil: B; temp_max: 85degC; qual: \"AEC-Q100\"; }", "vout=3.3V, i_max=100mA, vin=5V, temp_max=70degC", ""), &stdlib, &[]).unwrap().unwrap();
    assert_eq!(r.resolutions[0].requirement, "vout=3.3V, i_max=100mA, vin=5V, temp_max=70degC, asil=B, qual=AEC-Q100", "{}", r.resolutions[0].requirement);
    assert!(r.resolutions[0].bound.is_none(), "no stdlib LDO is AEC-Q100: {}", bhdl_synthesizer::stage_resolution::render_report(&r.resolutions[0]));
    let hot = r.resolutions[0].candidates.iter().find(|c| c.block == "Ldo_Hot").unwrap();
    assert!(hot.gates.iter().any(|g| g.0 == "asil" && g.2), "{hot:#?}");
    assert!(hot.unchecked.contains(&"qual".to_string()), "{hot:#?}");
    // 2. thermal path: 5V/100mA → T_J 95.5°C ≤ 125 passes; 12V/400mA → 294°C refused
    let r = resolve_stages(&board("requirements { asil: B; temp_max: 85degC; }", "vout=3.3V, i_max=100mA, vin=5V", ""), &stdlib, &[]).unwrap().unwrap();
    assert_eq!(r.resolutions[0].bound.as_deref(), Some("Ldo_Hot"), "{}", bhdl_synthesizer::stage_resolution::render_report(&r.resolutions[0]));
    let hot = r.resolutions[0].candidates.iter().find(|c| c.block == "Ldo_Hot").unwrap();
    let tg = hot.gates.iter().find(|g| g.0 == "temp_max").unwrap();
    assert!(tg.2 && tg.1.contains("thermal: T_J = 85°C + 0.175W × 60.0°C/W = 95.5°C"), "{tg:?}");
    let e = resolve_stages(&board("requirements { temp_max: 85degC; }", "vout=3.3V, i_max=400mA, vin=12V", "resolve u2 = Ldo_Hot;"), &stdlib, &[]).unwrap_err();
    assert!(format!("{e:#}").contains("294.5°C ≤ T_J,max 125°C"), "{e:#}");
    // 3. ASIL capability ordering: a C requirement fails a B-capable part
    let r = resolve_stages(&board("requirements { asil: C; }", "vout=3.3V, i_max=100mA, vin=5V", ""), &stdlib, &[]).unwrap().unwrap();
    let hot = r.resolutions[0].candidates.iter().find(|c| c.block == "Ldo_Hot").unwrap();
    assert!(hot.gates.iter().any(|g| g.0 == "asil" && !g.2 && g.1.contains("asil_capable B ≥ required ASIL C")), "{hot:#?}");
    // 4. trace matrix derives the ASIL from the goal on the driven rail
    let src = board("", "vout=3.3V, i_max=100mA, vin=5V", "").to_string() + r#"
safety Proj as brd {
    goal SG_RAIL: ASIL_B "V3V3 must not overvolt undetected" (id="SG-RAIL-1", ftti=10ms) {
        effect ov = brd.V3V3 > 3.6V   severity S2;
    }
}
"#;
    let r = resolve_stages(&src, &stdlib, &[]).unwrap().unwrap();
    let pr = parse(&r.source);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let mut checker = bhdl_synthesizer::design_rule_checker::DesignRuleChecker::new(bhdl_synthesizer::design_rule_checker::IndustryStandard::IPC2221);
    let drc = checker.run_checks(&n, &analysis);
    let safety = bhdl_synthesizer::safety_model::build_safety_model(&n, &[&sf]);
    let m = bhdl_synthesizer::trace_matrix::build_trace_matrix(&n, &analysis, &sf, &drc.violations, Some(&safety), false);
    let row = m.rows.iter().find(|r| r.id == "Proj.u2").expect("stage row");
    assert_eq!(row.status, bhdl_synthesizer::trace_matrix::TraceStatus::Unverified, "{row:#?}");
    assert!(row.evidence.contains("derived ASIL B (serves goal SG_RAIL)") && row.evidence.contains("requirement states NONE — add `asil=B`"), "{row:#?}");
}

/// Junction-temperature sign-off row + the as-built-load check: the
/// sign-off takes the part's stress-model dissipation through its
/// datasheet θ_JA against T_J,max at the stage requirement's ambient
/// (rise budget), and ERC032 flags a requirement whose i_max understates
/// the rail budget the board actually declares (the block was resolved
/// for the smaller load).
#[tokio::test]
async fn junction_temperature_row_and_understated_load() {
    use bhdl_synthesizer::stage_resolution::resolve_stages;
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let stdlib = ws.join("bhdl-stdlib");
    let board = |budget: &str, i_max: &str| format!(r#"
import {{ LdoStage }} from "bhdl-stdlib/power/stages.bhdl";
import {{ Res }} from "bhdl-stdlib/passives/resistor.bhdl";
board TjDemo {{
    power V5 = 5V @ 1A;
    port V3V3: power out = 3.3V @ {budget};
    ground GND;
    @V5 -> u2: LdoStage(vout=3.3V, i_max={i_max}, vin=5V, temp_max=85degC).VIN;
    u2.GND -> @GND; u2.VOUT -> @V3V3;
    resolve u2 = Ldo_LP2985;
    @V3V3 -> R_LOAD: Res(33Ω, wattage=1W).1; R_LOAD.2 -> @GND;
}}
"#);
    async fn build(src: String, stdlib: &std::path::Path) -> (bhdl_netlist::Netlist, bhdl_analyzer::AnalysisResult) {
        use bhdl_synthesizer::stage_resolution::resolve_stages;
        let r = resolve_stages(&src, stdlib, &[]).unwrap().unwrap();
        let pr = parse(&r.source);
        assert!(pr.errors().is_empty(), "{:?}", pr.errors());
        let sf = SourceFile::cast(pr.syntax()).unwrap();
        let analysis = analyze(&sf);
        let mut gen = NetlistGenerator::new();
        let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
        (n, analysis)
    }
    // requirement understates the rail (100mA vs 200mA budget) → ERC032 Error
    let (n, analysis) = build(board("200mA", "100mA"), &stdlib).await;
    let v = bhdl_synthesizer::erc::check_powertree_acceptance(&n, &analysis);
    let under = v.iter().find(|x| x.description.contains("understates the rail")).expect("understatement finding: {v:#?}");
    assert_eq!(under.severity, bhdl_synthesizer::design_rule_checker::ViolationSeverity::Error);
    assert!(under.description.contains("i_max=0.100A") && under.description.contains("0.200A"), "{}", under.description);
    // consistent requirement (a LEGAL point — LP2985's envelope refuses
    // 200mA outright) → no such finding; the sign-off's T_J row takes the
    // stress-model dissipation at the rail's 100mA through θ_JA 205.4
    let (n, analysis) = build(board("100mA", "100mA"), &stdlib).await;
    let v = bhdl_synthesizer::erc::check_powertree_acceptance(&n, &analysis);
    assert!(!v.iter().any(|x| x.description.contains("understates")), "{v:#?}");
    let rows = bhdl_synthesizer::signoff::compute_signoff(
        &n,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &analysis.entity_attribute_index,
        &analysis.stress_recipes,
    );
    let tj = rows.iter().find(|r| r.refdes == "u2_u" && r.class == "junction temperature").unwrap_or_else(|| panic!("T_J row: {:?}", rows.iter().map(|r| (&r.refdes, &r.class)).collect::<Vec<_>>()));
    let note = tj.ripple.clone().unwrap_or_default();
    assert!(note.contains("T_A=85°C (requirement temp_max)") && note.contains("× 205.4°C/W"), "{note}");
    // 0.174 W × 205.4 = 35.7 °C rise vs the 40 °C budget (125 − 85):
    // inside the rating but under the 1.2× sign-off margin — UnderMargin,
    // exactly what a JEDEC-board θ_JA at 85 °C ambient should say
    assert!(matches!(tj.verdict, bhdl_synthesizer::signoff::Verdict::UnderMargin), "{tj:?}");
    assert!((tj.rating.unwrap() - 40.0).abs() < 1e-6 && (tj.stress.unwrap() - 35.7).abs() < 0.5, "{tj:?}");
}

/// The FET loss model on external-stage topologies: BuckController's
/// stress assigns each FET its own dissipation (HS conduction·D +
/// switching; LS conduction·(1−D)) so the sign-off P rows land on the
/// FET children against THEIR power_rating; the design envelope refuses
/// a FET whose rating the computed dissipation exceeds; the ideal-diode
/// block declares its pass resistance (D = 1) so the estimator computes
/// the stage's heat as I²·R_ds.
#[tokio::test]
async fn fet_loss_model_for_controller_and_ideal_diode() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let src = std::fs::read_to_string(ws.join("tests/circuits/realistic/buck_controller_ext_fets.bhdl")).unwrap();
    let pr = parse(&src);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let rows = bhdl_synthesizer::signoff::compute_signoff(
        &n,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &analysis.entity_attribute_index,
        &analysis.stress_recipes,
    );
    // 24 V → 5 V at 5 A (D = 0.208): HS = 25·2.3mΩ·D + 24·5·300k·30n
    // = 0.012 + 1.08 ≈ 1.092 W vs 42 W; LS = 25·2.3mΩ·(1−D) ≈ 0.046 W
    let hs = rows.iter().find(|r| r.refdes == "u1_M_hs" && r.axis == "P").unwrap_or_else(|| panic!("HS P row: {:?}", rows.iter().map(|r| (&r.refdes, r.axis)).collect::<Vec<_>>()));
    assert!((hs.stress.unwrap() - 1.092).abs() < 0.02, "{hs:?}");
    assert!((hs.rating.unwrap() - 42.0).abs() < 1e-6 && matches!(hs.verdict, bhdl_synthesizer::signoff::Verdict::SignedOff), "{hs:?}");
    let ls = rows.iter().find(|r| r.refdes == "u1_M_ls" && r.axis == "P").expect("LS P row");
    assert!((ls.stress.unwrap() - 25.0 * 0.0023 * (1.0 - 5.0 / 24.0)).abs() < 0.005, "{ls:?}");

    // envelope refusal: same stage with a 0.5 W FET rating — the HS
    // dissipation (~1.11 W) exceeds it, the block refuses to build
    let shrunk = src.replace("fet_p_rating=42W", "fet_p_rating=0.5W");
    let pr = parse(&shrunk);
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let e = gen.generate_from_ast_and_analysis(&sf, &analysis).await.err().expect("undersized FET must refuse");
    assert!(format!("{e:#}").contains("high-side FET dissipation") && format!("{e:#}").contains("exceeds fet_p_rating"), "{e:#}");

    // ideal diode: the estimator computes the stage heat as I²·R_ds from
    // the block's declared pass resistance (protection arm, D = 1)
    let p = bhdl_synthesizer::stage_acceptance::estimate_dissipation_w(
        "protection", Some(12.0), Some(12.0), Some(8.0), None, None, Some(0.0023), None, None,
    );
    assert!((p.unwrap() - 64.0 * 0.0023).abs() < 1e-9);

    // ── PASS-THROUGH op recovery: an eFuse stage (V_out == V_in) now gets
    // an operating point from the rail it touches (i_out = the rail's
    // `@ I` budget), so its die stress row (internal FET I²·R_ON +
    // quiescent) and the ideal-diode FET / controller rows are MEASURED.
    let src = r#"
import {{ PreregStage }} from "bhdl-stdlib/power/stages.bhdl";
import {{ Res }} from "bhdl-stdlib/passives/resistor.bhdl";
board PassThrough {{
    power VIN = 12V @ 1A;
    port V_PROT: power out = 12V @ 1A;
    ground GND;
    @VIN -> fe: PreregStage(vout=12V, i_max=1A, vin=12V, ov_trip=30V, uv_trip=8V).VIN;
    fe.GND -> @GND; fe.VOUT -> @V_PROT;
    resolve fe = Efuse_TPS2660(r_ilim=16.5kΩ);
    @V_PROT -> R_LOAD: Res(12Ω, wattage=20W).1; R_LOAD.2 -> @GND;
}}
"#.replace("{{", "{").replace("}}", "}");
    let ws2 = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let r = bhdl_synthesizer::stage_resolution::resolve_stages(&src, ws2.join("bhdl-stdlib").as_path(), &[]).unwrap().unwrap();
    let pr = parse(&r.source);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let rows = bhdl_synthesizer::signoff::compute_signoff(
        &n,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &analysis.entity_attribute_index,
        &analysis.stress_recipes,
    );
    // 1 A through 150 mΩ + 12 V × 300 µA = 0.150 + 0.0036 ≈ 0.154 W
    let die = rows.iter().find(|r| r.refdes == "fe_u" && r.axis == "P").unwrap_or_else(|| panic!("eFuse die P row: {:?}", rows.iter().map(|r| (&r.refdes, r.axis)).collect::<Vec<_>>()));
    assert!((die.stress.unwrap() - 0.1536).abs() < 0.002, "{die:?}");
    // and its junction-temperature row through the datasheet θ_JA 38.6
    let tj = rows.iter().find(|r| r.refdes == "fe_u" && r.class == "junction temperature").expect("eFuse T_J row");
    assert!(tj.ripple.clone().unwrap_or_default().contains("× 38.6°C/W"), "{tj:?}");
}

/// STEP-UP: a rail above its feed gets a Boost stage (it used to fall
/// silently into the buck path with duty > 1) — the switch carries the
/// INPUT current I_out·V_out/V_in and the rating is derated against it;
/// the tree emits a `BoostStage(...)` requirement (no implementer yet —
/// honest placeholder); the estimator uses the boost loss form
/// (conduction at D = 1 − Vin/Vout on the input current, transitions
/// swing Vout); BuckBoostStage exists for straddling-feed requirements.
#[tokio::test]
async fn boost_topology_and_stage_interface() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    use bhdl_synthesizer::powertree::{emit_power_region, harvest_loads, propose_trees, splice_power_region, Topology};
    let src = r#"
entity Loady() {
    pin 1: power in;
    pin 2: ground;
    attribute component_class = "ic";
    domain VDD pins="1" v=12V tol=5% i_nom=0.2A i_max=0.3A
        source="FIXTURE — boost probe";
}
board BoostProbe {
    power VBAT = 5V @ 3A;
    power V12 = 12V;
    ground GND;
    @V12 -> ldy: Loady().1;
    ldy.2 -> @GND;
}
"#.to_string();
    let pr = parse(&src);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.unwrap();
    let h = harvest_loads(&n, &sf);
    let opts = propose_trees(&h, "VBAT").expect("options");
    let st = opts[0].stages.iter().find(|s| s.to == "V12").expect("V12 stage");
    assert_eq!(st.topology, Topology::Boost, "{st:#?}");
    // rating = the INPUT current 0.3·12/5 = 0.72 A, derated /0.8 = 0.9 A
    assert!((st.required_rating_a - 0.9).abs() < 1e-9, "{st:#?}");
    assert!(st.eff_basis.contains("INPUT current 0.72A"), "{st:#?}");
    // emits a BoostStage requirement; resolves to the honest placeholder
    let region = emit_power_region(&opts[0], "GND");
    assert!(region.contains("BoostStage(vout=12V, i_max=0.3A, vin=5V)"), "{region}");
    let emitted = splice_power_region(&src, &region).unwrap();
    let r = bhdl_synthesizer::stage_resolution::resolve_stages(&emitted, ws.join("bhdl-stdlib").as_path(), &[]).unwrap().unwrap();
    let u = r.resolutions.iter().find(|x| x.instance == "u_v12").unwrap();
    // Boost_TPS61022 is surveyed but its envelope refuses 12 V out
    // (SLVSDX7D output setting range tops at 5.5 V) — stated near-miss
    assert!(u.bound.is_none(), "{u:#?}");
    let c = u.candidates.iter().find(|c| c.block == "Boost_TPS61022").expect("TPS61022 surveyed");
    assert!(c.failures().iter().any(|f| f.contains("envelope") && f.contains("v_out <= 5.5V")), "{:?}", c.failures());
    assert!(r.source.contains("u_v12: GenericBoost("), "{}", r.source);
    // estimator: boost loss form — 0.3 A out, 5→12 V, 100 mΩ, 1 MHz, 20 ns:
    // I_in = 0.72, D = 0.583 → 0.030 + 12·0.72·0.02 = 0.203 W
    let p = bhdl_synthesizer::stage_acceptance::estimate_dissipation_w(
        "switching_regulator", Some(5.0), Some(12.0), Some(0.3), None, None, Some(0.1), Some(1.0e6), Some(20e-9),
    ).unwrap();
    assert!((p - (0.72_f64 * 0.72 * 0.1 * (1.0 - 5.0 / 12.0) + 12.0 * 0.72 * 1.0e6 * 20e-9)).abs() < 1e-9, "{p}");
}

/// Boost_TPS61022 (SLVSDX7D): the first BoostStage implementer.
/// Resolves a 3.6 V → 5 V / 2 A requirement (survey-priced); the FB
/// divider and C_out come from the datasheet equations; the VALLEY
/// current envelope — the ratio-aware capability arithmetic — refuses
/// the same output power from a 1.9 V input.
#[tokio::test]
async fn boost_tps61022_block() {
    use bhdl_synthesizer::stage_resolution::resolve_stages;
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let stdlib = ws.join("bhdl-stdlib");
    let board = |vin: &str| format!(r#"
import {{ BoostStage }} from "bhdl-stdlib/power/stages.bhdl";
import {{ Res }} from "bhdl-stdlib/passives/resistor.bhdl";
board BoostReq {{
    power VBAT = {vin} @ 4A;
    port V5: power out = 5V @ 2A;
    ground GND;
    @VBAT -> u1: BoostStage(vout=5V, i_max=2A, vin={vin}).VIN;
    u1.GND -> @GND; u1.VOUT -> @V5;
    @V5 -> R_LOAD: Res(2.5Ω, wattage=20W).1; R_LOAD.2 -> @GND;
}}
"#);
    let r = resolve_stages(&board("3.6V"), &stdlib, &[]).unwrap().unwrap();
    assert_eq!(r.resolutions[0].bound.as_deref(), Some("Boost_TPS61022"), "{}", bhdl_synthesizer::stage_resolution::render_report(&r.resolutions[0]));
    let pr = parse(&r.source);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let val = |name: &str| n.instances.values().find(|i| i.name == name).unwrap_or_else(|| panic!("{name}")).attributes.get("value").cloned().unwrap_or_default();
    let si = |v: String| bhdl_synthesizer::stage_acceptance::parse_si(&v).unwrap();
    // FB divider Eq. 4: (5/0.6 − 1)·100k = 733.3 kΩ (E96-snapped)
    assert!((si(val("u1_R_top")) - 733.3e3).abs() / 733.3e3 < 0.02, "{}", val("u1_R_top"));
    // C_out Eq. 8: 2·(1 − 3.6/5)/(1 MHz·50 mV) = 11.2 µF (snapped up)
    assert!(si(val("u1_C_out")) >= 11.0e-6 && si(val("u1_C_out")) <= 16e-6, "{}", val("u1_C_out"));
    assert!(n.instances.values().any(|i| i.name == "u1_u"), "the silicon materialised");
    // deep ratio: 1.9 V → 5 V at 2 A puts the switch valley beyond
    // 5.2 A (6.5 A min limit × 0.8) — refused with the arithmetic named
    let r = resolve_stages(&board("1.9V"), &stdlib, &[]).unwrap().unwrap();
    assert!(r.resolutions[0].bound.is_none());
    let c = r.resolutions[0].candidates.iter().find(|c| c.block == "Boost_TPS61022").expect("surveyed");
    assert!(c.failures().iter().any(|f| f.contains("VALLEY current") && f.contains("5.2A")), "{:?}", c.failures());
}

/// BuckBoost_TPS63020 (SLVS916I): the first BuckBoostStage implementer.
/// The requirement's STRADDLE (vin_min/vin_max) is conveyed into the
/// ctor via the impl bindings, so the envelope — the datasheet's Eq. 1/2
/// average-switch-current arithmetic at v_in_min — runs at the
/// requirement's own worst boost ratio. A Li-ion straddle across 3.3 V
/// resolves; a 1.8 V floor boosting to 5 V at 2 A is refused with the
/// arithmetic named.
#[tokio::test]
async fn buckboost_tps63020_block() {
    use bhdl_synthesizer::stage_resolution::resolve_stages;
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let stdlib = ws.join("bhdl-stdlib");
    let board = |vout: &str, imax: &str, vmin: &str| format!(r#"
import {{ BuckBoostStage }} from "bhdl-stdlib/power/stages.bhdl";
import {{ Res }} from "bhdl-stdlib/passives/resistor.bhdl";
board BuckBoostReq {{
    power VBAT = 3.6V @ 3A;
    port VRAIL: power out = {vout} @ {imax};
    ground GND;
    @VBAT -> u1: BuckBoostStage(vout={vout}, i_max={imax}, vin=3.6V, vin_min={vmin}, vin_max=4.2V).VIN;
    u1.GND -> @GND; u1.VOUT -> @VRAIL;
    @VRAIL -> R_LOAD: Res(2.2Ω, wattage=10W).1; R_LOAD.2 -> @GND;
}}
"#);
    // Li-ion straddle across 3.3 V: 2.5–4.2 V in, 1.5 A out — resolves
    let r = resolve_stages(&board("3.3V", "1.5A", "2.5V"), &stdlib, &[]).unwrap().unwrap();
    assert_eq!(r.resolutions[0].bound.as_deref(), Some("BuckBoost_TPS63020"), "{}", bhdl_synthesizer::stage_resolution::render_report(&r.resolutions[0]));
    let pr = parse(&r.source);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let val = |name: &str| n.instances.values().find(|i| i.name == name).unwrap_or_else(|| panic!("{name}")).attributes.get("value").cloned().unwrap_or_default();
    let si = |v: String| bhdl_synthesizer::stage_acceptance::parse_si(&v).unwrap();
    // FB divider Eq. 3: 180k·(3.3/0.5 − 1) = 1.008 MΩ — the datasheet's
    // own Table 3 row (3.3 V → 1 MΩ / 180 kΩ), E-series-snapped
    assert!((si(val("u1_R_top")) - 1.008e6).abs() / 1.008e6 < 0.02, "{}", val("u1_R_top"));
    // inductor between the two bridge legs; VINA bypass present
    assert!(n.instances.values().any(|i| i.name == "u1_L_bb"), "single inductor");
    assert!(n.instances.values().any(|i| i.name == "u1_C_a"), "VINA bypass");
    assert!(n.instances.values().any(|i| i.name == "u1_R_pg"), "PG pull-up (Figure 7)");
    // deep straddle: 1.8 V floor boosting to 5 V at 2 A — average switch
    // current 2/(0.9·0.36) = 6.2 A, beyond 2.8 A (3.5 A min × 0.8)
    let r = resolve_stages(&board("5V", "2A", "1.8V"), &stdlib, &[]).unwrap().unwrap();
    assert!(r.resolutions[0].bound.is_none());
    let c = r.resolutions[0].candidates.iter().find(|c| c.block == "BuckBoost_TPS63020").expect("surveyed");
    assert!(c.failures().iter().any(|f| f.contains("AVERAGE switch current") && f.contains("2.8A")), "{:?}", c.failures());
    assert!(r.source.contains("GenericBuckBoost("), "{}", r.source);
}

/// Boost/buck-boost CLASS templates (regulator.bhdl): capability axes
/// are REQUIRED designer arguments (the tps2660 r_ilim doctrine — no
/// invented class defaults), the envelope checks the switch PEAK
/// current (conservative under every vendor limit convention), and the
/// override args must actually REACH the envelope — this test pins the
/// self_namespace fix (conveyed/override values for required,
/// default-less params were silently dropped, so the envelope
/// evaluated against nothing).
#[tokio::test]
async fn boost_buckboost_class_templates() {
    use bhdl_synthesizer::stage_resolution::resolve_stages;
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let stdlib = ws.join("bhdl-stdlib");
    let board = |resolve_stmt: &str| format!(r#"
import {{ BoostStage }} from "bhdl-stdlib/power/stages.bhdl";
import {{ Res }} from "bhdl-stdlib/passives/resistor.bhdl";
board BoostTmpl {{
    power VBAT = 5V @ 4A;
    port V12: power out = 12V @ 0.5A;
    ground GND;
    @VBAT -> u1: BoostStage(vout=12V, i_max=0.5A, vin=5V).VIN;
    u1.GND -> @GND; u1.VOUT -> @V12;
    @V12 -> R_LOAD: Res(24Ω, wattage=10W).1; R_LOAD.2 -> @GND;
    {resolve_stmt}
}}
"#);
    // no resolve: the template is LISTED but never auto-bound
    let r = resolve_stages(&board(""), &stdlib, &[]).unwrap().unwrap();
    assert!(r.resolutions[0].bound.is_none());
    let t = r.resolutions[0].candidates.iter().find(|c| c.block == "BoostRegulator").expect("template surveyed");
    assert!(t.template, "listed as template");
    // committed with the designer's class numbers: the required
    // (default-less) args reach the envelope and the divider math
    let r = resolve_stages(&board("resolve u1 = BoostRegulator(i_sw_limit=4A, f_sw=1.2MHz, rds_on=80mΩ, i_quiescent=100µA, vref=1.245V);"), &stdlib, &[]).unwrap().unwrap();
    assert_eq!(r.resolutions[0].bound.as_deref(), Some("BoostRegulator"), "{}", bhdl_synthesizer::stage_resolution::render_report(&r.resolutions[0]));
    let pr = parse(&r.source);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let val = |name: &str| n.instances.values().find(|i| i.name == name).unwrap_or_else(|| panic!("{name}")).attributes.get("value").cloned().unwrap_or_default();
    let si = |v: String| bhdl_synthesizer::stage_acceptance::parse_si(&v).unwrap();
    // divider from the OVERRIDE vref: (12/1.245 − 1)·100k = 863.9 kΩ
    assert!((si(val("u1_R_top")) - 863.9e3).abs() / 863.9e3 < 0.02, "{}", val("u1_R_top"));
    assert!(n.instances.values().any(|i| i.name == "u1_L"), "boost inductor");
    // under-sized class numbers: the PEAK envelope refuses, arithmetic
    // named — a hard error at resolve time (peak 1.59 A > 1 A × 0.8)
    let e = resolve_stages(&board("resolve u1 = BoostRegulator(i_sw_limit=1A, f_sw=1.2MHz, rds_on=80mΩ, i_quiescent=100µA, vref=1.245V);"), &stdlib, &[]).unwrap_err();
    assert!(e.to_string().contains("PEAK current") && e.to_string().contains("i_sw_limit"), "{e}");

    // buck-boost template on the straddle BuckBoost_TPS63020 refuses
    // (1.8 V floor boosting to 5 V at 2 A): committed with a beefier
    // class part — peak 6.34 A ≤ 10 A × 0.8
    let bb = r#"
import { BuckBoostStage } from "bhdl-stdlib/power/stages.bhdl";
import { Res } from "bhdl-stdlib/passives/resistor.bhdl";
board BuckBoostTmpl {
    power VBAT = 3.6V @ 8A;
    port V50: power out = 5V @ 2A;
    ground GND;
    @VBAT -> u1: BuckBoostStage(vout=5V, i_max=2A, vin=3.6V, vin_min=1.8V, vin_max=4.2V).VIN;
    u1.GND -> @GND; u1.VOUT -> @V50;
    @V50 -> R_LOAD: Res(2.5Ω, wattage=20W).1; R_LOAD.2 -> @GND;
    resolve u1 = BuckBoostRegulator(i_sw_limit=10A, f_sw=1.5MHz, rds_on=30mΩ, rds_on_ls=30mΩ, i_quiescent=50µA, vref=0.8V);
}
"#;
    let r = resolve_stages(bb, &stdlib, &[]).unwrap().unwrap();
    assert_eq!(r.resolutions[0].bound.as_deref(), Some("BuckBoostRegulator"), "{}", bhdl_synthesizer::stage_resolution::render_report(&r.resolutions[0]));
    // the vendor block's own refusal stays visible in the survey
    let v = r.resolutions[0].candidates.iter().find(|c| c.block == "BuckBoost_TPS63020").expect("vendor surveyed");
    assert!(v.failures().iter().any(|f| f.contains("AVERAGE switch current")), "{:?}", v.failures());
    let pr = parse(&r.source);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    assert!(n.instances.values().any(|i| i.name == "u1_L_bb"), "single inductor between the legs");
    // divider from the override vref: (5/0.8 − 1)·100k = 525 kΩ
    let rt = n.instances.values().find(|i| i.name == "u1_R_top").unwrap().attributes.get("value").cloned().unwrap();
    let rv = bhdl_synthesizer::stage_acceptance::parse_si(&rt).unwrap();
    assert!((rv - 525e3).abs() / 525e3 < 0.02, "{rt}");
}

/// Power-up sequencing (ERC033): the domain contract carries any
/// combination of slots, explicit after-edges with hard t_min, and
/// sw-enabled rails; the netlist must IMPLEMENT it. The happy board
/// satisfies a slot edge and an after-edge through one rail-RC chain
/// (t = R·C·ln(Vs/(Vs−V_IH)) computed against t_min via the stage's
/// en_vih) and states the sw_enabled rail as a firmware assumption;
/// the variants break the chain (enable unwired) and shrink the C
/// (delay below t_min) — both named with the arithmetic.
#[tokio::test]
async fn power_sequencing_contract_and_erc033() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let board = |c_del: &str, chain: bool| {
        let chain_txt = if chain {
            format!("@V50 -> R_del: Res(100kΩ).1; R_del.2 -> u3.EN;\n    u3.EN -> C_del: Cap({c_del}).1; C_del.2 -> @GND;")
        } else {
            String::new()
        };
        format!(r#"
import {{ BuckBoost_TPS63020 }} from "bhdl-stdlib/power/tps63020.bhdl";
import {{ LinearRegulator }} from "bhdl-stdlib/power/regulator.bhdl";
import {{ Res }} from "bhdl-stdlib/passives/resistor.bhdl";
import {{ Cap }} from "bhdl-stdlib/passives/capacitor.bhdl";
entity SeqSoc() {{
    pin 1: power in;
    pin 2: power in;
    pin 3: power in;
    pin GND: ground;
    domain VDD_CORE pins="1" v=5V i_max=1A slot=1 source="FIXTURE — seq probe";
    domain VDD_AUX pins="2" v=3.3V i_max=0.5A slot=2 after="VDD_CORE" t_min=500us source="FIXTURE — seq probe";
    domain VDD_DBG pins="3" v=1.8V i_max=50mA sw_enabled=true after="VDD_AUX" source="FIXTURE — seq probe";
}}
board SeqBoard {{
    power VBAT = 3.6V @ 8A;
    port V50: power out = 5V @ 1A;
    port V33: power out = 3.3V @ 0.5A;
    port V18: power out = 1.8V @ 100mA;
    ground GND;
    @VBAT -> u1: BuckBoost_TPS63020(v_out=5V, i_out_max=1A, v_in=3.6V, v_in_min=3.0V, v_in_max=4.2V).VIN;
    u1.GND -> @GND; u1.VOUT -> @V50;
    @VBAT -> u3: BuckBoost_TPS63020(v_out=3.3V, i_out_max=0.5A, v_in=3.6V, v_in_min=3.0V, v_in_max=4.2V).VIN;
    u3.GND -> @GND; u3.VOUT -> @V33;
    {chain_txt}
    @V50 -> u2: LinearRegulator(1.8V).VIN;
    u2.GND -> @GND; u2.VOUT -> @V18;
    u2.EN -> R_pd: Res(100kΩ).1; R_pd.2 -> @GND;
    soc: SeqSoc();
    @V50 -> soc.1; @V33 -> soc.2; @V18 -> soc.3; soc.GND -> @GND;
}}
"#)
    };
    let build = |text: String| async move {
        let pr = parse(&text);
        assert!(pr.errors().is_empty(), "{:?}", pr.errors());
        let sf = SourceFile::cast(pr.syntax()).unwrap();
        let analysis = analyze(&sf);
        let mut gen = NetlistGenerator::new();
        let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
        let v = bhdl_synthesizer::sequencing::check_power_sequencing(&n, &analysis);
        (n, v)
    };
    use bhdl_synthesizer::design_rule_checker::ViolationSeverity;

    // happy path: slot 1→2 and after+t_min both ride the V50→RC→EN
    // chain (t = 1e5·22e-9·ln(5/3.8) = 604 µs ≥ 500 µs); sw_enabled is
    // a stated firmware assumption, not an error
    let (n, v) = build(board("22nF", true)).await;
    let soc = n.instances.values().find(|i| i.name == "soc").expect("soc");
    assert!(soc.attributes.contains_key("seqdom_VDD_CORE") && soc.attributes.contains_key("seqdom_VDD_AUX"), "contracts stamped: {:?}", soc.attributes.keys().collect::<Vec<_>>());
    let errs: Vec<_> = v.iter().filter(|x| x.severity == ViolationSeverity::Error).collect();
    assert!(errs.is_empty(), "happy board has sequencing errors: {errs:#?}");
    assert!(v.iter().any(|x| x.severity == ViolationSeverity::Info && x.description.contains("sw_enabled") && x.description.contains("software assumption")), "{v:#?}");

    // chain removed: the enable is unwired → the stage auto-enables and
    // the declared ordering has no implementing mechanism
    let (_n, v) = build(board("22nF", false)).await;
    assert!(v.iter().any(|x| x.severity == ViolationSeverity::Error && x.description.contains("EN of 'u3' is unwired") && x.description.contains("auto-enables")), "{v:#?}");

    // C shrunk: the RC crosses en_vih at 187 µs < the declared 500 µs
    let (_n, v) = build(board("6.8nF", true)).await;
    assert!(v.iter().any(|x| x.severity == ViolationSeverity::Error && x.description.contains("RC delay") && x.description.contains("t_min")), "{v:#?}");

    // PG chain: u1's power-good drives u3's enable (the exposed
    // contract PG), pulled up through the same RC — the chain is the
    // mechanism, the RC still implements t_min
    let (_n, v) = build(
        board("22nF", true).replace(
            "@V50 -> R_del:",
            "u1.PG -> u3.EN;\n    @V50 -> R_del:",
        ),
    )
    .await;
    let errs: Vec<_> = v.iter().filter(|x| x.severity == ViolationSeverity::Error).collect();
    assert!(errs.is_empty(), "PG-chain board has sequencing errors: {errs:#?}");

    // PG chain ALONE (no RC) under a declared t_min: the chain is
    // recognised as the mechanism, but the delay is unimplemented —
    // this error text only fires when the PG chain was detected, so it
    // is the non-vacuous proof of the PG mechanism
    let (_n, v) = build(
        board("22nF", false).replace("u3.GND -> @GND;", "u3.GND -> @GND; u1.PG -> u3.EN;"),
    )
    .await;
    assert!(v.iter().any(|x| x.severity == ViolationSeverity::Error && x.description.contains("PG chain with no RC")), "{v:#?}");
}

/// Power-up TIMELINE simulation (`bhdl powerup`): the knee. A
/// downstream stage's inrush, reflected through the topology, exceeds
/// the upstream stage's capability; the deficit drains the upstream
/// bulk and the rail sags below good while the downstream rail comes
/// up — a slot violation no pairwise check can see. Three arms:
/// 1. undersized bulk + RC enable → KNEE → slot re-opened (Error);
/// 2. big bulk + RC enable → the rail rises SLOWLY, the RC threshold
///    (1.2 V) fires long before the rail is good → order violation —
///    a REAL composition flaw the pairwise ERC033 check blesses;
/// 3. big bulk + PG chain (PG released only in regulation) → clean.
#[tokio::test]
async fn powerup_timeline_catches_the_knee() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let board = |bulk: bool, pg: bool| {
        let bulk_txt = if bulk { "@V50 -> C_bulk: Cap(1000µF).1; C_bulk.2 -> @GND;" } else { "" };
        let pg_txt = if pg { "u1.PG -> u3.EN;" } else { "" };
        format!(r#"
import {{ BuckBoost_TPS63020 }} from "bhdl-stdlib/power/tps63020.bhdl";
import {{ Res }} from "bhdl-stdlib/passives/resistor.bhdl";
import {{ Cap }} from "bhdl-stdlib/passives/capacitor.bhdl";
entity KneeSoc() {{
    pin 1: power in;
    pin 2: power in;
    pin GND: ground;
    domain VDD_CORE pins="1" v=5V i_nom=300mA slot=1 source="FIXTURE — knee probe";
    domain VDD_AUX pins="2" v=3.3V i_nom=100mA slot=2 source="FIXTURE — knee probe";
}}
board KneeBoard {{
    power VBAT = 3.6V @ 8A;
    port V50: power out = 5V @ 2A;
    port V33: power out = 3.3V @ 1A;
    ground GND;
    @VBAT -> u1: BuckBoost_TPS63020(v_out=5V, i_out_max=1A, v_in=3.6V, v_in_min=3.0V, v_in_max=4.2V).VIN;
    u1.GND -> @GND; u1.VOUT -> @V50;
    @V50 -> u3: BuckBoost_TPS63020(v_out=3.3V, i_out_max=0.5A, v_in=5V, v_in_min=4.5V, v_in_max=5.5V).VIN;
    u3.GND -> @GND; u3.VOUT -> @V33;
    {pg_txt}
    @V50 -> R_del: Res(100kΩ).1; R_del.2 -> u3.EN;
    u3.EN -> C_del: Cap(22nF).1; C_del.2 -> @GND;
    {bulk_txt}
    soc: KneeSoc();
    @V50 -> soc.1; @V33 -> soc.2; soc.GND -> @GND;
}}
"#)
    };
    let run = |text: String| async move {
        let pr = parse(&text);
        assert!(pr.errors().is_empty(), "{:?}", pr.errors());
        let sf = SourceFile::cast(pr.syntax()).unwrap();
        let analysis = analyze(&sf);
        let mut gen = NetlistGenerator::new();
        let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
        bhdl_synthesizer::powerup::simulate_powerup(&n, &sf)
    };
    use bhdl_synthesizer::powerup::Sev;

    // arm 1: the knee — V50 sags below good while V33 comes up
    let rep = run(board(false, false)).await;
    let v50 = rep.rails.iter().find(|r| r.net == "V50").unwrap();
    assert!(!v50.sags.is_empty(), "no sag simulated: {:#?}", rep.events);
    assert!(rep.events.iter().any(|e| e.text.contains("CURRENT LIMIT") && e.text.contains("demand") && e.text.contains("capability")), "{:#?}", rep.events);
    assert!(rep.events.iter().any(|e| e.text.contains("SAG begins")), "{:#?}", rep.events);
    assert!(rep.findings.iter().any(|f| f.sev == Sev::Error && f.text.contains("WHILE slot-1 rail") && f.text.contains("sagged below good")), "{:#?}", rep.findings);

    // arm 2: big bulk slows the rise; the RC threshold fires early and
    // AUX beats CORE — a real ordering violation from composition
    let rep = run(board(true, false)).await;
    assert!(rep.findings.iter().any(|f| f.sev == Sev::Error && f.text.contains("before slot 1 complete")), "{:#?}", rep.findings);

    // arm 3: PG chain (released only in regulation) + bulk → clean
    let rep = run(board(true, true)).await;
    let errs: Vec<_> = rep.findings.iter().filter(|f| f.sev == Sev::Error).collect();
    assert!(errs.is_empty(), "PG-chained board has timeline errors: {errs:#?}");
    let (v50g, v33g) = (
        rep.rails.iter().find(|r| r.net == "V50").unwrap().t_good.unwrap(),
        rep.rails.iter().find(|r| r.net == "V33").unwrap().t_good.unwrap(),
    );
    assert!(v33g > v50g, "PG chain must hold V33 until V50 is truly good");
}

/// Auto-decouple (spec §7.2): the power tree emits `decouple` for every
/// zmask-declaring domain — when the project names its capacitor
/// library. Without `decap_lib` the worklist is a STATED gap (C/ESR/ESL
/// are library data, never invented); a hand-written statement wins.
#[tokio::test]
async fn powertree_auto_decouple_worklist() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let board = |req: &str, hand: &str| format!(r#"
import {{ Ind }} from "bhdl-stdlib/passives/inductor.bhdl";
entity PdnSoc() {{
    pin 1: power in;
    pin GND: ground;
    domain VDD pins="1" v=3.3V i_nom=0.5A zmask="100kHz:200m 10MHz:200m" pdn_r=1m pdn_l=1n source="FIXTURE — autopdn probe";
}}
board AutoPdn {{
    {req}
    power VIN12 = 12V @ 3A;
    power V33 = 3.3V @ 1A;
    ground GND;
    soc: PdnSoc();
    @V33 -> l_feed: Ind(1µH).1; l_feed.2 -> soc.1; soc.GND -> @GND;
    {hand}
}}
"#);
    let run = |text: String| async move {
        let pr = parse(&text);
        assert!(pr.errors().is_empty(), "{:?}", pr.errors());
        let sf = SourceFile::cast(pr.syntax()).unwrap();
        let analysis = analyze(&sf);
        let mut gen = NetlistGenerator::new();
        let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
        let (stmts, notes) = bhdl_synthesizer::powertree::decouple_worklist(&n, &sf, &text);
        (stmts, notes)
    };
    // with the project library: the statement is emitted
    let (stmts, notes) = run(board(r#"requirements { decap_lib: "./tests/circuits/realistic/decap_lib_fixture.bhdl"; }"#, "")).await;
    assert_eq!(stmts.len(), 1, "{stmts:?} {notes:?}");
    assert!(stmts[0].starts_with("decouple soc.VDD from \"./tests/circuits/realistic/decap_lib_fixture.bhdl\""), "{}", stmts[0]);
    assert!(notes.is_empty(), "{notes:?}");
    // without it: a stated gap, never silence
    let (stmts, notes) = run(board("", "")).await;
    assert!(stmts.is_empty());
    assert!(notes.iter().any(|n| n.contains("soc.VDD") && n.contains("decap_lib") && n.contains("never invented")), "{notes:?}");
    // a hand-written statement wins (no duplicate emission, no note)
    let (stmts, notes) = run(board(
        r#"requirements { decap_lib: "./tests/circuits/realistic/decap_lib_fixture.bhdl"; }"#,
        r#"decouple soc.VDD from "./tests/circuits/realistic/decap_lib_fixture.bhdl" max_parts=4;"#,
    )).await;
    assert!(stmts.is_empty(), "{stmts:?}");
    assert!(notes.is_empty(), "{notes:?}");
}

/// Load-step interactions (§7.3): per-domain runs + peak-aligned
/// superposition screen with the SELF-CONSISTENCY gate. Two bursty
/// domains share one supply: each burst alone stays under the stage
/// limit (linear, zero droop — regulation holds), but the summed
/// demand crosses it, so superposition is invalid BY ITS OWN
/// ARITHMETIC — the screen names the stage and escalates the
/// implicated pair to a simultaneous nonlinear run, which confirms
/// current-limit entry and a droop over the declared droop_max.
/// Smaller bursts: the screen PROVES the linear region and the N
/// per-domain runs are the worst case (proof, not approximation).
#[tokio::test]
async fn load_step_superposition_screen_and_escalation() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let board = |step: &str| format!(r#"
import {{ BuckBoost_TPS63020 }} from "bhdl-stdlib/power/tps63020.bhdl";
import {{ Cap }} from "bhdl-stdlib/passives/capacitor.bhdl";
entity StepSoc() {{
    pin 1: power in;
    pin 2: power in;
    pin 3: power in;
    pin GND: ground;
    domain VDD_IO  pins="1" v=5V i_nom=150mA source="FIXTURE — steps probe";
    domain VDD_CPU pins="2" v=5V i_nom=50mA step={step} rise=10us dur=500us droop_max=3% source="FIXTURE — steps probe";
    domain VDD_GPU pins="3" v=5V i_nom=50mA step={step} rise=10us dur=500us droop_max=3% source="FIXTURE — steps probe";
}}
board StepBoard {{
    power VBAT = 3.6V @ 8A;
    port V50: power out = 5V @ 3A;
    ground GND;
    @VBAT -> u1: BuckBoost_TPS63020(v_out=5V, i_out_max=1.5A, v_in=3.6V, v_in_min=3.4V, v_in_max=4.2V).VIN;
    u1.GND -> @GND; u1.VOUT -> @V50;
    @V50 -> C_bulk: Cap(200µF).1; C_bulk.2 -> @GND;
    soc: StepSoc();
    @V50 -> soc.1; @V50 -> soc.2; @V50 -> soc.3; soc.GND -> @GND;
}}
"#);
    let run = |text: String| async move {
        let pr = parse(&text);
        assert!(pr.errors().is_empty(), "{:?}", pr.errors());
        let sf = SourceFile::cast(pr.syntax()).unwrap();
        let analysis = analyze(&sf);
        let mut gen = NetlistGenerator::new();
        let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
        bhdl_synthesizer::powerup::simulate_powerup(&n, &sf)
    };
    use bhdl_synthesizer::powerup::Sev;

    // coincidence crosses the limit: screen flags → escalation confirms
    let rep = run(board("1.2A")).await;
    assert_eq!(rep.steps.len(), 2, "{:#?}", rep.steps);
    assert!(rep.steps.iter().all(|s| s.extra_demand_a.iter().any(|(n, a)| n == "u1" && (*a - 1.2).abs() < 0.1)), "{:#?}", rep.steps);
    assert!(rep.interactions.iter().any(|l| l.contains("superposition invalid") && l.contains("u1")), "{:#?}", rep.interactions);
    assert!(rep.interactions.iter().any(|l| l.contains("SIMULTANEOUSLY")), "{:#?}", rep.interactions);
    assert!(rep.findings.iter().any(|f| f.sev == Sev::Error && f.text.contains("INTERACTION") && f.text.contains("current limit") && f.text.contains("droop_max")), "{:#?}", rep.findings);

    // smaller bursts: the self-consistency PROOF, no escalation
    let rep = run(board("0.9A")).await;
    assert!(rep.interactions.iter().any(|l| l.contains("SELF-CONSISTENT") && l.contains("proof, not approximation")), "{:#?}", rep.interactions);
    assert!(!rep.interactions.iter().any(|l| l.contains("SIMULTANEOUSLY")));
    assert!(rep.findings.iter().all(|f| f.sev != Sev::Error), "{:#?}", rep.findings);
}

/// Chain synthesis (§7.4): for a declared ordering whose target stage
/// has an unwired enable, the synthesizer emits the mechanism — PG
/// chain when the prerequisite's bound block exposes PG (C sized from
/// t_min against the detected pull-up and the target's en_vih), and a
/// hand-wired enable always wins.
#[tokio::test]
async fn seq_chain_synthesis_pg_and_rc() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    // both rails driven by BuckBoost_TPS63020 (PG + en_vih exposed):
    // the PG-chain branch with a t_min-sized C
    let text = r#"
import { BuckBoost_TPS63020 } from "bhdl-stdlib/power/tps63020.bhdl";
entity ChainSoc() {
    pin 1: power in;
    pin 2: power in;
    pin GND: ground;
    domain VDD_A pins="1" v=5V i_nom=100mA slot=1 source="FIXTURE — chain probe";
    domain VDD_B pins="2" v=3.3V i_nom=100mA slot=2 slot_t_min=1ms source="FIXTURE — chain probe";
}
board ChainBoard {
    power VBAT = 3.6V @ 8A;
    port V50: power out = 5V @ 1A;
    port V33: power out = 3.3V @ 1A;
    ground GND;
    @VBAT -> u1: BuckBoost_TPS63020(v_out=5V, i_out_max=1A, v_in=3.6V, v_in_min=3.0V, v_in_max=4.2V).VIN;
    u1.GND -> @GND; u1.VOUT -> @V50;
    @VBAT -> u2: BuckBoost_TPS63020(v_out=3.3V, i_out_max=0.5A, v_in=3.6V, v_in_min=3.0V, v_in_max=4.2V).VIN;
    u2.GND -> @GND; u2.VOUT -> @V33;
    soc: ChainSoc();
    @V50 -> soc.1; @V33 -> soc.2; soc.GND -> @GND;
}
"#;
    let pr = parse(text);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let plan = bhdl_synthesizer::powertree::synthesize_seq_chains(&n, &sf, "GND");
    assert!(plan.wiring.iter().any(|w| w == "u1.PG -> u2.EN;"), "{:?}", plan.wiring);
    // slot_t_min=1ms against the internal 1MΩ pull-up and en_vih 1.2V:
    // C = 1e-3 / (1e6·ln(5/3.8)) = 3.64 nF
    let c_line = plan.wiring.iter().find(|w| w.starts_with("u2.EN -> seqc_")).expect("delay C sized");
    let c_val: f64 = c_line.split("Cap(").nth(1).unwrap().split(')').next().unwrap().parse().unwrap();
    assert!((c_val - 3.64e-9).abs() / 3.64e-9 < 0.05, "{c_line}");
    // hand-wired enable wins: wire u2.EN and nothing is synthesized
    let hand = text.replace("soc: ChainSoc();", "@VBAT -> u2.EN;\n    soc: ChainSoc();");
    let pr = parse(&hand);
    assert!(pr.errors().is_empty());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let plan = bhdl_synthesizer::powertree::synthesize_seq_chains(&n, &sf, "GND");
    assert!(plan.wiring.is_empty(), "{:?}", plan.wiring);
}

/// Final PDN sanity (§7.5): loop stability against the DATASHEET
/// envelope (TPS61022: 20–1000 µF effective) and the resonance blind
/// spot (uncharacterized caps swept as ideal). The over-bulked board —
/// exactly what a runaway fixpoint would produce — is flagged with the
/// derated arithmetic; the sane board passes with only the stated
/// resonance UNCHECKED.
#[tokio::test]
async fn final_pdn_sanity_stability_and_resonance() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let board = |bulk: &str| format!(r#"
import {{ Boost_TPS61022 }} from "bhdl-stdlib/power/tps61022.bhdl";
import {{ Cap }} from "bhdl-stdlib/passives/capacitor.bhdl";
board SanityBoard {{
    power VBAT = 3.6V @ 8A;
    port V50: power out = 5V @ 2A;
    ground GND;
    @VBAT -> u1: Boost_TPS61022(v_out=5V, i_out_max=2A, v_in=3.6V).VIN;
    u1.GND -> @GND; u1.VOUT -> @V50;
    @V50 -> C_bulk: Cap({bulk}).1; C_bulk.2 -> @GND;
}}
"#);
    let run = |text: String| async move {
        let pr = parse(&text);
        assert!(pr.errors().is_empty(), "{:?}", pr.errors());
        let sf = SourceFile::cast(pr.syntax()).unwrap();
        let analysis = analyze(&sf);
        let mut gen = NetlistGenerator::new();
        let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
        bhdl_synthesizer::powertree::final_pdn_sanity(&n, &sf)
    };
    // 2200 µF nominal → ×1.2 effective > the 1000 µF datasheet max
    let out = run(board("2200µF")).await;
    assert!(out.iter().any(|l| l.starts_with("STABILITY:") && l.contains("1000µF") && l.contains("×1.2")), "{out:#?}");
    // 100 µF: inside the envelope both ways; only the stated resonance
    // blind spot remains (bare caps carry no ESR/ESL)
    let out = run(board("100µF")).await;
    assert!(!out.iter().any(|l| l.starts_with("STABILITY:")), "{out:#?}");
    assert!(out.iter().any(|l| l.contains("RESONANCE UNCHECKED") && l.contains("C_bulk")), "{out:#?}");
    // 30 µF nominal → ×0.5 worst-case effective (15+6 µF... total 36·0.5=18) < the 20 µF floor
    let out = run(board("24µF")).await;
    assert!(out.iter().any(|l| l.starts_with("STABILITY:") && l.contains("minimum") && l.contains("×0.5")), "{out:#?}");
}

/// Power-down / sleep timelines (§7.6): input loss discharges each
/// bank through ITS OWN loads (C·V/I physics — a lightly-loaded rail
/// outlives a heavy one), and sleep entry drops the sleep_off rails at
/// their i_sleep draw. Declared down orderings/windows are verified on
/// the simulated down-times; the missing-discharge-path case is named
/// with the fix.
#[tokio::test]
async fn powerdown_and_sleep_timelines() {
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let board = |bleed: &str, tmax: &str| format!(r#"
import {{ BuckBoost_TPS63020 }} from "bhdl-stdlib/power/tps63020.bhdl";
import {{ Res }} from "bhdl-stdlib/passives/resistor.bhdl";
import {{ Cap }} from "bhdl-stdlib/passives/capacitor.bhdl";
entity DownSoc() {{
    pin 1: power in;
    pin 2: power in;
    pin GND: ground;
    domain VDD_CORE pins="1" v=5V i_nom=300mA i_sleep=50mA source="FIXTURE — pdown probe";
    domain VDD_AUX pins="2" v=3.3V i_nom=5mA i_sleep=2uA sleep_off=true down_before="VDD_CORE" down_t_max={tmax} source="FIXTURE — pdown probe";
}}
board DownBoard {{
    power VBAT = 3.6V @ 8A;
    port V50: power out = 5V @ 1A;
    port V33: power out = 3.3V @ 0.5A;
    ground GND;
    @VBAT -> u1: BuckBoost_TPS63020(v_out=5V, i_out_max=1A, v_in=3.6V, v_in_min=3.0V, v_in_max=4.2V).VIN;
    u1.GND -> @GND; u1.VOUT -> @V50;
    @VBAT -> u2: BuckBoost_TPS63020(v_out=3.3V, i_out_max=0.3A, v_in=3.6V, v_in_min=3.0V, v_in_max=4.2V).VIN;
    u2.GND -> @GND; u2.VOUT -> @V33;
    @V33 -> C_big: Cap(470µF).1; C_big.2 -> @GND;
    {bleed}
    u2.EN -> R_pd: Res(100kΩ).1; R_pd.2 -> @GND;
    soc: DownSoc();
    @V50 -> soc.1; @V33 -> soc.2; soc.GND -> @GND;
}}
"#);
    let run = |text: String| async move {
        let pr = parse(&text);
        assert!(pr.errors().is_empty(), "{:?}", pr.errors());
        let sf = SourceFile::cast(pr.syntax()).unwrap();
        let analysis = analyze(&sf);
        let mut gen = NetlistGenerator::new();
        let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
        bhdl_synthesizer::powerup::simulate_powerdown(&n, &sf)
    };
    use bhdl_synthesizer::powerup::Sev;

    // no discharge path: the 536 µF bank at 5 mA cannot bleed within
    // the horizon — Error naming the fix; sleep re-entry hazard stated
    let rep = run(board("", "20ms")).await;
    assert!(rep.findings.iter().any(|f| f.sev == Sev::Error && f.text.contains("never discharged") && f.text.contains("discharge path")), "{:#?}", rep.findings);
    assert!(rep.findings.iter().any(|f| f.sev == Sev::Warning && f.text.contains("re-entry") && f.text.contains("2µA")), "{:#?}", rep.findings);

    // 40 Ω bleed: V33 down within its window — but the heavy-loaded
    // V50 dies in ~1 ms, so down_before=VDD_CORE is STILL violated,
    // with both times named (physics, not vacuity)
    let rep = run(board("@V33 -> R_bleed: Res(40Ω, wattage=1W).1; R_bleed.2 -> @GND;", "100ms")).await;
    assert!(rep.findings.iter().any(|f| f.sev == Sev::Error && f.text.contains("down_before=VDD_CORE violated") && f.text.contains("AFTER")), "{:#?}", rep.findings);
    assert!(!rep.findings.iter().any(|f| f.text.contains("never discharged")), "{:#?}", rep.findings);
    // sleep entry: the bleed shortens the re-entry latency, reported
    assert!(rep.sleep.iter().any(|e| e.text.contains("discharged in") && e.text.contains("re-entry")), "{:#?}", rep.sleep);

    // ordering dropped: clean
    let text = board("@V33 -> R_bleed: Res(40Ω, wattage=1W).1; R_bleed.2 -> @GND;", "100ms").replace(r#"down_before="VDD_CORE" "#, "");
    let rep = run(text).await;
    let errs: Vec<_> = rep.findings.iter().filter(|f| f.sev == Sev::Error).collect();
    assert!(errs.is_empty(), "{errs:#?}");
}

/// PMIC aggregation (§8): per-rail resolution first, then the
/// post-step asks whether one multi-output part covers the SET —
/// reported with coverage, price comparison and the built-in
/// sequencing, never auto-bound. The TPS65217B (B-variant OTP rails)
/// covers a 1.8 V buck + 1.1 V buck + 3.3 V LDO; a 5 V rail is
/// honestly left over. Direct instantiation gates per-rail circuits.
#[tokio::test]
async fn pmic_aggregation_reports_the_cover() {
    use bhdl_synthesizer::stage_resolution::resolve_stages;
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let stdlib = ws.join("bhdl-stdlib");
    let board = r#"
import { BuckStage, LdoStage, BoostStage } from "bhdl-stdlib/power/stages.bhdl";
import { Res } from "bhdl-stdlib/passives/resistor.bhdl";
board PmicBoard {
    power VSYS = 3.7V @ 5A;
    port V18: power out = 1.8V @ 0.8A;
    port V11: power out = 1.1V @ 0.9A;
    port V33: power out = 3.3V @ 80mA;
    port V50: power out = 5V @ 1A;
    ground GND;
    @VSYS -> u1: BuckStage(vout=1.8V, i_max=0.8A, vin=3.7V).VIN;
    u1.GND -> @GND; u1.VOUT -> @V18;
    @VSYS -> u2: BuckStage(vout=1.1V, i_max=0.9A, vin=3.7V).VIN;
    u2.GND -> @GND; u2.VOUT -> @V11;
    @VSYS -> u3: LdoStage(vout=3.3V, i_max=80mA, vin=3.7V).VIN;
    u3.GND -> @GND; u3.VOUT -> @V33;
    @VSYS -> u4: BoostStage(vout=5V, i_max=1A, vin=3.7V).VIN;
    u4.GND -> @GND; u4.VOUT -> @V50;
    @V50 -> R_LOAD: Res(5Ω, wattage=10W).1; R_LOAD.2 -> @GND;
}
"#;
    let r = resolve_stages(board, &stdlib, &[]).unwrap().unwrap();
    let lines = bhdl_synthesizer::aggregation::evaluate(&r.resolutions, &stdlib);
    let all = lines.join("\n");
    assert!(all.contains("Pmic_TPS65217B covers 3 of 4 rails"), "{all}");
    assert!(all.contains("u1 → DCDC1") && all.contains("u2 → DCDC2") && all.contains("u3 → LDO2"), "{all}");
    assert!(all.contains("not covered: u4"), "{all}");
    assert!(all.contains("built-in power-up order: LDO1,DCDC1,LDO2"), "{all}");
    assert!(all.contains("never auto-bound"), "{all}");

    // direct instantiation: only the used rails' application circuits
    // materialize (wired-gated) — 2 bucks + 1 LDO here
    let direct = r#"
import { Pmic_TPS65217B } from "bhdl-stdlib/power/tps65217.bhdl";
import { Res } from "bhdl-stdlib/passives/resistor.bhdl";
board PmicDirect {
    power VSYS = 3.7V @ 5A;
    port V18: power out = 1.8V @ 0.8A;
    port V11: power out = 1.1V @ 0.9A;
    port V33: power out = 3.3V @ 80mA;
    ground GND;
    pm: Pmic_TPS65217B();
    @VSYS -> pm.VIN;
    pm.GND -> @GND;
    pm.VOUT_DCDC1 -> @V18;
    pm.VOUT_DCDC2 -> @V11;
    pm.VOUT_LDO2 -> @V33;
    @V18 -> R1: Res(2.25Ω, wattage=2W).1; R1.2 -> @GND;
    @V11 -> R2: Res(1.25Ω, wattage=2W).1; R2.2 -> @GND;
    @V33 -> R3: Res(42Ω, wattage=1W).1; R3.2 -> @GND;
}
"#;
    let pr = parse(direct);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let inds = n.instances.values().filter(|i| i.name.starts_with("pm_L_dc")).count();
    assert_eq!(inds, 2, "only the two used bucks carry inductors");
    assert!(n.instances.values().any(|i| i.name == "pm_C_l2"), "LDO2 cap present");
    assert!(!n.instances.values().any(|i| i.name == "pm_L_dc3" || i.name == "pm_C_l1"), "unused rails carry no circuit");
}

/// Grouped PMIC commit + the strict sequencing gate (§8.1):
/// `resolve u1,u2,u3 = Pmic_TPS65217B;` collapses three per-rail
/// requirements into ONE multi-output block (endpoints remapped,
/// PWR_EN tied, mapping stamped), and ERC033 verifies declared domain
/// ordering against the PMIC's PROMISED strobe order + delay range:
/// guaranteed-met = promise-based pass; unachievable t_min = Error
/// with the arithmetic; contradicted order = Error with the strobes.
#[tokio::test]
async fn pmic_grouped_commit_and_strict_seq_gate() {
    use bhdl_synthesizer::stage_resolution::resolve_stages;
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let stdlib = ws.join("bhdl-stdlib");
    let board = |dom18: &str, dom33: &str| format!(r#"
import {{ BuckStage, LdoStage }} from "bhdl-stdlib/power/stages.bhdl";
import {{ Res }} from "bhdl-stdlib/passives/resistor.bhdl";
entity SeqSoc2() {{
    pin 1: power in;
    pin 2: power in;
    pin GND: ground;
    domain VDD18 pins="1" v=1.8V i_nom=100mA {dom18} source="FIXTURE — strict gate probe";
    domain VDD33 pins="2" v=3.3V i_nom=50mA {dom33} source="FIXTURE — strict gate probe";
}}
board PmicCommit {{
    power VSYS = 3.7V @ 5A;
    port V18: power out = 1.8V @ 0.8A;
    port V33: power out = 3.3V @ 80mA;
    ground GND;
    @VSYS -> u1: BuckStage(vout=1.8V, i_max=0.8A, vin=3.7V).VIN;
    u1.GND -> @GND; u1.VOUT -> @V18;
    @VSYS -> u3: LdoStage(vout=3.3V, i_max=80mA, vin=3.7V).VIN;
    u3.GND -> @GND; u3.VOUT -> @V33;
    soc: SeqSoc2();
    @V18 -> soc.1; @V33 -> soc.2; soc.GND -> @GND;
    resolve u1, u3 = Pmic_TPS65217B;
}}
"#);
    let run = |text: String| async move {
        let r = resolve_stages(&text, &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("bhdl-stdlib"), &[]).unwrap().unwrap();
        assert!(r.group_commits.iter().any(|l| l.contains("GROUPED COMMIT") && l.contains("u1→DCDC1") && l.contains("u3→LDO2")), "{:#?}", r.group_commits);
        let pr = parse(&r.source);
        assert!(pr.errors().is_empty(), "{:?}", pr.errors());
        let sf = SourceFile::cast(pr.syntax()).unwrap();
        let analysis = analyze(&sf);
        let mut gen = NetlistGenerator::new();
        let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
        let v = bhdl_synthesizer::sequencing::check_power_sequencing(&n, &analysis);
        (n, v)
    };
    use bhdl_synthesizer::design_rule_checker::ViolationSeverity;

    // guaranteed: 0.5 ms ≤ 1 strobe × 1 ms minimum delay — the promise
    // covers the declaration, a promise-based pass (Info)
    let (n, v) = run(board("", r#"after="VDD18" t_min=500us"#)).await;
    assert!(n.instances.values().any(|i| i.name == "u1_u"), "one PMIC materialized");
    assert!(!n.instances.values().any(|i| i.name.starts_with("u3_")), "u3 collapsed into the PMIC");
    assert!(v.iter().any(|x| x.severity == ViolationSeverity::Info && x.description.contains("inherited from") && x.description.contains("guaranteed")), "{v:#?}");
    assert!(!v.iter().any(|x| x.severity == ViolationSeverity::Error), "{v:#?}");
    // the powerup engine models the OTP STROBE schedule (SLVSB64I SEQ5
    // B reset: DLY1=5ms, others 1ms): DCDC1 good at strobe 2ms + 95%
    // of tSS 750µs ≈ 2.75ms; LDO2 at 7ms; windows verified on the
    // REAL times
    let pr2 = parse(&{
        let r = bhdl_synthesizer::stage_resolution::resolve_stages(&board("", r#"after="VDD18" t_min=500us"#), &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("bhdl-stdlib"), &[]).unwrap().unwrap();
        r.source
    });
    let sf2 = SourceFile::cast(pr2.syntax()).unwrap();
    let analysis2 = analyze(&sf2);
    let mut gen2 = NetlistGenerator::new();
    let n2 = gen2.generate_from_ast_and_analysis(&sf2, &analysis2).await.expect("synthesize");
    let rep = bhdl_synthesizer::powerup::simulate_powerup(&n2, &sf2);
    let tg = |rail: &str| rep.rails.iter().find(|r| r.net == rail).and_then(|r| r.t_good).unwrap_or_else(|| panic!("{rail}: {:#?}", rep.rails));
    assert!((tg("V18") - 2.75e-3).abs() < 0.3e-3, "V18 {}", tg("V18"));
    assert!((tg("V33") - 7.0e-3).abs() < 0.3e-3, "V33 {}", tg("V33"));
    assert!(rep.findings.iter().all(|f| f.sev != bhdl_synthesizer::powerup::Sev::Error), "{:#?}", rep.findings);

    // t_min 15 ms: the SEQ-AWARE assignment (§8.3) sidesteps the old
    // Error by picking LDO3 — 2 strobes from DCDC1, so 15 ms sits
    // inside the programmable 2–20 ms window: committed, with the
    // window stated UNCHECKED by ERC033
    {
        let r = resolve_stages(&board("", r#"after="VDD18" t_min=15ms"#), &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("bhdl-stdlib"), &[]).unwrap().unwrap();
        assert!(r.group_commits.iter().any(|l| l.contains("u3→LDO3")), "{:#?}", r.group_commits);
        let pr = parse(&r.source);
        let sf = SourceFile::cast(pr.syntax()).unwrap();
        let analysis = analyze(&sf);
        let mut gen = NetlistGenerator::new();
        let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
        let v = bhdl_synthesizer::sequencing::check_power_sequencing(&n, &analysis);
        assert!(v.iter().any(|x| x.severity == ViolationSeverity::Warning && x.description.contains("PROGRAMMED delays")), "{v:#?}");
        assert!(!v.iter().any(|x| x.severity == ViolationSeverity::Error), "{v:#?}");
    }

    // truly unachievable: t_min 40 ms exceeds even LDO4's 3-strobe ×
    // 10 ms spacing — the commit REFUSES with the custom-OTP proposal
    let e = resolve_stages(&board("", r#"after="VDD18" t_min=40ms"#), &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("bhdl-stdlib"), &[]).unwrap_err().to_string();
    assert!(e.contains("does not fit") && e.contains("CUSTOM-OTP proposal"), "{e}");

    // contradicted: VDD18 (1.8 V ⇒ only DCDC1, strobe 2) declared
    // after VDD33 (LDO outputs, all LATER strobes) — no assignment can
    // satisfy it; refused at RESOLVE time with the proposal
    let e = resolve_stages(&board(r#"after="VDD33""#, ""), &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("bhdl-stdlib"), &[]).unwrap_err().to_string();
    assert!(e.contains("does not fit") && e.contains("CUSTOM-OTP proposal"), "{e}");
}

/// OTP sequencing, both workflows (§8.3): with an EXISTING OTP the
/// assignment is SEQUENCING-AWARE — the requirement→output map is
/// chosen so the declared rail ordering rides the strobe order (the
/// greedy pick would violate it); when no assignment can fit, the
/// commit refuses AND emits the CUSTOM-OTP proposal — the strobe/DLY
/// spec to hand the vendor, quantized to the part's real delay codes,
/// with an honest INSUFFICIENT flag when a window exceeds the largest
/// step.
#[tokio::test]
async fn pmic_otp_seq_aware_assignment_and_proposal() {
    use bhdl_synthesizer::stage_resolution::resolve_stages;
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let stdlib = ws.join("bhdl-stdlib");
    let board = |tmin: &str| format!(r#"
import {{ LdoStage }} from "bhdl-stdlib/power/stages.bhdl";
entity OtpSoc() {{
    pin 1: power in;
    pin 2: power in;
    pin GND: ground;
    domain VDD_A pins="1" v=3.3V i_nom=50mA after="VDD_B" t_min={tmin} source="FIXTURE — otp probe";
    domain VDD_B pins="2" v=3.3V i_nom=50mA source="FIXTURE — otp probe";
}}
board OtpBoard {{
    power VSYS = 3.7V @ 5A;
    port VA: power out = 3.3V @ 150mA;
    port VB: power out = 3.3V @ 150mA;
    ground GND;
    @VSYS -> ua: LdoStage(vout=3.3V, i_max=150mA, vin=3.7V).VIN;
    ua.GND -> @GND; ua.VOUT -> @VA;
    @VSYS -> ub: LdoStage(vout=3.3V, i_max=150mA, vin=3.7V).VIN;
    ub.GND -> @GND; ub.VOUT -> @VB;
    soc: OtpSoc();
    @VA -> soc.1; @VB -> soc.2; soc.GND -> @GND;
    resolve ua, ub = Pmic_TPS65217B;
}}
"#);
    // fit: VDD_A after VDD_B forces the NON-greedy assignment
    // (ub→LDO3 strobe 4, ua→LDO4 strobe 5); 1 ms guaranteed
    let r = resolve_stages(&board("1ms"), &stdlib, &[]).unwrap().unwrap();
    assert!(r.group_commits.iter().any(|l| l.contains("ua→LDO4") && l.contains("ub→LDO3")), "{:#?}", r.group_commits);
    // unfit: 50 ms exceeds every strobe spacing — refused WITH the
    // vendor-handoff proposal
    let e = resolve_stages(&board("50ms"), &stdlib, &[]).unwrap_err().to_string();
    assert!(e.contains("does not fit"), "{e}");
    assert!(e.contains("CUSTOM-OTP proposal") && e.contains("STROBE2: ua") && e.contains("INSUFFICIENT"), "{e}");
}

/// OTP-friendly PMIC library (§8.4): ONE configurable block carries
/// every catalog variant as data. A grouped resolve naming the FAMILY
/// lets the resolver pick the variant (a 1.5 V DCDC1 exists only in
/// the C row); `otp="custom"` takes the §8.3 proposal shape directly,
/// drives the strobed timeline from the designer's schedule, and the
/// MPN is a visible PENDING sentinel until the vendor assigns one.
#[tokio::test]
async fn pmic_variant_family_selection_and_custom_otp() {
    use bhdl_synthesizer::stage_resolution::resolve_stages;
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    let stdlib = ws.join("bhdl-stdlib");
    // family resolve: only variant C has DCDC1 = 1.5 V and a 400 mA
    // 1.8 V LS-LDO — the resolver selects it
    let fam = r#"
import { BuckStage, LdoStage } from "bhdl-stdlib/power/stages.bhdl";
board FamilyBoard {
    power VSYS = 3.7V @ 5A;
    port V15: power out = 1.5V @ 0.8A;
    port V18L: power out = 1.8V @ 300mA;
    ground GND;
    @VSYS -> u1: BuckStage(vout=1.5V, i_max=0.8A, vin=3.7V).VIN;
    u1.GND -> @GND; u1.VOUT -> @V15;
    @VSYS -> u2: LdoStage(vout=1.8V, i_max=300mA, vin=3.7V).VIN;
    u2.GND -> @GND; u2.VOUT -> @V18L;
    resolve u1, u2 = Pmic_TPS65217;
}
"#;
    let r = resolve_stages(fam, &stdlib, &[]).unwrap().unwrap();
    assert!(r.group_commits.iter().any(|l| l.contains("Pmic_TPS65217C") && l.contains("u1→DCDC1")), "{:#?}", r.group_commits);

    // custom OTP: the proposal spec IS the configuration — strobed
    // timeline from the designer's schedule, PENDING MPN visible
    let custom = r#"
import { Pmic_TPS65217 } from "bhdl-stdlib/power/tps65217.bhdl";
import { Res } from "bhdl-stdlib/passives/resistor.bhdl";
board CustomOtp {
    power VSYS = 3.7V @ 5A;
    port V12: power out = 1.2V @ 1A;
    port V25: power out = 2.5V @ 80mA;
    ground GND;
    pm: Pmic_TPS65217(otp="custom", mpn="PENDING-CUSTOM-OTP", otp_outputs="DCDC1:buck:1.2V:1.2A,LDO2:ldo:2.5V:0.1A", otp_seq="DCDC1,LDO2", otp_strobe_t="DCDC1:0ms,LDO2:5ms");
    @VSYS -> pm.VIN;
    pm.GND -> @GND;
    pm.VOUT_DCDC1 -> @V12;
    pm.VOUT_LDO2 -> @V25;
    @V12 -> R1: Res(1.2Ω, wattage=2W).1; R1.2 -> @GND;
    @V25 -> R2: Res(31Ω, wattage=1W).1; R2.2 -> @GND;
}
"#;
    let pr = parse(custom);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    let rep = bhdl_synthesizer::powerup::simulate_powerup(&n, &sf);
    let tg = |rail: &str| rep.rails.iter().find(|r| r.net == rail).and_then(|r| r.t_good).unwrap_or_else(|| panic!("{rail}: {:#?}", rep.rails));
    // DCDC1 at 0 + 95 % of tSS; LDO2 at the designer's 5 ms strobe
    assert!((tg("V12") - 0.71e-3).abs() < 0.3e-3, "V12 {}", tg("V12"));
    assert!((tg("V25") - 5.0e-3).abs() < 0.3e-3, "V25 {}", tg("V25"));
    // the part carries the PENDING sentinel — a visible BOM gap
    let u = n.instances.values().find(|i| i.name == "pm_u").expect("silicon");
    assert_eq!(u.attributes.get("part_number").map(|v| v.trim_matches(char::from(34)).to_string()).unwrap_or_default(), "PENDING-CUSTOM-OTP");
}

/// Per-part DC-bias consumption (§7.5 addendum 3): a library part
/// declaring its vendor-exported curve is judged at the EFFECTIVE
/// capacitance for the rail's bias — the decap sweep stamps the
/// biased value, the minted instance carries the curve, and the
/// interpolation itself is exact at and between breakpoints. The ×0.5
/// class band remains only for curve-less parts.
#[tokio::test]
async fn dc_bias_curve_consumption() {
    use bhdl_synthesizer::decap_synthesis::{c_effective_at, parse_dc_bias};
    let ws = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::env::set_current_dir(ws).unwrap();
    // the arithmetic: exact at breakpoints, linear between, clamped
    let curve = parse_dc_bias("0V:47µF,2V:39µF,3.3V:31µF,5V:22µF");
    assert_eq!(curve.len(), 4);
    assert!((c_effective_at(47e-6, &curve, 3.3) - 31e-6).abs() < 1e-9);
    assert!((c_effective_at(47e-6, &curve, 1.0) - 43e-6).abs() < 1e-9); // midpoint 0→2 V
    assert!((c_effective_at(47e-6, &curve, 9.0) - 22e-6).abs() < 1e-9); // clamped
    assert!((c_effective_at(47e-6, &[], 3.3) - 47e-6).abs() < 1e-12); // no curve = nominal

    // integration: the biased fixture part, when selected, carries its
    // curve on the minted instance and the SOLVER value is the biased
    // effective (< nominal) at the 3.3 V rail
    let board = r#"
import { Res } from "bhdl-stdlib/passives/resistor.bhdl";
import { Ind } from "bhdl-stdlib/passives/inductor.bhdl";
entity BiasSoc() {
    pin 1: power in;
    pin GND: ground;
    domain VDD pins="1" v=3.3V i_nom=0.5A zmask="100kHz:200m 10MHz:200m" pdn_r=1m pdn_l=1n source="FIXTURE — dc-bias probe";
}
board BiasBoard {
    power V33 = 3.3V @ 2A;
    ground GND;
    @V33 -> l_feed: Ind(1µH).1; l_feed.2 -> soc: BiasSoc().1;
    soc.GND -> @GND;
    decouple soc.VDD from "tests/circuits/realistic/decap_lib_fixture.bhdl" max_parts=8;
}
"#;
    let pr = parse(board);
    assert!(pr.errors().is_empty(), "{:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let n = gen.generate_from_ast_and_analysis(&sf, &analysis).await.expect("synthesize");
    // whatever network was selected: every minted decap from a curve
    // part must carry dc_bias, and its solver value must equal the
    // biased effective at 3.3 V, not the nominal
    let mut saw_biased = false;
    for inst in n.instances.values() {
        if !inst.attributes.contains_key("decap_origin") {
            continue;
        }
        if let Some(curve_txt) = inst.attributes.get("dc_bias") {
            let nominal = bhdl_synthesizer::stage_acceptance::parse_si(
                inst.attributes.get("capacitance").unwrap(),
            )
            .unwrap();
            let val: f64 = inst.attributes.get("value").unwrap().parse().unwrap();
            let expect = c_effective_at(nominal, &parse_dc_bias(curve_txt.trim_matches(char::from(34))), 3.3);
            assert!((val - expect).abs() / expect < 1e-6, "{val} vs {expect}");
            assert!(val < nominal, "biased value must sit below nominal at 3.3 V");
            saw_biased = true;
        }
    }
    // the fixture's biased 47 µF part exists in the library; if the
    // greedy solver never selected it this test would be VACUOUS —
    // assert we actually exercised the path
    assert!(saw_biased, "no biased part was selected — the fixture no longer exercises the dc_bias path: {:#?}", n.instances.values().filter(|i| i.attributes.contains_key("decap_origin")).map(|i| i.name.clone()).collect::<Vec<_>>());
}
