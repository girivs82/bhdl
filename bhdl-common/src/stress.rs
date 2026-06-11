//! Device **stress** recipe — vendor-authored `simulation { stress { } }`
//! blocks (docs/spec/Vendor_Simulation_Blocks.md §4).
//!
//! Parallel to [`crate::design::DesignRecipe`]. A `design { }` block says
//! *what values* the support components take; a `stress { }` block says *how
//! this device stresses* those components at the operating point — the
//! analytic ripple/peak-current model that today is hardcoded in
//! `bhdl-synthesizer::signoff`. The analyzer extracts one recipe per entity
//! and stores it on `AnalysisResult::stress_recipes`; the sign-off loop
//! evaluates it for per-child stress overrides, falling back to the hardcoded
//! reference model when an entity declares no block.
//!
//! Like design recipes, expressions are kept as raw source text and re-parsed
//! by the evaluator (extraction stays language-agnostic; the evaluator owns
//! expression semantics).

/// A complete stress recipe for one entity.
#[derive(Debug, Clone)]
pub struct StressRecipe {
    /// Entity this recipe belongs to (e.g. "TPS54302").
    pub entity_name: String,
    /// Block body, in source order. `const`/`require` bindings interleave with
    /// the `<child>.<axis> = <expr>;` stress assignments and are evaluated
    /// top-to-bottom (a `const` is visible to later statements).
    pub statements: Vec<StressStatement>,
}

/// A statement inside a `stress { }` block. Expressions are kept as raw source
/// text — the evaluator parses them when it runs.
#[derive(Debug, Clone)]
pub enum StressStatement {
    /// `const NAME = EXPR;` — immutable local visible in later statements.
    Let { name: String, expr: String },

    /// `require EXPR else "MSG";` — vendor guard. If `EXPR` is falsey at the
    /// operating point the stress model does not apply (the parts keep their
    /// generic DC stress) and `MSG` explains why.
    Require { condition: String, message: String },

    /// `CHILD.AXIS = EXPR;` — set the stress axis `AXIS` (e.g. `i_peak`,
    /// `v_ripple`, `i_rms`) of the expansion child named `CHILD` to `EXPR`.
    Assign { child_name: String, axis: String, expr: String },
}

impl StressRecipe {
    /// Create a new empty recipe for `entity_name`.
    pub fn new(entity_name: String) -> Self {
        Self { entity_name, statements: Vec::new() }
    }

    /// True if the recipe carries any statements.
    pub fn has_statements(&self) -> bool {
        !self.statements.is_empty()
    }
}
