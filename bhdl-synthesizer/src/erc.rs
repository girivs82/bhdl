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
