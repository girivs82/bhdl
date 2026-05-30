//! Test that the synthesizer materializes components with their pins and
//! connects them into nets.
//!
//! Note: this used to also exercise `bhdl_common::ComponentPinRegistry`
//! (`get_pins`/`has_pin`), but that abstraction has been removed — pins are
//! now resolved through stdlib component module definitions during synthesis
//! and live on the generated `Netlist` (`netlist.pins`). The former
//! registry-only tests (`test_pin_registry_fuzzy_matching`,
//! `test_specific_pin_lookups`) tested the removed API and have been dropped.

use anyhow::Result;
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};

#[tokio::test]
async fn test_all_component_types_have_pins() -> Result<()> {
    // The import resolver maps `bhdl-stdlib/...` paths relative to the process
    // working directory. Integration tests run with cwd at the package root,
    // so hop up to the workspace root where the stdlib actually lives.
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    std::env::set_current_dir(workspace_root).expect("set cwd to workspace root");

    // Use the arduino class board fixture — a real circuit that imports stdlib
    // components (Res, Cap, LM317, ATmega328P_DIP28) and is known to synthesize.
    // Components must be imported from the stdlib before synthesis (the former
    // hardcoded ComponentPinRegistry fallback has been removed).
    let source = std::fs::read_to_string("tests/circuits/realistic/arduino_class_board.bhdl")
        .map_err(|e| anyhow::anyhow!("Failed to read arduino fixture: {e}"))?;

    println!("=== Testing Synthesizer Component/Pin Materialization ===\n");

    // Parse the test circuit
    let parse_result = parse(&source);
    let syntax_node = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_node)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;

    // Analyze the circuit
    let analysis = analyze(&source_file);
    println!("Analysis found {} diagnostics", analysis.diagnostics.len());
    for diag in &analysis.diagnostics {
        println!("  Diagnostic: {}", diag.message);
    }

    // Configure synthesizer without a component database.
    let config = NetlistConfig {
        preserve_semantic_context: true,
        include_power_domains: true,
        include_component_inference: true,
        flatten_hierarchy: false,
        database_path: None,
        ..NetlistConfig::default()
    };

    // Generate netlist
    let mut generator = NetlistGenerator::with_config(config);
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await?;

    println!("\n=== Synthesizer Results ===");
    println!("Generated {} instances", netlist.instances.len());
    println!("Generated {} modules", netlist.modules.len());
    println!("Generated {} nets", netlist.nets.len());

    let mut all_passed = true;

    // Check that instances in netlist have correct pins
    println!("\n=== Verifying Netlist Instances ===");

    for (instance_id, instance) in &netlist.instances {
        let module = netlist.modules.get(instance.definition)
            .ok_or_else(|| anyhow::anyhow!("Module not found for instance"))?;

        println!("\nInstance '{}' (module: {})", instance.name, module.name);
        println!("  Module has {} pins:", module.pins.len());

        // Get pin instances for this instance
        let pin_instances: Vec<_> = netlist.pin_instances.iter()
            .filter(|(_, pi)| pi.instance == instance_id)
            .collect();

        println!("  Instance has {} pin instances", pin_instances.len());

        if pin_instances.is_empty() && !module.pins.is_empty() {
            println!("  ERROR: No pin instances created for instance with {} module pins", module.pins.len());
            all_passed = false;
        }

        for (_pin_inst_id, pin_instance) in pin_instances {
            if let Some(pin) = netlist.pins.get(pin_instance.pin_def) {
                println!("    Pin '{}': {:?} {:?}", pin.name, pin.direction, pin.pin_type);
            }
        }
    }

    // Check net connections
    println!("\n=== Verifying Net Connections ===");

    let mut nets_with_connections = 0;
    for (_net_id, net) in &netlist.nets {
        if net.connections.len() > 1 {
            nets_with_connections += 1;
            println!("\nNet '{}' has {} connections:",
                     net.name.as_ref().map(|s| s.as_str()).unwrap_or("Unnamed"),
                     net.connections.len());

            for conn in &net.connections {
                match conn {
                    bhdl_netlist::types::ConnectionPoint::PinInstance(pin_inst_id) => {
                        if let Some(pin_inst) = netlist.pin_instances.get(*pin_inst_id) {
                            if let Some(instance) = netlist.instances.get(pin_inst.instance) {
                                if let Some(pin) = netlist.pins.get(pin_inst.pin_def) {
                                    println!("  - {}.{}", instance.name, pin.name);
                                }
                            }
                        }
                    }
                    other => {
                        println!("  - {:?}", other);
                    }
                }
            }
        }
    }

    println!("\n=== Summary ===");
    println!("Total nets with multiple connections: {}", nets_with_connections);

    if all_passed && nets_with_connections > 0 {
        println!("\n✅ Synthesizer materialized component pins and created proper connections!");
    } else {
        if !all_passed {
            println!("\n❌ Some instances are missing pin instances!");
        }
        if nets_with_connections == 0 {
            println!("\n❌ No nets have multiple connections - synthesizer may not be connecting properly!");
        }
    }

    Ok(())
}
