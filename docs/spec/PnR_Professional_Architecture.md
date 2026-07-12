# PnR: The Road to Professional Quality

Status: NORTH STAR + staged plan. Written 2026-07-12 against the
as-built engine (bhdl-pnr ~11k LOC: ePlace-class analytical placement,
PathFinder negotiated-congestion routing, typed layout-intent handshake,
IPC-2221 trace widths from solved GLACIER currents).

## 0. The honest starting point

What is already real and unusually good:

- **Placement**: analytical (LSE wirelength + ePlace electrostatic
  density via DCT Poisson, Adam, continuous rotation, progressive
  freezing, best-of-N trials). This is the modern-placer family, not a
  toy annealer.
- **Semantics into layout**: `Instance.layout_intents` (typed, no
  string re-parsing), intent → proximity/loop-area forces
  (high_freq_bypass proven end-to-end); interface constraints
  (diff-pair/length/impedance) flow through `intf_const__*`.
- **Electrically derived geometry**: solved DC currents → IPC-2221
  trace widths; net classes from solved voltages; physical part
  selection from solved stress.

What is far from professional:

1. KiCad export discards the geometry the engine computed: pads
   hardcoded 0.5×0.5, all copper on `(net 0)`, no zones/pours.
2. No geometric DRC (trace clearance, annular ring, courtyard-vs-track
   — the design_rule_checker geometric checks are stubs).
3. Routing on a 1 mm grid: valid topology, not track geometry. No
   push-and-shove, no 45° cleanup, no coupled diff-pairs, no
   serpentines.
4. Planes assumed, never synthesized (no pour, no thermal relief, no
   split awareness).
5. Impedance is a placeholder; the stackup never shapes track geometry.

The PASS gate today is "≥50% routed, no overlaps". The professional
gate is **100% routed, zero DRC, and an expert would sign it**.

## 1. The three theses (why bhdl can do what decades of EDA didn't)

### Thesis A — Semantics-first: layout rules are DERIVED, not configured

Every commercial tool starts from a dumb netlist and asks the engineer
to hand-author net classes, clearances, and length rules. bhdl already
KNOWS, from simulation and recipes:

- what every net carries (solved I → width [SHIPPED]; solved V →
  **clearance** per IPC-2221 [not yet]);
- why every part exists (expansion provenance: this cap decouples that
  pin; this divider is a feedback loop; this is the buck's hot loop —
  **the switching-loop area budget can come from the recipe**, the
  single most important power-layout rule);
- how every driven net's edge behaves (**measured IBIS rise time →
  which nets need termination, length rules, spacing**: a net with a
  1 ns measured edge gets coupled-noise budgets; a 100 kHz I2C bus does
  not — today every tool treats them identically until a human says
  otherwise);
- what each pin's return current does (the per-rail attribution work:
  **return-path continuity is checkable** because we know which rail
  sources each buffer's current).

Constraint synthesis becomes a pass exactly like value derivation:
measured electrical state → typed layout constraints, with
spec-vs-derived provenance in the sign-off. The engineer authors
intent; the toolchain authors rules.

### Thesis B — The verify-by-simulation loop: route → extract → re-measure

The measure-and-derive philosophy, closed over geometry:

1. Route.
2. Extract from REAL geometry: parallel-run coupling (closed-form
   microstrip C_mutual/L_mutual — noise-model step 2), actual loop
   areas, plane-split crossings, per-net length/impedance.
3. Re-simulate: coupled-injection crosstalk against measured aggressor
   edges (noise-model step 3), IR drop across pours, return-path
   discontinuity flags.
4. Feed violations back as router costs / placement forces; iterate.
5. Ship the board with a MEASURED SI sign-off section, not just DRC.

No shipping tool closes this loop autonomously. We already own both
ends (IBIS transient engine; extraction is closed-form geometry).

### Thesis C — Correct-by-construction geometry kernel

DRC must be the router's collision model, not a post-check. Two tiers:

- **Global**: keep PathFinder (it is the right algorithm) on a coarse
  congestion grid for layer assignment and region planning.
- **Detailed**: shape-based (or 0.05 mm fine-grid) routing where the
  cost field IS the clearance rule — a track cannot be emitted closer
  than clearance(net_a, net_b) because those cells are priced by the
  derived rules of both nets. Push-and-shove as rip-up negotiation at
  the shape level. 45°/arc cleanup as a post-pass. Pours synthesized
  as first-class objects (zones with thermal reliefs, priority fills),
  then re-checked by extraction.

Acceptance oracle: **KiCad's own DRC** (`kicad-cli pcb drc`) run in CI
over the corpus. We don't grade our own homework; an external
implementation of the IPC rules does. Gate: 0 violations, 100% routed.

### Thesis D — Learned priors, deterministic guarantees

Experts beat algorithms on placement *taste* and routing *style* —
pattern knowledge ("a buck converter is laid out like THIS"). That is
learnable, and the training data exists:

- **Corpus**: the public KiCad universe (GitHub hosts hundreds of
  thousands of real boards, many by professionals).
- **Labeling**: bhdl's OWN recipe/idiom classifiers run on their
  netlists — we can recognize buck converters, decoupling networks,
  crystal loops, diff pairs in other people's boards mechanically.
  That yields (intent-graph → expert relative placement) pairs at
  scale, back-annotated by the same vocabulary our placer consumes.
- **Models**: (a) a placement-prior model (graph → relative-position
  distributions per intent cluster) that SEEDS the analytical placer —
  the ePlace machinery then legalizes and optimizes, so ML guides and
  determinism guarantees; (b) RL over router net-ordering and cost
  weights, rewarded by our own verify metrics (routability, DRC count,
  extracted-SI margins) — the reward is computed by the deterministic
  pipeline, so the loop cannot learn to cheat physics; (c) optionally
  LLM extraction of placement notes from datasheets ("place C_BOOT
  within 2 mm of pin 5") into the existing PlacementRecipe format,
  human-reviewed like any vendor data.

ML never emits geometry directly. It proposes; the kernel disposes.

## 2. Staged plan

- **P0 — Fabrication truth** (weeks): wire real footprint/pad geometry
  and net indices into the KiCad writer; emit zones for planes with
  thermal reliefs; netnames + net classes in the file. Add
  `kicad-cli pcb drc` to the corpus sweep as the external oracle.
  Gate: every corpus board opens in KiCad with a live ratsnest and
  passes KiCad DRC (or every violation is understood).
- **P1 — Geometry kernel**: fine-grid/shape detailed routing under
  derived clearance rules; courtyard + copper DRC internally (must
  agree with the oracle); 45° cleanup; pour synthesis. Gate: 100%
  routed + 0 DRC on the corpus; retire the 50% pass bar.
- **P2 — Constraint synthesis**: voltage→clearance, recipe→loop
  budgets, measured-edge→net rules, return-path costs from rail
  attribution. Derived-rules section in the sign-off (same shape as
  derived values).
- **P3 — SI routing**: stackup impedance model → width/spacing per
  layer; coupled diff-pair routing; length-tuning serpentines against
  measured-edge budgets (a rule exists only when the physics demands
  it — Real-Data policy applied to constraints).
- **P4 — The loop**: post-route extraction → crosstalk/IR/return-path
  re-simulation → router cost feedback → measured SI sign-off on the
  shipped board. (Noise-model steps 2–4 land here.)
- **P5 — Learned priors**: KiCad-corpus mining pipeline + placement
  prior model seeding P0-P4's deterministic core; RL tuning of
  ordering/weights against P4's measured rewards.

P0/P1 are engineering. P2 is where bhdl passes every commercial tool
on automation. P4 is the trailblazing claim: boards signed off by
measurement, not convention. P5 is how it acquires taste.

## 3. Quality definition (what "expert" means here, measurably)

A board the pipeline ships must have: 100% routed nets; zero DRC by
the external oracle; every derived rule traceable to a measurement or
spec in the report; loop-area/return-path/crosstalk numbers WITHIN the
budgets the recipes declared; and a layout a reviewer recognizes as
idiomatic (P5's rubric — until then, human review on the corpus
boards). Anything not checked appears in the absence ledger. The
sign-off report is the product; the .kicad_pcb is its witness.
