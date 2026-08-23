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
    // true when `safety` is a campaign model the caller loaded (the
    // evidence was sought), false when it is just the resolved model
    safety_supplied: bool,
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
        // DERIVED ASIL: the safety goals whose effects reference the rail
        // this stage drives set the ASIL the part must be capable of —
        // machine-derived, compared with what the requirement states and
        // what the block declares
        let driven_rails: Vec<String> = netlist
            .pin_instances
            .values()
            .filter(|pi| pi.instance == *netlist.instances.iter().find(|(_, i)| i.name == inst.name).map(|(id, _)| id).as_ref().unwrap())
            .filter(|pi| netlist.pins.get(pi.pin_def).map(|p| p.name == "VOUT").unwrap_or(false))
            .filter_map(|pi| pi.net.and_then(|n| netlist.nets.get(n)).and_then(|n| n.name.clone()))
            .collect();
        let derived_asil: Option<(bhdl_common::safety::Level, String)> = safety.and_then(|sm| {
            sm.scopes
                .iter()
                .flat_map(|sc| sc.goals.iter())
                .filter(|g| g.effects.iter().any(|e| driven_rails.iter().any(|r| e.expr.contains(r.as_str()) || e.refs.iter().any(|x| x.ends_with(r.as_str())))))
                .map(|g| (g.level, g.path.clone()))
                .max_by_key(|(l, _)| *l)
        });
        let asil_note = derived_asil.as_ref().map(|(lvl, goal)| {
            let stated = req.split(',').filter_map(|kv| kv.split_once('=')).find(|(k, _)| k.trim() == "asil").map(|(_, v)| v.trim().to_string());
            let capable = inst.attributes.get("asil_capable").cloned();
            let short = lvl.as_str().trim_start_matches("ASIL_").to_string();
            format!(
                "derived ASIL {short} (serves goal {goal}); requirement states {}; block declares asil_capable {}",
                stated.unwrap_or_else(|| format!("NONE — add `asil={short}` to the requirement (or a board-level `requirements {{ asil: {short}; }}`) so the resolver filters on it")),
                capable.unwrap_or_else(|| "NONE".into())
            )
        });
        let (status, evidence) = if bound.is_empty() {
            (TraceStatus::Unresolved, "no library block covers the requirement (see the ⚙ survey near-misses); Generic* placeholder emitted".to_string())
        } else if let Some(v) = erc032.iter().find(|v| matches!(v.severity, ViolationSeverity::Error | ViolationSeverity::Critical)) {
            (TraceStatus::Violated, format!("ERC032: {}", v.description))
        } else if let Some(v) = erc032.iter().find(|v| v.description.contains("UNCHECKED")) {
            (TraceStatus::Unverified, format!("ERC032: {}", v.description))
        } else {
            (TraceStatus::Verified, format!("bound by {basis}; block envelope + promises accepted at resolution; ERC032 clean on the flattened circuit"))
        };
        // a derived ASIL the requirement does not state is a FINDING: the
        // safety analysis put a requirement on this stage that the
        // resolver never saw
        let (status, evidence) = match &asil_note {
            Some(n) if !req.contains("asil=") && status == TraceStatus::Verified => (TraceStatus::Unverified, format!("{evidence}; {n}")),
            Some(n) => (status, format!("{evidence}; {n}")),
            None => (status, evidence),
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
                // gaps are attributed to `<goal path>` or `<goal path>.<effect>`
                let gaps: Vec<&bhdl_common::safety::Gap> = sm
                    .gaps
                    .iter()
                    .filter(|gp| gp.goal == g.path || gp.goal.starts_with(&format!("{}.", g.path)))
                    .collect();
                // a gap that means "the verifier could not run" (fault not
                // run, FIT uncomputed, data missing, assumption open) is
                // UNVERIFIED; one that means "the goal is not met"
                // (undetected effect, PSM without LSM, unsourced DC) is
                // VIOLATED
                // a `FaultUnrun` gap whose fault record shows it DID run
                // with a failed expectation or FTTI is a violation too
                // (the campaign keeps the class, the record has the truth)
                let fault_failed = |gp: &bhdl_common::safety::Gap| -> bool {
                    gp.class == bhdl_common::safety::GapClass::FaultUnrun
                        && sm.scopes.iter().flat_map(|sc| sc.faults.iter()).any(|f| {
                            format!("{}({})", f.kind, f.targets.join(",")) == gp.subject
                                && f.run
                                && (f.expectation_met == Some(false) || f.timing_met == Some(false))
                        })
                };
                let is_violation = |gp: &bhdl_common::safety::Gap| {
                    matches!(
                        gp.class,
                        bhdl_common::safety::GapClass::EffectUndetected
                            | bhdl_common::safety::GapClass::PsmWithoutLsm
                            | bhdl_common::safety::GapClass::DcUnsourced
                            | bhdl_common::safety::GapClass::AouViolated
                            | bhdl_common::safety::GapClass::MetricMissed
                    ) || fault_failed(gp)
                };
                // measured evidence from the campaign model: per-mechanism
                // measured DC and the goal's fault expectations
                let measured: Vec<String> = scope
                    .mechanisms
                    .iter()
                    .filter(|mm| mm.goal == g.path)
                    .map(|mm| match mm.measured_dc {
                        Some(dc) => format!("{} measured DC {:.1}%{}", mm.instance, dc * 100.0, mm.claimed_dc.map(|c| format!(" (claimed {:.1}%)", c * 100.0)).unwrap_or_default()),
                        None => format!("{} DC not measured", mm.instance),
                    })
                    .collect();
                let universe_for_goal: Vec<&bhdl_common::safety::UniverseFault> = sm
                    .universe
                    .iter()
                    .filter(|u| u.scope == scope.path || u.scope == g.path.rsplit_once('.').map(|(s, _)| s).unwrap_or(""))
                    .collect();
                let gap_text = |v: &[&bhdl_common::safety::Gap]| v.iter().map(|gp| format!("{:?}: {} — {}", gp.class, gp.subject, gp.fix)).collect::<Vec<_>>().join("; ");
                let violations: Vec<&bhdl_common::safety::Gap> = gaps.iter().copied().filter(|gp| is_violation(gp)).collect();
                let unverifiable: Vec<&bhdl_common::safety::Gap> = gaps.iter().copied().filter(|gp| !is_violation(gp)).collect();
                let (status, evidence) = if !violations.is_empty() {
                    (TraceStatus::Violated, gap_text(&violations))
                } else if !unverifiable.is_empty() {
                    (TraceStatus::Unverified, format!("campaign evidence incomplete — {}", gap_text(&unverifiable)))
                } else if !campaign_ran {
                    (
                        TraceStatus::Unverified,
                        if safety_supplied {
                            "safety model supplied but its fault universe did not run (no converging DC solve) — no measured DC / FTTI evidence".into()
                        } else {
                            "fault campaign not part of this build — run `bhdl safety --json m.json`, then `bhdl trace --safety m.json` for the measured DC / FTTI evidence".into()
                        },
                    )
                } else if mechs.is_empty() {
                    (TraceStatus::Unresolved, "goal declares no mechanism".into())
                } else {
                    let ran = universe_for_goal.iter().filter(|u| u.ran).count();
                    let detected = universe_for_goal.iter().filter(|u| u.ran && !u.detected.is_empty()).count();
                    (
                        TraceStatus::Verified,
                        format!(
                            "fault campaign: no gap against this goal; {}; universe faults in scope: {ran} run, {detected} detected",
                            measured.join(", ")
                        ),
                    )
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

    // safety gaps attributed to no goal row (board-level FIT data, parts
    // without safety data): findings, not silence
    if let Some(sm) = safety {
        let goal_paths: Vec<String> = sm.scopes.iter().flat_map(|s| s.goals.iter().map(|g| g.path.clone())).collect();
        for gp in &sm.gaps {
            let attributed = goal_paths.iter().any(|p| gp.goal == *p || gp.goal.starts_with(&format!("{p}.")));
            if !attributed {
                m.findings.push(format!("safety gap (no goal row): {:?}: {} — {}", gp.class, gp.subject, gp.fix));
            }
        }
    }

    // ── 5b. hardware–software interface contracts `hsi NAME { … }` ──
    // Machine-checkable parts are checked on the netlist: the MCU pin and
    // the hardware source share a net (wiring), the pin's declared
    // direction agrees with the software view, and the source's supply
    // rail matches the declared logic level. `latency_max` has NO
    // verifier in this build and is stated UNVERIFIED, never a pass.
    for hsi in sf.syntax().descendants().filter(|n| n.kind() == bhdl_parser::SyntaxKind::HSI_STMT) {
        // `hsi` itself lexes as an IDENT (contextual keyword): the id is
        // the second IDENT token
        let id = hsi
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == bhdl_parser::SyntaxKind::IDENT)
            .nth(1)
            .map(|t| t.text().to_string())
            .unwrap_or_default();
        let mut kv: BTreeMap<String, String> = BTreeMap::new();
        for e in hsi.children().filter(|n| n.kind() == bhdl_parser::SyntaxKind::HSI_ENTRY) {
            let toks: Vec<_> = e.descendants_with_tokens().filter_map(|x| x.into_token()).filter(|t| t.kind() != bhdl_parser::SyntaxKind::WHITESPACE && t.kind() != bhdl_parser::SyntaxKind::COMMENT).collect();
            let Some(key) = toks.first().map(|t| t.text().to_string()) else { continue };
            let value: String = toks
                .iter()
                .skip(1)
                .skip_while(|t| t.kind() == bhdl_parser::SyntaxKind::COLON)
                .take_while(|t| t.kind() != bhdl_parser::SyntaxKind::SEMI)
                .map(|t| t.text().to_string())
                .collect::<Vec<_>>()
                .join("")
                .trim_matches('"')
                .to_string();
            kv.insert(key, value);
        }
        let get = |k: &str| kv.get(k).cloned().filter(|v| !v.is_empty());
        let pin_net_of = |spec: &str| -> Option<(String, String, bhdl_netlist::types::PinDirection, Option<bhdl_netlist::types::NetId>)> {
            let (inst, pin) = spec.split_once('.')?;
            let i = netlist.instances.iter().find(|(_, i)| i.name == inst)?;
            let pi = netlist.pin_instances.values().find(|pi| {
                pi.instance == i.0 && netlist.pins.get(pi.pin_def).map(|p| p.name == pin).unwrap_or(false)
            })?;
            let p = netlist.pins.get(pi.pin_def)?;
            Some((inst.to_string(), pin.to_string(), p.direction.clone(), pi.net))
        };
        let mut checks: Vec<(String, bool, bool)> = Vec::new(); // (detail, ok, unchecked)
        let signal = get("signal");
        let source = get("source");
        let sig = signal.as_deref().and_then(pin_net_of);
        let src = source.as_deref().and_then(pin_net_of);
        let sig_inst = sig.as_ref().and_then(|g| netlist.instances.iter().find(|(_, i)| i.name == g.0).map(|(id, _)| id));
        match (&signal, &sig) {
            (Some(sp), None) => checks.push((format!("signal {sp} is not a pin on this board"), false, false)),
            (None, _) => checks.push(("no `signal:` pin declared".into(), false, false)),
            _ => {}
        }
        if let (Some(sp), Some(s)) = (&source, &src) {
            match &sig {
                Some(g) => {
                    let wired = g.3.is_some() && g.3 == s.3;
                    checks.push((format!("wiring: {sp} and {} share a net", signal.clone().unwrap_or_default()), wired, false));
                }
                None => {}
            }
            let _ = s;
        } else if let Some(sp) = &source {
            checks.push((format!("source {sp} is not a pin on this board"), false, false));
        }
        if let (Some(dir), Some(g)) = (get("direction"), &sig) {
            use bhdl_netlist::types::PinDirection as D;
            let ok = match dir.as_str() {
                "input" => matches!(g.2, D::In | D::InOut | D::Passive),
                "output" => matches!(g.2, D::Out | D::InOut | D::Passive),
                "inout" => matches!(g.2, D::InOut | D::Passive),
                _ => false,
            };
            checks.push((format!("direction: software sees {dir}; pin {}.{} is declared {:?}", g.0, g.1, g.2), ok, false));
        }
        if let Some(level) = get("level") {
            let want = crate::stage_acceptance::parse_si(&level);
            // the supply rail of the part that DRIVES the source net sets
            // the logic level: walk the net to its output pin(s) (the
            // declared source may be a composite's virtual pin), then to
            // that part's power pin's rail
            // (net CLASSES, not pin types: expansion-minted child modules
            // carry generic pin types, but a rail net is a Power net)
            let supply_of = |inst: bhdl_netlist::types::InstanceId| -> Option<f64> {
                netlist.pin_instances.values().find_map(|pi| {
                    if pi.instance != inst { return None; }
                    match &netlist.nets.get(pi.net?)?.net_class {
                        bhdl_netlist::types::NetClass::Power { voltage, .. } => Some(*voltage),
                        _ => None,
                    }
                })
            };
            let rail_v = src.as_ref().and_then(|s| {
                let net = s.3?;
                netlist
                    .pin_instances
                    .values()
                    .filter(|pi| pi.net == Some(net) && Some(pi.instance) != sig_inst)
                    .filter(|pi| netlist.pins.get(pi.pin_def).map(|p| !p.is_virtual).unwrap_or(false))
                    .find_map(|pi| supply_of(pi.instance))
            });
            match (want, rail_v) {
                (Some(w), Some(v)) => checks.push((format!("level: source supply rail {v:.2}V vs declared {w:.2}V"), (v - w).abs() <= 0.05 * w.max(0.1), false)),
                (Some(_), None) => checks.push(("level: source's supply rail not resolvable on this board — UNCHECKED".into(), false, true)),
                (None, _) => checks.push((format!("level `{level}` is not a voltage"), false, false)),
            }
        }
        // latency_max: the HARDWARE share is derived — the driver's
        // declared response latency (a safety mechanism's `latency=`, or
        // the driving part's `latency` / `propagation_delay` attribute)
        // plus the signal net's RC settling (pull-up R × node C, 2.2·τ
        // for a 10–90 % edge) from the netlist. The FIRMWARE share cannot
        // be measured here: the contract may declare it as `fw_latency`
        // (a stated term the software side owns). hw + fw ≤ latency_max.
        if let Some(l) = get("latency_max") {
            let budget = crate::stage_acceptance::parse_si(&l);
            // driver: the non-signal instance on the source net
            let driver_inst = src.as_ref().and_then(|s| {
                let net = s.3?;
                netlist
                    .pin_instances
                    .values()
                    .filter(|pi| pi.net == Some(net) && Some(pi.instance) != sig_inst)
                    .filter(|pi| netlist.pins.get(pi.pin_def).map(|p| !p.is_virtual).unwrap_or(false))
                    .map(|pi| pi.instance)
                    .next()
            });
            let driver_name = driver_inst.and_then(|id| netlist.instances.get(id)).map(|i| i.name.clone());
            let declared_latency: Option<(f64, String)> = driver_name.as_ref().and_then(|dn| {
                // safety mechanism on that instance
                safety
                    .and_then(|sm| {
                        sm.scopes.iter().flat_map(|sc| sc.mechanisms.iter()).find(|mm| &mm.instance == dn).and_then(|mm| {
                            mm.latency.as_deref().and_then(crate::stage_acceptance::parse_si).map(|v| (v, format!("mechanism {dn} latency={}", mm.latency.clone().unwrap_or_default())))
                        })
                    })
                    .or_else(|| {
                        let i = netlist.instances.iter().find(|(_, i)| &i.name == dn)?.1;
                        ["latency", "propagation_delay", "t_prop", "response_time"]
                            .iter()
                            .find_map(|k| i.attributes.get(*k).and_then(|v| crate::stage_acceptance::parse_si(v)).map(|x| (x, format!("{dn}.{k}"))))
                    })
            });
            // RC on the signal net: pull-up R (resistor with its other pin on a
            // Power net) × sum of capacitors on the net
            let rc = sig.as_ref().and_then(|g| g.3).map(|net| {
                let on_net: Vec<_> = netlist.pin_instances.values().filter(|pi| pi.net == Some(net)).collect();
                let mut r_pu: Option<f64> = None;
                let mut c_sum = 0.0;
                let mut c_n = 0usize;
                for pi in &on_net {
                    let Some(i) = netlist.instances.get(pi.instance) else { continue };
                    let class = i.attributes.get("component_class").map(String::as_str).unwrap_or("");
                    let val = i.attributes.get("value").and_then(|v| crate::stage_acceptance::parse_si(v));
                    match class {
                        "resistor" => {
                            // other pin on a Power net → pull-up
                            let to_rail = netlist.pin_instances.values().any(|o| {
                                o.instance == pi.instance
                                    && o.net != Some(net)
                                    && o.net.and_then(|n| netlist.nets.get(n)).map(|n| matches!(n.net_class, bhdl_netlist::types::NetClass::Power { .. })).unwrap_or(false)
                            });
                            if to_rail {
                                if let Some(v) = val { r_pu = Some(r_pu.map_or(v, |r: f64| r.min(v))); }
                            }
                        }
                        "capacitor" => {
                            if let Some(v) = val { c_sum += v; c_n += 1; }
                        }
                        _ => {}
                    }
                }
                (r_pu, c_sum, c_n)
            });
            let fw = get("fw_latency").map(|v| (v.clone(), crate::stage_acceptance::parse_si(&v)));
            match (budget, &declared_latency) {
                (None, _) => checks.push((format!("latency_max `{l}` is not a time"), false, false)),
                (Some(_), None) => checks.push((
                    format!(
                        "latency_max {l}: driver {} declares no response latency (mechanism `latency=` or a `latency`/`propagation_delay` attribute) — hardware share UNCHECKED, not a pass",
                        driver_name.clone().unwrap_or_else(|| "?".into())
                    ),
                    false,
                    true,
                )),
                (Some(b), Some((lat, lat_src))) => {
                    let (edge, rc_note) = match rc {
                        Some((Some(r), c, n)) if c > 0.0 => (2.2 * r * c, format!("RC edge 2.2·({r:.0}Ω×{:.1}nF from {n} cap(s)) = {:.1}µs", c * 1e9, 2.2 * r * c * 1e6)),
                        Some((Some(r), _, _)) => (0.0, format!("pull-up {r:.0}Ω, no capacitor on the net — RC term 0")),
                        _ => (0.0, "no pull-up/RC on the net — RC term 0".into()),
                    };
                    let hw = lat + edge;
                    match fw {
                        Some((txt, Some(f))) => {
                            let total = hw + f;
                            checks.push((
                                format!(
                                    "latency: hw {:.3}ms ({lat_src} {:.3}ms + {rc_note}) + fw {txt} (declared contract term, not measured) = {:.3}ms ≤ {l}",
                                    hw * 1e3, lat * 1e3, total * 1e3
                                ),
                                total <= b + 1e-12,
                                false,
                            ));
                        }
                        Some((txt, None)) => checks.push((format!("fw_latency `{txt}` is not a time"), false, false)),
                        None => checks.push((
                            format!(
                                "latency: hw {:.3}ms ({lat_src} {:.3}ms + {rc_note}) ≤ {l}; no fw_latency declared — the firmware share is the software side's unstated budget (hardware share {})",
                                hw * 1e3, lat * 1e3, if hw <= b { "fits" } else { "ALONE exceeds the budget" }
                            ),
                            hw <= b + 1e-12,
                            false,
                        )),
                    }
                }
            }
        }
        let status = if checks.iter().any(|c| !c.1 && !c.2) {
            TraceStatus::Violated
        } else if checks.iter().any(|c| c.2) {
            TraceStatus::Unverified
        } else {
            TraceStatus::Verified
        };
        let mut implemented: Vec<String> = Vec::new();
        if let Some(s) = &source { implemented.push(format!("hw: {s}")); }
        if let Some(o) = get("owner") { implemented.push(format!("fw: {o}")); }
        m.rows.push(TraceRow {
            id: id.clone(),
            kind: "hardware–software interface".into(),
            stated_by: "designer (HSI)".into(),
            statement: kv.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join("; "),
            implemented_by: implemented,
            verifier: "netlist: wiring + pin direction + source supply level + hw latency (driver latency + RC edge) vs latency_max".into(),
            status,
            evidence: checks.iter().map(|c| format!("{} {}", if c.1 { "ok" } else if c.2 { "UNCHECKED" } else { "NOK" }, c.0)).collect::<Vec<_>>().join("; "),
        });
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
