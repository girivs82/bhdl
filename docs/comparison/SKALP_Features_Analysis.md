# SKALP Language Analysis - Features for BHDL

This document analyzes features from SKALP (Sankalpana - Intent-Driven Hardware Synthesis) that could benefit BHDL's development.

## Overview

SKALP is a modern hardware description language focused on:
- Intent-driven design with compile-time optimization
- Clock domain safety using Rust-style lifetimes
- Progressive refinement from high-level to RTL
- Strong typing with traits and generics
- Inline physical and timing constraints

**Repository:** Hardware HLS project at `/Users/girivs/src/hw/hls/`

---

## 1. Intent as a First-Class Type ⭐⭐⭐⭐⭐

**What It Is:**
Intent is a type parameter that can be passed to entities, queried at compile-time, and used for conditional compilation.

**SKALP Example:**
```skalp
entity Sqrt<intent I: Intent = Intent::default()> {
    in x: fp32
    out result: fp32
}

impl<intent I> Sqrt<I> {
    // Compile-time branching based on intent
    result = if I.latency < 4 {
        lut_sqrt(x)              // Fast LUT-based
    } else if I.accuracy == High {
        cordic_sqrt(x)           // High accuracy
    } else {
        newton_raphson_sqrt(x)   // Balanced
    }
}
```

**BHDL Relevance:** ⭐⭐⭐⭐⭐ **CRITICAL**

BHDL already has flow-based intents, but making intent a first-class type would enable:

1. **Component Library Parameterization:** Components could adapt based on intent
   ```bhdl
   entity TPS54302<intent I: Intent>(vin: voltage) {
       // Optimize bootstrap circuit based on intent
       pin BOOT: power out when I.optimize == Efficiency;
       pin BOOT: power out with_faster_switching when I.optimize == Speed;
   }
   ```

2. **Intent-Driven Component Selection:** Synthesizer chooses components based on intent
   ```bhdl
   net power: @VIN -> reg: Regulator<intent: high_efficiency>() -> @VOUT
       for power_regulation(efficiency: 95%);
   ```

3. **Compile-Time Optimization:** Different netlists generated for different intents
   ```bhdl
   // High current intent = thicker traces, bigger components
   // Low power intent = sleep modes, power gating
   ```

**Implementation Path:**
- Add `Intent` as builtin type in `bhdl-common`
- Extend parser to handle `<intent I: Intent>` syntax
- Implement intent resolution in analyzer
- Use intent in component matching (synthesizer)
- Generate different layouts based on intent (visualizer)

**Files to Modify:**
- `bhdl-common/src/types.rs` - Add Intent type
- `bhdl-parser/` - Parse intent generic parameters
- `bhdl-analyzer/` - Intent resolution and propagation
- `bhdl-synthesizer/` - Intent-driven component selection
- `bhdl-stdlib/` - Parameterize components with intent

---

## 2. Inline Physical Constraints ⭐⭐⭐⭐

**What It Is:**
Physical constraints (pin mappings, I/O standards, timing) are declared inline with port declarations, not in separate PCF/XDC files.

**SKALP Example:**
```skalp
entity LedBlinker {
    // Pin constraints inline
    in clk: clock @ {
        pin: "A1",
        io_standard: "LVCMOS33",
        frequency: 100MHz
    }

    // Bus with multiple pins
    out leds: bit[8] @ {
        pins: ["C1", "C2", "C3", "C4", "D1", "D2", "D3", "D4"],
        io_standard: "LVCMOS33",
        drive: 8mA,
        slew: fast
    }

    // Differential pair
    inout lvds_data: bit @ {
        pin_p: "E1",
        pin_n: "E2",
        io_standard: "LVDS_25"
    }
}
```

**Validation:**
- Compile-time checking against device database
- Pin conflict detection
- I/O standard compatibility checking
- Bank voltage compatibility

**BHDL Relevance:** ⭐⭐⭐⭐ **HIGH VALUE**

BHDL is for circuit boards, not FPGAs, but the inline constraint concept applies:

1. **PCB Physical Constraints:**
   ```bhdl
   board BuckConverter {
       power VIN @ {
           connector: "J1",
           trace_width: 50mil,      // High current
           via_size: 12mil,
           layer: top
       }

       net switched @ {
           trace_width: 20mil,
           impedance: 50ohm,        // Controlled impedance
           layer: top,
           keep_away: sensitive_signals
       }
   }
   ```

2. **Component Package Constraints:**
   ```bhdl
   entity TPS54302() {
       // Package information inline
       package: SOT23-6 @ {
           footprint: "SOT23-6",
           thermal_pad: true,
           pin_pitch: 0.95mm
       }
   }
   ```

3. **Thermal Constraints:**
   ```bhdl
   component MOSFET @ {
       thermal: {
           junction_temp_max: 150C,
           thermal_resistance: 40C/W,
           heat_sink_required: when power > 2W
       }
   }
   ```

**Implementation Path:**
- Design physical constraint syntax for PCB
- Add constraint parsing to `bhdl-parser`
- Store constraints in netlist metadata
- Use in layout generation (`bhdl-visualizer`)
- Generate manufacturing files with constraints

**Comparison to SKALP:**
| SKALP (FPGA) | BHDL (PCB) Equivalent |
|--------------|----------------------|
| `pin: "A1"` | `connector: "J1"` or `test_point: "TP1"` |
| `io_standard: "LVCMOS33"` | `voltage_level: 3.3V` |
| `drive: 8mA` | `trace_width: 20mil` (current capacity) |
| `differential pair` | `differential: {spacing: 8mil, impedance: 100ohm}` |

---

## 3. Clock Domain as Lifetime ⭐⭐⭐

**What It Is:**
Clock domains are expressed using Rust-style lifetime annotations, enabling compile-time CDC (Clock Domain Crossing) checking.

**SKALP Example:**
```skalp
// Signal in clock domain 'a
signal data: logic<'a>[32]

// Signal in different domain requires synchronization
signal sync_data: logic<'b>[32]

// Function generic over clock domain
fn process<'clk>(input: logic<'clk>[32]) -> logic<'clk>[32] {
    input + 1  // Same domain in and out
}

// CDC entity
entity CDC {
    in data: logic<'src>[32]
    out sync: logic<'dst>[32]
}

impl CDC {
    sync = synchronize(data)  // Compiler inserts CDC
}
```

**Compile-Time Checking:**
- Prevents accidental CDC violations
- Ensures synchronizers are present
- Verifies clock relationships

**BHDL Relevance:** ⭐⭐⭐ **MODERATE** (not directly applicable to boards)

BHDL is for analog/power circuits, not digital systems, but the concept of **domain tracking** applies:

1. **Power Domain Tracking:**
   ```bhdl
   // Power domain as "lifetime"
   net data_3v3: signal<@VCC_3V3>
   net data_5v: signal<@VCC_5V>

   // Level shifter required when crossing domains
   net data_5v: signal<@VCC_5V> = level_shift(data_3v3)
   ```

2. **Isolation Domain Tracking:**
   ```bhdl
   // Isolated vs non-isolated grounds
   net signal_isolated: signal<@GND_ISO>
   net signal_main: signal<@GND>

   // Optocoupler required
   net signal_main = isolate(signal_isolated)
   ```

**Implementation Path:**
- Track power domains as "lifetimes"
- Warn when signals cross domains without protection
- Suggest level shifters or isolators

---

## 4. Protocol System with Direction Flipping ⭐⭐⭐⭐

**What It Is:**
Protocols define bidirectional interfaces with automatic direction flipping using the `~` operator.

**SKALP Example:**
```skalp
protocol AXIStream {
    out data: bit[32],
    out valid: bit,
    in ready: bit,
    out last: bit
}

entity Producer {
    port axi: AXIStream      // Uses as defined (master)
}

entity Consumer {
    port axi: ~AXIStream     // Flips all directions (slave)
    // data, valid, last become inputs
    // ready becomes output
}
```

**BHDL Relevance:** ⭐⭐⭐⭐ **HIGH VALUE**

BHDL already has interfaces but could benefit from direction flipping:

1. **Power Interface Flipping:**
   ```bhdl
   interface PowerSupply {
       out vin: voltage,
       out gnd: ground,
       in enable: signal
   }

   entity Regulator() {
       interface supply: PowerSupply    // Provides power (vin/gnd are outputs)
   }

   entity Load() {
       interface supply: ~PowerSupply   // Consumes power (vin/gnd are inputs)
   }
   ```

2. **SPI/I2C Interface Flipping:**
   ```bhdl
   interface SPI {
       out mosi: signal,
       in miso: signal,
       out sclk: signal,
       out cs: signal
   }

   entity MCU() {
       interface spi: SPI      // SPI master
   }

   entity Sensor() {
       interface spi: ~SPI     // SPI slave (directions flipped)
   }
   ```

**Implementation Path:**
- Add `~` operator for interface flipping in parser
- Update analyzer to flip pin directions
- Synthesizer uses flipped directions for connections

**Files to Modify:**
- `bhdl-parser/` - Parse `~InterfaceName` syntax
- `bhdl-ast/` - Represent flipped interfaces
- `bhdl-analyzer/` - Handle direction flipping in type checking
- `bhdl-synthesizer/` - Use flipped directions in netlist

---

## 5. HLS-Style Intent System ⭐⭐⭐⭐⭐

**What It Is:**
Comprehensive intent annotations for memory optimization, loop transformations, dataflow, resource binding, power optimization, etc.

**SKALP Categories:**
1. **Memory Optimization:** Banking, partitioning, access patterns
2. **Loop Transformation:** Unrolling, pipelining, tiling, merging
3. **Dataflow:** Channel depth, pipeline mode, producer-consumer
4. **Interface Protocol:** AXI, AXI-Stream, handshaking
5. **Resource Binding:** DSP blocks, BRAM, LUT mapping
6. **Power Optimization:** Clock gating, voltage scaling, power gating
7. **Clock Domain:** CDC, async FIFOs, clock multiplexing
8. **Verification:** Assertions, protocol checking, formal properties
9. **Synthesis Strategy:** Optimization goals, retiming, resource sharing

**SKALP Example:**
```skalp
@intent(memory: {
    coeffs: {
        banking: 8,           // 8-way banking
        mode: cyclic,
        impl: bram
    }
})
@intent(loop: {
    unroll: complete,
    pipeline: {ii: 1}
})
@intent(optimize: throughput)
impl FIRFilter {
    // Implementation guided by intents
}
```

**BHDL Relevance:** ⭐⭐⭐⭐⭐ **CRITICAL**

BHDL's intent system could be expanded with similar categories adapted for board design:

1. **Component Selection Intent:**
   ```bhdl
   net power: @VIN -> reg -> @VOUT
       for power_regulation(
           efficiency: 95%,          // Select high-efficiency regulator
           cost: minimize,           // Prefer cheaper parts
           thermal: minimize,        // Low heat generation
           availability: high        // Common parts
       );
   ```

2. **Layout Optimization Intent:**
   ```bhdl
   net switched: reg.PH -> inductor -> @VOUT
       for switching_path(
           trace_impedance: 50ohm,
           coupling: minimize,       // Avoid EMI
           trace_length: minimize,   // Short high-frequency path
           via_count: minimize       // Reduce inductance
       );
   ```

3. **Thermal Management Intent:**
   ```bhdl
   component MOSFET
       for switching(
           thermal: {
               junction_temp: < 100C,
               cooling: passive,     // No active cooling
               thermal_vias: true    // Add thermal vias
           }
       );
   ```

4. **EMI/EMC Intent:**
   ```bhdl
   net clock: @CLK -> devices
       for signal_distribution(
           emi: {
               shielding: ground_plane,
               filtering: pi_filter,
               slew_rate: controlled
           }
       );
   ```

**Implementation Path:**
- Define intent taxonomy for board design
- Extend parser to handle complex intent syntax
- Store intents in netlist metadata
- Use intents in:
  - Component matching (synthesizer)
  - Layout generation (visualizer)
  - DRC checking
  - BOM optimization

**Intent Categories for BHDL:**
| SKALP (Digital/FPGA) | BHDL (Analog/Board) |
|---------------------|-------------------|
| Memory optimization | Component placement optimization |
| Loop transformation | Trace routing optimization |
| Resource binding (DSP/BRAM) | Component selection (SMD/THT, size) |
| Clock domain | Power domain, isolation |
| Throughput/Latency | Efficiency/Cost/Thermal |
| Verification (formal) | Verification (SPICE, DRC) |

---

## 6. Parametric Numeric Types ⭐⭐⭐

**What It Is:**
Unified numeric type system where floating-point and fixed-point are parameterized types.

**SKALP Example:**
```skalp
// Parametric floating-point
type fp<const F: FloatFormat> = bit[F.total_bits]

// Standard formats
type fp16 = fp<IEEE754_16>
type fp32 = fp<IEEE754_32>
type bf16 = fp<BFLOAT16>

// Custom format
const CUSTOM_FP24: FloatFormat = FloatFormat {
    total_bits: 24,
    exponent_bits: 7,
    mantissa_bits: 16,
    bias: 63
}
type fp24 = fp<CUSTOM_FP24>

// Generic operations work for ANY format
entity Add<T, intent I: Intent>
where T: Numeric
{
    in a: T
    in b: T
    out result: T
}
```

**BHDL Relevance:** ⭐⭐⭐ **MODERATE**

BHDL could benefit from parameterized electrical types:

1. **Parameterized Voltage Types:**
   ```bhdl
   type voltage<const NOMINAL: f64, const TOLERANCE: f64> = electrical

   type v3v3 = voltage<3.3, 0.05>    // 3.3V ±5%
   type v5v0 = voltage<5.0, 0.10>    // 5.0V ±10%
   type v12v = voltage<12.0, 0.10>   // 12V ±10%

   power VCC: v3v3 = 3.3V @ 1A;
   ```

2. **Parameterized Component Types:**
   ```bhdl
   type resistor<const VALUE: resistance, const TOLERANCE: f64, const POWER: power> = component

   type r1k = resistor<1kohm, 0.01, 0.25W>    // 1kΩ ±1% 1/4W

   // Generic resistor divider
   entity ResistorDivider<R1: resistor, R2: resistor>() {
       // Works for any resistor types
   }
   ```

---

## 7. Clear Assignment Operators ⭐⭐

**What It Is:**
Three distinct assignment operators with clear semantics:
- `=` : Continuous/combinational assignment
- `<=` : Signal assignment (hardware register)
- `:=` : Variable assignment (procedural, immediate)

**SKALP Example:**
```skalp
// Continuous assignment (outside process)
output = input + 1

// Sequential block
on(clock.rise) {
    counter <= counter + 1    // Signal (register)
    temp := temp + 1          // Variable (immediate)
}
```

**BHDL Relevance:** ⭐⭐ **LOW** (BHDL is not digital/HDL)

BHDL doesn't have registers or sequential logic, but the concept of clear operators applies:

- `->` : Flow connection (already exists)
- `<->` : Bidirectional connection (already exists)
- `|>` : Processing flow (already exists)
- `:` : Net assignment (already exists)

BHDL already has clear connection semantics, so this is less relevant.

---

## 8. Event Syntax with OR Notation ⭐

**What It Is:**
Explicit event triggering using `on(event1 | event2)` notation, making async behavior clear.

**SKALP Example:**
```skalp
// Synchronous reset - only responds to clock
on(clock.rise) {
    if (reset) {
        q <= 0
    }
}

// Asynchronous reset - responds to clock OR reset
on(clock.rise | reset.rise) {
    if (reset) {
        q <= 0    // Happens immediately on reset edge
    }
}
```

**BHDL Relevance:** ⭐ **LOW** (BHDL has no clocks/events)

BHDL is declarative for circuit boards - no sequential logic or events. Not applicable.

---

## 9. Requirements as First-Class Citizens ⭐⭐⭐⭐

**What It Is:**
Requirements, safety mechanisms, and FMEA are integrated into the language.

**SKALP Example:**
```skalp
requirement REQ_PERF_001 {
    id: "SYS-PERF-001",
    title: "Inference Throughput",
    type: functional,
    measurable: {
        metric: throughput,
        target: 1_TOPS
    }
}

entity AIAccelerator {
    // ...
} with {
    satisfies: [REQ_PERF_001],
    evidence: {
        REQ_PERF_001: {
            throughput_achieved: 1.1_TOPS,
            report: "validation/perf_report.html"
        }
    }
}
```

**BHDL Relevance:** ⭐⭐⭐⭐ **HIGH VALUE**

BHDL could integrate requirements, especially for safety-critical designs:

1. **Electrical Requirements:**
   ```bhdl
   requirement REQ_POWER_001 {
       id: "PWR-001",
       title: "Output Voltage Regulation",
       type: electrical,
       measurable: {
           parameter: output_voltage,
           target: 5.0V,
           tolerance: ±2%,
           load_range: 0A..3A
       }
   }

   board BuckConverter {
       // ...
   } with {
       satisfies: [REQ_POWER_001],
       evidence: {
           REQ_POWER_001: {
               simulated_regulation: 1.8%,
               measured_regulation: 1.5%,
               test_report: "validation/load_regulation.pdf"
           }
       }
   }
   ```

2. **Safety Requirements:**
   ```bhdl
   requirement SREQ_OVP_001 {
       id: "SAF-OVP-001",
       title: "Overvoltage Protection",
       type: safety,
       criticality: high,
       mechanism: {
           detection: voltage_sense,
           action: shutdown,
           response_time: < 10us
       }
   }

   board PowerSupply {
       // OVP circuit
   } with {
       satisfies: [SREQ_OVP_001],
       safety_mechanism: {
           type: protective,
           implements: SREQ_OVP_001
       }
   }
   ```

**Implementation Path:**
- Add `requirement` keyword to parser
- Store requirements in metadata
- Link components to requirements
- Generate compliance reports
- Integrate with SPICE analysis for verification

---

## 10. Testbench Separation ⭐⭐

**What It Is:**
Clear separation between synthesizable hardware and testbench code using `#[testbench]` attribute.

**SKALP Example:**
```skalp
// Hardware context (synthesizable)
entity Processor {
    in clk: clock
    out result: bit[32]
}

// Testbench context (non-synthesizable)
#[testbench]
mod tests {
    async fn test_processor() {
        let mut dut = ProcessorSim::new();
        await clock_cycle();
        assert_eq!(await dut.result.read(), 0x42);
    }
}
```

**BHDL Relevance:** ⭐⭐ **MODERATE**

BHDL could benefit from integrated testbench syntax:

```bhdl
// Board definition
board BuckConverter {
    power VIN = 12V @ 3A;
    ground GND;
    // ...
}

// Testbench
#[testbench]
mod load_regulation_test {
    async fn test_load_regulation() {
        let mut dut = BuckConverterSim::new();

        // Set input voltage
        dut.VIN = 12.0V;

        // Sweep load current
        for load in [0A, 0.5A, 1A, 2A, 3A] {
            dut.set_load(load);
            await settle_time(1ms);

            let vout = dut.measure_voltage("VOUT");
            assert!(vout >= 4.9V && vout <= 5.1V,
                "Output voltage out of spec at {load}");
        }
    }
}
```

---

## Summary: Priority Ranking for BHDL

| Feature | Priority | Effort | Impact | Status in BHDL |
|---------|----------|--------|--------|----------------|
| **Intent as First-Class Type** | ⭐⭐⭐⭐⭐ | High | Very High | Partially exists (flow intents) |
| **HLS-Style Intent System** | ⭐⭐⭐⭐⭐ | High | Very High | Basic intents exist, needs expansion |
| **Inline Physical Constraints** | ⭐⭐⭐⭐ | Medium | High | Not implemented |
| **Protocol Direction Flipping** | ⭐⭐⭐⭐ | Low | Medium | Interfaces exist, no flipping |
| **Requirements as First-Class** | ⭐⭐⭐⭐ | Medium | High | Not implemented |
| **Clock Domain as Lifetime** | ⭐⭐⭐ | Medium | Medium | Power domains tracked, not formal |
| **Parametric Numeric Types** | ⭐⭐⭐ | High | Medium | Basic types exist, not parameterized |
| **Testbench Separation** | ⭐⭐ | Medium | Low | No testbench support |
| **Clear Assignment Operators** | ⭐⭐ | Low | Low | Flow operators already clear |
| **Event Syntax** | ⭐ | Low | Very Low | Not applicable (no sequential logic) |

---

## Recommended Implementation Roadmap

### Phase 1: Intent System Enhancement (Highest Impact)
1. Make intent a first-class type
2. Add intent parameters to components
3. Implement compile-time intent resolution
4. Expand intent categories (thermal, EMI, cost, etc.)

**Expected Timeline:** 4-6 weeks
**Files Affected:** parser, ast, analyzer, synthesizer, common

### Phase 2: Physical Constraints (High Value, Lower Effort)
1. Design PCB constraint syntax
2. Add inline constraint parsing
3. Store constraints in netlist metadata
4. Use constraints in layout generation

**Expected Timeline:** 2-3 weeks
**Files Affected:** parser, netlist, visualizer

### Phase 3: Requirements Integration (Safety-Critical Value)
1. Add `requirement` keyword and parsing
2. Link requirements to components/circuits
3. Generate compliance reports
4. Integrate with simulation for verification

**Expected Timeline:** 3-4 weeks
**Files Affected:** parser, analyzer, spice (validation)

### Phase 4: Protocol Enhancements (Nice to Have)
1. Add `~` operator for interface flipping
2. Update analyzer for direction handling
3. Use in synthesizer for connection validation

**Expected Timeline:** 1-2 weeks
**Files Affected:** parser, analyzer, synthesizer

---

## Key Takeaways

1. **SKALP's intent system is more mature** - BHDL should adopt intent-as-type approach
2. **Inline constraints are powerful** - Keeping design data with code prevents drift
3. **Requirements integration is valuable** - Especially for safety-critical board design
4. **Not everything applies** - SKALP is digital/FPGA, BHDL is analog/board (different domains)
5. **Learn from HLS** - Intent-driven optimization is the future for both FPGA and PCB design

**Bottom Line:** BHDL should focus on **intent as first-class type**, **inline physical constraints**, and **requirements integration** as the highest-value features to adopt from SKALP.
