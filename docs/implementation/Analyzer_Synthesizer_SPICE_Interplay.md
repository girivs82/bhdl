# Analyzer-Synthesizer-SPICE Interplay in BHDL

## Overview

The BHDL toolchain consists of three major components that work together to analyze, synthesize, and validate electronic circuits:

1. **Analyzer** - Semantic analysis and component inference
2. **Synthesizer** - Netlist generation and component mapping  
3. **SPICE** - Electrical analysis and validation

## Architecture Flow

```
BHDL Source → Parser → AST → Analyzer → Synthesizer → Netlist
                              ↓            ↓
                           SPICE Models  Component DB
                              ↓            ↓
                        Electrical Analysis & Validation
```

## Component Interactions

### 1. Analyzer (bhdl-analyzer)

The analyzer performs multi-pass semantic analysis:

**Pass 1: Symbol Collection**
- Builds symbol tables for components, nets, and power domains
- Creates separate namespaces for nets (@NETNAME) vs component handles

**Pass 2: Reference Resolution**
- Resolves component references and net connections
- Type checking for pins and signals

**Pass 3: Constant Evaluation**
- Evaluates constant expressions (e.g., 10k, 4.7µF)
- Stores resolved values for later passes

**Pass 4: Bounds Checking**
- Validates array indices and bus widths
- Checks parameter constraints

**Pass 5: Power Analysis**
- Traces power flow through the circuit
- Creates power domain hierarchy
- Detects components needing power connections

**Pass 6: Component Inference**
- Uses electrical models to infer component parameters
- Integrates with bhdl-stdlib for component specifications
- Detects missing components (e.g., current limiting resistors)

**Pass 7: Power Sequencing**
- Determines power-up/down sequences
- Validates timing constraints

### 2. Synthesizer (bhdl-synthesizer)

The synthesizer converts semantic analysis results to structural netlists:

**Key Features:**
- Preserves semantic context for visualization
- Maps to real component database (KiCad symbols)
- Handles net naming and implicit handle creation
- Generates reference designators (R1, C1, etc.)

**Integration Points:**
```rust
// Uses analysis results for intelligent synthesis
let netlist = generator.generate_from_ast_and_analysis(&ast, &analysis).await?;

// Preserves power domain information
if config.include_power_domains {
    synthesizer.include_power_domain_info(&analysis)?;
}

// Includes component inference results
if config.include_component_inference {
    synthesizer.include_component_inference_info(&analysis)?;
}
```

### 3. SPICE Integration (bhdl-spice)

The SPICE module provides electrical analysis capabilities:

**Core Components:**
- **Circuit**: Graph representation of electrical network
- **ComponentModel**: Electrical models (resistor, LED, diode, etc.)
- **NonlinearDcAnalysis**: Newton-Raphson solver for DC analysis
- **ComponentInference**: Detects missing/incorrect components

**Example LED Current Limiting Detection:**
```rust
// SPICE detects overcurrent in LED without resistor
let mut inference = ComponentInference::new(circuit);
inference.add_model("D1", ComponentModel::LED {
    forward_voltage: 2.0,
    forward_current: 0.020,
    max_current: Some(0.030),
    ...
});

let inferred = inference.infer()?;
// Returns: Resistor needed between VCC and LED
```

## Data Flow Examples

### Example 1: LED Safety Check

1. **Parser**: Parses `VCC -> led: LED("red").A;`
2. **Analyzer**: 
   - Pass 5: Traces power from VCC to LED
   - Pass 6: Checks if current limiting exists
   - Creates ComponentSuggestion if resistor needed
3. **Synthesizer**: 
   - Generates netlist with LED connected to VCC
   - Includes inference warning in metadata
4. **SPICE** (if invoked):
   - Runs DC analysis
   - Detects overcurrent condition
   - Suggests appropriate resistor value

### Example 2: Power Domain Analysis

1. **Parser**: Parses power declarations and connections
2. **Analyzer**:
   - Pass 5: Builds PowerAnalysisContext
   - Creates domain hierarchy (VCC → @FILTERED → components)
   - Tracks voltage/current for each domain
3. **Synthesizer**:
   - Maps power domains to netlist nets
   - Assigns components to appropriate domains
4. **SPICE**:
   - Validates power distribution
   - Checks for voltage drops and current limits

### Example 3: Net Naming with @Syntax

1. **Parser**: Recognizes @NETNAME syntax
2. **Analyzer**:
   - Pass 1: Creates net symbols in separate namespace
   - Pass 2: Resolves net references
3. **Synthesizer**:
   - Creates named nets in netlist
   - Handles implicit connections via net references

## Key Data Structures

### Analyzer
```rust
pub struct AnalysisResult {
    pub power_analysis: PowerAnalysisContext,
    pub component_inference: ComponentInferenceContext,
    pub resolved_constants: HashMap<SyntaxNodePtr, i64>,
    // ... other fields
}

pub struct PowerAnalysisContext {
    pub domains: HashMap<String, PowerDomain>,
    pub component_domains: HashMap<String, String>,
    // ... other fields
}
```

### Synthesizer
```rust
pub struct NetlistGenerator {
    netlist: Netlist,
    ast_to_net: HashMap<String, NetId>,
    net_assignment_handles: HashMap<String, InstanceId>,
    // ... other fields  
}
```

### SPICE
```rust
pub struct Circuit {
    nodes: SlotMap<NodeId, Node>,
    branches: SlotMap<EdgeId, Branch>,
}

pub enum ComponentModel {
    Resistor { resistance: f64, tolerance: f64, limits: ElectricalLimits },
    LED { forward_voltage: f64, forward_current: f64, ... },
    // ... other models
}
```

## Future Enhancements

1. **Real-time Analysis**
   - Run SPICE analysis during editing
   - Provide immediate feedback on circuit issues

2. **Thermal Analysis**
   - Integrate thermal models
   - Check power dissipation limits

3. **EMC/EMI Analysis**
   - Signal integrity checks
   - Radiation/susceptibility analysis

4. **Cost Optimization**
   - Component selection based on cost
   - BOM optimization

5. **Manufacturing Constraints**
   - PCB design rule checking
   - Assembly process validation