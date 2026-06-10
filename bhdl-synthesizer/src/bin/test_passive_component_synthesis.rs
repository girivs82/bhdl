// Test passive component synthesis with proper power ratings and voltage selection
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    println!("Testing passive component synthesis with proper parameters...");
    
    // Test board with different voltage domains and power levels to verify
    // passive component selection matches the required specs
    let content = r#"
board PassiveTestBoard {
    // Different power domains to test parameter selection
    power VCC_3V3 = 3.3V @ 500mA;
    power VCC_5V = 5V @ 1A;
    power VCC_12V = 12V @ 2A;
    ground GND;
    
    // Low power signal circuit - should use small resistors/caps
    net signal_3v3: VCC_3V3 -> filter_3v3: FilterModule().VIN -> filter_3v3.VOUT -> led_3v3: LED(red).A
        for signal_amplification(3dB, 1MHz);
    
    // Medium power 5V circuit - should use higher power resistors
    net power_5v: VCC_5V -> filter_5v: FilterModule().VIN -> filter_5v.VOUT -> load_5v: PowerModule().VIN
        for power_output_protection(800mA, 5V);
    
    // High voltage 12V circuit - should use high voltage caps and power resistors
    net power_12v: VCC_12V -> filter_12v: FilterModule().VIN -> filter_12v.VOUT -> motor: MotorModule().VIN
        for input_protection(12V, 2A);
    
    // Ground connections
    GND <-> filter_3v3.GND;
    GND <-> filter_5v.GND;
    GND <-> filter_12v.GND;
    GND <-> led_3v3.K;
    GND <-> load_5v.GND;
    GND <-> motor.GND;
}

entity FilterModule() {
    pin VIN: power in;
    pin GND: ground inout;
    
    // Virtual pins for different protection levels based on intent
    pin VOUT: virtual power out;  // Should get appropriate decoupling caps
}

entity PowerModule() {
    pin VIN: power in;
    pin GND: ground inout;
}

entity MotorModule() {
    pin VIN: power in;
    pin GND: ground inout;
}
"#;

    // Parse and analyze
    let parsed = parse(content);
    if !parsed.errors().is_empty() {
        println!("Parse errors:");
        for error in parsed.errors() {
            println!("  - {}", error.message);
        }
        return Ok(());
    }

    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    
    println!("Running analysis with voltage-aware passive component synthesis...");
    let analysis = analyze(&source_file);
    
    if !analysis.diagnostics.is_empty() {
        println!("Analysis diagnostics ({} total):", analysis.diagnostics.len());
        for (i, diag) in analysis.diagnostics.iter().enumerate() {
            if i < 5 { // Limit diagnostic display
                println!("  - {}", diag.message);
            }
        }
        if analysis.diagnostics.len() > 5 {
            println!("  ... and {} more diagnostics", analysis.diagnostics.len() - 5);
        }
        println!();
    }
    
    // Generate netlist with voltage-aware passive component synthesis
    println!("Generating netlist with voltage-aware passive component synthesis...");
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await?;
    
    // Analyze synthesis results for proper passive component selection
    println!("Passive component synthesis analysis:");
    println!("  Modules: {}", netlist.modules.len());
    println!("  Instances: {}", netlist.instances.len());
    println!("  Nets: {}", netlist.nets.len());
    
    // Show power domains and their voltage levels
    println!("\nPower domains detected:");
    for (_net_id, net) in &netlist.nets {
        if let Some(ref name) = net.name {
            match &net.net_class {
                bhdl_netlist::NetClass::Power { voltage, .. } => {
                    println!("  {} - {}V power domain", name, voltage);
                },
                bhdl_netlist::NetClass::Ground => {
                    println!("  {} - Ground reference", name);
                },
                _ => {}
            }
        }
    }
    
    // Show module instances and analyze what passive components should be created
    println!("\nModule instances and expected passive components:");
    for (_instance_id, instance) in &netlist.instances {
        if let Some(module) = netlist.modules.get(instance.definition) {
            println!("  Instance: {} ({})", instance.name, module.name);
            
            // Check for attributes that would inform passive component selection
            if !instance.attributes.is_empty() {
                println!("    Attributes:");
                for (attr_name, attr_value) in &instance.attributes {
                    println!("      {}: {}", attr_name, attr_value);
                }
            }
            
            // Analyze what passive components should be synthesized based on context
            if module.name == "FilterModule" {
                analyze_filter_passive_requirements(&instance.name, &netlist);
            }
        }
    }
    
    // Show flow tracker results and intent-driven component selection
    if let Some(ref flow_tracker) = analysis.flow_tracker {
        let flow_paths = flow_tracker.get_flow_paths();
        println!("\nFlow-based passive component requirements:");
        
        for flow in flow_paths {
            if let Some(ref intent) = flow.intent {
                println!("  Intent: {} ({} nets)", intent.name, flow.nets.len());
                
                // Analyze what passive components this intent should generate
                match intent.name.as_str() {
                    "signal_amplification" => {
                        println!("    → Should generate: Low-noise decoupling caps (X7R, 0402/0603)");
                        println!("    → Resistors: Standard power (125mW 0805)");
                    },
                    "power_output_protection" => {
                        println!("    → Should generate: Higher current caps (X5R, 1206)");
                        println!("    → Resistors: Higher power (250mW+ 1206/2010)");
                    },
                    "input_protection" => {
                        let voltage = flow.intent.as_ref().unwrap().params.iter()
                            .find_map(|p| match p {
                                bhdl_common::IntentParam::Positional(bhdl_common::IntentValue::Number(v, Some(unit))) 
                                    if unit == "V" => Some(*v),
                                _ => None
                            }).unwrap_or(5.0);
                        let current = flow.intent.as_ref().unwrap().params.iter()
                            .find_map(|p| match p {
                                bhdl_common::IntentParam::Positional(bhdl_common::IntentValue::Number(i, Some(unit))) 
                                    if unit == "A" || unit == "mA" => Some(*i),
                                _ => None
                            }).unwrap_or(1.0);
                            
                        println!("    → Voltage: {}V, Current: {}A", voltage, current);
                        
                        if voltage > 10.0 {
                            println!("    → Should generate: High voltage caps (>25V rating, 1210+)");
                            println!("    → Resistors: High power (500mW+ 2010/2512)");
                        } else if current > 1.0 {
                            println!("    → Should generate: High current caps (Low ESR)");
                            println!("    → Resistors: Higher power (250mW+ 1206)");
                        } else {
                            println!("    → Should generate: Standard caps (X7R, 0805)");
                            println!("    → Resistors: Standard power (125mW 0805)");
                        }
                    },
                    _ => {
                        println!("    → Standard passive component selection");
                    }
                }
            }
        }
    }
    
    println!("\n🔍 Analysis Summary:");
    println!("Current system uses generic module_id placeholders for passive components.");
    println!("Proper implementation should:");
    println!("1. Select resistor power rating based on calculated power dissipation");
    println!("2. Select capacitor voltage rating based on net voltage + safety margin");
    println!("3. Select package size based on power/voltage requirements");
    println!("4. Use intent parameters to determine component specifications");
    println!("5. Reference bhdl-stdlib electrical parameters for realistic values");
    
    println!("\n✓ Passive component synthesis analysis completed");
    
    Ok(())
}

/// Analyze what passive components should be generated for a filter module
fn analyze_filter_passive_requirements(instance_name: &str, netlist: &bhdl_netlist::Netlist) {
    println!("    Expected passive components for {}:", instance_name);
    
    // Find the voltage domain this filter is connected to
    let voltage_level = netlist.nets.values()
        .find_map(|net| {
            if let Some(ref name) = net.name {
                if name.contains(&instance_name) {
                    match &net.net_class {
                        bhdl_netlist::NetClass::Power { voltage, .. } => Some(*voltage),
                        _ => None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap_or(5.0);
        
    // Recommend passive components based on voltage level
    if voltage_level <= 3.3 {
        println!("      - Decoupling caps: 100nF + 10μF, 6.3V rating, X7R, 0603/0805");
        println!("      - Current limit R: 10Ω, 125mW, 0805");
    } else if voltage_level <= 5.0 {
        println!("      - Decoupling caps: 100nF + 22μF, 10V rating, X7R, 0805/1206");
        println!("      - Current limit R: 22Ω, 250mW, 1206");
    } else if voltage_level <= 12.0 {
        println!("      - Decoupling caps: 100nF + 47μF, 25V rating, X5R, 1210");
        println!("      - Current limit R: 47Ω, 500mW, 2010");
        println!("      - Protection: TVS diode 15V standoff");
    } else {
        println!("      - High voltage caps: 25V+ rating required");
        println!("      - High power resistors: 1W+ rating required");
    }
}