# BHDL Examples

## Current Status

The examples are being updated to match the BHDL v2.0 specification (BHDL_Complete_Specification.md).

Old examples using the previous syntax have been moved to `old_syntax/` directory.

## New Syntax Examples

Examples using the current flow-based syntax will be added here as the parser is updated to support the v2.0 specification.

### Basic Component Instantiation
```bhdl
// Universal pattern: source -> component(parameters) -> destination
VCC -> Res(4.7kΩ).1 -> LED(red).A;
LED.K -> GND;

USB_5V -> regulator: LinearReg(3.3V, 1A).IN;
regulator.OUT -> Cap(10µF).+ -> VOUT;
```

### Flow Specification
```bhdl
// Universal flow operator |> for any domain
power_flow: USB_5V |> protection |> regulation |> distribution;
signal_flow: INPUT |> amplify(10x) |> filter(1kHz) |> OUTPUT;
```

See `../spec/BHDL_Complete_Specification.md` for the complete v2.0 syntax reference.