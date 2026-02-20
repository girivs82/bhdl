# BHDL Project Roadmap

## Project Vision

BHDL aims to revolutionize electronic circuit design by providing a modern, intuitive hardware description language with professional-grade tooling. We bring software development best practices to hardware design.

## Current Status (February 2026)

### ✅ Completed Milestones

#### Core Language & Toolchain
- ✅ **Parser**: Full v2.0 flow-based syntax with `entity` keyword
- ✅ **Analyzer**: 11-pass semantic analysis with arena-based scope registry
- ✅ **Synthesizer**: Multi-format netlist generation with entity variants
- ✅ **SPICE Engine**: DC analysis with safety checking
- ✅ **Standard Library**: 38 intent functions, 137+ `.bhdl` component files
- ✅ **Testbench System**: Simulation and verification

#### Professional Tooling
- ✅ **CLI**: 9 comprehensive commands
- ✅ **LSP**: 22 features for IDE integration
- ✅ **Documentation**: Complete guides, specs, and references

#### Advanced Features
- ✅ **Intent System**: Flow tracking and validation with `for` keyword
- ✅ **Power Domain Scalability**: Wildcards and ranges (Pass 1.25/1.5)
- ✅ **Unified Simulation**: DC + safety + thermal
- ✅ **Documentation Generation**: Automatic power domain docs

#### SKALP RFC Language Infrastructure (February 2026)
- ✅ **Scope Registry**: Arena-based scope storage with parent-chain lookup
- ✅ **Rich Const Evaluator**: Physical quantities with SI units (V, A, Ω, F, H, W, Hz, s)
- ✅ **Dimensional Analysis**: Built-in functions (`parallel()`, `divider_ratio()`, `rc_cutoff()`)
- ✅ **Enums & Match**: `enum` definitions with pattern matching
- ✅ **Structured Diagnostics**: Diagnostic framework with source spans and fix suggestions
- ✅ **Parameterized Types**: Generic entity instantiation with type parameters
- ✅ **Typed Generics**: `where` clause constraints on generic type parameters
- ✅ **Monomorphization**: Pass 2.5 for generic type specialization
- ✅ **Traits**: `trait` definitions and `impl` blocks with method resolution
- ✅ **Safety Annotations**: `safety_goal` and `fault_inject` for ISO 26262

#### Language Alignment (February 2026)
- ✅ **`module` → `entity` rename**: Full codebase rename (583 files) to align with SKALP

**Rust Lines of Code**: ~345K
**BHDL Lines of Code**: ~49K
**Lib Tests Passing**: 455 (341 core + 114 bhdl-sim)
**Workspace Crates**: 14 (bhdl-visualizer replaced by bhdl-schematic)

---

## Completed: Schematic Viewer Rewrite

### ✅ bhdl-schematic — SKALP-style Schematic Viewer (February 2026)

Replaced the broken `bhdl-visualizer` (Rust SVG) with `bhdl-schematic`: a Rust extraction layer + Canvas renderer ported from SKALP's proven schematic viewer.

#### Completed Features
- [x] Rust `extract_schematic_data()`: Netlist → SchematicData JSON
- [x] Ported SKALP `schematic.js` Canvas renderer with BHDL enhancements
- [x] ELK.js automatic layout (Sugiyama layered + orthogonal edge routing)
- [x] Power/ground pin and wire coloring (BHDL-specific)
- [x] Component parameter display
- [x] Power rail visualization
- [x] Bus-width visualization with slash marks and width labels
- [x] Signal highlighting on hover
- [x] Zoom/pan with trackpad gesture support
- [x] Dark theme
- [x] Standalone HTML output (`bhdl-cli visualize`)
- [x] JSON output for tooling (`bhdl-cli visualize --json`)
- [x] LSP `bhdl.generateSchematicJson` command
- [x] Click-to-navigate support (VSCode webview ready)

#### Architecture
- **Data extraction**: `bhdl-schematic/src/extract.rs` — Netlist slotmaps → JSON
- **Layout**: ELK.js layered algorithm (vendored `elk.bundled.js`, EPL-2.0)
- **Rendering**: `bhdl-schematic/viewer/schematic.js` — HTML5 Canvas
- **Bundling**: `bhdl-schematic/src/html_bundle.rs` — self-contained HTML

---

## Release 1.0 Goals (Target: Q2 2026)

### High Priority

#### 1. VSCode Extension for Schematic Viewer
- [ ] Create `bhdl-vscode/` extension directory
- [ ] Register `bhdl.showSchematic` command
- [ ] Create WebviewPanel with `schematic.js` + `elk.bundled.js`
- [ ] Handle `navigateToEntity` / `navigateToLine` messages

#### 2. VSCode Extension
- [ ] Package LSP + schematic viewer as VSCode extension
- [ ] Syntax highlighting (TextMate grammar for `.bhdl` files)
- [ ] Snippet support for common patterns
- [ ] Publish to VSCode marketplace

#### 3. Component Database
- [ ] **Database Expansion**
  - Import full KiCad symbol libraries
  - Manufacturer part database integration
  - Parametric search capabilities

- [ ] **Footprint Support**
  - KiCad footprint parsing
  - PCB layout hints
  - DRC rule generation

#### 4. Community & Adoption
- [ ] **GitHub Repository Setup**
  - CI/CD pipeline
  - Issue templates and PR guidelines
  - CONTRIBUTING.md

- [ ] **Example Library**
  - 20+ example circuits covering common use cases
  - Tutorial series (Getting Started, Power Domains, Generics, Safety)

- [ ] **Documentation**
  - Complete beginner's guide
  - Rustdoc for all public APIs
  - Migration guide from other HDLs

### Medium Priority

#### 5. Analysis Enhancements
- [ ] **Expanded SPICE Analysis**
  - AC frequency response
  - Transient analysis
  - Monte Carlo simulation

- [ ] **Signal Integrity**
  - Transmission line analysis
  - Crosstalk detection
  - Power supply noise

#### 6. Synthesis Improvements
- [ ] **Bill of Materials**
  - Automated BOM generation
  - Cost estimation
  - Alternative part suggestions

- [ ] **PCB Integration**
  - Routing constraint generation
  - Layer stackup recommendations

#### 7. Output Formats
- [ ] KiCad netlist export
- [ ] Altium Designer export
- [ ] PDF schematic export
- [x] Interactive HTML schematics (via bhdl-schematic)

### Lower Priority

#### 8. Advanced Constraint Solving
- [ ] Electrical constraints
- [ ] Manufacturing constraints
- [ ] Timing constraints

#### 9. Tool Integrations
- [ ] LTspice / ngspice direct interface
- [ ] Gerber generation
- [ ] Pick-and-place files

---

## Version 2.0 Goals (Target: Q4 2026)

### Language Evolution
- [ ] **Hierarchical Intents**: Nested scopes, inheritance, composition
- [ ] **Advanced Constraint Solving**: Multi-domain optimization
- [ ] **Metaprogramming**: Macro system for circuit pattern generation

### Toolchain Improvements
- [ ] **Incremental Compilation**: Fast recompilation with cached analysis
- [ ] **Parallel Processing**: Multi-threaded analysis passes

### Platform Expansion
- [x] **Web Viewer**: Standalone browser-based schematic viewer (bhdl-schematic HTML output)
- [ ] **Cloud Services**: Shared project workspace

---

## Long-term Vision (2027+)

- **AI-Assisted Design**: Copilot for circuit design
- **Automated PCB Layout**: AI-driven placement and routing
- **University Adoption**: Curriculum integration
- **Industry Partnerships**: Automotive (ISO 26262), medical, aerospace
- **Standards Influence**: Push for modern HDL standards

---

**Last Updated**: February 20, 2026

*This roadmap is a living document updated as the project evolves.*
