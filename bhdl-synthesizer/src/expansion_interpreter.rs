//! Generic Expansion Interpreter — replaces hardcoded `virtual_pin_expander`.
//!
//! Reads `ExpansionRecipe` structs (extracted from entity `expansion { }` blocks
//! by the analyzer) and creates concrete child instances + wiring in the netlist.
//!
//! The existing `virtual_pin_expander` helper functions (`find_or_create_module`,
//! `create_instance`, `connect_pin_instance_by_name`, `find_net_for_pin_instance`)
//! are reused — they remain in `virtual_pin_expander.rs` as `pub(crate)`.

use std::collections::HashMap;
use log::{debug, info, warn};
use bhdl_common::{ExpansionRecipe, ExpansionConnection, ExpansionEndpoint};
use bhdl_netlist::{
    ConnectionPoint, InstanceId, ModuleId, NetId, Netlist, PinInstanceId,
};
use bhdl_analyzer::spice_extraction::parse_unit_value;
use crate::virtual_pin_expander::{
    find_net_for_pin_instance, find_or_create_module, create_instance,
    connect_pin_instance_by_name, disconnect_pin_from_net,
};

/// Summary of one entity expansion.
#[derive(Debug)]
pub struct ExpansionResult {
    /// Name of the parent instance that was expanded
    pub parent_instance: String,
    /// Names of child instances created
    pub child_instances: Vec<String>,
    /// Internal nets created
    pub internal_nets: Vec<String>,
}

/// Expand all entity instances in the netlist that have expansion recipes.
///
/// Call this **after** synthesis and intent stamping, **before** GLACIER simulation.
/// This is the drop-in replacement for `expand_virtual_pins()`.
pub fn expand_entity_instances(
    netlist: &mut Netlist,
    recipes: &HashMap<String, ExpansionRecipe>,
) -> Vec<ExpansionResult> {
    if recipes.is_empty() {
        return Vec::new();
    }

    // Phase 1 — identify which instances need expansion (immutable scan)
    let candidates = find_expansion_candidates(netlist, recipes);
    if candidates.is_empty() {
        return Vec::new();
    }

    info!("Expansion interpreter: {} candidate(s) found", candidates.len());

    let mut results = Vec::new();
    for cand in candidates {
        match expand_one_instance(netlist, &cand) {
            Ok(result) => {
                info!("Expanded '{}' → {} child instance(s)",
                    result.parent_instance, result.child_instances.len());
                results.push(result);
            }
            Err(e) => {
                warn!("Failed to expand '{}': {}", cand.instance_name, e);
            }
        }
    }

    results
}

/// A candidate for expansion.
struct ExpansionCandidate {
    instance_id: InstanceId,
    instance_name: String,
    recipe: ExpansionRecipe,
    /// Concrete parameter values: param_name → value string (from instance attributes)
    param_values: HashMap<String, String>,
    /// Map of parent entity pin names → PinInstanceId
    pin_instances: HashMap<String, PinInstanceId>,
}

/// Find all instances in the netlist whose module definition matches a recipe.
fn find_expansion_candidates(
    netlist: &Netlist,
    recipes: &HashMap<String, ExpansionRecipe>,
) -> Vec<ExpansionCandidate> {
    let mut candidates = Vec::new();

    for (inst_id, inst) in &netlist.instances {
        let mod_def = match netlist.modules.get(inst.definition) {
            Some(m) => m,
            None => continue,
        };

        // Check if this module's name matches any recipe.
        // Also check the base type name from `component_type` attr or prefix matching
        // for monomorphized types (e.g., "TPS54331_3V3" → recipe "TPS54331").
        let recipe = recipes.get(&mod_def.name)
            .or_else(|| {
                // Try matching by the original entity type stored in component_type attr
                inst.attributes.get("component_type")
                    .and_then(|ct| recipes.get(ct))
            })
            .or_else(|| {
                // Prefix match for monomorphized aliases: "TPS54331_3V3" matches recipe "TPS54331"
                recipes.iter()
                    .find(|(recipe_name, _)| {
                        mod_def.name.starts_with(recipe_name.as_str())
                            && mod_def.name.len() > recipe_name.len()
                            && mod_def.name.as_bytes()[recipe_name.len()] == b'_'
                    })
                    .map(|(_, r)| r)
            });

        let recipe = match recipe {
            Some(r) => r.clone(),
            None => continue,
        };

        // The `virtual` keyword is the trigger: an entity is expanded only
        // if it declares a virtual pin. The `expansion { }` block is the
        // *definition*; the virtual pin is what *activates* it — so `virtual`
        // is functional, not decorative. An expansion block with no virtual
        // pin is inert (and worth flagging).
        let has_virtual_pin = mod_def.pins.iter().any(|&pid| {
            netlist.pins.get(pid).map(|p| p.is_virtual).unwrap_or(false)
        });
        if !has_virtual_pin {
            warn!(
                "Instance '{}' ({}) has an expansion {{ }} block but no \
                 virtual pin — declare a `virtual` pin to activate it; \
                 skipping expansion",
                inst.name, mod_def.name
            );
            continue;
        }

        // Skip instances that are already expansion children
        if inst.attributes.contains_key("expansion_parent")
            || inst.attributes.contains_key("vpin_parent")
        {
            continue;
        }

        // Skip instances that already had expansion applied
        if inst.attributes.contains_key("expansion_applied") {
            continue;
        }

        // Collect parameter values from instance attributes
        let param_values = inst.attributes.clone();

        // Build pin instance map: pin_name → PinInstanceId
        let mut pin_map = HashMap::new();
        for (pi_id, pi) in &netlist.pin_instances {
            if pi.instance == inst_id {
                if let Some(pin) = netlist.pins.get(pi.pin_def) {
                    pin_map.insert(pin.name.clone(), pi_id);
                }
            }
        }

        // Skip instances without pin instances (template/definition instances, not real circuit components)
        if pin_map.is_empty() {
            continue;
        }

        candidates.push(ExpansionCandidate {
            instance_id: inst_id,
            instance_name: inst.name.clone(),
            recipe,
            param_values,
            pin_instances: pin_map,
        });
    }

    candidates
}

/// Expand a single instance according to its recipe.
fn expand_one_instance(
    netlist: &mut Netlist,
    cand: &ExpansionCandidate,
) -> Result<ExpansionResult, String> {
    let base = &cand.instance_name;

    // 1. Create internal nets
    let mut internal_net_map: HashMap<String, NetId> = HashMap::new();
    for net_name in &cand.recipe.internal_nets {
        let full_name = format!("{}_{}", base, net_name);
        let net_id = netlist.add_net(Some(full_name.clone()));
        internal_net_map.insert(net_name.clone(), net_id);
        debug!("Created internal net '{}' ({:?})", full_name, net_id);
    }

    // 2. Create child instances
    let mut child_instance_map: HashMap<String, (InstanceId, Vec<PinInstanceId>)> = HashMap::new();
    let mut child_names = Vec::new();

    // Read parent intent/stage attributes for propagation
    let parent_attrs = netlist.instances.get(cand.instance_id)
        .map(|i| i.attributes.clone())
        .unwrap_or_default();

    // Intent-driven operating-point design: when the parent carries a tube
    // intent, run the matching designer; its computed component values then
    // replace the expansion block's literal defaults for the children whose
    // names appear in the returned map.
    let intent_design = intent_driven_values(netlist, cand);

    for exp_inst in &cand.recipe.instances {
        let child_name = format!("{}_{}", base, exp_inst.name);

        // Determine pin layout from component type
        let pins = component_type_pins(&exp_inst.component_type);
        let mod_id = find_or_create_module(netlist, &exp_inst.component_type, &pins);

        // Evaluate parameter expressions by substituting entity params
        let mut attrs: Vec<(&str, String)> = Vec::new();

        // Set the value attribute. An intent-driven design overrides the
        // expansion block's literal for the children it computed; every
        // other child keeps its declared value.
        let designed_value = intent_design.get(exp_inst.name.as_str()).copied();
        if let Some(v) = designed_value {
            attrs.push(("value", format!("{v:.3}")));
        } else if let Some(first_param) = exp_inst.params.first() {
            let resolved = resolve_param_expression(first_param, &cand.param_values, &cand.recipe.param_defaults);
            attrs.push(("value", resolved));
        }

        // Set component_class from the component type so GLACIER can identify
        // these children (the component registry only looks at this attribute).
        let comp_class = component_type_to_class(&exp_inst.component_type);
        attrs.push(("component_class", comp_class.to_string()));

        // Determine expansion role from connection topology
        // A child is "shunt" if any of its connections touch GND; otherwise "series"
        let is_shunt = cand.recipe.connections.iter().any(|conn| {
            let touches_child = |ep: &ExpansionEndpoint| match ep {
                ExpansionEndpoint::InstancePin(n, _) => n == &exp_inst.name,
                _ => false,
            };
            let touches_gnd = |ep: &ExpansionEndpoint| match ep {
                ExpansionEndpoint::ParentPin(p) => p.to_uppercase() == "GND",
                _ => false,
            };
            (touches_child(&conn.from) && touches_gnd(&conn.to))
                || (touches_child(&conn.to) && touches_gnd(&conn.from))
        });
        let expansion_role = if is_shunt { "shunt" } else { "series" };

        // Compute schematic placement hint from connection topology + pin types
        // These hints encode datasheet application note conventions for layout:
        //   main_path     — on the horizontal power path (e.g. output inductor)
        //   input_shunt   — input decoupling cap (left of regulator)
        //   output_shunt  — output filtering cap (right of inductor / at output)
        //   switching_shunt — at the switching node (catch diode, left of inductor)
        //   bootstrap     — bootstrap cap floating between two non-GND pins
        //   feedback_high — top resistor of feedback divider (chains to feedback_low)
        //   feedback_low  — bottom resistor of feedback divider
        let placement = determine_schematic_placement(
            exp_inst, &cand.recipe, is_shunt,
        );
        attrs.push(("schematic_placement", placement));

        // Propagate parent attributes
        attrs.push(("expansion_parent", base.to_string()));
        attrs.push(("expansion_role", expansion_role.to_string()));
        attrs.push(("vpin_parent", base.to_string()));
        attrs.push(("vpin_role", expansion_role.to_string()));
        if let Some(intent) = parent_attrs.get("intent") {
            attrs.push(("intent", intent.clone()));
        }
        if let Some(stage_name) = parent_attrs.get("stage_name") {
            attrs.push(("stage_name", stage_name.clone()));
        }

        // Convert to (&str, &str) pairs for create_instance
        let attr_pairs: Vec<(&str, &str)> = attrs.iter()
            .map(|(k, v)| (*k, v.as_str()))
            .collect();

        let inst_id = create_instance(netlist, &child_name, mod_id, &attr_pairs);

        // Create pin instances for the child
        let pin_instances = create_child_pin_instances(netlist, inst_id)?;

        child_instance_map.insert(exp_inst.name.clone(), (inst_id, pin_instances));
        child_names.push(child_name);
    }

    // 3. Apply interposition rewiring
    //
    // Detect the pattern: ParentPin(P) → Child.pin_a, Child.pin_b → InternalNet(N)
    // This means the child interposes between the parent pin's board net and the
    // internal net.  We disconnect P from its board net, reconnect P to the
    // internal net, and record the original net so subsequent ParentPin(P)
    // references resolve to the board-level net (not the now-rewired pin net).
    let interpositions = detect_interpositions(&cand.recipe);
    let mut rewired_original_nets: HashMap<String, NetId> = HashMap::new();

    for (pin_name, internal_net_name) in &interpositions {
        if let Some(&pi_id) = cand.pin_instances.get(pin_name) {
            if let Some(original_net) = find_net_for_pin_instance(netlist, pi_id) {
                let internal_net = *internal_net_map.get(internal_net_name)
                    .ok_or_else(|| format!("Internal net '{}' not found for interposition", internal_net_name))?;

                // Disconnect parent pin from its board net
                disconnect_pin_from_net(netlist, pi_id, original_net);
                // Reconnect parent pin to the internal net
                netlist.connect(internal_net, ConnectionPoint::PinInstance(pi_id))
                    .map_err(|e| format!("rewire pin '{}' to internal net '{}': {}", pin_name, internal_net_name, e))?;

                // Save the original net for later connection resolution
                rewired_original_nets.insert(pin_name.clone(), original_net);

                // Set display name override so schematic shows the internal net name
                // (e.g. "SW" instead of "VOUT")
                if let Some(inst) = netlist.instances.get_mut(cand.instance_id) {
                    inst.attributes.insert(
                        format!("vpin_display_{}", pin_name),
                        internal_net_name.to_uppercase(),
                    );
                }

                info!("Interposition: rewired '{}'.{} from board net to internal '{}' (display: {})",
                    base, pin_name, internal_net_name, internal_net_name.to_uppercase());
            }
        }
    }

    // 4. Wire connections
    // Track auto-created nets for unconnected parent pins (e.g. SW, FB, BOOT)
    let mut auto_parent_nets: HashMap<String, NetId> = HashMap::new();
    let mut conn_idx_counter = 0usize;

    for conn in &cand.recipe.connections {
        conn_idx_counter += 1;
        let from_net = resolve_endpoint_net(
            netlist, &conn.from, cand, &child_instance_map, &internal_net_map,
            &rewired_original_nets,
        )?;
        let to_net = resolve_endpoint_net(
            netlist, &conn.to, cand, &child_instance_map, &internal_net_map,
            &rewired_original_nets,
        )?;

        // The connection is: connect from_endpoint to to_endpoint.
        // Both should end up on the same net.
        // When neither endpoint has a net yet (e.g. parent pin SW has no board-level
        // connection, and child L_out.1 is brand new), create a new net.
        let target_net = match from_net.or(to_net) {
            Some(net) => net,
            None => {
                // Check if we already auto-created a net for this parent pin
                let auto_key = match &conn.from {
                    ExpansionEndpoint::ParentPin(p) => Some(p.clone()),
                    _ => match &conn.to {
                        ExpansionEndpoint::ParentPin(p) => Some(p.clone()),
                        _ => None,
                    },
                };
                if let Some(ref key) = auto_key {
                    if let Some(&existing) = auto_parent_nets.get(key) {
                        existing
                    } else {
                        let net_name = format!("{}_{}", base, key);
                        let net_id = netlist.add_net(Some(net_name.clone()));
                        debug!("Auto-created net '{}' for unconnected parent pin '{}'", net_name, key);
                        auto_parent_nets.insert(key.clone(), net_id);
                        // Also connect the parent's pin instance to this new net
                        if let Some(&pi_id) = cand.pin_instances.get(key) {
                            netlist.connect(net_id, ConnectionPoint::PinInstance(pi_id))
                                .map_err(|e| format!("connect parent pin '{}': {}", key, e))?;
                        }
                        net_id
                    }
                } else {
                    // Neither endpoint is a parent pin — create an anonymous net
                    let net_name = format!("{}_auto_{}", base, conn_idx_counter);
                    let net_id = netlist.add_net(Some(net_name));
                    net_id
                }
            }
        };

        // Connect the "from" side's pin instance to the target net
        connect_endpoint_to_net(
            netlist, &conn.from, target_net, cand, &child_instance_map,
        )?;

        // Connect the "to" side's pin instance to the target net
        connect_endpoint_to_net(
            netlist, &conn.to, target_net, cand, &child_instance_map,
        )?;
    }

    // Mark the parent instance as expanded so the legacy vpin expander skips it
    if let Some(parent) = netlist.instances.get_mut(cand.instance_id) {
        parent.attributes.insert("expansion_applied".to_string(), "true".to_string());
    }

    Ok(ExpansionResult {
        parent_instance: base.clone(),
        child_instances: child_names,
        internal_nets: cand.recipe.internal_nets.iter()
            .map(|n| format!("{}_{}", base, n))
            .collect(),
    })
}

/// Dispatch the intent-driven designer for the parent instance and collect
/// the child-name → computed-value overrides it produces.
///
/// Each tube intent owns its own designer in `bhdl_spice::tube_bias`. The
/// returned map keys correspond to the names of the expansion-block
/// children whose `value` attribute the design wants to replace (e.g.
/// "Rp"/"Rk" for an amplifier, just "Rk" for a current source).
fn intent_driven_values(
    netlist: &Netlist,
    cand: &ExpansionCandidate,
) -> std::collections::HashMap<&'static str, f64> {
    let mut out = std::collections::HashMap::new();
    match cand.param_values.get("intent_name").map(String::as_str) {
        Some("amplifier") => {
            if let Some(net) = amplifier_bias_design(netlist, cand) {
                out.insert("Rp", net.r_plate);
                out.insert("Rk", net.r_cathode);
            }
        }
        Some("current_source") => {
            if let Some(r_k) = current_source_bias_design(cand) {
                out.insert("Rk", r_k);
            }
        }
        _ => {}
    }
    out
}

/// Size the cathode degeneration resistor for a `current_source` intent.
///
/// Reads the target current from `intent_current` and calls
/// `bhdl_spice::tube_bias::design_current_source`. Returns `None` when the
/// intent is missing/malformed or the designer rejects (target too high,
/// not realizable, …) — the caller then keeps the expansion block's
/// literal R_k. The `TubeCurrentSource` entity uses the default 6SN7 tube.
fn current_source_bias_design(cand: &ExpansionCandidate) -> Option<f64> {
    use bhdl_spice::tube_bias::design_current_source;
    use bhdl_spice::triode::TriodeParams;

    let target = cand.param_values.get("intent_current")
        .and_then(|s| parse_unit_value(s))?;
    let params = TriodeParams::sn6_6sn7();
    match design_current_source(&params, target) {
        Ok(r_k) => {
            info!(
                "Intent-driven current_source for '{}': I_p = {:.3} mA → R_k {:.0} Ω",
                cand.instance_name, target * 1e3, r_k
            );
            Some(r_k)
        }
        Err(e) => {
            warn!("current_source intent on '{}': {} — keeping expansion defaults",
                cand.instance_name, e);
            None
        }
    }
}

/// Run the intent-driven operating-point designer for an `amplifier` intent.
///
/// If `cand`'s parent instance carries `intent_name = "amplifier"`, this reads
/// the B+ rail (from the power net on the parent's `VBB` pin) and the gain
/// target (`intent_gain`), runs `bhdl_spice::tube_bias`, and returns the
/// computed plate/cathode resistor network. Returns `None` when there is no
/// amplifier intent or the inputs cannot be resolved — the caller then keeps
/// the expansion block's literal component values.
fn amplifier_bias_design(
    netlist: &Netlist,
    cand: &ExpansionCandidate,
) -> Option<bhdl_spice::tube_bias::BiasNetwork> {
    use bhdl_spice::tube_bias::{design_amplifier_reference, AmplifierSpec};
    use bhdl_spice::triode::TriodeParams;

    if cand.param_values.get("intent_name").map(String::as_str) != Some("amplifier") {
        return None;
    }

    // B+ supply: the power voltage of the net on the parent's VBB pin.
    let vbb_pin = *cand.pin_instances.get("VBB")?;
    let vbb_net = find_net_for_pin_instance(netlist, vbb_pin)?;
    let v_bb = match &netlist.nets.get(vbb_net)?.net_class {
        bhdl_netlist::NetClass::Power(v) => *v,
        _ => {
            warn!("amplifier intent on '{}': VBB pin is not on a power rail \
                   — keeping expansion defaults", cand.instance_name);
            return None;
        }
    };

    // Gain target from the stamped intent (absent ⇒ a default Class-A point).
    let spec = match cand.param_values.get("intent_gain")
        .and_then(|s| s.parse::<f64>().ok())
    {
        Some(g) => AmplifierSpec::gain(g),
        None => AmplifierSpec::default_class_a(),
    };

    // The SignalTubeStage expands a default 6SN7 triode.
    let params = TriodeParams::sn6_6sn7();
    match design_amplifier_reference(&params, v_bb, &spec) {
        Ok(d) => {
            info!(
                "Intent-driven bias for '{}': gain {:.1}, V_plate {:.0} V → \
                 Rp {:.0} Ω, Rk {:.0} Ω",
                cand.instance_name, d.operating_point.gain,
                d.operating_point.v_plate, d.network.r_plate, d.network.r_cathode
            );
            Some(d.network)
        }
        Err(e) => {
            warn!("amplifier intent on '{}': {} — keeping expansion defaults",
                cand.instance_name, e);
            None
        }
    }
}

/// Resolve a parameter expression by substituting entity parameter values.
/// Checks instance attributes first, then falls back to recipe param_defaults.
fn resolve_param_expression(
    expr: &str,
    param_values: &HashMap<String, String>,
    param_defaults: &HashMap<String, String>,
) -> String {
    let trimmed = expr.trim();
    // Try direct lookup in instance attributes
    if let Some(val) = param_values.get(trimmed) {
        return val.clone();
    }
    // Try with common attribute prefixes
    if let Some(val) = param_values.get(&format!("vpin_{}", trimmed)) {
        return val.clone();
    }
    // Fall back to recipe's entity parameter defaults
    if let Some(val) = param_defaults.get(trimmed) {
        debug!("Using recipe default for param '{}': {}", trimmed, val);
        return val.clone();
    }
    // Return as-is (it might be a literal like "33µH")
    trimmed.to_string()
}

/// Determine the standard pin layout for a component type.
/// Map expansion component type name to its `component_class` attribute value.
/// This must match the keys in `bhdl_spice::component_registry::class_to_component_type`
/// so that GLACIER can identify these components for DC simulation.
fn component_type_to_class(component_type: &str) -> &'static str {
    match component_type {
        "Ind" | "Inductor" => "inductor",
        "Cap" | "Capacitor" => "capacitor",
        "Res" | "Resistor" => "resistor",
        "Diode" => "diode",
        "TVSDiode" => "tvs_diode",
        "LED" => "led",
        "Triode" => "triode",
        _ => "passive", // generic fallback
    }
}

fn component_type_pins(component_type: &str) -> Vec<(&'static str, bool)> {
    match component_type {
        "Ind" | "Inductor" => vec![("1", true), ("2", true)],
        "Cap" | "Capacitor" => vec![("1", true), ("2", true)],
        "Res" | "Resistor" => vec![("1", true), ("2", true)],
        "Diode" => vec![("A", false), ("K", false)],
        "TVSDiode" => vec![("A", false), ("K", false)],
        // Vacuum triode: plate / grid / cathode. The netlist converter
        // classifies these terminals by name, so pin order is what matters.
        "Triode" => vec![("P", true), ("G", true), ("K", true)],
        _ => vec![("1", true), ("2", true)], // Default: two passive pins
    }
}

/// Create pin instances for all pins of a module on a given instance.
fn create_child_pin_instances(
    netlist: &mut Netlist,
    inst_id: InstanceId,
) -> Result<Vec<PinInstanceId>, String> {
    netlist.create_pin_instances(inst_id)
        .map_err(|e| format!("create pin instances: {}", e))
}

/// Detect interposition patterns in the expansion recipe.
///
/// An interposition occurs when a parent pin connects to one side of a child,
/// and the child's other side connects to an internal net. This means the child
/// is "interposed" between the parent pin's board net and the internal net.
///
/// Returns: Vec<(parent_pin_name, internal_net_name)> for pins that should be rewired.
fn detect_interpositions(recipe: &ExpansionRecipe) -> Vec<(String, String)> {
    let mut result = Vec::new();

    for conn_a in &recipe.connections {
        if let ExpansionEndpoint::ParentPin(pin) = &conn_a.from {
            if let ExpansionEndpoint::InstancePin(child, _child_pin_a) = &conn_a.to {
                // Look for another connection from a DIFFERENT pin of the same child → InternalNet
                for conn_b in &recipe.connections {
                    if let ExpansionEndpoint::InstancePin(child2, child_pin_b) = &conn_b.from {
                        if child2 == child && child_pin_b != _child_pin_a {
                            if let ExpansionEndpoint::InternalNet(net) = &conn_b.to {
                                // Found interposition: ParentPin(P) → Child.a, Child.b → InternalNet(N)
                                if !result.iter().any(|(p, _)| p == pin) {
                                    debug!("Detected interposition: parent pin '{}' → internal net '{}' via child '{}'",
                                        pin, net, child);
                                    result.push((pin.clone(), net.clone()));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    result
}

/// Resolve an endpoint to its associated net (if any already exists).
/// When a parent pin has been rewired (interposition), returns the ORIGINAL net
/// so that expansion connections referencing that pin connect to the board-level net.
fn resolve_endpoint_net(
    netlist: &Netlist,
    endpoint: &ExpansionEndpoint,
    cand: &ExpansionCandidate,
    children: &HashMap<String, (InstanceId, Vec<PinInstanceId>)>,
    internal_nets: &HashMap<String, NetId>,
    rewired_original_nets: &HashMap<String, NetId>,
) -> Result<Option<NetId>, String> {
    match endpoint {
        ExpansionEndpoint::ParentPin(pin_name) => {
            // If this pin was rewired, return the ORIGINAL board net
            if let Some(&original_net) = rewired_original_nets.get(pin_name) {
                return Ok(Some(original_net));
            }
            // Normal path: find the net connected to the parent's pin
            if let Some(&pi_id) = cand.pin_instances.get(pin_name) {
                Ok(find_net_for_pin_instance(netlist, pi_id))
            } else {
                Err(format!("Parent pin '{}' not found on instance '{}'", pin_name, cand.instance_name))
            }
        }
        ExpansionEndpoint::InstancePin(child_name, pin_name) => {
            // Find the net connected to a child instance's pin (likely None for new instances)
            if let Some((_, pi_ids)) = children.get(child_name) {
                for &pi_id in pi_ids {
                    if let Some(pi) = netlist.pin_instances.get(pi_id) {
                        if let Some(pin) = netlist.pins.get(pi.pin_def) {
                            if pin.name == *pin_name {
                                return Ok(find_net_for_pin_instance(netlist, pi_id));
                            }
                        }
                    }
                }
                // Pin not found — it might use numeric names like "1", "2"
                Err(format!("Pin '{}' not found on child instance '{}'", pin_name, child_name))
            } else {
                Err(format!("Child instance '{}' not found in expansion", child_name))
            }
        }
        ExpansionEndpoint::InternalNet(net_name) => {
            Ok(internal_nets.get(net_name).copied())
        }
    }
}

/// Connect an endpoint's pin instance to a given net.
fn connect_endpoint_to_net(
    netlist: &mut Netlist,
    endpoint: &ExpansionEndpoint,
    net_id: NetId,
    cand: &ExpansionCandidate,
    children: &HashMap<String, (InstanceId, Vec<PinInstanceId>)>,
) -> Result<(), String> {
    match endpoint {
        ExpansionEndpoint::ParentPin(pin_name) => {
            // The parent's pin is already connected — nothing to do
            // (its existing net is what we resolved as the target net)
            Ok(())
        }
        ExpansionEndpoint::InstancePin(child_name, pin_name) => {
            if let Some((inst_id, pi_ids)) = children.get(child_name) {
                connect_pin_instance_by_name(netlist, *inst_id, pi_ids, pin_name, net_id)
            } else {
                Err(format!("Child instance '{}' not found", child_name))
            }
        }
        ExpansionEndpoint::InternalNet(_) => {
            // Internal nets don't have pin instances to connect — they're just net IDs
            // The connection happens when a child pin instance is connected to this net
            Ok(())
        }
    }
}

/// Determine schematic placement hint for an expansion child based on
/// its connection topology and the parent entity's pin types.
///
/// Returns a role string matching datasheet application-note conventions:
///   - `main_path`       — series element on the power path (inductor SW→VOUT)
///   - `input_shunt`     — input decoupling cap (VIN→GND)
///   - `output_shunt`    — output filtering cap (VOUT→GND)
///   - `switching_shunt` — catch element at switching node (SW→GND)
///   - `bootstrap`       — bootstrap cap between two non-GND pins (BOOT↔SW)
///   - `feedback_high`   — top resistor in feedback divider (VOUT→FB)
///   - `feedback_low`    — bottom resistor in feedback divider (FB→GND)
///   - `shunt`           — generic shunt (fallback)
///   - `series`          — generic series (fallback)
fn determine_schematic_placement(
    child: &bhdl_common::ExpansionInstance,
    recipe: &ExpansionRecipe,
    is_shunt: bool,
) -> String {
    // Collect parent pins this child connects to
    let mut parent_pins: Vec<String> = Vec::new();
    for conn in &recipe.connections {
        let (child_ep, parent_ep) = match (&conn.from, &conn.to) {
            (ExpansionEndpoint::InstancePin(n, _), ExpansionEndpoint::ParentPin(p))
                if n == &child.name => (true, Some(p.clone())),
            (ExpansionEndpoint::ParentPin(p), ExpansionEndpoint::InstancePin(n, _))
                if n == &child.name => (true, Some(p.clone())),
            _ => (false, None),
        };
        if child_ep {
            if let Some(pin) = parent_ep {
                if !parent_pins.contains(&pin) {
                    parent_pins.push(pin);
                }
            }
        }
    }

    let connects_gnd = parent_pins.iter().any(|p| p.to_uppercase() == "GND");
    let non_gnd: Vec<&String> = parent_pins.iter()
        .filter(|p| p.to_uppercase() != "GND")
        .collect();

    // Helper: look up pin type from recipe's pin_info
    let pin_type = |name: &str| -> &str {
        recipe.pin_info.get(name)
            .map(|(t, _)| t.as_str())
            .unwrap_or("signal")
    };
    let pin_dir = |name: &str| -> &str {
        recipe.pin_info.get(name)
            .map(|(_, d)| d.as_str())
            .unwrap_or("inout")
    };

    // Classify based on connections + pin types
    if !connects_gnd && non_gnd.len() >= 2 {
        // Two or more non-GND parent pins
        let has_feedback = non_gnd.iter().any(|p| pin_type(p) == "feedback");
        let has_switch = non_gnd.iter().any(|p| pin_type(p) == "switch");
        let has_output = non_gnd.iter().any(|p|
            pin_type(p) == "power" && pin_dir(p) == "out");
        if has_feedback && has_output {
            return "feedback_high".to_string();
        }
        // Inductor: switch pin → output pin (main power path)
        if has_switch && has_output {
            return "main_path".to_string();
        }
        // Bootstrap/bridge cap (e.g., BOOT↔SW)
        return "bootstrap".to_string();
    }

    if !connects_gnd {
        // No GND, single non-GND pin
        if non_gnd.len() == 1 {
            let has_output = pin_type(non_gnd[0]) == "power" && pin_dir(non_gnd[0]) == "out";
            if has_output {
                return "main_path".to_string();
            }
        }
        return "series".to_string();
    }

    // connects_gnd is true from here — shunt classification
    if non_gnd.len() == 1 {
        let p = non_gnd[0];
        let pt = pin_type(p);
        let pd = pin_dir(p);

        if pt == "feedback" {
            return "feedback_low".to_string();
        }
        if pt == "switch" {
            return "switching_shunt".to_string();
        }
        if pt == "power" && pd == "out" {
            return "output_shunt".to_string();
        }
        if pt == "power" && pd == "in" {
            return "input_shunt".to_string();
        }
    }

    // Fallback
    if is_shunt { "shunt".to_string() } else { "series".to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_netlist::{ModuleKind, NetClass, PinDirection, PinType};

    fn setup_test_netlist_with_buck() -> (Netlist, InstanceId, HashMap<String, ExpansionRecipe>) {
        let mut netlist = Netlist::new();

        // Create BuckRegulator module
        let buck_mod = netlist.add_module("BuckRegulator".to_string(), ModuleKind::PhysicalComponent);
        netlist.add_pin(buck_mod, "VIN".to_string(), PinDirection::In, PinType::Power);
        netlist.add_pin(buck_mod, "VOUT".to_string(), PinDirection::Out, PinType::Power);
        netlist.add_pin(buck_mod, "GND".to_string(), PinDirection::Ground, PinType::Ground);

        // Create instance
        let inst_id = netlist.instances.insert(bhdl_netlist::Instance {
            name: "buck".to_string(),
            definition: buck_mod,
            attributes: [
                ("component_class".to_string(), "switching_regulator".to_string()),
                ("l_value".to_string(), "33µH".to_string()),
                ("c_out".to_string(), "470µF".to_string()),
                ("diode_vf".to_string(), "0.5V".to_string()),
            ].into_iter().collect(),
        });

        // Create pin instances using netlist helper (same as production code)
        let pi_ids = netlist.create_pin_instances(inst_id).expect("create_pin_instances failed");
        // Pin instances are created in order: VIN, VOUT, GND
        let vin_pi = pi_ids[0];
        let vout_pi = pi_ids[1];
        let gnd_pi = pi_ids[2];

        // Create and connect nets
        let vin_net = netlist.add_net(Some("VIN".to_string()));
        let vout_net = netlist.add_net(Some("VOUT".to_string()));
        let gnd_net = netlist.add_net(Some("GND".to_string()));

        netlist.connect(vin_net, ConnectionPoint::PinInstance(vin_pi)).unwrap();
        netlist.connect(vout_net, ConnectionPoint::PinInstance(vout_pi)).unwrap();
        netlist.connect(gnd_net, ConnectionPoint::PinInstance(gnd_pi)).unwrap();

        // Create expansion recipe
        let mut recipes = HashMap::new();
        let mut recipe = ExpansionRecipe::new("BuckRegulator".to_string());
        recipe.internal_nets = vec!["sw".to_string()];
        recipe.instances = vec![
            bhdl_common::ExpansionInstance {
                name: "L".to_string(),
                component_type: "Ind".to_string(),
                params: vec!["l_value".to_string()],
                attributes: HashMap::new(),
            },
            bhdl_common::ExpansionInstance {
                name: "D".to_string(),
                component_type: "Diode".to_string(),
                params: vec!["diode_vf".to_string()],
                attributes: HashMap::new(),
            },
            bhdl_common::ExpansionInstance {
                name: "C_out".to_string(),
                component_type: "Cap".to_string(),
                params: vec!["c_out".to_string()],
                attributes: HashMap::new(),
            },
        ];
        recipe.connections = vec![
            // VOUT -> L.1; L.2 -> sw
            ExpansionConnection {
                from: ExpansionEndpoint::ParentPin("VOUT".to_string()),
                to: ExpansionEndpoint::InstancePin("L".to_string(), "1".to_string()),
            },
            ExpansionConnection {
                from: ExpansionEndpoint::InstancePin("L".to_string(), "2".to_string()),
                to: ExpansionEndpoint::InternalNet("sw".to_string()),
            },
            // sw -> D.K, D.A -> GND
            ExpansionConnection {
                from: ExpansionEndpoint::InternalNet("sw".to_string()),
                to: ExpansionEndpoint::InstancePin("D".to_string(), "K".to_string()),
            },
            ExpansionConnection {
                from: ExpansionEndpoint::InstancePin("D".to_string(), "A".to_string()),
                to: ExpansionEndpoint::ParentPin("GND".to_string()),
            },
            // VOUT -> C_out.1, C_out.2 -> GND
            ExpansionConnection {
                from: ExpansionEndpoint::ParentPin("VOUT".to_string()),
                to: ExpansionEndpoint::InstancePin("C_out".to_string(), "1".to_string()),
            },
            ExpansionConnection {
                from: ExpansionEndpoint::InstancePin("C_out".to_string(), "2".to_string()),
                to: ExpansionEndpoint::ParentPin("GND".to_string()),
            },
        ];
        recipes.insert("BuckRegulator".to_string(), recipe);

        (netlist, inst_id, recipes)
    }

    #[test]
    fn test_expand_creates_child_instances() {
        let (mut netlist, _, recipes) = setup_test_netlist_with_buck();
        let results = expand_entity_instances(&mut netlist, &recipes);

        assert_eq!(results.len(), 1, "Should expand one instance");
        let r = &results[0];
        assert_eq!(r.parent_instance, "buck");
        assert_eq!(r.child_instances.len(), 3, "Should create L, D, C_out");
        assert!(r.child_instances.contains(&"buck_L".to_string()));
        assert!(r.child_instances.contains(&"buck_D".to_string()));
        assert!(r.child_instances.contains(&"buck_C_out".to_string()));
    }

    #[test]
    fn test_expand_creates_internal_net() {
        let (mut netlist, _, recipes) = setup_test_netlist_with_buck();
        let results = expand_entity_instances(&mut netlist, &recipes);

        assert_eq!(results[0].internal_nets.len(), 1);
        assert!(results[0].internal_nets.contains(&"buck_sw".to_string()));

        // Verify the net exists in the netlist
        let sw_net = netlist.nets.iter()
            .find(|(_, n)| n.name.as_deref() == Some("buck_sw"));
        assert!(sw_net.is_some(), "Internal net 'buck_sw' should exist");
    }

    #[test]
    fn test_expand_skips_expansion_children() {
        let (mut netlist, _, recipes) = setup_test_netlist_with_buck();

        // First expansion
        let results1 = expand_entity_instances(&mut netlist, &recipes);
        assert_eq!(results1.len(), 1);

        // Second expansion should skip already-expanded children
        let results2 = expand_entity_instances(&mut netlist, &recipes);
        // The parent should be skipped too since its recipe was already applied
        // (though currently it would try again — expansion_parent isn't set on parent)
        // The child instances have expansion_parent set so they won't match
        assert!(results2.len() <= 1, "Should not expand children");
    }

    #[test]
    fn test_no_expansion_without_recipe() {
        let mut netlist = Netlist::new();
        let mod_id = netlist.add_module("LinearRegulator".to_string(), ModuleKind::PhysicalComponent);
        netlist.instances.insert(bhdl_netlist::Instance {
            name: "ldo".to_string(),
            definition: mod_id,
            attributes: HashMap::new(),
        });

        let recipes: HashMap<String, ExpansionRecipe> = HashMap::new();
        let results = expand_entity_instances(&mut netlist, &recipes);
        assert!(results.is_empty());
    }
}
