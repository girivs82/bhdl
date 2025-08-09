# MAESTRO Code Repository

This directory contains the complete source code for reproducing all MAESTRO results.

## Directory Structure

```
Code_Repository/
├── src/
│   ├── maestro_engine.rs      # Main MAESTRO engine implementation
│   ├── strategies/            # All strategy implementations
│   │   ├── progressive.rs
│   │   ├── symmetry.rs
│   │   ├── hierarchical.rs
│   │   └── current_sharing.rs
│   ├── topology_analyzer.rs   # Circuit pattern detection
│   ├── solver_core.rs         # Newton-Raphson base solver
│   └── glacier_solver.rs      # GLACIER implementation
├── benchmarks/
│   ├── run_all_experiments.sh # Main experiment runner
│   ├── circuits/              # Test circuit netlists
│   └── analysis/              # Analysis scripts
├── tests/
│   └── unit_tests.rs          # Unit test suite
└── Cargo.toml                 # Rust dependencies

```

## Quick Start

### Prerequisites

1. Rust 1.75.0 or later
2. OpenBLAS for matrix operations
3. Python 3.8+ for analysis scripts

### Running All Experiments

```bash
./benchmarks/run_all_experiments.sh
```

This will:
1. Compile all solvers with optimizations
2. Run all 52 test circuits
3. Generate raw data CSV files
4. Produce all visualizations
5. Calculate statistics

### Running Individual Tests

```bash
# Test a specific circuit
cargo run --release --bin maestro_test -- circuits/Series-5-LEDs.net

# Compare solvers on one circuit
cargo run --release --bin compare_solvers -- circuits/Series-3-LEDs.net

# Run with debug output
RUST_LOG=debug cargo run --release --bin maestro_test -- circuits/Series-5-LEDs.net
```

### Building

```bash
# Debug build
cargo build

# Release build with optimizations
cargo build --release

# Run tests
cargo test
```

## Implementation Notes

### Core MAESTRO Engine

The main orchestration loop is in `src/maestro_engine.rs`:

```rust
pub struct MAESTROEngine {
    topology_analyzer: TopologyAnalyzer,
    strategy_selector: StrategySelector,
    core_solver: NewtonRaphsonSolver,
    performance_db: PerformanceDatabase,
}

impl MAESTROEngine {
    pub fn solve(&mut self, circuit: &Circuit) -> SolverResult {
        // 1. Analyze topology
        let patterns = self.topology_analyzer.detect_patterns(circuit);
        
        // 2. Select strategies
        let strategies = self.strategy_selector.select_strategies(&patterns);
        
        // 3. Execute strategies
        for (pattern, strategy) in strategies {
            let result = strategy.apply(circuit, &pattern);
            if result.converged {
                return result;
            }
        }
        
        // 4. Fallback
        self.core_solver.solve(circuit)
    }
}
```

### Progressive Activation Strategy

Key implementation in `src/strategies/progressive.rs`:

```rust
pub struct ProgressiveActivationStrategy {
    high_resistance: f64,  // 10 MΩ
    damping_factor: f64,   // 0.5
}

impl Strategy for ProgressiveActivationStrategy {
    fn apply(&self, circuit: &Circuit, pattern: &Pattern) -> SolverResult {
        let components = self.order_components(&pattern.components);
        let mut solutions = Vec::new();
        
        for i in 1..=components.len() {
            // Activate components[0..i], deactivate rest
            let modified_circuit = self.modify_circuit(circuit, &components, i);
            
            // Use previous solution as initial guess
            let initial_guess = solutions.last()
                .map(|s| s.solution.clone())
                .unwrap_or_else(|| self.smart_guess(&modified_circuit));
            
            let result = self.solve_subproblem(modified_circuit, initial_guess);
            
            if !result.converged {
                return SolverResult::failed();
            }
            
            solutions.push(result);
        }
        
        SolverResult::from_steps(solutions)
    }
}
```

## Performance Optimizations

1. **Matrix Operations**: Uses BLAS for all linear algebra
2. **Parallelization**: Strategy execution can run in parallel
3. **Caching**: Component model evaluations are cached
4. **Memory Pool**: Reuses allocated matrices across iterations

## Validation

Each solution is validated against:
- Kirchhoff's Current Law (KCL)
- Kirchhoff's Voltage Law (KVL)  
- Component constitutive relations
- Power balance

## License

This code is provided for research purposes. See LICENSE file for details.

## Citation

If you use this code, please cite:

```bibtex
@inproceedings{maestro2024,
  title={MAESTRO: Multi-strategy Adaptive Engine for Smart Topology-driven Resolution and Orchestration},
  author={...},
  booktitle={International Conference on Computer-Aided Design},
  year={2024}
}
```