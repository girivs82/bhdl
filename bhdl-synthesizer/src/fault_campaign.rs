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
    /// instance name → pin names in DEFINITION order (the entity
    /// author's ordering — the package pin numbering for real parts).
    /// Adjacency for pin-bridge faults is consecutive pins in this
    /// order: an ORDERING approximation of package adjacency
    /// (geometric adjacency needs footprint pad coordinates).
    pins_ordered: HashMap<String, Vec<String>>,
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
        let mut pins_ordered: HashMap<String, Vec<String>> = HashMap::new();
        for (_, i) in n.instances.iter() {
            let Some(def) = n.modules.get(i.definition) else { continue };
            let ordered: Vec<String> = def
                .pins
                .iter()
                .filter_map(|pid| n.pins.get(*pid))
                .filter(|p| !p.is_virtual)
                .map(|p| p.name.clone())
                .filter(|pn| pin_net.contains_key(&(i.name.clone(), pn.clone())))
                .collect();
            pins_ordered.insert(i.name.clone(), ordered);
        }
        View { inst, pin_net, nets, pins_of, pins_ordered }
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

/// `force(PIN, <volts>)` — the failed pin actively drives a voltage:
/// the pin's net becomes a Power-classed net, which the spice converter
/// energises with an ideal source (the same mechanism as a declared
/// board rail).
fn apply_force(n: &mut Netlist, view: &View, inst: &str, pin: &str, volts: f64) -> Result<(), String> {
    let net_name = view
        .pin_net
        .get(&(inst.to_string(), pin.to_string()))
        .ok_or_else(|| format!("force: {inst}.{pin} has no net"))?
        .clone();
    let mut hit = false;
    for (_, net) in n.nets.iter_mut() {
        if net.name.as_deref() == Some(net_name.as_str()) {
            net.net_class = bhdl_netlist::NetClass::Power { voltage: volts, current: None };
            hit = true;
        }
    }
    if hit { Ok(()) } else { Err(format!("force: net '{net_name}' not found")) }
}

/// `drift(part, ±pct)`: scale the part's value attribute (resistance /
/// capacitance / inductance) by 1 + pct/100 — the classic
/// beyond-tolerance parametric fault. The numeric prefix of the
/// attribute is scaled in place; the unit suffix is preserved.
fn apply_drift(n: &mut Netlist, view: &View, inst: &str, pct_str: &str) -> Result<(), String> {
    let pct: f64 = pct_str
        .trim()
        .trim_end_matches('%')
        .parse()
        .map_err(|_| format!("drift magnitude '{pct_str}' is not ±<number>%"))?;
    let factor = 1.0 + pct / 100.0;
    let iid = *view.inst.get(inst).ok_or_else(|| format!("drift: instance '{inst}' not found"))?;
    let Some(instance) = n.instances.get_mut(iid) else { return Err("instance vanished".into()) };
    // Scale EVERY value-carrying attribute: different consumers read
    // different keys (the converter may use `value` while sign-off reads
    // `resistance`) — scaling only one leaves the solve on the healthy
    // number.
    let mut scaled_any = false;
    for key in ["resistance", "capacitance", "inductance", "value"] {
        if let Some(v) = instance.attributes.get(key).cloned() {
            let end = v.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(v.len());
            if let Ok(num) = v[..end].parse::<f64>() {
                let scaled = format!("{}{}", num * factor, &v[end..]);
                instance.attributes.insert(key.to_string(), scaled);
                scaled_any = true;
            }
        }
    }
    if scaled_any {
        Ok(())
    } else {
        Err(format!("drift: '{inst}' has no numeric resistance/capacitance/inductance/value attribute"))
    }
}

/// Parse a duration string ("10ms", "500us", "1s") to seconds.
fn parse_duration_s(v: &str) -> Option<f64> {
    let v = v.trim();
    let end = v.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(v.len());
    let num: f64 = v[..end].parse().ok()?;
    match v[end..].trim() {
        "s" | "" => Some(num),
        "ms" => Some(num * 1e-3),
        "us" | "µs" => Some(num * 1e-6),
        "ns" => Some(num * 1e-9),
        "m" | "min" => Some(num * 60.0),
        // proof-test intervals are declared in hours/days — without
        // these an `interval=1h` silently parsed as None and became a
        // ZERO budget in the FTTI check (wrongly passing)
        "h" => Some(num * 3600.0),
        "d" => Some(num * 86400.0),
        _ => None,
    }
}

/// Execute a vendor failure-state behavior on the faulted netlist:
/// `open(PIN)` | `short(PIN_A,PIN_B)` | `force(PIN, <voltage>)`, pins
/// relative to `inst`. Returns the net-alias map contribution.
fn apply_state_behavior(
    n: &mut Netlist,
    view: &View,
    inst: &str,
    behavior: &str,
    alias: &mut HashMap<String, String>,
) -> Result<(), String> {
    let b = behavior.trim();
    let open_paren = b.find('(').ok_or_else(|| format!("behavior '{b}' is not fn(args)"))?;
    let kind = b[..open_paren].trim();
    let args: Vec<String> = b[open_paren + 1..]
        .trim_end_matches(')')
        .split(',')
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .collect();
    match (kind, args.len()) {
        ("open", 1) => apply_open(n, view, &format!("{inst}.{}", args[0])),
        ("short", 2) => apply_short(n, view, &format!("{inst}.{}", args[0]), &format!("{inst}.{}", args[1])).map(|a| {
            if let Some((from, to)) = a {
                alias.insert(from, to);
            }
        }),
        ("force", 2) => {
            let v = args[1]
                .trim_end_matches('V')
                .parse::<f64>()
                .map_err(|_| format!("force voltage '{}' is not a number", args[1]))?;
            apply_force(n, view, inst, &args[0], v)
        }
        _ => Err(format!("behavior '{b}' not one of open(P)|short(A,B)|force(P,V)")),
    }
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

/// One pin-level symptom drive of a chip-internal transient fault: the
/// net is HELD at `level_v` from t=0 to `t_end_s` and released after.
/// A state with several drives is ONE fault (one λ) whose correlated
/// multi-pin symptom vector plays out together — never a multi-point
/// fault (that would wrongly discount a first-order λ to a product).
#[derive(Debug, Clone)]
pub struct PinDrive {
    pub net: String,
    pub level_v: f64,
    pub t_end_s: f64,
}

/// Time-domain solver: (faulted netlist, duration_s, fault drives) →
/// (sample times, net name → voltage series). The caller supplies the
/// engine (bhdl-spice transient with the HEALTHY operating point as
/// initial conditions — the fault itself is the stimulus); the
/// campaign only reads the trace. Empty `drives` = pure relaxation
/// (the FTTI measurement).
pub type TranSolver<'a> =
    dyn Fn(&Netlist, f64, &[PinDrive]) -> Result<(Vec<f64>, HashMap<String, Vec<f64>>), String> + 'a;

/// PDN-contract recheck: (mutated netlist) → the list of VIOLATED
/// domain contracts as (`<instance>.<domain>`, detail). The caller
/// supplies the engine (the CLI's Z(f) mask sweep + droop transient —
/// the SAME checks that discharge `assume pdn(...)` on the healthy
/// board). The campaign invokes it for CAPACITOR open/drift faults,
/// which are invisible to the DC operating point (a cap is an open at
/// DC) yet can defeat the dynamic contract the safety case leans on —
/// without this, their FIT weight lands silently in the safe bucket.
pub type PdnCheck<'a> = dyn Fn(&Netlist) -> Vec<(String, String)> + 'a;

/// FAULT-AT-BOOT recheck: (mutated netlist) → the power-up timeline's
/// VIOLATED declarations (the engine's Sev::Error finding texts). The
/// DC campaign classifies faults at the SETTLED operating point — a
/// fault that breaks the START-UP contract is invisible there: a
/// PG-chain pull-up open never enables the downstream rail, yet the
/// DC solve (which does not model enable gating) shows that rail
/// healthy. The caller supplies the engine (the CLI wraps the PWL
/// power-up simulation); the campaign runs it for every DC-BENIGN
/// fault row and fires the synthetic effect `boot:<ref>` on NEW
/// violations vs the healthy baseline.
pub type BootCheck<'a> = dyn Fn(&Netlist) -> Vec<String> + 'a;

/// Split a behavior string into DC mutation ops and transient pulse
/// ops. `pulse(PIN, <V>, <duration>)` is transient; everything else is
/// applied as a permanent mutation. Several ';'-separated ops are ONE
/// fault's symptom vector.
fn split_behavior_ops(behavior: &str) -> (Vec<String>, Vec<(String, f64, f64)>) {
    let mut dc = Vec::new();
    let mut pulses = Vec::new();
    for op in behavior.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let is_pulse = op.starts_with("pulse");
        if !is_pulse {
            dc.push(op.to_string());
            continue;
        }
        let args: Vec<&str> = op
            .trim_start_matches("pulse")
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .split(',')
            .map(str::trim)
            .collect();
        if args.len() == 3 {
            let v = args[1].trim_end_matches('V').parse::<f64>();
            let d = parse_duration_s(args[2]);
            if let (Ok(v), Some(d)) = (v, d) {
                pulses.push((args[0].to_string(), v, d));
                continue;
            }
        }
        // malformed pulse: surface as a DC op so the DC path errors
        // loudly instead of silently dropping the symptom
        dc.push(op.to_string());
    }
    (dc, pulses)
}

/// Outcome of running a transient (pulse-symptom) failure state.
struct TransientOutcome {
    fired: Vec<String>,
    /// (mechanism handle, measured first-crossing time of detected_when)
    detected_ext: Vec<(String, f64)>,
    /// vendor-declared internal detection: Some(Some(latency_s)) with
    /// timing, Some(None) declared without timing, None = not declared
    detected_int: Option<Option<f64>>,
    note: String,
}

/// Run one pulse-symptom state: apply any DC ops, drive the pulse nets,
/// classify over the WHOLE trace (a transient effect that self-clears
/// is still dangerous while asserted). External detection is the
/// measured FIRST CROSSING of detected_when (a real monitor latches —
/// stated); internal detection is the vendor's declaration.
#[allow(clippy::too_many_arguments)]
fn run_transient_state(
    netlist: &Netlist,
    view: &View,
    prefix: &str,
    ns: &str,
    effects: &[(String, String)],
    mechs: &[(String, Option<String>)],
    inst: &str,
    dc_ops: &[String],
    pulses: &[(String, f64, f64)],
    internal_detection: Option<&str>,
    tran: &TranSolver,
    within_s: Option<f64>,
    // aliases from mutations the CALLER already applied to `netlist`
    // (the latent probe co-injects a DC fault before the transient) —
    // merged so predicates and drive nets follow those merges too
    pre_alias: &HashMap<String, String>,
) -> Result<TransientOutcome, String> {
    let mut faulted = netlist.clone();
    let mut alias: HashMap<String, String> = pre_alias.clone();
    for op in dc_ops {
        apply_state_behavior(&mut faulted, view, inst, op, &mut alias)?;
    }
    let max_dur = pulses.iter().map(|p| p.2).fold(0.0f64, f64::max);
    let duration = within_s.map(|w| (2.0 * w).max(4.0 * max_dur)).unwrap_or(10.0 * max_dur);
    let drives: Vec<PinDrive> = pulses
        .iter()
        .map(|(pin, v, d)| {
            let net = view
                .pin_net
                .get(&(inst.to_string(), pin.clone()))
                .cloned()
                .ok_or_else(|| format!("pulse pin '{inst}.{pin}' is not connected"))?;
            let net = alias.get(&net).cloned().unwrap_or(net);
            Ok(PinDrive { net, level_v: *v, t_end_s: *d })
        })
        .collect::<Result<_, String>>()?;
    let (times, traces) = tran(&faulted, duration, &drives)?;
    if times.len() < 2 {
        return Err("transient returned <2 samples".into());
    }
    let sample = |k: usize| -> HashMap<String, f64> {
        traces
            .iter()
            .map(|(name, vs)| (name.clone(), vs.get(k).copied().unwrap_or(f64::NAN)))
            .collect()
    };
    let n = times.len();
    let mut fired: Vec<String> = Vec::new();
    let mut fired_span: Option<(f64, f64)> = None;
    for (path, expr) in effects {
        let mut first: Option<usize> = None;
        let mut last: Option<usize> = None;
        for k in 0..n {
            if let Ok(true) = eval_effect(expr, prefix, ns, view, &alias, &sample(k)) {
                if first.is_none() {
                    first = Some(k);
                }
                last = Some(k);
            }
        }
        if let (Some(a), Some(b)) = (first, last) {
            fired.push(path.clone());
            let span = (times[a], times[b]);
            fired_span = Some(match fired_span {
                Some((x, y)) => (x.min(span.0), y.max(span.1)),
                None => span,
            });
        }
    }
    let mut detected_ext: Vec<(String, f64)> = Vec::new();
    for (handle, dw) in mechs {
        let Some(pred) = dw else { continue };
        for k in 0..n {
            if let Ok(true) = eval_effect(pred, prefix, ns, view, &alias, &sample(k)) {
                detected_ext.push((handle.clone(), times[k]));
                break;
            }
        }
    }
    let detected_int = internal_detection.map(|v| {
        let t = v.trim().trim_matches('"');
        if t.eq_ignore_ascii_case("yes") || t.eq_ignore_ascii_case("true") {
            None
        } else {
            parse_duration_s(t)
        }
    });
    let mut notes = vec![format!(
        "transient: {} pin drive(s), longest {:.1}µs, simulated {:.1}µs",
        pulses.len(),
        max_dur * 1e6,
        duration * 1e6
    )];
    if let Some((a, b)) = fired_span {
        notes.push(format!("effect asserted {:.1}µs..{:.1}µs of the trace", a * 1e6, b * 1e6));
    }
    for (h, t) in &detected_ext {
        notes.push(format!("{h} crossed at {:.1}µs (measured)", t * 1e6));
    }
    match &detected_int {
        Some(Some(l)) => notes.push(format!("vendor declares INTERNAL detection, latency {:.1}µs", l * 1e6)),
        Some(None) => notes.push("vendor declares INTERNAL detection (no timing data — FTTI unverifiable via this path)".into()),
        None => {}
    }
    Ok(TransientOutcome {
        fired,
        detected_ext,
        detected_int,
        note: notes.join("; "),
    })
}

/// Run every declared fault. Marks each fault run (or notes exactly why
/// it could not run), records fired effects + whether the expectation
/// was met, and clears the corresponding FAULT_UNRUN gaps.
/// `tran` (when given) upgrades the FTTI check from declared budgets to
/// a MEASURED detection settle time.
pub fn run_declared_faults(
    netlist: &Netlist,
    model: &mut SafetyModel,
    solve: &Solver,
    tran: Option<&TranSolver>,
) -> (usize, usize) {
    let view = View::build(netlist);
    // (instance, state name) → behavior (None = declared without one)
    let state_behaviors: HashMap<(String, String), Option<String>> = model
        .parts
        .iter()
        .filter_map(|p| match &p.data {
            bhdl_common::safety::PartData::Behavioral { states, .. } => Some(
                states
                    .iter()
                    .map(|st| ((p.instance.clone(), st.name.clone()), st.behavior.clone()))
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .flatten()
        .collect();
    // (instance, state name) → vendor internal-detection declaration
    let state_internal: HashMap<(String, String), Option<String>> = model
        .parts
        .iter()
        .filter_map(|p| match &p.data {
            bhdl_common::safety::PartData::Behavioral { states, .. } => Some(
                states
                    .iter()
                    .map(|st| ((p.instance.clone(), st.name.clone()), st.internal_detection.clone()))
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .flatten()
        .collect();
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
    let scopes_mechs: Vec<Vec<(String, Vec<String>, Option<String>, Option<String>, Option<String>)>> = model
        .scopes
        .iter()
        .map(|s| {
            s.mechanisms
                .iter()
                .map(|m| (m.handle.clone(), m.detects.clone(), m.detected_when.clone(), m.interval.clone(), m.latency.clone()))
                .collect()
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
                "short" | "open" | "state" | "drift" => {}
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
                "state" => {
                    // vendor failure state: execute its declared behavior
                    let inst = f.targets.first().cloned().unwrap_or_default();
                    let sname = f.targets.get(1).map(|s| s.trim_matches('"').to_string()).unwrap_or_default();
                    match state_behaviors.get(&(inst.clone(), sname.clone())) {
                        Some(Some(beh)) => {
                            let (dc_ops, pulses) = split_behavior_ops(beh);
                            if !pulses.is_empty() {
                                // TRANSIENT state: classify over the trace,
                                // not the endpoint (post-pulse steady state
                                // is healthy by construction).
                                let within_s = f.within.as_deref().and_then(parse_duration_s);
                                let internal = state_internal.get(&(inst.clone(), sname.clone())).cloned().flatten();
                                match tran {
                                    None => {
                                        f.note = Some("transient behavior (pulse) needs a time-domain engine — not run".into());
                                    }
                                    Some(tr) => match run_transient_state(
                                        netlist, &view, prefix, ns, effects,
                                        &scopes_mechs[si].iter().map(|(h, _, dw, _, _)| (h.clone(), dw.clone())).collect::<Vec<_>>(),
                                        &inst, &dc_ops, &pulses, internal.as_deref(), tr, within_s,
                                        &HashMap::new(),
                                    ) {
                                        Err(e) => {
                                            f.run = true;
                                            f.expectation_met = None;
                                            f.note = Some(format!("transient state did not run to a verdict: {e}"));
                                            ran += 1;
                                        }
                                        Ok(out) => {
                                            f.run = true;
                                            ran += 1;
                                            f.fired = out.fired.clone();
                                            let expect_full = &f.expect;
                                            let met = out.fired.iter().any(|p| p == expect_full || p.ends_with(&format!(".{expect_full}")) || expect_full.ends_with(p.as_str()));
                                            f.expectation_met = Some(met);
                                            if !met {
                                                mismatched += 1;
                                            }
                                            // FTTI: min over (external crossing
                                            // + that monitor's declared chip
                                            // latency + interval) and (vendor
                                            // internal latency, end-to-end)
                                            if let Some(w) = within_s {
                                                let mut best: Option<f64> = None;
                                                for (h, t_ext) in &out.detected_ext {
                                                    let m = scopes_mechs[si].iter().find(|(mh, ..)| mh == h);
                                                    let extra = m
                                                        .map(|(_, _, _, i, l)| {
                                                            i.as_deref().and_then(parse_duration_s).unwrap_or(0.0)
                                                                + l.as_deref().and_then(parse_duration_s).unwrap_or(0.0)
                                                        })
                                                        .unwrap_or(0.0);
                                                    let tot = t_ext + extra;
                                                    if best.map(|b| tot < b).unwrap_or(true) {
                                                        best = Some(tot);
                                                    }
                                                }
                                                if let Some(Some(l)) = &out.detected_int {
                                                    if best.map(|b| *l < b).unwrap_or(true) {
                                                        best = Some(*l);
                                                    }
                                                }
                                                f.timing_met = match best {
                                                    Some(b) => Some(b <= w),
                                                    None if matches!(out.detected_int, Some(None)) => None, // detected, timing unknowable
                                                    None => Some(false), // never detected
                                                };
                                            }
                                            f.note = Some(out.note);
                                        }
                                    },
                                }
                                continue;
                            }
                            apply_state_behavior(&mut faulted, &view, &inst, beh, &mut alias)
                        }
                        Some(None) => Err(format!("failure_state '{sname}' declares no behavior=\"open(P)|short(A,B)|force(P,V)|pulse(P,V,T)\" — the vendor model must say what the state DOES")),
                        None => Err(format!("no failure_state '{sname}' on {inst}")),
                    }
                }
                "drift" => {
                    if f.targets.len() != 2 {
                        Err(format!("drift needs (part, ±pct), got {} args", f.targets.len()))
                    } else {
                        apply_drift(&mut faulted, &view, &f.targets[0], &f.targets[1])
                    }
                }
                _ => unreachable!(),
            };
            if let Err(e) = res {
                f.note = Some(format!("not run: {e}"));
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
            // FTTI: `within T` claims DETECTION inside T. Detection
            // presence comes from the mechanisms' detected_when on the
            // faulted point. The TIME argument, strongest first:
            //   1. MEASURED — transient solve of the FAULTED board
            //      relaxing from the HEALTHY operating point (the fault
            //      is the stimulus); measured settle time of the
            //      detection predicate + the mechanism's declared
            //      test interval ≤ within. The declared latency claim
            //      is SUPERSEDED by the measurement.
            //   2. DECLARED — the mechanism's interval+latency budget
            //      (when no transient engine is available, stated).
            //   3. UNVERIFIABLE — neither, stated, never assumed.
            if let Some(w) = &f.within {
                let ftti = parse_duration_s(w);
                let mut detecting: Vec<(f64, bool)> = Vec::new(); // (budget_s, budget_known)
                for (handle, _d, dw, interval, latency) in &scopes_mechs[si] {
                    let _ = handle;
                    if let Some(pred) = dw {
                        if let Ok(true) = eval_effect(pred, prefix, ns, &view, &alias, &volts) {
                            let b_int = interval.as_deref().and_then(parse_duration_s).unwrap_or(0.0);
                            let b_lat = latency.as_deref().and_then(parse_duration_s);
                            match b_lat {
                                Some(l) => detecting.push((b_int + l, true)),
                                None if interval.is_some() => detecting.push((b_int, true)),
                                None => detecting.push((0.0, false)),
                            }
                        }
                    }
                }
                f.timing_met = match (ftti, detecting.is_empty()) {
                    (None, _) => {
                        f.note = Some(format!("within '{w}' is not a parsable duration"));
                        None
                    }
                    (Some(_), true) => Some(false), // never detected ⇒ FTTI missed
                    (Some(t), false) => {
                        let mut verdict: Option<bool> = None;
                        let mut tnote: Option<String> = None;
                        if let Some(tr) = tran {
                            match tr(&faulted, 2.0 * t, &[]) {
                                Ok((times, traces)) if times.len() >= 2 => {
                                    // per-mechanism measured settle time of
                                    // detected_when; keep the best
                                    // (t_detect + declared test interval)
                                    let n = times.len();
                                    let sample = |k: usize| -> HashMap<String, f64> {
                                        traces
                                            .iter()
                                            .map(|(name, vs)| (name.clone(), vs.get(k).copied().unwrap_or(f64::NAN)))
                                            .collect()
                                    };
                                    // The measurement and the declared latency
                                    // describe DIFFERENT segments of the same
                                    // chain: the transient sees the BOARD path
                                    // to the detector's input pin; everything
                                    // inside the chip (comparator prop delay,
                                    // deglitch, ADC+firmware) is a black box
                                    // the solve structurally cannot see — the
                                    // declared latency IS the model for it.
                                    // So the terms COMPOSE, never supersede:
                                    //   t_board(measured) + latency(declared,
                                    //   chip-internal) + interval(declared).
                                    let mut best: Option<f64> = None;
                                    let mut best_detect = 0.0f64;
                                    let mut best_chip = 0.0f64;
                                    for (_h2, _d2, dw, interval, latency) in &scopes_mechs[si] {
                                        let Some(pred) = dw else { continue };
                                        let holds = |k: usize| {
                                            eval_effect(pred, prefix, ns, &view, &alias, &sample(k)).unwrap_or(false)
                                        };
                                        if !holds(n - 1) {
                                            continue; // never ends detected
                                        }
                                        let mut k = n - 1;
                                        while k > 0 && holds(k - 1) {
                                            k -= 1;
                                        }
                                        let t_detect = times[k];
                                        let b_int = interval.as_deref().and_then(parse_duration_s).unwrap_or(0.0);
                                        let b_lat = latency.as_deref().and_then(parse_duration_s).unwrap_or(0.0);
                                        let total = t_detect + b_lat + b_int;
                                        if best.map(|b| total < b).unwrap_or(true) {
                                            best = Some(total);
                                            best_detect = t_detect;
                                            best_chip = b_lat + b_int;
                                        }
                                    }
                                    match best {
                                        Some(total) => {
                                            verdict = Some(total <= t);
                                            let dt = times[1] - times[0];
                                            tnote = Some(format!(
                                                "FTTI MEASURED: board path settles {:.4}ms after the fault (transient from the healthy operating point, step {:.4}ms — resolution = one step) + {:.4}ms declared chip-internal latency+interval = {:.4}ms total",
                                                best_detect * 1e3,
                                                dt * 1e3,
                                                best_chip * 1e3,
                                                total * 1e3
                                            ));
                                        }
                                        None => {
                                            verdict = Some(false);
                                            tnote = Some(format!(
                                                "FTTI MEASURED: no detection predicate settles TRUE within {:.4}ms of the fault (transient)",
                                                2.0 * t * 1e3
                                            ));
                                        }
                                    }
                                }
                                Ok(_) => {
                                    tnote = Some("transient returned <2 samples — declared budget used".into());
                                }
                                Err(e) => {
                                    tnote = Some(format!("transient unavailable ({e}) — declared budget used"));
                                }
                            }
                        }
                        if verdict.is_none() {
                            verdict = if detecting.iter().any(|(_, known)| *known) {
                                let best = detecting.iter().filter(|(_, k)| *k).map(|(b, _)| *b).fold(f64::INFINITY, f64::min);
                                Some(best <= t)
                            } else {
                                f.note = Some("FTTI unverifiable: detecting mechanism declares no interval/latency budget (and no transient measurement was possible)".into());
                                None
                            };
                        }
                        if let Some(tn) = tnote {
                            f.note = Some(match f.note.take() {
                                Some(existing) => format!("{existing}; {tn}"),
                                None => tn,
                            });
                        }
                        verdict
                    }
                };
            }
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
                (true, Some(true)) if f.timing_met == Some(false) => format!(
                    "campaign ran: {} fired and is detected, but the FTTI check FAILED — never detected or the mechanism's declared interval+latency exceeds within {}",
                    f.expect,
                    f.within.as_deref().unwrap_or("?")
                ),
                (true, Some(true)) if f.within.is_some() && f.timing_met.is_none() => format!(
                    "campaign ran: expectation met, FTTI UNVERIFIABLE — {}",
                    f.note.as_deref().unwrap_or("no timing data")
                ),
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
/// multi-pin parts get a per-pin `open` plus ADJACENT-pin shorts
/// (solder bridges). Adjacency comes from `geo_adjacency` when the
/// caller resolved the part's footprint pad geometry (pads within
/// 1.5× the package's minimum pad spacing — real neighbours, so a
/// SOIC's 4↔5 "consecutive" pair is correctly ABSENT); parts without
/// resolvable geometry fall back to consecutive-pins-in-definition-
/// order, labelled as the approximation it is. Parts with declared
/// behavioral failure states run the VENDOR's states instead of the
/// generic guesses (bridges still apply — they are board-side).
/// Each fault is classified on the faulted operating point:
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
pub fn run_universe(
    netlist: &Netlist,
    model: &mut SafetyModel,
    solve: &Solver,
    geo_adjacency: &HashMap<String, Vec<(String, String)>>,
    tran: Option<&TranSolver>,
    pdn: Option<&PdnCheck>,
    boot: Option<&BootCheck>,
) {
    use bhdl_common::safety::{PartData, UniverseFault};
    let view = View::build(netlist);
    // ── fault-at-boot plumbing ──────────────────────────────────────
    // A violation's identity is its "<owner>.<domain>"/rail prefix
    // (the text before ':'): the numbers inside a finding change under
    // fault, the subject does not. A subject already violated HEALTHY
    // is the design's problem, not the fault's — suppressed (coarse
    // per-subject suppression, stated).
    let boot_key = |t: &str| -> String { t.split(':').next().unwrap_or(t).trim().to_string() };
    let boot_baseline: std::collections::HashSet<String> = match boot {
        Some(chk) => chk(netlist).iter().map(|t| boot_key(t)).collect(),
        None => std::collections::HashSet::new(),
    };
    let boot_recheck = |faulted: &Netlist, uf: &mut UniverseFault| {
        let Some(chk) = boot else { return };
        for t in chk(faulted) {
            let key = boot_key(&t);
            if boot_baseline.contains(&key) {
                continue;
            }
            uf.fired.push(format!("boot:{key}"));
            let n = format!(
                "BOOT: power-up under this fault violates a declared contract: {t} — dangerous at start-up; the settled DC solve cannot see an enable/ordering failure (stated)"
            );
            uf.note = Some(match uf.note.take() { Some(p) => format!("{p}; {n}"), None => n });
        }
    };
    // ── PDN recheck plumbing (capacitor open/drift faults) ──────────
    // A capacitor by the same broadened detection the sizing engines
    // use: module Cap|Capacitor OR a capacitance attribute.
    let is_capacitor = |inst_name: &str| -> bool {
        view.inst
            .get(inst_name)
            .and_then(|id| netlist.instances.get(*id))
            .map(|i| {
                netlist
                    .modules
                    .get(i.definition)
                    .map(|m| matches!(m.name.as_str(), "Cap" | "Capacitor"))
                    .unwrap_or(false)
                    || i.attributes.contains_key("capacitance")
            })
            .unwrap_or(false)
    };
    // `assume pdn(<inst>.<domain>)` refs the safety case consumes — a
    // violated one under fault is DANGEROUS to every goal leaning on
    // it; a violation the case never consumed is a design-only note.
    let pdn_assumed: std::collections::HashSet<String> = model
        .scopes
        .iter()
        .flat_map(|s| s.assumptions.iter())
        .filter_map(|a| a.id.strip_prefix("pdn:").map(str::to_string))
        .collect();
    // Baseline ONCE on the healthy board: a contract already violated
    // healthy carries its own AouViolated gap — only NEW violations
    // are the fault's doing.
    let pdn_baseline: std::collections::HashSet<String> = match pdn {
        Some(chk) => chk(netlist).into_iter().map(|(aref, _)| aref).collect(),
        None => std::collections::HashSet::new(),
    };
    // Fold NEW violations of the mutated board into a fault row: a
    // consumed contract fires the synthetic effect `pdn:<ref>` (DC
    // monitors cannot see a dynamic violation, so with no mechanism
    // detecting it the row classifies RESIDUAL — exactly the exposure
    // this recheck exists to surface).
    let pdn_recheck = |faulted: &Netlist, uf: &mut UniverseFault| {
        let Some(chk) = pdn else { return };
        for (aref, detail) in chk(faulted) {
            if pdn_baseline.contains(&aref) {
                continue;
            }
            if pdn_assumed.contains(&aref) {
                uf.fired.push(format!("pdn:{aref}"));
                let n = format!(
                    "PDN contract VIOLATED under this fault: {detail} — dynamic violation, invisible to DC monitors (a supervisor would only see it during the transient itself)"
                );
                uf.note = Some(match uf.note.take() { Some(p) => format!("{p}; {n}"), None => n });
            } else {
                let n = format!(
                    "PDN contract of {aref} violated under this fault ({detail}) — design-only: the safety case declares no `assume pdn({aref})`, stated"
                );
                uf.note = Some(match uf.note.take() { Some(p) => format!("{p}; {n}"), None => n });
            }
        }
    };
    // (instance, state name) → behavior, for state-mode (re-)application
    let state_behaviors: HashMap<(String, String), Option<String>> = model
        .parts
        .iter()
        .filter_map(|p| match &p.data {
            PartData::Behavioral { states, .. } => Some(
                states
                    .iter()
                    .map(|st| ((p.instance.clone(), st.name.clone()), st.behavior.clone()))
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .flatten()
        .collect();
    // (instance, state name) → vendor internal-detection declaration
    let state_internal: HashMap<(String, String), Option<String>> = model
        .parts
        .iter()
        .filter_map(|p| match &p.data {
            PartData::Behavioral { states, .. } => Some(
                states
                    .iter()
                    .map(|st| ((p.instance.clone(), st.name.clone()), st.internal_detection.clone()))
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .flatten()
        .collect();
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
        let ordered = view.pins_ordered.get(&part.instance).cloned().unwrap_or_else(|| pins.clone());
        // modes. A BEHAVIORAL part's failure modes are the VENDOR'S
        // declared states — never the generic 2-pin guesses — and each
        // state carries its REAL FIT share as the λ weight. A state
        // without a behavior stays unrun with that said.
        //
        // Multi-pin parts additionally get ADJACENT-PIN SHORTS (solder
        // bridges are board-side faults, so they apply to behavioral
        // parts too): consecutive pins in definition order — an
        // ORDERING approximation of package adjacency, stated; the
        // geometric version needs footprint pad coordinates. For
        // behavioral parts the bridge λ is NOT covered by the die's
        // failure-state FITs — weight None, said in the note.
        let mut modes: Vec<(String, Vec<String>, Option<f64>, Option<String>, Option<String>)> = Vec::new();
        let is_behavioral = matches!(part.data, PartData::Behavioral { .. });
        if let PartData::Behavioral { states, .. } = &part.data {
            for st in states {
                modes.push(("state".into(), vec![part.instance.clone(), st.name.clone()], st.fit, st.behavior.clone(), None));
            }
        } else if pins.len() == 2 {
            // Value-carrying parts additionally get parametric DRIFT
            // modes (FMD-91-family mode splits list drift-beyond-
            // tolerance as its own mode next to open/short). The
            // ENDPOINTS are NOT drift — R→∞ IS the open mode and R→0
            // IS the short mode, already carried with their own λ; a
            // "drift to worst case" row would re-solve those identical
            // netlists and double-count λ. Each direction is ONE mode
            // λ-wise, probed at two magnitudes: the part's declared
            // `tolerance` edge (real data when the attribute exists)
            // and the 0.5×/2× convention point (the de-facto FMEDA
            // convention — labelled as convention, not vendor data);
            // the row keeps the WORST-classified probe.
            let attrs = view
                .inst
                .get(&part.instance)
                .and_then(|id| netlist.instances.get(*id))
                .map(|i| &i.attributes);
            let has_value = attrs
                .map(|a| ["resistance", "capacitance", "inductance", "value"].iter().any(|k| a.contains_key(*k)))
                .unwrap_or(false);
            let tol_pct: Option<f64> = attrs
                .and_then(|a| a.get("tolerance"))
                .and_then(|t| t.trim().trim_start_matches('±').trim_end_matches('%').trim().parse::<f64>().ok())
                .filter(|p| *p > 0.0);
            let n = if has_value { 4.0 } else { 2.0 };
            let w = fit.map(|f| f / n);
            modes.push(("short".into(), vec![format!("{}.{}", part.instance, pins[0]), format!("{}.{}", part.instance, pins[1])], w, None, None));
            modes.push(("open".into(), vec![part.instance.clone()], w, None, None));
            if has_value {
                let basis = match tol_pct {
                    Some(t) => format!(
                        "probes: ±{t}% (declared tolerance) and 0.5×/2× (FMEDA convention, not vendor data); λ = equal 4-way mode split"
                    ),
                    None => "probes: 0.5×/2× (FMEDA convention, not vendor data; no tolerance attribute); λ = equal 4-way mode split".to_string(),
                };
                let mut high = vec![part.instance.clone()];
                let mut low = vec![part.instance.clone()];
                if let Some(t) = tol_pct {
                    high.push(format!("+{t}%"));
                    low.push(format!("-{t}%"));
                }
                high.push("+100%".to_string());
                low.push("-50%".to_string());
                modes.push(("drift_high".into(), high, w, None, Some(basis.clone())));
                modes.push(("drift_low".into(), low, w, None, Some(basis)));
            }
        }
        if pins.len() > 2 {
            // Adjacency basis: footprint pad GEOMETRY when the caller
            // resolved it (only pairs whose pads physically neighbour —
            // a bridge spans one gap, not the package body), else the
            // ordering approximation, labelled.
            let geo = geo_adjacency.get(&part.instance);
            let pairs: Vec<(String, String)> = match geo {
                Some(g) => g
                    .iter()
                    .filter(|(a, b)| pins.contains(a) && pins.contains(b))
                    .cloned()
                    .collect(),
                None => ordered.windows(2).map(|w| (w[0].clone(), w[1].clone())).collect(),
            };
            let basis = if geo.is_some() {
                "geometric adjacency (footprint pads)"
            } else {
                "ordering-adjacency approximation — no footprint geometry"
            };
            let n_adj = pairs.len();
            let n_modes = if is_behavioral { n_adj } else { pins.len() + n_adj };
            let w = if is_behavioral { None } else { fit.map(|f| f / n_modes.max(1) as f64) };
            if !is_behavioral {
                for p in &pins {
                    modes.push(("open_pin".into(), vec![format!("{}.{}", part.instance, p)], w, None, None));
                }
            }
            for (pa, pb) in &pairs {
                let note = if is_behavioral {
                    Some(format!(
                        "pin bridge ({}): λ not covered by the die failure states — unweighted",
                        basis
                    ))
                } else {
                    Some(basis.to_string())
                };
                modes.push((
                    "short_adjacent".into(),
                    vec![format!("{}.{}", part.instance, pa), format!("{}.{}", part.instance, pb)],
                    w,
                    None,
                    note,
                ));
            }
        }
        for (mode, targets, weight, behavior, mode_note) in modes {
            let mut uf = UniverseFault {
                scope: scopes[si].prefix.clone(),
                part: part.instance.clone(),
                mode: mode.clone(),
                targets: targets.clone(),
                ran: false,
                fired: vec![],
                detected: vec![],
                false_alarm: false,
                latent: false,
                latent_exposed_fit: 0.0,
                weight_fit: weight,
                note: mode_note,
            };
            // DRIFT rows: one λ mode probed at several magnitudes —
            // classify each probe, keep the WORST (residual > detected-
            // dangerous > false alarm > benign), then bisect for the
            // undetected-dangerous window (the region drift analysis
            // exists to find: drifted enough to violate the goal, not
            // enough to trip the detector — endpoints are usually the
            // EASY cases). Sweep runs only when a probe is dangerous
            // and the scope has a detected_when mechanism; boundaries
            // are bisected to ~1% between grid points.
            if mode == "drift_high" || mode == "drift_low" {
                let sc = &scopes[si];
                let inst = targets[0].clone();
                // classify one magnitude: None = did not converge
                let classify = |pct: f64| -> Option<(Vec<String>, Vec<String>)> {
                    let mut faulted = netlist.clone();
                    let pct_str = format!("{pct:+}%");
                    apply_drift(&mut faulted, &view, &inst, &pct_str).ok()?;
                    let volts = solve(&faulted).ok()?;
                    let alias: HashMap<String, String> = HashMap::new();
                    let mut fired = Vec::new();
                    let mut det = Vec::new();
                    for (path, expr) in &sc.effects {
                        if let Ok(true) = eval_effect(expr, &sc.prefix, &sc.ns, &view, &alias, &volts) {
                            fired.push(path.clone());
                        }
                    }
                    for (handle, _d, dw) in &sc.mechs {
                        if let Some(pred) = dw {
                            if let Ok(true) = eval_effect(pred, &sc.prefix, &sc.ns, &view, &alias, &volts) {
                                det.push(handle.clone());
                            }
                        }
                    }
                    Some((fired, det))
                };
                let severity = |r: &Option<(Vec<String>, Vec<String>)>| -> u8 {
                    match r {
                        Some((f, d)) if !f.is_empty() && d.is_empty() => 3, // residual
                        Some((f, _)) if !f.is_empty() => 2,                // dangerous, detected
                        Some((f, d)) if f.is_empty() && !d.is_empty() => 1, // false alarm
                        Some(_) => 0,
                        None => 0,
                    }
                };
                let probes: Vec<f64> = targets[1..]
                    .iter()
                    .filter_map(|s| s.trim().trim_end_matches('%').parse::<f64>().ok())
                    .collect();
                let mut worst: Option<(u8, f64, Vec<String>, Vec<String>)> = None;
                let mut unconverged = 0usize;
                for &p in &probes {
                    let r = classify(p);
                    if r.is_none() {
                        unconverged += 1;
                    }
                    let sev = severity(&r);
                    let better = worst.as_ref().map(|(ws, ..)| sev > *ws).unwrap_or(true);
                    if better {
                        let (f, d) = r.unwrap_or_default();
                        worst = Some((sev, p, f, d));
                    }
                }
                let (wsev, wpct, wf, wd) = worst.unwrap_or((0, probes.first().copied().unwrap_or(0.0), vec![], vec![]));
                uf.ran = unconverged < probes.len();
                uf.fired = wf;
                uf.detected = wd;
                uf.false_alarm = wsev == 1;
                uf.targets = vec![inst.clone(), format!("{wpct:+}%")];
                let mut notes: Vec<String> = uf.note.take().into_iter().collect();
                if unconverged > 0 {
                    notes.push(format!("{unconverged}/{} probe(s) did not converge", probes.len()));
                }
                if wsev >= 2 {
                    notes.push(format!("worst probe {wpct:+}%"));
                }
                // ── detectability sweep: bisect the boundaries of
                // "dangerous && undetected" over the probed range plus
                // coarse extensions toward (never onto) the endpoint.
                let has_mech = sc.mechs.iter().any(|(_, _, dw)| dw.is_some());
                if wsev >= 2 && has_mech {
                    let mut grid: Vec<f64> = probes.clone();
                    if mode == "drift_high" {
                        grid.extend([300.0, 900.0]);
                    } else {
                        grid.extend([-75.0, -95.0]);
                    }
                    grid.sort_by(|a, b| a.abs().partial_cmp(&b.abs()).unwrap());
                    grid.dedup();
                    let undet = |pct: f64| -> Option<bool> {
                        classify(pct).map(|(f, d)| !f.is_empty() && d.is_empty())
                    };
                    let status: Vec<(f64, Option<bool>)> = grid.iter().map(|&p| (p, undet(p))).collect();
                    // bisect each grid-adjacent transition to ~1/64 of the span
                    let bisect = |mut a: f64, mut b: f64| -> f64 {
                        // invariant: undet(a) != undet(b), both converge
                        for _ in 0..6 {
                            let m = (a + b) / 2.0;
                            match undet(m) {
                                Some(u) if Some(u) == undet(a) => a = m,
                                Some(_) => b = m,
                                None => break,
                            }
                        }
                        (a + b) / 2.0
                    };
                    let mut edges: Vec<(f64, bool)> = Vec::new(); // (boundary pct, entering-undetected?)
                    for w in status.windows(2) {
                        if let ((a, Some(ua)), (b, Some(ub))) = (&w[0], &w[1]) {
                            if ua != ub {
                                edges.push((bisect(*a, *b), *ub));
                            }
                        }
                    }
                    let end = *grid.last().unwrap_or(&wpct);
                    let any_undet = status.iter().any(|(_, u)| *u == Some(true));
                    if any_undet {
                        // report the window(s): entering edges open, leaving edges close
                        let mut windows: Vec<String> = Vec::new();
                        let mut open: Option<f64> = if status.first().and_then(|(_, u)| *u) == Some(true) {
                            Some(status[0].0)
                        } else {
                            None
                        };
                        for (b, entering) in &edges {
                            if *entering {
                                open = Some(*b);
                            } else if let Some(o) = open.take() {
                                windows.push(format!("{o:+.0}%..{b:+.0}%"));
                            }
                        }
                        if let Some(o) = open {
                            windows.push(format!("{o:+.0}%..{end:+.0}% (to probe limit)"));
                        }
                        notes.push(format!("sweep: UNDETECTED dangerous window {}", windows.join(", ")));
                    } else if status.iter().all(|(_, u)| u.is_some()) {
                        notes.push(format!("sweep: dangerous drift detected throughout probed range (to {end:+.0}%)"));
                    }
                }
                if !notes.is_empty() {
                    uf.note = Some(notes.join("; "));
                }
                // capacitance drift is invisible at DC — recheck the
                // dynamic PDN contract at the worst probe magnitude,
                // and the START-UP contract for DC-benign drift
                if is_capacitor(&inst) || (uf.ran && uf.fired.is_empty()) {
                    let mut refaulted = netlist.clone();
                    if apply_drift(&mut refaulted, &view, &inst, &format!("{wpct:+}%")).is_ok() {
                        if is_capacitor(&inst) {
                            pdn_recheck(&refaulted, &mut uf);
                        }
                        if uf.ran && uf.fired.is_empty() {
                            boot_recheck(&refaulted, &mut uf);
                        }
                        if !uf.fired.is_empty() {
                            uf.false_alarm = false;
                        }
                    }
                }
                universe.push(uf);
                continue;
            }
            // TRANSIENT (pulse-symptom) states: classify over the trace
            // — the post-pulse steady state is healthy by construction,
            // so an endpoint solve would call every one of them SAFE.
            if mode == "state" {
                let pulses_present = behavior
                    .as_deref()
                    .map(|b| !split_behavior_ops(b).1.is_empty())
                    .unwrap_or(false);
                if pulses_present {
                    let beh = behavior.as_deref().unwrap();
                    let (dc_ops, pulses) = split_behavior_ops(beh);
                    let sc = &scopes[si];
                    match tran {
                        None => {
                            uf.note = Some("transient behavior (pulse) needs a time-domain engine — not run".into());
                        }
                        Some(tr) => {
                            let internal = state_internal
                                .get(&(targets[0].clone(), targets.get(1).cloned().unwrap_or_default()))
                                .cloned()
                                .flatten();
                            let mech_pairs: Vec<(String, Option<String>)> =
                                sc.mechs.iter().map(|(h, _, dw)| (h.clone(), dw.clone())).collect();
                            match run_transient_state(
                                netlist, &view, &sc.prefix, &sc.ns, &sc.effects, &mech_pairs,
                                &targets[0], &dc_ops, &pulses, internal.as_deref(), tr, None,
                                &HashMap::new(),
                            ) {
                                Err(e) => {
                                    uf.note = Some(format!("transient state did not run to a verdict: {e}"));
                                }
                                Ok(out) => {
                                    uf.ran = true;
                                    uf.fired = out.fired;
                                    uf.detected = out.detected_ext.iter().map(|(h, _)| h.clone()).collect();
                                    if out.detected_int.is_some() {
                                        uf.detected.push("internal (vendor-declared)".to_string());
                                    }
                                    uf.false_alarm = uf.fired.is_empty() && !uf.detected.is_empty();
                                    uf.note = Some(match uf.note.take() {
                                        Some(nn) => format!("{nn}; {}", out.note),
                                        None => out.note,
                                    });
                                }
                            }
                        }
                    }
                    universe.push(uf);
                    continue;
                }
            }
            // mutate + solve
            let mut faulted = netlist.clone();
            let mut alias: HashMap<String, String> = HashMap::new();
            let res = match mode.as_str() {
                "short" | "short_adjacent" => apply_short(&mut faulted, &view, &targets[0], &targets[1]).map(|a| {
                    if let Some((from, to)) = a {
                        alias.insert(from, to);
                    }
                }),
                "open" | "open_pin" => apply_open(&mut faulted, &view, &targets[0]),
                "state" => match &behavior {
                    Some(beh) => apply_state_behavior(&mut faulted, &view, &targets[0], beh, &mut alias),
                    None => Err("failure_state declares no behavior= — the vendor model must say what the state DOES".into()),
                },
                _ => unreachable!(),
            };
            if let Err(e) = res {
                uf.note = Some(match uf.note.take() { Some(n) => format!("{n}; {e}"), None => e });
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
                let e = errs.join("; ");
                uf.note = Some(match uf.note.take() { Some(n) => format!("{n}; {e}"), None => e });
            }
            // an OPEN capacitor is a DC no-op — recheck the dynamic
            // PDN contract on the mutated board (the decap sweep's
            // single-open margin exempts BULK parts, stated there;
            // this is where that exemption gets its honest verdict)
            if mode == "open" && is_capacitor(&part.instance) {
                pdn_recheck(&faulted, &mut uf);
                if !uf.fired.is_empty() {
                    uf.false_alarm = false;
                }
            }
            // FAULT-AT-BOOT: a DC-BENIGN fault may still break the
            // START-UP contract (enable chains, sequencing windows) —
            // run the power-up timeline on the mutated board for every
            // row the settled solve called safe
            if uf.ran && uf.fired.is_empty() {
                boot_recheck(&faulted, &mut uf);
                if !uf.fired.is_empty() {
                    uf.false_alarm = false;
                }
            }
            universe.push(uf);
        }
    }
    // ── LATENT probe (ISO multi-point): a fault on a MECHANISM part
    // that alone is neither dangerous nor annunciated may still defeat
    // detection. Double-inject it with each otherwise-DETECTED dangerous
    // fault; if any detection is lost, the mechanism-part mode is
    // latent. Cost: |candidate mech faults| × |detected dangerous|.
    {
        let mech_parts: std::collections::HashSet<(String, String)> = model
            .scopes
            .iter()
            .flat_map(|s| s.mechanisms.iter().map(move |m| (s.path.clone(), m.instance.clone())))
            .collect();
        let detected_dangerous: Vec<(usize, String, Vec<String>)> = universe
            .iter()
            .enumerate()
            .filter(|(_, u)| u.ran && !u.fired.is_empty() && !u.detected.is_empty())
            .map(|(i, u)| (i, u.mode.clone(), u.targets.clone()))
            .collect();
        let candidates: Vec<usize> = universe
            .iter()
            .enumerate()
            .filter(|(_, u)| {
                u.ran
                    && u.fired.is_empty()
                    && u.detected.is_empty()
                    && mech_parts.contains(&(u.scope.clone(), u.part.clone()))
                    && (matches!(u.mode.as_str(), "short" | "short_adjacent" | "open" | "open_pin" | "drift_high" | "drift_low")
                        // A pulse state is NOT a latent candidate: a
                        // transient cannot be DORMANT — its exposure is
                        // its own pulse width, not a dormancy window.
                        || (u.mode == "state"
                            && state_behaviors
                                .get(&(u.part.clone(), u.targets.get(1).cloned().unwrap_or_default()))
                                .map(|b| b.as_deref().map(|beh| split_behavior_ops(beh).1.is_empty()).unwrap_or(false))
                                .unwrap_or(false)))
            })
            .map(|(i, _)| i)
            .collect();
        for ci in candidates {
            let (c_scope, c_mode, c_targets) = (universe[ci].scope.clone(), universe[ci].mode.clone(), universe[ci].targets.clone());
            let Some(si) = scopes.iter().position(|s| s.prefix == c_scope) else { continue };
            let sc = &scopes[si];
            for (di, d_mode, d_targets) in &detected_dangerous {
                if universe[*di].scope != c_scope {
                    continue;
                }
                let mut faulted = netlist.clone();
                let mut alias: HashMap<String, String> = HashMap::new();
                // TRANSIENT dangerous fault (pulse state) blinded by a
                // DORMANT board fault — the real latent scenario for
                // transients: the candidate's damage sits silent until
                // the glitch arrives, and the glitch's external
                // detection is gone. Apply the candidate's DC mutation,
                // then re-run the transient over it. A vendor-declared
                // INTERNAL detection is chip-side — a board fault
                // cannot blind it, so such states are skipped.
                let d_state_key = (
                    d_targets.first().cloned().unwrap_or_default(),
                    d_targets.get(1).cloned().unwrap_or_default(),
                );
                let d_pulse_behavior = (*d_mode == "state")
                    .then(|| state_behaviors.get(&d_state_key).cloned().flatten())
                    .flatten()
                    .filter(|b| !split_behavior_ops(b).1.is_empty());
                if let Some(beh) = d_pulse_behavior {
                    let Some(tr) = tran else { continue };
                    if state_internal.get(&d_state_key).cloned().flatten().is_some() {
                        continue; // internally detected — not blindable
                    }
                    let (dc_ops, pulses) = split_behavior_ops(&beh);
                    // apply the CANDIDATE's mutation first
                    let apply_c = |n: &mut Netlist, alias: &mut HashMap<String, String>| -> Result<(), String> {
                        match c_mode.as_str() {
                            "short" | "short_adjacent" => apply_short(n, &view, &c_targets[0], &c_targets[1]).map(|a| {
                                if let Some((from, to)) = a {
                                    alias.insert(from, to);
                                }
                            }),
                            "open" | "open_pin" => apply_open(n, &view, &c_targets[0]),
                            "drift_high" | "drift_low" => {
                                apply_drift(n, &view, &c_targets[0], c_targets.get(1).map(|s| s.as_str()).unwrap_or("+0%"))
                            }
                            "state" => match state_behaviors.get(&(c_targets[0].clone(), c_targets.get(1).cloned().unwrap_or_default())) {
                                Some(Some(b)) => apply_state_behavior(n, &view, &c_targets[0], b, alias),
                                _ => Err("state without behavior".into()),
                            },
                            _ => Err("unsupported".into()),
                        }
                    };
                    if apply_c(&mut faulted, &mut alias).is_err() {
                        continue;
                    }
                    let mech_pairs: Vec<(String, Option<String>)> =
                        sc.mechs.iter().map(|(h, _, dw)| (h.clone(), dw.clone())).collect();
                    let Ok(out) = run_transient_state(
                        &faulted, &view, &sc.prefix, &sc.ns, &sc.effects, &mech_pairs,
                        &d_state_key.0, &dc_ops, &pulses, None, tr, None, &alias,
                    ) else {
                        continue;
                    };
                    let still_dangerous = out.fired.iter().any(|p| universe[*di].fired.contains(p));
                    if still_dangerous && out.detected_ext.is_empty() {
                        universe[ci].latent = true;
                        if universe[ci].note.is_none() {
                            universe[ci].note = Some(format!(
                                "LATENT: with the TRANSIENT {}({}) also injected, its dangerous effect fires with external detection GONE",
                                d_mode,
                                d_targets.join(",")
                            ));
                        }
                        if let Some(wd) = universe[*di].weight_fit {
                            universe[ci].latent_exposed_fit += wd;
                        }
                    }
                    continue;
                }
                let apply = |n: &mut Netlist, mode: &str, targets: &[String], alias: &mut HashMap<String, String>| -> Result<(), String> {
                    match mode {
                        "short" | "short_adjacent" => apply_short(n, &view, &targets[0], &targets[1]).map(|a| {
                            if let Some((from, to)) = a {
                                alias.insert(from, to);
                            }
                        }),
                        "open" | "open_pin" => apply_open(n, &view, &targets[0]),
                        // drift rows carry their WORST probe magnitude
                        // as targets[1] after classification
                        "drift_high" | "drift_low" => {
                            apply_drift(n, &view, &targets[0], targets.get(1).map(|s| s.as_str()).unwrap_or("+0%"))
                        }
                        "state" => match state_behaviors.get(&(targets[0].clone(), targets.get(1).cloned().unwrap_or_default())) {
                            Some(Some(beh)) => apply_state_behavior(n, &view, &targets[0], beh, alias),
                            _ => Err("state without behavior".into()),
                        },
                        _ => Err("unsupported".into()),
                    }
                };
                if apply(&mut faulted, &c_mode, &c_targets, &mut alias).is_err() {
                    continue;
                }
                if apply(&mut faulted, d_mode, d_targets, &mut alias).is_err() {
                    continue;
                }
                let Ok(volts) = solve(&faulted) else { continue };
                // dangerous effect still present under the pair?
                let mut still_dangerous = false;
                for (path, expr) in &sc.effects {
                    if universe[*di].fired.contains(path) {
                        if let Ok(true) = eval_effect(expr, &sc.prefix, &sc.ns, &view, &alias, &volts) {
                            still_dangerous = true;
                        }
                    }
                }
                if !still_dangerous {
                    continue;
                }
                // detection lost?
                let mut any_detect = false;
                for (_h, _d, dw) in &sc.mechs {
                    if let Some(pred) = dw {
                        if let Ok(true) = eval_effect(pred, &sc.prefix, &sc.ns, &view, &alias, &volts) {
                            any_detect = true;
                        }
                    }
                }
                if !any_detect {
                    universe[ci].latent = true;
                    if universe[ci].note.is_none() {
                        universe[ci].note = Some(format!(
                            "LATENT: with {}({}) also injected, the dangerous effect persists UNDETECTED",
                            d_mode,
                            d_targets.join(",")
                        ));
                    }
                    // exposure for the dual-point PMHF term: Σ λ of the
                    // dangerous faults this latent mode blinds — so the
                    // probe runs EVERY pair, no early break.
                    if let Some(wd) = universe[*di].weight_fit {
                        universe[ci].latent_exposed_fit += wd;
                    }
                }
            }
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


/// Compute the FMEDA metrics per scope from the measured universe
/// (Phase 3 increment 3). Purely arithmetic over what the campaign
/// measured; incomplete measurement is stated, never normalized away.
pub fn compute_metrics(model: &mut SafetyModel) {
    use bhdl_common::safety::{metric_targets, Gap, Metrics};
    if model.universe.is_empty() {
        return;
    }
    let mut gaps: Vec<Gap> = Vec::new();
    for scope in model.scopes.iter_mut() {
        let faults: Vec<&bhdl_common::safety::UniverseFault> =
            model.universe.iter().filter(|u| u.scope == scope.path).collect();
        if faults.is_empty() {
            continue;
        }
        let measured: Vec<_> = faults.iter().filter(|u| u.ran && u.weight_fit.is_some()).collect();
        let unmeasured = faults.len() - measured.len();
        let lambda_total: f64 = measured.iter().map(|u| u.weight_fit.unwrap()).sum();
        let lambda_residual: f64 = measured
            .iter()
            .filter(|u| !u.fired.is_empty() && u.detected.is_empty())
            .map(|u| u.weight_fit.unwrap())
            .sum();
        let lambda_latent: f64 = measured.iter().filter(|u| u.latent).map(|u| u.weight_fit.unwrap()).sum();
        let spfm = if lambda_total > 0.0 { 1.0 - lambda_residual / lambda_total } else { 1.0 };
        let non_spf = lambda_total - lambda_residual;
        let lfm = if non_spf > 0.0 { 1.0 - lambda_latent / non_spf } else { 1.0 };
        // Dual-point term (second-order, ISO 26262-10 §8.3.3 shape):
        // for each latent fault L, both L and one of the dangerous
        // faults it blinds must coexist. The EXPOSURE WINDOW of the
        // latent fault is, strongest bound first:
        //   - the defeated mechanism's declared proof-test `interval`
        //     (a periodic self-test reveals the dormant fault at the
        //     next test — ISO's multiple-point fault detection
        //     interval), when the mechanism on the latent fault's part
        //     declares one;
        //   - else the service lifetime / 2 (never tested — the
        //     average dormancy of a uniformly-arriving fault).
        // λ in FIT (1e-9/h), window in hours ⇒ contribution in FIT =
        // w_L·w_exposed·T_window·1e-9 (the /2 belongs to the
        // untested-average case only; a test interval IS the worst-case
        // window). Needs the mission to DECLARE the lifetime;
        // otherwise PMHF stays the single-point approximation, stated.
        let lifetime_h = model.mission.as_ref().and_then(|m| m.lifetime_h);
        // latent part → min declared test interval (hours) over the
        // mechanisms on that part
        let mech_interval_h = |part: &str| -> Option<f64> {
            scope
                .mechanisms
                .iter()
                .filter(|m| m.instance == part)
                .filter_map(|m| m.interval.as_deref().and_then(parse_duration_s))
                .map(|s| s / 3600.0)
                .fold(None, |acc: Option<f64>, v| Some(acc.map_or(v, |a: f64| a.min(v))))
        };
        let pmhf_dual = lifetime_h.map(|t| {
            measured
                .iter()
                .filter(|u| u.latent)
                .map(|u| {
                    let window = match mech_interval_h(&u.part) {
                        Some(iv) => iv.min(t / 2.0),
                        None => t / 2.0,
                    };
                    u.weight_fit.unwrap() * u.latent_exposed_fit * window * 1e-9
                })
                .sum::<f64>()
        });
        let pmhf = lambda_residual + pmhf_dual.unwrap_or(0.0);
        // strictest goal level with targets
        let target_level = scope
            .goals
            .iter()
            .map(|g| g.level)
            .filter(|l| metric_targets(*l).is_some())
            .max();
        let targets = target_level.and_then(metric_targets);
        let complete = unmeasured == 0 && lambda_total > 0.0;
        let pass = match (targets, complete) {
            (Some((smin, lmin, pmax)), true) => Some(spfm >= smin && lfm >= lmin && pmhf <= pmax),
            (Some(_), false) => Some(false),
            (None, _) => None,
        };
        if let (Some((smin, lmin, pmax)), Some(false)) = (targets, pass) {
            let lvl = target_level.unwrap().as_str();
            if !complete {
                gaps.push(Gap {
                    class: bhdl_common::safety::GapClass::MetricMissed,
                    goal: scope.path.clone(),
                    subject: format!("{lvl} metrics"),
                    fix: format!("{unmeasured} universe fault(s) unmeasured (no λ share or not run) — metrics cannot pass at {lvl} until the whole universe is measured"),
                });
            } else {
                let mut misses = Vec::new();
                if spfm < smin { misses.push(format!("SPFM {:.1}% < {:.0}%", spfm * 100.0, smin * 100.0)); }
                if lfm < lmin { misses.push(format!("LFM {:.1}% < {:.0}%", lfm * 100.0, lmin * 100.0)); }
                if pmhf > pmax { misses.push(format!("PMHF {:.1} FIT > {:.0} FIT", pmhf, pmax)); }
                gaps.push(Gap {
                    class: bhdl_common::safety::GapClass::MetricMissed,
                    goal: scope.path.clone(),
                    subject: format!("{lvl} metrics"),
                    fix: format!("{} (ISO 26262-5:2018 T4/T5/T6) — raise coverage or reduce residual λ", misses.join("; ")),
                });
            }
        }
        scope.metrics = Some(Metrics {
            lambda_total_fit: lambda_total,
            lambda_residual_fit: lambda_residual,
            lambda_latent_fit: lambda_latent,
            unmeasured_faults: unmeasured,
            spfm,
            lfm,
            pmhf_fit: pmhf,
            pmhf_dual_fit: pmhf_dual,
            target_level,
            targets,
            pass,
        });
    }
    model.gaps.extend(gaps);
    model.gaps.sort_by(|a, b| (a.class as u8, &a.subject).cmp(&(b.class as u8, &b.subject)));
}


#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_common::safety::{Goal, Level, Metrics, Scope, UniverseFault};

    fn uf(scope: &str, part: &str, w: f64, fired: bool, detected: bool, latent: bool) -> UniverseFault {
        UniverseFault {
            scope: scope.into(),
            part: part.into(),
            mode: "short".into(),
            targets: vec![],
            ran: true,
            fired: if fired { vec!["G.e".into()] } else { vec![] },
            detected: if detected { vec!["m".into()] } else { vec![] },
            false_alarm: false,
            latent,
            latent_exposed_fit: if latent { 10.0 } else { 0.0 },
            weight_fit: Some(w),
            note: None,
        }
    }

    /// SPFM/LFM/PMHF arithmetic against hand-computed values, and the
    /// ASIL_B gate on a miss.
    #[test]
    fn metrics_arithmetic_and_targets() {
        let goal = Goal {
            name: "G".into(), path: "G".into(), library_type: None, level: Level::AsilB,
            title: String::new(), id: None, ftti: None, safe_state: None, effects: vec![], refines: None,
        };
        let mut model = SafetyModel {
            board: "B".into(), mission: None,
            scopes: vec![Scope { path: String::new(), entity: "B".into(), ns: "brd".into(),
                goals: vec![goal], mechanisms: vec![], faults: vec![], waivers: vec![], assumptions: vec![], metrics: None }],
            parts: vec![], universe: vec![
                uf("", "a", 10.0, true, true, false),   // dangerous detected
                uf("", "b", 5.0, true, false, false),   // RESIDUAL
                uf("", "c", 20.0, false, false, false), // safe
                uf("", "d", 5.0, false, false, true),   // LATENT
            ],
            gaps: vec![], errors: vec![],
        };
        compute_metrics(&mut model);
        let m: &Metrics = model.scopes[0].metrics.as_ref().unwrap();
        // λ_total=40, λ_res=5 → SPFM = 1 − 5/40 = 87.5%
        assert!((m.spfm - 0.875).abs() < 1e-9);
        // LFM = 1 − 5/(40−5) = 85.71%
        assert!((m.lfm - (1.0 - 5.0 / 35.0)).abs() < 1e-9);
        assert!((m.pmhf_fit - 5.0).abs() < 1e-9);
        // ASIL_B: SPFM 87.5% < 90% ⇒ MISS + gap naming SPFM
        assert_eq!(m.pass, Some(false));
        assert!(model.gaps.iter().any(|g| g.class == bhdl_common::safety::GapClass::MetricMissed && g.fix.contains("SPFM")));
        // raise coverage: make b detected too → SPFM 100%, PASS
        model.universe[1].detected = vec!["m".into()];
        model.gaps.clear();
        compute_metrics(&mut model);
        let m = model.scopes[0].metrics.as_ref().unwrap();
        assert_eq!(m.pass, Some(true), "{m:?}");
        assert!(model.gaps.is_empty());

        // dual-point PMHF: declare a lifetime → the latent fault (w=5,
        // exposure=10 FIT) contributes 5·10·10000/2·1e-9 = 2.5e-4 FIT.
        model.mission = Some(bhdl_common::safety::Mission {
            ambient_c: 40.0, on_hours: None, cycles: None, environment: None,
            quality: None, profile: None, phases: vec![], time_basis: None,
            lifetime_h: Some(10_000.0),
        });
        model.gaps.clear();
        compute_metrics(&mut model);
        let m = model.scopes[0].metrics.as_ref().unwrap();
        let d = m.pmhf_dual_fit.expect("dual term with lifetime");
        assert!((d - 2.5e-4).abs() < 1e-9, "dual = 2.5e-4 FIT, got {d}");
        assert!((m.pmhf_fit - (0.0 + d)).abs() < 1e-9, "PMHF = residual(0 after coverage fix) + dual");

        // Proof-test interval bounds the latent exposure: a mechanism on
        // the latent part 'd' declaring interval=1000h means the dormant
        // fault is revealed at the next test — window = min(1000h,
        // T/2=5000h) = 1000h ⇒ dual = 5·10·1000·1e-9 = 5e-5 FIT (no /2:
        // the interval IS the worst-case window).
        model.scopes[0].mechanisms.push(bhdl_common::safety::Mechanism {
            instance: "d".into(), handle: "brd.d".into(),
            kind: bhdl_common::safety::MechanismKind::Psm,
            goal: "G".into(), detects: vec![], protects: None,
            claimed_dc: None, dc_source: None,
            interval: Some("1000h".into()), latency: None,
            detected_when: None, measured_dc: None, measured_note: None,
        });
        model.gaps.clear();
        compute_metrics(&mut model);
        let d2 = model.scopes[0].metrics.as_ref().unwrap().pmhf_dual_fit.unwrap();
        assert!((d2 - 5e-5).abs() < 1e-12, "interval-bounded dual = 5e-5 FIT, got {d2}");
        model.scopes[0].mechanisms.clear();

        // duration parsing: hours/days are real proof-test units
        assert_eq!(parse_duration_s("1h"), Some(3600.0));
        assert_eq!(parse_duration_s("2d"), Some(172800.0));

        // SIL mapping: same residual arithmetic gated as SFF/PFH.
        // Restore the residual (b undetected) and set the goal to SIL3:
        // SFF 87.5% < 90% (IEC 61508-2 T3, Type A HFT=0) ⇒ MISS.
        model.universe[1].detected = vec![];
        model.scopes[0].goals[0].level = Level::Sil3;
        model.gaps.clear();
        compute_metrics(&mut model);
        let m = model.scopes[0].metrics.as_ref().unwrap();
        assert_eq!(m.target_level, Some(Level::Sil3));
        assert_eq!(m.targets, Some((0.90, 0.0, 100.0)));
        assert_eq!(m.pass, Some(false));
        // SIL1: no SFF floor at HFT=0, PFH 10000 FIT — passes here
        model.scopes[0].goals[0].level = Level::Sil1;
        model.gaps.clear();
        compute_metrics(&mut model);
        let m = model.scopes[0].metrics.as_ref().unwrap();
        assert_eq!(m.pass, Some(true), "{m:?}");
    }
}

// ── FMEDA export (assessor worksheet) ───────────────────────────────

/// The three CSV bodies of the FMEDA package: the per-fault worksheet,
/// the mechanism table (claimed vs MEASURED DC), and the metrics
/// summary. Everything comes from the measured model — no field is
/// computed here, only serialized; empty cells mean the datum does not
/// exist (never zero-filled).
pub struct FmedaCsvs {
    pub worksheet: String,
    pub mechanisms: String,
    pub metrics: String,
}

fn csv_cell(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn csv_row(cells: &[String]) -> String {
    cells.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(",")
}

fn fmt_f(v: f64) -> String {
    format!("{v:.4}")
}

/// Serialize the measured safety model as the FMEDA package.
pub fn export_fmeda(model: &bhdl_common::safety::SafetyModel) -> FmedaCsvs {
    use bhdl_common::safety::PartData;
    let s = |x: &str| x.to_string();
    // part → (type, λ_total, basis/source line) for the denormalized
    // worksheet columns
    let part_info: HashMap<&str, (String, Option<f64>, String)> = model
        .parts
        .iter()
        .map(|p| {
            let (fit, basis) = match &p.data {
                PartData::Handbook { class, source, per, fit, fit_basis } => (
                    *fit,
                    fit_basis.clone().unwrap_or_else(|| {
                        format!("handbook {class}{} — FIT not computed; {source}",
                            per.as_deref().map(|x| format!(" per {x}")).unwrap_or_default())
                    }),
                ),
                PartData::Behavioral { states, source, .. } => {
                    let total: f64 = states.iter().filter_map(|st| st.fit).sum();
                    (
                        (total > 0.0).then_some(total),
                        format!("vendor failure states ({}) — {source}", states.len()),
                    )
                }
                PartData::Seooc { lambda_fit, source } => (*lambda_fit, format!("SEooC — {source}")),
                PartData::Waived { reason } => (None, format!("WAIVED: {reason}")),
                PartData::None => (None, "no safety data (PART_NO_SAFETY_DATA gap)".to_string()),
            };
            (p.instance.as_str(), (p.type_name.clone(), fit, basis))
        })
        .collect();
    let mut w: Vec<String> = Vec::new();
    w.push(csv_row(&[
        s("scope"), s("part"), s("entity"), s("part_lambda_fit"), s("part_fit_basis"),
        s("failure_mode"), s("targets"), s("mode_lambda_share_fit"), s("ran"),
        s("classification"), s("effects_fired"), s("detected_by"),
        s("latent_exposed_fit"), s("note"),
    ]));
    let mut covered: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for u in &model.universe {
        covered.insert(u.part.as_str());
        let (ty, pfit, pbasis) = part_info
            .get(u.part.as_str())
            .cloned()
            .unwrap_or((String::new(), None, String::new()));
        let class = if !u.ran {
            "NOT_RUN"
        } else if u.latent {
            "LATENT"
        } else if !u.fired.is_empty() && u.detected.is_empty() {
            "RESIDUAL"
        } else if !u.fired.is_empty() {
            "DETECTED_DANGEROUS"
        } else if u.false_alarm {
            "FALSE_ALARM"
        } else {
            "SAFE"
        };
        w.push(csv_row(&[
            u.scope.clone(),
            u.part.clone(),
            ty,
            pfit.map(fmt_f).unwrap_or_default(),
            pbasis,
            u.mode.clone(),
            u.targets.join(" "),
            u.weight_fit.map(fmt_f).unwrap_or_default(),
            u.ran.to_string(),
            class.to_string(),
            u.fired.join(" "),
            u.detected.join(" "),
            if u.latent { fmt_f(u.latent_exposed_fit) } else { String::new() },
            u.note.clone().unwrap_or_default(),
        ]));
    }
    // parts the universe generated NO modes for still belong on the
    // worksheet — an assessor must see them, not infer them from gaps
    for p in &model.parts {
        if covered.contains(p.instance.as_str()) {
            continue;
        }
        let (ty, pfit, pbasis) = part_info.get(p.instance.as_str()).cloned().unwrap_or_default();
        w.push(csv_row(&[
            p.parent.clone().unwrap_or_default(),
            p.instance.clone(),
            ty,
            pfit.map(fmt_f).unwrap_or_default(),
            pbasis,
            s("(no universe modes)"),
            String::new(), String::new(), s("false"), s("NOT_RUN"),
            String::new(), String::new(), String::new(),
            s("part not represented in the fault universe — see gaps"),
        ]));
    }
    // Stated coverage exclusion — a worksheet ROW, so the assessor
    // sees it in the artifact itself, not only in the terminal report.
    // Not a UniverseFault: it must not enter the λ arithmetic or the
    // unmeasured-fault count (it is an exclusion, not an unrun fault).
    w.push(csv_row(&[
        String::new(), s("(board)"), String::new(), String::new(), String::new(),
        s("short_inter_part"), String::new(), String::new(), s("false"), s("EXCLUDED"),
        String::new(), String::new(), String::new(),
        s("EXCLUDED from this analysis: bridge faults between different parts' pads require placement (pad adjacency across parts is a layout outcome); this analysis does not consume layout"),
    ]));
    // mechanisms: claimed vs measured
    let mut m: Vec<String> = Vec::new();
    m.push(csv_row(&[
        s("scope"), s("mechanism"), s("instance"), s("goal"), s("detects"),
        s("claimed_dc"), s("dc_source"), s("measured_dc"), s("measurement_basis"),
        s("interval"), s("latency"), s("detected_when"),
    ]));
    for sc in &model.scopes {
        for mech in &sc.mechanisms {
            m.push(csv_row(&[
                sc.path.clone(),
                mech.handle.clone(),
                mech.instance.clone(),
                mech.goal.clone(),
                mech.detects.join(" "),
                mech.claimed_dc.map(fmt_f).unwrap_or_default(),
                mech.dc_source.clone().unwrap_or_default(),
                mech.measured_dc.map(fmt_f).unwrap_or_default(),
                mech.measured_note.clone().unwrap_or_default(),
                mech.interval.clone().unwrap_or_default(),
                mech.latency.clone().unwrap_or_default(),
                mech.detected_when.clone().unwrap_or_default(),
            ]));
        }
    }
    // metrics summary, one row per scope that computed metrics
    let mut t: Vec<String> = Vec::new();
    t.push(csv_row(&[
        s("scope"), s("lambda_total_fit"), s("lambda_residual_fit"), s("lambda_latent_fit"),
        s("unmeasured_faults"), s("spfm"), s("lfm"), s("pmhf_fit"), s("pmhf_dual_point_fit"),
        s("target_level"), s("spfm_min"), s("lfm_min"), s("pmhf_max_fit"), s("pass"),
    ]));
    for sc in &model.scopes {
        let Some(mx) = &sc.metrics else { continue };
        let (smin, lmin, pmax) = match mx.targets {
            Some((a, b, c)) => (fmt_f(a), fmt_f(b), fmt_f(c)),
            None => (String::new(), String::new(), String::new()),
        };
        t.push(csv_row(&[
            sc.path.clone(),
            fmt_f(mx.lambda_total_fit),
            fmt_f(mx.lambda_residual_fit),
            fmt_f(mx.lambda_latent_fit),
            mx.unmeasured_faults.to_string(),
            fmt_f(mx.spfm),
            fmt_f(mx.lfm),
            fmt_f(mx.pmhf_fit),
            mx.pmhf_dual_fit.map(|v| format!("{v:.3e}")).unwrap_or_default(),
            mx.target_level.map(|l| format!("{l:?}")).unwrap_or_default(),
            smin, lmin, pmax,
            mx.pass.map(|p| p.to_string()).unwrap_or_default(),
        ]));
    }
    FmedaCsvs {
        worksheet: w.join("\n") + "\n",
        mechanisms: m.join("\n") + "\n",
        metrics: t.join("\n") + "\n",
    }
}
