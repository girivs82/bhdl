# Board SKU Variants

> **Status:** Proposal v0.1. Scope: **DNP + value override**. Whole-
> module gating, MPN override, footprint variants, and cross-variant
> constraints are explicit non-goals for v0.1; see §4.

## 1. Motivation

A real product is one PCB → *many* shipping SKUs. Each SKU is the
same PCB layout with a different population list and possibly
different reel choices for some footprints. Examples:

- Basic / Pro tiers: same hardware, different value-set for a few
  resistors (loop bandwidth, gain) and capacitors (timing).
- Regional variants: EU / US / JP — different antenna front-end,
  different regulatory parts, sometimes whole RF sections omitted.
- Dev kit vs production: extra debug headers and a JTAG connector
  populated only in dev.
- Yield/cost variants: optional protection parts loaded only for
  customers who paid for the surge-rated SKU.

EDA precedent: KiCad has assembly variants, Altium has "Variant
Management," Cadence calls them "Options." JLCPCB / MacroFab accept
per-variant pick-place files.

BHDL today has no first-class concept. A board produces exactly one
BOM. This proposal adds **variant blocks**, a CLI selector, and
per-variant BOM/synthesis output — enough to express the DNP-and-
value variation that covers ~80 % of real product-management work.

The wider story (whole-module gating, etc.) is sketched in §4 so
v0.1 doesn't accidentally close doors v0.2 needs.

## 2. Surface

### 2.1 Declaration

A board carrying variants declares one or more `variant <Name> {
... }` blocks at the board level, alongside instances and nets:

```bhdl
board ProductFamily {
    // ─── Base design ────────────────────────────────────────────
    // Everything outside any variant block is "always populated"
    // and uses the literal values shown here.
    power VCC = 3.3V @ 500mA;
    ground GND;

    U1: STM32F401();
    R_FB: Res(10k);
    C_BACKUP: Cap(10uF);
    C_BYPASS: Cap(100nF);

    VCC -> U1.VDD;
    GND -> U1.VSS;
    // ... nets ...

    // ─── Variants ───────────────────────────────────────────────
    variant Basic {
        // Basic is the base design as-is — no patches.
    }
    variant Pro {
        // Per-SKU patches: same R_FB instance, different value.
        R_FB.value = 100k;
    }
    variant EU {
        // DNP a part for this SKU. The footprint still exists on
        // the PCB layout (silkscreen + pads); the manufacturing
        // BOM and pick-place omit it.
        dnp C_BACKUP;
    }
    variant DevKit {
        // Multiple patches in one variant.
        R_FB.value  = 47k;
        dnp C_BACKUP;
    }
}
```

### 2.2 Patch statements

The v0.1 variant body accepts exactly two statement forms.

**Value override:**

```bhdl
<instance_name>.value = <expr>;
```

Replaces the literal value the base design assigned to that
instance. `<expr>` uses the same expression grammar as the base
(numeric literals, unit suffixes, constants). The override is
type-checked against the entity's declared parameter shape — you
can't put a string where a number is expected.

**Do-not-populate:**

```bhdl
dnp <instance_name>;
```

The instance stays in the netlist and on the PCB layout (footprint,
silkscreen). It is omitted from manufacturing BOMs and pick-place
files for this SKU.

### 2.3 Semantics

- Every variant has access to every instance declared in the
  board's base. A variant block may not declare new instances or
  reference instances that don't exist in the base — v0.1.
- A variant block is a **patch** on the base. The base is evaluated
  first; the selected variant's patches are applied on top.
- If two variants exist but the user selects neither, behaviour is
  governed by the CLI rule below (§2.4).
- If a single variant block assigns the same field twice, the
  parser emits a warning; last write wins.
- A board with no `variant` blocks is treated as a single-variant
  board (the implicit "default" SKU). Existing boards keep working
  unchanged.

### 2.4 CLI

```sh
bhdl-cli board.bhdl --sku Pro bom
bhdl-cli board.bhdl --sku Pro spice
bhdl-cli board.bhdl list-skus
```

- `--sku <Name>` selects the variant to materialise before
  synthesis / BOM walk / SPICE export. Available on every
  subcommand that consumes a netlist.
- `list-skus` prints the variant names declared on the board, plus
  the implicit "default" if no variants are declared.
- When a board declares variants but the user runs a netlist-
  consuming subcommand without `--sku`, the CLI errors with a
  list of available variants. Rationale: silently picking the
  first variant is the kind of "silent fallback" we already
  explicitly rejected for vendor design recipes (§Stage 6 work).

## 3. Implementation phases

| Stage | Surface | Scope |
|---|---|---|
| **V1a — Lexer/parser** | `variant`/`dnp` keywords; `variant <name> { ... }` block; `<inst>.value = <expr>;` and `dnp <inst>;` body statements | Parser only; AST tests |
| **V1b — Analyzer extraction** | `Variant { name, value_patches, dnp_set }` on AnalysisResult | Extraction + diagnostics for nonexistent instance references |
| **V1c — Netlist patching** | Apply the selected variant's patches to the post-expansion netlist | DNP'd instances get `attribute do_not_populate = "true"` |
| **V1d — CLI** | `--sku` flag + `list-skus` subcommand; error on missing `--sku` when variants exist | One flag, one subcommand |
| **V1e — BOM walker** | Skip DNP'd instances; the value override already reaches the instance via attribute pass-through (Stage 7 work) | Trivial — DNP filter only |

## 4. Explicit non-goals for v0.1

- **Whole-module gating** (a variant adds/removes instances): needs
  netlist diff semantics — what nets a removed instance leaves
  dangling, whether the analyzer should warn or auto-stitch.
- **MPN / manufacturer override per variant**: mechanically easy —
  `R_FB.mpn = "..."` would work the same way — but the surface
  question (which SKU attributes can be overridden, ergonomics for
  large overrides) deserves a separate pass.
- **Footprint variants**: touches the PCB layout grammar, not just
  the netlist. v0.2+ in concert with layout-block work.
- **Cross-variant exclusion sets** (e.g. "EU SKU must not include
  the FCC-only part"): SAT-shaped. Deserves explicit constraint
  language; not "continue"-sized.
- **Hierarchical variant inheritance** (`variant ProEU includes
  Pro, EU`): nice ergonomics, complicates the patch-application
  order. Worth doing once base v0.1 is in production use.

## 5. Interaction with existing features

- **Intent surface (`for amplifier(...)`)**: intents fire per-SKU.
  A value override changes the inputs the design recipe sees, so a
  variant that bumps `R_FB.value` from 10k to 100k changes the
  resulting bias network.
- **Design blocks (Stages 1–6)**: same. Recipes are invoked with
  variant-applied values.
- **Expansion blocks**: untouched. Variants patch the parent's
  literals; the expansion runs against those.
- **SKU attributes (Stage 7)**: per-instance `attribute mpn = ...`
  on entity definitions is unchanged. Variant-level MPN override
  is v0.2 territory.
- **PnR / KiCad export**: the PCB layout is one (the same footprint
  exists for every variant); DNP'd instances are marked as such on
  the export so assembly houses know to skip them.

## 6. Naming notes

- `variant` is the name v0.1 picks for the block keyword. KiCad
  ("variant"), Altium ("Variant"), Cadence ("Option") aren't fully
  aligned in the EDA world; "variant" is the most common.
- `dnp` is the established EE abbreviation for "do not populate."
  No ambiguity. Some tools spell it `DNL` ("do not load"); same
  semantics, dnp is the broader convention.
- The SKU itself (the identifier) is the variant's *name*. v0.1
  doesn't introduce a separate "SKU type" — the variant name is
  the SKU. Future work might add manufacturing metadata (e.g.
  `variant Pro { sku_code = "PRO-2026-A"; }`).

## 7. Why this resolves the BOM ambiguity

Before v0.1: a single `.bhdl` produced a single BOM. To ship more
than one product variant, the user either maintained multiple
`.bhdl` files (duplication, drift) or post-processed the BOM
manually (error-prone, breaks reproducibility).

After v0.1: one `.bhdl` declares a base design plus a small set of
patches. Each shipped SKU is one variant; its BOM is reproducible
from the source. Manufacturing-house workflows that already accept
per-variant pick-place files map 1:1 onto our `--sku` output.

The supplier-picker integration (separate task, future work) then
runs per-variant — the same `Res(575)` in two SKUs both get the
same chosen reel because they share the abstract entity.
