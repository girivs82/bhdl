//! Equation parsing and evaluation engine for stdlib-defined SPICE models
//! 
//! This module provides a simple expression language for defining component
//! equations in stdlib BHDL files, enabling custom models and vendor libraries.

use std::collections::HashMap;
use anyhow::{Result, Context, bail};

/// AST node for equation expressions
#[derive(Debug, Clone)]
pub enum EquationAst {
    /// Numeric literal
    Number(f64),
    /// Variable reference
    Variable(String),
    /// Binary operation
    BinaryOp {
        op: BinaryOperator,
        left: Box<EquationAst>,
        right: Box<EquationAst>,
    },
    /// Unary operation
    UnaryOp {
        op: UnaryOperator,
        expr: Box<EquationAst>,
    },
    /// Function call
    FunctionCall {
        name: String,
        args: Vec<EquationAst>,
    },
    /// Conditional expression
    Conditional {
        condition: Box<EquationAst>,
        then_expr: Box<EquationAst>,
        else_expr: Box<EquationAst>,
    },
    /// Let binding
    Let {
        name: String,
        value: Box<EquationAst>,
        body: Box<EquationAst>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryOperator {
    Add, Sub, Mul, Div, Pow,
    Gt, Lt, Gte, Lte, Eq, Neq,
    And, Or,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOperator {
    Neg, Not,
}

/// Simple tokenizer for equation expressions
#[derive(Debug)]
struct Token {
    kind: TokenKind,
    text: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TokenKind {
    Number, Ident, 
    Plus, Minus, Star, Slash, Caret,
    LParen, RParen, LBrace, RBrace,
    Gt, Lt, Gte, Lte, Eq, Neq, Assign,
    AndAnd, OrOr, Not,
    If, Else, Let, Semicolon,
    Question, Colon, Comma,
    Eof,
}

/// Tokenizer for equation strings
struct Tokenizer {
    input: String,
    pos: usize,
}

impl Tokenizer {
    fn new(input: &str) -> Self {
        Self {
            input: input.to_string(),
            pos: 0,
        }
    }
    
    fn peek_char(&self) -> Option<char> {
        self.input.chars().nth(self.pos)
    }
    
    fn advance(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }
    
    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }
    
    fn read_number(&mut self) -> String {
        let mut num = String::new();
        while let Some(ch) = self.peek_char() {
            if ch.is_numeric() || ch == '.' || ch == 'e' || ch == 'E' || ch == '-' || ch == '+' {
                num.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        num
    }
    
    fn read_ident(&mut self) -> String {
        let mut ident = String::new();
        while let Some(ch) = self.peek_char() {
            if ch.is_alphanumeric() || ch == '_' {
                ident.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        ident
    }
    
    fn next_token(&mut self) -> Result<Token> {
        self.skip_whitespace();
        
        if self.pos >= self.input.len() {
            return Ok(Token { kind: TokenKind::Eof, text: String::new() });
        }
        
        let start_pos = self.pos;
        
        match self.peek_char() {
            Some(ch) if ch.is_numeric() => {
                let num = self.read_number();
                Ok(Token { kind: TokenKind::Number, text: num })
            }
            Some(ch) if ch.is_alphabetic() || ch == '_' => {
                let ident = self.read_ident();
                let kind = match ident.as_str() {
                    "if" => TokenKind::If,
                    "else" => TokenKind::Else,
                    "let" => TokenKind::Let,
                    _ => TokenKind::Ident,
                };
                Ok(Token { kind, text: ident })
            }
            Some('+') => {
                self.advance();
                Ok(Token { kind: TokenKind::Plus, text: "+".to_string() })
            }
            Some('-') => {
                self.advance();
                Ok(Token { kind: TokenKind::Minus, text: "-".to_string() })
            }
            Some('*') => {
                self.advance();
                Ok(Token { kind: TokenKind::Star, text: "*".to_string() })
            }
            Some('/') => {
                self.advance();
                Ok(Token { kind: TokenKind::Slash, text: "/".to_string() })
            }
            Some('^') => {
                self.advance();
                Ok(Token { kind: TokenKind::Caret, text: "^".to_string() })
            }
            Some('(') => {
                self.advance();
                Ok(Token { kind: TokenKind::LParen, text: "(".to_string() })
            }
            Some(')') => {
                self.advance();
                Ok(Token { kind: TokenKind::RParen, text: ")".to_string() })
            }
            Some('{') => {
                self.advance();
                Ok(Token { kind: TokenKind::LBrace, text: "{".to_string() })
            }
            Some('}') => {
                self.advance();
                Ok(Token { kind: TokenKind::RBrace, text: "}".to_string() })
            }
            Some(';') => {
                self.advance();
                Ok(Token { kind: TokenKind::Semicolon, text: ";".to_string() })
            }
            Some('>') => {
                self.advance();
                if self.peek_char() == Some('=') {
                    self.advance();
                    Ok(Token { kind: TokenKind::Gte, text: ">=".to_string() })
                } else {
                    Ok(Token { kind: TokenKind::Gt, text: ">".to_string() })
                }
            }
            Some('<') => {
                self.advance();
                if self.peek_char() == Some('=') {
                    self.advance();
                    Ok(Token { kind: TokenKind::Lte, text: "<=".to_string() })
                } else {
                    Ok(Token { kind: TokenKind::Lt, text: "<".to_string() })
                }
            }
            Some('=') => {
                self.advance();
                if self.peek_char() == Some('=') {
                    self.advance();
                    Ok(Token { kind: TokenKind::Eq, text: "==".to_string() })
                } else {
                    // Single '=' only binds in `let name = ...`; the parser
                    // rejects it anywhere a comparison was meant.
                    Ok(Token { kind: TokenKind::Assign, text: "=".to_string() })
                }
            }
            Some('!') => {
                self.advance();
                if self.peek_char() == Some('=') {
                    self.advance();
                    Ok(Token { kind: TokenKind::Neq, text: "!=".to_string() })
                } else {
                    Ok(Token { kind: TokenKind::Not, text: "!".to_string() })
                }
            }
            Some('&') => {
                self.advance();
                if self.peek_char() == Some('&') {
                    self.advance();
                    Ok(Token { kind: TokenKind::AndAnd, text: "&&".to_string() })
                } else {
                    bail!("Single '&' not allowed, use '&&' for logical AND")
                }
            }
            Some('|') => {
                self.advance();
                if self.peek_char() == Some('|') {
                    self.advance();
                    Ok(Token { kind: TokenKind::OrOr, text: "||".to_string() })
                } else {
                    bail!("Single '|' not allowed, use '||' for logical OR")
                }
            }
            Some('?') => {
                self.advance();
                Ok(Token { kind: TokenKind::Question, text: "?".to_string() })
            }
            Some(':') => {
                self.advance();
                Ok(Token { kind: TokenKind::Colon, text: ":".to_string() })
            }
            Some(',') => {
                self.advance();
                Ok(Token { kind: TokenKind::Comma, text: ",".to_string() })
            }
            Some(ch) => bail!("Unexpected character: {}", ch),
            None => Ok(Token { kind: TokenKind::Eof, text: String::new() }),
        }
    }
}

/// Recursive descent parser for equations
struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    eof_token: Token,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { 
            tokens, 
            pos: 0,
            eof_token: Token { kind: TokenKind::Eof, text: String::new() }
        }
    }
    
    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&self.eof_token)
    }
    
    fn advance(&mut self) -> &Token {
        if self.pos < self.tokens.len() {
            let pos = self.pos;
            self.pos += 1;
            &self.tokens[pos]
        } else {
            &self.eof_token
        }
    }
    
    fn expect(&mut self, kind: TokenKind) -> Result<&Token> {
        let token = self.advance();
        if token.kind != kind {
            bail!("Expected {:?}, found {:?}", kind, token.kind);
        }
        Ok(token)
    }
    
    /// Parse top-level expression
    fn parse_expr(&mut self) -> Result<EquationAst> {
        self.parse_conditional()
    }
    
    /// Parse conditional expression: if cond { then } else { else } or cond ? then : else
    fn parse_conditional(&mut self) -> Result<EquationAst> {
        if self.peek().kind == TokenKind::If {
            self.advance(); // consume 'if'
            let condition = Box::new(self.parse_or()?);
            self.expect(TokenKind::LBrace)?;
            let then_expr = Box::new(self.parse_expr()?);
            self.expect(TokenKind::RBrace)?;
            self.expect(TokenKind::Else)?;
            self.expect(TokenKind::LBrace)?;
            let else_expr = Box::new(self.parse_expr()?);
            self.expect(TokenKind::RBrace)?;
            Ok(EquationAst::Conditional { condition, then_expr, else_expr })
        } else if self.peek().kind == TokenKind::Let {
            self.advance(); // consume 'let'
            let name = match self.advance() {
                Token { kind: TokenKind::Ident, text } => text.clone(),
                _ => bail!("Expected identifier after 'let'"),
            };
            self.expect(TokenKind::Assign)?;
            let value = Box::new(self.parse_expr()?);
            self.expect(TokenKind::Semicolon)?;
            let body = Box::new(self.parse_expr()?);
            Ok(EquationAst::Let { name, value, body })
        } else {
            self.parse_ternary()
        }
    }
    
    /// Parse ternary operator: cond ? then : else
    fn parse_ternary(&mut self) -> Result<EquationAst> {
        let mut expr = self.parse_or()?;
        
        if self.peek().kind == TokenKind::Question {
            self.advance(); // consume '?'
            let then_expr = Box::new(self.parse_ternary()?);
            self.expect(TokenKind::Colon)?;
            let else_expr = Box::new(self.parse_ternary()?);
            expr = EquationAst::Conditional {
                condition: Box::new(expr),
                then_expr,
                else_expr,
            };
        }
        
        Ok(expr)
    }
    
    /// Parse logical OR
    fn parse_or(&mut self) -> Result<EquationAst> {
        let mut left = self.parse_and()?;
        while self.peek().kind == TokenKind::OrOr {
            self.advance();
            let right = self.parse_and()?;
            left = EquationAst::BinaryOp {
                op: BinaryOperator::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }
    
    /// Parse logical AND
    fn parse_and(&mut self) -> Result<EquationAst> {
        let mut left = self.parse_comparison()?;
        while self.peek().kind == TokenKind::AndAnd {
            self.advance();
            let right = self.parse_comparison()?;
            left = EquationAst::BinaryOp {
                op: BinaryOperator::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }
    
    /// Parse comparison operators
    fn parse_comparison(&mut self) -> Result<EquationAst> {
        let mut left = self.parse_additive()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Gt => BinaryOperator::Gt,
                TokenKind::Lt => BinaryOperator::Lt,
                TokenKind::Gte => BinaryOperator::Gte,
                TokenKind::Lte => BinaryOperator::Lte,
                TokenKind::Eq => BinaryOperator::Eq,
                TokenKind::Neq => BinaryOperator::Neq,
                _ => break,
            };
            self.advance();
            let right = self.parse_additive()?;
            left = EquationAst::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }
    
    /// Parse addition and subtraction
    fn parse_additive(&mut self) -> Result<EquationAst> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Plus => BinaryOperator::Add,
                TokenKind::Minus => BinaryOperator::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative()?;
            left = EquationAst::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }
    
    /// Parse multiplication and division
    fn parse_multiplicative(&mut self) -> Result<EquationAst> {
        let mut left = self.parse_power()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Star => BinaryOperator::Mul,
                TokenKind::Slash => BinaryOperator::Div,
                _ => break,
            };
            self.advance();
            let right = self.parse_power()?;
            left = EquationAst::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }
    
    /// Parse power operator
    fn parse_power(&mut self) -> Result<EquationAst> {
        let mut left = self.parse_unary()?;
        while self.peek().kind == TokenKind::Caret {
            self.advance();
            let right = self.parse_unary()?;
            left = EquationAst::BinaryOp {
                op: BinaryOperator::Pow,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }
    
    /// Parse unary operators
    fn parse_unary(&mut self) -> Result<EquationAst> {
        match self.peek().kind {
            TokenKind::Minus => {
                self.advance();
                Ok(EquationAst::UnaryOp {
                    op: UnaryOperator::Neg,
                    expr: Box::new(self.parse_unary()?),
                })
            }
            TokenKind::Not => {
                self.advance();
                Ok(EquationAst::UnaryOp {
                    op: UnaryOperator::Not,
                    expr: Box::new(self.parse_unary()?),
                })
            }
            _ => self.parse_primary(),
        }
    }
    
    /// Parse primary expressions
    fn parse_primary(&mut self) -> Result<EquationAst> {
        match self.peek().kind {
            TokenKind::Number => {
                let token = self.advance();
                let value = token.text.parse::<f64>()
                    .context("Failed to parse number")?;
                Ok(EquationAst::Number(value))
            }
            TokenKind::Ident => {
                let name = self.advance().text.clone();
                if self.peek().kind == TokenKind::LParen {
                    // Function call
                    self.advance(); // consume '('
                    let mut args = Vec::new();
                    
                    // Handle empty argument list
                    if self.peek().kind == TokenKind::RParen {
                        self.advance();
                        return Ok(EquationAst::FunctionCall { name, args });
                    }
                    
                    // Parse arguments
                    loop {
                        args.push(self.parse_expr()?);
                        
                        if self.peek().kind == TokenKind::Comma {
                            self.advance(); // consume ','
                        } else {
                            break;
                        }
                    }
                    
                    self.expect(TokenKind::RParen)?;
                    Ok(EquationAst::FunctionCall { name, args })
                } else {
                    // Variable reference
                    Ok(EquationAst::Variable(name))
                }
            }
            TokenKind::LParen => {
                self.advance(); // consume '('
                let expr = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                Ok(expr)
            }
            _ => bail!("Unexpected token: {:?}", self.peek()),
        }
    }
}

/// Equation evaluation engine
pub struct EquationEngine {
    /// Parsed equations by name
    equations: HashMap<String, EquationAst>,
}

impl EquationEngine {
    pub fn new() -> Self {
        Self {
            equations: HashMap::new(),
        }
    }
    
    /// Parse an equation string and store it
    pub fn parse_equation(&mut self, name: &str, equation: &str) -> Result<()> {
        // Tokenize
        let mut tokenizer = Tokenizer::new(equation);
        let mut tokens = Vec::new();
        loop {
            let token = tokenizer.next_token()?;
            if token.kind == TokenKind::Eof {
                tokens.push(token);
                break;
            }
            tokens.push(token);
        }
        
        // Parse
        let mut parser = Parser::new(tokens);
        let ast = parser.parse_expr()?;
        match parser.peek().kind {
            TokenKind::Eof => {}
            TokenKind::Assign => bail!("Single '=' not allowed, use '==' for equality"),
            kind => bail!("Unexpected trailing token {:?} in equation", kind),
        }

        self.equations.insert(name.to_string(), ast);
        Ok(())
    }
    
    /// Evaluate an equation with given variable bindings
    pub fn evaluate(&self, equation_name: &str, vars: &HashMap<String, f64>) -> Result<f64> {
        let ast = self.equations.get(equation_name)
            .ok_or_else(|| anyhow::anyhow!("Equation '{}' not found", equation_name))?;
        self.eval_ast(ast, vars, &HashMap::new())
    }
    
    /// Evaluate an AST node
    fn eval_ast(&self, ast: &EquationAst, vars: &HashMap<String, f64>, locals: &HashMap<String, f64>) -> Result<f64> {
        match ast {
            EquationAst::Number(n) => Ok(*n),
            EquationAst::Variable(name) => {
                locals.get(name)
                    .or_else(|| vars.get(name))
                    .copied()
                    .ok_or_else(|| anyhow::anyhow!("Variable '{}' not found", name))
            }
            EquationAst::BinaryOp { op, left, right } => {
                let l = self.eval_ast(left, vars, locals)?;
                let r = self.eval_ast(right, vars, locals)?;
                Ok(match op {
                    BinaryOperator::Add => l + r,
                    BinaryOperator::Sub => l - r,
                    BinaryOperator::Mul => l * r,
                    BinaryOperator::Div => l / r,
                    BinaryOperator::Pow => l.powf(r),
                    BinaryOperator::Gt => if l > r { 1.0 } else { 0.0 },
                    BinaryOperator::Lt => if l < r { 1.0 } else { 0.0 },
                    BinaryOperator::Gte => if l >= r { 1.0 } else { 0.0 },
                    BinaryOperator::Lte => if l <= r { 1.0 } else { 0.0 },
                    BinaryOperator::Eq => if (l - r).abs() < 1e-10 { 1.0 } else { 0.0 },
                    BinaryOperator::Neq => if (l - r).abs() >= 1e-10 { 1.0 } else { 0.0 },
                    BinaryOperator::And => if l != 0.0 && r != 0.0 { 1.0 } else { 0.0 },
                    BinaryOperator::Or => if l != 0.0 || r != 0.0 { 1.0 } else { 0.0 },
                })
            }
            EquationAst::UnaryOp { op, expr } => {
                let val = self.eval_ast(expr, vars, locals)?;
                Ok(match op {
                    UnaryOperator::Neg => -val,
                    UnaryOperator::Not => if val == 0.0 { 1.0 } else { 0.0 },
                })
            }
            EquationAst::FunctionCall { name, args } => {
                let arg_vals: Result<Vec<f64>> = args.iter()
                    .map(|arg| self.eval_ast(arg, vars, locals))
                    .collect();
                let arg_vals = arg_vals?;
                
                match name.as_str() {
                    "exp" => Ok(arg_vals[0].exp()),
                    "log" => Ok(arg_vals[0].ln()),
                    "sqrt" => Ok(arg_vals[0].sqrt()),
                    "abs" => Ok(arg_vals[0].abs()),
                    "min" => Ok(arg_vals.iter().fold(f64::INFINITY, |a, &b| a.min(b))),
                    "max" => Ok(arg_vals.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b))),
                    "sin" => Ok(arg_vals[0].sin()),
                    "cos" => Ok(arg_vals[0].cos()),
                    "tan" => Ok(arg_vals[0].tan()),
                    "tanh" => Ok(arg_vals[0].tanh()),
                    _ => bail!("Unknown function: {}", name),
                }
            }
            EquationAst::Conditional { condition, then_expr, else_expr } => {
                let cond_val = self.eval_ast(condition, vars, locals)?;
                if cond_val != 0.0 {
                    self.eval_ast(then_expr, vars, locals)
                } else {
                    self.eval_ast(else_expr, vars, locals)
                }
            }
            EquationAst::Let { name, value, body } => {
                let val = self.eval_ast(value, vars, locals)?;
                let mut new_locals = locals.clone();
                new_locals.insert(name.clone(), val);
                self.eval_ast(body, vars, &new_locals)
            }
        }
    }
    
    /// Get all variables referenced by an equation
    pub fn get_variables(&self, equation_name: &str) -> Vec<String> {
        let mut vars = Vec::new();
        if let Some(ast) = self.equations.get(equation_name) {
            self.collect_variables(ast, &mut vars);
        }
        vars.sort();
        vars.dedup();
        vars
    }
    
    fn collect_variables(&self, ast: &EquationAst, vars: &mut Vec<String>) {
        match ast {
            EquationAst::Variable(name) => vars.push(name.clone()),
            EquationAst::BinaryOp { left, right, .. } => {
                self.collect_variables(left, vars);
                self.collect_variables(right, vars);
            }
            EquationAst::UnaryOp { expr, .. } => {
                self.collect_variables(expr, vars);
            }
            EquationAst::FunctionCall { args, .. } => {
                for arg in args {
                    self.collect_variables(arg, vars);
                }
            }
            EquationAst::Conditional { condition, then_expr, else_expr } => {
                self.collect_variables(condition, vars);
                self.collect_variables(then_expr, vars);
                self.collect_variables(else_expr, vars);
            }
            EquationAst::Let { value, body, .. } => {
                self.collect_variables(value, vars);
                self.collect_variables(body, vars);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simple_arithmetic() {
        let mut engine = EquationEngine::new();
        engine.parse_equation("test", "2 + 3 * 4").unwrap();
        let result = engine.evaluate("test", &HashMap::new()).unwrap();
        assert_eq!(result, 14.0);
    }
    
    #[test]
    fn test_variables() {
        let mut engine = EquationEngine::new();
        engine.parse_equation("ohms_law", "v / r").unwrap();
        
        let mut vars = HashMap::new();
        vars.insert("v".to_string(), 10.0);
        vars.insert("r".to_string(), 5.0);
        
        let result = engine.evaluate("ohms_law", &vars).unwrap();
        assert_eq!(result, 2.0);
    }
    
    #[test]
    fn test_conditional() {
        let mut engine = EquationEngine::new();
        engine.parse_equation("test", "if x > 0 { x * 2 } else { -x }").unwrap();
        
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), 5.0);
        assert_eq!(engine.evaluate("test", &vars).unwrap(), 10.0);
        
        vars.insert("x".to_string(), -3.0);
        assert_eq!(engine.evaluate("test", &vars).unwrap(), 3.0);
    }
    
    #[test]
    fn test_functions() {
        let mut engine = EquationEngine::new();
        engine.parse_equation("test", "exp(min(x, 2))").unwrap();
        
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), 1.0);
        assert!((engine.evaluate("test", &vars).unwrap() - 1.0_f64.exp()).abs() < 1e-10);
        
        vars.insert("x".to_string(), 3.0);
        assert!((engine.evaluate("test", &vars).unwrap() - 2.0_f64.exp()).abs() < 1e-10);
    }
    
    #[test]
    fn test_let_binding() {
        let mut engine = EquationEngine::new();
        engine.parse_equation("test", "let a = x + 1; let b = a * 2; b + a").unwrap();
        
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), 5.0);
        // a = 6, b = 12, result = 18
        assert_eq!(engine.evaluate("test", &vars).unwrap(), 18.0);
    }
}