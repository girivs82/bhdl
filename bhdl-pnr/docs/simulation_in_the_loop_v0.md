# Simulation-in-the-Loop P&R (Glacier ↔ Layout) — design note v0

> **Status:** Design note / proposal. Not yet implemented. Sketches the
> Phase-2 successor to the constraint model: a closed loop where the
> Glacier (bhdl-spice) simulator becomes a *cost term* in placement and
> routing, not a post-hoc verification report.
>
> **Owners (proposed split):** Glacier/bhdl-spice session owns the
> simulator + parasitic extractor + budget emission; P&R session owns
> budget consumption as constraint costs and the iteration schedule.
> Same producer/consumer shape as the intent-vocabulary ↔ constraint-model
> split (`handshake_notes.md`).
>
> **Depends on:** `constraint_model_v0.md` (the constraint catalog this
> loop writes into), `intent_vocabulary_v0.md` (intent flags which nets
> get simulated).

## 1. Thesis

Traditional EDA runs signal-integrity / power-integrity simulation as a
**verification gate**: route the board, extract parasitics, simulate,
hand a human a report; the human manually moves copper. The feedback
path *is the engineer*. The loop is open.

bhdl can close it, because of three things it has that a netlist-only
flow doesn't:

1. **Intent says which nets matter.** The simulation budget goes to the
   handful of nets flagged `switching_input_filter` / `feedback_divider`
   / `current_sense` / `high_freq_bypass`, not all 200 nets. (These are
   real shipped `intent::vocabulary` kinds — `intent_vocabulary_v0.md`
   §4. A distinct `precision_measurement` kind, if wanted for ADC
   front-ends, is a future vocabulary minor-bump, not assumed here.)
2. **Pre-layout simulation produces machine-readable budgets**, not a
   human's mental model of "keep it short."
3. **The constraint model gives the simulator a direct write-path into
   the placer's objective** (`constraint_model_v0.md`).

The result: **simulation becomes a cost term, not a report.** That is
the one-sentence thesis.

## 2. Two phases

### Phase 1 — pre-layout (topological)

Schematic-only. Glacier already does much of this: DC operating point,
value selection, decoupling sizing from simulated transient currents
(the `input_filtering` intent in `bhdl-common/src/intent.rs` sizes the
cap bank from "actual GLACIER-simulated currents"). Phase 1 answers:

> **Name-trap (two mechanisms, one component).** `input_filtering`
> (simulation-lifecycle intent, `bhdl-common/src/intent.rs`) *sizes* Cin
> from Glacier currents. `switching_input_filter` (the v0 *placement*
> `LayoutIntent`, `intent::vocabulary`) *places* that same Cin —
> proximity to VIN + hot-loop area. Same part, complementary jobs: one
> picks the value, the other places the copper. This note leans on both;
> they do not conflict.

> *Is the circuit correct and are the components sized right, assuming
> ideal (zero-parasitic) interconnect?*

**New output for the loop:** per-critical-net **performance budgets** —
but expressed as *sensitivities*, not constants (§4).

### Phase 2 — post-layout (parasitic-aware)

The same Glacier solver, on the same netlist, **augmented with
parasitics extracted from the candidate placement + routing**. Answers:

> *Does the circuit still meet its budgets once the copper is real?*

**Output:** per-net **budget violations** (this loop's ΔV exceeds
50 mV; this feedback trace added 30° phase lag; this ground return
bounces 80 mV). These violations lower into the constraint catalog and
become placement/routing cost (§3).

## 3. The bridge: violations are constraints

A parasitic-induced budget violation is **not** a new parallel
mechanism. It lowers into the same `Constraint` catalog the placer
already consumes (`constraint_model_v0.md` §3).

Today, `LoopArea` uses the cheap shoelace centroid as a *proxy* for loop
inductance. Phase 2 replaces that proxy — **for critical loops only** —
with extract-then-simulate. Same constraint kind, better evaluator. The
placer doesn't know the difference; it still sees `Eval::Violated {
cost, slack }`.

```
Phase 1:  emits  LoopBudget { loop: [Cin, U.VIN, U.SW], ripple_max_mv: 50 }
                 (really: ripple = f(L_loop); cap at 50 mV — see §4)
Phase 2:  extract L_loop from routed geometry → Glacier transient → ΔV = 80 mV
          → Constraint eval = Violated { cost: g(80 − 50), slack: 30 }
          → placer tightens Cin because SIMULATION says ripple is blown,
            not because a geometric rule fired.
```

So Phase 2 converts a subset of constraints from *geometry-approximated*
to *simulation-backed*. Nothing else in the placer changes.

## 4. Budgets are functions, not constants (derived, not authored)

**Decision: budgets are derived by Phase 1 simulation, not authored.**

The reason is correctness-under-change. An authored "loop ≤ 4 nH" budget
goes silently wrong the moment someone bumps f_sw or swaps the
regulator — it is a human's cached conclusion divorced from the thing
that produced it (the same failure mode as a netlist without intent).

So the budget is a **sensitivity**, not a threshold:

- Phase 1 emits: *ripple = f(L_loop) at this circuit's operating point,
  and the `input_filtering` intent caps ripple at 50 mV.*
- The L-budget is therefore "whatever L makes ripple = 50 mV here" —
  recomputed automatically when the circuit changes.
- Phase 2 asks the inverse question against extracted L.

**Authored override** stays as an explicit escape hatch for cases where
the designer knows something the simulator doesn't (EMC margin,
certification): `for switching_input_filter(..., loop_inductance_max:
4nH)`. Exception, not default.

### Proposed budget type (Glacier-emitted)

```rust
/// Emitted by Phase-1 Glacier for an intent-flagged critical net/loop.
/// Consumed by P&R: Phase-2 extraction + sim produces the actual metric,
/// compared against the budget to yield a constraint violation cost.
struct PerformanceBudget {
    target: BudgetTarget,          // which loop / net / pin pair
    metric: BudgetMetric,          // Ripple | PhaseMargin | IrDrop | GroundBounce | Settling | ...
    limit: f64,                    // the intent-derived cap (50 mV, 45°, ...)
    /// Sensitivity: how the metric responds to the dominant parasitic,
    /// linearized at the Phase-1 operating point. Lets P&R convert a
    /// geometric proxy delta into an estimated metric delta WITHOUT a
    /// full re-sim every step (the middle clock, §5).
    sensitivity: ParasiticSensitivity, // e.g. d(ripple)/d(L_loop)
    source_intent: String,         // provenance: "switching_input_filter@buck.bhdl:NN"
}
```

The `sensitivity` field is what makes the cheap inner loop possible: P&R
can locally estimate `metric ≈ limit_at_calibration + sensitivity ·
(proxy_now − proxy_at_calibration)` between real sims.

## 5. The iteration schedule: three clocks

Naive "simulate the whole board every placement step" is the tarpit —
it is ~10⁴× too slow (placer takes thousands of micro-steps; each sim is
ms–s). Sim-gated placement (no move without a sim) is a non-starter.

The tractable structure is **three clocks**:

| Clock | Cadence | What runs | Cost |
|---|---|---|---|
| **Inner** | every placement iteration | pure geometric proxies (shoelace loop area, trace-length estimate, parallel-run-length) | µs |
| **Middle** | every ~50 iters / placement checkpoints | extract parasitics for intent-flagged nets, Glacier on *just those subcircuits*, **recalibrate proxy weights** | ms |
| **Outer** | per full P&R pass | full extract + full Glacier on the critical-net set; acceptance gate; decide re-place | s |

**The middle clock is the innovation.** Simulation does not *drive*
placement — it *re-aims the proxy*. The placer always runs at
proxy speed; a few dozen times per run, the proxy's cost surface is
corrected toward simulated truth. Never trust the proxy absolutely;
recalibrate it against ground truth periodically.

### Proxy quality is now a first-class concern

A proxy is useful only if recalibrating it actually steers — i.e.
`d(real_violation)/d(geometry)` and `d(proxy)/d(geometry)` point the
same way even when magnitudes differ.

- **Loop area → loop inductance:** monotonic, well-behaved. Good proxy;
  middle clock can tick slowly.
- **Crosstalk:** depends on edge rate + victim impedance, not just
  parallel run length. Poor proxy; either enrich it or tick the middle
  clock faster for coupling-sensitive nets.

Pick parasitics to tackle in order of proxy quality (§7).

## 6. What Glacier grows: an extractor, not a new solver

The MNA / companion-model transient solver stays. The new front-end is
**geometry → RLC netlist**:

| Parasitic | Extracted from | Drives |
|---|---|---|
| Trace R | length × width × sheet ρ | IR drop, sense error |
| Trace L (self + loop) | path geometry + return path | ground bounce, di/dt droop, buck hot loop |
| Coupling C / mutual L | adjacent spacing × parallel run length | crosstalk, feedback pickup |
| Via L/R | count × per-via model | stitching, layer-transition cost |
| Plane Z | cut geometry, stitching density | return-path discontinuity |

**No 3D field solver for v0.** Closed-form + table-driven models
(IPC-2141 impedance, partial-inductance formulas for loops) give ~80% of
the signal at ~0.1% of the cost. A 2D cross-section solver is a v1
refinement; full 3D is v2, for the handful of nets that need it.

## 7. Sequencing (after the ATmega placement milestone)

1. **Budget type first, no extractor.** Phase-1 Glacier emits
   `PerformanceBudget` for critical nets, parallel to how it already
   emits cap-bank sizing. Cheap; proves the contract.
2. **One parasitic, closed-form: loop inductance.** Replace the shoelace
   proxy on `switching_input_filter` / `high_freq_bypass` loops with a
   partial-inductance estimate from the routed path. One parasitic, one
   constraint kind, best-behaved proxy.
3. **One real Glacier-in-the-loop eval** at placement checkpoints for
   those loops: extract L → augment netlist → transient sim → ΔV →
   constraint cost. Prove the buck hot-loop case — the placer tightens
   Cin because *simulation* says the ripple budget is blown.
4. **Generalize:** trace R (IR drop), coupling (feedback/analog nets),
   ground return (bounce). Coupling last — worst proxy.

### Ideal first target: the buck converter

As the ATmega is the ideal placement milestone, the buck converter is
the ideal sim-in-loop milestone: its hot loop is *the* textbook case
where loop inductance — not connectivity — determines whether the board
works, and its budget (input ripple, switch-node overshoot) is crisp and
directly simulatable. bhdl already has buck behavioral models and a
metadata example (`docs/examples/buck_converter_with_metadata.bhdl`).

## 8. Producer/consumer handshake

Mirrors `handshake_notes.md`:

**Glacier/bhdl-spice session owns:**
- `PerformanceBudget` type (likely in `bhdl-common`, shared).
- Phase-1 budget emission per intent-flagged critical net.
- The parasitic extractor (geometry → RLC) and Phase-2 subcircuit sim.

**P&R session owns:**
- Consuming `PerformanceBudget` + the Phase-2 metric into the constraint
  catalog as simulation-backed `Eval::Violated` costs.
- The three-clock iteration schedule (§5) and proxy recalibration.
- Which constraint kinds get the simulation-backed evaluator vs. stay
  geometric.

**Jointly owned:**
- The `ParasiticSensitivity` shape (Glacier produces it, P&R consumes it
  for the inner-loop estimate).
- Which nets are "critical" (falls out of the intent vocabulary; no new
  mechanism).

## 9. Relationship to ML (deferred)

The middle clock's proxy recalibration is exactly where a learned
surrogate eventually fits: a model mapping geometry features → predicted
budget violation, replacing the periodic real-sim with an inference. But
that is a year-2 refinement. The v0/v1 path is **analytic proxies +
periodic real Glacier**, no ML. The deterministic loop must work first —
same discipline as the rest of this project.

## 10. Out of scope for this note

- Thermal co-simulation (electro-thermal coupling) — later.
- Full-wave / EM for RF — far later; the closed-form models target
  ≤ ~hundreds of MHz digital/power SI, not RF.
- Frequency-domain S-parameter flows — the loop is transient/time-domain
  (Glacier's native mode).
- Automatic stackup synthesis from impedance targets — interacts with
  the v1 stackup model (`constraint_model_v0.md` §12), separate thread.

## 11. Synth/stdlib-side review note (2026-05-30)

> Authored by the synthesizer/stdlib session — a secondary stakeholder
> here (Glacier + P&R are the owners). The thesis, the
> violations-are-constraints bridge, budgets-as-sensitivities, and the
> three-clock schedule all read as correct and consistent with what's
> shipped. Three consistency flags + one confirmation.

1. **`source_intent` provenance converges with the deferred conflict-
   detection work — design the channel once.** The proposed
   `PerformanceBudget.source_intent` (`"switching_input_filter@buck.bhdl:NN"`,
   §4/§8) needs the same `.bhdl` file:line provenance that the
   cross-net conflict-detection decision already parked on the synth
   side (`handshake_notes.md` §10 → synth task #96). That gives the
   provenance-emit channel **two consumers**: P&R conflict diagnostics
   and Glacier budget provenance. The synth side will emit it once to
   serve both — line numbers come from the `INTENT_CALL` /
   `CONSTRAINT_STMT` syntax-tree text ranges at lowering time. No
   divergent shapes, please; coordinate the format when either consumer
   is ready to read it.

2. **`precision_measurement` (§1) is not a vocabulary kind.** The
   shipped `intent_vocabulary_v0.md` §4 measurement intent is
   `current_sense` (with `SenseTopology::Kelvin | Standard`). The
   "which nets are critical" selector (§8, "falls out of the intent
   vocabulary") must reference real kinds — either rename to
   `current_sense` or file a vocabulary minor-bump for a distinct
   `precision_measurement` kind. The other three flags in §1
   (`switching_input_filter`, `feedback_divider`, `high_freq_bypass`)
   are real and shipped.

3. **`switching_input_filter` vs `input_filtering` — complementary, not
   duplicate.** §2/§4 lean on both, and the near-identical names are a
   reader-trap. `switching_input_filter` is the v0 *placement*
   `LayoutIntent` (`intent::vocabulary`, places Cin — proximity + hot-
   loop area). `input_filtering` is the older *simulation-lifecycle*
   intent in `bhdl-common/src/intent.rs` that *sizes* the cap bank from
   GLACIER currents. Same component, two mechanisms: one sets the value,
   the other places the part. A one-line clarification in §2 would save
   a reader the double-take.

**Confirmation:** §1's "intent says which nets matter" premise is fed
by the `Instance.layout_intents` field now threaded end-to-end (commit
`4464f7a`, handshake §8.5). Phase-1 budget emission reads the same
typed intents P&R placement and routing read — so that field is the
shared input to three consumers. No new "which nets are critical"
mechanism is needed; it's already materialized on the netlist.

**Synth-side commitment:** nothing to build pre-milestone (this is
correctly sequenced after the ATmega placement milestone, §7). The one
standing commitment is #1 — the #96 provenance channel will serve the
`source_intent` field. Ping the synth session when Phase-1 budget
emission is close enough to need it.

## 12. P&R-side response to §11 (2026-05-30)

> Authored by the P&R session. All three flags accepted; two are fixed
> in-doc, one is an architectural agreement recorded here.

- **Flag #2 (`precision_measurement` not a kind) — fixed.** §1 now lists
  `current_sense` (a real shipped kind) and notes `precision_measurement`
  as a possible future vocabulary minor-bump, not an assumed kind. Good
  catch — the "which nets are critical" selector must reference real
  `intent::vocabulary` variants only.
- **Flag #3 (`switching_input_filter` vs `input_filtering` name-trap) —
  fixed.** §2 gains a one-paragraph clarification: `input_filtering`
  *sizes* Cin (simulation-lifecycle intent), `switching_input_filter`
  *places* it (placement `LayoutIntent`). Same part, complementary jobs.
- **Flag #1 (one provenance channel, two consumers) — agreed, and it's
  the right call.** `PerformanceBudget.source_intent` and the §9
  conflict-detection `.bhdl` file:line provenance (handshake §10 → synth
  #96) must be the **same emit channel**, not divergent shapes. The
  format is the synth side's to define (you own the syntax tree). When
  either consumer is first to need it — likely P&R's conflict pass before
  Glacier budget emission, given §7's sequencing — we coordinate the
  shape then. I'll consume whatever single format you emit:
  `ConstraintSource` (constraint_model §6) already has `file` / `line`
  fields ready to receive it, and `PerformanceBudget.source_intent`
  should carry the same struct rather than a bespoke string. Noted as a
  shared dependency; neither side builds it pre-milestone.

**Confirmation acknowledged:** the `Instance.layout_intents` field
(commit `4464f7a`) as the single shared input to three consumers
(placement, routing, Phase-1 budget emission) is exactly the leverage
this note's §1 premise rests on. No separate "critical net" mechanism —
agreed and already true.

**Net:** the sim-in-loop note is now consistent with shipped vocabulary
and the provenance story is unified with the conflict-detection thread.
Still a forward-looking design note, not a v0 contract; opens as a real
handshake when Phase-1 budget emission begins.
