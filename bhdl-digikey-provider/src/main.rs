//! BHDL supply-chain provider — DigiKey Product Information API v4.
//!
//! Turns BHDL's parametric part requirements into real, orderable DigiKey
//! parts (MPN + manufacturer + stock + price), and — crucially for the
//! Real-Data Policy — extracts the part's **real published ESR** for
//! electrolytic / tantalum / polymer capacitors so the sign-off's loop
//! stability and ripple analyses run on measured data instead of an estimate.
//!
//! ## What DigiKey actually publishes (measured live against the v4 API)
//! - **Electrolytic / tantalum / polymer caps:** `ESR (Equivalent Series
//!   Resistance)` is a real per-MPN parameter (`"15mOhm"`, `"3Ohm @ 100kHz"`).
//!   We parse it (and its test frequency when stated).
//! - **Ceramic caps (MLCC):** DigiKey carries **neither** ESR **nor**
//!   Dissipation Factor — the Parameters array has only Temperature
//!   Coefficient / Tolerance / Voltage / Capacitance. So ceramic ESR stays
//!   honestly **UNCHECKED** even with this provider (Real-Data Policy: no
//!   fabricated stand-in). The fix for ceramics is manufacturer impedance
//!   curves, not a guess.
//!
//! ## Protocol (JSON over stdin/stdout), matching `bhdl-analyzer` plugin.rs
//! Same input schema as the jlcparts provider:
//!   stdin:  {"requirements":[{"class_index":0,"class":"capacitor",
//!             "value":1.0e-4,"voltage_v":35.0,"package":"0805"} ...],
//!            "objective":"balanced","quantity":100}
//!   stdout: {"protocol_version":"1","selections":[{"class_index":0,
//!             "mpn":"...","manufacturer":"...","vendor":"DigiKey",
//!             "vendor_sku":"...","stock":98924,"unit_price":0.23,
//!             "currency":"USD","esr_ohms":0.015,"esr_test_freq_hz":100000.0,
//!             "note":"..."} ...],"warnings":[...]}
//!
//! `value` is SI base units (Ω / F / H). Credentials come from
//! `$DIGIKEY_CLIENT_ID` / `$DIGIKEY_CLIENT_SECRET`; the API base defaults to
//! production (`https://api.digikey.com`) and can be overridden with
//! `$DIGIKEY_API_BASE` (e.g. the sandbox host). HTTP/TLS is self-contained
//! (`ureq` + rustls) — no external `curl`/`fetch`, no system TLS library, no
//! async runtime; a single statically-linked executable like the jlcparts
//! provider.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::sync::OnceLock;

// ── Protocol input types (identical schema to the jlcparts provider) ─────

#[derive(Debug, Deserialize)]
struct Requirements {
    #[serde(default)]
    requirements: Vec<Requirement>,
    #[serde(default)]
    objective: Option<Objective>,
    #[serde(default)]
    quantity: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct Requirement {
    class_index: usize,
    class: String,
    #[serde(default)]
    value: Option<f64>,
    #[serde(default)]
    package: Option<String>,
    #[serde(default)]
    tolerance_pct: Option<f64>,
    #[serde(default)]
    max_tolerance_pct: Option<f64>,
    #[serde(default)]
    dielectric: Option<String>,
    #[serde(default)]
    current_a: Option<f64>,
    #[serde(default)]
    voltage_v: Option<f64>,
    #[serde(default)]
    power_w: Option<f64>,
    #[serde(default)]
    objective: Option<Objective>,
    #[serde(default)]
    quantity: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum Objective {
    Profile(String),
    Weights(Weights),
}

#[derive(Debug, Clone, Copy, Deserialize)]
struct Weights {
    #[serde(default)]
    value: f64,
    #[serde(default)]
    price: f64,
    #[serde(default)]
    stock: f64,
    #[serde(default)]
    tolerance: f64,
}

impl Weights {
    fn profile(name: &str) -> Weights {
        match name.trim().to_ascii_lowercase().as_str() {
            "precision" | "value" | "exact" => Weights { value: 1.0, price: 0.0, stock: 0.0, tolerance: 0.2 },
            "cost" | "cheap" | "price" => Weights { value: 0.2, price: 1.0, stock: 0.2, tolerance: 0.0 },
            "availability" | "stock" | "supply" => Weights { value: 0.3, price: 0.2, stock: 1.0, tolerance: 0.0 },
            _ => Weights { value: 0.5, price: 0.5, stock: 0.3, tolerance: 0.2 },
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

// ── Protocol output types ────────────────────────────────────────────────

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
    lead_time_weeks: Option<u32>,
    /// Real published ESR (ohms), per Real-Data Policy. Present only when the
    /// part actually publishes it (electrolytic/tantalum/polymer); absent for
    /// ceramics, whose ESR DigiKey does not carry.
    #[serde(skip_serializing_if = "Option::is_none")]
    esr_ohms: Option<f64>,
    /// Test frequency (Hz) at which the ESR is specified, when stated.
    #[serde(skip_serializing_if = "Option::is_none")]
    esr_test_freq_hz: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

// ── HTTP transport (ureq + rustls; self-contained, no external binary) ────

/// POST `body` to `url` with the given headers. Returns the response body
/// **regardless of HTTP status** — DigiKey returns JSON error bodies on 4xx
/// (e.g. `Invalid clientId`), and the caller's JSON layer interprets success
/// vs. error. Only a genuine transport failure (DNS/TLS/connection) is an Err.
fn http_post(url: &str, headers: &[(&str, &str)], body: &str) -> Result<String, String> {
    let mut req = ureq::post(url);
    for (k, v) in headers {
        req = req.set(k, v);
    }
    match req.send_string(body) {
        Ok(resp) => resp.into_string().map_err(|e| format!("read body: {e}")),
        // HTTP error status: still carries a (JSON) body we want to surface.
        Err(ureq::Error::Status(_code, resp)) => {
            resp.into_string().map_err(|e| format!("read error body: {e}"))
        }
        Err(ureq::Error::Transport(t)) => Err(format!("transport error: {t}")),
    }
}

/// DigiKey OAuth2 client-credentials token.
fn get_token(base: &str, client_id: &str, client_secret: &str) -> Result<String, String> {
    // x-www-form-urlencoded body. The values are opaque tokens (no reserved
    // chars in DigiKey ids/secrets), but encode defensively anyway.
    let body = format!(
        "grant_type=client_credentials&client_id={}&client_secret={}",
        urlencode(client_id),
        urlencode(client_secret)
    );
    let txt = http_post(
        &format!("{base}/v1/oauth2/token"),
        &[("content-type", "application/x-www-form-urlencoded")],
        &body,
    )?;
    let v: serde_json::Value =
        serde_json::from_str(&txt).map_err(|e| format!("token response not JSON: {e} — {txt}"))?;
    v.get("access_token")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            let msg = v.get("ErrorMessage").and_then(|m| m.as_str()).unwrap_or("unknown");
            let det = v.get("ErrorDetails").and_then(|m| m.as_str()).unwrap_or("");
            format!("no access_token (DigiKey: {msg} {det})")
        })
}

/// Minimal percent-encoding for form values.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// One KeywordSearch call → parsed JSON. `limit` caps returned products.
fn keyword_search(
    base: &str,
    token: &str,
    client_id: &str,
    keywords: &str,
    limit: u32,
) -> Result<serde_json::Value, String> {
    let body = serde_json::json!({ "Keywords": keywords, "Limit": limit }).to_string();
    let auth = format!("Bearer {token}");
    let txt = http_post(
        &format!("{base}/products/v4/search/keyword"),
        &[
            ("Authorization", auth.as_str()),
            ("X-DIGIKEY-Client-Id", client_id),
            ("content-type", "application/json"),
            ("accept", "application/json"),
            ("X-DIGIKEY-Locale-Site", "US"),
            ("X-DIGIKEY-Locale-Currency", "USD"),
        ],
        &body,
    )?;
    serde_json::from_str(&txt).map_err(|e| format!("search response not JSON: {e} — {}", &txt.chars().take(200).collect::<String>()))
}

// ── Parameter parsing ────────────────────────────────────────────────────

const PREFIX: &[(&str, f64)] = &[
    ("p", 1e-12), ("n", 1e-9), ("u", 1e-6), ("µ", 1e-6), ("μ", 1e-6),
    ("m", 1e-3), ("k", 1e3), ("K", 1e3), ("M", 1e6), ("G", 1e9),
];
fn prefix_mult(p: &str) -> f64 {
    if p.is_empty() { return 1.0; }
    PREFIX.iter().find(|(s, _)| *s == p).map(|(_, m)| *m).unwrap_or(1.0)
}

/// Flatten a product's `Parameters` array into text→text.
fn params_map(product: &serde_json::Value) -> HashMap<String, String> {
    let mut m = HashMap::new();
    if let Some(arr) = product.get("Parameters").and_then(|p| p.as_array()) {
        for pr in arr {
            if let (Some(k), Some(v)) = (
                pr.get("ParameterText").and_then(|x| x.as_str()),
                pr.get("ValueText").and_then(|x| x.as_str()),
            ) {
                m.insert(k.to_string(), v.to_string());
            }
        }
    }
    m
}

fn re_value() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"([0-9]+(?:\.[0-9]+)?)\s*([pnuµμmkKMG]?)").unwrap())
}

/// Parse a DigiKey value cell like `"100 µF"`, `"35 V"`, `"10 kOhms"` → SI
/// base float. `unit_letters` are the trailing unit chars to tolerate
/// (we match the leading number+prefix and ignore the unit text).
fn parse_si(text: &str) -> Option<f64> {
    let t = text.trim();
    if t == "-" || t.is_empty() { return None; }
    let c = re_value().captures(t)?;
    let num: f64 = c.get(1)?.as_str().parse().ok()?;
    let mult = prefix_mult(c.get(2).map(|m| m.as_str()).unwrap_or(""));
    let v = num * mult;
    if v > 0.0 { Some(v) } else { None }
}

/// Tolerance "±20%" / "±1%" → 20.0 / 1.0.
fn parse_tol_pct(text: &str) -> Option<f64> {
    static R: OnceLock<Regex> = OnceLock::new();
    let re = R.get_or_init(|| Regex::new(r"([0-9]+(?:\.[0-9]+)?)\s*%").unwrap());
    re.captures(text).and_then(|c| c[1].parse().ok())
}

/// ESR cell → (ohms, optional test-freq Hz). Handles `"15mOhm"`, `"3Ohm"`,
/// `"800mOhm"`, `"3Ohm @ 100kHz"`, `"-"` (→ None).
fn parse_esr(text: &str) -> Option<(f64, Option<f64>)> {
    let t = text.trim();
    if t == "-" || t.is_empty() { return None; }
    static R: OnceLock<Regex> = OnceLock::new();
    // number, optional 'm' (milli), 'Ohm', optional '@ <num><prefix>Hz'
    let re = R.get_or_init(|| {
        Regex::new(r"(?i)([0-9]+(?:\.[0-9]+)?)\s*(m)?\s*ohm(?:s)?(?:\s*@\s*([0-9]+(?:\.[0-9]+)?)\s*([pnuµμmkKMG]?)\s*hz)?").unwrap()
    });
    let c = re.captures(t)?;
    let num: f64 = c.get(1)?.as_str().parse().ok()?;
    let milli = c.get(2).map(|m| !m.as_str().is_empty()).unwrap_or(false);
    let ohms = if milli { num * 1e-3 } else { num };
    let freq = match (c.get(3), c.get(4)) {
        (Some(n), pfx) => n.as_str().parse::<f64>().ok().map(|f| f * prefix_mult(pfx.map(|m| m.as_str()).unwrap_or(""))),
        _ => None,
    };
    if ohms > 0.0 { Some((ohms, freq)) } else { None }
}

/// Does a part's dielectric (Temperature Coefficient cell) satisfy a required
/// one? C0G≡NP0. Case-insensitive substring.
fn dielectric_matches(want: &str, have: &str) -> bool {
    let w = want.trim().trim_matches('"').trim().to_ascii_uppercase();
    if w.is_empty() { return true; }
    let h = have.to_ascii_uppercase();
    let aliases: &[&str] = if w == "C0G" || w == "NP0" { &["C0G", "NP0"] } else { &[] };
    h.contains(&w) || aliases.iter().any(|a| h.contains(a))
}

// ── Class → keyword + DigiKey unit handling ──────────────────────────────

/// Human SI string for a value+unit, e.g. 100e-6 F → "100µF", 10000 Ω → "10kΩ".
fn human_si(value: f64, unit: char) -> String {
    let steps: &[(f64, &str)] = &[
        (1e9, "G"), (1e6, "M"), (1e3, "k"), (1.0, ""), (1e-3, "m"),
        (1e-6, "µ"), (1e-9, "n"), (1e-12, "p"),
    ];
    for (scale, pfx) in steps {
        if value >= *scale {
            let n = value / scale;
            // trim trailing .0
            let s = if (n - n.round()).abs() < 1e-9 { format!("{}", n.round() as i64) } else { format!("{n}") };
            return format!("{s}{pfx}{unit}");
        }
    }
    format!("{value}{unit}")
}

/// The DigiKey `Parameters` keys for a class's value + the SI unit char.
fn value_param_unit(class: &str) -> Option<(&'static str, char)> {
    match class {
        "resistor" => Some(("Resistance", 'Ω')),
        "capacitor" => Some(("Capacitance", 'F')),
        "inductor" => Some(("Inductance", 'H')),
        _ => None,
    }
}

/// Build the keyword string for a requirement.
fn build_keywords(req: &Requirement) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let (Some(v), Some((_, unit))) = (req.value, value_param_unit(&req.class)) {
        parts.push(human_si(v, unit));
    }
    if req.class == "capacitor" {
        if let Some(v) = req.voltage_v {
            // round up to a sensible rating hint
            parts.push(format!("{}V", v.ceil() as i64));
        }
        if let Some(d) = &req.dielectric {
            parts.push(d.trim_matches('"').to_string());
        }
    }
    if let Some(p) = &req.package {
        parts.push(p.clone());
    }
    parts.join(" ")
}

// ── Candidate + scoring ──────────────────────────────────────────────────

struct Cand {
    mpn: String,
    manufacturer: Option<String>,
    dk_part: Option<String>,
    unit_price: Option<f64>,
    stock: u64,
    lead_weeks: Option<u32>,
    value_err: f64,
    tol_pct: Option<f64>,
    esr_ohms: Option<f64>,
    esr_freq_hz: Option<f64>,
    pkg_confirmed: bool,
}

fn normalize(vals: &[f64]) -> Vec<f64> {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for &v in vals { lo = lo.min(v); hi = hi.max(v); }
    let span = hi - lo;
    if span <= f64::EPSILON { return vec![0.0; vals.len()]; }
    vals.iter().map(|&v| (v - lo) / span).collect()
}

fn score_and_pick(req: &Requirement, cands: &[Cand], w: Weights, tol: f64) -> Option<Selection> {
    if cands.is_empty() { return None; }
    let tol_eff = tol.max(1e-9);
    let ve: Vec<f64> = cands.iter().map(|c| (c.value_err / tol_eff).min(1.0)).collect();
    let max_price = cands.iter().filter_map(|c| c.unit_price).fold(0.0_f64, f64::max);
    let pr: Vec<f64> = cands.iter().map(|c| c.unit_price.unwrap_or(max_price)).collect();
    let st: Vec<f64> = cands.iter().map(|c| -(c.stock as f64)).collect();
    let worst_tol = cands.iter().filter_map(|c| c.tol_pct).fold(0.0_f64, f64::max);
    let tl: Vec<f64> = cands.iter().map(|c| c.tol_pct.unwrap_or(worst_tol)).collect();
    let pr_n = normalize(&pr);
    let st_n = normalize(&st);
    let tl_n = normalize(&tl);

    let mut best_i = 0;
    let mut best = f64::INFINITY;
    for i in 0..cands.len() {
        // soft package preference: unconfirmed footprint adds a small penalty
        let pkg_pen = if cands[i].pkg_confirmed { 0.0 } else { 0.25 };
        let score = w.value * ve[i] + w.price * pr_n[i] + w.stock * st_n[i] + w.tolerance * tl_n[i] + pkg_pen;
        let better = score < best - 1e-12
            || ((score - best).abs() <= 1e-12
                && (cands[i].value_err < cands[best_i].value_err
                    || (cands[i].value_err == cands[best_i].value_err && pr[i] < pr[best_i])));
        if better { best = score; best_i = i; }
    }
    let c = &cands[best_i];
    let mut note = String::new();
    if !c.pkg_confirmed && req.package.is_some() {
        note.push_str("footprint not confirmed; ");
    }
    note.push_str(match c.esr_ohms {
        Some(_) => "ESR from datasheet",
        None => "no published ESR",
    });
    Some(Selection {
        class_index: req.class_index,
        mpn: Some(c.mpn.clone()),
        manufacturer: c.manufacturer.clone(),
        vendor: Some("DigiKey".to_string()),
        vendor_sku: c.dk_part.clone(),
        stock: Some(c.stock),
        unit_price: c.unit_price,
        currency: Some("USD".to_string()),
        lead_time_weeks: c.lead_weeks,
        esr_ohms: c.esr_ohms,
        esr_test_freq_hz: c.esr_freq_hz,
        note: Some(note),
        error: None,
    })
}

// ── Resolution ───────────────────────────────────────────────────────────

fn resolve(
    base: &str,
    token: &str,
    client_id: &str,
    req: &Requirement,
    w: Weights,
    warnings: &mut Vec<String>,
) -> Selection {
    let (val_param, unit) = match value_param_unit(&req.class) {
        Some(x) => x,
        None => {
            return Selection {
                class_index: req.class_index,
                error: Some(format!("unsupported class '{}'", req.class)),
                ..Default::default()
            }
        }
    };
    let keywords = build_keywords(req);
    if keywords.trim().is_empty() {
        return Selection {
            class_index: req.class_index,
            error: Some("no value/parameters to search on".into()),
            ..Default::default()
        };
    }
    let resp = match keyword_search(base, token, client_id, &keywords, 50) {
        Ok(r) => r,
        Err(e) => {
            return Selection {
                class_index: req.class_index,
                error: Some(format!("DigiKey search failed: {e}")),
                ..Default::default()
            }
        }
    };
    let products = match resp.get("Products").and_then(|p| p.as_array()) {
        Some(p) => p,
        None => {
            return Selection {
                class_index: req.class_index,
                error: Some("DigiKey response had no Products".into()),
                ..Default::default()
            }
        }
    };

    let tol = req.tolerance_pct.unwrap_or(20.0) / 100.0;
    let want_pkg = req.package.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let _ = unit; // unit is implied by val_param

    let mut cands: Vec<Cand> = Vec::new();
    for p in products {
        // stock / lifecycle gate
        let stock = p.get("QuantityAvailable").and_then(|q| q.as_u64()).unwrap_or(0);
        if stock == 0 { continue; }
        if p.get("Discontinued").and_then(|b| b.as_bool()).unwrap_or(false) { continue; }
        if p.get("EndOfLife").and_then(|b| b.as_bool()).unwrap_or(false) { continue; }

        let pm = params_map(p);

        // hard gate: value within tolerance
        let mut value_err = 0.0;
        if let Some(target) = req.value {
            let pv = match pm.get(val_param).and_then(|s| parse_si(s)) {
                Some(v) => v,
                None => continue,
            };
            value_err = (pv - target).abs() / target.max(1e-30);
            if value_err > tol.max(1e-9) { continue; }
        }

        // hard gate: capacitor voltage rating ≥ derated operating volts
        if let Some(need) = req.voltage_v {
            match pm.get("Voltage - Rated").and_then(|s| parse_si(s)) {
                Some(r) if r + 1e-9 >= need => {}
                _ => continue,
            }
        }

        // hard gate: required dielectric
        if let Some(want) = req.dielectric.as_deref() {
            let have = pm.get("Temperature Coefficient").map(|s| s.as_str()).unwrap_or("");
            if !dielectric_matches(want, have) { continue; }
        }

        // hard gate: part tolerance grade
        let tol_pct = pm.get("Tolerance").and_then(|s| parse_tol_pct(s));
        if let Some(max_tol) = req.max_tolerance_pct {
            match tol_pct {
                Some(t) if t <= max_tol + 1e-9 => {}
                _ => continue,
            }
        }

        // ESR (real, when published — electrolytic/tantalum/polymer)
        let (esr_ohms, esr_freq_hz) = pm
            .get("ESR (Equivalent Series Resistance)")
            .and_then(|s| parse_esr(s))
            .map(|(o, f)| (Some(o), f))
            .unwrap_or((None, None));

        // package confirmation (soft)
        let pkg_confirmed = match want_pkg {
            Some(pk) => {
                let case = pm.get("Package / Case").map(|s| s.as_str()).unwrap_or("");
                let sz = pm.get("Size / Dimension").map(|s| s.as_str()).unwrap_or("");
                let mpn = p.get("ManufacturerProductNumber").and_then(|m| m.as_str()).unwrap_or("");
                let pk_l = pk.to_ascii_lowercase();
                case.to_ascii_lowercase().contains(&pk_l)
                    || sz.to_ascii_lowercase().contains(&pk_l)
                    || mpn.to_ascii_lowercase().contains(&pk_l)
            }
            None => true,
        };

        let mpn = match p.get("ManufacturerProductNumber").and_then(|m| m.as_str()) {
            Some(m) => m.to_string(),
            None => continue,
        };
        let manufacturer = p
            .get("Manufacturer")
            .and_then(|m| m.get("Name"))
            .and_then(|n| n.as_str())
            .map(|s| s.to_string());
        let unit_price = p.get("UnitPrice").and_then(|u| u.as_f64()).filter(|&x| x > 0.0);
        let lead_weeks = p.get("ManufacturerLeadWeeks").and_then(|l| l.as_str())
            .and_then(|s| Regex::new(r"\d+").ok().and_then(|re| re.find(s)).and_then(|m| m.as_str().parse().ok()));
        let dk_part = p.get("ProductVariations").and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|v0| v0.get("DigiKeyProductNumber"))
            .and_then(|d| d.as_str()).map(|s| s.to_string());

        cands.push(Cand {
            mpn, manufacturer, dk_part, unit_price, stock, lead_weeks,
            value_err, tol_pct, esr_ohms, esr_freq_hz, pkg_confirmed,
        });
    }

    match score_and_pick(req, &cands, w, tol) {
        Some(sel) => {
            if want_pkg.is_some() && sel.note.as_deref().map(|n| n.contains("footprint not confirmed")).unwrap_or(false) {
                warnings.push(format!(
                    "class_index {}: no in-stock {} confirmed package '{}'; selected on value (footprint not confirmed)",
                    req.class_index, req.class, want_pkg.unwrap_or("")
                ));
            }
            sel
        }
        None => Selection {
            class_index: req.class_index,
            error: Some(format!(
                "no in-stock DigiKey {} matched value/voltage/dielectric for '{}'",
                req.class, keywords
            )),
            ..Default::default()
        },
    }
}

// ── main ─────────────────────────────────────────────────────────────────

fn emit(resp: &Response) {
    println!("{}", serde_json::to_string(resp).unwrap());
}

fn fail(warnings: Vec<String>) {
    emit(&Response { protocol_version: "1".into(), selections: vec![], warnings });
}

fn main() {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        fail(vec!["failed to read requirements from stdin".into()]);
        return;
    }
    let reqs: Requirements = match serde_json::from_str(&input) {
        Ok(r) => r,
        Err(e) => { fail(vec![format!("malformed requirements JSON: {e}")]); return; }
    };

    let base = std::env::var("DIGIKEY_API_BASE").unwrap_or_else(|_| "https://api.digikey.com".to_string());
    let client_id = std::env::var("DIGIKEY_CLIENT_ID").unwrap_or_default();
    let client_secret = std::env::var("DIGIKEY_CLIENT_SECRET").unwrap_or_default();
    if client_id.is_empty() || client_secret.is_empty() {
        fail(vec!["DIGIKEY_CLIENT_ID / DIGIKEY_CLIENT_SECRET not set — cannot authenticate".into()]);
        return;
    }

    let token = match get_token(&base, &client_id, &client_secret) {
        Ok(t) => t,
        Err(e) => { fail(vec![format!("DigiKey auth failed: {e}")]); return; }
    };

    let default_weights = reqs.objective.as_ref().map(Objective::weights)
        .unwrap_or_else(|| Weights::profile("balanced"));

    let mut warnings = Vec::new();
    let mut selections = Vec::with_capacity(reqs.requirements.len());
    for req in &reqs.requirements {
        let w = req.objective.as_ref().map(Objective::weights).unwrap_or(default_weights);
        selections.push(resolve(&base, &token, &client_id, req, w, &mut warnings));
    }

    emit(&Response { protocol_version: "1".into(), selections, warnings });
}

// ── tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn esr_parsing() {
        assert_eq!(parse_esr("15mOhm"), Some((0.015, None)));
        assert_eq!(parse_esr("3Ohm"), Some((3.0, None)));
        assert_eq!(parse_esr("800mOhm"), Some((0.8, None)));
        let (o, f) = parse_esr("3Ohm @ 100kHz").unwrap();
        assert!((o - 3.0).abs() < 1e-12);
        assert!((f.unwrap() - 100_000.0).abs() < 1e-6);
        assert_eq!(parse_esr("-"), None);
        assert_eq!(parse_esr(""), None);
    }

    #[test]
    fn si_parsing() {
        assert!((parse_si("100 µF").unwrap() - 100e-6).abs() < 1e-15);
        assert!((parse_si("35 V").unwrap() - 35.0).abs() < 1e-12);
        assert!((parse_si("10 kOhms").unwrap() - 10_000.0).abs() < 1e-9);
        assert_eq!(parse_si("-"), None);
    }

    #[test]
    fn tol_and_dielectric() {
        assert_eq!(parse_tol_pct("±20%"), Some(20.0));
        assert_eq!(parse_tol_pct("±1%"), Some(1.0));
        assert!(dielectric_matches("C0G", "NP0"));
        assert!(dielectric_matches("X7R", "X7R"));
        assert!(!dielectric_matches("C0G", "X7R"));
    }

    #[test]
    fn human_si_fmt() {
        assert_eq!(human_si(100e-6, 'F'), "100µF");
        assert_eq!(human_si(4.7e-6, 'H'), "4.7µH");
        assert_eq!(human_si(10000.0, 'Ω'), "10kΩ");
        assert_eq!(human_si(100e-9, 'F'), "100nF");
    }

    #[test]
    fn keyword_build() {
        let req = Requirement {
            class_index: 0, class: "capacitor".into(), value: Some(100e-6),
            package: Some("Radial".into()), tolerance_pct: None, max_tolerance_pct: None,
            dielectric: None, current_a: None, voltage_v: Some(35.0), power_w: None,
            objective: None, quantity: None,
        };
        assert_eq!(build_keywords(&req), "100µF 35V Radial");
    }
}
