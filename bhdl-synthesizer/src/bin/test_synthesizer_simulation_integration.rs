use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_synthesizer::import_preprocessor::preprocess_and_analyze;
use bhdl_synthesizer::NetlistGenerator;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("🔍 Testing Real Synthesizer Simulation Integration");
    println!("==================================================\n");
    
    // Simple circuit that should trigger simulation-based calculations
    let test_code = r#"
import { TPS54331 } from "bhdl-stdlib/components/power/switching_regulators/TPS54331.bhdl";

board SimulationTestBoard {
    power VIN = 12V @ 3A;
    power VOUT_5V = 5V @ 2A;
    ground GND;
    
    // TPS54331 buck converter - should trigger simulation
    U1: TPS54331(vout=5V);
    
    // Simple connections that should have simulation data
    VIN -> U1.VIN;
    U1.GND -> GND;
    U1.EN -> VIN;
    U1.VOUT -> @VOUT_5V;
    
    // Current limiting resistor - should use simulation for calculation
    R1: Res(150);
    VIN -> R1.1;
    R1.2 -> LED1: LED(red).A;
    LED1.K -> GND;
    
    // Decoupling capacitor - should use simulation for ESR/voltage rating
    C1: Cap(100µF, voltage=25V);
    VIN -> C1.1;
    C1.2 -> GND;
}
"#;
    
    println!("📄 Test Circuit (Buck Converter + LED + Components):");
    println!("{}", test_code);
    
    // Parse and analyze
    let parse_result = parse(test_code);
    if !parse_result.errors().is_empty() {
        println!("❌ Parse errors:");
        for error in parse_result.errors() {
            println!("  - {}", error.message);
        }
        return Ok(());
    }
    
    let syntax = parse_result.syntax();
    let source_file = SourceFile::cast(syntax).expect("Failed to cast to SourceFile");
    
    // Pre-process imports and run analysis with simulation
    println!("\n🧠 Running Analysis with Unified Simulation...");
    let base_path = "/Users/girivs/src/bhdl-new";
    let (analysis_result, preprocessor) = preprocess_and_analyze(&source_file, base_path)?;
    
    println!("Analysis Results:");
    println!("  - Total diagnostics: {}", analysis_result.diagnostics.len());
    println!("  - Imported entities: {}", preprocessor.imported_entities().len());
    
    // Check if unified simulation ran
    println!("\n📊 Unified Simulation Status:");
    println!("  - Engines: {}", analysis_result.simulation_data.simulation_metadata.engines_used.len());
    println!("  - Confidence: {:.1}%", analysis_result.simulation_data.simulation_metadata.simulation_accuracy.confidence_level * 100.0);
    println!("  - Simulation time: {:.1}ms", analysis_result.simulation_data.simulation_metadata.simulation_time_ms);
    
    // Check what simulation data is available
    println!("\n🔬 Available Simulation Data:");
    if let Some(ref dc_analysis) = analysis_result.simulation_data.dc_analysis {
        println!("  ✅ DC Analysis: {} node voltages, {} branch currents", 
                dc_analysis.node_voltages.len(), dc_analysis.branch_currents.len());
        
        // Show specific simulation results
        for (component, voltage) in &dc_analysis.node_voltages {
            println!("    - {}: {:.3}V", component, voltage);
        }
        for (component, current) in &dc_analysis.branch_currents {
            println!("    - {}: {:.3}A", component, current);
        }
    } else {
        println!("  ❌ No DC Analysis data");
    }
    
    if let Some(ref safety) = analysis_result.simulation_data.electrical_safety {
        println!("  ✅ Electrical Safety: {} component stress analyses", safety.component_stress.len());
        println!("    - Total violations: {}", safety.safety_summary.total_violations);
        for component in &safety.safety_summary.components_needing_derating {
            println!("      - {} needs derating", component);
        }
    } else {
        println!("  ❌ No Electrical Safety data");
    }
    
    if let Some(ref thermal) = analysis_result.simulation_data.thermal_analysis {
        println!("  ✅ Thermal Analysis: {} component temperatures", thermal.component_temperatures.len());
        println!("    - Ambient: {}°C", thermal.ambient_temperature);
        for (component, temp) in &thermal.component_temperatures {
            println!("      - {}: {:.1}°C", component, temp);
        }
    } else {
        println!("  ❌ No Thermal Analysis data");
    }
    
    // Generate netlist with synthesizer
    println!("\n⚙️  Generating Netlist with Real Synthesizer...");
    let mut generator = NetlistGenerator::new();
    generator.set_import_preprocessor(preprocessor);
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis_result).await?;
    
    println!("✅ Netlist generation completed");
    println!("  - Modules: {}", netlist.modules.len());
    println!("  - Instances: {}", netlist.instances.len());
    println!("  - Nets: {}", netlist.nets.len());
    
    // Check if synthesizer actually used simulation data
    println!("\n🎯 Checking if Synthesizer Used Simulation Data...");
    
    // Look for evidence of simulation-based calculations
    let mut found_simulation_usage = false;
    
    for (instance_id, instance) in &netlist.instances {
        // Get module definition to find the component type
        let component_type = if let Some(module_def) = netlist.modules.get(instance.definition) {
            &module_def.name
        } else {
            "unknown"
        };
        
        println!("  Instance: {} ({})", instance.name, component_type);
        
        // Check if instance has simulation-derived attributes
        for (attr_name, attr_value) in &instance.attributes {
            println!("    - {}: {:?}", attr_name, attr_value);
            
            // Look for attributes that would come from simulation
            if attr_name.contains("derating") || 
               attr_name.contains("operating") ||
               attr_name.contains("simulation") ||
               attr_name.contains("stress") {
                found_simulation_usage = true;
                println!("      *** SIMULATION-DERIVED ATTRIBUTE! ***");
            }
        }
        
        // Skip detailed pin analysis due to complex API
        // Focus on checking if simulation data exists and netlist was generated
    }
    
    // Also check netlist metadata
    if let Some(ref metadata) = netlist.analysis_data {
        println!("  ✅ Netlist has analysis data attached");
        if metadata.symbol_data.len() > 0 {
            println!("    - Symbol data: {} entries", metadata.symbol_data.len());
        }
        if metadata.module_definitions.len() > 0 {
            println!("    - Module definitions: {} entries", metadata.module_definitions.len());
        }
    } else {
        println!("  ❌ No analysis data attached to netlist");
    }
    
    println!("\n📋 FINAL VERDICT:");
    println!("==================");
    
    let simulation_ran = analysis_result.simulation_data.simulation_metadata.engines_used.len() > 0;
    let has_simulation_data = analysis_result.simulation_data.dc_analysis.is_some() || 
                             analysis_result.simulation_data.electrical_safety.is_some() ||
                             analysis_result.simulation_data.thermal_analysis.is_some();
    
    println!("✅ Unified Simulation Ran: {}", simulation_ran);
    println!("✅ Simulation Data Available: {}", has_simulation_data);
    println!("❓ Synthesizer Used Simulation Data: {}", found_simulation_usage);
    
    if simulation_ran && has_simulation_data && found_simulation_usage {
        println!("\n🎉 SUCCESS: Complete simulation-to-synthesis integration working!");
    } else if simulation_ran && has_simulation_data {
        println!("\n⚠️  PARTIAL: Simulation runs but synthesizer may not be using all data");
        println!("   This could mean:");
        println!("   - Simulation data exists but isn't being applied to component selection");
        println!("   - Component calculations aren't calling simulation-based methods");
        println!("   - Derating/stress analysis isn't being transferred to netlist attributes");
    } else {
        println!("\n❌ ISSUE: Simulation or synthesis pipeline has gaps");
    }
    
    Ok(())
}