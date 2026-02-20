# Unified Data Model Implementation

## Overview

This document describes the unified data model approach implemented in BHDL, which eliminates lossy conversions between different parts of the toolchain by augmenting the netlist with analysis data rather than converting between incompatible structures.

## Problem Statement

Previously, the BHDL toolchain had several issues:
1. **Multiple PinMetadata structures** - Different crates defined their own incompatible versions
2. **Lossy conversions** - Converting between analyzer results and SPICE structures lost information
3. **Hardcoded component data** - Component-specific information was hardcoded in analysis tools
4. **Complex data flow** - No clear path for metadata to flow through the pipeline

## Solution: Unified Data Model

### Core Principle
Instead of converting between different data structures, we augment the netlist with analysis data that flows through the entire pipeline unchanged.

### Architecture

```
┌─────────────┐     ┌──────────────┐     ┌──────────────┐     ┌─────────────┐
│   stdlib    │ --> │   Analyzer   │ --> │ Synthesizer  │ --> │    SPICE    │
│ (Pin Meta)  │     │ (Extracts)   │     │ (Augments)   │     │   (Uses)    │
└─────────────┘     └──────────────┘     └──────────────┘     └─────────────┘
                           │                     │                     │
                           v                     v                     v
                    AnalysisResult         Netlist with          Component
                    with symbols          AnalysisData         Role Detection
```

### Key Components

#### 1. Unified PinMetadata Structure (bhdl-common)
```rust
pub struct PinMetadata {
    pub direction: PinDirection,
    pub pin_type: PinType,
    pub function: Option<PinFunction>,
    pub electrical: PinElectricalData,
    pub electrical_specs: HashMap<String, String>,
    pub documentation: Option<String>,
}
```

This single structure serves all parts of the toolchain, eliminating conversions.

#### 2. Netlist Augmentation
```rust
pub struct Netlist {
    // ... existing fields ...
    pub analysis_data: Option<AnalysisData>,
}
```

The netlist now carries analysis metadata alongside structural information.

#### 3. AnalysisData Structure
```rust
pub struct AnalysisData {
    pub module_definitions: HashMap<String, ModuleDefinitionInfo>,
    pub symbol_data: HashMap<String, SymbolInfo>,
    pub instance_analysis: HashMap<String, InstanceAnalysisData>,
}
```

This structure contains all semantic information extracted during analysis.

## Implementation Details

### 1. Symbol Table Access
Made the analyzer's `symbol_table` module public to allow proper extraction of symbol information:
```rust
// In bhdl-analyzer/src/lib.rs
pub mod symbol_table;  // Was: mod symbol_table;
```

### 2. Analysis Data Population
The synthesizer extracts symbol information from the analyzer and populates the netlist:
```rust
fn populate_analysis_data(&mut self, analysis: &AnalysisResult) -> Result<()> {
    // Extract symbols from global scope
    // Extract module definitions from definition scopes
    // Convert to unified AnalysisData format
    // Set in netlist for downstream use
}
```

### 3. SPICE Integration
SPICE component role detection now retrieves metadata directly from the netlist:
```rust
pub fn with_ast_metadata(circuit: Circuit, netlist: &Netlist, ...) -> Self {
    let analysis_data = netlist.get_analysis_data();
    // Use analysis data for enhanced role detection
}
```

### 4. Component Registry Integration
All component-specific information comes from stdlib definitions:
- No hardcoded pin functions in SPICE
- Component pin metadata flows from BHDL entity definitions
- Database population from analysis data, not hardcoded values

## Benefits

1. **No Information Loss** - All metadata flows through unchanged
2. **Single Source of Truth** - One PinMetadata structure used everywhere
3. **Extensibility** - Easy to add new metadata without breaking compatibility
4. **Cleaner Architecture** - Clear data flow through the pipeline
5. **Data-Driven** - Component information comes from stdlib, not hardcoded

## Migration Guide

### For Existing Code
1. Replace all local PinMetadata structures with `bhdl_common::pin_metadata::PinMetadata`
2. Update imports to use the common types
3. Remove conversion functions between different metadata formats
4. Access analysis data from netlist instead of separate parameters

### For New Features
1. Add new fields to AnalysisData if needed
2. Populate them in the synthesizer's `populate_analysis_data` method
3. Access them downstream via `netlist.get_analysis_data()`

## Testing

The unified model is validated through:
1. End-to-end pipeline tests showing data flow
2. Component role detection tests confirming metadata usage
3. Compilation success across all crates

## Future Enhancements

1. **Richer Metadata** - Add more component characteristics to AnalysisData
2. **Visualization Integration** - Use analysis data for intelligent component placement
3. **Optimization Hints** - Pass layout and routing hints through the unified model
4. **Cross-Reference Data** - Include symbol-to-instance mappings for debugging