//! Test complete visualization pipeline with power symbols
//! Verifies power/ground symbols are placed and nets are rendered

use anyhow::Result;
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};
use bhdl_visualizer::sugiyama_layout::SugiyamaLayoutEngine;
use log::info;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();

    info!("=== Testing Power Symbol Visualization ===\n");

    // Load the test circuit with real regulator and power symbols
    let circuit_path = "tests/circuits/simple/test_intent_simple_with_real_regulator.bhdl";
    let source = std::fs::read_to_string(circuit_path)?;
    info!("📄 Loaded circuit: {}", circuit_path);

    // Parse
    let parsed = parse(&source);
    let ast = SourceFile::cast(parsed.syntax())
        .ok_or_else(|| anyhow::anyhow!("Failed to parse"))?;
    info!("✅ Parsing complete");

    // Analyze
    let analysis = analyze(&ast);
    info!("✅ Analysis complete: {} power domains", analysis.power_analysis.domains.len());

    // Generate netlist with power symbols
    let config = NetlistConfig::default();
    let mut generator = NetlistGenerator::with_config(config);
    let netlist = generator.generate_from_ast_and_analysis(&ast, &analysis).await?;
    let component_instances = generator.get_component_instances();

    info!("✅ Netlist generated:");
    info!("   Modules: {}", netlist.modules.len());
    info!("   Instances: {}", netlist.instances.len());
    info!("   Nets: {}", netlist.nets.len());
    info!("   Database components: {}", component_instances.len());

    // Create NEW Sugiyama hierarchical layout engine
    let mut layout_engine = SugiyamaLayoutEngine::new();

    // Generate layout using Sugiyama algorithm
    info!("\n🎨 Generating Sugiyama hierarchical layout...");
    let layout = layout_engine.layout_circuit(&netlist, &component_instances).await?;

    info!("✅ Layout generated:");
    info!("   Components placed: {}", layout.components.len());
    info!("   Nets routed: {}", layout.nets.len());

    // List all placed components
    info!("\n📍 Placed components:");
    for component in &layout.components {
        let label = component.label.as_ref()
            .or_else(|| netlist.instances.get(component.instance_id).map(|i| &i.name))
            .map(|s| s.as_str()).unwrap_or("unknown");
        info!("   {} at ({:.1}, {:.1})",
              label,
              component.position.x,
              component.position.y);
    }

    // Check for power symbols by looking up instance names
    let has_gnd = layout.components.iter().any(|c| {
        netlist.instances.get(c.instance_id).map(|i| i.name.as_str()) == Some("GND")
    });
    let has_vin = layout.components.iter().any(|c| {
        netlist.instances.get(c.instance_id).map(|i| i.name.as_str()) == Some("VIN")
    });
    let has_vout = layout.components.iter().any(|c| {
        netlist.instances.get(c.instance_id).map(|i| i.name.as_str()) == Some("VOUT")
    });

    info!("\n🎯 Power symbol placement:");
    info!("   GND: {}", if has_gnd { "✓ Placed" } else { "✗ Missing" });
    info!("   VIN (+12V): {}", if has_vin { "✓ Placed" } else { "✗ Missing" });
    info!("   VOUT (+5V): {}", if has_vout { "✓ Placed" } else { "✗ Missing" });

    // Generate SVG
    info!("\n📊 Generating SVG...");
    let output_path = "tests/outputs/svg/power_symbol_test.svg";
    bhdl_visualizer::semantic_visualizer::generate_svg(&layout, output_path)?;
    info!("✅ SVG saved to: {}", output_path);

    // Verify nets
    info!("\n🔌 Net routing:");
    info!("   Total nets: {}", layout.nets.len());
    for net in &layout.nets {
        info!("   Net '{}': {} connection points",
              net.name.as_ref().unwrap_or(&"unnamed".to_string()),
              net.connection_points.len());
    }

    if has_gnd && has_vin && has_vout && layout.nets.len() > 0 {
        info!("\n✅ SUCCESS! Complete visualization pipeline working:");
        info!("   • Power symbols correctly instantiated in synthesizer");
        info!("   • Power symbols placed in layout");
        info!("   • Nets routed and rendered");
        info!("   • SVG output generated");
    } else {
        info!("\n⚠️  Partial success - some issues detected");
    }

    Ok(())
}
