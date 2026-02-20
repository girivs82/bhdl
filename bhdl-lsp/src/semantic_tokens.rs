//! Semantic Tokens support - provides enhanced syntax highlighting

use tower_lsp::lsp_types::*;
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile, Board, Entity, ComponentDef, PowerDecl, GroundDecl, HasName};
use rowan::{SyntaxNode, SyntaxToken, NodeOrToken};
use bhdl_parser::{BhdlLanguage, SyntaxKind};

/// Token types supported by BHDL LSP
/// Must match the legend defined in server capabilities
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum TokenType {
    Keyword = 0,
    Type = 1,
    Variable = 2,
    Parameter = 3,
    Function = 4,
    Comment = 5,
    Number = 6,
    String = 7,
    Operator = 8,
    Namespace = 9,
}

impl TokenType {
    pub fn as_u32(self) -> u32 {
        self as u32
    }
}

/// Token modifiers (bitmask)
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum TokenModifier {
    Declaration = 0x01,
    Definition = 0x02,
    Readonly = 0x04,
}

/// A semantic token with position information
#[derive(Debug, Clone)]
struct SemanticToken {
    line: u32,
    start_char: u32,
    length: u32,
    token_type: TokenType,
    modifiers: u32,
}

/// Provide semantic tokens for the entire document
pub fn provide_semantic_tokens(text: &str) -> Option<SemanticTokensResult> {
    let parse_result = parse(text);
    let source_file = SourceFile::cast(parse_result.syntax())?;

    let mut tokens = Vec::new();

    // Traverse AST and collect tokens
    collect_tokens(&source_file.syntax().clone(), text, &mut tokens);

    // Sort tokens by position
    tokens.sort_by_key(|t| (t.line, t.start_char));

    // Encode tokens in LSP format (relative encoding)
    let data = encode_tokens(&tokens);

    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    }))
}

/// Collect semantic tokens from AST
fn collect_tokens(node: &SyntaxNode<BhdlLanguage>, text: &str, tokens: &mut Vec<SemanticToken>) {
    // Check if this node is a semantic entity we care about
    if let Some(board) = Board::cast(node.clone()) {
        if let Some(name) = board.name() {
            add_token(&name, TokenType::Namespace, 0, text, tokens);
        }
    } else if let Some(entity) = Entity::cast(node.clone()) {
        if let Some(name) = entity.name() {
            add_token(&name, TokenType::Namespace, 0, text, tokens);
        }
    } else if let Some(component) = ComponentDef::cast(node.clone()) {
        if let Some(name) = component.name() {
            add_token(&name, TokenType::Type, 0, text, tokens);
        }
    } else if let Some(power) = PowerDecl::cast(node.clone()) {
        if let Some(name) = power.name() {
            add_token(&name, TokenType::Variable, TokenModifier::Readonly as u32, text, tokens);
        }
    } else if let Some(ground) = GroundDecl::cast(node.clone()) {
        if let Some(name) = ground.name() {
            add_token(&name, TokenType::Variable, TokenModifier::Readonly as u32, text, tokens);
        }
    }

    // Traverse all tokens and classify by syntax kind
    for element in node.children_with_tokens() {
        match element {
            NodeOrToken::Node(child) => {
                collect_tokens(&child, text, tokens);
            }
            NodeOrToken::Token(token) => {
                classify_token(&token, text, tokens);
            }
        }
    }
}

/// Classify a token by its syntax kind
fn classify_token(token: &SyntaxToken<BhdlLanguage>, text: &str, tokens: &mut Vec<SemanticToken>) {
    let token_type = match token.kind() {
        SyntaxKind::BOARD_KW | SyntaxKind::ENTITY_KW | SyntaxKind::COMPONENT_KW |
        SyntaxKind::POWER_KW | SyntaxKind::GROUND_KW | SyntaxKind::NET_KW |
        SyntaxKind::IN_KW | SyntaxKind::OUT_KW | SyntaxKind::INOUT_KW |
        SyntaxKind::FOR_KW | SyntaxKind::GENERATE_KW | SyntaxKind::IF_KW |
        SyntaxKind::IMPORT_KW | SyntaxKind::FROM_KW | SyntaxKind::ALIAS_KW |
        SyntaxKind::CONST_KW | SyntaxKind::WHEN_KW | SyntaxKind::ELSE_KW => {
            Some(TokenType::Keyword)
        }
        SyntaxKind::NUMBER => Some(TokenType::Number),
        SyntaxKind::STRING => Some(TokenType::String),
        SyntaxKind::COMMENT => Some(TokenType::Comment),
        SyntaxKind::PLUS | SyntaxKind::MINUS | SyntaxKind::STAR | SyntaxKind::SLASH |
        SyntaxKind::ARROW | SyntaxKind::BI_ARROW | SyntaxKind::FLOW_OP | SyntaxKind::INTERFACE_OP => {
            Some(TokenType::Operator)
        }
        _ => None,
    };

    if let Some(tt) = token_type {
        add_token(token, tt, 0, text, tokens);
    }
}

/// Add a token to the list
fn add_token(
    token: &SyntaxToken<BhdlLanguage>,
    token_type: TokenType,
    modifiers: u32,
    text: &str,
    tokens: &mut Vec<SemanticToken>,
) {
    let range = token.text_range();
    let start_offset: usize = range.start().into();
    let length = token.text().len();

    let (line, start_char) = offset_to_position(text, start_offset);

    tokens.push(SemanticToken {
        line,
        start_char,
        length: length as u32,
        token_type,
        modifiers,
    });
}

/// Convert byte offset to line/character position
fn offset_to_position(text: &str, offset: usize) -> (u32, u32) {
    let mut line = 0;
    let mut character = 0;
    let mut current_offset = 0;

    for ch in text.chars() {
        if current_offset >= offset {
            break;
        }

        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
        current_offset += ch.len_utf8();
    }

    (line, character)
}

/// Encode tokens in LSP relative format
fn encode_tokens(tokens: &[SemanticToken]) -> Vec<tower_lsp::lsp_types::SemanticToken> {
    let mut prev_line = 0;
    let mut prev_char = 0;
    let mut encoded = Vec::new();

    for token in tokens {
        let delta_line = token.line - prev_line;
        let delta_start = if delta_line == 0 {
            token.start_char - prev_char
        } else {
            token.start_char
        };

        encoded.push(tower_lsp::lsp_types::SemanticToken {
            delta_line,
            delta_start,
            length: token.length,
            token_type: token.token_type.as_u32(),
            token_modifiers_bitset: token.modifiers,
        });

        prev_line = token.line;
        prev_char = token.start_char;
    }

    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semantic_tokens_basic() {
        let text = r#"
board TestBoard {
    power VCC = 5V;
    ground GND;
}
"#;

        let result = provide_semantic_tokens(text);
        assert!(result.is_some());

        if let Some(SemanticTokensResult::Tokens(tokens)) = result {
            // Should have tokens for: board (keyword), TestBoard (namespace),
            // power (keyword), VCC (variable), etc.
            assert!(!tokens.data.is_empty());
        }
    }

    #[test]
    fn test_token_encoding() {
        let tokens = vec![
            SemanticToken {
                line: 0,
                start_char: 0,
                length: 5,
                token_type: TokenType::Keyword,
                modifiers: 0,
            },
            SemanticToken {
                line: 0,
                start_char: 6,
                length: 4,
                token_type: TokenType::Variable,
                modifiers: 0,
            },
            SemanticToken {
                line: 1,
                start_char: 4,
                length: 3,
                token_type: TokenType::Number,
                modifiers: 0,
            },
        ];

        let encoded = encode_tokens(&tokens);

        // Should have 3 tokens
        assert_eq!(encoded.len(), 3);

        // First token: absolute position
        assert_eq!(encoded[0].delta_line, 0);
        assert_eq!(encoded[0].delta_start, 0);
        assert_eq!(encoded[0].length, 5);
        assert_eq!(encoded[0].token_type, TokenType::Keyword.as_u32());

        // Second token: same line, delta from previous
        assert_eq!(encoded[1].delta_line, 0);
        assert_eq!(encoded[1].delta_start, 6);
        assert_eq!(encoded[1].length, 4);

        // Third token: new line, absolute char position
        assert_eq!(encoded[2].delta_line, 1);
        assert_eq!(encoded[2].delta_start, 4);
        assert_eq!(encoded[2].length, 3);
    }

    #[test]
    fn test_offset_to_position() {
        let text = "hello\nworld\ntest";

        assert_eq!(offset_to_position(text, 0), (0, 0)); // 'h'
        assert_eq!(offset_to_position(text, 5), (0, 5)); // '\n'
        assert_eq!(offset_to_position(text, 6), (1, 0)); // 'w'
        assert_eq!(offset_to_position(text, 12), (2, 0)); // 't'
    }
}
