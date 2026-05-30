//! Tests for the constraint conflict-detection pass.

use super::*;
use crate::constraint::{
    Constraint, ConstraintSource, CostShape, EntitySel, Hardness, TopoKind,
};
use crate::types::{ComponentId, NetId};
use slotmap::SlotMap;

fn src(kind: &str) -> ConstraintSource {
    ConstraintSource::intent(kind)
}

fn two_components() -> (ComponentId, ComponentId) {
    let mut k: SlotMap<ComponentId, ()> = SlotMap::with_key();
    (k.insert(()), k.insert(()))
}

#[test]
fn detects_distance_contradiction() {
    let (a, b) = two_components();
    let cons = vec![
        Constraint::Proximity {
            a: EntitySel::Component(a),
            b: EntitySel::Component(b),
            max_mm: 2.0,
            hardness: Hardness::Hard,
            source: src("high_freq_bypass"),
        },
        Constraint::KeepAway {
            a: EntitySel::Component(b), // reversed order — still the same pair
            b: EntitySel::Component(a),
            min_mm: 5.0,
            hardness: Hardness::Soft { shape: CostShape::Linear, weight: 1.0 },
            source: src("feedback_divider"),
        },
    ];
    let conflicts = detect_conflicts(&cons);
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].kind, ConflictKind::DistanceContradiction);
    // One side is hard → error.
    assert_eq!(conflicts[0].severity, Severity::Error);
    // Diagnostic names both sources.
    let d = conflicts[0].describe();
    assert!(d.contains("high_freq_bypass") && d.contains("feedback_divider"), "{d}");
}

#[test]
fn compatible_distance_bounds_no_conflict() {
    let (a, b) = two_components();
    let cons = vec![
        Constraint::Proximity {
            a: EntitySel::Component(a), b: EntitySel::Component(b),
            max_mm: 5.0, hardness: Hardness::Hard, source: src("x"),
        },
        Constraint::KeepAway {
            a: EntitySel::Component(a), b: EntitySel::Component(b),
            min_mm: 2.0, hardness: Hardness::Hard, source: src("y"),
        },
    ];
    // 2mm ≤ d ≤ 5mm is satisfiable — no contradiction.
    assert!(detect_conflicts(&cons).is_empty());
}

#[test]
fn detects_cross_net_impedance_contradiction() {
    let mut nets: SlotMap<NetId, ()> = SlotMap::with_key();
    let n = nets.insert(());
    // Two interfaces merged onto one net demand different impedances.
    let cons = vec![
        Constraint::Impedance { net: n, target_ohms: 34.0, tolerance_pct: 10.0, source: src("interface:single_ended") },
        Constraint::Impedance { net: n, target_ohms: 40.0, tolerance_pct: 10.0, source: src("interface:single_ended") },
    ];
    let conflicts = detect_conflicts(&cons);
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].kind, ConflictKind::ImpedanceContradiction);
    assert_eq!(conflicts[0].severity, Severity::Error);
    assert!(conflicts[0].message.contains("34") && conflicts[0].message.contains("40"));
}

#[test]
fn same_impedance_no_conflict() {
    let mut nets: SlotMap<NetId, ()> = SlotMap::with_key();
    let n = nets.insert(());
    let cons = vec![
        Constraint::Impedance { net: n, target_ohms: 50.0, tolerance_pct: 5.0, source: src("a") },
        Constraint::Impedance { net: n, target_ohms: 50.0, tolerance_pct: 5.0, source: src("b") },
    ];
    assert!(detect_conflicts(&cons).is_empty());
}

#[test]
fn detects_topology_overdetermination() {
    let mut nets: SlotMap<NetId, ()> = SlotMap::with_key();
    let n = nets.insert(());
    let cons = vec![
        Constraint::Topology { net: n, kind: TopoKind::FlyBy, root: None, stub_max_mm: None, source: src("interface:topology") },
        Constraint::Topology { net: n, kind: TopoKind::Star, root: None, stub_max_mm: None, source: src("interface:topology") },
    ];
    let conflicts = detect_conflicts(&cons);
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].kind, ConflictKind::TopologyOverdetermined);
}

#[test]
fn detects_signal_class_conflict_as_warning() {
    let mut nets: SlotMap<NetId, ()> = SlotMap::with_key();
    let n = nets.insert(());
    let cons = vec![
        Constraint::SignalClass { net: n, class: "DATA".into(), max_freq_hz: None, source: src("a") },
        Constraint::SignalClass { net: n, class: "CLOCK".into(), max_freq_hz: None, source: src("b") },
    ];
    let conflicts = detect_conflicts(&cons);
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].kind, ConflictKind::SignalClassConflict);
    assert_eq!(conflicts[0].severity, Severity::Warning);
    let (errors, warnings) = count_by_severity(&conflicts);
    assert_eq!((errors, warnings), (0, 1));
}

#[test]
fn clean_set_has_no_conflicts() {
    let mut nets: SlotMap<NetId, ()> = SlotMap::with_key();
    let n1 = nets.insert(());
    let n2 = nets.insert(());
    let cons = vec![
        Constraint::Impedance { net: n1, target_ohms: 50.0, tolerance_pct: 5.0, source: src("a") },
        Constraint::Impedance { net: n2, target_ohms: 90.0, tolerance_pct: 5.0, source: src("b") },
        Constraint::SignalClass { net: n1, class: "DATA".into(), max_freq_hz: None, source: src("c") },
    ];
    assert!(detect_conflicts(&cons).is_empty());
}
