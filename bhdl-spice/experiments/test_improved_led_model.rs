//! Test the improved LED model without fixed voltage assumptions

use bhdl_spice::components_v2::LEDModelV2;

fn main() {
    println!("Testing Improved LED Model (Physics-Based Only)");
    println!("==============================================\n");
    
    let led = LEDModelV2::red();
    
    println!("Model Parameters:");
    println!("-----------------");
    println!("Saturation current: {:e} A", led.saturation_current);
    println!("Emission coefficient: {}", led.emission_coefficient);
    println!("Thermal voltage: {} V", led.thermal_voltage);
    println!("Series resistance: {} Ω", led.series_resistance.unwrap_or(0.0));
    
    println!("\nNOTE: No fixed forward voltage! It varies with current.\n");
    
    println!("Voltage-Current Characteristics:");
    println!("--------------------------------");
    println!("Current     Voltage    Dynamic R   Power");
    println!("--------    --------   ----------  -------");
    
    let test_currents = vec![
        0.0001, 0.0002, 0.0004, 0.001, 0.002, 0.005, 0.010, 0.020, 0.030
    ];
    
    for &current in &test_currents {
        let voltage = led.voltage_at_current(current);
        let r_dyn = led.dynamic_resistance(current);
        let power = voltage * current;
        
        println!("{:7.1}mA   {:6.3}V    {:7.1}Ω   {:6.2}mW",
                 current * 1000.0, voltage, r_dyn, power * 1000.0);
    }
    
    println!("\nKey Observations:");
    println!("-----------------");
    
    let v_at_0_4ma = led.voltage_at_current(0.0004);
    let v_at_1_7ma = led.voltage_at_current(0.0017);
    let v_at_9_7ma = led.voltage_at_current(0.0097);
    let v_at_20ma = led.voltage_at_current(0.020);
    
    println!("At 0.4mA: {:.3}V (low-current solution)", v_at_0_4ma);
    println!("At 1.7mA: {:.3}V (NOT 2V!)", v_at_1_7ma);
    println!("At 9.7mA: {:.3}V (high-current solution)", v_at_9_7ma);
    println!("At 20mA:  {:.3}V (typical test current)", v_at_20ma);
    
    println!("\nThe old model's 'forward_voltage: 2.0' was misleading!");
    println!("It implied constant voltage, but LED voltage varies from");
    println!("{:.2}V to {:.2}V in typical operating range.", v_at_0_4ma, v_at_20ma);
    
    // Demonstrate operating hints
    println!("\nOperating Hints (for solver guidance):");
    println!("--------------------------------------");
    if let Some(hints) = &led.operating_hints {
        for (current, voltage) in hints {
            println!("  {:.0}mA: ~{:.2}V", current * 1000.0, voltage);
        }
    }
    
    println!("\nThese hints help the solver find the desired operating point,");
    println!("but don't constrain the physics-based calculations.");
    
    // Show how to extract Is from datasheet values
    println!("\nExtracting Is from Datasheet:");
    println!("------------------------------");
    let vf_datasheet = 2.0;  // Datasheet says 2V
    let if_datasheet = 0.020; // at 20mA
    let calculated_is = LEDModelV2::from_operating_point(
        vf_datasheet, if_datasheet, 1.5, 0.026
    );
    
    println!("Datasheet: Vf = {}V @ {}mA", vf_datasheet, if_datasheet * 1000.0);
    println!("Calculated Is = {:e} A", calculated_is);
    println!("This Is gives V = {:.3}V at 20mA (close to datasheet)",
             1.5 * 0.026 * ((0.020 / calculated_is) + 1.0).ln());
}