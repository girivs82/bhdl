//! Operating-point **design** recipe — vendor-authored `design { }` blocks.
//!
//! Parallel to [`crate::expansion::ExpansionRecipe`]. An entity's
//! `expansion { }` block describes *what components exist*; its
//! `design for <intent> { }` block describes *what values they should take*
//! under a given intent.
//!
//! The analyzer extracts one recipe per (entity, intent) pair from the AST
//! and stores it on `AnalysisResult::design_recipes`. The synthesizer's
//! expansion interpreter consults that map *before* falling back to its
//! Rust reference designers — if a vendor has supplied a `design { }` block
//! for the matching intent, that block wins.
//!
//! Expressions are stored as raw source text and re-parsed by the
//! evaluator (deliberately: stage 2 extraction stays language-agnostic;
//! stage 3 owns expression semantics).

/// A complete design recipe for one (entity, intent) pair.
#[derive(Debug, Clone)]
pub struct DesignRecipe {
    /// Entity this recipe belongs to (e.g. "SignalTubeStage").
    pub entity_name: String,
    /// Intent this recipe is the designer for (e.g. "amplifier",
    /// "current_source", "digital_switch").
    pub intent_name: String,
    /// Block body, in the order it appears in the source. Mutually
    /// exclusive with [`Self::body`] — the analyzer rejects mixing.
    pub statements: Vec<DesignStatement>,
    /// Foreign-language hook (Stage 5). When present, the evaluator
    /// passes the recipe's inputs to an embedded interpreter (currently
    /// Rhai) and reads the outputs back from the script's return value.
    /// `None` means this is a Stage-1..4 declarative recipe driven by
    /// `statements` instead.
    pub body: Option<DesignBody>,
}

/// A foreign-language hook attached to a [`DesignRecipe`].
///
/// Stage 5 of the vendor design-block spec: vendors who need arbitrary
/// imperative code (iteration, combinatorial search, optimization)
/// embed it as source in the .bhdl file. The script sees the declared
/// `inputs` in its scope and must return a map keyed by the declared
/// `outputs` names. Sandboxed by construction — no I/O, no module
/// imports, fuel-limited execution.
#[derive(Debug, Clone)]
pub struct DesignBody {
    /// Language tag (e.g. "rhai"). The evaluator dispatches by this
    /// string; unknown languages produce a clear error at synth time.
    pub language: String,
    /// Names the script will see in its input scope. The evaluator
    /// marshals these from the standard context (tube/intent/supply)
    /// — this list is currently informational, used by the analyzer
    /// to validate the script's expected I/O.
    pub inputs: Vec<String>,
    /// Names the script must populate in its return value. The
    /// evaluator validates the returned map against this list and
    /// applies the values to the matching expansion children.
    pub outputs: Vec<String>,
    /// Verbatim foreign-language source (already stripped of the
    /// `r#"..."#` raw-string delimiters by the analyzer).
    pub source: String,
}

/// A statement inside a `design { }` block. Expressions are kept as raw
/// source text — the evaluator parses them when it runs.
#[derive(Debug, Clone)]
pub enum DesignStatement {
    /// `const NAME = EXPR;` — immutable binding visible in subsequent
    /// statements of the same block.
    Let { name: String, expr: String },

    /// `require EXPR else "MSG";` — vendor-supplied validation. If
    /// `EXPR` evaluates falsey at design time, the design is rejected
    /// with `MSG`.
    Require { condition: String, message: String },

    /// `CHILD = EXPR;` — override the expansion child named `CHILD`
    /// with the value of `EXPR`.
    Assign { child_name: String, expr: String },
}

impl DesignRecipe {
    /// Create a new empty recipe.
    pub fn new(entity_name: String, intent_name: String) -> Self {
        Self {
            entity_name,
            intent_name,
            statements: Vec::new(),
            body: None,
        }
    }

    /// True if this recipe carries a foreign-language body hook (Stage 5).
    pub fn has_body(&self) -> bool { self.body.is_some() }

    /// True if this recipe is a declarative Stage-1..4 statement sequence.
    pub fn has_statements(&self) -> bool { !self.statements.is_empty() }
}
