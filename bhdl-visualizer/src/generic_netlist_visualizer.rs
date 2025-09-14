/// Fully generic netlist visualizer that works with any circuit
/// Uses all available metadata from the synthesis pipeline
use crate::schematic_knowledge::schematic_knowledge::SchematicKnowledge;
use crate::simple_svg_renderer::SimpleSvgRenderer;
use crate::types::{Point, Component, Net, CircuitLayout};
use bhdl_netlist::{Netlist, InstanceId, NetId, ConnectionPoint};
use bhdl_synthesizer::DatabaseComponentInstance;
use std::collections::{HashMap, HashSet};

pub struct GenericNetlistVisualizer {
    knowledge: SchematicKnowledge,
    grid_size: f64,
    current_x: f64,
    current_y: f64,
    component_positions: HashMap<InstanceId, ComponentInfo>,
    net_connections: HashMap<NetId, Vec<ConnectionInfo>>,
}

#[derive(Debug, Clone)]
struct ComponentInfo {
    position: Point,
    width: f64,
    height: f64,
    pins: HashMap<String, Point>, // Pin name -> absolute position
    role: ComponentRole,
    instance_name: String,
}

#[derive(Debug, Clone)]
struct ConnectionInfo {
    instance_id: InstanceId,
    pin_name: String,
    position: Point,
}

#[derive(Debug, Clone, PartialEq)]
enum ComponentRole {
    PowerRegulator,
    InputFilter,
    OutputFilter,
    Protection,
    Load,
    Passive,
    Unknown,
}

impl GenericNetlistVisualizer {
    pub fn new() -> Self {
        Self {
            knowledge: SchematicKnowledge::new(),
            grid_size: 25.0,
            current_x: 100.0,
            current_y: 100.0,
            component_positions: HashMap::new(),
            net_connections: HashMap::new(),
        }
    }
    
    /// Generate layout from netlist and component database info
    pub fn generate_layout(
        &mut self,
        netlist: &Netlist,
        db_components: &[DatabaseComponentInstance],
    ) -> CircuitLayout {
        let mut layout = CircuitLayout::new();
        
        // Step 1: Analyze the circuit structure
        self.analyze_circuit(netlist, db_components);
        
        // Step 2: Determine component placement order based on signal flow
        let placement_order = self.determine_placement_order(netlist);
        
        // Step 3: Place components intelligently
        for instance_id in placement_order {
            self.place_component(instance_id, netlist, db_components, &mut layout);
        }
        
        // Step 4: Route nets using actual connectivity
        self.route_nets(netlist, &mut layout);
        
        // Step 5: Add power and ground rails
        self.add_power_rails(netlist, &mut layout);
        
        layout.update_bounding_box();
        layout
    }
    
    /// Analyze circuit to understand structure and roles
    fn analyze_circuit(&mut self, netlist: &Netlist, db_components: &[DatabaseComponentInstance]) {
        // Build a map of instance names to database components
        let db_map: HashMap<String, &DatabaseComponentInstance> = db_components
            .iter()
            .map(|c| (c.instance_name.clone(), c))
            .collect();
        
        // Analyze each instance
        for (instance_id, instance) in &netlist.instances {
            let role = self.determine_component_role(&instance.name, netlist, &db_map);
            
            // Get component size from database or use defaults
            let (width, height) = if let Some(db_comp) = db_map.get(&instance.name) {
                self.get_component_size(db_comp)
            } else {
                self.get_default_size(&instance.name)
            };
            
            let info = ComponentInfo {
                position: Point::new(0.0, 0.0), // Will be set during placement
                width,
                height,
                pins: HashMap::new(), // Will be populated during placement
                role,
                instance_name: instance.name.clone(),
            };
            
            self.component_positions.insert(instance_id, info);
        }
        
        // Build net connection map
        for (net_id, net) in &netlist.nets {
            let mut connections = Vec::new();
            
            for conn_point in &net.connections {
                if let ConnectionPoint::PinInstance(pin_inst_id) = conn_point {
                    if let Some(pin_inst) = netlist.pin_instances.get(*pin_inst_id) {
                        connections.push(ConnectionInfo {
                            instance_id: pin_inst.instance,
                            pin_name: self.get_pin_name(pin_inst.pin_def, netlist),
                            position: Point::new(0.0, 0.0), // Will be calculated
                        });
                    }
                }
            }
            
            self.net_connections.insert(net_id, connections);
        }
    }
    
    /// Determine component role from metadata and topology
    fn determine_component_role(
        &self,
        instance_name: &str,
        netlist: &Netlist,
        db_map: &HashMap<String, &DatabaseComponentInstance>,
    ) -> ComponentRole {
        // First check if we have analysis data with explicit roles
        if let Some(analysis_data) = &netlist.analysis_data {
            if let Some(instance_analysis) = analysis_data.instance_analysis.get(instance_name) {
                if let Some(role_str) = &instance_analysis.component_role {
                    return match role_str.as_str() {
                        s if s.contains("InputFilter") => ComponentRole::InputFilter,
                        s if s.contains("OutputStabilization") => ComponentRole::OutputFilter,
                        s if s.contains("OutputFilter") => ComponentRole::OutputFilter,
                        s if s.contains("Protection") => ComponentRole::Protection,
                        s if s.contains("Load") => ComponentRole::Load,
                        _ => ComponentRole::Passive,
                    };
                }
            }
        }
        
        // Fallback to component type-based classification
        if let Some(db_comp) = db_map.get(instance_name) {
            use bhdl_synthesizer::component_mapping::ComponentCategory;
            return match db_comp.category {
                ComponentCategory::PowerRegulator => ComponentRole::PowerRegulator,
                ComponentCategory::PassiveCapacitor => ComponentRole::Passive, // Will be refined by topology
                ComponentCategory::PassiveResistor => ComponentRole::Passive,
                ComponentCategory::Semiconductor => ComponentRole::Passive, // Includes diodes, LEDs
                ComponentCategory::Connector => ComponentRole::Unknown,
                ComponentCategory::Crystal => ComponentRole::Unknown,
                ComponentCategory::Unknown => ComponentRole::Unknown,
            };
        }
        
        // Last resort: name-based heuristics
        if instance_name.starts_with("U") || instance_name.starts_with("IC") {
            ComponentRole::PowerRegulator
        } else if instance_name.starts_with("D") && instance_name.contains("LED") {
            ComponentRole::Load
        } else {
            ComponentRole::Passive
        }
    }
    
    /// Get component size from database metadata
    fn get_component_size(&self, db_comp: &DatabaseComponentInstance) -> (f64, f64) {
        // Try to get size from embedded visualization metadata
        if let Some(rules) = self.knowledge.get_component_rules(&db_comp.bhdl_type) {
            if let Some((width, height)) = rules.get_symbol_size() {
                return (width, height);
            }
        }
        
        // Use category-based defaults
        use bhdl_synthesizer::component_mapping::ComponentCategory;
        match db_comp.category {
            ComponentCategory::PowerRegulator => (80.0, 50.0),
            ComponentCategory::PassiveCapacitor => (15.0, 30.0), // Vertical
            ComponentCategory::PassiveResistor => (40.0, 15.0),  // Horizontal
            ComponentCategory::Semiconductor => (20.0, 25.0), // Diodes, LEDs (vertical)
            ComponentCategory::Connector => (30.0, 20.0),
            ComponentCategory::Crystal => (30.0, 15.0),
            _ => (30.0, 20.0),
        }
    }
    
    /// Get default size based on instance name
    fn get_default_size(&self, instance_name: &str) -> (f64, f64) {
        if instance_name.starts_with("U") || instance_name.starts_with("IC") {
            (80.0, 50.0) // IC
        } else if instance_name.starts_with("C") {
            (15.0, 30.0) // Capacitor (vertical)
        } else if instance_name.starts_with("R") {
            (40.0, 15.0) // Resistor (horizontal)
        } else if instance_name.starts_with("L") {
            (40.0, 20.0) // Inductor (horizontal)
        } else if instance_name.starts_with("D") {
            (20.0, 25.0) // Diode/LED (vertical)
        } else {
            (30.0, 20.0) // Generic
        }
    }
    
    /// Determine placement order based on signal flow
    fn determine_placement_order(&self, netlist: &Netlist) -> Vec<InstanceId> {
        let mut order = Vec::new();
        let mut placed = HashSet::new();
        
        // First place power regulators / ICs
        for (id, info) in &self.component_positions {
            if info.role == ComponentRole::PowerRegulator {
                order.push(*id);
                placed.insert(*id);
            }
        }
        
        // Then place input filters (connected to IC inputs)
        for (id, info) in &self.component_positions {
            if info.role == ComponentRole::InputFilter && !placed.contains(id) {
                order.push(*id);
                placed.insert(*id);
            }
        }
        
        // Then place output filters (connected to IC outputs)
        for (id, info) in &self.component_positions {
            if info.role == ComponentRole::OutputFilter && !placed.contains(id) {
                order.push(*id);
                placed.insert(*id);
            }
        }
        
        // Then place remaining components
        for (id, _) in &self.component_positions {
            if !placed.contains(id) {
                order.push(*id);
            }
        }
        
        order
    }
    
    /// Place a component in the layout
    fn place_component(
        &mut self,
        instance_id: InstanceId,
        netlist: &Netlist,
        db_components: &[DatabaseComponentInstance],
        layout: &mut CircuitLayout,
    ) {
        let instance = netlist.instances.get(instance_id).unwrap();
        let mut info = self.component_positions.get(&instance_id).unwrap().clone();
        
        // Determine position based on role and connectivity
        let position = match info.role {
            ComponentRole::PowerRegulator => {
                // Center of layout, advance for next IC
                let pos = Point::new(self.current_x, 150.0);
                self.current_x += info.width + 100.0;
                pos
            }
            ComponentRole::InputFilter => {
                // Find the IC this is connected to and place to its left
                let ic_pos = self.find_connected_ic_position(instance_id, netlist);
                Point::new(ic_pos.x - 50.0 - info.width/2.0, ic_pos.y)
            }
            ComponentRole::OutputFilter => {
                // Find the IC this is connected to and place to its right
                let ic_pos = self.find_connected_ic_position(instance_id, netlist);
                Point::new(ic_pos.x + 50.0 + info.width/2.0, ic_pos.y)
            }
            _ => {
                // Place in sequence
                let pos = Point::new(self.current_x, 200.0);
                self.current_x += info.width + 30.0;
                pos
            }
        };
        
        info.position = position;
        
        // Calculate pin positions
        self.calculate_pin_positions(&mut info, netlist, db_components);
        
        // Add to layout
        layout.add_component(
            Component::new(instance_id, position)
                .with_label(instance.name.clone())
                .with_size(info.width, info.height)
        );
        
        // Update our tracking
        self.component_positions.insert(instance_id, info);
    }
    
    /// Find position of connected IC for relative placement
    fn find_connected_ic_position(&self, component_id: InstanceId, netlist: &Netlist) -> Point {
        // Find nets this component is connected to
        for (_, connections) in &self.net_connections {
            let has_component = connections.iter().any(|c| c.instance_id == component_id);
            if has_component {
                // Find if an IC is also on this net
                for conn in connections {
                    if let Some(info) = self.component_positions.get(&conn.instance_id) {
                        if info.role == ComponentRole::PowerRegulator {
                            return info.position;
                        }
                    }
                }
            }
        }
        
        // Default position if no IC found
        Point::new(self.current_x, 150.0)
    }
    
    /// Calculate actual pin positions for a component
    fn calculate_pin_positions(
        &self,
        info: &mut ComponentInfo,
        netlist: &Netlist,
        db_components: &[DatabaseComponentInstance],
    ) {
        // Simple pin placement based on component type
        // In a real system, this would use the database pin definitions
        
        if info.role == ComponentRole::PowerRegulator {
            // IC: IN on left, OUT on right, GND on bottom
            info.pins.insert("IN".to_string(), Point::new(info.position.x - info.width/2.0, info.position.y));
            info.pins.insert("OUT".to_string(), Point::new(info.position.x + info.width/2.0, info.position.y));
            info.pins.insert("GND".to_string(), Point::new(info.position.x, info.position.y + info.height/2.0));
        } else if info.instance_name.starts_with("C") || info.instance_name.starts_with("R") {
            // Two-terminal passive: pins at ends
            if info.width > info.height {
                // Horizontal
                info.pins.insert("1".to_string(), Point::new(info.position.x - info.width/2.0, info.position.y));
                info.pins.insert("2".to_string(), Point::new(info.position.x + info.width/2.0, info.position.y));
            } else {
                // Vertical
                info.pins.insert("1".to_string(), Point::new(info.position.x, info.position.y - info.height/2.0));
                info.pins.insert("2".to_string(), Point::new(info.position.x, info.position.y + info.height/2.0));
            }
        }
    }
    
    /// Get pin name from pin ID
    fn get_pin_name(&self, pin_id: bhdl_netlist::PinId, netlist: &Netlist) -> String {
        netlist.pins.get(pin_id)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "?".to_string())
    }
    
    /// Route all nets based on actual connectivity
    fn route_nets(&mut self, netlist: &Netlist, layout: &mut CircuitLayout) {
        for (net_id, net) in &netlist.nets {
            if let Some(connections) = self.net_connections.get(&net_id) {
                if connections.len() < 2 {
                    continue;
                }
                
                let mut net_obj = Net::new(net_id, net.name.clone());
                
                // Get actual pin positions for routing
                for conn in connections {
                    if let Some(comp_info) = self.component_positions.get(&conn.instance_id) {
                        if let Some(pin_pos) = comp_info.pins.get(&conn.pin_name) {
                            net_obj.add_connection_point(*pin_pos);
                        }
                    }
                }
                
                if net_obj.connection_points.len() >= 2 {
                    layout.add_net(net_obj);
                }
            }
        }
    }
    
    /// Add power and ground rails
    fn add_power_rails(&self, netlist: &Netlist, layout: &mut CircuitLayout) {
        // Find bounds of all components
        let mut min_x = f64::MAX;
        let mut max_x = f64::MIN;
        
        for info in self.component_positions.values() {
            min_x = min_x.min(info.position.x - info.width/2.0 - 50.0);
            max_x = max_x.max(info.position.x + info.width/2.0 + 50.0);
        }
        
        // Add power rail at top
        for (net_id, net) in &netlist.nets {
            if let Some(name) = &net.name {
                if name.contains("VCC") || name.contains("VDD") || name.contains("VIN") {
                    let mut power_net = Net::new(net_id, Some(name.clone()));
                    power_net.add_connection_point(Point::new(min_x, 50.0));
                    power_net.add_connection_point(Point::new(max_x, 50.0));
                    layout.add_net(power_net);
                    break;
                }
            }
        }
        
        // Add ground rail at bottom
        for (net_id, net) in &netlist.nets {
            if let Some(name) = &net.name {
                if name.contains("GND") || name.contains("VSS") {
                    let mut ground_net = Net::new(net_id, Some(name.clone()));
                    ground_net.add_connection_point(Point::new(min_x, 300.0));
                    ground_net.add_connection_point(Point::new(max_x, 300.0));
                    layout.add_net(ground_net);
                    break;
                }
            }
        }
    }
}

impl crate::schematic_knowledge::schematic_knowledge::ComponentVisualization {
    /// Helper to get symbol size
    pub fn get_symbol_size(&self) -> Option<(f64, f64)> {
        match &self.symbol_style {
            crate::schematic_knowledge::schematic_knowledge::SymbolStyle::Rectangle { width, height, .. } => {
                Some((*width, *height))
            }
            crate::schematic_knowledge::schematic_knowledge::SymbolStyle::Triangle { width, height } => {
                Some((*width, *height))
            }
            _ => None,
        }
    }
}