# BHDL Specification Documentation

The language reference and its subsystem specifications. All docs here are
implementation-grounded unless explicitly marked *Proposal* or *aspirational*.

## Start here

- **[BHDL_Complete_Specification.md](BHDL_Complete_Specification.md)** — the
  language reference and the map to every subsystem doc below. Grounded in the
  parser grammar and the working corpus (`tests/circuits/realistic/`,
  `bhdl-stdlib/`).

## Subsystem specifications (current)

| Topic | Document | Status |
|-------|----------|--------|
| Data-honesty doctrine | [Real_Data_Policy.md](Real_Data_Policy.md) | binding policy |
| Real-Data enforcement audit | [Real_Data_Enforcement_Worklist.md](Real_Data_Enforcement_Worklist.md) | complete (historical) |
| ERC architecture + catalog (ERC001–037; 010/012–015/021 unassigned) | [ERC.md](ERC.md) | all three tiers built |
| Requirements / blocks / resolution (library model, ERC032) | [Requirements_And_Resolution.md](Requirements_And_Resolution.md) | increments 1–3 landed |
| Functional safety (FMEDA, FIT, SPFM/LFM/PMHF) | [Functional_Safety.md](Functional_Safety.md) | normative for what is implemented |
| Handle vs refdes namespaces | [Handles_And_Refdes.md](Handles_And_Refdes.md) | built |
| `supply` statement + part selection | [Power_Supply_Synthesis.md](Power_Supply_Synthesis.md) | S1–S4c built |
| Auto-expansion / virtual pins | [Synthesis_Auto_Expansion.md](Synthesis_Auto_Expansion.md) | v0.9 shipped |
| Attribute declaration/values/typing | [BHDL_Attribute_Type_System.md](BHDL_Attribute_Type_System.md) | shipped |
| Attribute vocabulary/resolution/consumers | [Unified_Attribute_System_Specification.md](Unified_Attribute_System_Specification.md) | shipped |
| Schematic engine (idiom-driven) | [Schematic_V4.md](Schematic_V4.md) | built (V4.1 in progress) |
| Interfaces (SPI/I2C/UART/DDR) | [Interfaces.md](Interfaces.md) | shipped through v0.8 |
| Net naming | [Net_Naming_Specification.md](Net_Naming_Specification.md) | see main spec §3.2 |
| Library resolution + lockfile + freeze | [Library_Resolution.md](Library_Resolution.md) | lockfile/freeze landed |
| Source resolvers (auto-fetch) | [Source_Resolvers.md](Source_Resolvers.md) | core landed |
| Supply-chain plugins | [Supply_Chain_Plugins.md](Supply_Chain_Plugins.md) | protocol shipped |
| Testbench / simulation control | [BHDL_Testbench_Specification_v2.md](BHDL_Testbench_Specification_v2.md) | partially implemented |
| Product-description model | [Product_Description_Model.md](Product_Description_Model.md) | architectural overview |

## Design specs (proposal / partial build)

Honestly self-labeled; each states its build status.

- [Vendor_Design_Blocks.md](Vendor_Design_Blocks.md) — `design {}` sizing IP (declarative shipped, Rhai escape hatch).
- [Vendor_Simulation_Blocks.md](Vendor_Simulation_Blocks.md) — `simulation {}` device IP (`stress` built; `model`/`stability` deferred).
- [Simulation_Margin_Signoff.md](Simulation_Margin_Signoff.md) — sign-off and margins.
- [Parameterization_And_BOM_Resolution.md](Parameterization_And_BOM_Resolution.md) — parameter/BOM resolution (v0.2).
- [Board_SKU_Variants.md](Board_SKU_Variants.md) — `variant` / DNP (v0.1).
- [Behavioral_Models.md](Behavioral_Models.md) — `behavior {}` dynamic simulation (proposal).
- [PnR_Professional_Architecture.md](PnR_Professional_Architecture.md) — PnR north star + staged plan against the as-built engine.
- [geometry-kernel.md](geometry-kernel.md) — P1 clearance-by-construction routing kernel (design).

## Reference / rationale

- [Syntax_Decisions_Summary.md](Syntax_Decisions_Summary.md) — early syntax-decision notes (superseded by the main spec on specifics; see its banner).
- [Expansion_Vs_Hierarchy.md](Expansion_Vs_Hierarchy.md) — why expansion and hierarchical modules are distinct mechanisms (settled 2026-08-21).

## Archived / superseded

- [BHDL_Complete_Specification_v2.0_ARCHIVED.md](BHDL_Complete_Specification_v2.0_ARCHIVED.md) — the prior v2.0 draft; drifted from the implementation, kept for history.
- [Interface_Specification.md](Interface_Specification.md) — superseded by Interfaces.md.
- [BHDL_Testbench_Specification.md](BHDL_Testbench_Specification.md) — superseded by the v2 testbench spec.
- `BHDL_Specification.md.old`, `BHDL_Specification_Cleaned.md.old`, `Bus_Interface_Specification.md.old` — pre-v2 syntax, retained for reference only.

## Verifying an example

Every syntax example in the current docs is expected to parse:

```
bhdl-cli <file>.bhdl parse       # syntax
bhdl-cli <file>.bhdl analyze     # + semantic passes
```

The mechanical gate is `tools/doc-check.sh` — it extracts every ` ```bhdl `
fence in these docs and feeds it to `bhdl-cli … parse` (trying entity /
board / interface wraps for fragments); deliberate pseudo-code is skipped
with an explicit `<!-- doc-check: skip (reason) -->` marker.
