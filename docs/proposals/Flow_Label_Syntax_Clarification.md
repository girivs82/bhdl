# Flow Label Syntax Clarification

## The Problem

Current syntax uses `:` for both component handles AND flow labels:

```bhdl
// Component handle
@VCC -> r1: Res(10k).1;

// Flow label
protection: @VIN -> fuse.1 -> @protected;

// Ambiguous! Is protection a component or label?
```

## Proposed Solutions

### Option 1: Different Syntax for Flow Labels

Use a different delimiter for flow labels:

```bhdl
// Double colon for flow labels
protection:: @VIN -> fuse.1 -> @protected for overvoltage_protection;

// Or arrow syntax
protection => @VIN -> fuse.1 -> @protected for overvoltage_protection;

// Or parentheses
(protection) @VIN -> fuse.1 -> @protected for overvoltage_protection;
```

### Option 2: No Flow Labels (Recommended)

Flow labels add little value. Instead, use comments or intent names:

```bhdl
// Instead of label, use comment
// Protection circuit
@VIN -> fuse.1 -> @protected for overvoltage_protection;

// Or let the intent name document the purpose
@VIN -> fuse.1 -> @protected for input_protection(overvoltage: 15V);

// Or create a named net that documents purpose
@VIN -> fuse.1 -> @protected_input for overvoltage_protection;
```

### Option 3: Rethink Flow Labels as Named Flows

If we need named flows, make them more distinct:

```bhdl
// Define a named flow (different from connection)
flow protection_circuit {
    @VIN -> fuse.1 -> @protected for overvoltage_protection;
    @protected -> tvs: TVSDiode(15V).1;
    tvs.2 -> @GND;
}

// Or inline with clear syntax
flow protection = @VIN -> fuse.1 -> @protected for overvoltage_protection;
```

## Recommendation: Eliminate Flow Labels

Flow labels are rarely needed and add confusion. Instead:

1. **Use comments** for documentation
2. **Use intent names** to express purpose
3. **Use descriptive net names**

## Examples Without Flow Labels

### Before (Confusing)
```bhdl
protection: @VIN -> fuse.1 -> @protected for overvoltage_protection;
filtering: @protected -> @filtered for noise_immunity;
regulation: @filtered -> reg.IN for voltage_stability;
```

### After (Clear)
```bhdl
// Input protection
@VIN -> fuse.1 -> @protected_vin for overvoltage_protection(15V);

// Noise filtering  
@protected_vin -> @filtered_supply for noise_immunity(20dB);

// Voltage regulation
@filtered_supply -> reg: LM7805().IN for voltage_stability(±5%);
```

The intent clause already documents the purpose - we don't need labels!

## If Labels Are Essential

If flow labels are deemed essential, use a distinct syntax:

```bhdl
// Option A: Tagged flows with #
#protection: @VIN -> fuse.1 -> @protected;

// Option B: Flow assignments with =
protection_flow = @VIN -> fuse.1 -> @protected;

// Option C: Explicit flow keyword
flow protection: @VIN -> fuse.1 -> @protected;
```

## Summary

The `:` operator should be reserved exclusively for component handles to avoid confusion. Flow labels, if needed at all, should use a different syntax. However, the recommendation is to eliminate flow labels entirely since intent clauses and comments provide better documentation.