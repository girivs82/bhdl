# Intelligent SPICE Engine Architecture

## Overview

This document describes a revolutionary architecture for SPICE simulation that separates **circuit understanding** from **numerical solving**. The key insight is that the SPICE engine should contain the intelligence about circuit patterns and solving strategies, while the numerical solver (Two-Phase) remains simple and fast.

## Motivation

Traditional SPICE solvers struggle with certain circuit patterns:
- Series identical nonlinear elements (multiple solutions)
- Parallel current-sharing devices (convergence issues)
- Circuits with multiple stable states
- High-gain feedback loops

The root cause: solvers try to handle all complexity in one monolithic algorithm.

## Core Architectural Principle

```
Intelligence in SPICE Engine + Simple Fast Solver = Robust Simulation
```

Instead of:
- SPICE Engine: "Here's a circuit with 3 LEDs in series, solve it"
- Solver: *struggles with multiple nonlinear elements*

We have:
- SPICE Engine: "I recognize this pattern. Let me decompose it intelligently..."
- SPICE Engine: "Solver, try this simpler subproblem first"
- Solver: "Easy! Here's the solution"
- SPICE Engine: "Now build on that for the next subproblem..."

## Architecture Components

### 1. Topology Analyzer
Identifies problematic circuit patterns:
- Series LEDs/diodes (multiple solutions)
- Parallel MOSFETs/BJTs (current sharing)
- Bridge rectifiers (state-dependent topology)
- Switching converters (averaged models needed)
- High-gain feedback (numerical instability)

#### Enhanced by Synthesizer Information
The synthesizer provides high-level structural information that pure netlist analysis misses:

```rust
// From synthesizer's hierarchical view
pub struct SynthesizerContext {
    // Module boundaries and hierarchy
    pub module_instances: HashMap<InstanceId, ModuleInfo>,
    
    // Flow paths with semantic meaning
    pub flow_paths: Vec<FlowPath>,
    
    // Component roles from instantiation context
    pub component_roles: HashMap<ComponentId, SemanticRole>,
    
    // Net purposes from declarations
    pub net_attributes: HashMap<NetId, NetAttribute>,
}

// Example: Recognizing a power supply topology
impl TopologyAnalyzer {
    fn identify_with_context(&self, 
        circuit: &Circuit,
        context: &SynthesizerContext
    ) -> Vec<CircuitPattern> {
        let mut patterns = vec![];
        
        // Check module names for hints
        for (id, module) in &context.module_instances {
            match module.name.as_str() {
                "BuckConverter" | "StepDown" => {
                    patterns.push(CircuitPattern::SwitchingConverter {
                        topology: ConverterType::Buck,
                        components: self.find_module_components(id),
                    });
                },
                "H_Bridge" | "MotorDriver" => {
                    patterns.push(CircuitPattern::HBridge {
                        switches: self.find_switches_in_module(id),
                    });
                },
                _ => {}
            }
        }
        
        // Use flow information to find series chains
        for flow in &context.flow_paths {
            if let Some(intent) = &flow.intent {
                if intent.name == "sequential_indication" {
                    // This flow is meant to be sequential
                    let leds = self.find_leds_in_flow(flow);
                    if leds.len() > 1 {
                        patterns.push(CircuitPattern::SeriesLEDs {
                            count: leds.len(),
                            order_matters: true,
                        });
                    }
                }
            }
        }
        
        patterns
    }
}
```

### 2. Strategy Library
Domain-specific solving strategies:
- **Progressive Turn-On**: For series nonlinear elements
- **Current Distribution**: For parallel devices
- **State Enumeration**: For switching circuits
- **Gain Scheduling**: For high-gain loops
- **Symmetry Breaking**: For identical components

### 3. Orchestration Layer
Manages strategy execution:
- Sequential with fallback
- Parallel racing
- Ensemble voting
- Hierarchical refinement

### 4. Two-Phase Solver
Remains simple and focused on well-conditioned problems.

## Integration with Designer Intent

BHDL's intent system provides crucial information:

```bhdl
// Designer intent helps strategy selection
led_string: @VCC -> R1(330Ω).1 -> LED1(red).A -> LED2(red).A -> LED3(red).A -> @GND
    for sequential_indication(order: "left-to-right");
```

Intent-aware strategies:
1. **Sequential indication** → Progressive turn-on respecting sequence
2. **Current_sharing** → Equal distribution strategy
3. **Matched_pair** → Symmetry-preserving strategy
4. **Power_stage** → State-space averaging

## Implementation Plan

### Phase 1: Foundation
- [ ] Define Strategy interface
- [ ] Implement CircuitPattern types
- [ ] Create TopologyAnalyzer base class
- [ ] Integrate with existing TwoPhaseSolver

### Phase 2: Series LED Pattern
- [ ] Implement SeriesLEDMatcher
- [ ] Create ProgressiveTurnOnStrategy
- [ ] Add symmetry breaking logic
- [ ] Test on 2-10 LED chains

### Phase 3: Intent Integration
- [ ] Parse intent annotations from synthesizer
- [ ] Map intents to strategy hints
- [ ] Use intent for initial guess generation
- [ ] Implement intent-aware pattern matching

### Phase 4: Additional Patterns
- [ ] Parallel device matcher and strategy
- [ ] Bridge rectifier pattern
- [ ] Feedback loop detection
- [ ] Switching converter patterns

### Phase 5: Intelligence Features
- [ ] Strategy performance tracking
- [ ] Automatic parameter tuning
- [ ] Learning from convergence history
- [ ] User hint system

## Example: Series LED Strategy

```rust
// SPICE engine recognizes the pattern
let pattern = analyzer.identify_pattern(&circuit);
match pattern {
    CircuitPattern::SeriesLEDs { count, identical } => {
        let strategy = ProgressiveTurnOnStrategy::new();
        
        // Phase 1: All LEDs off (high resistance)
        let circuit_1 = circuit.with_leds_as_resistors(1e9);
        let solution_1 = solver.solve(&circuit_1)?;
        
        // Phase 2: First LED on, others off
        let circuit_2 = circuit.with_led_states([ON, OFF, OFF]);
        let solution_2 = solver.solve_with_guess(&circuit_2, solution_1)?;
        
        // Phase 3: First two LEDs on
        let circuit_3 = circuit.with_led_states([ON, ON, OFF]);
        let solution_3 = solver.solve_with_guess(&circuit_3, solution_2)?;
        
        // Continue progressively...
    }
}
```

## Benefits Over Traditional Approach

1. **Robustness**: Each subproblem is well-conditioned
2. **Performance**: Solver operates in its sweet spot
3. **Debuggability**: Clear strategy trace
4. **Extensibility**: New patterns don't require solver changes
5. **User Control**: Can guide strategy selection
6. **Intent Integration**: Leverages designer knowledge

## Technical Details

### Pattern Recognition Algorithm
```rust
trait PatternMatcher {
    fn identify(&self, circuit: &Circuit) -> Option<CircuitPattern>;
    fn confidence(&self) -> f64;
    fn severity(&self) -> Severity;
}
```

### Strategy Selection Heuristics
1. Pattern confidence score
2. Historical success rate
3. Problem severity
4. User preferences
5. Designer intent

### Parallel Execution Model
```rust
async fn solve_parallel(strategies: Vec<Strategy>) -> Solution {
    let handles = strategies.into_iter()
        .map(|s| tokio::spawn(s.solve()))
        .collect();
    
    // First successful solution wins
    tokio::select! { ... }
}
```

## Integration with BHDL Intent System

The synthesizer provides rich intent information through the flow tracking system:

### Intent Flow from Parser to SPICE

```rust
// 1. Parser captures intent on flow statements
protection: sensor -> tvs: TVSDiode(6V).cathode -> tvs.anode -> @GND
    for input_protection(overvoltage: 6V, current_limit: 5mA);

// 2. Flow tracker identifies components in path
FlowPath {
    components: ["sensor", "tvs"],
    nets: ["protection"],
    intent: IntentCall { 
        name: "input_protection",
        params: {"overvoltage": "6V", "current_limit": "5mA"}
    }
}

// 3. SPICE engine receives intent-annotated netlist
pub struct IntentAwareNetlist {
    pub netlist: Netlist,
    pub flow_intents: Vec<FlowIntent>,
    pub component_roles: HashMap<ComponentId, IntentRole>,
}
```

### Intent Categories and Strategy Mapping

#### Protection Intents
- `input_protection(overvoltage, current_limit)` → TVS-aware solving with clamping behavior
- `overvoltage_clamp(voltage)` → Zener/TVS breakdown modeling
- `current_limiting(max_current)` → Progressive current limiting strategy

#### Signal Processing Intents
- `noise_filtering(cutoff_freq)` → Frequency-aware initial conditions
- `signal_amplification(gain)` → Gain scheduling for high-gain circuits
- `level_shifting(from, to)` → Voltage translation aware solving

#### Power/Timing Intents
- `soft_start(ramp_time)` → Progressive power-up strategy
- `delay(time)` → Time-constant aware initialization
- `current_sharing()` → Equal distribution strategy for parallel devices
- `matched_pair()` → Symmetric solution enforcement

#### Digital/Mixed-Signal Intents
- `signal_buffering()` → Digital threshold-aware solving
- `fault_detection(threshold)` → Comparator-like behavior
- `edge_detection(type)` → Transition-sensitive solving

### How Intent Helps Strategy Selection

```rust
impl IntelligentSpiceEngine {
    fn select_strategy_with_intent(&self, 
        pattern: &CircuitPattern,
        intent: &FlowIntent
    ) -> Box<dyn SolvingStrategy> {
        match (pattern, intent.intent_type) {
            // Series LEDs with sequential indication
            (SeriesLEDs(n), "sequential_indication") => {
                Box::new(SequentialLEDStrategy {
                    order: intent.params.get("order"),
                    timing: intent.params.get("delay_between"),
                })
            },
            
            // Parallel MOSFETs with current sharing
            (ParallelDevices(devs), "current_sharing") => {
                Box::new(EqualCurrentStrategy {
                    tolerance: intent.params.get("tolerance").unwrap_or(0.1),
                    thermal_coupling: intent.params.get("thermal").is_some(),
                })
            },
            
            // Protection circuit with clamping
            (ProtectionCircuit, "input_protection") => {
                Box::new(ClampingStrategy {
                    clamp_voltage: intent.params.get("overvoltage"),
                    current_limit: intent.params.get("current_limit"),
                })
            },
            
            // Default fallback
            _ => self.default_strategy_for_pattern(pattern)
        }
    }
}
```

### Intent-Aware Initial Guess Generation

```rust
// Use intent parameters to generate better initial guesses
fn generate_initial_guess(&self, intent: &FlowIntent) -> InitialGuess {
    match intent.intent_type.as_str() {
        "soft_start" => {
            // Start with low voltages, ramp up
            InitialGuess::Ramped { 
                start: 0.0, 
                end: self.nominal_voltage,
                stages: 10 
            }
        },
        "matched_pair" => {
            // Enforce symmetry from the start
            InitialGuess::Symmetric
        },
        "current_limiting" => {
            // Start at current limit
            let i_limit = intent.params.get("max_current");
            InitialGuess::CurrentLimited(i_limit)
        },
        _ => InitialGuess::Default
    }
}
```

### Benefits of Intent Integration

1. **Better Initial Guesses**: Intent parameters provide expected operating points
2. **Smarter Strategy Selection**: Intent disambiguates multiple valid approaches
3. **Faster Convergence**: Strategies can use intent hints to avoid bad regions
4. **User Guidance**: Designers explicitly state expectations
5. **Debugging Aid**: Intent violations are clearly reported

## Future Enhancements

1. **Machine Learning**: Learn optimal strategies from circuit database
2. **Cloud Strategies**: Offload complex patterns to specialized solvers
3. **Interactive Mode**: User can guide strategy selection in real-time
4. **Formal Verification**: Prove solution uniqueness/stability

## Concrete Example: LED Status Indicator Chain

Let's walk through how the intelligent SPICE engine handles a real circuit:

### BHDL Source
```bhdl
board StatusPanel {
    power VCC = 5V @ 500mA;
    ground GND;
    
    // Status indicator chain
    status_chain: @VCC -> R_limit(470Ω).1 -> 
        PowerLED: LED(green).A -> PowerLED.K ->
        StatusLED: LED(yellow).A -> StatusLED.K ->
        ErrorLED: LED(red).A -> ErrorLED.K -> @GND
        for sequential_indication(
            order: "power-status-error",
            delay_between: 100ms
        );
}
```

### Step 1: Synthesizer Provides Context
```rust
SynthesizerContext {
    flow_paths: [
        FlowPath {
            id: 1,
            components: ["R_limit", "PowerLED", "StatusLED", "ErrorLED"],
            nets: ["status_chain"],
            intent: Some(IntentCall {
                name: "sequential_indication",
                params: {
                    "order": "power-status-error",
                    "delay_between": "100ms"
                }
            })
        }
    ],
    component_roles: {
        "PowerLED": SemanticRole::Indicator,
        "StatusLED": SemanticRole::Indicator,
        "ErrorLED": SemanticRole::Indicator,
    }
}
```

### Step 2: Topology Analyzer Identifies Pattern
```rust
patterns = analyzer.identify_with_context(&circuit, &context);
// Returns: CircuitPattern::SeriesLEDs {
//     count: 3,
//     order_matters: true,
//     sequence: ["PowerLED", "StatusLED", "ErrorLED"]
// }
```

### Step 3: Strategy Selection Based on Intent
```rust
strategy = engine.select_strategy_with_intent(&pattern, &intent);
// Returns: SequentialLEDStrategy {
//     sequence: ["PowerLED", "StatusLED", "ErrorLED"],
//     respect_order: true
// }
```

### Step 4: Progressive Solving Execution
```rust
// Stage 1: All LEDs off (verify basic connectivity)
circuit_stage1 = circuit.with_components({
    "PowerLED": Resistor(1e9),
    "StatusLED": Resistor(1e9),
    "ErrorLED": Resistor(1e9)
});
solution1 = solver.solve(&circuit_stage1); // Easy linear solve

// Stage 2: Power LED on (green)
circuit_stage2 = circuit.with_components({
    "PowerLED": LED(green),
    "StatusLED": Resistor(1e9),
    "ErrorLED": Resistor(1e9)
});
solution2 = solver.solve_with_guess(&circuit_stage2, solution1);

// Stage 3: Power + Status LEDs on
circuit_stage3 = circuit.with_components({
    "PowerLED": LED(green),
    "StatusLED": LED(yellow),
    "ErrorLED": Resistor(1e9)
});
solution3 = solver.solve_with_guess(&circuit_stage3, solution2);

// Stage 4: All LEDs on
final_solution = solver.solve_with_guess(&original_circuit, solution3);
```

### Step 5: Validation Against Intent
```rust
// Verify the solution matches intent expectations
validator.check_intent_compliance(&final_solution, &intent);
// - Check each LED has appropriate forward voltage
// - Verify current is within LED ratings
// - Confirm order matches specified sequence
```

### Benefits Demonstrated

1. **No Convergence Issues**: Each stage is well-conditioned
2. **Respects Intent**: Solution follows designer's specified order
3. **Fast Solution**: Progressive approach avoids difficult nonlinear regions
4. **Clear Debugging**: Can trace which stage failed if issues arise
5. **Semantic Understanding**: Uses component roles and flow meaning

## Conclusion

This architecture represents a paradigm shift in circuit simulation:
- **Traditional**: Make solver smarter to handle everything
- **Our Approach**: Make SPICE engine smart about problems, keep solver simple

The result is a more robust, faster, and more maintainable simulation system that leverages both domain knowledge and designer intent.