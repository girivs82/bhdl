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
    /// Block body, in the order it appears in the source.
    pub statements: Vec<DesignStatement>,
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
        }
    }
}
