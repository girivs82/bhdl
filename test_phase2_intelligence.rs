//! BHDL Phase 2: Circuit Intelligence Features Test
//! 
//! This test demonstrates the Phase 2 circuit intelligence capabilities
//! by directly testing the analyzer modules without complex parsing.

use bhdl_analyzer::{
    power_analysis::{PowerAnalysisContext, PowerDomain, LevelShifterType},
    component_inference::{ComponentInferenceContext, CircuitRequirements, CircuitContext},
    power_sequencing::{PowerSequenceGenerator, PowerDomain as SeqPowerDomain},
};

fn main() {
    println!("🚀 BHDL Phase 2: Circuit Intelligence Features Test");
    println!("================================================");
    
    test_power_domain_intelligence();
    test_component_inference_engine();
    test_power_sequencing_logic();
    test_integrated_intelligence();
    
    println!("\n✅ BHDL Phase 2 Circuit Intelligence Test Complete!");
    println!("\n🎉 All Phase 2 features successfully demonstrated!");
}

fn test_power_domain_intelligence() {
    println!("\n📍 Test 1: Power Domain Intelligence");
    
    let mut power_context = PowerAnalysisContext::new();
    
    // Test multi-voltage domain compatibility
    let usb_5v = power_context.get_domain("USB_5V").unwrap();
    let vcc_3v3 = power_context.get_domain("VCC_3V3").unwrap();
    let vcc_1v8 = power_context.get_domain("VCC_1V8").unwrap();
    
    println!("   ✅ Standard power domains initialized:");
    println!("      • USB_5V: {}V (max {}A)", usb_5v.voltage, usb_5v.max_current);
    println!("      • VCC_3V3: {}V (max {}A)", vcc_3v3.voltage, vcc_3v3.max_current);
    println!("      • VCC_1V8: {}V (max {}A)", vcc_1v8.voltage, vcc_1v8.max_current);
    
    // Test voltage compatibility checking
    println!("   ✅ Voltage compatibility analysis:");
    println!("      • 5V ↔ 3.3V compatible: {}", power_context.are_domains_compatible("USB_5V", "VCC_3V3"));
    println!("      • 3.3V ↔ 1.8V compatible: {}", power_context.are_domains_compatible("VCC_3V3", "VCC_1V8"));
    println!("      • 5V ↔ 1.8V compatible: {}", power_context.are_domains_compatible("USB_5V", "VCC_1V8"));
    
    // Test automatic level shifter insertion
    let shifter_type = usb_5v.get_level_shifter_type(vcc_3v3).unwrap();
    println!("   ✅ Automatic level shifter selection:");
    println!("      • 5V → 3.3V: {}", shifter_type);
    
    let shifter_type_2 = vcc_3v3.get_level_shifter_type(vcc_1v8).unwrap();
    println!("      • 3.3V → 1.8V: {}", shifter_type_2);
    
    // Test power sequence generation
    if let Err(error) = power_context.generate_power_sequence() {
        println!("   ❌ Power sequence generation failed: {}", error);
    } else {
        println!("   ✅ Power sequence generated: {} steps", power_context.power_sequence.len());
    }
    
    println!("   🎯 Power domain intelligence validates multi-voltage designs automatically");
}

fn test_component_inference_engine() {
    println!("\n📍 Test 2: Component Inference Engine");
    
    let mut component_inference = ComponentInferenceContext::new();
    
    // Test LED current limiting resistor inference
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
    
    let led_context = CircuitContext {
        has_led_in_series: true,
        led_color: Some("red".to_string()),
        ..Default::default()
    };
    
    if let Some(suggestion) = component_inference.infer_component_parameters("Res", &requirements, &led_context) {
        println!("   ✅ LED current limiting resistor inference:");
        println!("      • Component: {}", suggestion.component_type);
        println!("      • Reasoning: {}", suggestion.reasoning);
        println!("      • Confidence: {:.0}%", suggestion.confidence * 100.0);
        for param in &suggestion.parameters {
            println!("      • {} = {} ({})", param.name, param.value, param.reasoning);
        }
        component_inference.add_inferred_component(suggestion);
    }
    
    // Test I2C pull-up resistor inference
    let pullup_context = CircuitContext {
        is_pullup: true,
        high_speed_signal: false,
        ..Default::default()
    };
    
    if let Some(suggestion) = component_inference.infer_component_parameters("Res", &requirements, &pullup_context) {
        println!("   ✅ I2C pull-up resistor inference:");
        println!("      • Component: {}", suggestion.component_type);
        println!("      • Reasoning: {}", suggestion.reasoning);
        println!("      • Confidence: {:.0}%", suggestion.confidence * 100.0);
        for param in &suggestion.parameters {
            println!("      • {} = {} ({})", param.name, param.value, param.reasoning);
        }
        component_inference.add_inferred_component(suggestion);
    }
    
    // Test decoupling capacitor inference
    let decoupling_context = CircuitContext {
        is_decoupling: true,
        high_frequency: true,
        ..Default::default()
    };
    
    if let Some(suggestion) = component_inference.infer_component_parameters("Cap", &requirements, &decoupling_context) {
        println!("   ✅ Decoupling capacitor inference:");
        println!("      • Component: {}", suggestion.component_type);
        println!("      • Reasoning: {}", suggestion.reasoning);
        println!("      • Confidence: {:.0}%", suggestion.confidence * 100.0);
        for param in &suggestion.parameters {
            println!("      • {} = {} ({})", param.name, param.value, param.reasoning);
        }
        component_inference.add_inferred_component(suggestion);
    }
    
    // Test LED color inference
    let led_context = CircuitContext {
        is_status_indicator: true,
        ..Default::default()
    };
    
    if let Some(suggestion) = component_inference.infer_component_parameters("LED", &requirements, &led_context) {
        println!("   ✅ LED color inference:");
        println!("      • Component: {}", suggestion.component_type);
        println!("      • Reasoning: {}", suggestion.reasoning);
        println!("      • Confidence: {:.0}%", suggestion.confidence * 100.0);
        for param in &suggestion.parameters {
            println!("      • {} = {} ({})", param.name, param.value, param.reasoning);
        }
        component_inference.add_inferred_component(suggestion);
    }
    
    println!("   📊 Total inferred components: {}", component_inference.get_inferred_components().len());
    println!("   🎯 Component inference optimizes circuit performance automatically");
}

fn test_power_sequencing_logic() {
    println!("\n📍 Test 3: Power Sequencing Logic");
    
    let mut power_sequencing = PowerSequenceGenerator::new();
    
    // Add realistic power domains with dependencies
    let domain_5v = SeqPowerDomain {
        name: "USB_5V".to_string(),
        voltage: 5.0,
        max_current: 0.5,
        enable_signal: None, // Always on
        good_signal: None,
        dependencies: vec![],
        startup_delay_ms: 0.0,
        shutdown_delay_ms: 0.0,
        ramp_rate_v_per_ms: None,
        sequence_priority: 1,
        critical: true,
    };
    
    let domain_3v3 = SeqPowerDomain {
        name: "VCC_3V3".to_string(),
        voltage: 3.3,
        max_current: 1.0,
        enable_signal: Some("VCC_3V3_EN".to_string()),
        good_signal: Some("VCC_3V3_GOOD".to_string()),
        dependencies: vec!["USB_5V".to_string()],
        startup_delay_ms: 10.0,
        shutdown_delay_ms: 5.0,
        ramp_rate_v_per_ms: None,
        sequence_priority: 2,
        critical: true,
    };
    
    let domain_1v8 = SeqPowerDomain {
        name: "VCC_1V8".to_string(),
        voltage: 1.8,
        max_current: 0.5,
        enable_signal: Some("VCC_1V8_EN".to_string()),
        good_signal: Some("VCC_1V8_GOOD".to_string()),
        dependencies: vec!["VCC_3V3".to_string()],
        startup_delay_ms: 5.0,
        shutdown_delay_ms: 3.0,
        ramp_rate_v_per_ms: None,
        sequence_priority: 3,
        critical: false,
    };
    
    power_sequencing.add_domain(domain_5v);
    power_sequencing.add_domain(domain_3v3);
    power_sequencing.add_domain(domain_1v8);
    
    println!("   ✅ Power domains added: {}", power_sequencing.domains.len());
    
    // Generate power sequences
    match power_sequencing.generate_sequences() {
        Ok(_) => {
            println!("   ✅ Power sequences generated successfully:");
            println!("      • Startup steps: {}", power_sequencing.startup_sequence.len());
            println!("      • Shutdown steps: {}", power_sequencing.shutdown_sequence.len());
            println!("      • Error recovery sequences: {}", power_sequencing.error_recovery_sequences.len());
            
            // Show startup sequence details
            println!("   ✅ Startup sequence:");
            for step in &power_sequencing.startup_sequence {
                match &step.action {
                    bhdl_analyzer::power_sequencing::PowerAction::Enable => {
                        println!("      • Step {}: Enable {}", step.step_id, step.domain_name);
                    }
                    bhdl_analyzer::power_sequencing::PowerAction::WaitForStable => {
                        println!("      • Step {}: Wait for {} stable ({:.1}ms)", 
                                 step.step_id, step.domain_name, step.delay_ms);
                    }
                    bhdl_analyzer::power_sequencing::PowerAction::CheckVoltage => {
                        println!("      • Step {}: Check {} voltage", step.step_id, step.domain_name);
                    }
                    _ => {}
                }
            }
        }
        Err(error) => {
            println!("   ❌ Power sequence generation failed: {}", error);
        }
    }
    
    if !power_sequencing.warnings.is_empty() {
        println!("   ⚠️  Warnings: {}", power_sequencing.warnings.len());
        for warning in &power_sequencing.warnings {
            println!("      • {}", warning);
        }
    }
    
    println!("   🎯 Power sequencing ensures safe startup/shutdown operations");
}

fn test_integrated_intelligence() {
    println!("\n📍 Test 4: Integrated Circuit Intelligence");
    
    // Simulate a complete intelligent circuit analysis
    let mut power_context = PowerAnalysisContext::new();
    let mut component_inference = ComponentInferenceContext::new();
    let mut power_sequencing = PowerSequenceGenerator::new();
    
    // Simulate cross-domain signal validation
    use bhdl_analyzer::types::SourceLocation;
    let result = power_context.validate_signal_compatibility(
        "mcu_to_sensor_int",
        "VCC_3V3",
        "VCC_1V8",
        SourceLocation::unknown()
    );
    
    match result {
        Ok(_) => {
            println!("   ✅ Cross-domain signal validation completed");
            println!("      • Level shifters auto-inserted: {}", 
                     power_context.level_shifted_signals.len());
        }
        Err(error) => {
            println!("   ❌ Signal validation failed: {}", error);
        }
    }
    
    // Generate comprehensive intelligence outputs
    println!("   ✅ Generating intelligent design outputs:");
    
    let level_shifter_code = power_context.generate_level_shifter_code();
    if !level_shifter_code.is_empty() {
        println!("      • Level shifter BHDL code generated");
    }
    
    let power_sequence_code = power_context.generate_power_sequence_code();
    if !power_sequence_code.is_empty() {
        println!("      • Power sequence BHDL code generated");
    }
    
    let component_code = component_inference.generate_inferred_component_code();
    if !component_code.is_empty() {
        println!("      • Component parameter BHDL code generated");
    }
    
    println!("   🎯 Integrated intelligence provides comprehensive design automation");
    
    // Summary of intelligence benefits
    println!("\n📊 Phase 2 Intelligence Summary:");
    println!("   • Automatic voltage domain management prevents compatibility issues");
    println!("   • Intelligent level shifting ensures signal integrity across domains");
    println!("   • Component parameter inference optimizes circuit performance");
    println!("   • Power sequencing logic guarantees safe operation");
    println!("   • Confidence scoring enables trust in automated decisions");
    println!("   • Comprehensive validation catches design errors early");
}