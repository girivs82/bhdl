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

// ─────────────────────────────────────────────────────────────────
// §4c — Catalog matching engine
// ─────────────────────────────────────────────────────────────────
//
// Once mono produces concrete class instances (e.g. `Resistor<10kΩ,
// 1%, "0603">`), the matching engine finds candidate `part_family`
// declarations that can supply orderable MPNs for that class. The
// engine has three steps:
//
//   1. Filter families by entity name (`Resistor` only matches the
//      `Resistor<...>` class, not `Capacitor<...>`).
//   2. Walk the family's class-pattern positionally against the
//      class instance's bound generics:
//        - Literal pattern elements (`"1%"`, `"0603"`) must equal
//          the class generic at that position.
//        - Wildcard `*` matches any value and produces no binding.
//        - Named wildcard `R: *` matches any value and binds the
//          name (`R`) to it.
//   3. Evaluate the family's `require` constraints against the
//      bindings; reject any family whose constraints don't hold.

use std::collections::HashMap;
use bhdl_ast::ClassPattern;

/// A monomorphised class — entity name plus bound generic values
/// in declaration order. Phase 4d / Phase 5 will lift these out of
/// the analyzer's mono pass; for Phase 4c we accept hand-built
/// instances to exercise the matcher in isolation.
#[derive(Debug, Clone)]
pub struct ClassInstance {
    pub entity: String,
    pub generics: Vec<ConstValue>,
}

/// A successful match — the family name plus the bindings the
/// matcher established for named wildcards. The catalog scan
/// returns one of these per (class, candidate family) pair, then
/// the template engine renders the MPN against the bindings.
#[derive(Debug, Clone)]
pub struct MatchResult {
    pub family: String,
    pub bindings: HashMap<String, ConstValue>,
}

/// Element of a class pattern — what we expect at one positional
/// slot in the `<...>` block.
#[derive(Debug, Clone)]
pub enum PatternElement {
    /// A literal value: `"1%"`, `"0603"`, `3.3V`, `50V`, `12`.
    Literal(ConstValue),
    /// Unnamed wildcard `*` — matches anything, binds nothing.
    Wildcard,
    /// Named wildcard `IDENT: *` — matches anything, binds IDENT.
    NamedWildcard(String),
}

/// Parse the text content of a CLASS_PATTERN's TYPE_ARGS into a
/// vector of pattern elements. Returns an empty vector if the
/// class pattern has no `<...>` block (family-of-one case).
pub fn pattern_elements(pattern: &ClassPattern) -> Vec<PatternElement> {
    let Some(args) = pattern.type_args() else { return Vec::new(); };
    use rowan::ast::AstNode;
    let raw = args.syntax().text().to_string();
    // Strip surrounding `<` and `>`.
    let inner = raw.trim();
    let inner = inner.strip_prefix('<').unwrap_or(inner);
    let inner = inner.strip_suffix('>').unwrap_or(inner);

    let mut elements = Vec::new();
    for piece in split_top_level_commas(inner) {
        elements.push(parse_pattern_element(&piece));
    }
    elements
}

/// Split a string on commas at depth 0 (ignoring commas inside
/// nested brackets/parens/braces).
fn split_top_level_commas(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut depth = 0i32;
    for c in s.chars() {
        match c {
            '(' | '[' | '{' | '<' => { depth += 1; buf.push(c); }
            ')' | ']' | '}' | '>' => { depth -= 1; buf.push(c); }
            ',' if depth == 0 => {
                out.push(buf.trim().to_string());
                buf.clear();
            }
            _ => buf.push(c),
        }
    }
    if !buf.trim().is_empty() {
        out.push(buf.trim().to_string());
    }
    out
}

/// Parse a single pattern-element text into a [`PatternElement`].
/// Recognised forms:
///   *               → Wildcard
///   IDENT : *       → NamedWildcard(IDENT)
///   "..."           → Literal::String
///   number+unit     → Literal::<physical-quantity>
fn parse_pattern_element(text: &str) -> PatternElement {
    let t = text.trim();
    if t == "*" {
        return PatternElement::Wildcard;
    }
    // Named wildcard `R: *` (allow whitespace before the `:`).
    if let Some(colon_idx) = t.find(':') {
        let lhs = t[..colon_idx].trim();
        let rhs = t[colon_idx + 1..].trim();
        if rhs == "*"
            && !lhs.is_empty()
            && lhs.chars().all(|c| c.is_alphanumeric() || c == '_')
            && lhs.chars().next().map_or(false, |c| c.is_alphabetic() || c == '_')
        {
            return PatternElement::NamedWildcard(lhs.to_string());
        }
    }
    // String literal: `"..."`
    if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 {
        let inner = &t[1..t.len() - 1];
        return PatternElement::Literal(ConstValue::String(inner.to_string()));
    }
    // Number + optional unit (re-use the constraint tokenizer's value parser).
    if let Some((mantissa, unit)) = split_number_unit(t) {
        if let Ok(v) = parse_value(&mantissa, &unit) {
            return PatternElement::Literal(v);
        }
    }
    // Unrecognised — fall back to string literal of the raw text.
    PatternElement::Literal(ConstValue::String(t.to_string()))
}

/// Split a `10kΩ` / `100nF` / `3.3V` / `5%` / `12` literal into
/// (mantissa, unit). Returns None if the leading characters
/// don't parse as a number.
fn split_number_unit(s: &str) -> Option<(String, String)> {
    let mut mantissa = String::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() || c == '.' {
            mantissa.push(c);
            chars.next();
        } else {
            break;
        }
    }
    if mantissa.is_empty() { return None; }
    let unit: String = chars.collect();
    Some((mantissa, unit.trim().to_string()))
}

/// Match a class instance against a part_family's class pattern +
/// require constraints. Returns the family-binding map if the
/// match succeeds. Phase 5 will use the bindings to render
/// `mpn_template`.
///
/// `family_name` is the name token of the part_family declaration;
/// it's threaded through to the MatchResult unchanged.
pub fn match_class(
    family_name: &str,
    pattern: &ClassPattern,
    constraints: &[Constraint],
    class: &ClassInstance,
) -> Option<MatchResult> {
    // Step 1: entity name must agree.
    let entity = pattern.entity_name()?;
    if entity != class.entity {
        return None;
    }

    // Step 2: walk pattern elements positionally.
    let pattern_elems = pattern_elements(pattern);
    let mut bindings: HashMap<String, ConstValue> = HashMap::new();

    if !pattern_elems.is_empty() {
        if pattern_elems.len() != class.generics.len() {
            // Arity mismatch — fail fast.
            return None;
        }
        for (elem, value) in pattern_elems.iter().zip(class.generics.iter()) {
            match elem {
                PatternElement::Literal(lit) => {
                    if !values_compatible(lit, value) {
                        return None;
                    }
                }
                PatternElement::Wildcard => { /* matches anything */ }
                PatternElement::NamedWildcard(name) => {
                    bindings.insert(name.clone(), value.clone());
                }
            }
        }
    }

    // Step 3: evaluate each constraint against the bindings.
    for c in constraints {
        if !evaluate_constraint(c, &bindings) {
            return None;
        }
    }

    Some(MatchResult {
        family: family_name.to_string(),
        bindings,
    })
}

/// Are two ConstValues "equal enough" for a literal pattern match?
/// Same dimensional variant and floats within epsilon; or same
/// string value; etc.
fn values_compatible(pattern: &ConstValue, value: &ConstValue) -> bool {
    match (pattern, value) {
        (ConstValue::String(a), ConstValue::String(b)) => a == b,
        (ConstValue::Float(a), ConstValue::Float(b)) => (a - b).abs() < 1e-12 * a.abs().max(1.0),
        (ConstValue::Integer(a), ConstValue::Integer(b)) => a == b,
        (ConstValue::Voltage(a), ConstValue::Voltage(b)) => float_eq(*a, *b),
        (ConstValue::Current(a), ConstValue::Current(b)) => float_eq(*a, *b),
        (ConstValue::Resistance(a), ConstValue::Resistance(b)) => float_eq(*a, *b),
        (ConstValue::Capacitance(a), ConstValue::Capacitance(b)) => float_eq(*a, *b),
        (ConstValue::Inductance(a), ConstValue::Inductance(b)) => float_eq(*a, *b),
        // Pattern is a percentage written as `5%` → ConstValue::Float(0.05).
        // Value (from emitter) is also a percentage → Float. Already covered.
        _ => false,
    }
}

fn float_eq(a: f64, b: f64) -> bool {
    let scale = a.abs().max(b.abs()).max(1.0);
    (a - b).abs() < 1e-9 * scale
}

/// Evaluate a single constraint against the matched bindings.
fn evaluate_constraint(c: &Constraint, bindings: &HashMap<String, ConstValue>) -> bool {
    match c {
        Constraint::InESeries { axis, series: _, min, max } => {
            let Some(v) = bindings.get(axis) else { return false; };
            // Range check (E-series membership exact check is deferred
            // to Phase 4d/5 — for now we accept anything in the range).
            value_le(min, v) && value_le(v, max)
        }
        Constraint::InSet { axis, values } => {
            let Some(v) = bindings.get(axis) else { return false; };
            values.iter().any(|cand| values_compatible(cand, v))
        }
    }
}

/// `a <= b` over physical quantities of the same dimensional variant.
/// Mixed-variant comparisons return false.
fn value_le(a: &ConstValue, b: &ConstValue) -> bool {
    match (a, b) {
        (ConstValue::Float(a), ConstValue::Float(b)) => a <= b,
        (ConstValue::Integer(a), ConstValue::Integer(b)) => a <= b,
        (ConstValue::Voltage(a), ConstValue::Voltage(b)) => a <= b,
        (ConstValue::Current(a), ConstValue::Current(b)) => a <= b,
        (ConstValue::Resistance(a), ConstValue::Resistance(b)) => a <= b,
        (ConstValue::Capacitance(a), ConstValue::Capacitance(b)) => a <= b,
        (ConstValue::Inductance(a), ConstValue::Inductance(b)) => a <= b,
        _ => false,
    }
}

#[cfg(test)]
mod matcher_tests {
    use super::*;
    use bhdl_ast::{SourceFile, HasName};
    use bhdl_parser::parse;
    use rowan::ast::AstNode;
    use std::fs;

    /// Load a stdlib part-family file and return its first
    /// PartFamilyDef plus the parsed constraint list. Test helper.
    fn load_family(path: &str) -> (String, bhdl_ast::ClassPattern, Vec<Constraint>) {
        let content = fs::read_to_string(path).expect("read");
        let parse_result = parse(&content);
        assert!(parse_result.errors().is_empty(), "parse errors in {}", path);
        let source = SourceFile::cast(parse_result.syntax()).expect("source file");
        let pf = source
            .items()
            .find_map(|i| if let bhdl_ast::Item::PartFamilyDef(p) = i { Some(p) } else { None })
            .expect("part_family in file");
        let name = pf.name().map(|t| t.text().to_string()).expect("name");
        let pattern = pf.class_pattern().expect("class pattern");
        let constraints: Vec<Constraint> = pf
            .require_clauses()
            .filter_map(|c| parse_require_clause(&c).ok())
            .collect();
        (name, pattern, constraints)
    }

    #[test]
    fn yageo_matches_10k_1pct_0603() {
        let (name, pat, cons) = load_family("../bhdl-stdlib/parts/yageo/rc0603fr.bhdl");
        let class = ClassInstance {
            entity: "Resistor".to_string(),
            generics: vec![
                ConstValue::Resistance(10_000.0),
                ConstValue::String("1%".to_string()),
                ConstValue::String("0603".to_string()),
            ],
        };
        let m = match_class(&name, &pat, &cons, &class)
            .expect("Yageo RC0603FR should match 10kΩ/1%/0603");
        assert_eq!(m.family, "Yageo_RC0603FR_07");
        // R should be bound to 10k.
        let r = m.bindings.get("R").expect("R binding");
        assert!(matches!(r, ConstValue::Resistance(v) if (v - 10_000.0).abs() < 1.0));
    }

    #[test]
    fn yageo_rejects_5pct_tolerance() {
        let (name, pat, cons) = load_family("../bhdl-stdlib/parts/yageo/rc0603fr.bhdl");
        let class = ClassInstance {
            entity: "Resistor".to_string(),
            generics: vec![
                ConstValue::Resistance(10_000.0),
                ConstValue::String("5%".to_string()),  // Yageo family is 1% only
                ConstValue::String("0603".to_string()),
            ],
        };
        assert!(match_class(&name, &pat, &cons, &class).is_none());
    }

    #[test]
    fn yageo_rejects_out_of_range() {
        let (name, pat, cons) = load_family("../bhdl-stdlib/parts/yageo/rc0603fr.bhdl");
        let class = ClassInstance {
            entity: "Resistor".to_string(),
            generics: vec![
                ConstValue::Resistance(0.5),  // < 1Ω lower bound of E96 range
                ConstValue::String("1%".to_string()),
                ConstValue::String("0603".to_string()),
            ],
        };
        assert!(match_class(&name, &pat, &cons, &class).is_none());
    }

    #[test]
    fn ap2112k_voltage_enum() {
        let (name, pat, cons) = load_family("../bhdl-stdlib/parts/diodes/ap2112k.bhdl");
        // 3.3V is in the enum — should match.
        let class = ClassInstance {
            entity: "AP2112K".to_string(),
            generics: vec![ConstValue::Voltage(3.3)],
        };
        let m = match_class(&name, &pat, &cons, &class).expect("3.3V should match");
        assert_eq!(m.family, "Diodes_AP2112K");
        // 4.0V is NOT in the enum — should reject.
        let class_bad = ClassInstance {
            entity: "AP2112K".to_string(),
            generics: vec![ConstValue::Voltage(4.0)],
        };
        assert!(match_class(&name, &pat, &cons, &class_bad).is_none());
    }

    #[test]
    fn lm317_family_of_one_matches_bare_instance() {
        let (name, pat, cons) = load_family("../bhdl-stdlib/parts/ti/lm317.bhdl");
        let class = ClassInstance {
            entity: "LM317".to_string(),
            generics: vec![],
        };
        let m = match_class(&name, &pat, &cons, &class).expect("LM317 bare should match");
        assert_eq!(m.family, "TI_LM317T");
        assert!(m.bindings.is_empty());
    }

    #[test]
    fn entity_name_mismatch_rejects() {
        let (name, pat, cons) = load_family("../bhdl-stdlib/parts/ti/lm317.bhdl");
        let class = ClassInstance {
            entity: "Resistor".to_string(),
            generics: vec![],
        };
        assert!(match_class(&name, &pat, &cons, &class).is_none());
    }
}
