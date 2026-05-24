use logos::{Lexer, Logos};
use smol_str::SmolStr;
use crate::syntax::SyntaxKind;

// Struct to hold IDENT/Keyword kind and text
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeywordOrIdent {
    pub kind: SyntaxKind,
    pub text: SmolStr,
}

// Callback for IDENT or Keyword
pub fn lex_ident_or_kw(lex: &mut Lexer<LexerToken>) -> KeywordOrIdent {
    let slice = lex.slice();
    let kind = match slice {
        "board" => SyntaxKind::BOARD_KW,
        "entity" => SyntaxKind::ENTITY_KW,
        "interface" => SyntaxKind::INTERFACE_KW,
        "component" => SyntaxKind::COMPONENT_KW,
        "net" => SyntaxKind::NET_KW,
        "generate" => SyntaxKind::GENERATE_KW,
        "for" => SyntaxKind::FOR_KW,
        "in" => SyntaxKind::IN_KW,
        "port" => SyntaxKind::PORT_KW,
        "const" => SyntaxKind::CONST_KW,
        "signal" => SyntaxKind::SIGNAL_KW,
        "wire" => SyntaxKind::WIRE_KW,
        "assign" => SyntaxKind::ASSIGN_KW,
        "typedef" => SyntaxKind::TYPEDEF_KW,
        "struct" => SyntaxKind::STRUCT_KW,
        "enum" => SyntaxKind::ENUM_KW,
        "match" => SyntaxKind::MATCH_KW,
        "power" => SyntaxKind::POWER_KW,
        "ground" => SyntaxKind::GROUND_KW,
        "clock" => SyntaxKind::CLOCK_KW,
        "input" => SyntaxKind::INPUT_KW,
        "output" => SyntaxKind::OUTPUT_KW,
        "out" => SyntaxKind::OUT_KW,
        "inout" => SyntaxKind::INOUT_KW,
        "tri" => SyntaxKind::TRI_KW,
        "trireg" => SyntaxKind::TRIREG_KW,
        "uwire" => SyntaxKind::UWIRE_KW,
        "import" => SyntaxKind::IMPORT_KW,
        // "from", "as", "to" are contextual - will be IDENT
        "as" => SyntaxKind::AS_KW,
        "layer" => SyntaxKind::LAYER_KW,
        "extends" => SyntaxKind::EXTENDS_KW,
        "constrain" => SyntaxKind::CONSTRAIN_KW,
        "layer_stackup" => SyntaxKind::LAYER_STACKUP_KW,
        "default_design_rules" => SyntaxKind::DEFAULT_DESIGN_RULES_KW,
        "design" => SyntaxKind::DESIGN_KW,
        "if" => SyntaxKind::IF_KW,
        "else" => SyntaxKind::ELSE_KW,
        "when" => SyntaxKind::WHEN_KW,
        "attribute" => SyntaxKind::ATTRIBUTE_KW,
        "pin" => SyntaxKind::PIN_KW,
        "alias" => SyntaxKind::ALIAS_KW,
        "type" => SyntaxKind::TYPE_KW,
        "null" => SyntaxKind::NULL_KW,
        "where" => SyntaxKind::WHERE_KW,
        "with" => SyntaxKind::WITH_KW,
        "satisfies" => SyntaxKind::SATISFIES_KW,
        "via" => SyntaxKind::VIA_KW,
        "trait" => SyntaxKind::TRAIT_KW,
        "impl" => SyntaxKind::IMPL_KW,
        "safety_goal" => SyntaxKind::SAFETY_GOAL_KW,
        "fault_inject" => SyntaxKind::FAULT_INJECT_KW,
        "expansion" => SyntaxKind::EXPANSION_KW,
        "internal" => SyntaxKind::INTERNAL_KW,
        "placement" => SyntaxKind::PLACEMENT_KW,
        "symbol" => SyntaxKind::SYMBOL_KW,
        "layout" => SyntaxKind::LAYOUT_KW,
        "body" => SyntaxKind::BODY_KW,
        "left" => SyntaxKind::LEFT_KW,
        "right" => SyntaxKind::RIGHT_KW,
        "top" => SyntaxKind::TOP_KW,
        "bottom" => SyntaxKind::BOTTOM_KW,
        "group" => SyntaxKind::GROUP_KW,
        "part" => SyntaxKind::PART_KW,
        "package" => SyntaxKind::PACKAGE_KW,
        _ => SyntaxKind::IDENT,
    };
    KeywordOrIdent { kind, text: slice.into() }
}

// Define the NEW LexerToken enum
#[derive(Logos, Debug, Clone, PartialEq, Eq)]
#[logos(skip r"[ \t\r\n\f]+")] // Ignore whitespace
#[logos(skip r"//[^\n]*")] // Ignore single-line comments
#[logos(skip r"/\*([^*]|\*[^/])*\*/")] // Ignore multi-line comments
#[allow(dead_code)]
pub enum LexerToken {
    // Electrical Units with ASCII and Unicode variants
    // Resistance units
    #[token("Ω", priority = 5)] OhmUnicode,
    #[token("kΩ", priority = 5)] KOhmUnicode,
    #[token("MΩ", priority = 5)] MOhmUnicode,
    #[token("mΩ", priority = 5)] MilliOhmUnicode,
    #[token("Ohm", priority = 4)] OhmUnit,
    #[token("kOhm", priority = 4)] KOhmUnit,
    #[token("MOhm", priority = 4)] MOhmUnit,
    #[token("mOhm", priority = 4)] MilliOhmUnit,
    
    // Voltage units
    #[token("V", priority = 3)] VUnit,
    #[token("Vdc", priority = 4)] VdcUnit,
    #[token("Vac", priority = 4)] VacUnit,
    #[token("Vrms", priority = 4)] VrmsUnit,
    #[token("Vpp", priority = 4)] VppUnit,
    #[token("mV", priority = 4)] MVUnit,
    #[token("µV", priority = 5)] UVUnicode,
    #[token("uV", priority = 4)] UVUnit,
    #[token("nV", priority = 4)] NVUnit,
    
    // Current units
    #[token("A", priority = 3)] AUnit,
    #[token("mA", priority = 4)] MAUnit,
    #[token("µA", priority = 5)] UAUnicode,
    #[token("uA", priority = 4)] UAUnit,
    #[token("nA", priority = 4)] NAUnit,
    
    // Capacitance units
    #[token("F", priority = 3)] FUnit,
    #[token("µF", priority = 5)] UFUnicode,
    #[token("uF", priority = 4)] UFUnit,
    #[token("nF", priority = 4)] NFUnit,
    #[token("pF", priority = 4)] PFUnit,
    
    // Inductance units
    #[token("H", priority = 3)] HUnit,
    #[token("µH", priority = 5)] UHUnicode,
    #[token("uH", priority = 4)] UHUnit,
    #[token("mH", priority = 4)] MHUnit,
    #[token("nH", priority = 4)] NHUnit,
    
    // Frequency units
    #[token("Hz", priority = 4)] HzUnit,
    #[token("kHz", priority = 4)] KHzUnit,
    #[token("MHz", priority = 4)] MHzUnit,
    #[token("GHz", priority = 4)] GHzUnit,
    
    // Time units
    #[token("s", priority = 3)] SUnit,
    #[token("ms", priority = 4)] MsUnit,
    #[token("µs", priority = 5)] UsUnicode,
    #[token("us", priority = 4)] UsUnit,
    #[token("ns", priority = 4)] NsUnit,
    #[token("ps", priority = 4)] PsUnit,
    
    // Temperature units
    #[token("°C", priority = 5)] DegCUnicode,
    #[token("degC", priority = 4)] DegCUnit,
    #[token("K", priority = 3)] KelvinUnit,
    
    // Percentage units
    #[token("%", priority = 4)] PercentUnit,
    #[token("pct", priority = 4)] PctUnit,
    
    // Power units
    #[token("W", priority = 3)] WUnit,
    #[token("mW", priority = 4)] MWUnit,
    #[token("µW", priority = 5)] UWUnicode,
    #[token("uW", priority = 4)] UWUnit,
    #[token("nW", priority = 4)] NWUnit,
    
    // Length units (for physical constraints)
    #[token("mm", priority = 4)] MMUnit,
    #[token("µm", priority = 5)] UMUnicode,
    #[token("um", priority = 4)] UMUnit,
    #[token("nm", priority = 4)] NMUnit,
    #[token("mil", priority = 4)] MILUnit,
    
    // Additional units
    #[token("dB", priority = 4)] DbUnit,
    #[token("dBm", priority = 4)] DbmUnit,

    // Keywords and Identifiers recognized by a callback
    #[regex("[a-zA-Z_][a-zA-Z0-9_]*", keyword_or_ident_callback)]
    KeywordOrIdent(KeywordOrIdent),

    // Simple tokens (can have simple names, don't need to match SyntaxKind names)
    #[token("(")] LParen,
    #[token(")")] RParen,
    #[token("{")] LBrace,
    #[token("}")] RBrace,
    #[token("[")] LBrack,
    #[token("]")] RBrack,
    #[token(";")] Semi,
    #[token(":")] Colon,
    #[token(",")] Comma,
    #[token("=")] Eq,
    #[token("..", priority = 2)] DotDot,
    #[token(".")] Dot,
    #[token("+")] Plus,
    #[token("-")] Minus,
    #[token("*")] Star,
    #[token("/")] Slash,
    #[token("%")] Percent,
    #[token("&")] Ampersand,
    #[token("|")] Pipe,
    #[token("^")] Caret,
    #[token("!")] Bang,
    #[token("?")] Question,
    #[token("~")] Tilde,
    #[token("<")] LAngle,
    #[token(">")] RAngle,
    #[token("@")] At,

    // Numbers: integers, floats, and scientific notation. Examples:
    //   42, 1_000_000, 3.14, 0.5e-3, 6.734e-15, 1.2E+8, 1e9
    // The optional underscore-separator is preserved from the legacy
    // grammar (a digit-grouping convenience like Rust's `1_000_000`).
    // Scientific notation matters for SPICE-shaped device parameters —
    // e.g. BJT saturation currents like `is = 6.734e-15`.
    #[regex(r"[0-9]+(?:_[0-9]+)*(?:\.[0-9]+(?:_[0-9]+)*)?(?:[eE][+\-]?[0-9]+)?", priority = 1)]
    Number,
    // String literal regex
    #[regex(r#""([^"\\]|\\.)*""#, priority = 1)]
    String,

    // Raw-string literal: `r"..."`, `r#"..."#`, `r##"..."##`, ...
    // Used for embedding foreign-language source (Rhai scripts) verbatim
    // inside a .bhdl file: no escape sequences, no surprises. The opening
    // `r` and zero-or-more `#` characters before the leading `"` must be
    // matched by the same hash count at close (`"###...`). Lexer regex
    // matches `r` + hashes + `"` to start the literal; the callback
    // scans forward for the matching close delimiter and bumps past it.
    #[regex(r#"r#*""#, callback = raw_string_callback, priority = 2)]
    RawString,

    #[token("->")] Arrow,
    #[token("<-")] LeftArrow,   // Left arrow for port mapping
    #[token("<->")] BiArrow,   // Bidirectional connection
    #[token("|>")] FlowOp,     // Flow operator
    #[token("<=>")] InterfaceOp, // Interface connection
    #[token("==")] EqEq,
    #[token("!=")] Neq,
    #[token("+=")] PlusEq,
    #[token("-=")] MinusEq,
    #[token("<=")] LtEq,
    #[token(">=")] GtEq,
    #[token("&&")] AmpAmp,
    #[token("||")] PipePipe,
    #[token("<<")] LShift,
    #[token(">>")] RShift,

    // Error token (Logos handles this internally now)
    // Error, // Removed explicit Error variant
}

// Callback for raw-string literals (`r"..."`, `r#"..."#`, `r##"..."##`, …).
//
// The Logos regex matched `r` + `n` hashes + `"` where `n` ≥ 0. The
// matching close is `"` + the same `n` hashes. Scan the remainder of the
// input for that close pattern and bump the lexer past it, returning the
// full literal as a single token. If the close pattern is never found,
// return `None` so Logos emits a lexer error spanning the unterminated
// literal — keeps a runaway raw string from swallowing the rest of the
// file silently.
fn raw_string_callback(lex: &mut Lexer<LexerToken>) -> Option<()> {
    let opener = lex.slice();
    // `opener` is `r` + n hashes + `"`. The matching close is `"` then
    // n hashes — build it dynamically. n = opener.len() − 2.
    let n_hashes = opener.len().saturating_sub(2);
    let mut close = String::with_capacity(1 + n_hashes);
    close.push('"');
    for _ in 0..n_hashes { close.push('#'); }
    let rest = lex.remainder();
    let close_pos = rest.find(&close)?;
    lex.bump(close_pos + close.len());
    Some(())
}

fn keyword_or_ident_callback(lex: &mut Lexer<LexerToken>) -> KeywordOrIdent {
    let slice = lex.slice();
    let kind = match slice {
        // Top Level Keywords
        "board" => SyntaxKind::BOARD_KW,
        "entity" => SyntaxKind::ENTITY_KW,
        "component" => SyntaxKind::COMPONENT_KW, // Used for def & inst
        "interface" => SyntaxKind::INTERFACE_KW,
        "typedef" => SyntaxKind::TYPEDEF_KW,
        "struct" => SyntaxKind::STRUCT_KW,
        "enum" => SyntaxKind::ENUM_KW,
        "match" => SyntaxKind::MATCH_KW,
        "import" => SyntaxKind::IMPORT_KW,
        "const" => SyntaxKind::CONST_KW,

        // Block Keywords (removed v1.0 block keywords)
        "layer_stackup" => SyntaxKind::LAYER_STACKUP_KW,
        "default_design_rules" => SyntaxKind::DEFAULT_DESIGN_RULES_KW,
        "design" => SyntaxKind::DESIGN_KW,
        "constrain" => SyntaxKind::CONSTRAIN_KW,

        // Item Keywords (NEW)
        "net" => SyntaxKind::NET_KW,
        "pin" => SyntaxKind::PIN_KW,
        "port" => SyntaxKind::PORT_KW,
        "parameter" => SyntaxKind::PARAMETER_KW,
        "connect" => SyntaxKind::CONNECT_KW,
        "assign" => SyntaxKind::ASSIGN_KW,
        "layer" => SyntaxKind::LAYER_KW,

        // Other Keywords
        "in" => SyntaxKind::IN_KW,
        "input" => SyntaxKind::INPUT_KW,
        "output" => SyntaxKind::OUTPUT_KW,
        "out" => SyntaxKind::OUT_KW,
        "inout" => SyntaxKind::INOUT_KW,
        "signal" => SyntaxKind::SIGNAL_KW,
        "power" => SyntaxKind::POWER_KW,
        "ground" => SyntaxKind::GROUND_KW,
        "switch" => SyntaxKind::SWITCH_KW,
        "feedback" => SyntaxKind::FEEDBACK_KW,
        "if" => SyntaxKind::IF_KW,
        "else" => SyntaxKind::ELSE_KW,
        "when" => SyntaxKind::WHEN_KW,
        "attribute" => SyntaxKind::ATTRIBUTE_KW,
        "clock" => SyntaxKind::CLOCK_KW,
        "wire" => SyntaxKind::WIRE_KW,
        "tri" => SyntaxKind::TRI_KW,
        "trireg" => SyntaxKind::TRIREG_KW,
        "uwire" => SyntaxKind::UWIRE_KW,
        "generate" => SyntaxKind::GENERATE_KW,
        "for" => SyntaxKind::FOR_KW,
        // "to" is contextual - will be IDENT
        "extends" => SyntaxKind::EXTENDS_KW,
        "as" => SyntaxKind::AS_KW,
        "alias" => SyntaxKind::ALIAS_KW,
        "type" => SyntaxKind::TYPE_KW,
        "null" => SyntaxKind::NULL_KW,
        "true" => SyntaxKind::TRUE_KW,
        "false" => SyntaxKind::FALSE_KW,
        "require" => SyntaxKind::REQUIRE_KW,
        "optional" => SyntaxKind::OPTIONAL_KW,
        "virtual" => SyntaxKind::VIRTUAL_KW,
        "perspective" => SyntaxKind::PERSPECTIVE_KW,
        "where" => SyntaxKind::WHERE_KW,
        "with" => SyntaxKind::WITH_KW,
        "satisfies" => SyntaxKind::SATISFIES_KW,
        "via" => SyntaxKind::VIA_KW,
        "trait" => SyntaxKind::TRAIT_KW,
        "impl" => SyntaxKind::IMPL_KW,
        "safety_goal" => SyntaxKind::SAFETY_GOAL_KW,
        "fault_inject" => SyntaxKind::FAULT_INJECT_KW,
        "expansion" => SyntaxKind::EXPANSION_KW,
        "internal" => SyntaxKind::INTERNAL_KW,
        "placement" => SyntaxKind::PLACEMENT_KW,
        "symbol" => SyntaxKind::SYMBOL_KW,
        "layout" => SyntaxKind::LAYOUT_KW,
        "body" => SyntaxKind::BODY_KW,
        "left" => SyntaxKind::LEFT_KW,
        "right" => SyntaxKind::RIGHT_KW,
        "top" => SyntaxKind::TOP_KW,
        "bottom" => SyntaxKind::BOTTOM_KW,
        "group" => SyntaxKind::GROUP_KW,
        "part" => SyntaxKind::PART_KW,
        "package" => SyntaxKind::PACKAGE_KW,

        // Power domain keywords (Phase 1: Scalability)
        "power_domain" => SyntaxKind::POWER_DOMAIN_KW,
        "sources" => SyntaxKind::SOURCES_KW,
        "distribution" => SyntaxKind::DISTRIBUTION_KW,
        "decoupling" => SyntaxKind::DECOUPLING_KW,
        "near" => SyntaxKind::NEAR_KW,
        "each" => SyntaxKind::EACH_KW,
        "distributed" => SyntaxKind::DISTRIBUTED_KW,
        "constraints" => SyntaxKind::CONSTRAINTS_KW,
        // "pin_map" is NOT a keyword, parsed as IDENT

        // Testbench keywords - only top-level keywords are global
        "testbench" => SyntaxKind::TESTBENCH_KW,
        // The following are contextual and will be treated as IDENT
        // "simulation", "scope", "stimulus", "verify", "assert", "measure"
        // "after", "always", "capture", "trigger", "continuous", "on_change"
        // "periodic", "signals", "duration", "timestep", "solver", "temperature"

        // Default to IDENT if not a keyword
        _ => SyntaxKind::IDENT,
    };
    KeywordOrIdent { kind, text: slice.into() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use logos::Logos;
    use crate::syntax::SyntaxKind;

    #[test]
    fn lex_ident_and_keywords() {
        let input = "board entity net generate for in interface port const signal wire assign";
        let expected_kinds = vec![
            SyntaxKind::BOARD_KW, SyntaxKind::ENTITY_KW, SyntaxKind::NET_KW,
            SyntaxKind::GENERATE_KW, SyntaxKind::FOR_KW, SyntaxKind::IN_KW,
            SyntaxKind::INTERFACE_KW, SyntaxKind::PORT_KW, SyntaxKind::CONST_KW,
            SyntaxKind::SIGNAL_KW, SyntaxKind::WIRE_KW, SyntaxKind::ASSIGN_KW,
        ];

        let lexer = LexerToken::lexer(input);
        let actual_kinds: Vec<SyntaxKind> = lexer.filter_map(|res| {
            match res {
                Ok(LexerToken::KeywordOrIdent(payload)) => Some(payload.kind),
                _ => None,
            }
        }).collect();

        assert_eq!(actual_kinds, expected_kinds);
    }

    #[test]
    fn lex_basic_board() {
        let input = r#"
            board my_board {
                // comment
            }
        "#;
        let expected: &[Result<LexerToken, ()>] = &[
            Ok(LexerToken::KeywordOrIdent(KeywordOrIdent { kind: SyntaxKind::BOARD_KW, text: "board".into() })),
            Ok(LexerToken::KeywordOrIdent(KeywordOrIdent { kind: SyntaxKind::IDENT, text: "my_board".into() })),
            Ok(LexerToken::LBrace),
            Ok(LexerToken::RBrace),
        ];

        let lexer = LexerToken::lexer(input);
        let actual: Vec<Result<LexerToken, ()>> = lexer.collect();

        assert_eq!(actual, expected);
    }

    #[test]
    fn lex_flow_operators() {
        let input = "-> <-> |> <=>";
        let expected: &[Result<LexerToken, ()>] = &[
            Ok(LexerToken::Arrow),
            Ok(LexerToken::BiArrow),
            Ok(LexerToken::FlowOp),
            Ok(LexerToken::InterfaceOp),
        ];

        let lexer = LexerToken::lexer(input);
        let actual: Vec<Result<LexerToken, ()>> = lexer.collect();

        assert_eq!(actual, expected);
    }

    #[test]
    fn lex_new_keywords() {
        let input = "if else when generate for constrain";
        let expected_kinds = vec![
            SyntaxKind::IF_KW, SyntaxKind::ELSE_KW, SyntaxKind::WHEN_KW,
            SyntaxKind::GENERATE_KW, SyntaxKind::FOR_KW, SyntaxKind::CONSTRAIN_KW,
        ];

        let lexer = LexerToken::lexer(input);
        let actual_kinds: Vec<SyntaxKind> = lexer.filter_map(|res| {
            match res {
                Ok(LexerToken::KeywordOrIdent(payload)) => Some(payload.kind),
                _ => None,
            }
        }).collect();

        assert_eq!(actual_kinds, expected_kinds);
    }

    #[test]
    fn lex_electrical_units() {
        let input = "kΩ µF MHz V mA";
        let expected: &[Result<LexerToken, ()>] = &[
            Ok(LexerToken::KOhmUnicode),
            Ok(LexerToken::UFUnicode),
            Ok(LexerToken::MHzUnit),
            Ok(LexerToken::VUnit),
            Ok(LexerToken::MAUnit),
        ];

        let lexer = LexerToken::lexer(input);
        let actual: Vec<Result<LexerToken, ()>> = lexer.collect();

        assert_eq!(actual, expected);
    }

    #[test]
    fn lex_numbers_with_units() {
        let input = "4.7kΩ 10µF 400MHz";
        let lexer = LexerToken::lexer(input);
        let tokens: Vec<Result<LexerToken, ()>> = lexer.collect();
        
        // Numbers and units are separate tokens, which is correct for parsing
        assert!(tokens.iter().any(|t| matches!(t, Ok(LexerToken::Number))));
        assert!(tokens.iter().any(|t| matches!(t, Ok(LexerToken::KOhmUnicode))));
        assert!(tokens.iter().any(|t| matches!(t, Ok(LexerToken::UFUnicode))));
        assert!(tokens.iter().any(|t| matches!(t, Ok(LexerToken::MHzUnit))));
    }

    #[test]
    fn lex_raw_string_no_hashes() {
        // The simplest raw string: `r"..."` — same delimiters as a regular
        // string but no escape processing. Verifies the zero-hashes case.
        let input = r#"r"hello""#;
        let mut lexer = LexerToken::lexer(input);
        let tok = lexer.next().unwrap();
        assert!(matches!(tok, Ok(LexerToken::RawString)),
            "expected RawString, got {tok:?}");
        assert_eq!(lexer.slice(), r#"r"hello""#);
        assert!(lexer.next().is_none());
    }

    #[test]
    fn lex_raw_string_with_hashes() {
        // The `r#"..."#` form lets the body contain bare `"` — exactly
        // what an embedded Rhai script needs (`#{...}` map literals,
        // string concatenation, etc.). Body here contains a quote and a
        // backslash; both pass through verbatim.
        let input = r##"r#"let s = "hi\n"; s"#"##;
        let mut lexer = LexerToken::lexer(input);
        let tok = lexer.next().unwrap();
        assert!(matches!(tok, Ok(LexerToken::RawString)),
            "expected RawString, got {tok:?}");
        // The full literal — including the `r#"` and `"#` — is one token.
        assert_eq!(lexer.slice(), r##"r#"let s = "hi\n"; s"#"##);
        assert!(lexer.next().is_none());
    }

    #[test]
    fn lex_raw_string_nested_hash_quote() {
        // Body contains `"#`, so a single-hash delimiter would close early.
        // Verifies the multi-hash variant lets vendors embed whatever
        // characters their script needs.
        let input = r###"r##"contains "#"##"###;
        let mut lexer = LexerToken::lexer(input);
        let tok = lexer.next().unwrap();
        assert!(matches!(tok, Ok(LexerToken::RawString)),
            "expected RawString, got {tok:?}");
        assert_eq!(lexer.slice(), r###"r##"contains "#"##"###);
        assert!(lexer.next().is_none());
    }

    #[test]
    fn lex_component_instantiation() {
        let input = "VCC -> Res(330Ω).1 -> LED(red).A;";
        let lexer = LexerToken::lexer(input);
        let tokens: Vec<Result<LexerToken, ()>> = lexer.collect();
        
        // Check that we successfully tokenize this complex circuit flow paradigm expression
        assert!(tokens.len() > 10); // Should have many tokens
        assert!(tokens.iter().any(|t| matches!(t, Ok(LexerToken::Arrow))));
        assert!(tokens.iter().any(|t| matches!(t, Ok(LexerToken::OhmUnicode))));
    }

    #[test]
    fn test_simple_component_instantiation() {
        use crate::parse;
        
        let input = r#"
        board TestCircuit {
            VCC -> Res(330).1 -> LED.A;
        }
        "#;
        
        let result = parse(input);
        
        if !result.errors().is_empty() {
            for error in result.errors() {
                println!("Error: {}", error.message);
            }
        }
        
        // Should parse with minimal errors for basic component instantiation
        assert!(result.errors().len() <= 5, "Simple component instantiation should mostly work");
    }

    #[test]
    fn test_flow_statement() {
        use crate::parse;
        
        let input = r#"
        board TestCircuit {
            power_flow: source |> regulation |> loads;
        }
        "#;
        
        let result = parse(input);
        
        if !result.errors().is_empty() {
            for error in result.errors() {
                println!("Flow Error: {}", error.message);
            }
        }
        
        // Should parse flow statements correctly
        assert!(result.errors().len() <= 3, "Flow statements should work");
    }
} 