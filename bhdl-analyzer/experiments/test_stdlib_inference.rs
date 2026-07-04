//! Test component inference using stdlib parameters

use bhdl_analyzer::component_inference::{ComponentInferenceContext, CircuitRequirements, CircuitContext};
use bhdl_analyzer::component_library::ModuleResolver;

fn main() {
    println!("Testing component inference with stdlib parameters...\n");
    
    // Create component inference context
    let mut inference = ComponentInferenceContext::new();
    
    // Initialize module resolver with standard library
    let mut resolver = ModuleResolver::new();
    if let Err(e) = resolver.init_stdlib() {
        eprintln!("Failed to initialize stdlib: {}", e);
        return;
    }
    inference.set_module_resolver(resolver);
    
    // Test 1: LED resistor inference for red LED at 5V
    println!("Test 1: Red LED at 5V");
    let requirements = CircuitRequirements {
        supply_voltage: Some(5.0),
        load_current: None,
        required_current: None,
        frequency: None,
        max_power: None,
        temperature_range: None,
        tolerance: None,
        package_constraint: None,
    };
    
    let mut context = CircuitContext::default();
    context.has_led_in_series = true;
    context.led_color = Some("red".to_string());
    
    if let Some(suggestion) = inference.infer_component_parameters("Res", &requirements, &context) {
        println!("  Component: {}", suggestion.component_type);
        println!("  Reasoning: {}", suggestion.reasoning);
        println!("  Confidence: {:.0}%", suggestion.confidence * 100.0);
        println!("  Parameters:");
        for param in &suggestion.parameters {
            println!("    {} = {} ({})", param.name, param.value, param.reasoning);
        }
    } else {
        println!("  No suggestion generated!");
    }
    
    // Test 2: Blue LED at 3.3V
    println!("\nTest 2: Blue LED at 3.3V");
    let requirements = CircuitRequirements {
        supply_voltage: Some(3.3),
        load_current: None,
        required_current: None,
        frequency: None,
        max_power: None,
        temperature_range: None,
        tolerance: None,
        package_constraint: None,
    };
    
    let mut context = CircuitContext::default();
    context.has_led_in_series = true;
    context.led_color = Some("blue".to_string());
    
    if let Some(suggestion) = inference.infer_component_parameters("Res", &requirements, &context) {
        println!("  Component: {}", suggestion.component_type);
        println!("  Reasoning: {}", suggestion.reasoning);
        println!("  Confidence: {:.0}%", suggestion.confidence * 100.0);
        println!("  Parameters:");
        for param in &suggestion.parameters {
            println!("    {} = {} ({})", param.name, param.value, param.reasoning);
        }
        println!("  Warnings:");
        for warning in &inference.warnings {
            println!("    WARNING: {}", warning);
        }
    } else {
        println!("  No suggestion generated!");
    }
    
    // Test 3: Generic LED inference
    println!("\nTest 3: Generic LED inference");
    let requirements = CircuitRequirements {
        supply_voltage: Some(5.0),
        load_current: None,
        required_current: None,
        frequency: None,
        max_power: None,
        temperature_range: None,
        tolerance: None,
        package_constraint: None,
    };
    
    let context = CircuitContext::default();
    
    if let Some(suggestion) = inference.infer_component_parameters("LED", &requirements, &context) {
        println!("  Component: {}", suggestion.component_type);
        println!("  Reasoning: {}", suggestion.reasoning);
        println!("  Confidence: {:.0}%", suggestion.confidence * 100.0);
        println!("  Parameters:");
        for param in &suggestion.parameters {
            println!("    {} = {}", param.name, param.value);
        }
    } else {
        println!("  No suggestion generated!");
    }
}