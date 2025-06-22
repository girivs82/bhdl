# Flow Reference Use Cases

## Potential Use Cases for Flow References

### 1. Abstract Flow Specifications
```bhdl
// Define abstract flows
power_flow: USB_5V |> protection |> regulation |> distribution;
signal_flow: INPUT |> amplify(10x) |> filter(1kHz) |> OUTPUT;

// Later implementation might reference these?
implement power_flow.protection {
    @USB_5V -> fuse: Fuse(2A).1 -> @protected;
}
```

### 2. Flow-Based Constraints
```bhdl
// Define a critical timing path
timing_critical: @CLK -> ff1.D -> ff1.Q -> logic -> ff2.D;

// Apply constraints to the flow
constrain timing_critical {
    max_delay = 5ns;
    route_priority = high;
}
```

### 3. Test Points on Flows
```bhdl
// Define a signal path
audio_path: @INPUT -> preamp -> eq -> amp -> @OUTPUT;

// Add test points to the flow
test_points on audio_path {
    after preamp: TP1;
    after eq: TP2;
}
```

### 4. Flow-Based Analysis
```bhdl
// Define power path
main_power: @VIN -> protection -> regulation -> @VCC;

// Analyze the flow
analyze main_power {
    calculate power_loss;
    verify thermal_limits;
}
```

### 5. Documentation/Visualization
```bhdl
// Key flows for documentation
critical_path: @SENSOR -> adc -> processor -> dac -> @ACTUATOR;

// Generate flow diagram
document critical_path {
    highlight = red;
    show_timing = true;
}
```

## Analysis: Are These Really Needed?

Looking at these use cases:

1. **Abstract flows** (|> operator) seem different from connection flows (->)
2. **Constraints** could be applied to nets or components instead
3. **Test points** are better as explicit components in the flow
4. **Analysis** typically happens on the whole netlist
5. **Documentation** could use comments or attributes

## Conclusion: Two Different Concepts

There appear to be two different concepts being conflated:

### 1. Abstract Flow Specifications (Keep)
```bhdl
// These use |> and are more like functional pipelines
power_distribution: USB_5V |> protection |> regulation |> loads;
```
These might be useful for high-level design specification.

### 2. Connection Labels (Remove)
```bhdl
// These label actual connections - not really needed
protection: @VIN -> fuse.1 -> @protected;
```
These add confusion without clear value.

## Recommendation

1. **Keep abstract flow specifications** with |> operator as they serve a different purpose
2. **Remove connection labels** (the : syntax on actual connections)
3. **Use different syntax** for abstract flows to avoid confusion:

```bhdl
// Abstract flow (high-level spec)
flow power_distribution = USB_5V |> protection |> regulation;

// Concrete connections (no labels needed)
@VIN -> fuse: Fuse(2A).1 -> @protected for overvoltage_protection;
```

This separates high-level design intent from low-level connections.