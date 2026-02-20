#!/usr/bin/env rust-script
//! Debug tool to understand instance ID mismatch

use anyhow::Result;
use bhdl_parser::parse;
use bhdl_analyzer::analyze;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_synthesizer::NetlistGenerator;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let test_file = "tests/circuits/simple/test_intent_simple_demo.bhdl";

    println!("\n=== INSTANCE ID DEBUG TOOL ===\n");

    // Parse and analyze
    let content = std::fs::read_to_string(test_file)?;
    let parse_result = parse(&content);
    let source_file = SourceFile::cast(parse_result.syntax()).unwrap();
    let analysis = analyze(&source_file);

    // Generate netlist
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await?;
    let components = generator.get_component_instances();

    println!("📊 NETLIST INSTANCES:");
    for (instance_id, instance) in &netlist.instances {
        println!("  {:?}: {} (definition: {:?})",
            instance_id, instance.name, instance.definition);
    }

    println!("\n📍 PIN INSTANCES:");
    for (pin_inst_id, pin_inst) in &netlist.pin_instances {
        let pin_def = netlist.get_pin(pin_inst.pin_def);
        let instance = netlist.get_instance(pin_inst.instance);

        println!("  {:?}:", pin_inst_id);
        println!("    instance: {:?} ({})",
                 pin_inst.instance,
                 instance.map(|i| i.name.as_str()).unwrap_or("NOT FOUND"));
        println!("    pin_def: {:?} ({})",
                 pin_inst.pin_def,
                 pin_def.map(|p| p.name.as_str()).unwrap_or("NOT FOUND"));
        println!("    net: {:?}", pin_inst.net);
    }

    println!("\n💾 DATABASE COMPONENTS:");
    for comp in components {
        println!("  {}", comp.instance_name);
    }

    println!("\n🔌 NETS WITH PIN INSTANCE CONNECTIONS:");
    for (net_id, net) in &netlist.nets {
        if net.connections.is_empty() {
            continue;
        }

        println!("\n  {:?}: {:?}", net_id, net.name);
        for conn in &net.connections {
            match conn {
                bhdl_netlist::ConnectionPoint::PinInstance(pin_inst_id) => {
                    if let Some(pin_inst) = netlist.get_pin_instance(*pin_inst_id) {
                        if let Some(instance) = netlist.get_instance(pin_inst.instance) {
                            if let Some(pin) = netlist.get_pin(pin_inst.pin_def) {
                                println!("    PinInstance({:?}): instance={:?} ({}) pin={:?} ({})",
                                         pin_inst_id,
                                         pin_inst.instance,
                                         instance.name,
                                         pin_inst.pin_def,
                                         pin.name);
                            }
                        }
                    }
                }
                _ => println!("    Other: {:?}", conn),
            }
        }
    }

    Ok(())
}
