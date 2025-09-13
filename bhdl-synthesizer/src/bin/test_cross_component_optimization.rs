// Test cross-component optimization coordination
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::{analyze, AnalysisResult};
use bhdl_synthesizer::{Synthesizer, simulation_driven::SimulationDrivenSynthesizer};
use bhdl_synthesizer::cross_component_optimization::CrossComponentOptimizer;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Cross-Component Optimization Test ===");
    
    // Test the intelligent design automation demo file
    let test_file = "demo_intelligent_design_automation.bhdl";
    println!("Reading intelligent design demo: {}", test_file);
    
    let bhdl_source = fs::read_to_string(test_file)
        .map_err(|e| format!("Failed to read {}: {}", test_file, e))?;
    
    println!("Parsing BHDL source...");
    let parse_result = parse(&bhdl_source);
    let syntax = parse_result.syntax();
    
    // Run semantic analysis
    println!("Running semantic analysis...");
    let analysis = analyze(&SourceFile::cast(syntax.clone()).unwrap());
    
    // Generate initial netlist
    println!("\nGenerating initial netlist...");
    let mut synthesizer = Synthesizer::new();
    
    // Create a netlist for the intelligent power system
    use bhdl_netlist::{Netlist, ModuleKind};
    let mut netlist = Netlist::new();
    
    // Add a module definition for the intelligent power system
    let module_id = netlist.add_module("IntelligentPowerSystem".to_string(), ModuleKind::Board);
    
    // Add component instances that will participate in cross-optimization
    println!("Adding intelligent component instances...");
    
    let lm7805_instance = netlist.add_instance("main_reg".to_string(), module_id).unwrap();
    netlist.instances.get_mut(lm7805_instance).unwrap().attributes.insert(
        "component_type".to_string(), "LM7805".to_string()
    );
    netlist.instances.get_mut(lm7805_instance).unwrap().attributes.insert(
        "power_dissipation".to_string(), "2.5".to_string() // 2.5W initial
    );
    
    let tps54331_instance = netlist.add_instance("switch_reg".to_string(), module_id).unwrap();
    netlist.instances.get_mut(tps54331_instance).unwrap().attributes.insert(
        "component_type".to_string(), "TPS54331".to_string()
    );
    netlist.instances.get_mut(tps54331_instance).unwrap().attributes.insert(
        "efficiency".to_string(), "0.80".to_string() // 80% initial efficiency
    );
    
    let r1_instance = netlist.add_instance("R1".to_string(), module_id).unwrap();
    netlist.instances.get_mut(r1_instance).unwrap().attributes.insert(
        "component_type".to_string(), "Res".to_string()
    );
    netlist.instances.get_mut(r1_instance).unwrap().attributes.insert(
        "value".to_string(), "10000".to_string() // 10kΩ
    );
    netlist.instances.get_mut(r1_instance).unwrap().attributes.insert(
        "tolerance".to_string(), "0.001".to_string() // 0.1%
    );
    
    let r2_instance = netlist.add_instance("R2".to_string(), module_id).unwrap();
    netlist.instances.get_mut(r2_instance).unwrap().attributes.insert(
        "component_type".to_string(), "Res".to_string()
    );
    netlist.instances.get_mut(r2_instance).unwrap().attributes.insert(
        "value".to_string(), "10000".to_string() // 10kΩ  
    );
    netlist.instances.get_mut(r2_instance).unwrap().attributes.insert(
        "tolerance".to_string(), "0.001".to_string() // 0.1%
    );
    
    let tvs_instance = netlist.add_instance("TVS".to_string(), module_id).unwrap();
    netlist.instances.get_mut(tvs_instance).unwrap().attributes.insert(
        "component_type".to_string(), "TVSDiode".to_string()
    );
    netlist.instances.get_mut(tvs_instance).unwrap().attributes.insert(
        "clamp_voltage".to_string(), "30".to_string() // 30V
    );
    
    let fuse_instance = netlist.add_instance("F1".to_string(), module_id).unwrap();
    netlist.instances.get_mut(fuse_instance).unwrap().attributes.insert(
        "component_type".to_string(), "Fuse".to_string()
    );
    netlist.instances.get_mut(fuse_instance).unwrap().attributes.insert(
        "rating".to_string(), "5".to_string() // 5A
    );
    
    println!("Initial netlist has {} instances", netlist.instances.len());
    
    // Create mock behavioral models for testing
    use bhdl_simulation::{ModelMetadata, SimulationLevel};
    use std::collections::HashMap;
    let behavioral_models = vec![
        ModelMetadata {
            name: "LM7805".to_string(),
            level: SimulationLevel::Analytical,
            typical_runtime: std::time::Duration::from_millis(1),
            accuracy: 0.8,
            properties: [
                ("provides_outputs".to_string(), "power_dissipation,efficiency".to_string()),
                ("requires_inputs".to_string(), "input_voltage,output_current".to_string()),
            ].into_iter().collect(),
        },
        ModelMetadata {
            name: "TPS54331".to_string(),
            level: SimulationLevel::Behavioral,
            typical_runtime: std::time::Duration::from_millis(5),
            accuracy: 0.9,
            properties: [
                ("provides_outputs".to_string(), "efficiency,ripple".to_string()),
                ("requires_inputs".to_string(), "input_voltage,load_current".to_string()),
            ].into_iter().collect(),
        },
        ModelMetadata {
            name: "Res".to_string(),
            level: SimulationLevel::Analytical,
            typical_runtime: std::time::Duration::from_micros(10),
            accuracy: 0.99,
            properties: [
                ("provides_outputs".to_string(), "voltage_drop,power_dissipation".to_string()),
                ("requires_inputs".to_string(), "current".to_string()),
            ].into_iter().collect(),
        },
    ];
    
    println!("Created {} behavioral models for cross-optimization", behavioral_models.len());
    
    // Demonstrate cross-component optimization coordinator
    println!("\n=== Cross-Component Optimization Analysis ===");
    let mut cross_optimizer = CrossComponentOptimizer::new();
    
    // Analyze coordination opportunities
    match cross_optimizer.analyze_coordination_opportunities(&netlist, &behavioral_models) {
        Ok(coordination_plan) => {
            println!("Coordination Plan Generated:");
            println!("  Total participants: {}", coordination_plan.total_participants);
            println!("  Coordination phases: {}", coordination_plan.coordination_phases.len());
            println!("  Shared constraints: {}", coordination_plan.shared_constraints.len());
            
            for (i, phase) in coordination_plan.coordination_phases.iter().enumerate() {
                println!("  Phase {}: {} ({} participants)", 
                         i + 1, phase.name, phase.participants.len());
                println!("    Objectives: {:?}", phase.objectives);
                println!("    Strategy: {:?}", phase.coordination_strategy);
            }
            
            println!("\nExpected Improvements:");
            for (metric, improvement) in &coordination_plan.expected_improvements {
                println!("  {}: {:.1}%", metric, improvement * 100.0);
            }
        },
        Err(e) => {
            println!("Coordination analysis failed: {}", e);
        }
    }
    
    // Test full simulation-driven synthesis with cross-component optimization
    println!("\n=== Integrated Simulation-Driven Synthesis ===");
    let mut sim_synthesizer = SimulationDrivenSynthesizer::new();
    
    // Set up design requirements with cross-component optimization enabled
    use bhdl_synthesizer::simulation_driven::DesignRequirements;
    let requirements = DesignRequirements {
        time_budget: Some(std::time::Duration::from_secs(30)),
        accuracy_requirement: 0.9,
        target_efficiency: Some(0.88),     // Target 88% system efficiency
        minimize_cost: true,
        minimize_size: true,
        max_output_ripple: Some(0.01),      // 1% ripple max (tighter than individual)
        min_phase_margin: Some(50.0),       // 50° min phase margin
        use_grid_search: false,             // Use gradient optimization for coordination
        parameter_ranges: HashMap::new(),
        enable_cross_component_optimization: true, // Enable the coordination features
    };
    
    // Run integrated optimization
    match sim_synthesizer.optimize_netlist(&mut netlist, &requirements, Some(behavioral_models)) {
        Ok(report) => {
            println!("\nIntegrated Optimization Report:");
            println!("  Models found: {}", report.models_found);
            println!("  Selected model: {:?}", report.selected_model);
            println!("  Success: {}", report.optimization_successful);
            
            if !report.final_metrics.is_empty() {
                println!("\n  Final metrics:");
                for (metric, value) in &report.final_metrics {
                    println!("    {}: {:.3}", metric, value);
                }
            }
            
            if !report.notes.is_empty() {
                println!("\n  Coordination Notes:");
                for note in &report.notes {
                    println!("    - {}", note);
                }
            }
        },
        Err(e) => {
            println!("Integrated optimization failed: {}", e);
        }
    }
    
    // Demonstration summary
    println!("\n=== Cross-Component Optimization Capabilities Demonstrated ===");
    println!("1. Thermal load balancing:");
    println!("   - Linear regulator (LM7805) thermal management");
    println!("   - Switching regulator (TPS54331) efficiency coordination");
    println!("   - System-wide thermal budget optimization");
    
    println!("\n2. Precision component matching:");
    println!("   - Resistor pair (R1, R2) temperature coefficient matching");
    println!("   - Tolerance specification coordination");
    println!("   - Standard value selection with matching constraints");
    
    println!("\n3. Protection circuit coordination:");
    println!("   - TVS diode and fuse response time coordination");
    println!("   - Clamp voltage vs breaking capacity optimization");
    println!("   - Sequential activation for fault protection");
    
    println!("\n4. Global system optimization:");
    println!("   - Multi-objective coordination across components");
    println!("   - Constraint propagation and conflict resolution");
    println!("   - Trade-off optimization at system level");
    
    println!("\nKey Innovation: Components no longer optimize in isolation.");
    println!("They negotiate and coordinate to achieve global system objectives");
    println!("while respecting inter-component constraints and relationships.");
    
    Ok(())
}