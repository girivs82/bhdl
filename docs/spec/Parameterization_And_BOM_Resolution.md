# Parameterization and BOM Resolution

> **Status:** Proposal v0.2. Scope: the three-form argument model
> (generic / runtime / intent), the three-layer **entity / class /
> part** model with first-class `part_family` catalog declarations,
> and the user-supplied **plugin protocol** that owns stocking and
> selection.
>
> Changes from v0.1 (committed as `9ee7216`):
> - **Three-layer model** (entity / class / part) supersedes the
>   per-entity `resolution = "class" | "specific"` flag. The flag is
>   removed; resolution kind falls out structurally from how many
>   `part_family` declarations populate a class.
> - **`mpn_template`** moves off entities and onto `part_family`
>   declarations.
> - **Stocking is out of BHDL.** A user-supplied plugin (JSON stdin/
>   stdout) owns stock, price, lead time, and vendor SKU lookup.
>   BHDL emits "what's possible"; the plugin returns "what to buy."
> - New **Pass 4.5 (catalog scan)** sits between expansion and BOM.

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

But the language must also acknowledge a second reality: passives
exist in tens of thousands of MPNs that are *vendor-substitutable for
the same design intent*. A board specifying "10 kΩ 1 % 0603" should
pick *one* MPN per board (so manufacturing loads one reel), but
*which* MPN — Yageo, Panasonic, AVX, an in-house preferred-vendor
part — depends on the user's purchasing context, not on the design.

This document defines:

1. **Three argument forms** (§2), one per scenario, with no overloading.
2. **The entity / class / part model** (§3) — a parametric entity
   generates classes; classes are populated by zero or more
   `part_family` declarations; each `part_family` is a parametric
   generator of parts (orderable MPNs).
3. **`part_family` declarations** (§4) — catalog grammar, semantics,
   value-range constraints (`require R in E96(…)`).
4. **Virtual pins and `design` blocks** (§5) — how computed values
   inside an entity feed the generics of its internal children.
5. **The synthesis pipeline** (§6) — five passes that take a
   parametric board to a candidate-MPN list ready for the plugin.
6. **The plugin protocol** (§7) — JSON stdin/stdout interface that
   the user supplies for stocking, selection, and vendor SKU
   mapping. BHDL ships a default deterministic plugin.

The model deliberately mirrors how an electrical engineer thinks:
*here's the part I picked, configure it*; *here's a requirement, you
pick the part*; *here's an equivalence class, the purchasing layer
picks the SKU*. And the purchasing layer is **not** BHDL.

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
}
```

- Every distinct generic tuple monomorphises to a distinct class:
  `Resistor<10kΩ, 1%, "0603">` and `Resistor<10kΩ, 5%, "0603">` are
  *different classes* post-mono, with different mangled names.
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

- LM317T is one MPN regardless of `v_out`. `v_out` is therefore
  **not** generic. It's a runtime parameter.
- The `design { }` block runs the Rhai body to derive `r1_value`
  and `r2_value`. Those flow into the **generics** of the internal
  `Resistor<…>` instances. The mono pass then specialises those.
- A runtime parameter's value **must be resolvable at synthesis
  time** — it's a compile-time constant in the BHDL sense, not a
  runtime register. The name "runtime" here distinguishes it from
  "generic", which is part of the *type*.

### 2.3 Intent attachment — `for INTENT(…)`

Used when the **implementation is not chosen yet**. The user declares
a *requirement* against a net and a late binding pass picks an
entity from the candidate set whose `design for INTENT { }` block
can fulfil it.

```bhdl
// User: "I need a 3.3V rail somehow."
@VCC_3V3 for voltage_regulator(v_in: 5V, v_out: 3.3V, i_max: 500mA);
```

The intent matcher considers all entities with `design for
voltage_regulator { }`, picks one by policy (efficiency, cost,
board area), and emits the picked entity bound to the net.

**Critical distinction**: once the user writes `LM317(v_out: 5V)`,
they have *already chosen* the part. The `for voltage_regulator(…)`
form is **wrong** for that case — it would be expressing a
requirement against an entity that has already committed to an
implementation. Use `(…)` for "I picked it, configure it"; use
`for INTENT(…)` for "you pick it."

### 2.4 Summary

| Form | When | Resolves at | Example |
|---|---|---|---|
| `Entity<T1, T2, …>` | Part-defining axes | Monomorphization | `AP2112K<3.3V>()` |
| `Entity(p1, p2, …)` | Same-part configuration | Designer Rhai, then mono on internals | `LM317(v_out: 5V)` |
| `@net for INTENT(…)` | Abstract requirement; impl not chosen | Intent matcher → designer → mono | `@VCC for voltage_regulator(...)` |

## 3. Entity, class, part — the three-layer model

After monomorphisation, every parametric instance lands somewhere in
a three-layer hierarchy:

| Layer | What it is | Source | Example |
|---|---|---|---|
| **Entity** | Parametric template, no bound generics | BHDL `entity` decl | `Resistor<R, TOL, PKG>` |
| **Class** | Bound-generic identity — the equivalence class a design names | Synthesised from entity + bound generics | `Resistor<10kΩ, 1%, "0603">` |
| **Part family** | Parametric *generator* of MPNs within one or more classes | BHDL `part_family` decl | `Yageo_RC0603FR_07` |
| **Part (MPN)** | Concrete orderable item — what appears on the BOM | Expanded from `part_family` at catalog-scan time | `RC0603FR-0710KL` |

### 3.1 Where each layer lives

- **Entities** live in `bhdl-stdlib/<category>/<entity>.bhdl`. They
  describe what a kind of part *is*: its pin set, its electrical
  attributes, its parametric axes. They carry **no MPN information**.

- **Classes** are not source-level. They emerge as a side-effect of
  monomorphising entity instances against their generics. The mono
  pass's mangled name *is* the class identity.

- **Part families** live in `bhdl-stdlib/parts/<manufacturer>/`
  (or in a project's own `parts/` for user-specific catalogs). They
  declare which classes a manufacturer's product family populates,
  and how to derive concrete MPNs from class generics.

- **Parts** are not source-level. They are produced at catalog-scan
  time by expanding `part_family` templates against the concrete
  generic tuples found in the design. Their identity is the MPN
  string.

### 3.2 Why the layering replaces the `resolution` flag

v0.1 carried `attribute resolution = "class" | "specific"` per
entity. v0.2 drops it. The distinction *still exists* but is now
**structural** rather than declarative:

| Catalog-scan finds… | What it means | What was called this in v0.1 |
|---|---|---|
| Multiple `part_family` matches | Real equivalence class (e.g. a 10k 1% 0603) | `"class"` |
| One `part_family` match, parametric template | Mechanically-named part family (e.g. AP2112K-V variants) | `"specific"` (template form) |
| One `part_family` match, literal MPN | Family of one (e.g. LM317T) | `"specific"` (literal form) |
| Zero matches | Catalog gap; BOM pass errors with the class identity | n/a (was a compile-time error in v0.1 too) |

The synthesizer no longer has to look at a flag on the entity — it
counts the matches. Adding a new manufacturer's resistor line later
*automatically* turns more catalog rows into candidates for the
existing 10kΩ 1% 0603 class, without touching the `Resistor` entity.

### 3.3 Why this scales

The original concern that drove the v0.1 part-DB sketch was: there
are tens of thousands of resistor MPNs in the wild; you can't have
a `.bhdl` per MPN.

The three-layer model resolves this. The thousands collapse into:

- **One entity** per axis schema (`Resistor<R, TOL, PKG>`) — a few
  files in stdlib.
- **A few hundred classes per design** (every distinct generic
  tuple the design uses) — synthesised, not source.
- **A few hundred `part_family` declarations per stdlib** —
  manageable to write by hand or generate from manufacturer CSVs.
- **Concrete parts** emerge at catalog-scan time from family
  expansion; no per-MPN source files.

The N×M explosion of "every MPN in the world" collapses to N
entities + M part families, where N and M are both ~hundreds.

## 4. `part_family` declarations

A `part_family` is a parametric generator of MPNs that populates one
or more classes. Its job is to bridge between BHDL's type-level part
identity (class) and the manufacturer's orderable identity (MPN).

### 4.1 Grammar

```ebnf
part_family_decl :=
    "part_family" IDENT class_pattern "{"
        constraints
        attributes
    "}"

class_pattern :=
    ":" entity_name "<" generic_args ">"     // e.g. ": Resistor<*, '1%', '0603'>"

generic_args :=
    generic_arg ("," generic_arg)*

generic_arg :=
    IDENT ":" "*"          // unbound (wildcard) — narrowed by `require`
  | IDENT ":" const_value  // bound to a specific value
  | const_value            // positional shorthand
  | "*"                    // positional wildcard

constraints :=
    ("require" expr ("else" string_literal)? ";")*

attributes :=
    ("attribute" IDENT "=" expr ";")*
```

`*` in the `class_pattern` means "any value of this generic." A
`require` clause narrows it.

### 4.2 Examples

**Yageo's 1% 0603 thick-film resistors** — populates many resistor
classes (every E96 value × 7 decades):

```bhdl
part_family Yageo_RC0603FR_07 : Resistor<R: *, "1%", "0603"> {
    require R in E96(1Ω, 10MΩ);

    attribute manufacturer  = "Yageo";
    attribute mpn_template  = "RC0603FR-07{e96_code(R)}L";
    attribute datasheet     = "https://www.yageo.com/.../RC_thick_film.pdf";
    attribute tc_ppm        = 100;
    attribute power_w       = 0.1;
}
```

**Diodes Inc's AP2112K family** — fixed-V LDOs in SOT-25, one MPN
per voltage variant:

```bhdl
part_family Diodes_AP2112K : AP2112K<V_OUT: *> {
    require V_OUT in { 1.5V, 1.8V, 2.5V, 2.6V, 2.7V, 2.8V, 3.0V, 3.3V, 5.0V };

    attribute manufacturer  = "Diodes Incorporated";
    attribute mpn_template  = "AP2112K-{v_short(V_OUT)}TRG";
    attribute datasheet     = "https://www.diodes.com/.../AP2112.pdf";
    attribute package_real  = "SOT-25";
}
```

**LM317T** — adjustable LDO, family of one with a literal MPN:

```bhdl
part_family TI_LM317T : LM317 {
    attribute manufacturer = "Texas Instruments";
    attribute mpn          = "LM317T";   // literal; no template
    attribute datasheet    = "https://...";
    attribute package_real = "TO-220";
}
```

**Murata's GRM21 X7R 0805 cap line** — populates capacitor classes
across capacitance × voltage:

```bhdl
part_family Murata_GRM21BR71 : Capacitor<C: *, V: *, "X7R", "0805"> {
    require V in { 10V, 16V, 25V, 50V };
    require C in E12(1nF, 10µF);

    attribute manufacturer  = "Murata";
    attribute mpn_template  = "GRM21BR71{v_code(V)}{c_code(C)}KA01L";
    attribute datasheet     = "https://...";
    attribute dielectric    = "X7R";
}
```

### 4.3 Value-range constraints

The `require` clause supports common forms used in catalogs:

- **E-series membership** — `R in E12(...)`, `R in E24(...)`,
  `R in E48(...)`, `R in E96(...)`, `R in E192(...)`. Standard EIA
  values; the synthesizer knows the tables.
- **Range** — `R >= 1Ω`, `R <= 10MΩ`, `R in (1Ω..10MΩ)`.
- **Enumeration** — `V_OUT in { 1.5V, 1.8V, 2.5V, … }`. Discrete set.
- **Composite** — boolean combinations: `require (R in E96 && R in (1Ω..10MΩ)) || R == 0Ω;` (jumper-resistor exception).

If a `require` fails for a particular class's generic tuple, that
family does **not** match — the catalog scan moves on. No error is
raised; the absence of a match is just the absence of a candidate.
An error only emerges if *no* family matches the class at all.

### 4.4 The `mpn_template` mini-language

`mpn_template` is a string-interpolation template. Interpolation
slots take the form `{expr}` where `expr` is a small language of
named functions over generic-parameter values:

| Function | Purpose | Example |
|---|---|---|
| `e96_code(R)` | EIA E96 3-digit value-code (e.g. 1002 for 10k) | `{e96_code(R)}` → `"1002"` |
| `e24_code(R)` | EIA E24 2-digit code | |
| `v_short(V)` | Voltage as "3.3", "5.0", etc. (no trailing zero rules per fn) | `{v_short(V_OUT)}` → `"3.3"` |
| `v_code(V)` | Manufacturer-specific voltage codes (lookup table) | `{v_code(V)}` → `"1H"` for 50V Murata |
| `c_code(C)` | Capacitance codes per EIA-198 (3-digit + multiplier) | `{c_code(100nF)}` → `"104"` |
| Identity | Raw value as default-formatted | `{V}` → `"3.3V"` |

The function set is fixed in v0.2. v0.3 will allow user-defined
helper functions in BHDL's Rhai scope so manufacturer-specific
codings can be expressed without a synthesizer change.

### 4.5 Multiple families per class is the normal case

A class like `Resistor<10kΩ, 1%, "0603">` will typically be matched
by 5–20 part families (Yageo, Panasonic, AVX, Vishay, Bourns, …).
Catalog scan returns *all* matches as candidates; the plugin picks
one. The user's plugin is where company-specific preferences,
stock, and pricing enter the decision.

### 4.6 User-extensible catalogs

`part_family` is a top-level construct that can appear in user
projects, not just stdlib. A company that buys bulk-priced Bourns
reels can ship its own catalog:

```bhdl
// In acme-corp/parts/bourns_avl.bhdl — Acme Corp's approved Bourns
// resistor line (negotiated reel pricing).
part_family Acme_Bourns_CR0603 : Resistor<R: *, "1%", "0603"> {
    require R in E96(10Ω, 1MΩ);
    attribute manufacturer  = "Bourns";
    attribute mpn_template  = "CR0603-FX-{e96_code(R)}ELF";
    attribute avl_status    = "preferred";   // hint to the plugin
    attribute internal_pn   = "Acme:RES-{e96_code(R)}-0603-1P";
}
```

The user's plugin reads the `avl_status = "preferred"` attribute
and prioritises this family over the stdlib's generic Yageo/Panasonic
matches. The attribute is opaque to the synthesizer — it's plugin
metadata that rides along in the candidate list.

## 5. Virtual pins and `design` blocks

### 5.1 Virtual pins declare an expansion contract

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

### 5.2 Two flavours of `design` block

| Form | Purpose | When the body runs |
|---|---|---|
| `design { … }` | Plain runtime computation (scenario A) | Once per instance, after constructor binding |
| `design for INTENT { … }` | Intent matcher (scenario B) | Once per intent attachment that picks this entity |

Both flavours have the same Rhai surface. The difference is the
input namespace:

- `design { }` reads from `self.*` (runtime params + generics).
- `design for INTENT { }` reads from `self.*` **and** `intent.*`
  (the requirement that matched this candidate).

An entity may have both.

### 5.3 How values flow into expansion

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

## 6. The synthesis pipeline

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
│                      │  entity, emit one specialised class per
│                      │  distinct generic tuple. Deterministic
│                      │  mangled names.
└──────────────────────┘
            │
            ▼
┌──────────────────────┐
│  4.5 Catalog scan    │  for each class produced by pass 4, walk all
│       (NEW in v0.2)  │  `part_family` declarations and find those
│                      │  whose class_pattern matches and whose
│                      │  `require` clauses are satisfied. For each
│                      │  match, expand `mpn_template` against the
│                      │  class's generic values to produce candidate
│                      │  MPNs. Bundle: { class, instances, candidates }.
└──────────────────────┘
            │
            ▼
┌──────────────────────┐
│  5. Plugin BOM       │  serialise the candidate-bundle list to JSON,
│       resolution     │  invoke the user's plugin via stdin/stdout,
│   (REWRITTEN v0.2)   │  parse the plugin's selections, write the
│                      │  final BOM with vendor SKUs.
└──────────────────────┘
```

Passes 1–4 already exist in `bhdl-analyzer` (intent matcher and
designer for `current_source` / `amplifier`; full mono pass at
`bhdl-analyzer/src/passes/monomorphization.rs`). Pass 4.5 and the
revised Pass 5 are greenfield.

Pass 4.5 is **pure deterministic source-language traversal** — no
I/O, no network, no plugin involvement. It produces a JSON document
that can be persisted, diffed, or reviewed independently of the
plugin step. This separation matters: catalog scan results are
reproducible and source-controllable; only the plugin step is
context-dependent.

## 7. The plugin protocol

### 7.1 Why plugin

Stocking is a user-process concern, not a language concern:

- **Hobbyist** — Digi-Key / Mouser personal accounts, cost-optimised.
- **Company** — approved-vendor list, internal SAP stock, contract
  pricing.
- **Contract manufacturer** — their own preferred-parts library.
- **Air-gapped** — local CSV of pre-approved parts, no network.

Baking any of these into BHDL would lock out the others. The plugin
boundary makes BHDL portable; the plugin owns the messy real-world
data integration.

### 7.2 The contract

A plugin is **any executable** that:

1. Reads a JSON document from stdin (the candidate bundle).
2. Writes a JSON document to stdout (the selections).
3. Exits with status 0 on success, non-zero on error.

Stderr is free for the plugin's logging; BHDL surfaces it on
failure but otherwise passes it through to the user's terminal.

This is language-agnostic — plugins can be Bash scripts, Python
programs, compiled Rust binaries, anything that can read stdin and
write stdout.

### 7.3 Input schema (BHDL → plugin)

```json
{
  "bhdl_version": "0.2",
  "protocol_version": "1",
  "board": "MyBoard",
  "policy_hints": {
    "currency": "USD",
    "manufacturing_location": "US"
  },
  "selections_needed": [
    {
      "class": "Resistor",
      "generics": {
        "R": "10kΩ",
        "TOL": "1%",
        "PKG": "0603"
      },
      "instance_count": 4,
      "instances": ["R1", "R5", "R12", "R23"],
      "candidates": [
        {
          "family": "Yageo_RC0603FR_07",
          "mpn": "RC0603FR-0710KL",
          "manufacturer": "Yageo",
          "attributes": {
            "datasheet": "https://...",
            "tc_ppm": 100,
            "power_w": 0.1
          }
        },
        {
          "family": "Panasonic_ERJ_3EK",
          "mpn": "ERJ-3EKF1002V",
          "manufacturer": "Panasonic",
          "attributes": { "datasheet": "https://...", "tc_ppm": 100, "power_w": 0.1 }
        },
        {
          "family": "Acme_Bourns_CR0603",
          "mpn": "CR0603-FX-1002ELF",
          "manufacturer": "Bourns",
          "attributes": { "avl_status": "preferred", "internal_pn": "Acme:RES-1002-0603-1P" }
        }
      ]
    }
  ]
}
```

`policy_hints` carries user-supplied policy that BHDL doesn't
interpret but the plugin may want (preferred currency, manufacturing
location for region-restricted parts, etc.). Per-board hints live in
the board declaration:

```bhdl
board MyBoard {
    bom_policy {
        currency = "USD";
        manufacturing_location = "US";
    }
    // … instances …
}
```

### 7.4 Output schema (plugin → BHDL)

```json
{
  "protocol_version": "1",
  "selections": [
    {
      "class_index": 0,
      "mpn": "RC0603FR-0710KL",
      "manufacturer": "Yageo",
      "vendor": "Digi-Key",
      "vendor_sku": "311-10.0KHRCT-ND",
      "qty": 4,
      "unit_price": 0.0042,
      "currency": "USD",
      "stock": 4329,
      "lead_time_weeks": 0,
      "note": "lowest in-stock matching class"
    }
  ],
  "warnings": [
    "C5 (Capacitor<470µF, 20%, electrolytic, '8x10mm'>): only one candidate; consider second-sourcing"
  ]
}
```

- `class_index` references `selections_needed[i]` in the input.
- All fields after `mpn` and `manufacturer` are optional. A minimal
  plugin (the default) returns just MPN + manufacturer; richer
  plugins fill in vendor SKU, stock, pricing.
- `warnings[]` is plugin-emitted free-form text; BHDL prints it
  alongside the BOM.

### 7.5 Errors

A plugin returns an error per-class by replacing the selection object
with an error object:

```json
{
  "selections": [
    {
      "class_index": 1,
      "error": "no_stock",
      "message": "All candidate MPNs for Capacitor<470µF, 20%, electrolytic, '8x10mm'> are out of stock"
    }
  ]
}
```

BHDL surfaces these as `BomError::PluginRejected { class, message }`
diagnostics referencing the affected refdes(es). The user can:

- Adjust the design (pick a different value, look up alternatives).
- Add a `bom_preferences { pin … }` override (§7.7) that bypasses
  the plugin for the affected class.
- Retry later (if `no_stock` is transient).

If the plugin exits non-zero before returning JSON, BHDL treats it
as a fatal error with the captured stderr.

### 7.6 Selecting the plugin

Per-project, in `bhdl.toml`:

```toml
[bom]
plugin = "bhdl-plugin-digikey"     # or a path: "./tools/select_parts.py"
plugin_args = ["--prefer-jit", "--max-lead-weeks", "4"]
```

Per-board overrides:

```bhdl
board MyBoard {
    bom_policy {
        plugin = "./scripts/avl_select.py";
    }
    // … instances …
}
```

If no plugin is configured, BHDL invokes the **default plugin**.

### 7.7 The default plugin

Ships with BHDL. Deterministic, no network access:

1. For each class, pick the first candidate alphabetically by
   `(manufacturer, mpn)`.
2. Set `qty` from `instance_count`.
3. Leave `vendor`, `vendor_sku`, `unit_price`, `stock`,
   `lead_time_weeks` unset.

Out-of-the-box, every BHDL project produces a valid BOM (MPNs only,
no sourcing info). Serious users plug in their own.

### 7.8 Board-level overrides

Sometimes the user wants to *bypass* the plugin for a specific class:

```bhdl
board MyBoard {
    bom_preferences {
        // Hard pin: ignore the plugin's candidates, use this MPN.
        Resistor<10kΩ, 1%, "0603">  pin "Yageo:RC0603FR-0710KL";

        // Soft hint: passes through to the plugin as a policy hint.
        Resistor<*, *, "0603">      prefer_vendor "Panasonic";
    }
    // … instances …
}
```

- `pin` is enforced *before* the plugin sees the candidates: the
  candidate list is filtered to just the pinned MPN. If that MPN
  isn't in the candidate list (typo, wrong package), BHDL fails
  with `BomError::PinMismatch`.
- `prefer_vendor` and similar soft hints are forwarded to the
  plugin as additional `policy_hints` fields. The plugin may or
  may not honour them.

### 7.9 Plugin protocol versioning

`protocol_version` is in both directions of the JSON. BHDL emits the
protocol version it produces; plugins emit the version they
respond with. Mismatch is a hard error with a clear message ("plugin
speaks protocol v2; this BHDL emits protocol v1").

v0.2 freezes protocol v1. Future BHDL versions may emit additional
fields; plugins that don't understand them must ignore unknown
fields gracefully (the spec mandates this).

## 8. Examples end-to-end

### 8.1 Fixed-V LDO

```bhdl
entity AP2112K<V_OUT: voltage>() {
    pin VIN:  power in;
    pin VOUT: power out;
    pin GND:  ground;
    pin EN:   signal in;
    attribute package = "SOT-25";
}

// In bhdl-stdlib/parts/diodes/ap2112k.bhdl:
part_family Diodes_AP2112K : AP2112K<V_OUT: *> {
    require V_OUT in { 1.5V, 1.8V, 2.5V, 3.3V, 5.0V };
    attribute manufacturer = "Diodes Incorporated";
    attribute mpn_template = "AP2112K-{v_short(V_OUT)}TRG";
}

board MyBoard {
    LDO: AP2112K<3.3V>();
    @VBUS_5V -> LDO.VIN;
    LDO.VOUT -> @VCC_3V3;
    @EN_3V3  -> LDO.EN;
}

// After mono:        class AP2112K<3.3V> with 1 instance (LDO).
// After catalog:     candidates = [{ mpn: "AP2112K-3.3TRG", manufacturer: "Diodes Inc" }]
// After plugin:      selection = { mpn: "AP2112K-3.3TRG", vendor_sku: "AP2112K-3.3TRG-DICT-ND", … }
```

### 8.2 Adjustable LDO

```bhdl
entity LM317(v_out: voltage = 5V) {
    pin VIN:  power in;
    pin VOUT: power out virtual @requires(v_out: voltage);
    pin ADJ:  feedback in;
    pin GND:  ground;

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

// In bhdl-stdlib/parts/ti/lm317.bhdl:
part_family TI_LM317T : LM317 {
    attribute manufacturer = "Texas Instruments";
    attribute mpn          = "LM317T";
    attribute package_real = "TO-220";
}

board MyBoard {
    LDO: LM317(v_out: 5V);
    @VBUS_9V -> LDO.VIN;
    LDO.VOUT -> @VCC_5V;
}

// After designer: r1_value = 720, r2_value = 240.
// After expansion: C_in, C_out, R1, R2 materialised as children of LDO.
// After mono: 5 classes — LM317, Capacitor<10µF,20%,X7R,0603>,
//   Capacitor<22µF,20%,X7R,0805>, Resistor<720Ω,1%,0603>,
//   Resistor<240Ω,1%,0603>.
// After catalog: each class collects candidate part_families.
// After plugin: 5 MPNs selected (one per class) with the user's plugin policy.
```

### 8.3 Intent-driven

```bhdl
board MyBoard {
    @VBUS_5V for voltage_source(v: 5V, i_max: 1A);
    @VCC_3V3 for voltage_regulator(
        v_in:  5V,
        v_out: 3.3V,
        i_max: 500mA,
    );
}

// Intent matcher considers all entities with `design for voltage_regulator`.
// Picks (say) AP2112K<3.3V> on the strength of dropout and quiescent.
// Pipeline proceeds as in §8.1.
```

## 9. Migration from the current stdlib

Today's stdlib has three coexisting shapes:

1. **Generic + runtime** — `BuckRegulator<V_OUT>(duty)`. Matches §2.
2. **Runtime-only** — `Resistor(value, tolerance)`. Wrong by §2.1:
   `value` and `tolerance` are part-defining and must become generics.
3. **Zero-arg with `part_no` string** — the accretion entities
   (`ATmega328P_DIP28(part_no: string = "ATMEGA328P-PU")`). These
   carry MPN info as a constructor string; under v0.2 the MPN moves
   to a `part_family` declaration.

Five commit-sized phases:

### Phase 1: passives become generic

Convert `Resistor(value, tolerance)` → `Resistor<R, TOL, PKG>()`,
update importer call sites, parse KiCad value strings (`"10k"`) into
typed literals (`10kΩ`) before instantiation. **All round-trip tests
must stay green** — the netlist is unchanged; only the surface
syntax of the instance moves from `(…)` to `<…>`.

### Phase 2: introduce `part_family` grammar

Add the `part_family` keyword to the parser, the analyser's symbol
table, and the AST. Stub out catalog scan as a no-op (returns the
entity's existing `mpn` attribute as the single candidate, for
back-compat). Tests still pass.

### Phase 3: catalog seed

Write the first ~30 `part_family` declarations covering the most
common passives (3–5 manufacturers × 3 tolerance × 3 packages for
resistors, similar for caps) and every Arduino-board IC. Move the
existing `part_no` constructor parameters from accretion entities
into the corresponding `part_family` decls.

### Phase 4: catalog scan pass

Implement Pass 4.5 — walk `part_family` decls, match against classes,
expand templates. Emits the JSON candidate bundle. No plugin yet;
the catalog-scan output is just printed.

### Phase 5: plugin invocation

Implement Pass 5 — spawn the plugin process, pipe JSON in, parse
JSON out. Ship the default plugin. Document the protocol.

After Phase 5, the BOM is auto-derived from the design. Round-trip
tests continue to gate every step.

## 10. Open questions deferred to v0.3+

- **Spec windows** — temperature range, voltage range, GBW. These
  are part attributes the plugin reads, but the user can also
  *require* them at design time (`@VCC for voltage_regulator(…) with
  v_in_max >= 12V`). v0.2 treats them as attributes; v0.3 formalises
  them as constraint clauses.

- **Wildcard syntax in `bom_preferences`.** v0.2 ships the headline
  `Resistor<*, *, "0603"> prefer_vendor "Panasonic"` shape; the full
  pattern algebra (negation, conjunction across attributes, etc.) is
  v0.3.

- **Multi-MPN runtime configuration** — parts where a runtime field
  also changes the BOM line (factory-fused programmable LDO whose
  `v_out` is set at the factory, so you order a different MPN per
  V_OUT). Rare; deferred until a real example forces the issue.

- **Cross-board class identity.** When are two `Resistor<10kΩ, 1%,
  "0603">` invocations from *different boards* the same class for
  multi-board cost optimisation? Per-board scoping in v0.2; v0.3
  allows opt-in cross-board class merging.

- **User-defined `mpn_template` helper functions.** v0.2 fixes the
  function set (`e96_code`, `v_short`, etc.). v0.3 allows Rhai-defined
  helpers in stdlib so manufacturer-specific codings can be added
  without a synthesizer change.

- **Plugin protocol stability.** v0.2 freezes protocol v1.
  Backwards-compatible field additions are allowed (plugins must
  ignore unknown fields); breaking changes require bumping to v2
  with a transition window where BHDL supports both.

- **Catalog discontinuities and value exclusions.** Some real
  families have gaps (discontinued values, factory-restricted SKUs).
  v0.2 catalog rows declare a value range; v0.3 adds
  `excluded_values = { … }` per family.

## 11. Decision log

- **`value` belongs in `<…>`, not `(…)`** — because changing a
  resistor's value means ordering a different part. (User
  observation: "physically, each unique combination is a
  different part anyway.")

- **No combined `Entity() for INTENT(…)` shape** — that pattern
  conflates "I picked this part, configure it" with "you pick
  the part, here's my requirement." Use `Entity(…)` for the
  first; use `@net for INTENT(…)` for the second.

- **Three-layer model: entity / class / part** — supersedes v0.1's
  `resolution = "class" | "specific"` per-entity flag. Resolution
  kind is now structural (count of matching `part_family`
  declarations). Adding a new manufacturer's catalog row
  automatically expands candidates without touching the entity.
  (User observation: "given our classification — a resistor class
  that can hold multiple types/parts — maybe we rethink the DB.")

- **`part_family` is BHDL, not external data.** Catalog rows are
  source-controlled, parser-checked, type-checked alongside
  entities. Manufacturer-CSV importers may *generate* `part_family`
  decls but the canonical form is BHDL.

- **Stocking is a user-supplied plugin, not part of BHDL.**
  (User observation: "the stocking DB should not be part of BHDL,
  but the user process… should be an extension/plugin that the
  user supplies, so that the final part selection works the way
  the user wants.") Plugin contract: JSON over stdin/stdout, exit
  status. Language-agnostic. Default plugin ships with BHDL for
  zero-config operation.

- **Cost-function pass is board-scoped** — one winning MPN per
  `(class, board)`, never per instance. Avoids loading two reels
  of equivalent parts on the same board.

- **`design { }` vs `design for INTENT { }`** — distinct forms.
  The first is plain runtime computation; the second is the
  entity-side of intent matching. An entity may have both.

- **`mpn_template` lives on `part_family`, not on entities.**
  Entities are pure type definitions with no MPN information.
  Moving MPN derivation to `part_family` declarations is what
  makes the three-layer model work — multiple manufacturers can
  ship into the same class without entity-level coordination.
