//! End-to-end test demonstrating pin metadata through the entire pipeline
//! Parse -> Analyze -> Synthesize -> SPICE role detection

use std::fs;
use std::collections::HashMap;
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::{Analyzer, AnalysisConfig};
use bhdl_synthesizer::Synthesizer;
use bhdl_spice::{Circuit, ComponentRoleDetector, NetlistToSpiceConverter};
use bhdl_spice::pin_metadata_integration::{extract_pin_metadata_from_analysis, update_pin_database_from_ast};

fn main() {
    println!("=== End-to-End Pin Metadata Test ===\n");
    
    // Step 1: Parse BHDL file
    println!("1. Parsing BHDL file...");
    let bhdl_source = fs::read_to_string("docs/examples/buck_converter_with_metadata.bhdl")
        .expect("Failed to read BHDL file");
    
    let parse_result = parse(&bhdl_source);
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  {}", error.message);
        }
        return;
    }
    
    let root = parse_result.syntax();
    let source_file = SourceFile::cast(root).expect("Expected SourceFile");
    println!("  ✓ Parsing successful");
    
    // Step 2: Analyze
    println!("\n2. Analyzing circuit...");
    let mut analyzer = Analyzer::new(AnalysisConfig::default());
    let analysis_result = analyzer.analyze_ast(&source_file);
    
    if !analysis_result.diagnostics.is_empty() {
        println!("Analysis diagnostics:");
        for diag in &analysis_result.diagnostics {
            println!("  {}: {}", diag.severity, diag.message);
        }
    }
    println!("  ✓ Analysis complete");
    
    // Print module definitions with pin metadata
    if let Some(modules) = &analysis_result.module_definitions {
        println!("\n  Module definitions found:");
        for (name, module_def) in modules {
            println!("    {}", name);
            if let Some(pins) = &module_def.pins {
                for (pin_name, pin_info) in pins {
                    if let Some(func) = pin_info.get("function") {
                        println!("      {} -> function: {}", pin_name, func);
                    }
                }
            }
        }
    }
    
    // Step 3: Synthesize netlist
    println!("\n3. Synthesizing netlist...");
    let mut synthesizer = Synthesizer::new();
    let synthesis_result = synthesizer.synthesize(&source_file, &analysis_result);
    
    match synthesis_result {
        Ok(netlist) => {
            println!("  ✓ Synthesis successful");
            println!("  Netlist stats:");
            println!("    Modules: {}", netlist.modules.len());
            println!("    Instances: {}", netlist.instances.len());
            println!("    Nets: {}", netlist.nets.len());
            
            // Step 4: Convert to SPICE circuit
            println!("\n4. Converting to SPICE circuit...");
            let converter = NetlistToSpiceConverter::new();
            
            match converter.convert_netlist(&netlist, &analysis_result.symbol_table, &HashMap::new()) {
                Ok((circuit, instance_mapping)) => {
                    println!("  ✓ SPICE circuit created");
                    println!("  Circuit components: {}", circuit.branches().count());
                    
                    // Step 5: Component role detection WITHOUT metadata
                    println!("\n5. Component role detection WITHOUT pin metadata:");
                    let detector_no_metadata = ComponentRoleDetector::with_netlist(
                        circuit.clone(),
                        &netlist,
                        instance_mapping.clone()
                    );
                    let roles_no_metadata = detector_no_metadata.detect_all_roles();
                    print_roles(&roles_no_metadata, &circuit);
                    
                    // Step 6: Component role detection WITH metadata
                    println!("\n6. Component role detection WITH pin metadata:");
                    let detector_with_metadata = ComponentRoleDetector::with_ast_metadata(
                        circuit.clone(),
                        &netlist,
                        instance_mapping.clone(),
                        &analysis_result
                    );
                    let roles_with_metadata = detector_with_metadata.detect_all_roles();
                    print_roles(&roles_with_metadata, &circuit);
                    
                    // Step 7: Show improvements
                    println!("\n7. Improvements from pin metadata:");
                    compare_results(&roles_no_metadata, &roles_with_metadata, &circuit);
                    
                    // Step 8: Generate SPICE netlist
                    println!("\n8. SPICE netlist output:");
                    print_spice_netlist(&circuit, &roles_with_metadata);
                },
                Err(e) => {
                    println!("  ✗ Failed to convert to SPICE: {}", e);
                }
            }
        },
        Err(e) => {
            println!("  ✗ Synthesis failed: {}", e);
        }
    }
}

fn print_roles(
    roles: &HashMap<bhdl_spice::ComponentId, bhdl_spice::ComponentRole>,
    circuit: &Circuit
) {
    for (comp_id, component) in circuit.branches() {
        if let Some(role) = roles.get(&comp_id) {
            println!("  {} ({}) -> {:?}", 
                component.name(), 
                component.component_type(),
                role
            );
        }
    }
}

fn compare_results(
    without: &HashMap<bhdl_spice::ComponentId, bhdl_spice::ComponentRole>,
    with: &HashMap<bhdl_spice::ComponentId, bhdl_spice::ComponentRole>,
    circuit: &Circuit
) {
    let mut improvements = 0;
    let mut changes = Vec::new();
    
    for (id, role_with) in with {
        if let Some(role_without) = without.get(id) {
            if role_with != role_without {
                improvements += 1;
                if let Some(component) = circuit.get_component(*id) {
                    changes.push((component.name(), role_without, role_with));
                }
            }
        }
    }
    
    println!("  Total improvements: {}", improvements);
    for (name, old_role, new_role) in changes {
        println!("  {} : {:?} -> {:?}", name, old_role, new_role);
    }
}

fn print_spice_netlist(circuit: &Circuit, roles: &HashMap<bhdl_spice::ComponentId, bhdl_spice::ComponentRole>) {
    println!("* Buck Converter with Component Roles");
    println!("* Generated from BHDL with pin metadata");
    println!();
    
    // Group components by role
    let mut components_by_role: HashMap<bhdl_spice::ComponentRole, Vec<(bhdl_spice::ComponentId, &bhdl_spice::Component)>> = HashMap::new();
    
    for (comp_id, component) in circuit.branches() {
        if let Some(role) = roles.get(&comp_id) {
            components_by_role
                .entry(role.clone())
                .or_insert_with(Vec::new)
                .push((comp_id, component));
        }
    }
    
    // Print components grouped by role
    for (role, components) in components_by_role {
        println!("* {} components:", format!("{:?}", role));
        for (comp_id, component) in components {
            let nodes: Vec<String> = component.nodes()
                .iter()
                .map(|&node_id| {
                    circuit.get_node_by_id(node_id)
                        .map(|n| n.name.clone())
                        .unwrap_or_else(|| format!("n{}", node_id.0))
                })
                .collect();
                
            match component.component_type() {
                "Resistor" | "Res" => {
                    println!("{} {} {} {:.0}", 
                        component.name(),
                        nodes.join(" "),
                        component.value
                    );
                },
                "Capacitor" | "Cap" => {
                    println!("{} {} {} {:.2e}",
                        component.name(),
                        nodes.join(" "),
                        component.value
                    );
                },
                "Inductor" => {
                    println!("{} {} {} {:.2e}",
                        component.name(),
                        nodes.join(" "),
                        component.value
                    );
                },
                _ => {
                    println!("{} {} {} {}",
                        component.name(),
                        nodes.join(" "),
                        component.component_type(),
                        component.value
                    );
                }
            }
        }
        println!();
    }
}