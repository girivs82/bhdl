# BHDL Interface Specification

> **SUPERSEDED — see [Interfaces.md](Interfaces.md), the normative
> interface spec.** This file was an early design draft. Most of the
> syntax it described was never implemented, or shipped in a
> different form; its examples do not parse with today's compiler
> and have been removed. The table below maps each of the draft's
> ideas to what actually shipped.

| Draft idea | Outcome | Shipped spelling (if any) |
|---|---|---|
| `require pullup(SIG, 4.7kΩ);` | **Shipped**, at three scopes (interface, entity, board body) with a net-level satisfier | `require pullup(SDA, 4.7k);` — Interfaces.md §9 "Pull requirements" |
| `require esd_protection(...)`, `require differential/impedance/...` | Dropped as executable vocabulary (any `require IDENT(args);` parses; only pullup/pulldown are consumed). ESD/support networks come from vendor `expansion { }` blocks | [Synthesis_Auto_Expansion.md](Synthesis_Auto_Expansion.md) |
| `role transmitter { … }` blocks, `as transmitter` at use sites | Shipped in different form: per-role **perspectives** | `perspective master { … }`; `interface SPI:slave spi;` — Interfaces.md §2.2/§3 |
| `~Interface` direction reversal | Dropped — hard parse error since v0.7c | `interface SPI:slave spi;` |
| `interface SPI(frequency = 1MHz)` parameterization | Shipped in different form: **angle-bracket generics**, monomorphised pre-parse by the parametric resolver | `interface SPI<lanes: int = 1> { … }`; `interface SPI<lanes=4>:slave qspi;` — Interfaces.md §11 |
| `constrain SCK.frequency <= …;` statements | Shipped in different form: **`constraints { … }` blocks** with a lenient, text-bearing property vocabulary | `constraints { CK.*: signal_class CLOCK, max_freq 1600MHz; }` — Interfaces.md §13 |
| Nested inline interface definitions (`interface RGMII { interface TX { … } }`) | Shipped in different form: **sub-interface fields** referencing top-level definitions (nested *definitions* are not supported) | `interface UartChannel ch0;` inside an interface body — Interfaces.md §12 |
| `extends` inheritance on interfaces | Dropped (no interface inheritance; `extends` exists only on `typedef`) | — |
| `signal TXD[4]: out;` bracket arrays | Shipped in different form: parametric signal-array expansion | `signal IO<lanes>: inout;` → `IO0..IO<N-1>` — Interfaces.md §11.1 |
| `signal CK_P, CK_N: out;` comma-grouped declarations | Dropped — one signal per declaration | `signal CK_P: out; signal CK_N: out;` |
| Interface object instantiation on a board (`bus: I2C();`) | Dropped — interfaces are not components. They are **fields on entities**, connected as bundles on the board | `interface I2C:master i2c;` (entity); `mcu.i2c -> sensor.i2c;` (board) — Interfaces.md §3/§6 |
| Brace-set connections (`mcu.{SDA, SCL} <-> bus.{SDA, SCL}`) | Dropped — bundle form or per-signal form | `mcu.i2c -> sensor.i2c;` or `mcu.i2c.SDA -> sensor.i2c.SDA;` |
| Inline transformation (`a <=> level_shift(3.3V, 1.8V) <=> b`) | Dropped — a level shifter is a real part instance | — |
| `capability …;` declarations | Dropped | — |
| Protocol state machines (`state` / `transition`) | Dropped | — |
| `domain: power;` inside an interface | Dropped — power domains are entity/board `domain` declarations; IO banks bind pins to rails | `domain VDDIO … io_pins="…";` — Interfaces.md §9 |
| Interface arrays (`mem_channel[4]: MemoryBus();`) | Dropped — generative loops cover the use case | `generate for i in 0..<N> { … }` — Interfaces.md §11.2/§11.3 |

> **Trap:** `interface Foo(param: int = 1) { … }` still **parses**
> (the parser reuses the entity parameter-list production) but is
> **semantically inert** — nothing reads paren-style interface
> parameters. Only `<…>` angle-bracket generics are resolved (by the
> pre-parse parametric resolver). Use `interface Foo<param: int = 1>`.
