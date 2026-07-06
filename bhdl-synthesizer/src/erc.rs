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
        let entry = out.entry(inst.name.clone()).or_default();
        // One entry per (instance, rail): several power pins of one part on
        // the same rail (an expanded bus bank — VCCO[0..3] — or paired VCC
        // pins) are ONE supply relationship, not N. Per-pin entries made
        // every consumer (ERC017/ERC019/ERC020) emit N duplicate findings.
        if !entry.iter().any(|(n, _)| *n == net_id) {
            entry.push((net_id, voltage));
        }
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
    // The declared draw (`i_supply`) is a PER-INSTANCE figure. An instance
    // with several power pins on the same rail (an expanded bus bank —
    // VCCO[0..3] — or paired VCC pins) must count once per rail, not once
    // per pin.
    let mut seen: std::collections::HashSet<(NetId, bhdl_netlist::types::InstanceId)> =
        std::collections::HashSet::new();
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
        if !seen.insert((net_id, pi.instance)) {
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

// ───────────────────────── ERC025 — part-carried checks (T2) ─────────────────────────

/// Substitute every `NAME(args…)` call of one predicate function in a check
/// condition with `1`/`0`. `eval` answers per call: `Some(bool)` substitutes,
/// `None` makes the whole require skip (Real-Data — an unanswerable
/// reference, e.g. a pin the entity doesn't declare, is never guessed).
/// Non-call mentions of the name (identifiers merely containing it) pass
/// through untouched.
fn substitute_fn(
    condition: &str,
    fname: &str,
    mut eval: impl FnMut(&[&str]) -> Option<bool>,
) -> Option<String> {
    let mut out = String::with_capacity(condition.len());
    let mut rest = condition;
    while let Some(pos) = rest.find(fname) {
        let after = &rest[pos + fname.len()..];
        let after_trim = after.trim_start();
        // Must be a standalone identifier followed by `(`.
        let prev_ok = rest[..pos]
            .chars()
            .last()
            .map(|c| !c.is_alphanumeric() && c != '_' && c != '.')
            .unwrap_or(true);
        let next_ok = after
            .chars()
            .next()
            .map(|c| !c.is_alphanumeric() && c != '_')
            .unwrap_or(true);
        if !prev_ok || !next_ok || !after_trim.starts_with('(') {
            out.push_str(&rest[..pos + fname.len()]);
            rest = after;
            continue;
        }
        let close = after_trim.find(')')?;
        let args: Vec<&str> = after_trim[1..close]
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        let val = eval(&args)?;
        out.push_str(&rest[..pos]);
        out.push_str(if val { "1" } else { "0" });
        rest = &after_trim[close + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// Substitute every `connected(PIN)` term with `1`/`0` from the netlist.
/// "Connected" means the pin landed on a net AND that net has at least one
/// OTHER member (a pin alone on a net is electrically floating; ERC008
/// separately flags the likely-typo'd net name). Unknown pin → `None`.
fn substitute_connected(
    condition: &str,
    inst_pins: &HashMap<String, bool>,
) -> Option<String> {
    substitute_fn(condition, "connected", |args| match args {
        [pin] => inst_pins.get(*pin).copied(),
        _ => None,
    })
}

/// Numeric context for the non-predicate remainder of a check condition:
/// `self.<attr>` resolves through the entity's datasheet attributes and
/// `<child>.value` through the instance's support parts (expansion children
/// and S4-stamped siblings first, then a board-level instance of that bare
/// name). Anything else is unresolvable — the require skips (Real-Data
/// Policy).
struct CheckCtx<'a> {
    attrs: HashMap<String, &'a str>,
    child_values: HashMap<String, f64>,
}

impl crate::design_evaluator::EvalLookup for CheckCtx<'_> {
    fn lookup(&self, name: &str) -> Result<f64, crate::design_evaluator::DesignEvalError> {
        let name = name.trim();
        if let Some((ns, field)) = name.split_once('.') {
            if ns == "self" {
                if let Some(v) = self.attrs.get(field).and_then(|s| parse_si_txt(s)) {
                    return Ok(v);
                }
            } else if field == "value" {
                if let Some(v) = self.child_values.get(ns) {
                    return Ok(*v);
                }
            }
        }
        Err(crate::design_evaluator::DesignEvalError::EvalError(format!(
            "identifier '{name}' is not resolvable in a check block \
             (recognised: connected(PIN), exists(CHILD), same_net(P1, P2), \
             self.<attribute>, <child>.value)"
        )))
    }
}

/// ERC025 — part-carried `check { require … else "…"; }` rules
/// (docs/spec/ERC.md T2). The part's connection requirements are device IP
/// and travel with the entity like its stress model; each failed require is
/// one finding on the instance, with the vendor's message as the fix. A
/// require whose predicate cannot be resolved (unknown pin, unresolvable
/// identifier) is skipped, never guessed.
pub fn check_part_carried(
    netlist: &Netlist,
    analysis: &AnalysisResult,
) -> Vec<DRCViolation> {
    let mut out = Vec::new();
    if analysis.stress_recipes.is_empty() {
        return out;
    }
    let members = net_members(netlist);

    for (inst_id, inst) in &netlist.instances {
        if is_phantom(netlist, inst) {
            continue;
        }
        let Some(module) = netlist.modules.get(inst.definition) else { continue };
        let Some(recipe) = analysis.stress_recipes.get(&module.name) else { continue };
        if recipe.checks.is_empty() {
            continue;
        }

        // This instance's declared pins → connected? / → net?
        let mut inst_pins: HashMap<String, bool> = HashMap::new();
        let mut pin_nets: HashMap<String, Option<NetId>> = HashMap::new();
        for pi in netlist.pin_instances.values() {
            if pi.instance != inst_id {
                continue;
            }
            let Some(pin) = netlist.pins.get(pi.pin_def) else { continue };
            let connected = pi
                .net
                .map(|nid| {
                    members
                        .get(&nid)
                        .map(|m| m.len() >= 2)
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            // A pin can appear once per instantiation path; connected wins.
            let e = inst_pins.entry(pin.name.clone()).or_insert(false);
            *e = *e || connected;
            let n = pin_nets.entry(pin.name.clone()).or_insert(None);
            if n.is_none() {
                *n = pi.net;
            }
        }

        // Support parts reachable by LOCAL name: expansion children and
        // S4-stamped siblings ({inst}_c_boot with expansion_parent) take
        // precedence; a board-level instance of the bare name (the
        // hand-wired `c_boot: Cap(100nF)` idiom) is the fallback.
        let mut children: HashMap<String, f64> = HashMap::new();
        let val_of = |c: &bhdl_netlist::instance::Instance| {
            c.attributes.get("value").and_then(|v| parse_si_txt(v))
        };
        for c in netlist.instances.values() {
            if c.attributes.get("expansion_parent").map(String::as_str)
                != Some(inst.name.as_str())
            {
                continue;
            }
            if let Some(local) = c.name.strip_prefix(&format!("{}_", inst.name)) {
                if let Some(v) = val_of(c) {
                    children.insert(local.to_string(), v);
                }
            }
        }
        for c in netlist.instances.values() {
            if is_phantom(netlist, c) || children.contains_key(&c.name) {
                continue;
            }
            if let Some(v) = val_of(c) {
                children.entry(c.name.clone()).or_insert(v);
            }
        }

        let ctx = CheckCtx {
            attrs: analysis
                .entity_attribute_index
                .get(&module.name)
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_str())).collect())
                .unwrap_or_default(),
            child_values: children.clone(),
        };

        for chk in &recipe.checks {
            let substituted = substitute_connected(&chk.condition, &inst_pins)
                .and_then(|c| {
                    // exists(CHILD) — always answerable from the netlist.
                    substitute_fn(&c, "exists", |args| match args {
                        [name] => Some(children.contains_key(*name)),
                        _ => None,
                    })
                })
                .and_then(|c| {
                    // same_net(P1, P2) — both pins on the SAME net (a pin
                    // strapped to another, e.g. CS tied to GND). Unknown
                    // pin → skip; a floating pin is honestly `false`.
                    substitute_fn(&c, "same_net", |args| match args {
                        [a, b] => {
                            let na = pin_nets.get(*a)?;
                            let nb = pin_nets.get(*b)?;
                            Some(matches!((na, nb), (Some(x), Some(y)) if x == y))
                        }
                        _ => None,
                    })
                });
            let Some(substituted) = substituted else {
                log::debug!(
                    "ERC025: check on {} ({}) skipped — predicate '{}' references \
                     an unknown pin or malformed call",
                    inst.name, module.name, chk.condition
                );
                continue;
            };
            let holds = match crate::design_evaluator::evaluate_text(&substituted, &ctx) {
                Ok(v) => v != 0.0,
                Err(e) => {
                    log::debug!(
                        "ERC025: check on {} ({}) skipped — '{}' unresolvable: {e:?}",
                        inst.name, module.name, chk.condition
                    );
                    continue; // Real-Data Policy: absence of data ≠ pass/fail
                }
            };
            if !holds {
                out.push(DRCViolation {
                    rule_id: "ERC025".into(),
                    rule_name: "Part-carried check".into(),
                    category: RuleCategory::Electrical,
                    severity: ViolationSeverity::Error,
                    description: format!(
                        "{} ({}): require {} failed — {}",
                        inst.name, module.name, chk.condition.trim(), chk.message
                    ),
                    location: ViolationLocation::Component(inst_id),
                    fix_suggestion: chk.message.clone(),
                    standard_reference: None,
                });
            }
        }
    }
    out
}

// ──────────────────── ERC019 — reversed polarized capacitor ────────────────────

/// DC potential of a net: the SOLVED node voltage when the unified DC
/// analysis succeeded (keyed by net name — this also gives signal nets a
/// potential), else the DECLARED class (ground = 0V, power rail = its
/// declared voltage). Nets neither solved nor declared skip (Real-Data
/// Policy) — a potential is never guessed.
fn net_potential(netlist: &Netlist, analysis: &AnalysisResult, id: NetId) -> Option<f64> {
    let net = netlist.nets.get(id)?;
    if let Some(dc) = analysis.simulation_data.dc_analysis.as_ref() {
        if let Some(v) = net.name.as_ref().and_then(|n| dc.node_voltages.get(n)) {
            return Some(*v);
        }
    }
    match net.net_class {
        NetClass::Ground => Some(0.0),
        NetClass::Power { voltage, .. } => Some(voltage),
        _ => None,
    }
}

/// ERC019 — a `polarized = true` part whose `pos` pin sits at a LOWER
/// declared DC potential than its `neg` pin. Reverse-biased electrolytics
/// vent — this is a board-killer, Critical.
pub fn check_polarized_orientation(
    netlist: &Netlist,
    analysis: &AnalysisResult,
) -> Vec<DRCViolation> {
    let mut out = Vec::new();
    for (inst_id, inst) in &netlist.instances {
        if is_phantom(netlist, inst) {
            continue;
        }
        let polarized = attr_of(netlist, analysis, inst, "polarized")
            .map(|v| v == "true")
            .unwrap_or(false);
        if !polarized {
            continue;
        }
        let mut pin_v: HashMap<&str, f64> = HashMap::new();
        for pi in netlist.pin_instances.values() {
            if pi.instance != inst_id {
                continue;
            }
            let Some(pin) = netlist.pins.get(pi.pin_def) else { continue };
            let key = match pin.name.as_str() {
                "pos" | "P" | "+" => "pos",
                "neg" | "N" | "-" => "neg",
                _ => continue,
            };
            if let Some(v) = pi.net.and_then(|nid| net_potential(netlist, analysis, nid)) {
                pin_v.insert(key, v);
            }
        }
        let (Some(&vp), Some(&vn)) = (pin_v.get("pos"), pin_v.get("neg")) else {
            continue; // a floating or signal-net pin: potential unknown → skip
        };
        if vp < vn {
            out.push(DRCViolation {
                rule_id: "ERC019".into(),
                rule_name: "Reversed polarized capacitor".into(),
                category: RuleCategory::Electrical,
                severity: ViolationSeverity::Critical,
                description: format!(
                    "{} is a polarized capacitor mounted REVERSED: pos sits at \
                     {vp:.2}V, neg at {vn:.2}V — reverse bias vents/destroys \
                     electrolytics",
                    inst.name
                ),
                location: ViolationLocation::Component(inst_id),
                fix_suggestion: "swap the pos/neg connections (or the rail polarity)".into(),
                standard_reference: None,
            });
        }
    }
    out
}

// ──────────────────── ERC026 — interface completeness ────────────────────

/// ERC026 — a part using PART of a bus interface it declares pins for:
/// - I2C: SDA and SCL must both connect if either does (half an I2C bus
///   cannot work) — Error.
/// - SPI: a connected data pin (MOSI/MISO) with no clock (SCK/SCLK) — Error;
///   a connected clock with neither data pin — Warning (suspicious, but
///   clock-only streaming parts exist).
/// UART is deliberately NOT checked: TX-only (debug console) and RX-only
/// links are legitimate. Matching is by exact conventional pin name; parts
/// with multiple numbered buses (SDA0/SDA1) are out of v1 scope.
pub fn check_interface_completeness(
    netlist: &Netlist,
    _analysis: &AnalysisResult,
) -> Vec<DRCViolation> {
    let mut out = Vec::new();
    let members = net_members(netlist);
    for (inst_id, inst) in &netlist.instances {
        if is_phantom(netlist, inst) {
            continue;
        }
        // pin name → connected? (present pins only)
        let mut pins: HashMap<String, bool> = HashMap::new();
        for pi in netlist.pin_instances.values() {
            if pi.instance != inst_id {
                continue;
            }
            let Some(pin) = netlist.pins.get(pi.pin_def) else { continue };
            let connected = pi
                .net
                .map(|nid| members.get(&nid).map(|m| m.len() >= 2).unwrap_or(false))
                .unwrap_or(false);
            let e = pins.entry(pin.name.to_uppercase()).or_insert(false);
            *e = *e || connected;
        }
        let has = |n: &str| pins.contains_key(n);
        let conn = |n: &str| pins.get(n).copied().unwrap_or(false);

        // I2C: both or neither.
        if has("SDA") && has("SCL") && (conn("SDA") != conn("SCL")) {
            let (wired, missing) = if conn("SDA") { ("SDA", "SCL") } else { ("SCL", "SDA") };
            out.push(DRCViolation {
                rule_id: "ERC026".into(),
                rule_name: "Incomplete interface".into(),
                category: RuleCategory::Electrical,
                severity: ViolationSeverity::Error,
                description: format!(
                    "{}: I2C is half-wired — {wired} is connected but {missing} \
                     is not; the bus cannot work without both",
                    inst.name
                ),
                location: ViolationLocation::Component(inst_id),
                fix_suggestion: format!("connect {}.{missing} to the bus", inst.name),
                standard_reference: None,
            });
        }

        // SPI: data needs clock; clock without data is suspicious.
        let sck = conn("SCK") || conn("SCLK");
        let has_sck = has("SCK") || has("SCLK");
        let data_conn = conn("MOSI") || conn("MISO");
        let has_data = has("MOSI") || has("MISO");
        if has_sck && has_data {
            if data_conn && !sck {
                out.push(DRCViolation {
                    rule_id: "ERC026".into(),
                    rule_name: "Incomplete interface".into(),
                    category: RuleCategory::Electrical,
                    severity: ViolationSeverity::Error,
                    description: format!(
                        "{}: SPI data ({}) is connected but the clock is not — \
                         nothing can shift without SCK",
                        inst.name,
                        ["MOSI", "MISO"]
                            .iter()
                            .filter(|p| conn(p))
                            .copied()
                            .collect::<Vec<_>>()
                            .join("+"),
                    ),
                    location: ViolationLocation::Component(inst_id),
                    fix_suggestion: format!("connect {}.SCK to the bus clock", inst.name),
                    standard_reference: None,
                });
            } else if sck && !data_conn {
                out.push(DRCViolation {
                    rule_id: "ERC026".into(),
                    rule_name: "Incomplete interface".into(),
                    category: RuleCategory::Electrical,
                    severity: ViolationSeverity::Warning,
                    description: format!(
                        "{}: SPI clock is connected but neither MOSI nor MISO is — \
                         a clock into nothing is usually a wiring slip",
                        inst.name
                    ),
                    location: ViolationLocation::Component(inst_id),
                    fix_suggestion: "connect the data line(s), or remove the clock run".into(),
                    standard_reference: None,
                });
            }
        }
    }
    out
}

// ──────────────────── ERC022 — intent contradiction ────────────────────

/// ERC022 — a filtering intent whose declared cutoff the PLACED values
/// contradict. `for noise_filtering(cutoff: 10kHz)` on an RC whose placed
/// R·C gives 1.59kHz is a stated intent the board does not implement —
/// exactly the class of error a netlist-only tool cannot see.
///
/// v1 topology scope (Real-Data: anything more ambiguous SKIPS):
/// - anchor on the annotated shunt CAPACITOR (one pin on a ground-class
///   net, one on the hot net);
/// - exactly one resistor on the hot net → RC low-pass, f_c = 1/(2πRC);
/// - exactly one inductor and no resistor → LC, f_0 = 1/(2π√(LC));
/// - anything else (no ground side, several/zero series parts, missing
///   values, no parseable cutoff) → skip.
/// More than ONE OCTAVE off the declared cutoff → Error with all numbers.
pub fn check_intent_contradiction(
    netlist: &Netlist,
    analysis: &AnalysisResult,
) -> Vec<DRCViolation> {
    const FILTER_INTENTS: [&str; 4] =
        ["noise_filtering", "anti_alias", "filter", "filtering"];
    let mut out = Vec::new();
    let members = net_members(netlist);

    for (inst_id, inst) in &netlist.instances {
        if is_phantom(netlist, inst) {
            continue;
        }
        let Some(intent) = inst.attributes.get("intent_name") else { continue };
        if !FILTER_INTENTS.contains(&intent.as_str()) {
            continue;
        }
        if attr_of(netlist, analysis, inst, "component_class").as_deref() != Some("capacitor") {
            continue; // anchor once, on the shunt cap of the network
        }
        // Declared cutoff: named param first, positional first-arg fallback.
        let declared = ["intent_cutoff", "intent_param_0"]
            .iter()
            .find_map(|k| inst.attributes.get(*k).and_then(|v| parse_si_txt(v)))
            .filter(|f| *f > 0.0);
        let Some(f_decl) = declared else { continue };
        let Some(c_val) = inst.attributes.get("value").and_then(|v| parse_si_txt(v))
        else { continue };

        // Shunt topology: one pin on ground, the other is the hot net.
        let mut hot: Option<NetId> = None;
        let mut grounded = false;
        for pi in netlist.pin_instances.values() {
            if pi.instance != inst_id {
                continue;
            }
            let Some(nid) = pi.net else { continue };
            match netlist.nets.get(nid).map(|n| &n.net_class) {
                Some(NetClass::Ground) => grounded = true,
                _ => hot = Some(nid),
            }
        }
        let (true, Some(hot)) = (grounded, hot) else { continue };
        // Only SIGNAL-net filters: a shunt cap on a declared power rail has
        // no meaningful series R among net-local members (rails are
        // low-impedance; a load resistor on the rail is not a filter
        // element — that misread produced a false 48Hz \"cutoff\" from an
        // LED resistor). Rail-filter verification needs ESR/source-impedance
        // data this pass does not have (Real-Data: skip, don't guess).
        if !is_signal_net(netlist, hot) {
            continue;
        }

        // Series parts on the hot net (values via attr, entity fallback).
        let (mut rs, mut ls): (Vec<f64>, Vec<f64>) = (Vec::new(), Vec::new());
        for m in members.get(&hot).into_iter().flatten() {
            if m.inst == inst.name {
                continue;
            }
            let Some(other) = netlist.instances.values().find(|i| i.name == m.inst)
            else { continue };
            let class = attr_of(netlist, analysis, other, "component_class")
                .unwrap_or_default();
            let val = other.attributes.get("value").and_then(|v| parse_si_txt(v));
            match (class.as_str(), val) {
                ("resistor", Some(v)) if v > 0.0 => rs.push(v),
                ("inductor", Some(v)) if v > 0.0 => ls.push(v),
                _ => {}
            }
        }
        let (f_actual, network) = match (rs.as_slice(), ls.as_slice()) {
            ([r], []) => (
                1.0 / (2.0 * std::f64::consts::PI * r * c_val),
                format!("R={} × C={}", fmt_eng(*r, "Ω"), fmt_eng(c_val, "F")),
            ),
            ([], [l]) => (
                1.0 / (2.0 * std::f64::consts::PI * (l * c_val).sqrt()),
                format!("L={} × C={}", fmt_eng(*l, "H"), fmt_eng(c_val, "F")),
            ),
            _ => continue, // ambiguous network — never guess
        };
        let octaves = (f_actual / f_decl).log2().abs();
        if octaves > 1.0 {
            out.push(DRCViolation {
                rule_id: "ERC022".into(),
                rule_name: "Intent contradiction".into(),
                category: RuleCategory::Electrical,
                severity: ViolationSeverity::Error,
                description: format!(
                    "{} declares {intent}(cutoff: {}) but the placed network \
                     {network} gives f_c = {} — {octaves:.1} octaves off the \
                     stated intent",
                    inst.name,
                    fmt_eng(f_decl, "Hz"),
                    fmt_eng(f_actual, "Hz"),
                ),
                location: ViolationLocation::Component(inst_id),
                fix_suggestion: "re-size the network for the declared cutoff, or \
                                 correct the intent annotation to what the board \
                                 actually builds"
                    .into(),
                standard_reference: None,
            });
        }
    }
    out
}

/// Engineering-notation formatter for rule messages (1.59e3,"Hz" → "1.59kHz").
fn fmt_eng(v: f64, unit: &str) -> String {
    let (scale, prefix) = if v >= 1e6 {
        (1e6, "M")
    } else if v >= 1e3 {
        (1e3, "k")
    } else if v >= 1.0 {
        (1.0, "")
    } else if v >= 1e-3 {
        (1e-3, "m")
    } else if v >= 1e-6 {
        (1e-6, "µ")
    } else if v >= 1e-9 {
        (1e-9, "n")
    } else {
        (1e-12, "p")
    };
    let n = v / scale;
    let s = format!("{n:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("{s}{prefix}{unit}")
}

// ──────────────────── ERC023 — precision-path grade mismatch ────────────────────

/// Parse a percentage in either idiom: "1%" → 0.01, "0.05" (already a
/// fraction ≤ 1) → 0.05. Anything else → None.
fn parse_percent(txt: &str) -> Option<f64> {
    let t = txt.trim();
    if let Some(p) = t.strip_suffix('%') {
        return p.trim().parse::<f64>().ok().map(|v| v / 100.0);
    }
    t.parse::<f64>().ok().filter(|v| *v > 0.0 && *v <= 1.0)
}

/// ERC023 — a part inside a declared precision path whose grade cannot
/// deliver the declared accuracy: `for precision_measurement(accuracy: 1%)`
/// marks every component in the flow (intent stamping, generation phase
/// 12.5); a 5%-tolerance resistor in that flow contradicts the declared
/// accuracy before a single measurement is taken. Error, both numbers in
/// the finding. Parts without a declared tolerance skip (Real-Data — the
/// absence ledger, not this rule, is where unknowns surface).
pub fn check_grade_mismatch(
    netlist: &Netlist,
    analysis: &AnalysisResult,
) -> Vec<DRCViolation> {
    let mut out = Vec::new();
    for (inst_id, inst) in &netlist.instances {
        if is_phantom(netlist, inst) {
            continue;
        }
        if inst.attributes.get("intent_name").map(String::as_str)
            != Some("precision_measurement")
        {
            continue;
        }
        let Some(accuracy) = ["intent_accuracy", "intent_param_0"]
            .iter()
            .find_map(|k| inst.attributes.get(*k).and_then(|v| parse_percent(v)))
        else { continue };
        let Some(tolerance) = attr_of(netlist, analysis, inst, "tolerance")
            .and_then(|v| parse_percent(&v))
        else { continue };
        if tolerance > accuracy {
            out.push(DRCViolation {
                rule_id: "ERC023".into(),
                rule_name: "Precision-path grade mismatch".into(),
                category: RuleCategory::Electrical,
                severity: ViolationSeverity::Error,
                description: format!(
                    "{} sits in a precision_measurement path declaring \
                     {:.2}% accuracy but is a {:.1}%-tolerance part — the \
                     path cannot meet its own declaration",
                    inst.name,
                    accuracy * 100.0,
                    tolerance * 100.0,
                ),
                location: ViolationLocation::Component(inst_id),
                fix_suggestion: format!(
                    "use a ≤{:.2}% part (E96/E192 grade), or relax the \
                     declared accuracy",
                    accuracy * 100.0
                ),
                standard_reference: None,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connected_substitution() {
        let pins = HashMap::from([("EN".to_string(), true), ("BOOT".to_string(), false)]);
        assert_eq!(substitute_connected("connected(EN)", &pins).as_deref(), Some("1"));
        assert_eq!(substitute_connected("connected(BOOT)", &pins).as_deref(), Some("0"));
        assert_eq!(
            substitute_connected("connected( EN ) + connected(BOOT)", &pins).as_deref(),
            Some("1 + 0")
        );
        // Unknown pin → Real-Data skip.
        assert_eq!(substitute_connected("connected(NOPIN)", &pins), None);
        // Non-call mention passes through untouched.
        assert_eq!(
            substitute_connected("self.connected_max > 2", &pins).as_deref(),
            Some("self.connected_max > 2")
        );
    }

    #[test]
    fn generic_predicate_substitution() {
        // exists(): always answerable.
        let kids: std::collections::HashSet<&str> = ["c_boot"].into_iter().collect();
        let sub = substitute_fn("exists(c_boot)", "exists", |a| match a {
            [n] => Some(kids.contains(n)),
            _ => None,
        });
        assert_eq!(sub.as_deref(), Some("1"));
        // Two-arg same_net with mixed answers.
        let sub = substitute_fn("same_net(MODE, GND)", "same_net", |a| match a {
            [x, y] => Some(x == &"MODE" && y == &"GND"),
            _ => None,
        });
        assert_eq!(sub.as_deref(), Some("1"));
        // Identifier merely containing the name is untouched.
        let sub = substitute_fn("coexists(x) + 1", "exists", |_| Some(true));
        assert_eq!(sub.as_deref(), Some("coexists(x) + 1"));
        // Unanswerable call skips the whole predicate.
        assert_eq!(substitute_fn("exists(?)", "exists", |_| None), None);
    }

    #[test]
    fn check_ctx_resolves_child_values_and_equality() {
        use crate::design_evaluator::evaluate_text;
        let ctx = CheckCtx {
            attrs: HashMap::from([("bootstrap_capacitor".to_string(), "100nF")]),
            child_values: HashMap::from([("c_boot".to_string(), 1.0e-7)]),
        };
        // Engineering equality across parse/format round-trips.
        assert_eq!(
            evaluate_text("c_boot.value == self.bootstrap_capacitor", &ctx).unwrap(),
            1.0
        );
        assert_eq!(
            evaluate_text("c_boot.value != self.bootstrap_capacitor", &ctx).unwrap(),
            0.0
        );
    }

    #[test]
    fn check_ctx_resolves_self_attrs() {
        use crate::design_evaluator::{evaluate_text, EvalLookup};
        let ctx = CheckCtx {
            attrs: HashMap::from([("input_voltage_max".to_string(), "28V")]),
            child_values: HashMap::new(),
        };
        assert_eq!(evaluate_text("self.input_voltage_max >= 12", &ctx).unwrap(), 1.0);
        assert!(ctx.lookup("vin").is_err()); // bare names unresolvable here
    }

    // ── ERC028 fixtures: a board, one Power rail, one member instance ──

    fn anchoring_fixture(
        class: &str,
        pin_dir: PinDirection,
        pin_type: PinType,
    ) -> (Netlist, NetId) {
        use bhdl_netlist::types::ModuleKind;
        let mut nl = Netlist::new();
        let board = nl.add_module("B".into(), ModuleKind::Board);
        nl.top_level_module = Some(board);
        let comp = nl.add_module("Comp".into(), ModuleKind::Component);
        nl.add_pin(comp, "P1".into(), pin_dir, pin_type).unwrap();
        let inst = nl.add_instance("u1".into(), comp).unwrap();
        if !class.is_empty() {
            nl.instances
                .get_mut(inst)
                .unwrap()
                .attributes
                .insert("component_class".into(), class.into());
        }
        let pis = nl.create_pin_instances(inst).unwrap();
        let net = nl.add_net_with_class(
            Some("VIN".into()),
            NetClass::Power { voltage: 12.0, current: None },
        );
        nl.connect(net, ConnectionPoint::PinInstance(pis[0])).unwrap();
        (nl, net)
    }

    fn add_board_port(nl: &mut Netlist, net: NetId, dir: bhdl_netlist::types::PortDirection) {
        let top = nl.top_level_module.unwrap();
        let pid = nl.add_port(top, "VIN".into(), dir, None).unwrap();
        nl.ports.get_mut(pid).unwrap().net = Some(net);
    }

    #[test]
    fn erc028_unanchored_rail_errors() {
        // A Power net feeding a plain load, with no port and no driver —
        // floating fiat.
        let (nl, _) = anchoring_fixture("", PinDirection::Power, PinType::Power);
        let v = check_rail_anchoring(&nl, &AnalysisResult::default());
        assert_eq!(v.len(), 1, "expected exactly the unanchored-rail Error");
        assert_eq!(v[0].rule_id, "ERC028");
        assert!(matches!(v[0].severity, ViolationSeverity::Error));
    }

    #[test]
    fn erc028_driven_rail_silent() {
        // A regulator-style `power out` pin drives the rail — anchored
        // on-board, no port needed.
        let (nl, _) = anchoring_fixture("", PinDirection::Out, PinType::Power);
        assert!(check_rail_anchoring(&nl, &AnalysisResult::default()).is_empty());
    }

    #[test]
    fn erc028_port_without_connector_warns() {
        // The port declares power arriving from outside, but nothing on the
        // net is solderable.
        let (mut nl, net) = anchoring_fixture("", PinDirection::Power, PinType::Power);
        add_board_port(&mut nl, net, bhdl_netlist::types::PortDirection::Input);
        let v = check_rail_anchoring(&nl, &AnalysisResult::default());
        assert_eq!(v.len(), 1, "expected exactly the no-connector Warning");
        assert_eq!(v[0].rule_id, "ERC028");
        assert!(matches!(v[0].severity, ViolationSeverity::Warning));
    }

    #[test]
    fn erc028_port_with_connector_silent() {
        // Port + dc-jack on the rail: the declared boundary has its
        // physical entry.
        let (mut nl, net) =
            anchoring_fixture("dc-jack", PinDirection::InOut, PinType::Signal);
        add_board_port(&mut nl, net, bhdl_netlist::types::PortDirection::Input);
        assert!(check_rail_anchoring(&nl, &AnalysisResult::default()).is_empty());
    }

    #[test]
    fn erc028_port_on_driven_rail_silent() {
        // A rail generated on-board (regulator VOUT): its desugared port is
        // not the boundary source, so no connector is demanded.
        let (mut nl, net) = anchoring_fixture("", PinDirection::Out, PinType::Power);
        add_board_port(&mut nl, net, bhdl_netlist::types::PortDirection::Input);
        assert!(check_rail_anchoring(&nl, &AnalysisResult::default()).is_empty());
    }
}

// ──────────────── ERC027 — amplifier stage-gain consistency ────────────────

/// ERC027 — the gain triangle on op-amp stages: DERIVED (evaluated from the
/// placed feedback network), MEASURED (a small-signal stimulus transient
/// through the behavioral amp models — 100 mV / 1 kHz at the chain input,
/// per-stage amplitude ratio OUT/INP over the final cycle), and DECLARED
/// (a `gain`/`gain_stage` intent when the designer stated one). Any pair
/// disagreeing by more than 25% is an Error carrying every number.
///
/// Real-Data skips: a stage whose feedback the classifier can't derive
/// contributes only measured-vs-declared; an unexcited stage (input
/// amplitude < 5 mV) or one whose output sits on its own rail (clipped —
/// amplitude ratios are meaningless there) is skipped; no chain, no
/// convertible circuit, or a failed transient → no violations invented.
/// The linear transient ignores diode branches — valid for this small-signal
/// probe (output clamps are reverse-biased far from the rails).
pub fn check_stage_gain(netlist: &Netlist, analysis: &AnalysisResult) -> Vec<DRCViolation> {
    use bhdl_schematic::v4::classify::ChainElem;
    use bhdl_spice::transient::{run_transient, Stimulus, TransientParams};

    let mut out = Vec::new();
    let plan = bhdl_schematic::v4::classify_sheet(netlist);
    if plan.chains.is_empty() {
        return out;
    }
    let mut converter = bhdl_spice::NetlistToSpiceConverter::new();
    let Ok(circuit) = converter.convert(netlist) else { return out };

    let net_nm = |id: NetId| netlist.nets.get(id).and_then(|n| n.name.clone());
    let inst_by_name =
        |n: &str| netlist.instances.iter().find(|(_, i)| i.name == n);
    let value_of = |n: &str| -> Option<f64> {
        inst_by_name(n)
            .and_then(|(_, i)| i.attributes.get("value").cloned())
            .and_then(|v| parse_si_txt(&v))
    };
    let class_of = |n: &str| -> String {
        inst_by_name(n)
            .and_then(|(_, i)| attr_of(netlist, analysis, i, "component_class"))
            .unwrap_or_default()
    };
    // Declared gain intent on the amp or any feedback part.
    const GAIN_INTENTS: [&str; 4] = ["gain", "gain_stage", "amplification", "amplifier"];
    let declared_gain = |parts: &[&str]| -> Option<f64> {
        parts.iter().find_map(|p| {
            let (_, i) = inst_by_name(p)?;
            let name = i.attributes.get("intent_name")?;
            if !GAIN_INTENTS.contains(&name.as_str()) {
                return None;
            }
            ["intent_gain", "intent_g", "intent_param_0"]
                .iter()
                .find_map(|k| i.attributes.get(*k).and_then(|v| parse_si_txt(v)))
        })
    };
    let rails_of = |inst_name: &str| -> Vec<f64> {
        let Some((iid, _)) = inst_by_name(inst_name) else { return Vec::new() };
        netlist
            .pin_instances
            .values()
            .filter(|pi| pi.instance == iid)
            .filter_map(|pi| netlist.nets.get(pi.net?))
            .filter_map(|n| match n.net_class {
                NetClass::Power { voltage, .. } => Some(voltage),
                _ => None,
            })
            .collect()
    };

    for chain in &plan.chains {
        // Per-amp: (name, inp net, out net, derived G).
        struct Stage<'a> {
            inst: &'a str,
            inp: String,
            outn: String,
            derived: Option<f64>,
            declared: Option<f64>,
        }
        let mut stages: Vec<Stage> = Vec::new();
        for (i, elem) in chain.elems.iter().enumerate() {
            let ChainElem::Amp { inst, fb_parts, gnd_leg, unity } = elem else { continue };
            let (Some(inp), Some(outn)) = (
                net_nm(chain.spine_nets[i]),
                net_nm(chain.spine_nets[i + 1]),
            ) else {
                continue;
            };
            let derived = if *unity {
                Some(1.0)
            } else {
                let rf = fb_parts.iter().find(|p| class_of(p) == "resistor");
                let rg = gnd_leg.iter().find(|p| class_of(p) == "resistor");
                match (rf, rg) {
                    (Some(rf), Some(rg)) => match (value_of(rf), value_of(rg)) {
                        (Some(vf), Some(vg)) if vg > 0.0 => Some(1.0 + vf / vg),
                        _ => None,
                    },
                    (Some(_), None) if gnd_leg.is_empty() => Some(1.0),
                    _ => None,
                }
            };
            let mut intent_hosts: Vec<&str> = vec![inst.as_str()];
            intent_hosts.extend(fb_parts.iter().map(String::as_str));
            intent_hosts.extend(gnd_leg.iter().map(String::as_str));
            stages.push(Stage {
                inst,
                inp,
                outn,
                derived,
                declared: declared_gain(&intent_hosts),
            });
        }
        if stages.is_empty() {
            continue;
        }
        let Some(input_net) = chain.spine_nets.first().copied().and_then(net_nm) else {
            continue;
        };

        const AMP: f64 = 0.1;
        const FREQ: f64 = 1_000.0;
        let mut probes: Vec<String> = vec![input_net.clone()];
        for s in &stages {
            for n in [&s.inp, &s.outn] {
                if !probes.contains(n) {
                    probes.push(n.clone());
                }
            }
        }
        let params = TransientParams::new(
            input_net,
            Stimulus::Sine { amplitude: AMP, frequency_hz: FREQ, dc_offset: 0.0 },
            probes,
            5.0 / FREQ,
            1.0 / FREQ / 200.0,
        );
        let Ok(result) = run_transient(&circuit, &params) else { continue };
        let tail_amp = |net: &str| -> Option<(f64, f64, f64)> {
            let v = result.probe_voltages.get(net)?;
            let tail = &v[v.len().saturating_sub(200)..];
            let max = tail.iter().cloned().fold(f64::MIN, f64::max);
            let min = tail.iter().cloned().fold(f64::MAX, f64::min);
            Some(((max - min) / 2.0, max, min))
        };

        for s in &stages {
            let Some((a_in, _, _)) = tail_amp(&s.inp) else { continue };
            let Some((a_out, omax, omin)) = tail_amp(&s.outn) else { continue };
            if a_in < 5e-3 {
                continue; // unexcited — a ratio of noise is not a measurement
            }
            let clipped = rails_of(s.inst)
                .iter()
                .any(|r| (omax - r).abs() < 1e-3 || (omin - r).abs() < 1e-3);
            let measured = if clipped { None } else { Some(a_out / a_in) };

            // The triangle: every resolvable pair must agree within 25%.
            let pairs: [(&str, Option<f64>, &str, Option<f64>); 3] = [
                ("derived", s.derived, "measured", measured),
                ("declared", s.declared, "derived", s.derived),
                ("declared", s.declared, "measured", measured),
            ];
            for (an, a, bn, b) in pairs {
                let (Some(a), Some(b)) = (a, b) else { continue };
                if a <= 0.0 || b <= 0.0 {
                    continue;
                }
                let ratio = if a > b { a / b } else { b / a };
                if ratio > 1.25 {
                    let (iid, _) = inst_by_name(s.inst).expect("stage instance exists");
                    out.push(DRCViolation {
                        rule_id: "ERC027".into(),
                        rule_name: "Stage-gain consistency".into(),
                        category: RuleCategory::Electrical,
                        severity: ViolationSeverity::Error,
                        description: format!(
                            "{}: {an} gain ×{a:.2} disagrees with {bn} gain ×{b:.2} \
                             ({:.0}% apart; derived {}, declared {}, measured {})",
                            s.inst,
                            (ratio - 1.0) * 100.0,
                            s.derived.map(|g| format!("×{g:.2}")).unwrap_or_else(|| "—".into()),
                            s.declared.map(|g| format!("×{g:.2}")).unwrap_or_else(|| "—".into()),
                            measured.map(|g| format!("×{g:.2}")).unwrap_or_else(|| "—".into()),
                        ),
                        location: ViolationLocation::Component(iid),
                        fix_suggestion: "re-size the feedback network for the stated gain, \
                             correct the intent, or fix the wiring the simulation is seeing"
                            .into(),
                        standard_reference: None,
                    });
                    break; // one violation per stage carries the whole triangle
                }
            }
        }
    }
    out
}

// ──────────────── ERC028 — unanchored power rail / connectorless port ────────────────

/// Connector-class instances are the solderable physical entry points. Same
/// class set the schematic layer promotes into the flow diagram, plus
/// testpoints (probeable is solderable).
fn is_connector_class(class: &str) -> bool {
    matches!(
        class,
        "dc-jack" | "jack" | "connector" | "header" | "usb" | "testpoint"
    ) || class.contains("connector")
}

/// ERC028 — the ports-doctrine anchor check. Power is not magic: every rail's
/// energy has an accountable origin.
///
/// Error — "unanchored rail": a Power-class net with NO board port and NO
/// on-board driver (a regulator/source pin declared `power out`, a model
/// source, or a power-symbol instance). Nothing physical or declared feeds
/// it — its voltage is fiat. Declared rails always lower to a board port, so
/// this fires on nets that became Power-class some other way (net-name
/// heuristics, imports) without a source.
///
/// Warning — "nothing to solder": a power-in board port that is the rail's
/// actual boundary source (net has no on-board driver, so the DC solve puts
/// the ideal source at this port) but whose net touches no connector-class
/// instance. The board declares power arriving from outside yet provides no
/// physical part for it to arrive through. Ports on internally-generated
/// rails (a regulator drives the net) are not boundaries in the built board
/// and are skipped.
pub fn check_rail_anchoring(
    netlist: &Netlist,
    _analysis: &AnalysisResult,
) -> Vec<DRCViolation> {
    let mut out = Vec::new();
    let members = net_members(netlist);

    // Board ports on the top-level module, by net.
    let mut port_by_net: HashMap<NetId, (&str, bhdl_netlist::types::PortDirection)> =
        HashMap::new();
    for (_, port) in &netlist.ports {
        if Some(port.module) != netlist.top_level_module {
            continue;
        }
        if let Some(net_id) = port.net {
            port_by_net.insert(net_id, (port.name.as_str(), port.direction));
        }
    }

    for (net_id, net) in &netlist.nets {
        if !matches!(net.net_class, NetClass::Power { .. }) {
            continue;
        }
        let pins = members.get(&net_id).map(Vec::as_slice).unwrap_or(&[]);
        if pins.is_empty() && port_by_net.get(&net_id).is_none() {
            continue; // dead net — no members, no boundary; not a rail at all
        }

        // On-board driver: any push-pull output pin (a regulator's
        // `power out` VOUT/SW, or a filter/buffer `signal out` feeding a
        // net that is Power-class by name heuristic), a model source, or a
        // power-symbol instance (+5V-style module).
        let has_driver = pins.iter().any(|p| {
            matches!(p.dir, PinDirection::Out)
                || matches!(p.class.as_str(), "power_source" | "battery")
        }) || net.connections.iter().any(|cp| {
            let bhdl_netlist::types::ConnectionPoint::PinInstance(pi_id) = cp else {
                return false;
            };
            netlist
                .pin_instances
                .get(*pi_id)
                .filter(|pi| pi.net == Some(net_id))
                .and_then(|pi| netlist.instances.get(pi.instance))
                .and_then(|i| netlist.modules.get(i.definition))
                .map(|m| m.name.starts_with('+'))
                .unwrap_or(false)
        });

        match port_by_net.get(&net_id) {
            None => {
                if !has_driver {
                    out.push(DRCViolation {
                        rule_id: "ERC028".into(),
                        rule_name: "Unanchored power rail".into(),
                        category: RuleCategory::Electrical,
                        severity: ViolationSeverity::Error,
                        description: format!(
                            "Power rail '{}' has no board port and no on-board \
                             driver — its voltage is fiat: nothing physical or \
                             declared feeds it",
                            net_name(netlist, net_id)
                        ),
                        location: ViolationLocation::Net(net_id),
                        fix_suggestion:
                            "declare the boundary (`port X: power in = V @ I;` or the \
                             `power X = V @ I;` sugar) or wire the rail to a source"
                                .into(),
                        standard_reference: None,
                    });
                }
            }
            Some((port_name, direction)) => {
                // Only in-direction ports that are the rail's real boundary
                // source need a physical entry point.
                if !matches!(direction, bhdl_netlist::types::PortDirection::Input)
                    || has_driver
                {
                    continue;
                }
                let has_connector = pins.iter().any(|p| is_connector_class(&p.class));
                if !has_connector {
                    out.push(DRCViolation {
                        rule_id: "ERC028".into(),
                        rule_name: "Port has no physical connector".into(),
                        category: RuleCategory::Electrical,
                        severity: ViolationSeverity::Warning,
                        description: format!(
                            "Board port '{port_name}' supplies rail '{}' from outside, \
                             but the net touches no connector-class instance — nothing \
                             to solder",
                            net_name(netlist, net_id)
                        ),
                        location: ViolationLocation::Net(net_id),
                        fix_suggestion:
                            "add the physical entry (dc-jack/header/usb/testpoint) the \
                             power arrives through, and wire it to the rail"
                                .into(),
                        standard_reference: None,
                    });
                }
            }
        }
    }
    out
}
