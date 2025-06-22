# Intent Syntax Alignment Plan

## Summary

The intent system implementation currently requires a `net` keyword that conflicts with BHDL's flow-based design philosophy. This document outlines a plan to align the intent syntax with BHDL's natural, non-declarative approach.

## Current State

### Implementation
```bhdl
// Parser expects this syntax (with 'net' keyword)
net critical: VCC -> Res(10k).1 -> LED(red).A for delay(3ms);
```

### BHDL Philosophy
- No declarative keywords for nets
- Natural flow-based connections
- Implicit net creation through connections
- @ prefix for explicit net naming within flows

## Proposed Solution

Support intent clauses on existing flow constructs without introducing declarative keywords:

### 1. Named Flow with Intent
```bhdl
// Flow statement with optional intent
critical_path: VCC -> Res(10k).1 -> LED(red).A for delay(3ms);
```

### 2. Direct Connection with Intent  
```bhdl
// Anonymous connection with intent
VCC -> Res(10k).1 -> LED(red).A for delay(3ms);
```

### 3. @ Named Net with Intent
```bhdl
// Inline net naming with intent
VCC @filtered-> Cap(0.1uF).1 -> amp.in for anti_alias(before: adc);
```

## Implementation Tasks

### Phase 1: Parser Updates
1. **Remove NET_FLOW_STMT**: Deprecate the net flow statement entirely
2. **Update FLOW_STMT**: Add optional intent clause to flow statements
3. **Update CONNECTION_STMT**: Add optional intent clause to connections
4. **Enhance @ syntax**: Support intent after @ named connections

### Phase 2: AST Changes
1. Add `intent_clause` field to `FlowStmt`
2. Add `intent_clause` field to `ConnectionStmt`
3. Remove `NetFlowStmt` AST node
4. Update flow expression nodes for @ syntax with intent

### Phase 3: Flow Tracker Updates
1. Extract intents from flow statements
2. Extract intents from connection statements
3. Handle intent propagation through @ named nets

## Parser Implementation Details

### Current parse_flow_stmt()
```rust
pub(crate) fn parse_flow_stmt(&mut self) {
    self.builder.start_node(SyntaxKind::FLOW_STMT.into());
    self.expect(SyntaxKind::IDENT); // Flow name
    self.expect(SyntaxKind::COLON);
    
    // Parse flow expression
    self.builder.start_node(SyntaxKind::FLOW_EXPR.into());
    self.parse_expr(0);
    self.builder.finish_node();
    
    self.expect(SyntaxKind::SEMI);
    self.builder.finish_node();
}
```

### Updated parse_flow_stmt()
```rust
pub(crate) fn parse_flow_stmt(&mut self) {
    self.builder.start_node(SyntaxKind::FLOW_STMT.into());
    self.expect(SyntaxKind::IDENT); // Flow name
    self.expect(SyntaxKind::COLON);
    
    // Parse flow expression
    self.builder.start_node(SyntaxKind::FLOW_EXPR.into());
    self.parse_expr(0);
    self.builder.finish_node();
    
    // NEW: Check for optional intent clause
    if self.has_intent_clause() {
        self.parse_intent_clause();
    }
    
    self.expect(SyntaxKind::SEMI);
    self.builder.finish_node();
}
```

### Updated parse_v2_connection_expr()
```rust
pub(crate) fn parse_v2_connection_expr(&mut self) {
    self.builder.start_node(SyntaxKind::CONNECTION_STMT.into());
    
    self.parse_expr(0);
    
    // NEW: Check for optional intent clause
    if self.has_intent_clause() {
        self.parse_intent_clause();
    }
    
    self.expect(SyntaxKind::SEMI);
    self.builder.finish_node();
}
```

## Migration Path

### Phase 1: Support Both Syntaxes (v0.2.0)
- Keep `net` keyword support for backward compatibility
- Add support for flow-based intent syntax
- Mark `net` keyword as deprecated in documentation

### Phase 2: Transition Period (v0.3.0)
- Emit deprecation warnings for `net` keyword usage
- Update all examples and documentation
- Provide migration tool

### Phase 3: Remove Old Syntax (v1.0.0)
- Remove `net` keyword support entirely
- Clean up parser and AST

## Benefits

1. **Consistency**: Aligns with BHDL's flow-based philosophy
2. **Simplicity**: No special declarative syntax for intents
3. **Flexibility**: Attach intents to any flow or connection
4. **Natural**: Reads like natural circuit description with optional intent

## Example: Complete Circuit with Flow-Based Intents

```bhdl
board SafePowerSupply {
    power VIN = 12V @ 2A;
    power VCC = 5V @ 1A;
    ground GND;
    
    // Protection with intent - no 'net' keyword
    VIN -> fuse: Fuse(2A).1 -> @protected_vin for input_protection(overvoltage: 15V);
    
    // Regulation with intent
    @protected_vin -> reg: LM7805().IN for safety_critical(sil: 2);
    reg.OUT -> @VCC_RAW;
    
    // Filtering with intent
    filter_stage: @VCC_RAW -> Cap(100uF).+ -> @VCC for noise_filtering(cutoff: 1kHz);
    
    // Direct connection with timing intent
    @VCC -> Res(10k).1 -> Cap(1uF).1 -> delayed_enable for delay(3ms);
    
    // Multiple components with shared intent
    sensor_path: sensor -> amp: OpAmp().+ -> filter -> adc
        for precision_measurement(bandwidth: 10kHz, noise_floor: -60dB);
}
```

## Conclusion

This alignment brings the intent system in harmony with BHDL's core philosophy while maintaining all the power of intent-based design. The syntax becomes more natural and less declarative, fitting seamlessly into the flow-based paradigm.