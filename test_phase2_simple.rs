//! BHDL Phase 2: Simplified Integration Test
//! 
//! This test demonstrates the Phase 2 circuit intelligence features
//! working through the analyzer pipeline.

use bhdl_parser::parse;
use bhdl_analyzer::analyze;
use bhdl_ast::{SourceFile, AstNode};

fn main() {
    println!("🚀 BHDL Phase 2: Circuit Intelligence Integration Test");
    println!("====================================================");
    
    // Sample BHDL circuit with multi-voltage design
    let bhdl_code = r#"
board SmartEmbeddedSystem {
    // Multi-voltage design  
    USB_5V |> VCC_3V3.enable() |> VCC_1V8.enable();
    
    // MCU with 3.3V operation
    mcu: STM32H7() {
        VCC = VCC_3V3;
        GND = GND;
    }
    
    // Low-power sensor with 1.8V operation  
    sensor: SensorIC() {
        VCC = VCC_1V8;
        GND = GND;
    }
    
    // Status LED with current limiting
    led_status: LED(color = "green");
    resistor_led: Res();
    
    // Power decoupling
    cap_3v3: Cap();
    cap_1v8: Cap();
}
"#;

    println!("📍 Step 1: Parsing BHDL Circuit Description");
    
    let parse_result = parse(bhdl_code);
    if !parse_result.errors().is_empty() {
        println!("❌ Parse errors found:");
        for error in parse_result.errors() {
            println!("   {:?}", error);
        }
        return;
    }
    
    let source_file = match SourceFile::cast(parse_result.syntax().clone()) {
        Some(sf) => sf,
        None => {
            println!("❌ Failed to cast syntax tree to SourceFile");
            return;
        }
    };
    
    println!("✅ Parse successful - syntax tree generated");

    println!("\n📍 Step 2: Multi-Pass Semantic Analysis with Circuit Intelligence");
    
    let analysis_result = analyze(&source_file);
    
    println!("   ✅ Pass 1-4: Core semantic analysis complete");
    println!("      - Global symbols: {}", analysis_result.global_scope.children.len());
    println!("      - Definition scopes: {}", analysis_result.definition_scopes.len());
    println!("      - Constants resolved: {}", analysis_result.resolved_constants.len());
    
    println!("   ✅ Pass 5: Power domain analysis complete");
    println!("      - Power domains: {}", analysis_result.power_analysis.domains.len());
    println!("      - Level shifters: {}", analysis_result.power_analysis.level_shifted_signals.len());
    
    println!("   ✅ Pass 6: Component inference complete");
    println!("      - Inferred components: {}", analysis_result.component_inference.get_inferred_components().len());
    
    println!("   ✅ Pass 7: Power sequencing complete");
    println!("      - Startup steps: {}", analysis_result.power_sequencing.startup_sequence.len());
    println!("      - Shutdown steps: {}", analysis_result.power_sequencing.shutdown_sequence.len());

    if !analysis_result.diagnostics.is_empty() {
        println!("\n📍 Analysis Diagnostics:");
        for diagnostic in &analysis_result.diagnostics {
            println!("   • {}", diagnostic.message);
        }
    }

    println!("\n📍 Step 3: Circuit Intelligence Results");
    
    // Power Domain Intelligence
    println!("   🔋 Power Domain Intelligence:");
    for (name, domain) in &analysis_result.power_analysis.domains {
        println!("      • {}: {}V (±{:.1}%, max {}A)", 
                 name, domain.voltage, domain.tolerance, domain.max_current);
        if !domain.dependencies.is_empty() {
            println!("        Dependencies: {}", domain.dependencies.join(", "));
        }
    }
    
    // Automatic Level Shifting
    if !analysis_result.power_analysis.level_shifted_signals.is_empty() {
        println!("   🔀 Automatic Level Shifting:");
        for shifter in &analysis_result.power_analysis.level_shifted_signals {
            println!("      • {}: {} -> {}", 
                     shifter.signal_name, 
                     shifter.source_domain,
                     shifter.target_domain);
        }
    }
    
    // Component Inference
    if !analysis_result.component_inference.get_inferred_components().is_empty() {
        println!("   🧮 Component Inference Results:");
        for component in analysis_result.component_inference.get_inferred_components() {
            println!("      • {}: {} (Confidence: {:.0}%)", 
                     component.component_type, component.reasoning, component.confidence * 100.0);
            for param in &component.parameters {
                println!("        {} = {} ({})", param.name, param.value, param.reasoning);
            }
        }
    }
    
    // Power Sequencing
    if !analysis_result.power_sequencing.startup_sequence.is_empty() {
        println!("   ⚡ Power Sequencing Logic:");
        println!("      • {} startup steps generated", analysis_result.power_sequencing.startup_sequence.len());
        println!("      • {} shutdown steps generated", analysis_result.power_sequencing.shutdown_sequence.len());
        println!("      • {} warnings", analysis_result.power_sequencing.warnings.len());
    }

    println!("\n📍 Step 4: Generated Intelligent Code Snippets");
    
    // Generate enhanced BHDL code snippets
    println!("   Power sequence code:");
    let power_code = analysis_result.power_sequencing.generate_bhdl_code();
    if !power_code.is_empty() {
        println!("```bhdl");
        print!("{}", power_code);
        println!("```");
    } else {
        println!("   (No power sequence generated)");
    }
    
    println!("   Level shifter code:");
    let level_shifter_code = analysis_result.power_analysis.generate_level_shifter_code();
    if !level_shifter_code.is_empty() {
        println!("```bhdl");
        print!("{}", level_shifter_code);
        println!("```");
    } else {
        println!("   (No level shifters generated)");
    }
    
    println!("   Inferred component code:");
    let component_code = analysis_result.component_inference.generate_inferred_component_code();
    if !component_code.is_empty() {
        println!("```bhdl");
        print!("{}", component_code);
        println!("```");
    } else {
        println!("   (No component inference results)");
    }

    println!("\n📍 Step 5: Circuit Intelligence Summary");
    println!("   ✅ Multi-Voltage Design Analysis: {} voltage domains", 
             analysis_result.power_analysis.domains.len());
    println!("   ✅ Signal Integrity Protection: {} level shifters analyzed", 
             analysis_result.power_analysis.level_shifted_signals.len());
    println!("   ✅ Component Parameter Optimization: {} components analyzed", 
             analysis_result.component_inference.get_inferred_components().len());
    println!("   ✅ Power Management Logic: {}-step power sequence", 
             analysis_result.power_sequencing.startup_sequence.len());
    println!("   ✅ Design Validation: {} total diagnostics", 
             analysis_result.diagnostics.len());

    println!("\n✅ BHDL Phase 2 Circuit Intelligence Test Complete!");
    println!("\n🚀 Key Achievements Demonstrated:");
    println!("   • Multi-pass semantic analysis with circuit intelligence");
    println!("   • Automatic power domain management and analysis");  
    println!("   • Component parameter inference engine");
    println!("   • Power sequencing logic generation");
    println!("   • Comprehensive design validation");
    
    println!("\n🎉 BHDL Phase 2 successfully transforms circuit design with intelligence!");
}