//! Catalog scan (Phase 4e).
//!
//! Glues together the matching engine (4c) and template renderer
//! (4d) into a driver that produces the candidate-MPN bundle the
//! Phase 5 plugin will consume.
//!
//! Inputs:
//!   * The analyzer's scope registry — provides the universe of
//!     `part_family` declarations.
//!   * A list of [`InstanceClass`] entries — refdes plus the
//!     monomorphised entity-and-generics tuple for each instance.
//!     For now callers (tests, Phase-F integration) construct
//!     these by hand; Phase 5 will derive them from the mono
//!     pass's output.
//!
//! Output: a [`CandidateBundle`] grouped by class identity, with
//! a `serde_json::to_string_pretty` representation matching the
//! schema in spec §7.3.

use std::collections::BTreeMap;

use bhdl_ast::{Item, PartFamilyDef, SourceFile};
use bhdl_common::ConstValue;
use rowan::ast::AstNode;
use serde::Serialize;

use crate::part_family::{
    match_class, parse_require_clause, render_template, ClassInstance,
};

// ─────────────────────────────────────────────────────────────────
// Inputs
// ─────────────────────────────────────────────────────────────────

/// One instance in the synthesized design: its refdes plus the
/// monomorphised class. Multiple instances may share the same
/// class — the catalog scan groups them.
#[derive(Debug, Clone)]
pub struct InstanceClass {
    pub refdes: String,
    pub class: ClassInstance,
}

// ─────────────────────────────────────────────────────────────────
// Outputs (mirror spec §7.3 JSON shape)
// ─────────────────────────────────────────────────────────────────

/// The full JSON candidate bundle for one board. Serializes to the
/// schema documented in §7.3 of
/// `docs/spec/Parameterization_And_BOM_Resolution.md`.
#[derive(Debug, Serialize)]
pub struct CandidateBundle {
    pub bhdl_version: String,
    pub protocol_version: String,
    pub board: String,
    pub selections_needed: Vec<ClassSelection>,
}

#[derive(Debug, Serialize)]
pub struct ClassSelection {
    pub class: String,
    pub generics: BTreeMap<String, String>,
    pub instance_count: usize,
    pub instances: Vec<String>,
    pub candidates: Vec<Candidate>,
    /// Diagnostics emitted while gathering candidates for this
    /// class (template-render errors, missing helpers, etc.). The
    /// plugin sees these as `warnings` upstream; we surface them
    /// per-class so the user can correlate.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Candidate {
    pub family: String,
    pub mpn: String,
    pub manufacturer: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, String>,
}

// ─────────────────────────────────────────────────────────────────
// Driver
// ─────────────────────────────────────────────────────────────────

/// Walk every `part_family` declaration in `catalog_files`, gather
/// candidates for each class in `instances`, and produce a fully
/// formed [`CandidateBundle`].
///
/// `board` is the user-facing board name that the JSON's top-level
/// `board` field carries.
pub fn run_catalog_scan(
    board: &str,
    instances: &[InstanceClass],
    catalog_files: &[(SourceFile, String)],
) -> CandidateBundle {
    // Step 1: harvest `part_family` declarations from the catalog
    // source files. Each entry is (def, name, declared-entity).
    let families: Vec<HarvestedFamily> = catalog_files
        .iter()
        .flat_map(|(sf, _path)| {
            sf.items().filter_map(|item| {
                if let Item::PartFamilyDef(pf) = item {
                    HarvestedFamily::from_def(&pf)
                } else {
                    None
                }
            })
        })
        .collect();

    // Step 2: group instances by class identity.
    // Class identity is `(entity, generics)` — using a string key
    // for ordering / dedup.
    let mut by_class: BTreeMap<String, Vec<&InstanceClass>> = BTreeMap::new();
    for inst in instances {
        let key = class_key(&inst.class);
        by_class.entry(key).or_default().push(inst);
    }

    // Step 3: for each class, find candidate families, render
    // their MPN templates, and build the JSON entry.
    let mut selections = Vec::new();
    for (_key, insts) in by_class {
        let example = &insts[0].class;
        let mut candidates = Vec::new();
        let mut warnings = Vec::new();

        for fam in &families {
            // Class pattern entity name filter — cheap reject.
            if fam.entity_name.as_deref() != Some(example.entity.as_str()) {
                continue;
            }

            // Re-parse the require clauses every time. They're
            // small; the overhead is dominated by the AST walk.
            let constraints: Vec<_> = fam
                .require_clauses
                .iter()
                .filter_map(|c| parse_require_clause(c).ok())
                .collect();

            // Run the matcher.
            let m = match match_class(
                &fam.name,
                &fam.class_pattern,
                &constraints,
                example,
            ) {
                Some(m) => m,
                None => continue,
            };

            // Render the MPN template (or use the literal `mpn`).
            let mpn = if let Some(t) = &fam.mpn_template {
                match render_template(t, &m.bindings) {
                    Ok(s) => s,
                    Err(e) => {
                        warnings.push(format!(
                            "skipped family `{}`: template render failed: {}",
                            fam.name, e.message
                        ));
                        continue;
                    }
                }
            } else if let Some(literal) = &fam.mpn_literal {
                literal.clone()
            } else {
                warnings.push(format!(
                    "skipped family `{}`: no mpn or mpn_template attribute",
                    fam.name
                ));
                continue;
            };

            // Copy a handful of useful attributes into the
            // candidate's serialized form (plugin-side context).
            let mut attrs: BTreeMap<String, String> = BTreeMap::new();
            for (k, v) in &fam.misc_attrs {
                attrs.insert(k.clone(), v.clone());
            }

            candidates.push(Candidate {
                family: fam.name.clone(),
                mpn,
                manufacturer: fam.manufacturer.clone(),
                attributes: attrs,
            });
        }

        let generics = bindings_for_class(example);
        selections.push(ClassSelection {
            class: example.entity.clone(),
            generics,
            instance_count: insts.len(),
            instances: insts.iter().map(|i| i.refdes.clone()).collect(),
            candidates,
            warnings,
        });
    }

    CandidateBundle {
        bhdl_version: env!("CARGO_PKG_VERSION").to_string(),
        protocol_version: "1".to_string(),
        board: board.to_string(),
        selections_needed: selections,
    }
}

/// Pretty-print a `CandidateBundle` to the canonical JSON shape.
pub fn bundle_to_json(bundle: &CandidateBundle) -> String {
    serde_json::to_string_pretty(bundle).expect("CandidateBundle is always serializable")
}

// ─────────────────────────────────────────────────────────────────
// Internals
// ─────────────────────────────────────────────────────────────────

/// Pre-extracted view of a [`PartFamilyDef`] suitable for repeated
/// matching against many class instances. Walks the AST once at
/// scan startup; subsequent matches don't re-read the file.
struct HarvestedFamily {
    name: String,
    entity_name: Option<String>,
    class_pattern: bhdl_ast::ClassPattern,
    require_clauses: Vec<bhdl_ast::RequireClause>,
    mpn_literal: Option<String>,
    mpn_template: Option<String>,
    manufacturer: Option<String>,
    misc_attrs: Vec<(String, String)>,
}

impl HarvestedFamily {
    fn from_def(pf: &PartFamilyDef) -> Option<Self> {
        use bhdl_ast::HasName;
        let name = pf.name()?.text().to_string();
        let pat = pf.class_pattern()?;
        let entity_name = pat.entity_name();
        let require_clauses: Vec<_> = pf.require_clauses().collect();

        // Walk attribute declarations under the part_family node.
        let mut mpn_literal = None;
        let mut mpn_template = None;
        let mut manufacturer = None;
        let mut misc_attrs = Vec::new();

        for child in pf.syntax().children() {
            if child.kind() == bhdl_parser::SyntaxKind::ATTRIBUTE_DECL {
                if let Some((k, v)) = parse_attribute_kv(&child) {
                    match k.as_str() {
                        "mpn" => mpn_literal = Some(v),
                        "mpn_template" => mpn_template = Some(v),
                        "manufacturer" => manufacturer = Some(v.clone()),
                        _ => {}
                    }
                    // Always also keep in misc_attrs so the plugin
                    // sees the same data the user wrote.
                    if let Some((k2, v2)) = parse_attribute_kv(&child) {
                        if k2 != "mpn" && k2 != "mpn_template" {
                            misc_attrs.push((k2, v2));
                        }
                    }
                }
            }
        }

        Some(HarvestedFamily {
            name,
            entity_name,
            class_pattern: pat,
            require_clauses,
            mpn_literal,
            mpn_template,
            manufacturer,
            misc_attrs,
        })
    }
}

/// Extract `(name, value)` from an ATTRIBUTE_DECL node by walking
/// its text. Quick-and-dirty for v0.2 — formal AST-based access
/// can come later.
fn parse_attribute_kv(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>) -> Option<(String, String)> {
    let text = node.text().to_string();
    // Skip leading "attribute " keyword.
    let s = text.trim_start_matches("attribute").trim();
    let eq = s.find('=')?;
    let name = s[..eq].trim().to_string();
    let mut value = s[eq + 1..].trim().to_string();
    // Strip trailing semicolon.
    if let Some(stripped) = value.strip_suffix(';') {
        value = stripped.trim().to_string();
    }
    // Unquote a string literal.
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        value = value[1..value.len() - 1].to_string();
    }
    Some((name, value))
}

/// Stable string key for grouping instances by class.
fn class_key(c: &ClassInstance) -> String {
    let mut s = String::new();
    s.push_str(&c.entity);
    s.push('<');
    for (i, v) in c.generics.iter().enumerate() {
        if i > 0 { s.push(','); }
        s.push_str(&value_key(v));
    }
    s.push('>');
    s
}

fn value_key(v: &ConstValue) -> String {
    match v {
        ConstValue::String(s) => format!("\"{}\"", s),
        ConstValue::Integer(i) => i.to_string(),
        ConstValue::Float(f) => f.to_string(),
        ConstValue::Bool(b) => b.to_string(),
        ConstValue::Voltage(x) => format!("{}V", x),
        ConstValue::Current(x) => format!("{}A", x),
        ConstValue::Resistance(x) => format!("{}Ω", x),
        ConstValue::Capacitance(x) => format!("{}F", x),
        ConstValue::Inductance(x) => format!("{}H", x),
        ConstValue::Power(x) => format!("{}W", x),
        ConstValue::Frequency(x) => format!("{}Hz", x),
        ConstValue::Time(x) => format!("{}s", x),
    }
}

/// Synthesize a "generics" map for the JSON output. Uses generic-
/// param *positions* (G0, G1, ...) because the catalog scan
/// doesn't (yet) know the entity-side parameter names. Phase 5
/// will join with the entity AST to substitute real names.
fn bindings_for_class(c: &ClassInstance) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (i, v) in c.generics.iter().enumerate() {
        out.insert(format!("G{}", i), value_key(v));
    }
    out
}

// ─────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_parser::parse;
    use std::fs;

    fn load_catalog_file(path: &str) -> (SourceFile, String) {
        let content = fs::read_to_string(path).expect("read");
        let pr = parse(&content);
        assert!(pr.errors().is_empty(), "parse errors in {}: {:?}", path, pr.errors());
        let sf = SourceFile::cast(pr.syntax()).expect("source file");
        (sf, path.to_string())
    }

    #[test]
    fn yageo_10k_resistor() {
        let catalog = vec![load_catalog_file("../bhdl-stdlib/parts/yageo/rc0603fr.bhdl")];
        let instances = vec![InstanceClass {
            refdes: "R1".to_string(),
            class: ClassInstance {
                entity: "Resistor".to_string(),
                generics: vec![
                    ConstValue::Resistance(10_000.0),
                    ConstValue::String("1%".to_string()),
                    ConstValue::String("0603".to_string()),
                ],
            },
        }];
        let bundle = run_catalog_scan("test_board", &instances, &catalog);
        assert_eq!(bundle.board, "test_board");
        assert_eq!(bundle.selections_needed.len(), 1);

        let sel = &bundle.selections_needed[0];
        assert_eq!(sel.class, "Resistor");
        assert_eq!(sel.instance_count, 1);
        assert_eq!(sel.instances, vec!["R1"]);
        assert_eq!(sel.candidates.len(), 1);

        let cand = &sel.candidates[0];
        assert_eq!(cand.family, "Yageo_RC0603FR_07");
        assert_eq!(cand.mpn, "RC0603FR-071002L");
        assert_eq!(cand.manufacturer.as_deref(), Some("Yageo"));
    }

    #[test]
    fn multi_vendor_resistor() {
        let catalog = vec![
            load_catalog_file("../bhdl-stdlib/parts/yageo/rc0603fr.bhdl"),
            load_catalog_file("../bhdl-stdlib/parts/panasonic/erj_3ek.bhdl"),
            load_catalog_file("../bhdl-stdlib/parts/avx/cr0603_fx.bhdl"),
        ];
        let instances = vec![InstanceClass {
            refdes: "R1".to_string(),
            class: ClassInstance {
                entity: "Resistor".to_string(),
                generics: vec![
                    ConstValue::Resistance(10_000.0),
                    ConstValue::String("1%".to_string()),
                    ConstValue::String("0603".to_string()),
                ],
            },
        }];
        let bundle = run_catalog_scan("multi_vendor", &instances, &catalog);
        let sel = &bundle.selections_needed[0];
        // All three vendors should appear as candidates.
        assert_eq!(sel.candidates.len(), 3);
        let mpns: Vec<&str> = sel.candidates.iter().map(|c| c.mpn.as_str()).collect();
        assert!(mpns.iter().any(|m| m.starts_with("RC0603FR-07")));
        assert!(mpns.iter().any(|m| m.starts_with("ERJ-3EKF")));
        assert!(mpns.iter().any(|m| m.starts_with("CR0603-FX-")));
    }

    #[test]
    fn instance_grouping() {
        // Four instances all sharing the same class — should collapse
        // to a single selection_needed entry with instance_count=4.
        let catalog = vec![load_catalog_file("../bhdl-stdlib/parts/yageo/rc0603fr.bhdl")];
        let class = ClassInstance {
            entity: "Resistor".to_string(),
            generics: vec![
                ConstValue::Resistance(10_000.0),
                ConstValue::String("1%".to_string()),
                ConstValue::String("0603".to_string()),
            ],
        };
        let instances = vec![
            InstanceClass { refdes: "R1".to_string(),  class: class.clone() },
            InstanceClass { refdes: "R5".to_string(),  class: class.clone() },
            InstanceClass { refdes: "R12".to_string(), class: class.clone() },
            InstanceClass { refdes: "R23".to_string(), class: class.clone() },
        ];
        let bundle = run_catalog_scan("test", &instances, &catalog);
        assert_eq!(bundle.selections_needed.len(), 1);
        assert_eq!(bundle.selections_needed[0].instance_count, 4);
        let refs = &bundle.selections_needed[0].instances;
        assert_eq!(refs.len(), 4);
        assert!(refs.contains(&"R1".to_string()));
        assert!(refs.contains(&"R23".to_string()));
    }

    #[test]
    fn lm317_family_of_one() {
        let catalog = vec![load_catalog_file("../bhdl-stdlib/parts/ti/lm317.bhdl")];
        let instances = vec![InstanceClass {
            refdes: "U2".to_string(),
            class: ClassInstance {
                entity: "LM317".to_string(),
                generics: vec![],
            },
        }];
        let bundle = run_catalog_scan("test", &instances, &catalog);
        assert_eq!(bundle.selections_needed.len(), 1);
        let cand = &bundle.selections_needed[0].candidates[0];
        assert_eq!(cand.mpn, "LM317T");
    }

    #[test]
    fn ap2112k_voltage_template() {
        let catalog = vec![load_catalog_file("../bhdl-stdlib/parts/diodes/ap2112k.bhdl")];
        let instances = vec![InstanceClass {
            refdes: "U1".to_string(),
            class: ClassInstance {
                entity: "AP2112K".to_string(),
                generics: vec![ConstValue::Voltage(3.3)],
            },
        }];
        let bundle = run_catalog_scan("test", &instances, &catalog);
        let cand = &bundle.selections_needed[0].candidates[0];
        assert_eq!(cand.mpn, "AP2112K-3.3TRG");
    }

    #[test]
    fn json_shape_matches_spec() {
        // Smoke-test: bundle serializes and the JSON keys match the
        // spec's §7.3 schema. Doesn't validate values — that's
        // covered by the other tests.
        let catalog = vec![load_catalog_file("../bhdl-stdlib/parts/ti/lm317.bhdl")];
        let instances = vec![InstanceClass {
            refdes: "U2".to_string(),
            class: ClassInstance { entity: "LM317".to_string(), generics: vec![] },
        }];
        let bundle = run_catalog_scan("test", &instances, &catalog);
        let json = bundle_to_json(&bundle);
        assert!(json.contains("\"bhdl_version\""));
        assert!(json.contains("\"protocol_version\": \"1\""));
        assert!(json.contains("\"board\": \"test\""));
        assert!(json.contains("\"selections_needed\""));
        assert!(json.contains("\"class\": \"LM317\""));
        assert!(json.contains("\"mpn\": \"LM317T\""));
    }

    #[test]
    fn no_candidates_when_unmatched() {
        let catalog = vec![load_catalog_file("../bhdl-stdlib/parts/yageo/rc0603fr.bhdl")];
        // Out-of-range value → no candidate from Yageo's E96(1Ω, 10MΩ).
        let instances = vec![InstanceClass {
            refdes: "R99".to_string(),
            class: ClassInstance {
                entity: "Resistor".to_string(),
                generics: vec![
                    ConstValue::Resistance(0.1),    // < 1Ω
                    ConstValue::String("1%".to_string()),
                    ConstValue::String("0603".to_string()),
                ],
            },
        }];
        let bundle = run_catalog_scan("test", &instances, &catalog);
        assert_eq!(bundle.selections_needed.len(), 1);
        assert!(bundle.selections_needed[0].candidates.is_empty());
    }
}
