use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;

#[tokio::main]
async fn main() {
    env_logger::init();
    
    let source = r#"
    interface I2C {
        signal SDA: inout;
        signal SCL: out;
    }
    
    board TestBoard {
        power VCC = 3.3V @ 1A;
        ground GND;
        
        // Interface instance
        i2c_bus: I2C();
    }
    "#;
    
    println!("Parsing source...");
    let parsed = parse(source);
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    
    println!("Running analysis...");
    let analysis = analyze(&source_file);
    
    println!("\n=== Symbol Table Contents ===");
    for symbol in analysis.global_scope.iter() {
        println!("Symbol: {} (kind: {:?}, type: {:?})", 
                 symbol.name, 
                 symbol.kind, 
                 symbol.instance_type_name);
    }
    
    println!("\n=== Component Inference Results ===");
    println!("Inferred components: {}", analysis.component_inference.inferred_components.len());
    for comp in &analysis.component_inference.inferred_components {
        println!("  Component: {} (type: {:?}, reasoning: {})", 
                 comp.instance_name.as_ref().unwrap_or(&"<unnamed>".to_string()),
                 comp.component_type,
                 comp.reasoning);
    }
    
    println!("\n=== Generating Netlist ===");
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await
        .expect("Failed to generate netlist");
    
    println!("\n=== Netlist Results ===");
    println!("Modules: {}", netlist.modules.len());
    println!("Instances: {}", netlist.instances.len());
    println!("Nets: {}", netlist.nets.len());
    
    println!("\nNets:");
    for (_, net) in netlist.nets.iter() {
        println!("  Net: {:?}", net.name);
    }
}