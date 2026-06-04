# Vendor `simulation {}` Blocks — Device Simulation IP as Authored HDL

> **Status:** Proposal. The stress/ripple surface (§4) is built first, for the
> margin sign-off loop (task #5, see `Simulation_Margin_Signoff.md` §11). The
> device-model surface (§5) is specified here for coherence but its
> implementation — migrating the hardcoded regulator/BJT/triode models out of
> `bhdl-spice` — is a deferred follow-up.

## 1. Motivation — keep GLACIER generic; device IP belongs to the device

GLACIER is a *generic* circuit solver: modified-nodal-analysis DC (and, later,
AC/transient), plus device-agnostic physics — "an inductor is a DC short",
"resistor power = V²/R", "capacitor voltage = node difference". That set is
finite and universal; it belongs in core.

Everything past that is **device IP**, and there is a lot of it:

- *how to stamp a device into the solve* — a switching regulator is a
  controlled `V_OUT` source with an efficiency-scaled `V_IN` draw; a triode is
  a Koren model; a BJT is Gummel-Poon. Today these live hardcoded in
  `bhdl-spice` (`netlist_converter`, `model_extractor`, the model factories).
- *how to stress its support components* — a buck's output inductor sees a
  ripple current `ΔI_L` set by `f_sw` and `L`; its output cap sees a ripple
  voltage `ΔV_out`. Today these would have to be hardcoded in `signoff.rs`.

Multiplied across every vendor part, baking this into GLACIER bloats it and
pins the IP behind core PRs — the exact problem `design {}` blocks
(`Vendor_Design_Blocks.md`) already solved for *sizing*. The same split applies:

| Owner | Owns |
|---|---|
| **BHDL core / GLACIER** | the solver, device-agnostic physics, the operating point it produces, the generic V/I/P stress, the margin + stepping framework |
| **Vendor stdlib** | how *this device* stamps into the solve, and how *this device* stresses its support components — declared in HDL, beside the entity |

The device tells GLACIER what it is, instead of GLACIER pattern-matching the
topology (fragile) or carrying a model per part (bloat).

## 2. The block — third sibling to `design {}` and `expansion {}`

```
entity TPS54331(v_out, v_in, i_out_max, f_sw, …) {
    design    { … }   // seed L/C/R values from electrical targets
    expansion { … }   // the support topology (L, C_in, C_out, divider, …)
    simulation {
        // §5 — how GLACIER stamps THIS device (deferred build)
        model {
            node VOUT source = self.v_out;             // controlled output
            node VIN  draws  = i_out * self.v_out
                               / (self.v_in * efficiency);
        }
        // §4 — how THIS device stresses its support parts (build now)
        stress {
            const duty   = vout / vin;                  // operating point in
            const d_il   = (vin - vout) * duty
                           / (self.f_sw * L_out.value);  // ΔI_L
            L_out.i_peak  = i_out + d_il / 2;            // inductor current stress
            C_out.v_ripple = d_il / (8 * self.f_sw * C_out.value);
            C_in.v_ripple  = i_out * duty * (1 - duty)
                            / (self.f_sw * C_in.value);
        }
    }
}
```

Same evaluation surface as `design {}`: self-readable constructor params
(`self.f_sw`), `const` locals, `require` guards, and assignments — here to
**`child.stress_axis`** outputs (`L_out.i_peak`, `C_out.v_ripple`) rather than
`child.value`. `design {}` and `simulation {}` deliberately share formulas (the
ripple forms are the same the design block used to *seed* the values); a future
refinement may let `simulation {}` reference `design {}` consts directly.

## 3. Inputs available to the block

Resolved before evaluation and exposed as read-only bindings:

- **operating point** — `vin`, `vout` (and any named net's solved voltage),
  from the GLACIER DC solve of the snapped netlist.
- **load** — `i_out`, from the output rail's **declared current budget**
  (`power VOUT = 5V @ 2A` → `i_out = 2A`). The budget on a `power` decl is the
  load specification; this is where it is consumed.
- **self params** — the entity's constructor values (`self.f_sw`, `self.v_out`,
  efficiency, …), identical to `design {}`.
- **children** — each expansion child by name, with its snapped `.value` and a
  settable stress axis (`.i_peak`, `.v_ripple`, `.i_rms`, …). Roles are by the
  child's own identity in the block — no topology guessing.

If a needed input is absent (no regulator, no `f_sw`, no output rail), the block
does not evaluate and the parts fall back to GLACIER's generic DC stress —
ripple is purely additive head-room.

## 4. Stress surface (build now — for task #5)

### 4.1 What it produces

A map `child_refdes → stress_override`, where the override refines the generic
DC stress GLACIER computed:

- `i_peak` → the inductor's **current** stress becomes the peak (was DC avg);
  margin `= I_sat_rating / (i_peak · IND_CURRENT_DERATE)`.
- `v_ripple` on an output cap → **total** voltage stress `v_out + v_ripple/2`
  for the voltage gate, and the ripple is checked against the design target
  (`ripple_v`); on an input cap, `v_ripple` vs `ripple_v_in`.
- `i_rms` (optional) → a cap ripple-current gate where the part declares one.

### 4.2 How the sign-off loop consumes it

In `compute_signoff` (the `--simulate` path):

1. GLACIER solves the snapped netlist → operating point (already done).
2. For each instance that is (or expands from) an entity with a `simulation {}`
   `stress` block, evaluate the block at the operating point → per-child
   overrides.
3. Fold overrides into the per-part stress before the margin/verdict (§4 of the
   margin spec). The report gains a **Ripple** column naming the binding
   quantity (`ΔI_L=…`, `ΔV_out=…`).

Parts not covered by any block keep their generic DC stress unchanged. The
open-loop (`bhdl bom` without `--simulate`) path is untouched.

### 4.3 Stepping (task #5 Stage C)

A reactive part over its ripple target steps **up** the E-series (larger `L`/`C`
⇒ less ripple — monotone, so the direction is known from physics, no probing).
Each step re-evaluates the *analytic* block (cheap; no GLACIER re-solve, since a
reactive value change doesn't move the DC rails) until the part signs off or
hits the E-series ceiling → DNP + loud-warn per the margin spec §7.

## 5. Device-model surface (spec now, build deferred)

### 5.1 What it expresses

How GLACIER stamps the *active* device into the solve — the thing currently
hardcoded in `bhdl-spice`. A `model {}` sub-block declares, per device, the
branches/sources it contributes:

```
model {
    node VOUT source = self.v_out;     // ideal/controlled voltage source
    node VIN  draws  = <expr>;         // current draw (a current source to GND)
    // …or, for a transistor, a named built-in model + its parameters:
    builtin koren { mu: self.mu, ex: self.ex, kg1: self.kg1, … };
    // …or, wrap a real vendor simulation model and adapt it:
    vendor spice "models/tps54302.lib" subckt TPS54302
        map { VIN: VIN, SW: PH, GND: GND, FB: FB, EN: EN, BOOT: BOOT }
        params { fsw: self.f_sw };
}
```

Three forms, in **decreasing fidelity / increasing availability**:

1. **Vendor model** — wrap a real model the vendor ships (`spice` subckt /
   `.lib`, behavioral `verilog_a`, an `ibis` buffer, a tabulated efficiency or
   ripple curve). The block `map`s the vendor model's pins to the entity's pins
   and binds whatever params it can. This is the vendor's deep IP used verbatim.
2. **Builtin** — name a core numerical model (`koren`, `gummel_poon`,
   `shichman_hodges`) and supply parameters. Core owns the device-*class*
   physics; the parameters/choice are vendor IP in HDL.
3. **Primitive composition** — describe the behaviour ourselves with sources /
   branches between the device's named pins (a regulator as `V_OUT` source +
   `V_IN` draw). This is *our own data about how the part should behave* — the
   analytic reference, authored when no vendor model exists.

### 5.1.1 "Work with whatever we have" — graceful degradation

The three forms are not exclusive; an entity may declare several, and **GLACIER
uses the richest one whose inputs it can actually satisfy** from the netlist and
operating point. The block declares, per model variant, what it *needs* (pins,
params, a model file on disk); the solver binds what the design provides and
picks the best satisfiable variant, falling through:

```
vendor model present AND its file/pins/params resolvable   → use it
   else builtin with all params bound                      → use it
   else our primitive/analytic composition                 → use it
   else generic core stamping (today's hardcoded default)  → use it
```

So a board that ships the vendor `.lib` gets the vendor's exact model; the same
entity on a board without it still simulates via our analytic composition; and a
bare instance with neither still gets the generic default. Nothing *requires* the
vendor model — it is an upgrade when available, never a hard dependency. The same
ladder applies to the **stress** surface (§4): a vendor-supplied derating/ripple
model is used if present, else our reference ripple formulas, else generic DC
stress. This is the supply-chain-provider philosophy applied to simulation: a
pluggable, best-available model, with an always-present in-tree fallback.

### 5.2 Migration path

`netlist_converter`'s hardcoded regulator decomposition, and `model_extractor`'s
class→model routing, become the **fallback** for entities without a `model {}`
block. An entity that declares one overrides the fallback. Over time the stdlib
entities each grow a `model {}` block and the hardcoded per-device code in
`bhdl-spice` shrinks to the generic builtins + the MNA stamper. No big-bang
rewrite; the converter keeps working for un-migrated parts.

### 5.3 Why deferred

The stress surface (§4) delivers task #5 and is independently valuable and
low-risk (post-solve, additive). The model surface touches the converter's hot
path and the existing model factories — a larger, separately-validated refactor.
Specifying it now keeps the two surfaces coherent (same block, same input
environment, same evaluator) so the deferred half slots in without redesign.

## 6. Evaluation, determinism, fallback

- **When:** stress block — after the post-snap DC solve, inside the sign-off
  loop. model block — during netlist→circuit conversion, before the solve.
- **Determinism:** pure function of (netlist, operating point, catalogue); no
  RNG/clock. Same inputs → same overrides → reproducible sign-off, lock-stable.
- **Fallback:** no block, or unresolved inputs → generic GLACIER behaviour
  (DC stress; hardcoded device model). Blocks are strictly additive; nothing
  that works today regresses.

## 6A. Stability (control-loop) surface — device IP, deferred build

DC operating point and ripple (§4) are *static* checks. A switching regulator
also has a **control loop** that must be stable, and stability is **not
generic**: it depends on the regulator's error-amp transconductance, the
modulator/PWM gain, the internal or external compensation, *and* the external
network (`L`, `C_out` and its ESR, the feedback divider). Only the device knows
its loop; so the loop model belongs in the `simulation {}` block, as a third
surface beside `stress {}` and `model {}`:

```
stability {
    // the device's control-loop model — vendor IP
    error_amp   gm = self.gm_ea, ro = self.ro_ea;
    modulator   gain = self.v_in / self.v_ramp;       // or current-mode Gm_power
    compensation type3 { rc: Rc.value, cc: Cc.value, cp: Cp.value };
    // external plant assembled from the expansion children + ESR
    plant buck { l: L_out.value, c: C_out.value, esr: C_out.esr, rload: vout/iout };
    // targets the sign-off checks
    require phase_margin >= 45deg;
    require gain_margin  >= 10dB;
    require crossover    in (f_sw/20 .. f_sw/5);
}
```

The analysis is an **AC small-signal** loop sweep (Bode of `T(s) = plant ×
compensation × modulator`), reporting crossover frequency, phase margin and
gain margin against the `require`d targets — the same verdict bands as §4 but on
loop margins. Like §5, the loop *math* for a class (voltage-mode / current-mode
buck, type-II/III comp) is a **core builtin**; the *parameters and topology
choice* are the vendor's, in HDL. Provenance ladder still applies: a vendor that
ships a measured/AC `.lib` loop model is used verbatim; else the analytic builtin
from these parameters; else stability is reported **unchecked** (never silently
"passed").

### 6A.1 Coupling with the ripple stepping — the important consequence

Ripple-stepping (`Simulation_Margin_Signoff.md` §11.4) steps `C_out` **up** to
cut ripple, and treats that as monotone-safe. **With stability in scope it is
not:** a larger `C_out` lowers the output pole / ESR-zero and can *reduce* phase
margin. So `C_out` has a **two-sided** window — large enough for ripple/droop,
not so large (or wrong-ESR) that the loop loses margin. The stepping loop must
therefore consult the stability surface: a ripple step that would drop phase
margin below target is rejected (or the part is flagged), exactly the two-sided
treatment §11.3 already defines for divider ratios.

**Until the stability surface is built**, the ripple stepper must not present a
`C_out` increase as fully signed-off — it `log`s that loop stability was not
checked for the stepped value, so the change is never silently assumed safe.
This is the honest v1 stance; the stability surface removes the caveat.

## 7. Open questions

1. **Shared consts with `design {}`** — the ripple forms duplicate the design
   block's. Allow `simulation {}` to import `design {}` consts, or keep them
   independent (duplicated but decoupled)? Proposed: independent for v1, shared
   later.
2. **AST/evaluator reuse** — `simulation.stress` is so close to `design {}`
   (self params, consts, child outputs) that it should reuse the
   `design_evaluator` machinery with a different output binding
   (`child.stress_axis` vs `child.value`). Confirm before implementing.
3. **Operating point for the block** — pass the whole solved net-voltage map, or
   only the entity's boundary nets (`VIN`/`VOUT`)? Proposed: boundary nets by
   pin, plus any net the block names explicitly.
4. **Ripple target source** — the design targets (`ripple_v`, `ripple_v_in`,
   `ripple_ratio`) live on the entity already; the block reads them as
   `self.ripple_v`. Confirm that is the intended target channel.
