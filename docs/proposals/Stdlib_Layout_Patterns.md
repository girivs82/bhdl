# Stdlib-Driven PCB Layout Patterns

## Motivation

IC vendors (TI, Analog Devices, Diodes Inc, etc.) publish **layout recommendations** in their datasheets and application notes. These are expert-designed patterns for component placement and routing around their ICs. Examples:

- TI TPS54331 datasheet §10.2 "Layout Recommendations": input cap within 5mm of VIN pin, output cap close to VOUT, minimize power loop area, keep feedback divider close to FB pin
- Analog Devices LT8610 app note: specific PCB layout diagram showing exact component positions relative to IC pads
- Diodes Inc AP63205 eval board layout: reference PCB available as KiCad/Altium files

Currently, BHDL's expansion blocks define the **schematic** (what components and connections), but not the **physical layout** (where to place them on the PCB). The PnR engine has to guess — and it guesses poorly because it doesn't know the domain-specific patterns.

## Proposal: Layout Patterns in Stdlib

Extend the expansion block definition in stdlib to include **layout patterns** — vendor-recommended physical arrangements that the PnR engine follows exactly.

### Data Model

```bhdl
entity TPS54331(V_IN: voltage, V_OUT: voltage, I_OUT: current) {
    // ... pins, expansion block (schematic) ...

    // NEW: Layout pattern — physical arrangement of expansion children
    layout_pattern {
        // Reference: TI TPS54331 Datasheet §10.2, Figure 22
        // https://www.ti.com/lit/ds/symlink/tps54331.pdf

        // Power loop: VIN → IC → SW → L → VOUT → C_out → GND → C_in → VIN
        // This loop should have minimum area for EMI.
        power_loop: [C_in.1, VIN, SW, L_out.1, L_out.2, C_out.1, C_out.2, C_in.2];

        // Placement rules (relative to IC center)
        place C_in {
            near: VIN;           // within 2mm of VIN pin
            align_pin: 2 to GND; // pin 2 (GND) aligns with IC ground
            side: input;         // input side of IC
        }

        place C_out {
            near: VOUT;
            align_pin: 2 to GND;
            side: output;
        }

        place L_out {
            near: SW;
            orientation: horizontal;  // inductor runs left-right
        }

        place D_catch {
            near: SW;
            between: [SW, GND];  // cathode to SW, anode to GND
        }

        place R_top {
            near: FB;
            chain: [VOUT, R_top.1, R_top.2, R_bot.1]; // series chain
        }

        place R_bot {
            near: FB;
            chain: [R_top.2, R_bot.1, R_bot.2, GND];
        }

        place C_boot {
            near: BST;
            compact: true;  // as close as possible
        }

        // Ground strategy
        ground {
            strategy: solid_plane;      // continuous ground plane under IC
            thermal_vias: under IC;     // thermal pad connection
            star_point: IC.GND;         // all grounds star from IC GND pin
        }

        // Routing constraints
        routing {
            power_loop_max_area: 50mm²;  // from app note
            feedback_max_length: 10mm;   // keep short for noise
            bootstrap_max_length: 5mm;
        }
    }
}
```

### Supported Pattern Primitives

#### Placement Primitives

| Primitive | Meaning | Example |
|-----------|---------|---------|
| `near: PIN` | Place within 2.5mm of named pin | `near: VIN` |
| `align_pin: N to NET` | Align pad N of this component with the NET trace | `align_pin: 2 to GND` |
| `side: input\|output` | Place on input or output side of parent IC | `side: input` |
| `between: [A, B]` | Place on the trace between A and B | `between: [SW, GND]` |
| `chain: [...]` | Components in series chain, placed in line | `chain: [VOUT, R1, R2, GND]` |
| `compact: true` | Minimize distance to parent IC | Bootstrap caps |
| `orientation: H\|V` | Force horizontal or vertical | Inductors typically horizontal |
| `mirror: axis` | Mirror placement about an axis | Double-row cap banks |

#### Arrangement Patterns

| Pattern | Description | Use Case |
|---------|-------------|----------|
| `single_row(net)` | Components in a line, shared net aligned | Input cap bank on GND |
| `double_row(net)` | Two mirrored rows, shared net between them | Dense cap bank |
| `star(center)` | Radial around a pin | Decoupling caps around BGA |
| `series_chain(path)` | In-line chain following signal path | Resistor divider, filter chain |
| `power_loop(path)` | Minimize enclosed area | Buck converter power stage |
| `differential(pair)` | Symmetric about center line | USB, Ethernet, LVDS |

#### Routing Primitives

| Primitive | Meaning |
|-----------|---------|
| `power_loop_max_area` | Maximum area of high-current loop |
| `max_length: Nmm` | Maximum trace length for a net |
| `min_width: Nmm` | Minimum trace width |
| `keep_short: [nets]` | These nets should be as short as possible |
| `guard_trace: net` | Add ground guard traces alongside |
| `impedance: NΩ` | Controlled impedance trace |

### Standard Library Patterns

#### Buck Converter (Generic)

All buck converters share the same fundamental layout:

```
power_loop_pattern {
    // Critical current loop: minimize area
    //   VIN → C_in → IC_VIN → IC_SW → L → C_out → GND → C_in_GND
    //
    // Physical arrangement:
    //   C_in ──── IC ──── L
    //    |                 |
    //   GND ──── GND ──── C_out
    //
    // The ground plane connects all GND points underneath.

    row_1: [C_in, IC, L_out, C_out];  // top row: power path
    ground: plane_below;               // solid GND plane
    feedback: [R_top, R_bot] near IC.FB, chain;
    bootstrap: C_boot near IC.BST, compact;
}
```

#### LDO (Generic)

```
ldo_pattern {
    // Simple: input cap → IC → output cap
    //
    //   C_in ── IC ── C_out
    //    |      |      |
    //   GND    GND    GND

    row: [C_in, IC, C_out];
    align: GND;  // all GND pins in a line
    ground: star from IC.GND;
}
```

#### Cap Bank (Dense Packing)

```
cap_bank_pattern(count, net_shared, net_individual) {
    if count <= 5 {
        // Single row: all caps in line, shared net (GND) aligned
        single_row {
            align: net_shared;    // GND pins all in a line
            connect: net_individual; // power daisy-chained to ends
        }
    } else {
        // Double row: two mirrored rows
        // Row A:  [C1 C2 C3 C4 C5]  (GND facing down)
        // Row B:  [C6 C7 C8 C9 C10] (GND facing up, mirrored)
        // GND trace runs between the rows
        double_row {
            row_a: caps[0..count/2];
            row_b: caps[count/2..];  // flipped
            shared_net: net_shared;   // GND between rows
            individual_net: net_individual; // power on outer edges
        }
    }
}
```

#### Differential Pair Termination

```
diff_pair_pattern {
    // Symmetric placement about the differential pair axis
    //
    //   ┌─ R_P ─┐
    //   │       │
    // D+ ──────── IC.D+
    // D- ──────── IC.D-
    //   │       │
    //   └─ R_N ─┘

    symmetric_about: [D+, D-];
    termination: [R_P, R_N] mirrored;
    max_skew: 0.127mm;  // 5 mil length matching
}
```

### How PnR Uses Layout Patterns

1. **Block Formation** (existing): Expansion blocks become placement blocks
2. **Internal Layout** (new): Read `layout_pattern` from stdlib → compute exact relative positions for each child component
3. **Block Placement**: Place blocks on board (block has known size and pin positions on its boundary)
4. **Inter-block Routing**: Route between blocks; block internal routing already done
5. **Orientation Adjustment**: If inter-block routing has bad angles, rotate the block and re-route internally

### Integration with Existing BHDL Infrastructure

| Existing Feature | Layout Pattern Use |
|-----------------|-------------------|
| Expansion blocks | Define which components belong to the pattern |
| Intent system | `for regulation()` → buck/LDO pattern; `for input_filtering()` → cap bank pattern |
| GLACIER simulation | Operating current → trace width within pattern |
| IPC-7351B footprints | Real pad positions for pin alignment |
| Physical selection | Package size → pattern dimensions |

### Vendor-Specific Patterns

Each IC entity in stdlib includes the vendor's recommended layout:

```bhdl
// From TI TPS54331 datasheet §10.2
entity TPS54331(V_OUT: voltage) {
    expansion { /* schematic */ }

    layout_pattern {
        reference: "TI TPS54331 Datasheet Fig.22";
        power_loop: minimize_area([C_in, VIN, SW, L_out, C_out, GND]);
        place C_in   { near: VIN; side: input; }
        place C_out  { near: VOUT; side: output; }
        place L_out  { near: SW; orientation: horizontal; }
        place D_catch { near: SW; between: [SW, GND]; }
        place R_top  { near: FB; chain_with: R_bot; }
        place R_bot  { near: FB; to: GND; }
        place C_boot { near: BST; compact: true; }
    }
}

// From Diodes Inc AP63205 eval board
entity AP63205() {
    expansion { /* schematic */ }

    layout_pattern {
        reference: "AP63205 Eval Board Layout";
        place C_in  { near: VIN; side: left; }
        place C_out { near: VOUT; side: right; }
        place L_out { near: SW; above: IC; }
        place C_bst { near: BST; compact: true; }
    }
}

// Generic LDO pattern (AP2112K, XC6206, etc.)
entity GenericLDO(V_OUT: voltage) {
    expansion { C_in, C_out }

    layout_pattern {
        pattern: ldo_standard;
        row: [C_in, IC, C_out];
        align: GND;
    }
}
```

### Implementation Phases

#### Phase 1: Pattern Engine (Current Focus)
- Define `LayoutPattern` data structure in bhdl-common
- Parse `layout_pattern { }` blocks in bhdl-parser/bhdl-ast
- Store patterns alongside expansion recipes in analyzer
- PnR reads patterns in `blocks.rs` for internal layout

#### Phase 2: Standard Patterns in Stdlib
- Implement buck converter pattern (TPS54331, AP63205)
- Implement LDO pattern (AP2112K, XC6206)
- Implement cap bank pattern (single row, double row)
- Implement resistor divider chain pattern

#### Phase 3: Vendor-Specific Patterns
- Import reference layouts from IC eval board designs
- Map eval board component positions to relative pattern coordinates
- Community-contributed patterns for popular ICs

#### Phase 4: Pattern Optimization
- Auto-select pattern variant based on available board space
- Rotate patterns to match inter-block routing directions
- Tear up and re-layout blocks that create routing problems

### Key Principle

**The PnR engine should not invent layout patterns — it should follow patterns specified by domain experts (IC vendors, experienced designers) encoded in the stdlib.** The engine's job is to:
1. Select the right pattern for each block
2. Apply the pattern with real component dimensions
3. Place pattern blocks on the board
4. Route between blocks

This is analogous to how FPGA tools work: the CLB internal structure is fixed by the FPGA vendor, the tool just places and routes between CLBs.
