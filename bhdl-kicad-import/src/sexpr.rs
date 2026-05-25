//! S-expression lexer + parser for KiCad 6+ files.
//!
//! KiCad's S-expression dialect is a clean subset: atoms (symbols,
//! strings, numbers) and parenthesised lists. We hand-roll the
//! parser rather than depending on a generic S-expr crate because:
//!
//! 1. KiCad's format is well-defined and stable enough that custom
//!    handling for its specific quirks (e.g. multi-line string
//!    literals, the occasional unquoted symbol that contains odd
//!    characters) is cleaner than fighting a generic library.
//! 2. ~200 lines, no transitive deps, easy to debug.
//! 3. Format evolves between KiCad versions; owning the parser
//!    means version-specific handling is straightforward.
//!
//! This module produces a generic [`Sexpr`] tree. Conversion to
//! KiCad-specific typed IR happens in `reader.rs`.

use std::fmt;

/// A parsed S-expression value.
///
/// KiCad-flavoured S-expressions have four atom kinds (symbols are
/// distinct from strings; integers are normalised to f64 because
/// KiCad doesn't distinguish them at the surface) plus lists.
#[derive(Debug, Clone, PartialEq)]
pub enum Sexpr {
    /// A bareword identifier: `kicad_sch`, `at`, `Device:R`, …
    Symbol(String),
    /// A quoted string literal: `"hello world"`, `"a \"quoted\" word"`.
    /// Escape sequences (`\"`, `\\`, `\n`) are decoded here.
    Str(String),
    /// A numeric literal: `100`, `-3.14`, `1e6`. Stored as f64
    /// since KiCad doesn't distinguish int/float on the wire.
    Num(f64),
    /// A parenthesised list: `(at 100 50 0)`, `(symbol …)`.
    List(Vec<Sexpr>),
}

impl Sexpr {
    /// If this is a list whose first element is the symbol `head`,
    /// return the rest of the list. Otherwise None. Convenience for
    /// pattern matching `(head arg1 arg2 ...)` shapes.
    pub fn match_list(&self, head: &str) -> Option<&[Sexpr]> {
        if let Sexpr::List(items) = self {
            if let Some(Sexpr::Symbol(s)) = items.first() {
                if s == head {
                    return Some(&items[1..]);
                }
            }
        }
        None
    }

    /// Borrow as a symbol, if it is one.
    pub fn as_symbol(&self) -> Option<&str> {
        if let Sexpr::Symbol(s) = self { Some(s.as_str()) } else { None }
    }

    /// Borrow as a string, if it is one.
    pub fn as_str(&self) -> Option<&str> {
        if let Sexpr::Str(s) = self { Some(s.as_str()) } else { None }
    }

    /// Borrow as a list, if it is one.
    pub fn as_list(&self) -> Option<&[Sexpr]> {
        if let Sexpr::List(items) = self { Some(items.as_slice()) } else { None }
    }

    /// Numeric value, accepting either a Num or a Symbol that happens
    /// to parse as a number. KiCad sometimes uses bareword numbers
    /// (e.g. `(version 20231120)`).
    pub fn as_num(&self) -> Option<f64> {
        match self {
            Sexpr::Num(n) => Some(*n),
            Sexpr::Symbol(s) => s.parse().ok(),
            _ => None,
        }
    }

    /// Best-effort textual rendering — primarily for diagnostics.
    pub fn fmt_pretty(&self, indent: usize) -> String {
        match self {
            Sexpr::Symbol(s) => s.clone(),
            Sexpr::Str(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
            Sexpr::Num(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    format!("{}", *n as i64)
                } else {
                    format!("{}", n)
                }
            }
            Sexpr::List(items) => {
                if items.is_empty() {
                    "()".to_string()
                } else if items.len() <= 4 && items.iter().all(|i| matches!(i, Sexpr::Symbol(_) | Sexpr::Num(_) | Sexpr::Str(_))) {
                    // Short lists on one line
                    let inner = items.iter().map(|i| i.fmt_pretty(indent + 2)).collect::<Vec<_>>().join(" ");
                    format!("({})", inner)
                } else {
                    let mut out = String::from("(");
                    let pad = " ".repeat(indent + 2);
                    for (i, item) in items.iter().enumerate() {
                        if i == 0 {
                            out.push_str(&item.fmt_pretty(indent + 2));
                        } else {
                            out.push('\n');
                            out.push_str(&pad);
                            out.push_str(&item.fmt_pretty(indent + 2));
                        }
                    }
                    out.push(')');
                    out
                }
            }
        }
    }
}

impl fmt::Display for Sexpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.fmt_pretty(0))
    }
}

/// Parsing errors. Keep variant set narrow; most real errors are one
/// of these three with a line/column for context.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("unexpected character {ch:?} at line {line}, column {col}")]
    UnexpectedChar { ch: char, line: usize, col: usize },

    #[error("unterminated string starting at line {line}, column {col}")]
    UnterminatedString { line: usize, col: usize },

    #[error("unbalanced parentheses at line {line}, column {col}: {what}")]
    UnbalancedParens { line: usize, col: usize, what: String },

    #[error("unexpected end of input (expected {expected})")]
    UnexpectedEof { expected: String },

    #[error("malformed number {raw:?} at line {line}, column {col}")]
    MalformedNumber { raw: String, line: usize, col: usize },
}

/// Parse a full KiCad-flavoured S-expression source string into a
/// single top-level Sexpr. The top of a `.kicad_sch` is always one
/// list `(kicad_sch ...)`, so the return is one Sexpr — typically a
/// `Sexpr::List`.
pub fn parse(input: &str) -> Result<Sexpr, ParseError> {
    let mut p = Parser::new(input);
    p.skip_whitespace_and_comments();
    let result = p.parse_value()?;
    p.skip_whitespace_and_comments();
    if !p.is_done() {
        return Err(ParseError::UnexpectedChar {
            ch: p.peek_char().unwrap_or('?'),
            line: p.line,
            col: p.col,
        });
    }
    Ok(result)
}

/// Internal recursive-descent parser. KiCad's S-expr is small enough
/// that hand-rolling is the right move.
struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
    line: usize,
    col: usize,
}

impl<'a> Parser<'a> {
    fn new(s: &'a str) -> Self {
        Self { src: s.as_bytes(), pos: 0, line: 1, col: 1 }
    }

    fn is_done(&self) -> bool { self.pos >= self.src.len() }

    fn peek_char(&self) -> Option<char> {
        self.src.get(self.pos).map(|&b| b as char)
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.peek_char()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    /// Skip ASCII whitespace + `;`-prefixed line comments (KiCad
    /// occasionally has them in custom libraries).
    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.peek_char() {
                Some(c) if c.is_ascii_whitespace() => { self.advance(); }
                Some(';') => {
                    while let Some(c) = self.advance() {
                        if c == '\n' { break; }
                    }
                }
                _ => break,
            }
        }
    }

    /// Top-level dispatch.
    fn parse_value(&mut self) -> Result<Sexpr, ParseError> {
        self.skip_whitespace_and_comments();
        match self.peek_char() {
            None => Err(ParseError::UnexpectedEof { expected: "a value".to_string() }),
            Some('(') => self.parse_list(),
            Some('"') => self.parse_string(),
            Some(c) if c == '-' || c == '+' || c.is_ascii_digit() => self.parse_number_or_symbol(),
            Some(c) if is_symbol_char(c) => self.parse_symbol(),
            Some(c) => Err(ParseError::UnexpectedChar {
                ch: c, line: self.line, col: self.col,
            }),
        }
    }

    fn parse_list(&mut self) -> Result<Sexpr, ParseError> {
        let start_line = self.line;
        let start_col = self.col;
        debug_assert_eq!(self.peek_char(), Some('('));
        self.advance(); // consume '('
        let mut items = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            match self.peek_char() {
                None => return Err(ParseError::UnbalancedParens {
                    line: start_line, col: start_col,
                    what: "missing closing ')'".to_string(),
                }),
                Some(')') => { self.advance(); return Ok(Sexpr::List(items)); }
                _ => { items.push(self.parse_value()?); }
            }
        }
    }

    fn parse_string(&mut self) -> Result<Sexpr, ParseError> {
        let start_line = self.line;
        let start_col = self.col;
        debug_assert_eq!(self.peek_char(), Some('"'));
        self.advance(); // consume opening '"'
        let mut out = String::new();
        loop {
            match self.advance() {
                None => return Err(ParseError::UnterminatedString {
                    line: start_line, col: start_col,
                }),
                Some('"') => return Ok(Sexpr::Str(out)),
                Some('\\') => {
                    // Escape sequence — KiCad uses backslash escapes
                    // for embedded quotes, backslashes, and the usual
                    // C-style control characters.
                    match self.advance() {
                        Some('"')  => out.push('"'),
                        Some('\\') => out.push('\\'),
                        Some('n')  => out.push('\n'),
                        Some('t')  => out.push('\t'),
                        Some('r')  => out.push('\r'),
                        Some(c)    => { out.push('\\'); out.push(c); }
                        None => return Err(ParseError::UnterminatedString {
                            line: start_line, col: start_col,
                        }),
                    }
                }
                Some(c) => out.push(c),
            }
        }
    }

    /// Numbers and symbols can be ambiguous on the leading byte
    /// (`-1.5` is a number; `-abc` would be a symbol if KiCad
    /// allowed it). We collect a bareword run and try parsing as
    /// number; on failure fall back to symbol.
    fn parse_number_or_symbol(&mut self) -> Result<Sexpr, ParseError> {
        let start_line = self.line;
        let start_col = self.col;
        let mut raw = String::new();
        while let Some(c) = self.peek_char() {
            if is_symbol_char(c) || c == '-' || c == '+' || c == '.' {
                raw.push(c);
                self.advance();
            } else {
                break;
            }
        }
        if raw.is_empty() {
            return Err(ParseError::UnexpectedEof { expected: "a number or symbol".to_string() });
        }
        // Try parsing as f64 first (handles "100", "-3.14", "1e6", etc.).
        if let Ok(n) = raw.parse::<f64>() {
            Ok(Sexpr::Num(n))
        } else if looks_like_number(&raw) {
            // It looked numeric (started with digit/sign and had digit
            // characters) but failed to parse — diagnose as malformed
            // number rather than silently treating as a symbol.
            Err(ParseError::MalformedNumber { raw, line: start_line, col: start_col })
        } else {
            Ok(Sexpr::Symbol(raw))
        }
    }

    fn parse_symbol(&mut self) -> Result<Sexpr, ParseError> {
        let mut out = String::new();
        while let Some(c) = self.peek_char() {
            if is_symbol_char(c) {
                out.push(c);
                self.advance();
            } else {
                break;
            }
        }
        if out.is_empty() {
            Err(ParseError::UnexpectedChar {
                ch: self.peek_char().unwrap_or('?'),
                line: self.line, col: self.col,
            })
        } else {
            Ok(Sexpr::Symbol(out))
        }
    }
}

/// Characters allowed inside a bareword (symbol). KiCad symbols
/// include colons (`Device:R`), slashes (`/path/to/sheet`), digits,
/// hyphens, dots, underscores, and various punctuation that real
/// schematic files use for property names.
fn is_symbol_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(c, '_' | '-' | '.' | '/' | ':' | '*' | '+' | '#' | '$' | '~' | '%' | '@' | '&' | '?' | '!' | '<' | '>' | '=' | '[' | ']' | '|')
}

/// Heuristic for distinguishing "tried to be a number" from "is a
/// symbol that happens to start with a digit / sign." A real
/// number has only digits, at most one decimal point, an optional
/// leading sign, and optionally a trailing `[eE][+-]?[digits]`
/// exponent. Internal hyphens (e.g. UUIDs `12345678-1234-...`),
/// alphabetic characters other than `eE` in the right position,
/// or any non-numeric punctuation makes it NOT a number — those
/// are symbol-like, and the parser should treat them as symbols
/// without raising a MalformedNumber error.
fn looks_like_number(s: &str) -> bool {
    let mut chars = s.chars().peekable();
    let first = chars.peek().copied();
    if !matches!(first, Some(c) if c.is_ascii_digit() || c == '-' || c == '+' || c == '.') {
        return false;
    }
    // Walk states: optional sign, integer part, optional fractional
    // part, optional exponent. Any deviation = not a number.
    let mut saw_digit = false;
    let mut saw_dot = false;
    let mut saw_exp = false;
    let mut saw_exp_sign = false;
    let mut at_start = true;
    for c in s.chars() {
        match c {
            '+' | '-' if at_start => { at_start = false; }
            '+' | '-' if saw_exp && !saw_exp_sign => { saw_exp_sign = true; }
            '+' | '-' => return false,
            '0'..='9' => { saw_digit = true; at_start = false; }
            '.' if !saw_dot && !saw_exp => { saw_dot = true; at_start = false; }
            'e' | 'E' if saw_digit && !saw_exp => { saw_exp = true; at_start = false; }
            _ => return false,
        }
    }
    saw_digit
}

// ─────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_atom() {
        assert_eq!(parse("hello").unwrap(), Sexpr::Symbol("hello".into()));
        assert_eq!(parse("42").unwrap(), Sexpr::Num(42.0));
        assert_eq!(parse("\"a string\"").unwrap(), Sexpr::Str("a string".into()));
    }

    #[test]
    fn parse_negative_and_floating_numbers() {
        assert_eq!(parse("-3.14").unwrap(), Sexpr::Num(-3.14));
        assert_eq!(parse("1e6").unwrap(), Sexpr::Num(1e6));
        assert_eq!(parse("-1.5e-3").unwrap(), Sexpr::Num(-1.5e-3));
    }

    #[test]
    fn parse_simple_list() {
        let s = parse("(at 100 50 0)").unwrap();
        assert_eq!(s, Sexpr::List(vec![
            Sexpr::Symbol("at".into()),
            Sexpr::Num(100.0),
            Sexpr::Num(50.0),
            Sexpr::Num(0.0),
        ]));
    }

    #[test]
    fn parse_nested_list() {
        let s = parse("(stroke (width 0.1) (type default))").unwrap();
        let items = s.as_list().expect("a list");
        assert_eq!(items[0].as_symbol(), Some("stroke"));
        assert_eq!(items[1].match_list("width").unwrap()[0].as_num(), Some(0.1));
        assert_eq!(items[2].match_list("type").unwrap()[0].as_symbol(), Some("default"));
    }

    #[test]
    fn parse_quoted_string_with_escapes() {
        let s = parse("\"hello \\\"world\\\"\\n\"").unwrap();
        assert_eq!(s, Sexpr::Str("hello \"world\"\n".into()));
    }

    #[test]
    fn parse_kicad_property_shape() {
        // Real KiCad property:
        // (property "Reference" "R1" (at 102 48 0))
        let src = r#"(property "Reference" "R1" (at 102 48 0))"#;
        let s = parse(src).unwrap();
        let items = s.as_list().expect("list");
        assert_eq!(items[0].as_symbol(), Some("property"));
        assert_eq!(items[1].as_str(), Some("Reference"));
        assert_eq!(items[2].as_str(), Some("R1"));
        let at = items[3].match_list("at").unwrap();
        assert_eq!(at.len(), 3);
        assert_eq!(at[0].as_num(), Some(102.0));
    }

    #[test]
    fn parse_kicad_symbol_with_lib_id_form() {
        // Symbols like `Device:R` are barewords with a colon.
        let s = parse("(lib_id Device:R)").unwrap();
        let items = s.as_list().expect("list");
        assert_eq!(items[0].as_symbol(), Some("lib_id"));
        assert_eq!(items[1].as_symbol(), Some("Device:R"));
    }

    #[test]
    fn parse_handles_whitespace_and_comments() {
        let src = r#"
            ; this is a comment line
            (kicad_sch
                (version 20231120)
                (paper "A4") ; inline comment
            )
        "#;
        let s = parse(src).unwrap();
        assert_eq!(s.match_list("kicad_sch").unwrap().len(), 2);
    }

    #[test]
    fn parse_balanced_paren_error() {
        let result = parse("(foo (bar baz");
        assert!(matches!(result, Err(ParseError::UnbalancedParens { .. })));
    }

    #[test]
    fn parse_unterminated_string_error() {
        let result = parse("\"oops");
        assert!(matches!(result, Err(ParseError::UnterminatedString { .. })));
    }

    #[test]
    fn parse_uuid_strings() {
        // KiCad UUIDs appear as bareword symbols, e.g.
        // `(uuid 12345678-1234-5678-1234-567812345678)`
        let s = parse("(uuid 12345678-1234-5678-1234-567812345678)").unwrap();
        let items = s.as_list().unwrap();
        assert_eq!(items[0].as_symbol(), Some("uuid"));
        assert_eq!(items[1].as_symbol(), Some("12345678-1234-5678-1234-567812345678"));
    }

    #[test]
    fn parse_full_minimal_kicad_sch() {
        // The smallest valid .kicad_sch shape we'd see in practice.
        let src = r#"
            (kicad_sch
                (version 20231120)
                (generator eeschema)
                (uuid 11111111-1111-1111-1111-111111111111)
                (paper "A4")
                (lib_symbols
                    (symbol "Device:R"
                        (pin passive line (at 0 0 90) (length 2)
                            (name "~" (effects (font (size 1 1))))
                            (number "1" (effects (font (size 1 1)))))
                    )
                )
                (symbol
                    (lib_id "Device:R")
                    (at 100 50 0)
                    (unit 1)
                    (uuid 22222222-2222-2222-2222-222222222222)
                    (property "Reference" "R1" (at 102 48 0))
                    (property "Value" "10k" (at 102 52 0))
                )
            )
        "#;
        let s = parse(src).unwrap();
        let top = s.match_list("kicad_sch").expect("top-level kicad_sch");
        // Find lib_symbols and the schematic-level symbol
        let lib_symbols = top.iter().find_map(|x| x.match_list("lib_symbols"));
        let _instance = top.iter().find(|x|
            matches!(x.match_list("symbol"), Some(args) if matches!(args.first(), Some(Sexpr::List(_))))
        );
        assert!(lib_symbols.is_some(), "should find lib_symbols");
    }
}
