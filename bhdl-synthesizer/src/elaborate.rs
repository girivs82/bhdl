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
    emit_elaborated_with_preamble(netlist, source, ctors, "")
}

/// Like [`emit_elaborated`] but carries `preamble` — the original
/// file's non-board items (imports, entity/safety definitions)
/// verbatim, so the elaborated file re-synthesizes standalone. Build
/// it with [`extract_preamble`].
pub fn emit_elaborated_with_preamble(
    netlist: &Netlist,
    source: &str,
    ctors: &HashMap<String, EntityCtor>,
    preamble: &str,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "// ELABORATED bhdl — generated from {source}; DO NOT EDIT.\n\
         // The round-trip gate re-synthesizes this file and requires the\n\
         // IDENTICAL netlist. Every synthesized element carries a\n\
         // provenance comment naming the intent that produced it.\n\n"
    ));
    if !preamble.is_empty() {
        out.push_str(preamble.trim_end());
        out.push_str("\n\n");
    }
    // ── instances, sorted by name for stable diffs ──
    // ── board wrapper + power/ground declarations ──
    let board_name = netlist
        .top_level_module
        .and_then(|id| netlist.modules.get(id))
        .map(|m| m.name.clone())
        .unwrap_or_else(|| "Elaborated".to_string());
    out.push_str(&format!("board {board_name} {{\n"));
    let mut rails: Vec<String> = Vec::new();
    let mut grounds: Vec<String> = Vec::new();
    for (net_id, n) in netlist.nets.iter() {
        let Some(nm) = n.name.clone() else { continue };
        // Pin-derived internal rails ("u1.VCC" — synthesis mints one
        // per power-in pin) are not board declarations and their name
        // is not a legal identifier. They carry no members the rail
        // anchors don't already cover; emitting them would not parse.
        if nm.contains('.') {
            let members = netlist.pin_instances.values().any(|pi| pi.net == Some(net_id));
            debug_assert!(!members, "pin-derived rail {nm} unexpectedly has members");
            continue;
        }
        match n.net_class {
            bhdl_netlist::types::NetClass::Power { voltage, current } => {
                let amp = current.map(|c| format!(" @ {c}A")).unwrap_or_default();
                rails.push(format!("    power {nm} = {voltage}V{amp};"));
            }
            bhdl_netlist::types::NetClass::Ground => grounds.push(format!("    ground {nm};")),
            _ => {}
        }
    }
    rails.sort();
    grounds.sort();
    for l in rails.iter().chain(grounds.iter()) {
        out.push_str(l);
        out.push('\n');
    }
    out.push('\n');

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
                    // An empty attribute value means expansion never
                    // resolved it (seen on board-local entities whose
                    // param was left to its default) — treat as absent
                    // so the default-omission path applies instead of
                    // emitting a hole like `Ent(, 5%)`.
                    match attr
                        .as_ref()
                        .and_then(|a| inst.attributes.get(a))
                        .filter(|v| !v.is_empty())
                    {
                        Some(v) => vals.push(quote_if_not_bare(v)),
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
        // Power rails anchor every pin at @RAIL (so a single-pin rail
        // net never drops); ground pins attach to @GND; plain nets use
        // the first pin as anchor. Net-name preservation for auto_*
        // names is validated by the round-trip gate, not by emitting
        // explicit names.
        let class = netlist.nets.get(*net_id).map(|n| n.net_class.clone());
        match class {
            Some(bhdl_netlist::types::NetClass::Power { .. }) => {
                for (b_i, b_p) in &pins {
                    out.push_str(&format!("    @{name} -> {b_i}.{b_p};\n"));
                }
            }
            Some(bhdl_netlist::types::NetClass::Ground) => {
                for (b_i, b_p) in &pins {
                    out.push_str(&format!("    {b_i}.{b_p} -> @{name};\n"));
                }
            }
            _ => {
                if let Some((a_i, a_p)) = pins.first().cloned() {
                    for (b_i, b_p) in pins.iter().skip(1) {
                        out.push_str(&format!("    {a_i}.{a_p} -> {b_i}.{b_p};  // net {name}\n"));
                    }
                }
            }
        }
    }
    out.push_str("}\n");
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

    /// An EMPTY attribute value means expansion never resolved it
    /// (board-local entity left to its param default). It must count
    /// as absent — the whole prefix stops there and defaults carry the
    /// round-trip — never be emitted as a positional hole `Res(, 1%)`.
    #[test]
    fn empty_attribute_value_counts_as_unresolved() {
        let mut n = Netlist::new();
        let m = n.add_module("Res".into(), ModuleKind::PhysicalComponent);
        let r = n.add_instance("sense".into(), m).unwrap();
        n.instances[r].attributes.insert("resistance".into(), "".into());
        n.instances[r].attributes.insert("tolerance".into(), "1%".into());
        let mut ctors = HashMap::new();
        ctors.insert("Res".into(), EntityCtor {
            params: vec![
                ("value".into(), Some("resistance".into()), Some("10kΩ".into())),
                ("tolerance".into(), Some("tolerance".into()), Some("5%".into())),
            ],
        });
        let out = emit_elaborated(&n, "test.bhdl", &ctors);
        assert!(out.contains("sense: Res();"), "{out}");
        assert!(!out.contains("Res(,"), "{out}");
        assert!(!out.contains("WARNING"), "{out}");
    }
}

/// Attribute storage strips string quotes, so a reconstructed ctor
/// arg like `NCP1117-5.0_SOT223` would re-parse as IDENT MINUS NUMBER.
/// Re-quote anything that is not a legal bare atom: an identifier, or
/// a number-led value literal (10kΩ, 0.7V, 5%, 100nF…).
fn quote_if_not_bare(v: &str) -> String {
    let is_ident = {
        let mut ch = v.chars();
        matches!(ch.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
            && ch.all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    let is_value_literal = v.starts_with(|c: char| c.is_ascii_digit())
        && v.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | 'µ' | 'Ω' | '%'));
    if is_ident || is_value_literal {
        v.to_string()
    } else {
        format!("\"{}\"", v.replace('"', "\\\""))
    }
}

/// Round-trip equivalence: does re-synthesizing the elaborated file
/// yield THE SAME board? Compared structurally, not positionally —
/// instance identity is (name, module name), connectivity is the net
/// PARTITION over "inst.pin" endpoints plus each net's class
/// (auto-net NAMES legitimately differ between runs and never count).
/// Returns every difference, not just the first — a round-trip
/// failure is a bug report and should name all of it.
pub fn netlist_equiv(a: &Netlist, b: &Netlist) -> Result<(), Vec<String>> {
    use std::collections::BTreeMap;
    let mut diffs: Vec<String> = Vec::new();

    fn instance_set(n: &Netlist) -> std::collections::BTreeSet<(String, String)> {
        // Phantom definition-stubs (name == module name, zero connected
        // pins) are template artifacts the emitter filters — exclude
        // them here by the SAME rule, else every board with an in-file
        // entity definition fails its own round-trip.
        let connected: std::collections::HashSet<_> = n
            .pin_instances
            .values()
            .filter(|pi| pi.net.is_some())
            .map(|pi| pi.instance)
            .collect();
        n.instances
            .iter()
            .filter_map(|(id, i)| {
                let m = n.modules.get(i.definition).map(|m| m.name.clone()).unwrap_or_default();
                if i.name == m && !connected.contains(&id) {
                    return None;
                }
                Some((i.name.clone(), m))
            })
            .collect()
    }
    let (ia, ib) = (instance_set(a), instance_set(b));
    for (n, m) in ia.difference(&ib) {
        diffs.push(format!("instance only in ORIGINAL: {n}: {m}"));
    }
    for (n, m) in ib.difference(&ia) {
        diffs.push(format!("instance only in ELABORATED: {n}: {m}"));
    }

    // Net partition: each net → sorted endpoint list "inst.pin",
    // keyed by that list; value = a class tag that must also match.
    fn net_partition(n: &Netlist) -> BTreeMap<Vec<String>, String> {
        let mut m: BTreeMap<Vec<String>, String> = BTreeMap::new();
        for (net_id, net) in n.nets.iter() {
            let mut ends: Vec<String> = n
                .pin_instances
                .values()
                .filter(|pi| pi.net == Some(net_id))
                .filter_map(|pi| {
                    let inst = n.instances.get(pi.instance)?;
                    let pin = n.pins.get(pi.pin_def)?;
                    Some(format!("{}.{}", inst.name, pin.name))
                })
                .collect();
            ends.sort();
            if ends.is_empty() {
                continue; // memberless nets carry no connectivity
            }
            let class = match &net.net_class {
                bhdl_netlist::types::NetClass::Power { voltage, current } => {
                    format!("power {voltage}V {current:?}")
                }
                bhdl_netlist::types::NetClass::Ground => "ground".to_string(),
                _ => "signal".to_string(),
            };
            m.insert(ends, class);
        }
        m
    }
    let (na, nb) = (net_partition(a), net_partition(b));
    for (ends, class) in &na {
        match nb.get(ends) {
            None => diffs.push(format!("net only in ORIGINAL ({class}): {}", ends.join(", "))),
            Some(c2) if c2 != class => {
                diffs.push(format!("net class differs for {}: {class} vs {c2}", ends.join(", ")))
            }
            _ => {}
        }
    }
    for (ends, class) in &nb {
        if !na.contains_key(ends) {
            diffs.push(format!("net only in ELABORATED ({class}): {}", ends.join(", ")));
        }
    }

    if diffs.is_empty() { Ok(()) } else { Err(diffs) }
}

/// The original file's non-board content, verbatim: imports, entity
/// definitions, safety blocks — everything the elaborated board still
/// needs to re-synthesize standalone. Splices every BOARD_DEF span
/// out of the source text; nothing is reformatted, so definitions
/// keep their comments and the Real-Data provenance they carry.
pub fn extract_preamble(source_text: &str, sf: &bhdl_ast::SourceFile) -> String {
    use bhdl_ast::Board;
    use rowan::ast::AstNode;
    let mut cut: Vec<(usize, usize)> = sf
        .items()
        .filter_map(|it| Board::cast(it.syntax().clone()))
        .map(|b| {
            let r = b.syntax().text_range();
            (usize::from(r.start()), usize::from(r.end()))
        })
        .collect();
    cut.sort();
    let mut out = String::new();
    let mut pos = 0;
    for (s, e) in cut {
        out.push_str(&source_text[pos..s]);
        pos = e;
    }
    out.push_str(&source_text[pos..]);
    // collapse the hole's leftover blank runs at the seams
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    out.trim().to_string()
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
