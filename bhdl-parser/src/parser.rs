use logos::Logos;
use rowan::GreenNodeBuilder;
use smol_str::SmolStr;
use std::ops::Range;

use crate::lexer::LexerToken;
use crate::syntax::{SyntaxKind, BhdlLanguage};

// --- Public Interface ---

// ParseResult struct
#[derive(Debug, Clone)]
pub struct ParseResult {
    green_node: rowan::GreenNode,
    errors: Vec<ParseError>, // Keep track of errors encountered
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
    // Add range/span later
    // pub span: (usize, usize),
}

// New mapping function
fn map_token_stream(tokens: Vec<(Result<LexerToken, ()>, Range<usize>)>, source_text: &str) -> Vec<(SyntaxKind, SmolStr)> {
    tokens.into_iter()
        .filter_map(|(res, range)| {
            let text = SmolStr::new(&source_text[range]);
            match res {
                Ok(token) => Some((map_token(token), text)),
                Err(_) => Some((SyntaxKind::ERROR_TOKEN, text)), // Map lexer errors to ERROR_TOKEN
            }
        })
        .collect()
}

// Helper function to map a single LexerToken to SyntaxKind
fn map_token(token: LexerToken) -> SyntaxKind {
    use crate::SyntaxKind::*;
    
    match token {
        LexerToken::KeywordOrIdent(payload) => payload.kind,
        LexerToken::LParen => L_PAREN,
        LexerToken::RParen => R_PAREN,
        LexerToken::LBrace => L_BRACE,
        LexerToken::RBrace => R_BRACE,
        LexerToken::LBrack => L_BRACKET,
        LexerToken::RBrack => R_BRACKET,
        LexerToken::Semi => SEMI,
        LexerToken::Colon => COLON,
        LexerToken::Comma => COMMA,
        LexerToken::Eq => EQ,
        LexerToken::Dot => DOT,
        LexerToken::Plus => PLUS,
        LexerToken::Minus => MINUS,
        LexerToken::Star => STAR,
        LexerToken::Slash => SLASH,
        LexerToken::Percent => PERCENT,
        LexerToken::Ampersand => AMPERSAND,
        LexerToken::Pipe => PIPE,
        LexerToken::Caret => CARET,
        LexerToken::Bang => BANG,
        LexerToken::Question => QUESTION,
        LexerToken::Tilde => TILDE,
        LexerToken::LAngle => L_ANGLE,
        LexerToken::RAngle => R_ANGLE,
        LexerToken::At => AT,
        LexerToken::Number => NUMBER,
        LexerToken::String => STRING,
        LexerToken::Arrow => ARROW,
        LexerToken::BiArrow => BI_ARROW,
        LexerToken::FlowOp => FLOW_OP,
        LexerToken::InterfaceOp => INTERFACE_OP,
        LexerToken::EqEq => EQEQ,
        LexerToken::Neq => NEQ,
        LexerToken::LtEq => LTEQ,
        LexerToken::GtEq => GTEQ,
        LexerToken::AmpAmp => AMPAMP,
        LexerToken::PipePipe => PIPEPIPE,
        LexerToken::LShift => LSHIFT,
        LexerToken::RShift => RSHIFT,
        // All unit tokens map to UNIT_IDENTIFIER
        LexerToken::OhmUnicode | LexerToken::KOhmUnicode | LexerToken::MOhmUnicode | 
        LexerToken::MilliOhmUnicode | LexerToken::OhmUnit | LexerToken::KOhmUnit | 
        LexerToken::MOhmUnit | LexerToken::MilliOhmUnit |
        LexerToken::VUnit | LexerToken::VdcUnit | LexerToken::VacUnit | 
        LexerToken::VrmsUnit | LexerToken::VppUnit | LexerToken::MVUnit | 
        LexerToken::UVUnicode | LexerToken::UVUnit | LexerToken::NVUnit |
        LexerToken::AUnit | LexerToken::MAUnit | LexerToken::UAUnicode | 
        LexerToken::UAUnit | LexerToken::NAUnit |
        LexerToken::FUnit | LexerToken::UFUnicode | LexerToken::UFUnit | 
        LexerToken::NFUnit | LexerToken::PFUnit |
        LexerToken::HUnit | LexerToken::UHUnicode | LexerToken::UHUnit | 
        LexerToken::MHUnit | LexerToken::NHUnit |
        LexerToken::HzUnit | LexerToken::KHzUnit | LexerToken::MHzUnit | 
        LexerToken::GHzUnit |
        LexerToken::SUnit | LexerToken::MsUnit | LexerToken::UsUnicode | 
        LexerToken::UsUnit | LexerToken::NsUnit | LexerToken::PsUnit |
        LexerToken::DegCUnicode | LexerToken::DegCUnit | LexerToken::KelvinUnit |
        LexerToken::PercentUnit | LexerToken::PctUnit |
        LexerToken::WUnit | LexerToken::MWUnit | LexerToken::UWUnicode | 
        LexerToken::UWUnit | LexerToken::NWUnit |
        LexerToken::MMUnit | LexerToken::UMUnicode | LexerToken::UMUnit | 
        LexerToken::NMUnit | LexerToken::MILUnit |
        LexerToken::DbUnit | LexerToken::DbmUnit => UNIT_IDENTIFIER,
    }
}

// Main parse function
pub fn parse(text: &str) -> ParseResult {
    let tokens: Vec<_> = LexerToken::lexer(text).spanned().collect();
    let mapped_tokens = map_token_stream(tokens, text);
    let mut parser = crate::core::Parser::new(&mapped_tokens);
    parser.parse_source_file();
    let (green_node, errors) = parser.finish();
    ParseResult {
        green_node,
        errors,
    }
}