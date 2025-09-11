// Test simulation-integrated passive component selection
// Demonstrates how SPICE analysis results improve component selection accuracy

use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, passive_component_calculator::*, package_selector::*};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    println!("🔬 Testing Simulation-Integrated Passive Component Selection");
    println!("===========================================================");
    
    test_static_vs_simulation_comparison().await?;
    test_safety_analysis_integration().await?;
    test_component_inference_integration().await?;
    
    println!("\n✅ All simulation integration tests completed!");
    
    Ok(())
}

async fn test_static_vs_simulation_comparison() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📊 Static vs Simulation-Based Component Selection");
    
    // Test circuit with LED current limiting resistor
    let content = r#"
board LEDCircuit {
    power VCC = 5V @ 1A;
    ground GND;
    
    // This will be analyzed by SPICE to get actual current/power
    net led_circuit: VCC -> current_limit: Res(220).1 -> current_limit.2 -> led: LED(red).A -> led.K -> GND
        for input_protection(5V, 25mA);
}
"#;

    let parsed = parse(content);
    if !parsed.errors().is_empty() {
        println!("Parse errors:");
        for error in parsed.errors() {
            println!("  - {}", error.message);
        }
        return Ok(());
    }

    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    let analysis = analyze(&source_file);
    
    // Show what data we have available from analysis
    println!("Available simulation data:");
    
    // 1. Component Inference Data
    let inferred_components = analysis.component_inference.get_inferred_components();
    println!("  Component inference results: {} components", inferred_components.len());
    
    for (i, component) in inferred_components.iter().enumerate() {
        if i < 3 { // Limit display
            println!("    {}: {} = {:?}", 
                     component.instance_name, 
                     component.component_type,
                     component.parameters.get("value").unwrap_or(&bhdl_analyzer::component_inference::ParameterValue::String("unknown".to_string()))
            );
        }
    }
    
    // 2. Power Analysis Data
    println!("  Power domains: {} domains", analysis.power_analysis.get_power_domains().len());
    for (domain_name, domain_info) in analysis.power_analysis.get_power_domains() {
        println!("    {}: {}V @ {}A", domain_name, domain_info.voltage, domain_info.max_current);
    }
    
    // 3. Safety Analysis Data
    println!("  Safety analysis: {} violations", analysis.safety_analysis.violations.len());
    if !analysis.safety_analysis.violations.is_empty() {
        for violation in &analysis.safety_analysis.violations {
            println!("    {}: {}", violation.component_name, violation.description);
        }
    }
    
    // Demonstrate static vs simulation-enhanced calculations
    println!("\n🔧 Component Selection Comparison:");
    
    // Static calculation (what we do now)
    let calculator = PassiveComponentCalculator::new();
    let static_power_rating = calculator.calculate_resistor_power_rating(220.0, 0.025); // 25mA design intent
    println!("  Static calculation (220Ω @ 25mA intent):");
    println!("    Power dissipated: {:.1}mW", 220.0 * 0.025 * 0.025 * 1000.0);
    println!("    Selected: {} power rating", static_power_rating);
    
    // Simulation-enhanced calculation (what we should do)
    println!("  Simulation-enhanced calculation:");
    
    // Extract actual values from component inference if available
    let actual_current = extract_actual_current_from_analysis(&analysis, "current_limit").unwrap_or(0.025);
    let actual_power = extract_actual_power_from_analysis(&analysis, "current_limit").unwrap_or(220.0 * 0.025 * 0.025);
    
    println!("    Actual simulated current: {:.1}mA", actual_current * 1000.0);
    println!("    Actual power dissipated: {:.1}mW", actual_power * 1000.0);
    
    // Calculate with actual simulated values
    let sim_power_rating = calculator.calculate_resistor_power_rating(220.0, actual_current);
    println!("    Selected: {} power rating", sim_power_rating);
    
    // Show difference
    if sim_power_rating != static_power_rating {
        println!("    📈 Simulation changed selection: {} → {}", static_power_rating, sim_power_rating);
    } else {
        println!("    ✓ Both methods agree on {}", static_power_rating);
    }
    
    Ok(())
}

async fn test_safety_analysis_integration() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🛡️  Safety Analysis Enhanced Component Selection");
    
    // Test circuit that might have safety violations
    let content = r#"
board HighPowerCircuit {
    power VCC_12V = 12V @ 3A;
    ground GND;
    
    // High current path - might trigger safety warnings
    net motor_drive: VCC_12V -> sense: Res(0.1).1 -> sense.2 -> motor_load: Res(4).1 -> motor_load.2 -> GND
        for input_protection(12V, 3A);
}
"#;

    let parsed = parse(content);
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    let analysis = analyze(&source_file);
    
    println!("Safety analysis results:");
    println!("  Total violations: {}", analysis.safety_analysis.violations.len());
    
    let calculator = PassiveComponentCalculator::new();
    
    // Current sense resistor: 0.1Ω at 3A = 0.9W
    let base_power_rating = calculator.calculate_resistor_power_rating(0.1, 3.0);
    println!("  Base power rating (0.1Ω @ 3A): {}", base_power_rating);
    
    // Enhanced calculation with safety analysis
    let enhanced_power_rating = calculate_safety_enhanced_power_rating(
        &calculator,
        0.1,
        3.0,
        &analysis.safety_analysis
    );
    
    println!("  Safety-enhanced rating: {}", enhanced_power_rating);
    
    if enhanced_power_rating > base_power_rating {
        println!("  📊 Safety analysis recommended higher power rating for reliability");
    }
    
    Ok(())
}

async fn test_component_inference_integration() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🎯 Component Inference Integration");
    
    // Test circuit where SPICE should infer component values
    let content = r#"
board InferenceTestCircuit {
    power VCC_3V3 = 3.3V @ 100mA;
    ground GND;
    
    // Components with placeholder values - SPICE should calculate optimal values
    net signal_chain: VCC_3V3 -> decoupling: Cap().1 -> decoupling.2 -> GND;
    net pullup: VCC_3V3 -> pullup_r: Res().1 -> pullup_r.2 -> signal_out;
}
"#;

    let parsed = parse(content);
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    let analysis = analyze(&source_file);
    
    println!("Component inference results:");
    let inferred = analysis.component_inference.get_inferred_components();
    
    for component in &inferred {
        println!("  {}: {}", component.instance_name, component.component_type);
        
        if let Some(value) = component.parameters.get("value") {
            match value {
                bhdl_analyzer::component_inference::ParameterValue::Number(val, unit) => {
                    println!("    Inferred value: {}{}", val, unit.as_ref().unwrap_or(&"".to_string()));
                    
                    // Use inferred value for component selection
                    match component.component_type.as_str() {
                        "Res" => {
                            let selector = PackageSelector::new();
                            let calculator = PassiveComponentCalculator::new();
                            
                            // Use inferred resistance and estimated current
                            let estimated_current = estimate_current_for_component(&component);
                            let power_rating = calculator.calculate_resistor_power_rating(*val, estimated_current);
                            
                            let spec = selector.select_resistor_spec(
                                *val,
                                power_rating,
                                calculator.calculate_resistor_voltage_rating(3.3),
                                &ApplicationRequirements::default()
                            );
                            
                            println!("    Selected: {}, {}, ±{}%", 
                                     spec.package, spec.power_rating, spec.tolerance);
                        },
                        "Cap" => {
                            let selector = PackageSelector::new();
                            let calculator = PassiveComponentCalculator::new();
                            
                            let voltage_rating = calculator.calculate_capacitor_voltage_rating(3.3);
                            
                            let spec = selector.select_capacitor_spec(
                                *val,
                                voltage_rating,
                                &ApplicationRequirements::default()
                            );
                            
                            println!("    Selected: {}, {}, {}", 
                                     spec.package, spec.voltage_rating, spec.dielectric);
                        },
                        _ => {}
                    }
                },
                _ => println!("    Non-numeric inferred value"),
            }
        }
    }
    
    Ok(())
}

/// Extract actual current from SPICE analysis results (simulated)
fn extract_actual_current_from_analysis(analysis: &bhdl_analyzer::AnalysisResult, component_name: &str) -> Option<f64> {
    // In real implementation, this would extract from SPICE DC operating point
    // For now, simulate realistic values that might differ from design intent
    
    match component_name {
        "current_limit" => Some(0.0227), // Slightly different from 25mA design intent
        _ => None,
    }
}

/// Extract actual power dissipation from SPICE analysis results (simulated)
fn extract_actual_power_from_analysis(analysis: &bhdl_analyzer::AnalysisResult, component_name: &str) -> Option<f64> {
    // In real implementation, this would come from SPICE power calculations
    // For now, simulate realistic values
    
    match component_name {
        "current_limit" => Some(0.000113), // P = I²R = 0.0227² * 220 = 113mW
        _ => None,
    }
}

/// Calculate enhanced power rating based on safety analysis (prototype)
fn calculate_safety_enhanced_power_rating(
    calculator: &PassiveComponentCalculator,
    resistance: f64,
    current: f64,
    safety_analysis: &bhdl_analyzer::passes::SafetyAnalysisResult,
) -> PowerRating {
    let base_rating = calculator.calculate_resistor_power_rating(resistance, current);
    
    // Check for safety violations and enhance rating accordingly
    let mut enhancement_factor = 1.0;
    
    for violation in &safety_analysis.violations {
        match violation.severity {
            bhdl_analyzer::passes::safety_analysis::ViolationSeverity::Critical => {
                enhancement_factor *= 2.0; // Double power rating for critical violations
            },
            bhdl_analyzer::passes::safety_analysis::ViolationSeverity::Error => {
                enhancement_factor *= 1.5; // 50% increase for errors
            },
            bhdl_analyzer::passes::safety_analysis::ViolationSeverity::Warning => {
                enhancement_factor *= 1.2; // 20% increase for warnings
            },
            _ => {}
        }
    }
    
    if enhancement_factor > 1.0 {
        // Calculate enhanced requirement and select next higher rating
        let base_power = base_rating.as_watts();
        let enhanced_power = base_power * enhancement_factor;
        
        // Find the next higher standard rating
        let mut enhanced_rating = base_rating;
        while enhanced_rating.as_watts() < enhanced_power {
            enhanced_rating = enhanced_rating.next_higher();
        }
        
        enhanced_rating
    } else {
        base_rating
    }
}

/// Estimate current for component based on circuit context (prototype)
fn estimate_current_for_component(component: &bhdl_analyzer::component_inference::ComponentSuggestion) -> f64 {
    // In real implementation, this would use circuit topology analysis
    // For now, provide reasonable estimates based on component type and context
    
    match component.component_type.as_str() {
        "Res" => {
            if component.instance_name.contains("pullup") {
                0.001 // 1mA for pullup resistors
            } else if component.instance_name.contains("sense") {
                1.0 // 1A for current sense resistors
            } else {
                0.010 // 10mA default for general resistors
            }
        },
        _ => 0.001, // 1mA default
    }
}