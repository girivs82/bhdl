# Unified Data Structure Implementation

## Overview

This document describes the unified data structure approach implemented for BHDL analysis, which allows different analysis passes (SPICE, safety, etc.) to augment the netlist with their specific data without requiring complex conversions.

## Motivation

Previously, the BHDL toolchain had separate data structures for different analysis types:
- Netlist (structural representation)
- SPICE Circuit (electrical representation)
- Analysis results (various formats)

This led to:
- Complex and lossy conversions between formats
- Duplicated logic for component type determination
- Difficulty passing data between analysis stages
- Maintenance challenges

## Architecture

### Core Data Structure

The unified approach extends the existing `AnalysisData` structure in `bhdl-common`:

```rust
pub struct AnalysisData {
    /// Module-level information
    pub module_definitions: HashMap<String, ModuleDefinitionInfo>,
    
    /// Symbol table data
    pub symbol_data: HashMap<String, SymbolInfo>,
    
    /// Per-instance analysis data (NEW)
    pub instance_analysis: HashMap<String, InstanceAnalysisData>,
}

pub struct InstanceAnalysisData {
    /// SPICE component type (resistor, capacitor, etc.)
    pub spice_type: Option<String>,
    
    /// Component role from topology analysis
    pub component_role: Option<String>,
    
    /// Electrical parameters (value, tolerance, ratings)
    pub electrical_params: Option<ElectricalParams>,
    
    /// Safety analysis results
    pub safety_info: Option<SafetyInfo>,
    
    /// Extensible for future analysis types
    pub extensions: HashMap<String, serde_json::Value>,
}
```

### Key Components

#### 1. SpiceAnalysisAugmenter (`bhdl-spice/src/analysis_augmenter.rs`)

Augments the netlist with SPICE-specific information:

```rust
pub struct SpiceAnalysisAugmenter {
    model_extractor: ComponentModelExtractor,
    component_registry: ComponentRegistry,
}

impl SpiceAnalysisAugmenter {
    pub fn augment(&mut self, netlist: &Netlist, analysis_data: &mut AnalysisData) -> Result<()> {
        // Step 1: Determine SPICE types and extract parameters
        for (instance_id, instance) in &netlist.instances {
            let spice_type = self.determine_spice_type(&module.name, &instance.attributes)?;
            let electrical_params = self.extract_electrical_params(instance, module)?;
            
            // Store in unified structure
            analysis_data.instance_analysis
                .entry(instance.name.clone())
                .or_insert_with(InstanceAnalysisData::default)
                .spice_type = Some(spice_type);
        }
        
        // Step 2: Run component role detection
        self.detect_component_roles(netlist, analysis_data)?;
    }
}
```

#### 2. Component Registry (`bhdl-spice/src/component_registry.rs`)

Data-driven component type mapping:

```rust
pub struct ComponentRegistry {
    /// Mapping from component class to metadata
    class_map: HashMap<String, ComponentMetadata>,
    
    /// Mapping from module names to component classes
    module_map: HashMap<String, String>,
}

// Examples:
// "Res" -> "resistor" -> ComponentType::Resistor
// "LM7805" -> "voltage_regulator" -> ComponentType::VoltageRegulator
// "TestPoint" -> "test_point" -> ComponentType::Other("test_point")
```

#### 3. Integration Points

The unified data structure integrates with:

- **Analyzer**: Populates initial module and symbol data
- **SPICE**: Augments with electrical types and parameters
- **Role Detection**: Adds topology-based component roles
- **Safety Analysis**: Can add safety violations and limits
- **Visualizer**: Can use role information for layout
- **CLI**: Accesses unified data for various outputs

## Benefits

### 1. No Lossy Conversions
- Netlist remains the single source of truth
- Analysis data augments without modifying structure
- No information lost in format translations

### 2. Data-Driven Component Types
- Component registry determines types
- No heuristics or string matching
- Consistent type determination across tools

### 3. Extensibility
- New analysis types add their data to `InstanceAnalysisData`
- Extensions map allows arbitrary data
- No need to modify core structures

### 4. Better Separation of Concerns
- Netlist: Pure structural representation
- AnalysisData: All analysis-specific information
- Clear boundaries between domains

## Usage Example

```rust
// Load netlist
let netlist = load_netlist("circuit.json")?;

// Create analysis data
let mut analysis_data = AnalysisData::default();

// Augment with SPICE information
let mut augmenter = SpiceAnalysisAugmenter::new();
augmenter.augment(&netlist, &mut analysis_data)?;

// Access unified data
for (instance_name, data) in &analysis_data.instance_analysis {
    println!("{}: {} ({})", 
        instance_name,
        data.spice_type.as_ref().unwrap_or(&"unknown".to_string()),
        data.component_role.as_ref().unwrap_or(&"unclassified".to_string())
    );
}
```

## Migration Guide

### Old Approach
```rust
// Complex conversion
let circuit = Circuit::from_netlist(&netlist)?;
let roles = detect_roles(&circuit);
// Data scattered across different structures
```

### New Approach
```rust
// Unified augmentation
let mut analysis_data = AnalysisData::default();
augmenter.augment(&netlist, &mut analysis_data)?;
// All data in one place
```

## Future Extensions

The unified structure supports future enhancements:

1. **Thermal Analysis**: Add thermal parameters to `InstanceAnalysisData`
2. **EMC Analysis**: Add electromagnetic compatibility data
3. **Cost Analysis**: Add component pricing and availability
4. **Reliability**: Add MTBF and failure mode data

Each new analysis type simply extends the data structure without affecting existing code.

## Implementation Status

✅ Completed:
- Extended AnalysisData with instance analysis
- Created SpiceAnalysisAugmenter
- Integrated component role detection
- Updated CLI to use unified approach
- Removed heuristic-based type detection

🔄 In Progress:
- Safety analysis integration
- Visualizer integration for role-based layout

📋 Future:
- Pin metadata integration
- Thermal analysis support
- Component database integration