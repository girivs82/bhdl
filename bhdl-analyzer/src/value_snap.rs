//! Catalog-authoritative E-series value snapping (sizing pipeline stage 3,
//! live-pipeline form).
//!
//! A design block emits an *honest computed* passive value (e.g. a buck FB
//! resistor at 31250Ω, a ripple-floor output cap at 4.39µF). Those are not
//! orderable parts. This pass rewrites each passive instance's `value`
//! attribute to the nearest standard value of the E-series the matching
//! `part_family` catalog entry declares (`require R in E96(1Ω, 10MΩ)`), so
//! the value SPICE simulates and the value the BOM names are one and the
//! same real, orderable number (31250Ω → 31.6kΩ).
//!
//! The E-series is taken from the catalog declarations, never hardcoded
//! per component type — the catalog is the single source of truth. The
//! caller supplies the harvested [`FamilyDecl`]s (discovered through the
//! library-resolution system); this module is the decision-free core:
//! harvest → match → snap → write back.
//!
//! Pipeline placement: runs post-expansion, before SPICE conversion and
//! BOM emission (both read the same `value` attribute). Snapping here is
//! stage 3 of seed → simulate → snap → simulate → margin → simulate; the
//! snapped value is what the downstream validation sim sees.

use std::collections::HashMap;

use bhdl_ast::{AstNode, Item, PartFamilyDef, SourceFile};
use bhdl_netlist::Netlist;

use crate::part_family::{parse_require_clause, Constraint, ESeries};

/// One catalog family's snapping-relevant facts: the normalized component
/// class it applies to and the E-series ranges it declares. (MPN/template
/// data is irrelevant to value snapping and is dropped.)
#[derive(Debug, Clone)]
pub struct FamilyDecl {
    /// Normalized component class — `"resistor"`, `"capacitor"`,
    /// `"inductor"` (see [`normalize_class`]).
    pub class: String,
    /// `(series, min, max)` for each `require axis in E_N(min, max)`
    /// clause, with min/max already lowered to SI base units.
    pub series_ranges: Vec<(ESeries, f64, f64)>,
}

/// Normalize the several spellings of a component class to one canonical
/// lowercase token. The catalog pattern says `Resistor`, the netlist
/// instance carries `component_class = "resistor"`, and the stdlib entity
/// is actually `Res` (with `alias Resistor = Res`) — all three must
/// collapse to the same key for matching.
pub fn normalize_class(s: &str) -> Option<&'static str> {
    match s.trim() {
        "Res" | "Resistor" | "resistor" => Some("resistor"),
        "Cap" | "Capacitor" | "capacitor" => Some("capacitor"),
        "Ind" | "Inductor" | "inductor" => Some("inductor"),
        _ => None,
    }
}

/// SI base unit symbol for a normalized class, used when formatting a
/// snapped value back to a `value` string.
fn dimension_unit(class: &str) -> &'static str {
    match class {
        "resistor" => "Ω",
        "capacitor" => "F",
        "inductor" => "H",
        _ => "",
    }
}

/// Parse a `value`-attribute string to its SI-base f64. Handles both the
/// bare-SI-prefix form instances carry (`"10k"`, `"100µF"`, `"4.7µH"`) and
/// the dimensioned form catalog ranges use (`"1Ω"`, `"10MΩ"`, `"100nF"`).
/// Returns `None` for a non-numeric / unrecognized string.
pub fn parse_value_string(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    if let Ok(n) = t.parse::<f64>() {
        return Some(n);
    }
    // Leading numeric run, then a unit/prefix suffix.
    let split = t.find(|c: char| {
        !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E')
    })?;
    let (num, suffix) = t.split_at(split);
    let n: f64 = num.parse().ok()?;
    let suffix = suffix.trim();
    // Prefer the full unit table (handles `kΩ`, `µF`, `nH`, …), then fall
    // back to a bare SI prefix (`k`, `µ`, `m`, `n`, …) for unit-less forms
    // like `"10k"`.
    if let Some((scale, _ctor)) = bhdl_common::const_value::parse_unit_suffix(suffix) {
        return Some(n * scale);
    }
    if let Some(scale) = bhdl_common::const_value::parse_si_prefix(suffix) {
        return Some(n * scale);
    }
    None
}

/// Format an SI-base value back into an engineering-notation `value`
/// string for the given class (`31600.0, "resistor"` → `"31.6kΩ"`,
/// `4.7e-6, "capacitor"` → `"4.7µF"`). Engineering notation (mantissa in
/// [1,1000), step-of-1000 prefixes) keeps the attribute readable and in
/// the same shape the rest of the toolchain already emits/parses.
pub fn format_value(v: f64, class: &str) -> String {
    let unit = dimension_unit(class);
    if v == 0.0 {
        return format!("0{unit}");
    }
    const PREFIXES: &[(&str, f64)] = &[
        ("p", 1e-12),
        ("n", 1e-9),
        ("µ", 1e-6),
        ("m", 1e-3),
        ("", 1.0),
        ("k", 1e3),
        ("M", 1e6),
        ("G", 1e9),
    ];
    // Pick the prefix that puts the mantissa in [1, 1000).
    let (prefix, scale) = PREFIXES
        .iter()
        .rev()
        .find(|(_, s)| v.abs() / s >= 1.0)
        .copied()
        .unwrap_or(("p", 1e-12));
    let mantissa = v / scale;
    // Trim trailing zeros: 31.6, 4.7, 10, 1.
    let mut m = format!("{mantissa:.3}");
    if m.contains('.') {
        m = m.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    format!("{m}{prefix}{unit}")
}

/// Harvest the snapping-relevant facts from a set of parsed catalog source
/// files. Each `part_family` whose class normalizes and which carries at
/// least one `in E_N(...)` clause becomes a [`FamilyDecl`].
pub fn harvest_families(sources: &[SourceFile]) -> Vec<FamilyDecl> {
    let mut out = Vec::new();
    for sf in sources {
        for item in sf.items() {
            let Item::PartFamilyDef(pf) = item else { continue };
            if let Some(decl) = harvest_one(&pf) {
                out.push(decl);
            }
        }
    }
    out
}

fn harvest_one(pf: &PartFamilyDef) -> Option<FamilyDecl> {
    let class = normalize_class(&pf.class_pattern()?.entity_name()?)?.to_string();
    let mut series_ranges = Vec::new();
    for clause in pf.require_clauses() {
        if let Ok(Constraint::InESeries { series, min, max, .. }) = parse_require_clause(&clause) {
            if let (Some(lo), Some(hi)) = (min.as_f64(), max.as_f64()) {
                series_ranges.push((series, lo, hi));
            }
        }
    }
    if series_ranges.is_empty() {
        return None;
    }
    Some(FamilyDecl { class, series_ranges })
}

/// The component class of a netlist instance: its `component_class`
/// attribute (instance, then backing module), normalized. `None` if it
/// isn't a recognized snappable passive.
fn instance_class(netlist: &Netlist, id: bhdl_netlist::InstanceId) -> Option<&'static str> {
    let inst = netlist.instances.get(id)?;
    if let Some(c) = inst.attributes.get("component_class").and_then(|s| normalize_class(s)) {
        return Some(c);
    }
    // Fall back to the backing module's attributes, then its name (`Res`).
    let module = netlist.modules.get(inst.definition)?;
    if let Some(c) = module.attributes.get("component_class").and_then(|s| normalize_class(s)) {
        return Some(c);
    }
    normalize_class(&module.name)
}

/// Snap every passive instance's `value` attribute to the nearest standard
/// value of the E-series its matching catalog family declares. Returns the
/// number of instances rewritten. Instances with no value, an unparseable
/// value, no matching family, or a value outside every declared range are
/// left untouched.
pub fn snap_netlist_values(netlist: &mut Netlist, families: &[FamilyDecl]) -> usize {
    // Resolve (class, value) per instance first to avoid borrow conflicts.
    let mut plan: HashMap<bhdl_netlist::InstanceId, String> = HashMap::new();
    let ids: Vec<_> = netlist.instances.keys().collect();
    for id in ids {
        let Some(class) = instance_class(netlist, id) else { continue };
        let inst = match netlist.instances.get(id) {
            Some(i) => i,
            None => continue,
        };
        let Some(raw) = inst.attributes.get("value") else { continue };
        let Some(v) = parse_value_string(raw) else { continue };

        // First family of this class with a range containing v wins; snap
        // to that family's series.
        let snapped = families
            .iter()
            .filter(|f| f.class == class)
            .flat_map(|f| f.series_ranges.iter())
            .find(|(_, lo, hi)| v >= *lo && v <= *hi)
            .map(|(series, _, _)| series.nearest(v));

        if let Some(snapped) = snapped {
            // Skip the rewrite if it's already on-grid (no churn).
            if (snapped - v).abs() > v.abs() * 1e-9 {
                plan.insert(id, format_value(snapped, class));
            }
        }
    }

    let n = plan.len();
    for (id, new_val) in plan {
        if let Some(inst) = netlist.instances.get_mut(id) {
            inst.attributes.insert("value".to_string(), new_val);
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_class_collapses_spellings() {
        assert_eq!(normalize_class("Res"), Some("resistor"));
        assert_eq!(normalize_class("Resistor"), Some("resistor"));
        assert_eq!(normalize_class("resistor"), Some("resistor"));
        assert_eq!(normalize_class("Cap"), Some("capacitor"));
        assert_eq!(normalize_class("Ind"), Some("inductor"));
        assert_eq!(normalize_class("Triode"), None);
    }

    #[test]
    fn parse_value_handles_prefix_and_dimension() {
        // Compare with relative tolerance — `100 * 1e-6` is not bit-equal
        // to the literal `0.0001` in f64.
        let approx = |s: &str, want: f64| {
            let got = parse_value_string(s).unwrap_or_else(|| panic!("{s} did not parse"));
            assert!(
                (got - want).abs() <= want.abs() * 1e-9,
                "{s}: got {got}, want {want}"
            );
        };
        approx("10k", 10_000.0); // bare SI prefix
        approx("10kΩ", 10_000.0); // prefix + dimension
        approx("1Ω", 1.0);
        approx("100µF", 100e-6);
        approx("4.7µH", 4.7e-6);
        approx("31600", 31_600.0); // bare
        assert_eq!(parse_value_string("nonsense"), None);
    }

    #[test]
    fn format_value_engineering_notation() {
        assert_eq!(format_value(31_600.0, "resistor"), "31.6kΩ");
        assert_eq!(format_value(4.7e-6, "capacitor"), "4.7µF");
        assert_eq!(format_value(6.8e-6, "inductor"), "6.8µH");
        assert_eq!(format_value(1_000.0, "resistor"), "1kΩ");
        assert_eq!(format_value(220.0, "resistor"), "220Ω");
    }

    #[test]
    fn parse_format_roundtrip_on_grid() {
        for (s, class) in [("31.6kΩ", "resistor"), ("4.7µF", "capacitor")] {
            let v = parse_value_string(s).unwrap();
            assert_eq!(format_value(v, class), s);
        }
    }

    #[test]
    fn harvest_extracts_class_and_series() {
        let src = r#"
            part_family Yageo_RC0603FR_07 : Resistor<R: *, "1%", "0603"> {
                require R in E96(1Ω, 10MΩ);
                attribute manufacturer = "Yageo";
            }
        "#;
        let pr = bhdl_parser::parse(src);
        let sf = SourceFile::cast(pr.syntax()).unwrap();
        let fams = harvest_families(&[sf]);
        assert_eq!(fams.len(), 1);
        assert_eq!(fams[0].class, "resistor");
        assert_eq!(fams[0].series_ranges.len(), 1);
        let (series, lo, hi) = &fams[0].series_ranges[0];
        assert_eq!(*series, ESeries::E96);
        assert!((*lo - 1.0).abs() < 1e-9);
        assert!((*hi - 10e6).abs() < 1.0);
    }
}
