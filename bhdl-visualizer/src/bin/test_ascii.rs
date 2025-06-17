//! Test ASCII renderer with voltage regulator circuit

use anyhow::Result;
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};
use bhdl_visualizer::ascii_renderer::render_ascii;

#[tokio::main]
async fn main() -> Result<()> {
    // BHDL source for a voltage regulator
    let bhdl_source = r#"
board LinearRegulator {
    power VIN = 12V @ 1A;
    power VCC = 5V @ 500mA;
    ground GND;

    VIN -> C1: Cap(100uF, 25V).pos -> U1: LM7805(package="TO-220").IN;
    GND -> C1.neg -> U1.GND;
    U1.OUT -> VCC;
    VCC -> C2: Cap(10uF, 10V).pos;
    GND -> C2.neg;
    VCC -> R1: Res(330).1 -> D1: LED(red).A;
    GND -> D1.K;
    VCC -> C3: Cap(100nF).pos;
    GND -> C3.neg;
}
"#;

    // Parse
    let parsed = parse(bhdl_source);
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    
    // Analyze
    let analysis = analyze(&source_file);
    
    // Generate netlist
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await?;
    
    // Render to ASCII
    let ascii = render_ascii(&netlist, Some(&analysis), 80, 25)?;
    
    println!("ASCII Schematic Rendering:");
    println!("==========================");
    println!("{}", ascii);
    
    Ok(())
}