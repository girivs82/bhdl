# Simulation Infrastructure Implementation

## Overview

This document describes the implementation of the BHDL simulation infrastructure, which provides a unified framework for mixed-signal, digital, and behavioral simulation. The infrastructure supports intent-based simulation mode selection and seamless integration between different simulation domains.

## Architecture

### Core Components

1. **SimulationCoordinator** (`coordinator.rs`)
   - Manages circuit partitioning based on simulation modes
   - Identifies domain interfaces between partitions
   - Orchestrates overall simulation execution

2. **Domain Interface Converters** (`integration/converters/`)
   - **ADConverter**: Analog-to-Digital conversion with hysteresis
   - **DAConverter**: Digital-to-Analog conversion with slew rate limiting
   - **Synchronizer**: Clock domain crossing synchronization

3. **Engine Adapters** (`integration/adapters/`)
   - **SpiceAdapter**: Integrates bhdl-spice for analog simulation
   - **DigitalAdapter**: Event-driven digital simulation
   - **MixedSignalAdapter**: Coordinates between domains

4. **Expression Evaluation** (`evaluation/`)
   - **ExpressionParser**: Parses behavioral expressions from text
   - **SimulationAttributeEvaluator**: Evaluates time-dependent attributes
   - **WhenBlockProcessor**: Handles conditional behavioral updates

5. **Mixed-Signal Synchronization** (`integration/synchronizer.rs`)
   - Time-step coordination between analog and digital domains
   - Event-driven and adaptive synchronization strategies
   - Performance optimization for efficiency

## Implementation Details

### Phase 1: Domain Interface Converters

#### A/D Converter
```rust
pub struct ADConverter {
    config: ADCConfig,
    input_net: NetId,
    output_net: NetId,
    last_output: LogicLevel,
    last_voltage: f64,
    // Metastability detection
    voltage_stable_since: f64,
    transitions: usize,
}
```

Features:
- Configurable voltage thresholds with hysteresis
- Metastability detection and handling
- Propagation delay modeling
- Transition counting for debugging

#### D/A Converter
```rust
pub struct DAConverter {
    config: DACConfig,
    input_net: NetId,
    output_net: NetId,
    current_voltage: f64,
    target_voltage: f64,
    // Slew rate limiting
    in_transition: bool,
    max_observed_slew_rate: f64,
}
```

Features:
- Slew rate limiting for realistic transitions
- Exponential RC-like voltage transitions
- Rise/fall time asymmetry support
- Performance metrics tracking

### Phase 2: SPICE Integration

The SpiceAdapter provides seamless integration with the bhdl-spice engine:

```rust
pub struct SpiceAdapter {
    circuit: Circuit,
    engine: Option<SimulationEngine>,
    dc_analyzer: Option<NonlinearDcAnalysis>,
    net_to_node: HashMap<NetId, NodeId>,
    node_to_net: HashMap<NodeId, NetId>,
    boundary_sources: HashMap<NetId, ComponentId>,
}
```

Key capabilities:
- Automatic netlist conversion from BHDL to SPICE format
- DC operating point analysis
- Boundary value injection through voltage sources
- Bidirectional net mapping for result extraction

### Phase 3: Expression Evaluation

The expression evaluation system enables behavioral modeling:

```rust
// Expression parsing
let expr = expression_parser.parse("5.0 * sin(2 * pi * t)")?;

// Evaluation with simulation context
let sim_context = SimulationEvaluationContext::new(&circuit_state, &time_manager);
let result = ExpressionEvaluator::evaluate(&expr, &eval_context)?;
```

Supported features:
- Full arithmetic and logical operators
- Mathematical functions (sin, cos, exp, log, etc.)
- Time-dependent variables (t, dt)
- Attribute and pin value references
- Conditional (ternary) expressions

### Phase 4: Mixed-Signal Synchronization

The synchronization system ensures accurate time coordination:

```rust
pub enum SyncStrategy {
    LockStep,      // Regular interval synchronization
    EventDriven,   // Sync only at interface events  
    Adaptive,      // Dynamic strategy based on activity
}
```

Synchronization algorithm:
1. Monitor interface nets for value changes
2. Track digital events and analog thresholds
3. Schedule sync points based on strategy
4. Exchange values between domains at sync
5. Maintain minimum/maximum sync intervals

## Usage Example

```rust
// Create simulation coordinator
let coordinator = SimulationCoordinator::new(netlist, flow_tracker);

// Configure simulation context
let context = SimulationContext {
    start_time: 0.0,
    end_time: 1e-3,
    time_step: 1e-9,
    debug: true,
};

// Run simulation
let result = coordinator.simulate(&context)?;
```

## Performance Considerations

1. **Event Queue Optimization**: Uses BTreeSet for O(log n) event scheduling
2. **Lazy Synchronization**: Adaptive strategy reduces unnecessary syncs
3. **Expression Caching**: Parsed expressions are cached for reuse
4. **Parallel Potential**: Architecture supports future parallel execution

## Testing

Comprehensive test programs demonstrate each component:
- `test_domain_converters`: A/D and D/A converter behavior
- `test_spice_integration`: SPICE engine integration
- `test_expression_evaluation`: Behavioral expression evaluation
- `test_mixed_signal_sync`: Synchronization strategies

## Future Enhancements

1. **Parallel Execution**: Run non-interfacing partitions in parallel
2. **Advanced Synchronization**: Predictive synchronization using derivative estimation
3. **Waveform Compression**: Efficient storage of simulation results
4. **Interactive Debugging**: Breakpoint and single-step capabilities

## Integration with BHDL Toolchain

The simulation infrastructure integrates with:
- **bhdl-analyzer**: Uses flow tracking and intent results
- **bhdl-netlist**: Operates on synthesized netlists
- **bhdl-spice**: Leverages analog simulation capabilities
- **bhdl-stdlib**: Uses component parameters and intents

This unified approach ensures consistent behavior across the entire BHDL toolchain.