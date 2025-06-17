// Test component inference with proper voltage from power domains
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Testing Component Inference Voltage Fix ===\n");
    
    let bhdl_code = r#"
board VoltageRegulator_7805 {
    // Power and ground
    power VIN = 12V @ 1A;
    power VCC = 5V @ 500mA;
    ground GND;
    
    // Input protection
    VIN -> Fuse(500mA).1 -> protected_vin;
    protected_vin -> TVSDiode(15V).1;
    TVSDiode.2 -> GND;
    
    // Input filtering  
    protected_vin -> c_in1 -> GND;
    c_in1: ElectrolyticCap(100uF, 25V);
    protected_vin -> c_in2 -> GND;
    c_in2: Cap(100nF);
    
    // Voltage regulator
    protected_vin -> reg.IN;
    reg: LM7805();
    reg.GND -> GND;
    reg.OUT -> VCC;
    
    // Output filtering
    VCC -> c_out1 -> GND;
    c_out1: ElectrolyticCap(10uF, 10V);
    VCC -> c_out2 -> GND;
    c_out2: Cap(100nF);
    
    // Status LED
    VCC -> r_led -> led -> GND;
    r_led: Res(330R);
    led: LED(green);
    
    // Test points
    protected_vin -> tp_vin;
    tp_vin: TestPoint();
    VCC -> tp_vout;
    tp_vout: TestPoint();
    GND -> tp_gnd;
    tp_gnd: TestPoint();
}
"#;

    // Parse
    let parse_result = parse(bhdl_code);
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for err in parse_result.errors() {
            println!("  {}", err.message);
        }
        return Err("Parse failed".into());
    }
    
    // Convert to AST
    let syntax = parse_result.syntax();
    let source_file = SourceFile::cast(syntax).ok_or("Failed to cast to SourceFile")?;
    
    // Analyze
    println!("Running analysis with component inference...\n");
    let analysis_result = analyze(&source_file);
    
    // Print power domains
    if let Some(power_analysis) = &analysis_result.power_analysis {
        println!("Power Domains:");
        for (name, domain) in &power_analysis.domains {
            println!("  {}: {}V @ {}A", name, domain.voltage, domain.max_current);
        }
        
        println!("\nComponent Power Assignments:");
        for (comp, domain) in &power_analysis.component_domains {
            println!("  {} -> {}", comp, domain);
        }
    }
    
    // Print inferred components
    if let Some(inferred) = &analysis_result.inferred_components {
        println!("\nInferred Components:");
        for comp in inferred {
            println!("  {}: {}", comp.component_type, comp.suggested_part);
            if !comp.inferred_parameters.is_empty() {
                println!("    Parameters:");
                for param in &comp.inferred_parameters {
                    println!("      {} = {} (confidence: {:.0}%)", 
                        param.name, param.value, param.confidence * 100.0);
                    if !param.reasoning.is_empty() {
                        println!("        Reasoning: {}", param.reasoning);
                    }
                }
            }
        }
    }
    
    println!("\nTest completed!");
    Ok(())
}