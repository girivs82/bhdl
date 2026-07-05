# Power-Supply Synthesis — from Requirement to Signed-off BOM

> **Status:** Proposal, phased. S1 (explicit-part `supply` statement + spec
> threading + requirement sign-off rows + synthesis report skeleton) is built
> first; S2 (automatic topology/part selection) and S3 (objective ranking by
> real catalogue prices) follow. Everything below the selection layer is the
> already-verified stack: part `design{}` sizing, `expansion{}`
> materialisation, GLACIER solve, §4 `simulation{stress{}}` device models,
> margin sign-off, and the supplier plugins.

## 1. Motivation — state the requirement, not the part

Today a board *names* a regulator and the part designs itself:

```bhdl
U1: TPS54331(v_out=3.3V, v_in=12V, i_out_max=1.5A, ripple_v=30mV);
```

The part's `design {}` block sizes the inductor, capacitors and feedback
divider; `expansion {}` materialises them; GLACIER solves the operating
point; the part's `stress {}` block rates the as-built values; sign-off
gates the margins; the supplier plugins pick purchasable MPNs.

What a design engineer actually *starts* from is one level up:

> "I need 3.3 V at 1.5 A from a 12 V input, under 30 mV ripple, and it
> should be cheap."

Everything between that sentence and the BOM is derivable — topology choice
is closed-form power/efficiency math, part choice is a capability filter over
the stdlib catalogue, and the sizing/verification pipeline already exists.
The `supply` statement captures that sentence in HDL; the **synthesis
report** captures everything the engineer would have written down while
deriving the answer.

## 2. The `supply` statement

```bhdl
board Sensor {
    power VIN     = 12V @ 3A;
    power VCC_3V3 = 3.3V @ 1.5A;      // rails stay the LOAD declaration
    ground GND;

    supply @VCC_3V3 from @VIN {
        ripple_max:     30mV;          // output ripple budget (spec, gated)
        efficiency_min: 85%;           // optional — binding it excludes linears
        i_q_max:        100uA;         // optional — battery designs
        profile:        cost;          // cost | grade | balanced
        using:          TPS54331;      // S1 only — explicit part, no chooser
    }
}
```

Semantics:

- **The rails carry the electrical operating point.** `v_out`/`i_out` come
  from the target rail (`3.3V @ 1.5A`), `v_in` from the source rail. The
  `supply` block adds only the axes a rail cannot carry: ripple budget,
  efficiency floor, quiescent ceiling, selection objective. Real-Data
  Policy: a target rail with no `@ I` load is a hard error for `supply` —
  the whole derivation depends on the real load.
- **Desugaring, not new machinery.** The statement compiles to exactly the
  instantiation a board writes by hand today — chosen part + spec-threaded
  constructor args + VIN/VOUT/GND/EN wiring. Every layer beneath the choice
  is the existing, verified pipeline.
- **Spec threading.** `ripple_max` maps onto the chosen part's ripple
  constructor parameter (`ripple_v` on the buck entities); `profile` rides
  as the existing `supply_profile` attribute; `i_q_max`/`efficiency_min`
  are pure selection predicates (and sign-off checks, §5).
- `using:` names the part explicitly (S1, and the permanent escape hatch —
  an engineer overriding the chooser is a normal event, recorded as such in
  the report).

## 3. Topology and part selection (S2)

The candidate set is the stdlib itself: every scan-registered entity whose
`component_class` is a regulator class. A part's datasheet attributes — the
same ones §4 stress and the decomposition already consume — are its
capability declaration:

| Predicate (hard gate) | Attributes consumed |
|---|---|
| input range covers `v_in` | `input_voltage_min/max` |
| `v_out` reachable | fixed `output_voltage`, or adjustable `v_ref … v_in − dropout_voltage` |
| load within rating | `output_current` (derated) |
| **linear dissipation** | `(v_in−v_out)·i_out + v_in·i_quiescent ≤ power_rating / derate` |
| efficiency floor | linears: `v_out/v_in ≥ efficiency_min`; switchers: loss model |
| quiescent ceiling | `i_quiescent ≤ i_q_max` |
| ripple achievable | switchers: sized `C_out` from the part's own design equations; linears: pass |

The linear-dissipation predicate **is the §4 `self.p_diss` form** — the
selector and the sign-off gate share one physics, so the chooser can never
pick a part that sign-off then rejects for the same reason.

Ranking (soft, per `profile`): `cost` = regulator MPN price (jlcparts
catalogue) + support-part count/prices; `grade` = lowest ripple/noise/`i_q`;
`balanced` = weighted. Ties break toward fewer support parts.

Adding a part to the stdlib **is** extending the chooser — there is no
separate registry to update.

## 4. The synthesis report — the design engineer's report, generated

The candidate table is not a debug log; it is a **full design report**, the
artifact a power engineer would have produced by hand: requirements,
criteria, equations with substituted numbers, the candidate survey with
per-gate verdicts, the ranking, the winner, the support-component
derivations, simulation results, and the margin sign-off. It is large
because the engineer's report is large; that is the point. One report per
board, one chapter per `supply` statement.

`bhdl <board>.bhdl synth-report [-o out/]` emits Markdown (charts as
embedded SVG in a later phase):

1. **Requirements** — the `supply` spec verbatim, source-located, plus the
   rail-derived operating point.
2. **Topology decision** — the closed forms *evaluated with numbers*:
   `P_linear = (12−3.3)·1.5 = 13.05 W > 2 W/2 ⇒ linear infeasible`,
   `η_linear = 3.3/12 = 27.5 % < 85 % ⇒ linear excluded`, conclusion.
3. **Candidate survey** — every regulator in the catalogue, one row per
   part, every hard gate shown pass/fail *with the computed value* (the
   UNPOPULATED-diagnostic discipline applied to selection):
   `LM7805 — REJECT: p_diss 13.1 W > 1.0 W derated; XC6206 — REJECT:
   i_out 200 mA < 1.5 A; TPS54331 — PASS (all gates)`.
4. **Ranking** — objective, per-candidate scores, catalogue prices/stock.
5. **Winner and instantiation** — the desugared BHDL text, verbatim.
6. **Support-component design** — the winner's `design {}` equations with
   substituted numbers and snapped values (`L = (12−3.3)·0.275 /
   (570 kHz · 0.45 A) = 9.3 µH → 10 µH`), divider, seed-vs-snapped table.
7. **Simulation results** — GLACIER operating point; §4 stress values with
   `(stress block)` provenance.
8. **Sign-off** — the existing margin table, plus the requirement rows (§5).
9. **BOM** — MPNs, manufacturers, LCSC SKUs, unit prices, stock.

Report properties: deterministic (same inputs → same bytes, so it diffs in
review), build output (never committed), and honest — UNCHECKED axes and
chooser overrides (`using:`) are printed as such, never silently omitted.

## 5. Requirement sign-off — the spec is gated, not assumed

Each `supply` spec axis becomes a sign-off row checked against the
**as-built, snapped** values — the requirement is verified, not satisfied by
construction:

| Requirement | Achieved (provenance) | Verdict |
|---|---|---|
| ripple ≤ 30 mV | ΔV_out = 23.9 mV *(stress block)* | SIGNED-OFF |
| efficiency ≥ 85 % | 91.2 % *(loss model)* | SIGNED-OFF |
| i_q ≤ 100 µA | 85 µA *(datasheet attr)* | SIGNED-OFF |

A requirement whose achieved value cannot be computed (no stress block, no
loss model) reports UNCHECKED — per Real-Data Policy it never silently
passes.

## 6. Phases

- **S1** — `supply … { … using: <Part>; }`: parser production (contextual
  keyword, like `simulation`), desugar to instantiation + wiring, spec →
  constructor threading, requirement sign-off rows, report skeleton
  (sections 1, 5–9 — everything except the chooser's 2–4).
- **S2** — drop `using:`: capability filter + topology rule over the scan
  index; report sections 2–4 (decision math + full candidate survey).
- **S3** — `profile: cost` ranked by real jlcparts prices across the whole
  derived BOM; charts (efficiency vs load, ripple vs C_out) as embedded SVG.
- **S4a — application-circuit emission (BUILT).** A regulator IC alone is
  not a supply: the desugar emits the datasheet support parts around the
  chosen part, shape-driven by its pins.
  - Wiring shape: `VOUT -> @target` when the part has VOUT; `SW -> l_out ->
    @target` when it has a switch node (both for virtual-VOUT switchers);
    `BOOT/BST -> c_boot -> SW` with the part's declared
    `bootstrap_capacitor`; FB divider (`r_fb_top`/`r_fb_bot`) from
    `feedback_voltage` + the part's datasheet `fb_divider_bottom`; input
    and output caps across their rails.
  - Sizing: `max(ripple closed form, datasheet
    input/output_capacitor_rec/min)`, snapped to the CANONICAL E-series
    (IEC 60063 preferred-number tables for E12/E24 — 8.2uH and 47uF, not
    the rounded geometric grid's 8.3/46). A value derivable from neither
    spec nor datasheet is SKIPPED, not defaulted (Real-Data Policy) — the
    part's own T2 `check{}` rules then flag anything load-bearing that is
    missing.
  - Support instances use the TI-style designators
    (`{psu}_c_in1`, `_c_out1`, `_l_out`, …) and are stamped
    `expansion_parent={psu}`, so the part's §4 stress block resolves them
    by local name: the GENERATED supply signs off under the part's own
    stress model (`(stress block)` provenance) and the `ripple_max`
    requirement row gates against the achieved ΔV.
  - Voltage class of the rail caps is left to physical selection (it
    derates against the rail when picking the real MPN; an unpopulatable
    class surfaces as UNPOPULATED) — same convention as hand-wired boards.
- **S4b — supply trees: cascade + rail-budget propagation (BUILT).**
  Cascaded `supply` statements compose (each stage desugars against its
  declared rails; the chooser sees the intermediate rail's voltage like
  any source). The load side is made VISIBLE to the budget machinery:
  each generated regulator stamps its computed INPUT draw as `i_supply` —
  linear: I_in = I_out + I_q (exact physics); switcher: I_in =
  V_out·I_out/(η·V_in) with η from the part's declared `efficiency`
  attribute; underivable → NO stamp, and ERC016 honestly counts the
  stage among the UNDECLARED draws. ERC016 then gates every intermediate
  rail: a 5V rail budgeted at 200mA feeding a 3.3V/500mA stage reports
  "declared draws already total 371mA (psu_vcc_3v3 370.8mA)".
  Parts carrying their OWN `expansion { }` block (TPS54331 style) keep
  materializing their circuit themselves — S4a emission is for bare
  entities only (both at once would parallel two inductors on SW).
  Fixture: tests/circuits/realistic/test_supply_tree.bhdl.
  Remaining S4b scope, NOT built: shared input banks, sequencing
  constraints (the analyzer's power-sequencing pass is the substrate).

## 7. Non-goals (for now)

- Inventing regulator parts not in the stdlib (the catalogue is the universe).
- Transient-response ripple (load-step droop) — the DC+ripple forms first;
  the §5 model surface is the hook for richer dynamics later.
- Thermal simulation beyond the package `power_rating` derate (heatsink
  modelling arrives as an instance attribute override, matching how an
  engineer specifies it).
