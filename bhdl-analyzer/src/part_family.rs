//! `part_family` constraint mini-parser (Phase 4b).
//!
//! Re-parses the tolerant token-stream body that
//! [`bhdl_ast::RequireClause`] collected during Phase 2 into a
//! structured [`Constraint`] that the catalog scan (Phase 4c) can
//! evaluate against a class's bound-generic tuple.
//!
//! Supported forms (covering the v0.2 catalog seed):
//!
//! * `axis in E_N(min, max)` — E-series membership with a value
//!   range. `N ∈ {12, 24, 48, 96, 192}`.
//! * `axis in { v1, v2, … }`  — enumerated set.
//!
//! Future forms (deferred):
//! * `axis <op> value`        — simple comparison (`>=`, `<=`, `==`).
//! * Boolean combinations     — `&&`, `||`, parens.
//!
//! Values inside the clause carry physical units. The parser accepts
//! the standard BHDL forms (`1Ω`, `10kΩ`, `100nF`, `5.0V`, …) and
//! lowers them to [`bhdl_common::ConstValue`] variants — Resistance,
//! Capacitance, Inductance, Voltage, Current — keyed off the unit
//! suffix.

use bhdl_ast::RequireClause;
use bhdl_common::ConstValue;

// ─────────────────────────────────────────────────────────────────
// Shapes
// ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ESeries {
    E12,
    E24,
    E48,
    E96,
    E192,
}

impl ESeries {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "E12" => Some(Self::E12),
            "E24" => Some(Self::E24),
            "E48" => Some(Self::E48),
            "E96" => Some(Self::E96),
            "E192" => Some(Self::E192),
            _ => None,
        }
    }
}

/// One constraint clause from a `part_family` body.
#[derive(Debug, Clone)]
pub enum Constraint {
    /// `axis in E_N(min, max)`
    InESeries {
        axis: String,
        series: ESeries,
        min: ConstValue,
        max: ConstValue,
    },
    /// `axis in { v1, v2, … }`
    InSet {
        axis: String,
        values: Vec<ConstValue>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstraintError {
    pub message: String,
}

impl ConstraintError {
    fn new(s: impl Into<String>) -> Self {
        Self { message: s.into() }
    }
}

// ─────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────

/// Re-parse a [`RequireClause`] into a [`Constraint`].
///
/// The clause AST already carries the tolerant token body as a
/// concatenated string (see [`RequireClause::body_text`]). We
/// tokenise that string here with a tiny ad-hoc tokeniser: it
/// handles the punctuation we use (`in`, `,`, `(`, `)`, `{`, `}`)
/// plus the value forms documented above.
pub fn parse_require_clause(clause: &RequireClause) -> Result<Constraint, ConstraintError> {
    let body = clause.body_text();
    parse_constraint_str(&body)
}

/// Same as [`parse_require_clause`] but operating on a free-form
/// string — useful for tests and for callers that have already
/// stringified the body.
pub fn parse_constraint_str(body: &str) -> Result<Constraint, ConstraintError> {
    let tokens = tokenize(body)?;
    let mut cursor = TokenCursor::new(&tokens);

    let axis = match cursor.next() {
        Some(Tok::Ident(s)) => s.clone(),
        _ => return Err(ConstraintError::new("expected axis name at start of clause")),
    };

    match cursor.next() {
        Some(Tok::Ident(s)) if s == "in" => {}
        other => {
            return Err(ConstraintError::new(format!(
                "expected `in` after axis name, found {:?}",
                other
            )))
        }
    }

    match cursor.peek() {
        Some(Tok::Ident(name)) if ESeries::parse(name).is_some() => {
            let series = ESeries::parse(name).unwrap();
            cursor.next(); // consume series ident
            cursor.expect(&Tok::LParen)?;
            let min = parse_value_tokens(&mut cursor)?;
            cursor.expect(&Tok::Comma)?;
            let max = parse_value_tokens(&mut cursor)?;
            cursor.expect(&Tok::RParen)?;
            Ok(Constraint::InESeries { axis, series, min, max })
        }
        Some(Tok::LBrace) => {
            cursor.next(); // consume {
            let mut values = Vec::new();
            loop {
                if let Some(Tok::RBrace) = cursor.peek() {
                    cursor.next();
                    break;
                }
                values.push(parse_value_tokens(&mut cursor)?);
                match cursor.peek() {
                    Some(Tok::Comma) => { cursor.next(); }
                    Some(Tok::RBrace) => { cursor.next(); break; }
                    other => {
                        return Err(ConstraintError::new(format!(
                            "expected `,` or `}}` in set, found {:?}",
                            other
                        )))
                    }
                }
            }
            if values.is_empty() {
                return Err(ConstraintError::new("enumerated set must not be empty"));
            }
            Ok(Constraint::InSet { axis, values })
        }
        other => Err(ConstraintError::new(format!(
            "expected E-series helper or `{{`, found {:?}",
            other
        ))),
    }
}

// ─────────────────────────────────────────────────────────────────
// Token cursor
// ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Number(String, String), // mantissa text, unit text (may be empty)
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
}

struct TokenCursor<'a> {
    toks: &'a [Tok],
    pos: usize,
}

impl<'a> TokenCursor<'a> {
    fn new(toks: &'a [Tok]) -> Self { Self { toks, pos: 0 } }
    fn peek(&self) -> Option<&Tok> { self.toks.get(self.pos) }
    fn next(&mut self) -> Option<&Tok> {
        let t = self.toks.get(self.pos);
        if t.is_some() { self.pos += 1; }
        t
    }
    fn expect(&mut self, expected: &Tok) -> Result<(), ConstraintError> {
        match self.next() {
            Some(t) if t == expected => Ok(()),
            other => Err(ConstraintError::new(format!(
                "expected {:?}, found {:?}", expected, other
            ))),
        }
    }
}

// ─────────────────────────────────────────────────────────────────
// Tokeniser
// ─────────────────────────────────────────────────────────────────

/// Hand-rolled tokeniser. The input is the body string between
/// `require` and `;`. Strips whitespace; splits identifiers from
/// number-with-unit literals from punctuation.
fn tokenize(body: &str) -> Result<Vec<Tok>, ConstraintError> {
    let mut out = Vec::new();
    let mut chars = body.chars().peekable();

    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' | '\n' | '\r' => { chars.next(); }
            '(' => { chars.next(); out.push(Tok::LParen); }
            ')' => { chars.next(); out.push(Tok::RParen); }
            '{' => { chars.next(); out.push(Tok::LBrace); }
            '}' => { chars.next(); out.push(Tok::RBrace); }
            ',' => { chars.next(); out.push(Tok::Comma); }
            d if d.is_ascii_digit() || d == '.' => {
                // Number with optional fractional part, then optional unit.
                let mut mantissa = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_digit() || c == '.' {
                        mantissa.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                let mut unit = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_alphabetic() || c == 'Ω' || c == 'µ' {
                        unit.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                out.push(Tok::Number(mantissa, unit));
            }
            a if a.is_alphabetic() || a == '_' => {
                let mut id = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        id.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                out.push(Tok::Ident(id));
            }
            other => {
                return Err(ConstraintError::new(format!(
                    "unexpected character {:?} in constraint body", other
                )))
            }
        }
    }
    Ok(out)
}

// ─────────────────────────────────────────────────────────────────
// Value parsing
// ─────────────────────────────────────────────────────────────────

fn parse_value_tokens(cursor: &mut TokenCursor) -> Result<ConstValue, ConstraintError> {
    match cursor.next() {
        Some(Tok::Number(mantissa, unit)) => parse_value(mantissa, unit),
        other => Err(ConstraintError::new(format!(
            "expected typed-literal value, found {:?}", other
        ))),
    }
}

/// Parse a number+unit pair into a [`ConstValue`] of the appropriate
/// physical-quantity variant. `unit` may be empty (then ConstValue::Float).
fn parse_value(mantissa: &str, unit: &str) -> Result<ConstValue, ConstraintError> {
    let m = mantissa.parse::<f64>().map_err(|e| ConstraintError::new(format!(
        "could not parse mantissa {:?}: {}", mantissa, e
    )))?;
    if unit.is_empty() {
        return Ok(ConstValue::Float(m));
    }

    // Split unit into SI prefix and base. Recognise the BHDL set:
    // k/K (kilo), M (mega), G (giga), m (milli), u/µ (micro), n
    // (nano), p (pico), f (femto). The remainder is the base unit.
    //
    // Ambiguity: 'm' can mean milli (in front of a base) OR be part
    // of the base symbol itself ("MΩ"). Disambiguate by checking
    // whether the remainder after stripping the first char is a
    // valid base. Same for 'M', 'G'.
    let (multiplier, base) = split_prefix(unit);
    let scaled = m * multiplier;

    match base {
        "Ω" => Ok(ConstValue::Resistance(scaled)),
        "F" => Ok(ConstValue::Capacitance(scaled)),
        "H" => Ok(ConstValue::Inductance(scaled)),
        "V" => Ok(ConstValue::Voltage(scaled)),
        "A" => Ok(ConstValue::Current(scaled)),
        "W" => Ok(ConstValue::Power(scaled)),
        "Hz" => Ok(ConstValue::Frequency(scaled)),
        "s" => Ok(ConstValue::Time(scaled)),
        "" => Ok(ConstValue::Float(scaled)),
        other => Err(ConstraintError::new(format!(
            "unknown unit base {:?} in literal {}{}", other, mantissa, unit
        ))),
    }
}

/// Split a unit-token text into (multiplier, base). Recognises BHDL's
/// SI prefix set. Returns `(1.0, unit)` if no prefix is present.
fn split_prefix(unit: &str) -> (f64, &str) {
    // Try each known prefix in order. The first match wins, except
    // we require the remaining text to be a recognised base unit so
    // we don't strip 'M' off "MΩ" mistakenly (M before Ω is mega,
    // not millis).
    let candidates: &[(&str, f64)] = &[
        ("G",  1.0e9),
        ("M",  1.0e6),
        ("k",  1.0e3),
        ("K",  1.0e3),   // 'K' as alias for kilo (Yageo/KiCad value strings often use it)
        ("m",  1.0e-3),
        ("µ",  1.0e-6),
        ("u",  1.0e-6),
        ("n",  1.0e-9),
        ("p",  1.0e-12),
        ("f",  1.0e-15),
    ];

    for (prefix, mult) in candidates {
        if let Some(rest) = unit.strip_prefix(prefix) {
            if is_known_base_unit(rest) && !rest.is_empty() {
                return (*mult, rest);
            }
        }
    }
    (1.0, unit)
}

fn is_known_base_unit(s: &str) -> bool {
    matches!(s, "Ω" | "F" | "H" | "V" | "A" | "W" | "Hz" | "s")
}

// ─────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn r(v: ConstValue) -> f64 {
        match v {
            ConstValue::Resistance(x) => x,
            ConstValue::Capacitance(x) => x,
            ConstValue::Voltage(x) => x,
            _ => panic!("not a scalar physical quantity"),
        }
    }

    #[test]
    fn e_series_with_range() {
        let c = parse_constraint_str("R in E96(1Ω, 10MΩ)").unwrap();
        match c {
            Constraint::InESeries { axis, series, min, max } => {
                assert_eq!(axis, "R");
                assert_eq!(series, ESeries::E96);
                assert!((r(min) - 1.0).abs() < 1e-9);
                assert!((r(max) - 10_000_000.0).abs() < 1e-3);
            }
            _ => panic!("wrong shape"),
        }
    }

    #[test]
    fn enumerated_voltages() {
        let c = parse_constraint_str("V_OUT in { 1.5V, 1.8V, 2.5V, 3.3V, 5.0V }").unwrap();
        match c {
            Constraint::InSet { axis, values } => {
                assert_eq!(axis, "V_OUT");
                assert_eq!(values.len(), 5);
                let voltages: Vec<f64> = values.iter().map(|v| match v {
                    ConstValue::Voltage(x) => *x,
                    _ => panic!("expected voltage"),
                }).collect();
                assert!((voltages[0] - 1.5).abs() < 1e-9);
                assert!((voltages[4] - 5.0).abs() < 1e-9);
            }
            _ => panic!("wrong shape"),
        }
    }

    #[test]
    fn capacitance_e12() {
        let c = parse_constraint_str("C in E12(1nF, 1µF)").unwrap();
        match c {
            Constraint::InESeries { series, min, max, .. } => {
                assert_eq!(series, ESeries::E12);
                assert!((r(min) - 1.0e-9).abs() < 1e-15);
                assert!((r(max) - 1.0e-6).abs() < 1e-12);
            }
            _ => panic!("wrong shape"),
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_constraint_str("garbage").is_err());
        assert!(parse_constraint_str("R in unknown(1Ω, 2Ω)").is_err());
        assert!(parse_constraint_str("R in E96 1Ω").is_err());
    }
}
