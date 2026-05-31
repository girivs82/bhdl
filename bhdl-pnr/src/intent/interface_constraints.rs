//! Interface-constraint boundary reader.
//!
//! The second producer of the constraint catalog (`constraint_model_v0.md`
//! §1, §5a): protocol-derived net/signal rules that the synth side already
//! emits (v0.8, shipped) as **module attributes** on each instance:
//!
//! ```text
//! intf_const__<pin_path>__<prop>           = <value>   // per-signal
//! intf_const_rel__<from>__<to>__<prop>     = <value>   // pairwise relation
//! ```
//!
//! This module parses those attributes into the typed `Constraint`
//! catalog. It is split in two so the property vocabulary is testable
//! without a netlist:
//!   1. [`parse_interface_attrs`] — `(key, value)` → structured
//!      [`IfaceConstraint`] (pure; property-vocabulary + value parsing +
//!      diff-pair partner inference + swizzle classification).
//!   2. [`lower_interface_constraints`] — `[IfaceConstraint]` + a
//!      pin-path→`NetId` resolver → `Vec<Constraint>` (swizzle-group
//!      prefix reconstruction; net resolution).
//!
//! The two prefix constants are owned by the synth side
//! (`bhdl-synthesizer/src/hierarchical_connectivity.rs`); the reader uses
//! them rather than hardcoding the strings (handshake §8.2.1). They are
//! re-declared here to avoid a hard dep cycle, with a test asserting they
//! match.

use std::collections::BTreeMap;

use crate::constraint::{
    Constraint, ConstraintSource, SwizzleScope, TopoKind,
};
use crate::types::NetId;

/// `intf_const__` — must match
/// `hierarchical_connectivity::INTERFACE_CONSTRAINT_ATTR_PREFIX`.
pub const ATTR_PREFIX: &str = "intf_const__";
/// `intf_const_rel__` — must match
/// `hierarchical_connectivity::INTERFACE_CONSTRAINT_REL_ATTR_PREFIX`.
pub const REL_ATTR_PREFIX: &str = "intf_const_rel__";

// Propagation velocity for time→length conversion of `length_match`/
// `skew_max` picosecond values (constraint_model_v0 §5a.1): ≈ 6.5 ps/mm
// for a typical inner-layer microstrip. Replaced by per-layer velocity in
// the v1 stackup model.
const PS_PER_MM: f32 = 6.5;

/// What a single interface-constraint attribute targets.
#[derive(Debug, Clone, PartialEq)]
pub enum IfaceTarget {
    /// A single signal at a dotted leaf path (`ddr.lane0.DQ0`).
    PerSignal(String),
    /// A pairwise relation between two leaf paths.
    Pairwise(String, String),
}

/// A parsed interface-constraint property + value.
#[derive(Debug, Clone, PartialEq)]
pub enum IfaceProp {
    SingleEnded { ohms: f32 },
    Differential { ohms: f32 },
    SignalClass { class: String },
    MaxFreqHz { hz: f64 },
    Topology { kind: TopoKind },
    /// Pairwise length match (picoseconds of propagation delay).
    LengthMatchPs { ps: f32 },
    /// Pairwise skew bound (picoseconds).
    SkewMaxPs { ps: f32 },
    SwizzleWithinByte,
    SwizzleAcrossBytes,
    /// Property name we don't interpret — warn-and-degrade (§2).
    Unknown { name: String, value: String },
}

/// One parsed interface constraint (target + property).
#[derive(Debug, Clone, PartialEq)]
pub struct IfaceConstraint {
    pub target: IfaceTarget,
    pub prop: IfaceProp,
    /// The original `intf_const__*` / `intf_const_rel__*` attribute key,
    /// used to look this constraint up in the provenance sidecar map.
    pub key: String,
}

/// Parse a set of `(attr_key, attr_value)` pairs (an instance's module
/// attributes) into structured interface constraints. Non-`intf_const`
/// keys are ignored. Returns the parsed constraints plus any
/// warn-and-degrade diagnostics for unknown properties.
pub fn parse_interface_attrs<'a, I>(attrs: I) -> (Vec<IfaceConstraint>, Vec<String>)
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    let mut out = Vec::new();
    let mut diags = Vec::new();

    for (key, value) in attrs {
        // The provenance sidecar is not itself a constraint.
        if key == bhdl_common::constraint_provenance::INTERFACE_CONSTRAINT_PROVENANCE_ATTR {
            continue;
        }
        let parsed = if let Some(rest) = key.strip_prefix(REL_ATTR_PREFIX) {
            parse_rel(rest, value, key)
        } else if let Some(rest) = key.strip_prefix(ATTR_PREFIX) {
            parse_per_signal(rest, value, key)
        } else {
            continue; // not an interface-constraint attribute
        };
        match parsed {
            Some(c) => {
                if let IfaceProp::Unknown { name, .. } = &c.prop {
                    diags.push(format!(
                        "interface constraint: unrecognized property `{name}` (skipped)"
                    ));
                }
                out.push(c);
            }
            None => diags.push(format!("interface constraint: malformed key `{key}`")),
        }
    }
    (out, diags)
}

/// `<pin_path>__<prop>` (the prop is the final `__`-delimited segment).
fn parse_per_signal(rest: &str, value: &str, key: &str) -> Option<IfaceConstraint> {
    let (pin_path, prop_name) = rest.rsplit_once("__")?;
    Some(IfaceConstraint {
        target: IfaceTarget::PerSignal(pin_path.to_string()),
        prop: parse_prop(prop_name, value),
        key: key.to_string(),
    })
}

/// `<from>__<to>__<prop>`.
fn parse_rel(rest: &str, value: &str, key: &str) -> Option<IfaceConstraint> {
    // The prop is the last segment; from/to are the two before it.
    let (head, prop_name) = rest.rsplit_once("__")?;
    let (from, to) = head.rsplit_once("__")?;
    Some(IfaceConstraint {
        target: IfaceTarget::Pairwise(from.to_string(), to.to_string()),
        prop: parse_prop(prop_name, value),
        key: key.to_string(),
    })
}

fn parse_prop(name: &str, value: &str) -> IfaceProp {
    match name {
        "single_ended" => match parse_ohms(value) {
            Some(ohms) => IfaceProp::SingleEnded { ohms },
            None => unknown(name, value),
        },
        "differential" => match parse_ohms(value) {
            Some(ohms) => IfaceProp::Differential { ohms },
            None => unknown(name, value),
        },
        "signal_class" => IfaceProp::SignalClass { class: value.to_string() },
        "max_freq" => match parse_freq_hz(value) {
            Some(hz) => IfaceProp::MaxFreqHz { hz },
            None => unknown(name, value),
        },
        "topology" => match parse_topology(value) {
            Some(kind) => IfaceProp::Topology { kind },
            None => unknown(name, value),
        },
        "length_match" => match parse_ps(value) {
            Some(ps) => IfaceProp::LengthMatchPs { ps },
            None => unknown(name, value),
        },
        "skew_max" => match parse_ps(value) {
            Some(ps) => IfaceProp::SkewMaxPs { ps },
            None => unknown(name, value),
        },
        "swizzle_within_byte" if value == "true" => IfaceProp::SwizzleWithinByte,
        "swizzle_across_bytes" if value == "true" => IfaceProp::SwizzleAcrossBytes,
        _ => unknown(name, value),
    }
}

fn unknown(name: &str, value: &str) -> IfaceProp {
    IfaceProp::Unknown { name: name.to_string(), value: value.to_string() }
}

// ── value parsers ────────────────────────────────────────────────────

fn parse_ohms(v: &str) -> Option<f32> {
    v.trim().trim_end_matches("ohm").trim_end_matches('Ω').trim().parse().ok()
}

fn parse_ps(v: &str) -> Option<f32> {
    let v = v.trim();
    if let Some(n) = v.strip_suffix("ps") {
        n.trim().parse().ok()
    } else if let Some(n) = v.strip_suffix("ns") {
        n.trim().parse::<f32>().ok().map(|x| x * 1000.0)
    } else {
        v.parse().ok()
    }
}

fn parse_freq_hz(v: &str) -> Option<f64> {
    let v = v.trim();
    let (num, mult) = if let Some(n) = v.strip_suffix("GHz") {
        (n, 1e9)
    } else if let Some(n) = v.strip_suffix("MHz") {
        (n, 1e6)
    } else if let Some(n) = v.strip_suffix("kHz") {
        (n, 1e3)
    } else if let Some(n) = v.strip_suffix("Hz") {
        (n, 1.0)
    } else {
        (v, 1.0)
    };
    num.trim().parse::<f64>().ok().map(|x| x * mult)
}

fn parse_topology(v: &str) -> Option<TopoKind> {
    match v.trim() {
        "star" => Some(TopoKind::Star),
        "daisy_chain" | "daisy" => Some(TopoKind::DaisyChain),
        "fly_by" | "flyby" => Some(TopoKind::FlyBy),
        "t" | "tee" => Some(TopoKind::T),
        _ => None,
    }
}

// ── lowering: IfaceConstraint → Constraint ───────────────────────────

/// Resolve a dotted leaf pin-path (instance-relative, e.g.
/// `ddr.lane0.DQ0`) to the P&R `NetId` it lands on. Returns `None` if the
/// path doesn't resolve (warn-and-degrade).
pub type NetResolver<'a> = dyn Fn(&str) -> Option<NetId> + 'a;

/// Lower parsed interface constraints to the typed catalog, resolving pin
/// paths via `resolve`. `instance` names the owning instance for
/// provenance. Swizzle groups are reconstructed by shared parent prefix
/// (handshake §8.2.2). Returns constraints + diagnostics.
pub fn lower_interface_constraints(
    parsed: &[IfaceConstraint],
    instance: &str,
    resolve: &NetResolver,
    provenance: &bhdl_common::constraint_provenance::ConstraintProvenanceMap,
) -> (Vec<Constraint>, Vec<String>) {
    use bhdl_common::constraint_provenance::ConstraintProvenance;

    let mut out = Vec::new();
    let mut diags = Vec::new();
    // pin-path → scope, for swizzle prefix reconstruction.
    let mut swizzle_within: Vec<String> = Vec::new();
    let mut swizzle_across: Vec<String> = Vec::new();

    // Build a ConstraintSource, enriched with the synth side's provenance
    // sidecar (handshake §10/§11): the winning contributor's `.bhdl` line
    // and declaring interface scope. `file` carries the interface type
    // name (no absolute path threaded yet — line + scope is traceable).
    let src = |key: &str, prop: &str| -> ConstraintSource {
        let (file, line) = provenance
            .get(key)
            .and_then(|e| ConstraintProvenance::winner(e))
            .map(|w| (w.scope.clone(), w.line))
            .unwrap_or_default();
        ConstraintSource {
            file,
            line,
            intent_kind: format!("interface:{prop}"),
            recipe_version: "0".into(),
        }
    };

    let mut resolve_or_warn = |path: &str, prop: &str, diags: &mut Vec<String>| -> Option<NetId> {
        match resolve(path) {
            Some(n) => Some(n),
            None => {
                diags.push(format!(
                    "interface:{prop} on `{instance}.{path}`: net did not resolve (dropped)"
                ));
                None
            }
        }
    };

    for c in parsed {
        match (&c.target, &c.prop) {
            (IfaceTarget::PerSignal(path), IfaceProp::SingleEnded { ohms }) => {
                if let Some(net) = resolve_or_warn(path, "single_ended", &mut diags) {
                    out.push(Constraint::Impedance {
                        net,
                        target_ohms: *ohms,
                        tolerance_pct: 10.0,
                        source: src(&c.key, "single_ended"),
                    });
                }
            }
            (IfaceTarget::PerSignal(path), IfaceProp::Differential { ohms }) => {
                // `<...>.P` implies a pair with the sibling `<...>.N`.
                if let Some(p_path) = path.strip_suffix(".P").or_else(|| path.strip_suffix("_t")) {
                    let n_suffix = if path.ends_with(".P") { ".N" } else { "_c" };
                    let n_path = format!("{p_path}{n_suffix}");
                    let p = resolve_or_warn(path, "differential", &mut diags);
                    let n = resolve_or_warn(&n_path, "differential", &mut diags);
                    if let (Some(p_net), Some(n_net)) = (p, n) {
                        out.push(Constraint::DiffPair {
                            p_net,
                            n_net,
                            spacing_mm: 0.15,
                            length_match_mm: 0.1,
                            source: src(&c.key, "differential"),
                        });
                        for net in [p_net, n_net] {
                            out.push(Constraint::Impedance {
                                net,
                                target_ohms: *ohms,
                                tolerance_pct: 5.0,
                                source: src(&c.key, "differential"),
                            });
                        }
                    }
                } else if let Some(net) = resolve_or_warn(path, "differential", &mut diags) {
                    // Differential declared on a non-.P leaf: treat as impedance.
                    out.push(Constraint::Impedance {
                        net,
                        target_ohms: *ohms,
                        tolerance_pct: 5.0,
                        source: src(&c.key, "differential"),
                    });
                }
            }
            (IfaceTarget::PerSignal(path), IfaceProp::SignalClass { class }) => {
                if let Some(net) = resolve_or_warn(path, "signal_class", &mut diags) {
                    out.push(Constraint::SignalClass {
                        net,
                        class: class.clone(),
                        max_freq_hz: None,
                        source: src(&c.key, "signal_class"),
                    });
                }
            }
            (IfaceTarget::PerSignal(path), IfaceProp::MaxFreqHz { hz }) => {
                if let Some(net) = resolve_or_warn(path, "max_freq", &mut diags) {
                    out.push(Constraint::SignalClass {
                        net,
                        class: String::new(),
                        max_freq_hz: Some(*hz),
                        source: src(&c.key, "max_freq"),
                    });
                }
            }
            (IfaceTarget::PerSignal(path), IfaceProp::Topology { kind }) => {
                if let Some(net) = resolve_or_warn(path, "topology", &mut diags) {
                    out.push(Constraint::Topology {
                        net,
                        kind: kind.clone(),
                        root: None,
                        stub_max_mm: None,
                        source: src(&c.key, "topology"),
                    });
                }
            }
            (IfaceTarget::PerSignal(path), IfaceProp::SwizzleWithinByte) => {
                swizzle_within.push(path.clone());
            }
            (IfaceTarget::PerSignal(path), IfaceProp::SwizzleAcrossBytes) => {
                swizzle_across.push(path.clone());
            }
            (IfaceTarget::Pairwise(a, b), IfaceProp::LengthMatchPs { ps }) => {
                let na = resolve_or_warn(a, "length_match", &mut diags);
                let nb = resolve_or_warn(b, "length_match", &mut diags);
                if let (Some(na), Some(nb)) = (na, nb) {
                    out.push(Constraint::LengthMatchGroup {
                        nets: vec![na, nb],
                        tolerance_mm: ps / PS_PER_MM,
                        hardness: crate::constraint::Hardness::Hard,
                        source: src(&c.key, "length_match"),
                    });
                }
            }
            (IfaceTarget::Pairwise(a, b), IfaceProp::SkewMaxPs { ps }) => {
                let na = resolve_or_warn(a, "skew_max", &mut diags);
                let nb = resolve_or_warn(b, "skew_max", &mut diags);
                if let (Some(na), Some(nb)) = (na, nb) {
                    out.push(Constraint::LengthMatchGroup {
                        nets: vec![na, nb],
                        tolerance_mm: ps / PS_PER_MM,
                        hardness: crate::constraint::Hardness::Soft {
                            shape: crate::constraint::CostShape::Hinge { slack: 0.0 },
                            weight: 1.0,
                        },
                        source: src(&c.key, "skew_max"),
                    });
                }
            }
            (_, IfaceProp::Unknown { .. }) => {} // already diagnosed at parse
            _ => {} // prop/target mismatch (e.g. swizzle on a pairwise) — ignore
        }
    }

    // Reconstruct swizzle groups from shared parent prefix
    // (handshake §8.2.2): members sharing the dotted parent are one group.
    lower_swizzle(&swizzle_within, SwizzleScope::WithinGroup, instance, resolve, &src, &mut out, &mut diags);
    lower_swizzle(&swizzle_across, SwizzleScope::AcrossGroups, instance, resolve, &src, &mut out, &mut diags);

    (out, diags)
}

/// Group swizzle members by their dotted parent prefix and emit one
/// `SwizzleGroup` per parent.
fn lower_swizzle(
    paths: &[String],
    scope: SwizzleScope,
    instance: &str,
    resolve: &NetResolver,
    src: &dyn Fn(&str, &str) -> ConstraintSource,
    out: &mut Vec<Constraint>,
    diags: &mut Vec<String>,
) {
    if paths.is_empty() {
        return;
    }
    let prop = match scope {
        SwizzleScope::WithinGroup => "swizzle_within_byte",
        SwizzleScope::AcrossGroups => "swizzle_across_bytes",
    };
    // Group by parent prefix (everything up to the last '.'), keeping one
    // representative member path per parent for provenance lookup.
    let mut by_parent: BTreeMap<String, (Vec<NetId>, String)> = BTreeMap::new();
    for path in paths {
        let parent = path.rsplit_once('.').map(|(p, _)| p).unwrap_or("").to_string();
        match resolve(path) {
            Some(net) => {
                let e = by_parent.entry(parent).or_insert_with(|| (Vec::new(), path.clone()));
                e.0.push(net);
            }
            None => diags.push(format!(
                "interface:{prop} on `{instance}.{path}`: net did not resolve (dropped from swizzle group)"
            )),
        }
    }
    for (_parent, (members, rep_path)) in by_parent {
        if members.len() >= 2 {
            let key = format!("{ATTR_PREFIX}{rep_path}__{prop}");
            out.push(Constraint::SwizzleGroup { members, scope, source: src(&key, prop) });
        }
    }
}

#[cfg(test)]
#[path = "interface_constraints_tests.rs"]
mod tests;
