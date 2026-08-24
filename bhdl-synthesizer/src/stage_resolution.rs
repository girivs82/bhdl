//! Requirement → block resolution (docs/spec/Requirements_And_Resolution.md §3).
//!
//! A board states a power stage as a REQUIREMENT:
//!
//! ```text
//! u1: BuckStage(vout=5V, i_max=2A, vin=12V);
//! ```
//!
//! `BuckStage` is a trait (bhdl-stdlib/power/stages.bhdl): contract pins
//! plus the requirement vocabulary. Blocks declare `impl BuckStage for
//! Buck_TPS54331 { const vout = v_out; … }` — the impl body is the
//! requirement → constructor mapping. This pass runs BEFORE the main
//! parse (the same text-level discipline as `supply` desugaring):
//!
//! 1. survey every `impl <Trait> for <Block>` in the library;
//! 2. for each candidate, trial-instantiate: map the requirement onto the
//!    block's parameters and evaluate the block's `design { }` — a failed
//!    `require` IS the validity envelope rejecting it — then check the
//!    block's boundary promises (`output_current`, `vin_min/max`,
//!    `output_noise`, `efficiency`) against the requirement. A promise the
//!    requirement needs and the block does not declare is UNCHECKED and
//!    therefore a rejection, never a pass;
//! 3. bind: the lockfile's previous binding if it still passes (stable),
//!    else the designer's `resolve u1 = Block;` override (hard error if it
//!    fails its gates), else the best survivor — ranked by a declared
//!    `cost_rel` when every survivor has one, otherwise library order,
//!    and the ranking basis is stated;
//! 4. rewrite the requirement instantiation to the bound block with NAMED
//!    constructor args, add the import, and stamp `stage_*` scoped
//!    attributes so the requirement stays live on the instance (ERC032
//!    re-checks the promises on the real, flattened circuit every build).
//!
//! Resolving to nothing is a first-class outcome: the requirement becomes
//! the `Generic*` placeholder with `powertree_rating_required_a` stamped,
//! so ERC032 reports it every run, and the near-misses are printed.
//!
//! The SOURCE FILE is never modified — the requirement stays in it. The
//! bound block shows in the elaborated text; the binding lives in
//! bhdl.lock.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use bhdl_common::library::LockedStage;

use crate::supply_synthesis::{
    collect_bhdl, entity_attrs_txt, entity_declared_kind, find_entity_decl, parse_si_txt,
};

/// The power-tree derating policy (powertree.rs `CURRENT_DERATE`): a
/// regulator runs at ≤ 80 % of nameplate. `i_max` is the load; the
/// rating the block must promise is `i_max / 0.8`. Stamped as
/// `powertree_rating_required_a` so ERC032 applies the same predicate.
const CURRENT_DERATE: f64 = 0.8;

/// A requirement interface as declared by `trait X { pin …; const …; }`.
#[derive(Debug, Clone)]
pub struct StageTrait {
    pub name: String,
    /// Const names in declaration order (positional-arg order).
    pub consts: Vec<String>,
}

/// One `impl <Trait> for <Block> { const req = param; … }`.
#[derive(Debug, Clone)]
pub struct StageImpl {
    pub trait_name: String,
    pub block: String,
    pub file: PathBuf,
    /// The file's text (library file, or the board's own source for a
    /// block defined locally).
    pub text: String,
    /// Defined in the board's own source — no import needed.
    pub local: bool,
    /// requirement const → block constructor parameter.
    pub bindings: Vec<(String, String)>,
}

/// A requirement instantiation found in the source.
#[derive(Debug, Clone)]
pub struct StageRequirement {
    pub instance: String,
    pub trait_name: String,
    /// Requirement params as written (const → value text), in order.
    pub params: Vec<(String, String)>,
    /// Byte span of `Trait(args)` in the source.
    pub span: (usize, usize),
}

/// One surveyed candidate and its verdicts. `gates` = (gate, detail, ok).
#[derive(Debug, Clone)]
pub struct StageCandidate {
    pub block: String,
    pub file: PathBuf,
    /// Defined in the board's own source (no import needed).
    pub local: bool,
    pub gates: Vec<(String, String, bool)>,
    /// Gates that failed because a promise is UNDECLARED (UNCHECKED):
    /// a rejection for automatic binding, but a designer's explicit
    /// `resolve` override may accept them (stated).
    pub unchecked: Vec<String>,
    /// `supply_choosable = false`: a TEMPLATE (no orderable part) — listed,
    /// never auto-bound; only an override with the designer's numbers
    /// commits it.
    pub template: bool,
    /// Declared output_current rating (A) — the no-price tie-break.
    pub rating_a: Option<f64>,
    /// Real catalogue price of the block's silicon (cheapest in-stock MPN
    /// with the part-number prefix, via the supplier provider); None when
    /// the provider/DB is absent or nothing matched — stated, never 0.
    pub ic_price: Option<f64>,
    pub ic_mpn: Option<String>,
    /// The `part_number` of the silicon the block instantiates.
    pub part_number: Option<String>,
    /// Named ctor args the block would be instantiated with.
    pub ctor_args: Vec<(String, String)>,
}

impl StageCandidate {
    pub fn passes(&self) -> bool {
        self.gates.iter().all(|g| g.2) && !self.template
    }
    /// Passes for an explicit override: every DECLARED gate holds;
    /// UNCHECKED promises are tolerated (and stated); templates allowed.
    pub fn passes_override(&self) -> bool {
        self.gates.iter().all(|g| g.2 || g.0 == "template" || self.unchecked.contains(&g.0))
    }
    pub fn failures(&self) -> Vec<String> {
        self.gates
            .iter()
            .filter(|g| !g.2)
            .map(|g| format!("{}: {}", g.0, g.1))
            .collect()
    }
}

/// The outcome for one requirement.
#[derive(Debug, Clone)]
pub struct StageResolution {
    pub board: String,
    pub instance: String,
    pub trait_name: String,
    pub requirement: String,
    pub bound: Option<String>,
    /// "lock" | "override" | "survey" | "unresolved"
    pub basis: String,
    /// Human note: ranking basis, lock re-resolution, etc.
    pub notes: Vec<String>,
    pub candidates: Vec<StageCandidate>,
    /// The text the requirement compiled to.
    pub generated: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedSource {
    pub source: String,
    pub resolutions: Vec<StageResolution>,
}

/// Fast gate so ordinary boards pay nothing.
pub fn source_has_stage_requirement(source: &str) -> bool {
    source.contains("Stage(") || source.contains("resolve ")
}

/// Resolve every requirement instantiation in `source`. `locked` is the
/// lockfile's previous bindings. Returns `None` when the source holds no
/// requirement.
pub fn resolve_stages(
    source: &str,
    stdlib_root: &Path,
    locked: &[LockedStage],
) -> Result<Option<ResolvedSource>> {
    if !source_has_stage_requirement(source) {
        return Ok(None);
    }
    let mut files = Vec::new();
    collect_bhdl(stdlib_root, &mut files);
    files.sort();
    let lib: Vec<(PathBuf, String)> = files
        .into_iter()
        .filter_map(|p| std::fs::read_to_string(&p).ok().map(|t| (p, t)))
        .collect();

    // Traits: library + the source itself.
    let mut traits: HashMap<String, StageTrait> = HashMap::new();
    for (_, text) in &lib {
        for t in scan_traits(text) {
            traits.insert(t.name.clone(), t);
        }
    }
    for t in scan_traits(source) {
        traits.insert(t.name.clone(), t);
    }
    if traits.is_empty() {
        return Ok(None);
    }

    let masked = mask_comments(source);
    let board_name = scan_board_name(&masked).unwrap_or_default();
    let project = scan_project_requirements(&masked);
    let mut reqs = scan_requirements(&masked, &traits);
    // PROJECT-WIDE FILTERS: `requirements { qual: …; asil: …; temp_*: … }`
    // apply to every stage on the board; a key the stage states itself
    // wins. Merged BEFORE evaluation so the stamped text, the lock and
    // ERC032 all see one requirement.
    if !project.is_empty() {
        for r in reqs.iter_mut() {
            for (k, v) in &project {
                if !r.params.iter().any(|(rk, _)| rk == k) {
                    r.params.push((k.clone(), v.clone()));
                }
            }
        }
    }
    let overrides = scan_overrides(&masked);
    if reqs.is_empty() && overrides.is_empty() {
        return Ok(None);
    }
    for (inst, _, _) in &overrides {
        if !reqs.iter().any(|r| &r.instance == inst) {
            bail!("`resolve {inst} = …;` names no requirement instantiation '{inst}' in this file");
        }
    }

    let mut impls: Vec<StageImpl> = Vec::new();
    for (path, text) in &lib {
        impls.extend(scan_impls(text, path, false));
    }
    // blocks + impls defined in the board's own file (a designer's local
    // block is a legitimate candidate / override target)
    impls.extend(scan_impls(source, Path::new("<source>"), true));

    let mut resolutions = Vec::new();
    let mut edits: Vec<(usize, usize, String)> = Vec::new(); // (start, end, replacement)
    let mut imports_needed: BTreeMap<String, String> = BTreeMap::new(); // block → file rel path

    for req in &reqs {
        let trait_def = &traits[&req.trait_name];
        // (value quotes dropped: the text is itself stamped inside a quoted
        // attribute; the acceptance parser trims quotes either way)
        let req_text = req
            .params
            .iter()
            .map(|(k, v)| format!("{k}={}", v.trim().trim_matches('"')))
            .collect::<Vec<_>>()
            .join(", ");
        let override_block = overrides
            .iter()
            .find(|(i, _, _)| i == &req.instance)
            .map(|(_, b, _)| b.clone());
        let override_args: Vec<(String, String)> = overrides
            .iter()
            .find(|(i, _, _)| i == &req.instance)
            .map(|(_, _, a)| a.clone())
            .unwrap_or_default();
        let lock_block = locked
            .iter()
            .find(|l| l.board == board_name && l.instance == req.instance && l.trait_name == req.trait_name)
            .map(|l| (l.block.clone(), l.requirement.clone()));

        // the override's own args (the designer's power stage / datasheet
        // axes) feed THAT candidate's evaluation — its promises are read
        // through them (`output_current = fet_id_max`)
        let mut candidates: Vec<StageCandidate> = impls
            .iter()
            .filter(|i| i.trait_name == req.trait_name)
            .map(|i| {
                let extra: &[(String, String)] = if override_block.as_deref() == Some(i.block.as_str()) { &override_args } else { &[] };
                evaluate_candidate(i, req, trait_def, extra)
            })
            .collect();
        // Library order is deterministic (sorted paths, then impl order).
        let mut notes = Vec::new();
        let n_pass = candidates.iter().filter(|c| c.passes()).count();

        let (bound, basis) = if let Some(ob) = override_block {
            match candidates.iter().find(|c| c.block == ob) {
                None => bail!(
                    "resolve {} = {ob}: no `impl {} for {ob}` in the library",
                    req.instance, req.trait_name
                ),
                Some(c) if !c.passes_override() => bail!(
                    "resolve {} = {ob}: the block does not meet the requirement ({req_text}):\n  {}",
                    req.instance,
                    c.failures().join("\n  ")
                ),
                Some(c) => {
                    if c.template {
                        notes.push(format!("{ob} is a TEMPLATE (supply_choosable = false): committed by designer override; the override's args are its power stage"));
                    }
                    if !c.unchecked.is_empty() {
                        notes.push(format!(
                            "override accepted with UNCHECKED promise(s) — {} — the designer's decision, stated",
                            c.gates.iter().filter(|g| c.unchecked.contains(&g.0)).map(|g| format!("{}: {}", g.0, g.1)).collect::<Vec<_>>().join("; ")
                        ));
                    }
                    (Some(ob), "override".to_string())
                }
            }
        } else if let Some((lb, lreq)) = lock_block.clone().filter(|(lb, _)| {
            candidates.iter().any(|c| &c.block == lb && c.passes())
        }) {
            if lreq != req_text {
                notes.push(format!(
                    "requirement changed since lock ({lreq} → {req_text}); locked block {lb} still meets it — kept"
                ));
            }
            (Some(lb), "lock".to_string())
        } else {
            if let Some((lb, _)) = &lock_block {
                let why = candidates
                    .iter()
                    .find(|c| &c.block == lb)
                    .map(|c| c.failures().join("; "))
                    .unwrap_or_else(|| "no longer implements the interface".into());
                notes.push(format!("locked binding {lb} no longer meets the requirement ({why}) — re-resolved"));
            }
            // Price the survivors' silicon through the supplier provider
            // (real catalogue money, the same path the `supply` chooser
            // uses). Only survivors are priced — a rejected block's price
            // is irrelevant and the provider call is not free.
            for c in candidates.iter_mut().filter(|c| c.passes()) {
                if let Some(part) = &c.part_number {
                    if let Some((price, mpn, _sku)) = crate::supply_synthesis::price_via_provider(part, &format!("{}V", req_value(req, "vout").unwrap_or_default().trim_end_matches('V'))) {
                        c.ic_price = price;
                        c.ic_mpn = mpn;
                    }
                }
            }
            let passing: Vec<&StageCandidate> = candidates.iter().filter(|c| c.passes()).collect();
            if passing.is_empty() {
                (None, "unresolved".to_string())
            } else if passing.iter().all(|c| c.ic_price.is_some()) {
                // Every survivor priced → cheapest silicon wins. Support
                // parts are NOT priced here (stated) — the block's children
                // are sized per instantiation and priced at BOM time.
                let best = passing
                    .iter()
                    .min_by(|a, b| a.ic_price.partial_cmp(&b.ic_price).unwrap())
                    .unwrap();
                notes.push(format!(
                    "ranked by catalogue price of the silicon over {} survivor(s): {}; support parts not priced here (BOM-time)",
                    passing.len(),
                    passing
                        .iter()
                        .map(|c| format!("{} {} ${:.4}", c.block, c.ic_mpn.clone().unwrap_or_default(), c.ic_price.unwrap()))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                (Some(best.block.clone()), "survey".to_string())
            } else {
                // Not every survivor priced (provider/DB absent or no
                // catalogue match): NO price ranking — rank by LEAST
                // OVER-RATING (smallest declared output_current that still
                // covers the load; an engineering tie-break, stated).
                // Ties fall to library order.
                let missing = passing.iter().filter(|c| c.ic_price.is_none()).count();
                let rating = |c: &StageCandidate| c.rating_a.unwrap_or(f64::INFINITY);
                let best = passing
                    .iter()
                    .min_by(|a, b| rating(a).partial_cmp(&rating(b)).unwrap())
                    .unwrap();
                notes.push(format!(
                    "{} survivor(s); {missing} unpriced (provider/DB absent or no catalogue match) — NO price ranking; chose the least over-rated block ({} = {}), ties by library order",
                    passing.len(),
                    best.block,
                    best.rating_a.map(|r| format!("{r:.2}A")).unwrap_or_else(|| "rating undeclared".into())
                ));
                (Some(best.block.clone()), "survey".to_string())
            }
        };

        // Stamp: the requirement stays live on the instance.
        let i_max = req_value(req, "i_max").and_then(|v| parse_si_txt(&v));
        let required_rating = i_max.map(|i| i / CURRENT_DERATE);
        let mut stamp = String::new();
        stamp.push_str(&format!(
            "\n    attribute {0}.stage_trait = \"{1}\"; attribute {0}.stage_requirement = \"{2}\";",
            req.instance, req.trait_name, req_text
        ));
        // The power-tree emitter already stamps powertree_* on its
        // stages; never double-stamp a requirement it wrote.
        let tree_stamped = masked.contains(&format!("attribute {}.powertree_rating_required_a", req.instance));
        if let (Some(rr), false) = (required_rating, tree_stamped) {
            stamp.push_str(&format!(
                " attribute {}.powertree_rating_required_a = \"{:.4}\";",
                req.instance, rr
            ));
        }
        if let (Some(n), false) = (req_value(req, "noise").and_then(|v| parse_si_txt(&v)), tree_stamped) {
            stamp.push_str(&format!(
                " attribute {}.powertree_noise_assumed_uvrms = \"{:.2}\";",
                req.instance,
                n * 1e6
            ));
        }

        let generated = match &bound {
            Some(b) => {
                let c = candidates.iter().find(|c| &c.block == b).unwrap();
                stamp.push_str(&format!(
                    " attribute {}.stage_bound = \"{b}\"; attribute {}.stage_binding = \"{basis}\";",
                    req.instance, req.instance
                ));
                if !c.local {
                    let rel = rel_stdlib_path(&c.file, stdlib_root);
                    imports_needed.insert(b.clone(), rel);
                }
                // requirement-mapped args, then the override's own args
                // (the designer's power stage / datasheet axes) — the
                // override wins on a clash
                let mut merged: Vec<(String, String)> = c.ctor_args.clone();
                for (k, v) in &override_args {
                    if let Some(slot) = merged.iter_mut().find(|(mk, _)| mk == k) {
                        slot.1 = v.clone();
                    } else {
                        merged.push((k.clone(), v.clone()));
                    }
                }
                let args = merged
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{b}({args})")
            }
            None => {
                // Placeholder: honest Generic* so ERC032 keeps saying so.
                let generic = match req.trait_name.as_str() {
                    "LdoStage" => "GenericLdo",
                    "BuckExtStage" => "GenericBuckExt",
                    "PreregStage" => "GenericPrereg",
                    _ => "GenericBuck",
                };
                imports_needed.insert(generic.to_string(), "bhdl-stdlib/power/generic_regulators.bhdl".into());
                stamp.push_str(&format!(
                    " attribute {}.stage_bound = \"\"; attribute {}.stage_binding = \"unresolved\";",
                    req.instance, req.instance
                ));
                let vin = req_value(req, "vin").unwrap_or_else(|| "0V".into());
                let vout = req_value(req, "vout").unwrap_or_else(|| "0V".into());
                let rated = req_value(req, "i_max").unwrap_or_else(|| "0A".into());
                format!("{generic}(vin={vin}, vout={vout}, rated={rated})")
            }
        };

        edits.push((req.span.0, req.span.1, generated.clone()));
        // the stamp goes after the statement's terminating `;`
        let stmt_end = masked[req.span.1..]
            .find(';')
            .map(|p| req.span.1 + p + 1)
            .ok_or_else(|| anyhow!("requirement '{}' has no terminating `;`", req.instance))?;
        edits.push((stmt_end, stmt_end, stamp));

        resolutions.push(StageResolution {
            board: board_name.clone(),
            instance: req.instance.clone(),
            trait_name: req.trait_name.clone(),
            requirement: req_text,
            bound,
            basis,
            notes,
            candidates,
            generated,
        });
        let _ = n_pass;
    }

    // Consume the override statements (they are resolver input, not board
    // structure) — replaced by a comment so line numbers hold.
    // Removed outright — NOT replaced by a `//` comment, which would
    // swallow any statement sharing the line (`resolve …; u1.VOUT -> @V5;`).
    for (start, end, _, _, _) in scan_override_spans(&masked) {
        edits.push((start, end, String::new()));
    }

    // Apply edits back-to-front.
    edits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    let mut out = source.to_string();
    for (s, e, rep) in edits {
        out.replace_range(s..e, &rep);
    }
    // Imports for the bound blocks / placeholders that the file lacks.
    let mut import_lines = String::new();
    for (block, rel) in &imports_needed {
        let already = out
            .lines()
            .any(|l| l.trim_start().starts_with("import") && l.contains(&format!(" {block} ")) || l.contains(&format!("{{ {block} }}")) || l.contains(&format!("{{ {block},")) || l.contains(&format!(", {block} }}")) || l.contains(&format!(", {block},")));
        if !already {
            import_lines.push_str(&format!("import {{ {block} }} from \"{rel}\";\n"));
        }
    }
    if !import_lines.is_empty() {
        // after the last import line, else at the top
        let mut insert_at = 0usize;
        let mut off = 0usize;
        for line in out.split_inclusive('\n') {
            if line.trim_start().starts_with("import ") {
                insert_at = off + line.len();
            }
            off += line.len();
        }
        out.insert_str(insert_at, &import_lines);
    }

    Ok(Some(ResolvedSource { source: out, resolutions }))
}

fn req_value(req: &StageRequirement, key: &str) -> Option<String> {
    req.params.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}

fn rel_stdlib_path(file: &Path, stdlib_root: &Path) -> String {
    let rel = file.strip_prefix(stdlib_root).unwrap_or(file);
    format!("bhdl-stdlib/{}", rel.display()).replace('\\', "/")
}

/// The `self.*` namespace a block's `design { }` sees: constructor
/// params (defaults overridden by `params`) PLUS the entity's attributes
/// resolved through them (`attribute feedback_voltage = v_ref` → the
/// v_ref value; literals as written) — what synthesis stamps on an
/// instance.
fn self_namespace(entity_text: &str, block: &str, params: &[(String, String)]) -> HashMap<String, String> {
    let mut all: HashMap<String, String> = entity_params_txt(entity_text, block)
        .into_iter()
        .filter_map(|(n, d)| d.map(|d| (n, d)))
        .collect();
    for (k, v) in params {
        if all.contains_key(k) {
            all.insert(k.clone(), v.clone());
        }
    }
    for (k, v) in entity_attrs_txt(entity_text, block) {
        if all.contains_key(&k) {
            continue; // a param of the same name wins
        }
        let resolved = all.get(v.trim()).cloned().unwrap_or(v);
        all.insert(k, resolved);
    }
    all
}

/// Trial-evaluate a block's validity envelope (its plain `design { }`)
/// at an operating point: `params` override the block's constructor
/// defaults. `None` = the entity declares no design block (nothing to
/// check — the caller states UNCHECKED); `Some(Err(msg))` = the envelope
/// rejected it (a failed `require`, or a recipe error). Shared by the
/// requirement resolver and the `supply` chooser so both apply ONE
/// predicate.
pub fn trial_envelope(entity_text: &str, block: &str, params: &[(String, String)]) -> Option<Result<(), String>> {
    let recipe = design_recipe_for(entity_text, block)?;
    let all = self_namespace(entity_text, block, params);
    Some(
        crate::design_evaluator::evaluate_recipe(&recipe, &all, HashMap::new(), String::new(), HashMap::new())
            .map(|_| ())
            .map_err(|e| match e {
                crate::design_evaluator::DesignEvalError::RequireFailed(m) => m,
                other => format!("design recipe error: {other}"),
            }),
    )
}

/// Trial-instantiate one block against the requirement.
fn evaluate_candidate(imp: &StageImpl, req: &StageRequirement, trait_def: &StageTrait, extra_args: &[(String, String)]) -> StageCandidate {
    let text = imp.text.clone();
    let mut gates: Vec<(String, String, bool)> = Vec::new();
    let mut unchecked: Vec<String> = Vec::new();
    fn push_gate(gates: &mut Vec<(String, String, bool)>, g: &str, d: String, ok: bool) {
        gates.push((g.to_string(), d, ok));
    }
    // an UNDECLARED promise: recorded as a failed gate AND as unchecked
    fn push_unchecked(gates: &mut Vec<(String, String, bool)>, unchecked: &mut Vec<String>, g: &str, d: String) {
        gates.push((g.to_string(), d, false));
        unchecked.push(g.to_string());
    }

    let kind = entity_declared_kind(&text, &imp.block);
    // An `as design` block, or a migration-era conflated entity that
    // still carries its application circuit as `expansion { }` (the
    // generic BuckController template): both materialise the stage.
    let migration_era = kind.is_none() && {
        find_entity_decl(&text, &imp.block)
            .map(|at| {
                let tail = &text[at..];
                let end = tail[1..].find("\nentity ").map(|p| p + 1).unwrap_or(tail.len());
                tail[..end].contains("expansion {")
            })
            .unwrap_or(false)
    };
    push_gate(&mut gates, 
        "block",
        match &kind {
            Some(k) if k == "design" => "`as design` block".into(),
            Some(k) => format!("declared `as {k}` — only `as design` blocks satisfy a stage interface"),
            None if migration_era => "migration-era entity with its own `expansion { }` (materialises the stage)".into(),
            None => "no partness declared and no expansion — cannot materialise a stage".into(),
        },
        kind.as_deref() == Some("design") || migration_era,
    );

    // requirement → ctor args through the impl bindings
    let mut ctor_args: Vec<(String, String)> = Vec::new();
    for (k, v) in &req.params {
        match imp.bindings.iter().find(|(c, _)| c == k) {
            Some((_, p)) => ctor_args.push((p.clone(), v.clone())),
            None => {
                // vin_min/vin_max/noise/efficiency_min are promise-checked,
                // not conveyed; vout/i_max/vin MUST be conveyed.
                if matches!(k.as_str(), "vout" | "i_max" | "vin") {
                    push_gate(&mut gates, "impl", format!("impl binds no block parameter for requirement `{k}`"), false);
                }
            }
        }
    }
    let _ = trait_def;

    // Block parameters: defaults, overridden by the conveyed requirement
    // and the override's extra args, plus the entity's attributes (the
    // `self.*` namespace).
    let mut eval_args = ctor_args.clone();
    for (k, v) in extra_args {
        if let Some(slot) = eval_args.iter_mut().find(|(mk, _)| mk == k) {
            slot.1 = v.clone();
        } else {
            eval_args.push((k.clone(), v.clone()));
        }
    }
    let params: HashMap<String, String> = self_namespace(&text, &imp.block, &eval_args);

    // Envelope: evaluate the block's design { } with these params.
    match design_recipe_for(&text, &imp.block) {
        Some(recipe) => {
            match crate::design_evaluator::evaluate_recipe(
                &recipe, &params, HashMap::new(), String::new(), HashMap::new(),
            ) {
                Ok(_) => push_gate(&mut gates, "envelope", "design { } accepts the operating point".into(), true),
                Err(crate::design_evaluator::DesignEvalError::RequireFailed(m)) => {
                    push_gate(&mut gates, "envelope", m, false)
                }
                Err(e) => push_gate(&mut gates, "envelope", format!("design recipe error: {e}"), false),
            }
        }
        None => push_unchecked(&mut gates, &mut unchecked, "envelope", "block declares no design { } — no envelope to check (UNCHECKED, not a pass)".into()),
    }

    let attrs = entity_attrs_txt(&text, &imp.block);
    let attr_si = |k: &str| -> Option<f64> {
        let v = attrs.get(k)?;
        parse_si_txt(v).or_else(|| params.get(v.trim()).and_then(|d| parse_si_txt(d)))
    };

    // Promises — THE SAME PREDICATE ERC032 applies after binding
    // (stage_acceptance::check). Here the promises are read from the
    // block's entity text with attribute values resolved through its
    // params (`attribute f_sw = f_sw`); ERC032 reads the instance's
    // resolved attributes on the flattened circuit.
    {
        use crate::stage_acceptance::{check, promises_from_attrs, requirement_from_pairs, GateVerdict};
        let mut sreq = requirement_from_pairs(req.params.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        if req.trait_name == "BuckExtStage" && sreq.phases.is_none() {
            sreq.phases = Some(1.0); // a controller stage must state its phase support
        }
        let promises = promises_from_attrs(|k| attrs.get(k).map(|v| params.get(v.trim()).cloned().unwrap_or_else(|| v.clone())));
        // the stage's dissipation at the requirement's operating point —
        // what an ambient temperature requirement is checked THERMALLY
        // against when the part is junction-rated (θ_JA + T_J,max)
        {
            let class = attrs.get("component_class").map(|c| c.trim_matches('"').to_string()).unwrap_or_default();
            let si = |k: &str| attr_si(k);
            sreq.p_diss_w = crate::stage_acceptance::estimate_dissipation_w(
                &class,
                req_value(req, "vin").and_then(|v| parse_si_txt(&v)),
                req_value(req, "vout").and_then(|v| parse_si_txt(&v)),
                sreq.i_max_a,
                si("i_quiescent"),
                promises.efficiency,
                si("rds_on"),
                si("f_sw").or_else(|| si("switching_frequency")),
                si("t_sw"),
            );
        }
        for g in check(&sreq, &promises) {
            let name: &'static str = g.name;
            match g.verdict {
                GateVerdict::Ok => push_gate(&mut gates, name, g.detail, true),
                GateVerdict::Failed => push_gate(&mut gates, name, g.detail, false),
                GateVerdict::Unchecked => push_unchecked(&mut gates, &mut unchecked, name, format!("block {}", g.detail)),
            }
        }
    }

    let template = attrs.get("supply_choosable").map(|v| v.trim() == "false").unwrap_or(false);
    if template {
        push_gate(&mut gates, 
            "template",
            "supply_choosable = false: a TEMPLATE with no orderable part — never auto-bound; commit it with `resolve <inst> = Block(<your datasheet/power-stage args>);`".into(),
            false,
        );
    }

    let rating_a = attr_si("output_current");
    // The silicon the block instantiates (`u: <Part>(…)` in its body) and
    // that part's `part_number` — what the provider prices.
    let part_number = block_part_instance(&text, &imp.block)
        .and_then(|part| entity_attrs_txt(&text, &part).get("part_number").cloned())
        .filter(|p| !p.is_empty());
    StageCandidate {
        block: imp.block.clone(),
        file: imp.file.clone(),
        local: imp.local,
        gates,
        unchecked,
        template,
        rating_a,
        ic_price: None,
        ic_mpn: None,
        part_number,
        ctor_args,
    }
}


/// The part a block instantiates: the first `<name>: <Entity>(` body
/// statement whose entity is declared `as part` in the same file (or
/// the conventional `u:` child).
fn block_part_instance(text: &str, block: &str) -> Option<String> {
    let at = find_entity_decl(text, block)?;
    let tail = &text[at..];
    let end = tail[1..].find("\nentity ").map(|p| p + 1).unwrap_or(tail.len());
    let body = mask_comments(&tail[..end]);
    let mut fallback = None;
    for line in body.lines() {
        let l = line.trim();
        let Some((name, rest)) = l.split_once(':') else { continue };
        let name = name.trim();
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') { continue; }
        let ent: String = rest.trim().chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if ent.is_empty() || !rest.trim()[ent.len()..].starts_with('(') { continue; }
        if entity_declared_kind(text, &ent).as_deref() == Some("part") {
            return Some(ent);
        }
        if name == "u" { fallback = Some(ent); }
    }
    fallback
}


/// The block's plain `design { }` recipe, extracted through the analyzer
/// so the resolver evaluates EXACTLY what synthesis will.
fn design_recipe_for(text: &str, block: &str) -> Option<bhdl_common::design::DesignRecipe> {
    use rowan::ast::AstNode;
    let pr = bhdl_parser::parse(text);
    let sf = bhdl_ast::SourceFile::cast(pr.syntax())?;
    let all = bhdl_analyzer::extract_design_recipes(&sf);
    all.get(block)?.get("<plain>").cloned()
}

/// `entity X(a: t = d, b: t, …)` → [(a, Some("d")), (b, None)].
fn entity_params_txt(src: &str, name: &str) -> Vec<(String, Option<String>)> {
    let Some(at) = find_entity_decl(src, name) else { return Vec::new() };
    let after = &src[at..];
    let head_end = after.find('{').unwrap_or(after.len());
    let head = &after[..head_end];
    let Some(open) = head.find('(') else { return Vec::new() };
    // strip comments inside the header
    let mut clean = String::new();
    for line in head[open + 1..].lines() {
        clean.push_str(line.split("//").next().unwrap_or(""));
        clean.push('\n');
    }
    let mut depth = 0usize;
    let mut chunks = Vec::new();
    let mut cur = String::new();
    for c in clean.chars() {
        match c {
            '(' | '<' => { depth += 1; cur.push(c); }
            ')' if depth == 0 => { chunks.push(cur.clone()); cur.clear(); break; }
            ')' | '>' => { depth = depth.saturating_sub(1); cur.push(c); }
            ',' if depth == 0 => { chunks.push(cur.clone()); cur.clear(); }
            _ => cur.push(c),
        }
    }
    chunks
        .into_iter()
        .filter_map(|ch| {
            let (n, rest) = ch.split_once(':')?;
            let n = n.trim();
            if n.is_empty() { return None; }
            Some((n.to_string(), rest.split_once('=').map(|(_, d)| d.trim().to_string())))
        })
        .collect()
}

/// Replace `// …` comment bodies with spaces (same length) so offsets hold.
fn mask_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.split_inclusive('\n') {
        if let Some(p) = line.find("//") {
            out.push_str(&line[..p]);
            // byte-preserving: a multibyte char becomes that many
            // spaces so byte offsets into `masked` index the source
            for c in line[p..].chars() {
                if c == '\n' {
                    out.push('\n');
                } else {
                    for _ in 0..c.len_utf8() {
                        out.push(' ');
                    }
                }
            }
        } else {
            out.push_str(line);
        }
    }
    out
}

fn scan_traits(text: &str) -> Vec<StageTrait> {
    let masked = mask_comments(text);
    let mut out = Vec::new();
    let mut off = 0usize;
    while let Some(p) = masked[off..].find("trait ") {
        let at = off + p;
        let line_start = masked[..at].rfind('\n').map(|x| x + 1).unwrap_or(0);
        if masked[line_start..at].trim().is_empty() {
            let rest = &masked[at + 6..];
            let name: String = rest.trim_start().chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            if let Some(open) = rest.find('{') {
                if let Some(close) = rest[open..].find('}') {
                    let body = &rest[open + 1..open + close];
                    let consts = body
                        .lines()
                        .filter_map(|l| {
                            let l = l.trim();
                            let r = l.strip_prefix("const ")?;
                            let n: String = r.trim().chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                            if n.is_empty() { None } else { Some(n) }
                        })
                        .collect();
                    if !name.is_empty() {
                        out.push(StageTrait { name, consts });
                    }
                }
            }
        }
        off = at + 6;
    }
    out
}

fn scan_impls(text: &str, file: &Path, local: bool) -> Vec<StageImpl> {
    let masked = mask_comments(text);
    let mut out = Vec::new();
    let mut off = 0usize;
    while let Some(p) = masked[off..].find("impl ") {
        let at = off + p;
        let line_start = masked[..at].rfind('\n').map(|x| x + 1).unwrap_or(0);
        if masked[line_start..at].trim().is_empty() {
            let rest = &masked[at + 5..];
            let trait_name: String = rest.trim_start().chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            let after_trait = rest.trim_start()[trait_name.len()..].trim_start();
            if let Some(r2) = after_trait.strip_prefix("for ") {
                let block: String = r2.trim_start().chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                if let Some(open) = r2.find('{') {
                    if let Some(close) = r2[open..].find('}') {
                        let body = &r2[open + 1..open + close];
                        let bindings = body
                            .split(';')
                            .filter_map(|st| {
                                let st = st.trim().strip_prefix("const ")?;
                                let (c, p) = st.split_once('=')?;
                                Some((c.trim().to_string(), p.trim().to_string()))
                            })
                            .collect();
                        if !trait_name.is_empty() && !block.is_empty() {
                            out.push(StageImpl { trait_name, block, file: file.to_path_buf(), text: text.to_string(), local, bindings });
                        }
                    }
                }
            }
        }
        off = at + 5;
    }
    out
}

/// `name: Trait(args)` anywhere in the (comment-masked) source.
fn scan_requirements(masked: &str, traits: &HashMap<String, StageTrait>) -> Vec<StageRequirement> {
    let mut out = Vec::new();
    for (tname, tdef) in traits {
        let pat = format!("{tname}(");
        let mut off = 0usize;
        while let Some(p) = masked[off..].find(&pat) {
            let at = off + p;
            off = at + pat.len();
            // preceding ident boundary
            if at > 0 && masked[..at].chars().last().map(|c| c.is_alphanumeric() || c == '_').unwrap_or(false) {
                continue;
            }
            // `name :` before it
            let before = masked[..at].trim_end();
            let Some(b2) = before.strip_suffix(':') else { continue };
            let b2 = b2.trim_end();
            let name: String = b2.chars().rev().take_while(|c| c.is_alphanumeric() || *c == '_').collect::<Vec<_>>().into_iter().rev().collect();
            if name.is_empty() { continue; }
            let Some(close) = masked[at + pat.len()..].find(')') else { continue };
            let args = &masked[at + pat.len()..at + pat.len() + close];
            let mut params = Vec::new();
            for (i, a) in args.split(',').map(str::trim).filter(|a| !a.is_empty()).enumerate() {
                if let Some((k, v)) = a.split_once('=') {
                    params.push((k.trim().to_string(), v.trim().to_string()));
                } else if let Some(k) = tdef.consts.get(i) {
                    params.push((k.clone(), a.to_string()));
                }
            }
            out.push(StageRequirement {
                instance: name,
                trait_name: tname.clone(),
                params,
                span: (at, at + pat.len() + close + 1),
            });
        }
    }
    out.sort_by_key(|r| r.span.0);
    out
}

fn scan_override_spans(masked: &str) -> Vec<(usize, usize, String, String, Vec<(String, String)>)> {
    // `resolve <inst> = <Block>;` or `resolve <inst> = <Block>(k=v, …);` —
    // statement-based (several statements may share a line), anchored at
    // a statement boundary. The args are the designer's own constructor
    // args for the committed block (tier B power stage, datasheet axes).
    let mut out = Vec::new();
    let mut off = 0usize;
    while let Some(p) = masked[off..].find("resolve ") {
        let at = off + p;
        off = at + 8;
        let prev = masked[..at].trim_end().chars().last();
        if !matches!(prev, None | Some(';') | Some('{') | Some('}')) {
            continue;
        }
        let Some(semi) = masked[at..].find(';') else { continue };
        let body = &masked[at + 8..at + semi];
        let Some((inst, rhs)) = body.split_once('=') else { continue };
        let (inst, rhs) = (inst.trim(), rhs.trim());
        let (block, args_txt) = match rhs.split_once('(') {
            Some((b, rest)) => (b.trim(), rest.trim_end().strip_suffix(')').unwrap_or(rest).to_string()),
            None => (rhs, String::new()),
        };
        let ok = |t: &str| !t.is_empty() && t.chars().all(|c| c.is_alphanumeric() || c == '_');
        if !(ok(inst) && ok(block)) {
            continue;
        }
        let args: Vec<(String, String)> = args_txt
            .split(',')
            .map(str::trim)
            .filter(|a| !a.is_empty())
            .filter_map(|a| a.split_once('=').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())))
            .collect();
        out.push((at, at + semi + 1, inst.to_string(), block.to_string(), args));
    }
    out
}

/// `requirements { key: value; … }` at board scope (comment-masked text).
fn scan_project_requirements(masked: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut off = 0usize;
    while let Some(p) = masked[off..].find("requirements") {
        let at = off + p;
        off = at + 12;
        let before = masked[..at].chars().last();
        if before.map(|c| c.is_alphanumeric() || c == '_').unwrap_or(false) {
            continue;
        }
        let rest = masked[at + 12..].trim_start();
        if !rest.starts_with('{') {
            continue;
        }
        let Some(close) = rest.find('}') else { continue };
        for entry in rest[1..close].split(';') {
            if let Some((k, v)) = entry.split_once(':') {
                let (k, v) = (k.trim(), v.trim());
                if !k.is_empty() && !v.is_empty() {
                    out.push((k.to_string(), v.to_string()));
                }
            }
        }
    }
    out
}

fn scan_board_name(masked: &str) -> Option<String> {
    masked.lines().find_map(|l| {
        let r = l.trim_start().strip_prefix("board ")?;
        let n: String = r.trim_start().chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        (!n.is_empty()).then_some(n)
    })
}

fn scan_overrides(masked: &str) -> Vec<(String, String, Vec<(String, String)>)> {
    scan_override_spans(masked).into_iter().map(|(_, _, i, b, a)| (i, b, a)).collect()
}

/// Render the resolution report (CLI + tests).
pub fn render_report(r: &StageResolution) -> String {
    let mut s = String::new();
    match &r.bound {
        Some(b) => s.push_str(&format!("{}: {}({}) → {b} [{}]\n", r.instance, r.trait_name, r.requirement, r.basis)),
        None => s.push_str(&format!("{}: {}({}) → UNRESOLVED (placeholder emitted; ERC032 reports it every build)\n", r.instance, r.trait_name, r.requirement)),
    }
    for n in &r.notes {
        s.push_str(&format!("    note: {n}\n"));
    }
    if r.candidates.is_empty() {
        s.push_str(&format!("    no block in the library implements {}\n", r.trait_name));
    }
    for c in &r.candidates {
        let mark = if c.passes() { "✓" } else if c.template { "⧖" } else { "✗" };
        let price = match (&c.ic_price, &c.ic_mpn) {
            (Some(p), Some(m)) => format!("  [{m} ${p:.4}]"),
            _ => String::new(),
        };
        s.push_str(&format!("    {} {}{}{price}\n", mark, c.block, if c.template { " (template)" } else { "" }));
        for g in &c.gates {
            s.push_str(&format!("        {} {}: {}\n", if g.2 { "ok " } else { "NOK" }, g.0, g.1));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_is_byte_preserving() {
        let src = "a // — µ ≤\nb: BuckStage(1V)";
        let m = mask_comments(src);
        assert_eq!(m.len(), src.len());
        assert_eq!(&m[m.find("BuckStage").unwrap()..], &src[src.find("BuckStage").unwrap()..]);
    }

    #[test]
    fn scans_requirements_and_impls() {
        let mut traits = HashMap::new();
        traits.insert("BuckStage".to_string(), StageTrait { name: "BuckStage".into(), consts: vec!["vout".into(), "i_max".into(), "vin".into()] });
        let src = "board B {\n    @VIN -> u1: BuckStage(5V, i_max=2A, vin=12V).VIN; // BuckStage(ignored)\n    resolve u1 = Buck_TPS54331;\n}";
        let m = mask_comments(src);
        let r = scan_requirements(&m, &traits);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].instance, "u1");
        assert_eq!(r[0].params, vec![("vout".to_string(), "5V".to_string()), ("i_max".into(), "2A".into()), ("vin".into(), "12V".into())]);
        assert_eq!(scan_overrides(&m), vec![("u1".to_string(), "Buck_TPS54331".to_string(), vec![])]);
        let m2 = mask_comments("board B {\n    resolve u2 = BuckController(hs_fet=\"BSC0902NS\", fet_rds_on=2.3mΩ);\n}");
        let o = scan_overrides(&m2);
        assert_eq!(o[0].1, "BuckController");
        assert_eq!(o[0].2, vec![("hs_fet".to_string(), "\"BSC0902NS\"".to_string()), ("fet_rds_on".to_string(), "2.3mΩ".to_string())]);
        let imp = scan_impls("impl BuckStage for Buck_X {\n    const vout = v_out; const i_max = i_out_max;\n}\n", Path::new("x.bhdl"), false);
        assert_eq!(imp.len(), 1);
        assert_eq!(imp[0].bindings.len(), 2);
    }
}
