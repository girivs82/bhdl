//! Elaboration emitter — synthesized netlist → STRUCTURAL bhdl text.
//!
//! The default pipeline is `bhdl → elaborate → synthesize`: this
//! module renders the post-sugar netlist (virtual pins resolved,
//! expansion children explicit, derived values inlined) back as plain
//! bhdl so a designer can READ what synthesized, and so the round-trip
//! gate can prove the elaborated file re-synthesizes to the IDENTICAL
//! netlist. Generated-only — never hand-edited; every synthesized
//! element carries a provenance comment naming the intent that
//! produced it.
//!
//! Emission rules (v1):
//! - imports are passed through so entity TYPES stay imported;
//! - each instance re-emits as `name: Type(<args>)` with ctor args
//!   reconstructed from the entity's declared params via the
//!   param→exported-attribute mapping (stdlib convention:
//!   `Res(value: resistance)` exports `attribute resistance = value`);
//!   a param with no derivable attribute falls back to its default and
//!   says so in the provenance comment;
//! - connectivity is anchor arrows per net: the first (inst, pin)
//!   anchors, every further pin attaches with `anchor -> other;`
//!   (chains sharing a pin merge into one net); power nets emit as
//!   `@RAIL -> pin;`, ground as `pin -> @GND;` — net NAMES are chosen
//!   by the same auto-net rules, which the round-trip gate checks.

use std::collections::{BTreeMap, HashMap};

use bhdl_netlist::netlist::Netlist;

/// One entity's constructor signature, extracted from its AST: the
/// params in DECLARATION order, each with the attribute the entity
/// exports it under (`Res(value: resistance)` + `attribute resistance
/// = value;` ⇒ ("value", Some("resistance"), None)) and its default
/// literal when declared. This is how ctor args are reconstructed
/// from a synthesized instance's RESOLVED attributes.
#[derive(Debug, Clone, Default)]
pub struct EntityCtor {
    /// (param_name, exported_attribute, default_literal)
    pub params: Vec<(String, Option<String>, Option<String>)>,
}

/// Render the synthesized netlist as structural bhdl.
///
/// `source` names the original file (header provenance only).
/// UNFINISHED (task #85): instance-arg reconstruction and the
/// power-net classification emit are stubs — see TODOs.
pub fn emit_elaborated(
    netlist: &Netlist,
    source: &str,
    ctors: &HashMap<String, EntityCtor>,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "// ELABORATED bhdl — generated from {source}; DO NOT EDIT.\n\
         // The round-trip gate re-synthesizes this file and requires the\n\
         // IDENTICAL netlist. Every synthesized element carries a\n\
         // provenance comment naming the intent that produced it.\n\n"
    ));
    // ── instances, sorted by name for stable diffs ──
    // Phantom entity-definition stubs (instance named exactly like its
    // module, zero connected pins) are template artifacts, not parts —
    // same filter build_board applies.
    let connected: std::collections::HashSet<_> = netlist
        .pin_instances
        .values()
        .filter(|pi| pi.net.is_some())
        .map(|pi| pi.instance)
        .collect();
    let mut insts: Vec<_> = netlist
        .instances
        .iter()
        .filter(|(id, i)| {
            let is_stub_name = netlist
                .modules
                .get(i.definition)
                .map(|m| m.name == i.name)
                .unwrap_or(false);
            !(is_stub_name && !connected.contains(id))
        })
        .map(|(_, i)| i)
        .collect();
    insts.sort_by(|a, b| a.name.cmp(&b.name));
    for inst in &insts {
        let ty = netlist
            .modules
            .get(inst.definition)
            .map(|m| m.name.clone())
            .unwrap_or_default();
        // Provenance: any expansion_/vpin_ attribute means this
        // instance was SYNTHESIZED, not typed — say by what.
        let attrs: BTreeMap<&String, &String> = inst.attributes.iter().collect();
        for (k, v) in &attrs {
            if k.starts_with("expansion_") || k.starts_with("vpin_") {
                out.push_str(&format!("    // synthesized: {k} = {v}\n"));
            }
        }
        // Ctor args: the longest prefix of declared params whose
        // exported attribute carries a resolved value; the remainder
        // must all have defaults (else round-trip may differ — said).
        let (args, warn) = match ctors.get(&ty) {
            None => (String::new(), (!inst.attributes.is_empty()).then(|| "no ctor signature for this entity — emitted bare; round-trip unverified".to_string())),
            Some(c) => {
                let mut vals: Vec<String> = Vec::new();
                let mut stopped_at: Option<usize> = None;
                for (i, (_p, attr, _d)) in c.params.iter().enumerate() {
                    match attr.as_ref().and_then(|a| inst.attributes.get(a)) {
                        Some(v) => vals.push(v.clone()),
                        None => {
                            stopped_at = Some(i);
                            break;
                        }
                    }
                }
                let warn = match stopped_at {
                    Some(i) if c.params[i..].iter().any(|(_, _, d)| d.is_none()) => Some(format!(
                        "param '{}' has no exported attribute and no default — round-trip may differ",
                        c.params[i].0
                    )),
                    _ => None,
                };
                (vals.join(", "), warn)
            }
        };
        if let Some(w) = warn {
            out.push_str(&format!("    // WARNING: {w}\n"));
        }
        out.push_str(&format!("    {}: {}({});\n", inst.name, ty, args));
    }
    // ── connectivity: anchor arrows per net, sorted by net name ──
    let mut nets: Vec<_> = netlist
        .nets
        .iter()
        .filter_map(|(id, n)| n.name.clone().map(|nm| (nm, id)))
        .collect();
    nets.sort();
    for (name, net_id) in &nets {
        let mut pins: Vec<(String, String)> = netlist
            .pin_instances
            .values()
            .filter(|pi| pi.net == Some(*net_id))
            .filter_map(|pi| {
                let i = netlist.instances.get(pi.instance)?;
                let p = netlist.pins.get(pi.pin_def)?;
                Some((i.name.clone(), p.name.clone()))
            })
            .collect();
        pins.sort();
        // TODO(#85): power/ground nets emit @RAIL anchors; plain nets
        // use the first pin as anchor. Net-name preservation for
        // auto_* names is validated by the round-trip gate, not by
        // emitting explicit names.
        if let Some((a_i, a_p)) = pins.first().cloned() {
            for (b_i, b_p) in pins.iter().skip(1) {
                out.push_str(&format!("    {a_i}.{a_p} -> {b_i}.{b_p};  // net {name}\n"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_netlist::types::ModuleKind;

    /// Ctor reconstruction: derivable prefix emits literals; a trailing
    /// defaulted param is omitted; an underivable non-default param
    /// warns; an unknown entity emits bare with a warning.
    #[test]
    fn ctor_args_reconstruct_from_exported_attributes() {
        let mut n = Netlist::new();
        let m_res = n.add_module("Res".into(), ModuleKind::PhysicalComponent);
        let m_odd = n.add_module("Odd".into(), ModuleKind::PhysicalComponent);
        let r1 = n.add_instance("r1".into(), m_res).unwrap();
        n.instances[r1].attributes.insert("resistance".into(), "10kΩ".into());
        n.instances[r1].attributes.insert("tolerance".into(), "1%".into());
        let r2 = n.add_instance("r2".into(), m_res).unwrap();
        n.instances[r2].attributes.insert("resistance".into(), "470Ω".into());
        // r2 lacks tolerance → trailing default omitted, no warning
        let o1 = n.add_instance("o1".into(), m_odd).unwrap();
        n.instances[o1].attributes.insert("expansion_origin".into(), "vpin V5".into());
        let mut ctors = HashMap::new();
        ctors.insert("Res".into(), EntityCtor {
            params: vec![
                ("value".into(), Some("resistance".into()), None),
                ("tolerance".into(), Some("tolerance".into()), Some("5%".into())),
            ],
        });
        ctors.insert("Odd".into(), EntityCtor {
            params: vec![("mystery".into(), None, None)],
        });
        let out = emit_elaborated(&n, "test.bhdl", &ctors);
        assert!(out.contains("r1: Res(10kΩ, 1%);"), "{out}");
        assert!(out.contains("r2: Res(470Ω);"), "{out}");
        assert!(!out.contains("r2: Res(470Ω);\n    // WARNING"), "{out}");
        assert!(out.contains("// WARNING: param 'mystery' has no exported attribute and no default"), "{out}");
        assert!(out.contains("// synthesized: expansion_origin = vpin V5"), "{out}");
        assert!(out.contains("o1: Odd();"), "{out}");
    }
}

/// Extract every entity's ctor signature from parsed sources (main
/// file + transitively imported files — the caller supplies all of
/// them). The param→exported-attribute mapping is the stdlib
/// convention: an `attribute X = <param>;` whose value expression is
/// the BARE param name exports that param as attribute `X`.
pub fn extract_ctors(sources: &[&bhdl_ast::SourceFile]) -> HashMap<String, EntityCtor> {
    use bhdl_ast::{Entity, HasName};
    use rowan::ast::AstNode;
    let mut out: HashMap<String, EntityCtor> = HashMap::new();
    for sf in sources {
        for item in sf.items() {
            let Some(entity) = Entity::cast(item.syntax().clone()) else { continue };
            let Some(name) = entity.name().map(|t| t.text().to_string()) else { continue };
            let mut params: Vec<(String, Option<String>, Option<String>)> = Vec::new();
            if let Some(pl) = entity.param_list() {
                for pd in pl.param_defs() {
                    let Some(pn) = pd.name().map(|t| t.text().to_string()) else { continue };
                    let default = pd
                        .default_value()
                        .map(|e| e.syntax().text().to_string().trim().to_string());
                    params.push((pn, None, default));
                }
            }
            for attr in entity.attributes() {
                let Some(an) = attr.name().map(|t| t.text().to_string()) else { continue };
                let Some(v) = attr.value().map(|e| e.syntax().text().to_string().trim().to_string()) else { continue };
                if let Some(slot) = params.iter_mut().find(|(pn, exp, _)| *pn == v && exp.is_none()) {
                    slot.1 = Some(an);
                }
            }
            out.insert(name, EntityCtor { params });
        }
    }
    out
}

#[cfg(test)]
mod extract_tests {
    use super::*;
    use bhdl_ast::{AstNode, SourceFile};

    #[test]
    fn ctors_extract_params_exports_and_defaults() {
        let src = r#"
entity Res(value: resistance, tolerance: percentage = 5%, wattage: power = 0.25W) {
    pin 1: signal inout;
    pin 2: signal inout;
    attribute resistance = value;
    attribute tolerance = tolerance;
    attribute power_rating = wattage;
}
"#;
        let pr = bhdl_parser::parse(src);
        assert!(pr.errors().is_empty(), "{:?}", pr.errors());
        let sf = SourceFile::cast(pr.syntax()).unwrap();
        let ctors = extract_ctors(&[&sf]);
        let c = ctors.get("Res").expect("Res ctor");
        assert_eq!(c.params.len(), 3);
        assert_eq!(c.params[0], ("value".into(), Some("resistance".into()), None));
        assert_eq!(c.params[1], ("tolerance".into(), Some("tolerance".into()), Some("5%".into())));
        assert_eq!(c.params[2], ("wattage".into(), Some("power_rating".into()), Some("0.25W".into())));
    }
}
