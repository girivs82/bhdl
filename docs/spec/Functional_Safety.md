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
safety Reg5V {
    // ... goal, effect, mechanism and fault statements (2.2–2.5) ...
}
```

`safety <BoardName> { ... }` is a top-level item that refers to the
named board's nets (`@VOUT`) and instance handles (`mon`, `reg.EN`) by
name. It may appear in the same file as the board or in a separate
file that imports the board:

```bhdl
// reg5v.safety.bhdl — owned by the safety engineer
import { Reg5V } from "reg5v.bhdl";

safety Reg5V {
    // ...
}
```

Several `safety` blocks for the same board merge (they are parts of
one analysis); a `safety` block for a board that is not defined or not
imported is a hard error, as is any net or handle it names that the
board does not have. Everything inside is the board's vocabulary — no
abstract signals, no binding step. Handles address into the hierarchy
with the usual dotted paths: `ch1.u` is the op-amp inside instance
`ch1`, `ch1.u.OUT` its pin.

`safety <Entity> { ... }` is the same form applied to an entity: its
statements are written against the entity's own ports and internal
handles, and they travel with the entity — every instance carries the
analysis (the SEooC idea at board-block granularity). A board-level
block composes them: it may add effects over instance pins, attach
mechanisms by path, and bind the entities' assumptions of use. An
entity's `safety` block may sit inside the entity body (the vendor
ships it with the model) or in a sidecar, same as for boards.

Goal refinement follows the tree: board goals are refined by entity
goals (`goal SG_CH: ASIL_B "..." refines Board.SG_OVP;`), which is the
ISO safety-goal → FSR → TSR → HSR ladder mapped onto board → entity →
sub-entity. Traceability is the instance tree; the report and the
delta (§5) are grouped by entity and by instance.

### 2.2 Goals

```bhdl
    goal SG_OVP: ASIL_B "No undetected overvoltage on VOUT" (ftti=10ms, safe_state="EN low, output off");
    goal SG_UV:  ASIL_A "VOUT brown-out is signalled"        (ftti=50ms);
```

`goal <Name>: <Level> "<title>" (<kwargs>);` — the same shape as an
instance declaration with constructor kwargs. Level: `ASIL_A`..`ASIL_D`,
`QM`, `SIL1`..`SIL4` (the standard is implied; `standard=iec61508` may
be given explicitly). Recognised kwargs: `id`, `ftti`, `safe_state`,
`standard`, and — in an entity block — `refines=<Board>.<Goal>` (or a
trailing `refines <Goal>` clause). Unknown kwargs are hard errors (E0403
discipline).

### 2.3 Effects — predicates over real nets and pins

```bhdl
    SG_OVP.effect overvoltage = @VOUT > 5.5V                      severity S3;
    SG_OVP.effect silent_ov   = @VOUT > 5.5V && mon.nRESET == 1   severity S3;
    SG_OVP.effect no_output   = @VOUT < 4.5V && reg.EN == 1       severity S1;
    SG_UV.effect  brownout_unsignalled = @VOUT < 4.5V && mon.nRESET == 1 severity S2;
```

`<Goal>.effect <name> = <expr> severity <S0|S1|S2|S3>;` — the
expression grammar is the board's own (the one `when (cond)` and
`stress` blocks use): nets `@NET`, instance pins `inst.PIN`, numbers
with units, comparisons, `&&`/`||`/`!`. *(Phase 3)* adds the temporal
qualifier `for > <duration>`. Every effect names a declared goal;
duplicate effect names within a goal are errors.

### 2.4 Mechanisms — instance attributes, written from the sidecar

```bhdl
    mon.safety_mechanism = psm(SG_OVP, detects=[overvoltage, silent_ov], dc=0.90,
                               source="TPS3700 datasheet §7.3");
    wdt.safety_mechanism = lsm(SG_OVP, protects=mon, interval=100ms, latency=1ms,
                               dc=0.85, source="TPS3430 datasheet §8.2");
```

`<handle>.safety_mechanism = psm(<Goal>, ...) | lsm(<Goal>, ...);`
sets the attribute on the board instance from the safety block, so the
board file stays safety-free. (Inline in the board, the equivalent is
the ordinary kwarg form `mon: TPS3700(4.5V, 5.5V, safety_mechanism=psm(SG_OVP, detects=[...]))`
— same attribute, same transport as `erc_waive`.) Fields: `detects`
(list of effect names, required for `psm`), `protects` (mechanism
handle, required for `lsm`), `dc` (claimed 0..1), `source` (required
whenever `dc` is given — a claimed DC without a source is a **gap**),
`interval`/`latency` (durations, `lsm`). Unknown fields, unknown
goals/effects/handles are hard errors.

### 2.5 Fault injections and assumptions

```bhdl
    fault short(r_fb_bot.1, @GND)  expect SG_OVP.overvoltage detected_by mon within 10ms;
    fault open(reg.EN)             expect SG_OVP.no_output;
    fault state(reg, "ref_drift_high") expect SG_OVP.overvoltage detected_by mon;

    assume ASM_PWR_001 "VIN stays within 9..16V" satisfied_by @VIN;   // SEooC assumption of use
```

`fault <kind>(<targets>) expect <Goal>.<effect> [detected_by <handle>] [within <duration>];`
declares a specific injection the safety engineer insists on; the
automatic campaign *(Phase 3)* covers the fault universe regardless.
Kinds: `short(a, b)`, `open(pin)`, `drift(part, ±pct)`,
`state(part, "<model failure state>")`. Until the campaign exists these
are listed as "declared, not run" (gap `FAULT_UNRUN`).

`assume <Id> "<text>" satisfied_by <handle|@net> | waived "<reason>";`
records an assumption of use (typically imported from a part's SEooC
data or a SKALP safety manual) and binds it to the board element that
discharges it. Unsatisfied, unwaived assumptions are gaps.

### 2.6 Complete example (sidecar form)

```bhdl
// reg5v.safety.bhdl
import { Reg5V } from "reg5v.bhdl";

safety Reg5V {
    goal SG_OVP: ASIL_B "No undetected overvoltage on VOUT" (ftti=10ms, safe_state="EN low, output off");

    SG_OVP.effect overvoltage = @VOUT > 5.5V                    severity S3;
    SG_OVP.effect silent_ov   = @VOUT > 5.5V && mon.nRESET == 1 severity S3;
    SG_OVP.effect no_output   = @VOUT < 4.5V && reg.EN == 1     severity S1;

    mon.safety_mechanism = psm(SG_OVP, detects=[overvoltage, silent_ov], dc=0.90,
                               source="TPS3700 datasheet §7.3");

    fault short(r_fb_bot.1, @GND) expect SG_OVP.overvoltage detected_by mon within 10ms;
    fault open(reg.EN)            expect SG_OVP.no_output;
}
```

The board file `reg5v.bhdl` contains only the circuit. The inline form
(a `safety Reg5V { }` block in the same file) is legal and identical in
meaning; the sidecar is the recommended style.

Entity-level block — the safety part *is* the entity, and the analysis
rides with every instance (here `Channel` from the mixer; four
instances, one declaration):

```bhdl
safety Channel {
    goal SG_CH_BIAS: ASIL_A "Channel output stays within the rail" (ftti=50ms);
    SG_CH_BIAS.effect rail_hit = OUT > VCC - 0.2V || OUT < 0.2V   severity S1;   // entity ports
    r_fb.safety = qm "feedback divider — worst case is gain error, not a hazard";  // waiver with reason
}
```

A part added inside `Channel` later (say `r_snub`) is automatically in
the fault universe of all four instances and shows up in the delta
under each `chN`.

### 2.7 Part failure data *(Phase 2)*

On entities (stdlib/vendor models), in the same style as `simulation`:

```bhdl
entity TPS3700 {
    // ...
    safety {
        failure_state ref_drift_high  fit=12 of 60 source="TI TPS3700 FIT report 2023";
        failure_state output_stuck_high fit=8 of 60 source="...";
        // or, for a black box:
        seooc lambda=240 spfm=0.97 lfm=0.80 source="Vendor Safety Manual rev C";
        assumption ASM_PWR_001 "VIN within 9..16V";
        terminal VIN range 2.7V..30V below_min="reset asserted";
    }
}
```

Phase 1 only reports presence/absence of such data per part.

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
