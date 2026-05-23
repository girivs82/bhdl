//! Evaluator for vendor-authored `design { }` blocks.
//!
//! Stage 3 of the vendor-extensibility surface (see
//! `docs/spec/Vendor_Design_Blocks.md`). Takes a [`DesignRecipe`] extracted
//! by the analyzer and produces a child-name → designed-value map by
//! walking each statement, evaluating each expression, and applying the
//! `require` validations.
//!
//! Expressions are stored on the recipe as raw source text. The evaluator
//! re-parses each one with `bhdl_parser::parse_expression` and walks the
//! resulting [`Expr`] AST recursively. Identifier resolution looks in:
//!
//!   1. block-local `const` bindings (from earlier statements of the same
//!      block)
//!   2. `intent.<param>` — the stamped `intent_<param>` instance attribute,
//!      parsed as a number
//!   3. `tube.<param>` — the tube's Koren parameters (currently the 6SN7
//!      defaults; stage 4 will read them off the triode child)
//!   4. bare parent-pin names (currently just `VBB`; future board context)
//!
//! Primitives `plate_current` and `koren_inverse_vgk` dispatch to
//! `bhdl_spice::triode` / `bhdl_spice::tube_bias`. The set of primitives is
//! deliberately small — vendors compose primitives, they don't define new
//! ones in HDL.

use std::collections::HashMap;
use bhdl_ast::SyntaxKind;
use bhdl_ast::expr::Expr;
use rowan::ast::AstNode;
use bhdl_common::design::{DesignRecipe, DesignStatement};

/// Errors the evaluator can report. Stage 3 keeps these as plain strings —
/// they're displayed once and dropped; promoting to a typed enum is easy
/// later when error UI starts caring about the cases.
#[derive(Debug, Clone)]
pub enum DesignEvalError {
    /// A `require` validation rejected the design.
    RequireFailed(String),
    /// Something went wrong evaluating an expression.
    EvalError(String),
}

impl std::fmt::Display for DesignEvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RequireFailed(msg) => write!(f, "require failed: {msg}"),
            Self::EvalError(msg) => write!(f, "{msg}"),
        }
    }
}

/// Context the evaluator resolves identifiers against.
pub struct DesignContext<'a> {
    /// Block-local bindings introduced by earlier `const NAME = EXPR;` lines.
    pub locals: HashMap<String, f64>,
    /// Stamped intent parameters from the parent instance — accessed as
    /// `intent.<name>`, looked up as `intent_<name>` in this map.
    pub intent_attrs: &'a HashMap<String, String>,
    /// Tube parameters — accessed as `tube.<param>`.
    pub tube_params: HashMap<String, f64>,
    /// Bare-name board context (e.g. `VBB` → 300.0).
    pub board: HashMap<String, f64>,
}

impl<'a> DesignContext<'a> {
    /// Resolve an identifier (possibly dotted) to a number.
    fn lookup(&self, name: &str) -> Result<f64, DesignEvalError> {
        let name = name.trim();
        if let Some((ns, field)) = name.split_once('.') {
            return match ns {
                "intent" => {
                    let key = format!("intent_{field}");
                    self.intent_attrs.get(&key)
                        .ok_or_else(|| DesignEvalError::EvalError(
                            format!("intent.{field} is not set on this instance")))
                        .and_then(|s| parse_literal(s).map_err(|_| DesignEvalError::EvalError(
                            format!("intent.{field} = '{s}' is not a number"))))
                }
                "tube" => self.tube_params.get(field).copied()
                    .ok_or_else(|| DesignEvalError::EvalError(
                        format!("tube.{field} is not in the parameter set"))),
                // `supply.<PIN>` reads a parent power-pin voltage. The
                // unqualified bare-name form is also accepted (the legacy
                // surface from stage 3); `supply.` is the spelling vendors
                // are encouraged to use because `board.` collides with the
                // `board` keyword in expression context.
                "supply" => self.board.get(field).copied()
                    .ok_or_else(|| DesignEvalError::EvalError(
                        format!("supply.{field} is not a power pin on the parent"))),
                _ => Err(DesignEvalError::EvalError(
                    format!("unknown namespace '{ns}' in identifier '{name}'"))),
            };
        }
        if let Some(v) = self.locals.get(name).copied() { return Ok(v); }
        if let Some(v) = self.board.get(name).copied() { return Ok(v); }
        Err(DesignEvalError::EvalError(format!("identifier '{name}' is not in scope")))
    }
}

/// Evaluate a complete design recipe.
///
/// `intent_attrs` should be the parent instance's attribute map (containing
/// `intent_<param>` keys stamped by `intent_attribute_stamper`). `board`
/// supplies bare-name values for parent pins (e.g. `VBB` from the power net
/// on the parent's `VBB` pin). Returns the child-name → value map, or
/// `Err(RequireFailed)` if a vendor `require` rejected the design.
pub fn evaluate_recipe(
    recipe: &DesignRecipe,
    intent_attrs: &HashMap<String, String>,
    board: HashMap<String, f64>,
) -> Result<HashMap<String, f64>, DesignEvalError> {
    // For stage 3 the tube parameters are the 6SN7 defaults — exactly what
    // `bhdl_spice::triode::TriodeParams::sn6_6sn7()` returns. Stage 4
    // generalises by reading them from the triode child of the entity.
    let mut tube_params = HashMap::new();
    let sn6 = bhdl_spice::triode::TriodeParams::sn6_6sn7();
    tube_params.insert("mu".into(),  sn6.mu);
    tube_params.insert("ex".into(),  sn6.ex);
    tube_params.insert("kg1".into(), sn6.kg1);
    tube_params.insert("kp".into(),  sn6.kp);
    tube_params.insert("kvb".into(), sn6.kvb);

    let mut ctx = DesignContext {
        locals: HashMap::new(),
        intent_attrs,
        tube_params,
        board,
    };

    let mut out: HashMap<String, f64> = HashMap::new();
    for stmt in &recipe.statements {
        match stmt {
            DesignStatement::Let { name, expr } => {
                let v = evaluate_text(expr, &ctx)?;
                ctx.locals.insert(name.clone(), v);
            }
            DesignStatement::Require { condition, message } => {
                let v = evaluate_text(condition, &ctx)?;
                // Truthy: non-zero. Comparisons (<, >, …) yield 1.0 / 0.0.
                if v == 0.0 {
                    return Err(DesignEvalError::RequireFailed(message.clone()));
                }
            }
            DesignStatement::Assign { child_name, expr } => {
                let v = evaluate_text(expr, &ctx)?;
                out.insert(child_name.clone(), v);
            }
        }
    }
    Ok(out)
}

/// Parse `text` as a BHDL expression and evaluate it against `ctx`.
fn evaluate_text(text: &str, ctx: &DesignContext) -> Result<f64, DesignEvalError> {
    let parse = bhdl_parser::parse_expression(text);
    if !parse.errors().is_empty() {
        return Err(DesignEvalError::EvalError(
            format!("expression parse errors: {:?}", parse.errors())));
    }
    let root = parse.syntax();
    let expr = root.descendants().find_map(Expr::cast)
        .ok_or_else(|| DesignEvalError::EvalError(
            format!("could not parse '{text}' as an expression")))?;
    evaluate_expr(&expr, ctx)
}

/// Walk an `Expr` AST and produce a number.
fn evaluate_expr(expr: &Expr, ctx: &DesignContext) -> Result<f64, DesignEvalError> {
    match expr {
        Expr::PrefixExpr(p) => {
            let inner = p.expr().ok_or_else(|| DesignEvalError::EvalError(
                "prefix expression missing operand".into()))?;
            let val = evaluate_expr(&inner, ctx)?;
            match p.op() {
                Some(SyntaxKind::MINUS) => Ok(-val),
                Some(SyntaxKind::PLUS)  => Ok(val),
                Some(op) => Err(DesignEvalError::EvalError(
                    format!("unsupported prefix op {op:?}"))),
                None => Err(DesignEvalError::EvalError(
                    "prefix expression missing operator".into())),
            }
        }
        Expr::BinaryExpr(b) => {
            let lhs_expr = b.lhs().ok_or_else(|| DesignEvalError::EvalError(
                "binary expression missing lhs".into()))?;
            let rhs_expr = b.rhs().ok_or_else(|| DesignEvalError::EvalError(
                "binary expression missing rhs".into()))?;
            let lhs = evaluate_expr(&lhs_expr, ctx)?;
            let rhs = evaluate_expr(&rhs_expr, ctx)?;
            let op = b.op().ok_or_else(|| DesignEvalError::EvalError(
                "binary expression missing operator".into()))?;
            apply_binop(op, lhs, rhs)
        }
        Expr::FunctionCallExpr(f) => {
            let name = f.function_name().or_else(|| f.name())
                .ok_or_else(|| DesignEvalError::EvalError(
                    "function call missing name".into()))?
                .text().to_string();
            // The standalone expression parser wraps the args in PARAM_LIST
            // (the named-parameter aware variant) rather than ARGUMENT_LIST,
            // so look for either — whichever subnode is present.
            let arg_list = f.syntax().children().find(|n|
                n.kind() == SyntaxKind::PARAM_LIST
                    || n.kind() == SyntaxKind::ARGUMENT_LIST);
            let args: Vec<f64> = match arg_list {
                Some(list) => list.children()
                    .filter_map(Expr::cast)
                    .map(|a| evaluate_expr(&a, ctx))
                    .collect::<Result<_, _>>()?,
                None => Vec::new(),
            };
            dispatch_primitive(&name, &args)
        }
        // For any other Expr variant fall through to text-based handling:
        // numeric literals, identifiers (including dotted paths like
        // `tube.mu`), and parenthesised sub-expressions all end up here.
        other => {
            let text = other.syntax().text().to_string();
            let trimmed = text.trim();
            // Strip a single outer pair of parens so `( 1 + 2 )` evaluates
            // even though the parser wrapped it as an Ident around a Bin.
            let bare = strip_outer_parens(trimmed);
            if let Ok(n) = parse_literal(bare) {
                return Ok(n);
            }
            ctx.lookup(bare)
        }
    }
}

fn strip_outer_parens(s: &str) -> &str {
    let s = s.trim();
    if s.starts_with('(') && s.ends_with(')') {
        let inner = &s[1..s.len() - 1];
        // Only strip if the outer parens balance — naïve but enough here.
        let mut depth = 0i32;
        let mut ok = true;
        for (i, ch) in inner.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth < 0 { ok = false; break; }
                    if depth < 0 && i + 1 < inner.len() { ok = false; break; }
                }
                _ => {}
            }
        }
        if ok && depth == 0 { return inner.trim(); }
    }
    s
}

fn apply_binop(op: SyntaxKind, lhs: f64, rhs: f64) -> Result<f64, DesignEvalError> {
    match op {
        SyntaxKind::PLUS    => Ok(lhs + rhs),
        SyntaxKind::MINUS   => Ok(lhs - rhs),
        SyntaxKind::STAR    => Ok(lhs * rhs),
        SyntaxKind::SLASH   => Ok(lhs / rhs),
        SyntaxKind::L_ANGLE => Ok(if lhs <  rhs { 1.0 } else { 0.0 }),
        SyntaxKind::R_ANGLE => Ok(if lhs >  rhs { 1.0 } else { 0.0 }),
        SyntaxKind::LTEQ    => Ok(if lhs <= rhs { 1.0 } else { 0.0 }),
        SyntaxKind::GTEQ    => Ok(if lhs >= rhs { 1.0 } else { 0.0 }),
        other => Err(DesignEvalError::EvalError(
            format!("unsupported binary operator {other:?}"))),
    }
}

/// Dispatch a primitive function call to the implementation in bhdl-spice.
fn dispatch_primitive(name: &str, args: &[f64]) -> Result<f64, DesignEvalError> {
    use bhdl_spice::triode::{plate_current, TriodeParams};
    use bhdl_spice::tube_bias::koren_inverse_vgk;
    match name {
        // plate_current(mu, ex, kg1, kp, kvb, vpk, vgk) → I_p
        "plate_current" => {
            if args.len() != 7 {
                return Err(DesignEvalError::EvalError(
                    format!("plate_current expects 7 args (mu, ex, kg1, kp, kvb, V_pk, V_gk), got {}",
                        args.len())));
            }
            let p = TriodeParams::new(args[0], args[1], args[2], args[3], args[4]);
            Ok(plate_current(&p, args[5], args[6]))
        }
        // koren_inverse_vgk(mu, ex, kg1, kp, kvb, vpk, target_ip) → V_gk
        "koren_inverse_vgk" => {
            if args.len() != 7 {
                return Err(DesignEvalError::EvalError(
                    format!("koren_inverse_vgk expects 7 args (mu, ex, kg1, kp, kvb, V_pk, I_p), got {}",
                        args.len())));
            }
            let p = TriodeParams::new(args[0], args[1], args[2], args[3], args[4]);
            Ok(koren_inverse_vgk(&p, args[5], args[6]))
        }
        other => Err(DesignEvalError::EvalError(
            format!("unknown primitive function '{other}' — the design \
                     evaluator currently knows plate_current, koren_inverse_vgk"))),
    }
}

/// Parse a numeric literal — bare or with a BHDL electrical unit suffix
/// (`100V`, `5mA`, `1MΩ`, …). Reuses `bhdl_spice::model_factory::parse_value`
/// so unit handling matches the rest of the toolchain.
fn parse_literal(text: &str) -> Result<f64, String> {
    let t = text.trim();
    if let Ok(n) = t.parse::<f64>() {
        return Ok(n);
    }
    bhdl_spice::model_factory::parse_value(t)
        .ok_or_else(|| format!("not a numeric literal: '{t}'"))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (HashMap<String, String>, HashMap<String, f64>) {
        let mut intent = HashMap::new();
        intent.insert("intent_name".into(), "current_source".into());
        intent.insert("intent_current".into(), "0.005".into());
        let mut board = HashMap::new();
        board.insert("VBB".into(), 300.0);
        (intent, board)
    }

    fn make_ctx<'a>(intent: &'a HashMap<String, String>, board: HashMap<String, f64>) -> DesignContext<'a> {
        let mut tube_params = HashMap::new();
        let sn6 = bhdl_spice::triode::TriodeParams::sn6_6sn7();
        tube_params.insert("mu".into(),  sn6.mu);
        tube_params.insert("ex".into(),  sn6.ex);
        tube_params.insert("kg1".into(), sn6.kg1);
        tube_params.insert("kp".into(),  sn6.kp);
        tube_params.insert("kvb".into(), sn6.kvb);
        DesignContext { locals: HashMap::new(), intent_attrs: intent, tube_params, board }
    }

    #[test]
    fn evaluates_arithmetic() {
        let (intent, board) = ctx();
        let c = make_ctx(&intent, board);
        assert_eq!(evaluate_text("1 + 2", &c).unwrap(), 3.0);
        assert_eq!(evaluate_text("10 - 3", &c).unwrap(), 7.0);
        assert_eq!(evaluate_text("4 * 5", &c).unwrap(), 20.0);
        assert_eq!(evaluate_text("20 / 4", &c).unwrap(), 5.0);
    }

    #[test]
    fn evaluates_intent_lookup() {
        let (intent, board) = ctx();
        let c = make_ctx(&intent, board);
        assert!((evaluate_text("intent.current", &c).unwrap() - 0.005).abs() < 1e-12);
    }

    #[test]
    fn evaluates_tube_lookup() {
        let (intent, board) = ctx();
        let c = make_ctx(&intent, board);
        assert_eq!(evaluate_text("tube.mu", &c).unwrap(), 20.0);
        assert_eq!(evaluate_text("tube.kp", &c).unwrap(), 470.0);
    }

    #[test]
    fn evaluates_board_pin_lookup() {
        let (intent, board) = ctx();
        let c = make_ctx(&intent, board);
        assert_eq!(evaluate_text("VBB", &c).unwrap(), 300.0);
        assert_eq!(evaluate_text("VBB / 2", &c).unwrap(), 150.0);
    }

    #[test]
    fn evaluates_plate_current_primitive() {
        let (intent, board) = ctx();
        let c = make_ctx(&intent, board);
        // plate_current of a 6SN7 at V_pk=250, V_gk=-8 — should be a few mA
        // (matches the reference operating point used elsewhere in tests).
        let i = evaluate_text(
            "plate_current(tube.mu, tube.ex, tube.kg1, tube.kp, tube.kvb, 250.0, 0.0 - 8.0)",
            &c
        ).unwrap();
        assert!((1e-3..30e-3).contains(&i), "plate_current = {i} A");
    }

    #[test]
    fn evaluates_full_current_source_recipe() {
        // The current_source designer expressed as a recipe; verify it
        // reproduces what bhdl_spice::tube_bias::design_current_source
        // returns for the same target current.
        let recipe = DesignRecipe {
            entity_name: "Foo".into(),
            intent_name: "current_source".into(),
            statements: vec![
                DesignStatement::Let { name: "v_pk".into(), expr: "100.0".into() },
                DesignStatement::Let { name: "i_target".into(), expr: "intent.current".into() },
                DesignStatement::Let { name: "i_max".into(),
                    expr: "plate_current(tube.mu, tube.ex, tube.kg1, tube.kp, tube.kvb, v_pk, 0.0)".into() },
                DesignStatement::Require {
                    condition: "i_target < i_max".into(),
                    message: "current target exceeds zero-bias capacity".into(),
                },
                DesignStatement::Let { name: "v_gk".into(),
                    expr: "koren_inverse_vgk(tube.mu, tube.ex, tube.kg1, tube.kp, tube.kvb, v_pk, i_target)".into() },
                DesignStatement::Assign { child_name: "Rk".into(),
                    expr: "(0.0 - v_gk) / i_target".into() },
            ],
        };
        let (intent, board) = ctx();
        let out = evaluate_recipe(&recipe, &intent, board).expect("recipe eval failed");
        let r_k = *out.get("Rk").expect("Rk not assigned");
        // Reference: the Rust path that this recipe re-expresses.
        let p = bhdl_spice::triode::TriodeParams::sn6_6sn7();
        let r_k_ref = bhdl_spice::tube_bias::design_current_source(&p, 0.005).unwrap();
        let rel = (r_k - r_k_ref).abs() / r_k_ref;
        assert!(rel < 1e-9, "R_k from recipe {r_k}, from Rust {r_k_ref}");
    }

    #[test]
    fn require_failure_is_propagated() {
        // A current target that exceeds the tube's zero-bias capacity must
        // be rejected by the recipe's `require` statement.
        let recipe = DesignRecipe {
            entity_name: "Foo".into(),
            intent_name: "current_source".into(),
            statements: vec![
                DesignStatement::Let { name: "v_pk".into(), expr: "100.0".into() },
                DesignStatement::Let { name: "i_target".into(), expr: "intent.current".into() },
                DesignStatement::Let { name: "i_max".into(),
                    expr: "plate_current(tube.mu, tube.ex, tube.kg1, tube.kp, tube.kvb, v_pk, 0.0)".into() },
                DesignStatement::Require {
                    condition: "i_target < i_max".into(),
                    message: "current target exceeds zero-bias capacity".into(),
                },
            ],
        };
        // 1 A from a 6SN7 — far beyond i_max.
        let mut intent = HashMap::new();
        intent.insert("intent_current".into(), "1.0".into());
        let board = HashMap::new();
        let err = evaluate_recipe(&recipe, &intent, board).expect_err("expected require failure");
        match err {
            DesignEvalError::RequireFailed(_) => {}
            other => panic!("expected RequireFailed, got {other}"),
        }
    }
}
