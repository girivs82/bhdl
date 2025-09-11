// Test the passive component calculation and package selection engines
// Validates that component specifications match real-world requirements

use bhdl_synthesizer::passive_component_calculator::*;
use bhdl_synthesizer::package_selector::*;

fn main() {
    println!("🧮 Testing Passive Component Calculation Engine");
    println!("================================================");
    
    test_basic_calculations();
    test_safety_factors();
    test_package_selection();
    test_real_world_scenarios();
    test_automotive_requirements();
    
    println!("\n✅ All tests completed successfully!");
}

fn test_basic_calculations() {
    println!("\n🔬 Testing Basic Electrical Calculations");
    let calculator = PassiveComponentCalculator::new();
    
    // Test 1: LED current limiting resistor
    println!("  Test 1: LED Current Limiting Resistor");
    let led_voltage = 5.0;     // 5V supply
    let led_current = 0.020;   // 20mA LED
    let led_forward_v = 2.0;   // Red LED forward voltage
    let resistor_voltage = led_voltage - led_forward_v; // 3V across resistor
    let resistance = resistor_voltage / led_current;    // 150Ω
    
    let power_rating = calculator.calculate_resistor_power_rating(resistance, led_current);
    let voltage_rating = calculator.calculate_resistor_voltage_rating(resistor_voltage);
    
    println!("    Supply: {}V, LED: {}V, Resistor: {:.1}Ω", led_voltage, led_forward_v, resistance);
    println!("    Power dissipated: {:.1}mW ({}mW * {}A²)", 
             resistance * led_current * led_current * 1000.0, resistance as i32, led_current);
    println!("    Selected: {} power rating, {} voltage rating", power_rating, voltage_rating);
    
    assert_eq!(power_rating, PowerRating::P100mW); // 60mW/0.7 ≈ 86mW -> 100mW
    assert_eq!(voltage_rating, VoltageRating::V6_3); // 3V * 1.5 = 4.5V -> 6.3V
    
    // Test 2: Decoupling capacitor
    println!("  Test 2: Decoupling Capacitor");
    let operating_voltage = 3.3;
    let cap_voltage_rating = calculator.calculate_capacitor_voltage_rating(operating_voltage);
    
    println!("    Operating: {}V, Safety margin: 2x", operating_voltage);
    println!("    Selected: {} voltage rating", cap_voltage_rating);
    
    assert_eq!(cap_voltage_rating, VoltageRating::V10); // 3.3V * 2 = 6.6V -> 10V
    
    // Test 3: High power resistor
    println!("  Test 3: High Power Motor Current Sense");
    let motor_current = 2.0;  // 2A motor
    let sense_resistance = 0.1; // 100mΩ current sense
    
    let high_power_rating = calculator.calculate_resistor_power_rating(sense_resistance, motor_current);
    
    println!("    Current: {}A, Resistance: {}Ω", motor_current, sense_resistance);
    println!("    Power dissipated: {:.1}W", sense_resistance * motor_current * motor_current);
    println!("    Selected: {} power rating", high_power_rating);
    
    assert_eq!(high_power_rating, PowerRating::P1W); // 400mW/0.7 ≈ 571mW -> 1W
}

fn test_safety_factors() {
    println!("\n🛡️  Testing Safety Factor Applications");
    
    // Standard consumer electronics
    let standard_calc = PassiveComponentCalculator::new();
    
    // Automotive application (more conservative)
    let automotive_calc = PassiveComponentCalculator::with_safety_factors(SafetyFactors::automotive());
    
    // Industrial application
    let industrial_calc = PassiveComponentCalculator::with_safety_factors(SafetyFactors::industrial());
    
    let resistance = 1000.0; // 1kΩ
    let current = 0.010;     // 10mA
    let voltage = 3.3;       // 3.3V
    
    println!("  Comparing safety factors for 1kΩ resistor @ 10mA:");
    
    let std_power = standard_calc.calculate_resistor_power_rating(resistance, current);
    let auto_power = automotive_calc.calculate_resistor_power_rating(resistance, current);
    let ind_power = industrial_calc.calculate_resistor_power_rating(resistance, current);
    
    println!("    Standard (70% derating):   {}", std_power);
    println!("    Automotive (60% derating): {}", auto_power);
    println!("    Industrial (65% derating): {}", ind_power);
    
    let std_voltage = standard_calc.calculate_capacitor_voltage_rating(voltage);
    let auto_voltage = automotive_calc.calculate_capacitor_voltage_rating(voltage);
    let ind_voltage = industrial_calc.calculate_capacitor_voltage_rating(voltage);
    
    println!("  Comparing voltage ratings for 3.3V capacitor:");
    println!("    Standard (2.0x margin):   {}", std_voltage);
    println!("    Automotive (2.5x margin): {}", auto_voltage);
    println!("    Industrial (2.2x margin): {}", ind_voltage);
    
    // Automotive should be same or more conservative
    assert!(auto_power >= std_power);
    assert!(auto_voltage >= std_voltage);
}

fn test_package_selection() {
    println!("\n📦 Testing Package Selection Logic");
    let selector = PackageSelector::new();
    
    // Test 1: Small signal resistor
    println!("  Test 1: Small Signal Resistor (1kΩ, 125mW)");
    let requirements = ApplicationRequirements::default();
    let resistor_spec = selector.select_resistor_spec(
        1000.0,
        PowerRating::P125mW,
        VoltageRating::V50,
        &requirements
    );
    
    println!("    Package: {}, Tolerance: ±{}%", resistor_spec.package, resistor_spec.tolerance);
    println!("    Temp coeff: ±{} ppm/°C", resistor_spec.temp_coefficient);
    
    // 125mW maps to 0805, but standard constraint may upgrade to 1206 
    assert!(matches!(resistor_spec.package, PackageSize::_0805 | PackageSize::_1206)); // Acceptable for 125mW
    assert_eq!(resistor_spec.tolerance, 5.0); // Standard tolerance
    
    // Test 2: Precision resistor
    println!("  Test 2: Precision Resistor (High Accuracy)");
    let mut precision_requirements = ApplicationRequirements::default();
    precision_requirements.precision_requirement = PrecisionRequirement::High;
    
    let precision_spec = selector.select_resistor_spec(
        10000.0,
        PowerRating::P125mW,
        VoltageRating::V50,
        &precision_requirements
    );
    
    println!("    Package: {}, Tolerance: ±{}%", precision_spec.package, precision_spec.tolerance);
    println!("    Temp coeff: ±{} ppm/°C", precision_spec.temp_coefficient);
    
    assert_eq!(precision_spec.tolerance, 1.0); // High precision tolerance
    assert_eq!(precision_spec.temp_coefficient, 50.0); // Low temp coefficient
    
    // Test 3: Bypass capacitor
    println!("  Test 3: Bypass Capacitor (100nF)");
    let capacitor_spec = selector.select_capacitor_spec(
        100e-9, // 100nF
        VoltageRating::V16,
        &requirements
    );
    
    println!("    Package: {}, Dielectric: {}", capacitor_spec.package, capacitor_spec.dielectric);
    println!("    Tolerance: ±{}%", capacitor_spec.tolerance);
    
    assert_eq!(capacitor_spec.dielectric, DielectricType::X7R); // Good for bypass
    assert!(matches!(capacitor_spec.package, PackageSize::_0603 | PackageSize::_0805 | PackageSize::_1206)); // Reasonable size range
    
    // Test 4: High frequency capacitor
    println!("  Test 4: High Frequency Capacitor (10pF @ 100MHz)");
    let mut hf_requirements = ApplicationRequirements::default();
    hf_requirements.frequency = Some(100e6); // 100MHz
    hf_requirements.precision_requirement = PrecisionRequirement::High;
    
    let hf_cap_spec = selector.select_capacitor_spec(
        10e-12, // 10pF
        VoltageRating::V50,
        &hf_requirements
    );
    
    println!("    Package: {}, Dielectric: {}", hf_cap_spec.package, hf_cap_spec.dielectric);
    println!("    Tolerance: ±{}%", hf_cap_spec.tolerance);
    
    assert_eq!(hf_cap_spec.dielectric, DielectricType::C0G); // Best for HF precision
    assert!(hf_cap_spec.tolerance <= 5.0); // C0G can achieve good tolerance (2% is better than 5%)
}

fn test_real_world_scenarios() {
    println!("\n🌍 Testing Real-World Circuit Scenarios");
    let calculator = PassiveComponentCalculator::new();
    let selector = PackageSelector::new();
    
    // Scenario 1: 3.3V Microcontroller Power Supply Filter
    println!("  Scenario 1: 3.3V MCU Power Supply Filter");
    let mcu_voltage = 3.3;
    let mcu_current = 0.100; // 100mA
    
    // Input filter capacitor
    let input_cap_rating = calculator.calculate_capacitor_voltage_rating(mcu_voltage);
    let input_cap_spec = selector.select_capacitor_spec(
        10e-6, // 10μF input cap
        input_cap_rating,
        &ApplicationRequirements::default()
    );
    
    // Bypass capacitor
    let bypass_cap_spec = selector.select_capacitor_spec(
        100e-9, // 100nF bypass cap
        input_cap_rating,
        &ApplicationRequirements::default()
    );
    
    println!("    Input Cap: {}μF, {}, {}, {}", 
             input_cap_spec.capacitance * 1e6,
             input_cap_spec.package,
             input_cap_spec.dielectric,
             input_cap_spec.voltage_rating);
    println!("    Bypass Cap: {}nF, {}, {}, {}",
             bypass_cap_spec.capacitance * 1e9,
             bypass_cap_spec.package,
             bypass_cap_spec.dielectric,
             bypass_cap_spec.voltage_rating);
    
    // Scenario 2: 12V Motor Drive Protection
    println!("  Scenario 2: 12V Motor Drive Circuit");
    let motor_voltage = 12.0;
    let motor_current = 2.0; // 2A
    
    // Current sense resistor (0.1Ω for 200mV drop at 2A)
    let sense_resistance = 0.1;
    let sense_power = calculator.calculate_resistor_power_rating(sense_resistance, motor_current);
    let sense_voltage = calculator.calculate_resistor_voltage_rating(motor_current * sense_resistance);
    
    let mut high_power_req = ApplicationRequirements::default();
    high_power_req.size_constraint = SizeConstraint::Relaxed; // Can use larger packages
    
    let sense_spec = selector.select_resistor_spec(
        sense_resistance,
        sense_power,
        sense_voltage,
        &high_power_req
    );
    
    // Input filter capacitor for motor
    let motor_cap_rating = calculator.calculate_capacitor_voltage_rating(motor_voltage);
    let motor_cap_spec = selector.select_capacitor_spec(
        470e-6, // 470μF motor filter cap
        motor_cap_rating,
        &high_power_req
    );
    
    println!("    Current Sense: {:.1}Ω, {}, {}, {}", 
             sense_spec.resistance,
             sense_spec.power_rating,
             sense_spec.package,
             sense_spec.voltage_rating);
    println!("    Motor Filter: {}μF, {}, {}, {}",
             motor_cap_spec.capacitance * 1e6,
             motor_cap_spec.package,
             motor_cap_spec.dielectric,
             motor_cap_spec.voltage_rating);
             
    // Verify high power components are properly sized
    assert!(sense_power >= PowerRating::P500mW); // High current needs high power rating
    assert!(motor_cap_rating >= VoltageRating::V25); // 12V * 2 = 24V -> 25V rating
}

fn test_automotive_requirements() {
    println!("\n🚗 Testing Automotive Application Requirements");
    let automotive_calc = PassiveComponentCalculator::with_safety_factors(SafetyFactors::automotive());
    let selector = PackageSelector::new();
    
    // Automotive applications need wider temperature range and higher reliability
    let mut auto_requirements = ApplicationRequirements::default();
    auto_requirements.temperature_range = Some((-40.0, 125.0)); // Automotive temp range
    auto_requirements.cost_sensitivity = CostSensitivity::Premium; // Reliability over cost
    auto_requirements.precision_requirement = PrecisionRequirement::High;
    
    // ECU power supply (12V automotive)
    println!("  ECU Power Supply (12V Automotive)");
    let ecu_voltage = 12.0;
    let ecu_current = 0.500; // 500mA
    
    // More conservative automotive ratings
    let auto_cap_rating = automotive_calc.calculate_capacitor_voltage_rating(ecu_voltage);
    let auto_cap_spec = selector.select_capacitor_spec(
        22e-6, // 22μF
        auto_cap_rating,
        &auto_requirements
    );
    
    // CAN bus termination resistor (120Ω standard)
    let can_resistance = 120.0;
    let can_current = 0.040; // 40mA typical
    let can_power = automotive_calc.calculate_resistor_power_rating(can_resistance, can_current);
    let can_spec = selector.select_resistor_spec(
        can_resistance,
        can_power,
        VoltageRating::V16, // 12V system with margin
        &auto_requirements
    );
    
    println!("    Power Cap: {}μF, {}, {}, {} (vs standard {})",
             auto_cap_spec.capacitance * 1e6,
             auto_cap_spec.package,
             auto_cap_spec.dielectric,
             auto_cap_spec.voltage_rating,
             VoltageRating::V25); // Compare with standard calculation
             
    println!("    CAN Term: {}Ω, {}, {}, ±{}%",
             can_spec.resistance,
             can_spec.power_rating,
             can_spec.package,
             can_spec.tolerance);
    
    // Automotive should use higher voltage ratings for safety
    assert!(auto_cap_rating >= VoltageRating::V35); // 12V * 2.5 = 30V -> 35V
    assert!(can_spec.tolerance <= 5.0); // High precision for CAN
    
    println!("  ✅ Automotive components properly derated for harsh environment");
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn validate_calculation_engine() {
        // This ensures the calculation engine produces sensible results
        let calculator = PassiveComponentCalculator::new();
        
        // Test power calculation progression
        let power_62mw = calculator.calculate_resistor_power_rating(10000.0, 0.0025); // 62.5mW
        let power_250mw = calculator.calculate_resistor_power_rating(1000.0, 0.016);  // 256mW
        let power_1w = calculator.calculate_resistor_power_rating(100.0, 0.1);       // 1W
        
        assert_eq!(power_62mw, PowerRating::P62mW);
        assert_eq!(power_250mw, PowerRating::P500mW); // Derated 256/0.7 = 366mW -> 500mW
        assert_eq!(power_1w, PowerRating::P2W);       // Derated 1W/0.7 = 1.43W -> 2W
    }
}