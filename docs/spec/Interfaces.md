# Interfaces

> **Status:** Proposal v0.7. Scope: peripheral-protocol bundles
> (SPI, I²C, UART, USB, etc.) declared as named signal groups with
> per-role *perspectives*, optional cross-name *wire mappings*, and
> per-instance *pin bindings* that tie interface signals to physical
> pins on a chip. Covers the v0.1–v0.6 design that's already shipped
> plus the v0.7 perspectives surface that closes the cross-name gap.
>
> Out of scope for v0.7: parameterised interfaces (`SPI<width=16>`,
> grammar parses but unused), hierarchical sub-interfaces (`RGMII`
> containing nested `TX` and `RX` sub-interfaces), and timing
> constraints (`constrain setup(...)`). All deferred to v0.8+.
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
