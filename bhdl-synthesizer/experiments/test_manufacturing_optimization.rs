use bhdl_synthesizer::{NetlistConfig, NetlistGenerator};
use bhdl_analyzer;
use bhdl_parser::parse;
use bhdl_ast::AstNode;
use log::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    info!("Starting manufacturing and assembly optimization (DFM/DFA) test...");
    
    // Test BHDL circuit with various component types for manufacturing analysis
    let circuit = r#"
    // Define all components locally for testing
    entity Res(value: resistance) {
        pin 1: signal inout;
        pin 2: signal inout;
    }
    
    entity Cap(value: capacitance) {
        pin 1: signal inout;
        pin 2: signal inout;
    }
    
    entity LED(color: string) {
        pin A: signal in;
        pin K: signal out;
    }

    entity Diode() {
        pin A: signal in;
        pin K: signal out;
    }
    
    entity IC_MCU() {
        pin VDD: power in;
        pin VSS: ground in;
    }
    
    entity Res_TH(value: resistance) {
        pin 1: signal inout;
        pin 2: signal inout;
    }
    
    board ManufacturingTest {
        power VCC = 5V @ 3A;
        ground GND;
        
        // SMT section - standard assembly
        @VCC -> r1: Res(10k).1 -> r1.2 -> led1: LED(green).A;
        led1.K -> @GND;
        
        // Fine pitch components - requires special handling
        @VCC -> c1: Cap(100nF).1 -> c1.2 -> @GND;
        @VCC -> c2: Cap(10uF).1 -> c2.2 -> @GND;
        
        // High value components - critical for manufacturing
        @VCC -> ic1_power;
        ic1_power -> u1: IC_MCU().VDD;
        u1.VSS -> @GND;
        
        // Mixed technology warning (through-hole)
        @VCC -> r_th: Res_TH(1k).1 -> r_th.2 -> test_point;
        
        // Protection components
        input_signal -> d1: Diode().A -> d1.K -> protected;
        protected -> r2: Res(330).1 -> r2.2 -> led2: LED(red).A;
        led2.K -> @GND;
        
        // Multiple resistor values for consolidation analysis
        @VCC -> r3: Res(1k).1 -> r3.2 -> net1;
        @VCC -> r4: Res(1.1k).1 -> r4.2 -> net2;
        @VCC -> r5: Res(1.2k).1 -> r5.2 -> net3;
        @VCC -> r6: Res(4.7k).1 -> r6.2 -> net4;
        @VCC -> r7: Res(4.99k).1 -> r7.2 -> net5;
        @VCC -> r8: Res(5.1k).1 -> r8.2 -> net6;
        @VCC -> r9: Res(10k).1 -> r9.2 -> net7;
        @VCC -> r10: Res(10.2k).1 -> r10.2 -> net8;
    }
    "#;
    
    // Parse and analyze the circuit
    let parse_result = parse(circuit);
    let syntax_tree = parse_result.syntax();
    let source_file = bhdl_ast::SourceFile::cast(syntax_tree).unwrap();
    
    let analysis_result = bhdl_analyzer::analyze(&source_file);
    
    info!("Analysis completed. Found {} diagnostics", analysis_result.diagnostics.len());
    
    // Create generator with manufacturing optimization enabled
    let config = NetlistConfig {
        enable_manufacturing_optimization: true,
        include_component_inference: true,
        enable_design_rule_check: true,
        ..Default::default()
    };
    
    let mut generator = NetlistGenerator::with_config(config);
    
    // Generate netlist with manufacturing optimization
    info!("Starting netlist generation with manufacturing optimization...");
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
    
    // The manufacturing optimization runs internally during synthesis and provides:
    // - DFM (Design for Manufacturability) score
    // - DFA (Design for Assembly) score
    // - Production yield estimation
    // - Unit cost estimation with volume discounts
    // - Manufacturing rule violations
    // - Assembly warnings and suggestions
    // - Panelization optimization
    // - Test coverage analysis
    // - Assembly sequence optimization
    // - Critical component identification
    
    info!("Manufacturing and assembly optimization test completed successfully!");
    Ok(())
}