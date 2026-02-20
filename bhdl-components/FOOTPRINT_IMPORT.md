# KiCad Footprint Import Tool

## Overview

The `import_kicad_footprints` tool imports KiCad footprint files (`.kicad_mod`) into the BHDL component database, associating them with existing components.

## Prerequisites

- Component must already exist in the database (import the symbol first using `import_kicad_symbols`)
- KiCad footprint file(s) (`.kicad_mod` format)

## Usage

### Import a Single Footprint

```bash
cargo run -p bhdl-components --bin import_kicad_footprints -- \
    /path/to/footprint.kicad_mod \
    --component "R" \
    --database components.db
```

### Import All Footprints from a Directory

```bash
cargo run -p bhdl-components --bin import_kicad_footprints -- \
    /path/to/footprints_directory/ \
    --component "R" \
    --database components.db
```

### Dry Run (Parse and Validate Only)

```bash
cargo run -p bhdl-components --bin import_kicad_footprints -- \
    /path/to/footprint.kicad_mod \
    --component "R" \
    --dry-run
```

## Command-Line Options

| Option | Short | Required | Description |
|--------|-------|----------|-------------|
| `footprint` | - | Yes | Path to `.kicad_mod` file or directory containing footprints |
| `--component` | `-c` | Yes | Component name to associate footprint with |
| `--database` | `-d` | No | Path to database (default: `components.db`) |
| `--dry-run` | - | No | Parse and process but don't write to database |

## Example: Importing 0805 Resistor Footprint

```bash
# First, import the resistor symbol (if not already done)
cargo run -p bhdl-components --bin import_kicad_symbols -- \
    kicad_symbol_cache/Device.kicad_sym \
    -d components.db

# Then import the footprint
cargo run -p bhdl-components --bin import_kicad_footprints -- \
    kicad_footprint_cache/R_0805_2012Metric.kicad_mod \
    -c "R" \
    -d components.db
```

## Workflow

The tool performs the following steps:

1. **Parse** - Reads and parses the KiCad `.kicad_mod` file
2. **Convert** - Converts KiCad format to BHDL ComponentFootprint structure
3. **Extract** - Extracts pad data (positions, sizes, shapes, types)
4. **Calculate** - Computes body dimensions, pitch, and bounding box
5. **Generate** - Creates SVG representation of the footprint
6. **Store** - Inserts footprint and pad data into database

## Data Extracted

From each KiCad footprint, the tool extracts:

- **Metadata**: Footprint name, description, tags
- **Dimensions**: Body width/height, pad pitch
- **Pads**: Number, position, size, shape (circle/rect/oval), type (SMD/through-hole)
- **Graphics**: Silkscreen and fabrication layer drawings
- **SVG**: Visual representation for visualization

## Footprint Data Storage

Footprints are stored in two database tables:

- `component_footprints`: Metadata and SVG for each footprint
- `footprint_pads`: Individual pad data (linked by foreign key)

## Error Handling

The tool will fail gracefully if:

- Component name is not found in database → Error: "Component 'X' not found in database. Import the component symbol first."
- Footprint file is malformed → Error: "Failed to parse KiCad footprint"
- File doesn't exist → Error: "Failed to read footprint file"

## Logging

Enable detailed logging with `RUST_LOG`:

```bash
RUST_LOG=info cargo run -p bhdl-components --bin import_kicad_footprints -- ...
```

Log levels:
- `error`: Fatal errors only
- `info`: Progress messages and summary
- `debug`: Detailed parsing and database operations

## Output Example

```
🔧 KiCad Footprint Import Tool
Footprint: /path/to/R_0805_2012Metric.kicad_mod
Component: R
Database: components.db
📖 Found 1 footprint file(s) to process
✅ Imported footprint: R_0805_2012Metric.kicad_mod
🎉 Import complete!
   Successfully imported: 1
   Errors: 0
   Total footprints processed: 1
```

## Integration with BHDL Pipeline

Once imported, footprints become available to:

- **bhdl-synthesizer**: Component selection and netlist generation
- **bhdl-visualizer**: PCB layout visualization
- **bhdl-analyzer**: Footprint-aware design rule checking

## Testing

Test the parser independently:

```bash
cargo test -p bhdl-components test_parse_smd_resistor_footprint -- --nocapture
```

## See Also

- `import_kicad_symbols` - Import KiCad symbol libraries
- `bhdl-components/README.md` - Component database documentation
- `docs/examples/` - Example BHDL circuits using imported components
