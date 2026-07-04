use anyhow::Result;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .init();

    // Read test file
    let input = std::fs::read_to_string("tests/test_net_ref_with_space.bhdl")?;
    println!("Testing 'VIN @RAW->' pattern...\n");

    // Parse
    let parse_result = parse(&input);
    if !parse_result.errors().is_empty() {
        eprintln!("Parse errors:");
        for error in parse_result.errors() {
            eprintln!("  - {}", error.message);
        }
        return Err(anyhow::anyhow!("Parsing failed"));
    }
    println!("✅ Parsing successful!");

    // Convert to AST
    let syntax_tree = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_tree)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    
    // Analyze
    let analysis_result = analyze(&source_file);
    if !analysis_result.diagnostics.is_empty() {
        eprintln!("Analysis diagnostics:");
        for diag in &analysis_result.diagnostics {
            eprintln!("  - {}", diag.message);
        }
        return Err(anyhow::anyhow!("Analysis failed"));
    }
    println!("✅ Analysis successful!");
    
    // Generate netlist
    let config = NetlistConfig {
        ..Default::default()
    };
    
    let mut generator = NetlistGenerator::with_config(config);
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis_result).await?;
    println!("✅ Netlist generated successfully!");
    
    // Check for named nets
    println!("\nNamed nets created:");
    for (_, net) in netlist.nets.iter() {
        if let Some(name) = &net.name {
            println!("  - {}", name);
        }
    }
    
    // Verify expected nets
    let expected_nets = ["RAW", "PROTECTED"];
    for expected in &expected_nets {
        let found = netlist.nets.iter().any(|(_, net)| {
            net.name.as_ref().map(|n| n == expected).unwrap_or(false)
        });
        
        if found {
            println!("✅ Found expected net: {}", expected);
        } else {
            return Err(anyhow::anyhow!("Missing expected net: {}", expected));
        }
    }
    
    println!("\n✅ All tests passed!");
    Ok(())
}