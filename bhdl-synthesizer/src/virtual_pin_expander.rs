//! Netlist-mutation helpers shared by the expansion subsystem.
//!
//! The hardcoded virtual-pin expander that once lived here is gone — entity
//! expansion is now driven entirely by declarative `expansion { }` blocks
//! (see `expansion_interpreter`). What remains are the low-level netlist
//! manipulation helpers (`find_or_create_module`, `create_instance`, …) that
//! the interpreter — and the capacitor sizers — build on.

use bhdl_netlist::{
    ConnectionPoint, InstanceId, ModuleId, ModuleKind, NetId, Netlist,
    PinDirection, PinInstanceId, PinType,
};


// ── Helpers ─────────────────────────────────────────────────────────────

/// Find the net that contains a given pin instance by scanning all nets' connection lists.
/// This is more reliable than `pin_instance.net` which can be stale after net merges.
pub(crate) fn find_net_for_pin_instance(netlist: &Netlist, pi_id: PinInstanceId) -> Option<NetId> {
    let target = ConnectionPoint::PinInstance(pi_id);
    for (net_id, net) in &netlist.nets {
        if net.connections.contains(&target) {
            return Some(net_id);
        }
    }
    // Fall back to pin_instance.net if the connection list doesn't have it
    // (this can happen if the pin was connected via the PinInstance path in connect())
    netlist.pin_instances.get(pi_id)
        .and_then(|pi| pi.net)
        .filter(|nid| netlist.nets.contains_key(*nid))
}

/// Find an existing module definition by name, or create one with the given pins.
/// `pins` is a slice of (name, is_passive): passive pins get InOut/Passive,
/// non-passive get In/Signal or Out/Signal based on name convention (A=In, K=Out).
pub(crate) fn find_or_create_module(
    netlist: &mut Netlist,
    name: &str,
    pins: &[(&str, bool)],
) -> ModuleId {
    // Check if a *physical-component* module of this name already exists.
    // The match must be limited to PhysicalComponent — an imported entity
    // module of the same name (kind = Module, a logical definition) would
    // otherwise be reused, and the converter silently skips Module-kind
    // instances, so the expansion child would never reach the SPICE circuit.
    let required_pin_names: Vec<&str> = pins.iter().map(|(n, _)| *n).collect();
    'outer: for (mod_id, mod_def) in &netlist.modules {
        if mod_def.name == name && mod_def.kind == ModuleKind::PhysicalComponent {
            // Verify pin names match — a stdlib "Ind" with "1"/"2" must not
            // be reused when the caller needs "IN"/"OUT".
            if mod_def.pins.len() == pins.len() {
                for (i, &pin_id) in mod_def.pins.iter().enumerate() {
                    if let Some(pin) = netlist.pins.get(pin_id) {
                        if pin.name != required_pin_names[i] {
                            continue 'outer;
                        }
                    }
                }
                return mod_id;
            }
        }
    }

    // Create new module
    let mod_id = netlist.add_module(name.to_string(), ModuleKind::PhysicalComponent);

    for &(pin_name, is_passive) in pins {
        let (dir, ptype) = if is_passive {
            (PinDirection::InOut, PinType::Passive)
        } else {
            match pin_name {
                "A" | "IN" | "VIN" => (PinDirection::In, PinType::Signal),
                "K" | "OUT" | "VOUT" => (PinDirection::Out, PinType::Signal),
                _ => (PinDirection::InOut, PinType::Signal),
            }
        };
        netlist.add_pin(mod_id, pin_name.to_string(), dir, ptype);
    }

    mod_id
}

/// Create a component instance with the given attributes.
pub(crate) fn create_instance(
    netlist: &mut Netlist,
    name: &str,
    module_id: ModuleId,
    attrs: &[(&str, &str)],
) -> InstanceId {
    let inst_id = netlist.instances.insert(bhdl_netlist::Instance {
        name: name.to_string(),
        definition: module_id,
        attributes: attrs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        layout_intents: Vec::new(),
    });
    inst_id
}

/// Disconnect a pin instance from a specific net (remove from net's connection list,
/// clear the pin instance's net reference).
pub(crate) fn disconnect_pin_from_net(netlist: &mut Netlist, pi_id: PinInstanceId, net_id: NetId) {
    // Remove from net's connections
    if let Some(net) = netlist.nets.get_mut(net_id) {
        net.connections.retain(|c| *c != ConnectionPoint::PinInstance(pi_id));
    }
    // Clear pin instance's net ref
    if let Some(pi) = netlist.pin_instances.get_mut(pi_id) {
        if pi.net == Some(net_id) {
            pi.net = None;
        }
    }
}

/// Connect a named pin of an instance to a net.
pub(crate) fn connect_pin_instance_by_name(
    netlist: &mut Netlist,
    inst_id: InstanceId,
    pin_instances: &[PinInstanceId],
    pin_name: &str,
    net_id: NetId,
) -> Result<(), String> {
    // Find the pin instance whose pin_def has the matching name
    for &pi_id in pin_instances {
        let pi = netlist.pin_instances.get(pi_id)
            .ok_or_else(|| format!("pin instance {:?} not found", pi_id))?;
        let pin = netlist.pins.get(pi.pin_def)
            .ok_or_else(|| format!("pin def {:?} not found", pi.pin_def))?;
        if pin.name == pin_name {
            return netlist.connect(net_id, ConnectionPoint::PinInstance(pi_id));
        }
    }
    Err(format!("pin '{}' not found on instance {:?}", pin_name, inst_id))
}

/// Format a capacitance value (in farads) as a human-readable string for attributes.
pub(crate) fn format_cap_value_for_attr(farads: f64) -> String {
    if farads >= 1e-3 {
        format!("{:.0}mF", farads * 1e3)
    } else if farads >= 1e-6 {
        let uf = farads * 1e6;
        if (uf - uf.round()).abs() < 0.05 {
            format!("{:.0}µF", uf)
        } else {
            format!("{:.1}µF", uf)
        }
    } else if farads >= 1e-9 {
        format!("{:.0}nF", farads * 1e9)
    } else {
        format!("{:.0}pF", farads * 1e12)
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

