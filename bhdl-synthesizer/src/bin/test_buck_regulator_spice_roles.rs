//! Test component role detection on a realistic buck regulator circuit

use std::error::Error;
use std::path::Path;

// BHDL pipeline imports
use bhdl_parser::parse;
use bhdl_analyzer::{analyze, types::AnalysisResult};
use bhdl_synthesizer::NetlistGenerator;
use bhdl_netlist::{Netlist, InstanceId};
use bhdl_ast::{SourceFile, AstNode};
use anyhow;
use std::collections::HashMap;

// SPICE imports
use bhdl_spice::circuit::{Circuit, ComponentId};
use bhdl_spice::extended_analysis::{ComponentRoleDetector, ComponentRole};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Component Role Analysis with Pin Metadata");
    println!("=========================================\n");
    
    // Path to the realistic regulator BHDL file - using 7805 for now
    // as the buck regulator file has parser issues with module flow syntax
    let bhdl_file = "tests/circuits/realistic/test_7805_regulator_realistic.bhdl";
    
    if !Path::new(bhdl_file).exists() {
        eprintln!("Error: BHDL file not found: {}", bhdl_file);
        return Err("File not found".into());
    }
    
    // Step 1: Parse BHDL file
    println!("Step 1: Parsing BHDL file...");
    let content = std::fs::read_to_string(bhdl_file)?;
    let parse_result = parse(&content);
    
    if !parse_result.errors().is_empty() {
        eprintln!("Parser errors:");
        for error in parse_result.errors() {
            eprintln!("  {}", error.message);
        }
        return Err("Parsing failed".into());
    }
    
    let syntax_node = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_node)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
        
    println!("✅ Parsing successful\n");
    
    // Step 2: Run analyzer
    println!("Step 2: Running semantic analysis...");
    let analysis_result = analyze(&source_file);
    
    if !analysis_result.diagnostics.is_empty() {
        println!("Analysis diagnostics:");
        for diag in &analysis_result.diagnostics {
            println!("  {}", diag.message);
        }
    }
    println!("✅ Analysis complete\n");
    
    // Step 3: Synthesize to netlist
    println!("Step 3: Synthesizing to netlist...");
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis_result).await?;
    
    println!("Netlist statistics:");
    println!("  Modules: {}", netlist.modules.len());
    println!("  Instances: {}", netlist.instances.len());
    println!("  Nets: {}", netlist.nets.len());
    
    
    println!("✅ Synthesis complete\n");
    
    // Step 4: Convert netlist to SPICE circuit
    println!("Step 4: Converting to SPICE circuit...");
    let (circuit, instance_to_component) = convert_netlist_to_circuit(&netlist, &analysis_result)?;
    
    println!("Circuit statistics:");
    println!("  Nodes: {}", circuit.nodes().count());
    println!("  Components: {}", circuit.branches().count());
    
    
    println!("✅ Conversion complete\n");
    
    // Step 5: Run component role detection with netlist information
    println!("Step 5: Detecting component roles...");
    let mut detector = ComponentRoleDetector::with_netlist(circuit, &netlist, instance_to_component);
    
    // Initialize simulation engine
    match detector.initialize_simulation() {
        Ok(()) => println!("✅ Simulation engine initialized"),
        Err(e) => {
            println!("⚠️  Simulation initialization failed: {}", e);
            println!("   Continuing with topology-only analysis");
        }
    }
    
    // Detect all component roles
    let roles = detector.detect_all_roles();
    
    
    // Step 6: Display results organized by role
    println!("\n📊 Component Role Analysis Results");
    println!("==================================\n");
    
    // Group components by role
    let mut role_groups: std::collections::HashMap<ComponentRole, Vec<(String, String)>> = 
        std::collections::HashMap::new();
    
    for (comp_id, role) in &roles {
        if let Some(component) = detector.circuit.get_component(*comp_id) {
            let name = component.name().to_string();
            let comp_type = component.component_type().to_string();
            let value = format_component_value(component.value, &comp_type);
            let info = format!("{} ({}, {})", name, comp_type, value);
            
            role_groups.entry(role.clone())
                .or_insert_with(Vec::new)
                .push((name, info));
        }
    }
    
    // Display by category
    println!("🔌 Input Stage:");
    display_role_group(&role_groups, &ComponentRole::InputProtection);
    display_role_group(&role_groups, &ComponentRole::InputFilter);
    display_role_group(&role_groups, &ComponentRole::EMIFiltering);
    
    println!("\n⚡ Power Stage:");
    display_role_group(&role_groups, &ComponentRole::PowerSwitch);
    display_role_group(&role_groups, &ComponentRole::PowerInductor);
    display_role_group(&role_groups, &ComponentRole::CatchDiode);
    display_role_group(&role_groups, &ComponentRole::Bootstrap);
    
    println!("\n📊 Control Stage:");
    display_role_group(&role_groups, &ComponentRole::FeedbackNetwork);
    display_role_group(&role_groups, &ComponentRole::Compensation);
    display_role_group(&role_groups, &ComponentRole::SoftStart);
    display_role_group(&role_groups, &ComponentRole::Sense);
    
    println!("\n🔋 Output Stage:");
    display_role_group(&role_groups, &ComponentRole::OutputStabilization);
    display_role_group(&role_groups, &ComponentRole::OutputProtection);
    display_role_group(&role_groups, &ComponentRole::Decoupling);
    display_role_group(&role_groups, &ComponentRole::Load);
    
    println!("\n❓ Unclassified:");
    display_role_group(&role_groups, &ComponentRole::Unknown);
    
    // Summary statistics
    println!("\n📈 Summary:");
    println!("  Total components analyzed: {}", roles.len());
    let identified = roles.iter().filter(|(_, r)| **r != ComponentRole::Unknown).count();
    let accuracy = (identified as f64 / roles.len() as f64) * 100.0;
    println!("  Successfully identified: {} ({:.1}%)", identified, accuracy);
    
    Ok(())
}

fn convert_netlist_to_circuit(netlist: &Netlist, analysis_result: &AnalysisResult) -> Result<(Circuit, HashMap<InstanceId, ComponentId>), Box<dyn Error>> {
    // Create a custom circuit with proper component values
    let mut circuit = Circuit::new();
    let mut instance_to_component = HashMap::new();
    
    // First, add all nets as nodes
    for (net_id, net) in &netlist.nets {
        let name = net.name.clone().unwrap_or_else(|| format!("net_{:?}", net_id));
        circuit.add_node(name, Some(net_id));
    }
    
    // Then add components with proper values from analysis
    for (instance_id, instance) in &netlist.instances {
        if let Some(module) = netlist.modules.get(instance.definition) {
            // Find connected nets using PinInstance connections
            let mut connected_nets = Vec::new();
            for (net_id, net) in &netlist.nets {
                for conn_point in &net.connections {
                    match conn_point {
                        bhdl_netlist::ConnectionPoint::PinInstance(pin_inst_id) => {
                            if let Some(pin_inst) = netlist.pin_instances.get(*pin_inst_id) {
                                if pin_inst.instance == instance_id {
                                    connected_nets.push(net_id);
                                    break;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            
            // For 2-pin components, create a branch
            if connected_nets.len() >= 2 {
                let node1 = netlist.nets.get(connected_nets[0])
                    .and_then(|n| n.name.clone())
                    .unwrap_or_else(|| format!("net_{:?}", connected_nets[0]));
                let node2 = netlist.nets.get(connected_nets[1])
                    .and_then(|n| n.name.clone())
                    .unwrap_or_else(|| format!("net_{:?}", connected_nets[1]));
                
                // Extract proper value from analysis results
                let value = extract_component_value(&instance.name, &module.name, &analysis_result);
                
                let comp_id = circuit.add_branch(
                    instance.name.clone(),
                    &node1,
                    &node2,
                    module.name.clone(),
                    value,
                    Some(instance_id),
                );
                
                instance_to_component.insert(instance_id, comp_id);
            }
        }
    }
    
    
    Ok((circuit, instance_to_component))
}

fn extract_component_value(instance_name: &str, component_type: &str, analysis_result: &AnalysisResult) -> f64 {
    // Look for the component in inferred components
    for component in &analysis_result.component_inference.inferred_components {
        if component.instance_name.as_deref() == Some(instance_name) {
            // Extract primary value parameter
            for param in &component.parameters {
                // Check for value parameter or empty name (primary value)
                if param.name.is_empty() || param.name == "value" {
                    match &param.value {
                        bhdl_analyzer::component_inference::ParameterValue::Real(v) => return *v,
                        bhdl_analyzer::component_inference::ParameterValue::Resistance(r) => return *r,
                        bhdl_analyzer::component_inference::ParameterValue::Capacitance(c) => return *c,
                        bhdl_analyzer::component_inference::ParameterValue::Inductance(l) => return *l,
                        bhdl_analyzer::component_inference::ParameterValue::Voltage(v) => return *v,
                        _ => {}
                    }
                }
            }
        }
    }
    
    // Fallback: parse from component type if it contains a value
    // For example: "330Ω" -> 330.0
    if let Some(value) = parse_value_from_type(component_type) {
        return value;
    }
    
    // Default fallback
    1.0
}

fn parse_value_from_type(component_type: &str) -> Option<f64> {
    // Simple parser for values in component types
    let numeric_part = component_type
        .chars()
        .take_while(|c| c.is_numeric() || *c == '.')
        .collect::<String>();
    
    numeric_part.parse::<f64>().ok()
}

fn format_component_value(value: f64, comp_type: &str) -> String {
    match comp_type {
        "Resistor" | "Res" => {
            if value >= 1e6 {
                format!("{:.1}MΩ", value / 1e6)
            } else if value >= 1e3 {
                format!("{:.1}kΩ", value / 1e3)
            } else if value < 1.0 {
                format!("{:.0}mΩ", value * 1000.0)
            } else {
                format!("{:.1}Ω", value)
            }
        },
        "Capacitor" | "Cap" => {
            if value >= 1e-3 {
                format!("{:.0}mF", value * 1e3)
            } else if value >= 1e-6 {
                format!("{:.1}µF", value * 1e6)
            } else if value >= 1e-9 {
                format!("{:.0}nF", value * 1e9)
            } else {
                format!("{:.0}pF", value * 1e12)
            }
        },
        "Inductor" | "Ferrite" => {
            if value >= 1e-3 {
                format!("{:.1}mH", value * 1e3)
            } else if value >= 1e-6 {
                format!("{:.1}µH", value * 1e6)
            } else {
                format!("{:.1}nH", value * 1e9)
            }
        },
        "Diode" | "SchottkyDiode" | "TVSDiode" => format!("{:.1}V", value),
        "VoltageSource" => format!("{:.1}V", value),
        _ => format!("{:.3}", value),
    }
}

fn display_role_group(
    role_groups: &std::collections::HashMap<ComponentRole, Vec<(String, String)>>,
    role: &ComponentRole,
) {
    if let Some(components) = role_groups.get(role) {
        if !components.is_empty() {
            println!("  {:?}:", role);
            let mut sorted = components.clone();
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            for (_, info) in sorted {
                println!("    - {}", info);
            }
        }
    }
}