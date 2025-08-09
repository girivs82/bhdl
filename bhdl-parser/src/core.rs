use rowan::GreenNodeBuilder;
use smol_str::SmolStr;
use crate::syntax::SyntaxKind;
use crate::ParseError; // Assuming ParseError is pub in lib.rs
use rowan::GreenNode; // Import GreenNode for the finish method return type

// Parser struct definition
pub struct Parser<'t> {
    pub(crate) tokens: &'t [(SyntaxKind, SmolStr)],
    pub(crate) builder: GreenNodeBuilder<'static>,
    pub(crate) errors: Vec<ParseError>,
    pub(crate) pos: usize,
}

// Core Parser implementation (new, peek, eat, expect, bump, skip_trivia, error, etc.)
impl<'t> Parser<'t> {
    // Correct constructor signature
    pub(crate) fn new(tokens: &'t [(SyntaxKind, SmolStr)]) -> Self {
        Parser {
            tokens,
            builder: GreenNodeBuilder::new(),
            errors: Vec::new(),
            pos: 0,
        }
    }

    // --- Core Helper Methods ---

    /// Returns the kind of the current token, skipping trivia.
    pub(crate) fn peek(&self) -> Option<SyntaxKind> {
        let mut temp_pos = self.pos;
        loop {
            match self.tokens.get(temp_pos) {
                Some((kind, _)) if kind.is_trivia() => temp_pos += 1,
                Some((kind, _)) => return Some(*kind),
                None => return None,
            }
        }
    }
    
    /// Returns the text of the current token, skipping trivia.
    pub(crate) fn peek_text(&self) -> Option<SmolStr> {
        let mut temp_pos = self.pos;
        loop {
            match self.tokens.get(temp_pos) {
                Some((kind, text)) if kind.is_trivia() => temp_pos += 1,
                Some((_, text)) => return Some(text.clone()),
                None => return None,
            }
        }
    }

    /// Returns the kind of the current token *without* skipping trivia.
    #[allow(dead_code)] // May not be used after refactor
    pub(crate) fn peek_raw(&self) -> Option<SyntaxKind> {
        self.tokens.get(self.pos).map(|(kind, _)| *kind)
    }

    /// Returns the kind of the nth token ahead, skipping trivia between tokens.
    #[allow(dead_code)] // May not be used after refactor
    pub(crate) fn peek_n(&self, n: usize) -> Option<SyntaxKind> {
        let mut count = 0;
        let mut temp_pos = self.pos;
        loop {
            match self.tokens.get(temp_pos) {
                Some((kind, _)) if kind.is_trivia() => temp_pos += 1,
                Some((kind, _)) => {
                    if count == n {
                        return Some(*kind);
                    }
                    count += 1;
                    temp_pos += 1;
                }
                None => return None,
            }
        }
    }

    /// Consumes the current token if it matches the expected kind.
    /// Returns true if consumed, false otherwise.
    pub(crate) fn eat(&mut self, expected: SyntaxKind) -> bool {
        if self.peek() == Some(expected) {
            self.bump(); // bump handles trivia and adds token to builder
            true
        } else {
            false
        }
    }

    /// Consumes the current token if it matches the expected kind.
    /// Reports an error if the token doesn't match.
    pub(crate) fn expect(&mut self, expected: SyntaxKind) {
        if self.peek() == Some(expected) {
            self.bump();
        } else {
            self.error(format!("Expected {:?}, found {:?}", expected, self.peek()));
            // Recovery: Don't bump here, let the caller decide how to recover.
            //             Or, alternatively, bump the unexpected token:
            // if self.peek().is_some() { self.bump_any(); }
        }
    }

    /// Consumes trivia tokens (whitespace, comments) until a non-trivia token is found.
    pub(crate) fn skip_trivia(&mut self) {
        while let Some((kind, text)) = self.tokens.get(self.pos) {
            if kind.is_trivia() {
                self.builder.token((*kind).into(), text);
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// Consumes the current token regardless of its kind, adding it to the tree.
    /// Skips trivia first.
    pub(crate) fn bump(&mut self) {
        self.skip_trivia();
        if self.pos < self.tokens.len() {
            let (kind, text) = self.tokens[self.pos].clone();
            self.builder.token(kind.into(), &text);
            self.pos += 1;
        } else {
             // Should not happen if peek() was checked before bump()
             self.error("Internal error: bump called at EOF".to_string());
        }
    }

    /// Consumes the current token *without* skipping trivia first. Used for recovery.
    /// Adds the token (potentially trivia or error) to the node builder.
    pub(crate) fn bump_any(&mut self) {
        if self.pos < self.tokens.len() {
            let (kind, text) = self.tokens[self.pos].clone();
            self.builder.token(kind.into(), &text);
            self.pos += 1;
        } else {
             self.error("Internal error: bump_any called at EOF".to_string());
        }
    }

    /// Expect an identifier, but also accept certain keywords that can be used as identifiers in this context
    pub(crate) fn expect_ident_or_contextual_keyword(&mut self) {
        match self.peek() {
            Some(SyntaxKind::IDENT) => self.bump(),
            // Keywords that can be used as identifiers in certain contexts
            Some(SyntaxKind::OUTPUT_KW) |
            Some(SyntaxKind::INPUT_KW) |
            Some(SyntaxKind::SIGNAL_KW) |
            Some(SyntaxKind::POWER_KW) |
            Some(SyntaxKind::GROUND_KW) |
            Some(SyntaxKind::CLOCK_KW) => self.bump(),
            _ => {
                self.error(format!("Expected identifier, found {:?}", self.peek()));
            }
        }
    }
    
    /// Records a parse error.
    pub(crate) fn error(&mut self, message: String) {
        self.errors.push(ParseError { message });
        // Optionally add error node to the tree for better recovery?
        // self.builder.start_node(SyntaxKind::ERROR.into());
        // self.builder.finish_node();
    }

    /// Finalizes parsing and returns the resulting GreenNode and errors.
    pub(crate) fn finish(self) -> (GreenNode, Vec<ParseError>) {
        (self.builder.finish(), self.errors)
    }

    /// Returns the kind of the current token being pointed to. Used in error reporting.
    #[allow(dead_code)] // May not be used after refactor
    pub(crate) fn current(&self) -> Option<SyntaxKind> {
        self.peek() // Use peek to get the kind after skipping trivia
    }
}

// Move SyntaxKindExt trait here if it makes sense
// Or keep it in lib.rs if map_token uses it?
// Let's move it here for now, as peek() uses it.
// Remove specific imports as they are covered by the glob import above
// use crate::syntax::SyntaxKind::WHITESPACE;
// use crate::syntax::SyntaxKind::COMMENT;

pub(crate) trait SyntaxKindExt {
    fn is_trivia(self) -> bool;
}

impl SyntaxKindExt for SyntaxKind {
    fn is_trivia(self) -> bool {
        matches!(self, SyntaxKind::WHITESPACE | SyntaxKind::COMMENT)
    }
} 