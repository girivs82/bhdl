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

/// Lowest-quantity unit price from the tiered `price` JSON array.
fn first_tier_price(price_json: &str) -> Option<f64> {
    let v: serde_json::Value = serde_json::from_str(price_json).ok()?;
    v.as_array()?
        .first()?
        .get("price")
        .and_then(|p| p.as_f64().or_else(|| p.as_str().and_then(|s| s.parse().ok())))
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
        // value-only (warn). When there's no requested package, only the
        // unconstrained pass applies.
        let modes: &[PkgMode] = match want_pkg {
            Some(_) => &[PkgMode::Strict, PkgMode::Token, PkgMode::Any],
            None => &[PkgMode::Any],
        };
        for mode in modes {
            if let Some(sel) = self.query_best(&ids, req, unit, re, tol, want_pkg, *mode) {
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

    fn query_best(
        &self,
        cat_ids: &[i64],
        req: &Requirement,
        unit: char,
        re: &Regex,
        tol: f64,
        pkg: Option<&str>,
        mode: PkgMode,
    ) -> Option<Selection> {
        let placeholders = cat_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let mut sql = format!(
            "SELECT mfr, manufacturer, basic, preferred, stock, price, description, lcsc \
             FROM v_components \
             WHERE category_id IN ({placeholders}) AND stock > 0"
        );
        match (mode, pkg) {
            (PkgMode::Strict, Some(_)) => sql.push_str(" AND lower(package) = lower(?)"),
            (PkgMode::Token, Some(_)) => {
                // size code as a substring of the package string or the MPN
                sql.push_str(" AND (lower(package) LIKE lower(?) OR lower(mfr) LIKE lower(?))")
            }
            _ => {}
        }
        // Best-ranked first: basic > preferred > most stock. We scan in this
        // order and take the first row whose parsed value matches, so the
        // result is the best-ranked value-matching part.
        sql.push_str(" ORDER BY basic DESC, preferred DESC, stock DESC");

        let mut stmt = self.conn.prepare(&sql).ok()?;
        // bind: category ids, then package params per mode
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

        let mut rows = stmt.query(params.as_slice()).ok()?;
        // Among value-matching candidates, prefer the *closest* value (so an
        // exact E96 121Ω wins over a high-stock 120Ω that is merely within
        // tolerance), with the SQL rank (basic > preferred > most stock) as
        // the tiebreak — rows arrive in rank order, so on an equal value
        // error the first-seen (better-ranked) candidate is kept. An exact
        // hit (err == 0) short-circuits the scan.
        let mut best: Option<(f64, Selection)> = None;
        while let Ok(Some(row)) = rows.next() {
            let description: String = row.get(6).ok()?;
            let mut err = 0.0;
            if let Some(target) = req.value {
                let v = match parse_value(re, unit, &description) {
                    Some(v) => v,
                    None => continue,
                };
                err = (v - target).abs() / target.max(1e-30);
                if err > tol.max(1e-9) {
                    continue;
                }
            }
            // strictly-better value error replaces; ties keep the earlier
            // (better-ranked) candidate already stored.
            if best.as_ref().map(|(be, _)| err < *be).unwrap_or(true) {
                let mfr: String = row.get(0).ok()?;
                let manufacturer: Option<String> = row.get(1).ok()?;
                let basic: i64 = row.get(2).ok()?;
                let preferred: i64 = row.get(3).ok()?;
                let stock: i64 = row.get(4).ok()?;
                let price: String = row.get(5).ok()?;
                let lcsc: i64 = row.get(7).ok()?;
                best = Some((
                    err,
                    Selection {
                        class_index: req.class_index,
                        mpn: Some(mfr),
                        manufacturer,
                        vendor: Some("LCSC".to_string()),
                        vendor_sku: Some(format!("C{lcsc}")),
                        stock: Some(stock.max(0) as u64),
                        unit_price: first_tier_price(&price),
                        currency: Some("USD".to_string()),
                        note: Some(if basic == 1 {
                            "basic".to_string()
                        } else if preferred == 1 {
                            "preferred".to_string()
                        } else {
                            "extended".to_string()
                        }),
                        error: None,
                    },
                ));
                if err == 0.0 {
                    break; // exact value at best rank — cannot improve
                }
            }
        }
        best.map(|(_, sel)| sel)
    }
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
        selections.push(cat.resolve(req, &re, &mut warnings));
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
}
