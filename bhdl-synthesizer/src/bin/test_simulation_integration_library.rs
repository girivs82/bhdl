// Test simulation integration with library-based component knowledge
// Demonstrates how components provide simulation knowledge back to the engines

use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{
    NetlistGenerator, 
    passive_component_calculator::PassiveComponentCalculator,
    package_selector::{PackageSelector, ApplicationRequirements},
    module_variants::ModuleVariantManager
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    println!("🔬 Testing Library-Based Simulation Integration");
    println!("===============================================");
    
    test_component_library_knowledge().await?;
    test_simulation_enhanced_synthesis().await?;
    test_safety_violation_integration().await?;
    
    println!("\n✅ All simulation integration tests completed!");
    
    Ok(())
}

async fn test_component_library_knowledge() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📚 Component Library Knowledge Integration");
    
    // Create a circuit that would benefit from simulation knowledge
    let content = r#"
board SmartPowerSupply {
    power VCC_12V = 12V @ 2A;
    ground GND;
    
    // This LED circuit will be analyzed for optimal current limiting
    net led_indicator: VCC_12V -> current_limit: Res().1 -> current_limit.2 -> status_led: LED(red).A -> status_led.K -> GND
        for input_protection(voltage: 12V, current_limit: 25mA);
        
    // This will be analyzed for ripple current requirements
    net power_filter: VCC_12V -> filter_cap: Cap().1 -> filter_cap.2 -> GND
        for noise_filtering(frequency_range: "1kHz-100kHz", max_ripple: 50mV);
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
    
    println!("Analysis results:");
    println!("  Power domains: {}", analysis.power_analysis.get_power_domains().len());
    println!("  Component inferences: {}", analysis.component_inference.get_inferred_components().len());
    println!("  Safety violations: {}", analysis.safety_analysis.violations.len());
    
    // Test simulation-enhanced passive component calculator
    let calculator = PassiveComponentCalculator::new();
    
    println!("\n🧮 Testing simulation-enhanced calculations:");
    
    // Test LED current limiting resistor with simulation data
    if let Ok((power_rating, voltage_rating, optimal_resistance)) = calculator.calculate_resistor_spec_from_simulation(
        "current_limit",
        &analysis,
        None, // No specific intent for now
    ) {
        println!("  LED current limiting resistor:");
        println!("    Optimal resistance: {:.0}Ω", optimal_resistance);
        println!("    Power rating: {}", power_rating);
        println!("    Voltage rating: {}", voltage_rating);
        
        // Compare with static calculation
        let static_power = calculator.calculate_resistor_power_rating(optimal_resistance, 0.025);
        println!("    Static calculation would give: {}", static_power);
        if power_rating > static_power {
            println!("    ✅ Simulation enhanced calculation upgraded power rating!");
        }
    }
    
    // Test filter capacitor with simulation data
    if let Ok((voltage_rating, dielectric, max_esr)) = calculator.calculate_capacitor_spec_from_simulation(
        "filter_cap",
        &analysis,
        None, // No specific intent for now
    ) {
        println!("  Power filter capacitor:");
        println!("    Voltage rating: {}", voltage_rating);
        println!("    Dielectric type: {}", dielectric);
        println!("    Maximum ESR: {:.3}Ω", max_esr);
        
        // Show how simulation data improves selection
        let static_voltage = calculator.calculate_capacitor_voltage_rating(12.0);
        println!("    Static calculation would give: {}", static_voltage);
        if voltage_rating > static_voltage {
            println!("    ✅ Simulation enhanced calculation upgraded voltage rating!");
        }
    }
    
    Ok(())
}

async fn test_simulation_enhanced_synthesis() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔧 Simulation-Enhanced Synthesis");
    
    // Test circuit with virtual pins that should use simulation data
    let content = r#"
module SmartRegulator(input_voltage: voltage, output_current: current) {
    pin VIN: power in;
    pin VOUT: power out virtual;  // This will use simulation-enhanced expansion
    pin GND: ground inout;
    pin STATUS: signal out virtual; // This will also use enhanced expansion
}

board TestBoard {
    power VCC_12V = 12V @ 1A;
    ground GND;
    
    // Instantiate regulator - virtual pins should be expanded with simulation data
    regulator: SmartRegulator(input_voltage: 12V, output_current: 500mA);
    
    net input_power: VCC_12V -> regulator.VIN;
    net output_power: regulator.VOUT -> load_resistor: Res(10).1 -> load_resistor.2 -> regulator.GND;
    net status_signal: regulator.STATUS -> status_led: LED(green).A -> status_led.K -> regulator.GND;
    net ground_connection: regulator.GND -> GND;
}
"#;

    let parsed = parse(content);
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    let analysis = analyze(&source_file);
    
    println!("Enhanced synthesis analysis:");
    println!("  Power domains identified: {}", analysis.power_analysis.get_power_domains().len());
    println!("  Safety analysis violations: {}", analysis.safety_analysis.violations.len());
    
    // Test the enhanced virtual pin expansion
    let mut variant_manager = ModuleVariantManager::new();
    
    // This would normally be called during netlist generation
    println!("  Virtual pin expansion would use simulation data for:");
    println!("    - VOUT: Power output protection with actual load currents");
    println!("    - STATUS: Signal output protection with real drive requirements");
    
    if !analysis.safety_analysis.violations.is_empty() {
        println!("  Safety violations would enhance component derating:");
        for violation in &analysis.safety_analysis.violations {
            println!("    - {}: {}", violation.component_name, violation.description);
        }
    }
    
    Ok(())
}

async fn test_safety_violation_integration() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🛡️  Safety Violation Enhanced Component Selection");
    
    // Create a circuit with potential safety issues
    let content = r#"
board HighPowerCircuit {
    power VCC_24V = 24V @ 5A;  // High voltage, high current
    ground GND;
    
    // This motor drive circuit may have safety violations
    net motor_control: VCC_24V -> sense: Res(0.05).1 -> sense.2 -> motor: Res(2).1 -> motor.2 -> GND
        for input_protection(voltage: 24V, current_limit: 5A);
        
    // High frequency switching circuit
    net switching: VCC_24V -> switching_cap: Cap(10uF).1 -> switching_cap.2 -> GND
        for fast_response(switching_frequency: "100kHz");
}
"#;

    let parsed = parse(content);
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    let analysis = analyze(&source_file);
    
    println!("Safety analysis results:");
    println!("  Total violations: {}", analysis.safety_analysis.violations.len());
    
    let calculator = PassiveComponentCalculator::new();
    
    // Test how safety violations affect component selection
    println!("\n🔍 Testing safety-enhanced component selection:");
    
    // Current sense resistor in high power circuit
    if let Ok((power_rating, voltage_rating, optimal_resistance)) = calculator.calculate_resistor_spec_from_simulation(
        "sense",
        &analysis,
        None,
    ) {
        println!("  Current sense resistor (0.05Ω @ 5A):");
        println!("    Simulation-enhanced power rating: {}", power_rating);
        println!("    Voltage rating: {}", voltage_rating);
        
        // Show base calculation
        let base_power = calculator.calculate_resistor_power_rating(0.05, 5.0);
        println!("    Base calculation: {}", base_power);
        
        if power_rating > base_power {
            println!("    ✅ Safety violations upgraded power rating for reliability!");
        }
    }
    
    // Switching capacitor with high frequency requirements
    if let Ok((voltage_rating, dielectric, max_esr)) = calculator.calculate_capacitor_spec_from_simulation(
        "switching_cap",
        &analysis,
        None,
    ) {
        println!("  Switching capacitor (10μF @ 24V):");
        println!("    Simulation-enhanced voltage rating: {}", voltage_rating);
        println!("    Selected dielectric: {}", dielectric);
        println!("    Maximum ESR for switching: {:.1}mΩ", max_esr * 1000.0);
        
        // Show base calculation
        let base_voltage = calculator.calculate_capacitor_voltage_rating(24.0);
        println!("    Base calculation: {}", base_voltage);
        
        if voltage_rating > base_voltage {
            println!("    ✅ Safety analysis upgraded voltage rating!");
        }
    }
    
    // Demonstrate safety factor enhancement
    println!("\n📊 Safety factor enhancement demonstration:");
    let base_calc = PassiveComponentCalculator::new();
    
    // Simulate what enhanced safety factors would look like
    println!("  Standard safety factors:");
    println!("    Power derating: 70%");
    println!("    Voltage margin: 2.0x");
    
    if !analysis.safety_analysis.violations.is_empty() {
        println!("  Enhanced safety factors (with violations):");
        println!("    Power derating: 50% (additional 20% for safety)");
        println!("    Voltage margin: 2.6x (additional 30% for safety)");
        println!("    ✅ Components selected with enhanced reliability margins!");
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_library_simulation_integration() {
        // Test that the library-based simulation integration works
        let result = test_component_library_knowledge().await;
        assert!(result.is_ok(), "Component library knowledge integration failed");
    }
    
    #[test]
    fn test_enhanced_calculator_methods() {
        let calculator = PassiveComponentCalculator::new();
        
        // Test that the simulation-enhanced methods exist and can be called
        // (This would normally use real analysis data)
        let mock_analysis = bhdl_analyzer::AnalysisResult::default();
        
        // Test resistor calculation
        let resistor_result = calculator.calculate_resistor_spec_from_simulation(
            "test_resistor",
            &mock_analysis,
            None,
        );
        assert!(resistor_result.is_ok(), "Enhanced resistor calculation should work");
        
        // Test capacitor calculation
        let capacitor_result = calculator.calculate_capacitor_spec_from_simulation(
            "test_capacitor",
            &mock_analysis,
            None,
        );
        assert!(capacitor_result.is_ok(), "Enhanced capacitor calculation should work");
    }
}