# BHDL Unit Syntax Guide

This guide documents the supported unit syntax in BHDL, including both Unicode and ASCII alternatives for easier typing.

## Overview

BHDL supports electrical units in both Unicode and ASCII formats. The parser automatically recognizes both forms, allowing users to choose based on their preferences and keyboard capabilities.

## Supported Units

### Resistance Units

| Unicode | ASCII | Example | Description |
|---------|-------|---------|-------------|
| `Ω` | `Ohm` | `4.7Ω` or `4.7Ohm` | Ohms |
| `kΩ` | `kOhm` | `10kΩ` or `10kOhm` | Kiloohms |
| `MΩ` | `MOhm` | `1MΩ` or `1MOhm` | Megaohms |
| `mΩ` | `mOhm` | `100mΩ` or `100mOhm` | Milliohms |

### Voltage Units

| Unicode | ASCII | Example | Description |
|---------|-------|---------|-------------|
| `V` | `V` | `5V` | Volts |
| `mV` | `mV` | `100mV` | Millivolts |
| `µV` | `uV` | `50µV` or `50uV` | Microvolts |
| `nV` | `nV` | `10nV` | Nanovolts |
| - | `Vdc` | `12Vdc` | Volts DC |
| - | `Vac` | `120Vac` | Volts AC |
| - | `Vrms` | `230Vrms` | Volts RMS |
| - | `Vpp` | `10Vpp` | Volts peak-to-peak |

### Current Units

| Unicode | ASCII | Example | Description |
|---------|-------|---------|-------------|
| `A` | `A` | `2A` | Amperes |
| `mA` | `mA` | `20mA` | Milliamperes |
| `µA` | `uA` | `100µA` or `100uA` | Microamperes |
| `nA` | `nA` | `50nA` | Nanoamperes |

### Capacitance Units

| Unicode | ASCII | Example | Description |
|---------|-------|---------|-------------|
| `F` | `F` | `1F` | Farads |
| `µF` | `uF` | `10µF` or `10uF` | Microfarads |
| `nF` | `nF` | `100nF` | Nanofarads |
| `pF` | `pF` | `22pF` | Picofarads |

### Inductance Units

| Unicode | ASCII | Example | Description |
|---------|-------|---------|-------------|
| `H` | `H` | `1H` | Henries |
| `mH` | `mH` | `10mH` | Millihenries |
| `µH` | `uH` | `100µH` or `100uH` | Microhenries |
| `nH` | `nH` | `50nH` | Nanohenries |

### Frequency Units

| Unicode | ASCII | Example | Description |
|---------|-------|---------|-------------|
| `Hz` | `Hz` | `50Hz` | Hertz |
| `kHz` | `kHz` | `1kHz` | Kilohertz |
| `MHz` | `MHz` | `100MHz` | Megahertz |
| `GHz` | `GHz` | `2.4GHz` | Gigahertz |

### Time Units

| Unicode | ASCII | Example | Description |
|---------|-------|---------|-------------|
| `s` | `s` | `1s` | Seconds |
| `ms` | `ms` | `10ms` | Milliseconds |
| `µs` | `us` | `100µs` or `100us` | Microseconds |
| `ns` | `ns` | `50ns` | Nanoseconds |
| `ps` | `ps` | `10ps` | Picoseconds |

### Temperature Units

| Unicode | ASCII | Example | Description |
|---------|-------|---------|-------------|
| `°C` | `degC` | `25°C` or `25degC` | Degrees Celsius |
| `K` | `K` | `300K` | Kelvin |

### Power Units

| Unicode | ASCII | Example | Description |
|---------|-------|---------|-------------|
| `W` | `W` | `5W` | Watts |
| `mW` | `mW` | `100mW` | Milliwatts |
| `µW` | `uW` | `50µW` or `50uW` | Microwatts |
| `nW` | `nW` | `10nW` | Nanowatts |

### Length Units (Physical Dimensions)

| Unicode | ASCII | Example | Description |
|---------|-------|---------|-------------|
| `mm` | `mm` | `10mm` | Millimeters |
| `µm` | `um` | `100µm` or `100um` | Micrometers |
| `nm` | `nm` | `90nm` | Nanometers |
| `mil` | `mil` | `50mil` | Mils (thousandths of inch) |

### Other Units

| Unicode | ASCII | Example | Description |
|---------|-------|---------|-------------|
| `%` | `pct` | `5%` or `5pct` | Percentage |
| `dB` | `dB` | `3dB` | Decibels |
| `dBm` | `dBm` | `10dBm` | Decibel-milliwatts |

## Usage Examples

### Component Parameters
```bhdl
// Both forms are equivalent
const r1 = Res(4.7kΩ);      // Unicode
const r2 = Res(4.7kOhm);    // ASCII

const c1 = Cap(10µF, 16V);  // Unicode
const c2 = Cap(10uF, 16V);  // ASCII
```

### Attributes
```bhdl
attribute forward_voltage = 2.0V;
attribute max_current = 20mA;
attribute rise_time = 10ns;
attribute junction_temp = 85°C;      // Unicode
attribute storage_temp = 125degC;    // ASCII
```

### Power Declarations
```bhdl
power VCC = 5V @ 1A;
power VIN = 12V @ 500mA;
```

## Smart Unit Recognition

The parser uses context-aware tokenization to distinguish between units and identifiers:

```bhdl
pin A: signal in;    // 'A' is an identifier (pin name)
const current = 2A;  // 'A' is a unit (Amperes)

module F {           // 'F' is an identifier (module name)
    const cap = 1F;  // 'F' is a unit (Farads)
}
```

Units are only recognized when immediately following a number with no intervening space.

## Typing Unicode Characters

### Windows
- **Character Map**: Win+R → charmap
- **Alt codes**: Hold Alt + numeric code
  - Alt+234 → Ω
  - Alt+230 → µ
  - Alt+248 → °

### macOS
- **Option key combinations**:
  - Option+Z → Ω
  - Option+M → µ
  - Option+Shift+8 → °
- **Character Viewer**: Control+Command+Space

### Linux
- **Compose key** (if configured):
  - Compose+O+M → Ω
  - Compose+m+u → µ
  - Compose+0+0 → °
- **Unicode input**: Ctrl+Shift+U, then code

### IDE Solutions
Most modern IDEs support:
- **Snippets/Live Templates**
- **Custom keybindings**
- **Character palette plugins**

## Best Practices

1. **Consistency**: Choose either Unicode or ASCII and stick with it within a project
2. **Team Preferences**: Discuss with your team which format to standardize on
3. **Tool Support**: Use ASCII if your toolchain has limited Unicode support
4. **Documentation**: Always accept both forms in parsers/tools for maximum compatibility

## Future IDE Support

Planned features for BHDL language extensions:
- Auto-completion from ASCII to Unicode
- Format-on-save options
- Quick fixes to convert between formats
- Snippet support for common units