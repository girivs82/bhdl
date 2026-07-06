//! Sheet-tree partitioning — hierarchical boards as one sheet per expanded
//! entity instance plus a top-level sheet (docs/spec/Schematic_V4.md).
//!
//! The synthesizer's expansion interpreter stamps every child it mints with
//! `attributes["expansion_parent"] = <parent instance>` and names it
//! `{parent}_{child}`, so hierarchy recovery is a grouping pass, never a
//! name-parse. A child sheet holds the parent instance (the physical IC
//! belongs with its support parts) plus its children; the top sheet holds
//! everything else, with each expanded entity represented by a linked BLOCK
//! whose ports are the parent's own pins. No `expansion_parent` on the
//! board → no tree (the flat single-sheet render stands).

use std::collections::{HashMap, HashSet};

use bhdl_netlist::netlist::Netlist;
use bhdl_netlist::types::InstanceId;

/// One partitioned sheet: which instances live on it.
#[derive(Debug)]
pub struct SheetGroup {
    /// Parent instance name (child sheets) — None for the top sheet.
    pub parent: Option<String>,
    pub members: HashSet<InstanceId>,
}

/// Group the netlist's instances into sheets. Returns None when the board
/// has no expansion hierarchy.
pub fn partition_sheets(netlist: &Netlist) -> Option<Vec<SheetGroup>> {
    let mut by_parent: HashMap<String, HashSet<InstanceId>> = HashMap::new();
    for (id, inst) in &netlist.instances {
        if let Some(parent) = inst.attributes.get("expansion_parent") {
            by_parent.entry(parent.clone()).or_default().insert(id);
        }
    }
    if by_parent.is_empty() {
        return None;
    }

    // The parent instance itself joins its children's sheet.
    let mut parent_ids: HashMap<String, InstanceId> = HashMap::new();
    for (id, inst) in &netlist.instances {
        if by_parent.contains_key(&inst.name) {
            parent_ids.insert(inst.name.clone(), id);
        }
    }

    let mut groups: Vec<SheetGroup> = Vec::new();
    let mut claimed: HashSet<InstanceId> = HashSet::new();
    let mut parents: Vec<&String> = by_parent.keys().collect();
    parents.sort();
    for parent in parents {
        let mut members = by_parent.get(parent).cloned().unwrap_or_default();
        if let Some(&pid) = parent_ids.get(parent) {
            members.insert(pid);
        }
        claimed.extend(members.iter().copied());
        groups.push(SheetGroup { parent: Some(parent.clone()), members });
    }

    // Top sheet: everything unclaimed.
    let top: HashSet<InstanceId> = netlist
        .instances
        .iter()
        .filter(|(id, _)| !claimed.contains(id))
        .map(|(id, _)| id)
        .collect();
    groups.insert(0, SheetGroup { parent: None, members: top });
    Some(groups)
}

/// Build a drawable sub-netlist containing only `keep` instances: their
/// pin instances survive, every other instance (and its pin instances) is
/// removed, and net connection lists are pruned so member back-pointers
/// stay consistent. Nets are kept even when emptied — an empty net simply
/// never draws.
pub fn subset_netlist(netlist: &Netlist, keep: &HashSet<InstanceId>) -> Netlist {
    let mut out = netlist.clone();
    out.analysis_data = None;

    let drop_insts: Vec<InstanceId> = out
        .instances
        .iter()
        .filter(|(id, _)| !keep.contains(id))
        .map(|(id, _)| id)
        .collect();
    let drop_set: HashSet<InstanceId> = drop_insts.iter().copied().collect();

    let drop_pis: Vec<_> = out
        .pin_instances
        .iter()
        .filter(|(_, pi)| drop_set.contains(&pi.instance))
        .map(|(id, _)| id)
        .collect();
    for pi in &drop_pis {
        out.pin_instances.remove(*pi);
    }
    for id in drop_insts {
        out.instances.remove(id);
    }
    let dropped: HashSet<_> = drop_pis.into_iter().collect();
    for (_, net) in out.nets.iter_mut() {
        net.connections.retain(|cp| match cp {
            bhdl_netlist::types::ConnectionPoint::PinInstance(pi) => !dropped.contains(pi),
            _ => true,
        });
    }
    out
}

/// A linked block on the top sheet representing one expanded entity.
/// Ports are the group's BOUNDARY NETS — nets its members share with the
/// rest of the board (plus power rails, which are global by nature) —
/// never the entity's internal auto-nets, and never missing a rail that
/// the entity produces through a child part rather than a parent pin.
#[derive(Debug, Clone)]
pub struct BlockSpec {
    /// Parent instance name (label ink comes from the refdes map).
    pub inst: String,
    /// Entity/module name shown inside the block.
    pub part: String,
    /// Relative href of the child sheet this block opens.
    pub href: String,
    /// (parent pin name when one sits on the net — else "", net name,
    /// net-is-power, net-is-ground).
    pub ports: Vec<(String, String, bool, bool)>,
}

/// Build the top sheet's block specs from the FULL netlist.
pub fn block_specs(
    netlist: &Netlist,
    groups: &[SheetGroup],
    href_for: &dyn Fn(&str) -> String,
) -> Vec<BlockSpec> {
    let mut out = Vec::new();
    for g in groups {
        let Some(parent) = &g.parent else { continue };
        let Some((parent_id, inst)) =
            netlist.instances.iter().find(|(_, i)| &i.name == parent)
        else {
            continue;
        };
        let part = netlist
            .modules
            .get(inst.definition)
            .map(|m| m.name.clone())
            .unwrap_or_default();

        // Nets touched by the group, split into inside-only vs boundary.
        use std::collections::HashMap as Map;
        let mut touched: Map<bhdl_netlist::types::NetId, (bool, bool)> = Map::new(); // (inside, outside)
        for pi in netlist.pin_instances.values() {
            let Some(nid) = pi.net else { continue };
            let e = touched.entry(nid).or_insert((false, false));
            if g.members.contains(&pi.instance) {
                e.0 = true;
            } else {
                e.1 = true;
            }
        }

        let mut ports = Vec::new();
        for (nid, (inside, outside)) in &touched {
            if !inside {
                continue;
            }
            let Some(net) = netlist.nets.get(*nid) else { continue };
            let is_power = matches!(net.net_class, bhdl_netlist::types::NetClass::Power { .. });
            let is_ground = matches!(net.net_class, bhdl_netlist::types::NetClass::Ground);
            // Boundary = shared with the outside, or a global rail/ground.
            if !outside && !is_power && !is_ground {
                continue;
            }
            // Parent pin on this net, if any (nicest label).
            let pin = netlist
                .pin_instances
                .values()
                .find(|pi| pi.instance == parent_id && pi.net == Some(*nid))
                .and_then(|pi| netlist.pins.get(pi.pin_def))
                .filter(|p| !p.is_virtual)
                .map(|p| p.name.clone())
                .unwrap_or_default();
            ports.push((
                pin,
                net.name.clone().unwrap_or_default(),
                is_power,
                is_ground,
            ));
        }
        ports.sort_by(|a, b| (&a.1, &a.0).cmp(&(&b.1, &b.0)));
        ports.dedup_by(|a, b| a.1 == b.1);
        out.push(BlockSpec {
            inst: parent.clone(),
            part,
            href: href_for(parent),
            ports,
        });
    }
    out
}
