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

/// Source-text preprocessing for abstract-entity resolution.
///
/// Returns the rewritten source. If the input has no abstract-entity
/// declarations, the returned string equals the input verbatim.
pub fn preprocess(source: &str) -> Result<String> {
    let decls = extract_abstract_decls(source);
    if decls.is_empty() {
        return Ok(source.to_string());
    }

    // Find every `inst_name: ABSTRACT_NAME(...)` instance.
    let instances = extract_abstract_instances(source, &decls);

    // Resolve each instance + collect per-instance pin-alias rewrites.
    type AliasRewrite = HashMap<String, String>; // alias → concrete pin
    let mut resolutions: HashMap<String, (String, AliasRewrite)> = HashMap::new();
    for (inst_name, entity_type, _type_range) in &instances {
        let decl = &decls[entity_type];
        let wired_aliases = extract_wired_pins(source, inst_name);

        let chosen = decl.family.iter().find(|fam| {
            wired_aliases.iter().all(|a| fam.pin_map.contains_key(a))
        });
        match chosen {
            Some(fam) => {
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

    Ok(rewrite(source, &decls, &instances, &resolutions))
}

/// Parsed abstract-entity declaration.
struct AbstractDecl {
    /// Source byte range of the entire `abstract entity NAME { ... }`
    /// block — gets stripped on rewrite.
    range: std::ops::Range<usize>,
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
        let block_body = &source[block_open + 1..block_close];

        // Within the abstract block, find `family { ... }`.
        let family = match block_body.find("family") {
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
            family,
        });
        cursor = block_close + 1;
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
                let cleaned = stmt.split("//").next().unwrap_or("").trim();
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
) -> String {
    type Edit = (std::ops::Range<usize>, String);
    let mut edits: Vec<Edit> = Vec::new();

    // (a) Strip abstract entity declarations.
    for decl in abstract_decls.values() {
        edits.push((decl.range.clone(), String::new()));
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
