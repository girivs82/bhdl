//! Tests for the interface-constraint reader, against the real shipped
//! DDR4 attribute strings (`bhdl-synthesizer/src/bin/test_ddr4_stdlib.rs`).

use super::*;
use crate::constraint::{Constraint, SwizzleScope, TopoKind};
use crate::types::NetId;
use slotmap::SlotMap;

/// The DDR4 controller (`mc`) interface-constraint attributes as shipped.
const DDR4_MC: &[(&str, &str)] = &[
    ("intf_const__ddr.lane0.DQ0__single_ended", "34ohm"),
    ("intf_const__ddr.lane0.DQ0__signal_class", "DATA"),
    ("intf_const__ddr.lane0.DQS.P__differential", "80ohm"),
    ("intf_const__ddr.ca.CK_t__differential", "100ohm"),
    ("intf_const__ddr.ca.CK_t__signal_class", "CLOCK"),
    ("intf_const__ddr.ca.A0__signal_class", "ADDR"),
    ("intf_const__ddr.lane0.DQ0__swizzle_within_byte", "true"),
    ("intf_const__ddr.lane0.DQ1__swizzle_within_byte", "true"),
    ("intf_const__ddr.lane0.DQ0__swizzle_across_bytes", "true"),
    ("intf_const__ddr.ca.A0__topology", "fly_by"),
];

#[test]
fn parses_ddr4_per_signal_props() {
    let (parsed, diags) = parse_interface_attrs(DDR4_MC.iter().copied());
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    // single_ended 34ohm on DQ0
    assert!(parsed.iter().any(|c| matches!(&c.prop, IfaceProp::SingleEnded { ohms } if (*ohms - 34.0).abs() < 1e-3)
        && c.target == IfaceTarget::PerSignal("ddr.lane0.DQ0".into())));
    // differential 80ohm on DQS.P
    assert!(parsed.iter().any(|c| matches!(&c.prop, IfaceProp::Differential { ohms } if (*ohms - 80.0).abs() < 1e-3)
        && c.target == IfaceTarget::PerSignal("ddr.lane0.DQS.P".into())));
    // signal_class CLOCK on CK_t
    assert!(parsed.iter().any(|c| matches!(&c.prop, IfaceProp::SignalClass { class } if class == "CLOCK")));
    // topology fly_by on A0
    assert!(parsed.iter().any(|c| matches!(&c.prop, IfaceProp::Topology { kind: TopoKind::FlyBy })));
    // swizzle flags
    assert!(parsed.iter().any(|c| matches!(c.prop, IfaceProp::SwizzleWithinByte)));
    assert!(parsed.iter().any(|c| matches!(c.prop, IfaceProp::SwizzleAcrossBytes)));
}

#[test]
fn unknown_property_warns_and_degrades() {
    let attrs = [
        ("intf_const__ddr.lane0.DQ0__single_ended", "34ohm"),
        ("intf_const__ddr.lane0.DQ0__future_prop", "whatever"),
    ];
    let (parsed, diags) = parse_interface_attrs(attrs.iter().copied());
    // The known one parses; the unknown one is captured + diagnosed, not fatal.
    assert!(parsed.iter().any(|c| matches!(c.prop, IfaceProp::SingleEnded { .. })));
    assert!(parsed.iter().any(|c| matches!(&c.prop, IfaceProp::Unknown { name, .. } if name == "future_prop")));
    assert_eq!(diags.len(), 1);
}

#[test]
fn lowers_ddr4_to_typed_constraints() {
    // Assign a stable NetId per distinct pin-path the test references.
    let mut nets: SlotMap<NetId, ()> = SlotMap::with_key();
    let paths = [
        "ddr.lane0.DQ0", "ddr.lane0.DQ1",
        "ddr.lane0.DQS.P", "ddr.lane0.DQS.N",
        "ddr.ca.CK_t", "ddr.ca.CK_c", "ddr.ca.A0",
    ];
    let ids: std::collections::HashMap<&str, NetId> =
        paths.iter().map(|p| (*p, nets.insert(()))).collect();
    let resolve = |path: &str| ids.get(path).copied();

    let (parsed, _) = parse_interface_attrs(DDR4_MC.iter().copied());
    let (cons, diags) = lower_interface_constraints(&parsed, "mc", &resolve, &Default::default());
    assert!(diags.is_empty(), "lowering diagnostics: {diags:?}");

    let count = |pred: &dyn Fn(&Constraint) -> bool| cons.iter().filter(|c| pred(c)).count();

    // single_ended → Impedance(34) on DQ0.
    assert!(cons.iter().any(|c| matches!(c, Constraint::Impedance { target_ohms, .. } if (*target_ohms - 34.0).abs() < 1e-3)));

    // differential DQS.P → DiffPair(DQS.P, DQS.N) + Impedance(80) on both.
    let diff_pairs = count(&|c| matches!(c, Constraint::DiffPair { .. }));
    assert_eq!(diff_pairs, 2, "DQS and CK pairs"); // DQS.P/.N and CK_t/_c
    let imp80 = count(&|c| matches!(c, Constraint::Impedance { target_ohms, .. } if (*target_ohms - 80.0).abs() < 1e-3));
    assert_eq!(imp80, 2, "80ohm on DQS.P and DQS.N");

    // signal_class → SignalClass tags (DATA, CLOCK, ADDR).
    let classes: Vec<&str> = cons.iter().filter_map(|c| match c {
        Constraint::SignalClass { class, .. } if !class.is_empty() => Some(class.as_str()),
        _ => None,
    }).collect();
    assert!(classes.contains(&"DATA"));
    assert!(classes.contains(&"CLOCK"));
    assert!(classes.contains(&"ADDR"));

    // topology fly_by → Topology.
    assert!(cons.iter().any(|c| matches!(c, Constraint::Topology { kind: TopoKind::FlyBy, .. })));

    // swizzle_within_byte on DQ0+DQ1 (same parent ddr.lane0) → one
    // WithinGroup of 2. swizzle_across_bytes on DQ0 alone → no group
    // (needs ≥2 members).
    let within: Vec<&Constraint> = cons.iter().filter(|c| matches!(c, Constraint::SwizzleGroup { scope: SwizzleScope::WithinGroup, .. })).collect();
    assert_eq!(within.len(), 1, "one within-byte swizzle group");
    if let Constraint::SwizzleGroup { members, .. } = within[0] {
        assert_eq!(members.len(), 2, "DQ0 + DQ1");
    }
}

#[test]
fn ck_t_underscore_pair_inference() {
    // CK_t / CK_c differential naming (DDR4Ca uses _t/_c, not .P/.N).
    let mut nets: SlotMap<NetId, ()> = SlotMap::with_key();
    let ckt = nets.insert(());
    let ckc = nets.insert(());
    let resolve = |path: &str| match path {
        "ddr.ca.CK_t" => Some(ckt),
        "ddr.ca.CK_c" => Some(ckc),
        _ => None,
    };
    let attrs = [("intf_const__ddr.ca.CK_t__differential", "100ohm")];
    let (parsed, _) = parse_interface_attrs(attrs.iter().copied());
    let (cons, diags) = lower_interface_constraints(&parsed, "mc", &resolve, &Default::default());
    assert!(diags.is_empty(), "diags: {diags:?}");
    // Pair inferred from _t → _c sibling.
    assert!(cons.iter().any(|c| matches!(c, Constraint::DiffPair { .. })));
}

#[test]
fn prefix_constants_match_synth_side() {
    assert_eq!(ATTR_PREFIX, "intf_const__");
    assert_eq!(REL_ATTR_PREFIX, "intf_const_rel__");
}

#[test]
fn provenance_enriches_constraint_source() {
    use bhdl_common::constraint_provenance::{
        ConstraintProvenance, ConstraintProvenanceMap, ConstraintTier,
    };

    let mut nets: SlotMap<NetId, ()> = SlotMap::with_key();
    let n = nets.insert(());
    let resolve = |path: &str| if path == "ddr.lane0.DQ0" { Some(n) } else { None };

    let attrs = [("intf_const__ddr.lane0.DQ0__single_ended", "34ohm")];
    let (parsed, _) = parse_interface_attrs(attrs.iter().copied());

    // Provenance sidecar: the winning contributor at line 34 in DDR4Data.
    let mut prov: ConstraintProvenanceMap = Default::default();
    prov.insert(
        "intf_const__ddr.lane0.DQ0__single_ended".into(),
        vec![ConstraintProvenance::new("34ohm", Some(34), ConstraintTier::Specific, "DDR4Data")],
    );

    let (cons, _) = lower_interface_constraints(&parsed, "mc", &resolve, &prov);
    let imp = cons.iter().find(|c| matches!(c, Constraint::Impedance { .. })).unwrap();
    let src = imp.source();
    assert_eq!(src.line, Some(34), "line carried from provenance");
    assert_eq!(src.file, "DDR4Data", "interface scope carried as file");
    assert_eq!(src.intent_kind, "interface:single_ended");

    // Without provenance, the source has no line (back-compat).
    let (cons2, _) = lower_interface_constraints(&parsed, "mc", &resolve, &Default::default());
    let src2 = cons2.iter().find(|c| matches!(c, Constraint::Impedance { .. })).unwrap().source();
    assert_eq!(src2.line, None);
    assert!(src2.file.is_empty());
}
