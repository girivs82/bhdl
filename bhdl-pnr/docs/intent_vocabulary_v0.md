# P&R Intent Vocabulary v0

> **Status:** Proposal v0. Scope: the typed intent kinds that bhdl-stdlib
> entities use inside `expansion { }` blocks (and that board authors may use
> on nets/components) to drive geometric/electrical constraints in
> `bhdl-pnr`. This document is the contract between stdlib annotation work
> (other session's domain) and the P&R recipe engine (this session's
> domain).
>
> **Non-goals:** the constraint algebra that intents lower to (covered in
> a sibling doc, `constraint_model_v0.md`, TBD); the placement/routing
> cost-function changes that consume those constraints; ML.

## 1. Motivation and framing

bhdl carries circuit *intent* that traditional EDA does not see: the
stdlib author of `ATmega328P_DIP28` knows that `C_vcc` is a high-frequency
bypass cap, that `C_bulk` is a low-frequency reservoir, that `C_aref`
filters the analog reference. Today that knowledge is encoded only as
connectivity; P&R has to re-infer it (and gets it wrong on edge cases like
buck converters whose expansion also produces inductors and Schottkys).

This document defines a small, typed vocabulary of **intent kinds**.
Annotating an expansion-born component with one — e.g.
`Cap(100nF) for high_freq_bypass(rail: VCC, return: GND1, loop_area_max: 1.5mm²)` —
gives bhdl-pnr unambiguous, datasheet-rooted information about how that
component must be laid out.

### 1.1 Sources of layout intent in bhdl

| Source | Where it lives | Governs | Example |
|---|---|---|---|
| **Expansion intent** (this doc) | `for INTENT(...)` on `expansion { }` children | support-passive *placement* | decoupling cap → `high_freq_bypass` |
| **Interface constraints** (`Interfaces.md` §13) | `constraints { }` on an interface | signal-net *routing* | DDR4 `DQS.*: differential 80ohm` |
| **Structural** | Born-from-`expansion`-block provenance | placement fallback | caps born from MCU's expansion |
| **Hierarchical** | Module nesting | placement cohesion | power module wants components together |

This vocabulary covers the **expansion-intent** path: typed
`LayoutIntent` annotations on the support passives a chip's
`expansion { }` materializes. It deliberately does **not** cover
signal-net properties (impedance, length match, diff pairs, skew) —
those come from the **interface-constraint** mechanism, which is already
shipped and feeds the same constraint catalog (see §4.6 and
`constraint_model_v0.md` §1, §5a).

The intent enums also serve as the *annotation target* for the
**structural** path: when the stdlib author writes the expansion, they
attach an intent to each child, making provenance explicit rather than
inferred. The **hierarchical** path is handled separately by placement
grouping (already in `bhdl-pnr/src/placement/grouping.rs`) and not
covered here.

### 1.2 Relationship to `PlacementRecipe`

`bhdl-common/src/placement_recipe.rs` already defines a rigid form:
hand-placed `(dx, dy, rotation)` offsets relative to the parent, copied
from datasheet recommended layouts. That is the **strong** form — when
present, P&R should honor it directly.

Intent annotations are the **flexible** form — they let the placer
optimize globally subject to the constraints they imply (proximity, loop
area, layer hint, keepout). They coexist with `PlacementRecipe`:

| State | P&R behavior |
|---|---|
| `placement { }` present | Use absolute offsets verbatim |
| `for INTENT(...)` present (no `placement { }`) | Lower to constraints, optimize globally |
| Neither | Fall back to structural inference + generic heuristics |

Stdlib authors annotate with intent by default; they reach for
`PlacementRecipe` only when they have an authoritative reference layout
they want copied (e.g. switching-regulator IC reference designs).

## 2. Design principles

- **Typed, not stringly typed.** Each intent kind is a Rust enum variant
  (in `bhdl-common/src/intent.rs`) with typed named fields. No string
  parsing in the P&R consumer.
- **Host-relative references.** Intents inside `expansion { }` reference
  the host entity's pins by name (e.g. `rail: VCC`, `return: GND1`). The
  recipe engine resolves them to flat board nets after expansion +
  network coalescing. Board-level intents may reference board nets
  (`@VCC_3V3`) or component pins.
- **Additive and back-compat.** Expansion blocks without intent
  annotations still expand and route. Intent is purely additive
  guidance; absence means "use defaults / structural inference."
- **Unknown-intent policy: warn and degrade.** If `bhdl-pnr` sees an
  intent kind it doesn't recognize (newer stdlib, older P&R), emit a
  diagnostic and skip the constraint contribution — never fail the
  build. This lets the two sessions evolve without lockstep coupling.
- **Vocabulary is a versioned contract.** Adding an intent kind = minor
  bump (P&R + recipes both extend). Changing a kind's parameter shape =
  breaking; requires a deprecation cycle.
- **Recipe engine, not magic.** Each intent kind has exactly one lowering
  recipe in `bhdl-pnr/src/intent/recipes/<kind>.rs`. The recipe is the
  *only* place that converts intent → constraint. No scattered ad-hoc
  handling.

## 3. Parameter types

The vocabulary uses a small typed-parameter alphabet that extends what's
already in `bhdl-common/src/intent.rs::ParamType`:

| Type | Notes | Already in common? |
|---|---|---|
| `Voltage`, `Current`, `Frequency`, `Duration` | Existing electrical types | ✓ |
| `Length` | Distances (mm). New for v0. | needs add |
| `Area` | Loop areas (mm²). New for v0. | needs add |
| `Ohms` | Impedance/resistance. New for v0. | needs add |
| `Pin` | Host-pin reference (resolved by expansion context). New for v0. | needs add |
| `Net` | Board net reference (board-level intents only). New for v0. | needs add |
| `ComponentRef` | Sibling component reference inside the same expansion. | needs add |
| `LayerHint` | Enum: `Any | Top | Bottom | Inner | AdjacentToGroundPlane`. | needs add |
| `Topology` | Enum: `Star | DaisyChain | FlyBy | T`. | needs add |

These extend `ParamType` and `IntentValue` in `bhdl-common`. Editing
`bhdl-common` is on the boundary between sessions; we'll propose the
exact diff before touching it.

## 4. The vocabulary (v0)

Each entry: signature, semantics, what it lowers to. "P" = required
positional/named param; "?" = optional; defaults in [brackets].

### 4.1 Decoupling / power-integrity

#### `high_freq_bypass`

```
high_freq_bypass(
    rail: Pin,                    // P — the supply pin to decouple
    return: Pin,                  // P — the ground pin to return through
    loop_area_max: Area,          // P — datasheet loop budget [≤ 2 mm²]
    proximity_max: Length = 2mm,  // ? — placement distance to `rail`
)
```

**Semantics:** A small ceramic cap that must be placed adjacent to a
single supply pin, with the lowest-inductance possible return through a
specific ground pin. The component is `self`; its two pins connect
implicitly via the surrounding expansion wires.

**Lowers to:**
- `Proximity(self, rail.parent_component) ≤ proximity_max`
- `LoopArea(rail, self.pin1, self.pin2, return) ≤ loop_area_max`
- `LayerHint(self) = AdjacentToGroundPlane` (soft)

#### `bulk_reservoir`

```
bulk_reservoir(
    rail: Pin,
    return: Pin,
    proximity_max: Length = 10mm,
)
```

**Semantics:** Larger-value cap providing low-frequency current
reservoir; placement is less critical than `high_freq_bypass`.

**Lowers to:**
- `Proximity(self, rail.parent_component) ≤ proximity_max` (soft)

#### `analog_ref_filter`

```
analog_ref_filter(
    ref_pin: Pin,         // P — the AREF / VREF pin
    return: Pin,          // P — analog ground
    proximity_max: Length = 3mm,
)
```

**Semantics:** Quiet placement near an analog reference pin, returning
through analog ground; keep away from switching/digital nets.

**Lowers to:**
- `Proximity(self, ref_pin.parent_component) ≤ proximity_max`
- `LoopArea(ref_pin, self.pin1, self.pin2, return) ≤ 2 mm²`
- `KeepAwayFromNetClass(self, Switching | HighSpeedDigital, ≥ 3mm)` (soft)

### 4.2 Clocking

#### `crystal_load_cap`

```
crystal_load_cap(
    xtal_pin: Pin,                    // P — XTAL1 or XTAL2
    return: Pin,                      // P — typically GND nearest the crystal
    partner: ComponentRef,            // P — the other load cap of the pair
    proximity_max: Length = 3mm,
)
```

**Semantics:** One of a symmetric pair of load caps for a crystal
oscillator. Must be symmetric in trace length to its `partner`; trace
from cap to `xtal_pin` must be short.

**Lowers to:**
- `Proximity(self, xtal_pin.parent_component) ≤ proximity_max`
- `TraceLength(self.pin1, xtal_pin) ≤ 3 mm`
- `LengthMatch([self.pin1→xtal_pin, partner.pin1→partner_xtal_pin], ±0.2mm)`
- `KeepoutUnderCrystal(self)` (soft)

### 4.3 Switching power

#### `switching_input_filter`

```
switching_input_filter(
    rail: Pin,
    return: Pin,
    loop_area_max: Area,           // P — typically tight (≤ 4 mm²)
    switch_node_keepaway: Length = 2mm,
)
```

**Semantics:** Cin of a switching regulator. Drives the *hot loop* —
Cin → IC.VIN → IC.SW → Cin.return — whose area dominates EMC.

**Lowers to:**
- `LoopArea(rail, self.pin1, self.pin2, return) ≤ loop_area_max` (hard)
- `Proximity(self, rail.parent_component) ≤ 3 mm`

#### `feedback_divider`

```
feedback_divider(
    sense_node: Pin,           // P — output sense (top of divider)
    fb_pin: Pin,               // P — IC FB pin (bottom of resistor chain)
    keepaway_from: Pin,        // P — switch node to avoid
    keepaway_min: Length = 3mm,
)
```

**Semantics:** Resistors of the feedback divider for a switching
regulator. Trace from divider tap to `fb_pin` is sensitive; route short
and away from the switch node.

**Lowers to:**
- `Proximity(self, fb_pin.parent_component) ≤ 5 mm`
- `KeepAwayFromPin(self, keepaway_from, ≥ keepaway_min)`
- `TraceLength(divider_tap, fb_pin) ≤ 5 mm`

#### `snubber`

```
snubber(
    across: (Pin, Pin),        // P — the two nodes the snubber sits across
)
```

**Semantics:** RC or RD snubber across a switching node / diode. Loop
between the two pins must be minimized.

**Lowers to:**
- `LoopArea(across.0, self.pin1, self.pin2, across.1) ≤ 1.5 mm²`
- `Proximity(self, across.0.parent_component) ≤ 3 mm`

### 4.4 Signal conditioning

#### `series_termination`

```
series_termination(
    driver: Pin,               // P — the driver-side pin
    line: Net,                 // P — the signal net being terminated
)
```

**Semantics:** Source-terminated resistor on a fast signal; must sit
immediately adjacent to the driver.

**Lowers to:**
- `Proximity(self, driver.parent_component) ≤ 3 mm`
- `TraceLength(driver, self.pin1) ≤ 3 mm`

#### `gate_resistor`

```
gate_resistor(
    driver: Pin,               // P — gate driver output
    gate: Pin,                 // P — FET gate pin
)
```

**Semantics:** Series resistor on a MOSFET gate; place near the FET, not
the driver (the long trace tolerates being on the high-impedance side).

**Lowers to:**
- `Proximity(self, gate.parent_component) ≤ 3 mm`
- `TraceLength(self.pin2, gate) ≤ 2 mm`

#### `pullup` / `pulldown`

```
pullup(signal: Pin, rail: Pin)
pulldown(signal: Pin, return: Pin)
```

**Semantics:** Discrete pull resistor. Placement is unconstrained
geometrically; the value of annotating is *net classification* (the net
gets tagged as pulled, which downstream analyses can use).

**Lowers to:**
- No geometric constraint.
- Tag net `signal` with `NetTag::Pulled { rail | return }`.

### 4.5 Measurement

#### `current_sense`

```
current_sense(
    across: (Pin, Pin),
    topology: Topology,        // P — Kelvin | Standard
)
```

**Semantics:** Shunt resistor in a current path. Kelvin topology
requires a 4-wire sense; standard is 2-wire.

**Lowers to (Kelvin):**
- `TopologyConstraint(net_high, Star, root: self.pin1)`
- `TopologyConstraint(net_low, Star, root: self.pin2)`
- Sense traces must originate exactly at `self.pin1` / `self.pin2`, not
  on the high-current bus.

**Lowers to (Standard):**
- No special topology; tag net as carrying current ≥ X (consumed by
  trace-width sizing).

### 4.6 Differential and matched signals — NOT in this vocabulary

> **Removed from the intent vocabulary.** Differential pairs, length
> matching, impedance, skew, and signal class are **net/signal**
> properties that come from the *protocol*, not from a support-passive
> placement decision. They are already expressed — better — by the
> shipped v0.8 **interface `constraints { }` mechanism**
> (`Interfaces.md` §13):
>
> - **Diff pairs** are a structural hierarchical sub-interface
>   (`interface DiffPair { P; N }`) with `*: differential <z>ohm` on the
>   pair-as-a-unit and `P -> N: length_match` for intra-pair skew. One
>   definition serves USB / MIPI / PCIe / DDR.
> - **Length match / skew** are `length_match` / `skew_max` properties
>   in the interface's `constraints { }`.
> - **Impedance / signal class** are `single_ended <z>ohm` /
>   `signal_class <C>` properties.
>
> These reach bhdl-pnr as `intf_const__*` / `intf_const_rel__*` module
> attributes and are parsed into the `DiffPair`, `LengthMatchGroup`,
> `Impedance`, `Topology`, and `NetTag::SignalClass` constraints — see
> `constraint_model_v0.md` §1 (two-producer model) and §5a (the
> interface-constraint boundary).
>
> The intent vocabulary in this doc therefore covers **only the
> support-passive placement half** (proximity, loop area, layer hints).
> The net/signal half is the interface-constraint mechanism's job. Earlier
> drafts of this doc carried `diff_pair` and `length_match_group` intents;
> they are dropped to avoid two mechanisms expressing the same thing.

## 5. Worked example: ATmega328P decoupling annotated

The current expansion (paraphrased):

```bhdl
expansion {
    VCC  -> C_vcc:  Cap(100nF).1;  C_vcc.2  -> GND1;
    VCC  -> C_bulk: Cap(10µF).1;   C_bulk.2 -> GND1;
    AVCC -> C_avcc: Cap(100nF).1;  C_avcc.2 -> GND2;
    AREF -> C_aref: Cap(100nF).1;  C_aref.2 -> GND2;
}
```

Annotated with v0 vocabulary:

```bhdl
expansion {
    C_vcc:  Cap(100nF) for high_freq_bypass(
        rail: VCC, return: GND1, loop_area_max: 1.5mm²
    );
    VCC -> C_vcc.1;  C_vcc.2 -> GND1;

    C_bulk: Cap(10µF)  for bulk_reservoir(
        rail: VCC, return: GND1, proximity_max: 10mm
    );
    VCC -> C_bulk.1; C_bulk.2 -> GND1;

    C_avcc: Cap(100nF) for high_freq_bypass(
        rail: AVCC, return: GND2, loop_area_max: 1.5mm²
    );
    AVCC -> C_avcc.1; C_avcc.2 -> GND2;

    C_aref: Cap(100nF) for analog_ref_filter(
        ref_pin: AREF, return: GND2
    );
    AREF -> C_aref.1; C_aref.2 -> GND2;
}
```

After expansion + recipe lowering, `bhdl-pnr` consumes (paraphrased):

- `Proximity(C_vcc,  mcu.VCC,  ≤ 2 mm)`, `LoopArea(VCC,  C_vcc, GND1)  ≤ 1.5 mm²`
- `Proximity(C_bulk, mcu.VCC,  ≤ 10 mm, soft)`
- `Proximity(C_avcc, mcu.AVCC, ≤ 2 mm)`, `LoopArea(AVCC, C_avcc, GND2) ≤ 1.5 mm²`
- `Proximity(C_aref, mcu.AREF, ≤ 3 mm)`, `LoopArea(AREF, C_aref, GND2) ≤ 2 mm²`,
  `KeepAwayFromNetClass(C_aref, Switching, ≥ 3 mm, soft)`

These flow into placement's cost function as proximity penalties and
loop-area objectives. Total board-level annotation needed: **zero**.

## 6. Versioning and stability

- **v0 = this document.** All kinds listed in §4 are stable for v0;
  parameter names and types are part of the contract.
- **Minor bump (v0.1, v0.2, …):** add new kinds. Existing kinds unchanged.
  Old P&R + new stdlib silently ignores unknown kinds (warn-and-degrade).
- **Breaking bump (v1):** change parameter shape on an existing kind, or
  remove a kind. Requires a deprecation cycle with both forms supported
  for one minor cycle.
- **Vocabulary registry location:** `bhdl-common/src/intent/vocabulary.rs`
  (new file). Single source of truth; both stdlib parsing and P&R
  recipe dispatch reference the same names.

## 7. Open questions

- ~~**Pin reference syntax in expansion intents.**~~ **RESOLVED**
  (handshake §8.3): the `for INTENT(...)` clause grammar already exists
  (`bhdl-parser/src/intent.rs`, nodes `INTENT_CLAUSE`/`INTENT_CALL`);
  synth side threads parsed params through to a typed
  `layout_intents: Vec<LayoutIntent>` on the netlist `Instance`. Pin
  refs resolve at **lowering time** (string-now), so a typo surfaces as
  "no such pin on entity X," not a parse error.
- ~~**`Area` literal syntax.**~~ **RESOLVED** (handshake §8.3): the lexer
  will not accept the `²` superscript in a unit token. **Canonical form
  is `1.5mm2` (ASCII).** `²` may be added as an alias later. (The `mm²`
  spellings elsewhere in this doc are prose; the source-syntax token is
  `mm2`.)
- **Topology constraints in this doc vs. the constraint-model doc.**
  `current_sense(topology: Kelvin)` references `TopologyConstraint`,
  which is defined in the (forthcoming) constraint model doc. This doc
  names what the recipes emit; the constraint algebra is its own spec.
- **Coexistence with `for INTENT(...)` for entity selection.** The spec
  already uses `for INTENT(...)` on `@net` to drive entity selection
  (e.g. `@VCC for voltage_regulator(...)`). The P&R vocabulary uses the
  same surface form but a different (additive) set of intent kinds; the
  same `Intent` enum carries both. Different passes consume different
  variants. Need to confirm parser handles intent on component decls
  inside `expansion { }`, not just `@net`.
- **Should `proximity_max` defaults live in the recipe (engine-side)
  or be required at every annotation site?** Current draft uses
  defaults in the parameter signatures, which puts the policy in
  bhdl-common rather than bhdl-pnr. Could be argued either way; default
  there feels stdlib-friendly.

## 8. Out of scope for v0

- **Thermal intent** (`thermal_via`, `heat_sink_pad`) — defer to v0.2.
- **Impedance-controlled routing as a first-class intent** — needs
  stackup model; defer to v0.3.
- **EMI/shielding intents** (`shielded_trace`, `guard_ring`) — defer.
- **Mechanical / connector-fix intents** — handled today by
  `FixedPlacement`; don't need an intent kind.
- **Inductor placement** in switching regulators — has its own
  considerations (magnetic field, keep-away); deferred.
- **All ML/learned intent inference** — out of scope for the entire P&R
  thesis until the deterministic path works.

## 9. Implementation handshake

This document is the contract. Implementation splits cleanly:

**Other session (stdlib annotation work):**
- Extend `bhdl-common/src/intent.rs` with v0 `ParamType` additions
  (proposed diff to be reviewed by both sessions).
- Annotate `expansion { }` blocks in stdlib actives with v0 intents,
  starting with `ATmega328P_DIP28`.

**This session (P&R recipe engine):**
- Define typed `Intent` enum mirroring §4 in `bhdl-pnr/src/intent/mod.rs`.
  (Owned by `bhdl-pnr` initially; can be promoted to `bhdl-common` later
  once the boundary form is settled.)
- Write a string→typed intent parser at the `semantic.rs` intake boundary
  that reads today's string-attribute intent form
  (`intent_name`/`stage_name` + params) into typed `Intent` values.
  This supersedes the hardcoded string-match logic currently in
  `semantic.rs::intent_routing_constraints()` around line 710.
- Write `bhdl-pnr/src/intent/recipes/<kind>.rs` — one per intent kind in
  §4 — emitting constraints per the "Lowers to" sections.
- Wire constraint output into placement (`lib.rs` cost loop +
  `placement/optimizer.rs`) and routing (`routing/pathfinder.rs`) cost
  functions.
- First milestone: `tests/circuits/realistic/atmega328p_decoupling.bhdl`
  routes with caps adjacent to their parent pins.
