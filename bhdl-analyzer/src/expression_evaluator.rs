// Expression evaluator for behavioral simulation
// Evaluates attribute expressions at runtime with actual values

use std::collections::HashMap;
use bhdl_ast::expr::{Expr, BinaryExpr, PrefixExpr, TernaryExpr, FunctionCallExpr, ArrayExpr, StructLiteral};
use bhdl_ast::common::{Value, IdentRef};
use bhdl_ast::{SyntaxNode, BhdlLanguage, SyntaxKind};
use rowan::ast::AstNode;
use crate::builtin_variables::SimulationContext;

/// Runtime value types for expression evaluation
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeValue {
    Integer(i64),
    Real(f64),
    Boolean(bool),
    String(String),
    Array(Vec<RuntimeValue>),
    Object(HashMap<String, RuntimeValue>),
}

impl RuntimeValue {
    /// Convert to f64 for numeric operations
    pub fn to_f64(&self) -> Result<f64, EvaluationError> {
        match self {
            RuntimeValue::Integer(i) => Ok(*i as f64),
            RuntimeValue::Real(f) => Ok(*f),
            _ => Err(EvaluationError::TypeError {
                expected: "numeric".to_string(),
                found: self.type_name(),
            }),
        }
    }
    
    /// Convert to i64 for integer operations
    pub fn to_i64(&self) -> Result<i64, EvaluationError> {
        match self {
            RuntimeValue::Integer(i) => Ok(*i),
            RuntimeValue::Real(f) => Ok(*f as i64),
            _ => Err(EvaluationError::TypeError {
                expected: "integer".to_string(),
                found: self.type_name(),
            }),
        }
    }
    
    /// Convert to bool for logical operations
    pub fn to_bool(&self) -> Result<bool, EvaluationError> {
        match self {
            RuntimeValue::Boolean(b) => Ok(*b),
            RuntimeValue::Integer(i) => Ok(*i != 0),
            RuntimeValue::Real(f) => Ok(*f != 0.0),
            _ => Err(EvaluationError::TypeError {
                expected: "boolean".to_string(),
                found: self.type_name(),
            }),
        }
    }
    
    /// Get type name for error messages
    pub fn type_name(&self) -> String {
        match self {
            RuntimeValue::Integer(_) => "integer".to_string(),
            RuntimeValue::Real(_) => "real".to_string(),
            RuntimeValue::Boolean(_) => "boolean".to_string(),
            RuntimeValue::String(_) => "string".to_string(),
            RuntimeValue::Array(_) => "array".to_string(),
            RuntimeValue::Object(_) => "object".to_string(),
        }
    }
}

/// Errors that can occur during expression evaluation
#[derive(Debug, Clone)]
pub enum EvaluationError {
    /// Variable not found in context
    UndefinedVariable(String),
    /// Type mismatch in operation
    TypeError {
        expected: String,
        found: String,
    },
    /// Division by zero
    DivisionByZero,
    /// Invalid operation
    InvalidOperation(String),
    /// Function not found
    UndefinedFunction(String),
    /// Wrong number of arguments
    ArityMismatch {
        function: String,
        expected: usize,
        found: usize,
    },
}

impl std::fmt::Display for EvaluationError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            EvaluationError::UndefinedVariable(name) => {
                write!(f, "Undefined variable: {}", name)
            }
            EvaluationError::TypeError { expected, found } => {
                write!(f, "Type error: expected {}, found {}", expected, found)
            }
            EvaluationError::DivisionByZero => {
                write!(f, "Division by zero")
            }
            EvaluationError::InvalidOperation(op) => {
                write!(f, "Invalid operation: {}", op)
            }
            EvaluationError::UndefinedFunction(name) => {
                write!(f, "Undefined function: {}", name)
            }
            EvaluationError::ArityMismatch { function, expected, found } => {
                write!(f, "Function {} expects {} arguments, found {}", function, expected, found)
            }
        }
    }
}

/// Expression evaluation context
pub struct EvaluationContext<'a> {
    /// Attribute values
    pub attributes: HashMap<String, RuntimeValue>,
    /// Pin values (for behavioral modeling)
    pub pins: HashMap<String, RuntimeValue>,
    /// Simulation context with built-in variables
    pub simulation: &'a SimulationContext,
}

impl<'a> EvaluationContext<'a> {
    pub fn new(simulation: &'a SimulationContext) -> Self {
        Self {
            attributes: HashMap::new(),
            pins: HashMap::new(),
            simulation,
        }
    }
    
    /// Set an attribute value
    pub fn set_attribute(&mut self, name: String, value: RuntimeValue) {
        self.attributes.insert(name, value);
    }
    
    /// Set a pin value
    pub fn set_pin(&mut self, name: String, value: RuntimeValue) {
        self.pins.insert(name, value);
    }
    
    /// Get a variable value (attribute, pin, or built-in)
    fn get_variable(&self, name: &str) -> Result<RuntimeValue, EvaluationError> {
        // Check attributes first
        if let Some(value) = self.attributes.get(name) {
            return Ok(value.clone());
        }
        
        // Check pins
        if let Some(value) = self.pins.get(name) {
            return Ok(value.clone());
        }
        
        // Check built-in variables
        if let Some(value) = self.simulation.get_builtin_value(name) {
            return Ok(RuntimeValue::Real(value));
        }
        
        Err(EvaluationError::UndefinedVariable(name.to_string()))
    }
}

/// Expression evaluator
pub struct ExpressionEvaluator;

impl ExpressionEvaluator {
    /// Evaluate an expression in the given context
    pub fn evaluate(
        expr: &Expr,
        context: &EvaluationContext,
    ) -> Result<RuntimeValue, EvaluationError> {
        match expr {
            Expr::Value(value) => Self::evaluate_value(value),
            Expr::IdentRef(ident) => Self::evaluate_ident(ident, context),
            Expr::Ident(node) => Self::evaluate_ident_node(node, context),
            Expr::BinaryExpr(binary) => Self::evaluate_binary(binary, context),
            Expr::PrefixExpr(prefix) => Self::evaluate_prefix(prefix, context),
            Expr::TernaryExpr(ternary) => Self::evaluate_ternary(ternary, context),
            Expr::FunctionCallExpr(call) => Self::evaluate_function_call(call, context),
            Expr::ArrayExpr(array) => Self::evaluate_array(array, context),
            Expr::StructLiteral(struct_lit) => Self::evaluate_struct_literal(struct_lit, context),
            Expr::Literal(node) => Self::evaluate_literal_node(node),
            _ => Err(EvaluationError::InvalidOperation(
                format!("Cannot evaluate expression type: {:?}", expr)
            )),
        }
    }
    
    /// Evaluate a value node
    fn evaluate_value(value: &Value) -> Result<RuntimeValue, EvaluationError> {
        if let Some(num_token) = value.number_token() {
            let text = num_token.text();
            // Try to parse as integer first
            if !text.contains('.') {
                if let Ok(parsed) = text.parse::<i64>() {
                    return Ok(RuntimeValue::Integer(parsed));
                }
            }
            // Try to parse as float
            let parsed = text.parse::<f64>()
                .map_err(|_| EvaluationError::InvalidOperation(
                    format!("Invalid number: {}", text)
                ))?;
            Ok(RuntimeValue::Real(parsed))
        } else {
            // Check if it's a literal value in the syntax tree
            let text = value.syntax().text().to_string();
            // Try parsing the text directly
            if let Ok(int_val) = text.parse::<i64>() {
                Ok(RuntimeValue::Integer(int_val))
            } else if let Ok(float_val) = text.parse::<f64>() {
                Ok(RuntimeValue::Real(float_val))
            } else {
                match text.trim() {
                    "true" => Ok(RuntimeValue::Boolean(true)),
                    "false" => Ok(RuntimeValue::Boolean(false)),
                    _ => Err(EvaluationError::InvalidOperation(format!("Unknown value: {}", text))),
                }
            }
        }
    }
    
    /// Evaluate an identifier reference
    fn evaluate_ident(ident: &IdentRef, context: &EvaluationContext) -> Result<RuntimeValue, EvaluationError> {
        if let Some(token) = ident.token() {
            let name = token.text().trim();
            context.get_variable(name)
        } else {
            Err(EvaluationError::InvalidOperation("Invalid identifier reference".to_string()))
        }
    }
    
    /// Evaluate an identifier node
    fn evaluate_ident_node(node: &SyntaxNode<BhdlLanguage>, context: &EvaluationContext) -> Result<RuntimeValue, EvaluationError> {
        let name = node.text().to_string().trim().to_string();
        context.get_variable(&name)
    }
    
    /// Evaluate a literal node
    fn evaluate_literal_node(node: &SyntaxNode<BhdlLanguage>) -> Result<RuntimeValue, EvaluationError> {
        let text = node.text().to_string();
        
        // Try to parse as number
        if let Ok(int_val) = text.parse::<i64>() {
            return Ok(RuntimeValue::Integer(int_val));
        }
        if let Ok(float_val) = text.parse::<f64>() {
            return Ok(RuntimeValue::Real(float_val));
        }
        
        // Check for boolean literals
        match text.as_str() {
            "true" => return Ok(RuntimeValue::Boolean(true)),
            "false" => return Ok(RuntimeValue::Boolean(false)),
            _ => {}
        }
        
        // Otherwise treat as string (remove quotes if present)
        let string_val = if text.starts_with('"') && text.ends_with('"') {
            text[1..text.len()-1].to_string()
        } else {
            text
        };
        Ok(RuntimeValue::String(string_val))
    }
    
    /// Evaluate a binary expression
    fn evaluate_binary(
        binary: &BinaryExpr,
        context: &EvaluationContext,
    ) -> Result<RuntimeValue, EvaluationError> {
        let left = binary.lhs()
            .ok_or_else(|| EvaluationError::InvalidOperation("Missing left operand".to_string()))?;
        let right = binary.rhs()
            .ok_or_else(|| EvaluationError::InvalidOperation("Missing right operand".to_string()))?;
        
        let left_val = Self::evaluate(&left, context)?;
        let right_val = Self::evaluate(&right, context)?;
        
        if let Some(op_token) = binary.op_token() {
            match op_token.kind() {
                SyntaxKind::PLUS => Self::eval_add(left_val, right_val),
                SyntaxKind::MINUS => Self::eval_subtract(left_val, right_val),
                SyntaxKind::STAR => Self::eval_multiply(left_val, right_val),
                SyntaxKind::SLASH => Self::eval_divide(left_val, right_val),
                SyntaxKind::PERCENT => Self::eval_modulo(left_val, right_val),
                SyntaxKind::EQEQ => Self::eval_equal(left_val, right_val),
                SyntaxKind::NEQ => Self::eval_not_equal(left_val, right_val),
                SyntaxKind::L_ANGLE => Self::eval_less_than(left_val, right_val),
                SyntaxKind::R_ANGLE => Self::eval_greater_than(left_val, right_val),
                SyntaxKind::LTEQ => Self::eval_less_equal(left_val, right_val),
                SyntaxKind::GTEQ => Self::eval_greater_equal(left_val, right_val),
                SyntaxKind::AMPAMP => Self::eval_logical_and(left_val, right_val),
                SyntaxKind::PIPEPIPE => Self::eval_logical_or(left_val, right_val),
                _ => Err(EvaluationError::InvalidOperation(
                    format!("Unknown binary operator: {:?}", op_token.kind())
                )),
            }
        } else {
            Err(EvaluationError::InvalidOperation("Missing operator".to_string()))
        }
    }
    
    /// Evaluate a prefix expression
    fn evaluate_prefix(
        prefix: &PrefixExpr,
        context: &EvaluationContext,
    ) -> Result<RuntimeValue, EvaluationError> {
        let operand = prefix.expr()
            .ok_or_else(|| EvaluationError::InvalidOperation("Missing operand".to_string()))?;
        let operand_val = Self::evaluate(&operand, context)?;
        
        if let Some(op_token) = prefix.op_token() {
            match op_token.kind() {
                SyntaxKind::MINUS => Self::eval_negate(operand_val),
                SyntaxKind::BANG => Self::eval_logical_not(operand_val),
                _ => Err(EvaluationError::InvalidOperation(
                    format!("Unknown prefix operator: {:?}", op_token.kind())
                )),
            }
        } else {
            Err(EvaluationError::InvalidOperation("Missing operator".to_string()))
        }
    }
    
    /// Evaluate a ternary expression
    fn evaluate_ternary(
        ternary: &TernaryExpr,
        context: &EvaluationContext,
    ) -> Result<RuntimeValue, EvaluationError> {
        let condition = ternary.condition()
            .ok_or_else(|| EvaluationError::InvalidOperation("Missing condition".to_string()))?;
        let condition_val = Self::evaluate(&condition, context)?;
        
        if condition_val.to_bool()? {
            let true_expr = ternary.true_expr()
                .ok_or_else(|| EvaluationError::InvalidOperation("Missing true expression".to_string()))?;
            Self::evaluate(&true_expr, context)
        } else {
            let false_expr = ternary.false_expr()
                .ok_or_else(|| EvaluationError::InvalidOperation("Missing false expression".to_string()))?;
            Self::evaluate(&false_expr, context)
        }
    }
    
    /// Evaluate a function call
    fn evaluate_function_call(
        call: &FunctionCallExpr,
        context: &EvaluationContext,
    ) -> Result<RuntimeValue, EvaluationError> {
        let name_token = call.name()
            .ok_or_else(|| EvaluationError::InvalidOperation("Missing function name".to_string()))?;
        let name = name_token.text();
        
        let args: Vec<RuntimeValue> = call.arguments()
            .map(|arg| Self::evaluate(&arg, context))
            .collect::<Result<Vec<_>, _>>()?;
        
        // Built-in functions
        match name {
            "sin" => Self::eval_sin(&args),
            "cos" => Self::eval_cos(&args),
            "tan" => Self::eval_tan(&args),
            "sqrt" => Self::eval_sqrt(&args),
            "abs" => Self::eval_abs(&args),
            "pow" => Self::eval_pow(&args),
            "log" => Self::eval_log(&args),
            "exp" => Self::eval_exp(&args),
            "min" => Self::eval_min(&args),
            "max" => Self::eval_max(&args),
            "floor" => Self::eval_floor(&args),
            "ceil" => Self::eval_ceil(&args),
            "round" => Self::eval_round(&args),
            _ => Err(EvaluationError::UndefinedFunction(name.to_string())),
        }
    }
    
    // Arithmetic operations
    fn eval_add(left: RuntimeValue, right: RuntimeValue) -> Result<RuntimeValue, EvaluationError> {
        match (left, right) {
            (RuntimeValue::Integer(a), RuntimeValue::Integer(b)) => Ok(RuntimeValue::Integer(a + b)),
            (RuntimeValue::Real(a), RuntimeValue::Real(b)) => Ok(RuntimeValue::Real(a + b)),
            (RuntimeValue::Integer(a), RuntimeValue::Real(b)) => Ok(RuntimeValue::Real(a as f64 + b)),
            (RuntimeValue::Real(a), RuntimeValue::Integer(b)) => Ok(RuntimeValue::Real(a + b as f64)),
            (RuntimeValue::String(a), RuntimeValue::String(b)) => Ok(RuntimeValue::String(a + &b)),
            _ => Err(EvaluationError::TypeError {
                expected: "numeric or string".to_string(),
                found: "incompatible types".to_string(),
            }),
        }
    }
    
    fn eval_subtract(left: RuntimeValue, right: RuntimeValue) -> Result<RuntimeValue, EvaluationError> {
        let a = left.to_f64()?;
        let b = right.to_f64()?;
        Ok(RuntimeValue::Real(a - b))
    }
    
    fn eval_multiply(left: RuntimeValue, right: RuntimeValue) -> Result<RuntimeValue, EvaluationError> {
        let a = left.to_f64()?;
        let b = right.to_f64()?;
        Ok(RuntimeValue::Real(a * b))
    }
    
    fn eval_divide(left: RuntimeValue, right: RuntimeValue) -> Result<RuntimeValue, EvaluationError> {
        let a = left.to_f64()?;
        let b = right.to_f64()?;
        if b == 0.0 {
            Err(EvaluationError::DivisionByZero)
        } else {
            Ok(RuntimeValue::Real(a / b))
        }
    }
    
    fn eval_modulo(left: RuntimeValue, right: RuntimeValue) -> Result<RuntimeValue, EvaluationError> {
        let a = left.to_i64()?;
        let b = right.to_i64()?;
        if b == 0 {
            Err(EvaluationError::DivisionByZero)
        } else {
            Ok(RuntimeValue::Integer(a % b))
        }
    }
    
    // Comparison operations
    fn eval_equal(left: RuntimeValue, right: RuntimeValue) -> Result<RuntimeValue, EvaluationError> {
        let result = match (left, right) {
            (RuntimeValue::Integer(a), RuntimeValue::Integer(b)) => a == b,
            (RuntimeValue::Real(a), RuntimeValue::Real(b)) => (a - b).abs() < f64::EPSILON,
            (RuntimeValue::Boolean(a), RuntimeValue::Boolean(b)) => a == b,
            (RuntimeValue::String(a), RuntimeValue::String(b)) => a == b,
            _ => false,
        };
        Ok(RuntimeValue::Boolean(result))
    }
    
    fn eval_not_equal(left: RuntimeValue, right: RuntimeValue) -> Result<RuntimeValue, EvaluationError> {
        let eq_result = Self::eval_equal(left, right)?;
        Ok(RuntimeValue::Boolean(!eq_result.to_bool()?))
    }
    
    fn eval_less_than(left: RuntimeValue, right: RuntimeValue) -> Result<RuntimeValue, EvaluationError> {
        let a = left.to_f64()?;
        let b = right.to_f64()?;
        Ok(RuntimeValue::Boolean(a < b))
    }
    
    fn eval_greater_than(left: RuntimeValue, right: RuntimeValue) -> Result<RuntimeValue, EvaluationError> {
        let a = left.to_f64()?;
        let b = right.to_f64()?;
        Ok(RuntimeValue::Boolean(a > b))
    }
    
    fn eval_less_equal(left: RuntimeValue, right: RuntimeValue) -> Result<RuntimeValue, EvaluationError> {
        let a = left.to_f64()?;
        let b = right.to_f64()?;
        Ok(RuntimeValue::Boolean(a <= b))
    }
    
    fn eval_greater_equal(left: RuntimeValue, right: RuntimeValue) -> Result<RuntimeValue, EvaluationError> {
        let a = left.to_f64()?;
        let b = right.to_f64()?;
        Ok(RuntimeValue::Boolean(a >= b))
    }
    
    // Logical operations
    fn eval_logical_and(left: RuntimeValue, right: RuntimeValue) -> Result<RuntimeValue, EvaluationError> {
        let a = left.to_bool()?;
        let b = right.to_bool()?;
        Ok(RuntimeValue::Boolean(a && b))
    }
    
    fn eval_logical_or(left: RuntimeValue, right: RuntimeValue) -> Result<RuntimeValue, EvaluationError> {
        let a = left.to_bool()?;
        let b = right.to_bool()?;
        Ok(RuntimeValue::Boolean(a || b))
    }
    
    fn eval_negate(val: RuntimeValue) -> Result<RuntimeValue, EvaluationError> {
        let n = val.to_f64()?;
        Ok(RuntimeValue::Real(-n))
    }
    
    fn eval_logical_not(val: RuntimeValue) -> Result<RuntimeValue, EvaluationError> {
        let b = val.to_bool()?;
        Ok(RuntimeValue::Boolean(!b))
    }
    
    // Mathematical functions
    fn eval_sin(args: &[RuntimeValue]) -> Result<RuntimeValue, EvaluationError> {
        if args.len() != 1 {
            return Err(EvaluationError::ArityMismatch {
                function: "sin".to_string(),
                expected: 1,
                found: args.len(),
            });
        }
        let x = args[0].to_f64()?;
        Ok(RuntimeValue::Real(x.sin()))
    }
    
    fn eval_cos(args: &[RuntimeValue]) -> Result<RuntimeValue, EvaluationError> {
        if args.len() != 1 {
            return Err(EvaluationError::ArityMismatch {
                function: "cos".to_string(),
                expected: 1,
                found: args.len(),
            });
        }
        let x = args[0].to_f64()?;
        Ok(RuntimeValue::Real(x.cos()))
    }
    
    fn eval_tan(args: &[RuntimeValue]) -> Result<RuntimeValue, EvaluationError> {
        if args.len() != 1 {
            return Err(EvaluationError::ArityMismatch {
                function: "tan".to_string(),
                expected: 1,
                found: args.len(),
            });
        }
        let x = args[0].to_f64()?;
        Ok(RuntimeValue::Real(x.tan()))
    }
    
    fn eval_sqrt(args: &[RuntimeValue]) -> Result<RuntimeValue, EvaluationError> {
        if args.len() != 1 {
            return Err(EvaluationError::ArityMismatch {
                function: "sqrt".to_string(),
                expected: 1,
                found: args.len(),
            });
        }
        let x = args[0].to_f64()?;
        if x < 0.0 {
            Err(EvaluationError::InvalidOperation("sqrt of negative number".to_string()))
        } else {
            Ok(RuntimeValue::Real(x.sqrt()))
        }
    }
    
    fn eval_abs(args: &[RuntimeValue]) -> Result<RuntimeValue, EvaluationError> {
        if args.len() != 1 {
            return Err(EvaluationError::ArityMismatch {
                function: "abs".to_string(),
                expected: 1,
                found: args.len(),
            });
        }
        let x = args[0].to_f64()?;
        Ok(RuntimeValue::Real(x.abs()))
    }
    
    fn eval_pow(args: &[RuntimeValue]) -> Result<RuntimeValue, EvaluationError> {
        if args.len() != 2 {
            return Err(EvaluationError::ArityMismatch {
                function: "pow".to_string(),
                expected: 2,
                found: args.len(),
            });
        }
        let base = args[0].to_f64()?;
        let exp = args[1].to_f64()?;
        Ok(RuntimeValue::Real(base.powf(exp)))
    }
    
    fn eval_log(args: &[RuntimeValue]) -> Result<RuntimeValue, EvaluationError> {
        if args.len() != 1 {
            return Err(EvaluationError::ArityMismatch {
                function: "log".to_string(),
                expected: 1,
                found: args.len(),
            });
        }
        let x = args[0].to_f64()?;
        if x <= 0.0 {
            Err(EvaluationError::InvalidOperation("log of non-positive number".to_string()))
        } else {
            Ok(RuntimeValue::Real(x.ln()))
        }
    }
    
    fn eval_exp(args: &[RuntimeValue]) -> Result<RuntimeValue, EvaluationError> {
        if args.len() != 1 {
            return Err(EvaluationError::ArityMismatch {
                function: "exp".to_string(),
                expected: 1,
                found: args.len(),
            });
        }
        let x = args[0].to_f64()?;
        Ok(RuntimeValue::Real(x.exp()))
    }
    
    fn eval_min(args: &[RuntimeValue]) -> Result<RuntimeValue, EvaluationError> {
        if args.is_empty() {
            return Err(EvaluationError::ArityMismatch {
                function: "min".to_string(),
                expected: 1,
                found: 0,
            });
        }
        let values: Vec<f64> = args.iter()
            .map(|v| v.to_f64())
            .collect::<Result<Vec<_>, _>>()?;
        let min_val = values.into_iter()
            .fold(f64::INFINITY, |a, b| a.min(b));
        Ok(RuntimeValue::Real(min_val))
    }
    
    fn eval_max(args: &[RuntimeValue]) -> Result<RuntimeValue, EvaluationError> {
        if args.is_empty() {
            return Err(EvaluationError::ArityMismatch {
                function: "max".to_string(),
                expected: 1,
                found: 0,
            });
        }
        let values: Vec<f64> = args.iter()
            .map(|v| v.to_f64())
            .collect::<Result<Vec<_>, _>>()?;
        let max_val = values.into_iter()
            .fold(f64::NEG_INFINITY, |a, b| a.max(b));
        Ok(RuntimeValue::Real(max_val))
    }
    
    fn eval_floor(args: &[RuntimeValue]) -> Result<RuntimeValue, EvaluationError> {
        if args.len() != 1 {
            return Err(EvaluationError::ArityMismatch {
                function: "floor".to_string(),
                expected: 1,
                found: args.len(),
            });
        }
        let x = args[0].to_f64()?;
        Ok(RuntimeValue::Integer(x.floor() as i64))
    }
    
    fn eval_ceil(args: &[RuntimeValue]) -> Result<RuntimeValue, EvaluationError> {
        if args.len() != 1 {
            return Err(EvaluationError::ArityMismatch {
                function: "ceil".to_string(),
                expected: 1,
                found: args.len(),
            });
        }
        let x = args[0].to_f64()?;
        Ok(RuntimeValue::Integer(x.ceil() as i64))
    }
    
    fn eval_round(args: &[RuntimeValue]) -> Result<RuntimeValue, EvaluationError> {
        if args.len() != 1 {
            return Err(EvaluationError::ArityMismatch {
                function: "round".to_string(),
                expected: 1,
                found: args.len(),
            });
        }
        let x = args[0].to_f64()?;
        Ok(RuntimeValue::Integer(x.round() as i64))
    }
    
    /// Evaluate array expression: [elem1, elem2, ...]
    fn evaluate_array(array: &ArrayExpr, context: &EvaluationContext) -> Result<RuntimeValue, EvaluationError> {
        let mut elements = Vec::new();
        
        for element in array.elements() {
            let value = Self::evaluate(&element, context)?;
            elements.push(value);
        }
        
        Ok(RuntimeValue::Array(elements))
    }
    
    /// Evaluate struct literal: { field1: value1, field2: value2, ... }
    fn evaluate_struct_literal(struct_lit: &StructLiteral, context: &EvaluationContext) -> Result<RuntimeValue, EvaluationError> {
        let mut fields = HashMap::new();
        
        for field in struct_lit.fields() {
            if let Some(name_token) = field.name() {
                let field_name = name_token.text().to_string();
                
                if let Some(value_expr) = field.value() {
                    let value = Self::evaluate(&value_expr, context)?;
                    fields.insert(field_name, value);
                } else {
                    return Err(EvaluationError::InvalidOperation(
                        format!("Missing value for field '{}'", field_name)
                    ));
                }
            } else {
                return Err(EvaluationError::InvalidOperation(
                    "Field missing name in struct literal".to_string()
                ));
            }
        }
        
        Ok(RuntimeValue::Object(fields))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_parser::parse;
    use bhdl_ast::SourceFile;
    
    fn parse_and_get_expr(code: &str) -> Option<Expr> {
        let full_code = format!("board test {{ attribute test_attr = {}; }}", code);
        let parsed = parse(&full_code);
        let source_file = SourceFile::cast(parsed.syntax())?;
        let board = source_file.boards().next()?;
        let attr = board.attribute_decls().next()?;
        attr.value()
    }
    
    fn assert_numeric_eq(result: RuntimeValue, expected: f64) {
        match result {
            RuntimeValue::Integer(v) => assert_eq!(v as f64, expected),
            RuntimeValue::Real(v) => assert!((v - expected).abs() < 1e-10,
                "expected {}, got {}", expected, v),
            other => panic!("expected numeric value, got {:?}", other),
        }
    }

    #[test]
    fn test_basic_arithmetic() {
        let sim_ctx = SimulationContext::new(0.001);
        let eval_ctx = EvaluationContext::new(&sim_ctx);

        // Test addition
        let expr = parse_and_get_expr("2 + 3").unwrap();
        let result = ExpressionEvaluator::evaluate(&expr, &eval_ctx).unwrap();
        assert_numeric_eq(result, 5.0);

        // Test multiplication
        let expr = parse_and_get_expr("4 * 5").unwrap();
        let result = ExpressionEvaluator::evaluate(&expr, &eval_ctx).unwrap();
        assert_numeric_eq(result, 20.0);

        // Test division
        let expr = parse_and_get_expr("10 / 2").unwrap();
        let result = ExpressionEvaluator::evaluate(&expr, &eval_ctx).unwrap();
        assert_numeric_eq(result, 5.0);
    }
    
    #[test]
    fn test_builtin_variables() {
        let sim_ctx = SimulationContext::new(0.001);
        let eval_ctx = EvaluationContext::new(&sim_ctx);
        
        // Test dt
        let expr = parse_and_get_expr("dt * 1000").unwrap();
        let result = ExpressionEvaluator::evaluate(&expr, &eval_ctx).unwrap();
        assert_eq!(result, RuntimeValue::Real(1.0)); // 0.001 * 1000 = 1.0
        
        // Test pi
        let expr = parse_and_get_expr("2 * pi").unwrap();
        let result = ExpressionEvaluator::evaluate(&expr, &eval_ctx).unwrap();
        if let RuntimeValue::Real(val) = result {
            assert!((val - 2.0 * std::f64::consts::PI).abs() < 1e-10);
        } else {
            panic!("Expected Real value");
        }
    }
    
    #[test]
    fn test_comparison_operators() {
        let sim_ctx = SimulationContext::new(0.001);
        let eval_ctx = EvaluationContext::new(&sim_ctx);
        
        // Test greater than
        let expr = parse_and_get_expr("5 > 3").unwrap();
        let result = ExpressionEvaluator::evaluate(&expr, &eval_ctx).unwrap();
        assert_eq!(result, RuntimeValue::Boolean(true));
        
        // Test less than
        let expr = parse_and_get_expr("2 < 1").unwrap();
        let result = ExpressionEvaluator::evaluate(&expr, &eval_ctx).unwrap();
        assert_eq!(result, RuntimeValue::Boolean(false));
    }
    
    #[test]
    #[ignore] // TODO: ternary expression is not parsed in attribute value context
    fn test_ternary_operator() {
        let sim_ctx = SimulationContext::new(0.001);
        let mut eval_ctx = EvaluationContext::new(&sim_ctx);

        eval_ctx.set_attribute("x".to_string(), RuntimeValue::Integer(5));

        let expr = parse_and_get_expr("x > 3 ? 10 : 20").unwrap();
        let result = ExpressionEvaluator::evaluate(&expr, &eval_ctx).unwrap();
        assert_eq!(result, RuntimeValue::Integer(10));

        eval_ctx.set_attribute("x".to_string(), RuntimeValue::Integer(1));
        let result = ExpressionEvaluator::evaluate(&expr, &eval_ctx).unwrap();
        assert_eq!(result, RuntimeValue::Integer(20));
    }
    
    #[test]
    fn test_function_calls() {
        let sim_ctx = SimulationContext::new(0.001);
        let eval_ctx = EvaluationContext::new(&sim_ctx);

        let expr = parse_and_get_expr("sin(0)").unwrap();
        let result = ExpressionEvaluator::evaluate(&expr, &eval_ctx).unwrap();
        if let RuntimeValue::Real(val) = result {
            assert!(val.abs() < 1e-10);
        } else {
            panic!("Expected Real value");
        }

        let expr = parse_and_get_expr("sqrt(16)").unwrap();
        let result = ExpressionEvaluator::evaluate(&expr, &eval_ctx).unwrap();
        assert_eq!(result, RuntimeValue::Real(4.0));

        // Multi-argument builtin
        let expr = parse_and_get_expr("pow(2, 10)").unwrap();
        let result = ExpressionEvaluator::evaluate(&expr, &eval_ctx).unwrap();
        assert_numeric_eq(result, 1024.0);

        // Nested calls with an expression argument
        let expr = parse_and_get_expr("sqrt(pow(3, 2) + pow(4, 2))").unwrap();
        let result = ExpressionEvaluator::evaluate(&expr, &eval_ctx).unwrap();
        assert_numeric_eq(result, 5.0);
    }

    #[test]
    fn test_boolean_literals() {
        let sim_ctx = SimulationContext::new(0.001);
        let eval_ctx = EvaluationContext::new(&sim_ctx);

        let expr = parse_and_get_expr("true").unwrap();
        let result = ExpressionEvaluator::evaluate(&expr, &eval_ctx).unwrap();
        assert_eq!(result, RuntimeValue::Boolean(true));

        let expr = parse_and_get_expr("false").unwrap();
        let result = ExpressionEvaluator::evaluate(&expr, &eval_ctx).unwrap();
        assert_eq!(result, RuntimeValue::Boolean(false));
    }
}