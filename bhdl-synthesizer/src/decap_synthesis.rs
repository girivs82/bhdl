//! Decap-network synthesis from a domain's Z(f) target-impedance mask
//! (arc (c) of the PDN plan; docs/spec/Functional_Safety.md §2.10).
//!
//! `decouple <inst>.<domain> from "<lib.bhdl>" [max_parts=N] [margin=1]
//! [bulk_over=10uF] [z_margin=20%];`
//!
//! The library is a bhdl file of REAL capacitors — entities whose
//! attributes declare capacitance, esr and esl from the datasheet
//! (Real-Data: a cap without declared ESR/ESL cannot place its
//! anti-resonances and is skipped, stated). Selection is greedy and
//! physics-driven: each round, every candidate is TRIALLED by actually
//! adding it and re-running the AC impedance sweep of the whole tree;
//! the one that most reduces the worst |Z|/mask ratio is committed.
//! Infeasibility is a HARD ERROR naming the physics (worst frequency,
//! achieved |Z|, mask, and whether the declared PDN budget alone
//! already exceeds the mask — no cap can fix that).
//!
//! Margin policy (design margin + safety open-fault robustness): after
//! the mask is met, ONE extra cap per distinct chosen value is added —
//! except bulk caps (value > bulk_over, default 10µF). The margin is
//! then VERIFIED, not assumed: every non-bulk chosen cap is opened one
//! at a time and the sweep must still meet the mask; a single-open
//! that violates is a hard error (the margin failed its purpose).
//!
//! Layout headroom (z_margin, default 20%): this sweep does not model
//! layout parasitics — per-cap mounting inductance (vias, pad
//! escapes) or plane spreading — which SYSTEMATICALLY worsen every
//! cap. Extra redundancy (N+2) would not model that; headroom does.
//! The whole mask is tightened to mask/(1+z_margin) once, up front,
//! so selection, the budget floor and the single-open verification
//! all carry the same layout allowance.

use crate::safety_model::entity_domain_map;
use bhdl_ast::SourceFile;
use bhdl_netlist::types::{ModuleKind, NetClass, NetId, PinDirection, PinType};
use bhdl_netlist::Netlist;
use bhdl_parser::SyntaxKind;
use log::info;
use rowan::ast::AstNode;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DecoupleStmt {
    pub instance: String,
    pub domain: String,
    pub lib: String,
    pub max_parts: usize,
    /// extra caps per distinct chosen value (0 disables)
    pub margin: usize,
    /// values strictly above this (farads) are bulk: exempt from margin
    pub bulk_over_f: f64,
    /// mask derating headroom in percent (default 20): the whole mask
    /// is tightened to mask/(1+z_margin) before ANY check — selection,
    /// budget floor, and single-open verification alike — to absorb
    /// unmodeled layout parasitics (per-cap mounting inductance, plane
    /// spreading). Layout effects are SYSTEMATIC, so headroom models
    /// them where extra redundancy (N+2) would not.
    pub z_margin_pct: f64,
}

#[derive(Debug, Clone)]
struct Candidate {
    entity: String,
    c_f: f64,
    esr_ohm: f64,
    esl_h: f64,
    c_text: String,
    /// Rated DC working voltage (V) — REQUIRED library data: an
    /// undeclared rating is not infinite, so a candidate without one
    /// is skipped with a stated reason, exactly like missing ESR/ESL.
    v_rating: f64,
    /// RMS ripple-current rating (A) — OPTIONAL library data (many
    /// MLCC datasheets publish a thermal model instead of a rating);
    /// absent = the sign-off row for a cap carrying computed ripple
    /// reports NoData/UNCHECKED, never a pass.
    ripple_a: Option<f64>,
    /// DC-bias derating curve (V, F) breakpoints — the vendor tool's
    /// export, declared per part; empty = no curve (nominal used,
    /// stated). Effective C at the rail voltage = linear interpolation.
    dc_bias: Vec<(f64, f64)>,
}

/// Effective capacitance at `v` volts from a (V, F) curve — linear
/// interpolation, clamped to the end points. Empty curve = nominal.
pub fn c_effective_at(nominal: f64, curve: &[(f64, f64)], v: f64) -> f64 {
    if curve.is_empty() {
        return nominal;
    }
    let mut pts: Vec<(f64, f64)> = curve.to_vec();
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    if v <= pts[0].0 {
        return pts[0].1;
    }
    if v >= pts[pts.len() - 1].0 {
        return pts[pts.len() - 1].1;
    }
    for w in pts.windows(2) {
        if v >= w[0].0 && v <= w[1].0 {
            let t = (v - w[0].0) / (w[1].0 - w[0].0).max(1e-12);
            return w[0].1 + t * (w[1].1 - w[0].1);
        }
    }
    nominal
}

/// Parse a "0V:22µF,5V:12µF" curve string.
pub fn parse_dc_bias(txt: &str) -> Vec<(f64, f64)> {
    txt.split(',')
        .filter_map(|e| {
            let (v, c) = e.trim().split_once(':')?;
            Some((
                parse_unit(v.trim(), &[("mV", 1e-3), ("V", 1.0)])?,
                parse_farads(c.trim())?,
            ))
        })
        .collect()
}

/// Walk the board for `decouple` statements. Token soup:
/// decouple <inst> . <dom> from "<lib>" [k=v ...] ;
pub fn parse_decouple_stmts(sf: &SourceFile) -> Result<Vec<DecoupleStmt>, String> {
    let mut out = Vec::new();
    for node in sf
        .syntax()
        .descendants()
        .filter(|n| n.kind() == SyntaxKind::DECOUPLE_STMT)
    {
        let toks: Vec<String> = node
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| {
                !matches!(t.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT | SyntaxKind::SEMI)
            })
            .map(|t| t.text().to_string())
            .collect();
        // ["decouple", inst, ".", dom, "from", "\"lib\"", k, "=", v, ...]
        let bad = |why: &str| format!("decouple statement `{}`: {}", toks.join(" "), why);
        if toks.len() < 6 || toks[0] != "decouple" || toks[2] != "." || toks[4] != "from" {
            return Err(bad("expected `decouple <inst>.<domain> from \"<lib>\" ...;`"));
        }
        let lib = toks[5].trim_matches('"').to_string();
        let mut stmt = DecoupleStmt {
            instance: toks[1].clone(),
            domain: toks[3].clone(),
            lib,
            max_parts: 12,
            margin: 1,
            bulk_over_f: 10e-6,
            z_margin_pct: 20.0,
        };
        let mut i = 6;
        while i + 2 < toks.len() + 1 {
            if i + 2 > toks.len() {
                break;
            }
            if toks.get(i + 1).map(String::as_str) != Some("=") {
                return Err(bad(&format!("expected k=v, found '{}'", toks[i])));
            }
            // the lexer may split "10uF" into "10" "uF" — rejoin.
            // NB: consume via `step`, never by bumping `i` here — the
            // key match below still reads toks[i].
            let mut v = toks[i + 2].clone();
            let mut step = 3;
            if i + 3 < toks.len()
                && toks.get(i + 3).map(String::as_str) != Some("=")
                && toks[i + 3].chars().all(|c| c.is_alphabetic() || c == 'µ')
                && toks.get(i + 4).map(String::as_str) != Some("=")
            {
                v.push_str(&toks[i + 3]);
                step = 4;
            }
            match toks[i].as_str() {
                "max_parts" => {
                    stmt.max_parts = v
                        .parse()
                        .map_err(|_| bad(&format!("max_parts '{v}' is not a number")))?
                }
                "margin" => {
                    stmt.margin = v
                        .parse()
                        .map_err(|_| bad(&format!("margin '{v}' is not a number")))?
                }
                "bulk_over" => {
                    stmt.bulk_over_f = parse_farads(&v)
                        .ok_or_else(|| bad(&format!("bulk_over '{v}' is not a capacitance")))?
                }
                "z_margin" => {
                    stmt.z_margin_pct = v
                        .trim_end_matches('%')
                        .parse()
                        .map_err(|_| bad(&format!("z_margin '{v}' is not a percentage")))?
                }
                other => return Err(bad(&format!("unknown parameter '{other}'"))),
            }
            i += step;
            // a trailing lexer-split unit token ("35" "%") that the
            // rejoin above didn't fold is consumed by the value parse —
            // skip it if it is not the next key
            if i < toks.len()
                && toks.get(i + 1).map(String::as_str) != Some("=")
                && toks[i].chars().all(|c| c == '%')
            {
                i += 1;
            }
        }
        out.push(stmt);
    }
    Ok(out)
}

fn parse_farads(v: &str) -> Option<f64> {
    parse_unit(v, &[("pF", 1e-12), ("nF", 1e-9), ("uF", 1e-6), ("µF", 1e-6), ("mF", 1e-3), ("F", 1.0)])
}

fn parse_unit(v: &str, mults: &[(&str, f64)]) -> Option<f64> {
    let v = v.trim();
    let end = v
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(v.len());
    let num: f64 = v[..end].parse().ok()?;
    let suf = v[end..].trim();
    if suf.is_empty() {
        return Some(num);
    }
    mults
        .iter()
        .find(|(s, _)| suf.eq_ignore_ascii_case(s) || suf == *s)
        .map(|(_, m)| num * m)
}

/// Load the candidate library: entities with declared capacitance, esr
/// and esl attributes. Entities missing any of the three are skipped
/// with a stated note (Real-Data: an undeclared parasite is not zero).

/// The SHORTLIST's bulk part (spec §7.5 addendum 4): the
/// largest-capacitance characterized candidate in the project's decap
/// library — what the --emit bulk fixpoint stacks instead of a bare
/// farad value, so every capacitor on the rail is a shortlisted,
/// characterized, orderable part. Returns (entity, nominal F,
/// dc_bias curve).
/// The shortlist's bulk candidates, LARGEST capacitance first, each
/// carrying its rated voltage: (entity, c_f, v_rating, dc_bias). The
/// caller picks per RAIL — the largest candidate whose rating covers
/// that rail's voltage (times the project derating policy) — because
/// one global pick cannot be voltage-safe on every rail.
pub fn bulk_parts_from_library(lib_path: &str) -> Vec<(String, f64, f64, Vec<(f64, f64)>)> {
    let Ok((mut cands, _skipped)) = load_library(lib_path) else { return Vec::new() };
    cands.sort_by(|a, b| b.c_f.partial_cmp(&a.c_f).unwrap());
    cands.into_iter().map(|c| (c.entity, c.c_f, c.v_rating, c.dc_bias)).collect()
}

/// One candidate's declared ESR by entity name — the EMI filter's
/// damping math needs the filter cap's real parasitic.
pub fn bulk_candidate_esr(lib_path: &str, entity: &str) -> Option<f64> {
    let (cands, _) = load_library(lib_path).ok()?;
    cands.into_iter().find(|c| c.entity == entity).map(|c| c.esr_ohm)
}

fn load_library(lib_path: &str) -> Result<(Vec<Candidate>, Vec<String>), String> {
    let resolved = bhdl_common::import_search::resolve_relative(lib_path, std::path::Path::new("."));
    let text = std::fs::read_to_string(&resolved)
        .map_err(|e| format!("decouple: cannot read library '{}': {e}", resolved.display()))?;
    let pr = bhdl_parser::parse(&text);
    if !pr.errors().is_empty() {
        return Err(format!("decouple: library '{lib_path}' has parse errors: {:?}", pr.errors()));
    }
    let sf = SourceFile::cast(pr.syntax()).ok_or("decouple: library is not a source file")?;
    let mut out = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for item in sf.items() {
        let Some(ent) = bhdl_ast::Entity::cast(item.syntax().clone()) else { continue };
        use bhdl_ast::HasName;
        let Some(name) = ent.name().map(|t| t.text().to_string()) else { continue };
        let mut attrs: HashMap<String, String> = HashMap::new();
        for a in ent.attributes() {
            if let (Some(k), Some(v)) = (a.name(), a.value()) {
                attrs.insert(k.text().to_string(), v.syntax().text().to_string().trim().trim_matches('"').to_string());
            }
        }
        let (Some(c_text), Some(esr_text), Some(esl_text)) = (
            attrs.get("capacitance").cloned(),
            attrs.get("esr").cloned(),
            attrs.get("esl").cloned(),
        ) else {
            info!("decouple: library entity '{name}' skipped — needs declared capacitance, esr AND esl (Real-Data: an undeclared parasite is not zero)");
            skipped.push(format!("{name}: capacitance/esr/esl not all declared"));
            continue;
        };
        let Some(c_f) = parse_farads(&c_text) else {
            info!("decouple: library entity '{name}' skipped — capacitance '{c_text}' unparseable");
            skipped.push(format!("{name}: capacitance '{c_text}' unparseable"));
            continue;
        };
        let Some(esr_ohm) = parse_unit(&esr_text, &[("uΩ", 1e-6), ("mΩ", 1e-3), ("Ω", 1.0), ("mohm", 1e-3), ("ohm", 1.0)]) else {
            info!("decouple: library entity '{name}' skipped — esr '{esr_text}' unparseable");
            skipped.push(format!("{name}: esr '{esr_text}' unparseable"));
            continue;
        };
        let Some(esl_h) = parse_unit(&esl_text, &[("pH", 1e-12), ("nH", 1e-9), ("uH", 1e-6), ("µH", 1e-6), ("H", 1.0)]) else {
            info!("decouple: library entity '{name}' skipped — esl '{esl_text}' unparseable");
            skipped.push(format!("{name}: esl '{esl_text}' unparseable"));
            continue;
        };
        // rated DC working voltage — REQUIRED: an undeclared rating is
        // not infinite (the same Real-Data stance as ESR/ESL)
        let Some(vr_text) = attrs.get("voltage_rating").cloned() else {
            info!("decouple: library entity '{name}' skipped — no voltage_rating declared (an undeclared rating is not infinite)");
            skipped.push(format!("{name}: voltage_rating not declared"));
            continue;
        };
        let Some(v_rating) = parse_unit(&vr_text, &[("mV", 1e-3), ("V", 1.0)]) else {
            info!("decouple: library entity '{name}' skipped — voltage_rating '{vr_text}' unparseable");
            skipped.push(format!("{name}: voltage_rating '{vr_text}' unparseable"));
            continue;
        };
        // optional per-part DC-bias curve (the vendor tool's export);
        // absence = nominal, stated in the synthesis notes
        let dc_bias = attrs.get("dc_bias").map(|t| parse_dc_bias(t)).unwrap_or_default();
        // OPTIONAL RMS ripple-current rating (vendors often publish a
        // thermal model instead — absent is a stated gap at sign-off)
        let ripple_a = attrs
            .get("ripple_current")
            .and_then(|t| parse_unit(t, &[("mA", 1e-3), ("A", 1.0)]));
        out.push(Candidate { entity: name, c_f, esr_ohm, esl_h, c_text, v_rating, ripple_a, dc_bias });
    }
    if out.is_empty() {
        return Err(format!("decouple: library '{lib_path}' has no usable candidates (capacitance+esr+esl declared)"));
    }
    Ok((out, skipped))
}

/// Characterize BLOCK-INTERNAL generic power caps from the project's
/// shortlist (spec §7.5 addendum 6). A design block's application
/// circuit instantiates bare `Cap(<farads>)` children — the
/// datasheet's REQUIRED MINIMUM, with no ESR/ESL — so the decap
/// verification swept them as IDEAL and the final sanity reported
/// their anti-resonances RESONANCE UNCHECKED. With a characterized
/// shortlist declared (`requirements { decap_lib: ... }`), the part
/// that will PHYSICALLY be placed is a shortlist part: this pass
/// substitutes, for every still-uncharacterized capacitor sitting
/// rail↔ground on a POWER-class net, the SMALLEST shortlist candidate
/// meeting the declared minimum — stamping its value/esr/esl/dc_bias
/// and a provenance attribute, so every consumer (decap sweep,
/// power-up engine, spice, sanity) sims what will actually be
/// soldered. Signal-net caps (compensation, feed-forward, bootstrap,
/// timing) are NEVER touched — substituting upward there changes the
/// loop, not the reservoir. A minimum no candidate meets stays
/// uncharacterized, with a stated gap.
pub fn characterize_block_caps(
    netlist: &mut Netlist,
    lib_path: &str,
    v_derate: Option<f64>,
) -> Result<Vec<String>, String> {
    let (mut cands, _skipped) = load_library(lib_path)?;
    cands.sort_by(|a, b| a.c_f.partial_cmp(&b.c_f).unwrap());
    let mut pin_net: HashMap<(bhdl_netlist::types::InstanceId, String), NetId> = Default::default();
    for pi in netlist.pin_instances.values() {
        let (Some(net), Some(p)) = (pi.net, netlist.pins.get(pi.pin_def)) else { continue };
        pin_net.insert((pi.instance, p.name.clone()), net);
    }
    // collect targets first (mutation below)
    struct Target {
        id: bhdl_netlist::types::InstanceId,
        name: String,
        c_min: f64,
        rail_v: f64,
    }
    let mut targets: Vec<Target> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    for (i, inst) in netlist.instances.iter() {
        let module = netlist
            .modules
            .get(inst.definition)
            .map(|m| m.name.clone())
            .unwrap_or_default();
        let is_cap = matches!(module.as_str(), "Cap" | "Capacitor")
            || inst.attributes.contains_key("capacitance");
        if !is_cap {
            continue;
        }
        // already characterized (library parts, minted decaps) — skip
        if inst.attributes.contains_key("esr") && inst.attributes.contains_key("esl") {
            continue;
        }
        let Some(v) = inst
            .attributes
            .get("value")
            .or_else(|| inst.attributes.get("capacitance"))
            .and_then(|t| crate::stage_acceptance::parse_si(t))
        else {
            continue;
        };
        // one pin on GROUND, the other on a POWER-class net — the
        // reservoir shape; anything else is a loop/timing element
        let (Some(n1), Some(n2)) = (
            pin_net.get(&(i, "1".to_string())),
            pin_net.get(&(i, "2".to_string())),
        ) else { continue };
        let class = |n: NetId| netlist.nets.get(n).map(|x| x.net_class.clone());
        let rail_v = [(n1, n2), (n2, n1)].iter().find_map(|(a, b)| {
            if !matches!(class(**b), Some(NetClass::Ground)) {
                return None;
            }
            match class(**a) {
                Some(NetClass::Power { voltage, .. }) => Some(voltage),
                _ => None,
            }
        });
        let Some(rail_v) = rail_v else { continue };
        targets.push(Target { id: i, name: inst.name.clone(), c_min: v, rail_v });
    }
    for t in targets {
        // the block's value is the datasheet's minimum requirement;
        // the placed part is the smallest shortlist candidate meeting
        // BOTH the minimum and the rail's voltage (times the project
        // derating policy — undeclared = 100 % of rating, stated)
        let v_req = t.rail_v / v_derate.unwrap_or(1.0);
        let Some(c) = cands
            .iter()
            .find(|c| c.c_f >= t.c_min * (1.0 - 1e-6) && c.v_rating >= v_req - 1e-9)
        else {
            notes.push(format!(
                "characterize {}: no shortlist candidate in '{lib_path}' meets the {:.2}µF application minimum AND a voltage_rating ≥ {:.2}V for the {:.2}V rail{} — stays uncharacterized (RESONANCE gap remains, stated; add a larger/higher-rated part to the shortlist)",
                t.name,
                t.c_min * 1e6,
                v_req,
                t.rail_v,
                match v_derate { Some(d) => format!(" (cap_v_derating {:.0}%)", d * 100.0), None => " (no cap_v_derating declared — 100% of rating, stated)".to_string() }
            ));
            continue;
        };
        let Some(inst) = netlist.instances.get_mut(t.id) else { continue };
        // the PRETTY text for both keys: this is a stdlib Cap whose
        // value flows through the normal unit-aware paths, and the
        // emit round-trip gate compares attribute STRINGS — a raw
        // float here re-emerges from elaborated text as "10µF" and
        // fails the gate
        inst.attributes.insert("value".into(), c.c_text.clone());
        inst.attributes.insert("capacitance".into(), c.c_text.clone());
        inst.attributes.insert("esr".into(), format!("{}", c.esr_ohm));
        inst.attributes.insert("esl".into(), format!("{}", c.esl_h));
        inst.attributes.insert("voltage_rating".into(), format!("{}V", c.v_rating));
        if let Some(r) = c.ripple_a {
            inst.attributes.insert("ripple_current".into(), format!("{r}A"));
        }
        if !c.dc_bias.is_empty() {
            inst.attributes.insert(
                "dc_bias".into(),
                c.dc_bias.iter().map(|(v, f)| format!("{v}V:{f}F")).collect::<Vec<_>>().join(","),
            );
        }
        inst.attributes.insert(
            "cap_resolved".into(),
            format!(
                "{} from {} — smallest shortlist candidate ≥ the {:.2}µF application minimum",
                c.entity,
                lib_path,
                t.c_min * 1e6
            ),
        );
        notes.push(format!(
            "characterize {}: {:.2}µF application minimum → {} ({:.2}µF, esr {:.1}mΩ, esl {:.2}nH, rated {:.1}V ≥ {:.2}V required{}) from the shortlist",
            t.name,
            t.c_min * 1e6,
            c.entity,
            c.c_f * 1e6,
            c.esr_ohm * 1e3,
            c.esl_h * 1e9,
            c.v_rating,
            v_req,
            if c.dc_bias.is_empty() { "" } else { ", DC-bias curve" }
        ));
    }
    Ok(notes)
}

/// Piecewise log-log interpolation of the mask; None outside its span.
fn mask_at(mask: &[(f64, f64)], f: f64) -> Option<f64> {
    if mask.len() < 2 || f < mask[0].0 || f > mask[mask.len() - 1].0 {
        return None;
    }
    let w = mask.windows(2).find(|w| f >= w[0].0 && f <= w[1].0)?;
    let t = (f.ln() - w[0].0.ln()) / (w[1].0.ln() - w[0].0.ln());
    Some((w[0].1.ln() + t * (w[1].1.ln() - w[0].1.ln())).exp())
}

/// Sweep the netlist's rail impedance and return the worst |Z|/mask
/// ratio (plus its frequency and achieved |Z| incl. the PDN budget).
fn worst_ratio(
    netlist: &Netlist,
    rail_net: &str,
    dom: &bhdl_common::safety::PowerDomain,
    overrides: &std::collections::HashMap<String, bhdl_common::model::EvaluatedModel>,
) -> Result<(f64, f64, f64), String> {
    let mut conv = bhdl_spice::NetlistToSpiceConverter::new();
    conv.set_model_overrides(overrides.clone());
    let circ = conv
        .convert(netlist)
        .map_err(|e| format!("decouple: SPICE conversion failed: {e}"))?;
    // the sweep floor follows the mask's lowest breakpoint (the
    // auto-mask can sit below 100 kHz when a stage declares its f_c)
    let f_start = dom
        .zmask
        .iter()
        .map(|(f, _)| *f)
        .fold(100e3_f64, f64::min)
        .max(100.0);
    let (freqs, z) = match bhdl_spice::ac::run_ac_impedance(&circ, rail_net, f_start, 50e6, 20) {
        Ok(fz) => fz,
        // a bare function-first rail (black-box pins only, no caps
        // placed yet) IS an open: infinite baseline, worst at the
        // mask's first breakpoint — the greedy loop takes it from there
        Err(e) if e.to_string().contains("no stamped elements") => {
            let f0 = dom.zmask.first().map(|(f, _)| *f).unwrap_or(f_start);
            return Ok((f64::INFINITY, f0, f64::INFINITY));
        }
        Err(e) => return Err(format!("decouple: impedance sweep failed: {e}")),
    };
    let budget = |f: f64| -> f64 {
        let r = dom.pdn_r_ohm.unwrap_or(0.0);
        let l = dom.pdn_l_h.unwrap_or(0.0);
        (r * r + (2.0 * std::f64::consts::PI * f * l).powi(2)).sqrt()
    };
    let mut worst = (0.0f64, 0.0f64, 0.0f64);
    for (i, &f) in freqs.iter().enumerate() {
        let zt = z[i].norm() + budget(f);
        if let Some(mk) = mask_at(&dom.zmask, f) {
            let r = zt / mk;
            if r > worst.0 {
                worst = (r, f, zt);
            }
        }
    }
    Ok(worst)
}

/// Mint one decap instance of `cand` wired rail→GND. Returns its name.
fn mint_decap(
    netlist: &mut Netlist,
    cand: &Candidate,
    name: &str,
    rail: NetId,
    gnd: NetId,
    stmt: &DecoupleStmt,
    rail_v: f64,
) -> Result<(), String> {
    let mod_id = netlist
        .modules
        .iter()
        .find(|(_, m)| m.name == cand.entity)
        .map(|(id, _)| id)
        .unwrap_or_else(|| netlist.add_module(cand.entity.clone(), ModuleKind::PhysicalComponent));
    // a found-by-name module can be an IMPORT STUB with no pin defs
    // (the entity was imported but nothing instantiated it) — a trial
    // cap minted onto it would have zero pin instances
    for pn in ["1", "2"] {
        let has = netlist.pins.values().any(|p| p.module == mod_id && p.name == pn);
        if !has {
            netlist.add_pin(mod_id, pn.into(), PinDirection::Passive, PinType::Passive);
        }
    }
    let inst_id = netlist
        .add_instance(name.to_string(), mod_id)
        .ok_or("decouple: instance creation failed")?;
    {
        let inst = netlist.instances.get_mut(inst_id).unwrap();
        inst.attributes.insert("component_class".into(), "capacitor".into());
        // Solver contract: the converter honors `value` (unit-aware via
        // spice_model) and NUMERIC esr/esl attribute strings; a pretty
        // "5nH" would silently parse to NOTHING and the cap would stamp
        // ideal — a silent-drop path. Numbers for the solver, the
        // library entity keeps the pretty datasheet text.
        inst.attributes.insert("spice_model".into(), "capacitor".into());
        // the SOLVER sees the EFFECTIVE capacitance at the rail's DC
        // bias (per-part curve when declared; nominal otherwise) — the
        // sweep judges the network as biased ceramics actually behave
        let c_eff = c_effective_at(cand.c_f, &cand.dc_bias, rail_v);
        inst.attributes.insert("value".into(), format!("{c_eff}"));
        inst.attributes.insert("capacitance".into(), cand.c_text.clone());
        if !cand.dc_bias.is_empty() {
            inst.attributes.insert(
                "dc_bias".into(),
                cand.dc_bias.iter().map(|(v, c)| format!("{v}V:{c}F")).collect::<Vec<_>>().join(","),
            );
        }
        inst.attributes.insert("esr".into(), format!("{}", cand.esr_ohm));
        inst.attributes.insert("esl".into(), format!("{}", cand.esl_h));
        inst.attributes.insert("voltage_rating".into(), format!("{}V", cand.v_rating));
        if let Some(r) = cand.ripple_a {
            inst.attributes.insert("ripple_current".into(), format!("{r}A"));
        }
        inst.attributes.insert("kicad_symbol".into(), "Device:C".into());
        inst.attributes
            .insert("decap_origin".into(), format!("decouple {}.{}", stmt.instance, stmt.domain));
        inst.attributes.insert("decap_lib".into(), stmt.lib.clone());
    }
    let pis = netlist
        .create_pin_instances(inst_id)
        .map_err(|e| format!("decouple: pin instances: {e}"))?;
    crate::virtual_pin_expander::connect_pin_instance_by_name(netlist, inst_id, &pis, "1", rail)?;
    crate::virtual_pin_expander::connect_pin_instance_by_name(netlist, inst_id, &pis, "2", gnd)?;
    Ok(())
}

fn remove_instance(netlist: &mut Netlist, name: &str) {
    let Some((id, _)) = netlist.instances.iter().find(|(_, i)| i.name == name) else { return };
    let pis: Vec<_> = netlist
        .pin_instances
        .iter()
        .filter(|(_, pi)| pi.instance == id)
        .map(|(pid, _)| pid)
        .collect();
    for pid in pis {
        if let Some(pi) = netlist.pin_instances.get(pid) {
            if let Some(net) = pi.net {
                crate::virtual_pin_expander::disconnect_pin_from_net(netlist, pid, net);
            }
        }
        netlist.pin_instances.remove(pid);
    }
    netlist.instances.remove(id);
}

/// Run every `decouple` statement against the synthesized netlist.
/// Hard error on infeasibility or a failed margin verification.

/// The supplying stage's declared control crossover (`f_c`) for the
/// rails a load instance hangs on — None when no stage declares one
/// (the usual case today; absence is a stated gap, never a default).
fn stage_fc(netlist: &Netlist, inst_name: &str) -> Option<f64> {
    let inst_id = netlist.instances.iter().find(|(_, i)| i.name == inst_name).map(|(id, _)| id)?;
    let mut inst_nets: Vec<bhdl_netlist::types::NetId> = Vec::new();
    for pi in netlist.pin_instances.values() {
        if pi.instance == inst_id {
            if let Some(n) = pi.net {
                inst_nets.push(n);
            }
        }
    }
    for pi in netlist.pin_instances.values() {
        let Some(n) = pi.net else { continue };
        if !inst_nets.contains(&n) {
            continue;
        }
        let Some(p) = netlist.pins.get(pi.pin_def) else { continue };
        if !p.name.starts_with("VOUT") {
            continue;
        }
        if let Some(inst) = netlist.instances.get(pi.instance) {
            if let Some(fc) = inst.attributes.get("f_c").and_then(|v| crate::stage_acceptance::parse_si(v)) {
                return Some(fc);
            }
        }
    }
    None
}

pub fn run_decap_synthesis(
    netlist: &mut Netlist,
    sf: &SourceFile,
    overrides: &std::collections::HashMap<String, bhdl_common::model::EvaluatedModel>,
) -> Result<Vec<bhdl_common::analysis_interface::DecapReport>, String> {
    use bhdl_common::analysis_interface::{DecapReport, DecapStep};
    let mut reports: Vec<DecapReport> = Vec::new();
    let stmts = parse_decouple_stmts(sf)?;
    if stmts.is_empty() {
        return Ok(reports);
    }
    let domains = entity_domain_map(&sf.syntax().clone());

    for stmt in &stmts {
        // Idempotency: instances from a previous run of THIS statement
        // (an elaborated file re-states them; the statement itself is
        // not re-emitted, but belt-and-braces for hand-carried boards).
        let already: usize = netlist
            .instances
            .iter()
            .filter(|(_, i)| {
                i.attributes.get("decap_origin").map(String::as_str)
                    == Some(&format!("decouple {}.{}", stmt.instance, stmt.domain))
            })
            .count();
        if already > 0 {
            info!("decouple {}.{}: {already} synthesized decap(s) already present — skipped", stmt.instance, stmt.domain);
            reports.push(DecapReport {
                target: format!("{}.{}", stmt.instance, stmt.domain),
                lib: stmt.lib.clone(),
                z_margin_pct: stmt.z_margin_pct,
                already_present: true,
                ..Default::default()
            });
            continue;
        }

        // Resolve the instance → entity → declared domain.
        let Some((_, inst)) = netlist.instances.iter().find(|(_, i)| i.name == stmt.instance) else {
            return Err(format!("decouple: no instance '{}' on the board", stmt.instance));
        };
        let ety = netlist
            .modules
            .get(inst.definition)
            .map(|m| m.name.clone())
            .unwrap_or_default();
        let dom = domains
            .get(&ety)
            .and_then(|(ds, _)| ds.iter().find(|d| d.name == stmt.domain))
            .cloned()
            .ok_or_else(|| {
                format!(
                    "decouple: entity '{ety}' (instance '{}') declares no domain '{}' — the Z(f) mask must come from the entity's `domain` contract",
                    stmt.instance, stmt.domain
                )
            })?;
        // AUTO-MASK (spec §7.5): a domain that declares `step` and
        // `droop_max` has already stated its low-frequency target
        // impedance — Z = droop_max/step, flat across the step's
        // spectral band [1/(2π·(dur+2·rise)) … 1/(π·rise)]. DERIVED,
        // never invented. Below the supplying stage's control
        // crossover the REGULATOR carries the step; no block declares
        // `f_c` yet, so without one the auto-mask applies only from
        // the 100 kHz sweep floor up, and the sub-crossover region is
        // a NAMED UNCHECKED gap — never a silent pass, and never an
        // absurd caps-only demand at hundreds of Hz.
        let mut dom = dom;
        if let (Some(step), Some(droop_pct)) = (dom.step_a, dom.droop_max_pct) {
            if step > 0.0 {
                let z = droop_pct / 100.0 * dom.v_nom / step;
                let rise = dom.step_rise_s.unwrap_or(1e-6);
                let durs = dom.step_dur_s.unwrap_or(1e-4);
                let f_hi = 1.0 / (std::f64::consts::PI * rise);
                // a PERIODIC burst concentrates its content at the
                // fundamental 1/period and harmonics — the low edge is
                // the fundamental, not the single-shot estimate
                let f_lo_step = match dom.step_period_s {
                    Some(per) if per > 0.0 => 1.0 / per,
                    _ => 1.0 / (2.0 * std::f64::consts::PI * (durs + 2.0 * rise)),
                };
                let f_c = stage_fc(netlist, &stmt.instance);
                let f_lo = match f_c {
                    Some(fc) => f_lo_step.max(fc),
                    None => f_lo_step.max(100e3),
                };
                if f_hi > f_lo {
                    for &(f, zz) in &[(f_lo, z), (f_hi, z)] {
                        let declared = mask_at(&dom.zmask, f);
                        if declared.map(|d| zz < d).unwrap_or(true) {
                            dom.zmask.push((f, zz));
                        }
                    }
                    dom.zmask.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                    info!(
                        "decouple {}.{}: AUTO-MASK {:.1}mΩ = droop_max/step across {:.0}–{:.0} kHz (derived from the domain's own declaration)",
                        stmt.instance, stmt.domain, z * 1e3, f_lo / 1e3, f_hi / 1e3
                    );
                }
                if f_c.is_none() && f_lo_step < 100e3 {
                    info!(
                        "decouple {}.{}: step content below 100 kHz ({:.2} kHz–) is the REGULATOR's — its control crossover is undeclared, so that region is UNCHECKED (declare `attribute f_c = <datasheet>` on the stage block to close it)",
                        stmt.instance, stmt.domain, f_lo_step / 1e3
                    );
                }
            }
        }
        if dom.zmask.len() < 2 {
            // a step/droop_max declaration whose spectral content sits
            // ENTIRELY below the crossover floor derives no in-band
            // mask — that region is the REGULATOR's (stated UNCHECKED
            // pending an `f_c` declaration), so the statement is
            // SKIPPED gracefully, never a hard failure
            if dom.step_a.is_some() && dom.droop_max_pct.is_some() {
                info!(
                    "decouple {}.{}: the step's spectral content lies entirely below the crossover floor — no in-band mask to synthesize against; the low-frequency transient is the regulator's (UNCHECKED pending `attribute f_c`, stated) — statement skipped",
                    stmt.instance, stmt.domain
                );
                continue;
            }
            return Err(format!(
                "decouple {}.{}: the domain declares no usable zmask (need ≥2 breakpoints) and no step/droop_max to derive one — nothing to synthesize against",
                stmt.instance, stmt.domain
            ));
        }
        // Derate the mask ONCE, up front: every check below (selection,
        // budget floor, single-open verification) sees the tightened
        // mask, so the synthesized network carries z_margin headroom
        // against layout parasitics this sweep does not model.
        if stmt.z_margin_pct > 0.0 {
            let k = 1.0 + stmt.z_margin_pct / 100.0;
            for bp in dom.zmask.iter_mut() {
                bp.1 /= k;
            }
            info!(
                "decouple {}.{}: mask derated by {}% headroom for unmodeled layout parasitics (z_margin) — all checks run against the tightened mask",
                stmt.instance, stmt.domain, stmt.z_margin_pct
            );
        }

        // Rail net = the net on the domain's first pin; GND = the
        // board's ground net.
        let rail_name = dom
            .pins
            .first()
            .and_then(|p0| {
                netlist.pin_instances.values().find_map(|pi| {
                    let i = netlist.instances.get(pi.instance)?;
                    if i.name != stmt.instance {
                        return None;
                    }
                    let p = netlist.pins.get(pi.pin_def)?;
                    if p.name != *p0 {
                        return None;
                    }
                    netlist.nets.get(pi.net?)?.name.clone()
                })
            })
            .ok_or_else(|| {
                format!("decouple {}.{}: domain pins not connected — no rail net to decouple", stmt.instance, stmt.domain)
            })?;
        let rail_id = netlist
            .nets
            .iter()
            .find(|(_, n)| n.name.as_deref() == Some(rail_name.as_str()))
            .map(|(id, _)| id)
            .ok_or("decouple: rail net vanished")?;
        let gnd_id = netlist
            .nets
            .iter()
            .filter(|(_, n)| matches!(n.net_class, NetClass::Ground))
            .max_by_key(|(_, n)| (n.name.as_deref() == Some("GND")) as u8)
            .map(|(id, _)| id)
            .ok_or_else(|| format!("decouple {}.{}: the board has no ground net", stmt.instance, stmt.domain))?;

        // Physics floor: if the declared PDN budget ALONE exceeds the
        // mask anywhere, no capacitor can fix it — say so first.
        {
            let l = dom.pdn_l_h.unwrap_or(0.0);
            let r = dom.pdn_r_ohm.unwrap_or(0.0);
            for &(f, m) in &dom.zmask {
                let zb = (r * r + (2.0 * std::f64::consts::PI * f * l).powi(2)).sqrt();
                if zb > m {
                    return Err(format!(
                        "decouple {}.{}: INFEASIBLE — the declared PDN budget alone (R={:.2}mΩ, L={:.2}nH → |Z|={:.1}mΩ at {:.2}MHz) exceeds the mask ({:.1}mΩ incl. {}% z_margin derating) before any capacitor is placed; reduce the budget inductance, renegotiate the mask, or lower z_margin",
                        stmt.instance, stmt.domain, dom.pdn_r_ohm.unwrap_or(0.0) * 1e3, dom.pdn_l_h.unwrap_or(0.0) * 1e9, zb * 1e3, f / 1e6, m * 1e3, stmt.z_margin_pct
                    ));
                }
            }
        }

        let (cands, mut cand_skips) = load_library(&stmt.lib)?;
        // VOLTAGE-RATING gate: a candidate must be rated for THIS rail
        // (times the project's derating policy where one is declared —
        // undeclared policy = checked at 100 % of rating, stated).
        let derate = crate::powertree::project_cap_v_derating(&sf.syntax().text().to_string());
        let v_req = dom.v_nom / derate.unwrap_or(1.0);
        let n_before = cands.len();
        let cands: Vec<Candidate> = cands
            .into_iter()
            .filter(|c| {
                let ok = c.v_rating >= v_req - 1e-9;
                if !ok {
                    info!(
                        "decouple {}.{}: candidate '{}' EXCLUDED — voltage_rating {:.1}V < required {:.2}V for the {:.2}V rail{}",
                        stmt.instance, stmt.domain, c.entity, c.v_rating, v_req, dom.v_nom,
                        match derate { Some(d) => format!(" (cap_v_derating {:.0}%)", d * 100.0), None => " (no cap_v_derating declared — 100% of rating, stated)".to_string() }
                    );
                    cand_skips.push(format!("{}: voltage_rating {:.1}V < {:.2}V required", c.entity, c.v_rating, v_req));
                }
                ok
            })
            .collect();
        if cands.is_empty() && n_before > 0 {
            return Err(format!(
                "decouple {}.{}: every library candidate is voltage-EXCLUDED for the {:.2}V rail (required rating ≥ {:.2}V{}) — add rated parts to the shortlist",
                stmt.instance, stmt.domain, dom.v_nom, v_req,
                match derate { Some(d) => format!(", cap_v_derating {:.0}%", d * 100.0), None => String::new() }
            ));
        }
        info!(
            "decouple {}.{} @ net {}: {} candidate(s) from {} (voltage-gated ≥ {:.2}V), mask {} breakpoints, max_parts {}",
            stmt.instance, stmt.domain, rail_name, cands.len(), stmt.lib, v_req, dom.zmask.len(), stmt.max_parts
        );

        // Greedy: trial every candidate, commit the argmin of the worst
        // ratio, until the mask is met or max_parts exhausted.
        let mut chosen: Vec<Candidate> = Vec::new();
        let mut steps: Vec<DecapStep> = Vec::new();
        let mut margin_added: Vec<String> = Vec::new();
        let mut bulk_exempt: Vec<String> = Vec::new();
        let (mut ratio, mut wf, mut wz) = worst_ratio(netlist, &rail_name, &dom, overrides)?;
        let mut n = 0usize;
        while ratio > 1.0 && n < stmt.max_parts {
            let mut best: Option<(f64, usize)> = None;
            for (ci, cand) in cands.iter().enumerate() {
                let trial_name = format!("__decap_trial__");
                mint_decap(netlist, cand, &trial_name, rail_id, gnd_id, stmt, dom.v_nom)?;
                let r = worst_ratio(netlist, &rail_name, &dom, overrides)?.0;
                remove_instance(netlist, &trial_name);
                if best.map(|(br, _)| r < br).unwrap_or(true) {
                    best = Some((r, ci));
                }
            }
            let (best_r, best_ci) = best.expect("candidates nonempty");
            if best_r >= ratio - 1e-9 {
                return Err(format!(
                    "decouple {}.{}: INFEASIBLE — no library capacitor improves the worst violation (|Z|={:.1}mΩ vs mask {:.1}mΩ at {:.2}MHz, ratio {:.2}); the library lacks a part effective at that frequency",
                    stmt.instance, stmt.domain, wz * 1e3, (wz / ratio) * 1e3, wf / 1e6, ratio
                ));
            }
            n += 1;
            let name = format!("{}_{}_dec{}", stmt.instance, stmt.domain, n);
            mint_decap(netlist, &cands[best_ci], &name, rail_id, gnd_id, stmt, dom.v_nom)?;
            chosen.push(cands[best_ci].clone());
            let w = worst_ratio(netlist, &rail_name, &dom, overrides)?;
            info!(
                "decouple {}.{}: +{} ({}) → worst |Z|/mask {:.2} at {:.2}MHz",
                stmt.instance, stmt.domain, name, cands[best_ci].entity, w.0, w.1 / 1e6
            );
            steps.push(DecapStep {
                instance: name,
                entity: cands[best_ci].entity.clone(),
                value: cands[best_ci].c_text.clone(),
                ratio_after: w.0,
                freq_hz: w.1,
            });
            (ratio, wf, wz) = w;
        }
        if ratio > 1.0 {
            return Err(format!(
                "decouple {}.{}: INFEASIBLE within max_parts={} — worst |Z|={:.1}mΩ vs mask {:.1}mΩ at {:.2}MHz (ratio {:.2}); raise max_parts, extend the library, or renegotiate the mask",
                stmt.instance, stmt.domain, stmt.max_parts, wz * 1e3, (wz / ratio) * 1e3, wf / 1e6, ratio
            ));
        }

        // Margin: one extra per distinct non-bulk value (design margin +
        // open-fault robustness). Bulk caps (> bulk_over) are exempt —
        // area/cost — and their exemption is stated.
        let mut extra = 0usize;
        if stmt.margin > 0 {
            let mut seen: Vec<&Candidate> = Vec::new();
            for c in &chosen {
                if seen.iter().any(|s| (s.c_f - c.c_f).abs() < 1e-15) {
                    continue;
                }
                seen.push(c);
                if c.c_f > stmt.bulk_over_f {
                    info!(
                        "decouple {}.{}: margin — {} ({}) is bulk (> {}F), exempt (stated)",
                        stmt.instance, stmt.domain, c.entity, c.c_text, stmt.bulk_over_f
                    );
                    bulk_exempt.push(format!("{} ({})", c.entity, c.c_text));
                    continue;
                }
                for _ in 0..stmt.margin {
                    n += 1;
                    extra += 1;
                    let name = format!("{}_{}_dec{}", stmt.instance, stmt.domain, n);
                    mint_decap(netlist, c, &name, rail_id, gnd_id, stmt, dom.v_nom)?;
                    info!("decouple {}.{}: margin +{} ({})", stmt.instance, stmt.domain, name, c.entity);
                    margin_added.push(format!("{name} ({})", c.entity));
                }
            }
        }

        // VERIFY the margin: open each non-bulk synthesized cap one at a
        // time — the mask must still hold (safety: a single decap open
        // fault must not break the contract). Bulk opens are exempt and
        // stated.
        let all: Vec<(String, f64)> = netlist
            .instances
            .iter()
            .filter(|(_, i)| {
                i.attributes.get("decap_origin").map(String::as_str)
                    == Some(&format!("decouple {}.{}", stmt.instance, stmt.domain))
            })
            .map(|(_, i)| {
                let c = i.attributes.get("capacitance").and_then(|v| parse_farads(v)).unwrap_or(0.0);
                (i.name.clone(), c)
            })
            .collect();
        let mut opens_verified = 0usize;
        let mut opens_bulk_exempt = 0usize;
        if stmt.margin > 0 {
            for (name, c_f) in &all {
                if *c_f > stmt.bulk_over_f {
                    info!("decouple {}.{}: open({name}) not verified — bulk, margin-exempt (stated)", stmt.instance, stmt.domain);
                    opens_bulk_exempt += 1;
                    continue;
                }
                // open = detach pin 1 from the rail
                let pi_id = netlist
                    .pin_instances
                    .iter()
                    .find(|(_, pi)| {
                        netlist.instances.get(pi.instance).map(|i| i.name.as_str()) == Some(name.as_str())
                            && pi.net == Some(rail_id)
                    })
                    .map(|(id, _)| id);
                let Some(pi_id) = pi_id else { continue };
                crate::virtual_pin_expander::disconnect_pin_from_net(netlist, pi_id, rail_id);
                let (r_open, f_open, z_open) = worst_ratio(netlist, &rail_name, &dom, overrides)?;
                netlist
                    .connect(rail_id, bhdl_netlist::types::ConnectionPoint::PinInstance(pi_id))
                    .map_err(|e| format!("decouple: reconnect failed: {e}"))?;
                if r_open > 1.0 {
                    return Err(format!(
                        "decouple {}.{}: margin verification FAILED — with {name} open, |Z|={:.1}mΩ vs mask at {:.2}MHz (ratio {:.2}); the N+1 margin does not cover this open fault (add margin or a different value mix)",
                        stmt.instance, stmt.domain, z_open * 1e3, f_open / 1e6, r_open
                    ));
                }
                opens_verified += 1;
            }
        }

        info!(
            "decouple {}.{}: SYNTHESIZED {} cap(s) + {} margin — final worst |Z|/mask {:.2} at {:.2}MHz against the {}%-derated mask; every non-bulk single-open verified against the same derated mask",
            stmt.instance, stmt.domain, chosen.len(), extra, ratio, wf / 1e6, stmt.z_margin_pct
        );
        let _ = extra;
        reports.push(DecapReport {
            target: format!("{}.{}", stmt.instance, stmt.domain),
            net: rail_name.clone(),
            lib: stmt.lib.clone(),
            mask_breakpoints: dom.zmask.len(),
            z_margin_pct: stmt.z_margin_pct,
            candidates_usable: cands.len(),
            candidates_skipped: cand_skips,
            steps,
            margin_added,
            bulk_exempt,
            opens_verified,
            opens_bulk_exempt,
            final_ratio: ratio,
            final_freq_hz: wf,
            already_present: false,
        });
    }
    Ok(reports)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The statement's knobs parse — including z_margin (the layout-
    /// headroom derating) and lexer-split unit suffixes.
    #[test]
    fn decouple_stmt_parses_all_knobs() {
        let src = r#"
board B {
    ground GND;
    decouple soc.VDD from "lib.bhdl" max_parts=6 margin=2 bulk_over=47uF z_margin=35%;
}
"#;
        let pr = bhdl_parser::parse(src);
        assert!(pr.errors().is_empty(), "{:?}", pr.errors());
        let sf = SourceFile::cast(pr.syntax()).unwrap();
        let stmts = parse_decouple_stmts(&sf).unwrap();
        assert_eq!(stmts.len(), 1);
        let s = &stmts[0];
        assert_eq!((s.instance.as_str(), s.domain.as_str()), ("soc", "VDD"));
        assert_eq!(s.max_parts, 6);
        assert_eq!(s.margin, 2);
        assert!((s.bulk_over_f - 47e-6).abs() < 1e-12);
        assert!((s.z_margin_pct - 35.0).abs() < 1e-12);
    }

    /// Defaults: max_parts=12, margin=1 (N+1), bulk_over=10µF,
    /// z_margin=20% — generous-by-default because layout effects are
    /// not modeled.
    #[test]
    fn decouple_stmt_defaults() {
        let src = r#"
board B {
    ground GND;
    decouple u.CORE from "lib.bhdl";
}
"#;
        let pr = bhdl_parser::parse(src);
        let sf = SourceFile::cast(pr.syntax()).unwrap();
        let s = &parse_decouple_stmts(&sf).unwrap()[0];
        assert_eq!(s.max_parts, 12);
        assert_eq!(s.margin, 1);
        assert!((s.bulk_over_f - 10e-6).abs() < 1e-12);
        assert!((s.z_margin_pct - 20.0).abs() < 1e-12);
    }
}
