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
(`lsm`). Unknown fields/goals/effects/handles: hard errors. A measured
DC *(Phase 3)* always overrides the claim; both are reported.

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
— explicit injections on top of the automatic campaign *(Phase 3)*;
kinds `short(a, b)`, `open(pin|part)`, `drift(part, ±pct)`,
`state(part, "<model failure state>")`. Listed as `FAULT_UNRUN` until
the campaign exists.

`waive <ns>.<handle> qm "<reason>";` takes a part out of the argument
with the reason on record (ERC-waiver idiom). Every other physical
part is in the fault universe whether mentioned or not.

`assume <Id>(<args>) | <Id> "<text>";` declares an assumption of use
(typically from a SEooC catalogue or a SKALP safety manual). A parent
block discharges it: `<ns>.<inst>.<Id> satisfied_by <ns>.<h>;` or
`... waived "<reason>";`. Open assumptions are gaps.

### 2.6 Complete example

See `docs/examples/safety_mockup/supervised_reg.bhdl`: a supervised
regulator entity with its own safety block, a board instantiating it
twice, the board-level sidecar composing both, and the Phase-1 report
the CLI prints for it.

### 2.7 Part failure data *(Phase 2)*

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
