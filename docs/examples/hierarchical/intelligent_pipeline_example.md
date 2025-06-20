# Intelligent Pipeline Example

## Scenario: Multi-Channel LED Driver

```bhdl
module LEDChannel(max_current: current = 350mA) {
    pin VIN: power in;
    pin LED_A: current out;
    pin LED_K: current in;
    pin DIM: analog in;
    pin FAULT: digital out;
    
    // Current control
    driver: AL8861 {
        VIN -> .VIN;
        .LED -> LED_K;
        .DIM -> DIM;
        .FAULT -> FAULT;
    }
    
    // Current sense resistor
    R_sense: Res(0.3, 1W) {  // 0.3Ω for 350mA
        LED_K -> .1;
        GND -> .2;
    }
    
    // Input bypass
    C_in: Cap(1uF) {
        VIN -> .1;
        GND -> .2;
    }
    
    // LED connection
    LED_A -> LED: LED(white, 3W).A;
    LED.K -> LED_K;
}

board FourChannelLEDDriver {
    power VIN = 24V @ 2A;
    ground GND;
    
    // Four identical channels
    ch1: LEDChannel() { VIN -> .VIN; }
    ch2: LEDChannel() { VIN -> .VIN; }
    ch3: LEDChannel() { VIN -> .VIN; }
    ch4: LEDChannel() { VIN -> .VIN; }
    
    // One different channel (high power)
    ch_high: LEDChannel(max_current=700mA) { VIN -> .VIN; }
}
```

## Traditional Pipeline Output

### Reference Designators (Confusing)
```
U1, R1, C1, D1     // ch1
U2, R2, C2, D2     // ch2  
U3, R3, C3, D3     // ch3
U4, R4, C4, D4     // ch4
U5, R5, C5, D5     // ch_high

// Which R is which? No clear pattern!
```

### Analysis (Redundant)
```
Analyzing ch1.driver... [500ms]
Analyzing ch1.R_sense... [100ms]
Analyzing ch1.LED... [200ms]

Analyzing ch2.driver... [500ms]  // Same work!
Analyzing ch2.R_sense... [100ms]
Analyzing ch2.LED... [200ms]

... repeated for ch3, ch4

Total: 3200ms for 5 channels
```

## Intelligent Pipeline Output

### Smart Reference Designators
```
U1_1, R1_1, C1_1, D1_1    // ch1
U1_2, R1_2, C1_2, D1_2    // ch2
U1_3, R1_3, C1_3, D1_3    // ch3  
U1_4, R1_4, C1_4, D1_4    // ch4
U1_5, R2_5, C1_5, D1_5    // ch_high (R2 because different value!)

// Clear pattern: component_instance
```

### Optimized Analysis
```
Module signature: LEDChannel(max_current=350mA)
Analyzing once... [800ms]
Applying to: ch1, ch2, ch3, ch4

Module signature: LEDChannel(max_current=700mA)  
Analyzing once... [800ms]
Applying to: ch_high

Total: 1600ms (2x faster!)
```

### SPICE Safety Analysis
```rust
// Internal processing
let signatures = detect_signatures(&board);
// Found: 
//   - LEDChannel(350mA) x 4
//   - LEDChannel(700mA) x 1

// Analyze representatives
let result_350 = analyze_module(&ch1);  // Representative
let result_700 = analyze_module(&ch_high);

// Apply to instances with context
for ch in [ch1, ch2, ch3, ch4] {
    results.add(apply_with_context(result_350, ch));
}
results.add(result_700);  // Already analyzed

// Safety report shows:
"LEDChannel(350mA) instances: ch1, ch2, ch3, ch4
 - Current limit: 350mA ✓
 - Power dissipation: 0.3W @ 0.3Ω ✓
 - LED rating: 3W (8.5x margin) ✓

LEDChannel(700mA) instances: ch_high  
 - Current limit: 700mA ✓
 - Power dissipation: 1.2W @ 0.3Ω ⚠️ (close to 1W limit)
 - LED rating: 3W (4.3x margin) ✓
 - Recommendation: Use 2W sense resistor"
```

### Layout Generation
```
// Template layout for LEDChannel computed once
LEDChannel_template {
    width: 20mm
    height: 15mm
    U1 at (10, 7.5)
    R1 at (5, 10)
    C1 at (5, 5)
    D1 at (15, 7.5)
}

// Placed in grid
[ch1] [ch2]
[ch3] [ch4]
[ch_high]  // Slightly different layout due to R value

// Total layout time: 200ms vs 1000ms
```

### Netlist Structure
```json
{
  "modules": {
    "LEDChannel_350mA": {
      "signature": {"max_current": "350mA"},
      "components": {
        "U1": {"type": "AL8861"},
        "R1": {"type": "Resistor", "value": "0.3"},
        "C1": {"type": "Capacitor", "value": "1uF"},
        "D1": {"type": "LED", "spec": "white,3W"}
      }
    },
    "LEDChannel_700mA": {
      "signature": {"max_current": "700mA"},
      "components": {
        "U1": {"type": "AL8861"},
        "R2": {"type": "Resistor", "value": "0.15"},  // Different!
        "C1": {"type": "Capacitor", "value": "1uF"},
        "D1": {"type": "LED", "spec": "white,3W"}
      }
    }
  },
  "instances": [
    {"name": "ch1", "module": "LEDChannel_350mA"},
    {"name": "ch2", "module": "LEDChannel_350mA"},
    {"name": "ch3", "module": "LEDChannel_350mA"},
    {"name": "ch4", "module": "LEDChannel_350mA"},
    {"name": "ch_high", "module": "LEDChannel_700mA"}
  ]
}
```

### BOM Generation
```
Component Summary:
- AL8861 (U1): 5x (4x in 350mA channels, 1x in 700mA channel)
- Resistor 0.3Ω 1W (R1): 4x (350mA channels)
- Resistor 0.15Ω 2W (R2): 1x (700mA channel)  
- Capacitor 1uF (C1): 5x (all channels)
- LED white 3W (D1): 5x (all channels)

// Note: Different resistor values properly tracked!
```

## Advanced Example: Generated Arrays

```bhdl
module ParallelDriverBank(channels: int = 8) {
    generate for i in 0..channels {
        ch[i]: LEDChannel() { ... }
    }
}

board Display {
    // 32 channels total!
    bank1: ParallelDriverBank(channels=16) { ... }
    bank2: ParallelDriverBank(channels=16) { ... }
}
```

### Super-Intelligent Output
```
Detecting patterns...
- Found: ch[0..15] in bank1 (identical)
- Found: ch[0..15] in bank2 (identical)
- Total unique modules: 1 (LEDChannel)

Reference designators:
bank1.ch[0]: U1_1_1, R1_1_1, C1_1_1, D1_1_1
bank1.ch[1]: U1_1_2, R1_1_2, C1_1_2, D1_1_2
...
bank2.ch[15]: U1_2_16, R1_2_16, C1_2_16, D1_2_16

Analysis time:
- Traditional: 32 * 800ms = 25.6 seconds
- Intelligent: 1 * 800ms = 0.8 seconds (32x faster!)

Layout: Grid of 4x8 repeated modules
```

## Benefits Summary

1. **Clear Naming**: `R1_2_5` = Resistor 1, bank 2, channel 5
2. **Fast Analysis**: Analyze once, apply many times
3. **Accurate BOMs**: Same components grouped correctly
4. **Smart Layout**: Template-based placement
5. **Debugging**: Easy to trace issues to specific instances
6. **Memory Efficient**: One module definition, many references

## Implementation Notes

The pipeline tracks:
- Module signatures (type + parameters)
- Instance paths (hierarchical location)  
- Component positions within modules
- Analysis results per signature
- Layout templates per signature

This intelligence is transparent to the user but provides massive performance and usability benefits!