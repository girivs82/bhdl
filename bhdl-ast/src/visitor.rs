//! Visitor pattern for traversing BHDL AST nodes

use crate::flow::{FlowStmt, FlowExpr, ComponentInstantiation, GenerateStmt, ConditionalStmt, AssignStmt};
use crate::v2_statements::ConnectionStmt;
use crate::expr::{Expr, BinaryExpr};
use crate::items::{Board, Module, ComponentDef, InterfaceDef};
use crate::{SyntaxNode, BhdlLanguage};
use rowan::ast::AstNode;

/// Base visitor trait for traversing AST nodes
pub trait AstVisitor {
    /// Visit the root source file
    fn visit_source_file(&mut self, node: &SyntaxNode<BhdlLanguage>) {
        self.walk_source_file(node);
    }

    /// Visit a board definition
    fn visit_board(&mut self, board: &Board) {
        self.walk_board(board);
    }

    /// Visit a module definition
    fn visit_module(&mut self, module: &Module) {
        self.walk_module(module);
    }

    /// Visit a component definition
    fn visit_component_def(&mut self, comp_def: &ComponentDef) {
        self.walk_component_def(comp_def);
    }

    /// Visit an interface definition
    fn visit_interface_def(&mut self, interface_def: &InterfaceDef) {
        self.walk_interface_def(interface_def);
    }

    /// Visit a flow statement
    fn visit_flow_stmt(&mut self, flow_stmt: &FlowStmt) {
        self.walk_flow_stmt(flow_stmt);
    }

    /// Visit a flow expression
    fn visit_flow_expr(&mut self, flow_expr: &FlowExpr) {
        self.walk_flow_expr(flow_expr);
    }

    /// Visit a component instantiation
    fn visit_component_instantiation(&mut self, comp_inst: &ComponentInstantiation) {
        self.walk_component_instantiation(comp_inst);
    }

    /// Visit a generate statement
    fn visit_generate_stmt(&mut self, generate_stmt: &GenerateStmt) {
        self.walk_generate_stmt(generate_stmt);
    }

    /// Visit a conditional statement
    fn visit_conditional_stmt(&mut self, conditional_stmt: &ConditionalStmt) {
        self.walk_conditional_stmt(conditional_stmt);
    }

    /// Visit an assignment statement
    fn visit_assign_stmt(&mut self, assign_stmt: &AssignStmt) {
        self.walk_assign_stmt(assign_stmt);
    }

    /// Visit a connection statement
    fn visit_connection_stmt(&mut self, connection_stmt: &ConnectionStmt) {
        self.walk_connection_stmt(connection_stmt);
    }

    /// Visit an expression
    fn visit_expr(&mut self, expr: &Expr) {
        self.walk_expr(expr);
    }

    /// Visit a binary expression
    fn visit_binary_expr(&mut self, binary_expr: &BinaryExpr) {
        self.walk_binary_expr(binary_expr);
    }

    // --- Default walking implementations ---

    fn walk_source_file(&mut self, node: &SyntaxNode<BhdlLanguage>) {
        use rowan::ast::AstNode;
        
        // Walk through all board definitions
        for child in node.children() {
            if let Some(board) = Board::cast(child.clone()) {
                self.visit_board(&board);
            } else if let Some(module) = Module::cast(child.clone()) {
                self.visit_module(&module);
            } else if let Some(comp_def) = ComponentDef::cast(child.clone()) {
                self.visit_component_def(&comp_def);
            } else if let Some(interface_def) = InterfaceDef::cast(child.clone()) {
                self.visit_interface_def(&interface_def);
            }
        }
    }

    fn walk_board(&mut self, board: &Board) {
        use rowan::ast::AstNode;
        
        // Visit all statements in the board
        for child in board.syntax().children() {
            if let Some(flow_stmt) = FlowStmt::cast(child.clone()) {
                self.visit_flow_stmt(&flow_stmt);
            } else if let Some(generate_stmt) = GenerateStmt::cast(child.clone()) {
                self.visit_generate_stmt(&generate_stmt);
            } else if let Some(conditional_stmt) = ConditionalStmt::cast(child.clone()) {
                self.visit_conditional_stmt(&conditional_stmt);
            } else if let Some(assign_stmt) = AssignStmt::cast(child.clone()) {
                self.visit_assign_stmt(&assign_stmt);
            } else if let Some(connection_stmt) = ConnectionStmt::cast(child.clone()) {
                self.visit_connection_stmt(&connection_stmt);
            }
        }
    }

    fn walk_module(&mut self, module: &Module) {
        // Similar to board walking
        self.walk_board_like_node(module.syntax());
    }

    fn walk_component_def(&mut self, _comp_def: &ComponentDef) {
        // Walk component definition - pins, parameters, etc.
    }

    fn walk_interface_def(&mut self, _interface_def: &InterfaceDef) {
        // Walk interface definition - pins, parameters, etc.
    }

    fn walk_flow_stmt(&mut self, flow_stmt: &FlowStmt) {
        if let Some(flow_expr) = flow_stmt.flow_expr() {
            self.visit_flow_expr(&flow_expr);
        }
    }

    fn walk_flow_expr(&mut self, flow_expr: &FlowExpr) {
        // Visit flow elements if they contain expressions
        if let Some(expr) = flow_expr.as_expr() {
            self.visit_expr(&expr);
        }
    }

    fn walk_component_instantiation(&mut self, _comp_inst: &ComponentInstantiation) {
        // Walk parameters if needed
    }

    fn walk_generate_stmt(&mut self, generate_stmt: &GenerateStmt) {
        // Visit body statements
        for stmt_node in generate_stmt.body_statements() {
            use rowan::ast::AstNode;
            
            if let Some(flow_stmt) = FlowStmt::cast(stmt_node.clone()) {
                self.visit_flow_stmt(&flow_stmt);
            } else if let Some(connection_stmt) = ConnectionStmt::cast(stmt_node.clone()) {
                self.visit_connection_stmt(&connection_stmt);
            } else if let Some(assign_stmt) = AssignStmt::cast(stmt_node.clone()) {
                self.visit_assign_stmt(&assign_stmt);
            }
        }
    }

    fn walk_conditional_stmt(&mut self, conditional_stmt: &ConditionalStmt) {
        // Visit condition
        if let Some(condition) = conditional_stmt.condition() {
            self.visit_expr(&condition);
        }

        // Visit if statements
        for stmt_node in conditional_stmt.if_statements() {
            use rowan::ast::AstNode;
            
            if let Some(assign_stmt) = AssignStmt::cast(stmt_node.clone()) {
                self.visit_assign_stmt(&assign_stmt);
            } else if let Some(connection_stmt) = ConnectionStmt::cast(stmt_node.clone()) {
                self.visit_connection_stmt(&connection_stmt);
            }
        }

        // Visit else statements
        for stmt_node in conditional_stmt.else_statements() {
            use rowan::ast::AstNode;
            
            if let Some(assign_stmt) = AssignStmt::cast(stmt_node.clone()) {
                self.visit_assign_stmt(&assign_stmt);
            } else if let Some(connection_stmt) = ConnectionStmt::cast(stmt_node.clone()) {
                self.visit_connection_stmt(&connection_stmt);
            }
        }
    }

    fn walk_assign_stmt(&mut self, assign_stmt: &AssignStmt) {
        if let Some(value) = assign_stmt.value() {
            self.visit_expr(&value);
        }
    }

    fn walk_connection_stmt(&mut self, _connection_stmt: &ConnectionStmt) {
        // Walk source and sink references
    }

    fn walk_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::BinaryExpr(bin_expr) => self.visit_binary_expr(bin_expr),
            Expr::PrefixExpr(prefix_expr) => {
                if let Some(inner_expr) = prefix_expr.expr() {
                    self.visit_expr(&inner_expr);
                }
            }
            Expr::TernaryExpr(ternary_expr) => {
                if let Some(condition) = ternary_expr.condition() {
                    self.visit_expr(&condition);
                }
                if let Some(true_expr) = ternary_expr.true_expr() {
                    self.visit_expr(&true_expr);
                }
                if let Some(false_expr) = ternary_expr.false_expr() {
                    self.visit_expr(&false_expr);
                }
            }
            Expr::FunctionCallExpr(func_call) => {
                for arg in func_call.arguments() {
                    self.visit_expr(&arg);
                }
            }
            Expr::ComponentInstExpr(comp_inst) => {
                for param in comp_inst.parameters() {
                    self.visit_expr(&param);
                }
            }
            _ => {} // Handle other expression types
        }
    }

    fn walk_binary_expr(&mut self, binary_expr: &BinaryExpr) {
        if let Some(lhs) = binary_expr.lhs() {
            self.visit_expr(&lhs);
        }
        if let Some(rhs) = binary_expr.rhs() {
            self.visit_expr(&rhs);
        }
    }

    // Helper for walking board-like nodes (modules, etc.)
    fn walk_board_like_node(&mut self, node: &SyntaxNode<BhdlLanguage>) {
        use rowan::ast::AstNode;
        
        for child in node.children() {
            if let Some(flow_stmt) = FlowStmt::cast(child.clone()) {
                self.visit_flow_stmt(&flow_stmt);
            } else if let Some(generate_stmt) = GenerateStmt::cast(child.clone()) {
                self.visit_generate_stmt(&generate_stmt);
            } else if let Some(conditional_stmt) = ConditionalStmt::cast(child.clone()) {
                self.visit_conditional_stmt(&conditional_stmt);
            } else if let Some(assign_stmt) = AssignStmt::cast(child.clone()) {
                self.visit_assign_stmt(&assign_stmt);
            } else if let Some(connection_stmt) = ConnectionStmt::cast(child.clone()) {
                self.visit_connection_stmt(&connection_stmt);
            }
        }
    }
}

/// Example visitor that counts different types of constructs
#[derive(Debug, Default)]
pub struct ConstructCounter {
    pub flow_statements: usize,
    pub component_instantiations: usize,
    pub generate_statements: usize,
    pub conditional_statements: usize,
    pub assignment_statements: usize,
    pub connection_statements: usize,
    pub binary_expressions: usize,
}

impl AstVisitor for ConstructCounter {
    fn visit_flow_stmt(&mut self, flow_stmt: &FlowStmt) {
        self.flow_statements += 1;
        self.walk_flow_stmt(flow_stmt);
    }

    fn visit_component_instantiation(&mut self, comp_inst: &ComponentInstantiation) {
        self.component_instantiations += 1;
        self.walk_component_instantiation(comp_inst);
    }

    fn visit_generate_stmt(&mut self, generate_stmt: &GenerateStmt) {
        self.generate_statements += 1;
        self.walk_generate_stmt(generate_stmt);
    }

    fn visit_conditional_stmt(&mut self, conditional_stmt: &ConditionalStmt) {
        self.conditional_statements += 1;
        self.walk_conditional_stmt(conditional_stmt);
    }

    fn visit_assign_stmt(&mut self, assign_stmt: &AssignStmt) {
        self.assignment_statements += 1;
        self.walk_assign_stmt(assign_stmt);
    }

    fn visit_connection_stmt(&mut self, connection_stmt: &ConnectionStmt) {
        self.connection_statements += 1;
        self.walk_connection_stmt(connection_stmt);
    }

    fn visit_binary_expr(&mut self, binary_expr: &BinaryExpr) {
        self.binary_expressions += 1;
        self.walk_binary_expr(binary_expr);
    }
}

impl ConstructCounter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn total_constructs(&self) -> usize {
        self.flow_statements + 
        self.component_instantiations + 
        self.generate_statements + 
        self.conditional_statements + 
        self.assignment_statements + 
        self.connection_statements
    }
}

/// Example visitor that collects component types used
#[derive(Debug, Default)]
pub struct ComponentTypeCollector {
    pub component_types: std::collections::HashSet<String>,
}

impl ComponentTypeCollector {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AstVisitor for ComponentTypeCollector {
    fn visit_component_instantiation(&mut self, comp_inst: &ComponentInstantiation) {
        if let Some(comp_type_token) = comp_inst.component_type() {
            self.component_types.insert(comp_type_token.text().to_string());
        }
        self.walk_component_instantiation(comp_inst);
    }
}