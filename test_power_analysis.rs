//! Test power analysis functionality

use bhdl_analyzer::power_analysis::{PowerAnalysisContext, PowerDomain, LevelShifterType};

fn main() {
    println!("🔋 Testing BHDL Power Analysis (Phase 2)");
    println!("========================================");

    // Test 1: Basic power domain functionality
    test_power_domains();
    
    // Test 2: Level shifter detection
    test_level_shifter_detection();
    
    // Test 3: Power sequence generation
    test_power_sequence_generation();
    
    // Test 4: Signal validation
    test_signal_validation();
    
    println!("\n✅ All power analysis tests passed!");
    println!("🚀 Phase 2 power domain system is working correctly!");
}

fn test_power_domains() {
    println!("\n📍 Test 1: Power Domain Functionality");
    
    let context = PowerAnalysisContext::new();
    
    // Check standard domains are present
    assert!(context.get_domain("USB_5V").is_some(), "USB_5V domain should exist");
    assert!(context.get_domain("VCC_3V3").is_some(), "VCC_3V3 domain should exist");
    assert!(context.get_domain("VCC_1V8").is_some(), "VCC_1V8 domain should exist");
    assert!(context.get_domain("GND").is_some(), "GND domain should exist");
    
    let usb_domain = context.get_domain("USB_5V").unwrap();
    assert_eq!(usb_domain.voltage, 5.0, "USB domain should be 5V");
    assert!(!usb_domain.controllable, "USB domain should not be controllable");
    
    let vcc_3v3 = context.get_domain("VCC_3V3").unwrap();
    assert_eq!(vcc_3v3.voltage, 3.3, "VCC_3V3 should be 3.3V");
    assert!(vcc_3v3.controllable, "VCC_3V3 should be controllable");
    assert!(vcc_3v3.dependencies.contains(&"USB_5V".to_string()), 
            "VCC_3V3 should depend on USB_5V");
    
    println!("   ✅ Standard power domains loaded correctly");
    println!("   ✅ Power domain properties validated");
}

fn test_level_shifter_detection() {
    println!("\n📍 Test 2: Level Shifter Detection");
    
    let context = PowerAnalysisContext::new();
    
    // Test voltage compatibility
    let domain_3v3 = context.get_domain("VCC_3V3").unwrap();
    let domain_5v = context.get_domain("USB_5V").unwrap();
    let domain_1v8 = context.get_domain("VCC_1V8").unwrap();
    
    // 3.3V and 5V should need level shifting
    assert!(domain_3v3.needs_level_shifter(domain_5v), 
            "3.3V -> 5V should need level shifter");
    assert!(domain_5v.needs_level_shifter(domain_3v3), 
            "5V -> 3.3V should need level shifter");
    
    // 3.3V and 1.8V should need level shifting
    assert!(domain_3v3.needs_level_shifter(domain_1v8), 
            "3.3V -> 1.8V should need level shifter");
    
    // Test level shifter type selection
    let shifter_type = domain_3v3.get_level_shifter_type(domain_5v);
    assert!(shifter_type.is_some(), "Should suggest a level shifter type");
    
    if let Some(LevelShifterType::Unidirectional { from, to }) = shifter_type {
        assert_eq!(from, 3.3, "Should shift from 3.3V");
        assert_eq!(to, 5.0, "Should shift to 5V");
    } else {
        panic!("Expected unidirectional level shifter");
    }
    
    println!("   ✅ Voltage incompatibility detected correctly");
    println!("   ✅ Level shifter types selected appropriately");
}

fn test_power_sequence_generation() {
    println!("\n📍 Test 3: Power Sequence Generation");
    
    let mut context = PowerAnalysisContext::new();
    
    // Generate power sequence
    let result = context.generate_power_sequence();
    assert!(result.is_ok(), "Power sequence generation should succeed");
    
    // Check that sequence was generated
    assert!(!context.power_sequence.is_empty(), "Power sequence should not be empty");
    
    // Verify sequence contains expected domains
    let domain_names: Vec<String> = context.power_sequence.iter()
        .map(|step| step.domain_name.clone())
        .collect();
    
    assert!(domain_names.contains(&"VCC_3V3".to_string()), 
            "Sequence should include VCC_3V3");
    assert!(domain_names.contains(&"VCC_1V8".to_string()), 
            "Sequence should include VCC_1V8");
    
    println!("   ✅ Power sequence generated successfully");
    println!("   ✅ Sequence includes controllable domains");
}

fn test_signal_validation() {
    println!("\n📍 Test 4: Signal Validation");
    
    let mut context = PowerAnalysisContext::new();
    
    // Test compatible signal connection
    let location = bhdl_analyzer::types::SourceLocation::new(1, 1);
    let result = context.validate_signal_compatibility(
        "compatible_signal", 
        "VCC_3V3", 
        "VCC_3V3", 
        location.clone()
    );
    assert!(result.is_ok(), "Compatible domains should validate successfully");
    
    // Test incompatible signal connection (should add level shifter)
    let result = context.validate_signal_compatibility(
        "incompatible_signal", 
        "VCC_3V3", 
        "USB_5V", 
        location.clone()
    );
    assert!(result.is_ok(), "Incompatible domains should add level shifter");
    
    // Check that level shifter was added
    assert_eq!(context.level_shifted_signals.len(), 1, 
               "Should have added one level shifter requirement");
    
    let level_shifter = &context.level_shifted_signals[0];
    assert_eq!(level_shifter.signal_name, "incompatible_signal");
    assert_eq!(level_shifter.source_domain, "VCC_3V3");
    assert_eq!(level_shifter.target_domain, "USB_5V");
    
    // Check that warning was generated
    assert!(!context.warnings.is_empty(), "Should have generated warnings");
    assert!(context.warnings[0].contains("Auto-inserting level shifter"), 
            "Warning should mention level shifter insertion");
    
    println!("   ✅ Signal compatibility validation working");
    println!("   ✅ Automatic level shifter insertion working");
    println!("   ✅ Warning generation working");
}