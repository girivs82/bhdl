//! Phase-3 fault campaign (docs/spec/Functional_Safety.md §2.5, §3):
//! run the DECLARED faults for real. Each board-side fault is a netlist
//! mutation — short = merge the two nets, open = detach the pin(s) /
//! remove the part — solved with the same GLACIER DC path the sign-off
//! uses, and every effect predicate in the fault's scope is evaluated
//! on the FAULTED operating point. The vendor's healthy behavioral
//! models react to the mutated board on their own (doctrine: an FB
//! resistor shorting to ground is simulated on the PCB; the model sees
//! FB = 0 V).
//!
//! What this increment does NOT do, and says so per fault instead of
//! pretending: `state(part, "…")` faults need the vendor model
//! failure-state hook (they stay unrun with that note); `drift` needs
//! value mutation with a magnitude argument (same); `within <FTTI>`
//! timing needs transient simulation — the static campaign only
//! classifies the settled operating point. A fault whose mutated board
//! does not converge is reported as such — non-convergence is itself
//! information (usually a catastrophic short), never silently skipped.
//!
//! Effect predicates are VOLTAGE predicates over resolved nets/pins
//! (`brd.r_bot.1 > 8V`, `dut.VOUT < 4V && dut.nFAULT > 2.5V`).
//! Comparisons happen in volts; `&&`/`||`/`!` and parentheses compose.
//! An identifier that does not resolve, or a non-voltage comparison, is
//! a per-fault error — never a silently-false predicate.

use std::collections::HashMap;

use bhdl_common::safety::{GapClass, SafetyModel};
use bhdl_netlist::{ConnectionPoint, Netlist};

/// Solver callback: faulted netlist → net-name → volts (or why not).
pub type Solver<'a> = dyn Fn(&Netlist) -> Result<HashMap<String, f64>, String> + 'a;

/// Minimal connectivity view for mutation + predicate resolution.
struct View {
    /// instance name → InstanceId
    inst: HashMap<String, bhdl_netlist::InstanceId>,
    /// (instance name, pin name) → net name
    pin_net: HashMap<(String, String), String>,
    /// net names
    nets: std::collections::HashSet<String>,
    /// instance name → connected pin names (sorted)
    pins_of: HashMap<String, Vec<String>>,
}

impl View {
    fn build(n: &Netlist) -> View {
        let mut inst = HashMap::new();
        for (id, i) in n.instances.iter() {
            inst.insert(i.name.clone(), id);
        }
        let mut pin_net = HashMap::new();
        for (_, pi) in n.pin_instances.iter() {
            let Some(net_id) = pi.net else { continue };
            let Some(net) = n.nets.get(net_id) else { continue };
            let Some(net_name) = net.name.clone() else { continue };
            let Some(i) = n.instances.get(pi.instance) else { continue };
            let Some(pin) = n.pins.get(pi.pin_def) else { continue };
            pin_net.insert((i.name.clone(), pin.name.clone()), net_name);
        }
        let nets = n
            .nets
            .iter()
            .filter_map(|(_, net)| net.name.clone())
            .collect();
        let mut pins_of: HashMap<String, Vec<String>> = HashMap::new();
        for (inst_name, pin_name) in pin_net.keys() {
            pins_of.entry(inst_name.clone()).or_default().push(pin_name.clone());
        }
        for v in pins_of.values_mut() {
            v.sort();
        }
        View { inst, pin_net, nets, pins_of }
    }

    /// Resolve an ns-stripped, scope-prefixed identifier to a NET name.
    /// `segs` like ["r_bot", "1"] or ["VOUT"]; prefix "" or "rail_a".
    fn resolve_net(&self, prefix: &str, segs: &[String]) -> Option<String> {
        let join = |a: &str, b: &str| if a.is_empty() { b.to_string() } else { format!("{a}_{b}") };
        // instance + pin
        if segs.len() >= 2 {
            let inst = segs[..segs.len() - 1]
                .iter()
                .fold(prefix.to_string(), |a, s| join(&a, s));
            if let Some(net) = self.pin_net.get(&(inst, segs[segs.len() - 1].clone())) {
                return Some(net.clone());
            }
        }
        // bare net name (with and without prefix)
        let flat = segs.iter().fold(prefix.to_string(), |a, s| join(&a, s));
        if self.nets.contains(&flat) {
            return Some(flat);
        }
        let bare = segs.join("_");
        if self.nets.contains(&bare) {
            return Some(bare);
        }
        None
    }
}

// ── netlist mutations ───────────────────────────────────────────────

/// `short(a.x, a.y)`: move every connection of pin-B's net onto pin-A's
/// net (the two nodes become one).
fn apply_short(n: &mut Netlist, view: &View, a: &str, b: &str) -> Result<Option<(String, String)>, String> {
    let pin_of = |t: &str| -> Result<(String, String), String> {
        t.rsplit_once('.')
            .map(|(i, p)| (i.to_string(), p.to_string()))
            .ok_or_else(|| format!("short target '{t}' is not <instance>.<pin>"))
    };
    let (ia, pa) = pin_of(a)?;
    let (ib, pb) = pin_of(b)?;
    let mut na = view.pin_net.get(&(ia.clone(), pa.clone())).ok_or_else(|| format!("{ia}.{pa}: no net"))?.clone();
    let mut nb = view.pin_net.get(&(ib.clone(), pb.clone())).ok_or_else(|| format!("{ib}.{pb}: no net"))?.clone();
    if na == nb {
        return Ok(None); // already the same node — the short is a no-op
    }
    // The surviving node must be the RAIL when one side is a supply:
    // merging GND into a signal node would destroy the solver's ground
    // reference. Keep the power/ground-classed (or GND-named) net.
    let is_rail = |name: &str| -> bool {
        if name == "GND" { return true; }
        n.nets.iter().any(|(_, net)| net.name.as_deref() == Some(name)
            && matches!(net.net_class, bhdl_netlist::NetClass::Power { .. }))
    };
    if is_rail(&nb) && !is_rail(&na) {
        std::mem::swap(&mut na, &mut nb);
    }
    // The synthesizer can leave DUPLICATE nets carrying the same name
    // (different NetIds, one shared node in the solver) — merge by NAME,
    // catching every id, or the mutation silently misses the copy the
    // pin instances actually point at.
    let ids_a: Vec<_> = n.nets.iter().filter(|(_, net)| net.name.as_deref() == Some(na.as_str())).map(|(id, _)| id).collect();
    let ids_b: Vec<_> = n.nets.iter().filter(|(_, net)| net.name.as_deref() == Some(nb.as_str())).map(|(id, _)| id).collect();
    let id_a = *ids_a.first().ok_or("net A missing")?;
    if ids_b.is_empty() {
        return Err("net B missing".into());
    }
    // re-point pin instances on ANY id of net B (and any duplicate of A)
    let move_from: Vec<_> = ids_b.iter().chain(ids_a.iter().skip(1)).copied().collect();
    for (_, pi) in n.pin_instances.iter_mut() {
        if pi.net.map(|nid| move_from.contains(&nid)).unwrap_or(false) {
            pi.net = Some(id_a);
        }
    }
    // move connection points, then remove the emptied nets
    for from in move_from {
        let conns: Vec<ConnectionPoint> = n.nets.get(from).map(|net| net.connections.clone()).unwrap_or_default();
        if let Some(net_a) = n.nets.get_mut(id_a) {
            net_a.connections.extend(conns);
        }
        n.nets.remove(from);
    }
    Ok(Some((nb, na)))
}

/// `open(part)` — remove the instance entirely; `open(part.pin)` —
/// detach just that pin.
fn apply_open(n: &mut Netlist, view: &View, target: &str) -> Result<(), String> {
    if let Some((inst_name, pin_name)) = target.rsplit_once('.') {
        if view.inst.contains_key(inst_name) {
            let iid = view.inst[inst_name];
            let pi_id = n
                .pin_instances
                .iter()
                .find(|(_, pi)| pi.instance == iid && n.pins.get(pi.pin_def).map(|p| p.name == pin_name).unwrap_or(false))
                .map(|(id, _)| id)
                .ok_or_else(|| format!("open: pin {target} not found"))?;
            let net_id = n.pin_instances.get(pi_id).and_then(|pi| pi.net);
            if let Some(pi) = n.pin_instances.get_mut(pi_id) {
                pi.net = None;
            }
            if let Some(nid) = net_id {
                if let Some(net) = n.nets.get_mut(nid) {
                    net.connections.retain(|c| !matches!(c, ConnectionPoint::PinInstance(id) if *id == pi_id));
                }
            }
            return Ok(());
        }
    }
    let iid = *view.inst.get(target).ok_or_else(|| format!("open: instance '{target}' not found"))?;
    // detach all pins, then remove the instance
    let pin_ids: Vec<_> = n.pin_instances.iter().filter(|(_, pi)| pi.instance == iid).map(|(id, _)| id).collect();
    for pid in pin_ids {
        let net_id = n.pin_instances.get(pid).and_then(|pi| pi.net);
        if let Some(pi) = n.pin_instances.get_mut(pid) {
            pi.net = None;
        }
        if let Some(nid) = net_id {
            if let Some(net) = n.nets.get_mut(nid) {
                net.connections.retain(|c| !matches!(c, ConnectionPoint::PinInstance(id) if *id == pid));
            }
        }
    }
    if let Some(net_ids) = Some(()) {
        let _ = net_ids;
    }
    // also strip InstancePin/InstancePort connection points naming it
    for (_, net) in n.nets.iter_mut() {
        net.connections.retain(|c| !matches!(c, ConnectionPoint::InstancePin(id, _) | ConnectionPoint::InstancePort(id, _) if *id == iid));
    }
    n.instances.remove(iid);
    Ok(())
}

// ── effect predicate evaluation ─────────────────────────────────────

struct Pred<'a> {
    toks: Vec<String>,
    i: usize,
    prefix: &'a str,
    ns: &'a str,
    view: &'a View,
    alias: &'a HashMap<String, String>,
    volts: &'a HashMap<String, f64>,
}

fn lex(expr: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            if !cur.is_empty() { out.push(std::mem::take(&mut cur)); }
            i += 1;
        } else if c.is_alphanumeric() || c == '_' || c == '.' {
            cur.push(c);
            i += 1;
        } else {
            if !cur.is_empty() { out.push(std::mem::take(&mut cur)); }
            // two-char ops
            if i + 1 < chars.len() {
                let two: String = chars[i..i + 2].iter().collect();
                if matches!(two.as_str(), "&&" | "||" | ">=" | "<=" | "==" | "!=") {
                    out.push(two);
                    i += 2;
                    continue;
                }
            }
            out.push(c.to_string());
            i += 1;
        }
    }
    if !cur.is_empty() { out.push(cur); }
    out
}

impl<'a> Pred<'a> {
    fn peek(&self) -> Option<&str> { self.toks.get(self.i).map(|s| s.as_str()) }
    fn bump(&mut self) -> Option<String> { let t = self.toks.get(self.i).cloned(); self.i += 1; t }

    fn value(&mut self) -> Result<f64, String> {
        let t = self.bump().ok_or("expected value")?;
        // number with optional unit suffix (5.5V, 2.5, 100mV)
        let numeric = t.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false);
        if numeric {
            let end = t.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(t.len());
            let num: f64 = t[..end].parse().map_err(|_| format!("bad number '{t}'"))?;
            let mult = match t[end..].trim_end_matches('V') {
                "" => 1.0,
                "m" => 1e-3,
                "u" | "µ" => 1e-6,
                "k" | "K" => 1e3,
                other => return Err(format!("unknown unit '{other}' in '{t}'")),
            };
            return Ok(num * mult);
        }
        // identifier path → net voltage
        let mut segs: Vec<String> = t.split('.').map(|s| s.to_string()).collect();
        if segs.first().map(|s| s == self.ns).unwrap_or(false) {
            segs.remove(0);
        }
        let mut net = self
            .view
            .resolve_net(self.prefix, &segs)
            .ok_or_else(|| format!("'{t}' does not resolve to a net in scope '{}'", if self.prefix.is_empty() { "board" } else { self.prefix }))?;
        // follow merge aliases (a shorted net's identity moved)
        let mut hops = 0;
        while let Some(to) = self.alias.get(&net) {
            net = to.clone();
            hops += 1;
            if hops > 8 { break; }
        }
        self.volts
            .get(&net)
            .copied()
            .ok_or_else(|| format!("net '{net}' has no solved voltage on the faulted board"))
    }

    fn comparison(&mut self) -> Result<bool, String> {
        if self.peek() == Some("!") {
            self.bump();
            return Ok(!self.comparison()?);
        }
        if self.peek() == Some("(") {
            self.bump();
            let v = self.or_expr()?;
            if self.bump().as_deref() != Some(")") {
                return Err("expected ')'".into());
            }
            return Ok(v);
        }
        let lhs = self.value()?;
        let op = self.bump().ok_or("expected comparison operator")?;
        let rhs = self.value()?;
        Ok(match op.as_str() {
            ">" => lhs > rhs,
            "<" => lhs < rhs,
            ">=" => lhs >= rhs,
            "<=" => lhs <= rhs,
            "==" => (lhs - rhs).abs() < 1e-3,
            "!=" => (lhs - rhs).abs() >= 1e-3,
            other => return Err(format!("unknown operator '{other}'")),
        })
    }

    fn and_expr(&mut self) -> Result<bool, String> {
        let mut v = self.comparison()?;
        while self.peek() == Some("&&") {
            self.bump();
            let r = self.comparison()?;
            v = v && r;
        }
        Ok(v)
    }

    fn or_expr(&mut self) -> Result<bool, String> {
        let mut v = self.and_expr()?;
        while self.peek() == Some("||") {
            self.bump();
            let r = self.and_expr()?;
            v = v || r;
        }
        Ok(v)
    }
}

/// Evaluate one effect predicate. Identifiers resolve against the
/// HEALTHY connectivity (a predicate is a property of the board's nets
/// — an opened part's pin still names the net it sat on), then the
/// fault's net-alias map translates merged nets, and the voltage comes
/// from the FAULTED solve.
fn eval_effect(
    expr: &str,
    prefix: &str,
    ns: &str,
    view: &View,
    alias: &HashMap<String, String>,
    volts: &HashMap<String, f64>,
) -> Result<bool, String> {
    let mut p = Pred { toks: lex(expr), i: 0, prefix, ns, view, alias, volts };
    let v = p.or_expr()?;
    if p.i != p.toks.len() {
        return Err(format!("trailing tokens after predicate: {:?}", &p.toks[p.i..]));
    }
    Ok(v)
}

/// Run every declared fault. Marks each fault run (or notes exactly why
/// it could not run), records fired effects + whether the expectation
/// was met, and clears the corresponding FAULT_UNRUN gaps.
pub fn run_declared_faults(netlist: &Netlist, model: &mut SafetyModel, solve: &Solver) -> (usize, usize) {
    let view = View::build(netlist);
    let mut ran = 0usize;
    let mut mismatched = 0usize;
    // (scope idx, fault idx) → work on a snapshot of goals for evaluation
    let scopes_goals: Vec<(String, String, Vec<(String, String)>)> = model
        .scopes
        .iter()
        .map(|s| {
            (
                s.path.clone(),
                s.ns.clone(),
                s.goals
                    .iter()
                    .flat_map(|g| g.effects.iter().map(move |e| (format!("{}.{}", g.path, e.name), e.expr.clone())))
                    .collect(),
            )
        })
        .collect();
    for (si, scope) in model.scopes.iter_mut().enumerate() {
        let (prefix, ns, effects) = &scopes_goals[si];
        for f in scope.faults.iter_mut() {
            if f.run {
                continue;
            }
            // unsupported kinds: say why, precisely
            match f.kind.as_str() {
                "short" | "open" => {}
                "state" => {
                    f.note = Some("needs the vendor-model failure-state hook (behavioral model campaign)".into());
                    continue;
                }
                "drift" => {
                    f.note = Some("drift magnitude mutation not implemented in the static campaign yet".into());
                    continue;
                }
                other => {
                    f.note = Some(format!("fault kind '{other}' not supported by the campaign"));
                    continue;
                }
            }
            // mutate a clone
            let mut faulted = netlist.clone();
            let mut alias: HashMap<String, String> = HashMap::new();
            let res = match f.kind.as_str() {
                "short" => {
                    if f.targets.len() != 2 {
                        Err(format!("short needs 2 pin targets, got {}", f.targets.len()))
                    } else {
                        apply_short(&mut faulted, &view, &f.targets[0], &f.targets[1]).map(|a| {
                            if let Some((from, to)) = a {
                                alias.insert(from, to);
                            }
                        })
                    }
                }
                "open" => {
                    if f.targets.len() != 1 {
                        Err(format!("open needs 1 target, got {}", f.targets.len()))
                    } else {
                        apply_open(&mut faulted, &view, &f.targets[0])
                    }
                }
                _ => unreachable!(),
            };
            if let Err(e) = res {
                f.note = Some(format!("mutation failed: {e}"));
                continue;
            }
            // solve the faulted board
            let volts = match solve(&faulted) {
                Ok(v) => v,
                Err(e) => {
                    f.run = true;
                    f.note = Some(format!("faulted board did not converge ({e}) — usually a catastrophic short; classify by inspection"));
                    f.expectation_met = None;
                    ran += 1;
                    continue;
                }
            };
            // evaluate every effect in the scope on the faulted point
            let mut fired = Vec::new();
            let mut eval_errs = Vec::new();
            for (path, expr) in effects {
                match eval_effect(expr, prefix, ns, &view, &alias, &volts) {
                    Ok(true) => fired.push(path.clone()),
                    Ok(false) => {}
                    Err(e) => eval_errs.push(format!("{path}: {e}")),
                }
            }
            let expect_full = &f.expect; // e.g. "SG_MID.overvoltage" or "rail_a.SG_OV.overvoltage"
            let met = fired.iter().any(|p| p == expect_full || p.ends_with(&format!(".{expect_full}")) || expect_full.ends_with(p.as_str()));
            f.run = true;
            f.fired = fired;
            f.expectation_met = Some(met);
            if !met {
                mismatched += 1;
            }
            if !eval_errs.is_empty() {
                f.note = Some(format!("effect eval errors: {}", eval_errs.join("; ")));
            }
            ran += 1;
        }
    }
    // FAULT_UNRUN gaps: drop the build-time placeholders and regenerate
    // from the post-campaign truth. A fault clears its gap ONLY when it
    // ran AND its expectation held — ran-without-verdict (divergence)
    // and expectation-not-met each keep a gap saying exactly what
    // happened.
    model.gaps.retain(|g| g.class != GapClass::FaultUnrun);
    let mut new_gaps = Vec::new();
    for s in &model.scopes {
        for f in &s.faults {
            let subject = format!("{}({})", f.kind, f.targets.join(","));
            let fix = match (f.run, f.expectation_met) {
                (true, Some(true)) => continue,
                (true, Some(false)) => format!(
                    "campaign ran: expected {} did NOT fire (fired: [{}]) — fix the fault declaration or the design",
                    f.expect,
                    f.fired.join(", ")
                ),
                (true, None) => format!("campaign ran without verdict: {}", f.note.as_deref().unwrap_or("no note")),
                (false, _) => format!("not run: {}", f.note.as_deref().unwrap_or("fault campaign did not reach this fault")),
            };
            new_gaps.push(bhdl_common::safety::Gap { class: GapClass::FaultUnrun, goal: f.expect.clone(), subject, fix });
        }
    }
    model.gaps.extend(new_gaps);
    model.gaps.sort_by(|a, b| (a.class as u8, &a.subject).cmp(&(b.class as u8, &b.subject)));
    (ran, mismatched)
}


// ── whole-universe campaign + measured DC (Phase 3 increment 2) ─────

/// Run the AUTOMATIC fault universe: every unwaived physical part ×
/// its standard failure modes — 2-pin parts get `short` + `open`,
/// multi-pin parts get a per-pin `open` (pin-to-pin shorts beyond the
/// declared faults need adjacency knowledge the netlist does not
/// carry — stated, not silently skipped), and parts with declared
/// behavioral failure states are LISTED as needing the vendor-model
/// hook. Each fault is classified on the faulted operating point:
///
///   dangerous     — any effect predicate in the owning scope fired
///   detected      — a mechanism's `detected_when` predicate was TRUE
///   residual      — dangerous and NOT detected
///   false alarm   — detected with NO dangerous effect
///
/// Measured DC per mechanism = detected dangerous weight / dangerous
/// weight, over the faults whose fired effects intersect the
/// mechanism's `detects` list. Weight = the part's COMPUTED FIT split
/// equally over its modes when the reliability engine produced one
/// (the equal split is labelled — mode fractions are data we do not
/// have); count-basis otherwise, also labelled.
pub fn run_universe(netlist: &Netlist, model: &mut SafetyModel, solve: &Solver) {
    use bhdl_common::safety::{PartData, UniverseFault};
    let view = View::build(netlist);
    // scope effect + mechanism tables
    struct ScopeInfo {
        prefix: String,
        ns: String,
        effects: Vec<(String, String)>,           // (path, expr)
        mechs: Vec<(String, Vec<String>, Option<String>)>, // (handle, detects, detected_when)
    }
    let scopes: Vec<ScopeInfo> = model
        .scopes
        .iter()
        .map(|s| ScopeInfo {
            prefix: s.path.clone(),
            ns: s.ns.clone(),
            effects: s
                .goals
                .iter()
                .flat_map(|g| g.effects.iter().map(move |e| (format!("{}.{}", g.path, e.name), e.expr.clone())))
                .collect(),
            mechs: s
                .mechanisms
                .iter()
                .map(|m| (m.handle.clone(), m.detects.clone(), m.detected_when.clone()))
                .collect(),
        })
        .collect();
    let scope_idx_of = |owner: &Option<String>| -> Option<usize> {
        let key = owner.clone().unwrap_or_default();
        scopes.iter().position(|s| s.prefix == key).or_else(|| scopes.iter().position(|s| s.prefix.is_empty()))
    };
    let mut universe: Vec<UniverseFault> = Vec::new();
    for part in &model.parts {
        if matches!(part.data, PartData::Waived { .. }) {
            continue;
        }
        let Some(si) = scope_idx_of(&part.parent) else { continue };
        let fit = match &part.data {
            PartData::Handbook { fit, .. } => *fit,
            _ => None,
        };
        let pins = view.pins_of.get(&part.instance).cloned().unwrap_or_default();
        // modes
        let mut modes: Vec<(String, Vec<String>)> = Vec::new();
        if pins.len() == 2 {
            modes.push(("short".into(), vec![format!("{}.{}", part.instance, pins[0]), format!("{}.{}", part.instance, pins[1])]));
            modes.push(("open".into(), vec![part.instance.clone()]));
        } else if pins.len() > 2 {
            for p in &pins {
                modes.push(("open_pin".into(), vec![format!("{}.{}", part.instance, p)]));
            }
        }
        // declared behavioral failure states: listed, need the hook
        if let PartData::Behavioral { failure_states, .. } = &part.data {
            for i in 0..*failure_states {
                universe.push(UniverseFault {
                    scope: scopes[si].prefix.clone(),
                    part: part.instance.clone(),
                    mode: "state".into(),
                    targets: vec![format!("failure_state[{i}]")],
                    ran: false,
                    fired: vec![],
                    detected: vec![],
                    false_alarm: false,
                    weight_fit: None,
                    note: Some("needs the vendor-model failure-state hook".into()),
                });
            }
        }
        let n_modes = modes.len().max(1);
        for (mode, targets) in modes {
            let weight = fit.map(|f| f / n_modes as f64);
            let mut uf = UniverseFault {
                scope: scopes[si].prefix.clone(),
                part: part.instance.clone(),
                mode: mode.clone(),
                targets: targets.clone(),
                ran: false,
                fired: vec![],
                detected: vec![],
                false_alarm: false,
                weight_fit: weight,
                note: None,
            };
            // mutate + solve
            let mut faulted = netlist.clone();
            let mut alias: HashMap<String, String> = HashMap::new();
            let res = match mode.as_str() {
                "short" => apply_short(&mut faulted, &view, &targets[0], &targets[1]).map(|a| {
                    if let Some((from, to)) = a {
                        alias.insert(from, to);
                    }
                }),
                "open" | "open_pin" => apply_open(&mut faulted, &view, &targets[0]),
                _ => unreachable!(),
            };
            if let Err(e) = res {
                uf.note = Some(format!("mutation failed: {e}"));
                universe.push(uf);
                continue;
            }
            let volts = match solve(&faulted) {
                Ok(v) => v,
                Err(e) => {
                    uf.ran = true;
                    uf.note = Some(format!("did not converge ({e})"));
                    universe.push(uf);
                    continue;
                }
            };
            let sc = &scopes[si];
            let mut errs = Vec::new();
            for (path, expr) in &sc.effects {
                match eval_effect(expr, &sc.prefix, &sc.ns, &view, &alias, &volts) {
                    Ok(true) => uf.fired.push(path.clone()),
                    Ok(false) => {}
                    Err(e) => errs.push(format!("{path}: {e}")),
                }
            }
            for (handle, _detects, dw) in &sc.mechs {
                if let Some(pred) = dw {
                    match eval_effect(pred, &sc.prefix, &sc.ns, &view, &alias, &volts) {
                        Ok(true) => uf.detected.push(handle.clone()),
                        Ok(false) => {}
                        Err(e) => errs.push(format!("detected_when {handle}: {e}")),
                    }
                }
            }
            uf.ran = true;
            uf.false_alarm = uf.fired.is_empty() && !uf.detected.is_empty();
            if !errs.is_empty() {
                uf.note = Some(errs.join("; "));
            }
            universe.push(uf);
        }
    }
    // measured DC per mechanism: over dangerous faults whose fired
    // effects intersect the mechanism's detects list.
    for (si, s) in model.scopes.iter_mut().enumerate() {
        let sc = &scopes[si];
        let _ = sc;
        for m in s.mechanisms.iter_mut() {
            if m.detected_when.is_none() {
                m.measured_note = Some("no detected_when predicate — measured DC impossible".into());
                continue;
            }
            let short_effect = |full: &str| full.rsplit('.').next().unwrap_or(full).to_string();
            let relevant: Vec<&UniverseFault> = universe
                .iter()
                .filter(|u| u.scope == s.path && u.ran && u.fired.iter().any(|f| m.detects.contains(&short_effect(f))))
                .collect();
            if relevant.is_empty() {
                m.measured_note = Some("no universe fault produced a detected effect class — nothing to measure".into());
                continue;
            }
            let all_weighted = relevant.iter().all(|u| u.weight_fit.is_some());
            let w = |u: &UniverseFault| if all_weighted { u.weight_fit.unwrap() } else { 1.0 };
            let total: f64 = relevant.iter().map(|u| w(u)).sum();
            let det: f64 = relevant.iter().filter(|u| u.detected.contains(&m.handle)).map(|u| w(u)).sum();
            m.measured_dc = Some(det / total);
            m.measured_note = Some(format!(
                "{} dangerous fault(s), {} basis (equal mode split{})",
                relevant.len(),
                if all_weighted { "λ-weighted" } else { "count" },
                if all_weighted { " — mode fractions are unmeasured data" } else { "" },
            ));
        }
    }
    model.universe = universe;
}
