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

    let mut supplies = Vec::new();
    // Splice back-to-front so earlier byte ranges stay valid.
    let mut out = source.to_string();
    let mut import_lines: Vec<String> = Vec::new();

    for stmt in stmts.iter().rev() {
        let d = desugar_one(stmt, &rails, &ground, source, stdlib_root)?;
        let range = stmt.text_range();
        let (start, end) = (usize::from(range.start()), usize::from(range.end()));
        out.replace_range(start..end, &d.generated);
        if !d.import_line.is_empty() && !import_lines.contains(&d.import_line) {
            import_lines.push(d.import_line.clone());
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

fn desugar_one(
    stmt: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
    rails: &HashMap<String, RailDecl>,
    ground: &str,
    original: &str,
    stdlib_root: &Path,
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
                &profile,
            )?
        }
    };

    // Resolve the part in the stdlib and read its constructor params + pins.
    let (part_path, entity_src) = find_entity(stdlib_root, &part)?;
    let params = entity_param_names(&entity_src, &part);
    let pins = entity_pin_names(&entity_src, &part);
    for required in ["VIN", "VOUT", "GND"] {
        if !pins.iter().any(|p| p == required) {
            bail!(
                "supply @{target}: part `{part}` has no {required} pin — not a \
                 regulator entity this statement can wire"
            );
        }
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
    if pins.iter().any(|p| p == "EN") {
        g.push_str(&format!("{indent}@{source_rail} -> {instance}.EN;\n"));
    }
    g.push_str(&format!("{indent}{instance}.VOUT -> @{target};"));

    // Import line, repo-root-relative (the loader's convention), skipped when
    // the board already imports the part.
    let already_imported = original.contains(&format!("{{ {part} }}"))
        || original.contains(&format!("{part},"))
        || original.contains(&format!(", {part} }}"));
    let import_line = if already_imported {
        String::new()
    } else {
        format!("import {{ {part} }} from \"{}\";", part_path.display())
    };

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

/// Textual rail scan: `power NAME = 12V @ 3A;` lines. The desugar runs before
/// analysis, so this stays a plain line scan rather than a CST walk.
fn parse_rails(source: &str) -> HashMap<String, RailDecl> {
    let mut rails = HashMap::new();
    for line in source.lines() {
        let l = line.trim();
        let Some(rest) = l.strip_prefix("power ") else { continue };
        let Some((name, rhs)) = rest.split_once('=') else { continue };
        let rhs = rhs.trim().trim_end_matches(';').trim();
        let (voltage, current) = match rhs.split_once('@') {
            Some((v, i)) => (v.trim().to_string(), Some(i.trim().to_string())),
            None => (rhs.to_string(), None),
        };
        rails.insert(name.trim().to_string(), RailDecl { voltage, current });
    }
    rails
}

fn parse_ground(source: &str) -> Option<String> {
    source.lines().find_map(|l| {
        l.trim()
            .strip_prefix("ground ")
            .map(|g| g.trim_end_matches(';').trim().to_string())
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
fn parse_si_txt(s: &str) -> Option<f64> {
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
    profile: &str,
) -> Result<(String, Vec<Candidate>)> {
    let mut files = Vec::new();
    collect_bhdl(stdlib_root, &mut files);

    let mut survey: Vec<Candidate> = Vec::new();
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
    }

    // Rank the survivors per the requested profile:
    //   cost     → real regulator price (support parts count then loss as
    //              tiebreaks; support-part PRICING is the remaining S3 gap);
    //   balanced / grade → lowest loss, then fewest parts.
    let key = |c: &Candidate| -> (f64, f64, f64) {
        let loss = c.loss_w.unwrap_or(f64::MAX);
        match profile {
            "cost" => (
                c.ic_price.unwrap_or(f64::MAX),
                c.support_parts as f64,
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
    let pts: Vec<(f64, f64)> = [0.2, 0.4, 0.6, 0.8, 1.0]
        .iter()
        .map(|frac| {
            let i = frac * i_out;
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
            let pts: Vec<(f64, f64)> = [0.5, 0.75, 1.0, 1.5, 2.0]
                .iter()
                .map(|k| {
                    let cap = k * c_req;
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
