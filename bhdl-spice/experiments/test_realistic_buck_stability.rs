//! Test realistic buck converter stability analysis
//! 
//! Creates detailed buck converter circuits and analyzes their stability

use bhdl_spice::{
    Circuit, NodeId,
    stability::{
        PowerConverterStabilityAnalyzer, ConverterNodes,
        StabilityWarning, StabilityRecommendation,
    },
};

fn main() {
    println!("Realistic Buck Converter Stability Analysis");
    println!("==========================================\n");
    
    // Test different buck configurations
    test_basic_buck();
    test_well_compensated_buck();
    test_poorly_compensated_buck();
    test_high_q_resonance_buck();
}

fn test_basic_buck() {
    println!("1. Basic Buck Converter (12V to 5V @ 3A)");
    println!("----------------------------------------\n");
    
    let circuit = create_realistic_buck(
        12.0,   // Vin
        5.0,    // Vout
        3.0,    // Iout
        300e3,  // fsw = 300kHz
        15e-6,  // L = 15µH
        220e-6, // Cout = 220µF
        0.05,   // ESR = 50mΩ
        CompensationType::TypeII,
    );
    
    analyze_and_report(circuit, "BasicBuck");
}

fn test_well_compensated_buck() {
    println!("\n2. Well-Compensated Buck (24V to 12V @ 5A)");
    println!("-------------------------------------------\n");
    
    let circuit = create_realistic_buck(
        24.0,   // Vin
        12.0,   // Vout
        5.0,    // Iout
        200e3,  // fsw = 200kHz
        33e-6,  // L = 33µH
        470e-6, // Cout = 470µF
        0.03,   // ESR = 30mΩ
        CompensationType::TypeIII,
    );
    
    analyze_and_report(circuit, "WellCompensatedBuck");
}

fn test_poorly_compensated_buck() {
    println!("\n3. Poorly Compensated Buck (12V to 3.3V @ 2A)");
    println!("----------------------------------------------\n");
    
    let circuit = create_realistic_buck(
        12.0,   // Vin
        3.3,    // Vout
        2.0,    // Iout
        500e3,  // fsw = 500kHz
        10e-6,  // L = 10µH (too small)
        47e-6,  // Cout = 47µF (too small)
        0.1,    // ESR = 100mΩ (too high)
        CompensationType::None,
    );
    
    analyze_and_report(circuit, "PoorlyCompensatedBuck");
}

fn test_high_q_resonance_buck() {
    println!("\n4. Buck with High-Q Input Filter Resonance");
    println!("-------------------------------------------\n");
    
    let mut circuit = create_realistic_buck(
        12.0,   // Vin
        5.0,    // Vout
        3.0,    // Iout
        400e3,  // fsw = 400kHz
        22e-6,  // L = 22µH
        150e-6, // Cout = 150µF
        0.04,   // ESR = 40mΩ
        CompensationType::TypeII,
    );
    
    // Add undamped input filter that creates high-Q resonance
    add_input_filter(&mut circuit, 10e-6, 100e-6, 0.001); // Very low damping
    
    analyze_and_report(circuit, "HighQResonanceBuck");
}

#[derive(Debug, Clone, Copy)]
enum CompensationType {
    None,
    TypeII,
    TypeIII,
}

fn create_realistic_buck(
    vin: f64,
    vout: f64,
    iout: f64,
    fsw: f64,
    l: f64,
    cout: f64,
    esr: f64,
    comp_type: CompensationType,
) -> Circuit {
    let mut circuit = Circuit::new();
    
    // Main nodes - store names as strings for easier use
    let vin_name = "VIN";
    let sw_name = "SW";
    let vout_name = "VOUT";
    let fb_name = "FB";
    let comp_name = "COMP";
    let gnd_name = "GND";
    let vref_name = "VREF";
    
    // Create nodes
    circuit.add_node(vin_name.to_string(), None);
    circuit.add_node(sw_name.to_string(), None);
    circuit.add_node(vout_name.to_string(), None);
    circuit.add_node(fb_name.to_string(), None);
    circuit.add_node(comp_name.to_string(), None);
    circuit.add_node(gnd_name.to_string(), None);
    circuit.add_node(vref_name.to_string(), None);
    
    // Input capacitor (with ESR)
    let cin_esr_name = "CIN_ESR";
    circuit.add_node(cin_esr_name.to_string(), None);
    circuit.add_branch("CIN".to_string(), vin_name, cin_esr_name, 
                      "Capacitor".to_string(), 100e-6, None);
    circuit.add_branch("CIN_ESR".to_string(), cin_esr_name, gnd_name, 
                      "Resistor".to_string(), 0.02, None);
    
    // Power stage
    // Inductor with DCR
    circuit.add_branch("L1".to_string(), sw_name, vout_name, 
                      "Inductor".to_string(), l, None);
    let l_dcr = 0.001 + (10.0 / l); // DCR increases for smaller inductors
    circuit.add_branch("L1_DCR".to_string(), sw_name, vout_name, 
                      "Resistor".to_string(), l_dcr, None);
    
    // Output capacitor with ESR
    let cout_esr_name = "COUT_ESR";
    circuit.add_node(cout_esr_name.to_string(), None);
    circuit.add_branch("COUT".to_string(), vout_name, cout_esr_name, 
                      "Capacitor".to_string(), cout, None);
    circuit.add_branch("COUT_ESR".to_string(), cout_esr_name, gnd_name, 
                      "Resistor".to_string(), esr, None);
    
    // Add ceramic cap in parallel for better HF response
    circuit.add_branch("COUT_CER".to_string(), vout_name, gnd_name, 
                      "Capacitor".to_string(), 22e-6, None);
    
    // Feedback divider
    let vref = 0.8; // Typical reference voltage
    let rfb_ratio = vout / vref - 1.0;
    let rfb_bot = 10e3;
    let rfb_top = rfb_bot * rfb_ratio;
    
    circuit.add_branch("RFB_TOP".to_string(), vout_name, fb_name, 
                      "Resistor".to_string(), rfb_top, None);
    circuit.add_branch("RFB_BOT".to_string(), fb_name, gnd_name, 
                      "Resistor".to_string(), rfb_bot, None);
    
    // Compensation network
    match comp_type {
        CompensationType::None => {
            // Direct connection (unity gain)
            circuit.add_branch("RCOMP_DIRECT".to_string(), fb_name, comp_name, 
                              "Resistor".to_string(), 0.1, None);
        }
        CompensationType::TypeII => {
            // Type II compensation
            let fc = fsw / 20.0; // Crossover at fsw/20
            let rcomp = 22e3;
            let ccomp = 1.0 / (2.0 * std::f64::consts::PI * fc * rcomp);
            
            circuit.add_branch("RCOMP".to_string(), fb_name, comp_name, 
                              "Resistor".to_string(), rcomp, None);
            circuit.add_branch("CCOMP1".to_string(), comp_name, gnd_name, 
                              "Capacitor".to_string(), ccomp, None);
            
            // Add high frequency pole
            circuit.add_branch("CCOMP2".to_string(), comp_name, fb_name, 
                              "Capacitor".to_string(), 47e-12, None);
        }
        CompensationType::TypeIII => {
            // Type III compensation
            let fc = fsw / 15.0; // Higher bandwidth
            
            // Primary path
            circuit.add_branch("RCOMP1".to_string(), fb_name, comp_name, 
                              "Resistor".to_string(), 15e3, None);
            circuit.add_branch("CCOMP1".to_string(), comp_name, gnd_name, 
                              "Capacitor".to_string(), 10e-9, None);
            
            // Zero-pole pair
            let comp_zero_name = "COMP_ZERO";
            circuit.add_node(comp_zero_name.to_string(), None);
            circuit.add_branch("RCOMP2".to_string(), comp_name, comp_zero_name, 
                              "Resistor".to_string(), 4.7e3, None);
            circuit.add_branch("CCOMP2".to_string(), comp_zero_name, fb_name, 
                              "Capacitor".to_string(), 1e-9, None);
            
            // High frequency pole
            circuit.add_branch("CCOMP3".to_string(), comp_name, fb_name, 
                              "Capacitor".to_string(), 22e-12, None);
        }
    }
    
    // Error amplifier (behavioral model)
    circuit.add_branch("EA".to_string(), vref_name, fb_name, 
                      "OpAmp".to_string(), 1e6, None);
    
    // Reference voltage source
    circuit.add_branch("VREF".to_string(), vref_name, gnd_name, 
                      "VoltageSource".to_string(), 0.8, None);
    
    // Input voltage source
    circuit.add_branch("VIN".to_string(), vin_name, gnd_name, 
                      "VoltageSource".to_string(), vin, None);
    
    // Load
    let rload = vout / iout;
    circuit.add_branch("RLOAD".to_string(), vout_name, gnd_name, 
                      "Resistor".to_string(), rload, None);
    
    // Buck switch (simplified as voltage-controlled switch)
    circuit.add_branch("SWITCH".to_string(), vin_name, sw_name, 
                      "Switch".to_string(), 1.0, None);
    
    circuit
}

fn add_input_filter(circuit: &mut Circuit, l_filter: f64, c_filter: f64, r_damp: f64) {
    // Add LC input filter with optional damping
    circuit.add_node("VIN_FILTERED".to_string(), None);
    circuit.add_node("FILTER_MID".to_string(), None);
    
    // Filter inductor
    circuit.add_branch("L_FILTER".to_string(), "VIN", "VIN_FILTERED", 
                      "Inductor".to_string(), l_filter, None);
    
    // Filter capacitor with damping resistor
    circuit.add_branch("C_FILTER".to_string(), "VIN_FILTERED", "FILTER_MID", 
                      "Capacitor".to_string(), c_filter, None);
    circuit.add_branch("R_DAMP".to_string(), "FILTER_MID", "GND", 
                      "Resistor".to_string(), r_damp, None);
}

fn analyze_and_report(circuit: Circuit, name: &str) {
    let mut analyzer = PowerConverterStabilityAnalyzer::new(circuit);
    
    // Register converter
    analyzer.add_converter(name.to_string(), ConverterNodes {
        input: NodeId::new(0),  // VIN
        output: NodeId::new(2), // VOUT
        feedback: Some(NodeId::new(3)), // FB
        compensation: Some(NodeId::new(4)), // COMP
        ground: NodeId::new(5), // GND
    });
    
    // Run analysis
    let result = match analyzer.analyze_stability(name) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("❌ Analysis failed: {}", e);
            return;
        }
    };
    
    // Print results
    println!("📊 Stability Metrics:");
    println!("  Phase Margin: {:.1}° {}", 
        result.loop_stability.phase_margin_deg,
        if result.loop_stability.phase_margin_deg > 45.0 { "✅" } else { "⚠️" });
    println!("  Gain Margin: {:.1} dB {}",
        result.loop_stability.gain_margin_db,
        if result.loop_stability.gain_margin_db > 10.0 { "✅" } else { "⚠️" });
    println!("  Crossover: {:.1} kHz", 
        result.loop_stability.crossover_frequency_hz / 1000.0);
    println!("  DC Loop Gain: {:.1} dB", result.loop_stability.dc_loop_gain_db);
    
    // Impedances
    println!("\n📈 Impedance Profile:");
    let freqs = [100.0, 1e3, 10e3, 100e3];
    for freq in freqs {
        println!("  Zout @ {:.0}Hz: {:.1} mΩ", 
            freq, 
            get_z_at_freq(&result.output_impedance, freq) * 1000.0);
    }
    
    // Resonances
    if !result.resonances.is_empty() {
        println!("\n🔔 Resonances:");
        for res in &result.resonances {
            println!("  {:.1} kHz: Q={:.1}, Damping={:?}",
                res.frequency_hz / 1000.0,
                res.q_factor,
                res.damping);
        }
    }
    
    // Overall status
    println!("\n✅ Overall: {}", 
        if result.is_stable { "STABLE" } else { "UNSTABLE ⚠️" });
    
    // Warnings
    if !result.warnings.is_empty() {
        println!("\n⚠️  Warnings:");
        for warning in &result.warnings {
            match warning {
                StabilityWarning::LowPhaseMargin { margin_deg, minimum_deg } => {
                    println!("  - Phase margin {:.1}° below {:.1}°", margin_deg, minimum_deg);
                }
                StabilityWarning::LowGainMargin { margin_db, minimum_db } => {
                    println!("  - Gain margin {:.1}dB below {:.1}dB", margin_db, minimum_db);
                }
                StabilityWarning::HighQResonance { frequency_hz, q_factor } => {
                    println!("  - High-Q resonance (Q={:.1}) at {:.1}kHz", 
                        q_factor, frequency_hz / 1000.0);
                }
                _ => {}
            }
        }
        
        // Generate and show recommendations
        let recommendations = analyzer.generate_recommendations(&result);
        if !recommendations.is_empty() {
            println!("\n💡 Recommendations:");
            for rec in &recommendations {
                match rec {
                    StabilityRecommendation::IncreasePhaseMargin { current_deg, target_deg, suggestions } => {
                        println!("  📐 Increase Phase Margin ({:.1}° → {:.1}°):", current_deg, target_deg);
                        for suggestion in suggestions {
                            println!("     • {}", suggestion);
                        }
                    }
                    StabilityRecommendation::DampResonance { frequency_hz, q_factor, damping_resistor_ohms, suggestions } => {
                        println!("  🔧 Damp Resonance at {:.1}kHz (Q={:.1}):", frequency_hz / 1000.0, q_factor);
                        for suggestion in suggestions {
                            println!("     • {}", suggestion);
                        }
                    }
                    StabilityRecommendation::FixCascadeInteraction { source_converter, load_converter, frequency_hz, suggestions } => {
                        println!("  🔗 Fix Cascade Interaction {} → {} at {:.1}kHz:", 
                            source_converter, load_converter, frequency_hz / 1000.0);
                        for suggestion in suggestions {
                            println!("     • {}", suggestion);
                        }
                    }
                    StabilityRecommendation::GeneralStability { suggestions } => {
                        println!("  📋 General Stability Improvements:");
                        for suggestion in suggestions {
                            println!("     • {}", suggestion);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn get_z_at_freq(profile: &bhdl_spice::stability::ImpedanceProfile, freq: f64) -> f64 {
    let idx = profile.frequencies.iter()
        .position(|&f| f >= freq)
        .unwrap_or(0);
    profile.magnitude_ohms[idx]
}