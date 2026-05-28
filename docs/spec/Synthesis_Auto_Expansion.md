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

1. **A power-rail name** — `VCC*`, `GND*`, `AVCC*`, `VDD*`,
   `VSS*`, `AGND`, `AREF`, `UVCC`, `UGND`, `UCAP`, `VBUS`,
   `VBAT`, `V3OUT`, `NRST`, `RESET`, `RESET_N`. (Heuristic;
   see `is_power_rail_pin`.) Power pins are assumed always-on
   for chip operation, so children attached to them fire even
   if the board hasn't *explicitly* connected them yet at the
   moment expansion runs.
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

### 5.4 Known gap (task #90)

Phase 4.4 only stamps args the user *explicitly* passed.
Entity-parameter *defaults* don't propagate to instance
attributes. So `mcu: STM32F103Cx()` lands with empty
`mcu.part_no` even though the entity body uses the default
internally. Fix: extend Phase 4.4 to also stamp entity defaults
when no override is given.

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

## 8. Planned: abstract entities + SKU resolution (v0.9b)

The next layer up. Today a designer must pick the SKU at
instantiation time: `ATmega328P_DIP28()` vs `ATmega328P_QFN32()`.
The DIP-28 lacks ADC6/ADC7 (only on QFN-32); a board using ADC7
fails to synthesize without the designer knowing they need the
QFN package.

**Goal:** designer writes the abstract chip; synth picks the
smallest SKU whose pin set covers the board's actual usage.

### 8.1 Sketch

```bhdl
abstract entity ATmega328P {
    // Superset pin/alias set across all SKUs in the family.
    aliases { gpio0..gpio23, adc0..adc7, reset, ... }

    interface SPI, I2C, UART, ICSP;

    family {
        // Order = preference (smallest/cheapest first).
        ATmega328P_DIP28:  exposes [gpio0..gpio23, adc0..adc5, reset]
                           footprint "Package_DIP:DIP-28_W7.62mm"
                           pin_map { gpio0 = ("PD0", 2), ... };
        ATmega328P_TQFP32: exposes [gpio0..gpio23, adc0..adc7, reset]
                           footprint "Package_QFP:TQFP-32_7x7mm";
        ATmega328P_QFN32:  exposes [...]
                           footprint "Package_DFN_QFN:QFN-32_5x5mm";
    }
}
```

Board:

```bhdl
mcu: ATmega328P();          // abstract
mcu.gpio11 -> @MOSI;        // any SKU has gpio11 ✓
mcu.adc7   -> @TEMP_SENSE;  // ← only TQFP32/QFN32 have this
// Synthesizer picks TQFP32 (smallest fit; DIP-28 dropped).
```

### 8.2 Resolution algorithm

```
1. Walk board, collect set of aliases wired on each abstract
   instance.
2. For each instance, find the first SKU in its family{} whose
   `exposes` is a superset of the wired aliases.
3. Rewrite the abstract instance to the chosen physical SKU
   (instantiate the concrete entity; copy connections).
4. Stamp BOM attrs (mpn, footprint, kicad_symbol) from the SKU.
5. KiCad export consults the SKU's pin_map for physical pin
   numbers.
```

### 8.3 What the mechanism buys

- Designer experience: write intent, not package.
- BOM correctness: smallest part wins by default; explicit
  override (`mcu: ATmega328P(package="QFN-32")`) for board
  re-laying.
- Catchable errors: "Your board wires `mcu.adc7` but none of
  the available SKUs in family `ATmega328P` expose it." beats
  silent malfunction.

### 8.4 Cost

| What's new | Effort |
|---|---|
| `abstract entity` + `family { }` + `exposes` + `pin_map` grammar | ~1 session |
| Pin-usage analysis (collect aliases per instance) | ~0.5 session |
| SKU-resolution pass (Phase 4.6, after expansion) | ~1 session |
| Footprint/MPN propagation + KiCad export hook | ~0.5 session |
| Spec polish + 1 demo board | ~0.5 session |

~3-4 sessions total.

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
| Reference stdlib entities | `bhdl-stdlib/actives/atmega328p.bhdl`, `bhdl-stdlib/actives/stm32f103cx.bhdl`, `bhdl-stdlib/power/lm317.bhdl` |
| Reference tests | `bhdl-synthesizer/src/bin/test_{lm317_5v,atmega328p_decoupling,conditional_expansion,gpio_aliases,stm32_variant_sku,arduino_class_board}.rs` |
