//! The per-build requirement TRACE MATRIX
//! (docs/spec/Requirements_And_Resolution.md §4).
//!
//! Rule: a requirement the machine cannot check is documentation, not a
//! requirement. So the matrix is derived — never transcribed — from the
//! contract constructs that already have a machine verifier, and every
//! row ends in evidence:
//!
//! | kind | stated by | verifier |
//! |---|---|---|
//! | stage requirement `u1: BuckStage(...)` | designer | resolver + ERC032 |
//! | rail budget `power V = V @ I` / `port … power out` | designer | ERC028 (driven / boundary) |
//! | vendor `domain` contract (`decouple` target) | the part | PDN Z(f) mask sweep + single-open verification |
//! | part-carried `check { require … }` | the part | ERC025 |
//! | safety goal | safety goal | fault campaign (`bhdl safety`) |
//!
//! Status per row: `Verified` (evidence recorded), `Violated` (a verifier
//! rejected it), `Unverified` (the verifier could not run — stated as a
//! FINDING, never a pass), `Unresolved` (a requirement with no
//! implementing element yet).
//!
//! Ids: every row has a stable machine id (`<board>.<inst>`, `rail.<net>`,
//! `<inst>.<domain>`, `<inst>.check[n]`, the safety goal's declared id or
//! path). A designer may name a requirement explicitly with the scoped
//! attribute `attribute u1.requirement_id = "PWR-003";` — the id is then
//! what the matrix reports and what `satisfies` links resolve against.
//!
//! `satisfies { REQ: via <element>; }` links (the safety prototype's
//! grammar, generalized): the element is appended to the requirement's
//! implementing set; a link naming an unknown requirement or element is a
//! finding.

use std::collections::{BTreeMap, BTreeSet};

use bhdl_netlist::netlist::Netlist;
use serde::{Deserialize, Serialize};

use crate::design_rule_checker::{DRCViolation, ViolationLocation, ViolationSeverity};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TraceStatus {
    Verified,
    Violated,
    Unverified,
    Unresolved,
}

impl TraceStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            TraceStatus::Verified => "VERIFIED",
            TraceStatus::Violated => "VIOLATED",
            TraceStatus::Unverified => "UNVERIFIED",
            TraceStatus::Unresolved => "UNRESOLVED",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceRow {
    pub id: String,
    pub kind: String,
    /// Who originates the requirement (designer / part / safety goal).
    pub stated_by: String,
    /// The requirement as stated.
    pub statement: String,
    /// Implementing design elements (instances, blocks, mechanisms).
    pub implemented_by: Vec<String>,
    pub verifier: String,
    pub status: TraceStatus,
    /// Evidence or the reason the verifier could not run.
    pub evidence: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TraceMatrix {
    pub board: String,
    pub rows: Vec<TraceRow>,
    /// Findings that are not rows: dangling `satisfies` links etc.
    pub findings: Vec<String>,
}

impl TraceMatrix {
    pub fn count(&self, s: TraceStatus) -> usize {
        self.rows.iter().filter(|r| r.status == s).count()
    }
    /// The build-gating verdict: no VIOLATED rows and no findings.
    /// UNVERIFIED / UNRESOLVED rows are stated, not gating here — the
    /// commit gate (resolution required at commit) is a policy above this.
    pub fn clean(&self) -> bool {
        self.count(TraceStatus::Violated) == 0 && self.findings.is_empty()
    }
}

fn inst_name(n: &Netlist, loc: &ViolationLocation) -> Option<String> {
    match loc {
        ViolationLocation::Component(id) => n.instances.get(*id).map(|i| i.name.clone()),
        _ => None,
    }
}

fn net_name(n: &Netlist, loc: &ViolationLocation) -> Option<String> {
    match loc {
        ViolationLocation::Net(id) => n.nets.get(*id).and_then(|x| x.name.clone()),
        _ => None,
    }
}

/// Build the matrix from the build's evidence. `violations` = the DRC run
/// on this netlist; `safety` = the resolved safety model (goals +
/// mechanisms + gaps; `universe` non-empty only when the campaign ran).
pub fn build_trace_matrix(
    netlist: &Netlist,
    analysis: &bhdl_analyzer::AnalysisResult,
    sf: &bhdl_ast::SourceFile,
    violations: &[DRCViolation],
    safety: Option<&bhdl_common::safety::SafetyModel>,
) -> TraceMatrix {
    use rowan::ast::AstNode;
    let board = sf
        .syntax()
        .descendants()
        .find(|n| n.kind() == bhdl_parser::SyntaxKind::BOARD_DEF)
        .and_then(|b| {
            b.children_with_tokens()
                .filter_map(|e| e.into_token())
                .find(|t| t.kind() == bhdl_parser::SyntaxKind::IDENT)
                .map(|t| t.text().to_string())
        })
        .unwrap_or_default();
    let mut m = TraceMatrix { board: board.clone(), ..Default::default() };

    // explicit ids: `attribute <inst>.requirement_id = "…"`
    let explicit_id = |inst: &bhdl_netlist::Instance| -> Option<String> {
        inst.attributes.get("requirement_id").cloned().filter(|s| !s.is_empty())
    };

    // ── 1. stage requirements ──
    // template stubs (creation-site `template="true"`) are not design
    // elements — they would duplicate every domain row
    let mut insts: Vec<_> = netlist
        .instances
        .iter()
        .filter(|(id, _)| !crate::is_template_stub(netlist, *id))
        .map(|(_, i)| i)
        .collect();
    insts.sort_by(|a, b| a.name.cmp(&b.name));
    for inst in &insts {
        let Some(trait_name) = inst.attributes.get("stage_trait") else { continue };
        let req = inst.attributes.get("stage_requirement").cloned().unwrap_or_default();
        let bound = inst.attributes.get("stage_bound").cloned().unwrap_or_default();
        let basis = inst.attributes.get("stage_binding").cloned().unwrap_or_default();
        let id = explicit_id(inst).unwrap_or_else(|| format!("{board}.{}", inst.name));
        let mut implemented: Vec<String> = Vec::new();
        if !bound.is_empty() {
            implemented.push(format!("{} : {bound}", inst.name));
            let mut kids: Vec<String> = netlist
                .instances
                .iter()
                .filter(|(_, k)| {
                    k.attributes.get("composed_parent").map(|p| p == &inst.name).unwrap_or(false)
                        || k.attributes.get("expansion_parent").map(|p| p == &inst.name).unwrap_or(false)
                })
                .map(|(_, k)| k.name.clone())
                .collect();
            kids.sort();
            implemented.extend(kids);
        }
        let erc032: Vec<&DRCViolation> = violations
            .iter()
            .filter(|v| v.rule_id == "ERC032" && inst_name(netlist, &v.location).as_deref() == Some(inst.name.as_str()))
            .collect();
        let (status, evidence) = if bound.is_empty() {
            (TraceStatus::Unresolved, "no library block covers the requirement (see the ⚙ survey near-misses); Generic* placeholder emitted".to_string())
        } else if let Some(v) = erc032.iter().find(|v| matches!(v.severity, ViolationSeverity::Error | ViolationSeverity::Critical)) {
            (TraceStatus::Violated, format!("ERC032: {}", v.description))
        } else if let Some(v) = erc032.iter().find(|v| v.description.contains("UNCHECKED")) {
            (TraceStatus::Unverified, format!("ERC032: {}", v.description))
        } else {
            (TraceStatus::Verified, format!("bound by {basis}; block envelope + promises accepted at resolution; ERC032 clean on the flattened circuit"))
        };
        m.rows.push(TraceRow {
            id,
            kind: "stage requirement".into(),
            stated_by: "designer".into(),
            statement: format!("{trait_name}({req})"),
            implemented_by: implemented,
            verifier: "resolver (trial-instantiation) + ERC032".into(),
            status,
            evidence,
        });
    }

    // ── 2. rail budgets ──
    let mut rails: Vec<(String, String)> = netlist
        .nets
        .iter()
        .filter_map(|(_, n)| match &n.net_class {
            bhdl_netlist::types::NetClass::Power { voltage, current } => n
                .name
                .clone()
                .map(|nm| (nm, format!("{voltage}V @ {}", current.map(|c| format!("{c}A")).unwrap_or_else(|| "unbudgeted".into())))),
            _ => None,
        })
        .collect();
    rails.sort();
    rails.dedup();
    for (net, stmt) in rails {
        let on_rail: Vec<&DRCViolation> = violations
            .iter()
            .filter(|v| {
                v.rule_id == "ERC028"
                    && (net_name(netlist, &v.location).as_deref() == Some(net.as_str())
                        || v.description.contains(&format!("'{net}'")))
            })
            .collect();
        let drivers: Vec<String> = {
            let mut d: Vec<String> = netlist
                .pin_instances
                .values()
                .filter(|pi| netlist.nets.get(pi.net.unwrap_or_default()).and_then(|n| n.name.as_deref()) == Some(net.as_str()))
                .filter_map(|pi| {
                    let p = netlist.pins.get(pi.pin_def)?;
                    let i = netlist.instances.get(pi.instance)?;
                    (p.direction == bhdl_netlist::types::PinDirection::Out).then(|| i.name.clone())
                })
                .collect();
            d.sort();
            d.dedup();
            d
        };
        let (status, evidence) = if let Some(v) = on_rail.iter().find(|v| matches!(v.severity, ViolationSeverity::Error | ViolationSeverity::Critical)) {
            (TraceStatus::Violated, format!("ERC028: {}", v.description))
        } else if let Some(v) = on_rail.first() {
            (TraceStatus::Verified, format!("ERC028 (warning, not gating): {}", v.description))
        } else {
            (TraceStatus::Verified, "ERC028 clean: rail driven / boundary consistent".into())
        };
        m.rows.push(TraceRow {
            id: format!("{board}.rail.{net}"),
            kind: "rail budget".into(),
            stated_by: "designer".into(),
            statement: format!("{net} = {stmt}"),
            implemented_by: drivers,
            verifier: "ERC028 (driven / boundary)".into(),
            status,
            evidence,
        });
    }

    // ── 3. vendor domain contracts ──
    let domain_map = crate::safety_model::entity_domain_map(sf.syntax());
    let decap: Vec<bhdl_common::analysis_interface::DecapReport> = netlist
        .get_analysis_data()
        .map(|a| a.decap_reports.clone())
        .unwrap_or_default();
    for inst in &insts {
        let module = netlist.modules.get(inst.definition).map(|m| m.name.clone()).unwrap_or_default();
        let Some((domains, _)) = domain_map.get(&module) else { continue };
        for d in domains {
            let target = format!("{}.{}", inst.name, d.name);
            let stmt = format!(
                "{} {:.3}V{}{}",
                d.name,
                d.v_nom,
                d.tol_pct.map(|t| format!(" ±{t}%")).unwrap_or_default(),
                d.i_max_a.map(|i| format!(" ≤ {i}A")).unwrap_or_default()
            );
            let rep = decap.iter().find(|r| r.target == target);
            let (status, evidence, implemented) = match rep {
                Some(r) => {
                    let mut imp: Vec<String> = r.steps.iter().map(|s| format!("{} : {} {}", s.instance, s.entity, s.value)).collect();
                    imp.extend(r.margin_added.iter().cloned());
                    if r.final_ratio <= 1.0 {
                        (
                            TraceStatus::Verified,
                            format!(
                                "PDN mask met: worst |Z|/mask {:.3} at {:.3e} Hz (derated {}%); {} single-open(s) verified, {} bulk exempt",
                                r.final_ratio, r.final_freq_hz, r.z_margin_pct, r.opens_verified, r.opens_bulk_exempt
                            ),
                            imp,
                        )
                    } else {
                        (
                            TraceStatus::Violated,
                            format!("PDN mask NOT met: worst |Z|/mask {:.3} at {:.3e} Hz", r.final_ratio, r.final_freq_hz),
                            imp,
                        )
                    }
                }
                None => (
                    TraceStatus::Unverified,
                    "vendor domain contract declared but no `decouple` statement targets it — PDN Z(f)/droop never checked".into(),
                    Vec::new(),
                ),
            };
            m.rows.push(TraceRow {
                id: format!("{board}.{target}"),
                kind: "vendor domain contract".into(),
                stated_by: format!("part {module}"),
                statement: stmt,
                implemented_by: implemented,
                verifier: "PDN Z(f) mask sweep + single-open verification (decouple)".into(),
                status,
                evidence,
            });
        }
    }

    // ── 4. part-carried check rules ──
    for inst in &insts {
        let module = netlist.modules.get(inst.definition).map(|m| m.name.clone()).unwrap_or_default();
        let Some(recipe) = analysis.stress_recipes.get(&module) else { continue };
        for (i, chk) in recipe.checks.iter().enumerate() {
            let cond = chk.condition.trim();
            let mine: Vec<&DRCViolation> = violations
                .iter()
                .filter(|v| v.rule_id == "ERC025" && inst_name(netlist, &v.location).as_deref() == Some(inst.name.as_str()) && v.description.contains(cond))
                .collect();
            let (status, evidence) = if let Some(v) = mine.iter().find(|v| matches!(v.severity, ViolationSeverity::Error | ViolationSeverity::Critical)) {
                (TraceStatus::Violated, format!("ERC025: {}", v.description))
            } else if let Some(v) = mine.iter().find(|v| v.description.contains("UNCHECKED")) {
                (TraceStatus::Unverified, format!("ERC025: {}", v.description))
            } else {
                (TraceStatus::Verified, "ERC025: predicate holds on the netlist".into())
            };
            m.rows.push(TraceRow {
                id: format!("{board}.{}.check[{i}]", inst.name),
                kind: "part-carried check".into(),
                stated_by: format!("part {module}"),
                statement: format!("require {cond} else \"{}\"", chk.message),
                implemented_by: vec![inst.name.clone()],
                verifier: "ERC025".into(),
                status,
                evidence,
            });
        }
    }

    // ── 5. safety goals ──
    if let Some(sm) = safety {
        let campaign_ran = !sm.universe.is_empty();
        for scope in &sm.scopes {
            for g in &scope.goals {
                let mechs: Vec<String> = scope
                    .mechanisms
                    .iter()
                    .filter(|mm| mm.goal == g.path)
                    .map(|mm| format!("{} ({:?})", mm.instance, mm.kind))
                    .collect();
                let gaps: Vec<&bhdl_common::safety::Gap> = sm.gaps.iter().filter(|gp| gp.goal == g.path).collect();
                let (status, evidence) = if !gaps.is_empty() {
                    (
                        TraceStatus::Violated,
                        gaps.iter().map(|gp| format!("{:?}: {} — {}", gp.class, gp.subject, gp.fix)).collect::<Vec<_>>().join("; "),
                    )
                } else if !campaign_ran {
                    (TraceStatus::Unverified, "fault campaign not part of this build — run `bhdl safety` for the measured DC / FTTI evidence".into())
                } else if mechs.is_empty() {
                    (TraceStatus::Unresolved, "goal declares no mechanism".into())
                } else {
                    (TraceStatus::Verified, "fault campaign: no gap against this goal".into())
                };
                m.rows.push(TraceRow {
                    id: g.id.clone().unwrap_or_else(|| g.path.clone()),
                    kind: "safety goal".into(),
                    stated_by: format!("safety goal {} ({})", g.path, g.level.as_str()),
                    statement: format!(
                        "{}{}",
                        g.title,
                        g.ftti.as_ref().map(|f| format!(" — FTTI {f}")).unwrap_or_default()
                    ),
                    implemented_by: mechs,
                    verifier: "fault campaign (measured DC, FTTI) — `bhdl safety`".into(),
                    status,
                    evidence,
                });
            }
        }
    }

    // ── 6. `satisfies { REQ: via element; }` links ──
    let ids: BTreeSet<String> = m.rows.iter().map(|r| r.id.clone()).collect();
    let inst_names: BTreeSet<String> = insts.iter().map(|i| i.name.clone()).collect();
    let mut links: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for item in sf.syntax().descendants().filter(|n| n.kind() == bhdl_parser::SyntaxKind::SATISFIES_ITEM) {
        let idents: Vec<String> = item
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == bhdl_parser::SyntaxKind::IDENT)
            .map(|t| t.text().to_string())
            .collect();
        let Some(req) = idents.first().cloned() else { continue };
        let via: Vec<String> = idents.into_iter().skip(1).collect();
        if !ids.contains(&req) {
            m.findings.push(format!("satisfies {req}: names no requirement in this build (known ids: {})", ids.iter().cloned().collect::<Vec<_>>().join(", ")));
            continue;
        }
        for el in via {
            if !inst_names.contains(&el) {
                m.findings.push(format!("satisfies {req}: via '{el}' is not an instance on this board"));
                continue;
            }
            links.entry(req.clone()).or_default().push(format!("{el} (declared)"));
        }
    }
    for r in &mut m.rows {
        if let Some(l) = links.get(&r.id) {
            r.implemented_by.extend(l.iter().cloned());
        }
    }

    m
}

/// Markdown rendering for the CLI.
pub fn render_markdown(m: &TraceMatrix) -> String {
    let mut s = String::new();
    s.push_str(&format!("## Requirement trace matrix — {}\n\n", m.board));
    s.push_str(&format!(
        "{} requirement(s): {} verified, {} violated, {} unverified (no verifier ran — findings), {} unresolved\n\n",
        m.rows.len(),
        m.count(TraceStatus::Verified),
        m.count(TraceStatus::Violated),
        m.count(TraceStatus::Unverified),
        m.count(TraceStatus::Unresolved)
    ));
    s.push_str("| Id | Kind | Stated by | Requirement | Implemented by | Verifier | Status | Evidence |\n|---|---|---|---|---|---|---|---|\n");
    for r in &m.rows {
        let cell = |t: &str| t.replace('|', "\\|").replace('\n', " ");
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
            cell(&r.id),
            cell(&r.kind),
            cell(&r.stated_by),
            cell(&r.statement),
            cell(&if r.implemented_by.is_empty() { "—".to_string() } else { r.implemented_by.join(", ") }),
            cell(&r.verifier),
            r.status.as_str(),
            cell(&r.evidence)
        ));
    }
    if !m.findings.is_empty() {
        s.push_str("\n### Findings\n\n");
        for f in &m.findings {
            s.push_str(&format!("- {f}\n"));
        }
    }
    s
}
