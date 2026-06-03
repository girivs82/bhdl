//! BHDL supply-chain provider — JLCPCB / LCSC (jlcparts dataset), Rust.
//!
//! Turns BHDL's parametric part requirements into real, orderable LCSC parts
//! (MPN + manufacturer + stock + price) by querying an OFFLINE jlcparts
//! SQLite catalogue. No API key, no network at query time, fully
//! reproducible — and, crucially, **zero runtime dependencies**: SQLite is
//! statically linked (`rusqlite`'s `bundled` feature), so the binary needs
//! neither a Python interpreter nor a system `libsqlite3`. A single
//! self-contained executable, consistent with the Rust workspace.
//!
//! Data source: CDFER/jlcpcb-parts-database (MIT), derived from
//! yaqwsx/jlcparts.
//!   full SQLite (~1.6 GB):  …/jlcpcb-components.sqlite3
//! The full catalogue (not just basic/preferred) covers the odd E96 values
//! and specialised parts the CSV subset omits (e.g. 1.65Ω, 31.6kΩ).
//!
//! Protocol (JSON over stdin/stdout), matching `bhdl-analyzer` plugin.rs:
//!
//!   stdin:  {"protocol":1, "requirements":[
//!              {"class_index":0, "class":"resistor", "value":10000.0,
//!               "package":"0603", "tolerance_pct":1.0} ...]}
//!   stdout: {"protocol_version":"1", "selections":[
//!              {"class_index":0, "mpn":"...", "manufacturer":"...",
//!               "vendor":"LCSC", "vendor_sku":"C25804", "stock":100000,
//!               "unit_price":0.0008, "currency":"USD", "note":"basic"} ...],
//!            "warnings":[...]}
//!
//! `value` is SI base units (Ω / F / H). The DB path comes from
//! `$BHDL_JLCPARTS_DB` or argv[1].

use regex::Regex;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;

// ── Protocol types ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct Requirements {
    #[serde(default)]
    requirements: Vec<Requirement>,
    /// Top-level default objective when a requirement omits its own.
    #[serde(default)]
    objective: Option<Objective>,
    /// Top-level default build quantity (for the price tier + stock headroom).
    #[serde(default)]
    quantity: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct Requirement {
    class_index: usize,
    class: String,
    #[serde(default)]
    value: Option<f64>,
    #[serde(default)]
    package: Option<String>,
    #[serde(default)]
    tolerance_pct: Option<f64>,
    /// Hard gate on the *part's* tolerance grade (±%): candidates worse than
    /// this are infeasible. E.g. a feedback divider that needs ≤1% parts.
    #[serde(default)]
    max_tolerance_pct: Option<f64>,
    /// Hard gate on a ceramic capacitor's dielectric (e.g. `"C0G"`): only
    /// parts of that class (C0G≡NP0 aliased) are feasible. For
    /// filter/timing/reference caps that need a temperature-stable
    /// dielectric.
    #[serde(default)]
    dielectric: Option<String>,
    /// Per-requirement optimization objective (overrides the top-level one).
    #[serde(default)]
    objective: Option<Objective>,
    /// Per-requirement build quantity (overrides the top-level one).
    #[serde(default)]
    quantity: Option<u64>,
}

/// An optimization objective: either a named profile or an explicit weight
/// set. Deserializes from a bare string (`"cost"`) or an object
/// (`{"value":1.0,"price":0.1,...}`).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum Objective {
    Profile(String),
    Weights(Weights),
}

/// Weights for the soft cost terms (each normalized 0..1 across the feasible
/// candidate set before weighting). Lower total score is better.
#[derive(Debug, Clone, Copy, Deserialize)]
struct Weights {
    #[serde(default)]
    value: f64,
    #[serde(default)]
    price: f64,
    #[serde(default)]
    assembly: f64,
    #[serde(default)]
    stock: f64,
    #[serde(default)]
    lead: f64,
    /// part tolerance grade (tighter % is better) — for precision paths
    #[serde(default)]
    tolerance: f64,
    /// temperature drift (lower ppm/°C, better dielectric) is better
    #[serde(default)]
    tempco: f64,
}

impl Weights {
    /// Named profiles. Unknown names fall back to `balanced`.
    fn profile(name: &str) -> Weights {
        match name.trim().to_ascii_lowercase().as_str() {
            // value error dominates → exact E-series wins (the old behaviour);
            // value is the *only* term, tiebroken (deterministically) by price
            "precision" | "value" | "exact" => Weights {
                value: 1.0,
                ..Self::zero()
            },
            // precision *path*: exact value AND high part grade — tight
            // tolerance, low temperature drift (e.g. feedback dividers,
            // measurement/reference chains). Cost is secondary.
            "grade" | "precision-grade" | "feedback" | "measurement" | "reference" => Weights {
                value: 0.7,
                tolerance: 1.0,
                tempco: 0.7,
                price: 0.1,
                assembly: 0.0,
                stock: 0.1,
                lead: 0.0,
            },
            // cheapest to assemble: unit price + basic/extended fee dominate;
            // a slightly-off-but-in-tolerance value is acceptable
            "cost" | "cheap" | "price" => Weights {
                value: 0.2,
                price: 1.0,
                assembly: 0.8,
                stock: 0.2,
                ..Self::zero()
            },
            // production resilience: maximise stock headroom, minimise lead
            "availability" | "stock" | "supply" => Weights {
                value: 0.3,
                price: 0.2,
                assembly: 0.2,
                stock: 1.0,
                lead: 0.5,
                ..Self::zero()
            },
            // sensible all-rounder
            _ => Weights {
                value: 0.5,
                price: 0.5,
                assembly: 0.4,
                stock: 0.3,
                lead: 0.1,
                tolerance: 0.2,
                tempco: 0.1,
            },
        }
    }

    fn zero() -> Weights {
        Weights {
            value: 0.0,
            price: 0.0,
            assembly: 0.0,
            stock: 0.0,
            lead: 0.0,
            tolerance: 0.0,
            tempco: 0.0,
        }
    }
}

impl Objective {
    fn weights(&self) -> Weights {
        match self {
            Objective::Profile(name) => Weights::profile(name),
            Objective::Weights(w) => *w,
        }
    }
}

#[derive(Debug, Serialize)]
struct Response {
    protocol_version: String,
    selections: Vec<Selection>,
    warnings: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
struct Selection {
    class_index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    mpn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    manufacturer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vendor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vendor_sku: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stock: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unit_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

// ── Class → catalogue mapping ────────────────────────────────────

/// Top-level jlcparts `category` for each BHDL passive class.
fn category_for(class: &str) -> Option<&'static str> {
    match class {
        "resistor" => Some("Resistors"),
        "capacitor" => Some("Capacitors"),
        "inductor" => Some("Inductors/Coils/Transformers"),
        _ => None,
    }
}

/// Dimension unit letter as it appears in the `description` text.
fn unit_for(class: &str) -> Option<char> {
    match class {
        "resistor" => Some('Ω'),
        "capacitor" => Some('F'),
        "inductor" => Some('H'),
        _ => None,
    }
}

// ── Value parsing out of the description text ────────────────────

const PREFIX: &[(&str, f64)] = &[
    ("p", 1e-12),
    ("n", 1e-9),
    ("u", 1e-6),
    ("µ", 1e-6), // U+00B5 micro sign
    ("μ", 1e-6), // U+03BC greek mu
    ("m", 1e-3),
    ("k", 1e3),
    ("K", 1e3),
    ("M", 1e6),
    ("G", 1e9),
];

fn prefix_mult(p: &str) -> f64 {
    if p.is_empty() {
        return 1.0;
    }
    PREFIX
        .iter()
        .find(|(s, _)| *s == p)
        .map(|(_, m)| *m)
        .unwrap_or(1.0)
}

/// Extract the first `<number><prefix><unit>` value from `text` as an
/// SI-base float — "510kΩ"→510e3, "100nF"→100e-9, "6.8uH"→6.8e-6.
///
/// The `regex` crate has no lookahead, so we reject a Henry match that is
/// actually a frequency ("100MHz") by checking the byte after the unit:
/// a following 'z'/'Z' means it was "Hz", not Henry.
fn parse_value(re: &Regex, unit: char, text: &str) -> Option<f64> {
    for caps in re.captures_iter(text) {
        let m = caps.get(0).unwrap();
        if unit == 'H' {
            let after = &text[m.end()..];
            if matches!(after.chars().next(), Some('z') | Some('Z')) {
                continue; // "Hz" — frequency, not Henry
            }
        }
        let num: f64 = caps.get(1).unwrap().as_str().parse().ok()?;
        let mult = prefix_mult(caps.get(2).map(|m| m.as_str()).unwrap_or(""));
        let v = num * mult;
        if v > 0.0 {
            return Some(v);
        }
    }
    None
}

/// Unit price for a given build quantity from the tiered `price` JSON array
/// (`[{"qFrom":1,"qTo":10,"price":x}, {"qFrom":11,"qTo":null,...}]`). Picks
/// the tier whose `[qFrom, qTo]` window contains `qty`; falls back to the
/// last tier at or below `qty`, else the first tier.
fn price_at_qty(price_json: &str, qty: u64) -> Option<f64> {
    let v: serde_json::Value = serde_json::from_str(price_json).ok()?;
    let tiers = v.as_array()?;
    let price_of = |t: &serde_json::Value| -> Option<f64> {
        t.get("price")
            .and_then(|p| p.as_f64().or_else(|| p.as_str().and_then(|s| s.parse().ok())))
    };
    let mut fallback: Option<f64> = None;
    for t in tiers {
        let q_from = t.get("qFrom").and_then(|x| x.as_u64()).unwrap_or(0);
        let q_to = t.get("qTo").and_then(|x| x.as_u64()); // null = open-ended
        if let Some(p) = price_of(t) {
            if fallback.is_none() {
                fallback = Some(p); // first valid tier
            }
            let in_window = qty >= q_from && q_to.map(|hi| qty <= hi).unwrap_or(true);
            if in_window {
                return Some(p);
            }
            if q_from <= qty {
                fallback = Some(p); // best tier at or below qty so far
            }
        }
    }
    fallback
}

// ── Part-grade parsing (tolerance, temperature drift) ────────────

/// Part tolerance as a percentage from the description (`±1%`, `±0.1%`).
/// Falls back to the worst leg of an asymmetric spec (`-20%~+80%` → 80,
/// the Y5V case) so loose parts aren't mistaken for tight ones.
fn parse_tolerance_pct(text: &str) -> Option<f64> {
    // ±N% (the symmetric, well-specified case)
    if let Some(c) = regex_tol_pm().captures(text) {
        if let Ok(v) = c[1].parse::<f64>() {
            return Some(v);
        }
    }
    // else the max of any N% appearing (covers -20%~+80%)
    let mut worst: Option<f64> = None;
    for c in regex_tol_any().captures_iter(text) {
        if let Ok(v) = c[1].parse::<f64>() {
            worst = Some(worst.map_or(v, |w: f64| w.max(v)));
        }
    }
    worst
}

/// Temperature drift as ppm/°C. Resistors state it directly (`±100ppm/℃`);
/// ceramic capacitors encode it in the dielectric code (C0G/NP0 ≪ X7R ≪ Y5V),
/// mapped to a representative ppm-equivalent for ranking.
fn parse_tempco_ppm(text: &str, class: &str) -> Option<f64> {
    if let Some(c) = regex_ppm().captures(text) {
        if let Ok(v) = c[1].parse::<f64>() {
            return Some(v);
        }
    }
    if class == "capacitor" {
        return dielectric_drift(text);
    }
    None
}

/// Map a ceramic dielectric code → representative drift (ppm-equiv) for
/// ranking. C0G/NP0 are temperature-stable; Y5V/Z5U swing wildly.
fn dielectric_drift(text: &str) -> Option<f64> {
    let up = text.to_ascii_uppercase();
    // order matters: check the stable codes first
    const TABLE: &[(&str, f64)] = &[
        ("C0G", 30.0),
        ("NP0", 30.0),
        ("X8R", 150.0),
        ("X7R", 800.0),
        ("X7S", 800.0),
        ("X7T", 800.0),
        ("X6S", 800.0),
        ("X5R", 800.0),
        ("Y5V", 10000.0),
        ("Z5U", 10000.0),
        ("Y5U", 10000.0),
    ];
    TABLE
        .iter()
        .find(|(code, _)| up.contains(code))
        .map(|(_, d)| *d)
}

/// Does a part's description satisfy a required dielectric? Case-insensitive
/// substring, with C0G≡NP0 treated as equivalent (they name the same
/// temperature-stable Class-I dielectric).
fn dielectric_matches(want: &str, desc: &str) -> bool {
    let up = desc.to_ascii_uppercase();
    let w = want.trim().trim_matches('"').trim().to_ascii_uppercase();
    if w.is_empty() {
        return true;
    }
    let w_str = w.as_str();
    let aliases: &[&str] = if w == "C0G" || w == "NP0" {
        &["C0G", "NP0"]
    } else {
        std::slice::from_ref(&w_str)
    };
    aliases.iter().any(|a| up.contains(a))
}

// Compiled lazily once each (no `lazy_static`/`once_cell` needed — built in
// main and threaded, but these tiny helpers re-use thread-local-free statics
// via a single OnceLock).
use std::sync::OnceLock;
fn regex_tol_pm() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"±\s*([0-9]+(?:\.[0-9]+)?)\s*%").unwrap())
}
fn regex_tol_any() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"([0-9]+(?:\.[0-9]+)?)\s*%").unwrap())
}
fn regex_ppm() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"([0-9]+(?:\.[0-9]+)?)\s*ppm").unwrap())
}

// ── Resolution ───────────────────────────────────────────────────

/// How the requested package code constrains the SQL query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PkgMode {
    /// `package` string equality (R/C EIA codes match verbatim).
    Strict,
    /// code appears as a substring of the package string or the MPN
    /// (translates inductor size codes like `6045` → `SRN6045-…`).
    Token,
    /// no package constraint — value only.
    Any,
}

struct Catalogue {
    conn: Connection,
    /// class → category_ids (resolved once; the indexed filter column)
    cat_ids: HashMap<String, Vec<i64>>,
}

impl Catalogue {
    fn open(path: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        Ok(Self {
            conn,
            cat_ids: HashMap::new(),
        })
    }

    fn category_ids(&mut self, class: &str) -> rusqlite::Result<Vec<i64>> {
        if let Some(ids) = self.cat_ids.get(class) {
            return Ok(ids.clone());
        }
        let category = match category_for(class) {
            Some(c) => c,
            None => return Ok(vec![]),
        };
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM categories WHERE category = ?1")?;
        let ids: Vec<i64> = stmt
            .query_map([category], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        self.cat_ids.insert(class.to_string(), ids.clone());
        Ok(ids)
    }

    /// Resolve one requirement with a footprint-matching cascade, so the
    /// provider actively translates between BHDL's package codes and
    /// jlcparts' freeform notation rather than giving up:
    ///
    /// 1. **Strict** — `package` string equality. Works directly for R/C,
    ///    whose EIA codes (0603, 1206…) jlcparts uses verbatim.
    /// 2. **Code token** — the requested code appears as a substring in the
    ///    package string *or the MPN*. This is the inductor translator: a
    ///    "6045" request matches `SRN6045-6R8` / `NR6045…` even though
    ///    jlcparts labels the package `SMD,6x6mm` (in SRN**6045** the 45 is
    ///    the 4.5 mm height, not the width — so naive L×W translation is
    ///    wrong, but the size code lives in the part number). Footprint is
    ///    still considered confirmed.
    /// 3. **Value-only** — last resort, no package constraint, with a
    ///    warning that the footprint could not be confirmed.
    fn resolve(
        &mut self,
        req: &Requirement,
        re: &Regex,
        weights: Weights,
        qty: u64,
        warnings: &mut Vec<String>,
    ) -> Selection {
        let unit = match unit_for(&req.class) {
            Some(u) => u,
            None => {
                return Selection {
                    class_index: req.class_index,
                    error: Some(format!("unsupported class '{}'", req.class)),
                    ..Default::default()
                }
            }
        };
        let ids = match self.category_ids(&req.class) {
            Ok(ids) if !ids.is_empty() => ids,
            _ => {
                return Selection {
                    class_index: req.class_index,
                    error: Some(format!("no catalogue category for '{}'", req.class)),
                    ..Default::default()
                }
            }
        };

        let want_pkg = req.package.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let tol = req.tolerance_pct.unwrap_or(2.0) / 100.0;

        // footprint cascade: strict string → code token in package/MPN →
        // value-only (warn). Within the first non-empty feasible set we run
        // the multi-objective score, so the footprint gate stays hard while
        // value/price/assembly/stock trade off softly.
        let modes: &[PkgMode] = match want_pkg {
            Some(_) => &[PkgMode::Strict, PkgMode::Token, PkgMode::Any],
            None => &[PkgMode::Any],
        };
        for mode in modes {
            let cands = self.collect_feasible(&ids, req, unit, re, tol, qty, want_pkg, *mode);
            if let Some(sel) = score_and_pick(req, &cands, weights, tol) {
                if *mode == PkgMode::Any && want_pkg.is_some() {
                    warnings.push(format!(
                        "class_index {}: no in-stock {} matched package '{}' \
                         (by code or MPN); selected on value only (footprint not confirmed)",
                        req.class_index,
                        req.class,
                        want_pkg.unwrap_or("")
                    ));
                }
                return sel;
            }
        }

        Selection {
            class_index: req.class_index,
            error: Some(format!(
                "no in-stock {} matching value/package in catalogue",
                req.class
            )),
            ..Default::default()
        }
    }

    /// Gather every feasible in-stock candidate for one footprint mode:
    /// value within tolerance (hard gate). Bounded to `MAX_CANDS` rows in
    /// rank order so a loose tolerance can't blow up. The soft scoring then
    /// runs over this set.
    fn collect_feasible(
        &self,
        cat_ids: &[i64],
        req: &Requirement,
        unit: char,
        re: &Regex,
        tol: f64,
        qty: u64,
        pkg: Option<&str>,
        mode: PkgMode,
    ) -> Vec<Cand> {
        const MAX_CANDS: usize = 4000;
        let placeholders = cat_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let mut sql = format!(
            "SELECT mfr, manufacturer, basic, preferred, stock, price, description, lcsc \
             FROM v_components \
             WHERE category_id IN ({placeholders}) AND stock > 0"
        );
        match (mode, pkg) {
            (PkgMode::Strict, Some(_)) => sql.push_str(" AND lower(package) = lower(?)"),
            (PkgMode::Token, Some(_)) => {
                sql.push_str(" AND (lower(package) LIKE lower(?) OR lower(mfr) LIKE lower(?))")
            }
            _ => {}
        }
        // rank order so the bounded scan keeps the most relevant rows
        sql.push_str(" ORDER BY basic DESC, preferred DESC, stock DESC");

        let mut out = Vec::new();
        let mut stmt = match self.conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return out,
        };
        let mut params: Vec<&dyn rusqlite::ToSql> = Vec::new();
        for id in cat_ids {
            params.push(id);
        }
        let pkg_exact;
        let pkg_like;
        match (mode, pkg) {
            (PkgMode::Strict, Some(p)) => {
                pkg_exact = p.to_string();
                params.push(&pkg_exact);
            }
            (PkgMode::Token, Some(p)) => {
                pkg_like = format!("%{p}%");
                params.push(&pkg_like);
                params.push(&pkg_like);
            }
            _ => {}
        }
        let mut rows = match stmt.query(params.as_slice()) {
            Ok(r) => r,
            Err(_) => return out,
        };
        while let Ok(Some(row)) = rows.next() {
            if out.len() >= MAX_CANDS {
                break;
            }
            let description: String = match row.get(6) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let mut value_err = 0.0;
            if let Some(target) = req.value {
                let v = match parse_value(re, unit, &description) {
                    Some(v) => v,
                    None => continue,
                };
                value_err = (v - target).abs() / target.max(1e-30);
                if value_err > tol.max(1e-9) {
                    continue; // hard gate: value out of tolerance
                }
            }
            let tol_pct = parse_tolerance_pct(&description);
            // hard gate: part tolerance grade worse than required → infeasible
            if let Some(max_tol) = req.max_tolerance_pct {
                match tol_pct {
                    Some(t) if t <= max_tol + 1e-9 => {}
                    // unknown tolerance is treated as failing a hard grade gate
                    _ => continue,
                }
            }
            // hard gate: required ceramic dielectric (C0G/NP0, X7R, …)
            if let Some(want) = req.dielectric.as_deref() {
                if !dielectric_matches(want, &description) {
                    continue;
                }
            }
            let tempco = parse_tempco_ppm(&description, &req.class);
            let basic: i64 = row.get(2).unwrap_or(0);
            let preferred: i64 = row.get(3).unwrap_or(0);
            let stock: i64 = row.get(4).unwrap_or(0);
            let price_json: String = row.get(5).unwrap_or_default();
            out.push(Cand {
                mfr: row.get(0).unwrap_or_default(),
                manufacturer: row.get(1).ok(),
                lcsc: row.get(7).unwrap_or(0),
                value_err,
                unit_price: price_at_qty(&price_json, qty),
                tol_pct,
                tempco,
                // assembly-fee proxy: basic parts are free to place, preferred
                // mid, extended carries the per-part fee + feeder setup
                assembly: if basic == 1 {
                    0.0
                } else if preferred == 1 {
                    0.5
                } else {
                    1.0
                },
                stock: stock.max(0) as u64,
                note: if basic == 1 {
                    "basic"
                } else if preferred == 1 {
                    "preferred"
                } else {
                    "extended"
                },
            });
        }
        out
    }
}

/// One feasible candidate (passed the hard gates) with the raw metrics the
/// scorer normalizes and weights.
struct Cand {
    mfr: String,
    manufacturer: Option<String>,
    lcsc: i64,
    value_err: f64,
    unit_price: Option<f64>,
    assembly: f64,
    stock: u64,
    /// part tolerance grade (±%), lower = better
    tol_pct: Option<f64>,
    /// temperature drift (ppm/°C or dielectric proxy), lower = better
    tempco: Option<f64>,
    note: &'static str,
}

/// Min-max normalize a metric across candidates → 0..1 (0 = best/lowest).
/// All-equal collapses to 0 so the term drops out.
fn normalize(vals: &[f64]) -> Vec<f64> {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for &v in vals {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    let span = hi - lo;
    if span <= f64::EPSILON {
        return vec![0.0; vals.len()];
    }
    vals.iter().map(|&v| (v - lo) / span).collect()
}

/// Score the feasible set and return the lowest-cost pick. Each soft term is
/// min-max normalized across the set, then weighted; lower total wins.
/// Tiebroken by value error then price so the result is deterministic.
fn score_and_pick(req: &Requirement, cands: &[Cand], w: Weights, tol: f64) -> Option<Selection> {
    if cands.is_empty() {
        return None;
    }
    // Value error is an *absolute spec* metric, not a relative one: normalize
    // against the tolerance budget (0 = exact, 1 = at the tolerance edge) so a
    // wide tolerance band doesn't dilute it. Min-max (below) is right for the
    // genuinely relative quantities (price, stock).
    let tol_eff = tol.max(1e-9);
    let ve_n: Vec<f64> = cands
        .iter()
        .map(|c| (c.value_err / tol_eff).min(1.0))
        .collect();
    // missing price → treated as worst (max) so unknowns aren't free
    let max_price = cands
        .iter()
        .filter_map(|c| c.unit_price)
        .fold(0.0_f64, f64::max);
    let pr: Vec<f64> = cands
        .iter()
        .map(|c| c.unit_price.unwrap_or(max_price))
        .collect();
    let asm: Vec<f64> = cands.iter().map(|c| c.assembly).collect();
    // stock as a cost: more stock = lower cost, so negate (headroom relative
    // to the build qty is captured by the normalization span)
    let st: Vec<f64> = cands.iter().map(|c| -(c.stock as f64)).collect();
    // lead time isn't in the offline catalogue → uniform 0 (term drops out)
    let ld: Vec<f64> = vec![0.0; cands.len()];
    // part grade: tighter tolerance % and lower drift ppm are better. Missing
    // → treated as the worst in the set so an unspecified part can't masquerade
    // as a precision one.
    let worst_tol = cands.iter().filter_map(|c| c.tol_pct).fold(0.0_f64, f64::max);
    let worst_tc = cands.iter().filter_map(|c| c.tempco).fold(0.0_f64, f64::max);
    let tl: Vec<f64> = cands.iter().map(|c| c.tol_pct.unwrap_or(worst_tol)).collect();
    let tc: Vec<f64> = cands.iter().map(|c| c.tempco.unwrap_or(worst_tc)).collect();

    let pr_n = normalize(&pr);
    let asm_n = normalize(&asm);
    let st_n = normalize(&st);
    let ld_n = normalize(&ld);
    let tl_n = normalize(&tl);
    let tc_n = normalize(&tc);

    let mut best_i = 0;
    let mut best = f64::INFINITY;
    for i in 0..cands.len() {
        let score = w.value * ve_n[i]
            + w.price * pr_n[i]
            + w.assembly * asm_n[i]
            + w.stock * st_n[i]
            + w.lead * ld_n[i]
            + w.tolerance * tl_n[i]
            + w.tempco * tc_n[i];
        // deterministic tiebreak: lower score, then closer value, then cheaper
        let better = score < best - 1e-12
            || ((score - best).abs() <= 1e-12
                && (cands[i].value_err < cands[best_i].value_err
                    || (cands[i].value_err == cands[best_i].value_err
                        && pr[i] < pr[best_i])));
        if better {
            best = score;
            best_i = i;
        }
    }

    let c = &cands[best_i];
    Some(Selection {
        class_index: req.class_index,
        mpn: Some(c.mfr.clone()),
        manufacturer: c.manufacturer.clone(),
        vendor: Some("LCSC".to_string()),
        vendor_sku: Some(format!("C{}", c.lcsc)),
        stock: Some(c.stock),
        unit_price: c.unit_price,
        currency: Some("USD".to_string()),
        note: Some(c.note.to_string()),
        error: None,
    })
}

fn emit(resp: &Response) {
    println!("{}", serde_json::to_string(resp).unwrap());
}

fn main() {
    let db_path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("BHDL_JLCPARTS_DB").ok());

    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        emit(&Response {
            protocol_version: "1".into(),
            selections: vec![],
            warnings: vec!["failed to read requirements from stdin".into()],
        });
        return;
    }
    let reqs: Requirements = match serde_json::from_str(&input) {
        Ok(r) => r,
        Err(e) => {
            emit(&Response {
                protocol_version: "1".into(),
                selections: vec![],
                warnings: vec![format!("malformed requirements JSON: {e}")],
            });
            return;
        }
    };

    let db_path = match db_path {
        Some(p) if std::path::Path::new(&p).exists() => p,
        _ => {
            // No data: well-formed empty response so the caller falls back to
            // catalogue defaults rather than failing.
            emit(&Response {
                protocol_version: "1".into(),
                selections: vec![],
                warnings: vec![
                    "jlcparts SQLite not found; set $BHDL_JLCPARTS_DB or pass a path \
                     (download from cdfer.github.io/jlcpcb-parts-database)"
                        .into(),
                ],
            });
            return;
        }
    };

    let mut cat = match Catalogue::open(&db_path) {
        Ok(c) => c,
        Err(e) => {
            emit(&Response {
                protocol_version: "1".into(),
                selections: vec![],
                warnings: vec![format!("cannot open jlcparts DB '{db_path}': {e}")],
            });
            return;
        }
    };

    // number, optional SI prefix, the unit letter
    let mut warnings = Vec::new();
    let mut selections = Vec::with_capacity(reqs.requirements.len());
    // top-level defaults (overridden per-requirement)
    let default_weights = reqs
        .objective
        .as_ref()
        .map(Objective::weights)
        .unwrap_or_else(|| Weights::profile("balanced"));
    let default_qty = reqs.quantity.unwrap_or(1).max(1);
    // one compiled regex per unit (cheap; ≤3 distinct units)
    let mut res: HashMap<char, Regex> = HashMap::new();
    for req in &reqs.requirements {
        let unit = match unit_for(&req.class) {
            Some(u) => u,
            None => {
                selections.push(Selection {
                    class_index: req.class_index,
                    error: Some(format!("unsupported class '{}'", req.class)),
                    ..Default::default()
                });
                continue;
            }
        };
        let re = res.entry(unit).or_insert_with(|| {
            Regex::new(&format!(
                r"(\d+(?:\.\d+)?)\s*([pnuµμmkKMG]?){}",
                regex::escape(&unit.to_string())
            ))
            .unwrap()
        });
        let re = re.clone();
        let weights = req
            .objective
            .as_ref()
            .map(Objective::weights)
            .unwrap_or(default_weights);
        let qty = req.quantity.unwrap_or(default_qty).max(1);
        selections.push(cat.resolve(req, &re, weights, qty, &mut warnings));
    }

    emit(&Response {
        protocol_version: "1".into(),
        selections,
        warnings,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn re(unit: char) -> Regex {
        Regex::new(&format!(
            r"(\d+(?:\.\d+)?)\s*([pnuµμmkKMG]?){}",
            regex::escape(&unit.to_string())
        ))
        .unwrap()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= b.abs() * 1e-9
    }

    #[test]
    fn resistor_values() {
        let r = re('Ω');
        assert!(close(parse_value(&r, 'Ω', "100mW 1.65Ω 75V ±1%").unwrap(), 1.65));
        assert!(close(parse_value(&r, 'Ω', "100mW 31.6kΩ 75V ±1%").unwrap(), 31_600.0));
        assert!(close(parse_value(&r, 'Ω', "121Ω Thick Film").unwrap(), 121.0));
        assert!(close(parse_value(&r, 'Ω', "82kΩ").unwrap(), 82_000.0));
    }

    #[test]
    fn capacitor_values() {
        let r = re('F');
        assert!(close(parse_value(&r, 'F', "100nF 50V X7R").unwrap(), 100e-9));
        assert!(close(parse_value(&r, 'F', "4.7µF 25V").unwrap(), 4.7e-6));
        // "Film" must NOT parse as an F-unit value (no leading number)
        assert_eq!(parse_value(&r, 'F', "Thick Film Resistor"), None);
    }

    #[test]
    fn inductor_ascii_micro_and_hz_rejection() {
        let r = re('H');
        // ASCII 'u' for micro (jlcparts inductor notation)
        assert!(close(parse_value(&r, 'H', "1.7Ω 6.8uH ±10% 0603").unwrap(), 6.8e-6));
        assert!(close(parse_value(&r, 'H', "6.8nH ±5%").unwrap(), 6.8e-9));
        // "MHz"/"GHz" are frequencies, not Henry — must be rejected
        assert_eq!(parse_value(&r, 'H', "3.9GHz 300mΩ"), None);
        // a frequency followed by a real inductance still finds the inductance
        assert!(close(
            parse_value(&r, 'H', "8@100MHz 6.8nH ±5%").unwrap(),
            6.8e-9
        ));
    }

    #[test]
    fn price_tier_selection() {
        let j = r#"[{"qFrom":1,"qTo":9,"price":0.01},
                    {"qFrom":10,"qTo":99,"price":0.005},
                    {"qFrom":100,"qTo":null,"price":0.002}]"#;
        assert!(close(price_at_qty(j, 1).unwrap(), 0.01));
        assert!(close(price_at_qty(j, 50).unwrap(), 0.005));
        assert!(close(price_at_qty(j, 100).unwrap(), 0.002));
        assert!(close(price_at_qty(j, 100_000).unwrap(), 0.002)); // open-ended top tier
    }

    fn cand(value_err: f64, price: f64, asm: f64, stock: u64, note: &'static str) -> Cand {
        Cand {
            mfr: format!("M{note}{stock}"),
            manufacturer: None,
            lcsc: stock as i64,
            value_err,
            unit_price: Some(price),
            assembly: asm,
            stock,
            tol_pct: None,
            tempco: None,
            note,
        }
    }

    /// graded candidate: label carries the tolerance for readable assertions
    fn candg(value_err: f64, price: f64, tol: f64, tempco: f64, note: &'static str) -> Cand {
        Cand {
            mfr: format!("R{tol}"),
            manufacturer: None,
            lcsc: (tol * 1000.0) as i64,
            value_err,
            unit_price: Some(price),
            assembly: 0.0,
            stock: 100_000,
            tol_pct: Some(tol),
            tempco: Some(tempco),
            note,
        }
    }

    fn req() -> Requirement {
        Requirement {
            class_index: 0,
            class: "resistor".into(),
            value: Some(121.0),
            package: Some("0603".into()),
            tolerance_pct: Some(2.0),
            max_tolerance_pct: None,
            dielectric: None,
            objective: None,
            quantity: None,
        }
    }

    #[test]
    fn dielectric_gate() {
        let c0g = "100pF 50V C0G ±5% 0402 MLCC";
        let np0 = "100pF 50V NP0 ±5% 0402 MLCC";
        let x7r = "100nF 50V X7R ±10% 0603 MLCC";
        // C0G request accepts C0G and its NP0 alias, rejects X7R
        assert!(dielectric_matches("C0G", c0g));
        assert!(dielectric_matches("C0G", np0));
        assert!(!dielectric_matches("C0G", x7r));
        // case / quote / whitespace robustness
        assert!(dielectric_matches(" \"c0g\" ", c0g));
        // X7R request is exact (not satisfied by C0G)
        assert!(dielectric_matches("X7R", x7r));
        assert!(!dielectric_matches("X7R", c0g));
    }

    #[test]
    fn profile_changes_pick() {
        // A: exact value, extended, pricey, low stock.
        // B: 0.8% off, basic, cheap, huge stock.
        let cands = vec![
            cand(0.0, 0.02, 1.0, 5_000, "extended"),
            cand(0.008, 0.001, 0.0, 9_000_000, "basic"),
        ];
        let tol = 0.02; // req tolerance 2% as a fraction
        // precision → exact value wins (candidate A / extended)
        let p = score_and_pick(&req(), &cands, Weights::profile("precision"), tol).unwrap();
        assert_eq!(p.note.as_deref(), Some("extended"));
        // cost → cheap basic wins despite the slight value error (candidate B)
        let c = score_and_pick(&req(), &cands, Weights::profile("cost"), tol).unwrap();
        assert_eq!(c.note.as_deref(), Some("basic"));
        // availability → the huge-stock basic part wins
        let a = score_and_pick(&req(), &cands, Weights::profile("availability"), tol).unwrap();
        assert_eq!(a.note.as_deref(), Some("basic"));
    }

    #[test]
    fn single_candidate_is_returned() {
        let cands = vec![cand(0.0, 0.01, 0.0, 1000, "basic")];
        assert!(score_and_pick(&req(), &cands, Weights::profile("balanced"), 0.02).is_some());
        assert!(score_and_pick(&req(), &[], Weights::profile("balanced"), 0.02).is_none());
    }

    #[test]
    fn parse_grade_specs() {
        // resistor tolerance + tempco
        assert!(close(
            parse_tolerance_pct("100mW 10kΩ Thin Film ±0.1% ±25ppm/℃").unwrap(),
            0.1
        ));
        assert!(close(
            parse_tolerance_pct("100mW 10kΩ Thick Film ±100ppm/℃ ±5%").unwrap(),
            5.0
        ));
        assert!(close(
            parse_tempco_ppm("Thin Film ±0.1% ±25ppm/℃", "resistor").unwrap(),
            25.0
        ));
        // asymmetric Y5V tolerance → worst leg
        assert!(close(parse_tolerance_pct("-20%~+80% 100nF 50V Y5V").unwrap(), 80.0));
        // capacitor dielectric → drift proxy (C0G ≪ X7R ≪ Y5V)
        let c0g = parse_tempco_ppm("10pF 50V C0G ±5%", "capacitor").unwrap();
        let x7r = parse_tempco_ppm("100nF 50V X7R ±10%", "capacitor").unwrap();
        let y5v = parse_tempco_ppm("-20%~+80% 100nF 50V Y5V", "capacitor").unwrap();
        assert!(c0g < x7r && x7r < y5v);
    }

    #[test]
    fn grade_profile_prefers_tight_low_drift() {
        // all exact value; differ only in grade.
        // A: ±5%, 100ppm (cheap jellybean).  B: ±0.1%, 25ppm (precision).
        let cands = vec![
            candg(0.0, 0.001, 5.0, 100.0, "thick"),
            candg(0.0, 0.02, 0.1, 25.0, "thin"),
        ];
        // grade → the tight-tolerance, low-drift part wins despite higher price
        let g = score_and_pick(&req(), &cands, Weights::profile("grade"), 0.02).unwrap();
        assert_eq!(g.note.as_deref(), Some("thin"));
        // cost → the cheap jellybean wins (grade ignored)
        let c = score_and_pick(&req(), &cands, Weights::profile("cost"), 0.02).unwrap();
        assert_eq!(c.note.as_deref(), Some("thick"));
    }

    #[test]
    fn max_tolerance_is_a_hard_gate() {
        // simulate the collect_feasible gate: a ±5% part must be excluded when
        // the requirement caps tolerance at 1%.
        let mut r = req();
        r.max_tolerance_pct = Some(1.0);
        let keep = |t: f64| match r.max_tolerance_pct {
            Some(m) => t <= m + 1e-9,
            None => true,
        };
        assert!(keep(0.1));
        assert!(keep(1.0));
        assert!(!keep(5.0));
    }
}
