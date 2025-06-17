# BHDL Architecture and Design Decisions

## Overview

This document captures key architectural decisions and design patterns for the BHDL (Board Hardware Description Language) toolchain implementation. It serves as a reference for developers and ensures consistency across the codebase.

Last Updated: 2024-12-16

## Table of Contents

1. [Overall Architecture](#overall-architecture)
2. [Parser Design](#parser-design)
3. [AST Structure](#ast-structure)
4. [Analyzer Architecture](#analyzer-architecture)
5. [Netlist Design](#netlist-design)
6. [Component Abstraction Levels](#component-abstraction-levels)
7. [Pin Mapping Strategy](#pin-mapping-strategy)
8. [Database Integration](#database-integration)
9. [Future Enhancements](#future-enhancements)

## Overall Architecture

### Pipeline Structure

```
BHDL Source → Parser → AST → Analyzer → Netlist → Synthesizer → Output
                               ↓           ↓
                         Power Analysis  Component DB
                         Component Inference
```

### Key Design Principles

1. **Separation of Concerns**: Each stage has a clear responsibility
2. **Immutable Data Flow**: Each stage produces new data structures
3. **Semantic Preservation**: Context flows through the entire pipeline
4. **Extensibility**: New analysis passes can be added easily

## Parser Design

### Technology Choice

- **Parser Library**: `rowan` - Provides lossless syntax trees
- **Lexer**: `logos` - Fast, compile-time generated lexer
- **Grammar**: Hand-written recursive descent parser

### Design Decisions

1. **Lossless Parsing**: Preserve all source information including whitespace and comments
2. **Error Recovery**: Continue parsing after errors to provide multiple diagnostics
3. **Incremental Parsing Ready**: Structure supports future incremental parsing

### BHDL v2.0 Support

The parser fully supports BHDL v2.0 flow-based syntax:
- Flow operators: `->`, `<->`, `|>`
- Inline component instantiation: `Cap(100uF).1`
- Named handles: `C1: Cap(100uF)`
- Generate constructs
- Power/ground declarations

## AST Structure

### Node Types Hierarchy

```
SourceFile
├── Board
│   ├── PowerDecl
│   ├── GroundDecl
│   └── ConnectionStmt
├── Module
│   ├── PinDecl
│   └── AttributeDecl
└── ComponentDef
```

### Design Decisions

1. **Typed Wrappers**: Each AST node has a strongly-typed wrapper
2. **Visitor Pattern**: Implemented for tree traversal
3. **HasName Trait**: Common interface for named entities
4. **Immutable**: AST nodes are immutable after creation

## Analyzer Architecture

### Multi-Pass Analysis

1. **Pass 1**: Symbol table construction
2. **Pass 2**: Reference resolution
3. **Pass 3**: Constant evaluation
4. **Pass 4**: Bounds checking
5. **Pass 5**: Power domain analysis
6. **Pass 6**: Component inference
7. **Pass 7**: Power sequencing

### Design Decisions

1. **Pass Independence**: Each pass can run independently
2. **Diagnostic Collection**: Errors don't stop analysis
3. **Context Preservation**: Analysis results flow to synthesis

### Component Inference

**Key Innovation**: Infer component parameters from circuit context

```rust
// Example: LED current limiting resistor
if context.has_led_in_series {
    resistance = (supply_voltage - led_vf) / led_current;
}
```

### Power Analysis

- Tracks power domains through the circuit
- Propagates voltage/current requirements
- Identifies level shifting needs
- Generates power sequencing

## Netlist Design

### Current Structure (Logical Netlist)

```rust
Netlist {
    modules: SlotMap<ModuleId, ModuleDefinition>,
    instances: SlotMap<InstanceId, Instance>,
    nets: SlotMap<NetId, Net>,
}
```

### Planned Pin Connection Enhancement

**Design Decision**: Separate logical and physical representations

#### Logical Pin Representation
```rust
struct Pin {
    id: PinId,
    name: String,              // "IN", "OUT", "GND"
    direction: PinDirection,   // in, out, inout, power, ground
    pin_type: PinType,        // signal, power, ground, clock
}

struct PinInstance {
    id: PinInstanceId,
    pin_def: PinId,           // Reference to pin definition
    instance: InstanceId,     // Parent instance
    net: Option<NetId>,       // Connected net
}
```

#### Physical Representation (Future PNL)
```rust
struct PhysicalPin {
    logical_pin: PinId,
    physical_number: u32,     // Package-specific
    package: PackageId,
    position: Point2D,
}
```

### Connection Model

**Design Decision**: Explicit connections only, no implicit power/ground

```rust
enum ConnectionPoint {
    Pin(PinInstanceId),
    // Future: TestPoint, Via, etc.
}

struct Net {
    id: NetId,
    name: Option<String>,
    connections: Vec<ConnectionPoint>,
    net_class: NetClass,
}
```

### Differential Pairs and Buses

**Design Decision**: First-class support for modern interconnects

```rust
enum NetClass {
    Signal,
    Power(f64),              // With voltage
    Ground,
    DifferentialPair,        // USB D+/D-, LVDS, etc.
    Bus(BusInfo),           // ADDR[0:15], DATA[0:7]
}
```

## Component Abstraction Levels

### Three-Level Hierarchy

**Design Decision**: Support multiple abstraction levels for different use cases

#### 1. High-Level (Requirements)
```bhdl
supply: PowerSupply(5V, 1A, ripple=50mV);
```
- User specifies requirements
- System designs implementation
- Multiple topologies possible

#### 2. Mid-Level (Generic Components)
```bhdl
vreg: LinearRegulator(5V, 1A);
```
- User specifies component type
- System selects specific part
- Hints guide selection

#### 3. Low-Level (Specific Parts)
```bhdl
U1: LM7805(package="TO-220");
```
- User specifies exact part
- Direct database lookup
- Package as parameter

### Component Resolution Flow

```
PowerSupply → Analyzer (topology selection) → LinearRegulator
     ↓                                              ↓
LinearRegulator → Synthesizer (part selection) → LM7805
     ↓                                              ↓
LM7805 → Database lookup → Component with pins/footprint
```

## Pin Mapping Strategy

### Universal Pin Naming

**Design Decision**: Same logical pin names across all packages

```bhdl
module LM7805(package: string = "TO-220") {
    pin IN: power in;      // Same name for all packages
    pin GND: ground;
    pin OUT: power out;
    
    attribute pin_mapping = match package {
        "TO-220" => { IN: 1, GND: 2, OUT: 3 },
        "DPAK" => { IN: 1, GND: 4, OUT: 3 },
    };
}
```

### Benefits

1. **Design Reuse**: Change package without changing connections
2. **Clear Intent**: Logical names convey purpose
3. **Type Safety**: Pin types checked at compile time

## Database Integration

### Component Database Schema

```sql
-- Core component information
CREATE TABLE components (
    id INTEGER PRIMARY KEY,
    name TEXT UNIQUE,
    category TEXT,
    manufacturer TEXT
);

-- Pin definitions
CREATE TABLE pins (
    component_id INTEGER,
    name TEXT,
    number INTEGER,
    type TEXT,
    package TEXT
);

-- Electrical characteristics
CREATE TABLE specifications (
    component_id INTEGER,
    parameter TEXT,
    value TEXT,
    conditions TEXT
);
```

### Caching Strategy

1. **Two-level cache**: In-memory + persistent
2. **Lazy loading**: Load components on demand
3. **Batch operations**: Minimize database queries

## Future Enhancements

### Near Term

1. **Complete Pin Connections**: Implement PinInstance and connection tracking
2. **Netlist Export**: Generate KiCad/Altium netlists
3. **DRC Engine**: Design rule checking on netlist

### Medium Term

1. **Physical Netlist (PNL)**: Package-specific representation
2. **Thermal Analysis**: Power dissipation tracking
3. **Cost Optimization**: BOM cost minimization

### Long Term

1. **Auto-routing Hints**: Topology-aware routing suggestions
2. **Simulation Export**: SPICE netlist generation
3. **Cloud Component DB**: Shared component libraries

## Design Patterns

### 1. Builder Pattern
Used for complex object construction (NetlistGenerator, SymbolTableBuilder)

### 2. Visitor Pattern
Used for AST traversal and analysis passes

### 3. Strategy Pattern
Used for component selection algorithms

### 4. Command Pattern
Future: For undo/redo in interactive tools

## Error Handling

### Principles

1. **Collect, Don't Crash**: Gather multiple errors
2. **Recovery**: Continue processing after errors
3. **Context**: Provide location and fix suggestions
4. **Severity Levels**: Error, Warning, Info

## Testing Strategy

### Unit Tests
- Parser: Grammar rule coverage
- Analyzer: Each pass independently
- Netlist: Data structure operations

### Integration Tests
- End-to-end pipeline tests
- Real circuit examples
- Component database integration

### Property Tests
- Fuzzing for parser robustness
- Invariant checking for netlist

## Performance Considerations

### Current Focus
- Correctness over performance
- Clean architecture over optimization

### Future Optimizations
1. Incremental parsing
2. Parallel analysis passes
3. Database query optimization
4. Memory pool for AST nodes

## Conclusion

This architecture provides a solid foundation for a modern EDA tool while maintaining flexibility for future enhancements. The separation of logical and physical representations, multi-level component abstractions, and explicit connection model ensure the system can grow to meet increasing complexity in electronic design.