//! Electrical rule checks (ERC) — the first REAL design-rule content.
//!
//! Five schematic-review classics, each a `DesignRule` check function over
//! the netlist's pin-direction/net model (directions flow from the entity
//! declarations: `pin TX: signal out;`):
//!
//!   ERC001  driver conflicts — two push-pull outputs on one net
//!           (contention), or a net of pure inputs with nothing driving it.
//!   ERC002  differential-pair polarity — a `_P` and a `_N` pin sharing a
//!           net means the pair is crossed somewhere.
//!   ERC003  TX/RX straight-through — TX wired to TX (or RX to RX) between
//!           two devices; UARTs must cross.
//!   ERC004  cross-voltage-domain net — a signal net joining components
//!           powered from different rails with no level shifter declared.
//!   ERC005  I2C pull-ups — SDA/SCL are open-drain; each needs a pull-up
//!           resistor to a rail (and to the RIGHT rail, see ERC004).
//!
//! Philosophy per Real-Data Policy: a check that cannot resolve its inputs
//! (no pin directions, no rail voltage) SKIPS silently rather than guessing —
//! absence of a violation is never manufactured from absent data.

use std::collections::HashMap;

use bhdl_analyzer::AnalysisResult;
use bhdl_netlist::netlist::Netlist;
use bhdl_netlist::types::{ConnectionPoint, NetClass, NetId, PinDirection, PinType};

use crate::design_rule_checker::{DRCViolation, RuleCategory, ViolationLocation, ViolationSeverity};

/// One resolved net member: which instance, which pin, what electrical role.
#[derive(Debug, Clone)]
struct NetPin {
    inst: String,
    pin: String,
    dir: PinDirection,
    ptype: PinType,
    /// component_class attribute of the instance (level shifters, resistors).
    class: String,
}

/// Resolve every net's members to (instance, pin, direction, type, class).
fn net_members(netlist: &Netlist) -> HashMap<NetId, Vec<NetPin>> {
    let mut out: HashMap<NetId, Vec<NetPin>> = HashMap::new();
    for (net_id, net) in &netlist.nets {
        let mut v = Vec::new();
        for cp in &net.connections {
            let ConnectionPoint::PinInstance(pi_id) = cp else { continue };
            let Some(pi) = netlist.pin_instances.get(*pi_id) else { continue };
            // Trust the pin's own back-pointer: connectivity leaves stale
            // duplicate nets whose connection lists still reference pins that
            // have since been merged onto another net — counting those
            // fabricates single-member "typo" nets that don't exist.
            if pi.net != Some(net_id) {
                continue;
            }
            let Some(pin) = netlist.pins.get(pi.pin_def) else { continue };
            let Some(inst) = netlist.instances.get(pi.instance) else { continue };
            v.push(NetPin {
                inst: inst.name.clone(),
                pin: pin.name.clone(),
                dir: pin.direction.clone(),
                ptype: pin.pin_type.clone(),
                class: inst
                    .attributes
                    .get("component_class")
                    .cloned()
                    .unwrap_or_default(),
            });
        }
        out.insert(net_id, v);
    }
    // Truth dump for rule debugging: BHDL_ERC_DEBUG=1 prints every net with
    // its by-backpointer members AND its raw connection-list size.
    if std::env::var("BHDL_ERC_DEBUG").is_ok() {
        for (net_id, net) in &netlist.nets {
            let members: Vec<String> = out
                .get(&net_id)
                .map(|v| v.iter().map(|p| format!("{}.{}", p.inst, p.pin)).collect())
                .unwrap_or_default();
            eprintln!(
                "[ERC-DEBUG] net {:?} name={:?} class={:?} conns={} members_by_backptr=[{}]",
                net_id,
                net.name,
                std::mem::discriminant(&net.net_class),
                net.connections.len(),
                members.join(", ")
            );
        }
    }
    out
}

fn net_name(netlist: &Netlist, id: NetId) -> String {
    netlist
        .nets
        .get(id)
        .and_then(|n| n.name.clone())
        .unwrap_or_else(|| format!("<unnamed net {id:?}>"))
}

fn is_signal_net(netlist: &Netlist, id: NetId) -> bool {
    matches!(
        netlist.nets.get(id).map(|n| &n.net_class),
        Some(NetClass::Signal)
    )
}

/// The instance's supply voltage: follow its power-input pin(s) to a
/// `NetClass::Power { voltage }` rail. None when the instance has no
/// resolvable supply (passives, connectors) — those carry no domain.
fn instance_supply_voltage(netlist: &Netlist) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    for pi in netlist.pin_instances.values() {
        let Some(pin) = netlist.pins.get(pi.pin_def) else { continue };
        let is_supply_in = matches!(pin.direction, PinDirection::Power)
            || (pin.ptype_is_power() && !matches!(pin.direction, PinDirection::Out));
        if !is_supply_in {
            continue;
        }
        let Some(net_id) = pi.net else { continue };
        let Some(NetClass::Power { voltage, .. }) =
            netlist.nets.get(net_id).map(|n| n.net_class.clone())
        else {
            continue;
        };
        let Some(inst) = netlist.instances.get(pi.instance) else { continue };
        // Highest supply wins when a part touches several rails (VIN vs EN
        // reference etc. — the IO domain is usually the core supply; this is
        // a heuristic, and ERC004 reports the voltages it used).
        let e = out.entry(inst.name.clone()).or_insert(voltage);
        if voltage > *e {
            *e = voltage;
        }
    }
    out
}

trait PinPowerExt {
    fn ptype_is_power(&self) -> bool;
}
impl PinPowerExt for bhdl_netlist::portpin::Pin {
    fn ptype_is_power(&self) -> bool {
        matches!(self.pin_type, PinType::Power)
    }
}

// ────────────────────────── ERC001: driver conflicts ──────────────────────────

pub fn check_driver_conflicts(
    netlist: &Netlist,
    _analysis: &AnalysisResult,
) -> Vec<DRCViolation> {
    let mut out = Vec::new();
    let members = net_members(netlist);
    for (net_id, pins) in &members {
        if !is_signal_net(netlist, *net_id) {
            continue;
        }
        let drivers: Vec<&NetPin> = pins
            .iter()
            .filter(|p| {
                matches!(p.dir, PinDirection::Out)
                    && !matches!(p.ptype, PinType::Power | PinType::Ground)
            })
            .collect();
        let sinks: Vec<&NetPin> = pins
            .iter()
            .filter(|p| matches!(p.dir, PinDirection::In))
            .collect();
        let flexible = pins.iter().any(|p| {
            matches!(p.dir, PinDirection::InOut | PinDirection::Passive)
                || matches!(p.ptype, PinType::Power | PinType::Ground)
        });

        if drivers.len() >= 2 {
            // Two push-pull outputs fighting. InOut pins are exempt (buses,
            // open-drain); `signal out` is declared push-pull.
            let who: Vec<String> = drivers
                .iter()
                .map(|p| format!("{}.{}", p.inst, p.pin))
                .collect();
            out.push(DRCViolation {
                rule_id: "ERC001".into(),
                rule_name: "Driver conflict".into(),
                category: RuleCategory::Electrical,
                description: format!(
                    "net '{}' has {} push-pull drivers: {} — outputs shorted \
                     together contend (damage / undefined logic)",
                    net_name(netlist, *net_id),
                    drivers.len(),
                    who.join(", ")
                ),
                severity: ViolationSeverity::Error,
                location: ViolationLocation::Net(*net_id),
                fix_suggestion: "one driver per push-pull net; use inout/open-drain pins \
                     for shared buses"
                        .into(),
                standard_reference: None,
            });
        }
        if drivers.is_empty() && !flexible && sinks.len() >= 1 && pins.len() == sinks.len() {
            // Nothing can ever drive this net: every member is a declared
            // input (the input-to-input mistake, or a floating input).
            let who: Vec<String> =
                sinks.iter().map(|p| format!("{}.{}", p.inst, p.pin)).collect();
            out.push(DRCViolation {
                rule_id: "ERC001".into(),
                rule_name: "Undriven net".into(),
                category: RuleCategory::Electrical,
                description: format!(
                    "net '{}' connects only inputs ({}) — nothing drives it \
                     (floating logic level)",
                    net_name(netlist, *net_id),
                    who.join(", ")
                ),
                severity: ViolationSeverity::Warning,
                location: ViolationLocation::Net(*net_id),
                fix_suggestion: "connect a driver, a rail, or a pull resistor".into(),
                standard_reference: None,
            });
        }
    }
    out
}

// ─────────────────── ERC002: differential-pair polarity ───────────────────

/// Classify a pin name's differential polarity: `X_P`/`XP`/`X+`/`DP` → Pos,
/// `X_N`/`XN`/`X-`/`DM`/`DN` → Neg. Returns (base, polarity).
fn diff_polarity(name: &str) -> Option<(String, bool)> {
    let n = name.to_uppercase();
    for (suf, pos) in [
        ("_P", true),
        ("_N", false),
        ("+", true),
        ("-", false),
        ("DP", true),
        ("DM", false),
        ("DN", false),
    ] {
        if let Some(base) = n.strip_suffix(suf) {
            if !base.is_empty() || suf.len() > 1 {
                return Some((base.to_string(), pos));
            }
        }
    }
    None
}

pub fn check_differential_polarity(
    netlist: &Netlist,
    _analysis: &AnalysisResult,
) -> Vec<DRCViolation> {
    let mut out = Vec::new();
    let members = net_members(netlist);
    for (net_id, pins) in &members {
        if !is_signal_net(netlist, *net_id) {
            continue;
        }
        // Differential members: explicit pin_type first, name-suffix second.
        let diffs: Vec<(String, bool, String)> = pins
            .iter()
            .filter_map(|p| match p.ptype {
                PinType::DifferentialPos => {
                    Some(("".into(), true, format!("{}.{}", p.inst, p.pin)))
                }
                PinType::DifferentialNeg => {
                    Some(("".into(), false, format!("{}.{}", p.inst, p.pin)))
                }
                _ => diff_polarity(&p.pin)
                    .map(|(b, pos)| (b, pos, format!("{}.{}", p.inst, p.pin))),
            })
            .collect();
        if diffs.len() < 2 {
            continue;
        }
        let pos: Vec<&(String, bool, String)> = diffs.iter().filter(|d| d.1).collect();
        let neg: Vec<&(String, bool, String)> = diffs.iter().filter(|d| !d.1).collect();
        if !pos.is_empty() && !neg.is_empty() {
            out.push(DRCViolation {
                rule_id: "ERC002".into(),
                rule_name: "Differential polarity crossed".into(),
                category: RuleCategory::Electrical,
                description: format!(
                    "net '{}' carries BOTH polarities of a differential pair: \
                     positive {} vs negative {} — P/N swapped on one side",
                    net_name(netlist, *net_id),
                    pos.iter().map(|d| d.2.as_str()).collect::<Vec<_>>().join(", "),
                    neg.iter().map(|d| d.2.as_str()).collect::<Vec<_>>().join(", "),
                ),
                severity: ViolationSeverity::Error,
                location: ViolationLocation::Net(*net_id),
                fix_suggestion: "swap the P/N connections on one endpoint".into(),
                standard_reference: None,
            });
        }
    }
    out
}

// ───────────────────── ERC003: TX/RX straight-through ─────────────────────

fn uart_role(name: &str) -> Option<bool /* is_tx */> {
    let n = name.to_uppercase();
    let base = n.trim_end_matches('D'); // TXD/RXD
    if base.ends_with("TX") || n == "TXD" || n == "TX" {
        return Some(true);
    }
    if base.ends_with("RX") || n == "RXD" || n == "RX" {
        return Some(false);
    }
    None
}

pub fn check_tx_rx_cross(
    netlist: &Netlist,
    _analysis: &AnalysisResult,
) -> Vec<DRCViolation> {
    let mut out = Vec::new();
    let members = net_members(netlist);
    for (net_id, pins) in &members {
        if !is_signal_net(netlist, *net_id) {
            continue;
        }
        let uart: Vec<(bool, String, String)> = pins
            .iter()
            .filter_map(|p| {
                uart_role(&p.pin).map(|tx| (tx, p.inst.clone(), format!("{}.{}", p.inst, p.pin)))
            })
            .collect();
        // Same-role pins from DIFFERENT instances on one net = straight-through.
        for (role, label) in [(true, "TX"), (false, "RX")] {
            let same: Vec<&(bool, String, String)> = uart
                .iter()
                .filter(|(r, _, _)| *r == role)
                .collect();
            let distinct_instances: std::collections::HashSet<&str> =
                same.iter().map(|(_, i, _)| i.as_str()).collect();
            if distinct_instances.len() >= 2 {
                out.push(DRCViolation {
                    rule_id: "ERC003".into(),
                    rule_name: "UART not crossed".into(),
                category: RuleCategory::Electrical,
                    description: format!(
                        "net '{}' ties {} pins of different devices together \
                         ({}) — UART links must cross (TX→RX)",
                        net_name(netlist, *net_id),
                        label,
                        same.iter().map(|(_, _, p)| p.as_str()).collect::<Vec<_>>().join(", "),
                    ),
                    severity: ViolationSeverity::Error,
                    location: ViolationLocation::Net(*net_id),
                    fix_suggestion: "swap TX/RX on one end of the link".into(),
                    standard_reference: None,
                });
            }
        }
    }
    out
}

// ──────────────── ERC004: cross-voltage-domain signal net ────────────────

pub fn check_voltage_domains(
    netlist: &Netlist,
    _analysis: &AnalysisResult,
) -> Vec<DRCViolation> {
    let mut out = Vec::new();
    let members = net_members(netlist);
    let supply = instance_supply_voltage(netlist);
    for (net_id, pins) in &members {
        if !is_signal_net(netlist, *net_id) {
            continue;
        }
        // A declared level shifter on the net exempts it.
        if pins.iter().any(|p| p.class.contains("level_shifter")) {
            continue;
        }
        // Domains of the ACTIVE members (instances with a resolvable supply);
        // passives bridge domains legitimately (dividers, series R).
        let mut domains: Vec<(f64, String)> = Vec::new();
        for p in pins {
            if matches!(p.dir, PinDirection::Passive) {
                continue;
            }
            if let Some(v) = supply.get(&p.inst) {
                domains.push((*v, format!("{}.{} @{v:.2}V", p.inst, p.pin)));
            }
        }
        if domains.len() < 2 {
            continue;
        }
        let lo = domains.iter().map(|(v, _)| *v).fold(f64::MAX, f64::min);
        let hi = domains.iter().map(|(v, _)| *v).fold(f64::MIN, f64::max);
        if hi - lo > 0.05 * hi.max(1.0) {
            out.push(DRCViolation {
                rule_id: "ERC004".into(),
                rule_name: "Cross-domain signal without level shifter".into(),
                category: RuleCategory::Electrical,
                description: format!(
                    "net '{}' joins components in different voltage domains \
                     ({}) with no level shifter — a {hi:.2}V output can \
                     overdrive a {lo:.2}V input",
                    net_name(netlist, *net_id),
                    domains.iter().map(|(_, d)| d.as_str()).collect::<Vec<_>>().join(", "),
                ),
                severity: ViolationSeverity::Error,
                location: ViolationLocation::Net(*net_id),
                fix_suggestion: "insert a level shifter (component_class = \"level_shifter\") \
                     or supply both from one rail"
                        .into(),
                standard_reference: None,
            });
        }
    }
    out
}

// ───────────────────────── ERC005: I2C pull-ups ─────────────────────────

pub fn check_i2c_pullups(
    netlist: &Netlist,
    _analysis: &AnalysisResult,
) -> Vec<DRCViolation> {
    let mut out = Vec::new();
    let members = net_members(netlist);
    let supply = instance_supply_voltage(netlist);

    // Resistor instances and, per instance, the set of nets it touches.
    let mut resistor_nets: HashMap<String, Vec<NetId>> = HashMap::new();
    for pi in netlist.pin_instances.values() {
        let Some(inst) = netlist.instances.get(pi.instance) else { continue };
        let is_res = inst
            .attributes
            .get("component_class")
            .map(|c| c == "resistor")
            .unwrap_or(false);
        if !is_res {
            continue;
        }
        if let Some(net) = pi.net {
            resistor_nets.entry(inst.name.clone()).or_default().push(net);
        }
    }

    for (net_id, pins) in &members {
        if !is_signal_net(netlist, *net_id) {
            continue;
        }
        let i2c_pins: Vec<&NetPin> = pins
            .iter()
            .filter(|p| {
                let n = p.pin.to_uppercase();
                n == "SDA" || n == "SCL" || n.ends_with("_SDA") || n.ends_with("_SCL")
            })
            .collect();
        if i2c_pins.is_empty() {
            continue;
        }
        // Pull-up present? A resistor with one leg on this net and the other
        // on a Power-class net.
        let mut pullup_rail: Option<f64> = None;
        let mut has_pullup = false;
        for nets in resistor_nets.values() {
            if !nets.contains(net_id) {
                continue;
            }
            for other in nets {
                if other == net_id {
                    continue;
                }
                if let Some(NetClass::Power { voltage, .. }) =
                    netlist.nets.get(*other).map(|n| n.net_class.clone())
                {
                    has_pullup = true;
                    pullup_rail = Some(voltage);
                }
            }
        }
        let bus = net_name(netlist, *net_id);
        let who: Vec<String> =
            i2c_pins.iter().map(|p| format!("{}.{}", p.inst, p.pin)).collect();
        if !has_pullup {
            out.push(DRCViolation {
                rule_id: "ERC005".into(),
                rule_name: "I2C pull-up missing".into(),
                category: RuleCategory::Electrical,
                description: format!(
                    "I2C net '{bus}' ({}) has no pull-up resistor to a rail — \
                     the open-drain bus can never go high",
                    who.join(", ")
                ),
                severity: ViolationSeverity::Error,
                location: ViolationLocation::Net(*net_id),
                fix_suggestion: "add a pull-up (2.2k–10k typical) from the net to the bus \
                     supply rail"
                        .into(),
                standard_reference: None,
            });
        } else if let Some(rail_v) = pullup_rail {
            // Wrong-rail pull-up: compare against the I2C devices' domain.
            let dev_v: Vec<f64> = i2c_pins
                .iter()
                .filter_map(|p| supply.get(&p.inst).copied())
                .collect();
            if let Some(min_dev) = dev_v.iter().cloned().reduce(f64::min) {
                if rail_v > min_dev * 1.05 {
                    out.push(DRCViolation {
                        rule_id: "ERC005".into(),
                        rule_name: "I2C pull-up to wrong rail".into(),
                category: RuleCategory::Electrical,
                        description: format!(
                            "I2C net '{bus}' pulls up to {rail_v:.2}V but a bus \
                             device is powered at {min_dev:.2}V — the high level \
                             overdrives its pad",
                        ),
                        severity: ViolationSeverity::Warning,
                        location: ViolationLocation::Net(*net_id),
                        fix_suggestion: "pull up to the lowest bus-device rail".into(),
                        standard_reference: None,
                    });
                }
            }
        }
    }
    out
}

// ═══════════════════════ Batch 2: connectivity + datasheet + budgets ═══════════════════════

use crate::supply_synthesis::parse_si_txt;

/// Instance attribute with entity-index fallback: same-file entities never
/// get their attributes stamped onto instances (only imported/known types
/// do), but the analyzer's `entity_attribute_index` carries them — look up
/// by the instance's module (entity) name when the instance itself is bare.
fn attr_of(
    netlist: &Netlist,
    analysis: &AnalysisResult,
    inst: &bhdl_netlist::instance::Instance,
    key: &str,
) -> Option<String> {
    if let Some(v) = inst.attributes.get(key) {
        return Some(v.clone());
    }
    let module = netlist.modules.get(inst.definition)?;
    analysis
        .entity_attribute_index
        .get(&module.name)?
        .get(key)
        .cloned()
}

/// Per-instance: the Power-class rails its power-input pins sit on.
fn instance_rails(netlist: &Netlist) -> HashMap<String, Vec<(NetId, f64)>> {
    let mut out: HashMap<String, Vec<(NetId, f64)>> = HashMap::new();
    for pi in netlist.pin_instances.values() {
        let Some(pin) = netlist.pins.get(pi.pin_def) else { continue };
        if !matches!(pin.direction, PinDirection::Power) {
            continue;
        }
        let Some(net_id) = pi.net else { continue };
        let Some(NetClass::Power { voltage, .. }) =
            netlist.nets.get(net_id).map(|n| n.net_class.clone())
        else {
            continue;
        };
        let Some(inst) = netlist.instances.get(pi.instance) else { continue };
        out.entry(inst.name.clone()).or_default().push((net_id, voltage));
    }
    out
}

/// Skip the abstract module-definition phantoms (instance named after its
/// entity) — same guard as sign-off.
fn is_phantom(netlist: &Netlist, inst: &bhdl_netlist::instance::Instance) -> bool {
    netlist
        .modules
        .get(inst.definition)
        .map(|m| m.name == inst.name)
        .unwrap_or(false)
}

/// ERC006 floating inputs + ERC007 unpowered parts + ERC011 orphan passives:
/// declared pins of each placed instance that never landed on a net.
pub fn check_unconnected_pins_real(
    netlist: &Netlist,
    _analysis: &AnalysisResult,
) -> Vec<DRCViolation> {
    let mut out = Vec::new();
    // instance → set of connected pin_def ids
    let mut connected: HashMap<bhdl_netlist::types::InstanceId, Vec<bhdl_netlist::types::PinId>> =
        HashMap::new();
    for pi in netlist.pin_instances.values() {
        if pi.net.is_some() {
            connected.entry(pi.instance).or_default().push(pi.pin_def);
        }
    }
    for (inst_id, inst) in &netlist.instances {
        if is_phantom(netlist, inst) {
            continue;
        }
        let conn = connected.get(&inst_id).cloned().unwrap_or_default();
        if conn.is_empty() {
            // Entirely unconnected instances are usually mid-elaboration
            // artifacts; the per-pin story below would spam. One finding.
            continue;
        }
        let is_passive_inst = inst
            .attributes
            .get("component_class")
            .map(|c| matches!(c.as_str(), "resistor" | "capacitor" | "inductor"))
            .unwrap_or(false);
        for (pin_id, pin) in &netlist.pins {
            if pin.module != inst.definition || pin.is_virtual {
                continue;
            }
            if conn.contains(&pin_id) {
                continue;
            }
            match (&pin.direction, is_passive_inst) {
                (PinDirection::Power, _) => out.push(DRCViolation {
                    rule_id: "ERC007".into(),
                    rule_name: "Unpowered part".into(),
                    category: RuleCategory::Electrical,
                    severity: ViolationSeverity::Error,
                    description: format!(
                        "{}.{} (power input) is unconnected — the part is dead \
                         and every function on it is moot",
                        inst.name, pin.name
                    ),
                    location: ViolationLocation::Component(inst_id),
                    fix_suggestion: "wire the supply pin to its rail".into(),
                    standard_reference: None,
                }),
                (PinDirection::In, false) => out.push(DRCViolation {
                    rule_id: "ERC006".into(),
                    rule_name: "Floating input".into(),
                    category: RuleCategory::Electrical,
                    severity: ViolationSeverity::Warning,
                    description: format!(
                        "{}.{} (input) is unconnected — a floating CMOS input \
                         oscillates and draws shoot-through current",
                        inst.name, pin.name
                    ),
                    location: ViolationLocation::Component(inst_id),
                    fix_suggestion: "drive it, or tie to a rail / pull resistor".into(),
                    standard_reference: None,
                }),
                (_, true) => out.push(DRCViolation {
                    rule_id: "ERC011".into(),
                    rule_name: "Orphan passive".into(),
                    category: RuleCategory::Electrical,
                    severity: ViolationSeverity::Warning,
                    description: format!(
                        "passive {} has unconnected pin {} — a component with \
                         a dangling leg does nothing",
                        inst.name, pin.name
                    ),
                    location: ViolationLocation::Component(inst_id),
                    fix_suggestion: "complete the connection or remove the part".into(),
                    standard_reference: None,
                }),
                _ => {}
            }
        }
    }
    out
}

/// ERC008: single-pin nets — almost always a typo'd net name.
pub fn check_single_pin_nets(
    netlist: &Netlist,
    _analysis: &AnalysisResult,
) -> Vec<DRCViolation> {
    let mut out = Vec::new();
    // The connectivity pass sometimes leaves an electrically-merged node
    // split across two Net objects (an `@name` alias net plus the auto pin
    // net), with the pin listed in both connection lists. A pin that appears
    // in MORE than one net's connections is merged elsewhere — reporting it
    // as a lonely net would be a false positive on a synthesizer artifact
    // (known quirk, tracked in docs/spec/ERC.md batch 3).
    let mut listed_in: HashMap<bhdl_netlist::types::PinInstanceId, usize> = HashMap::new();
    for net in netlist.nets.values() {
        for cp in &net.connections {
            if let ConnectionPoint::PinInstance(pi) = cp {
                *listed_in.entry(*pi).or_insert(0) += 1;
            }
        }
    }
    for (net_id, pins) in net_members(netlist) {
        if !is_signal_net(netlist, net_id) || pins.len() != 1 {
            continue;
        }
        // Resolve the lone member's PinInstanceId to apply the merged-elsewhere
        // exemption.
        let lone_multi = netlist
            .nets
            .get(net_id)
            .map(|n| {
                n.connections.iter().any(|cp| match cp {
                    ConnectionPoint::PinInstance(pi) => {
                        listed_in.get(pi).copied().unwrap_or(0) > 1
                    }
                    _ => false,
                })
            })
            .unwrap_or(false);
        if lone_multi {
            continue;
        }
        let p = &pins[0];
        out.push(DRCViolation {
            rule_id: "ERC008".into(),
            rule_name: "Single-pin net".into(),
            category: RuleCategory::Electrical,
            severity: ViolationSeverity::Warning,
            description: format!(
                "net '{}' has exactly one member ({}.{}) — connects to nothing \
                 (typo'd net name?)",
                net_name(netlist, net_id),
                p.inst,
                p.pin
            ),
            location: ViolationLocation::Net(net_id),
            fix_suggestion: "check the net name spelling on the other end".into(),
            standard_reference: None,
        });
    }
    out
}

/// ERC009: a Power-class rail carrying a ground-direction pin — a supply
/// shorted to ground through a mis-wired part.
pub fn check_rail_ground_short(
    netlist: &Netlist,
    _analysis: &AnalysisResult,
) -> Vec<DRCViolation> {
    let mut out = Vec::new();
    for (net_id, pins) in net_members(netlist) {
        let Some(NetClass::Power { voltage, .. }) =
            netlist.nets.get(net_id).map(|n| n.net_class.clone())
        else {
            continue;
        };
        let grounds: Vec<&NetPin> = pins
            .iter()
            .filter(|p| matches!(p.dir, PinDirection::Ground))
            .collect();
        if !grounds.is_empty() {
            out.push(DRCViolation {
                rule_id: "ERC009".into(),
                rule_name: "Rail shorted to ground pin".into(),
                category: RuleCategory::Electrical,
                severity: ViolationSeverity::Error,
                description: format!(
                    "{v:.2}V rail '{}' is wired to ground pin(s) {} — the rail \
                     is shorted through the part",
                    net_name(netlist, net_id),
                    grounds
                        .iter()
                        .map(|p| format!("{}.{}", p.inst, p.pin))
                        .collect::<Vec<_>>()
                        .join(", "),
                    v = voltage,
                ),
                location: ViolationLocation::Net(net_id),
                fix_suggestion: "move the ground pin to the GND net".into(),
                standard_reference: None,
            });
        }
    }
    out
}

/// ERC016: rail budget — Σ declared instance draws vs the rail's declared
/// `@ I` budget. Fires only when at least one member DECLARES a draw
/// (`i_supply` / `supply_current`, `i_quiescent` as the regulator fallback);
/// members without a declaration are counted and reported, never guessed.
pub fn check_rail_budget(
    netlist: &Netlist,
    _analysis: &AnalysisResult,
) -> Vec<DRCViolation> {
    let mut out = Vec::new();
    // rail net → (declared draw sum, declared count, undeclared count, names)
    let mut per_rail: HashMap<NetId, (f64, usize, usize, Vec<String>)> = HashMap::new();
    for pi in netlist.pin_instances.values() {
        let Some(pin) = netlist.pins.get(pi.pin_def) else { continue };
        if !matches!(pin.direction, PinDirection::Power) {
            continue;
        }
        let Some(net_id) = pi.net else { continue };
        if !matches!(
            netlist.nets.get(net_id).map(|n| &n.net_class),
            Some(NetClass::Power { .. })
        ) {
            continue;
        }
        let Some(inst) = netlist.instances.get(pi.instance) else { continue };
        if is_phantom(netlist, inst) {
            continue;
        }
        let draw = ["i_supply", "supply_current", "i_quiescent"]
            .iter()
            .find_map(|k| attr_of(netlist, _analysis, inst, k).and_then(|v| parse_si_txt(&v)));
        let e = per_rail.entry(net_id).or_insert((0.0, 0, 0, Vec::new()));
        match draw {
            Some(d) => {
                e.0 += d;
                e.1 += 1;
                e.3.push(format!("{} {:.1}mA", inst.name, d * 1e3));
            }
            None => e.2 += 1,
        }
    }
    for (net_id, (sum, declared, undeclared, names)) in per_rail {
        let Some(NetClass::Power { current: Some(budget), voltage }) =
            netlist.nets.get(net_id).map(|n| n.net_class.clone())
        else {
            continue;
        };
        if declared == 0 || sum <= budget {
            continue;
        }
        out.push(DRCViolation {
            rule_id: "ERC016".into(),
            rule_name: "Rail budget exceeded".into(),
            category: RuleCategory::Electrical,
            severity: ViolationSeverity::Error,
            description: format!(
                "{voltage:.2}V rail '{}' declares {budget_ma:.0}mA but the \
                 declared draws already total {sum_ma:.0}mA ({}){}",
                net_name(netlist, net_id),
                names.join(", "),
                if undeclared > 0 {
                    format!(" — plus {undeclared} member(s) with UNDECLARED draw")
                } else {
                    String::new()
                },
                budget_ma = budget * 1e3,
                sum_ma = sum * 1e3,
            ),
            location: ViolationLocation::Net(net_id),
            fix_suggestion: "raise the rail's `@ I` budget or shed load".into(),
            standard_reference: None,
        });
    }
    out
}

/// ERC017: regulator below dropout — input rail V < output_voltage +
/// dropout_voltage (both datasheet attributes). The LDO cannot regulate.
pub fn check_regulator_dropout(
    netlist: &Netlist,
    _analysis: &AnalysisResult,
) -> Vec<DRCViolation> {
    let mut out = Vec::new();
    let rails = instance_rails(netlist);
    for (inst_id, inst) in &netlist.instances {
        if is_phantom(netlist, inst) {
            continue;
        }
        let class = attr_of(netlist, _analysis, inst, "component_class").unwrap_or_default();
        if !matches!(class.as_str(), "voltage_regulator" | "ldo") {
            continue;
        }
        let get = |k: &str| attr_of(netlist, _analysis, inst, k).and_then(|v| parse_si_txt(&v));
        let (Some(v_out), Some(dropout)) = (get("output_voltage"), get("dropout_voltage"))
        else {
            continue; // no declared data → no verdict (Real-Data Policy)
        };
        let Some(v_in) = rails
            .get(&inst.name)
            .and_then(|r| r.iter().map(|(_, v)| *v).reduce(f64::max))
        else {
            continue;
        };
        if v_in + 1e-9 < v_out + dropout {
            out.push(DRCViolation {
                rule_id: "ERC017".into(),
                rule_name: "Regulator below dropout".into(),
                category: RuleCategory::Electrical,
                severity: ViolationSeverity::Error,
                description: format!(
                    "{}: input rail {v_in:.2}V < output {v_out:.2}V + dropout \
                     {dropout:.2}V = {:.2}V — the regulator cannot regulate",
                    inst.name,
                    v_out + dropout
                ),
                location: ViolationLocation::Component(inst_id),
                fix_suggestion:
                    "raise the input rail, pick a lower-dropout part, or lower V_OUT"
                        .into(),
                standard_reference: None,
            });
        }
    }
    out
}

/// ERC018: absolute-maximum input — supply rail above the part's declared
/// `input_voltage_max`.
pub fn check_abs_max_input(
    netlist: &Netlist,
    _analysis: &AnalysisResult,
) -> Vec<DRCViolation> {
    let mut out = Vec::new();
    let rails = instance_rails(netlist);
    for (inst_id, inst) in &netlist.instances {
        if is_phantom(netlist, inst) {
            continue;
        }
        let Some(v_max) =
            attr_of(netlist, _analysis, inst, "input_voltage_max").and_then(|v| parse_si_txt(&v))
        else {
            continue;
        };
        let Some(v_in) = rails
            .get(&inst.name)
            .and_then(|r| r.iter().map(|(_, v)| *v).reduce(f64::max))
        else {
            continue;
        };
        if v_in > v_max * (1.0 + 1e-9) {
            out.push(DRCViolation {
                rule_id: "ERC018".into(),
                rule_name: "Absolute-maximum input exceeded".into(),
                category: RuleCategory::Electrical,
                severity: ViolationSeverity::Error,
                description: format!(
                    "{}: supply rail {v_in:.2}V exceeds the part's declared \
                     input_voltage_max {v_max:.2}V",
                    inst.name
                ),
                location: ViolationLocation::Component(inst_id),
                fix_suggestion: "supply from a lower rail or pick a wider-Vin part".into(),
                standard_reference: None,
            });
        }
    }
    out
}

/// ERC020: missing decoupling — an active part whose supply rail carries no
/// capacitor AT ALL (Info; the part-specific placement/quantity rules are
/// T2 territory, see docs/spec/ERC.md).
pub fn check_missing_decoupling(
    netlist: &Netlist,
    _analysis: &AnalysisResult,
) -> Vec<DRCViolation> {
    let mut out = Vec::new();
    // rails that carry at least one capacitor
    let mut rail_has_cap: HashMap<NetId, bool> = HashMap::new();
    for pi in netlist.pin_instances.values() {
        let Some(net_id) = pi.net else { continue };
        if !matches!(
            netlist.nets.get(net_id).map(|n| &n.net_class),
            Some(NetClass::Power { .. })
        ) {
            continue;
        }
        let is_cap = netlist
            .instances
            .get(pi.instance)
            .and_then(|i| i.attributes.get("component_class"))
            .map(|c| c == "capacitor")
            .unwrap_or(false);
        *rail_has_cap.entry(net_id).or_insert(false) |= is_cap;
    }
    let rails = instance_rails(netlist);
    for (inst_id, inst) in &netlist.instances {
        if is_phantom(netlist, inst) {
            continue;
        }
        let class = attr_of(netlist, _analysis, inst, "component_class").unwrap_or_default();
        if matches!(class.as_str(), "resistor" | "capacitor" | "inductor" | "") {
            continue; // passives / unknown parts carry no decoupling habit
        }
        for (rail, v) in rails.get(&inst.name).into_iter().flatten() {
            if !rail_has_cap.get(rail).copied().unwrap_or(false) {
                out.push(DRCViolation {
                    rule_id: "ERC020".into(),
                    rule_name: "No decoupling on rail".into(),
                    category: RuleCategory::Electrical,
                    severity: ViolationSeverity::Info,
                    description: format!(
                        "{} is supplied from the {v:.2}V rail '{}' which has no \
                         capacitor at all — datasheets universally require local \
                         decoupling",
                        inst.name,
                        net_name(netlist, *rail),
                    ),
                    location: ViolationLocation::Component(inst_id),
                    fix_suggestion: "add 100nF local + bulk per the part's datasheet".into(),
                    standard_reference: None,
                });
            }
        }
    }
    out
}
