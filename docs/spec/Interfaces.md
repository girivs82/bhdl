# Interfaces

> **Status:** Shipped through v0.8. Scope: peripheral-protocol
> bundles (SPI, I²C, UART, USB, DDR, etc.) declared as named signal
> groups with per-role *perspectives*, optional cross-name *wire
> mappings*, per-instance *pin bindings* that tie interface signals
> to physical pins on a chip, plus the v0.8 additions: **hierarchical
> sub-interfaces**, **parametric interfaces** (with generative
> loops), and **protocol-derived timing/electrical constraints**.
> Together these make a full DDR4 stack expressible (see §10).
>
> The v0.8 features earlier drafts listed as "deferred" are now all
> shipped:
>
> - **Parametric interfaces** (`SPI<lanes=4>`) — §11. Tier 1
>   (parameter substitution + signal-array expansion) and tier 2
>   (generative `generate for` loops, including list-literal
>   iteration and `(idx, val)` destructuring) both landed.
> - **Hierarchical sub-interfaces** (`RGMII` with nested `TX`/`RX`,
>   `DiffPair` as a reusable diff-pair) — §12.
> - **Interface constraints** (`constraints { … }`) — §13.
>
> **`require pullup`/`require decap`** (originally planned as a v0.8
> interface-syntax addition) was retired in favour of **vendor
> `expansion { }` blocks with conditional gating**. See
> [`Synthesis_Auto_Expansion.md`](Synthesis_Auto_Expansion.md) — the
> chip's stdlib entity owns the recipe and sources passives from
> the chip's own VCC pin, which avoids the multi-rail I²C-backdrive
> failure an interface-level `require` would have to solve
> separately. (Decision recorded 2026-05-28.)

## 1. Motivation

Boards connect chips. Most chip-to-chip wiring is *bundles* of
related signals — SPI's MOSI/MISO/SCK/CS, I²C's SDA/SCL, UART's
TX/RX. Writing each signal connection separately on every board:

- bloats the board source,
- couples board authors to chip-side pin numbering,
- gives no help when a signal direction is wrong or a pin is over-
  committed to multiple peripherals.

Interfaces let a board author state intent at the protocol level —
"this MCU's SPI talks to this flash's SPI" — and have the
synthesizer expand to the correct per-signal connections, route
through the chip's datasheet pin map, and reject wiring mistakes.

## 2. Interface definition

A top-level declaration. Lives anywhere a `board` or `entity` can
appear; conventionally in `bhdl-stdlib/interfaces/`.

### 2.1 Single-perspective form (v0.1)

For protocols that look the same from every endpoint's view:

```bhdl
interface I2C {
    signal SDA: inout;
    signal SCL: out;
}
```

The interface declares one implicit perspective named `default`.
Existing v0.1–v0.6 interfaces use this form; they continue to work
unchanged.

### 2.2 Multi-perspective form (v0.7)

Most real peripherals look different from each endpoint — SPI has
master and slave, UART has DTE and DCE, USB has host and device.
The interface declares one block per role:

```bhdl
interface SPI {
    perspective master {
        signal MOSI: out;
        signal MISO: in;
        signal SCK:  out;
        signal CS:   out optional;
    }
    perspective slave {
        signal MOSI: in;
        signal MISO: out;
        signal SCK:  in;
        signal CS:   in optional;
    }
}
```

Each `perspective NAME { … }` block is a role: its own signal
names and directions. The signal-name set across perspectives can
match (SPI, I²C, USB) or differ (UART — see §4 below).

The first-declared perspective is the **default** — the one used
when a field declaration omits the perspective. Order within a
perspective body is free.

### 2.3 Signal modifiers

Each `signal` declaration carries:

- **Name** (`IDENT`).
- **Direction** (`in` / `out` / `inout`), required.
- **Optional flag** (`optional` keyword). Optional signals can be
  omitted when binding; the synthesizer doesn't emit a net for
  them if absent.

```bhdl
signal CS: out optional;
```

## 3. Use-site selection

A field declaration inside an entity body picks one perspective of
the interface and gives the bundle a local name:

```bhdl
entity Foo {
    interface SPI         spi;          // default = first-declared (master)
    interface SPI:master  spi_b;        // explicit master
    interface SPI:slave   spi_c;        // explicit slave
}
```

Grammar:

```
INTERFACE_FIELD_DECL := 'interface' IDENT (':' IDENT)? IDENT field_body
field_body           := ';'
                      | '{' binding_list '}'    // pin bindings — see §5
```

- `interface IDENT IDENT;` — bare interface name, default
  perspective.
- `interface IDENT ':' IDENT IDENT;` — explicit perspective.

The colon form is the only way to opt into a non-default
perspective. There is no `~Interface` sugar in v0.7.

## 4. Wire mapping (cross-name perspectives)

For SPI and I²C, both perspectives use the same signal names.
Connection bundle expansion can pair them by name (master.MOSI
joins slave.MOSI — same wire, opposite direction at each
endpoint).

UART is the canonical *cross-name* protocol: the wire that the
master calls TX is the same wire the slave calls RX. By-name
pairing would be wrong. The interface declares the cross-mapping
explicitly:

```bhdl
interface UART {
    perspective dte {                   // host / master
        signal TX: out;
        signal RX: in;
    }
    perspective dce {                   // peripheral / bridge
        signal TX: out;
        signal RX: in;
    }
    wires {
        dte.TX <-> dce.RX;              // master's TX = slave's RX wire
        dte.RX <-> dce.TX;
    }
}
```

The `wires { }` block is **optional**. When omitted, the
synthesizer pairs signals across perspectives by *name*. When
present, it pairs them per the explicit `<->` mappings — the
exact same operator used for bidirectional connections in board
bodies.

Qualified-name form `perspective.signal` is required inside
`wires { }` because signal names may collide across perspectives.

The synthesizer's direction-compatibility check (v0.5) still
fires across these mappings, so forgetting `wires { }` on a
cross-name protocol surfaces loudly: master.TX (`out`) ×
slave.TX (`out`) → two-driver error.

## 5. Pin bindings (v0.4 recap)

A field declaration can attach a binding block that maps
interface signals to physical pins already declared on the entity:

```bhdl
entity ATmega328P {
    pin PB2: signal inout;
    pin PB3: signal inout;
    pin PB4: signal inout;
    pin PB5: signal inout;

    interface SPI:master spi {
        MOSI = PB3;
        MISO = PB4;
        SCK  = PB5;
        CS   = PB2;
    }
}
```

Effects:

- The interface signals are *aliases* for the bound physical pins,
  not separate pins. At connection time, `mcu.spi.MOSI` resolves to
  the same wire as `mcu.PB3`.
- An interface field with a binding block does **not** materialise
  additional pins on the entity's pin set.
- A field without a binding (peripheral side) materialises its
  signals as fresh pins (e.g. `flash.spi.MOSI`).

Binding signal names resolve against the *selected perspective's*
signal set. So `interface UART:dce uart { TX = PD1; RX = PD0; }`
on a peripheral entity uses the DCE perspective's TX/RX — and per
the UART `wires { }` block, those wires cross-map to the master's
RX/TX respectively.

## 6. Connection semantics

A board may write either form:

```bhdl
// Bundle form — preferred:
mcu.spi -> flash.spi;

// Per-signal form — still legal, useful when only some signals
// are wired:
mcu.spi.MOSI -> flash.spi.MOSI;
mcu.spi.MISO -> flash.spi.MISO;
mcu.spi.SCK  -> flash.spi.SCK;
```

The synthesizer's connection pipeline:

1. **Bundle expansion** (`expand_interface_bundle_chain`) — when
   every endpoint in a chain is an interface field reference,
   produces one parallel chain per signal in the bundle. Reuses
   `wires { }` to resolve cross-name pairings.
2. **Alias resolution** (`resolve_field_binding_alias`) — for
   bound fields, translates `instance.field.signal` to the
   underlying physical pin (`instance.PB3`) before the netlist
   lookup.
3. **Direction-compatibility check** (`check_chain_directions`,
   v0.5) — pairwise across endpoints of an expanded chain, rejects
   `out × out` (two drivers) and `in × in` (no driver). `inout` on
   either side passes.
4. **Field conflict detection** (v0.6, `detect_interface_field_conflicts`)
   — per instance, walks every interface field the board *uses*,
   groups the bindings by physical pin, and rejects multiple
   fields claiming the same pin.

## 7. Grammar reference

```ebnf
interface_decl := 'interface' IDENT '{' interface_body '}'

interface_body := (signal_decl | perspective_decl | wires_block)*

perspective_decl := 'perspective' IDENT '{' signal_decl* '}'

signal_decl := 'signal' IDENT ':' direction 'optional'? ';'

direction := 'in' | 'out' | 'inout'

wires_block := 'wires' '{' wire_mapping* '}'

wire_mapping := IDENT '.' IDENT '<->' IDENT '.' IDENT ';'

field_decl := 'interface' IDENT (':' IDENT)? IDENT field_body

field_body := ';'
            | '{' binding* '}'

binding := IDENT '=' (IDENT | NUMBER) ';'
```

## 8. Migration

| Today | Becomes |
|---|---|
| `interface SPI { signal MOSI: out; … }` (v0.1 single perspective) | Unchanged. Implicit `default` perspective. |
| `interface SPI spi;` at use site | Unchanged. Resolves to first-declared (or `default`). |
| `interface ~SPI spi;` at use site | **Replaced** by `interface SPI:slave spi;`. |
| Existing peripheral entities (`W25Q32`, etc.) | Update field decl to `:slave` form. |

The `~` token-handling in the parser stays available for
diagnostic purposes (recommend "use `:slave` instead") but no
longer participates in interface field declarations.

## 9. Decision log

- **Six surface decisions** were settled in a design conversation
  (see the discussion thread that produced this doc):

  1. **Explicit `perspective { }` blocks, no implicit overrides.**
     Top-level signal-only form remains valid as a single implicit
     perspective for v0.1–v0.6 back-compat. (Chose this over
     "primary signals at top + override blocks" — uniform reads
     more clearly across SPI and UART styles.)

  2. **`Interface:perspective` colon syntax at use sites.** Bare
     `Interface` defaults to first-declared. (Chose colon over
     `Interface(perspective: X)` keyword-arg form — concise,
     parser-unambiguous, no other meaning at that grammar site.)

  3. **Drop the `~Interface` sugar.** Once perspectives are
     explicit, `~` becomes ambiguous: what is it reversing —
     directions, names, both? `:slave` says what it is. (User
     observation: "when we have perspectives, `~` is confusing —
     what are you reversing?")

  4. **`wires { }` is optional.** By-name pairing is the default;
     declare `wires` only when names cross. The 90 % case (SPI /
     I²C / USB) gets zero ceremony.

  5. **`<->` operator inside `wires { }`.** Matches BHDL's existing
     bidirectional connection operator on board bodies. (Chose
     over `=` which reads more like an alias / assignment.)

  6. **Perspective + binding block compose freely.** Signal names
     in a binding resolve against the field's chosen perspective.
     No special syntax for the combination.

- **Direction-compatibility check (v0.5) is the safety net for
  forgetting `wires { }`.** A cross-name protocol declared without
  the wire map will have its signals collide as same-direction
  drivers, producing a clear error rather than silently bad
  wiring.

- **Pin bindings (v0.4) are the chip-vendor side of pinmux.** All
  candidate peripheral bindings live on the entity; the board's
  choice of which interface field to wire is the implicit pinmux
  selection. The v0.6 conflict check enforces "one pin can serve
  one role at a time."

---

## 10. v0.8 worked example: DDR4

A complete DDR4 byte-laned memory interface, exercising all three
v0.8 features at once. (Trimmed to 4-bit byte lanes for brevity;
real DDR4 uses 8.)

> **Shipped in stdlib.** This example is real, not illustrative:
> `bhdl-stdlib/interfaces/ddr4.bhdl` defines `DiffPair`, `DDR4Data`,
> `DDR4Ca`, and the parametric `DDR4<byte_lanes>` controller bundle;
> `bhdl-stdlib/actives/ddr4_sdram.bhdl` is the Micron MT40A x8 SDRAM
> entity (parametric over density) whose `expansion { }` block adds
> the 240 Ω ZQ calibration resistor + VPP/VDD/VDDQ decoupling +
> VREFCA bypass. `test_ddr4_stdlib` validates both the structural
> materialisation (37-pin controller, 33-pin SDRAM, constraints on
> every leaf) and the imported full-pipeline path (all six datasheet
> support components fire). One composition wrinkle surfaced and was
> fixed: the conditional-gating "always-on support pin" set had to
> learn `ZQ`/`VPP`/`VREF` — see
> [`Synthesis_Auto_Expansion.md`](Synthesis_Auto_Expansion.md) §3.1.
>
> Two deliberate differences between the snippet below and the
> shipped files: (1) the shipped `DiffPair` is **non-parametric** —
> the parametric resolver does not rewrite a parametric use-site
> *inside* a parametric template body, so a `DiffPair<z>` nested in
> `DDR4<byte_lanes>` wouldn't monomorphise; impedance is instead
> declared by the containing interface's constraints (`DQS.*:
> differential 80ohm`). (2) The shipped `DDR4Ca` uses explicit
> `CK_t`/`CK_c` signals rather than a `DiffPair CK` sub-interface,
> because it has perspectives and perspective + sub-interface
> composition is kept out of this first stdlib.

```bhdl
// A differential pair — reused for clock and every strobe.
interface DiffPair<z: int = 100> {
    signal P: inout;
    signal N: inout;
    constraints {
        *:       differential <z>ohm;   // `*` = the pair as a unit
        P -> N:  length_match 1ps;       // tight intra-pair skew
    }
}

// One byte lane: data bits + strobe + mask, with bit-swizzle freedom.
interface DDR4ByteLane {
    signal DQ0: inout; signal DQ1: inout;
    signal DQ2: inout; signal DQ3: inout;
    signal DM:  inout;
    interface DiffPair<z=80> DQS;        // 80Ω strobe pair
    constraints {
        DQ0, DQ1, DQ2, DQ3: single_ended 40ohm, signal_class DATA;
        DM:                 single_ended 40ohm, signal_class DM;
        // Bit swizzle: the strobe latches all data lines together,
        // so the router may permute DQ0..DQ3 + DM within the byte.
        DQ0, DQ1, DQ2, DQ3, DM: swizzle_within_byte true;
    }
}

// The whole protocol, parameterised by byte-lane count.
interface DDR4<byte_lanes: int = 8> {
    signal A0: out; signal A1: out; signal CS: out;
    interface DiffPair CK;               // 100Ω clock pair (default z)
    generate for i in 0..<byte_lanes> {
        interface DDR4ByteLane lane<i>;
    }
    constraints {
        CK.*:        signal_class CLOCK, max_freq 1600MHz;
        A0, A1, CS:  single_ended 50ohm, signal_class ADDR;
        // Byte swizzle: byte lanes train independently and may be
        // reordered as whole units.
        lane*:       swizzle_across_bytes true;
    }
}

entity MemController { interface DDR4<byte_lanes=4> ddr; }
```

What the synthesiser produces from this:

- **Parametric expansion** turns `DDR4<byte_lanes=4>` into a
  monomorphised `DDR4__byte_lanes_4` and unrolls the `generate for`
  into `lane0 … lane3`.
- **Hierarchical materialisation** flattens every nested interface
  into dotted leaf pins: `ddr.CK.P`, `ddr.lane2.DQS.N`,
  `ddr.lane0.DQ3`, …
- **Constraint propagation** attaches each protocol rule to the
  leaves it covers — `DiffPair`'s `differential 80ohm` lands on all
  four `ddr.laneK.DQS.{P,N}` pairs, the outer `CK.*` clock rules
  reach the nested `ddr.CK.{P,N}`, and the cross-bundle skew bounds
  (if declared) cross-product across endpoints.

Board-level **swizzle** is then expressed with the generalised
generate primitive — no swizzle-specific syntax (§11.3):

```bhdl
board {
    mc:  MemController();
    mem: MemoryChip();
    // Byte swizzle: the list literal IS the permutation table.
    generate for (j, i) in [2, 3, 0, 1] {
        mc.ddr.lane<j>.DQS -> mem.ddr.lane<i>.DQS;
    }
}
```

---

## 11. Parametric interfaces (v0.8)

An interface may declare integer parameters with defaults, used to
size signal arrays and unroll generative loops. Implemented as a
**source-text monomorphisation pass** (`parametric_resolver`) that
runs before the parser: each distinct argument tuple becomes a
mangled concrete interface, and use sites are rewritten to the
mangled name.

### 11.1 Parameter substitution + signal arrays (tier 1)

```bhdl
interface SPI<lanes: int = 1> {
    perspective master { signal SCK: out; signal CS: out; signal IO<lanes>: inout; }
    perspective slave  { signal SCK: in;  signal CS: in;  signal IO<lanes>: inout; }
}

interface SPI<lanes=4>:slave qspi;   // → SPI__lanes_4, IO0..IO3
interface SPI<lanes=8>:slave ospi;   // → SPI__lanes_8, IO0..IO7
interface SPI            :slave spi;  // → SPI__lanes_1 (default), IO0
```

- `signal NAME<N>: dir;` expands to `signal NAME0: dir; … NAME<N-1>: dir;`.
- Arguments may be **named** (`<lanes=4>`) or **positional**
  (`<4>`); defaults fill any unbound parameter.
- Unadorned use (`interface SPI …`) resolves to the all-defaults
  monomorphisation, so legacy non-parametric interfaces keep
  working unchanged.

### 11.2 Generative loops (tier 2)

```bhdl
interface DDR4<byte_lanes: int = 8> {
    generate for i in 0..<byte_lanes> {
        interface DDR4ByteLane lane<i>;
    }
}
```

`generate for VAR in ITER { body }` copies `body` once per element,
substituting `<VAR>` with the value. Two iteration sources:

- **Range**: `0..<N>` (bounds may be bare integers or `<N>`-wrapped,
  the form parameter substitution leaves behind).
- **List literal**: `[2, 3, 0, 1]` — iterates the values verbatim.

A paired binding `for (idx, val) in [...]` binds the first name to
the iteration index and the second to the current value (use `_` to
suppress either). Loops nest; the resolver unrolls outermost-first
and re-scans.

### 11.3 Top-level generates + swizzle

Generate-loop unrolling also runs over **board and entity bodies**,
not just inside parametric templates. This makes DDR byte/bit
swizzle expressible without any swizzle-specific syntax — the list
literal IS the permutation table:

```bhdl
// Byte swizzle: mc.lane0→mem.lane2, lane1→lane3, lane2→lane0, lane3→lane1
generate for (j, i) in [2, 3, 0, 1] {
    mc.lane<j> -> mem.lane<i>;
}
// Bit swizzle within a lane: nest the loops.
generate for (j, i) in [2, 0, 3, 1] {
    mc.lane0.DQ<j> -> mem.lane0.DQ<i>;
}
```

This is strictly more expressive than a chosen mapping: the
interface's `swizzle_*` constraints (§13) declare *what permutations
are legal*; the generate form realises *one specific choice*. Hybrid
use (bulk generate + hand-locked exceptions via a trailing explicit
connection, last-wins) works too.

---

## 12. Hierarchical sub-interfaces (v0.8)

An interface body may declare fields whose type is **another
interface**:

```bhdl
interface UartChannel {
    perspective dte { signal TX: out; signal RX: in; }
    perspective dce { signal TX: out; signal RX: in; }
    wires { dte.TX <-> dce.RX; dte.RX <-> dce.TX; }
}
interface DualUART {
    interface UartChannel ch0;
    interface UartChannel ch1;
}
```

Materialisation recurses, producing dotted leaf pins
(`duart.ch0.TX`, `duart.ch1.RX`). Sub-interface fields **inherit the
parent's perspective + `~` reversal** (xored), so `DualUART:dte`
resolves both channels as `dte`. Each sub-interface's `wires { }`
cross-name mapping is propagated onto the outer field's xwire
attributes, so a single bundle binding `mcu.duart -> per.duart` fans
out to the correct pairwise nets across every nested channel.

This is the foundation for diff pairs (`DiffPair { P; N }` reused
across CK/DQS/PCIe-lanes), multi-channel buses (RGMII's TX/RX
sub-bundles), and the DDR4 byte-lane stack in §10.

---

## 13. Interface constraints (v0.8)

A `constraints { … }` block inside an interface body carries
**protocol-derived** timing/electrical metadata — rules that come
from the protocol spec, not from any individual chip, so they belong
with the interface and apply to every chip that uses it.

```bhdl
constraints {
    *:               differential 100ohm;            // bundle-self target
    DQ0, DQ1, DQ2:   single_ended 40ohm, signal_class DATA;
    CK.*:            signal_class CLOCK, max_freq 1600MHz;
    P -> N:          length_match 1ps;               // pairwise relation
    CK -> lane0.DQS: skew_max 100ps;                 // cross-bundle relation
}
```

Three statement shapes:

1. **Per-signal**: `SIG[, SIG2, …]: prop1, prop2, …;`
2. **Pairwise relation**: `SIG_A -> SIG_B: prop;` (cross-products
   across resolved LHS × RHS targets).
3. **Group/wildcard**: targets may be a bare ident, a dotted path
   (`CK.P`), `*` (the interface as a unit), `IDENT.*` (everything
   under a sub-field), or a trailing-`*` wildcard (`DQ*`, `lane*`).

**Grammar is lenient and text-bearing** (CONSTRAINT_LHS / _RHS /
_PROPS hold uninterpreted token spans). The property vocabulary is
re-parsed synth-side, so new property names (`swizzle_within_byte`,
`topology`, …) need *no* grammar change — they flow through as
metadata.

**Storage.** Constraints become module attributes the downstream
PCB router / SI analyser / BOM walker reads by prefix:

```
intf_const__<pin_path>__<prop>           = <value>
intf_const_rel__<from>__<to>__<prop>     = <value>
```

**Inheritance** falls out of recursion: each interface applies its
*own* constraints at the depth where its leaves are materialised,
prefixed with the dotted path. `DiffPair`'s `*: differential 100ohm`
automatically reaches every `DiffPair` instantiation (CK, every
laneK.DQS) with no duplication.

> **Tier-1 storage is single-valued per (pin, property).** Two
> co-existing freedoms on the same pin (bit + byte swizzle) use
> *distinct property names* (`swizzle_within_byte` vs
> `swizzle_across_bytes`) to avoid last-write-wins collision.
> Multi-value storage, entity-level overrides, board-level
> additions, and cross-net conflict detection were deferred to
> tier-2, pending a real constraint consumer.

> **Tier-2 — partly landed (task #96).** With `bhdl-pnr` now the live
> consumer, the synth side stopped silently overwriting on a same
> `(pin, property)` collision. It keeps **every** contributor, resolves a
> winning value by **override precedence** (an explicit-pin target is
> `Specific` and beats a wildcard's `Interface` tier; same-tier ties go
> to the last writer), and writes that winner into the primary
> `intf_const__*` attribute exactly as before. The full contributor list
> — value + tier + source line + declaring interface name — is emitted
> **once per module** as a JSON
> [`ConstraintProvenanceMap`](../../bhdl-common/src/constraint_provenance.rs)
> under the `intf_const_provenance` attribute, so the router can render
> traceable diagnostics and flag same-tier contradictions. This is the
> "multi-value storage + override precedence + provenance" slice.
>
> Still deferred: **entity-level / board-level override blocks** (no
> grammar yet — the `Entity`/`Board` precedence tiers are reserved in
> `ConstraintTier` so adding the grammar won't reshape the wire format);
> the literal **source filename** in provenance (line + interface name
> ship now; the `.bhdl` path needs threading through the import loader);
> and **cross-net conflict detection** itself, which is structurally
> P&R-owned — a cross-net contradiction only exists after board
> net-merge, downstream of where the synth side emits these per-module
> attributes (P&R↔synth handshake §10/§11).

---

## 14. v0.8 decision log

- **Parametric interfaces as a text preprocessor, not a parser
  feature.** Monomorphisation is a transformation of source text;
  doing it before the parser keeps the AST and downstream synth
  ignorant of generics. Same architecture as the abstract-entity
  and import preprocessors. (Mirrors the v0.9b decision.)

- **Constraints belong on the interface, not the chip entity.** The
  rules (impedance class, length match, swizzle freedom) come from
  the *protocol*, so anchoring them to the interface means one
  authoritative definition instead of cut-and-paste across every
  SKU. Chip-specific tweaks (entity overrides) and design-intent
  additions (board) are tier-2 layers on top. (User framing:
  "constraints belong with the part that requires them" — refined
  to "the part is the protocol, not the silicon.")

- **Diff pairs are a hierarchical sub-interface, not a constraint
  annotation.** `interface DiffPair { P; N }` makes differential-ness
  structural; `differential Nohm` targets the pair-as-a-unit via the
  bundle-self `*`. No `differential_with OTHER` syntax, and one
  DiffPair definition serves USB/MIPI/PCIe/DDR. (User: "doesn't make
  sense to duplicate DQS, DQSn multiple times.")

- **Swizzle is generative iteration, not new syntax.** Rather than a
  board-level `swizzle { … }` block (rejected as too tedious), the
  generate-loop primitive was generalised to list-literal iteration
  + `(idx, val)` destructuring. The permutation table *is* the list
  literal. (User: "if we had an arbitrary generate loop, we don't
  need new features.")

- **Swizzle *freedom* vs swizzle *choice* are separate concerns.**
  The interface's `swizzle_*` constraints declare which permutations
  the protocol permits (preserving all 8! options for the router);
  a board-level generate realises one specific mapping. Bounded
  freedom is more powerful than a hard-coded choice.
