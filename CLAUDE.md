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
- `cargo run -p bhdl-cli --bin bhdl-cli <circuit.bhdl> <command>` - Run the CLI tool
  - `doc` - Generate power domain documentation (see `docs/cli/DOC_COMMAND.md`)
  - `parse` - Parse and validate BHDL syntax
  - `analyze` - Run semantic analysis
  - `synthesize` - Generate netlist
  - `visualize` - Create interactive schematic visualization (HTML)

## Architecture

The toolchain follows a multi-stage pipeline:

**bhdl-parser** → **bhdl-ast** → **bhdl-analyzer** → **bhdl-netlist** → **bhdl-schematic**

### Core Crates

1. **bhdl-parser**: Foundation layer providing lexical analysis and syntax parsing
   - Uses `rowan` for Concrete Syntax Tree (CST) generation  
   - Handles electrical units (kΩ, μF, MHz, etc.)
   - Error recovery during parsing

2. **bhdl-ast**: Abstraction layer converting CST to typed AST nodes
   - Provides high-level wrappers around syntax nodes
   - Key types: `SourceFile`, `Board`, `Entity`, `ComponentDef`

3. **bhdl-analyzer**: Multi-pass semantic analysis (11+ passes)
   - Pass 1: Build scopes and collect definitions (uses ScopeRegistry arena)
   - Pass 1.25: Early component instance registry (scalability)
   - Pass 1.5: Power domain expansion with wildcards/ranges (scalability)
   - Pass 2: Resolve references and type checking
   - Pass 2.5: Monomorphization (generic type instantiation)
   - Pass 3: Constant evaluation (rich ConstValue with physical quantities)
   - Pass 4: Bounds checking and validation
   - Pass 5: Power domain analysis
   - Pass 6: Component inference
   - Pass 6.5: SPICE synthesis
   - Pass 7: Power sequencing
   - Pass 8: Attribute analysis
   - Pass 9: Flow tracking and intent resolution
   - Pass 10: Unified simulation
   - Pass 11: Safety analysis
   - Outputs symbol tables, diagnostics, netlist, and expanded power domains

4. **bhdl-netlist**: Structural circuit representation
   - Core types: `Netlist`, `ModuleDefinition`, `Instance`, `Net`
   - Uses `slotmap` for type-safe ID management
   - Serialization support via `serde`

5. **bhdl-schematic**: Interactive schematic viewer (replaces bhdl-visualizer)
   - Rust extraction layer: Netlist → SchematicData JSON
   - TypeScript/Canvas renderer ported from SKALP's proven schematic viewer
   - Custom topological placement with orthogonal wire routing
   - HTML5 Canvas interactive rendering (zoom, pan, hover)
   - Standalone HTML output or JSON for LSP/IDE integration
   - Power rail visualization, component parameter display

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

10. **bhdl-cli**: Command-line interface ✅ **PRODUCTION READY**
   - **9 comprehensive commands**: parse, analyze, synthesize, visualize, spice, pipeline, simulate, intents, doc
   - Parse and validate BHDL syntax
   - Full semantic analysis with all 11 passes
   - Netlist generation (JSON and SPICE formats)
   - Interactive schematic visualization (HTML output)
   - Component role detection and SPICE analysis
   - Complete pipeline execution
   - Simulation with testbenches
   - Intent analysis and flow tracking
   - Power domain documentation generation
   - See `docs/implementation/CLI_IMPLEMENTATION_GUIDE.md` for complete reference

11. **bhdl-lsp**: Language Server Protocol for IDE integration ✅ **PRODUCTION READY**
   - **22 major features**: diagnostics, autocomplete, hover, go to definition, find references, rename, and more
   - Full Intent System integration with autocomplete for all 38 intent functions
   - Real-time semantic analysis
   - Works with any LSP-compatible editor (VSCode, Neovim, Emacs, etc.)
   - See `docs/implementation/LSP_IMPLEMENTATION_SUMMARY.md` for complete reference

### Key Data Structures

- **Parse Layer**: `SyntaxNode<BhdlLanguage>`, AST nodes (`Board`, `Entity`, etc.)
- **Analysis Layer**: `SymbolTable`, `AnalysisResult`, `Diagnostic`, `ResolvedConstants` 
- **Netlist Layer**: Type-safe IDs (`ModuleId`, `InstanceId`, `NetId`), `ConnectionPoint`
- **Visualization Layer**: `SchematicData`, `SchematicInstance`, `SchematicNet`, `PowerRail`

## BHDL Language Features (v2.0)

BHDL v2.0 uses a flow-based syntax for intuitive circuit description:
- **Structure**: boards with direct flow connections, entities with inline pins
- **Flow operators**: `->` (connection), `<->` (bidirectional), `|>` (flow), `<=>` (interface)
- **Power/Ground**: Explicit declarations `power VCC = 5V @ 1A;` and `ground GND;`
- **Direct instantiation**: `VCC -> Res(10k).1 -> LED(red).A;`
- **Net assignments**: `protected_vin: TVSDiode(15V).K` creates net and implicit handle
- **Entity definition**: `entity Name(params) { pin name: type direction; ... }`
- **Generate constructs**: `generate for i in 0..n { ... }` for repetitive structures
- **Units**: Comprehensive electrical unit system (V, A, Ω, F, H, Hz, etc.)
- **Enums**: `enum PinState { High, Low, HighZ, Unknown }` with match expressions
- **Match expressions**: `match state { PinState::High => ..., PinState::Low => ... }`
- **Generics**: `entity Filter<T: Passive>(cutoff: frequency) where T: HasValue { ... }`
- **Traits**: `trait Filterable { fn cutoff_frequency() -> frequency; }` with `impl` blocks
- **Safety annotations**: `safety_goal ASIL_B("description") { ... }` and `fault_inject { ... }`

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

- KiCad footprint parsing not yet implemented
- Future: VSCode extension wrapper for LSP schematic webview
- Additional manufacturer datasheet integration opportunities

## Important Files

- `docs/spec/BHDL_Complete_Specification.md` - **Complete and authoritative v2.0 specification** (all other specs consolidated here)
- `docs/proposals/Simulation_Architecture_Proposal.md` - **Intent system design** (flow-based design intent)
- `docs/implementation/Intent_System_Implementation_Plan.md` - Detailed implementation plan for intent system
- `docs/implementation/Electrical_Safety_Analysis_Implementation.md` - Safety analysis system documentation
- `docs/examples/` - Example BHDL circuit files demonstrating v2.0 syntax
- `test_7805_regulator_realistic.bhdl` - Realistic test circuit with net assignments
- `bhdl-stdlib/electrical_params.bhdl` - Centralized component parameter library
- `bhdl-stdlib/src/intents/` - Intent function library (to be implemented)
- `bhdl-spice/src/safety/` - Electrical safety analysis implementation
- `bhdl-spice/src/bin/test_safety_with_dc.rs` - Complete safety analysis example
- `bhdl-schematic/viewer/schematic.js` - Canvas-based schematic renderer (ported from SKALP)
- `bhdl-schematic/viewer/schematic.js` - Canvas-based schematic renderer with custom layout

## Development Reminders

### Test File Organization
⚠️ **CRITICAL**: When creating or running tests, NEVER use the project root directory:
1. **Test binaries** (.rs) → Place in `<crate>/src/bin/` directory
2. **Test circuits** (.bhdl) → Place in `tests/circuits/{simple|realistic|edge_cases}/`
3. **Test outputs** (.html, .net) → Write to `tests/outputs/{schematics|netlists}/`
4. **Temporary files** → Use `tests/scratch/` (git-ignored)
5. **Run tests** → Use `./tests/run_tests.sh` or cargo commands with proper paths

Example test structure:
```rust
// In test binary: bhdl-synthesizer/src/bin/my_test.rs
let test_file = std::env::args().nth(1)
    .unwrap_or_else(|| "tests/circuits/realistic/my_circuit.bhdl".to_string());
    
let output_path = "tests/outputs/schematics/my_test_output.html";
```

See `tests/TESTING.md` for complete testing guidelines.

### No Hardcoding Policy
⚠️ **CRITICAL**: NEVER hardcode values in tests or implementation:
1. **No hardcoded coordinates** - Use actual component positions from database or calculation
2. **No hardcoded component parameters** - Use values from bhdl-stdlib or database
3. **No mock data** - Use real circuits processed through the actual pipeline
4. **No placeholder values** - Every value must come from authentic sources
5. **Test with real data** - Process actual BHDL files through parser → analyzer → synthesizer → schematic

Example of what NOT to do:
```rust
// ❌ WRONG - Hardcoded positions
let r1 = Component::new(id, Point::new(100.0, 100.0));
r1.pins.insert("1", Point::new(-60.0, 0.0));

// ✅ CORRECT - Use actual pipeline
let netlist = synthesizer.generate_from_ast(&ast)?;
let schematic = schematic_extractor.extract(&netlist)?;
// Layout computed by custom placer from SchematicData
```

### Schematic Visualization Quality Control
⚠️ **CRITICAL**: After generating any HTML schematic output, always:
1. **Read and inspect the actual HTML/JSON content** - don't just assume it worked
2. **Check for component overlapping** - components should be clearly separated
3. **Verify all components are visible** - ensure the schematic data includes all elements
4. **Validate proper routing** - connections should be clear and not overlapping
5. **Test with different circuit types** - simple and complex circuits
6. **Never claim success without visual verification**

Common schematic issues to watch for:
- Components rendered at same coordinates (overlapping)
- Components missing from SchematicData JSON
- Incorrect layout calculations
- Missing or malformed component parameters
- Routing edges that don't connect properly to ports

## BHDL Version 2.0 Support

✅ **CURRENT STATUS**: The parser now fully supports BHDL v2.0 flow-based syntax including all complex features. All v1.0 syntax support has been removed.

🚧 **NEXT MAJOR FEATURE**: Flow-based intent system using `for` keyword to capture design intent.

**Key v2.0 Features:**
- Official spec: `docs/spec/BHDL_Complete_Specification.md` (v2.0)
- Flow operators: `->` (unidirectional), `<->` (bidirectional), `|>` (flow)
- Direct component instantiation: `VCC -> Res(4.7kΩ).1 -> LED(red).A;`
- Generate constructs: `generate for i in 0..7 { ... }`
- Flow specifications: `power_flow: USB_5V |> regulation |> distribution;`
- Power/ground declarations: `power VCC = 5V @ 1A;`
- Entity syntax with inline pins: `entity Res(value: resistance) { pin 1: signal inout; ... }`

**Complex Syntax Support (2025-06-18):**
- ✅ Const declarations with type annotations: `const name: type = value;`
- ✅ Smart unit tokenization: Units recognized only after numbers
- ✅ Ternary operator: `condition ? true_expr : false_expr`
- ✅ String comparisons: `color == "red"`
- ✅ Logical operators: `||`, `&&`
- ✅ Member access: `params.forward_voltage`
- ✅ Conditional pins: `pin EN: signal in when condition;`
- ✅ Entity aliases: `alias 7805 = LM7805;`
- ✅ Destructuring imports: `import { A, B } from "file.bhdl";`
- ✅ Numeric pin references: `resistor.1`, `capacitor.2` (2025-10-12)

**SKALP RFC Language Features (2026-02):**
- ✅ Enum definitions: `enum PinState { High, Low, HighZ, Unknown }`
- ✅ Match expressions: `match expr { Pattern => result, ... }`
- ✅ Generic entities: `entity Filter<T>(cutoff: frequency) { ... }`
- ✅ Where clauses: `where T: Passive + HasValue`
- ✅ Trait definitions: `trait Filterable { fn cutoff_frequency() -> frequency; }`
- ✅ Trait implementations: `impl Filterable for LowPassFilter { ... }`
- ✅ Safety goals: `safety_goal ASIL_B("description") { ... }`
- ✅ Fault injection: `fault_inject { ... }`
- ✅ Rich const evaluation: Physical quantities with dimensional analysis
- ✅ Arena-based scope registry: Replaces stack-based scope lookup

**Migration Notes:**
- Old v1.0 examples moved to `docs/examples/old_syntax/` for reference
- All library files updated to v2.0 syntax
- Parser, AST, and analyzer only support v2.0 constructs
- Use `alias` keyword instead of `entity Name = Target;` for aliases
- Use `||` instead of `or` for logical OR operations

## End-to-End Pipeline Development Policy

⚠️ **CRITICAL DEVELOPMENT RULE**: When developing and testing the BHDL pipeline:

1. **NO MOCKING/HARDCODING**: Never mock or hardcode values just to get through the pipeline flow
2. **PROPER IMPLEMENTATION REQUIRED**: Each stage must properly process real data from the previous stage
3. **AUTHENTIC DATA FLOW**: Parser → AST → Analyzer → Netlist → Schematic must use authentic data structures
4. **REAL COMPONENT MATCHING**: Components must be matched to actual KiCad symbols, not placeholder data
5. **GENUINE TESTING**: Test with real circuits that can be verified at each pipeline stage

This ensures we build a robust, production-ready toolchain rather than a demo with shortcuts.

## Current Pipeline Status

### Recent Major Advances

15. **SKALP RFC Language Infrastructure** (2026-02): ✅ ALL 10 TASKS COMPLETED
    - **Scope Registry** (Task 1): Arena-based scope storage with parent-chain lookup in `scope_registry.rs`
    - **Rich Const Evaluator** (Task 2): ConstValue enum with physical quantities (V, A, Ω, F, H, W, Hz, s)
    - **Dimensional Analysis** (Task 3): Built-in functions (parallel(), divider_ratio(), rc_cutoff(), etc.)
    - **Enums & Match** (Task 4): `enum` definitions with `match` expressions in parser, AST, and analyzer
    - **Structured Diagnostics** (Task 5): Diagnostic framework in bhdl-common
    - **Parameterized Types** (Task 6): Generic entity instantiation with type parameters
    - **Typed Generics** (Task 7): `where` clause constraints on generic type parameters
    - **Monomorphization** (Task 8): Pass 2.5 for generic type specialization
    - **Traits** (Task 9): `trait` definitions and `impl` blocks with method resolution
    - **Safety Annotations** (Task 10): `safety_goal` and `fault_inject` constructs for ISO 26262
    - See `docs/proposals/RFC_SKALP_Language_Infrastructure.md` for complete RFC

10. **Complete Intent and Flow System**: Full implementation of design intent framework
   - Parser support for `for` keyword on flow statements and net declarations
   - Flow tracking system that identifies components in signal paths
   - Intent resolution with simulation mode determination
   - Hierarchical intent propagation through entity instances
   - Standard library of intent functions (delay, analog, digital, etc.)
   - See `docs/implementation/Intent_and_Flow_System.md` for details

11. **Consistent Net Reference Syntax**: All nets require @ prefix
   - Power and ground domains must use @ prefix (@VCC, @GND)
   - Bare identifiers that are nets produce clear error messages
   - Implicit net creation only in flow contexts (middle of chains)
   - Complete validation in Pass 2 with proper diagnostics

12. **Power Domains as Net Attributes**: Unified representation
   - Power domains stored as nets with NetAttribute metadata
   - Visible in symbol table like regular nets
   - Supports voltage, current, tolerance, and control properties
   - Electrical unit conversion (mA→A, mV→V, kΩ→Ω, etc.)

13. **Power Domain Scalability Features** (2025-10-11): ✅ COMPLETED
   - **Pass 1.25**: Early Component Instance Registry
     - Scans AST before power domain expansion
     - Registers all component instances (sensor_0, sensor_1, etc.)
     - Enables wildcard pattern matching for bulk operations
   - **Pass 1.5 Enhancements**: Power Domain Expansion with scalability
     - **Wildcard expansion**: `sensor[*].VCC` → expands to all matching instances
     - **Range expansion**: `fpga.VCCO[0..7]` → expands to 8 indexed pins
     - **Decoupling generation**: Automatic capacitor instantiation with placement constraints
   - Reduces verbosity by 10-100x for large designs
   - See `docs/implementation/Power_Domain_Scalability_Implementation.md` for details
   - Test: `cargo run -p bhdl-analyzer --bin test_scalability_comprehensive`

14. **Enhanced Documentation Generation** (2025-10-12): ✅ COMPLETED
   - Automatic Markdown documentation for power domains
   - **5 comprehensive sections**: voltage summary, power tree, BOM, budget analysis, connection summary
   - **Pattern detection**: Shows wildcard/range expansions
   - **Capacitance parsing**: Smart unit conversion (pF/nF/µF/mF/F)
   - **CLI integration**: `bhdl circuit.bhdl doc` command with multiple modes
   - **Flexible output**: Full docs, BOM-only, budget-only modes
   - See `docs/cli/DOC_COMMAND.md` and `docs/examples/documentation_usage.md`
   - Test: `cargo run -p bhdl-analyzer --bin test_documentation_generation`

### Recent Major Advances (Previous)
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
   - Net assignment with implicit handle creation (`protected_vin: TVSDiode(15V).K`)
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

9. **Unified Data Model**: Complete elimination of lossy conversions
   - Netlist augmented with AnalysisData instead of complex conversions
   - Single authoritative PinMetadata structure in bhdl-common
   - Symbol table made public in analyzer for proper data extraction
   - All component-specific data flows from stdlib (no hardcoding)
   - See `docs/implementation/Unified_Data_Model.md` for details

10. **Flow-Based Intent System**: Revolutionary design intent capture (PLANNED)
   - Single `for` keyword attaches intent to signal flows
   - Intent applies to entire flow path (multiple nets/pins), not individual nets
   - All intent logic in stdlib (38+ standard functions)
   - Different branches can have different intents
   - Maps to simulation strategies (PureDigital, DigitalWithTiming, MixedSignal, AnalogRequired)
   - See `docs/proposals/Simulation_Architecture_Proposal.md` for complete design
   - See `docs/implementation/Intent_System_Implementation_Plan.md` for implementation details

### Current Focus Areas
- **SKALP RFC Language Infrastructure**: ✅ ALL 10 TASKS COMPLETED
  - Scope registry, rich const eval, dimensional analysis
  - Enums/match, structured diagnostics, parameterized types
  - Typed generics, monomorphization, traits, safety annotations
- **GLACIER-Driven Component Physical Selection** (PLANNED)
  - User specifies only electrical values: `Res(10k)`, `Cap(100nF)`
  - GLACIER DC simulation computes operating point (V, I, P at every node)
  - Post-simulation pass in Pass 6 (Component Inference) selects physical parameters:
    - **Resistor**: P = V×I → power rating → package size (0402 ≤ 1/16W, 0603 ≤ 1/10W, 0805 ≤ 1/8W, 1206 ≤ 1/4W)
    - **Capacitor**: V across → voltage rating (2× derating) → dielectric (C0G for small/precision, X7R for bulk) → package
    - **LED**: I through → package size
    - **TVS**: clamping voltage + peak current → package
  - Physical parameters stored as attributes on netlist instances for BOM/layout
  - Stdlib stays clean (electrical intent only); toolchain determines physical realization
- VSCode extension for schematic webview (Phase 2)
- Further simulation tool integration

### Test Commands
- `cargo run -p bhdl-analyzer --bin test_scalability_comprehensive` - Test power domain scalability features
- `cargo run -p bhdl-synthesizer --bin test_scalability_pipeline_simple` - Test scalability through full pipeline
- `cargo run -p bhdl-synthesizer --bin test_7805_realistic` - Test net assignment handling
- `cargo run --bin test_pipeline_7805` - Test 7805 regulator circuit through pipeline
- `cargo run --bin end_to_end_test` - Run complete end-to-end test
- `cargo run -p bhdl-components --example kicad_integration` - Set up component database
- `cargo run -p bhdl-spice --bin nonlinear_analysis_test` - Test SPICE solver
- `cargo run -p bhdl-spice --bin test_safety_with_dc` - Test electrical safety analysis with DC
- `cargo run -p bhdl-spice --bin test_component_role_detection` - Test topology-based role detection
- `cargo run -p bhdl-spice --bin test_realistic_buck_stability` - Test buck converter stability analysis
- `cargo test -p bhdl-analyzer` - Test component inference with new parameters

## Intent System Quick Reference

The intent system allows designers to declare the purpose of signal flows:

```bhdl
// Intent applies to entire flow path
net protection: sensor -> tvs: TVSDiode(6V).K -> tvs.A -> r: Res(1k).1 -> r.2 -> @protected
    for input_protection(overvoltage: 6V, current_limit: 5mA);

// Different intents on branches
net monitor: @protected -> buf: Buffer().A -> buf.Y -> status_out
    for fault_detection(response: 10ns);
    
net measure: @protected -> filter -> adc
    for precision_measurement(accuracy: 0.1%);
```

### Key Intent Categories:
- **Timing**: delay(), pulse_stretch(), debounce(), stable_for()
- **Signal Processing**: noise_filtering(), anti_alias(), fast_response()
- **Protection**: input_protection(), overvoltage_clamp(), glitch_immunity()
- **Power/Analog**: signal_amplification(), level_shifting(), current_limiting()
- **Digital**: signal_buffering(), output_buffering(), signal_distribution()
- **Measurement**: precision_measurement(), data_logging(), control_loop()
- **Safety**: automotive_safety(), industrial_control(), medical_safety()

### Intent Implementation Status:
- [x] Parser support for `for` keyword on flow statements
- [x] Stdlib intent function framework with standard intents
- [x] Flow tracking engine with component identification
- [x] Hierarchical intent propagation through entities
- [x] Net syntax consistency with @ prefix requirement
- [ ] Tool integration (partial - simulation coordinator ready)
- [x] Documentation and examples

### Key Principle: "One Flow, One Intent"
Intent applies to entire signal flow paths, not individual nets. When a net branches, each branch can have its own intent. This captures design purpose explicitly and enables intelligent tool automation.