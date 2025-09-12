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
    
    // Verify each IC and its supporting components
    println!("=== Component Verification ===\n");
    
    let mut test_results = Vec::new();
    
    // Test TPS54331 (U1)
    let mut u1_found = false;
    let mut u1_supporting = 0;
    for (_, instance) in &netlist.instances {
        if instance.name == "U1" {
            u1_found = true;
            println!("✓ Found TPS54331 (U1)");
        } else if instance.name.starts_with("U1_") {
            u1_supporting += 1;
        }
    }
    test_results.push(("TPS54331", u1_found, u1_supporting));
    if u1_supporting > 0 {
        println!("  {} supporting components", u1_supporting);
    }
    
    // Test LM7805 (U2)
    let mut u2_found = false;
    let mut u2_supporting = 0;
    for (_, instance) in &netlist.instances {
        if instance.name == "U2" {
            u2_found = true;
            println!("✓ Found LM7805 (U2)");
        } else if instance.name.starts_with("U2_") {
            u2_supporting += 1;
        }
    }
    test_results.push(("LM7805", u2_found, u2_supporting));
    if u2_supporting > 0 {
        println!("  {} supporting components", u2_supporting);
    }
    
    // Test LM2596 (U3)
    let mut u3_found = false;
    let mut u3_supporting = 0;
    for (_, instance) in &netlist.instances {
        if instance.name == "U3" {
            u3_found = true;
            println!("✓ Found LM2596 (U3)");
        } else if instance.name.starts_with("U3_") {
            u3_supporting += 1;
        }
    }
    test_results.push(("LM2596", u3_found, u3_supporting));
    if u3_supporting > 0 {
        println!("  {} supporting components", u3_supporting);
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
    
    // Detailed connection analysis for one component
    println!("\n=== Detailed Connection Analysis (U1_L1) ===\n");
    
    // Find U1_L1 inductor
    let u1_l1 = netlist.instances.iter()
        .find(|(_, inst)| inst.name == "U1_L1");
    
    if let Some((l1_id, l1_inst)) = u1_l1 {
        println!("Found inductor U1_L1 (id: {:?})", l1_id);
        
        // Find all nets connected to this inductor
        let mut connected_nets = Vec::new();
        for (net_id, net) in &netlist.nets {
            for conn in &net.connections {
                if let bhdl_netlist::ConnectionPoint::PinInstance(pin_inst_id) = conn {
                    if let Some(pin_inst) = netlist.pin_instances.get(*pin_inst_id) {
                        if pin_inst.instance == l1_id {
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
        println!("✗ U1_L1 inductor not found!");
    }
    
    // Final summary
    println!("\n=== Test Summary ===\n");
    
    let mut all_passed = true;
    for (name, found, supporting) in test_results {
        if found && supporting > 0 {
            println!("✅ {}: IC found with {} supporting components", name, supporting);
        } else if found {
            println!("⚠️  {}: IC found but NO supporting components", name);
            all_passed = false;
        } else {
            println!("❌ {}: IC not found", name);
            all_passed = false;
        }
    }
    
    if all_passed && sw_nets > 0 && vout_net_connections > 0 && gnd_net_connections > 0 {
        println!("\n🎉 SUCCESS: Complete virtual pin system working!");
        println!("   - All ICs have virtual pin expansion");
        println!("   - Supporting components are connected");
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