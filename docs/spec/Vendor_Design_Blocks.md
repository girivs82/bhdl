# Vendor `design { }` Blocks — Intent → Bias as Authored HDL

> **Status:** SHIPPED through Stage 5. The `design for <intent> { }`
> surface (const / require-else / child-assignment), analyzer recipe
> extraction, and the synthesizer evaluator are implemented, along
> with the §11 `body rhai` escape hatch (`inputs`/`outputs` decls,
> raw-string literals, sandboxed fuel-limited Rhai — see
> `bhdl-synthesizer/src/design_evaluator.rs`). The reference
> designers were migrated to authored blocks in
> `bhdl-stdlib/actives/triode.bhdl`: current source and switch as
> declarative blocks, the amplifier as `body rhai`.
>
> Divergence from §3 as originally drafted: the shipped declarative
> surface spells bindings `const` (not `let`), and closures /
> `bisect_*` primitives never shipped — search loops live in a
> `body rhai` script instead. Still planned: the per-block fuel
> override (`runtime rhai(max_operations: …)`, §11.5); the default
> 1M-operation limit is fixed today.

## 1. Motivation

bhdl now wires intent-driven operating-point design end to end: a
designer asks for `for amplifier(gain: 15)` (or `current_source(...)`, or
`digital_switch()`), and a Rust designer in `bhdl-spice/src/tube_bias.rs`
computes the bias network the expansion's children inherit. The
*framework* — intent surface, refine loop, GLACIER, expansion mechanism
— is now in place.

The unresolved tension is **who owns the design logic**. Today the three
designers (amplifier, current source, switch) live in BHDL core (Rust).
Only the bhdl maintainers can extend them. But the stdlib premise is that
*vendors* ship components — and a vendor's deep IP is exactly the
operating-point math their devices need: the formulas, tables and fudge
factors in their application-note spreadsheets. Pinning that IP behind
core PRs breaks the premise.

The design splits cleanly:

| Owner | Owns |
|---|---|
| **BHDL core** | the intent surface (`for <name>(...)`), the simulate-and-refine loop, GLACIER, the expansion mechanism — and a *reference designer* per intent as the default |
| **Vendor stdlib** | the device's analytic first-guess design — *the formulas* — declared in HDL, alongside the entity that uses them |

The simulate→parameterize→**finalize** loop already runs against the
reference designers. This proposal lets a vendor's design logic plug in
to the *parameterize* step, declaratively, from a `.bhdl` file.

## 2. What the surface must express

The three reference designers (`design_amplifier`, `design_current_source`,
`design_switch`) bound what the surface must support. Pulled out of the
Rust:

1. **Inputs**
   - tube parameters: `mu`, `ex`, `kg1`, `kp`, `kvb` (the Koren model)
   - board context: `V_bb` (read from the power net on the parent's
     `VBB` pin), and any other rail voltages the topology touches
   - intent parameters: `gain`, `current`, etc. (from the stamped
     `intent_<param>` attribute)
2. **Operations**
   - basic arithmetic and comparisons
   - forward Koren: `plate_current(params, V_pk, V_gk) → I_p`
   - inverse Koren: `koren_inverse_vgk(params, V_pk, I_p) → V_gk`
   - small-signal: `conductances(params, V_pk, V_gk) → (g_p, g_m)`
   - 1-D search over a closed-form scalar function (the `gain_at(I_p)`
     bisection in `design_amplifier`)
3. **Outputs**
   - a map of *expansion child name → designed value* (`Rp ↦ 15434.0`,
     `Rk ↦ 416.2`, …); the expansion interpreter applies these on top
     of the `expansion { }` block's literal defaults
4. **Validation**
   - reject out-of-band targets with a human-readable message
5. **(Optional) Refinement hook**
   - after the analytic guess, call out to the simulator to verify and
     correct; covered by the *framework*, not the vendor block —
     vendor logic supplies the guess only

(5) is the deliberate boundary: vendors author closed-form / lookup
logic in HDL; the GLACIER-refine loop is core machinery every intent
shares and vendors do *not* re-author.

## 3. Proposed syntax

A new optional block on a tube entity, evaluated when the entity
expands under a matching intent. (The sketch below is the *original
proposal* — `let` bindings and closures did not ship; the shipped
spelling uses `const` and, for search loops like this amplifier, a
`body rhai` script — see §11 and `bhdl-stdlib/actives/triode.bhdl`.)

<!-- doc-check: skip (documents the originally-proposed let/closure surface; shipped amplifier uses body rhai) -->
```bhdl
entity SignalTubeStage() {
    pin IN:  signal in;
    pin VBB: power in;
    pin GND: ground;
    pin OUT: signal out virtual;

    attribute component_class = "tube_gain_stage";

    expansion { ... }    // unchanged — topology

    design for amplifier {        // intent name (matches `for amplifier(...)`)
        let target_gain = intent.gain;
        let mu  = tube.mu;
        let ex  = tube.ex;
        let kg1 = tube.kg1;
        let kp  = tube.kp;
        let kvb = tube.kvb;
        let v_p = VBB / 2;

        // Inline closed-form: pin V_p at V_bb/2, bisect I_p for the gain.
        let i_p = bisect_descending(0.5mA, 30mA, target_gain, fn(i) {
            let r_p_trial = v_p / i;
            let v_gk = koren_inverse_vgk(mu, ex, kg1, kp, kvb, v_p, i);
            let (g_p, g_m) = conductances(mu, ex, kg1, kp, kvb, v_p, v_gk);
            g_m / (g_p + 1 / r_p_trial)
        });

        let v_gk = koren_inverse_vgk(mu, ex, kg1, kp, kvb, v_p, i_p);

        // Assign to the expansion children — these override their
        // `Res(...)` defaults declared in the expansion block above.
        Rp = v_p / i_p;
        Rk = -v_gk / i_p;
    }
}
```

Key syntactic choices, each defensible:

- **`design for <intent_name> { … }`**: one design block per supported
  intent. An entity with no `design for foo` block on the `foo` intent
  falls back to core's reference designer (or the expansion's literal
  values if no reference exists).
- **`intent.<param>`**: typed access to stamped intent parameters
  (`intent.gain`, `intent.current`). Missing required params are an
  HDL-level error at expansion time.
- **`tube.<param>`**: access to the entity's children's parameters. For
  `SignalTubeStage`, `tube` resolves to the (single) `Triode` child;
  for entities with more than one device, the syntax extends to
  `<child>.<param>`. The Koren parameters are exactly the attributes
  the Triode entity already declares.
- **`VBB`**, **`GND`**, **`IN`**, **`OUT`**: the entity's pins. When
  read in a `design` block they evaluate to the *voltage* of the net
  on that pin (read from `NetClass::Power(v)` for power rails;
  unsupported for signal nets at design time).
- **`Rp = …; Rk = …;`**: assignments to expansion-child names. The
  child must exist in the expansion block (so `Rp` and `Rk` here
  reference the `Rp: Res(...)` and `Rk: Res(...)` instances). A
  `design` block may assign to any subset; unset children keep their
  literal expansion-block defaults.
- **Built-in primitive functions**: `koren_inverse_vgk`,
  `plate_current`, `conductances`, `bisect_descending`,
  `bisect_increasing`. Calls dispatch to bhdl-spice's existing Rust
  implementations. The set is closed — vendors compose primitives,
  they don't define new ones in HDL.
- **Closures as primitive arguments**: `bisect_descending(...)` takes
  an anonymous `fn(x) { ... }` returning a scalar. Restricted: a
  closure body is itself a `design` block with one free parameter and
  one return expression.
- **Units**: numeric literals carry the same unit suffixes as the
  rest of bhdl (`0.5mA`, `30mA`, `1MΩ`, …).
- **Errors**: `require <condition> else "<message>";` validation
  statements produce structured errors the synthesizer surfaces to the
  user verbatim, mirroring today's Rust `SpiceError::AnalysisFailed`
  shape.

## 4. Worked examples — the 3 reference designers in `design { }`

### 4.1 Amplifier

(Full version in §3.) The Rust at `tube_bias.rs::ReferenceTriodeDesigner`
maps cleanly: peak-scan + descending-flank bisection + Koren inversion.
`bisect_descending` covers the scan-then-bisect motif; the closure
captures the per-I_p gain computation. The `require` statement replaces
the explicit `Err` returns for "target above peak / below min".

### 4.2 Current source

Shipped verbatim (modulo comments) in
`bhdl-stdlib/actives/triode.bhdl`:

```bhdl
design for current_source {
    const v_pk = 100.0;              // design point — documented
    const i_target = intent.current;
    const i_max = plate_current(tube.mu, tube.ex, tube.kg1, tube.kp, tube.kvb, v_pk, 0.0);
    require i_target < i_max
        else "current target exceeds the tube's zero-bias current at V_pk = 100V";
    const v_gk = koren_inverse_vgk(tube.mu, tube.ex, tube.kg1, tube.kp, tube.kvb, v_pk, i_target);
    Rk = (0.0 - v_gk) / i_target;
}
```

### 4.3 Digital switch

Shipped verbatim (modulo comments) in
`bhdl-stdlib/actives/triode.bhdl` — note supply voltages are read
via the `supply.*` namespace, not the bare pin name:

```bhdl
design for digital_switch {
    const v_sat = 10.0;
    const v_bb = supply.VBB;
    require v_bb > v_sat
        else "switch needs V_bb > 10 V to leave headroom for saturation";
    const i_sat = plate_current(tube.mu, tube.ex, tube.kg1, tube.kp, tube.kvb, v_sat, 0.0);
    require i_sat > 0.0
        else "tube draws no current at zero bias — cannot pull plate down";
    Rp = (v_bb - v_sat) / i_sat;
}
```

Three designers, three intent shapes, all expressible inside the
proposed surface. The amplifier exercises every primitive; current
source and switch are progressively simpler.

## 5. Semantics

### Evaluation model

A `design` block is evaluated **once per matching expansion** in a
side-effect-free environment:

1. The expansion interpreter has already built `cand.param_values` and
   located the parent's pin nets, exactly as today.
2. When `cand.recipe` includes a `design for <intent>` block matching
   `cand.param_values["intent_name"]`, the interpreter chooses that
   block over the (Rust) reference designer.
3. The block's bindings are evaluated top-to-bottom. `let` introduces a
   fresh, immutable binding. Identifiers resolve in this order:
   *block-local lets → `intent.*` → `tube.*` → parent pin names → no
   match (compile error)*.
4. Each child-assignment (`Rp = …;`) computes the right-hand expression
   and stores the result in the same `HashMap<child_name, f64>` that
   `intent_driven_values` currently returns from Rust. The expansion
   interpreter then uses that map exactly as it does today.

The `design` block has **no I/O, no mutation, no side effects** — it is
a pure function from (intent params, tube params, board context) to a
child-value map. This is what makes it tractable both to parse and to
reason about.

### Closures

Closures are allowed only as arguments to `bisect_*` (and equivalent
primitives we add later). Their syntax is a restricted sub-`design`:
one parameter, one expression, no `require`, no assignments. This keeps
the language strictly finite — no recursion, no general control flow,
no user-defined functions.

### Refinement is the framework's job

A vendor `design` block returns an analytic first guess. The expansion
interpreter then runs the *same* generic refine loop that wraps the
Rust reference designers today (`tube_bias::refine` for the amplifier;
nothing for current source / switch). That loop owns:

- building the test circuit,
- calling GLACIER,
- measuring the achieved operating point,
- adjusting free variables and re-calling the designer,
- bounded iteration with a "last good design" fallback.

Vendors don't author the refine loop. They state the first guess and
the framework polishes it.

## 6. Integration with what's already there

This proposal adds machinery; it doesn't displace any.

- **Parser**: a new `design for <ident> { … }` top-level item inside
  an `entity` body (alongside `expansion { }`). A new keyword
  `design` *contextually* — `design` is unlikely to collide with
  existing identifiers but the same caveat that gave us
  `digital_switch` (because `switch` is reserved) will guide the
  final keyword choice.
- **Analyzer**: an extraction pass that pulls a per-intent
  `DesignRecipe` out of an entity's AST, parallel to the existing
  `ExpansionRecipe`. The recipe carries the AST of the block's
  statements + a typed signature (what intent params it consumes,
  what children it assigns to).
- **Synthesizer**: when `intent_driven_values` is about to dispatch
  on an intent, it first checks for a `DesignRecipe` on the
  expanding entity. Present → evaluate the recipe (new code). Absent
  → fall back to today's Rust designer (existing code).
- **Evaluator**: a small `bhdl-synthesizer::design_evaluator` module
  walks the recipe AST, resolving `let` bindings, calling primitive
  functions, and producing the value map. Primitives dispatch to
  `bhdl_spice` (Koren math, conductances).

The expansion-interpreter changes are bounded: one new "evaluate
vendor recipe" branch in `intent_driven_values`. Everything downstream
(refine, expansion, converter) is untouched.

## 7. Implementation roadmap

The work breaks into four shippable stages, each independently useful:

1. **Lexer + parser** for `design for <name> { … }`. New tokens
   (`design`, `let`, `require`, `else`, `fn`). AST nodes:
   `DesignBlock`, `LetBinding`, `Assignment`, `RequireStmt`, `Call`,
   `Closure`, `Identifier`. ≈ 2 days. Recovered cleanly even if the
   block's contents fail to parse — the entity's `expansion` block
   must keep working.
2. **Analyzer recipe extraction** — a `DesignRecipe { intent_name,
   statements, free_param_signature, assigned_children }`. Validates
   that assigned children exist in the corresponding `expansion`
   block. ≈ 1 day.
3. **Evaluator** in bhdl-synthesizer, dispatching primitive calls into
   `bhdl_spice::triode::{plate_current, conductances}` and a new
   `bhdl_spice::bisect` helper. ≈ 2 days. Tests reproduce the three
   reference designers' numerical outputs from authored
   `design { }` blocks.
4. **Migration of the reference designers**: rewrite the three
   designers from `bhdl-spice/src/tube_bias.rs` Rust into authored
   `design { }` blocks in `bhdl-stdlib/actives/triode.bhdl`. The
   reference designers become *the first vendor authors* of the
   surface — the strongest possible exercise of the design. ≈ 1 day.

Total estimated effort: ~6 days, plus normal testing/iteration. A
prototype handling current source only (the simplest math) is a
half-day standalone milestone within stage 3.

## 8. Migration path

After stages 1–4, the Rust functions
`design_amplifier_reference`, `design_current_source`,
`design_switch` continue to exist (as the test-callable numerical
core), but they are no longer invoked by the synthesizer when a
vendor `design { }` block is present. The triode stdlib becomes the
canonical reference — it ships the *whole story*, model and design,
in one HDL file.

A vendor adding a new tube family writes a new `.bhdl` with the
device's Koren parameters and an entity whose `design for amplifier`
block encodes that vendor's house biasing methodology. No Rust PR,
no core changes.

## 9. Open questions

- **Stateful or stateless across refine iterations?** Today the
  framework's refine loop calls the *same* designer with adjusted
  inputs. Should the `design { }` block also receive
  refine-iteration state (current GLACIER op point), so a vendor
  could implement a Newton correction itself? Tentative answer: no
  — keep refine framework-owned, vendors stay closed-form. If a
  vendor needs richer behaviour they migrate that intent's whole
  design to Rust core.
- **Cross-stage design** (cascade gain split across two stages).
  Outside the scope of a per-entity `design { }`; belongs to a
  higher-level "system design" mechanism. Park.
- **Vendor lookup tables**. The MOSFET / op-amp world uses tables
  more than formulas. Adding `lookup_table(...)` as a primitive is
  trivial mechanically; the open question is how the vendor
  authors the table in `.bhdl` — list literals are already in the
  grammar, so probably just allow `let tbl = [(...), (...), ...]`
  and a `lerp_table(tbl, x)` primitive.
- **Static vs. dynamic typing** of `design { }` expressions. Going
  dynamic (`f64` everywhere, units stripped) gets a prototype out
  faster; going static (richer types: `voltage`, `current`,
  `resistance`) preserves the unit safety the rest of bhdl has.
  Start dynamic, statically-type later.

## 10. Why this resolves the tension

It cleanly puts the *method* in vendor hands and the *framework* in
core hands, with a typed surface between. The vendor writes their
Excel-sheet logic *as HDL* — same `.bhdl` file as the device — and
ships it as part of the stdlib. The simulate→parameterize→finalize
loop, GLACIER, and the expansion mechanism are all reused unchanged.
A vendor never recompiles `bhdl-spice` to ship a new biasing
methodology, and bhdl maintainers never review a vendor's design
math — they only review the bounded set of *primitives* the surface
exposes.

The reference designers stay valuable: they're the worked examples
this whole proposal was designed against.

---

## 11. Amendment — `body` hook for general-purpose vendor logic

> **Status:** Amendment v2 — SHIPPED (Stage 5). Stages 1–4 shipped
> the declarative `design { }` block surface (const/require/assign);
> this amendment's `body rhai` escape hatch is implemented
> (parser: `parse_design_body_hook`; evaluator:
> `bhdl-synthesizer/src/design_evaluator.rs`; canonical user:
> the amplifier in `bhdl-stdlib/actives/triode.bhdl`).

### 11.1 The problem the declarative surface left open

The reference amplifier designer in `bhdl-spice/src/tube_bias.rs` does
a 64-point log-grid peak find followed by an 80-step bisection on the
descending flank. Sections 3–5 above proposed closures-as-arguments
(`bisect_descending(lo, hi, target, fn(i) { ... })`) to cover this
without admitting general control flow. That's enough for the
amplifier specifically — but only for the amplifier. Generalising
honestly:

- **Pinmux assignment** on an SoC is a constraint problem
  (essentially small SAT/CSP).
- **PLL divider chains** are an integer-search problem with
  per-vendor disqualifying rules.
- **Power-supply optimisation** is `fsolve`/`minimize` in 1–3D.
- **Lookup + interpolation** of characterised tables is
  array-shaped, not closed-form.
- **State-machine configuration** (DDR training, charger profiles)
  is straight imperative code over enums and maps.

These do not factor into "closed-form + a single bisection
primitive." We can either chase them with ever-more-specific HDL
primitives (`csp_solve`, `integer_grid_search`, `lookup_3d`,
`fsm_walk`, …) or admit that vendor design logic is sometimes
**arbitrary imperative code**, and host it.

The cases that *genuinely* need scipy / cvxpy / sympy (filter
synthesis, RF matching) are designer-driven interactive work whose
outputs are *numbers a human types into a `design { }` block*. They
are not synthesis-time `(inputs) → outputs` steps. So the worst case
the vendor `design` block actually needs to host is **imperative
code with loops, maps, and the BHDL math primitives** — not the full
scientific Python stack.

### 11.2 The runtime question (and its trap)

The obvious move — call out to user-installed Python — is the trap.
Pinning a user's Python version, venv, and package set across a
heterogeneous EDA-tool install is how KiCad scripting became a
support burden. Cadence and Synopsys ship their *own* Python for
exactly this reason. If BHDL tells the user "install Python 3.11,
configure `uv` first," we have already lost.

The actual answer is **an embedded scripting language baked into the
BHDL binary**: zero install, zero version selection, zero venv, one
runtime forever, statically linked.

### 11.3 Choice: Rhai

Concrete language: **Rhai** (`cargo add rhai`).

Rationale:

| Property | Why it matters |
|---|---|
| Native Rust crate (~200 KB) | Statically links into `bhdl-synthesizer`. No FFI surface to maintain, no DSO loading, no version drift between vendor authoring and synthesis. |
| Sandboxed by default | No file I/O, no syscalls, no network — unless we explicitly register host functions for them. A misbehaving vendor script cannot escape into the user's filesystem. |
| Deterministic fuel limits | Built-in `set_max_operations(n)` and time bounds. A runaway script (infinite loop, accidental quadratic) terminates with a clear synthesis error instead of hanging the build. |
| Map literals are first-class | `#{ Rp: ..., Rk: ... }` returns directly serialise to the `HashMap<String, f64>` the expansion interpreter already consumes. |
| JS/Rust-flavoured syntax | Modern devs read it fluently; not the `local`/`end`/1-indexed surprises of Lua. |
| MIT-licensed | Permissive, ships inside BHDL without licence-compatibility concerns. |

Lua (via `mlua`) is the EDA-precedent runner-up (Synopsys, Mentor
have used it for decades, though most of their scripting is Tcl).
We chose Rhai for the Rust-native integration story.

### 11.4 Syntax — `body <lang> r#"..."#`

The `design { }` block grows an alternate body form. The declarative
const/require/assign surface (Stages 1–4) is preserved verbatim for
the easy cases; the `body` form is the escape hatch.

```bhdl
design for amplifier {
    // Declared I/O. Optional; if omitted, the evaluator passes the
    // full intent/tube/supply context as-is.
    inputs  { tube; intent; supply; }
    outputs { Rp; Rk; }

    // Vendor's imperative code. Single .bhdl file, no sidecar.
    body rhai r#"
        let v_p = supply.VBB / 2.0;
        let i_lo = 0.5e-3;
        let i_hi = min(30e-3, 0.85 * plate_current(tube, v_p, 0.0));

        // Log-grid peak find.
        let peak_i = i_lo;
        let peak_g = 0.0;
        for k in 0..64 {
            let i = i_lo * (i_hi / i_lo).pow(k.to_float() / 63.0);
            let g = small_signal_gain(tube, v_p, i);
            if g > peak_g { peak_g = g; peak_i = i; }
        }
        // Descending-flank bisection on the target gain.
        let lo = peak_i;
        let hi = i_hi;
        for _ in 0..80 {
            let mid = (lo * hi).sqrt();
            if small_signal_gain(tube, v_p, mid) > intent.target_gain {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let i_p = (lo * hi).sqrt();
        let v_gk = koren_inverse_vgk(tube, v_p, i_p);
        #{ Rp: v_p / i_p, Rk: (-v_gk) / i_p }
    "#
}
```

A `design` block has **either** declarative statements (Stages 1–4)
**or** a `body` clause — not both. Mixing is rejected at analyzer
time; vendors who want both pre-validation and imperative logic put
the `require` checks inside the script.

Raw-string syntax `r#"..."#` (Rust-flavoured) is added to the
lexer specifically to make embedded foreign-language source readable:
arbitrary `"`, `\`, and even `#` inside the script need no escaping;
the closing delimiter is `"#`. Multi-hash variants (`r##"..."##`)
handle scripts that themselves contain `"#`.

### 11.5 Host-function surface (frozen contract)

A vendor script sees a fixed, versioned host API. This surface is
**part of BHDL's public ABI** — adding host functions is permitted,
changing or removing them is a breaking change. The v1 surface:

**Math primitives** (already in `bhdl_spice`):

| Host function | Returns | Notes |
|---|---|---|
| `plate_current(tube, v_pk, v_gk)` | `f64` | Koren plate-current law. |
| `koren_inverse_vgk(tube, v_pk, target_ip)` | `f64` | Inverse: V_gk that draws `target_ip` at `v_pk`. Negative for Class A. |
| `conductances(tube, v_pk, v_gk)` | `(f64, f64)` | Tuple `(g_p, g_m)`. |
| `small_signal_gain(tube, v_pk, i_p)` | `f64` | Convenience: `g_m / (g_p + 1/r_p)` with `r_p = v_pk / i_p`. |

`tube` is an opaque struct the host passes in; vendor scripts treat
it as a black-box token threaded through these primitives. The
Koren parameters are also reachable individually via `tube.mu`,
`tube.ex`, `tube.kg1`, `tube.kp`, `tube.kvb` for vendors who want
to do the math themselves.

**Generic numerics** (Rhai-native, no host code):

`min`, `max`, `abs`, `sqrt`, `pow`, `log`, `exp`, `sin`, `cos`,
trigonometry, `to_float()`, array literals, hash maps. Standard
Rhai library.

**Forbidden** (sandboxing):

No `eval`, no file I/O, no network, no process spawn, no module
imports. The script sees `inputs` and emits `outputs` — that is the
entire surface area.

**Fuel limit**: `set_max_operations(1_000_000)` (≈ 10 ms wall time
for typical scripts). The per-block override below is **planned,
not shipped** — today the 1M-operation limit is fixed:

<!-- doc-check: skip (documents planned per-block fuel override; limit is fixed at 1M ops today) -->
```bhdl
design for ddr_train {
    runtime rhai(max_operations: 10_000_000)
    body rhai r#" ... "#
}
```

### 11.6 Evaluator semantics

When the expansion interpreter encounters a `design for <intent>`
block with a `body rhai r#"..."#`:

1. Build the Rhai `Engine` (cached per process; one-time
   registration of host functions).
2. Marshal inputs into a Rhai `Scope`:
   - `tube` — the device-family parameter struct (Triode, BJT, …)
   - `intent` — a map of `{ field: f64 }` from the
     `intent_<param>` attributes
   - `supply` — a map of `{ pin_name: voltage }` from the parent's
     power-pin nets
3. Apply the recipe's fuel limit (default 1M ops).
4. `engine.eval_with_scope::<Map>(&mut scope, source)`.
5. The script's return value MUST be a Rhai `Map` whose keys are
   exactly the entity's `outputs { … }` declaration (or a subset —
   missing outputs keep their literal expansion-block defaults).
6. Marshal the `Map` back into `HashMap<String, f64>` and feed it to
   the existing expansion-interpreter machinery.

Errors (script panic, fuel exhaustion, type mismatch on return,
missing output key) propagate as `DesignEvalError::ScriptFailed`.
The synthesizer then falls through to the Rust reference designer
(if declared) or fails the synthesis with the script's stderr
captured verbatim.

### 11.7 Why this preserves every invariant we promised

| Invariant | How it holds |
|---|---|
| **Single .bhdl per component family** | The script lives in `body rhai r#"..."#`. No sidecar files. |
| **No user-managed runtime** | Rhai is statically linked. The user installs BHDL; the runtime is already there. |
| **Reproducible across machines** | The script is part of the .bhdl, captured by git. The Rhai version is pinned by BHDL's Cargo.lock. |
| **Sandboxed — vendors cannot escape** | Rhai is sandbox-by-default; we register no I/O host functions. |
| **Bounded execution time** | Fuel limit kills runaway scripts; synthesis never hangs. |
| **Rust reference still the safety net** | Declarative block / script failure → falls through to the Rust seam (when one exists). |
| **Worst-case vendor calculation is hostable** | Loops, maps, conditionals, host primitives. Combinatorial search, numerical optimisation, state machines — all expressible. |

### 11.8 Implementation roadmap (Stage 5)

1. **Lexer**: raw-string literals `r"..."`, `r#"..."#`, `r##"..."##`,
   … Captures the byte range verbatim. ≈ half day.
2. **Parser**: `inputs { ... }`, `outputs { ... }`, `body <lang>
   <rawstr>` clauses inside `design { }`. Mutual-exclusion check
   against const/require/assign statements. ≈ 1 day.
3. **AST/analyzer**: `DesignRecipe` grows a `body: Option<BodyHook
   { lang, source, inputs, outputs }>` variant. ≈ half day.
4. **Synthesizer Rhai integration**: `cargo add rhai`; build the
   `Engine` with host functions registered; marshal context, run
   script, parse map. ≈ 2 days.
5. **Migrate the amplifier**: `bhdl-stdlib/actives/triode.bhdl`
   gets `design for amplifier { body rhai r#"..."# }` — direct port
   of `ReferenceTriodeDesigner::first_guess`. The Rust trait stays
   as fallback / test seam. ≈ 1 day.

Total ≈ 5 days. Roughly the same as Stages 1–4 combined; the work
buys end-to-end coverage of the worst-case vendor calculation.

### 11.9 What we are explicitly *not* doing

- **Not hosting scipy/numpy/sympy/cvxpy**. Those are designer-time
  interactive tools, not synthesis-time vendor logic.
- **Not loading user-supplied .so / .dll / .py at synthesis time.**
  All vendor code lives in the .bhdl file, sandboxed by Rhai.
- **Not building our own scripting language.** Rhai is mature
  enough that reinventing it would be pure ego cost.
- **Not letting the body do I/O or call out**. Pure function
  `(inputs) → outputs`. Vendor wants reproducibility, BHDL wants
  no surprises.
