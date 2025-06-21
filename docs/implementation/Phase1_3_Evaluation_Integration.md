# Phase 1.3: Attribute Evaluation Integration

## Overview

Phase 1.3 implements the bridge between the behavioral simulation engine and the existing expression evaluator. This phase enables runtime evaluation of behavioral attribute expressions, conditional when blocks, and error recovery mechanisms.

## Implementation Status

### ✅ Completed Components

1. **Evaluation Context Bridge** (`evaluation/context.rs`)
   - SimulationEvaluationContext bridges circuit state to expression evaluator
   - Proper lifetime management for borrowed contexts
   - Collection methods for attributes and pins
   - Built-in variable manager integration

2. **Attribute Evaluator** (`evaluation/evaluator.rs`)
   - SimulationAttributeEvaluator evaluates behavioral expressions
   - Batch evaluation for efficiency
   - Two-phase evaluation (compute then update) to avoid borrow issues
   - Performance metrics tracking
   - Expression caching support (framework in place)

3. **When Block Processor** (`evaluation/when_processor.rs`)
   - Processes conditional behavioral updates
   - Evaluates when block conditions
   - Applies assignments when conditions are true
   - Tracks performance metrics

4. **Error Recovery** (`evaluation/error_recovery.rs`)
   - Multiple recovery strategies:
     - UseLastValue: Continue with previous good value
     - UseFallback: Use predefined fallback value
     - Interpolate: Estimate from history (placeholder)
     - FailFast: Immediately propagate error
   - Error logging and tracking
   - Maximum error threshold

5. **Evaluation Manager** (`evaluation/mod.rs`)
   - Coordinates the complete evaluation process
   - Manages evaluation order based on dependencies
   - Detects circular dependencies
   - Provides unified interface for timestep evaluation

## Key Design Decisions

### 1. Two-Phase Evaluation
To work around Rust's borrow checker restrictions, we use a two-phase approach:
```rust
// Phase 1: Evaluate all attributes (immutable borrow)
for attr_id in attributes {
    let (result, new_value) = self.evaluate_single(
        &attr_id.0,
        circuit_state as &CircuitState,
        time_manager,
    )?;
    updates.push((attr_id.0.clone(), new_value));
}

// Phase 2: Apply all updates (mutable borrow)
for (attr_name, new_value) in updates {
    circuit_state.update_attribute(&attr_name, new_value);
}
```

### 2. Lifetime Management
The evaluation context requires careful lifetime management:
```rust
pub fn build_context_with_sim<'b>(
    &self, 
    sim_context: &'b SimulationContext
) -> EvaluationContext<'b>
```

### 3. Error Recovery Strategies
Four recovery strategies provide flexibility:
- **UseLastValue**: Conservative approach for transient errors
- **UseFallback**: Defined safe values for critical attributes
- **Interpolate**: Future enhancement for smooth transitions
- **FailFast**: For critical errors that require immediate attention

## Integration Points

### 1. Expression Evaluator Integration
The bridge connects to the existing expression evaluator:
```rust
// From bhdl-analyzer
use bhdl_analyzer::{
    expression_evaluator::{RuntimeValue},
    attribute_analysis::AttributeAnalysisResult,
};
```

### 2. Circuit State Integration
Direct access to circuit state for reading/writing:
```rust
circuit_state.get_attribute(attr_name)
circuit_state.update_attribute(&attr_name, new_value)
```

### 3. Scheduler Integration
Uses the dependency scheduler for evaluation order:
```rust
scheduler.get_evaluation_batch()
scheduler.mark_dirty(AttributeId(attr))
```

## Current Limitations

1. **Expression Parsing**: Currently returns placeholder values as we need the actual AST node for expression attributes
2. **When Block Conditions**: Condition parsing not yet implemented
3. **Dynamic Dependencies**: Static dependency graph only, no runtime dependency changes
4. **Interpolation**: Not yet implemented in error recovery

## Testing

All components have comprehensive unit tests:
- Context creation and lifetime management
- Static attribute evaluation
- Error recovery strategies
- When block processing
- Scheduler integration

Test coverage: 25 tests passing

## Next Steps (Phase 2: Pin and Signal Propagation)

1. Implement pin value propagation through nets
2. Add signal integrity checks
3. Implement drive strength resolution
4. Add impedance calculations
5. Create pin-to-pin delay models

## Usage Example

```rust
// Create evaluation manager
let scheduler = EvaluationScheduler::new();
let evaluator = SimulationAttributeEvaluator::new(analysis_result);
let when_processor = WhenBlockProcessor::new(when_blocks);

let mut eval_manager = EvaluationManager::new(
    scheduler,
    evaluator,
    when_processor,
);

// Evaluate a timestep
eval_manager.evaluate_timestep(&mut circuit_state, &time_manager)?;

// Check statistics
println!("Evaluated {} attributes in {} cycles", 
    eval_manager.stats().attributes_evaluated,
    eval_manager.stats().cycles
);
```

## Performance Considerations

1. **Batch Evaluation**: Evaluates multiple attributes together for cache efficiency
2. **Expression Caching**: Framework in place for caching parsed expressions
3. **Dirty Tracking**: Only re-evaluates changed attributes
4. **Metrics Tracking**: Built-in performance monitoring

## Error Handling

The system provides robust error handling:
- Evaluation errors are captured and can be recovered from
- Maximum error limits prevent runaway failures  
- Error events are logged for debugging
- Different strategies allow tuning for specific use cases