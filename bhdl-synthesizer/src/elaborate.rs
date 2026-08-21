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

use std::collections::BTreeMap;

use bhdl_netlist::netlist::Netlist;

/// Render the synthesized netlist as structural bhdl.
///
/// `source` names the original file (header provenance only).
/// UNFINISHED (task #85): instance-arg reconstruction and the
/// power-net classification emit are stubs — see TODOs.
pub fn emit_elaborated(netlist: &Netlist, source: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "// ELABORATED bhdl — generated from {source}; DO NOT EDIT.\n\
         // The round-trip gate re-synthesizes this file and requires the\n\
         // IDENTICAL netlist. Every synthesized element carries a\n\
         // provenance comment naming the intent that produced it.\n\n"
    ));
    // ── instances, sorted by name for stable diffs ──
    let mut insts: Vec<_> = netlist.instances.iter().map(|(_, i)| i).collect();
    insts.sort_by(|a, b| a.name.cmp(&b.name));
    for inst in &insts {
        let ty = netlist
            .modules
            .get(inst.definition)
            .map(|m| m.name.clone())
            .unwrap_or_default();
        // TODO(#85): reconstruct ctor args from module params via the
        // param→exported-attribute mapping; emit provenance comments
        // for expansion_/vpin_-tagged instances.
        let attrs: BTreeMap<&String, &String> = inst.attributes.iter().collect();
        let _ = attrs;
        out.push_str(&format!("    {}: {}(/* TODO args */);\n", inst.name, ty));
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
