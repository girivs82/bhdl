use logos::{Lexer, Logos};
use smol_str::SmolStr;
use crate::syntax::SyntaxKind;

// Struct to hold IDENT/Keyword kind and text
#[derive(Debug, Clone, PartialEq)]
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
        _ => SyntaxKind::IDENT,
    };
    KeywordOrIdent { kind, text: slice.into() }
}

// Define the NEW LexerToken enum
#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\r\n\f]+")] // Ignore whitespace
#[logos(skip r"//[^\n]*")] // Ignore single-line comments
#[logos(skip r"/\*([^*]|\*[^/])*\*/")] // Ignore multi-line comments
pub enum LexerToken {
    #[regex("[a-zA-Z_][a-zA-Z0-9_]*", lex_ident_or_kw)]
    KeywordOrIdent(KeywordOrIdent), // Carries kind and text

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
    #[token("~")] Tilde,
    #[token("<")] LAngle,
    #[token(">")] RAngle,
    #[token("@")] At,

    #[regex(r"[0-9]+(?:_[0-9]+)*")] Number,
    #[regex(r#""([^"\\]|\\.)*""#)] String,

    #[token("->")] Arrow,
    #[token("==")] EqEq,
    #[token("!=")] Neq,
    #[token("<=")] LtEq,
    #[token(">=")] GtEq,
    #[token("&&")] AmpAmp,
    #[token("||")] PipePipe,
    #[token("<<")] LShift,
    #[token(">>")] RShift,

    // This represents an error during lexing. Logos requires an Error variant.
    // It doesn't map directly to a SyntaxKind node, but signifies a lexing failure.
    // REMOVE #[error] - Not needed in Logos 0.13+
    Error, // Required by Logos for error handling
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