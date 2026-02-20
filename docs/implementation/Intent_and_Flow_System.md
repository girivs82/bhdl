# Intent and Flow System Implementation

## Overview

The BHDL intent and flow system provides a declarative way to specify simulation requirements and propagate them through signal flows. This document describes the complete implementation including parsing, resolution, flow tracking, and hierarchical propagation.

## Architecture

### 1. Intent System Components

#### Intent Declaration Syntax
```bhdl
net signal_name: flow_expression for intent_name(params);
```

Example:
```bhdl
net audio_in: @VCC -> Res(10k).1 -> Cap(100nF).1 for analog(bandwidth: 10kHz);
```

#### Intent Functions (bhdl-stdlib)
- `delay(time)` - Requires analog simulation for timing
- `analog(bandwidth)` - Requires analog simulation with frequency constraints  
- `digital()` - Pure digital simulation
- `mixed_signal(sample_rate)` - Mixed analog/digital simulation
- `power_analysis(tolerance)` - Power domain analysis
- `thermal(max_temp)` - Thermal simulation
- `signal_integrity(rise_time)` - Signal integrity analysis

### 2. Flow Tracking System

The flow tracker (`bhdl-analyzer/src/flow_tracking.rs`) identifies signal paths and propagates intents:

```rust
pub struct FlowPath {
    pub id: usize,
    pub nets: Vec<String>,
    pub components: Vec<String>,
    pub intent: Option<IntentCall>,
    pub intent_result: Option<IntentResult>,
}
```

Key features:
- Traces component instantiations in flows
- Tracks nets involved in signal paths
- Associates intents with entire flow paths
- Supports hierarchical propagation through modules

### 3. Parser Extensions

Added intent clause parsing to flow statements:

```rust
// In parse_v2_connection_expr and parse_flow_stmt
if self.has_intent_clause() {
    self.parse_intent_clause();
}
```

### 4. Hierarchical Intent Propagation

Module instances inherit intents from their parent flows:

```rust
pub fn propagate_hierarchical_intents(&mut self, 
    symbol_table: &SymbolTable,
    definition_scopes: &HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>
) {
    // Find entity instances in flows with intents
    // Create new flow paths for entity internals
    // Propagate intent to internal components
}
```

## Net Syntax Consistency

### @ Prefix Requirement

All net references (including power/ground) now require the @ prefix:

```bhdl
// Correct
@VCC -> Res(10k).1;
LED.K -> @GND;

// Incorrect (will produce error)
VCC -> Res(10k).1;  // Error: Net 'VCC' must be referenced with @ prefix
```

### Implementation Details

1. **Pass 2 Validation**: Added IDENT_REF checking to validate bare identifiers
2. **Power Analysis Update**: Modified to check for @ prefix before domain lookups
3. **NET_REF Error Handling**: Fixed to properly push diagnostics

## Power Domains as Nets

### NetAttribute System

Power domains are now stored as net attributes rather than separate entities:

```rust
pub enum NetAttribute {
    PowerDomain {
        voltage: f64,
        tolerance: f64,
        max_current: f64,
        controllable: bool,
        enable_signal: Option<String>,
        startup_delay_ms: f64,
        sequence_priority: u32,
        dependencies: Vec<String>,
    },
    GroundDomain,
    Generic(HashMap<String, String>),
}
```

### Benefits
- Unified representation of all nets
- Power domains visible in symbol table
- Consistent handling in analysis passes
- Supports future attribute extensions

## Unit Conversion

Added comprehensive electrical unit parsing with proper multipliers:

```rust
// Supported units
"mA" => 0.001      // milliamps
"μA" => 0.000001   // microamps  
"mV" => 0.001      // millivolts
"kΩ" => 1000.0     // kilohms
"μF" => 1e-6       // microfarads
// ... and more
```

## Usage Examples

### Basic Flow with Intent
```bhdl
board AudioBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Analog flow for audio processing
    net audio_path: @VCC -> Res(10k).1 -> Cap(100nF).1 for analog(bandwidth: 20kHz);
    
    // Digital control signals
    net control: MCU.GPIO1 -> LED.A for digital();
}
```

### Hierarchical Entity with Intent Propagation
```bhdl
entity FilterStage(cutoff: frequency) {
    pin IN: signal in;
    pin OUT: signal out;
    // Internal components inherit parent intent
}

board System {
    // Intent propagates into filter1 module
    net filtered: Input.sig -> filter1.IN for analog(bandwidth: 10kHz);
    
    filter1: FilterStage(1kHz) {
        OUT -> Output.sig;
    }
}
```

## Testing

Comprehensive test suite added:
- `test_net_syntax_comprehensive.rs` - @ prefix validation
- `test_flow_intent_parsing.rs` - Intent parsing on flows
- `test_power_as_nets.rs` - Power domains as net attributes
- `test_hierarchical_intent_module.rs` - Module propagation
- `test_flow_intent_basic.rs` - Basic flow tracking

## Integration Points

1. **Analyzer Pass 9**: Flow tracking and intent resolution
2. **Symbol Table**: Extended with net_attributes field
3. **Power Analysis**: Loads domains from symbol table
4. **Simulation Coordinator**: Uses flow tracker results

## Future Enhancements

1. Cross-module intent validation
2. Intent conflict resolution
3. Dynamic intent modification during simulation
4. Intent-based optimization hints
5. Tool-specific intent extensions