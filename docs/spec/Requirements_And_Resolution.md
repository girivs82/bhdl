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
5. **HSI (landed).** The hardware–software interface is a board-level
   contract statement:
   ```
   hsi HSI_FAULT_A {
       signal: u3.PB0;           // the MCU pin the firmware reads/drives
       direction: input;         // from the software's seat
       level: 3.3V;              // logic level the hardware presents
       active: low;
       source: rail_a.nFAULT;    // the hardware driver
       latency_max: 10ms;        // declared, NOT verified in this build
       owner: "fw/safety_monitor";
   }
   ```
   It is a trace-matrix row (kind "hardware–software interface", id =
   the statement's name). Machine-verified on the netlist: `signal` and
   `source` share a net (wiring); the pin's declared direction agrees
   with the software view; the supply rail of the part that drives the
   source net matches `level` (±5 %; net classes, not pin types — a
   composite's virtual pin is followed to the real driver). `latency_max`
   (landed verifier): the HARDWARE share is derived — the driver's
   declared response latency (a safety mechanism's `latency=`, or the
   driving part's `latency` / `propagation_delay` attribute) plus the
   signal net's RC settling (pull-up R × node C, 2.2·τ for a 10–90 %
   edge) from the netlist; the FIRMWARE share cannot be measured here
   and is declared by the contract as `fw_latency` (a stated term the
   software side owns). `hw + fw ≤ latency_max` is the gate, itemized
   in the evidence; without `fw_latency` only the hardware share is
   checked and the row says so; a driver with no declared latency makes
   the hardware share UNCHECKED. `owner` and `source` are the
   implementers (fw / hw). A wrong net, level or latency is VIOLATED.
6. **HSR evidence (landed).** The safety goal *is* the HSR here (goal →
   effect → FTTI → mechanism → measured DC), and it already carries an
   id. `bhdl trace --safety m.json` consumes the campaign model that
   `bhdl safety --json m.json` wrote: goal rows become VIOLATED on an
   undetected effect / PSM-without-LSM / unsourced DC / AoU violation /
   metric miss — and on a fault that RAN with a failed expectation or
   FTTI (the campaign keeps the `FaultUnrun` class; the fault record has
   the truth); UNVERIFIED naming the exact fault not yet run; VERIFIED
   with each mechanism's measured DC and the in-scope universe counts.
   Gaps attributed to no goal (FIT uncomputed, parts without safety
   data) are findings. A model for another board is refused. Without
   `--safety`, goal rows stay UNVERIFIED and say how to get the evidence.
   The trace command now resolves the same transitive imports the safety
   command does, so library goals/assumptions are visible.

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
   regulator. (The `where` clause form and `wired(EN)` gating landed
   later — see the end of increment 2.) Hardening that fell
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
   on a changed binding. The pre-passes are UNIVERSAL: they run on the
   CLI input before command dispatch, AND on board text a command reads
   from DISK because its board is a different file than the input — the
   safety sidecar and powertree's regenerate-strip both route through
   `preprocess_board_text` (supply desugar + requirement resolution +
   parse), so a board builds identically no matter which command
   reaches it (they used to parse raw disk text and die with
   "Undefined component type: LdoStage" on a requirements-style board). The power tree now EMITS `BuckStage(...)` /
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
   `PreregStage` (the protected front end; landed last) completes the
   set: vocabulary `vout, i_max, vin, vin_min/max?` plus the protection
   FUNCTIONS as distinct words — `ov_clamp` (passive clamp ≤), `ov_trip`
   (active cutoff ≤), `uv_trip` (lockout ≥), `reverse_polarity` — each
   gated only against a block that declares it. The tree emits
   `PreregStage(...)` for its front end; `GenericPrereg` is now just the
   unresolved placeholder. Implementer: `PassiveFrontEnd`
   (`bhdl-stdlib/protection/front_end.bhdl`, fuse + unidirectional TVS
   from real library parts — a genuine block, not a template) promising
   the fuse rating, `ov_clamp = tvs_v`, and an input range BY
   CONSTRUCTION (0 V … the clamp point); it declares no cutoff, lockout
   or reverse-polarity protection, so a requirement stating one is
   UNCHECKED against it and stays unresolved. Its envelope is the fuse
   derating (`where i_load <= 0.8 * i_rating, v_in < tvs_v`).
   `Efuse_TPS2660` (`bhdl-stdlib/protection/tps2660.bhdl`, TI 60 V / 2 A
   eFuse with reverse-input protection) promises `ov_trip`, `uv_trip`,
   `reverse_polarity`, 4.2–55 V and 2 A, and sizes the OVP / UVLO
   dividers from the requirement's trip points against the 1.2 V pin
   threshold (provenance stated in the file; re-verify at sign-off).
   The ILIM resistor law is NOT in this library's data, so the block
   does not compute it: `r_ilim` is a REQUIRED argument — which makes
   the block a TEMPLATE (listed, committed by `resolve fe =
   Efuse_TPS2660(r_ilim=…)`); an instantiation without it is refused by
   the constructor-arg validator (E0404). That is the Real-Data rule
   applied to a block: a missing datasheet axis becomes a designer
   argument, never a default. `IdealDiode_LM74700`
   (`bhdl-stdlib/protection/lm74700.bhdl`, TI ideal-diode controller,
   3.2–65 V, reverse input to −65 V, AEC-Q100) drives an external
   N-FET in the positive rail: it promises `reverse_polarity`, the
   input range and the qualification, no cutoff/lockout; the stage
   carries what its FET carries, so — the `BuckController` idiom — the
   FET's axes travel with the override and the block is a TEMPLATE
   (`resolve fe = IdealDiode_LM74700(fet="…", fet_id_max=…, …)`), with
   `where … i_out_max <= 0.8 * fet_id_max, fet_vds_max >= v_in`.
   `BoostStage` (step-up: the rail sits ABOVE its feed) uses the same
   vocabulary; the power tree now plans a Boost stage for such a rail
   (it used to fall silently into the buck path with duty > 1) and
   states the boost physics: the switch carries the INPUT current
   I_out·V_out/V_in, the rating/derating are against that, and the
   estimator's loss form is conduction at D = 1 − V_in/V_out on the
   input current with transitions swinging V_out. `BuckBoostStage` is
   for a feed RANGE that straddles the output (battery discharge across
   V_out): the straddle IS the requirement, so vin_min/vin_max must be
   stated — the tree carries one nominal feed voltage and never emits
   it. `BoostStage` has its first implementer, `Boost_TPS61022`
   (`bhdl-stdlib/power/tps61022.bhdl`, SLVSDX7D): its `where` envelope
   carries the vendor's own ratio arithmetic — the switch VALLEY
   current I_L(DC) − ΔI/2 = V_out·I_out/(V_in·η) − V_in·D/(2·L·f) must
   stay under the 6.5 A minimum valley limit × 0.8 derating — so the
   same 5 V / 2 A requirement resolves from a 3.6 V feed and is refused
   from 1.9 V with that arithmetic named. `BuckBoostStage` has
   `BuckBoost_TPS63020` (`bhdl-stdlib/power/tps63020.bhdl`, SLVS916I):
   the impl BINDS `vin_min`/`vin_max`, so the requirement's straddle is
   conveyed into the ctor and the envelope — the datasheet's Eq. 1/2
   average-switch-current arithmetic, evaluated at v_in_min where the
   boost ratio is worst — runs at the requirement's own operating
   point: IOUT/(η·(1−D)) ≤ 2.8 A (3.5 A minimum average limit × 0.8),
   with the datasheet's own η = 0.9 assumption. A Li-ion straddle
   (2.5–4.2 V) across 3.3 V resolves; a 1.8 V floor boosting to 5 V at
   2 A is refused with the arithmetic named.
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
   not import `MOSFET`).
   `regulator.bhdl` (landed): the class templates took the same shape
   as TEMPLATES — `LinearRegulatorIc` / `BuckRegulatorIc as part`
   (placeholder silicon carrying the class axes, NO part_number) +
   `LinearRegulator` / `BuckRegulator as design` with
   `supply_choosable = false`, `impl LdoStage` / `impl BuckStage`: the
   survey lists them (⧖) and only an override with the designer's
   numbers commits one (`resolve u = LinearRegulator(dropout=1.2V, …);`).
   No current rating is promised (a class cannot) — `i_max` is UNCHECKED
   against them. The `HAS_EN` generic became `wired(EN)` gating; the
   `V_OUT` generic became the `v_out` parameter. `LM7805` / `LM1117_*` /
   `LM2596_*` stay as class stand-in aliases with that status stated
   (the TI µA7805 itself is `Ldo_LM7805`). Nothing in the regulator
   library is a migration-era conflated entity any more except
   `BuckController`'s yieldable takeover recipe, kept by design.
   `BoostRegulator` / `BuckBoostRegulator` (same file) extend the class-
   template set to the step-up topologies — with one doctrine shift: no
   canonical part lends these classes their figures, so the CAPABILITY
   axes (`i_sw_limit`, `f_sw`, `rds_on`[+`_ls`], `i_quiescent`, `vref`;
   the buck-boost also `v_in_min`/`v_in_max`) are REQUIRED designer
   arguments — the `r_ilim` doctrine, never an invented class default.
   Their envelope checks the switch PEAK current (I_L(DC) + ΔI/2 ≤
   i_sw_limit × 0.8): peak ≥ average ≥ valley, so the check is
   conservative whichever convention the designer's part uses — a
   vendor block encodes its part's own convention instead
   (`Boost_TPS61022` valley, `BuckBoost_TPS63020` average). Landing
   them flushed a resolver silent-drop: `self_namespace` only overlaid
   conveyed/override values onto params that HAD defaults, so a
   required (default-less) param never reached the envelope — the
   template evaluated against nothing (`Efuse_TPS2660`'s required
   `r_ilim` never tripped it because no envelope reads it). Fixed:
   overlay onto every DECLARED param.
   Envelope spelling (landed): `entity X(…) as design where i_out_max <=
   2.4A, v_in >= 3.5V, v_in <= 28V { … }` — each comparison is lowered
   to a `require` at the front of the block's plain `design { }` (created
   if absent; bare parameter names become `self.<param>`, the message
   quotes the clause as written), so the resolver's trial-instantiation
   and synthesis evaluate ONE predicate. Clauses needing arithmetic (SKU
   membership, `v_in >= v_out + dropout`) stay as explicit `require`s;
   both spellings are valid. The `where` may precede or follow the `as`
   tag. Wiring-gated optional features (landed): in a design block's
   plain body, `generate if (wired(EN)) { EN -> u.EN; } else { VIN ->
   u.EN; }` — the board's wiring is the configuration. Lowered to a
   per-statement gate on the firm recipe, evaluated at expansion against
   the board's nets (the same "has a net" criterion the heuristic
   live-pin rule uses, but declared). Gated pins are stamped on the
   instance (`gated_pins`) so ERC006 does not report an intentionally
   unwired optional input. Every EN-carrying block (TPS54331, TPS54302,
   AP63205, LP2985, AP2112K) now defaults to enabled when EN is left
   unwired. Only `wired(PIN)` / `!wired(PIN)` are supported as the
   condition; nesting is not.
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
   range (not in SLVS839H — retracted; see §6); no stdlib part declares a qualification —
   an `AEC-Q100` requirement resolves to nothing, honestly.
   PROJECT-WIDE FILTERS (landed): a board states once, where they
   originate, the requirements that apply to every stage on it —
   `requirements { qual: "AEC-Q100"; asil: B; temp_min: -40degC;
   temp_max: 125degC; }`. The resolver merges them into each stage
   requirement before evaluation (a key the stage states itself wins)
   and stamps the MERGED text, so the lock, ERC032 and the trace matrix
   see one requirement. `asil` (QM < A < B < C < D) gates against a
   part's `asil_capable` — the vendor's SEooC / functional-safety-
   compliant claim, never inferred. The trace matrix also DERIVES the
   ASIL a stage must meet from the safety goals whose effects reference
   the rail it drives, and flags a stage that serves an ASIL goal
   without stating `asil=` (the resolver never saw the filter) or whose
   block declares no capability.
   THERMAL PATH (landed): the interface's `temp_min/max` is the
   operating AMBIENT range. A junction-rated part meets an ambient
   requirement THERMALLY when it declares `theta_ja` and `tj_max`:
   T_J = T_A,max + P·θ_JA ≤ T_J,max, with P the stage's dissipation at
   the requirement's operating point — computed by one estimator from
   the block's class and promises (linear: (Vin−Vout)·I + Vin·Iq;
   switcher: (1−η)/η·Vout·I; pass-through: I²·R_on), used by both the
   resolver and ERC032. What is missing for the derivation is named in
   the UNCHECKED text. The same θ_JA feeds the handbook FIT model (T_J).
   The SIGN-OFF closes the loop with measured numbers: a part whose
   stress model yields `self.p_diss` and that declares `theta_ja` +
   `tj_max` gets a JUNCTION-TEMPERATURE row — thermal rise P·θ_JA
   against the rise budget T_J,max − T_A, with T_A from the stage
   requirement's `temp_max` (via the block) or 25 °C ASSUMED and said
   so; the 1.2× sign-off margin applies to the rise. And ERC032 checks
   the requirement against the AS-BUILT board: an `i_max` that
   understates the rail budget the board declares (`@ I` on the driven
   rail) is an Error naming both numbers — the block was resolved
   (envelope, derating, thermal) for a load the board does not have.
   A local block defined in the board's own file is a candidate and an
   override target like a library block.
   ONE PREDICATE (landed): `bhdl_synthesizer::stage_acceptance::check`
   is the acceptance function. The resolver calls it with promises read
   from the block's entity text (attributes resolved through params);
   ERC032 calls it with the committed instance's resolved attributes on
   the flattened circuit, its requirement taken from the stamped
   `stage_requirement` text (a tree-emitted stage's `powertree_*`
   assumptions fill in i_max / noise / efficiency). Same gates, same
   derating constant, same UNCHECKED semantics: a gate the resolver
   reported UNCHECKED is exactly the gate ERC032 reports as an UNCHECKED
   Info — never an Error, never silence. The resolver keeps only its own
   concerns on top: the `as design` / migration-era block gate, the
   envelope trial, the template rule, and ranking.
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

## 6. Library coverage (per interface)

The survey prints this every build for the requirement at hand; this is
the standing picture. *auto* = a genuine block the resolver may bind on
its own; *template* = listed (⧖), committed only by a `resolve … = Block(
<the designer's args>)` override. A function a block does not declare is
UNCHECKED against a requirement that states it — never a pass.

| Interface | Block | Binds | Promises | UNCHECKED (not provided) | The designer supplies |
|---|---|---|---|---|---|
| `BuckStage` | `Buck_TPS54331` | auto | 3 A, 3.5–28 V, 89 %, T_J ≤ 150 °C via θ_JA 116.3 | noise, qual | — |
| `BuckStage` | `Buck_TPS54302` | auto | 3 A, 4.5–28 V, 92 %, T_J ≤ 150 °C via θ_JA 118.9 | noise, qual | — |
| `BuckStage` | `Buck_AP63205` | auto (5 V SKU only) | 2 A, 3.8–32 V, −40…85 °C ambient, θ_JA 89 | noise, qual, efficiency | — |
| `BuckStage` | `BuckRegulator` (LM2596 class) | template | headroom envelope only | i_max, vin range, noise, temp, qual | class numbers |
| `LdoStage` | `Ldo_LP2985` | auto | 150 mA, 2.5–16 V, 30 µV, T_J ≤ 125 °C via θ_JA 205.4 | qual | — |
| `LdoStage` | `Ldo_XC6206` | auto | 200 mA, 1.8–6 V, −40…85 °C | noise, qual | — |
| `LdoStage` | `Ldo_AP2112K` | auto (1.8/2.5/3.3 V SKUs) | 600 mA, 2.5–6 V, −40…85 °C ambient, θ_JA 184 | noise, qual | — |
| `LdoStage` | `Ldo_LM7805` | auto (5 V) | 1.5 A, 7–35 V, 40 µV, T_J ≤ 125 °C via θ_JA 19 | qual | — |
| `LdoStage` | `Ldo_LM317` | auto (1.35–37 V) | 1.5 A, ≤ 40 V in, T_J ≤ 125 °C via θ_JA 23.5 | vin_min, noise, qual | — |
| `LdoStage` | `Ldo_NCP1117` | auto (5 V) | 1 A, 0…125 °C ambient (NCP grade), θ_JA 160 min-pad | vin range, noise, qual | — |
| `LdoStage` | `LinearRegulator` (78xx class) | template | 40 µV class, headroom | i_max, vin range, temp, qual | class numbers |
| `BuckExtStage` | `BuckController` | template | 1 phase; rating = the FETs' | vin range, noise, temp, qual | FETs + axes |
| `BoostStage` | `Boost_TPS61022` | auto | 3 A @ 3.6→5 V (94.7 %), 2.2–5.5 V out, 1.8 V startup floor, T_J ≤ 125 °C via θ_JA 108.2 | noise, qual | — |
| `BoostStage` | `BoostRegulator` (class) | template | PEAK-current envelope only | i_max, vin range, noise, temp, qual | class numbers (required args) |
| `BuckBoostStage` | `BuckBoost_TPS63020` | auto | 2 A boost mode (4 A buck), 1.8–5.5 V in, 1.2–5.5 V out, −40…85 °C ambient + T_J ≤ 125 °C via θ_JA 41.8 | noise, qual | — |
| `BuckBoostStage` | `BuckBoostRegulator` (class) | template | PEAK-current envelope at v_in_min only | i_max, vin range, noise, temp, qual | class numbers (required args) |
| `PreregStage` | `PassiveFrontEnd` (fuse + TVS) | auto | fuse rating, `ov_clamp`, 0 V … clamp point | ov_trip, uv_trip, reverse_polarity, temp, qual | — |
| `PreregStage` | `Efuse_TPS2660` | template | 2 A, 4.2–55 V, `ov_trip`, `uv_trip`, `reverse_polarity`, T_J ≤ 150 °C via θ_JA 38.6 | ov_clamp, qual | `r_ilim` (ILIM law not in the library) |
| `PreregStage` | `IdealDiode_LM74700` | template | rating = the FET's, 3.2–65 V, `reverse_polarity`, AEC-Q100 grade 1 (−40…125 °C) | ov_clamp, ov_trip, uv_trip | pass FET + axes |

Reading the table is the point: a `PreregStage` requirement that states
`reverse_polarity` has no auto-bindable block — the designer must choose
the eFuse (and supply its current-limit resistor) or the ideal diode
(and supply its FET); one that states only `ov_clamp` binds the passive
front end on its own. No block in the library declares output noise for
a switcher (ripple is not noise) or a qualification other than the
LM74700-Q1's — an `AEC-Q100` buck or LDO requirement resolves to
nothing, honestly. Temperature: the interface's `temp_min/max` is the
operating AMBIENT range; only datasheets that rate ambient feed it
directly (XC6206, AP2112K, LM74700-Q1 grade 1). Parts whose datasheets
rate the JUNCTION carry `tj_min/tj_max` plus `theta_ja` / `theta_jc`
read from the datasheet thermal tables (TPS54331 SLVS839H 116.3,
TPS54302 SLVSDG6C 118.9, LP2985 SLVS522S 205.4, LM317 SLVS044Z 23.5,
µA7805 SLVS056P 19, TPS2660 SLVSDG2G 38.6, LM74700-Q1 SNOSD17G 189.8
°C/W — all JEDEC-standard board metrics, SPRA953, board-dependent) and
meet an ambient requirement THERMALLY at the stage's dissipation. A
junction figure is never declared as an ambient one: TPS54331's earlier
"−40…85 °C ambient" was not in its datasheet and was retracted. No TI
datasheet here claims an ASIL: "Functional Safety-Capable" (TPS2660,
LM74700-Q1) means documentation is available and is recorded as
`functional_safety`, not as `asil_capable`. Non-TI datasheets (fetched
via distributor mirrors where the vendor site blocks automation):
AP2112 DS39724 Rev. 2 (SOT25 θ_JA 184, TA −40…85), AP63205 DS41326
Rev. 3 (TSOT26 θ_JA 89, TA −40…85 — and fSW 1100 kHz / RDS(on) 125 mΩ,
correcting the carried-over 500 kHz), NCP1117/D Rev. 17 (SOT-223 θ_JA
160 minimum pad, ambient 0…125 °C for the NCP grade), XC6206 ETR0305
(Topr −40…85, Pd 250 mW — no θ_JA or T_J in the datasheet, stated).
The "Promises" column is what ERC032 re-checks on the flattened circuit
every build.

## 7. Power-up sequencing — requirement / promise / verify

Sequencing is a DECLARED contract, never a generated heuristic (the old
analyzer Pass 7 — name-substring criticality, invented default delays,
output nobody consumed — is retired). The requirement lives on the
load's `domain` contract, source-cited like every other axis, and any
combination of the three spellings is valid:

```bhdl
domain VDD_CORE pins="1" v=0.8V i_max=2A slot=1              source="…";
domain VDD_IO   pins="2" v=1.8V slot=2 slot_t_min=1ms        source="…";
domain VDD_PHY  pins="3" v=1.2V after="VDD_CORE" t_min=500us source="…";
domain VDD_AUX  pins="4" v=3.3V sw_enabled=true after="VDD_IO" source="…";
```

- `slot=N` — slot-N rails come up after ALL slot-(N−1) rails (the shape
  SoC datasheets state their tables in); `slot_t_min` is the minimum
  inter-slot delay before this rail's slot.
- `after="A,B"` — explicit ordering edges; `t_min` is a hard minimum
  delay on them.
- `sw_enabled=true` — firmware raises the rail after boot. The hardware
  obligation shrinks to "the enable IS software-reachable"; the
  ordering itself is discharged to a STATED software assumption (Info,
  including any declared order, for the bring-up code to honor).

VERIFICATION is ERC033, on the flattened netlist (the contracts are
stamped onto their instances as `seqdom_*` attributes at synthesis so
the DRC-signature check can see them). Every edge `B after A` must be
IMPLEMENTED by one of:

- **PG chain** — B's supply-stage `EN` on the same net as A's stage
  `PG` (the `BuckBoost_TPS63020` block exposes its part's PG, with the
  Figure-7 pull-up, exactly for this);
- **rail chain** — B's `EN` driven from rail A directly or through a
  series R; with a C to ground the RC's enable-threshold crossing time
  `t = R·C·ln(Vs/(Vs−V_IH))` is COMPUTED (V_IH from the stage's
  `en_vih` datasheet attribute; Vs = the prerequisite domain's v_nom)
  and checked against a declared `t_min`;
- **sw_enabled** — B's `EN` on a Signal-class net (see above).

A declared ordering with no mechanism — enable unwired (the stage
auto-enables at power-in), or wired to something that is neither the
PG nor the rail — is an Error naming the missing edge and the fix. A
declared `t_min` with no timing element, or an RC that crosses too
early, is an Error with the arithmetic. Missing `en_vih` is a stated
UNCHECKED. A rail with no on-board supply stage cannot be sequenced by
this board — stated Warning. One block driving BOTH rails of an edge is
the multi-output-supply (PMIC) hook: it must PROMISE its built-in
power-up order for the edge to pass; no block declares that promise
yet, so today it is a stated UNCHECKED — this is the sequencing gate
the aggregation post-step will use when merging per-rail stages into a
PMIC.

### 7.1 The power-up timeline — `bhdl powerup`

The pairwise ERC033 check verifies each edge's MECHANISM; it cannot
verify the TIMELINE, because delays compose and because sources have
finite CAPACITY. The canonical miss: a downstream stage enables, its
inrush (charging its output bank at the soft-start/current limit,
reflected through the topology into input current) exceeds the
upstream stage's capability, the upstream stage goes constant-current
and the deficit drains the upstream bulk — the rail SAGS below good
(the knee), thresholds stretch or un-cross, and the accumulated delay
walks a rail into the next slot's window.

`bhdl powerup` simulates exactly this as a piecewise-linear event
timeline — deliberately NOT SPICE: every source is a current-limited
PWL source (soft-start behavior from datasheet-cited attributes:
`ss_i_initial`/`ss_v_full`, e.g. TPS63020's 400 mA-until-1.2 V ramp,
TPS61022's 700 mA-below-0.4 V), every load a constant domain current,
every net its summed real capacitance — so per interval every rail's
dV/dt is constant and event times are exact (EN-RC nodes use the
exponential crossing formula against the interval-held source;
intervals additionally capped at 2 % rail movement). PG semantics are
modeled from the datasheet: TPS63020's PG monitors the CURRENT loop
(`pg_on_regulation`), so a PG-chained stage automatically waits out an
upstream knee. Modeling choices that are approximations are printed in
the report header, stated.

The declared windows (order, `t_min`, the new `t_max` — SoC latch-up
windows only a timeline can check — and slots incl. `slot_t_min`) are
verified against the simulated good-times; a slot-N rail going good
WHILE a slot-(N−1) rail is sagged below good is the knee finding, with
the arithmetic (demand vs capability, the reflected inrush culprit,
the bank size) and the fix (more bulk, or a PG-chained enable) named.
The three-arm regression (`powerup_timeline_catches_the_knee`):
undersized bulk + RC enable → knee → slot re-opened; large bulk + RC
enable → the rail rises SLOWLY and the RC threshold fires long before
the rail is good (a REAL composition flaw the pairwise check blesses);
large bulk + PG chain → clean. `sw_enabled` rails are excluded from
the hardware timeline, stated. Exit is non-zero when any declared
window fails.

### 7.2 Toward the automatic PDN — EN/PG on the interfaces, auto-decouple

The pieces of §1–§7.1 compose into a closed loop: plan the tree,
resolve the regulators, synthesize the decap networks, size the bulk
from the power-up timeline, synthesize the sequencing chains, verify —
one emitted region. Two enablers land first:

- **EN/PG contract pins on every stage interface** (and the Generic*
  placeholders, `gated_pins`-exempt): a board — or the tree's chain
  synthesizer — can wire an enable chain against the INTERFACE before
  knowing the block. A block whose part has no PG simply cannot anchor
  a PG chain (only `BuckBoost_TPS63020` exposes one today); the chain
  synthesizer must fall back to rail-RC there.
- **Auto-`decouple`** (`powertree::decouple_worklist`): every
  instantiated domain declaring a Z(f) mask gets its `decouple`
  statement emitted into the generated region — when the project names
  its capacitor library, `requirements { decap_lib: "<path>"; }`,
  because C/ESR/ESL are library DATA this repo never invents (the
  `r_ilim` doctrine). Without `decap_lib` the worklist is a stated ⚠
  gap; a hand-written `decouple` always wins. The emitted statement
  runs through the normal decap synthesis inside `--emit`'s re-verify,
  so an infeasible mask FAILS the emission loudly and restores the
  board — the emission gate and the decap physics are one judgement.
  Project keys are now WHITELISTED into stage requirements (`qual`,
  `asil`, `temp_*`, `noise`, `efficiency_min`, `vin_*`) — other keys
  (`decap_lib`, …) have their own consumers and no longer leak into
  every requirement.

Still ahead (the agreed plan): the chain synthesizer + bulk-sizing
fixpoint in `--emit` (the powerup knee finding run backwards:
C ≥ deficit·t_inrush/ΔV_allowed), load-step (EDP) axes feeding bulk
sizing alongside the knee, and the full-system interaction simulation —
the PWL engine's post-settle phase firing each domain's declared
`step_a`/`step_rise` (and coincident sibling steps) to catch
cross-regulation through shared feeds.

### 7.3 Load-step interactions — superposition with a self-consistency gate

Once the tree is built, the domains pull power in their own patterns —
steady draws (I/O), load steps, bursts. Fire them one at a time and
superimpose, or simulate simultaneously? BOTH, in an order that makes
superposition's validity checkable from its own result:

1. **Per-domain runs** (N cheap sims): each domain's declared
   trapezoid (`step`/`rise`/`dur`) fired ALONE from the settled
   operating point; recorded per run: self-droop, coupling onto every
   sibling rail, and the extra peak demand imposed on every stage.
   In-regulation (small-signal) droop is the Z(f)/decap domain's
   business — mask-verified in §arc-(c); this engine models droop
   where LIMITS engage.
2. **Peak-aligned superposition screen** (conservative, like
   worst-case timing): sum the contributions; check every stage's
   summed demand against its current limit and every rail's summed
   droop against the tightest declared `droop_max` (good-threshold
   fallback, stated). **The self-consistency gate**: if no summed
   demand crosses a limit, no clamp ever engaged, the system provably
   stayed linear, and the superposition IS the worst case — proof,
   not approximation. The report says so in those words.
3. **Escalation**: a crossed limit means superposition is invalid AT
   THAT POINT BY ITS OWN ARITHMETIC — the screen is the pruning
   oracle. The implicated domains (only those) are fired
   SIMULTANEOUSLY through the same nonlinear engine, which handles
   the limit clamps and the constant-power input reflection
   (I_in = P/V_in, negative incremental resistance) natively; the
   joint run's actual droop and current-limit entries become the
   findings, fully attributed ("coincident steps A + B droop rail X
   by 501 mV over its declared 150 mV; u1 entered current limit").

The regression (`load_step_superposition_screen_and_escalation`): two
bursts that individually stay under the shared stage's limit and
jointly cross it — flagged, escalated, confirmed; the same board with
smaller bursts earns the linear-region proof with zero joint runs.
Genuinely periodic burst patterns (frame cadences) would want a
repetition axis on the contract; peak alignment is conservative for
them meanwhile (stated).

### 7.4 The emission convergence loop — chains synthesized, bulk sized

`powertree --emit` is now a closed loop, built IN MEMORY and written
to disk once, converged: stages + auto-`decouple` (§7.2), then the
SEQUENCING CHAINS discovered from the first resolved build (PG
exposure and `en_vih` come from the BOUND blocks, not the
requirements), then BULK sized by the fixpoint whose oracle is the
`powerup` engine itself (§7.1/§7.3).

- **Chain synthesis** (`powertree::synthesize_seq_chains`): for every
  declared ordering edge whose target stage has an unwired enable —
  a hand-wired enable always wins — emit the mechanism: PG chain when
  every prerequisite's bound block exposes a PG contract pin
  (open-drain wired-AND; the pull-up lives inside the PG block's own
  application circuit, detected by its `_R_pg` child, 1 MΩ assumed
  with a note otherwise), with a declared t_min adding
  C = t_min / (R·ln(Vs/(Vs−V_IH))); rail-RC fallback otherwise
  (100 kΩ series — a stated sizing choice that also limits current
  into the EN clamp — 10 nF benign default without a t_min). Multiple
  prerequisites without full PG coverage fall back to the first + a
  note; everything emitted is verified by ERC033 and the timeline —
  generator and checkers share one arithmetic.
- **Bulk fixpoint**: run `powerup` (timeline + interaction screen);
  every Error finding carrying a structured rail attribution bumps
  that rail's bulk (22 µF seed, doubling); iterate to clean, max 10.
  Findings with NO rail attribution are NOT closable by capacitance
  (an ordering flaw, a Generic placeholder whose `en_vih` cannot be
  known): the emission is kept — it builds — and the findings stay
  OPEN and printed as designer action, never silently absorbed. A
  non-converging fixpoint refuses the emission entirely.

Verified live: a slot-ordered SoC with a 4.5 A burst over the boost
stage's capability converges at 2 chain wires + 704 µF on the burst
rail in 3 iterations, with the Generic-placeholder ordering finding
correctly left open (its resolver survey names exactly why nothing
bound: the 30 µV noise requirement rejects every non-promising LDO).

### 7.5 Load-derived bulk (auto-mask) + the final PDN sanity check

Startup is only one master of bulk sizing — the LOAD is the other.
A domain that declares `step` and `droop_max` has already stated its
low-frequency target impedance: Z = droop_max/step, flat across the
step's spectral band [1/(2π·(dur+2·rise)) … 1/(π·rise)]. The decap
synthesizer now derives that AUTO-MASK (tighten-only merged with any
declared zmask; a step/droop domain with NO zmask still gets a
`decouple` in the auto worklist) and sizes the network from the REAL
capacitor library — so linear-regime bulk becomes orderable parts
with characterized ESR/ESL, not an abstract farad count. Below the
supplying stage's control crossover the REGULATOR carries the step;
no block declares `f_c` yet, so the auto-mask applies from the
100 kHz sweep floor up and the sub-crossover region is a NAMED
UNCHECKED gap (`attribute f_c = <datasheet>` closes it — the sweep
floor follows the mask down when it is declared). Never a silent
pass, never an absurd caps-only demand at hundreds of Hz. The two
sizers compose: auto-mask/decap handles the linear regime with real
parts, the powerup fixpoint handles the clamp regime, and the loop
re-runs powerup after decap so more bulk cannot silently worsen the
upstream inrush.

With bulk and decap settled, `--emit` runs the FINAL PDN SANITY
(`powertree::final_pdn_sanity`), on either exit path:

- **Loop stability**: each stage's total output capacitance against
  its DATASHEET envelope — `c_out_eff_min`/`c_out_eff_max`
  (TPS61022: 20–1000 µF effective, SLVSDX7D EC) or `c_out_min`
  (TPS63020: 44 µF per Table 1; §8.2.2.3 states no upper limit) —
  honoring the effective-vs-nominal gap the datasheets themselves
  state: nominal×0.5 must clear the floor, nominal×1.2 the ceiling.
  A runaway fixpoint IS catchable: the 900 µs-burst probe drove bulk
  to 2816 µF and was flagged at 3386 µF effective > 1000 µF. No
  declared envelope = stated UNCHECKED.
- **Resonance**: capacitors with no declared ESR/ESL (fixpoint bulk,
  block application caps) are swept as IDEAL by the decap
  verification — their anti-resonances are UNPLACED, and the check
  says so by name; the close is selecting bulk from a characterized
  library.

**Envelope-aware search (§7.5 addendum).** Instability appearing does
not mean the designer must act — the fixpoint searches INSIDE the
feasible interval itself. Bulk has a lower bound (the droop/knee
physics, from the sim) and an upper bound (the stage's datasheet
stability envelope, ÷1.2 effective, minus the fixed caps already on
the rail): the doubling search is CLAMPED at that ceiling, and after
the first pass it BISECTS down to the smallest sufficient bulk (the
doubling overshoots by up to 2× — the 200 µs-burst probe now lands at
660 µF instead of 704). Designer action is reserved for the one case
the tool can prove: droop still failing AT the clamped ceiling means
the feasible interval is EMPTY — capacitance cannot fix it — and the
finding says so with both numbers and the remedies (split the rail,
chain the load's enable, reduce the step, pick a stage with a wider
envelope). The ×1.2 sanity ceiling check remains as the backstop for
hand-authored bulk.

**Ceramic derating on the SIZING side (§7.5 addendum 2).** A
ceramics-only reliability policy means the bank's effective
capacitance lives in the vendors' stated tolerance band (−50 %/+20 %,
SLVS916I Table-1 footnote). The design must hold at EVERY point of
that band, so the two bounds compose as a robust criterion on the
NOMINAL value: nominal ∈ [2 × physics-need, envelope-max ÷ 1.2].
Fixpoint bulk (`seqbulk_*`) therefore enters the simulation at ×0.5
nominal — the sized bank meets the droop with worst-case derated
ceramics by construction — while the ceiling check keeps ×1.2. The
operating point the search lands on is exactly the requested one:
the LOWER bound plus the derating margin, moved up only as far as
the droop physics demands and never past the stability envelope. A
spec that fits at nominal but not across the band now honestly
reports an EMPTY interval (the 200 µs/4.5 A/2 % probe does; at 3 %
it converges to 660 µF nominal = 330 µF guaranteed effective).
Refinement path: a characterized MLCC library with per-part DC-bias
data replaces the ×0.5 class factor with each part's own curve.

### 7.6 Power-down and sleep — the timeline run backwards

The same PWL engine, two scenarios (`bhdl powerdown`):

- **Input loss**: the ideal input rails drop to 0; stages lose VIN and
  turn off (load-disconnect per their datasheets), so each output bank
  discharges through ITS OWN loads — C·V/I physics, where a
  lightly-loaded rail OUTLIVES a heavy one (the classic reason
  discharge paths exist). Declared `down_before` orderings and
  `down_t_max` windows (new domain axes, with `i_sleep` and
  `sleep_off`) are verified on the simulated down-times (10 % of
  nominal, stated). A bank that cannot bleed within the horizon under
  a declared ordering/window is an Error naming the fix (bleed R /
  discharge FET); the engine now models rail→GND resistors as real
  conductances, so the fix is SIMULATABLE — the probe's 536 µF bank
  needs ~40 Ω to meet a 100 ms window (τ·ln10), which is exactly why
  real designs switch the discharge with a FET instead of burning a
  passive bleed.
- **Sleep entry**: firmware drops the `sleep_off` rails (their stages
  forced off — requires a signal-driven enable, checked; an EN net
  with no discoverable pull source is treated as a FIRMWARE signal,
  raised by default, stated); every domain draws its `i_sleep`
  (others keep i_nom, stated). Dropped-rail discharge times are
  reported — a 2 µA sleep load bleeding a 536 µF bank for 48 ms is
  the re-entry latency to see — and rails that STAY must hold good
  through the transition (a disturbed survivor is an Error). A rail
  carrying both a `sleep_off` domain and a staying one is an Error
  (split the rails).

The probe chain is honest end-to-end: no path → "never discharged"
with the fix; a 40 Ω bleed meets the window but the `down_before`
ordering is STILL violated (the heavy-loaded prerequisite dies in
1 ms) with both times named; dropping the ordering is clean.

## 8. Multi-output supplies (PMICs) — aggregation as a post-step

Per-rail resolution runs FIRST, exactly as §3 defines: one stage per
rail, independently surveyed, priced, bound. THEN the aggregation
post-step (`bhdl_synthesizer::aggregation`) asks the question a
per-rail greedy survey structurally cannot see: could one multi-output
part cover a SET of these requirements? (A PMIC loses to a $0.40 buck
on every individual rail and wins on the set.)

A multi-output block declares its capability table as DATA:

```bhdl
attribute pmic_outputs = "DCDC1:buck:1.8V:1.2A,DCDC2:buck:1.1V:1.2A,…";
attribute pmic_seq     = "LDO1,DCDC1,LDO2,LDO3,LDO4,DCDC2|DCDC3";
attribute pmic_seq_dly = "1ms-10ms";
```

Fixed voltages are the HONEST model of an OTP part: the ordered
TPS65217B boots at the B-variant defaults; changing them is an I2C
runtime act, not a board-design one. The first implementer is
`Pmic_TPS65217B` (`bhdl-stdlib/power/tps65217.bhdl`, SLVSB64I): three
1.2 A bucks + LDO1/2 (100 mA) + LS1/2-as-LDO3/4 (200 mA), per-rail
application circuits wired-gated (an unused rail carries no parts),
θJA 30.4, and the factory-fixed strobe sequencer — the sequencing a
covered rail set inherits FOR FREE. Scope stated in the file: the
charger/power-path, WLED and I2C subsystems are not modeled.

The evaluation: each resolved requirement matches an unused output
when the fixed voltage equals the requirement's vout (within 2 %, the
accuracy class), the derated current (i_max/0.8) fits the output's
rating, and the input rail lies in the PMIC's range. Greedy
assignment; the report prints the cover, the honest leftovers, the
REAL price comparison (PMIC silicon via the supplier provider vs the
Σ of the displaced bound discretes — the probe's verdict: TPS65217B
$2.50 vs $0.70 of discretes, "the discretes win on silicon; the PMIC
may still win on area/BOM-lines/sequencing"), and the built-in
power-up order. NOTHING is auto-bound: aggregation is REPORTED — the
designer's lever, exactly like a template commit. The strict
sequencing gate (built-in strobe order vs declared domain ordering)
and the grouped `resolve`-commit land in a future increment, stated;
ERC033's one-block-drives-both-rails hook is already waiting for it.

## 9. The power-delivery report — `bhdl pdreport`

The capstone deliverable: the document that convinces a power engineer
the tool did its job. One markdown file (inline SVG curves), and every
number in it was computed by the SAME pipeline that gates every build,
or cited from a datasheet — the report renders, it never re-derives:

1. **Power topology** — every stage, its bound block, topology class,
   feed → rail, and the key datasheet figures (fSW, RDS(on), current
   limits, θJA, η).
2. **Requirement resolution** — every survey VERBATIM: each
   candidate, each gate, the near-misses, the prices, the ranking
   basis; plus the PMIC aggregation options (§8) when a set is
   coverable.
3. **Sizing** — the application-circuit values the blocks' `design{}`
   procedures computed (dividers, inductors, banks), SI-formatted;
   the symbolic derivations live in the cited block sources; the full
   stress sign-off is `bhdl report`.
4. **Simulated curves** — V(t) per rail from the PWL engine's actual
   samples (waveform capture on the power-up, escalated-interaction,
   input-loss and sleep runs), rendered as inline SVG with nominal
   markers; every modeling approximation printed above them, stated.
5. **Power-up** — the event timeline, per-rail good-times and sags,
   the load-step table, the superposition screen with its
   self-consistency verdict, and the findings verbatim.
6. **Power-down and sleep** — both scenarios' timelines and findings.
7. **Decap networks** — per `decouple`: the greedy commits with
   worst-|Z|/mask after each, margin adds, single-open verification
   counts, the final ratio.
8. **Final PDN sanity** — the loop-stability envelope verdicts and
   the resonance blind spots, stated.

`bhdl pdreport [--output <file>]`, default `<input>.pd.md`.

### 8.1 The grouped commit and the strict sequencing gate

Aggregation becomes COMMITTABLE:

```bhdl
resolve u1, u2, u3 = Pmic_TPS65217B;
```

A text transform (the resolver's discipline), applied before per-rail
scanning: every named instance's requirement must match an unused PMIC
output (the same gates as the §8 report — a miss is a HARD ERROR
naming the requirement, the output table, and why); the first
instance's requirement instantiation becomes the PMIC block, the
others' statements are removed, every endpoint reference is remapped
(`uk.VOUT` → `<first>.VOUT_<output>`; VIN/GND coalesce — duplicate
identical connections are benign), `PWR_EN` is tied to the feed rail
when unwired (the sequencer starts with the supply, stated), and the
requirement→output mapping is stamped (`pmic_committed`). A per-rail
`EN`/`PG` reference under a grouped commit is a hard error: the
SEQUENCER owns the enables. A grouped commit takes a bare block name —
the rails are OTP-fixed, there are no ctor args to pass.

**The strict sequencing gate** (ERC033's same-instance hook, upgraded):
for an ordering edge whose BOTH rails one block drives, the block's
promised strobe order (`pmic_seq`) is checked — a contradicted order
is an Error naming both strobes ("rails cannot be re-strobed at board
level; reassign the outputs"), and timing composes against the
promised inter-strobe delay RANGE (`pmic_seq_dly`): t_min guaranteed
even at the minimum delay = a promise-based PASS (Info, "inherited
from the built-in sequencer"); t_min beyond the maximum achievable
spacing (strobes × max delay) = Error with the arithmetic; inside the
programmable window = stated UNCHECKED ("depends on the PROGRAMMED
delays — verify the OTP/firmware DLYx values"). The power-up engine
idealizes a PMIC's output rails at their promised voltages (internal
sequencer/soft-start not modeled — stated in the report header);
ERC033's promise check is the sequencing authority for them.
