# Staged Power Flow: Unified Intent, Ordering, and Safety Grouping

## Problem

When multiple components connect to the same power rail, the layout engine has no information about their functional ordering. Given:

```bhdl
VIN -> tvs: TVSDiode(15V).K;
VIN -> c_in: Cap(100µF).1;
VIN -> buck: LM2596_5V().VIN;
VIN -> reg5aux: LM7805().VI;
```

All four are independent connections to VIN. The layout engine treats them as equal peers, placing them in arbitrary order. But the designer's intent is clear: protection and filtering are *upstream* of regulation. The schematic should convey this — TVS and caps before regulators.

Today this intent is invisible to the toolchain. A safety engineer reviewing the schematic must manually identify component roles (input protection vs output filtering vs regulation), group them, and infer failure effects. The information exists in the designer's head but not in the source.

## Solution

Extend the `power` declaration with a staged flow specification using the existing `|>` operator, and use the existing `for` intent syntax to assign components to stages with parameterized intent.

### Power Declaration with Stages

```bhdl
power VIN = 24V @ 5A
    |> input_protection |> input_filtering |> regulation;
```

The `|>` chain declares named stages in order. This single line defines:
- **Stage names**: `input_protection`, `input_filtering`, `regulation`
- **Ordering**: input_protection is upstream of input_filtering, which is upstream of regulation
- **Rail association**: these stages belong to the VIN rail

### Component Assignment via `for`

The existing `for` suffix assigns components to stages and passes parameters that guide synthesis and analysis:

```bhdl
VIN -> tvs: TVSDiode(15V).K for input_protection(clamp: 15V, response: 1ns);
tvs.A -> GND;

VIN -> c_in: Cap(100µF).1 for input_filtering(bulk: true, max_esr: 50mΩ);
c_in.2 -> GND;

VIN -> buck: LM2596_5V().VIN for regulation(soft_start: 5ms);
VIN -> reg5aux: LM7805().VI for regulation();
```

The stage name in `for` references the stage declared on the power rail. The parameters are intent-specific — they inform the synthesizer, simulator, and safety tools about *how* this component fulfills its role.

### Output Rails Follow the Same Pattern

```bhdl
power V5_BUCK = 5V @ 2A
    |> output_filtering |> loading;

V5_BUCK -> c5b: Cap(10µF).1 for output_filtering();
c5b.2 -> GND;

V5_BUCK -> r_led5b: Res(330).1 -> r_led5b.2 -> led5b: LED("green").A
    for loading(purpose: "indicator");
led5b.K -> GND;

V5_BUCK -> r_load5b: Res(10).1 -> r_load5b.2 -> GND
    for loading(purpose: "test_load");

V5_BUCK -> reg33: LM1117_33().VI for regulation();
```

Note: `regulation` appears on V5_BUCK even though it wasn't declared in V5_BUCK's stage list. This is valid — the stage name refers to a well-known role from stdlib, and the tool infers its position (regulation is always downstream of filtering). Alternatively, the designer can be explicit:

```bhdl
power V5_BUCK = 5V @ 2A
    |> output_filtering |> loading |> regulation;
```

## Complete Example

```bhdl
board ComplexPowerTree {
    power VIN = 24V @ 5A
        |> input_protection |> input_filtering |> regulation;
    power V5_BUCK = 5V @ 2A
        |> output_filtering |> loading |> regulation;
    power V3_3 = 3.3V @ 300mA
        |> output_filtering |> loading;
    power V5_AUX = 5V @ 500mA
        |> output_filtering |> loading |> regulation;
    power V1_8 = 1.8V @ 200mA
        |> output_filtering |> loading;
    ground GND;

    // === VIN Input Stage ===

    VIN -> tvs: TVSDiode(15V).K for input_protection(clamp: 15V);
    tvs.A -> GND;

    VIN -> c_in: Cap(100µF).1 for input_filtering(bulk: true);
    c_in.2 -> GND;

    // === VIN → Regulators ===

    VIN -> buck: LM2596_5V().VIN for regulation();
    buck.VOUT -> V5_BUCK;
    buck.GND -> GND;

    VIN -> reg5aux: LM7805().VI for regulation();
    reg5aux.VO -> V5_AUX;
    reg5aux.GND -> GND;

    // === V5_BUCK Rail ===

    V5_BUCK -> c5b: Cap(10µF).1 for output_filtering();
    c5b.2 -> GND;

    V5_BUCK -> r_led5b: Res(330).1 -> r_led5b.2 -> led5b: LED("green").A
        for loading(purpose: "indicator");
    led5b.K -> GND;

    V5_BUCK -> r_load5b: Res(10).1 -> r_load5b.2 -> GND
        for loading(purpose: "test_load");

    V5_BUCK -> reg33: LM1117_33().VI for regulation();
    reg33.VO -> V3_3;
    reg33.GND -> GND;

    // === V5_AUX Rail ===

    V5_AUX -> c5a: Cap(10µF).1 for output_filtering();
    c5a.2 -> GND;

    V5_AUX -> r_led5a: Res(470).1 -> r_led5a.2 -> led5a: LED("red").A
        for loading(purpose: "indicator");
    led5a.K -> GND;

    V5_AUX -> reg18: LM1117_18().VI for regulation();
    reg18.VO -> V1_8;
    reg18.GND -> GND;

    // === V3_3 Rail ===

    V3_3 -> c33: Cap(10µF).1 for output_filtering();
    c33.2 -> GND;

    V3_3 -> r_led33: Res(150).1 -> r_led33.2 -> led33: LED("blue").A
        for loading(purpose: "indicator");
    led33.K -> GND;

    V3_3 -> r_load33: Res(330).1 -> r_load33.2 -> GND
        for loading(purpose: "test_load");

    // === V1_8 Rail ===

    V1_8 -> c18: Cap(10µF).1 for output_filtering();
    c18.2 -> GND;

    V1_8 -> r_load18: Res(18).1 -> r_load18.2 -> GND
        for loading(purpose: "main_load");

    V1_8 -> r_led18: Res(56).1 -> r_led18.2 -> led18: LED("yellow").A
        for loading(purpose: "indicator");
    led18.K -> GND;
}
```

## What Each Tool Gets

### Layout Engine

The stage ordering on the power declaration directly controls placement along the rail:

```
VIN ──[ input_protection: tvs ]──[ input_filtering: c_in ]──┬── regulation: buck  ──→ V5_BUCK
                                                             └── regulation: reg5aux ──→ V5_AUX
```

Components tagged `input_protection` are placed leftmost on the VIN wire. `input_filtering` components come next. `regulation` components (regulators that branch off the rail) are placed rightmost. No heuristics needed — the ordering is explicitly declared.

### FMEDA (Failure Modes, Effects, and Diagnostic Analysis)

Each stage becomes an FMEDA functional group with role-specific failure modes:

| Stage | Components | Failure Mode (Open) | Failure Mode (Short) |
|-------|-----------|---------------------|---------------------|
| input_protection | tvs | Loss of overvoltage clamping | Rail shorted to GND |
| input_filtering | c_in | Increased input ripple/noise | Rail shorted to GND |
| output_filtering | c5b | Increased output ripple | Output rail shorted |
| regulation | buck | Loss of 5V rail | Unregulated voltage pass-through |
| loading (indicator) | r_led5b, led5b | Loss of power indication | Increased rail load |
| loading (test_load) | r_load5b | Reduced load on rail | Overcurrent on rail |

The `for` parameters refine failure analysis. `input_protection(clamp: 15V)` tells the safety tool that this component's function is clamping at 15V — its failure mode is "loss of clamping at 15V," not just "TVS failed."

### BOM Generator

Group components by rail and stage:

```
VIN Rail:
  Input Protection: D1 (TVSDiode, 15V clamp)
  Input Filtering:  C1 (100µF, X5R, 50V)

V5_BUCK Rail:
  Output Filtering: C2 (10µF, X7R, 10V)
  Loading:          R1 (330Ω), D2 (LED green), R2 (10Ω)
  Regulation:       U2 (LM1117_33)
```

### Documentation Generator

Auto-generate power tree documentation with stages:

```
VIN (24V @ 5A)
  Stage 1 - Input Protection: TVS clamp at 15V, <1ns response
  Stage 2 - Input Filtering: 100µF bulk capacitor
  Stage 3 - Regulation:
    ├── LM2596_5V (buck) → V5_BUCK (5V @ 2A)
    │     Stage 1 - Output Filtering: 10µF
    │     Stage 2 - Loading: LED indicator, 10Ω test load
    │     Stage 3 - Regulation: LM1117_33 → V3_3 (3.3V)
    └── LM7805 (linear) → V5_AUX (5V @ 500mA)
          Stage 1 - Output Filtering: 10µF
          Stage 2 - Loading: LED indicator
          Stage 3 - Regulation: LM1117_18 → V1_8 (1.8V)
```

### Synthesizer

The `for` parameters guide synthesis decisions:

- `input_filtering(bulk: true, max_esr: 50mΩ)` → select low-ESR capacitor, prefer electrolytic/polymer
- `output_filtering()` on a buck rail → if `output_filtering(max_ripple: 5mV)` intent exists, generate multi-tier cap bank (existing ripple-aware path)
- `regulation(soft_start: 5ms)` → configure regulator's soft-start parameter
- `loading(purpose: "indicator")` → LED current can be reduced to save power without functional impact

## Syntax Summary

### Power Declaration (extended)

```
power <name> = <voltage> @ <current>
    [|> <stage_name> [|> <stage_name> ...]];
```

- `|>` chain is optional — rails without stages work as before
- Stage names are identifiers that match `for` intent function names
- Order of `|>` defines visual ordering on the schematic

### `for` Intent (unchanged syntax, new semantics)

```
<flow_statement> for <stage_name>(<param>: <value>, ...);
```

- Stage name references a stage declared on a power rail
- Parameters are passed to the intent function (defined in stdlib)
- Multiple components can share the same stage name — they're grouped
- The `for` clause is the sole mechanism for role assignment, parameterized intent, and stage membership

### Stdlib Stage Definitions

```bhdl
// In bhdl-stdlib/intents/power_stages.bhdl
intent input_protection(clamp: voltage, response: time) {
    stage_order: 1;
    failure_open: "Loss of overvoltage protection";
    failure_short: "Rail shorted to ground";
}

intent input_filtering(bulk: bool, max_esr: resistance) {
    stage_order: 2;
    failure_open: "Increased input ripple and noise";
    failure_short: "Rail shorted to ground";
}

intent regulation(soft_start: time, dropout: voltage) {
    stage_order: 3;
    failure_open: "Loss of regulated output";
    failure_short: "Unregulated voltage pass-through";
}

intent output_filtering(max_ripple: voltage) {
    stage_order: 4;
    failure_open: "Increased output ripple";
    failure_short: "Output rail shorted";
}

intent loading(purpose: string) {
    stage_order: 5;
    failure_open: "Loss of load function";
    failure_short: "Overcurrent on rail";
}
```

The `stage_order` field provides default ordering when the power declaration omits the `|>` chain. When the `|>` chain is present, it overrides stdlib ordering (allows designer-specific stage sequences).

## Design Decisions

### Why `for` and not a new keyword?

`for` already exists and attaches intent to flow statements. Adding a new keyword for role assignment would create two parallel systems doing similar things. By making `for` serve both purposes (parameterized intent AND stage membership), we keep the language minimal.

### Why stages on the power declaration?

The power declaration is the natural place to describe a rail's architecture. It already carries voltage, current limits, and rail name. Adding the stage pipeline makes it a complete description of what happens on this rail. A designer reading just the power declarations understands the entire power tree topology.

### Why not implicit ordering from source order?

Source line ordering is fragile — refactoring, code formatting, or copy-paste can silently change the schematic layout. Explicit `|>` ordering is intentional and survives refactoring.

### What if `for` is omitted on a statement?

Components without `for` tags are untagged — they connect to the rail but don't belong to any stage. The layout engine places them using existing heuristics (category-based: caps as shunts, regulators as branches). This preserves backward compatibility. Gradually adding `for` tags improves the schematic incrementally.

### What about components on multiple rails?

A component can appear in different stages on different rails:

```bhdl
VIN -> buck: LM2596_5V().VIN for regulation();
buck.VOUT -> V5_BUCK;  // buck is also the source of V5_BUCK
```

The buck is `regulation` on VIN and implicitly the source of V5_BUCK. No conflict — stage membership is per-rail.

## Implementation Phases

### Phase 1: Parser + AST
- Extend `power` declaration grammar to accept optional `|> stage_name` chain
- Store stage list as `Vec<String>` on the power AST node
- No changes to `for` syntax (already parsed)

### Phase 2: Analyzer
- Validate that `for` stage names reference stages declared on the appropriate rail
- Build a `RailStageMap`: rail_name → Vec<(stage_name, Vec<component_name>)>
- Warn if a `for` tag references an undeclared stage (suggest adding it to the power declaration, or fall back to stdlib ordering)

### Phase 3: Schematic Layout
- In the layout engine, read stage ordering from SchematicData
- When placing shunts and branches along a rail, sort by stage order
- Components in earlier stages placed left/upstream; later stages right/downstream

### Phase 4: FMEDA Integration
- Use `RailStageMap` to auto-generate FMEDA functional groups
- Stdlib intent definitions provide default failure modes
- `for` parameters refine failure mode descriptions

### Phase 5: Documentation
- `bhdl doc` command generates staged power tree diagrams
- Each rail section organized by stage with component lists
