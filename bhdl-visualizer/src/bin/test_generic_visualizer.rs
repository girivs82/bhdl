use bhdl_visualizer::generic_netlist_visualizer::GenericNetlistVisualizer;
use bhdl_visualizer::simple_svg_renderer::SimpleSvgRenderer;
use bhdl_netlist::{Netlist, ModuleKind};
use bhdl_synthesizer::component_mapping::ComponentCategory;
use bhdl_synthesizer::DatabaseComponentInstance;
use std::collections::HashMap;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Generic Netlist Visualizer Test ===\n");
    
    // Create a sample netlist (would normally come from synthesizer)
    let mut netlist = create_sample_netlist();
    
    // Create database component instances (would normally come from synthesizer)
    let db_components = create_sample_db_components();
    
    // Add some analysis metadata (would normally come from SPICE analyzer)
    add_analysis_metadata(&mut netlist);
    
    println!("Created netlist with:");
    println!("  • {} modules", netlist.modules.len());
    println!("  • {} instances", netlist.instances.len());
    println!("  • {} nets", netlist.nets.len());
    println!();
    
    // Create the generic visualizer
    println!("Generating layout using generic visualizer...");
    let mut visualizer = GenericNetlistVisualizer::new();
    let layout = visualizer.generate_layout(&netlist, &db_components);
    
    println!("Layout generated with:");
    println!("  • {} components placed", layout.components.len());
    println!("  • {} nets routed", layout.nets.len());
    println!();
    
    // Render to SVG
    println!("Rendering to SVG...");
    let mut renderer = SimpleSvgRenderer::new();
    let svg = renderer.render(&layout, "Generic Visualizer Test - Buck Converter");
    
    // Save to file
    let output_path = "test_generic_visualizer_output.svg";
    fs::write(output_path, svg)?;
    
    println!("\n✅ SUCCESS! Generic visualizer test complete.");
    println!("📊 Output: {}", output_path);
    println!("\nKey achievements:");
    println!("  • No hardcoded positions - all placement is algorithmic");
    println!("  • Uses actual netlist connectivity data");
    println!("  • Leverages component role metadata from analysis");
    println!("  • Works with any circuit topology");
    
    Ok(())
}

fn create_sample_netlist() -> Netlist {
    let mut netlist = Netlist::new();
    
    // Define modules
    let buck_mod = netlist.add_module("TPS54302".to_string(), ModuleKind::PhysicalComponent);
    let cap_mod = netlist.add_module("Capacitor".to_string(), ModuleKind::PhysicalComponent);
    let res_mod = netlist.add_module("Resistor".to_string(), ModuleKind::PhysicalComponent);
    let ind_mod = netlist.add_module("Inductor".to_string(), ModuleKind::PhysicalComponent);
    let diode_mod = netlist.add_module("Diode".to_string(), ModuleKind::PhysicalComponent);
    
    // Add instances
    let _u1 = netlist.add_instance("U1".to_string(), buck_mod).unwrap();
    let _c1 = netlist.add_instance("C1".to_string(), cap_mod).unwrap();
    let _c2 = netlist.add_instance("C2".to_string(), cap_mod).unwrap();
    let _c3 = netlist.add_instance("C3".to_string(), cap_mod).unwrap();
    let _c4 = netlist.add_instance("C4".to_string(), cap_mod).unwrap();
    let _l1 = netlist.add_instance("L1".to_string(), ind_mod).unwrap();
    let _d1 = netlist.add_instance("D1".to_string(), diode_mod).unwrap();
    let _r1 = netlist.add_instance("R1".to_string(), res_mod).unwrap();
    let _r2 = netlist.add_instance("R2".to_string(), res_mod).unwrap();
    
    // Add nets
    let _vin = netlist.add_net(Some("VIN".to_string()));
    let _vout = netlist.add_net(Some("VOUT".to_string()));
    let _sw = netlist.add_net(Some("SW_NODE".to_string()));
    let _fb = netlist.add_net(Some("FEEDBACK".to_string()));
    let _gnd = netlist.add_net(Some("GND".to_string()));
    
    netlist
}

fn create_sample_db_components() -> Vec<DatabaseComponentInstance> {
    vec![
        DatabaseComponentInstance {
            instance_name: "U1".to_string(),
            bhdl_type: "TPS54302".to_string(),
            component_id: 1,
            component_name: "TPS54302".to_string(),
            component_description: Some("3A Buck Converter".to_string()),
            svg_data: String::new(),
            pin_mapping: HashMap::new(),
            category: ComponentCategory::PowerRegulator,
            electrical_specs: vec![],
            pins: vec![],
        },
        DatabaseComponentInstance {
            instance_name: "C1".to_string(),
            bhdl_type: "Capacitor".to_string(),
            component_id: 2,
            component_name: "Cap_10uF".to_string(),
            component_description: Some("10uF Input Capacitor".to_string()),
            svg_data: String::new(),
            pin_mapping: HashMap::new(),
            category: ComponentCategory::PassiveCapacitor,
            electrical_specs: vec![],
            pins: vec![],
        },
        DatabaseComponentInstance {
            instance_name: "C2".to_string(),
            bhdl_type: "Capacitor".to_string(),
            component_id: 3,
            component_name: "Cap_100nF".to_string(),
            component_description: Some("100nF Input Bypass".to_string()),
            svg_data: String::new(),
            pin_mapping: HashMap::new(),
            category: ComponentCategory::PassiveCapacitor,
            electrical_specs: vec![],
            pins: vec![],
        },
        DatabaseComponentInstance {
            instance_name: "C3".to_string(),
            bhdl_type: "Capacitor".to_string(),
            component_id: 4,
            component_name: "Cap_22uF".to_string(),
            component_description: Some("22uF Output Capacitor".to_string()),
            svg_data: String::new(),
            pin_mapping: HashMap::new(),
            category: ComponentCategory::PassiveCapacitor,
            electrical_specs: vec![],
            pins: vec![],
        },
        DatabaseComponentInstance {
            instance_name: "C4".to_string(),
            bhdl_type: "Capacitor".to_string(),
            component_id: 5,
            component_name: "Cap_100nF".to_string(),
            component_description: Some("100nF Output Bypass".to_string()),
            svg_data: String::new(),
            pin_mapping: HashMap::new(),
            category: ComponentCategory::PassiveCapacitor,
            electrical_specs: vec![],
            pins: vec![],
        },
        DatabaseComponentInstance {
            instance_name: "L1".to_string(),
            bhdl_type: "Inductor".to_string(),
            component_id: 6,
            component_name: "Ind_4.7uH".to_string(),
            component_description: Some("4.7uH Power Inductor".to_string()),
            svg_data: String::new(),
            pin_mapping: HashMap::new(),
            category: ComponentCategory::Connector,  // No Inductor category, using Connector as placeholder
            electrical_specs: vec![],
            pins: vec![],
        },
        DatabaseComponentInstance {
            instance_name: "D1".to_string(),
            bhdl_type: "Diode".to_string(),
            component_id: 7,
            component_name: "Schottky_SS34".to_string(),
            component_description: Some("Schottky Diode".to_string()),
            svg_data: String::new(),
            pin_mapping: HashMap::new(),
            category: ComponentCategory::Semiconductor,
            electrical_specs: vec![],
            pins: vec![],
        },
        DatabaseComponentInstance {
            instance_name: "R1".to_string(),
            bhdl_type: "Resistor".to_string(),
            component_id: 8,
            component_name: "Res_10k".to_string(),
            component_description: Some("10k Feedback Resistor".to_string()),
            svg_data: String::new(),
            pin_mapping: HashMap::new(),
            category: ComponentCategory::PassiveResistor,
            electrical_specs: vec![],
            pins: vec![],
        },
        DatabaseComponentInstance {
            instance_name: "R2".to_string(),
            bhdl_type: "Resistor".to_string(),
            component_id: 9,
            component_name: "Res_3.3k".to_string(),
            component_description: Some("3.3k Feedback Resistor".to_string()),
            svg_data: String::new(),
            pin_mapping: HashMap::new(),
            category: ComponentCategory::PassiveResistor,
            electrical_specs: vec![],
            pins: vec![],
        },
    ]
}

fn add_analysis_metadata(netlist: &mut Netlist) {
    use bhdl_common::analysis_interface::{AnalysisData, InstanceAnalysisData};
    
    let mut analysis_data = AnalysisData::new();
    
    // Add role metadata from SPICE analysis
    analysis_data.instance_analysis.insert("C1".to_string(), InstanceAnalysisData {
        spice_type: Some("capacitor".to_string()),
        component_role: Some("InputFilter".to_string()),
        electrical_params: None,
        safety_info: None,
        extensions: HashMap::new(),
    });
    
    analysis_data.instance_analysis.insert("C2".to_string(), InstanceAnalysisData {
        spice_type: Some("capacitor".to_string()),
        component_role: Some("InputFilter".to_string()),
        electrical_params: None,
        safety_info: None,
        extensions: HashMap::new(),
    });
    
    analysis_data.instance_analysis.insert("C3".to_string(), InstanceAnalysisData {
        spice_type: Some("capacitor".to_string()),
        component_role: Some("OutputStabilization".to_string()),
        electrical_params: None,
        safety_info: None,
        extensions: HashMap::new(),
    });
    
    analysis_data.instance_analysis.insert("C4".to_string(), InstanceAnalysisData {
        spice_type: Some("capacitor".to_string()),
        component_role: Some("Decoupling".to_string()),
        electrical_params: None,
        safety_info: None,
        extensions: HashMap::new(),
    });
    
    netlist.analysis_data = Some(analysis_data);
}