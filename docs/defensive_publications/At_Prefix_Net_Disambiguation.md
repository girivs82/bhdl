# Defensive Publication: @ Prefix for Net Disambiguation in HDL

**Publication Date**: [DATE]  
**Authors**: [Your Name]  
**Contact**: [Your Email]

## Abstract

This publication discloses a novel syntax mechanism for disambiguating net references from component identifiers in hardware description languages. The innovation uses the @ prefix to explicitly mark net references, solving long-standing ambiguity issues in HDLs where the same identifier could refer to either a component instance or a net. This simple yet powerful mechanism improves code clarity, reduces errors, enables better tooling support, and allows nets and components to share namespaces without conflicts.

## Background and Prior Art

### Traditional HDL Net/Component Ambiguity

In traditional HDLs, the same syntax is often used for both nets and components:

1. **Verilog Ambiguity**:
   ```verilog
   wire data;
   DataBuffer data(...);  // Name collision
   
   // Is 'data' the wire or the module?
   assign out = data;
   ```

2. **VHDL Disambiguation**:
   ```vhdl
   signal clk : std_logic;
   component clk_gen ...
   
   -- Must use different names or context
   ```

3. **SPICE Conventions**:
   ```spice
   * Nodes are just names
   R1 N001 N002 1k
   * No visual distinction between nodes and components
   ```

### Limitations of Prior Art

- **Namespace Conflicts**: Cannot use same name for net and component
- **Context Dependence**: Reader must understand context to interpret identifier
- **Poor Readability**: No visual cue distinguishing nets from components
- **Tooling Complexity**: Parsers need complex context analysis
- **Refactoring Hazards**: Changing identifier type requires careful analysis

## Innovation Details

### 1. The @ Prefix Convention

The innovation introduces @ as a prefix specifically for net references:

```bhdl
// Component instantiation (no prefix)
VCC -> R1: Res(10k).1 -> R1.2 -> @node1

// Net reference (with @ prefix)
@node1 -> C1: Cap(100n).1 -> GND

// Clear distinction in same statement
@filtered_vcc -> amp: OpAmp() -> @output
```

### 2. Solving Namespace Conflicts

The @ prefix allows nets and components to coexist in the same namespace:

```bhdl
// Traditional approach - requires different names
net filtered_power: VCC -> Cap(10u) -> filtered_power_net
component filtered_power: PowerFilter()
filtered_power_net -> filtered_power.input  // Confusing!

// With @ prefix - same logical name, clear distinction  
net power: VCC -> Cap(10u) -> @power
component power: PowerFilter()
@power -> power.input  // Clear: net feeds component
```

### 3. Visual Parsing Benefits

The @ prefix provides immediate visual distinction:

```bhdl
// Complex statement - easy to parse visually
@sensor_data -> filter.input -> filter.output -> @filtered_data -> 
    adc: ADC().input -> adc.output -> @digital_data -> processor.GPIO1

// Without @ prefix (harder to parse)
sensor_data -> filter.input -> filter.output -> filtered_data -> 
    adc: ADC().input -> adc.output -> digital_data -> processor.GPIO1
```

### 4. Net Declaration and Reference Syntax

```bhdl
// Inline net creation
VCC -> @filtered_vcc: Cap(10u) -> GND

// Subsequent reference  
@filtered_vcc -> subcircuit.power

// Net assignment with implicit handle
protected_input: protection_circuit.output
@protected_input -> amplifier.input

// Multiple references to same net
net i2c_bus: MCU.SDA <-> @sda <-> Sensor1.SDA <-> Sensor2.SDA
@sda -> PullUp(4.7k).1 -> VCC
```

### 5. Scope and Hierarchical References

```bhdl
board MainBoard {
    // Local net
    VCC -> @local_power -> subcircuit
    
    entity PowerSupply {
        // Entity-local net
        input -> @filtered -> output
        
        // Reference parent net (requires qualification)
        parent.@local_power -> indicator_led
    }
    
    // Hierarchical net reference
    PowerSupply.@filtered -> measurement_point
}
```

### 6. Benefits for Tool Development

#### Parser Simplification
```rust
// Traditional parser - needs context
match identifier {
    id if is_component_in_scope(id) => ParsedItem::Component(id),
    id if is_net_in_scope(id) => ParsedItem::Net(id),
    _ => ParsedItem::Error("Ambiguous identifier")
}

// With @ prefix - immediate classification
match token {
    Token::AtIdentifier(name) => ParsedItem::Net(name),
    Token::Identifier(name) => ParsedItem::Component(name),
}
```

#### Static Analysis
```rust
// Easy to find all net references
fn find_net_references(ast: &AST) -> Vec<NetRef> {
    ast.traverse()
       .filter(|node| node.is_at_identifier())
       .map(|node| NetRef::from(node))
       .collect()
}
```

### 7. Integration with Flow Syntax

The @ prefix integrates naturally with flow-based syntax:

```bhdl
// Named intermediate points in flow
power -> protection -> @protected -> regulator -> @regulated -> load

// Branch points
@main_power -> {
    -> @branch1 -> subsystem1
    -> @branch2 -> subsystem2
}

// Rejoining flows
@branch1 -> process1 -> @processed1
@branch2 -> process2 -> @processed2  
@processed1, @processed2 -> combiner -> @output
```

### 8. Backward Compatibility Strategy

For languages adopting this innovation:

```bhdl
// Compatibility mode - both syntaxes accepted
#pragma compatibility_mode

// Old style (still works)
net power: VCC -> Cap(10u) -> power_net
power_net -> load

// New style (preferred)
net power: VCC -> Cap(10u) -> @power
@power -> load

// Compiler warning encourages migration
// WARNING: Consider using @power instead of power_net
```

### 9. Extended Applications

#### Bus and Array References
```bhdl
// Bus nets with @ prefix
@data_bus[7:0] -> processor.PORT_A[7:0]

// Individual bit reference
@data_bus[3] -> debug_led

// Generated nets
generate for i in 0..7 {
    processor.PORT_B[i] -> @control[i] -> device[i].enable
}
```

#### Net Properties and Constraints
```bhdl
// Properties attached to nets
@high_speed_clock -> buffer.input
    with @high_speed_clock.constraint(frequency=100MHz, jitter<10ps)

// Differential pairs
@diff_p, @diff_n -> receiver.IN_P, receiver.IN_N
    with differential_pair(@diff_p, @diff_n, impedance=100)
```

### 10. Error Prevention

The @ prefix prevents common errors:

```bhdl
// Error: Cannot assign net to component property
filter.cutoff = @node1  // TYPE ERROR: Expected value, got net

// Error: Cannot use component as net
VCC -> filter -> load  // ERROR: filter is component, not net
                      // Did you mean: filter.output?

// Correct versions
filter.cutoff = 1kHz
VCC -> filter.input -> filter.output -> load
```

## Implementation Considerations

### Lexical Analysis
```rust
// Simple lexer rule
Rule::AtIdentifier => {
    consume('@');
    let name = consume_identifier();
    Token::NetReference(name)
}
```

### Symbol Table Design
```rust
struct SymbolTable {
    components: HashMap<String, ComponentInfo>,
    nets: HashMap<String, NetInfo>,
    // No naming conflicts - different namespaces
}

impl SymbolTable {
    fn resolve(&self, name: &str, is_net: bool) -> Option<Symbol> {
        if is_net {
            self.nets.get(name).map(Symbol::Net)
        } else {
            self.components.get(name).map(Symbol::Component)
        }
    }
}
```

## Comparison with Prior Art

| Aspect | Traditional HDL | SPICE | This Innovation |
|--------|----------------|--------|-----------------|
| Net/Component Distinction | Context-based | None | Explicit @ prefix |
| Namespace Sharing | No | N/A | Yes |
| Visual Clarity | Poor | Poor | Excellent |
| Parser Complexity | High | Low | Low |
| Refactoring Safety | Low | Low | High |
| Learning Curve | Moderate | Low | Low |

## Novel Aspects Summary

1. **Explicit Prefix**: @ unambiguously marks net references
2. **Namespace Separation**: Nets and components can share names
3. **Visual Parsing**: Immediate recognition without context
4. **Tool Simplification**: Parsers and analyzers become simpler
5. **Error Prevention**: Type system can catch net/component confusion
6. **Backward Compatible**: Can be adopted gradually

## Example: Complex Circuit with Clear Net References

```bhdl
board MixedSignalProcessor {
    // Components and nets with same logical names
    component power: PowerManager()
    net @power: power.output -> @power  // Clear distinction
    
    // Signal flow with named intermediate nets
    section analog_input {
        sensor -> @raw_signal: protection() -> @raw_signal
        @raw_signal -> filter: LowPass(fc=1kHz) -> @filtered
        @filtered -> amp: Amplifier(gain=10) -> @amplified
        @amplified -> adc: ADC(bits=16) -> @digital
    }
    
    // Multiple net references
    section digital_processing {
        @digital -> processor.input
        @digital -> debug.monitor
        @digital -> recorder.store()
        
        // Clear that @digital is a net being routed to multiple destinations
    }
    
    // Hierarchical references
    analog_input.@filtered -> test_point.TP1
    digital_processing.@digital -> external_connector.PIN_3
}
```

## Conclusion

The @ prefix for net disambiguation represents a simple yet powerful innovation in HDL syntax design. By providing explicit, visual distinction between nets and components, it improves code clarity, reduces errors, simplifies tool development, and enables more flexible naming schemes. This innovation can be adopted by existing HDLs or incorporated into new language designs with minimal disruption.

---

*This publication is intended to establish prior art and ensure these innovations remain freely available for use by the engineering community. No patent rights are sought or reserved.*