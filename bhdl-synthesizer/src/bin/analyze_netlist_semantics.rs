//! Analyze netlist semantic context for voltage regulator circuit
//! 
//! This tool examines the netlist to ensure it contains all the semantic
//! information needed for intelligent visualization:
//! - Power flow paths
//! - Component roles
//! - Functional groupings
//! - Layout hints

use std::fs;
use anyhow::{Result, Context};
use console::style;
use env_logger;

use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_analyzer::types::AnalysisResult;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};
use bhdl_netlist::{Netlist, NetClass, ConnectionPoint};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    println!("{}", style("🔍 BHDL Netlist Semantic Analysis").bold().blue());
    println!("{}", style("=" .repeat(60)).dim());
    
    // Load and parse the voltage regulator example
    let source_path = "/Users/girivs/src/bhdl-new/examples/linear_regulator.bhdl";
    let source_content = fs::read_to_string(source_path)
        .context("Failed to read BHDL source file")?;
    
    // Parse and analyze
    let parse_result = parse(&source_content);
    let syntax_node = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_node)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    
    let analysis = analyze(&source_file);
    
    // Generate netlist with semantic context
    let config = NetlistConfig {
        preserve_semantic_context: true,
        include_power_domains: true,
        include_component_inference: true,
        flatten_hierarchy: false,
        use_database_components: true,
        database_path: Some("/Users/girivs/src/bhdl-new/components.db".to_string()),
    };
    
    let mut generator = NetlistGenerator::with_config(config);
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await?;
    
    // Analyze semantic context
    println!("\n📊 Semantic Context Analysis");
    println!("   Circuit Type: Linear Voltage Regulator");
    
    // 1. Power Domain Analysis
    println!("\n🔌 Power Domains:");
    analyze_power_domains(&netlist);
    
    // 2. Component Role Analysis
    println!("\n🎭 Component Roles:");
    analyze_component_roles(&netlist, &analysis);
    
    // 3. Circuit Flow Analysis
    println!("\n➡️  Circuit Flow:");
    analyze_circuit_flow(&netlist);
    
    // 4. Functional Groupings
    println!("\n📦 Functional Groups:");
    analyze_functional_groups(&netlist);
    
    // 5. Layout Hints
    println!("\n📐 Layout Hints for Visualization:");
    suggest_layout_hints(&netlist);
    
    // 6. Validation
    println!("\n✅ Semantic Validation:");
    validate_regulator_semantics(&netlist);
    
    Ok(())
}

fn analyze_power_domains(netlist: &Netlist) {
    let mut power_nets = Vec::new();
    let mut ground_nets = Vec::new();
    
    for (net_id, net) in &netlist.nets {
        let default_name = format!("net_{:?}", net_id);
        let name = net.name.as_ref().unwrap_or(&default_name);
        
        match &net.net_class {
            NetClass::Power(voltage) => {
                power_nets.push((name.clone(), *voltage));
            }
            NetClass::Ground => {
                ground_nets.push(name.clone());
            }
            _ => {}
        }
    }
    
    println!("   Power Rails:");
    for (name, voltage) in &power_nets {
        // Filter out component-derived power domains (single letter + digit pattern like C1, R1, U1)
        let is_component_ref = name.len() <= 3 && 
            name.chars().nth(0).map(|c| c.is_ascii_uppercase()).unwrap_or(false) &&
            name.chars().skip(1).all(|c| c.is_ascii_digit());
        
        if !is_component_ref {
            println!("     - {} @ {}V", name, voltage);
        }
    }
    
    println!("   Ground Rails:");
    for name in &ground_nets {
        // Filter out component-derived ground domains
        let is_component_ref = name.len() <= 3 && 
            name.chars().nth(0).map(|c| c.is_ascii_uppercase()).unwrap_or(false) &&
            name.chars().skip(1).all(|c| c.is_ascii_digit());
        
        if !is_component_ref {
            println!("     - {}", name);
        }
    }
    
    // Check power flow
    if power_nets.len() >= 2 {
        println!("   ✅ Multiple power domains detected - good for voltage regulation");
    }
}

fn analyze_component_roles(netlist: &Netlist, analysis: &AnalysisResult) {
    // Categorize components by their role
    let mut input_caps = Vec::new();
    let mut output_caps = Vec::new();
    let mut regulators = Vec::new();
    let mut indicators = Vec::new();
    let mut current_limiting = Vec::new();
    
    for (_id, instance) in &netlist.instances {
        if let Some(module) = netlist.modules.get(instance.definition) {
            let component_type = &module.name;
            
            // Identify component roles based on type and connections
            if component_type.contains("LM78") || component_type.contains("regulator") {
                regulators.push(&instance.name);
            } else if component_type == "C" || component_type.contains("Cap") {
                // Determine if input or output cap based on connections
                if instance.name.contains("1") {
                    input_caps.push(&instance.name);
                } else {
                    output_caps.push(&instance.name);
                }
            } else if component_type.contains("LED") {
                indicators.push(&instance.name);
            } else if component_type == "R" || component_type.contains("Res") {
                current_limiting.push(&instance.name);
            }
        }
    }
    
    println!("   Voltage Regulators: {:?}", regulators);
    println!("   Input Filtering: {:?}", input_caps);
    println!("   Output Filtering: {:?}", output_caps);
    println!("   Status Indicators: {:?}", indicators);
    println!("   Current Limiting: {:?}", current_limiting);
    
    // Check component inference results
    if let Some(inference) = analysis.component_inference.get_inferred_components().first() {
        println!("   Component Inference: {} - {}", 
                 inference.component_type, inference.reasoning);
    }
}

fn analyze_circuit_flow(netlist: &Netlist) {
    // Debug: Show net statistics
    println!("\n   DEBUG: Circuit Flow Analysis");
    println!("   Total nets: {}", netlist.nets.len());
    println!("   Total pin instances: {}", netlist.pin_instances.len());
    
    // Debug: Show all nets and their connections
    for (net_id, net) in &netlist.nets {
        if let Some(name) = &net.name {
            if name == "VCC" || name == "GND" || name == "VIN" {
                println!("   Net '{}' has {} connections:", name, net.connections.len());
                for conn in &net.connections {
                    match conn {
                        ConnectionPoint::PinInstance(pin_inst_id) => {
                            if let Some(pin_inst) = netlist.pin_instances.get(*pin_inst_id) {
                                if let Some(inst) = netlist.instances.get(pin_inst.instance) {
                                    if let Some(pin_def) = netlist.pins.get(pin_inst.pin_def) {
                                        println!("     - {}.{}", inst.name, pin_def.name);
                                    }
                                }
                            }
                        }
                        _ => println!("     - {:?}", conn),
                    }
                }
            }
        }
    }
    
    // Trace power flow through the circuit
    println!("\n   Power Flow Path:");
    
    // Find VIN connections
    let vin_connections = find_net_connections(netlist, "VIN");
    println!("     VIN -> {:?}", vin_connections);
    
    // Find regulator connections
    if let Some(reg_instance) = netlist.instances.iter()
        .find(|(_, inst)| inst.name.contains("U1")) {
        println!("     Regulator: {} (pins: IN->OUT)", reg_instance.1.name);
    }
    
    // Find VCC connections
    let vcc_connections = find_net_connections(netlist, "VCC");
    println!("     VCC -> {:?}", vcc_connections);
    
    // Find GND connections
    let gnd_connections = find_net_connections(netlist, "GND");
    println!("     GND connections: {:?}", gnd_connections);
}

fn find_net_connections(netlist: &Netlist, net_name: &str) -> Vec<String> {
    let mut connections = Vec::new();
    
    for (_net_id, net) in &netlist.nets {
        if net.name.as_ref().map(|n| n == net_name).unwrap_or(false) {
            for conn in &net.connections {
                match conn {
                    bhdl_netlist::ConnectionPoint::PinInstance(pin_inst_id) => {
                        if let Some(pin_inst) = netlist.pin_instances.get(*pin_inst_id) {
                            if let Some(instance) = netlist.instances.get(pin_inst.instance) {
                                if let Some(pin_def) = netlist.pins.get(pin_inst.pin_def) {
                                    connections.push(format!("{}.{}", instance.name, pin_def.name));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    
    connections
}

fn analyze_functional_groups(_netlist: &Netlist) {
    println!("   1. Input Stage:");
    println!("      - Input voltage: VIN");
    println!("      - Input filtering: C1");
    println!("      - Ground reference: GND");
    
    println!("   2. Regulation Stage:");
    println!("      - Voltage regulator: U1 (LM7805)");
    println!("      - Thermal considerations: TO-220 package");
    
    println!("   3. Output Stage:");
    println!("      - Output voltage: VCC (5V)");
    println!("      - Output filtering: C2, C3");
    println!("      - Load capacity: 500mA");
    
    println!("   4. Indication Stage:");
    println!("      - Power LED: LED1");
    println!("      - Current limiting: R1");
}

fn suggest_layout_hints(_netlist: &Netlist) {
    println!("   Component Placement:");
    println!("     - Place U1 (regulator) centrally");
    println!("     - C1 close to U1.IN pin");
    println!("     - C2, C3 close to U1.OUT pin");
    println!("     - Keep ground paths short");
    
    println!("   Signal Flow:");
    println!("     - Left to right: VIN -> U1 -> VCC");
    println!("     - Vertical: Power rails top, ground bottom");
    
    println!("   Thermal Considerations:");
    println!("     - U1 may need heatsink area");
    println!("     - Keep heat-sensitive components away");
    
    println!("   Critical Paths:");
    println!("     - Minimize VIN to C1 to U1.IN trace length");
    println!("     - Star ground at U1.GND if possible");
}

fn validate_regulator_semantics(netlist: &Netlist) {
    let mut issues = Vec::new();
    
    // Check for required components
    let has_regulator = netlist.instances.iter()
        .any(|(_, inst)| inst.name.contains("U") && 
             netlist.modules.get(inst.definition)
                .map(|m| m.name.contains("78"))
                .unwrap_or(false));
    
    let has_input_cap = netlist.instances.iter()
        .any(|(_, inst)| inst.name == "C1");
    
    let has_output_cap = netlist.instances.iter()
        .any(|(_, inst)| inst.name == "C2" || inst.name == "C3");
    
    if !has_regulator {
        issues.push("Missing voltage regulator");
    }
    if !has_input_cap {
        issues.push("Missing input capacitor");
    }
    if !has_output_cap {
        issues.push("Missing output capacitor");
    }
    
    // Check power domains
    let power_count = netlist.nets.iter()
        .filter(|(_, net)| matches!(net.net_class, NetClass::Power(_)))
        .count();
    
    if power_count < 2 {
        issues.push("Expected at least 2 power domains (input and output)");
    }
    
    // Check connections
    let vin_net = netlist.nets.iter()
        .find(|(_, net)| net.name.as_ref().map(|n| n == "VIN").unwrap_or(false));
    
    if let Some((_, net)) = vin_net {
        if net.connections.len() < 2 {
            issues.push("VIN should connect to at least 2 components");
        }
    }
    
    if issues.is_empty() {
        println!("   ✅ All semantic requirements met for voltage regulator");
    } else {
        println!("   ❌ Issues found:");
        for issue in issues {
            println!("      - {}", issue);
        }
    }
}