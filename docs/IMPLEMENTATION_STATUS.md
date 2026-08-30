# BHDL Implementation Status

## Overview

This document tracks the implementation status of BHDL (Board Hardware Description Language) - a complete toolchain for electronic circuit design, analysis, and manufacturing.

> **Status note (refreshed 2026-08-30):** the full
> source→analysis→netlist→synthesis→GLACIER DC solve→sign-off→BOM flow runs
> end-to-end (`bhdl-cli … bom --simulate`), and since the 2026-06 refresh the
> default pipeline has grown a front half: every netlist-consuming command
> runs **bhdl → elaborate → synthesize** with a round-trip gate that proves
> the elaborated structural form re-synthesizes identically
> (`--no-elaborate` skips the gate, loudly). Also live today: the ERC
> catalog through ERC037, functional-safety modeling (`safety`/`trace`),
> power-tree planning + power-up/down simulation
> (`powertree`/`powerup`/`powerdown`/`pdreport`), PnR + fab export
> (`layout --fab`), and the schematic V4 engine. The frozen test counts and
> performance/cost numbers that used to live in this file were snapshots of
> a 2025 pass; the live gate is `cargo test --workspace`.

## Architecture Overview

```
BHDL Source Code
      ↓
   Parser            (bhdl-parser: rowan CST)
      ↓
   AST + Analysis    (bhdl-ast, bhdl-analyzer: multi-pass; recipes, attrs, power)
      ↓
   Elaboration       (default pipeline front half: desugar to structural bhdl,
      ↓               round-trip gate proves re-synthesis identity; --no-elaborate)
   Synthesis         (bhdl-synthesizer: analysis → structural netlist;
      ↓               design/expansion/simulation recipes; value snapping;
      ↓               requirement/stage resolution; refdes allocation at
      ↓               phase 12.7, persisted in <board>.bhdl.refdes;
      ↓               ERC — rules through ERC037 across three tiers)
   Netlist           (bhdl-netlist; `freeze` emits the immutable as-fabbed
      ↓               record; bhdl-safety + `safety`/`trace` build the
      ↓               functional-safety model and requirement trace matrix)
   ┌─────────────────┴───────────────────────────────┐
   ↓                                                   ↓
   Component Intelligence          Circuit Simulation & Sign-off
   (bhdl-components: DB +           (bhdl-spice: netlist→SPICE circuit;
    supplier APIs + synthesis;       GLACIER DC solver; Real-Data margin
    bhdl-{digikey,jlcparts}-          sign-off; vendor simulation blocks)
    provider: live ESR/stock)
      ↓                                                   ↓
   BOM (bhdl-cli `bom` / `bom --simulate`) ←─────────────┘
      ↓
   Layout & Visualization   (bhdl-pnr, bhdl-schematic — present, not re-verified)
```

## Phase 1: BHDL Foundation ✅ **COMPLETE**

### Core Language Implementation
- **BHDL Parser**: Full parsing of the v2.0 flow syntax with `rowan` CST
- **AST Generation**: Typed abstract syntax tree with semantic wrappers
- **Multi-pass Analysis**: 4-pass semantic analysis pipeline
- **Error Recovery**: Robust error handling and diagnostic reporting
- **Unit System**: Comprehensive electrical units (V, A, Ω, F, H, Hz, etc.)

### Language Features Implemented
- ✅ Hierarchical design (boards, entities, components)
- ✅ Parameter system with electrical units
- ✅ Type system (signal, power, ground with specifications)
- ✅ Component definitions with physical packages
- ✅ Connection statements and net declarations
- ✅ Interface definitions for modular design

### Codebase Structure
- `bhdl-parser/` - Lexical analysis and syntax parsing
- `bhdl-ast/` - AST generation and semantic wrappers
- `bhdl-analyzer/` - Multi-pass semantic analysis

## Phase 2: Circuit Intelligence ✅ **COMPLETE**

### Netlist Generation
- **Netlist Data Model**: Complete structural representation
- **Entity Definitions**: Hierarchical entity system with instances
- **Connection Resolution**: Net connectivity analysis
- **Type-safe IDs**: `slotmap`-based component/net identification
- **Serialization**: JSON export for downstream tools

### Advanced Analysis
- **Connectivity Analysis**: Complete net resolution and validation
- **Hierarchy Flattening**: Top-level design expansion
- **Type Checking**: Signal/power/ground type validation
- **Parameter Resolution**: Constant evaluation and propagation

### Codebase Structure  
- `bhdl-netlist/` - Structural circuit representation
- Integration with analyzer for semantic-to-structural conversion

## Phase 3: Component Intelligence ✅ **COMPLETE**

### 🗄️ Component Database System
- **SQLite Backend**: FTS5 full-text search, structured storage
- **Electrical Specifications**: Proper units (Ω, F, V, W) with tolerances
- **Component Categories**: Resistors, capacitors, ICs, connectors, etc.
- **Database Migrations**: Automatic schema versioning (v0 → v2)
- **KiCad Integration**: Symbol library import and parsing

### 🔌 Multi-Supplier API Integration
- **DigiKey API**: Complete REST integration with OAuth2 authentication
- **Nexar API**: GraphQL-based Octopart supplier data access
- **Rate Limiting**: Token bucket algorithm (8/min DigiKey, 5/min Nexar)
- **Multi-level Caching**: Memory LRU → SQLite persistent → API fallback
  (the old "90% API call reduction" figure is a historical, unverified claim)

### ⚙️ Two-Stage Component Synthesis
- **Stage 1**: Local database fuzzy matching with electrical constraints
- **Stage 2**: Selective real-time supplier API calls for optimization
- **Requirements Engine**: Electrical, cost, availability, package constraints
- **Context-Aware Selection**: Application type and criticality levels

### 🎯 Advanced Optimization
- **Cost Optimization**: Volume pricing analysis (10 → 10,000 units)
- **Alternative Selection**: Intelligent component ranking and substitution
- **Package Constraints**: SMD/THT preferences (0402, 0603, 0805, etc.)
- **Supply Chain Risk**: Multi-supplier sourcing and availability analysis

### 📊 Performance (historical, unverified)
> The cache-hit ratios, millisecond latencies, and dollar-savings figures
> that used to be listed here were one-off measurements from the 2025
> Phase-3 pass and have not been re-measured since. They are not current
> claims and should not be quoted; re-measure before relying on any of them.

### 🧪 Testing & Validation
- **Live gate**: `cargo test --workspace` is the authoritative test status —
  this file no longer carries frozen per-module counts (the old "44/44"
  snapshot predated most of the current toolchain).
- **Integration Tests**: Real KiCad library import and API validation
- **Real-World Testing**: Confirmed with actual DigiKey API responses

### Codebase Structure
- `bhdl-components/` - Complete component intelligence system
  - `src/database/` - SQLite backend with migrations
  - `src/supplier/` - Multi-backend API integration with caching  
  - `src/synthesis/` - Two-stage synthesis and optimization engine
  - `src/kicad/` - KiCad symbol library import and parsing (there is no
    separate `bhdl-kicad-import/` crate — KiCad import lives here)
  - `tests/integration/` - Real-world integration test suite

## Phase 4: Synthesis → Netlist ✅ **WORKING**

`bhdl-synthesizer` converts the analyzed design into a structural `Netlist`
and drives the value/part resolution loop. This is exercised on every
`bhdl-cli bom` / `bom --simulate` run.

- **Analysis → Netlist**: instances, nets, hierarchical connectivity, and
  semantic roles (power/regulator/passive class) are materialised.
- **Expansion recipes** (`expansion {}`): a vendor entity's support topology
  (L, C_in, C_out, divider, …) is instantiated from the entity, not hardcoded.
- **Design recipes** (`design for <intent> {}`): vendor-authored value sizing
  from electrical targets, evaluated by `design_evaluator` (with a sandboxed
  foreign-language body hook).
- **Value snapping** to E-series, attribute stamping onto instances,
  variant/SKU patches.
- **Elaborate round-trip gate**: the default pipeline elaborates the board to
  structural bhdl and proves it re-synthesizes to a structurally identical
  netlist before consumers see it (`--no-elaborate` opts out).
- **ERC**: three-tier rule engine (core rules through ERC037, part-carried
  `check {}` blocks, policy plugins) with severity gating and reasoned
  waivers.
- **Refdes allocation**: one deterministic allocator at synthesis phase 12.7,
  persisted in the `<board>.bhdl.refdes` sidecar; every consumer reads, none
  mints.
- **Requirement/stage resolution**: stage requirements, `resolve` overrides,
  and the `trace` matrix (VERIFIED / VIOLATED / UNVERIFIED / UNRESOLVED).
- **Freeze**: `bhdl <board> freeze` emits the immutable as-fabbed structural
  record stamped with the toolchain version + library lock.
- **Functional safety**: `safety` / `trace --safety` resolve `safety` model
  blocks against the synthesized board — gap reports, FMEDA package,
  safety-case capstone (bhdl-safety + bhdl-synthesizer).

## Phase 5: Circuit Simulation & Sign-off ✅ **WORKING**

`bhdl-spice` turns the netlist into a circuit and solves it; `bhdl-cli
bom --simulate` reports per-part margins.

- **GLACIER DC solver**: modified-nodal-analysis DC operating point; regulators
  decomposed from pin types (controlled output source + loss model).
- **Margin sign-off** (`signoff.rs`): each passive's stress (cap voltage,
  resistor power, inductor peak current) vs its derated catalogue rating;
  analytic switcher ripple model + Stage-C inductor value-stepping;
  control-loop stability (crossover + ESR-zero classification).
- **Real-Data Policy**: every value in the analysis/selection path is real
  (catalogue/datasheet/entity-declared) or loudly `UNCHECKED`/hard-error — no
  fabricated defaults. Enforcement worklist complete (sweeps 1–10).
- **Vendor `simulation {}` blocks** (Vendor_Simulation_Blocks.md): device IP
  authored on the entity, GLACIER/sign-off stay generic —
  - `stress {}` (§4): how a device stresses its support parts (ripple/peak
    current), overriding the hardcoded reference model.
  - `model {}` (§5): how a device stamps into the solve — `node VOUT source`
    overrides the output voltage; `node VIN draws` is a datasheet-specific
    efficiency loss model that supersedes the generic physics loss model.
  - Both work on inline *and* imported (stdlib) entities.

## Phase 6: Supply-Chain Providers ✅ **WORKING**

- `bhdl-digikey-provider` (DigiKey Product Information API v4, OAuth2,
  self-contained ureq+rustls): live per-MPN real ESR for electrolytic/
  tantalum/polymer caps, fed into sign-off stability.
- `bhdl-jlcparts-provider`: JLCPARTS catalogue (stock/price/MPN), the default
  selection backend for `bom --simulate`.
- Selections are pinned in `bhdl.lock` for reproducibility.

## Phase 7: Layout & Visualization ✅ **WORKING** (layout campaign parked)

- `bhdl-pnr` — placement + routing, KiCad `.kicad_pcb` export, and a direct
  Gerber/Excellon fab package (`layout --fab`); exit status is the sign-off
  verdict. The active layout-quality campaign is parked — current state and
  resume notes in `bhdl-pnr/docs/LAYOUT-STATUS.md`.
- `bhdl-schematic` — the V4 idiom-composition engine (`visualize`,
  `--svg-v4`, `--binder` print/PDF): deterministic Rust-computed SVG,
  residue-region absence ledger.

## Integration Status

### ✅ **Working Pipelines (verified)**
1. **BHDL Source → AST → Analysis** — multi-pass semantic processing.
2. **Analysis → Netlist (synthesis)** — structural representation with roles.
3. **Netlist → SPICE circuit → GLACIER DC solve → margin sign-off** — the
   `bom --simulate` flow.
4. **Component Requirements → Real Parts** — live supplier integration
   (DigiKey/JLCPARTS), pinned in `bhdl.lock`.
5. **Vendor extensibility** — `design`/`expansion`/`simulation` blocks authored
   on entities drive sizing, topology, and device-simulation IP.

### 🔄 **Not re-verified / remaining**
1. **Layout quality campaign** — PnR/schematic run end-to-end
   (`layout`/`visualize`), but the layout-quality campaign is parked; see
   `bhdl-pnr/docs/LAYOUT-STATUS.md`.
2. **§5 `builtin`/`vendor spice` model forms** — deferred by spec (only the
   primitive `node source/draws` composition is built).
3. **Data sourcing** — the standing Real-Data constraint: where the catalogue/
   datasheet lacks coverage (ceramic ESR, diode/LED Vf·Is, regulator
   rds_on/t_sw/i_q) the analysis degrades to `UNCHECKED` by design.

## File Structure

Workspace members (from the root `Cargo.toml`):

```
bhdl-new/
├── bhdl-parser/             # Lexing + rowan CST
├── bhdl-ast/                # Typed AST / semantic wrappers
├── bhdl-analyzer/           # Multi-pass analysis; recipe extraction
├── bhdl-common/             # Shared recipe/data types (design/stress/model/…)
├── bhdl-netlist/            # Structural circuit representation
├── bhdl-synthesizer/        # Analysis → Netlist; design/stress/model evaluators;
│                            #   ERC; refdes allocation; stage/requirement resolution
├── bhdl-spice/              # Netlist → SPICE; GLACIER DC solver; sign-off
├── bhdl-components/         # Component DB + supplier synthesis + KiCad import (src/kicad/)
├── bhdl-digikey-provider/   # Live DigiKey ESR/stock provider
├── bhdl-jlcparts-provider/  # JLCPARTS selection backend
├── bhdl-stdlib/             # Vendor entity library (.bhdl)
├── bhdl-pnr/                # Placement + routing; KiCad/Gerber export
├── bhdl-schematic/          # Schematic V4 idiom engine / SVG
├── bhdl-sim/ bhdl-simulation/ bhdl-testbench/  # transient/testbench sim
├── bhdl-safety/             # safety_goal / fault analysis
├── bhdl-cli/                # Command-line interface (24 subcommands, see below)
└── bhdl-lsp/                # Language server
```

Note: a `bhdl-core/` directory exists on disk but is **not** a workspace
member — it is an orphan not built by `cargo build`/`test`.

### CLI subcommands (`bhdl-cli <FILE> <command>`)

`parse`, `analyze`, `synthesize`, `elaborate`, `powerup`, `powerdown`,
`pdreport`, `powertree`, `freeze`, `trace`, `safety`, `visualize`, `spice`,
`mine-priors`, `transient`, `pipeline`, `simulate`, `intents`, `layout`,
`mech-import`, `doc`, `bom`, `report`, `list-skus` — plus the
board-independent `bhdl vendor <status|install …>` maintenance mode.
One-line semantics for each are in
[spec/BHDL_Complete_Specification.md](spec/BHDL_Complete_Specification.md)
§16; the ground truth is the `Commands` enum in `bhdl-cli/src/main.rs`.

## Technology Stack

- **Language**: Rust with comprehensive type safety
- **Parsing**: `rowan` for CST, custom AST with semantic analysis
- **Database**: SQLite with FTS5 for component search
- **APIs**: REST (DigiKey) and GraphQL (Nexar) integration
- **Caching**: Multi-tier with LRU memory + persistent storage
- **Testing**: `cargo test --workspace` (live gate) plus real-world integration validation
- **Visualization**: SVG generation with placement/routing algorithms

## Phase-3 Achievements (historical record, 2025)

- **Real DigiKey Integration**: Confirmed working with live component data
- **Alternative Selection**: Intelligent component substitution engine
- (The caching-percentage, dollar-savings, and "44/44 tests" figures that
  used to be repeated here were unverified snapshots — see the Performance
  and Testing notes above.)

## Next Steps

1. **🖼️ Layout campaign resume**: pick the parked layout-quality work back
   up from `bhdl-pnr/docs/LAYOUT-STATUS.md`.
2. **🔌 Data sourcing**: enrich catalogue/datasheet coverage (ceramic ESR,
   diode/LED Vf·Is, regulator rds_on/t_sw/i_q) so fewer analyses go
   `UNCHECKED` — the standing Real-Data unblocker.
3. **🧩 §5 vendor model forms**: `builtin <model>` and `vendor spice "…"`
   wrappers (deferred by spec) when a real vendor model needs adapting.
4. **🪣 Parallel-bank addressing** for `stress {}` child references (today a
   child name resolves to one instance; a cap *bank* needs a convention).