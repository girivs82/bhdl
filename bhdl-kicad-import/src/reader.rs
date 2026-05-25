//! Build the typed IR from a parsed S-expression tree.
//!
//! This is where the KiCad-specific structure shows: we walk the
//! S-expr looking for known constructs (`symbol`, `wire`, `label`,
//! …) and project them into the typed IR.
//!
//! Unrecognised constructs are quietly skipped — KiCad's format
//! has fields we don't care about (`stroke`, `effects`, font
//! styling) and version-specific additions we don't yet support.
//! The `debug!` log lines noting them help future expansion.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ir::*;
use crate::sexpr::Sexpr;

/// Errors specific to IR-building (S-expr structure didn't match
/// what we expected). Parsing errors come from [`crate::sexpr::ParseError`].
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("file I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("S-expr parse error: {0}")]
    Parse(#[from] crate::sexpr::ParseError),

    #[error("expected top-level (kicad_sch ...) but got something else")]
    NotASchematic,

    #[error("missing required field {field:?} in {context}")]
    MissingField { field: String, context: String },

    #[error("unsupported KiCad format version {version}; supported range is 6.0..=8.x")]
    UnsupportedVersion { version: u32 },

    #[error("malformed {what}: {reason}")]
    Malformed { what: String, reason: String },
}

/// Public entrypoint: read a `.kicad_sch` from disk, follow
/// hierarchical sheet references, return the full schematic.
pub fn read_schematic(path: &Path) -> Result<Schematic, ReadError> {
    let src = std::fs::read_to_string(path)?;
    let sexpr = crate::sexpr::parse(&src)?;
    let root_dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let root_rel = path.file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("root.kicad_sch"));

    let (root_sheet, version, generator) = build_root(&sexpr, root_rel)?;

    let mut child_sheets = HashMap::new();
    for sheet_ref in &root_sheet.sheet_refs {
        let child_path = root_dir.join(&sheet_ref.file_path);
        if child_sheets.contains_key(&sheet_ref.file_path) { continue; }
        if !child_path.exists() {
            // Reference to a sheet file that's not on disk —
            // not fatal (the user may be debugging) but we
            // record an empty placeholder so the connection
            // analysis can still run.
            child_sheets.insert(sheet_ref.file_path.clone(), Sheet {
                path: sheet_ref.file_path.clone(),
                ..Default::default()
            });
            continue;
        }
        let child_src = std::fs::read_to_string(&child_path)?;
        let child_sexpr = crate::sexpr::parse(&child_src)?;
        let (mut child, _, _) = build_root(&child_sexpr, sheet_ref.file_path.clone())?;
        child.path = sheet_ref.file_path.clone();
        child_sheets.insert(sheet_ref.file_path.clone(), child);
    }

    Ok(Schematic {
        root: root_sheet,
        child_sheets,
        version,
        generator,
    })
}

/// Parse a single .kicad_sch source string. Used by tests that
/// want to skip the file-I/O dance and by the recursive sheet
/// loader above.
pub fn read_from_str(source: &str, path: PathBuf) -> Result<Sheet, ReadError> {
    let sexpr = crate::sexpr::parse(source)?;
    let (sheet, _v, _g) = build_root(&sexpr, path)?;
    Ok(sheet)
}

/// Project a parsed `(kicad_sch ...)` top-level into a [`Sheet`]
/// (plus version + generator metadata).
fn build_root(sexpr: &Sexpr, path: PathBuf) -> Result<(Sheet, u32, String), ReadError> {
    let items = sexpr.match_list("kicad_sch").ok_or(ReadError::NotASchematic)?;

    let mut sheet = Sheet { path, ..Default::default() };
    let mut version = 0u32;
    let mut generator = String::new();
    let mut lib_symbols: Vec<LibSymbol> = Vec::new();

    for item in items {
        if let Some(args) = item.match_list("version") {
            version = args.first().and_then(|s| s.as_num()).unwrap_or(0.0) as u32;
        } else if let Some(args) = item.match_list("generator") {
            generator = args.first().and_then(|s| s.as_symbol().or(s.as_str()))
                .unwrap_or("").to_string();
        } else if let Some(args) = item.match_list("uuid") {
            sheet.uuid = args.first().and_then(|s| s.as_symbol().or(s.as_str()))
                .unwrap_or("").to_string();
        } else if let Some(args) = item.match_list("title_block") {
            sheet.title_block = Some(parse_title_block(args));
        } else if let Some(args) = item.match_list("lib_symbols") {
            for sym in args {
                if let Some(lib_sym) = parse_lib_symbol(sym) {
                    lib_symbols.push(lib_sym);
                }
            }
        } else if let Some(args) = item.match_list("symbol") {
            // A schematic-level symbol instance OR a library symbol
            // (when the latter is inline-defined at top level by
            // some old files). Disambiguate by presence of `lib_id`.
            if args.iter().any(|a| a.match_list("lib_id").is_some()) {
                // Schematic instance.
                if let Some(s) = parse_schematic_symbol(args, &lib_symbols) {
                    sheet.symbols.push(s);
                }
            } else {
                // Top-level library symbol (rare; keep for compat).
                if let Some(name_sexpr) = args.first() {
                    if name_sexpr.as_str().is_some() || name_sexpr.as_symbol().is_some() {
                        let mut sym_items = vec![Sexpr::Symbol("symbol".into())];
                        sym_items.extend(args.iter().cloned());
                        let wrapped = Sexpr::List(sym_items);
                        if let Some(lib_sym) = parse_lib_symbol(&wrapped) {
                            lib_symbols.push(lib_sym);
                        }
                    }
                }
            }
        } else if let Some(args) = item.match_list("wire") {
            if let Some(w) = parse_wire(args) {
                sheet.wires.push(w);
            }
        } else if let Some(args) = item.match_list("junction") {
            if let Some(j) = parse_junction(args) {
                sheet.junctions.push(j);
            }
        } else if let Some(args) = item.match_list("no_connect") {
            if let Some(nc) = parse_no_connect(args) {
                sheet.no_connects.push(nc);
            }
        } else if let Some(args) = item.match_list("label") {
            if let Some(l) = parse_label(args) {
                sheet.labels.push(l);
            }
        } else if let Some(args) = item.match_list("global_label") {
            if let Some(gl) = parse_global_label(args) {
                sheet.global_labels.push(gl);
            }
        } else if let Some(args) = item.match_list("hierarchical_label") {
            if let Some(hl) = parse_hierarchical_label(args) {
                sheet.hierarchical_labels.push(hl);
            }
        } else if let Some(args) = item.match_list("sheet") {
            if let Some(sr) = parse_sheet_ref(args) {
                sheet.sheet_refs.push(sr);
            }
        }
        // Everything else (paper, sheet_instances, symbol_instances,
        // bus, bus_entry, polyline, image, …) is silently skipped
        // for v0.1. Phase A's job is to read the constructs that
        // affect connectivity + identity; we'll expand coverage as
        // later boards demand it.
    }

    sheet.lib_symbols = lib_symbols;

    // Post-process: classify schematic-level symbols that are
    // actually power flags. KiCad's power flags use the `power`
    // library; their lib_id starts with `power:` (e.g. `power:GND`,
    // `power:+5V`).
    let mut regular_symbols = Vec::new();
    for sym in sheet.symbols.drain(..) {
        if is_power_flag_lib_id(&sym.lib_id) {
            let label = sym.value().unwrap_or(&sym.lib_id).to_string();
            let (category, voltage) = classify_power_label(&label);
            sheet.power_symbols.push(PowerSymbol {
                label,
                at: sym.at,
                category,
                voltage,
                uuid: sym.uuid,
            });
        } else {
            regular_symbols.push(sym);
        }
    }
    sheet.symbols = regular_symbols;

    // Version check: tolerate KiCad 6, 7, 8 (versions 20211014..);
    // older files (KiCad 5.x with format 20200214 and below) are
    // explicitly unsupported and produce a clear error.
    if version > 0 && version < 20211014 {
        return Err(ReadError::UnsupportedVersion { version });
    }

    Ok((sheet, version, generator))
}

fn parse_title_block(args: &[Sexpr]) -> TitleBlock {
    let mut tb = TitleBlock::default();
    for a in args {
        if let Some(t) = a.match_list("title").and_then(|x| x.first()).and_then(|s| s.as_str()) {
            tb.title = Some(t.to_string());
        } else if let Some(t) = a.match_list("date").and_then(|x| x.first()).and_then(|s| s.as_str()) {
            tb.date = Some(t.to_string());
        } else if let Some(t) = a.match_list("rev").and_then(|x| x.first()).and_then(|s| s.as_str()) {
            tb.rev = Some(t.to_string());
        } else if let Some(t) = a.match_list("company").and_then(|x| x.first()).and_then(|s| s.as_str()) {
            tb.company = Some(t.to_string());
        } else if let Some(c) = a.match_list("comment") {
            // (comment 1 "text") — second arg is the comment
            if let Some(t) = c.get(1).and_then(|s| s.as_str()) {
                tb.comments.push(t.to_string());
            }
        }
    }
    tb
}

/// Public wrapper used by `lib_resolver` to parse `(symbol ...)` forms
/// from external `.kicad_sym` library files into the same `LibSymbol` IR
/// used for embedded `lib_symbols`.
pub fn parse_lib_symbol_public(item: &Sexpr) -> Option<LibSymbol> {
    parse_lib_symbol(item)
}

fn parse_lib_symbol(item: &Sexpr) -> Option<LibSymbol> {
    let args = item.match_list("symbol")?;
    let lib_id = args.first().and_then(|s| s.as_str().or(s.as_symbol()))?.to_string();
    let mut pins = Vec::new();
    let mut unit_count = 1u32;
    let mut properties = HashMap::new();

    // Walk the symbol body. Pin definitions can be at the top level
    // OR inside sub-symbol blocks (for multi-unit ICs — each unit
    // is its own `(symbol "Foo_1_1" ...)` sub-list).
    for a in args.iter().skip(1) {
        if let Some(pin_args) = a.match_list("pin") {
            if let Some(p) = parse_lib_pin(pin_args, 1) {
                pins.push(p);
            }
        } else if let Some(sub_args) = a.match_list("symbol") {
            // Nested symbol = a unit. Name suffix encodes the unit index:
            // "Foo_1_1" is unit 1, body variant 1.
            if let Some(sub_name) = sub_args.first().and_then(|s| s.as_str().or(s.as_symbol())) {
                let unit_idx = parse_unit_index_from_name(sub_name).unwrap_or(1);
                unit_count = unit_count.max(unit_idx);
                for sub_a in sub_args.iter().skip(1) {
                    if let Some(pin_args) = sub_a.match_list("pin") {
                        if let Some(p) = parse_lib_pin(pin_args, unit_idx) {
                            pins.push(p);
                        }
                    }
                }
            }
        } else if let Some(prop_args) = a.match_list("property") {
            if let (Some(key), Some(val)) = (
                prop_args.get(0).and_then(|s| s.as_str()),
                prop_args.get(1).and_then(|s| s.as_str()),
            ) {
                properties.insert(key.to_string(), val.to_string());
            }
        }
    }

    Some(LibSymbol { lib_id, pins, unit_count, properties })
}

fn parse_lib_pin(args: &[Sexpr], unit_index: u32) -> Option<LibPin> {
    // KiCad pin shape:
    // (pin electrical_type line (at x y rot) (length L) (name "N" ...) (number "1" ...))
    let electrical_type = args.first()
        .and_then(|s| s.as_symbol())
        .map(PinElectricalType::from_kicad)
        .unwrap_or(PinElectricalType::Unspecified);

    let at = args.iter().find_map(|a| a.match_list("at"))
        .map(parse_at_3)
        .unwrap_or((0.0, 0.0, 0.0));

    let name = args.iter().find_map(|a| a.match_list("name"))
        .and_then(|x| x.first())
        .and_then(|s| s.as_str())
        .unwrap_or("~")
        .to_string();

    let number = args.iter().find_map(|a| a.match_list("number"))
        .and_then(|x| x.first())
        .and_then(|s| s.as_str())
        .unwrap_or("?")
        .to_string();

    Some(LibPin { number, name, electrical_type, unit_index, at })
}

/// Names like `R_0_1`, `LM358_1_1`, `LM358_2_1` — the first integer
/// after the part name is the unit index.
fn parse_unit_index_from_name(name: &str) -> Option<u32> {
    // Find last two underscores; the segment between them is the
    // unit index in KiCad's convention.
    let parts: Vec<&str> = name.rsplitn(3, '_').collect();
    parts.get(1).and_then(|s| s.parse().ok())
}

fn parse_schematic_symbol(args: &[Sexpr], lib_symbols: &[LibSymbol]) -> Option<SchematicSymbol> {
    let lib_id = args.iter().find_map(|a| a.match_list("lib_id"))
        .and_then(|x| x.first())
        .and_then(|s| s.as_str().or(s.as_symbol()))?
        .to_string();
    let uuid = args.iter().find_map(|a| a.match_list("uuid"))
        .and_then(|x| x.first())
        .and_then(|s| s.as_symbol().or(s.as_str()))
        .unwrap_or("")
        .to_string();
    let at = args.iter().find_map(|a| a.match_list("at"))
        .map(parse_at_3)
        .unwrap_or((0.0, 0.0, 0.0));
    let mirror = args.iter().find_map(|a| a.match_list("mirror"))
        .and_then(|x| x.first())
        .and_then(|s| s.as_symbol().or(s.as_str()))
        .map(String::from);
    let unit = args.iter().find_map(|a| a.match_list("unit"))
        .and_then(|x| x.first())
        .and_then(|s| s.as_num())
        .map(|n| n as u32)
        .unwrap_or(1);

    let in_bom = bool_field(args, "in_bom").unwrap_or(true);
    let on_board = bool_field(args, "on_board").unwrap_or(true);
    let dnp = bool_field(args, "dnp").unwrap_or(false);

    let mut properties: HashMap<String, SymbolProperty> = HashMap::new();
    for a in args {
        if let Some(prop_args) = a.match_list("property") {
            if let (Some(key), Some(val)) = (
                prop_args.get(0).and_then(|s| s.as_str()),
                prop_args.get(1).and_then(|s| s.as_str()),
            ) {
                let prop_at = prop_args.iter().find_map(|a| a.match_list("at")).map(parse_at_3);
                let hidden = prop_args.iter().any(|a|
                    a.match_list("effects").map(|fx|
                        fx.iter().any(|f| f.as_symbol() == Some("hide"))
                    ).unwrap_or(false)
                );
                properties.insert(key.to_string(), SymbolProperty {
                    value: val.to_string(),
                    at: prop_at,
                    hidden,
                });
            }
        }
    }

    // Compute absolute pin positions if we can find the matching
    // library symbol. The transform is (rotate by `at.2` degrees,
    // then translate by (at.0, at.1)).
    let pin_positions = lib_symbols.iter()
        .find(|ls| ls.lib_id == lib_id)
        .map(|ls| {
            ls.pins.iter()
                .filter(|p| p.unit_index == 0 || p.unit_index == unit)
                .map(|p| {
                    let (px, py) = transform_point((p.at.0, p.at.1), at);
                    PinPosition {
                        pin_number: p.number.clone(),
                        pin_name: p.name.clone(),
                        electrical_type: p.electrical_type,
                        at: (px, py),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    Some(SchematicSymbol {
        lib_id, uuid, at, mirror, unit,
        properties, pin_positions,
        in_bom, on_board, dnp,
    })
}

/// Apply the symbol's at-transform to a pin's relative position.
/// at.2 is the symbol's rotation in degrees (KiCad uses 0/90/180/270).
fn transform_point(rel: (f64, f64), at: (f64, f64, f64)) -> (f64, f64) {
    let (rx, ry) = rel;
    let theta = at.2.to_radians();
    let cos = theta.cos();
    let sin = theta.sin();
    let nx = rx * cos - ry * sin;
    let ny = rx * sin + ry * cos;
    (at.0 + nx, at.1 + ny)
}

fn parse_wire(args: &[Sexpr]) -> Option<Wire> {
    let pts = args.iter().find_map(|a| a.match_list("pts"))?;
    let mut points = Vec::new();
    for xy in pts {
        if let Some(xy_args) = xy.match_list("xy") {
            if xy_args.len() >= 2 {
                let x = xy_args[0].as_num()?;
                let y = xy_args[1].as_num()?;
                points.push((x, y));
            }
        }
    }
    if points.len() < 2 { return None; }
    let uuid = args.iter().find_map(|a| a.match_list("uuid"))
        .and_then(|x| x.first())
        .and_then(|s| s.as_symbol().or(s.as_str()))
        .unwrap_or("")
        .to_string();
    Some(Wire {
        start: points[0],
        end: points[points.len() - 1],
        uuid,
    })
}

fn parse_junction(args: &[Sexpr]) -> Option<Junction> {
    let at = args.iter().find_map(|a| a.match_list("at")).map(parse_at_2)?;
    let uuid = args.iter().find_map(|a| a.match_list("uuid"))
        .and_then(|x| x.first())
        .and_then(|s| s.as_symbol().or(s.as_str()))
        .unwrap_or("")
        .to_string();
    Some(Junction { at, uuid })
}

fn parse_no_connect(args: &[Sexpr]) -> Option<NoConnect> {
    let at = args.iter().find_map(|a| a.match_list("at")).map(parse_at_2)?;
    let uuid = args.iter().find_map(|a| a.match_list("uuid"))
        .and_then(|x| x.first())
        .and_then(|s| s.as_symbol().or(s.as_str()))
        .unwrap_or("")
        .to_string();
    Some(NoConnect { at, uuid })
}

/// Decode KiCad's net-label escape sequences. KiCad represents
/// characters that aren't allowed in identifiers using `{slash}`,
/// `{tilde}`, `{backslash}`, `{caret}`, `{newline}` markers in the
/// label source text. We turn them into BHDL-safe identifier
/// characters at read time so downstream phases see clean names.
fn unescape_kicad_label(raw: &str) -> String {
    raw.replace("{slash}", "_")
       .replace("{tilde}", "_")
       .replace("{backslash}", "_")
       .replace("{caret}", "_")
       .replace("{newline}", "_")
       .replace("{space}", "_")
}

fn parse_label(args: &[Sexpr]) -> Option<Label> {
    let text = args.first().and_then(|s| s.as_str())
        .map(unescape_kicad_label)?;
    let at = args.iter().find_map(|a| a.match_list("at")).map(parse_at_3)?;
    let uuid = args.iter().find_map(|a| a.match_list("uuid"))
        .and_then(|x| x.first())
        .and_then(|s| s.as_symbol().or(s.as_str()))
        .unwrap_or("")
        .to_string();
    Some(Label { text, at, uuid })
}

fn parse_global_label(args: &[Sexpr]) -> Option<GlobalLabel> {
    let text = args.first().and_then(|s| s.as_str())
        .map(unescape_kicad_label)?;
    let at = args.iter().find_map(|a| a.match_list("at")).map(parse_at_3)?;
    let shape = args.iter().find_map(|a| a.match_list("shape"))
        .and_then(|x| x.first())
        .and_then(|s| s.as_symbol())
        .map(parse_global_label_shape)
        .unwrap_or(GlobalLabelShape::Bidirectional);
    let uuid = args.iter().find_map(|a| a.match_list("uuid"))
        .and_then(|x| x.first())
        .and_then(|s| s.as_symbol().or(s.as_str()))
        .unwrap_or("")
        .to_string();
    Some(GlobalLabel { text, at, shape, uuid })
}

fn parse_hierarchical_label(args: &[Sexpr]) -> Option<HierarchicalLabel> {
    let text = args.first().and_then(|s| s.as_str())
        .map(unescape_kicad_label)?;
    let at = args.iter().find_map(|a| a.match_list("at")).map(parse_at_3)?;
    let shape = args.iter().find_map(|a| a.match_list("shape"))
        .and_then(|x| x.first())
        .and_then(|s| s.as_symbol())
        .map(parse_global_label_shape)
        .unwrap_or(GlobalLabelShape::Bidirectional);
    let uuid = args.iter().find_map(|a| a.match_list("uuid"))
        .and_then(|x| x.first())
        .and_then(|s| s.as_symbol().or(s.as_str()))
        .unwrap_or("")
        .to_string();
    Some(HierarchicalLabel { text, at, shape, uuid })
}

fn parse_global_label_shape(s: &str) -> GlobalLabelShape {
    match s {
        "input"          => GlobalLabelShape::Input,
        "output"         => GlobalLabelShape::Output,
        "bidirectional" => GlobalLabelShape::Bidirectional,
        "tri_state"      => GlobalLabelShape::Tristate,
        "passive"        => GlobalLabelShape::Passive,
        _                => GlobalLabelShape::Bidirectional,
    }
}

fn parse_sheet_ref(args: &[Sexpr]) -> Option<SheetRef> {
    let at = args.iter().find_map(|a| a.match_list("at")).map(parse_at_2)?;
    let size = args.iter().find_map(|a| a.match_list("size"))
        .and_then(|x| Some((x.get(0)?.as_num()?, x.get(1)?.as_num()?)))?;
    let uuid = args.iter().find_map(|a| a.match_list("uuid"))
        .and_then(|x| x.first())
        .and_then(|s| s.as_symbol().or(s.as_str()))
        .unwrap_or("")
        .to_string();

    let mut name = String::new();
    let mut file_path = PathBuf::new();
    for a in args {
        if let Some(prop_args) = a.match_list("property") {
            if let (Some(key), Some(val)) = (
                prop_args.get(0).and_then(|s| s.as_str()),
                prop_args.get(1).and_then(|s| s.as_str()),
            ) {
                match key {
                    "Sheetname" => name = val.to_string(),
                    "Sheetfile" => file_path = PathBuf::from(val),
                    _ => {}
                }
            }
        }
    }
    if file_path.as_os_str().is_empty() { return None; }

    let mut pins = Vec::new();
    for a in args {
        if let Some(pin_args) = a.match_list("pin") {
            if let Some(pin) = parse_sheet_pin(pin_args) {
                pins.push(pin);
            }
        }
    }

    Some(SheetRef { file_path, name, at, size, pins, uuid })
}

fn parse_sheet_pin(args: &[Sexpr]) -> Option<SheetPin> {
    let name = args.first().and_then(|s| s.as_str())?.to_string();
    let shape = args.get(1).and_then(|s| s.as_symbol())
        .map(parse_global_label_shape)
        .unwrap_or(GlobalLabelShape::Bidirectional);
    let at = args.iter().find_map(|a| a.match_list("at")).map(parse_at_3)?;
    let uuid = args.iter().find_map(|a| a.match_list("uuid"))
        .and_then(|x| x.first())
        .and_then(|s| s.as_symbol().or(s.as_str()))
        .unwrap_or("")
        .to_string();
    Some(SheetPin { name, shape, at, uuid })
}

// ─── helpers ──────────────────────────────────────────────────────

fn parse_at_2(args: &[Sexpr]) -> (f64, f64) {
    let x = args.first().and_then(|s| s.as_num()).unwrap_or(0.0);
    let y = args.get(1).and_then(|s| s.as_num()).unwrap_or(0.0);
    (x, y)
}

fn parse_at_3(args: &[Sexpr]) -> (f64, f64, f64) {
    let x = args.first().and_then(|s| s.as_num()).unwrap_or(0.0);
    let y = args.get(1).and_then(|s| s.as_num()).unwrap_or(0.0);
    let r = args.get(2).and_then(|s| s.as_num()).unwrap_or(0.0);
    (x, y, r)
}

/// Read a `(<name> yes|no)` flag.
fn bool_field(args: &[Sexpr], name: &str) -> Option<bool> {
    args.iter().find_map(|a| a.match_list(name))
        .and_then(|x| x.first())
        .and_then(|s| s.as_symbol())
        .map(|s| s == "yes" || s == "true")
}

fn is_power_flag_lib_id(lib_id: &str) -> bool {
    lib_id.starts_with("power:") || lib_id.starts_with("Power:")
}

fn classify_power_label(label: &str) -> (PowerCategory, Option<f64>) {
    let upper = label.to_uppercase();
    if upper.contains("GND") || upper == "0V" || upper == "VSS" {
        return (PowerCategory::Ground, None);
    }
    // Try to parse a voltage out of common forms:
    //   "+5V", "+3V3", "+12V", "+1V8", "VCC_3V3", "VBUS"
    if let Some(v) = parse_voltage_from_label(&upper) {
        return (PowerCategory::Power, Some(v));
    }
    if upper.starts_with('V') || upper.starts_with('+') {
        return (PowerCategory::Power, None);
    }
    (PowerCategory::Other, None)
}

/// Extract a voltage value from common power-label spellings.
/// `+5V` → 5.0, `+3V3` → 3.3, `+1V8` → 1.8, `+12V` → 12.0.
fn parse_voltage_from_label(label: &str) -> Option<f64> {
    // Strip leading `+`, `VCC_`, `VDD_`, etc.
    let stripped = label.trim_start_matches('+').trim_start_matches("VCC_").trim_start_matches("VDD_");

    // Pattern: digits, optional 'V' with decimal-after-V form
    // (`3V3` → 3.3), or digits then 'V' (`5V` → 5).
    let bytes = stripped.as_bytes();
    let mut int_part = String::new();
    let mut frac_part = String::new();
    let mut seen_v = false;
    for &b in bytes {
        let c = b as char;
        if c.is_ascii_digit() && !seen_v {
            int_part.push(c);
        } else if c == 'V' && !seen_v {
            seen_v = true;
        } else if c.is_ascii_digit() && seen_v {
            frac_part.push(c);
        } else if seen_v {
            break;
        } else {
            break;
        }
    }
    if !seen_v || int_part.is_empty() { return None; }
    let v: f64 = format!("{}.{}", int_part, if frac_part.is_empty() { "0" } else { &frac_part })
        .parse().ok()?;
    Some(v)
}
