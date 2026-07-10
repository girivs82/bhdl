//! v0.8 parametric interfaces — source-text preprocessor.
//!
//! Recognises template declarations of the form
//!
//! ```text
//!     interface SPI<lanes: int = 1> {
//!         perspective master { signal SCK: out; signal CS: out; signal IO<lanes>: inout; }
//!         perspective slave  { signal SCK: in;  signal CS: in;  signal IO<lanes>: inout; }
//!     }
//! ```
//!
//! and per-use specialisations
//!
//! ```text
//!     interface SPI<lanes=4>:slave qspi;
//!     interface SPI flash;                  // uses defaults
//! ```
//!
//! For every distinct argument tuple, the preprocessor emits a
//! monomorphisation `interface SPI__lanes_4 { ... }` with
//! `<lanes>` substituted everywhere in the template body, then
//! expands any `signal IDENT<INT>: dir;` row into a flat list
//! `signal IDENT0: dir; signal IDENT1: dir; ... signal IDENT<N-1>: dir;`.
//! Use sites are rewritten to refer to the mangled name; the
//! template definition is removed.
//!
//! Tier-1 scope (this slice):
//!   - integer parameters only
//!   - default values supported (bare `interface NAME field;` allowed)
//!   - substitution into per-signal width annotations is the only
//!     transformation; other body shapes pass through unchanged
//!     after literal `<param>` → value replacement.

use anyhow::{anyhow, bail, Result};
use std::collections::{HashMap, BTreeMap};
use std::ops::Range;

#[derive(Debug, Clone)]
struct ParamDecl {
    name: String,
    default: Option<String>,
}

#[derive(Debug, Clone)]
struct Template {
    name: String,
    params: Vec<ParamDecl>,
    body: String,
    // Range of the full `interface NAME<...> { ... }` text in the
    // (stripped) source — used to delete the template after expansion.
    decl_range: Range<usize>,
}

/// Use site of a parametric interface. `args` may be a subset of
/// the template's parameters (missing entries are filled from defaults).
#[derive(Debug, Clone)]
struct UseSite {
    template_name: String,
    /// Range covering just `NAME[<args>]` — what we replace with
    /// the mangled name. The trailing perspective/field/binding tokens
    /// are preserved verbatim.
    range: Range<usize>,
    args: BTreeMap<String, String>,
}

pub fn preprocess(source: &str) -> Result<String> {
    let stripped = strip_block_comments(source);
    let templates = find_templates(&stripped)?;
    // Even when no parametric templates are declared, we still want
    // to run a generate-loop expansion pass over the source so that
    // board/entity-level `generate for ... { ... }` blocks (the
    // swizzle use case) get unrolled before the parser sees them.
    if templates.is_empty() {
        let expanded = expand_generate_loops(&stripped);
        return if expanded == stripped {
            Ok(source.to_string())
        } else {
            Ok(expanded)
        };
    }

    // Skip ranges covered by template definitions when scanning for
    // use sites — otherwise the template's own `<param>` annotations
    // look like usages of itself.
    let template_ranges: Vec<Range<usize>> =
        templates.values().map(|t| t.decl_range.clone()).collect();

    let uses = find_uses(&stripped, &templates, &template_ranges)?;

    // Build monomorphisations keyed by mangled name.
    let mut specs: HashMap<String, String> = HashMap::new();
    let mut rewrites: Vec<(Range<usize>, String)> = Vec::new();
    for u in &uses {
        let tpl = templates
            .get(&u.template_name)
            .ok_or_else(|| anyhow!("unknown parametric interface `{}`", u.template_name))?;
        let args = apply_defaults(tpl, &u.args)?;
        let mangled = mangle(&tpl.name, &tpl.params, &args);
        specs.entry(mangled.clone())
            .or_insert_with(|| expand_template(tpl, &args, &mangled));
        rewrites.push((u.range.clone(), mangled));
    }

    // Single edit pass: delete templates + rewrite uses, back-to-front.
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    for t in templates.values() {
        edits.push((t.decl_range.clone(), String::new()));
    }
    for (r, repl) in rewrites {
        edits.push((r, repl));
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.0.start));
    for w in edits.windows(2) {
        if w[0].0.start < w[1].0.end {
            bail!(
                "internal: overlapping parametric-resolver edits at {}..{} and {}..{}",
                w[1].0.start, w[1].0.end, w[0].0.start, w[0].0.end,
            );
        }
    }

    let mut out = stripped.clone();
    for (r, repl) in &edits {
        out.replace_range(r.clone(), repl);
    }

    // Append all monomorphisations at the bottom — order doesn't
    // matter to the parser (interfaces resolve by name).
    out.push('\n');
    for body in specs.values() {
        out.push_str(body);
        out.push('\n');
    }

    // Top-level generate-loop pass. Template bodies were already
    // unrolled inside `expand_template`; this pass catches
    // board/entity-level generates (the swizzle use case).
    let out = expand_generate_loops(&out);

    Ok(out)
}

// ---------- template detection ----------

fn find_templates(source: &str) -> Result<HashMap<String, Template>> {
    let mut out: HashMap<String, Template> = HashMap::new();
    let bytes = source.as_bytes();
    for pos in find_keyword_positions(source, "interface") {
        // Skip leading-context: we want top-level `interface NAME<` only.
        // Templates whose `<` is actually a sub-interface reversal `~`
        // or a perspective selector won't start with `<` right after
        // an identifier — but we still need to be careful.
        let after_kw = pos + "interface".len();
        let name_start = match skip_ws(source, after_kw) {
            Some(p) => p,
            None => continue,
        };
        let name_end = scan_ident(source, name_start);
        if name_end == name_start { continue; }
        let name = source[name_start..name_end].to_string();
        let after_name = match skip_ws(source, name_end) {
            Some(p) => p,
            None => continue,
        };
        if bytes.get(after_name) != Some(&b'<') { continue; }

        // Found `interface NAME<` — parse param list until matching `>`.
        let (params, after_args) = match parse_param_decl_list(source, after_name) {
            Some(x) => x,
            None => continue, // not a parametric decl (could be `<->`-something — ignore)
        };
        let body_start_open = match skip_ws(source, after_args) {
            Some(p) => p,
            None => continue,
        };
        if bytes.get(body_start_open) != Some(&b'{') { continue; }
        let body_end_close = match find_matching_brace(source, body_start_open) {
            Some(e) => e,
            None => continue,
        };
        let body = source[body_start_open + 1..body_end_close].to_string();
        let decl_range = pos..body_end_close + 1;

        if out.contains_key(&name) {
            bail!("duplicate parametric interface `{}`", name);
        }
        out.insert(
            name.clone(),
            Template { name, params, body, decl_range },
        );
    }
    Ok(out)
}

/// Parse `<p1: int = D1, p2: int = D2, ...>` starting at the `<`.
/// Returns the param list + byte index just past the `>`. `None` if
/// the `<...>` doesn't look like a param list (so we silently skip
/// non-parametric occurrences of `<` after an identifier).
fn parse_param_decl_list(source: &str, lt_idx: usize) -> Option<(Vec<ParamDecl>, usize)> {
    let bytes = source.as_bytes();
    if bytes.get(lt_idx) != Some(&b'<') { return None; }
    // Find matching `>` while tracking nesting. We don't expect
    // nested `<>` in param decls, so depth-1 closing `>` wins.
    let mut i = lt_idx + 1;
    let mut depth = 1i32;
    while i < bytes.len() {
        match bytes[i] {
            b'<' => depth += 1,
            b'>' => { depth -= 1; if depth == 0 { break; } }
            b';' | b'{' | b'}' => return None, // can't be a param list
            _ => {}
        }
        i += 1;
    }
    if depth != 0 { return None; }
    let inner = &source[lt_idx + 1..i];
    let mut params: Vec<ParamDecl> = Vec::new();
    for chunk in inner.split(',') {
        let s = chunk.trim();
        if s.is_empty() { continue; }
        // Shape: NAME [: TYPE] [= DEFAULT]
        let (head, default) = match s.find('=') {
            Some(eq) => (s[..eq].trim().to_string(), Some(s[eq + 1..].trim().to_string())),
            None => (s.to_string(), None),
        };
        let name = match head.find(':') {
            Some(c) => head[..c].trim().to_string(),
            None => head.trim().to_string(),
        };
        if name.is_empty() || !is_ident_start(name.as_bytes()[0]) { return None; }
        params.push(ParamDecl { name, default });
    }
    if params.is_empty() { return None; }
    Some((params, i + 1))
}

// ---------- use-site detection ----------

fn find_uses(
    source: &str,
    templates: &HashMap<String, Template>,
    template_ranges: &[Range<usize>],
) -> Result<Vec<UseSite>> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    'outer: for pos in find_keyword_positions(source, "interface") {
        for r in template_ranges {
            if pos >= r.start && pos < r.end {
                continue 'outer;
            }
        }
        let after_kw = pos + "interface".len();
        let name_start = match skip_ws(source, after_kw) {
            Some(p) => p,
            None => continue,
        };
        let name_end = scan_ident(source, name_start);
        if name_end == name_start { continue; }
        let name = source[name_start..name_end].to_string();
        let Some(tpl) = templates.get(&name) else { continue; };

        // `interface NAME` … may be followed by `<args>` (specialisation)
        // OR perspective/field directly (defaults form).
        let after_name = match skip_ws(source, name_end) {
            Some(p) => p,
            None => continue,
        };
        let (args, range_end) = if bytes.get(after_name) == Some(&b'<') {
            match parse_arg_list(source, after_name) {
                Some(x) => x,
                None => continue,
            }
        } else {
            // Defaults: range covers just the bare `NAME`.
            (BTreeMap::new(), name_end)
        };

        // Range covers `NAME[<args>]` (not `interface` keyword, not the
        // suffix). Output substitutes a mangled identifier here.
        let range = name_start..range_end;
        out.push(UseSite { template_name: tpl.name.clone(), range, args });
    }
    Ok(out)
}

/// Parse `<k1=v1, k2=v2, ...>` OR `<v1, v2, ...>` starting at `<`.
/// Returns (args-map keyed by parameter name, byte index past `>`).
/// Positional args are bound to template params in declaration order
/// later (in `apply_defaults`); here we record them under the
/// reserved key `"__pos_<i>"`.
fn parse_arg_list(source: &str, lt_idx: usize) -> Option<(BTreeMap<String, String>, usize)> {
    let bytes = source.as_bytes();
    if bytes.get(lt_idx) != Some(&b'<') { return None; }
    let mut i = lt_idx + 1;
    let mut depth = 1i32;
    while i < bytes.len() {
        match bytes[i] {
            b'<' => depth += 1,
            b'>' => { depth -= 1; if depth == 0 { break; } }
            b';' | b'{' | b'}' => return None,
            _ => {}
        }
        i += 1;
    }
    if depth != 0 { return None; }
    let inner = &source[lt_idx + 1..i];
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    let mut pos_idx = 0usize;
    for chunk in inner.split(',') {
        let s = chunk.trim();
        if s.is_empty() { continue; }
        if let Some(eq) = s.find('=') {
            let k = s[..eq].trim().to_string();
            let v = s[eq + 1..].trim().to_string();
            if k.is_empty() || v.is_empty() { return None; }
            out.insert(k, v);
        } else {
            out.insert(format!("__pos_{}", pos_idx), s.to_string());
            pos_idx += 1;
        }
    }
    Some((out, i + 1))
}

fn apply_defaults(
    tpl: &Template,
    raw_args: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    // Positional args bind first.
    for (i, param) in tpl.params.iter().enumerate() {
        let pos_key = format!("__pos_{}", i);
        if let Some(v) = raw_args.get(&pos_key) {
            out.insert(param.name.clone(), v.clone());
        }
    }
    // Named args override / fill in.
    for (k, v) in raw_args {
        if k.starts_with("__pos_") { continue; }
        if !tpl.params.iter().any(|p| &p.name == k) {
            bail!("parametric interface `{}` has no parameter `{}`", tpl.name, k);
        }
        out.insert(k.clone(), v.clone());
    }
    // Apply defaults for params not yet bound.
    for p in &tpl.params {
        if out.contains_key(&p.name) { continue; }
        match &p.default {
            Some(d) => { out.insert(p.name.clone(), d.clone()); }
            None => bail!(
                "parametric interface `{}` requires argument `{}` (no default)",
                tpl.name, p.name,
            ),
        }
    }
    Ok(out)
}

// ---------- expansion + mangling ----------

fn mangle(name: &str, params: &[ParamDecl], args: &BTreeMap<String, String>) -> String {
    let mut s = name.to_string();
    // Iterate in declaration order so mangled names are stable.
    for p in params {
        if let Some(v) = args.get(&p.name) {
            s.push_str("__");
            s.push_str(&p.name);
            s.push('_');
            // Sanitise: keep alnum/underscore, drop everything else.
            for ch in v.chars() {
                if ch.is_ascii_alphanumeric() || ch == '_' {
                    s.push(ch);
                }
            }
        }
    }
    s
}

fn expand_template(tpl: &Template, args: &BTreeMap<String, String>, mangled: &str) -> String {
    // Step 1: textual substitution of `<param>` → value within the body.
    let mut body = tpl.body.clone();
    for p in &tpl.params {
        let needle = format!("<{}>", p.name);
        if let Some(v) = args.get(&p.name) {
            body = body.replace(&needle, &format!("<{}>", v));
        }
    }
    // Step 1.5 (tier 2): unroll `generate for i in <a>..<b> { ... }`
    // loops, substituting `<i>` per iteration. Nested loops are
    // handled by the multi-pass driver. Loop bounds may be bare
    // numbers or `<NUMBER>` (the form parametric substitution
    // produces, since `<param>` is the placeholder syntax).
    let body = expand_generate_loops(&body);
    // Step 2: expand `signal IDENT<INT>: dir;` rows into N flat
    // `signal IDENTk: dir;` lines.
    let body = expand_signal_arrays(&body);

    format!("interface {} {{{}}}", mangled, body)
}

/// Tier-2 generative loops.
///
/// ```text
/// generate for IDENT in BOUND..BOUND { body }
/// ```
///
/// Each loop is unrolled to N copies of `body`, with `<IDENT>`
/// replaced by each integer in the half-open range. A bound is
/// either a bare ASCII integer or `<N>` (the form parametric
/// substitution produces, since `<param>` is the placeholder
/// syntax we already use elsewhere).
///
/// Nested loops are supported via the multi-pass driver below —
/// each pass expands the first matched `generate for` and re-runs
/// against the result, so inner loops surface in subsequent passes.
fn expand_generate_loops(body: &str) -> String {
    let mut out = body.to_string();
    loop {
        match find_and_expand_one_generate(&out) {
            Some((start, end, replacement)) => {
                out.replace_range(start..end, &replacement);
            }
            None => return out,
        }
    }
}

fn find_and_expand_one_generate(text: &str) -> Option<(usize, usize, String)> {
    let bytes = text.as_bytes();
    for kw_pos in find_keyword_positions(text, "generate") {
        let after_kw = kw_pos + "generate".len();
        let p = match skip_ws(text, after_kw) { Some(x) => x, None => continue };
        if !text[p..].starts_with("for") { continue; }
        let after_for = p + 3;
        if bytes.get(after_for).map(|c| c.is_ascii_alphanumeric() || *c == b'_').unwrap_or(false) {
            continue;
        }

        // Header: var-decl `in` iteration-source
        let (header, after_header) = match parse_loop_header(text, after_for) {
            Some(x) => x,
            None => continue,
        };

        // `{` body `}`
        let p = match skip_ws(text, after_header) { Some(x) => x, None => continue };
        if bytes.get(p) != Some(&b'{') { continue; }
        let close = match find_matching_brace(text, p) { Some(c) => c, None => continue };
        let loop_body = &text[p + 1..close];

        // Build the unrolled replacement. For each (idx, val) in
        // the iteration source, copy the body with `<var_val>` →
        // val and (if a paired index var was declared) `<var_idx>`
        // → idx. `_` as a name suppresses substitution.
        let mut expanded = String::with_capacity(loop_body.len() * header.values.len());
        for (idx, val) in header.values.iter().enumerate() {
            let mut copy = loop_body.to_string();
            if header.var_val != "_" {
                copy = copy.replace(&format!("<{}>", header.var_val), &val.to_string());
            }
            if let Some(vi) = &header.var_idx {
                if vi != "_" {
                    copy = copy.replace(&format!("<{}>", vi), &idx.to_string());
                }
            }
            expanded.push_str(&copy);
            expanded.push('\n');
        }
        return Some((kw_pos, close + 1, expanded));
    }
    None
}

/// Parsed generate-loop header: variable binding(s) + the concrete
/// values to iterate. The body is substituted using `<var_val>` for
/// the current value and `<var_idx>` (if present) for the iteration
/// index.
struct LoopHeader {
    /// Optional paired index variable from `for (j, i) in …` syntax.
    /// `None` for the single-variable form `for i in …`.
    var_idx: Option<String>,
    /// The value variable.
    var_val: String,
    /// Concrete iteration values. For `0..<N>` this is `0..N`; for
    /// `[2,3,0,1]` it's the list verbatim.
    values: Vec<usize>,
}

fn parse_loop_header(text: &str, after_for: usize) -> Option<(LoopHeader, usize)> {
    let bytes = text.as_bytes();
    let p = skip_ws(text, after_for)?;

    // Variable declaration: either `IDENT` or `(IDENT, IDENT)`.
    let (var_idx, var_val, after_vars) = if bytes.get(p) == Some(&b'(') {
        let p = p + 1;
        let p = skip_ws(text, p)?;
        let i1_end = scan_ident_or_underscore(text, p);
        if i1_end == p { return None; }
        let i1 = text[p..i1_end].to_string();
        let p = skip_ws(text, i1_end)?;
        if bytes.get(p) != Some(&b',') { return None; }
        let p = skip_ws(text, p + 1)?;
        let i2_end = scan_ident_or_underscore(text, p);
        if i2_end == p { return None; }
        let i2 = text[p..i2_end].to_string();
        let p = skip_ws(text, i2_end)?;
        if bytes.get(p) != Some(&b')') { return None; }
        (Some(i1), i2, p + 1)
    } else {
        let end = scan_ident_or_underscore(text, p);
        if end == p { return None; }
        (None, text[p..end].to_string(), end)
    };

    // `in` keyword.
    let p = skip_ws(text, after_vars)?;
    if !text[p..].starts_with("in") { return None; }
    let after_in = p + 2;
    if bytes.get(after_in).map(|c| c.is_ascii_alphanumeric() || *c == b'_').unwrap_or(false) {
        return None;
    }

    // Iteration source: `[list, of, ints]` OR `BOUND..BOUND`.
    let p = skip_ws(text, after_in)?;
    let (values, after_src) = if bytes.get(p) == Some(&b'[') {
        parse_list_literal(text, p)?
    } else {
        parse_range_literal(text, p)?
    };

    Some((LoopHeader { var_idx, var_val, values }, after_src))
}

fn scan_ident_or_underscore(text: &str, start: usize) -> usize {
    let bytes = text.as_bytes();
    if start >= bytes.len() { return start; }
    let first = bytes[start];
    if !(first.is_ascii_alphabetic() || first == b'_') { return start; }
    let mut i = start + 1;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    i
}

/// Parse `[N, N, N, ...]` starting at `[`. Each element is a bare
/// integer or a `<N>`-wrapped integer (the latter is what parametric
/// substitution might leave behind).
fn parse_list_literal(text: &str, start: usize) -> Option<(Vec<usize>, usize)> {
    let bytes = text.as_bytes();
    if bytes.get(start) != Some(&b'[') { return None; }
    // Find matching `]`. We don't expect nested brackets in tier 1.
    let mut i = start + 1;
    while i < bytes.len() && bytes[i] != b']' {
        i += 1;
    }
    if i >= bytes.len() { return None; }
    let inner = &text[start + 1..i];
    let mut values = Vec::new();
    for chunk in inner.split(',') {
        let s = chunk.trim();
        if s.is_empty() { continue; }
        let n = parse_int_token(s)?;
        values.push(n);
    }
    Some((values, i + 1))
}

/// Parse `BOUND..BOUND` into a concrete `0..N`-style range.
fn parse_range_literal(text: &str, start: usize) -> Option<(Vec<usize>, usize)> {
    let (lo, after_lo) = parse_range_bound(text, start)?;
    let p = skip_ws(text, after_lo)?;
    if !text[p..].starts_with("..") { return None; }
    let p = p + 2;
    let p = skip_ws(text, p)?;
    let (hi, after_hi) = parse_range_bound(text, p)?;
    let values = if hi > lo { (lo..hi).collect() } else { Vec::new() };
    Some((values, after_hi))
}

/// Parse a single integer or `<N>`-wrapped integer from `s`.
fn parse_int_token(s: &str) -> Option<usize> {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix('<').and_then(|r| r.strip_suffix('>')) {
        return inner.trim().parse().ok();
    }
    s.parse().ok()
}

/// Parse a generate-loop range bound. Accepts bare digits (`8`) or
/// the wrapped form (`<8>`) that parametric substitution emits.
fn parse_range_bound(text: &str, start: usize) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut i = start;
    let wrapped = bytes.get(i) == Some(&b'<');
    if wrapped { i += 1; }
    let num_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == num_start { return None; }
    let n: usize = text[num_start..i].parse().ok()?;
    if wrapped {
        if bytes.get(i) != Some(&b'>') { return None; }
        Some((n, i + 1))
    } else {
        Some((n, i))
    }
}

/// Scan the body for every `signal IDENT<INT>: dir;` occurrence and
/// expand it in place to `signal IDENT0: dir; signal IDENT1: dir; ...`.
/// Anything not matching that shape passes through unchanged.
fn expand_signal_arrays(body: &str) -> String {
    let mut out = String::with_capacity(body.len() + 64);
    let bytes = body.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // Quick reject: only look at word-boundary `signal` keyword starts.
        if bytes[i] == b's' && body[i..].starts_with("signal")
            && (i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_'))
            && bytes.get(i + 6).map(|c| !(c.is_ascii_alphanumeric() || *c == b'_')).unwrap_or(true)
        {
            if let Some((expanded, consumed)) = try_expand_signal_array_at(body, i) {
                out.push_str(&expanded);
                i += consumed;
                continue;
            }
        }
        // Copy this byte through. SAFETY: indexing on a UTF-8 string by
        // bytes is fine because the source is ASCII for keywords/punct
        // and we only step one byte at a time; if a multi-byte sequence
        // appears we step through its bytes one at a time, which still
        // produces valid output via the bytes vector — but we keep the
        // String API via push of the char at this position.
        let ch_len = utf8_char_len(bytes[i]);
        out.push_str(&body[i..i + ch_len]);
        i += ch_len;
    }
    out
}

fn utf8_char_len(b: u8) -> usize {
    if b < 0x80 { 1 }
    else if b < 0xC0 { 1 } // continuation byte; treat as 1 to avoid infinite loop
    else if b < 0xE0 { 2 }
    else if b < 0xF0 { 3 }
    else { 4 }
}

/// Try to match `signal IDENT<INT>: DIR;` starting at `start` (which
/// is the `s` of `signal`). Returns `(expanded_text, bytes_consumed)`
/// on success.
fn try_expand_signal_array_at(body: &str, start: usize) -> Option<(String, usize)> {
    let rest = &body[start..];
    // Strip `signal` + ws
    let after_kw = rest.strip_prefix("signal")?;
    let ws_len = after_kw.bytes().take_while(|b| (*b as char).is_whitespace()).count();
    if ws_len == 0 { return None; }
    let after_ws = &after_kw[ws_len..];

    // ident
    let ident_end = after_ws.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))?;
    if ident_end == 0 { return None; }
    let ident = &after_ws[..ident_end];
    let after_ident = &after_ws[ident_end..];

    let after_lt = after_ident.strip_prefix('<')?;
    let gt = after_lt.find('>')?;
    let inside = after_lt[..gt].trim();
    let n: usize = inside.parse().ok()?;
    let after_gt = &after_lt[gt + 1..];

    // Optional ws then `:`
    let ws2 = after_gt.bytes().take_while(|b| (*b as char).is_whitespace()).count();
    let after_ws2 = &after_gt[ws2..];
    let after_colon = after_ws2.strip_prefix(':')?;
    let ws3 = after_colon.bytes().take_while(|b| (*b as char).is_whitespace()).count();
    let after_ws3 = &after_colon[ws3..];

    let semi = after_ws3.find(';')?;
    let dir = after_ws3[..semi].trim();
    if dir.is_empty() { return None; }

    // Bytes consumed: signal kw + ws + ident + `<` + inside + `>` + ws + `:` + ws + dir + `;`
    let consumed = "signal".len() + ws_len + ident_end + 1 + gt + 1 + ws2 + 1 + ws3 + semi + 1;

    let mut out = String::new();
    for i in 0..n {
        out.push_str("signal ");
        out.push_str(ident);
        out.push_str(&i.to_string());
        out.push_str(": ");
        out.push_str(dir);
        out.push(';');
        if i + 1 < n { out.push(' '); }
    }
    Some((out, consumed))
}

// ---------- small shared helpers (duplicated from abstract_resolver
// to keep the modules independent) ----------

fn strip_block_comments(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                out.push(b' ');
                i += 1;
            }
        } else if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            out.push(b' '); out.push(b' ');
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                if bytes[i] == b'\n' { out.push(b'\n'); } else { out.push(b' '); }
                i += 1;
            }
            if i + 1 < bytes.len() {
                out.push(b' '); out.push(b' ');
                i += 2;
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| source.to_string())
}

fn find_keyword_positions(source: &str, keyword: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let mut search = 0;
    while let Some(rel) = source[search..].find(keyword) {
        let pos = search + rel;
        let before_ok = pos == 0
            || !(bytes[pos - 1].is_ascii_alphanumeric() || bytes[pos - 1] == b'_');
        let after = pos + keyword.len();
        let after_ok = after >= source.len()
            || !(bytes[after].is_ascii_alphanumeric() || bytes[after] == b'_');
        if before_ok && after_ok {
            out.push(pos);
        }
        search = pos + 1;
    }
    out
}

fn find_matching_brace(source: &str, open_idx: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(open_idx) != Some(&b'{') { return None; }
    let mut depth = 0i32;
    let mut i = open_idx;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 { return Some(i); }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn skip_ws(source: &str, mut i: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    while i < bytes.len() && (bytes[i] as char).is_whitespace() {
        i += 1;
    }
    if i >= bytes.len() { None } else { Some(i) }
}

fn scan_ident(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut i = start;
    if i >= bytes.len() || !is_ident_start(bytes[i]) { return i; }
    i += 1;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    i
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}
