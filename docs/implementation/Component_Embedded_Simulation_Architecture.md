# Component-Embedded Simulation Architecture

## Overview

This document defines the architecture for embedding simulation models, optimization strategies, and design knowledge directly within BHDL component definitions. Each component becomes a self-contained expert system that knows how to simulate itself at various abstraction levels.

## Core Principles

1. **Single Source of Truth**: All simulation models for a component live within that component's definition
2. **Encapsulation**: Components own their simulation behavior, models, and optimization strategies
3. **Hierarchical Abstraction**: Multiple simulation models at different accuracy/speed trade-offs
4. **Intelligent Guidance**: Components guide the synthesizer on how to simulate and optimize them
5. **Reusability**: Base components provide common models that derived components can inherit

## Architecture Components

### 1. Simulation Models (`@behavioral_model`)

Components can define multiple behavioral models at different abstraction levels:

```bhdl
@behavioral_model <model_name> {
    model_type: string,           // "averaged", "small_signal", "switching", etc.
    valid_range: { ... },         // Conditions where model is valid
    accuracy: { ... },            // Expected accuracy metrics
    equations: { ... },           // Model equations or state matrices
    provides: { ... },            // What outputs this model can generate
    use_when: string,             // Conditions for using this model
}
```

### 2. Simulation Requirements (`@simulation_requirements`)

Components declare what analyses they need:

```bhdl
@simulation_requirements {
    dc_analysis: { ... },         // DC operating point requirements
    ac_analysis: { ... },         // AC frequency response requirements  
    transient: { ... },           // Time-domain requirements
    noise: { ... },               // Noise analysis requirements
}
```

### 3. Simulation Strategy (`@simulation_strategy`)

Components define how to simulate them efficiently:

```bhdl
@simulation_strategy {
    initial_analysis: { ... },    // Fast initial evaluation
    optimization_loop: { ... },   // Iterative optimization approach
    final_verification: { ... },  // Detailed final validation
}
```

### 4. Component Knowledge (`@component_knowledge`)

Design expertise embedded in the component:

```bhdl
@component_knowledge {
    good_starting_points: [ ... ], // Proven initial values
    scaling_rules: { ... },        // How parameters scale
    common_issues: { ... },        // Known problems and fixes
    coupled_parameters: { ... },   // Parameters that must be optimized together
}
```

### 5. Optimization Strategy (`@optimization_strategy`)

How to optimize this specific component:

```bhdl
@optimization_strategy {
    step1_topology: { ... },      // High-level decisions
    step2_components: { ... },    // Component value optimization
    step3_verify: { ... },        // Verification steps
    convergence_criteria: { ... }, // When to stop optimizing
}
```

### 6. Test Sequences (`@test_sequences`)

Standard tests for the component:

```bhdl
@test_sequences {
    stability: { ... },           // Stability analysis tests
    load_step: { ... },          // Transient response tests
    efficiency: { ... },         // Performance measurements
}
```

## Abstraction Levels

### Level 0: Analytical (Milliseconds)
- Pure equations, no simulation
- Used for initial sizing and sanity checks
- Example: `L_min = (Vin - Vout) * Vout / (Vin * ΔI * fsw)`

### Level 1: Behavioral Averaged (Seconds)
- State-space averaged models
- Small-signal AC analysis
- Good for control loop and stability
- Example: State-space matrices for averaged buck

### Level 2: Behavioral Switching (Minutes)
- Simplified switching models
- Captures major effects without full SPICE
- Example: Ideal switches with Ron, simplified PWM

### Level 3: Full SPICE (Hours)
- Complete SPICE models with parasitics
- Final verification only
- Example: Full transistor models, layout parasitics

## Implementation Workflow

### Phase 1: Component Analysis
1. Synthesizer reads component's `@simulation_requirements`
2. Identifies which analyses are needed
3. Determines minimum abstraction level required

### Phase 2: Model Selection
1. Based on analysis phase and time budget
2. Component's `@model_selector` chooses appropriate model
3. Falls back to simpler models if convergence issues

### Phase 3: Optimization Loop
1. Start with Level 0/1 models for exploration
2. Use `@optimization_strategy` to guide parameter search
3. Progressively refine with higher-level models
4. Stop when convergence criteria met

### Phase 4: Verification
1. Run `@test_sequences` on final design
2. Use Level 2/3 models for accuracy
3. Report pass/fail with specific metrics

## Feedback Loop Architecture

```
┌─────────────────┐
│   Component     │
│   Definition    │
│ ┌─────────────┐ │
│ │  @models    │ │
│ │  @strategy  │ │
│ │  @knowledge │ │
│ └─────────────┘ │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│   Synthesizer   │
│                 │
│  1. Read models │
│  2. Select level│
│  3. Simulate    │
│  4. Measure     │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Optimization   │
│     Engine      │
│                 │
│  - Sweep params │
│  - Check limits │
│  - Iterate      │
└────────┬────────┘
         │
    ◄────┘ Feedback
```

## Example: Buck Converter

```bhdl
module BuckConverter(...) {
    // Level 0: Equations
    @behavioral_model equations {
        model_type: "analytical",
        L_min: "(Vin - Vout) * Vout / (Vin * 0.3 * Iout * fsw)",
        C_min: "0.3 * Iout / (8 * fsw * Vripple)",
        use_when: "initial_sizing",
    }
    
    // Level 1: Averaged
    @behavioral_model averaged {
        model_type: "state_space_averaged",
        A_matrix: [ ... ],
        B_matrix: [ ... ],
        transfer_function: "Gvd = ...",
        use_when: "control_loop_analysis",
    }
    
    // Level 2: Behavioral Switching
    @behavioral_model switching_behavioral {
        model_type: "simplified_switching",
        switch_model: "ideal_with_Ron",
        pwm_model: "averaged_duty_cycle", 
        use_when: "ripple_analysis",
    }
    
    // Optimization guidance
    @optimization_strategy {
        initial_sweep: {
            model: "equations",
            parameters: ["L", "C"],
            grid: "coarse",
        },
        
        refinement: {
            model: "averaged",
            parameters: ["L", "C", "comp_R", "comp_C"],
            method: "gradient_descent",
            objective: "maximize(bandwidth) with phase_margin > 45°",
        },
        
        verification: {
            model: "switching_behavioral",
            checks: ["ripple < 1%", "efficiency > 90%"],
        }
    }
}
```

## Benefits

1. **Maintainability**: Update component = update all its models
2. **Reusability**: Inherit models from base classes
3. **Scalability**: Add new models without changing synthesizer
4. **Intelligence**: Components guide their own optimization
5. **Verification**: Built-in test sequences ensure correctness

## Migration Path

1. Start with key components (buck, boost, LDO)
2. Add behavioral models incrementally
3. Validate against known designs
4. Extend to more complex components
5. Build inheritance hierarchy for common patterns

## Success Metrics

- **Simulation Speed**: 100x faster than full SPICE for optimization
- **Accuracy**: Within 5% of SPICE for key metrics
- **Convergence**: 90% of designs converge in < 5 iterations
- **Coverage**: 80% of common circuits have embedded models

## Next Steps

1. Implement base simulation model types
2. Create buck converter with full embedded models
3. Build optimization engine that uses component guidance
4. Validate against real designs
5. Extend to other switching topologies