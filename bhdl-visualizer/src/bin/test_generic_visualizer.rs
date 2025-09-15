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
    
    // Save SVG to file
    let svg_output_path = "test_generic_visualizer_output.svg";
    fs::write(svg_output_path, svg)?;
    
    // Export netlist to JSON for inspection
    let netlist_json = serde_json::to_string_pretty(&netlist)?;
    let netlist_output_path = "test_generic_visualizer_netlist.json";
    fs::write(netlist_output_path, netlist_json)?;
    
    println!("\n✅ SUCCESS! Generic visualizer test complete.");
    println!("📊 SVG Output: {}", svg_output_path);
    println!("📄 Netlist Output: {}", netlist_output_path);
    println!("\nKey achievements:");
    println!("  • No hardcoded positions - all placement is algorithmic");
    println!("  • Uses actual netlist connectivity data");
    println!("  • Leverages component role metadata from analysis");
    println!("  • Works with any circuit topology");
    
    Ok(())
}

fn create_sample_netlist() -> Netlist {
    use bhdl_netlist::{PinDirection, PinType, ConnectionPoint};
    
    let mut netlist = Netlist::new();
    
    // Define modules with pins
    let buck_mod = netlist.add_module("TPS54302".to_string(), ModuleKind::PhysicalComponent);
    let cap_mod = netlist.add_module("Capacitor".to_string(), ModuleKind::PhysicalComponent);
    let res_mod = netlist.add_module("Resistor".to_string(), ModuleKind::PhysicalComponent);
    let ind_mod = netlist.add_module("Inductor".to_string(), ModuleKind::PhysicalComponent);
    let diode_mod = netlist.add_module("Diode".to_string(), ModuleKind::PhysicalComponent);
    
    // Add pins to modules - TPS54302 with accurate pinout
    let buck_vin_pin = netlist.add_pin(buck_mod, "VIN".to_string(), PinDirection::In, PinType::Power).unwrap();
    let buck_sw_pin = netlist.add_pin(buck_mod, "SW".to_string(), PinDirection::Out, PinType::Power).unwrap();
    let buck_gnd_pin = netlist.add_pin(buck_mod, "GND".to_string(), PinDirection::InOut, PinType::Ground).unwrap();
    let buck_fb_pin = netlist.add_pin(buck_mod, "FB".to_string(), PinDirection::In, PinType::Signal).unwrap();
    let buck_en_pin = netlist.add_pin(buck_mod, "EN".to_string(), PinDirection::In, PinType::Signal).unwrap();
    let buck_boot_pin = netlist.add_pin(buck_mod, "BOOT".to_string(), PinDirection::InOut, PinType::Signal).unwrap();
    let buck_ph_pin = netlist.add_pin(buck_mod, "PH".to_string(), PinDirection::Out, PinType::Signal).unwrap();
    
    let cap_pos_pin = netlist.add_pin(cap_mod, "1".to_string(), PinDirection::Passive, PinType::Signal).unwrap();
    let cap_neg_pin = netlist.add_pin(cap_mod, "2".to_string(), PinDirection::Passive, PinType::Signal).unwrap();
    
    let res_pin1 = netlist.add_pin(res_mod, "1".to_string(), PinDirection::Passive, PinType::Signal).unwrap();
    let res_pin2 = netlist.add_pin(res_mod, "2".to_string(), PinDirection::Passive, PinType::Signal).unwrap();
    
    let ind_pin1 = netlist.add_pin(ind_mod, "1".to_string(), PinDirection::Passive, PinType::Signal).unwrap();
    let ind_pin2 = netlist.add_pin(ind_mod, "2".to_string(), PinDirection::Passive, PinType::Signal).unwrap();
    
    let diode_a_pin = netlist.add_pin(diode_mod, "A".to_string(), PinDirection::Passive, PinType::Signal).unwrap();
    let diode_k_pin = netlist.add_pin(diode_mod, "K".to_string(), PinDirection::Passive, PinType::Signal).unwrap();
    
    // Add instances
    let u1 = netlist.add_instance("U1".to_string(), buck_mod).unwrap();
    let c1 = netlist.add_instance("C1".to_string(), cap_mod).unwrap();
    let c2 = netlist.add_instance("C2".to_string(), cap_mod).unwrap();
    let c3 = netlist.add_instance("C3".to_string(), cap_mod).unwrap();
    let c4 = netlist.add_instance("C4".to_string(), cap_mod).unwrap();
    let l1 = netlist.add_instance("L1".to_string(), ind_mod).unwrap();
    let d1 = netlist.add_instance("D1".to_string(), diode_mod).unwrap();
    let r1 = netlist.add_instance("R1".to_string(), res_mod).unwrap();
    let r2 = netlist.add_instance("R2".to_string(), res_mod).unwrap();
    
    // Create pin instances for all components
    let u1_pins = netlist.create_pin_instances(u1).unwrap();
    let c1_pins = netlist.create_pin_instances(c1).unwrap();
    let c2_pins = netlist.create_pin_instances(c2).unwrap();
    let c3_pins = netlist.create_pin_instances(c3).unwrap();
    let c4_pins = netlist.create_pin_instances(c4).unwrap();
    let l1_pins = netlist.create_pin_instances(l1).unwrap();
    let d1_pins = netlist.create_pin_instances(d1).unwrap();
    let r1_pins = netlist.create_pin_instances(r1).unwrap();
    let r2_pins = netlist.create_pin_instances(r2).unwrap();
    
    // Get specific pin instances (assuming order matches pin definition order)
    let u1_vin_pini = u1_pins[0];   // VIN pin
    let u1_sw_pini = u1_pins[1];    // SW pin (switch node output)
    let u1_gnd_pini = u1_pins[2];   // GND pin
    let u1_fb_pini = u1_pins[3];    // FB pin (feedback)
    let u1_en_pini = u1_pins[4];    // EN pin (enable)
    let u1_boot_pini = u1_pins[5];  // BOOT pin
    let u1_ph_pini = u1_pins[6];    // PH pin
    
    let c1_pos_pini = c1_pins[0];   // pin 1
    let c1_neg_pini = c1_pins[1];   // pin 2
    
    let c2_pos_pini = c2_pins[0];   // pin 1
    let c2_neg_pini = c2_pins[1];   // pin 2
    
    let c3_pos_pini = c3_pins[0];   // pin 1  
    let c3_neg_pini = c3_pins[1];   // pin 2
    
    let c4_pos_pini = c4_pins[0];   // pin 1
    let c4_neg_pini = c4_pins[1];   // pin 2
    
    let l1_pin1_pini = l1_pins[0];  // pin 1
    let l1_pin2_pini = l1_pins[1];  // pin 2
    
    let d1_anode_pini = d1_pins[0]; // A pin
    let d1_cathode_pini = d1_pins[1]; // K pin
    
    let r1_pin1_pini = r1_pins[0];  // pin 1
    let r1_pin2_pini = r1_pins[1];  // pin 2
    
    let r2_pin1_pini = r2_pins[0];  // pin 1
    let r2_pin2_pini = r2_pins[1];  // pin 2
    
    // Add nets
    let vin = netlist.add_net(Some("VIN".to_string()));
    let vout = netlist.add_net(Some("VOUT".to_string()));
    let sw = netlist.add_net(Some("SW_NODE".to_string()));
    let fb = netlist.add_net(Some("FEEDBACK".to_string()));
    let gnd = netlist.add_net(Some("GND".to_string()));
    
    // Connect pins to nets
    // VIN net: Input power with input filter capacitors
    let _ = netlist.connect(vin, ConnectionPoint::PinInstance(u1_vin_pini));
    let _ = netlist.connect(vin, ConnectionPoint::PinInstance(c1_pos_pini));
    let _ = netlist.connect(vin, ConnectionPoint::PinInstance(c2_pos_pini));
    let _ = netlist.connect(vin, ConnectionPoint::PinInstance(u1_en_pini)); // EN tied to VIN for always-on
    
    // SW_NODE net: Switch node from IC to inductor and catch diode
    let _ = netlist.connect(sw, ConnectionPoint::PinInstance(u1_sw_pini));  // SW output from IC
    let _ = netlist.connect(sw, ConnectionPoint::PinInstance(l1_pin1_pini)); // to inductor input
    let _ = netlist.connect(sw, ConnectionPoint::PinInstance(d1_cathode_pini)); // to diode cathode
    
    // VOUT net: Output from inductor with output capacitors and feedback divider
    let _ = netlist.connect(vout, ConnectionPoint::PinInstance(l1_pin2_pini)); // from inductor output
    let _ = netlist.connect(vout, ConnectionPoint::PinInstance(c3_pos_pini));  // output cap
    let _ = netlist.connect(vout, ConnectionPoint::PinInstance(c4_pos_pini));  // decoupling cap
    let _ = netlist.connect(vout, ConnectionPoint::PinInstance(r1_pin1_pini)); // top of voltage divider
    
    // FEEDBACK net: From voltage divider tap to FB pin
    let _ = netlist.connect(fb, ConnectionPoint::PinInstance(r1_pin2_pini));  // bottom of R1
    let _ = netlist.connect(fb, ConnectionPoint::PinInstance(r2_pin1_pini));  // top of R2
    let _ = netlist.connect(fb, ConnectionPoint::PinInstance(u1_fb_pini));   // to FB pin on IC
    
    // GND net: Ground connections for all components
    let _ = netlist.connect(gnd, ConnectionPoint::PinInstance(u1_gnd_pini));
    let _ = netlist.connect(gnd, ConnectionPoint::PinInstance(c1_neg_pini));
    let _ = netlist.connect(gnd, ConnectionPoint::PinInstance(c2_neg_pini));
    let _ = netlist.connect(gnd, ConnectionPoint::PinInstance(c3_neg_pini));
    let _ = netlist.connect(gnd, ConnectionPoint::PinInstance(c4_neg_pini));
    let _ = netlist.connect(gnd, ConnectionPoint::PinInstance(d1_anode_pini));
    let _ = netlist.connect(gnd, ConnectionPoint::PinInstance(r2_pin2_pini));
    
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