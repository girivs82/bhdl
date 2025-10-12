# BHDL CLI - Documentation Generation Command

## Overview

The `doc` command generates comprehensive Markdown documentation for power domains in BHDL circuits.

## Usage

```bash
bhdl <circuit.bhdl> doc [OPTIONS]
```

## Options

| Option | Description | Default |
|--------|-------------|---------|
| `-o, --output <FILE>` | Output file path | `power_domains.md` |
| `--bom-only` | Generate only Bill of Materials | Full documentation |
| `--budget-only` | Generate only power budget analysis | Full documentation |
| `--no-tree` | Disable power tree visualization | Enabled |
| `--no-patterns` | Disable pattern detection in summaries | Enabled |

## Examples

### Generate Full Documentation

```bash
bhdl circuit.bhdl doc
```

Generates complete power domain documentation including:
- Voltage domain summary table
- Power tree hierarchy
- Bill of Materials (BOM)
- Power budget analysis
- Connection summaries

### Custom Output Path

```bash
bhdl circuit.bhdl doc --output docs/power_analysis.md
```

### Generate Only BOM

```bash
bhdl circuit.bhdl doc --bom-only
```

Useful for procurement and manufacturing teams.

### Generate Only Budget Analysis

```bash
bhdl circuit.bhdl doc --budget-only
```

Useful for power supply sizing and thermal analysis.

### Generate Without Tree Visualization

```bash
bhdl circuit.bhdl doc --no-tree
```

Omits ASCII tree, useful for automated processing.

## Output Format

The generated documentation is in Markdown format with the following sections:

### 1. Voltage Domain Summary

High-level statistics table:

```markdown
| Domain | Connections | Components | Decoupling | Total Capacitance |
|--------|-------------|------------|------------|-------------------|
| @VCC_3V3 | 9 | 9 | 6 caps | 10.5 µF |
| @VCC_5V | 5 | 5 | 2 caps | 220.1 µF |
```

### 2. Power Tree

ASCII hierarchy visualization:

```
Power Distribution
  ├─ VCC_3V3 (3.3V) → 9 components
  ├─ VCC_5V (5V) → 5 components
```

### 3. Power Budget Analysis

Current consumption and margins:

```markdown
| Domain | Total Current | Component Count | Status |
|--------|---------------|-----------------|--------|
| @VCC_3V3 | 450.0 mA | 9 | ✓ Good |
| @VCC_5V | 1200.0 mA | 5 | ⚠ Adequate |
```

### 4. Bill of Materials

Decoupling capacitor BOM:

```markdown
| Ref Des | Value | Quantity | Placement |
|---------|-------|----------|-----------|
| C1-C6 | 100nF | 6 | Distributed |
| C7 | 10µF | 1 | Near mcu |
```

### 5. Connection Summary

Detailed connection listings per domain with pattern detection.

## Requirements

Your BHDL circuit must use `power_domain` syntax for documentation generation:

```bhdl
power_domain @VCC_3V3 = 3.3V @ 1A {
    sources {
        // Power source configuration
    }

    distribution {
        // Component connections
        sensor[*].VCC;
        mcu.VDDA;
    }

    decoupling {
        // Decoupling capacitors
        near mcu: 100nF @ 1, 10µF @ 1;
        distributed: 100nF @ 4;
    }
}
```

## Integration

### CI/CD Pipeline

```yaml
# .github/workflows/docs.yml
- name: Generate Power Documentation
  run: |
    cargo build --release -p bhdl-cli
    ./target/release/bhdl-cli circuit.bhdl doc --output docs/power_domains.md
    git add docs/power_domains.md
```

### Build Script

```bash
#!/bin/bash
# generate_docs.sh

BHDL_CLI="./target/release/bhdl-cli"
CIRCUITS_DIR="circuits"
DOCS_DIR="docs/generated"

mkdir -p "$DOCS_DIR"

for circuit in "$CIRCUITS_DIR"/*.bhdl; do
    basename=$(basename "$circuit" .bhdl)
    echo "Generating documentation for $basename..."
    $BHDL_CLI "$circuit" doc --output "$DOCS_DIR/${basename}_power.md"
done

echo "✓ Documentation generation complete"
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Parse errors in BHDL file |
| 1 | No power domains found |
| 1 | Documentation generation failed |

## Troubleshooting

### No Power Domains Found

**Error**: `Warning: No power domains found in circuit`

**Solution**: Ensure your circuit uses `power_domain` blocks:

```bhdl
power_domain @VCC = 5V @ 1A {
    distribution { /* ... */ }
    decoupling { /* ... */ }
}
```

### Parse Errors

**Error**: Various parse errors in BHDL file

**Solution**: Check that your BHDL syntax matches v2.0 specification. Common issues:
- Use proper unit syntax: `100nF` not `100n`
- Ensure power domains use `@` prefix: `@VCC` not `VCC`
- Check for missing semicolons or braces

**Note**: Both named pins (`component.VCC`) and numeric pins (`component.1`) are fully supported.

### Empty Documentation

If documentation is generated but empty:
1. Verify power domain distribution blocks have connections
2. Check that component instances are defined
3. Ensure proper syntax in decoupling blocks

## Related Documentation

- [Power Domain Specification](../spec/BHDL_Complete_Specification.md#power-domains)
- [Documentation Generation API](../examples/documentation_usage.md)
- [Power Domain Testing](../testing/POWER_DOMAIN_TEST_SUMMARY.md)

## Implementation Details

The doc command:
1. Parses the BHDL file
2. Runs semantic analysis including power domain expansion
3. Generates documentation from the expansion results
4. Writes formatted Markdown to the output file

**Performance**: Documentation generation typically takes <100ms for circuits with <1000 components.

**Files**: See `bhdl-cli/src/main.rs:684-771` for implementation details.
