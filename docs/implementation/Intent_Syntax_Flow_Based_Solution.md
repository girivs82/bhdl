# Flow-Based Intent Syntax Solution

## Problem Statement

The current intent implementation requires a `net` keyword:
```bhdl
net critical: VCC -> Res(10k).1 -> LED(red).A for delay(3ms);
```

This conflicts with BHDL's core philosophy of natural, flow-based syntax without declarative constructs.

## Proposed Solution: Intent on Flow Statements

Align intent syntax with BHDL's flow-based philosophy by attaching intents to existing flow constructs:

### 1. Intent on Named Flows
```bhdl
// Named flow with intent - no 'net' keyword needed
critical_path: VCC -> Res(10k).1 -> LED(red).A for delay(3ms);
```

### 2. Intent on Direct Connections
```bhdl
// Direct connection with intent
VCC -> Res(10k).1 -> LED(red).A for delay(3ms);
```

### 3. Intent on @ Named Nets
```bhdl
// Named net with @ syntax and intent
VCC @filtered-> Cap(0.1uF).1 -> amp.in for anti_alias(before: adc);
```

## Parser Changes Required

### 1. Modify Flow Statement Grammar

Current:
```
flow_stmt = identifier ":" flow_expr ";"
net_flow_stmt = "net" identifier ":" flow_expr "for" intent_clause ";"
```

Proposed:
```
flow_stmt = identifier ":" flow_expr ["for" intent_clause] ";"
```

### 2. Modify Connection Statement Grammar

Current:
```
connection_stmt = flow_expr ";"
```

Proposed:
```
connection_stmt = flow_expr ["for" intent_clause] ";"
```

### 3. Update @ Syntax to Support Intent

Current:
```
named_connection = expr "@" identifier "->" expr
```

Proposed:
```
named_connection = expr "@" identifier "->" expr ["for" intent_clause]
```

## Implementation Plan

### Phase 1: Update Parser (Priority: High)

1. Remove `parse_net_flow_stmt` function
2. Update `parse_flow_stmt` to optionally parse intent clause
3. Update `parse_connection_stmt` to optionally parse intent clause
4. Update flow expression parsing to handle intent after @ syntax

### Phase 2: Update AST (Priority: High)

1. Remove `NetFlowStmt` AST node
2. Add `intent_clause` field to `FlowStmt`
3. Add `intent_clause` field to `ConnectionStmt`
4. Update flow expression nodes to carry intent information

### Phase 3: Update Flow Tracking (Priority: Medium)

1. Modify flow tracker to extract intents from flow statements
2. Handle intents on anonymous connections
3. Track intent propagation through @ named nets

## Benefits

1. **Natural Syntax**: No declarative keywords, just flow with optional intent
2. **Flexibility**: Attach intent to any flow, named or anonymous
3. **Consistency**: Aligns with BHDL's flow-based philosophy
4. **Simplicity**: Fewer grammar rules, more intuitive

## Example: Complete Board with Flow-Based Intents

```bhdl
board PowerSupply {
    power VIN = 12V @ 2A;
    power VCC = 5V @ 1A;
    ground GND;
    
    // Named flow with protection intent
    protected_input: VIN -> fuse: Fuse(2A).1 -> @protected_vin 
        for input_protection(overvoltage: 15V);
    
    // Direct connection with safety intent
    @protected_vin -> reg: LM7805().IN for safety_critical;
    
    // Component output with filtering intent
    reg.OUT -> Cap(100uF).+ -> @VCC_FILTERED for noise_filtering(cutoff: 1kHz);
    
    // Anonymous flow with timing intent
    @VCC_FILTERED -> Res(10k).1 -> Cap(1uF).1 -> delay_out
        for delay(3ms);
    
    // @ syntax with intent
    sensor_in @conditioned-> filter: LowPass(10kHz).in 
        for signal_conditioning;
}
```

## Migration Strategy

1. Support both syntaxes temporarily (deprecation period)
2. Update all examples and documentation
3. Provide automated migration tool
4. Remove `net` keyword support in next major version

## Conclusion

This solution preserves BHDL's flow-based philosophy while enabling powerful intent annotations. Users can naturally express circuit connections and optionally add intent to guide tools - without introducing declarative syntax that breaks the flow paradigm.