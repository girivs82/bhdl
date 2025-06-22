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
mod error_recovery;
mod tests;
mod v2_fixes;
mod v2_parsing;
mod intent;

#[cfg(test)]
mod test_net_ref;

// Re-export key types
// pub use crate::syntax::{BhdlLanguage, SyntaxKind};
pub use crate::core::Parser;
pub use crate::syntax::BhdlLanguage;
pub use crate::syntax::SyntaxKind;

// Export lex for testing
pub fn lex(text: &str) -> Vec<(SyntaxKind, SmolStr)> {
    let tokens: Vec<_> = LexerToken::lexer(text).spanned().collect();
    map_token_stream(tokens, text)
}

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

    for (i, (lex_result, range)) in tokens.iter().enumerate() {
        // Add WHITESPACE for gaps between tokens
        if range.start > current_pos {
            let text = SmolStr::new(&source_text[current_pos..range.start]);
            result.push((SyntaxKind::WHITESPACE, text));
        }

        let text = SmolStr::new(&source_text[range.clone()]);
        match lex_result {
            Ok(token) => {
                let kind = map_token(token.clone());
                
                // Post-process unit tokens: if a unit token is not preceded by a number,
                // treat it as an identifier instead
                let final_kind = if kind == SyntaxKind::UNIT_IDENTIFIER {
                    // Check if previous non-whitespace token was a number
                    let mut prev_was_number = false;
                    for j in (0..result.len()).rev() {
                        match result[j].0 {
                            SyntaxKind::WHITESPACE | SyntaxKind::COMMENT => continue,
                            SyntaxKind::NUMBER => {
                                prev_was_number = true;
                                break;
                            }
                            _ => break,
                        }
                    }
                    
                    if prev_was_number {
                        kind // Keep as unit
                    } else {
                        // Check if this looks like a single-letter identifier that was misclassified
                        match text.as_str() {
                            "A" | "F" | "H" | "K" | "V" | "W" | "s" => SyntaxKind::IDENT,
                            _ => kind, // Keep multi-letter units as units
                        }
                    }
                } else {
                    kind
                };
                
                result.push((final_kind, text.clone()));
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
        LexerToken::LeftArrow => SyntaxKind::LEFT_ARROW,
        LexerToken::BiArrow => SyntaxKind::BI_ARROW,
        LexerToken::FlowOp => SyntaxKind::FLOW_OP,
        LexerToken::InterfaceOp => SyntaxKind::INTERFACE_OP,
        LexerToken::EqEq => SyntaxKind::EQEQ,
        LexerToken::Neq => SyntaxKind::NEQ,
        LexerToken::LtEq => SyntaxKind::LTEQ,
        LexerToken::GtEq => SyntaxKind::GTEQ,
        LexerToken::AmpAmp => SyntaxKind::AMPAMP,
        LexerToken::PipePipe => SyntaxKind::PIPEPIPE,
        LexerToken::PlusEq => SyntaxKind::PLUS_EQ,
        LexerToken::MinusEq => SyntaxKind::MINUS_EQ,
        LexerToken::LShift => SyntaxKind::LSHIFT,
        LexerToken::RShift => SyntaxKind::RSHIFT,
        // Resistance units (Unicode and ASCII)
        LexerToken::OhmUnicode | LexerToken::KOhmUnicode | LexerToken::MOhmUnicode | LexerToken::MilliOhmUnicode |
        LexerToken::OhmUnit | LexerToken::KOhmUnit | LexerToken::MOhmUnit | LexerToken::MilliOhmUnit |
        
        // Voltage units
        LexerToken::VUnit | LexerToken::VdcUnit | LexerToken::VacUnit | LexerToken::VrmsUnit | LexerToken::VppUnit |
        LexerToken::MVUnit | LexerToken::UVUnicode | LexerToken::UVUnit | LexerToken::NVUnit |
        
        // Current units
        LexerToken::AUnit | LexerToken::MAUnit | LexerToken::UAUnicode | LexerToken::UAUnit | LexerToken::NAUnit |
        
        // Capacitance units
        LexerToken::FUnit | LexerToken::UFUnicode | LexerToken::UFUnit | LexerToken::NFUnit | LexerToken::PFUnit |
        
        // Inductance units
        LexerToken::HUnit | LexerToken::UHUnicode | LexerToken::UHUnit | LexerToken::MHUnit | LexerToken::NHUnit |
        
        // Frequency units
        LexerToken::HzUnit | LexerToken::KHzUnit | LexerToken::MHzUnit | LexerToken::GHzUnit |
        
        // Time units
        LexerToken::SUnit | LexerToken::MsUnit | LexerToken::UsUnicode | LexerToken::UsUnit | LexerToken::NsUnit | LexerToken::PsUnit |
        
        // Temperature units
        LexerToken::DegCUnicode | LexerToken::DegCUnit | LexerToken::KelvinUnit |
        
        // Percentage units
        LexerToken::PercentUnit | LexerToken::PctUnit |
        
        // Power units
        LexerToken::WUnit | LexerToken::MWUnit | LexerToken::UWUnicode | LexerToken::UWUnit | LexerToken::NWUnit |
        
        // Length units
        LexerToken::MMUnit | LexerToken::UMUnicode | LexerToken::UMUnit | LexerToken::NMUnit | LexerToken::MILUnit |
        
        // Additional units
        LexerToken::DbUnit | LexerToken::DbmUnit
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