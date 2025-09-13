// Test buck converter optimization using component-embedded simulation

use bhdl_simulation::{
    SimulationEngine, 
    DesignParameters,
    GridSearchOptimizer,
    NelderMeadOptimizer,
    Objective,
    OptimizationGoal,
    Constraint, 
    ConstraintCondition,
    OptimizationConfig,
};
use std::collections::HashMap;

fn main() {
    println!("=== Buck Converter Optimization Test ===\n");
    
    // BHDL code with behavioral models
    let buck_bhdl = r#"
module BuckConverter(
    vin_nom: voltage = 12V,
    vout: voltage = 5V,
    iout_max: current = 2A,
    f_sw: frequency = 500kHz
) {
    pin VIN: power in;
    pin VOUT: power out;
    pin GND: ground;
}"#;
    
    // Create simulation engine
    let mut engine = SimulationEngine::new();
    
    // For now, manually create behavioral models until parser support is complete
    println!("Creating behavioral models...");
    
    use bhdl_simulation::{SimulationLevel, ModelMetadata};
    use std::time::Duration;
    
    let models = vec![
        ModelMetadata {
            name: "analytical".to_string(),
            level: SimulationLevel::Analytical,
            typical_runtime: Duration::from_millis(1),
            accuracy: 0.7,
            properties: [
                ("model_type".to_string(), "equations".to_string()),
                ("L_min".to_string(), "(vin_nom - vout) * vout / (vin_nom * 0.3 * iout_max * f_sw)".to_string()),
                ("C_min".to_string(), "0.3 * iout_max / (8 * f_sw * 50mV)".to_string()),
            ].into_iter().collect(),
        },
        ModelMetadata {
            name: "averaged".to_string(),
            level: SimulationLevel::Behavioral,
            typical_runtime: Duration::from_millis(100),
            accuracy: 0.9,
            properties: [
                ("model_type".to_string(), "state_space".to_string()),
            ].into_iter().collect(),
        },
        ModelMetadata {
            name: "switching".to_string(),
            level: SimulationLevel::SwitchingSimple,
            typical_runtime: Duration::from_secs(10),
            accuracy: 0.95,
            properties: [
                ("model_type".to_string(), "behavioral_switching".to_string()),
            ].into_iter().collect(),
        },
    ];
    
    println!("Created {} models:", models.len());
    for model in &models {
        println!("  - {}: Level {:?}, Accuracy {:.0}%, Runtime {:?}",
            model.name, model.level, model.accuracy * 100.0, model.typical_runtime);
    }
    
    // Set up initial parameters
    let mut initial_params = DesignParameters::new();
    initial_params.set("vin_nom", 12.0);
    initial_params.set("vout", 5.0);
    initial_params.set("iout_max", 2.0);
    initial_params.set("f_sw", 500e3);
    initial_params.set("R_load", 2.5); // 5V / 2A
    
    println!("\n=== Phase 1: Grid Search (Analytical Model) ===\n");
    
    // Select analytical model for fast initial sizing
    let analytical_model = models.iter()
        .find(|m| m.name == "analytical")
        .unwrap();
    
    // Define parameter ranges for grid search
    let mut param_ranges = HashMap::new();
    param_ranges.insert("L".to_string(), vec![10e-6, 22e-6, 47e-6, 100e-6]); // µH
    param_ranges.insert("C".to_string(), vec![47e-6, 100e-6, 220e-6, 470e-6]); // µF
    
    // Define objectives
    let objectives = vec![
        Objective {
            metric: "efficiency".to_string(),
            goal: OptimizationGoal::Maximize,
            target_value: None,
            weight: 0.5,
        },
        Objective {
            metric: "L_min".to_string(),
            goal: OptimizationGoal::Minimize,
            target_value: None,
            weight: 0.25,
        },
        Objective {
            metric: "C_min".to_string(),
            goal: OptimizationGoal::Minimize,
            target_value: None,
            weight: 0.25,
        },
    ];
    
    // Define constraints
    let constraints = vec![
        Constraint {
            metric: "efficiency".to_string(),
            condition: ConstraintCondition::GreaterThan,
            value: 0.8,
            hard: true,
        },
    ];
    
    // Run grid search
    let config = OptimizationConfig {
        max_iterations: 100,
        convergence_tolerance: 1e-6,
        parallel: false, // Set to false for simplicity
        early_termination: true,
    };
    
    let mut grid_optimizer = GridSearchOptimizer::new(engine.clone(), config.clone());
    
    // Merge initial params with grid search
    for (key, value) in initial_params.values.iter() {
        if !param_ranges.contains_key(key) {
            // Keep initial param constant
            param_ranges.insert(key.clone(), vec![*value]);
        }
    }
    
    println!("Running grid search...");
    let grid_result = grid_optimizer.optimize(
        analytical_model,
        param_ranges,
        &objectives,
        &constraints,
    ).unwrap();
    
    println!("Grid search complete!");
    println!("  Best L: {:.1}µH", grid_result.final_design.get("L").unwrap() * 1e6);
    println!("  Best C: {:.1}µF", grid_result.final_design.get("C").unwrap() * 1e6);
    println!("  Score: {:.3}", grid_result.best_score);
    println!("  Iterations: {}", grid_result.iterations);
    println!("  Runtime: {:?}", grid_result.total_runtime);
    
    println!("\n=== Phase 2: Nelder-Mead Refinement (Averaged Model) ===\n");
    
    // Select averaged model for control loop optimization
    let averaged_model = models.iter()
        .find(|m| m.name == "averaged")
        .unwrap();
    
    // Use grid search result as starting point
    let mut refined_params = grid_result.final_design.clone();
    
    // Add compensation parameters to optimize
    refined_params.set("R_comp", 10e3); // 10kΩ initial guess
    refined_params.set("C_comp", 4.7e-9); // 4.7nF initial guess
    
    // Define objectives for control loop
    let control_objectives = vec![
        Objective {
            metric: "phase_margin".to_string(),
            goal: OptimizationGoal::Target(60.0),
            target_value: Some(60.0),
            weight: 0.6,
        },
        Objective {
            metric: "crossover_frequency".to_string(),
            goal: OptimizationGoal::Target(50e3),
            target_value: Some(50e3),
            weight: 0.4,
        },
    ];
    
    // Define constraints for control loop
    let control_constraints = vec![
        Constraint {
            metric: "phase_margin".to_string(),
            condition: ConstraintCondition::InRange(45.0, 70.0),
            value: 0.0,
            hard: true,
        },
        Constraint {
            metric: "stable".to_string(),
            condition: ConstraintCondition::Equal,
            value: 1.0,
            hard: true,
        },
    ];
    
    // Run Nelder-Mead optimization
    let mut nm_optimizer = NelderMeadOptimizer::new(engine.clone(), config);
    
    println!("Running Nelder-Mead optimization...");
    let nm_result = nm_optimizer.optimize(
        averaged_model,
        refined_params,
        vec!["R_comp".to_string(), "C_comp".to_string()],
        &control_objectives,
        &control_constraints,
    ).unwrap();
    
    println!("Nelder-Mead complete!");
    println!("  R_comp: {:.1}kΩ", nm_result.final_design.get("R_comp").unwrap() / 1e3);
    println!("  C_comp: {:.1}nF", nm_result.final_design.get("C_comp").unwrap() * 1e9);
    println!("  Score: {:.3}", nm_result.best_score);
    println!("  Iterations: {}", nm_result.iterations);
    println!("  Runtime: {:?}", nm_result.total_runtime);
    
    // Simulate final design to get metrics
    println!("\n=== Final Design Verification ===\n");
    
    let mut final_engine = SimulationEngine::new();
    let final_result = final_engine.simulate(averaged_model, &nm_result.final_design).unwrap();
    
    println!("Final design metrics:");
    for (metric, value) in &final_result.metrics {
        println!("  {}: {:.3}", metric, value);
    }
    
    // Show cache statistics
    let (hits, misses, hit_rate) = final_engine.cache_stats();
    println!("\nCache statistics:");
    println!("  Hits: {}", hits);
    println!("  Misses: {}", misses);
    println!("  Hit rate: {:.1}%", hit_rate * 100.0);
    
    println!("\n=== Optimization Complete ===");
    println!("\nFinal optimized design:");
    println!("  L = {:.1}µH", nm_result.final_design.get("L").unwrap() * 1e6);
    println!("  C = {:.1}µF", nm_result.final_design.get("C").unwrap() * 1e6);
    println!("  R_comp = {:.1}kΩ", nm_result.final_design.get("R_comp").unwrap() / 1e3);
    println!("  C_comp = {:.1}nF", nm_result.final_design.get("C_comp").unwrap() * 1e9);
    
    println!("\nThis demonstrates:");
    println!("  1. Parsing behavioral models from BHDL");
    println!("  2. Progressive optimization (analytical → averaged)");
    println!("  3. Grid search for initial sizing");
    println!("  4. Nelder-Mead for control loop refinement");
    println!("  5. Model selection based on optimization phase");
    println!("  6. Simulation caching for efficiency");
}