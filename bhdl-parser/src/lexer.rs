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
        "module" => SyntaxKind::MODULE_KW,
        "interface" => SyntaxKind::INTERFACE_KW,
        "component" => SyntaxKind::COMPONENT_KW,
        "parameters" => SyntaxKind::PARAMETERS_KW,
        "ports" => SyntaxKind::PORTS_KW,
        "pins" => SyntaxKind::PINS_KW,
        "components" => SyntaxKind::COMPONENTS_KW,
        "connections" => SyntaxKind::CONNECTIONS_KW,
        "nets" => SyntaxKind::NETS_KW,
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
        "power" => SyntaxKind::POWER_KW,
        "ground" => SyntaxKind::GROUND_KW,
        "clock" => SyntaxKind::CLOCK_KW,
        "out" => SyntaxKind::OUT_KW,
        "inout" => SyntaxKind::INOUT_KW,
        "tri" => SyntaxKind::TRI_KW,
        "trireg" => SyntaxKind::TRIREG_KW,
        "uwire" => SyntaxKind::UWIRE_KW,
        "import" => SyntaxKind::IMPORT_KW,
        "from" => SyntaxKind::FROM_KW,
        "as" => SyntaxKind::AS_KW,
        "to" => SyntaxKind::TO_KW,
        "layer" => SyntaxKind::LAYER_KW,
        "extends" => SyntaxKind::EXTENDS_KW,
        "constrain" => SyntaxKind::CONSTRAIN_KW,
        "interfaces" => SyntaxKind::INTERFACES_KW,
        "layer_stackup" => SyntaxKind::LAYER_STACKUP_KW,
        "default_design_rules" => SyntaxKind::DEFAULT_DESIGN_RULES_KW,
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
    // Units (Higher priority for ambiguous multi-letter, low for single)
    #[token("mOhm", priority = 3)] MOHmUnit,
    #[token("Gohm", priority = 3)] GOhmUnit,
    #[token("Ohm", priority = 3)] OhmUnit,
    // Capacitance
    #[token("kOhm", priority = 3)] KOhmUnit,
    #[token("uF", priority = 3)] UFUnit,
    #[token("nF", priority = 3)] NFUnit,
    #[token("pF", priority = 3)] PFUnit,
    #[token("uH", priority = 3)] UHUnit,
    #[token("nH", priority = 3)] NHUnit,
    #[token("pH", priority = 3)] PHUnit,
    #[token("Vdc", priority = 3)] VdcUnit,
    #[token("Vac", priority = 3)] VacUnit,
    #[token("Vrms", priority = 3)] VrmsUnit,
    #[token("Vpp", priority = 3)] VppUnit,
    #[token("Hz", priority = 3)] HzUnit, // Increased priority
    #[token("kHz", priority = 3)] KHzUnit,
    #[token("MHz", priority = 3)] MHUnit,
    #[token("GHz", priority = 3)] GHUnit,
    #[token("ms", priority = 3)] MsUnit,
    #[token("us", priority = 3)] UsUnit,
    #[token("ns", priority = 3)] NsUnit,
    #[token("ps", priority = 3)] PsUnit,
    #[token("deg", priority = 3)] DegUnit, // Increased priority
    #[token("rad", priority = 3)] RadUnit, // Increased priority
    #[token("dB", priority = 3)] DbUnit,  // Increased priority
    #[token("dBm", priority = 3)] DbmUnit,
    // Add mV, uV, nV
    #[token("mV", priority = 3)] MVUnit,
    #[token("uV", priority = 3)] UVUnit,
    #[token("nV", priority = 3)] NVUnit,

    // ... after AUnit ...
    // Add mA, uA, nA
    #[token("mA", priority = 3)] MAUnit,
    #[token("uA", priority = 3)] UAUnit,
    #[token("nA", priority = 3)] NAUnit,

    // ... after WUnit ...
    // Add mW, uW, nW
    #[token("mW", priority = 3)] MWUnit,
    #[token("uW", priority = 3)] UWUnit,
    #[token("nW", priority = 3)] NWUnit,

    // ... after PercentUnit ...
    // Add pct as an alternative
    #[token("pct", priority = 3)] PctUnit,
    // Add length units
    #[token("mm", priority = 3)] MMUnit,
    #[token("um", priority = 3)] UMUnit,
    #[token("nm", priority = 3)] NMUnit,
    #[token("mil", priority = 3)] MILUnit,

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

    // Updated Number regex to handle integers and floats (basic)
    #[regex(r"[0-9]+(?:_[0-9]+)*(?:\.[0-9]+(?:_[0-9]+)*)?", priority = 1)] 
    Number,
    // String literal regex
    #[regex(r#""([^"\\]|\\.)*""#, priority = 1)] 
    String,

    #[token("->")] Arrow,
    #[token("==")] EqEq,
    #[token("!=")] Neq,
    #[token("<=")] LtEq,
    #[token(">=")] GtEq,
    #[token("&&")] AmpAmp,
    #[token("||")] PipePipe,
    #[token("<<")] LShift,
    #[token(">>")] RShift,
    #[token("<=>")] IfConnect, // Interface connection token

    // Error token (Logos handles this internally now)
    // Error, // Removed explicit Error variant
}

fn keyword_or_ident_callback(lex: &mut Lexer<LexerToken>) -> KeywordOrIdent {
    let slice = lex.slice();
    let kind = match slice {
        // Top Level Keywords
        "board" => SyntaxKind::BOARD_KW,
        "module" => SyntaxKind::MODULE_KW,
        "component" => SyntaxKind::COMPONENT_KW, // Used for def & inst
        "interface" => SyntaxKind::INTERFACE_KW,
        "typedef" => SyntaxKind::TYPEDEF_KW,
        "import" => SyntaxKind::IMPORT_KW,
        "const" => SyntaxKind::CONST_KW,

        // Block Keywords
        "parameters" => SyntaxKind::PARAMETERS_KW,
        "ports" => SyntaxKind::PORTS_KW,
        "pins" => SyntaxKind::PINS_KW,
        "nets" => SyntaxKind::NETS_KW,
        "components" => SyntaxKind::COMPONENTS_KW,
        "connections" => SyntaxKind::CONNECTIONS_KW,
        "interfaces" => SyntaxKind::INTERFACES_KW,
        "layer_stackup" => SyntaxKind::LAYER_STACKUP_KW,
        "default_design_rules" => SyntaxKind::DEFAULT_DESIGN_RULES_KW,
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
        "out" => SyntaxKind::OUT_KW,
        "inout" => SyntaxKind::INOUT_KW,
        "signal" => SyntaxKind::SIGNAL_KW,
        "power" => SyntaxKind::POWER_KW,
        "ground" => SyntaxKind::GROUND_KW,
        "clock" => SyntaxKind::CLOCK_KW,
        "wire" => SyntaxKind::WIRE_KW,
        "tri" => SyntaxKind::TRI_KW,
        "trireg" => SyntaxKind::TRIREG_KW,
        "uwire" => SyntaxKind::UWIRE_KW,
        "generate" => SyntaxKind::GENERATE_KW,
        "for" => SyntaxKind::FOR_KW,
        "to" => SyntaxKind::TO_KW,
        "extends" => SyntaxKind::EXTENDS_KW,
        "as" => SyntaxKind::AS_KW,
        "true" => SyntaxKind::TRUE_KW,
        "false" => SyntaxKind::FALSE_KW,
        // "pin_map" is NOT a keyword, parsed as IDENT

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
        let input = "board module pins components connections nets generate for in interface port const signal wire assign";
        let expected_kinds = vec![
            SyntaxKind::BOARD_KW, SyntaxKind::MODULE_KW, SyntaxKind::PINS_KW,
            SyntaxKind::COMPONENTS_KW, SyntaxKind::CONNECTIONS_KW, SyntaxKind::NETS_KW,
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
} 