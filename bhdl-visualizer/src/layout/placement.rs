use std::collections::HashMap;
use bhdl_netlist::{InstanceId, Netlist};
use crate::layout::types::{Point, PlacementNode};
use crate::layout::utils::find_mst_prim;

pub struct PlacementEngine<'a> {
    netlist: &'a Netlist,
}

impl<'a> PlacementEngine<'a> {
    pub fn new(netlist: &'a Netlist) -> Self {
        PlacementEngine { netlist }
    }

    pub fn run_layered_placement(&self, positions: &mut HashMap<InstanceId, Point>) {
        // Collect all instance positions
        let mut points: Vec<Point> = positions.values().cloned().collect();
        let instance_ids: Vec<InstanceId> = positions.keys().cloned().collect();
        
        if points.len() < 2 {
            return;
        }

        // Find minimum spanning tree to determine placement order
        let mst_edges = find_mst_prim(&points);
        
        // Apply MST-based layered placement
        self.apply_mst_placement(&mst_edges, &instance_ids, positions);
        
        // Apply alignment optimization
        self.run_alignment_optimization(positions);
    }

    fn apply_mst_placement(
        &self, 
        mst_edges: &[(usize, usize)], 
        instance_ids: &[InstanceId], 
        positions: &mut HashMap<InstanceId, Point>
    ) {
        // Group components by layers based on MST structure
        let mut layers: Vec<Vec<usize>> = Vec::new();
        let mut visited = vec![false; instance_ids.len()];
        
        // Start with node 0 as root
        if !instance_ids.is_empty() {
            layers.push(vec![0]);
            visited[0] = true;
        }
        
        // Build layers using MST edges
        let mut current_layer = 0;
        while current_layer < layers.len() {
            let mut next_layer = Vec::new();
            
            for &parent_idx in &layers[current_layer] {
                for &(from, to) in mst_edges {
                    if from == parent_idx && !visited[to] {
                        next_layer.push(to);
                        visited[to] = true;
                    } else if to == parent_idx && !visited[from] {
                        next_layer.push(from);
                        visited[from] = true;
                    }
                }
            }
            
            if !next_layer.is_empty() {
                layers.push(next_layer);
            }
            current_layer += 1;
        }
        
        // Position components based on layers
        let layer_spacing = 100.0;
        let component_spacing = 80.0;
        
        for (layer_idx, layer) in layers.iter().enumerate() {
            let y = layer_idx as f64 * layer_spacing;
            let start_x = -(layer.len() as f64 - 1.0) * component_spacing / 2.0;
            
            for (pos_in_layer, &component_idx) in layer.iter().enumerate() {
                if component_idx < instance_ids.len() {
                    let x = start_x + pos_in_layer as f64 * component_spacing;
                    positions.insert(instance_ids[component_idx], Point::new(x, y));
                }
            }
        }
    }

    fn run_alignment_optimization(&self, positions: &mut HashMap<InstanceId, Point>) {
        // Group components by similar Y coordinates for horizontal alignment
        let mut y_groups: Vec<Vec<(InstanceId, f64)>> = Vec::new();
        const ALIGNMENT_THRESHOLD: f64 = 20.0;
        
        for (&instance_id, &pos) in positions.iter() {
            let mut added_to_group = false;
            
            for group in &mut y_groups {
                if let Some((_, group_y)) = group.first() {
                    if (pos.y - group_y).abs() < ALIGNMENT_THRESHOLD {
                        group.push((instance_id, pos.x));
                        added_to_group = true;
                        break;
                    }
                }
            }
            
            if !added_to_group {
                y_groups.push(vec![(instance_id, pos.x)]);
            }
        }
        
        // Align components within each group
        for group in &mut y_groups {
            if group.len() > 1 {
                // Calculate average Y position for this group
                let avg_y = group.iter()
                    .map(|(id, _)| positions[id].y)
                    .sum::<f64>() / group.len() as f64;
                
                // Sort by X coordinate
                group.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                
                // Apply alignment
                for (instance_id, _) in group {
                    if let Some(pos) = positions.get_mut(instance_id) {
                        pos.y = avg_y;
                    }
                }
            }
        }
        
        // Group components by similar X coordinates for vertical alignment
        let mut x_groups: Vec<Vec<(InstanceId, f64)>> = Vec::new();
        
        for (&instance_id, &pos) in positions.iter() {
            let mut added_to_group = false;
            
            for group in &mut x_groups {
                if let Some((_, group_x)) = group.first() {
                    if (pos.x - group_x).abs() < ALIGNMENT_THRESHOLD {
                        group.push((instance_id, pos.y));
                        added_to_group = true;
                        break;
                    }
                }
            }
            
            if !added_to_group {
                x_groups.push(vec![(instance_id, pos.y)]);
            }
        }
        
        // Align components within each X group
        for group in &mut x_groups {
            if group.len() > 1 {
                // Calculate average X position for this group
                let avg_x = group.iter()
                    .map(|(id, _)| positions[id].x)
                    .sum::<f64>() / group.len() as f64;
                
                // Sort by Y coordinate
                group.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                
                // Apply alignment
                for (instance_id, _) in group {
                    if let Some(pos) = positions.get_mut(instance_id) {
                        pos.x = avg_x;
                    }
                }
            }
        }
    }
} 