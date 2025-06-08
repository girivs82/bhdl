// Integration-style tests for bhdl_visualizer

use bhdl_visualizer::visualize_netlist;
use bhdl_netlist::{Netlist, ModuleKind, ConnectionPoint, PinId, InstanceId, ModuleId};
use svg::Document;
use quick_xml::Reader;
use quick_xml::events::Event;
use std::fs;
use std::path::Path;

// Helper function to create a simple netlist with one resistor
fn create_resistor_netlist() -> (Netlist, PinId, PinId, InstanceId) {
    let mut netlist = Netlist::new();
    let mod_id = netlist.add_module("Resistor".to_string(), ModuleKind::PhysicalComponent);
    let p1 = netlist.add_pin(mod_id, "1".to_string()).unwrap();
    let p2 = netlist.add_pin(mod_id, "2".to_string()).unwrap(); 
    let r1_inst = netlist.add_instance("R1".to_string(), mod_id).unwrap();
    (netlist, p1, p2, r1_inst)
}

// Helper to check SVG structure using quick-xml
fn check_svg_element(svg: &str, tag_name: &str, check_attr: Option<(&str, &str)>, should_exist: bool) -> bool {
    let mut reader = Reader::from_str(svg);
    reader.trim_text(true);
    let mut buf = Vec::new();
    let mut found = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                if e.name().as_ref() == tag_name.as_bytes() {
                    if let Some((attr_name, attr_value)) = check_attr {
                        let mut attr_found = false;
                        for attr_result in e.attributes() {
                            if let Ok(attr) = attr_result {
                                if attr.key.as_ref() == attr_name.as_bytes() {
                                    // Use starts_with for potentially complex attributes like transform
                                    // For simple values, require exact match.
                                    if attr_name == "transform" {
                                        // Basic check for transform presence/start if needed
                                        if String::from_utf8_lossy(attr.value.as_ref()).contains(attr_value) {
                                             attr_found = true;
                                             break;
                                        }
                                    } else if attr.value.as_ref() == attr_value.as_bytes() {
                                        attr_found = true;
                                        break;
                                    }
                                }
                            }
                        }
                        if attr_found {
                            found = true;
                            break; // Found tag with matching attribute
                        }
                        // If attribute check was requested but not found, continue searching other tags
                    } else {
                        // If no attribute check needed, just finding the tag is enough
                        found = true;
                        break; // Found tag
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                eprintln!("XML parsing error: {}", e);
                return false; // Indicate error during check
            }
            _ => (),
        }
        buf.clear();
    }
    found == should_exist
}

// Helper to check if text content exists (ignores surrounding tags/whitespace)
fn check_svg_text_content(svg: &str, text_content: &str, should_exist: bool) -> bool {
    let mut reader = Reader::from_str(svg);
    reader.trim_text(true);
    let mut buf = Vec::new();
    let mut found = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Text(e)) => {
                if let Ok(text) = e.unescape() {
                    if text.trim() == text_content {
                        found = true;
                        break;
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                 eprintln!("XML parsing error: {}", e);
                return false; 
            }
            _ => ()
        }
         buf.clear();
    }
     found == should_exist
}

// Helper function to save SVG output to a file
fn save_svg_test_output(test_name: &str, doc: &Document) {
    let output_dir = Path::new("target").join("test-svgs");
    fs::create_dir_all(&output_dir).expect("Failed to create test output directory");
    let file_path = output_dir.join(format!("{}.svg", test_name));
    svg::save(&file_path, doc).expect(&format!("Failed to save SVG to {:?}", file_path));
    println!("Saved SVG output to: {:?}", file_path); // Optional: Confirm save location
}

#[test]
fn test_empty_netlist() {
    let netlist = Netlist::new();
    let doc = visualize_netlist(&netlist);
    let svg_string = doc.to_string();
    println!("Empty Netlist SVG:\n{}", svg_string);

    assert!(check_svg_element(&svg_string, "svg", None, true), "Missing <svg> tag");
    // Check for some width/height, but not specific values unless min size is guaranteed
    // assert!(check_svg_element(&svg_string, "svg", Some(("width", "100")), true), "Incorrect/missing width"); 
    // assert!(check_svg_element(&svg_string, "svg", Some(("height", "100")), true), "Incorrect/missing height");
    assert!(!check_svg_element(&svg_string, "g", None, true), "Unexpected <g> tag found");

    save_svg_test_output("empty_netlist", &doc);
}

#[test]
fn test_simple_resistor_no_nets() {
    let (netlist, _p1, _p2, _r1_inst) = create_resistor_netlist();
    let doc = visualize_netlist(&netlist);
    let svg_string = doc.to_string();
    println!("Simple Resistor (No Nets) SVG:\n{}", svg_string);

    assert!(check_svg_element(&svg_string, "svg", None, true), "Missing <svg> tag");
    // Check R1 group exists (check for a group with a rect inside maybe?)
    assert!(check_svg_element(&svg_string, "g", None, true), "Should contain at least one <g> for the instance");
    assert!(check_svg_element(&svg_string, "rect", None, true), "Missing <rect> for resistor body");
    assert!(check_svg_text_content(&svg_string, "R1", true), "Missing text content 'R1'");
    // Check nets group does NOT exist
    assert!(!check_svg_element(&svg_string, "g", Some(("id", "nets")), true), "Unexpected nets group found");

    save_svg_test_output("simple_resistor_no_nets", &doc);
}

#[test]
fn test_resistor_with_stub_net() {
    let (mut netlist, p1, _p2, r1_inst) = create_resistor_netlist();
    let net_id = netlist.add_net(Some("signal".to_string()));
    netlist.connect(net_id, ConnectionPoint::InstancePin(r1_inst, p1)).unwrap();
    
    let doc = visualize_netlist(&netlist);
    let svg_string = doc.to_string();
    println!("Resistor Stub Net SVG:\n{}", svg_string);

    assert!(check_svg_element(&svg_string, "svg", None, true), "Missing <svg> tag");
    // Check nets group does NOT exist (since no lines drawn)
    assert!(!check_svg_element(&svg_string, "g", Some(("id", "nets")), true), "Unexpected nets group found");

    save_svg_test_output("resistor_with_stub_net", &doc);
}


#[test]
fn test_two_resistors_connected() {
    let (mut netlist, _r1_p1, r1_p2, r1_inst) = create_resistor_netlist();
    let res_mod_id = netlist.modules.keys().next().unwrap(); 
    let r2_inst = netlist.add_instance("R2".to_string(), res_mod_id).unwrap();
    let mut r2_p1 = None;
    for pin_id in &netlist.modules[res_mod_id].pins {
        if let Some(pin_data) = netlist.get_pin(*pin_id) {
            if pin_data.name == "1" { r2_p1 = Some(*pin_id); }
        }
    }
    let r2_p1 = r2_p1.expect("Could not find pin '1' for R2 module");
    
    let net_id = netlist.add_net(Some("N1".to_string()));
    netlist.connect(net_id, ConnectionPoint::InstancePin(r1_inst, r1_p2)).unwrap();
    netlist.connect(net_id, ConnectionPoint::InstancePin(r2_inst, r2_p1)).unwrap();
    
    let doc = visualize_netlist(&netlist);
    let svg_string = doc.to_string();
    println!("Two Resistors Connected SVG:\n{}", svg_string);

    assert!(check_svg_element(&svg_string, "svg", None, true), "Missing <svg> tag");
    // Check R1 and R2 elements exist
    assert!(check_svg_text_content(&svg_string, "R1", true), "Missing text content 'R1'");
    assert!(check_svg_text_content(&svg_string, "R2", true), "Missing text content 'R2'");
    // Check nets group exists
    assert!(check_svg_element(&svg_string, "g", Some(("id", "nets")), true), "Missing nets group");
    // Check nets group has correct stroke attribute
    assert!(check_svg_element(&svg_string, "g", Some(("stroke", "blue")), true), "Missing/incorrect nets group stroke");
    // Check line element exists within the SVG 
    assert!(check_svg_element(&svg_string, "line", None, true), "Missing line element for net");
    // // Check line attributes (fragile, requires parsing attributes within the line tag) - REMOVED checks for attributes on line itself
    // assert!(check_svg_element(&svg_string, "line", Some(("x1", "120")), true), "Missing/incorrect line x1");
    // assert!(check_svg_element(&svg_string, "line", Some(("y1", "40")), true), "Missing/incorrect line y1");
    // assert!(check_svg_element(&svg_string, "line", Some(("x2", "150")), true), "Missing/incorrect line x2");
    // assert!(check_svg_element(&svg_string, "line", Some(("y2", "40")), true), "Missing/incorrect line y2");

    save_svg_test_output("two_resistors_connected", &doc);
}

// Helper function to create a Resistor component module and return its ID and pin IDs
// Renamed from create_resistor_netlist to avoid confusion, as it only defines the module.
fn define_resistor_module(netlist: &mut Netlist) -> (ModuleId, PinId, PinId) {
    let mod_id = netlist.add_module("Resistor".to_string(), ModuleKind::PhysicalComponent);
    let p1 = netlist.add_pin(mod_id, "1".to_string()).unwrap();
    let p2 = netlist.add_pin(mod_id, "2".to_string()).unwrap(); 
    (mod_id, p1, p2)
}

// Helper function to create a Capacitor component module and return its ID and pin IDs
fn define_capacitor_module(netlist: &mut Netlist) -> (ModuleId, PinId, PinId) {
    let mod_id = netlist.add_module("Capacitor".to_string(), ModuleKind::PhysicalComponent);
    let p1 = netlist.add_pin(mod_id, "1".to_string()).unwrap();
    let p2 = netlist.add_pin(mod_id, "2".to_string()).unwrap();
    (mod_id, p1, p2)
}

// Helper function to create a Ground component module and return its ID and pin ID
fn define_ground_module(netlist: &mut Netlist) -> (ModuleId, PinId) {
    let mod_id = netlist.add_module("Ground".to_string(), ModuleKind::PhysicalComponent);
    // Match the pin name used in layout.rs and symbols/power.rs
    let pin_gnd = netlist.add_pin(mod_id, "GND".to_string()).unwrap();
    (mod_id, pin_gnd)
}


#[test]
fn test_rc_low_pass_filter() {
    let mut netlist = Netlist::new();

    // Define component types
    let (res_mod_id, r_p1, r_p2) = define_resistor_module(&mut netlist); // Use new helper
    let (cap_mod_id, c_p1, c_p2) = define_capacitor_module(&mut netlist);
    let (gnd_mod_id, gnd_pin) = define_ground_module(&mut netlist);

    // Instantiate components
    let r1_inst = netlist.add_instance("R1".to_string(), res_mod_id).unwrap();
    let c1_inst = netlist.add_instance("C1".to_string(), cap_mod_id).unwrap();
    let gnd_inst = netlist.add_instance("GND".to_string(), gnd_mod_id).unwrap(); // Instance name GND

    // Define nets (representing input, output, and ground rail)
    // We won't draw explicit input/output ports, just the core RC connection.
    let net_input = netlist.add_net(Some("Net_Input".to_string())); // Define input net
    let net_output = netlist.add_net(Some("Net_Output".to_string()));
    let net_gnd = netlist.add_net(Some("Net_GND".to_string()));

    // Connect components
    // Input connects to R1.1 (r_p1)
    netlist.connect(net_input, ConnectionPoint::InstancePin(r1_inst, r_p1)).unwrap();
    // Output is taken from junction of R1.2 (r_p2) and C1.1 (c_p1)
    netlist.connect(net_output, ConnectionPoint::InstancePin(r1_inst, r_p2)).unwrap();
    netlist.connect(net_output, ConnectionPoint::InstancePin(c1_inst, c_p1)).unwrap();
    // C1.2 (c_p2) connects to ground
    netlist.connect(net_gnd, ConnectionPoint::InstancePin(c1_inst, c_p2)).unwrap();
    // Ground symbol connects to ground net
    netlist.connect(net_gnd, ConnectionPoint::InstancePin(gnd_inst, gnd_pin)).unwrap();

    // Visualize
    let doc = visualize_netlist(&netlist);
    let svg_string = doc.to_string();
    println!("\n--- RC Low Pass Filter SVG ---\n{}\n--- End RC LPF SVG ---", svg_string);

    // Assertions
    assert!(check_svg_element(&svg_string, "svg", None, true), "Missing <svg> tag");
    assert!(check_svg_text_content(&svg_string, "R1", true), "Missing text content 'R1'");
    assert!(check_svg_text_content(&svg_string, "C1", true), "Missing text content 'C1'");
    assert!(check_svg_text_content(&svg_string, "GND", true), "Missing text content 'GND'"); // Check for GND instance name
    assert!(check_svg_element(&svg_string, "g", Some(("id", "nets")), true), "Missing nets group");

    // Count lines within the nets group specifically
     let mut reader = Reader::from_str(&svg_string);
     reader.trim_text(true);
     let mut buf = Vec::new();
     let mut line_count_in_nets = 0;
     let mut in_nets_group = false;
     loop {
         match reader.read_event_into(&mut buf) {
             Ok(Event::Start(ref e)) if e.name().as_ref() == b"g" => {
                 if e.attributes().any(|a| a.map(|attr| attr.key.as_ref() == b"id" && attr.value.as_ref() == b"nets").unwrap_or(false)) {
                     in_nets_group = true;
                 }
             }
             Ok(Event::End(ref e)) if e.name().as_ref() == b"g" => {
                 // Simple check, might break with nested groups. Assume nets group isn't nested for now.
                 if in_nets_group { // Only turn off if we were inside the nets group
                    in_nets_group = false;
                 }
             }
             Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) if e.name().as_ref() == b"line" => {
                 if in_nets_group {
                     line_count_in_nets += 1;
                 }
             }
             Ok(Event::Eof) => break,
             Err(e) => {
                eprintln!("XML parsing error during line count: {}", e);
                break; // Stop counting on error
             }
             _ => (),
         }
         buf.clear();
     }
     // Expected lines: Net_Output (R1.2 -> C1.1) = 1 line, Net_GND (C1.2 -> GND_sym.GND) = 1 line
     assert_eq!(line_count_in_nets, 2, "Expected 2 lines in the nets group, found {}", line_count_in_nets);

    save_svg_test_output("rc_low_pass_filter", &doc);
}
