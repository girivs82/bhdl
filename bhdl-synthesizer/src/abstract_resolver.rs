//! v0.9b: abstract-entity resolution as a source-text preprocessor.
//!
//! User syntax:
//!
//!     abstract entity ATmega328P {
//!         family {
//!             ATmega328P_DIP28 {
//!                 vcc = VCC; gnd = GND1; reset = PC6;
//!                 adc0 = PC0; adc1 = PC1; ... adc5 = PC5;
//!                 // adc6 / adc7 absent: DIP-28 doesn't expose them
//!             };
//!             ATmega328P_QFN32 {
//!                 vcc = VCC1; gnd = GND1; reset = PC6;
//!                 adc0 = PC0; ... adc5 = PC5;
//!                 adc6 = ADC6; adc7 = ADC7;       // QFN-only
//!             };
//!         }
//!     }
//!
//!     board MyBoard {
//!         mcu: ATmega328P();         // abstract instance
//!         mcu.adc7 -> @THERMISTOR;   // ← uses an abstract alias
//!     }
//!
//! Each family entry declares its own `pin_map` mapping abstract
//! alias names to the concrete entity's own pin names. The
//! resolver:
//!
//!   1. Finds every `inst_name: ABSTRACT_NAME(...)` instance and
//!      the set of `inst_name.X` references the board uses.
//!   2. For each instance, picks the first family entry whose
//!      pin_map keys cover the used X set.
//!   3. Rewrites the source:
//!        - removes the abstract entity declaration
//!        - rewrites each `mcu.X` to `mcu.<pin_map[X]>` (per the
//!          chosen SKU's map)
//!        - rewrites `mcu: ABSTRACT(` to `mcu: CONCRETE(`
//!
//! The rewritten source flows through the regular parser /
//! analyzer / synthesizer — none of which knows about abstract
//! entities. SKU-specific naming differences (DIP-28's `VCC` vs
//! QFN-32's `VCC1`) stay hidden behind the abstract aliases the
//! board uses (`mcu.vcc`).

use std::collections::{HashMap, HashSet};
use anyhow::{Result, anyhow};

/// Per-instance resolution record returned by [`preprocess_with_resolutions`].
/// `abstract_entity` is the name the user wrote (`ATmega328P`),
/// `concrete_sku` is the SKU the resolver picked (`ATmega328P_QFN32`).
#[derive(Debug, Clone)]
pub struct Resolution {
    pub abstract_entity: String,
    pub concrete_sku: String,
}

/// Map from instance name (e.g. `mcu`) to its resolution.
pub type ResolutionMap = HashMap<String, Resolution>;

/// As [`preprocess`] but also returns the per-instance resolution
/// map. Useful when downstream tooling (e.g. the synthesizer driver)
/// wants to stamp the chosen SKU back onto netlist instances after
/// synthesis so BOM walkers can see which SKU each abstract
/// instance became.
pub fn preprocess_with_resolutions(source: &str) -> Result<(String, ResolutionMap)> {
    let (rewritten, raw) = preprocess_impl(source)?;
    let map: ResolutionMap = raw.into_iter()
        .map(|(inst, abs, sku)| (inst, Resolution {
            abstract_entity: abs,
            concrete_sku: sku,
        }))
        .collect();
    Ok((rewritten, map))
}

/// Source-text preprocessing for abstract-entity resolution.
///
/// Returns the rewritten source. If the input has no abstract-entity
/// declarations, the returned string equals the input verbatim.
pub fn preprocess(source: &str) -> Result<String> {
    Ok(preprocess_impl(source)?.0)
}

/// Shared implementation. Returns the rewritten source plus a list
/// of (instance_name, abstract_entity_name, concrete_sku_name) per
/// resolved abstract instance.
fn preprocess_impl(source: &str) -> Result<(String, Vec<(String, String, String)>)> {
    let decls = extract_abstract_decls(source);
    if decls.is_empty() {
        return Ok((source.to_string(), Vec::new()));
    }

    // Find every `inst_name: ABSTRACT_NAME(...)` instance.
    let instances = extract_abstract_instances(source, &decls);

    // Validate: every family entry's pin_map keys must be a subset of
    // the declared abstract ports. Otherwise the stdlib author has a
    // bug (the SKU is exposing aliases the abstract entity doesn't
    // declare, and a board can't reference them by name).
    for decl in decls.values() {
        if decl.abstract_ports.is_empty() { continue; }  // legacy: no ports declared
        for fam in &decl.family {
            for alias in fam.pin_map.keys() {
                if !decl.abstract_ports.contains(alias) {
                    return Err(anyhow!(
                        "Family entry '{}' maps abstract port '{}' which is \
                         not declared on the abstract entity. Declared ports: \
                         {:?}.",
                        fam.concrete_name, alias,
                        {
                            let mut v: Vec<&String> = decl.abstract_ports.iter().collect();
                            v.sort();
                            v
                        }));
                }
            }
        }
    }

    // Resolve each instance + collect per-instance pin-alias rewrites.
    type AliasRewrite = HashMap<String, String>; // alias → concrete pin
    let mut resolutions: HashMap<String, (String, AliasRewrite)> = HashMap::new();
    for (inst_name, entity_type, _type_range) in &instances {
        let decl = &decls[entity_type];
        let wired_aliases = extract_wired_pins(source, inst_name);

        // Validate: every board-side reference must be a declared
        // abstract port. Caught here (rather than as "no family
        // member covers") so the error message names the *abstract
        // entity*, which is what the user sees in their source.
        if !decl.abstract_ports.is_empty() {
            for alias in &wired_aliases {
                if !decl.abstract_ports.contains(alias) {
                    return Err(anyhow!(
                        "Board references '{}.{}' but abstract entity \
                         '{}' has no port named '{}'. Declared ports: {:?}.",
                        inst_name, alias, entity_type, alias,
                        {
                            let mut v: Vec<&String> = decl.abstract_ports.iter().collect();
                            v.sort();
                            v
                        }));
                }
            }
        }

        let chosen = decl.family.iter().find(|fam| {
            wired_aliases.iter().all(|a| fam.pin_map.contains_key(a))
        });
        match chosen {
            Some(fam) => {
                // Multi-function-pin conflict check: each physical
                // pin can serve only one role per board. If two
                // wired aliases both map to the same concrete pin
                // (e.g. on atmega328p, `adc4` and `sda` both → PC4),
                // the user is asking the pin to do two jobs at once.
                // Flag it with a clear diagnostic that names the
                // colliding aliases AND the offending physical pin.
                let mut by_physical: HashMap<&str, Vec<&str>> = HashMap::new();
                for alias in &wired_aliases {
                    if let Some(physical) = fam.pin_map.get(alias) {
                        by_physical.entry(physical.as_str())
                            .or_default()
                            .push(alias.as_str());
                    }
                }
                let mut collisions: Vec<(&str, Vec<&str>)> = by_physical.into_iter()
                    .filter(|(_, aliases)| aliases.len() > 1)
                    .map(|(p, mut a)| { a.sort(); (p, a) })
                    .collect();
                if !collisions.is_empty() {
                    collisions.sort_by_key(|(p, _)| p.to_string());
                    let details: Vec<String> = collisions.iter()
                        .map(|(p, aliases)| format!(
                            "physical pin '{}' is claimed by aliases {:?}",
                            p, aliases))
                        .collect();
                    return Err(anyhow!(
                        "Multi-function-pin conflict on '{}' ({} resolved \
                         to SKU '{}'): {}. Each physical pin can only serve \
                         one role at a time — pick one alias per pin.",
                        inst_name, entity_type, fam.concrete_name,
                        details.join("; ")));
                }

                eprintln!(
                    "[abstract_resolver] '{}' ({}) wires {:?} → resolved to {} \
                     (family candidates tried: {:?})",
                    inst_name, entity_type, wired_aliases, fam.concrete_name,
                    decl.family.iter().map(|f| &f.concrete_name).collect::<Vec<_>>());
                resolutions.insert(
                    inst_name.clone(),
                    (fam.concrete_name.clone(), fam.pin_map.clone()),
                );
            }
            None => {
                return Err(anyhow!(
                    "Abstract entity '{}' instance '{}' wires aliases {:?}, \
                     but no family member's pin_map covers all of them. \
                     Family candidates: {:?}.",
                    entity_type, inst_name, wired_aliases,
                    decl.family.iter().map(|f| {
                        format!("{} ({} aliases)", f.concrete_name, f.pin_map.len())
                    }).collect::<Vec<_>>()));
            }
        }
    }

    // Collect the set of concrete entity names that were *chosen* by
    // resolution, and the set that *appears in any family list*.
    // Imports of family members that weren't chosen get stripped
    // from the rewritten source — otherwise the downstream analyzer
    // emits "Undefined component type" for them (it doesn't tolerate
    // imported-but-unused entries from the same file).
    let mut chosen: HashSet<String> = HashSet::new();
    let mut all_family_members: HashSet<String> = HashSet::new();
    for (_name, decl) in &decls {
        for fam in &decl.family {
            all_family_members.insert(fam.concrete_name.clone());
        }
    }
    for (_inst, (concrete, _)) in &resolutions {
        chosen.insert(concrete.clone());
    }
    let stripped_imports: HashSet<String> = all_family_members
        .difference(&chosen)
        .cloned()
        .collect();

    let rewritten = rewrite(source, &decls, &instances, &resolutions, &stripped_imports);

    // Build the (inst_name, abstract_entity, concrete_sku) list for callers.
    let resolved_list: Vec<(String, String, String)> = instances.iter()
        .filter_map(|(inst_name, entity_type, _)| {
            resolutions.get(inst_name).map(|(concrete, _)|
                (inst_name.clone(), entity_type.clone(), concrete.clone()))
        })
        .collect();

    Ok((rewritten, resolved_list))
}

/// Parsed abstract-entity declaration.
struct AbstractDecl {
    /// Source byte range of the entire `abstract entity NAME { ... }`
    /// block — gets stripped on rewrite.
    range: std::ops::Range<usize>,
    /// The abstract entity's port set — the set of pin names the
    /// abstract entity declares (e.g. `pin vcc: signal inout;`).
    /// This is the surface board authors read to know what they can
    /// reference. Each family entry's pin_map MUST map only these
    /// names (validated at extract time); a board referencing a
    /// name not in this set is a board-side error reported by
    /// preprocess().
    abstract_ports: HashSet<String>,
    /// Ordered list of family entries (preference order).
    family: Vec<FamilyEntry>,
}

struct FamilyEntry {
    concrete_name: String,
    /// alias_name → concrete_pin_name
    pin_map: HashMap<String, String>,
}

fn extract_abstract_decls(source: &str) -> HashMap<String, AbstractDecl> {
    let mut out: HashMap<String, AbstractDecl> = HashMap::new();
    let bytes = source.as_bytes();
    let mut cursor = 0;
    while let Some(rel) = source[cursor..].find("abstract") {
        let kw_start = cursor + rel;
        // Word boundary.
        if kw_start > 0 {
            let prev = bytes[kw_start - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' {
                cursor = kw_start + 1; continue;
            }
        }
        let after_kw = kw_start + "abstract".len();
        let after = source[after_kw..].trim_start();
        let after_skip = after_kw + (source.len() - after_kw - after.len());
        if !after.starts_with("entity") {
            cursor = after_kw; continue;
        }
        let after_entity = after_skip + "entity".len();
        let rest = source[after_entity..].trim_start();
        let name_start = after_entity + (source.len() - after_entity - rest.len());
        let name_end = name_start + rest.bytes().take_while(|b|
            b.is_ascii_alphanumeric() || *b == b'_').count();
        if name_end == name_start { cursor = after_entity; continue; }
        let name = source[name_start..name_end].to_string();

        let Some(rel_brace) = source[name_end..].find('{') else {
            cursor = name_end; continue;
        };
        let block_open = name_end + rel_brace;
        let block_close = match find_matching_brace(source, block_open) {
            Some(c) => c,
            None => { cursor = block_open + 1; continue; }
        };
        let raw_block_body = &source[block_open + 1..block_close];
        // Strip comments so words like "family" inside doc comments
        // don't false-match the family-keyword scan below.
        let stripped_body = strip_block_comments(raw_block_body);
        let block_body = stripped_body.as_str();

        // Within the abstract block, find `family { ... }`. Pin
        // declarations live before it (between the abstract block's
        // `{` and `family`); they declare the abstract port set.
        // Use word-boundary scan to avoid matching "family" inside
        // identifiers (e.g. `family_param`).
        let family_positions = find_keyword_positions(block_body, "family");
        let family_pos_in_body = family_positions.first().copied();
        let pin_decl_region = match family_pos_in_body {
            Some(rel) => &block_body[..rel],
            None => block_body,
        };
        let abstract_ports = parse_port_decls(pin_decl_region);

        let family = match family_pos_in_body {
            Some(fam_rel) => {
                let fam_kw_pos = block_open + 1 + fam_rel;
                let after_fam_kw = fam_kw_pos + "family".len();
                let after_fam = source[after_fam_kw..].trim_start();
                let after_fam_skip = after_fam_kw + (source.len() - after_fam_kw - after_fam.len());
                if after_fam.starts_with('{') {
                    let fam_open = after_fam_skip;
                    let fam_close = find_matching_brace(source, fam_open)
                        .unwrap_or(block_close);
                    parse_family_entries(&source[fam_open + 1..fam_close])
                } else {
                    Vec::new()
                }
            }
            None => Vec::new(),
        };

        out.insert(name, AbstractDecl {
            range: kw_start..(block_close + 1),
            abstract_ports,
            family,
        });
        cursor = block_close + 1;
    }
    out
}

/// Inside the abstract entity's body but BEFORE the `family` block,
/// extract every `pin NAME: …;` declaration's NAME. These are the
/// names a board author may use on instances of the abstract entity.
fn parse_port_decls(body: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let bytes = body.as_bytes();
    let mut search = 0;
    while let Some(rel) = body[search..].find("pin") {
        let pos = search + rel;
        // Word-boundary check.
        let before_ok = pos == 0
            || !(bytes[pos - 1].is_ascii_alphanumeric() || bytes[pos - 1] == b'_');
        let after_kw = pos + "pin".len();
        let after_ok = after_kw < body.len()
            && bytes[after_kw].is_ascii_whitespace();
        if !(before_ok && after_ok) {
            search = pos + 1; continue;
        }
        // Read the IDENT after `pin `.
        let rest = body[after_kw..].trim_start();
        let name_start = after_kw + (body.len() - after_kw - rest.len());
        let name_end = name_start + rest.bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_').count();
        if name_end > name_start {
            // Sanity: next non-whitespace char should be `:` (the type
            // separator) to avoid matching `pin foo` in prose comments
            // that survived the strip.
            let after_name = name_end + body[name_end..].bytes()
                .take_while(|b| b.is_ascii_whitespace()).count();
            if after_name < body.len() && bytes[after_name] == b':' {
                out.insert(body[name_start..name_end].to_string());
            }
        }
        search = name_end.max(pos + 1);
    }
    out
}

/// Inside a `family { ... }` block body, parse each entry of the form:
///
///     CONCRETE_NAME { alias1 = pin1; alias2 = pin2; ... };
///
/// or the (compat) bare form:
///
///     CONCRETE_NAME;
///
/// The bare form is allowed for "no pin_map needed" cases — boards
/// using it can only reference pins that exist on the concrete
/// entity directly (no abstract aliases).
fn parse_family_entries(body: &str) -> Vec<FamilyEntry> {
    // Strip comments first so an inline `// ...` on one line doesn't
    // eat the following line when we later split on `;`. Byte
    // offsets are preserved (each comment char becomes a space or
    // newline) so positions in the stripped body still align with
    // the original.
    let stripped = strip_block_comments(body);
    let body = stripped.as_str();
    let mut out = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < body.len() {
        // Skip whitespace and `//` line comments.
        while i < body.len() {
            let c = bytes[i];
            if c.is_ascii_whitespace() {
                i += 1;
            } else if c == b'/' && i + 1 < body.len() && bytes[i + 1] == b'/' {
                while i < body.len() && bytes[i] != b'\n' { i += 1; }
            } else {
                break;
            }
        }
        if i >= body.len() { break; }
        let c = bytes[i];
        if !(c.is_ascii_alphabetic() || c == b'_') {
            i += 1; continue;
        }
        // Read CONCRETE_NAME.
        let name_start = i;
        while i < body.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
        let concrete_name = body[name_start..i].to_string();
        // Skip whitespace.
        while i < body.len() && bytes[i].is_ascii_whitespace() { i += 1; }
        if i >= body.len() { break; }

        let mut pin_map = HashMap::new();
        if bytes[i] == b'{' {
            // Pin-map block.
            let map_open = i;
            // Find matching brace within `body`.
            let map_close = match find_matching_brace(body, map_open) {
                Some(c) => c,
                None => break,
            };
            let map_body = &body[map_open + 1..map_close];
            for stmt in map_body.split(';') {
                // Comments already stripped at function entry.
                let cleaned = stmt.trim();
                if cleaned.is_empty() { continue; }
                // Parse `alias = pin`.
                if let Some(eq_pos) = cleaned.find('=') {
                    let alias = cleaned[..eq_pos].trim();
                    let pin = cleaned[eq_pos + 1..].trim();
                    if !alias.is_empty() && !pin.is_empty() {
                        pin_map.insert(alias.to_string(), pin.to_string());
                    }
                }
            }
            i = map_close + 1;
            // Skip whitespace.
            while i < body.len() && bytes[i].is_ascii_whitespace() { i += 1; }
        }
        // Expect a trailing semicolon (after a `{ ... }` block or bare).
        if i < body.len() && bytes[i] == b';' { i += 1; }

        out.push(FamilyEntry { concrete_name, pin_map });
    }
    out
}

/// Replace `//…` line and `/* … */` block comments with spaces so
/// keyword/word scans don't match content inside doc comments.
/// Byte offsets are preserved (each comment char becomes a space
/// or newline), so positions in the stripped string still map
/// 1:1 to positions in the original source.
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

/// Word-boundary keyword-position scan over a source string.
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

fn extract_abstract_instances(
    source: &str,
    abstract_decls: &HashMap<String, AbstractDecl>,
) -> Vec<(String, String, std::ops::Range<usize>)> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < source.len() {
        let c = bytes[i];
        if !(c.is_ascii_alphabetic() || c == b'_') { i += 1; continue; }
        let ident1_start = i;
        while i < source.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
        let ident1_end = i;
        let after1 = i + source[i..].bytes().take_while(|b| b.is_ascii_whitespace()).count();
        if after1 >= source.len() || bytes[after1] != b':' { continue; }
        let mut j = after1 + 1;
        j += source[j..].bytes().take_while(|b| b.is_ascii_whitespace()).count();
        if j >= source.len() { continue; }
        let c2 = bytes[j];
        if !(c2.is_ascii_alphabetic() || c2 == b'_') { continue; }
        let ident2_start = j;
        while j < source.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
            j += 1;
        }
        let ident2_end = j;
        let after2 = j + source[j..].bytes().take_while(|b| b.is_ascii_whitespace()).count();
        if after2 >= source.len() || bytes[after2] != b'(' { continue; }

        let inst_name = source[ident1_start..ident1_end].to_string();
        let entity_type = source[ident2_start..ident2_end].to_string();
        if abstract_decls.contains_key(&entity_type) {
            out.push((inst_name, entity_type, ident2_start..ident2_end));
        }
        i = after2;
    }
    out
}

fn extract_wired_pins(source: &str, inst_name: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let prefix = format!("{}.", inst_name);
    let bytes = source.as_bytes();
    let mut search_start = 0;
    while let Some(rel) = source[search_start..].find(&prefix) {
        let dot_pos = search_start + rel + inst_name.len();
        let before_idx = search_start + rel;
        if before_idx > 0 {
            let prev = bytes[before_idx - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' {
                search_start = dot_pos + 1; continue;
            }
        }
        let pin_start = dot_pos + 1;
        let mut k = pin_start;
        while k < source.len() && (bytes[k].is_ascii_alphanumeric() || bytes[k] == b'_') {
            k += 1;
        }
        if k > pin_start {
            // Skip further-dotted refs (`mcu.spi.MOSI`) — interface-field
            // refs are not raw pin/alias names.
            if k < source.len() && bytes[k] == b'.' {
                search_start = k; continue;
            }
            out.insert(source[pin_start..k].to_string());
        }
        search_start = k.max(dot_pos + 1);
    }
    out
}

/// Build the rewritten source. Three kinds of edits in priority order
/// (apply in reverse byte-order so offsets stay valid):
///   - Strip each `abstract entity` declaration entirely.
///   - For each abstract instance, rewrite its type IDENT to the
///     chosen concrete name.
///   - For each abstract instance, rewrite each `inst_name.alias` →
///     `inst_name.<concrete_pin>` per the chosen pin_map.
fn rewrite(
    source: &str,
    abstract_decls: &HashMap<String, AbstractDecl>,
    instances: &[(String, String, std::ops::Range<usize>)],
    resolutions: &HashMap<String, (String, HashMap<String, String>)>,
    stripped_imports: &HashSet<String>,
) -> String {
    type Edit = (std::ops::Range<usize>, String);
    let mut edits: Vec<Edit> = Vec::new();

    // (a) Strip abstract entity declarations.
    for decl in abstract_decls.values() {
        edits.push((decl.range.clone(), String::new()));
    }

    // (a.5) Strip imports of family members that weren't chosen.
    // Looks for the pattern `import { ConcreteName } from "...";` —
    // the entire line gets removed. Imports that pull multiple
    // entries are left alone (rare in stdlib usage).
    for entity_name in stripped_imports {
        // Find every `import { ENTITY }` occurrence and remove the
        // enclosing statement.
        let pattern = format!("import {{ {} }}", entity_name);
        let mut search = 0;
        while let Some(rel) = source[search..].find(&pattern) {
            let pos = search + rel;
            // Find the start of the line (preserve preceding ws).
            let line_start = source[..pos].rfind('\n')
                .map(|i| i + 1).unwrap_or(0);
            // Find the end of the statement (next ';' then to end-of-line).
            let semi = source[pos..].find(';').map(|i| pos + i + 1);
            let end = match semi {
                Some(semi_pos) => {
                    let after = &source[semi_pos..];
                    let nl = after.find('\n').map(|i| semi_pos + i + 1);
                    nl.unwrap_or(semi_pos)
                }
                None => pos + pattern.len(),
            };
            edits.push((line_start..end, String::new()));
            search = end;
        }
    }

    // (b) Rewrite instance type tokens + per-pin alias rewrites.
    let bytes = source.as_bytes();
    for (inst_name, _entity_type, type_range) in instances {
        let Some((concrete, pin_map)) = resolutions.get(inst_name) else { continue; };
        // Rewrite the type IDENT.
        edits.push((type_range.clone(), concrete.clone()));

        // Walk `inst_name.alias` occurrences and rewrite each pin.
        let prefix = format!("{}.", inst_name);
        let mut search = 0;
        while let Some(rel) = source[search..].find(&prefix) {
            let dot_pos = search + rel + inst_name.len();
            let before_idx = search + rel;
            if before_idx > 0 {
                let prev = bytes[before_idx - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' {
                    search = dot_pos + 1; continue;
                }
            }
            let pin_start = dot_pos + 1;
            let mut k = pin_start;
            while k < source.len() && (bytes[k].is_ascii_alphanumeric() || bytes[k] == b'_') {
                k += 1;
            }
            if k > pin_start {
                if k < source.len() && bytes[k] == b'.' {
                    search = k; continue;
                }
                let alias = &source[pin_start..k];
                if let Some(concrete_pin) = pin_map.get(alias) {
                    edits.push((pin_start..k, concrete_pin.clone()));
                }
            }
            search = k.max(dot_pos + 1);
        }
    }

    edits.sort_by(|a, b| b.0.start.cmp(&a.0.start));
    let mut out = source.to_string();
    for (range, replacement) in edits {
        out.replace_range(range, &replacement);
    }
    out
}
