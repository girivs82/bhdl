mod lexer;
pub mod syntax;
mod parser;

// Re-export key types
pub use syntax::{SyntaxKind, BhdlLanguage};
pub use parser::{ParseResult, ParseError, parse};
// Remove the export for the non-existent lex function
// pub use lexer::lex;

// Optional: Re-export specific AST node types if desired
// pub use ast::BoardDef;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
} 