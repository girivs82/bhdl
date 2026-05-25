//! Resolution of KiCad library-symbol references.
//!
//! Phase B of the KiCad-to-BHDL translator pipeline. KiCad symbols
//! are identified by `Library:SymbolName` (e.g. `Device:R`,
//! `MCU_ST_STM32F4:STM32F411RETx`). Modern KiCad files usually
//! embed the symbol definitions they use in a `(lib_symbols ...)`
//! section of the `.kicad_sch`, but older or hand-authored files
//! may reference external `.kicad_sym` library files via a
//! `sym-lib-table`.
//!
//! This module provides:
//!
//! - [`parse_kicad_sym_file`] / [`parse_kicad_sym_str`] — read a
//!   `.kicad_sym` library file (or string) and produce
//!   [`LibSymbol`]s.
//! - [`parse_sym_lib_table_file`] / [`parse_sym_lib_table_str`] —
//!   read a `sym-lib-table` file mapping library nicknames to
//!   paths.
//! - [`LibraryResolver`] — caches embedded + external libraries
//!   and answers `lookup(lib_id)` queries.
//!
//! For real-world `.kicad_sch` files the embedded library is
//! usually sufficient; the external-file fallback is a safety
//! net. Both paths are implemented because some boards (especially
//! older ones or those using custom libraries) lean on the
//! external lookup.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ir::LibSymbol;
use crate::sexpr::Sexpr;

/// Errors specific to library resolution.
#[derive(Debug, thiserror::Error)]
pub enum LibResolveError {
    #[error("file I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("S-expr parse error: {0}")]
    Parse(#[from] crate::sexpr::ParseError),

    #[error("malformed sym-lib-table at {0}: {1}")]
    MalformedSymLibTable(PathBuf, String),

    #[error("malformed .kicad_sym file: {0}")]
    MalformedSymLib(String),

    #[error("library nickname {0:?} not found in any sym-lib-table")]
    UnknownLibrary(String),
}

/// One entry in a `sym-lib-table` — maps a library nickname (like
/// "Device") to a file path (or URL, for git-hosted libraries).
#[derive(Debug, Clone)]
pub struct SymLibTableEntry {
    /// Nickname used in `lib_id` references (`Device` in `Device:R`).
    pub name: String,
    /// File path or `${KICAD_X_SYMBOL_DIR}` template. Tilde and env
    /// vars are NOT expanded here — that's the resolver's job once
    /// it knows the host environment.
    pub uri: String,
    /// Library type — usually "KiCad" for S-expr-format libs.
    pub kind: String,
    /// Options string (rare; kept for round-trip fidelity).
    pub options: String,
    /// Description (informational).
    pub descr: String,
}

/// Parse a sym-lib-table source string.
pub fn parse_sym_lib_table_str(src: &str) -> Result<Vec<SymLibTableEntry>, LibResolveError> {
    let sexpr = crate::sexpr::parse(src)?;
    let entries = sexpr.match_list("sym_lib_table").ok_or_else(|| {
        LibResolveError::MalformedSymLibTable(
            PathBuf::new(),
            "expected top-level (sym_lib_table ...)".into(),
        )
    })?;

    let mut out = Vec::new();
    for entry in entries {
        if let Some(fields) = entry.match_list("lib") {
            let mut e = SymLibTableEntry {
                name: String::new(),
                uri: String::new(),
                kind: String::new(),
                options: String::new(),
                descr: String::new(),
            };
            for field in fields {
                if let Some(args) = field.match_list("name") {
                    e.name = first_string(args).unwrap_or_default();
                } else if let Some(args) = field.match_list("type") {
                    e.kind = first_string(args).unwrap_or_default();
                } else if let Some(args) = field.match_list("uri") {
                    e.uri = first_string(args).unwrap_or_default();
                } else if let Some(args) = field.match_list("options") {
                    e.options = first_string(args).unwrap_or_default();
                } else if let Some(args) = field.match_list("descr") {
                    e.descr = first_string(args).unwrap_or_default();
                }
            }
            if !e.name.is_empty() && !e.uri.is_empty() {
                out.push(e);
            }
        }
    }
    Ok(out)
}

/// Read a sym-lib-table file from disk.
pub fn parse_sym_lib_table_file(path: &Path) -> Result<Vec<SymLibTableEntry>, LibResolveError> {
    let src = std::fs::read_to_string(path)?;
    parse_sym_lib_table_str(&src).map_err(|e| match e {
        LibResolveError::MalformedSymLibTable(_, reason) =>
            LibResolveError::MalformedSymLibTable(path.to_path_buf(), reason),
        other => other,
    })
}

/// Parse a `.kicad_sym` library source string. Returns all
/// `LibSymbol`s defined in the file. Library files have a
/// `(kicad_symbol_lib ...)` top-level wrapper with `(symbol "...")`
/// children — same shape as the embedded library inside a
/// `.kicad_sch`.
pub fn parse_kicad_sym_str(src: &str) -> Result<Vec<LibSymbol>, LibResolveError> {
    let sexpr = crate::sexpr::parse(src)?;
    let items = sexpr.match_list("kicad_symbol_lib").ok_or_else(|| {
        LibResolveError::MalformedSymLib("expected top-level (kicad_symbol_lib ...)".into())
    })?;

    // The reader's lib-symbol parser is what we want here; rather
    // than duplicating logic, expose it through a thin wrapper.
    let mut out = Vec::new();
    for item in items {
        if item.match_list("symbol").is_some() {
            if let Some(sym) = crate::reader::parse_lib_symbol_public(item) {
                out.push(sym);
            }
        }
    }
    Ok(out)
}

/// Read a `.kicad_sym` library file from disk.
pub fn parse_kicad_sym_file(path: &Path) -> Result<Vec<LibSymbol>, LibResolveError> {
    let src = std::fs::read_to_string(path)?;
    parse_kicad_sym_str(&src)
}

/// Caches embedded + external library symbols and answers
/// `lookup(lib_id)` queries.
///
/// Build one of these per import session. Add embedded libraries
/// from the `.kicad_sch` first (highest priority); register
/// external library paths via `add_library_path`; resolve symbols
/// via `lookup`.
pub struct LibraryResolver {
    /// Symbols indexed by `Library:Symbol` lib_id. Insertion order
    /// = priority order: embedded first, then external.
    symbols: HashMap<String, LibSymbol>,
    /// External libraries: name → file path. Loaded lazily on
    /// first lookup that doesn't hit the embedded set.
    external_paths: HashMap<String, PathBuf>,
    /// Tracks which external libraries we've already loaded (so we
    /// don't keep re-parsing the same file for repeated misses).
    loaded_external: std::collections::HashSet<String>,
}

impl LibraryResolver {
    /// Empty resolver. Add symbols + paths before resolving.
    pub fn new() -> Self {
        Self {
            symbols: HashMap::new(),
            external_paths: HashMap::new(),
            loaded_external: std::collections::HashSet::new(),
        }
    }

    /// Add a library symbol to the embedded set. If the same lib_id
    /// is already present (e.g. from a previously-loaded sheet),
    /// the new entry replaces the old — embedded library symbols
    /// are expected to be identical across sheets that reference
    /// the same KiCad part.
    pub fn add_embedded(&mut self, sym: LibSymbol) {
        self.symbols.insert(sym.lib_id.clone(), sym);
    }

    /// Add all embedded library symbols from a parsed sheet.
    pub fn add_embedded_from_sheet(&mut self, sheet: &crate::ir::Sheet) {
        for sym in &sheet.lib_symbols {
            self.add_embedded(sym.clone());
        }
    }

    /// Register an external library file by nickname. The library
    /// is loaded lazily on first `lookup` that needs it.
    pub fn add_library_path(&mut self, name: impl Into<String>, path: impl Into<PathBuf>) {
        self.external_paths.insert(name.into(), path.into());
    }

    /// Register every entry from a parsed sym-lib-table. URI
    /// templates (containing `${KICAD_*_SYMBOL_DIR}`) are expanded
    /// using `env_vars` — typically the host's environment plus
    /// any KiCad-installation overrides.
    pub fn add_sym_lib_table(
        &mut self,
        entries: &[SymLibTableEntry],
        env_vars: &HashMap<String, String>,
    ) {
        for entry in entries {
            // Only handle KiCad-format S-expr libraries. Older
            // "Legacy" .lib files would need a different parser.
            if entry.kind != "KiCad" && !entry.kind.is_empty() {
                continue;
            }
            let path = expand_env_vars(&entry.uri, env_vars);
            self.external_paths.insert(entry.name.clone(), PathBuf::from(path));
        }
    }

    /// Look up a library symbol by lib_id (e.g. "Device:R"). On
    /// miss, attempts to load the external library file and try
    /// again. Returns None if neither embedded nor external lookup
    /// succeeds.
    pub fn lookup(&mut self, lib_id: &str) -> Option<&LibSymbol> {
        // Fast path: embedded hit.
        if self.symbols.contains_key(lib_id) {
            return self.symbols.get(lib_id);
        }
        // Slow path: external lookup.
        let (lib_name, _) = match lib_id.split_once(':') {
            Some(pair) => pair,
            None => return None,
        };
        if self.loaded_external.contains(lib_name) {
            return None; // already tried; not there
        }
        self.loaded_external.insert(lib_name.to_string());
        let path = match self.external_paths.get(lib_name) {
            Some(p) => p.clone(),
            None => return None,
        };
        match parse_kicad_sym_file(&path) {
            Ok(symbols) => {
                for sym in symbols {
                    let key = format!("{}:{}", lib_name, strip_lib_prefix(&sym.lib_id, lib_name));
                    let mut sym = sym;
                    sym.lib_id = key.clone();
                    self.symbols.insert(key, sym);
                }
                self.symbols.get(lib_id)
            }
            Err(_) => None,
        }
    }

    /// Number of embedded + cached external symbols.
    pub fn cached_count(&self) -> usize { self.symbols.len() }
}

impl Default for LibraryResolver {
    fn default() -> Self { Self::new() }
}

// ─── helpers ──────────────────────────────────────────────────────

fn first_string(args: &[Sexpr]) -> Option<String> {
    args.first().and_then(|s| s.as_str().or(s.as_symbol())).map(|s| s.to_string())
}

/// Expand `${VAR}` references in a URI template using the provided
/// env-vars map. Leaves unknown variables in place (so missing
/// configuration produces a recognisable error message later).
fn expand_env_vars(template: &str, env: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut name = String::new();
            while let Some(&nc) = chars.peek() {
                if nc == '}' { chars.next(); break; }
                name.push(nc);
                chars.next();
            }
            if let Some(val) = env.get(&name) {
                out.push_str(val);
            } else {
                out.push_str(&format!("${{{}}}", name));
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Library symbols inside a `.kicad_sym` file are named bare (e.g.
/// `"R"`) without the library nickname prefix. Strip a potential
/// `LibName:` prefix to normalise.
fn strip_lib_prefix<'a>(name: &'a str, lib_name: &str) -> &'a str {
    let prefix = format!("{}:", lib_name);
    name.strip_prefix(&prefix).unwrap_or(name)
}

// ─────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sym_lib_table() {
        let src = r#"
            (sym_lib_table
                (version 7)
                (lib (name "Device")(type "KiCad")(uri "${KICAD8_SYMBOL_DIR}/Device.kicad_sym")(options "")(descr "Basic passive devices"))
                (lib (name "power") (type "KiCad")(uri "${KICAD8_SYMBOL_DIR}/power.kicad_sym")(options "")(descr "Power flags"))
                (lib (name "MyLocalLib")(type "KiCad")(uri "./lib/my-symbols.kicad_sym")(options "")(descr "Project-local symbols"))
            )
        "#;
        let entries = parse_sym_lib_table_str(src).expect("parse");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "Device");
        assert_eq!(entries[0].kind, "KiCad");
        assert!(entries[0].uri.contains("Device.kicad_sym"));
        assert_eq!(entries[2].name, "MyLocalLib");
    }

    #[test]
    fn parses_kicad_sym_file() {
        let src = r#"
            (kicad_symbol_lib
                (version 20231120)
                (generator kicad_symbol_editor)
                (symbol "R"
                    (pin_numbers hide)
                    (pin_names (offset 0))
                    (pin passive line (at 0 3.81 270) (length 1.27)
                        (name "~") (number "1"))
                    (pin passive line (at 0 -3.81 90) (length 1.27)
                        (name "~") (number "2")))
                (symbol "LED"
                    (pin passive line (at 0 0 270) (length 2.54)
                        (name "K") (number "1"))
                    (pin passive line (at 0 0 90) (length 2.54)
                        (name "A") (number "2")))
            )
        "#;
        let symbols = parse_kicad_sym_str(src).expect("parse");
        assert_eq!(symbols.len(), 2);
        let r = symbols.iter().find(|s| s.lib_id == "R").expect("R symbol");
        assert_eq!(r.pins.len(), 2);
        let led = symbols.iter().find(|s| s.lib_id == "LED").expect("LED symbol");
        assert_eq!(led.pins.len(), 2);
        assert_eq!(led.pins.iter().find(|p| p.number == "1").unwrap().name, "K");
    }

    #[test]
    fn resolver_finds_embedded_symbols() {
        let mut resolver = LibraryResolver::new();
        // Build a minimal LibSymbol by hand
        let sym = LibSymbol {
            lib_id: "Device:R".to_string(),
            pins: vec![],
            unit_count: 1,
            properties: HashMap::new(),
        };
        resolver.add_embedded(sym);
        assert!(resolver.lookup("Device:R").is_some());
        assert!(resolver.lookup("Device:NotThere").is_none());
    }

    #[test]
    fn resolver_falls_back_to_external_with_caching() {
        // Write a fake .kicad_sym to a tempfile, register its path,
        // verify resolver picks it up.
        let dir = std::env::temp_dir().join("bhdl_kicad_test_lib_resolver");
        std::fs::create_dir_all(&dir).unwrap();
        let lib_path = dir.join("TestLib.kicad_sym");
        std::fs::write(&lib_path, r#"
            (kicad_symbol_lib
                (version 20231120)
                (symbol "MyChip"
                    (pin power_in line (at 0 0 0) (length 1)
                        (name "VCC") (number "1")))
            )
        "#).unwrap();

        let mut resolver = LibraryResolver::new();
        resolver.add_library_path("TestLib", &lib_path);

        let sym = resolver.lookup("TestLib:MyChip").expect("found");
        assert_eq!(sym.pins.len(), 1);
        assert_eq!(sym.pins[0].name, "VCC");

        // Second lookup should hit the cache without re-parsing.
        assert!(resolver.lookup("TestLib:MyChip").is_some());

        // Lookup of a name in the same library but not in the file
        // should fail (the library was loaded; no second attempt).
        assert!(resolver.lookup("TestLib:NotThere").is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn env_var_expansion_in_uris() {
        let mut env = HashMap::new();
        env.insert("KICAD8_SYMBOL_DIR".into(), "/usr/share/kicad/symbols".into());
        let result = expand_env_vars(
            "${KICAD8_SYMBOL_DIR}/Device.kicad_sym",
            &env,
        );
        assert_eq!(result, "/usr/share/kicad/symbols/Device.kicad_sym");

        // Missing var is preserved as a literal (so debugging is
        // clearer than silent empty-string substitution).
        let result2 = expand_env_vars("${MISSING}/foo", &env);
        assert_eq!(result2, "${MISSING}/foo");
    }
}
