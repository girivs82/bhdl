//! Sub-schematic layout engines for expansion blocks and capacitor banks.
//!
//! These produce sealed `SubSchematic` units with pre-positioned components,
//! pre-routed internal wires, and external port stubs.  The global layout
//! engine sees each sub-schematic as an opaque box with 2-4 ports.

use std::collections::HashMap;
use bhdl_common::ExpansionRecipe;
use crate::types::*;

// ─── Layout constants (match schematic.js SYMBOL_SIZES) ────────────────────

/// Symbol sizes matching the JS renderer's SYMBOL_SIZES table.
/// (bodyW, bodyH, boundW, boundH)
fn symbol_bound(category: &str) -> (f64, f64) {
    match category {
        "resistor"     => (68.0, 44.0),
        "capacitor"    => (34.0, 44.0),
        "inductor"     => (64.0, 44.0),
        "diode"        => (56.0, 44.0),
        "protection"   => (60.0, 44.0),
        "ferrite_bead" => (64.0, 44.0),
        _              => (68.0, 44.0),
    }
}

/// Map component type to rendering category (mirrors extract.rs categorize_component).
fn component_type_to_category(component_type: &str) -> &'static str {
    match component_type {
        "Ind" | "Inductor" => "inductor",
        "Cap" | "Capacitor" => "capacitor",
        "Res" | "Resistor" => "resistor",
        "Diode" => "diode",
        "TVSDiode" => "protection",
        "LED" => "diode",
        _ => "resistor",
    }
}

const INTERNAL_GAP: f64 = 24.0;       // gap between internal components
const SHUNT_DROP: f64 = 60.0;         // vertical drop from main band to shunt
const PADDING: f64 = 20.0;            // padding inside sub-schematic border
const IC_BODY_W: f64 = 90.0;          // IC body width (simplified)
const IC_BODY_H: f64 = 80.0;          // IC body height
const GND_STUB_HEIGHT: f64 = 18.0;    // ground stub below components
const PORT_STUB_LEN: f64 = 14.0;      // external port stub length
const FB_CHAIN_GAP: f64 = 6.0;        // gap between feedback resistors
const CAP_SPACING: f64 = 12.0;        // gap between caps in a bank
const CAP_BANK_MAX_ROW: usize = 4;    // max caps per row before wrapping
const CAP_BANK_ROW_GAP: f64 = 50.0;   // vertical gap between cap bank rows

// ─── Expansion Sub-Schematic ───────────────────────────────────────────────

/// Build a pre-laid-out sub-schematic for an expansion block.
///
/// The layout follows datasheet application-note conventions:
///   IC body on the left, main-path series components extending right,
///   shunt components as vertical drops, feedback divider chain, bootstrap
///   cap above the switching node.
///
/// # Arguments
/// * `recipe` — The expansion recipe describing child components and connections
/// * `instance_attrs` — Attributes on the parent instance (value, schematic_placement on children)
/// * `child_attrs` — Map of child_local_name → attributes from netlist (includes schematic_placement, value, refdes)
/// * `child_refdes` — Map of child_local_name → reference designator
/// * `simulation` — Optional DC simulation data for current/power annotations
/// * `parent_name` — Instance name of the parent (e.g., "buck")
pub fn compute_expansion_sub_schematic(
    recipe: &ExpansionRecipe,
    _instance_attrs: &HashMap<String, String>,
    child_attrs: &HashMap<String, HashMap<String, String>>,
    child_refdes: &HashMap<String, String>,
    simulation: Option<&SimulationAnnotations>,
    parent_name: &str,
) -> SubSchematic {
    // ── 1. Classify children by schematic_placement ──
    let mut main_path_children: Vec<&str> = Vec::new();
    let mut input_shunt: Vec<&str> = Vec::new();
    let mut output_shunt: Vec<&str> = Vec::new();
    let mut switching_shunt: Vec<&str> = Vec::new();
    let mut bootstrap: Vec<&str> = Vec::new();
    let mut feedback_high: Vec<&str> = Vec::new();
    let mut feedback_low: Vec<&str> = Vec::new();
    let mut generic_shunt: Vec<&str> = Vec::new();

    for exp_inst in &recipe.instances {
        let placement = child_attrs.get(&exp_inst.name)
            .and_then(|a| a.get("schematic_placement"))
            .map(|s| s.as_str())
            .unwrap_or("shunt");

        match placement {
            "main_path" => main_path_children.push(&exp_inst.name),
            "input_shunt" => input_shunt.push(&exp_inst.name),
            "output_shunt" => output_shunt.push(&exp_inst.name),
            "switching_shunt" => switching_shunt.push(&exp_inst.name),
            "bootstrap" => bootstrap.push(&exp_inst.name),
            "feedback_high" => feedback_high.push(&exp_inst.name),
            "feedback_low" => feedback_low.push(&exp_inst.name),
            _ => generic_shunt.push(&exp_inst.name),
        }
    }

    // Build a lookup from local name → ExpansionInstance for sizing
    let inst_lookup: HashMap<&str, &bhdl_common::ExpansionInstance> = recipe.instances.iter()
        .map(|ei| (ei.name.as_str(), ei))
        .collect();

    let mut components = Vec::new();
    let mut wires = Vec::new();
    let mut gnd_stubs = Vec::new();
    let mut ports = Vec::new();

    // Track named positions for wire routing
    let mut positions: HashMap<String, (f64, f64, f64, f64)> = HashMap::new(); // name → (x, y, w, h)

    // ── 2. Place IC body ──
    let ic_x = PADDING;
    let ic_y = PADDING + SHUNT_DROP; // leave room for bootstrap above
    positions.insert("__IC__".to_string(), (ic_x, ic_y, IC_BODY_W, IC_BODY_H));

    // IC body is represented as a SubComponent with its own ports
    let ic_ports = build_ic_ports(recipe, IC_BODY_W, IC_BODY_H);
    components.push(SubComponent {
        name: parent_name.to_string(),
        refdes: None, // IC itself gets parent's refdes
        component_type: recipe.entity_name.clone(),
        category: "ic".to_string(),
        x: ic_x, y: ic_y,
        width: IC_BODY_W, height: IC_BODY_H,
        is_vertical: false,
        symbol_variant: None,
        value: None,
        ports: ic_ports,
        sim_current: simulation.and_then(|s| s.instance_currents.get(parent_name).copied()),
        sim_power: simulation.and_then(|s| s.instance_power.get(parent_name).copied()),
    });

    // ── 3. Place main-path series children extending right from IC ──
    // Increase gap after IC when bootstrap cap needs room above the switching junction
    let gap_after_ic = if !bootstrap.is_empty() {
        let (boot_cap_w, _) = symbol_bound("capacitor");
        (boot_cap_w + 8.0).max(INTERNAL_GAP)
    } else {
        INTERNAL_GAP
    };
    let mut cursor_x = ic_x + IC_BODY_W + gap_after_ic;
    let main_band_y = ic_y + IC_BODY_H / 2.0; // center of IC = main band height

    for &child_name in &main_path_children {
        if let Some(exp_inst) = inst_lookup.get(child_name) {
            let cat = component_type_to_category(&exp_inst.component_type);
            let (bw, bh) = symbol_bound(cat);
            let cy = main_band_y - bh / 2.0;

            let child_full_name = format!("{}_{}", parent_name, child_name);
            let value = child_attrs.get(child_name)
                .and_then(|a| a.get("value"))
                .cloned();

            components.push(SubComponent {
                name: child_full_name.clone(),
                refdes: child_refdes.get(child_name).cloned(),
                component_type: exp_inst.component_type.clone(),
                category: cat.to_string(),
                x: cursor_x, y: cy,
                width: bw, height: bh,
                is_vertical: false,
                symbol_variant: None,
                value,
                ports: build_2pin_ports_horizontal(bw, bh),
                sim_current: simulation.and_then(|s| s.instance_currents.get(&child_full_name).copied()),
                sim_power: simulation.and_then(|s| s.instance_power.get(&child_full_name).copied()),
            });
            positions.insert(child_name.to_string(), (cursor_x, cy, bw, bh));
            cursor_x += bw + INTERNAL_GAP;
        }
    }

    // Right edge of main band
    let right_edge = cursor_x;

    // ── 4. Place shunt components as vertical drops ──
    // switching_shunt: below the junction between IC and first main-path child
    let switching_junction_x = ic_x + IC_BODY_W + gap_after_ic / 2.0;
    for &child_name in &switching_shunt {
        place_shunt_child(
            child_name, &inst_lookup, parent_name, child_attrs, child_refdes,
            simulation, switching_junction_x, main_band_y, &mut components,
            &mut gnd_stubs, &mut wires, &mut positions,
        );
    }

    // output_shunt: below the right end (VOUT junction)
    let output_junction_x = if main_path_children.is_empty() {
        ic_x + IC_BODY_W + INTERNAL_GAP / 2.0
    } else {
        // After last main-path child
        let last_name = main_path_children.last().unwrap();
        if let Some(&(x, _, w, _)) = positions.get(*last_name) {
            x + w + INTERNAL_GAP / 2.0
        } else {
            right_edge - INTERNAL_GAP / 2.0
        }
    };
    for &child_name in &output_shunt {
        place_shunt_child(
            child_name, &inst_lookup, parent_name, child_attrs, child_refdes,
            simulation, output_junction_x, main_band_y, &mut components,
            &mut gnd_stubs, &mut wires, &mut positions,
        );
    }

    // input_shunt: below the IC's input side (left), with clearance from IC body
    let input_junction_x = ic_x - INTERNAL_GAP;
    for &child_name in &input_shunt {
        place_shunt_child(
            child_name, &inst_lookup, parent_name, child_attrs, child_refdes,
            simulation, input_junction_x, main_band_y, &mut components,
            &mut gnd_stubs, &mut wires, &mut positions,
        );
    }

    // generic_shunt: below output junction, offset right
    let mut generic_offset = 0.0;
    for &child_name in &generic_shunt {
        place_shunt_child(
            child_name, &inst_lookup, parent_name, child_attrs, child_refdes,
            simulation, output_junction_x + generic_offset, main_band_y,
            &mut components, &mut gnd_stubs, &mut wires, &mut positions,
        );
        generic_offset += 40.0;
    }

    // ── 5. Place feedback divider chain (right side, below main band) ──
    let fb_x = output_junction_x + 30.0;
    let mut fb_cursor_y = main_band_y + 10.0;
    for &child_name in &feedback_high {
        if let Some(exp_inst) = inst_lookup.get(child_name) {
            let cat = component_type_to_category(&exp_inst.component_type);
            let (bw, bh) = symbol_bound(cat);

            let child_full_name = format!("{}_{}", parent_name, child_name);
            let value = child_attrs.get(child_name)
                .and_then(|a| a.get("value"))
                .cloned();

            components.push(SubComponent {
                name: child_full_name.clone(),
                refdes: child_refdes.get(child_name).cloned(),
                component_type: exp_inst.component_type.clone(),
                category: cat.to_string(),
                x: fb_x, y: fb_cursor_y,
                width: bw, height: bh,
                is_vertical: true,
                symbol_variant: None,
                value,
                ports: build_2pin_ports_vertical(bw, bh),
                sim_current: simulation.and_then(|s| s.instance_currents.get(&child_full_name).copied()),
                sim_power: simulation.and_then(|s| s.instance_power.get(&child_full_name).copied()),
            });
            positions.insert(child_name.to_string(), (fb_x, fb_cursor_y, bw, bh));
            fb_cursor_y += bh + FB_CHAIN_GAP;
        }
    }
    for &child_name in &feedback_low {
        if let Some(exp_inst) = inst_lookup.get(child_name) {
            let cat = component_type_to_category(&exp_inst.component_type);
            let (bw, bh) = symbol_bound(cat);

            let child_full_name = format!("{}_{}", parent_name, child_name);
            let value = child_attrs.get(child_name)
                .and_then(|a| a.get("value"))
                .cloned();

            components.push(SubComponent {
                name: child_full_name.clone(),
                refdes: child_refdes.get(child_name).cloned(),
                component_type: exp_inst.component_type.clone(),
                category: cat.to_string(),
                x: fb_x, y: fb_cursor_y,
                width: bw, height: bh,
                is_vertical: true,
                symbol_variant: None,
                value,
                ports: build_2pin_ports_vertical(bw, bh),
                sim_current: simulation.and_then(|s| s.instance_currents.get(&child_full_name).copied()),
                sim_power: simulation.and_then(|s| s.instance_power.get(&child_full_name).copied()),
            });
            positions.insert(child_name.to_string(), (fb_x, fb_cursor_y, bw, bh));
            fb_cursor_y += bh + GND_STUB_HEIGHT;
            gnd_stubs.push(SubGndStub { x: fb_x + bw / 2.0, y: fb_cursor_y });
        }
    }

    // ── 6. Place bootstrap cap (above switching junction) ──
    for &child_name in &bootstrap {
        if let Some(exp_inst) = inst_lookup.get(child_name) {
            let cat = component_type_to_category(&exp_inst.component_type);
            let (bw, bh) = symbol_bound(cat);
            let boot_x = switching_junction_x - bw / 2.0;
            let boot_y = ic_y - bh - INTERNAL_GAP / 2.0;

            let child_full_name = format!("{}_{}", parent_name, child_name);
            let value = child_attrs.get(child_name)
                .and_then(|a| a.get("value"))
                .cloned();

            components.push(SubComponent {
                name: child_full_name.clone(),
                refdes: child_refdes.get(child_name).cloned(),
                component_type: exp_inst.component_type.clone(),
                category: cat.to_string(),
                x: boot_x, y: boot_y,
                width: bw, height: bh,
                is_vertical: true,
                symbol_variant: None,
                value,
                ports: build_2pin_ports_vertical(bw, bh),
                sim_current: simulation.and_then(|s| s.instance_currents.get(&child_full_name).copied()),
                sim_power: simulation.and_then(|s| s.instance_power.get(&child_full_name).copied()),
            });
            positions.insert(child_name.to_string(), (boot_x, boot_y, bw, bh));
        }
    }

    // ── 7. Compute bounding box ──
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for comp in &components {
        min_x = min_x.min(comp.x);
        min_y = min_y.min(comp.y);
        max_x = max_x.max(comp.x + comp.width);
        max_y = max_y.max(comp.y + comp.height);
    }
    for stub in &gnd_stubs {
        max_y = max_y.max(stub.y + GND_STUB_HEIGHT);
    }

    let bbox_w = (max_x - min_x + PADDING * 2.0).max(200.0);
    let bbox_h = (max_y - min_y + PADDING * 2.0 + GND_STUB_HEIGHT).max(150.0);

    // Shift all component positions so they are relative to bbox origin (0,0)
    let offset_x = min_x - PADDING;
    let offset_y = min_y - PADDING;
    for comp in &mut components {
        comp.x -= offset_x;
        comp.y -= offset_y;
    }
    for stub in &mut gnd_stubs {
        stub.x -= offset_x;
        stub.y -= offset_y;
    }
    // Shift existing wires (shunt vertical drops from place_shunt_child)
    for wire in &mut wires {
        for seg in &mut wire.segments {
            seg.0 -= offset_x;
            seg.1 -= offset_y;
            seg.2 -= offset_x;
            seg.3 -= offset_y;
        }
    }

    // ── 7b. Route internal wires (in shifted coordinate space) ──
    // Now that we know the bbox dimensions and offset, generate wires that
    // connect to the bbox edges (ports) and between internal components.
    let main_band_y_rel = main_band_y - offset_y;
    {
        let ic_shifted_x = ic_x - offset_x;
        let ic_shifted_right = ic_shifted_x + IC_BODY_W;

        // VIN input wire: left bbox edge (x=0) → IC body left
        wires.push(SubWire {
            segments: vec![(0.0, main_band_y_rel, ic_shifted_x, main_band_y_rel)],
            net_name: String::new(), is_power: true, voltage: None,
        });

        // Main-band horizontal wires through main-path children
        if !main_path_children.is_empty() {
            // IC right → first main-path child left
            let first = main_path_children[0];
            if let Some(&(fx, _, _, _)) = positions.get(first) {
                let fx_shifted = fx - offset_x;
                wires.push(SubWire {
                    segments: vec![(ic_shifted_right, main_band_y_rel, fx_shifted, main_band_y_rel)],
                    net_name: String::new(), is_power: true, voltage: None,
                });
            }
            // Last main-path child right → right bbox edge (VOUT)
            let last = main_path_children.last().unwrap();
            if let Some(&(lx, _, lw, _)) = positions.get(*last) {
                let lx_shifted_right = (lx + lw) - offset_x;
                wires.push(SubWire {
                    segments: vec![(lx_shifted_right, main_band_y_rel, bbox_w, main_band_y_rel)],
                    net_name: String::new(), is_power: true, voltage: None,
                });
            }
        } else {
            // No main-path children: IC right → right bbox edge
            wires.push(SubWire {
                segments: vec![(ic_shifted_right, main_band_y_rel, bbox_w, main_band_y_rel)],
                net_name: String::new(), is_power: true, voltage: None,
            });
        }

        // Bootstrap cap: two wires — pin 2 (bottom) → SW node, pin 1 (top) ← IC BOOT pin
        for &child_name in &bootstrap {
            if let Some(&(bx, by, bw, bh)) = positions.get(child_name) {
                let cap_center_x = (bx + bw / 2.0) - offset_x;
                let cap_bottom_y = (by + bh) - offset_y;
                let cap_top_y = by - offset_y;

                // Pin 2 (bottom) → main band (SW node)
                wires.push(SubWire {
                    segments: vec![(cap_center_x, cap_bottom_y, cap_center_x, main_band_y_rel)],
                    net_name: String::new(), is_power: false, voltage: None,
                });

                // Pin 1 (top) ← IC BOOT pin: L-route from IC top-right up to cap level
                let ic_right_shifted = ic_shifted_right;
                let ic_top_shifted = ic_y - offset_y;
                wires.push(SubWire {
                    segments: vec![
                        (ic_right_shifted, ic_top_shifted, ic_right_shifted, cap_top_y),
                        (ic_right_shifted, cap_top_y, cap_center_x, cap_top_y),
                    ],
                    net_name: String::new(), is_power: false, voltage: None,
                });
            }
        }

        // Feedback chain: horizontal from output junction, then vertical down chain
        if !feedback_high.is_empty() {
            let first_fb = feedback_high[0];
            if let Some(&(fx, fy, fw, _)) = positions.get(first_fb) {
                let fb_center_x = (fx + fw / 2.0) - offset_x;
                let out_junc_shifted = output_junction_x - offset_x;
                let fb_top_y = fy - offset_y;
                wires.push(SubWire {
                    segments: vec![
                        (out_junc_shifted, main_band_y_rel, fb_center_x, main_band_y_rel),
                        (fb_center_x, main_band_y_rel, fb_center_x, fb_top_y),
                    ],
                    net_name: String::new(), is_power: false, voltage: None,
                });
            }
            // Wire between feedback_high bottom and feedback_low top
            if !feedback_low.is_empty() {
                let last_high = feedback_high.last().unwrap();
                let first_low = feedback_low[0];
                if let Some(&(hx, hy, hw, hh)) = positions.get(*last_high) {
                    if let Some(&(_, ly, _, _)) = positions.get(first_low) {
                        let cx = (hx + hw / 2.0) - offset_x;
                        let h_bottom = (hy + hh) - offset_y;
                        let l_top = ly - offset_y;
                        wires.push(SubWire {
                            segments: vec![(cx, h_bottom, cx, l_top)],
                            net_name: String::new(), is_power: false, voltage: None,
                        });
                    }
                }
            }
        }
    }

    // ── 8. Define external ports at main band Y (not bbox center) ──
    // VIN: left edge at main band Y
    ports.push(SubPort {
        name: "VIN".to_string(),
        side: "left".to_string(),
        x: 0.0, y: main_band_y_rel,
        pin_type: "power".to_string(),
    });
    // VOUT: right edge at main band Y
    ports.push(SubPort {
        name: "VOUT".to_string(),
        side: "right".to_string(),
        x: bbox_w, y: main_band_y_rel,
        pin_type: "power".to_string(),
    });
    // GND: bottom center
    ports.push(SubPort {
        name: "GND".to_string(),
        side: "bottom".to_string(),
        x: bbox_w / 2.0, y: bbox_h,
        pin_type: "ground".to_string(),
    });
    // EN: left edge, above VIN
    if recipe.pin_info.contains_key("EN") {
        ports.push(SubPort {
            name: "EN".to_string(),
            side: "left".to_string(),
            x: 0.0, y: main_band_y_rel - 24.0,
            pin_type: "signal".to_string(),
        });
    }

    SubSchematic {
        kind: SubSchematicKind::Expansion,
        label: Some(recipe.entity_name.clone()),
        width: bbox_w,
        height: bbox_h,
        ports,
        components,
        wires,
        gnd_stubs,
    }
}

/// Place a shunt child as a vertical drop below a junction point.
fn place_shunt_child(
    child_name: &str,
    inst_lookup: &HashMap<&str, &bhdl_common::ExpansionInstance>,
    parent_name: &str,
    child_attrs: &HashMap<String, HashMap<String, String>>,
    child_refdes: &HashMap<String, String>,
    simulation: Option<&SimulationAnnotations>,
    junction_x: f64,
    junction_y: f64,
    components: &mut Vec<SubComponent>,
    gnd_stubs: &mut Vec<SubGndStub>,
    wires: &mut Vec<SubWire>,
    positions: &mut HashMap<String, (f64, f64, f64, f64)>,
) {
    if let Some(exp_inst) = inst_lookup.get(child_name) {
        let cat = component_type_to_category(&exp_inst.component_type);
        let (bw, bh) = symbol_bound(cat);
        let cx = junction_x - bw / 2.0;
        let cy = junction_y + SHUNT_DROP;

        let child_full_name = format!("{}_{}", parent_name, child_name);
        let value = child_attrs.get(child_name)
            .and_then(|a| a.get("value"))
            .cloned();

        components.push(SubComponent {
            name: child_full_name.clone(),
            refdes: child_refdes.get(child_name).cloned(),
            component_type: exp_inst.component_type.clone(),
            category: cat.to_string(),
            x: cx, y: cy,
            width: bw, height: bh,
            is_vertical: true,
            symbol_variant: None,
            value,
            ports: build_2pin_ports_vertical(bw, bh),
            sim_current: simulation.and_then(|s| s.instance_currents.get(&child_full_name).copied()),
            sim_power: simulation.and_then(|s| s.instance_power.get(&child_full_name).copied()),
        });

        // Vertical wire from junction to shunt top
        wires.push(SubWire {
            segments: vec![(junction_x, junction_y, junction_x, cy)],
            net_name: String::new(),
            is_power: false,
            voltage: None,
        });

        // GND stub below shunt
        gnd_stubs.push(SubGndStub { x: junction_x, y: cy + bh });

        positions.insert(child_name.to_string(), (cx, cy, bw, bh));
    }
}

/// Build IC body ports from the recipe's pin_info.
fn build_ic_ports(recipe: &ExpansionRecipe, w: f64, h: f64) -> Vec<SubComponentPort> {
    let mut ports = Vec::new();
    let mut left_idx = 0;
    let mut right_idx = 0;
    let port_spacing = 20.0;

    for (pin_name, (pin_type, direction)) in &recipe.pin_info {
        let upper = pin_name.to_uppercase();
        if upper == "GND" {
            ports.push(SubComponentPort {
                name: pin_name.clone(),
                x: w / 2.0, y: h,
                direction: "ground".to_string(),
            });
        } else if direction == "in" || upper == "VIN" || upper == "EN" || upper == "BOOT" {
            let y = port_spacing + left_idx as f64 * port_spacing;
            ports.push(SubComponentPort {
                name: pin_name.clone(),
                x: 0.0, y,
                direction: "in".to_string(),
            });
            left_idx += 1;
        } else {
            let y = port_spacing + right_idx as f64 * port_spacing;
            ports.push(SubComponentPort {
                name: pin_name.clone(),
                x: w, y,
                direction: "out".to_string(),
            });
            right_idx += 1;
        }
    }
    ports
}

/// Build ports for a horizontal 2-pin component (pin 1 left, pin 2 right).
fn build_2pin_ports_horizontal(w: f64, h: f64) -> Vec<SubComponentPort> {
    vec![
        SubComponentPort { name: "1".to_string(), x: 0.0, y: h / 2.0, direction: "in".to_string() },
        SubComponentPort { name: "2".to_string(), x: w, y: h / 2.0, direction: "out".to_string() },
    ]
}

/// Build ports for a vertical 2-pin component (pin 1 top, pin 2 bottom).
fn build_2pin_ports_vertical(w: f64, h: f64) -> Vec<SubComponentPort> {
    vec![
        SubComponentPort { name: "1".to_string(), x: w / 2.0, y: 0.0, direction: "in".to_string() },
        SubComponentPort { name: "2".to_string(), x: w / 2.0, y: h, direction: "out".to_string() },
    ]
}

// ─── Cap Bank Sub-Schematic ────────────────────────────────────────────────

/// Description of a single capacitor in a bank group.
pub struct CapBankMember {
    /// Instance name (e.g., "c_in", "c_in_2")
    pub name: String,
    /// Display value (e.g., "47uF", "100nF")
    pub value: Option<String>,
    /// Reference designator (e.g., "C3")
    pub refdes: Option<String>,
    /// Is this the bank parent (original unsplit cap)?
    pub is_parent: bool,
    /// Bank parent name (None for parent itself)
    pub bank_parent: Option<String>,
}

/// Build a pre-laid-out sub-schematic for a capacitor bank group.
///
/// Caps are placed side-by-side in rows, vertically oriented (shunt),
/// with a signal port on top and GND port on bottom.  Multi-row wrapping
/// uses boustrophedon (serpentine) layout with L-bend bus routing.
///
/// # Arguments
/// * `caps` — Ordered list of caps in this bank (parent first, then children)
/// * `intent_label` — Stage name: "input_filtering", "output_filtering", etc.
/// * `simulation` — Optional DC simulation data
pub fn compute_cap_bank_sub_schematic(
    caps: &[CapBankMember],
    intent_label: &str,
    simulation: Option<&SimulationAnnotations>,
) -> SubSchematic {
    if caps.is_empty() {
        return SubSchematic {
            kind: SubSchematicKind::CapBank,
            label: Some(intent_label.to_string()),
            width: 40.0,
            height: 60.0,
            ports: vec![],
            components: vec![],
            wires: vec![],
            gnd_stubs: vec![],
        };
    }

    let (cap_w, cap_h) = symbol_bound("capacitor");
    let cap_stride = cap_w + CAP_SPACING;

    // Determine row structure
    let n = caps.len();
    let n_rows = if n <= CAP_BANK_MAX_ROW { 1 } else { (n + CAP_BANK_MAX_ROW - 1) / CAP_BANK_MAX_ROW };
    let caps_per_row = if n_rows == 1 { n } else { CAP_BANK_MAX_ROW };

    let mut components = Vec::new();
    let mut wires = Vec::new();
    let mut gnd_stubs = Vec::new();

    let row_width = caps_per_row as f64 * cap_stride - CAP_SPACING;
    let total_height = n_rows as f64 * (cap_h + CAP_BANK_ROW_GAP) - CAP_BANK_ROW_GAP;

    // Place caps
    for (i, cap) in caps.iter().enumerate() {
        let row = i / caps_per_row;
        let col_in_row = i % caps_per_row;

        // Boustrophedon: even rows L→R, odd rows R→L
        let col = if row % 2 == 0 { col_in_row } else { caps_per_row - 1 - col_in_row };

        let cx = PADDING + col as f64 * cap_stride;
        let cy = PADDING + row as f64 * (cap_h + CAP_BANK_ROW_GAP);

        components.push(SubComponent {
            name: cap.name.clone(),
            refdes: cap.refdes.clone(),
            component_type: "Cap".to_string(),
            category: "capacitor".to_string(),
            x: cx, y: cy,
            width: cap_w, height: cap_h,
            is_vertical: true,
            symbol_variant: None,
            value: cap.value.clone(),
            ports: build_2pin_ports_vertical(cap_w, cap_h),
            sim_current: simulation.and_then(|s| s.instance_currents.get(&cap.name).copied()),
            sim_power: simulation.and_then(|s| s.instance_power.get(&cap.name).copied()),
        });

        // GND stub at bottom of each cap
        gnd_stubs.push(SubGndStub {
            x: cx + cap_w / 2.0,
            y: cy + cap_h,
        });
    }

    // Internal bus wires: horizontal bus connecting all cap tops
    if n > 1 {
        for row in 0..n_rows {
            let row_start = row * caps_per_row;
            let row_end = ((row + 1) * caps_per_row).min(n);
            if row_end - row_start < 2 { continue; }

            let row_y = PADDING + row as f64 * (cap_h + CAP_BANK_ROW_GAP);
            let bus_y = row_y; // top of caps

            // Horizontal bus across this row
            let first_col = if row % 2 == 0 { 0 } else { caps_per_row - 1 - ((row_end - row_start) - 1) };
            let last_col = if row % 2 == 0 { row_end - row_start - 1 } else { caps_per_row - 1 };
            let x1 = PADDING + first_col.min(last_col) as f64 * cap_stride + cap_w / 2.0;
            let x2 = PADDING + first_col.max(last_col) as f64 * cap_stride + cap_w / 2.0;

            wires.push(SubWire {
                segments: vec![(x1, bus_y, x2, bus_y)],
                net_name: intent_label.to_string(),
                is_power: true,
                voltage: None,
            });

            // L-bend to next row (if not last row)
            if row + 1 < n_rows {
                let next_row_y = PADDING + (row + 1) as f64 * (cap_h + CAP_BANK_ROW_GAP);
                // L-bend goes from end of current row down to start of next row
                let bend_x = if row % 2 == 0 { x2 + CAP_SPACING } else { x1 - CAP_SPACING };
                wires.push(SubWire {
                    segments: vec![
                        (if row % 2 == 0 { x2 } else { x1 }, bus_y, bend_x, bus_y),
                        (bend_x, bus_y, bend_x, next_row_y),
                    ],
                    net_name: intent_label.to_string(),
                    is_power: true,
                    voltage: None,
                });
            }
        }
    }

    // Signal stub: vertical wire from signal port at top of bbox down to bus level
    {
        let bus_y = PADDING; // top of caps = bus level (first row)
        let first_cap_center = if !components.is_empty() {
            components[0].x + cap_w / 2.0
        } else {
            PADDING + cap_w / 2.0
        };
        // Single cap: wire to its center; multi-cap: wire to midpoint of bus
        let stub_x = if n == 1 {
            first_cap_center
        } else {
            (row_width + PADDING * 2.0) / 2.0
        };
        wires.push(SubWire {
            segments: vec![(stub_x, 0.0, stub_x, bus_y)],
            net_name: intent_label.to_string(),
            is_power: true,
            voltage: None,
        });
    }

    // Bounding box
    let bbox_w = row_width + PADDING * 2.0;
    let bbox_h = total_height + PADDING * 2.0 + GND_STUB_HEIGHT;

    // External ports
    let mid_x = bbox_w / 2.0;
    let ports = vec![
        SubPort {
            name: "signal".to_string(),
            side: "top".to_string(),
            x: mid_x, y: 0.0,
            pin_type: "power".to_string(),
        },
        SubPort {
            name: "GND".to_string(),
            side: "bottom".to_string(),
            x: mid_x, y: bbox_h,
            pin_type: "ground".to_string(),
        },
    ];

    SubSchematic {
        kind: SubSchematicKind::CapBank,
        label: Some(intent_label.to_string()),
        width: bbox_w,
        height: bbox_h,
        ports,
        components,
        wires,
        gnd_stubs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_common::{ExpansionInstance, ExpansionConnection, ExpansionEndpoint};

    fn make_tps54331_recipe() -> ExpansionRecipe {
        let mut recipe = ExpansionRecipe::new("TPS54331".to_string());
        recipe.internal_nets = vec!["sw".to_string()];
        recipe.pin_info = [
            ("VIN".to_string(), ("power".to_string(), "in".to_string())),
            ("VOUT".to_string(), ("power".to_string(), "out".to_string())),
            ("SW".to_string(), ("switch".to_string(), "out".to_string())),
            ("FB".to_string(), ("feedback".to_string(), "in".to_string())),
            ("BOOT".to_string(), ("signal".to_string(), "inout".to_string())),
            ("EN".to_string(), ("signal".to_string(), "in".to_string())),
            ("GND".to_string(), ("ground".to_string(), "in".to_string())),
        ].into_iter().collect();

        recipe.instances = vec![
            ExpansionInstance { gate: None, layout_intents: Vec::new(), name: "L_out".to_string(), component_type: "Ind".to_string(), params: vec!["10µH".to_string()], attributes: HashMap::new() },
            ExpansionInstance { gate: None, layout_intents: Vec::new(), name: "D_catch".to_string(), component_type: "Diode".to_string(), params: vec!["0.45V".to_string()], attributes: HashMap::new() },
            ExpansionInstance { gate: None, layout_intents: Vec::new(), name: "C_out".to_string(), component_type: "Cap".to_string(), params: vec!["22µF".to_string()], attributes: HashMap::new() },
            ExpansionInstance { gate: None, layout_intents: Vec::new(), name: "R_top".to_string(), component_type: "Res".to_string(), params: vec!["31.6kΩ".to_string()], attributes: HashMap::new() },
            ExpansionInstance { gate: None, layout_intents: Vec::new(), name: "R_bot".to_string(), component_type: "Res".to_string(), params: vec!["10kΩ".to_string()], attributes: HashMap::new() },
            ExpansionInstance { gate: None, layout_intents: Vec::new(), name: "C_boot".to_string(), component_type: "Cap".to_string(), params: vec!["100nF".to_string()], attributes: HashMap::new() },
        ];

        recipe.connections = vec![
            ExpansionConnection { gate: None, from: ExpansionEndpoint::ParentPin("SW".to_string()), to: ExpansionEndpoint::InstancePin("L_out".to_string(), "1".to_string()) },
            ExpansionConnection { gate: None, from: ExpansionEndpoint::InstancePin("L_out".to_string(), "2".to_string()), to: ExpansionEndpoint::ParentPin("VOUT".to_string()) },
            ExpansionConnection { gate: None, from: ExpansionEndpoint::ParentPin("GND".to_string()), to: ExpansionEndpoint::InstancePin("D_catch".to_string(), "A".to_string()) },
            ExpansionConnection { gate: None, from: ExpansionEndpoint::InstancePin("D_catch".to_string(), "K".to_string()), to: ExpansionEndpoint::ParentPin("SW".to_string()) },
            ExpansionConnection { gate: None, from: ExpansionEndpoint::ParentPin("VOUT".to_string()), to: ExpansionEndpoint::InstancePin("C_out".to_string(), "1".to_string()) },
            ExpansionConnection { gate: None, from: ExpansionEndpoint::InstancePin("C_out".to_string(), "2".to_string()), to: ExpansionEndpoint::ParentPin("GND".to_string()) },
            ExpansionConnection { gate: None, from: ExpansionEndpoint::ParentPin("VOUT".to_string()), to: ExpansionEndpoint::InstancePin("R_top".to_string(), "1".to_string()) },
            ExpansionConnection { gate: None, from: ExpansionEndpoint::InstancePin("R_top".to_string(), "2".to_string()), to: ExpansionEndpoint::ParentPin("FB".to_string()) },
            ExpansionConnection { gate: None, from: ExpansionEndpoint::ParentPin("FB".to_string()), to: ExpansionEndpoint::InstancePin("R_bot".to_string(), "1".to_string()) },
            ExpansionConnection { gate: None, from: ExpansionEndpoint::InstancePin("R_bot".to_string(), "2".to_string()), to: ExpansionEndpoint::ParentPin("GND".to_string()) },
            ExpansionConnection { gate: None, from: ExpansionEndpoint::ParentPin("BOOT".to_string()), to: ExpansionEndpoint::InstancePin("C_boot".to_string(), "1".to_string()) },
            ExpansionConnection { gate: None, from: ExpansionEndpoint::InstancePin("C_boot".to_string(), "2".to_string()), to: ExpansionEndpoint::ParentPin("SW".to_string()) },
        ];

        recipe
    }

    #[test]
    fn test_expansion_sub_schematic_basic() {
        let recipe = make_tps54331_recipe();
        let child_attrs: HashMap<String, HashMap<String, String>> = [
            ("L_out", [("schematic_placement", "main_path"), ("value", "10µH")]),
            ("D_catch", [("schematic_placement", "switching_shunt"), ("value", "0.45V")]),
            ("C_out", [("schematic_placement", "output_shunt"), ("value", "22µF")]),
            ("R_top", [("schematic_placement", "feedback_high"), ("value", "31.6kΩ")]),
            ("R_bot", [("schematic_placement", "feedback_low"), ("value", "10kΩ")]),
            ("C_boot", [("schematic_placement", "bootstrap"), ("value", "100nF")]),
        ].iter().map(|(name, attrs)| {
            (name.to_string(), attrs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
        }).collect();

        let sub = compute_expansion_sub_schematic(
            &recipe, &HashMap::new(), &child_attrs, &HashMap::new(), None, "buck",
        );

        assert_eq!(sub.kind, SubSchematicKind::Expansion);
        assert_eq!(sub.label, Some("TPS54331".to_string()));
        // IC body + 6 children = 7 components
        assert_eq!(sub.components.len(), 7, "Expected 7 components (IC + 6 children)");
        // VIN, VOUT, GND, EN ports
        assert!(sub.ports.len() >= 3, "Expected at least 3 external ports");
        assert!(sub.width > 150.0, "Width should be substantial: {}", sub.width);
        assert!(sub.height > 100.0, "Height should be substantial: {}", sub.height);
    }

    #[test]
    fn test_cap_bank_sub_schematic_single_row() {
        let caps = vec![
            CapBankMember { name: "c_in".into(), value: Some("47uF".into()), refdes: Some("C1".into()), is_parent: true, bank_parent: None },
            CapBankMember { name: "c_in_bypass".into(), value: Some("100nF".into()), refdes: Some("C2".into()), is_parent: false, bank_parent: Some("c_in".into()) },
        ];

        let sub = compute_cap_bank_sub_schematic(&caps, "input_filtering", None);

        assert_eq!(sub.kind, SubSchematicKind::CapBank);
        assert_eq!(sub.label, Some("input_filtering".to_string()));
        assert_eq!(sub.components.len(), 2);
        assert_eq!(sub.ports.len(), 2); // signal top, GND bottom
        assert_eq!(sub.gnd_stubs.len(), 2); // one per cap
    }

    #[test]
    fn test_cap_bank_sub_schematic_multi_row() {
        let caps: Vec<CapBankMember> = (0..6).map(|i| {
            CapBankMember {
                name: format!("c_{}", i),
                value: Some("4.7uF".into()),
                refdes: Some(format!("C{}", i + 1)),
                is_parent: i == 0,
                bank_parent: if i > 0 { Some("c_0".into()) } else { None },
            }
        }).collect();

        let sub = compute_cap_bank_sub_schematic(&caps, "output_filtering", None);

        assert_eq!(sub.components.len(), 6);
        // Should have multi-row layout: 4 + 2
        assert!(sub.height > 100.0, "Multi-row should be taller");
        // Should have internal bus wires
        assert!(!sub.wires.is_empty(), "Multi-row should have bus wires");
    }
}
