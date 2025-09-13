// Test component compatibility analysis capabilities
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::{analyze, AnalysisResult};
use bhdl_synthesizer::{Synthesizer, component_compatibility::ComponentCompatibilityAnalyzer};
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Component Compatibility Analysis Test ===");
    
    // Test with the intelligent design automation demo file
    let test_file = "demo_intelligent_design_automation.bhdl";
    println!("Reading circuit design: {}", test_file);
    
    let bhdl_source = fs::read_to_string(test_file)
        .map_err(|e| format!("Failed to read {}: {}", test_file, e))?;
    
    println!("Parsing BHDL source...");
    let parse_result = parse(&bhdl_source);
    let syntax = parse_result.syntax();
    
    // Run semantic analysis
    println!("Running semantic analysis...");
    let analysis = analyze(&SourceFile::cast(syntax.clone()).unwrap());
    
    // Generate netlist for compatibility analysis
    println!("\nGenerating netlist for compatibility analysis...");
    let mut synthesizer = Synthesizer::new();
    
    // Create a netlist representing the intelligent power system
    use bhdl_netlist::{Netlist, ModuleKind};
    let mut netlist = Netlist::new();
    
    // Add a module definition
    let module_id = netlist.add_module("IntelligentPowerSystem".to_string(), ModuleKind::Board);
    
    // Add components that will be analyzed for compatibility
    println!("Adding components with potential compatibility issues...");
    
    // Linear regulator: LM7805 (needs 7V+ input for 5V output)
    let lm7805_instance = netlist.add_instance("main_reg".to_string(), module_id).unwrap();
    netlist.instances.get_mut(lm7805_instance).unwrap().name = "LM7805".to_string();
    netlist.instances.get_mut(lm7805_instance).unwrap().attributes.insert(
        "input_voltage".to_string(), "24".to_string()
    );
    netlist.instances.get_mut(lm7805_instance).unwrap().attributes.insert(
        "output_voltage".to_string(), "5".to_string()
    );
    netlist.instances.get_mut(lm7805_instance).unwrap().attributes.insert(
        "load_current".to_string(), "1.5".to_string() // 1.5A load
    );
    
    // Switching regulator: TPS54331 (efficient 5V to 3.3V conversion)
    let tps54331_instance = netlist.add_instance("switch_reg".to_string(), module_id).unwrap();
    netlist.instances.get_mut(tps54331_instance).unwrap().name = "TPS54331".to_string();
    netlist.instances.get_mut(tps54331_instance).unwrap().attributes.insert(
        "input_voltage".to_string(), "5".to_string()
    );
    netlist.instances.get_mut(tps54331_instance).unwrap().attributes.insert(
        "output_voltage".to_string(), "3.3".to_string()
    );
    netlist.instances.get_mut(tps54331_instance).unwrap().attributes.insert(
        "load_current".to_string(), "0.8".to_string() // 800mA load
    );
    
    // Microcontroller: STM32F103C8T6 (3.3V logic levels)
    let mcu_instance = netlist.add_instance("MCU".to_string(), module_id).unwrap();
    netlist.instances.get_mut(mcu_instance).unwrap().name = "STM32F103C8T6".to_string();
    netlist.instances.get_mut(mcu_instance).unwrap().attributes.insert(
        "vdd_voltage".to_string(), "3.3".to_string()
    );
    netlist.instances.get_mut(mcu_instance).unwrap().attributes.insert(
        "current_consumption".to_string(), "0.05".to_string() // 50mA
    );
    
    // Precision resistors for voltage reference (temperature matching critical)
    let r1_instance = netlist.add_instance("R1".to_string(), module_id).unwrap();
    netlist.instances.get_mut(r1_instance).unwrap().name = "Res".to_string();
    netlist.instances.get_mut(r1_instance).unwrap().attributes.insert(
        "value".to_string(), "10000".to_string() // 10kΩ
    );
    netlist.instances.get_mut(r1_instance).unwrap().attributes.insert(
        "tolerance".to_string(), "0.001".to_string() // 0.1%
    );
    netlist.instances.get_mut(r1_instance).unwrap().attributes.insert(
        "temp_coefficient".to_string(), "25".to_string() // 25 ppm/°C
    );
    
    let r2_instance = netlist.add_instance("R2".to_string(), module_id).unwrap();
    netlist.instances.get_mut(r2_instance).unwrap().name = "Res".to_string();
    netlist.instances.get_mut(r2_instance).unwrap().attributes.insert(
        "value".to_string(), "10000".to_string() // 10kΩ
    );
    netlist.instances.get_mut(r2_instance).unwrap().attributes.insert(
        "tolerance".to_string(), "0.001".to_string() // 0.1%
    );
    netlist.instances.get_mut(r2_instance).unwrap().attributes.insert(
        "temp_coefficient".to_string(), "25".to_string() // 25 ppm/°C
    );
    
    // Current limiting resistor (potential thermal issue)
    let r_limit_instance = netlist.add_instance("R_limit".to_string(), module_id).unwrap();
    netlist.instances.get_mut(r_limit_instance).unwrap().name = "Res".to_string();
    netlist.instances.get_mut(r_limit_instance).unwrap().attributes.insert(
        "value".to_string(), "100".to_string() // 100Ω
    );
    netlist.instances.get_mut(r_limit_instance).unwrap().attributes.insert(
        "power_rating".to_string(), "0.25".to_string() // 1/4W
    );
    netlist.instances.get_mut(r_limit_instance).unwrap().attributes.insert(
        "expected_power".to_string(), "0.2".to_string() // 200mW expected
    );
    
    // Capacitors for power filtering
    let c_in_instance = netlist.add_instance("C_in".to_string(), module_id).unwrap();
    netlist.instances.get_mut(c_in_instance).unwrap().name = "Cap".to_string();
    netlist.instances.get_mut(c_in_instance).unwrap().attributes.insert(
        "value".to_string(), "100e-6".to_string() // 100µF
    );
    netlist.instances.get_mut(c_in_instance).unwrap().attributes.insert(
        "voltage_rating".to_string(), "35".to_string() // 35V
    );
    
    let c_out_instance = netlist.add_instance("C_out".to_string(), module_id).unwrap();
    netlist.instances.get_mut(c_out_instance).unwrap().name = "Cap".to_string();
    netlist.instances.get_mut(c_out_instance).unwrap().attributes.insert(
        "value".to_string(), "10e-6".to_string() // 10µF
    );
    netlist.instances.get_mut(c_out_instance).unwrap().attributes.insert(
        "voltage_rating".to_string(), "10".to_string() // 10V
    );
    
    // Add power nets to test power domain compatibility
    let vin_net = netlist.add_net(Some("VIN".to_string()));
    let vout_5v_net = netlist.add_net(Some("VOUT_5V".to_string()));
    let vout_3v3_net = netlist.add_net(Some("LOGIC_3V3".to_string()));
    let gnd_net = netlist.add_net(Some("GND".to_string()));
    let ref_net = netlist.add_net(Some("REF_2V5".to_string()));
    
    println!("Created netlist with {} instances and {} nets", 
             netlist.instances.len(), netlist.nets.len());
    
    // Initialize compatibility analyzer
    println!("\n=== Starting Component Compatibility Analysis ===\");
    let compatibility_analyzer = ComponentCompatibilityAnalyzer::new();
    
    // Run comprehensive compatibility analysis
    match compatibility_analyzer.analyze_compatibility(&netlist, &analysis) {
        Ok(report) => {
            println!("\nComponent Compatibility Analysis Report:");
            println!("======================================");
            
            // Overall compatibility score
            println!("\n🎯 Overall Compatibility Score: {:.1}%", report.overall_compatibility_score * 100.0);
            
            // Power domain analysis
            println!("\n⚡ Power Domain Compatibility Analysis:");
            if report.power_domain_analysis.is_empty() {
                println!("  No power domains analyzed");
            } else {
                for (i, domain) in report.power_domain_analysis.iter().enumerate() {
                    println!("  {}. Domain: {} ({:.1}V)", i + 1, domain.domain_name, domain.nominal_voltage);
                    println!("     Connected components: {}", domain.connected_components.len());
                    println!("     Max current capacity: {:.1}A", domain.max_current);
                    
                    if !domain.compatibility_issues.is_empty() {
                        println!("     Compatibility Issues:");
                        for issue in &domain.compatibility_issues {
                            let icon = match issue.issue_level {
                                bhdl_synthesizer::component_compatibility::CompatibilityIssue::Critical => "❌",
                                bhdl_synthesizer::component_compatibility::CompatibilityIssue::Warning => "⚠️",
                                bhdl_synthesizer::component_compatibility::CompatibilityIssue::Info => "ℹ️",
                                bhdl_synthesizer::component_compatibility::CompatibilityIssue::Suggestion => "💡",
                            };
                            println!("       {} {}: {}", icon, issue.title, issue.description);
                            println!("          Action: {}", issue.recommended_action);
                        }
                    } else {
                        println!("     ✅ No compatibility issues found");
                    }
                    
                    if !domain.power_sequencing_requirements.is_empty() {
                        println!("     Power Sequencing:");
                        for req in &domain.power_sequencing_requirements {
                            println!("       - {}", req);
                        }
                    }
                    println!();
                }
            }
            
            // Interface compatibility analysis
            println!("🔌 Interface Compatibility Analysis:");
            if report.interface_analysis.is_empty() {
                println!("  No digital interfaces detected for analysis");
            } else {
                for interface in &report.interface_analysis {
                    println!("  Interface: {}", interface.interface_type);
                    println!("    Logic levels: {:.2}V - {:.2}V", interface.voltage_levels.0, interface.voltage_levels.1);
                    println!("    Participants: {}", interface.participating_components.len());
                    
                    if !interface.timing_requirements.is_empty() {
                        println!("    Timing requirements:");
                        for (param, value) in &interface.timing_requirements {
                            println!("      {}: {:.1}ns", param, value);
                        }
                    }
                    
                    println!("    Compatibility matrix:");
                    for ((comp1, comp2), score) in &interface.compatibility_matrix {
                        println!("      Component pair compatibility: {:.1}%", score * 100.0);
                    }
                }
            }
            
            // Thermal compatibility analysis
            println!("\n🌡️  Thermal Compatibility Analysis:");
            if report.thermal_analysis.is_empty() {
                println!("  No thermal zones analyzed");
            } else {
                for (i, zone) in report.thermal_analysis.iter().enumerate() {
                    println!("  {}. Thermal Zone: {}", i + 1, zone.thermal_zone);
                    println!("     Total power dissipation: {:.2}W", zone.total_power_dissipation);
                    println!("     Max junction temperature: {:.1}°C", zone.max_junction_temp);
                    println!("     Ambient temperature: {:.1}°C", zone.ambient_temp);
                    
                    if !zone.cooling_requirements.is_empty() {
                        println!("     Cooling requirements:");
                        for req in &zone.cooling_requirements {
                            println!("       - {}", req);
                        }
                    }
                    
                    if !zone.hotspot_analysis.is_empty() {
                        println!("     Thermal hotspots:");
                        for (comp_id, temp, description) in &zone.hotspot_analysis {
                            println!("       - Component ID {}: {:.1}°C ({})", comp_id.0, temp, description);
                        }
                    }
                    println!();
                }
            }
            
            println!("Cross-component checks completed");
            println!("Found {} compatibility issues", report.cross_component_checks.len());
            println!("Critical issues: {}", report.critical_issues.len());
            println!("Optimization opportunities: {}", report.optimization_opportunities.len());
        },
        Err(e) => {
            println!("Compatibility analysis failed: {}", e);
        }
    }
    
    println!("Component compatibility analysis test completed successfully");
    
    Ok(())
}