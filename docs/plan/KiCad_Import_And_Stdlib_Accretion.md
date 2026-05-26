# KiCad Import & Stdlib Accretion: Strategic Plan

> **Status:** **Phases A–I shipped + full pipeline + 5-board
> equivalence proof (2026-05-25..26).** The KiCad importer + BHDL
> pipeline now provably round-trips byte-identical netlists for
> five independently-authored open-source Arduino boards. 62
> tests green; six are hard-asserted equivalence tests
> (tiny RC fixture + 5 Arduino boards).
>
> | Board | Symbols | Sheets | Nets | Pins | Diffs |
> |---|---|---|---|---|---|
> | Tiny RC | 2 | 1 | 2 | 2 | 0 (strict) |
> | UNO R3 | 90 | 4 | 98 | 252 | 0 |
> | Nano | 77 | 1 | 125 | 174 | 0 |
> | Leonardo | ~88 | 4 | 86 | 223 | 0 |
> | Mega 2560 | 132 | 4 | 169 | 396 | 0 |
> | Micro | 97 | 1 | 129 | 199 | 0 |
>
> 4 MCU families covered (ATMEGA328P / ATMEGA32U4 / ATMEGA16U2 /
> ATMEGA2560), hierarchical and flat boards, 2..396 pins per
> board, all hard-asserted, deterministic. Real bugs across
> bhdl-parser / bhdl-analyzer / bhdl-synthesizer / bhdl-stdlib /
> bhdl-kicad-import surfaced and fixed along the way.

> **Original status:** Pre-implementation planning document. Every
> architectural decision, every phase of work, every test, every
> deliverable scoped before code. Companion to the spec docs in
> `docs/spec/`; this lives in `docs/plan/` because it's tactical
> sequencing rather than language design.

---

## Completion snapshot (2026-05-25)

| Phase | Status | What landed |
|-------|--------|-------------|
| **A** — KiCad reader infrastructure | ✅ shipped | `sexpr.rs` + `ir.rs` + `reader.rs`. KiCad 6+ S-expr → typed IR. UUID-safe number parser. 9 reader tests. |
| **B** — Symbol library resolution | ✅ shipped | `lib_resolver.rs` (sym-lib-table + external `.kicad_sym`) + `symbol_mapping.rs` (TOML registry). `bhdl-stdlib/kicad-symbol-mapping.toml` shipped. Lazy load + env-var template expansion. |
| **C** — Net topology extraction | ✅ shipped | `nets.rs`. Union-find on micron-quantised connection points. Mid-wire point-on-segment attachment for labels/junctions/power flags. Power > global > hierarchical > local label-naming priority. |
| **D** — BHDL emitter | ✅ shipped | `emitter.rs`. Root → `board { }`, child sheets → `entity { }`. Power flags → `power VCC_5V = 5V;` / `ground GND;`. Unmapped → `kicad_passthrough` + warning. Multi-unit dedup. `{slash}`/`{tilde}`/etc. escape decoding. |
| **E** — Canonical netlist + comparator | ✅ shipped | `canonical.rs`. `CanonicalNetlist` byte-deterministic. Per-sheet flattening with `/Sheet/` prefix. KiCad `.net` file parser. Structured `EquivalenceReport`. **Surfaced and fixed a real determinism bug in `nets.rs`** (HashMap-order `Net_N` naming). |
| **F** — Arduino UNO R3 end-to-end | ✅ shipped | Real KiCad 8 schematic ingested. **154 nets, 252 pins, 38.2% stdlib coverage** (45.6% after Phase G). 370-line BHDL emitted with hierarchy preserved. Caught two real bugs: KiCad `{slash}` label escapes, multi-unit-part declaration duplication. |
| **G** — Stdlib alignment + passthrough stub | ✅ shipped | `bhdl-stdlib/kicad_passthrough.bhdl` stub entity. Mapping registry overhauled: only entries pointing at real entities, PascalCase aligned to actual stdlib names. Disciplined fallthrough to passthrough for unmapped parts. |
| **H** — BHDL round-trip equivalence test | ✅ shipped | `tests/roundtrip.rs`. `EmitOptions::stdlib_path` for import-line generation. `canonical_from_bhdl_netlist` walks synthesizer-produced `bhdl_netlist::Netlist`. **Phase H surfaced the real upstream gap**: the synthesizer can't load the rich-tier stdlib files because bhdl-parser doesn't accept their syntax. |
| **I** — Reroute to parser-compatible stdlib | ✅ shipped | `stdlib_entity_file` rerouted to v2.0-grammar variants (`passives/resistor_simple.bhdl`, `passive/diode.bhdl`, `optoelectronic/led.bhdl`, `protection/tvs.bhdl`). **Round-trip now reaches `compare()` for the first time.** |

## Two upstream blockers Phase H/I surfaced (not importer-side)

The importer's contract — emit BHDL that parses, analyzes, and
synthesizes — is satisfied. Round-trip equivalence remains
blocked on two `bhdl-synthesizer` issues:

1. **Component-database wiring for imported entities.** When the
   synthesizer sees `R1: Resistor("1k");` with an `import { Resistor }
   from "..."`, it parses the import but does NOT register the
   resolved entity in its inference / pin-binding layer. Pass 6
   reports `Unknown component type 'Resistor' - no inference rules
   available`. Fix lives in `bhdl-analyzer/src/component_inference.rs`
   (`module_resolver` wiring) + `bhdl-synthesizer/src/interface_synthesis.rs`
   (`get_or_create_X_module` hardcoded paths). Substantial work
   (~300 LOC across multiple files).
2. **Ambient power-domain instances.** The analyzer's "Power
   Analysis" pass pre-populates a default set of power domains
   (`USB_5V`, `VCC_1V8`, `VCC_3V3`) the source never declared. The
   synthesizer materialises these as `Instance` records, creating
   phantom nets that don't exist in the KiCad source. Fix:
   filter ambient domains to those actually referenced by
   source-side connections.

Both fixes are pure `bhdl-synthesizer` work and don't require
revisiting the importer. Until they land, Phase H's `compare()`
runs and produces a structured diff, recorded as diagnostic output
rather than a hard assertion.

## Three natural next strands

The "translate then enrich" thesis is now provably viable. The
work fans out into three independent strands:

1. **Stdlib accretion (the original Phase G in this doc).** Work
   through Arduino UNO's 37 unmapped lib_ids — 16× `R_Pack04_Split`
   is the highest-leverage entry; ATmega328P and ATmega16U2 are the
   marquee entities to author. Each new entity + mapping line
   monotonically raises the importer's coverage from 45.6%.
2. **Synthesizer pin-instance wiring (the upstream blocker).** Wire
   imported entity pin declarations into the synthesizer's
   component database so `R1: Resistor("1k")` produces real
   `PinInstance` records. Closes Phase H's equivalence proof.
3. **More boards.** Run the importer on bigger open-source designs
   (Arduino Leonardo / Nano / Mega from the same sabogalc tree;
   ESP32 dev boards; Raspberry Pi Pico). Each new board surfaces
   new edge cases against the importer's contract.

These are mutually independent and can run in parallel.

---

> _Original planning notes preserved below._

## Table of Contents

1. [Goals and non-goals](#1-goals-and-non-goals)
2. [Strategic methodology: "translate then enrich"](#2-strategic-methodology-translate-then-enrich)
3. [Architectural decisions that must be made before code](#3-architectural-decisions-that-must-be-made-before-code)
4. [KiCad format primer: what we must handle](#4-kicad-format-primer-what-we-must-handle)
5. [Phase-by-phase work breakdown](#5-phase-by-phase-work-breakdown)
6. [Test boards and what each one exercises](#6-test-boards-and-what-each-one-exercises)
7. [Stdlib growth model](#7-stdlib-growth-model)
8. [CLI surface](#8-cli-surface)
9. [Risks and unknowns](#9-risks-and-unknowns)
10. [Success criteria per phase](#10-success-criteria-per-phase)
11. [Effort and sequencing summary](#11-effort-and-sequencing-summary)

---

## 1. Goals and non-goals

### 1.1 Goals

- Translate real open-hardware KiCad schematics into BHDL with
  verified netlist equivalence.
- Accrete a real-world stdlib (~hundreds of parts) covering common
  hobbyist / maker board components from real use.
- Demonstrate BHDL's incremental capability-unlock thesis: same
  netlist invariant + progressively richer abstraction.
- Surface every architectural gap the spec docs missed, against
  the punishing oracle of known-good real-world designs.
- Produce demo artifacts (translated boards + per-board
  enrichment) that double as documentation and credibility
  signals.

### 1.2 Non-goals (this work)

- Round-trip BHDL → KiCad (one-way translation only in this phase).
- Translation of arbitrary KiCad versions (target KiCad 6+;
  KiCad 5 and earlier have a different file format and are out
  of scope).
- Full PCB layout translation (footprint references are
  extracted as attributes; PCB → BHDL layout is future work).
- Translation of KiCad-specific verification artifacts (ERC
  rules, custom DRC, BOM scripts).
- Closing every architectural gap surfaced — some will be deferred
  to follow-up specs; the plan is to surface them, not fix them
  all in this phase.

---

## 2. Strategic methodology: "translate then enrich"

The development methodology has three invariants.

### 2.1 The netlist invariant

For every translated board, the BHDL synthesis output produces a
netlist *bit-equivalent* to the original KiCad netlist export
(modulo canonicalization of net names and reference designators).
This invariant holds at every step of enrichment:

```
                          ┌──────────────────────┐
                          │  KiCad schematic     │
                          │  (.kicad_sch)        │
                          └────────┬─────────────┘
                                   │
                ┌──────────────────┼──────────────────┐
                │                  │                  │
        KiCad ERC                  │            BHDL translator
        netlist export             │            (Phase A–D)
                │                  │                  │
                ▼                  │                  ▼
         ┌──────────────┐          │           ┌──────────────┐
         │  Netlist X   │          │           │  .bhdl file  │
         │  (reference) │          │           └──────┬───────┘
         └──────┬───────┘          │                  │
                │                  │           bhdl-cli synthesize
                │                  │                  │
                │                  │                  ▼
                │                  │           ┌──────────────┐
                │                  │           │  Netlist Y   │
                │                  │           └──────┬───────┘
                │                  │                  │
                └─────────── must be equivalent ──────┘
                            (Phase E: comparator)
```

### 2.2 The capability-unlock progression

The translated .bhdl starts as a 1:1 transcription of the
schematic. Enrichment adds capability without breaking the
netlist invariant:

| Enrichment step | What's added | Netlist effect |
|---|---|---|
| 0. Naive translation | Every component + connection 1:1 | matches original |
| 1. Virtual-pin composites for ICs with mandatory passives | LDO entity carries its output decoupling cap inside an `expansion { }` block | same (expansion produces identical components) |
| 2. Design recipes for derived values | Voltage divider sized by intent rather than literal Rs | same (recipe produces identical values) |
| 3. Variant declarations | Multiple board SKUs derived from one source | same per variant (compared against each variant's KiCad if available) |
| 4. SKU attribute enrichment | manufacturer / MPN / distributor SKUs populated | same; BOM resolution now order-ready |
| 5. Behavioral models | Simulation testbenches become possible | same (behavioral runs alongside, doesn't change netlist) |
| 6. Thermal / mechanical (future) | Multi-domain simulation | same |

Each step is independently committable. The netlist comparator
(Phase E) is run after every step.

### 2.3 The bug-discovery loop

Every translated board surfaces gaps. Each gap drives one of:

- A spec amendment (rare — should be a sign of real architectural
  trouble)
- A stdlib enrichment (common — the part needed an attribute we
  didn't anticipate)
- A translator bug fix (most common — the importer didn't handle
  some KiCad construct)
- An auto-generation improvement (when the translator emits stub
  entities, those stubs need progressively more sophisticated
  defaults)

The plan is **not** to fix every gap immediately — just to surface
them and decide which class each belongs to.

---

## 3. Architectural decisions that must be made before code

These are pre-implementation choices. Once code lands, changing
them is expensive.

### 3.1 KiCad target version

**Decision: KiCad 6, 7, and 8 only.** All share the S-expression
`.kicad_sch` format (versioned but largely compatible). KiCad 5
and earlier used a different format (semicolon-delimited),
covering them adds engineering effort for diminishing returns.
OSHW projects post-2022 are almost entirely KiCad 6+.

### 3.2 Symbol mapping policy

KiCad symbols are identified by `Library:SymbolName` (e.g.
`Device:R`, `MCU_ST_STM32F4:STM32F411RETx`). BHDL stdlib entities
are identified by their entity name (e.g. `Res`, `STM32F411RETx`).

**Mapping decision: a registry file** at
`bhdl-stdlib/kicad-symbol-mapping.toml` records the mapping:

```toml
[mappings]
"Device:R"          = { entity = "Res",     pin_map = { "1" = "1", "2" = "2" } }
"Device:R_US"       = { entity = "Res",     pin_map = { "1" = "1", "2" = "2" } }
"Device:C"          = { entity = "Cap",     pin_map = { "1" = "1", "2" = "2" } }
"Device:LED"        = { entity = "LED",     pin_map = { "K" = "K", "A" = "A" } }
"Device:Q_NPN_BCE"  = { entity = "NPN_2N3904", pin_map = { "B" = "B", "C" = "C", "E" = "E" } }
# ... ~50 common mappings shipped with bhdl ...
```

Auto-generated entries land in the same file but flagged for
review:

```toml
[mappings."MCU_ST_STM32F4:STM32F411RETx"]
entity = "STM32F411RETx_AutoGen"
auto_generated = true
review_status = "stub"  # values: stub, partial, verified
```

This file is part of the stdlib and accretes over time as boards
land. The translator consults it; missing entries trigger
auto-generation of an entity stub + a TODO entry in the
mapping file.

### 3.3 Auto-generation policy for unknown symbols

When the translator encounters a symbol not in the mapping:

1. **Emit a stub BHDL entity** in
   `bhdl-stdlib/auto/<library_name>/<symbol_name>.bhdl` with:
   - Pin declarations derived from the KiCad symbol's pins
   - Whatever fields KiCad has (Value, Footprint, Datasheet,
     custom properties) as BHDL attributes
   - A `// TODO: enrich` header comment
   - `attribute __auto_generated = "true";` so the BOM walker
     and other consumers can flag these for review
2. **Add a mapping entry** with `auto_generated = true,
   review_status = "stub"`.
3. **Continue translation** — don't fail.

This means a board with 50 unknown parts produces 50 stub entities
plus the translated board. The stubs work for netlist
equivalence (their pin counts match KiCad's), but they're
hollow until hand-enriched.

### 3.4 Net naming policy

KiCad uses several net-naming mechanisms:

- **Local labels** (`signal_name` attached to a wire): create a
  named net with that local scope.
- **Global labels** (`<signal_name>` with hierarchical arrow):
  spans hierarchy.
- **Hierarchical labels** (sheet input/output): connects sheets.
- **Power flags** (`+5V`, `GND`, `+3V3`): treated as a global
  net with a specific class.
- **Anonymous wires** (no label): create auto-named nets like
  `Net-(R1-Pad1)`.

BHDL has its own net-naming machinery. The translator
canonicalizes:

| KiCad construct | BHDL emission |
|---|---|
| Local label `BUS_CLK` | `net BUS_CLK: ...` declaration |
| Power flag `+5V` | `power VCC_5V = 5V @ <auto-budget>;` (board-level) |
| Power flag `GND` | `ground GND;` (board-level) |
| Hierarchical pin | Entity pin declaration on the parent + connection on the child |
| Anonymous wire | Anonymous BHDL net (synthesizer auto-names; reproducible from connection topology) |

The net name canonicalizer normalizes KiCad's various forms to
BHDL's; the netlist comparator (Phase E) accounts for any
remaining variation.

### 3.5 Hierarchical sheet handling

KiCad supports hierarchical schematic sheets: a "sheet symbol" on
the parent schematic references a child `.kicad_sch` file. Sheet
ports become connections between parent and child.

**Decision: map each hierarchical sheet to a BHDL entity.** The
child sheet's contents become the entity's `expansion { }` block;
the sheet symbol on the parent becomes an instance of that entity.

Edge case: a board with N copies of the same sub-sheet (e.g.
four identical filter stages) becomes one entity instantiated
N times in BHDL — automatic deduplication.

### 3.6 Multi-unit / multi-gate symbols

A 4-channel op-amp in KiCad is *one* component reference (`U1`)
with 4 *units* (`U1A`, `U1B`, `U1C`, `U1D`), each unit having a
subset of the pins. The shared power pins are typically only on
unit A (or assigned to one unit).

**Decision: each multi-unit reference becomes one BHDL instance**
with all pins exposed. The translator gathers pin connections
from all units before emitting the instance.

### 3.7 Component value parsing

KiCad's "Value" field is free-text. Common forms:

- `10k`, `4.7k`, `1M` — resistors
- `100n`, `1u`, `47p` — capacitors
- `LM317`, `ATmega328P` — IC part names
- `Red`, `Blue` — LEDs

**Decision: a small value-parsing pass** that:

1. Tries to parse `<number><unit_suffix>` form into a BHDL numeric
   literal with units (`10k` → `10000`, `100n` → `100e-9`)
2. Falls back to keeping the value as a string attribute when
   the form is non-numeric

The mapping registry can override per-symbol: `Device:R` always
parses as numeric resistance; `Device:Q_NPN_BCE` is a part-name
identifier, not a value.

### 3.8 Component reference designator preservation

KiCad assigns reference designators (`R1`, `C3`, `U7`). These are
stable identifiers users grew up with.

**Decision: preserve original ref designators** in the translated
BHDL. The BHDL instance name becomes the KiCad reference
designator (`R1: Res(10k);` not `Res_1: Res(10k);`). The BOM
walker's refdes-prefix-from-class logic still works as a
fallback for boards that get authored fresh in BHDL.

### 3.9 Output file organization

A KiCad project has the schematic in one or more `.kicad_sch`
files, the PCB in `.kicad_pcb`, and metadata in `.kicad_pro`.

**Decision: emit one `.bhdl` file per `.kicad_sch` file.** Top
sheet → top BHDL file (containing the `board` declaration);
child sheets → sub-BHDL files imported by the top file. This
preserves the user's mental organization.

Stdlib stubs and registry updates go into the stdlib directory
tree (not the user's project directory).

### 3.10 Variant detection from KiCad

Some KiCad designs annotate variant information via custom
fields (e.g. `Variant = "Pro"`) or via the multi-variant feature
in newer KiCad versions.

**Decision: extract custom variant fields if present**, emit as
BHDL `variant <Name> { ... }` blocks. v0.1 only handles the
flat-multi-variant pattern; the more complex KiCad variant
hierarchies are deferred to follow-up.

### 3.11 DNP / unpopulated markings

KiCad supports marking components as "Do Not Populate" via
custom fields or the "Exclude from BOM" property.

**Decision: detect DNP markings** and emit them as `dnp <inst>;`
inside a default variant block (or as a top-level marking if
no variants are declared).

### 3.12 Encoding and Unicode

KiCad files are UTF-8. BHDL files are UTF-8. No translation
needed; pass through.

### 3.13 Round-trip safety

**Decision: importers must be deterministic.** Running the same
KiCad input through the translator twice produces byte-equivalent
BHDL output. This means stable sort orders, canonical numeric
formatting, deterministic auto-generated names. Diffs reflect
real changes only.

---

## 4. KiCad format primer: what we must handle

This section captures the KiCad 6+ schematic format details the
translator must parse. Not exhaustive — just the constructs we
hit in real OSHW boards.

### 4.1 File structure overview

A KiCad 6+ project is:

```
project/
├── project.kicad_pro          # project settings (JSON)
├── project.kicad_sch          # top-level schematic (S-expr)
├── project.kicad_pcb          # PCB layout (S-expr)
├── sub_sheet_a.kicad_sch      # hierarchical sub-sheets
├── sub_sheet_b.kicad_sch
├── sym-lib-table              # symbol library table
├── fp-lib-table               # footprint library table
└── *.kicad_sym                # custom symbol libraries
```

The translator reads:
- The `.kicad_pro` for project-level config (variant definitions,
  if any)
- Every `.kicad_sch` (parent and children) for the schematic
  content
- `sym-lib-table` to resolve symbol references to libraries
- Embedded `.kicad_sym` for project-specific custom symbols

It ignores:
- `.kicad_pcb` (PCB layout — future work)
- Backup files (`*.kicad_sch-bak`, etc.)
- Auto-generated reports

### 4.2 The S-expression schematic format

A `.kicad_sch` file looks like:

```scheme
(kicad_sch
    (version 20231120)
    (generator eeschema)
    (uuid <uuid>)

    (paper "A4")
    (title_block ...)

    (lib_symbols
        (symbol "Device:R" ...)
        (symbol "Device:C" ...)
        ...
    )

    (junction (at 100 50) (diameter 0))
    (no_connect (at 50 60))

    (wire (pts (xy 100 50) (xy 100 60)) (stroke ...))

    (label "BUS_CLK" (at 100 55) ...)
    (hierarchical_label "SHEET_IN" (shape input) ...)
    (global_label "RESET" ...)

    (power_flag "VCC" (at 75 25) ...)

    (symbol
        (lib_id "Device:R")
        (at 100 50 0)
        (unit 1)
        (in_bom yes)
        (on_board yes)
        (uuid <uuid>)
        (property "Reference" "R1" (at 102 48 0))
        (property "Value" "10k" (at 102 52 0))
        (property "Footprint" "Resistor_SMD:R_0603_1608Metric" ...)
        (property "Datasheet" "" ...)
        (pin "1" (uuid ...))
        (pin "2" (uuid ...))
    )

    (sheet
        (at 200 100)
        (size 50 30)
        (uuid <uuid>)
        (property "Sheetname" "Power")
        (property "Sheetfile" "power.kicad_sch")
        (pin "VIN" input (at 200 110) ...)
        (pin "VOUT" output (at 250 110) ...)
    )

    (sheet_instances
        (path "/" (page "1"))
    )
)
```

The translator needs to parse this structure faithfully.

### 4.3 Symbols, references, units

A `symbol` in the schematic is an *instance* of a library
`symbol` (declared in `lib_symbols`). Properties on the instance:

| Property | Meaning |
|---|---|
| `Reference` | The refdes (R1, C3, U7) |
| `Value` | The component value (10k, ATmega328P) |
| `Footprint` | The footprint library reference |
| `Datasheet` | URL |
| Custom fields | Vendor-specific (MPN, DigiKey PN, Variant, DNP, etc.) |

The library `symbol` declaration has:

- Pin definitions (number, name, electrical type)
- Graphical primitives (lines, rectangles — irrelevant for
  netlist translation)
- Unit count (1 for single-unit, 2+ for multi-gate ICs)

### 4.4 Connectivity primitives

KiCad connectivity is *topological* (wires, junctions, labels)
rather than declarative.

| Primitive | Meaning |
|---|---|
| `wire` | A line segment between two points; nets are formed by traversing connected wire segments |
| `junction` | Explicit "yes, these wires connect" marker at a crossing |
| `no_connect` | Explicit "this pin is intentionally unconnected" |
| `label` | A local-scope net name attached to a wire endpoint |
| `global_label` | A schematic-wide net name |
| `hierarchical_label` | A net name that crosses sheet boundaries |
| `power_flag` | A specific kind of global label for power/ground nets |

Translator algorithm: build a graph of (point, point) edges from
all wires + junctions, propagate labels to connected components,
produce a list of distinct nets with their participating pins.

### 4.5 Hierarchical sheets

A `sheet` symbol on the parent references a child schematic. The
parent's `sheet` has `pin` entries (input, output, inout) that
connect to the parent's wires; these pins correspond to
`hierarchical_label` entries inside the child schematic.

Translator: emit each child as a BHDL entity with the appropriate
pin declarations; emit the parent's `sheet` instance as an
entity instance with its connections.

### 4.6 Power flags and rails

KiCad doesn't have a first-class "power rail" concept — power
flags are just specific labels by convention. The translator
recognizes common power-flag symbol names:

- `+5V`, `+3V3`, `+1V8`, `+12V`, `VBUS`, `VBAT`, `VCC` → power nets
- `GND`, `GNDA`, `GNDD`, `AGND`, `DGND` → ground nets

These map to BHDL `power <name> = <auto>V @ <auto>A;` and
`ground <name>;` declarations. Voltage values are auto-detected
from the flag name (e.g. `+5V` → 5 V); current budgets are
auto-detected from connected load components or default to a
reasonable value.

### 4.7 Buses

KiCad supports buses like `DATA[7..0]` with `bus_entry` connections.
For v0.1: **flatten buses to individual nets** (`DATA0` through
`DATA7`). BHDL doesn't currently have first-class bus support;
keeping the translation simple by flattening avoids cascading
language extensions.

### 4.8 Net classes

KiCad allows assigning components to "net classes" with different
electrical/trace properties. For schematic translation, these
are metadata only and become BHDL `attribute net_class = "..."`
entries on the relevant components.

---

## 5. Phase-by-phase work breakdown

Each phase is independently committable and produces a
shippable artifact.

### 5.1 Phase A — KiCad reader infrastructure

**Goal:** Parse `.kicad_sch` files into an in-memory IR. No
BHDL emission yet; just faithful reading.

**Files created:**

- `bhdl-kicad-import/Cargo.toml` (new crate)
- `bhdl-kicad-import/src/lib.rs` — public API
- `bhdl-kicad-import/src/sexpr.rs` — S-expression lexer + parser
- `bhdl-kicad-import/src/ir/mod.rs` — typed IR (Sheet, Symbol,
  Wire, Junction, Label, Sheet, etc.)
- `bhdl-kicad-import/src/reader.rs` — IR builder
- `bhdl-kicad-import/tests/fixtures/` — small `.kicad_sch`
  files for unit testing

**Public API:**

```rust
pub fn read_schematic(path: &Path) -> Result<Schematic, ImportError>;

pub struct Schematic {
    pub root: Sheet,
    pub child_sheets: HashMap<PathBuf, Sheet>,
}

pub struct Sheet {
    pub symbols: Vec<SchematicSymbol>,
    pub wires: Vec<Wire>,
    pub labels: Vec<Label>,
    pub power_flags: Vec<PowerFlag>,
    pub junctions: Vec<Junction>,
    pub no_connects: Vec<NoConnect>,
    pub sheet_refs: Vec<SheetRef>,
}

// ... typed IR for each construct ...
```

**Tests:**

- Parse a 1-component schematic (just a resistor and a power
  flag)
- Parse a hierarchical 2-sheet schematic
- Parse a multi-unit-IC schematic
- Round-trip: parse → re-emit (debug-format) → diff is zero
- Error cases: malformed S-expr, unknown KiCad version

**Effort:** 1 week (S-expression parsing is bounded; IR
modelling is straightforward; tests are quick).

**Success criterion:** Can parse the Arduino Uno R3
`uno.kicad_sch` (assuming KiCad 6+ port exists) without
errors. The IR fully captures every symbol, wire, label.

---

### 5.2 Phase B — Symbol library resolution

**Goal:** Resolve `Library:SymbolName` references to the actual
KiCad symbol library entries (pin lists, electrical types).

**Files created/modified:**

- `bhdl-kicad-import/src/lib_resolver.rs` — symbol library
  table parser + lookup
- `bhdl-kicad-import/src/lib_symbol.rs` — typed library symbol
  representation (pins, units, etc.)
- `bhdl-stdlib/kicad-symbol-mapping.toml` — the BHDL stdlib's
  KiCad symbol mapping (initial: ~50 common parts)

**Algorithm:**

1. Read `sym-lib-table` (TOML-ish format).
2. For each `Library:Symbol` reference in the schematic:
   a. Locate the library file.
   b. Parse `.kicad_sym` (S-expr format, similar to
      `.kicad_sch` but simpler — just symbol definitions).
   c. Extract pin list, units, electrical types.
3. Build a `HashMap<LibraryRef, LibSymbol>` of all referenced
   library symbols.

**Tests:**

- Resolve a few common symbols (Device:R, Device:C, Device:LED)
- Resolve a multi-unit symbol (Amplifier_Operational:LM358)
- Handle missing library (gracefully error)
- Handle embedded library (KiCad supports caching project-
  specific libraries inline)

**Effort:** 3-4 days.

**Success criterion:** Every symbol in Arduino Uno's schematic
resolves to a real KiCad library entry, with pin lists matching
what KiCad's own ERC would see.

---

### 5.3 Phase C — Net topology extraction

**Goal:** Given the parsed schematic + library symbols, compute
the list of distinct nets and which pins connect to each.

**Files created:**

- `bhdl-kicad-import/src/net_extraction.rs` — net graph
  construction
- `bhdl-kicad-import/src/net_topology.rs` — typed Net structure

**Algorithm:**

1. **Build endpoint graph.** Every pin on every symbol has a
   physical position (`at x y`). Every wire is a segment with
   two endpoints. Every label/power-flag is attached to a
   specific endpoint.
2. **Union-find on endpoints.** Wires + junctions merge
   endpoints into equivalence classes. Each class is a net.
3. **Assign names.** Within each class, the highest-priority
   label wins. Priority: power-flag > global-label >
   hierarchical-label > local-label > auto-name.
4. **Cross-sheet propagation.** Hierarchical labels in child
   sheets connect to the parent's sheet-pins on the matching
   `sheet` instance.
5. **Output.** A `Vec<Net>` where each net carries its name,
   class (power/ground/signal), and the list of
   `(symbol_ref, pin_number)` it connects.

**Tests:**

- Two resistors in series, junction in middle: one net for the
  middle, one for each end
- Bus-flattened net: `DATA[7..0]` becomes 8 separate nets
- Hierarchical: a sheet pin connects on parent to a
  hierarchical label on child
- Power flag merging: two `+5V` flags on different sheets =
  one global net

**Effort:** 1 week (graph algorithms are bounded; edge cases
around hierarchical labels need careful testing).

**Success criterion:** Net count and per-net pin lists for
Arduino Uno match KiCad's own ERC netlist export exactly.

---

### 5.4 Phase D — BHDL emitter

**Goal:** Given the resolved schematic IR + net list, emit a
valid `.bhdl` file.

**Files created:**

- `bhdl-kicad-import/src/emitter/mod.rs` — top-level emission
- `bhdl-kicad-import/src/emitter/entities.rs` — entity stub
  generation for unknown symbols
- `bhdl-kicad-import/src/emitter/board.rs` — board declaration
  generation
- `bhdl-kicad-import/src/emitter/connections.rs` — flow-syntax
  emission for nets
- `bhdl-kicad-import/src/emitter/imports.rs` — `import { ... }`
  statement generation
- `bhdl-kicad-import/src/symbol_mapping.rs` — read & maintain
  the symbol mapping registry

**Algorithm:**

1. Load `kicad-symbol-mapping.toml`.
2. For each schematic symbol, look up the BHDL entity name.
   If not in the mapping, auto-generate a stub entity and add
   a registry entry.
3. Emit the `.bhdl` file:
   a. Imports (every used entity)
   b. `board <Name> { ... }`
   c. Power and ground declarations from power flags
   d. Component instantiations (preserving reference designators)
   e. Net connections in flow syntax
4. For hierarchical sheets, emit child entities first (with
   `expansion { }` blocks), then the top board.
5. Output formatting: stable sort, canonical numeric formatting,
   2-space indent, trailing semicolons. Determinism guaranteed.

**Auto-generation strategy:**

When a symbol has no mapping, emit something like:

```bhdl
// auto-generated stub from KiCad symbol "MCU_ST_STM32F4:STM32F411RETx"
// review needed: add SKU attrs, behavior model, design recipes
entity STM32F411RETx_AutoGen() {
    pin VBAT:  power in;
    pin VDD:   power in;
    pin VSS:   ground inout;
    pin PA0:   signal inout;
    // ... all pins from KiCad symbol ...

    attribute __auto_generated = "true";
    attribute __kicad_library = "MCU_ST_STM32F4";
    attribute __kicad_symbol = "STM32F411RETx";
    attribute kicad_symbol = "MCU_ST_STM32F4:STM32F411RETx";
    attribute footprint = "Package_QFP:LQFP-64_10x10mm_P0.5mm";

    // TODO: enrich with SKU, behavior, design recipes
}
```

The `__auto_generated` attribute lets the BOM walker print a
"this part needs review" warning.

**Tests:**

- Emit a board with two resistors and a power supply — verify
  syntactic validity (re-parses)
- Emit a board with a hierarchical sheet — verify child entity
  emission
- Emit a board with an unknown symbol — verify stub generation
- Emit a board with DNP'd parts — verify variant block

**Effort:** 1 week.

**Success criterion:** Arduino Uno emits a `.bhdl` file that
parses successfully through the BHDL parser.

---

### 5.5 Phase E — Netlist equivalence checker

**Goal:** Compare a KiCad netlist export with a BHDL synthesis
netlist; report discrepancies.

**Files created:**

- `bhdl-netlist-compare/Cargo.toml` (new crate)
- `bhdl-netlist-compare/src/lib.rs` — public API
- `bhdl-netlist-compare/src/kicad_netlist.rs` — read KiCad's
  netlist export (multiple formats supported: KiCadXML,
  OrCAD, etc.)
- `bhdl-netlist-compare/src/bhdl_netlist.rs` — read BHDL
  synthesis output (the netlist already serializes to JSON)
- `bhdl-netlist-compare/src/canonicalize.rs` — name
  normalization (KiCad's `Net-(R1-Pad1)` ↔ BHDL's anonymous
  net naming)
- `bhdl-netlist-compare/src/diff.rs` — structured diff output

**Algorithm:**

1. Read both netlists into a common IR: `HashMap<ComponentRef,
   PinAssignment>`.
2. Canonicalize:
   - Net names (both sides use the same priority: power-flag
     name > label > auto-name)
   - Reference designators (both should use original Refs)
   - Pin numbers (both libraries should agree)
3. Compute three diffs:
   - **Components missing on one side**
   - **Components with different attributes** (value, footprint)
   - **Pins on different nets between the two**

Output as a structured Markdown report:

```
Netlist comparison: Arduino_Uno (KiCad) vs Arduino_Uno (BHDL)
============================================================

✓ Components: 30/30 match (R1..R10, C1..C12, U1, U2, ...)
✓ Power nets: 4/4 match (+5V, +3V3, VBUS, GND)
✗ 1 net mismatch:
   - Net "Net-(C7-Pad2)" in KiCad equals "Net-(R5-Pad2)" in BHDL —
     same topology, different anonymous names (canonicalize fix)
✓ Connection count: 87/87 match

Overall: NETLIST EQUIVALENT (1 cosmetic difference, 0 structural)
```

**Tests:**

- Both netlists identical → "fully equivalent"
- One missing component → reported
- Same components, one pin on a different net → reported
- Naming difference only → reported as cosmetic, not structural
- Two completely different boards → many violations, clearly
  not the same

**Effort:** 1 week.

**Success criterion:** For Arduino Uno, the BHDL output's
netlist matches KiCad's netlist export with zero structural
violations.

---

### 5.6 Phase F — First board: Arduino Uno R3

**Goal:** Run the full pipeline end-to-end on Arduino Uno.
Iterate until verified equivalent.

**Source:** Arduino Uno R3 schematic from Arduino's official
GitHub (or a community port to KiCad 6+).

**Files created:**

- `tests/circuits/oshw/arduino_uno_r3/arduino_uno_r3.bhdl`
- `tests/circuits/oshw/arduino_uno_r3/source/arduino_uno_r3.kicad_sch`
- `tests/circuits/oshw/arduino_uno_r3/expected_netlist.json`
- `bhdl-stdlib/auto/...` — auto-generated entity stubs for any
  Uno parts we don't already have
- `bhdl-stdlib/kicad-symbol-mapping.toml` — additions for
  Uno's symbols

**Tasks:**

1. Import the schematic with `bhdl-cli import-kicad`.
2. Run the netlist comparator.
3. Fix discrepancies (probably translator bugs in the first
   few iterations).
4. Once equivalent, hand-enrich the auto-generated stubs:
   - Add SKU attributes (manufacturer, MPN, distributor PNs)
     from Arduino's published BOM
   - Verify footprints
   - Add `kicad_symbol` and `kicad_footprint` references
5. Commit the verified `.bhdl` + enriched stdlib.
6. Document the process in `docs/walkthroughs/arduino_uno.md`.

**Effort:** 1-2 weeks (first board always takes longest
because of unforeseen issues; subsequent boards will be faster).

**Success criteria:**

- `bhdl-cli synthesize` runs end-to-end without errors.
- Netlist comparison shows full equivalence with the original
  KiCad netlist.
- BOM output (`bhdl-cli bom`) covers all populated parts with
  SKU data matching Arduino's published BOM.
- The committed .bhdl file parses, synthesizes, and produces
  equivalent output deterministically.

---

### 5.7 Phase G — Stdlib enrichment for Uno parts

**Goal:** Take the auto-generated stubs and hand-enrich them.

**Stdlib parts likely needing enrichment** (from Arduino Uno's
BOM):

- `ATmega328P` (MCU) — pins are auto-generated; we need SKU
  data, a behavioral model (for future testbenches), maybe an
  IBIS model
- `ATmega16U2` (USB-to-serial MCU) — same
- `AMS1117-5V` (LDO) — SKU + behavioral model with PG sequencing
- USB-B connector — SKU + footprint
- Crystal oscillator (16 MHz) — SKU + footprint + behavioral
  (clock source for testbenches)
- Reset button — SKU + footprint
- Various passives — SKU lookups from LCSC

**Per-part enrichment template:**

```bhdl
entity ATmega328P_TQFP32() {
    // Pins (already in stub)
    pin VCC: power in;
    pin GND: ground inout;
    pin AVCC: power in;
    // ... PB0..PB7, PC0..PC6, PD0..PD7 ...

    // Identity
    attribute component_class = "mcu";
    attribute manufacturer = "Microchip";
    attribute mpn = "ATMEGA328P-AU";
    attribute physical_package = "TQFP-32";
    attribute footprint = "Package_QFP:TQFP-32_7x7mm_P0.8mm";
    attribute kicad_symbol = "MCU_Microchip_ATmega:ATmega328P-AU";
    attribute datasheet = "https://...";
    attribute digikey_pn = "ATMEGA328P-AU-ND";
    attribute lcsc_pn = "C14877";

    // Future: behavior block with power-on reset timing,
    // GPIO drive characteristics, ...
}
```

**Effort:** ~30 minutes per part × ~10 unique parts = ~5 hours
focused work for Uno's full enrichment.

**Success criteria:**

- Every Uno part has full SKU data
- BOM output is order-ready (could be uploaded to LCSC/JLCPCB
  for assembly)
- The `tests/circuits/oshw/arduino_uno_r3/arduino_uno_r3.bhdl`
  re-runs and produces identical netlist + enriched BOM

---

### 5.8 Phase H — Adafruit Feather M0

**Goal:** Translate a more complex modern board; add behavioral
modeling for the first time on a real translated board.

**Source:** Adafruit Feather M0 Basic Proto schematic (from
Adafruit's GitHub).

**New scope:**

- USB-C connector (vs USB-B on Uno) — different footprint
- Lipo battery charge management (MCP73831 or BQ24074) — first
  real behavioral model (charge state machine)
- SAMD21 MCU (vs ATmega328P) — different pin counts and
  architecture
- AP2112 3V3 LDO — behavioral model with sequencing
- USB-to-serial bridge (sometimes via SAMD21 native USB, no
  separate FT chip)

**First behavioral testbench:**

```bhdl
testbench PowerOnSequence for Feather_M0 {
    apply USB_VBUS = step(5V, at 1ms);
    apply VBAT = 0V;  // no battery

    expect VDD_3V3 settles_to(3.3V) within 50ms after USB_VBUS > 4.5;
    expect SAMD21.RESET deasserts_within 50ms after VDD_3V3 reaches 90%;

    forbid SAMD21.VDD > 3.6V at_any_time;
    forbid backdrive_on(USB_VBUS) during initial 100ms;
}
```

**Effort:** 2-3 weeks. Phase H is where we first exercise the
behavioral spec (B2-B7 from `Behavioral_Models.md`). Behavioral
implementation work happens in parallel with this phase.

**Success criteria:**

- Netlist equivalence vs KiCad output
- Full BOM with order-ready SKUs
- Power-on testbench runs and assertions pass
- First demo of "BHDL caught a power-sequencing bug at simulation
  time" — even if just contrived

---

### 5.9 Phase I — Continued board coverage

**Goal:** Accrete the stdlib via more boards. Each board takes
1-2 weeks.

**Candidates in rough order:**

1. **Raspberry Pi Pico** — popular RP2040 board, simple
2. **STM32 Black Pill** — popular bare-metal ARM
3. **Adafruit QT Py** — tiny SAMD board, good for
   minimal-footprint exercises
4. **ESP32-WROOM-32 module breakout** — WiFi MCU, more power
   complexity
5. **Open-source dev tool** (Bus Pirate, Logic analyzer, etc.) —
   non-MCU board for variety
6. **Audio amp** (continuation of the triode lineage) — adds
   transformers, sockets, mechanical considerations

**Per-board pattern (now well-understood):**

1. Translate (1 day with infrastructure in place)
2. Fix discrepancies (~half day)
3. Enrich stdlib stubs (~half day)
4. Write a behavioral testbench (~half day)
5. Document the walkthrough (~half day)
6. Commit

**Effort:** 1-2 weeks per board, depending on complexity.

**Success criteria:**

- Each board added covers some new architectural surface (new
  IC class, new variant pattern, new behavioral model class)
- Stdlib coverage measured at each point — "what % of common
  hobbyist parts are now in stdlib"
- Demo set in `tests/circuits/oshw/` grows linearly

---

## 6. Test boards and what each one exercises

Summary table of what each candidate board specifically validates:

| Board | Architecture surface | New stdlib coverage |
|---|---|---|
| Arduino Uno R3 | Translator basics, power flags, hierarchical sheets (if used), AVR MCU | ATmega328P, FT232 or 16U2, AMS1117, USB-B, common passives |
| Adafruit Feather M0 | USB-C, lipo charging, behavioral models, sequencing testbench | SAMD21, USB-C, charge ICs, ESD diodes, modern passives |
| Raspberry Pi Pico | RP2040, flash chip, debug interface | RP2040, W25Q16 flash, USB micro-B |
| STM32 Black Pill | STM32F4 family, crystal, USB-C | STM32F411, HSE crystal |
| ESP32 dev board | Wireless MCU, complex power sequencing, antennas | ESP32-WROOM, antenna footprint references, complex power |
| ICE40 dev board | FPGA, configuration flash, SPI bus | ICE40 FPGA family, SPI flash |
| Bus Pirate / Logic analyzer | Multi-protocol I/O, level shifting | Level shifters, voltage references |

Boards aren't sequential — they're parallel candidates, chosen
based on what feature the next iteration most wants to exercise.

---

## 7. Stdlib growth model

### 7.1 What "stdlib" means

The BHDL stdlib at `bhdl-stdlib/` is a collection of BHDL entity
declarations for real-world parts, organized by category:

```
bhdl-stdlib/
├── passive/                    # R, C, L, ferrite beads, ...
├── actives/                    # transistors, diodes, tubes, ...
├── ics/
│   ├── mcus/                   # MCU entities
│   ├── regulators/             # LDO, switching regulators
│   ├── opamps/                 # op-amps, comparators
│   ├── interface/              # USB, UART, I2C bridges
│   ├── memory/                 # flash, EEPROM, RAM
│   └── ...
├── connectors/                 # USB, header, socket
├── crystals/                   # oscillators, crystals
├── auto/                       # auto-generated stubs (review pending)
└── kicad-symbol-mapping.toml   # symbol → entity registry
```

### 7.2 Growth driven by boards, not catalog mining

Each board landed adds ~20-50 stdlib entries (most auto-
generated initially, hand-enriched as the board demo demands).
After ~10-15 boards, the stdlib covers most parts a typical
hobbyist board needs (~500-1000 entries).

### 7.3 Quality tiers

Entities exist at multiple quality tiers:

| Tier | Marker | Capabilities |
|---|---|---|
| **Stub** | `attribute __auto_generated = "true"` | Pins + value parse; usable for netlist; missing SKU, behavior, recipes |
| **SKU-complete** | `attribute __review_status = "sku_complete"` | Full SKU data (mfr, MPN, distributor PNs); ready for order-ready BOM |
| **Behavioral** | `attribute __review_status = "behavioral"` | SKU + `behavior { }` block for simulation |
| **Production** | `attribute __review_status = "production"` | All of the above + `design { }` recipes + footprint validated + datasheet linked |

The mapping registry tracks tier. CI checks for "newly stub-tier
entries; please review" can flag PRs that introduce or leave
stubs.

### 7.4 The `bhdl-parts` registry vision

Long-term, the stdlib accreted in this conversation becomes
the seed of a public `bhdl-parts` registry — a separate repo
or a directory in this one — that vendors and users can
contribute to. Discussed in
`Product_Description_Model.md` §8.

---

## 8. CLI surface

Three new subcommands land in the CLI alongside the existing
ones (parse, analyze, synthesize, spice, bom, list-skus, etc.):

### 8.1 `import-kicad`

```
bhdl-cli import-kicad <path-to-kicad-project>
                      [-o <output.bhdl>]
                      [--include-pcb]              # future: PCB layout
                      [--variant <Name>]           # extract variant info
                      [--stdlib-mapping <toml>]    # custom mapping registry
                      [--auto-stub-dir <path>]     # where stubs go (default: bhdl-stdlib/auto)
                      [--report <path>]            # write import report
```

Default behavior: reads the KiCad project, emits BHDL to stdout
(or to `<output.bhdl>` if -o), writes any auto-generated stubs
to `bhdl-stdlib/auto/`, updates `kicad-symbol-mapping.toml`.

### 8.2 `compare-netlist`

```
bhdl-cli compare-netlist <a.kicad_sch> <b.bhdl>
                         [--format markdown|json]
                         [--ignore cosmetic|none]
                         [-o <report.md>]
```

Runs the netlist equivalence checker. Returns exit code 0 if
structurally equivalent, 1 if not.

### 8.3 `import-ibis` (future, paired with the behavioral spec)

```
bhdl-cli import-ibis <file.ibs>
                     [-o <output.bhdl>]
                     [--component-class ic_buffer]
```

Same shape but for IBIS files. Detailed in
`Behavioral_Models.md` §10.2. Not part of THIS plan's phases
A–I, but mentioned because the architecture is shared.

---

## 9. Risks and unknowns

### 9.1 KiCad 6 vs 7 vs 8 format differences

Newer KiCad versions occasionally add fields or change the
S-expression schema. Mitigation: target 6.0 as baseline, test
on 7.x and 8.x corpus, write conditional logic where versions
diverge. Unknown: how often this'll bite. Estimated: occasionally,
not constantly.

### 9.2 Custom KiCad libraries

Many real OSHW projects use custom (non-stdlib) KiCad symbol
libraries. The translator must handle libraries declared in
`sym-lib-table` pointing to project-local `.kicad_sym` files.
This is well-defined in KiCad's format; just need to implement.

### 9.3 Reference vs assembly variant data

Some boards' variant information is in the KiCad project file
(`.kicad_pro`) rather than the schematic. Others use external
files for assembly variants. Mitigation: support the
in-schematic form first; extend to project-file form when a
board demands it.

### 9.4 Net-naming canonical form

KiCad's anonymous net names (`Net-(R1-Pad2)`) and BHDL's
auto-generated names will likely differ. The equivalence
checker must canonicalize. Risk: if canonicalization differs
between revisions, false-positive mismatches.

Mitigation: write canonicalizer to produce deterministic
topology-based names for both sides; pin pin names by
component-instance position rather than KiCad's internal
ordering.

### 9.5 Multi-unit / shared-pin components

Op-amp packages with 4 channels sharing 2 power pins — the
translator gathers all units. Risk: misassigning pins between
units. Mitigation: explicit per-unit pin lists in the library
symbol; test with at least one quad-opamp board.

### 9.6 Auto-generated stub completeness

For unknown IC symbols, the stub has the right pins but no
electrical semantics. A board with 5 unknown ICs becomes mostly
hollow until enrichment. Risk: demo boards look incomplete.
Mitigation: prioritize enrichment of ICs that appear in the
first few boards; document the "stub" state clearly.

### 9.7 PCB layout not translated

KiCad has rich PCB data (placement, routing, copper pours) that
BHDL's layout block doesn't yet capture. Phase A–I does NOT
translate this. Risk: someone expects "full KiCad import" and
finds layout missing. Mitigation: clear scope statement in
docs; future work item.

### 9.8 Hierarchical schematic edge cases

Hierarchical labels can be the same name in different sheets —
KiCad uses sheet-path scoping. The translator must handle the
scoping correctly. Risk: nets get incorrectly merged across
sheets. Mitigation: thorough hierarchical-sheet tests; net
unification logic that respects sheet boundaries.

### 9.9 Symbol library churn

KiCad's standard libraries evolve: `Device:R` was once
`Device:Resistor`, etc. Real OSHW boards may use old names.
Mitigation: mapping registry covers historical aliases (e.g.
both `Device:R` and `Device:Resistor` map to BHDL `Res`).

### 9.10 Performance with large boards

A 500-component board has potentially 1000s of pins, 100s of
nets. The translator's net-extraction (union-find on
endpoints) should be fast (O(n α(n))) but real performance
depends on implementation details. Mitigation: not premature
optimisation; profile after Phase F if it's an issue.

---

## 10. Success criteria per phase

| Phase | Hard success criterion |
|---|---|
| A — Reader | Parses Arduino Uno schematic into IR without errors |
| B — Library resolution | Every symbol in Uno's schematic resolves to a pin list matching KiCad's own ERC view |
| C — Net topology | Net count + per-net pin lists match KiCad's netlist export for Uno |
| D — Emitter | Uno emits a valid `.bhdl` that re-parses cleanly |
| E — Comparator | For Uno, comparator reports "STRUCTURALLY EQUIVALENT, 0 violations" |
| F — Uno end-to-end | `tests/circuits/oshw/arduino_uno_r3.bhdl` synthesizes + produces BOM matching Arduino's official BOM |
| G — Uno enrichment | All Uno parts at SKU-complete tier with verified manufacturer + MPN |
| H — Feather M0 + behavioral | Translated + power-on testbench runs + assertions resolve |
| I — Continued | Each new board adds verified equivalence + new stdlib entries |

---

## 11. Effort and sequencing summary

| Phase | Effort | Notes |
|---|---|---|
| A — Reader | 1 week | Bounded engineering |
| B — Library resolution | 3-4 days | |
| C — Net topology | 1 week | Most algorithmically interesting |
| D — Emitter | 1 week | |
| E — Comparator | 1 week | Validation oracle |
| F — Uno | 1-2 weeks | First board always longest |
| G — Uno enrichment | <1 week | Manual but bounded |
| H — Feather M0 | 2-3 weeks | First behavioral work in parallel |
| I — Continued boards | 1-2 weeks each | Cadence dependent on demand |

**Total effort to "Arduino Uno fully translated, enriched, and
demoable" (Phases A–G): ~6-7 weeks of focused work.**

**Total effort to "Feather M0 with behavioral testbench
demoable" (through Phase H): ~10-11 weeks**, partially
overlapping with the behavioral spec's implementation phases
B2-B7.

After Phase H, the platform has:

- A working KiCad importer
- Two well-known boards translated, verified, and enriched
- A growing stdlib of real-world parts
- A working behavioral simulator with at least one production
  testbench
- A reproducible methodology for adding more boards

That's the deliverable that turns BHDL from "interesting spec
documents" into "real platform with working demos and a
credible parts catalog."

---

## 12. Commit cadence and integration with existing work

### 12.1 What lands first

Phases A–E are translator infrastructure. They can land before
any behavioral work happens. They unblock Phase F (Uno) and
beyond.

### 12.2 What integrates with behavioral spec

Phase H (Feather M0 + power-on testbench) is the first phase
that depends on `Behavioral_Models.md` implementation. The
behavioral spec's phases B2–B7 should land in parallel with
Phase H or just before:

```
KiCad import:    A → B → C → D → E → F → G → H
                                            ↑
Behavioral:                  B2 → B3 → B4 → B5 → B6 → B7
                                                     ↑
                                            both unblock H
```

### 12.3 Suggested commit sequence (concrete)

1. New crate `bhdl-kicad-import` with Phase A (S-expression
   parser + IR)
2. Phase B (symbol library resolution) lands inside the same
   crate
3. Phase C (net topology) lands
4. Phase D (emitter) + initial `bhdl-cli import-kicad`
   subcommand
5. New crate `bhdl-netlist-compare` with Phase E
6. `bhdl-cli compare-netlist` subcommand
7. Phase F: Arduino Uno R3 translated, verified, committed
   under `tests/circuits/oshw/`. Stdlib stubs in
   `bhdl-stdlib/auto/`.
8. Phase G: enriched stdlib entries land
9. (interleaved) Behavioral spec phases B2-B7 start landing
10. Phase H: Feather M0 with behavioral testbench
11. Phase I: continued accretion

---

## 13. Open questions before code starts

These are real decisions to make in the planning step that
might change implementation:

### 13.1 Where does the symbol-mapping registry live?

Options:
- In the BHDL repo (`bhdl-stdlib/kicad-symbol-mapping.toml`) —
  shared across all users
- Per-project (in the user's project directory) — local
  customization
- Both — global mapping with per-project overrides

**Decision needed:** start with repo-global; add per-project
override path if a real board needs it.

### 13.2 How aggressive is auto-stub-generation?

Options:
- **Conservative** — fail loudly on unknown symbols; force the
  user to add a mapping
- **Permissive** — silently auto-generate; user reviews later

**Decision needed:** permissive for translator UX, but mark
clearly with `__auto_generated = "true"` and `__review_status
= "stub"` so review pressure isn't lost.

### 13.3 What about PCB import?

Phases A-I don't translate PCBs. But every translated schematic
has matching PCB data in `.kicad_pcb`. Should we:

- Defer entirely (current plan)
- Read PCB metadata only (XY positions for placement hints)
- Translate PCB → BHDL layout block (huge scope)

**Decision needed:** defer fully for v0.1; revisit after Phase
G when layout-block work is needed.

### 13.4 How do we handle KiCad project variants (assembly
variants)?

KiCad 7+ has built-in assembly variant support. Older boards
sometimes use field properties for the same purpose. Two
different conventions to translate.

**Decision needed:** support modern variant format first; extend
to field-property form when an older OSHW board demands it.

### 13.5 LCSC importer — when does it land?

The plan focuses on KiCad. Once we have ~10 boards landed, the
parts-naming structure is well-understood enough that an LCSC
importer (for filling in SKU details quickly) becomes natural.
But LCSC importer could also accelerate Phase G enrichment
work.

**Decision needed:** LCSC importer is its own track; can run
in parallel with KiCad work. Worth a separate planning
document.

### 13.6 KiCad version coverage

KiCad 6 → 8 are largely compatible. KiCad 9 (current dev) and
beyond may diverge. How long do we maintain 6 support?

**Decision needed:** start with 6+; revisit if 6 retains
significant share in 2027+ OSHW community.

---

## 14. Summary

This plan converts the "translate then enrich" strategy into a
concrete sequenced implementation. Every phase is independently
committable, every decision is named, every risk is identified.

The path from "current state" to "first demoable open-hardware
board fully translated + enriched + simulable" is:

- Phases A–E (~5 weeks): translator infrastructure
- Phase F (~1-2 weeks): Arduino Uno end-to-end
- Phase G (~1 week): Uno stdlib enrichment
- Phase H (~2-3 weeks): Feather M0 + first behavioral
  testbench (paired with behavioral implementation phases
  B2-B7 from `Behavioral_Models.md`)

Total: ~10-11 weeks of focused work to a credible demo.
After that, each additional board takes 1-2 weeks and grows
the stdlib by 10-30 parts.

The deliverable: a platform that **demonstrably translates
real open-hardware boards, validates netlist equivalence
against KiCad, accretes a real parts library, and adds
capabilities (behavioral simulation, variants, design
recipes) the original KiCad designs don't have** — all from a
single source-of-truth `.bhdl` file per board.
