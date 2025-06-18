use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;

fn main() {
    let test_cases = vec![
        ("Empty params", r#"
board Test {
    power VCC = 5V @ 500mA;
    ground GND;
    VCC -> r1: Res().1 -> led1: LED(red, 20mA).A;
    led1.K -> GND;
}"#),
        ("Explicit placeholder", r#"
board Test {
    power VCC = 5V @ 500mA;
    ground GND;
    VCC -> r2: Res(?).1 -> led2: LED(green, 20mA).A;
    led2.K -> GND;
}"#),
        ("Placeholder with constraints", r#"
board Test {
    power VCC = 5V @ 500mA;
    ground GND;
    VCC -> r3: Res(?, rating=0.25W, tolerance=5%).1 -> led3: LED(blue, 20mA).A;
    led3.K -> GND;
}"#),
        ("Normal value - no placeholder", r#"
board Test {
    power VCC = 5V @ 500mA;
    ground GND;
    VCC -> r4: Res(220).1 -> led4: LED(yellow, 20mA).A;
    led4.K -> GND;
}"#),
    ];
    
    for (name, source) in test_cases {
        println!("\n=== {} ===", name);
        
        // Parse
        let parse_result = parse(source);
        if !parse_result.errors().is_empty() {
            println!("Parse errors:");
            for error in parse_result.errors() {
                println!("  {}", error.message);
            }
            continue;
        }
        
        let root = parse_result.syntax();
        let source_file = SourceFile::cast(root).expect("Expected SourceFile");
        
        // Analyze
        let analysis_result = analyze(&source_file);
        
        // Check for unresolved components
        let unresolved = analysis_result.component_inference.get_unresolved_components();
        println!("Unresolved components (needing SPICE): {}", unresolved.len());
        
        for comp in unresolved {
            println!("  - {} ({})", comp.instance_name, comp.component_type);
            if !comp.constraints.power_rating.is_none() {
                println!("    Power rating constraint: {:?}W", comp.constraints.power_rating);
            }
            if !comp.constraints.tolerance.is_none() {
                println!("    Tolerance constraint: {:?}%", comp.constraints.tolerance.map(|t| t * 100.0));
            }
            
            // Print circuit context
            match &comp.circuit_context {
                bhdl_analyzer::spice_synthesis::CircuitContext::LEDCurrentLimit { led_spec, supply_voltage, .. } => {
                    println!("    Context: LED current limiting");
                    println!("      LED color: {}", led_spec.color);
                    println!("      Supply voltage: {}V", supply_voltage);
                    println!("      Target current: {}mA", led_spec.target_current * 1000.0);
                }
                _ => {
                    println!("    Context: Unknown");
                }
            }
        }
        
        // Check for normally inferred components
        let inferred = analysis_result.component_inference.get_inferred_components();
        println!("Normally inferred components: {}", inferred.len());
        for comp in inferred {
            if let Some(name) = &comp.instance_name {
                println!("  - {} ({})", name, comp.component_type);
                for param in &comp.parameters {
                    println!("    {}: {}", param.name, param.value);
                }
            }
        }
    }
}