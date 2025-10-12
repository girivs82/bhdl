# Power Domain Documentation Generator - Design Document

**Status**: 🚧 In Development
**Date**: October 12, 2025
**Priority**: Medium (Low Effort, High Value)

## Overview

The Power Domain Documentation Generator automatically creates comprehensive documentation from BHDL power domain specifications. This tool helps designers understand, review, and document their power architecture without manual effort.

## Motivation

After designing power domains in BHDL, users need to:
- **Communicate** the design to team members
- **Document** power architecture for reviews
- **Generate** BOMs for procurement
- **Validate** power budgets meet requirements
- **Create** reports for design documentation

Manual documentation is:
- Time-consuming
- Error-prone
- Becomes outdated quickly
- Inconsistent across teams

**Solution**: Auto-generate documentation from BHDL specifications.

## Features

### 1. Power Tree Diagram

Generate visual representation of power domain hierarchy.

**Example Output**:
```
Power Tree
==========

Input: 12V @ 5A (60W max)
├─ Buck → @VCC_5V (5V @ 2A = 10W)
│  ├─ MCU (500mA)
│  ├─ Peripherals (800mA)
│  └─ LEDs[0..7] (700mA total)
│
└─ Buck → @VCC_3V3 (3.3V @ 3A = 9.9W)
   ├─ FPGA Core (2A)
   ├─ DDR3 Memory (800mA)
   └─ I/O Buffers (200mA)

Total Power Budget: 19.9W / 60W (33% utilized)
```

**Features**:
- Tree structure showing voltage conversion
- Current draw per domain
- Power consumption calculation
- Utilization percentage

### 2. Decoupling Capacitor BOM

Generate complete parts list for decoupling capacitors.

**Example Output**:
```markdown
## Decoupling Capacitor BOM

| Reference | Value | Voltage | Placement | Quantity | Notes |
|-----------|-------|---------|-----------|----------|-------|
| C1-C2     | 100µF | 6.3V    | Near buck_5v | 2     | Bulk storage |
| C3-C6     | 10µF  | 6.3V    | Near MCU     | 4     | Mid-frequency |
| C7-C14    | 100nF | 10V     | Distributed  | 8     | High-frequency |
| C15-C16   | 100µF | 6.3V    | Near buck_3v3| 2     | Bulk storage |
| C17-C20   | 10µF  | 6.3V    | Near FPGA    | 4     | Mid-frequency |
| C21-C30   | 100nF | 10V     | Distributed  | 10    | High-frequency |

**Total**: 30 capacitors
**Estimated Cost**: $3.50 (assuming $0.05/100nF, $0.15/10µF, $0.25/100µF)
```

**Features**:
- Reference designators
- Consolidated by value
- Voltage ratings based on domain voltage
- Placement constraints
- Cost estimation

### 3. Power Budget Table

Detailed current/power breakdown per domain.

**Example Output**:
```markdown
## Power Budget Analysis

| Domain     | Voltage | Max Current | Components | Actual Draw | Margin | Power  |
|------------|---------|-------------|------------|-------------|--------|--------|
| @VCC_5V    | 5.0V    | 2.0A        | 11         | 1.85A       | 7.5%   | 9.25W  |
| @VCC_3V3   | 3.3V    | 3.0A        | 15         | 2.75A       | 8.3%   | 9.08W  |
| **Total**  |         | 5.0A        | 26         | 4.60A       | 8.0%   | 18.33W |

### Current Breakdown by Component Type

**@VCC_5V:**
- MCU: 500mA (27%)
- UART, USB, I2C: 800mA (43%)
- LED[0..7]: 700mA (38%)

**@VCC_3V3:**
- FPGA: 2000mA (73%)
- DDR3: 800mA (29%)
- I/O: 200mA (7%)

**Design Margins:**
- ✅ All domains have >5% margin (recommended minimum)
- ⚠️  @VCC_5V has only 7.5% margin - consider 2.5A supply
```

**Features**:
- Voltage and current per domain
- Component count
- Actual vs. specified current
- Margin calculation
- Power consumption
- Component type breakdown

### 4. Connection Summary

Detailed listing of all power connections.

**Example Output**:
```markdown
## Power Domain Connections

### @VCC_5V (5V @ 2A)

**Regulator Output:**
- buck_5v.VOUT

**Component Connections:** (10 total)
- mcu.VCC
- uart.VCC
- usb.VCC
- i2c.VCC
- led[0].A, led[1].A, led[2].A, led[3].A
- led[4].A, led[5].A, led[6].A, led[7].A

**Decoupling:** (14 capacitors)
- Near buck_5v: 2× 100µF
- Near mcu: 4× 10µF
- Distributed: 8× 100nF

### @VCC_3V3 (3.3V @ 3A)

**Regulator Output:**
- buck_3v3.VOUT

**Component Connections:** (15 total)
- fpga.VCCINT[0..11] (12 connections via range)
- ddr3.VCC
- io_buffer[*].VCC (3 connections via wildcard)

**Decoupling:** (16 capacitors)
- Near buck_3v3: 2× 100µF
- Near fpga: 4× 10µF
- Distributed: 10× 100nF
```

**Features**:
- Domain-by-domain breakdown
- Source connections (regulators)
- All component connections
- Pattern expansion shown
- Decoupling summary

### 5. Voltage Domain Summary

High-level overview of all domains.

**Example Output**:
```markdown
## Voltage Domain Summary

| Domain    | Voltage | Current | Components | Decoupling | Pattern Usage |
|-----------|---------|---------|------------|------------|---------------|
| @VCC_5V   | 5V      | 2A      | 11         | 14 caps    | Wildcard      |
| @VCC_3V3  | 3.3V    | 3A      | 15         | 16 caps    | Range, Wildcard|
| @AVCC_P   | 5V      | 1A      | 4          | 8 caps     | Even keyword  |
| @AVCC_N   | 5V      | 1A      | 4          | 8 caps     | Odd keyword   |

**Total Domains**: 4
**Total Connections**: 34
**Total Decoupling**: 46 capacitors
**Pattern Types Used**: Wildcard, Range, Even/Odd keywords
```

## Architecture

### Module Structure

```
bhdl-analyzer/src/documentation/
├── mod.rs                    // Public API
├── power_tree.rs             // Power tree diagram generator
├── bom_generator.rs          // BOM generation
├── budget_analyzer.rs        // Power budget analysis
├── connection_summary.rs     // Connection listing
└── formatters/
    ├── markdown.rs           // Markdown output
    ├── ascii_table.rs        // ASCII table formatting
    └── html.rs               // HTML output (future)
```

### Data Flow

```
PowerDomainExpansion (from analyzer)
           ↓
  DocumentationContext
           ↓
    ┌──────┴──────┬──────────┬─────────────┐
    ↓             ↓          ↓             ↓
PowerTree    BOMGenerator  Budget    ConnectionSummary
    ↓             ↓          ↓             ↓
    └──────┬──────┴──────────┴─────────────┘
           ↓
     Formatter (Markdown/ASCII/HTML)
           ↓
      Output String
```

### Core Types

```rust
/// Documentation generator context
pub struct DocumentationContext {
    /// Power domain expansion data
    pub expansion: PowerDomainExpansion,
    /// Component metadata (current draw, etc.)
    pub component_metadata: HashMap<String, ComponentMetadata>,
    /// Formatting options
    pub options: DocumentationOptions,
}

/// Component metadata for documentation
pub struct ComponentMetadata {
    pub typical_current: Option<f64>,
    pub max_current: Option<f64>,
    pub description: Option<String>,
}

/// Documentation output options
pub struct DocumentationOptions {
    pub format: OutputFormat,
    pub include_power_tree: bool,
    pub include_bom: bool,
    pub include_budget: bool,
    pub include_connections: bool,
    pub include_summary: bool,
    pub show_patterns: bool,
}

pub enum OutputFormat {
    Markdown,
    AsciiTable,
    Html,
}
```

## Implementation Plan

### Phase 1: Core Infrastructure (2-3 hours)

1. **Create module structure**
   - `bhdl-analyzer/src/documentation/mod.rs`
   - `bhdl-analyzer/src/documentation/context.rs`
   - `bhdl-analyzer/src/documentation/formatters/mod.rs`

2. **Define core types**
   - `DocumentationContext`
   - `DocumentationOptions`
   - `ComponentMetadata`

3. **Create public API**
   ```rust
   pub fn generate_documentation(
       expansion: &PowerDomainExpansion,
       options: DocumentationOptions
   ) -> Result<String, DocumentationError>
   ```

### Phase 2: Basic Generators (3-4 hours)

1. **Connection Summary** (simplest)
   - List all connections per domain
   - Show pattern expansion
   - Count components

2. **Voltage Domain Summary** (simple table)
   - Domain overview table
   - Statistics

3. **Markdown Formatter**
   - Tables
   - Headers
   - Lists

### Phase 3: Advanced Generators (4-5 hours)

1. **Power Tree Generator**
   - Build tree structure from regulators
   - Calculate power at each node
   - ASCII tree rendering

2. **BOM Generator**
   - Group capacitors by value
   - Generate reference designators
   - Calculate quantities
   - Estimate costs

3. **Budget Analyzer**
   - Calculate current per domain
   - Compute margins
   - Breakdown by component type

### Phase 4: Testing & Examples (2-3 hours)

1. **Unit Tests**
   - Each generator module
   - Formatters
   - Edge cases

2. **Integration Tests**
   - Use existing example circuits
   - Validate output format
   - Check calculations

3. **CLI Tool**
   - `bhdl-analyzer --document power-domains <file.bhdl>`
   - Output to stdout or file
   - Format selection

## Usage Examples

### Command Line

```bash
# Generate full documentation
bhdl-analyzer --document power-domains design.bhdl > power-report.md

# Generate only BOM
bhdl-analyzer --document power-domains --bom-only design.bhdl

# Generate budget analysis
bhdl-analyzer --document power-domains --budget-only design.bhdl

# HTML output
bhdl-analyzer --document power-domains --format html design.bhdl > report.html
```

### Programmatic API

```rust
use bhdl_analyzer::documentation::{generate_documentation, DocumentationOptions, OutputFormat};

// Analyze circuit
let analyzer = Analyzer::new(source, syntax);
let result = analyzer.analyze();

// Generate documentation
let options = DocumentationOptions {
    format: OutputFormat::Markdown,
    include_power_tree: true,
    include_bom: true,
    include_budget: true,
    include_connections: true,
    include_summary: true,
    show_patterns: true,
};

let documentation = generate_documentation(&result.power_domain_expansion, options)?;
println!("{}", documentation);
```

## Output Examples

### Full Report Example

See `docs/examples/power-documentation-example.md` for a complete example of generated documentation.

### Customization

Users can customize output by:
- Selecting which sections to include
- Choosing output format
- Adding component metadata
- Configuring cost estimates

## Benefits

1. **Time Savings**: Auto-generate documentation in seconds
2. **Consistency**: Same format across all designs
3. **Accuracy**: Always matches actual design
4. **Up-to-date**: Regenerate when design changes
5. **Professional**: Clean, well-formatted output

## Future Enhancements

1. **HTML Output** with interactive diagrams
2. **PDF Generation** for formal documentation
3. **Excel Export** for BOMs
4. **Custom Templates** for company standards
5. **Cost Database Integration** for accurate pricing
6. **Power Integrity Metrics** (IR drop, ripple)
7. **Comparison Reports** (before/after changes)

## Testing Strategy

1. **Unit Tests**: Each generator module independently
2. **Integration Tests**: Full documentation generation on example circuits
3. **Golden Files**: Compare output against expected results
4. **Visual Inspection**: Manual review of formatting

## Success Criteria

- ✅ Generates all 5 documentation types
- ✅ Markdown output is well-formatted
- ✅ Calculations are accurate (current, power, margins)
- ✅ BOM includes all capacitors with correct values
- ✅ Power tree shows correct hierarchy
- ✅ Works with all pattern types (wildcard, range, even/odd, etc.)
- ✅ Performance: <100ms for typical designs
- ✅ CLI tool is user-friendly

## Implementation Timeline

- **Phase 1**: Core infrastructure (Day 1, 2-3 hours)
- **Phase 2**: Basic generators (Day 1, 3-4 hours)
- **Phase 3**: Advanced generators (Day 2, 4-5 hours)
- **Phase 4**: Testing & examples (Day 2, 2-3 hours)

**Total**: 1.5-2 days

## References

- Power Domain Expansion: `bhdl-analyzer/src/passes/power_domain_expansion.rs`
- Example Circuits: `docs/examples/`
- BHDL Specification: `docs/spec/BHDL_Complete_Specification.md`
