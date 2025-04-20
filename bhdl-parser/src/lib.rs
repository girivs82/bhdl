use logos::Logos;
use rowan::{GreenNode /*, GreenNodeBuilder, Language */}; // Keep GreenNode
use smol_str::SmolStr;
use std::ops::Range;

use crate::lexer::{LexerToken /*, LexedStr */}; // Remove LexedStr
// use crate::syntax::{BhdlLanguage, SyntaxKind};

// Declare modules
mod core;
mod expressions;
mod items;
mod blocks;
mod top_level;
mod lexer;
mod syntax;
mod tests;

// Re-export key types
// pub use crate::syntax::{BhdlLanguage, SyntaxKind};
pub use crate::core::Parser;
pub use crate::syntax::BhdlLanguage;
pub use crate::syntax::SyntaxKind;

// Optional: Re-export specific AST node types if desired
// pub use ast::BoardDef;

#[derive(Debug, Clone)]
pub struct ParseResult {
    green_node: GreenNode,
    pub(crate) errors: Vec<ParseError>,
}

impl ParseResult {
    pub fn syntax(&self) -> rowan::SyntaxNode<BhdlLanguage> {
        rowan::SyntaxNode::new_root(self.green_node.clone())
    }

    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    // pub span: (usize, usize),
}

// Main parse function
pub fn parse(text: &str) -> ParseResult {
    let tokens: Vec<_> = LexerToken::lexer(text).spanned().collect();
    let mapped_tokens = map_token_stream(tokens, text);
    let mut parser = core::Parser::new(&mapped_tokens);
    parser.parse_source_file(); // Assuming parse_source_file moves to top_level.rs
    let (green_node, errors) = parser.finish();
    ParseResult {
        green_node,
        errors,
    }
}

// Token mapping functions (remain in lib.rs?)
fn map_token_stream(tokens: Vec<(Result<LexerToken, ()>, Range<usize>)>, source_text: &str) -> Vec<(SyntaxKind, SmolStr)> {
    let mut result = Vec::new();
    let mut current_pos = 0;

    for (lex_result, range) in tokens {
        // Add WHITESPACE for gaps between tokens
        if range.start > current_pos {
            let text = SmolStr::new(&source_text[current_pos..range.start]);
            result.push((SyntaxKind::WHITESPACE, text));
        }

        let text = SmolStr::new(&source_text[range.clone()]);
        match lex_result {
            Ok(token) => {
                result.push((map_token(token), text));
            }
            Err(_) => {
                result.push((SyntaxKind::ERROR_TOKEN, text));
            }
        }
        current_pos = range.end;
    }

    // Add trailing WHITESPACE if any
    if current_pos < source_text.len() {
        let text = SmolStr::new(&source_text[current_pos..]);
         result.push((SyntaxKind::WHITESPACE, text));
    }

    result
}

fn map_token(token: LexerToken) -> SyntaxKind {
    match token {
        LexerToken::KeywordOrIdent(payload) => payload.kind,
        LexerToken::LParen => SyntaxKind::L_PAREN,
        LexerToken::RParen => SyntaxKind::R_PAREN,
        LexerToken::LBrace => SyntaxKind::L_BRACE,
        LexerToken::RBrace => SyntaxKind::R_BRACE,
        LexerToken::LBrack => SyntaxKind::L_BRACKET,
        LexerToken::RBrack => SyntaxKind::R_BRACKET,
        LexerToken::Semi => SyntaxKind::SEMI,
        LexerToken::Colon => SyntaxKind::COLON,
        LexerToken::Comma => SyntaxKind::COMMA,
        LexerToken::Eq => SyntaxKind::EQ,
        LexerToken::Dot => SyntaxKind::DOT,
        LexerToken::Plus => SyntaxKind::PLUS,
        LexerToken::Minus => SyntaxKind::MINUS,
        LexerToken::Star => SyntaxKind::STAR,
        LexerToken::Slash => SyntaxKind::SLASH,
        LexerToken::Percent => SyntaxKind::PERCENT,
        LexerToken::Ampersand => SyntaxKind::AMPERSAND,
        LexerToken::Pipe => SyntaxKind::PIPE,
        LexerToken::Caret => SyntaxKind::CARET,
        LexerToken::Bang => SyntaxKind::BANG,
        LexerToken::Question => SyntaxKind::QUESTION,
        LexerToken::Tilde => SyntaxKind::TILDE,
        LexerToken::LAngle => SyntaxKind::L_ANGLE,
        LexerToken::RAngle => SyntaxKind::R_ANGLE,
        LexerToken::At => SyntaxKind::AT,
        LexerToken::Number => SyntaxKind::NUMBER,
        LexerToken::String => SyntaxKind::STRING,
        LexerToken::Arrow => SyntaxKind::ARROW,
        LexerToken::EqEq => SyntaxKind::EQEQ,
        LexerToken::Neq => SyntaxKind::NEQ,
        LexerToken::LtEq => SyntaxKind::LTEQ,
        LexerToken::GtEq => SyntaxKind::GTEQ,
        LexerToken::AmpAmp => SyntaxKind::AMPAMP,
        LexerToken::PipePipe => SyntaxKind::PIPEPIPE,
        LexerToken::LShift => SyntaxKind::LSHIFT,
        LexerToken::RShift => SyntaxKind::RSHIFT,
        LexerToken::IfConnect => SyntaxKind::IF_CONNECT,
        LexerToken::KOhmUnit | LexerToken::MOHmUnit | LexerToken::GOhmUnit | LexerToken::OhmUnit |
        LexerToken::UFUnit | LexerToken::NFUnit | LexerToken::PFUnit |
        LexerToken::UHUnit | LexerToken::NHUnit | LexerToken::PHUnit |
        LexerToken::VdcUnit | LexerToken::VacUnit | LexerToken::VrmsUnit | LexerToken::VppUnit |
        LexerToken::MVUnit | LexerToken::UVUnit | LexerToken::NVUnit |
        LexerToken::MAUnit | LexerToken::UAUnit | LexerToken::NAUnit |
        LexerToken::MWUnit | LexerToken::UWUnit | LexerToken::NWUnit |
        LexerToken::HzUnit | LexerToken::KHzUnit | LexerToken::MHUnit | LexerToken::GHUnit |
        LexerToken::MsUnit | LexerToken::UsUnit | LexerToken::NsUnit | LexerToken::PsUnit |
        LexerToken::DegUnit | LexerToken::RadUnit |
        LexerToken::DbUnit | LexerToken::DbmUnit |
        LexerToken::PctUnit |
        LexerToken::MMUnit | LexerToken::UMUnit | LexerToken::NMUnit | LexerToken::MILUnit
        => SyntaxKind::UNIT_IDENTIFIER,
    }
}

// Remove the entire test module block below if it exists
// #[cfg(test)]
// mod tests {
//     #[test]
//     fn it_works() {
//         let result = 2 + 2;
//         assert_eq!(result, 4);
//     }
// } 