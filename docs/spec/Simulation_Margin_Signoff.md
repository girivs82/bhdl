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
   Is DC-only sign-off acceptable for v1, with transient deferred? Proposed: yes,
   DC v1; document the gap loudly in the report.
