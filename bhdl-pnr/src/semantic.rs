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
    // Phantom entity-definition stubs: an instance named exactly like its
    // module with ZERO net-connected pins is a template artifact, not a
    // part — each one duplicated a real footprint in the KiCad export
    // (same refdes, no nets); the DRC oracle flagged the ghosts as
    // shorts. A real (connected) part is never filtered.
    let top_instances: Vec<InstanceId> = top_instances
        .into_iter()
        .filter(|&inst_id| {
            let Some(instance) = netlist.instances.get(inst_id) else {
                return false;
            };
            let Some(module) = netlist.modules.get(instance.definition) else {
                return false;
            };
            if instance.name != module.name {
                return true;
            }
            let connected = netlist
                .pin_instances
                .values()
                .any(|pi| pi.instance == inst_id && pi.net.is_some());
            if connected {
                return true;
            }
            // A true phantom always SHADOWS a connected sibling of the
            // same module (the duplicate-footprint signature). A lone
            // module-named unconnected instance is a real part on a
            // minimal board (anonymous instances are auto-named after
            // their module) — filtering those emptied 12 corpus boards.
            let has_connected_sibling = netlist.instances.iter().any(|(oid, other)| {
                oid != inst_id
                    // By module NAME, not ModuleId: the phantom stub and
                    // the real instance carry SEPARATE ModuleDefinition
                    // entries with the same name.
                    && netlist
                        .modules
                        .get(other.definition)
                        .map(|m| m.name == module.name)
                        .unwrap_or(false)
                    && netlist
                        .pin_instances
                        .values()
                        .any(|pi| pi.instance == oid && pi.net.is_some())
            });
            !has_connected_sibling
        })
        .collect();

    if top_instances.is_empty() {
        return Err(SemanticError::EmptyNetlist);
    }

    // 3. Build net lookup tables (mirrors bhdl-schematic/src/extract.rs:60-95)
    let (pin_inst_to_net, inst_pin_to_net) = build_net_lookups(netlist);

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

        // Skip power/ground symbol instances — these are netlist-level
        // constructs (power rails, ground symbols), not physical components.
        // They have no package and shouldn't appear on the PCB.
        if is_power_symbol(module_def, instance, netlist) {
            continue;
        }

        // Pin count drives the default-package fit (computed before the
        // full pin_defs vec because package resolution needs it first).
        let pin_defs_count = module_def
            .pins
            .iter()
            .filter(|&&pid| {
                netlist
                    .pins
                    .get(pid)
                    .map(|p| !p.is_virtual)
                    .unwrap_or(false)
            })
            .count();

        // Resolve package string
        let package = config
            .package_overrides
            .get(&instance.name)
            .cloned()
            .or_else(|| instance.attributes.get("package").cloned())
            .unwrap_or_else(|| {
                let cat = categorize_component(&module_def.name, &instance.attributes);
                default_package_for_category(&cat, pin_defs_count)
            });

        // Generate footprint via IPC-7351B
        let footprint = ipc7351::standard_package(&package)
            .map(|family| ipc7351::generate_footprint(&family, config.density_level));

        // Use full footprint extent (body + pad protrusion), not just body
        let (width, height, bbox_dx, bbox_dy) = footprint
            .as_ref()
            .map(|fp| footprint_extent(fp))
            .unwrap_or((5.0, 5.0, 0.0, 0.0));

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

        let pins: Vec<PinPosition> = if let Some(ref fp) = footprint {
            // PAIRING CONTRACT: comp.pins[i] must correspond to module
            // pin_def[i] — net wiring later indexes comp.pins by the
            // module pin-def index (resolve_connection). The old code
            // paired pads to defs by RAW EMISSION ORDER; any footprint
            // whose pad order differs from the entity's declaration
            // order scrambled names AND nets (frontier dumps showed
            // pins whose coordinates landed on someone else's NC pad).
            //
            // Pairing, strongest first:
            //   1. name match (pad_number == pin name, case-insens.) —
            //      passives ("1"/"2") and lettered pads (A/K).
            //   2. numeric pad order: when every pad_number parses as a
            //      number, def i pairs with pad number i+1 (entities
            //      declare pins in package-numbering order — the
            //      established convention).
            //   3. emission order among still-unused pads.
            // Unmatched pads (thermal/EP) append AFTER the defs with no
            // net — they block copper but never steal a pin slot.
            let to_geom = |pad: &bhdl_components::types::component::FootprintPad| {
                crate::types::PadGeom {
                    width_mm: pad.width,
                    height_mm: pad.height,
                    shape: match pad.shape {
                        bhdl_components::types::component::PadShape::Circle => {
                            crate::types::PadShapeKind::Circle
                        }
                        bhdl_components::types::component::PadShape::Oval => {
                            crate::types::PadShapeKind::Oval
                        }
                        bhdl_components::types::component::PadShape::RoundedRectangle => {
                            crate::types::PadShapeKind::RoundRect
                        }
                        _ => crate::types::PadShapeKind::Rect,
                    },
                    drill_mm: pad.drill_diameter,
                }
            };
            let mut used = vec![false; fp.pads.len()];
            let numeric: Option<Vec<usize>> = {
                let mut idx: Vec<(usize, usize)> = Vec::new();
                let mut ok = true;
                for (i, pad) in fp.pads.iter().enumerate() {
                    match pad.pad_number.parse::<usize>() {
                        Ok(n) => idx.push((n, i)),
                        Err(_) => {
                            ok = false;
                            break;
                        }
                    }
                }
                if ok {
                    idx.sort();
                    Some(idx.into_iter().map(|(_, i)| i).collect())
                } else {
                    None
                }
            };
            let mut out: Vec<PinPosition> = Vec::new();
            for (di, &pid) in pin_defs.iter().enumerate() {
                let pname = netlist
                    .pins
                    .get(pid)
                    .map(|p| p.name.clone())
                    .unwrap_or_default();
                let by_name = fp
                    .pads
                    .iter()
                    .position(|p| p.pad_number.eq_ignore_ascii_case(&pname));
                // ONLY the provably-safe layers ship: exact name match
                // (pad_number == pin name — passives "1"/"2", lettered
                // A/K), else the original emission-index pairing.
                // Numeric reordering strategies measured worse-or-noise
                // under nondeterministic trials — parked until seed
                // determinism lands and pairing changes can be A/B'd
                // (see memory: pds pins on NC pads = real evidence the
                // index contract breaks on SOME footprint; find WHICH
                // deterministically).
                // Numeric order: def i pairs with the pad NUMBERED
                // i+1 (entities declare pins in package-numbering
                // order). SHIPPED ON A/B EVIDENCE at frozen seed 42:
                // byte-identical output on every board whose pads were
                // already index-aligned; strictly better on
                // intent_system_demo (copper clearance+mask violations
                // → CLEAN, silk 12→8) — the board whose footprint
                // emission order genuinely diverges from numbering.
                let chosen = by_name
                    .filter(|&i| !used[i])
                    .or_else(|| {
                        numeric
                            .as_ref()
                            .and_then(|ord| ord.get(di).copied())
                            .filter(|&i| !used[i])
                    })
                    .or_else(|| Some(di).filter(|&i| i < used.len() && !used[i]))
                    .or_else(|| used.iter().position(|u| !u));
                match chosen {
                    Some(i) => {
                        used[i] = true;
                        let pad = &fp.pads[i];
                        out.push(PinPosition {
                            pin_id: PinId::default(),
                            name: pname,
                            dx: pad.x_position,
                            dy: pad.y_position,
                            net: None,
                            pad: Some(to_geom(pad)),
                            unplaced: false,
                        });
                    }
                    None => {
                        // More defs than pads: the package cannot carry
                        // this entity. Loud error + unplaced marker —
                        // stacked placeholder pads at the origin used to
                        // ship as shorting_items; an honest unconnected
                        // pin is the truthful failure mode.
                        log::error!(
                            "entity '{}' pin '{}' has no pad slot in package '{}' \
                             ({} pins > {} pads) — pin left unplaced (no copper)",
                            module_def.name, pname, package,
                            pin_defs.len(), fp.pads.len()
                        );
                        out.push(PinPosition {
                            pin_id: PinId::default(),
                            name: pname,
                            dx: 0.0,
                            dy: 0.0,
                            net: None,
                            pad: None,
                            unplaced: true,
                        });
                    }
                }
            }
            for (i, pad) in fp.pads.iter().enumerate() {
                if !used[i] {
                    out.push(PinPosition {
                        pin_id: PinId::default(),
                        name: pad.pad_number.clone(),
                        dx: pad.x_position,
                        dy: pad.y_position,
                        net: None,
                        pad: Some(to_geom(pad)),
                            unplaced: false,
                        });
                }
            }
            out
        } else {
            // Fallback: estimate pin positions
            estimate_pin_positions(&pin_defs, netlist, width, height)
        };

        // The envelope must cover the ACTUAL emitted copper. The
        // footprint path folds pad protrusion in via footprint_extent,
        // but the estimated-pin fallback (unknown package) places pads
        // AT the assumed body edge — half of each pad sticks out of the
        // default envelope, so the legalizer's edge clamp certifies
        // spots whose copper violates copper_edge_clearance. Union the
        // envelope with every pad rect (pad-less estimated pins emit
        // the exporter's 0.5mm default square) so the legalization
        // guarantee is measured on real copper on both paths.
        let (width, height, bbox_dx, bbox_dy) = {
            let mut x_min = bbox_dx - width / 2.0;
            let mut x_max = bbox_dx + width / 2.0;
            let mut y_min = bbox_dy - height / 2.0;
            let mut y_max = bbox_dy + height / 2.0;
            for p in pins.iter().filter(|p| !p.unplaced) {
                let (pw, ph) = p
                    .pad
                    .as_ref()
                    .map(|pad| (pad.width_mm, pad.height_mm))
                    .unwrap_or((0.5, 0.5));
                x_min = x_min.min(p.dx - pw / 2.0);
                x_max = x_max.max(p.dx + pw / 2.0);
                y_min = y_min.min(p.dy - ph / 2.0);
                y_max = y_max.max(p.dy + ph / 2.0);
            }
            (
                x_max - x_min,
                y_max - y_min,
                (x_min + x_max) / 2.0,
                (y_min + y_max) / 2.0,
            )
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

        // Placement constraint: hard lock (chassis coordinates) wins,
        // then region binding (thermal boss / heatsink pad: WITHIN the
        // zone, position otherwise free), then free.
        let placement = fixed_placements
            .get(&instance.name)
            .map(|fp| PlacementConstraint::Fixed {
                x: fp.x_mm,
                y: fp.y_mm,
                theta: fp.rotation_deg.to_radians(),
            })
            .or_else(|| {
                config
                    .board_config
                    .placement_regions
                    .iter()
                    .find(|r| r.preferred_instances.iter().any(|n| n == &instance.name))
                    .map(|r| PlacementConstraint::PreferRegion {
                        region_name: r.name.clone(),
                    })
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
            bbox_dx,
            bbox_dy,
            pins,
            side: fixed_placements
                .get(&instance.name)
                .map(|fp| fp.side)
                .unwrap_or(BoardSide::Top),
            group: None,
            thermal_power_w: thermal_power,
            solved_current_a: simulation
                .and_then(|sim| sim.instance_currents.get(&instance.name))
                .map(|c| c.abs()),
            placement,
            x: 0.0,
            y: 0.0,
            theta: 0.0,
            density_inflation: 1.0,
            // Typed layout intents flow directly off the netlist instance
            // (synth step 4.1 landed: Phase 4.5 copies ExpansionInstance
            // intents onto Instance.layout_intents — no string-lift,
            // handshake §8.3). The intent-lowering pass turns these into
            // geometric constraints; empty for un-annotated components.
            layout_intents: instance.layout_intents.clone(),
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

        let solved_v = simulation.and_then(|sim| sim.net_voltages.get(&net_name).copied());
        let edge_swing = simulation.and_then(|sim| {
            sim.transients
                .iter()
                .filter(|t| t.net == net_name)
                .min_by_key(|t| if t.corner == "typ" { 0 } else { 1 })
                .and_then(|t| {
                    let (a, b) = (t.volts.first()?, t.volts.last()?);
                    let sw = (b - a).abs();
                    (sw > 0.1).then_some(sw)
                })
        });
        nets.push(PnrNet {
            id: NetId::default(),
            name: net_name,
            pins: Vec::new(),
            net_class: pnr_class,
            weight: intent_weight,
            required_trace_width_mm: trace_width,
            layer_constraint,
            intent,
            // Board-level @net intents (rare); populated by synth step 4.1.
            layout_intents: Vec::new(),
            plane_layer: None,
            plane_region: None,
            allowed_layers: None,
            solved_voltage_v: solved_v,
            edge_swing_v: edge_swing,
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

    if std::env::var("BHDL_PNR_DEBUG_NETS").is_ok() {
        for net in &nets {
            log::warn!(
                "NET '{}' class={:?} pins={} ",
                net.name, net.net_class, net.pins.len()
            );
        }
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
    let mut layer_stack = stackup::resolve_stackup(
        &config.board_config.stackup,
        components.len(),
        nets.len(),
        num_power,
        has_hs,
    );
    // Laminate override: geometry keeps the preset's build; εr/loss
    // come from the declared material (delay grading and the
    // impedance width floor inherit automatically).
    if let Some(mat) = &config.board_config.stackup_material {
        if let Err(e) = stackup::apply_material(&mut layer_stack, mat) {
            log::error!("{e}"); // CLI validates first; defensive here
        } else {
            log::info!(
                "stackup material: {} applied to all dielectrics",
                mat
            );
        }
    }

    // 10.5 Assign plane layers to nets. The stackup is INPUT (resolved
    // above); Ground-kind planes carry the ground net, each Power-kind
    // plane carries the fattest still-unassigned power rail (required
    // trace width = the flow classifier's own currency, name as the
    // deterministic tie-break). Assigned nets are plane-connected: the
    // router skips them, the exporter emits real fill copper, surface
    // pads get via drops.
    // Plane assignment is ON by default (BHDL_PNR_NO_PLANES=1 to
    // disable). The "multi-plane interplay" that kept this dark turned
    // out to be the orphan-stub prune discarding every drop stub (pad
    // and via anchors weren't counted) the moment any span of the net
    // was amputated — fixed; planes-on is oracle-clean or better than
    // planes-off on every board (pds 47→0 unc, uno copper CLEAN
    // unc 57→48, shorts zero).
    // Polygon boards: the fill fractures against the inset outline —
    // rectilinear only (chassis cutouts in practice); anything else
    // keeps plane fills gated off. Rails are NOT band-split on polygon
    // boards — one rail per Power layer (banding a concave board is a
    // later refinement).
    let (polygon_board, non_rectilinear) = match &config.board_config.outline {
        BoardOutline::Polygon(pts) => {
            let rect = crate::output::kicad::poly_is_rectilinear(pts);
            if !rect {
                log::warn!(
                    "plane fills disabled: polygon outline is not rectilinear \
                     (the fracture only clips axis-aligned cutouts)"
                );
            }
            (true, !rect)
        }
        _ => (false, false),
    };
    if std::env::var("BHDL_PNR_NO_PLANES").is_err() && !non_rectilinear {
        let gnd_idx = nets
            .iter()
            .position(|n| matches!(n.net_class, PnrNetClass::Ground));
        let mut power_by_fat: Vec<usize> = nets
            .iter()
            .enumerate()
            .filter(|(_, n)| matches!(n.net_class, PnrNetClass::Power { .. }))
            .map(|(i, _)| i)
            .collect();
        // Fattest first, PIN COUNT as tie-break: "give 5V the plane"
        // means the rail with the most consumers, not the input jack
        // that happens to share its IPC width (DC_IN, 3 pins, was
        // beating VCC_5V, 29 pins, on the name tie).
        power_by_fat.sort_by(|&a, &b| {
            nets[b]
                .required_trace_width_mm
                .partial_cmp(&nets[a].required_trace_width_mm)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| nets[b].pins.len().cmp(&nets[a].pins.len()))
                .then_with(|| nets[a].name.cmp(&nets[b].name))
        });
        let mut next_power = power_by_fat.into_iter();
        for (li, layer) in layer_stack.layers.iter().enumerate() {
            match layer.kind {
                LayerKind::Ground => {
                    if let Some(gi) = gnd_idx {
                        if nets[gi].plane_layer.is_none() {
                            nets[gi].plane_layer = Some(li);
                            log::info!(
                                "plane assignment: {} -> '{}' (ground)",
                                layer.name, nets[gi].name
                            );
                        }
                    }
                }
                LayerKind::Power => {
                    // Distribute rails ACROSS Power layers before
                    // splitting within one: with two Power layers the
                    // top rails each get a whole plane; splits only
                    // absorb the overflow. (Greedy 4-per-first-layer
                    // starved the second plane entirely.)
                    let layers_left = layer_stack.layers[li..]
                        .iter()
                        .filter(|l| l.kind == LayerKind::Power)
                        .count()
                        .max(1);
                    let rails_left = next_power.len();
                    let take = if polygon_board {
                        1 // no band regions on polygon boards
                    } else {
                        ((rails_left + layers_left - 1) / layers_left).min(4)
                    };
                    let mut took = 0;
                    while took < take {
                        let Some(pi) = next_power.next() else { break };
                        nets[pi].plane_layer = Some(li);
                        log::info!(
                            "plane assignment: {} -> '{}' ({:.2}mm IPC width{})",
                            layer.name,
                            nets[pi].name,
                            nets[pi].required_trace_width_mm,
                            if took == 0 { "" } else { ", split region" }
                        );
                        took += 1;
                    }
                }
                _ => {}
            }
        }
    }

    // 11. Auto-size board outline
    let outline = match &config.board_config.outline {
        BoardOutline::AutoSize => {
            let total_area: f64 = components
                .iter()
                .map(|c| c.width_mm * c.height_mm)
                .sum();
            // Board needs enough space for components + routing channels.
            // Estimate: sum of component areas needs 5-6× for routing.
            // Also consider that components have courtyard around them.
            let n_comps = components.len() as f64;
            let avg_dim = (total_area / n_comps.max(1.0)).sqrt();
            // Dense boards need routing headroom that scales faster
            // than component count: at 60+ components the diffuse
            // congestion tail (many nets each losing one pin) is an
            // AREA problem — negotiation converges, sinks are simply
            // unreachable at grid granularity.
            let headroom = if n_comps > 40.0 { 1.15 } else { 1.0 };
            let side =
                ((n_comps.sqrt().ceil() + 2.0) * (avg_dim + 2.0) * headroom).max(50.0);
            BoardOutline::Rectangle {
                width_mm: side,
                height_mm: side,
            }
        }
        other => other.clone(),
    };

    let board_config = BoardConfig {
        outline,
        si_return_cost: false,
        // Courtyard keepout (per side) follows the density level the
        // footprints were generated at. BHDL_PNR_COURTYARD_BOOST adds
        // to it (mm/side) — the aisle-reservation experiment lever:
        // wider courtyards = wider escape corridors between parts.
        courtyard_excess_mm: config.density_level.courtyard_excess_mm()
            + std::env::var("BHDL_PNR_COURTYARD_BOOST")
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0),
        ..config.board_config
    };

    // Interface constraints: parse the synth side's `intf_const__*` module
    // attributes (shipped v0.8) into typed net/signal constraints, with
    // live pin-path→NetId resolution. The expansion-intent half is added
    // later by `intent::lower_board_intents` (it appends to this vec).
    let (mut iface_constraints, iface_diags) =
        extract_interface_constraints(netlist, &id_map, &inst_pin_to_net);
    for d in &iface_diags {
        log::warn!("interface constraints: {d}");
    }
    if !iface_constraints.is_empty() {
        log::info!(
            "interface constraints: {} from intf_const__* attributes",
            iface_constraints.len()
        );
    }

    // Impedance → trace-width floor (constraint synthesis v1): the
    // stackup's outer dielectric + IPC-2141 microstrip give the width
    // that hits the target. Differential targets count per-line as
    // Z0 ≈ Zdiff/2 (loosely-coupled approximation). A target the
    // stackup can't reach in a routable width floors NOTHING — the
    // sign-off report shows the honest FAIL instead of shipping a
    // fabricated width.
    {
        use crate::constraint::Constraint;
        let h = layer_stack.dielectrics.first().map(|d| (d.thickness_mm, d.er));
        let t = layer_stack.layers.first().map(|l| l.thickness_mm).unwrap_or(0.035);
        if let Some((h_mm, er)) = h {
            let diff_nets: std::collections::HashSet<NetId> = iface_constraints
                .iter()
                .filter_map(|c| match c {
                    Constraint::DiffPair { p_net, n_net, .. } => Some([*p_net, *n_net]),
                    _ => None,
                })
                .flatten()
                .collect();
            for c in &iface_constraints {
                if let Constraint::Impedance { net, target_ohms, .. } = c {
                    // P3: a DIFF member's width comes from the JOINT
                    // coupled design point (w, s=1.5w) — solving the
                    // uncoupled Zdiff/2 then asking for a gap is
                    // degenerate (lands at s = infinity).
                    if diff_nets.contains(net) {
                        if let Some((w, _s)) = crate::routing::measure::diff_pair_geometry(
                            *target_ohms as f64, h_mm, t, er, true,
                        ) {
                            if w <= 2.0 {
                                if let Some(n) = nets.iter_mut().find(|n| n.id == *net) {
                                    if w > n.required_trace_width_mm {
                                        log::info!(
                                            "impedance floor (coupled): '{}' Zdiff {:.0}Ω → {:.2}mm trace (h={h_mm}mm er={er})",
                                            n.name, *target_ohms as f64, w
                                        );
                                        n.required_trace_width_mm = w;
                                    }
                                }
                                continue;
                            }
                        }
                    }
                    let z0 = if diff_nets.contains(net) {
                        *target_ohms as f64 / 2.0
                    } else {
                        *target_ohms as f64
                    };
                    match crate::routing::measure::microstrip_width_for(z0, h_mm, t, er) {
                        Some(w) if w <= 2.0 => {
                            if let Some(n) = nets.iter_mut().find(|n| n.id == *net) {
                                if w > n.required_trace_width_mm {
                                    log::info!(
                                        "impedance floor: '{}' {:.0}Ω → {:.2}mm trace \
                                         (h={h_mm}mm er={er})",
                                        n.name, z0, w
                                    );
                                    n.required_trace_width_mm = w;
                                }
                            }
                        }
                        _ => {
                            if let Some(n) = nets.iter().find(|n| n.id == *net) {
                                log::warn!(
                                    "impedance floor: '{}' {:.0}Ω unreachable on this \
                                     stackup (outer dielectric {h_mm}mm) — no width \
                                     applied, sign-off will show the miss",
                                    n.name, z0
                                );
                            }
                        }
                    }
                }
            }
        }
        // P3 — DERIVED PAIR GAP: the interface lowering stamps a
        // conventional 0.15mm spacing; where the pair carries a
        // differential-impedance target, the stackup DEMANDS a
        // specific edge-coupling gap (Real-Data policy: the rule
        // exists because the physics does). User-declared spacings
        // arrive through other constraint forms and are not touched.
        if let Some((h_mm, er)) = h {
            let zdiff_of: std::collections::HashMap<NetId, f64> = iface_constraints
                .iter()
                .filter_map(|c| match c {
                    Constraint::Impedance { net, target_ohms, .. } => {
                        Some((*net, *target_ohms as f64))
                    }
                    _ => None,
                })
                .collect();
            let width_of: std::collections::HashMap<NetId, f64> = nets
                .iter()
                .map(|n| (n.id, n.required_trace_width_mm))
                .collect();
            for c in iface_constraints.iter_mut() {
                if let Constraint::DiffPair { p_net, n_net, spacing_mm, source, .. } = c {
                    if (*spacing_mm as f64 - 0.15).abs() > 1e-6
                        || !source.intent_kind.contains("differential")
                    {
                        continue; // not the lowering default — designer's word wins
                    }
                    let Some(&zdiff) = zdiff_of.get(p_net).or_else(|| zdiff_of.get(n_net))
                    else {
                        continue; // no impedance target — nothing demands a gap
                    };
                    let _ = width_of;
                    let min_w = config.board_config.min_trace_width_mm;
                    if let Some((w, s)) = crate::routing::measure::diff_pair_geometry(
                        zdiff, h_mm, t, er, true,
                    ) {
                        // The design point must be FABBABLE: below the
                        // min trace width, clamp w and re-solve the
                        // gap at the real width. When the target sits
                        // at/above 2·Z0(min_w) no finite gap reaches
                        // it — decouple (wide gap) and let the
                        // sign-off grade the honest asymptote.
                        let (w_eff, s_eff) = if w < min_w {
                            match crate::routing::measure::diff_gap_for(
                                zdiff, min_w, h_mm, t, er, true,
                            ) {
                                Some(s2) => (min_w, s2),
                                None => (min_w, 1.0),
                            }
                        } else {
                            (w, s)
                        };
                        let s_eff = s_eff.clamp(0.09, 1.0);
                        log::info!(
                            "derived pair gap: Zdiff {zdiff:.0}Ω coupled design point \
                             w={w_eff:.2}mm s={s_eff:.2}mm on outer (h={h_mm}mm er={er}) — was default 0.15",
                        );
                        *spacing_mm = s_eff as f32;
                    }
                }
            }
        }
        // P3 — DERIVED SKEW BUDGET: where a pair member's driver has a
        // MEASURED IBIS edge (a solved transient trace), the skew
        // budget the physics demands is a fraction of that edge —
        // t_rise/10 in time, graded as routed DELAY. Real-Data policy:
        // no trace, no time budget (the mm default stands). Only the
        // lowering default is replaced; declared budgets win.
        if let Some(sim) = simulation {
            let name_of: std::collections::HashMap<NetId, &str> = nets
                .iter()
                .map(|n| (n.id, n.name.as_str()))
                .collect();
            for c in iface_constraints.iter_mut() {
                if let Constraint::DiffPair { p_net, n_net, length_match_ps, source, .. } = c
                {
                    if length_match_ps.is_some() || !source.intent_kind.contains("differential")
                    {
                        continue;
                    }
                    let trace = [p_net, n_net].iter().find_map(|nid| {
                        let name = name_of.get(nid)?;
                        sim.transients
                            .iter()
                            .filter(|t| t.net == *name)
                            .min_by_key(|t| if t.corner == "typ" { 0 } else { 1 })
                    });
                    let Some(tr) = trace else { continue };
                    let Some(t_rise) =
                        crate::routing::measure::rise_time_ps(&tr.times, &tr.volts)
                    else {
                        continue;
                    };
                    let budget = (t_rise / 10.0).max(1.0);
                    log::info!(
                        "derived skew budget: pair edge measured {t_rise:.0}ps ({} corner {}) → length_match {budget:.0}ps (t_rise/10)",
                        tr.net, tr.corner
                    );
                    *length_match_ps = Some(budget as f32);
                }
            }
        }
    }

    // Layer rules → allowed-layer masks. Pad layers are physics: a
    // rule that excludes a pad's layer is relaxed to include it, with
    // a warning (the alternative ships an unroutable pin).
    {
        use crate::constraint::{Constraint, LayerBind};
        let n_layers = layer_stack.layers.len();
        let signal_layers: Vec<usize> = layer_stack.signal_layer_indices();
        for c in &iface_constraints {
            let Constraint::LayerRule { net, bind, .. } = c else { continue };
            let mut allowed: Vec<usize> = match bind {
                LayerBind::Top => vec![0],
                LayerBind::Bottom => vec![n_layers - 1],
                LayerBind::Outer => vec![0, n_layers - 1],
                LayerBind::Inner => signal_layers
                    .iter()
                    .copied()
                    .filter(|&l| l != 0 && l != n_layers - 1)
                    .collect(),
            };
            if allowed.is_empty() {
                log::warn!(
                    "layer rule: no such signal layers on this stackup — rule ignored"
                );
                continue;
            }
            let Some(n) = nets.iter_mut().find(|n| n.id == *net) else { continue };
            // Pads live where they live.
            for &(comp_id, pin_id) in &n.pins {
                let Some(comp) = components.iter().find(|c| c.id == comp_id) else {
                    continue;
                };
                if comp.pins.iter().any(|p| {
                    p.pin_id == pin_id
                        && p.pad.as_ref().and_then(|pd| pd.drill_mm).is_some()
                }) {
                    continue; // THT reaches every layer
                }
                let pad_layer = match comp.side {
                    BoardSide::Top => 0,
                    BoardSide::Bottom => n_layers - 1,
                };
                if !allowed.contains(&pad_layer) {
                    log::warn!(
                        "layer rule on '{}': pad on layer {pad_layer} conflicts \
                         with the rule — pad layer added (rule relaxed)",
                        n.name
                    );
                    allowed.push(pad_layer);
                }
            }
            allowed.sort_unstable();
            allowed.dedup();
            log::info!(
                "layer rule: '{}' restricted to layers {:?}",
                n.name, allowed
            );
            n.allowed_layers = Some(allowed);
        }
    }

    Ok(Board {
        config: board_config,
        layer_stack,
        components,
        nets,
        groups,
        placement_recipes: std::collections::BTreeMap::new(), // populated by caller
        // Net/signal constraints from interface `intf_const__*` attributes
        // (above); expansion-intent constraints appended by
        // `intent::lower_board_intents`.
        constraints: iface_constraints,
    })
}

// ── Net lookup tables ────────────────────────────────────────────────

/// Parse every instance's `intf_const__*` module attributes into typed
/// net/signal constraints, resolving dotted leaf pin-paths
/// (`ddr.lane0.DQ0`) to P&R `NetId`s.
///
/// Resolution chain: instance + pin-path → module pin (by name) →
/// `inst_pin_to_net` → `NlNetId` → `id_map` → `NetId`. Paths that don't
/// resolve are dropped with a diagnostic (warn-and-degrade).
fn extract_interface_constraints(
    netlist: &Netlist,
    id_map: &IdMap,
    inst_pin_to_net: &HashMap<(InstanceId, NlPinId), NlNetId>,
) -> (Vec<crate::constraint::Constraint>, Vec<String>) {
    use crate::intent::interface_constraints::{
        lower_interface_constraints, parse_interface_attrs,
    };

    let mut all = Vec::new();
    let mut diags = Vec::new();

    for (inst_id, instance) in netlist.instances.iter() {
        let module = match netlist.modules.get(instance.definition) {
            Some(m) => m,
            None => continue,
        };
        if module.attributes.is_empty() && instance.attributes.is_empty() {
            continue;
        }

        // Module attrs carry interface-emitted constraints; INSTANCE
        // attrs carry entity `attribute intf_const__…` statements
        // (stamped per instance by the synthesizer). Read both —
        // instance wins on key collision.
        let mut merged: std::collections::BTreeMap<&str, &str> = module
            .attributes
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        for (k, v) in &instance.attributes {
            merged.insert(k.as_str(), v.as_str());
        }
        let attrs: Vec<(&str, &str)> = merged.into_iter().collect();
        let (parsed, pdiags) = parse_interface_attrs(attrs.iter().copied());
        diags.extend(pdiags);
        if parsed.is_empty() {
            continue;
        }

        // Decode the synth side's provenance sidecar (handshake §10/§11),
        // if present, to enrich each constraint's source with the `.bhdl`
        // line + declaring interface scope. Absent / malformed → empty map
        // (sources keep their pin-path-only provenance, back-compat).
        let provenance: bhdl_common::constraint_provenance::ConstraintProvenanceMap = module
            .attributes
            .get(bhdl_common::constraint_provenance::INTERFACE_CONSTRAINT_PROVENANCE_ATTR)
            .and_then(|json| serde_json::from_str(json).ok())
            .unwrap_or_default();

        // Resolver: dotted leaf pin-path → NetId, scoped to this instance.
        let resolve = |path: &str| -> Option<NetId> {
            let to_pnr = |nl_net: &NlNetId| -> Option<NetId> {
                let idx = id_map.net_to_idx.get(nl_net)?;
                id_map.net_ids.get(*idx).copied()
            };
            // Primary: the pin INSTANCE carries the connected net
            // directly (connection resolution binds there; the
            // (instance, pin_def) side map misses connections recorded
            // only on pin instances).
            netlist
                .pin_instances
                .values()
                .find_map(|pi| {
                    if pi.instance != inst_id {
                        return None;
                    }
                    let pdef = netlist.pins.get(pi.pin_def)?;
                    if pdef.name != path {
                        return None;
                    }
                    to_pnr(&pi.net?)
                })
                .or_else(|| {
                    // Fallback: (instance, pin_def) side map. Scan EVERY
                    // pin def with this name — duplicate defs happen and
                    // the connection binds to one of them.
                    module
                        .pins
                        .iter()
                        .copied()
                        .filter(|&pid| {
                            netlist
                                .pins
                                .get(pid)
                                .map(|p| p.name == path)
                                .unwrap_or(false)
                        })
                        .find_map(|pid| to_pnr(inst_pin_to_net.get(&(inst_id, pid))?))
                })
        };

        let (cons, ldiags) =
            lower_interface_constraints(&parsed, &instance.name, &resolve, &provenance);
        all.extend(cons);
        diags.extend(ldiags);
    }

    (all, diags)
}

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
        NetClass::Power { voltage, current: declared } => {
            // Prefer the source-declared per-rail budget (`@ I`). Only when it
            // is absent fall back to the sim-derived estimate (and, last, 0.5A)
            // for trace-width sizing.
            let current = declared.unwrap_or_else(|| {
                simulation
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
                    .unwrap_or(0.5)
            });
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
    // BTreeMap: group order feeds straight into block order and thus the
    // whole placement — HashMap iteration here made layout flap per process.
    let mut parent_children: std::collections::BTreeMap<String, Vec<usize>> =
        std::collections::BTreeMap::new();
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

/// Compute the full footprint extent encompassing body and all pads.
///
/// For gull-wing packages (SOIC, SOT), pads extend beyond the IC body.
/// The component outline should encompass everything.
fn footprint_extent(fp: &bhdl_components::ComponentFootprint) -> (f64, f64, f64, f64) {
    if fp.pads.is_empty() {
        return (fp.body_width, fp.body_height, 0.0, 0.0);
    }

    let mut x_min = -fp.body_width / 2.0;
    let mut x_max = fp.body_width / 2.0;
    let mut y_min = -fp.body_height / 2.0;
    let mut y_max = fp.body_height / 2.0;

    for pad in &fp.pads {
        x_min = x_min.min(pad.x_position - pad.width / 2.0);
        x_max = x_max.max(pad.x_position + pad.width / 2.0);
        y_min = y_min.min(pad.y_position - pad.height / 2.0);
        y_max = y_max.max(pad.y_position + pad.height / 2.0);
    }

    // (w, h, center offset) — dropping the offset let asymmetric
    // packages (offset tabs) keep copper outside the assumed centered
    // envelope.
    (
        x_max - x_min,
        y_max - y_min,
        (x_min + x_max) / 2.0,
        (y_min + y_max) / 2.0,
    )
}

/// Check if an instance is a power/ground symbol (not a physical component).
///
/// Real physical components have `component_class` set by the synthesizer
/// (e.g., "resistor", "capacitor", "voltage_regulator"). Power symbols don't.
fn is_power_symbol(
    module_def: &bhdl_netlist::ModuleDefinition,
    instance: &bhdl_netlist::Instance,
    netlist: &Netlist,
) -> bool {
    // component_class may live on the INSTANCE (stamped by synthesis /
    // GLACIER selection) or only on the MODULE — same-file entities keep
    // it at the entity level. Checking the instance alone silently
    // dropped every same-file IC from the board (u1/SinkDriver on the
    // led_derive fixture was never placed, its netted pads never routed).
    let cc = instance
        .attributes
        .get("component_class")
        .or_else(|| module_def.attributes.get("component_class"));
    match cc {
        Some(c) => {
            let c = c.trim_matches('"');
            matches!(c, "power_source" | "ground_symbol" | "power" | "ground" | "power_symbol" | "net")
        }
        // No class anywhere: infer by SHAPE. A module with two or more
        // real pins, at least one of them a signal, is a part — the
        // old "no class = symbol" rule silently dropped every
        // class-less same-file entity from the board (they needed an
        // explicit `attribute component_class = …` to exist at all).
        // Rail symbols keep their signature: ≤1 pin, or power/ground
        // pins only.
        None => {
            let mut real_pins = 0usize;
            let mut signal_pins = 0usize;
            for &pid in &module_def.pins {
                let Some(p) = netlist.pins.get(pid) else { continue };
                if p.is_virtual {
                    continue;
                }
                real_pins += 1;
                if !matches!(
                    p.pin_type,
                    bhdl_netlist::PinType::Power | bhdl_netlist::PinType::Ground
                ) {
                    signal_pins += 1;
                }
            }
            real_pins < 2 || signal_pins == 0
        }
    }
}

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

fn default_package_for_category(category: &str, pin_count: usize) -> String {
    // Grow the default to FIT the pin count: a class default smaller
    // than the entity's pin list leaves surplus pins unplaced (no
    // copper) — the honest failure, but a fitting default avoids it.
    let ic_by_pins = |n: usize| -> &'static str {
        match n {
            0..=8 => "SOIC-8",
            9..=14 => "SOIC-14",
            15..=16 => "SOIC-16",
            17..=32 => "TQFP-32",
            _ => "TQFP-64",
        }
    };
    match category {
        "resistor" | "capacitor" | "ferrite_bead" => "0603",
        "inductor" => "1210",
        "diode" | "led" | "tvs_diode" if pin_count <= 3 => "SOT-23",
        "diode" | "led" | "tvs_diode" => "SOT-23-6",
        "voltage_regulator" if pin_count <= 4 => "SOT-223",
        "voltage_regulator" if pin_count <= 5 => "SOT-23-5",
        "voltage_regulator" if pin_count <= 6 => "SOT-23-6",
        "voltage_regulator" => ic_by_pins(pin_count),
        "microcontroller" => "TQFP-64",
        _ => ic_by_pins(pin_count),
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
                pad: None,
                            unplaced: false,
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
                pad: None,
                            unplaced: false,
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
        .filter(|net| matches!(net.net_class, NetClass::Power { .. }))
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
