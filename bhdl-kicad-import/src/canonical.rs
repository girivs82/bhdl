//! Canonical netlist + equivalence checker.
//!
//! Phase E of the KiCad-to-BHDL translator (plan §5.5). The
//! invariant the importer must preserve is **the netlist**:
//! whatever BHDL we emit, when run through bhdl-synthesizer it
//! must produce the same bag of `(net_name, [(ref, pin)])`
//! tuples as the KiCad source.
//!
//! This module defines the canonical netlist shape and the two
//! producers/consumers:
//!
//! - [`canonical_from_schematic`] — flatten a parsed [`Schematic`]
//!   (root + all child sheets) into a single canonical netlist
//!   keyed by stable net names. Power nets are merged across
//!   sheets (KiCad treats power flags as implicit globals).
//!   Hierarchical labels are merged with the corresponding
//!   parent sheet-pin nets.
//! - [`parse_kicad_net_file`] — parse a `.net` file exported by
//!   KiCad's Tools → "Generate Netlist". This is KiCad's
//!   authoritative netlist; comparing our extracted canonical
//!   form against it proves the importer's net topology matches
//!   what KiCad's own ERC sees.
//! - [`compare`] — produce a structured [`EquivalenceReport`]
//!   describing additions, removals, and pin-set differences
//!   between two canonical netlists.
//!
//! BHDL-side extraction (running the synthesizer on the emitted
//! `.bhdl` and reading back its canonical netlist) lives in the
//! cross-crate Phase F integration; this module only defines the
//! shape and the comparator that Phase F will call.

use std::collections::{BTreeMap, BTreeSet};

use crate::ir::Schematic;
use crate::nets::extract_nets;

/// A single pin-connection in the canonical netlist. Order-stable
/// (sorted by ref, then pin number) so two canonical netlists
/// over the same circuit are byte-identical.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PinRef {
    /// Reference designator: `R1`, `U7`, `C12`, …
    pub reference: String,
    /// Pin number as a string (KiCad pin numbers can be `"PAD1"`
    /// or `"A12"` for BGAs — string is the only safe type).
    pub pin: String,
}

/// The canonical netlist: each net name maps to its sorted set of
/// pin references. `BTreeMap` and `BTreeSet` give deterministic
/// iteration, which makes byte-level diffs trivial.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CanonicalNetlist {
    pub nets: BTreeMap<String, BTreeSet<PinRef>>,
}

impl CanonicalNetlist {
    pub fn new() -> Self { Self::default() }

    /// Number of nets.
    pub fn len(&self) -> usize { self.nets.len() }
    pub fn is_empty(&self) -> bool { self.nets.is_empty() }

    /// Total pin count summed across all nets.
    pub fn pin_count(&self) -> usize {
        self.nets.values().map(|s| s.len()).sum()
    }

    /// Add one pin to one net. Creates the net if absent.
    pub fn add(&mut self, net: impl Into<String>, pin: PinRef) {
        self.nets.entry(net.into()).or_default().insert(pin);
    }
}

// ─────────────────────────────────────────────────────────────────
// KiCad-side extraction
// ─────────────────────────────────────────────────────────────────

/// Flatten a parsed [`Schematic`] into a canonical netlist. Power
/// nets are merged across sheets (KiCad treats power flags as
/// implicit globals). Auto-generated `Net_N` names get a stable
/// prefix per sheet so collisions across sheets don't merge
/// unrelated nets.
pub fn canonical_from_schematic(sch: &Schematic) -> CanonicalNetlist {
    let mut out = CanonicalNetlist::new();

    // Per-sheet: extract nets, then emit (canonical_name, pinrefs)
    // into the global netlist. Power nets keep their canonical
    // name (no prefix). Local/auto names get a sheet prefix so
    // identically-named local nets in different sheets stay
    // separate.
    add_sheet(&mut out, &sch.root, "");
    for (path, sheet) in &sch.child_sheets {
        let prefix = format!("/{}/", path.file_stem()
            .and_then(|s| s.to_str()).unwrap_or("subsheet"));
        add_sheet(&mut out, sheet, &prefix);
    }
    out
}

fn add_sheet(out: &mut CanonicalNetlist, sheet: &crate::ir::Sheet, prefix: &str) {
    let nets = extract_nets(sheet);
    // Build symbol_uuid → reference lookup so PinRefs carry the
    // designator, not the UUID.
    let mut ref_of: BTreeMap<&str, String> = BTreeMap::new();
    for sym in &sheet.symbols {
        if let Some(r) = sym.reference() {
            ref_of.insert(sym.uuid.as_str(), r.to_string());
        }
    }
    for net in nets.nets {
        let name = if net.is_power || prefix.is_empty() {
            net.name.clone()
        } else {
            format!("{}{}", prefix, net.name)
        };
        for pin in net.pins {
            let reference = ref_of.get(pin.symbol_uuid.as_str())
                .cloned()
                .unwrap_or_else(|| format!("_uuid:{}", pin.symbol_uuid));
            out.add(name.clone(), PinRef {
                reference,
                pin: pin.pin_number,
            });
        }
    }
}

// ─────────────────────────────────────────────────────────────────
// KiCad .net file parser
// ─────────────────────────────────────────────────────────────────

/// Parse the netlist file KiCad exports via Tools → Generate
/// Netlist. The file is an S-expression with the shape:
///
/// ```text
/// (export
///   (version "E")
///   (components ...)
///   (libparts ...)
///   (libraries ...)
///   (nets
///     (net (code 1) (name "/MID")
///       (node (ref "R1") (pin "2") ...)
///       (node (ref "C1") (pin "1") ...))
///     ...))
/// ```
///
/// We extract only the `(nets ...)` section.
pub fn parse_kicad_net_file(src: &str) -> Result<CanonicalNetlist, crate::sexpr::ParseError> {
    let sexpr = crate::sexpr::parse(src)?;
    let mut out = CanonicalNetlist::new();
    // Top must be (export ...)
    let Some(export_items) = sexpr.match_list("export") else {
        return Ok(out); // not an export file — empty result
    };
    // Find the (nets ...) sub-list.
    for item in export_items {
        if let Some(net_items) = item.match_list("nets") {
            for n in net_items {
                let Some(fields) = n.match_list("net") else { continue; };
                let mut name = String::new();
                let mut pins: BTreeSet<PinRef> = BTreeSet::new();
                for f in fields {
                    if let Some(args) = f.match_list("name") {
                        if let Some(first) = args.first() {
                            name = first.as_str().or(first.as_symbol())
                                .unwrap_or("").to_string();
                        }
                    } else if let Some(args) = f.match_list("node") {
                        let mut r = String::new();
                        let mut p = String::new();
                        for sub in args {
                            if let Some(a2) = sub.match_list("ref") {
                                if let Some(v) = a2.first() {
                                    r = v.as_str().or(v.as_symbol()).unwrap_or("").to_string();
                                }
                            } else if let Some(a2) = sub.match_list("pin") {
                                if let Some(v) = a2.first() {
                                    p = v.as_str().or(v.as_symbol()).unwrap_or("").to_string();
                                }
                            }
                        }
                        if !r.is_empty() {
                            pins.insert(PinRef { reference: r, pin: p });
                        }
                    }
                }
                if !name.is_empty() {
                    out.nets.insert(canonicalise_kicad_net_name(&name), pins);
                }
            }
        }
    }
    Ok(out)
}

/// KiCad's exported net names carry a `/Sheet/` path prefix and
/// quote signal labels verbatim (`+5V`, `GND`). Normalise to the
/// same form `canonical_from_schematic` emits so the diff is
/// meaningful.
fn canonicalise_kicad_net_name(raw: &str) -> String {
    // Drop a leading `/` (KiCad prefixes top-level nets with it).
    let s = raw.trim_start_matches('/');
    match s {
        "GND" | "Earth" => "GND".to_string(),
        "+5V" => "VCC_5V".to_string(),
        "+3V3" | "+3.3V" => "VCC_3V3".to_string(),
        "+12V" => "VCC_12V".to_string(),
        "-12V" => "VEE_12V".to_string(),
        other => other.to_string(),
    }
}

// ─────────────────────────────────────────────────────────────────
// Equivalence comparison
// ─────────────────────────────────────────────────────────────────

/// Per-net difference between two canonical netlists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetDiff {
    /// Net present in `a` but missing in `b`.
    OnlyInA { net: String, pins: BTreeSet<PinRef> },
    /// Net present in `b` but missing in `a`.
    OnlyInB { net: String, pins: BTreeSet<PinRef> },
    /// Net exists in both but pin set differs.
    PinSetDiffers {
        net: String,
        only_in_a: BTreeSet<PinRef>,
        only_in_b: BTreeSet<PinRef>,
    },
}

/// Result of comparing two canonical netlists. `is_equivalent()`
/// is the headline boolean; `diffs` carries the structured detail.
#[derive(Debug, Clone, Default)]
pub struct EquivalenceReport {
    pub diffs: Vec<NetDiff>,
    /// Pin counts of each side, for at-a-glance sanity.
    pub pin_count_a: usize,
    pub pin_count_b: usize,
    pub net_count_a: usize,
    pub net_count_b: usize,
}

impl EquivalenceReport {
    pub fn is_equivalent(&self) -> bool { self.diffs.is_empty() }

    /// One-line summary suitable for CI logs.
    pub fn summary(&self) -> String {
        if self.is_equivalent() {
            format!("✓ equivalent: {} nets, {} pins",
                self.net_count_a, self.pin_count_a)
        } else {
            format!("✗ NOT equivalent: {} differences (A: {} nets / {} pins; B: {} nets / {} pins)",
                self.diffs.len(),
                self.net_count_a, self.pin_count_a,
                self.net_count_b, self.pin_count_b)
        }
    }
}

/// Compare two canonical netlists. The result lists every
/// structural difference; an empty `diffs` means the two are
/// equivalent at the netlist level (modulo net naming, since net
/// names ARE compared — but identical-by-name is the expected
/// behaviour given both sides flow from the same canonicaliser).
pub fn compare(a: &CanonicalNetlist, b: &CanonicalNetlist) -> EquivalenceReport {
    let mut report = EquivalenceReport {
        pin_count_a: a.pin_count(),
        pin_count_b: b.pin_count(),
        net_count_a: a.len(),
        net_count_b: b.len(),
        ..Default::default()
    };
    let all_nets: BTreeSet<&String> =
        a.nets.keys().chain(b.nets.keys()).collect();
    for net in all_nets {
        match (a.nets.get(net), b.nets.get(net)) {
            (Some(pa), None) => report.diffs.push(NetDiff::OnlyInA {
                net: net.clone(), pins: pa.clone(),
            }),
            (None, Some(pb)) => report.diffs.push(NetDiff::OnlyInB {
                net: net.clone(), pins: pb.clone(),
            }),
            (Some(pa), Some(pb)) if pa != pb => {
                let only_a: BTreeSet<PinRef> = pa.difference(pb).cloned().collect();
                let only_b: BTreeSet<PinRef> = pb.difference(pa).cloned().collect();
                report.diffs.push(NetDiff::PinSetDiffers {
                    net: net.clone(),
                    only_in_a: only_a,
                    only_in_b: only_b,
                });
            }
            _ => {}
        }
    }
    report
}

// ─────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read_from_str;
    use std::path::PathBuf;

    const RC_SCH: &str = r#"(kicad_sch
        (version 20231120) (generator eeschema)
        (lib_symbols
          (symbol "Device:R"
            (pin passive line (at 0 3.81 270) (length 1.27) (name "~") (number "1"))
            (pin passive line (at 0 -3.81 90) (length 1.27) (name "~") (number "2")))
          (symbol "Device:C"
            (pin passive line (at 0 3.81 270) (length 1.27) (name "~") (number "1"))
            (pin passive line (at 0 -3.81 90) (length 1.27) (name "~") (number "2"))))
        (symbol (lib_id "Device:R") (at 100 100 0) (unit 1) (in_bom yes) (on_board yes)
          (uuid "11111111-aaaa-bbbb-cccc-000000000001")
          (property "Reference" "R1" (at 0 0 0))
          (property "Value" "10k" (at 0 0 0)))
        (symbol (lib_id "Device:C") (at 100 110 0) (unit 1) (in_bom yes) (on_board yes)
          (uuid "11111111-aaaa-bbbb-cccc-000000000002")
          (property "Reference" "C1" (at 0 0 0))
          (property "Value" "100nF" (at 0 0 0)))
        (wire (pts (xy 100 96.19) (xy 100 113.81)) (uuid "w1"))
        (label "MID" (at 100 105 0) (uuid "l1")))
    "#;

    fn schematic_of(src: &str) -> Schematic {
        let sheet = read_from_str(src, PathBuf::from("t.kicad_sch")).expect("read");
        Schematic {
            root: sheet,
            child_sheets: std::collections::HashMap::new(),
            version: 20231120,
            generator: "test".into(),
        }
    }

    #[test]
    fn round_trip_canonical_is_self_equivalent() {
        let sch = schematic_of(RC_SCH);
        let n1 = canonical_from_schematic(&sch);
        let n2 = canonical_from_schematic(&sch);
        let rep = compare(&n1, &n2);
        assert!(rep.is_equivalent(), "{}\n{:#?}", rep.summary(), rep.diffs);
    }

    #[test]
    fn canonical_collects_expected_nets() {
        let sch = schematic_of(RC_SCH);
        let n = canonical_from_schematic(&sch);
        // Wire joins R1 and C1; label "MID" names the net.
        let mid = n.nets.get("MID").expect("MID net");
        assert_eq!(mid.len(), 2);
        let refs: BTreeSet<&str> = mid.iter().map(|p| p.reference.as_str()).collect();
        assert!(refs.contains("R1"));
        assert!(refs.contains("C1"));
    }

    #[test]
    fn diff_detects_missing_net() {
        let sch = schematic_of(RC_SCH);
        let mut a = canonical_from_schematic(&sch);
        let mut b = a.clone();
        b.nets.remove("MID");
        let rep = compare(&a, &b);
        assert!(!rep.is_equivalent());
        assert!(matches!(rep.diffs[0], NetDiff::OnlyInA { ref net, .. } if net == "MID"));

        // And the symmetric case.
        a.nets.remove("MID");
        a.nets.insert("MID".into(), Default::default());
        a.nets.get_mut("MID").unwrap().insert(PinRef { reference: "R99".into(), pin: "1".into() });
        let rep2 = compare(&a, &b);
        assert!(!rep2.is_equivalent());
    }

    #[test]
    fn diff_detects_pin_set_change() {
        let sch = schematic_of(RC_SCH);
        let a = canonical_from_schematic(&sch);
        let mut b = a.clone();
        // Swap a pin on the MID net.
        let mid = b.nets.get_mut("MID").unwrap();
        mid.insert(PinRef { reference: "R99".into(), pin: "1".into() });
        let rep = compare(&a, &b);
        assert_eq!(rep.diffs.len(), 1);
        match &rep.diffs[0] {
            NetDiff::PinSetDiffers { net, only_in_b, .. } => {
                assert_eq!(net, "MID");
                assert!(only_in_b.iter().any(|p| p.reference == "R99"));
            }
            other => panic!("expected PinSetDiffers, got {:?}", other),
        }
    }

    const KICAD_NET_EXPORT: &str = r#"
        (export (version "E")
          (nets
            (net (code 1) (name "/MID")
              (node (ref "R1") (pin "2"))
              (node (ref "C1") (pin "1")))
            (net (code 2) (name "+5V")
              (node (ref "R1") (pin "1")))
            (net (code 3) (name "GND")
              (node (ref "C1") (pin "2")))))
    "#;

    #[test]
    fn parses_kicad_net_file() {
        let n = parse_kicad_net_file(KICAD_NET_EXPORT).expect("parse");
        assert_eq!(n.len(), 3);
        assert!(n.nets.contains_key("MID"));
        // KiCad's "+5V" is canonicalised to VCC_5V.
        assert!(n.nets.contains_key("VCC_5V"));
        assert!(n.nets.contains_key("GND"));
        let mid = &n.nets["MID"];
        assert_eq!(mid.len(), 2);
    }

    #[test]
    fn empty_compare_is_equivalent() {
        let a = CanonicalNetlist::new();
        let b = CanonicalNetlist::new();
        let rep = compare(&a, &b);
        assert!(rep.is_equivalent());
        assert_eq!(rep.summary(), "✓ equivalent: 0 nets, 0 pins");
    }
}
