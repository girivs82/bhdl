//! Netlist to SPICE Circuit Converter
//! 
//! This module provides enhanced conversion from BHDL netlists to SPICE circuits,
//! leveraging the component model extraction system for accurate models.

use std::collections::HashMap;
use anyhow::{Result, Context};
use log::{debug, info, warn};

use crate::{
    Circuit, ComponentModelExtractor, ExtractedModel,
    circuit::{
        DeviceKind,
        META_PARENT_INSTANCE, META_DECOMPOSITION_ROLE, META_COMPONENT_CLASS,
        META_RDS_ON, META_F_SW, META_T_SW, META_I_QUIESCENT, META_MODEL_I_IN,
        META_TOLERANCE, META_POWER_RATING, META_ESR, META_ESL, META_VOLTAGE_RATING, META_DCR,
        META_SATURATION_CURRENT, META_EMISSION_COEFFICIENT, META_THERMAL_VOLTAGE,
        META_FORWARD_VOLTAGE, META_FORWARD_CURRENT,
        META_MAX_CURRENT, META_MAX_VOLTAGE, META_MAX_POWER, META_TEMP_MIN, META_TEMP_MAX,
        META_VARIANT,
    },
    model_factory::SpiceModelFactory,
    models::SpiceModel,
};
use bhdl_netlist::{
    Netlist, NetId, InstanceId,
    ConnectionPoint, ModuleKind, ModuleDefinition,
};

/// Pin connection info for an instance, including pin type and direction
struct PinNetInfo {
    net_id: NetId,
    net_name: String,
    pin_name: String,
    pin_type: bhdl_netlist::PinType,
    pin_direction: bhdl_netlist::PinDirection,
}

/// Parse a resistance/voltage value string ("31.6kΩ", "10k", "0.8V") for
/// the closed-loop divider derivation.
fn parse_res_val(txt: &str) -> Option<f64> {
    let t = txt.trim().trim_end_matches('V').trim_end_matches('Ω')
        .trim_end_matches("ohm").trim();
    let (num, mult) = match t.chars().last()? {
        'k' | 'K' => (&t[..t.len() - 1], 1e3),
        'M' => (&t[..t.len() - 1], 1e6),
        'm' => (&t[..t.len() - 1], 1e-3),
        'u' | 'µ' => (&t[..t.len() - 1], 1e-6),
        _ => (t, 1.0),
    };
    num.trim().parse::<f64>().ok().map(|v| v * mult)
}

/// Find the FB divider on `fb_net`: exactly one two-terminal resistor to a
/// non-ground net (Rtop) and one to ground (Rbot). Anything else → None
/// (ambiguous networks are never guessed into a closed-loop model).
fn divider_at(netlist: &Netlist, fb_net: NetId) -> Option<(f64, f64)> {
    let mut rt: Option<f64> = None;
    let mut rb: Option<f64> = None;
    for inst in netlist.instances.values() {
        if inst.attributes.get("component_class").map(String::as_str) != Some("resistor") {
            continue;
        }
        let pins: Vec<Option<NetId>> = netlist
            .pin_instances
            .values()
            .filter(|pi| {
                netlist.instances.iter().any(|(id, i)| {
                    std::ptr::eq(i, inst) && pi.instance == id
                })
            })
            .map(|pi| pi.net)
            .collect();
        if pins.len() != 2 {
            continue;
        }
        let (a, b) = (pins[0], pins[1]);
        let other = match (a, b) {
            (Some(x), Some(y)) if x == fb_net => y,
            (Some(x), Some(y)) if y == fb_net => x,
            _ => continue,
        };
        let val = inst.attributes.get("value").and_then(|v| parse_res_val(v));
        let Some(val) = val.filter(|v| *v > 0.0) else { continue };
        let grounded = matches!(
            netlist.nets.get(other).map(|n| &n.net_class),
            Some(bhdl_netlist::NetClass::Ground)
        );
        if grounded {
            if rb.replace(val).is_some() {
                return None; // two bottom legs — ambiguous
            }
        } else if rt.replace(val).is_some() {
            return None; // two top legs — ambiguous
        }
    }
    Some((rt?, rb?))
}

/// Enhanced netlist converter with proper SPICE model creation
pub struct NetlistToSpiceConverter {
    /// Component model extractor
    model_extractor: ComponentModelExtractor,
    /// Model factory for creating SPICE models
    model_factory: SpiceModelFactory,
    /// Cached models for instances
    instance_models: HashMap<InstanceId, Box<dyn SpiceModel>>,
    /// Symbol table data from analyzer (if available)
    symbol_table: HashMap<String, HashMap<String, String>>,
    /// Component registry for type lookup
    component_registry: crate::component_registry::ComponentRegistry,
    /// Pre-evaluated device-model overrides keyed by entity name
    /// (Vendor_Simulation_Blocks.md §5). When an entity declares a
    /// `simulation { model { } }` block, its evaluated `node source` voltages
    /// replace the hardcoded regulator decomposition's output voltage. Empty
    /// ⇒ every regulator uses the hardcoded fallback.
    model_overrides: HashMap<String, bhdl_common::model::EvaluatedModel>,
    /// Vendor IBIS references by entity name (§5 vendor-model form #1).
    ibis_models: HashMap<String, Vec<bhdl_common::model::IbisRef>>,
    /// Directory .ibs paths resolve against.
    ibis_base_dir: Option<std::path::PathBuf>,
    /// Scheduled buffer transitions collected from `ibis_wave_<PIN>`
    /// directives during convert(); consumed by the transient command
    /// via [`take_ibis_drives`][Self::take_ibis_drives].
    ibis_drives: Vec<crate::ibis_transient::IbisDrive>,
    /// When set, every IBIS stamp uses THIS silicon corner instead of the
    /// one each `ibis` stanza declares — the corner-sweep surface.
    ibis_corner_override: Option<crate::ibis::Corner>,
}

impl NetlistToSpiceConverter {
    /// Create new converter
    pub fn new() -> Self {
        Self {
            model_extractor: ComponentModelExtractor::new(),
            model_factory: SpiceModelFactory::new(),
            instance_models: HashMap::new(),
            symbol_table: HashMap::new(),
            component_registry: crate::component_registry::ComponentRegistry::new(),
            model_overrides: HashMap::new(),
            ibis_models: HashMap::new(),
            ibis_base_dir: None,
            ibis_drives: Vec::new(),
            ibis_corner_override: None,
        }
    }

    /// Override the silicon corner for ALL IBIS stamps (sweep surface).
    /// None (default) = each stanza's own declared corner.
    pub fn set_ibis_corner_override(&mut self, corner: Option<crate::ibis::Corner>) {
        self.ibis_corner_override = corner;
    }

    /// Take the IBIS drives built from `ibis_wave_<PIN>` directives during
    /// the last convert() — the input to `run_transient_ibis`.
    pub fn take_ibis_drives(&mut self) -> Vec<crate::ibis_transient::IbisDrive> {
        std::mem::take(&mut self.ibis_drives)
    }

    /// Set symbol table data from analyzer
    pub fn set_symbol_table(&mut self, symbol_table: HashMap<String, HashMap<String, String>>) {
        self.symbol_table = symbol_table;
    }

    /// Set pre-evaluated device-model overrides (§5), keyed by entity name.
    /// Register vendor IBIS model references (§5 form #1): entity name →
    /// IbisRef, plus the directory .ibs paths resolve against (the board
    /// source's dir). Files are parsed lazily at convert() and cached.
    pub fn set_ibis_models(
        &mut self,
        refs: std::collections::HashMap<String, Vec<bhdl_common::model::IbisRef>>,
        base_dir: std::path::PathBuf,
    ) {
        self.ibis_models = refs;
        self.ibis_base_dir = Some(base_dir);
    }

    pub fn set_model_overrides(
        &mut self,
        overrides: HashMap<String, bhdl_common::model::EvaluatedModel>,
    ) {
        self.model_overrides = overrides;
    }
    
    /// Convert BHDL netlist to SPICE circuit with proper models
    pub fn convert(&mut self, netlist: &Netlist) -> Result<Circuit> {
        self.ibis_drives.clear();
        let mut circuit = Circuit::new();

        info!("Converting netlist to SPICE circuit with {} instances", netlist.instances.len());

        // Step 1: Add all nets as nodes
        self.add_nets_as_nodes(&mut circuit, netlist)?;

        // Step 2: Process regulators first (they define voltage sources on output nets)
        let mut vsource_nodes: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut deferred: Vec<(InstanceId, &bhdl_netlist::Instance)> = Vec::new();

        for (instance_id, instance) in &netlist.instances {
            let module = netlist.modules.get(instance.definition);
            let is_regulator = module.map(|m| {
                self.component_registry.get_component_type(&m.name, &instance.attributes)
                    == Some(crate::components::ComponentType::VoltageRegulator)
            }).unwrap_or(false);

            if is_regulator {
                // Use pin types from stdlib to identify regulator output
                let pin_info = Self::get_pin_net_info(netlist, instance_id);
                let reg_output_net = pin_info.iter()
                    .find(|p| p.pin_type == bhdl_netlist::PinType::Power
                           && p.pin_direction == bhdl_netlist::PinDirection::Out)
                    .map(|p| p.net_name.clone());

                info!("Regulator pre-scan for {}: output_net={:?} (from pin types: {:?})",
                      instance.name, reg_output_net,
                      pin_info.iter().map(|p| format!("{}:{:?}/{:?}→{}", p.pin_name, p.pin_type, p.pin_direction, p.net_name)).collect::<Vec<_>>());

                match self.process_instance(&mut circuit, netlist, instance_id, instance) {
                    Ok(_) => {
                        info!("Successfully processed regulator: {}", instance.name);
                        if let Some(output_net) = reg_output_net {
                            vsource_nodes.insert(output_net.clone());
                            info!("Regulator {} drives output net: {}", instance.name, output_net);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to process regulator {}: {}", instance.name, e);
                    }
                }
            } else {
                deferred.push((instance_id, instance));
            }
        }

        // Step 2b: Propagate vsource_nodes through inductors from virtual pin
        //          expansion.  After expansion, a buck's SW node is in vsource_nodes
        //          but the original VOUT net (connected via inductor) is not.  We
        //          need to mark those too so power symbols don't create redundant
        //          voltage sources.
        for (instance_id, instance) in &netlist.instances {
            let module = netlist.modules.get(instance.definition);
            let is_inductor = module.map(|m| {
                m.name.contains("Inductor") || m.name.contains("Ind")
                    || instance.attributes.get("component_class")
                        .map(|c| c == "inductor")
                        .unwrap_or(false)
            }).unwrap_or(false);

            if is_inductor {
                let pin_nets = Self::get_pin_net_info(netlist, instance_id);
                let net_names: Vec<String> = pin_nets.iter().map(|p| p.net_name.clone()).collect();
                // If one end is already in vsource_nodes, add the other end too
                for i in 0..net_names.len() {
                    if vsource_nodes.contains(&net_names[i]) {
                        for j in 0..net_names.len() {
                            if i != j && !vsource_nodes.contains(&net_names[j]) {
                                info!("Inductor {} propagates voltage-source-driven status: {} → {}",
                                      instance.name, net_names[i], net_names[j]);
                                vsource_nodes.insert(net_names[j].clone());
                            }
                        }
                    }
                }
            }
        }

        // Step 3: Process remaining instances, skipping power symbols whose nets are
        //         already driven by a regulator voltage source
        for (instance_id, instance) in deferred {
            let module = netlist.modules.get(instance.definition);
            let is_power_symbol = module.map(|m| Self::is_power_symbol(&m.name)).unwrap_or(false);

            if is_power_symbol {
                let connected = self.get_connected_nets(netlist, instance_id).unwrap_or_default();
                if let Some((net_id, net_name)) = connected.first() {
                    // Skip if net already driven by a regulator voltage source
                    if vsource_nodes.contains(net_name) {
                        info!("Skipping power symbol {} — net '{}' already driven by regulator",
                              instance.name, net_name);
                        continue;
                    }
                    // Skip if net is floating (only the power symbol is connected, no load)
                    let net_connection_count = netlist.nets.get(*net_id)
                        .map(|n| n.connections.len())
                        .unwrap_or(0);
                    if net_connection_count <= 1 {
                        info!("Skipping power symbol {} — net '{}' is floating ({} connections)",
                              instance.name, net_name, net_connection_count);
                        continue;
                    }
                }
            }

            match self.process_instance(&mut circuit, netlist, instance_id, instance) {
                Ok(_) => info!("Successfully processed instance: {}", instance.name),
                Err(e) => {
                    warn!("Failed to process instance {}: {}", instance.name, e);
                }
            }
        }

        // Step 3b: Energise declared power rails. A board `power V = 5V`
        //          declaration sets the net's class to Power(v) but — unlike a
        //          `+5V` power-symbol instance — adds no source, so without
        //          this the rail is undriven and the DC solve is trivial
        //          (every node 0 V, no current, no stress). Add a VoltageSource
        //          from each Power net to GND, skipping any net already driven
        //          by a regulator (`vsource_nodes`) or a power symbol (an
        //          existing VoltageSource branch) so a rail is never
        //          double-driven.
        //
        //          Ports doctrine: the ideal source is the BOUNDARY CONDITION
        //          of a board port — power enters through the port, so the
        //          source belongs to that boundary object (the schematic layer
        //          draws it as the port's boundary flag). Every declared rail
        //          lowers to a Port on the top-level module, so an undriven
        //          Power net normally has one; rails that only exist by net-
        //          name heuristics (VCC/VDD/VIN substrings, KiCad imports)
        //          have no Port and keep the legacy net-level attribution.
        let board_ports: std::collections::HashMap<&str, bhdl_netlist::types::NetId> = netlist
            .ports
            .iter()
            .filter(|(_, p)| Some(p.module) == netlist.top_level_module && p.net.is_some())
            .map(|(_, p)| (p.name.as_str(), p.net.unwrap()))
            .collect();
        let mut driven: std::collections::HashSet<crate::circuit::NodeId> = circuit
            .branches()
            .filter(|(_, b)| b.component_type == "VoltageSource")
            .flat_map(|(_, b)| b.nodes.iter().copied())
            .collect();
        circuit.add_node("GND".to_string(), None);
        for (net_id, net) in &netlist.nets {
            let bhdl_netlist::types::NetClass::Power { voltage: v, .. } = net.net_class else {
                continue;
            };
            let Some(name) = net.name.clone() else { continue };
            if net.connections.is_empty() {
                continue; // unused rail — nothing to energise
            }
            if vsource_nodes.contains(&name) {
                continue; // already driven by a regulator output
            }
            let idx = circuit.add_node(name.clone(), None); // idempotent
            if driven.contains(&idx) {
                continue; // already driven by a power symbol
            }
            circuit.add_branch(
                format!("V_{name}"),
                &name,
                "GND",
                "VoltageSource".to_string(),
                v,
                None,
            );
            driven.insert(idx);
            if board_ports.get(name.as_str()) == Some(&net_id) {
                info!(
                    "Energised board port {} — boundary VoltageSource {}V → GND at the port",
                    name, v
                );
            } else {
                info!("Added declared power rail {} as VoltageSource {}V → GND", name, v);
            }
        }

        // Step 3c: DC-reference ties for isolated ground domains. A board
        // with a second `ground` port (an optocoupler's field-side return)
        // has a Ground-class net whose NAME isn't the solver's reference
        // ("GND"/"gnd"/"0") — without a tie it floats and gshunt parks it
        // mid-rail (observed: GND_ISO at 6 V, halving every stress reading
        // in its domain). Every solve needs exactly one reference per
        // domain; tying the returns is the standard SPICE treatment of an
        // isolation barrier (1 mΩ — microvolts at real currents). GALVANIC
        // isolation is a netlist/ERC/layout property and is untouched — the
        // tie exists only inside the DC solve.
        for (_net_id, net) in &netlist.nets {
            if !matches!(net.net_class, bhdl_netlist::types::NetClass::Ground) {
                continue;
            }
            let Some(name) = net.name.clone() else { continue };
            if matches!(name.to_lowercase().as_str(), "gnd" | "ground" | "0") {
                continue; // the reference itself
            }
            if net.connections.len() < 2 {
                continue; // floating — never became a node
            }
            circuit.add_node("GND".to_string(), None);
            circuit.add_branch(
                format!("GNDTIE_{name}"),
                &name,
                "GND",
                "Resistor".to_string(),
                1e-3,
                None,
            );
            info!(
                "Tied isolated ground domain {} to the DC reference (1 mΩ solve-only tie)",
                name
            );
        }

        // ── Vendor IBIS buffers (§5 form #1). For every instance whose
        // entity carries an `ibis` model reference: parse the .ibs (cached
        // per path), resolve each wired entity pin to its buffer model via
        // the [Pin] table (explicit `map` entries first, then signal-name
        // match), compose the DC I-V for the pin's declared state
        // (`ibis_state_<PIN>` instance attribute; default Hi-Z — clamps
        // only), and stamp a TableIV branch pin→GND. Real-Data ladder: a
        // missing/unparseable file or unmatched component degrades with a
        // WARN to no stamp — never a fabricated model.
        if !self.ibis_models.is_empty() {
            let mut file_cache: HashMap<std::path::PathBuf, Option<crate::ibis::IbisFile>> =
                HashMap::new();
            let gnd_name = "GND".to_string();
            circuit.add_node(gnd_name.clone(), None);
            for (instance_id, instance) in &netlist.instances {
                let Some(entity) = netlist.modules.get(instance.definition).map(|m| m.name.clone())
                else { continue };
                let Some(ibis_refs) = self.ibis_models.get(&entity) else { continue };
                // Per pin, the FIRST declared ref that resolves it wins —
                // the 16U2 declares its GPIO file first and the USB-pad
                // file second, each covering disjoint pins.
                let mut stamped_pins: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                for ibis_ref in ibis_refs {
                // Resolve the file through the vendor search path:
                // as-written → board dir → local vendor store
                // ($BHDL_VENDOR_DIR / ~/.bhdl/vendor — see
                // bhdl_common::vendor). Missing files keep the as-written
                // path so the §5 warn names what was looked for.
                let path = bhdl_common::vendor::resolve(
                    &ibis_ref.path,
                    self.ibis_base_dir.as_deref(),
                )
                .unwrap_or_else(|| std::path::PathBuf::from(&ibis_ref.path));
                let parsed = file_cache.entry(path.clone()).or_insert_with(|| {
                    match crate::ibis::parse_file(&path) {
                        Ok(f) => Some(f),
                        Err(e) => {
                            warn!(
                                "ibis model for '{}': cannot read {} ({e}) — \
                                 falling through to the next model form (§5 ladder). \
                                 Vendor files install via `bhdl vendor install <download>` \
                                 (see vendor/MANIFEST.toml; `bhdl vendor status` lists them)",
                                entity, path.display()
                            );
                            None
                        }
                    }
                });
                let Some(file) = parsed.as_ref() else { continue };
                let Some(component) = (if ibis_ref.component.is_empty() {
                    file.components.first()
                } else {
                    file.component(&ibis_ref.component)
                }) else {
                    warn!(
                        "ibis model for '{}': component '{}' not in {} — skipping",
                        entity, ibis_ref.component, path.display()
                    );
                    continue;
                };
                let corner = self.ibis_corner_override.unwrap_or_else(|| {
                    crate::ibis::Corner::parse(&ibis_ref.corner).unwrap_or_default()
                });

                // Per-rail attribution: the net wired to the component's
                // primary POWER row (see Component::power_pin). Buffers
                // then stamp as TWO branches — pulldown+GND-clamp to GND,
                // pullup+POWER-clamp to this rail — so sourced current is
                // booked against the rail that physically supplies it (and
                // the Vcc-relative tables track actual rail voltage). None
                // ⇒ single pin→GND composite as before.
                let rail_net: Option<(String, Option<f64>)> = component.power_pin().and_then(|pp| {
                    netlist.pin_instances.values().find_map(|pi2| {
                        if pi2.instance != instance_id {
                            return None;
                        }
                        let pin2 = netlist.pins.get(pi2.pin_def)?;
                        let target2 = ibis_ref
                            .pin_map
                            .iter()
                            .find(|(p, _)| p.eq_ignore_ascii_case(&pin2.name))
                            .map(|(_, sig)| sig.as_str())
                            .unwrap_or(pin2.name.as_str());
                        if target2.eq_ignore_ascii_case(&pp.signal_name)
                            || target2.eq_ignore_ascii_case(&pp.pin)
                        {
                            let net = netlist.nets.get(pi2.net?)?;
                            let v = match net.net_class {
                                bhdl_netlist::types::NetClass::Power { voltage, .. } => {
                                    Some(voltage)
                                }
                                _ => None,
                            };
                            net.name.clone().map(|n| (n, v))
                        } else {
                            None
                        }
                    })
                });

                for pi in netlist.pin_instances.values() {
                    if pi.instance != instance_id { continue; }
                    let Some(net_id) = pi.net else { continue };
                    let Some(pin) = netlist.pins.get(pi.pin_def) else { continue };
                    if stamped_pins.contains(&pin.name) { continue; }
                    let Some(net_name) = netlist.nets.get(net_id).and_then(|n| n.name.clone())
                    else { continue };
                    // Explicit map overrides, then the .ibs [Pin] table.
                    let target = ibis_ref
                        .pin_map
                        .iter()
                        .find(|(p, _)| p.eq_ignore_ascii_case(&pin.name))
                        .map(|(_, sig)| sig.as_str())
                        .unwrap_or(pin.name.as_str());
                    let Some(pin_row) = component.pin_for(target) else { continue };
                    let Some(model) = file.resolve_model(&pin_row.model_name) else { continue };
                    // Scheduled transitions: `ibis_wave_<PIN>="rise@2n fall@10n"`
                    // (ibis_* passthrough namespace, like ibis_state_*). A
                    // malformed spec or an edge the file has no data for is
                    // a hard error — a directive that can't be honored must
                    // not degrade into a silent static buffer.
                    let wave_attr = format!("ibis_wave_{}", pin.name);
                    let wave_events = match instance.attributes.get(&wave_attr) {
                        Some(spec) => Some(
                            crate::ibis_transient::parse_wave_spec(spec).map_err(|e| {
                                crate::errors::SpiceError::InvalidModel(format!(
                                    "{}.{}: {wave_attr}: {e}",
                                    instance.name, pin.name
                                ))
                            })?,
                        ),
                        None => None,
                    };
                    let state_attr = format!("ibis_state_{}", pin.name);
                    let explicit_state = instance
                        .attributes
                        .get(&state_attr)
                        .and_then(|s| crate::ibis::BufferState::parse(s));
                    // Initial state: explicit ibis_state wins; else a waved
                    // pin starts in the state its first edge leaves (rise ⇒
                    // Low, fall ⇒ High); else honest Hi-Z.
                    let state = explicit_state.unwrap_or_else(|| match &wave_events {
                        Some(evs) => {
                            if evs.iter().min_by(|a, b| a.t.partial_cmp(&b.t).unwrap())
                                .map(|e| e.rising).unwrap_or(true)
                            {
                                crate::ibis::BufferState::Low
                            } else {
                                crate::ibis::BufferState::High
                            }
                        }
                        None => Default::default(),
                    });
                    let (ku, kd) = match state {
                        crate::ibis::BufferState::High => (1.0, 0.0),
                        crate::ibis::BufferState::Low => (0.0, 1.0),
                        crate::ibis::BufferState::HiZ => (0.0, 0.0),
                    };
                    // Split by return rail ONLY when the board rail sits
                    // inside the file's own characterized [Voltage Range]:
                    // the Vcc-relative tables ride the rail by definition,
                    // but riding a rail volts outside the measurement
                    // domain would be silent extrapolation of vendor data
                    // (the 16U2's gpio buffer is a 1.8V-domain model — on
                    // a 5V board it must NOT be rigid-shifted to 5V).
                    // Unknown rail voltage ⇒ can't validate ⇒ composite.
                    let rail_matches = match (&rail_net, model.vcc(corner)) {
                        (Some((_, Some(v_rail))), Some(vcc)) => {
                            let lo = model.voltage_range[1].unwrap_or(vcc * 0.9);
                            let hi = model.voltage_range[2].unwrap_or(vcc * 1.1);
                            let (lo, hi) = (lo.min(hi), lo.max(hi));
                            *v_rail >= lo - 1e-6 && *v_rail <= hi + 1e-6
                        }
                        _ => false,
                    };
                    if let (Some((rn, Some(v_rail))), Some(vcc), false) =
                        (&rail_net, model.vcc(corner), rail_matches)
                    {
                        info!(
                            "ibis: {}.{} model {} characterized at Vcc {vcc}V but rail '{rn}' is {v_rail}V — outside the file's range, keeping the GND-referenced composite (no rail attribution)",
                            instance.name, pin.name, model.name
                        );
                    }
                    let (gnd_points, vcc_points) = if rail_matches {
                        model.composed_iv_split(ku, kd, corner)
                    } else {
                        (model.composed_iv(state, corner), None)
                    };
                    if gnd_points.is_none() && vcc_points.is_none() {
                        continue;
                    }
                    let provenance =
                        format!("ibis:{}#{}:{:?}", path.display(), model.name, corner);
                    let branch_name = format!("{}_{}_ibis", instance.name, pin.name);
                    let vcc_branch_name = format!("{}_{}_ibis_vcc", instance.name, pin.name);
                    if let Some(points) = &gnd_points {
                        let mut meta = HashMap::new();
                        meta.insert(crate::circuit::META_IV_TABLE.to_string(), crate::circuit::encode_iv_table(points));
                        meta.insert(META_PARENT_INSTANCE.to_string(), instance.name.clone());
                        meta.insert("sim_model_provenance".to_string(), provenance.clone());
                        circuit.add_branch_with_metadata(
                            branch_name.clone(),
                            &net_name,
                            &gnd_name,
                            "IbisBuffer".to_string(),
                            0.0,
                            Some(instance_id),
                            meta,
                        );
                    }
                    if let (Some(points), Some((rail, _))) = (&vcc_points, &rail_net) {
                        let mut meta = HashMap::new();
                        meta.insert(crate::circuit::META_IV_TABLE.to_string(), crate::circuit::encode_iv_table(points));
                        meta.insert(META_PARENT_INSTANCE.to_string(), instance.name.clone());
                        meta.insert("sim_model_provenance".to_string(), provenance.clone());
                        circuit.add_branch_with_metadata(
                            vcc_branch_name.clone(),
                            &net_name,
                            rail,
                            "IbisBuffer".to_string(),
                            0.0,
                            Some(instance_id),
                            meta,
                        );
                    }
                    if let Some(events) = wave_events {
                        let mut drive = crate::ibis_transient::IbisDrive::new(
                            branch_name, model.clone(), corner, state, events,
                        )?;
                        if rail_matches && vcc_points.is_some() {
                            drive.vcc_branch = Some(vcc_branch_name);
                        }
                        // Package lead inductance for the ground-bounce
                        // estimate: per-pin override else [Package] lump;
                        // None when the file carries neither.
                        drive.pkg_l = component.pin_inductance(pin_row, corner);
                        self.ibis_drives.push(drive);
                    }
                    // Die capacitance: C_comp as a real Capacitor branch
                    // at the pin. Invisible to the DC solve (caps are
                    // open), integrated by the transient routes.
                    if let Some(cc) = model.c_comp_at(corner) {
                        circuit.add_branch(
                            format!("{}_{}_ccomp", instance.name, pin.name),
                            &net_name,
                            &gnd_name,
                            "Capacitor".to_string(),
                            cc,
                            Some(instance_id),
                        );
                    }
                    stamped_pins.insert(pin.name.clone());
                    info!(
                        "ibis: stamped {}.{} on '{}' as {} buffer (state {:?}, model {}{})",
                        instance.name, pin.name, net_name, model.model_type, state, model.name,
                        match (&rail_net, rail_matches) {
                            (Some((r, _)), true) => format!(", rail {r}"),
                            _ => String::new(),
                        }
                    );
                }
                } // per-ref loop
            }
        }

        info!("Created SPICE circuit with {} nodes and {} components",
             circuit.nodes().count(), circuit.branches().count());

        Ok(circuit)
    }
    
    /// Add nets as circuit nodes, skipping floating nets (those with < 2 connections)
    fn add_nets_as_nodes(&self, circuit: &mut Circuit, netlist: &Netlist) -> Result<()> {
        for (net_id, net) in &netlist.nets {
            // Skip nets with fewer than 2 connections — they're floating and would
            // create underconstrained equations in MNA. They'll be added lazily if
            // a branch references them later.
            if net.connections.len() < 2 {
                let name = net.name.as_deref().unwrap_or("unnamed");
                debug!("Skipping floating net '{}' ({} connections)", name, net.connections.len());
                continue;
            }
            let name = net.name.clone()
                .unwrap_or_else(|| format!("net_{:?}", net_id));
            circuit.add_node(name, Some(net_id));
        }
        Ok(())
    }
    
    /// Parse voltage from a power symbol name like "+12V", "+3V3", "+5V"
    fn parse_power_symbol_voltage(name: &str) -> Option<f64> {
        let trimmed = name.trim_start_matches('+');

        // Handle "3V3" style: replace 'V' with '.' → "3.3"
        // This covers: "3V3" → 3.3, "1V8" → 1.8, "2V5" → 2.5
        if trimmed.contains('V') {
            let normalized = trimmed.replace('V', ".");
            // Strip trailing '.' from cases like "12V" → "12."
            let normalized = normalized.trim_end_matches('.');
            if let Ok(v) = normalized.parse::<f64>() {
                return Some(v);
            }
        }

        // Try parsing directly (e.g., just a number)
        if let Ok(v) = trimmed.parse::<f64>() {
            return Some(v);
        }

        None
    }

    /// Check if a module name represents a power symbol
    fn is_power_symbol(module_name: &str) -> bool {
        module_name.starts_with('+') && module_name.len() > 1
    }

    /// Check if a module name represents a ground symbol
    fn is_ground_symbol(module_name: &str) -> bool {
        let lower = module_name.to_lowercase();
        lower == "gnd" || lower == "ground" || lower == "vss" || lower == "0"
    }

    /// Process a single instance and create SPICE model
    fn process_instance(
        &mut self,
        circuit: &mut Circuit,
        netlist: &Netlist,
        instance_id: InstanceId,
        instance: &bhdl_netlist::Instance,
    ) -> Result<()> {
        let module = netlist.modules.get(instance.definition)
            .ok_or_else(|| anyhow::anyhow!("Module not found for instance {}", instance.name))?;

        // Skip parts marked do-not-populate by the active SKU variant.
        // The instance stays in the netlist (so PnR can keep the
        // footprint on the silkscreen and downstream consumers see a
        // structurally identical netlist across SKUs), but
        // electrically the part isn't on the shipped board — SPICE
        // must simulate the variant-as-built. A missing R/C is an
        // open circuit at simulation time.
        if instance.attributes.get("do_not_populate").map(String::as_str) == Some("true") {
            info!("Skipping {} — flagged do-not-populate by active SKU variant",
                  instance.name);
            return Ok(());
        }

        info!("Processing instance {} with module kind {:?}, module name: {}",
              instance.name, module.kind, module.name);

        // Get connected nets for this instance
        let connected_nets = self.get_connected_nets(netlist, instance_id)?;

        info!("Instance {} has {} connected nets: {:?}",
              instance.name, connected_nets.len(),
              connected_nets.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>());

        // Special handling: power symbols (+12V, +5V, +3V3, etc.)
        // These are 1-pin components that represent voltage sources to GND
        if Self::is_power_symbol(&module.name) {
            if let Some(voltage) = Self::parse_power_symbol_voltage(&module.name) {
                if let Some((_, net_name)) = connected_nets.first() {
                    // Ensure GND node exists
                    circuit.add_node("GND".to_string(), None);
                    // Add voltage source from net to GND
                    circuit.add_branch(
                        instance.name.clone(),
                        net_name,
                        "GND",
                        "VoltageSource".to_string(),
                        voltage,
                        Some(instance_id),
                    );
                    info!("Added power symbol {} as VoltageSource {}V: {} -> GND",
                          instance.name, voltage, net_name);
                }
            } else {
                warn!("Could not parse voltage from power symbol: {}", module.name);
            }
            return Ok(());
        }

        // Special handling: ground symbols
        // GND instances just mark their connected net as ground — the node is already added
        if Self::is_ground_symbol(&module.name) {
            debug!("Skipping ground symbol instance: {} (ground node already exists)", instance.name);
            return Ok(());
        }

        // Special handling: sockets are electrically transparent
        // (zero-impedance pass-through wires). They occupy a BOM row
        // and a PCB footprint, but they contribute no SPICE device —
        // the SOCKETED component (the tube, the IC) is what
        // contributes the behaviour. Their pin instances are already
        // on the shared nets that connect to the held component, so
        // ignoring the socket here leaves the held component's nets
        // intact.
        let class = instance.attributes.get("component_class")
            .or_else(|| netlist.modules.get(instance.definition)
                .and_then(|m| m.attributes.get("component_class")))
            .map(String::as_str)
            .unwrap_or("");
        if matches!(class,
                    "socket" | "tube_socket" | "dip_socket" | "sip_socket" |
                    "relay_socket" | "ic_socket")
        {
            info!("Skipping {} — SPICE-transparent socket (class={class}); \
                   held part contributes the electrical model",
                  instance.name);
            return Ok(());
        }

        // Op-amps become a three-terminal ideal-VCVS branch the linear
        // transient stamps as a replaced OUT row (v_out = A·(v+ − v−),
        // clamped to the supply rails). Until this branch existed the
        // converter dropped amps entirely — every "solved" signal chain
        // was an amplifier-free passive network (task #41 finding).
        if class == "opamp" {
            let pin_info = Self::get_pin_net_info(netlist, instance_id);
            let net_of = |pin: &str| {
                pin_info
                    .iter()
                    .find(|p| p.pin_name.eq_ignore_ascii_case(pin))
                    .map(|p| p.net_name.clone())
            };
            let (Some(inp), Some(inn), Some(out)) =
                (net_of("INP"), net_of("INN"), net_of("OUT"))
            else {
                warn!(
                    "Op-amp {} missing a resolvable INP/INN/OUT net — no SPICE device emitted",
                    instance.name
                );
                return Ok(());
            };
            // Unit-aware magnitude parse: "1MHz", "2MΩ", "75µV", "0.5".
            // Case-exact multipliers ('m' = milli, 'M' = mega) so "mV" and
            // "MHz" both read correctly.
            let parse_qty = |s: &str| -> Option<f64> {
                let s = s.trim();
                let num_end = s
                    .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E'))
                    .unwrap_or(s.len());
                // 'e' may be a unit start ("e" isn't) — retry without exp chars on failure.
                let v: f64 = s[..num_end].parse().ok().or_else(|| {
                    let ne = s
                        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
                        .unwrap_or(s.len());
                    s[..ne].parse().ok()
                })?;
                let suffix = s[num_end..].trim_start_matches(|c: char| c == 'e' || c == 'E');
                Some(v * match suffix.chars().next() {
                    Some('p') => 1e-12,
                    Some('n') => 1e-9,
                    Some('µ') | Some('u') => 1e-6,
                    Some('m') => 1e-3,
                    Some('k') | Some('K') => 1e3,
                    Some('M') => 1e6,
                    Some('G') => 1e9,
                    Some('T') => 1e12,
                    _ => 1.0,
                })
            };
            let attr = |names: &[&str]| -> Option<f64> {
                names.iter().find_map(|n| {
                    instance
                        .attributes
                        .get(*n)
                        .or_else(|| module.attributes.get(*n))
                        .and_then(|s| parse_qty(s))
                })
            };
            // Open-loop gain: datasheet attribute when declared, else the
            // documented ideal default (2e5 ≈ LM741 typ; closed-loop results
            // are insensitive to A at feedback-network gains).
            let gain = attr(&["spice_aol", "open_loop_gain"]).unwrap_or(2e5);
            // The full behavioral parameter set — every value comes from the
            // part's OWN stdlib declaration; absent attributes leave the
            // corresponding physics out of the model (no GBW → memoryless,
            // no slew → unlimited), never a fabricated stand-in.
            let mut meta = std::collections::HashMap::new();
            for (key, names) in [
                (crate::circuit::META_GBW, &["spice_gbw", "gain_bandwidth"][..]),
                (crate::circuit::META_RIN, &["spice_rin", "input_resistance"][..]),
                (crate::circuit::META_ROUT, &["spice_rout", "output_resistance"][..]),
                (crate::circuit::META_VOS, &["spice_vos", "input_offset"][..]),
                (crate::circuit::META_SLEW, &["spice_slew_rate", "slew_rate"][..]),
            ] {
                if let Some(v) = attr(names) {
                    meta.insert(key.to_string(), v.to_string());
                }
            }
            // Saturation = the REAL supply rails this instance is wired to
            // (net-class voltages), not an assumed headroom.
            let rail_v = |pin: &str| {
                pin_info
                    .iter()
                    .find(|p| p.pin_name.eq_ignore_ascii_case(pin))
                    .and_then(|p| netlist.nets.get(p.net_id))
                    .and_then(|n| match n.net_class {
                        bhdl_netlist::types::NetClass::Power { voltage, .. } => Some(voltage),
                        _ => None,
                    })
            };
            if let Some(v) = rail_v("VCC") {
                meta.insert(crate::circuit::META_VSAT_P.to_string(), v.to_string());
            }
            if let Some(v) = rail_v("VEE") {
                meta.insert(crate::circuit::META_VSAT_N.to_string(), v.to_string());
            }
            info!(
                "Added op-amp {}: {}→{} (INN {}), A={:.0e}",
                instance.name, inp, out, inn, gain
            );
            circuit.add_opamp_branch(
                instance.name.clone(),
                &inp,
                &inn,
                &out,
                gain,
                Some(instance_id),
                meta,
            );
            return Ok(());
        }

        // Optocoupler: decomposed into the two elements the package really
        // contains — an IRED (LED-class exponential branch A→K, Shockley Is
        // derived from the part's CITED Vf@IF point) and a phototransistor
        // (`PhotoCoupled` branch C→E whose current is CTR·IF with a smooth
        // tanh saturation at the cited VCE(sat)). The coupling is a
        // controlled source only — no galvanic path crosses the barrier.
        // CTR = the part's ctr_min_pct: the MINIMUM is the only guaranteed
        // transfer claim a datasheet makes (typ isn't even published for
        // ranked parts), so the solved operating point is the conservative
        // one — a load the min-rank device can sink is signed off for the
        // whole rank.
        // Potentiometer: ONE physical part, decomposed SIM-SIDE into two
        // half-resistances around the wiper at 50% rotation (stated
        // modeling choice — the wiper is a user input, not a datum; the
        // mid-travel point is the conventional DC bias). Same one-part/
        // many-branches pattern as the optocoupler below.
        if class == "potentiometer" {
            let pin_info = Self::get_pin_net_info(netlist, instance_id);
            let net_of = |name: &str| {
                pin_info
                    .iter()
                    .find(|p| p.pin_name.eq_ignore_ascii_case(name))
                    .map(|p| p.net_name.clone())
            };
            let (Some(n1), Some(nw), Some(n3)) =
                (net_of("1"), net_of("2"), net_of("3"))
            else {
                warn!(
                    "Potentiometer {} missing a resolvable 1/2/3 net — no SPICE branches emitted",
                    instance.name
                );
                return Ok(());
            };
            // SI-suffixed value ("10kΩ", "4.7M", "470"): numeric prefix
            // scaled by the first suffix char (same shape as the opto
            // block's parse_qty, plus the resistance-side k/M/G).
            let parse_r = |s: &str| -> Option<f64> {
                let s = s.trim();
                let num_end = s
                    .find(|ch: char| {
                        !(ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+')
                    })
                    .unwrap_or(s.len());
                let v: f64 = s[..num_end].parse().ok()?;
                Some(v * match s[num_end..].chars().next() {
                    Some('m') => 1e-3,
                    Some('k') | Some('K') => 1e3,
                    Some('M') => 1e6,
                    Some('G') => 1e9,
                    _ => 1.0,
                })
            };
            let total: f64 = instance
                .attributes
                .get("resistance")
                .and_then(|s| parse_r(s))
                .unwrap_or(0.0);
            if total <= 0.0 {
                warn!(
                    "Potentiometer {} has no real resistance — Real-Data Policy: declare it (no branches emitted)",
                    instance.name
                );
                return Ok(());
            }
            let half = total / 2.0;
            for (tag, a, b) in [("_ccw", &n1, &nw), ("_cw", &nw, &n3)] {
                let mut meta = HashMap::new();
                meta.insert(META_PARENT_INSTANCE.to_string(), instance.name.clone());
                circuit.add_branch_with_metadata(
                    format!("{}{}", instance.name, tag),
                    a,
                    b,
                    "Resistor".to_string(),
                    half,
                    Some(instance_id),
                    meta,
                );
            }
            info!(
                "Added potentiometer {}: {}Ω split {}—{}—{} at 50% wiper",
                instance.name, total, n1, nw, n3
            );
            return Ok(());
        }

        if class == "optocoupler" {
            let pin_info = Self::get_pin_net_info(netlist, instance_id);
            let net_of = |names: &[&str]| {
                pin_info
                    .iter()
                    .find(|p| names.iter().any(|n| p.pin_name.eq_ignore_ascii_case(n)))
                    .map(|p| p.net_name.clone())
            };
            let (Some(a), Some(k), Some(c), Some(e)) = (
                net_of(&["A", "ANODE"]),
                net_of(&["K", "CATHODE"]),
                net_of(&["C", "COLLECTOR"]),
                net_of(&["E", "EMITTER"]),
            ) else {
                warn!(
                    "Optocoupler {} missing a resolvable A/K/C/E net — no SPICE device emitted",
                    instance.name
                );
                return Ok(());
            };
            // Numeric attribute with SI suffix ("1.2V", "50mA", "50.0").
            let parse_qty = |s: &str| -> Option<f64> {
                let s = s.trim();
                let num_end = s
                    .find(|ch: char| !(ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+'))
                    .unwrap_or(s.len());
                let v: f64 = s[..num_end].parse().ok()?;
                Some(v * match s[num_end..].chars().next() {
                    Some('p') => 1e-12,
                    Some('n') => 1e-9,
                    Some('µ') | Some('u') => 1e-6,
                    Some('m') => 1e-3,
                    Some('k') | Some('K') => 1e3,
                    Some('M') => 1e6,
                    _ => 1.0,
                })
            };
            let attr = |name: &str| -> Option<f64> {
                instance
                    .attributes
                    .get(name)
                    .or_else(|| module.attributes.get(name))
                    .and_then(|s| parse_qty(s))
            };
            // Both halves need their defining datum — absence beats
            // invention: no cited Vf or CTR floor, no model.
            let (Some(vf), Some(ctr_min_pct)) = (attr("forward_voltage"), attr("ctr_min_pct"))
            else {
                warn!(
                    "Optocoupler {} lacks forward_voltage/ctr_min_pct attributes — no SPICE device emitted (absence beats invention)",
                    instance.name
                );
                return Ok(());
            };
            // IRED: Is from the cited (Vf, IF) point at the LED-class
            // emission coefficient n=2 — Is = IF / exp(Vf/(n·Vt)).
            let if_ref = attr("forward_current").unwrap_or(0.020); // Vf citation point
            let (n_ideality, vt) = (2.0, 0.026);
            let is = if_ref / (vf / (n_ideality * vt)).exp();
            let vce_sat = attr("vce_sat").unwrap_or(0.1);

            let ired_name = format!("{}_ired", instance.name);
            let mut led_meta = HashMap::new();
            led_meta.insert(META_SATURATION_CURRENT.to_string(), format!("{is:e}"));
            led_meta.insert(META_EMISSION_COEFFICIENT.to_string(), n_ideality.to_string());
            led_meta.insert(META_PARENT_INSTANCE.to_string(), instance.name.clone());
            led_meta.insert(META_DECOMPOSITION_ROLE.to_string(), "opto_ired".to_string());
            circuit.add_branch_with_metadata(
                ired_name.clone(),
                &a,
                &k,
                "LED".to_string(),
                vf,
                Some(instance_id),
                led_meta,
            );

            let ctr = ctr_min_pct / 100.0;
            let mut ce_meta = HashMap::new();
            ce_meta.insert(crate::circuit::META_CTR.to_string(), ctr.to_string());
            // CTR-vs-IF curve (Fig.6 points, normalized to the 5 mA
            // rank point) — carried when the entity declares them, so
            // the solve derates/boosts CTR at the ACTUAL operating IF
            // instead of assuming the rank point.
            {
                let curve_attrs: [(&str, f64); 6] = [
                    ("ctr_norm_100ua", 100e-6),
                    ("ctr_norm_500ua", 500e-6),
                    ("ctr_norm_1ma", 1e-3),
                    ("ctr_norm_2ma", 2e-3),
                    ("ctr_norm_5ma", 5e-3),
                    ("ctr_norm_10ma", 10e-3),
                ];
                let pts: Vec<String> = curve_attrs
                    .iter()
                    .filter_map(|&(name, if_a)| {
                        attr(name).map(|f| format!("{if_a}:{f}"))
                    })
                    .collect();
                if pts.len() >= 2 {
                    ce_meta.insert(
                        crate::circuit::META_CTR_CURVE.to_string(),
                        pts.join(";"),
                    );
                }
            }
            // Knee at VCE(sat)/2: tanh(2) = 0.96 of full CTR current at the
            // cited saturation voltage.
            ce_meta.insert(crate::circuit::META_CTR_VKNEE.to_string(), (vce_sat / 2.0).to_string());
            ce_meta.insert(crate::circuit::META_CTRL_BRANCH.to_string(), ired_name.clone());
            ce_meta.insert(META_PARENT_INSTANCE.to_string(), instance.name.clone());
            ce_meta.insert(META_DECOMPOSITION_ROLE.to_string(), "opto_ce".to_string());
            circuit.add_branch_with_metadata(
                format!("{}_ce", instance.name),
                &c,
                &e,
                "PhotoCoupled".to_string(),
                ctr,
                Some(instance_id),
                ce_meta,
            );
            info!(
                "Added optocoupler {}: IRED {}→{} (Is={:.3e}, n={}), CTR {:.0}% min, C-E {}→{}",
                instance.name, a, k, is, n_ideality, ctr_min_pct, c, e
            );
            return Ok(());
        }

        // §5 model surface, LOAD side: an entity whose model block authors
        // `draws` on a pin is a real DC load — emit a CurrentSource from
        // that pin's net to ground so GLACIER solves genuine rail currents
        // (the buck inductor carries the MCU's declared draw, not a
        // placeholder). Opt-in strictly by model declaration — datasheet
        // attributes like i_supply never imply "model me as a load".
        // Regulators keep their dedicated branch (draws is an efficiency
        // correction there, not a load).
        if !matches!(class, "voltage_regulator" | "ldo" | "switching_regulator") {
            let entity_name = netlist.instances.get(instance_id)
                .and_then(|i| netlist.modules.get(i.definition))
                .map(|m| m.name.clone());
            if let Some(model) = entity_name.as_ref().and_then(|e| self.model_overrides.get(e)) {
                if !model.draws.is_empty() {
                    let gnd_name = netlist.nets.iter()
                        .find(|(_, n)| matches!(n.net_class, bhdl_netlist::types::NetClass::Ground))
                        .and_then(|(_, n)| n.name.clone())
                        .unwrap_or_else(|| "GND".to_string());
                    let pin_info = Self::get_pin_net_info(netlist, instance_id);
                    let mut emitted = false;
                    for p in &pin_info {
                        let Some(i_draw) = model.draws.get(&p.pin_name) else { continue };
                        let mut meta = HashMap::new();
                        meta.insert(META_PARENT_INSTANCE.to_string(), instance.name.clone());
                        circuit.add_branch_with_metadata(
                            format!("{}_draw", instance.name),
                            &p.net_name,
                            &gnd_name,
                            "CurrentSource".to_string(),
                            *i_draw,
                            Some(instance_id),
                            meta,
                        );
                        info!("Load {}: model-declared draw {}A on {} ({})",
                              instance.name, i_draw, p.pin_name, p.net_name);
                        emitted = true;
                    }
                    if emitted {
                        return Ok(());
                    }
                }
            }
        }

        // Extract model based on available information
        let extracted_model = self.extract_model_for_instance(
            &instance.name,
            module,
            &instance.attributes,
        )?;

        // Create SPICE model
        let spice_model = self.model_extractor.create_spice_model(&extracted_model)?;

        // Handle different component types
        match module.kind {
            ModuleKind::PhysicalComponent | ModuleKind::Component => {
                self.add_physical_component(
                    circuit,
                    netlist,
                    &instance.name,
                    instance_id,
                    &connected_nets,
                    extracted_model,
                )?;
            }
            ModuleKind::Interface => {
                // Interfaces might represent connectors or test points
                if module.name.to_lowercase().contains("test") ||
                   module.name.to_lowercase().contains("point") ||
                   module.name.to_lowercase().contains("connector") {
                    self.add_physical_component(
                        circuit,
                        netlist,
                        &instance.name,
                        instance_id,
                        &connected_nets,
                        extracted_model,
                    )?;
                } else {
                    debug!("Skipping interface module: {}", instance.name);
                }
            }
            ModuleKind::Module => {
                // A regulator ENTITY imports as a logical Module, but it is an
                // electrically meaningful device: route it through the physical
                // path so the regulator decomposition (VOUT source + dropout
                // connectivity) stamps into the solve. Without this, an
                // imported stdlib LDO/buck was silently skipped and its output
                // rail was never driven. Other logical modules stay skipped.
                //
                // TRIODES too: a STANDALONE `v: TriodeECC83();` instance
                // arrives as a logical Module and was silently dropped from
                // the solve — every earlier tube fixture happened to
                // instantiate INLINE (`V1: Triode().P`) or via an expansion
                // recipe, which land as Component. Flushed by the ecc83-pp
                // demo transcription (SRPP solved as a pure resistor
                // network, converged in 1 iteration with no tube current).
                if matches!(
                    extracted_model.component_type,
                    crate::components::ComponentType::VoltageRegulator
                        | crate::components::ComponentType::Triode
                ) {
                    self.add_physical_component(
                        circuit,
                        netlist,
                        &instance.name,
                        instance_id,
                        &connected_nets,
                        extracted_model,
                    )?;
                } else {
                    debug!("Skipping logical module: {}", instance.name);
                }
            }
            ModuleKind::DesignBlock => {
                // A DESIGN BLOCK (`entity X as design`) is composition — its
                // children are the physical parts. But in the two-layer
                // library model (docs/spec/Requirements_And_Resolution.md)
                // the BLOCK is what the board sees as "the regulator": the
                // bare silicon underneath has no switching model of its
                // own, so the behavioral regulator model (VOUT source +
                // dropout connectivity) lives on the block, exactly as it
                // lived on the old conflated entity. Route regulator-class
                // blocks through the physical path; pure compositions
                // (no electrical model of their own) stay skipped.
                if matches!(
                    extracted_model.component_type,
                    crate::components::ComponentType::VoltageRegulator
                ) {
                    self.add_physical_component(
                        circuit,
                        netlist,
                        &instance.name,
                        instance_id,
                        &connected_nets,
                        extracted_model,
                    )?;
                } else {
                    debug!("Skipping design block (composition only): {}", instance.name);
                }
            }
            _ => {
                debug!("Skipping module kind {:?} for {}", module.kind, instance.name);
            }
        }

        // Cache the model
        self.instance_models.insert(instance_id, spice_model);

        Ok(())
    }
    
    /// Extract model for an instance
    fn extract_model_for_instance(
        &mut self,
        instance_name: &str,
        module: &ModuleDefinition,
        attributes: &HashMap<String, String>,
    ) -> Result<ExtractedModel> {
        // First try symbol table if available
        if let Some(symbol_data) = self.symbol_table.get(instance_name) {
            if let Ok(model) = self.model_extractor.extract_from_symbol_table(instance_name, symbol_data) {
                return Ok(model);
            }
        }
        
        // Then try user attributes
        if !attributes.is_empty() {
            if let Ok(model) = self.model_extractor.extract_from_user_attributes(instance_name, attributes) {
                return Ok(model);
            }
        }
        
        // Build extraction data from module and instance attributes
        let mut extraction_data = HashMap::new();
        
        // Add instance name
        extraction_data.insert("name".to_string(), instance_name.to_string());
        
        // Add module name for registry lookup
        extraction_data.insert("type".to_string(), module.name.clone());
        
        // Add instance attributes (these override module attributes)
        for (key, value) in attributes {
            extraction_data.insert(key.clone(), value.clone());
        }
        
        // Add module attributes
        for (key, value) in &module.attributes {
            extraction_data.insert(key.clone(), value.clone());
        }
        
        // Try to extract from the combined data
        self.model_extractor.extract_from_data(extraction_data)
    }
    
    /// Get nets connected to an instance
    fn get_connected_nets(
        &self,
        netlist: &Netlist,
        instance_id: InstanceId,
    ) -> Result<Vec<(NetId, String)>> {
        let mut connected_nets = Vec::new();

        // Look at pin instances for this instance
        for (_pin_inst_id, pin_inst) in &netlist.pin_instances {
            if pin_inst.instance == instance_id {
                if let Some(net_id) = pin_inst.net {
                    if let Some(net) = netlist.nets.get(net_id) {
                        let net_name = net.name.clone()
                            .unwrap_or_else(|| format!("net_{:?}", net_id));

                        // Add the net if not already present
                        if !connected_nets.iter().any(|(id, _)| *id == net_id) {
                            connected_nets.push((net_id, net_name));
                        }
                    }
                } else {
                    // Log unconnected pins for debugging
                    let pin_name = netlist.pins.get(pin_inst.pin_def)
                        .map(|p| p.name.as_str())
                        .unwrap_or("?");
                    debug!("Pin {} of instance {:?} has no net assigned", pin_name, instance_id);
                }
            }
        }

        // Also check net connections pointing to this instance's pins
        for (_net_id, net) in &netlist.nets {
            for conn in &net.connections {
                match conn {
                    ConnectionPoint::PinInstance(pi_id) => {
                        if let Some(pi) = netlist.pin_instances.get(*pi_id) {
                            if pi.instance == instance_id {
                                let net_name = net.name.clone()
                                    .unwrap_or_else(|| format!("net_{:?}", _net_id));
                                if !connected_nets.iter().any(|(id, _)| *id == _net_id) {
                                    connected_nets.push((_net_id, net_name));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        debug!("Instance {:?} has {} connected nets", instance_id, connected_nets.len());

        Ok(connected_nets)
    }
    
    /// Map ComponentType to SPICE equation system string
    fn component_type_to_spice_string(ct: &crate::components::ComponentType) -> String {
        use crate::components::ComponentType;
        match ct {
            ComponentType::Resistor => "Resistor".to_string(),
            ComponentType::Capacitor => "Capacitor".to_string(),
            ComponentType::Inductor => "Inductor".to_string(),
            ComponentType::Diode => "Diode".to_string(),
            ComponentType::LED => "LED".to_string(),
            ComponentType::VoltageSource => "VoltageSource".to_string(),
            ComponentType::CurrentSource => "CurrentSource".to_string(),
            ComponentType::VoltageRegulator => "VoltageRegulator".to_string(),
            ComponentType::BJT => "BJT".to_string(),
            ComponentType::MOSFET => "MOSFET".to_string(),
            ComponentType::OpAmp => "OpAmp".to_string(),
            ComponentType::Triode => "Triode".to_string(),
            ComponentType::Other(s) => s.clone(),
        }
    }

    /// Get connected nets with full pin metadata for an instance.
    /// Uses both pin_inst.net and net.connections for completeness.
    fn get_pin_net_info(
        netlist: &Netlist,
        instance_id: InstanceId,
    ) -> Vec<PinNetInfo> {
        let mut result = Vec::new();
        let mut seen_pins: std::collections::HashSet<bhdl_netlist::PinId> = std::collections::HashSet::new();

        // Pass 1: pin_inst.net (direct assignment)
        for (_pi_id, pin_inst) in &netlist.pin_instances {
            if pin_inst.instance == instance_id {
                if let Some(net_id) = pin_inst.net {
                    if let (Some(net), Some(pin_def)) = (netlist.nets.get(net_id), netlist.pins.get(pin_inst.pin_def)) {
                        seen_pins.insert(pin_inst.pin_def);
                        let net_name = net.name.clone().unwrap_or_else(|| format!("net_{:?}", net_id));
                        result.push(PinNetInfo {
                            net_id,
                            net_name,
                            pin_name: pin_def.name.clone(),
                            pin_type: pin_def.pin_type,
                            pin_direction: pin_def.direction,
                        });
                    }
                }
            }
        }

        // Pass 2: net.connections → PinInstance (fills gaps where pin_inst.net is null)
        for (net_id, net) in &netlist.nets {
            for conn in &net.connections {
                if let ConnectionPoint::PinInstance(pi_id) = conn {
                    if let Some(pi) = netlist.pin_instances.get(*pi_id) {
                        if pi.instance == instance_id && !seen_pins.contains(&pi.pin_def) {
                            if let Some(pin_def) = netlist.pins.get(pi.pin_def) {
                                seen_pins.insert(pi.pin_def);
                                let net_name = net.name.clone().unwrap_or_else(|| format!("net_{:?}", net_id));
                                result.push(PinNetInfo {
                                    net_id,
                                    net_name,
                                    pin_name: pin_def.name.clone(),
                                    pin_type: pin_def.pin_type,
                                    pin_direction: pin_def.direction,
                                });
                            }
                        }
                    }
                }
            }
        }

        result
    }

    /// Add a physical component to the circuit
    fn add_physical_component(
        &self,
        circuit: &mut Circuit,
        netlist: &Netlist,
        instance_name: &str,
        instance_id: InstanceId,
        connected_nets: &[(NetId, String)],
        extracted_model: ExtractedModel,
    ) -> Result<()> {
        use crate::components::ComponentType;

        let spice_type = Self::component_type_to_spice_string(&extracted_model.component_type);

        // Special handling for voltage regulators (3-terminal)
        // Use pin types from stdlib: power-in = VIN, power-out = VOUT, ground = GND
        if extracted_model.component_type == ComponentType::VoltageRegulator {
            // Real-Data Policy: the regulator's output voltage must be a real
            // entity-declared value — no fabricated 5 V fallback.
            let mut vout_voltage = extracted_model.parameters.get("vout").copied()
                .or(extracted_model.parameters.get("output_voltage").copied())
                .or(extracted_model.parameters.get("voltage").copied())
                .ok_or_else(|| anyhow::anyhow!(
                    "regulator '{}' has no real output voltage (vout/output_voltage/voltage) — \
                     Real-Data Policy: declare it on the stdlib entity (the datasheet)",
                    instance_name))?;

            let pin_info = Self::get_pin_net_info(netlist, instance_id);

            // §5 device-model override: if this instance's entity declares a
            // `model { node <pin> source = … }` block, its evaluated voltage
            // supplies the output source in place of the hardcoded `vout`
            // attribute. Keyed by entity name → pin name (the model's `node`
            // net is the entity pin). Everything else (dropout path, loss
            // metadata) is unchanged — the input-current `draws` branch is a
            // later stage.
            let entity_name = netlist.instances.get(instance_id)
                .and_then(|i| netlist.modules.get(i.definition))
                .map(|m| m.name.clone());
            let vout_pin_name = pin_info.iter()
                .find(|p| p.pin_type == bhdl_netlist::PinType::Power
                       && p.pin_direction == bhdl_netlist::PinDirection::Out)
                .map(|p| p.pin_name.clone());
            if let (Some(ent), Some(pin)) = (&entity_name, &vout_pin_name) {
                if let Some(v) = self.model_overrides.get(ent).and_then(|m| m.sources.get(pin)) {
                    info!("Regulator {} VOUT from model block: {}V (was {}V hardcoded)",
                          instance_name, v, vout_voltage);
                    vout_voltage = *v;
                }
            }

            // ── Closed-loop DC (review finding: the FB chicken-and-egg) ──
            // Physically the loop forces FB = VREF and VOUT FOLLOWS the
            // divider: VOUT = VREF·(1 + Rtop/Rbot) with the PLACED (snapped)
            // resistors. Pinning VOUT at the declared nominal inverted the
            // causality — the solver back-derived FB (793mV) from the
            // declaration and ERASED the divider snap error (+0.85% here),
            // which is real verification data. The loop's DC fixed point is
            // algebraic, so no iteration is needed: when the part declares
            // feedback_voltage and its FB pin has a resolvable divider, the
            // source voltage is DERIVED from it. Falls back to the declared
            // nominal when either input is absent (Real-Data).
            let vref = extracted_model.parameters.get("feedback_voltage").copied()
                .or_else(|| {
                    netlist.instances.get(instance_id)
                        .and_then(|i| i.attributes.get("feedback_voltage"))
                        .and_then(|v| parse_res_val(v))
                });
            if let Some(vref) = vref.filter(|v| *v > 0.0) {
                let fb_net = pin_info.iter()
                    .find(|p| p.pin_name.eq_ignore_ascii_case("FB"))
                    .map(|p| p.net_id);
                if let Some(fb_net) = fb_net {
                    if let Some((rt, rb)) = divider_at(netlist, fb_net) {
                        let derived = vref * (1.0 + rt / rb);
                        info!(
                            "Regulator {}: closed-loop DC — VOUT derived from FB divider:                              {:.3}V = {:.3}V·(1 + {:.1}/{:.1}) (declared nominal was {}V)",
                            instance_name, derived, vref, rt, rb, vout_voltage
                        );
                        vout_voltage = derived;
                    }
                }
            }

            // Classify pins by their type and direction from stdlib definitions
            // `power in` lowers to PinDirection::Power (the ERC convention);
            // In/InOut kept for legacy signal-typed declarations.
            let vin_net = pin_info.iter()
                .find(|p| p.pin_type == bhdl_netlist::PinType::Power
                       && (p.pin_direction == bhdl_netlist::PinDirection::Power
                           || p.pin_direction == bhdl_netlist::PinDirection::In
                           || p.pin_direction == bhdl_netlist::PinDirection::InOut))
                .map(|p| p.net_name.clone());
            let vout_net = pin_info.iter()
                .find(|p| p.pin_type == bhdl_netlist::PinType::Power
                       && p.pin_direction == bhdl_netlist::PinDirection::Out)
                .map(|p| p.net_name.clone());
            let gnd_node_name = pin_info.iter()
                .find(|p| p.pin_type == bhdl_netlist::PinType::Ground)
                .map(|p| p.net_name.clone())
                .unwrap_or_else(|| "GND".to_string());

            circuit.add_node(gnd_node_name.clone(), None);

            info!("Regulator {} decomposition (from pin types): VIN={:?}, VOUT={:?}, GND={}, vout_voltage={}V",
                  instance_name, vin_net, vout_net, gnd_node_name, vout_voltage);

            // Determine regulator type and extract loss model parameters
            let component_class = extracted_model.attributes.get("component_class")
                .cloned()
                .unwrap_or_default();
            let is_switching = component_class == "switching_regulator";

            // Helper: REQUIRE a real parameter from extracted_model (f64) or
            // attributes (string). Real-Data Policy: no fabricated default —
            // a regulator that does not declare its loss-model constants is a
            // hard error (the device datasheet must supply rds_on / f_sw / …).
            let req_param = |name: &str| -> anyhow::Result<f64> {
                extracted_model.parameters.get(name).copied()
                    // Unit-aware fallback: stdlib attributes are written
                    // idiomatically with SI units (`3.4mA`, `500kHz`, `90mΩ`);
                    // a bare f64 parse silently failed on all of them, making
                    // the regulator decompose reject correctly-declared parts.
                    .or_else(|| extracted_model.attributes.get(name)
                        .and_then(|s| crate::model_factory::parse_value(s)))
                    .ok_or_else(|| anyhow::anyhow!(
                        "regulator '{}' is missing real SPICE parameter '{}' — Real-Data \
                         Policy: declare it on the stdlib entity (the datasheet)",
                        instance_name, name))
            };

            // Add voltage source on output: VOUT → GND = vout_voltage
            if let Some(vout_name) = &vout_net {
                let mut vout_meta = HashMap::new();
                vout_meta.insert(META_PARENT_INSTANCE.to_string(), instance_name.to_string());
                vout_meta.insert(META_DECOMPOSITION_ROLE.to_string(), "vout".to_string());
                if !component_class.is_empty() {
                    vout_meta.insert(META_COMPONENT_CLASS.to_string(), component_class.clone());
                }
                // Store device loss model parameters for post-simulation power
                // computation from physics, not lumped estimates.
                // I_quiescent applies to both linear and switching regulators.
                vout_meta.insert(META_I_QUIESCENT.to_string(), req_param("i_quiescent")?.to_string());
                // Efficiency (fraction) for the input-draw fixpoint:
                // "92%" / "0.92" / "92" all accepted; absent → no stamp
                // (the fixpoint then uses the linear i_in ≈ i_out).
                if let Some(eff) = extracted_model.attributes.get("efficiency")
                    .and_then(|e| e.trim().trim_end_matches('%').trim().parse::<f64>().ok())
                {
                    let eff = if eff > 1.0 { eff / 100.0 } else { eff };
                    if eff > 0.0 && eff <= 1.0 {
                        vout_meta.insert(crate::circuit::META_EFFICIENCY.to_string(), eff.to_string());
                    }
                }
                if is_switching {
                    vout_meta.insert(META_RDS_ON.to_string(), req_param("rds_on")?.to_string());
                    vout_meta.insert(META_F_SW.to_string(), req_param("f_sw")?.to_string());
                    vout_meta.insert(META_T_SW.to_string(), req_param("t_sw")?.to_string());
                }
                // §5 Stage 3b: if the entity's model block authors a VIN current
                // draw, record it. Post-sim power then uses this vendor
                // efficiency model (P_in − P_out) in place of the generic
                // physics loss model — the datasheet-specific correction
                // supersedes the generic computation only when explicitly given.
                let vin_pin_name = pin_info.iter()
                    .find(|p| p.pin_type == bhdl_netlist::PinType::Power
                           && (p.pin_direction == bhdl_netlist::PinDirection::Power
                               || p.pin_direction == bhdl_netlist::PinDirection::In
                               || p.pin_direction == bhdl_netlist::PinDirection::InOut))
                    .map(|p| p.pin_name.clone());
                if let (Some(ent), Some(pin)) = (&entity_name, &vin_pin_name) {
                    if let Some(i_in) = self.model_overrides.get(ent).and_then(|m| m.draws.get(pin)) {
                        vout_meta.insert(META_MODEL_I_IN.to_string(), i_in.to_string());
                        info!("Regulator {} input current from model block: {}A (efficiency model)",
                              instance_name, i_in);
                    }
                }
                circuit.add_branch_with_metadata(
                    format!("{}_vout", instance_name),
                    vout_name,
                    &gnd_node_name,
                    "VoltageSource".to_string(),
                    vout_voltage,
                    Some(instance_id),
                    vout_meta,
                );
                info!("Added regulator {} output as VoltageSource {}V: {} -> {}",
                      instance_name, vout_voltage, vout_name, gnd_node_name);
            }

            // Model the regulator's internal VIN→VOUT dropout path as a high resistance.
            // This provides necessary circuit connectivity for MNA solver stability
            // but must NOT create a significant parallel current path — the voltage
            // source at VOUT already sets the output; this resistor only ensures the
            // VIN node has a DC path to the rest of the circuit.
            // Use 10kΩ: high enough that parasitic current is negligible (~0.7mA at 7V drop)
            // but low enough for MNA numerical stability.
            if let (Some(vin_name), Some(vout_name)) = (&vin_net, &vout_net) {
                let internal_resistance = 10_000.0; // 10kΩ — connectivity only, not a power path
                let mut dropout_meta = HashMap::new();
                dropout_meta.insert(META_PARENT_INSTANCE.to_string(), instance_name.to_string());
                dropout_meta.insert(META_DECOMPOSITION_ROLE.to_string(), "dropout".to_string());
                circuit.add_branch_with_metadata(
                    format!("{}_dropout", instance_name),
                    vin_name,
                    vout_name,
                    "Resistor".to_string(),
                    internal_resistance,
                    Some(instance_id),
                    dropout_meta,
                );
                info!("Added regulator {} dropout path ({:.1}Ω): {} -> {}",
                      instance_name, internal_resistance, vin_name, vout_name);
            }

            return Ok(());
        }

        // Vacuum triode — a genuine 3-terminal nonlinear device. It is NOT
        // decomposed into branches the way a regulator is (it cannot be
        // expressed as linear primitives); it is emitted as a multi-terminal
        // `Circuit` device, which GLACIER stamps directly from its inline
        // Koren parameters.
        if extracted_model.component_type == ComponentType::Triode {
            let pin_info = Self::get_pin_net_info(netlist, instance_id);

            // Classify the three terminals by pin name (case-insensitive).
            // The Koren `terminals` order is [plate, grid, cathode].
            let find = |names: &[&str]| -> Option<String> {
                pin_info
                    .iter()
                    .find(|p| names.iter().any(|n| p.pin_name.eq_ignore_ascii_case(n)))
                    .map(|p| p.net_name.clone())
            };
            let plate = find(&["P", "PLATE", "ANODE", "A"]);
            let grid = find(&["G", "GRID"]);
            let cathode = find(&["K", "CATHODE", "C"]);

            let (plate, grid, cathode) = match (plate, grid, cathode) {
                (Some(p), Some(g), Some(k)) => (p, g, k),
                _ => {
                    warn!(
                        "Triode {} has unclassifiable pins {:?}; skipping",
                        instance_name,
                        pin_info.iter().map(|p| &p.pin_name).collect::<Vec<_>>()
                    );
                    return Ok(());
                }
            };

            // Koren parameters: prefer extracted values, fall back to the
            // nominal 6SN7 set (also the component registry's default).
            let read_param = |name: &str, default: f64| -> f64 {
                extracted_model.parameters.get(name).copied()
                    .or_else(|| extracted_model.attributes.get(name)
                        .and_then(|s| s.parse::<f64>().ok()))
                    .unwrap_or(default)
            };
            let kind = DeviceKind::Triode {
                mu: read_param("mu", 20.0),
                ex: read_param("ex", 1.4),
                kg1: read_param("kg1", 1180.0),
                kp: read_param("kp", 470.0),
                kvb: read_param("kvb", 300.0),
            };

            for n in [&plate, &grid, &cathode] {
                circuit.add_node(n.clone(), None);
            }
            circuit.add_device(
                instance_name.to_string(),
                kind,
                &[plate.as_str(), grid.as_str(), cathode.as_str()],
                Some(instance_id),
            );
            // ALSO stamp a "KorenTriode" plate→cathode BRANCH: the DC
            // path (SpiceEquationSystem) solves branches only and
            // silently ignores multi-terminal devices — without this
            // the ecc83-pp SRPP solved as a pure resistor network
            // (both triodes present, zero plate current). The device
            // above serves glacier_production (AC/transient/bias);
            // each side ignores the other's representation, so
            // nothing double-stamps.
            let mut tri_meta = HashMap::new();
            tri_meta.insert(META_PARENT_INSTANCE.to_string(), instance_name.to_string());
            let DeviceKind::Triode { mu, ex, kg1, kp, kvb } = kind;
            tri_meta.insert(crate::circuit::META_TRIODE_MU.to_string(), mu.to_string());
            tri_meta.insert(crate::circuit::META_TRIODE_EX.to_string(), ex.to_string());
            tri_meta.insert(crate::circuit::META_TRIODE_KG1.to_string(), kg1.to_string());
            tri_meta.insert(crate::circuit::META_TRIODE_KP.to_string(), kp.to_string());
            tri_meta.insert(crate::circuit::META_TRIODE_KVB.to_string(), kvb.to_string());
            tri_meta.insert(
                crate::circuit::META_TRIODE_GRID_NODE.to_string(),
                grid.clone(),
            );
            circuit.add_branch_with_metadata(
                format!("{}_plate", instance_name),
                &plate,
                &cathode,
                "KorenTriode".to_string(),
                0.0,
                Some(instance_id),
                tri_meta,
            );
            info!(
                "Added triode {}: plate={}, grid={}, cathode={}",
                instance_name, plate, grid, cathode
            );
            return Ok(());
        }

        // For 2-terminal components
        if connected_nets.len() >= 2 {
            // Get primary value from extracted model
            let value = self.get_primary_value(&extracted_model);

            // For diode-type components, use pin metadata to determine anode/cathode
            // ordering. SPICE convention: node1 = anode, node2 = cathode.
            let (node1, node2) = if matches!(extracted_model.component_type,
                ComponentType::Diode | ComponentType::LED)
            {
                let pin_info = Self::get_pin_net_info(netlist, instance_id);
                let anode_net = pin_info.iter()
                    .find(|p| {
                        let name = p.pin_name.to_uppercase();
                        name == "A" || name == "ANODE"
                            || p.pin_direction == bhdl_netlist::PinDirection::In
                    })
                    .map(|p| p.net_name.clone());
                let cathode_net = pin_info.iter()
                    .find(|p| {
                        let name = p.pin_name.to_uppercase();
                        name == "K" || name == "CATHODE"
                            || p.pin_direction == bhdl_netlist::PinDirection::Out
                    })
                    .map(|p| p.net_name.clone());

                match (anode_net, cathode_net) {
                    (Some(anode), Some(cathode)) => {
                        info!("Diode {} pin-aware ordering: anode={}, cathode={}",
                              instance_name, anode, cathode);
                        (anode, cathode)
                    }
                    _ => {
                        // Fallback to connection order if pin metadata is ambiguous
                        warn!("Diode {} has ambiguous pin metadata, using connection order", instance_name);
                        (connected_nets[0].1.clone(), connected_nets[1].1.clone())
                    }
                }
            } else {
                // Symmetric components (resistors, capacitors): order doesn't matter
                (connected_nets[0].1.clone(), connected_nets[1].1.clone())
            };

            info!("Adding component {} ({}): {} -> {}, value={}",
                  instance_name, spice_type, node1, node2, value);

            let metadata = build_branch_metadata(&extracted_model);
            circuit.add_branch_with_metadata(
                instance_name.to_string(),
                &node1,
                &node2,
                spice_type,
                value,
                Some(instance_id),
                metadata,
            );
        } else if connected_nets.len() == 1 {
            // Single-pin components (like test points)
            debug!("Single-pin component {}: connected to {}",
                   instance_name, connected_nets[0].1);
        } else {
            warn!("Component {} has no connections", instance_name);
        }

        Ok(())
    }
    
    /// Infer component type from module name
    fn infer_component_type(&self, module_name: &str) -> String {
        let lower = module_name.to_lowercase();
        if lower.contains("voltage") && lower.contains("source") {
            "voltage_source".to_string()
        } else if lower.contains("res") || lower.starts_with('r') {
            "resistor".to_string()
        } else if lower.contains("cap") || lower.starts_with('c') {
            "capacitor".to_string()
        } else if lower.contains("led") {
            "led".to_string()
        } else if lower.contains("ind") || lower.starts_with('l') {
            "inductor".to_string()
        } else if lower.contains("diode") || lower.starts_with('d') {
            "diode".to_string()
        } else if lower.starts_with('v') {
            "voltage_source".to_string()
        } else {
            module_name.to_string()
        }
    }
    
    /// Extract value from component name (e.g., "Res_10k" -> "10k")
    fn extract_value_from_name(&self, name: &str) -> Option<String> {
        // Look for patterns like _10k, _100n, etc.
        if let Some(underscore_pos) = name.rfind('_') {
            let value_part = &name[underscore_pos + 1..];
            // Check if it looks like a value
            if value_part.chars().next()?.is_numeric() {
                return Some(value_part.to_string());
            }
        }
        
        // Look for embedded values like "R10k" or "C100n"
        if name.len() > 1 {
            let without_prefix = &name[1..];
            if without_prefix.chars().next().map(|c| c.is_numeric()).unwrap_or(false) {
                return Some(without_prefix.to_string());
            }
        }
        
        None
    }
    
    /// Get primary value from extracted model
    fn get_primary_value(&self, model: &ExtractedModel) -> f64 {
        use crate::components::ComponentType;
        
        let value = match model.component_type {
            ComponentType::Resistor => {
                model.parameters.get("resistance").copied().unwrap_or(1e3)
            }
            ComponentType::Capacitor => {
                model.parameters.get("capacitance").copied().unwrap_or(1e-6)
            }
            ComponentType::Inductor => {
                model.parameters.get("inductance").copied().unwrap_or(1e-6)
            }
            ComponentType::VoltageSource => {
                model.parameters.get("voltage").copied().unwrap_or(5.0)
            }
            _ => 1.0,
        };
        
        debug!("Primary value for {:?}: {}", model.component_type, value);
        value
    }
}

impl Default for NetlistToSpiceConverter {
    fn default() -> Self {
        Self::new()
    }
}

/// Enhanced Circuit::from_netlist using the converter
impl Circuit {
    /// Create SPICE circuit from BHDL netlist with proper models
    pub fn from_netlist_with_models(netlist: &Netlist) -> Result<Self> {
        let mut converter = NetlistToSpiceConverter::new();
        converter.convert(netlist)
    }
}

/// Project a synthesizer-produced `ExtractedModel` onto the per-branch metadata
/// HashMap consumed downstream by `stdlib_model_loader`.
///
/// This is the actual "stdlib drives SPICE behavior" bridge: stdlib `.bhdl`
/// attributes (e.g. `attribute esr = 0.05;`) reach the synthesizer, which
/// produces an `ExtractedModel` with `parameters` (numeric) and `attributes`
/// (string). This function maps the well-known names from those collections
/// onto stable `META_*` keys, which the model loader then reads to populate
/// `ComponentModel::*` variants.
///
/// Unknown attributes are intentionally not passed through — metadata is a
/// solver contract, not a free-form bag. New SPICE-relevant attributes should
/// be added to `circuit.rs` (the `META_*` constants) and to this function in
/// the same change.
fn build_branch_metadata(model: &ExtractedModel) -> HashMap<String, String> {
    let mut meta = HashMap::new();

    // Component class is the only string-typed key worth pulling through
    // verbatim today; everything else is numeric.
    if let Some(class) = model.attributes.get("component_class") {
        meta.insert(META_COMPONENT_CLASS.to_string(), class.clone());
    }

    // Read a numeric attribute. Prefer `parameters` (typed `f64`); fall back to
    // `attributes` only if the string parses cleanly as `f64`. This matches
    // the convention already established by the regulator path's `read_param`.
    let put_num = |meta: &mut HashMap<String, String>, src: &str, dst: &str| {
        if let Some(v) = model.parameters.get(src) {
            meta.insert(dst.to_string(), v.to_string());
        } else if let Some(s) = model.attributes.get(src) {
            if s.parse::<f64>().is_ok() {
                meta.insert(dst.to_string(), s.clone());
            }
        }
    };
    let put_str = |meta: &mut HashMap<String, String>, src: &str, dst: &str| {
        if let Some(s) = model.attributes.get(src) {
            meta.insert(dst.to_string(), s.clone());
        }
    };

    // Passive & semiconductor parameters that affect AC/transient solves
    // or that downstream safety analyses read out of branch metadata.
    put_num(&mut meta, "tolerance",            META_TOLERANCE);
    put_num(&mut meta, "power_rating",         META_POWER_RATING);
    put_num(&mut meta, "esr",                  META_ESR);
    put_num(&mut meta, "esl",                  META_ESL);
    put_num(&mut meta, "voltage_rating",       META_VOLTAGE_RATING);
    put_num(&mut meta, "dcr",                  META_DCR);
    put_num(&mut meta, "saturation_current",   META_SATURATION_CURRENT);
    put_num(&mut meta, "emission_coefficient", META_EMISSION_COEFFICIENT);
    put_num(&mut meta, "thermal_voltage",      META_THERMAL_VOLTAGE);
    put_num(&mut meta, "forward_voltage",      META_FORWARD_VOLTAGE);
    put_num(&mut meta, "forward_current",      META_FORWARD_CURRENT);
    put_num(&mut meta, "max_current",          META_MAX_CURRENT);
    put_num(&mut meta, "max_voltage",          META_MAX_VOLTAGE);
    put_num(&mut meta, "max_power",            META_MAX_POWER);
    put_num(&mut meta, "temp_min",             META_TEMP_MIN);
    put_num(&mut meta, "temp_max",             META_TEMP_MAX);

    // Free-form variant tag (LED `color`, future diode/transistor families).
    // The loader uses this to dispatch into fallback Rust LUTs only when the
    // numeric attributes above are not present.
    put_str(&mut meta, "color",   META_VARIANT);
    put_str(&mut meta, "variant", META_VARIANT);

    meta
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_value_extraction() {
        let converter = NetlistToSpiceConverter::new();
        
        assert_eq!(converter.extract_value_from_name("Res_10k"), Some("10k".to_string()));
        assert_eq!(converter.extract_value_from_name("Cap_100nF"), Some("100nF".to_string()));
        assert_eq!(converter.extract_value_from_name("R10k"), Some("10k".to_string()));
        assert_eq!(converter.extract_value_from_name("C100n"), Some("100n".to_string()));
        assert_eq!(converter.extract_value_from_name("LED"), None);
    }
    
    #[test]
    fn test_component_type_inference() {
        let converter = NetlistToSpiceConverter::new();

        assert_eq!(converter.infer_component_type("Resistor"), "resistor");
        assert_eq!(converter.infer_component_type("Res_10k"), "resistor");
        assert_eq!(converter.infer_component_type("Cap_100n"), "capacitor");
        assert_eq!(converter.infer_component_type("LED_Red"), "led");
        assert_eq!(converter.infer_component_type("Diode_1N4148"), "diode");
    }

    #[test]
    fn triode_instance_becomes_a_device() {
        // A triode instance must convert to a multi-terminal Circuit *device*
        // (DeviceKind::Triode), not a branch. This exercises the whole path:
        // component_class="triode" → registry → ComponentType::Triode → the
        // netlist_converter device-emitting case, with the Koren parameters
        // carried through and the three terminals classified by pin name.
        let mut netlist = Netlist::new();

        // Triode module: component_class + a non-default (12AU7) Koren set, so
        // the test also proves the parameters flow through rather than the
        // 6SN7 fallback defaults being substituted.
        let triode_mod =
            netlist.add_module("Triode".to_string(), ModuleKind::PhysicalComponent);
        if let Some(m) = netlist.modules.get_mut(triode_mod) {
            m.attributes.insert("component_class".to_string(), "triode".to_string());
            m.attributes.insert("mu".to_string(), "21.5".to_string());
            m.attributes.insert("ex".to_string(), "1.3".to_string());
            m.attributes.insert("kg1".to_string(), "1180".to_string());
            m.attributes.insert("kp".to_string(), "84".to_string());
            m.attributes.insert("kvb".to_string(), "300".to_string());
        }
        netlist.add_pin(triode_mod, "P".to_string(),
            bhdl_netlist::PinDirection::InOut, bhdl_netlist::PinType::Signal).unwrap();
        netlist.add_pin(triode_mod, "G".to_string(),
            bhdl_netlist::PinDirection::In, bhdl_netlist::PinType::Signal).unwrap();
        netlist.add_pin(triode_mod, "K".to_string(),
            bhdl_netlist::PinDirection::InOut, bhdl_netlist::PinType::Signal).unwrap();

        let v1 = netlist.add_instance("V1".to_string(), triode_mod).unwrap();
        let v1_pins = netlist.create_pin_instances(v1).unwrap();

        let plate = netlist.add_net(Some("PLATE_NET".to_string()));
        let grid = netlist.add_net(Some("GRID_NET".to_string()));
        let cathode = netlist.add_net(Some("GND".to_string()));
        netlist.connect(plate, ConnectionPoint::PinInstance(v1_pins[0])).unwrap();
        netlist.connect(grid, ConnectionPoint::PinInstance(v1_pins[1])).unwrap();
        netlist.connect(cathode, ConnectionPoint::PinInstance(v1_pins[2])).unwrap();

        let mut converter = NetlistToSpiceConverter::new();
        let circuit = converter.convert(&netlist).unwrap();

        // Exactly one device (glacier_production's view) AND exactly one
        // KorenTriode plate branch (SpiceEquationSystem's view — the DC
        // path stamps branches only; without it the ecc83-pp SRPP solved
        // as a resistor network). Each solver ignores the other's
        // representation, so nothing double-stamps.
        assert_eq!(circuit.devices().len(), 1, "expected one triode device");
        let branches: Vec<_> = circuit.branches().collect();
        assert_eq!(branches.len(), 1, "expected the KorenTriode plate branch");
        let (_, plate_branch) = branches[0];
        assert_eq!(plate_branch.component_type, "KorenTriode");
        assert_eq!(
            plate_branch
                .metadata
                .get(crate::circuit::META_TRIODE_GRID_NODE)
                .map(String::as_str),
            Some("GRID_NET"),
            "grid net must ride the branch metadata"
        );

        let device = &circuit.devices()[0];
        assert_eq!(device.name, "V1");
        match device.kind {
            DeviceKind::Triode { mu, ex, kg1, kp, kvb } => assert_eq!(
                (mu, ex, kg1, kp, kvb),
                (21.5, 1.3, 1180.0, 84.0, 300.0),
                "Koren parameters did not flow through the converter",
            ),
        }
        // Terminals are [plate, grid, cathode] by the Koren convention.
        let term = |i: usize| circuit.get_node_name(device.terminals[i]).unwrap();
        assert_eq!(term(0), "PLATE_NET");
        assert_eq!(term(1), "GRID_NET");
        assert_eq!(term(2), "GND");
    }
}