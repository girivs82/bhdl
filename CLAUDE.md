# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

BHDL (Board Hardware Description Language) is a domain-specific language for describing electronic circuit boards at multiple levels of abstraction. This Rust workspace implements a complete toolchain for parsing, analyzing, transforming, and visualizing electronic circuit designs.

## Development Commands

### Build and Test
- `cargo build` - Build all workspace crates  
- `cargo test` - Run all tests across the workspace
- `cargo test -p <crate-name>` - Test a specific crate (e.g., `cargo test -p bhdl-analyzer`)
- `cargo check` - Quick syntax check without full compilation
- `cargo clippy` - Run linter for code quality

### Running Tools
- `cargo run -p bhdl-cli` - Run the CLI tool (currently placeholder)
- `cargo run -p bhdl-visualizer` - Run the visualizer
- `cargo run -p bhdl-visualizer --bin <binary>` - Run specific visualizer binary

## Architecture

The toolchain follows a multi-stage pipeline:

**bhdl-parser** → **bhdl-ast** → **bhdl-analyzer** → **bhdl-netlist** → **bhdl-visualizer**

### Core Crates

1. **bhdl-parser**: Foundation layer providing lexical analysis and syntax parsing
   - Uses `rowan` for Concrete Syntax Tree (CST) generation  
   - Handles electrical units (kΩ, μF, MHz, etc.)
   - Error recovery during parsing

2. **bhdl-ast**: Abstraction layer converting CST to typed AST nodes
   - Provides high-level wrappers around syntax nodes
   - Key types: `SourceFile`, `Board`, `Module`, `ComponentDef`

3. **bhdl-analyzer**: Multi-pass semantic analysis (4 passes)
   - Pass 1: Build scopes and collect definitions
   - Pass 2: Resolve references and type checking  
   - Pass 3: Constant evaluation
   - Pass 4: Bounds checking and validation
   - Outputs symbol tables and diagnostics

4. **bhdl-netlist**: Structural circuit representation
   - Core types: `Netlist`, `ModuleDefinition`, `Instance`, `Net`
   - Uses `slotmap` for type-safe ID management
   - Serialization support via `serde`

5. **bhdl-visualizer**: Layout generation and SVG visualization
   - Multi-threaded placement algorithms (semantic, analytical, force-directed)
   - Intelligent routing with pathfinding and cost optimization
   - Component symbol libraries for passives, ICs, power components
   - SVG generation for circuit diagrams

6. **bhdl-synthesizer**: Future synthesis capabilities (placeholder)
7. **bhdl-cli**: Command-line interface (placeholder)  
8. **bhdl-lsp**: Language Server Protocol for IDE integration (placeholder)

### Key Data Structures

- **Parse Layer**: `SyntaxNode<BhdlLanguage>`, AST nodes (`Board`, `Module`, etc.)
- **Analysis Layer**: `SymbolTable`, `AnalysisResult`, `Diagnostic`, `ResolvedConstants` 
- **Netlist Layer**: Type-safe IDs (`ModuleId`, `InstanceId`, `NetId`), `ConnectionPoint`
- **Visualization Layer**: `LayoutEngine`, `Point`, `LayoutHints`, `RoutingCosts`

## BHDL Language Features

BHDL supports hierarchical electronic design with:
- **Structure**: boards, modules, components, interfaces
- **Parameters**: with electrical units and expressions
- **Types**: signal, power, ground with voltage/current specifications
- **Components**: resistors, capacitors, ICs with physical packages
- **Connections**: explicit net declarations and connection statements
- **Units**: Comprehensive electrical unit system (V, A, Ω, F, H, Hz, etc.)

## Test Structure

Tests are organized per crate:
- Unit tests in `src/tests/` directories
- Integration tests in crate-level `tests/` directories
- Test utilities in `tests/common.rs` modules
- Example BHDL files in `docs/examples/` for integration testing

## Component Database Management

### Database Files
- **Never commit database files** (*.db, *.sqlite) to version control
- Database files are user-specific and can become very large
- Use temporary databases for testing (automatically cleaned up)

### Sample Data
- `bhdl-components/sample_data/basic_components.kicad_sym` - Sample KiCad symbols for testing
- Use `cargo run --example kicad_integration` to see KiCad parsing in action
- Real component libraries should be imported from KiCad installations

### Setting Up Component Database
```bash
# Run the integration demo to create and populate a sample database
cargo run -p bhdl-components --example kicad_integration

# For production use, import from KiCad libraries
# (Future: CLI command will be available)
```

## Known Gaps

- CLI and LSP implementations are placeholders  
- End-to-end source-to-visualization pipeline needs integration work
- KiCad footprint parsing not yet implemented

## Important Files

- `docs/spec/BHDL_Specification.md` - Complete language specification
- `docs/examples/` - Example BHDL circuit files
- `bhdl-visualizer/src/symbols/` - Component symbol definitions
- Various `*.svg` files in `bhdl-visualizer/` - Test visualization outputs