//! Integration test for the functional-safety semantic pass
//! (docs/spec/Functional_Safety.md §3–§5) on the supervised-regulator
//! fixture: an entity that is its own safety part, a board instantiating
//! it twice, and the board block composing both.
//!
//! Asserts: zero resolution errors; the entity block is applied once per
//! instance with handles resolved to the flattened instances
//! (`dut.mon` → `rail_a_mon`); refinement and assumption discharge from
//! the board block land on the instance scopes; the parts table groups
//! by safety part; the gap list is exactly the honest Phase-1 set (no
//! EFFECT_UNDETECTED thanks to refinement coverage, FAULT_UNRUN for
//! every declared fault, PART_NO_SAFETY_DATA for every unwaived part);
//! and the baseline/delta sees a part added INSIDE the entity under both
//! instances.

use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_common::safety::{AssumptionStatus, Delta, GapClass, MechanismKind, PartData, SafetyModel};
use bhdl_parser::parse;
use bhdl_synthesizer::safety_model::build_safety_model;
use bhdl_synthesizer::NetlistGenerator;

async fn model_for(src: &str) -> SafetyModel {
    let ws_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    std::env::set_current_dir(ws_root).expect("cwd");
    let pr = parse(src);
    assert!(pr.errors().is_empty(), "parse: {:?}", pr.errors());
    let sf = SourceFile::cast(pr.syntax()).unwrap();
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let netlist = gen
        .generate_from_ast_and_analysis(&sf, &analysis)
        .await
        .expect("synthesize");
    build_safety_model(&netlist, &[&sf])
}

#[tokio::test]
async fn supervised_reg_model_resolves_and_gaps_are_honest() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("tests/circuits/realistic/test_safety_supervised_reg.bhdl"),
    )
    .unwrap();
    let m = model_for(&src).await;
    assert!(m.errors.is_empty(), "errors: {:#?}", m.errors);
    assert_eq!(m.board, "DualRail");

    // Scopes: board + rail_a + rail_b (sorted by path; "" first).
    let paths: Vec<&str> = m.scopes.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(paths, vec!["", "rail_a", "rail_b"]);

    let ra = m.scopes.iter().find(|s| s.path == "rail_a").unwrap();
    assert_eq!(ra.entity, "SupervisedReg5V");
    assert_eq!(ra.ns, "dut");
    let sg_ov = ra.goals.iter().find(|g| g.name == "SG_OV").unwrap();
    assert_eq!(sg_ov.path, "rail_a.SG_OV");
    assert_eq!(sg_ov.level.as_str(), "ASIL_B");
    assert_eq!(sg_ov.refines.as_deref(), Some("SG_SUPPLY"), "board block refined the instance goal");
    assert_eq!(sg_ov.effects.len(), 2);
    assert!(sg_ov.effects.iter().any(|e| e.name == "silent_ov" && e.refs.contains(&"rail_a.nFAULT".to_string())));

    // Mechanisms resolved to the flattened instance.
    let psm: Vec<_> = ra.mechanisms.iter().filter(|m| m.kind == MechanismKind::Psm).collect();
    assert_eq!(psm.len(), 2);
    assert!(psm.iter().all(|m| m.instance == "rail_a_mon" && m.handle == "dut.mon"));
    assert!(psm.iter().all(|m| m.claimed_dc == Some(0.90) && m.dc_source.is_some()));

    // Faults resolved; none run.
    assert_eq!(ra.faults.len(), 4);
    assert!(ra.faults.iter().all(|f| !f.run));
    assert!(ra.faults.iter().any(|f| f.kind == "short" && f.targets == vec!["rail_a_r_fb_bot.1", "rail_a_r_fb_bot.2"] && f.detected_by.as_deref() == Some("rail_a_mon") && f.within.as_deref() == Some("10ms")));
    assert!(ra.faults.iter().any(|f| f.kind == "state" && f.targets[0] == "rail_a_mon"));

    // Waiver + assumptions discharged from the board block.
    assert_eq!(ra.waivers.len(), 1);
    assert_eq!(ra.waivers[0].instance, "rail_a_c_out");
    let a1 = ra.assumptions.iter().find(|a| a.id == "ASM_SUPPLY_WITHIN_ABSMAX").unwrap();
    assert_eq!(a1.status, AssumptionStatus::SatisfiedBy("tvs".into()));
    let a2 = ra.assumptions.iter().find(|a| a.id == "ASM_LOAD_REACTS_TO_FLAG").unwrap();
    assert!(matches!(a2.status, AssumptionStatus::Waived(_)));
    // The supervisor entity's own assumption of use surfaced in the rail
    // scope and was discharged by the board block.
    let a3 = ra.assumptions.iter().find(|a| a.id == "mon.ASM_SUP_VDD").expect("part assumption surfaced");
    assert_eq!(a3.path, "rail_a.mon.ASM_SUP_VDD");
    assert!(matches!(a3.status, AssumptionStatus::Waived(_)));
    // Entity safety data reached the part table.
    assert!(matches!(m.parts.iter().find(|p| p.instance == "rail_a_mon").unwrap().data, PartData::Behavioral { failure_states: 3, .. }));

    // Parts grouped by safety part; waived part carries its reason.
    let rail_a_parts: Vec<&str> = m.parts.iter().filter(|p| p.parent.as_deref() == Some("rail_a")).map(|p| p.instance.as_str()).collect();
    assert_eq!(rail_a_parts, vec!["rail_a_c_out", "rail_a_mon", "rail_a_r_fb_bot", "rail_a_r_fb_top", "rail_a_r_pu", "rail_a_reg"]);
    assert!(matches!(m.parts.iter().find(|p| p.instance == "rail_a_c_out").unwrap().data, PartData::Waived { .. }));
    assert!(m.parts.iter().any(|p| p.instance == "tvs" && p.parent.is_none()));

    // Gaps: refinement covers SG_SUPPLY.any_ov; every fault unrun; every
    // unwaived part lacks data; no open assumptions; no unsourced dc.
    let count = |c: GapClass| m.gaps.iter().filter(|g| g.class == c).count();
    assert_eq!(count(GapClass::EffectUndetected), 0, "{:#?}", m.gaps);
    assert_eq!(count(GapClass::FaultUnrun), 8);
    assert_eq!(count(GapClass::PartNoSafetyData), 11); // 15 parts - 2 waived caps - 2 supervisors with data
    assert_eq!(count(GapClass::AssumptionOpen), 0);
    assert_eq!(count(GapClass::DcUnsourced), 0);
    assert_eq!(count(GapClass::PsmWithoutLsm), 0); // ASIL B needs none
    assert!(!m.verdict_pass());

    // Baseline/delta: a part added INSIDE the entity shows up under both
    // instances, with its two new gaps, and nothing else moves.
    let before = m.baseline();
    let src2 = src.replace(
        "        VOUT -> r_pu: Res(10kΩ).1; r_pu.2 -> nFAULT;\n",
        "        VOUT -> r_pu: Res(10kΩ).1; r_pu.2 -> nFAULT;\n        adj -> r_snub: Res(1kΩ).1; r_snub.2 -> GND;\n",
    );
    assert_ne!(src, src2);
    let m2 = model_for(&src2).await;
    assert!(m2.errors.is_empty(), "errors: {:#?}", m2.errors);
    let d = Delta::between(&before, &m2.baseline());
    let added: Vec<&str> = d.parts.added.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(added, vec!["rail_a_r_snub", "rail_b_r_snub"]);
    assert!(d.parts.removed.is_empty() && d.parts.changed.is_empty());
    assert_eq!(d.gaps.added.len(), 2);
    assert!(d.goals.is_empty() && d.effects.is_empty() && d.mechanisms.is_empty() && d.assumptions.is_empty() && d.faults.is_empty());
    // Determinism: same source → identical baseline.
    let m3 = model_for(&src).await;
    assert_eq!(m3.baseline(), before);
}
