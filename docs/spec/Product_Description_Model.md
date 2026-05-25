# BHDL Product Description Model

> **Status:** Architectural overview. Captures the model BHDL has
> grown into as of 2026-05. Cross-references the per-feature specs
> (`Vendor_Design_Blocks.md`, `Board_SKU_Variants.md`) plus the
> features that don't yet have their own spec (SKU attributes,
> socket composition, BOM walker).

## 1. What a `.bhdl` file is

Originally: a description of a circuit. *One* circuit.

After the layers documented below: a description of a **product
family** — every fact a manufacturing partner needs to fabricate,
populate, ship, and a simulator needs to verify, sitting in a
single source file (plus an entity stdlib).

Concretely a `.bhdl` file with the full surface in play declares:

- **Topology** — components and how they connect.
- **Design intent** — `for amplifier(gain: 14)`, `for
  current_source(current: 2mA)`. The board declares *what it wants*;
  the synthesizer designs the operating point.
- **Vendor design recipes** — math (declarative or scripted Rhai)
  that turns intent + device parameters into component values.
- **Device family information** — Koren parameters for triodes,
  Gummel-Poon for BJTs, etc., carried on the entity as numeric
  attributes. The synthesizer auto-routes them to recipes.
- **SKU data** — manufacturer, MPN, package, distributor part
  numbers on the entity, so the BOM is order-ready.
- **Composition** — `expansion { … }` blocks let one entity
  expand into many; `socket V in S;` declares one child is
  physically held inside another.
- **Product variants** — `variant <Name> { … }` blocks declare
  per-SKU value overrides and do-not-populate marks.

Every consumer (BOM walker, SPICE converter, schematic export,
PnR) reads the **same** netlist, with the same attributes, after
the same variant has been applied. There is one source of truth
per shipped SKU.

## 2. The layer cake (top-down)

```
            ┌───────────────────────────────────────────┐
            │       Variant patches (DNP + value)       │   ← board-level
            ├───────────────────────────────────────────┤
            │       Composition (expansion blocks,      │
            │       socket pairings)                    │
            ├───────────────────────────────────────────┤
            │       Vendor design recipes (declarative  │
            │       const/require/assign OR body rhai)  │   ← entity-level
            ├───────────────────────────────────────────┤
            │       Device-family discovery via         │
            │       `component_class`                   │
            ├───────────────────────────────────────────┤
            │       SKU attributes (manufacturer,       │
            │       mpn, physical_package, …)           │
            ├───────────────────────────────────────────┤
            │       Topology (entity instantiation,     │   ← always
            │       net connections, intent clauses)    │
            └───────────────────────────────────────────┘
```

Each layer is **optional and independent of the layers below it**:

- A board with no `variant` blocks is treated as a single
  implicit "default" SKU.
- An entity with no `design { }` block uses its expansion's
  literal values.
- An entity with no SKU attributes still synthesizes and
  simulates — the BOM row just shows empty manufacturer/MPN.
- An expansion block without a `socket … in …;` declaration just
  expands normally.

So a one-line `R1: Res(10k);` on a board is still a complete
description; you opt into each higher layer as the product needs
it.

## 3. The layers in detail

### 3.1 Topology and intent

```bhdl
board MyAmp {
    power VBB = 300V @ 100mA;
    ground GND;

    U1: SignalTubeStage();
    net amp_in: SIGIN -> U1.IN for amplifier(gain: 14);
    VBB -> U1.VBB;
    U1.GND -> GND;
}
```

The `for amplifier(gain: 14)` is the **intent clause**. It rides on
a net into the consuming entity; the synthesizer stamps it as
`intent_gain = "14.0"` on the consumer's instance. Multiple
intents exist out-of-the-box: `amplifier`, `current_source`,
`digital_switch`. New intents are stdlib-level additions, no
core changes.

### 3.2 Vendor design recipes

The intent gets turned into concrete component values by a
**design recipe** attached to the consuming entity. Two surfaces:

**Declarative** (Stages 1–4) — `const`, `require`, `assign`:

```bhdl
design for current_source {
    const v_be = 0.65;
    const i_target = intent.current;
    const v_cc = supply.VCC;
    require v_cc > v_be else "V_CC must exceed V_BE";
    Rref = (v_cc - v_be) / i_target;
}
```

Read more in `docs/spec/Vendor_Design_Blocks.md` §3.

**Rhai body** (Stage 5) — full embedded scripting language for
iterative math (bisection, search, optimization):

```bhdl
design for amplifier {
    inputs  { tube; intent; supply; }
    outputs { Rp; Rk; }
    body rhai r#"
        // log-grid peak find + bisection
        // …
        #{ Rp: v_p / i_p, Rk: (-v_gk) / i_p }
    "#
}
```

Read more in `docs/spec/Vendor_Design_Blocks.md` §11.

### 3.3 Device-family discovery

The recipe references device parameters via a namespaced prefix:

```bhdl
plate_current(tube, v_pk, v_gk)        // triode
ic_of_vbe(bjt, v_be, v_ce)             // BJT (future)
```

The synthesizer scans the expanding entity's children for one
tagged with a recognised `component_class` (`triode`, `bjt`, …)
and routes that entity's numeric attribute defaults to the recipe.
Adding a new device family is a single line in the synthesizer's
discovery list plus a new entity tagged with the new class.

The `tube` namespace is a backwards-compat alias for `triode`.
See `bhdl-stdlib/actives/triode.bhdl` for the worked example;
`bhdl-stdlib/actives/bjt_design.bhdl` shows the same pattern for
a second device family.

### 3.4 SKU attributes

Canonical names live in `bhdl-common::sku::attr` so all producers
and consumers agree:

| Attribute | Meaning |
|---|---|
| `manufacturer`     | Company name as the manufacturer spells it |
| `mpn`              | Manufacturer part number (canonical orderable identifier) |
| `part_number`      | Generic part-family name (historic; prefer `mpn`) |
| `physical_package` | Package / case style (TO-92, SOT-23, 0603, …). Spelled `physical_package` because `package` is a reserved BHDL keyword |
| `footprint`        | KiCad footprint library reference |
| `kicad_symbol`     | KiCad schematic symbol library reference |
| `datasheet`        | URL |
| `tolerance`        | "1 %", "0.1 %", … |
| `voltage_rating`   | Required minimum |
| `temp_coeff`       | "X7R", "C0G/NP0", "100ppm" |
| `power_rating`     | Resistors |
| `digikey_pn` / `mouser_pn` / `lcsc_pn` / `arrow_pn` / `nexar_pn` | Pre-pinned distributor SKUs |
| `component_class`  | "resistor", "capacitor", "triode", "bjt", "tube_socket", … |
| `refdes_prefix`    | BOM ref-designator prefix override (e.g. "FL" for fuses); inferred from class when absent |

The general-purpose BOM walker (`bhdl-analyzer::sku_bom`) reads
these directly off the post-expansion netlist. CLI:

```
bhdl-cli <board.bhdl> bom -f markdown        # human-readable
bhdl-cli <board.bhdl> bom -f csv -o bom.csv  # assembly-house upload
```

### 3.5 Composition: expansion + sockets

`expansion { … }` blocks turn one entity into many children with
shared internal nets. Example: `SignalTubeStage` expands into
`Rp`, `Rk`, `Ck`, `Rg`, `Cin`, `Cout`, `V` (the actual tube).

**Socket composition** layers on top: a composition entity can
declare `socket <held> in <socket>;` to mark one child as
physically held by another. Both children are real BOM rows
(the user buys the socket AND the tube), but downstream:

- **SPICE**: the socket is electrically transparent (any
  `component_class = "socket"|"tube_socket"|"dip_socket"|…` is
  skipped silently — the held part contributes the model).
- **PnR** (future consumer): the held part gets
  `socketed_in = "<socket_instance_name>"` stamped, so its
  footprint is suppressed (the socket carries the footprint).

Worked example: `SocketedTriode_Octal` in
`bhdl-stdlib/actives/triode.bhdl`, paired with
`bhdl-stdlib/connectors/tube_socket.bhdl`. The held tube entity
does NOT itself declare "I have no footprint" — that's a
property of THIS composition, not the device, so the same Triode
can be either socketed or soldered directly.

### 3.6 Board SKU variants

A single .bhdl file declares zero or more shipping SKUs. Each
variant is a *patch* on the base design — value override or DNP.

```bhdl
board ProductFamily {
    R_FB: Res(10k);
    C_BACKUP: Cap(10uF);

    variant Basic { /* base unchanged */ }
    variant Pro {
        R_FB.value = 100k;
    }
    variant EU {
        dnp C_BACKUP;
    }
}
```

CLI:

```
bhdl-cli board.bhdl list-skus
bhdl-cli board.bhdl --sku Pro bom
bhdl-cli board.bhdl --sku EU  spice
```

`--sku` is required when variants are declared (anti-silent-
fallback principle). DNP propagates to every electrical /
manufacturing consumer: the part doesn't appear in the BOM, the
SPICE simulator skips it (a missing R/C is an open circuit), and
the (future) pick-place export omits it. PCB layout keeps the
footprint and silkscreen so a populated unit can be hand-assembled.

Read more in `docs/spec/Board_SKU_Variants.md`.

## 4. Cross-cutting principles

These hold across every layer; deviations have been called out
and fixed in the conversation history.

### 4.1 No silent fallbacks

Every place the synthesizer could plausibly "make a good guess"
when something is missing instead errors out loudly:

- Empty device parameter map + recipe references `tube.*` →
  `EvalError("design recipe refers to device parameters but the
  synthesizer didn't discover a qualifying device child …")`.
- Vendor design recipe present but evaluation fails →
  `error!` + `return empty map` (no second-guessing with the
  Rust reference designer).
- Board declares variants but `--sku` not passed → CLI errors
  with the list of available variants.
- Recipe asks for a device family we don't have a discovery rule
  for → "unknown namespace …" diagnostic with the list of
  recognised namespaces.

### 4.2 Order-independence

The analyzer used to have order-dependent overlay machinery (a
board that imported the composition entity's file before the
held entity's file would miss the held entity's SKU attrs). The
synthesizer now late-binds at expansion time from a global
`entity_attribute_index` built across all imports plus the main
file, so import order doesn't matter.

### 4.3 Vendor extensibility = zero Rust

Every layer above the lexer is extensible without touching core
Rust code:

- **New tube family**: one entity in a `.bhdl` with Koren attrs +
  `component_class = "triode"`.
- **New BJT part**: one entity with Gummel-Poon attrs +
  `component_class = "bjt"`.
- **New stage topology**: one entity with an `expansion` block +
  a `design for <intent>` recipe.
- **New socket type**: one entity with `component_class =
  "tube_socket"` or similar + an EXPANSION_SOCKET_STMT in the
  composition entity that uses it.
- **New SKU variant**: one `variant` block on the board.

Adding a new *device family* (MOSFET, op-amp class) is one entry
in `DEVICE_CLASSES` in `expansion_interpreter.rs` plus the
stdlib entities. Adding a new *script-callable primitive* is one
`engine.register_fn` in `design_evaluator.rs`.

### 4.4 Single source of truth per shipped SKU

Once `--sku <Name>` is selected, every downstream consumer
(BOM, SPICE, future PnR/KiCad export) sees the *same* netlist
with the *same* attributes. There is no "BOM ignores DNP" /
"SPICE includes DNP" inconsistency: the variant-application step
runs once, and downstream reads from the patched netlist.

## 5. Worked example: end-to-end

A single file describing two shipping SKUs of a BJT current mirror:

```bhdl
// Imports — any order. The synthesizer's late-binding handles
// cross-file SKU attribute resolution.
import { Triode, SocketedTriode_Octal } from "bhdl-stdlib/actives/triode.bhdl";
import { TubeSocket_Octal } from "bhdl-stdlib/connectors/tube_socket.bhdl";
import { Res } from "bhdl-stdlib/passive/resistor.bhdl";

board MyProduct {
    power VBB = 300V @ 100mA;
    power HTR = 6.3V @ 2A;
    ground GND;

    // Topology: one socketed triode.
    V1: SocketedTriode_Octal();

    // Plate sees a load. Intent could drive sizing, but here
    // we declare it directly.
    net plate_load: VBB -> Rload: Res(22000).1 -> Rload.2 -> V1.P;
    net cathode:    V1.K -> Rk: Res(820).1 -> Rk.2 -> GND;
    net grid:       V1.G -> Rg: Res(1000000).1 -> Rg.2 -> GND;

    HTR -> V1.H1;
    V1.H2 -> GND;

    // Two shipping SKUs.
    variant Standard { /* base */ }
    variant Audiophile {
        // Higher-end build uses a closer-tolerance plate resistor.
        Rload.value = 22000;       // same value, but the SKU could…
        // (in a real Audiophile variant: lcsc_pn / mpn overrides
        //  would land here once v0.2 lifts the .value restriction)
    }
}
```

Running through the layers:

1. **Topology**: V1 + 3 board-level passives + power nets.
2. **Expansion**: SocketedTriode_Octal expands into V1_S (socket)
   + V1_V (triode), connected by shared internal nets.
3. **Socket pairing**: V1_V gets `socketed_in = "V1_S"` stamped.
4. **SKU resolution**: V1_S gets Belton VTB9-PT MPN + digikey_pn.
   V1_V gets the Triode entity's attributes (6SN7 in this case).
5. **Variant**: with `--sku Standard`, no patches apply.
6. **BOM**: Markdown / CSV with Rload + Rk + Rg + V1_S + V1_V,
   each row carrying its full SKU data.
7. **SPICE**: Rload, Rk, Rg, V1_V (triode device); V1_S skipped
   silently (SPICE-transparent socket).
8. **(Future) PnR**: V1_S placed at its footprint; V1_V skipped
   (socketed_in attribute respected).
9. **(Future) KiCad export**: V1_V shown on the schematic with a
   "DNP — socketed" annotation; V1_S shown with its footprint.

Every consumer reads the same netlist. The .bhdl file is the
single source of truth.

## 6. Spec cross-reference

| Document | Covers |
|---|---|
| `Vendor_Design_Blocks.md` | Design recipe surface (declarative + Rhai), §1–§10, then §11 amendment for the body hook |
| `Board_SKU_Variants.md`   | Variants v0.1: DNP + value override + CLI |
| `Behavioral_Models.md`    | System-level dynamic simulation: `behavior { }` DSL, scheduler, testbench surface, IBIS/PSpice import paths, multi-domain extension architecture |
| *(this document)*         | The architectural overview — how the pieces compose |

Features without a dedicated spec but described in this overview:

- **SKU attribute convention** (§3.4 above; canonical names in
  `bhdl-common::sku`)
- **Socket composition** (§3.5 above; reference implementation in
  `bhdl-stdlib/connectors/tube_socket.bhdl` +
  `SocketedTriode_Octal` in `bhdl-stdlib/actives/triode.bhdl`)
- **BOM walker** (`bhdl-analyzer::sku_bom`; CLI `bom` subcommand)

## 7. What's not (yet) in the picture

Real gaps that have been deliberately deferred — not blocking
the architecture, but worth knowing about:

- **Supplier-picker integration**: when a row has a value
  (`Res(575)`) but no MPN, ideally we'd query `bhdl-components`
  for the best 576 Ω 1 % 0603 reel and stamp the chosen part on
  the row. The supplier layer exists; wiring it to the BOM
  walker needs a product decision on local DB seed data vs
  live API credentials.
- **Variant whole-module gating** (v0.2): a variant that adds /
  removes whole sub-circuits, not just patches existing ones.
- **Variant SKU codes / manufacturing metadata**: `variant Pro
  { sku_code = "PRO-2026-A"; }` — small, would land at v0.2.
- **PnR `socketed_in` placement suppression**: the attribute is
  stamped; PnR just needs to read it.
- **KiCad schematic export DNP/socket annotations**: depends on
  KiCad export plumbing maturity.
- **Per-instance SKU overrides** (`R1: Res(575) { attribute
  lcsc_pn = "C44542"; }`): needs an instance-body grammar
  extension.
- **Cross-variant exclusion sets** ("EU SKU must not include the
  FCC-only part"): SAT-shaped, needs a constraint language.
- **Hierarchical variant inheritance** (`variant ProEU extends
  Pro, EU`): nice ergonomics once base v0.1 is in use.

None of these are architectural gaps — they're consumer-side
work or speculative extensions.

---

## 8. BHDL as the unifying component description format

This is the strategic frame that ties the architecture together
across electrical, manufacturing, configuration, simulation,
thermal, and mechanical dimensions.

### 8.1 The fragmentation problem

A complete description of a single component today is **scattered
across five or six different fragmentary files in different
formats in different places**:

| Fragment | Where it lives today | Format |
|---|---|---|
| Identity (manufacturer + MPN + package) | Distributor catalog row (LCSC, DigiKey, Mouser) | CSV / JSON via web API |
| Datasheet (human-readable spec) | Manufacturer "design resources" page | PDF |
| Electrical SPICE model | Manufacturer "design resources" page | `.lib` / `.mod` (Berkeley SPICE) |
| IBIS model (I/O buffer behavior) | Same | `.ibs` (IEEE-standardised) |
| PSpice behavioral subckt | Same | PSpice-dialect SPICE |
| Schematic symbol | KiCad / SnapEDA / Ultra Librarian | KiCad symbol, Eagle library, OrCAD |
| Footprint | Same | KiCad footprint, Eagle library |
| 3D body for mechanical / clearance / drop sims | Manufacturer or SnapEDA | STEP / IGES |
| Thermal R-network (theta_ja, theta_jc) | Inline in datasheet PDF | Hand-typed values |
| Application-note design math | Vendor design center, sometimes Excel | PDF + Excel spreadsheets |

No single file contains all of these. There is **no canonical
form of a component**. Every tool that wants to use the
component (synthesis, simulation, layout, BOM, mechanical
analysis) reads a different subset from a different format,
with version drift between fragments a chronic problem.

### 8.2 BHDL as the unifying form

Every fragment in §8.1 has a natural mapping into BHDL's per-
class importer pipeline:

| Fragment | BHDL importer | Lands in the .bhdl as |
|---|---|---|
| LCSC / DigiKey catalog row | Catalog scraper (planned) | `attribute manufacturer`, `mpn`, `digikey_pn`, `lcsc_pn`, `physical_package`, `value`, `tolerance`, `voltage_rating`, … |
| SPICE `.lib` / `.mod` | SPICE harvester (Behavioral_Models §10.1) | `behavior { analog { … } }` with PSpice→BHDL translation, OR device-family-attribute extraction (BJT Gummel-Poon, MOSFET BSIM, etc.) |
| IBIS `.ibs` | IBIS importer (Behavioral_Models §10.2) | `behavior { analog { lookup(table, …) } state Off, Driving; … }` |
| PSpice behavioral subckt | PSpice translator (Behavioral_Models §10.1) | `behavior { analog { … } }` |
| KiCad symbol | Symbol importer (existing) | `attribute kicad_symbol = "..."` + `symbol { … }` block |
| KiCad footprint | Footprint importer (existing) | `attribute footprint = "..."` + `layout { … }` block |
| STEP 3D body | STEP importer (planned, v1.0+) | `mechanical { mass = …; height = …; cog = …; }` |
| Thermal datasheet table | Datasheet-extraction LLM (planned) | `thermal { theta_ja = …; theta_jc = …; max_tj = …; }` |
| Design app-note math | Vendor authoring + `design { }` blocks | `design for amplifier { … }` (existing) |

The end state of an importer run is a **single `.bhdl` file per
component containing every fragment's content in BHDL syntax**.
The fragments become *upstream sources* that the BHDL
component imports from; the `.bhdl` is the **canonical post-
import form** — the source of truth every downstream tool
reads.

### 8.3 What the unified component looks like

```bhdl
// Auto-generated from upstream sources:
//   manufacturer / SKU:  LCSC catalog
//   electrical (SPICE):  TI design-resources `.lib`
//   IBIS:                TI `STM32F4-GPIO.ibs`
//   symbol:              KiCad standard library
//   footprint:           KiCad standard library
//   thermal:             datasheet PDF (LLM-extracted, human-verified)
//   3D body:             SnapEDA STEP
//
// All eight upstream fragments now live in this one .bhdl
// file with consistent versioning, single source of truth,
// machine-checkable cross-references.

entity STM32F401RE_GPIOC_5() {
    pin VDD: power in;
    pin VSS: ground inout;
    pin IO:  signal inout;

    // ─── Identity (from LCSC) ───────────────────────────────────
    attribute component_class = "mcu_gpio";
    attribute manufacturer = "STMicroelectronics";
    attribute mpn = "STM32F401RET6";
    attribute physical_package = "LQFP-64";
    attribute footprint = "Package_QFP:LQFP-64_10x10mm_P0.5mm";
    attribute kicad_symbol = "MCU_ST_STM32F4:STM32F401RETx";
    attribute datasheet = "https://www.st.com/resource/en/datasheet/stm32f401re.pdf";
    attribute digikey_pn = "497-14916-ND";
    attribute lcsc_pn = "C76995";

    // ─── Electrical I/O behaviour (from IBIS) ───────────────────
    behavior {
        // Pulled from STM32F4-GPIO.ibs
        table pullup_iv {
            -1.0   -0.080
             0.0   -0.060
            // … typical/min/max corners
        }
        table pulldown_iv { /* … */ }
        table power_clamp { /* … */ }
        table ground_clamp { /* … */ }

        state HighZ, DrivingHigh, DrivingLow;
        initial { state = HighZ; }

        analog {
            i_drive = if state == DrivingHigh then lookup(pullup_iv, V(IO))
                      else if state == DrivingLow then lookup(pulldown_iv, V(IO))
                      else 0;
            i_clamp = lookup(power_clamp, V(IO) - V(VDD))
                    + lookup(ground_clamp, V(IO) - V(VSS));
            i_total = i_drive + i_clamp;
        }
        // ... event handlers for state transitions ...
    }

    // ─── Thermal (from datasheet, human-verified) ───────────────
    thermal {
        theta_ja = 45 K/W;        // LQFP-64 on standard 4-layer board
        theta_jc = 12 K/W;
        max_tj = 105 °C;
    }

    // ─── Mechanical (from SnapEDA STEP) ─────────────────────────
    mechanical {
        mass = 230 mg;
        height = 1.4 mm;
        body_dimensions = (10mm, 10mm, 1.4mm);
        cog_offset = (0, 0, 0.7mm);
    }
}
```

This file is now the **single source of truth** for that
component across every downstream consumer:

- The synthesizer reads electrical for netlist + SPICE export
- The BOM walker reads identity for the order-ready bill
- The PnR reads footprint + mechanical for placement
- The behavioral simulator reads `behavior { }` for system tests
- The thermal solver reads `thermal { }` for junction-temp
  estimates
- The mechanical analyzer reads `mechanical { }` for board CG /
  shock checks

No version drift between fragments — they're all in one file,
one commit, one version. Updates re-run the importers; the
result is a regenerated `.bhdl` whose diff against the
previous version is auditable.

### 8.4 The economic flywheel

This unification has a real strategic implication for the
ecosystem:

- **Today**: every EDA tool maintains its own component
  database (Cadence's internal library, Altium's library
  manager, KiCad's library, OrCAD CIS, …). Library curation
  is a per-tool problem; libraries don't travel.
- **With BHDL as the canonical form**: a single
  `bhdl-parts` repository serves every tool that can read
  BHDL. KiCad, EDA browser-based tools, future BHDL-native
  flows all pull from one library. Component data becomes
  *portable*.
- **For vendors**: publishing a `.bhdl` (or letting the
  importer pipeline pull from existing fragments) means one
  file covers every downstream tool's needs. Reduces vendor
  support load.
- **For users**: opening any `.bhdl` reveals the full
  component description — no hunting across five different
  formats and three different vendor sites.

The same architectural property that makes BHDL the canonical
post-import form for one component also makes the *.bhdl
ecosystem* a single shared resource the way GitHub became a
single shared resource for code or PyPI became one for Python
packages. Single source of truth, single point of update,
versioned and auditable.

### 8.5 What this requires (and what it doesn't)

Required to realise the unification:

- **Importers** for each upstream fragment format (planned in
  `Behavioral_Models.md` §10 and elsewhere)
- **Roundtrip-safety**: importers must be re-runnable —
  re-importing the same upstream source produces the same
  `.bhdl` byte-for-byte (so diffs reflect real upstream
  changes, not importer variance)
- **Public registry**: a `bhdl-parts` repo or equivalent that
  hosts post-import components, ideally with CI that
  re-imports on upstream changes

Not required:

- **Replacing existing formats**: SPICE / IBIS / PSpice /
  KiCad libs all continue to exist as upstream sources.
  BHDL doesn't compete with them; it consolidates them.
- **Vendor cooperation**: most upstream fragments are already
  public. The importers don't require vendor sign-off.
- **A revolution in EE practice**: users can adopt BHDL
  component files incrementally, alongside their existing
  flows. The unification benefit kicks in as soon as ONE
  consumer of the `.bhdl` content exists (which it does:
  bhdl-cli itself).

### 8.6 What's left for this to be real

| Piece | Status |
|---|---|
| Architecture supports unified single-file form | ✅ done (this conversation) |
| Electrical/SKU/variants/sockets in the unified form | ✅ done |
| Behavioral architecture for unification of SPICE/IBIS/PSpice | ✅ spec'd (`Behavioral_Models.md`) |
| Thermal/mechanical sibling-block architecture | ✅ spec'd (`Behavioral_Models.md` §10.4) |
| LCSC importer (identity / SKU / passive electrical) | not started |
| SPICE harvester (device-family electrical) | not started |
| IBIS importer | not started (B10 in behavioral spec) |
| PSpice behavioral translator | not started (B9) |
| Symbol / footprint importers (KiCad) | partial |
| STEP 3D body importer | not started |
| Datasheet thermal-table extractor | not started (LLM-assisted) |
| `bhdl-parts` public registry | not started |

The remaining work is **importer engineering + ecosystem
infrastructure**, not architecture. Each importer is bounded
and independently committable. The unifying claim becomes more
true as more importers land; even the first one or two
already deliver value for the parts they cover.
