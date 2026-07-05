//! T3 — org-policy ERC plugins (docs/spec/ERC.md §2).
//!
//! Review policy that is neither universal physics (T1) nor device IP (T2):
//! naming conventions, forbidden vendors, creepage classes, house rules.
//! Such policy is proprietary and lives OUTSIDE the tree, as an external
//! process — the same single stdin/stdout JSON exchange discipline as the
//! BOM-selection plugins (`bhdl-analyzer::plugin`) and supply-chain
//! providers.
//!
//! Configuration: `BHDL_ERC_PLUGINS` — colon-separated executable paths.
//! Each plugin receives one [`DesignSummary`] on stdin and replies with one
//! [`PolicyResponse`] on stdout. Its findings enter the DRC report BEFORE
//! the waiver partition, so severity gating (`--erc-fail-on`) and
//! `erc_waive` waivers treat org findings exactly like built-in ones.
//!
//! Failure semantics: a plugin that cannot be spawned, exits non-zero, or
//! replies with unparseable JSON is surfaced as a single Warning finding
//! (rule `ERC-PLUGIN`) — a broken policy gate must be VISIBLE, but it is a
//! tooling failure, not a design fact, so it does not fabricate design
//! errors (Real-Data Policy applied to tooling).

use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};

use bhdl_analyzer::AnalysisResult;
use bhdl_netlist::netlist::Netlist;
use bhdl_netlist::types::{ConnectionPoint, NetClass, PinDirection};
use log::{debug, warn};
use serde::{Deserialize, Serialize};

use crate::design_rule_checker::{
    DRCViolation, RuleCategory, ViolationLocation, ViolationSeverity,
};

// ─────────────────────────── input schema ───────────────────────────

/// What a policy plugin sees: the design ABOVE the netlist — instances with
/// their entities and attributes, nets with class and membership, rails with
/// budgets. Enough for naming/vendor/topology policy without re-deriving
/// connectivity.
#[derive(Debug, Serialize)]
pub struct DesignSummary {
    pub protocol_version: String,
    pub kind: String, // "erc_policy_check"
    pub instances: Vec<SummaryInstance>,
    pub nets: Vec<SummaryNet>,
}

#[derive(Debug, Serialize)]
pub struct SummaryInstance {
    pub refdes: String,
    pub entity: String,
    pub attributes: HashMap<String, String>,
    pub pins: Vec<SummaryPin>,
}

#[derive(Debug, Serialize)]
pub struct SummaryPin {
    pub name: String,
    pub direction: String,
    /// Net NAME the pin landed on, absent when unconnected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SummaryNet {
    pub name: String,
    pub class: String, // "signal" | "power" | "ground" | ...
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voltage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_a: Option<f64>,
    pub members: Vec<String>, // "refdes.PIN"
}

// ─────────────────────────── output schema ───────────────────────────

#[derive(Debug, Deserialize)]
pub struct PolicyResponse {
    #[allow(dead_code)]
    pub protocol_version: String,
    #[serde(default)]
    pub findings: Vec<PolicyFinding>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct PolicyFinding {
    /// Org rule id (e.g. "ACME-NAME-001"). Waivable via `erc_waive` exactly
    /// like a built-in id.
    pub rule_id: String,
    pub severity: String, // "critical" | "error" | "warning" | "info"
    pub description: String,
    #[serde(default)]
    pub fix: String,
    /// Optional anchor: an instance refdes or a net name from the summary.
    #[serde(default)]
    pub instance: Option<String>,
    #[serde(default)]
    pub net: Option<String>,
}

// ─────────────────────────── invocation ───────────────────────────

fn direction_str(d: &PinDirection) -> &'static str {
    match d {
        PinDirection::In => "in",
        PinDirection::Out => "out",
        PinDirection::InOut => "inout",
        PinDirection::Power => "power",
        PinDirection::Ground => "ground",
        PinDirection::Passive => "passive",
    }
}

fn class_str(c: &NetClass) -> String {
    match c {
        NetClass::Signal => "signal".into(),
        NetClass::Power { .. } => "power".into(),
        NetClass::Ground => "ground".into(),
        other => format!("{other:?}").to_lowercase(),
    }
}

/// Build the summary a policy plugin receives.
pub fn build_summary(netlist: &Netlist, _analysis: &AnalysisResult) -> DesignSummary {
    let net_name = |nid: bhdl_netlist::types::NetId| {
        netlist
            .nets
            .get(nid)
            .map(|n| n.name.clone().unwrap_or_else(|| format!("{nid:?}")))
    };

    let mut instances = Vec::new();
    for (inst_id, inst) in &netlist.instances {
        // Skip the abstract module-definition phantoms (same guard as ERC).
        if netlist
            .modules
            .get(inst.definition)
            .map(|m| m.name == inst.name)
            .unwrap_or(false)
        {
            continue;
        }
        let entity = netlist
            .modules
            .get(inst.definition)
            .map(|m| m.name.clone())
            .unwrap_or_default();
        let mut pins = Vec::new();
        for pi in netlist.pin_instances.values() {
            if pi.instance != inst_id {
                continue;
            }
            let Some(pin) = netlist.pins.get(pi.pin_def) else { continue };
            pins.push(SummaryPin {
                name: pin.name.clone(),
                direction: direction_str(&pin.direction).to_string(),
                net: pi.net.and_then(net_name),
            });
        }
        pins.sort_by(|a, b| a.name.cmp(&b.name));
        instances.push(SummaryInstance {
            refdes: inst.name.clone(),
            entity,
            attributes: inst.attributes.clone().into_iter().collect(),
            pins,
        });
    }
    instances.sort_by(|a, b| a.refdes.cmp(&b.refdes));

    let mut nets = Vec::new();
    for (net_id, net) in &netlist.nets {
        let mut members = Vec::new();
        for cp in &net.connections {
            let ConnectionPoint::PinInstance(pi_id) = cp else { continue };
            let Some(pi) = netlist.pin_instances.get(*pi_id) else { continue };
            if pi.net != Some(net_id) {
                continue; // stale duplicate-net residue — trust the back-pointer
            }
            let (Some(pin), Some(inst)) =
                (netlist.pins.get(pi.pin_def), netlist.instances.get(pi.instance))
            else {
                continue;
            };
            members.push(format!("{}.{}", inst.name, pin.name));
        }
        if members.is_empty() {
            continue; // vestigial empty nets carry no policy signal
        }
        members.sort();
        let (voltage, budget_a) = match net.net_class {
            NetClass::Power { voltage, current } => (Some(voltage), current),
            _ => (None, None),
        };
        nets.push(SummaryNet {
            name: net.name.clone().unwrap_or_else(|| format!("{net_id:?}")),
            class: class_str(&net.net_class),
            voltage,
            budget_a,
            members,
        });
    }
    nets.sort_by(|a, b| a.name.cmp(&b.name));

    DesignSummary {
        protocol_version: "1".to_string(),
        kind: "erc_policy_check".to_string(),
        instances,
        nets,
    }
}

/// One Warning finding describing a plugin-tooling failure — visible in the
/// report, but never a fabricated design error.
fn tooling_failure(plugin: &str, why: String) -> DRCViolation {
    DRCViolation {
        rule_id: "ERC-PLUGIN".into(),
        rule_name: "Policy plugin failure".into(),
        category: RuleCategory::Electrical,
        severity: ViolationSeverity::Warning,
        description: format!("policy plugin '{plugin}' did not produce findings: {why}"),
        location: ViolationLocation::Global,
        fix_suggestion: "fix or remove the plugin from BHDL_ERC_PLUGINS — a broken \
                         policy gate checks nothing"
            .into(),
        standard_reference: None,
    }
}

/// Run every plugin in `BHDL_ERC_PLUGINS` (colon-separated executables) over
/// the design summary. Returns the mapped findings and how many plugins ran.
pub fn run_policy_plugins(
    netlist: &Netlist,
    analysis: &AnalysisResult,
) -> (Vec<DRCViolation>, usize) {
    let Ok(spec) = std::env::var("BHDL_ERC_PLUGINS") else {
        return (Vec::new(), 0);
    };
    let plugins: Vec<&str> = spec.split(':').filter(|s| !s.is_empty()).collect();
    if plugins.is_empty() {
        return (Vec::new(), 0);
    }

    let summary = build_summary(netlist, analysis);
    let input = match serde_json::to_string(&summary) {
        Ok(s) => s,
        Err(e) => {
            warn!("ERC policy plugins skipped: summary serialization failed: {e}");
            return (Vec::new(), 0);
        }
    };

    // Name → id maps for anchoring findings back onto the design.
    let inst_ids: HashMap<String, bhdl_netlist::types::InstanceId> = netlist
        .instances
        .iter()
        .map(|(id, i)| (i.name.clone(), id))
        .collect();
    let net_ids: HashMap<String, bhdl_netlist::types::NetId> = netlist
        .nets
        .iter()
        .filter_map(|(id, n)| n.name.clone().map(|nm| (nm, id)))
        .collect();

    let mut out = Vec::new();
    let mut ran = 0usize;
    for plugin in plugins {
        ran += 1;
        let mut child = match Command::new(plugin)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                out.push(tooling_failure(plugin, format!("spawn failed: {e}")));
                continue;
            }
        };
        if let Some(stdin) = child.stdin.take() {
            let mut stdin = stdin;
            if let Err(e) = stdin.write_all(input.as_bytes()) {
                out.push(tooling_failure(plugin, format!("stdin write failed: {e}")));
                let _ = child.wait();
                continue;
            }
            // Drop closes the pipe — the plugin sees EOF and replies.
        }
        let output = match child.wait_with_output() {
            Ok(o) => o,
            Err(e) => {
                out.push(tooling_failure(plugin, format!("wait failed: {e}")));
                continue;
            }
        };
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            out.push(tooling_failure(
                plugin,
                format!("exit {:?}: {}", output.status.code(), stderr.trim()),
            ));
            continue;
        }
        let response: PolicyResponse = match serde_json::from_slice(&output.stdout) {
            Ok(r) => r,
            Err(e) => {
                out.push(tooling_failure(plugin, format!("bad response JSON: {e}")));
                continue;
            }
        };
        for w in &response.warnings {
            warn!("policy plugin '{plugin}': {w}");
        }
        debug!(
            "policy plugin '{plugin}': {} finding(s), {} warning(s)",
            response.findings.len(),
            response.warnings.len()
        );
        for f in response.findings {
            let severity = match f.severity.to_lowercase().as_str() {
                "critical" => ViolationSeverity::Critical,
                "error" => ViolationSeverity::Error,
                "warning" => ViolationSeverity::Warning,
                "info" => ViolationSeverity::Info,
                other => {
                    out.push(tooling_failure(
                        plugin,
                        format!("finding '{}' has unknown severity '{other}'", f.rule_id),
                    ));
                    continue;
                }
            };
            let location = if let Some(id) =
                f.instance.as_ref().and_then(|n| inst_ids.get(n))
            {
                ViolationLocation::Component(*id)
            } else if let Some(id) = f.net.as_ref().and_then(|n| net_ids.get(n)) {
                ViolationLocation::Net(*id)
            } else {
                ViolationLocation::Global
            };
            out.push(DRCViolation {
                rule_id: f.rule_id,
                rule_name: "Org policy".into(),
                category: RuleCategory::Electrical,
                severity,
                description: f.description,
                location,
                fix_suggestion: if f.fix.is_empty() {
                    "see the org policy owning this rule id".into()
                } else {
                    f.fix
                },
                standard_reference: None,
            });
        }
    }
    (out, ran)
}
