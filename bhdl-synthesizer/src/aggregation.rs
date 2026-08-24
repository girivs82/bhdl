//! PMIC AGGREGATION — the resolver's post-step (docs/spec/
//! Requirements_And_Resolution.md §8).
//!
//! Per-rail resolution runs FIRST, exactly as before: one stage per
//! rail, each independently surveyed, priced and bound. THEN this pass
//! asks whether one multi-output part could cover a SET of those
//! requirements — the question a per-rail greedy survey structurally
//! cannot see, because a PMIC loses to a $0.40 buck on every single
//! rail and wins on the set.
//!
//! A multi-output block declares its capability table as data:
//!   attribute pmic_outputs = "DCDC1:buck:1.8V:1.2A,LDO2:ldo:3.3V:0.1A,…";
//!   attribute pmic_seq     = "LDO1,DCDC1,…";   // built-in power-up order
//! (fixed voltages — the honest model of an OTP part). The evaluation:
//! each resolved requirement matches an unused output when the fixed
//! voltage equals the requirement's vout (within 2 %, the parts' own
//! accuracy class), the derated requirement current fits the output's
//! rating, and the requirement's input rail lies inside the PMIC's
//! input range. Greedy assignment, outputs never reused.
//!
//! This increment REPORTS the option — coverage, leftover rails, the
//! price of the PMIC silicon vs the Σ of the bound discrete stages,
//! and the built-in sequencing order (informational; the strict
//! sequencing gate binds when an aggregation COMMIT lands — a future
//! increment, stated). Nothing is auto-committed: the comparison is
//! the designer's lever.

use std::collections::HashMap;
use std::path::Path;

use crate::stage_resolution::StageResolution;
use crate::supply_synthesis::{collect_bhdl, entity_attrs_txt, parse_si_txt};

#[derive(Debug, Clone)]
struct PmicOutput {
    name: String,
    topology: String,
    vout: f64,
    i_max: f64,
}

#[derive(Debug, Clone)]
struct PmicBlock {
    block: String,
    part_number: Option<String>,
    vin_min: Option<f64>,
    vin_max: Option<f64>,
    outputs: Vec<PmicOutput>,
    seq: Option<String>,
    seq_dly: Option<String>,
}


/// GROUPED COMMIT (spec §8.1): `resolve u1, u2, u3 = Pmic_X;` — the
/// designer's lever the aggregation report points at. A TEXT transform
/// (the resolver's discipline) applied BEFORE per-rail scanning:
///
/// - every named instance's requirement must match an unused PMIC
///   output (same gates as the report: fixed voltage within the 2 %
///   accuracy class, derated current within the rating) — a miss is a
///   HARD ERROR naming it;
/// - the FIRST instance's `Trait(args)` span becomes `Pmic_X()`; the
///   other instances' instantiation statements are removed;
/// - every endpoint reference is remapped: `uk.VOUT` →
///   `<first>.VOUT_<output>`, and for the non-first instances
///   `uk.VIN`/`uk.GND` → `<first>.VIN`/`<first>.GND` (duplicate
///   identical connections are benign — same endpoints, same net);
/// - a per-rail `uk.EN`/`uk.PG` reference is a HARD ERROR: the
///   SEQUENCER owns the enables on a PMIC (wire PWR_EN instead);
/// - `<first>.PWR_EN` is tied to the feed rail when the board leaves
///   it unwired (the sequencer starts with the supply — stated);
/// - the committed mapping is stamped as a scoped attribute
///   (`pmic_committed`) so ERC033's strict sequencing gate and the
///   report can read it on the flattened netlist.
///
/// Returns the transformed source + report lines; Ok(None) when the
/// source has no grouped resolve.
pub fn apply_group_overrides(
    source: &str,
    stdlib_root: &Path,
) -> Result<Option<(String, Vec<String>)>, String> {
    let masked = crate::stage_resolution::mask_comments(source);
    // scan `resolve a, b[, …] = Block;` (comma = grouped; single-instance
    // stays with the per-rail override path)
    struct Group {
        span: (usize, usize),
        members: Vec<String>,
        block: String,
    }
    let mut groups: Vec<Group> = Vec::new();
    let mut off = 0usize;
    while let Some(pfound) = masked[off..].find("resolve ") {
        let at = off + pfound;
        off = at + 8;
        let prev = masked[..at].trim_end().chars().last();
        if !matches!(prev, None | Some(';') | Some('{') | Some('}')) {
            continue;
        }
        let Some(semi) = masked[at..].find(';') else { continue };
        let body = &masked[at + 8..at + semi];
        let Some((lhs, rhs)) = body.split_once('=') else { continue };
        if !lhs.contains(',') {
            continue; // per-rail override, not ours
        }
        let members: Vec<String> = lhs.split(',').map(|m| m.trim().to_string()).collect();
        let ok = |t: &str| !t.is_empty() && t.chars().all(|c| c.is_alphanumeric() || c == '_');
        if !members.iter().all(|m| ok(m)) {
            continue;
        }
        let block = rhs.trim().trim_end_matches("()").trim().to_string();
        if !ok(&block) {
            return Err(format!("grouped resolve: `{}` — a grouped commit takes a bare block name (its rails are OTP-fixed; there are no ctor args to pass)", body.trim()));
        }
        groups.push(Group { span: (at, at + semi + 1), members, block });
    }
    if groups.is_empty() {
        return Ok(None);
    }

    let pmics = scan_pmics(stdlib_root);
    let mut out = source.to_string();
    let mut notes = Vec::new();
    // apply back-to-front so spans stay valid
    for g in groups.iter().rev() {
        let Some(pmic) = pmics.iter().find(|p| p.block == g.block) else {
            return Err(format!(
                "resolve {} = {}: no multi-output block of that name declares `pmic_outputs` in the library",
                g.members.join(", "), g.block
            ));
        };
        // locate each member's requirement instantiation `m: Trait(args)`
        struct Member {
            name: String,
            trait_span: (usize, usize), // `Trait(args)` span in `out`
            stmt_span: (usize, usize),  // enclosing statement in `out`
            vout: f64,
            imax: f64,
        }
        let cur_masked = crate::stage_resolution::mask_comments(&out);
        let mut mems: Vec<Member> = Vec::new();
        for m in &g.members {
            let pat = format!("{m}");
            let mut found = None;
            let mut o2 = 0usize;
            while let Some(pp) = cur_masked[o2..].find(&pat) {
                let a2 = o2 + pp;
                o2 = a2 + pat.len();
                // ident boundaries + `name :` shape
                let before_ok = a2 == 0 || !cur_masked[..a2].chars().last().map(|c| c.is_alphanumeric() || c == '_').unwrap_or(false);
                let rest = cur_masked[a2 + pat.len()..].trim_start();
                if !before_ok || !rest.starts_with(':') {
                    continue;
                }
                let after_colon = &cur_masked[a2 + pat.len()..];
                let ci = after_colon.find(':').unwrap();
                let tstart = a2 + pat.len() + ci + 1;
                let ttxt = cur_masked[tstart..].trim_start();
                let lead = cur_masked[tstart..].len() - ttxt.len();
                let tname: String = ttxt.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                if !tname.ends_with("Stage") {
                    continue;
                }
                let Some(op) = ttxt.find('(') else { continue };
                let Some(cl) = ttxt.find(')') else { continue };
                let args = &ttxt[op + 1..cl];
                let get = |k: &str| -> Option<f64> {
                    args.split(',')
                        .filter_map(|a| a.trim().split_once('='))
                        .find(|(kk, _)| kk.trim() == k)
                        .and_then(|(_, v)| parse_si_txt(v.trim()))
                };
                let (Some(v), Some(i)) = (get("vout"), get("i_max")) else {
                    return Err(format!("resolve group: {m}'s requirement lacks vout/i_max named args (grouped commits need them named)"));
                };
                // enclosing statement
                let stmt_start = cur_masked[..a2].rfind(|c| c == ';' || c == '{').map(|x| x + 1).unwrap_or(0);
                let stmt_end = tstart + lead + cl + 1
                    + cur_masked[tstart + lead + cl + 1..].find(';').map(|x| x + 1).unwrap_or(0);
                found = Some(Member {
                    name: m.clone(),
                    trait_span: (tstart + lead, tstart + lead + cl + 1),
                    stmt_span: (stmt_start, stmt_end),
                    vout: v,
                    imax: i,
                });
                break;
            }
            let Some(mm) = found else {
                return Err(format!("resolve {} = {}: '{m}' has no stage-requirement instantiation in this board", g.members.join(", "), g.block));
            };
            mems.push(mm);
        }
        // cover: every member must land on an unused output
        let mut used: Vec<usize> = Vec::new();
        let mut mapping: Vec<(String, String)> = Vec::new();
        for m in &mems {
            let slot = pmic.outputs.iter().enumerate().find(|(i, o)| {
                !used.contains(i)
                    && (o.vout - m.vout).abs() <= 0.02 * m.vout + 1e-9
                    && o.i_max + 1e-9 >= m.imax / 0.8
            });
            match slot {
                Some((i, o)) => {
                    used.push(i);
                    mapping.push((m.name.clone(), o.name.clone()));
                }
                None => {
                    return Err(format!(
                        "resolve {} = {}: '{}' ({}V @ {}A ⇒ rating ≥ {:.2}A) matches NO unused output of {} — the commit is refused (outputs: {})",
                        g.members.join(", "), g.block, m.name, m.vout, m.imax, m.imax / 0.8, g.block,
                        pmic.outputs.iter().map(|o| format!("{}:{:.2}V/{:.1}A", o.name, o.vout, o.i_max)).collect::<Vec<_>>().join(", ")
                    ));
                }
            }
        }
        // per-rail EN/PG references are the sequencer's — hard error
        for m in &g.members {
            for pin in ["EN", "PG"] {
                if cur_masked.contains(&format!("{m}.{pin}")) {
                    return Err(format!(
                        "resolve group: '{m}.{pin}' is wired, but on a PMIC commit the SEQUENCER owns the enables/power-good — wire {}.PWR_EN instead and drop the per-rail {pin}",
                        g.members[0]
                    ));
                }
            }
        }
        let first = &g.members[0];
        // text surgery, back-to-front by span start
        struct Edit {
            span: (usize, usize),
            text: String,
        }
        let mut edits: Vec<Edit> = vec![Edit { span: g.span, text: String::new() }];
        for (k, m) in mems.iter().enumerate() {
            if k == 0 {
                edits.push(Edit { span: m.trait_span, text: format!("{}()", g.block) });
            } else {
                edits.push(Edit { span: m.stmt_span, text: String::new() });
            }
        }
        edits.sort_by(|a, b| b.span.0.cmp(&a.span.0));
        for e in &edits {
            out.replace_range(e.span.0..e.span.1, &e.text);
        }
        // endpoint remap (post-surgery text)
        for (m, o) in &mapping {
            out = out.replace(&format!("{m}.VOUT"), &format!("{first}.VOUT_{o}"));
        }
        for m in g.members.iter().skip(1) {
            out = out.replace(&format!("{m}.VIN"), &format!("{first}.VIN"));
            out = out.replace(&format!("{m}.GND"), &format!("{first}.GND"));
        }
        // PWR_EN tie when unwired: sequencer starts with the supply
        if !out.contains(&format!("{first}.PWR_EN")) {
            // feed rail from the first member's VIN wiring `@X -> first`
            let feed = out
                .find(&format!("-> {first}:"))
                .and_then(|pos| {
                    let head = out[..pos].rfind('@')?;
                    let r: String = out[head + 1..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                    (!r.is_empty()).then_some(r)
                });
            if let Some(feed) = feed {
                if let Some(stmt_end) = out.find(&format!("{first}: {}()", g.block)).and_then(|p| out[p..].find(';').map(|x| p + x + 1)) {
                    out.insert_str(stmt_end, &format!("\n    @{feed} -> {first}.PWR_EN; // sequencer starts with the supply (grouped commit)"));
                    notes.push(format!("{first}.PWR_EN tied to @{feed} (unwired — the sequencer starts with the supply, stated)"));
                }
            } else {
                notes.push(format!("{first}.PWR_EN left unwired — wire it to your enable source"));
            }
        }
        // stamp the committed mapping for ERC033's strict gate + reports
        if let Some(stmt_end) = out.find(&format!("{first}: {}()", g.block)).and_then(|p| out[p..].find(';').map(|x| p + x + 1)) {
            let map_txt = mapping.iter().map(|(m, o)| format!("{m}:{o}")).collect::<Vec<_>>().join(",");
            out.insert_str(stmt_end, &format!("\n    attribute {first}.pmic_committed = \"{map_txt}\";"));
        }
        notes.insert(0, format!(
            "GROUPED COMMIT: {} = {} — {}",
            g.members.join(" + "), g.block,
            mapping.iter().map(|(m, o)| format!("{m}→{o}")).collect::<Vec<_>>().join(", ")
        ));
        if let Some(seq) = &pmic.seq {
            notes.push(format!("built-in power-up order now governs these rails: {seq} — ERC033 verifies it against the declared domain ordering"));
        }
        // import for the block
        let lib_file = find_block_file(stdlib_root, &g.block);
        if let Some(rel) = lib_file {
            let already = out.lines().any(|l| l.trim_start().starts_with("import") && l.contains(&g.block));
            if !already {
                let mut insert_at = 0usize;
                let mut o3 = 0usize;
                for line in out.split_inclusive('\n') {
                    if line.trim_start().starts_with("import ") {
                        insert_at = o3 + line.len();
                    }
                    o3 += line.len();
                }
                out.insert_str(insert_at, &format!("import {{ {} }} from \"{rel}\";\n", g.block));
            }
        }
    }
    Ok(Some((out, notes)))
}

fn find_block_file(stdlib_root: &Path, block: &str) -> Option<String> {
    let mut files = Vec::new();
    collect_bhdl(stdlib_root, &mut files);
    files.sort();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else { continue };
        if text.contains(&format!("entity {block}")) {
            // library-relative path (bhdl-stdlib/…)
            let s = f.to_string_lossy();
            if let Some(idx) = s.find("bhdl-stdlib") {
                return Some(s[idx..].to_string());
            }
            return Some(s.to_string());
        }
    }
    None
}

fn scan_pmics(stdlib_root: &Path) -> Vec<PmicBlock> {
    let mut files = Vec::new();
    collect_bhdl(stdlib_root, &mut files);
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else { continue };
        if !text.contains("pmic_outputs") {
            continue;
        }
        // every `entity <X>` in the file that declares pmic_outputs
        let mut off = 0usize;
        while let Some(p) = text[off..].find("entity ") {
            let at = off + p;
            off = at + 7;
            let name: String = text[at + 7..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() {
                continue;
            }
            let attrs = entity_attrs_txt(&text, &name);
            let Some(tbl) = attrs.get("pmic_outputs") else { continue };
            let outputs: Vec<PmicOutput> = tbl
                .trim_matches('"')
                .split(',')
                .filter_map(|e| {
                    let p: Vec<&str> = e.split(':').collect();
                    if p.len() != 4 {
                        return None;
                    }
                    Some(PmicOutput {
                        name: p[0].to_string(),
                        topology: p[1].to_string(),
                        vout: parse_si_txt(p[2])?,
                        i_max: parse_si_txt(p[3])?,
                    })
                })
                .collect();
            if outputs.is_empty() {
                continue;
            }
            // the silicon's part_number: the part entity this block
            // instantiates, or its own attr
            let part_number = attrs.get("part_number").cloned().or_else(|| {
                crate::stage_resolution::block_part_number(&text, &name)
            });
            out.push(PmicBlock {
                block: name,
                part_number,
                vin_min: attrs.get("vin_min").and_then(|v| parse_si_txt(v)),
                vin_max: attrs.get("vin_max").and_then(|v| parse_si_txt(v)),
                outputs,
                seq: attrs.get("pmic_seq").map(|s| s.trim_matches('"').to_string()),
                seq_dly: attrs.get("pmic_seq_dly").map(|s| s.trim_matches('"').to_string()),
            });
        }
    }
    out
}

/// Evaluate every declared multi-output block against the board's
/// resolved requirements; returns report lines (empty when fewer than
/// two requirements, or nothing covers more than one rail).
pub fn evaluate(resolutions: &[StageResolution], stdlib_root: &Path) -> Vec<String> {
    if resolutions.len() < 2 {
        return Vec::new();
    }
    let req_val = |r: &StageResolution, key: &str| -> Option<f64> {
        r.requirement
            .split(',')
            .filter_map(|kv| kv.trim().split_once('='))
            .find(|(k, _)| k.trim() == key)
            .and_then(|(_, v)| parse_si_txt(v.trim()))
    };
    let mut lines = Vec::new();
    for pmic in scan_pmics(stdlib_root) {
        let mut used: Vec<usize> = Vec::new();
        let mut cover: Vec<(String, String)> = Vec::new(); // (instance, output)
        let mut covered_price = 0.0f64;
        let mut covered_priced = true;
        let mut leftovers: Vec<String> = Vec::new();
        for r in resolutions {
            let (Some(vout), Some(imax)) = (req_val(r, "vout"), req_val(r, "i_max")) else {
                leftovers.push(format!("{} (no vout/i_max)", r.instance));
                continue;
            };
            let vin = req_val(r, "vin");
            let vin_ok = match (vin, pmic.vin_min, pmic.vin_max) {
                (Some(v), Some(lo), Some(hi)) => v >= lo - 1e-9 && v <= hi + 1e-9,
                _ => true, // unstated = not disqualifying at report level
            };
            let derated = imax / 0.8;
            let slot = pmic.outputs.iter().enumerate().find(|(i, o)| {
                !used.contains(i)
                    && vin_ok
                    && (o.vout - vout).abs() <= 0.02 * vout + 1e-9
                    && o.i_max + 1e-9 >= derated
            });
            match slot {
                Some((i, o)) => {
                    used.push(i);
                    cover.push((r.instance.clone(), format!("{} ({}, {:.2}V, {:.1}A rated)", o.name, o.topology, o.vout, o.i_max)));
                    // the discrete price this output would displace
                    match r
                        .candidates
                        .iter()
                        .find(|c| Some(&c.block) == r.bound.as_ref())
                        .and_then(|c| c.ic_price)
                    {
                        Some(p) => covered_price += p,
                        None => covered_priced = false,
                    }
                }
                None => leftovers.push(format!(
                    "{} ({}V @ {}A{})",
                    r.instance,
                    vout,
                    imax,
                    if vin_ok { "" } else { " — input rail outside the PMIC range" }
                )),
            }
        }
        if cover.len() < 2 {
            continue; // a PMIC that covers one rail is not aggregation
        }
        let pmic_price: Option<(f64, String)> = pmic
            .part_number
            .as_deref()
            .and_then(|pn| crate::supply_synthesis::price_via_provider(pn, ""))
            .and_then(|(p, m, _)| p.map(|p| (p, m.unwrap_or_default())));
        lines.push(format!(
            "AGGREGATION option — {} covers {} of {} rails:",
            pmic.block,
            cover.len(),
            resolutions.len()
        ));
        for (inst, out) in &cover {
            lines.push(format!("    {inst} → {out}"));
        }
        for l in &leftovers {
            lines.push(format!("    not covered: {l}"));
        }
        match (pmic_price, covered_priced, covered_price) {
            (Some((p, mpn)), true, d) if d > 0.0 => lines.push(format!(
                "    price: {} ${:.4} vs Σ displaced discrete silicon ${:.4} (support parts BOM-time both ways) — {}",
                mpn, p, d,
                if p < d { "the PMIC wins on silicon" } else { "the discretes win on silicon; the PMIC may still win on area/BOM-lines/sequencing" }
            )),
            (Some((p, mpn)), _, _) => lines.push(format!(
                "    price: {} ${:.4}; displaced discrete Σ not fully priced (a covered stage is unresolved/unpriced) — stated",
                mpn, p
            )),
            (None, _, _) => lines.push(
                "    price: PMIC silicon not priced (provider/DB absent or no match) — stated".into(),
            ),
        }
        if let Some(seq) = &pmic.seq {
            lines.push(format!(
                "    built-in power-up order: {} (inter-strobe delay {}) — the sequencing these rails inherit for free; the strict gate against declared domain ordering binds at aggregation COMMIT (future increment, stated)",
                seq,
                pmic.seq_dly.as_deref().unwrap_or("unstated")
            ));
        }
        lines.push("    commit is the designer's lever — aggregation is REPORTED, never auto-bound (this increment)".into());
    }
    lines
}
