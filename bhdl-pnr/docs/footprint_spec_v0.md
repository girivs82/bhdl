# BHDL Footprint Spec v0

> **Status:** Proposal v0 — **runtime/P&R steps partially landed
> (2026-05-30).** Implemented: the `ipc7351_dip` through-hole generator
> (§3.4, step 3 — closes the ATmega DIP-28 estimation gap; pad geometry
> unit-tested); density-aware courtyard (§6.1, part of step 2 —
> `DensityLevel::courtyard_excess_mm`, `BoardConfig.courtyard_excess_mm`,
> `Component::courtyard_extent`, wired into overlap resolution); and the
> **KiCad→bhdl footprint translator** (§7 —
> `footprint.rs::translate_kicad_mod`, emits a `footprint { }`
> declaration, 4 unit tests). Not yet landed: the `footprint` /
> `footprint_ref` **grammar** (steps 4–6, synth-owned) needed to *parse*
> the translator's output, tier-1 declared-footprint resolution, the
> `bhdl-common` footprint type with explicit pad↔pin binding, and the
> entity-side footprint-reference attribute. Defines a first-class,
> bhdl-native `footprint` construct so
> footprint geometry — like the netlist, intents, and constraints —
> lives inside bhdl, source-controlled and parser-checked, with KiCad as
> an import source / export target rather than a runtime dependency.
>
> **Ownership (handshake model):** the **grammar + parser** for the
> `footprint` declaration is synth/parser-session-owned (same as the
> `for INTENT(...)` and `part_family` grammar). The **three-tier
> resolution into `Component.pins`/courtyard** and the **P&R
> consumption** (pads → wirelength/loop-area, courtyard → spacing
> constraints) are P&R-session-owned. The **KiCad-import converter** is
> jointly relevant; lives wherever the existing symbol importer lives.
> This doc is drafted P&R-side for synth review, mirroring
> `intent_vocabulary_v0.md`.
>
> **Precedent:** this is the same decision already made for the part
> catalog — `Parameterization_And_BOM_Resolution.md`: *"`part_family` is
> BHDL, not external data … importers may generate them but the
> canonical form is BHDL."* Footprints get the identical treatment for
> the geometric layer.

## 1. Motivation

Footprints are physical reference data: pad positions/sizes, the
courtyard (manufacturable keepout), the body extent, and cosmetic
layers. Today bhdl-pnr regenerates them at runtime from the `package`
string via IPC-7351B (`bhdl-pnr/src/ipc7351.rs`), with two consequences:

1. **Pad → pin binding is positional/inferred**, not declared
   (`semantic.rs` matches the *k*-th pad to the *k*-th pin def). Fragile;
   the exact thing that misplaces pins on non-standard parts.
2. **Coverage gaps fall back to estimation.** The IPC generator covers
   Chip / GullWing / QuadFlat / QFN / DPAK families. **DIP / through-hole
   is absent** — so the ATmega328P DIP-28 (and many KiCad-imported
   parts) currently get crude `estimate_pin_positions()` coordinates.
3. **No courtyard.** Overlap/spacing uses the body bounding box, not the
   manufacturable courtyard, so clearance isn't a real number.

bhdl already chose to own its catalog data rather than depend on a
third party. Footprints are the same kind of data and deserve the same
treatment — with one bhdl leg-up KiCad can't offer: **parametric
footprints** (§4).

## 2. Principles

- **Footprints are declarative data, not imperative language.** A BGA is
  400+ pads — a table, not a program. So `footprint` is a *declaration*
  (sibling to `entity`, `part_family`), parser-checked and in the symbol
  table, but with no control flow.
- **Footprints are a shared, importable library — never inlined.** This
  is the `part_family` insight one level more extreme: one SOIC-8
  footprint serves thousands of ICs; one 0603 serves every 0603 passive.
  So footprints live in their own `.bhdl` files
  (`bhdl-stdlib/footprints/…`) and are **imported** into component bhdls,
  exactly as entities import interfaces. Inlining footprint geometry into
  entity files would defeat the whole point.
- **Geometry and pin-naming are separated** (the rule that *makes* reuse
  work, §3.3). The shared footprint carries pad **geometry keyed by pad
  number/designator** — no pin names. The importing **entity** maps its
  own named pins to those pad numbers. (A SOIC-8's pad "1" is `OUT` on an
  op-amp but `IN` on a regulator — the name is the part's business, the
  geometry is shared.)
- **Two forms, mirroring `part_family`'s literal-vs-template split:** a
  **generated** form (IPC family + body dims → pads computed) for the
  common case, and a **literal** form (explicit pad table) for imported
  or odd parts.
- **Courtyard is first-class**, because it feeds the constraint model
  (§6): courtyard-to-courtyard clearance is a real `KeepAway` number.
- **KiCad is import/export, never runtime.** A `.kicad_mod` → `footprint`
  converter emits the literal form once; thereafter bhdl-native.
- **Round-trip-faithful, not ECAD-complete.** v0 interprets pads +
  courtyard + body. Cosmetic/fab layers (silk, paste, mask, fab notes)
  are carried as **opaque pass-through** blocks that import preserves and
  export re-emits but the toolchain does not interpret (warn-and-degrade
  on unknown — same discipline as intent properties).

## 3. The `footprint` declaration

### 3.1 Literal form

Pads are keyed by **pad number/designator** and carry **no pin names** —
the footprint is shared geometry. The importing entity supplies the
pin→pad map (§3.3).

```bhdl
// In bhdl-stdlib/footprints/connectors.bhdl
footprint Molex_PicoBlade_5021_08 {
    // pad DESIGNATOR kind shape at (x, y) size (w, h) [layer L] [drill D]
    pad "1"  smd roundrect at (-3.50, 4.0) size (1.20, 0.60) layer top;
    pad "2"  smd roundrect at (-3.50, 2.0) size (1.20, 0.60) layer top;
    pad "3"  smd roundrect at (-3.50, 0.0) size (1.20, 0.60) layer top;
    pad "MP1" smd rect      at (-5.00, 0.0) size (1.60, 2.0) layer top; // shield

    courtyard rect (10.0, 8.0);          // manufacturable keepout extent
    body       rect (8.0, 6.0) height 1.25;  // physical body + Z

    // Opaque, preserved-not-interpreted (round-trip to KiCad):
    silk { ... }
    fab  { ... }
}
```

### 3.2 Generated (parametric) form

The IPC generator already in `ipc7351.rs` is promoted from a hidden
helper to a declared generator. The common case is ~5 lines:

```bhdl
// In bhdl-stdlib/footprints/ipc_smd.bhdl

// A chip-resistor/cap family, parametric over body code.
// Produces pads designated "1","2" by the IPC equations — no pin names.
footprint Chip<BODY: package> : ipc7351_chip {
    body    = BODY;          // "0603" → Lmin/Wmin from the IPC chip table
    density = nominal;       // density level → pad expansion
}

// A gull-wing family, parametric over pin count.
// Produces pads designated "1".."PINS".
footprint SOIC<PINS: int> : ipc7351_gullwing {
    body    = soic_body(PINS); // table lookup: span/pitch/lead from PINS
    density = nominal;
}
```

`: ipc7351_chip` / `: ipc7351_gullwing` / `: ipc7351_qfn` etc. name the
built-in generators (one per `PackageFamily` variant already in
`ipc7351.rs`). Monomorphization expands `R_chip<"0603">` to a concrete
pad set exactly as the runtime path does today — but now it's a declared,
type-checked, cacheable artifact, and changing a body dimension
regenerates every dependent footprint.

The generated form's `bind …` clauses here are *generator-default* pad
designators (pad 1, pad 2, …), still not pin names — the entity overrides
with its own pin→pad map on import.

### 3.3 Import + bind: where pin names attach

The footprint is geometry; the **entity** names the pins and binds them
to pad designators on import. This is the rule that makes a single
SOIC-8 serve thousands of parts:

```bhdl
import { Chip, SOIC } from "bhdl-stdlib/footprints/ipc_smd.bhdl";

entity Resistor<R, TOL, PKG>() {
    pin 1: signal inout;
    pin 2: signal inout;
    footprint Chip<PKG> { 1 -> "1"; 2 -> "2"; }      // pin → pad designator
}

entity LM358() {                       // dual op-amp, SOIC-8
    pin OUT1; pin IN1N; pin IN1P; pin VEE;
    pin IN2P; pin IN2N; pin OUT2; pin VCC;
    footprint SOIC<8> {                // SAME shared footprint a regulator uses
        OUT1 -> "1"; IN1N -> "2"; IN1P -> "3"; VEE -> "4";
        IN2P -> "5"; IN2N -> "6"; OUT2 -> "7"; VCC -> "8";
    }
}
```

`Chip<"0603">` is one footprint shared across every 0603 passive;
`SOIC<8>` is one footprint shared across every SOIC-8 part — each entity
contributes only its own pin→pad lines.

**Default when the map is omitted.** Trivial parts (pin `"1"` == pad
`"1"`) may write `footprint Chip<PKG>;` with no block; resolution falls
back to **identity/positional binding with a warning**, suppressible by
an explicit `bind_identity`. This keeps the ~10⁴ passives terse while
forcing the op-amp / regulator / connector cases — where pad ≠ pin name
— to be explicit.

### 3.4 New built-in generator to close the gap: `ipc7351_dip`

v0 adds a through-hole DIP/SIP generator (absent today), so the ATmega
DIP-28 stops hitting estimation:

```bhdl
// In bhdl-stdlib/footprints/ipc_through_hole.bhdl
footprint DIP<PINS: int, PITCH: length = 2.54mm, ROW: length = 7.62mm>
    : ipc7351_dip
{
    pins = PINS; pitch = PITCH; row_spacing = ROW;
}
// DIP<28> → 28 plated through-holes designated "1".."28",
//           2 rows of 14, 2.54mm pitch. The ATmega328P_DIP28 entity
//           imports it and supplies the PD0/PB0/VCC/… → pad map.
```

### 3.5 Grammar sketch (synth-owned)

```ebnf
// ── Footprint declaration (its own file; pads keyed by designator) ──
footprint_decl :=
    "footprint" IDENT generic_params? generator_base? "{" footprint_body "}"

generator_base := ":" IDENT            // ipc7351_chip | _gullwing | _qfn | _dip | ...

footprint_body :=
    ( pad_stmt | courtyard_stmt | body_stmt | gen_param_stmt | opaque_block )*

pad_stmt :=                            // NO pin name — geometry only
    "pad" STRING pad_type pad_shape
    "at" "(" num "," num ")" "size" "(" num "," num ")"
    ( "layer" IDENT )? ( "drill" num )? ";"

pad_type  := "smd" | "tht" | "npth"
pad_shape := "rect" | "roundrect" | "circle" | "oval"
courtyard_stmt := "courtyard" shape ";"
body_stmt      := "body" shape ( "height" num )? ";"
gen_param_stmt := IDENT "=" expr ";"   // generator inputs (body=, pitch=, pins=…)
opaque_block   := ("silk"|"fab"|"paste"|"mask") "{" /* uninterpreted */ "}"

// ── Entity-side footprint reference (where pin names bind to pads) ──
footprint_ref :=
    "footprint" IDENT generic_args? ( bind_block | "bind_identity" )? ";"

bind_block := "{" ( bind_one )* "}"
bind_one   := pin_ref "->" STRING ";"  // entity pin → pad designator
            | "bind_sequential" ";"    // pin k → pad k, in declaration order
```

The footprint's pad/courtyard/body grammar is small and closed and names
no pins; the entity's `footprint_ref` attaches pin names to pad
designators. Opaque blocks carry whatever KiCad import produced, verbatim.

## 4. Why parametric footprints are the payoff

KiCad ships a static `.kicad_mod` per footprint. bhdl already has
generics + the entity/class/part monomorphization machinery; footprints
have the identical structure:

| Family | Varies by | One declaration covers |
|---|---|---|
| Chip | body code | 0201 … 2512 |
| SOIC/SOP | pin count | SOIC-8/14/16 |
| QFN/QFP | pitch × count × body | a whole grid |
| DIP | pin count × pitch × row | DIP-8 … DIP-40 |

So a few parametric `footprint` declarations replace hundreds of static
files, and a body-dimension fix propagates automatically. This is the
footprint analogue of what intent is for layout: bhdl expresses the
*generator*, not just the instance.

## 5. Linking footprint → entity → pins

The `package` generic an entity already carries is the **footprint
selector**, symmetric with how it selects a `part_family`:

```
Resistor<10kΩ, 1%, "0603">
    │  package = "0603"
    ├──→ part_family  (orderable MPN — Parameterization spec)
    └──→ footprint    (geometry — this spec):  R_chip<"0603">
```

One axis (`package`) resolves both the part to buy and the geometry to
place. The class's `package` value maps to a `footprint` by the same
catalog-scan mechanism `part_family` uses (§4.5 of the Parameterization
spec) — a `footprint` declares which classes it serves.

## 6. Three-tier resolution into P&R (P&R-owned)

`semantic.rs` gains an explicit source priority, mirroring the
rigid-vs-flexible pattern already in the placer (`PlacementRecipe` vs
intent):

| Tier | Source | When |
|---|---|---|
| 1 | declared bhdl `footprint` (literal or generated) | present for the class/package — **authoritative** |
| 2 | runtime IPC-7351B generation (`ipc7351.rs`) | recognized standard family, no declared footprint |
| 3 | `estimate_pin_positions()` | last resort; **logs a warning** naming the part (no silent low-fidelity placement) |

Today only tier 2 + 3 exist. Adding tier 1 closes the fidelity gap.

What each tier must produce for P&R:

- **`Component.pins`** — `PinPosition { name, dx, dy }` with the pad
  coordinate **and the bound pin name** (tier 1 binds explicitly; tiers
  2/3 keep today's positional matching but the warning makes the
  downgrade visible).
- **`Component` courtyard** — a new field (`courtyard: Shape` or
  `courtyard_w/h`) used by overlap resolution + density + the spacing
  constraints, instead of the body bbox.
- **`Component` body height** — stub field for future 3D/mechanical.

### 6.1 Courtyard → constraint model

The courtyard makes clearance a real number the constraint catalog
(`constraint_model_v0.md` §3.1) consumes directly:

- `KeepAway { a, b, min_mm }` and the overlap/legalization spacing use
  **courtyard extents**, not body bbox.
- A `Keepout` region can be a footprint's courtyard on a given layer.

This is the concrete reason courtyard is first-class rather than
cosmetic: pads drive wirelength/loop-area, **courtyard drives spacing**,
and both are now real geometry rather than estimates.

## 7. KiCad → bhdl translator (offline; the runtime imports bhdl only)

**Architectural decision (2026-05-30):** the toolchain imports **bhdl
only** — there is no runtime KiCad dependency, for footprints *or*
symbols. KiCad is a one-time *translation source*. A separate offline
translator converts a KiCad project to `.bhdl` (entities + `footprint`
declarations); from then on bhdl is canonical and KiCad is absent from
the build. This is the same principle the rest of the system already
applies (netlist, intents, constraints, part catalog all bhdl-native).

The footprint half of the translator is **implemented**:
`bhdl-pnr/src/footprint.rs::translate_kicad_mod()` parses a `.kicad_mod`
(reusing the existing `bhdl-components` parser) and emits a bhdl
`footprint { }` declaration per §3.5 — unit-tested against a real 0603
and a through-hole pad. (The emitted text is parsed by the synth-owned
`footprint` grammar, not yet built, so translate → parse → consume
round-trips once the grammar lands.)

```
.kicad_mod  ──translate──▶  footprint <Name> { pad table + courtyard
                                + body + opaque silk/fab blocks }  (.bhdl)
       (offline, one-time)            │
                                      ▼  runtime imports THIS, never .kicad_mod
```

Mapping:

| KiCad `.kicad_mod` | bhdl `footprint` (geometry only) |
|---|---|
| `(pad "1" smd roundrect (at x y) (size w h) (layers ...))` | `pad "1" smd roundrect at (x,y) size (w,h) layer top` |
| `(fp_line ... (layer "F.CrtYd"))` collected into an extent | `courtyard rect (...)` |
| `F.Fab` body outline | `body rect (...) height <from 3D model if present>` |
| `F.SilkS`, `F.Paste`, `F.Mask`, fab text | opaque `silk{}` / `paste{}` / `mask{}` / `fab{}` (preserved) |

The KiCad footprint has no pin names (only pad numbers) — so the
translation maps cleanly to a name-free bhdl `footprint`. The **pad→pin
binding** comes from the *symbol* translation (KiCad symbol pin numbers →
names), and lands on the **entity's** `footprint_ref` block (§3.3), not
in the footprint. This is why bhdl can re-describe a KiCad
symbol+footprint pair as `entity { … footprint F { pin -> "pad" } }` with
the geometry shared.

After translation the footprint is bhdl-native: editable, parametrizable,
diffable, KiCad-independent. Export re-emits `.kicad_mod` from the
footprint declaration plus the entity's bind map (round-trip),
re-attaching the preserved opaque blocks.

> **Symbol side is synth-owned.** The same bhdl-only / offline-translator
> principle applies to KiCad *symbols* → `entity` declarations, but that
> translator is the synth session's domain (the existing runtime symbol
> import should likewise become an offline translation step). Flagged for
> coordination; out of scope for this P&R-side doc.

## 8. Data model changes (P&R-side)

In `bhdl-pnr/src/types.rs`:

- `Component` gains `courtyard: Courtyard` (extent or polygon) and
  `body_height_mm: f64` (stub, default from footprint or 0).
- `PinPosition` already has `name`/`dx`/`dy`; tier-1 resolution fills
  `name` from the explicit `-> PIN` binding rather than positional
  inference.

The existing `bhdl_components::ComponentFootprint` / `FootprintPad`
(`bhdl-components/src/types/component.rs`) are extended or wrapped to
carry: **pad layer**, **pad→pin binding**, **courtyard**, **body
height**. (Today `FootprintPad` has none of these; `ComponentFootprint`
has `svg_data` (cosmetic) + body dims but no courtyard/binding.) Whether
to extend those types in `bhdl-components` or define a P&R/`bhdl-common`
footprint type is an open question (§10) — leaning `bhdl-common` so the
type crosses the same boundary the netlist/intent types do.

## 9. Scope discipline (what v0 does NOT do)

- No via-in-pad, complex pad stacks, thermal-relief spokes, or
  per-layer pad geometry — pads are single-layer (or TH spanning) with
  one shape.
- No silkscreen/paste/mask *interpretation* — opaque pass-through only.
- No 3D model geometry — just a `height` scalar slot.
- No DRC rule encoding in the footprint — clearance rules live in the
  constraint model / board config, not the footprint.
- No footprint *synthesis* from a datasheet — import or declare only.

## 10. Open questions

- **Footprint type home:** extend `bhdl_components::ComponentFootprint`,
  or a new `bhdl_common` footprint type crossing the session boundary
  like the netlist/intent types? Leaning `bhdl-common`.
- **Courtyard shape:** rectangle-only for v0 (covers ~all parts) vs.
  polygon from the start (KiCad courtyards can be non-rectangular).
  Leaning rect + an opaque polygon fallback preserved for export.
- **Default bind when the entity omits the map** (resolved-ish): identity
  binding (pin name == pad designator) with a warning, suppressible by
  `bind_identity`. The remaining sub-question is whether identity should
  match on pin *name* or declaration *order* when both are present —
  leaning name-first, order-fallback.
- **Bind ergonomics for large parts:** the entity `footprint_ref` bind
  block lists `pin -> "pad"` per pin; QFN/BGA with 100+ pins want a
  compact form (`bind_sequential` for in-order; a row/col generator for
  BGA grids — possibly reusing the `generate for` primitive).
- **Interaction with abstract entities (v0.9b):** each SKU in a
  `family { }` already has a `footprint "..."` string
  (`Synthesis_Auto_Expansion.md` §8). v0.9b's footprint string should
  resolve to a declared `footprint` by this spec, not a KiCad library
  path. Coordinate when v0.9b footprint propagation is implemented.

## 11. Implementation order (after current P&R milestones)

1. **Type + data model** (`bhdl-common` footprint type or extend
   `bhdl-components`): pad layer + pad→pin binding + courtyard + body
   height. P&R-side `Component.courtyard`.
2. **Tier-1 resolution in `semantic.rs`** + the warning on tier-3
   fallback. Wire courtyard into overlap/density/`KeepAway`.
3. **`ipc7351_dip` generator** — closes the ATmega/through-hole gap;
   smallest concrete win, testable against the ATmega DIP-28.
4. **`footprint` grammar + parser** (synth-owned) — literal form first,
   then the generated form keyed to the existing `ipc7351.rs` generators.
5. **KiCad `.kicad_mod` → `footprint` importer** (one-time convert) +
   export round-trip.
6. **`package` → footprint catalog-scan link**, symmetric with
   `part_family` selection.

Steps 1–3 are P&R-side and unblock the placement-fidelity gap without
any grammar work. Steps 4–6 are the language/import surface and need the
synth/parser session.
