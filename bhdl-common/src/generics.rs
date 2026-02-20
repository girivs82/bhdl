//! Generic parameter types and constraint representations for BHDL.
//!
//! Supports typed generics with where-clause constraints:
//! ```bhdl
//! module BuckConverter(V_IN: voltage, V_OUT: voltage, I_MAX: current)
//!     where V_IN >= 4.5V && V_IN <= 40V,
//!           V_OUT < V_IN,
//!           I_MAX <= 3A
//! { ... }
//! ```

use crate::BhdlType;
use crate::ConstValue;

/// A generic (type or const) parameter declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct GenericParam {
    /// Parameter name (e.g., "V_IN", "T")
    pub name: String,
    /// Type classification of this parameter.
    pub param_type: GenericParamType,
    /// Constraints on this parameter from `where` clauses.
    pub constraints: Vec<Constraint>,
    /// Optional default value.
    pub default: Option<ConstValue>,
}

/// Classification of a generic parameter.
#[derive(Debug, Clone, PartialEq)]
pub enum GenericParamType {
    /// A type parameter (e.g., `T`) — the parameter itself is a type.
    Type,
    /// A type parameter bounded by traits/interfaces (e.g., `T: SpiPeripheral`).
    TypeBounded(Vec<String>),
    /// A constant parameter with a specific type (e.g., `V: voltage`, `N: integer`).
    Const(BhdlType),
}

/// A constraint on a generic parameter.
#[derive(Debug, Clone, PartialEq)]
pub enum Constraint {
    /// `lhs > rhs`
    GreaterThan(ConstraintExpr, ConstraintExpr),
    /// `lhs >= rhs`
    GreaterEqual(ConstraintExpr, ConstraintExpr),
    /// `lhs < rhs`
    LessThan(ConstraintExpr, ConstraintExpr),
    /// `lhs <= rhs`
    LessEqual(ConstraintExpr, ConstraintExpr),
    /// `lhs == rhs`
    Equal(ConstraintExpr, ConstraintExpr),
    /// `lhs != rhs`
    NotEqual(ConstraintExpr, ConstraintExpr),
    /// `value` is between `low` and `high` (inclusive).
    InRange(ConstraintExpr, ConstraintExpr, ConstraintExpr),
    /// A trait bound: `T: TraitName`.
    TraitBound(String, String),
}

/// An expression within a constraint.
/// Constraints reference parameters and literal values.
#[derive(Debug, Clone, PartialEq)]
pub enum ConstraintExpr {
    /// Reference to a parameter by name.
    Param(String),
    /// A compile-time constant value.
    Literal(ConstValue),
    /// Binary arithmetic: lhs op rhs
    BinaryOp {
        op: ConstraintOp,
        lhs: Box<ConstraintExpr>,
        rhs: Box<ConstraintExpr>,
    },
}

/// Binary operations in constraint expressions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConstraintOp {
    Add,
    Sub,
    Mul,
    Div,
}

impl Constraint {
    /// Check if a constraint is satisfied given parameter values.
    /// Returns `Ok(())` if satisfied, `Err(message)` if violated.
    pub fn check(&self, resolve: &dyn Fn(&str) -> Option<ConstValue>) -> Result<(), String> {
        match self {
            Constraint::GreaterThan(lhs, rhs) => {
                let l = lhs.evaluate(resolve).ok_or("Cannot evaluate LHS")?;
                let r = rhs.evaluate(resolve).ok_or("Cannot evaluate RHS")?;
                if const_to_f64(&l) > const_to_f64(&r) { Ok(()) }
                else { Err(format!("{} must be > {}", const_display(&l), const_display(&r))) }
            }
            Constraint::GreaterEqual(lhs, rhs) => {
                let l = lhs.evaluate(resolve).ok_or("Cannot evaluate LHS")?;
                let r = rhs.evaluate(resolve).ok_or("Cannot evaluate RHS")?;
                if const_to_f64(&l) >= const_to_f64(&r) { Ok(()) }
                else { Err(format!("{} must be >= {}", const_display(&l), const_display(&r))) }
            }
            Constraint::LessThan(lhs, rhs) => {
                let l = lhs.evaluate(resolve).ok_or("Cannot evaluate LHS")?;
                let r = rhs.evaluate(resolve).ok_or("Cannot evaluate RHS")?;
                if const_to_f64(&l) < const_to_f64(&r) { Ok(()) }
                else { Err(format!("{} must be < {}", const_display(&l), const_display(&r))) }
            }
            Constraint::LessEqual(lhs, rhs) => {
                let l = lhs.evaluate(resolve).ok_or("Cannot evaluate LHS")?;
                let r = rhs.evaluate(resolve).ok_or("Cannot evaluate RHS")?;
                if const_to_f64(&l) <= const_to_f64(&r) { Ok(()) }
                else { Err(format!("{} must be <= {}", const_display(&l), const_display(&r))) }
            }
            Constraint::Equal(lhs, rhs) => {
                let l = lhs.evaluate(resolve).ok_or("Cannot evaluate LHS")?;
                let r = rhs.evaluate(resolve).ok_or("Cannot evaluate RHS")?;
                if (const_to_f64(&l) - const_to_f64(&r)).abs() < 1e-12 { Ok(()) }
                else { Err(format!("{} must equal {}", const_display(&l), const_display(&r))) }
            }
            Constraint::NotEqual(lhs, rhs) => {
                let l = lhs.evaluate(resolve).ok_or("Cannot evaluate LHS")?;
                let r = rhs.evaluate(resolve).ok_or("Cannot evaluate RHS")?;
                if (const_to_f64(&l) - const_to_f64(&r)).abs() >= 1e-12 { Ok(()) }
                else { Err(format!("{} must not equal {}", const_display(&l), const_display(&r))) }
            }
            Constraint::InRange(value, low, high) => {
                let v = value.evaluate(resolve).ok_or("Cannot evaluate value")?;
                let lo = low.evaluate(resolve).ok_or("Cannot evaluate low bound")?;
                let hi = high.evaluate(resolve).ok_or("Cannot evaluate high bound")?;
                if const_to_f64(&v) >= const_to_f64(&lo) && const_to_f64(&v) <= const_to_f64(&hi) { Ok(()) }
                else { Err(format!("{} must be in range [{}, {}]", const_display(&v), const_display(&lo), const_display(&hi))) }
            }
            Constraint::TraitBound(_, _) => {
                // Trait bounds are checked structurally, not by value
                Ok(())
            }
        }
    }
}

impl ConstraintExpr {
    /// Evaluate this expression given a resolver for parameter names.
    pub fn evaluate(&self, resolve: &dyn Fn(&str) -> Option<ConstValue>) -> Option<ConstValue> {
        match self {
            ConstraintExpr::Param(name) => resolve(name),
            ConstraintExpr::Literal(value) => Some(value.clone()),
            ConstraintExpr::BinaryOp { op, lhs, rhs } => {
                let l = lhs.evaluate(resolve)?;
                let r = rhs.evaluate(resolve)?;
                match op {
                    ConstraintOp::Add => l.add(r).ok(),
                    ConstraintOp::Sub => l.sub(r).ok(),
                    ConstraintOp::Mul => l.mul(r).ok(),
                    ConstraintOp::Div => l.div(r).ok(),
                }
            }
        }
    }
}

/// Helper to get f64 from ConstValue for constraint checking.
fn const_to_f64(v: &ConstValue) -> f64 {
    v.as_f64().unwrap_or(0.0)
}

/// Helper to display ConstValue for error messages.
fn const_display(v: &ConstValue) -> String {
    format!("{}", v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constraint_greater_than() {
        let c = Constraint::GreaterThan(
            ConstraintExpr::Param("V_IN".to_string()),
            ConstraintExpr::Literal(ConstValue::Voltage(4.5)),
        );
        let resolve = |name: &str| -> Option<ConstValue> {
            match name {
                "V_IN" => Some(ConstValue::Voltage(12.0)),
                _ => None,
            }
        };
        assert!(c.check(&resolve).is_ok());
    }

    #[test]
    fn test_constraint_less_than_violated() {
        let c = Constraint::LessThan(
            ConstraintExpr::Param("V_OUT".to_string()),
            ConstraintExpr::Param("V_IN".to_string()),
        );
        let resolve = |name: &str| -> Option<ConstValue> {
            match name {
                "V_IN" => Some(ConstValue::Voltage(3.0)),
                "V_OUT" => Some(ConstValue::Voltage(5.0)),
                _ => None,
            }
        };
        assert!(c.check(&resolve).is_err());
    }

    #[test]
    fn test_constraint_satisfied() {
        let c = Constraint::LessEqual(
            ConstraintExpr::Param("I_MAX".to_string()),
            ConstraintExpr::Literal(ConstValue::Current(3.0)),
        );
        let resolve = |name: &str| -> Option<ConstValue> {
            match name {
                "I_MAX" => Some(ConstValue::Current(2.0)),
                _ => None,
            }
        };
        assert!(c.check(&resolve).is_ok());
    }

    #[test]
    fn test_constraint_expression_arithmetic() {
        // V_OUT < V_IN - 1V
        let c = Constraint::LessThan(
            ConstraintExpr::Param("V_OUT".to_string()),
            ConstraintExpr::BinaryOp {
                op: ConstraintOp::Sub,
                lhs: Box::new(ConstraintExpr::Param("V_IN".to_string())),
                rhs: Box::new(ConstraintExpr::Literal(ConstValue::Voltage(1.0))),
            },
        );
        let resolve = |name: &str| -> Option<ConstValue> {
            match name {
                "V_IN" => Some(ConstValue::Voltage(12.0)),
                "V_OUT" => Some(ConstValue::Voltage(3.3)),
                _ => None,
            }
        };
        assert!(c.check(&resolve).is_ok());
    }

    #[test]
    fn test_generic_param_creation() {
        let param = GenericParam {
            name: "V_IN".to_string(),
            param_type: GenericParamType::Const(BhdlType::Voltage(None)),
            constraints: vec![
                Constraint::GreaterEqual(
                    ConstraintExpr::Param("V_IN".to_string()),
                    ConstraintExpr::Literal(ConstValue::Voltage(4.5)),
                ),
                Constraint::LessEqual(
                    ConstraintExpr::Param("V_IN".to_string()),
                    ConstraintExpr::Literal(ConstValue::Voltage(40.0)),
                ),
            ],
            default: None,
        };
        assert_eq!(param.name, "V_IN");
        assert_eq!(param.constraints.len(), 2);
    }
}
