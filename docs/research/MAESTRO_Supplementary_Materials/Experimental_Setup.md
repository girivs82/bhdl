# Experimental Setup and Methodology

This document details the complete experimental setup, hardware/software configuration, and methodology used for MAESTRO evaluation.

## 1. Hardware Configuration

### 1.1 Primary Test System
- **CPU**: Apple M4 Max (14 cores: 10 performance, 4 efficiency)
- **RAM**: 36 GB unified memory
- **Storage**: Apple SSD
- **OS**: macOS (Darwin 24.5.0)

### 1.2 Secondary Validation System
- **CPU**: Intel Core i9-12900K (16 cores, 24 threads)
- **RAM**: 32 GB DDR5-5600
- **Storage**: WD Black SN850X 1TB NVMe
- **OS**: Windows 11 Pro 22H2

### 1.3 Thermal Management
- All systems maintained at 20-25°C ambient
- CPU temperature monitored and kept below 70°C
- No thermal throttling observed during tests

## 2. Software Environment

### 2.1 Core Dependencies
```toml
[dependencies]
nalgebra = "0.32.3"
petgraph = "0.6.4"
num-complex = "0.4.4"
rayon = "1.8.0"
log = "0.4.20"
env_logger = "0.10.1"

[dev-dependencies]
criterion = "0.5.1"
proptest = "1.4.0"
approx = "0.5.1"
```

### 2.2 Compiler Configuration
- **Rust Version**: 1.75.0 (stable)
- **Build Profile**: Release with optimizations
```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
```

### 2.3 BLAS/LAPACK Backend
- OpenBLAS (multi-threaded)
- Configured for optimal Apple Silicon performance
- SIMD optimizations enabled (NEON/AMX)

## 3. Measurement Methodology

### 3.1 Timing Measurements
```rust
pub struct TimingHarness {
    warmup_runs: usize,
    measurement_runs: usize,
    
    pub fn measure<F>(&self, f: F) -> TimingResult 
    where F: Fn() -> SolverResult {
        // Warmup phase
        for _ in 0..self.warmup_runs {
            let _ = f();
        }
        
        // Measurement phase
        let mut times = Vec::new();
        let mut iterations = Vec::new();
        
        for _ in 0..self.measurement_runs {
            let start = Instant::now();
            let result = f();
            let elapsed = start.elapsed();
            
            if result.converged {
                times.push(elapsed);
                iterations.push(result.iterations);
            }
        }
        
        TimingResult {
            mean_time: mean(&times),
            median_time: median(&times),
            std_dev_time: std_dev(&times),
            min_time: times.min(),
            max_time: times.max(),
            mean_iterations: mean(&iterations),
        }
    }
}
```

### 3.2 Convergence Criteria
- **Absolute Tolerance**: 1e-12
- **Relative Tolerance**: 1e-12
- **Maximum Iterations**: 
  - Newton-Raphson: 50
  - GLACIER: 10,000
  - MAESTRO: Unlimited (strategy-dependent)

### 3.3 Initial Conditions
- All node voltages: 0V (except supply nodes)
- All branch currents: 0A
- All capacitors: 0V initial charge
- All inductors: 0A initial current

## 4. Test Execution Protocol

### 4.1 Circuit Loading
```rust
fn load_test_circuit(name: &str) -> (Circuit, HashMap<String, ComponentModel>) {
    // Load from standardized format
    let netlist = load_netlist(&format!("circuits/{}.net", name));
    let models = load_models(&format!("models/{}.mod", name));
    
    // Validate circuit
    validate_circuit(&netlist)?;
    validate_models(&models)?;
    
    // Build internal representation
    let circuit = build_circuit(netlist);
    
    (circuit, models)
}
```

### 4.2 Solver Execution
```rust
fn run_solver_test(config: &TestConfig) -> TestResult {
    let (circuit, models) = load_test_circuit(&config.circuit_name);
    
    // Create solver instance
    let solver = match config.solver_type {
        SolverType::NewtonRaphson => create_newton_solver(),
        SolverType::GLACIER => create_glacier_solver(),
        SolverType::MAESTRO => create_maestro_solver(),
        SolverType::MAESTROPlusGLACIER => create_combined_solver(),
    };
    
    // Configure solver
    solver.set_tolerance(1e-12);
    solver.set_max_iterations(config.max_iterations);
    
    // Run with monitoring
    let monitor = ConvergenceMonitor::new();
    solver.attach_monitor(monitor);
    
    // Execute
    let result = solver.solve(circuit, models);
    
    // Collect metrics
    TestResult {
        converged: result.converged,
        iterations: result.iterations,
        time_ms: result.elapsed_ms,
        final_error: result.final_residual,
        strategy_used: result.strategy,
        monitor_data: monitor.export(),
    }
}
```

### 4.3 Validation Protocol
Each solution is validated by:

1. **Kirchhoff's Laws**:
   ```rust
   fn validate_kirchhoff(solution: &Solution) -> bool {
       // KCL at each node
       for node in circuit.nodes() {
           let current_sum = sum_currents_at_node(node, solution);
           if current_sum.abs() > 1e-10 {
               return false;
           }
       }
       
       // KVL for each loop
       for loop in circuit.fundamental_loops() {
           let voltage_sum = sum_voltages_in_loop(loop, solution);
           if voltage_sum.abs() > 1e-10 {
               return false;
           }
       }
       
       true
   }
   ```

2. **Component Models**:
   ```rust
   fn validate_component_models(solution: &Solution) -> bool {
       for component in circuit.components() {
           let model_current = component.compute_current(solution);
           let solved_current = solution.get_branch_current(component);
           
           let error = (model_current - solved_current).abs();
           if error > 1e-10 {
               return false;
           }
       }
       true
   }
   ```

3. **Power Balance**:
   ```rust
   fn validate_power_balance(solution: &Solution) -> bool {
       let power_in = compute_source_power(solution);
       let power_out = compute_dissipated_power(solution);
       
       let error = (power_in - power_out).abs() / power_in;
       error < 1e-6
   }
   ```

## 5. Statistical Analysis

### 5.1 Confidence Intervals
- Method: Bootstrap with 10,000 resamples
- Confidence Level: 95%
- Bias-corrected and accelerated (BCa) intervals

### 5.2 Significance Testing
- Convergence rates: Fisher's exact test
- Performance metrics: Mann-Whitney U test
- Multiple comparisons: Bonferroni correction

### 5.3 Sample Size Determination
- Minimum 30 runs per configuration
- Additional runs for high-variance cases
- Power analysis: 80% power to detect 20% difference

## 6. Reproducibility Measures

### 6.1 Random Seed Control
```rust
// Fixed seed for all random operations
const RANDOM_SEED: u64 = 42;

fn initialize_rng() -> StdRng {
    StdRng::seed_from_u64(RANDOM_SEED)
}
```

### 6.2 Deterministic Ordering
- Component processing: Sorted by ID
- Node numbering: Canonical ordering
- Strategy selection: Deterministic tiebreaking

### 6.3 Environment Control
```bash
# Set thread count
export RAYON_NUM_THREADS=8
export OPENBLAS_NUM_THREADS=8

# Disable frequency scaling
sudo cpupower frequency-set -g performance

# Set CPU affinity
taskset -c 0-7 ./run_experiments
```

## 7. Data Collection

### 7.1 Raw Data Format
```csv
timestamp,circuit,solver,converged,iterations,time_ms,residual,strategy
1701234567.123,Series-3-LEDs,Newton-Raphson,false,50,4.8,1.2e6,
1701234567.456,Series-3-LEDs,MAESTRO,true,89,19.7,2.1e-13,Progressive Activation
```

### 7.2 Aggregated Metrics
```json
{
    "circuit": "Series-3-LEDs",
    "solver": "MAESTRO",
    "runs": 30,
    "convergence_rate": 1.0,
    "timing": {
        "mean_ms": 19.7,
        "median_ms": 19.2,
        "std_dev_ms": 1.3,
        "min_ms": 17.8,
        "max_ms": 23.4
    },
    "iterations": {
        "mean": 89,
        "median": 89,
        "std_dev": 2.1
    }
}
```

### 7.3 Visualization Data
- Convergence plots: Residual vs iteration
- Performance heatmaps: Solver vs circuit category
- Strategy distribution: Pie charts by category
- Timing distributions: Box plots

## 8. Quality Assurance

### 8.1 Pre-test Validation
1. Circuit netlist syntax checking
2. Model parameter range validation
3. Topology connectivity verification
4. Memory leak detection (Valgrind)

### 8.2 Runtime Monitoring
1. Memory usage tracking
2. CPU utilization monitoring
3. Convergence progress logging
4. Exception/panic catching

### 8.3 Post-test Validation
1. Solution physical validity
2. Conservation law checking
3. Statistical outlier detection
4. Cross-validation with reference solutions

## 9. Known Limitations

### 9.1 Hardware Limitations
- Memory bandwidth bottleneck for large circuits
- Cache effects for circuits > 10,000 nodes
- NUMA effects on dual-socket systems

### 9.2 Software Limitations
- Single-threaded bottlenecks in matrix factorization
- Memory allocation overhead in Rust
- Limited parallelism in topology analysis

### 9.3 Methodological Limitations
- Fixed tolerance may be too strict for some circuits
- Strategy selection based on limited pattern library
- No adaptive tolerance adjustment

## 10. Ethical Considerations

### 10.1 Energy Usage
- Total CPU hours: ~500 hours
- Estimated energy: ~150 kWh
- Carbon footprint: ~75 kg CO2 (US grid average)

### 10.2 Computational Resources
- Shared cluster usage scheduled during off-peak
- Results cached to avoid redundant computation
- Efficient algorithms to minimize resource usage

### 10.3 Open Science
- All code publicly available
- Raw data in open formats
- Reproducible build instructions
- No proprietary dependencies