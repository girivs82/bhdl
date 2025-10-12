# Power Domain Documentation Generation - Usage Guide

## Overview

The BHDL analyzer provides automatic documentation generation for power domain specifications. This guide shows practical examples of how to use the documentation generation feature.

## Basic Usage

### 1. Programmatic API

```rust
use bhdl_analyzer::documentation::{
    generate_documentation,
    DocumentationOptions,
    OutputFormat,
};
use bhdl_parser;
use bhdl_ast::AstNode;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse BHDL file
    let source = fs::read_to_string("my_circuit.bhdl")?;
    let parse_result = bhdl_parser::parse(&source);

    // Analyze
    let source_file = bhdl_ast::SourceFile::cast(parse_result.syntax().clone())
        .expect("Failed to cast to SourceFile");
    let analysis_result = bhdl_analyzer::analyze(&source_file);

    // Generate documentation
    let options = DocumentationOptions::default();
    let documentation = generate_documentation(
        &analysis_result.power_domain_expansion,
        options
    )?;

    // Save to file
    fs::write("docs/power_domains.md", documentation)?;

    Ok(())
}
```

### 2. Custom Options

```rust
use bhdl_analyzer::documentation::{DocumentationOptions, OutputFormat};

// Customize which sections to include
let options = DocumentationOptions {
    format: OutputFormat::Markdown,
    include_power_tree: true,      // ASCII tree visualization
    include_bom: true,              // Bill of materials
    include_budget: true,           // Power budget analysis
    include_connections: true,      // Detailed connection listings
    include_summary: true,          // Voltage domain summary table
    show_patterns: true,            // Show pattern detection
};

let documentation = generate_documentation(&expansion, options)?;
```

### 3. Selective Documentation

Generate only specific sections:

```rust
// Only BOM and summary
let bom_only = DocumentationOptions {
    format: OutputFormat::Markdown,
    include_power_tree: false,
    include_bom: true,
    include_budget: false,
    include_connections: false,
    include_summary: true,
    show_patterns: false,
};

// Only budget analysis
let budget_only = DocumentationOptions {
    include_budget: true,
    ..Default::default()
};
```

## Output Sections

### 1. Voltage Domain Summary

Provides a high-level overview of all power domains:

```markdown
| Domain | Connections | Components | Decoupling | Total Capacitance |
|--------|-------------|------------|------------|-------------------|
| @VCC_3V3 | 9 | 9 | 6 caps | 10.5 µF |
| @VCC_5V | 5 | 5 | 2 caps | 220.1 µF |
| **Total** | **14** | **14** | **8 caps** | - |
```

**Use Case**: Quick reference card, design review presentations

### 2. Power Tree

ASCII tree showing power distribution hierarchy:

```
Power Distribution
  ├─ VCC_3V3 (3.3V) [450 mA] → 9 components
  ├─ VCC_5V (5V) [1200 mA] → 5 components
```

**Use Case**: Understanding power architecture, debugging distribution

### 3. Power Budget Analysis

Detailed current consumption and margin analysis:

```markdown
| Domain | Total Current | Component Count | Peak Current | Margin | Status |
|--------|---------------|-----------------|--------------|--------|--------|
| @VCC_3V3 | 450.0 mA | 9 | 675.0 mA | 32% | ✓ Good |
| @VCC_5V | 1200.0 mA | 5 | 1800.0 mA | 25% | ⚠ Adequate |
```

**Use Case**: Power supply sizing, thermal analysis, design validation

### 4. Bill of Materials

Grouped capacitor listing with reference designators:

```markdown
| Ref Des | Value | Quantity | Type | Voltage | Placement |
|---------|-------|----------|------|---------|-----------|
| C1-C6 | 100nF | 6 | Ceramic | 25V | Distributed |
| C7 | 10µF | 1 | Ceramic | 16V | Near-component |
```

**Use Case**: Procurement, PCB assembly, cost estimation

### 5. Power Domain Connections

Detailed connection listings per domain:

```markdown
### @VCC_3V3

**Connections** (9 total):

*Pattern Expansion:*
- sensor[*]: 8 connections

- VCC_3V3 → sensor_0.VCC
- VCC_3V3 → sensor_1.VCC
...

**Decoupling** (6 capacitors):
*Near-component placement:*
- 1× 100nF
- 1× 10µF
```

**Use Case**: PCB layout planning, connection verification

## Practical Examples

### Example 1: Design Review Documentation

Generate comprehensive documentation for design review:

```rust
// Generate full documentation
let options = DocumentationOptions::default();
let doc = generate_documentation(&expansion, options)?;
fs::write("design_review/power_domains.md", doc)?;

// Output includes:
// - Complete statistics for stakeholders
// - Power budget for thermal review
// - BOM for cost analysis
// - Connection details for layout review
```

### Example 2: Manufacturing Documentation

Generate BOM and assembly information:

```rust
let manufacturing_docs = DocumentationOptions {
    format: OutputFormat::Markdown,
    include_power_tree: false,
    include_bom: true,              // ✓ BOM for procurement
    include_budget: false,
    include_connections: true,      // ✓ Connections for assembly
    include_summary: true,          // ✓ Summary for quick reference
    show_patterns: false,           // Hide pattern details
};

let doc = generate_documentation(&expansion, manufacturing_docs)?;
fs::write("manufacturing/assembly_doc.md", doc)?;
```

### Example 3: Budget Analysis Only

Extract just the power budget for thermal analysis:

```rust
let budget_options = DocumentationOptions {
    include_power_tree: false,
    include_bom: false,
    include_budget: true,           // Only budget analysis
    include_connections: false,
    include_summary: true,          // Include summary for context
    ..Default::default()
};

let budget_doc = generate_documentation(&expansion, budget_options)?;
```

### Example 4: Automated CI/CD Integration

Generate documentation as part of CI pipeline:

```rust
use std::process;

fn generate_docs_for_ci() -> Result<(), Box<dyn std::error::Error>> {
    let source = fs::read_to_string("src/main.bhdl")?;
    let parse_result = bhdl_parser::parse(&source);

    if !parse_result.errors().is_empty() {
        eprintln!("Parse errors detected");
        process::exit(1);
    }

    let source_file = bhdl_ast::SourceFile::cast(parse_result.syntax().clone())
        .expect("Failed to cast");
    let analysis = bhdl_analyzer::analyze(&source_file);

    // Generate docs
    let options = DocumentationOptions::default();
    let doc = generate_documentation(&analysis.power_domain_expansion, options)?;

    // Save to docs directory
    fs::create_dir_all("docs/generated")?;
    fs::write("docs/generated/power_domains.md", doc)?;

    // Also check budget margins
    let budget_lines: Vec<_> = doc.lines()
        .filter(|l| l.contains("Margin"))
        .collect();

    println!("Generated documentation with {} power domains",
             analysis.power_domain_expansion.connections.len());

    Ok(())
}
```

## Integration Patterns

### Pattern 1: CLI Tool Integration

```rust
// Future: CLI command
// $ bhdl doc circuit.bhdl --output docs/

use clap::Parser;

#[derive(Parser)]
struct Args {
    #[arg(help = "BHDL source file")]
    file: String,

    #[arg(short, long, default_value = "docs/power_domains.md")]
    output: String,

    #[arg(long, help = "Generate only BOM")]
    bom_only: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Parse and analyze
    let source = fs::read_to_string(&args.file)?;
    let parse_result = bhdl_parser::parse(&source);
    let source_file = bhdl_ast::SourceFile::cast(parse_result.syntax().clone())?;
    let analysis = bhdl_analyzer::analyze(&source_file);

    // Configure options
    let options = if args.bom_only {
        DocumentationOptions {
            include_bom: true,
            include_summary: true,
            ..Default::default()
        }
    } else {
        DocumentationOptions::default()
    };

    // Generate and save
    let doc = generate_documentation(&analysis.power_domain_expansion, options)?;
    fs::write(&args.output, doc)?;

    println!("Documentation generated: {}", args.output);
    Ok(())
}
```

### Pattern 2: Build System Integration

```rust
// build.rs
fn main() {
    // Regenerate documentation on build
    let _ = generate_power_docs();
}

fn generate_power_docs() -> Result<(), Box<dyn std::error::Error>> {
    for entry in glob::glob("src/**/*.bhdl")? {
        let path = entry?;
        let source = fs::read_to_string(&path)?;

        // ... parse and generate docs ...

        let output_path = format!("docs/generated/{}.md",
                                  path.file_stem().unwrap().to_str().unwrap());
        fs::write(output_path, documentation)?;
    }
    Ok(())
}
```

### Pattern 3: LSP Integration (Future)

```rust
// Future: IDE hover documentation
fn provide_hover_documentation(position: Position) -> Option<String> {
    // When hovering over a power domain...
    let expansion = analyze_at_position(position);

    let options = DocumentationOptions {
        include_summary: true,
        include_budget: true,
        ..Default::default()
    };

    generate_documentation(&expansion, options).ok()
}
```

## Component Metadata

For accurate power budget analysis, provide component metadata:

```rust
use std::collections::HashMap;
use bhdl_analyzer::documentation::ComponentMetadata;

let mut metadata = HashMap::new();

// Add current specifications
metadata.insert("mcu".to_string(), ComponentMetadata {
    typical_current: Some(0.150),  // 150 mA
    max_current: Some(0.200),      // 200 mA peak
    description: Some("STM32H7 MCU".to_string()),
});

metadata.insert("sensor_0".to_string(), ComponentMetadata {
    typical_current: Some(0.005),  // 5 mA
    max_current: Some(0.010),      // 10 mA peak
    description: Some("BME280 Sensor".to_string()),
});

// Use in context
let context = DocumentationContext {
    expansion: power_domain_expansion,
    component_metadata: metadata,
    options: DocumentationOptions::default(),
};
```

## Output Formats

### Current: Markdown

Fully implemented and production-ready.

### Future: HTML

```rust
let options = DocumentationOptions {
    format: OutputFormat::Html,
    ..Default::default()
};

// Will generate HTML with:
// - Interactive tables
// - Collapsible sections
// - CSS styling
// - Links between sections
```

### Future: JSON/CSV

```rust
// JSON for programmatic access
let options = DocumentationOptions {
    format: OutputFormat::Json,
    ..Default::default()
};

// CSV for spreadsheet import
let options = DocumentationOptions {
    format: OutputFormat::Csv,
    ..Default::default()
};
```

## Best Practices

### 1. Always Include Summary

The summary provides essential context:

```rust
let options = DocumentationOptions {
    include_summary: true,  // ✓ Always include
    // ... other options
    ..Default::default()
};
```

### 2. Use Pattern Detection

Shows how wildcards expanded:

```rust
let options = DocumentationOptions {
    show_patterns: true,  // ✓ Helpful for debugging
    ..Default::default()
};
```

### 3. Regular Documentation Updates

Regenerate docs after design changes:

```bash
# In CI/CD pipeline
cargo run --bin generate_docs
git add docs/generated/
git commit -m "docs: Update power domain documentation"
```

### 4. Version Control Documentation

Keep generated docs in version control for:
- Design history tracking
- Review diffs
- Documentation availability

### 5. Link to Design Files

Add references in generated docs:

```markdown
<!-- Add to generated docs -->
Source: [circuit.bhdl](../src/circuit.bhdl)
Last Updated: 2025-10-12
```

## Troubleshooting

### Missing Statistics

If power budget shows 0 mA:

```rust
// Provide component metadata
let mut metadata = HashMap::new();
metadata.insert("component_name".to_string(), ComponentMetadata {
    typical_current: Some(0.100),  // Add current specs
    max_current: Some(0.150),
    description: None,
});
```

### Incomplete BOM

If capacitors are missing:

```rust
// Check that decoupling block is parsed correctly
// Verify @ syntax: distributed: 100nF @ 4
```

### Pattern Detection Not Working

If patterns don't show:

```rust
let options = DocumentationOptions {
    show_patterns: true,  // Enable pattern detection
    ..Default::default()
};
```

## Related Documentation

- [Power Domain Specification](../spec/BHDL_Complete_Specification.md#power-domains)
- [Power Domain Scalability](../implementation/Power_Domain_Scalability.md)
- [Analyzer Architecture](../implementation/Analyzer_Architecture.md)

## API Reference

See `bhdl-analyzer/src/documentation/mod.rs` for complete API documentation.
