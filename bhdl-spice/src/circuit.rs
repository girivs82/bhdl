//! Circuit representation for electrical analysis

use std::collections::HashMap;
use petgraph::graph::{Graph, NodeIndex, EdgeIndex};
use petgraph::visit::EdgeRef;
use serde::{Serialize, Deserialize};
use bhdl_netlist::{NetId, InstanceId, ConnectionPoint};

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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
                    let value = 1.0; // Placeholder - would extract from parameters
                    
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