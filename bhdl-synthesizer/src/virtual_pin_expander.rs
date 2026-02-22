//! Virtual Pin Expansion — post-synthesis pass
//!
//! After the main synthesis builds the netlist, this pass scans for instances
//! whose module has a virtual VOUT pin (identified by `component_class =
//! "switching_regulator"`) and expands them into concrete inductor, catch
//! diode, and output capacitor instances wired to the correct nets.
//!
//! The expansion creates an internal SW (switch-node) net and rewires the
//! buck IC's VOUT pin to it, inserting:
//!   - Inductor: SW → original VOUT net
//!   - Diode (optional): GND → SW (catch / freewheeling)
//!   - Output cap: VOUT → GND
//!
//! GLACIER DC simulation then processes the expanded netlist normally:
//! the buck IC becomes a VoltageRegulator decomposition with
//! VoltageSource(SW→GND) + dropout(VIN→SW), the inductor is a DC short,
//! and the diode/cap have no DC contribution.

use std::collections::HashMap;
use log::{debug, info};
use bhdl_netlist::{
    ConnectionPoint, InstanceId, ModuleId, ModuleKind, NetId, Netlist,
    PinDirection, PinInstanceId, PinType,
};

/// Summary of one virtual-pin expansion.
#[derive(Debug)]
pub struct ExpansionResult {
    pub regulator_name: String,
    pub inductor_name: String,
    pub diode_name: Option<String>,
    pub output_cap_name: String,
    /// Name of the switching node net, e.g. "buck_SW" or "buck_LX"
    pub sw_net_name: String,
    /// Switching node pin name from library (e.g. "SW", "LX", "PH")
    pub sw_pin_name: String,
}

/// Expand all virtual pins in the netlist.
///
/// Call this **after** synthesis but **before** GLACIER simulation.
pub fn expand_virtual_pins(netlist: &mut Netlist) -> Vec<ExpansionResult> {
    // Phase 1 — identify candidates (immutable scan)
    let candidates = find_candidates(netlist);
    if candidates.is_empty() {
        return Vec::new();
    }

    info!("Virtual pin expansion: {} candidate(s) found", candidates.len());

    // Build ref-des counters from existing instances to avoid name collisions.
    // Scans all instance names for patterns like "L3", "D1", "C12" and seeds
    // counters so new instances get the next available number.
    let mut refdes_counters = RefDesCounters::from_netlist(netlist);

    let mut results = Vec::new();

    for cand in candidates {
        match expand_one(netlist, &cand, &mut refdes_counters) {
            Ok(result) => {
                info!("Expanded virtual pin for {} → L={}, D={:?}, COUT={}",
                      result.regulator_name, result.inductor_name,
                      result.diode_name, result.output_cap_name);
                results.push(result);
            }
            Err(e) => {
                log::warn!("Virtual pin expansion failed for {}: {}", cand.instance_name, e);
            }
        }
    }

    results
}

// ── Reference designator generation ─────────────────────────────────────

/// Tracks per-prefix counters to generate unique reference designators
/// (L1, L2, D1, C1, ...) consistent with the hierarchical ref-des system.
struct RefDesCounters {
    counters: HashMap<String, usize>,
}

impl RefDesCounters {
    /// Scan existing instance names for ref-des patterns and seed counters.
    fn from_netlist(netlist: &Netlist) -> Self {
        let mut counters: HashMap<String, usize> = HashMap::new();
        for (_, inst) in &netlist.instances {
            // Match names like "L3", "D1", "C12", or suffixed like "buck_L1"
            if let Some((prefix, num)) = parse_refdes(&inst.name) {
                let entry = counters.entry(prefix).or_insert(0);
                *entry = (*entry).max(num);
            }
        }
        Self { counters }
    }

    /// Allocate the next reference designator for a given prefix (e.g. "L" → "L1", "L2", ...).
    fn next(&mut self, prefix: &str) -> String {
        let count = self.counters.entry(prefix.to_string()).or_insert(0);
        *count += 1;
        format!("{}{}", prefix, count)
    }
}

/// Extract (prefix, number) from a reference designator string.
/// Handles both bare "L3" and suffixed "buck_L1" patterns.
fn parse_refdes(name: &str) -> Option<(String, usize)> {
    // Try the last segment after '_' first (for "buck_L1" → "L1")
    let segment = name.rsplit('_').next().unwrap_or(name);
    // Find where the trailing digits start
    let digit_start = segment.rfind(|c: char| !c.is_ascii_digit())?;
    let prefix = &segment[..=digit_start];
    let num_str = &segment[digit_start + 1..];
    if prefix.is_empty() || num_str.is_empty() {
        return None;
    }
    // Only match single-letter prefixes (L, D, C, R, U, etc.)
    if prefix.len() == 1 && prefix.chars().next()?.is_ascii_uppercase() {
        num_str.parse::<usize>().ok().map(|n| (prefix.to_string(), n))
    } else {
        None
    }
}

// ── Candidate discovery ─────────────────────────────────────────────────

struct Candidate {
    instance_id: InstanceId,
    instance_name: String,
    vout_pin_inst: PinInstanceId,
    vout_net: NetId,
    gnd_net: NetId,
    // Expansion attributes (read from instance)
    sw_name: String,
    inductor_value: String,
    diode_vf: String,
    cout_value: String,
    has_diode: bool,
}

fn find_candidates(netlist: &Netlist) -> Vec<Candidate> {
    let mut candidates = Vec::new();

    for (inst_id, inst) in &netlist.instances {
        // Only switching regulators participate
        let class = inst.attributes.get("component_class");
        if class.map(|c| c.as_str()) != Some("switching_regulator") {
            continue;
        }

        // Read expansion attributes (with defaults)
        let sw_name = inst.attributes.get("vpin_sw_name")
            .cloned().unwrap_or_else(|| "SW".to_string());
        let inductor_value = inst.attributes.get("vpin_inductor")
            .cloned().unwrap_or_else(|| "33µH".to_string());
        let diode_vf = inst.attributes.get("vpin_diode_vf")
            .cloned().unwrap_or_else(|| "0.5V".to_string());
        let cout_value = inst.attributes.get("vpin_cout")
            .cloned().unwrap_or_else(|| "470µF".to_string());
        let has_diode = inst.attributes.get("vpin_has_diode")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);

        // Locate VOUT and GND pin instances
        let module_def = match netlist.modules.get(inst.definition) {
            Some(d) => d,
            None => continue,
        };

        let mut vout_pin_inst: Option<PinInstanceId> = None;
        let mut vout_net: Option<NetId> = None;
        let mut gnd_net: Option<NetId> = None;

        for &pin_id in &module_def.pins {
            let pin = match netlist.pins.get(pin_id) {
                Some(p) => p,
                None => continue,
            };

            // Find pin instance for this (instance, pin_def) pair
            let pi_id = netlist.pin_instances.iter()
                .find(|(_, pi)| pi.instance == inst_id && pi.pin_def == pin_id)
                .map(|(id, _)| id);

            let pi_id = match pi_id {
                Some(id) => id,
                None => continue,
            };

            let pi_net = netlist.pin_instances.get(pi_id).and_then(|pi| pi.net);

            match (pin.pin_type, pin.direction) {
                (PinType::Power, PinDirection::Out) | (PinType::Signal, PinDirection::Out) => {
                    // VOUT — power out or signal out on a regulator
                    if pin.name.to_uppercase().contains("OUT") || pin.name == "VO" || pin.name == "VOUT" {
                        vout_pin_inst = Some(pi_id);
                        vout_net = pi_net;
                    }
                }
                (PinType::Ground, _) | (_, PinDirection::Ground) => {
                    gnd_net = pi_net;
                }
                _ => {}
            }
        }

        // Need both VOUT and GND nets to proceed
        let (vout_pi, vout_nid, gnd_nid) = match (vout_pin_inst, vout_net, gnd_net) {
            (Some(pi), Some(v), Some(g)) => (pi, v, g),
            _ => {
                debug!("Skipping {} — could not identify VOUT/GND nets", inst.name);
                continue;
            }
        };

        candidates.push(Candidate {
            instance_id: inst_id,
            instance_name: inst.name.clone(),
            vout_pin_inst: vout_pi,
            vout_net: vout_nid,
            gnd_net: gnd_nid,
            sw_name,
            inductor_value,
            diode_vf,
            cout_value,
            has_diode,
        });
    }

    candidates
}

// ── Single-instance expansion ───────────────────────────────────────────

fn expand_one(netlist: &mut Netlist, cand: &Candidate, refdes: &mut RefDesCounters) -> Result<ExpansionResult, String> {
    let base = &cand.instance_name; // e.g. "buck"

    // Re-resolve nets by scanning all nets for ones that contain our pin instances.
    // The pin_instance.net field can be stale after net merges, but the net's
    // connection list is authoritative.
    let vout_net = find_net_for_pin_instance(netlist, cand.vout_pin_inst)
        .ok_or_else(|| "VOUT pin instance not connected to any net".to_string())?;

    let gnd_net = {
        let inst = netlist.instances.get(cand.instance_id)
            .ok_or("instance not found")?;
        let module_def = netlist.modules.get(inst.definition)
            .ok_or("module def not found")?;
        let mut found_gnd = None;
        for &pin_id in &module_def.pins {
            if let Some(pin) = netlist.pins.get(pin_id) {
                if pin.pin_type == PinType::Ground || pin.direction == PinDirection::Ground {
                    let pi_id = netlist.pin_instances.iter()
                        .find(|(_, pi)| pi.instance == cand.instance_id && pi.pin_def == pin_id)
                        .map(|(id, _)| id);
                    if let Some(pi_id) = pi_id {
                        found_gnd = find_net_for_pin_instance(netlist, pi_id);
                    }
                }
            }
        }
        found_gnd.ok_or_else(|| "GND pin has no net".to_string())?
    };

    debug!("Expanding {} — VOUT net: {:?}, GND net: {:?}", base, vout_net, gnd_net);

    // 1. Create internal switching-node net (name from library, e.g. "SW", "LX", "PH")
    let sw_net_name = format!("{}_{}", base, cand.sw_name);
    let sw_net = netlist.add_net(Some(sw_net_name.clone()));

    // 2. Rewire: disconnect buck's VOUT pin instance from user VOUT net,
    //    reconnect it to the SW net.
    disconnect_pin_from_net(netlist, cand.vout_pin_inst, vout_net);
    netlist.connect(sw_net, ConnectionPoint::PinInstance(cand.vout_pin_inst))
        .map_err(|e| format!("connect VOUT→SW: {}", e))?;

    // Record display name override: the viewer should show the switching node name
    // (e.g. "SW", "LX", "PH") instead of "VOUT"
    if let Some(inst) = netlist.instances.get_mut(cand.instance_id) {
        inst.attributes.insert("vpin_display_VOUT".to_string(), cand.sw_name.clone());
    }

    // 3. Create module defs (reuse if already present) and instances
    // Inductor pins: "IN" (from SW) and "OUT" (to VOUT) with directional types,
    // so the schematic extractor assigns correct left/right port placement.
    let ind_mod = find_or_create_module(netlist, "Ind", &[("IN", false), ("OUT", false)]);
    let cap_mod = find_or_create_module(netlist, "Cap", &[("1", true), ("2", true)]);

    // --- Inductor: pin 1 → SW, pin 2 → VOUT ---
    let ind_name = refdes.next("L");
    let ind_id = create_instance(netlist, &ind_name, ind_mod, &[
        ("component_class", "inductor"),
        ("value", &cand.inductor_value),
        ("vpin_parent", base),
        ("vpin_role", "series"),
    ]);
    let ind_pins = netlist.create_pin_instances(ind_id)
        .map_err(|e| format!("create inductor pins: {}", e))?;
    // IN → SW net
    connect_pin_instance_by_name(netlist, ind_id, &ind_pins, "IN", sw_net)?;
    // OUT → original VOUT net
    connect_pin_instance_by_name(netlist, ind_id, &ind_pins, "OUT", vout_net)?;

    // --- Diode (optional): A → GND, K → SW ---
    let diode_name = if cand.has_diode {
        let diode_mod = find_or_create_module(netlist, "Diode", &[("A", false), ("K", false)]);
        let d_name = refdes.next("D");
        let d_id = create_instance(netlist, &d_name, diode_mod, &[
            ("component_class", "diode"),
            ("forward_voltage", &cand.diode_vf),
            ("vpin_parent", base),
            ("vpin_role", "shunt"),
        ]);
        let d_pins = netlist.create_pin_instances(d_id)
            .map_err(|e| format!("create diode pins: {}", e))?;
        connect_pin_instance_by_name(netlist, d_id, &d_pins, "A", gnd_net)?;
        connect_pin_instance_by_name(netlist, d_id, &d_pins, "K", sw_net)?;
        Some(d_name)
    } else {
        None
    };

    // --- Output cap: pin 1 → VOUT, pin 2 → GND ---
    let cout_name = refdes.next("C");
    let cout_id = create_instance(netlist, &cout_name, cap_mod, &[
        ("component_class", "capacitor"),
        ("value", &cand.cout_value),
        ("vpin_parent", base),
        ("vpin_role", "shunt"),
    ]);
    let cout_pins = netlist.create_pin_instances(cout_id)
        .map_err(|e| format!("create cout pins: {}", e))?;
    connect_pin_instance_by_name(netlist, cout_id, &cout_pins, "1", vout_net)?;
    connect_pin_instance_by_name(netlist, cout_id, &cout_pins, "2", gnd_net)?;

    Ok(ExpansionResult {
        regulator_name: base.clone(),
        inductor_name: ind_name,
        diode_name,
        output_cap_name: cout_name,
        sw_net_name,
        sw_pin_name: cand.sw_name.clone(),
    })
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Find the net that contains a given pin instance by scanning all nets' connection lists.
/// This is more reliable than `pin_instance.net` which can be stale after net merges.
fn find_net_for_pin_instance(netlist: &Netlist, pi_id: PinInstanceId) -> Option<NetId> {
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
fn find_or_create_module(
    netlist: &mut Netlist,
    name: &str,
    pins: &[(&str, bool)],
) -> ModuleId {
    // Check if module already exists with matching pin names
    let required_pin_names: Vec<&str> = pins.iter().map(|(n, _)| *n).collect();
    'outer: for (mod_id, mod_def) in &netlist.modules {
        if mod_def.name == name {
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
fn create_instance(
    netlist: &mut Netlist,
    name: &str,
    module_id: ModuleId,
    attrs: &[(&str, &str)],
) -> InstanceId {
    let inst_id = netlist.instances.insert(bhdl_netlist::Instance {
        name: name.to_string(),
        definition: module_id,
        attributes: attrs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
    });
    inst_id
}

/// Disconnect a pin instance from a specific net (remove from net's connection list,
/// clear the pin instance's net reference).
fn disconnect_pin_from_net(netlist: &mut Netlist, pi_id: PinInstanceId, net_id: NetId) {
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
fn connect_pin_instance_by_name(
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

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_netlist::{ModuleKind, NetClass};

    /// Build a minimal netlist with a switching regulator instance (VIN, VOUT, GND)
    /// connected to three nets, then run expand_virtual_pins and verify the result.
    fn make_buck_netlist() -> Netlist {
        let mut nl = Netlist::default();

        // Module def for BuckRegulator with 3 pins
        let buck_mod = nl.add_module("BuckRegulator".to_string(), ModuleKind::PhysicalComponent);
        nl.add_pin(buck_mod, "VIN".to_string(), PinDirection::In, PinType::Power);
        nl.add_pin(buck_mod, "VOUT".to_string(), PinDirection::Out, PinType::Power);
        nl.add_pin(buck_mod, "GND".to_string(), PinDirection::Ground, PinType::Ground);

        // Instance
        let mut attrs = HashMap::new();
        attrs.insert("component_class".to_string(), "switching_regulator".to_string());
        attrs.insert("output_voltage".to_string(), "5".to_string());
        attrs.insert("vpin_inductor".to_string(), "33µH".to_string());
        attrs.insert("vpin_cout".to_string(), "470µF".to_string());
        attrs.insert("vpin_has_diode".to_string(), "true".to_string());
        let buck_id = nl.instances.insert(bhdl_netlist::Instance {
            name: "buck".to_string(),
            definition: buck_mod,
            attributes: attrs,
        });

        // Create pin instances
        let pin_insts = nl.create_pin_instances(buck_id).unwrap();

        // Three nets
        let vin_net = nl.add_net_with_class(Some("VIN_12V".to_string()), NetClass::Power(12.0));
        let vout_net = nl.add_net_with_class(Some("V5".to_string()), NetClass::Power(5.0));
        let gnd_net = nl.add_net_with_class(Some("GND".to_string()), NetClass::Ground);

        // Connect pin instances to nets (VIN=0, VOUT=1, GND=2)
        nl.connect(vin_net, ConnectionPoint::PinInstance(pin_insts[0])).unwrap();
        nl.connect(vout_net, ConnectionPoint::PinInstance(pin_insts[1])).unwrap();
        nl.connect(gnd_net, ConnectionPoint::PinInstance(pin_insts[2])).unwrap();

        nl
    }

    #[test]
    fn test_expand_creates_inductor_diode_cap() {
        let mut nl = make_buck_netlist();

        let initial_instances = nl.instances.len();
        let initial_nets = nl.nets.len();

        let results = expand_virtual_pins(&mut nl);

        assert_eq!(results.len(), 1, "should expand exactly one regulator");
        let r = &results[0];
        assert_eq!(r.regulator_name, "buck");
        assert!(r.inductor_name.starts_with("L"), "inductor should get L ref-des, got {}", r.inductor_name);
        assert!(r.diode_name.as_ref().unwrap().starts_with("D"), "diode should get D ref-des");
        assert!(r.output_cap_name.starts_with("C"), "cap should get C ref-des, got {}", r.output_cap_name);
        assert!(r.sw_net_name.contains("SW"));

        // 3 new instances (L, D, C)
        assert_eq!(nl.instances.len(), initial_instances + 3);
        // 1 new net (SW)
        assert_eq!(nl.nets.len(), initial_nets + 1);

        // Verify SW net exists
        let sw_net = nl.nets.iter()
            .find(|(_, n)| n.name.as_deref() == Some("buck_SW"));
        assert!(sw_net.is_some(), "SW net should exist");

        // Verify buck's VOUT is now on SW net, not the original V5 net
        let buck_vout_pi = nl.pin_instances.iter()
            .find(|(_, pi)| {
                let pin = nl.pins.get(pi.pin_def);
                pin.map(|p| p.name == "VOUT").unwrap_or(false)
                    && nl.instances.get(pi.instance).map(|i| i.name == "buck").unwrap_or(false)
            });
        assert!(buck_vout_pi.is_some());
        let (_, pi) = buck_vout_pi.unwrap();
        let sw_net_id = sw_net.unwrap().0;
        assert_eq!(pi.net, Some(sw_net_id), "VOUT pin should now be on SW net");

        // Verify display name override on parent
        let buck_inst = nl.instances.iter()
            .find(|(_, i)| i.name == "buck").unwrap().1;
        assert_eq!(buck_inst.attributes.get("vpin_display_VOUT").map(|s| s.as_str()),
                   Some("SW"), "buck should have display name override for VOUT");

        // Verify expansion metadata on children (find by vpin_parent, not by name)
        let children: Vec<_> = nl.instances.iter()
            .filter(|(_, i)| i.attributes.get("vpin_parent").map(|s| s.as_str()) == Some("buck"))
            .collect();
        assert_eq!(children.len(), 3, "should have 3 expansion children");

        let inductor = children.iter().find(|(_, i)| i.attributes.get("vpin_role").map(|s| s.as_str()) == Some("series")).unwrap().1;
        assert_eq!(inductor.attributes.get("component_class").map(|s| s.as_str()), Some("inductor"));
        assert!(inductor.name.starts_with("L"));

        let shunts: Vec<_> = children.iter()
            .filter(|(_, i)| i.attributes.get("vpin_role").map(|s| s.as_str()) == Some("shunt"))
            .collect();
        assert_eq!(shunts.len(), 2, "should have 2 shunt children (diode + cap)");

        // Verify sw_pin_name in result
        assert_eq!(r.sw_pin_name, "SW");
    }

    #[test]
    fn test_no_diode_when_disabled() {
        let mut nl = make_buck_netlist();

        // Set has_diode = false
        for (_, inst) in &mut nl.instances {
            if inst.name == "buck" {
                inst.attributes.insert("vpin_has_diode".to_string(), "false".to_string());
            }
        }

        let results = expand_virtual_pins(&mut nl);
        assert_eq!(results.len(), 1);
        assert!(results[0].diode_name.is_none(), "diode should be skipped for synchronous buck");

        // Only 2 new instances (L, C — no D)
        // original: 1 (buck) → total should be 3
        assert_eq!(nl.instances.len(), 3);
    }

    #[test]
    fn test_no_expansion_for_linear_regulator() {
        let mut nl = Netlist::default();
        let lin_mod = nl.add_module("LinearRegulator".to_string(), ModuleKind::PhysicalComponent);
        nl.add_pin(lin_mod, "VI".to_string(), PinDirection::In, PinType::Power);
        nl.add_pin(lin_mod, "VO".to_string(), PinDirection::Out, PinType::Power);
        nl.add_pin(lin_mod, "GND".to_string(), PinDirection::Ground, PinType::Ground);

        let mut attrs = HashMap::new();
        attrs.insert("component_class".to_string(), "voltage_regulator".to_string());
        nl.instances.insert(bhdl_netlist::Instance {
            name: "reg".to_string(),
            definition: lin_mod,
            attributes: attrs,
        });

        let results = expand_virtual_pins(&mut nl);
        assert!(results.is_empty(), "linear regulators should not be expanded");
    }

    #[test]
    fn test_module_reuse() {
        let mut nl = Netlist::default();

        // Create two buck regulators
        let buck_mod = nl.add_module("BuckRegulator".to_string(), ModuleKind::PhysicalComponent);
        nl.add_pin(buck_mod, "VIN".to_string(), PinDirection::In, PinType::Power);
        nl.add_pin(buck_mod, "VOUT".to_string(), PinDirection::Out, PinType::Power);
        nl.add_pin(buck_mod, "GND".to_string(), PinDirection::Ground, PinType::Ground);

        for name in &["buck1", "buck2"] {
            let mut attrs = HashMap::new();
            attrs.insert("component_class".to_string(), "switching_regulator".to_string());
            attrs.insert("output_voltage".to_string(), "5".to_string());
            let inst_id = nl.instances.insert(bhdl_netlist::Instance {
                name: name.to_string(),
                definition: buck_mod,
                attributes: attrs,
            });
            let pins = nl.create_pin_instances(inst_id).unwrap();
            let vin = nl.add_net(Some(format!("{}_VIN", name)));
            let vout = nl.add_net(Some(format!("{}_VOUT", name)));
            let gnd = nl.add_net(Some("GND".to_string()));
            nl.connect(vin, ConnectionPoint::PinInstance(pins[0])).unwrap();
            nl.connect(vout, ConnectionPoint::PinInstance(pins[1])).unwrap();
            nl.connect(gnd, ConnectionPoint::PinInstance(pins[2])).unwrap();
        }

        let results = expand_virtual_pins(&mut nl);
        assert_eq!(results.len(), 2, "both regulators should expand");

        // Ind and Cap module defs should be created once and reused
        let ind_mods: Vec<_> = nl.modules.iter()
            .filter(|(_, m)| m.name == "Ind")
            .collect();
        assert_eq!(ind_mods.len(), 1, "Ind module should exist exactly once");

        // Verify expansion metadata on both sets of children (find by vpin_parent)
        for prefix in &["buck1", "buck2"] {
            let children: Vec<_> = nl.instances.iter()
                .filter(|(_, i)| i.attributes.get("vpin_parent").map(|s| s.as_str()) == Some(*prefix))
                .collect();
            // 3 children each: L, D, C
            assert_eq!(children.len(), 3, "{} should have 3 expansion children", prefix);

            let inductor = children.iter()
                .find(|(_, i)| i.attributes.get("vpin_role").map(|s| s.as_str()) == Some("series"))
                .unwrap().1;
            assert!(inductor.name.starts_with("L"), "inductor ref-des should start with L, got {}", inductor.name);

            let shunts: Vec<_> = children.iter()
                .filter(|(_, i)| i.attributes.get("vpin_role").map(|s| s.as_str()) == Some("shunt"))
                .collect();
            assert_eq!(shunts.len(), 2, "{} should have 2 shunt children", prefix);
        }

        // Verify unique ref-des: all L instances should have distinct names
        let l_names: Vec<_> = nl.instances.iter()
            .filter(|(_, i)| i.name.starts_with("L"))
            .map(|(_, i)| i.name.clone())
            .collect();
        assert_eq!(l_names.len(), 2);
        assert_ne!(l_names[0], l_names[1], "inductors should have distinct ref-des");
    }
}
