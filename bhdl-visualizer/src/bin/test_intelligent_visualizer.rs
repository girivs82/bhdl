/// Test the intelligent visualizer that uses placement rules and signal flow analysis
/// This demonstrates the generic place and route intelligence

use std::fs;
use std::collections::HashMap;
use anyhow::Result;
use serde_json::Value;

use bhdl_visualizer::placement_rules::{self, PlacementRules};
use bhdl_visualizer::signal_flow_analyzer::{SignalFlowAnalyzer, SignalFlowAnalysis};
use bhdl_visualizer::intelligent_placer::{IntelligentPlacer, ComponentPlacement};

fn main() -> Result<()> {
    println!("=== Intelligent Circuit Visualizer ===");
    println!("Using placement rules and signal flow analysis\n");
    
    // Load netlist
    let netlist_json = fs::read_to_string("test_generic_visualizer_netlist.json")?;
    let netlist: Value = serde_json::from_str(&netlist_json)?;
    
    // Step 1: Analyze signal flow using rich netlist metadata
    println!("Step 1: Analyzing signal flow using netlist metadata...");
    let analyzer = SignalFlowAnalyzer::from_netlist(&netlist);
    let flow_analysis = analyzer.analyze_with_metadata(&netlist);
    
    println!("  Input nets: {:?}", flow_analysis.input_nets);
    println!("  Output nets: {:?}", flow_analysis.output_nets);
    println!("  Power path: {:?}", flow_analysis.power_path);
    println!("  Stages: {} identified", flow_analysis.signal_stages.len());
    println!("  Component roles from metadata:");
    for (comp, role) in &flow_analysis.component_roles {
        println!("    {}: {:?}", comp, role);
    }
    
    // Step 2: Get placement rules based on topology
    println!("\nStep 2: Selecting placement rules...");
    let rules = if flow_analysis.power_path.contains(&"L1".to_string()) {
        println!("  Detected: Buck converter topology");
        placement_rules::buck_converter_rules()
    } else {
        println!("  Using: Generic placement rules");
        placement_rules::generic_rules()
    };
    
    // Step 3: Execute intelligent placement using metadata
    println!("\nStep 3: Executing intelligent placement using component metadata...");
    let mut placer = IntelligentPlacer::new(rules, flow_analysis);
    
    // Add all components from analyzer (which already parsed the metadata)
    for (comp_name, comp_info) in analyzer.components.iter() {
        placer.add_component(comp_name.clone(), comp_info.component_type.clone());
    }
    
    let placements = placer.place_components();
    
    println!("  Placed {} components", placements.len());
    
    // Step 4: Generate SVG
    println!("\nStep 4: Generating SVG...");
    let svg = generate_svg_from_placements(&placements);
    
    // Save to file
    fs::write("test_intelligent_output.svg", svg)?;
    
    println!("\n✅ SUCCESS! Metadata-driven intelligent layout complete.");
    println!("📊 Output: test_intelligent_output.svg");
    println!("\nKey features:");
    println!("  • Uses rich netlist metadata for component roles");
    println!("  • Signal flow analysis from actual connectivity");
    println!("  • Component roles extracted from analysis_data");
    println!("  • Topology-specific placement rules applied");
    println!("  • Components grouped by actual function");
    println!("  • Alignment rules create clean lines");
    println!("  • Force-directed optimization prevents overlaps");
    println!("\nThis demonstrates using the rich metadata in netlists");
    println!("instead of inferring everything from naming patterns!");
    
    Ok(())
}

fn generate_svg_from_placements(placements: &HashMap<String, ComponentPlacement>) -> String {
    let mut svg = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="600" viewBox="0 0 1000 600">
  <title>Intelligent Circuit Layout</title>
  <rect width="100%" height="100%" fill="white"/>
  <g id="circuit">
"#);
    
    // Draw components
    for (name, placement) in placements {
        draw_component(&mut svg, name, placement);
    }
    
    // Add title and legend
    svg.push_str(r#"
    <text x="500" y="30" text-anchor="middle" font-size="18" font-weight="bold">
      Metadata-Driven Circuit Layout
    </text>
    <text x="500" y="50" text-anchor="middle" font-size="12">
      Generated using rich netlist metadata and component roles
    </text>
"#);
    
    svg.push_str("  </g>\n</svg>");
    svg
}

fn draw_component(svg: &mut String, name: &str, placement: &ComponentPlacement) {
    let (x, y) = placement.position;
    let component_type = match &name[..1] {
        "U" => "IC",
        "C" => "Capacitor",
        "R" => "Resistor",
        "L" => "Inductor",
        "D" => "Diode",
        _ => "Unknown",
    };
    
    match component_type {
        "IC" => {
            svg.push_str(&format!(
                r#"    <g transform="translate({}, {})">
      <rect x="-40" y="-30" width="80" height="60" fill="white" stroke="black" stroke-width="2"/>
      <text x="0" y="0" text-anchor="middle" font-size="12" font-weight="bold">{}</text>
    </g>
"#, x, y, name));
        }
        "Capacitor" => {
            svg.push_str(&format!(
                r#"    <g transform="translate({}, {})">
      <line x1="0" y1="-20" x2="0" y2="-10" stroke="black" stroke-width="2"/>
      <line x1="-10" y1="-10" x2="10" y2="-10" stroke="black" stroke-width="3"/>
      <line x1="-10" y1="10" x2="10" y2="10" stroke="black" stroke-width="3"/>
      <line x1="0" y1="10" x2="0" y2="20" stroke="black" stroke-width="2"/>
      <text x="20" y="0" font-size="10">{}</text>
    </g>
"#, x, y, name));
        }
        "Resistor" => {
            svg.push_str(&format!(
                r#"    <g transform="translate({}, {})">
      <path d="M -25 0 l 5 0 l 2.5 -5 l 5 10 l 5 -10 l 5 10 l 5 -10 l 5 10 l 5 -10 l 5 10 l 2.5 -5 l 5 0" 
        fill="none" stroke="black" stroke-width="2"/>
      <text x="0" y="-15" text-anchor="middle" font-size="10">{}</text>
    </g>
"#, x, y, name));
        }
        "Inductor" => {
            svg.push_str(&format!(
                r#"    <g transform="translate({}, {})">
      <path d="M -25,0 Q -18.75,-7.5 -12.5,0 Q -6.25,-7.5 0,0 Q 6.25,-7.5 12.5,0 Q 18.75,-7.5 25,0" 
            stroke="black" stroke-width="2" fill="none"/>
      <line x1="-25" y1="0" x2="-30" y2="0" stroke="black" stroke-width="2"/>
      <line x1="25" y1="0" x2="30" y2="0" stroke="black" stroke-width="2"/>
      <text x="0" y="-15" text-anchor="middle" font-size="10">{}</text>
    </g>
"#, x, y, name));
        }
        "Diode" => {
            svg.push_str(&format!(
                r#"    <g transform="translate({}, {})">
      <polyline points="-15,10 0,-10 -15,-10 -15,10" stroke="black" stroke-width="2" fill="none"/>
      <line x1="0" y1="-10" x2="0" y2="10" stroke="black" stroke-width="2"/>
      <line x1="-15" y1="0" x2="-20" y2="0" stroke="black" stroke-width="2"/>
      <line x1="0" y1="0" x2="5" y2="0" stroke="black" stroke-width="2"/>
      <text x="0" y="25" text-anchor="middle" font-size="10">{}</text>
    </g>
"#, x, y, name));
        }
        _ => {
            svg.push_str(&format!(
                r#"    <g transform="translate({}, {})">
      <circle cx="0" cy="0" r="15" fill="white" stroke="black" stroke-width="2"/>
      <text x="0" y="5" text-anchor="middle" font-size="10">{}</text>
    </g>
"#, x, y, name));
        }
    }
}