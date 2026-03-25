//! Hierarchical block-based placement.
//!
//! Like FPGA CLB packing: related components are absorbed into blocks,
//! blocks are placed on the board, then components are detailed within blocks.
//!
//! Block types:
//! - **Expansion block**: IC + expansion children (L, D, C for buck regulator)
//! - **Singleton block**: standalone component (LED, load resistor)
//!
//! Flow:
//! 1. Form blocks from functional groups + standalone components
//! 2. Layout within each block (IC center, caps flanking, known patterns)
//! 3. Place blocks on board (simple problem: ~8-10 blocks)
//! 4. Stamp block positions back to components

use std::collections::{HashMap, HashSet};
use crate::types::*;

/// A placement block containing one or more components.
#[derive(Debug, Clone)]
pub struct PlacementBlock {
    pub name: String,
    /// Component indices (into Board.components)
    pub members: Vec<usize>,
    /// The "anchor" component (IC/main component), if any
    pub anchor: Option<usize>,
    /// Block bounding box after internal layout (width, height in mm)
    pub width: f64,
    pub height: f64,
    /// Block position on board (center)
    pub x: f64,
    pub y: f64,
    /// Internal component positions relative to block center
    pub internal_positions: Vec<(f64, f64, f64)>, // (dx, dy, theta) per member
}

/// Form blocks from the board's functional groups and standalone components.
pub fn form_blocks(board: &Board) -> Vec<PlacementBlock> {
    let mut blocks = Vec::new();
    let mut assigned: HashSet<usize> = HashSet::new();

    let comp_id_to_idx: HashMap<ComponentId, usize> = board.components.iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();

    // 1. Create blocks from functional groups (expansion blocks)
    for group in &board.groups {
        let member_indices: Vec<usize> = group.members.iter()
            .filter_map(|id| comp_id_to_idx.get(id).copied())
            .collect();

        if member_indices.is_empty() { continue; }

        // Find the anchor (parent IC — largest component or the one named by group)
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

        layout_expansion_block(&mut block, board);
        blocks.push(block);
    }

    // 2. Create singleton blocks for unassigned components
    for (i, comp) in board.components.iter().enumerate() {
        if assigned.contains(&i) { continue; }
        let block = PlacementBlock {
            name: comp.name.clone(),
            members: vec![i],
            anchor: Some(i),
            width: comp.width_mm + 2.0, // add routing margin
            height: comp.height_mm + 2.0,
            x: 0.0,
            y: 0.0,
            internal_positions: vec![(0.0, 0.0, 0.0)],
        };
        blocks.push(block);
    }

    blocks
}

/// Layout components within an expansion block.
///
/// Pattern: IC at center, passive children arranged around it.
/// - Caps: flanking the IC on the side nearest their connecting pin
/// - Inductors: above or below the IC
/// - Diodes: next to the inductor
/// - Resistors: on the feedback/sense side
fn layout_expansion_block(block: &mut PlacementBlock, board: &Board) {
    let n = block.members.len();
    if n == 0 { return; }

    if n == 1 {
        let comp = &board.components[block.members[0]];
        block.width = comp.width_mm + 2.0;
        block.height = comp.height_mm + 2.0;
        block.internal_positions = vec![(0.0, 0.0, 0.0)];
        return;
    }

    // Separate anchor (IC) from children (passives)
    let anchor_idx = block.anchor.unwrap_or(block.members[0]);
    let anchor_local = block.members.iter().position(|&m| m == anchor_idx).unwrap_or(0);
    let anchor_comp = &board.components[anchor_idx];

    // Classify children by component type
    let mut caps: Vec<usize> = Vec::new();    // local indices
    let mut inductors: Vec<usize> = Vec::new();
    let mut diodes: Vec<usize> = Vec::new();
    let mut resistors: Vec<usize> = Vec::new();
    let mut others: Vec<usize> = Vec::new();

    for (local_i, &global_i) in block.members.iter().enumerate() {
        if global_i == anchor_idx { continue; }
        let cat = board.components[global_i].name.to_lowercase();
        let cls = board.components[global_i].package.as_str();
        if cat.contains("_c_") || cat.contains("cap") || cat.starts_with("c") {
            caps.push(local_i);
        } else if cat.contains("_l_") || cat.contains("ind") || cat.starts_with("l") {
            inductors.push(local_i);
        } else if cat.contains("_d_") || cat.contains("diode") || cat.starts_with("d") {
            diodes.push(local_i);
        } else if cat.contains("_r_") || cat.contains("res") || cat.starts_with("r") {
            resistors.push(local_i);
        } else {
            others.push(local_i);
        }
    }

    // Layout: IC at center, children arranged around it
    let ic_w = anchor_comp.width_mm;
    let ic_h = anchor_comp.height_mm;
    let spacing = 1.5; // mm between components within block

    let mut positions = vec![(0.0, 0.0, 0.0); n];
    positions[anchor_local] = (0.0, 0.0, 0.0); // IC at center

    // Place caps on left/right sides of IC (input caps left, output caps right)
    let cap_x_start = ic_w / 2.0 + spacing;
    let mut left_y = -(caps.len() as f64 * 2.0) / 2.0;
    let mut right_y = left_y;
    let mut left_side = true;

    for &local_i in &caps {
        let comp = &board.components[block.members[local_i]];
        let ch = comp.height_mm;
        if left_side {
            positions[local_i] = (-cap_x_start - comp.width_mm / 2.0, left_y, 0.0);
            left_y += ch + spacing;
        } else {
            positions[local_i] = (cap_x_start + comp.width_mm / 2.0, right_y, 0.0);
            right_y += ch + spacing;
        }
        left_side = !left_side;
    }

    // Place inductors above IC
    let mut ind_x = -(inductors.len() as f64 * 4.0) / 2.0;
    for &local_i in &inductors {
        let comp = &board.components[block.members[local_i]];
        positions[local_i] = (ind_x, -(ic_h / 2.0 + spacing + comp.height_mm / 2.0), 0.0);
        ind_x += comp.width_mm + spacing;
    }

    // Place diodes next to inductors
    let mut diode_x = ind_x;
    for &local_i in &diodes {
        let comp = &board.components[block.members[local_i]];
        positions[local_i] = (diode_x, -(ic_h / 2.0 + spacing + comp.height_mm / 2.0), 0.0);
        diode_x += comp.width_mm + spacing;
    }

    // Place resistors below IC
    let mut res_x = -(resistors.len() as f64 * 3.0) / 2.0;
    for &local_i in &resistors {
        let comp = &board.components[block.members[local_i]];
        positions[local_i] = (res_x, ic_h / 2.0 + spacing + comp.height_mm / 2.0, 0.0);
        res_x += comp.width_mm + spacing;
    }

    // Place others in remaining spots
    let mut other_x = res_x;
    for &local_i in &others {
        let comp = &board.components[block.members[local_i]];
        positions[local_i] = (other_x, ic_h / 2.0 + spacing + comp.height_mm / 2.0, 0.0);
        other_x += comp.width_mm + spacing;
    }

    // Compute block bounding box from all internal positions
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

    // Add margin for internal routing
    let margin = 2.0;
    block.width = (max_x - min_x) + margin * 2.0;
    block.height = (max_y - min_y) + margin * 2.0;

    // Center the internal positions within the block
    let cx = (min_x + max_x) / 2.0;
    let cy = (min_y + max_y) / 2.0;
    for pos in &mut positions {
        pos.0 -= cx;
        pos.1 -= cy;
    }

    block.internal_positions = positions;
}

/// Place blocks on the board using a simple grid layout.
/// Blocks are ordered by size (largest first) and placed in a grid.
pub fn place_blocks(blocks: &mut [PlacementBlock], board_w: f64, board_h: f64, ec: f64, seed: u64) {
    let n = blocks.len();
    if n == 0 { return; }

    // Sort blocks by area (largest first) for stable placement
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        let area_a = blocks[a].width * blocks[a].height;
        let area_b = blocks[b].width * blocks[b].height;
        area_b.partial_cmp(&area_a).unwrap()
    });

    // Use seed to vary the initial rotation of the order
    let rotate_by = (seed as usize) % n.max(1);
    order.rotate_left(rotate_by.min(n.saturating_sub(1)));

    // Simple shelf packing: place blocks left-to-right, top-to-bottom
    let usable_w = board_w - 2.0 * ec;
    let mut shelf_x = ec;
    let mut shelf_y = ec;
    let mut shelf_height = 0.0_f64;

    for &idx in &order {
        let bw = blocks[idx].width;
        let bh = blocks[idx].height;

        // Wrap to next shelf if needed
        if shelf_x + bw > board_w - ec {
            shelf_x = ec;
            shelf_y += shelf_height + 2.0; // gap between shelves
            shelf_height = 0.0;
        }

        blocks[idx].x = shelf_x + bw / 2.0;
        blocks[idx].y = shelf_y + bh / 2.0;
        shelf_x += bw + 2.0; // gap between blocks on shelf
        shelf_height = shelf_height.max(bh);
    }
}

/// Stamp block positions back to components.
/// Each component's global position = block position + internal offset.
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
