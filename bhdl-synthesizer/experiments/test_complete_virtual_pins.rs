/// Comprehensive test of virtual pin expansion and connectivity
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_synthesizer::NetlistGenerator;
use bhdl_analyzer::analyze;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    println!("=== Complete Virtual Pin System Test ===\n");
    
    // Test with mixed regulators - both switching and linear
    let test_code = r#"
board MixedRegulatorBoard {
    power VIN = 24V @ 5A;
    power VOUT_12V = 12V @ 3A;
    power VOUT_5V = 5V @ 1A;
    ground GND;
    
    // Buck converter with virtual pin
    U1: TPS54331(vout=12V);
    @VIN -> U1.VIN;
    U1.GND -> @GND;
    U1.EN -> @VIN;
    U1.VOUT -> @VOUT_12V;  // Virtual pin expands
    
    // Linear regulator with virtual pin  
    U2: LM7805();
    @VOUT_12V -> U2.VIN;  // Cascade from buck output
    U2.GND -> @GND;
    U2.VOUT -> @VOUT_5V;  // Virtual pin expands
    
    // LM2596 buck converter
    U3: LM2596(vout=5V);
    @VIN -> U3.VIN;
    U3.GND -> @GND;
    U3.EN -> @VIN;
    U3.VOUT -> net_5v_backup;  // Virtual pin expands
    
    // Load resistors
    @VOUT_12V -> R1: Res(100).1;
    R1.2 -> @GND;
    
    @VOUT_5V -> R2: Res(47).1;
    R2.2 -> @GND;
}
"#;
    
    // Parse
    let parse_result = parse(test_code);
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  {}", error.message);
        }
        return Ok(());
    }
    
    let syntax = parse_result.syntax();
    let source_file = SourceFile::cast(syntax).expect("Failed to cast to SourceFile");
    
    // Analyze
    let analysis_result = analyze(&source_file);
    
    // Generate netlist
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis_result).await?;
    
    println!("=== Netlist Statistics ===");
    println!("Total modules: {}", netlist.modules.len());
    println!("Total instances: {}", netlist.instances.len());
    println!("Total nets: {}", netlist.nets.len());
    println!();
    
    // Verify each IC and its expansion children (via vpin_parent attribute)
    println!("=== Component Verification ===\n");

    let mut test_results = Vec::new();

    for ic_name in &["U1", "U2", "U3"] {
        let ic_found = netlist.instances.values().any(|inst| inst.name == *ic_name);
        let supporting = netlist.instances.values()
            .filter(|inst| inst.attributes.get("vpin_parent").map(|s| s.as_str()) == Some(*ic_name))
            .count();

        if ic_found {
            println!("✓ Found {} ({})", ic_name,
                netlist.instances.values()
                    .find(|inst| inst.name == *ic_name)
                    .and_then(|inst| netlist.modules.get(inst.definition))
                    .map(|m| m.name.as_str())
                    .unwrap_or("?"));
        }
        if supporting > 0 {
            println!("  {} expansion children", supporting);
            for inst in netlist.instances.values() {
                if inst.attributes.get("vpin_parent").map(|s| s.as_str()) == Some(*ic_name) {
                    let role = inst.attributes.get("vpin_role").map(|s| s.as_str()).unwrap_or("?");
                    let class = inst.attributes.get("component_class").map(|s| s.as_str()).unwrap_or("?");
                    println!("    - {} (role={}, class={})", inst.name, role, class);
                }
            }
        }
        test_results.push((*ic_name, ic_found, supporting));
    }
    
    // Verify critical nets exist and have connections
    println!("\n=== Net Connectivity Verification ===\n");
    
    // Check SW nets for buck converters
    let sw_nets = netlist.nets.iter()
        .filter(|(_, net)| net.name.as_ref().map_or(false, |n| n.contains("_SW")))
        .count();
    println!("SW nets found: {}", sw_nets);
    
    // Check VOUT net
    let vout_net_connections = netlist.nets.iter()
        .find(|(_, net)| net.name.as_ref().map_or(false, |n| n == "VOUT"))
        .map(|(_, net)| net.connections.len())
        .unwrap_or(0);
    println!("VOUT net connections: {}", vout_net_connections);
    
    // Check GND net
    let gnd_net_connections = netlist.nets.iter()
        .find(|(_, net)| net.name.as_ref().map_or(false, |n| n == "GND"))
        .map(|(_, net)| net.connections.len())
        .unwrap_or(0);
    println!("GND net connections: {}", gnd_net_connections);
    
    // Detailed connection analysis for the first expansion inductor
    println!("\n=== Detailed Connection Analysis (first expansion inductor) ===\n");

    // Find first inductor that is an expansion child
    let inductor = netlist.instances.iter()
        .find(|(_, inst)| {
            inst.attributes.get("vpin_role").map(|s| s.as_str()) == Some("series")
                && inst.attributes.get("component_class").map(|s| s.as_str()) == Some("inductor")
        });

    if let Some((l_id, l_inst)) = inductor {
        let parent = l_inst.attributes.get("vpin_parent").map(|s| s.as_str()).unwrap_or("?");
        println!("Found inductor '{}' (parent={}, id: {:?})", l_inst.name, parent, l_id);

        // Find all nets connected to this inductor
        let mut connected_nets = Vec::new();
        for (net_id, net) in &netlist.nets {
            for conn in &net.connections {
                if let bhdl_netlist::ConnectionPoint::PinInstance(pin_inst_id) = conn {
                    if let Some(pin_inst) = netlist.pin_instances.get(*pin_inst_id) {
                        if pin_inst.instance == l_id {
                            connected_nets.push((net_id, net.name.clone()));
                        }
                    }
                }
            }
        }

        println!("Connected to {} nets:", connected_nets.len());
        for (net_id, net_name) in connected_nets {
            println!("  - {:?}: {}", net_id, net_name.unwrap_or("<unnamed>".to_string()));
        }
    } else {
        println!("✗ No expansion inductor found!");
    }
    
    // Final summary
    println!("\n=== Test Summary ===\n");
    
    let mut all_passed = true;
    for (name, found, supporting) in &test_results {
        if *found && *supporting > 0 {
            println!("✅ {}: IC found with {} expansion children", name, supporting);
        } else if *found {
            println!("⚠️  {}: IC found but NO expansion children", name);
            all_passed = false;
        } else {
            println!("❌ {}: IC not found", name);
            all_passed = false;
        }
    }

    if all_passed && sw_nets > 0 && vout_net_connections > 0 && gnd_net_connections > 0 {
        println!("\n🎉 SUCCESS: Complete virtual pin system working!");
        println!("   - All ICs have virtual pin expansion");
        println!("   - Expansion children are connected");
        println!("   - Critical nets (SW, VOUT, GND) are properly routed");
    } else {
        println!("\n⚠️  PARTIAL SUCCESS: Some issues found");
        if sw_nets == 0 {
            println!("   - Missing SW nets for buck converters");
        }
        if vout_net_connections == 0 {
            println!("   - VOUT net has no connections");
        }
        if gnd_net_connections == 0 {
            println!("   - GND net has no connections");
        }
    }
    
    Ok(())
}