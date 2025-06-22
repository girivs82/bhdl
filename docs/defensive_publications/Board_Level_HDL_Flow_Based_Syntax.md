# Defensive Publication: Board-Level HDL with Flow-Based Syntax

**Publication Date**: [DATE]  
**Authors**: [Your Name]  
**Contact**: [Your Email]

## Abstract

This publication discloses a novel hardware description language specifically designed for board-level electronic circuit description using flow-based syntax. Unlike traditional HDLs that focus on chip-level design (Verilog, VHDL) or schematic capture tools, this innovation provides a text-based language that mirrors how engineers naturally think about signal flow through circuit boards. The language features intuitive flow operators, direct component instantiation, integrated electrical units, and progressive abstraction levels from high-level signal flows down to specific component connections.

## Background and Prior Art

### Traditional HDL Approaches

1. **Chip-Level HDLs (Verilog/VHDL)**:
   ```verilog
   // Designed for IC design, not board-level
   module board_design;
     wire vcc, gnd, signal;
     // No concept of physical components
     // No electrical units or constraints
   endmodule
   ```

2. **Netlist Formats (SPICE, EDIF)**:
   ```spice
   * Low-level, hard to write manually
   R1 N001 N002 10k
   C1 N002 0 100n
   * No abstraction or structure
   ```

3. **Schematic Capture (GUI-based)**:
   - KiCad, Altium, Eagle
   - Requires graphical interface
   - Version control unfriendly
   - Difficult to parameterize or generate

### Limitations of Prior Art

- **Wrong Abstraction Level**: Chip HDLs focus on logic, not board-level connections
- **Poor Readability**: Netlists are machine-oriented, not human-friendly
- **No Flow Concept**: Existing languages use point-to-point connections
- **Missing Electrical Awareness**: No integrated units or electrical validation
- **Limited Expressiveness**: Cannot naturally express design intent

## Innovation Details

### 1. Flow-Based Connection Syntax

The core innovation is using flow operators to express signal paths:

```bhdl
// Traditional netlist approach (prior art)
NET VCC R1.1
NET N1 R1.2 LED1.A
NET GND LED1.K

// Novel flow-based approach
net led_circuit: VCC -> R1(1k).1 -> R1.2 -> LED1(red).A -> LED1.K -> GND
```

#### Flow Operators

```bhdl
// Unidirectional flow (most common)
power -> component -> output

// Bidirectional connection
sensor <-> processor

// Flow with transformation
analog_in |> adc |> digital_out

// Named flow points
input -> filter -> @filtered_signal -> amplifier

// Multi-path flows  
power -> { 
    branch1 -> load1
    branch2 -> load2
}
```

### 2. Integrated Component Instantiation

Components are instantiated inline with their connections:

```bhdl
// Direct instantiation with parameters
VCC -> Res(4.7k).1 -> node -> Cap(100nF).1 -> GND

// With component identifier
VCC -> R1: Res(4.7k).1 -> R1.2 -> node

// Multiple parameters
sensor -> amp: OpAmp(LM358, gain=10, bandwidth=1MHz).IN+ 

// Positional and named parameters
MCU(STM32F4, clock=16MHz).PA0 -> LED(red, 20mA).A
```

### 3. Electrical Units System

Native understanding of electrical units throughout:

```bhdl
// Resistance with units
Res(4.7kΩ)    // or 4.7k, 4k7
Res(1MΩ)      // or 1M, 1MEG

// Capacitance
Cap(100nF)    // or 0.1µF, 0.1u
Cap(10µF)     // or 10u, 10uF

// Current and voltage constraints
LED(red, If=20mA, Vf=2.1V)
PowerSupply(voltage=5V, current=2A)

// Frequency and time
Crystal(freq=16MHz)
Delay(time=100ms)

// Power
Resistor(10k, power=0.5W)
```

### 4. Progressive Abstraction Levels

#### High-Level Signal Flows
```bhdl
board AudioAmplifier {
    // Abstract signal flow
    audio_input |> preprocessing |> amplification |> speaker_output
    
    // Define subsections
    section preprocessing {
        audio_input -> highpass: Filter(fc=20Hz) -> 
                      compressor: DynamicRange(ratio=3:1) ->
                      @processed_audio
    }
}
```

#### Mid-Level Functional Blocks
```bhdl
section amplification {
    @processed_audio -> preamp: OpAmp(gain=10) ->
                       driver: PowerAmp(TDA2030) ->
                       speaker_output
}
```

#### Low-Level Component Details
```bhdl
// Detailed implementation of preamp
module OpAmp(gain: number) {
    pin IN+: signal in
    pin IN-: signal in  
    pin OUT: signal out
    pin V+: power in
    pin V-: power in
    
    // Internal implementation
    IN+ -> R1: Res(10k).1 -> R1.2 -> @inv_input
    @inv_input -> R2: Res(gain * 10k).1 -> R2.2 -> OUT
    @inv_input -> IN-
}
```

### 5. Hierarchical Design Structure

```bhdl
// Top-level board
board PowerSupply {
    power VIN = 12V @ 2A
    ground GND
    
    // Import modules
    use switching_regulator from "./regulators.bhdl"
    use protection_circuit from "./protection.bhdl"
    
    // High-level flow
    VIN -> protection: protection_circuit -> 
           reg5v: switching_regulator(vout=5V) ->
           @VCC_5V
    
    // Distribution
    @VCC_5V -> {
        -> subsystem1_power
        -> subsystem2_power  
        -> led_indicator_circuit
    }
}

// Reusable module
module switching_regulator(vout: voltage) {
    pin vin: power in
    pin vout: power out
    pin gnd: ground inout
    
    // Buck converter implementation
    vin -> L1: Inductor(10µH).1 -> L1.2 -> @sw_node
    
    // Dynamic component selection based on parameter
    if vout <= 5V {
        @sw_node -> reg: LM2596(vout=vout) -> vout
    } else {
        @sw_node -> reg: LM2576(vout=vout) -> vout  
    }
}
```

### 6. Design Intent Annotations

```bhdl
// Intent for entire flow path
net protected_input: sensor -> tvs: TVSDiode(5.1V).1 -> tvs.2 -> 
                     filter: RC_Filter(fc=1kHz) -> @cleaned_signal
    for protection(overvoltage: 5.1V, noise_rejection: 40dB)

// Multiple intents on branches
net power_distribution: VCC_12V -> {
    -> reg5v: Buck(5V) -> @VCC_5V 
        for regulation(tolerance: 2%, ripple: 50mV)
    
    -> reg3v3: Buck(3.3V) -> @VCC_3V3
        for regulation(tolerance: 1%, ripple: 20mV)
}
```

### 7. Generate Constructs for Repetitive Structures

```bhdl
// LED array with current limiting
generate for i in 0..7 {
    net led_array[i]: PORT[i] -> Res(330).1 -> 
                      LED(red, id="D{i}").A -> GND
}

// Conditional generation
generate for ch in channels {
    if ch.differential {
        net diff_input[ch.id]: 
            IN[ch.id]+ -> R1: Res(10k) -> amp: DiffAmp().IN+
            IN[ch.id]- -> R2: Res(10k) -> amp.IN-
            amp.OUT -> ADC[ch.id]
    } else {
        net single_input[ch.id]:
            IN[ch.id] -> Buffer() -> ADC[ch.id]
    }
}
```

### 8. Net Naming and Referencing

```bhdl
// Named nets with @ syntax
VCC -> @filtered_vcc: Cap(10µF) -> GND
@filtered_vcc -> subcircuit_power

// Net assignment creates implicit handle
protected_vin: TVSDiode(15V).1
protected_vin -> regulator.input

// Multi-point net references
net i2c_sda: MCU.PA4 <-> @sda_bus <-> Sensor1.SDA <-> Sensor2.SDA
@sda_bus -> PullUp(4.7k) -> VCC_3V3
```

### 9. Constraint Specification

```bhdl
// Current constraints
net led_circuit: VCC -> Res(?, current=20mA) -> LED -> GND
    // Resistance value inferred from constraints

// Matched components
net differential_pair: {
    IN+ -> R1: Res(10k, tolerance=1%, tempco=25ppm) -> OUT+
    IN- -> R2: Res(10k, match=R1) -> OUT-
}

// Trace constraints  
net high_speed: FPGA.DOUT -> @critical_trace -> ADC://Input
    with trace(impedance=50Ω, length=matched, diff_pair=true)
```

### 10. Power and Ground Declarations

```bhdl
board MixedSignalBoard {
    // Multiple power domains
    power VCC_DIGITAL = 3.3V @ 500mA
    power VCC_ANALOG = 3.3V @ 200mA isolated
    power VCC_MOTOR = 12V @ 5A
    
    // Multiple grounds
    ground GND_DIGITAL
    ground GND_ANALOG isolated
    ground GND_POWER
    
    // Star ground connection
    star_ground at J1.1 connects {
        GND_DIGITAL,
        GND_ANALOG,
        GND_POWER
    }
}
```

## Comparison with Prior Art

| Feature | Traditional HDL | Schematic | SPICE | This Innovation |
|---------|----------------|-----------|--------|-----------------|
| Board-level focus | No | Yes | Partial | Yes |
| Text-based | Yes | No | Yes | Yes |
| Human readable | Partial | N/A | No | Yes |
| Flow concept | No | No | No | Yes |
| Electrical units | No | Yes | Partial | Yes |
| Abstraction levels | Logic only | Fixed | None | Multiple |
| Version control | Yes | Poor | Yes | Yes |
| Parameterization | Yes | Limited | Yes | Yes |

## Novel Aspects Summary

1. **Flow-Based Syntax**: Natural expression of signal flow through components
2. **Board-Level Focus**: Designed specifically for PCB design, not IC design
3. **Integrated Instantiation**: Components created inline with connections
4. **Electrical Awareness**: Native understanding of units and constraints
5. **Progressive Detail**: From high-level flows to specific implementations
6. **Design Intent**: Explicit capture of why, not just what
7. **@ Net References**: Clear disambiguation of net references vs components

## Example: Complete Board Design

```bhdl
board USBPowerAdapter {
    description = "5V/2A USB power adapter with protection"
    
    // Interface definitions
    power VIN_AC = 120V @ 60Hz
    ground EARTH
    connector USB_OUT type USB_A
    
    // AC input section
    section ac_input {
        VIN_AC -> F1: Fuse(2A) -> @fused_ac
        @fused_ac -> MOV1: MOV(150V) -> EARTH
        @fused_ac -> bridge: BridgeRectifier(1N4007) -> @rectified
        bridge.AC2 -> EARTH
        @rectified -> C1: Cap(100µF, 200V) -> DC_GND
    }
    
    // DC-DC conversion
    section dc_conversion {
        @rectified -> xfmr: Transformer(ratio=10:1, isolation=3kV) -> @isolated_dc
        @isolated_dc -> reg: SwitchingRegulator(vout=5V, iout=2A) -> @vcc_5v
        reg.FB -> divider: VoltageDivider(ratio=0.5) -> @vcc_5v
    }
    
    // Output protection and filtering  
    section output {
        net usb_power: @vcc_5v -> 
            L1: Inductor(10µH) ->
            polyfuse: PolyFuse(2.5A) ->
            @protected_5v
            for protection(overcurrent: 2.5A)
            
        @protected_5v -> C2: Cap(100µF) -> USB_GND
        @protected_5v -> C3: Cap(0.1µF) -> USB_GND
        
        // USB connections
        @protected_5v -> USB_OUT.VBUS
        USB_GND -> USB_OUT.GND
        USB_OUT.D+ -> Res(10k) -> USB_OUT.D-  // Signal 2A capable
    }
}
```

## Industrial Applications

1. **PCB Design Tools**: Next-generation text-based PCB design
2. **Design Automation**: Scriptable, parameterizable board generation
3. **Continuous Integration**: Version-controlled hardware design
4. **Design Reuse**: Modular, hierarchical component libraries
5. **Educational Tools**: Teaching electronics with readable syntax

## Conclusion

This board-level HDL with flow-based syntax represents a fundamental shift in how electronic circuits are described textually. By focusing on signal flow, integrating electrical units, and providing multiple abstraction levels, it enables more intuitive, maintainable, and verifiable hardware descriptions than existing approaches.

---

*This publication is intended to establish prior art and ensure these innovations remain freely available for use by the engineering community. No patent rights are sought or reserved.*