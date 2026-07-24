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

/// Metadata key: parent instance name for decomposed branches
pub const META_PARENT_INSTANCE: &str = "parent_instance";
/// Metadata key: role within decomposition ("vout" or "dropout")
pub const META_DECOMPOSITION_ROLE: &str = "decomposition_role";
/// Metadata key: component class of the parent regulator (e.g., "switching_regulator", "voltage_regulator")
pub const META_COMPONENT_CLASS: &str = "component_class";
/// Metadata keys: switching regulator loss model parameters (from device datasheet)
pub const META_RDS_ON: &str = "rds_on";
pub const META_F_SW: &str = "f_sw";
pub const META_T_SW: &str = "t_sw";
pub const META_I_QUIESCENT: &str = "i_quiescent";
/// Metadata key: input current authored by an entity `simulation { model {
/// node VIN draws = … } }` block (Vendor_Simulation_Blocks.md §5). When
/// present it supersedes the generic physics loss model for that regulator —
/// the vendor's datasheet-specific efficiency model is the correction. Power
/// is then `P_in − P_out = i_in·V_in − i_out·V_out`.
pub const META_MODEL_I_IN: &str = "model_i_in";
/// Piecewise-linear I-V table for a `TableIV` branch (an IBIS buffer's
/// composed DC curve, a varistor, …), encoded `v:i;v:i;…` in SI volts/amps.
/// Kept as branch metadata so the generic `Branch` needs no new field.
pub const META_IV_TABLE: &str = "iv_table";

/// Encode PWL points for [`META_IV_TABLE`].
pub fn encode_iv_table(points: &[(f64, f64)]) -> String {
    points
        .iter()
        .map(|(v, i)| format!("{v}:{i}"))
        .collect::<Vec<_>>()
        .join(";")
}

/// Decode [`META_IV_TABLE`] points. None on any malformed pair — a
/// half-parseable table is not a model.
pub fn decode_iv_table(s: &str) -> Option<Vec<(f64, f64)>> {
    let mut out = Vec::new();
    for pair in s.split(';') {
        let (v, i) = pair.split_once(':')?;
        out.push((v.trim().parse().ok()?, i.trim().parse().ok()?));
    }
    (!out.is_empty()).then_some(out)
}

// ─────────────────────────────────────────────────────────────────────────────
// SPICE-relevant stdlib attribute keys (P0 metadata bridge).
//
// These flow from stdlib `.bhdl` files (via `attribute foo = ...;`) through the
// synthesizer's `ExtractedModel`, into per-branch metadata in this module, and
// out into `ComponentModel::*` variants in `stdlib_model_loader`. They are the
// single source of truth for "what does this component look like to SPICE,"
// replacing the prior pattern where parameters lived in Rust source LUTs.
//
// New SPICE-touching attributes should be declared here, populated by
// `netlist_converter::add_physical_component`, and consumed by
// `stdlib_model_loader::load_models_from_circuit`. The existing Rust LUTs
// (`LedColor::get_params`) remain as fallbacks for circuits whose stdlib
// definition predates the corresponding attribute.
// ─────────────────────────────────────────────────────────────────────────────

/// Manufacturing tolerance (fraction, e.g. 0.05 for ±5%).
pub const META_TOLERANCE: &str = "tolerance";
/// Power-dissipation rating in watts.
pub const META_POWER_RATING: &str = "power_rating";
/// Equivalent series resistance for capacitors, in ohms.
pub const META_ESR: &str = "esr";
/// Op-amp positive output saturation (volts) — the positive supply rail.
pub const META_VSAT_P: &str = "vsat_p";
/// Op-amp negative output saturation (volts) — the negative supply rail.
pub const META_VSAT_N: &str = "vsat_n";
/// Op-amp gain-bandwidth product, in Hz (sets the dominant pole together
/// with the open-loop gain carried in `Branch::value`).
pub const META_GBW: &str = "gbw_hz";
/// Op-amp differential input resistance, in ohms.
pub const META_RIN: &str = "rin_ohm";
/// Op-amp open-loop output resistance, in ohms.
pub const META_ROUT: &str = "rout_ohm";
/// Op-amp input offset voltage, in volts (added to the differential input).
pub const META_VOS: &str = "vos_v";
/// Op-amp slew rate, in V/µs.
pub const META_SLEW: &str = "slew_v_per_us";
/// Regulator efficiency (fraction 0..1) — stamped on the output-source
/// branch so the input-draw fixpoint can compute i_in from i_out.
pub const META_EFFICIENCY: &str = "efficiency";
/// Capacitor or device voltage rating, in volts.
pub const META_VOLTAGE_RATING: &str = "voltage_rating";
/// Inductor DC resistance, in ohms.
pub const META_DCR: &str = "dcr";
/// Shockley saturation current `Is`, in amperes.
pub const META_SATURATION_CURRENT: &str = "saturation_current";
/// Diode/LED ideality factor `n` (dimensionless).
pub const META_EMISSION_COEFFICIENT: &str = "emission_coefficient";
/// Thermal voltage `Vt = kT/q`, in volts.
pub const META_THERMAL_VOLTAGE: &str = "thermal_voltage";
/// Nominal forward voltage at the rated forward current, in volts.
pub const META_FORWARD_VOLTAGE: &str = "forward_voltage";
/// Rated forward current, in amperes.
pub const META_FORWARD_CURRENT: &str = "forward_current";
/// Limit-table entries (mirrored into `ElectricalLimits` by the loader).
pub const META_MAX_CURRENT: &str = "max_current";
pub const META_MAX_VOLTAGE: &str = "max_voltage";
pub const META_MAX_POWER: &str = "max_power";
pub const META_TEMP_MIN: &str = "temp_min";
pub const META_TEMP_MAX: &str = "temp_max";
/// Free-form variant tag (e.g. LED `color = "red"`); used by fallback LUTs.
pub const META_VARIANT: &str = "variant";
/// Optocoupler current-transfer ratio (IC/IF, as a fraction, e.g. 0.5 for
/// 50%). Carried on the phototransistor (`PhotoCoupled`) branch of the
/// converter's optocoupler decomposition.
pub const META_CTR: &str = "ctr";
/// Soft-saturation knee voltage for the `PhotoCoupled` collector-emitter
/// branch, in volts: i = CTR·IF·tanh(Vce/knee). Derived from the part's
/// cited VCE(sat).
pub const META_CTR_VKNEE: &str = "ctr_v_knee";
/// Name of the branch whose solved current CONTROLS this branch (the
/// optocoupler's IRED branch controlling its phototransistor).
pub const META_CTRL_BRANCH: &str = "ctrl_branch";
/// Optocoupler CTR-vs-IF curve, normalized to the rank point:
/// "if_amps:factor;if_amps:factor;..." sorted by IF ascending (the
/// datasheet Fig.6 points). The PhotoCoupled equation scales CTR by
/// the interpolated factor at the solved IF.
pub const META_CTR_CURVE: &str = "ctr_curve";

/// Koren triode parameters on a "KorenTriode" plate→cathode branch —
/// the DC equation path's view of a triode (the multi-terminal
/// DeviceKind::Triode device serves the production/AC/transient
/// solvers; the branch serves SpiceEquationSystem, which stamps
/// branches only). The grid enters through META_TRIODE_GRID_NODE
/// (net name, resolved to a node at equation-build time).
pub const META_TRIODE_MU: &str = "triode_mu";
pub const META_TRIODE_EX: &str = "triode_ex";
pub const META_TRIODE_KG1: &str = "triode_kg1";
pub const META_TRIODE_KP: &str = "triode_kp";
pub const META_TRIODE_KVB: &str = "triode_kvb";
pub const META_TRIODE_GRID_NODE: &str = "triode_grid_node";

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
    /// Structured metadata for semantic relationships (e.g. decomposition role)
    pub metadata: HashMap<String, String>,
}

impl Branch {
    /// Builder-style method to add a metadata key-value pair
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }

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

/// A multi-terminal device — one that cannot be represented as a 2-node
/// graph edge (a `Branch`). Triodes, BJTs, MOSFETs, pentodes and
/// transformers all have three or more terminals.
///
/// Devices live in `Circuit.devices`, a flat list parallel to the
/// 2-terminal branch graph — the SPICE-style "device list" structure. A
/// device carries its own model parameters (in `DeviceKind`); unlike a
/// branch it is not threaded through any external model map.
#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    /// Instance name (e.g. the tube reference `V1`).
    pub name: String,
    /// Originating BHDL netlist instance, if any.
    pub instance_id: Option<InstanceId>,
    /// Device type and its model parameters.
    pub kind: DeviceKind,
    /// Terminal nodes, ordered as documented per `DeviceKind` variant.
    pub terminals: Vec<NodeId>,
}

/// Device type, carrying that type's model parameters inline.
///
/// Model parameters are stored as plain `f64` fields (not a typed params
/// struct) so this foundational module needs no dependency on the model
/// modules — the solver reconstructs whatever typed parameter struct it
/// wants from these.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeviceKind {
    /// Vacuum triode, Koren model. `terminals` = `[plate, grid, cathode]`.
    Triode {
        /// Amplification factor μ.
        mu: f64,
        /// Plate-current exponent Ex.
        ex: f64,
        /// Current-scaling constant Kg1.
        kg1: f64,
        /// Grid-drive sharpness Kp.
        kp: f64,
        /// Knee constant Kvb.
        kvb: f64,
    },
}

/// Circuit representation using a graph
#[derive(Clone)]
pub struct Circuit {
    /// Graph where nodes are electrical nodes and edges are components
    pub graph: Graph<Node, Branch>,
    /// Map from node names to graph indices
    node_map: HashMap<String, NodeIndex>,
    /// Map from component names to edge indices
    branch_map: HashMap<String, EdgeIndex>,
    /// Ground node index
    ground_node: Option<NodeIndex>,
    /// Multi-terminal devices (triodes, …) — parallel to the branch graph.
    devices: Vec<Device>,
}

impl Circuit {
    /// Create a new empty circuit
    pub fn new() -> Self {
        Self {
            graph: Graph::new(),
            node_map: HashMap::new(),
            branch_map: HashMap::new(),
            ground_node: None,
            devices: Vec::new(),
        }
    }
    
    /// Add a node to the circuit
    pub fn add_node(&mut self, name: String, net_id: Option<NetId>) -> NodeIndex {
        if let Some(&idx) = self.node_map.get(&name) {
            return idx;
        }
        
        let is_ground = name == "0" || name.to_lowercase() == "gnd" || name.to_lowercase() == "ground";
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
            metadata: HashMap::new(),
        };

        let idx = self.graph.add_edge(n1, n2, branch);
        self.branch_map.insert(name, idx);
        idx
    }

    /// Add an ideal op-amp as a THREE-terminal branch: `nodes = [inp, inn,
    /// out]`, `value` = open-loop gain A. The linear transient solver
    /// replaces the OUT node's KCL row with `v_out − A·(v_p − v_n) = 0`
    /// (infinite input impedance, zero output impedance), clamped to the
    /// META_VSAT_P / META_VSAT_N rails when present. The petgraph edge runs
    /// inp→out; solvers must read `branch.nodes`, not the edge endpoints.
    pub fn add_opamp_branch(
        &mut self,
        name: String,
        inp: &str,
        inn: &str,
        out: &str,
        open_loop_gain: f64,
        instance_id: Option<InstanceId>,
        metadata: HashMap<String, String>,
    ) -> EdgeIndex {
        let n_p = self.node_map.get(inp).copied()
            .unwrap_or_else(|| self.add_node(inp.to_string(), None));
        let n_n = self.node_map.get(inn).copied()
            .unwrap_or_else(|| self.add_node(inn.to_string(), None));
        let n_o = self.node_map.get(out).copied()
            .unwrap_or_else(|| self.add_node(out.to_string(), None));

        let branch = Branch {
            name: name.clone(),
            instance_id,
            component_type: "OpAmp".to_string(),
            value: open_loop_gain,
            current: None,
            nodes: vec![n_p, n_n, n_o],
            metadata,
        };

        let idx = self.graph.add_edge(n_p, n_o, branch);
        self.branch_map.insert(name, idx);
        idx
    }

    /// Add a component (branch) between two nodes with structured metadata
    pub fn add_branch_with_metadata(
        &mut self,
        name: String,
        node1: &str,
        node2: &str,
        component_type: String,
        value: f64,
        instance_id: Option<InstanceId>,
        metadata: HashMap<String, String>,
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
            metadata,
        };

        let idx = self.graph.add_edge(n1, n2, branch);
        self.branch_map.insert(name, idx);
        idx
    }
    
    /// Update an existing branch's value in place (upsert-by-name support
    /// for the DC input-draw fixpoint). Returns false when no branch of
    /// that name exists.
    pub fn set_branch_value(&mut self, name: &str, value: f64) -> bool {
        if let Some(&idx) = self.branch_map.get(name) {
            if let Some(b) = self.graph.edge_weight_mut(idx) {
                b.value = value;
                return true;
            }
        }
        false
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

    /// Add a multi-terminal device. `terminal_names` lists the device's
    /// terminal nodes in the order documented by its `DeviceKind` (for a
    /// triode: plate, grid, cathode); any not yet present are created.
    /// Returns the index of the device in `devices()`.
    pub fn add_device(
        &mut self,
        name: String,
        kind: DeviceKind,
        terminal_names: &[&str],
        instance_id: Option<InstanceId>,
    ) -> usize {
        let terminals: Vec<NodeId> = terminal_names
            .iter()
            .map(|n| {
                self.node_map.get(*n).copied()
                    .unwrap_or_else(|| self.add_node((*n).to_string(), None))
            })
            .collect();
        self.devices.push(Device { name, instance_id, kind, terminals });
        self.devices.len() - 1
    }

    /// All multi-terminal devices in the circuit.
    pub fn devices(&self) -> &[Device] {
        &self.devices
    }

    /// Get node by name
    pub fn get_node(&self, name: &str) -> Option<(NodeIndex, &Node)> {
        self.node_map.get(name)
            .map(|&idx| (idx, &self.graph[idx]))
    }
    
    /// Get node by ID
    pub fn get_node_by_id(&self, node_id: NodeIndex) -> Option<&Node> {
        self.graph.node_weight(node_id)
    }
    
    /// Get branch by name
    pub fn get_branch(&self, name: &str) -> Option<(EdgeIndex, &Branch)> {
        self.branch_map.get(name)
            .map(|&idx| (idx, &self.graph[idx]))
    }
    
    /// Get mutable branch by name
    pub fn get_branch_mut(&mut self, name: &str) -> Option<(EdgeIndex, &mut Branch)> {
        self.branch_map.get(name)
            .copied()
            .and_then(move |idx| self.graph.edge_weight_mut(idx).map(|branch| (idx, branch)))
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
    
    /// Modify branch value and type (for fault injection)
    pub fn modify_branch(&mut self, edge_idx: EdgeIndex, new_value: f64, new_type: String) {
        if let Some(branch) = self.graph.edge_weight_mut(edge_idx) {
            branch.value = new_value;
            branch.component_type = new_type;
        }
    }
    
    /// Set branch current (from analysis results)
    pub fn set_branch_current(&mut self, edge_idx: EdgeIndex, current: f64) {
        if let Some(branch) = self.graph.edge_weight_mut(edge_idx) {
            branch.current = Some(current);
        }
    }
    
    /// Set branch current by name
    pub fn set_branch_current_by_name(&mut self, name: &str, current: f64) {
        if let Some(&edge_idx) = self.branch_map.get(name) {
            self.set_branch_current(edge_idx, current);
        }
    }
    
    /// Get branch current from analysis results
    pub fn branch_current(&self, name: &str, result: &crate::AnalysisResult) -> crate::Result<f64> {
        let (edge_idx, _) = self.get_branch(name)
            .ok_or_else(|| crate::SpiceError::ComponentNotFound(name.to_string()))?;
        
        result.branch_currents.get(&edge_idx)
            .copied()
            .ok_or_else(|| crate::SpiceError::Other(anyhow::anyhow!(
                "No current result for branch {}", name
            )))
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
    
    /// Convert from BHDL netlist (basic version - use from_netlist_with_models for better results)
    pub fn from_netlist(netlist: &bhdl_netlist::Netlist) -> crate::Result<Self> {
        // Use the enhanced converter
        crate::netlist_converter::NetlistToSpiceConverter::new()
            .convert(netlist)
            .map_err(|e| crate::SpiceError::Other(e))
    }
    
    /// Update a component's model (for progressive solving strategies)
    pub fn update_component_model(&mut self, name: &str, model: crate::ComponentModel) {
        // This would need to be implemented based on how models are stored
        // For now, this is a placeholder
        // In practice, the model would be stored separately and referenced
    }
    
    /// Get a component's model
    pub fn get_component_model(&self, name: &str) -> Option<crate::ComponentModel> {
        // This would need to be implemented based on how models are stored
        // For now, return None
        None
    }
}