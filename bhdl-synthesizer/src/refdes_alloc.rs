//! Reference-designator allocation — the ONE place refdes is minted.
//!
//! Handles vs refdes: an instance's `name` is the user-authored HANDLE
//! (`r_load`, `input_bulk_cap`, or a synthesized one like `U1_C_out`) —
//! the human namespace used in source wiring, net names, and log prose.
//! The REFDES (`R1`, `C3`, `U5`) is the fab namespace: BOM, silkscreen,
//! pick-and-place, schematic labels. This pass allocates a refdes for
//! every physical instance and stamps it as the `refdes` instance
//! attribute; every downstream consumer (schematic, BOM, sign-off,
//! freeze, ERC plugin summaries, PnR) READS that attribute and never
//! allocates its own numbering — two allocators is how a schematic's R1
//! and a BOM's R1 end up naming different physical parts.
//!
//! Stability: the handle → refdes mapping persists in a committed
//! sidecar (`<board>.bhdl.refdes`, a lockfile analogue) so a part keeps
//! its designator across runs, edits, and machines. Allocation walks
//! instances name-sorted — SlotMap iteration order is unstable — so NEW
//! handles number deterministically too.
//!
//! Called at pipeline phase 12.7 (after every synthesizer phase that
//! mints instances — expansion 4.5, entity-attribute stamping 4.6 —
//! and before DRC 13 so ERC plugin summaries carry real designators).
//! Callers that mint instances later (the CLI's input/output cap-bank
//! sizers) re-invoke it; already-stamped instances are left untouched,
//! so re-running is cheap and idempotent.

use std::path::Path;

use bhdl_common::refdes::RefDesLut;
use bhdl_common::sku::refdes_prefix_for_class;
use bhdl_netlist::{InstanceId, Netlist};
use log::{debug, info};

/// Allocate and stamp the `refdes` attribute on every physical instance
/// that doesn't have one yet. `lut_path` is the persistent sidecar; when
/// `None` (unit tests, in-memory flows) allocation still happens, just
/// without persistence.
pub fn assign_refdes(netlist: &mut Netlist, lut_path: Option<&Path>) {
    let mut lut = lut_path.map(RefDesLut::load).unwrap_or_default();
    lut.version = 1;

    // Name-sorted walk for deterministic numbering of new handles.
    let mut walk: Vec<(String, InstanceId)> = netlist
        .instances
        .iter()
        .map(|(id, inst)| (inst.name.clone(), id))
        .collect();
    walk.sort();

    let mut stamped = 0usize;
    for (handle, id) in walk {
        let Some(inst) = netlist.instances.get(id) else { continue };
        if inst.attributes.contains_key("refdes") {
            continue;
        }
        // Phantom definition-instances (module named after itself) are
        // synthesis bookkeeping, not parts — same exclusion the schematic
        // label pass uses.
        let is_phantom = netlist
            .modules
            .get(inst.definition)
            .map(|m| m.name == inst.name)
            .unwrap_or(false);
        if is_phantom {
            continue;
        }
        let class = inst
            .attributes
            .get("component_class")
            .or_else(|| {
                netlist
                    .modules
                    .get(inst.definition)
                    .and_then(|m| m.attributes.get("component_class"))
            })
            .map(String::as_str)
            .unwrap_or("");
        let refdes = lut.assign(refdes_prefix_for_class(class), &handle);
        if let Some(inst) = netlist.instances.get_mut(id) {
            inst.attributes.insert("refdes".to_string(), refdes.clone());
            debug!("refdes: '{}' → {}", handle, refdes);
            stamped += 1;
        }
    }

    if stamped > 0 {
        info!("refdes allocation: {} instance(s) stamped", stamped);
    }
    if let Some(path) = lut_path {
        if let Err(e) = lut.save(path) {
            log::warn!("refdes: failed to persist sidecar {}: {e}", path.display());
        }
    }
}

/// Display form for report tables: `handle (refdes)` when the two differ,
/// just the handle when they coincide or no refdes was stamped.
pub fn handle_refdes_label(netlist: &Netlist, handle: &str) -> String {
    let refdes = netlist
        .instances
        .iter()
        .find(|(_, i)| i.name == handle)
        .and_then(|(_, i)| i.attributes.get("refdes"));
    match refdes {
        Some(rd) if rd != handle => format!("{handle} ({rd})"),
        _ => handle.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_netlist::types::ModuleKind;

    #[test]
    fn stamps_refdes_and_is_idempotent() {
        let mut nl = Netlist::new();
        let m = nl.add_module("Res".into(), ModuleKind::Component);
        let a = nl.add_instance("r_load".into(), m).unwrap();
        let b = nl.add_instance("input_bulk_cap".into(), m).unwrap();
        nl.instances.get_mut(a).unwrap().attributes
            .insert("component_class".into(), "resistor".into());
        nl.instances.get_mut(b).unwrap().attributes
            .insert("component_class".into(), "capacitor".into());

        assign_refdes(&mut nl, None);
        let rd_a = nl.instances.get(a).unwrap().attributes["refdes"].clone();
        let rd_b = nl.instances.get(b).unwrap().attributes["refdes"].clone();
        assert_eq!(rd_a, "R1");
        assert_eq!(rd_b, "C1");

        // Re-running never renumbers.
        assign_refdes(&mut nl, None);
        assert_eq!(nl.instances.get(a).unwrap().attributes["refdes"], rd_a);
    }

    #[test]
    fn phantom_definition_instances_skipped() {
        let mut nl = Netlist::new();
        let m = nl.add_module("LM2596".into(), ModuleKind::Component);
        nl.add_instance("LM2596".into(), m).unwrap(); // phantom
        let real = nl.add_instance("u_buck".into(), m).unwrap();
        assign_refdes(&mut nl, None);
        let phantom = nl.instances.values().find(|i| i.name == "LM2596").unwrap();
        assert!(!phantom.attributes.contains_key("refdes"));
        assert!(nl.instances.get(real).unwrap().attributes.contains_key("refdes"));
    }

    #[test]
    fn label_shows_both_only_when_distinct() {
        let mut nl = Netlist::new();
        let m = nl.add_module("Res".into(), ModuleKind::Component);
        let a = nl.add_instance("r_load".into(), m).unwrap();
        nl.instances.get_mut(a).unwrap().attributes
            .insert("component_class".into(), "resistor".into());
        assign_refdes(&mut nl, None);
        assert_eq!(handle_refdes_label(&nl, "r_load"), "r_load (R1)");
        // A handle that IS its refdes doesn't print doubled.
        let b = nl.add_instance("R7".into(), m).unwrap();
        nl.instances.get_mut(b).unwrap().attributes
            .insert("refdes".into(), "R7".into());
        assert_eq!(handle_refdes_label(&nl, "R7"), "R7");
    }
}
