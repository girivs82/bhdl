//! Test power converter stability analysis
//! 
//! Demonstrates:
//! - Loop stability analysis (phase/gain margin)
//! - Input/output impedance measurement
//! - Resonance detection
//! - Cascade stability for multi-converter systems

use bhdl_spice::{
    Circuit, NodeId,
    stability::{
        PowerConverterStabilityAnalyzer, ConverterNodes,
        StabilityWarning,
    },
};

fn main() {
    println!("Power Converter Stability Analysis Demo");
    println!("======================================\n");
    
    // Create a simple buck converter circuit
    let circuit = create_buck_converter_circuit();
    
    // Create stability analyzer
    let mut analyzer = PowerConverterStabilityAnalyzer::new(circuit);
    
    // Register the buck converter nodes
    analyzer.add_converter("Buck1".to_string(), ConverterNodes {
        input: NodeId::new(0),  // VIN
        output: NodeId::new(1), // VOUT
        feedback: Some(NodeId::new(2)), // FB
        compensation: Some(NodeId::new(3)), // COMP
        ground: NodeId::new(4), // GND
    });
    
    // Perform stability analysis
    match analyzer.analyze_stability("Buck1") {
        Ok(result) => {
            println!("📊 Stability Analysis Results for Buck1");
            println!("=====================================\n");
            
            // Loop stability
            println!("🔄 Loop Stability:");
            println!("  Phase Margin: {:.1}°", result.loop_stability.phase_margin_deg);
            println!("  Gain Margin: {:.1} dB", result.loop_stability.gain_margin_db);
            println!("  Crossover Frequency: {:.1} kHz", result.loop_stability.crossover_frequency_hz / 1000.0);
            println!("  Bandwidth: {:.1} kHz", result.loop_stability.bandwidth_hz / 1000.0);
            println!("  DC Loop Gain: {:.1} dB", result.loop_stability.dc_loop_gain_db);
            println!("  Nyquist Stable: {}", if result.loop_stability.nyquist_stable { "✅" } else { "❌" });
            
            // Impedance profiles
            println!("\n📈 Impedance Characteristics:");
            println!("  Input Impedance @ 1kHz: {:.2} Ω", 
                get_impedance_at_freq(&result.input_impedance, 1000.0));
            println!("  Output Impedance @ 1kHz: {:.2} mΩ", 
                get_impedance_at_freq(&result.output_impedance, 1000.0) * 1000.0);
            
            // Resonances
            if !result.resonances.is_empty() {
                println!("\n🔔 Detected Resonances:");
                for resonance in &result.resonances {
                    println!("  {:?} at {:.1} kHz (Q={:.1}, Damping: {:?})",
                        resonance.resonance_type,
                        resonance.frequency_hz / 1000.0,
                        resonance.q_factor,
                        resonance.damping);
                }
            }
            
            // Overall assessment
            println!("\n✅ Overall Stability: {}", if result.is_stable { "STABLE" } else { "UNSTABLE" });
            
            // Warnings
            if !result.warnings.is_empty() {
                println!("\n⚠️  Stability Warnings:");
                for warning in &result.warnings {
                    match warning {
                        StabilityWarning::LowPhaseMargin { margin_deg, minimum_deg } => {
                            println!("  - Low phase margin: {:.1}° (minimum: {:.1}°)", margin_deg, minimum_deg);
                        }
                        StabilityWarning::LowGainMargin { margin_db, minimum_db } => {
                            println!("  - Low gain margin: {:.1} dB (minimum: {:.1} dB)", margin_db, minimum_db);
                        }
                        StabilityWarning::HighQResonance { frequency_hz, q_factor } => {
                            println!("  - High-Q resonance at {:.1} kHz (Q={:.1})", frequency_hz / 1000.0, q_factor);
                        }
                        _ => {}
                    }
                }
            }
            
            // Test cascade stability with a second converter
            test_cascade_stability();
        }
        Err(e) => {
            eprintln!("❌ Stability analysis failed: {}", e);
        }
    }
}

fn create_buck_converter_circuit() -> Circuit {
    let mut circuit = Circuit::new();
    
    // Add nodes
    let vin = circuit.add_node("VIN".to_string(), None);
    let vout = circuit.add_node("VOUT".to_string(), None);
    let fb = circuit.add_node("FB".to_string(), None);
    let comp = circuit.add_node("COMP".to_string(), None);
    let gnd = circuit.add_node("GND".to_string(), None);
    let sw = circuit.add_node("SW".to_string(), None);
    
    // Input capacitor
    circuit.add_branch("CIN".to_string(), "VIN", "GND", "Capacitor".to_string(), 100e-6, None);
    
    // Buck controller (behavioral model)
    circuit.add_branch("CTRL".to_string(), "VIN", "GND", "BuckController".to_string(), 1.0, None);
    
    // Power stage inductor
    circuit.add_branch("L1".to_string(), "SW", "VOUT", "Inductor".to_string(), 10e-6, None);
    
    // Output capacitor
    circuit.add_branch("COUT".to_string(), "VOUT", "GND", "Capacitor".to_string(), 47e-6, None);
    
    // Feedback resistors
    circuit.add_branch("RFB1".to_string(), "VOUT", "FB", "Resistor".to_string(), 10000.0, None);
    circuit.add_branch("RFB2".to_string(), "FB", "GND", "Resistor".to_string(), 1000.0, None);
    
    // Compensation network
    circuit.add_branch("RCOMP".to_string(), "FB", "COMP", "Resistor".to_string(), 10000.0, None);
    circuit.add_branch("CCOMP".to_string(), "COMP", "GND", "Capacitor".to_string(), 1e-9, None);
    
    // Load
    circuit.add_branch("RLOAD".to_string(), "VOUT", "GND", "Resistor".to_string(), 10.0, None);
    
    circuit
}

fn test_cascade_stability() {
    println!("\n\n🔗 Cascade Stability Analysis");
    println!("============================\n");
    
    // Create a system with two cascaded converters
    let mut circuit = Circuit::new();
    
    // First converter: 12V to 5V buck
    let vin1 = circuit.add_node("VIN1".to_string(), None);
    let vout1 = circuit.add_node("VOUT1".to_string(), None);
    let gnd = circuit.add_node("GND".to_string(), None);
    
    // Second converter: 5V to 3.3V buck (powered from first)
    let vin2 = vout1; // Output of first feeds input of second
    let vout2 = circuit.add_node("VOUT2".to_string(), None);
    
    // Add components for both converters...
    
    let mut analyzer = PowerConverterStabilityAnalyzer::new(circuit);
    
    // Register both converters
    analyzer.add_converter("Buck_12to5".to_string(), ConverterNodes {
        input: vin1,
        output: vout1,
        feedback: None,
        compensation: None,
        ground: gnd,
    });
    
    analyzer.add_converter("Buck_5to3.3".to_string(), ConverterNodes {
        input: vin2,
        output: vout2,
        feedback: None,
        compensation: None,
        ground: gnd,
    });
    
    // Analyze the first converter
    if let Ok(result) = analyzer.analyze_stability("Buck_12to5") {
        if let Some(cascade) = result.cascade_stability {
            println!("Cascade Stability: {}", if cascade.is_stable { "✅ STABLE" } else { "❌ UNSTABLE" });
            println!("Minimum Stability Margin: {:.1} dB", cascade.stability_margin_db);
            
            // Show impedance interactions
            for interaction in &cascade.impedance_interactions {
                if interaction.violation_ratio > 0.5 {
                    println!("\n⚠️  Impedance Interaction:");
                    println!("  {} → {}", interaction.source_converter, interaction.load_converter);
                    println!("  Frequency: {:.1} kHz", interaction.frequency_hz / 1000.0);
                    println!("  Impedance Ratio: {:.2}", interaction.impedance_ratio);
                    println!("  Middlebrook Violation: {:.1}x", interaction.violation_ratio);
                }
            }
            
            // Show recommendations
            if !cascade.recommendations.is_empty() {
                println!("\n💡 Recommendations:");
                for (i, rec) in cascade.recommendations.iter().enumerate() {
                    println!("  {}. {:?}", i + 1, rec);
                }
            }
        }
    }
}

fn get_impedance_at_freq(profile: &bhdl_spice::stability::ImpedanceProfile, target_freq: f64) -> f64 {
    // Find closest frequency point
    let idx = profile.frequencies.iter()
        .position(|&f| f >= target_freq)
        .unwrap_or(profile.frequencies.len() - 1);
    
    profile.magnitude_ohms[idx]
}