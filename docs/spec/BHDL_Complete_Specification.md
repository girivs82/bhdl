# BHDL — Board Hardware Description Language

**Complete Specification (implementation-grounded, 2026-07)**

> This specification describes the language **as implemented**. Every construct
> here corresponds to a production in `bhdl-parser` and appears in the working
> corpus under `tests/circuits/realistic/` or `bhdl-stdlib/`. Syntax that the
> grammar accepts but no design uses is marked *(rare)*; syntax that was
> proposed but never shipped is not documented at all. The prior aspirational
> v2.0 draft is kept as `BHDL_Complete_Specification_v2.0_ARCHIVED.md`.
>
> Deep subsystems have dedicated companion specs, cited inline; this document
> is the language reference and the map to them.

## Table of Contents

1. [What BHDL Is](#1-what-bhdl-is)
2. [A Worked Example](#2-a-worked-example)
3. [Boards, Nets, and Connections](#3-boards-nets-and-connections)
4. [Component Instantiation](#4-component-instantiation)
5. [Types, Units, and Values](#5-types-units-and-values)
6. [Entities](#6-entities)
7. [Parameters and Constructor-Argument Validation](#7-parameters-and-constructor-argument-validation)
8. [Entity Synthesis Blocks (`expansion` / `design` / `simulation`)](#8-entity-synthesis-blocks)
9. [Symbols, Layout, Interfaces, Aliases](#9-symbols-layout-interfaces-aliases)
10. [Ports, Power, and Supply Synthesis](#10-ports-power-and-supply-synthesis)
11. [The Data-Honesty Doctrine (Real-Data Policy)](#11-the-data-honesty-doctrine)
12. [Electrical Rule Checking (ERC)](#12-electrical-rule-checking)
13. [Sign-off, BOM, and Freeze](#13-sign-off-bom-and-freeze)
14. [Schematic Visualization](#14-schematic-visualization)
15. [Handles vs Reference Designators](#15-handles-vs-reference-designators)
16. [The Toolchain (CLI)](#16-the-toolchain-cli)
17. [Grammar Reference](#17-grammar-reference)
18. [Companion Specifications](#18-companion-specifications)

---

## 1. What BHDL Is

BHDL is a domain-specific language for describing electronic circuit boards.
A board is written as **flows** — sources connected through components to
destinations — rather than as a structural netlist. The toolchain reads the
source, resolves and sizes components, runs a real DC/transient circuit
solver, checks electrical rules, produces a schematic, and emits a BOM and a
frozen as-fabbed record.

Two principles run through the whole system:

- **Flow, not structure.** You write `@VIN -> C1: Cap(100µF).1; C1.2 -> @GND`,
  describing how energy and signals move. Reference designators, net names for
  anonymous wires, and support components are derived, not hand-placed.
- **Real data or no data.** The toolchain never fabricates an electrical value
  to make a design "work." A quantity that was not computed from declared
  inputs is not printed; a missing datasheet value is a hard error, not a
  guessed default. This is the *Real-Data Policy* (§11), and it is enforced
  end-to-end — in the solver, the sizing logic, sign-off, and even the
  schematic annotations.

The language is small. The constructs below — boards, nets, flows, entities,
and a handful of entity-body blocks — compose to describe everything from a
blinking LED to a two-MCU Arduino-class board with switching regulators.

---

## 2. A Worked Example

A complete, synthesizable buck converter (from
`tests/circuits/realistic/buck_converter_simple.bhdl`):

```bhdl
import { Cap } from "bhdl-stdlib/passives/capacitor.bhdl";
import { SchottkyDiode } from "bhdl-stdlib/passives/diode.bhdl";
import { Inductor } from "bhdl-stdlib/passives/inductor.bhdl";
import { Res } from "bhdl-stdlib/passives/resistor.bhdl";
import { LM2596 } from "bhdl-stdlib/components/power/switching_regulators/LM2596.bhdl";

board SimpleBuckConverter {
    // Rails
    power VIN = 12V @ 3A;
    power VOUT = 5V @ 2A;
    ground GND;

    // Input filtering
    VIN -> C1: Cap(100µF, voltage=25V).1;
    C1.2 -> GND;

    // Buck controller
    VIN -> U1: LM2596().VIN;
    U1.GND -> GND;
    U1.EN -> VIN;                 // always enabled

    // Power stage
    U1.SW -> L1: Inductor(33µH).1;
    L1.2 -> VOUT;
    GND -> D1: SchottkyDiode(SS34).A;
    D1.K -> U1.SW;

    // Output filtering + feedback divider (5V = 1.23V·(1 + 10k/3.24k))
    VOUT -> C3: Cap(220µF, voltage=10V).1;
    C3.2 -> GND;
    VOUT -> R1: Res(10kΩ, tolerance=1%).1;
    R1.2 -> fb_node;
    fb_node -> R2: Res(3.24kΩ, tolerance=1%).1;
    R2.2 -> GND;
    fb_node -> U1.FB;

    // Load
    VOUT -> RL: Res(2.5Ω, wattage=10W).1;
    RL.2 -> GND;
}
```

Everything in later sections is a generalization of what appears here: rail
declarations, imports, inline component instantiation inside a flow, numeric
and named pin references, named nets (`fb_node`), and typed constructor
arguments with units.

---

## 3. Boards, Nets, and Connections

### 3.1 Board

```bhdl
board Name { … }
```

A `board` is the top-level design unit. Its body holds rail declarations
(`power` / `ground` / `port`), component instances, connection/flow
statements, and — less commonly — `const`, `attribute`, `generate`, `when`,
`variant`, `power_domain`, and `supply` statements.

### 3.2 Nets and the `@` prefix

A net is a wire. Two ways to name one appear in practice:

- A **declared rail** (`power VIN`, `ground GND`, `port VCC_5V`) is referenced
  by its name. The `@` prefix is optional on a declared rail: both
  `VIN -> …` and `@VIN -> …` are used across the corpus and mean the same net.
- An **ad-hoc named net** is introduced simply by naming it in a flow
  (`R1.2 -> fb_node; fb_node -> U1.FB`) — `fb_node` becomes a net joining
  those pins.

`@name` unambiguously denotes a net (never a component handle); a bare
identifier is resolved as a net when it is not a declared instance handle.
Anonymous nets — the wires implied by chaining `A -> B -> C` — need no name
and are numbered by the toolchain (see [Net_Naming_Specification.md](Net_Naming_Specification.md)).

### 3.3 Connections and flows

A connection statement is an expression terminated by `;`. The connection
operators, in the flow direction, are:

| Operator | Meaning |
|----------|---------|
| `->`  | directed connection / flow (the workhorse) |
| `<->` | bidirectional connection *(rare)* |
| `\|>` | staged power flow *(rare; see §10)* |

Chained `->` forms a flat series of hops, each hop a net:

```bhdl
@VCC -> Res(4.7kΩ).1 -> LED(red).A;   // two anonymous nets in series
```

A connection may optionally carry a physical `where` constraint and/or a
design `for` intent (§3.4):

```bhdl
EXPR [where c1, c2, …] [for intent(args)] ;
```

### 3.4 Intent clauses (`for`)

A flow can be annotated with a single design *intent* — a named, parameterized
statement of what the connection is for. The synthesizer and ERC read intents
to size and check the circuit:

```bhdl
@VIN -> cin: Cap(100µF).1; cin.2 -> @GND
    for noise_filtering(cutoff: 10kHz, attenuation: 40dB);
```

Intent arguments are `name: value` pairs (values may carry units). Common
intent names in the corpus include `voltage_regulation`, `noise_filtering`,
`current_sensing`, `communication_interface`, `precision_measurement`, and
`high_freq_bypass`. Intents are advisory metadata consumed by synthesis and
ERC; they never change connectivity.

---

## 4. Component Instantiation

An instance binds a **handle** (a human name) to an entity with constructor
arguments, and — in a flow — a pin to wire.

### 4.1 Forms

```bhdl
// Standalone declaration
mcu: ATmega328P_DIP28();
u_reg: LM317(v_out=5V);

// Inline in a flow: the instance is declared at the point it is wired
VIN -> C1: Cap(100µF, voltage=25V).1;
U1.SW -> L1: Inductor(33µH).1;
```

`handle: Type(args)` creates the instance; `.pin` selects the pin to connect.
The same handle is then used to reach the instance's other pins
(`C1.2`, `U1.GND`, `U1.FB`).

### 4.2 Pin references

- Numeric: `.1`, `.2` (two-terminal passives).
- Named: `.VIN`, `.GND`, `.SW`, `.A`, `.K` — the entity's declared pin names.
- Polarity: `.+` / `.-` (e.g. electrolytic caps).

### 4.3 Constructor arguments

Positional and named (keyword) arguments, freely mixed, with the named ones
following the datasheet parameter names of the entity:

```bhdl
Capacitor(22uF, 25V)                    // positional
Res(10kΩ, tolerance=1%)                 // positional + named
LM317(v_out=5V)                         // named
```

Every argument must bind to a declared parameter and satisfy its value domain
(§7). Unrecognized arguments are hard errors, not silently-dropped
annotations.

### 4.4 Wiring a declared instance

Once an instance has a handle, its pins are wired by naming them in flows —
there is no separate block-mapping syntax; connection is always a flow:

```bhdl
mcu: ATmega328P_DIP28();
@VCC -> mcu.VCC;
@VCC -> mcu.AVCC;
mcu.GND1 -> @GND;
```

---

## 5. Types, Units, and Values

### 5.1 Physical-quantity types

Entity parameters and constants are typed by physical quantity. The types in
use:

`resistance`, `capacitance`, `inductance`, `voltage`, `current`, `frequency`,
`time`, `power`, `percentage`, plus the scalar types `float`, `int`, `bool`,
and `string`.

### 5.2 Unit literals

A value is `<number><unit>` with **no space**. The number supports `_` digit
grouping and scientific notation (`6.734e-15`). Units by quantity:

| Quantity | Units |
|----------|-------|
| Resistance | `Ω` (`Ohm`), `mΩ`, `kΩ`, `MΩ` |
| Capacitance | `F`, `pF`, `nF`, `µF` (`uF`) |
| Inductance | `H`, `nH`, `µH` (`uH`), `mH` |
| Voltage | `V`, `mV`, `µV`, `kV` |
| Current | `A`, `µA` (`uA`), `mA` |
| Power | `W`, `mW`, `kW` |
| Frequency | `Hz`, `kHz`, `MHz`, `GHz` |
| Time | `s`, `ns`, `µs` (`us`), `ms` |
| Ratio | `%` |
| Misc | `dB`, `mm`, `mm2` (`mm²`), `°C` |

Both the micro sign `µ` and ASCII `u` are accepted (`µF` = `uF`). At board
call sites, an SI prefix without the unit letter is also accepted as shorthand
where the entity's parameter type fixes the quantity (`Res(10k)`,
`Cap(100u)`).

### 5.3 The rail spec `V @ I`

A power rail carries a voltage and a per-rail current budget:

```bhdl
power VCC_3V3 = 3.3V @ 500mA;
```

The `@ I` part is the design's *declared* load budget for that rail; it is
used by rail-budget checks and is never fabricated when omitted (Real-Data,
§11).

### 5.4 Expressions

Attributes, `design` blocks, and `where` constraints use a standard
expression grammar with the usual precedence: `||`, `&&`, bitwise `| ^ &`,
comparisons `== != < > <= >=`, additive `+ -`, multiplicative `* / %`, unary
`+ - ! ~`, and a ternary `cond ? a : b`. Built-in math functions include
`sqrt, abs, floor, ceil, round, pow, exp, log, log10, min, max`, and the
trigonometric set. A chained ternary is the idiom for value tables:

```bhdl
attribute footprint =
    style == "pad"  ? "TestPoint:TestPoint_Pad_D1.5mm" :
    style == "hole" ? "TestPoint:TestPoint_THTPad_D1.5mm_Drill0.7mm" :
    "TestPoint:TestPoint_Pad_D1.5mm";
```

---

## 6. Entities

An **entity** is a reusable circuit element — a part, or a parameterized
subcircuit. Entities live in the standard library (`bhdl-stdlib/`) or in board
files, and are pulled in with `import`.

### 6.1 Definition

```bhdl
entity Res(value: resistance, tolerance: percentage = 5%, wattage: power = 0.25W) {
    pin 1: signal inout;
    pin 2: signal inout;

    attribute component_class = "resistor";
    attribute resistance = value;
    attribute tolerance = tolerance;
    attribute power_rating = wattage;
    attribute kicad_symbol = "Device:R";
}
```

An entity has a parameter list (§7), pin declarations (§6.2), attributes
(§6.3), and — for parts that size or expand themselves — the synthesis blocks
of §8.

### 6.2 Pins

```bhdl
pin NAME[bus]? : KIND [DIR] [virtual] [when EXPR] [@metadata(...)] ;
```

- **KIND** ∈ `signal`, `power`, `ground`, `switch`, `feedback`.
- **DIR** ∈ `in`, `out`, `inout` (a `ground` pin takes no direction).
- **`virtual`** marks a *logical* pin with no package copper — a datasheet
  application-circuit node (e.g. a regulator's `VOUT`) that the entity's
  expansion produces. Wiring a virtual pin hands the support circuit to the
  entity's `expansion`; leaving it unwired signals the board authored that
  circuit itself (see §8 and [Synthesis_Auto_Expansion.md](Synthesis_Auto_Expansion.md)).
- **`when EXPR`** conditions a pin on a generic parameter
  (`pin EN: signal in when HAS_EN;`).

Pin names may be identifiers or numbers (`pin 1:`, `pin VCC:`).

Pin **kind + direction is electrical truth**, not decoration: `power in` marks
a supply input, `power out` a driver — the ERC rules and the DC solver key on
these. Declaring a regulator's input pin as `signal inout` (instead of
`power in`) hides it from every power-aware check.

### 6.3 Attributes

```bhdl
attribute NAME = EXPR;
```

Attributes carry the datasheet and toolchain-facing facts of a part:
`component_class`, `part_no`/`part_number`, `manufacturer`, `mpn`,
`kicad_symbol`, `output_voltage`, `i_quiescent`, `feedback_voltage`,
`efficiency`, `rds_on`, and so on. Values may be literals, quantities,
booleans, references to a parameter (`attribute resistance = value;`), or
computed expressions (the ternary table above). Attributes are **static**:
`attribute name = value;`. The canonical attribute *vocabulary* (the agreed
names like `manufacturer`, `mpn`, `footprint`, `component_class`) lives in
`bhdl-common::sku` and is documented in
[Product_Description_Model.md](Product_Description_Model.md). A richer *typed*
attribute system and a *behavioral* (per-timestep) attribute surface are
designed but not yet implemented — see
[BHDL_Attribute_Type_System.md](BHDL_Attribute_Type_System.md) and
[Unified_Attribute_System_Specification.md](Unified_Attribute_System_Specification.md)
(both marked aspirational).

### 6.4 Generics *(compile-time parameters)*

Angle-bracket generics are resolved at monomorphization, distinct from the
runtime constructor parameters:

```bhdl
entity LinearRegulator<V_OUT: voltage, HAS_EN: bool = false>(
    dropout: voltage = 2V,
    i_quiescent: current = 5mA
) {
    pin VI: power in;
    pin VO: power out;
    pin GND: ground;
    pin EN: signal in when HAS_EN;
    …
}

alias LM7805       = LinearRegulator<5V>;
alias LM1117_33_EN = LinearRegulator<3.3V, true>;
```

---

## 7. Parameters and Constructor-Argument Validation

### 7.1 Parameter list

```bhdl
entity Name(p1: type, p2: type = default, …) [where …] { … }
```

Each parameter has a name, a type (§5.1), and an optional default. Parameters
with defaults are optional at the call site. Ordering matters for positional
arguments.

### 7.2 Value domains — `where <param> in (...)`

A parameter whose legal values are a **closed set** declares that set in the
entity's `where` clause. The set is the parameter's *value domain*; supplying
anything outside it is a hard error (**E0403**), not a silently-ignored value:

```bhdl
entity MOSFET(part_no: string = "2N7000", channel: string = "nmos", …)
    where channel in ("nmos", "pmos")
{ … }
```

```bhdl
q1: MOSFET(part_no="FDN340P", channel="pmos")   // ✓
q2: MOSFET(part_no="FDN340P", channel="P")       // ✗ E0403
```

The domain check applies to both named and positional arguments (SKU aliases
pass the value positionally, `MOSFET("BSS84", "pmos", …)`), and resolves
across imports. Multiple parameters may each carry a membership clause,
comma-separated alongside numeric `where` constraints. The entity-level
`where` (after the parameter list, before `{`) is distinct from the
connection-level `where` of §3.3.

**Declare a domain** when an unlisted value is a *mistake* the toolchain would
otherwise swallow silently (a wrong polarity, an unrecognized footprint
style). **Leave it open** when unlisted values are *valid but uncommon* and a
sensible default applies — e.g. `LED(color)` maps common colors to a forward
voltage but orange/amber/UV LEDs are all real, so its color is deliberately
unconstrained.

### 7.3 The constructor-argument error family

Every argument must bind to a declared parameter and satisfy any declared
domain. Two hard errors gate this, both stopping synthesis before a netlist
is produced — so a design never ships carrying an argument the part never
received:

| Code | Condition | Example |
|------|-----------|---------|
| **E0402** | An argument names no declared parameter, or supplies more positional arguments than the entity has parameters. Emits an edit-distance "did you mean". | `Res(1kΩ, tolernce=1%)` → *did you mean `tolerance`?* |
| **E0403** | An argument value falls outside the parameter's declared domain. | `MOSFET(channel="P")` → not one of `("nmos", "pmos")` |

Unknown *entity types* are left to symbol resolution and never flagged here. A
reserved namespace stamped by the toolchain itself — `supply_*` / `i_supply`
(supply-synthesis metadata read by sign-off and ERC) and `expansion_*` /
`vpin_parent` (expansion provenance) — is exempt from E0402: these are
machine-authored attribute passthrough, not user parameters.

Before this validation existed, an unrecognized argument passed silently
through to the instance as a dead attribute nothing read — a
`Res(2.5Ω, wattage=10W)` whose "10 W" evaporated while the part kept its
0.25 W default. Promoting the legitimate ones to real parameters (like
`wattage`) and rejecting the rest is the Real-Data Policy applied to the
call site.

---

## 8. Entity Synthesis Blocks

*(`expansion` / `design` / `simulation`)*

Three optional sibling blocks let an entity carry its own application circuit,
its sizing math, and its simulation IP — so that adding a part to the library
is the entire change needed to support it ("adding a part IS extending the
tool"). The core toolchain stays generic; the device-specific knowledge lives
in HDL beside the entity.

### 8.1 `expansion { }` — the application circuit

An entity with a `virtual` pin fires its `expansion` block once per instance,
materializing the datasheet support components. Child components are declared
and wired just like board flows; `internal NAME: net;` declares a local node:

```bhdl
expansion {
    VIN  -> C_in:  Cap(0.33µF).1;  C_in.2  -> GND;
    VOUT -> C_out: Cap(0.1µF).1;   C_out.2 -> GND;
}
```

A parent pin named in a child statement joins the same board net the parent
pin sits on; a sibling child is referenced by handle. **Conditional gating**
keeps a child only when the pins it touches are mandatory support pins,
board-wired, or referenced by multiple children — so an unused peripheral's
support parts don't materialize. Support instances are stamped
`expansion_parent = <parent>`, which groups them on the parent's schematic
sheet and lets the parent's `stress` block sign them off. Full mechanism:
[Synthesis_Auto_Expansion.md](Synthesis_Auto_Expansion.md).

The virtual-pin/board-authored boundary is load-bearing: wiring a virtual
output pin hands the output circuit to the expansion; leaving it unwired while
wiring the physical path (`SW`, `FB`) tells the toolchain the board authored
that circuit itself, and the expansion's output-side children are suppressed.
ERC029/ERC030 (§12) backstop the mistakes this boundary can produce.

### 8.2 `design { }` — sizing math

A `design` block computes support-component values from the instance's
parameters. It reads `self.<param>` and produces values consumed by the
expansion (by child name or `<name>_value`), guarded by `require`:

```bhdl
design {
    const v_ref = 1.25;
    require self.v_out >= 1.35
        else "LM317 minimum V_OUT is V_REF + headroom (~1.35V)";
    const delta = self.v_out - v_ref;
    r2_value = 240.0;
    r1_value = delta * (240.0 / v_ref);
}
```

An intent-scoped form `design for <intent> { inputs {…} outputs {…} … }`
supplies the analytic first guess for an intent-driven flow; the core's
generic refine loop (build → solve → measure → adjust, bounded) polishes it.
For logic the declarative surface can't express, a sandboxed Rhai escape
hatch is available:

```bhdl
design for amplifier {
    inputs  { tube; intent; supply; }
    outputs { Rp; Rk; }
    body rhai r#"
        let v_p = supply.VBB / 2.0;
        // … analytic bias math …
        #{ Rp: v_p / i_p, Rk: (-v_gk) / i_p }
    "#
}
```

Rhai is statically linked, sandboxed (no I/O, network, or spawn), and
fuel-limited for determinism. See
[Vendor_Design_Blocks.md](Vendor_Design_Blocks.md).

### 8.3 `simulation { }` — device simulation IP

`simulation` holds up to three sub-blocks that brief the generic circuit
solver (GLACIER) on this specific device. GLACIER never grows a match on part
names; when it needs a device specific, it reads the entity.

- **`stress { }`** *(shipped)* — how this device stresses its support parts.
  It assigns per-child stress axes (`L_out.i_peak`, `C_out.v_ripple`) from the
  solved operating point (`vin`/`vout`), the rail's declared load current, and
  the device's own parameters:

  ```bhdl
  simulation {
      stress {
          const duty = vout / vin;
          const d_il = (vin - vout) * duty / (self.switching_frequency * L_out.value);
          L_out.i_peak   = i_out + d_il / 2;
          C_out.v_ripple = d_il / (8 * self.switching_frequency * C_out.value);
      }
  }
  ```

- **`model { node NET draws = EXPR; }`** *(shipped for draws; richer device
  models specified, build deferred)* — declares a supply-draw current source
  so a load's current is a *measured* value in the DC solve, not an assumption:

  ```bhdl
  simulation { model { node VCC draws = self.i_active; } }
  ```

- **`stability { }`** *(specified, build deferred)* — a control-loop model for
  closed-loop margin sign-off.

See [Vendor_Simulation_Blocks.md](Vendor_Simulation_Blocks.md).

### 8.4 `check { }` — part-carried ERC rules

Inside `simulation`, a `check` block ships design rules with the part
(ERC tier 2, §12). Each rule is `require <predicate> else "message";`, where
predicates extend the expression grammar with netlist queries
`connected(PIN)`, `exists(CHILD)`, `same_net(P1, P2)` and tolerance-aware
comparison of `self.<attr>` / `<child>.value`:

```bhdl
check {
    require connected(BOOT)
        else "BOOT needs the 100nF bootstrap cap to SW/PH (datasheet 7.3.1)";
    require c_boot.value == self.bootstrap_capacitor
        else "the bootstrap cap must be the datasheet 100nF (7.3.1)";
}
```

An unresolvable predicate *skips* rather than fails (Real-Data). A part can
only reference its own pins — it cannot know board rail names.

---

## 9. Symbols, Layout, Interfaces, Aliases

### 9.1 `symbol` and `layout`

An entity may ship its schematic symbol layout and its footprint:

```bhdl
symbol TPS54302 {
    body rectangle;
    left   { VIN, EN }
    right  { SW, PH, FB }
    top    { BOOT }
    bottom { GND }
}

layout TPS54302 {
    package SOT-23-6;
}
```

The symbol's edge groups drive pin placement in the schematic renderer (§14).

### 9.2 `interface` — peripheral protocol bundles

An interface is a named signal group (SPI, I2C, UART, USB, DDR) with per-role
*perspectives* and per-instance pin bindings. On an entity, an `interface`
field binds protocol signals to physical pins:

```bhdl
interface SPI spi {
    MOSI = PB3;
    MISO = PB4;
    SCK  = PB5;
    CS   = PB2;
}
interface ICSP:slave icsp { … }   // :role selects a perspective
```

Interface *definitions* carry the perspectives, optional `wires { … }`
cross-name mappings (for protocols like UART where TX↔RX), and
protocol-derived `constraints { }`. Doctrine: constraints belong on the
interface (the protocol), not the chip. Full model (v0.8, shipped, incl.
parametric interfaces and hierarchical sub-interfaces like `DiffPair`):
[Interfaces.md](Interfaces.md).

### 9.3 `aliases` — logical pin names

Inside an entity, an `aliases` block maps friendly names to datasheet pins,
resolved at connection time (the netlist keeps physical names):

```bhdl
aliases {
    gpio0 = PD0;   // RXD
    reset = PC6;   // /RESET
}
```

### 9.4 Top-level `alias`

Three forms, all in stdlib use:

```bhdl
alias Resistor = Res;                                    // plain rename
alias LM7805   = LinearRegulator<5V>;                    // bind generics
alias MOSFET_IRF540 = MOSFET("IRF540N", "nmos", 100V,    // bind ctor args (SKU)
                             33A, 44mΩ, 4V, 130W);
```

---

## 10. Ports, Power, and Supply Synthesis

### 10.1 Rail declarations

Three declarations introduce power/ground nets in a board:

```bhdl
power VIN = 12V @ 3A;              // internal rail, with load budget
ground GND;                       // ground reference
port  VCC_5V: power out = 5V @ 2A; // boundary port — direction tells the truth
```

A **`port`** is the board's honest boundary object. Its direction is a claim
about physical reality:

- `power in` — energy *enters* here (through a connector/jack); the source is
  external.
- `power out` — the rail is *generated on-board* (by a regulator); declaring a
  generated rail `in` claims a supply that doesn't exist.
- `ground` / `signal in|out|inout` — as expected.

ERC028 (§12) enforces this: a generated rail with no on-board driver, or a
power-in port whose net touches no connector, is an error. `power X`/`ground X`
desugar to ports internally.

### 10.2 `supply` — declarative power synthesis

The `supply` statement asks the toolchain to synthesize a regulator stage
between two rails, rather than hand-authoring it:

```bhdl
supply @VCC_5V from @VIN_12V {
    ripple_max: 50mV;
    profile:    efficiency;      // cost | grade | balanced | efficiency
}
supply @VCC_3V3 from @VCC_5V {
    ripple_max: 30mV;
    using: TPS54331;             // explicit part (escape hatch)
}
```

The rails already carry the operating point (`v_out`/`i_out` from the target,
`v_in` from the source); the block adds only what a rail cannot express —
ripple/efficiency/quiescent limits, a ranking `profile`, or an explicit part.
A target rail with no `@ I` load is a hard error (Real-Data).

`supply` **desugars** to exactly the hand-written instantiation: it selects a
regulator from the stdlib catalogue (datasheet attributes are the capability
declaration; hard gates on input range, reachable output, load rating,
dissipation, ripple; soft ranking per profile), emits the datasheet support
circuit with E-series-snapped values, and threads the requirements as
attributes that sign-off reads back. Cascaded supplies compose into trees,
each stage stamping its computed input draw so downstream rail budgets are
real. Supplies sharing a source share one input bank. Full model (S1–S4c):
[Power_Supply_Synthesis.md](Power_Supply_Synthesis.md).

### 10.3 Staged power flow `|>` *(rare)*

The grammar supports a staged power-flow operator for expressing a rail's
processing chain (`power VIN |> fuse() |> filter() |> …`); the corpus expresses
the same structure with ordinary flows and `supply`.

---

## 11. The Data-Honesty Doctrine (Real-Data Policy)

The single most load-bearing principle in the toolchain: **every quantity
consumed by analysis, sign-off, or part selection must come from real data for
the actual part.** No fabricated defaults, no typical-value tables, no
estimated package/dielectric numbers, and never a *requirement* used as if it
were a *measurement*.

When a required value is unavailable, the toolchain does not synthesize one —
the dependent result reports **UNCHECKED**, naming the missing datum, and a
part that cannot supply the data is not preferred over one that can. The
pressure goes on the catalogue and the vendor to publish data, never on the
tool to invent it.

Concretely, this doctrine appears as:

- **Solver/sizing**: no `.unwrap_or(0.3)` default ripple ratios or V_ref; a
  regulator missing a loss-model constant is a hard error.
- **Sign-off**: an unverifiable axis renders as UNCHECKED, not "passed"
  (the ERC024 absence ledger, §12; deliberately not waivable).
- **Sourcing**: an over-stressed part is left UNPOPULATED with a loud warning,
  never auto-swapped for an under-rated one.
- **Library**: a recipe edited in place with no version bump is caught by the
  content hash (§13), never silently substituted.
- **Schematic**: a part the renderer can't idiomize is drawn plainly in a
  residue region and counted, never hidden (§14).
- **Call site**: an unrecognized constructor argument is rejected, not
  swallowed as a dead attribute (§7).

Full statement: [Real_Data_Policy.md](Real_Data_Policy.md).

---

## 12. Electrical Rule Checking (ERC)

ERC is organized in three tiers by where the knowledge lives:

- **Tier 1 — core rules**: universal physics/topology, an in-tree Rust
  registry; individually toggleable, thresholds configurable. Hardcoding these
  is correct — they are the semantics of electricity.
- **Tier 2 — part-carried rules**: the entity's `check { }` block (§8.4),
  evaluated per instance. No central registry; adding a part adds its rules.
- **Tier 3 — policy plugins**: external executables (JSON over stdio,
  `BHDL_ERC_PLUGINS`) that receive a design summary (instances carry both
  handle and refdes) and return findings. A broken plugin becomes one visible
  warning, never fabricated errors.

Cross-cutting: severity gating (`--erc-fail-on error` → build fails, exit 3);
reasoned waivers (`attribute erc_waive = "ERC016: reason"`, printed in a
separate table — a waiver hides nothing); every finding anchors on the handle
and lands in a `## Design rule check` table with numbers and a suggested fix;
a rule that cannot resolve its inputs *skips* rather than inventing a result.

**Tier-1 catalog** (all built):

| Rule | Checks |
|------|--------|
| ERC001 | Driver conflict — ≥2 push-pull outputs on a net (Error); input-only net (Warning) |
| ERC002 | Differential-pair polarity — P and N on one net |
| ERC003 | UART not crossed — TX↔TX / RX↔RX |
| ERC004 | Cross-voltage-domain signal without a level shifter |
| ERC005 | I2C pull-ups missing, or to the wrong rail |
| ERC006 | Floating input — `signal in` with no net (Warning) |
| ERC007 | Unpowered part — `power in` with no net (Error) |
| ERC008 | Single-pin net — a typo'd net name (Warning) |
| ERC009 | Rail shorted to ground |
| ERC011 | Orphan passive — an unconnected passive pin (Warning) |
| ERC016 | Rail-budget overload — Σ declared draws vs the rail's `@ I` (Error) |
| ERC017 | Regulator below dropout — `V_in < V_out + dropout` (Error) |
| ERC018 | Absolute-max input exceeded |
| ERC019 | Reversed polarized cap — using solved DC potentials (Critical) |
| ERC020 | Missing decoupling on an active part's rail (Info) |
| ERC022 | Intent contradiction — placed filter cutoff vs a declared `for` intent |
| ERC023 | Precision-path grade mismatch — tolerance coarser than a declared accuracy |
| ERC024 | Unchecked-axis visibility — the absence ledger (Info; not waivable) |
| ERC026 | Interface completeness — half-wired I2C/SPI |
| ERC027 | Op-amp stage-gain consistency — derived vs measured vs declared |
| ERC028 | Rail anchoring — a rail needs a boundary port or on-board driver |
| ERC029 | Floating duplicated support circuit — an expansion island on a virtual pin |
| ERC030 | Board part shadows an expansion child — a double-authored circuit (Warning) |
| ERC031 | Feedback divider contradicts declared rail — `VREF·(1+Rtop/Rbot)` vs `power VOUT` >10% (Error) |

ERC025 is the tier-2 surface itself (part-carried `check {}` blocks). Full
architecture and rule detail: [ERC.md](ERC.md).

---

## 13. Sign-off, BOM, and Freeze

### 13.1 Requirement sign-off

`bhdl <board> bom --simulate` runs the full sizing/verification loop and emits
a sign-off table. Each spec axis (a rail's ripple, a regulator's dropout, a
part's stress margin) is checked against the **as-built, snapped** values, not
satisfied by construction. Margins use `slack = rating − derated_stress` and
`margin = rating / derated_stress`, banded OVER_STRESS / UNDER_MARGIN /
SIGNED_OFF. A part that cannot reach sign-off through E-series stepping is left
DNP with a reason rather than silently populated under-rated; an uncomputable
axis reports UNCHECKED. See
[Simulation_Margin_Signoff.md](Simulation_Margin_Signoff.md).

### 13.2 Parameter and BOM resolution

Passive part-defining axes (value, tolerance, package) are *generic*
parameters — each combination is physically a different orderable part —
while same-part configuration (`LM317(v_out=5V)` — one MPN) is a runtime
argument. A `part_family` declaration is the parametric MPN generator that
turns a bound-generic class into concrete MPNs; a supply-chain plugin (JSON
over stdio) then resolves the actual orderable part and its sourcing. BHDL
emits "what's possible"; the plugin returns "what to buy." Selection is a
multi-objective cost function with hard stress gates; an over-stressed part is
left unpopulated, never downgraded. See
[Parameterization_And_BOM_Resolution.md](Parameterization_And_BOM_Resolution.md)
and [Supply_Chain_Plugins.md](Supply_Chain_Plugins.md).

### 13.3 Library resolution, lockfile, and freeze

Library dependencies are declared (Cargo-shaped) and pinned in a committed
`bhdl.lock` with a sha256 content hash over each library's sources — catching
the dangerous case of a recipe edited in place with no version bump. Builds
refuse to proceed on undeclared drift. `bhdl <board> freeze` emits the frozen
structural netlist: resolved values/footprints/MPNs and flat connectivity
after all expansion and inference, with provenance — the immutable as-fabbed
record (a snapshot, not rebuildable source). See
[Library_Resolution.md](Library_Resolution.md) and
[Source_Resolvers.md](Source_Resolvers.md).

### 13.4 Board SKU variants

A single PCB ships as multiple SKUs via board-level `variant` blocks, each a
patch on the base carrying value overrides (`inst.value = expr;`) and
do-not-populate (`dnp inst;`). A DNP part stays in the structural netlist (PnR
keeps its footprint) but every electrical/BOM consumer skips it — SPICE
excludes it (a missing R/C is an open circuit). A netlist-consuming command
errors rather than silently picking a SKU when variants exist (`--sku Name`,
`list-skus`). See [Board_SKU_Variants.md](Board_SKU_Variants.md).

---

## 14. Schematic Visualization

`bhdl <board> visualize` produces a schematic. Its guiding doctrine: **a good
schematic is a composition of electrical idioms, not a laid-out graph** — three
prior generic graph-layout attempts failed, and the engine deliberately uses
semantic facts a layout tool can't recover: pin roles set IC symbol sides;
rails and the supply tree become horizontal buses in power-up order;
`expansion_parent` groups become pre-composed datasheet-figure blocks; net
classes route ground down and signals left-to-right.

Layout is computed in Rust (`bhdl-schematic/src/v4/`) emitting positioned SVG;
the HTML shell adds only pan/zoom/hover, so the result is deterministic and
unit-testable. Hierarchical boards render as a sheet tree with drill-down
links and a print-ready PDF binder. Real-Data applies to drawing too: a part
the classifier can't idiomize is drawn plainly in a *residue region* with net
flags and counted ("N components unidiomized") — a machine-checkable absence
ledger, so a clean corpus render is zero unidiomized and zero label
collisions. See [Schematic_V4.md](Schematic_V4.md).

---

## 15. Handles vs Reference Designators

BHDL keeps two strictly separate namespaces:

- A **handle** is the human name — user-authored, descriptive, stable because
  it *is* the source text (`u_buck`, `input_bulk_cap`; synthesis-minted
  instances get derived handles like `U1_C_out`). Handles name source wiring,
  nets, logs, and ERC anchors.
- A **refdes** is the fab name — `R1`, `C3`, `U5`, allocated by the toolchain
  for the BOM, silkscreen, and schematic labels.

Refdes allocation happens exactly **once**, at synthesis phase 12.7 (after all
instance-minting, before DRC): a deterministic name-sorted walk stamps the
`refdes` attribute and persists the handle→refdes map in a committed
`<board>.bhdl.refdes` sidecar — a lockfile analogue, so a part keeps its
designator across edits and machines. Every consumer (schematic, BOM, sign-off,
freeze, ERC plugins, PnR) *reads* that attribute; none allocates its own, which
is how a schematic's `R1` and a BOM's `R1` are guaranteed to be the same
physical part. Sign-off tables print `handle (refdes)`. See
[Handles_And_Refdes.md](Handles_And_Refdes.md).

---

## 16. The Toolchain (CLI)

`bhdl-cli <FILE> <command>`:

| Command | Purpose |
|---------|---------|
| `parse` | Parse and check syntax |
| `analyze` | Analyze for errors and warnings (runs the E0402/E0403 checks) |
| `synthesize` | Synthesize the netlist |
| `visualize` | Generate the schematic (HTML/SVG; `--binder` for PDF) |
| `spice` | Run SPICE/GLACIER analysis |
| `simulate` | Run a testbench simulation |
| `bom` | Generate the BOM (`--simulate` runs sizing + sign-off) |
| `report` | Full synthesis report (requirements → sign-off → BOM) |
| `freeze` | Emit the frozen as-fabbed netlist |
| `layout` | PCB place & route, export KiCad PCB |
| `intents` | Analyze design intent and flow tracking |
| `doc` | Generate power-domain documentation |
| `list-skus` | List the board's declared SKU variants |
| `pipeline` | parse → analyze → synthesize → visualize |

Netlist-producing commands refuse to build on a constructor-argument error
(§7) or, with `--erc-fail-on error`, on an ERC error (§12). Variant boards
require `--sku`. Reproducibility flags: `--locked` (CI, no lock changes),
`--update-lock`, `--offline`.

---

## 17. Grammar Reference

This section is a condensed, authoritative summary; the parser
(`bhdl-parser/`) is the definitive grammar.

### 17.1 Top-level items

`import`, `entity`, `board`, `alias`, `typedef` (empty body), `type`, `const`,
`enum`, `interface`, `trait`, `impl`, `part_family`, `symbol`, `layout`,
`safety_goal`, `fault_inject`, `testbench`.

```
import { A, B } from "lib/path.bhdl";        // destructuring import
import a.b.c [as Alias];                     // dotted import
```

### 17.2 Operators and precedence

Lowest → highest binding power: `|>` `<=>`  ·  `<->` `->`  ·  `||`  ·  `&&`  ·
`|`  ·  `^`  ·  `&`  ·  `== != < > <= >=`  ·  `+ -`  ·  `* / %`  ·  unary
`+ - ! ~`. Ternary `cond ? a : b` sits just above `&&`. Flow operators
(`-> <-> |> <=>`) form flat sibling chains; arithmetic left-nests. `<-`
(reverse arrow) and `<< >>` are lexed but not usable in board/flow syntax.

### 17.3 Reserved words

Types: `signal power ground switch feedback clock wire`. Directions:
`in out inout input output`. Structure: `board entity interface typedef type
enum struct match trait impl const import alias part_family symbol layout
testbench safety_goal fault_inject`. Items/modifiers: `pin port attribute
generate for if else when where with require optional virtual extends as
satisfies via expansion internal design variant dnp socket placement
power_domain aliases`. Booleans: `true false`. Contextual (matched by text, not
reserved): `from to supply simulation stress model check inputs outputs body
rhai near each distributed`.

Note: several keywords are lexer-reserved but have no production and are
inert — `assign`, top-level `component`, statement-form `net`, `connect`,
`parameter`, `layer`, `constrain`. Do not use them.

### 17.4 Value grammar

A value is an optional sign, a number (with `_` grouping and scientific
notation), and an optional attached unit (§5.2); or a string, or `true`/`false`.
Raw strings `r#"…"#` carry Rhai `design` bodies. Bus/range suffixes:
`[expr]` index, `[expr:expr]` range.

---

## 18. Companion Specifications

This document is the language reference and map. The subsystems have dedicated,
current specifications:

| Topic | Document |
|-------|----------|
| Data-honesty doctrine | [Real_Data_Policy.md](Real_Data_Policy.md) |
| ERC architecture + full rule catalog | [ERC.md](ERC.md) |
| Handle vs refdes namespaces | [Handles_And_Refdes.md](Handles_And_Refdes.md) |
| `supply` statement + part selection | [Power_Supply_Synthesis.md](Power_Supply_Synthesis.md) |
| Auto-expansion / virtual pins | [Synthesis_Auto_Expansion.md](Synthesis_Auto_Expansion.md) |
| `design {}` blocks (sizing IP) | [Vendor_Design_Blocks.md](Vendor_Design_Blocks.md) |
| `simulation {}` blocks (device sim IP) | [Vendor_Simulation_Blocks.md](Vendor_Simulation_Blocks.md) |
| Sign-off and margins | [Simulation_Margin_Signoff.md](Simulation_Margin_Signoff.md) |
| Parameter / BOM resolution | [Parameterization_And_BOM_Resolution.md](Parameterization_And_BOM_Resolution.md) |
| Supply-chain plugins | [Supply_Chain_Plugins.md](Supply_Chain_Plugins.md) |
| Library resolution + lockfile | [Library_Resolution.md](Library_Resolution.md) |
| Source resolvers (auto-fetch) | [Source_Resolvers.md](Source_Resolvers.md) |
| Schematic engine | [Schematic_V4.md](Schematic_V4.md) |
| Interfaces (SPI/I2C/UART/DDR) | [Interfaces.md](Interfaces.md) |
| Attribute vocabulary (shipped) | [Product_Description_Model.md](Product_Description_Model.md) |
| Typed / behavioral attributes *(aspirational)* | [BHDL_Attribute_Type_System.md](BHDL_Attribute_Type_System.md), [Unified_Attribute_System_Specification.md](Unified_Attribute_System_Specification.md) |
| Net naming | [Net_Naming_Specification.md](Net_Naming_Specification.md) |
| Board SKU variants | [Board_SKU_Variants.md](Board_SKU_Variants.md) |
| Behavioral / dynamic models *(proposal)* | [Behavioral_Models.md](Behavioral_Models.md) |
| Product-description model | [Product_Description_Model.md](Product_Description_Model.md) |

### Status note

Shipped and exercised by the corpus: the full language core, entities with
`expansion`/`design`/`simulation{stress,model,check}`/`interface`/`symbol`/
`layout`, `where … in` value domains, ports, `supply` synthesis (through supply
trees and shared input banks), the ERC catalog (ERC001–ERC031), refdes
allocation, sign-off, freeze, and the schematic engine. Specified with partial
or deferred implementation: the `simulation { stability }` loop-margin surface,
`part_family`/`bom_preferences` catalog resolution, the `behavior {}` dynamic
simulation surface, and auto-fetch source resolvers beyond the built-in `git`
scheme. Those sections cite their companion doc's build status.

