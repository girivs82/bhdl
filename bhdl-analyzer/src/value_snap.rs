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

/// One catalog family's selection-relevant facts: the normalized
/// component class, the E-series value range(s) it stocks, and its
/// ratings + package (for stress-aware, size-minimizing selection).
#[derive(Debug, Clone, Default)]
pub struct FamilyDecl {
    /// part_family declaration name (e.g. `"Yageo_RC0603FR_07"`).
    pub name: String,
    /// Normalized component class — `"resistor"`, `"capacitor"`,
    /// `"inductor"` (see [`normalize_class`]).
    pub class: String,
    /// `(series, min, max)` for each `require axis in E_N(min, max)`
    /// clause, with min/max already lowered to SI base units.
    pub series_ranges: Vec<(ESeries, f64, f64)>,
    /// Package/footprint code (`"0603"`, `"1206"`, …) — the physical
    /// size, from an `attribute package = …`. Drives size minimization.
    pub package: Option<String>,
    /// Working-voltage rating in volts (`attribute voltage_rating = …`).
    pub voltage_rating: Option<f64>,
    /// Saturation/RMS current rating in amps (`attribute current_rating = …`).
    pub current_rating: Option<f64>,
    /// Power rating in watts (`attribute power_w = …`).
    pub power_w: Option<f64>,
}

/// Electrical stress a passive instance experiences. Any dimension that
/// isn't known (`None`) is simply not used to filter candidates.
#[derive(Debug, Clone, Default)]
pub struct Stress {
    /// Volts across the part (cap rail / resistor drop).
    pub voltage: Option<f64>,
    /// Amps through the part (inductor peak/RMS).
    pub current: Option<f64>,
    /// Watts dissipated (resistor I²R).
    pub power: Option<f64>,
}

/// A catalog family chosen for an instance, plus the value snapped to that
/// family's E-series.
#[derive(Debug, Clone)]
pub struct SelectedPart {
    pub family: FamilyDecl,
    /// The instance's value snapped to the chosen family's series.
    pub value: f64,
}

// Derating policy: a part's rating must exceed the operating stress by
// these factors (rating >= stress * factor). Ceramic caps lose
// capacitance under DC bias, so 2× headroom is conventional; power and
// resistor-voltage use 2× / 1.5×; inductor saturation current 1.25×.
const CAP_VOLTAGE_DERATE: f64 = 2.0;
const RES_VOLTAGE_DERATE: f64 = 1.5;
const RES_POWER_DERATE: f64 = 2.0;
const IND_CURRENT_DERATE: f64 = 1.25;

/// Relative physical size of a 2-terminal SMD package — lower is smaller
/// (preferred). Unknown packages rank last so a rating-adequate part with
/// a known small package always wins over an unspecified one.
fn package_rank(pkg: Option<&str>) -> u32 {
    match pkg.map(|s| s.trim()) {
        Some("01005") => 0,
        Some("0201") => 1,
        Some("0402") => 2,
        Some("0603") => 3,
        Some("0805") => 4,
        Some("1206") => 5,
        Some("1210") => 6,
        Some("1812") => 7,
        Some("2010") => 8,
        Some("2512") => 9,
        // Hand-coded larger footprints (power inductors etc.) sort above
        // chip sizes by their numeric prefix; unknown sorts last.
        Some(other) => other
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<u32>()
            .map(|n| 100 + n)
            .unwrap_or(u32::MAX - 1),
        None => u32::MAX,
    }
}

/// Does this family's ratings cover the (derated) stress for its class?
/// A rating that the family doesn't declare can't be checked, so it's
/// treated as adequate (the caller's catalog is responsible for declaring
/// the ratings it wants enforced).
fn rating_covers(f: &FamilyDecl, class: &str, stress: &Stress) -> bool {
    match class {
        "capacitor" => match (f.voltage_rating, stress.voltage) {
            (Some(rating), Some(v)) => rating >= v * CAP_VOLTAGE_DERATE,
            _ => true,
        },
        "resistor" => {
            let v_ok = match (f.voltage_rating, stress.voltage) {
                (Some(rating), Some(v)) => rating >= v * RES_VOLTAGE_DERATE,
                _ => true,
            };
            let p_ok = match (f.power_w, stress.power) {
                (Some(rating), Some(p)) => rating >= p * RES_POWER_DERATE,
                _ => true,
            };
            v_ok && p_ok
        }
        "inductor" => match (f.current_rating, stress.current) {
            (Some(rating), Some(i)) => rating >= i * IND_CURRENT_DERATE,
            _ => true,
        },
        _ => true,
    }
}

/// Select the smallest-package catalog family that stocks `value` (within
/// one of its E-series ranges) AND whose ratings cover the derated
/// `stress`, for the given normalized `class`. Returns the family plus the
/// value snapped to its series, or `None` if nothing adequate exists.
///
/// This is the stress-aware, size-minimizing successor to the value-only
/// [`snap_netlist_values`]: it answers "which real part" (value + rating +
/// package), not just "which standard value".
pub fn select_family<'a>(
    families: &'a [FamilyDecl],
    class: &str,
    value: f64,
    stress: &Stress,
) -> Option<&'a FamilyDecl> {
    families
        .iter()
        .filter(|f| f.class == class)
        .filter(|f| f.series_ranges.iter().any(|(_, lo, hi)| value >= *lo && value <= *hi))
        .filter(|f| rating_covers(f, class, stress))
        .min_by_key(|f| package_rank(f.package.as_deref()))
}

/// Snap `value` to `family`'s E-series (the range that contains it).
pub fn snap_to_family(family: &FamilyDecl, value: f64) -> f64 {
    family
        .series_ranges
        .iter()
        .find(|(_, lo, hi)| value >= *lo && value <= *hi)
        .map(|(series, _, _)| series.nearest(value))
        .unwrap_or(value)
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
    use bhdl_ast::HasName;
    let class = normalize_class(&pf.class_pattern()?.entity_name()?)?.to_string();
    let name = pf.name().map(|n| n.text().to_string()).unwrap_or_default();
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
    let attrs = read_part_family_attrs(pf);
    let num = |k: &str| attrs.get(k).and_then(|s| parse_value_string(s));
    Some(FamilyDecl {
        name,
        class,
        series_ranges,
        // `physical_package`, not `package`: `package` is a reserved
        // keyword (layout blocks), so `attribute package = …` fails to
        // parse. `physical_package` is also the key the BOM reads
        // (bhdl_common::sku::PACKAGE).
        package: attrs.get("physical_package").or_else(|| attrs.get("package")).cloned(),
        voltage_rating: num("voltage_rating"),
        current_rating: num("current_rating"),
        power_w: num("power_w"),
    })
}

/// Read `attribute K = V;` declarations directly under a `part_family`
/// node into a key→value map (values unquoted). Quick text-based parse,
/// matching the catalog scanner's approach.
fn read_part_family_attrs(pf: &PartFamilyDef) -> HashMap<String, String> {
    let mut out = HashMap::new();
    // Use descendants(), not children(): attribute decls may be nested
    // inside the part_family's body block rather than direct children.
    for child in pf.syntax().descendants() {
        if child.kind() != bhdl_parser::SyntaxKind::ATTRIBUTE_DECL {
            continue;
        }
        let text = child.text().to_string();
        let s = text
            .trim()
            .trim_start_matches("attribute")
            .trim()
            .trim_end_matches(';')
            .trim();
        if let Some((k, v)) = s.split_once('=') {
            out.insert(
                k.trim().to_string(),
                v.trim().trim_matches('"').to_string(),
            );
        }
    }
    out
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

    fn fam(name: &str, class: &str, pkg: &str, v: Option<f64>, p: Option<f64>, i: Option<f64>) -> FamilyDecl {
        FamilyDecl {
            name: name.into(),
            class: class.into(),
            series_ranges: vec![(ESeries::E12, 1e-12, 1e3)],
            package: Some(pkg.into()),
            voltage_rating: v,
            current_rating: i,
            power_w: p,
            ..Default::default()
        }
    }

    #[test]
    fn select_picks_smallest_package_meeting_ratings() {
        // Two cap families: 0603/50V and 1210/100V. A 12V rail (×2 derate
        // → needs ≥24V) fits both → pick the smaller 0603. A 60V rail
        // (needs ≥120V) fits only the 1210.
        let fams = vec![
            fam("C_0603_50V", "capacitor", "0603", Some(50.0), None, None),
            fam("C_1210_100V", "capacitor", "1210", Some(100.0), None, None),
        ];
        // 12V op → needs ≥24V; both fit → smaller 0603.
        let low = select_family(&fams, "capacitor", 1e-6, &Stress { voltage: Some(12.0), ..Default::default() });
        assert_eq!(low.unwrap().name, "C_0603_50V", "smallest adequate package");
        // 40V op → needs ≥80V; 50V part inadequate, 100V part fits → 1210.
        let high = select_family(&fams, "capacitor", 1e-6, &Stress { voltage: Some(40.0), ..Default::default() });
        assert_eq!(high.unwrap().name, "C_1210_100V", "only the 100V part is adequate");
        // 60V op → needs ≥120V; even the 100V part is inadequate → none.
        let none = select_family(&fams, "capacitor", 1e-6, &Stress { voltage: Some(60.0), ..Default::default() });
        assert!(none.is_none());
    }

    #[test]
    fn select_resistor_filters_on_power() {
        // 0603/0.1W vs 2512/1W. 0.3W dissipation (×2 → needs ≥0.6W) → 2512.
        let fams = vec![
            fam("R_0603", "resistor", "0603", Some(75.0), Some(0.1), None),
            fam("R_2512", "resistor", "2512", Some(200.0), Some(1.0), None),
        ];
        let s = select_family(&fams, "resistor", 100.0, &Stress { power: Some(0.3), voltage: Some(5.0), ..Default::default() });
        assert_eq!(s.unwrap().name, "R_2512", "0603 0.1W can't take 0.3W");
        let s2 = select_family(&fams, "resistor", 100.0, &Stress { power: Some(0.01), voltage: Some(5.0), ..Default::default() });
        assert_eq!(s2.unwrap().name, "R_0603", "low power → smallest");
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
        assert_eq!(fams[0].name, "Yageo_RC0603FR_07");
    }

    #[test]
    fn harvest_reads_package_and_ratings() {
        let src = r#"
            part_family X : Resistor<R: *, "1%", "1206"> {
                require R in E96(1Ω, 10MΩ);
                attribute physical_package = "1206";
                attribute power_w        = 0.25;
                attribute voltage_rating = 200;
            }
        "#;
        let pr = bhdl_parser::parse(src);
        let sf = SourceFile::cast(pr.syntax()).unwrap();
        let fams = harvest_families(&[sf]);
        assert_eq!(fams.len(), 1);
        assert_eq!(fams[0].package.as_deref(), Some("1206"), "package attr");
        assert_eq!(fams[0].power_w, Some(0.25), "power_w attr");
        assert_eq!(fams[0].voltage_rating, Some(200.0), "voltage_rating attr");
        let (series, lo, hi) = &fams[0].series_ranges[0];
        assert_eq!(*series, ESeries::E96);
        assert!((*lo - 1.0).abs() < 1e-9);
        assert!((*hi - 10e6).abs() < 1.0);
    }
}

