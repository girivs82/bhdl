//! Power-tree load harvesting — the input side of power-tree design.
//!
//! Pipeline stance (settled with the user): the power tree is NOT a
//! language abstraction. The board is built FUNCTION-FIRST — parts
//! instantiated, signals wired, rails declared but undriven (ERC028's
//! findings are the power-design worklist). This module harvests the
//! LOADS from that partial board: every instantiated entity's `domain`
//! contract (v, i_nom/i_max, noise target) plus every Power-class
//! rail's declared budget and driven/undriven status. The option
//! calculator consumes this; the designer picks a tree; bhdl is
//! generated with generic placeholder regulators whose parametric
//! contract matches the real parts', so committing a part is a rename.
//!
//! A "stub board" of nothing but load declarations is the degenerate
//! case of the same harvest — useful for architecture/thermal
//! planning, never a gate.

use crate::safety_model::entity_domain_map;
use bhdl_ast::SourceFile;
use bhdl_netlist::types::{NetClass, PinDirection};
use bhdl_netlist::Netlist;
use rowan::ast::AstNode;
use serde::{Deserialize, Serialize};

/// One load: an instantiated entity's power-domain contract, resolved
/// to the rail net it actually hangs on (None = pins not wired yet).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RailLoad {
    pub instance: String,
    pub entity: String,
    pub domain: String,
    pub v_nom: f64,
    pub tol_pct: Option<f64>,
    pub i_nom_a: Option<f64>,
    pub i_max_a: Option<f64>,
    /// Rail noise target (µVrms) from the domain contract.
    pub noise_uvrms: Option<f64>,
    pub net: Option<String>,
}

/// One Power-class rail with everything the tree calculator needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RailSummary {
    pub net: String,
    /// Declared rail voltage.
    pub voltage: f64,
    /// Declared load budget (`power X = V @ I`), when stated.
    pub declared_budget_a: Option<f64>,
    /// Sum of attached domain loads' i_nom / i_max (None when no
    /// attached load declares the figure — absent data stays absent).
    pub i_nom_total_a: Option<f64>,
    pub i_max_total_a: Option<f64>,
    /// Tightest attached noise target (µVrms) — the rail must meet
    /// its most sensitive load.
    pub noise_uvrms: Option<f64>,
    /// True when something on the board already generates this rail
    /// (regulator output pin, power-source class, power symbol).
    pub driven: bool,
    /// Instance.domain names of the attached loads.
    pub loads: Vec<String>,
}

/// The harvest: what the option calculator (and the designer) sees.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PowerTreeLoads {
    /// All domain loads, wired or not.
    pub loads: Vec<RailLoad>,
    /// Power-class rails, undriven ones being the worklist.
    pub rails: Vec<RailSummary>,
    /// Loads whose domain pins are not wired to any net yet — stated,
    /// not silently dropped.
    pub unwired: Vec<String>,
}

/// Harvest the loads and rails from a (possibly partial) board.
pub fn harvest_loads(netlist: &Netlist, sf: &SourceFile) -> PowerTreeLoads {
    let domains = entity_domain_map(&sf.syntax().clone());
    let mut out = PowerTreeLoads::default();

    // ── loads: every instance of an entity with domain contracts ──
    // (phantom definition stubs — instance named like its module with
    // zero connected pins — are template artifacts, same filter as
    // everywhere else)
    let connected: std::collections::HashSet<_> = netlist
        .pin_instances
        .values()
        .filter(|pi| pi.net.is_some())
        .map(|pi| pi.instance)
        .collect();
    for (inst_id, inst) in netlist.instances.iter() {
        let ety = netlist
            .modules
            .get(inst.definition)
            .map(|m| m.name.clone())
            .unwrap_or_default();
        if inst.name == ety && !connected.contains(&inst_id) {
            continue;
        }
        let Some((doms, _)) = domains.get(&ety) else { continue };
        for dom in doms {
            // resolve the rail net through the domain's first pin
            let net = dom.pins.first().and_then(|p0| {
                netlist.pin_instances.values().find_map(|pi| {
                    if pi.instance
                        != netlist
                            .instances
                            .iter()
                            .find(|(_, i)| i.name == inst.name)
                            .map(|(id, _)| id)?
                    {
                        return None;
                    }
                    let p = netlist.pins.get(pi.pin_def)?;
                    if p.name != *p0 {
                        return None;
                    }
                    netlist.nets.get(pi.net?)?.name.clone()
                })
            });
            if net.is_none() {
                out.unwired.push(format!("{}.{}", inst.name, dom.name));
            }
            out.loads.push(RailLoad {
                instance: inst.name.clone(),
                entity: ety.clone(),
                domain: dom.name.clone(),
                v_nom: dom.v_nom,
                tol_pct: dom.tol_pct,
                i_nom_a: dom.i_nom_a,
                i_max_a: dom.i_max_a,
                noise_uvrms: dom.noise_uvrms,
                net,
            });
        }
    }

    // ── rails: every Power-class net ──
    for (net_id, net) in netlist.nets.iter() {
        let NetClass::Power { voltage, current } = net.net_class else { continue };
        let Some(name) = net.name.clone() else { continue };
        if name.contains('.') {
            continue; // pin-derived internal rail, not a board rail
        }
        // Driven: same heuristic ERC028 uses — an output pin, a
        // power-source-class part, or a power-symbol (+5V) module.
        let driven = netlist.pin_instances.values().any(|pi| {
            if pi.net != Some(net_id) {
                return false;
            }
            let Some(pin) = netlist.pins.get(pi.pin_def) else { return false };
            if matches!(pin.direction, PinDirection::Out) {
                return true;
            }
            netlist
                .instances
                .get(pi.instance)
                .map(|i| {
                    i.attributes
                        .get("component_class")
                        .map(|c| matches!(c.as_str(), "power_source" | "battery"))
                        .unwrap_or(false)
                        || netlist
                            .modules
                            .get(i.definition)
                            .map(|m| m.name.starts_with('+'))
                            .unwrap_or(false)
                })
                .unwrap_or(false)
        });
        let attached: Vec<&RailLoad> = out
            .loads
            .iter()
            .filter(|l| l.net.as_deref() == Some(name.as_str()))
            .collect();
        let sum = |f: fn(&RailLoad) -> Option<f64>| -> Option<f64> {
            let vals: Vec<f64> = attached.iter().filter_map(|l| f(l)).collect();
            if vals.is_empty() { None } else { Some(vals.iter().sum()) }
        };
        out.rails.push(RailSummary {
            net: name,
            voltage,
            declared_budget_a: current,
            i_nom_total_a: sum(|l| l.i_nom_a),
            i_max_total_a: sum(|l| l.i_max_a),
            noise_uvrms: attached
                .iter()
                .filter_map(|l| l.noise_uvrms)
                .min_by(|a, b| a.partial_cmp(b).unwrap()),
            driven,
            loads: attached
                .iter()
                .map(|l| format!("{}.{}", l.instance, l.domain))
                .collect(),
        });
    }
    out.rails.sort_by(|a, b| a.net.cmp(&b.net));
    out.loads.sort_by(|a, b| (a.instance.clone(), a.domain.clone()).cmp(&(b.instance.clone(), b.domain.clone())));
    out
}
