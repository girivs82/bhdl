// Test hierarchical intent propagation through entity instances
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;

fn main() {
    println!("Testing Hierarchical Intent Propagation\n");
    
    let test_bhdl = r#"
// Entity that will inherit intent from parent
entity Filter(cutoff: frequency) {
    pin IN: signal in;
    pin OUT: signal out;
    pin GND: signal inout;
    
    // Internal filtering implementation
    IN -> Cap(100nF).1 -> OUT;
    Cap(100nF).2 -> GND;
}

// Entity with explicit intent
entity SignalProcessor() {
    pin IN: signal in;
    pin OUT: signal out;
    pin GND: signal inout;
    
    // Local net with intent for analog simulation
    net filtered: IN -> Filter(1kHz).OUT for analog(bandwidth: 10kHz);
    
    // Output processing
    filtered -> Amp(gain: 10).OUT -> OUT;
}

board AudioSystem {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Main signal flow with digital intent
    net audio_in: InputJack.signal -> Preamp.IN for digital();
    
    // This should inherit the digital intent from parent board
    entity sp: SignalProcessor();
    Preamp.OUT -> sp.IN;
    sp.OUT -> OutputJack.signal;
    
    // Entity instance with mixed-signal intent
    net mixed_path: Sensor.out -> ADC.in for mixed_signal(sample_rate: 48kHz);
    entity sp2: SignalProcessor();
    ADC.out -> sp2.IN;
}
"#;
    
    // Parse
    let parse_result = parse(test_bhdl);
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  - {}", error.message);
        }
        return;
    }
    
    let source_file = SourceFile::cast(parse_result.syntax()).expect("Should be a SourceFile");
    
    // Analyze
    let result = analyze(&source_file);
    
    // Check flow tracking results
    println!("=== Flow Tracking Results ===");
    if let Some(ref flow_tracker) = result.flow_tracker {
        println!("Flow paths found: {}", flow_tracker.get_flow_paths().len());
        for (i, flow) in flow_tracker.get_flow_paths().iter().enumerate() {
            println!("\nFlow Path {}:", i + 1);
            println!("  Nets: {:?}", flow.nets);
            println!("  Components: {:?}", flow.components);
            if let Some(ref intent) = flow.intent {
                println!("  Intent: {}({:?})", intent.name, intent.params);
            }
            if let Some(ref result) = flow.intent_result {
                println!("  Sim Mode: {:?}", result.sim_mode);
            }
        }
        
        // Check hierarchical propagation
        println!("\n=== Hierarchical Intent Propagation ===");
        
        // Check if SignalProcessor instance inherits parent intent
        if let Some(mode) = flow_tracker.get_component_sim_mode("SignalProcessor") {
            println!("SignalProcessor entity instance sim mode: {:?}", mode);
        }

        // Check if Filter inside SignalProcessor has its own intent
        if let Some(mode) = flow_tracker.get_component_sim_mode("Filter") {
            println!("Filter entity instance sim mode: {:?}", mode);
        }
        
        println!("\nRequired simulation mode: {:?}", flow_tracker.get_required_sim_mode());
    } else {
        println!("No flow tracker available");
    }
    
    // Show diagnostics
    println!("\n=== Diagnostics ===");
    if result.diagnostics.is_empty() {
        println!("No diagnostics");
    } else {
        for diag in &result.diagnostics {
            println!("  - {}", diag.message);
        }
    }
}