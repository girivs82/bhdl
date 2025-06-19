use anyhow::Result;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};
use bhdl_parser;
use bhdl_ast::AstNode;
use bhdl_analyzer::analyze;
use log::info;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();

    info!("Testing net naming (@NETNAME) syntax in synthesizer");

    // Test BHDL code with new net naming syntax
    let test_code = r#"
board TestNetNaming {
    power VIN = 12V @ 1A;
    power VCC = 5V @ 1A;
    ground GND;
    
    // Test @NETNAME-> syntax for creating named nets
    VIN @RAW-> fuse: Fuse(1A).1;
    fuse.2 @PROTECTED-> tvs: TVSDiode(15V).1;
    tvs.2 -> GND;
    
    // Reference the named net
    @PROTECTED -> bulk_cap: ElectrolyticCap(100µF, 25V).+;
    @PROTECTED -> ceramic_cap: Cap(0.1µF).1;
    bulk_cap.- -> GND;
    ceramic_cap.2 -> GND;
    
    // Regular connection for comparison
    @PROTECTED -> reg: LM7805().IN;
    reg.OUT @5V-> c_out: Cap(100µF).+;
    reg.GND -> GND;
    c_out.- -> GND;
    
    // Multiple references to @5V net
    @5V -> r_led: Res(330Ω).1;
    r_led.2 -> led: LED(green).A;
    led.K -> GND;
    @5V -> test_point: TestPoint().1;
}
"#;

    // Parse the code
    info!("Parsing BHDL code...");
    let parse_result = bhdl_parser::parse(test_code);
    
    if !parse_result.errors().is_empty() {
        for error in parse_result.errors() {
            eprintln!("Parse error: {:?}", error);
        }
        return Err(anyhow::anyhow!("Parsing failed with errors"));
    }

    // Convert to AST
    let syntax_tree = parse_result.syntax();
    let source_file = bhdl_ast::SourceFile::cast(syntax_tree)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    
    // Run semantic analysis
    info!("Running semantic analysis...");
    let analysis_result = analyze(&source_file);
    
    // Generate netlist
    info!("Generating netlist with net naming support...");
    let config = NetlistConfig {
        ..Default::default()
    };
    
    let mut generator = NetlistGenerator::with_config(config);
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis_result).await?;
    
    // Verify the results
    info!("\n=== Netlist Results ===");
    info!("Total nets: {}", netlist.nets.len());
    info!("Total instances: {}", netlist.instances.len());
    
    // Check named nets
    info!("\nNamed nets:");
    for (net_id, net) in netlist.nets.iter() {
        if let Some(name) = &net.name {
            info!("  Net {:?}: '{}'", net_id, name);
            
            // Count connections
            info!("    Connections: {}", net.connections.len());
            
            // List connected components
            for conn in &net.connections {
                match conn {
                    bhdl_netlist::types::ConnectionPoint::PinInstance(pin_inst_id) => {
                        if let Some(pin_inst) = netlist.get_pin_instance(*pin_inst_id) {
                            if let Some(inst) = netlist.get_instance(pin_inst.instance) {
                                if let Some(pin) = netlist.get_pin(pin_inst.pin_def) {
                                    info!("      -> {}.{}", inst.name, pin.name);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    
    // Check that specific nets were created
    let mut found_raw = false;
    let mut found_protected = false;
    let mut found_5v = false;
    
    for (_net_id, net) in netlist.nets.iter() {
        if let Some(name) = &net.name {
            match name.as_str() {
                "RAW" => found_raw = true,
                "PROTECTED" => found_protected = true,
                "5V" => found_5v = true,
                _ => {}
            }
        }
    }
    
    info!("\n=== Test Results ===");
    info!("Found @RAW net: {}", found_raw);
    info!("Found @PROTECTED net: {}", found_protected);
    info!("Found @5V net: {}", found_5v);
    
    if found_raw && found_protected && found_5v {
        info!("\n✅ All named nets were created successfully!");
    } else {
        return Err(anyhow::anyhow!("Not all expected nets were created"));
    }
    
    Ok(())
}