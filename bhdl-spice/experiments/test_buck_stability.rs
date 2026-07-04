//! Test realistic buck converter stability analysis
//! 
//! Creates a detailed buck converter model with:
//! - Realistic component values
//! - Compensation network
//! - ESR effects
//! - Load variations

use bhdl_spice::{
    Circuit, NodeId,
    stability::{
        PowerConverterStabilityAnalyzer, ConverterNodes,
        StabilityWarning,
    },
};

fn main() {
    println!("Realistic Buck Converter Stability Analysis");
    println!("==========================================\n");
    
    // Test multiple buck converter designs
    test_basic_buck();
    test_compensated_buck();
    test_cascaded_bucks();
}

fn test_basic_buck() {
    println!("1. Basic Buck Converter (12V to 3.3V @ 2A)");
    println!("------------------------------------------\n");
    
    let circuit = create_basic_buck();
    analyze_converter(circuit, "BasicBuck");
}

fn test_compensated_buck() {
    println!("\n2. Compensated Buck Converter (12V to 5V @ 3A)");
    println!("-----------------------------------------------\n");
    
    let circuit = create_compensated_buck();
    analyze_converter(circuit, "CompensatedBuck");
}

fn test_cascaded_bucks() {
    println!("\n3. Cascaded Buck Converters (24V->12V->5V->3.3V)");
    println!("-------------------------------------------------\n");
    
    let circuit = create_cascaded_bucks();
    analyze_cascaded_system(circuit);
}

fn create_basic_buck() -> Circuit {
    let mut circuit = Circuit::new();
    
    // Nodes
    let vin = circuit.add_node("VIN".to_string(), None);
    let sw = circuit.add_node("SW".to_string(), None);
    let vout = circuit.add_node("VOUT".to_string(), None);
    let fb = circuit.add_node("FB".to_string(), None);
    let comp = circuit.add_node("COMP".to_string(), None);
    let gnd = circuit.add_node("GND".to_string(), None);
    let vref = circuit.add_node("VREF".to_string(), None);
    
    // Input capacitor with ESR
    let cin_node = circuit.add_node("CIN_NODE".to_string(), None);
    circuit.add_branch("CIN".to_string(), "VIN", "CIN_NODE", "Capacitor".to_string(), 220e-6, None);
    circuit.add_branch("CIN_ESR".to_string(), "CIN_NODE", "GND", "Resistor".to_string(), 0.05, None);
    
    // Power inductor (22µH with DCR)
    circuit.add_branch("L1".to_string(), "SW", "VOUT", "Inductor".to_string(), 22e-6, None);
    circuit.add_branch("L1_DCR".to_string(), "SW", "VOUT", "Resistor".to_string(), 0.015, None);
    
    // Output capacitors with ESR (parallel combination)
    let cout1_node = circuit.add_node("COUT1_NODE".to_string(), None);
    let cout2_node = circuit.add_node("COUT2_NODE".to_string(), None);
    
    // First output cap: 100µF electrolytic with higher ESR
    circuit.add_branch("COUT1".to_string(), "VOUT", "COUT1_NODE", "Capacitor".to_string(), 100e-6, None);
    circuit.add_branch("COUT1_ESR".to_string(), "COUT1_NODE", "GND", "Resistor".to_string(), 0.08, None);
    
    // Second output cap: 22µF ceramic with low ESR
    circuit.add_branch("COUT2".to_string(), "VOUT", "COUT2_NODE", "Capacitor".to_string(), 22e-6, None);
    circuit.add_branch("COUT2_ESR".to_string(), "COUT2_NODE", "GND", "Resistor".to_string(), 0.005, None);
    
    // Feedback divider (3.3V to 0.8V)
    circuit.add_branch("RFB_TOP".to_string(), "VOUT", "FB", "Resistor".to_string(), 31600.0, None);
    circuit.add_branch("RFB_BOT".to_string(), "FB", "GND", "Resistor".to_string(), 10000.0, None);
    
    // Simple Type II compensation
    circuit.add_branch("RCOMP".to_string(), "FB", "COMP", "Resistor".to_string(), 47000.0, None);
    circuit.add_branch("CCOMP".to_string(), "COMP", "GND", "Capacitor".to_string(), 2.2e-9, None);
    
    // Error amplifier (behavioral model)
    circuit.add_branch("EA".to_string(), "VREF", "FB", "OpAmp".to_string(), 1e6, None);
    
    // Load
    circuit.add_branch("RLOAD".to_string(), "VOUT", "GND", "Resistor".to_string(), 1.65, None); // 2A @ 3.3V
    
    circuit
}

fn create_compensated_buck() -> Circuit {
    let mut circuit = Circuit::new();
    
    // Nodes
    let vin = circuit.add_node("VIN".to_string(), None);
    let sw = circuit.add_node("SW".to_string(), None);
    let vout = circuit.add_node("VOUT".to_string(), None);
    let fb = circuit.add_node("FB".to_string(), None);
    let comp = circuit.add_node("COMP".to_string(), None);
    let gnd = circuit.add_node("GND".to_string(), None);
    let vref = circuit.add_node("VREF".to_string(), None);
    let comp_zero = circuit.add_node("COMP_ZERO".to_string(), None);
    
    // Input filtering
    let cin_node = circuit.add_node("CIN_NODE".to_string(), None);
    circuit.add_branch("CIN1".to_string(), "VIN", "CIN_NODE", "Capacitor".to_string(), 470e-6, None);
    circuit.add_branch("CIN1_ESR".to_string(), "CIN_NODE", "GND", "Resistor".to_string(), 0.02, None);
    
    // Add input ceramic for high frequency
    circuit.add_branch("CIN2".to_string(), "VIN", "GND", "Capacitor".to_string(), 10e-6, None);
    
    // Power inductor (15µH with DCR)
    circuit.add_branch("L1".to_string(), "SW", "VOUT", "Inductor".to_string(), 15e-6, None);
    circuit.add_branch("L1_DCR".to_string(), "SW", "VOUT", "Resistor".to_string(), 0.008, None);
    
    // Output capacitor bank
    let cout1_node = circuit.add_node("COUT1_NODE".to_string(), None);
    let cout2_node = circuit.add_node("COUT2_NODE".to_string(), None);
    let cout3_node = circuit.add_node("COUT3_NODE".to_string(), None);
    
    // Bulk cap: 220µF electrolytic
    circuit.add_branch("COUT1".to_string(), "VOUT", "COUT1_NODE", "Capacitor".to_string(), 220e-6, None);
    circuit.add_branch("COUT1_ESR".to_string(), "COUT1_NODE", "GND", "Resistor".to_string(), 0.05, None);
    
    // Mid-frequency: 47µF ceramic
    circuit.add_branch("COUT2".to_string(), "VOUT", "COUT2_NODE", "Capacitor".to_string(), 47e-6, None);
    circuit.add_branch("COUT2_ESR".to_string(), "COUT2_NODE", "GND", "Resistor".to_string(), 0.003, None);
    
    // High-frequency: 10µF ceramic
    circuit.add_branch("COUT3".to_string(), "VOUT", "COUT3_NODE", "Capacitor".to_string(), 10e-6, None);
    circuit.add_branch("COUT3_ESR".to_string(), "COUT3_NODE", "GND", "Resistor".to_string(), 0.002, None);
    
    // Feedback divider (5V to 0.8V)
    circuit.add_branch("RFB_TOP".to_string(), "VOUT", "FB", "Resistor".to_string(), 52300.0, None);
    circuit.add_branch("RFB_BOT".to_string(), "FB", "GND", "Resistor".to_string(), 10000.0, None);
    
    // Type III compensation network
    // Primary integrator
    circuit.add_branch("RCOMP1".to_string(), "FB", "COMP", "Resistor".to_string(), 22000.0, None);
    circuit.add_branch("CCOMP1".to_string(), "COMP", "GND", "Capacitor".to_string(), 4.7e-9, None);
    
    // Zero-pole pair
    circuit.add_branch("RCOMP2".to_string(), "COMP", "COMP_ZERO", "Resistor".to_string(), 6800.0, None);
    circuit.add_branch("CCOMP2".to_string(), "COMP_ZERO", "FB", "Capacitor".to_string(), 680e-12, None);
    
    // High frequency pole
    circuit.add_branch("CCOMP3".to_string(), "COMP", "FB", "Capacitor".to_string(), 47e-12, None);
    
    // Error amplifier
    circuit.add_branch("EA".to_string(), "VREF", "FB", "OpAmp".to_string(), 2e6, None);
    
    // Load
    circuit.add_branch("RLOAD".to_string(), "VOUT", "GND", "Resistor".to_string(), 1.67, None); // 3A @ 5V
    
    circuit
}

fn create_cascaded_bucks() -> Circuit {
    let mut circuit = Circuit::new();
    
    // Main power rail nodes
    let vin = circuit.add_node("VIN_24V".to_string(), None);
    let v12 = circuit.add_node("V12".to_string(), None);
    let v5 = circuit.add_node("V5".to_string(), None);
    let v3_3 = circuit.add_node("V3_3".to_string(), None);
    let gnd = circuit.add_node("GND".to_string(), None);
    
    // Stage 1: 24V to 12V buck (simplified)
    let sw1 = circuit.add_node("SW1".to_string(), None);
    circuit.add_branch("L1".to_string(), "SW1", "V12", "Inductor".to_string(), 33e-6, None);
    circuit.add_branch("C1".to_string(), "V12", "GND", "Capacitor".to_string(), 330e-6, None);
    circuit.add_branch("C1_ESR".to_string(), "V12", "GND", "Resistor".to_string(), 0.03, None);
    
    // Stage 2: 12V to 5V buck
    let sw2 = circuit.add_node("SW2".to_string(), None);
    circuit.add_branch("L2".to_string(), "SW2", "V5", "Inductor".to_string(), 22e-6, None);
    circuit.add_branch("C2".to_string(), "V5", "GND", "Capacitor".to_string(), 220e-6, None);
    circuit.add_branch("C2_ESR".to_string(), "V5", "GND", "Resistor".to_string(), 0.04, None);
    
    // Add input cap for stage 2 (critical for cascade stability)
    circuit.add_branch("CIN2".to_string(), "V12", "GND", "Capacitor".to_string(), 100e-6, None);
    
    // Stage 3: 5V to 3.3V buck
    let sw3 = circuit.add_node("SW3".to_string(), None);
    circuit.add_branch("L3".to_string(), "SW3", "V3_3", "Inductor".to_string(), 15e-6, None);
    circuit.add_branch("C3".to_string(), "V3_3", "GND", "Capacitor".to_string(), 150e-6, None);
    circuit.add_branch("C3_ESR".to_string(), "V3_3", "GND", "Resistor".to_string(), 0.05, None);
    
    // Add input cap for stage 3
    circuit.add_branch("CIN3".to_string(), "V5", "GND", "Capacitor".to_string(), 47e-6, None);
    
    // Loads on each rail
    circuit.add_branch("LOAD_12V".to_string(), "V12", "GND", "Resistor".to_string(), 12.0, None); // 1A
    circuit.add_branch("LOAD_5V".to_string(), "V5", "GND", "Resistor".to_string(), 5.0, None); // 1A
    circuit.add_branch("LOAD_3_3V".to_string(), "V3_3", "GND", "Resistor".to_string(), 3.3, None); // 1A
    
    circuit
}

fn analyze_converter(circuit: Circuit, name: &str) {
    let mut analyzer = PowerConverterStabilityAnalyzer::new(circuit);
    
    // Register converter nodes
    analyzer.add_converter(name.to_string(), ConverterNodes {
        input: NodeId::new(0),  // VIN
        output: NodeId::new(2), // VOUT
        feedback: Some(NodeId::new(3)), // FB
        compensation: Some(NodeId::new(4)), // COMP
        ground: NodeId::new(5), // GND
    });
    
    // Perform analysis
    match analyzer.analyze_stability(name) {
        Ok(result) => {
            // Loop stability metrics
            println!("Loop Stability:");
            println!("  Phase Margin: {:.1}° {}", 
                result.loop_stability.phase_margin_deg,
                if result.loop_stability.phase_margin_deg > 45.0 { "✅" } else { "⚠️" });
            println!("  Gain Margin: {:.1} dB {}",
                result.loop_stability.gain_margin_db,
                if result.loop_stability.gain_margin_db > 10.0 { "✅" } else { "⚠️" });
            println!("  Crossover: {:.1} kHz", result.loop_stability.crossover_frequency_hz / 1000.0);
            println!("  Bandwidth: {:.1} kHz", result.loop_stability.bandwidth_hz / 1000.0);
            
            // Impedance characteristics
            println!("\nImpedance Profile:");
            println!("  Zin @ 1kHz: {:.1} Ω", get_impedance_at_freq(&result.input_impedance, 1000.0));
            println!("  Zout @ 1kHz: {:.1} mΩ", get_impedance_at_freq(&result.output_impedance, 1000.0) * 1000.0);
            println!("  Zout @ 10kHz: {:.1} mΩ", get_impedance_at_freq(&result.output_impedance, 10000.0) * 1000.0);
            
            // Resonances
            if !result.resonances.is_empty() {
                println!("\nResonances Detected:");
                for res in &result.resonances {
                    println!("  {:.1} kHz: Q={:.1}, Damping={:?}, Peak={:.1}Ω",
                        res.frequency_hz / 1000.0,
                        res.q_factor,
                        res.damping,
                        res.peak_impedance_ohms);
                }
            }
            
            // Overall stability
            println!("\nOverall: {} {}", 
                name,
                if result.is_stable { "is STABLE ✅" } else { "has STABILITY ISSUES ❌" });
            
            // Warnings
            if !result.warnings.is_empty() {
                println!("\nWarnings:");
                for warning in &result.warnings {
                    match warning {
                        StabilityWarning::LowPhaseMargin { margin_deg, minimum_deg } => {
                            println!("  ⚠️  Phase margin {:.1}° is below {:.1}°", margin_deg, minimum_deg);
                        }
                        StabilityWarning::LowGainMargin { margin_db, minimum_db } => {
                            println!("  ⚠️  Gain margin {:.1}dB is below {:.1}dB", margin_db, minimum_db);
                        }
                        StabilityWarning::HighQResonance { frequency_hz, q_factor } => {
                            println!("  ⚠️  High-Q resonance (Q={:.1}) at {:.1}kHz", q_factor, frequency_hz / 1000.0);
                        }
                        _ => {}
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Analysis failed: {}", e);
        }
    }
}

fn analyze_cascaded_system(circuit: Circuit) {
    let mut analyzer = PowerConverterStabilityAnalyzer::new(circuit);
    
    // Register all three buck stages
    analyzer.add_converter("Buck_24to12".to_string(), ConverterNodes {
        input: NodeId::new(0),  // VIN_24V
        output: NodeId::new(1), // V12
        feedback: None,
        compensation: None,
        ground: NodeId::new(4), // GND
    });
    
    analyzer.add_converter("Buck_12to5".to_string(), ConverterNodes {
        input: NodeId::new(1),  // V12
        output: NodeId::new(2), // V5
        feedback: None,
        compensation: None,
        ground: NodeId::new(4), // GND
    });
    
    analyzer.add_converter("Buck_5to3.3".to_string(), ConverterNodes {
        input: NodeId::new(2),  // V5
        output: NodeId::new(3), // V3_3
        feedback: None,
        compensation: None,
        ground: NodeId::new(4), // GND
    });
    
    // Analyze the middle converter to check cascade interactions
    match analyzer.analyze_stability("Buck_12to5") {
        Ok(result) => {
            println!("Individual Converter Analysis (12V->5V):");
            println!("  Phase Margin: {:.1}°", result.loop_stability.phase_margin_deg);
            println!("  Gain Margin: {:.1} dB", result.loop_stability.gain_margin_db);
            
            if let Some(cascade) = &result.cascade_stability {
                println!("\nCascade Stability Analysis:");
                println!("  Overall: {}", if cascade.is_stable { "STABLE ✅" } else { "UNSTABLE ❌" });
                println!("  Stability Margin: {:.1} dB", cascade.stability_margin_db);
                
                // Show critical interactions
                println!("\nCritical Impedance Interactions:");
                for interaction in &cascade.impedance_interactions {
                    if interaction.violation_ratio > 0.5 {
                        println!("  {} → {}", interaction.source_converter, interaction.load_converter);
                        println!("    Frequency: {:.1} kHz", interaction.frequency_hz / 1000.0);
                        println!("    Z ratio: {:.2} (limit: 0.5)", interaction.impedance_ratio);
                        println!("    Violation: {:.1}x", interaction.violation_ratio);
                    }
                }
                
                // Beat frequencies
                if !cascade.beat_frequencies.is_empty() {
                    println!("\nBeat Frequencies:");
                    for beat in &cascade.beat_frequencies {
                        println!("  {} ↔ {}: {:.1} kHz ({:?})",
                            beat.converter1,
                            beat.converter2,
                            beat.beat_frequency_hz / 1000.0,
                            beat.issue);
                    }
                }
                
                // Recommendations
                if !cascade.recommendations.is_empty() {
                    println!("\nStability Improvements:");
                    for (i, rec) in cascade.recommendations.iter().enumerate() {
                        println!("  {}. {:?}", i + 1, rec);
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Cascade analysis failed: {}", e);
        }
    }
}

fn get_impedance_at_freq(profile: &bhdl_spice::stability::ImpedanceProfile, freq: f64) -> f64 {
    let idx = profile.frequencies.iter()
        .position(|&f| f >= freq)
        .unwrap_or(0);
    profile.magnitude_ohms[idx]
}