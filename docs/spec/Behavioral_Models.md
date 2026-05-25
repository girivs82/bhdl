# Behavioral Models for BHDL

> **Status:** Proposal v0.1. This document specifies the surface and
> semantics for behavioral modeling at the board level. Companion
> documents:
> `Vendor_Design_Blocks.md` (intent-driven operating-point design),
> `Board_SKU_Variants.md` (product configuration),
> `Product_Description_Model.md` (the architectural overview).

## Table of Contents

1. [Motivation](#1-motivation)
2. [Scope and explicit non-goals](#2-scope-and-explicit-non-goals)
3. [The two-domain model](#3-the-two-domain-model)
4. [The `behavior { }` block — surface](#4-the-behavior--block--surface)
5. [Event language](#5-event-language)
6. [Action language](#6-action-language)
7. [Testbench surface](#7-testbench-surface)
8. [Execution model and scheduler semantics](#8-execution-model-and-scheduler-semantics)
9. [Integration with existing BHDL features](#9-integration-with-existing-bhdl-features)
10. [Format translation](#10-format-translation)
11. [Worked examples](#11-worked-examples)
12. [Implementation phases](#12-implementation-phases)
13. [Open questions](#13-open-questions)
14. [Naming and grammar rationale](#14-naming-and-grammar-rationale)

---

## 1. Motivation

### 1.1 The class of bug structural + DC analysis cannot catch

BHDL today has:

- **Structural correctness** (the netlist is well-formed, every pin is
  connected, no shorts).
- **DC operating-point analysis** via GLACIER (the steady-state
  voltages and currents after the board settles).
- **Bill of materials** with full SKU data.
- **Design intent** that turns `for amplifier(gain: 14)` into sized
  passives at synthesis time.

None of these catch *system-level dynamic* failures. A non-exhaustive
list of real production-grade bugs they miss:

| Failure mode | Example | Cost when caught after fab |
|---|---|---|
| **Power-sequencing violation** | VDD_3V3 must rise within 1 ms after VDD_1V8 reaches 90 %; gets it wrong → MCU latches up | Board respin (~6 weeks, ~$50k) |
| **Backdrive** | USB host plugged in with target board off; VBUS forces current backward through clamp diodes; ESD diode burns out | Field RMA, reliability cost |
| **Reset-deassertion timing** | RESET# deasserts before VDD_CORE stabilises; MCU boots from random state intermittently | Field intermittent failures, recall risk |
| **Inrush** | Bulk-cap charge through PMOS at startup exceeds SOA briefly; sometimes survives, sometimes doesn't | "Works on the bench, fails in the field" |
| **Brownout recovery** | VIN dips below threshold, regulator drops out, comes back, sequencing missed | Random reboots in the field |
| **Power-good chain** | Reg1.PG enables Reg2; PG threshold mis-set means Reg2 never enables | Bring-up debug, often days |
| **I²C/SPI lockup** | Two masters, arbitration mis-configured, bus hangs | Days of field-debug to root-cause |
| **Mixed-signal glitch** | ADC samples mid-transition during digital edge; digital pipeline acts on garbage | Subtle, hard to reproduce |

Every line in that table is a real production failure mode. The bug
gets caught on the bench, or in the field, or never. The simulation
infrastructure board designers have access to today either:

- Doesn't model these dynamics (free open-source EDA stops at DC).
- Costs $10k–$200k per seat and requires a specialist (Cadence Sigrity,
  Allegro Sigrity PowerSI, Mentor HyperLynx, Ansys SIwave).
- Models *part* of these dynamics but not the system-level interplay
  (LTspice on a sub-circuit, but you can't easily simulate the
  whole-board power sequence).

### 1.2 What "behavioral" means here

Borrowing from Verilog-AMS at the IC level, **behavioral modeling**
means describing a component by what it *does* rather than by its
internal transistors. A regulator is "a 5 V output when input is
above 7 V, with a 5 ms ramp and a power-good signal that asserts 10
ms later." A USB transceiver is "a state machine over the LineState
inputs that fires certain outputs after timer durations." A
microcontroller is "a fixed program that asserts these outputs in
this sequence, given these resets and clocks."

The behavioral model trades fidelity (no transistor-level noise, no
exact AC response) for **system-level simulability**: a 50-component
board with multiple regulators, an MCU, a few PHYs, and external
loads can be simulated end-to-end in seconds instead of minutes,
because each component is a few equations and a state machine
instead of thousands of nodes.

### 1.3 Why Verilog-AMS at the IC level isn't enough at the board level

Verilog-AMS solves this problem for IC design. Why not just adopt it?

1. **Verilog-AMS is for IC silicon designers.** Its primitives
   (`nature`, `discipline`, `branch`, `idt()`, `ddt()`, contribution
   `<+`, transition `transition()`, etc.) are tuned for analog
   circuit blocks inside a chip. They're more than board designers
   need and structured around silicon abstractions board folk don't
   live in.
2. **Vendor models in Verilog-AMS exist but are gated.** Most
   Verilog-AMS models for commercial parts are NDA-locked behind
   simulator licenses. They're not in the open ecosystem.
3. **Tooling expects Cadence/Synopsys/Mentor flow.** Free Verilog-A
   simulators exist (Xyce, Verilog-A in ngspice via ADMS) but
   integration is fragile and the documentation assumes you have a
   six-figure simulator license.
4. **The board-level domain is simpler.** A regulator's behavioral
   model is "output = clamp(input − dropout, 0, target); PG = state
   machine on the input crossings." Verilog-AMS overshoots this
   need.

BHDL's behavioral surface is **Verilog-AMS-shaped, board-level-scoped,
free-and-open-by-construction**. It deliberately covers a narrower
range of physics (no `idt`/`ddt` arbitrary-equation solver; no
multi-physical domains; no RF/EM coupling) but a much broader range
of system-level dynamics (multi-component sequencing, mixed-signal
interactions, protocol-level checks).

---

## 2. Scope and explicit non-goals

### 2.1 In scope (v0.1)

- A **DSL** for declaring entity behavior: continuous-domain
  equations, state machines, event handlers, delayed actions.
- A **discrete-event scheduler** that runs board-level simulations
  in coordination with GLACIER's DC/transient solver.
- A **testbench surface** with stimuli (`apply`) and temporal
  assertions (`expect`, `forbid`).
- A **Rhai escape hatch** inside event handlers for arbitrary
  computation (DSP, complex stateful algorithms).
- **PSpice behavioral translation** — read B/E/F/G/H/LAPLACE
  elements, emit `behavior { analog { … } }` blocks.
- **IBIS as a subset** — import `.ibs` files as
  `behavior { analog { lookup_table(…) } state Off, Driving; … }`.

### 2.2 Out of scope (v0.1) — explicit non-goals

- **Arbitrary differential equations inside `analog { }`**. The
  `analog` domain expresses *static* relationships (e.g.
  `VOUT = clamp(VIN − 2, 0, 5)`). For dynamic behavior (RC
  discharge curves, real time-domain transient response of the
  device's silicon), use either:
  (a) the structural side — let GLACIER stamp real R/C/L
       passives,
  (b) the discrete-event side — model the dynamics as state
       transitions with `after <duration>` delays.

  Rationale: a real differential-equation solver mixed with the
  event scheduler is hard to get right. Verilog-AMS spent years
  iterating on this. BHDL v0.1 deliberately avoids the trap.
  Most board-level behavior is well-approximated by piecewise-
  linear analog + event-driven state. v0.2+ may add `idt()` and
  `ddt()` operators if the use case demands it.

- **Multi-physical domains.** No thermal, no mechanical, no
  optical. Pure electrical. (Thermal could be a v1.0 addition
  with a `thermal { }` sibling domain, but not now.)

- **RF / EM simulation.** Touchstone S-parameters, full-wave
  field solvers, etc. — these are different tooling.

- **Formal verification.** No SAT-style proof of properties;
  the testbench `expect` machinery is bounded *simulation* with
  assertions, not exhaustive checking.

- **Hardware verification languages.** No SVA (SystemVerilog
  Assertions) parity. The testbench assertion grammar is small
  and intentionally less expressive than SVA.

- **Mixed-language IPC.** No PLI / VPI / DPI shims for calling
  out to compiled foreign code. The Rhai escape hatch covers
  the legitimate "I need a real algorithm here" case.

- **Distributed-system simulation.** No multi-board, no
  network-of-boards. The testbench is one board at a time.

### 2.3 Adjacent features that *interact* with behavioral but are separate

- **Vendor design recipes** (`Vendor_Design_Blocks.md`): run at
  *synthesis time*, before simulation. Sizes the passives. The
  behavioral model then simulates the sized circuit.
- **Board SKU variants** (`Board_SKU_Variants.md`): applied to
  the netlist before behavioral simulation; variant-specific
  testbenches address `--sku Pro` etc.
- **GLACIER**: solves the analog operating point; the
  behavioral scheduler drives stimuli and queries node voltages
  through GLACIER's solver.

---

## 3. The two-domain model

A behavioral model lives in two domains that coexist and synchronise:

```
                ┌────────────────────────────────────┐
                │                                    │
   ┌────────────▼─────────────┐    ┌─────────────────▼─────────────┐
   │  Continuous-time analog  │    │   Discrete-event             │
   │                          │    │                              │
   │  V(net) = f(other Vs)    │    │   state = ...                │
   │  I(branch) = g(...)      │◀──▶│   on <event> { ... }         │
   │                          │    │   after <dur> => ...         │
   │  Solved by GLACIER       │    │   Scheduled by               │
   │  at each timestep        │    │   event engine                │
   └──────────────────────────┘    └──────────────────────────────┘
              ▲                                  ▲
              └─────── synchronise on events ────┘
```

### 3.1 Continuous-time analog

The `analog { }` block declares *equations* relating signals. They
hold continuously (at every simulation timestep). Each equation is
of the form `<output> = <expression>` where the right-hand side is
a function of node voltages, branch currents, internal state
variables, and time.

**v0.1 restriction**: expressions are *algebraic* — no `idt()`
integral, no `ddt()` derivative, no `transition()` ramp. The
equation is recomputed at every timestep based on the current
inputs; dynamic behavior comes from state machines instead.

Example:

```bhdl
analog {
    // Linear regulator: output tracks input minus dropout, clamped
    // at the target voltage. Holds at every timestep.
    VOUT = clamp(VIN − 2.0, 0.0, 5.0);

    // Quiescent current the regulator draws from VIN.
    i_quiescent = 5mA;
}
```

### 3.2 Discrete-event

The discrete-event domain consists of three things:

1. **State variables**: enums (`state Off, Regulating, ...`),
   typed scalars (`real timer_value`).
2. **Events**: moments in time when something happens. Built-in
   event types: signal crossings, threshold comparisons, edges,
   state matches, timer expirations.
3. **Handlers**: code that runs when an event fires. Handlers
   contain actions (signal assignments, state transitions,
   delayed actions, synthetic events).

Example:

```bhdl
state Off, RampingUp, Regulating, ThermalShutdown;
initial { state = Off; }

on VIN crosses 7.0 rising {
    state = RampingUp;
    after 5ms => { state = Regulating; }
    after 10ms => { PG = high; }
}

on VIN crosses 6.5 falling {
    state = Off;
    PG = low;
}
```

### 3.3 Synchronisation

The two domains communicate at well-defined points:

- **Continuous → discrete**: when an `analog` expression's value
  crosses a threshold named in an `on <signal> crosses <value>
  ...` event, the scheduler fires the corresponding handler at
  the exact crossing time.
- **Discrete → continuous**: when a handler changes a state
  variable that appears in an `analog` expression, the next
  timestep's analog solve uses the new state. (E.g. `state ==
  Off` in an `analog` expression evaluates to true/false; flipping
  the state changes the equation the solver sees on the next step.)
- **Signal assignments** from a handler take effect immediately
  (zero-time): if a handler sets `PG = high`, downstream entities
  that read `PG` see `high` at the same simulation instant.

Detailed scheduling rules are in [§8](#8-execution-model-and-scheduler-semantics).

---

## 4. The `behavior { }` block — surface

The `behavior { }` block is a sibling of `expansion { }`, `design { }`,
and `placement { }` inside an entity definition. v0.1 grammar:

```ebnf
behavior_block      = "behavior" "{" behavior_item* "}"

behavior_item       = analog_block
                    | state_decl
                    | initial_block
                    | event_handler
                    | timer_decl
                    | local_variable_decl

analog_block        = "analog" "{" analog_statement* "}"

analog_statement    = analog_assignment ";"
                    | conditional_analog

analog_assignment   = signal_name "=" expression
conditional_analog  = "if" expression "{" analog_statement* "}"
                      [ "else" "{" analog_statement* "}" ]

state_decl          = "state" ident_list ";"
ident_list          = identifier ("," identifier)*

initial_block       = "initial" "{" action_statement* "}"

event_handler       = "on" event_expression "{" action_statement* "}"

timer_decl          = "timer" identifier ";"

local_variable_decl = "let" identifier (":" type)? "=" expression ";"

action_statement    = assignment_action
                    | state_transition
                    | delayed_action
                    | trigger_action
                    | assertion_action
                    | rhai_escape
                    | conditional_action

assignment_action   = (signal_name | local_var) "=" expression ";"
state_transition    = "state" "=" identifier ";"
delayed_action      = "after" duration "=>" "{" action_statement* "}" ";"
trigger_action      = "trigger" identifier ";"
assertion_action    = "assert" expression ("," string_literal)? ";"
rhai_escape         = "body" "rhai" raw_string
conditional_action  = "if" expression "{" action_statement* "}"
                      [ "else" "{" action_statement* "}" ]
```

The `event_expression` and `expression` grammars are detailed in
[§5](#5-event-language) and [§6](#6-action-language) respectively.

### 4.1 `analog { }` — continuous-domain equations

`analog` blocks contain **static algebraic relationships**. Each
statement assigns to a signal name (typically an entity pin or an
internal variable). Expressions can reference:

- Entity pins: `VIN`, `VOUT`, etc. — referring to the node voltage at
  that pin.
- Internal state variables: any name declared via `let` or `state`.
- Time: the built-in `t` variable holds current simulation time.
- Mathematical functions: `clamp`, `min`, `max`, `abs`, `sqrt`,
  `exp`, `log`, `sin`, `cos`, `pow`, `if-then-else`.
- Lookup tables: `lookup(table_name, x)` interpolates a declared
  table.

**Multiple `analog` statements assigning to the same signal compose
last-wins** (later statements override earlier ones), so vendors
can write a base equation followed by conditional overrides:

```bhdl
analog {
    VOUT = clamp(VIN − 2.0, 0.0, 5.0);       // base equation

    if (state == ThermalShutdown) {
        VOUT = 0;                              // override on fault
    }
}
```

The simulator's resolution rule: at each timestep, the analog
solver computes the *final* value of each assigned signal after
applying every `analog` statement in source order.

### 4.2 `state` and `initial { }` — state machine

`state` declares an enum-shaped state variable. The simulator
allocates one slot per entity instance. The initial value is set
inside `initial { state = <Name>; ... }`.

```bhdl
state Off, RampingUp, Regulating, ThermalShutdown;

initial {
    state = Off;
    PG = low;
    let cycle_count = 0;        // local state variable
}
```

Multiple `state` declarations within one `behavior { }` block are
allowed and produce independent state variables (named after each
identifier in the list). For multi-FSM models, prefer multiple
`state` declarations over packing everything into one enum.

`initial { }` can contain any `action_statement`, including
`after <duration> =>` (useful for staggered startup behavior).

### 4.3 `on <event> { … }` — event handlers

The core of the discrete-event surface. A handler is registered
with the scheduler at elaboration time; whenever the event fires
during simulation, the body runs. See [§5](#5-event-language) for
the event grammar.

Handlers are **non-preempting**: if a handler is running and a
second event would fire, the second event is queued until the
first completes. This eliminates a class of race conditions.
Handlers also run **atomically with respect to the analog domain**:
the analog solver does not advance time while a handler is
executing.

### 4.4 `after <duration> => { … }`

Inside a handler, `after <duration> => { <actions> }` schedules
the inner actions to run at a future simulation time. Durations
are time literals: `1ms`, `100us`, `1ns`, `5s`. The future
firing is itself a non-preempting handler; multiple `after`
clauses fire in chronological order; same-time firings use
source-order tie-breaking.

```bhdl
on VIN crosses 7.0 rising {
    state = RampingUp;
    after 5ms  => { state = Regulating; }
    after 10ms => { PG = high; }
    // PG fires after Regulating because 10ms > 5ms.
}
```

`after` clauses can be cancelled by a `cancel` statement (future
feature) — for v0.1, an explicit state-machine pattern handles
cancellation:

```bhdl
on VIN crosses 6.5 falling {
    state = Off;       // any after-clauses checking state guard
    PG = low;
}

on VIN crosses 7.0 rising {
    state = RampingUp;
    after 10ms => {
        // Only assert PG if we're still ramping up — handles the
        // case where VIN dipped again during the 10ms window.
        if (state == RampingUp) {
            state = Regulating;
            PG = high;
        }
    };
}
```

### 4.5 `trigger <event>;`

Inside any handler, `trigger named_event` fires a synthetic event
by name. Listeners use `on named_event { ... }`. Synthetic events
support intra-entity coordination without going through pin signals.

```bhdl
on VIN crosses 7.0 rising {
    state = RampingUp;
    trigger startup_begin;     // synthetic
}

on startup_begin {
    // Cleanup behavior shared across multiple causes
}
```

### 4.6 `body rhai r#"…"#` — escape hatch

Inside any handler body, `body rhai r#"..."#` runs a Rhai script with
the entity's state and inputs in scope. The script can read signals,
modify state, and return values. Reuses the Stage-5 design-block
machinery — sandboxed, fuel-limited, statically linked.

```bhdl
on adc_clk rising {
    body rhai r#"
        // 4th-order sigma-delta modulator
        let acc = state.integrators;
        acc[0] += V("ain") - state.feedback;
        // ... DSP ...
        state.integrators = acc;
        state.dout = acc[3] > 0 ? 1 : 0;
    "#
}
```

Same trade-off as design blocks: the DSL handles the 80 % case
where temporal structure is the value; Rhai handles the 20 % case
where you need arbitrary computation inside a known event hook.

---

## 5. Event language

An event-expression is a *predicate over simulation state and time*
that becomes true at well-defined instants. v0.1 event grammar:

```ebnf
event_expression    = signal_crossing
                    | threshold_event
                    | edge_event
                    | state_match
                    | timer_event
                    | named_event
                    | event_conjunction
                    | event_disjunction

signal_crossing     = signal_ref "crosses" expression direction
direction           = "rising" | "falling" | "either"

threshold_event     = signal_ref comparison expression
comparison          = ">" | "<" | ">=" | "<="

edge_event          = signal_ref ("rising" | "falling")

state_match         = state_var "==" identifier
                    | state_var "!=" identifier

timer_event         = "timer" identifier "expires"

named_event         = identifier

event_conjunction   = event_expression "and" event_expression
event_disjunction   = event_expression "or" event_expression
```

### 5.1 Signal crossings

`<signal> crosses <value> rising` fires when `signal` transitions
from below `value` to above `value`. `falling` is the reverse;
`either` is the union.

```bhdl
on VIN crosses 7.0 rising { ... }     // VIN went above 7 V
on VIN crosses 6.5 falling { ... }    // VIN went below 6.5 V
on RESET crosses 1.65 either { ... }  // any crossing of mid-rail
```

The exact crossing time is interpolated from GLACIER's most recent
two timepoints — typically sub-femtosecond accuracy for sane
signals; bounded by the simulator's timestep for sharper edges.

### 5.2 Threshold events

`<signal> > <value>` fires once each time `signal` becomes greater
than `value` (i.e. the rising crossing). `<` fires on the falling
side. `>=` and `<=` include the equal case (relevant for stepped
signals like states-as-numerics).

Semantically identical to `signal crosses value rising|falling`;
the comparator form reads more naturally when the threshold is
implicit (e.g. `on temperature > 150 { ... }`).

### 5.3 Edge events

For digital signals, `<signal> rising` and `<signal> falling` fire
at edge transitions. Implementation-wise: digital signals have
hysteresis-based threshold detection (default ±10 % of full
swing); custom thresholds can be specified on the signal
declaration.

```bhdl
on CLK rising { ... }
on RESET falling { ... }
```

### 5.4 State matches

`<state_var> == <Name>` fires *once* each time the state machine
enters `Name`. (It does not repeatedly fire while the state holds
at `Name` — only on entry.) `!=` fires on exit.

```bhdl
on state == Regulating {
    // fires the instant we transition into Regulating
}
```

### 5.5 Timer expirations

Timers are explicit countdowns. Declared with `timer <name>;`,
started with `start <name> = <duration>;`, fire on `on timer
<name> expires { ... }`. Can be cancelled with `cancel <name>;`.

```bhdl
timer watchdog;

on heartbeat {
    start watchdog = 100ms;       // restart on every heartbeat
}

on timer watchdog expires {
    state = WatchdogFault;
    trigger fault;
}
```

### 5.6 Named events

`trigger <name>;` in a handler fires the synthetic event;
`on <name> { ... }` registers a listener. Named events have no
implicit firing; they exist only when explicitly triggered.

### 5.7 Event composition

`<e1> and <e2>` fires when both have fired and neither has been
"consumed" by an intermediate event (the and-state is reset after
the handler completes). `<e1> or <e2>` fires when either does.

Useful for guard conditions:

```bhdl
on (CLK rising) and (state == Active) {
    // only sample when active AND on a clock edge
}
```

Complex Boolean compositions are allowed but discouraged when a
state-machine reformulation reads more clearly.

---

## 6. Action language

Inside any handler body (`on { ... }`, `initial { ... }`,
`after { ... }`), the following statements are allowed.

### 6.1 Signal assignment

`<signal> = <expression>;` writes a value to a signal at the
current simulation instant. The signal must be an entity output
(or an internal variable). Writes to entity inputs are rejected
at analyzer time.

```bhdl
PG = high;
VOUT_override = 0.0;
debug_count = debug_count + 1;
```

Writes are **zero-time**: downstream entities reading the signal
see the new value at the same instant. If multiple handlers
write the same signal at the same instant, source-order
tie-breaking applies.

### 6.2 State transition

`state = <Name>;` is a constrained form of signal assignment for
state-enum variables. Type-checked at analyzer time: `<Name>`
must be one of the declared states.

### 6.3 Delayed action

`after <duration> => { <action>* };` schedules the inner actions
for a future simulation time. See [§4.4](#44-after-duration--).

### 6.4 Trigger

`trigger <name>;` fires a named (synthetic) event. See
[§4.5](#45-trigger-event).

### 6.5 Assertion (testbench only)

`assert <expression> [, "<message>"];` evaluates the expression at
the current simulation instant. If false, the testbench records
the violation with the supplied message (or auto-generates one).

Assertions are allowed inside `behavior { }` blocks on a board
but only fire when the board is being simulated under a
testbench. In a synthesis-only run they're inert.

### 6.6 Rhai escape

`body rhai r#" … "#` — see [§4.6](#46-body-rhai-r--escape-hatch).

### 6.7 Conditional action

```bhdl
if (state == Regulating and VIN > 8.0) {
    diagnostic_count = diagnostic_count + 1;
} else {
    last_underrun_time = t;
}
```

Standard if/else over Boolean expressions.

---

## 7. Testbench surface

Testbenches are top-level items in a `.bhdl` file, parallel to
`board { ... }` declarations. They contain a sequence of
stimulus statements (`apply`) and assertions (`expect`, `forbid`).

### 7.1 The `testbench { … }` block

```ebnf
testbench           = "testbench" identifier "for" board_name "{"
                      testbench_item*
                      "}"

testbench_item      = apply_stmt
                    | expect_stmt
                    | forbid_stmt
                    | local_decl
                    | sequence_block

apply_stmt          = "apply" signal_name "=" stimulus ";"
stimulus            = expression
                    | "ramp" "(" expression "," expression "," "over" duration ")"
                    | "step" "(" expression "," "at" duration ")"
                    | "pulse" "(" args ")"
                    | "sin" "(" args ")"

expect_stmt         = "expect" temporal_predicate ";"
forbid_stmt         = "forbid" temporal_predicate ";"
```

Example:

```bhdl
testbench BoardStartup for MyProduct {
    apply VBAT = ramp(0V, 12V, over 100ms);
    apply USB_VBUS = 0V;

    expect VDD_3V3 settles_to(3.3V, within 200ms after VBAT > 6.5);
    expect VDD_1V8.rising_edge happens_after VDD_3V3 reaches 90%;
    expect MCU.RESET deasserts_within(1ms after_all(VDD_3V3.PG, VDD_1V8.PG));

    forbid backdrive_on(USB_VBUS) during VBAT.ramp;
    forbid VDD_1V8 > 1.9V at_any_time;
}
```

### 7.2 Stimulus forms

| Form | Meaning |
|---|---|
| `apply X = <constant>;` | Hold `X` at the constant value |
| `apply X = ramp(a, b, over T);` | Linear ramp from `a` to `b` over time `T` starting at simulation start |
| `apply X = step(v, at T);` | Step to `v` at time `T` |
| `apply X = pulse(low, high, t_rise, t_high, t_fall, t_low, t_period);` | PSpice-style pulse train |
| `apply X = sin(offset, amplitude, freq, phase);` | Sinusoid |

Stimuli compose by superposition when applied to the same signal:
`apply X = ramp(0V, 12V, over 100ms);  apply X += sin(0V, 0.1V, 1kHz);`
gives a 12 V ramp with 100 mV / 1 kHz ripple.

### 7.3 Temporal operators

The testbench predicate language layers temporal operators on top
of the expression language.

| Operator | Meaning |
|---|---|
| `within <duration> after <event>` | The condition becomes true within `<duration>` of `<event>` |
| `within <duration> of <event>` | Same as `after` (alias) |
| `before <event>` | The condition becomes true strictly before `<event>` |
| `after <event>` | The condition becomes true strictly after `<event>` |
| `during <interval>` | The condition holds for every instant in `<interval>` |
| `at_any_time` | The condition holds *for some* instant in the simulation |
| `for_all_time` | The condition holds *at every* instant in the simulation |
| `happens_after <event>` | The event-expression occurs after another event |
| `happens_before <event>` | Symmetric |
| `settles_to(<value>, <bound>)` | Reaches and stays within `<bound>` of `<value>` |
| `reaches(<value>)` | Crosses `<value>` (rising) — an event |
| `deasserts_within(<bound>)` | Compound check: goes low within bound |
| `asserts_within(<bound>)` | Same on the high side |
| `after_all(<list>)` | All listed events have fired |
| `after_any(<list>)` | At least one listed event has fired |

Operators compose:

```bhdl
expect VDD_CORE settles_to(0.85V, within ±5%) within 50ms after_all(VBAT > 7V, EN.rising);
```

### 7.4 `expect` versus `forbid`

- `expect P;` records a *failed* assertion if `P` is *false* at
  the relevant time(s).
- `forbid P;` records a *failed* assertion if `P` is *true* at any
  relevant time.

Both produce structured testbench output: a list of
`(simulation_time, signal_state, predicate, result)` rows that
the testbench reporter formats as Markdown / JSON / JUnit XML
depending on the CLI flag.

### 7.5 Reporter format

CLI: `bhdl-cli board.bhdl --testbench BoardStartup test`.

Default output:

```
Testbench BoardStartup for board MyProduct
  ✓ VDD_3V3 settles_to(3.3V, within 200ms after VBAT > 6.5)
       observed: 187ms after threshold, settled at 3.297V
  ✓ VDD_1V8.rising_edge happens_after VDD_3V3 reaches 90%
       observed: VDD_1V8 rising at 195ms, VDD_3V3 at 90% at 173ms (gap 22ms)
  ✗ MCU.RESET deasserts_within(1ms after_all(VDD_3V3.PG, VDD_1V8.PG))
       observed: RESET deasserted 3.4ms after both PG asserted
       violation: tolerance was 1ms
  ✓ forbid backdrive_on(USB_VBUS) during VBAT.ramp
       observed: no current flow on USB_VBUS during ramp
  ✓ forbid VDD_1V8 > 1.9V at_any_time
       observed: max VDD_1V8 = 1.806V at 211ms

3/4 expectations passed, 1 violation. Total simulation time: 1.2s real, 1500ms simulated.
```

`--format json` emits a parseable result for CI integration.

---

## 8. Execution model and scheduler semantics

This is the section most easy to get wrong. Verilog-AMS spent over
a decade ironing out edge cases. v0.1 deliberately makes
conservative choices that are easy to reason about, at the
expense of some expressiveness.

### 8.1 Event scheduler — at a glance

The simulator maintains a single global event queue ordered by
simulation time. Each event has:

- A fire time (`f64` in seconds).
- A handler reference (entity instance + handler index).
- Optional context (e.g. the signal value at the crossing time).
- A source-order tiebreaker for same-time ordering.

The main simulation loop is:

```
while queue not empty:
    event = pop earliest from queue
    advance analog solver to event.fire_time
    fire event.handler
    if handler scheduled new events, push them
```

### 8.2 Analog/discrete synchronisation

**Analog domain** runs in time steps managed by GLACIER. At each
step, GLACIER solves the DC operating point given the current
state of every entity's discrete state and the current stimulus
values. The output is voltages on every node.

After each analog step, the scheduler:

1. Scans for **signal crossings** of every threshold currently
   being listened to by an `on <signal> crosses <threshold> ...`
   handler.
2. For each detected crossing, computes the **exact crossing
   time** by linear interpolation between the previous and current
   analog points.
3. Schedules a discrete event for that exact time.

When a discrete event fires:

1. GLACIER's analog state is rolled to the event's fire time.
2. The handler runs atomically. Its signal assignments and state
   transitions become visible to the next analog solve.
3. If the handler schedules `after <duration>` actions, they're
   pushed into the queue.

### 8.3 Same-time event ordering

When multiple events fire at the same simulation time:

1. **Within one entity**: source-order. Handlers earlier in the
   `behavior { }` block fire first.
2. **Across entities**: instance-creation order in the netlist
   (stable across runs because the netlist itself is order-
   stable).
3. **`after <duration>` clauses** scheduled from a single handler
   fire in the order they appear (source order).

Determinism: same input → same scheduling → same output. The
testbench engine asserts this invariant by re-running deterministic
testbenches twice and comparing observed event sequences.

### 8.4 Causality and infinite loops

A handler can write a signal that triggers another handler. Risk
of infinite loop: handler A writes X; X is monitored by handler
B; B writes Y; Y is monitored by A.

Defenses:

- **Same-time depth bound**: at any simulation instant, the
  scheduler allows up to `MAX_INSTANT_DEPTH` (default 1024)
  handler firings. Exceeding the bound is a `ScheduleError`
  surfaced as a testbench failure: "potential causality loop at
  time T = …".
- **Convergence check**: if signal values are still changing after
  `MAX_INSTANT_DEPTH` firings, the simulator reports a non-
  converged state (rather than silently running forever).
- **Fuel limit on handler bodies**: each handler invocation runs
  with a Rhai-style operation budget (default 100k ops). Runaway
  computation is bounded.

In practice, well-formed behavioral models converge in 1–5
handler firings per simulation instant.

### 8.5 State allocation per instance

Each entity instance has its own copy of the entity's state
variables, local variables, and pending timers. The synthesizer
allocates these at elaboration time; the scheduler keeps a
mutable state record per instance.

Memory cost: linear in the number of instances × declared state
variables. A board with 100 behavioral entities and 5 state
variables each costs ~500 state records (~kilobytes total).

### 8.6 Initial conditions

The `initial { }` block runs at simulation time *zero*, before
any stimuli are applied. State variables are initialised to the
values assigned in `initial`; pin signals are initialised to
their default (zero for `signal in`, indeterminate for `signal
inout`, etc.).

For more complex startup (e.g. ramped initial conditions), use
`apply` stimuli + `on` handlers; the testbench expresses
"what's the initial environment" while `initial` expresses
"what's the entity's starting state."

### 8.7 Time-step control

v0.1: the simulator chooses time steps based on:

1. The next scheduled event's fire time.
2. The minimum time step needed to detect a signal crossing for
   any monitored threshold (informed by the analog solver's
   current gradient estimates).
3. A user-configurable max step (`testbench { max_step = 1us; }`).

Devices that need very fine time resolution can hint:
`behavior { min_step = 10ns; }` — a request for the simulator
not to step coarser than 10 ns while this entity is active.

### 8.8 Determinism guarantees

Given the same `.bhdl` files, the same testbench, the same
selected SKU variant, and the same simulator version, the
simulation produces identical observed event sequences.

What's NOT guaranteed deterministic:

- Floating-point bit-equivalence across hardware (x86 vs ARM)
  for analog calculations.
- Performance (wall-clock time).
- Memory addresses or other implementation details.

What IS guaranteed:

- Same scheduled event order.
- Same assertion violation/pass list.
- Numerically equivalent (within sane tolerance) analog values.

### 8.9 Convergence and termination

A simulation terminates when:

- The event queue is empty AND the testbench has no more `apply`
  stimuli pending.
- `max_simulated_time` (default 10 s) is exceeded.
- A `ScheduleError` (causality loop, non-convergence) is raised.
- The user interrupts.

After termination, the testbench reporter walks the assertion
results.

---

## 9. Integration with existing BHDL features

### 9.1 With `expansion { }` blocks

A behavioral entity can have an `expansion { }` block (concrete
device implementation) OR a `behavior { }` block (model-level
description) — but not both for the same logical role. Vendors
publish multiple entities for the same part:

- `LM7805_Structural` with full `expansion { }` showing every
  internal transistor (rarely used; only for IP-disclosure-
  permissive vendors).
- `LM7805_Behavioral` with `behavior { }` describing the
  external behavior.

The board author picks the appropriate level per part. Most
boards use behavioral for ICs and structural for passives/
discretes.

### 9.2 With `design { }` blocks

Design recipes run at *synthesis time*; behavioral models run at
*simulation time*. They don't conflict.

A typical flow:

1. Synthesis: design recipe sizes Rload + Rfb.
2. Variant: `--sku Pro` applies value overrides.
3. Simulation: behavioral testbench runs against the sized,
   variant-patched netlist.

### 9.3 With variants

Variants are applied to the netlist before simulation. A DNP'd
behavioral entity contributes nothing — no analog equations, no
event handlers, no state. The simulator skips it entirely (same
rule as the SPICE converter [§Variants §V1c]).

Testbenches can be variant-aware: `testbench BoardStartup for
MyProduct(sku = Pro) { ... }` selects the variant the testbench
expects.

### 9.4 With sockets

A socketed component's `behavior { }` runs normally — the socket
is electrically transparent, the held entity contributes the
behavioral model. No special handling needed beyond what the
SPICE converter already does for the analog side.

### 9.5 With the intent surface

Intents drive synthesis-time design. The behavioral model can
read the resulting sized values via the `attribute` mechanism
(e.g. an `LM7805_Behavioral` entity that adapts its dropout to
the value of an externally-set `vout_target` parameter), but
intents don't directly cause behavioral effects at simulation
time.

---

## 10. Format translation

### 10.1 PSpice behavioral → BHDL

PSpice's behavioral element family maps cleanly to BHDL `analog`:

| PSpice form | BHDL equivalent |
|---|---|
| `Bout out 0 V={expr}` | `analog { out = <expr>; }` |
| `E1 out 0 VALUE={expr}` | Same |
| `G1 out 0 VALUE={expr}` | `analog { i_out = <expr>; }` (current source) |
| `G1 out 0 TABLE {V(in)} = (0,0) (1,1)` | `analog { out = lookup(table, V(in)); }` |
| `E1 out 0 LAPLACE {V(in)} {s/(s+1000)}` | Not yet in v0.1 (needs frequency-domain support) |
| `.MODEL ... POLY(2) ...` | Polynomial expression in `analog { }` |

PSpice's `.IF`/`.ELSEIF` conditional models map to BHDL
`if`/`else` inside `analog { }`. PSpice `.MEASURE` directives map
to testbench `expect` statements.

A `bhdl-cli import pspice <file.cir>` utility (planned: B9) reads
a PSpice subckt and emits the equivalent BHDL entity.

### 10.2 IBIS → BHDL

IBIS files are a special case of behavioral: the I-V tables become
`lookup_table()` expressions, the driver states (`HIGH`, `LOW`,
`HighZ`) become `state` enum values, the ramp characteristics
become `after <duration>` clauses.

```ibis
[Component] STM32F4_GPIO
[Model] LVCMOS33_Output
[Pullup]
   -2.0    -0.100
   -1.0    -0.080
    ...
[Pulldown]
    ...
[Power Clamp]
    ...
```

becomes (sketch):

```bhdl
entity STM32F4_GPIO_LVCMOS33() {
    pin PIN: signal inout;
    pin VCC: power in;
    pin GND: ground inout;

    behavior {
        state HighZ, DrivingHigh, DrivingLow;
        initial { state = HighZ; }

        // The lookup tables live as named tables (declared in
        // the BHDL grammar via `table { ... }` blocks).
        analog {
            // Driver current contribution
            if (state == DrivingHigh) {
                i_drive = lookup(pullup_table, V(PIN));
            } else if (state == DrivingLow) {
                i_drive = lookup(pulldown_table, V(PIN));
            } else {
                i_drive = 0;
            }

            // ESD clamps always active
            i_power_clamp = lookup(power_clamp_table, V(PIN) - V(VCC));
            i_ground_clamp = lookup(ground_clamp_table, V(PIN) - V(GND));

            i_pin = i_drive + i_power_clamp + i_ground_clamp;
        }

        // Driver control comes from a logical input (the digital
        // signal driving this pin) — the board ties this to its
        // MCU's output register.
        on logical_high { state = DrivingHigh; }
        on logical_low  { state = DrivingLow; }
        on logical_z    { state = HighZ; }
    }
}
```

A `bhdl-cli import ibis <file.ibs>` utility (planned: B10) reads
an IBIS file and emits the equivalent BHDL entity with `lookup`
tables.

### 10.3 Verilog-AMS subset (future, v0.2+)

A subset of Verilog-A can be translated mechanically:

| Verilog-A | BHDL |
|---|---|
| `analog begin V(out) <+ V(in); end` | `analog { out = V(in); }` |
| `@(cross(V(in) - 1.65, +1))` | `on V(in) crosses 1.65 rising` |
| `@(initial_step)` | `initial { ... }` |
| `parameter real RDS = 0.05;` | `attribute rds = 0.05;` |
| `idt(...)`, `ddt(...)`, `transition(...)` | **Not supported in v0.1** (deliberate non-goal) |

v0.1 ships the translator for the static subset; v0.2 considers
adding `idt`/`ddt` and translating the dynamic subset.

---

## 11. Worked examples

### 11.1 LDO regulator with power-good

```bhdl
entity LDO_3V3() {
    pin VIN:  power in;
    pin VOUT: power out;
    pin GND:  ground inout;
    pin EN:   signal in;
    pin PG:   signal out;

    attribute component_class = "ic_regulator_behavioral";
    attribute manufacturer = "Texas Instruments";
    attribute mpn = "TLV75533PDBV";

    behavior {
        state Disabled, RampingUp, Regulating, ThermalShutdown, Disabled_PG_Falling;
        initial { state = Disabled; PG = low; }

        // Output equation depending on state
        analog {
            if (state == Regulating) {
                VOUT = clamp(VIN − 0.2, 0, 3.3);
            } else if (state == RampingUp) {
                // Linear ramp during 100us startup
                VOUT = (t - startup_time) / 100us * 3.3;
            } else {
                VOUT = 0;
            }
        }

        on EN crosses 1.0 rising {
            if (VIN > 3.5) {
                state = RampingUp;
                let startup_time = t;
                after 100us => {
                    state = Regulating;
                    after 1ms => { PG = high; }
                }
            }
        }

        on EN crosses 1.0 falling {
            state = Disabled;
            PG = low;
            after 100us => { /* output decays via external load */ }
        }

        on VIN crosses 3.5 falling {
            // UVLO
            state = Disabled;
            PG = low;
        }

        // Thermal protection — would need a thermal model on the
        // structural side or in a `thermal { }` block (v1.0).
    }
}
```

### 11.2 Multi-rail power sequencer

```bhdl
entity PowerSequencer() {
    pin VBAT:        power in;
    pin REG_3V3_EN:  signal out;
    pin REG_1V8_EN:  signal out;
    pin REG_3V3_PG:  signal in;
    pin REG_1V8_PG:  signal in;
    pin MASTER_PG:   signal out;

    behavior {
        state Off, Enable_3V3, Wait_3V3, Enable_1V8, Wait_1V8, AllUp, Fault;
        initial { state = Off; }

        on VBAT crosses 6.5 rising {
            state = Enable_3V3;
            REG_3V3_EN = high;
            after 200ms => {
                if (state == Enable_3V3) {
                    state = Fault;
                    trigger fault_3v3_no_pg;
                }
            }
        }

        on REG_3V3_PG rising {
            if (state == Enable_3V3) {
                state = Wait_3V3;
                after 5ms => {
                    state = Enable_1V8;
                    REG_1V8_EN = high;
                    after 200ms => {
                        if (state == Enable_1V8) {
                            state = Fault;
                            trigger fault_1v8_no_pg;
                        }
                    }
                }
            }
        }

        on REG_1V8_PG rising {
            if (state == Enable_1V8) {
                state = AllUp;
                MASTER_PG = high;
            }
        }

        on VBAT crosses 6.0 falling {
            state = Off;
            REG_3V3_EN = low;
            REG_1V8_EN = low;
            MASTER_PG = low;
        }
    }
}
```

Testbench:

```bhdl
testbench PowerSequencingStartup for SystemBoard {
    apply VBAT = ramp(0V, 12V, over 50ms);

    expect REG_3V3_EN.rising happens_before REG_1V8_EN.rising;
    expect MASTER_PG.rising happens_after_all(
        REG_3V3_PG.rising,
        REG_1V8_PG.rising
    );
    expect REG_3V3_PG.rising happens_within 50ms after REG_3V3_EN.rising;
    expect MASTER_PG settles_to(high) within 100ms after VBAT > 6.5;

    forbid fault_3v3_no_pg at_any_time;
    forbid fault_1v8_no_pg at_any_time;
}
```

### 11.3 USB VBUS backdrive detection

```bhdl
testbench BackdriveCheck for MyProduct {
    apply VBAT = 0V;                          // board powered off
    apply USB_VBUS = step(5V, at 10ms);       // host plugged in

    // No current should flow from VBUS into the board's regulator chain
    forbid I(USB_VBUS_to_REG_IN_path) > 1mA during simulation;

    // The board's protection diode must clamp VBUS to safe levels
    expect V(REG_IN) < 0.7V for_all_time;
}
```

### 11.4 I²C lockup detection

```bhdl
entity I2C_Master_Behavioral() {
    pin SDA: signal inout;
    pin SCL: signal inout;
    pin RDY: signal out;

    behavior {
        state Idle, Start, AddrSent, Data, Stop, ArbitrationLost;
        initial { state = Idle; }

        timer protocol_watchdog;

        on start_command {
            state = Start;
            start protocol_watchdog = 100us;
            // ... drive SDA/SCL ...
        }

        on SDA falling and SCL high and state != Start {
            // Another master is starting — we may have lost arbitration
            state = ArbitrationLost;
        }

        on timer protocol_watchdog expires {
            // Protocol stall
            state = Idle;
            trigger i2c_protocol_timeout;
        }
    }
}

testbench BusReady for SystemBoard {
    apply VBAT = 12V;
    apply start_command_from_mcu = step(true, at 100ms);

    forbid i2c_protocol_timeout at_any_time;
    expect RDY settles_to(high) within 200us after start_command_from_mcu;
}
```

### 11.5 ADC with sigma-delta noise

```bhdl
entity SigmaDelta_ADC_12bit() {
    pin AIN: signal in;
    pin CLK: signal in;
    pin DOUT: signal out;

    behavior {
        state Off, Sampling;
        initial { state = Off; }

        let dout_value = 0;

        on AIN.enable rising { state = Sampling; }

        on CLK rising {
            if (state == Sampling) {
                body rhai r#"
                    // 4th-order sigma-delta modulator implemented in Rhai
                    let v_in = V("AIN");
                    let q = sigma_delta_4th(state.integrators, v_in, 12);
                    state.integrators = q.next_state;
                    state.dout_value = q.bits;
                "#
            }
        }

        analog {
            DOUT = dout_value;       // digital output, updated discretely
        }
    }
}
```

---

## 12. Implementation phases

The behavioral surface lands in phases. Each phase is independently
useful; later phases compose on top.

| Stage | Scope | Approx effort |
|---|---|---|
| **B1: Spec** *(this document)* | Architectural design | done |
| **B2: Parser + AST** | `behavior { analog ... state ... on ... after ... trigger ... body rhai }` block grammar + AST nodes + parser tests | 1 week |
| **B3: Analyzer extraction** | `BehavioralModel { entity, signals_read/written, events_listened, states, handlers }` on AnalysisResult | 1 week |
| **B4: Static analog-source bridge** | Simplest case: `analog { V_out = expr; }` becomes a controlled source that GLACIER stamps at each step | 1 week |
| **B5: Discrete-event scheduler** | Time-stepping engine, watchpoint registration, event queue, signal-crossing detection, fire handler | 2 weeks |
| **B6: Domain synchronisation** | Analog → discrete event firing, discrete → analog state changes, causality loop detection, same-time ordering | 2 weeks |
| **B7: Testbench surface** | `testbench`, `apply`, `expect`, `forbid`, temporal operators, reporter | 1.5 weeks |
| **B8: Rhai escape in handlers** | `body rhai r#"..."#` inside `on { ... }` blocks; reuses Stage-5 design-block machinery | 3 days |
| **B9: PSpice behavioral translator** | Read PSpice behavioral subckts (B/E/G/H), emit BHDL `behavior { analog { ... } }` | 1 week |
| **B10: IBIS importer** | Read `.ibs` files, emit BHDL entity with lookup tables + state enum + analog dispatch | 1 week |
| **Total estimated effort** | | **~10–11 weeks** |

Each phase has its own per-feature spec section pulled out of this
document and committed as `Behavioral_Models_Stage_<n>.md` as the
work lands.

### 12.1 Suggested commit sequence

1. **B1 spec lands** — this document, in `docs/spec/`.
2. **B2 parser** — lexer keywords, syntax kinds, parse tests. Same
   shape as the Stage-5b work for design blocks.
3. **B3 analyzer extraction** — `BehavioralModel` data structures
   in `bhdl-common`, extraction in `bhdl-analyzer`.
4. **B4 analog-only bridge** — wire `analog { }` to GLACIER as
   controlled sources. Demonstrate a simple LDO model.
5. **B5 discrete-event scheduler** — first useful version; can
   simulate a state machine with timer-based transitions.
6. **B6 mixed signal** — full analog ↔ discrete coupling.
7. **B7 testbench** — assertions become useful tooling.
8. **B8 Rhai escape** — small additive change.
9. **B9 PSpice translator** — bulk import utility.
10. **B10 IBIS importer** — bulk import utility.

Stages B2–B6 are the architecturally interesting work; B7 onwards
is mostly grinding through known shapes.

---

## 13. Open questions

These are the spec gaps that need answers before code starts.

### 13.1 Hierarchical behavior

Can a `behavior { }` block reference signals from internal
`expansion { }` children? E.g.: an LDO entity has an `expansion { }`
that creates internal nets `vref_internal`, `fb_internal`; can the
`behavior { }` on the same entity say `analog { if V(vref_internal)
> ... }` ?

**Tentative**: yes, but only for `internal` nets declared in the
`expansion` block. Pins on expansion children's children are not
accessible. The synthesizer maps `internal_name` to the post-
expansion net name for the behavioral model's queries.

### 13.2 Branch currents vs node voltages

Some entities want to express behavior over a *branch current*
(e.g. "if I through Rsense > 100mA, trip"). The `analog { }`
language reads `V(node)` naturally; reading `I(branch)` is less
clean. Verilog-AMS uses `I(branch)` notation.

**Tentative**: BHDL `behavior` supports `I(<instance>.<pin>)` to
read the current flowing into that pin. The synthesizer wires this
to GLACIER's branch-current state.

### 13.3 Lookup tables

Where are they declared? Inline in the entity, or as separate
top-level items?

**Tentative**: as `table` blocks inside `behavior { }`:

```bhdl
behavior {
    table pullup_iv {
        -1.0   -0.080
         0.0   -0.060
         ...
    }

    analog {
        i_drive = lookup(pullup_iv, V(PIN));
    }
}
```

Generated automatically by the IBIS importer; vendor-authored
otherwise.

### 13.4 Parameter-of-behavior

Can a behavioral model be parametric (different ramp time per
instance)? E.g.: `entity LDO_3V3(ramp_us: int = 100) { behavior {
... after ramp_us us => ... } }`.

**Tentative**: yes — entity parameters are visible in the
behavioral block as constants. Instance-level overrides
(`U1: LDO_3V3(ramp_us: 50)`) parameterise the model.

### 13.5 Initial-state ambiguity

If a board has 50 behavioral entities and a `testbench` applies a
12V VBAT ramp, what's the initial state? Each entity's `initial {
}` runs at t=0, but signals are evolving... cause/effect ordering
of initials.

**Tentative**: `initial` blocks fire in entity-creation order at
t = 0–ε (epsilon negative); stimuli start at t = 0; first analog
solve at t = 0+ε. This gives a well-defined startup snapshot.

### 13.6 Convergence on multi-rail loops

A PMIC's behavior depends on rail X; rail X's behavior depends on
PMIC. Iterative convergence within an analog timestep — how many
inner iterations are allowed before declaring non-convergence?

**Tentative**: at each analog timestep, the solver iterates until
node voltages change by less than ε between iterations OR
`MAX_TIMESTEP_ITER` (default 32) is exceeded. Non-convergence
raises `ScheduleError`.

### 13.7 Multi-instance state

Each instance of a behavioral entity has its own state. But what
if two instances of the same entity want to communicate? (e.g.
two halves of a differential driver?)

**Tentative**: through pins / nets — the only communication
channel between instances is the netlist. No globals, no
"talking around the back". If two halves of a differential need
shared state, model them as one entity with two pin-pairs.

### 13.8 Testbench-time mutation

Can a testbench *modify* an entity's behavior (e.g. to inject a
fault)? "make this regulator's PG trigger 5ms late as if fault
condition"?

**Tentative**: yes, via `inject` statements in the testbench:

```bhdl
testbench FaultInjection for Board {
    inject U_REG3V3.PG = high after 50ms_delay;
    expect ... ;
}
```

Detailed grammar TBD; deferred to a v0.2 spec amendment.

---

## 14. Naming and grammar rationale

Why the chosen vocabulary.

- **`behavior` vs `model`**: "model" is overloaded (SPICE model,
  IBIS model, …). "behavior" is what Verilog-AMS uses and what
  the EE community recognises.
- **`analog` vs `equation`**: "analog" matches Verilog-AMS and
  reads naturally in EE prose ("the analog behavior of this
  regulator is …"). "equation" feels more mathematical.
- **`on` vs `when` vs `@`**: "on" is natural English ("on rising
  edge, do X"). Verilog uses `@`, VHDL uses `when`. BHDL already
  uses `when` for conditional generation blocks; reusing it would
  clash.
- **`crosses` vs `cross` vs `crossing`**: "crosses" reads as a
  verb phrase ("on VIN crosses 7 volts"). The Verilog-AMS form
  `@(cross(V(in) − 1.65, +1))` is technically more precise but
  much harder to read.
- **`after <duration> =>` vs `delay`**: the `=>` is an arrow used
  in many DSLs for "then do" (Rust's match, Erlang's case). Reads
  as "after 5ms, then do X."
- **`initial` vs `start`**: "initial" matches Verilog convention;
  "start" might be confused with the `start <timer>` syntax.
- **`state X, Y, Z;`**: comma-separated enum declaration is
  compact and aligns with `pin a, b, c;` style elsewhere.
- **`trigger <name>`**: matches the "trigger an event" mental
  model. Alternative `fire <name>` was considered but "trigger"
  is more commonly used in EE contexts.
- **`expect` / `forbid`**: `expect` matches BDD test frameworks
  (RSpec, Jasmine, Chai); `forbid` is the natural negation. Verilog
  uses `assert`/`assume`/`cover`; we use `expect`/`forbid` because
  `assert` is reserved for in-handler use (and the BDD-style
  reads better in a testbench).
- **`apply` for stimulus**: matches LTspice/PSpice nomenclature
  ("apply a source") and reads as imperative ("at t=0, apply 5
  volts to VIN").

---

## 15. Summary

This spec describes a behavioral-modeling surface that:

- Adds a `behavior { }` block to BHDL entities, parallel to
  `expansion { }`, `design { }`, `placement { }`.
- Supports a small DSL with continuous-time `analog { }`
  equations, discrete-event `on { }` handlers, state machines,
  timers, and synthetic events.
- Provides a Rhai escape hatch for arbitrary computation inside
  event handlers — same architectural pattern as the design-
  recipe Stage-5 work.
- Introduces a testbench surface with stimuli (`apply`) and
  temporal assertions (`expect`/`forbid`).
- Integrates cleanly with all existing BHDL features
  (expansion, design recipes, variants, sockets, intents) — no
  conflicts.
- Translates from PSpice behavioral and IBIS formats via bulk
  importers; covers the vendor-data ecosystem.

The implementation lands in 10 stages over ~10–11 weeks of
focused work. The architectural decisions in this document are
the ones the spec has to nail; the implementation that follows
is incremental and bounded.

The strategic value: this is what closes BHDL from "structural
schematic + DC analysis + BOM" to "**full board-level system
simulation**" — a category that doesn't exist in the open-source
EDA world today. The bugs it catches (power sequencing, backdrive,
reset timing, protocol issues) are real production failures that
currently require either expensive proprietary tools or
post-fab debug. Closing this gap is the highest-leverage
architectural addition left in the conversation arc.
