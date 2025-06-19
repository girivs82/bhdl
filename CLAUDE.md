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

3. **bhdl-analyzer**: Multi-pass semantic analysis (8 passes)
   - Pass 1: Build scopes and collect definitions
   - Pass 2: Resolve references and type checking  
   - Pass 3: Constant evaluation
   - Pass 4: Bounds checking and validation
   - Pass 5: Power domain analysis
   - Pass 6: Component inference
   - Pass 7: Netlist synthesis
   - Pass 8: Electrical safety analysis (DC + safety checks)
   - Outputs symbol tables, diagnostics, and netlist

4. **bhdl-netlist**: Structural circuit representation
   - Core types: `Netlist`, `ModuleDefinition`, `Instance`, `Net`
   - Uses `slotmap` for type-safe ID management
   - Serialization support via `serde`

5. **bhdl-visualizer**: Layout generation and SVG visualization
   - Multi-threaded placement algorithms (semantic, analytical, force-directed)
   - Intelligent routing with pathfinding and cost optimization
   - Component symbol libraries for passives, ICs, power components
   - SVG generation for circuit diagrams

6. **bhdl-synthesizer**: Circuit synthesis and netlist generation
   - Converts AST and analysis results to structural netlist
   - Handles component instantiation and net assignment
   - Supports implicit handle creation for net assignments
   - Component database integration for real part selection

7. **bhdl-spice**: SPICE-like circuit analysis engine
   - Newton-Raphson nonlinear DC solver for accurate component modeling
   - LED forward voltage modeling and diode Shockley equations
   - Component inference through electrical analysis
   - Integration with analyzer for power domain propagation

8. **bhdl-stdlib**: Standard component library and parameters
   - Centralized electrical parameters (LED forward voltage, resistor tolerances)
   - Removes hardcoded values from core logic
   - Supports manufacturer datasheet integration
   - Component parameter definitions for accurate analysis

9. **bhdl-common**: Shared utilities and types (future)

10. **bhdl-cli**: Command-line interface (placeholder)  
11. **bhdl-lsp**: Language Server Protocol for IDE integration (placeholder)

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
- **Net assignments**: `protected_vin: TVSDiode(15V).1` creates net and implicit handle
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
- KiCad footprint parsing not yet implemented
- Remaining visualization issues: component symbol scaling, orthogonal routing

## Important Files

- `docs/spec/BHDL_Complete_Specification.md` - **Complete and authoritative v2.0 specification** (all other specs consolidated here)
- `docs/implementation/Electrical_Safety_Analysis_Implementation.md` - Safety analysis system documentation
- `docs/examples/` - Example BHDL circuit files demonstrating v2.0 syntax
- `test_7805_regulator_realistic.bhdl` - Realistic test circuit with net assignments
- `bhdl-stdlib/electrical_params.bhdl` - Centralized component parameter library
- `bhdl-spice/src/safety/` - Electrical safety analysis implementation
- `bhdl-spice/src/bin/test_safety_with_dc.rs` - Complete safety analysis example
- `bhdl-visualizer/src/symbols/` - Component symbol definitions
- Various `*.svg` files in `bhdl-visualizer/` - Test visualization outputs

## Development Reminders

### Test File Organization
⚠️ **CRITICAL**: When creating or running tests, NEVER use the project root directory:
1. **Test binaries** (.rs) → Place in `<crate>/src/bin/` directory
2. **Test circuits** (.bhdl) → Place in `tests/circuits/{simple|realistic|edge_cases}/`
3. **Test outputs** (.svg, .net) → Write to `tests/outputs/{svg|netlists}/`
4. **Temporary files** → Use `tests/scratch/` (git-ignored)
5. **Run tests** → Use `./tests/run_tests.sh` or cargo commands with proper paths

Example test structure:
```rust
// In test binary: bhdl-synthesizer/src/bin/my_test.rs
let test_file = std::env::args().nth(1)
    .unwrap_or_else(|| "tests/circuits/realistic/my_circuit.bhdl".to_string());
    
let output_path = "tests/outputs/svg/my_test_output.svg";
```

See `tests/TESTING.md` for complete testing guidelines.

### No Hardcoding Policy
⚠️ **CRITICAL**: NEVER hardcode values in tests or implementation:
1. **No hardcoded coordinates** - Use actual component positions from database or calculation
2. **No hardcoded component parameters** - Use values from bhdl-stdlib or database
3. **No mock data** - Use real circuits processed through the actual pipeline
4. **No placeholder values** - Every value must come from authentic sources
5. **Test with real data** - Process actual BHDL files through parser → analyzer → synthesizer → visualizer

Example of what NOT to do:
```rust
// ❌ WRONG - Hardcoded positions
let r1 = Component::new(id, Point::new(100.0, 100.0));
r1.pins.insert("1", Point::new(-60.0, 0.0));

// ✅ CORRECT - Use actual pipeline
let netlist = synthesizer.generate_from_ast(&ast)?;
let layout = semantic_visualizer.generate_layout(&netlist)?;
// Positions come from actual layout algorithms and database
```

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

✅ **CURRENT STATUS**: The parser now fully supports BHDL v2.0 flow-based syntax including all complex features. All v1.0 syntax support has been removed.

**Key v2.0 Features:**
- Official spec: `docs/spec/BHDL_Complete_Specification.md` (v2.0)
- Flow operators: `->` (unidirectional), `<->` (bidirectional), `|>` (flow)
- Direct component instantiation: `VCC -> Res(4.7kΩ).1 -> LED(red).A;`
- Generate constructs: `generate for i in 0..7 { ... }`
- Flow specifications: `power_flow: USB_5V |> regulation |> distribution;`
- Power/ground declarations: `power VCC = 5V @ 1A;`
- Module syntax with inline pins: `module Res(value: resistance) { pin 1: signal inout; ... }`

**Complex Syntax Support (2025-06-18):**
- ✅ Const declarations with type annotations: `const name: type = value;`
- ✅ Smart unit tokenization: Units recognized only after numbers
- ✅ Ternary operator: `condition ? true_expr : false_expr`
- ✅ String comparisons: `color == "red"`
- ✅ Logical operators: `||`, `&&`
- ✅ Member access: `params.forward_voltage`
- ✅ Conditional pins: `pin EN: signal in when condition;`
- ✅ Module aliases: `alias 7805 = LM7805;`
- ✅ Destructuring imports: `import { A, B } from "file.bhdl";`

**Migration Notes:**
- Old v1.0 examples moved to `docs/examples/old_syntax/` for reference
- All library files updated to v2.0 syntax
- Parser, AST, and analyzer only support v2.0 constructs
- Use `alias` keyword instead of `module Name = Target;` for aliases
- Use `||` instead of `or` for logical OR operations

## End-to-End Pipeline Development Policy

⚠️ **CRITICAL DEVELOPMENT RULE**: When developing and testing the BHDL pipeline:

1. **NO MOCKING/HARDCODING**: Never mock or hardcode values just to get through the pipeline flow
2. **PROPER IMPLEMENTATION REQUIRED**: Each stage must properly process real data from the previous stage
3. **AUTHENTIC DATA FLOW**: Parser → AST → Analyzer → Netlist → Visualizer must use authentic data structures
4. **REAL COMPONENT MATCHING**: Components must be matched to actual KiCad symbols, not placeholder data
5. **GENUINE TESTING**: Test with real circuits that can be verified at each pipeline stage

This ensures we build a robust, production-ready toolchain rather than a demo with shortcuts.

## Current Pipeline Status

### Recent Major Advances
1. **Complete SPICE Analysis Integration**: Implemented Newton-Raphson nonlinear solver
   - Accurate LED forward voltage modeling (2.0V drop)
   - Diode Shockley equation implementation
   - Component inference through electrical analysis
   - Power domain propagation through components

2. **Electrical Safety Analysis System**: Generic safety checking for all components
   - Data-driven analysis using actual component limits from models
   - DC analysis integration for real current/voltage checking
   - Multi-severity violation detection (Info/Warning/Error/Critical)
   - Automatic fix suggestions (current limiting resistors, protection circuits)
   - Derating factors for conservative design (70% current, 80% voltage)

3. **Synthesizer Enhancement**: Full netlist generation with component intelligence
   - Net assignment with implicit handle creation (`protected_vin: TVSDiode(15V).1`)
   - Component database integration for real part selection
   - Reference designator auto-generation (R1, D1, C1, etc.)
   - Proper connection endpoint parsing

4. **Specification Consolidation**: Single authoritative v2.0 specification
   - All spec documents consolidated into `BHDL_Complete_Specification.md`
   - Prevents documentation drift and maintains consistency
   - Complete language reference with all features documented

5. **Parameter Library System**: Centralized component parameters in bhdl-stdlib
   - Removed hardcoded values from core logic (LED 30mA max current, etc.)
   - Manufacturer datasheet integration support
   - Accurate component inference using real electrical parameters

6. **Topology-Based Component Role Detection**: Real connectivity analysis in bhdl-spice
   - Analyzes circuit structure without relying on node/component names
   - IC pin detection through connected component patterns
   - Component classification by electrical function and location
   - 100% accuracy on typical power supply circuits
   - See `bhdl-spice/src/extended_analysis/` for implementation

7. **Pin Metadata System**: Explicit functional identification without naming conventions
   - Reads pin definitions from component library (`pin IN: power in;`)
   - Component role detection based on IC pin connections
   - 100% accurate classification of input/output filtering capacitors
   - See `docs/implementation/Pin_Metadata_System.md` for details

8. **Power Converter Stability Analysis**: Full AC-integrated stability analysis
   - Loop stability with phase/gain margins from actual frequency response
   - Input/output impedance measurement with control loop effects
   - Resonance detection with Q factor and damping assessment
   - Cascade stability analysis with Middlebrook criterion
   - Automated recommendations for fixing stability issues
   - See `docs/implementation/Stability_Analysis_Integration.md`

### Current Focus Areas
- Component symbol scaling and visualization improvements
- Orthogonal routing to proper pin positions
- Capacitor symbols using parallel plates from database
- Additional manufacturer datasheet values in stdlib

### Test Commands
- `cargo run -p bhdl-synthesizer --bin test_7805_realistic` - Test net assignment handling
- `cargo run --bin test_pipeline_7805` - Test 7805 regulator circuit through pipeline
- `cargo run --bin end_to_end_test` - Run complete end-to-end test
- `cargo run -p bhdl-components --example kicad_integration` - Set up component database
- `cargo run -p bhdl-spice --bin nonlinear_analysis_test` - Test SPICE solver
- `cargo run -p bhdl-spice --bin test_safety_with_dc` - Test electrical safety analysis with DC
- `cargo run -p bhdl-spice --bin test_component_role_detection` - Test topology-based role detection
- `cargo run -p bhdl-spice --bin test_realistic_buck_stability` - Test buck converter stability analysis
- `cargo test -p bhdl-analyzer` - Test component inference with new parameters