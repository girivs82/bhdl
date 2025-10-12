# BHDL Next Steps - Suggested Development Priorities

**Last Updated**: October 12, 2025
**Current Status**: Advanced Pattern Matching ✅ Complete

## Recently Completed

✅ **Power Domain Scalability - Complete Pipeline** (Oct 11, 2025)
- **Analyzer** Pass 1.25: Early Component Instance Registry
- **Analyzer** Pass 1.5: Wildcard expansion, range expansion, decoupling generation
- **Analyzer** Fuzzy matching error messages with "Did you mean...?" suggestions
- **Synthesizer** Phase 2.7: Full netlist integration with capacitor generation
- **Visualizer** Power/Ground net visual distinction with type-specific styling
- Full test coverage and documentation
- 10-100x verbosity reduction for large designs
- End-to-end pipeline from BHDL source → Parser → Analyzer → Synthesizer → Visualizer

## High Priority Features

### 1. Synthesizer Integration for Power Domain Expansion

**Status**: ✅ Complete (Oct 11, 2025)
**Effort**: Completed
**Impact**: High

**Completed Work**:
- ✅ Updated synthesizer to read `power_domain_expansion` from `AnalysisResult`
- ✅ Created proper Capacitor module with + and - pins
- ✅ Generate netlist instances for all decoupling capacitors
- ✅ Create pin instances and proper power/ground connections
- ✅ Connect expanded power distribution to component pins
- ✅ Store capacitor values and placement as instance attributes
- ✅ Use proper NetClass for power and ground nets

**Files Modified**:
- `bhdl-synthesizer/src/lib.rs:1866-2017` - Complete rewrite of `process_power_domain_expansion()`

**Documentation**:
- `CHANGELOG_2025-10-11_SYNTHESIZER_INTEGRATION.md` - Complete implementation details

**Known Limitation**:
- Parser doesn't support numeric pin references (`.1`) in power domain distribution blocks
- This prevents end-to-end testing but doesn't affect the synthesizer implementation
- Parser enhancement needed as separate task

### 2. Visualizer Support for Power Domains

**Status**: ✅ Complete (Phase 1 - Oct 11, 2025)
**Effort**: Completed
**Impact**: High

**Completed Work (Phase 1)**:
- ✅ Extract net type from netlist NetClass (Power/Ground/Signal)
- ✅ Visual distinction for power nets (thick red lines, 2.5px)
- ✅ Visual distinction for ground nets (thick black lines, 2.0px)
- ✅ Signal nets remain blue with normal thickness (1.2px)
- ✅ Automatic classification from netlist data
- ✅ Integration with synthesizer power domain expansion

**Files Modified**:
- `bhdl-visualizer/src/types.rs` - Added NetType enum
- `bhdl-visualizer/src/layout.rs:459-469` - Net type extraction
- `bhdl-visualizer/src/renderer.rs:110-122` - Type-specific rendering
- `bhdl-visualizer/src/svg.rs:47-49` - CSS styles for power/ground
- `bhdl-visualizer/src/lib.rs:33` - Export NetType

**Documentation**:
- `CHANGELOG_2025-10-11_VISUALIZER_POWER_DOMAINS.md` - Complete implementation details

**Remaining Work (Phase 2 - Future)**:
- [ ] Decoupling capacitor placement based on constraints (near/distributed)
- [ ] Power domain boundary visualization
- [ ] Placement hints for "near component" constraints
- [ ] Multi-voltage domain color coding

### 3. Generate Block Wildcard Integration

**Status**: ✅ Complete (Oct 11, 2025)
**Effort**: Completed
**Impact**: Medium

**Goal**: Enable wildcard expansion for components created in generate blocks.

**Completed Work**:
- ✅ Added `generate_blocks()` methods to Board and Module AST nodes
- ✅ Added `loop_var()` and `range_bounds()` methods to ForLoopGenerate
- ✅ Enhanced instance registry to scan and expand generate blocks
- ✅ Created instance name extraction from NET_REF nodes
- ✅ Implemented component type extraction for CONNECTION_STMT
- ✅ Full test coverage with test_generate_wildcard.rs
- ✅ Comprehensive documentation in CHANGELOG_2025-10-11_GENERATE_WILDCARD.md

**Files Modified**:
- `bhdl-ast/src/items.rs:92-95, 145-148` - Added generate_blocks() methods
- `bhdl-ast/src/blocks.rs:133-177` - Added loop information extraction
- `bhdl-analyzer/src/passes/instance_registry.rs:136-359` - Generate block scanning and expansion

**Test Results**: ✅ All tests passing - 11 instances registered, wildcards expanding correctly

**Remaining Work**:
- [ ] Nested generate blocks (future enhancement)
- [ ] If-generate support (future enhancement)

### 4. Hierarchical Wildcard Expansion

**Status**: ✅ Complete (Oct 12, 2025)
**Effort**: Completed
**Impact**: Medium

**Goal**: Expand wildcards across module instance boundaries.

**Example**:
```bhdl
module SensorModule() {
    sensor: TempSensor();
    // ...
}

board System {
    sensor_board_0: SensorModule();
    sensor_board_1: SensorModule();

    power_domain @VCC = 5V @ 1A {
        distribution {
            sensor_board[*].sensor.VCC;  // Hierarchical wildcard
        }
    }
}
```

**Completed Work (Phase 1 - Analyzer)**:
- ✅ Extended InstanceRegistry to track module instances with InstanceKind enum
- ✅ Built module definition registry (ModuleContents)
- ✅ Implemented recursive hierarchical expansion logic (expand_hierarchical_wildcard)
- ✅ Added expand_through_module for nested module traversal
- ✅ Integrated with power domain expansion
- ✅ Created comprehensive test infrastructure
- ✅ Identified parser limitation

**Completed Work (Phase 2 - Parser)**:
- ✅ Updated parser grammar to support multi-segment hierarchical paths
- ✅ Extended DistributionPinList AST with path_segments(), is_hierarchical(), and full_path() methods
- ✅ Added support for bare wildcard patterns (.*sensor)
- ✅ Implemented DOT-based path segmentation for correct hierarchical parsing
- ✅ Fixed module vs component detection in BHDL v2.0 syntax
- ✅ Added suffix wildcard matching (*sensor matches temp_sensor, humidity_sensor, etc.)
- ✅ All tests passing with 10/10 hierarchical connections expanded correctly

**Files Modified**:
- `bhdl-parser/src/top_level.rs:1081-1153` - Multi-segment path parsing
- `bhdl-ast/src/items.rs:650-733` - Path segmentation methods
- `bhdl-analyzer/src/passes/instance_registry.rs` - Module tracking, hierarchical expansion, suffix wildcard support
- `bhdl-analyzer/src/passes/power_domain_expansion.rs:140-283` - Hierarchical path handling

**Test Results**: ✅ All tests passing
- `sensor_board[*].sensor.VCC` → 3 connections (array wildcard across modules)
- `sensor_board[*].buffer.VCC` → 3 connections (array wildcard across modules)
- `array.*sensor.VCC` → 3 connections (suffix wildcard inside module)
- Total: 10 hierarchical connections expanded correctly

**Documentation**:
- `CHANGELOG_2025-10-12_HIERARCHICAL_WILDCARD_PROGRESS.md` - Phase 1 implementation details
- `CHANGELOG_2025-10-12_HIERARCHICAL_WILDCARD_COMPLETE.md` - Phase 2 completion (to be created)

## Medium Priority Features

### 5. Advanced Pattern Matching

**Status**: ✅ Complete (Oct 12, 2025)
**Effort**: Completed
**Impact**: Medium

**Goal**: Extend wildcard pattern matching with fine-grained instance selection.

**New Pattern Syntax**:
```bhdl
distribution {
    sensor[even].VCC;          // Only even-numbered instances
    sensor[odd].VCC;           // Only odd-numbered instances
    sensor[0,2,4,8].VCC;       // Specific list
    sensor[0..7:2].VCC;        // Range with step (0, 2, 4, 6)
}
```

**Completed Work (Phase 1 - Parser & AST)**:
- ✅ Added PATTERN_KEYWORD and PATTERN_INDICES syntax kinds
- ✅ Implemented parse_bracket_contents() for keyword/range/list parsing
- ✅ Implemented parse_pattern_range_or_list() helper for stepped ranges and explicit lists
- ✅ Added parse_path_segment() for cleaner path parsing
- ✅ Extended DistributionPinList AST with pattern_type() and pattern_params() methods
- ✅ Created PatternType enum (Wildcard, SimpleRange, SteppedRange, ExplicitList, EvenKeyword, OddKeyword)
- ✅ Created PatternParams struct with pre-computed indices
- ✅ Implemented extract_number_from_expr() and parse_pattern_indices() helpers

**Completed Work (Phase 2 - Analyzer)**:
- ✅ Replaced range expansion logic with pattern matching switch
- ✅ Implemented extract_index_from_name() helper for even/odd filtering
- ✅ Added stepped range expansion logic (SteppedRange pattern)
- ✅ Added explicit list expansion logic (ExplicitList pattern)
- ✅ Added even/odd filtering using wildcard matches and index extraction
- ✅ Integrated pattern matching with existing wildcard and hierarchical expansion

**Files Modified**:
- `bhdl-parser/src/syntax.rs:224-226` - New SyntaxKind tokens
- `bhdl-parser/src/top_level.rs:1130-1209` - Parser grammar for patterns
- `bhdl-ast/src/items.rs:747-882` - AST pattern classification methods
- `bhdl-analyzer/src/passes/power_domain_expansion.rs:9,181-306,252-283` - Pattern expansion logic

**Test Results**: ✅ All tests passing
- Parser test: 7/7 pattern types correctly identified
- End-to-end test: 48 connections expanded correctly
  - Even pattern: 8 connections (indices 0,2,4,6,8,10,12,14)
  - Odd pattern: 8 connections (indices 1,3,5,7,9,11,13,15)
  - Explicit list: 4 connections (indices 0,5,10,15)
  - Stepped range: 6 connections (indices 0,3,6,9,12,15)
  - Simple range: 5 connections (indices 0,1,2,3,4)
  - Single index: 1 connection (index 7)
  - Wildcard: 16 connections (all indices 0-15)

**Test Binaries**:
- `bhdl-analyzer/src/bin/test_advanced_patterns.rs` - Parser pattern classification test
- `bhdl-analyzer/src/bin/test_pattern_expansion.rs` - End-to-end expansion test
- `tests/circuits/realistic/test_advanced_patterns.bhdl` - Test circuit

**Documentation**:
- `docs/implementation/Advanced_Pattern_Matching_Design.md` - Complete design document
- `CHANGELOG_2025-10-12_ADVANCED_PATTERNS_COMPLETE.md` - Implementation details (to be created)

**Benefits**:
- Fine-grained control over instance selection in power domains
- Supports differential pair routing patterns (even/odd)
- Enables selective population of instances
- Phased array and sampling patterns (stepped ranges)
- Backward compatible - all existing patterns still work

### 6. AI-Driven Decoupling Optimization

**Status**: 🚧 Not Started
**Effort**: High (1-2 weeks)
**Impact**: Medium

**Goal**: Automatically calculate optimal decoupling capacitor values and placement.

**Approach**:
- Use component power consumption data
- Apply PDN impedance analysis
- Generate recommendations for:
  - Capacitor values
  - Capacitor count
  - Placement locations
  - Minimize impedance peaks

**Integration Point**: Pass 1.5 (power domain expansion)

### 7. Power Integrity Analysis

**Status**: 🚧 Not Started
**Effort**: High (2-3 weeks)
**Impact**: Medium-High

**Features**:
- IR drop analysis across power distribution network
- Voltage ripple calculation
- Current density validation
- Thermal hotspot detection
- PDN impedance profiling

**New Analyzer Pass**: Pass 1.75 (Power Integrity Analysis)

## Low Priority / Future Enhancements

### 8. Constraint Validation

**Status**: 🚧 Not Started
**Effort**: Low (1-2 days)
**Impact**: Low

**Goal**: Validate power domain constraints specified in BHDL.

**Example**:
```bhdl
power_domain @VCC_3V3 = 3.3V @ 5A {
    constraints {
        max_voltage_drop: 100mV;    // Validate via IR drop analysis
        max_ripple: 50mV;           // Validate via AC analysis
        min_decoupling: 10µF;       // Validate total capacitance
    }
}
```

### 9. Power Sequencing from Domain Specifications

**Status**: 🚧 Not Started
**Effort**: Medium (3-4 days)
**Impact**: Low

**Goal**: Auto-generate power sequencing from domain dependencies.

**Example**:
```bhdl
power_domain @VCC_CORE = 1.0V @ 30A {
    requires: @VCC_AUX;  // Core requires AUX to be up first
}
```

### 10. Enhanced Documentation Generation

**Status**: ✅ Complete (Oct 12, 2025)
**Effort**: Completed
**Impact**: Medium

**Goal**: Generate power domain documentation from BHDL specifications.

**Completed Work**:
- ✅ Voltage domain summary with statistics tables
- ✅ Connection summary per domain with pattern detection
- ✅ Power tree hierarchical visualization (ASCII)
- ✅ BOM generator for decoupling capacitors
- ✅ Power budget analyzer with margin calculations
- ✅ Capacitance value parsing (pF, nF, µF, mF, F)
- ✅ Modular architecture with reusable formatters
- ✅ Complete test binary demonstrating all features

**Output Formats**:
- Markdown (implemented)
- ASCII table support
- HTML (prepared for future)

**Files Created**:
- `bhdl-analyzer/src/documentation/mod.rs` - Main API
- `bhdl-analyzer/src/documentation/context.rs` - Core types
- `bhdl-analyzer/src/documentation/voltage_summary.rs` - Domain statistics
- `bhdl-analyzer/src/documentation/connection_summary.rs` - Connection listings
- `bhdl-analyzer/src/documentation/power_tree.rs` - Tree visualization
- `bhdl-analyzer/src/documentation/bom_generator.rs` - BOM generation
- `bhdl-analyzer/src/documentation/budget_analyzer.rs` - Power budgets
- `bhdl-analyzer/src/documentation/formatters/markdown.rs` - Formatting utilities
- `bhdl-analyzer/src/bin/test_documentation_generation.rs` - Test binary

**Documentation**:
- `CHANGELOG_2025-10-12_DOCUMENTATION_GENERATION_COMPLETE.md` (to be created)

## Architecture Improvements

### 11. Improved Error Messages for Power Domains

**Status**: ✅ Complete (Oct 11, 2025)
**Effort**: Completed
**Impact**: Medium

**Completed Work**:
- ✅ Implemented Levenshtein distance algorithm for fuzzy matching
- ✅ Added `find_similar_base_names()` to InstanceRegistry
- ✅ Created intelligent error message generator with multi-strategy fallback
- ✅ Integrated with wildcard expansion error handling
- ✅ Handles common typo patterns (extra/missing/substituted characters)
- ✅ Provides "Did you mean...?" suggestions with instance lists
- ✅ Falls back to listing available instances when no matches found

**Files Modified**:
- `bhdl-analyzer/src/passes/instance_registry.rs:85-287` - Fuzzy matching methods
- `bhdl-analyzer/src/passes/power_domain_expansion.rs:248-327` - Error message generation

**Documentation**:
- `CHANGELOG_2025-10-11_ERROR_MESSAGES.md` - Complete implementation details

**Example Error Messages**:
```
Error: Wildcard expansion for 'sensors[*]' found no matching instances
  Help: Did you mean 'sensor'? (found 3 instances: sensor_0, sensor_1, sensor_2)
```

### 12. Power Domain Inheritance

**Status**: 🚧 Not Started
**Effort**: Medium (2-3 days)
**Impact**: Low-Medium

**Goal**: Allow power domains to inherit from templates.

**Example**:
```bhdl
power_domain_template StandardCore {
    decoupling {
        near each pin: 100nF @ 1;
        distributed: 10µF @ 4, 1µF @ 8;
    }
}

power_domain @VCC_CORE = 1.0V @ 50A : StandardCore {
    distribution {
        fpga.VCCINT[*];
    }
}
```

## Testing & Quality

### 13. Comprehensive Integration Tests

**Status**: 🟡 Partial
**Effort**: Medium (ongoing)
**Impact**: High

**Needed Tests**:
- [ ] Real FPGA design with 100+ power pins
- [ ] Multi-voltage domain board (1.0V, 1.8V, 3.3V, 5V)
- [ ] Generate block + wildcard integration
- [ ] Hierarchical module + wildcard integration
- [ ] Performance test with 1000+ components

### 14. Fuzzing for Parser Robustness

**Status**: 🚧 Not Started
**Effort**: Medium (1 week)
**Impact**: Medium

**Goal**: Ensure parser handles malformed power_domain blocks gracefully.

## Community & Documentation

### 15. Example Gallery

**Status**: ✅ Complete (Core Examples - Oct 12, 2025)
**Effort**: Completed
**Impact**: High

**Completed Examples**:
- ✅ Simple LED board with power domain (01_simple_led_board.bhdl)
- ✅ Multi-LED board demonstrating wildcards (02_multi_led_wildcards.bhdl)
- ✅ Sensor array with ranges and lists (03_sensor_array.bhdl)
- ✅ FPGA development board with multi-voltage domains (04_fpga_dev_board.bhdl)
- ✅ Comprehensive power demo with all features (comprehensive_power_demo.bhdl)
- ✅ Complete README with learning path and pattern reference

**Remaining Examples (Optional Future Work)**:
- [ ] Motor controller (multiple voltage domains with power sequencing)
- [ ] Server PSU (complex power architecture with redundancy)

## Recommended Priority Order

Based on impact and effort, here's the suggested order:

1. ✅ **Synthesizer Integration** (COMPLETED Oct 11, 2025)
   - Completed the scalability feature end-to-end
   - Enabled visualization of expanded power domains

2. ✅ **Visualizer Support** (COMPLETED Oct 11, 2025)
   - Provides visual feedback for power distribution
   - Helps designers verify power domain specifications

3. ✅ **Generate Block Integration** (COMPLETED Oct 11, 2025)
   - Common use case for repetitive structures
   - Natural extension of wildcard expansion

4. ✅ **Improved Error Messages** (COMPLETED Oct 11, 2025)
   - Quick win for developer experience
   - Low hanging fruit

5. **Hierarchical Wildcard Expansion** (Medium Impact, High Effort) - NEXT PRIORITY
   - Enables wildcards across module boundaries
   - Important for complex hierarchical designs

6. **Advanced Pattern Matching** (Low-Medium Impact, Medium Effort)
   - Nice-to-have for power users
   - Can wait for user demand

Continue with AI optimization and power integrity analysis as longer-term goals.

---

**Note**: This is a living document. Update priorities based on user feedback and project needs.
