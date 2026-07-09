# BHDL Attribute System — Declaration, Values, and Typing

> Implementation-grounded (2026-07). This document describes attributes **as
> shipped**: the `attribute name = value;` declaration, the value forms it
> accepts, and how those values are typed. The canonical attribute
> *vocabulary* (which names mean what) and how attributes are *resolved and
> consumed* across the toolchain are in the companion
> [Unified_Attribute_System_Specification.md](Unified_Attribute_System_Specification.md);
> the language-level summary is in
> [BHDL_Complete_Specification.md](BHDL_Complete_Specification.md) §6.3.
>
> A *typed* attribute-declaration surface (declared attribute types,
> capabilities, validation rules) and a *behavioral* per-timestep surface were
> designed but are not implemented; that intent is preserved at the end of
> this file (§7) and in [Behavioral_Models.md](Behavioral_Models.md).

## 1. Declaration

An attribute is a `name = value` fact attached to an entity (or, less
commonly, a board or an instance):

```bhdl
entity Res(value: resistance, tolerance: percentage = 5%, wattage: power = 0.25W) {
    pin 1: signal inout;
    pin 2: signal inout;

    attribute component_class = "resistor";
    attribute resistance      = value;      // ← passes the constructor param through
    attribute tolerance       = tolerance;
    attribute power_rating    = wattage;
    attribute kicad_symbol    = "Device:R";
}
```

- Syntax: `attribute NAME = EXPR;`.
- `NAME` is a lowercase snake_case key. Attribute keys are an open namespace —
  any name is accepted — but the toolchain-meaningful names are a fixed,
  agreed vocabulary (see the companion doc).
- An entity may declare any number of attributes; a board-level or per-instance
  attribute overrides the entity default for that instance (see the companion
  doc's resolution section).

## 2. Value forms

An attribute value is any expression (main spec §5.4). The forms that appear
in practice:

| Form | Example | Notes |
|------|---------|-------|
| String | `"resistor"`, `"Device:R"` | Free text; the common case for identity/vocabulary attributes. |
| Quantity + unit | `5V`, `85%`, `44mΩ`, `150kHz` | Carries a *physical quantity* (§3). |
| Bare number | `32`, `100`, `0.35` | A dimensionless `float`/`int`. |
| Boolean | `true`, `false` | e.g. `attribute has_thermal_protection = true;`. |
| Parameter reference | `attribute resistance = value;` | The value of a constructor parameter (§4). |
| Computed expression | `attribute p_diss = (vin - vout) * i_out;` | Arithmetic over params/consts (§4). |
| Conditional (ternary) | see below | A value table keyed on a parameter. |
| Display string | `"VOUT = {VREF}*(1 + {R1}/{R2})"` | An equation string rendered by the schematic layer. |

The chained ternary is the idiom for a small value table — here a footprint
selected by a `style` parameter (`bhdl-stdlib/connectors/testpoint.bhdl`):

```bhdl
attribute footprint =
    style == "pad"  ? "TestPoint:TestPoint_Pad_D1.5mm" :
    style == "hole" ? "TestPoint:TestPoint_THTPad_D1.5mm_Drill0.7mm" :
    style == "loop" ? "TestPoint:TestPoint_Loop_D2.5mm" :
    "TestPoint:TestPoint_Pad_D1.5mm";
```

When the keyed parameter is a closed enum, pair the ternary with a
`where <param> in (...)` value domain so an unlisted key is rejected rather
than silently defaulting to the fallback arm (main spec §7.2).

## 3. Typing

An attribute value's type is inferred from the literal — there is no separate
attribute-type declaration today. The inference:

- A number with a **unit** carries the corresponding physical quantity
  (`5V` → voltage, `10mA` → current, `44mΩ` → resistance, `150kHz` →
  frequency, `85%` → percentage). The unit grammar and quantity set are in
  main spec §5.
- A number **without** a unit is a `float` (or `int` where whole).
- A quoted literal is a `string`; `true`/`false` is a `bool`.

Because a quantity carries its dimension, a consumer that expects a voltage and
finds a current-typed attribute can detect the mismatch; a consumer reading a
plain string gets no such guarantee. This is why the canonical electrical
facts (`output_voltage`, `feedback_voltage`, `rds_on`, …) are declared with
units, not as bare strings.

## 4. Evaluation

Attributes are **static**: evaluated once, at synthesis time, and not
re-evaluated during simulation. Three cases:

- **Literal** attributes need no evaluation.
- **Parameter-reference** attributes (`attribute resistance = value;`) resolve
  to the constructor argument supplied at the instantiation site. This is what
  lets an abstract entity (`Res`) forward its `value` argument onto the
  instance so the part chooser and the schematic can read it.
- **Computed** attributes (arithmetic, ternary, references to other params or
  `const`s) are evaluated from the instance's parameters. The analyzer
  topologically orders attribute dependencies so one attribute may reference
  another; a dependency cycle is a hard error.

Values computed inside `design {}` / `simulation { stress }` blocks read
parameters via `self.<param>`; that these are readable depends on the
synthesizer stamping constructor arguments (and entity-parameter defaults)
onto the instance before those blocks run.

## 5. Names: open namespace, canonical constants

Attribute *keys* are free strings, so a board or a new part can attach any
attribute. But every attribute the toolchain acts on has a **canonical name**
defined as a Rust constant in `bhdl-common::sku::attr` (identity/manufacturing)
or read by name in a specific pass (electrical, model, provenance). Producers
and consumers reach for the same constant, so a *misspelled canonical
attribute* fails to line up at the consumer boundary rather than silently
disappearing — for example `physical_package` is the canonical key (spelled
thus because `package` is a reserved word), and a part that writes `package`
instead is simply not seen by the BOM walker.

The full vocabulary — SKU identity, electrical, `component_class`, the
`spice_*` model surface, `intent_*`, and the toolchain-stamped provenance and
control attributes — is catalogued in the companion doc.

## 6. What attributes are for

An attribute carries a fact a *consumer* needs. The consumers (all read-only):
the BOM/SKU walker, the part chooser, ERC (rule applicability and thresholds),
the DC/transient model builder (`spice_*`), sign-off, the schematic renderer
(symbol, refdes, display equations), refdes allocation, and PnR/KiCad export.
Which consumer reads which attribute is the subject of the companion doc.

Because attributes feed analysis, the Real-Data Policy applies: a consumer that
needs an attribute it cannot find reports UNCHECKED (or, where the datum is
mandatory, errors) — it never fabricates the missing value. See
[Real_Data_Policy.md](Real_Data_Policy.md).

## 7. Not yet implemented (design intent)

The following were specified but are **not** built; they are recorded so the
intent isn't lost, and must not be read as current syntax:

- **Declared attribute types** — `attribute_type` definitions with units,
  ranges, enums/structs, and `warn_if`/`error_if` validation, so an attribute
  is checked against a library-defined schema rather than only inferred from
  its literal.
- **Capabilities** — `capability { required_attributes; … }` built from typed
  attributes, for capability-based part matching.
- **Behavioral attributes** — expression attributes recomputed each simulation
  timestep and `when (condition) { … }` blocks that mutate state; that dynamic
  surface belongs to system-level simulation and is tracked in
  [Behavioral_Models.md](Behavioral_Models.md).

Today's attributes are static `name = value` bindings, typed by their literal,
evaluated once at synthesis.
