#![allow(unused_imports)]
use std::collections::HashMap;

use bhdl_netlist::{InstanceId, ModuleKind, Netlist, PinId, Instance, Pin, NetId};
use svg::node::element::{Circle, Group, Line, Path, Rectangle, Text as SvgText};
use svg::node::Text as SvgTextNode;
use svg::Document;

// Import layout structures
use crate::layout::{BoundingBox, ComponentLayout, NetLayout, Point};
// Import symbol drawing functions correctly
use crate::symbols::passives::{draw_capacitor, draw_resistor};
use crate::symbols::power::{draw_ground, draw_vcc};
use crate::symbols::ics::draw_ic_box;
use crate::symbols::{self, draw_instance_name};
use svg::Node; // Import Node trait for append
use crate::global_router::{CoarseGridGraph, CoarseGridTile}; // Import coarse grid types

// Style Constants (using f64)
const STROKE_WIDTH: f64 = 1.0;
const STROKE_COLOR: &str = "black";
const NET_COLOR: &str = "blue";
const NET_STROKE_WIDTH: f64 = 0.8;
const TEXT_COLOR: &str = "black";
const FONT_SIZE: f64 = 10.0;

// Helper function to create a text element (uses f64)
fn create_text(x: f64, y: f64, text: &str, anchor: &str) -> SvgText {
    SvgText::new(text)
        .set("x", x)
        .set("y", y)
        .set("font-family", "monospace")
        .set("font-size", FONT_SIZE)
        .set("text-anchor", anchor)
        .set("fill", TEXT_COLOR)
}

/// Draws a single component instance based on its layout (uses f64).
fn draw_instance(
    netlist: &Netlist,
    instance_id: InstanceId,
    layout: &ComponentLayout, // Contains the fields directly
) -> Group {
    let instance_data = netlist.instances.get(instance_id).unwrap();
    let module_data = netlist.get_module(instance_data.definition).unwrap();

    // Access fields directly from layout
    let mut group = Group::new().set(
        "transform",
        format!(
            "translate({} {}) rotate({} 0 0)",
            layout.center_x,
            layout.center_y,
            layout.rotation
        ),
    );

    let symbol_svg_group: Group;
    let mut text_y_offset = symbols::TEXT_OFFSET_Y_BELOW as f64;

    match module_data.kind {
        ModuleKind::PhysicalComponent => {
            let name_lower = module_data.name.to_lowercase();
            if name_lower == "resistor" {
                let (sym, _, _, _) = draw_resistor(); symbol_svg_group = sym;
            } else if name_lower == "capacitor" || name_lower == "cap" {
                let (sym, _, _, _) = draw_capacitor(); symbol_svg_group = sym;
            } else if name_lower == "gnd" || name_lower == "ground" {
                let (sym, _, _, _) = draw_ground(); symbol_svg_group = sym;
                text_y_offset = -(layout.height / 2.0) + symbols::TEXT_OFFSET_Y_ABOVE as f64;
            } else if name_lower == "vcc" || name_lower == "vdd" || name_lower == "power" {
                let (sym, _, _, _) = draw_vcc(); symbol_svg_group = sym;
                text_y_offset = (layout.height / 2.0) + symbols::TEXT_OFFSET_Y_BELOW as f64;
            } else {
                // Collect actual Pin structs for draw_ic_box
                let pins_data: Vec<Pin> = module_data
                    .pins.iter().filter_map(|pid| netlist.get_pin(*pid)).cloned().collect();
                let (sym, _, _, _) = draw_ic_box(&module_data.name, layout.width, layout.height, &pins_data);
                symbol_svg_group = sym;
            }
        }
        _ => {
             // Collect actual Pin structs for draw_ic_box
             let pins_data: Vec<Pin> = module_data
                 .pins.iter().filter_map(|pid| netlist.get_pin(*pid)).cloned().collect();
             let (sym, _, _, _) = draw_ic_box(&module_data.name, layout.width, layout.height, &pins_data);
             symbol_svg_group = sym;
        }
    };
    group.append(symbol_svg_group);

    let instance_name_text = draw_instance_name(&instance_data.name, text_y_offset as f32);
    group.append(instance_name_text);

    group
}

/// Draws the nets (wires) connecting component pins (uses f64).
pub fn draw_nets(nets_layout: &HashMap<NetId, NetLayout>) -> Group {
    let mut group = Group::new()
        .set("id", "nets")
        .set("stroke", NET_COLOR)
        .set("stroke-width", NET_STROKE_WIDTH)
        .set("fill", "none");

    for net_layout in nets_layout.values() {
        // Access net_layout.segments directly
        for (p1, p2) in &net_layout.segments {
            let line = Line::new()
                .set("x1", p1.x)
                .set("y1", p1.y)
                .set("x2", p2.x)
                .set("y2", p2.y);
            group.append(line);
        }
    }
    group
}

/// Creates the main SVG document contents (uses f64 BoundingBox).
pub fn draw_netlist_svg(
    netlist: &Netlist,
    component_layouts: &HashMap<InstanceId, ComponentLayout>,
    nets_layout: &HashMap<NetId, NetLayout>,
    bounding_box: &BoundingBox, // Contains fields directly
) -> Document {
    // Access fields directly from bounding_box
    let view_box_str = format!(
        "{} {} {} {}",
        bounding_box.min_x,
        bounding_box.min_y,
        bounding_box.max_x - bounding_box.min_x,
        bounding_box.max_y - bounding_box.min_y
    );

    let mut document = Document::new()
        .set("viewBox", view_box_str)
        // Access fields directly from bounding_box
        .set("width", format!("{}", bounding_box.max_x - bounding_box.min_x))
        .set("height", format!("{}", bounding_box.max_y - bounding_box.min_y));

    let net_group = draw_nets(nets_layout);
    document.append(net_group);

    for (instance_id, layout) in component_layouts {
        let instance_group = draw_instance(netlist, *instance_id, layout);
        document.append(instance_group);
    }

    document
}

// Remove the old visualize_netlist function and its dependencies
// (e.g., SIM_ITERATIONS, ATTRACTION_K etc. if they are no longer used here)
// Also remove the old net drawing logic if it was part of the old visualize_netlist

// --- Add function to draw global routing debug info --- 
pub fn draw_global_routing_debug(
    coarse_grid: &CoarseGridGraph,
    global_paths: &HashMap<bhdl_netlist::NetId, Vec<(usize, usize)>>,
    output: &mut String,
) {
    // --- Draw Coarse Grid Tiles --- 
    output.push_str("  <g id=\"coarse-grid-tiles\" stroke=\"lightblue\" stroke-width=\"0.2\" fill-opacity=\"0.1\">
");
    for tile in &coarse_grid.tiles {
        let x = tile.bounds.min_x;
        let y = tile.bounds.min_y;
        let width = coarse_grid.tile_width;
        let height = coarse_grid.tile_height;
        
        // Congestion determines fill color (e.g., grayscale or heatmap)
        let congestion_level = (tile.congestion * 255.0).round().min(255.0) as u8;
        let fill_color = format!("rgb({}, {}, {})", congestion_level, 255 - congestion_level, 255 - congestion_level); // Example: Red intensity
        // let fill_color = format!("rgb({}, {}, {})", congestion_level, congestion_level, congestion_level); // Grayscale

        output.push_str(&format!(
            "    <rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" fill=\"{}\" />
",
            x, y, width, height, fill_color
        ));
    }
    output.push_str("  </g>
");

    // --- Draw Global Paths --- 
    output.push_str("  <g id=\"global-paths\" stroke=\"purple\" stroke-width=\"0.8\" stroke-dasharray=\"2,2\" fill=\"none\">
");
    for (_net_id, path_tiles) in global_paths {
        if path_tiles.len() < 2 {
            continue;
        }
        let mut path_data = String::from("M");
        for (i, &(c, r)) in path_tiles.iter().enumerate() {
            if let Some(tile) = coarse_grid.get_tile(c, r) {
                let center_x = (tile.bounds.min_x + tile.bounds.max_x) / 2.0;
                let center_y = (tile.bounds.min_y + tile.bounds.max_y) / 2.0;
                if i == 0 {
                    path_data.push_str(&format!(" {:.2} {:.2}", center_x, center_y));
                } else {
                    path_data.push_str(&format!(" L {:.2} {:.2}", center_x, center_y));
                }
            } else {
                 eprintln!("Warning: Tile index ({}, {}) not found in coarse grid during drawing.", c, r);
            }
        }
        output.push_str(&format!("    <path d=\"{}\" />
", path_data));
    }
    output.push_str("  </g>
");

}
