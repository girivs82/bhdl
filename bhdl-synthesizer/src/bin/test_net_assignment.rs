use anyhow::Result;
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    println!("Testing net assignment handling...\n");
    
    // Test case with net assignments like: fuse.2 -> protected_vin: TVSDiode(15V).1;
    let source_content = r#"
board NetAssignmentTest {
    power VIN = 12V @ 1A;
    ground GND;
    
    // Test net assignment pattern
    VIN -> fuse: Fuse(1A).1;
    fuse.2 -> protected_vin: TVSDiode(15V).1;
    protected_vin -> c1: Cap(100µF).+;
    protected_vin -> c2: Cap(0.1µF).1;
    
    // All grounds
    protected_vin.2 -> GND;
    c1.- -> GND;
    c2.2 -> GND;
}
"#;
    
    // Parse
    let parse_result = parse(&source_content);
    let syntax_node = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_node)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    
    // Analyze
    println!("=== Analysis Phase ===");
    let analysis = analyze(&source_file);
    
    // Generate netlist
    println!("\n=== Synthesis Phase ===");
    let config = NetlistConfig {
        preserve_semantic_context: true,
        include_power_domains: true,
        include_component_inference: true,
        flatten_hierarchy: false,
        use_database_components: false,
        database_path: None,
    };
    
    let mut generator = NetlistGenerator::with_config(config);
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await?;
    
    println!("\nNetlist Statistics:");
    println!("  Instances: {}", netlist.instances.len());
    println!("  Nets: {}", netlist.nets.len());
    
    println!("\nInstances:");
    for (id, instance) in &netlist.instances {
        println!("  {:?}: {}", id, instance.name);
    }
    
    println!("\nNets:");
    for (id, net) in &netlist.nets {
        let name = net.name.as_ref().map(|n| n.as_str()).unwrap_or("<unnamed>");
        println!("  {:?}: {} (connections: {})", id, name, net.connections.len());
        
        // Show connection count for non-empty nets
        if !net.connections.is_empty() {
            println!("    ({} connections)", net.connections.len());
        }
    }
    
    // Check if protected_vin net exists
    let protected_vin_exists = netlist.nets.values().any(|net| 
        net.name.as_ref().map(|n| n == "protected_vin").unwrap_or(false)
    );
    
    println!("\n=== Result ===");
    if protected_vin_exists {
        println!("✅ Net assignment 'protected_vin' handled correctly!");
    } else {
        println!("❌ Net assignment 'protected_vin' NOT found!");
        println!("\nDebugging: All net names:");
        for net in netlist.nets.values() {
            if let Some(name) = &net.name {
                println!("  - {}", name);
            }
        }
    }
    
    Ok(())
}