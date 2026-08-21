# Functional Safety — language and semantic model (v0)

Status: NORMATIVE for what is implemented; sections marked *(Phase n)*
describe planned extensions. Design rationale lives in
`docs/proposals/Functional_Safety_Plan_2026.md`.

Primary frame ISO 26262 (ASIL, SPFM/LFM/PMHF); IEC 61508 (SIL, SFF/PFH)
shares the same model and differs only in metric definitions and
targets.

## 1. Principles

1. **Observable effects, not per-part labels.** A safety goal declares
   which conditions on the board's nets and pins constitute each
   failure effect. The tool classifies faults by the effect they
   produce; nobody hand-labels the effect of a part failing.
2. **Measured, not assumed.** Diagnostic coverage, effect classes and
   convergence come from simulating the real netlist with the vendor's
   behavioral models in the loop *(Phase 3)*. A claimed DC may be
   declared; a measured DC always wins and both are reported.
3. **No invented data.** FIT, failure-mode fractions and DC come only
   from a named source (datasheet, vendor safety manual / qualification
   report, named handbook table, field data). Missing data is a
   reported **gap**, never a default.
4. **Three kinds of part** *(Phase 2)*: behavioral model with declared
   failure states; black box with SEooC data (λ, classes, internal DC,
   assumptions of use, terminal contract); or nothing (gap, QM-only).
5. **Deterministic.** The same source regenerates the same report
   byte-for-byte.
6. **Same language, separate audience.** Safety is written in ordinary
   bhdl — the same nets, handles, units and expressions the board uses
   — in a `safety <Board> { }` block that lives beside the board (same
   file or a sidecar file), exactly as `layout <Board> { }` does for
   mechanical intent. The board file never has to contain safety text.
7. **The design entity IS the safety part.** Safety engineers used to
   draw a box around part of a flat schematic and call it a safety
   part. bhdl is hierarchical: the box is the entity instance. Goals,
   effects and mechanisms attach to instances by handle path
   (`ch1.u`, `reg`), analysis scopes to the instance subtree, and a
   part added inside an entity belongs to every instance of that
   entity — "which safety part did the change land in" is answered by
   the hierarchy, never by a person.

## 2. Syntax

### 2.1 The `safety` block — one new top-level form

```bhdl
safety SupervisedReg5V as dut { ... }                       // short form: block named after the entity
safety Reg5V_ASIL_B of SupervisedReg5V as dut { ... }       // long form: a differently named analysis of the entity
safety DualRail as brd { ... }                              // boards are entities too
```

`safety <Name> [of <Entity>] as <ns> { ... }` links a safety block to
an entity (a board is an entity) **explicitly** and exports the design
under the namespace `<ns>`: every design handle, port and net is
reached as `<ns>.<handle>` (`dut.mon`, `dut.VOUT`, `brd.rail_a.mon`).
Nothing in a safety block is referred to without being visible; an
`<ns>.x` that the entity does not have is a hard error. `of <Entity>`
may be omitted when the block's name is the entity's name. Several
blocks for the same entity merge into one analysis.

The block may sit in the same file as the entity or in a sidecar that
imports it (`import { SupervisedReg5V } from "reg.bhdl";`). Either way
the entity's file never has to contain safety text. The analysis
travels with the entity: every instance carries it (the SEooC idea at
block granularity), and a parent block composes the instances' goals
and assumptions through the namespace (`brd.rail_a.SG_OV`).

### 2.2 Goals — library instances or inline

Reusable, parameterised goals live in library files and are imported
like entities:

```bhdl
// bhdl-stdlib/safety/power_rails.bhdl
safety_goal RailOvervoltage(vmax: voltage, level: asil = ASIL_B)
    "No undetected overvoltage on the rail"
{
    signal RAIL: power;     // formals — bound per instance, same shape as entity ports
    signal FLAG: signal;
    effect overvoltage = RAIL > vmax               severity S3;
    effect silent_ov   = RAIL > vmax && FLAG == 1  severity S3;
}
```

```bhdl
import { RailOvervoltage } from "bhdl-stdlib/safety/power_rails.bhdl";

safety SupervisedReg5V as dut {
    SG_OV: RailOvervoltage(vmax=5.5V, level=ASIL_B)
               { RAIL: dut.VOUT; FLAG: dut.nFAULT; }
               (id="SG-REG-001", ftti=10ms, safe_state="nFAULT low; load shall disable");
}
```

`<Name>: <Goal>(<params>) { <formal>: <ns>.<handle>; ... } (<kwargs>);`
— params in parens, the goal's formals bound in braces, per-instance
metadata (`id`, `ftti`, `safe_state`, `standard`) in the trailing
kwargs. Unbound formal, unknown formal, unknown kwarg: hard errors.

Inline one-off goals:

```bhdl
    goal SG_SUPPLY: ASIL_B "Neither 5V rail overvolts undetected" (id="SG-SYS-010", ftti=10ms) {
        effect any_ov = brd.V5_A > 5.5V || brd.V5_B > 5.5V   severity S3;
    }
```

Level: `ASIL_A`..`ASIL_D`, `QM`, `SIL1`..`SIL4` (standard implied;
`standard=iec61508` explicit if wanted). Refinement follows the
hierarchy — `<ns>.<inst>.<Goal> refines <Goal>;` in a parent block —
which is the ISO safety-goal → FSR → TSR → HSR ladder on board →
entity → sub-entity.

### 2.3 Effects

`effect <name> = <expr> severity <S0|S1|S2|S3>;` inside a goal. The
expression grammar is the board's own (`when (cond)`, `stress`):
`<ns>.<net|port|inst.PIN>`, numbers with units, comparisons,
`&&`/`||`/`!`. *(Phase 3)* adds `for > <duration>`. Duplicate effect
names within a goal are errors.

### 2.4 Mechanisms

```bhdl
    mechanism dut.mon: psm(SG_OV, detects=[overvoltage, silent_ov], dc=0.90,
                           source="TPS3700 datasheet §7.3");
    mechanism dut.wdt: lsm(SG_OV, protects=dut.mon, interval=100ms, latency=1ms,
                           dc=0.85, source="TPS3430 datasheet §8.2");
```

`mechanism <ns>.<handle>: psm(<Goal>, ...) | lsm(<Goal>, ...);` —
"this design element IS a mechanism for that goal". Fields: `detects`
(effect names, required for `psm`), `protects` (mechanism handle,
required for `lsm`), `dc` (claimed 0..1), `source` (required with `dc`;
claimed DC without source = gap `DC_UNSOURCED`), `interval`/`latency`
(`lsm`), and `detected_when` — a voltage predicate over design handles
that is TRUE on a faulted operating point exactly when this mechanism
has detected the fault (`detected_when = dut.nFAULT < 1V`). Unknown
fields/goals/effects/handles: hard errors.

**Measured DC** *(Phase 3, implemented)*: the whole-universe campaign
generates the automatic fault set — every unwaived physical part × its
standard modes (2-pin parts: `short` + `open`, and value-carrying
parts additionally `drift_high`/`drift_low` — parametric drift beyond
tolerance is its own failure mode in the FMD-91-family mode splits.
The endpoints are NOT drift: R→∞ *is* the open mode and R→0 *is* the
short mode, already carried with their own λ, so a "drift to worst
case" row would re-solve identical netlists and double-count λ. Each
drift direction is one λ mode (equal 4-way split, labelled) probed at
two magnitudes: the part's declared `tolerance` edge (real data when
the attribute exists) and the 0.5×/2× de-facto FMEDA convention point
(labelled as convention, not vendor data); the row keeps the
worst-classified probe. When a probe is dangerous and the scope has a
`detected_when` mechanism, a **detectability sweep** bisects the
boundaries of "dangerous && undetected" across the probed range plus
coarse extensions toward (never onto) the endpoints, and the row's
note reports the undetected window — e.g.
`sweep: UNDETECTED dangerous window +90%..+900% (to probe limit)` —
the region drift analysis exists to find: drifted enough to violate
the goal, not enough to trip the detector; multi-pin parts: per-pin
`open` plus `short_adjacent` solder-bridge faults between ADJACENT
pins. Adjacency is **geometric**: the part's package resolves through
the same ladder PnR uses (`package` attribute → `physical_package` →
class default) and the bridge pairs are the pads within 1.5× the
package's minimum pad spacing — so a SOIC's pin 4↔5, consecutive in
number but on opposite corners, is correctly absent. No layout run is
involved; footprint generation is a pure function. A vendor
`.kicad_mod` takes precedence over the parametric registry — the
`footprint` attribute (or the package name) is tried as a path ending
in `.kicad_mod` relative to the board source, else as
`<name>.kicad_mod` under `$BHDL_FOOTPRINT_DIR` and
`<board dir>/footprints/`; a loaded file's real pad coordinates feed
the same criterion and the loaded path is printed. Parts whose package
does not resolve fall back to consecutive-pins-in-definition-order,
and every bridge row's note names which basis produced it. Declared
behavioral failure states run the vendor's states instead of the
generic modes; bridges still apply to behavioral parts — they are
board-side faults — but carry no λ weight, since the die's
failure-state FITs do not cover a solder bridge, and the note says
so) — solves
each faulted board, and classifies it on the faulted operating point:
**dangerous** (an effect fired), **detected** (a mechanism's
`detected_when` was TRUE), **residual** (dangerous, undetected),
**false alarm** (detection with no effect — a mechanism self-fault or
spurious trip). Measured DC per mechanism = detected dangerous weight /
dangerous weight over the faults whose fired effects intersect the
mechanism's `detects` list; weight = the part's computed FIT split
equally over its modes when the reliability engine produced one (the
equal split is labelled — mode fractions are data we do not have),
count-basis otherwise, also labelled. A measured DC below the claim is
flagged; both are always reported. A mechanism without `detected_when`
cannot have a measured DC, and the report says so.

**FMEDA export**: `bhdl-cli <file> safety --fmeda <path.csv>` writes
the assessor package — the per-fault worksheet (one row per universe
fault: scope, part, entity, part λ + its full FIT basis citation,
mode, targets, λ share, classification SAFE / DETECTED_DANGEROUS /
RESIDUAL / FALSE_ALARM / LATENT / NOT_RUN, effects, detection, latent
exposure, note incl. sweep windows; parts the universe generated no
modes for still get a row), plus `<stem>_mechanisms.csv` (claimed vs
MEASURED DC side by side with sources) and `<stem>_metrics.csv`
(λ totals, SPFM/LFM/PMHF with dual-point term, targets, pass). Export
serializes the measured model — nothing is computed at export time,
and empty cells mean the datum does not exist, never zero.

**Stated exclusion — inter-part bridges**: the universe's bridge
faults are INTRA-package only. A bridge between DIFFERENT parts' pads
(or a pad and another net's track/via) is just as real, but which
pads neighbour which is a placement outcome, and this analysis does
not consume layout — enumerating such pairs without placement would
be invented data. The report prints the exclusion under the fault
universe and the FMEDA worksheet carries an `EXCLUDED
short_inter_part` row (outside the λ arithmetic), so the artifact
itself says what is not covered. Planned: opportunistic consumption
of a placement artifact when one exists.

### 2.5 Faults, waivers, assumptions

```bhdl
    fault short(dut.r_fb_bot.1, dut.r_fb_bot.2) expect SG_OV.overvoltage detected_by dut.mon within 10ms;
    fault open(dut.r_pu)                        expect SG_UV.silent_uv;
    fault state(dut.reg, "ref_drift_high")      expect SG_OV.overvoltage detected_by dut.mon;

    waive dut.c_out qm "output cap: open -> ripple, short -> UV which SG_UV covers";

    assume ASM_SUPPLY_WITHIN_ABSMAX(dut.VIN, 36V);             // from an assumptions catalogue
    assume ASM_LOCAL_007 "EN is driven, never left floating";   // inline
```

`fault <kind>(<targets>) expect <Goal>.<effect> [detected_by <ns>.<h>] [within <duration>];`
— explicit injections on top of the automatic campaign; kinds
`short(a, b)`, `open(pin|part)`, `drift(part, ±pct)`,
`state(part, "<model failure state>")`.

**The declared-fault campaign runs** *(Phase 3, first increment)*: when
the board's healthy GLACIER DC solve converges, each `short`/`open`
fault mutates a clone of the netlist (short = the two nets become one —
the surviving node is always the rail, so a short to GND never destroys
the solver's ground reference; open = the pin(s) detach / the part is
removed), the faulted board is re-solved with the same DC path, and
every effect predicate in the fault's scope is evaluated on the FAULTED
operating point. Predicates resolve against the HEALTHY connectivity
(an opened part's pin still names the net it sat on) with a net-alias
map following merges. The report shows, per fault, what fired and
whether the expected effect did:

```
short(r_top.1, r_top.2) expect SG_MID.overvoltage  → ran: fired [SG_MID.overvoltage] ✓
open(r_top) expect SG_MID.overvoltage  → ran: fired [SG_MID.undervoltage] — EXPECTED … DID NOT FIRE
```

`drift(part, ±pct)` scales the part's value attributes (every
value-carrying key — different consumers read different ones) by
1 + pct/100 and runs like any other fault. Declared drift asserts a
magnitude deliberately; the universe additionally probes its own
labelled magnitudes (tolerance edge + 0.5×/2× convention, above).

**Transient pin-disturbance states**: a chip-internal transient (a
gate-driver glitch, an SEU) cannot be simulated inside the die — but
its pin-level symptoms can, and their propagation through the board is
solved physics. A `failure_state` behavior may contain
`pulse(PIN, <V>, <duration>)` ops: the pin's net is HELD at that
voltage for the window and RELEASED after (the circuit decides the
node before and afterwards). Several ';'-separated ops are ONE fault's
correlated multi-pin symptom vector — one die event, one λ — never a
multi-point fault (that would wrongly discount a first-order λ to a
second-order product; genuinely independent coexisting faults remain
the latent double-fault probe's territory). The waveform is VENDOR
data; no declaration, no simulation — never a guessed pulse. The
state runs as a transient from the healthy operating point and is
classified over the WHOLE trace (the endpoint is healthy by
construction — endpoint classification would call every transient
SAFE): the effect fired if TRUE at any sample, with its assertion
window reported. Detection is two-path: **external** = the measured
first crossing of a mechanism's `detected_when` (a real monitor
latches); **internal** = the vendor's `detected_internally=` claim on
the failure state — a duration is the chip's reaction latency, a bare
`yes` credits detection with unverifiable timing (stated). The FTTI
verdict takes the best available path: min(external crossing + that
monitor's declared latency+interval, internal latency) ≤ within.
Without a time-domain engine the state is honestly not run, stated.

`within <FTTI>` requires the fault DETECTED (a mechanism's
`detected_when` TRUE on the faulted point); the TIME argument is,
strongest first: **MEASURED** — a transient solve of the faulted board
relaxing from the healthy operating point (the fault is the stimulus;
initial conditions = the healthy DC solution) measures the BOARD-path
settle time of `detected_when` at the detector's input. The chip
inside the detector is a black box the solve structurally cannot see
(comparator propagation, deglitch, ADC + firmware) — the mechanism's
declared `latency` is exactly the model for that segment, so the
verdict COMPOSES the terms, never substitutes one for the other:
measured board settle + declared chip-internal `latency` + declared
test `interval` ≤ FTTI (the fault line prints each term and the
integration step — the resolution). A predicate that does not settle
TRUE within 2×FTTI ⇒ MEASURED FAIL. **DECLARED** — when no
transient engine is available, the mechanism's `interval` + `latency`
budget, stated as declared. **UNVERIFIABLE** — neither, stated; an
undeclared budget is never assumed. Never detected, or the time
argument exceeds the FTTI ⇒ the FTTI check fails and the fault keeps a
gap even when its effect expectation held.

A fault clears its FAULT_UNRUN gap only when it ran AND the expectation
held AND any `within` check passed. An expectation the physics did not
produce is a first-class gap (the safety argument claimed a wrong
effect); a faulted board that does not converge is reported as
ran-without-verdict (non-convergence is information — usually a
catastrophic short), and stays a gap. Effect predicates are VOLTAGE
predicates (`brd.r_bot.1 > 8V`); an identifier that does not resolve is
a per-fault error, never a silently-false predicate.

`waive <ns>.<handle> qm "<reason>";` takes a part out of the argument
with the reason on record (ERC-waiver idiom). Every other physical
part is in the fault universe whether mentioned or not.

`assume <Id>(<args>) | <Id> "<text>";` declares an assumption of use
(typically from a SEooC catalogue or a SKALP safety manual). A parent
block discharges it: `<ns>.<inst>.<Id> satisfied_by <ns>.<h>;` or
`... waived "<reason>";`. Open assumptions are gaps.

Library assumptions *(Phase 2b — implemented)* are defined once in a
catalogue file and imported like entities:

```bhdl
// bhdl-stdlib/safety/assumptions.bhdl
safety_assumption ASM_SUPPLY_WITHIN_ABSMAX(supply: handle, vmax: voltage)
    "Supply into {supply} stays below {vmax} (absolute maximum) under all transients";
```

`assume ASM_SUPPLY_WITHIN_ABSMAX(dut.VIN, 40V);` instantiates it: the
arguments fill the `{param}` placeholders (positional or `name=value`;
missing arguments fall back to the parameter default, or are a hard
error), and design handles print qualified by the instance path
(`dut.VIN` → `rail_a.VIN`), so the report reads concretely. A
parenthesised `assume` whose `Id` matches no imported
`safety_assumption` is a hard error. Note `pin`/`net` are reserved
words — catalogue parameters use `handle` for design-path parameters.

The catalogue lives in `bhdl-stdlib/safety/` (`power_rails.bhdl` for
goals, `assumptions.bhdl` for assumptions of use). Goals are LOGIC,
not data: nothing in the catalogue claims a failure rate or a DC.

### 2.6 Complete example

See `docs/examples/safety_mockup/supervised_reg.bhdl`: a supervised
regulator entity with its own safety block, a board instantiating it
twice, the board-level sidecar composing both, and the Phase-1 report
the CLI prints for it.

### 2.7 Part failure data *(Phase 2a — implemented)*

On entities (stdlib/vendor models), in the same style as `simulation`:

```bhdl
entity TPS3700 {
    // ...
    safety {
        failure_state ref_drift_high    fit=12 of 60 source="TI TPS3700 FIT report 2023";
        failure_state output_stuck_high fit=8  of 60 source="...";
        // or, for a black box:
        seooc lambda=240 spfm=0.97 lfm=0.80 source="Vendor Safety Manual rev C";
        assumption ASM_PWR_001 "VIN within 9..16V";
        terminal VIN range 2.7V..30V below_min="reset asserted";
        config vlo=4.5V vhi=5.5V source="TI FMEDA tool v2.1, cfg=window/OD";
    }
}
```

Semantics (Phase 2a, shipped):

- `failure_state` / `seooc` / `handbook` fill the part's row in the parts
  table (`behavioral, N failure states` / `seooc λ=..` / `handbook class`),
  clearing PART_NO_SAFETY_DATA for every instance of the entity. The
  `source=` string is mandatory in spirit: the report prints it verbatim.
- `fault state(h, "name")` is validated against the target entity's
  declared `failure_state`s — naming a state the vendor model does not
  declare is a hard error (no invented failure modes).
- `failure_state … behavior="open(PIN)|short(PIN_A,PIN_B)|force(PIN, <V>)"`
  says what the state DOES, as a board-observable mutation relative to
  the part's pins (open-drain output stuck high = `open(nOUT)`; output
  shorted to ground = `short(nOUT,GND)`; push-pull stuck driving =
  `force(nOUT, 5V)` — an ideal source on the pin's net, the same
  mechanism that energises a declared rail). The fault campaign RUNS a
  state through its behavior: declared `state()` faults and the
  automatic universe both execute it; a behavioral part's universe
  modes are the VENDOR'S states — never the generic 2-pin guesses —
  each weighted by its REAL `fit=X of Y` share (actual mode fractions,
  not an equal split). A state without a behavior stays unrun with
  exactly that said. The latent double-fault probe covers states too.
- each `assumption ID "text"` is surfaced as an OPEN assumption
  `<owner>.<local>.<ID>` in the scope that owns the instance (its safety
  part, else the board); a parent block discharges it like any other:
  `brd.rail_a.mon.ASM_SUP_VDD satisfied_by brd.x;` / `waived "..."`.
- `terminal` contracts are recorded; they become fault-campaign checks in
  Phase 3.

- `config k=v … source="<FMEDA tool + configuration>";` records the
  configuration the vendor data was computed for — an FMEDA Excel or
  safety manual gives a FIT **per configuration**, not per part. Each
  declared parameter is checked against every instance's attribute of
  the same name (entity parameters reach instances through the
  attribute flow — export them: `attribute vlo = vlo;`). An instance
  whose actual configuration differs, or which does not expose the
  parameter, gets a CONFIG_MISMATCH gap: the FIT/failure split does not
  apply to it, and the fix is to regenerate the vendor FMEDA for the
  real configuration (values compare numerically when both sides parse
  as numbers, else as trimmed strings).

Safety data is read from the entity's own source file, so the CLI parses
imports transitively (relative to the board file, `BHDL_LIB_PATH`, cwd).

### 2.8 Mission profile and computed handbook FITs *(Phase 2c — implemented)*

Passives don't get vendor FMEDAs; their FIT comes from a prediction
standard's *equations* (IEC 62380 historically; IEC 61709 absorbed its
models; SN 29500 is the automotive table equivalent). Three ingredients,
all explicit, none guessed:

1. **Mission profile** — declared once, in the board's safety block:

   ```bhdl
   safety MyBoard as brd {
       mission { ambient = 55degC; on_hours = 8760; cycles = 4000;
                 environment = GM; quality = lower; }
       ...
   }
   ```

   `environment` (π_E symbol: GB/GF/GM/NS/NU/…, default GB) and
   `quality` (π_Q level: S/R/P/M/mil_spec/lower, default lower = COTS)
   feed standards whose models use them (MIL-HDBK-217F); the applied
   defaults are printed in every FIT basis. `lifetime = <hours>` is the
   service life — the exposure window of the dual-point PMHF term
   (§2.9); without it PMHF stays the single-point approximation.

   A real mission is not one ambient — it is a temperature/time
   histogram. Two ways to declare it:

   ```bhdl
   mission { profile = passenger_compartment; }         // named project tunable
   mission {                                            // or inline phases
       phase parked  { time = 90%; ambient = 23degC; powered = false; }
       phase driving { time = 8%;  ambient = 40degC; }
       phase hot     { time = 2%;  ambient = 85degC; }
   }
   ```

   Named profiles live in `mission_profiles.toml` (resolved through the
   same 3-tier overlay as the coefficient tables — the shipped file
   carries clearly-labelled EXAMPLE shapes: `passenger_compartment`,
   `motor_control`, `industrial_continuous`; a real program replaces
   them with its OEM mission spec in the gitignored `.local.toml`).
   Explicit mission items override the profile's fields. The engine
   computes the time-weighted λ over powered phases; unpowered phases
   contribute zero (the shipped models carry no dormant term — printed,
   not hidden). `time_basis = operating` (default) reports λ per
   operating hour — the FMEDA/PMHF convention; `calendar` averages over
   total life. Phases must sum to 1.0 (hard error otherwise), and the
   full per-phase breakdown is printed in the FIT basis.

   Board-level only: an entity block is applied per instance, and a
   per-instance environment would be a contradiction (hard error).

2. **Stress ratio — sim-derived, never estimated.** S = applied/rated
   from the same GLACIER DC solve and sign-off rows the margin table
   uses (a resistor's applied power over its `power_rating`). A board
   whose DC solve does not converge gets no stress and therefore no FIT.

3. **Coefficient table** — one class row per component class, each row
   individually sourced. Resolution order (first hit wins):
   `$BHDL_SAFETY_TABLES/<std>.toml` (external dir, e.g. a company
   share), then `bhdl-stdlib/safety/<std>.local.toml` (per-checkout
   overlay, **gitignored**), then the in-repo
   `bhdl-stdlib/safety/<std>.toml`. Standards data transcribed from
   licensed or registration-walled documents (IEC 62380, FIDES,
   SN 29500) is not redistributable and must NEVER be committed — it
   lives in the `.local.toml` overlay or the external dir; the repo
   carries only FIXTURE demo tables. The report prints which file each
   table was loaded from. The
   engine implements the shared equation shape
   λ = λ_base · π_T(Ea, T_ref, T_amb) · π_S((S/S_ref)^n); the standard
   is a *data* choice (`per="IEC62380"`), not a code fork — an SN 29500
   run is a new table file, not a new engine. Five model forms exist:
   `model = "arrhenius_stress"` (λ_base·π_T·π_S, the 62380/61709/29500
   family shape), `model = "mil217f_resistor"`
   (λ_p = λ_b·π_R·π_Q·π_E per MIL-HDBK-217F §9) and
   `model = "mil217f_capacitor"` (λ_p = λ_b·π_CV·[π_SR]·π_Q·π_E per
   §10; S = V_applied/V_rated, π_CV from the capacitance attribute,
   π_SR — tantalum — requires a circuit-resistance input or the FIT
   stays uncomputed), `model = "mil217f_semiconductor"` (§6 diodes /
   BJTs / FETs: λ_p = λ_b·π_T·π_A·π_R·π_S·π_C·π_Q·π_E with
   T_J = T_A + θ_JA·P — the power is sim-derived, θ_JA comes from the
   `theta_ja` attribute, and COTS default quality is "plastic") and
   `model = "mil217f_inductive"` (§11 coils/transformers:
   λ_p = λ_b·π_C·π_Q·π_E at the hot spot T_HS = T_A + 1.1·ΔT, §11.3;
   ΔT from the `temp_rise` attribute). FIT = 1000·λ_p for every 217F
   form; a missing θ_JA, power, or temp_rise is a FIT_UNCOMPUTED gap
   naming the attribute, never a guessed thermal path.

The entity declares its class once (`handbook class="res_film_low_dissipation"
per="IEC62380" source="..."` on the stdlib `Res`), and every instance
gets its own computed FIT with the full basis printed:

```
r_hot  Res  handbook res_film_low_dissipation per IEC62380: λ=0.32 FIT = 0.10·π_T(1.29)·π_S(2.45) @ S=1.23, Ta=55°C ...
```

Any missing ingredient (no mission, unconverged solve, no table, class
not in the table, unrated part) leaves the FIT uncomputed and adds a
FIT_UNCOMPUTED gap naming exactly what is missing.

**The shipped default is MIL-HDBK-217F** (`milhdbk217f.toml`): a US
government work in the public domain, so its real §9.1/§9.2 resistor
constants ship in-repo — the transcription is unit-tested against the
handbook's own printed λ_b tables. It is dated (1995) and pessimistic
for modern parts; treat it as the honest out-of-the-box backend and
override per project with your licensed standard via the `.local.toml`
overlay. **The shipped `iec62380.toml` is FIXTURE-labelled**: the equation shape
is IEC 62380's, the constants are illustrative. Transcribe the real
constants (or the Isograph RWB configuration's equivalents) into the
table and update each row's `source` before using a computed FIT in a
real FMEDA. See `tests/circuits/realistic/test_safety_fit_divider.bhdl`
for the end-to-end demonstration.

### 2.9 Measured FMEDA metrics *(Phase 3 — implemented)*

After the universe runs, each scope gets its architectural metrics —
computed from MEASURED λ, never from claims:

- **λ_total** — Σ λ shares of universe faults that ran with a computed
  FIT (a fault with no λ share or that did not run is counted as
  UNMEASURED; metrics with unmeasured faults cannot pass a target).
- **λ_residual** — dangerous and undetected (the measured campaign does
  not distinguish ISO's single-point from residual faults; both count).
- **λ_latent** — from the DOUBLE-FAULT probe: a fault on a mechanism
  part that alone is neither dangerous nor annunciated, co-injected
  with each otherwise-detected dangerous fault; if the dangerous effect
  persists undetected, the mechanism-part mode is latent.
- **SPFM** = 1 − λ_residual/λ_total (ISO 26262-5 §8.4.5),
  **LFM** = 1 − λ_latent/(λ_total − λ_residual) (§8.4.6),
  **PMHF** = λ_residual plus, when the mission declares
  `lifetime = <hours>`, the **dual-point term**
  Σ_L λ_L·λ_exposed·T_life/2 over the latent faults (second-order,
  ISO 26262-10 §8.3.3 shape) — λ_exposed is the measured Σ λ of the
  detected-dangerous faults each latent fault blinds, from the
  all-pairs double-fault probe. Without a lifetime, PMHF is the
  single-point approximation and the report says how to fix that.

Targets per ISO 26262-5:2018 Tables 4/5/6: ASIL B (SPFM≥90%, LFM≥60%,
PMHF≤100 FIT), C (97%, 80%, 100), D (99%, 90%, 10). QM and ASIL A carry
no normative targets — metrics are reported without a gate. The
strictest goal level in the scope selects the targets; a miss (or an
incomplete measurement at a targeted level) is a METRIC_MISSED gap and
fails the verdict:

```
metrics [board]: λ_total=45.7 FIT, λ_residual=12.3, λ_latent=5.3
  → SPFM=73.1%  LFM=84.2%  PMHF=12.3 FIT (targets ASIL_B: …)  MISS
METRIC_MISSED  ASIL_B metrics  SPFM 73.1% < 90% — raise coverage or reduce residual λ
```

SIL-level goals use the same residual arithmetic under IEC 61508's
names: **SFF** = 1 − λ_DU/λ_total (identical to SPFM; λ_DU = the
measured residual) gated per IEC 61508-2:2010 Table 3 assuming a
**Type A subsystem with HFT = 0** — the assumption is printed in every
SIL metrics line; a Type B or redundant architecture needs its own
targets — and **PFH** = the PMHF value gated per IEC 61508-1:2010
Table 3 (high demand / continuous mode): SIL1 ≤10⁴ FIT, SIL2 ≤10³,
SIL3 ≤10², SIL4 ≤10. IEC has no LFM equivalent (still reported,
ungated for SIL goals).

## 3. Semantic model

`bhdl_common::safety::SafetyModel`, built by the synthesizer after the
netlist exists, for each board that has ≥1 `safety` block:

- `goals[]`: name, id, title, level, standard, ftti, safe_state,
  effects → (expr, severity).
- `mechanisms[]`: handle, kind, goal, detects[], protects, claimed dc
  (+source), interval/latency.
- `faults[]`, `assumptions[]` as declared.
- `parts[]`: every physical instance (full handle path) with its
  safety-data kind: `Behavioral{failure_states}` | `Seooc{...}` |
  `None`, and the entity instance it belongs to (the safety part).
- `tree`: the instance hierarchy, so goals/mechanisms/parts group by
  entity and the delta can say "added in `Channel` → ch1..ch4".
- `gaps[]`: every reason the analysis cannot claim a goal at its
  level, each with location and a one-line fix.

## 4. Gap report (Phase 1 output)

`bhdl-cli safety <file>` prints, per goal: level, effects with/without
a detecting PSM, mechanisms (claimed DC + source), faults
(declared/run), assumptions (satisfied/waived/open), and the gap list;
then a parts table (handle, kind of safety data, source). Exit status 1
when any goal has gaps, unless `BHDL_SIGNOFF_ADVISORY=1`. Phase 1 gap
classes:

| class | meaning |
|---|---|
| `EFFECT_UNDETECTED` | failure effect with no PSM declaring it |
| `PSM_WITHOUT_LSM` | ASIL C/D (or SIL 3/4) goal whose PSM has no LSM |
| `DC_UNSOURCED` | mechanism claims a DC without a source |
| `ASSUMPTION_OPEN` | assumption of use neither satisfied nor waived |
| `PART_NO_SAFETY_DATA` | physical part with neither failure states nor SEooC data |
| `FAULT_UNRUN` | declared `fault`, campaign not implemented yet |

Metrics (SPFM/LFM/PMHF, SFF/PFH) are *not* computed in Phase 1; a
report that cannot compute them says so rather than printing a number.

## 5. Baseline and delta — change detection as a property of the design

The board and its safety analysis compile together, so the tool sees
every design change the moment it is built. Three rules make this a
change detector with no process overhead:

1. **Total coverage.** Every physical instance is in the fault
   universe whether or not anyone annotated it. A new part with
   class-level failure data (handbook resistor/capacitor modes, Phase 2)
   is analysed automatically; its faults are classified by the goals'
   effect predicates and the FMEDA regenerates.
2. **Nothing is silently unanalysed.** A new part with no failure data
   is a `PART_NO_SAFETY_DATA` gap and flips the goal's verdict — the
   report, and the exit status already in CI, is the discovery
   mechanism.
3. **Relevance is derived, not maintained.** A part is safety-relevant
   to a goal iff a simulated fault on it produces one of the goal's
   effects (or it lies on the path between a monitored net and a
   mechanism). Harmless additions (pull-ups, decoupling) classify as
   safe faults with no human step. A designer may still waive a part
   with `safety=qm` + reason on the instance (ERC-waiver idiom).

`bhdl-cli safety --baseline <file>` writes (or, if the file exists,
compares against) a baseline of `{ parts, fault universe, per-fault
classification, mechanisms, assumptions, metrics, verdict }` — the
same discipline as `freeze` for the netlist. Every build then prints
the delta since baseline:

```
since baseline reg5v.safety-baseline.json (2026-08-18, 4f740f1):
  parts      +1  R27 (Res 10k, net FB / GND)          ← new
  faults     +3  R27.open, R27.short, R27.drift
  classes    R27.short → SG_OVP.silent_ov  UNDETECTED  (new single-point fault)
  mechanisms  unchanged
  SPFM       not computed (Phase 1)          verdict PASS → FAIL (1 new gap)
```

The baseline is what an assessor signs; the delta is the impact
analysis (ISO 26262-8 §8) — produced by the build, reviewed by the
safety engineer, no re-derivation. Phase 1 already emits the baseline
for `{ parts, mechanisms, assumptions, gaps, verdict }`; Phase 3 adds
fault classes and metrics.
