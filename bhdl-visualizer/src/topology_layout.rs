//! Topology-aware circuit layout engine
//!
//! This module implements human-readable schematic layout based on circuit topology
//! and signal flow analysis, rather than arbitrary geometric placement.

use std::collections::{HashMap, HashSet, VecDeque};
use anyhow::{Result, Context, bail};
use log::{debug, info};

use bhdl_netlist::{Netlist, InstanceId, NetId, ConnectionPoint};
use bhdl_synthesizer::DatabaseComponentInstance;
use crate::types::{Point, Component, Net, NetType, RoutingSegment, CircuitLayout};
use crate::symbols::SymbolManager;

/// Component role in the circuit (for intelligent placement)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComponentRole {
    PowerSource,    // Voltage sources, batteries
    Regulator,      // Voltage regulators (LM7805, etc.)
    Filter,         // Capacitors for filtering
    Protection,     // TVS diodes, fuses
    Load,          // LEDs, motors, outputs
    Passive,       // Resistors, general passives
    Ground,        // Ground symbols/connections
    Unknown,       // Cannot classify
}

/// Circuit topology graph for analysis
#[derive(Debug)]
pub struct CircuitGraph {
    /// Component nodes
    pub nodes: HashMap<InstanceId, GraphNode>,
    /// Net connections between components
    pub edges: Vec<(InstanceId, InstanceId, NetId)>,
    /// Power domain nets
    pub power_nets: HashSet<NetId>,
    /// Ground nets
    pub ground_nets: HashSet<NetId>,
}

/// Node in the circuit graph
#[derive(Debug, Clone)]
pub struct GraphNode {
    pub instance_id: InstanceId,
    pub component_type: String,
    pub role: ComponentRole,
    pub connected_to: Vec<InstanceId>,
}

impl CircuitGraph {
    /// Build circuit graph from netlist
    pub fn from_netlist(
        netlist: &Netlist,
        db_components: &[DatabaseComponentInstance],
    ) -> Result<Self> {
        debug!("Building circuit graph from netlist");

        let mut nodes = HashMap::new();
        let mut edges = Vec::new();
        let mut power_nets = HashSet::new();
        let mut ground_nets = HashSet::new();

        // Create type lookup
        let type_map: HashMap<String, &DatabaseComponentInstance> = db_components
            .iter()
            .map(|c| (c.bhdl_type.clone(), c))
            .collect();

        // Build nodes - classify each component
        for (instance_id, instance) in &netlist.instances {
            let module_def = netlist.get_module(instance.definition)
                .with_context(|| format!("Module definition not found for {}", instance.name))?;

            let component_type = module_def.name.clone();
            let role = classify_component(&component_type);

            debug!("  Instance {}: type={}, role={:?}", instance.name, component_type, role);

            nodes.insert(instance_id, GraphNode {
                instance_id,
                component_type,
                role,
                connected_to: Vec::new(),
            });
        }

        // Build edges - find which components connect through nets
        for (net_id, net) in &netlist.nets {
            // Classify net type
            match &net.net_class {
                bhdl_netlist::types::NetClass::Power(_) => {
                    power_nets.insert(net_id);
                }
                bhdl_netlist::types::NetClass::Ground => {
                    ground_nets.insert(net_id);
                }
                _ => {}
            }

            // Extract instances connected by this net
            let mut connected_instances = HashSet::new();
            for connection in &net.connections {
                match connection {
                    ConnectionPoint::InstancePin(inst_id, _) => {
                        connected_instances.insert(*inst_id);
                    }
                    ConnectionPoint::PinInstance(pin_inst_id) => {
                        if let Some(pin_inst) = netlist.get_pin_instance(*pin_inst_id) {
                            connected_instances.insert(pin_inst.instance);
                        }
                    }
                    _ => {}
                }
            }

            // Create edges between all pairs on this net
            let instances: Vec<_> = connected_instances.into_iter().collect();
            for i in 0..instances.len() {
                for j in (i+1)..instances.len() {
                    edges.push((instances[i], instances[j], net_id));
                    edges.push((instances[j], instances[i], net_id));
                }
            }
        }

        // Update connected_to lists in nodes
        for (from, to, _) in &edges {
            if let Some(node) = nodes.get_mut(from) {
                if !node.connected_to.contains(to) {
                    node.connected_to.push(*to);
                }
            }
        }

        info!("Circuit graph: {} nodes, {} edges, {} power nets, {} ground nets",
              nodes.len(), edges.len(), power_nets.len(), ground_nets.len());

        Ok(CircuitGraph {
            nodes,
            edges,
            power_nets,
            ground_nets,
        })
    }

    /// Get all components connected to this component
    pub fn get_connected_components(&self, instance_id: InstanceId) -> Vec<InstanceId> {
        self.nodes.get(&instance_id)
            .map(|node| node.connected_to.clone())
            .unwrap_or_default()
    }
}

/// Classify component by type string
fn classify_component(component_type: &str) -> ComponentRole {
    match component_type {
        // Power sources
        "VoltageSource" | "PowerSupply" | "Battery" => ComponentRole::PowerSource,

        // Voltage regulators
        t if t.contains("7805") || t.contains("7812") ||
             t.contains("LM78") || t.contains("LM317") ||
             t.contains("AMS1117") => ComponentRole::Regulator,

        // Protection devices
        "TVSDiode" | "Fuse" | "PTC" => ComponentRole::Protection,

        // Capacitors (usually filters in power supplies)
        "Cap" | "Capacitor" => ComponentRole::Filter,

        // Loads
        "LED" | "Motor" | "Buzzer" => ComponentRole::Load,

        // Passive components
        "Res" | "Resistor" | "Inductor" => ComponentRole::Passive,

        // Ground symbols
        "Ground" | "GND" | "PowerFlag" | "0V" | "EARTH" => ComponentRole::Ground,

        // Power symbols
        "VCC" | "VDD" | "VIN" | "VOUT" | "5V" | "3V3" | "12V" => ComponentRole::PowerSource,

        _ => ComponentRole::Unknown,
    }
}

/// Compute signal flow stages using BFS from power sources
pub fn compute_signal_flow_stages(graph: &CircuitGraph) -> HashMap<InstanceId, usize> {
    debug!("Computing signal flow stages");

    let mut stages = HashMap::new();
    let mut queue = VecDeque::new();

    // Find starting points: power sources and protection at input
    for (instance_id, node) in &graph.nodes {
        match node.role {
            ComponentRole::PowerSource | ComponentRole::Protection => {
                queue.push_back((*instance_id, 0));
                stages.insert(*instance_id, 0);
                debug!("  Starting point: {:?} at stage 0", node.component_type);
            }
            _ => {}
        }
    }

    // If no power sources found, use any component as starting point
    if queue.is_empty() {
        if let Some((instance_id, _)) = graph.nodes.iter().next() {
            queue.push_back((*instance_id, 0));
            stages.insert(*instance_id, 0);
            debug!("  No power sources found, starting from first component");
        }
    }

    // BFS traversal
    while let Some((current_id, stage)) = queue.pop_front() {
        for &connected_id in &graph.nodes[&current_id].connected_to {
            // Skip ground connections for stage assignment
            if matches!(graph.nodes[&connected_id].role, ComponentRole::Ground) {
                continue;
            }

            if !stages.contains_key(&connected_id) {
                let new_stage = stage + 1;
                stages.insert(connected_id, new_stage);
                queue.push_back((connected_id, new_stage));
                debug!("  {:?} assigned to stage {}",
                       graph.nodes[&connected_id].component_type, new_stage);
            }
        }
    }

    // Assign ground components to final stage
    let max_stage = stages.values().max().copied().unwrap_or(0);
    for (instance_id, node) in &graph.nodes {
        if matches!(node.role, ComponentRole::Ground) && !stages.contains_key(instance_id) {
            stages.insert(*instance_id, max_stage);
        }
    }

    info!("Assigned {} components to {} stages", stages.len(), max_stage + 1);

    stages
}

/// Topology-aware layout engine
pub struct TopologyLayoutEngine {
    spacing: f64,
    vertical_spacing: f64,
    ground_rail_y: f64,
    power_rail_y: f64,
    symbol_manager: SymbolManager,
}

impl TopologyLayoutEngine {
    /// Create new topology layout engine
    pub fn new() -> Self {
        Self {
            spacing: 1200.0,           // Horizontal spacing between stages
            vertical_spacing: 600.0,    // Vertical spacing within stage
            ground_rail_y: 1000.0,      // Y position of ground rail
            power_rail_y: -800.0,       // Y position of power rail (if needed)
            symbol_manager: SymbolManager::new(),
        }
    }

    /// Generate complete circuit layout from netlist
    pub async fn layout_circuit(
        &mut self,
        netlist: &Netlist,
        db_components: &[DatabaseComponentInstance],
    ) -> Result<CircuitLayout> {
        info!("Starting topology-aware circuit layout");

        // Phase 1: Build circuit graph
        let graph = CircuitGraph::from_netlist(netlist, db_components)?;

        // Phase 2: Compute signal flow stages
        let stages = compute_signal_flow_stages(&graph);

        // Phase 3: Place components by topology
        let components = self.place_components_by_topology(
            &graph,
            &stages,
            netlist,
            db_components,
        ).await?;

        // Build layout
        let mut layout = CircuitLayout::new();
        layout.grid_spacing = 100.0;

        for component in components {
            layout.add_component(component);
        }

        // Phase 4: Route nets with power rails
        let nets = self.route_nets_with_rails(
            netlist,
            &layout,
            &graph,
            db_components,
        ).await?;

        for net in nets {
            layout.add_net(net);
        }

        layout.update_bounding_box();

        info!("Layout complete: {} components, {} nets",
              layout.components.len(), layout.nets.len());

        Ok(layout)
    }

    /// Place components based on topology and signal flow
    async fn place_components_by_topology(
        &mut self,
        graph: &CircuitGraph,
        stages: &HashMap<InstanceId, usize>,
        netlist: &Netlist,
        db_components: &[DatabaseComponentInstance],
    ) -> Result<Vec<Component>> {
        debug!("Placing components by topology");

        let mut components = Vec::new();
        let max_stage = *stages.values().max().unwrap_or(&0);

        // Create type lookup
        let type_map: HashMap<String, &DatabaseComponentInstance> = db_components
            .iter()
            .map(|c| (c.bhdl_type.clone(), c))
            .collect();

        // Group components by stage
        let mut stage_groups: HashMap<usize, Vec<InstanceId>> = HashMap::new();
        for (&instance_id, &stage) in stages {
            stage_groups.entry(stage).or_default().push(instance_id);
        }

        // Place each stage left to right
        let empty_vec = Vec::new();
        for stage in 0..=max_stage {
            let x = stage as f64 * self.spacing;
            let components_in_stage = stage_groups.get(&stage).unwrap_or(&empty_vec);

            debug!("Stage {}: {} components at x={}", stage, components_in_stage.len(), x);

            // Separate by role within stage for vertical positioning
            let mut power_comps = vec![];
            let mut regulator_comps = vec![];
            let mut signal_comps = vec![];
            let mut ground_comps = vec![];

            for &id in components_in_stage {
                let node = &graph.nodes[&id];
                match node.role {
                    ComponentRole::Ground => ground_comps.push(id),
                    ComponentRole::PowerSource => power_comps.push(id),
                    ComponentRole::Regulator => regulator_comps.push(id),
                    _ => signal_comps.push(id),
                }
            }

            // Place regulator components on centerline (y=0) - main signal path
            for (i, &instance_id) in regulator_comps.iter().enumerate() {
                let y = i as f64 * self.vertical_spacing * 0.3;
                let component = self.create_component_at_position(
                    instance_id,
                    Point::new(x, y),
                    netlist,
                    &type_map,
                ).await?;
                components.push(component);
            }

            // Place signal components (filters, loads, etc.) below regulator
            for (i, &instance_id) in signal_comps.iter().enumerate() {
                let y = self.vertical_spacing * 0.6 + i as f64 * self.vertical_spacing * 0.4;
                let component = self.create_component_at_position(
                    instance_id,
                    Point::new(x, y),
                    netlist,
                    &type_map,
                ).await?;
                components.push(component);
            }

            // Place power symbols slightly above ground rail (near top of signal components)
            for (i, &instance_id) in power_comps.iter().enumerate() {
                let y = self.vertical_spacing * 1.0 + i as f64 * self.vertical_spacing * 0.3;
                let component = self.create_component_at_position(
                    instance_id,
                    Point::new(x, y),
                    netlist,
                    &type_map,
                ).await?;
                components.push(component);
            }

            // Place ground symbols just above ground rail (visible but close to rail)
            for (i, &instance_id) in ground_comps.iter().enumerate() {
                let y = self.ground_rail_y - 200.0 - i as f64 * 100.0;  // Above rail
                let component = self.create_component_at_position(
                    instance_id,
                    Point::new(x, y),
                    netlist,
                    &type_map,
                ).await?;
                components.push(component);
            }
        }

        debug!("Placed {} components total", components.len());
        Ok(components)
    }

    /// Create component at specific position
    async fn create_component_at_position(
        &mut self,
        instance_id: InstanceId,
        position: Point,
        netlist: &Netlist,
        type_map: &HashMap<String, &DatabaseComponentInstance>,
    ) -> Result<Component> {
        let instance = netlist.get_instance(instance_id)
            .with_context(|| format!("Instance {:?} not found", instance_id))?;

        let module_def = netlist.get_module(instance.definition)
            .with_context(|| format!("Module not found for {}", instance.name))?;

        let db_component = type_map.get(&module_def.name)
            .with_context(|| format!("Database component not found for type {}", module_def.name))?;

        self.symbol_manager.create_component(
            instance_id,
            db_component,
            position,
            0.0,  // rotation
        ).await
    }

    /// Route nets with power rails and intelligent routing
    async fn route_nets_with_rails(
        &mut self,
        netlist: &Netlist,
        layout: &CircuitLayout,
        graph: &CircuitGraph,
        db_components: &[DatabaseComponentInstance],
    ) -> Result<Vec<Net>> {
        debug!("Routing nets with power rails");

        let mut routed_nets = Vec::new();

        // Route ground nets with ground rail
        for &net_id in &graph.ground_nets {
            if let Some(netlist_net) = netlist.nets.get(net_id) {
                let mut net = Net::with_type(net_id, netlist_net.name.clone(), NetType::Ground);

                let connection_points = self.collect_connection_points(
                    netlist_net,
                    layout,
                    netlist,
                    db_components,
                )?;

                if connection_points.len() < 2 {
                    continue;
                }

                // Create ground rail routing
                let min_x = connection_points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
                let max_x = connection_points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);

                // Horizontal rail
                net.add_routing_segment(RoutingSegment::line(
                    Point::new(min_x - 100.0, self.ground_rail_y),
                    Point::new(max_x + 100.0, self.ground_rail_y),
                ));

                // Vertical stubs from pins to rail
                for &pin_pos in &connection_points {
                    net.add_connection_point(pin_pos);
                    let rail_point = Point::new(pin_pos.x, self.ground_rail_y);
                    net.add_routing_segment(RoutingSegment::line(pin_pos, rail_point));
                }

                routed_nets.push(net);
            }
        }

        // Route power nets
        for &net_id in &graph.power_nets {
            if let Some(netlist_net) = netlist.nets.get(net_id) {
                let mut net = Net::with_type(net_id, netlist_net.name.clone(), NetType::Power);

                let connection_points = self.collect_connection_points(
                    netlist_net,
                    layout,
                    netlist,
                    db_components,
                )?;

                if connection_points.len() < 2 {
                    continue;
                }

                // Use orthogonal routing for power nets
                let segments = self.route_orthogonal(&connection_points);
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

            let mut net = Net::with_type(net_id, netlist_net.name.clone(), NetType::Signal);

            let segments = self.route_orthogonal(&connection_points);
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
                            points.push(pin_pos);
                        }
                    }
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

    /// Orthogonal routing between connection points
    fn route_orthogonal(&self, points: &[Point]) -> Vec<RoutingSegment> {
        let mut segments = Vec::new();

        if points.len() == 2 {
            // Two-point routing: L-shaped
            let start = points[0];
            let end = points[1];

            // Choose routing direction based on positions
            if (end.x - start.x).abs() > (end.y - start.y).abs() {
                // Horizontal distance larger: go horizontal first
                let mid = Point::new(end.x, start.y);
                segments.push(RoutingSegment::line(start, mid));
                segments.push(RoutingSegment::line(mid, end));
            } else {
                // Vertical distance larger: go vertical first
                let mid = Point::new(start.x, end.y);
                segments.push(RoutingSegment::line(start, mid));
                segments.push(RoutingSegment::line(mid, end));
            }
        } else if points.len() > 2 {
            // Multi-point: create bus along average Y
            let avg_y = points.iter().map(|p| p.y).sum::<f64>() / points.len() as f64;

            let mut sorted = points.to_vec();
            sorted.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));

            for (i, &point) in sorted.iter().enumerate() {
                // Vertical stub to bus
                if (point.y - avg_y).abs() > 1.0 {
                    segments.push(RoutingSegment::line(point, Point::new(point.x, avg_y)));
                }

                // Horizontal segment to next
                if i < sorted.len() - 1 {
                    let next = sorted[i + 1];
                    segments.push(RoutingSegment::line(
                        Point::new(point.x, avg_y),
                        Point::new(next.x, avg_y),
                    ));
                }
            }
        }

        segments
    }
}
