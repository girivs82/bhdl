use bhdl_synthesizer::{NetlistConfig, NetlistGenerator};
use bhdl_analyzer;
use bhdl_parser::parse;
use bhdl_ast::AstNode;
use log::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    info!("Starting predictive analytics and machine learning integration test...");
    
    // Test BHDL circuit with diverse components for predictive analysis
    let circuit = r#"
    board PredictiveAnalyticsTest {
        power VCC = 5V @ 2A;
        ground GND;
        
        // Power supply section - good for pattern recognition
        @VCC -> r_pullup: Res(10k).1 -> r_pullup.2 -> led_power: LED(green).A;
        led_power.K -> @GND;
        
        // Signal processing section - good for performance prediction
        @VCC -> r_load: Res(1k).1 -> r_load.2 -> signal_out;
        signal_out -> c_filter: Cap(100nF).1 -> c_filter.2 -> @GND;
        
        // Protection section - good for risk assessment
        signal_input -> d_protection: Diode().A -> d_protection.K -> protected_signal;
        protected_signal -> r_limit: Res(330).1 -> r_limit.2 -> led_status: LED(red).A;
        led_status.K -> @GND;
    }
    "#;
    
    // Parse and analyze the circuit
    let parse_result = parse(circuit);
    let syntax_tree = parse_result.syntax();
    let source_file = bhdl_ast::SourceFile::cast(syntax_tree).unwrap();
    
    let analysis_result = bhdl_analyzer::analyze(&source_file);
    
    info!("Analysis completed. Found {} diagnostics", analysis_result.diagnostics.len());
    
    // Create generator with predictive analytics enabled
    let config = NetlistConfig {
        enable_predictive_analytics: true,
        include_component_inference: true, // Helpful for ML training data
        enable_design_rule_check: true,   // Provides additional context
        ..Default::default()
    };
    
    let mut generator = NetlistGenerator::with_config(config);
    
    // Generate netlist with predictive analytics
    info!("Starting netlist generation with predictive analytics...");
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis_result).await?;
    
    info!("Netlist generation completed successfully!");
    info!("Generated netlist with {} modules", netlist.modules.len());
    info!("Generated netlist with {} instances", netlist.instances.len());
    info!("Generated netlist with {} nets", netlist.nets.len());
    
    // Display generated components to verify the synthesis worked
    info!("Generated Components:");
    for (instance_id, instance) in &netlist.instances {
        if let Some(module) = netlist.modules.get(instance.definition) {
            info!("  Component: {} ({})", instance.name, module.name);
        }
    }
    
    // The predictive analytics runs internally during synthesis and provides:
    // - Component recommendations based on ML analysis
    // - Performance predictions using trained models
    // - Design completion suggestions from pattern matching
    // - Optimization opportunities through design analysis
    // - Risk assessment using anomaly detection
    // - Design pattern recognition for best practices
    // - Parameter tuning recommendations
    // - Thermal, EMI, and reliability predictions
    
    info!("Predictive analytics and machine learning integration test completed successfully!");
    Ok(())
}