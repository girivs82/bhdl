//! General-purpose Bill of Materials walker.
//!
//! Walks every netlist instance, collects SKU attributes (see
//! [`bhdl_common::sku`]), groups identical parts, assigns reference
//! designators, and emits a complete BOM. Output formats:
//!
//! - Markdown table (human-readable, embeds in design docs).
//! - CSV (machine-readable, hands directly to PCB-assembly preorder
//!   forms like JLCPCB's parts-list upload).
//!
//! This is the post-Stage-6 manufacturing companion to the vendor
//! design block surface: every instance that survives synthesis —
//! including expansion children produced from `expansion { }` blocks
//! and bias resistors sized by `design { }` recipes — gets a row.
//!
//! Lookup order for each SKU attribute, instance by instance:
//!   1. The instance's own `attributes` map (board-level override).
//!   2. The instance's module-definition `attributes` map (entity
//!      defaults from the .bhdl file).
//!   3. Empty.
//!
//! Future supplier-layer integration: when an instance has a `value`
//! but no `mpn`/`manufacturer`, the supplier picker (in
//! `bhdl-components`) is called with category + value + optional
//! `tolerance` / `voltage_rating` / `package` constraints. That
//! query lives in a higher layer; this walker is a pure
//! netlist-reading reporter.

use bhdl_common::sku::{attr, refdes_prefix_for_class};
use bhdl_netlist::Netlist;
use std::collections::HashMap;
use std::fmt::Write;

/// A single row in the BOM after grouping.
#[derive(Debug, Clone)]
pub struct BomRow {
    /// Reference-designator list for the parts in this row (e.g.
    /// "R1, R2, R5" or "C3-C6, C9").
    pub ref_designators: Vec<String>,
    /// Quantity = ref_designators.len(), pre-computed for convenience.
    pub quantity: usize,
    /// User-visible value string (e.g. "10kΩ", "100nF", "MMBT3904LT1G").
    /// Falls back through value → mpn → part_number → component_class.
    pub value: String,
    /// Manufacturer name (canonical spelling). Empty if unknown.
    pub manufacturer: String,
    /// Manufacturer part number. Empty if unknown.
    pub mpn: String,
    /// Package / footprint hint for the assembler (e.g. "SOT-23",
    /// "0603"). Empty if unknown.
    pub package: String,
    /// Per-distributor SKUs. Keys are distributor names ("digikey",
    /// "mouser", "lcsc", …); values are the distributor's part
    /// number. Empty when the entity didn't pin a distributor SKU.
    pub distributors: HashMap<String, String>,
    /// component_class string ("resistor", "capacitor", "bjt", …).
    pub component_class: String,
    /// Datasheet URL, if known.
    pub datasheet: String,
}

/// Walk a netlist and produce a BOM. Instances with the same
/// (value, mpn, manufacturer, package) tuple are grouped onto one
/// row with multiple reference designators.
///
/// Reference designators are assigned by component_class using the
/// EDA-standard prefix table (R/C/L/D/Q/U/…). Within a class they're
/// numbered in netlist order — stable across re-runs when the
/// netlist is stable.
pub fn walk(netlist: &Netlist) -> Vec<BomRow> {
    // First pass: collect one row per instance, with its grouping key.
    #[derive(Clone)]
    struct PerInstance {
        refdes: String,
        value: String,
        manufacturer: String,
        mpn: String,
        package: String,
        footprint: String,
        component_class: String,
        datasheet: String,
        distributors: HashMap<String, String>,
    }

    // Per-class refdes counters keep ref designators contiguous.
    let mut counters: HashMap<&'static str, usize> = HashMap::new();
    let mut rows: Vec<PerInstance> = Vec::new();

    for (_inst_id, inst) in &netlist.instances {
        // Look up the called module's attribute defaults; instance
        // attributes shadow module attributes (board-level override).
        let module_attrs: &HashMap<String, String> = match netlist.modules.get(inst.definition) {
            Some(m) => &m.attributes,
            None => continue,
        };
        let get = |key: &str| -> String {
            inst.attributes.get(key)
                .or_else(|| module_attrs.get(key))
                .cloned()
                .unwrap_or_default()
        };

        let component_class = get(attr::COMPONENT_CLASS);
        // Skip net-symbol pseudo-instances (power rails, ground).
        // These have a component_class like "power" or "ground" and
        // shouldn't appear on a manufacturing BOM.
        if matches!(component_class.as_str(),
                    "power" | "ground" | "power_symbol" | "net" | "")
        {
            continue;
        }
        // Skip instances explicitly flagged as logical (entity
        // wrappers that the expansion interpreter dissolved into
        // their children — these are still in the netlist with
        // kind=Module but shouldn't be on the BOM).
        if let Some(module) = netlist.modules.get(inst.definition) {
            use bhdl_netlist::ModuleKind;
            if !matches!(module.kind, ModuleKind::PhysicalComponent) {
                continue;
            }
        }

        // Pick a reference-designator prefix: explicit override on the
        // entity wins, otherwise the class-based default.
        let prefix_attr = get(attr::REFDES_PREFIX);
        let prefix: &str = if !prefix_attr.is_empty() {
            // Leak to &'static str — tiny, bounded by the
            // (component_class) ↔ prefix map cardinality.
            Box::leak(prefix_attr.into_boxed_str())
        } else {
            refdes_prefix_for_class(&component_class)
        };
        let n = counters.entry(prefix).or_insert(0);
        *n += 1;
        let refdes = format!("{}{}", prefix, *n);

        // value falls back through several possible attributes —
        // useful because passives use `value`, ICs use `mpn`/
        // `part_number`, vendor-designed children get `value`
        // populated by the synthesizer.
        let value = {
            let v = get("value");
            if !v.is_empty() { v }
            else {
                let m = get(attr::MPN);
                if !m.is_empty() { m }
                else {
                    let p = get(attr::PART_NUMBER);
                    if !p.is_empty() { p }
                    else { component_class.clone() }
                }
            }
        };

        // Collect distributor SKUs into a single map for the row.
        let mut distributors = HashMap::new();
        for (key, name) in &[
            (attr::DIGIKEY_PN, "digikey"),
            (attr::MOUSER_PN,  "mouser"),
            (attr::LCSC_PN,    "lcsc"),
            (attr::ARROW_PN,   "arrow"),
            (attr::NEXAR_PN,   "nexar"),
        ] {
            let v = get(key);
            if !v.is_empty() {
                distributors.insert(name.to_string(), v);
            }
        }

        rows.push(PerInstance {
            refdes,
            value,
            manufacturer:   get(attr::MANUFACTURER),
            mpn:            get(attr::MPN),
            package:        get(attr::PACKAGE),
            footprint:      get(attr::FOOTPRINT),
            component_class,
            datasheet:      get(attr::DATASHEET),
            distributors,
        });
    }

    // Second pass: group rows with identical SKU fingerprints. The
    // grouping key intentionally ignores the refdes — that's the
    // thing we're trying to fold together.
    type Key = (String, String, String, String, String);
    let key_of = |r: &PerInstance| -> Key {
        (r.value.clone(), r.manufacturer.clone(), r.mpn.clone(),
         r.package.clone(), r.component_class.clone())
    };

    let mut grouped: Vec<(Key, Vec<PerInstance>)> = Vec::new();
    for row in rows {
        let k = key_of(&row);
        if let Some((_, bucket)) = grouped.iter_mut().find(|(kk, _)| *kk == k) {
            bucket.push(row);
        } else {
            grouped.push((k, vec![row]));
        }
    }

    grouped.into_iter().map(|(_, bucket)| {
        let head = &bucket[0];
        let mut refs: Vec<String> = bucket.iter().map(|r| r.refdes.clone()).collect();
        refs.sort_by(|a, b| natural_refdes_cmp(a, b));
        BomRow {
            quantity: refs.len(),
            ref_designators: refs,
            value: head.value.clone(),
            manufacturer: head.manufacturer.clone(),
            mpn: head.mpn.clone(),
            package: if !head.package.is_empty() { head.package.clone() } else { head.footprint.clone() },
            distributors: head.distributors.clone(),
            component_class: head.component_class.clone(),
            datasheet: head.datasheet.clone(),
        }
    }).collect()
}

/// Format the BOM as a Markdown table. Suitable for embedding in
/// design notes / READMEs.
pub fn to_markdown(rows: &[BomRow]) -> String {
    if rows.is_empty() {
        return "_No BOM-eligible components in the netlist._\n".to_string();
    }
    let mut out = String::new();
    let _ = writeln!(out, "| Ref des | Qty | Value | Manufacturer | MPN | Package | Distributor PNs |");
    let _ = writeln!(out, "|---------|-----|-------|--------------|-----|---------|-----------------|");
    for r in rows {
        let dist: String = {
            let mut keys: Vec<&String> = r.distributors.keys().collect();
            keys.sort();
            keys.iter()
                .map(|k| format!("{}={}", k, r.distributors[*k]))
                .collect::<Vec<_>>()
                .join("; ")
        };
        let refs = format_refdes_range(&r.ref_designators);
        let _ = writeln!(out, "| {} | {} | {} | {} | {} | {} | {} |",
            refs,
            r.quantity,
            r.value,
            if r.manufacturer.is_empty() { "_—_" } else { &r.manufacturer },
            if r.mpn.is_empty() { "_—_" } else { &r.mpn },
            if r.package.is_empty() { "_—_" } else { &r.package },
            if dist.is_empty() { "_—_".to_string() } else { dist });
    }
    out
}

/// Format the BOM as CSV (RFC-4180-ish — fields with commas/quotes
/// get quoted). Column order is fixed so downstream tooling can rely
/// on it.
pub fn to_csv(rows: &[BomRow]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "ref_designators,quantity,value,manufacturer,mpn,package,digikey_pn,mouser_pn,lcsc_pn,component_class,datasheet");
    for r in rows {
        let refs = r.ref_designators.join(" ");
        let cells = [
            refs.as_str(),
            &r.quantity.to_string(),
            r.value.as_str(),
            r.manufacturer.as_str(),
            r.mpn.as_str(),
            r.package.as_str(),
            r.distributors.get("digikey").map(String::as_str).unwrap_or(""),
            r.distributors.get("mouser").map(String::as_str).unwrap_or(""),
            r.distributors.get("lcsc").map(String::as_str).unwrap_or(""),
            r.component_class.as_str(),
            r.datasheet.as_str(),
        ];
        let line: Vec<String> = cells.iter().map(|c| csv_escape(c)).collect();
        let _ = writeln!(out, "{}", line.join(","));
    }
    out
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Natural-order comparison for reference designators so "R10" sorts
/// after "R9", not after "R1".
fn natural_refdes_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (a_pfx, a_num) = split_refdes(a);
    let (b_pfx, b_num) = split_refdes(b);
    a_pfx.cmp(b_pfx).then(a_num.cmp(&b_num))
}

fn split_refdes(s: &str) -> (&str, u64) {
    let split_at = s.find(|c: char| c.is_ascii_digit()).unwrap_or(s.len());
    let (pfx, num) = s.split_at(split_at);
    (pfx, num.parse().unwrap_or(0))
}

/// Collapse a list of reference designators into a compact range
/// representation: ["R1","R2","R3","R7"] → "R1-R3, R7".
fn format_refdes_range(refs: &[String]) -> String {
    if refs.is_empty() { return String::new(); }
    let mut sorted = refs.to_vec();
    sorted.sort_by(|a, b| natural_refdes_cmp(a, b));

    let mut out = String::new();
    let mut run_start: &str = &sorted[0];
    let mut run_end: &str = &sorted[0];
    let mut run_pfx: &str = split_refdes(&sorted[0]).0;
    let mut run_last_n: u64 = split_refdes(&sorted[0]).1;

    let emit = |out: &mut String, start: &str, end: &str| {
        if !out.is_empty() { out.push_str(", "); }
        if start == end { out.push_str(start); }
        else { out.push_str(start); out.push('-'); out.push_str(end); }
    };

    for r in &sorted[1..] {
        let (pfx, n) = split_refdes(r);
        if pfx == run_pfx && n == run_last_n + 1 {
            run_end = r.as_str();
            run_last_n = n;
        } else {
            emit(&mut out, run_start, run_end);
            run_start = r.as_str();
            run_end   = r.as_str();
            run_pfx   = pfx;
            run_last_n = n;
        }
    }
    emit(&mut out, run_start, run_end);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refdes_range_collapses_runs() {
        let refs = vec!["R1".into(), "R2".into(), "R3".into(), "R7".into(), "R8".into(), "C1".into()];
        let s = format_refdes_range(&refs);
        // Sort puts C-prefix first; then R1-R3 and R7-R8 as runs.
        assert_eq!(s, "C1, R1-R3, R7-R8");
    }

    #[test]
    fn refdes_range_handles_singletons() {
        let refs = vec!["U1".into()];
        assert_eq!(format_refdes_range(&refs), "U1");
    }

    #[test]
    fn natural_order_sorts_correctly() {
        let mut v: Vec<&str> = vec!["R10", "R2", "R1", "R20"];
        v.sort_by(|a, b| natural_refdes_cmp(a, b));
        assert_eq!(v, vec!["R1", "R2", "R10", "R20"]);
    }
}
