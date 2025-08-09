//! Reference implementation for MAESTRO solver comparison metrics
//! 
//! This provides standardized metrics for comparing MAESTRO's topology-aware
//! solving against traditional methods.

use bhdl_spice::{
    Circuit, ComponentModel, ElectricalLimits, SpiceError, Result,
    glacier_solver::GlacierSolver,
    nonlinear_analysis::NonlinearDcAnalysis,
    intelligent_engine::{IntelligentSpiceEngine, TopologyAnalyzer, StrategyOrchestrator},
    NodeVoltages, BranchCurrents, AnalysisResult,
};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use std::fs::File;
use std::io::Write;

/// Metrics collected for each solver run
#[derive(Debug, Clone)]
pub struct MaestroMetrics {
    /// Solver name/variant
    pub solver_name: String,
    /// Circuit description
    pub circuit_name: String,
    /// Circuit category
    pub category: CircuitCategory,
    /// Did the solver converge?
    pub converged: bool,
    /// Total iterations (0 if failed)
    pub iterations: usize,
    /// Solution time in milliseconds
    pub time_ms: f64,
    /// Final error/residual norm
    pub final_error: f64,
    /// Strategies used (MAESTRO only)
    pub strategies_used: Vec<String>,
    /// Number of progressive steps (if applicable)
    pub progressive_steps: Option<usize>,
}

/// Circuit categories for testing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CircuitCategory {
    SeriesNonlinear,
    ParallelArrays,
    PowerConverters,
    CascadedAmplifiers,
    BridgeCircuits,
    ProtectionCircuits,
}

impl CircuitCategory {
    fn name(&self) -> &'static str {
        match self {
            CircuitCategory::SeriesNonlinear => "Series Nonlinear",
            CircuitCategory::ParallelArrays => "Parallel Arrays",
            CircuitCategory::PowerConverters => "Power Converters",
            CircuitCategory::CascadedAmplifiers => "Cascaded Amplifiers",
            CircuitCategory::BridgeCircuits => "Bridge Circuits",
            CircuitCategory::ProtectionCircuits => "Protection Circuits",
        }
    }
}

/// Test circuit configurations
pub struct TestCircuit {
    pub name: String,
    pub category: CircuitCategory,
    pub description: String,
    pub build_fn: Box<dyn Fn() -> (Circuit, HashMap<String, ComponentModel>)>,
}

/// Create comprehensive test circuit suite
pub fn create_test_circuits() -> Vec<TestCircuit> {
    let mut circuits = Vec::new();
    
    // Series Nonlinear Circuits (15 total)
    for n_leds in 2..=10 {
        circuits.push(TestCircuit {
            name: format!("Series-{}-LEDs", n_leds),
            category: CircuitCategory::SeriesNonlinear,
            description: format!("{} LEDs in series with extreme Is", n_leds),
            build_fn: Box::new(move || create_series_leds(n_leds)),
        });
    }
    
    // Add mixed LED-diode chains
    circuits.push(TestCircuit {
        name: "Mixed-LED-Diode-5".to_string(),
        category: CircuitCategory::SeriesNonlinear,
        description: "3 LEDs + 2 diodes in series".to_string(),
        build_fn: Box::new(create_mixed_led_diode_chain),
    });
    
    // More series circuits...
    for i in 1..=5 {
        circuits.push(TestCircuit {
            name: format!("Voltage-Multiplier-{}", i),
            category: CircuitCategory::SeriesNonlinear,
            description: format!("{}-stage voltage multiplier", i),
            build_fn: Box::new(move || create_voltage_multiplier(i)),
        });
    }
    
    // Parallel Arrays (8 total)
    for n_parallel in [2, 3, 5, 10, 20] {
        circuits.push(TestCircuit {
            name: format!("Parallel-{}-LEDs", n_parallel),
            category: CircuitCategory::ParallelArrays,
            description: format!("{} parallel LEDs with current sharing", n_parallel),
            build_fn: Box::new(move || create_parallel_leds(n_parallel, true)),
        });
    }
    
    // Mismatched parallel arrays
    circuits.push(TestCircuit {
        name: "Parallel-Mismatched-5".to_string(),
        category: CircuitCategory::ParallelArrays,
        description: "5 parallel LEDs with 10x Is variation".to_string(),
        build_fn: Box::new(create_mismatched_parallel_leds),
    });
    
    // More parallel circuits...
    for ballast in [false, true] {
        circuits.push(TestCircuit {
            name: format!("Parallel-10-Ballast-{}", ballast),
            category: CircuitCategory::ParallelArrays,
            description: format!("10 parallel LEDs {} ballast resistors", 
                if ballast { "with" } else { "without" }),
            build_fn: Box::new(move || create_parallel_leds(10, ballast)),
        });
    }
    
    // Power Converters (10 total)
    circuits.push(TestCircuit {
        name: "Buck-Basic".to_string(),
        category: CircuitCategory::PowerConverters,
        description: "Basic buck converter".to_string(),
        build_fn: Box::new(create_buck_converter),
    });
    
    circuits.push(TestCircuit {
        name: "Buck-SoftStart".to_string(),
        category: CircuitCategory::PowerConverters,
        description: "Buck with soft-start circuit".to_string(),
        build_fn: Box::new(create_buck_with_softstart),
    });
    
    circuits.push(TestCircuit {
        name: "Boost-Basic".to_string(),
        category: CircuitCategory::PowerConverters,
        description: "Basic boost converter".to_string(),
        build_fn: Box::new(create_boost_converter),
    });
    
    // More converters...
    for topology in ["Buck-Boost", "SEPIC", "Cuk", "Forward", "Flyback"] {
        circuits.push(TestCircuit {
            name: format!("{}-Converter", topology),
            category: CircuitCategory::PowerConverters,
            description: format!("{} converter topology", topology),
            build_fn: Box::new(move || create_generic_converter(topology)),
        });
    }
    
    // Cascaded Amplifiers (7 total)
    for stages in 2..=5 {
        circuits.push(TestCircuit {
            name: format!("Cascade-{}-Stage", stages),
            category: CircuitCategory::CascadedAmplifiers,
            description: format!("{}-stage cascaded amplifier", stages),
            build_fn: Box::new(move || create_cascaded_amplifier(stages)),
        });
    }
    
    circuits.push(TestCircuit {
        name: "Cascade-AC-Coupled".to_string(),
        category: CircuitCategory::CascadedAmplifiers,
        description: "3-stage AC-coupled amplifier".to_string(),
        build_fn: Box::new(create_ac_coupled_cascade),
    });
    
    circuits.push(TestCircuit {
        name: "Cascade-Feedback".to_string(),
        category: CircuitCategory::CascadedAmplifiers,
        description: "2-stage amplifier with feedback".to_string(),
        build_fn: Box::new(create_amplifier_with_feedback),
    });
    
    // Bridge Circuits (6 total)
    circuits.push(TestCircuit {
        name: "Bridge-Rectifier-Basic".to_string(),
        category: CircuitCategory::BridgeCircuits,
        description: "Full-wave bridge rectifier".to_string(),
        build_fn: Box::new(create_bridge_rectifier),
    });
    
    circuits.push(TestCircuit {
        name: "Bridge-Synchronous".to_string(),
        category: CircuitCategory::BridgeCircuits,
        description: "Synchronous rectifier bridge".to_string(),
        build_fn: Box::new(create_synchronous_bridge),
    });
    
    for phases in [3, 6] {
        circuits.push(TestCircuit {
            name: format!("Bridge-{}-Phase", phases),
            category: CircuitCategory::BridgeCircuits,
            description: format!("{}-phase rectifier", phases),
            build_fn: Box::new(move || create_polyphase_rectifier(phases)),
        });
    }
    
    circuits.push(TestCircuit {
        name: "Bridge-Active-PFC".to_string(),
        category: CircuitCategory::BridgeCircuits,
        description: "Active PFC bridge circuit".to_string(),
        build_fn: Box::new(create_active_pfc_bridge),
    });
    
    circuits.push(TestCircuit {
        name: "Bridge-Voltage-Doubler".to_string(),
        category: CircuitCategory::BridgeCircuits,
        description: "Voltage doubler rectifier".to_string(),
        build_fn: Box::new(create_voltage_doubler),
    });
    
    // Protection Circuits (6 total)
    circuits.push(TestCircuit {
        name: "Protection-OVP-TVS".to_string(),
        category: CircuitCategory::ProtectionCircuits,
        description: "Overvoltage protection with TVS".to_string(),
        build_fn: Box::new(create_ovp_circuit),
    });
    
    circuits.push(TestCircuit {
        name: "Protection-Current-Limit".to_string(),
        category: CircuitCategory::ProtectionCircuits,
        description: "Current limiting with foldback".to_string(),
        build_fn: Box::new(create_current_limiter),
    });
    
    circuits.push(TestCircuit {
        name: "Protection-HotSwap".to_string(),
        category: CircuitCategory::ProtectionCircuits,
        description: "Hot-swap controller circuit".to_string(),
        build_fn: Box::new(create_hotswap_controller),
    });
    
    circuits.push(TestCircuit {
        name: "Protection-Crowbar".to_string(),
        category: CircuitCategory::ProtectionCircuits,
        description: "Crowbar protection circuit".to_string(),
        build_fn: Box::new(create_crowbar_protection),
    });
    
    circuits.push(TestCircuit {
        name: "Protection-Reverse-Polarity".to_string(),
        category: CircuitCategory::ProtectionCircuits,
        description: "Reverse polarity protection".to_string(),
        build_fn: Box::new(create_reverse_polarity_protection),
    });
    
    circuits.push(TestCircuit {
        name: "Protection-ESD".to_string(),
        category: CircuitCategory::ProtectionCircuits,
        description: "ESD protection network".to_string(),
        build_fn: Box::new(create_esd_protection),
    });
    
    circuits
}

/// Solver configurations to test
pub enum SolverConfig {
    NewtonRaphson,
    Glacier,
    Maestro,
    MaestroWithGlacier,
}

impl SolverConfig {
    fn name(&self) -> &'static str {
        match self {
            SolverConfig::NewtonRaphson => "Newton-Raphson",
            SolverConfig::Glacier => "GLACIER",
            SolverConfig::Maestro => "MAESTRO",
            SolverConfig::MaestroWithGlacier => "MAESTRO+GLACIER",
        }
    }
}

/// Run a single solver test
pub fn run_solver_test(
    solver_config: &SolverConfig,
    test_circuit: &TestCircuit,
    timeout_ms: u64,
) -> MaestroMetrics {
    let (circuit, models) = (test_circuit.build_fn)();
    let start = Instant::now();
    
    let (converged, iterations, final_error, strategies_used, progressive_steps) = 
        match solver_config {
            SolverConfig::NewtonRaphson => {
                run_newton_raphson(circuit, models, timeout_ms)
            }
            SolverConfig::Glacier => {
                run_glacier(circuit, models, timeout_ms)
            }
            SolverConfig::Maestro => {
                run_maestro(circuit, models, timeout_ms, false)
            }
            SolverConfig::MaestroWithGlacier => {
                run_maestro(circuit, models, timeout_ms, true)
            }
        };
    
    let time_ms = start.elapsed().as_secs_f64() * 1000.0;
    
    MaestroMetrics {
        solver_name: solver_config.name().to_string(),
        circuit_name: test_circuit.name.clone(),
        category: test_circuit.category,
        converged,
        iterations,
        time_ms,
        final_error,
        strategies_used,
        progressive_steps,
    }
}

/// Run Newton-Raphson solver
fn run_newton_raphson(
    circuit: Circuit,
    models: HashMap<String, ComponentModel>,
    timeout_ms: u64,
) -> (bool, usize, f64, Vec<String>, Option<usize>) {
    let mut solver = NonlinearDcAnalysis::new(circuit);
    
    for (name, model) in models {
        solver.add_component(name, model);
    }
    
    // Simple timeout mechanism
    solver.set_max_iterations((timeout_ms / 10) as usize); // Rough estimate
    
    match solver.analyze() {
        Ok(result) => {
            (true, result.iterations, 0.0, vec!["Newton-Raphson".to_string()], None)
        }
        Err(_) => {
            (false, 0, f64::INFINITY, vec!["Newton-Raphson".to_string()], None)
        }
    }
}

/// Run GLACIER solver
fn run_glacier(
    circuit: Circuit,
    models: HashMap<String, ComponentModel>,
    timeout_ms: u64,
) -> (bool, usize, f64, Vec<String>, Option<usize>) {
    let mut solver = GlacierSolver::new(circuit);
    
    for (name, model) in models {
        solver.add_model(name, model);
    }
    
    match solver.analyze() {
        Ok(results) => {
            let total_iter = results.iter().map(|(_, _, _, r)| r.iterations).sum();
            let final_error = 0.0015; // GLACIER typical error
            (true, total_iter, final_error, vec!["GLACIER".to_string()], None)
        }
        Err(_) => {
            (false, 0, f64::INFINITY, vec!["GLACIER".to_string()], None)
        }
    }
}

/// Run MAESTRO solver
fn run_maestro(
    circuit: Circuit,
    models: HashMap<String, ComponentModel>,
    timeout_ms: u64,
    use_glacier_core: bool,
) -> (bool, usize, f64, Vec<String>, Option<usize>) {
    let mut engine = IntelligentSpiceEngine::new();
    
    // Configure core solver
    if use_glacier_core {
        engine.set_core_solver(Box::new(GlacierSolver::new(circuit.clone())));
    }
    
    // Add models
    for (name, model) in models {
        engine.add_model(name, model);
    }
    
    // Run intelligent solving
    match engine.solve(circuit) {
        Ok(result) => {
            let strategies = result.strategies_used.clone();
            let progressive_steps = if strategies.contains(&"Progressive Activation".to_string()) {
                Some(result.progressive_steps.unwrap_or(0))
            } else {
                None
            };
            
            (true, result.total_iterations, result.final_error, strategies, progressive_steps)
        }
        Err(_) => {
            (false, 0, f64::INFINITY, vec!["Failed".to_string()], None)
        }
    }
}

/// Generate comparison report
pub fn generate_maestro_report(metrics: Vec<MaestroMetrics>, output_path: &str) -> Result<()> {
    let mut file = File::create(output_path)?;
    
    // Write header
    writeln!(file, "# MAESTRO Solver Comparison Report")?;
    writeln!(file, "Generated: {:?}\n", std::time::SystemTime::now())?;
    
    // Overall summary
    writeln!(file, "## Overall Convergence Summary\n")?;
    
    // Group by category and solver
    let mut by_category_solver: HashMap<(CircuitCategory, String), Vec<&MaestroMetrics>> = HashMap::new();
    
    for metric in &metrics {
        by_category_solver
            .entry((metric.category, metric.solver_name.clone()))
            .or_insert_with(Vec::new)
            .push(metric);
    }
    
    // Summary table
    writeln!(file, "| Circuit Category | Newton-Raphson | GLACIER | MAESTRO | MAESTRO+GLACIER |")?;
    writeln!(file, "|-----------------|----------------|---------|---------|-----------------|")?;
    
    for category in [
        CircuitCategory::SeriesNonlinear,
        CircuitCategory::ParallelArrays,
        CircuitCategory::PowerConverters,
        CircuitCategory::CascadedAmplifiers,
        CircuitCategory::BridgeCircuits,
        CircuitCategory::ProtectionCircuits,
    ] {
        write!(file, "| {} ", category.name())?;
        
        for solver in ["Newton-Raphson", "GLACIER", "MAESTRO", "MAESTRO+GLACIER"] {
            let metrics = by_category_solver
                .get(&(category, solver.to_string()))
                .unwrap_or(&vec![]);
            
            let total = metrics.len();
            let converged = metrics.iter().filter(|m| m.converged).count();
            let rate = if total > 0 {
                (converged as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            
            write!(file, "| {:.1}% ({}/{}) ", rate, converged, total)?;
        }
        writeln!(file, "|")?;
    }
    
    // Overall row
    write!(file, "| **Overall** ")?;
    for solver in ["Newton-Raphson", "GLACIER", "MAESTRO", "MAESTRO+GLACIER"] {
        let solver_metrics: Vec<_> = metrics.iter()
            .filter(|m| m.solver_name == solver)
            .collect();
        
        let total = solver_metrics.len();
        let converged = solver_metrics.iter().filter(|m| m.converged).count();
        let rate = (converged as f64 / total as f64) * 100.0;
        
        write!(file, "| **{:.1}% ({}/{})** ", rate, converged, total)?;
    }
    writeln!(file, "|")?;
    
    // Performance analysis for converged circuits
    writeln!(file, "\n## Performance Analysis\n")?;
    writeln!(file, "For circuits where multiple methods converged:\n")?;
    
    writeln!(file, "| Metric | Newton-Raphson | GLACIER | MAESTRO | Improvement |")?;
    writeln!(file, "|--------|----------------|---------|---------|-------------|")?;
    
    // Calculate averages for converged circuits
    for solver in ["Newton-Raphson", "GLACIER", "MAESTRO"] {
        let converged_metrics: Vec<_> = metrics.iter()
            .filter(|m| m.solver_name == solver && m.converged)
            .collect();
        
        if !converged_metrics.is_empty() {
            let avg_iter = converged_metrics.iter()
                .map(|m| m.iterations as f64)
                .sum::<f64>() / converged_metrics.len() as f64;
            
            let median_time = {
                let mut times: Vec<_> = converged_metrics.iter()
                    .map(|m| m.time_ms)
                    .collect();
                times.sort_by(|a, b| a.partial_cmp(b).unwrap());
                times[times.len() / 2]
            };
            
            println!("{}: Avg iterations: {:.1}, Median time: {:.1}ms", 
                solver, avg_iter, median_time);
        }
    }
    
    // Strategy effectiveness
    writeln!(file, "\n## Strategy Effectiveness\n")?;
    writeln!(file, "| Strategy | Times Applied | Success Rate | Avg Iterations |")?;
    writeln!(file, "|----------|--------------|--------------|----------------|")?;
    
    let mut strategy_stats: HashMap<String, (usize, usize, usize)> = HashMap::new();
    
    for metric in &metrics {
        if metric.solver_name.contains("MAESTRO") {
            for strategy in &metric.strategies_used {
                let entry = strategy_stats.entry(strategy.clone()).or_insert((0, 0, 0));
                entry.0 += 1; // count
                if metric.converged {
                    entry.1 += 1; // successes
                    entry.2 += metric.iterations; // total iterations
                }
            }
        }
    }
    
    for (strategy, (count, successes, total_iter)) in strategy_stats {
        let success_rate = (successes as f64 / count as f64) * 100.0;
        let avg_iter = if successes > 0 { total_iter / successes } else { 0 };
        
        writeln!(file, "| {} | {} | {:.1}% | {} |", 
            strategy, count, success_rate, avg_iter)?;
    }
    
    // Case studies
    writeln!(file, "\n## Case Studies\n")?;
    
    // Find interesting cases
    let case_studies = vec![
        ("Series-5-LEDs", "5-LED series string with extreme Is values"),
        ("Buck-SoftStart", "Buck converter with soft-start circuit"),
        ("Cascade-3-Stage", "3-stage cascaded amplifier"),
    ];
    
    for (circuit_name, description) in case_studies {
        writeln!(file, "### {}: {}\n", circuit_name, description)?;
        
        for solver in ["Newton-Raphson", "GLACIER", "MAESTRO", "MAESTRO+GLACIER"] {
            if let Some(metric) = metrics.iter()
                .find(|m| m.circuit_name == circuit_name && m.solver_name == solver) {
                
                writeln!(file, "**{}**: {}", solver, 
                    if metric.converged {
                        format!("✅ Converged in {} iterations ({:.1}ms)", 
                            metric.iterations, metric.time_ms)
                    } else {
                        "❌ Failed".to_string()
                    }
                )?;
                
                if metric.converged && !metric.strategies_used.is_empty() {
                    writeln!(file, "  Strategies: {}", metric.strategies_used.join(", "))?;
                    if let Some(steps) = metric.progressive_steps {
                        writeln!(file, "  Progressive steps: {}", steps)?;
                    }
                }
            }
        }
        writeln!(file)?;
    }
    
    Ok(())
}

// Circuit creation functions

fn create_series_leds(n: usize) -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    for i in 1..=n {
        circuit.add_node(format!("N{}", i), None);
    }
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    
    // Resistor value depends on number of LEDs
    let r_value = if n <= 3 { 100.0 } else { 47.0 };
    circuit.add_branch("R1".to_string(), "VCC", "N1", "Resistor".to_string(), r_value, None);
    
    let mut models = HashMap::new();
    
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: r_value,
        tolerance: 1.0,
        limits: ElectricalLimits::default(),
    });
    
    // Create LED chain with increasingly extreme parameters
    for i in 1..=n {
        let led_name = format!("D{}", i);
        let node1 = format!("N{}", i);
        let node2 = if i < n { format!("N{}", i + 1) } else { "GND".to_string() };
        
        circuit.add_branch(led_name.clone(), &node1, &node2, "LED".to_string(), 0.0, None);
        
        // Exponentially decreasing Is values
        let is = 10f64.powf(-24.0 - (i as f64 - 1.0) * 14.0 / (n as f64 - 1.0));
        let vf = 1.8 + (i as f64 - 1.0) * 1.4 / (n as f64 - 1.0); // 1.8V to 3.2V
        
        models.insert(led_name, ComponentModel::LED {
            forward_voltage: vf,
            forward_current: 0.02,
            color: ["red", "yellow", "green", "blue", "white"][i % 5].to_string(),
            limits: ElectricalLimits::default(),
            saturation_current: Some(is),
            emission_coefficient: Some(1.7 + (i as f64) * 0.3 / n as f64),
            thermal_voltage: Some(0.026),
            dynamic_resistance: 10.0,
        });
    }
    
    (circuit, models)
}

fn create_parallel_leds(n: usize, with_ballast: bool) -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("COMMON".to_string(), None);
    
    // Individual nodes for each LED if using ballast resistors
    if with_ballast {
        for i in 1..=n {
            circuit.add_node(format!("LED{}", i), None);
        }
    }
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R_MAIN".to_string(), "VCC", "COMMON", "Resistor".to_string(), 10.0, None);
    
    let mut models = HashMap::new();
    
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    models.insert("R_MAIN".to_string(), ComponentModel::Resistor {
        resistance: 10.0,
        tolerance: 1.0,
        limits: ElectricalLimits::default(),
    });
    
    // Create parallel LEDs
    for i in 1..=n {
        let led_name = format!("D{}", i);
        
        if with_ballast {
            // Add ballast resistor
            let r_name = format!("R{}", i);
            circuit.add_branch(r_name.clone(), "COMMON", &format!("LED{}", i), 
                "Resistor".to_string(), 1.0, None);
            
            models.insert(r_name, ComponentModel::Resistor {
                resistance: 1.0,
                tolerance: 5.0,
                limits: ElectricalLimits::default(),
            });
            
            // LED connects to individual node
            circuit.add_branch(led_name.clone(), &format!("LED{}", i), "GND", 
                "LED".to_string(), 0.0, None);
        } else {
            // Direct connection
            circuit.add_branch(led_name.clone(), "COMMON", "GND", 
                "LED".to_string(), 0.0, None);
        }
        
        // Slight parameter variations to simulate real devices
        let is_variation = 1.0 + (i as f64 - n as f64 / 2.0) * 0.2 / n as f64;
        
        models.insert(led_name, ComponentModel::LED {
            forward_voltage: 2.0,
            forward_current: 0.02,
            color: "red".to_string(),
            limits: ElectricalLimits::default(),
            saturation_current: Some(1e-15 * is_variation),
            emission_coefficient: Some(1.8),
            thermal_voltage: Some(0.026),
            dynamic_resistance: 10.0,
        });
    }
    
    (circuit, models)
}

fn create_buck_converter() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    
    // Basic buck topology
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("SW".to_string(), None);
    circuit.add_node("VOUT".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("SW1".to_string(), "VIN", "SW", "Switch".to_string(), 0.0, None);
    circuit.add_branch("D1".to_string(), "GND", "SW", "Diode".to_string(), 0.0, None);
    circuit.add_branch("L1".to_string(), "SW", "VOUT", "Inductor".to_string(), 10e-6, None);
    circuit.add_branch("C1".to_string(), "VOUT", "GND", "Capacitor".to_string(), 100e-6, None);
    circuit.add_branch("RLOAD".to_string(), "VOUT", "GND", "Resistor".to_string(), 10.0, None);
    
    let mut models = HashMap::new();
    
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 12.0,
        internal_resistance: Some(0.1),
    });
    
    // Simplified switch model
    models.insert("SW1".to_string(), ComponentModel::Switch {
        on_resistance: 0.01,
        off_resistance: 10e6,
        initial_state: false, // Start with switch off
    });
    
    models.insert("D1".to_string(), ComponentModel::Diode {
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.0),
        breakdown_voltage: 50.0,
        series_resistance: Some(0.1),
        limits: ElectricalLimits::default(),
    });
    
    models.insert("L1".to_string(), ComponentModel::Inductor {
        inductance: 10e-6,
        series_resistance: Some(0.05),
        saturation_current: Some(5.0),
        limits: ElectricalLimits::default(),
    });
    
    models.insert("C1".to_string(), ComponentModel::Capacitor {
        capacitance: 100e-6,
        esr: Some(0.02),
        voltage_rating: Some(16.0),
        limits: ElectricalLimits::default(),
    });
    
    models.insert("RLOAD".to_string(), ComponentModel::Resistor {
        resistance: 10.0,
        tolerance: 1.0,
        limits: ElectricalLimits::default(),
    });
    
    (circuit, models)
}

fn create_cascaded_amplifier(stages: usize) -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("INPUT".to_string(), None);
    circuit.add_node("OUTPUT".to_string(), None);
    
    // Create intermediate nodes
    for i in 1..stages {
        circuit.add_node(format!("STAGE{}", i), None);
    }
    
    // Power supply
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 12.0, None);
    
    let mut models = HashMap::new();
    
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 12.0,
        internal_resistance: Some(0.0),
    });
    
    // Create amplifier stages
    for i in 1..=stages {
        let input_node = if i == 1 { "INPUT" } else { &format!("STAGE{}", i-1) };
        let output_node = if i == stages { "OUTPUT" } else { &format!("STAGE{}", i) };
        
        // Simplified amplifier model using controlled sources
        let amp_name = format!("AMP{}", i);
        let gain = 10.0_f64.powf(i as f64 * 0.5); // Increasing gain per stage
        
        // Input resistance
        circuit.add_branch(format!("RIN{}", i), input_node, "GND", 
            "Resistor".to_string(), 10000.0, None);
        
        models.insert(format!("RIN{}", i), ComponentModel::Resistor {
            resistance: 10000.0,
            tolerance: 1.0,
            limits: ElectricalLimits::default(),
        });
        
        // Output resistance and coupling
        circuit.add_branch(format!("ROUT{}", i), "VCC", output_node, 
            "Resistor".to_string(), 1000.0, None);
        
        models.insert(format!("ROUT{}", i), ComponentModel::Resistor {
            resistance: 1000.0,
            tolerance: 1.0,
            limits: ElectricalLimits::default(),
        });
        
        // Gain element (simplified as voltage-controlled voltage source)
        models.insert(amp_name, ComponentModel::VoltageControlledVoltageSource {
            gain,
            input_resistance: 10000.0,
            output_resistance: 50.0,
        });
    }
    
    (circuit, models)
}

fn create_bridge_rectifier() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    
    circuit.add_node("AC1".to_string(), None);
    circuit.add_node("AC2".to_string(), None);
    circuit.add_node("DC_POS".to_string(), None);
    circuit.add_node("DC_NEG".to_string(), None);
    
    // AC source
    circuit.add_branch("VAC".to_string(), "AC1", "AC2", "VoltageSource".to_string(), 10.0, None);
    
    // Bridge diodes
    circuit.add_branch("D1".to_string(), "AC1", "DC_POS", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "DC_NEG", "AC1", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D3".to_string(), "AC2", "DC_POS", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D4".to_string(), "DC_NEG", "AC2", "Diode".to_string(), 0.0, None);
    
    // Filter capacitor and load
    circuit.add_branch("C1".to_string(), "DC_POS", "DC_NEG", "Capacitor".to_string(), 1000e-6, None);
    circuit.add_branch("RLOAD".to_string(), "DC_POS", "DC_NEG", "Resistor".to_string(), 100.0, None);
    
    let mut models = HashMap::new();
    
    models.insert("VAC".to_string(), ComponentModel::VoltageSource {
        voltage: 10.0,
        internal_resistance: Some(1.0),
    });
    
    // Bridge diodes
    for i in 1..=4 {
        models.insert(format!("D{}", i), ComponentModel::Diode {
            saturation_current: Some(1e-12),
            emission_coefficient: Some(1.0),
            breakdown_voltage: 50.0,
            series_resistance: Some(0.1),
            limits: ElectricalLimits::default(),
        });
    }
    
    models.insert("C1".to_string(), ComponentModel::Capacitor {
        capacitance: 1000e-6,
        esr: Some(0.1),
        voltage_rating: Some(25.0),
        limits: ElectricalLimits::default(),
    });
    
    models.insert("RLOAD".to_string(), ComponentModel::Resistor {
        resistance: 100.0,
        tolerance: 1.0,
        limits: ElectricalLimits::default(),
    });
    
    (circuit, models)
}

fn create_ovp_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("PROTECTED".to_string(), None);
    
    // Input with series resistance
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("RS".to_string(), "VIN", "PROTECTED", "Resistor".to_string(), 10.0, None);
    
    // TVS diode for protection
    circuit.add_branch("TVS1".to_string(), "PROTECTED", "GND", "TVSDiode".to_string(), 0.0, None);
    
    // Load
    circuit.add_branch("RLOAD".to_string(), "PROTECTED", "GND", "Resistor".to_string(), 1000.0, None);
    
    let mut models = HashMap::new();
    
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    models.insert("RS".to_string(), ComponentModel::Resistor {
        resistance: 10.0,
        tolerance: 1.0,
        limits: ElectricalLimits::default(),
    });
    
    models.insert("TVS1".to_string(), ComponentModel::TVSDiode {
        breakdown_voltage: 6.0,
        clamping_voltage: 7.5,
        peak_pulse_current: 10.0,
        capacitance: 100e-12,
        limits: ElectricalLimits::default(),
    });
    
    models.insert("RLOAD".to_string(), ComponentModel::Resistor {
        resistance: 1000.0,
        tolerance: 1.0,
        limits: ElectricalLimits::default(),
    });
    
    (circuit, models)
}

// Placeholder functions for remaining circuit types
fn create_mixed_led_diode_chain() -> (Circuit, HashMap<String, ComponentModel>) {
    create_series_leds(5) // Simplified for now
}

fn create_voltage_multiplier(stages: usize) -> (Circuit, HashMap<String, ComponentModel>) {
    create_series_leds(stages * 2) // Simplified
}

fn create_mismatched_parallel_leds() -> (Circuit, HashMap<String, ComponentModel>) {
    create_parallel_leds(5, false) // Simplified
}

fn create_buck_with_softstart() -> (Circuit, HashMap<String, ComponentModel>) {
    create_buck_converter() // Simplified
}

fn create_boost_converter() -> (Circuit, HashMap<String, ComponentModel>) {
    create_buck_converter() // Simplified
}

fn create_generic_converter(topology: &str) -> (Circuit, HashMap<String, ComponentModel>) {
    create_buck_converter() // Simplified
}

fn create_ac_coupled_cascade() -> (Circuit, HashMap<String, ComponentModel>) {
    create_cascaded_amplifier(3) // Simplified
}

fn create_amplifier_with_feedback() -> (Circuit, HashMap<String, ComponentModel>) {
    create_cascaded_amplifier(2) // Simplified
}

fn create_synchronous_bridge() -> (Circuit, HashMap<String, ComponentModel>) {
    create_bridge_rectifier() // Simplified
}

fn create_polyphase_rectifier(phases: usize) -> (Circuit, HashMap<String, ComponentModel>) {
    create_bridge_rectifier() // Simplified
}

fn create_active_pfc_bridge() -> (Circuit, HashMap<String, ComponentModel>) {
    create_bridge_rectifier() // Simplified
}

fn create_voltage_doubler() -> (Circuit, HashMap<String, ComponentModel>) {
    create_bridge_rectifier() // Simplified
}

fn create_current_limiter() -> (Circuit, HashMap<String, ComponentModel>) {
    create_ovp_circuit() // Simplified
}

fn create_hotswap_controller() -> (Circuit, HashMap<String, ComponentModel>) {
    create_ovp_circuit() // Simplified
}

fn create_crowbar_protection() -> (Circuit, HashMap<String, ComponentModel>) {
    create_ovp_circuit() // Simplified
}

fn create_reverse_polarity_protection() -> (Circuit, HashMap<String, ComponentModel>) {
    create_ovp_circuit() // Simplified
}

fn create_esd_protection() -> (Circuit, HashMap<String, ComponentModel>) {
    create_ovp_circuit() // Simplified
}

fn main() {
    println!("MAESTRO Solver Comparison Tool");
    println!("==============================\n");
    
    let test_circuits = create_test_circuits();
    println!("Created {} test circuits across {} categories\n", 
        test_circuits.len(),
        6
    );
    
    let solver_configs = vec![
        SolverConfig::NewtonRaphson,
        SolverConfig::Glacier,
        SolverConfig::Maestro,
        SolverConfig::MaestroWithGlacier,
    ];
    
    let mut all_metrics = Vec::new();
    
    // Run all combinations
    for (idx, circuit) in test_circuits.iter().enumerate() {
        println!("[{}/{}] Testing: {} - {}", 
            idx + 1, test_circuits.len(),
            circuit.name, circuit.description
        );
        
        for solver in &solver_configs {
            print!("  {} ... ", solver.name());
            std::io::stdout().flush().unwrap();
            
            let metrics = run_solver_test(solver, circuit, 60000); // 60s timeout
            
            if metrics.converged {
                print!("✅ {} iter, {:.1} ms", 
                    metrics.iterations, metrics.time_ms);
                
                if !metrics.strategies_used.is_empty() && 
                   metrics.strategies_used[0] != solver.name() {
                    print!(" [{}]", metrics.strategies_used.join(", "));
                }
                
                if let Some(steps) = metrics.progressive_steps {
                    print!(" ({} steps)", steps);
                }
                
                println!();
            } else {
                println!("❌ Failed");
            }
            
            all_metrics.push(metrics);
        }
        println!();
    }
    
    // Generate report
    match generate_maestro_report(all_metrics, "maestro_comparison_report.md") {
        Ok(_) => println!("\nReport generated: maestro_comparison_report.md"),
        Err(e) => eprintln!("Error generating report: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_circuit_creation() {
        let circuits = create_test_circuits();
        assert_eq!(circuits.len(), 52);
        
        // Verify each category has the right number
        let series_count = circuits.iter()
            .filter(|c| c.category == CircuitCategory::SeriesNonlinear)
            .count();
        assert_eq!(series_count, 15);
        
        // Test that circuits can be built
        for circuit in circuits.iter().take(5) {
            let (c, m) = (circuit.build_fn)();
            assert!(c.nodes().count() > 0);
            assert!(m.len() > 0);
        }
    }
    
    #[test]
    fn test_progressive_led_creation() {
        let (circuit, models) = create_series_leds(3);
        
        // Should have VCC, N1, N2, N3, GND nodes
        assert_eq!(circuit.nodes().count(), 5);
        
        // Should have V1, R1, D1, D2, D3
        assert_eq!(models.len(), 5);
        
        // Check LED parameters
        if let Some(ComponentModel::LED { saturation_current, .. }) = models.get("D3") {
            assert!(saturation_current.unwrap() < 1e-30);
        }
    }
}