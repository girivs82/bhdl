//! Test component role detection on a realistic buck regulator circuit

use std::error::Error;
use std::path::Path;

// BHDL pipeline imports
use bhdl_parser::Parser;
use bhdl_analyzer::Analyzer;
use bhdl_synthesizer::Synthesizer;
use bhdl_netlist::Netlist;

// SPICE imports
use bhdl_spice::circuit::Circuit;
use bhdl_spice::extended_analysis::{ComponentRoleDetector, ComponentRole};

fn main() -> Result<(), Box<dyn Error>> {
    println!("Buck Regulator Component Role Analysis");
    println!("=====================================\n");
    
    // Path to the buck regulator BHDL file
    let bhdl_file = "tests/circuits/realistic/buck_regulator_12v_to_5v.bhdl";
    
    if !Path::new(bhdl_file).exists() {
        eprintln!("Error: BHDL file not found: {}", bhdl_file);
        return Err("File not found".into());
    }
    
    // Step 1: Parse BHDL file
    println!("Step 1: Parsing BHDL file...");
    let content = std::fs::read_to_string(bhdl_file)?;
    let mut parser = Parser::new(&content);
    let source_file = parser.parse();
    
    if !parser.errors().is_empty() {
        eprintln!("Parser errors:");
        for error in parser.errors() {
            eprintln!("  {}", error);
        }
        return Err("Parsing failed".into());
    }
    println!("✅ Parsing successful\n");
    
    // Step 2: Run analyzer
    println!("Step 2: Running semantic analysis...");
    let mut analyzer = Analyzer::new();
    let analysis_result = analyzer.analyze(&source_file);
    
    if !analysis_result.diagnostics.is_empty() {
        println!("Analysis diagnostics:");
        for diag in &analysis_result.diagnostics {
            println!("  {}: {}", diag.severity, diag.message);
        }
    }
    println!("✅ Analysis complete\n");
    
    // Step 3: Synthesize to netlist
    println!("Step 3: Synthesizing to netlist...");
    let mut synthesizer = Synthesizer::new();
    let netlist = synthesizer.synthesize(&source_file, &analysis_result)?;
    
    println!("Netlist statistics:");
    println!("  Modules: {}", netlist.modules.len());
    println!("  Instances: {}", netlist.top_module().instances.len());
    println!("  Nets: {}", netlist.top_module().nets.len());
    println!("✅ Synthesis complete\n");
    
    // Step 4: Convert netlist to SPICE circuit
    println!("Step 4: Converting to SPICE circuit...");
    let circuit = convert_netlist_to_circuit(&netlist)?;
    
    println!("Circuit statistics:");
    println!("  Nodes: {}", circuit.nodes().count());
    println!("  Components: {}", circuit.branches().count());
    println!("✅ Conversion complete\n");
    
    // Step 5: Run component role detection
    println!("Step 5: Detecting component roles...");
    let mut detector = ComponentRoleDetector::new(circuit);
    
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

fn convert_netlist_to_circuit(netlist: &Netlist) -> Result<Circuit, Box<dyn Error>> {
    let mut circuit = Circuit::new();
    
    // Get the top module
    let top_module = netlist.top_module();
    
    // Add all nets as nodes
    for (net_id, net) in &top_module.nets {
        let node_name = net.name.clone().unwrap_or_else(|| format!("net_{}", net_id.0));
        circuit.add_node(node_name, Some(*net_id));
    }
    
    // Add all instances as components
    for (inst_id, instance) in &top_module.instances {
        let comp_name = instance.name.clone();
        
        // Map instance connections to nodes
        if instance.connections.len() >= 2 {
            let node1 = instance.connections[0].net
                .and_then(|net_id| top_module.nets.get(&net_id))
                .and_then(|net| net.name.clone())
                .unwrap_or_else(|| "unknown".to_string());
                
            let node2 = instance.connections.get(1)
                .and_then(|conn| conn.net)
                .and_then(|net_id| top_module.nets.get(&net_id))
                .and_then(|net| net.name.clone())
                .unwrap_or_else(|| "unknown".to_string());
            
            // Determine component type and value
            let (comp_type, value) = extract_component_info(&instance.component);
            
            circuit.add_branch(
                comp_name,
                &node1,
                &node2,
                comp_type,
                value,
                Some(*inst_id),
            );
        }
    }
    
    Ok(circuit)
}

fn extract_component_info(component: &bhdl_netlist::Component) -> (String, f64) {
    match component {
        bhdl_netlist::Component::Resistor(r) => ("Resistor".to_string(), r.resistance),
        bhdl_netlist::Component::Capacitor(c) => ("Capacitor".to_string(), c.capacitance),
        bhdl_netlist::Component::Inductor(i) => ("Inductor".to_string(), i.inductance),
        bhdl_netlist::Component::Diode(d) => {
            let diode_type = if d.model.contains("Schottky") { "SchottkyDiode" } else { "Diode" };
            (diode_type.to_string(), d.forward_voltage)
        },
        bhdl_netlist::Component::LED(l) => ("LED".to_string(), l.forward_voltage),
        bhdl_netlist::Component::VoltageSource(v) => ("VoltageSource".to_string(), v.voltage),
        bhdl_netlist::Component::Module(m) => {
            // Map module types to component types
            let comp_type = match m.module_name.as_str() {
                "TPS54360" => "BuckController",
                "TVSDiode" => "TVSDiode",
                "Fuse" => "Fuse",
                "Ferrite" => "Inductor",
                _ => "Module",
            };
            (comp_type.to_string(), 0.0)
        },
        _ => ("Unknown".to_string(), 0.0),
    }
}

fn format_component_value(value: f64, comp_type: &str) -> String {
    match comp_type {
        "Resistor" => {
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
        "Capacitor" => {
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