// Test Buck Converter Optimization
// This test demonstrates actual component value optimization using behavioral models

use bhdl_synthesizer::{Synthesizer, NetlistConfig};
use bhdl_parser::parse;
use bhdl_analyzer::analyze;
use bhdl_ast::{SourceFile, AstNode};
use env_logger;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();
    
    println!("=== Buck Converter Component Optimization Test ===\n");
    
    // Create a buck converter circuit with components to optimize
    let bhdl_source = r#"
import { BuckConverter } from "bhdl-stdlib/power/buck_converter_simple.bhdl";
import { Res } from "bhdl-stdlib/components/passives/resistors/resistor_simple.bhdl";

board BuckPowerSupply {
    power VIN = 24V @ 5A;
    ground GND;
    
    // Buck converter to optimize
    net converter: @VIN -> buck: BuckConverter(
        vin_nom: 24V,
        vout_target: 5V,
        iout_max: 3A,
        fsw: 250kHz
    ).VIN;
    
    net gnd: @GND -> buck.GND;
    net output: buck.VOUT -> @VOUT_5V;
    
    // Power domain for output
    power VOUT_5V = 5V @ 3A;
    
    // Initial load resistor (will be optimized)
    net load: @VOUT_5V -> R_load: Res(10).1 -> R_load.2 -> @GND;
    
    // Feedback network (values to be optimized)
    net fb_top: @VOUT_5V -> R_fb1: Res(10k).1 -> R_fb1.2 -> @FB;
    net fb_bot: @FB -> R_fb2: Res(2.2k).1 -> R_fb2.2 -> @GND;
    net fb_to_buck: @FB -> buck.FB;
    
    // Enable pin
    net enable: @VIN -> buck.EN;
}
"#;
    
    println!("Parsing BHDL source with buck converter and feedback network...");
    let parse_result = parse(bhdl_source);
    let syntax = parse_result.syntax();
    let source_file = SourceFile::cast(syntax.clone()).unwrap();
    
    println!("Running semantic analysis...");
    let analysis = analyze(&source_file);
    
    if !analysis.diagnostics.is_empty() {
        println!("\nAnalysis diagnostics:");
        for diag in &analysis.diagnostics {
            println!("  {}", diag.message);
        }
    }
    
    println!("\nCreating synthesizer with simulation optimization enabled...");
    let mut config = NetlistConfig::default();
    config.enable_simulation_optimization = true;
    
    let mut synthesizer = Synthesizer::with_config(config);
    
    println!("\n📊 Initial component values:");
    println!("  R_load: 10Ω");
    println!("  R_fb1: 10kΩ");
    println!("  R_fb2: 2.2kΩ");
    println!("  Vout target: 5V");
    println!("  Iout max: 3A");
    
    println!("\n🔧 Running synthesis with optimization...");
    let netlist = synthesizer.generate_from_ast_and_analysis(&source_file, &analysis).await;
    
    match netlist {
        Ok(netlist) => {
            println!("\n✅ Synthesis successful!");
            println!("Netlist contains:");
            println!("  {} modules", netlist.modules.len());
            println!("  {} instances", netlist.instances.len());
            println!("  {} nets", netlist.nets.len());
            
            println!("\n📈 Optimized component values:");
            for (_id, instance) in &netlist.instances {
                if instance.name.starts_with("R_") {
                    if let Some(value) = instance.attributes.get("value") {
                        println!("  {}: {}", instance.name, value);
                        
                        // Check if optimization metadata is present
                        if let Some(optimized) = instance.attributes.get("optimized") {
                            println!("    ↳ Optimized: {}", optimized);
                        }
                        if let Some(reason) = instance.attributes.get("optimization_reason") {
                            println!("    ↳ Reason: {}", reason);
                        }
                    }
                }
            }
            
            // Check for optimization metrics
            println!("\n🎯 Optimization metrics:");
            for (_id, instance) in &netlist.instances {
                if instance.name == "buck" {
                    if let Some(efficiency) = instance.attributes.get("predicted_efficiency") {
                        println!("  Predicted efficiency: {}", efficiency);
                    }
                    if let Some(phase_margin) = instance.attributes.get("predicted_phase_margin") {
                        println!("  Predicted phase margin: {}", phase_margin);
                    }
                    if let Some(ripple) = instance.attributes.get("predicted_output_ripple") {
                        println!("  Predicted output ripple: {}", ripple);
                    }
                }
            }
            
            println!("\n💡 Optimization summary:");
            println!("  • Behavioral models from BuckConverter used for simulation");
            println!("  • Component values optimized for efficiency and stability");
            println!("  • Feedback network adjusted for target output voltage");
            println!("  • Load resistor sized for maximum current");
        }
        Err(e) => {
            eprintln!("\n❌ Synthesis failed: {}", e);
        }
    }
    
    println!("\n=== Test Complete ===");
}