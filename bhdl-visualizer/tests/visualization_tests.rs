// Integration-style tests for bhdl_visualizer

use bhdl_visualizer::visualize_netlist;
use bhdl_netlist::{Netlist, ModuleKind, ConnectionPoint, PinId, InstanceId};
use quick_xml::Reader;
use quick_xml::events::Event;

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
                                if attr.key.as_ref() == attr_name.as_bytes() && 
                                   attr.value.as_ref() == attr_value.as_bytes() {
                                    attr_found = true;
                                    break;
                                }
                            }
                        }
                        if attr_found {
                            found = true;
                            break;
                        }
                    } else {
                        // If no attribute check needed, just finding the tag is enough
                        found = true;
                        break;
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


#[test]
fn test_empty_netlist() {
    let netlist = Netlist::new();
    let doc = visualize_netlist(&netlist);
    let svg_string = doc.to_string();
    println!("Empty Netlist SVG:\n{}", svg_string);

    assert!(check_svg_element(&svg_string, "svg", None, true), "Missing <svg> tag");
    assert!(check_svg_element(&svg_string, "svg", Some(("width", "100")), true), "Incorrect/missing width");
    assert!(check_svg_element(&svg_string, "svg", Some(("height", "100")), true), "Incorrect/missing height");
    assert!(!check_svg_element(&svg_string, "g", None, true), "Unexpected <g> tag found");
}

#[test]
fn test_simple_resistor_no_nets() {
    let (netlist, _p1, _p2, _r1_inst) = create_resistor_netlist();
    
    let doc = visualize_netlist(&netlist);
    let svg_string = doc.to_string();
    println!("Simple Resistor (No Nets) SVG:\n{}", svg_string);

    assert!(check_svg_element(&svg_string, "svg", None, true), "Missing <svg> tag");
    // Check R1 group exists
    assert!(check_svg_element(&svg_string, "g", Some(("transform", "translate(75, 40)")), true), "Missing/incorrect R1 group");
    // Check R1 text exists (might need nested check)
    // assert!(check_svg_element(&svg_string, "text", Some(("y", "25")), true), "Missing/incorrect R1 text");
    // Check nets group does NOT exist
    assert!(!check_svg_element(&svg_string, "g", Some(("id", "nets")), true), "Unexpected nets group found");
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
    // // Check no line elements exist - REMOVED - This check was flawed as it found symbol lines
    // assert!(!check_svg_element(&svg_string, "line", None, true), "Unexpected line element found");
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
    // Check R1 and R2 groups exist
    assert!(check_svg_element(&svg_string, "g", Some(("transform", "translate(75, 40)")), true), "Missing/incorrect R1 group");
    assert!(check_svg_element(&svg_string, "g", Some(("transform", "translate(195, 40)")), true), "Missing/incorrect R2 group");
    // Check nets group exists
    assert!(check_svg_element(&svg_string, "g", Some(("id", "nets")), true), "Missing nets group");
    // Check line element exists within the SVG (more specific check might require nested parsing)
    assert!(check_svg_element(&svg_string, "line", None, true), "Missing line element");
    // Check line attributes (fragile, requires parsing attributes within the line tag)
    assert!(check_svg_element(&svg_string, "line", Some(("x1", "120")), true), "Missing/incorrect line x1");
    assert!(check_svg_element(&svg_string, "line", Some(("y1", "40")), true), "Missing/incorrect line y1");
    assert!(check_svg_element(&svg_string, "line", Some(("x2", "150")), true), "Missing/incorrect line x2");
    assert!(check_svg_element(&svg_string, "line", Some(("y2", "40")), true), "Missing/incorrect line y2");

}
