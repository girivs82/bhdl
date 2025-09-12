//! Pretty-printing functionality for BHDL AST nodes
//! 
//! This module provides traits and implementations for converting AST nodes
//! back to readable BHDL source code.

use crate::flow::{FlowStmt, FlowExpr, FlowElement, ComponentInstantiation, GenerateStmt, ConditionalStmt, AssignStmt, ConditionalExpr};
use crate::common::{ParamAssign, PinRef, NetRef, IdentRef, Value, RangeExpr, BusSuffix};
use crate::v2_statements::ConnectionStmt;
use crate::expr::{Expr, BinaryExpr, PrefixExpr, TernaryExpr, FunctionCallExpr, ComponentInstExpr, FlowExpr as ExprFlowExpr, ArrayExpr, StructLiteral, StructField};
use crate::items::{Board};
use crate::{SyntaxKind, BhdlLanguage, SyntaxNode, HasName};
use rowan::ast::AstNode;
use std::fmt::{self};

/// Configuration for pretty-printing output
#[derive(Debug, Clone)]
pub struct PrettyPrintConfig {
    /// Indentation string (e.g., "  " for 2 spaces, "\t" for tabs)
    pub indent: String,
    /// Maximum line width before wrapping
    pub max_width: usize,
    /// Whether to add extra newlines for readability
    pub extra_spacing: bool,
}

impl Default for PrettyPrintConfig {
    fn default() -> Self {
        Self {
            indent: "  ".to_string(),
            max_width: 100,
            extra_spacing: true,
        }
    }
}

/// Context for pretty-printing, tracks current indentation level
#[derive(Debug)]
pub struct PrettyPrintContext {
    config: PrettyPrintConfig,
    indent_level: usize,
    current_line_length: usize,
}

impl PrettyPrintContext {
    pub fn new(config: PrettyPrintConfig) -> Self {
        Self {
            config,
            indent_level: 0,
            current_line_length: 0,
        }
    }

    pub fn indent(&mut self) {
        self.indent_level += 1;
    }

    pub fn dedent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    pub fn current_indent(&self) -> String {
        self.config.indent.repeat(self.indent_level)
    }

    pub fn write_line(&mut self, f: &mut dyn fmt::Write, text: &str) -> fmt::Result {
        if !text.is_empty() {
            write!(f, "{}{}", self.current_indent(), text)?;
        }
        writeln!(f)?;
        self.current_line_length = 0;
        Ok(())
    }

    pub fn write_text(&mut self, f: &mut dyn fmt::Write, text: &str) -> fmt::Result {
        write!(f, "{}", text)?;
        self.current_line_length += text.len();
        Ok(())
    }

    pub fn needs_line_break(&self, additional_length: usize) -> bool {
        self.current_line_length + additional_length > self.config.max_width
    }
}

/// Trait for pretty-printing AST nodes
pub trait PrettyPrint {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result;
    
    /// Convenience method to pretty-print with default configuration
    fn to_pretty_string(&self) -> String where Self: Sized {
        let mut output = String::new();
        let mut ctx = PrettyPrintContext::new(PrettyPrintConfig::default());
        self.pretty_print(&mut ctx, &mut output).expect("String formatting should not fail");
        output
    }
}

// --- Flow Constructs ---

impl PrettyPrint for FlowStmt {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        if let Some(name) = self.name() {
            ctx.write_text(f, &name.text())?;
            ctx.write_text(f, ": ")?;
        }
        
        if let Some(flow_expr) = self.flow_expr() {
            flow_expr.pretty_print(ctx, f)?;
        }
        
        ctx.write_text(f, ";")?;
        Ok(())
    }
}

impl PrettyPrint for FlowExpr {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        // For flow expressions, we need to reconstruct the flow chain
        // This is complex since we need to traverse the binary expression tree
        if let Some(expr) = self.as_expr() {
            expr.pretty_print(ctx, f)?;
        } else {
            // Fallback: print elements separated by flow operators
            let elements: Vec<_> = self.elements().collect();
            for (i, element) in elements.iter().enumerate() {
                if i > 0 {
                    ctx.write_text(f, " |> ")?;
                }
                pretty_print_flow_element(&element, ctx, f)?;
            }
        }
        Ok(())
    }
}

impl PrettyPrint for ComponentInstantiation {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        if let Some(comp_type) = self.component_type() {
            ctx.write_text(f, &comp_type.text())?;
        }
        
        // Print parameters
        if let Some(params) = self.parameters() {
            ctx.write_text(f, "(")?;
            let assignments: Vec<_> = params.assignments().collect();
            for (i, assignment) in assignments.iter().enumerate() {
                if i > 0 {
                    ctx.write_text(f, ", ")?;
                }
                assignment.pretty_print(ctx, f)?;
            }
            ctx.write_text(f, ")")?;
        }
        
        // Print pin access
        if let Some(pin_access) = self.pin_access() {
            ctx.write_text(f, ".")?;
            ctx.write_text(f, &pin_access.text())?;
        }
        
        Ok(())
    }
}

impl PrettyPrint for GenerateStmt {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        ctx.write_text(f, "generate for ")?;
        
        if let Some(loop_var) = self.loop_variable() {
            ctx.write_text(f, &loop_var.text())?;
        }
        
        ctx.write_text(f, " in ")?;
        
        if let Some(range) = self.range() {
            range.pretty_print(ctx, f)?;
        }
        
        ctx.write_text(f, " {")?;
        
        if ctx.config.extra_spacing {
            ctx.write_text(f, "\n")?;
        }
        
        ctx.indent();
        for stmt in self.body_statements() {
            ctx.write_text(f, &ctx.current_indent())?;
            // We need to determine the statement type and print accordingly
            pretty_print_statement_node(&stmt, ctx, f)?;
            ctx.write_text(f, "\n")?;
        }
        ctx.dedent();
        
        ctx.write_text(f, &ctx.current_indent())?;
        ctx.write_text(f, "}")?;
        
        Ok(())
    }
}

impl PrettyPrint for ConditionalStmt {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        ctx.write_text(f, "if (")?;
        
        if let Some(condition) = self.condition() {
            condition.pretty_print(ctx, f)?;
        }
        
        ctx.write_text(f, ") {")?;
        
        if ctx.config.extra_spacing {
            ctx.write_text(f, "\n")?;
        }
        
        ctx.indent();
        for stmt in self.if_statements() {
            ctx.write_text(f, &ctx.current_indent())?;
            pretty_print_statement_node(&stmt, ctx, f)?;
            ctx.write_text(f, "\n")?;
        }
        ctx.dedent();
        
        ctx.write_text(f, &ctx.current_indent())?;
        ctx.write_text(f, "}")?;
        
        if self.has_else() {
            ctx.write_text(f, " else {")?;
            if ctx.config.extra_spacing {
                ctx.write_text(f, "\n")?;
            }
            
            ctx.indent();
            for stmt in self.else_statements() {
                ctx.write_text(f, &ctx.current_indent())?;
                pretty_print_statement_node(&stmt, ctx, f)?;
                ctx.write_text(f, "\n")?;
            }
            ctx.dedent();
            
            ctx.write_text(f, &ctx.current_indent())?;
            ctx.write_text(f, "}")?;
        }
        
        Ok(())
    }
}

impl PrettyPrint for AssignStmt {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        if let Some(variable) = self.variable() {
            ctx.write_text(f, &variable.text())?;
        }
        
        ctx.write_text(f, " = ")?;
        
        if let Some(value) = self.value() {
            value.pretty_print(ctx, f)?;
        }
        
        ctx.write_text(f, ";")?;
        
        Ok(())
    }
}

impl PrettyPrint for ConditionalExpr {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        ctx.write_text(f, "if (")?;
        
        if let Some(condition) = self.condition() {
            condition.pretty_print(ctx, f)?;
        }
        
        ctx.write_text(f, ") ")?;
        
        if let Some(then_expr) = self.then_expr() {
            then_expr.pretty_print(ctx, f)?;
        }
        
        if let Some(else_expr) = self.else_expr() {
            ctx.write_text(f, " : ")?;
            else_expr.pretty_print(ctx, f)?;
        }
        
        Ok(())
    }
}

// --- Expressions ---

impl PrettyPrint for Expr {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        match self {
            Expr::Value(value) => value.pretty_print(ctx, f),
            Expr::IdentRef(ident_ref) => ident_ref.pretty_print(ctx, f),
            Expr::NetRef(net_ref) => net_ref.pretty_print(ctx, f),
            Expr::PinRef(pin_ref) => pin_ref.pretty_print(ctx, f),
            Expr::PrefixExpr(prefix_expr) => prefix_expr.pretty_print(ctx, f),
            Expr::BinaryExpr(binary_expr) => binary_expr.pretty_print(ctx, f),
            Expr::TernaryExpr(ternary_expr) => ternary_expr.pretty_print(ctx, f),
            Expr::FunctionCallExpr(func_call) => func_call.pretty_print(ctx, f),
            Expr::FlowExpr(flow_expr) => {
                // This is the expression version of FlowExpr
                flow_expr.pretty_print(ctx, f)
            }
            Expr::ComponentInstExpr(comp_inst) => comp_inst.pretty_print(ctx, f),
            Expr::ArrayExpr(array_expr) => array_expr.pretty_print(ctx, f),
            Expr::StructLiteral(struct_literal) => struct_literal.pretty_print(ctx, f),
            Expr::Ident(node) => ctx.write_text(f, &node.text().to_string()),
            Expr::Literal(node) => ctx.write_text(f, &node.text().to_string()),
        }
    }
}

impl PrettyPrint for BinaryExpr {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        if let Some(lhs) = self.lhs() {
            lhs.pretty_print(ctx, f)?;
        }
        
        ctx.write_text(f, " ")?;
        
        if let Some(op) = self.op() {
            let op_str = match op {
                SyntaxKind::PLUS => "+",
                SyntaxKind::MINUS => "-",
                SyntaxKind::STAR => "*",
                SyntaxKind::SLASH => "/",
                SyntaxKind::AMPERSAND => "&",
                SyntaxKind::PIPE => "|",
                SyntaxKind::CARET => "^",
                SyntaxKind::EQEQ => "==",
                SyntaxKind::NEQ => "!=",
                SyntaxKind::L_ANGLE => "<",
                SyntaxKind::LTEQ => "<=",
                SyntaxKind::R_ANGLE => ">",
                SyntaxKind::GTEQ => ">=",
                SyntaxKind::ARROW => "->",
                SyntaxKind::BI_ARROW => "<->",
                SyntaxKind::FLOW_OP => "|>",
                SyntaxKind::INTERFACE_OP => "<=>",
                _ => "?",
            };
            ctx.write_text(f, op_str)?;
        }
        
        ctx.write_text(f, " ")?;
        
        if let Some(rhs) = self.rhs() {
            rhs.pretty_print(ctx, f)?;
        }
        
        Ok(())
    }
}

impl PrettyPrint for PrefixExpr {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        if let Some(op) = self.op() {
            let op_str = match op {
                SyntaxKind::MINUS => "-",
                SyntaxKind::PLUS => "+",
                SyntaxKind::BANG => "!",
                SyntaxKind::TILDE => "~",
                _ => "?",
            };
            ctx.write_text(f, op_str)?;
        }
        
        if let Some(expr) = self.expr() {
            expr.pretty_print(ctx, f)?;
        }
        
        Ok(())
    }
}

impl PrettyPrint for TernaryExpr {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        if let Some(condition) = self.condition() {
            condition.pretty_print(ctx, f)?;
        }
        
        ctx.write_text(f, " ? ")?;
        
        if let Some(true_expr) = self.true_expr() {
            true_expr.pretty_print(ctx, f)?;
        }
        
        ctx.write_text(f, " : ")?;
        
        if let Some(false_expr) = self.false_expr() {
            false_expr.pretty_print(ctx, f)?;
        }
        
        Ok(())
    }
}

impl PrettyPrint for FunctionCallExpr {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        if let Some(func_name) = self.function_name() {
            ctx.write_text(f, &func_name.text())?;
        }
        
        ctx.write_text(f, "(")?;
        
        let args: Vec<_> = self.arguments().collect();
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                ctx.write_text(f, ", ")?;
            }
            arg.pretty_print(ctx, f)?;
        }
        
        ctx.write_text(f, ")")?;
        
        Ok(())
    }
}

impl PrettyPrint for ComponentInstExpr {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        if let Some(comp_type) = self.component_type() {
            ctx.write_text(f, &comp_type.text())?;
        }
        
        ctx.write_text(f, "(")?;
        
        let params: Vec<_> = self.parameters().collect();
        for (i, param) in params.iter().enumerate() {
            if i > 0 {
                ctx.write_text(f, ", ")?;
            }
            param.pretty_print(ctx, f)?;
        }
        
        ctx.write_text(f, ")")?;
        
        Ok(())
    }
}

impl PrettyPrint for ExprFlowExpr {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        let elements: Vec<_> = self.elements().collect();
        for (i, element) in elements.iter().enumerate() {
            if i > 0 {
                ctx.write_text(f, " |> ")?;
            }
            element.pretty_print(ctx, f)?;
        }
        Ok(())
    }
}

// --- Common constructs ---

impl PrettyPrint for Value {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        if let Some(token) = self.syntax().first_token() {
            ctx.write_text(f, &token.text())?;
        }
        Ok(())
    }
}

impl PrettyPrint for IdentRef {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        if let Some(token) = self.token() {
            ctx.write_text(f, &token.text())?;
        }
        Ok(())
    }
}

impl PrettyPrint for NetRef {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        // Always print with @ prefix
        ctx.write_text(f, "@")?;
        if let Some(name) = self.name() {
            ctx.write_text(f, &name)?;
        }
        Ok(())
    }
}

impl PrettyPrint for ParamAssign {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        if let Some(name) = self.name() {
            ctx.write_text(f, &name.text())?;
            ctx.write_text(f, " = ")?;
        }
        
        if let Some(value) = self.value() {
            value.pretty_print(ctx, f)?;
        }
        
        Ok(())
    }
}

// Add implementations for RangeExpr, ConnectionStmt, etc.

impl PrettyPrint for RangeExpr {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        if let Some(lhs) = self.lhs() {
            lhs.pretty_print(ctx, f)?;
        }
        
        ctx.write_text(f, "..")?;
        
        if let Some(rhs) = self.rhs() {
            rhs.pretty_print(ctx, f)?;
        }
        
        Ok(())
    }
}

impl PrettyPrint for ConnectionStmt {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        // v2.0 connection statements are flow-based, not using connect keyword
        if let Some(expr) = self.expr() {
            // The expression contains the entire connection flow
            // For now, just output the text
            ctx.write_text(f, &expr.text().to_string())?;
        } else {
            // Fallback to raw text
            ctx.write_text(f, &self.text())?;
        }
        
        Ok(())
    }
}

// --- Board and high-level constructs ---

impl PrettyPrint for Board {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        ctx.write_text(f, "board ")?;
        
        if let Some(name) = self.name() {
            ctx.write_text(f, &name.text())?;
        }
        
        ctx.write_text(f, " {")?;
        
        if ctx.config.extra_spacing {
            ctx.write_text(f, "\n")?;
        }
        
        ctx.indent();
        
        // Print board contents
        for child in self.syntax().children() {
            ctx.write_text(f, &ctx.current_indent())?;
            pretty_print_board_item(&child, ctx, f)?;
            ctx.write_text(f, "\n")?;
            
            if ctx.config.extra_spacing {
                ctx.write_text(f, "\n")?;
            }
        }
        
        ctx.dedent();
        ctx.write_text(f, "}")?;
        
        Ok(())
    }
}

// --- Helper functions ---

/// Pretty-print a flow element
fn pretty_print_flow_element(element: &FlowElement, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
    match element {
        FlowElement::Identifier(token) => {
            ctx.write_text(f, &token.text())?;
        }
        FlowElement::ComponentInstantiation(comp_inst) => {
            comp_inst.pretty_print(ctx, f)?;
        }
        FlowElement::ConditionalExpr(cond_expr) => {
            cond_expr.pretty_print(ctx, f)?;
        }
    }
    Ok(())
}

/// Pretty-print a statement node based on its syntax kind
fn pretty_print_statement_node(node: &SyntaxNode<BhdlLanguage>, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
    match node.kind() {
        SyntaxKind::FLOW_STMT => {
            if let Some(flow_stmt) = FlowStmt::cast(node.clone()) {
                flow_stmt.pretty_print(ctx, f)?;
            }
        }
        SyntaxKind::CONNECTION_STMT => {
            if let Some(conn_stmt) = ConnectionStmt::cast(node.clone()) {
                conn_stmt.pretty_print(ctx, f)?;
            }
        }
        SyntaxKind::ASSIGN_STMT => {
            if let Some(assign_stmt) = AssignStmt::cast(node.clone()) {
                assign_stmt.pretty_print(ctx, f)?;
            }
        }
        _ => {
            // Fallback: print the raw text
            ctx.write_text(f, &node.to_string())?;
        }
    }
    Ok(())
}

/// Pretty-print a reference node (PinRef, NetRef, etc.)
fn pretty_print_reference_node(node: &SyntaxNode<BhdlLanguage>, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
    match node.kind() {
        SyntaxKind::PIN_REF => {
            if let Some(pin_ref) = PinRef::cast(node.clone()) {
                pin_ref.pretty_print(ctx, f)?;
            }
        }
        SyntaxKind::NET_REF => {
            if let Some(net_ref) = NetRef::cast(node.clone()) {
                net_ref.pretty_print(ctx, f)?;
            }
        }
        SyntaxKind::IDENT_REF => {
            if let Some(ident_ref) = IdentRef::cast(node.clone()) {
                ident_ref.pretty_print(ctx, f)?;
            }
        }
        _ => {
            // Fallback: print the raw text
            ctx.write_text(f, &node.to_string())?;
        }
    }
    Ok(())
}

/// Pretty-print a board item
fn pretty_print_board_item(node: &SyntaxNode<BhdlLanguage>, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
    match node.kind() {
        SyntaxKind::FLOW_STMT => {
            if let Some(flow_stmt) = FlowStmt::cast(node.clone()) {
                flow_stmt.pretty_print(ctx, f)?;
            }
        }
        SyntaxKind::GENERATE_STMT => {
            if let Some(generate_stmt) = GenerateStmt::cast(node.clone()) {
                generate_stmt.pretty_print(ctx, f)?;
            }
        }
        SyntaxKind::CONDITIONAL_STMT => {
            if let Some(conditional_stmt) = ConditionalStmt::cast(node.clone()) {
                conditional_stmt.pretty_print(ctx, f)?;
            }
        }
        SyntaxKind::CONNECTION_STMT => {
            if let Some(conn_stmt) = ConnectionStmt::cast(node.clone()) {
                conn_stmt.pretty_print(ctx, f)?;
            }
        }
        SyntaxKind::ASSIGN_STMT => {
            if let Some(assign_stmt) = AssignStmt::cast(node.clone()) {
                assign_stmt.pretty_print(ctx, f)?;
            }
        }
        _ => {
            // Handle other board items like blocks, etc.
            ctx.write_text(f, &node.to_string())?;
        }
    }
    Ok(())
}

impl PrettyPrint for PinRef {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        if let Some(instance) = self.instance_name() {
            ctx.write_text(f, &instance.text())?;
            ctx.write_text(f, ".")?;
        }
        
        if let Some(pin) = self.pin_name() {
            ctx.write_text(f, &pin.text())?;
        }
        
        if let Some(bus_suffix) = self.bus_suffix() {
            bus_suffix.pretty_print(ctx, f)?;
        }
        
        Ok(())
    }
}


// Remove duplicate BusSuffix import

impl PrettyPrint for BusSuffix {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        ctx.write_text(f, "[")?;
        
        if let Some(range) = self.range() {
            range.pretty_print(ctx, f)?;
        } else if let Some(index) = self.index_expr() {
            index.pretty_print(ctx, f)?;
        }
        
        ctx.write_text(f, "]")?;
        
        Ok(())
    }
}

// --- Array Expression and Struct Literal pretty printing ---

impl PrettyPrint for ArrayExpr {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        if self.is_tuple() {
            ctx.write_text(f, "(")?;
        } else {
            ctx.write_text(f, "[")?;
        }
        
        let elements: Vec<_> = self.elements().collect();
        for (i, element) in elements.iter().enumerate() {
            if i > 0 {
                ctx.write_text(f, ", ")?;
            }
            element.pretty_print(ctx, f)?;
        }
        
        if self.is_tuple() {
            ctx.write_text(f, ")")?;
        } else {
            ctx.write_text(f, "]")?;
        }
        
        Ok(())
    }
}

impl PrettyPrint for StructLiteral {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        ctx.write_text(f, "{ ")?;
        
        let fields: Vec<_> = self.fields().collect();
        for (i, field) in fields.iter().enumerate() {
            if i > 0 {
                ctx.write_text(f, ", ")?;
            }
            field.pretty_print(ctx, f)?;
        }
        
        ctx.write_text(f, " }")?;
        Ok(())
    }
}

impl PrettyPrint for StructField {
    fn pretty_print(&self, ctx: &mut PrettyPrintContext, f: &mut dyn fmt::Write) -> fmt::Result {
        if let Some(name) = self.name() {
            ctx.write_text(f, &name.text().to_string())?;
        }
        ctx.write_text(f, ": ")?;
        if let Some(value) = self.value() {
            value.pretty_print(ctx, f)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::*;
    use crate::common::*;
    
    // Helper function to create test tokens (simplified)
    fn create_test_context() -> PrettyPrintContext {
        PrettyPrintContext::new(PrettyPrintConfig::default())
    }
    
    #[test]
    fn test_pretty_print_config() {
        let config = PrettyPrintConfig {
            indent: "\t".to_string(),
            max_width: 80,
            extra_spacing: false,
        };
        
        let ctx = PrettyPrintContext::new(config);
        assert_eq!(ctx.config.indent, "\t");
        assert_eq!(ctx.config.max_width, 80);
        assert!(!ctx.config.extra_spacing);
    }
    
    #[test]
    fn test_context_indentation() {
        let mut ctx = create_test_context();
        assert_eq!(ctx.current_indent(), "");
        
        ctx.indent();
        assert_eq!(ctx.current_indent(), "  ");
        
        ctx.indent();
        assert_eq!(ctx.current_indent(), "    ");
        
        ctx.dedent();
        assert_eq!(ctx.current_indent(), "  ");
        
        ctx.dedent();
        assert_eq!(ctx.current_indent(), "");
        
        // Should not go negative
        ctx.dedent();
        assert_eq!(ctx.current_indent(), "");
    }
}