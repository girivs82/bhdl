//! Sugiyama hierarchical graph layout algorithm for professional circuit schematics
//!
//! Based on the classic Sugiyama method (1981) for layered graph drawing:
//! 1. Cycle Breaking - Convert to DAG
//! 2. Layer Assignment - Assign nodes to horizontal layers
//! 3. Crossing Minimization - Reorder nodes to reduce edge crossings
//! 4. Coordinate Assignment - Position nodes with proper spacing
//! 5. Edge Routing - Orthogonal edge routing with minimal bends
//!
//! References:
//! - Sugiyama, K., Tagawa, S., & Toda, M. (1981). "Methods for Visual Understanding of Hierarchical System Structures"
//! - Eclipse Layout Kernel (ELK) Layered Algorithm
//! - NetlistSVG approach for circuit-specific optimizations

use std::collections::{HashMap, HashSet, VecDeque};
use anyhow::{Result, Context};
use log::{debug, info, warn};

use bhdl_netlist::{Netlist, InstanceId, NetId, ConnectionPoint};
use bhdl_synthesizer::DatabaseComponentInstance;
use crate::types::{Point, Component, Net, NetType, CircuitLayout};
use crate::symbols::SymbolManager;
use crate::topology_layout::{CircuitGraph, ComponentRole};
use crate::orthogonal_edge_router::OrthogonalEdgeRouter;

/// Grid size for snapping coordinates
const GRID_SIZE: f64 = 50.0;

/// Horizontal spacing between layers
const LAYER_SPACING: f64 = 300.0; // Reduced from 800 to 300 for compact layout

/// Vertical spacing between nodes within a layer
const NODE_SPACING: f64 = 150.0; // Reduced from 400 to 150 for compact layout

/// Snap value to grid
fn snap_to_grid(value: f64) -> f64 {
    (value / GRID_SIZE).round() * GRID_SIZE
}

/// Edge in the graph
#[derive(Debug, Clone)]
pub struct GraphEdge {
    pub from: InstanceId,
    pub to: InstanceId,
    pub net_id: NetId,
    pub reversed: bool, // If edge was reversed during cycle breaking
}

/// Layer assignment for nodes
pub type LayerAssignment = HashMap<InstanceId, usize>;

/// Ordering of nodes within each layer
pub type LayerOrdering = Vec<Vec<InstanceId>>;

/// Sugiyama layout engine
pub struct SugiyamaLayoutEngine {
    /// Grid size for coordinate snapping
    grid_size: f64,
    /// Horizontal spacing between layers
    layer_spacing: f64,
    /// Vertical spacing within layers
    node_spacing: f64,
    /// Symbol manager for component rendering
    symbol_manager: SymbolManager,
}

impl SugiyamaLayoutEngine {
    /// Create new Sugiyama layout engine
    pub fn new() -> Self {
        Self {
            grid_size: GRID_SIZE,
            layer_spacing: LAYER_SPACING,
            node_spacing: NODE_SPACING,
            symbol_manager: SymbolManager::new(),
        }
    }

    /// Generate circuit layout using Sugiyama algorithm
    pub async fn layout_circuit(
        &mut self,
        netlist: &Netlist,
        db_components: &[DatabaseComponentInstance],
    ) -> Result<CircuitLayout> {
        info!("Starting Sugiyama hierarchical layout");

        // Build circuit graph
        let mut graph = CircuitGraph::from_netlist(netlist, db_components)?;

        // Phase 1: Cycle Breaking
        info!("Phase 1: Breaking cycles");
        let edges = self.break_cycles(&mut graph)?;

        // Phase 2: Layer Assignment
        info!("Phase 2: Assigning layers");
        let layer_assignment = self.assign_layers(&graph, &edges)?;

        // Phase 3: Crossing Minimization
        info!("Phase 3: Minimizing crossings");
        let layer_ordering = self.minimize_crossings(&graph, &edges, &layer_assignment)?;

        // Phase 4: Coordinate Assignment
        info!("Phase 4: Assigning coordinates");
        let coordinates = self.assign_coordinates(&layer_ordering, &graph)?;

        // Phase 5: Create components at assigned positions
        info!("Phase 5: Creating components");
        let components = self.create_components_at_positions(
            &coordinates,
            netlist,
            db_components,
        ).await?;

        // Build layout
        let mut layout = CircuitLayout::new();
        layout.grid_spacing = self.grid_size;

        for component in components {
            layout.add_component(component);
        }

        // Phase 5: Orthogonal edge routing
        info!("Phase 5: Routing nets");
        let nets = self.route_nets_orthogonal(
            netlist,
            &layout,
            &graph,
            db_components,
        )?;

        for net in nets {
            layout.add_net(net);
        }

        layout.update_bounding_box();

        info!("Sugiyama layout complete: {} components, {} nets",
              layout.components.len(), layout.nets.len());

        Ok(layout)
    }

    /// Phase 1: Break cycles to create a DAG
    ///
    /// Uses greedy cycle breaking: reverse edges that create cycles
    fn break_cycles(&self, graph: &CircuitGraph) -> Result<Vec<GraphEdge>> {
        debug!("Breaking cycles using greedy algorithm");

        let mut edges = Vec::new();
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        // Convert graph edges to our edge structure
        for &(from, to, net_id) in &graph.edges {
            edges.push(GraphEdge {
                from,
                to,
                net_id,
                reversed: false,
            });
        }

        // DFS to detect cycles
        fn has_cycle(
            node: InstanceId,
            graph: &CircuitGraph,
            edges: &[GraphEdge],
            visited: &mut HashSet<InstanceId>,
            rec_stack: &mut HashSet<InstanceId>,
        ) -> Vec<(InstanceId, InstanceId)> {
            visited.insert(node);
            rec_stack.insert(node);

            let mut cycles = Vec::new();

            for edge in edges {
                if edge.from == node && !edge.reversed {
                    if !visited.contains(&edge.to) {
                        let mut sub_cycles = has_cycle(edge.to, graph, edges, visited, rec_stack);
                        cycles.append(&mut sub_cycles);
                    } else if rec_stack.contains(&edge.to) {
                        // Found a cycle
                        cycles.push((edge.from, edge.to));
                    }
                }
            }

            rec_stack.remove(&node);
            cycles
        }

        // Find all cycles
        let all_nodes: Vec<_> = graph.nodes.keys().copied().collect();
        let mut cycles_to_break = Vec::new();

        for &node in &all_nodes {
            if !visited.contains(&node) {
                let mut cycles = has_cycle(node, graph, &edges, &mut visited, &mut rec_stack);
                cycles_to_break.append(&mut cycles);
            }
        }

        // Reverse edges that create cycles
        let cycles_set: HashSet<_> = cycles_to_break.into_iter().collect();
        for edge in &mut edges {
            if cycles_set.contains(&(edge.from, edge.to)) {
                // Reverse this edge
                std::mem::swap(&mut edge.from, &mut edge.to);
                edge.reversed = true;
                debug!("  Reversed edge {:?} -> {:?}", edge.to, edge.from);
            }
        }

        info!("Cycle breaking complete: {} edges, {} reversed",
              edges.len(), edges.iter().filter(|e| e.reversed).count());

        Ok(edges)
    }

    /// Phase 2: Assign nodes to layers
    ///
    /// Uses longest-path layering to minimize edge lengths
    fn assign_layers(
        &self,
        graph: &CircuitGraph,
        edges: &[GraphEdge],
    ) -> Result<LayerAssignment> {
        debug!("Assigning layers using longest-path algorithm");

        let mut layers = HashMap::new();
        let mut in_degree: HashMap<InstanceId, usize> = HashMap::new();

        // Calculate in-degrees
        for node_id in graph.nodes.keys() {
            in_degree.insert(*node_id, 0);
        }

        for edge in edges {
            *in_degree.entry(edge.to).or_insert(0) += 1;
        }

        // Start with source nodes (in-degree = 0) or power sources
        let mut queue = VecDeque::new();
        for (node_id, &degree) in &in_degree {
            if degree == 0 || matches!(graph.nodes[node_id].role, ComponentRole::PowerSource | ComponentRole::Protection) {
                layers.insert(*node_id, 0);
                queue.push_back(*node_id);
                debug!("  Source node: {:?} at layer 0", graph.nodes[node_id].component_type);
            }
        }

        // If no sources found, pick first node
        if queue.is_empty() {
            if let Some(&node_id) = graph.nodes.keys().next() {
                layers.insert(node_id, 0);
                queue.push_back(node_id);
            }
        }

        // Topological sort with longest path
        let mut processed = HashSet::new();
        while let Some(current) = queue.pop_front() {
            if processed.contains(&current) {
                continue;
            }
            processed.insert(current);

            let current_layer = layers[&current];

            // Update successors
            for edge in edges {
                if edge.from == current {
                    let successor = edge.to;
                    let new_layer = current_layer + 1;

                    // Take maximum layer (longest path)
                    let prev_layer = layers.get(&successor).copied().unwrap_or(0);
                    if new_layer > prev_layer {
                        layers.insert(successor, new_layer);
                        debug!("  {:?} assigned to layer {}",
                               graph.nodes[&successor].component_type, new_layer);
                    }

                    // Check if all predecessors processed
                    let all_preds_done = edges.iter()
                        .filter(|e| e.to == successor)
                        .all(|e| processed.contains(&e.from));

                    if all_preds_done && !processed.contains(&successor) {
                        queue.push_back(successor);
                    }
                }
            }
        }

        // Assign remaining nodes (isolated or in separate components)
        let max_layer = layers.values().max().copied().unwrap_or(0);
        for node_id in graph.nodes.keys() {
            if !layers.contains_key(node_id) {
                warn!("  Node {:?} not reached, assigning to layer {}",
                      graph.nodes[node_id].component_type, max_layer + 1);
                layers.insert(*node_id, max_layer + 1);
            }
        }

        // Special handling: move ground symbols to last layer
        let final_layer = layers.values().max().copied().unwrap_or(0);
        for (node_id, node) in &graph.nodes {
            if matches!(node.role, ComponentRole::Ground) {
                layers.insert(*node_id, final_layer);
                debug!("  Ground symbol {:?} moved to final layer {}", node.component_type, final_layer);
            }
        }

        let num_layers = layers.values().max().map(|&l| l + 1).unwrap_or(0);
        info!("Layer assignment complete: {} nodes in {} layers", layers.len(), num_layers);

        Ok(layers)
    }

    /// Phase 3: Minimize edge crossings
    ///
    /// Uses barycenter heuristic with multiple sweeps
    fn minimize_crossings(
        &self,
        graph: &CircuitGraph,
        edges: &[GraphEdge],
        layer_assignment: &LayerAssignment,
    ) -> Result<LayerOrdering> {
        debug!("Minimizing crossings using barycenter heuristic");

        // Group nodes by layer
        let max_layer = *layer_assignment.values().max().unwrap_or(&0);
        let mut layers: Vec<Vec<InstanceId>> = vec![Vec::new(); max_layer + 1];

        for (&node_id, &layer) in layer_assignment {
            layers[layer].push(node_id);
        }

        // Initial ordering: sort by component role for better starting point
        for layer_nodes in &mut layers {
            layer_nodes.sort_by_key(|id| {
                let node = &graph.nodes[id];
                match node.role {
                    ComponentRole::PowerSource => 0,
                    ComponentRole::Protection => 1,
                    ComponentRole::Regulator => 2,
                    ComponentRole::Filter => 3,
                    ComponentRole::Passive => 4,
                    ComponentRole::Load => 5,
                    ComponentRole::Ground => 6,
                    ComponentRole::Unknown => 7,
                }
            });
        }

        debug!("Initial layer distribution:");
        for (i, layer) in layers.iter().enumerate() {
            debug!("  Layer {}: {} nodes", i, layer.len());
        }

        // Multiple sweep passes to minimize crossings
        const MAX_ITERATIONS: usize = 10;
        let mut best_crossings = self.count_crossings(&layers, edges);
        let mut best_ordering = layers.clone();

        for iteration in 0..MAX_ITERATIONS {
            // Forward sweep (top to bottom)
            for layer_idx in 1..layers.len() {
                self.reorder_layer_barycenter(
                    &mut layers,
                    layer_idx,
                    edges,
                    true, // use previous layer
                );
            }

            // Backward sweep (bottom to top)
            for layer_idx in (0..layers.len()-1).rev() {
                self.reorder_layer_barycenter(
                    &mut layers,
                    layer_idx,
                    edges,
                    false, // use next layer
                );
            }

            let crossings = self.count_crossings(&layers, edges);
            debug!("  Iteration {}: {} crossings", iteration, crossings);

            if crossings < best_crossings {
                best_crossings = crossings;
                best_ordering = layers.clone();
            }

            // Early termination if no crossings
            if crossings == 0 {
                break;
            }
        }

        info!("Crossing minimization complete: {} crossings", best_crossings);

        Ok(best_ordering)
    }

    /// Reorder a layer using barycenter heuristic
    fn reorder_layer_barycenter(
        &self,
        layers: &mut [Vec<InstanceId>],
        layer_idx: usize,
        edges: &[GraphEdge],
        use_previous: bool,
    ) {
        if layers[layer_idx].is_empty() {
            return;
        }

        let reference_layer = if use_previous {
            if layer_idx == 0 { return; }
            layer_idx - 1
        } else {
            if layer_idx >= layers.len() - 1 { return; }
            layer_idx + 1
        };

        // Calculate barycenter for each node
        let mut barycenters: Vec<(InstanceId, f64)> = Vec::new();

        for &node in &layers[layer_idx] {
            // Find connected nodes in reference layer
            let mut positions = Vec::new();

            for (pos, &ref_node) in layers[reference_layer].iter().enumerate() {
                // Check if connected
                let connected = if use_previous {
                    edges.iter().any(|e| e.from == ref_node && e.to == node)
                } else {
                    edges.iter().any(|e| e.from == node && e.to == ref_node)
                };

                if connected {
                    positions.push(pos as f64);
                }
            }

            // Calculate barycenter (average position)
            let barycenter = if positions.is_empty() {
                // No connections: keep current position
                layers[layer_idx].iter().position(|&n| n == node).unwrap() as f64
            } else {
                positions.iter().sum::<f64>() / positions.len() as f64
            };

            barycenters.push((node, barycenter));
        }

        // Sort by barycenter
        barycenters.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // Update layer ordering
        layers[layer_idx] = barycenters.into_iter().map(|(node, _)| node).collect();
    }

    /// Count total edge crossings in current layout
    fn count_crossings(&self, layers: &[Vec<InstanceId>], edges: &[GraphEdge]) -> usize {
        let mut crossings = 0;

        // Check crossings between adjacent layers
        for layer_idx in 0..layers.len()-1 {
            let layer1 = &layers[layer_idx];
            let layer2 = &layers[layer_idx + 1];

            // Build position maps
            let pos1: HashMap<_, _> = layer1.iter().enumerate()
                .map(|(i, &node)| (node, i))
                .collect();
            let pos2: HashMap<_, _> = layer2.iter().enumerate()
                .map(|(i, &node)| (node, i))
                .collect();

            // Count crossings between all edge pairs
            for e1 in edges {
                if let (Some(&p1_from), Some(&p1_to)) = (pos1.get(&e1.from), pos2.get(&e1.to)) {
                    for e2 in edges {
                        if e1.from != e2.from && e1.to != e2.to {
                            if let (Some(&p2_from), Some(&p2_to)) = (pos1.get(&e2.from), pos2.get(&e2.to)) {
                                // Check if edges cross
                                if (p1_from < p2_from && p1_to > p2_to) || (p1_from > p2_from && p1_to < p2_to) {
                                    crossings += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        crossings / 2 // Each crossing counted twice
    }

    /// Phase 4: Assign grid-aligned coordinates
    fn assign_coordinates(
        &self,
        layers: &LayerOrdering,
        graph: &CircuitGraph,
    ) -> Result<HashMap<InstanceId, Point>> {
        debug!("Assigning coordinates with grid alignment");

        let mut coordinates = HashMap::new();

        for (layer_idx, layer_nodes) in layers.iter().enumerate() {
            let x = snap_to_grid(layer_idx as f64 * self.layer_spacing);

            // Calculate total height needed for this layer
            let total_height = (layer_nodes.len() as f64 - 1.0) * self.node_spacing;
            let start_y = -total_height / 2.0; // Center vertically

            for (node_idx, &node_id) in layer_nodes.iter().enumerate() {
                let y = snap_to_grid(start_y + node_idx as f64 * self.node_spacing);

                coordinates.insert(node_id, Point::new(x, y));

                debug!("  {:?} at ({}, {})",
                       graph.nodes[&node_id].component_type, x, y);
            }
        }

        info!("Coordinate assignment complete: {} positions", coordinates.len());

        Ok(coordinates)
    }

    /// Create component instances at assigned positions with automatic rotation
    async fn create_components_at_positions(
        &mut self,
        coordinates: &HashMap<InstanceId, Point>,
        netlist: &Netlist,
        db_components: &[DatabaseComponentInstance],
    ) -> Result<Vec<Component>> {
        debug!("Creating components at assigned positions");

        let mut components = Vec::new();

        // Build type lookup
        let type_map: HashMap<String, &DatabaseComponentInstance> = db_components
            .iter()
            .map(|c| (c.bhdl_type.clone(), c))
            .collect();

        for (&instance_id, &position) in coordinates {
            let instance = netlist.get_instance(instance_id)
                .with_context(|| format!("Instance {:?} not found", instance_id))?;

            let module_def = netlist.get_module(instance.definition)
                .with_context(|| format!("Module not found for {}", instance.name))?;

            let db_component = type_map.get(&module_def.name)
                .with_context(|| format!("Database component not found for type {}", module_def.name))?;

            // Determine rotation based on component type and signal flow
            // For horizontal signal flow (left-to-right), most components should be horizontal (0°)
            // Capacitors and resistors between signal and ground should be vertical (90°)
            let rotation = self.determine_component_rotation(
                instance_id,
                &module_def.name,
                netlist,
            );

            debug!("  Component {} at ({}, {}) with rotation {}°",
                   instance.name, position.x, position.y, rotation);

            let component = self.symbol_manager.create_component(
                instance_id,
                db_component,
                position,
                rotation,
            ).await?;

            components.push(component);
        }

        debug!("Created {} components", components.len());

        Ok(components)
    }

    /// Determine appropriate rotation for component based on its type and connections
    fn determine_component_rotation(
        &self,
        instance_id: InstanceId,
        component_type: &str,
        netlist: &Netlist,
    ) -> f64 {
        let type_lower = component_type.to_lowercase();

        // Check if component is connected to ground (suggests vertical orientation)
        let connected_to_ground = netlist.nets.iter().any(|(_, net)| {
            net.name.as_ref().map(|n| n == "GND").unwrap_or(false) &&
            net.connections.iter().any(|conn| {
                match conn {
                    bhdl_netlist::ConnectionPoint::InstancePin(inst_id, _) => *inst_id == instance_id,
                    bhdl_netlist::ConnectionPoint::PinInstance(pin_inst_id) => {
                        netlist.get_pin_instance(*pin_inst_id)
                            .map(|pi| pi.instance == instance_id)
                            .unwrap_or(false)
                    }
                    _ => false,
                }
            })
        });

        // Rotation rules:
        // - Power symbols: 0° (horizontal)
        // - Ground symbols: 0° (horizontal)
        // - Capacitors to ground: 90° (vertical)
        // - Resistors: 0° (horizontal in signal path)
        // - ICs/Regulators: 0° (horizontal)
        // - LEDs: 90° (vertical, cathode down)
        // - Diodes in signal path: 0° (horizontal)

        if type_lower.contains("cap") || type_lower.contains("capacitor") {
            if connected_to_ground {
                90.0  // Vertical for bypass/decoupling caps
            } else {
                90.0  // Generally vertical looks better for caps
            }
        } else if type_lower.contains("led") {
            90.0  // LEDs vertical (standard orientation)
        } else {
            0.0   // Default horizontal for signal flow
        }
    }

    /// Phase 5: Route nets orthogonally
    fn route_nets_orthogonal(
        &self,
        netlist: &Netlist,
        layout: &CircuitLayout,
        graph: &CircuitGraph,
        db_components: &[DatabaseComponentInstance],
    ) -> Result<Vec<Net>> {
        debug!("Routing nets using orthogonal router");

        let mut router = OrthogonalEdgeRouter::new();
        let mut routed_nets = Vec::new();

        // Route ground nets with ground rail
        for &net_id in &graph.ground_nets {
            if let Some(netlist_net) = netlist.nets.get(net_id) {
                let connection_points = self.collect_connection_points(
                    netlist_net,
                    layout,
                    netlist,
                    db_components,
                )?;

                if connection_points.len() < 2 {
                    continue;
                }

                debug!("  Routing ground net '{}' with {} points",
                       netlist_net.name.as_ref().unwrap_or(&"unnamed".to_string()),
                       connection_points.len());

                let mut net = Net::with_type(net_id, netlist_net.name.clone(), NetType::Ground);

                // Use ground rail routing at bottom of circuit
                let max_y = layout.components.iter()
                    .map(|c| c.position.y)
                    .fold(f64::NEG_INFINITY, f64::max);
                let ground_rail_y = snap_to_grid(max_y + 600.0);

                let segments = router.route_ground_rail(&connection_points, ground_rail_y);
                for segment in segments {
                    net.add_routing_segment(segment);
                }

                for &point in &connection_points {
                    net.add_connection_point(point);
                }

                routed_nets.push(net);
            }
        }

        // Route power nets with power rail or normal routing
        for &net_id in &graph.power_nets {
            if let Some(netlist_net) = netlist.nets.get(net_id) {
                let connection_points = self.collect_connection_points(
                    netlist_net,
                    layout,
                    netlist,
                    db_components,
                )?;

                if connection_points.len() < 2 {
                    continue;
                }

                debug!("  Routing power net '{}' with {} points",
                       netlist_net.name.as_ref().unwrap_or(&"unnamed".to_string()),
                       connection_points.len());

                let mut net = Net::with_type(net_id, netlist_net.name.clone(), NetType::Power);

                // Use multi-point routing (bus-based)
                let segments = router.route_multi_point(&connection_points);
                for segment in segments {
                    net.add_routing_segment(segment);
                }

                for &point in &connection_points {
                    net.add_connection_point(point);
                }

                routed_nets.push(net);
            }
        }

        // Route signal nets
        for (net_id, netlist_net) in &netlist.nets {
            if graph.power_nets.contains(&net_id) || graph.ground_nets.contains(&net_id) {
                continue; // Already routed
            }

            let connection_points = self.collect_connection_points(
                netlist_net,
                layout,
                netlist,
                db_components,
            )?;

            if connection_points.len() < 2 {
                continue;
            }

            debug!("  Routing signal net '{}' with {} points",
                   netlist_net.name.as_ref().unwrap_or(&"unnamed".to_string()),
                   connection_points.len());

            let mut net = Net::with_type(net_id, netlist_net.name.clone(), NetType::Signal);

            // Route based on number of connection points
            let segments = if connection_points.len() == 2 {
                router.route_two_point(connection_points[0], connection_points[1])
            } else {
                router.route_multi_point(&connection_points)
            };

            for segment in segments {
                net.add_routing_segment(segment);
            }

            for &point in &connection_points {
                net.add_connection_point(point);
            }

            routed_nets.push(net);
        }

        debug!("Routed {} nets total", routed_nets.len());

        Ok(routed_nets)
    }

    /// Collect actual pin positions for a net
    fn collect_connection_points(
        &self,
        netlist_net: &bhdl_netlist::Net,
        layout: &CircuitLayout,
        netlist: &Netlist,
        db_components: &[DatabaseComponentInstance],
    ) -> Result<Vec<Point>> {
        let mut points = Vec::new();

        for connection in &netlist_net.connections {
            let (instance_id, pin_id) = match connection {
                ConnectionPoint::InstancePin(inst_id, p_id) => (*inst_id, *p_id),
                ConnectionPoint::PinInstance(pin_inst_id) => {
                    if let Some(pin_inst) = netlist.get_pin_instance(*pin_inst_id) {
                        (pin_inst.instance, pin_inst.pin_def)
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };

            if let Some(component) = layout.get_component_by_instance(instance_id) {
                // Try to get actual pin position
                let mut used_pin_position = false;

                if let Some(pin) = netlist.get_pin(pin_id) {
                    let resolved_pin = self.resolve_pin_name(
                        component,
                        &pin.name,
                        netlist,
                        instance_id,
                        db_components,
                    );

                    if let Some(pin_name) = resolved_pin {
                        if let Some(pin_pos) = component.get_pin_world_position(&pin_name) {
                            // Use actual pin position
                            points.push(pin_pos);
                            used_pin_position = true;
                            debug!("  Using pin position for {}.{}: ({}, {})",
                                   component.label.as_ref().unwrap_or(&"?".to_string()),
                                   pin_name, pin_pos.x, pin_pos.y);
                        } else {
                            debug!("  Pin {} found but no world position available (component has {} pins)",
                                   pin_name, component.pins.len());
                        }
                    } else {
                        debug!("  Could not resolve pin name '{}' for component (has pins: {:?})",
                               pin.name, component.pins.keys().collect::<Vec<_>>());
                    }
                }

                // Fall back to component center if pin position not available
                if !used_pin_position {
                    points.push(component.position);
                    debug!("  Using component center for {} (no pin position)",
                           component.label.as_ref().unwrap_or(&"?".to_string()));
                }
            }
        }

        Ok(points)
    }

    /// Resolve netlist pin name to component pin name
    fn resolve_pin_name(
        &self,
        component: &Component,
        netlist_pin_name: &str,
        netlist: &Netlist,
        instance_id: InstanceId,
        db_components: &[DatabaseComponentInstance],
    ) -> Option<String> {
        // Strategy 1: Direct match
        if component.pins.contains_key(netlist_pin_name) {
            return Some(netlist_pin_name.to_string());
        }

        // Strategy 2: Use pin_mapping from database
        if let Some(instance) = netlist.get_instance(instance_id) {
            if let Some(db_comp) = db_components.iter().find(|c| c.instance_name == instance.name) {
                if let Some(db_pin) = db_comp.pin_mapping.get(netlist_pin_name) {
                    if component.pins.contains_key(db_pin) {
                        return Some(db_pin.clone());
                    }
                }
            }
        }

        // Strategy 3: Common aliases
        let aliases = match netlist_pin_name {
            "VIN" | "IN" => vec!["VI", "1", "INPUT"],
            "VOUT" | "OUT" => vec!["VO", "3", "OUTPUT"],
            "GND" => vec!["2", "GND", "GROUND"],
            "A" | "ANODE" => vec!["A", "1"],
            "K" | "CATHODE" => vec!["K", "2"],
            _ => vec![],
        };

        for alias in aliases {
            if component.pins.contains_key(alias) {
                return Some(alias.to_string());
            }
        }

        None
    }
}

impl Default for SugiyamaLayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}
