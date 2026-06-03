// GLACIER-Driven Component Physical Selection
//
// Uses GLACIER DC simulation results (voltage, current, power at every node)
// to automatically determine physical parameters for passive components:
// package size, voltage rating, power rating, dielectric type.
//
// Results are written as instance attributes on the netlist, flowing
// naturally through to the schematic viewer.

use std::collections::HashMap;
use log::{debug, info};
use bhdl_analyzer::spice_extraction::parse_unit_value;
use bhdl_netlist::{ConnectionPoint, NetId, Netlist};

use crate::passive_component_calculator::{DielectricType, PackageSize, PassiveComponentCalculator};
use crate::package_selector::{PackageSelector, ApplicationRequirements};

/// Summary of a single component's physical selection result.
#[derive(Debug)]
pub struct PhysicalSelectionResult {
    pub instance_name: String,
    pub component_type: String,
    pub package: String,
    pub power_rating: Option<String>,
    pub voltage_rating: Option<String>,
    pub dielectric: Option<String>,
}

/// Describes a capacitor that must be split into a parallel bank.
#[derive(Debug)]
struct BankSplit {
    original_id: bhdl_netlist::InstanceId,
    original_name: String,
    count: usize,
    per_unit_value: String,
    package: String,
    voltage_rating: Option<String>,
    dielectric: Option<String>,
    /// Propagated from original instance so bank children stay grouped
    /// with their virtual-pin expansion parent in the schematic layout.
    vpin_parent: Option<String>,
    vpin_role: Option<String>,
    /// Propagated stage/intent metadata so bank children share their
    /// parent's stage coloring and intent in the schematic viewer.
    stage_name: Option<String>,
    stage_order: Option<String>,
    stage_rail: Option<String>,
}

/// Format a capacitance value in Farads as a human-readable string.
fn format_cap_value(farads: f64) -> String {
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

/// Find the two nets connected to an instance's pins (pin 1 and pin 2).
/// Returns (net_for_pin1, net_for_pin2) by scanning the netlist connections.
fn find_instance_nets(
    netlist: &Netlist,
    inst_id: bhdl_netlist::InstanceId,
) -> (Option<NetId>, Option<NetId>) {
    let instance = match netlist.instances.get(inst_id) {
        Some(i) => i,
        None => return (None, None),
    };
    let module_def = match netlist.modules.get(instance.definition) {
        Some(d) => d,
        None => return (None, None),
    };

    // Collect pin instances for this instance, ordered by pin definition
    let mut pin_nets: Vec<Option<NetId>> = Vec::new();
    for &pin_id in &module_def.pins {
        // Find the pin instance for (this instance, this pin_def)
        let pi_id = netlist.pin_instances.iter()
            .find(|(_, pi)| pi.instance == inst_id && pi.pin_def == pin_id)
            .map(|(id, _)| id);

        let net_id = pi_id.and_then(|pi_id| {
            // Scan nets for one containing this pin instance (authoritative)
            let target = ConnectionPoint::PinInstance(pi_id);
            netlist.nets.iter()
                .find(|(_, net)| net.connections.contains(&target))
                .map(|(nid, _)| nid)
        });
        pin_nets.push(net_id);
    }

    let net1 = pin_nets.first().copied().flatten();
    let net2 = pin_nets.get(1).copied().flatten();
    (net1, net2)
}

/// Apply GLACIER simulation results to select physical parameters for passive components.
///
/// Iterates over all netlist instances, identifies resistors and capacitors,
/// and uses the simulation-derived current/power/voltage to select appropriate
/// package sizes, voltage ratings, power ratings, and dielectric types.
///
/// Selected parameters are written directly as instance attributes.
pub fn apply_glacier_physical_selection(
    netlist: &mut Netlist,
    instance_currents: &HashMap<String, f64>,
    instance_power: &HashMap<String, f64>,
    net_voltages: &HashMap<String, f64>,
) -> Vec<PhysicalSelectionResult> {
    let calculator = PassiveComponentCalculator::new();
    let selector = PackageSelector::new();
    let requirements = ApplicationRequirements::default();
    let mut results = Vec::new();

    // Build a map from instance name to the net names it touches,
    // so we can look up max voltage across a capacitor.
    let instance_net_voltages = compute_instance_max_voltages(netlist, net_voltages);

    // Build per-net total load current (sum of absolute currents of all
    // non-source instances touching each net). Used to infer inductor current.
    let net_load_currents = compute_net_load_currents(netlist, instance_currents);

    // Collect instance IDs first to avoid borrow conflicts
    let instance_ids: Vec<_> = netlist.instances.keys().collect();

    // Bank splits collected during the loop, applied afterwards to avoid
    // mutating the netlist while iterating over instances.
    let mut bank_splits: Vec<BankSplit> = Vec::new();

    for inst_id in instance_ids {
        let inst = &netlist.instances[inst_id];
        let inst_name = inst.name.clone();
        let def_id = inst.definition;
        let attrs = inst.attributes.clone();

        let component_class = classify_component(netlist, def_id, &attrs);

        match component_class.as_deref() {
            Some("resistor") => {
                if let Some(result) = select_resistor_physical(
                    &inst_name,
                    &attrs,
                    instance_currents,
                    instance_power,
                    &instance_net_voltages,
                    &calculator,
                    &selector,
                    &requirements,
                ) {
                    // Write attributes back to the instance
                    let inst_mut = &mut netlist.instances[inst_id];
                    inst_mut.attributes.insert("package".to_string(), result.package.clone());
                    if let Some(ref pr) = result.power_rating {
                        inst_mut.attributes.insert("power_rating".to_string(), pr.clone());
                    }
                    if let Some(ref vr) = result.voltage_rating {
                        inst_mut.attributes.insert("voltage_rating".to_string(), vr.clone());
                    }
                    results.push(result);
                }
            }
            Some("capacitor") => {
                if let Some(result) = select_capacitor_physical(
                    &inst_name,
                    &attrs,
                    &instance_net_voltages,
                    &calculator,
                    &selector,
                    &requirements,
                ) {
                    // Check if the capacitance exceeds what's realizable in one part
                    let capacitance = attrs.get("value")
                        .and_then(|v| parse_unit_value(v));
                    let max_per_unit = result.dielectric.as_ref()
                        .and_then(|d| DielectricType::from_display_str(d))
                        .and_then(|dt| PackageSize::from_str(&result.package).map(|ps| (dt, ps)))
                        .map(|(dt, ps)| PackageSelector::max_realizable_capacitance(dt, ps));

                    let needs_split = match (capacitance, max_per_unit) {
                        (Some(c), Some(max)) => c > max * 1.05,
                        _ => false,
                    };

                    if needs_split {
                        let c = capacitance.unwrap();
                        let max = max_per_unit.unwrap();
                        let count = (c / max).ceil() as usize;
                        let per_unit = c / count as f64;
                        let per_unit_str = format_cap_value(per_unit);
                        let total_str = format_cap_value(c);

                        info!(
                            "Capacitor bank split: {} ({}) → {}× {}",
                            inst_name, total_str, count, per_unit_str
                        );

                        // Update original instance to per-unit value
                        let inst_mut = &mut netlist.instances[inst_id];
                        inst_mut.attributes.insert("value".to_string(), per_unit_str.clone());
                        inst_mut.attributes.insert("bank_count".to_string(), count.to_string());
                        inst_mut.attributes.insert("bank_total".to_string(), total_str);
                        inst_mut.attributes.insert("package".to_string(), result.package.clone());
                        if let Some(ref vr) = result.voltage_rating {
                            inst_mut.attributes.insert("voltage_rating".to_string(), vr.clone());
                        }
                        if let Some(ref di) = result.dielectric {
                            inst_mut.attributes.insert("dielectric".to_string(), di.clone());
                        }

                        // Schedule creation of (count - 1) additional parallel instances.
                        // Propagate vpin_parent/vpin_role so bank children stay grouped
                        // with their expansion parent in the schematic layout.
                        bank_splits.push(BankSplit {
                            original_id: inst_id,
                            original_name: inst_name.clone(),
                            count,
                            per_unit_value: per_unit_str,
                            package: result.package.clone(),
                            voltage_rating: result.voltage_rating.clone(),
                            dielectric: result.dielectric.clone(),
                            vpin_parent: attrs.get("vpin_parent").cloned(),
                            vpin_role: attrs.get("vpin_role").cloned(),
                            stage_name: attrs.get("stage_name").cloned(),
                            stage_order: attrs.get("stage_order").cloned(),
                            stage_rail: attrs.get("stage_rail").cloned(),
                        });

                        results.push(PhysicalSelectionResult {
                            instance_name: inst_name,
                            component_type: "capacitor".to_string(),
                            package: result.package,
                            power_rating: None,
                            voltage_rating: result.voltage_rating,
                            dielectric: result.dielectric,
                        });
                    } else {
                        // Normal single-cap path
                        let inst_mut = &mut netlist.instances[inst_id];
                        inst_mut.attributes.insert("package".to_string(), result.package.clone());
                        if let Some(ref vr) = result.voltage_rating {
                            inst_mut.attributes.insert("voltage_rating".to_string(), vr.clone());
                        }
                        if let Some(ref di) = result.dielectric {
                            inst_mut.attributes.insert("dielectric".to_string(), di.clone());
                        }
                        results.push(result);
                    }
                }
            }
            Some("inductor") => {
                if let Some(result) = select_inductor_physical(
                    &inst_name,
                    inst_id,
                    &attrs,
                    instance_currents,
                    &net_load_currents,
                    netlist,
                ) {
                    let inst_mut = &mut netlist.instances[inst_id];
                    inst_mut.attributes.insert("package".to_string(), result.package.clone());
                    if let Some(ref pr) = result.power_rating {
                        inst_mut.attributes.insert("power_rating".to_string(), pr.clone());
                    }
                    if let Some(ref vr) = result.voltage_rating {
                        inst_mut.attributes.insert("current_rating".to_string(), vr.clone());
                    }
                    // Also store DCR and saturation current
                    if let Some(ref di) = result.dielectric {
                        inst_mut.attributes.insert("dcr".to_string(), di.clone());
                    }
                    results.push(result);
                }
            }
            _ => {
                // Not a passive component we handle — skip
            }
        }
    }

    // ── Phase 2: create additional parallel instances for bank splits ────
    for split in &bank_splits {
        let (net_pin1, net_pin2) = find_instance_nets(netlist, split.original_id);

        // Find or create a Cap module with pins "1" and "2"
        let cap_mod = crate::virtual_pin_expander::find_or_create_module(
            netlist, "Cap", &[("1", true), ("2", true)],
        );

        for i in 1..split.count {
            let name = format!("{}_{}", split.original_name, i + 1);
            let count_str = split.count.to_string();
            let mut attrs: Vec<(&str, &str)> = vec![
                ("component_class", "capacitor"),
                ("value", &split.per_unit_value),
                ("bank_count", &count_str),
                ("bank_parent", &split.original_name),
                ("package", &split.package),
            ];

            if let Some(ref vr) = split.voltage_rating {
                attrs.push(("voltage_rating", vr));
            }
            if let Some(ref di) = split.dielectric {
                attrs.push(("dielectric", di));
            }
            // Propagate expansion metadata so schematic groups bank children
            // with the virtual-pin parent (e.g. buck regulator)
            if let Some(ref vp) = split.vpin_parent {
                attrs.push(("vpin_parent", vp));
            }
            if let Some(ref vr) = split.vpin_role {
                attrs.push(("vpin_role", vr));
            }
            // Propagate stage/intent metadata so bank children share
            // parent's stage coloring in the schematic viewer
            if let Some(ref sn) = split.stage_name {
                attrs.push(("stage_name", sn));
            }
            if let Some(ref so) = split.stage_order {
                attrs.push(("stage_order", so));
            }
            if let Some(ref sr) = split.stage_rail {
                attrs.push(("stage_rail", sr));
            }

            let new_id = crate::virtual_pin_expander::create_instance(
                netlist, &name, cap_mod, &attrs,
            );
            let pins = netlist.create_pin_instances(new_id)
                .unwrap_or_default();

            // Connect pins to the same nets as the original capacitor
            if let Some(n1) = net_pin1 {
                let _ = crate::virtual_pin_expander::connect_pin_instance_by_name(
                    netlist, new_id, &pins, "1", n1,
                );
            }
            if let Some(n2) = net_pin2 {
                let _ = crate::virtual_pin_expander::connect_pin_instance_by_name(
                    netlist, new_id, &pins, "2", n2,
                );
            }

            debug!("  Created bank instance {} on same nets as {}", name, split.original_name);
        }
    }

    if !bank_splits.is_empty() {
        info!("Capacitor bank splitting: {} capacitor(s) split into parallel banks", bank_splits.len());
    }

    if !results.is_empty() {
        info!("Physical selection applied to {} components", results.len());
        for r in &results {
            debug!(
                "  {} ({}): package={}, power={}, voltage={}, dielectric={}",
                r.instance_name,
                r.component_type,
                r.package,
                r.power_rating.as_deref().unwrap_or("-"),
                r.voltage_rating.as_deref().unwrap_or("-"),
                r.dielectric.as_deref().unwrap_or("-"),
            );
        }
    }

    results
}

/// Determine the component class from the module definition name and instance attributes.
/// Catalog-driven physical selection — the unification of value snapping
/// with rating/package selection. For each passive, compute its stress
/// (voltage across, current through, power) from the GLACIER results + net
/// voltages, then ask the catalog (`value_snap::select_family`) for the
/// smallest-package `part_family` whose E-series value range covers the
/// part AND whose ratings cover the *derated* stress. When one is found,
/// override the package/ratings the hardcoded ladder picked and snap the
/// `value` to that family's series. Falls through (leaves the ladder
/// result untouched) when no catalog family matches — so it never
/// regresses a part the catalogue doesn't cover. Returns the count
/// overridden. Runs after [`apply_glacier_physical_selection`].
/// Resolve real, orderable MPNs for the catalogue-selected passives via an
/// external supply-chain provider (e.g. the bundled jlcparts provider),
/// and write `mpn`/`manufacturer`/`lcsc_pn`/`stock` onto the instances so
/// the BOM names real parts. Runs after [`apply_catalog_physical_selection`]
/// (which fixed value + package); the provider turns (class, value,
/// package) → a real part.
///
/// The provider is any executable named by `$BHDL_SUPPLY_PROVIDER`
/// (whitespace-split into program + args; e.g.
/// `python3 .../bhdl_jlcparts_provider.py`). If that is unset, the bundled
/// zero-dependency Rust provider (`bhdl-jlcparts-provider`, found next to
/// the running executable or on `PATH`) is used automatically whenever a
/// jlcparts DB is available via `$BHDL_JLCPARTS_DB` — no Python or system
/// SQLite required. Best-effort: no provider, a spawn failure, or an
/// unparseable reply ⇒ leaves the catalogue result untouched (no MPN),
/// never errors the build. Reuses the JSON stdin/stdout plugin protocol
/// (`bhdl_analyzer::plugin`).
/// Zero-config default: the bundled Rust `bhdl-jlcparts-provider`, used
/// only when a catalogue DB is available via `$BHDL_JLCPARTS_DB`. Resolves
/// the binary next to the current executable first (the normal install
/// layout), then falls back to bare `bhdl-jlcparts-provider` on `PATH`.
/// Returns the whitespace-joined `program [db-path]` spec, or `None` when
/// no DB is configured (so the build stays MPN-less rather than erroring).
fn default_provider_spec() -> Option<String> {
    let db = std::env::var("BHDL_JLCPARTS_DB")
        .ok()
        .filter(|s| !s.trim().is_empty())?;
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("bhdl-jlcparts-provider")))
        .filter(|p| p.exists())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "bhdl-jlcparts-provider".to_string());
    Some(format!("{exe} {db}"))
}

/// Supply-chain optimization policy passed to [`apply_supply_chain_mpns`].
///
/// Resolved per passive with **three-level precedence** so the objective is
/// selectable at synthesis time AND overridable per part / per net:
/// 1. the instance's own `supply_profile` / `supply_weights` / `supply_qty`
///    attribute (set in BHDL source — travels with the design);
/// 2. the policy of a net the passive connects to (`net_profiles`, keyed by
///    net name — e.g. `FB=precision`, `VCC=cost`);
/// 3. the global default (`profile` / `quantity`).
///
/// The provider itself defaults to `balanced` when nothing is specified.
#[derive(Debug, Default, Clone)]
pub struct SupplyOptions {
    /// Global default objective (profile name, e.g. "cost"/"precision").
    pub profile: Option<String>,
    /// Global default build quantity (price tier + stock headroom).
    pub quantity: Option<u64>,
    /// Per-net objective overrides, keyed by net name.
    pub net_profiles: HashMap<String, String>,
}

impl SupplyOptions {
    /// Fill any unset field from the environment:
    /// `BHDL_SUPPLY_PROFILE`, `BHDL_SUPPLY_QTY`,
    /// `BHDL_SUPPLY_NET_PROFILES="VCC=cost,FB=precision"`.
    pub fn with_env_fallback(mut self) -> Self {
        if self.profile.is_none() {
            self.profile = std::env::var("BHDL_SUPPLY_PROFILE")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }
        if self.quantity.is_none() {
            self.quantity = std::env::var("BHDL_SUPPLY_QTY")
                .ok()
                .and_then(|s| s.trim().parse().ok());
        }
        if self.net_profiles.is_empty() {
            if let Ok(s) = std::env::var("BHDL_SUPPLY_NET_PROFILES") {
                self.net_profiles = parse_net_profiles(&s);
            }
        }
        self
    }
}

/// Parse a tolerance/percentage attribute robustly into *percent*:
/// `"1%"` → 1.0, `"0.05%"` → 0.05, a bare fraction `"0.05"` → 5.0, a bare
/// number `"5"` → 5.0. (A value ≤ 1 with no `%` is read as a fraction, the
/// convention the stdlib uses, e.g. `attribute tolerance = 0.05`.)
fn parse_pct(s: &str) -> Option<f64> {
    let t = s.trim().trim_matches('"').trim();
    if let Some(stripped) = t.strip_suffix('%') {
        stripped.trim().parse::<f64>().ok()
    } else {
        t.parse::<f64>()
            .ok()
            .map(|v| if v <= 1.0 { v * 100.0 } else { v })
    }
}

/// Parse a current attribute into amps: `"2A"` → 2.0, `"500mA"` → 0.5,
/// `"0.5"` (bare) → 0.5. Robust to surrounding quotes/space.
fn parse_amps(s: &str) -> Option<f64> {
    let t = s.trim().trim_matches('"').trim();
    let t = t.strip_suffix('A').or_else(|| t.strip_suffix('a')).unwrap_or(t).trim();
    if let Some(num) = t.strip_suffix('m').or_else(|| t.strip_suffix('M')) {
        num.trim().parse::<f64>().ok().map(|v| v * 1e-3)
    } else {
        t.parse::<f64>().ok()
    }
}

/// Parse `"VCC=cost,FB=precision"` into a net-name → profile map.
pub fn parse_net_profiles(s: &str) -> HashMap<String, String> {
    s.split(',')
        .filter_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            let (k, v) = (k.trim(), v.trim());
            if k.is_empty() || v.is_empty() {
                None
            } else {
                Some((k.to_string(), v.to_string()))
            }
        })
        .collect()
}

/// instance id → sorted distinct connected net names (for per-net policy).
fn instance_connected_nets(
    netlist: &Netlist,
) -> HashMap<bhdl_netlist::InstanceId, Vec<String>> {
    let mut out: HashMap<bhdl_netlist::InstanceId, Vec<String>> = HashMap::new();
    for (_net_id, net) in &netlist.nets {
        let Some(net_name) = &net.name else { continue };
        for conn in &net.connections {
            let iid = match conn {
                bhdl_netlist::ConnectionPoint::InstancePort(iid, _)
                | bhdl_netlist::ConnectionPoint::InstancePin(iid, _) => Some(*iid),
                bhdl_netlist::ConnectionPoint::PinInstance(pi_id) => {
                    netlist.pin_instances.get(*pi_id).map(|pi| pi.instance)
                }
                _ => None,
            };
            if let Some(iid) = iid {
                out.entry(iid).or_default().push(net_name.clone());
            }
        }
    }
    for nets in out.values_mut() {
        nets.sort();
        nets.dedup();
    }
    out
}

/// One resolved supply-chain selection, returned so the caller can pin it in
/// `bhdl.lock`. `refdes` is the instance's structural name (a stable key).
#[derive(Debug, Clone)]
pub struct ResolvedPart {
    pub refdes: String,
    pub mpn: String,
    pub manufacturer: Option<String>,
    pub vendor_sku: Option<String>,
    pub provider: Option<String>,
}

/// Apply previously-pinned MPNs (from `bhdl.lock`) onto matching instances by
/// structural name, WITHOUT calling any provider. Returns how many applied.
pub fn apply_locked_parts(netlist: &mut Netlist, parts: &[ResolvedPart]) -> usize {
    use std::collections::HashMap;
    let by_name: HashMap<String, bhdl_netlist::InstanceId> = netlist
        .instances
        .iter()
        .map(|(id, inst)| (inst.name.clone(), id))
        .collect();
    let mut n = 0;
    for p in parts {
        let Some(&id) = by_name.get(&p.refdes) else { continue };
        let Some(inst) = netlist.instances.get_mut(id) else { continue };
        inst.attributes.insert("mpn".to_string(), p.mpn.clone());
        if let Some(m) = &p.manufacturer {
            inst.attributes.insert("manufacturer".to_string(), m.clone());
        }
        if let Some(sku) = &p.vendor_sku {
            inst.attributes.insert("lcsc_pn".to_string(), sku.clone());
        }
        n += 1;
    }
    n
}

pub fn apply_supply_chain_mpns(
    netlist: &mut Netlist,
    opts: &SupplyOptions,
    net_voltages: &HashMap<String, f64>,
    instance_power: &HashMap<String, f64>,
) -> Vec<ResolvedPart> {
    let spec = match std::env::var("BHDL_SUPPLY_PROVIDER") {
        Ok(s) if !s.trim().is_empty() => s,
        _ => match default_provider_spec() {
            Some(s) => s,
            None => return Vec::new(),
        },
    };

    let inst_nets = instance_connected_nets(netlist);
    // Per-instance operating stress for the V/P gates: cap voltage from the
    // (declared or simulated) rail voltages across the part; resistor power
    // from the simulated dissipation. Derated 2× to match the catalogue
    // selection's headroom convention. Inductor current rides the
    // `current_rating` attribute instead (recipe seed or GLACIER).
    let inst_v = compute_instance_max_voltages(netlist, net_voltages);
    const CAP_V_DERATE: f64 = 2.0;
    const RES_P_DERATE: f64 = 2.0;

    // Build the requirement list from the selected passives; track
    // class_index → InstanceId for applying the reply.
    let mut idx_to_id: Vec<bhdl_netlist::InstanceId> = Vec::new();
    // Parallel to `idx_to_id`: a human-readable summary of the derated
    // stress gate(s) applied to each requirement, used to explain an
    // UNPOPULATED part when the provider can find nothing that clears them.
    let mut gate_summary: Vec<String> = Vec::new();
    let mut reqs: Vec<serde_json::Value> = Vec::new();
    for id in netlist.instances.keys().collect::<Vec<_>>() {
        let inst = &netlist.instances[id];
        let Some(class) = classify_component(netlist, inst.definition, &inst.attributes) else {
            continue;
        };
        if !matches!(class.as_str(), "resistor" | "capacitor" | "inductor") {
            continue;
        }
        let Some(value) = inst
            .attributes
            .get("value")
            .and_then(|s| bhdl_analyzer::value_snap::parse_value_string(s))
        else {
            continue;
        };
        let package = inst
            .attributes
            .get("physical_package")
            .or_else(|| inst.attributes.get("package"))
            .cloned();
        // Value-match window (how close the catalogue nominal must be to the
        // computed/snapped target). Read literally (NOT via parse_pct): the
        // stdlib's `tolerance = 0.05` keeps this a tight ~0.05% window so the
        // exact E-series value is pinned (an adjacent standard value like
        // 120 vs 121 — 0.83% off — is excluded), while a recipe's
        // `tolerance = 1%` widens it to 1%. Decoupled from the grade gate.
        let tolerance_pct = inst
            .attributes
            .get("tolerance_pct")
            .or_else(|| inst.attributes.get("tolerance"))
            .and_then(|s| s.trim().trim_end_matches('%').trim().parse::<f64>().ok())
            .unwrap_or(2.0);
        // Hard gate on the part's *grade* (±%): the selected part must be at
        // least this good. Sourced from an explicit `max_tolerance`, else the
        // part's own declared `tolerance` spec (`tolerance: 1%` on a feedback
        // resistor → ≤1% parts; the default `0.05` → ≤5%, which excludes
        // almost nothing). Robust to "1%", "0.05" (fraction), and "5".
        let max_tolerance_pct = inst
            .attributes
            .get("max_tolerance")
            .or_else(|| inst.attributes.get("max_tol"))
            .or_else(|| inst.attributes.get("tolerance"))
            .and_then(|s| parse_pct(s));

        // per-requirement objective: instance attr > per-net policy > none
        // (none ⇒ the top-level default applies in the provider).
        let objective: Option<serde_json::Value> = inst
            .attributes
            .get("supply_weights")
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .or_else(|| {
                inst.attributes
                    .get("supply_profile")
                    // raw stamped values can carry surrounding quotes/space
                    // (e.g. ` "grade"`) — sanitize to a bare profile name.
                    .map(|p| p.trim().trim_matches('"').trim())
                    .filter(|p| !p.is_empty())
                    .map(|p| serde_json::Value::String(p.to_string()))
            })
            .or_else(|| {
                // first connected net (sorted) carrying a policy
                inst_nets.get(&id).and_then(|nets| {
                    nets.iter()
                        .find_map(|n| opts.net_profiles.get(n))
                        .map(|p| serde_json::Value::String(p.clone()))
                })
            });
        let quantity: Option<u64> = inst
            .attributes
            .get("supply_qty")
            .and_then(|s| s.trim().parse().ok());
        // Required ceramic dielectric (e.g. `dielectric = "C0G"` on a
        // filter/timing/reference cap) → hard gate in the provider. Sanitize
        // the raw stamped value's surrounding quotes/space.
        let dielectric: Option<String> = inst
            .attributes
            .get("dielectric")
            .map(|d| d.trim().trim_matches('"').trim())
            .filter(|d| !d.is_empty())
            .map(str::to_string);
        // Required inductor rated current (amps) → hard gate in the provider.
        // Precedence: GLACIER-derived `current_rating` (the simulated
        // operating point, stamped by apply_glacier_physical_selection when a
        // DC solve ran) wins; else the recipe/board closed-form
        // `rated_current` (e.g. the buck's design block, or
        // `Ind(value, rated_current = "2A")`); else a bare `current`.
        let current_a: Option<f64> = inst
            .attributes
            .get("current_rating")
            .or_else(|| inst.attributes.get("rated_current"))
            .or_else(|| inst.attributes.get("current"))
            .and_then(|s| parse_amps(s));
        // Capacitor voltage gate: derated operating voltage across the part
        // (from declared rails in the BOM path, or sim node voltages under
        // --simulate). Resistor power gate: derated simulated dissipation
        // (no value in the no-sim BOM path → no gate then).
        let voltage_v: Option<f64> = if class == "capacitor" {
            inst_v
                .get(&inst.name)
                .copied()
                .filter(|v| *v > 1e-9)
                .map(|v| v * CAP_V_DERATE)
        } else {
            None
        };
        let power_w: Option<f64> = if class == "resistor" {
            instance_power
                .get(&inst.name)
                .copied()
                .map(f64::abs)
                .filter(|p| *p > 1e-12)
                .map(|p| p * RES_P_DERATE)
        } else {
            None
        };

        let ci = idx_to_id.len();
        idx_to_id.push(id);
        // Record the derated stress gate(s) for this requirement so an
        // unfillable one can be explained precisely.
        let mut gates: Vec<String> = Vec::new();
        if let Some(v) = voltage_v {
            gates.push(format!("V≥{v:.3}V (derated {CAP_V_DERATE}×)"));
        }
        if let Some(p) = power_w {
            gates.push(format!("P≥{p:.4}W (derated {RES_P_DERATE}×)"));
        }
        if let Some(c) = current_a {
            gates.push(format!("I≥{c:.4}A"));
        }
        gate_summary.push(if gates.is_empty() {
            "value/package/tolerance only (no V/I/P stress gate)".to_string()
        } else {
            gates.join(", ")
        });
        let mut req = serde_json::json!({
            "class_index": ci,
            "class": class,
            "value": value,
            "package": package,
            "tolerance_pct": tolerance_pct,
        });
        if let Some(m) = max_tolerance_pct {
            req["max_tolerance_pct"] = serde_json::json!(m);
        }
        if let Some(d) = dielectric {
            req["dielectric"] = serde_json::Value::String(d);
        }
        if let Some(c) = current_a {
            req["current_a"] = serde_json::json!(c);
        }
        if let Some(v) = voltage_v {
            req["voltage_v"] = serde_json::json!(v);
        }
        if let Some(p) = power_w {
            req["power_w"] = serde_json::json!(p);
        }
        if let Some(o) = objective {
            req["objective"] = o;
        }
        if let Some(q) = quantity {
            req["quantity"] = serde_json::json!(q);
        }
        reqs.push(req);
    }
    if reqs.is_empty() {
        return Vec::new();
    }
    let mut top = serde_json::json!({ "protocol": 1, "requirements": reqs });
    if let Some(p) = &opts.profile {
        top["objective"] = serde_json::Value::String(p.clone());
    }
    if let Some(q) = opts.quantity {
        top["quantity"] = serde_json::json!(q);
    }
    let payload = top.to_string();

    // Spawn the provider directly with our requirements payload (the
    // plugin.rs `run_plugin` helper is hardcoded to a CandidateBundle
    // input; we use the leaner supply-requirements JSON), and parse its
    // reply with the shared `PluginResponse` type.
    use std::io::Write;
    use std::process::Stdio;
    let mut parts = spec.split_whitespace();
    let prog = match parts.next() {
        Some(p) => p,
        None => return Vec::new(),
    };
    let provider_name = prog.rsplit('/').next().unwrap_or(prog).to_string();
    let mut cmd = std::process::Command::new(prog);
    for a in parts {
        cmd.arg(a);
    }
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("supply-chain provider `{prog}` failed to spawn: {e}");
            return Vec::new();
        }
    };
    if let Some(mut si) = child.stdin.take() {
        let _ = si.write_all(payload.as_bytes());
        // drop closes stdin → provider sees EOF
    }
    let out = match child.wait_with_output() {
        Ok(o) => o,
        Err(e) => {
            log::warn!("supply-chain provider wait failed: {e}");
            return Vec::new();
        }
    };
    if !out.status.success() {
        log::warn!(
            "supply-chain provider exited {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
        return Vec::new();
    }
    let resp: bhdl_analyzer::plugin::PluginResponse =
        match serde_json::from_slice(&out.stdout) {
            Ok(r) => r,
            Err(e) => {
                log::warn!("supply-chain provider reply not parseable: {e}");
                return Vec::new();
            }
        };
    for w in &resp.warnings {
        log::warn!("supply-chain provider: {w}");
    }

    // Index the provider's reply by requirement so we can both apply the
    // chosen MPNs and detect requirements it left UNFILLED (e.g. nothing in
    // the catalogue clears the derated V/I/P stress gate).
    let mut by_index: HashMap<usize, &bhdl_analyzer::plugin::PluginSelection> = HashMap::new();
    for sel in &resp.selections {
        by_index.insert(sel.class_index, sel);
    }

    let mut resolved = Vec::new();
    for (ci, &id) in idx_to_id.iter().enumerate() {
        let sel = by_index.get(&ci).copied();
        let mpn = sel.and_then(|s| s.mpn.as_ref());

        match mpn {
            Some(mpn) => {
                let Some(inst) = netlist.instances.get_mut(id) else { continue };
                inst.attributes.insert("mpn".to_string(), mpn.clone());
                if let Some(m) = sel.and_then(|s| s.manufacturer.as_ref()) {
                    inst.attributes.insert("manufacturer".to_string(), m.clone());
                }
                if let Some(sku) = sel.and_then(|s| s.vendor_sku.as_ref()) {
                    inst.attributes.insert("lcsc_pn".to_string(), sku.clone());
                }
                if let Some(s) = sel.and_then(|s| s.stock) {
                    inst.attributes.insert("stock".to_string(), s.to_string());
                }
                resolved.push(ResolvedPart {
                    refdes: inst.name.clone(),
                    mpn: mpn.clone(),
                    manufacturer: sel.and_then(|s| s.manufacturer.clone()),
                    vendor_sku: sel.and_then(|s| s.vendor_sku.clone()),
                    provider: Some(provider_name.clone()),
                });
            }
            None => {
                // The provider could not source a part for this requirement.
                // Mark it DO-NOT-POPULATE with an explicit reason and warn
                // LOUDLY — we never silently substitute a weaker part. The
                // most common cause is over-stress: the operating point
                // exceeds every catalogue family's derated rating.
                let gate = gate_summary.get(ci).map(String::as_str).unwrap_or("");
                let reason = sel
                    .and_then(|s| s.error.clone().or_else(|| s.note.clone()))
                    .filter(|n| !n.trim().is_empty())
                    .unwrap_or_else(|| {
                        "no catalogue part meets the value/stress requirement".to_string()
                    });
                let Some(inst) = netlist.instances.get_mut(id) else { continue };
                let refdes = inst.name.clone();
                let class = inst
                    .attributes
                    .get("component_class")
                    .cloned()
                    .unwrap_or_default();
                inst.attributes.insert("dnp".to_string(), "true".to_string());
                inst.attributes
                    .insert("dnp_reason".to_string(), reason.clone());
                if !gate.is_empty() {
                    inst.attributes
                        .insert("stress_gate".to_string(), gate.to_string());
                }
                log::warn!(
                    "⚠ UNPOPULATED {refdes}{}: {reason} — required {gate}. \
                     No part substituted; populate manually or relax the operating point.",
                    if class.is_empty() {
                        String::new()
                    } else {
                        format!(" ({class})")
                    },
                );
            }
        }
    }
    resolved
}

/// Declared rail voltages (net name → volts) from each `Power` net's
/// class. Used for the voltage stress when no GLACIER node-voltage solve
/// is available — e.g. the BOM path, which runs no simulation. A 2-pin
/// passive's voltage stress is then the max declared voltage across its
/// nets (via [`compute_instance_max_voltages`]).
pub fn declared_net_voltages(netlist: &Netlist) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    for (_id, net) in netlist.nets.iter() {
        if let bhdl_netlist::types::NetClass::Power(v) = net.net_class {
            if let Some(name) = &net.name {
                out.insert(name.clone(), v);
            }
        }
    }
    out
}

/// Stamp each inductor's `current_rating` from a GLACIER DC solve's branch
/// currents, WITHOUT touching package/value selection (that stays with the
/// catalogue pass). For a buck inductor whose DC branch current reads ~0 (it
/// is modelled as a short), the operating current is inferred from the
/// VOUT-side net's total load. The value stamped is the 80%-saturation
/// requirement (`I/0.8`), matching `select_inductor_physical`; the
/// supply-chain current gate consumes it, so the part is selected against the
/// simulated operating point rather than the recipe's closed-form seed.
/// Returns how many inductors were stamped.
pub fn stamp_inductor_sim_current(
    netlist: &mut Netlist,
    instance_currents: &HashMap<String, f64>,
) -> usize {
    let net_load = compute_net_load_currents(netlist, instance_currents);
    let ids: Vec<_> = netlist.instances.keys().collect();
    let mut n = 0;
    for id in ids {
        let inst = &netlist.instances[id];
        if classify_component(netlist, inst.definition, &inst.attributes).as_deref()
            != Some("inductor")
        {
            continue;
        }
        let name = inst.name.clone();
        let mut current = instance_currents.get(&name).copied().unwrap_or(0.0).abs();
        if current < 1e-6 {
            if let Some(c) = find_inductor_vout_net_current(netlist, id, &net_load) {
                current = c;
            }
        }
        if current < 1e-9 {
            continue;
        }
        let required = current / 0.8; // 80% saturation derating
        let s = if required >= 1.0 {
            format!("{required:.3}A")
        } else {
            format!("{:.0}mA", required * 1e3)
        };
        if let Some(inst_mut) = netlist.instances.get_mut(id) {
            inst_mut.attributes.insert("current_rating".to_string(), s);
            n += 1;
        }
    }
    n
}

pub fn apply_catalog_physical_selection(
    netlist: &mut Netlist,
    families: &[bhdl_analyzer::value_snap::FamilyDecl],
    instance_currents: &HashMap<String, f64>,
    instance_power: &HashMap<String, f64>,
    net_voltages: &HashMap<String, f64>,
) -> usize {
    use bhdl_analyzer::value_snap::{
        format_value, parse_value_string, select_family, snap_to_family, Stress,
    };
    if families.is_empty() {
        return 0;
    }
    let inst_v = compute_instance_max_voltages(netlist, net_voltages);
    let ids: Vec<_> = netlist.instances.keys().collect();
    // (id, class, package, voltage_rating, current_rating, power_w, value_str)
    type Plan = (
        bhdl_netlist::InstanceId,
        Option<String>,
        Option<f64>,
        Option<f64>,
        Option<f64>,
        String,
    );
    let mut plan: Vec<Plan> = Vec::new();
    for id in ids {
        let inst = &netlist.instances[id];
        let name = inst.name.clone();
        let Some(class) = classify_component(netlist, inst.definition, &inst.attributes) else {
            continue;
        };
        if !matches!(class.as_str(), "resistor" | "capacitor" | "inductor") {
            continue;
        }
        let Some(value) = inst.attributes.get("value").and_then(|s| parse_value_string(s)) else {
            continue;
        };
        let stress = Stress {
            voltage: inst_v.get(&name).copied().filter(|v| *v > 1e-9),
            current: instance_currents
                .get(&name)
                .copied()
                .map(f64::abs)
                .filter(|c| *c > 1e-12),
            power: instance_power
                .get(&name)
                .copied()
                .map(f64::abs)
                .filter(|p| *p > 1e-15),
        };
        if let Some(fam) = select_family(families, &class, value, &stress) {
            let snapped = snap_to_family(fam, value);
            plan.push((
                id,
                fam.package.clone(),
                fam.voltage_rating,
                fam.current_rating,
                fam.power_w,
                format_value(snapped, &class),
            ));
        }
    }
    let n = plan.len();
    for (id, pkg, vr, ir, pw, value_str) in plan {
        if let Some(inst) = netlist.instances.get_mut(id) {
            if let Some(p) = pkg {
                // `package` is the convention the GLACIER selector + other
                // consumers use; `physical_package` (bhdl_common::sku::PACKAGE)
                // is the key the BOM walker reads for its Package column.
                // Write both so the selected package actually surfaces in
                // the BOM.
                inst.attributes.insert("package".to_string(), p.clone());
                inst.attributes.insert("physical_package".to_string(), p);
            }
            if let Some(v) = vr {
                inst.attributes.insert("voltage_rating".to_string(), format!("{v}"));
            }
            if let Some(i) = ir {
                inst.attributes.insert("current_rating".to_string(), format!("{i}"));
            }
            if let Some(w) = pw {
                inst.attributes.insert("power_rating".to_string(), format!("{w}"));
            }
            inst.attributes.insert("value".to_string(), value_str);
        }
    }
    n
}

fn classify_component(
    netlist: &Netlist,
    def_id: bhdl_netlist::ModuleId,
    attrs: &HashMap<String, String>,
) -> Option<String> {
    // Check explicit component_class attribute first
    if let Some(class) = attrs.get("component_class") {
        let lower = class.to_lowercase();
        if lower.contains("resistor") || lower == "res" || lower == "r" {
            return Some("resistor".to_string());
        }
        if lower.contains("capacitor") || lower == "cap" || lower == "c" {
            return Some("capacitor".to_string());
        }
        if lower.contains("inductor") || lower == "ind" || lower == "l" {
            return Some("inductor".to_string());
        }
    }

    // Fall back to the module definition name
    if let Some(def) = netlist.modules.get(def_id) {
        let name_lower = def.name.to_lowercase();
        if name_lower == "res" || name_lower == "resistor" || name_lower.starts_with("res_") || name_lower == "r" {
            return Some("resistor".to_string());
        }
        if name_lower == "cap" || name_lower == "capacitor" || name_lower.starts_with("cap_") || name_lower == "c" {
            return Some("capacitor".to_string());
        }
        if name_lower == "ind" || name_lower == "inductor" || name_lower.starts_with("ind_") || name_lower == "l" {
            return Some("inductor".to_string());
        }
    }

    // Check if instance name starts with a standard reference designator prefix
    // (This is a fallback; component_class or module name should be authoritative)
    None
}

/// For each instance, compute the maximum voltage seen across its connected nets.
/// This is used for capacitor voltage rating selection where V=P/I is 0/0 for DC caps.
fn compute_instance_max_voltages(
    netlist: &Netlist,
    net_voltages: &HashMap<String, f64>,
) -> HashMap<String, f64> {
    // Build instance → set of connected net names
    let mut instance_nets: HashMap<String, Vec<String>> = HashMap::new();

    for (_net_id, net) in &netlist.nets {
        let net_name = match &net.name {
            Some(n) => n.clone(),
            None => continue,
        };
        for conn in &net.connections {
            let inst_id = match conn {
                bhdl_netlist::ConnectionPoint::InstancePort(iid, _)
                | bhdl_netlist::ConnectionPoint::InstancePin(iid, _) => Some(*iid),
                bhdl_netlist::ConnectionPoint::PinInstance(pi_id) => {
                    netlist.pin_instances.get(*pi_id).map(|pi| pi.instance)
                }
                _ => None,
            };
            if let Some(iid) = inst_id {
                if let Some(inst) = netlist.instances.get(iid) {
                    instance_nets
                        .entry(inst.name.clone())
                        .or_default()
                        .push(net_name.clone());
                }
            }
        }
    }

    // For each instance, find the max absolute voltage difference across its nets
    let mut max_voltages: HashMap<String, f64> = HashMap::new();
    for (inst_name, nets) in &instance_nets {
        let voltages: Vec<f64> = nets
            .iter()
            .filter_map(|n| net_voltages.get(n).copied())
            .collect();

        if voltages.len() >= 2 {
            // Max voltage across the component = max - min of connected net voltages
            let vmax = voltages.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let vmin = voltages.iter().cloned().fold(f64::INFINITY, f64::min);
            max_voltages.insert(inst_name.clone(), (vmax - vmin).abs());
        } else if voltages.len() == 1 {
            // Single net connected — voltage referenced to ground
            max_voltages.insert(inst_name.clone(), voltages[0].abs());
        }
    }

    max_voltages
}

/// Select physical parameters for a resistor instance.
fn select_resistor_physical(
    inst_name: &str,
    attrs: &HashMap<String, String>,
    instance_currents: &HashMap<String, f64>,
    instance_power: &HashMap<String, f64>,
    instance_max_voltages: &HashMap<String, f64>,
    calculator: &PassiveComponentCalculator,
    selector: &PackageSelector,
    requirements: &ApplicationRequirements,
) -> Option<PhysicalSelectionResult> {
    // Get resistance value
    let value_str = attrs.get("value")?;
    let resistance = parse_unit_value(value_str)?;

    // Get current from GLACIER results (use absolute value)
    let current = instance_currents
        .get(inst_name)
        .copied()
        .unwrap_or(0.0)
        .abs();

    // Calculate power rating: use GLACIER power if available, else I²R
    let power = instance_power
        .get(inst_name)
        .copied()
        .unwrap_or_else(|| current * current * resistance)
        .abs();

    let power_rating = calculator.calculate_resistor_power_rating(resistance, current);

    // Get voltage across resistor for voltage rating
    let voltage_across = instance_max_voltages
        .get(inst_name)
        .copied()
        .unwrap_or_else(|| current * resistance);
    let voltage_rating = calculator.calculate_resistor_voltage_rating(voltage_across);

    let spec = selector.select_resistor_spec(resistance, power_rating, voltage_rating, requirements);

    debug!(
        "Resistor {}: R={}Ω, I={:.3}mA, P={:.3}mW → {} / {} / {}",
        inst_name,
        resistance,
        current * 1e3,
        power * 1e3,
        spec.package,
        spec.power_rating,
        spec.voltage_rating
    );

    Some(PhysicalSelectionResult {
        instance_name: inst_name.to_string(),
        component_type: "resistor".to_string(),
        package: spec.package.to_string(),
        power_rating: Some(spec.power_rating.to_string()),
        voltage_rating: Some(spec.voltage_rating.to_string()),
        dielectric: None,
    })
}

/// Select physical parameters for a capacitor instance.
///
/// If the instance has a `dielectric_hint` attribute (e.g. from multi-tier
/// ripple bank generation), that dielectric is used instead of the default
/// selection. This ensures bulk caps get X5R, mid-freq caps get X7R, and
/// HF bypass caps get C0G.
fn select_capacitor_physical(
    inst_name: &str,
    attrs: &HashMap<String, String>,
    instance_max_voltages: &HashMap<String, f64>,
    calculator: &PassiveComponentCalculator,
    selector: &PackageSelector,
    requirements: &ApplicationRequirements,
) -> Option<PhysicalSelectionResult> {
    // Get capacitance value
    let value_str = attrs.get("value")?;
    let capacitance = parse_unit_value(value_str)?;

    // For capacitors, use max voltage across connected nets (not P/I which is 0/0 for DC).
    let max_voltage = instance_max_voltages
        .get(inst_name)
        .copied()
        .unwrap_or(0.0)
        .abs();

    let voltage_rating = calculator.calculate_capacitor_voltage_rating(max_voltage);

    let spec = selector.select_capacitor_spec(capacitance, voltage_rating, requirements);

    // If a dielectric_hint is set (from multi-tier ripple bank), override the default
    let dielectric = if let Some(hint) = attrs.get("dielectric_hint") {
        debug!("Capacitor {}: using dielectric_hint={} (from ripple tier)",
            inst_name, hint);
        hint.clone()
    } else {
        spec.dielectric.to_string()
    };

    // Re-select package if dielectric was overridden (different dielectrics
    // have different max capacitance per package)
    let package = if attrs.contains_key("dielectric_hint") {
        // Use the dielectric-specific package selection
        let dt = DielectricType::from_display_str(&dielectric);
        if let Some(dt) = dt {
            selector.select_capacitor_package_for_dielectric(capacitance, voltage_rating, dt, requirements)
                .unwrap_or_else(|| spec.package.to_string())
        } else {
            spec.package.to_string()
        }
    } else {
        spec.package.to_string()
    };

    debug!(
        "Capacitor {}: C={:.3e}F, Vmax={:.2}V → {} / {} / {}{}",
        inst_name,
        capacitance,
        max_voltage,
        package,
        spec.voltage_rating,
        dielectric,
        if attrs.contains_key("ripple_tier") {
            format!(" [tier: {}]", attrs.get("ripple_tier").unwrap())
        } else {
            String::new()
        },
    );

    Some(PhysicalSelectionResult {
        instance_name: inst_name.to_string(),
        component_type: "capacitor".to_string(),
        package,
        power_rating: None,
        voltage_rating: Some(spec.voltage_rating.to_string()),
        dielectric: Some(dielectric),
    })
}

/// Compute total load current per net.
///
/// For each net, sum the absolute currents of all non-source, non-regulator
/// instances connected to it. This represents the total current sunk by
/// loads on that net — exactly what an inductor feeding that net must carry.
fn compute_net_load_currents(
    netlist: &Netlist,
    instance_currents: &HashMap<String, f64>,
) -> HashMap<String, f64> {
    // Build net_name → list of (instance_name, current)
    let mut net_instances: HashMap<String, Vec<(String, f64)>> = HashMap::new();

    for (_net_id, net) in &netlist.nets {
        let net_name = match &net.name {
            Some(n) => n.clone(),
            None => continue,
        };
        for conn in &net.connections {
            let inst_name = match conn {
                bhdl_netlist::ConnectionPoint::PinInstance(pi_id) => {
                    netlist.pin_instances.get(*pi_id)
                        .and_then(|pi| netlist.instances.get(pi.instance))
                        .map(|i| i.name.clone())
                }
                bhdl_netlist::ConnectionPoint::InstancePort(iid, _)
                | bhdl_netlist::ConnectionPoint::InstancePin(iid, _) => {
                    netlist.instances.get(*iid).map(|i| i.name.clone())
                }
                _ => None,
            };
            if let Some(name) = inst_name {
                if let Some(&current) = instance_currents.get(&name) {
                    net_instances.entry(net_name.clone())
                        .or_default()
                        .push((name, current.abs()));
                }
            }
        }
    }

    // For each net, sum all load currents (use max as a conservative estimate
    // since individual branch currents may overlap on a power rail)
    let mut result = HashMap::new();
    for (net_name, instances) in &net_instances {
        // Take the maximum current seen — on a power rail this is typically
        // the power source's current, which equals total load current.
        let max_current = instances.iter()
            .map(|(_, c)| *c)
            .fold(0.0f64, f64::max);
        result.insert(net_name.clone(), max_current);
    }

    result
}

/// Select physical parameters for an inductor instance.
///
/// Uses the GLACIER-derived current to determine:
/// - Saturation current rating (derated by 0.8)
/// - DCR estimate: ~0.01Ω × √(L_µH) for SMD inductors
/// - Power dissipation: I² × DCR
/// - Package size by current rating
///
/// For expanded buck inductors (identified by `vpin_role = "series"`),
/// GLACIER reports 0A because both terminals sit at the same DC voltage.
/// In this case, we find the VOUT-side net and use the total load current
/// on that net — which is exactly what the inductor must carry. The cascade
/// fixup in `build_simulation_annotations()` ensures that power sources on
/// the VOUT net already include all downstream regulator loads.
fn select_inductor_physical(
    inst_name: &str,
    inst_id: bhdl_netlist::InstanceId,
    attrs: &HashMap<String, String>,
    instance_currents: &HashMap<String, f64>,
    net_load_currents: &HashMap<String, f64>,
    netlist: &Netlist,
) -> Option<PhysicalSelectionResult> {
    // Get inductance value
    let value_str = attrs.get("value")?;
    let inductance = parse_unit_value(value_str)?;

    // Get current from GLACIER (absolute value)
    let mut current = instance_currents
        .get(inst_name)
        .copied()
        .unwrap_or(0.0)
        .abs();

    // For expanded buck inductors, the DC inductor current is 0 because both
    // sides sit at the same voltage. Infer the actual current from the
    // VOUT-side net's total load current.
    //
    // The inductor connects SW (pin 1) → VOUT (pin 2). Pin 2's net carries
    // the load current we need.
    if current < 1e-6 {
        // Find pin 2's net (the VOUT side)
        let vout_side_current = find_inductor_vout_net_current(
            netlist, inst_id, net_load_currents,
        );
        if let Some(load_current) = vout_side_current {
            current = load_current;
            debug!("Inductor {} inferred current from VOUT-side net: {:.3}A",
                   inst_name, current);
        }
    }

    // Estimate DCR: ~0.01Ω × √(L in µH)
    // This is a rough heuristic; actual DCR depends on construction.
    let l_uh = inductance * 1e6; // convert H → µH
    let dcr = 0.01 * l_uh.sqrt().max(1.0);

    // Power dissipation = I² × DCR
    let power = current * current * dcr;

    // Current rating: derate by 0.8 (select for 80% of saturation)
    let required_sat_current = if current > 0.0 { current / 0.8 } else { 0.1 };

    // Package selection by current rating
    let package = if required_sat_current <= 0.5 {
        "0805"
    } else if required_sat_current <= 1.5 {
        "1210"
    } else if required_sat_current <= 3.0 {
        "1812"
    } else if required_sat_current <= 5.0 {
        "2220"
    } else {
        "THT"
    };

    // Format saturation current for display
    let sat_current_str = if required_sat_current >= 1.0 {
        format!("{:.1}A", required_sat_current)
    } else {
        format!("{:.0}mA", required_sat_current * 1e3)
    };

    debug!(
        "Inductor {}: L={:.1}µH, I={:.3}A, DCR={:.3}Ω, P={:.1}mW → {} / I_sat={}",
        inst_name, l_uh, current, dcr, power * 1e3, package, sat_current_str
    );

    Some(PhysicalSelectionResult {
        instance_name: inst_name.to_string(),
        component_type: "inductor".to_string(),
        package: package.to_string(),
        power_rating: Some(format!("{:.1}mW", power * 1e3)),
        // Reuse voltage_rating field for current_rating (written as "current_rating" in attrs)
        voltage_rating: Some(sat_current_str),
        // Reuse dielectric field for DCR
        dielectric: Some(format!("{:.3}Ω", dcr)),
    })
}

/// For an inductor instance, find the VOUT-side net (pin "2") and return the
/// total load current on that net.
///
/// In a buck converter topology:
///   SW ──[L pin1]──[L pin2]── VOUT_net ──[loads]── GND
///
/// The inductor must carry the total current consumed by everything on VOUT_net.
/// The `net_load_currents` map has the max current per net (typically the power
/// source's current, which equals total load including cascaded regulators).
fn find_inductor_vout_net_current(
    netlist: &Netlist,
    inst_id: bhdl_netlist::InstanceId,
    net_load_currents: &HashMap<String, f64>,
) -> Option<f64> {
    let instance = netlist.instances.get(inst_id)?;
    let module_def = netlist.modules.get(instance.definition)?;

    // Find the VOUT-side pin of the inductor ("OUT" for expansion inductors, "2" for legacy)
    let pin2_id = module_def.pins.iter()
        .find(|&&pid| netlist.pins.get(pid).map(|p| p.name == "OUT" || p.name == "2").unwrap_or(false))
        .copied()?;

    // Find the pin instance for (this instance, pin 2)
    let pi_entry = netlist.pin_instances.iter()
        .find(|(_, pi)| pi.instance == inst_id && pi.pin_def == pin2_id);

    let (pi_id, _) = pi_entry?;

    // Find which net this pin instance is on (scan connection lists for reliability)
    let net_name = netlist.nets.iter()
        .find(|(_, net)| {
            net.connections.contains(&bhdl_netlist::ConnectionPoint::PinInstance(pi_id))
        })
        .and_then(|(_, net)| net.name.clone())?;

    let load = net_load_currents.get(&net_name).copied();
    if let Some(c) = load {
        debug!("Inductor on net '{}': load current = {:.3}A", net_name, c);
    }
    load
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_netlist::{Netlist, ModuleId, InstanceId};

    fn make_test_netlist() -> (Netlist, InstanceId, InstanceId) {
        let mut netlist = Netlist::default();

        // Create a resistor module definition
        let res_mod_id = netlist.modules.insert(bhdl_netlist::ModuleDefinition {
            name: "Res".to_string(),
            kind: bhdl_netlist::ModuleKind::PhysicalComponent,
            ports: vec![],
            pins: vec![],
            internal_instances: vec![],
            internal_nets: vec![],
            attributes: HashMap::new(),
        });

        // Create a capacitor module definition
        let cap_mod_id = netlist.modules.insert(bhdl_netlist::ModuleDefinition {
            name: "Cap".to_string(),
            kind: bhdl_netlist::ModuleKind::PhysicalComponent,
            ports: vec![],
            pins: vec![],
            internal_instances: vec![],
            internal_nets: vec![],
            attributes: HashMap::new(),
        });

        // Create resistor instance with 10k value
        let mut res_attrs = HashMap::new();
        res_attrs.insert("value".to_string(), "10k".to_string());
        let res_id = netlist.instances.insert(bhdl_netlist::Instance {
            name: "R1".to_string(),
            definition: res_mod_id,
            attributes: res_attrs,
            layout_intents: Vec::new(),
        });

        // Create capacitor instance with 100nF value
        let mut cap_attrs = HashMap::new();
        cap_attrs.insert("value".to_string(), "100nF".to_string());
        let cap_id = netlist.instances.insert(bhdl_netlist::Instance {
            name: "C1".to_string(),
            definition: cap_mod_id,
            attributes: cap_attrs,
            layout_intents: Vec::new(),
        });

        (netlist, res_id, cap_id)
    }

    #[test]
    fn test_resistor_physical_selection() {
        let (mut netlist, res_id, _cap_id) = make_test_netlist();

        let mut instance_currents = HashMap::new();
        instance_currents.insert("R1".to_string(), 0.5e-3); // 0.5mA

        let mut instance_power = HashMap::new();
        instance_power.insert("R1".to_string(), 2.5e-3); // 2.5mW

        let net_voltages = HashMap::new();

        let results = apply_glacier_physical_selection(
            &mut netlist,
            &instance_currents,
            &instance_power,
            &net_voltages,
        );

        // Should have at least the resistor result
        let res_result = results.iter().find(|r| r.instance_name == "R1");
        assert!(res_result.is_some(), "R1 should have physical selection");
        let res_result = res_result.unwrap();
        assert_eq!(res_result.component_type, "resistor");
        assert!(!res_result.package.is_empty());
        assert!(res_result.power_rating.is_some());
        assert!(res_result.voltage_rating.is_some());

        // Verify attributes were written to the instance
        let inst = &netlist.instances[res_id];
        assert!(inst.attributes.contains_key("package"));
        assert!(inst.attributes.contains_key("power_rating"));
        assert!(inst.attributes.contains_key("voltage_rating"));
    }

    #[test]
    fn test_capacitor_physical_selection() {
        let (mut netlist, _res_id, cap_id) = make_test_netlist();

        let instance_currents = HashMap::new();
        let instance_power = HashMap::new();

        // Simulate 5V across the capacitor via net voltages
        let mut net_voltages = HashMap::new();
        net_voltages.insert("VCC".to_string(), 5.0);
        net_voltages.insert("GND".to_string(), 0.0);

        // We need to connect the capacitor to these nets for voltage computation
        // Create nets and connect them to C1
        let port1 = netlist.ports.insert(bhdl_netlist::Port {
            name: "1".to_string(),
            direction: bhdl_netlist::PortDirection::InOut,
            net: None,
            width: None,
            module: netlist.instances[cap_id].definition,
        });
        let port2 = netlist.ports.insert(bhdl_netlist::Port {
            name: "2".to_string(),
            direction: bhdl_netlist::PortDirection::InOut,
            net: None,
            width: None,
            module: netlist.instances[cap_id].definition,
        });

        netlist.nets.insert(bhdl_netlist::Net {
            name: Some("VCC".to_string()),
            connections: vec![bhdl_netlist::ConnectionPoint::InstancePort(cap_id, port1)],
            net_class: bhdl_netlist::NetClass::Signal,
        });
        netlist.nets.insert(bhdl_netlist::Net {
            name: Some("GND".to_string()),
            connections: vec![bhdl_netlist::ConnectionPoint::InstancePort(cap_id, port2)],
            net_class: bhdl_netlist::NetClass::Signal,
        });

        let results = apply_glacier_physical_selection(
            &mut netlist,
            &instance_currents,
            &instance_power,
            &net_voltages,
        );

        let cap_result = results.iter().find(|r| r.instance_name == "C1");
        assert!(cap_result.is_some(), "C1 should have physical selection");
        let cap_result = cap_result.unwrap();
        assert_eq!(cap_result.component_type, "capacitor");
        assert!(cap_result.dielectric.is_some());
        assert!(cap_result.voltage_rating.is_some());

        // Verify attributes were written
        let inst = &netlist.instances[cap_id];
        assert!(inst.attributes.contains_key("package"));
        assert!(inst.attributes.contains_key("voltage_rating"));
        assert!(inst.attributes.contains_key("dielectric"));

        // 5V cap should get at least 10V rating (2x derating)
        let vr = inst.attributes.get("voltage_rating").unwrap();
        assert!(vr == "10V" || vr == "16V" || vr == "25V",
            "Expected voltage rating >= 10V for 5V cap, got {}", vr);
    }

    #[test]
    fn test_no_value_attribute_skips() {
        let mut netlist = Netlist::default();

        let mod_id = netlist.modules.insert(bhdl_netlist::ModuleDefinition {
            name: "Res".to_string(),
            kind: bhdl_netlist::ModuleKind::PhysicalComponent,
            ports: vec![],
            pins: vec![],
            internal_instances: vec![],
            internal_nets: vec![],
            attributes: HashMap::new(),
        });

        // Instance without a "value" attribute
        netlist.instances.insert(bhdl_netlist::Instance {
            name: "R_novalue".to_string(),
            definition: mod_id,
            attributes: HashMap::new(),
            layout_intents: Vec::new(),
        });

        let results = apply_glacier_physical_selection(
            &mut netlist,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );

        assert!(results.is_empty(), "Instance without value should be skipped");
    }

    #[test]
    fn test_high_power_resistor_gets_large_package() {
        let (mut netlist, res_id, _) = make_test_netlist();

        // 10k resistor with 10mA → P = I²R = 1W
        let mut instance_currents = HashMap::new();
        instance_currents.insert("R1".to_string(), 10e-3);

        let results = apply_glacier_physical_selection(
            &mut netlist,
            &instance_currents,
            &HashMap::new(),
            &HashMap::new(),
        );

        let res_result = results.iter().find(|r| r.instance_name == "R1").unwrap();
        // 1W derated by 0.7 → 1.43W → needs P2W (2512 package)
        assert!(
            res_result.package == "2512" || res_result.package == "THT",
            "High power resistor should get large package, got {}",
            res_result.package
        );
    }

    #[test]
    fn test_inductor_physical_selection() {
        let mut netlist = Netlist::default();

        let ind_mod_id = netlist.modules.insert(bhdl_netlist::ModuleDefinition {
            name: "Ind".to_string(),
            kind: bhdl_netlist::ModuleKind::PhysicalComponent,
            ports: vec![],
            pins: vec![],
            internal_instances: vec![],
            internal_nets: vec![],
            attributes: HashMap::new(),
        });

        let mut ind_attrs = HashMap::new();
        ind_attrs.insert("value".to_string(), "33µH".to_string());
        ind_attrs.insert("component_class".to_string(), "inductor".to_string());
        let ind_id = netlist.instances.insert(bhdl_netlist::Instance {
            name: "L1".to_string(),
            definition: ind_mod_id,
            attributes: ind_attrs,
            layout_intents: Vec::new(),
        });

        // 2A through the inductor
        let mut instance_currents = HashMap::new();
        instance_currents.insert("L1".to_string(), 2.0);

        let results = apply_glacier_physical_selection(
            &mut netlist,
            &instance_currents,
            &HashMap::new(),
            &HashMap::new(),
        );

        let ind_result = results.iter().find(|r| r.instance_name == "L1");
        assert!(ind_result.is_some(), "L1 should have physical selection");
        let ind_result = ind_result.unwrap();
        assert_eq!(ind_result.component_type, "inductor");

        // 2A / 0.8 = 2.5A required sat current → 1812 package (≤3A)
        assert_eq!(ind_result.package, "1812",
            "2A inductor should get 1812 package, got {}", ind_result.package);

        // Check attributes written to instance
        let inst = &netlist.instances[ind_id];
        assert!(inst.attributes.contains_key("package"));
        assert!(inst.attributes.contains_key("current_rating"));
        assert!(inst.attributes.contains_key("dcr"));
    }

    #[test]
    fn test_high_current_inductor_gets_large_package() {
        let mut netlist = Netlist::default();

        let ind_mod_id = netlist.modules.insert(bhdl_netlist::ModuleDefinition {
            name: "Ind".to_string(),
            kind: bhdl_netlist::ModuleKind::PhysicalComponent,
            ports: vec![],
            pins: vec![],
            internal_instances: vec![],
            internal_nets: vec![],
            attributes: HashMap::new(),
        });

        let mut ind_attrs = HashMap::new();
        ind_attrs.insert("value".to_string(), "10µH".to_string());
        ind_attrs.insert("component_class".to_string(), "inductor".to_string());
        netlist.instances.insert(bhdl_netlist::Instance {
            name: "L_big".to_string(),
            definition: ind_mod_id,
            attributes: ind_attrs,
            layout_intents: Vec::new(),
        });

        // 8A — needs THT
        let mut instance_currents = HashMap::new();
        instance_currents.insert("L_big".to_string(), 8.0);

        let results = apply_glacier_physical_selection(
            &mut netlist,
            &instance_currents,
            &HashMap::new(),
            &HashMap::new(),
        );

        let ind_result = results.iter().find(|r| r.instance_name == "L_big").unwrap();
        assert_eq!(ind_result.package, "THT",
            "8A inductor should get THT package, got {}", ind_result.package);
    }

    // ── Capacitor bank splitting tests ──────────────────────────────────

    /// Create a capacitor-only test netlist with the given capacitance value string.
    /// Returns (netlist, cap_id, net1_id, net2_id).
    fn make_cap_netlist(cap_value: &str) -> (Netlist, bhdl_netlist::InstanceId, bhdl_netlist::NetId, bhdl_netlist::NetId) {
        let mut netlist = Netlist::default();

        // Create Cap module with two passive pins using the proper API
        let cap_mod_id = netlist.add_module("Cap".to_string(), bhdl_netlist::ModuleKind::PhysicalComponent);
        netlist.add_pin(cap_mod_id, "1".to_string(), bhdl_netlist::PinDirection::InOut, bhdl_netlist::PinType::Passive);
        netlist.add_pin(cap_mod_id, "2".to_string(), bhdl_netlist::PinDirection::InOut, bhdl_netlist::PinType::Passive);

        let mut cap_attrs = HashMap::new();
        cap_attrs.insert("value".to_string(), cap_value.to_string());
        cap_attrs.insert("component_class".to_string(), "capacitor".to_string());
        let cap_id = netlist.instances.insert(bhdl_netlist::Instance {
            name: "C1".to_string(),
            definition: cap_mod_id,
            attributes: cap_attrs,
            layout_intents: Vec::new(),
        });

        // Create pin instances
        let pin_insts = netlist.create_pin_instances(cap_id).unwrap();

        // Create two nets and connect the cap
        let net1 = netlist.add_net(Some("VOUT".to_string()));
        let net2 = netlist.add_net(Some("GND".to_string()));
        netlist.connect(net1, bhdl_netlist::ConnectionPoint::PinInstance(pin_insts[0])).unwrap();
        netlist.connect(net2, bhdl_netlist::ConnectionPoint::PinInstance(pin_insts[1])).unwrap();

        (netlist, cap_id, net1, net2)
    }

    #[test]
    fn test_capacitor_bank_split_needed() {
        // 470µF X5R/1210 should split: max per unit = 47µF → 10× 47µF
        let (mut netlist, cap_id, _, _) = make_cap_netlist("470µF");

        // 5V across the cap
        let mut net_voltages = HashMap::new();
        net_voltages.insert("VOUT".to_string(), 5.0);
        net_voltages.insert("GND".to_string(), 0.0);

        let initial_instances = netlist.instances.len();

        let results = apply_glacier_physical_selection(
            &mut netlist,
            &HashMap::new(),
            &HashMap::new(),
            &net_voltages,
        );

        // Should have a result for C1
        let cap_result = results.iter().find(|r| r.instance_name == "C1");
        assert!(cap_result.is_some(), "C1 should have physical selection");

        // Original instance should have bank_count attribute
        let orig = &netlist.instances[cap_id];
        assert!(orig.attributes.contains_key("bank_count"),
            "Original cap should have bank_count attr");
        let count: usize = orig.attributes.get("bank_count").unwrap().parse().unwrap();
        assert!(count > 1, "bank_count should be > 1 for 470µF, got {}", count);

        // Should have created (count - 1) additional instances
        let total = netlist.instances.len();
        assert_eq!(total, initial_instances + count - 1,
            "Expected {} total instances ({} original + {} new), got {}",
            initial_instances + count - 1, initial_instances, count - 1, total);

        // Original should have updated value (per-unit, not total)
        let per_unit_value = orig.attributes.get("value").unwrap();
        assert_ne!(per_unit_value, "470µF",
            "Original value should be updated to per-unit value, got {}", per_unit_value);

        // bank_total should record the original total
        assert!(orig.attributes.contains_key("bank_total"),
            "Original cap should have bank_total attr");
    }

    #[test]
    fn test_capacitor_bank_no_split() {
        // 100nF X7R should NOT split (well under max for any package)
        let (mut netlist, cap_id, _, _) = make_cap_netlist("100nF");

        let mut net_voltages = HashMap::new();
        net_voltages.insert("VOUT".to_string(), 3.3);
        net_voltages.insert("GND".to_string(), 0.0);

        let initial_instances = netlist.instances.len();

        let results = apply_glacier_physical_selection(
            &mut netlist,
            &HashMap::new(),
            &HashMap::new(),
            &net_voltages,
        );

        let cap_result = results.iter().find(|r| r.instance_name == "C1");
        assert!(cap_result.is_some(), "C1 should have physical selection");

        // No additional instances should be created
        assert_eq!(netlist.instances.len(), initial_instances,
            "100nF cap should not be split");

        // Should NOT have bank_count attribute
        let inst = &netlist.instances[cap_id];
        assert!(!inst.attributes.contains_key("bank_count"),
            "100nF cap should not have bank_count");
    }

    #[test]
    fn test_capacitor_bank_moderate() {
        // 100µF X5R/1206 should split: max per unit = 22µF → ceil(100/22) = 5× 20µF
        let (mut netlist, cap_id, _, _) = make_cap_netlist("100µF");

        let mut net_voltages = HashMap::new();
        net_voltages.insert("VOUT".to_string(), 3.3);
        net_voltages.insert("GND".to_string(), 0.0);

        let results = apply_glacier_physical_selection(
            &mut netlist,
            &HashMap::new(),
            &HashMap::new(),
            &net_voltages,
        );

        let cap_result = results.iter().find(|r| r.instance_name == "C1");
        assert!(cap_result.is_some(), "C1 should have physical selection");

        let orig = &netlist.instances[cap_id];
        assert!(orig.attributes.contains_key("bank_count"),
            "100µF cap should be split into a bank");
        let count: usize = orig.attributes.get("bank_count").unwrap().parse().unwrap();
        assert!(count >= 2 && count <= 10,
            "100µF should split into 2-10 units, got {}", count);
    }

    #[test]
    fn test_bank_instances_connected() {
        // Verify that new bank instances are connected to the same nets
        let (mut netlist, _cap_id, net1, net2) = make_cap_netlist("470µF");

        let mut net_voltages = HashMap::new();
        net_voltages.insert("VOUT".to_string(), 5.0);
        net_voltages.insert("GND".to_string(), 0.0);

        let _results = apply_glacier_physical_selection(
            &mut netlist,
            &HashMap::new(),
            &HashMap::new(),
            &net_voltages,
        );

        // Find all bank child instances (name starts with "C1_")
        let bank_children: Vec<_> = netlist.instances.iter()
            .filter(|(_, i)| i.name.starts_with("C1_"))
            .collect();

        assert!(!bank_children.is_empty(), "Should have bank child instances");

        // Each child should be connected to both nets
        for (child_id, child) in &bank_children {
            let (child_net1, child_net2) = find_instance_nets(&netlist, *child_id);
            assert!(child_net1.is_some() && child_net2.is_some(),
                "Bank child {} should be connected to two nets", child.name);
            assert_eq!(child_net1.unwrap(), net1,
                "Bank child {} pin 1 should be on VOUT net", child.name);
            assert_eq!(child_net2.unwrap(), net2,
                "Bank child {} pin 2 should be on GND net", child.name);
        }
    }

    #[test]
    fn test_format_cap_value() {
        assert_eq!(format_cap_value(470e-6), "470µF");
        assert_eq!(format_cap_value(47e-6), "47µF");
        assert_eq!(format_cap_value(100e-9), "100nF");
        assert_eq!(format_cap_value(10e-12), "10pF");
        assert_eq!(format_cap_value(2.2e-6), "2.2µF");
        assert_eq!(format_cap_value(1e-3), "1mF");
    }

    #[test]
    fn test_dielectric_hint_respected() {
        // A capacitor with dielectric_hint="C0G" should get C0G, not the default
        let (mut netlist, cap_id, _, _) = make_cap_netlist("100nF");

        // Set dielectric_hint as if placed by multi-tier ripple bank
        netlist.instances[cap_id].attributes.insert(
            "dielectric_hint".to_string(), "C0G".to_string(),
        );
        netlist.instances[cap_id].attributes.insert(
            "ripple_tier".to_string(), "hf_bypass".to_string(),
        );

        let mut net_voltages = HashMap::new();
        net_voltages.insert("VOUT".to_string(), 5.0);
        net_voltages.insert("GND".to_string(), 0.0);

        let results = apply_glacier_physical_selection(
            &mut netlist,
            &HashMap::new(),
            &HashMap::new(),
            &net_voltages,
        );

        let cap_result = results.iter().find(|r| r.instance_name == "C1");
        assert!(cap_result.is_some(), "C1 should have physical selection");
        let cap_result = cap_result.unwrap();

        // Dielectric should be C0G (from hint), not the default X7R
        assert_eq!(cap_result.dielectric.as_deref(), Some("C0G"),
            "dielectric_hint=C0G should be respected, got {:?}", cap_result.dielectric);

        // Verify the attribute was written to the instance
        let inst = &netlist.instances[cap_id];
        assert_eq!(inst.attributes.get("dielectric").map(|s| s.as_str()), Some("C0G"));
    }
}
