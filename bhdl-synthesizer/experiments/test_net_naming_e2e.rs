use anyhow::Result;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use log::info;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();

    info!("=== End-to-End Test: Net Naming with @ Syntax ===\n");

    // Read test file
    let input = std::fs::read_to_string("tests/test_net_naming_e2e.bhdl")?;
    info!("Input BHDL:\n{}", input);

    // Parse the code
    info!("\n1. PARSING...");
    let parse_result = parse(&input);
    
    if !parse_result.errors().is_empty() {
        eprintln!("Parse errors:");
        for error in parse_result.errors() {
            eprintln!("  - {}", error.message);
        }
        return Err(anyhow::anyhow!("Parsing failed with errors"));
    }
    info!("✅ Parsing successful!");

    // Convert to AST
    let syntax_tree = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_tree)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    
    // Run semantic analysis
    info!("\n2. SEMANTIC ANALYSIS...");
    let analysis_result = analyze(&source_file);
    
    if !analysis_result.diagnostics.is_empty() {
        eprintln!("Analysis diagnostics:");
        for diag in &analysis_result.diagnostics {
            eprintln!("  - {}", diag.message);
        }
        return Err(anyhow::anyhow!("Analysis failed with diagnostics"));
    }
    info!("✅ Analysis successful!");
    
    // Check nets in symbol table
    info!("\nNets found during analysis:");
    for (node_ptr, scope) in &analysis_result.definition_scopes {
        if let Some(scope_name) = &scope.scope_name {
            if scope_name == "NetNamingE2E" {
                for (name, symbol) in scope.get_nets() {
                    info!("  - @{}", name);
                }
            }
        }
    }
    
    // Generate netlist
    info!("\n3. NETLIST GENERATION...");
    let config = NetlistConfig {
        ..Default::default()
    };
    
    let mut generator = NetlistGenerator::with_config(config);
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis_result).await?;
    info!("✅ Netlist generated successfully!");
    
    // Verify the results
    info!("\n4. VERIFICATION...");
    info!("Total nets: {}", netlist.nets.len());
    info!("Total instances: {}", netlist.instances.len());
    
    // Check named nets
    info!("\nNamed nets in netlist:");
    let mut named_net_count = 0;
    for (net_id, net) in netlist.nets.iter() {
        if let Some(name) = &net.name {
            info!("  - {} (connections: {})", name, net.connections.len());
            named_net_count += 1;
            
            // Show what's connected to this net
            for conn in &net.connections {
                match conn {
                    bhdl_netlist::types::ConnectionPoint::PinInstance(pin_inst_id) => {
                        if let Some(pin_inst) = netlist.get_pin_instance(*pin_inst_id) {
                            if let Some(inst) = netlist.get_instance(pin_inst.instance) {
                                if let Some(pin) = netlist.get_pin(pin_inst.pin_def) {
                                    info!("    -> {}.{}", inst.name, pin.name);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    
    // Verify expected nets exist
    let expected_nets = ["INPUT_NET", "FILTERED", "VOLTAGE_DIV"];
    let mut found_all = true;
    
    info!("\nVerifying expected nets:");
    for expected in &expected_nets {
        let found = netlist.nets.iter().any(|(_, net)| {
            net.name.as_ref().map(|n| n == expected).unwrap_or(false)
        });
        
        if found {
            info!("  ✅ Found net: {}", expected);
        } else {
            info!("  ❌ Missing net: {}", expected);
            found_all = false;
        }
    }
    
    if found_all && named_net_count >= expected_nets.len() {
        info!("\n✅ END-TO-END TEST PASSED!");
        info!("Successfully processed net naming through parser → analyzer → synthesizer");
    } else {
        return Err(anyhow::anyhow!("Not all expected nets were created"));
    }
    
    Ok(())
}