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
    /// A foreign-language body hook (Stage 5) failed at runtime —
    /// parse error in the script, fuel exhaustion, wrong return shape,
    /// host-function panic, etc. Carries the underlying engine's
    /// diagnostic verbatim so the user sees the line and reason.
    ScriptFailed(String),
}

impl std::fmt::Display for DesignEvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RequireFailed(msg) => write!(f, "require failed: {msg}"),
            Self::EvalError(msg) => write!(f, "{msg}"),
            Self::ScriptFailed(msg) => write!(f, "script failed: {msg}"),
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

/// Cheap predicate over a recipe's source to decide whether device
/// parameters are required at evaluation time. We choose a textual
/// over-approximation because (a) the declarative-path expression
/// language stores statements as raw text and re-parses them in
/// `evaluate_text`, and (b) the body-hook path stores the Rhai script
/// as one big string — in both cases the cheapest reliable signal is
/// substring presence of `tube`. A false positive (recipe text
/// mentions `tube` only in a comment) merely produces an early error
/// when a tube *is* expected; a false negative (recipe avoids the
/// substring entirely while still using tube parameters somehow)
/// can't happen because there is no other way to access them.
fn recipe_needs_device(recipe: &DesignRecipe) -> bool {
    if let Some(body) = &recipe.body {
        if body.inputs.iter().any(|n| n == "tube") { return true; }
        if body.source.contains("tube") { return true; }
    }
    for stmt in &recipe.statements {
        let exprs: &[&str] = match stmt {
            DesignStatement::Let { expr, .. } => &[expr.as_str()],
            DesignStatement::Require { condition, .. } => &[condition.as_str()],
            DesignStatement::Assign { expr, .. } => &[expr.as_str()],
        };
        if exprs.iter().any(|e| e.contains("tube.") || e.contains("tube ")) {
            return true;
        }
    }
    false
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
    device: HashMap<String, f64>,
) -> Result<HashMap<String, f64>, DesignEvalError> {
    // Stage 6: device-family parameters come from the actual expansion
    // child the synthesizer identified via the `component_class` discovery
    // rule. We deliberately do NOT silently substitute a default tube
    // set when the map is empty — silent defaults hide wiring bugs that
    // produce wrong-looking but plausible answers. If a recipe asks for
    // `tube.<param>` (declarative) or declares `tube` as an input (body
    // hook) and the synthesizer didn't find a qualifying device, fail
    // loudly so the vendor / board author sees the configuration error.
    let tube_params = device;
    if recipe_needs_device(recipe) && tube_params.is_empty() {
        return Err(DesignEvalError::EvalError(format!(
            "design recipe for '{}'.'{}' refers to `tube.*` parameters \
             but the synthesizer didn't discover a device child with \
             component_class = \"triode\" in the expansion block. Check \
             that the entity's expansion instantiates a triode (Triode, \
             Triode12AU7, …) and that its `component_class` attribute is \
             set.", recipe.entity_name, recipe.intent_name)));
    }

    // Stage 5 — foreign-language body hook takes precedence. The
    // analyzer guarantees mutual exclusion (body wins; statements are
    // dropped with a warning at extraction time), so this branch is
    // an early return: if the recipe carries a body, the script owns
    // the outputs entirely.
    if recipe.body.is_some() {
        return evaluate_body_hook(recipe, intent_attrs, &tube_params, &board);
    }

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
// Stage 5 — foreign-language body hooks (Rhai)
// ─────────────────────────────────────────────────────────────────────────────

/// Evaluate a [`DesignRecipe`] whose body is a foreign-language hook.
///
/// Currently only `language = "rhai"` is dispatched; other languages
/// produce a clear ScriptFailed diagnostic at synth time so the user
/// learns the issue is missing language support rather than a script
/// bug.
///
/// The Rhai script sees three variables in its scope:
///
/// - `tube`    — an object map of the device's parameters (`mu`, `ex`,
///   `kg1`, `kp`, `kvb` for triodes). Vendors who want to do their own
///   math have direct access; the host primitives also take this map
///   as their first argument.
/// - `intent`  — an object map of the parsed intent parameters
///   (`target_gain`, `current`, …). Numeric values where the input
///   parsed as a number; raw strings otherwise.
/// - `supply`  — an object map of the parent's power-pin voltages
///   (`VBB`, `VCC`, …).
///
/// The script must return a Rhai `Map` whose keys are the names the
/// expansion interpreter expects (the entity's `outputs { … }`
/// declaration). Each value is coerced to `f64`. Missing keys cause a
/// ScriptFailed error; extra keys are ignored with a one-line warning.
fn evaluate_body_hook(
    recipe: &DesignRecipe,
    intent_attrs: &HashMap<String, String>,
    tube_params: &HashMap<String, f64>,
    board: &HashMap<String, f64>,
) -> Result<HashMap<String, f64>, DesignEvalError> {
    use rhai::{Dynamic, Engine, Map, Scope};

    let body = recipe.body.as_ref().ok_or_else(|| {
        DesignEvalError::ScriptFailed(
            "internal: evaluate_body_hook called without a body".to_string())
    })?;

    if body.language != "rhai" {
        return Err(DesignEvalError::ScriptFailed(format!(
            "unknown body language '{}' — only 'rhai' is supported in this build",
            body.language)));
    }

    // Build (cache later) the Rhai engine with our host functions and
    // sandbox limits applied.
    let engine = build_rhai_engine();

    // Marshal inputs into the script's scope as Rhai object maps.
    let mut scope = Scope::new();
    scope.push("tube",   hashmap_to_rhai_map(tube_params));
    scope.push("intent", intent_attrs_to_rhai_map(intent_attrs));
    scope.push("supply", hashmap_to_rhai_map(board));

    let result: Dynamic = engine
        .eval_with_scope::<Dynamic>(&mut scope, &body.source)
        .map_err(|e| DesignEvalError::ScriptFailed(format_rhai_error(
            &recipe.entity_name, &recipe.intent_name, &body.source, &e)))?;

    // The script's return must be a Map. Coerce each declared output
    // to f64. Honour the entity's `outputs { … }` list when present —
    // if the script omits a declared output, fail loudly; if it
    // returns extras, warn but ignore.
    let map: Map = result.try_cast::<Map>().ok_or_else(|| {
        DesignEvalError::ScriptFailed(format!(
            "script for '{}'.'{}' did not return a map — got something else",
            recipe.entity_name, recipe.intent_name))
    })?;

    let mut out = HashMap::with_capacity(body.outputs.len().max(map.len()));
    if body.outputs.is_empty() {
        // Schema-less mode: take whatever the script returned. Coerce
        // each value to f64 best-effort; non-numeric entries are an
        // error since the expansion interpreter consumes a numeric map.
        for (k, v) in map.into_iter() {
            let f = rhai_dynamic_to_f64(&v).ok_or_else(|| {
                DesignEvalError::ScriptFailed(format!(
                    "script for '{}'.'{}' returned non-numeric value at key '{k}'",
                    recipe.entity_name, recipe.intent_name))
            })?;
            out.insert(k.to_string(), f);
        }
    } else {
        // Schema-checked mode: pick the declared outputs, fail on any
        // missing key. Extras (script returned more than declared) are
        // dropped with a one-shot diagnostic per recipe.
        let mut extra_keys = Vec::new();
        for k in map.keys() {
            if !body.outputs.iter().any(|o| o.as_str() == k.as_str()) {
                extra_keys.push(k.to_string());
            }
        }
        if !extra_keys.is_empty() {
            log::warn!(
                "Vendor design recipe for '{}'.'{}' returned undeclared output(s): {:?} — ignored",
                recipe.entity_name, recipe.intent_name, extra_keys);
        }
        for name in &body.outputs {
            let v = map.get(name.as_str()).ok_or_else(|| {
                DesignEvalError::ScriptFailed(format!(
                    "script for '{}'.'{}' did not populate declared output '{name}'",
                    recipe.entity_name, recipe.intent_name))
            })?;
            let f = rhai_dynamic_to_f64(v).ok_or_else(|| {
                DesignEvalError::ScriptFailed(format!(
                    "script for '{}'.'{}' produced non-numeric value for '{name}'",
                    recipe.entity_name, recipe.intent_name))
            })?;
            out.insert(name.clone(), f);
        }
    }
    Ok(out)
}

/// Construct a Rhai engine with the BHDL host functions registered and
/// sandbox limits applied. The default fuel limit is conservative
/// (1M operations ≈ tens of ms wall time on typical scripts); vendors
/// whose recipes legitimately need more should declare it via the
/// recipe's `runtime` clause (future work).
fn build_rhai_engine() -> rhai::Engine {
    use rhai::{Dynamic, Engine, Map};
    let mut engine = Engine::new();

    // Sandboxing: no module imports, no progress callback, fuel-limited.
    engine.set_max_operations(1_000_000);
    engine.set_max_call_levels(64);
    engine.set_max_expr_depths(64, 32);

    // Host function: plate_current(tube, v_pk, v_gk) -> f64
    // Vendors pass the `tube` map they got in their scope; the host
    // unpacks the five Koren parameters and dispatches to bhdl-spice.
    engine.register_fn("plate_current",
        |tube: Map, v_pk: f64, v_gk: f64| -> f64 {
            let p = tube_params_from_map(&tube);
            bhdl_spice::triode::plate_current(&p, v_pk, v_gk)
        });

    // Host function: koren_inverse_vgk(tube, v_pk, target_ip) -> f64
    engine.register_fn("koren_inverse_vgk",
        |tube: Map, v_pk: f64, ip: f64| -> f64 {
            let p = tube_params_from_map(&tube);
            bhdl_spice::tube_bias::koren_inverse_vgk(&p, v_pk, ip)
        });

    // Host function: conductances(tube, v_pk, v_gk) -> #{ g_p: …, g_m: … }
    // Rhai doesn't have native tuples ergonomically; we return a map
    // with named fields so the script reads `cond.g_p` / `cond.g_m`.
    engine.register_fn("conductances",
        |tube: Map, v_pk: f64, v_gk: f64| -> Map {
            let p = tube_params_from_map(&tube);
            let (g_p, g_m) = bhdl_spice::triode::conductances(&p, v_pk, v_gk);
            let mut m = Map::new();
            m.insert("g_p".into(), Dynamic::from_float(g_p));
            m.insert("g_m".into(), Dynamic::from_float(g_m));
            m
        });

    // Host convenience: small_signal_gain(tube, v_pk, i_p) -> f64
    // The composite the amplifier designer uses repeatedly:
    // g_m · (R_p ∥ r_p) at the operating point set by (v_pk, i_p), with
    // R_p assumed to be v_pk / i_p (a designer holding the plate at v_pk).
    engine.register_fn("small_signal_gain",
        |tube: Map, v_pk: f64, i_p: f64| -> f64 {
            let p = tube_params_from_map(&tube);
            let r_p = v_pk / i_p;
            let v_gk = bhdl_spice::tube_bias::koren_inverse_vgk(&p, v_pk, i_p);
            let (g_p, g_m) = bhdl_spice::triode::conductances(&p, v_pk, v_gk);
            g_m / (g_p + 1.0 / r_p)
        });

    engine
}

/// Convert a [`HashMap<String, f64>`] into a Rhai object map.
fn hashmap_to_rhai_map(h: &HashMap<String, f64>) -> rhai::Map {
    use rhai::Dynamic;
    let mut m = rhai::Map::new();
    for (k, v) in h {
        m.insert(k.as_str().into(), Dynamic::from_float(*v));
    }
    m
}

/// Build the `intent` Rhai map from the stamped `intent_<param>` attrs.
/// Strips the `intent_` prefix; coerces numeric-looking values to f64,
/// otherwise stores as string so the script can decide what to do.
fn intent_attrs_to_rhai_map(intent_attrs: &HashMap<String, String>) -> rhai::Map {
    use rhai::Dynamic;
    let mut m = rhai::Map::new();
    for (k, v) in intent_attrs {
        let name = k.strip_prefix("intent_").unwrap_or(k.as_str());
        if let Ok(f) = parse_literal(v) {
            m.insert(name.into(), Dynamic::from_float(f));
        } else {
            m.insert(name.into(), Dynamic::from(v.clone()));
        }
    }
    m
}

/// Read tube parameters back out of the Rhai object map we sent in.
/// Missing keys default to 0.0 — the script is allowed to override
/// individual params before calling primitives, but typical recipes
/// just thread the `tube` map straight through.
fn tube_params_from_map(map: &rhai::Map) -> bhdl_spice::triode::TriodeParams {
    let g = |k: &str| map.get(k).and_then(|d| d.as_float().ok()).unwrap_or(0.0);
    bhdl_spice::triode::TriodeParams {
        mu:  g("mu"),
        ex:  g("ex"),
        kg1: g("kg1"),
        kp:  g("kp"),
        kvb: g("kvb"),
    }
}

/// Coerce a Rhai `Dynamic` to `f64`. Accepts Rhai floats and integers;
/// returns None for any other shape (strings, maps, arrays, …).
fn rhai_dynamic_to_f64(d: &rhai::Dynamic) -> Option<f64> {
    if let Ok(f) = d.as_float() { return Some(f); }
    if let Ok(i) = d.as_int() { return Some(i as f64); }
    None
}

/// Format a Rhai evaluation error with the offending source line and a
/// caret pointer. Rhai's bare diagnostic gives us `(line N, position M)`
/// relative to the script body but no context; vendors authoring a
/// `body rhai r#"..."#` block need to see *which* line went wrong
/// without manually counting newlines inside their raw-string literal.
///
/// Output shape:
///
/// ```text
/// vendor design recipe 'SignalTubeStage'.'amplifier' — Rhai eval failed:
///     Function not found: pow (f64, f64)
/// at script line 13, column 46:
///     12 |             let ratio = i_hi / i_lo;
///     13 |             let i = i_lo * ratio.pow(frac);
///        |                                          ^
///     14 |             let g = small_signal_gain(tube, v_p, i);
/// ```
///
/// When the line / position can't be parsed out of Rhai's message
/// (different error class, or future Rhai version changes the format)
/// we fall back to the previous one-liner so the user still sees the
/// raw diagnostic.
fn format_rhai_error(
    entity_name: &str,
    intent_name: &str,
    source: &str,
    err: &rhai::EvalAltResult,
) -> String {
    use rhai::EvalAltResult;
    // Rhai's Position is on the inner error for parse / runtime errors;
    // we ask it directly rather than parse text out of the Display impl.
    let pos = match err {
        EvalAltResult::ErrorParsing(_, p) => Some(*p),
        e => {
            let p = e.position();
            if p.is_none() { None } else { Some(p) }
        }
    };

    let header = format!(
        "vendor design recipe '{entity_name}'.'{intent_name}' — Rhai eval failed:\n    {err}"
    );

    let (line, col) = match pos.and_then(|p| Some((p.line()?, p.position()?))) {
        Some(lc) => lc,
        None => return header, // no position info available
    };

    let lines: Vec<&str> = source.lines().collect();
    let idx = line.saturating_sub(1);
    if idx >= lines.len() { return header; }

    // Determine the printed-prefix width from the largest line number
    // we'll show (line + 1) so the caret aligns with the column.
    let max_line_no = (line + 1).min(lines.len());
    let prefix_w = max_line_no.to_string().len();

    let mut buf = String::new();
    use std::fmt::Write;
    let _ = writeln!(&mut buf, "{header}");
    let _ = writeln!(&mut buf, "at script line {line}, column {col}:");

    // Print a one-line context window: previous line (if any), the
    // offending line, then the caret, then the following line.
    if idx > 0 {
        let _ = writeln!(&mut buf,
            "    {:>w$} | {}", idx, lines[idx - 1], w = prefix_w);
    }
    let _ = writeln!(&mut buf,
        "    {:>w$} | {}", line, lines[idx], w = prefix_w);
    // Caret: column is 1-indexed. The source-content column in our
    // output line starts after `    ` (4 spaces) + line number
    // (prefix_w chars) + ` | ` (3 chars). The caret offset within the
    // content portion is `col - 1`. We emit a "gutter-only" prefix
    // (matching the line-number column with spaces) then `col - 1`
    // spaces then '^'.
    let _ = writeln!(&mut buf,
        "    {} | {}^",
        " ".repeat(prefix_w),
        " ".repeat(col.saturating_sub(1)));
    if idx + 1 < lines.len() {
        let _ = writeln!(&mut buf,
            "    {:>w$} | {}", line + 1, lines[idx + 1], w = prefix_w);
    }
    buf
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

    /// Default 6SN7 device map for tests that don't care which tube they
    /// design for. Tests exercising tube-rolling (e.g.
    /// `device_params_flow_through_to_script`) build their own device
    /// map and don't go through this helper.
    fn default_device() -> HashMap<String, f64> {
        let mut d = HashMap::new();
        let sn6 = bhdl_spice::triode::TriodeParams::sn6_6sn7();
        d.insert("mu".into(),  sn6.mu);
        d.insert("ex".into(),  sn6.ex);
        d.insert("kg1".into(), sn6.kg1);
        d.insert("kp".into(),  sn6.kp);
        d.insert("kvb".into(), sn6.kvb);
        d
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
            body: None,
        };
        let (intent, board) = ctx();
        let out = evaluate_recipe(&recipe, &intent, board, default_device()).expect("recipe eval failed");
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
            body: None,
        };
        // 1 A from a 6SN7 — far beyond i_max.
        let mut intent = HashMap::new();
        intent.insert("intent_current".into(), "1.0".into());
        let board = HashMap::new();
        let err = evaluate_recipe(&recipe, &intent, board, default_device()).expect_err("expected require failure");
        match err {
            DesignEvalError::RequireFailed(_) => {}
            other => panic!("expected RequireFailed, got {other}"),
        }
    }

    // ─── Stage 5: Rhai body-hook evaluator ──────────────────────────────

    #[test]
    fn body_hook_returns_declared_outputs() {
        // The simplest body hook: arithmetic on intent + supply,
        // returning a map with the declared outputs. Verifies the
        // marshalling round-trip (in: scope vars, out: typed map).
        use bhdl_common::design::DesignBody;
        let recipe = DesignRecipe {
            entity_name: "Foo".into(),
            intent_name: "current_source".into(),
            statements: Vec::new(),
            body: Some(DesignBody {
                language: "rhai".into(),
                inputs: vec!["intent".into(), "supply".into()],
                outputs: vec!["Rk".into()],
                source: r#"
                    let i = intent.current;
                    let v = supply.VBB;
                    #{ Rk: (v - 100.0) / i / 10.0 }
                "#.into(),
            }),
        };
        let (intent, board) = ctx();
        let out = evaluate_recipe(&recipe, &intent, board, HashMap::new()).expect("body hook eval");
        // Manual check: (300 - 100) / 0.005 / 10 = 4000.0
        let r_k = *out.get("Rk").expect("Rk missing from script output");
        assert!((r_k - 4000.0).abs() < 1e-6, "Rk = {r_k}, expected 4000");
    }

    #[test]
    fn body_hook_invokes_host_primitive() {
        // The script calls a BHDL host function (`plate_current`).
        // Verifies the host registration path: vendor scripts get
        // direct access to bhdl-spice numerics without re-implementing
        // the Koren equations.
        use bhdl_common::design::DesignBody;
        let recipe = DesignRecipe {
            entity_name: "Foo".into(),
            intent_name: "current_source".into(),
            statements: Vec::new(),
            body: Some(DesignBody {
                language: "rhai".into(),
                inputs: vec!["tube".into()],
                outputs: vec!["i_max".into()],
                source: r#"
                    let i = plate_current(tube, 100.0, 0.0);
                    #{ i_max: i }
                "#.into(),
            }),
        };
        let (intent, board) = ctx();
        let out = evaluate_recipe(&recipe, &intent, board, default_device()).expect("body hook eval");
        let i_max = *out.get("i_max").expect("i_max missing");
        // Compare against the Rust call directly — the host function
        // is just a thin wrapper, so the values must match exactly.
        let p = bhdl_spice::triode::TriodeParams::sn6_6sn7();
        let i_ref = bhdl_spice::triode::plate_current(&p, 100.0, 0.0);
        assert!((i_max - i_ref).abs() < 1e-9, "i_max = {i_max}, ref = {i_ref}");
    }

    #[test]
    fn body_hook_loop_amplifier_first_guess() {
        // The whole point of Stage 5: a body hook can iterate. This
        // is the amplifier first-guess bisection ported to Rhai —
        // log-grid peak find then descending-flank bisection on the
        // target gain. The output (Rp, Rk) is compared against the
        // Rust reference designer that does the same computation.
        use bhdl_common::design::DesignBody;
        let recipe = DesignRecipe {
            entity_name: "Foo".into(),
            intent_name: "amplifier".into(),
            statements: Vec::new(),
            body: Some(DesignBody {
                language: "rhai".into(),
                inputs: vec!["tube".into(), "intent".into(), "supply".into()],
                outputs: vec!["Rp".into(), "Rk".into()],
                source: r#"
                    let v_p   = supply.VBB / 2.0;
                    let i_lo  = 0.5e-3;
                    let i_max = plate_current(tube, v_p, 0.0);
                    let i_hi  = if 30e-3 < 0.85 * i_max { 30e-3 } else { 0.85 * i_max };

                    // Log-grid peak find.
                    let peak_i = i_lo;
                    let peak_g = 0.0;
                    for k in 0..64 {
                        let frac = (k.to_float()) / 63.0;
                        let ratio = i_hi / i_lo;
                        let i = i_lo * (ratio ** frac);
                        let g = small_signal_gain(tube, v_p, i);
                        if g > peak_g { peak_g = g; peak_i = i; }
                    }
                    // Descending-flank bisection.
                    let lo = peak_i;
                    let hi = i_hi;
                    for _step in 0..80 {
                        let mid = (lo * hi).sqrt();
                        let g = small_signal_gain(tube, v_p, mid);
                        if g > intent.target_gain { lo = mid; } else { hi = mid; }
                    }
                    let i_p  = (lo * hi).sqrt();
                    let v_gk = koren_inverse_vgk(tube, v_p, i_p);
                    #{ Rp: v_p / i_p, Rk: (-v_gk) / i_p }
                "#.into(),
            }),
        };
        let mut intent = HashMap::new();
        intent.insert("intent_name".into(), "amplifier".into());
        intent.insert("intent_target_gain".into(), "14.0".into());
        let mut board = HashMap::new();
        board.insert("VBB".into(), 300.0);

        let out = evaluate_recipe(&recipe, &intent, board, default_device()).expect("amplifier body hook");
        let rp = *out.get("Rp").expect("Rp missing");
        let rk = *out.get("Rk").expect("Rk missing");

        // Reference: the Rust first-guess for the same inputs (we
        // ignore the GLACIER refine, which is the framework's job).
        let p = bhdl_spice::triode::TriodeParams::sn6_6sn7();
        let ref_net = bhdl_spice::tube_bias::TriodeAmplifierDesigner::first_guess(
            &bhdl_spice::tube_bias::ReferenceTriodeDesigner,
            &p, 300.0,
            &bhdl_spice::tube_bias::AmplifierSpec::gain(14.0),
        ).expect("Rust reference first_guess");

        // Both designers do the same bisection, but Rhai's f64 chain
        // differs slightly from Rust's at the last few ulps. Accept
        // 0.5 % relative — well below the loose-tolerance bounds the
        // refine loop converges to.
        let rel_rp = (rp - ref_net.r_plate).abs() / ref_net.r_plate;
        let rel_rk = (rk - ref_net.r_cathode).abs() / ref_net.r_cathode;
        assert!(rel_rp < 0.005,
            "Rp from script {rp:.1} Ω, Rust {:.1} Ω (rel {:.4})",
            ref_net.r_plate, rel_rp);
        assert!(rel_rk < 0.005,
            "Rk from script {rk:.1} Ω, Rust {:.1} Ω (rel {:.4})",
            ref_net.r_cathode, rel_rk);
    }

    #[test]
    fn device_params_flow_through_to_script() {
        // Stage 6: when the caller supplies device parameters explicitly,
        // they reach the Rhai script via the `tube` map. A 6SN7 and a
        // 12AU7 ask for very different operating points at the same
        // gain target — verifying that different device params actually
        // produce different Rp/Rk proves the wiring isn't silently
        // ignored.
        use bhdl_common::design::DesignBody;
        let recipe = DesignRecipe {
            entity_name: "Foo".into(),
            intent_name: "amplifier".into(),
            statements: Vec::new(),
            body: Some(DesignBody {
                language: "rhai".into(),
                inputs:  vec!["tube".into(), "intent".into(), "supply".into()],
                outputs: vec!["Rp".into(), "Rk".into()],
                // Minimal closed-form designer for the test — pins V_p at
                // V_bb/2 and uses the tube's mu directly so it depends on
                // the device map in an obvious way.
                source: r#"
                    let v_p = supply.VBB / 2.0;
                    let i_p = 0.005;
                    let v_gk = koren_inverse_vgk(tube, v_p, i_p);
                    #{ Rp: v_p / i_p, Rk: (-v_gk) / i_p }
                "#.into(),
            }),
        };
        let mut intent = HashMap::new();
        intent.insert("intent_gain".into(), "14.0".into());
        let mut board = HashMap::new();
        board.insert("VBB".into(), 300.0);

        // 6SN7 device — what the synthesizer would discover from the
        // stdlib Triode entity's attributes today.
        let mut dev_6sn7 = HashMap::new();
        let sn6 = bhdl_spice::triode::TriodeParams::sn6_6sn7();
        dev_6sn7.insert("mu".into(),  sn6.mu);
        dev_6sn7.insert("ex".into(),  sn6.ex);
        dev_6sn7.insert("kg1".into(), sn6.kg1);
        dev_6sn7.insert("kp".into(),  sn6.kp);
        dev_6sn7.insert("kvb".into(), sn6.kvb);

        // 12AU7 device — what a vendor "rolling" a different tube into
        // the stage would have the synthesizer discover instead.
        let mut dev_12au7 = HashMap::new();
        let ecc = bhdl_spice::triode::TriodeParams::ecc82_12au7();
        dev_12au7.insert("mu".into(),  ecc.mu);
        dev_12au7.insert("ex".into(),  ecc.ex);
        dev_12au7.insert("kg1".into(), ecc.kg1);
        dev_12au7.insert("kp".into(),  ecc.kp);
        dev_12au7.insert("kvb".into(), ecc.kvb);

        let out_6sn7 = evaluate_recipe(&recipe, &intent, board.clone(), dev_6sn7).expect("6SN7 eval");
        let out_12au7 = evaluate_recipe(&recipe, &intent, board, dev_12au7).expect("12AU7 eval");

        let rk_6sn7  = *out_6sn7.get("Rk").unwrap();
        let rk_12au7 = *out_12au7.get("Rk").unwrap();
        // Same V_p, same I_p — R_p is identical between tubes (this is the
        // expected closed-form behavior of the test designer). R_k differs
        // because V_gk for 5 mA at V_p=150 V depends on the tube's kp.
        // Both tubes happen to want similar self-bias points here (the
        // 6SN7 and 12AU7 are both medium-µ designs); we just need to see
        // non-equality to prove the device map actually reaches the script.
        assert!(
            (rk_6sn7 - rk_12au7).abs() > 1.0,
            "Rk should differ between tubes — wiring would otherwise produce \
             byte-equal results: 6SN7 {rk_6sn7:.1} Ω, 12AU7 {rk_12au7:.1} Ω"
        );
    }

    #[test]
    fn body_hook_script_error_shows_source_line_and_caret() {
        // Deliberately syntactically broken Rhai — `for _ in` (using
        // `_` as a loop variable name) was the bug we hit during the
        // amplifier migration. The diagnostic should show the offending
        // line and a caret at the column Rhai reports.
        use bhdl_common::design::DesignBody;
        let recipe = DesignRecipe {
            entity_name: "Foo".into(),
            intent_name: "amplifier".into(),
            statements: Vec::new(),
            body: Some(DesignBody {
                language: "rhai".into(),
                inputs:  vec!["intent".into()],
                outputs: vec!["Rp".into()],
                // The error position is on line 4 — `for _` is rejected
                // because Rhai requires a real identifier.
                source: r#"
let v = 1.0;
let n = 80;
for _ in 0..n {
    v = v * 2.0;
}
#{ Rp: v }
"#.into(),
            }),
        };
        let mut intent = HashMap::new();
        intent.insert("intent_gain".into(), "14.0".into());
        let board = HashMap::new();

        let err = evaluate_recipe(&recipe, &intent, board, default_device())
            .expect_err("script should fail");
        let msg = match err {
            DesignEvalError::ScriptFailed(m) => m,
            other => panic!("expected ScriptFailed, got {other:?}"),
        };

        // Diagnostic shape checks. Order matters — header first, then
        // location pointer, then the source preview.
        assert!(
            msg.contains("'Foo'.'amplifier'"),
            "error should name the recipe: {msg}"
        );
        assert!(
            msg.contains("at script line"),
            "error should call out the script-relative line: {msg}"
        );
        assert!(
            msg.contains("for _ in 0..n"),
            "error should show the offending source line: {msg}"
        );
        assert!(
            msg.contains("^"),
            "error should include a caret pointing at the column: {msg}"
        );
    }

    #[test]
    fn body_hook_missing_declared_output_is_reported() {
        // The script returns a map missing one of the declared outputs;
        // the evaluator must surface that as ScriptFailed rather than
        // silently dropping the design.
        use bhdl_common::design::DesignBody;
        let recipe = DesignRecipe {
            entity_name: "Foo".into(),
            intent_name: "current_source".into(),
            statements: Vec::new(),
            body: Some(DesignBody {
                language: "rhai".into(),
                inputs:  vec!["intent".into()],
                outputs: vec!["Rp".into(), "Rk".into()],
                source: r#" #{ Rp: 1000.0 } "#.into(),
            }),
        };
        let (intent, board) = ctx();
        let err = evaluate_recipe(&recipe, &intent, board, HashMap::new()).expect_err("missing output");
        match err {
            DesignEvalError::ScriptFailed(msg) => {
                assert!(msg.contains("Rk"),
                    "expected message to mention missing 'Rk', got: {msg}");
            }
            other => panic!("expected ScriptFailed, got {other:?}"),
        }
    }
}
