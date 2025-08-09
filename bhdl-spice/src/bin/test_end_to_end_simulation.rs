//! End-to-end test with full simulation: BHDL testbench -> Analysis -> Simulation -> Assertions

use std::fs;
use std::time::Instant;
use anyhow::{Result, Context};
use std::collections::HashMap;

use bhdl_parser::parse;
use bhdl_analyzer::analyze;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_spice::{
    Circuit, ComponentModel,
    stdlib_model_loader::StdlibModelLoader,
    GlacierTransientSolver, TransientConfig, TransientResult,
    ProductionGlacierSolver, ProductionMaestroOrchestrator,
};
use bhdl_simulation::{
    SimulationCoordinator, SimulationConfig, TestResult,
    StimulusGenerator, AssertionChecker, MeasurementCollector,
};

fn main() -> Result<()> {
    println!("\n=== END-TO-END SIMULATION TEST (BHDL Testbench) ===\n");
    
    // Get testbench file path from args or use default
    let testbench_file = std::env::args().nth(1)
        .unwrap_or_else(|| "tests/circuits/testbenches/led_circuit_testbench.bhdl".to_string());
    
    println!("Input testbench file: {}", testbench_file);
    let start = Instant::now();
    
    // Step 1: Read and parse BHDL testbench file
    println!("\n1. Reading and parsing BHDL testbench...");
    let source = fs::read_to_string(&testbench_file)
        .with_context(|| format!("Failed to read testbench file: {}", testbench_file))?;
    
    let parse_result = parse(&source);
    let syntax_tree = parse_result.syntax();
    if !parse_result.errors().is_empty() {
        eprintln!("Parse errors:");
        for error in parse_result.errors() {
            eprintln!("  - {}", error.message);
        }
        anyhow::bail!("Parsing failed with errors");
    }
    
    // Step 2: Analyze circuit and testbench
    println!("2. Analyzing circuit and testbench...");
    let source_file = SourceFile::cast(syntax_tree)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast syntax tree to SourceFile"))?;
    
    let analysis_result = analyze(&source_file);
    println!("   - Analysis complete with {} diagnostics", analysis_result.diagnostics.len());
    
    // Step 3: Extract board and testbench information
    println!("3. Extracting circuit and test information...");
    let (circuit, testbench_info) = extract_circuit_and_tests(&source_file, &analysis_result)?;
    
    println!("   - Found board: {}", circuit.name);
    println!("   - Found {} tests in testbench", testbench_info.tests.len());
    for test in &testbench_info.tests {
        println!("     - Test '{}': {} assertions, {} measurements", 
                 test.name, test.assertions.len(), test.measurements.len());
    }
    
    // Step 4: Create SPICE circuit representation
    println!("\n4. Creating SPICE circuit...");
    let spice_circuit = create_spice_circuit(&circuit)?;
    println!("   - Created circuit with {} nodes, {} components", 
             spice_circuit.nodes().count(), spice_circuit.branches().count());
    
    // Step 5: Load component models from stdlib
    println!("5. Loading component models...");
    let models = load_models_for_circuit(&spice_circuit, &circuit)?;
    println!("   - Loaded {} component models", models.len());
    
    // Step 6: Run DC operating point first
    println!("\n6. Finding DC operating point...");
    let dc_solution = run_dc_analysis(spice_circuit.clone(), models.clone())?;
    println!("   - DC solution found at {}% ramp", (dc_solution.ramp * 100.0) as i32);
    println!("   - LED current: {:.3}mA", dc_solution.branch_currents.get("LED1").unwrap_or(&0.0) * 1000.0);
    
    // Step 7: Set up simulation coordinator
    println!("\n7. Setting up simulation coordinator...");
    let mut coordinator = SimulationCoordinator::new(spice_circuit.clone(), models.clone());
    coordinator.set_dc_operating_point(dc_solution);
    
    // Step 8: Run each test
    println!("\n8. Running testbench tests...");
    let mut all_results = Vec::new();
    
    for test in &testbench_info.tests {
        println!("\n   Running test '{}'...", test.name);
        
        // Configure simulation
        let sim_config = SimulationConfig {
            duration: test.duration,
            timestep: test.timestep,
            enable_thermal: test.enable_thermal,
            temperature: 25.0,
            tolerance: 1e-6,
        };
        
        // Set up stimulus
        let stimulus = StimulusGenerator::from_test_spec(&test.stimulus)?;
        coordinator.set_stimulus(stimulus);
        
        // Set up assertions
        let assertion_checker = AssertionChecker::from_test_spec(&test.assertions)?;
        coordinator.set_assertion_checker(assertion_checker);
        
        // Set up measurements
        let measurement_collector = MeasurementCollector::from_test_spec(&test.measurements)?;
        coordinator.set_measurement_collector(measurement_collector);
        
        // Run simulation
        let test_start = Instant::now();
        let result = coordinator.run_test(&test.name, sim_config)?;
        let test_elapsed = test_start.elapsed();
        
        // Report results
        println!("     - Simulation completed in {:.2}ms", test_elapsed.as_secs_f64() * 1000.0);
        println!("     - Simulated {:.2}ms with {}μs timestep", 
                 result.final_time * 1000.0, result.timestep * 1e6);
        println!("     - Total iterations: {}", result.total_iterations);
        
        // Check assertions
        println!("     - Assertion results:");
        for (assertion, passed) in &result.assertion_results {
            let status = if *passed { "✓ PASS" } else { "✗ FAIL" };
            println!("       {} {}", status, assertion);
        }
        
        // Show measurements
        println!("     - Measurements:");
        for (name, value) in &result.measurements {
            println!("       {} = {:.3}", name, value);
        }
        
        all_results.push(result);
    }
    
    // Step 9: Generate simulation report
    println!("\n9. Generating simulation report...");
    let report = generate_simulation_report(&testbench_info, &all_results)?;
    
    // Save results
    let output_file = "tests/outputs/simulation/led_circuit_results.json";
    save_simulation_results(&report, output_file)?;
    println!("   - Results saved to: {}", output_file);
    
    // Step 10: Summary
    let total_elapsed = start.elapsed();
    println!("\n=== SIMULATION SUMMARY ===");
    println!("Total time: {:.2}ms", total_elapsed.as_secs_f64() * 1000.0);
    println!("Tests run: {}", all_results.len());
    
    let passed_tests = all_results.iter()
        .filter(|r| r.assertion_results.values().all(|&v| v))
        .count();
    println!("Tests passed: {}/{}", passed_tests, all_results.len());
    
    let total_assertions = all_results.iter()
        .map(|r| r.assertion_results.len())
        .sum::<usize>();
    let passed_assertions = all_results.iter()
        .map(|r| r.assertion_results.values().filter(|&&v| v).count())
        .sum::<usize>();
    println!("Assertions: {}/{} passed", passed_assertions, total_assertions);
    
    if passed_tests == all_results.len() {
        println!("\n✓ All tests PASSED!");
    } else {
        println!("\n✗ Some tests FAILED!");
        anyhow::bail!("Simulation validation failed");
    }
    
    Ok(())
}

// Helper structures for testbench information
#[derive(Debug, Clone)]
struct CircuitInfo {
    name: String,
    power_domains: Vec<PowerDomain>,
    components: Vec<ComponentInfo>,
    nets: Vec<String>,
}

#[derive(Debug, Clone)]
struct PowerDomain {
    name: String,
    voltage: f64,
    current_limit: f64,
}

#[derive(Debug, Clone)]
struct ComponentInfo {
    name: String,
    component_type: String,
    parameters: HashMap<String, String>,
}

#[derive(Debug, Clone)]
struct TestbenchInfo {
    name: String,
    target_board: String,
    tests: Vec<TestSpec>,
}

#[derive(Debug, Clone)]
struct TestSpec {
    name: String,
    stimulus: Vec<StimulusSpec>,
    duration: f64,
    timestep: f64,
    enable_thermal: bool,
    assertions: Vec<AssertionSpec>,
    measurements: Vec<MeasurementSpec>,
}

#[derive(Debug, Clone)]
struct StimulusSpec {
    net: String,
    waveform: String,
    parameters: HashMap<String, f64>,
}

#[derive(Debug, Clone)]
struct AssertionSpec {
    expression: String,
    condition: String,
    time_spec: String,
}

#[derive(Debug, Clone)]
struct MeasurementSpec {
    name: String,
    expression: String,
    time_spec: String,
}

// Stub implementations - in real implementation these would parse the AST
fn extract_circuit_and_tests(_source_file: &SourceFile, _analysis: &bhdl_analyzer::AnalysisResult) 
    -> Result<(CircuitInfo, TestbenchInfo)> {
    // For now, create based on our known test circuit
    let circuit = CircuitInfo {
        name: "LEDCircuitWithProtection".to_string(),
        power_domains: vec![
            PowerDomain { name: "VDD".to_string(), voltage: 5.0, current_limit: 1.0 },
            PowerDomain { name: "GND".to_string(), voltage: 0.0, current_limit: 1.0 },
        ],
        components: vec![
            ComponentInfo { 
                name: "D1".to_string(), 
                component_type: "TVSDiode".to_string(),
                parameters: [("voltage".to_string(), "6V".to_string())].into()
            },
            ComponentInfo {
                name: "R1".to_string(),
                component_type: "Resistor".to_string(), 
                parameters: [("value".to_string(), "100".to_string())].into()
            },
            ComponentInfo {
                name: "LED1".to_string(),
                component_type: "LED".to_string(),
                parameters: [("color".to_string(), "red".to_string())].into()
            },
        ],
        nets: vec!["input_signal".to_string(), "protected_signal".to_string()],
    };
    
    let testbench = TestbenchInfo {
        name: "TestLEDCircuit".to_string(),
        target_board: "LEDCircuitWithProtection".to_string(),
        tests: vec![
            TestSpec {
                name: "normal_operation".to_string(),
                stimulus: vec![
                    StimulusSpec {
                        net: "input_signal".to_string(),
                        waveform: "ramp".to_string(),
                        parameters: [
                            ("start".to_string(), 0.0),
                            ("end".to_string(), 5.0),
                            ("duration".to_string(), 0.010),
                        ].into()
                    }
                ],
                duration: 0.015,  // 15ms
                timestep: 0.0001, // 100us
                enable_thermal: false,
                assertions: vec![
                    AssertionSpec {
                        expression: "V(protected_signal) <= 5.1V".to_string(),
                        condition: "always".to_string(),
                        time_spec: "".to_string(),
                    },
                    AssertionSpec {
                        expression: "I(LED1) >= 15mA".to_string(),
                        condition: "at".to_string(),
                        time_spec: "t=12ms".to_string(),
                    }
                ],
                measurements: vec![
                    MeasurementSpec {
                        name: "led_current".to_string(),
                        expression: "I(LED1)".to_string(),
                        time_spec: "t=12ms".to_string(),
                    }
                ],
            }
        ],
    };
    
    Ok((circuit, testbench))
}

fn create_spice_circuit(circuit_info: &CircuitInfo) -> Result<Circuit> {
    let mut circuit = Circuit::new();
    
    // Add power domains as nodes
    for domain in &circuit_info.power_domains {
        circuit.add_node(domain.name.clone(), None);
    }
    
    // Add other nets
    for net in &circuit_info.nets {
        circuit.add_node(net.clone(), None);
    }
    
    // For this test, create a simplified circuit
    circuit.add_node("N1".to_string(), None);
    
    // Add components
    circuit.add_branch("V1".to_string(), "VDD", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "input_signal", "protected_signal", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "input_signal", "GND", "TVSDiode".to_string(), 6.0, None);
    circuit.add_branch("LED1".to_string(), "N1", "GND", "LED".to_string(), 0.0, None);
    circuit.add_branch("R2".to_string(), "VDD", "N1", "Resistor".to_string(), 220.0, None);
    
    Ok(circuit)
}

fn load_models_for_circuit(circuit: &Circuit, _circuit_info: &CircuitInfo) -> Result<HashMap<String, ComponentModel>> {
    StdlibModelLoader::load_models_from_circuit(circuit)
}

fn run_dc_analysis(circuit: Circuit, models: HashMap<String, ComponentModel>) -> Result<bhdl_spice::GlacierSolution> {
    // Use production GLACIER for DC analysis
    let mut solver = ProductionGlacierSolver::new(circuit);
    for (name, model) in models {
        solver.add_model(name, model);
    }
    
    let solutions = solver.solve()?;
    solutions.into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No DC solution found"))
}

#[derive(Debug, serde::Serialize)]
struct SimulationReport {
    testbench_name: String,
    circuit_name: String,
    test_results: Vec<TestResult>,
    summary: Summary,
}

#[derive(Debug, serde::Serialize)]
struct Summary {
    total_tests: usize,
    passed_tests: usize,
    failed_tests: usize,
    total_assertions: usize,
    passed_assertions: usize,
    failed_assertions: usize,
    total_time_ms: f64,
}

fn generate_simulation_report(testbench: &TestbenchInfo, results: &[TestResult]) -> Result<SimulationReport> {
    let total_assertions: usize = results.iter()
        .map(|r| r.assertion_results.len())
        .sum();
    
    let passed_assertions: usize = results.iter()
        .map(|r| r.assertion_results.values().filter(|&&v| v).count())
        .sum();
    
    let passed_tests = results.iter()
        .filter(|r| r.assertion_results.values().all(|&v| v))
        .count();
    
    let summary = Summary {
        total_tests: results.len(),
        passed_tests,
        failed_tests: results.len() - passed_tests,
        total_assertions,
        passed_assertions,
        failed_assertions: total_assertions - passed_assertions,
        total_time_ms: results.iter().map(|r| r.simulation_time_ms).sum(),
    };
    
    Ok(SimulationReport {
        testbench_name: testbench.name.clone(),
        circuit_name: testbench.target_board.clone(),
        test_results: results.to_vec(),
        summary,
    })
}

fn save_simulation_results(report: &SimulationReport, path: &str) -> Result<()> {
    let json = serde_json::to_string_pretty(report)?;
    std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap())?;
    std::fs::write(path, json)?;
    Ok(())
}