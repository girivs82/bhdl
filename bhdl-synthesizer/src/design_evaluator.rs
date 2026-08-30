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
    /// Device parameters — accessed as `<device_class>.<param>`. The
    /// namespace name is the `component_class` string the synthesizer
    /// discovered (e.g. "triode" → `triode.mu`, "bjt" → `bjt.bf`). For
    /// backwards compatibility, the legacy hardcoded namespace `tube`
    /// also resolves into this map when the discovered class is "triode".
    pub device_class: String,
    pub device_params: HashMap<String, f64>,
    /// Bare-name board context (e.g. `VBB` → 300.0).
    pub board: HashMap<String, f64>,
}

impl<'a> DesignContext<'a> {
    /// True if the dotted prefix names the discovered device family.
    /// Matches exact class string (`triode`, `bjt`, …) and the
    /// legacy `tube` alias for triodes.
    fn ns_matches_device(&self, ns: &str) -> bool {
        ns == self.device_class
            || (ns == "tube" && self.device_class == "triode")
    }

    /// Resolve an identifier (possibly dotted) to a number.
    fn lookup(&self, name: &str) -> Result<f64, DesignEvalError> {
        let name = name.trim();
        if let Some((ns, field)) = name.split_once('.') {
            if ns == "intent" {
                let key = format!("intent_{field}");
                return self.intent_attrs.get(&key)
                    .ok_or_else(|| DesignEvalError::EvalError(
                        format!("intent.{field} is not set on this instance")))
                    .and_then(|s| parse_literal(s).map_err(|_| DesignEvalError::EvalError(
                        format!("intent.{field} = '{s}' is not a number"))));
            }
            if self.ns_matches_device(ns) {
                return self.device_params.get(field).copied()
                    .ok_or_else(|| DesignEvalError::EvalError(
                        format!("{ns}.{field} is not in the discovered device's parameter set")));
            }
            // `supply.<PIN>` reads a parent power-pin voltage. The
            // unqualified bare-name form is also accepted (the legacy
            // surface from stage 3); `supply.` is the spelling vendors
            // are encouraged to use because `board.` collides with the
            // `board` keyword in expression context.
            if ns == "supply" {
                return self.board.get(field).copied()
                    .ok_or_else(|| DesignEvalError::EvalError(
                        format!("supply.{field} is not a power pin on the parent")));
            }
            // `self.<param>` resolves the entity's own constructor
            // arguments (spec §5.2 plain `design { }` form). The
            // values live on the instance attribute map under their
            // bare names — same map `intent_<x>` is stamped into,
            // just without the `intent_` prefix. Strings like "5V"
            // are parsed for their numeric component so e.g.
            // `self.v_out` in a body reads 5.0 from `v_out = "5V"`.
            if ns == "self" {
                return self.intent_attrs.get(field)
                    .ok_or_else(|| DesignEvalError::EvalError(
                        format!("self.{field} is not set on this instance \
                                 (constructor arg missing?)")))
                    .and_then(|s| parse_literal(s).map_err(|_| DesignEvalError::EvalError(
                        format!("self.{field} = '{s}' is not a number"))));
            }
            return Err(DesignEvalError::EvalError(
                format!("unknown namespace '{ns}' in identifier '{name}' \
                         (recognised: intent, self, supply, {})",
                        if self.device_class.is_empty() {
                            "<no device discovered>".to_string()
                        } else {
                            self.device_class.clone()
                        })));
        }
        if let Some(v) = self.locals.get(name).copied() { return Ok(v); }
        if let Some(v) = self.board.get(name).copied() { return Ok(v); }
        Err(DesignEvalError::EvalError(format!("identifier '{name}' is not in scope")))
    }
}

/// Recognised device-family namespaces. Vendors authoring a design
/// recipe reach for one of these; the discovery rule routes the
/// matching expansion child's attributes into a context keyed by the
/// `component_class` string. `tube` is the legacy alias for `triode`
/// preserved for backward compatibility with stdlib triodes shipped
/// before the BJT generalisation.
const DEVICE_NAMESPACES: &[&str] = &["tube", "triode", "bjt"];

/// Cheap predicate over a recipe's source to decide whether device
/// parameters are required at evaluation time. We choose a textual
/// over-approximation because (a) the declarative-path expression
/// language stores statements as raw text and re-parses them in
/// `evaluate_text`, and (b) the body-hook path stores the Rhai script
/// as one big string — in both cases the cheapest reliable signal is
/// substring presence of a known device-namespace identifier. A false
/// positive merely produces an early error when a device *is*
/// expected; a false negative can't happen because there's no other
/// way to access the params.
fn recipe_needs_device(recipe: &DesignRecipe) -> bool {
    let mentions_device = |s: &str| -> bool {
        DEVICE_NAMESPACES.iter().any(|ns| {
            s.contains(&format!("{ns}.")) || s.contains(&format!("{ns} "))
        })
    };
    if let Some(body) = &recipe.body {
        if body.inputs.iter().any(|n| DEVICE_NAMESPACES.contains(&n.as_str())) {
            return true;
        }
        if mentions_device(&body.source) { return true; }
    }
    for stmt in &recipe.statements {
        let exprs: &[&str] = match stmt {
            DesignStatement::Let { expr, .. } => &[expr.as_str()],
            DesignStatement::Require { condition, .. } => &[condition.as_str()],
            DesignStatement::Assign { expr, .. } => &[expr.as_str()],
        };
        if exprs.iter().any(|e| mentions_device(e)) { return true; }
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
    device_class: String,
    device_params: HashMap<String, f64>,
) -> Result<HashMap<String, f64>, DesignEvalError> {
    // Stage 6: device-family parameters come from the actual expansion
    // child the synthesizer identified via the `component_class` discovery
    // rule. We deliberately do NOT silently substitute a default device
    // set when the map is empty — silent defaults hide wiring bugs that
    // produce wrong-looking but plausible answers. If a recipe asks for
    // `<family>.<param>` (declarative) or declares the family as an input
    // (body hook) and the synthesizer didn't find a qualifying device,
    // fail loudly so the vendor / board author sees the configuration
    // error.
    if recipe_needs_device(recipe) && device_params.is_empty() {
        return Err(DesignEvalError::EvalError(format!(
            "design recipe for '{}'.'{}' refers to device parameters \
             (tube/triode/bjt/…) but the synthesizer didn't discover a \
             qualifying device child in the expansion block. Check that \
             the entity's expansion instantiates a device with a \
             recognised `component_class` attribute (\"triode\" or \
             \"bjt\").", recipe.entity_name, recipe.intent_name)));
    }

    // Stage 5 — foreign-language body hook takes precedence. The
    // analyzer guarantees mutual exclusion (body wins; statements are
    // dropped with a warning at extraction time), so this branch is
    // an early return: if the recipe carries a body, the script owns
    // the outputs entirely.
    if recipe.body.is_some() {
        return evaluate_body_hook(recipe, intent_attrs, &device_class, &device_params, &board);
    }

    let mut ctx = DesignContext {
        locals: HashMap::new(),
        intent_attrs,
        device_class,
        device_params,
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

/// Resolution surface for the shared expression engine. The engine
/// (`evaluate_text` / `evaluate_expr`) is identical across block kinds — only
/// identifier resolution differs (a `design { }` block resolves
/// `intent`/`self`/device params; a `stress { }` block resolves the operating
/// point + child values). Implementors provide `lookup`; the engine handles
/// literals, operators, and primitive functions.
pub(crate) trait EvalLookup {
    fn lookup(&self, name: &str) -> Result<f64, DesignEvalError>;
}

impl EvalLookup for DesignContext<'_> {
    fn lookup(&self, name: &str) -> Result<f64, DesignEvalError> {
        // Concrete-type call resolves to the inherent method (Rust prefers
        // inherent over trait), so this is delegation, not recursion.
        DesignContext::lookup(self, name)
    }
}

/// Parse `text` as a BHDL expression and evaluate it against `ctx`.
pub(crate) fn evaluate_text<C: EvalLookup>(text: &str, ctx: &C) -> Result<f64, DesignEvalError> {
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
fn evaluate_expr<C: EvalLookup>(expr: &Expr, ctx: &C) -> Result<f64, DesignEvalError> {
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

/// Relative-tolerance equality (1e-6, absolute floor 1e-15) — `100nF ==
/// 100nF` must hold across parse/format round-trips.
fn approx_eq(a: f64, b: f64) -> bool {
    let scale = a.abs().max(b.abs()).max(1e-15);
    (a - b).abs() <= 1e-6 * scale
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
        // Engineering equality: nominal values compared through snapping /
        // unit round-trips need a relative tolerance, not bit equality.
        SyntaxKind::EQEQ => Ok(if approx_eq(lhs, rhs) { 1.0 } else { 0.0 }),
        SyntaxKind::NEQ  => Ok(if approx_eq(lhs, rhs) { 0.0 } else { 1.0 }),
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
        // sqrt(x) — the RMS forms in vendor stress blocks (capacitor
        // ripple-current shares are √-shaped in every topology)
        "sqrt" => {
            if args.len() != 1 {
                return Err(DesignEvalError::EvalError(
                    format!("sqrt expects 1 arg, got {}", args.len())));
            }
            if args[0] < 0.0 {
                return Err(DesignEvalError::EvalError(
                    format!("sqrt of negative value {}", args[0])));
            }
            Ok(args[0].sqrt())
        }
        // pow(x, y) — vendor sizing laws with non-integer exponents
        // (TPS54560 SLVSBN0C Eq. 7: RT(kΩ) = 101756 / fsw(kHz)^1.008)
        "pow" => {
            if args.len() != 2 {
                return Err(DesignEvalError::EvalError(
                    format!("pow expects 2 args (base, exponent), got {}", args.len())));
            }
            Ok(args[0].powf(args[1]))
        }
        other => Err(DesignEvalError::EvalError(
            format!("unknown primitive function '{other}' — the design \
                     evaluator currently knows plate_current, koren_inverse_vgk, sqrt, pow"))),
    }
}

/// Parse a numeric literal — bare or with a BHDL electrical unit suffix
/// (`100V`, `5mA`, `1MΩ`, `570kHz`, `30mV`, …) — into its SI-base f64.
///
/// The canonical unit table is `bhdl_common::const_value::parse_unit_suffix`
/// (V/mV/µV, A/mA, Ω/kΩ/MΩ/mΩ, F/µF/nF/pF, H/mH/µH/nH, Hz/kHz/MHz/GHz),
/// which normalizes to SI base units the same way the analyzer's
/// type-arg / default parsing does. We split the leading numeric part
/// from the trailing unit and apply that multiplier. The SPICE
/// `model_factory::parse_value` is kept only as a last-resort fallback —
/// it uses SPICE-style suffixes (`k`, `meg`, `u`) and notably mishandles
/// frequency units like `kHz` (returning the bare number), which silently
/// produced a 1000× sizing error before this path was preferred.
fn parse_literal(text: &str) -> Result<f64, String> {
    let t = text.trim();
    if let Ok(n) = t.parse::<f64>() {
        return Ok(n);
    }
    // Leading numeric run (digits, sign, decimal point, exponent), then
    // the unit suffix. A bare scientific literal like `5e-6` is already
    // handled by the direct parse above, so treating `e`/`E` as numeric
    // here only matters for non-bare inputs, none of whose units begin
    // with `e`.
    if let Some(idx) = t.find(|c: char| {
        !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E')
    }) {
        let (num, unit) = t.split_at(idx);
        if let (Ok(n), Some((scale, _ctor))) = (
            num.parse::<f64>(),
            bhdl_common::const_value::parse_unit_suffix(unit.trim()),
        ) {
            return Ok(n * scale);
        }
    }
    bhdl_spice::model_factory::parse_value(t)
        .ok_or_else(|| format!("not a numeric literal: '{t}'"))
}

// ─────────────────────────────────────────────────────────────────────────────
// Stage 5 — foreign-language body hooks (Rune)
// ─────────────────────────────────────────────────────────────────────────────

/// Evaluate a [`DesignRecipe`] whose body is a foreign-language hook.
///
/// The engine is **Rune** (MIT OR Apache-2.0 — it replaced Rhai to
/// clear the last MPL-2.0 crate, smartstring, from the dependency
/// graph). `language = "rune"` is dispatched; `"rhai"` gets a
/// targeted migration error (the port is mechanical: `**` →
/// `.powf()`, `.to_float()` → `as f64`); anything else a clear
/// unknown-language error.
///
/// Execution model: the body is wrapped as a function whose
/// parameters are the variables the old Rhai scope pushed —
/// `<device_class>` (plus the `tube` alias when the device is a
/// triode), `intent`, `supply` — each an object map:
///
/// - `tube`    — the device's parameters (`mu`, `ex`, `kg1`, `kp`,
///   `kvb` for triodes). The host primitives take this map as their
///   first argument.
/// - `intent`  — the parsed intent parameters (`target_gain`,
///   `current`, …). Numeric values where the input parsed as a
///   number; raw strings otherwise.
/// - `supply`  — the parent's power-pin voltages (`VBB`, `VCC`, …).
///
/// The script's last expression must be an object whose keys are the
/// names the expansion interpreter expects (the entity's
/// `outputs { … }` declaration). Each value is coerced to `f64`.
/// Missing keys cause a ScriptFailed error; extra keys are ignored
/// with a one-line warning.
///
/// Sandbox: no imports beyond the default modules, and the whole call
/// runs under a 1M-operation fuel budget (`rune::runtime::budget`) —
/// a runaway vendor script halts instead of hanging the build.
fn evaluate_body_hook(
    recipe: &DesignRecipe,
    intent_attrs: &HashMap<String, String>,
    device_class: &str,
    device_params: &HashMap<String, f64>,
    board: &HashMap<String, f64>,
) -> Result<HashMap<String, f64>, DesignEvalError> {
    use rune::runtime::Object;
    use rune::Value;

    let body = recipe.body.as_ref().ok_or_else(|| {
        DesignEvalError::ScriptFailed(
            "internal: evaluate_body_hook called without a body".to_string())
    })?;

    if body.language == "rhai" {
        return Err(DesignEvalError::ScriptFailed(format!(
            "body language 'rhai' was replaced by 'rune' (same Rust-like syntax; \
             port: `**` → `.powf()`, `.to_float()` → `as f64`) — update the \
             `body rhai r#\"…\"#` block in '{}' to `body rune r#\"…\"#`",
            recipe.entity_name)));
    }
    if body.language != "rune" {
        return Err(DesignEvalError::ScriptFailed(format!(
            "unknown body language '{}' — only 'rune' is supported in this build",
            body.language)));
    }

    // The parameter list mirrors the old Rhai scope pushes, in a fixed
    // order: device_class (if any), the `tube` triode alias, intent,
    // supply. The script references whichever names it uses.
    let mut params: Vec<&str> = Vec::new();
    if !device_class.is_empty() {
        params.push(device_class);
    }
    if device_class == "triode" {
        params.push("tube");
    }
    params.push("intent");
    params.push("supply");
    let wrapped = format!(
        "pub fn __bhdl_design({}) {{\n{}\n}}\n",
        params.join(", "),
        body.source
    );

    let mut vm = build_rune_vm(&wrapped).map_err(|e| {
        DesignEvalError::ScriptFailed(format!(
            "vendor design recipe '{}'.'{}' — Rune compile failed:\n{}\n\
             (line numbers are relative to the wrapped script; the body \
             starts on line 2)",
            recipe.entity_name, recipe.intent_name, e))
    })?;

    // Marshal the argument values in the same order as `params`.
    let mut args: Vec<Value> = Vec::new();
    if !device_class.is_empty() {
        args.push(object_value(hashmap_to_rune_object(device_params)?)?);
    }
    if device_class == "triode" {
        args.push(object_value(hashmap_to_rune_object(device_params)?)?);
    }
    args.push(object_value(intent_attrs_to_rune_object(intent_attrs)?)?);
    args.push(object_value(hashmap_to_rune_object(board)?)?);

    // Fuel-limited execution: 1M VM operations, the same budget the
    // Rhai engine enforced via set_max_operations.
    let result: Value = rune::runtime::budget::with(1_000_000, || {
        vm.call(["__bhdl_design"], args)
    })
    .call()
    .map_err(|e| DesignEvalError::ScriptFailed(format!(
        "vendor design recipe '{}'.'{}' — Rune eval failed:\n    {}",
        recipe.entity_name, recipe.intent_name, e)))?;

    // The script's return must be an object. Coerce each declared
    // output to f64. Honour the entity's `outputs { … }` list when
    // present — if the script omits a declared output, fail loudly;
    // if it returns extras, warn but ignore.
    let map: Object = rune::from_value(result).map_err(|_| {
        DesignEvalError::ScriptFailed(format!(
            "script for '{}'.'{}' did not return an object — got something else",
            recipe.entity_name, recipe.intent_name))
    })?;

    let mut out = HashMap::with_capacity(body.outputs.len().max(map.len()));
    if body.outputs.is_empty() {
        for (k, v) in map.iter() {
            let f = rune_value_to_f64(v).ok_or_else(|| {
                DesignEvalError::ScriptFailed(format!(
                    "script for '{}'.'{}' returned non-numeric value at key '{k}'",
                    recipe.entity_name, recipe.intent_name))
            })?;
            out.insert(k.to_string(), f);
        }
    } else {
        let mut extra_keys = Vec::new();
        for (k, _) in map.iter() {
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
            let f = rune_value_to_f64(v).ok_or_else(|| {
                DesignEvalError::ScriptFailed(format!(
                    "script for '{}'.'{}' produced non-numeric value for '{name}'",
                    recipe.entity_name, recipe.intent_name))
            })?;
            out.insert(name.clone(), f);
        }
    }
    Ok(out)
}

/// Wrap an [`Object`] as a [`rune::Value`].
fn object_value(o: rune::runtime::Object) -> Result<rune::Value, DesignEvalError> {
    rune::to_value(o).map_err(|e| DesignEvalError::ScriptFailed(format!(
        "internal: object marshalling failed: {e}")))
}

/// Compile a wrapped script into a [`rune::Vm`] with the BHDL host
/// functions registered. Compile diagnostics are rendered (plain, no
/// color) with the offending source lines and carets — vendors
/// authoring a `body rune r#"…"#` block see *which* line went wrong.
fn build_rune_vm(wrapped_source: &str) -> Result<rune::Vm, String> {
    use rune::runtime::Object;
    use rune::{Context, Diagnostics, Module, Source, Sources, Value, Vm};
    use std::sync::Arc;

    let mut module = Module::new();

    // Host function: plate_current(tube, v_pk, v_gk) -> f64
    // Vendors pass the `tube` map they got as a parameter; the host
    // unpacks the five Koren parameters and dispatches to bhdl-spice.
    module
        .function("plate_current", |tube: Value, v_pk: f64, v_gk: f64| -> f64 {
            let p = tube_params_from_value(&tube);
            bhdl_spice::triode::plate_current(&p, v_pk, v_gk)
        })
        .build()
        .map_err(|e| e.to_string())?;

    // Host function: koren_inverse_vgk(tube, v_pk, target_ip) -> f64
    module
        .function("koren_inverse_vgk", |tube: Value, v_pk: f64, ip: f64| -> f64 {
            let p = tube_params_from_value(&tube);
            bhdl_spice::tube_bias::koren_inverse_vgk(&p, v_pk, ip)
        })
        .build()
        .map_err(|e| e.to_string())?;

    // Host function: conductances(tube, v_pk, v_gk) -> #{ g_p, g_m }
    module
        .function("conductances", |tube: Value, v_pk: f64, v_gk: f64| -> Object {
            let p = tube_params_from_value(&tube);
            let (g_p, g_m) = bhdl_spice::triode::conductances(&p, v_pk, v_gk);
            let mut m = Object::new();
            let _ = rune::alloc::String::try_from("g_p")
                .and_then(|k| { let v = rune::to_value(g_p).expect("f64 value"); m.insert(k, v).map(|_| ()) });
            let _ = rune::alloc::String::try_from("g_m")
                .and_then(|k| { let v = rune::to_value(g_m).expect("f64 value"); m.insert(k, v).map(|_| ()) });
            m
        })
        .build()
        .map_err(|e| e.to_string())?;

    // Host convenience: small_signal_gain(tube, v_pk, i_p) -> f64
    // The composite the amplifier designer uses repeatedly:
    // g_m · (R_p ∥ r_p) at the operating point set by (v_pk, i_p), with
    // R_p assumed to be v_pk / i_p (a designer holding the plate at v_pk).
    module
        .function("small_signal_gain", |tube: Value, v_pk: f64, i_p: f64| -> f64 {
            let p = tube_params_from_value(&tube);
            let r_p = v_pk / i_p;
            let v_gk = bhdl_spice::tube_bias::koren_inverse_vgk(&p, v_pk, i_p);
            let (g_p, g_m) = bhdl_spice::triode::conductances(&p, v_pk, v_gk);
            g_m / (g_p + 1.0 / r_p)
        })
        .build()
        .map_err(|e| e.to_string())?;

    let mut context = Context::with_default_modules().map_err(|e| e.to_string())?;
    context.install(module).map_err(|e| e.to_string())?;
    let runtime = Arc::new(context.runtime().map_err(|e| e.to_string())?);

    let mut sources = Sources::new();
    sources
        .insert(Source::memory(wrapped_source).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut diagnostics = Diagnostics::new();
    let result = rune::prepare(&mut sources)
        .with_context(&context)
        .with_diagnostics(&mut diagnostics)
        .build();
    if !diagnostics.is_empty() {
        // Render the rustc-style report (lines + carets) into a plain
        // buffer — this is the vendor-facing compile error.
        let mut writer = rune::termcolor::Buffer::no_color();
        let _ = diagnostics.emit(&mut writer, &sources);
        let rendered = String::from_utf8_lossy(writer.as_slice()).into_owned();
        if let Err(e) = result {
            return Err(format!("{rendered}\n{e}"));
        }
        // warnings only — log and continue
        log::warn!("Rune script warnings:\n{rendered}");
        let unit = result.map_err(|e| e.to_string())?;
        return Ok(Vm::new(runtime, Arc::new(unit)));
    }
    let unit = result.map_err(|e| e.to_string())?;
    Ok(Vm::new(runtime, Arc::new(unit)))
}

/// Convert a [`HashMap<String, f64>`] into a Rune object.
fn hashmap_to_rune_object(
    h: &HashMap<String, f64>,
) -> Result<rune::runtime::Object, DesignEvalError> {
    let mut m = rune::runtime::Object::new();
    for (k, v) in h {
        let key = rune::alloc::String::try_from(k.as_str()).map_err(|e| {
            DesignEvalError::ScriptFailed(format!("internal: key alloc: {e}"))
        })?;
        let val = rune::to_value(*v).map_err(|e| {
            DesignEvalError::ScriptFailed(format!("internal: value marshalling: {e}"))
        })?;
        m.insert(key, val).map_err(|e| {
            DesignEvalError::ScriptFailed(format!("internal: object insert: {e}"))
        })?;
    }
    Ok(m)
}

/// Build the `intent` object from the stamped `intent_<param>` attrs.
/// Strips the `intent_` prefix; coerces numeric-looking values to f64,
/// otherwise stores as string so the script can decide what to do.
fn intent_attrs_to_rune_object(
    intent_attrs: &HashMap<String, String>,
) -> Result<rune::runtime::Object, DesignEvalError> {
    let mut m = rune::runtime::Object::new();
    for (k, v) in intent_attrs {
        let name = k.strip_prefix("intent_").unwrap_or(k.as_str());
        let key = rune::alloc::String::try_from(name).map_err(|e| {
            DesignEvalError::ScriptFailed(format!("internal: key alloc: {e}"))
        })?;
        let val = if let Ok(f) = parse_literal(v) {
            rune::to_value(f)
        } else {
            rune::to_value(v.clone())
        }
        .map_err(|e| {
            DesignEvalError::ScriptFailed(format!("internal: value marshalling: {e}"))
        })?;
        m.insert(key, val).map_err(|e| {
            DesignEvalError::ScriptFailed(format!("internal: object insert: {e}"))
        })?;
    }
    Ok(m)
}

/// Read tube parameters back out of the object the script passed to a
/// host primitive. Missing keys default to 0.0 — the script is allowed
/// to override individual params before calling primitives, but
/// typical recipes just thread the `tube` map straight through.
fn tube_params_from_value(v: &rune::Value) -> bhdl_spice::triode::TriodeParams {
    let get = |k: &str| -> f64 {
        v.borrow_ref::<rune::runtime::Object>()
            .ok()
            .and_then(|o| o.get(k).and_then(rune_value_to_f64))
            .unwrap_or(0.0)
    };
    bhdl_spice::triode::TriodeParams {
        mu:  get("mu"),
        ex:  get("ex"),
        kg1: get("kg1"),
        kp:  get("kp"),
        kvb: get("kvb"),
    }
}

/// Coerce a Rune [`Value`] to `f64`. Accepts floats and integers;
/// returns None for any other shape (strings, objects, arrays, …).
fn rune_value_to_f64(v: &rune::Value) -> Option<f64> {
    if let Ok(f) = rune::from_value::<f64>(v.clone()) {
        return Some(f);
    }
    if let Ok(i) = rune::from_value::<i64>(v.clone()) {
        return Some(i as f64);
    }
    None
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
        let mut device_params = HashMap::new();
        let sn6 = bhdl_spice::triode::TriodeParams::sn6_6sn7();
        device_params.insert("mu".into(),  sn6.mu);
        device_params.insert("ex".into(),  sn6.ex);
        device_params.insert("kg1".into(), sn6.kg1);
        device_params.insert("kp".into(),  sn6.kp);
        device_params.insert("kvb".into(), sn6.kvb);
        DesignContext {
            locals: HashMap::new(),
            intent_attrs: intent,
            device_class: "triode".into(),
            device_params,
            board,
        }
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
    fn evaluates_self_lookup() {
        // `self.X` reads constructor args from the instance attribute
        // map by the bare name. Same map intent_<x> uses, different
        // prefix convention.
        let mut attrs = HashMap::new();
        attrs.insert("v_out".to_string(), "5V".to_string());
        attrs.insert("dropout".to_string(), "1.2V".to_string());
        let c = make_ctx(&attrs, HashMap::new());
        assert!((evaluate_text("self.v_out", &c).unwrap() - 5.0).abs() < 1e-12);
        assert!((evaluate_text("self.dropout", &c).unwrap() - 1.2).abs() < 1e-12);
        // Missing arg surfaces a clear error.
        assert!(evaluate_text("self.nonexistent", &c).is_err());
    }

    #[test]
    fn lm317_style_design_block_evaluates() {
        // Reproduce LM317's plain `design { }` body in isolation, using
        // simple two-operand sub-expressions stored in named locals.
        // The full inline form `240.0 * (self.v_out / v_ref - 1.0)`
        // exposes a known issue in `parse_expression`'s handling of
        // chained mixed-precedence binops (TODO: fix in the parser);
        // for now we factor the arithmetic into single-binop steps,
        // which the evaluator handles cleanly.
        //
        // LM317: V_OUT = V_REF * (1 + R1/R2), with V_REF = 1.25 V and
        // R2 = 240 Ω. For V_OUT = 5 V:
        //   delta = V_OUT - V_REF        = 3.75
        //   scale = R2 / V_REF           = 192
        //   R1    = delta * scale        = 720
        let mut attrs = HashMap::new();
        attrs.insert("v_out".to_string(), "5V".to_string());

        let recipe = DesignRecipe {
            entity_name: "LM317".into(),
            intent_name: "<plain>".into(),
            statements: vec![
                DesignStatement::Let { name: "v_ref".into(), expr: "1.25".into() },
                DesignStatement::Require {
                    condition: "self.v_out >= 1.35".into(),
                    message: "LM317 minimum V_OUT is V_REF + headroom (~1.35V)".into(),
                },
                DesignStatement::Let { name: "delta".into(), expr: "self.v_out - v_ref".into() },
                DesignStatement::Let { name: "scale".into(), expr: "240.0 / v_ref".into() },
                DesignStatement::Assign { child_name: "r2_value".into(), expr: "240.0".into() },
                DesignStatement::Assign { child_name: "r1_value".into(), expr: "delta * scale".into() },
            ],
            body: None,
        };

        let out = evaluate_recipe(
            &recipe, &attrs, HashMap::new(), String::new(), HashMap::new(),
        ).expect("LM317 recipe should evaluate");

        assert!((out["r2_value"] - 240.0).abs() < 1e-9, "r2_value = {}", out["r2_value"]);
        assert!((out["r1_value"] - 720.0).abs() < 1e-9, "r1_value = {}", out["r1_value"]);
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
        let out = evaluate_recipe(&recipe, &intent, board, "triode".to_string(), default_device()).expect("recipe eval failed");
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
        let err = evaluate_recipe(&recipe, &intent, board, "triode".to_string(), default_device()).expect_err("expected require failure");
        match err {
            DesignEvalError::RequireFailed(_) => {}
            other => panic!("expected RequireFailed, got {other}"),
        }
    }

    // ─── Stage 5: Rune body-hook evaluator ──────────────────────────────

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
                language: "rune".into(),
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
        let out = evaluate_recipe(&recipe, &intent, board, String::new(), HashMap::new()).expect("body hook eval");
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
                language: "rune".into(),
                inputs: vec!["tube".into()],
                outputs: vec!["i_max".into()],
                source: r#"
                    let i = plate_current(tube, 100.0, 0.0);
                    #{ i_max: i }
                "#.into(),
            }),
        };
        let (intent, board) = ctx();
        let out = evaluate_recipe(&recipe, &intent, board, "triode".to_string(), default_device()).expect("body hook eval");
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
        // is the amplifier first-guess bisection ported to the script engine —
        // log-grid peak find then descending-flank bisection on the
        // target gain. The output (Rp, Rk) is compared against the
        // Rust reference designer that does the same computation.
        use bhdl_common::design::DesignBody;
        let recipe = DesignRecipe {
            entity_name: "Foo".into(),
            intent_name: "amplifier".into(),
            statements: Vec::new(),
            body: Some(DesignBody {
                language: "rune".into(),
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
                        let frac = (k as f64) / 63.0;
                        let ratio = i_hi / i_lo;
                        let i = i_lo * ratio.powf(frac);
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

        let out = evaluate_recipe(&recipe, &intent, board, "triode".to_string(), default_device()).expect("amplifier body hook");
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

        // Both designers do the same bisection, but the script engine's f64 chain
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
        // they reach the script via the `tube` map. A 6SN7 and a
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
                language: "rune".into(),
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

        let out_6sn7 = evaluate_recipe(&recipe, &intent, board.clone(), "triode".to_string(), dev_6sn7).expect("6SN7 eval");
        let out_12au7 = evaluate_recipe(&recipe, &intent, board, "triode".to_string(), dev_12au7).expect("12AU7 eval");

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
        // Deliberately syntactically broken script. The diagnostic must
        // show the offending line and a caret (the rendered Rune
        // compile report).
        use bhdl_common::design::DesignBody;
        let recipe = DesignRecipe {
            entity_name: "Foo".into(),
            intent_name: "amplifier".into(),
            statements: Vec::new(),
            body: Some(DesignBody {
                language: "rune".into(),
                inputs:  vec!["intent".into()],
                outputs: vec!["Rp".into()],
                // The error is on the `for in` line — the loop
                // variable is missing, a hard syntax error.
                source: r#"
let v = 1.0;
let n = 80;
for in 0..n {
    v = v * 2.0;
}
#{ Rp: v }
"#.into(),
            }),
        };
        let mut intent = HashMap::new();
        intent.insert("intent_gain".into(), "14.0".into());
        let board = HashMap::new();

        let err = evaluate_recipe(&recipe, &intent, board, "triode".to_string(), default_device())
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
            msg.contains("for in 0..n"),
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
                language: "rune".into(),
                inputs:  vec!["intent".into()],
                outputs: vec!["Rp".into(), "Rk".into()],
                source: r#" #{ Rp: 1000.0 } "#.into(),
            }),
        };
        let (intent, board) = ctx();
        let err = evaluate_recipe(&recipe, &intent, board, String::new(), HashMap::new()).expect_err("missing output");
        match err {
            DesignEvalError::ScriptFailed(msg) => {
                assert!(msg.contains("Rk"),
                    "expected message to mention missing 'Rk', got: {msg}");
            }
            other => panic!("expected ScriptFailed, got {other:?}"),
        }
    }
}
