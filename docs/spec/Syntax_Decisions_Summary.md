> **Early decision notes (2026-02), body rewritten 2026-08-30 to match the
> shipped grammar.** The high-level decisions (`:` for handles, `for`
> intents, no `net` keyword, `power`/`ground` declarations) hold. One early
> decision did NOT survive contact with the corpus: the mandatory `@` prefix.
> In the shipped grammar `@` is **optional** — see rule 1. The current,
> verified net model is in
> [BHDL_Complete_Specification.md](BHDL_Complete_Specification.md) §3.2.

# BHDL Syntax Decisions Summary

## Final Syntax Rules

### 1. Net References — `@` Is Optional

A net can be referenced with or without the `@` prefix. `@name`
*unambiguously* denotes a net (never a component handle); a bare identifier
resolves as a net when it is not a declared instance handle — **instance
names take precedence** over net names for bare identifiers. The corpus
mostly uses the bare form.

```bhdl
board NetRefs {
    // Power/ground declarations create nets
    power VCC = 5V @ 1A;         // creates net VCC
    ground GND;                  // creates net GND

    // @ is OPTIONAL on net references — both forms mean the same net
    @VCC -> r1: Res(10k).1;
    r1.2 -> led: LED("red").A;
    led.K -> GND;                // bare form

    // Named nets created inline (no declaration needed)
    VCC -> rf: Res(1k).1;
    rf.2 -> filtered;
    filtered -> cf: Cap(100n).1;
    cf.2 -> GND;
}
```

**Rationale**:
- `@` is available for disambiguation whenever a name could shadow an
  instance handle; it is never *required* on an unambiguous name.
- Power/ground are just nets with special attributes.

### 2. Component Handles - Use : Only

**The : operator is exclusively for component handles**:

```bhdl
// Create a component with a handle
@VCC -> r1: Res(10k).1;

// Reference the component via its handle
r1.2 -> led: LED("red").A;

// Anonymous components (no handle)
@VCC -> Res(4.7k).1 -> LED("blue").A;
```

**Rationale**:
- Clear distinction: : means component handle
- No confusion with other uses of :

### 3. No Connection Labels

**There is no label: syntax for connections** — a leading identifier before
a flow would read as a handle. Purpose is documented with a `for` intent
clause:

```bhdl
@VIN -> f1: Fuse(2A).1;
f1.2 -> protected for overvoltage_protection;
```

**Rationale**:
- Labels add confusion (is protection a component?)
- Intent clauses already document purpose
- Comments can provide additional documentation

### 4. Intent Syntax - No 'net' Keyword

**Attach intents using 'for' on flows and connections.** (An early draft
also sketched a named-flow statement — `flow name = …;` — that keyword was
**never implemented** and does not exist in the shipped grammar. The staged
power-flow operator `|>` on `power` declarations is the closest shipped
relative, and it too is unused by the realistic corpus.)

```bhdl
// Intent on a connection
@VIN -> f1: Fuse(2A).1;
f1.2 -> protected for overvoltage_protection;

// Intent on an anonymous flow
@VCC -> Res(10k).1 -> LED("red").A for power_indicator;
```

**Rationale**:
- Maintains flow-based philosophy
- No declarative 'net' keyword needed
- Natural left-to-right reading

### 5. Power/Ground Keywords Stay

**Keep 'power' and 'ground' keywords for declarations**:

```bhdl
board Example {
    // Clear power infrastructure
    power VIN = 12V @ 2A;
    power VCC = 5V @ 1A;
    ground GND;

    // Regulation between the rails is a real circuit, not a shorthand:
    VIN -> reg: LM7805().VIN;
    reg.VOUT -> VCC;
    reg.GND -> GND;
}
```

(Note: a direct rail-to-rail short like `@VCC -> @GND` is *syntactically*
possible but electrically wrong — ERC009 "rail shorted to ground" refuses
it. Rails are connected through real circuits, or via the `supply`
statement.)

**Rationale**:
- Self-documenting power infrastructure
- Clear board structure at a glance
- Power domains are nets with attributes

## Complete Example

All parts below are real stdlib entities (`Fuse`/`TVSDiode` from
`bhdl-stdlib/protection/tvs.bhdl`, `LM7805` from
`bhdl-stdlib/power/regulator.bhdl`, `LED(color: string)` from
`bhdl-stdlib/optoelectronic/led.bhdl`):

```bhdl
board ClearSyntaxExample {
    // Power declarations (create nets with attributes)
    power VIN = 12V @ 2A;
    power VCC = 5V @ 1A;
    ground GND;

    // Input protection — bare and @-prefixed net references both work
    VIN -> f1: Fuse(2A).1;
    f1.2 -> protected;
    protected -> tvs: TVSDiode(15V).K;
    tvs.A -> GND;

    // Intent without labels or a 'net' keyword
    protected -> reg: LM7805().VIN for voltage_regulation;
    reg.VOUT -> VCC;
    reg.GND -> GND;

    // Component handles use :
    @VCC -> r1: Res(330).1;
    r1.2 -> led: LED("green").A;
    led.K -> @GND;

    // Anonymous components in a chain
    VCC -> Res(10k).1 -> pulled_up;

    // Multiple connections to the same net
    VCC -> c_bulk: Cap(100uF).1;  c_bulk.2 -> GND;   // bulk cap
    VCC -> c_byp: Cap(0.1uF).1;   c_byp.2 -> GND;    // bypass cap
}
```

## Summary Table

| Syntax | Purpose | Example |
|--------|---------|---------|
| `name` / `@name` | Net reference (`@` optional; instance names take precedence for bare identifiers) | `VCC`, `@VCC`, `filtered` |
| `handle:` | Component handle | `r1: Res(10k)`, `reg: LM7805()` |
| `handle.pin` | Component pin | `r1.2`, `reg.VOUT` |
| `for intent` | Intent clause | `for overvoltage_protection` |
| `power/ground` | Net declaration | `power VCC = 5V @ 1A` |

## Key Principles

1. **One symbol, one meaning**: `:` always means component handle; `@`
   always means net (and is optional where the bare name is unambiguous)
2. **Flow-based**: No declarative keywords beyond power/ground
3. **Consistent**: Same rules everywhere in the language
