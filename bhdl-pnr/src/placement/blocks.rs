//! Hierarchical block-based placement.
//!
//! Like FPGA CLB packing: related components are absorbed into blocks,
//! blocks are placed on the board, then components are detailed within blocks.
//!
//! Internal block layout is netlist-driven:
//! - Children placed on the side of the IC where their connecting pin is
//! - Children oriented so connecting pins face the IC
//! - Shared-net components aligned on their shared pin
//!
//! This follows the vendor app note approach: place components relative to
//! the IC pins they connect to.

use crate::det::{HashMap, HashSet};
use bhdl_common::PlacementRecipe;
use crate::types::*;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Side { Left, Right, Top, Bottom }

struct ChildPlacement {
    local_idx: usize,
    side: Side,
    ic_pin_dx: f64,
    ic_pin_dy: f64,
    child_pin_dx: f64,
    child_pin_dy: f64,
}

/// A placement block containing one or more components.
#[derive(Debug, Clone)]
pub struct PlacementBlock {
    pub name: String,
    pub members: Vec<usize>,
    pub anchor: Option<usize>,
    pub width: f64,
    pub height: f64,
    pub x: f64,
    pub y: f64,
    pub internal_positions: Vec<(f64, f64, f64)>,
}

/// Form blocks from the board's functional groups and standalone components.
/// If `placement_recipes` contains a recipe for an expansion block's entity type,
/// the exact datasheet coordinates are used instead of the netlist-driven heuristic.
pub fn form_blocks(
    board: &Board,
    placement_recipes: &std::collections::BTreeMap<String, PlacementRecipe>,
) -> Vec<PlacementBlock> {
    let mut blocks = Vec::new();
    let mut assigned: HashSet<usize> = HashSet::default();

    let comp_id_to_idx: HashMap<ComponentId, usize> = board.components.iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();

    // 1. Create blocks from functional groups
    for group in &board.groups {
        let member_indices: Vec<usize> = group.members.iter()
            .filter_map(|id| comp_id_to_idx.get(id).copied())
            .collect();

        if member_indices.is_empty() { continue; }

        let anchor = group.parent
            .and_then(|pid| comp_id_to_idx.get(&pid).copied())
            .or_else(|| {
                member_indices.iter().copied().max_by(|&a, &b| {
                    let area_a = board.components[a].width_mm * board.components[a].height_mm;
                    let area_b = board.components[b].width_mm * board.components[b].height_mm;
                    area_a.partial_cmp(&area_b).unwrap()
                })
            });

        for &idx in &member_indices {
            assigned.insert(idx);
        }

        let mut block = PlacementBlock {
            name: group.name.clone(),
            members: member_indices,
            anchor,
            width: 0.0,
            height: 0.0,
            x: 0.0,
            y: 0.0,
            internal_positions: Vec::new(),
        };

        // Try datasheet placement recipe first, fall back to netlist-driven
        let anchor_name = &board.components[anchor.unwrap_or(block.members[0])].name;
        if let Some(recipe) = find_recipe_for_block(anchor_name, board, &block.members, placement_recipes) {
            apply_placement_recipe(&mut block, board, recipe);
            log::info!("  Block '{}': using datasheet layout ({})",
                block.name, recipe.reference.as_deref().unwrap_or(""));
        } else {
            layout_block_netlist_driven(&mut block, board);
        }
        blocks.push(block);
    }

    // 2. Singleton blocks for unassigned components
    for (i, comp) in board.components.iter().enumerate() {
        if assigned.contains(&i) { continue; }
        blocks.push(PlacementBlock {
            name: comp.name.clone(),
            members: vec![i],
            anchor: Some(i),
            width: comp.width_mm + 2.0,
            height: comp.height_mm + 2.0,
            x: 0.0,
            y: 0.0,
            internal_positions: vec![(0.0, 0.0, 0.0)],
        });
    }

    blocks
}

/// Internal block layout — tries datasheet pattern first, falls back to netlist-driven.
///
/// Datasheet pattern: reads `layout_<child>_dx/dy/rot` attributes from the
/// anchor component (set in stdlib from vendor datasheet layout figures).
/// These are exact coordinates relative to IC center.
///
/// Netlist-driven fallback: infers placement from pin connections.
fn layout_block_netlist_driven(block: &mut PlacementBlock, board: &Board) {
    let n = block.members.len();
    if n == 0 { return; }

    if n == 1 {
        let comp = &board.components[block.members[0]];
        block.width = comp.width_mm + 2.0;
        block.height = comp.height_mm + 2.0;
        block.internal_positions = vec![(0.0, 0.0, 0.0)];
        return;
    }

    let anchor_idx = block.anchor.unwrap_or(block.members[0]);
    let anchor_local = block.members.iter().position(|&m| m == anchor_idx).unwrap_or(0);
    let anchor_comp = &board.components[anchor_idx];
    let ic_w = anchor_comp.width_mm;
    let ic_h = anchor_comp.height_mm;

    // Build a map: for each child, which IC pin(s) does it connect to?
    let comp_id_to_idx: HashMap<ComponentId, usize> = board.components.iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();

    // For each child, determine which side of the IC it should be placed on
    // by looking at which IC pin it shares a net with
    let mut child_placements: Vec<ChildPlacement> = Vec::new();

    for (local_i, &global_i) in block.members.iter().enumerate() {
        if global_i == anchor_idx { continue; }

        let child_comp = &board.components[global_i];

        // Find a net connecting this child to the anchor IC
        let mut best_ic_pin: Option<(f64, f64)> = None;
        let mut best_child_pin: Option<(f64, f64)> = None;

        for net in &board.nets {
            let has_anchor = net.pins.iter().any(|(cid, _)| {
                comp_id_to_idx.get(cid).copied() == Some(anchor_idx)
            });
            let has_child = net.pins.iter().any(|(cid, _)| {
                comp_id_to_idx.get(cid).copied() == Some(global_i)
            });

            if has_anchor && has_child {
                // Find the IC pin and child pin on this net
                for &(cid, pid) in &net.pins {
                    if let Some(&ci) = comp_id_to_idx.get(&cid) {
                        if ci == anchor_idx {
                            if let Some(pin) = anchor_comp.pins.iter().find(|p| p.pin_id == pid) {
                                // Skip GND pins for side determination (GND is everywhere)
                                if pin.name != "GND" && pin.name != "2" || best_ic_pin.is_none() {
                                    best_ic_pin = Some((pin.dx, pin.dy));
                                }
                            }
                        }
                        if ci == global_i {
                            if let Some(pin) = child_comp.pins.iter().find(|p| p.pin_id == pid) {
                                best_child_pin = Some((pin.dx, pin.dy));
                            }
                        }
                    }
                }
                if best_ic_pin.is_some() && best_child_pin.is_some() {
                    break; // Found a non-GND connection
                }
            }
        }

        let (ic_dx, ic_dy) = best_ic_pin.unwrap_or((0.0, 0.0));
        let (ch_dx, ch_dy) = best_child_pin.unwrap_or((0.0, 0.0));

        // Determine side based on IC pin position relative to IC center
        let side = if ic_dx.abs() > ic_dy.abs() {
            if ic_dx < 0.0 { Side::Left } else { Side::Right }
        } else {
            if ic_dy < 0.0 { Side::Top } else { Side::Bottom }
        };

        child_placements.push(ChildPlacement {
            local_idx: local_i,
            side,
            ic_pin_dx: ic_dx,
            ic_pin_dy: ic_dy,
            child_pin_dx: ch_dx,
            child_pin_dy: ch_dy,
        });
    }

    // Group children by side
    let mut left:   Vec<&ChildPlacement> = Vec::new();
    let mut right:  Vec<&ChildPlacement> = Vec::new();
    let mut top:    Vec<&ChildPlacement> = Vec::new();
    let mut bottom: Vec<&ChildPlacement> = Vec::new();

    for cp in &child_placements {
        match cp.side {
            Side::Left   => left.push(cp),
            Side::Right  => right.push(cp),
            Side::Top    => top.push(cp),
            Side::Bottom => bottom.push(cp),
        }
    }

    // Place children
    let mut positions = vec![(0.0, 0.0, 0.0); n];
    positions[anchor_local] = (0.0, 0.0, 0.0); // IC at center
    let gap = 1.5; // mm between IC and children

    // Place children on each side, stacked along the side's axis
    place_side_children(&left, &block.members, board, &mut positions,
        -(ic_w / 2.0 + gap), true, ic_h);
    place_side_children(&right, &block.members, board, &mut positions,
        ic_w / 2.0 + gap, true, ic_h);
    place_side_children(&top, &block.members, board, &mut positions,
        -(ic_h / 2.0 + gap), false, ic_w);
    place_side_children(&bottom, &block.members, board, &mut positions,
        ic_h / 2.0 + gap, false, ic_w);

    // Compute block bounding box
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for (local_i, &(dx, dy, _)) in positions.iter().enumerate() {
        let comp = &board.components[block.members[local_i]];
        min_x = min_x.min(dx - comp.width_mm / 2.0);
        max_x = max_x.max(dx + comp.width_mm / 2.0);
        min_y = min_y.min(dy - comp.height_mm / 2.0);
        max_y = max_y.max(dy + comp.height_mm / 2.0);
    }

    let margin = 2.0;
    block.width = (max_x - min_x) + margin * 2.0;
    block.height = (max_y - min_y) + margin * 2.0;

    let cx = (min_x + max_x) / 2.0;
    let cy = (min_y + max_y) / 2.0;
    for pos in &mut positions {
        pos.0 -= cx;
        pos.1 -= cy;
    }

    block.internal_positions = positions;
}

/// Place children along one side of the IC.
///
/// `offset`: distance from IC center to this side (negative = left/top)
/// `horizontal`: if true, offset is along X and children stack along Y
///               if false, offset is along Y and children stack along X
/// `span`: how much space along the stacking axis (IC dimension)
fn place_side_children(
    children: &[&ChildPlacement],
    members: &[usize],
    board: &Board,
    positions: &mut [(f64, f64, f64)],
    offset: f64,
    horizontal: bool, // offset direction is X (left/right sides)
    span: f64,
) {
    if children.is_empty() { return; }

    let spacing = 1.0;
    let total_children = children.len();

    // Distribute children evenly along the side
    let start = -(total_children as f64 - 1.0) * spacing * 1.5;

    for (i, cp) in children.iter().enumerate() {
        let comp = &board.components[members[cp.local_idx]];
        let stack_pos = start + i as f64 * (comp.height_mm.max(comp.width_mm) + spacing);

        let (x, y, theta) = if horizontal {
            // Left/Right side: offset along X, stack along Y
            let child_w = comp.width_mm;
            let x = if offset < 0.0 {
                offset - child_w / 2.0 // left side
            } else {
                offset + child_w / 2.0 // right side
            };
            // Orient child: if on left side, rotate 180° so pins face right (toward IC)
            // if on right side, keep at 0° (pins face left toward IC)
            let theta = if offset < 0.0 && cp.child_pin_dx > 0.0 {
                std::f64::consts::PI // flip so connecting pin faces IC
            } else if offset > 0.0 && cp.child_pin_dx < 0.0 {
                std::f64::consts::PI
            } else {
                0.0
            };
            (x, stack_pos, theta)
        } else {
            // Top/Bottom side: offset along Y, stack along X
            let child_h = comp.height_mm;
            let y = if offset < 0.0 {
                offset - child_h / 2.0
            } else {
                offset + child_h / 2.0
            };
            let theta = if offset < 0.0 && cp.child_pin_dy > 0.0 {
                std::f64::consts::PI
            } else if offset > 0.0 && cp.child_pin_dy < 0.0 {
                std::f64::consts::PI
            } else {
                0.0
            };
            (stack_pos, y, theta)
        };

        positions[cp.local_idx] = (x, y, theta);
    }
}

/// Place blocks on the board using shelf packing.
pub fn place_blocks(blocks: &mut [PlacementBlock], board_w: f64, board_h: f64, ec: f64, seed: u64) {
    let n = blocks.len();
    if n == 0 { return; }

    // Sort by area (largest first)
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        let area_a = blocks[a].width * blocks[a].height;
        let area_b = blocks[b].width * blocks[b].height;
        area_b.partial_cmp(&area_a).unwrap()
    });

    let rotate_by = (seed as usize) % n.max(1);
    order.rotate_left(rotate_by.min(n.saturating_sub(1)));

    // Shelf packing
    let mut shelf_x = ec;
    let mut shelf_y = ec;
    let mut shelf_height = 0.0_f64;

    for &idx in &order {
        let bw = blocks[idx].width;
        let bh = blocks[idx].height;

        if shelf_x + bw > board_w - ec {
            shelf_x = ec;
            shelf_y += shelf_height + 3.0; // routing channel between shelves
            shelf_height = 0.0;
        }

        blocks[idx].x = shelf_x + bw / 2.0;
        blocks[idx].y = shelf_y + bh / 2.0;
        shelf_x += bw + 3.0; // routing channel between blocks
        shelf_height = shelf_height.max(bh);
    }
}

/// Find a placement recipe that matches this block's anchor component.
///
/// Matches by looking at the anchor component's entity type name against
/// the recipe's entity_name. For example, if the anchor is an instance
/// of "AP63205", it matches the recipe with entity_name "AP63205".
fn find_recipe_for_block<'a>(
    anchor_name: &str,
    board: &Board,
    members: &[usize],
    recipes: &'a std::collections::BTreeMap<String, PlacementRecipe>,
) -> Option<&'a PlacementRecipe> {
    // Try matching by the anchor component's name or package/entity type
    // The recipe key is the entity name (e.g., "AP63205")
    for (entity_name, recipe) in recipes {
        // Check if this block's group name contains the entity name,
        // or if the anchor component's name matches
        if anchor_name.contains(entity_name.as_str())
            || entity_name.contains(anchor_name)
        {
            return Some(recipe);
        }
    }
    // Also check by looking at component names in the block
    // Expansion children are named like "buck_L_out" where "buck" is the instance name
    // The recipe has children named "L_out", "C_out", etc.
    for (_, recipe) in recipes {
        let recipe_child_names: HashSet<&str> = recipe.positions.iter()
            .map(|p| p.name.as_str())
            .collect();
        let block_child_suffixes: HashSet<String> = members.iter()
            .map(|&idx| {
                let name = &board.components[idx].name;
                // Extract suffix after instance prefix: "buck_L_out" → "L_out"
                name.split_once('_').map(|(_, s)| s.to_string())
                    .unwrap_or_else(|| name.clone())
            })
            .collect();
        // If most recipe children match block suffixes, it's a match
        let matches = recipe_child_names.iter()
            .filter(|&&rn| block_child_suffixes.iter().any(|bs| bs.contains(rn)))
            .count();
        if matches >= recipe.positions.len() / 2 {
            return Some(recipe);
        }
    }
    None
}

/// Apply exact datasheet coordinates from a PlacementRecipe.
fn apply_placement_recipe(
    block: &mut PlacementBlock,
    board: &Board,
    recipe: &PlacementRecipe,
) {
    let n = block.members.len();
    let anchor_idx = block.anchor.unwrap_or(block.members[0]);
    let anchor_local = block.members.iter().position(|&m| m == anchor_idx).unwrap_or(0);

    let mut positions = vec![(0.0, 0.0, 0.0); n];
    positions[anchor_local] = (0.0, 0.0, 0.0); // IC at center

    // Match recipe children to block members by name suffix
    for pos in &recipe.positions {
        for (local_i, &global_i) in block.members.iter().enumerate() {
            if global_i == anchor_idx { continue; }
            let comp_name = &board.components[global_i].name;
            // Match: "buck_L_out" ends with "L_out", recipe has "L_out"
            if comp_name.ends_with(&pos.name)
                || comp_name == &pos.name
                || comp_name.contains(&pos.name)
            {
                positions[local_i] = (
                    pos.dx_mm,
                    pos.dy_mm,
                    pos.rotation_deg.to_radians(),
                );
                break;
            }
        }
    }

    // Compute block bounding box
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for (local_i, &(dx, dy, _)) in positions.iter().enumerate() {
        let comp = &board.components[block.members[local_i]];
        min_x = min_x.min(dx - comp.width_mm / 2.0);
        max_x = max_x.max(dx + comp.width_mm / 2.0);
        min_y = min_y.min(dy - comp.height_mm / 2.0);
        max_y = max_y.max(dy + comp.height_mm / 2.0);
    }

    let margin = 2.0;
    block.width = (max_x - min_x) + margin * 2.0;
    block.height = (max_y - min_y) + margin * 2.0;

    let cx = (min_x + max_x) / 2.0;
    let cy = (min_y + max_y) / 2.0;
    for p in &mut positions {
        p.0 -= cx;
        p.1 -= cy;
    }

    block.internal_positions = positions;
}

/// Stamp block positions back to components.
pub fn stamp_positions(blocks: &[PlacementBlock], board: &mut Board) {
    for block in blocks {
        for (local_i, &comp_idx) in block.members.iter().enumerate() {
            let (dx, dy, dtheta) = block.internal_positions[local_i];
            board.components[comp_idx].x = block.x + dx;
            board.components[comp_idx].y = block.y + dy;
            board.components[comp_idx].theta = dtheta;
        }
    }
}
