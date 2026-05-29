# Synthesis: Auto-Expansion, Aliases, and Parametric Entities

> **Status:** v0.9a (shipped). Scope: how a board author's intent
> ("this is an ATmega328P running at 5V with I²C in use") becomes
> a complete netlist including every support passive the chip's
> datasheet requires, with values computed from constructor args,
> and only the parts the board actually uses.
>
> Covers five orthogonal mechanisms that compose:
>   1. Virtual-pin **expansion blocks** (chip-vendor recipes).
>   2. **Conditional gating** (children fire only when wired).
>   3. **Design recipes** (compute child values from constructor args).
>   4. **Parametric entities** (one entity, many SKUs).
>   5. **Function aliases** (logical pin names like `gpio0`).
>
> Out of scope (v0.9b+): abstract entities + family{} blocks for
> usage-driven SKU resolution. Sketched in §8.

## 1. Motivation

A datasheet "typical application" circuit for any modern chip has
a lot more than just the chip. The ATmega328P's basic circuit
needs four decoupling caps. The STM32F103's needs nine parts
plus a power-on reset RC. An I²C bus needs pullups, USB needs ESD
diodes, regulators need feedback dividers sized for the desired
output voltage.

Real board failures aren't usually wrong port pins — they're
*forgotten passives*. Boards that "almost work" because the
designer didn't add the 100nF on AVCC. Boards that ship with
brown-outs because the bulk cap is missing. Boards that crash
on hot-plug because there's no ESD on D+/D-.

The auto-expansion track is BHDL's answer: **the chip vendor
authors the recipe once in the stdlib; every board that drops the
chip in gets the right support network for free, scaled to which
peripherals the board actually uses.**

A 12-line board file produces a 12-instance netlist:

```bhdl
import { LM317 } from "bhdl-stdlib/power/lm317.bhdl";
import { ATmega328P_DIP28 } from "bhdl-stdlib/actives/atmega328p.bhdl";

board MyBoard {
    U_REG: LM317(v_out=5V);
    MCU: ATmega328P_DIP28();
    @VIN -> U_REG.VIN;
    U_REG.VOUT -> @VCC;
    @VCC -> MCU.VCC;
    @VCC -> MCU.AVCC;
    MCU.GND1 -> @GND;  MCU.GND2 -> @GND;
    MCU.PC4 -> @SDA;   MCU.PC5 -> @SCL;
}
```

→ synthesizes 12 instances: the two chips plus 4 LM317 children
(C_in, C_out, R1=720Ω, R2=240Ω) plus 6 ATmega328P children
(4 decoupling caps + 2 I²C pullups). Every value correct, no
manual passive sizing.

## 2. Virtual-pin expansion blocks

The base mechanism. An entity declares one or more `virtual` pins
and an `expansion { }` block; the synthesizer fires the block
once per instance.

### 2.1 Form

```bhdl
entity LM317(v_out: voltage = 5V) {
    pin VIN:  power in;
    pin VOUT: power out virtual;       // ← virtual gate
    pin ADJ:  signal inout;
    pin GND:  ground;

    expansion {
        // Child syntax: parent_pin -> ChildName: ChildType(arg).pin;
        //               ChildName.other_pin -> parent_pin;
        VIN  -> C_in:  Cap(10µF).1;  C_in.2  -> GND;
        VOUT -> C_out: Cap(22µF).1;  C_out.2 -> GND;
        VOUT -> R1:    Res(r1_value).1; R1.2 -> ADJ;
        ADJ  -> R2:    Res(r2_value).1; R2.2 -> GND;
    }
}
```

### 2.2 When the block fires

Per-instance, exactly once. Triggered by the presence of *any*
`virtual` pin on the entity (the keyword is purely a gate; it
doesn't itself need to be the pin the block uses). The block's
children are stamped onto the netlist between phase 4
(connectivity extraction) and phase 5 (semantic annotation) —
see `expansion_interpreter::expand_entity_instances_with_designs`.

### 2.3 Pin references in the block

- `ParentPin(NAME)` — references an entity-declared pin. The
  child's connection joins the same net the board wired to that
  pin (e.g. if the board did `@VIN -> U1.VIN`, then `VIN -> C_in`
  inside the recipe lands on the `@VIN` net).
- `InstancePin(child_name, pin)` — references a child created in
  the same block.

### 2.4 Implementation pointers

- `bhdl-synthesizer/src/expansion_interpreter.rs` — the
  interpreter. `expand_one_instance` walks the recipe's
  connections and materializes children.
- `bhdl-synthesizer/src/lib.rs` — Phase 4.5 calls
  `expand_entity_instances_with_designs` after connectivity
  extraction. Without this call, recipes are silently dead
  code (the pre-`da3786a` state).

## 3. Conditional gating

By default an expansion block fires *all* its child connections
when the instance is materialized. That's wrong for recipes
that augment specific peripherals: an entity that adds I²C
pullups to PC4/PC5 shouldn't materialize them on a board that
uses PC4/PC5 as ADC4/ADC5.

### 3.1 Live-child rule

A recipe child is "live" (fires) iff every `ParentPin` it
references is either:

1. **An always-on support pin.** Despite the historical name
   `is_power_rail_pin`, this set is *not* limited to power rails:
   it's the category of pins whose datasheet support network is
   **mandatory regardless of board wiring**. It covers supply
   rails (`VCC*`, `GND*`, `AVCC*`, `VDD*`, `VSS*`, `VPP*`,
   `AGND`, `UVCC`, `UGND`, `VBUS`, `VBAT`, `V3OUT`), reference
   inputs (`AREF`, `VREF*`), charge-pump caps (`UCAP`),
   calibration references (`ZQ`), and reset pins (`NRST`,
   `RESET`, `RESET_N`). These are pins the board never "wires"
   in the peripheral sense but that always need their support
   component, so children attached to them fire even if the
   board hasn't explicitly connected them.

   The distinction this rule draws is **mandatory chip-support
   pin** (always fire) vs **peripheral signal pin** (`SDA`,
   `SCL`, GPIO — gate on actual board use, rule 2). When a
   vendor adds a new always-on support pin family, extend the
   curated set in `is_power_rail_pin`. (DDR4's `ZQ`/`VPP`/`VREF`
   were added this way — see §7 decision log.)
2. **Wired on the board** — the pin's `PinInstance` has a net.
   The board's flow extractor gives a pin a net whenever the
   pin appears in any connection statement.
3. **Multi-referenced in the expansion itself** — the parent
   pin appears as an endpoint in ≥2 of the recipe's
   connections. This means the recipe is wiring the pin
   internally (e.g. LM317's ADJ: R1.2→ADJ and ADJ→R2.1).

Otherwise (single recipe reference + no board net + not a power
rail) the child is dropped along with all its connections.

### 3.2 Example: atmega328p I²C pullups

```bhdl
expansion {
    // Always-on (VCC + GND are power rails):
    VCC  -> C_vcc:  Cap(100nF).1; C_vcc.2  -> GND1;
    ...

    // Conditional: only fires when PC4/PC5 are wired:
    PC4  -> R_pu_sda: Res(4.7kΩ).1; R_pu_sda.2 -> VCC;
    PC5  -> R_pu_scl: Res(4.7kΩ).1; R_pu_scl.2 -> VCC;
}
```

A board using I²C wires PC4/PC5 — pullups fire. A board using
those pins as ADC4/ADC5 doesn't wire them through the I²C
interface — pullups stay absent. The synthesizer logs which
children fired and which were dropped.

### 3.3 Implementation pointer

`expansion_interpreter::compute_live_children` (called by
`expand_one_instance` before child materialization).

## 4. Design recipes

Constructor args drive child values. A regulator with `v_out=5V`
should compute its feedback resistors from V_OUT; an op-amp with
`gain=10` should compute the gain-setting resistors.

### 4.1 Form

```bhdl
entity LM317(v_out: voltage = 5V) {
    pin VIN: power in;
    ...

    design {
        const v_ref = 1.25;
        require self.v_out >= 1.35
            else "LM317 minimum V_OUT is V_REF + headroom (~1.35V)";
        r2_value = 240.0;
        r1_value = 240.0 * (self.v_out - v_ref) / v_ref;
    }

    expansion {
        VOUT -> R1: Res(r1_value).1; R1.2 -> ADJ;
        ADJ  -> R2: Res(r2_value).1; R2.2 -> GND;
    }
}
```

### 4.2 Two-name-space substitution

The interpreter resolves child values via two lookup keys, in
priority order:

1. **By child name.** When the design block stores
   `Assign { child_name: "R1", expr: "..." }`, the value
   substitutes into the child `R1` directly. Used by Rust
   reference designers (`amplifier_bias_design` returns
   `{"Rp": ..., "Rk": ...}`).
2. **By first param identifier.** When the design block stores
   `Assign { child_name: "r1_value" }` (a script variable, not
   a child), and the expansion block writes `Res(r1_value)`,
   the recipe extractor stores `"r1_value"` as the child's
   first param string. The synthesizer looks up that string in
   the design output map; if found, substitutes the computed
   number. Used by stdlib `design { }` blocks (LM317 etc.).

This bridge lets a vendor write either style of design block —
"name the child" or "name the variable" — and have the values
flow into the netlist either way.

### 4.3 Required pre-step: Phase 4.4 constructor-arg stamping

For `self.v_out` to be readable from the design block, the
instance's attribute map must contain `v_out = "5V"`. The board
file's `U1: LM317(v_out=5V)` carries the arg on the
`COMPONENT_INST` AST node, but the multi-path instance-creation
code (database mapper / hierarchical extractor / component
inference) doesn't reliably copy those args onto the netlist
instance.

Phase 4.4 (`stamp_constructor_args_on_instances`, in lib.rs)
walks every `COMPONENT_INST` in the AST after Phase 4 and merges
its constructor args into the matching netlist instance with
`entry().or_insert()` semantics — doesn't clobber already-set
attrs but fills in anything missing. Phase 4.5 then runs the
expansion interpreter with `analysis.design_recipes` threaded
through.

### 4.4 Implementation pointers

- `bhdl-synthesizer/src/design_evaluator.rs` — `evaluate_recipe`.
- `bhdl-synthesizer/src/expansion_interpreter.rs:200-218` —
  design-recipe lookup per candidate.
- `bhdl-synthesizer/src/expansion_interpreter.rs:298-308` —
  value substitution at child-stamp time.

## 5. Parametric entities

A product family with N SKUs that differ only in memory size,
MPN, or other metadata gets one entity and N sets of constructor
args.

### 5.1 Form

```bhdl
entity STM32F103Cx(
    part_no: string      = "STM32F103C8T6",
    flash_kb: int        = 64,
    sram_kb: int         = 20,
    kicad_symbol: string = "MCU_ST_STM32F1:STM32F103C8Tx",
) {
    pin PA0: signal inout;
    ...
    attribute flash_kb     = flash_kb;
    attribute part_number  = part_no;
}
```

### 5.2 Use

```bhdl
// Default args target the canonical SKU.
mcu: STM32F103Cx();                   // = C8T6 Blue Pill

// Variant board overrides what differs.
mcu: STM32F103Cx(
    part_no="STM32F103CBT6",
    flash_kb=128,
    kicad_symbol="MCU_ST_STM32F1:STM32F103CBTx",
);
```

### 5.3 Decision matrix: one entity vs many

| Variant axis | Right model |
|---|---|
| Memory / MPN / temp grade only | Parametric (one entity) |
| Pin functions or peripherals differ | Separate entities, share footprint attr |
| Package differs (same electrical) | Separate entities per package |
| Both die *and* package differ | Separate entities, separate footprints |

The pitfall to avoid: encoding "pin X exists only on this SKU"
as a parameter. The type system can't catch wiring errors that
way. Datasheets that show "PE0/NC" entries for some SKUs are
already telling you these are different chips, not parameters
of one chip — just one PDF for convenience.

### 5.4 Default propagation (task #90, landed 2026-05-29)

Phase 4.4 stamps entity-parameter *defaults* in addition to
explicit constructor args. So `mcu: STM32F103Cx()` lands with
the canonical SKU's identifiers (`mcu.part_no="STM32F103C8T6"`,
`mcu.flash_kb="64"`) on the instance attrs — usable by the BOM
walker and KiCad exporter. The explicit-override path keeps
priority because Phase 4.4 stamps overrides first and defaults
use `entry().or_insert()` semantics.

Implementation: a per-entity `(param_name, default_text)` index
is built up front from same-file entity defs + imported stdlib
entities, then consulted per-instance after the explicit args
are stamped. See `extract_param_defaults` in
`bhdl-synthesizer/src/lib.rs`.

## 6. Function aliases

Logical pin names so board authors don't type datasheet
vocabulary. Resolved at connection-time; the underlying physical
pin is what lands in the netlist.

### 6.1 Form

```bhdl
entity ATmega328P_DIP28 {
    pin PB0..PB7: signal inout;
    pin PC0..PC6: signal inout;
    pin PD0..PD7: signal inout;
    ...

    aliases {
        gpio0  = PD0;    gpio1  = PD1;    ...   gpio13 = PB5;
        adc0   = PC0;    adc1   = PC1;    ...   adc5   = PC5;
        reset  = PC6;
    }
}
```

### 6.2 Use

```bhdl
mcu.gpio11 -> @MOSI_NET;        // resolves to mcu.PB3
mcu.adc0   -> @ADC_IN;          // resolves to mcu.PC0
mcu.reset  -> @RESET_NET;       // resolves to mcu.PC6
```

The emitted netlist contains the physical pin names — aliases
are a designer-side convenience, not a netlist concept.

### 6.3 Resolution rules

When the synthesizer processes a connection `instance.X`:

1. If `X` is a directly-declared pin on the module → use it.
2. Otherwise check `module.attributes["intf_bind__X"]` (v0.7
   interface-field-binding form, dotted like `spi.MOSI`).
3. Otherwise check `module.attributes["alias__X"]` (v0.9
   function-alias form).
4. Otherwise: pin-not-found error.

### 6.4 Implementation pointers

- `bhdl-parser/src/top_level.rs::parse_entity_aliases_block`
- `bhdl-synthesizer/src/hierarchical_connectivity.rs::ENTITY_ALIAS_ATTR_PREFIX`
  and `resolve_field_binding_alias` (extended to handle both
  prefixes).
- `bhdl-synthesizer/src/lib.rs::add_pins_for_component` (where
  alias-attribute stamping happens during module creation).

## 7. How the mechanisms compose

The five mechanisms are designed to compose. A single chip
entity uses all of them:

```bhdl
entity ATmega328P_DIP28(part_no: string = "ATMEGA328P-PU") {
    pin VCC: signal inout virtual;     // §2 — gate for expansion
    pin AVCC, AREF, GND1, GND2, ...;

    // §5: SKU-variant constructor arg.
    attribute part_number = part_no;

    // v0.7 interfaces — designer writes mcu.spi.MOSI.
    interface SPI spi { MOSI=PB3; MISO=PB4; SCK=PB5; CS=PB2; }

    // §6: function aliases — designer writes mcu.gpio11.
    aliases { gpio0 = PD0; ...  reset = PC6; }

    // §2 + §3: expansion with always-on + conditional children.
    expansion {
        // Always-on (power-rail rule, §3.1.1).
        VCC  -> C_vcc: Cap(100nF).1; C_vcc.2 -> GND1;
        ...
        // Conditional (board-wired rule, §3.1.2).
        PC4  -> R_pu_sda: Res(4.7kΩ).1; R_pu_sda.2 -> VCC;
        PC5  -> R_pu_scl: Res(4.7kΩ).1; R_pu_scl.2 -> VCC;
    }
}
```

A 4-line board fragment generates ~10 instances with correct
values, wiring, and BOM identifiers. The board author writes
intent at the chip-and-interface level; the synthesizer fills
in the datasheet-mandated circuit around it.

## 8. Abstract entities + SKU resolution (v0.9b, landed 2026-05-29)

**Goal achieved:** designer writes the abstract chip; the resolver
picks the first SKU whose pin-map covers the aliases the board
actually uses, and rewrites the source to use that SKU's
specific names.

### 8.1 Form

```bhdl
abstract entity ATmega328P {
    // Abstract port list — the surface a board author reads to
    // know what's available. Every BHDL entity has a portlist;
    // the abstract entity is no exception. Each family entry's
    // pin_map below maps these abstract ports → that SKU's
    // concrete pin name. SKUs that don't expose a given abstract
    // port simply omit it from their map.
    pin vcc:   signal inout;
    pin avcc:  signal inout;
    pin gnd:   signal inout;
    pin agnd:  signal inout;
    pin adc0:  signal inout;
    ...
    pin adc7:  signal inout;   // QFN-only
    pin reset: signal inout;

    family {
        ATmega328P_DIP28 {
            // Each family entry declares its own pin_map from
            // abstract alias → that SKU's concrete pin name.
            // SKUs that don't expose an alias omit it.
            vcc  = VCC;
            avcc = AVCC;
            gnd  = GND1;
            agnd = GND2;
            adc0 = PC0;   adc1 = PC1;   …   adc5 = PC5;
            reset = PC6;
            // adc6 / adc7 absent — DIP-28 doesn't bring them out.
        };
        ATmega328P_QFN32 {
            vcc  = VCC1;          // ← QFN supply pin is named differently
            avcc = AVCC;
            gnd  = GND1;
            agnd = GND3;
            adc0 = PC0;   …   adc5 = PC5;
            adc6 = ADC6;  adc7 = ADC7;     // QFN-only
            reset = PC6;
        };
    }
}
```

### 8.2 Use

```bhdl
mcu: ATmega328P();                  // abstract
@VCC -> mcu.vcc;                    // SKU-stable alias names
mcu.gnd  -> @GND;
mcu.adc7 -> @TEMP_SENSE;            // ← only QFN's pin_map has adc7
```

The preprocessor rewrites both the instance type token and each
alias reference. Board B above lands as:

```bhdl
// (abstract entity block stripped)
mcu: ATmega328P_QFN32();
@VCC -> mcu.VCC1;                   // mcu.vcc → QFN's VCC1
mcu.GND1 -> @GND;
mcu.ADC7 -> @TEMP_SENSE;
```

— exactly the source the regular parser/analyzer/synthesizer
expect.

### 8.3 Resolution algorithm

```
1. extract_abstract_decls(source)
     → HashMap<abstract_name, (abstract_ports, family entries)>
2. Validate: every family entry's pin_map keys ⊆ abstract_ports
     (catches stdlib-author bugs where a SKU exposes an alias
      the abstract entity doesn't declare).
3. extract_abstract_instances(source, decls)
     → instances using one of the abstract names
4. For each instance:
     a. Find the set of `inst.X` references the board makes.
     b. Validate: every X ∈ abstract_ports (catches board-author
        typos; error names the abstract entity, not a SKU).
     c. Pick the first family entry whose pin_map keys ⊇ used set.
     d. Record (instance → chosen entry).
5. Rewrite source:
     - strip every `abstract entity NAME { ... }` block
     - rewrite each abstract instance type token to the chosen SKU
     - rewrite each `mcu.X` → `mcu.<pin_map[X]>` per the chosen map
```

The two-level validation gives clear diagnostics at each layer:

- **Stdlib author makes a mistake** (SKU pin_map references an
  alias not in the abstract port list):
  ```
  Family entry 'ATmega328P_QFN32' maps abstract port 'spi_mosi'
  which is not declared on the abstract entity. Declared ports:
  ["adc0", ..., "vcc"].
  ```

- **Board author makes a typo** (uses an alias that doesn't exist):
  ```
  Board references 'mcu.adc9' but abstract entity 'ATmega328P'
  has no port named 'adc9'. Declared ports: ["adc0", ..., "vcc"].
  ```

Both errors point at the layer the user can fix.

**Multi-function-pin conflict.** Real chips multiplex pins:
PC4 on ATmega328P is GPIO *or* ADC4 *or* I²C SDA — but only one
role at a time. The abstract entity can expose all three as
separate ports, with the SKU's pin_map routing all three to PC4:

```bhdl
abstract entity ATmega328P {
    pin adc4: signal inout;
    pin sda:  signal inout;          // mux'd with adc4 on PC4
    family {
        ATmega328P_DIP28 {
            adc4 = PC4;              // PC4 as ADC4 ...
            sda  = PC4;              // ...or as SDA, not both
        };
    }
}
```

A board that wires both at once errors with a diagnostic naming
the offending physical pin AND the colliding aliases:

```
Multi-function-pin conflict on 'mcu' (ATmega328P resolved to
SKU 'ATmega328P_DIP28'): physical pin 'PC4' is claimed by
aliases ["adc4", "sda"]. Each physical pin can only serve one
role at a time — pick one alias per pin.
```

The check happens after SKU resolution (so the diagnostic
references the chosen SKU's pin_map) but before source rewriting
(so the user sees the alias names they actually wrote).

### 8.4 Implementation: source-text preprocessor

The mechanism lives *above* the parser, as a string rewrite
over the raw source. No parser/grammar changes — abstract
entities are a preprocessor convention, parser-invisible. This
sidesteps the multi-module-creation-site integration challenge
that derailed an earlier in-tree attempt (see the related
note in task #92 history).

Public entry point: `bhdl-synthesizer/src/abstract_resolver.rs::preprocess`.
Callers (e.g. the synthesizer driver or a test) read the source
string, call `preprocess`, then parse the result.

### 8.5 What this buys

- **SKU-independent designer experience.** The board author
  writes `mcu.vcc` and gets the right pin for whichever SKU
  is chosen — DIP-28's plain `VCC` or QFN-32's `VCC1`.
- **BOM correctness.** Smallest-listed family member wins by
  default; users can lock a choice by instantiating the concrete
  entity directly (`mcu: ATmega328P_DIP28()`), bypassing the
  abstract resolver.
- **Catchable errors at synth time.** "Abstract entity
  'ATmega328P' instance 'mcu' wires aliases {adc7, …}, but no
  family member's pin_map covers all of them." beats silent
  malfunction or wrong-pin behaviour.
- **No grammar overhead.** The bhdl-parser doesn't change;
  abstract entities are a preprocessing layer.

### 8.6 Integrated entry point: `synthesize_from_source`

```rust
pub async fn synthesize_from_source(source: &str) -> Result<(String, Netlist)>
```

Single call that runs the full pipeline: preprocessor → parser
→ analyzer → synthesizer. Returns the rewritten source alongside
the netlist so callers can inspect what the parser actually saw.
Callers don't need to know preprocessing happened.

The preprocessor also **strips imports of family members that
weren't chosen** — the downstream analyzer otherwise emits
"Undefined component type" for entities imported but unused
from the same file. So a board that imports both
`ATmega328P_DIP28` and `ATmega328P_QFN32` (for the family list)
and ends up resolving to DIP-28 will have the QFN-32 import
quietly removed from the rewritten source.

### 8.7 SKU choice exposed as instance attributes

After SKU resolution, `synthesize_from_source` stamps two
attributes on each abstract-resolved instance:

- `abstract_origin` — the abstract entity the user wrote
  (e.g. `"ATmega328P"`).
- `selected_sku` — the concrete SKU the resolver picked
  (e.g. `"ATmega328P_QFN32"`).

BOM walkers, KiCad exporters, SPICE exporters, and comparators
can read these directly from `instance.attributes` without
re-running the preprocessor.

### 8.8 Open follow-ups

- **`exposes [list]` shorthand** for SKUs where most aliases
  map trivially (alias name == pin name); avoids verbose
  per-pin = mappings. Cosmetic; the explicit form works fine.

## 9. Decision log

- **2026-05-28 (v0.9a)** — Wire Phase 4.5 expansion-interpreter
  call. Previously dead code; every stdlib `expansion { }` block
  silently no-op'd. (`da3786a`)
- **2026-05-28** — Add Phase 4.4 constructor-arg stamping;
  without this, design blocks couldn't read `self.v_out`.
  (`57992dc`)
- **2026-05-28** — Add design→expansion value-substitution by
  param identifier (key 2 in §4.2), in addition to the
  existing by-child-name path. (`57992dc`)
- **2026-05-28** — Add conditional gating
  (`compute_live_children` with the three-rule heuristic).
  Closes task #89 and obsoletes the proposed v0.8 `require`
  interface syntax (task #83) — vendor expansion + gating
  covers everything `require` would have. (`5f5a22b`)
- **2026-05-28** — Add `aliases { }` block + resolution.
  Foundation for the v0.9b abstract-entity layer. (`e052ce0`)
- **2026-05-28** — Fix `merge_nets` pin-instance staleness
  bug exposed by the alias test. Same commit (`e052ce0`).
- **2026-05-29 (v0.9b)** — Land abstract entities + per-SKU
  pin maps via a source-text preprocessor
  (`bhdl-synthesizer/src/abstract_resolver.rs`). Boards write
  `mcu.vcc` and the resolver picks the SKU + rewrites to that
  SKU's specific pin name (DIP-28's `VCC` vs QFN-32's `VCC1`).
  Closes task #92.
- **2026-05-29 (v0.9b refinement)** — Abstract entities declare
  their own port list (`pin name: type direction;`). Validated
  at both stdlib-author and board-author layers:
  family entries' pin_map keys must be a subset of the abstract
  ports, and board references must name declared ports. The
  abstract port list is the surface a board author reads to
  know what's available — every BHDL entity has a portlist,
  the abstract entity is no exception.
- **2026-05-29** — Extend Phase 4.4 to stamp entity parameter
  defaults in addition to explicit overrides. Closes task #90;
  the C8T6 default-args board now lands with full BOM
  identifiers (`part_no`, `flash_kb`) on its instance.
- **2026-05-30 (DDR4 stdlib)** — First stdlib part exercising the
  full v0.8 interface stack (parametric + generate + hierarchical
  + constraints) together with an expansion network:
  `bhdl-stdlib/actives/ddr4_sdram.bhdl` (Micron MT40A x8) +
  `bhdl-stdlib/interfaces/ddr4.bhdl`. The chip's expansion adds the
  240 Ω ZQ calibration resistor, VPP/VDD/VDDQ decoupling, and the
  VREFCA bypass. Closes task #97. (`c087231`)
- **2026-05-30 (gating fix)** — The conditional-gating rule 1 set
  was over-narrow: it suppressed *mandatory* support children whose
  pin connects a non-rail pin to a rail and is referenced only once
  (DDR4 `R_zq` ZQ→VSS, `C_vpp` VPP→VSS, `C_vref` VREFCA→VSS). These
  are as mandatory as the already-whitelisted `AREF`/`UCAP`/`NRST`,
  so `is_power_rail_pin` was extended with `VPP*`/`VREF*`/`ZQ` and
  reframed as "always-on support pin" (§3.1 rule 1). Also confirmed
  the full `synthesize_from_source` pipeline composes imports +
  parametric + expansion for the real imported-entity path (the
  empty-module symptom was specific to inline entity defs, an
  unusual pattern). Closes task #98. (`c264bb0`)

## 10. Implementation file index

| Concern | File:line |
|---|---|
| Phase 4.4 constructor-arg stamping | `bhdl-synthesizer/src/lib.rs::stamp_constructor_args_on_instances` |
| Phase 4.5 expansion call | `bhdl-synthesizer/src/lib.rs` (in `generate_from_ast_and_analysis_internal`) |
| Expansion interpreter | `bhdl-synthesizer/src/expansion_interpreter.rs` |
| Conditional gating | `expansion_interpreter::compute_live_children` |
| Design recipe evaluator | `bhdl-synthesizer/src/design_evaluator.rs` |
| Alias parsing | `bhdl-parser/src/top_level.rs::parse_entity_aliases_block` |
| Alias resolution | `bhdl-synthesizer/src/hierarchical_connectivity.rs::resolve_field_binding_alias` |
| `is_power_rail_pin` heuristic | `bhdl-synthesizer/src/expansion_interpreter.rs` |
| Reference stdlib entities | `bhdl-stdlib/actives/atmega328p.bhdl`, `bhdl-stdlib/actives/stm32f103cx.bhdl`, `bhdl-stdlib/actives/ddr4_sdram.bhdl`, `bhdl-stdlib/power/lm317.bhdl` |
| DDR4 interface stack | `bhdl-stdlib/interfaces/ddr4.bhdl` (DiffPair, DDR4Data, DDR4Ca, parametric `DDR4<byte_lanes>`) |
| Reference tests | `bhdl-synthesizer/src/bin/test_{lm317_5v,atmega328p_decoupling,conditional_expansion,gpio_aliases,stm32_variant_sku,arduino_class_board,ddr4_stdlib}.rs` |
