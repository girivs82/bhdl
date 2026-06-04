# Simulation-Refined Margin & Sign-Off Loop

> **Status:** Proposal — spec for tasks #4 (GLACIER-fed sim-refined stress)
> and #5 (margin + re-sim sign-off loop). Implementation is gated on review
> of this document. The single-stage analogue already ships as
> `bhdl-spice/src/tube_bias.rs::refine` (the bias-network refine loop); this
> generalises that discipline to whole-netlist passive sizing.

## 1. Motivation

Passive sizing today is **open loop**:

```
design { } seed  →  E-series snap  →  catalogue/supply gate  →  BOM
```

The closed-form `design { }` block computes a real-valued seed (L, C, R_top,
…). `value_snap` rounds it to the nearest E-series value. The supply gate
(`apply_supply_chain_mpns`) then checks the part's *derated* stress against
catalogue ratings. Two gaps:

1. **Stress is measured at the seed, never re-verified after the snap.** The
   snap can move a value enough to matter — `C_out` rounded *down* one E-series
   step raises ripple; `R_top` rounded changes the divider ratio and the
   regulated output. Nothing re-checks the operating point of the values that
   actually land on the BOM.

2. **No margin is computed or reported.** A part either clears the derate gate
   or it doesn't. There is no per-component "rating ÷ stress" figure, no
   sign-off record, and no way to see that a part *passed but barely*.

The `tps54331.bhdl` header already documents the intended 6-step pipeline
(`seed → simulate → snap → simulate → margin → simulate`) and calls the
current entity "stage 1 only". This spec defines stages 2–6.

The bias designers (`design_amplifier` et al.) already close their loop via
`tube_bias::refine`: build the real circuit, GLACIER-solve, measure, correct
the free variables (damped, clamped), commit only if still solvable, stop on
convergence or after bounded passes — and *always return the actual operating
point reached*. This proposal applies the same discipline to passives, where
the free variables are **discrete E-series values** instead of continuous R's.

## 2. Scope

In scope: the stress-bearing passives the supply gate already classifies —
`resistor`, `capacitor`, `inductor` — whose value is either a `design { }`
output or a literal, and whose stress is one of the existing axes:

| Class | Stress axis | Existing derate (`value_snap.rs`) |
|---|---|---|
| capacitor | voltage across | `CAP_VOLTAGE_DERATE = 2.0` |
| resistor | power dissipated | `RES_POWER_DERATE = 2.0` |
| resistor | voltage across | `RES_VOLTAGE_DERATE = 1.5` |
| inductor | current through | `IND_CURRENT_DERATE = 1.25` |

Out of scope (unchanged): active-device bias (owned by `tube_bias::refine`
and the `design { }` evaluators), connectivity, package/footprint selection.

## 3. Where it sits in the pipeline

The loop wraps **snap + simulate**, between expansion and the supply-chain
MPN gate, and runs only when a GLACIER solve is possible (today: under
`--simulate`; see §8):

```
expand (seed) ──▶ ┌─────────────────────────────────────────────┐ ──▶ supply gate ──▶ BOM
                  │  snap → GLACIER solve → margin → step → loop  │      (+ sign-off
                  └─────────────────────────────────────────────┘       report)
```

It reuses, not replaces, existing machinery:

- `NetlistToSpiceConverter` + `GlacierDcSolver` — build & solve (already exist).
- `SimulationAnnotations { net_voltages, instance_currents, instance_power }`
  — the per-instance stress source (already built by `build_simulation_annotations`).
- `compute_instance_max_voltages` — voltage stress from node voltages.
- `value_snap` E-series grid + derate constants — the value lattice and the
  derate convention margins are measured against.
- The #1 DNP/loud-warn path — the terminal state when no E-series value escapes.

## 4. Margin definition

For a part with derated stress `s` (stress × derate factor) and catalogue
rating `r`:

```
slack  = r − s          (head-room in stress units; ≥ 0 is safe)
margin = r / s          (ratio; ≥ 1.0 is safe, the derate already folded in)
```

`margin ≥ 1.0` means the part clears its derated gate. We additionally define a
**sign-off band**: a part is

- **OVER_STRESS** if `margin < 1.0` (fails the derate gate),
- **UNDER_MARGIN** if `1.0 ≤ margin < SIGNOFF_MARGIN` (passes the gate but with
  less than the target head-room — e.g. `SIGNOFF_MARGIN = 1.2`, a further 20 %
  on top of the derate),
- **SIGNED_OFF** if `margin ≥ SIGNOFF_MARGIN`.

`SIGNOFF_MARGIN` is a per-class constant beside the derate factors (proposed
default 1.2 for all three classes; revisit per class in review).

### Two-sided specs

Some values are not "bigger is safer". A feedback divider (`R_top`/`R_bot`) or a
gate resistor has a **target window**: the regulated output / threshold must
land inside a tolerance band, so the sign-off objective is *distance from window
centre*, not *distance from a single boundary*. These are flagged by the
existing `tolerance`/`supply_profile = "grade"` attributes the stdlib already
stamps on precision parts. For two-sided parts the loop steps **toward the
window centre**; for one-sided parts it steps **away from the binding boundary**
(see §6).

## 5. Stage 4 — the re-simulation (task #4)

After the snap, rebuild the SPICE circuit **from the snapped netlist** (not the
seed) and GLACIER-solve. This is a straight reuse of the existing convert+solve,
the only change being *when* it runs: today the one optional solve under
`--simulate` happens once, on seed values; here it runs on the snapped values,
inside the loop. The solve yields, per instance:

- capacitor / resistor voltage ← `compute_instance_max_voltages(net_voltages)`
- resistor power ← `instance_power`
- inductor current ← `instance_currents`

These feed §4's margin computation directly — no new stress math, just the
post-snap operating point that was previously never measured.

This stage alone (re-sim + margin **report**, no stepping) is the low-risk
increment: it delivers the missing measurement and the sign-off table without
changing any value. Stage 6's stepping builds on top.

## 6. Stage 5/6 — the sign-off loop (task #5)

Mirror `tube_bias::refine`'s structure exactly, with discrete steps:

```
solve snapped netlist → annotations
for pass in 0..MAX_PASSES (proposed 8):
    margins = compute per-part margin (§4)
    if every part SIGNED_OFF: break
    trial = netlist
    for each part that is OVER_STRESS or UNDER_MARGIN:
        step its value ONE E-series position in the slack-improving direction:
            one-sided  → away from the binding boundary
                         (cap V / res P / ind I: larger rating ⇒ usually a
                          larger/again-derated value or a higher-rated family;
                          for the value itself, the direction is the sign of
                          ∂slack/∂value estimated from this pass vs last)
            two-sided  → toward the target-window centre
        clamp to the E-series grid bounds; if already at the safe extreme and
        still failing → mark DNP (§7), do not step further
    re-snap trial (values stay on-grid by construction)
    solve trial:
        Ok(op)  → commit: netlist = trial, annotations = op
        Err(_)  → break (keep last good; caller learns what it got)
return SignoffReport over the final netlist
```

Discipline carried over from `refine`:

- **Bounded passes** — `MAX_PASSES` (proposed 8; smaller than refine's 24
  because the E-series grid is coarse and convergence is in a few steps or not
  at all).
- **Commit only if solvable** — a step that makes the circuit unsolvable ends
  the loop with the last good design.
- **One step per pass per part** — no large jumps; the coupled divider/ripple
  interactions settle over passes, exactly as `R_p`/`R_k` do in `refine`.
- **Always return the actual result** — the report records the *measured*
  operating point and every value change, never a silent edit.

### Sensitivity / direction

`∂slack/∂value` is estimated **empirically** from the loop itself: the sign of
(Δslack / Δvalue) between the previous pass and this one tells which way to step.
The first pass, with no history, uses the analytic default per class (e.g. a
higher-voltage cap is a larger rating; a higher-power resistor is a physically
larger part at the same value, so the *value* often holds and only the
*family/rating* moves). This avoids a separate finite-difference solve per part
per pass (which would multiply GLACIER calls) — the loop's own trajectory is the
sensitivity signal, the same way `refine` reads gain/centring error each pass.

## 7. Terminal states & interaction with #1

- **SIGNED_OFF** — value (and family) recorded; supply gate proceeds normally.
- **Stepped to sign-off** — final value differs from the seed snap; the report
  shows seed → final and the margin at each, so the change is auditable.
- **No E-series escape (still OVER_STRESS at the grid extreme)** — the part is
  stamped DNP via the **task #1** mechanism (`dnp` + `dnp_reason` + `stress_gate`
  attributes, loud `log::warn!`). The loop never substitutes a weaker part and
  never silently leaves an over-stressed part populated.
- **UNDER_MARGIN, un-improvable within bounds** — populated, but flagged
  UNDER_MARGIN in the report with a (non-fatal) warning. The board builds; the
  designer is told it's tight.

## 8. CLI surface & output

The loop needs a GLACIER solve, so it is bound to the simulate path:

- `bhdl bom --simulate` runs the **full loop** (re-sim + margin + stepping) and
  prints the sign-off report alongside the BOM.
- A `--signoff-report` flag (or always, under `--simulate`) emits the per-part
  table: `refdes, class, seed_value, final_value, stress, derated, rating,
  margin, verdict`.
- Without `--simulate`, behaviour is unchanged (open-loop seed → snap → BOM);
  the report header notes that margins were not simulated.

Proposed constants live beside the derate factors in `value_snap.rs`:
`SIGNOFF_MARGIN` (per class), `MAX_PASSES`.

## 9. Determinism

`Date.now()`/RNG are not used; the loop is a pure function of (netlist, catalogue,
GLACIER). Given the same inputs it produces the same value trajectory and report,
so the sign-off result is reproducible and lock-file-stable.

## 10. Open questions for review

1. **`SIGNOFF_MARGIN` per class** — is a flat 1.2 (20 % over derate) right, or
   should caps (already 2× derated) sign off at exactly 1.0 while inductors
   (1.25× derate) want more? Proposed: revisit per class with one real buck.
2. **Where the loop lives** — a new `bhdl-synthesizer/src/signoff.rs` orchestrating
   convert/solve/snap, or fold into the existing `glacier_physical_selection`
   path? Proposed: new module, called from the `--simulate` arm in `main.rs`,
   so the open-loop path is untouched.
3. **Stepping the family vs the value** — for resistor power and inductor
   current, the fix is often a higher-rated part at the *same nominal value*
   (a larger package), not a different value. Should the loop step the
   *catalogue family* (rating axis) rather than the E-series value in those
   cases? This couples the loop to the supply gate ordering. Proposed: step
   value for cap-voltage / divider-ratio cases; defer family-stepping to the
   supply gate (which already filters by derated rating) and only DNP if the
   gate then finds nothing — i.e. the loop owns *value* margin, the supply gate
   owns *rating* coverage. **This is the main architectural choice to confirm.**
4. **Two-sided window source** — infer the target window from the `design { }`
   `require` clauses (e.g. `require vout < v_in`), or from explicit
   `tolerance`/window attributes? Proposed: explicit attributes first; `require`
   mining is a later enhancement.
5. **AC / transient stress** — output-cap selection is dominated by load-transient
   droop, which a DC solve doesn't capture (the `tps54331` header flags this).
   Is DC-only sign-off acceptable for v1, with transient deferred? **Resolved:
   no — DC alone is too thin to give value-stepping teeth (see §11).** The loop
   adds an analytic *ripple* stress model so a reactive part's value genuinely
   sets its stress.

---

## 11. Ripple-aware sign-off (the value-stepping that bites)

### 11.1 Why DC alone is not enough

Under a pure DC operating point, a passive's stress is almost entirely
*topological*, not value-dependent: a cap's voltage is the rail it sits across;
a resistor's power is `V²/R` whose over-stress fix is a higher *rating*, not a
different value. Both are already enforced — the supply gate filters by derated
rating and §7 DNPs when nothing clears. So a DC-only value-stepping loop has
almost nothing productive to step.

Value-stepping only *bites* where the **value sets the stress**, and for the
switching topologies this stdlib targets that means **ripple**:

| Part | Ripple quantity | Closed form | Value effect |
|---|---|---|---|
| output inductor `L` | peak current `I_pk = I_out + ΔI_L/2` | `ΔI_L = (V_in−V_out)·D / (f_sw·L)` | smaller `L` ⇒ larger `ΔI_L` ⇒ larger `I_pk` |
| output cap `C_out` | ripple voltage `ΔV_out` | `ΔI_L / (8·f_sw·C_out)` | smaller `C` ⇒ larger `ΔV_out` |
| input cap `C_in` | ripple voltage `ΔV_in` | `I_out·D·(1−D) / (f_sw·C_in)` | smaller `C` ⇒ larger `ΔV_in` |

These are **the same closed forms `tps54331.bhdl`'s `design {}` block already
uses to seed the values** (`D = V_out/V_in`). The sign-off loop runs them
*forward on the snapped values* to check what the BOM part actually delivers —
no new transient/AC SPICE solver, just the analytic ripple model evaluated at
the real operating point.

### 11.2 Operating point — recovered from the netlist, not re-entered

Everything the ripple forms need is already present:

- `V_in`, `V_out` — the GLACIER DC node voltages (the rails).
- `I_out` — **the output rail's declared current budget** (`power VOUT = 5V @ 2A`
  → `I_out = 2A`). This is why the budget on a `power` decl matters here.
- `f_sw` — the switching regulator's `f_sw` attribute (the stdlib entity stamps
  it, e.g. TPS54331 `attribute f_sw = 570kHz`).
- **component role** — by connectivity: a cap whose nets are `{V_out, GND}` is an
  output cap, `{V_in, GND}` an input cap; the inductor between the switch node and
  `V_out` is *the* output inductor. No new annotation — the rails already name
  themselves.

If a regulator / `f_sw` / output rail can't be identified (not a switching
topology, or a bare passive board), the part has **no ripple stress** and falls
back to the DC margin of §4 — ripple is purely additive head-room information.

### 11.3 Ripple-aware stress & margin

For each reactive part, the ripple model contributes:

- inductor: stress axis becomes **peak current** `I_pk` (was the DC/average
  current); margin `= I_sat_rating / (I_pk · IND_CURRENT_DERATE)`.
- output cap: **total voltage** `V_out + ΔV_out/2` for the voltage gate, *and* a
  ripple-current figure for caps that declare one; the ripple voltage `ΔV_out`
  is additionally checked against the design **ripple target** (`ripple_v`).
- input cap: ripple voltage `ΔV_in` vs `ripple_v_in`.

The verdict bands (§4) are unchanged; the *stress* fed into them is the
ripple-aware figure when a ripple model applies, the DC figure otherwise. The
report gains a `Ripple` column and names the binding quantity.

### 11.4 The stepping (now it has somewhere to go)

A reactive part **UNDER-MARGIN or over its ripple target** is stepped **up** the
E-series (larger `L`/`C` ⇒ less ripple). For the *ripple* axis in isolation the
direction is monotone, so no `∂slack/∂value` probing is needed. Each step:

1. bump the value to the next E-series position **up**;
2. recompute ripple (analytic — cheap, no solve) and the DC margin (the rails
   don't move when only a reactive value changes, so the GLACIER re-solve can be
   skipped for pure-reactive steps — a key efficiency win over §6);
3. stop when the part signs off, or at the E-series ceiling (then DNP / loud-warn
   per §7, e.g. "no standard C_out meets the 30 mV ripple target at this f_sw").

> **Caveat — stepping `C_out` is NOT monotone once loop stability is in scope.**
> A larger output cap cuts ripple but lowers the output pole / ESR-zero and can
> *reduce* the regulator's phase margin, so `C_out` actually has a **two-sided**
> window (enough for ripple/droop, not so much that the loop destabilises). The
> stepper must consult the control-loop **stability surface**
> (`Vendor_Simulation_Blocks.md` §6A) and reject a ripple step that would drop
> phase margin below target — the same two-sided handling §11.3 gives divider
> ratios. Until that surface is built, a `C_out` increase is **not** reported as
> fully signed-off: the stepper `log`s that loop stability was not checked for the
> stepped value, so nothing is silently assumed safe. The inductor `I_pk` step
> (larger `L` ⇒ less ripple current, monotone and stability-benign) is unaffected.

Resistive / DC-only parts continue to use §6's bounded re-solve loop; the two
compose — reactive parts converge analytically, resistive parts via the solver.

### 11.5 Staging

- **A. Operating-point extraction** — identify the regulator (`f_sw`), the rails
  and their `I_out` budget, and each reactive part's role. Fix the VOUT
  node-voltage reporting gap (output-side nodes must survive into the annotation
  map) as a prerequisite so output caps have a `V_out` at all.
- **B. Ripple model** — the three closed forms as a pure function
  `ripple_stress(role, op, value)`; fold into `compute_signoff` so the report
  shows ripple-aware stress + a `Ripple` column.
- **C. Stepping** — the up-the-E-series reactive loop (analytic, no re-solve),
  composing with §6 for resistive parts; DNP at the ceiling.

Each stage is independently shippable: A makes the buck output side report real
numbers; B makes them ripple-aware; C makes them self-correcting.
