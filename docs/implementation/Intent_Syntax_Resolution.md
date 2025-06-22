# Intent Syntax Resolution

## Issue Summary

There is a syntax inconsistency between the BHDL specification and the intent system implementation:

1. **BHDL Specification (v2.0)**: Does not include a `net` keyword. Nets are created through:
   - Direct connections: `VCC -> Res(4.7kΩ).1 -> LED(red).A;`
   - Named nets with @ prefix: `VCC @FILTERED-> r1: Res(4.7kΩ).1;`
   - Net references: `fuse.2 -> @protected_vin -> TVSDiode(15V).1;`

2. **Intent System Implementation**: Requires `net` keyword for intent attachment:
   - Parser expects: `net name: flow_expr for intent_clause;`
   - AST expects NET_KW token in NetFlowStmt node
   - All documentation shows this syntax

## Test Results

Testing different syntax options shows:
- ✓ `net critical: VCC -> R(1k).1 for delay(3ms);` - Parses successfully
- ✗ `critical: VCC -> R(1k).1 for delay(3ms);` - Parse error
- ✗ `VCC -> R(1k).1 for delay(3ms);` - Parse error

## Analysis

The intent system was designed and implemented with the `net` keyword syntax, as evidenced by:
- Intent System Implementation Plan explicitly mentions "Update grammar to support intent attachment to net declarations"
- Defensive publications show `net sensor_path: sensor -> protection -> adc for high_reliability(redundancy=2)`
- All test cases use the `net` keyword

## Proposed Resolution

### Option 1: Update BHDL Specification (Recommended)

Add the `net` keyword to BHDL v2.0 specification for explicit net declarations with intents:

```bhdl
// New syntax for nets with intents
net critical_path: VCC -> Res(10k).1 -> LED(red).A 
    for delay(3ms);

// Existing syntax remains for nets without intents
VCC -> Res(4.7kΩ).1 -> LED(red).A;  // Anonymous net
VCC @FILTERED-> r1: Res(4.7kΩ).1;   // Named net with @
```

**Advantages:**
- Minimal code changes (parser already implements this)
- Clear distinction between nets with and without intents
- Consistent with all existing intent documentation
- Provides explicit syntax for named nets

**Implementation:**
1. Update BHDL_Complete_Specification.md to include `net` keyword syntax
2. Document that `net` is used when attaching intents to flows
3. Keep existing @ syntax for inline net naming without intents

### Option 2: Update Parser to Support Intent on Flows

Modify parser to accept intent clauses on flow statements and connections:

```bhdl
// Intent on flow statement
critical_path: VCC -> Res(10k).1 -> LED(red).A for delay(3ms);

// Intent on direct connection
VCC -> Res(10k).1 -> LED(red).A for delay(3ms);
```

**Disadvantages:**
- Requires significant parser changes
- Conflicts with existing implementation and documentation
- May create ambiguity in grammar

### Option 3: Hybrid Approach

Support both syntaxes, but this adds complexity without clear benefit.

## Recommendation

**Go with Option 1**: Update the BHDL specification to include the `net` keyword for explicit net declarations with intents. This aligns with the existing implementation and all documentation, requiring minimal changes while providing a clear, unambiguous syntax for intent attachment.

## Next Steps

1. Update BHDL_Complete_Specification.md section 16.1 to include:
   ```
   net_declaration = "net" identifier ":" flow_expression ["for" intent_clause] ";"
   ```

2. Document that:
   - `net` keyword is used for explicit net declarations, especially when attaching intents
   - @ syntax remains for inline net naming within flow expressions
   - Direct connections without names continue to work as before

3. Update intent validation tests to use the correct syntax

This resolution maintains backward compatibility while providing the functionality needed for the intent system.