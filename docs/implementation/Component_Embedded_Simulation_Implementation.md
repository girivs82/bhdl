# Component-Embedded Simulation Implementation Plan

## Overview

This document provides the detailed implementation plan for adding component-embedded simulation to the BHDL toolchain. Unlike the conceptual BHDL examples shown in the proposal, this focuses on the actual Rust implementation required.

## Current State Analysis

### Existing Infrastructure
- **bhdl-parser**: Parses BHDL v2.0 syntax, supports attributes
- **bhdl-ast**: AST representation with attribute support
- **bhdl-analyzer**: 8-pass semantic analysis 
- **bhdl-synthesizer**: Generates netlists from AST
- **bhdl-spice**: Has DC solver, but no behavioral models
- **bhdl-netlist**: Structural representation

### Gaps to Fill
1. Parser doesn't recognize `@behavioral_model`, `@optimization_strategy` annotations
2. No simulation engine to execute behavioral models
3. No optimization algorithms (grid search, Nelder-Mead, etc.)
4. No model selection logic
5. No simulation-synthesis feedback loop

## Implementation Architecture

```
┌─────────────────────────────────────────────────────┐
│                    BHDL File                        │
│  entity BuckConverter {                             │
│    @behavioral_model analytical { ... }             │
│    @optimization_strategy { ... }                   │
│  }                                                  │
└─────────────────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────┐
│               bhdl-parser (MODIFIED)                │
│  - Add BEHAVIORAL_MODEL_KW token                    │
│  - Add OPTIMIZATION_STRATEGY_KW token               │
│  - Parse annotation blocks                          │
└─────────────────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────┐
│                bhdl-ast (MODIFIED)                  │
│  - Add BehavioralModel struct                       │
│  - Add OptimizationStrategy struct                  │
│  - Store in ComponentDef                            │
└─────────────────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────┐
│              bhdl-analyzer (MODIFIED)               │
│  - Extract behavioral models in Pass 1              │
│  - Store in symbol table                            │
│  - Validate model definitions                       │
└─────────────────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────┐
│           bhdl-simulation (NEW CRATE)               │
│  - SimulationEngine: Orchestrates optimization      │
│  - ModelEvaluator: Executes behavioral models       │
│  - OptimizationAlgorithms: Grid, Nelder-Mead, etc. │
│  - SimulationCache: Results caching                 │
└─────────────────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────┐
│           bhdl-synthesizer (MODIFIED)               │
│  - Receives optimized parameters from simulation    │
│  - Generates netlist with optimal values            │
│  - Feeds back verification results                  │
└─────────────────────────────────────────────────────┘
```

## Detailed Implementation Steps

### Step 1: Parser Extensions

#### 1.1 Add New Tokens
```rust
// bhdl-parser/src/syntax.rs
pub enum SyntaxKind {
    // ... existing tokens ...
    
    // New annotation keywords
    BEHAVIORAL_MODEL_KW,      // @behavioral_model
    OPTIMIZATION_STRATEGY_KW, // @optimization_strategy
    COMPONENT_KNOWLEDGE_KW,   // @component_knowledge
    SIMULATION_REQUIREMENTS_KW, // @simulation_requirements
    TEST_SEQUENCES_KW,        // @test_sequences
}
```

#### 1.2 Parse Annotation Blocks
```rust
// bhdl-parser/src/parser.rs
fn parse_component_annotation(p: &mut Parser) {
    match p.current() {
        BEHAVIORAL_MODEL_KW => parse_behavioral_model(p),
        OPTIMIZATION_STRATEGY_KW => parse_optimization_strategy(p),
        COMPONENT_KNOWLEDGE_KW => parse_component_knowledge(p),
        _ => p.error("Unknown annotation"),
    }
}

fn parse_behavioral_model(p: &mut Parser) {
    let m = p.start();
    p.expect(BEHAVIORAL_MODEL_KW);
    p.expect(IDENT); // model name
    p.expect(L_BRACE);
    
    // Parse model properties
    while !p.at(R_BRACE) && !p.at(EOF) {
        parse_model_property(p);
    }
    
    p.expect(R_BRACE);
    m.complete(p, BEHAVIORAL_MODEL);
}
```

### Step 2: AST Extensions

#### 2.1 Add Model Structures
```rust
// bhdl-ast/src/models.rs
use crate::ast::{AstNode, SyntaxNode};

#[derive(Debug, Clone)]
pub struct BehavioralModel {
    syntax: SyntaxNode,
}

impl BehavioralModel {
    pub fn name(&self) -> Option<String> {
        // Extract model name
    }
    
    pub fn model_type(&self) -> Option<ModelType> {
        // Extract model type (analytical, averaged, etc.)
    }
    
    pub fn properties(&self) -> HashMap<String, Value> {
        // Extract all model properties
    }
}

#[derive(Debug, Clone)]
pub enum ModelType {
    Analytical,
    StateSpaceAveraged,
    BehavioralSwitching,
    FullSpice,
}

#[derive(Debug, Clone)]
pub struct OptimizationStrategy {
    syntax: SyntaxNode,
    phases: Vec<OptimizationPhase>,
}

#[derive(Debug, Clone)]
pub struct OptimizationPhase {
    name: String,
    model: String,
    algorithm: OptimizationAlgorithm,
    parameters: Vec<String>,
}
```

#### 2.2 Extend ComponentDef
```rust
// bhdl-ast/src/ast.rs
impl ComponentDef {
    pub fn behavioral_models(&self) -> Vec<BehavioralModel> {
        self.syntax
            .children()
            .filter_map(BehavioralModel::cast)
            .collect()
    }
    
    pub fn optimization_strategy(&self) -> Option<OptimizationStrategy> {
        self.syntax
            .children()
            .find_map(OptimizationStrategy::cast)
    }
}
```

### Step 3: New Simulation Crate

#### 3.1 Create bhdl-simulation
```toml
# bhdl-simulation/Cargo.toml
[package]
name = "bhdl-simulation"
version = "0.1.0"

[dependencies]
bhdl-ast = { path = "../bhdl-ast" }
bhdl-netlist = { path = "../bhdl-netlist" }
nalgebra = "0.32"  # For matrix operations
rayon = "1.7"      # For parallel simulation
lru = "0.12"       # For simulation cache
```

#### 3.2 Simulation Engine Core
```rust
// bhdl-simulation/src/engine.rs
use std::collections::HashMap;
use bhdl_ast::{BehavioralModel, OptimizationStrategy};

pub struct SimulationEngine {
    cache: SimulationCache,
    thread_pool: rayon::ThreadPool,
}

impl SimulationEngine {
    pub fn optimize_component(
        &mut self,
        component: &ComponentDef,
        requirements: &Requirements,
    ) -> OptimizationResult {
        // 1. Extract behavioral models
        let models = component.behavioral_models();
        
        // 2. Get optimization strategy
        let strategy = component.optimization_strategy()
            .unwrap_or_else(|| self.default_strategy());
        
        // 3. Execute optimization phases
        let mut current_design = self.initial_design(component);
        
        for phase in strategy.phases() {
            let model = self.select_model(&models, &phase);
            current_design = self.execute_phase(
                model,
                phase,
                current_design,
                requirements,
            )?;
        }
        
        // 4. Final verification
        let verification = self.verify_design(
            &models,
            &current_design,
            requirements,
        );
        
        OptimizationResult {
            final_design: current_design,
            verification,
        }
    }
}
```

#### 3.3 Model Evaluators
```rust
// bhdl-simulation/src/evaluators/analytical.rs
pub struct AnalyticalEvaluator;

impl ModelEvaluator for AnalyticalEvaluator {
    fn evaluate(
        &self,
        model: &BehavioralModel,
        parameters: &Parameters,
    ) -> SimulationResult {
        // Parse equations from model
        let equations = model.get_property("equations")?;
        
        // Evaluate using expression parser
        let mut results = HashMap::new();
        for (name, expr) in equations {
            let value = evaluate_expression(expr, parameters)?;
            results.insert(name, value);
        }
        
        SimulationResult { results }
    }
}

// bhdl-simulation/src/evaluators/state_space.rs
use nalgebra::{DMatrix, DVector};

pub struct StateSpaceEvaluator;

impl ModelEvaluator for StateSpaceEvaluator {
    fn evaluate(
        &self,
        model: &BehavioralModel,
        parameters: &Parameters,
    ) -> SimulationResult {
        // Extract state-space matrices
        let a_matrix = parse_matrix(model.get_property("A_matrix")?);
        let b_matrix = parse_matrix(model.get_property("B_matrix")?);
        let c_matrix = parse_matrix(model.get_property("C_matrix")?);
        let d_matrix = parse_matrix(model.get_property("D_matrix")?);
        
        // Create state-space model
        let ss_model = StateSpaceModel::new(a_matrix, b_matrix, c_matrix, d_matrix);
        
        // Calculate transfer functions
        let gvd = ss_model.control_to_output();
        
        // Analyze stability
        let phase_margin = calculate_phase_margin(&gvd);
        let crossover_freq = find_crossover_frequency(&gvd);
        
        SimulationResult {
            phase_margin,
            crossover_frequency: crossover_freq,
        }
    }
}
```

#### 3.4 Optimization Algorithms
```rust
// bhdl-simulation/src/optimization/grid_search.rs
pub struct GridSearchOptimizer {
    resolution: GridResolution,
}

impl Optimizer for GridSearchOptimizer {
    fn optimize(
        &self,
        model: &dyn ModelEvaluator,
        initial: Parameters,
        objectives: &[Objective],
        constraints: &[Constraint],
    ) -> Parameters {
        // Create parameter grid
        let grid = self.create_grid(&initial);
        
        // Evaluate in parallel
        let results: Vec<_> = grid
            .par_iter()
            .map(|params| {
                let result = model.evaluate(params);
                let score = calculate_score(&result, objectives, constraints);
                (params.clone(), score)
            })
            .collect();
        
        // Find best
        results
            .into_iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(params, _)| params)
            .unwrap()
    }
}

// bhdl-simulation/src/optimization/nelder_mead.rs
pub struct NelderMeadOptimizer {
    max_iterations: usize,
    tolerance: f64,
}

impl Optimizer for NelderMeadOptimizer {
    fn optimize(
        &self,
        model: &dyn ModelEvaluator,
        initial: Parameters,
        objectives: &[Objective],
        constraints: &[Constraint],
    ) -> Parameters {
        // Initialize simplex
        let mut simplex = self.create_simplex(&initial);
        
        for _ in 0..self.max_iterations {
            // Evaluate all points
            for point in &mut simplex {
                point.score = evaluate_point(model, &point.params, objectives, constraints);
            }
            
            // Sort by score
            simplex.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
            
            // Check convergence
            if self.converged(&simplex) {
                break;
            }
            
            // Nelder-Mead operations
            self.update_simplex(&mut simplex, model, objectives, constraints);
        }
        
        simplex[0].params.clone()
    }
}
```

### Step 4: Integration with Synthesizer

#### 4.1 Modify Synthesizer
```rust
// bhdl-synthesizer/src/lib.rs
use bhdl_simulation::{SimulationEngine, Requirements};

pub struct Synthesizer {
    simulation_engine: Option<SimulationEngine>,
}

impl Synthesizer {
    pub fn synthesize_with_optimization(
        &mut self,
        ast: &SourceFile,
        requirements: &Requirements,
    ) -> Result<Netlist> {
        // Run optimization if engine available
        if let Some(engine) = &mut self.simulation_engine {
            for component in ast.components() {
                if component.has_behavioral_models() {
                    let result = engine.optimize_component(&component, requirements)?;
                    
                    // Apply optimized parameters
                    self.apply_optimization_result(&component, &result);
                }
            }
        }
        
        // Continue with normal synthesis
        self.synthesize(ast)
    }
}
```

### Step 5: Testing Infrastructure

#### 5.1 Test Buck Converter
```rust
// bhdl-simulation/tests/test_buck_optimization.rs
#[test]
fn test_buck_converter_optimization() {
    let bhdl_code = r#"
        entity BuckConverter(vin_nom: voltage, vout: voltage) {
            @behavioral_model analytical {
                model_type: "equations",
                L_min: "(vin_nom - vout) * vout / (vin_nom * 0.3 * 2A * 500kHz)",
            }
            
            @optimization_strategy {
                phase1: {
                    model: "analytical",
                    algorithm: "grid_search",
                }
            }
        }
    "#;
    
    let ast = parse(bhdl_code);
    let component = ast.components().next().unwrap();
    
    let mut engine = SimulationEngine::new();
    let requirements = Requirements {
        efficiency: 0.9,
        ripple: 50e-3,
    };
    
    let result = engine.optimize_component(&component, &requirements);
    
    assert!(result.final_design.get("L").unwrap() > 10e-6);
    assert!(result.final_design.get("L").unwrap() < 100e-6);
}
```

## Migration Strategy

### Phase 1: Parser Support (Week 1)
1. Add tokens for annotations
2. Parse behavioral model blocks
3. Store in AST
4. Write parser tests

### Phase 2: Simulation Engine (Week 2-3)
1. Create bhdl-simulation crate
2. Implement analytical evaluator
3. Add grid search optimizer
4. Create simulation cache

### Phase 3: State-Space Models (Week 4)
1. Add matrix operations
2. Implement state-space evaluator
3. Add stability analysis
4. Implement Nelder-Mead optimizer

### Phase 4: Integration (Week 5)
1. Connect to synthesizer
2. Add feedback loop
3. Implement model selection logic
4. Test with real circuits

### Phase 5: Advanced Features (Week 6+)
1. Add parallel simulation
2. Implement adaptive algorithms
3. Add convergence detection
4. Performance optimization

## Performance Considerations

### Caching Strategy
- Use LRU cache with 1000 entry limit
- Key: hash of (model_id, parameters, requirements)
- Persist cache to disk between sessions

### Parallel Execution
- Use rayon for parallel grid search
- Thread pool size = num_cpus - 1
- Work stealing for load balancing

### Memory Management
- Lazy load behavioral models
- Share models between component instances
- Clear cache periodically

## Testing Plan

### Unit Tests
- Parser: Annotation parsing
- AST: Model extraction
- Simulation: Each evaluator
- Optimization: Each algorithm

### Integration Tests
- Buck converter optimization
- Multi-converter system
- Convergence behavior
- Cache effectiveness

### Performance Tests
- Optimization time vs manual
- Cache hit rate
- Parallel speedup
- Memory usage

## Success Criteria

1. **Parser**: Successfully parses all annotation types
2. **Models**: Analytical and state-space models working
3. **Optimization**: Grid search and Nelder-Mead converge
4. **Integration**: Synthesizer uses optimized values
5. **Performance**: 10x faster than manual iteration

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Parser complexity | Incremental implementation |
| Convergence failures | Multiple algorithm fallbacks |
| Performance issues | Aggressive caching |
| Memory bloat | Lazy loading, model sharing |

## Conclusion

This implementation plan provides a concrete path to adding component-embedded simulation to BHDL. By extending the parser, creating a simulation engine, and integrating with the synthesizer, we enable automatic optimization guided by component-specific knowledge.

The key insight is that components become active participants in their own optimization, dramatically reducing design time while improving quality.