# Parameterization and BOM Resolution

> **Status:** Proposal v0.1. Scope: the three-form argument model
> (generic / runtime / intent), how virtual pins and design blocks
> participate, and the pipeline that turns parametric BHDL into a
> deterministic BOM.
>
> Out of scope for v0.1: the **cost function** itself (what makes one
> Yageo 10k preferable to a Panasonic 10k on a given board), board-
> level `bom_preferences { }` syntax beyond the headline shape, and
> the format of the part database the SKU resolver queries.

## 1. Motivation

Every physical component is uniquely identified by the combination of
properties that distinguish it on a Digi-Key invoice. A 10 kΩ resistor
is a *different physical part* from a 1 kΩ resistor; a 1 % 10 kΩ is a
different physical part from a 5 % 10 kΩ; a 10 kΩ 1 % 0603 is a
different physical part from a 10 kΩ 1 % 0805. Each combination shows
up as its own line on a BOM and its own reel in pick-and-place.

The language must reflect this. Anything that distinguishes physical
parts belongs in a type position — not as a free-form attribute string
parsed at run time, and not as a runtime constructor argument that
makes two physically different parts share an entity.

This document defines:

1. **Three argument forms**, one per scenario, with no overloading.
2. **Class vs. specific resolution** — whether a fully-monomorphised
   entity name is already an MPN or still names an equivalence class
   the BOM layer must narrow.
3. **Virtual pins, `design` blocks, and `design for INTENT` blocks** —
   how computed values inside an entity feed the generics of its
   internal children.
4. **The synthesis pipeline** — the four passes that take a parametric
   board description and produce a deterministic, MPN-resolved BOM.

The model deliberately mirrors how a human electrical engineer
reasons: *here's the part I picked, configure it*; *here's a
requirement, you pick the part*; *here's an equivalence class, the
purchasing layer picks the SKU*.

## 2. The three argument forms

### 2.1 Generic parameters — `<…>`

Used for axes whose value **changes the physical part you order**.

```bhdl
entity Resistor<
    R:   resistance,
    TOL: percentage = 5%,
    PKG: package    = "0603",
>() {
    pin 1: signal inout;
    pin 2: signal inout;
    attribute component_class = "resistor";
    attribute resistance      = R;
    attribute tolerance       = TOL;
    attribute package         = PKG;
    attribute resolution      = "class";
}
```

- Every distinct generic tuple monomorphises to a distinct entity:
  `Resistor<10kΩ, 1%, "0603">` and `Resistor<10kΩ, 5%, "0603">` are
  *different* entities post-mono, with different mangled names.
- Default values fill in unspecified positions: `Resistor<10kΩ>`
  expands to `Resistor<10kΩ, 5%, "0603">`.

The rule for choosing generic axes is mechanical:

> **An axis is generic if and only if changing it requires ordering
> a different part number** (or selecting from a different
> equivalence class).

For a resistor that's `R`, `TOL`, `PKG`, and optionally `P_MAX` and
`TC`. For a fixed-V LDO it's `V_OUT`. For an op-amp it's the
package and the channel count, but not the supply voltage (which is
a *spec window*, not an axis). When in doubt: would a designer have
to change the BOM line to change this value? If yes, it's generic.

### 2.2 Runtime parameters — `(…)`

Used for **same-part configuration**: values that are computed at
design time, may participate in calculation inside `design { }`
blocks, but do **not** change the part you order.

```bhdl
entity LM317(v_out: voltage = 5V) {
    pin VIN:  power in;
    pin VOUT: power out virtual;
    pin ADJ:  feedback in;
    pin GND:  ground;

    attribute resolution = "specific";
    attribute mpn        = "LM317T";

    design {
        const v_ref = 1.25;
        require v_out >= v_ref + 0.1
            else "LM317 minimum V_OUT is V_REF + headroom (~1.35V)";
        r2_value = 240;
        r1_value = 240 * (v_out / v_ref - 1.0);
    }

    expansion {
        VIN  -> C_in:  Capacitor<10µF>.1;          C_in.2  -> GND;
        VOUT -> C_out: Capacitor<22µF>.1;          C_out.2 -> GND;
        VOUT -> R1:    Resistor<r1_value, 1%>.1;   R1.2    -> ADJ;
        ADJ  -> R2:    Resistor<r2_value, 1%>.1;   R2.2    -> GND;
    }
}
```

- `LM317T` is one MPN regardless of `v_out`. `v_out` is therefore
  **not** generic. It's a runtime parameter.
- The `design { }` block runs the Rhai body to derive `r1_value` and
  `r2_value`. Those flow into the **generics** of the internal
  `Resistor<…>` instances. The mono pass then specialises those.
- A runtime parameter's value **must be resolvable at synthesis
  time** — it's a compile-time constant in the BHDL sense, not a
  runtime register. The name "runtime" here distinguishes it from
  "generic", which is part of the *type*.

### 2.3 Intent attachment — `for INTENT(…)`

Used when the **implementation is not chosen yet**. The user declares
a *requirement* against a net (or, more rarely, an instance) and a
late binding pass picks an entity from the candidate set whose
`design for INTENT { }` block can fulfil it.

```bhdl
// User: "I need a 3.3V rail somehow."
@VCC_3V3 for voltage_regulator(v_in: 5V, v_out: 3.3V, i_max: 500mA);

// Matcher considers all entities with `design for voltage_regulator`:
//   - AP2112K<3.3V>           (fixed-V LDO)
//   - LM317                   (adjustable LDO)
//   - BuckRegulator<3.3V>     (switching)
//   - …
// It picks the best fit by some policy (efficiency, cost, board area).
```

Each candidate entity carries a `design for INTENT { }` block whose
Rhai body computes its parameters from the intent's `intent.*` fields.
This is the form already used in the current stdlib for
`design for current_source { }` and `design for amplifier { }`.

**Critical distinction**: once the user writes `LM317(v_out: 5V)`
they have *already chosen* the part. The `for voltage_regulator(…)`
form is **wrong** for that case — it would be expressing a
requirement against an entity that has already committed to an
implementation. Use `(…)` for "I picked it, configure it"; use
`for INTENT(…)` for "you pick it."

### 2.4 Summary table

| Form | When | Resolves at | Example |
|---|---|---|---|
| `Entity<T1, T2, …>` | Part-defining axes | Monomorphization | `AP2112K<3.3V>()` |
| `Entity(p1, p2, …)` | Same-part configuration | Designer Rhai, then mono on internals | `LM317(v_out: 5V)` |
| `@net for INTENT(…)` | Abstract requirement; impl not chosen | Intent matcher → designer → mono | `@VCC for voltage_regulator(...)` |

These three forms compose cleanly:

```bhdl
// Generic + runtime in one entity.
LDO: SomeFamily<V_VARIANT: voltage>(trim: percentage = 0%)(...) { … }

// Use site: choose the variant, configure the trim.
RAIL: SomeFamily<3.3V>(trim: 1%);
```

## 3. Class vs. specific resolution

After monomorphisation, an entity name is either:

| `resolution` | Meaning | What follows |
|---|---|---|
| `"specific"` | Name **is** the orderable part — MPN derives mechanically | Nothing; BOM line written |
| `"class"` | Name is an equivalence class; many MPNs satisfy it | Cost-function pass picks one MPN |

This split exists because most ICs collapse to a single MPN per
generic tuple (`AP2112K<3.3V>` ↔ `AP2112K-3.3TRG`), while passives
have thousands of pin-and-spec compatible MPNs across vendors.

### 3.1 Specific entities

A `"specific"` entity declares its MPN — either as a literal, or via
a template string interpolating its generic parameters:

```bhdl
entity AP2112K<V_OUT: voltage>() {
    attribute resolution   = "specific";
    attribute mpn_template = "AP2112K-{V_OUT_short}TRG";
    // … pins, attributes, etc.
}
```

`{V_OUT_short}` is a derived form (3.3 → "3.3", written without
trailing zeros). The template syntax is intentionally narrow: only
literal interpolation of generic-param values, no logic. Anything
that needs branching belongs in a `design { }` block, not in MPN
derivation.

Adjustable parts like LM317 take the literal form:

```bhdl
attribute resolution = "specific";
attribute mpn        = "LM317T";
```

### 3.2 Class entities

A `"class"` entity leaves the MPN unset and lets the BOM layer
choose. The cost function looks at:

- All MPNs in the part database whose attributes satisfy the
  monomorphised entity's generics (`R == 10kΩ ± TOL`, `PKG == "0603"`,
  etc.).
- Stock-on-hand, price, lead time, vendor preference.
- Per-board overrides in `bom_preferences { }`.

It picks one winner per `(class, board)` pair — never one winner per
*instance*, because a board with 30 instances of `Resistor<10kΩ, 1%,
"0603">` should not order 30 different reels of equivalent parts.

### 3.3 Board-level overrides

Boards can pin or hint MPN selection:

```bhdl
board MyBoard {
    bom_preferences {
        Resistor<10kΩ, 1%, "0603">    pin "Yageo:RC0603FR-0710KL";
        Resistor<*, *, "0603">        prefer_vendor "Panasonic";
        Capacitor<*, *, "X7R", *>     prefer_vendor "Murata";
    }
    // … instances …
}
```

`pin` is a hard binding (cost function must use the named MPN or
fail). `prefer_vendor` is a soft hint (used only as a tiebreaker).
The `*` wildcards in the LHS keys are placeholders for "any value of
this generic"; full syntax is left for v0.2.

## 4. Virtual pins and `design` blocks

### 4.1 Virtual pins declare an expansion contract

A pin marked `virtual` is *not* a physical pad on the part. It is an
expansion point where synthesis materialises supporting components
(decoupling caps, feedback dividers, snubbers, etc.) based on the
host entity's `expansion { }` and `design { }` blocks.

```bhdl
pin VOUT: power out virtual
    @requires(v_out: voltage);   // declares which runtime/intent
                                  // field must be supplied
```

The `@requires` clause makes the contract checkable: if a use site
instantiates the entity without supplying `v_out` (and there's no
default), the analyzer rejects the program *at type-check time*,
before any Rhai runs.

### 4.2 Two flavours of `design` block

| Form | Purpose | When the body runs |
|---|---|---|
| `design { … }` | Plain runtime computation (scenario A) | Once per instance, after constructor binding |
| `design for INTENT { … }` | Intent matcher (scenario B) | Once per intent attachment that picks this entity |

Both flavours have the same Rhai surface — they read inputs, write
outputs, may emit `require` diagnostics. The difference is the input
namespace:

- `design { }` reads from `self.*` (runtime params + generics).
- `design for INTENT { }` reads from `self.*` **and** `intent.*`
  (the requirement that matched this candidate).

An entity may have both — a `design { }` for the always-on
configuration computation, plus `design for INTENT { }` blocks for
each intent it can fulfil.

### 4.3 How values flow into expansion

The `design` block writes locals (`r1_value`, `c_out_value`, …). The
`expansion { }` block references those locals in generic-parameter
positions of internal components:

```bhdl
expansion {
    VOUT -> R1: Resistor<r1_value, 1%>.1; R1.2 -> ADJ;
    //                  ^^^^^^^^^
    //                  Set by the design block above.
}
```

Once the design block runs and locals are bound to concrete values,
the expansion produces concrete generic tuples on the internal
components — and those flow into the monomorphisation pass like any
other generic.

## 5. The synthesis pipeline

After parsing and type-checking, four passes lower a parametric
board to a deterministic, MPN-resolved BOM:

```
┌──────────────────────┐
│  1. Intent matcher   │  for each `@net for INTENT(…)`, pick the
│                      │  candidate entity whose `design for INTENT`
│                      │  block accepts the requirement. Bind the
│                      │  net to a fresh instance of that entity.
└──────────────────────┘
            │
            ▼
┌──────────────────────┐
│  2. Designer pass    │  for each instance, run its `design { }` and
│                      │  `design for INTENT { }` bodies (Rhai). Write
│                      │  computed values into the instance's locals.
└──────────────────────┘
            │
            ▼
┌──────────────────────┐
│  3. Expansion        │  for each entity with an `expansion { }` block,
│                      │  inline its body — virtual pins disappear,
│                      │  real children are materialised, their generic
│                      │  tuples filled from the locals computed above.
└──────────────────────┘
            │
            ▼
┌──────────────────────┐
│  4. Monomorphization │  walk the now-flat tree; for each parametric
│                      │  entity, emit one specialised copy per distinct
│                      │  generic tuple. Deterministic mangled names.
└──────────────────────┘
            │
            ▼
┌──────────────────────┐
│  5. BOM resolution   │  for each specialised entity:
│                      │   - `resolution = "specific"` → derive MPN from
│                      │     `mpn_template` or `mpn` literal.
│                      │   - `resolution = "class"`    → cost function
│                      │     picks one MPN per (class, board).
└──────────────────────┘
```

Pass 4 (monomorphization) already exists in
`bhdl-analyzer/src/passes/monomorphization.rs` and handles `<…>`
specialisation correctly. Passes 1–3 are partially implemented
(intent matcher exists for current_source / amplifier; designer
Rhai exists; expansion exists). Pass 5 is greenfield.

## 6. Examples end-to-end

### 6.1 Fixed-V LDO (specific, generic on V_OUT)

```bhdl
entity AP2112K<V_OUT: voltage>() {
    pin VIN:  power in;
    pin VOUT: power out;
    pin GND:  ground;
    pin EN:   signal in;

    attribute resolution   = "specific";
    attribute mpn_template = "AP2112K-{V_OUT_short}TRG";
    attribute package      = "SOT-25";
}

board MyBoard {
    LDO: AP2112K<3.3V>();
    @VBUS_5V -> LDO.VIN;
    LDO.VOUT -> @VCC_3V3;
    @EN_3V3  -> LDO.EN;
}

// After mono: AP2112K_3V3 (one specialised entity).
// After BOM:  AP2112K-3.3TRG (one MPN, derived from template).
```

### 6.2 Adjustable LDO (specific, runtime V_OUT, internal design Rhai)

```bhdl
entity LM317(v_out: voltage = 5V) {
    pin VIN:  power in;
    pin VOUT: power out virtual @requires(v_out: voltage);
    pin ADJ:  feedback in;
    pin GND:  ground;

    attribute resolution = "specific";
    attribute mpn        = "LM317T";

    design {
        const v_ref = 1.25;
        require v_out >= v_ref + 0.1
            else "LM317 minimum V_OUT is V_REF + headroom (~1.35V)";
        r2_value = 240;
        r1_value = 240 * (v_out / v_ref - 1.0);
    }

    expansion {
        VIN  -> C_in:  Capacitor<10µF,  20%, "X7R", "0603">.1; C_in.2  -> GND;
        VOUT -> C_out: Capacitor<22µF,  20%, "X7R", "0805">.1; C_out.2 -> GND;
        VOUT -> R1:    Resistor<r1_value, 1%, "0603">.1;       R1.2    -> ADJ;
        ADJ  -> R2:    Resistor<r2_value, 1%, "0603">.1;       R2.2    -> GND;
    }
}

board MyBoard {
    LDO: LM317(v_out: 5V);
    @VBUS_9V -> LDO.VIN;
    LDO.VOUT -> @VCC_5V;
}

// After designer: r1_value = 720, r2_value = 240.
// After expansion: C_in, C_out, R1, R2 materialised as children of LDO.
// After mono:
//   - LM317                            (specific, 1 MPN)
//   - Capacitor<10µF, 20%, X7R, 0603>  (class)
//   - Capacitor<22µF, 20%, X7R, 0805>  (class)
//   - Resistor<720Ω, 1%, 0603>         (class)
//   - Resistor<240Ω, 1%, 0603>         (class)
// After BOM: LM317T + 4 cost-function-picked MPNs.
```

### 6.3 Intent-driven (matcher picks the LDO)

```bhdl
board MyBoard {
    @VBUS_5V for voltage_source(v: 5V, i_max: 1A);    // upstream
    @VCC_3V3 for voltage_regulator(
        v_in:  5V,
        v_out: 3.3V,
        i_max: 500mA,
    );
}

// Matcher considers AP2112K, LM317, BuckRegulator, etc.
// Picks (say) AP2112K<3.3V> on the strength of dropout and quiescent.
// Pipeline proceeds as in §6.1.
```

## 7. Migration from the current stdlib

Today's stdlib has three coexisting shapes:

1. **Generic + runtime** — `BuckRegulator<V_OUT>(duty)`. Matches §2.
2. **Runtime-only** — `Resistor(value, tolerance)`. Wrong by §2.1:
   `value` and `tolerance` are part-defining and must become generics.
3. **Zero-arg with `part_no` string** — the accretion entities
   (`ATmega328P_DIP28(part_no: string = "ATMEGA328P-PU")`). These
   are already "specific" parts; `part_no` is the MPN. Migration is
   cosmetic: drop `part_no`, add `attribute resolution = "specific";
   attribute mpn = "ATMEGA328P-PU";`.

The migration unfolds in three commit-sized phases:

### Phase 1: passives become generic

Convert `Resistor(value, tolerance)` → `Resistor<R, TOL, PKG>()`,
update the four importer call sites that emit these, update the
KiCad importer to parse string values (`"10k"`) into typed
literals (`10kΩ`) before instantiation. **All round-trip tests
must stay green** — the netlist is unchanged; only the surface
syntax of the instance moves from `(…)` to `<…>`.

### Phase 2: ICs declare `resolution`

For every entity in `actives/`, `power/`, `connectors/`: add
`attribute resolution = "specific";` and either `mpn` or
`mpn_template`. Drop the `part_no: string` constructor parameter
where present (it duplicates `mpn`). No netlist changes.

### Phase 3: monomorphization key extends

The mono pass currently keys on generic tuple alone. Add `class`
entities to the key so the BOM pass can group instances by
class. No behavioural change for existing tests; sets up pass 5.

Pass 5 (cost-function BOM resolver) and `bom_preferences { }`
syntax are out of scope for this migration — they're separate
features that build on the resolution metadata once it's in place.

## 8. Open questions deferred to v0.2

- **Spec windows** — temperature range, voltage range, GBW, etc.
  These are part-specific specs the BOM cost function reads but
  the user can also *require* (`@VCC_3V3 for voltage_regulator(…)
  with v_in_max >= 12V`). v0.1 treats them as attributes; v0.2
  formalises them as constraint clauses.

- **Wildcard `bom_preferences` syntax** — beyond the headline
  shape shown here.

- **Multi-MPN runtime configuration** — parts where one runtime
  field also changes the BOM line (e.g., a programmable LDO
  whose `v_out` is fused at the factory, so you order a
  different MPN per V_OUT). These exist but are rare; deferred
  until a real example forces the issue.

- **Class equivalence rules** — when are two `Resistor<10kΩ, 1%,
  "0603">` invocations from different boards "the same class"?
  Per-board scoping in v0.1; cross-board class identity (for
  multi-board cost optimisation) is v0.2.

- **The cost function itself** — what makes one Yageo 10 kΩ
  better than a Panasonic 10 kΩ on a given board. This needs a
  proper part database, vendor APIs, and policy hooks; out of
  scope.

## 9. Decision log

- **`value` belongs in `<…>`, not `(…)`** — because changing a
  resistor's value means ordering a different part. (User
  observation: "physically, each unique combination is a
  different part anyway.")

- **No combined `Entity() for INTENT(…)` shape** — that pattern
  conflates "I picked this part, configure it" with "you pick
  the part, here's my requirement." Use `Entity(…)` for the
  first; use `@net for INTENT(…)` for the second.

- **`resolution = "class" | "specific"`** — a per-entity flag
  that tells the BOM pass who owns MPN selection. Passives are
  classes; most ICs are specific.

- **Cost-function pass is board-scoped** — one winning MPN per
  `(class, board)`, never per instance. Avoids loading two
  reels of equivalent parts on the same board.

- **`design { }` vs `design for INTENT { }`** — distinct forms.
  The first is plain runtime computation; the second is the
  entity-side of intent matching. An entity may have both.
