use bhdl_netlist::{Netlist, ModuleKind, PinId, ConnectionPoint};
use svg::{Document, Node};
use svg::node::element::{Group, Line};
use std::collections::HashMap;

// Import the symbols module and its submodules
use crate::symbols; 

// Import layout module
use crate::layout::{Layout, InstanceLayout, Point};

const PADDING: f32 = 30.0; // Padding around symbols
const START_X: f32 = PADDING;
const START_Y: f32 = PADDING + symbols::passives::RESISTOR_SYMBOL_HEIGHT / 2.0; // Base vertical position on resistor height for now
const MIN_WIDTH: f32 = 100.0; 
const MIN_HEIGHT: f32 = 100.0;
const NET_COLOR: &str = "blue";
const NET_STROKE_WIDTH: f32 = 0.8;

pub fn visualize_netlist(netlist: &Netlist) -> Document {
    println!("Generating layout for {} instances, {} nets...", netlist.instances.len(), netlist.nets.len());
    
    let mut document = Document::new();
    let mut layout = Layout::new(); // Create the layout structure

    let mut current_x = START_X;
    let y_pos = START_Y; // Keep symbols on one line for now
    let mut max_x = START_X;
    let mut max_y = MIN_HEIGHT; // Start with minimum height

    let mut has_content = false;
    for (inst_id, instance) in netlist.instances.iter() {
        if let Some(module_def) = netlist.get_module(instance.definition) {
            has_content = true;
            let mut symbol_svg_group = Group::new();
            let mut symbol_width = 0.0;
            let mut symbol_height = 0.0; // Track height for max_y calc
            let mut center_y_offset = 0.0; // Adjust y-position for symbols like ground/vcc
            // Store relative pin coordinates returned by draw functions
            let mut relative_pins: HashMap<String, Point> = HashMap::new(); 

            // Determine which symbol to draw
            // TODO: This matching logic needs to be much more robust!
            let instance_name_lower = instance.name.to_lowercase();
            let module_name_lower = module_def.name.to_lowercase();

            if module_def.kind == ModuleKind::PhysicalComponent {
                if module_name_lower == "resistor" {
                    let (sym, width, p1, p2) = symbols::passives::draw_resistor();
                    symbol_svg_group = sym;
                    symbol_width = width;
                    symbol_height = symbols::passives::RESISTOR_SYMBOL_HEIGHT;
                    relative_pins.insert("1".to_string(), Point::new(p1.0, p1.1)); // Assume pin "1" is left
                    relative_pins.insert("2".to_string(), Point::new(p2.0, p2.1)); // Assume pin "2" is right
                } else if module_name_lower == "capacitor" || module_name_lower == "cap" {
                    let (sym, width, p1, p2) = symbols::passives::draw_capacitor();
                    symbol_svg_group = sym;
                    symbol_width = width;
                    symbol_height = symbols::passives::CAPACITOR_SYMBOL_HEIGHT;
                    relative_pins.insert("1".to_string(), Point::new(p1.0, p1.1));
                    relative_pins.insert("2".to_string(), Point::new(p2.0, p2.1));
                } else if module_name_lower == "gnd" || module_name_lower == "ground" {
                    let (sym, width, p) = symbols::power::draw_ground();
                    symbol_svg_group = sym;
                    symbol_width = width;
                    symbol_height = symbols::PIN_LENGTH + symbols::power::GROUND_LINE_GAP * 2.0; // Approx height
                    center_y_offset = symbols::PIN_LENGTH; // Ground connects from top, shift down
                    relative_pins.insert("GND".to_string(), Point::new(p.0, p.1)); // Assume single pin named "GND"
                } else if module_name_lower == "vcc" || module_name_lower == "vdd" || module_name_lower == "power" {
                    let (sym, width, p) = symbols::power::draw_vcc();
                    symbol_svg_group = sym;
                    symbol_width = width;
                    symbol_height = symbols::PIN_LENGTH + symbols::power::VCC_ARROW_HEIGHT;
                    center_y_offset = -symbols::power::VCC_ARROW_HEIGHT; // VCC connects from bottom, shift up
                    relative_pins.insert("VCC".to_string(), Point::new(p.0, p.1)); // Assume single pin named "VCC"
                } else {
                    // Default to IC box for other physical components
                    let default_ic_width = 60.0;
                    let default_ic_height = 40.0;
                    let (sym, width, height, _) = symbols::ics::draw_ic_box(&module_def.name, default_ic_width, default_ic_height);
                    symbol_svg_group = sym;
                    symbol_width = width;
                    symbol_height = height;
                }
            } else { // For non-PhysicalComponent (Modules, Interfaces etc.) - draw generic box 
                 let default_ic_width = 80.0;
                 let default_ic_height = 50.0;
                 let (sym, width, height, _) = symbols::ics::draw_ic_box(&module_def.name, default_ic_width, default_ic_height);
                 symbol_svg_group = sym;
                 symbol_width = width;
                 symbol_height = height;
            }

            // Calculate absolute position
            let center_x = current_x + symbol_width / 2.0;
            let center_y = y_pos + center_y_offset;
            let instance_center = Point::new(center_x, center_y);

            // --- Create InstanceLayout --- 
            let mut absolute_pin_locations: HashMap<PinId, Point> = HashMap::new();
            // ** VERY IMPORTANT CAVEAT: **
            // This mapping assumes pin names ("1", "2", "GND", "VCC") match the actual Pin names
            // defined in the ModuleDefinition's pins Vec in the netlist, and that the order/meaning
            // matches the symbol drawing function (e.g., "1" is left, "2" is right). 
            // This is fragile and needs a proper solution, like passing pin definitions 
            // to symbol functions or using metadata.
            for pin_def in &module_def.pins {
                if let Some(pin_data) = netlist.get_pin(*pin_def) {
                    if let Some(relative_pos) = relative_pins.get(&pin_data.name) {
                        let absolute_pos = Point::new(
                            center_x + relative_pos.x, 
                            center_y + relative_pos.y
                        );
                        absolute_pin_locations.insert(*pin_def, absolute_pos);
                    }
                }
            }

            let instance_layout = InstanceLayout {
                center: instance_center,
                pin_locations: absolute_pin_locations,
            };
            layout.add_instance(inst_id, instance_layout);
            // --- End InstanceLayout Creation ---

            // Add instance name (adjust y offset based on symbol type)
            let text_y_offset = if center_y_offset >= 0.0 { 
                symbol_height / 2.0 + symbols::TEXT_OFFSET_Y_BELOW // Place below 
            } else { 
                -symbol_height / 2.0 + symbols::TEXT_OFFSET_Y_ABOVE // Place above
            };
            let instance_name_text = symbols::draw_instance_name(&instance.name, text_y_offset);

            // Position the group
            let instance_svg_group = Group::new()
                .set("transform", format!("translate({}, {})", center_x, center_y))
                .add(symbol_svg_group)
                .add(instance_name_text);
            
            document = document.add(instance_svg_group);

            // Update horizontal position and track max dimensions
            current_x += symbol_width + PADDING;
            max_x = max_x.max(current_x);
            max_y = max_y.max(center_y + symbol_height / 2.0 + PADDING + symbols::TEXT_OFFSET_Y_BELOW.abs()); // Consider text below
            max_y = max_y.max(center_y - symbol_height / 2.0 + PADDING + symbols::TEXT_OFFSET_Y_ABOVE.abs()); // Consider text above

        } else {
            eprintln!("Warning: Could not find module definition for instance {:?}", inst_id);
        }
    }

    // --- Draw Nets --- 
    let mut nets_to_draw: Vec<Line> = Vec::new(); // Collect lines first

    for (_net_id, net) in netlist.nets.iter() {
        let mut points_to_connect: Vec<Point> = Vec::new();
        for connection_point in &net.connections {
            match connection_point {
                ConnectionPoint::InstancePin(inst_id, pin_id) => {
                    if let Some(point) = layout.get_pin_location(*inst_id, *pin_id) {
                        points_to_connect.push(point);
                    } else {
                        eprintln!("Warning: Layout location not found for {:?}.{:?}", inst_id, pin_id);
                    }
                }
                ConnectionPoint::InstancePort(_inst_id, _port_id) => {
                    eprintln!("Warning: InstancePort connection drawing not implemented yet.");
                }
                ConnectionPoint::ModulePort(_port_id) => {
                    eprintln!("Warning: ModulePort connection drawing not implemented yet.");
                }
            }
        }

        // Generate lines for this net 
        if points_to_connect.len() >= 2 {
            let first_point = points_to_connect[0];
            for other_point in points_to_connect.iter().skip(1) {
                let line = Line::new()
                    .set("x1", first_point.x)
                    .set("y1", first_point.y)
                    .set("x2", other_point.x)
                    .set("y2", other_point.y);
                nets_to_draw.push(line);
            }
        }
    }
    
    // Only create and add the nets group if there are lines to draw
    if !nets_to_draw.is_empty() {
        let mut nets_group = Group::new()
            .set("id", "nets") 
            .set("stroke", NET_COLOR)
            .set("stroke-width", NET_STROKE_WIDTH)
            .set("fill", "none");
        for line in nets_to_draw {
            nets_group = nets_group.add(line);
        }
        document = document.add(nets_group);
    }
    // --------------- 

    // Set final SVG dimensions
    let final_width = if has_content { max_x } else { MIN_WIDTH };
    let final_height = max_y.max(MIN_HEIGHT); 

    document = document
        .set("width", final_width)
        .set("height", final_height)
        .set("viewBox", (0, 0, final_width, final_height));
    
    document 
}
