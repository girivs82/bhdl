# Functional Safety for bhdl — reconciled plan (2026-08-18)

Status: PROPOSAL, decision-grade. Supersedes the working parts of
`Functional_Safety_ISO26262_Proposal.md` and
`Complete_Functional_Safety_Architecture.md` (both kept for history);
where this document disagrees with them, this one wins.

Primary frame: **ISO 26262 (ASIL, SPFM/LFM/PMHF)**. Shared core with
**IEC 61508 (SIL, SFF/PFH/PFD)** — same failure model, same measured
diagnostic coverage; only the metric definitions and targets differ.

## 0. What exists today (audit)

| layer | state |
|---|---|
| Grammar `safety_goal`, `fault_inject` (parser, AST `safety_hierarchy.rs`) | parses; parsed items are dropped before semantic build |
| `bhdl-common/src/safety.rs` | data types only (Asil/Sil, SafetyGoal, SafetyMechanism, FaultInjection, Derating/Redundancy) |
| `bhdl-analyzer/passes/requirement_hierarchy.rs` | example requirements from `satisfies` blocks |
| `bhdl-analyzer/passes/fmea_analysis.rs` | **placeholder**: ignores the board, five hard-coded components, hard-coded FIT and DC per type name, defaults ASIL-B — violates `Real_Data_Policy.md`; must not be enabled as is |
| `bhdl-simulation/fault_injection.rs` | fault manager + event log; not driven by any flow |
| `bhdl-synthesizer/reliability_analysis.rs` | Arrhenius/derating/MTBF math over three hard-coded profiles; off by default |
| CLI | no safety command; none of the above reachable |
| Real and running, adjacent | sign-off margins (`signoff.rs`, spec `Simulation_Margin_Signoff.md`), entity `stress` blocks, ERC + waivers, GLACIER operating point, testbenches, layout DRC/preflight |

## 1. What SKALP learned (and what transfers)

SKALP (`~/src/hw/hls`, `crates/skalp-safety`, ~33k lines, docs
`REQUIREMENTS_AND_SAFETY.md`, `implementation/AUTOMATED_SAFETY_ANALYSIS.md`,
`implementation/SAFETY_GOAL_SYNTAX.md`, `SEOOC_ASSUMPTIONS.md`).

Transferable ideas (adopt):

1. **Safety goal = observable failure effects, not per-component
   labels.** The goal declares what is *monitored* and which
   *conditions* constitute each failure effect (with severity and
   FTTI); the tool classifies every fault by which effect it produces.
   Nobody hand-labels the effect of R7 opening.
2. **Diagnostic coverage is measured by fault simulation**, not read
   from a table. Inject every fault in the fault universe, run the
   scenarios, observe the goal's monitored signals, record which
   safety mechanism flagged it. DC per effect and per mechanism becomes
   evidence; undetected sites become the gap list ("these 3 parts are
   single-point faults").
3. **Abstract goal, instantiation binding.** The safety engineer writes
   the goal against abstract signals; the designer binds them to real
   nets in one block; the compiler errors on unbound signals. Clean
   ownership split.
4. **SEooC assumptions are first-class outputs.** What the analysed
   element assumes of its environment (clock, reset, supply, sensor
   redundancy, watchdog service) is emitted as a checklist the
   integrator must discharge.
5. **Uncertainty on metrics** (FIT distributions → PMHF confidence
   intervals) rather than a single number.
6. **Auto-generated FMEDA work product** with the measurement metadata
   (faults injected, scenarios, wall time, tool version) so the number
   is auditable.

Do NOT transfer:

- SKALP's *built-in* "standard libraries" of FIT (`create_automotive_digital_library()` etc.) — constructed typical values. Its own SEooC doc labels FIT as "assumed". bhdl's Real-Data policy forbids invented rates; FIT must come from named sources.
- Gate-level fault universe. A board's fault universe is components × datasheet/handbook failure modes (open/short/drift/stuck), not stuck-at on primitives.
- Cone-of-influence symbolic propagation. On a board the propagation engine already exists: GLACIER (DC/transient) + the existing testbench/`fault_injection` engine.

## 2. The seam between SKALP and bhdl

SKALP's SEooC assumptions (ASM-CLK, ASM-RST, ASM-PWR, ASM-INP, ASM-COM,
ASM-SW) are exactly the obligations a **board** discharges: brown-out
reset, UVLO, dual oscillators, redundant sensors, E2E on interfaces,
watchdog. Design the bhdl model so that a SKALP-produced SEooC checklist
can be *imported* as board-level safety requirements and each item
traced to the board mechanism that satisfies it. This is the concrete
"chip + board" story; the shared vocabulary (safety goal, mechanism,
DC, FIT, SPFM/LFM/PMHF, SEooC assumption) is deliberately identical.

## 2a. The board fault model — two mechanisms, one simulator

A chip tool owns its primitives and can inject at every gate. A board
tool owns the *netlist* and the *vendor behavioral models* of the parts
on it; it does not own the die. The fault universe is therefore built
from two mechanisms that the same simulator (GLACIER DC/transient with
behavioral models in the loop) evaluates together:

1. **Board-side faults = netlist mutations.** A passive or discrete
   fails electrically: open, short (to a neighbour net / GND / rail),
   value drift ±x%, stuck. The board is re-solved with the mutation.
   ICs are NOT put into a failure state for these — their *healthy*
   behavioral model sees an abnormal terminal condition and reacts the
   way the vendor modelled it (FB resistor shorts → FB at 0 V → the
   regulator model drives VOUT up / hits OVP / current-limits — whatever
   its model says). The effect on the goal's monitored nets, and whether
   a supervisor flags it, falls out of the co-simulation. Nobody
   enumerates "what happens if R_FB_bottom shorts"; the model knows.
2. **IC-internal faults = model failure states.** The only way to reach
   inside a black box is for the vendor to model it: the behavioral
   model declares die and package failure modes (reference drifts high,
   output stage shorts to VIN, EN input open, bond-wire open on pin n,
   pin-to-adjacent bridge), each with its FIT share from qualification
   data and each a *state the model can be switched into*. Injection =
   simulate with that state active.

Consequences:
- The behavioral-model contract becomes the critical FuSa interface.
  A model must (a) declare its internal failure modes with sourced FIT
  to be analysable for mechanism 2, and (b) be **honest off-nominal**
  — respond physically to out-of-range terminal conditions (FB at 0 V,
  EN floating, VIN above abs-max, output shorted, a pin open) — to be
  trustworthy under mechanism 1. Most vendor models only promise the
  nominal region; one that clamps, freezes or goes NaN there would
  silently hide board faults. The tool CHECKS this before a campaign:
  a terminal-range probe sweeps each model's pins through the
  conditions the campaign will induce and reports any model that
  returns undefined/non-physical behaviour as "not FuSa-grade at pin
  X" — a gap, never a guess. A model with no declared failure modes is
  QM-only for mechanism 2 (gap), but still participates in mechanism 1
  if it passes the probe.
- Package-level modes (bond-wire open, pin bridge, thermal/solder) are
  generic per package family and come from a named package model; they
  still need a source, same rule.
- Fault operating points are stiff and far off nominal; convergence at
  them is real work and a measured outcome. A fault case that does not
  converge is reported as such (and counted), not dropped.
- The campaign is many independent off-nominal solves + mode switches:
  embarrassingly parallel, same shape as the layout trial tiers.

## 2b. Three kinds of part

Not every part gets a behavioral model — an SoC has hundreds of pins
and the vendor has already done its safety work. The part spectrum:

1. **Behavioral model + failure states** (regulators, supervisors,
   op-amps, gate drivers, …): full participation in both mechanisms of
   §2a; effects are *observed* by simulation.
2. **Black box with SEooC data** (SoC, MCU, complex ASIC): a closed
   element with vendor-published safety-manual data — λ (die/package),
   SPFM/LFM or SPF/RF/MPF class breakdown, DC of internal mechanisms,
   and **assumptions of use**. The tool does NOT simulate its insides;
   it consumes the numbers for the metrics (reported as *inherited*,
   with provenance) and turns every assumption of use into a
   board-level requirement that must be bound to a board mechanism
   (UVLO here, watchdog serviced by that, ECC supply monitored, reset
   from an independent POR) or explicitly waived with a reason — the
   ERC waiver discipline. Unbound assumption = gap. This is SKALP's
   SEooC seam made concrete and it is the ISO 26262-10 workflow.
   The boundary is still simulated: a black box needs a *minimal
   terminal contract* (supply ranges, reset/enable thresholds, what its
   safety-relevant outputs do in the safe state) so mechanism-1 board
   faults at its pins have a defined response — far cheaper than a
   behavioral model, and exactly what a safety manual tabulates. A
   missing terminal contract on a pin that matters to a goal is a gap
   at that pin.
3. **Nothing** (no model, no SEooC data): reported gap; the part is
   QM-only and the goal cannot be claimed at the requested level.

Metrics therefore mix *measured* classes (mechanisms 1/2) and
*inherited* classes (SEooC parts); the report keeps them separate so
an assessor can see which numbers were simulated and which were taken
from a vendor document.

## 3. Object model (shared core)

- **SafetyGoal** `{ id, title, standard: Iso26262 | Iec61508,
  level: ASIL A–D | SIL 1–4, ftti, safe_state, monitored signals,
  failure_effects[name → condition + severity], scenarios[] }`.
- **SafetyMechanism** `{ id, kind: PSM | LSM, bound to component(s)/net(s),
  detects: which effects, claimed_dc (optional; measured DC always
  wins), diagnostic interval/latency for LSM }`.
- **FailureModel** per component instance, two kinds: board-side
  `{ mode: open | short(to) | drift(±x%) | stuck, fraction, fit_source }`
  for passives/discretes (injected as netlist mutations); model-side
  `{ state: <vendor-declared failure state>, die|package, fraction,
  fit_source }` for ICs (injected by switching the behavioral model into
  that state). Both must be *injectable* to count. Third kind, not
  injected: `Seooc { lambda, class_breakdown | spfm+lfm, dc_internal,
  assumptions_of_use[], terminal_contract, source }` — consumed for
  metrics, assumptions turned into requirements (§2b).
- **FitSource** — every FIT/fraction carries provenance: `datasheet`,
  `vendor qualification report`, `handbook(IEC 62380 | SN 29500 |
  MIL-HDBK-217F, table ref)`, `field data`. Missing = **not analysed**
  (reported as a gap), never defaulted.
- **Analysis** `{ fault universe, per-fault outcome (effect | none |
  safe), detected_by, DC per effect/mechanism, SPFM/LFM/PMHF (26262) or
  SFF/PFH (61508), gaps, uncertainty, seooc_assumptions }`.

## 4. Language surface (minimal, reuse what parses)

Reuse the existing `safety_goal` and `fault_inject` keywords; extend
their bodies rather than adding constructs.

```bhdl
safety_goal SG_OVP: ASIL_B {
    id: "SG-001";
    ftti: 10ms;
    safe_state: "output disabled (EN low)";
    monitor { vout: net; en: net; fault: net; }
    failure_effects {
        overvoltage:  vout > 5.5V for > 100us;
        silent_ov:    vout > 5.5V && fault == 0;
        no_output:    vout < 4.5V && en == 1;
    }
    severity { overvoltage: S3; silent_ov: S3; no_output: S1; }
    scenarios { nominal: tb_nominal; load_step: tb_load_step; }
}

board Reg5V {
    ...
    safety SG_OVP { vout: @VOUT; en: @EN; fault: @nFAULT; }   // binding
    // mechanisms are attributes on instances, not new syntax:
    mon: VoltageSupervisor(4.5V..5.5V) safety_mechanism(psm, detects: [overvoltage, silent_ov]);
}
```

`fault_inject` blocks become explicit *additional* campaign entries
(specific faults the safety engineer insists on); the automatic
campaign covers the whole fault universe regardless.

Component failure data lives on entities (stdlib or vendor), e.g.
`attribute failure_modes = [...]` with `fit_source`, following the same
`data_source` discipline the stdlib already uses for electrical data.

## 5. Analysis pipeline

1. **Bind**: goal signals → nets; mechanisms → instances. Errors on
   unbound signals, on mechanisms detecting effects the goal doesn't
   declare, and on missing failure data (gap, counted).
2. **Fault universe**: for each physical instance × failure mode →
   a netlist mutation (passives/discretes) or a model failure state
   (ICs), per §2a. Pre-campaign: the terminal-range probe on every
   behavioral model; models failing it are gaps at the affected pins.
3. **Campaign**: for each fault × scenario, run the existing simulator
   (GLACIER DC / transient via `simulate`); evaluate failure-effect
   conditions on monitored nets; evaluate mechanism outputs → detected?
   Fault collapsing + parallelism as in the layout trial machinery.
4. **Classify** per fault: safe / SPF / residual / MPF-detected /
   MPF-latent, using measured detection and LSM coverage.
5. **Metrics**: SPFM, LFM, PMHF vs ASIL targets (26262-5 tables:
   ASIL B ≥90/60, C ≥97/80, D ≥99/90; PMHF 10⁻⁷/10⁻⁷/10⁻⁸ per h);
   SFF/PFH vs SIL for 61508. Uncertainty via FIT distributions.
6. **Report**: FMEDA table, gap list ("undetected: R12 open → silent_ov;
   suggestion: add LSM on supervisor"), SEooC assumptions, verdict.
   `bhdl-cli safety <file>` (and a section in `report`); exit status =
   verdict, `BHDL_SIGNOFF_ADVISORY=1` demotes, same as layout.

## 6. Non-negotiables

- No invented FIT/DC/fractions. Missing data is a reported gap.
- Measured DC always overrides a claimed DC; the report shows both.
- The oracle for the effect classification is the simulator on the
  real netlist with the vendor's behavioral models, not a lookup table
  of "typical effects". A model that is not honest off-nominal is a
  reported gap, not an assumption.
- Deterministic campaigns (fixed seeds, fixed iteration order) so a
  FMEDA regenerates byte-identically from the same source.

## 7. Phases

| # | deliverable | notes |
|---|---|---|
| 0 | this doc + `docs/spec/Functional_Safety.md` (normative subset of §3–§6) | 1 sitting |
| 1 | **DONE (c6c0eff, 345d4e6)** — grammar (`safety <Name> [of E] as ns { }`, library `safety_goal`), semantic model (`bhdl_common::safety`), `bhdl-cli safety` gap report + `--baseline` delta; placeholder FMEA/redundancy passes deleted | spec `docs/spec/Functional_Safety.md` |
| 2 | data campaign on the models the examples use: (a) terminal-range probe + honest-off-nominal fixes for behavioral models (regulator, supervisor, op-amp); (b) vendor-sourced `failure_modes` states where published; (c) SEooC blocks for one MCU/SoC from its safety manual (λ, classes, assumptions of use, terminal contract); (d) passives/discretes from a named handbook table the user provides (SN 29500 / IEC 62380 excerpts) | "the datasheet/safety manual IS the model"; no FIT table of our own |
| 3 | fault universe + campaign on GLACIER; effect evaluation; measured DC; FMEDA + gap report; verdict + exit gate | the core; reuse `fault_injection.rs`, testbench runner, trial-tier parallelism |
| 4 | LSM/latent modelling, PMHF uncertainty, IEC 61508 metrics, SEooC checklist emission + import from SKALP | shared core proves itself here |
| 5 | fixtures: an ASIL-B supervised regulator (from `docs/examples/safety_annotations.bhdl`) and one 61508 example; sweep in CI | |

First slice I would start on: Phase 1 (semantic model + `bhdl-cli safety`
gap report), because it turns the parsed-and-dropped syntax into
something real with zero invented data, and every later phase hangs
off it.
