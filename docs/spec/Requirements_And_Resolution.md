# Requirements, Blocks, and Resolution — the library model

Status: design record (settled in discussion 2026-08-23). Increment 1
(the TPS54331 part/block split, `bhdl-stdlib/power/tps54331.bhdl`) is
LANDED and is the template; increment 2 (requirement interfaces +
resolver + lock + override) is LANDED; increment 3 (ids, `satisfies`,
the trace matrix — `bhdl trace`) is LANDED. Companion to
`Expansion_Vs_Hierarchy.md`, which settled composition under `entity`.

## 1. The two-layer library

Today's stdlib regulator entity conflates two things: the vendor's chip
and the designer's application circuit, in one body with an
`expansion { }` recipe. The composition work (`as part | design`,
plain-body composition) makes the split expressible; this document
makes it the library model.

- **`entity X as part`** — vendor truth. Exact pins and package;
  datasheet attributes (ratings, dropout, `output_noise`, `efficiency`,
  thermal). No application circuit. Owned by the vendor, or proxied by
  the designer reading the datasheet. The Real-Data Policy applies at
  this layer exactly: every number cites its source.
- **`entity Y as design`** — the designer's subcircuit. Instantiates the
  part, adds the application circuit, exposes the contract pins
  (`VIN / VOUT / GND`, optional `EN`, `PGOOD`, …). Built once,
  reviewed with the vendor, parameterized, reused across designs. Its
  provenance is honest about what it is: *the designer's reading of
  the vendor's design procedure, reviewed against datasheet rev X*.

On this model a design block's internals are firm by nature and the
vendor-recipe "board takeover" (ERC029) stops being needed: a designer
who wants a different application circuit parameterizes or derives a
different block. `expansion { }` is the migration-era form that splits
into part + block; it keeps working until the library has migrated.

## 2. How one block flexes — the generalization ladder

The datasheet's design procedure already **is** a parameterization:
given Vin range, Vout, Iout, ripple, fsw, it tells you the inductor,
the output capacitance, the divider, the compensation. That procedure
is the block's generality. In language terms:

1. **Parameterize by requirements, derive the internals.** The block
   takes application facts (`vout`, `i_max`, `vin_min/max`, `ripple`,
   `noise`) and its `design { }` computes the children. A parameter
   earns its place by being an input to the datasheet procedure or an
   application requirement — never a part value, never a topology
   switch. This is the discipline that keeps a block from becoming a
   configuration swamp.
2. **Wiring-gated optional features.** `EN`, `PGOOD`, `SYNC`,
   soft-start: the board's wiring is the configuration. For design
   blocks this should be explicit — `generate if wired(EN) { … }` —
   not an engine heuristic. (`wired(pin)` is a predicate to add.)
3. **Explicit feature parameters only for genuine topology
   differences** (sync vs non-sync, external clock). Few, and
   expressed with `generate if`.
4. **A validity envelope, enforced.** `where` clauses state the block's
   generality: `where vin >= 4.5V, i_max <= 2.4A` (rating × derate).
   Instantiation outside the envelope is a hard error — never a silent
   bad circuit. This is what makes "reuse anywhere" safe.
5. **Promises on the boundary.** The block declares what it delivers
   (`output_noise`, `efficiency`, `i_rating`, `vout_nom`) derived from
   its part and parameters. ERC032, power-tree emission, and drift
   detection read the block's promises; the regulator pin contract and
   swap-by-rename apply unchanged because they were always about the
   port surface.
6. **Proof obligations travel with the block.** Ripple, stability
   margin, thermal checks are parameterized sign-off items; the board
   flattens, so they run per instantiation.

**The generality boundary.** One block per *part family*. The variations
a block absorbs are the ones the datasheet procedure parameterizes;
past that it is a different block. The exception that proves the rule:
fixed-LDO families with structurally identical application circuits
can be generic over the part (`LdoStage<Part>`); switchers differ too
much for a part-generic block to keep datasheet fidelity.

## 3. Requirement → block → resolution (late binding)

From the designer's seat, nobody starts with "a TPS54331 here". They
start with "a buck here with these specifications". Vendor choice is
downstream — cost, availability, qualification, safety rating — for
exactly the reason passives carry no MPN in source: `Res(10kΩ, 1%)` is
the requirement, resolution binds the MPN later from data that lives
outside the repo. Switchers work the same way. Three roles:

- **Requirement** — what the designer says:
  `u1: Buck(vout=1.0V, i_max=4A, vin=9–14V, ripple=20mV, noise=500µV,
  qual=AEC-Q100)`. The vocabulary is the generalization ladder's —
  application facts. The `GenericBuck` placeholder the power tree
  emits is already this object; only its framing as "something you
  rename by hand" was wrong.
- **Block** — the `as design` subcircuit per part family (§1), with its
  `where` envelope and boundary promises. Switcher diversity lives
  entirely here; the requirement never knows a datasheet procedure.
- **Resolution** — a survey: every block whose envelope covers the
  requirement and whose promises meet it is a candidate; rank by
  cost, availability, qualification, safety rating; bind; **record in
  the lock**. `Library_Resolution.md`'s lock/freeze semantics and the
  `supply` statement's survey ("chosen from N candidates, M passed
  all gates") already do this — `supply` just binds at parse time and
  discards the requirement. The evolution: keep the requirement in the
  source; make binding a locked, reversible, designer-overridable step
  (`resolve u1 = Buck_TPS54331;` pins it by hand).

Two properties make this tighter than the passive case:

- **Acceptance and resolution are the same predicate.** ERC032 asks
  "does this committed part meet the stage's assumptions?";
  resolution asks "which blocks meet this requirement?". One
  definition, two uses.
- **The requirement stays live after binding.** The bound block
  flattens into real parts, so PDN, decap synthesis, safety and drift
  run on the real circuit while the requirement remains the contract.
  Change the requirement and the binding is re-checked.

The interface shape: a requirement instantiates an interface
(`BuckStage`: contract pins + requirement params), a block declares
`impl BuckStage`, resolution binds an impl. `trait`/`impl` already
exist in the grammar; the pin-contract test becomes the interface's
signature check.

**Where the passive analogy strains — stated.** Passives are a dense
continuum; switcher coverage is sparse and designer-built. A
requirement may resolve to nothing, and that is a first-class outcome:
"no block covers vout=0.85V @ 180A — nearest: Buck_TPS54331 fails
i_max (3A), VRM_XDPE fails vin range" (the survey's near-misses,
printed). Design proceeds unresolved — the requirement synthesizes as
a placeholder and ERC032 says so every run — and resolution is
required at commit, not at design. Library coverage becomes visible
and measurable: the correct incentive for a designer-owned library.

**Caution on requirement vocabulary.** It must stay at the
datasheet-procedure level: things every family block can be asked for.
"sync rectification" or "fsw = 2.2MHz" in a requirement is family
selection by another name — sometimes legitimate (EMI band
constraints are real), but a deliberate narrowing, never the default.

## 4. Requirements as explicit contracts — requirements-first

Question settled here: should BHDL separate requirements out as an
explicit, traceable contract — as safety does with goals → HSRs/HSIs —
even for non-safety designs?

**Yes, with one rule: a requirement that the machine cannot check is
documentation, not a requirement.** Traceability earns its cost only
when the trace ends in evidence. The existing FuSa machine is the
pattern: goal → effect (checkable predicate) → mechanism → measured DC
→ verdict; `assume pdn(...)` consumes a vendor-declared contract and
the PDN checks discharge or violate it. Every requirement kind below
already has, or can have, a machine verifier:

| Kind | Example | Stated by | Satisfied by | Verified by |
|---|---|---|---|---|
| Interface / structural | "a buck: 1.0V, 4A, ≤500µV" | designer (requirement instantiation) | a resolved block | resolution predicate; ERC032 after commit |
| Vendor contract | `domain VDD … zmask … droop_max` | the part (`as part`) | board network | PDN sweeps (Z(f), droop), decap verification |
| Behavioral / measurable | rail ±3%, ripple ≤ 20mV, FTTI ≤ 10ms | designer / safety goal | the circuit | DC/AC/transient sign-off, fault campaign |
| Safety (HSR) | detect overvoltage within 10ms, ASIL B | safety goal | mechanism (PSM/LSM) | measured DC, FMEDA metrics |
| Qualification / process | AEC-Q100, −40…125 °C grade | designer / program | part attributes | resolution filter; ERC at commit |

Principles that fall out:

1. **State a requirement ONCE, where it originates; everything else
   references it.** The vendor's `domain` contract is the requirement
   on that rail — the board never transcribes it. A designer-authored
   requirement ("≤ 20mV ripple on V1V0") is new information and is
   stated once with an id. Transcription is how spreadsheets rot; the
   drift check exists because of exactly this.
2. **IDs and `satisfies` links make the trace explicit.** Every
   contract construct (requirement instantiations, `domain`, safety
   goals, `assume`, power budgets) carries an id; implementing
   elements annotate `satisfies REQ-x`. The grammar already has
   `functional_requirement` / `technical_requirement` / `satisfies …
   via …` nodes and a safety-only V-model prototype
   (`requirement_hierarchy.rs`: goal → functional → technical →
   implementation, ASIL inheritance). The direction is to generalize
   that prototype beyond safety rather than invent a parallel system.
3. **The trace matrix is machine-derived, per build:** requirement →
   implementing elements → verification evidence → status
   (verified / unverified / violated / unresolved). The FuSa gap list,
   the PDN section, ERC032 and the drift report are already rows of
   this matrix; a requirement with no verifier is itself a finding.
4. **Requirements-first is already the flow we built.** The stub
   board of loads is requirement-level design; function-first
   instantiates parts that *carry* their vendor requirements; the
   power tree consumes requirements and emits requirement-level
   stages; resolution binds parts last. Making requirements explicit
   objects adds ids, links and the matrix — review and evidence — not
   new semantics.
5. **HSI.** The hardware–software interface is an interface contract
   (signals, levels, timing, ownership) — expressible through the
   interface/trait system with the same id/satisfies discipline; a
   natural extension once interface requirements exist.

## 5. First increments

1. DONE — TPS54331 split as the template: `TPS54331 as part` (pins,
   package, datasheet attrs, class `regulator_ic`) + `Buck_TPS54331(v_out,
   v_in, i_out_max, ripple, …) as design` (contract pins VIN/VOUT
   virtual/EN/GND, class `switching_regulator`, promises, sized
   internals, envelope `require i_out_max <= 2.4A` = 3A × 0.8 derating
   as a hard synthesis error). The SKU aliases name the block. The
   silicon's pins are reachable as `U1.u.SW`. Consumers: the chooser
   skips `as part` candidates and treats `as design` as self-expanding;
   the SPICE converter models a regulator-class design block as the
   regulator. Not yet: the `where` clause form (the envelope is a
   `require` in `design { }`), `wired(EN)` gating. Hardening that fell
   out: a failed expansion / design-recipe `require` is now a synthesis
   ERROR, not a log line — it exposed two stdlib recipes wiring
   `TVSDiode` pins `1/2` (declared `A/K`) that had been silently
   dropping the USB ESD diodes.
2. DONE — `trait BuckStage` / `trait LdoStage`
   (`bhdl-stdlib/power/stages.bhdl`: contract pins + requirement consts
   `vout, i_max, vin, vin_min?, vin_max?, noise?, efficiency_min?`). A
   block satisfies one with `impl BuckStage for Buck_TPS54331 { const
   vout = v_out; … }` — the impl body IS the requirement → constructor
   mapping. `bhdl_synthesizer::stage_resolution` runs as a text-level
   pre-pass before the main parse (the `supply` discipline): it surveys
   every impl in the library, TRIAL-INSTANTIATES each candidate
   (evaluates the block's `design { }` with the mapped params — a
   failed `require` is the envelope rejecting it) and checks the
   boundary promises (`output_current ≥ i_max/0.8`, `vin_min/max`,
   `output_noise`, `efficiency`; an undeclared promise the requirement
   needs = UNCHECKED = not a pass). Binding order: lock (kept while it
   still passes, re-resolved loudly when stale) → `resolve u1 = Block;`
   override (hard error if it fails its gates or names no impl) →
   survey (ranked by `cost_rel` only when every survivor declares one,
   else library order — the basis is stated). The bound block is
   spliced in with NAMED ctor args + import; `stage_*` scoped
   attributes keep the requirement live on the instance and ERC032
   re-checks the promises on the flattened circuit. Unresolved →
   `Generic*` placeholder with `powertree_rating_required_a` stamped
   (ERC032 every build) and the near-misses printed. Bindings live in
   `bhdl.lock` `[[stage]]` keyed by (board, instance); `--locked` fails
   on a changed binding. The power tree now EMITS `BuckStage(...)` /
   `LdoStage(...)` requirements (controller+external stages and the
   prereg have no interface yet and stay `Generic*`). Synthesis refuses
   an unresolved trait instantiation (it used to build an empty stub).
   Library coverage today: `BuckStage` ← `Buck_TPS54331`; `LdoStage` ←
   `Ldo_LP2985` (`bhdl-stdlib/power/lp2985.bhdl`, the LDO template:
   `LP2985(v_out) as part` carries the family MPN — the `-xx` SKU suffix
   is the BOM/MPN path's to derive from `output_voltage`; the block's
   envelope is SKU membership (summed equalities — the expression engine
   has no `||`), ≤120 mA, 2.5–16 V in, v_in ≥ v_out + dropout).
   Migrated since: `Ldo_XC6206`, `Ldo_AP2112K`, `Ldo_NCP1117` (input
   range NOT declared — UNCHECKED against a vin requirement, stated),
   `Ldo_LM7805`, `Ldo_LM317` (the OUT–ADJ divider sizing moved into the
   block's `design { }`), `Buck_TPS54302` (the part stays hand-wirable:
   boards that author their own application circuit use `TPS54302`
   directly), `Buck_AP63205` (fixed 5 V SKU — its envelope is
   `v_out == 5`). Aliases (`XC6206P332`, `LM317_5V`, …) name the blocks.
   The `supply` chooser now trial-evaluates an `as design` candidate's
   envelope through the SAME predicate as the resolver
   (`stage_resolution::trial_envelope`) — it had been selecting the fixed
   5 V AP63205 for 1.2 V / 3.3 V rails because the old entity silently
   took any `v_out`. With no cost data the resolver breaks ties by LEAST
   OVER-RATING (smallest `output_current` that covers the load), then
   library order — stated in the survey note.
   `BuckExtStage` (controller + external power stage; `phases` is part
   of the requirement) is the third interface; the power tree emits it
   for `BuckExternal` stages. Its only implementer is the generic
   `BuckController` TEMPLATE (`supply_choosable = false`, no orderable
   controller, placeholder FETs): the survey LISTS a template (`⧖`) but
   never auto-binds it; the designer commits it with an override that
   carries the power stage — `resolve u1 = BuckController(hs_fet=
   "BSC0902NS", fet_rds_on=2.3mΩ, fet_id_max=30A, …);` — whose args feed
   the candidate's evaluation (`output_current = fet_id_max`) and the
   splice. UNCHECKED promises (the template declares no input range —
   the real controller SKU's axis) are stated on an override, not
   blocking; declared-gate failures (i_max, phases) still refuse it.
   Migration-era entities that keep an `expansion { }` may `impl` a stage
   interface (stated as such in the survey). Library files now import
   the entities their application circuits instantiate — a requirement-
   first board imports nothing but the trait (the controller's FET
   children had been silently taking MOSFET defaults when the board did
   not import `MOSFET`). Still class templates, not parts:
   `regulator.bhdl` (`LinearRegulator<V_OUT, HAS_EN>` /
   `BuckRegulator<V_OUT>`).
   Ranking (landed): survivors' silicon is priced through the supplier
   provider (cheapest in-stock MPN with the part-number prefix — the
   `supply` chooser's path); when EVERY survivor priced, cheapest wins
   and the prices are printed; otherwise least over-rating, ties by
   library order — the basis is always in the survey note. Support
   parts are sized per instantiation and priced at BOM time, not here
   (stated). Qualification (landed): requirement consts `temp_min`,
   `temp_max`, `qual` gate against DECLARED part promises
   (`temp_min/temp_max` in degC, `qualification` string); undeclared =
   UNCHECKED = not a pass. Today only TPS54331 declares its ambient
   range (SLVSA86 −40…85 °C); no stdlib part declares a qualification —
   an `AEC-Q100` requirement resolves to nothing, honestly. Not yet:
   ERC032 and the resolver share the derate policy and the promise
   vocabulary but are still two code paths.
3. DONE — `bhdl <board> trace [--json]`
   (`bhdl_synthesizer::trace_matrix`). The matrix is DERIVED from this
   build's evidence, never transcribed: one row per contract construct
   that has a machine verifier — stage requirements (resolver + ERC032),
   rail budgets (ERC028), vendor `domain` contracts (the `decouple` PDN
   sweep report), part-carried `check { }` rules (ERC025), safety goals
   (campaign gaps; goal rows are UNVERIFIED unless `bhdl safety` ran).
   Every row carries id, kind, stated-by, the statement, implementing
   elements, verifier, status (VERIFIED / VIOLATED / UNVERIFIED /
   UNRESOLVED) and evidence. UNVERIFIED means the verifier could not run
   — e.g. a vendor domain with no `decouple` targeting it, or a promise
   the part does not declare — and is reported as a finding, never a
   pass. Ids are stable machine ids (`<board>.<inst>`, `…rail.<net>`,
   `<inst>.<domain>`, `<inst>.check[n]`, the safety goal's declared id);
   `attribute u1.requirement_id = "PWR_003";` names one explicitly.
   `satisfies { ID: via inst; }` (the safety prototype's grammar,
   generalized) appends declared implementers; a link to an unknown id
   or element is a finding. Exit 1 on VIOLATED rows or findings;
   UNVERIFIED/UNRESOLVED are stated (the commit gate is policy above
   this). Fell out: ERC025 now emits an UNCHECKED Info for an
   unresolvable predicate instead of a silent skip. Not yet: HSI
   interface requirements; designer-authored behavioral requirements
   ("ripple ≤ 20mV") as first-class rows need their own verifiers
   first — per the rule, they are documentation until then.
