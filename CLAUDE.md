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

## BHDL Language Features (v2.0)

BHDL v2.0 uses a flow-based syntax for intuitive circuit description:
- **Structure**: boards with direct flow connections, modules with inline pins
- **Flow operators**: `->` (connection), `<->` (bidirectional), `|>` (flow), `<=>` (interface)
- **Power/Ground**: Explicit declarations `power VCC = 5V @ 1A;` and `ground GND;`
- **Direct instantiation**: `VCC -> Res(10k).1 -> LED(red).A;`
- **Module definition**: `module Name(params) { pin name: type direction; ... }`
- **Generate constructs**: `generate for i in 0..n { ... }` for repetitive structures
- **Units**: Comprehensive electrical unit system (V, A, Ω, F, H, Hz, etc.)

**Note**: v1.0 block-based syntax (pins {}, parameters {}, etc.) has been completely removed.

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

- `docs/spec/BHDL_Complete_Specification.md` - Complete v2.0 language specification
- `docs/examples/` - Example BHDL circuit files
- `bhdl-visualizer/src/symbols/` - Component symbol definitions
- Various `*.svg` files in `bhdl-visualizer/` - Test visualization outputs

## Development Reminders

### SVG Visualization Quality Control
⚠️ **CRITICAL**: After generating any SVG visualization, always:
1. **Read and inspect the actual SVG content** - don't just assume it worked
2. **Check for component overlapping** - components should be clearly separated
3. **Verify all components are visible** - ensure viewBox includes all elements
4. **Validate proper routing** - connections should be clear and not overlapping
5. **Test with different circuit types** - simple and complex circuits
6. **Never claim success without visual verification**

Common SVG issues to watch for:
- Components rendered at same coordinates (overlapping)
- Components outside viewBox boundaries (not visible)
- Incorrect SVG transform calculations
- Missing or malformed component symbols
- Routing lines that don't connect properly to pins

## BHDL Version 2.0 Support

✅ **CURRENT STATUS**: The parser now fully supports BHDL v2.0 flow-based syntax. All v1.0 syntax support has been removed.

**Key v2.0 Features:**
- Official spec: `docs/spec/BHDL_Complete_Specification.md` (v2.0)
- Flow operators: `->` (unidirectional), `<->` (bidirectional), `|>` (flow)
- Direct component instantiation: `VCC -> Res(4.7kΩ).1 -> LED(red).A;`
- Generate constructs: `generate for i in 0..7 { ... }`
- Flow specifications: `power_flow: USB_5V |> regulation |> distribution;`
- Power/ground declarations: `power VCC = 5V @ 1A;`
- Module syntax with inline pins: `module Res(value: resistance) { pin 1: signal inout; ... }`

**Migration Notes:**
- Old v1.0 examples moved to `docs/examples/old_syntax/` for reference
- All library files updated to v2.0 syntax
- Parser, AST, and analyzer only support v2.0 constructs

## End-to-End Pipeline Development Policy

⚠️ **CRITICAL DEVELOPMENT RULE**: When developing and testing the BHDL pipeline:

1. **NO MOCKING/HARDCODING**: Never mock or hardcode values just to get through the pipeline flow
2. **PROPER IMPLEMENTATION REQUIRED**: Each stage must properly process real data from the previous stage
3. **AUTHENTIC DATA FLOW**: Parser → AST → Analyzer → Netlist → Visualizer must use authentic data structures
4. **REAL COMPONENT MATCHING**: Components must be matched to actual KiCad symbols, not placeholder data
5. **GENUINE TESTING**: Test with real circuits that can be verified at each pipeline stage

This ensures we build a robust, production-ready toolchain rather than a demo with shortcuts.

## Current Pipeline Testing Status

### Recent Progress (2025-06-16)
1. **Power Domain Propagation Fix**: Fixed analyzer to properly propagate power domains through components
   - Power domains now correctly flow from sources through resistors to other components
   - This should improve component inference by providing proper voltage/current context

2. **Component Inference Testing**: Working on testing if the power domain fix improved component selection
   - Need to verify components are being inferred with proper power ratings
   - Testing with 7805 regulator circuit

3. **Known Issues**:
   - Component matching in synthesizer fails to find components in database
   - Visualization has overlapping components and routing issues
   - Need to complete end-to-end pipeline with proper component database integration

### Test Commands
- `cargo run --bin test_pipeline_7805` - Test 7805 regulator circuit through pipeline
- `cargo run --bin end_to_end_test` - Run complete end-to-end test
- `cargo run -p bhdl-components --example kicad_integration` - Set up component database