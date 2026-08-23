//! Power-supply synthesis — the `supply` statement desugar (S1).
//!
//! `docs/spec/Power_Supply_Synthesis.md`. A `supply @VOUT from @VIN { … }`
//! statement is a REQUIREMENT; this pass rewrites it into exactly the
//! instantiation a board writes by hand — import + spec-threaded constructor
//! call + VIN/VOUT/GND/EN wiring — so every layer beneath the choice runs the
//! existing, verified pipeline (part `design{}` sizing, expansion, GLACIER,
//! §4 stress, sign-off, supplier plugins).
//!
//! S1 scope: the part is named explicitly (`using: <Part>;`). S2 replaces
//! that with the capability filter + topology rule and fills in the report's
//! candidate-survey sections.
//!
//! The desugar is SOURCE-LEVEL: parse → locate SUPPLY_STMT nodes → splice
//! replacement text → the caller re-parses. That keeps every downstream pass
//! (analyzer, synthesizer, sign-off, BOM) oblivious to the feature, and the
//! generated text doubles as the report's "winner and instantiation" section
//! verbatim.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use bhdl_parser::SyntaxKind;

/// One desugared `supply` statement — kept for the synthesis report.
#[derive(Debug, Clone)]
pub struct SupplyDesugar {
    pub target_rail: String,
    pub source_rail: String,
    pub part: String,
    pub instance: String,
    /// The spec entries verbatim (key → value text), `using` included.
    pub specs: Vec<(String, String)>,
    /// The generated BHDL text spliced in place of the statement.
    pub generated: String,
    /// The generated import line (empty when the board already imports the part).
    pub import_line: String,
    /// S2 candidate survey — every regulator considered, with per-gate
    /// verdicts and the ranking score. Empty when the part was named
    /// explicitly (`using:`) — that override is itself recorded in the report.
    pub survey: Vec<Candidate>,
    /// Report design curves (S3): (title, x-label, y-label, points, note).
    /// Computed from the same closed forms the chooser/sizer use — seed
    /// values, marked as such.
    pub curves: Vec<(String, String, String, Vec<(f64, f64)>, String)>,
}

/// One surveyed regulator candidate (report sections 3–4).
#[derive(Debug, Clone)]
pub struct Candidate {
    pub part: String,
    /// (gate name, detail with computed numbers, passed).
    pub gates: Vec<(String, String, bool)>,
    /// Estimated regulator loss in watts (the ranking score, lower = better):
    /// linears P = (V_IN−V_OUT)·I + V_IN·I_q; switchers
    /// P ≈ I²·R_ds·D + V_IN·I·f_sw·t_sw + V_IN·I_q. `None` when the part
    /// failed a hard gate.
    pub loss_w: Option<f64>,
    /// Support parts the entity's expansion materialises (BOM-size proxy —
    /// the `profile: cost` ranking key until real per-candidate catalogue
    /// pricing lands).
    pub support_parts: usize,
    pub chosen: bool,
    /// Loss-model params for the report curves:
    /// (is_switcher, rds_on Ω, f_sw Hz, t_sw s, i_q A).
    pub loss_params: Option<(bool, f64, f64, f64, f64)>,
    /// Real catalogue price of the regulator itself (cheapest in-stock MPN
    /// with this part-name prefix, via the jlcparts provider's mpn_query) —
    /// the `profile: cost` primary key. None when the provider/DB is absent
    /// or the prefix matches nothing (stated in the survey, never silent).
    pub ic_price: Option<f64>,
    pub ic_mpn: Option<String>,
    pub ic_sku: Option<String>,
    /// Summed catalogue price of the SEED-sized support parts (expansion
    /// children resolved to class+value: literals, param defaults, and the
    /// L/C/divider closed forms), priced through the provider's passive
    /// path in one batch. None when the provider/DB is absent.
    pub support_cost: Option<f64>,
    /// Support parts that could not be resolved or priced (diodes, exotic
    /// values, un-resolvable refs) — counted and stated, never silent.
    pub unpriced_parts: usize,
}

/// Result of the desugar pass over one source file.
#[derive(Debug, Clone)]
pub struct DesugaredSource {
    pub source: String,
    pub supplies: Vec<SupplyDesugar>,
}

/// Fast gate so ordinary boards pay nothing: only sources that contain the
/// token `supply` anywhere go through the parse-and-splice pass.
pub fn source_has_supply_stmt(source: &str) -> bool {
    source.contains("supply ") || source.contains("supply@")
}

/// Desugar every `supply` statement in `source`. Returns `None` when there
/// are none. `stdlib_root` is the directory scanned for the `using:` part
/// (normally `bhdl-stdlib/`).
pub fn desugar_supplies(source: &str, stdlib_root: &Path) -> Result<Option<DesugaredSource>> {
    if !source_has_supply_stmt(source) {
        return Ok(None);
    }
    let parsed = bhdl_parser::parse(source);
    let root = parsed.syntax();

    // Collect (byte-range, node) for every SUPPLY_STMT, in source order.
    let stmts: Vec<_> = root
        .descendants()
        .filter(|n| n.kind() == SyntaxKind::SUPPLY_STMT)
        .collect();
    if stmts.is_empty() {
        return Ok(None);
    }

    let rails = parse_rails(source);
    let ground = parse_ground(source).unwrap_or_else(|| "GND".to_string());

    // S4c — shared input banks: supplies drawing from the SAME source rail
    // share one input bank sized for the summed demand instead of each
    // emitting its own c_in1. Pre-count supplies per source; statements are
    // processed back-to-front, so each contributor adds its demand and the
    // FILE-FIRST supply of the group (processed last) emits the bank.
    let mut shared_c_in: HashMap<String, SharedBank> = HashMap::new();
    for stmt in &stmts {
        if let Some(src_rail) = supply_stmt_source(stmt) {
            shared_c_in
                .entry(src_rail)
                .or_insert_with(SharedBank::default)
                .remaining += 1;
        }
    }
    shared_c_in.retain(|_, b| b.remaining > 1);

    let mut supplies = Vec::new();
    // Splice back-to-front so earlier byte ranges stay valid.
    let mut out = source.to_string();
    let mut import_lines: Vec<String> = Vec::new();

    for stmt in stmts.iter().rev() {
        let d = desugar_one(stmt, &rails, &ground, source, stdlib_root, &mut shared_c_in)?;
        let range = stmt.text_range();
        let (start, end) = (usize::from(range.start()), usize::from(range.end()));
        out.replace_range(start..end, &d.generated);
        // import_line may carry several lines (part + support passives, S4);
        // dedupe per line so two supplies sharing Cap don't double-import.
        for line in d.import_line.lines() {
            let line = line.to_string();
            if !line.is_empty() && !import_lines.contains(&line) {
                import_lines.push(line);
            }
        }
        supplies.push(d);
    }
    supplies.reverse();

    // Prepend the import lines after the last existing import (or at the top).
    if !import_lines.is_empty() {
        let insert_at = last_import_end(&out);
        let block = import_lines.join("\n") + "\n";
        out.insert_str(insert_at, &block);
    }

    Ok(Some(DesugaredSource { source: out, supplies }))
}

/// S4c accumulator for one shared source rail: how many supplies still owe
/// their input-cap demand, the running total, and who contributed.
#[derive(Default)]
struct SharedBank {
    remaining: usize,
    total_c: f64,
    contributors: Vec<String>,
}

/// The SOURCE rail name of a supply statement (second rail ident).
fn supply_stmt_source(stmt: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>) -> Option<String> {
    let idents: Vec<String> = stmt
        .children_with_tokens()
        .filter_map(|el| el.into_token())
        .filter(|t| t.kind() == SyntaxKind::IDENT)
        .map(|t| t.text().to_string())
        .collect();
    idents
        .iter()
        .skip(1)
        .filter(|t| t.as_str() != "from")
        .nth(1)
        .cloned()
}

fn desugar_one(
    stmt: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
    rails: &HashMap<String, RailDecl>,
    ground: &str,
    original: &str,
    stdlib_root: &Path,
    shared_c_in: &mut HashMap<String, SharedBank>,
) -> Result<SupplyDesugar> {
    // Statement-level identifiers: [target, source] are the first two IDENT
    // tokens after the leading `supply` keyword token.
    let idents: Vec<String> = stmt
        .children_with_tokens()
        .filter_map(|el| el.into_token())
        .filter(|t| t.kind() == SyntaxKind::IDENT)
        .map(|t| t.text().to_string())
        .collect();
    // idents[0] is the contextual keyword `supply` itself; `from` may appear
    // as a plain IDENT (the lexer only produces FROM_KW in import position).
    let rail_idents: Vec<&String> = idents
        .iter()
        .skip(1)
        .filter(|t| t.as_str() != "from")
        .collect();
    let (target, source_rail) = match (rail_idents.first(), rail_idents.get(1)) {
        (Some(t), Some(s)) => ((*t).clone(), (*s).clone()),
        _ => bail!("supply statement: missing target/source rail names"),
    };

    // Spec entries.
    let mut specs: Vec<(String, String)> = Vec::new();
    for entry in stmt
        .children()
        .filter(|n| n.kind() == SyntaxKind::SUPPLY_SPEC_ENTRY)
    {
        let key = entry
            .children_with_tokens()
            .filter_map(|el| el.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT)
            .map(|t| t.text().to_string())
            .ok_or_else(|| anyhow!("supply spec entry without a key"))?;
        let text = entry.text().to_string();
        let value = text
            .split_once(':')
            .map(|(_, v)| v.trim().trim_end_matches(';').trim().to_string())
            .unwrap_or_default();
        specs.push((key, value));
    }

    // Rails: the whole derivation depends on the real operating point.
    let t = rails.get(&target).ok_or_else(|| {
        anyhow!("supply @{target}: no `power {target} = …;` rail declared on the board")
    })?;
    let s = rails.get(&source_rail).ok_or_else(|| {
        anyhow!("supply @{target}: source rail `{source_rail}` not declared")
    })?;
    let i_out = t.current.clone().ok_or_else(|| {
        anyhow!(
            "supply @{target}: target rail declares no `@ I` load — the sizing and \
             stress derivations depend on the real load (Real-Data Policy); declare \
             `power {target} = {v} @ <load>;`",
            v = t.voltage
        )
    })?;

    // Part: explicit `using:` (recorded as an engineer override), else the
    // S2 chooser — capability gates + loss ranking over the stdlib catalogue
    // (Power_Supply_Synthesis.md §3), with the full survey kept for the
    // report.
    let spec_num = |k: &str| {
        specs
            .iter()
            .find(|(sk, _)| sk == k)
            .and_then(|(_, v)| parse_si_txt(v))
    };
    let explicit = specs
        .iter()
        .find(|(k, _)| k == "using")
        .map(|(_, v)| v.clone());
    let (part, survey) = match explicit {
        Some(p) => (p, Vec::new()),
        None => {
            let v_in_n = parse_si_txt(&s.voltage)
                .ok_or_else(|| anyhow!("supply @{target}: unparseable source rail voltage `{}`", s.voltage))?;
            let v_out_n = parse_si_txt(&t.voltage)
                .ok_or_else(|| anyhow!("supply @{target}: unparseable target rail voltage `{}`", t.voltage))?;
            let i_out_n = parse_si_txt(&i_out)
                .ok_or_else(|| anyhow!("supply @{target}: unparseable rail load `{i_out}`"))?;
            let profile = specs
                .iter()
                .find(|(k, _)| k == "profile")
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| "balanced".into());
            choose_part(
                stdlib_root,
                v_in_n,
                v_out_n,
                i_out_n,
                spec_num("efficiency_min"),
                spec_num("i_q_max"),
                spec_num("ripple_max"),
                &profile,
            )?
        }
    };

    // Resolve the part in the stdlib and read its constructor params + pins.
    let (part_path, entity_src) = find_entity(stdlib_root, &part)?;
    let params = entity_param_names(&entity_src, &part);
    let pins = entity_pin_names(&entity_src, &part);
    for required in ["VIN", "GND"] {
        if !pins.iter().any(|p| p == required) {
            bail!(
                "supply @{target}: part `{part}` has no {required} pin — not a \
                 regulator entity this statement can wire"
            );
        }
    }
    let has_pin = |p: &str| pins.iter().any(|x| x == p);
    if !has_pin("VOUT") && !has_pin("SW") {
        bail!(
            "supply @{target}: part `{part}` has neither a VOUT nor a SW pin — \
             not a regulator shape this statement can wire"
        );
    }

    // Spec → constructor threading, intersected with the entity's params.
    let spec_val = |k: &str| specs.iter().find(|(sk, _)| sk == k).map(|(_, v)| v.clone());
    let mut args: Vec<String> = Vec::new();
    let mut push_if = |param: &str, val: Option<String>| {
        if let Some(v) = val {
            if params.iter().any(|p| p == param) {
                args.push(format!("{param}={v}"));
            }
        }
    };
    push_if("v_out", Some(t.voltage.clone()));
    push_if("vout_target", Some(t.voltage.clone()));
    push_if("v_in", Some(s.voltage.clone()));
    push_if("i_out_max", Some(i_out.clone()));
    push_if("ripple_v", spec_val("ripple_max"));
    // Requirement attributes ride as named args regardless of entity params —
    // Phase 4.4 stamps every named constructor arg onto the instance, which is
    // how the requirement sign-off rows later find the spec.
    if let Some(r) = spec_val("ripple_max") {
        args.push(format!("supply_ripple_max={r}"));
    }
    if let Some(e) = spec_val("efficiency_min") {
        args.push(format!("supply_efficiency_min={e}"));
    }
    if let Some(q) = spec_val("i_q_max") {
        args.push(format!("supply_i_q_max={q}"));
    }
    if let Some(p) = spec_val("profile") {
        args.push(format!("supply_profile=\"{p}\""));
    }

    // Part datasheet attributes + numeric operating point — used by the
    // input-draw stamp below and the S4 application-circuit emitter.
    let part_attrs = entity_attrs_txt(&entity_src, &part);
    let attr_num = |k: &str| {
        part_attrs.get(k).and_then(|v| {
            parse_si_txt(v).or_else(|| {
                entity_param_default(&entity_src, &part, v.trim())
                    .and_then(|d| parse_si_txt(&d))
            })
        })
    };
    let v_in_f = parse_si_txt(&s.voltage).unwrap_or(0.0);
    let v_out_f = parse_si_txt(&t.voltage).unwrap_or(0.0);
    let i_out_f = parse_si_txt(&i_out).unwrap_or(0.0);
    let f_sw = attr_num("switching_frequency")
        .or_else(|| attr_num("f_sw"))
        .unwrap_or(0.0);

    // ── S4b: rail-budget propagation (supply trees) ──
    //
    // Stamp the supply's INPUT draw as `i_supply` so ERC016 gates the
    // SOURCE rail against everything hanging off it — a cascaded supply
    // is just another declared load on its upstream rail. Derivation is
    // physics or datasheet, never a guess (Real-Data Policy):
    //  - linear (no switching frequency): I_in = I_out + I_q exactly;
    //  - switcher: I_in = V_out·I_out / (η·V_in), with η from the part's
    //    declared `efficiency`; no efficiency attr → NO stamp, and ERC016
    //    honestly counts this instance among the UNDECLARED draws.
    let i_q = attr_num("i_quiescent").unwrap_or(0.0);
    let i_in = if f_sw <= 0.0 {
        (i_out_f > 0.0).then(|| i_out_f + i_q)
    } else {
        let eff = part_attrs.get("efficiency").and_then(|v| {
            let v = v.trim();
            if let Some(pct) = v.strip_suffix('%') {
                pct.trim().parse::<f64>().ok().map(|p| p / 100.0)
            } else {
                parse_si_txt(v).filter(|e| *e > 0.0 && *e <= 1.0)
            }
        });
        match (eff, v_in_f > 0.0 && i_out_f > 0.0) {
            (Some(e), true) if e > 0.0 => Some(v_out_f * i_out_f / (e * v_in_f)),
            _ => None,
        }
    };
    if let Some(i) = i_in.filter(|i| *i > 0.0) {
        args.push(format!("i_supply={}", fmt_si(i, "A")));
    }

    let instance = format!("psu_{}", target.to_lowercase());
    let indent = line_indent(original, usize::from(stmt.text_range().start()));

    let mut g = String::new();
    g.push_str(&format!(
        "// synthesized from `supply @{target} from @{source_rail}` (S1, using: {part})\n"
    ));
    g.push_str(&format!(
        "{indent}@{source_rail} -> {instance}: {part}({}).VIN;\n",
        args.join(", ")
    ));
    g.push_str(&format!("{indent}{instance}.GND -> @{ground};\n"));
    if has_pin("EN") {
        g.push_str(&format!("{indent}@{source_rail} -> {instance}.EN;\n"));
    }
    if has_pin("VOUT") {
        g.push_str(&format!("{indent}{instance}.VOUT -> @{target};\n"));
    }

    // ── S4: the application circuit (Power_Supply_Synthesis.md §5) ──
    //
    // A regulator IC alone is not a supply. Emit the datasheet support
    // parts around the chosen part, sized from the SAME closed forms the
    // chooser priced (E-series snapped) combined with the part's declared
    // datasheet recommendations — max(ripple closed form, datasheet rec),
    // both real data, never a bare guess. Support instances are named with
    // the TI-style application-circuit designators ({instance}_c_in1, …)
    // and stamped `expansion_parent={instance}` so the part's §4 stress
    // block resolves them by local name (c_in1 / c_out1 / l_out) exactly
    // like expansion children — the generated supply signs off under the
    // part's own stress model. A value that cannot be derived from spec or
    // datasheet is SKIPPED, not defaulted (Real-Data Policy); the part's
    // own T2 `check{}` rules then flag anything load-bearing that is
    // missing. (part_attrs / attr_num / the numeric operating point are
    // hoisted above the instantiation for the S4b input-draw stamp.)
    //
    // Parts that carry their OWN `expansion { }` block (TPS54331, LM317
    // style) materialize their application circuit themselves — emitting
    // S4 support parts too would put two inductors in parallel on the
    // switch node. S4 emission is for BARE entities only.
    //
    // An `entity … as design` block (the two-layer library model) IS its
    // application circuit — same rule, declared rather than detected.
    let self_expanding = find_entity_decl(&entity_src, &part)
        .map(|at| {
            let tail = &entity_src[at..];
            let end = tail[1..].find("\nentity ").map(|p| p + 1).unwrap_or(tail.len());
            tail[..end].contains("expansion {")
        })
        .unwrap_or(false)
        || entity_declared_kind(&entity_src, &part).as_deref() == Some("design");
    let duty = if v_in_f > 0.0 { v_out_f / v_in_f } else { 0.0 };
    let d_il = 0.3 * i_out_f;
    let mut used_cap = false;
    let mut used_ind = false;
    let mut used_res = false;

    // max(closed-form seed, datasheet rec), E12-snapped. Either source alone
    // is enough; neither → None (skip).
    let cap_value = |seed: Option<f64>, rec_keys: &[&str]| -> Option<f64> {
        let rec = rec_keys.iter().find_map(|k| attr_num(k)).filter(|v| *v > 0.0);
        let seed = seed.filter(|v| *v > 0.0);
        match (seed, rec) {
            (Some(a), Some(b)) => Some(e_series_nearest(a.max(b), 12)),
            (Some(a), None) => Some(e_series_nearest(a, 12)),
            (None, Some(b)) => Some(e_series_nearest(b, 12)),
            (None, None) => None,
        }
    };

    // Switch node first: a SW-shaped part needs the output inductor to make
    // @target exist at all.
    if !self_expanding && has_pin("SW") && f_sw > 0.0 && d_il > 0.0 && v_in_f > v_out_f {
        let l = e_series_nearest((v_in_f - v_out_f) * duty / (f_sw * d_il), 12);
        if l > 0.0 {
            used_ind = true;
            g.push_str(&format!(
                "{indent}{instance}.SW -> {instance}_l_out: Ind({}, expansion_parent=\"{instance}\").1;\n",
                fmt_si(l, "H")
            ));
            g.push_str(&format!("{indent}{instance}_l_out.2 -> @{target};\n"));
            // The entity's logical output port (virtual on SW-shaped parts —
            // the copper leaves through the inductor) joins the rail so the
            // port carries its declared NAME: block diagrams and closed-loop
            // DC read `VOUT`, not an anonymous boundary net.
            if has_pin("VOUT") {
                g.push_str(&format!("{indent}{instance}.VOUT -> @{target};\n"));
            }
        }
    }
    // Bootstrap cap — only with the part's declared datasheet value.
    if !self_expanding && has_pin("SW") && (has_pin("BOOT") || has_pin("BST")) {
        if let Some(cb) = attr_num("bootstrap_capacitor").filter(|v| *v > 0.0) {
            let boot = if has_pin("BOOT") { "BOOT" } else { "BST" };
            used_cap = true;
            g.push_str(&format!(
                "{indent}{instance}.{boot} -> {instance}_c_boot: Cap({}, expansion_parent=\"{instance}\").1;\n",
                fmt_si(cb, "F")
            ));
            g.push_str(&format!("{indent}{instance}_c_boot.2 -> {instance}.SW;\n"));
        }
    }
    // Rail caps carry the entity's default voltage rating, like hand-wired
    // application circuits do — the required voltage CLASS is physical
    // selection's job (it derates against the rail and picks the real MPN;
    // an unpopulatable class surfaces as its UNPOPULATED warning, never a
    // silent pass).
    // Input cap across the source rail.
    let c_in_seed = (f_sw > 0.0)
        .then(|| i_out_f * duty * (1.0 - duty) / (f_sw * 0.15));
    let own_c_in = cap_value(c_in_seed, &["input_capacitor_rec", "input_capacitor_min"])
        .filter(|_| !self_expanding);
    match shared_c_in.get_mut(&source_rail) {
        Some(bank) => {
            // S4c: pool this supply's demand into the shared bank. EVERY
            // group member decrements the countdown (a self-expanding part
            // contributes no S4 demand — its own expansion carries its
            // input cap — but must not strand the bank); the member that
            // zeroes it emits the whole bank, sized Σ demands, snapped.
            if let Some(c) = own_c_in {
                bank.total_c += c;
                bank.contributors.push(instance.clone());
            }
            bank.remaining -= 1;
            if bank.remaining == 0 && bank.total_c > 0.0 {
                // Named as THIS supply's c_in1 (expansion_parent) so its §4
                // stress model gates the bank; the other contributors'
                // input-ripple axes go UNCHECKED (ERC024 ledger) rather
                // than guessed.
                let total = e_series_nearest(bank.total_c, 12);
                used_cap = true;
                g.push_str(&format!(
                    "{indent}// shared input bank for {} (S4c) — Σ of {} supplies' demand\n",
                    source_rail,
                    bank.contributors.len()
                ));
                g.push_str(&format!(
                    "{indent}@{source_rail} -> {instance}_c_in1: Cap({}, expansion_parent=\"{instance}\").1;\n",
                    fmt_si(total, "F")
                ));
                g.push_str(&format!("{indent}{instance}_c_in1.2 -> @{ground};\n"));
            }
        }
        None => {
            if let Some(c) = own_c_in {
                used_cap = true;
                g.push_str(&format!(
                    "{indent}@{source_rail} -> {instance}_c_in1: Cap({}, expansion_parent=\"{instance}\").1;\n",
                    fmt_si(c, "F")
                ));
                g.push_str(&format!("{indent}{instance}_c_in1.2 -> @{ground};\n"));
            }
        }
    }
    // Output cap across the target rail — the ripple form uses the ACTUAL
    // ripple spec when given.
    let c_out_seed = (f_sw > 0.0).then(|| {
        let dv = spec_num("ripple_max").unwrap_or(0.05);
        d_il / (8.0 * f_sw * dv)
    });
    if let Some(c) = cap_value(c_out_seed, &["output_capacitor_rec", "output_capacitor_min"])
        .filter(|_| !self_expanding)
    {
        used_cap = true;
        g.push_str(&format!(
            "{indent}@{target} -> {instance}_c_out1: Cap({}, expansion_parent=\"{instance}\").1;\n",
            fmt_si(c, "F")
        ));
        g.push_str(&format!("{indent}{instance}_c_out1.2 -> @{ground};\n"));
    }
    // Feedback divider — needs the FB pin, the reference, and the part's
    // datasheet-recommended bottom-leg value (`fb_divider_bottom`).
    if !self_expanding && has_pin("FB") {
        let v_ref = attr_num("feedback_voltage").unwrap_or(0.0);
        let r_bot = attr_num("fb_divider_bottom").unwrap_or(0.0);
        if v_ref > 0.0 && r_bot > 0.0 && v_out_f > v_ref {
            let r_top = e_series_nearest(r_bot * (v_out_f - v_ref) / v_ref, 96);
            used_res = true;
            g.push_str(&format!(
                "{indent}@{target} -> {instance}_r_fb_top: Res({}, expansion_parent=\"{instance}\").1;\n",
                fmt_si(r_top, "")
            ));
            g.push_str(&format!("{indent}{instance}_r_fb_top.2 -> {instance}.FB;\n"));
            g.push_str(&format!(
                "{indent}{instance}.FB -> {instance}_r_fb_bot: Res({}, expansion_parent=\"{instance}\").1;\n",
                fmt_si(r_bot, "")
            ));
            g.push_str(&format!("{indent}{instance}_r_fb_bot.2 -> @{ground};\n"));
        }
    }
    // Trim the trailing newline so the splice stays statement-shaped.
    while g.ends_with('\n') {
        g.pop();
    }

    // Import lines, repo-root-relative (the loader's convention), skipped
    // when the board already imports the entity. May be multi-line (part +
    // support passives); the caller dedupes per line.
    // Only actual `import` lines count — a bare substring scan read a
    // COMMENT mentioning "TPS54302, …" as an existing import and skipped
    // the real one.
    let already_imported = |name: &str| {
        original.lines().any(|l| {
            let l = l.trim_start();
            l.starts_with("import")
                && (l.contains(&format!("{{ {name} }}"))
                    || l.contains(&format!("{{ {name},"))
                    || l.contains(&format!(" {name},"))
                    || l.contains(&format!(", {name} }}")))
        })
    };
    let mut import_vec: Vec<String> = Vec::new();
    if !already_imported(&part) {
        import_vec.push(format!("import {{ {part} }} from \"{}\";", part_path.display()));
    }
    for (used, name, file) in [
        (used_cap, "Cap", "bhdl-stdlib/passives/capacitor.bhdl"),
        (used_ind, "Ind", "bhdl-stdlib/passives/inductor.bhdl"),
        (used_res, "Res", "bhdl-stdlib/passives/resistor.bhdl"),
    ] {
        if used && !already_imported(name) {
            import_vec.push(format!("import {{ {name} }} from \"{file}\";"));
        }
    }
    let import_line = import_vec.join("\n");

    // S3 design curves — only when the chooser ran (it computed the numeric
    // operating point). Seed closed forms, marked as such in the note.
    let curves = if survey.is_empty() {
        Vec::new()
    } else {
        build_curves(
            &survey,
            &part,
            parse_si_txt(&s.voltage).unwrap_or(0.0),
            parse_si_txt(&t.voltage).unwrap_or(0.0),
            parse_si_txt(&i_out).unwrap_or(0.0),
            spec_num("ripple_max"),
        )
    };

    Ok(SupplyDesugar {
        target_rail: target,
        source_rail,
        part,
        instance,
        specs,
        generated: g,
        import_line,
        survey,
        curves,
    })
}

#[derive(Debug, Clone)]
struct RailDecl {
    voltage: String,
    current: Option<String>,
}

/// Textual rail scan: `power NAME = 12V @ 3A;` lines and their explicit
/// boundary-port spelling `port NAME: power [dir] = 12V @ 3A;` (ports
/// doctrine — the two forms are one declaration). The desugar runs before
/// analysis, so this stays a plain line scan rather than a CST walk.
fn parse_rails(source: &str) -> HashMap<String, RailDecl> {
    let mut rails = HashMap::new();
    for line in source.lines() {
        let l = line.trim();
        let (name, rhs) = if let Some(rest) = l.strip_prefix("power ") {
            let Some((name, rhs)) = rest.split_once('=') else { continue };
            (name.trim().to_string(), rhs)
        } else if let Some(rest) = l.strip_prefix("port ") {
            // `port NAME: power [in|out|inout] = V @ I;`
            let Some((name, decl)) = rest.split_once(':') else { continue };
            if decl.trim_start().strip_prefix("power").is_none() {
                continue;
            }
            let Some((_, rhs)) = decl.split_once('=') else { continue };
            (name.trim().to_string(), rhs)
        } else {
            continue;
        };
        let rhs = rhs.trim().trim_end_matches(';').trim();
        let (voltage, current) = match rhs.split_once('@') {
            Some((v, i)) => (v.trim().to_string(), Some(i.trim().to_string())),
            None => (rhs.to_string(), None),
        };
        rails.insert(name, RailDecl { voltage, current });
    }
    rails
}

fn parse_ground(source: &str) -> Option<String> {
    source.lines().find_map(|l| {
        let l = l.trim();
        if let Some(g) = l.strip_prefix("ground ") {
            return Some(g.trim_end_matches(';').trim().to_string());
        }
        // `port NAME: ground;` — the explicit boundary-port spelling.
        let rest = l.strip_prefix("port ")?;
        let (name, decl) = rest.split_once(':')?;
        decl.trim().trim_end_matches(';').trim().eq("ground").then(|| name.trim().to_string())
    })
}

/// Byte offset just past the last top-level `import …;` line (or 0).
fn last_import_end(source: &str) -> usize {
    let mut end = 0usize;
    let mut off = 0usize;
    for line in source.split_inclusive('\n') {
        if line.trim_start().starts_with("import ") {
            end = off + line.len();
        }
        off += line.len();
    }
    end
}

fn line_indent(source: &str, at: usize) -> String {
    let line_start = source[..at].rfind('\n').map(|p| p + 1).unwrap_or(0);
    source[line_start..at]
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect()
}

/// Find the stdlib file defining `entity <name>` (or `alias <name> =`).
/// Returns (repo-root-relative import path, file contents).
fn find_entity(stdlib_root: &Path, name: &str) -> Result<(PathBuf, String)> {
    let mut files = Vec::new();
    collect_bhdl(stdlib_root, &mut files);
    let ent_pat = format!("entity {name}");
    let alias_pat = format!("alias {name} ");
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else { continue };
        let defines = text.lines().any(|l| {
            let l = l.trim_start();
            (l.starts_with(&ent_pat)
                && l[ent_pat.len()..]
                    .chars()
                    .next()
                    .map(|c| c == '(' || c == '<' || c == ' ' || c == '{')
                    .unwrap_or(false))
                || l.starts_with(&alias_pat)
        });
        if defines {
            // Import path relative to the CWD (repo root), matching the
            // loader's `bhdl-stdlib/…` convention.
            let rel = f
                .strip_prefix(std::env::current_dir().unwrap_or_default())
                .map(|p| p.to_path_buf())
                .unwrap_or(f.clone());
            return Ok((rel, text));
        }
    }
    bail!(
        "supply: part `{name}` not found in the stdlib ({}) — the catalogue is the \
         candidate universe",
        stdlib_root.display()
    )
}

fn collect_bhdl(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if p.file_name().and_then(|n| n.to_str()) == Some("experiments") {
                continue;
            }
            collect_bhdl(&p, out);
        } else if p.extension().and_then(|e| e.to_str()) == Some("bhdl") {
            out.push(p);
        }
    }
}

/// Constructor parameter names of `entity <name>(…)` in `src` — the text
/// between the entity's first `(` and its matching `)`, split on top-level
/// commas, name = the token before `:`.
fn entity_param_names(src: &str, name: &str) -> Vec<String> {
    let Some(ent_at) = find_entity_decl(src, name) else { return Vec::new() };
    let after = &src[ent_at..];
    let Some(open_rel) = after.find('(') else { return Vec::new() };
    // A `<generics>` list may precede the paren; the first `(` after the
    // entity name is the constructor list either way.
    let mut depth = 0usize;
    let mut params = Vec::new();
    let mut cur = String::new();
    // `//` comments inside the param list (the stdlib annotates every param)
    // contain commas and parens that would wreck the depth walk — skip them.
    let mut chars = after[open_rel + 1..].chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'/') {
            for k in chars.by_ref() {
                if k == '\n' {
                    break;
                }
            }
            continue;
        }
        match c {
            '(' => {
                depth += 1;
                cur.push(c);
            }
            ')' if depth == 0 => {
                if !cur.trim().is_empty() {
                    params.push(cur.clone());
                }
                break;
            }
            ')' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => {
                params.push(cur.clone());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    params
        .iter()
        .filter_map(|p| {
            p.split_once(':')
                .map(|(n, _)| n.trim().to_string())
                .filter(|n| !n.is_empty() && !n.starts_with("//"))
        })
        .collect()
}

/// Pin names of `entity <name>` in `src`: `pin <NAME>:` lines between the
/// entity declaration and the next line that is exactly `}`.
fn entity_pin_names(src: &str, name: &str) -> Vec<String> {
    let Some(ent_at) = find_entity_decl(src, name) else { return Vec::new() };
    let mut pins = Vec::new();
    for line in src[ent_at..].lines() {
        if line.trim_end() == "}" {
            break;
        }
        let l = line.trim_start();
        if let Some(rest) = l.strip_prefix("pin ") {
            if let Some((pname, _)) = rest.split_once(':') {
                pins.push(pname.trim().to_string());
            }
        }
    }
    pins
}

// ───────────────────────── S2: the chooser ─────────────────────────

/// Numeric SI parse for datasheet attribute text: `"12V"`, `"3.4mA"`,
/// `"90mΩ"/"90mohm"`, `"570kHz"`, `"2W"`, `"85%"` (→ 0.85), `"20ns"`,
/// plain numbers. Returns `None` for anything non-numeric.
pub(crate) fn parse_si_txt(s: &str) -> Option<f64> {
    let t = s.trim().trim_matches('"');
    let num_end = t
        .char_indices()
        .take_while(|(_, c)| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+')
        .map(|(i, c)| i + c.len_utf8())
        .last()?;
    let num: f64 = t[..num_end].parse().ok()?;
    let unit = t[num_end..].trim();
    if unit == "%" {
        return Some(num / 100.0);
    }
    // Strip the unit letters (V/A/W/F/H/Hz/s/ohm/Ω) to isolate the prefix.
    let prefix = unit
        .trim_end_matches("Hz")
        .trim_end_matches("ohm")
        .trim_end_matches(['V', 'A', 'W', 'F', 'H', 's', 'Ω']);
    let scale = match prefix {
        "" => 1.0,
        "p" => 1e-12,
        "n" => 1e-9,
        "u" | "µ" => 1e-6,
        "m" => 1e-3,
        "k" => 1e3,
        "M" => 1e6,
        "G" => 1e9,
        _ => return None,
    };
    Some(num * scale)
}

/// `attribute <key> = <value>;` map of the named entity (comment-stripped,
/// quotes trimmed), scanned textually from the entity block.
fn entity_attrs_txt(src: &str, name: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(at) = find_entity_decl(src, name) else { return out };
    for line in src[at..].lines() {
        if line.trim_end() == "}" {
            break;
        }
        let l = line.split("//").next().unwrap_or("").trim();
        let Some(rest) = l.strip_prefix("attribute ") else { continue };
        if let Some((k, v)) = rest.split_once('=') {
            out.insert(
                k.trim().to_string(),
                v.trim().trim_end_matches(';').trim().trim_matches('"').to_string(),
            );
        }
    }
    out
}

/// Default value text of one constructor param, e.g. `i_out_max` → `"2A"`.
fn entity_param_default(src: &str, name: &str, param: &str) -> Option<String> {
    // Reuse the comment-safe walker by re-deriving the raw chunks.
    let Some(ent_at) = find_entity_decl(src, name) else { return None };
    let after = &src[ent_at..];
    let open_rel = after.find('(')?;
    let mut depth = 0usize;
    let mut chunks = Vec::new();
    let mut cur = String::new();
    let mut chars = after[open_rel + 1..].chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'/') {
            for k in chars.by_ref() {
                if k == '\n' {
                    break;
                }
            }
            continue;
        }
        match c {
            '(' => {
                depth += 1;
                cur.push(c);
            }
            ')' if depth == 0 => {
                chunks.push(cur.clone());
                break;
            }
            ')' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => {
                chunks.push(cur.clone());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    for ch in chunks {
        let (n, rest) = ch.split_once(':')?;
        if n.trim() == param {
            return rest.split_once('=').map(|(_, d)| d.trim().to_string());
        }
    }
    None
}

/// The S2 chooser (Power_Supply_Synthesis.md §3): survey every stdlib
/// regulator, hard-gate on the datasheet capability attributes, rank the
/// survivors by estimated regulator loss. Every verdict carries the computed
/// numbers — the survey IS report sections 3–4, and a rejection with no
/// stated reason would be a fabricated default.
#[allow(clippy::too_many_arguments)]
fn choose_part(
    stdlib_root: &Path,
    v_in: f64,
    v_out: f64,
    i_out: f64,
    efficiency_min: Option<f64>,
    i_q_max: Option<f64>,
    ripple_max: Option<f64>,
    profile: &str,
) -> Result<(String, Vec<Candidate>)> {
    let mut files = Vec::new();
    collect_bhdl(stdlib_root, &mut files);

    let mut survey: Vec<Candidate> = Vec::new();
    let mut src_of: HashMap<String, String> = HashMap::new();
    for f in &files {
        let Ok(src) = std::fs::read_to_string(f) else { continue };
        // Every entity in the file whose class is a regulator class.
        for line in src.lines() {
            let l = line.trim_start();
            let Some(rest) = l.strip_prefix("entity ") else { continue };
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() {
                continue;
            }
            let attrs = entity_attrs_txt(&src, &name);
            let class = attrs.get("component_class").map(String::as_str).unwrap_or("");
            if !matches!(class, "voltage_regulator" | "ldo" | "switching_regulator") {
                continue;
            }
            // Two-layer library model (docs/spec/Requirements_And_Resolution.md):
            // an `entity X as part` is vendor truth — the bare silicon,
            // never "a regulator the board sees". Only the `as design`
            // block (or a migration-era conflated entity) is choosable.
            if entity_declared_kind(&src, &name).as_deref() == Some("part") {
                continue;
            }
            // Explicit opt-out: a generic-tier entity (BuckController) is a
            // template the author instantiates DIRECTLY with their part's
            // numbers — it names no orderable device, so it must never win
            // an automatic selection over a real part.
            if attrs.get("supply_choosable").map(String::as_str) == Some("false") {
                continue;
            }
            // Generic templates (no package → no honest power_rating; generic
            // `<V_OUT>` entities) are not selectable parts.
            if src[find_entity_decl(&src, &name).unwrap_or(0)..]
                .lines()
                .next()
                .map(|l| l.contains('<'))
                .unwrap_or(false)
            {
                continue;
            }
            // Numeric attr with param-ref resolution: `attribute rds_on =
            // rds_on;` stores the literal param NAME textually — resolve it
            // through the constructor default (the same `attribute X = X`
            // discipline the analyzer applies, done textually here because
            // the chooser runs pre-parse). A bare-identifier value that
            // matches no param yields None (honest UNCHECKED downstream).
            let attr_si = |k: &str| {
                let v = attrs.get(k)?;
                parse_si_txt(v).or_else(|| {
                    let ident = v.trim();
                    if ident.chars().all(|c| c.is_alphanumeric() || c == '_') {
                        entity_param_default(&src, &name, ident)
                            .and_then(|d| parse_si_txt(&d))
                    } else {
                        None
                    }
                })
            };
            let is_switcher = class == "switching_regulator"
                || attr_si("f_sw").is_some()
                || attr_si("switching_frequency").is_some();

            let mut gates: Vec<(String, String, bool)> = Vec::new();
            let mut push = |g: &str, d: String, ok: bool| gates.push((g.to_string(), d, ok));

            // v_out reachability.
            let adjustable = ["v_out", "vout_target"]
                .iter()
                .find(|p| entity_param_default(&src, &name, p).is_some());
            match (adjustable, attr_si("output_voltage")) {
                (Some(p), _) => {
                    let dropout = attr_si("dropout_voltage").unwrap_or(0.0);
                    let ok = v_out < v_in - dropout;
                    push(
                        "v_out",
                        format!(
                            "adjustable via `{p}`; needs {v_out:.2}V ≤ {v_in:.1}V − {dropout:.2}V dropout"
                        ),
                        ok,
                    );
                }
                (None, Some(fixed)) => {
                    let ok = (fixed - v_out).abs() <= 0.01 * v_out.max(0.1);
                    push("v_out", format!("fixed {fixed:.2}V vs required {v_out:.2}V"), ok);
                }
                (None, None) => push(
                    "v_out",
                    "no output-voltage capability declared (no v_out param, no attr)".into(),
                    false,
                ),
            }

            // Input range (when declared; missing = UNCHECKED pass, stated).
            match (attr_si("input_voltage_min"), attr_si("input_voltage_max")) {
                (Some(lo), Some(hi)) => push(
                    "v_in range",
                    format!("{lo:.1}–{hi:.1}V covers {v_in:.1}V"),
                    v_in >= lo && v_in <= hi,
                ),
                _ => push("v_in range", "UNCHECKED (no input range attrs)".into(), true),
            }

            // Load within rating: output_current attr, else the i_out_max
            // param default as the declared design envelope.
            let i_cap = attr_si("output_current").or_else(|| {
                entity_param_default(&src, &name, "i_out_max").and_then(|d| parse_si_txt(&d))
            });
            match i_cap {
                Some(cap) => push(
                    "i_out",
                    format!("{i_out:.2}A load vs {cap:.2}A capability"),
                    i_out <= cap,
                ),
                None => push("i_out", "UNCHECKED (no current capability declared)".into(), true),
            }

            let i_q = attr_si("i_quiescent");
            // Linear dissipation — the §4 self.p_diss form as a predictor.
            let mut loss = None;
            if !is_switcher {
                let p = (v_in - v_out) * i_out + v_in * i_q.unwrap_or(0.0);
                match attr_si("power_rating") {
                    Some(rating) => push(
                        "p_diss",
                        format!(
                            "(({v_in:.1}−{v_out:.2})·{i_out:.2} + {v_in:.1}·I_q) = {p:.2}W vs {rating:.2}W/2 derated"
                        ),
                        p <= rating / 2.0,
                    ),
                    None => push("p_diss", format!("{p:.2}W but no power_rating declared"), false),
                }
                if let Some(eff_min) = efficiency_min {
                    let eff = v_out / v_in;
                    push(
                        "efficiency",
                        format!("linear η = {v_out:.2}/{v_in:.1} = {:.1}% vs ≥{:.1}%", eff * 100.0, eff_min * 100.0),
                        eff >= eff_min,
                    );
                }
                loss = Some(p);
            } else {
                // Switcher loss estimate from the loss-model attrs.
                let duty = v_out / v_in;
                let rds = attr_si("rds_on").unwrap_or(0.0);
                let f_sw = attr_si("f_sw").or_else(|| attr_si("switching_frequency")).unwrap_or(0.0);
                let t_sw = attr_si("t_sw").unwrap_or(0.0);
                let p = i_out * i_out * rds * duty
                    + v_in * i_out * f_sw * t_sw
                    + v_in * i_q.unwrap_or(0.0);
                push(
                    "loss model",
                    format!(
                        "I²·R_ds·D + V·I·f_sw·t_sw + V·I_q = {p:.2}W (η ≈ {:.1}%)",
                        100.0 * (v_out * i_out) / (v_out * i_out + p)
                    ),
                    true,
                );
                loss = Some(p);
            }

            // Quiescent ceiling.
            if let Some(qmax) = i_q_max {
                match i_q {
                    Some(q) => push(
                        "i_q",
                        format!("{} vs ≤ {}", fmt_a(q), fmt_a(qmax)),
                        q <= qmax,
                    ),
                    None => push("i_q", "UNCHECKED (no i_quiescent attr)".into(), true),
                }
            }

            let all_pass = gates.iter().all(|(_, _, ok)| *ok);
            if all_pass {
                src_of.insert(name.clone(), src.clone());
            }
            // Support-part count: the entity's expansion children (each
            // `-> Name:` instantiation inside the expansion block).
            let support_parts = count_expansion_children(&src, &name);
            let loss_params = Some((
                is_switcher,
                attr_si("rds_on").unwrap_or(0.0),
                attr_si("f_sw").or_else(|| attr_si("switching_frequency")).unwrap_or(0.0),
                attr_si("t_sw").unwrap_or(0.0),
                i_q.unwrap_or(0.0),
            ));
            survey.push(Candidate {
                part: name,
                gates,
                loss_w: if all_pass { loss } else { None },
                support_parts,
                chosen: false,
                loss_params,
                ic_price: None,
                ic_mpn: None,
                ic_sku: None,
                support_cost: None,
                unpriced_parts: 0,
            });
        }
    }

    // Price the qualifiers through the supplier plugin (all part selection
    // goes through plugins — no direct catalogue coupling here): cheapest
    // in-stock MPN with the part-name prefix. Missing provider/DB ⇒ prices
    // stay None and the ranking falls back to part count (stated below).
    let prefer = format!("{v_out:.1}V");
    for c in survey.iter_mut().filter(|c| c.loss_w.is_some()) {
        if let Some(sel) = price_via_provider(&c.part, &prefer) {
            c.ic_price = sel.0;
            c.ic_mpn = sel.1;
            c.ic_sku = sel.2;
        }
        // Support parts: resolve the expansion children to (class, value)
        // using literals, constructor defaults, and the seed closed forms,
        // then price them through the provider's passive path in one batch.
        if let Some(src) = src_of.get(&c.part) {
            let (reqs, unresolved) = support_part_values(
                src, &c.part, v_in, v_out, i_out, ripple_max,
            );
            c.unpriced_parts = unresolved;
            if !reqs.is_empty() {
                let (total, unpriced) = price_supports(&reqs);
                c.support_cost = total;
                c.unpriced_parts += unpriced;
            } else {
                c.support_cost = Some(0.0);
            }
        }
    }

    // Rank the survivors per the requested profile:
    //   cost     → real regulator price (support parts count then loss as
    //              tiebreaks; support-part PRICING is the remaining S3 gap);
    //   balanced / grade → lowest loss, then fewest parts.
    let key = |c: &Candidate| -> (f64, f64, f64) {
        let loss = c.loss_w.unwrap_or(f64::MAX);
        match profile {
            "cost" => (
                match (c.ic_price, c.support_cost) {
                    // Total BOM money when both sides priced; IC-only and
                    // unpriced candidates rank after fully-priced ones.
                    (Some(ic), Some(sup)) => ic + sup,
                    (Some(ic), None) => ic + 1e3,
                    _ => f64::MAX,
                },
                c.unpriced_parts as f64 * 1e3 + c.support_parts as f64,
                loss,
            ),
            _ => (loss, c.support_parts as f64, 0.0),
        }
    };
    let winner = survey
        .iter()
        .filter(|c| c.loss_w.is_some())
        .min_by(|a, b| key(a).partial_cmp(&key(b)).unwrap_or(std::cmp::Ordering::Equal))
        .map(|c| c.part.clone());
    match winner {
        Some(w) => {
            for c in survey.iter_mut() {
                c.chosen = c.part == w;
            }
            Ok((w, survey))
        }
        None => {
            let mut msg = String::from(
                "supply: no stdlib regulator passes every capability gate for this \
                 requirement. Survey:\n",
            );
            for c in &survey {
                let first_fail = c
                    .gates
                    .iter()
                    .find(|(_, _, ok)| !ok)
                    .map(|(g, d, _)| format!("{g}: {d}"))
                    .unwrap_or_else(|| "?".into());
                msg.push_str(&format!("  {} — REJECT: {}\n", c.part, first_fail));
            }
            bail!(msg)
        }
    }
}

/// Resolve an entity's expansion children to priceable (class, value-SI)
/// requirements. Values come from, in order: a numeric literal in the
/// instantiation (`Cap(100nF)`), the referenced constructor default
/// (`Cap(c_in)` → the `c_in` param's default), or the SEED closed forms for
/// the design-block outputs (`Ind(l_value)`, `Cap(c_out_value)`,
/// `Res(r_top_value)`), computed from the operating point exactly as the
/// design block will. Unresolvable children (diodes, exotic classes,
/// unmatched refs) are counted, not guessed.
fn support_part_values(
    src: &str,
    name: &str,
    v_in: f64,
    v_out: f64,
    i_out: f64,
    ripple_max: Option<f64>,
) -> (Vec<(String, f64)>, usize) {
    let attrs = entity_attrs_txt(src, name);
    let num_attr = |k: &str| {
        attrs.get(k).and_then(|v| {
            parse_si_txt(v).or_else(|| {
                entity_param_default(src, name, v.trim()).and_then(|d| parse_si_txt(&d))
            })
        })
    };
    let f_sw = num_attr("f_sw")
        .or_else(|| num_attr("switching_frequency"))
        .unwrap_or(0.0);
    let duty = if v_in > 0.0 { v_out / v_in } else { 0.0 };
    let d_il = 0.3 * i_out;
    let v_ref = num_attr("feedback_voltage").unwrap_or(0.0);
    let r_bot = entity_param_default(src, name, "r_fb_bot")
        .and_then(|d| parse_si_txt(&d))
        .unwrap_or(0.0);

    // Seed closed forms keyed by the design-output variable names the stdlib
    // expansion blocks reference.
    let seed = |ident: &str| -> Option<f64> {
        match ident {
            "l_value" if f_sw > 0.0 && d_il > 0.0 => {
                Some((v_in - v_out) * duty / (f_sw * d_il))
            }
            "c_out_value" | "c_out" if f_sw > 0.0 => {
                let dv = ripple_max.unwrap_or(0.05);
                Some(d_il / (8.0 * f_sw * dv))
            }
            "c_in_value" | "c_in" if f_sw > 0.0 => {
                Some(i_out * duty * (1.0 - duty) / (f_sw * 0.15))
            }
            "r_top_value" if v_ref > 0.0 && r_bot > 0.0 => {
                Some(r_bot * (v_out - v_ref) / v_ref)
            }
            "r_fb_bot" => Some(r_bot).filter(|r| *r > 0.0),
            _ => None,
        }
    };

    let Some(at) = find_entity_decl(src, name) else { return (Vec::new(), 0) };
    let mut reqs = Vec::new();
    let mut unresolved = 0usize;
    let mut in_exp = false;
    for line in src[at..].lines() {
        let l = line.split("//").next().unwrap_or("").trim();
        if l.starts_with("expansion {") || l == "expansion" {
            in_exp = true;
            continue;
        }
        if !in_exp {
            continue;
        }
        if l == "}" {
            break;
        }
        // Each `Name: Type(arg…)` instantiation on the line.
        let mut rest = l;
        while let Some(pos) = rest.find(": ") {
            let after = &rest[pos + 2..];
            let ty: String = after
                .chars()
                .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
                .collect();
            let tail = &after[ty.len()..];
            if !tail.starts_with('(') || ty.is_empty()
                || !ty.chars().next().unwrap().is_ascii_uppercase()
            {
                rest = &rest[pos + 2..];
                continue;
            }
            let arg: String = tail[1..]
                .chars()
                .take_while(|ch| *ch != ',' && *ch != ')')
                .collect();
            let arg = arg.trim();
            let class = match ty.as_str() {
                "Cap" | "Capacitor" => Some("capacitor"),
                "Res" | "Resistor" => Some("resistor"),
                "Ind" | "Inductor" => Some("inductor"),
                _ => None,
            };
            match class {
                Some(cls) => {
                    // Literals/defaults are already standard values; SEED
                    // closed-form outputs are raw numbers and must be snapped
                    // to the E-series grid before pricing — the catalogue's
                    // value window (rightly) rejects a 31.25kΩ or 65µH that
                    // no one manufactures. Same snap the design flow applies.
                    let value = parse_si_txt(arg)
                        .or_else(|| {
                            entity_param_default(src, name, arg)
                                .and_then(|d| parse_si_txt(&d))
                        })
                        .or_else(|| {
                            seed(arg).map(|v| {
                                if cls == "resistor" {
                                    e_series_nearest(v, 96)
                                } else {
                                    e_series_nearest(v, 12)
                                }
                            })
                        });
                    match value.filter(|v| *v > 0.0) {
                        Some(v) => reqs.push((cls.to_string(), v)),
                        None => unresolved += 1,
                    }
                }
                None => unresolved += 1,
            }
            rest = &rest[pos + 2..];
        }
    }
    (reqs, unresolved)
}

/// Format a value in engineering notation for generated BHDL source —
/// `2.2e-5, "F"` → `"22uF"`, `1e4, ""` → `"10k"` (bare form for resistors,
/// matching the `Res(4.7k)` fixture idiom). Uses `u` (not `µ`) for micro.
fn fmt_si(v: f64, unit: &str) -> String {
    let (scale, prefix) = if v >= 1e6 {
        (1e6, "M")
    } else if v >= 1e3 {
        (1e3, "k")
    } else if v >= 1.0 {
        (1.0, "")
    } else if v >= 1e-3 {
        (1e-3, "m")
    } else if v >= 1e-6 {
        (1e-6, "u")
    } else if v >= 1e-9 {
        (1e-9, "n")
    } else {
        (1e-12, "p")
    };
    let n = v / scale;
    // Up to 3 significant digits, trailing zeros trimmed.
    let s = format!("{n:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("{s}{prefix}{unit}")
}

/// Nearest standard E-series value (E12 for reactives, E96 for resistors) —
/// the pricing-side analogue of the design flow's snap stage.
fn e_series_nearest(v: f64, series: u32) -> f64 {
    if !(v > 0.0) {
        return v;
    }
    let decade = 10f64.powf(v.log10().floor());
    let mut best = v;
    let mut best_err = f64::MAX;
    // E12/E24 use the HISTORICAL preferred-number tables, not the rounded
    // geometric grid — IEC 60063 kept pre-war values (2.7, 3.3, 3.9, 4.7,
    // 8.2) where naive rounding gives 2.6/3.2/3.8/4.6/8.3. Generated source
    // must name values an engineer can actually buy. E96 has no such
    // exceptions: 3-significant-digit rounding of the grid IS the table.
    const E12: [f64; 12] =
        [1.0, 1.2, 1.5, 1.8, 2.2, 2.7, 3.3, 3.9, 4.7, 5.6, 6.8, 8.2];
    const E24: [f64; 24] = [
        1.0, 1.1, 1.2, 1.3, 1.5, 1.6, 1.8, 2.0, 2.2, 2.4, 2.7, 3.0,
        3.3, 3.6, 3.9, 4.3, 4.7, 5.1, 5.6, 6.2, 6.8, 7.5, 8.2, 9.1,
    ];
    let table: Vec<f64> = match series {
        12 => E12.to_vec(),
        24 => E24.to_vec(),
        n => (0..n)
            .map(|k| {
                let b = 10f64.powf(k as f64 / n as f64);
                (b * 100.0).round() / 100.0
            })
            .collect(),
    };
    for dec in [decade / 10.0, decade, decade * 10.0] {
        for base in &table {
            let cand = base * dec;
            let err = ((cand - v) / v).abs();
            if err < best_err {
                best_err = err;
                best = cand;
            }
        }
    }
    best
}

/// Price a batch of (class, value-SI) support parts through the provider's
/// passive path. Returns (sum of unit prices when at least one priced,
/// count that came back unpriced).
fn price_supports(reqs: &[(String, f64)]) -> (Option<f64>, usize) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("bhdl-jlcparts-provider")))
        .filter(|p| p.exists())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "bhdl-jlcparts-provider".to_string());
    let requirements: Vec<serde_json::Value> = reqs
        .iter()
        .enumerate()
        .map(|(i, (cls, v))| {
            serde_json::json!({"class_index": i, "class": cls, "value": v})
        })
        .collect();
    let req = serde_json::json!({"protocol": 1, "requirements": requirements});
    let run = || -> Option<serde_json::Value> {
        let mut child = Command::new(&exe)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        child.stdin.take()?.write_all(req.to_string().as_bytes()).ok()?;
        let out = child.wait_with_output().ok()?;
        serde_json::from_slice(&out.stdout).ok()
    };
    let Some(v) = run() else { return (None, reqs.len()) };
    let empty = Vec::new();
    let sels = v
        .get("selections")
        .and_then(|s| s.as_array())
        .unwrap_or(&empty);
    let mut total = 0.0;
    let mut priced = 0usize;
    for sel in sels {
        if let Some(p) = sel.get("unit_price").and_then(|p| p.as_f64()) {
            total += p;
            priced += 1;
        }
    }
    if priced == 0 {
        (None, reqs.len())
    } else {
        (Some(total), reqs.len() - priced)
    }
}

/// Price a regulator through the bundled jlcparts provider's `mpn_query`
/// mode. Provider binary resolved next to the current executable (the
/// normal target-dir layout), else on PATH; the provider finds the in-tree
/// DB itself (its own walk-up). Returns (unit_price, mpn, lcsc_sku).
fn price_via_provider(
    part: &str,
    prefer: &str,
) -> Option<(Option<f64>, Option<String>, Option<String>)> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("bhdl-jlcparts-provider")))
        .filter(|p| p.exists())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "bhdl-jlcparts-provider".to_string());
    let req = serde_json::json!({
        "protocol": 1,
        "requirements": [{
            "class_index": 0, "class": "ic",
            "mpn_query": part, "mpn_prefer": prefer
        }]
    });
    let mut child = Command::new(&exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child
        .stdin
        .take()?
        .write_all(req.to_string().as_bytes())
        .ok()?;
    let out = child.wait_with_output().ok()?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let sel = v.get("selections")?.get(0)?;
    if sel.get("error").is_some() {
        return None;
    }
    Some((
        sel.get("unit_price").and_then(|p| p.as_f64()),
        sel.get("mpn").and_then(|m| m.as_str()).map(str::to_string),
        sel.get("vendor_sku").and_then(|m| m.as_str()).map(str::to_string),
    ))
}

/// Report design curves (S3): computed from the same closed forms the
/// chooser and the part's design block use — SEED values, labelled as such
/// (sign-off checks the as-built snapped values; these curves show the
/// design space around the chosen point).
fn build_curves(
    survey: &[Candidate],
    part: &str,
    v_in: f64,
    v_out: f64,
    i_out: f64,
    ripple_max: Option<f64>,
) -> Vec<(String, String, String, Vec<(f64, f64)>, String)> {
    let Some(c) = survey.iter().find(|c| c.part == part) else { return Vec::new() };
    let Some((is_sw, rds, f_sw, t_sw, i_q)) = c.loss_params else { return Vec::new() };
    if !(v_in > 0.0 && v_out > 0.0 && i_out > 0.0) {
        return Vec::new();
    }
    let duty = v_out / v_in;
    let mut curves = Vec::new();

    // Efficiency vs load — the loss model swept over 20…100 % load.
    let loss_at = |i: f64| -> f64 {
        if is_sw {
            i * i * rds * duty + v_in * i * f_sw * t_sw + v_in * i_q
        } else {
            (v_in - v_out) * i + v_in * i_q
        }
    };
    // 25 samples 4%…100% load — dense enough for a smooth SVG polyline;
    // the report table prints every 5th row for exact numbers.
    let pts: Vec<(f64, f64)> = (1..=25)
        .map(|k| {
            let i = (k as f64 / 25.0) * i_out;
            let p = loss_at(i);
            (i, 100.0 * (v_out * i) / (v_out * i + p))
        })
        .collect();
    curves.push((
        format!("Estimated efficiency vs load — {part}"),
        "I_load (A)".into(),
        "η (%)".into(),
        pts,
        if is_sw {
            "loss model I²·R_ds·D + V·I·f_sw·t_sw + V·I_q (datasheet attrs); \
             seed closed form — sign-off gates the as-built values"
                .into()
        } else {
            "linear P = (V_IN−V_OUT)·I + V_IN·I_q; efficiency is topology-bound \
             at V_OUT/V_IN"
                .into()
        },
    ));

    // Output ripple vs C_out (switchers with a ripple budget): ΔV = ΔI_L /
    // (8·f_sw·C), ΔI_L at the seed ripple ratio 0.3·I_out.
    if is_sw && f_sw > 0.0 {
        if let Some(dv_max) = ripple_max {
            let d_il = 0.3 * i_out;
            let c_req = d_il / (8.0 * f_sw * dv_max);
            let pts: Vec<(f64, f64)> = (0..25)
                .map(|k| {
                    let mult = 0.5 + 1.5 * (k as f64 / 24.0);
                    let cap = mult * c_req;
                    (cap * 1e6, 1000.0 * d_il / (8.0 * f_sw * cap))
                })
                .collect();
            curves.push((
                format!("Output ripple vs C_out — {part}"),
                "C_out (µF)".into(),
                "ΔV (mV)".into(),
                pts,
                format!(
                    "ΔV = ΔI_L/(8·f_sw·C), ΔI_L = 0.3·I_out = {d_il:.2}A seed; \
                     spec ≤ {:.0}mV ⇒ C_out ≥ {:.1}µF (design block sizes, \
                     sign-off verifies the snapped value)",
                    dv_max * 1000.0,
                    c_req * 1e6
                ),
            ));
        }
    }
    curves
}

/// Number of instantiations inside the entity's `expansion { }` block
/// (`X -> Name: Type(...)` lines) — the support-part BOM-size proxy.
fn count_expansion_children(src: &str, name: &str) -> usize {
    let Some(at) = find_entity_decl(src, name) else { return 0 };
    let mut in_exp = false;
    let mut n = 0usize;
    for line in src[at..].lines() {
        let l = line.split("//").next().unwrap_or("").trim();
        if l.starts_with("expansion {") || l == "expansion" {
            in_exp = true;
            continue;
        }
        if in_exp {
            if l == "}" {
                break;
            }
            // Each `Name: Type(` instantiation (standalone or mid-flow):
            // count only `: ` followed by an Uppercase type ident that is
            // immediately called with `(` — this excludes intent-annotation
            // args (`for filter(rail: VIN, …)`) and value fields, which
            // previously inflated the count and mis-ranked the cost profile.
            let bytes = l.as_bytes();
            let mut i = 0usize;
            while let Some(rel) = l[i..].find(": ") {
                let mut j = i + rel + 2;
                while j < bytes.len() && bytes[j] == b' ' {
                    j += 1;
                }
                if j < bytes.len() && bytes[j].is_ascii_uppercase() {
                    let mut k = j;
                    while k < bytes.len()
                        && (bytes[k].is_ascii_alphanumeric() || bytes[k] == b'_')
                    {
                        k += 1;
                    }
                    if k < bytes.len() && bytes[k] == b'(' {
                        n += 1;
                    }
                }
                i += rel + 2;
            }
        }
        if l.starts_with("entity ") && n > 0 {
            break;
        }
    }
    n
}

fn fmt_a(v: f64) -> String {
    if v >= 1.0 {
        format!("{v:.2}A")
    } else if v >= 1e-3 {
        format!("{:.1}mA", v * 1e3)
    } else {
        format!("{:.0}µA", v * 1e6)
    }
}

/// Byte offset of `entity <name>` in `src`, resolving one level of
/// `alias <name> = <Target>` indirection (SKU aliases).
/// Textual `entity X(...) as part|design {` partness (chooser runs pre-parse).
/// Follows the alias hop of `find_entity_decl`. None = undeclared.
fn entity_declared_kind(src: &str, name: &str) -> Option<String> {
    let at = find_entity_decl(src, name)?;
    let head = &src[at..];
    let head = &head[..head.find('{').unwrap_or(head.len())];
    let after_as = head.rsplit(" as ").next()?;
    if head.contains(" as ") {
        let kind: String = after_as.trim().chars().take_while(|c| c.is_alphabetic()).collect();
        if kind == "part" || kind == "design" {
            return Some(kind);
        }
    }
    None
}

fn find_entity_decl(src: &str, name: &str) -> Option<usize> {
    let pat = format!("entity {name}");
    let mut off = 0usize;
    for line in src.split_inclusive('\n') {
        let l = line.trim_start();
        if l.starts_with(&pat)
            && l[pat.len()..]
                .chars()
                .next()
                .map(|c| c == '(' || c == '<' || c == ' ' || c == '{')
                .unwrap_or(false)
        {
            return Some(off + (line.len() - l.len()));
        }
        off += line.len();
    }
    // Alias hop: `alias <name> = Target(…)` / `= Target<…>` / `= Target;`
    let apat = format!("alias {name} ");
    for line in src.lines() {
        let l = line.trim_start();
        if let Some(rest) = l.strip_prefix(&apat) {
            let rhs = rest.trim_start_matches('=').trim();
            let target: String = rhs
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !target.is_empty() && target != name {
                return find_entity_decl(src, &target);
            }
        }
    }
    None
}

/// Render one design curve as a self-contained inline SVG line chart for the
/// synthesis report (GitHub-flavored Markdown renders inline SVG). Clean
/// engineer style: light grid, labeled ticks, a single polyline with point
/// markers. Pure data in, one string out — no external assets.
pub fn curve_svg(title: &str, xl: &str, yl: &str, pts: &[(f64, f64)]) -> String {
    if pts.len() < 2 {
        return String::new();
    }
    let (w, h) = (640.0, 320.0);
    let (ml, mr, mt, mb) = (64.0, 16.0, 34.0, 44.0); // margins
    let (pw, ph) = (w - ml - mr, h - mt - mb);
    let (mut x0, mut x1) = (f64::MAX, f64::MIN);
    let (mut y0, mut y1) = (f64::MAX, f64::MIN);
    for &(x, y) in pts {
        x0 = x0.min(x); x1 = x1.max(x);
        y0 = y0.min(y); y1 = y1.max(y);
    }
    if !(x1 > x0) { x1 = x0 + 1.0; }
    // A little vertical headroom so the trace doesn't hug the frame.
    let pad = ((y1 - y0) * 0.08).max(y1.abs() * 1e-6).max(1e-12);
    y0 -= pad; y1 += pad;
    let sx = |x: f64| ml + (x - x0) / (x1 - x0) * pw;
    let sy = |y: f64| mt + (1.0 - (y - y0) / (y1 - y0)) * ph;
    let fmt = |v: f64| -> String {
        let a = v.abs();
        if a >= 100.0 { format!("{v:.0}") }
        else if a >= 1.0 { format!("{v:.1}") }
        else { format!("{v:.3}") }
    };
    let esc = |t: &str| t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");

    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" \
         width=\"{w}\" height=\"{h}\" role=\"img\" aria-label=\"{}\">\n\
         <style>text{{font:12px sans-serif;fill:#444}}.t{{font:13px sans-serif;font-weight:600;fill:#222}}</style>\n\
         <rect x=\"0\" y=\"0\" width=\"{w}\" height=\"{h}\" fill=\"white\"/>\n\
         <text class=\"t\" x=\"{}\" y=\"20\" text-anchor=\"middle\">{}</text>\n",
        esc(title), w / 2.0, esc(title)
    );
    // Grid + ticks (5 divisions each way).
    for k in 0..=5 {
        let fx = x0 + (x1 - x0) * k as f64 / 5.0;
        let px = sx(fx);
        svg += &format!(
            "<line x1=\"{px:.1}\" y1=\"{mt}\" x2=\"{px:.1}\" y2=\"{:.1}\" stroke=\"#e5e5e5\"/>\n\
             <text x=\"{px:.1}\" y=\"{:.1}\" text-anchor=\"middle\">{}</text>\n",
            mt + ph, mt + ph + 16.0, fmt(fx)
        );
        let fy = y0 + (y1 - y0) * k as f64 / 5.0;
        let py = sy(fy);
        svg += &format!(
            "<line x1=\"{ml}\" y1=\"{py:.1}\" x2=\"{:.1}\" y2=\"{py:.1}\" stroke=\"#e5e5e5\"/>\n\
             <text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\">{}</text>\n",
            ml + pw, ml - 6.0, py + 4.0, fmt(fy)
        );
    }
    // Axes frame + labels.
    svg += &format!(
        "<rect x=\"{ml}\" y=\"{mt}\" width=\"{pw}\" height=\"{ph}\" fill=\"none\" stroke=\"#999\"/>\n\
         <text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\">{}</text>\n\
         <text x=\"14\" y=\"{:.1}\" text-anchor=\"middle\" transform=\"rotate(-90 14 {:.1})\">{}</text>\n",
        ml + pw / 2.0, h - 8.0, esc(xl), mt + ph / 2.0, mt + ph / 2.0, esc(yl)
    );
    // The trace.
    let path: Vec<String> = pts.iter().map(|&(x, y)| format!("{:.1},{:.1}", sx(x), sy(y))).collect();
    svg += &format!(
        "<polyline points=\"{}\" fill=\"none\" stroke=\"#2266cc\" stroke-width=\"2\"/>\n",
        path.join(" ")
    );
    for &(x, y) in pts {
        svg += &format!(
            "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"2.4\" fill=\"#2266cc\"/>\n",
            sx(x), sy(y)
        );
    }
    svg += "</svg>";
    svg
}
