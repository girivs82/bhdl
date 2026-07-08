# Handles and Reference Designators — Two Namespaces, One Allocator

> **Status:** BUILT (synthesizer phase 12.7, commit `9cbbaee`). Every
> consumer listed in §4 reads the stamped attribute; none allocates.

## 1. The doctrine

A design names its parts twice, for two different audiences:

- **Handle** — the *human* namespace. User-authored in board source
  (`r_load`, `input_bulk_cap`, `U1`), arbitrarily long and descriptive,
  stable by construction (it IS the source text). Synthesis-minted
  instances get synthesized handles derived from their parent's handle:
  expansion children (`U1_C_out`), cap-bank members (`C_IN1_2`), auto
  nets (`U1_VOUT`, `auto_D1_K`).
- **Refdes** — the *fab* namespace. Allocated by the toolchain
  (`R1`, `C3`, `U5`, `TP2`), compact because it lands on silkscreen, in
  BOM columns, pick-and-place files, and assembly-house paperwork.
  Prefix comes from the part's `component_class`
  (`bhdl_common::sku::refdes_prefix_for_class`); numbering is
  per-prefix.

Where each appears:

| Surface | Namespace |
|---|---|
| Board source wiring, entity expansion blocks | handle |
| Net names (`fb_node`, `U1_VOUT`) | derived from handles |
| Log prose, ERC finding text, plugin anchors | handle |
| Schematic part labels | refdes |
| BOM "Ref des" column, silkscreen, pick-and-place, KiCad export | refdes |
| Frozen netlist (`freeze`) components | refdes (attribute) |
| Sign-off / report tables | **`handle (refdes)`** — both, so the reader can act in either world |
| ERC plugin `DesignSummary.instances[]` | both fields: `handle`, `refdes` |

## 2. One allocator

Refdes is minted in exactly ONE place:
`bhdl_synthesizer::refdes_alloc::assign_refdes`, running as **pipeline
phase 12.7** — after every phase that mints instances (expansion 4.5,
entity-attribute stamping 4.6) and before DRC (13), so ERC plugin
summaries already carry real designators. It stamps the result as the
**`refdes` instance attribute** on the netlist; that attribute is the
single source of truth downstream.

Rules the allocator follows:

- **Idempotent**: an instance that already carries `refdes` is never
  touched. Flows that mint instances after generation (the CLI's
  input/output cap-bank sizers) simply re-invoke `assign_refdes`.
- **Deterministic**: allocation walks instances name-sorted — SlotMap
  iteration order is unstable and must never influence numbering.
- **Phantom-free**: definition-instances (module named after itself —
  synthesis bookkeeping, not parts) are skipped.
- An entity may force its prefix with the `refdes_prefix` attribute;
  otherwise `component_class` decides.

Why one allocator is a hard rule and not a style preference: before this,
the schematic and the SKU BOM each allocated independently, so the
schematic's `R1` and the BOM's `R1` could be **different physical
parts**. Nothing in the type system prevents a second allocator from
reintroducing that — this document does. If you need a designator,
read the attribute.

## 3. Stability: the committed sidecar

The handle → refdes mapping persists in a JSON sidecar next to the
board source: `<board>.bhdl.refdes` (`bhdl_common::refdes::RefDesLut`,
grouped by prefix). Once a handle has a designator it keeps it forever —
adding, removing, or reordering parts never renumbers survivors, so
silkscreen, assembly documentation, and review diffs stay valid across
revisions.

The sidecar is a **committed artifact** — the lockfile analogue. It is
not a cache: deleting it renumbers the board (every handle re-allocates
from 1 in walk order), which invalidates any fab paperwork already
issued. Check it in, review its diffs (a new line = a new part; a
changed line should not happen), and treat conflicts like lockfile
conflicts: regenerate on the merged source, don't hand-merge numbers.

Numbering gaps are normal and correct — a deleted part retires its
designator; the LUT allocates `max+1` per prefix and never reuses.

## 4. Consumers (read-only)

| Consumer | Behavior |
|---|---|
| Schematic v4 + HTML extraction | label map from the attribute; the extraction path keeps a LUT fallback ONLY for netlists that never went through synthesis (unit tests, hand-built netlists) |
| `bom` (`sku_bom`) | attribute; per-class counter fallback for never-synthesized netlists only |
| Sign-off report (`signoff`) | tables print `handle (refdes)`; row identity/override keys stay handle-based |
| `freeze` | attribute (handle fallback) into the frozen record |
| ERC T3 plugins (`erc_plugin`) | `DesignSummary` instances carry `handle` (anchor findings on this) and `refdes` |
| PnR / KiCad / fab exports | attribute |

A consumer that finds no `refdes` attribute is looking at a netlist that
never went through synthesis; falling back to the handle (or a local
LUT) is acceptable THERE, and only there.

## 5. Interaction with expansion and naming

Expansion children are named `<parent_handle>_<recipe_local>` —
`U1_C_out` — which is a *handle*, not a designator, even when the parent
handle happens to look like one (`U1`). The allocator gives such a child
its own refdes (`C3`), and report tables show `U1_C_out (C3)`. The test
corpus historically hand-names parts `C1`/`R1`/`U1`, which makes handles
*look* like designators; do not be misled — the namespaces are distinct
even when their spellings collide, and `handle (refdes)` labels collapse
to just the handle when the two coincide.

Net names remain handle-derived (`U1_VOUT`, `fb_node`) — nets have no
fab designator namespace.
