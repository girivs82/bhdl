//! Constraint conflict detection (`constraint_model_v0.md` §9).
//!
//! Runs over the flat `Board.constraints` set *before* placement and
//! reports contradictions, naming both `ConstraintSource`s so a user can
//! fix the offending intent / interface constraint. Hard contradictions
//! are errors; soft ones are warnings.
//!
//! Cross-net protocol contradictions (e.g. two interface `single_ended`
//! impedances merged onto one net by board wiring) are detected here, not
//! synth-side — the synth emits `intf_const__*` per-module *before*
//! net-merge and is structurally blind to the merge (handshake §10).

use std::collections::HashMap;

use crate::types::NetId;

use super::{Constraint, ConstraintSource, EntitySel, TopoKind};

/// What kind of contradiction was found.
#[derive(Debug, Clone, PartialEq)]
pub enum ConflictKind {
    /// `Proximity(a,b ≤ d)` and `KeepAway(a,b ≥ d')` with `d < d'`.
    DistanceContradiction,
    /// Two `Impedance` on one net with different targets.
    ImpedanceContradiction,
    /// Two `Topology` on one net with different kinds.
    TopologyOverdetermined,
    /// Two distinct non-empty `signal_class` values on one net.
    SignalClassConflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// One detected contradiction, with both originating sources.
#[derive(Debug, Clone)]
pub struct Conflict {
    pub kind: ConflictKind,
    pub severity: Severity,
    pub message: String,
    pub sources: (ConstraintSource, ConstraintSource),
}

impl Conflict {
    /// One-line diagnostic naming both sources.
    pub fn describe(&self) -> String {
        let sev = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        format!(
            "constraint {sev}: {} [{} @ {}{} vs {} @ {}{}]",
            self.message,
            self.sources.0.intent_kind,
            self.sources.0.file,
            self.sources.0.line.map(|l| format!(":{l}")).unwrap_or_default(),
            self.sources.1.intent_kind,
            self.sources.1.file,
            self.sources.1.line.map(|l| format!(":{l}")).unwrap_or_default(),
        )
    }
}

/// Unordered equality of an entity pair.
fn same_pair(a1: EntitySel, b1: EntitySel, a2: EntitySel, b2: EntitySel) -> bool {
    (a1 == a2 && b1 == b2) || (a1 == b2 && b1 == a2)
}

/// Detect all conflicts in a constraint set.
pub fn detect_conflicts(constraints: &[Constraint]) -> Vec<Conflict> {
    let mut conflicts = Vec::new();

    detect_distance(constraints, &mut conflicts);
    detect_same_net(constraints, &mut conflicts);

    conflicts
}

/// Proximity vs KeepAway on the same entity pair with incompatible bounds.
fn detect_distance(constraints: &[Constraint], out: &mut Vec<Conflict>) {
    let proximities: Vec<(&EntitySel, &EntitySel, f32, &ConstraintSource, bool)> = constraints
        .iter()
        .filter_map(|c| match c {
            Constraint::Proximity { a, b, max_mm, hardness, source } => {
                Some((a, b, *max_mm, source, hardness.is_hard()))
            }
            _ => None,
        })
        .collect();
    let keepaways: Vec<(&EntitySel, &EntitySel, f32, &ConstraintSource, bool)> = constraints
        .iter()
        .filter_map(|c| match c {
            Constraint::KeepAway { a, b, min_mm, hardness, source } => {
                Some((a, b, *min_mm, source, hardness.is_hard()))
            }
            _ => None,
        })
        .collect();

    for (pa, pb, pmax, psrc, phard) in &proximities {
        for (ka, kb, kmin, ksrc, khard) in &keepaways {
            if same_pair(**pa, **pb, **ka, **kb) && *pmax < *kmin {
                let severity = if *phard || *khard { Severity::Error } else { Severity::Warning };
                out.push(Conflict {
                    kind: ConflictKind::DistanceContradiction,
                    severity,
                    message: format!(
                        "proximity ≤ {pmax}mm conflicts with keep-away ≥ {kmin}mm on the same pair"
                    ),
                    sources: ((*psrc).clone(), (*ksrc).clone()),
                });
            }
        }
    }
}

/// Same-net contradictions: impedance, topology, signal class.
fn detect_same_net(constraints: &[Constraint], out: &mut Vec<Conflict>) {
    // net → (target_ohms, source)
    let mut imped: HashMap<NetId, (f32, &ConstraintSource)> = HashMap::new();
    // net → (kind, source)
    let mut topo: HashMap<NetId, (&TopoKind, &ConstraintSource)> = HashMap::new();
    // net → (class, source)
    let mut sclass: HashMap<NetId, (&str, &ConstraintSource)> = HashMap::new();

    for c in constraints {
        match c {
            Constraint::Impedance { net, target_ohms, source, .. } => {
                if let Some((prev, psrc)) = imped.get(net) {
                    if (prev - target_ohms).abs() > 1e-3 {
                        out.push(Conflict {
                            kind: ConflictKind::ImpedanceContradiction,
                            severity: Severity::Error,
                            message: format!(
                                "net has conflicting impedance targets: {prev}Ω vs {target_ohms}Ω"
                            ),
                            sources: ((*psrc).clone(), source.clone()),
                        });
                    }
                } else {
                    imped.insert(*net, (*target_ohms, source));
                }
            }
            Constraint::Topology { net, kind, source, .. } => {
                if let Some((prev, psrc)) = topo.get(net) {
                    if *prev != kind {
                        out.push(Conflict {
                            kind: ConflictKind::TopologyOverdetermined,
                            severity: Severity::Error,
                            message: format!(
                                "net has conflicting topologies: {prev:?} vs {kind:?}"
                            ),
                            sources: ((*psrc).clone(), source.clone()),
                        });
                    }
                } else {
                    topo.insert(*net, (kind, source));
                }
            }
            Constraint::SignalClass { net, class, source, .. } if !class.is_empty() => {
                if let Some((prev, psrc)) = sclass.get(net) {
                    if *prev != class.as_str() {
                        out.push(Conflict {
                            kind: ConflictKind::SignalClassConflict,
                            severity: Severity::Warning,
                            message: format!(
                                "net has conflicting signal classes: '{prev}' vs '{class}'"
                            ),
                            sources: ((*psrc).clone(), source.clone()),
                        });
                    }
                } else {
                    sclass.insert(*net, (class.as_str(), source));
                }
            }
            _ => {}
        }
    }
}

/// Count conflicts by severity.
pub fn count_by_severity(conflicts: &[Conflict]) -> (usize, usize) {
    let errors = conflicts.iter().filter(|c| c.severity == Severity::Error).count();
    let warnings = conflicts.len() - errors;
    (errors, warnings)
}

#[cfg(test)]
#[path = "conflicts_tests.rs"]
mod tests;
