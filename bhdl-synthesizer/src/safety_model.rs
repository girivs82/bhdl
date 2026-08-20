//! Functional-safety semantic pass (docs/spec/Functional_Safety.md §3–§4).
//!
//! Input: the parsed source(s) — library `safety_goal` definitions and
//! `safety <Name> [of E] as ns { }` blocks — and the synthesized netlist
//! of the board. Output: `bhdl_common::safety::SafetyModel` with every
//! handle resolved against real instances/nets and the Phase-1 gap list.
//! Nothing here computes a metric; nothing here invents data.
//!
//! Resolution model. The synthesizer flattens an entity instance `rail_a`
//! of a composite entity into children named `rail_a_<child>` carrying
//! `expansion_parent = "rail_a"`. So inside `safety SupervisedReg5V as dut`,
//! applied to instance `rail_a`, `dut.mon` → instance `rail_a_mon`,
//! `dut.mon.nOUT` → that instance's pin, and `dut.VOUT` (an entity pin) →
//! the net attached to `rail_a`'s VOUT port. In a board block
//! `brd.rail_a.mon` resolves through the same table. Entity-level blocks
//! are applied once per instance of the entity (the analysis travels
//! with the entity).

use std::collections::{BTreeMap, HashMap, HashSet};

use bhdl_ast::{AstNode, BhdlLanguage, SourceFile, SyntaxKind};
type SyntaxNode = bhdl_ast::SyntaxNode<BhdlLanguage>;
use bhdl_common::safety::{
    Assumption, AssumptionStatus, Effect, Fault, Gap, GapClass, Goal, Level, Mechanism,
    MechanismKind, Part, PartData, SafetyModel, Scope, Severity, Waiver,
};
use bhdl_netlist::{Netlist, ModuleKind};

/// A library `safety_goal` definition as parsed.
#[derive(Debug, Clone)]
struct GoalDef {
    name: String,
    title: String,
    /// parameter names with optional defaults (raw text)
    params: Vec<(String, Option<String>)>,
    /// formal signals
    formals: Vec<String>,
    /// (effect name, expr text, severity)
    effects: Vec<(String, String, Severity)>,
}

/// One `safety` block as parsed, before resolution.
#[derive(Debug, Clone)]
struct SafetyBlock {
    name: String,
    entity: String,
    ns: String,
    stmts: Vec<SyntaxNode>,
}

// ─────────────────────────── CST helpers ───────────────────────────

/// Direct IDENT tokens of a node, minus the contextual statement heads
/// (`goal`, `effect`, `severity`, `mechanism`, …) which the lexer emits
/// as plain IDENTs.
fn idents(node: &SyntaxNode) -> Vec<String> {
    const HEADS: &[&str] = &[
        "goal", "effect", "severity", "mechanism", "fault", "waive", "assume", "refines",
        "satisfied_by", "waived", "expect", "detected_by", "within", "of", "qm",
    ];
    node.children_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| t.kind() == SyntaxKind::IDENT)
        .map(|t| t.text().to_string())
        .filter(|t| !HEADS.contains(&t.as_str()))
        .collect()
}

fn first_string(node: &SyntaxNode) -> Option<String> {
    node.children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == SyntaxKind::STRING)
        .map(|t| t.text().trim_matches('"').to_string())
}

fn child_nodes(node: &SyntaxNode, kind: SyntaxKind) -> Vec<SyntaxNode> {
    node.children().filter(|c| c.kind() == kind).collect()
}

fn first_child(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxNode> {
    node.children().find(|c| c.kind() == kind)
}

/// Text of a node with trivia collapsed (for expressions / paths).
fn text_of(node: &SyntaxNode) -> String {
    let mut s = String::new();
    for e in node.descendants_with_tokens() {
        if let Some(t) = e.into_token() {
            if !matches!(t.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT) {
                s.push_str(t.text());
            }
        }
    }
    s
}

/// `(a=1, b="x", c)` param list → BTreeMap (positional values keyed "0","1",…).
fn kwargs_of(node: &SyntaxNode) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(list) = node.children().find(|c| {
        c.kind() == SyntaxKind::PARAM_LIST
    }) else {
        return out;
    };
    let mut pos = 0usize;
    for item in list.children() {
        let txt = text_of(&item);
        if let Some((k, v)) = txt.split_once('=') {
            out.insert(k.trim().to_string(), v.trim().trim_matches('"').to_string());
        } else if !txt.is_empty() {
            out.insert(pos.to_string(), txt.trim_matches('"').to_string());
            pos += 1;
        }
    }
    out
}

/// Split `psm(SG_OV, detects=[a, b], dc=0.9, source="x")` into
/// (kind, positional[0], kwargs).
fn parse_mechanism_call(txt: &str) -> Option<(String, String, BTreeMap<String, String>)> {
    let open = txt.find('(')?;
    let kind = txt[..open].trim().to_string();
    let inner = txt[open + 1..].trim_end_matches(')');
    // split on top-level commas
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    let mut in_str = false;
    for ch in inner.chars() {
        match ch {
            '"' => { in_str = !in_str; cur.push(ch); }
            '[' | '(' if !in_str => { depth += 1; cur.push(ch); }
            ']' | ')' if !in_str => { depth -= 1; cur.push(ch); }
            ',' if depth == 0 && !in_str => { parts.push(cur.trim().to_string()); cur.clear(); }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        parts.push(cur.trim().to_string());
    }
    let mut kw = BTreeMap::new();
    let mut first = String::new();
    for (i, p) in parts.iter().enumerate() {
        if let Some((k, v)) = p.split_once('=') {
            kw.insert(k.trim().to_string(), v.trim().to_string());
        } else if i == 0 {
            first = p.clone();
        }
    }
    Some((kind, first, kw))
}

fn list_items(v: &str) -> Vec<String> {
    v.trim().trim_start_matches('[').trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

// ─────────────────────────── collection ───────────────────────────

fn collect_goal_defs(root: &SyntaxNode) -> HashMap<String, GoalDef> {
    let mut out = HashMap::new();
    for n in root.descendants().filter(|n| n.kind() == SyntaxKind::SAFETY_GOAL_DEF) {
        let ids = idents(&n);
        let Some(name) = ids.first().cloned() else { continue };
        let title = first_string(&n).unwrap_or_default();
        let mut params = Vec::new();
        if let Some(p) = first_child(&n, SyntaxKind::SAFETY_GOAL_PARAMS) {
            // entity-parameter shape: `name: type [= default]`, comma-separated
            let txt = text_of(&p);
            let inner = txt.trim_start_matches('(').trim_end_matches(')');
            for item in inner.split(',') {
                let item = item.trim();
                if item.is_empty() { continue; }
                let (lhs, default) = match item.split_once('=') {
                    Some((l, d)) => (l, Some(d.trim().to_string())),
                    None => (item, None),
                };
                let pname = lhs.split(':').next().unwrap_or("").trim().to_string();
                if !pname.is_empty() { params.push((pname, default)); }
            }
        }
        let formals: Vec<String> = child_nodes(&n, SyntaxKind::SAFETY_SIGNAL_DECL)
            .iter()
            .filter_map(|d| idents(d).first().cloned())
            .collect();
        let effects = collect_effects(&n);
        out.insert(name.clone(), GoalDef { name, title, params, formals, effects });
    }
    out
}

fn collect_effects(goal_node: &SyntaxNode) -> Vec<(String, String, Severity)> {
    let mut out = Vec::new();
    for e in child_nodes(goal_node, SyntaxKind::SAFETY_EFFECT) {
        let ids = idents(&e);
        let Some(name) = ids.first().cloned() else { continue };
        let sev = ids.last().and_then(|s| Severity::parse(s)).unwrap_or(Severity::S0);
        // expression = the child node that is an expression (between `=` and `severity`)
        let expr = e
            .children()
            .map(|c| text_of(&c))
            .find(|t| !t.is_empty())
            .unwrap_or_default();
        out.push((name, expr, sev));
    }
    out
}

fn collect_blocks(root: &SyntaxNode) -> Vec<SafetyBlock> {
    let mut out = Vec::new();
    for n in root.descendants().filter(|n| n.kind() == SyntaxKind::SAFETY_DEF) {
        let name = n
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT)
            .map(|t| t.text().to_string())
            .unwrap_or_default();
        let entity = first_child(&n, SyntaxKind::SAFETY_LINK)
            .and_then(|l| idents(&l).first().cloned())
            .unwrap_or_else(|| name.clone());
        let ns = first_child(&n, SyntaxKind::SAFETY_NS)
            .and_then(|l| idents(&l).first().cloned())
            .unwrap_or_default();
        let stmts: Vec<SyntaxNode> = n
            .children()
            .filter(|c| {
                matches!(
                    c.kind(),
                    SyntaxKind::SAFETY_GOAL_INST
                        | SyntaxKind::SAFETY_GOAL_INLINE
                        | SyntaxKind::SAFETY_MECHANISM
                        | SyntaxKind::SAFETY_FAULT
                        | SyntaxKind::SAFETY_WAIVE
                        | SyntaxKind::SAFETY_ASSUME
                        | SyntaxKind::SAFETY_REFINES
                        | SyntaxKind::SAFETY_SATISFIED
                        | SyntaxKind::SAFETY_MISSION
                )
            })
            .collect();
        out.push(SafetyBlock { name, entity, ns, stmts });
    }
    out
}

/// A library `safety_assumption` definition (spec §2.5):
/// `safety_assumption ASM_X(pin: net, vmax: voltage) "text with {pin} and {vmax}";`
#[derive(Debug, Clone)]
struct AssumptionDef {
    params: Vec<(String, Option<String>)>, // (name, default)
    text: String,
}

fn collect_assumption_defs(root: &SyntaxNode) -> HashMap<String, AssumptionDef> {
    let mut out = HashMap::new();
    for n in root.descendants().filter(|n| n.kind() == SyntaxKind::SAFETY_ASSUMPTION_DEF) {
        let Some(name) = idents(&n).first().cloned() else { continue };
        let text = first_string(&n).unwrap_or_default();
        let mut params = Vec::new();
        if let Some(p) = first_child(&n, SyntaxKind::SAFETY_GOAL_PARAMS) {
            let txt = text_of(&p);
            let inner = txt.trim_start_matches('(').trim_end_matches(')');
            for item in inner.split(',') {
                let item = item.trim();
                if item.is_empty() { continue; }
                let (lhs, default) = match item.split_once('=') {
                    Some((l, d)) => (l, Some(d.trim().to_string())),
                    None => (item, None),
                };
                let pname = lhs.split(':').next().unwrap_or("").trim().to_string();
                if !pname.is_empty() { params.push((pname, default)); }
            }
        }
        out.insert(name, AssumptionDef { params, text });
    }
    out
}

/// Split `Id(a, b, kw=v)` into all positionals + kwargs (top-level commas).
fn split_call_args(txt: &str) -> (Vec<String>, BTreeMap<String, String>) {
    let Some(open) = txt.find('(') else { return (Vec::new(), BTreeMap::new()) };
    let inner = txt[open + 1..].trim_end_matches(')');
    let mut parts = Vec::new();
    let (mut depth, mut cur, mut in_str) = (0i32, String::new(), false);
    for ch in inner.chars() {
        match ch {
            '"' => { in_str = !in_str; cur.push(ch); }
            '[' | '(' if !in_str => { depth += 1; cur.push(ch); }
            ']' | ')' if !in_str => { depth -= 1; cur.push(ch); }
            ',' if depth == 0 && !in_str => { parts.push(cur.trim().to_string()); cur.clear(); }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() { parts.push(cur.trim().to_string()); }
    let mut pos = Vec::new();
    let mut kw = BTreeMap::new();
    for p in parts {
        match p.split_once('=') {
            Some((k, v)) if k.trim().chars().all(|c| c.is_alphanumeric() || c == '_') => {
                kw.insert(k.trim().to_string(), v.trim().trim_matches('"').to_string());
            }
            _ => pos.push(p),
        }
    }
    (pos, kw)
}

/// Entity-level safety data (spec §2.7), keyed by entity name.
#[derive(Debug, Clone, Default)]
struct EntityData {
    failure_states: Vec<(String, Option<f64>, Option<f64>, String, Option<String>)>, // (name, fit, of, source, behavior)
    seooc: Option<(Option<f64>, Option<f64>, Option<f64>, String)>,  // (lambda, spfm, lfm, source)
    handbook: Option<(String, String, Option<String>)>,              // (class, source, per-standard)
    /// Vendor-data validity configuration: the FMEDA/safety-manual FIT
    /// was computed for THIS configuration (param=value…) — checked
    /// against each instance's attributes. (params, source)
    config: Option<(BTreeMap<String, String>, String)>,
    terminals: Vec<(String, String)>,                                 // (pin, raw text)
    assumptions: Vec<(String, String)>,                               // (id, text)
    errors: Vec<String>,
}

impl EntityData {
    fn merge(&mut self, o: EntityData) {
        self.failure_states.extend(o.failure_states);
        if o.seooc.is_some() { self.seooc = o.seooc; }
        if o.handbook.is_some() { self.handbook = o.handbook; }
        if o.config.is_some() { self.config = o.config; }
        self.terminals.extend(o.terminals);
        self.assumptions.extend(o.assumptions);
        self.errors.extend(o.errors);
    }
}

fn tokens_of(node: &SyntaxNode) -> Vec<String> {
    node.descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| !matches!(t.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT | SyntaxKind::SEMI))
        .map(|t| t.text().to_string())
        .collect()
}

/// `k=v` pairs from a token run, where `=` may be its own token.
fn kv_from_tokens(toks: &[String]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut i = 0;
    while i < toks.len() {
        if i + 2 < toks.len() && toks[i + 1] == "=" {
            out.insert(toks[i].clone(), toks[i + 2].trim_matches('"').to_string());
            i += 3;
        } else if let Some((k, v)) = toks[i].split_once('=') {
            out.insert(k.to_string(), v.trim_matches('"').to_string());
            i += 1;
        } else {
            i += 1;
        }
    }
    out
}

fn num(s: Option<&String>) -> Option<f64> {
    s.and_then(|v| v.trim_end_matches(|c: char| c.is_alphabetic()).parse::<f64>().ok())
}

fn collect_entity_data(root: &SyntaxNode) -> HashMap<String, EntityData> {
    let mut out: HashMap<String, EntityData> = HashMap::new();
    for ent in root.descendants().filter(|n| n.kind() == SyntaxKind::ENTITY_DEF) {
        let Some(ename) = ent
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT)
            .map(|t| t.text().to_string())
        else { continue };
        for blk in ent.children().filter(|c| c.kind() == SyntaxKind::SAFETY_DATA_BLOCK) {
            let d = out.entry(ename.clone()).or_default();
            for item in blk.children().filter(|c| c.kind() == SyntaxKind::SAFETY_DATA_ITEM) {
                let toks = tokens_of(&item);
                let Some(head) = toks.first() else { continue };
                match head.as_str() {
                    "failure_state" => {
                        let name = toks.get(1).cloned().unwrap_or_default();
                        let kv = kv_from_tokens(&toks[2..]);
                        // `fit=12 of 60` → fit=12, and the token after `of`
                        let of = toks.iter().position(|t| t == "of").and_then(|i| toks.get(i + 1)).and_then(|v| v.parse::<f64>().ok());
                        let src = kv.get("source").cloned().unwrap_or_default();
                        if src.is_empty() { d.errors.push(format!("{ename}: failure_state {name} has no source")); }
                        d.failure_states.push((name, num(kv.get("fit")), of, src, kv.get("behavior").cloned()));
                    }
                    "seooc" => {
                        let kv = kv_from_tokens(&toks[1..]);
                        let src = kv.get("source").cloned().unwrap_or_default();
                        if src.is_empty() { d.errors.push(format!("{ename}: seooc has no source")); }
                        d.seooc = Some((num(kv.get("lambda")), num(kv.get("spfm")), num(kv.get("lfm")), src));
                    }
                    "handbook" => {
                        let kv = kv_from_tokens(&toks[1..]);
                        let class = kv.get("class").cloned().or_else(|| toks.get(1).filter(|t| !t.contains('=')).cloned()).unwrap_or_default();
                        let src = kv.get("source").cloned().unwrap_or_default();
                        if src.is_empty() { d.errors.push(format!("{ename}: handbook has no source")); }
                        d.handbook = Some((class, src, kv.get("per").cloned()));
                    }
                    "terminal" => {
                        let pin = toks.get(1).cloned().unwrap_or_default();
                        d.terminals.push((pin, toks[2..].join(" ")));
                    }
                    "config" => {
                        let kv = kv_from_tokens(&toks[1..]);
                        let src = kv.get("source").cloned().unwrap_or_default();
                        if src.is_empty() { d.errors.push(format!("{ename}: config has no source (name the FMEDA tool + configuration)")); }
                        let params: BTreeMap<String, String> = kv.into_iter().filter(|(k, _)| k != "source").collect();
                        if params.is_empty() { d.errors.push(format!("{ename}: config declares no parameters")); }
                        d.config = Some((params, src));
                    }
                    "assumption" => {
                        let id = toks.get(1).cloned().unwrap_or_default();
                        let text = toks.iter().skip(2).find(|t| t.starts_with('"')).map(|t| t.trim_matches('"').to_string()).unwrap_or_default();
                        d.assumptions.push((id, text));
                    }
                    other => d.errors.push(format!("{ename}: unknown safety data item '{other}' (failure_state|seooc|handbook|terminal|assumption|config)")),
                }
            }
        }
    }
    out
}

// ─────────────────────────── netlist view ───────────────────────────

struct NetView {
    /// instance name → type name
    inst_type: HashMap<String, String>,
    /// instance name → expansion parent
    parent_of: HashMap<String, String>,
    /// parent → children
    children_of: HashMap<String, Vec<String>>,
    /// instance name → pin names
    pins_of: HashMap<String, Vec<String>>,
    net_names: HashSet<String>,
    /// set of instance names that are physical parts (not composites, not definition shadows)
    physical: Vec<String>,
    /// instance name → attributes (for the vendor-config validity check)
    attrs_of: HashMap<String, HashMap<String, String>>,
    board_name: String,
}

impl NetView {
    fn build(netlist: &Netlist) -> NetView {
        let mut inst_type = HashMap::new();
        let mut parent_of = HashMap::new();
        let mut children_of: HashMap<String, Vec<String>> = HashMap::new();
        let mut pins_of = HashMap::new();
        let mut attrs_of = HashMap::new();
        let mut physical = Vec::new();
        let board_name = netlist
            .top_level_module
            .and_then(|m| netlist.modules.get(m))
            .map(|m| m.name.clone())
            .unwrap_or_default();
        for (_, inst) in netlist.instances.iter() {
            let Some(def) = netlist.modules.get(inst.definition) else { continue };
            inst_type.insert(inst.name.clone(), def.name.clone());
            attrs_of.insert(inst.name.clone(), inst.attributes.clone());
            let pins: Vec<String> = def
                .pins
                .iter()
                .filter_map(|p| netlist.pins.get(*p).map(|x| x.name.clone()))
                .chain(def.ports.iter().filter_map(|p| netlist.ports.get(*p).map(|x| x.name.clone())))
                .collect();
            pins_of.insert(inst.name.clone(), pins);
            if let Some(p) = inst.attributes.get("expansion_parent") {
                parent_of.insert(inst.name.clone(), p.clone());
                children_of.entry(p.clone()).or_default().push(inst.name.clone());
            }
            // definition shadows are instances named exactly like their type
            let shadow = inst.name == def.name;
            let kind_ok = matches!(def.kind, ModuleKind::PhysicalComponent | ModuleKind::Component | ModuleKind::Module);
            if kind_ok && !shadow {
                physical.push(inst.name.clone());
            }
        }
        // composites (have children) are scopes, not physical parts
        physical.retain(|n| !children_of.contains_key(n));
        physical.sort();
        let net_names: HashSet<String> = netlist
            .nets
            .iter()
            .filter_map(|(_, n)| n.name.clone())
            .collect();
        for v in children_of.values_mut() {
            v.sort();
        }
        NetView { inst_type, parent_of, children_of, pins_of, net_names, physical, attrs_of, board_name }
    }

    /// Resolve `ns.a.b.c` within a scope (instance path prefix, or "" for the board).
    /// Returns (resolved handle, kind) where kind ∈ {instance, pin, net, port}.
    fn resolve(&self, scope_prefix: &str, path: &[String]) -> Option<(String, &'static str)> {
        // candidates: walk the path joining with '_' under the prefix
        // e.g. prefix "rail_a", path ["mon","nOUT"] → "rail_a_mon" + pin "nOUT"
        let join = |a: &str, b: &str| if a.is_empty() { b.to_string() } else { format!("{a}_{b}") };
        let mut cur = scope_prefix.to_string();
        for (i, seg) in path.iter().enumerate() {
            let cand = join(&cur, seg);
            if self.inst_type.contains_key(&cand) {
                cur = cand;
                continue;
            }
            // pin of the current instance?
            if !cur.is_empty() {
                if let Some(pins) = self.pins_of.get(&cur) {
                    if pins.iter().any(|p| p == seg) {
                        if i == path.len() - 1 {
                            return Some((format!("{cur}.{seg}"), "pin"));
                        }
                        return None;
                    }
                }
            }
            // net at this level?
            if i == path.len() - 1 {
                let nn = join(if cur == scope_prefix { "" } else { &cur }, seg);
                if self.net_names.contains(&nn) || self.net_names.contains(seg) {
                    return Some((seg.clone(), "net"));
                }
                // entity port of the scope (virtual pin) — a net on the
                // scope's boundary: accept as "port" if the scope instance has that pin
                if !scope_prefix.is_empty() {
                    if let Some(pins) = self.pins_of.get(scope_prefix) {
                        if pins.iter().any(|p| p == seg) {
                            return Some((format!("{scope_prefix}.{seg}"), "port"));
                        }
                    }
                }
            }
            return None;
        }
        if cur != scope_prefix { Some((cur, "instance")) } else { None }
    }
}

// ─────────────────────────── the pass ───────────────────────────

/// Build the safety model for `netlist` from all `safety_goal` and
/// `safety` blocks in `sources`. `sources` may include a sidecar plus
/// the board file.
pub fn build_safety_model(netlist: &Netlist, sources: &[&SourceFile]) -> SafetyModel {
    let view = NetView::build(netlist);
    let mut goal_defs: HashMap<String, GoalDef> = HashMap::new();
    let mut asm_defs: HashMap<String, AssumptionDef> = HashMap::new();
    let mut blocks: Vec<SafetyBlock> = Vec::new();
    let mut entity_data: HashMap<String, EntityData> = HashMap::new();
    for sf in sources {
        let root = sf.syntax();
        goal_defs.extend(collect_goal_defs(&root));
        asm_defs.extend(collect_assumption_defs(&root));
        blocks.extend(collect_blocks(&root));
        for (k, v) in collect_entity_data(&root) {
            entity_data.entry(k).or_default().merge(v);
        }
    }

    let mut model = SafetyModel {
        board: view.board_name.clone(),
        mission: None,
        scopes: Vec::new(),
        parts: Vec::new(),
        universe: Vec::new(),
        gaps: Vec::new(),
        errors: Vec::new(),
    };

    // Which instances are instances of which entity? inst_type gives it.
    // Apply each block to: the board (if block.entity == board name, scope prefix "")
    // or to every instance of that entity (scope prefix = instance name).
    let mut applications: Vec<(SafetyBlock, String)> = Vec::new(); // (block, scope prefix)
    for b in &blocks {
        if b.entity == view.board_name {
            applications.push((b.clone(), String::new()));
        } else {
            let mut insts: Vec<String> = view
                .inst_type
                .iter()
                .filter(|(n, t)| *t == &b.entity && *n != &b.entity)
                .map(|(n, _)| n.clone())
                .collect();
            insts.sort();
            if insts.is_empty() {
                model.errors.push(format!(
                    "safety {}: entity '{}' is neither the board nor instantiated in it",
                    b.name, b.entity
                ));
            }
            for i in insts {
                applications.push((b.clone(), i));
            }
        }
    }
    // Deterministic: entity scopes first (sorted by prefix), board last,
    // so a board block can refine/discharge instance goals/assumptions.
    applications.sort_by(|a, b| match (a.1.is_empty(), b.1.is_empty()) {
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        _ => a.1.cmp(&b.1).then(a.0.name.cmp(&b.0.name)),
    });

    // A part's declared assumptions of use (spec §2b/§2.7) are
    // requirements on its integrator: surface each as an OPEN assumption
    // in the scope that owns the part (its safety part, else the board),
    // under the path `<owner>.<local>.<id>`, so a parent block can
    // discharge it (`brd.rail_a.mon.ASM_X satisfied_by ..`). Seeded when
    // the scope is created so later-applied blocks see them.
    let seed_part_assumptions = |sc: &mut Scope| {
        for inst in &view.physical {
            let tname = view.inst_type.get(inst).cloned().unwrap_or_default();
            let Some(ed) = entity_data.get(&tname) else { continue };
            if ed.assumptions.is_empty() { continue; }
            let owner = view.parent_of.get(inst).cloned().unwrap_or_default();
            if owner != sc.path { continue; }
            let local = if owner.is_empty() { inst.clone() } else { inst.strip_prefix(&format!("{owner}_")).unwrap_or(inst).to_string() };
            for (id, text) in &ed.assumptions {
                let path = if owner.is_empty() { format!("{local}.{id}") } else { format!("{owner}.{local}.{id}") };
                if !sc.assumptions.iter().any(|a| a.path == path) {
                    sc.assumptions.push(Assumption { id: format!("{local}.{id}"), path, text: text.clone(), status: AssumptionStatus::Open });
                }
            }
        }
    };

    let mut scopes: Vec<Scope> = Vec::new();
    for (blk, prefix) in &applications {
        let mut scope = scopes
            .iter()
            .position(|s| s.path == *prefix)
            .map(|i| scopes.remove(i))
            .unwrap_or_else(|| {
                let mut sc = Scope {
                    path: prefix.clone(),
                    entity: blk.entity.clone(),
                    ns: blk.ns.clone(),
                    goals: Vec::new(),
                    mechanisms: Vec::new(),
                    faults: Vec::new(),
                    waivers: Vec::new(),
                    assumptions: Vec::new(),
                    metrics: None,
                };
                seed_part_assumptions(&mut sc);
                sc
            });
        let qual = |local: &str| if prefix.is_empty() { local.to_string() } else { format!("{prefix}.{local}") };
        // strip leading `ns.` and split
        let strip_ns = |p: &str| -> Vec<String> {
            let mut segs: Vec<String> = p.split('.').map(|s| s.to_string()).collect();
            if segs.first().map(|s| s == &blk.ns).unwrap_or(false) {
                segs.remove(0);
            }
            segs
        };
        // resolve a handle path; for board scope `brd.rail_a.mon` → prefix "" path [rail_a, mon]
        let resolve_handle = |p: &str, errs: &mut Vec<String>| -> Option<String> {
            let segs = strip_ns(p);
            match view.resolve(prefix, &segs) {
                Some((h, _)) => Some(h),
                None => {
                    errs.push(format!("safety {} ({}): unknown handle '{}'", blk.name, if prefix.is_empty() { "board" } else { prefix }, p));
                    None
                }
            }
        };
        // effect expression references: find tokens that look like ns.x.y
        let refs_in = |expr: &str, errs: &mut Vec<String>| -> Vec<String> {
            let mut out = Vec::new();
            for tok in expr.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.' || c == '@')) {
                let t = tok.trim_start_matches('@');
                if t.starts_with(&format!("{}.", blk.ns)) {
                    if let Some(h) = resolve_handle(t, errs) { out.push(h); }
                }
            }
            out.sort();
            out.dedup();
            out
        };

        for st in &blk.stmts {
            match st.kind() {
                SyntaxKind::SAFETY_GOAL_INLINE => {
                    let ids = idents(st);
                    let name = ids.first().cloned().unwrap_or_default();
                    let level = ids.get(1).and_then(|l| Level::parse(l)).unwrap_or(Level::QM);
                    let title = first_string(st).unwrap_or_default();
                    let kw = kwargs_of(st);
                    let mut effects = Vec::new();
                    for (en, ex, sev) in collect_effects(st) {
                        let refs = refs_in(&ex, &mut model.errors);
                        effects.push(Effect { name: en, expr: ex, severity: sev, refs });
                    }
                    scope.goals.push(Goal {
                        path: qual(&name),
                        name,
                        library_type: None,
                        level,
                        title,
                        id: kw.get("id").cloned(),
                        ftti: kw.get("ftti").cloned(),
                        safe_state: kw.get("safe_state").cloned(),
                        effects,
                        refines: None,
                    });
                }
                SyntaxKind::SAFETY_GOAL_INST => {
                    let ids = idents(st);
                    let name = ids.first().cloned().unwrap_or_default();
                    let gtype = ids.get(1).cloned().unwrap_or_default();
                    let Some(def) = goal_defs.get(&gtype) else {
                        model.errors.push(format!("safety {}: unknown library goal '{}'", blk.name, gtype));
                        continue;
                    };
                    // two param lists may exist: first = goal params, second = instance kwargs
                    let lists: Vec<SyntaxNode> = st
                        .children()
                        .filter(|c| c.kind() == SyntaxKind::PARAM_LIST)
                        .collect();
                    let pk = lists.first().map(|l| kwargs_of_list(l)).unwrap_or_default();
                    let ik = lists.get(1).map(|l| kwargs_of_list(l)).unwrap_or_default();
                    // level: explicit `level=` param, else def default, else QM
                    let level = pk
                        .get("level")
                        .and_then(|l| Level::parse(l))
                        .or_else(|| def.params.iter().find(|(n, _)| n == "level").and_then(|(_, d)| d.as_deref().and_then(Level::parse)))
                        .unwrap_or(Level::QM);
                    // bindings
                    let mut bind: BTreeMap<String, String> = BTreeMap::new();
                    for b in child_nodes(st, SyntaxKind::SAFETY_BIND_ITEM) {
                        let fids = idents(&b);
                        let formal = fids.first().cloned().unwrap_or_default();
                        let target = b.children().last().map(|c| text_of(&c)).unwrap_or_default();
                        bind.insert(formal, target);
                    }
                    for f in &def.formals {
                        if !bind.contains_key(f) {
                            model.errors.push(format!("safety {}: goal {} ({}) leaves formal '{}' unbound", blk.name, name, gtype, f));
                        }
                    }
                    for f in bind.keys() {
                        if !def.formals.contains(f) {
                            model.errors.push(format!("safety {}: goal {} ({}) binds unknown formal '{}'", blk.name, name, gtype, f));
                        }
                    }
                    // substitute formals + params into effect expressions
                    let mut effects = Vec::new();
                    for (en, ex, sev) in &def.effects {
                        let mut e = ex.clone();
                        for (f, t) in &bind {
                            e = replace_ident(&e, f, t);
                        }
                        for (pn, pd) in &def.params {
                            let val = pk.get(pn).cloned().or_else(|| pd.clone());
                            if let Some(v) = val { e = replace_ident(&e, pn, &v); }
                        }
                        let refs = refs_in(&e, &mut model.errors);
                        effects.push(Effect { name: en.clone(), expr: e, severity: *sev, refs });
                    }
                    scope.goals.push(Goal {
                        path: qual(&name),
                        name,
                        library_type: Some(gtype.clone()),
                        level,
                        title: def.title.clone(),
                        id: ik.get("id").cloned(),
                        ftti: ik.get("ftti").cloned(),
                        safe_state: ik.get("safe_state").cloned(),
                        effects,
                        refines: None,
                    });
                }
                SyntaxKind::SAFETY_MECHANISM => {
                    let handle = first_child(st, SyntaxKind::NET_REF).map(|n| text_of(&n)).unwrap_or_default();
                    let call = st
                        .children()
                        .filter(|c| c.kind() != SyntaxKind::NET_REF)
                        .map(|c| text_of(&c))
                        .find(|t| !t.is_empty())
                        .unwrap_or_default();
                    let Some(inst) = resolve_handle(&handle, &mut model.errors) else { continue };
                    let Some((kind, goal, kw)) = parse_mechanism_call(&call) else {
                        model.errors.push(format!("safety {}: malformed mechanism '{}'", blk.name, call));
                        continue;
                    };
                    let mk = match kind.as_str() {
                        "psm" => MechanismKind::Psm,
                        "lsm" => MechanismKind::Lsm,
                        _ => { model.errors.push(format!("safety {}: mechanism kind must be psm|lsm, got '{}'", blk.name, kind)); continue; }
                    };
                    let goal_path = qual(&goal);
                    if !scope.goals.iter().any(|g| g.path == goal_path) {
                        model.errors.push(format!("safety {}: mechanism {} names unknown goal '{}'", blk.name, handle, goal));
                    }
                    let detects = kw.get("detects").map(|v| list_items(v)).unwrap_or_default();
                    if let Some(g) = scope.goals.iter().find(|g| g.path == goal_path) {
                        for d in &detects {
                            if !g.effects.iter().any(|e| &e.name == d) {
                                model.errors.push(format!("safety {}: mechanism {} detects unknown effect '{}' of {}", blk.name, handle, d, goal));
                            }
                        }
                    }
                    let protects = kw.get("protects").and_then(|p| resolve_handle(p, &mut model.errors));
                    let claimed_dc = kw.get("dc").and_then(|d| d.parse::<f64>().ok());
                    let dc_source = kw.get("source").map(|s| s.trim_matches('"').to_string());
                    for k in kw.keys() {
                        if !matches!(k.as_str(), "detects" | "protects" | "dc" | "source" | "interval" | "latency" | "detected_when") {
                            model.errors.push(format!("safety {}: mechanism {} has unknown field '{}'", blk.name, handle, k));
                        }
                    }
                    scope.mechanisms.push(Mechanism {
                        instance: inst,
                        handle,
                        kind: mk,
                        goal: goal_path,
                        detects,
                        protects,
                        claimed_dc,
                        dc_source,
                        interval: kw.get("interval").cloned(),
                        latency: kw.get("latency").cloned(),
                        detected_when: kw.get("detected_when").cloned(),
                        measured_dc: None,
                        measured_note: None,
                    });
                }
                SyntaxKind::SAFETY_FAULT => {
                    // children: expr (kind(targets)), NET_REF (expect path), [NET_REF detected_by], [expr within]
                    let mut kind = String::new();
                    let mut targets = Vec::new();
                    let mut expect = String::new();
                    let mut detected_by = None;
                    let mut within = None;
                    let kids: Vec<SyntaxNode> = st.children().collect();
                    // first non-NET_REF child is the fault call
                    if let Some(c) = kids.iter().find(|c| c.kind() != SyntaxKind::NET_REF) {
                        let t = text_of(c);
                        if let Some((k, first, kw)) = parse_mechanism_call(&t) {
                            kind = k;
                            let mut tg = vec![first];
                            tg.extend(kw.into_iter().map(|(k, v)| format!("{k}={v}")));
                            // positional targets beyond the first were dropped by parse_mechanism_call; re-split
                            let open = t.find('(').unwrap_or(0);
                            let inner = t[open + 1..].trim_end_matches(')');
                            tg = inner.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                            for x in tg {
                                if x.starts_with(&format!("{}.", blk.ns)) {
                                    if let Some(h) = resolve_handle(&x, &mut model.errors) { targets.push(h); }
                                } else {
                                    targets.push(x);
                                }
                            }
                        }
                    }
                    let refs: Vec<SyntaxNode> = kids.iter().filter(|c| c.kind() == SyntaxKind::NET_REF).cloned().collect();
                    if let Some(e) = refs.first() {
                        let t = text_of(e);
                        // Goal.effect → qualified
                        let (g, eff) = t.split_once('.').unwrap_or((t.as_str(), ""));
                        expect = format!("{}.{}", qual(g), eff);
                        if !scope.goals.iter().any(|gg| gg.path == qual(g) && gg.effects.iter().any(|x| x.name == eff)) {
                            model.errors.push(format!("safety {}: fault expects unknown effect '{}'", blk.name, t));
                        }
                    }
                    if let Some(d) = refs.get(1) {
                        detected_by = resolve_handle(&text_of(d), &mut model.errors);
                    }
                    // `within` value: last non-NET_REF child after the call
                    if kids.len() > 1 {
                        if let Some(w) = kids.iter().skip(1).rev().find(|c| c.kind() != SyntaxKind::NET_REF) {
                            let wt = text_of(w);
                            if !wt.contains('(') { within = Some(wt); }
                        }
                    }
                    if kind == "state" {
                        // IC-internal faults are vendor-declared model states
                        // (spec §2a): the state must exist on the part's entity.
                        if let (Some(inst), Some(st)) = (targets.first(), targets.get(1)) {
                            let st = st.trim_matches('"');
                            let tname = view.inst_type.get(inst).cloned().unwrap_or_default();
                            let known = entity_data.get(&tname).map(|d| d.failure_states.iter().any(|f| f.0 == st)).unwrap_or(false);
                            if !known {
                                model.errors.push(format!(
                                    "safety {}: fault state({}, \"{}\"): entity {} declares no such failure state{}",
                                    blk.name, inst, st, tname,
                                    if entity_data.get(&tname).map(|d| d.failure_states.is_empty()).unwrap_or(true) { " (no `safety { failure_state .. }` data at all)" } else { "" }
                                ));
                            }
                        }
                    }
                    scope.faults.push(Fault { kind, targets, expect, detected_by, within, run: false, fired: Vec::new(), expectation_met: None, note: None, timing_met: None });
                }
                SyntaxKind::SAFETY_WAIVE => {
                    let handle = first_child(st, SyntaxKind::NET_REF).map(|n| text_of(&n)).unwrap_or_default();
                    let reason = first_string(st).unwrap_or_default();
                    if let Some(inst) = resolve_handle(&handle, &mut model.errors) {
                        scope.waivers.push(Waiver { instance: inst, handle, reason });
                    }
                }
                SyntaxKind::SAFETY_MISSION => {
                    // Board-level mission profile (spec §2.8). Belongs to
                    // the board block: an entity block is applied per
                    // instance and a per-instance environment would be a
                    // contradiction.
                    if !prefix.is_empty() {
                        model.errors.push(format!("safety {}: mission {{ }} belongs to the board block, not an entity block", blk.name));
                        continue;
                    }
                    let mut ambient: Option<f64> = None;
                    let mut on_hours: Option<f64> = None;
                    let mut cycles: Option<f64> = None;
                    let mut environment: Option<String> = None;
                    let mut quality: Option<String> = None;
                    let mut profile: Option<String> = None;
                    let mut time_basis: Option<String> = None;
                    let mut phases: Vec<bhdl_common::safety::MissionPhase> = Vec::new();
                    let leading_num = |v: &str| -> Option<f64> {
                        let end = v.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+')).unwrap_or(v.len());
                        v[..end].parse::<f64>().ok()
                    };
                    for item in child_nodes(st, SyntaxKind::SAFETY_DATA_ITEM) {
                        let t = text_of(&item);
                        let Some((k, v)) = t.split_once('=') else {
                            model.errors.push(format!("safety {}: mission item '{}' is not `key = value`", blk.name, t));
                            continue;
                        };
                        let (k, v) = (k.trim(), v.trim().trim_end_matches(';').trim());
                        match k {
                            "ambient" => ambient = leading_num(v),
                            "on_hours" => on_hours = leading_num(v),
                            "cycles" => cycles = leading_num(v),
                            "environment" => environment = Some(v.trim_matches('"').to_string()),
                            "quality" => quality = Some(v.trim_matches('"').to_string()),
                            "profile" => profile = Some(v.trim_matches('"').to_string()),
                            "time_basis" => time_basis = Some(v.trim_matches('"').to_string()),
                            other => model.errors.push(format!("safety {}: unknown mission item '{}' (ambient|on_hours|cycles|environment|quality|profile|time_basis)", blk.name, other)),
                        }
                    }
                    // Inline phases: `phase NAME { time = 8%; ambient = 60degC; powered = false; }`
                    for ph in child_nodes(st, SyntaxKind::SAFETY_MISSION_PHASE) {
                        let pname = idents(&ph).first().cloned().unwrap_or_default();
                        let (mut frac, mut amb, mut powered): (Option<f64>, Option<f64>, bool) = (None, None, true);
                        for item in child_nodes(&ph, SyntaxKind::SAFETY_DATA_ITEM) {
                            let t = text_of(&item);
                            let Some((k, v)) = t.split_once('=') else { continue };
                            let v = v.trim().trim_end_matches(';');
                            match k.trim() {
                                "time" => {
                                    frac = leading_num(v.trim());
                                    if v.contains('%') { frac = frac.map(|f| f / 100.0); }
                                }
                                "ambient" => amb = leading_num(v.trim()),
                                "powered" => powered = v.trim() != "false",
                                other => model.errors.push(format!("safety {}: phase {}: unknown item '{}' (time|ambient|powered)", blk.name, pname, other)),
                            }
                        }
                        match (frac, amb) {
                            (Some(f), Some(a)) => phases.push(bhdl_common::safety::MissionPhase { name: pname, frac: f, ambient_c: a, powered }),
                            _ => model.errors.push(format!("safety {}: phase {} needs `time = <frac>` and `ambient = <temp>`", blk.name, pname)),
                        }
                    }
                    // Inline phases must cover the whole life. A named
                    // profile's phases are validated after resolution (CLI).
                    if !phases.is_empty() {
                        let sum: f64 = phases.iter().map(|p| p.frac).sum();
                        if (sum - 1.0).abs() > 0.02 {
                            model.errors.push(format!("safety {}: mission phases sum to {:.3}, not 1.0", blk.name, sum));
                        }
                    }
                    if ambient.is_none() && !phases.is_empty() {
                        // display ambient = time-weighted mean of the phases
                        let sum: f64 = phases.iter().map(|p| p.frac).sum::<f64>().max(1e-9);
                        ambient = Some(phases.iter().map(|p| p.frac * p.ambient_c).sum::<f64>() / sum);
                    }
                    match (ambient, &profile) {
                        (Some(a), _) => model.mission = Some(bhdl_common::safety::Mission { ambient_c: a, on_hours, cycles, environment, quality, profile, time_basis, phases }),
                        (None, Some(_)) => model.mission = Some(bhdl_common::safety::Mission { ambient_c: f64::NAN, on_hours, cycles, environment, quality, profile, time_basis, phases }),
                        (None, None) => model.errors.push(format!("safety {}: mission {{ }} has no `ambient = <temp>`, no phases and no profile", blk.name)),
                    }
                }
                SyntaxKind::SAFETY_ASSUME => {
                    let t = st.children().map(|c| text_of(&c)).find(|t| !t.is_empty()).unwrap_or_default();
                    let id = t.split('(').next().unwrap_or("").trim().to_string();
                    let inline = first_string(st);
                    let text = if let Some(def) = asm_defs.get(&id) {
                        // library assumption: substitute call args into the
                        // `{param}` placeholders; design handles are shown
                        // qualified by the instance path (dut.VIN → rail_a.VIN).
                        let (pos, kw) = split_call_args(&t);
                        let show = |v: &str| -> String {
                            let v = v.trim_start_matches('@');
                            if v.starts_with(&format!("{}.", blk.ns)) {
                                let rest = v.splitn(2, '.').nth(1).unwrap_or(v);
                                if prefix.is_empty() { rest.to_string() } else { format!("{prefix}.{rest}") }
                            } else {
                                v.to_string()
                            }
                        };
                        if pos.len() > def.params.len() {
                            model.errors.push(format!("safety {}: assume {}: {} arguments, {} parameters", blk.name, id, pos.len(), def.params.len()));
                        }
                        for k in kw.keys() {
                            if !def.params.iter().any(|(n, _)| n == k) {
                                model.errors.push(format!("safety {}: assume {}: unknown parameter '{}'", blk.name, id, k));
                            }
                        }
                        let mut text = def.text.clone();
                        for (i, (pname, default)) in def.params.iter().enumerate() {
                            let val = pos.get(i).cloned()
                                .or_else(|| kw.get(pname).cloned())
                                .or_else(|| default.clone());
                            match val {
                                Some(v) => text = text.replace(&format!("{{{pname}}}"), &show(&v)),
                                None => model.errors.push(format!("safety {}: assume {}: parameter '{}' has no argument and no default", blk.name, id, pname)),
                            }
                        }
                        text
                    } else if inline.is_none() && t.contains('(') {
                        model.errors.push(format!("safety {}: assume {}: unknown library assumption (no `safety_assumption {}` in scope and no inline text)", blk.name, id, id));
                        continue;
                    } else {
                        inline.unwrap_or_else(|| t.clone())
                    };
                    scope.assumptions.push(Assumption { path: qual(&id), id, text, status: AssumptionStatus::Open });
                }
                SyntaxKind::SAFETY_REFINES => {
                    let refs: Vec<SyntaxNode> = child_nodes(st, SyntaxKind::NET_REF);
                    let subj = refs.first().map(|n| text_of(&n)).unwrap_or_default();
                    let parent = refs.get(1).map(|n| text_of(&n)).unwrap_or_default();
                    // subject: ns.inst.Goal → scope path "inst", goal name
                    let segs = strip_ns(&subj);
                    let (gname, spath) = match segs.split_last() { Some((g, s)) => (g.clone(), s.join("_")), None => (String::new(), String::new()) };
                    let parent_path = qual(&parent);
                    let target_scope_path = if prefix.is_empty() { spath } else { format!("{prefix}_{spath}") };
                    let mut found = false;
                    for s in scopes.iter_mut() {
                        if s.path == target_scope_path {
                            if let Some(g) = s.goals.iter_mut().find(|g| g.name == gname) {
                                g.refines = Some(parent_path.clone());
                                found = true;
                            }
                        }
                    }
                    if !found {
                        model.errors.push(format!("safety {}: refines: unknown goal '{}'", blk.name, subj));
                    }
                    if !scope.goals.iter().any(|g| g.path == parent_path) {
                        model.errors.push(format!("safety {}: refines: unknown parent goal '{}'", blk.name, parent));
                    }
                }
                SyntaxKind::SAFETY_SATISFIED => {
                    let refs: Vec<SyntaxNode> = child_nodes(st, SyntaxKind::NET_REF);
                    let subj = refs.first().map(|n| text_of(&n)).unwrap_or_default();
                    let segs = strip_ns(&subj);
                    // The subject is `<scope path>.<assumption id>`; the id may
                    // itself be dotted (part assumptions surface as
                    // `<local>.<ID>`), so try every split: the LONGEST scope
                    // path that exists wins, the rest is the id.
                    let mut split: Option<(String, String)> = None;
                    for cut in (1..segs.len()).rev() {
                        let spath = segs[..cut].join("_");
                        let spath = if prefix.is_empty() { spath } else { format!("{prefix}_{spath}") };
                        if scopes.iter().any(|s| s.path == spath) || scope.path == spath {
                            split = Some((segs[cut..].join("."), spath));
                            break;
                        }
                    }
                    let (aid, target_scope_path) = split.unwrap_or_else(|| {
                        match segs.split_last() { Some((a, s)) => (a.clone(), s.join("_")), None => (String::new(), String::new()) }
                    });
                    let status = if let Some(by) = refs.get(1) {
                        match resolve_handle(&text_of(by), &mut model.errors) {
                            Some(h) => AssumptionStatus::SatisfiedBy(h),
                            None => continue,
                        }
                    } else {
                        AssumptionStatus::Waived(first_string(st).unwrap_or_default())
                    };
                    let mut found = false;
                    for s in scopes.iter_mut() {
                        if s.path == target_scope_path {
                            if let Some(a) = s.assumptions.iter_mut().find(|a| a.id == aid) {
                                a.status = status.clone();
                                found = true;
                            }
                        }
                    }
                    if !found {
                        model.errors.push(format!("safety {}: satisfied_by/waived: unknown assumption '{}'", blk.name, subj));
                    }
                }
                _ => {}
            }
        }
        scopes.push(scope);
    }
    scopes.sort_by(|a, b| a.path.cmp(&b.path));

    for sc in scopes.iter_mut() { seed_part_assumptions(sc); }

    // Parts table: every physical instance, grouped by expansion parent.
    let waived: HashMap<String, String> = scopes
        .iter()
        .flat_map(|s| s.waivers.iter().map(|w| (w.instance.clone(), w.reason.clone())))
        .collect();
    for (_, ed) in entity_data.iter() {
        model.errors.extend(ed.errors.iter().cloned());
    }
    for inst in &view.physical {
        let tname = view.inst_type.get(inst).cloned().unwrap_or_default();
        let ed = entity_data.get(&tname);
        let data = if let Some(r) = waived.get(inst) {
            PartData::Waived { reason: r.clone() }
        } else if let Some(ed) = ed {
            if !ed.failure_states.is_empty() {
                let src = ed.failure_states.iter().map(|f| f.3.clone()).find(|s| !s.is_empty()).unwrap_or_default();
                PartData::Behavioral {
                    failure_states: ed.failure_states.len(),
                    source: src,
                    states: ed.failure_states.iter().map(|f| bhdl_common::safety::FailureState {
                        name: f.0.clone(), fit: f.1, behavior: f.4.clone(),
                    }).collect(),
                }
            } else if let Some((lambda, _, _, src)) = &ed.seooc {
                PartData::Seooc { lambda_fit: *lambda, source: src.clone() }
            } else if let Some((class, src, per)) = &ed.handbook {
                PartData::Handbook { class: class.clone(), source: src.clone(), per: per.clone(), fit: None, fit_basis: None }
            } else {
                PartData::None
            }
        } else {
            PartData::None
        };
        // Vendor-data validity check (spec §2.7): the FMEDA/safety-manual
        // FIT was computed for a declared configuration; compare each
        // declared param against this instance's actual attribute (entity
        // params reach instances through the attribute flow). A param the
        // instance does not expose, or exposes with a different value,
        // means the vendor data does not apply to this instance.
        if let Some(ed) = ed {
            if let Some((params, src)) = &ed.config {
                let attrs = view.attrs_of.get(inst);
                let same = |a: &str, b: &str| -> bool {
                    let numeric = |v: &str| -> Option<f64> {
                        let end = v.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+')).unwrap_or(v.len());
                        v[..end].parse::<f64>().ok()
                    };
                    match (numeric(a), numeric(b)) {
                        (Some(x), Some(y)) => (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1e-30),
                        _ => a.trim() == b.trim(),
                    }
                };
                for (k, want) in params {
                    match attrs.and_then(|a| a.get(k)) {
                        None => model.gaps.push(Gap {
                            class: GapClass::ConfigMismatch,
                            goal: String::new(),
                            subject: inst.clone(),
                            fix: format!("vendor data ({src}) is valid for {k}={want}, but the instance does not expose '{k}' — export it (`attribute {k} = {k};`) so the configuration is checkable"),
                        }),
                        Some(have) if !same(have, want) => model.gaps.push(Gap {
                            class: GapClass::ConfigMismatch,
                            goal: String::new(),
                            subject: inst.clone(),
                            fix: format!("vendor data ({src}) is valid for {k}={want}, this instance has {k}={have} — the FIT/failure split does not apply; regenerate the vendor FMEDA for this configuration"),
                        }),
                        Some(_) => {}
                    }
                }
            }
        }
        model.parts.push(Part {
            instance: inst.clone(),
            type_name: view.inst_type.get(inst).cloned().unwrap_or_default(),
            parent: view.parent_of.get(inst).cloned(),
            data,
        });
    }

    // Gaps.
    // Refinement: a goal refined by child goals is covered when every
    // refiner has all of its effects covered by a PSM (ISO 26262-3/4: the
    // parent goal is satisfied through its refining requirements).
    let all_goals: Vec<&Goal> = scopes.iter().flat_map(|s| s.goals.iter()).collect();
    let psm_covers = |goal_path: &str, effect: &str| -> bool {
        scopes.iter().any(|s| s.mechanisms.iter().any(|m| m.kind == MechanismKind::Psm && m.goal == goal_path && m.detects.iter().any(|d| d == effect)))
    };
    let refiners_of = |goal_path: &str| -> Vec<&Goal> { all_goals.iter().copied().filter(|g| g.refines.as_deref() == Some(goal_path)).collect() };
    let covered_by_refinement = |goal_path: &str| -> bool {
        let rs = refiners_of(goal_path);
        !rs.is_empty() && rs.iter().all(|r| r.effects.iter().all(|e| psm_covers(&r.path, &e.name)))
    };
    let mut gaps = Vec::new();
    for s in &scopes {
        for g in &s.goals {
            for e in &g.effects {
                let covered = psm_covers(&g.path, &e.name) || covered_by_refinement(&g.path);
                if !covered {
                    gaps.push(Gap { class: GapClass::EffectUndetected, goal: g.path.clone(), subject: e.name.clone(), fix: format!("declare a psm mechanism that detects '{}' or refine the goal", e.name) });
                }
            }
            if g.level.requires_lsm() {
                for m in s.mechanisms.iter().filter(|m| m.kind == MechanismKind::Psm && m.goal == g.path) {
                    let has_lsm = s.mechanisms.iter().any(|l| l.kind == MechanismKind::Lsm && l.goal == g.path && l.protects.as_deref() == Some(m.instance.as_str()));
                    if !has_lsm {
                        gaps.push(Gap { class: GapClass::PsmWithoutLsm, goal: g.path.clone(), subject: m.handle.clone(), fix: format!("{} requires latent-fault coverage: declare an lsm protecting {}", g.level.as_str(), m.handle) });
                    }
                }
            }
        }
        for m in &s.mechanisms {
            if m.claimed_dc.is_some() && m.dc_source.is_none() {
                gaps.push(Gap { class: GapClass::DcUnsourced, goal: m.goal.clone(), subject: m.handle.clone(), fix: "add source=\"...\" for the claimed dc".into() });
            }
        }
        for a in &s.assumptions {
            if a.status == AssumptionStatus::Open {
                gaps.push(Gap { class: GapClass::AssumptionOpen, goal: s.path.clone(), subject: a.path.clone(), fix: format!("discharge in the parent block: {} satisfied_by <handle>; or waived \"reason\";", a.path) });
            }
        }
        for f in &s.faults {
            if !f.run {
                gaps.push(Gap { class: GapClass::FaultUnrun, goal: f.expect.clone(), subject: format!("{}({})", f.kind, f.targets.join(",")), fix: "fault not yet run (the campaign runs when the board's DC solve converges)".into() });
            }
        }
    }
    // part gaps only for scopes that exist (a board with no safety block has nothing to claim)
    if !scopes.is_empty() {
        for p in &model.parts {
            if p.data == PartData::None {
                gaps.push(Gap { class: GapClass::PartNoSafetyData, goal: p.parent.clone().unwrap_or_else(|| view.board_name.clone()), subject: p.instance.clone(), fix: format!("{}: declare failure states / seooc data on the entity, or waive with a reason", p.type_name) });
            }
        }
    }
    model.scopes = scopes;
    // Append: the parts loop may already have pushed CONFIG_MISMATCH gaps.
    model.gaps.extend(gaps);
    model
}

/// kwargs from a PARAM_LIST-like node directly.
fn kwargs_of_list(list: &SyntaxNode) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut pos = 0usize;
    for item in list.children() {
        let txt = text_of(&item);
        if let Some((k, v)) = txt.split_once('=') {
            out.insert(k.trim().to_string(), v.trim().trim_matches('"').to_string());
        } else if !txt.is_empty() {
            out.insert(pos.to_string(), txt.trim_matches('"').to_string());
            pos += 1;
        }
    }
    out
}

/// Whole-identifier replacement (no regex crate dependency needed).
fn replace_ident(expr: &str, ident: &str, with: &str) -> String {
    let mut out = String::with_capacity(expr.len());
    let bytes: Vec<char> = expr.chars().collect();
    let id: Vec<char> = ident.chars().collect();
    let mut i = 0;
    let is_id = |c: char| c.is_alphanumeric() || c == '_';
    while i < bytes.len() {
        if i + id.len() <= bytes.len()
            && bytes[i..i + id.len()] == id[..]
            && (i == 0 || !is_id(bytes[i - 1]))
            && (i + id.len() == bytes.len() || !is_id(bytes[i + id.len()]))
            && (i == 0 || bytes[i - 1] != '.')
        {
            out.push_str(with);
            i += id.len();
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}
