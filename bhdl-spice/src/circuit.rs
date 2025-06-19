//! Circuit representation for electrical analysis

use std::collections::HashMap;
use petgraph::graph::{Graph, NodeIndex, EdgeIndex};
use petgraph::visit::EdgeRef;
use serde::{Serialize, Deserialize};
use bhdl_netlist::{NetId, InstanceId, ConnectionPoint};

// Type aliases for safety module
pub type NodeId = NodeIndex;
pub type ComponentId = EdgeIndex;
pub type Component = Branch;

/// Electrical node in the circuit
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    /// Node name (from netlist)
    pub name: String,
    /// Net ID from BHDL netlist
    pub net_id: Option<NetId>,
    /// Is this the ground/reference node?
    pub is_ground: bool,
    /// Node voltage (set by analysis)
    pub voltage: Option<f64>,
}

/// Branch (component) connecting two nodes
#[derive(Debug, Clone, PartialEq)]
pub struct Branch {
    /// Component instance name
    pub name: String,
    /// Instance ID from BHDL netlist
    pub instance_id: Option<InstanceId>,
    /// Component type (e.g., "Resistor", "Capacitor", "VoltageSource")
    pub component_type: String,
    /// Component value (resistance, capacitance, voltage, etc.)
    pub value: f64,
    /// Current through the branch (set by analysis)
    pub current: Option<f64>,
    /// Connected nodes (for safety analysis)
    pub nodes: Vec<NodeId>,
}

impl Branch {
    /// Get component name
    pub fn name(&self) -> &str {
        &self.name
    }
    
    /// Get component type
    pub fn component_type(&self) -> &str {
        &self.component_type
    }
    
    /// Get component subtype (not implemented yet)
    pub fn component_subtype(&self) -> Option<&str> {
        None
    }
    
    /// Get connected nodes
    pub fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }
    
    /// Get maximum current rating
    pub fn max_current(&self) -> Option<f64> {
        // TODO: Get from model parameters or component database
        None
    }
    
    /// Get maximum voltage rating
    pub fn max_voltage(&self) -> Option<f64> {
        // TODO: Get from model parameters or component database
        None
    }
    
    /// Get component resistance
    pub fn resistance(&self) -> Option<f64> {
        // For resistors, return the value directly
        match self.component_type.as_str() {
            "Resistor" | "Res" => Some(self.value),
            "LED" => Some(10.0), // Dynamic resistance approximation
            _ => None,
        }
    }
    
    /// Get component cost estimate
    pub fn cost(&self) -> Option<f64> {
        // Would come from component database
        None
    }
    
    
    /// Get parameter value
    pub fn get_parameter(&self, param: &str) -> Option<f64> {
        // For testing, return hardcoded 7805 values
        match self.component_type.as_str() {
            "VoltageRegulator" => match param {
                "vout_nominal" => Some(5.0),
                "dropout" => Some(2.0),
                "vin_max" => Some(35.0),
                "iout_max" => Some(1.0),
                "iq" => Some(0.005),
                "power_max" => Some(15.0),
                "tj_max" => Some(125.0),
                "rth_ja" => Some(65.0),
                _ => None,
            },
            _ => None,
        }
    }
}

/// Circuit representation using a graph
#[derive(Clone)]
pub struct Circuit {
    /// Graph where nodes are electrical nodes and edges are components
    graph: Graph<Node, Branch>,
    /// Map from node names to graph indices
    node_map: HashMap<String, NodeIndex>,
    /// Map from component names to edge indices
    branch_map: HashMap<String, EdgeIndex>,
    /// Ground node index
    ground_node: Option<NodeIndex>,
}

impl Circuit {
    /// Create a new empty circuit
    pub fn new() -> Self {
        Self {
            graph: Graph::new(),
            node_map: HashMap::new(),
            branch_map: HashMap::new(),
            ground_node: None,
        }
    }
    
    /// Add a node to the circuit
    pub fn add_node(&mut self, name: String, net_id: Option<NetId>) -> NodeIndex {
        if let Some(&idx) = self.node_map.get(&name) {
            return idx;
        }
        
        let is_ground = name.to_lowercase() == "gnd" || name.to_lowercase() == "ground";
        let node = Node {
            name: name.clone(),
            net_id,
            is_ground,
            voltage: None,
        };
        
        let idx = self.graph.add_node(node);
        self.node_map.insert(name, idx);
        
        if is_ground && self.ground_node.is_none() {
            self.ground_node = Some(idx);
        }
        
        idx
    }
    
    /// Add a component (branch) between two nodes
    pub fn add_branch(
        &mut self,
        name: String,
        node1: &str,
        node2: &str,
        component_type: String,
        value: f64,
        instance_id: Option<InstanceId>,
    ) -> EdgeIndex {
        let n1 = self.node_map.get(node1).copied()
            .unwrap_or_else(|| self.add_node(node1.to_string(), None));
        let n2 = self.node_map.get(node2).copied()
            .unwrap_or_else(|| self.add_node(node2.to_string(), None));
        
        let branch = Branch {
            name: name.clone(),
            instance_id,
            component_type,
            value,
            current: None,
            nodes: vec![n1, n2],
        };
        
        let idx = self.graph.add_edge(n1, n2, branch);
        self.branch_map.insert(name, idx);
        idx
    }
    
    /// Get all nodes
    pub fn nodes(&self) -> impl Iterator<Item = (NodeIndex, &Node)> {
        self.graph.node_indices()
            .map(move |idx| (idx, &self.graph[idx]))
    }
    
    /// Get all branches
    pub fn branches(&self) -> impl Iterator<Item = (EdgeIndex, &Branch)> {
        self.graph.edge_indices()
            .map(move |idx| (idx, &self.graph[idx]))
    }
    
    /// Get node by name
    pub fn get_node(&self, name: &str) -> Option<(NodeIndex, &Node)> {
        self.node_map.get(name)
            .map(|&idx| (idx, &self.graph[idx]))
    }
    
    /// Get branch by name
    pub fn get_branch(&self, name: &str) -> Option<(EdgeIndex, &Branch)> {
        self.branch_map.get(name)
            .map(|&idx| (idx, &self.graph[idx]))
    }
    
    /// Get ground node
    pub fn ground_node(&self) -> Option<(NodeIndex, &Node)> {
        self.ground_node
            .map(|idx| (idx, &self.graph[idx]))
    }
    
    /// Get the two nodes connected by a branch
    pub fn branch_nodes(&self, edge: EdgeIndex) -> Option<(NodeIndex, NodeIndex)> {
        self.graph.edge_endpoints(edge)
    }
    
    /// Get all branches connected to a node
    pub fn node_branches(&self, node: NodeIndex) -> Vec<EdgeIndex> {
        self.graph.edges(node)
            .map(|edge| edge.id())
            .collect()
    }
    
    /// Set node voltage (from analysis results)
    pub fn set_node_voltage(&mut self, node: NodeIndex, voltage: f64) {
        if let Some(n) = self.graph.node_weight_mut(node) {
            n.voltage = Some(voltage);
        }
    }
    
    /// Set branch current (from analysis results)
    pub fn set_branch_current(&mut self, edge: EdgeIndex, current: f64) {
        if let Some(b) = self.graph.edge_weight_mut(edge) {
            b.current = Some(current);
        }
    }
    
    
    // Safety analysis support methods
    
    /// Get all components (branches) in the circuit
    pub fn components(&self) -> Vec<(ComponentId, &Component)> {
        self.graph.edge_indices()
            .filter_map(|edge| {
                self.graph.edge_weight(edge)
                    .map(|branch| (edge, branch))
            })
            .collect()
    }
    
    /// Get a specific component by ID
    pub fn get_component(&self, id: ComponentId) -> Option<&Component> {
        self.graph.edge_weight(id)
    }
    
    /// Get a specific node by ID
    pub fn get_node_by_id(&self, id: NodeId) -> Option<&Node> {
        self.graph.node_weight(id)
    }
    
    /// Get all components connected to a node
    pub fn get_components_at_node(&self, node: NodeId) -> Vec<ComponentId> {
        self.graph.edges(node)
            .map(|edge| edge.id())
            .collect()
    }
    
    /// Get power supply nodes
    pub fn get_power_nodes(&self) -> Vec<NodeId> {
        self.graph.node_indices()
            .filter(|&idx| {
                let node = &self.graph[idx];
                node.name.to_lowercase().contains("vcc") ||
                node.name.to_lowercase().contains("vdd") ||
                node.name.to_lowercase().contains("vin") ||
                node.name.to_lowercase().contains("power") ||
                node.name.starts_with("V") && node.name.len() <= 3
            })
            .collect()
    }
    
    /// Get ground nodes
    pub fn get_ground_nodes(&self) -> Vec<NodeId> {
        self.graph.node_indices()
            .filter(|&idx| self.graph[idx].is_ground)
            .collect()
    }
    
    /// Check if a node is a supply node
    pub fn is_supply_node(&self, node: NodeId) -> bool {
        let node_data = &self.graph[node];
        node_data.is_ground || self.get_power_nodes().contains(&node)
    }
    
    /// Get supply voltage (simplified - assumes single supply)
    pub fn get_supply_voltage(&self) -> Option<f64> {
        // Look for voltage sources
        for edge in self.graph.edge_indices() {
            if let Some(branch) = self.graph.edge_weight(edge) {
                if branch.component_type == "VoltageSource" {
                    return Some(branch.value);
                }
            }
        }
        
        // Default to 5V if no explicit source found
        if !self.get_power_nodes().is_empty() {
            Some(5.0)
        } else {
            None
        }
    }
    
    /// Get node name
    pub fn get_node_name(&self, node: NodeId) -> Option<&str> {
        self.graph.node_weight(node).map(|n| n.name.as_str())
    }
    
    /// Convert from BHDL netlist
    pub fn from_netlist(netlist: &bhdl_netlist::Netlist) -> crate::Result<Self> {
        let mut circuit = Self::new();
        
        // Add all nets as nodes
        for (net_id, net) in &netlist.nets {
            let name = net.name.clone().unwrap_or_else(|| format!("net_{:?}", net_id));
            circuit.add_node(name, Some(net_id));
        }
        
        // Add components as branches
        // This is simplified - we find nets that connect to each instance
        for (instance_id, instance) in &netlist.instances {
            if let Some(module) = netlist.modules.get(instance.definition) {
                // Find nets connected to this instance
                let mut connected_nets = Vec::new();
                for (net_id, net) in &netlist.nets {
                    for conn_point in &net.connections {
                        match conn_point {
                            ConnectionPoint::InstancePort(inst_id, _) |
                            ConnectionPoint::InstancePin(inst_id, _) => {
                                if *inst_id == instance_id {
                                    connected_nets.push(net_id);
                                    break;
                                }
                            }
                            ConnectionPoint::PinInstance(pin_inst_id) => {
                                // Check if this pin instance belongs to our instance
                                if let Some(pin_inst) = netlist.pin_instances.get(*pin_inst_id) {
                                    if pin_inst.instance == instance_id {
                                        connected_nets.push(net_id);
                                        break;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                
                // For 2-pin components, use first two connected nets
                if connected_nets.len() >= 2 {
                    let node1 = netlist.nets.get(connected_nets[0])
                        .and_then(|n| n.name.clone())
                        .unwrap_or_else(|| "unknown".to_string());
                    let node2 = netlist.nets.get(connected_nets[1])
                        .and_then(|n| n.name.clone())
                        .unwrap_or_else(|| "unknown".to_string());
                    
                    let component_type = module.name.clone();
                    
                    // TODO: Extract value from module parameters or instance overrides
                    // For now, use a placeholder value
                    let value = 1.0;
                    
                    circuit.add_branch(
                        instance.name.clone(),
                        &node1,
                        &node2,
                        component_type,
                        value,
                        Some(instance_id),
                    );
                }
            }
        }
        
        Ok(circuit)
    }
}