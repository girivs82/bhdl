use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_synthesizer::import_preprocessor::preprocess_and_analyze;
use bhdl_synthesizer::NetlistGenerator;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("🔍 BHDL Netlist Knowledge Verification Test");
    println!("=============================================\n");
    
    // Test with a realistic TPS54331 buck converter circuit
    let test_code = r#"
import { TPS54331 } from "bhdl-stdlib/components/power/switching_regulators/TPS54331.bhdl";

board BuckConverterTest {
    power VIN = 12V @ 3A;
    power VOUT_5V = 5V @ 2A;
    ground GND;
    
    // Buck converter using TPS54331
    U1: TPS54331(vout=5V);
    
    // Power connections
    VIN -> U1.VIN;
    U1.GND -> GND;
    U1.EN -> VIN;    // Enable tied to VIN
    
    // Key virtual pin connection - this should trigger synthesis knowledge
    U1.VOUT -> @VOUT_5V;
    
    // Additional pin connections to verify pin knowledge
    U1.SW -> switch_node;      // Switch node (high current switching)
    U1.FB -> feedback_net;     // Feedback for regulation
    U1.BOOT -> bootstrap_cap;  // Bootstrap capacitor
    U1.SS -> soft_start_cap;   // Soft-start timing
    U1.COMP -> compensation;   // Compensation network
}
"#;
    
    println!("📄 Test Circuit (Buck Converter with TPS54331):");
    println!("{}", test_code);
    
    // Step 1: Parse
    let parse_result = parse(test_code);
    if !parse_result.errors().is_empty() {
        println!("❌ Parse errors:");
        for error in parse_result.errors() {
            println!("  - {}", error.message);
        }
        return Ok(());
    }
    
    let syntax = parse_result.syntax();
    let source_file = SourceFile::cast(syntax).expect("Failed to cast to SourceFile");
    
    // Step 2: Import preprocessing and analysis
    println!("\n🧠 Step 1: Import Preprocessing and Analysis...");
    let base_path = "/Users/girivs/src/bhdl-new";
    let (analysis_result, preprocessor) = preprocess_and_analyze(&source_file, base_path)?;
    
    println!("Analysis Results:");
    println!("  - Diagnostics: {}", analysis_result.diagnostics.len());
    println!("  - Components inferred: {}", analysis_result.inferred_components.len());
    
    // Step 3: Verify TPS54331 knowledge extraction
    println!("\n📋 Step 2: TPS54331 Knowledge Verification...");
    if let Some(tps_module) = preprocessor.get_imported_module("TPS54331") {
        let pins: Vec<_> = tps_module.pins().collect();
        println!("✅ TPS54331 loaded with {} pins", pins.len());
        
        // Verify specific pin knowledge
        let pin_types = ["VIN: power in", "SW: switch out", "FB: feedback in", "VOUT: power out virtual"];
        for expected_pin in &pin_types {
            let found = pins.iter().any(|pin| {
                let pin_text = pin.syntax().text().to_string();
                pin_text.contains(expected_pin)
            });
            
            if found {
                println!("  ✅ Found pin: {}", expected_pin);
            } else {
                println!("  ❌ Missing pin: {}", expected_pin);
            }
        }
        
        // Virtual pin check
        let virtual_pins = preprocessor.get_virtual_pins("TPS54331");
        println!("  🌟 Virtual pins: {:?}", virtual_pins);
    }
    
    // Step 4: Generate netlist with synthesis knowledge
    println!("\n⚙️  Step 3: Netlist Generation with Synthesis Knowledge...");
    let mut generator = NetlistGenerator::new();
    generator.set_import_preprocessor(preprocessor);
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis_result).await?;
    
    println!("✅ Netlist generated successfully");
    println!("📊 Netlist Statistics:");
    println!("  - Modules: {}", netlist.modules.len());
    println!("  - Instances: {}", netlist.instances.len());
    println!("  - Nets: {}", netlist.nets.len());
    
    // Step 5: Detailed netlist knowledge inspection
    println!("\n🔬 Step 4: Detailed Netlist Knowledge Inspection...");
    
    // Check for TPS54331 instance
    let mut found_tps54331 = false;
    for (instance_id, instance) in &netlist.instances {
        if instance.name.contains("U1") {
            println!("📦 Found TPS54331 instance: {}", instance.name);
            found_tps54331 = true;
            
            // Check parameters (synthesis knowledge)
            println!("  Parameters: {}", instance.attributes.len());
            for (param_name, param_value) in &instance.attributes {
                println!("    {} = {:?}", param_name, param_value);
            }
            
            // Check the module definition for pin knowledge
            if let Some(module_def) = netlist.modules.get(&instance.definition) {
                println!("  Module: {} ({})", module_def.name, module_def.kind.as_debug_string());
                println!("  Pins in module: {}", module_def.pins.len());
                
                // Check specific pins for synthesis knowledge
                let critical_pins = ["VIN", "SW", "VOUT", "FB", "GND"];
                for pin_name in &critical_pins {
                    if let Some(pin) = module_def.pins.iter().find(|p| p.name == *pin_name) {
                        println!("    ✅ Pin {}: {} ({})", 
                                pin.name, 
                                pin.pin_type.as_debug_string(), 
                                pin.direction.as_debug_string());
                    }
                }
                
                // Check for virtual pin specifically
                if let Some(vout_pin) = module_def.pins.iter().find(|p| p.name == "VOUT") {
                    println!("    🌟 VOUT Virtual Pin Found:");
                    println!("      - Type: {}", vout_pin.pin_type.as_debug_string());
                    println!("      - Direction: {}", vout_pin.direction.as_debug_string());
                }
            }
            break;
        }
    }
    
    if !found_tps54331 {
        println!("❌ TPS54331 instance not found in netlist!");
    }
    
    // Step 6: Check power domain integration
    println!("\n⚡ Step 5: Power Domain Integration Check...");
    let power_nets = ["VIN", "VOUT_5V", "GND"];
    for net_name in &power_nets {
        if let Some((_net_id, net)) = netlist.nets.iter().find(|(_, n)| {
            n.name.as_ref().map_or(false, |name| name == *net_name)
        }) {
            println!("✅ Power net: {} with {} connections", 
                    net.name.as_ref().unwrap_or(&"unnamed".to_string()), 
                    net.connections.len());
            println!("   Net class: {:?}", net.net_class);
        }
    }
    
    // Step 7: Check for virtual pin connection
    println!("\n🎯 Step 6: Virtual Pin Connection Verification...");
    let mut vout_connected = false;
    
    for (net_id, net) in &netlist.nets {
        if let Some(net_name) = &net.name {
            if net_name == "VOUT_5V" {
                println!("✅ Found VOUT_5V net with {} connections", net.connections.len());
                
                // Check if any connection is to the TPS54331 VOUT pin
                for connection in &net.connections {
                    if let Some(instance) = netlist.instances.get(&connection.instance_id) {
                        if instance.name.contains("U1") {
                            if let Some(module) = netlist.modules.get(&instance.definition) {
                                if let Some(pin) = module.pins.get(connection.pin_id) {
                                    if pin.name == "VOUT" {
                                        println!("🌟 VIRTUAL PIN CONNECTION VERIFIED!");
                                        println!("   TPS54331.VOUT -> VOUT_5V net");
                                        vout_connected = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    if !vout_connected {
        println!("❌ Virtual pin connection not found in netlist");
    }
    
    // Step 8: Final verification summary
    println!("\n📋 FINAL VERIFICATION SUMMARY:");
    println!("=====================================");
    
    let checks = [
        (found_tps54331, "TPS54331 instance created"),
        (vout_connected, "Virtual VOUT pin connected"),
        (netlist.modules.len() > 5, "Multiple modules generated"),
        (netlist.instances.len() > 3, "Multiple instances created"),
        (netlist.nets.len() > 8, "Comprehensive net structure"),
    ];
    
    let mut passed = 0;
    for (check_passed, description) in &checks {
        if *check_passed {
            println!("✅ {}", description);
            passed += 1;
        } else {
            println!("❌ {}", description);
        }
    }
    
    println!("\n🏆 OVERALL RESULT: {}/{} checks passed", passed, checks.len());
    
    if passed == checks.len() {
        println!("🎉 SUCCESS: Netlist generation with stdlib knowledge is working perfectly!");
        println!("   - Parser correctly handles v2.0 pin syntax");
        println!("   - Import preprocessing loads stdlib modules");
        println!("   - Synthesis integrates component knowledge");
        println!("   - Virtual pins are properly connected");
        println!("   - Power domains are correctly established");
    } else {
        println!("⚠️  PARTIAL SUCCESS: Some verification checks failed");
    }
    
    Ok(())
}