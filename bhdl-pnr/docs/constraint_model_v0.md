# P&R Constraint Model v0

> **Status:** v0 — **both producers implemented** (2026-05-30).
> *Geometric/placement half:* `bhdl-pnr/src/constraint/` (catalog + eval +
> shoelace loop area), `bhdl-pnr/src/intent/` (resolve + recipes +
> lowering), `bhdl-pnr/src/placement/intent_forces.rs` (proximity +
> loop-area Forces in the lib.rs cost loop). *Net/signal half:* the
> interface-constraint boundary reader (§5a),
> `bhdl-pnr/src/intent/interface_constraints.rs`, parses the shipped
> `intf_const__*` attributes into `Impedance` / `DiffPair` /
> `SignalClass` / `Topology` / `SwizzleGroup` — tested against the real
> DDR4 strings. Catalog variants `Topology`/`SwizzleGroup`/`SignalClass`
> landed. The reader is **wired into `semantic.rs`**:
> `build_board` parses each instance's `intf_const__*` module attributes
> and resolves dotted leaf pin-paths → `NetId` (via the netlist pin→net
> map) into `Board.constraints` — verified non-regressing against the
> full suite. Still pending: routing-side *evaluation* of these
> (the router consumes them; `eval` returns `Unknown` pre-routing), the
> §9 conflict pass, and a DDR4 board fixture that wires the *data* bus
> (the current fixture wires only power, so data-net resolution isn't
> exercised end-to-end yet — the parser/lowering/resolver chain is
> unit-verified). Scope:
> the typed constraint algebra that intent recipes emit and that
> placement + routing consume. Sibling of `intent_vocabulary_v0.md`;
> together they define the contract from stdlib annotation through to
> layout optimization.
>
> **Non-goals:** the intent surface syntax (covered in
> `intent_vocabulary_v0.md`); the specific cost-function changes inside
> the analytical placer (touched on in §7 but its own follow-up);
> stackup/impedance modeling (deferred to v1); ML.

## 1. Where this layer sits

```
┌──────────────────────────────┐   ┌──────────────────────────────┐
│  Expansion intent            │   │  Interface constraints       │
│  (intent_vocabulary_v0.md)   │   │  (Interfaces.md §13, SHIPPED) │
│  support-passive placement:  │   │  protocol net/signal rules:  │
│  proximity, loop area, layer │   │  impedance, length-match,    │
│                              │   │  skew, diff-pair, swizzle    │
└──────────────┬───────────────┘   └──────────────┬───────────────┘
   for INTENT(...) on expansion        intf_const__* / intf_const_rel__*
   children → typed LayoutIntent       module attributes on leaf pins
               │                                   │
               │  recipe lowering                  │  attribute → constraint
               │  (one recipe per kind)            │  parse (§6)
               ▼                                   ▼
              ┌─────────────────────────────────────┐
              │  Constraint set     ← THIS DOCUMENT  │
              │  (typed enum, hard | soft)           │
              └──────────────────┬──────────────────┘
                                 │  consumed by:
                                 ▼
              ┌─────────────────────────────────────┐
              │  Placement + routing                │
              │  cost-function terms + Lagrangian   │
              │  penalties + per-net router hints + │
              │  swizzle degrees-of-freedom (§5a.2) │
              └─────────────────────────────────────┘
```

The constraint set is the **central IR** for layout. It has **two
upstream producers**:

1. **Expansion intent** (`intent_vocabulary_v0.md`) — datasheet/vendor
   support-passive placement: proximity, loop area, layer hints. Lowered
   from typed `LayoutIntent` values via recipes. Covers the geometric /
   placement half of the catalog (§3.1).

2. **Interface constraints** (`Interfaces.md` §13, already shipped) —
   protocol-derived net/signal rules: impedance, length match, skew,
   differential pairs, signal class, topology, swizzle freedom. These
   arrive as `intf_const__<pin_path>__<prop>` and
   `intf_const_rel__<from>__<to>__<prop>` module attributes and are
   parsed into constraints (§5a). Covers the electrical / routing half
   of the catalog (§3.2) + net tags (§3.3).

The two producers are complementary, not overlapping: expansion intent
governs *where support components sit*; interface constraints govern
*how signal nets route*. Placement and routing are the only consumers;
both producers feed the same typed catalog with explicit source
provenance for diagnostics.

> **The v0.8 interface-constraint spec explicitly names "the downstream
> PCB router" as its consumer and defers tier-2 — multi-value storage,
> entity-level overrides, board-level additions, and cross-net conflict
> detection — "pending a real constraint consumer."** bhdl-pnr is that
> consumer; implementing this constraint model is what unblocks the
> deferred tier-2 work on the synth side.

### 1.1 Headline design move

Traditional EDA wires DRC + topological rules into the router and treats
placement as wirelength minimization with rough density. The constraint
model here makes the placement *objective* explicit and additive:

```
total_cost(layout) = Σ wirelength_term
                   + Σ density_term
                   + Σ proximity_term       ← intent-derived
                   + Σ loop_area_term       ← intent-derived
                   + Σ length_match_term    ← intent-derived
                   + Σ diff_pair_term       ← intent-derived
                   + λ · Σ hard_constraint_violations
```

Every intent-derived term carries provenance back to the intent that
produced it. The user (or a debugger) can ask "why is this cap stuck to
this pin?" and get back "`high_freq_bypass(C_vcc, mcu.VCC, ...)` from
`atmega328p.bhdl:133`."

## 2. Design principles

- **Typed catalog.** Each constraint kind is a Rust enum variant with
  typed fields. No strings, no free-form annotation channel.
- **Hard vs soft is a per-instance choice.** Most kinds support both:
  e.g. `Proximity` can be a hard cap (placer must respect it) or a soft
  penalty (placer balances it against other costs). The recipe picks.
- **Composable by sum.** Multiple constraints on the same entity simply
  sum their cost contributions. Hard constraints AND together; conflicts
  surface as diagnostics, not silent failures.
- **Evaluable on partial layouts.** Every constraint defines its
  evaluation against (a) placement-only, (b) placement + routes. Some
  emit `Unknown` until enough information is present (e.g. impedance
  needs a stackup and a routed trace).
- **Loop area is first-class.** A cheap centroid-polygon approximation
  lets `LoopArea` participate in placement cost *before* any wires
  exist. Post-routing, it's recomputed from actual trace geometry.
- **Source-traceable.** Every constraint carries `source: ConstraintSource`
  pointing back to (file, line, intent kind, intent params) — the
  recipe engine fills this when lowering.
- **Constraint set is persistable.** Serializable to JSON so users can
  diff "what constraints does my board imply?" across edits, and so the
  recipe-engine pass output is reviewable independently of placement.

## 3. The constraint catalog (v0)

Notation: each entry lists the variant, its typed fields, its hardness
options (`H` = hard supported, `S` = soft supported, `H/S` = both), and
its evaluation availability — `P` (placement-only suffices), `R` (needs
routes), `K` (needs stackup, deferred).

### 3.1 Geometric / placement constraints

| Constraint | Fields | Hardness | Eval |
|---|---|---|---|
| `Proximity` | `a: EntityRef, b: EntityRef, max_mm: f32` | H/S | P |
| `KeepAway` | `a: EntityRef, b: EntityRef, min_mm: f32` | H/S | P |
| `Keepout` | `region: Region, layers: LayerSet` | H | P |
| `KeepoutUnder` | `footprint_of: ComponentRef, entity: EntityRef, layers: LayerSet` | H | P |
| `LayerHint` | `entity: EntityRef, hint: LayerHint` | S | P |
| `FixedPose` | `entity: ComponentRef, x_mm: f32, y_mm: f32, rotation_deg: f32` | H | P |
| `SymmetricPair` | `a: ComponentRef, b: ComponentRef, axis: SymAxis` | H/S | P |
| `PreferRegion` | `entity: EntityRef, region: Region` | S | P |

`EntityRef` is a sum type — either a `ComponentRef` or a `PinRef`. This
lets `Proximity` mean "place these two components near each other" *or*
"place this component near that specific pin."

### 3.2 Electrical / routing constraints

| Constraint | Fields | Hardness | Eval |
|---|---|---|---|
| `LoopArea` | `loop: Vec<PinRef>, max_mm2: f32` | H/S | P (approx) / R (exact) |
| `TraceLength` | `from: PinRef, to: PinRef, max_mm: f32` | H/S | R |
| `TraceWidth` | `net: NetRef, min_mm: f32` | H | R |
| `DiffPair` | `p_net: NetRef, n_net: NetRef, spacing_mm: f32, length_match_mm: f32` | H | R |
| `LengthMatchGroup` | `nets: Vec<NetRef>, tolerance_mm: f32` | H/S | R |
| `Topology` | `net: NetRef, kind: TopoKind, root: Option<PinRef>, stub_max_mm: Option<f32>` | H | R |
| `ReturnPath` | `signal: NetRef, return: NetRef, kind: ReturnKind` | S | R |
| `Impedance` | `net: NetRef, target_ohms: f32, tolerance_pct: f32` | H | K |

`TopoKind`: `Star { root }`, `DaisyChain { order }`, `FlyBy`, `T { stub_max }`.
`ReturnKind`: `Plane`, `Trace`, `Mixed`.

### 3.2a Swizzle freedom (degrees of freedom, not a constraint)

| Variant | Fields | Hardness | Eval |
|---|---|---|---|
| `SwizzleGroup` | `members: Vec<NetRef>, scope: SwizzleScope` | — | R |

Unlike every other entry, `SwizzleGroup` is not a restriction — it is a
*grant of freedom*. It declares that the router may permute the
endpoint-to-net assignment among `members` (a fixed netlist normally
forbids this). Sourced from interface `swizzle_within_byte` /
`swizzle_across_bytes` properties (`Interfaces.md` §13). The router
treats the members as interchangeable and picks the assignment that
minimizes total routing cost, subject to all *other* constraints still
holding per the chosen assignment.

`SwizzleScope`: `WithinGroup` (permute leaf signals inside one bundle,
e.g. DQ0..DQ3+DM within a byte lane), `AcrossGroups { group_size }`
(permute whole bundles as units, e.g. reorder byte lanes).

This is a routing *input*, consumed at net-ordering / assignment time
(§7), not a cost term. See §5a.2 for how it interacts with a board that
fixes a specific permutation via a generate-loop.

### 3.3 Net classification (tags, not costs)

| Tag | Fields | Consumer |
|---|---|---|
| `NetTag::Pulled` | `net: NetRef, rail_or_return: NetRef` | analyzer/router net class |
| `NetTag::Switching` | `net: NetRef, f_sw_hint: Option<f32>` | EMC keepaway rules |
| `NetTag::AnalogSensitive` | `net: NetRef` | KeepAwayFromNetClass selectors |
| `NetTag::HighSpeed` | `net: NetRef, edge_rate_ns: Option<f32>` | routing prio + return-path |
| `NetTag::SignalClass` | `net: NetRef, class: String, max_freq_hz: Option<f64>` | routing prio, class-based rules |

`NetTag::SignalClass` carries the interface `signal_class` property
(`DATA`, `CLOCK`, `ADDR`, `DM`, …) and optional `max_freq`. The class
string is open-ended (it flows verbatim from the protocol's
`constraints { }` block, per `Interfaces.md` §13's text-bearing
vocabulary), so new classes need no change here.

Tags don't carry hardness; they're metadata that other constraints (or
the router itself) can dispatch on. Example: a `KeepAway` whose `b` is
`AnyOf(NetClass::Switching)` becomes meaningful only once switching nets
are tagged.

### 3.4 Cost shape (soft constraints)

A soft constraint pairs a cost shape with a weight:

```rust
enum CostShape {
    Linear,        // cost = w · max(0, error)
    Quadratic,     // cost = w · max(0, error)²
    Hinge { ramp_mm: f32 },  // 0 inside slack, linear ramp outside
    Exponential { k: f32 },  // cost = w · (exp(k · error) − 1)
}
```

`error` is the constraint-specific overshoot (e.g. for `Proximity`,
`error = max(0, distance − max_mm)`).

Quadratic is the default for geometric softness (smooth gradients for
the analytical placer); hinge is right for constraints with a clear
"good enough" zone (length match within tolerance); exponential for
DRC-shaped things that must blow up sharply outside a bound.

## 4. Evaluation semantics

Every constraint implements:

```rust
trait Constraint {
    fn source(&self) -> &ConstraintSource;
    fn hardness(&self) -> Hardness;
    fn eval(&self, layout: &LayoutSnapshot) -> Eval;
}

enum Eval {
    Satisfied,
    SoftCost(f64),
    Violated { cost: f64, slack: f32 },
    Unknown,            // not enough info in this snapshot
}
```

`LayoutSnapshot` is whatever the caller has: placement-only, placement +
partial routes, placement + complete routes. Constraints that need more
than is present return `Unknown` and are skipped without penalty.

The placer cost loop:

```
for c in board.constraints {
    match c.eval(&snapshot) {
        Eval::Satisfied        => {}
        Eval::SoftCost(x)      => total += x,
        Eval::Violated { cost, slack } => {
            total += cost;
            if c.hardness() == Hard {
                hard_violations.push((c.source(), slack));
            }
        }
        Eval::Unknown          => {}
    }
}
```

Hard violations get accumulated into a Lagrangian penalty term whose
multiplier ramps over the placement iteration schedule — same pattern the
existing analytical placer already uses for density.

## 5. The loop-area approximation

Loop area is the most P&R-relevant new objective. The math:

Pre-routing, the loop `[Pin_a, Pin_b, Pin_c, …]` has no wires; we don't
know the trace path. Approximate the enclosed area by the **signed
polygon area on pin centroids** via the shoelace formula:

```
A ≈ ½ · | Σᵢ (xᵢ · yᵢ₊₁ − xᵢ₊₁ · yᵢ) |
```

Properties that make this useful as a placement objective:

- **Cheap.** O(N) in loop length; loops are typically 3–4 pins.
- **Differentiable** in pin positions, so the analytical placer can
  follow the gradient.
- **Monotonically related** to actual routed loop area: moving the pins
  closer can only reduce achievable trace-loop area. Not exact, but the
  inequality `actual ≥ centroid_approx` is consistently directional.

Post-routing, `LoopArea::eval(routed_snapshot)` recomputes from the
actual trace polygon. The router can pass on its own per-loop-area
estimate during ripup-and-retry to refine.

Specific cases — for a 4-pin loop (rail, bypass.1, bypass.2, return),
the centroid polygon is a quadrilateral, and the area collapses to ≈ 0
when the cap is placed exactly between rail and return. That's the
correct optimum.

## 5a. Interface-constraint boundary

The second producer (§1) is the shipped v0.8 interface-constraint
mechanism. Unlike expansion intent — which we receive as typed
`LayoutIntent` values and lower via recipes — interface constraints
arrive as **module attributes** that the synth side already emits. We
parse those attributes into the same typed `Constraint` catalog.

> **Boundary fact (confirmed by synth session, handshake §8.2.1):**
> these are entries in the **per-module attribute map**
> (`module.attributes: HashMap<String, String>` on the `Module` backing
> each instance), **not** per-`Pin` or per-`Net` attribute objects.
> There is nothing to "walk on each net's pins." The reader iterates,
> per instance, `netlist.modules[inst.definition].attributes` and
> filters by the two prefix constants, which are `pub` in
> `bhdl-synthesizer/src/hierarchical_connectivity.rs`
> (`INTERFACE_CONSTRAINT_ATTR_PREFIX`,
> `INTERFACE_CONSTRAINT_REL_ATTR_PREFIX`). Use those constants, do not
> hardcode the prefix strings.

### 5a.1 Attribute forms consumed

Per `Interfaces.md` §13, two prefixes:

```
intf_const__<pin_path>__<prop>           = <value>   // per-signal
intf_const_rel__<from>__<to>__<prop>     = <value>   // pairwise relation
```

`<pin_path>` is a dotted leaf path (e.g. `ddr.lane2.DQS.P`). The
property vocabulary is **text-bearing and open-ended** on the synth
side (new property names need no grammar change there) — which means
*we* own the semantics of each property name. The v0 property → constraint
mapping:

| Interface property | Lowers to |
|---|---|
| `differential <z>ohm` (on a `*` pair-self) | `DiffPair { p_net, n_net, ... }` + `Impedance(z)` on both |
| `single_ended <z>ohm` | `Impedance { net, target_ohms: z, ... }` (→ `TraceWidth` floor in v0) |
| `length_match <t>` (rel form `A -> B`) | `LengthMatchGroup` / pairwise length-match |
| `skew_max <t>` (rel form) | skew bound on the `LengthMatchGroup` |
| `signal_class <C>` | `NetTag::SignalClass { class: C }` |
| `max_freq <f>` | `max_freq_hz` on the net's `SignalClass` tag |
| `topology <kind>` | `Topology { net, kind, ... }` |
| `swizzle_within_byte true` | `SwizzleGroup { scope: WithinGroup }` — members reconstructed by shared parent prefix (see below) |
| `swizzle_across_bytes true` | `SwizzleGroup { scope: AcrossGroups }` — members reconstructed by shared parent prefix |

> **Boundary fact (confirmed by synth session, handshake §8.2.2):**
> there is **no member list** in the encoding. Each leaf independently
> carries `intf_const__<pin>__swizzle_within_byte = true`. The reader
> reconstructs group membership from the **shared dotted parent
> prefix**: e.g. all `ddr.lane0.DQ*` + `ddr.lane0.DM` carrying
> `swizzle_within_byte` form one `WithinGroup`; all `ddr.lane*`
> carrying `swizzle_across_bytes` form one `AcrossGroups`. If
> prefix-reconstruction proves fragile, the synth side has offered to
> emit an explicit `intf_const__<pin>__swizzle_group_id = <n>` (a
> bounded tier-2 ask) — see handshake §9 for the decision.

Unknown property names: warn-and-degrade (§2), same as unknown intent
kinds. The lenient synth-side grammar means a board can carry a property
we don't yet interpret; we skip it rather than fail.

Time-domain values (`length_match 1ps`, `skew_max 100ps`) are given in
picoseconds of propagation delay. v0 converts to length via a fixed
propagation-velocity assumption (≈ 6.5 ps/mm for a typical inner-layer
microstrip) and notes the assumption in the constraint's source. A
proper per-layer velocity comes with the v1 stackup model.

### 5a.2 Swizzle: freedom vs. fixed choice

`Interfaces.md` §11.3/§13/§14 draw a sharp line we must honor:

- The interface's `swizzle_*` constraints declare *which permutations
  are legal* — a grant of freedom to the router.
- A board *may* realize one specific permutation with a generate-loop
  (the list literal is the permutation table). When it does, those nets
  are fixed and the freedom is spent.

So at constraint-build time we check: for the members of a
`SwizzleGroup`, did the board fix their assignment (explicit
connections present for all members)? If yes, the `SwizzleGroup` is
**inert** — the nets are ordinary fixed nets and the router has no
latitude. If no (members left for the router to assign), the
`SwizzleGroup` is **live** and the router picks the assignment (§7).

This is the one place bhdl hands the router *more* freedom than a
traditional netlist, not less — and it's exactly where intent pays off:
the router can choose the DQ-to-pad mapping that untangles a crossing,
something a fixed netlist forbids. v0 may implement only the inert case
(honor swizzle metadata, but require the board to fix the permutation)
and defer live router-chosen swizzle to v0.1 — but the constraint
must be *represented* from v0 so the information isn't lost.

### 5a.3 Where this is parsed

A boundary reader in `bhdl-pnr/src/intent/interface_constraints.rs`
iterates each instance's module-attribute map
(`netlist.modules[inst.definition].attributes`, per §5a.1), filters by
the two prefix constants, resolves `<pin_path>` to `NetRef`s, and emits
constraints with `ConstraintSource { intent_kind: "interface:<prop>",
... }`. Runs in the same lowering pass as expansion-intent recipes
(§14), populating the same `Board.constraints`.

**Pin-path stability (handshake §8.2.3, open):** the dotted paths are
stable in the `extract_hierarchical_connectivity` output but may be
renamed by hierarchical-module flattening on the
`synthesize_from_source` path. Which netlist `bhdl-pnr` consumes is a
pending coordination item (handshake §9) — the boundary reader must not
hardcode pin-path parsing until the canonical form is agreed.

## 6. Provenance

Every constraint carries source attribution:

```rust
struct ConstraintSource {
    file: PathBuf,           // bhdl source location
    line: u32,
    intent_kind: String,     // e.g. "high_freq_bypass"
    intent_params: BTreeMap<String, IntentValue>,
    recipe_version: String,  // intent vocabulary version that emitted this
}
```

This serves three purposes:

1. **Diagnostics.** "C_vcc must be within 2mm of mcu.VCC — required by
   `high_freq_bypass` at `atmega328p.bhdl:133`."
2. **Debugging the recipe engine.** When a constraint set seems wrong,
   you can trace which intent and which recipe version produced it.
3. **Conflict resolution.** When two constraints disagree (one says
   "near A," another says "far from A"), the diagnostic names both
   sources so the user can fix the intent or pin-override the conflict.

## 7. Integration with placement and routing

The existing analytical placer ([bhdl-pnr/src/lib.rs](../src/lib.rs))
already runs an iterative cost-minimization with terms for analytical
wirelength (LSE), density (FFT), group cohesion, and thermal spreading.
The constraint model adds three terms to that cost function and one
Lagrangian penalty:

| Existing term | New term added | Source |
|---|---|---|
| HPWL | (unchanged) | wirelength |
| Density (FFT) | (unchanged) | overlap avoidance |
| Cohesion | (unchanged) | module grouping |
| Thermal | (unchanged) | spreading hint |
| — | **Proximity sum** | `Proximity` constraints (soft) |
| — | **Loop-area sum** | `LoopArea` constraints (centroid approx, soft) |
| — | **Length-match variance** | `LengthMatchGroup` (soft) |
| — | **Diff-pair length term** | `DiffPair` (soft) |
| — | **Lagrangian Σ hard violations** | all `Hardness::Hard` constraints |

Each new term has a weight that ramps across the placement schedule,
analogous to how density weight ramps to break out of folded states.
Initial weights are conservative; tuning happens on the ATmega fixture.

The routing side ([routing/pathfinder.rs](../src/routing/pathfinder.rs))
consumes:

- `TraceLength`, `LengthMatchGroup`, `DiffPair` → per-net routing cost.
- `Topology` → tree construction hint (Steiner-tree shape changes per
  kind: Star = root-centered, DaisyChain = serpentine, FlyBy = single
  spine with stubs).
- `TraceWidth` → minimum-width gate during cell expansion.
- `Keepout`, `KeepoutUnder` → obstacle injection.
- `LayerHint` → cell preference per layer.
- `NetTag::Switching` / `NetTag::HighSpeed` → priority + return-path-aware
  routing.

`Impedance` is deferred to v1 (needs stackup model); for v0 it lowers to
a `TraceWidth` floor derived from a simple lookup table.

## 8. Worked example: ATmega decoupling end-to-end

The fixture [tests/circuits/realistic/atmega328p_decoupling.bhdl](../../tests/circuits/realistic/atmega328p_decoupling.bhdl)
after intent annotation (per `intent_vocabulary_v0.md` §5) lowers to
this constraint set:

```
// from high_freq_bypass(rail: VCC, return: GND1, loop_area_max: 1.5mm²)
Proximity { a: C_vcc, b: mcu.VCC, max_mm: 2.0, hard: Hard,
            source: high_freq_bypass@atmega328p.bhdl:133 }
LoopArea  { loop: [mcu.VCC, C_vcc.1, C_vcc.2, mcu.GND1], max_mm2: 1.5,
            hard: Soft(Quadratic, w=4.0),
            source: high_freq_bypass@atmega328p.bhdl:133 }
LayerHint { entity: C_vcc, hint: AdjacentToGroundPlane,
            hard: Soft(Linear, w=1.0),
            source: high_freq_bypass@atmega328p.bhdl:133 }

// from bulk_reservoir(rail: VCC, return: GND1, proximity_max: 10mm)
Proximity { a: C_bulk, b: mcu.VCC, max_mm: 10.0, hard: Soft(Linear, w=1.0),
            source: bulk_reservoir@atmega328p.bhdl:134 }

// from high_freq_bypass(rail: AVCC, return: GND2, loop_area_max: 1.5mm²)
Proximity { a: C_avcc, b: mcu.AVCC, max_mm: 2.0, hard: Hard, ... }
LoopArea  { loop: [mcu.AVCC, C_avcc.1, C_avcc.2, mcu.GND2], max_mm2: 1.5,
            hard: Soft(Quadratic, w=4.0), ... }

// from analog_ref_filter(ref_pin: AREF, return: GND2)
Proximity { a: C_aref, b: mcu.AREF, max_mm: 3.0, hard: Hard, ... }
LoopArea  { loop: [mcu.AREF, C_aref.1, C_aref.2, mcu.GND2], max_mm2: 2.0,
            hard: Soft(Quadratic, w=4.0), ... }
// (KeepAwayFromNetClass deferred until other intents tag switching nets)
```

The placer's iterative optimization, given this set:

1. Density + HPWL keep C_vcc / C_avcc / C_aref from overlapping each
   other and the MCU body — a coarse layout emerges.
2. Hard `Proximity` constraints with `max_mm: 2` collapse each cap to
   the immediate neighborhood of its parent pin. The Lagrangian penalty
   dominates until satisfied.
3. `LoopArea` soft costs (quadratic) orient each cap so the pin1→pin2
   axis bridges the rail → return shortest line — i.e. caps end up
   "straddling" the supply/ground pair.
4. C_bulk's soft proximity nudges it within 10mm of mcu.VCC but lets
   density and routing have say.

Expected layout: each small cap sits on top of (or immediately beside)
its parent pin, orientation aligned to the rail/return pair; C_bulk sits
nearby but with more freedom. This matches a hand-routed reference Uno
layout for the same chip neighborhood, with no board-level annotations
and no manual `PlacementRecipe`.

## 9. Conflicts and diagnostics

The constraint set is a flat `Vec<Constraint>`; conflicts are detected
by a dedicated pass (`bhdl-pnr/src/constraint/conflicts.rs`,
**implemented** — run from `place_and_route` before the placement loop,
logging errors/warnings) before
placement begins:

- **Same-axis contradictions.** `Proximity(a, b, ≤ 2mm)` +
  `KeepAway(a, b, ≥ 5mm)` → `ConflictError::DistanceContradiction`
  with both sources named.
- **Topology over-determination.** Two `Topology { net: N, ... }` with
  different `kind` → `ConflictError::TopologyOverdetermined`.
- **Fixed-pose collision.** Two `FixedPose` whose footprints overlap →
  `ConflictError::FixedPoseOverlap`.
- **Unreachable hard constraints.** Hard `LoopArea ≤ 0.5mm²` on a loop
  whose minimum-conceivable area (sum of pad sizes) exceeds it →
  `ConflictWarning::Infeasible`.
- **Cross-net protocol contradiction.** Two `Impedance { net: N, ... }`
  (or two `SignalClass`, etc.) on one net with disagreeing targets —
  e.g. `ddr.lane0.DQ0` (34Ω) and `sensor.dq0` (40Ω) merged onto net N
  by board wiring → `ConflictError::ImpedanceContradiction`. This is the
  *same shape* as topology over-determination: same-net disagreement
  among lowered constraints, one more case in this pass, no new
  mechanism.

  > **Ownership (handshake §10).** This conflict only *exists* after
  > board net-merge joins two interfaces' pins onto one net; the synth
  > side emits `intf_const__*` per-module *before* any board connection,
  > so it is structurally blind to the contradiction at emit time.
  > Detection is therefore P&R's job (this pass, post-net-merge). v0
  > diagnostics use the pin paths already present in the attribute keys
  > ("net N: `ddr.lane0.DQ0` says 34ohm, `sensor.dq0` says 40ohm").
  > **Source-location enrichment** (the conflicting constraints' `.bhdl`
  > file:line) is a deferred synth-side tier-2 sub-item (synth task #96):
  > synth emits constraint origins alongside `intf_const__*` once this
  > pass exists and wants them. P&R *detects*; synth *enriches*.

Conflicts default to *errors* for hard constraints, *warnings* for soft.
A user can downgrade hard conflicts to warnings via
`board { layout_policy { allow_constraint_conflicts = true } }`.

## 10. Persistence and inspection

The constraint set serializes to JSON, both as a debugging aid and to
make the recipe-lowering pass output independently reviewable:

```json
{
  "version": "0",
  "board": "ATmega328P_Decoupling_Test",
  "constraints": [
    {
      "kind": "Proximity",
      "a": { "component": "C_vcc" },
      "b": { "pin": "mcu.VCC" },
      "max_mm": 2.0,
      "hardness": "Hard",
      "source": {
        "file": "bhdl-stdlib/actives/atmega328p.bhdl",
        "line": 133,
        "intent_kind": "high_freq_bypass",
        "intent_params": {
          "rail": "VCC",
          "return": "GND1",
          "loop_area_max": "1.5 mm²"
        },
        "recipe_version": "0"
      }
    }
    // ...
  ]
}
```

CLI: `bhdl pnr --emit-constraints board.bhdl > constraints.json` runs
intent lowering + conflict detection and stops before placement. Useful
for review, diffing, and CI gates ("this PR added 3 new hard
constraints; check they're intended").

## 11. Versioning

Same model as `intent_vocabulary_v0.md`:

- **v0 = this document.** Variants listed in §3 are stable; field names
  and types are part of the contract.
- **Minor bump:** add variants; existing kept unchanged. Old placers +
  new recipes silently ignore unknown variants (warn-and-degrade).
- **Breaking bump:** change field shape or remove a variant. Requires a
  deprecation cycle with both forms supported in the parser.

Versions of intent vocabulary and constraint model are independent. A
v0 intent vocabulary may lower into v0 *or* v0.1 constraints; a v0.1
intent vocabulary may lower into v0 constraints if no new constraint
kinds are needed.

## 12. Out of scope for v0

- **Stackup model and impedance-controlled routing.** `Impedance` is in
  the catalog with eval class `K` (needs stackup); a stub lowering to
  `TraceWidth` is the v0 placeholder. Real impedance routing is v1.
- **Thermal constraints.** Need a thermal solver; v0 keeps the existing
  "thermal spread" placement term as a generic hint.
- **3D constraints** (component height profiles, mating-connector
  clearance, mechanical enclosures). v0 ignores Z entirely.
- **Multi-board / interposer constraints.** Per-board only.
- **Cross-net constraints beyond length match.** No coupling-budget
  constraint, no global power-integrity solver, no crosstalk model.
- **ML.** Not until the deterministic path is on its feet.

## 13. Open questions

- **`EntityRef` representation.** Component-or-pin sum type vs.
  always-pin (with "any pin of component X" sugar). Sum type reads
  better in source; uniform-pin is simpler to evaluate. Leaning sum type.
- **Cost-shape defaults per constraint kind.** Each kind should pick a
  sensible default (`Proximity` → quadratic, `LengthMatchGroup` → hinge,
  `LoopArea` → quadratic) so recipe authors usually don't specify.
  Document the defaults next to each kind in §3.
- **`LoopArea` for non-planar loops.** Multi-via loops on different
  layers — the centroid approximation degenerates. v0 punts (assumes
  same-layer); v1 adds a Z-aware variant.
- **Constraint set as part of `Board` vs. sidecar.** Today
  `bhdl-pnr/src/types.rs::Board` doesn't carry constraints; adding a
  `constraints: Vec<Constraint>` field is the obvious shape, but a
  sidecar `ConstraintSet` keeps `Board` lean. Probably field on `Board`.
- **Ordering of conflict detection vs. recipe lowering.** Could run
  conflict detection per-intent before lowering completes, with better
  error messages naming intents not constraints. Trade-off: simpler
  pipeline (lower then detect) vs. richer errors (interleave).

## 14. Implementation handshake

The constraint model is consumed entirely within `bhdl-pnr`; no
`bhdl-common` extensions are required by v0 (the intent vocabulary doc
covers the param-type additions to `bhdl-common`).

**Files to create in `bhdl-pnr/src/`:**

- `constraint/mod.rs` — `Constraint` enum, `Hardness`, `CostShape`,
  `Eval`, `LayoutSnapshot` trait.
- `constraint/proximity.rs`, `loop_area.rs`, `length_match.rs`,
  `diff_pair.rs`, `topology.rs` — one file per kind family with
  `eval()` implementations.
- `constraint/conflicts.rs` — conflict-detection pass.
- `intent/recipes/<kind>.rs` — one per intent kind in
  `intent_vocabulary_v0.md` §4, emitting constraints.
- `intent/interface_constraints.rs` — the boundary reader for
  `intf_const__*` / `intf_const_rel__*` module attributes (§5a),
  emitting the net/signal half of the catalog. No upstream change
  needed: the synth side already emits these attributes (shipped v0.8).
- `intent/lowering.rs` — the unified driver: walks both expansion
  intents (via recipes) and interface-constraint attributes (via the
  boundary reader), populates `Board.constraints`.

**Files to extend:**

- `types.rs::Board` — add `pub constraints: Vec<Constraint>`. (`Board`
  already carries `placement_recipes: HashMap<_, PlacementRecipe>` — the
  rigid hand-placement form. Intent-derived constraints are the flexible
  form that coexists with it.)
- `types.rs::PnrNet` — replace `intent: Option<String>` placeholder with
  `intent: Vec<Intent>` (the placeholder is read by nothing today, so
  removing it is free).
- `types.rs::Component` — add `intent: Vec<Intent>`.
- `lib.rs` placement cost loop (around lines 120–195) — accumulate the
  four new soft `Forces` terms and the Lagrangian hard-penalty term
  (§7). Each new term computes per-component `(dx, dy, dθ)` gradients
  and is added via `forces.accumulate(&new, lambda_new)` — same pattern
  as the existing density / cohesion / thermal / via-penalty terms.
- `routing/pathfinder.rs` — consume routing-side constraints inside the
  Dijkstra inner loop (around lines 200–209) and at net-priority sort
  (line 31).
- `semantic.rs::intent_routing_constraints()` (around line 710) — the
  current hardcoded string-match logic gets superseded by the typed
  intent → constraint pipeline.

**First milestone (same as in `intent_vocabulary_v0.md`):**
`atmega328p_decoupling.bhdl` produces a layout with each cap adjacent
to its parent pin, comparable to a hand-routed Uno's chip neighborhood.

After that:
- Annotate the Arduino Uno port (`emit_arduino_uno.rs` output) with
  intents covering its USB diff pair, ADC, crystal, and bypass network.
- Route it end-to-end and diff against the reference KiCad PCB.
