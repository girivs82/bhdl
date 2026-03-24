//! Semantic preprocessor: BHDL Netlist → PnR Board.
//!
//! Converts a synthesized BHDL `Netlist` (with optional GLACIER simulation data)
//! into a fully populated `Board` struct ready for place-and-route.
//!
//! Package geometry is resolved via:
//! 1. IPC-7351B footprint generator (from component package string)
//! 2. Fallback defaults when package string is unknown

use std::collections::HashMap;

use anyhow::Result;
use slotmap::SlotMap;
use thiserror::Error;

use bhdl_netlist::{
    ConnectionPoint, InstanceId, Net, NetClass, NetId as NlNetId,
    Netlist, PinId as NlPinId, PinInstanceId,
};
use bhdl_schematic::types::SimulationAnnotations;

use crate::ipc7351::{self, DensityLevel};
use crate::stackup;
use crate::types::*;

// ── Public types ─────────────────────────────────────────────────────

/// Configuration for the semantic preprocessor.
#[derive(Debug, Clone)]
pub struct SemanticConfig {
    /// Board configuration (outline, stackup, design rules, fixed placements).
    pub board_config: BoardConfig,
    /// Override package for specific instances: instance_name → package_string.
    pub package_overrides: HashMap<String, String>,
    /// IPC-7351B density level for footprint generation.
    pub density_level: DensityLevel,
}

impl Default for SemanticConfig {
    fn default() -> Self {
        SemanticConfig {
            board_config: BoardConfig::default(),
            package_overrides: HashMap::new(),
            density_level: DensityLevel::Nominal,
        }
    }
}

/// Errors from the semantic preprocessing stage.
#[derive(Debug, Error)]
pub enum SemanticError {
    #[error("no top-level module in netlist")]
    NoTopModule,
    #[error("top-level module not found in netlist")]
    TopModuleNotFound,
    #[error("netlist has no instances")]
    EmptyNetlist,
}

// ── Internal ID mapping ──────────────────────────────────────────────

/// Ephemeral mapping between netlist IDs and PnR IDs.
/// Lives only inside `build_board()`.
struct IdMap {
    /// Netlist InstanceId → index in Board.components
    inst_to_idx: HashMap<InstanceId, usize>,
    /// Netlist NetId → index in Board.nets
    net_to_idx: HashMap<NlNetId, usize>,
    /// Component index → ComponentId (assigned after vec construction)
    comp_ids: Vec<ComponentId>,
    /// Net index → PnR NetId
    net_ids: Vec<NetId>,
}

// ── Public API ───────────────────────────────────────────────────────

/// Build a PnR `Board` from a BHDL `Netlist`.
///
/// Resolution order for package geometry:
/// 1. `config.package_overrides[instance_name]`
/// 2. `instance.attributes["package"]`
/// 3. `default_package_for_category(component_class)`
/// 4. IPC-7351B `standard_package()` → `generate_footprint()`
pub fn build_board(
    netlist: &Netlist,
    simulation: Option<&SimulationAnnotations>,
    config: SemanticConfig,
) -> Result<Board, SemanticError> {
    // 1. Validate top-level module
    let top_module_id = netlist.top_level_module.ok_or(SemanticError::NoTopModule)?;
    let top_module = netlist
        .modules
        .get(top_module_id)
        .ok_or(SemanticError::TopModuleNotFound)?;

    // 2. Collect top-level instances
    let top_instances: Vec<InstanceId> = if !top_module.internal_instances.is_empty() {
        top_module.internal_instances.clone()
    } else {
        netlist.instances.keys().collect()
    };

    if top_instances.is_empty() {
        return Err(SemanticError::EmptyNetlist);
    }

    // 3. Build net lookup tables (mirrors bhdl-schematic/src/extract.rs:60-95)
    let (pin_inst_to_net, _inst_pin_to_net) = build_net_lookups(netlist);

    // 4. Build fixed placement lookup
    let fixed_placements: HashMap<String, &FixedPlacement> = config
        .board_config
        .fixed_placements
        .iter()
        .map(|fp| (fp.instance_name.clone(), fp))
        .collect();

    // 5. Construct components
    let mut id_map = IdMap {
        inst_to_idx: HashMap::new(),
        net_to_idx: HashMap::new(),
        comp_ids: Vec::new(),
        net_ids: Vec::new(),
    };
    let mut components = Vec::new();
    let mut refdes_counters: HashMap<String, usize> = HashMap::new();

    for &inst_id in &top_instances {
        let instance = match netlist.instances.get(inst_id) {
            Some(i) => i,
            None => continue,
        };
        let module_def = match netlist.modules.get(instance.definition) {
            Some(m) => m,
            None => continue,
        };

        // Resolve package string
        let package = config
            .package_overrides
            .get(&instance.name)
            .cloned()
            .or_else(|| instance.attributes.get("package").cloned())
            .unwrap_or_else(|| {
                let cat = categorize_component(&module_def.name, &instance.attributes);
                default_package_for_category(&cat)
            });

        // Generate footprint via IPC-7351B
        let footprint = ipc7351::standard_package(&package)
            .map(|family| ipc7351::generate_footprint(&family, config.density_level));

        let (width, height) = footprint
            .as_ref()
            .map(|fp| (fp.body_width, fp.body_height))
            .unwrap_or((5.0, 5.0));

        // Build pin positions from footprint pads
        let pin_defs: Vec<NlPinId> = module_def
            .pins
            .iter()
            .copied()
            .filter(|&pid| {
                netlist
                    .pins
                    .get(pid)
                    .map(|p| !p.is_virtual)
                    .unwrap_or(false)
            })
            .collect();

        // If module has no pin definitions (e.g., expansion-created instances),
        // count pin_instances belonging to this instance instead
        let pin_instance_count = if pin_defs.is_empty() {
            netlist
                .pin_instances
                .values()
                .filter(|pi| pi.instance == inst_id)
                .count()
        } else {
            0
        };

        let effective_pin_count = if !pin_defs.is_empty() {
            pin_defs.len()
        } else if pin_instance_count > 0 {
            pin_instance_count
        } else {
            // Infer from component category (passives = 2 pins)
            let cat = categorize_component(&module_def.name, &instance.attributes);
            match cat.as_str() {
                "resistor" | "capacitor" | "inductor" | "diode" | "led" | "ferrite_bead" => 2,
                _ => 0,
            }
        };

        let pins: Vec<PinPosition> = if let Some(ref fp) = footprint {
            // Use real pad positions from IPC-7351B
            if !pin_defs.is_empty() {
                pin_defs
                    .iter()
                    .enumerate()
                    .map(|(i, &pid)| {
                        let pin_name = netlist
                            .pins
                            .get(pid)
                            .map(|p| p.name.clone())
                            .unwrap_or_else(|| (i + 1).to_string());
                        let (dx, dy) = fp
                            .pads
                            .get(i)
                            .map(|pad| (pad.x_position, pad.y_position))
                            .unwrap_or((0.0, 0.0));
                        PinPosition {
                            pin_id: PinId::default(),
                            name: pin_name,
                            dx,
                            dy,
                            net: None,
                        }
                    })
                    .collect()
            } else {
                // No module pin defs — use footprint pad count up to effective_pin_count
                (0..effective_pin_count)
                    .map(|i| {
                        let (dx, dy) = fp
                            .pads
                            .get(i)
                            .map(|pad| (pad.x_position, pad.y_position))
                            .unwrap_or((0.0, 0.0));
                        PinPosition {
                            pin_id: PinId::default(),
                            name: (i + 1).to_string(),
                            dx,
                            dy,
                            net: None,
                        }
                    })
                    .collect()
            }
        } else {
            // Fallback: estimate pin positions
            estimate_pin_positions(&pin_defs, netlist, width, height)
        };

        // Thermal power from GLACIER
        let thermal_power = simulation
            .and_then(|sim| sim.instance_power.get(&instance.name).copied())
            .unwrap_or(0.0);

        // Reference designator
        let category = categorize_component(&module_def.name, &instance.attributes);
        let prefix = category_prefix(&category);
        let counter = refdes_counters.entry(prefix.clone()).or_insert(0);
        *counter += 1;
        let refdes = instance
            .attributes
            .get("refdes")
            .cloned()
            .unwrap_or_else(|| format!("{}{}", prefix, counter));

        // Placement constraint
        let placement = fixed_placements
            .get(&instance.name)
            .map(|fp| PlacementConstraint::Fixed {
                x: fp.x_mm,
                y: fp.y_mm,
                theta: fp.rotation_deg.to_radians(),
            })
            .unwrap_or(PlacementConstraint::Free);

        let comp_idx = components.len();
        id_map.inst_to_idx.insert(inst_id, comp_idx);

        components.push(Component {
            id: ComponentId::default(),
            name: instance.name.clone(),
            refdes,
            package,
            width_mm: width,
            height_mm: height,
            pins,
            side: BoardSide::Top,
            group: None,
            thermal_power_w: thermal_power,
            placement,
            x: 0.0,
            y: 0.0,
            theta: 0.0,
            density_inflation: 1.0,
        });
    }

    // 6. Construct nets
    let mut nets = Vec::new();

    for (nl_net_id, net) in netlist.nets.iter() {
        if net.connections.len() < 2 {
            continue;
        }

        // Check if any connection belongs to a top-level instance
        let has_top_connection = net.connections.iter().any(|conn| match conn {
            ConnectionPoint::PinInstance(pi_id) => netlist
                .pin_instances
                .get(*pi_id)
                .map(|pi| id_map.inst_to_idx.contains_key(&pi.instance))
                .unwrap_or(false),
            ConnectionPoint::InstancePin(inst_id, _) => {
                id_map.inst_to_idx.contains_key(inst_id)
            }
            ConnectionPoint::InstancePort(inst_id, _) => {
                id_map.inst_to_idx.contains_key(inst_id)
            }
            ConnectionPoint::ModulePort(_) => true,
        });

        if !has_top_connection {
            continue;
        }

        let net_name = net
            .name
            .clone()
            .unwrap_or_else(|| format!("__net{}", nets.len()));
        let pnr_class = classify_net(&net.net_class, &net_name, simulation);
        let trace_width = compute_trace_width(
            &pnr_class,
            &net_name,
            simulation,
            config.board_config.min_trace_width_mm,
        );

        let intent = extract_net_intent(net, netlist, &id_map);

        // Intent-driven weight and layer constraint (Phase 4: semantic integration)
        let (intent_weight, intent_layer) = intent_routing_constraints(intent.as_deref());
        let base_layer = layer_constraint_for_net(&pnr_class);
        // Intent layer overrides net-class default if more specific
        let layer_constraint = match (&intent_layer, &base_layer) {
            (LayerConstraint::Any, other) => other.clone(),
            (specific, _) => specific.clone(),
        };

        let net_idx = nets.len();
        id_map.net_to_idx.insert(nl_net_id, net_idx);

        nets.push(PnrNet {
            id: NetId::default(),
            name: net_name,
            pins: Vec::new(),
            net_class: pnr_class,
            weight: intent_weight,
            required_trace_width_mm: trace_width,
            layer_constraint,
            intent,
        });
    }

    // 7. Assign fresh slotmap IDs
    let mut comp_slot: SlotMap<ComponentId, ()> = SlotMap::with_key();
    for comp in &mut components {
        comp.id = comp_slot.insert(());
    }
    let mut pin_slot: SlotMap<PinId, ()> = SlotMap::with_key();
    for comp in &mut components {
        for pin in &mut comp.pins {
            pin.pin_id = pin_slot.insert(());
        }
    }
    let mut net_slot: SlotMap<NetId, ()> = SlotMap::with_key();
    for net in &mut nets {
        net.id = net_slot.insert(());
    }

    id_map.comp_ids = components.iter().map(|c| c.id).collect();
    id_map.net_ids = nets.iter().map(|n| n.id).collect();

    // 8. Wire up net.pins cross-references
    for (nl_net_id, net) in netlist.nets.iter() {
        let pnr_net_idx = match id_map.net_to_idx.get(&nl_net_id) {
            Some(&idx) => idx,
            None => continue,
        };

        for conn in &net.connections {
            let (inst_id, pin_index) = match resolve_connection(conn, netlist, &pin_inst_to_net) {
                Some(pair) => pair,
                None => continue,
            };

            let comp_idx = match id_map.inst_to_idx.get(&inst_id) {
                Some(&idx) => idx,
                None => continue,
            };

            let comp = &components[comp_idx];
            if pin_index < comp.pins.len() {
                let comp_id = comp.id;
                let pin_id = comp.pins[pin_index].pin_id;
                nets[pnr_net_idx].pins.push((comp_id, pin_id));
            }
        }

        // Deduplicate
        nets[pnr_net_idx].pins.dedup();
    }

    // Set pin.net back-references
    for net in &nets {
        for &(comp_id, pin_id) in &net.pins {
            if let Some(comp) = components.iter_mut().find(|c| c.id == comp_id) {
                if let Some(pin) = comp.pins.iter_mut().find(|p| p.pin_id == pin_id) {
                    pin.net = Some(net.id);
                }
            }
        }
    }

    // 9. Extract functional groups
    let mut groups = extract_groups(netlist, &id_map, &components);
    let mut group_slot: SlotMap<GroupId, ()> = SlotMap::with_key();
    for group in &mut groups {
        group.id = group_slot.insert(());
    }
    for group in &groups {
        for &member_id in &group.members {
            if let Some(comp) = components.iter_mut().find(|c| c.id == member_id) {
                comp.group = Some(group.id);
            }
        }
    }

    // 10. Resolve stackup
    let num_power = count_power_domains(netlist);
    let has_hs = has_high_speed_nets(netlist);
    let layer_stack = stackup::resolve_stackup(
        &config.board_config.stackup,
        components.len(),
        nets.len(),
        num_power,
        has_hs,
    );

    // 11. Auto-size board outline
    let outline = match &config.board_config.outline {
        BoardOutline::AutoSize => {
            let total_area: f64 = components
                .iter()
                .map(|c| c.width_mm * c.height_mm)
                .sum();
            let side = (total_area.sqrt() * 2.5).max(20.0);
            BoardOutline::Rectangle {
                width_mm: side,
                height_mm: side,
            }
        }
        other => other.clone(),
    };

    let board_config = BoardConfig {
        outline,
        ..config.board_config
    };

    Ok(Board {
        config: board_config,
        layer_stack,
        components,
        nets,
        groups,
    })
}

// ── Net lookup tables ────────────────────────────────────────────────

/// Build lookup tables mapping connection points to net IDs.
/// Mirrors bhdl-schematic/src/extract.rs:60-95.
fn build_net_lookups(
    netlist: &Netlist,
) -> (
    HashMap<PinInstanceId, NlNetId>,
    HashMap<(InstanceId, NlPinId), NlNetId>,
) {
    let mut pin_inst_to_net: HashMap<PinInstanceId, NlNetId> = HashMap::new();
    let mut inst_pin_to_net: HashMap<(InstanceId, NlPinId), NlNetId> = HashMap::new();

    for (net_id, net) in netlist.nets.iter() {
        for conn in &net.connections {
            match *conn {
                ConnectionPoint::PinInstance(pi_id) => {
                    pin_inst_to_net.insert(pi_id, net_id);
                }
                ConnectionPoint::InstancePin(inst_id, pin_id) => {
                    inst_pin_to_net.insert((inst_id, pin_id), net_id);
                }
                _ => {}
            }
        }
    }

    (pin_inst_to_net, inst_pin_to_net)
}

/// Resolve a ConnectionPoint to (InstanceId, pin_index) where pin_index
/// is the position in the module definition's non-virtual pin list.
fn resolve_connection(
    conn: &ConnectionPoint,
    netlist: &Netlist,
    _pin_inst_to_net: &HashMap<PinInstanceId, NlNetId>,
) -> Option<(InstanceId, usize)> {
    match conn {
        ConnectionPoint::PinInstance(pi_id) => {
            let pi = netlist.pin_instances.get(*pi_id)?;
            let instance = netlist.instances.get(pi.instance)?;
            let module = netlist.modules.get(instance.definition)?;
            // Find pin index among non-virtual pins
            let pin_index = module
                .pins
                .iter()
                .filter(|&&pid| {
                    netlist
                        .pins
                        .get(pid)
                        .map(|p| !p.is_virtual)
                        .unwrap_or(false)
                })
                .position(|&pid| pid == pi.pin_def)?;
            Some((pi.instance, pin_index))
        }
        ConnectionPoint::InstancePin(inst_id, pin_id) => {
            let instance = netlist.instances.get(*inst_id)?;
            let module = netlist.modules.get(instance.definition)?;
            let pin_index = module
                .pins
                .iter()
                .filter(|&&pid| {
                    netlist
                        .pins
                        .get(pid)
                        .map(|p| !p.is_virtual)
                        .unwrap_or(false)
                })
                .position(|&pid| pid == *pin_id)?;
            Some((*inst_id, pin_index))
        }
        _ => None,
    }
}

// ── Net classification ───────────────────────────────────────────────

fn classify_net(
    net_class: &NetClass,
    net_name: &str,
    simulation: Option<&SimulationAnnotations>,
) -> PnrNetClass {
    match net_class {
        NetClass::Power(voltage) => {
            let current = simulation
                .and_then(|sim| {
                    // Use net voltage to validate, estimate current from connected instances
                    sim.net_voltages.get(net_name)?;
                    // Sum currents of instances on this net as an estimate
                    Some(
                        sim.instance_currents
                            .values()
                            .map(|c| c.abs())
                            .fold(0.0_f64, f64::max)
                            .max(0.1),
                    )
                })
                .unwrap_or(0.5);
            PnrNetClass::Power {
                voltage: *voltage,
                current,
            }
        }
        NetClass::Ground => PnrNetClass::Ground,
        NetClass::DifferentialPair { pair_name, .. } => PnrNetClass::DifferentialPair {
            partner_net_name: pair_name.clone(),
        },
        NetClass::Signal | NetClass::Bus { .. } => PnrNetClass::Signal,
    }
}

fn compute_trace_width(
    net_class: &PnrNetClass,
    _net_name: &str,
    simulation: Option<&SimulationAnnotations>,
    min_width: f64,
) -> f64 {
    match net_class {
        PnrNetClass::Power { current, .. } => {
            stackup::trace_width_for_current(*current, 1.0, 10.0).max(min_width)
        }
        PnrNetClass::Ground => {
            let max_current = simulation
                .map(|sim| {
                    sim.instance_currents
                        .values()
                        .map(|c| c.abs())
                        .fold(0.0_f64, f64::max)
                })
                .unwrap_or(0.5);
            stackup::trace_width_for_current(max_current, 1.0, 10.0).max(min_width)
        }
        _ => min_width,
    }
}

fn extract_net_intent(
    net: &Net,
    netlist: &Netlist,
    id_map: &IdMap,
) -> Option<String> {
    // Look for intent on any instance connected to this net.
    // Intents may be stored as "intent", "intent_name", or "stage_name" attributes.
    for conn in &net.connections {
        let inst_id = match conn {
            ConnectionPoint::PinInstance(pi_id) => {
                netlist.pin_instances.get(*pi_id).map(|pi| pi.instance)
            }
            ConnectionPoint::InstancePin(inst_id, _) => Some(*inst_id),
            ConnectionPoint::InstancePort(inst_id, _) => Some(*inst_id),
            _ => None,
        };
        if let Some(iid) = inst_id {
            if id_map.inst_to_idx.contains_key(&iid) {
                if let Some(inst) = netlist.instances.get(iid) {
                    // Check multiple attribute names for intent
                    if let Some(intent) = inst.attributes.get("intent")
                        .or_else(|| inst.attributes.get("intent_name"))
                        .or_else(|| inst.attributes.get("stage_name"))
                    {
                        return Some(intent.clone());
                    }
                }
            }
        }
    }
    None
}

// ── Functional groups ────────────────────────────────────────────────

fn extract_groups(
    netlist: &Netlist,
    id_map: &IdMap,
    _components: &[Component],
) -> Vec<FunctionalGroup> {
    let mut parent_children: HashMap<String, Vec<usize>> = HashMap::new();
    let mut name_to_idx: HashMap<String, usize> = HashMap::new();

    for (inst_id, inst) in &netlist.instances {
        if let Some(&comp_idx) = id_map.inst_to_idx.get(&inst_id) {
            name_to_idx.insert(inst.name.clone(), comp_idx);
        }
    }

    for (inst_id, inst) in &netlist.instances {
        let comp_idx = match id_map.inst_to_idx.get(&inst_id) {
            Some(&idx) => idx,
            None => continue,
        };

        let parent_name = inst
            .attributes
            .get("vpin_parent")
            .or_else(|| inst.attributes.get("expansion_parent"));

        if let Some(parent) = parent_name {
            parent_children
                .entry(parent.clone())
                .or_default()
                .push(comp_idx);
        }
    }

    let mut groups = Vec::new();
    for (parent_name, child_indices) in &parent_children {
        let parent_comp_idx = name_to_idx.get(parent_name).copied();

        let mut members: Vec<ComponentId> = child_indices
            .iter()
            .filter_map(|&idx| id_map.comp_ids.get(idx).copied())
            .collect();

        if let Some(pidx) = parent_comp_idx {
            if let Some(&pid) = id_map.comp_ids.get(pidx) {
                if !members.contains(&pid) {
                    members.insert(0, pid);
                }
            }
        }

        if members.len() < 2 {
            continue;
        }

        let parent_id = parent_comp_idx.and_then(|pidx| id_map.comp_ids.get(pidx).copied());

        groups.push(FunctionalGroup {
            id: GroupId::default(),
            name: parent_name.clone(),
            members,
            parent: parent_id,
        });
    }

    groups
}

// ── Component categorization ─────────────────────────────────────────

fn categorize_component(entity_type: &str, attrs: &HashMap<String, String>) -> String {
    // Prefer component_class attribute (set by GLACIER physical selection)
    if let Some(cc) = attrs.get("component_class") {
        return cc.clone();
    }

    let lower = entity_type.to_lowercase();
    if lower.contains("res") {
        "resistor".to_string()
    } else if lower.contains("cap") {
        "capacitor".to_string()
    } else if lower.contains("ind") || lower.contains("inductor") {
        "inductor".to_string()
    } else if lower.contains("diode") || lower.contains("led") {
        "diode".to_string()
    } else if lower.contains("tvs") {
        "tvs_diode".to_string()
    } else if lower.contains("regulator") || lower.contains("ldo") || lower.contains("7805") {
        "voltage_regulator".to_string()
    } else if lower.contains("buck") {
        "voltage_regulator".to_string()
    } else if lower.contains("opamp") {
        "opamp".to_string()
    } else {
        "ic".to_string()
    }
}

fn category_prefix(category: &str) -> String {
    match category {
        "resistor" => "R",
        "capacitor" => "C",
        "inductor" => "L",
        "diode" | "led" => "D",
        "tvs_diode" => "D",
        "voltage_regulator" => "U",
        "opamp" => "U",
        "ic" | "microcontroller" => "U",
        "connector" => "J",
        "ferrite_bead" => "FB",
        _ => "X",
    }
    .to_string()
}

fn default_package_for_category(category: &str) -> String {
    match category {
        "resistor" | "capacitor" | "ferrite_bead" => "0603",
        "inductor" => "1210",
        "diode" | "led" | "tvs_diode" => "SOT-23",
        "voltage_regulator" => "SOT-223",
        "opamp" | "buffer" => "SOIC-8",
        "microcontroller" => "TQFP-64",
        "connector" => "SOIC-8", // placeholder
        _ => "SOIC-8",
    }
    .to_string()
}

// ── Pin position estimation (fallback) ───────────────────────────────

fn estimate_pin_positions(
    pin_defs: &[NlPinId],
    netlist: &Netlist,
    width: f64,
    height: f64,
) -> Vec<PinPosition> {
    let n = pin_defs.len();
    if n == 0 {
        return Vec::new();
    }

    let mut positions = Vec::with_capacity(n);
    if n == 2 {
        // 2-pin: left and right
        for (i, &pid) in pin_defs.iter().enumerate() {
            let name = netlist
                .pins
                .get(pid)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| (i + 1).to_string());
            let dx = if i == 0 { -width / 2.0 } else { width / 2.0 };
            positions.push(PinPosition {
                pin_id: PinId::default(),
                name,
                dx,
                dy: 0.0,
                net: None,
            });
        }
    } else {
        // Distribute around perimeter
        let per = 2.0 * (width + height);
        let spacing = per / n as f64;
        for (i, &pid) in pin_defs.iter().enumerate() {
            let name = netlist
                .pins
                .get(pid)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| (i + 1).to_string());
            let d = i as f64 * spacing;
            let (dx, dy) = perimeter_point(d, width, height);
            positions.push(PinPosition {
                pin_id: PinId::default(),
                name,
                dx,
                dy,
                net: None,
            });
        }
    }
    positions
}

/// Map a distance along the perimeter to (x, y) relative to center.
fn perimeter_point(dist: f64, w: f64, h: f64) -> (f64, f64) {
    let hw = w / 2.0;
    let hh = h / 2.0;
    let per = 2.0 * (w + h);
    let d = dist % per;
    if d < w {
        (-hw + d, -hh) // top edge, left to right
    } else if d < w + h {
        (hw, -hh + (d - w)) // right edge, top to bottom
    } else if d < 2.0 * w + h {
        (hw - (d - w - h), hh) // bottom edge, right to left
    } else {
        (-hw, hh - (d - 2.0 * w - h)) // left edge, bottom to top
    }
}

// ── Layer constraint assignment ───────────────────────────────────────

/// Map intent annotations to routing constraints (weight, layer preference).
///
/// Higher weight = routed first by PathFinder (higher priority).
/// Layer constraint = which layers the net can use.
///
/// Based on PCB_Routing_Best_Practices.md §8 (Intent-Driven Routing).
fn intent_routing_constraints(intent: Option<&str>) -> (f64, LayerConstraint) {
    match intent {
        // Critical signals: route first, controlled impedance
        Some("clock_distribution") => (10.0, LayerConstraint::AdjacentToGround),
        Some("precision_measurement") => (10.0, LayerConstraint::AdjacentToGround),
        Some("communication_interface") => (8.0, LayerConstraint::AdjacentToGround),

        // Protection: route early (short paths to connector)
        Some("input_protection") => (5.0, LayerConstraint::Any),
        Some("esd_protection") => (5.0, LayerConstraint::Any),
        Some("overvoltage_protection") => (5.0, LayerConstraint::Any),

        // Regulation: important power path
        Some("regulation") => (5.0, LayerConstraint::Any),

        // Signal processing: moderate priority, good ground reference
        Some("anti_alias") => (5.0, LayerConstraint::AdjacentToGround),
        Some("noise_filtering") => (5.0, LayerConstraint::AdjacentToGround),
        Some("low_noise") => (5.0, LayerConstraint::AdjacentToGround),

        // Analog: moderate priority
        Some("signal_amplification") => (3.0, LayerConstraint::AdjacentToGround),
        Some("current_limiting") => (3.0, LayerConstraint::Any),
        Some("level_shifting") => (3.0, LayerConstraint::Any),

        // Digital: standard priority
        Some("signal_buffering") => (2.0, LayerConstraint::Any),
        Some("output_buffering") => (2.0, LayerConstraint::Any),
        Some("signal_distribution") => (2.0, LayerConstraint::Any),

        // Power filtering: already handled by cap sizer, low routing priority
        Some("input_filtering") => (1.0, LayerConstraint::Any),
        Some("output_filtering") => (1.0, LayerConstraint::Any),

        // Loading: low priority (LEDs, test loads)
        Some("loading") => (0.5, LayerConstraint::Any),

        // Safety: high priority, any layer
        Some("automotive_safety") => (8.0, LayerConstraint::Any),
        Some("medical_safety") => (8.0, LayerConstraint::Any),
        Some("industrial_control") => (5.0, LayerConstraint::Any),

        // No intent or unknown: default
        _ => (1.0, LayerConstraint::Any),
    }
}

/// Assign layer constraints based on net class.
///
/// - High-speed / differential pairs: adjacent-to-ground for impedance control
/// - Power nets: any signal layer (wide traces, routed like signals)
/// - Ground: any signal layer (plane-connected when ground layer exists, else routed)
/// - Signal: any signal layer
fn layer_constraint_for_net(net_class: &PnrNetClass) -> LayerConstraint {
    match net_class {
        PnrNetClass::HighSpeed { .. } | PnrNetClass::DifferentialPair { .. } => {
            LayerConstraint::AdjacentToGround
        }
        _ => LayerConstraint::Any,
    }
}

// ── Stackup heuristics ───────────────────────────────────────────────

fn count_power_domains(netlist: &Netlist) -> usize {
    netlist
        .nets
        .values()
        .filter(|net| matches!(net.net_class, NetClass::Power(_)))
        .count()
}

fn has_high_speed_nets(netlist: &Netlist) -> bool {
    netlist.nets.values().any(|net| {
        matches!(
            net.net_class,
            NetClass::DifferentialPair { .. } | NetClass::Bus { .. }
        )
    })
}
