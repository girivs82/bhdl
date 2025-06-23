//! SPICE engine adapter for analog simulation
//!
//! This adapter connects the bhdl-spice engine to the simulation coordinator,
//! handling netlist conversion, time stepping, and result extraction.

use std::collections::HashMap;
use bhdl_netlist::{Netlist, NetId, InstanceId, ModuleKind};
use bhdl_spice::{
    Circuit, Node, Branch, NodeId, ComponentId,
    NonlinearDcAnalysis, AnalysisResult,
    SimulationEngine, TransientAnalysisResult,
};
use crate::error::{SimulationResult, SimulationError};
use super::{EngineAdapter, ConvergenceInfo};
use thiserror::Error;

/// SPICE adapter error types
#[derive(Error, Debug)]
pub enum SpiceAdapterError {
    #[error("SPICE engine error: {0}")]
    EngineError(String),
    
    #[error("Netlist conversion error: {0}")]
    ConversionError(String),
    
    #[error("Invalid net ID: {0:?}")]
    InvalidNetId(NetId),
    
    #[error("Convergence failure after {0} iterations")]
    ConvergenceFailure(usize),
}

/// SPICE simulation results
#[derive(Debug, Clone)]
pub struct SpiceResults {
    /// Node voltages indexed by net ID
    pub voltages: HashMap<NetId, f64>,
    /// Branch currents indexed by (from_net, to_net)
    pub currents: HashMap<(NetId, NetId), f64>,
    /// Power dissipation by instance
    pub power: HashMap<InstanceId, f64>,
}

/// Adapter for the bhdl-spice engine
pub struct SpiceAdapter {
    /// The SPICE circuit representation
    circuit: Circuit,
    
    /// The simulation engine
    engine: Option<SimulationEngine>,
    
    /// DC analysis engine
    dc_analyzer: Option<NonlinearDcAnalysis>,
    
    /// Mapping from NetId to SPICE node indices
    net_to_node: HashMap<NetId, NodeId>,
    
    /// Mapping from SPICE node indices to NetId
    node_to_net: HashMap<NodeId, NetId>,
    
    /// Mapping from InstanceId to SPICE component indices
    instance_to_component: HashMap<InstanceId, Vec<ComponentId>>,
    
    /// Current simulation time
    current_time: f64,
    
    /// Last analysis result
    last_result: Option<AnalysisResult>,
    
    /// Convergence information
    last_convergence: ConvergenceInfo,
    
    /// Boundary conditions (voltage sources for interface nets)
    boundary_sources: HashMap<NetId, ComponentId>,
}

impl SpiceAdapter {
    /// Create a new SPICE adapter
    pub fn new() -> Self {
        Self {
            circuit: Circuit::new(),
            engine: None,
            dc_analyzer: None,
            net_to_node: HashMap::new(),
            node_to_net: HashMap::new(),
            instance_to_component: HashMap::new(),
            current_time: 0.0,
            last_result: None,
            last_convergence: ConvergenceInfo {
                iterations: 0,
                max_error: 0.0,
                converged: true,
                step_time: 0.0,
            },
            boundary_sources: HashMap::new(),
        }
    }
    
    /// Convert a netlist subset to SPICE circuit
    fn convert_netlist(&mut self, netlist: &Netlist, instance_ids: &[InstanceId], net_ids: &[NetId]) -> Result<(), SpiceAdapterError> {
        // Clear existing circuit
        self.circuit = Circuit::new();
        self.net_to_node.clear();
        self.node_to_net.clear();
        self.instance_to_component.clear();
        
        // Create nodes for each net
        for &net_id in net_ids {
            let net = netlist.nets.get(net_id)
                .ok_or_else(|| SpiceAdapterError::InvalidNetId(net_id))?;
            
            // Use net name or create a default name
            let node_name = net.name.clone()
                .unwrap_or_else(|| format!("net_{:?}", net_id));
            
            // Create SPICE node
            let node_idx = self.circuit.add_node(node_name, Some(net_id));
            self.net_to_node.insert(net_id, node_idx);
            self.node_to_net.insert(node_idx, net_id);
        }
        
        // Convert instances to SPICE components
        for &instance_id in instance_ids {
            let instance = netlist.instances.get(instance_id)
                .ok_or_else(|| SpiceAdapterError::ConversionError(format!("Invalid instance ID: {:?}", instance_id)))?;
            
            let module = netlist.modules.get(instance.definition)
                .ok_or_else(|| SpiceAdapterError::ConversionError("Invalid module ID".to_string()))?;
            
            // Only convert physical components
            if module.kind != ModuleKind::PhysicalComponent {
                continue;
            }
            
            // Convert based on component type
            self.convert_instance_to_spice(instance_id, instance, module, netlist)?;
        }
        
        Ok(())
    }
    
    /// Convert a single instance to SPICE component(s)
    fn convert_instance_to_spice(
        &mut self,
        instance_id: InstanceId,
        instance: &bhdl_netlist::Instance,
        module: &bhdl_netlist::ModuleDefinition,
        netlist: &Netlist,
    ) -> Result<(), SpiceAdapterError> {
        let mut component_indices = Vec::new();
        
        // Extract component parameters
        let value = instance.attributes.get("value")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);
        
        // Determine component type from module name or attributes
        let comp_type = module.name.to_lowercase();
        
        // Get connected nets from instance's pins
        let mut connected_nets = Vec::new();
        // Find pin instances for this component instance
        for (_, pin_inst) in &netlist.pin_instances {
            if pin_inst.instance == instance_id {
                if let Some(net_id) = pin_inst.net {
                    connected_nets.push(net_id);
                }
            }
        }
        
        if connected_nets.len() < 2 {
            return Ok(()); // Skip components without proper connections
        }
        
        // Get node names for the connected nets
        let net1 = netlist.nets.get(connected_nets[0])
            .and_then(|n| n.name.clone())
            .unwrap_or_else(|| format!("net_{:?}", connected_nets[0]));
        let net2 = netlist.nets.get(connected_nets[1])
            .and_then(|n| n.name.clone())
            .unwrap_or_else(|| format!("net_{:?}", connected_nets[1]));
        
        // Create appropriate SPICE component
        let component_name = instance.name.clone();
        let edge_idx = match comp_type.as_str() {
            s if s.contains("resistor") || s.starts_with("r") => {
                self.circuit.add_branch(
                    component_name,
                    &net1,
                    &net2,
                    "Resistor".to_string(),
                    value,
                    Some(instance_id),
                )
            }
            s if s.contains("capacitor") || s.starts_with("c") => {
                self.circuit.add_branch(
                    component_name,
                    &net1,
                    &net2,
                    "Capacitor".to_string(),
                    value,
                    Some(instance_id),
                )
            }
            s if s.contains("inductor") || s.starts_with("l") => {
                self.circuit.add_branch(
                    component_name,
                    &net1,
                    &net2,
                    "Inductor".to_string(),
                    value,
                    Some(instance_id),
                )
            }
            s if s.contains("diode") || s.starts_with("d") => {
                self.circuit.add_branch(
                    component_name,
                    &net1,
                    &net2,
                    "Diode".to_string(),
                    0.0, // Diodes don't have a simple value parameter
                    Some(instance_id),
                )
            }
            _ => {
                // Unknown component type - skip for now
                return Ok(());
            }
        };
        
        component_indices.push(edge_idx);
        
        if !component_indices.is_empty() {
            self.instance_to_component.insert(instance_id, component_indices);
        }
        
        Ok(())
    }
    
    /// Extract results from SPICE analysis
    fn extract_results(&self) -> SpiceResults {
        let mut results = SpiceResults {
            voltages: HashMap::new(),
            currents: HashMap::new(),
            power: HashMap::new(),
        };
        
        // Extract node voltages from last analysis result
        if let Some(result) = &self.last_result {
            for (&node_idx, &net_id) in &self.node_to_net {
                if let Some(&voltage) = result.node_voltages.get(&node_idx) {
                    results.voltages.insert(net_id, voltage);
                }
            }
            
            // Extract branch currents
            for (edge_idx, current) in &result.branch_currents {
                // TODO: Map branch currents back to net pairs when API is available
                // For now, just store the raw currents
            }
        }
        
        results
    }
}

impl EngineAdapter for SpiceAdapter {
    fn initialize(&mut self, netlist: &Netlist, instance_ids: &[InstanceId], net_ids: &[NetId]) -> SimulationResult<()> {
        // Convert netlist to SPICE circuit
        self.convert_netlist(netlist, instance_ids, net_ids)
            .map_err(|e| SimulationError::EngineError(e.to_string()))?;
        
        // Create DC analyzer
        self.dc_analyzer = Some(NonlinearDcAnalysis::new(self.circuit.clone()));
        
        // Create simulation engine
        self.engine = Some(SimulationEngine::new(self.circuit.clone()));
        
        // Run DC operating point analysis
        if let Some(analyzer) = &mut self.dc_analyzer {
            match analyzer.analyze() {
                Ok(result) => {
                    self.last_result = Some(result);
                    self.last_convergence.converged = true;
                }
                Err(e) => {
                    return Err(SimulationError::EngineError(
                        format!("DC analysis failed: {}", e)
                    ));
                }
            }
        }
        
        Ok(())
    }
    
    fn step(&mut self, current_time: f64, target_time: f64) -> SimulationResult<()> {
        let start = std::time::Instant::now();
        
        // For now, just run DC analysis at each time step
        // TODO: Implement proper transient analysis when available
        if let Some(analyzer) = &mut self.dc_analyzer {
            match analyzer.analyze() {
                Ok(result) => {
                    self.last_result = Some(result);
                    self.last_convergence = ConvergenceInfo {
                        iterations: 1, // TODO: Get actual iteration count when API available
                        max_error: 0.0, // TODO: Get actual error when API available
                        converged: true,
                        step_time: start.elapsed().as_secs_f64(),
                    };
                }
                Err(e) => {
                    self.last_convergence.converged = false;
                    return Err(SimulationError::ConvergenceError {
                        iterations: 100, // TODO: Get actual iteration count
                    });
                }
            }
        }
        
        self.current_time = target_time;
        Ok(())
    }
    
    fn get_net_values(&self) -> HashMap<NetId, f64> {
        self.extract_results().voltages
    }
    
    fn set_boundary_value(&mut self, net_id: NetId, value: f64) -> SimulationResult<()> {
        // Find the node for this net
        if let Some(&node_idx) = self.net_to_node.get(&net_id) {
            // For boundary conditions, we'll need to add a voltage source
            // This requires rebuilding the circuit, which is expensive
            // TODO: Implement a more efficient way to handle boundary conditions
            
            // For now, just store the boundary value
            // The next analysis will need to account for it
            if !self.boundary_sources.contains_key(&net_id) {
                // Need to add a voltage source to the circuit
                let source_name = format!("V_boundary_{:?}", net_id);
                let net_name = format!("net_{:?}", net_id); // Use the same format as in convert_netlist
                
                let edge_idx = self.circuit.add_branch(
                    source_name,
                    &net_name,
                    "GND", // Assuming ground exists
                    "VoltageSource".to_string(),
                    value,
                    None,
                );
                
                self.boundary_sources.insert(net_id, edge_idx);
                
                // Recreate analyzers with updated circuit
                self.dc_analyzer = Some(NonlinearDcAnalysis::new(self.circuit.clone()));
                self.engine = Some(SimulationEngine::new(self.circuit.clone()));
            }
        }
        
        Ok(())
    }
    
    fn has_converged(&self) -> bool {
        self.last_convergence.converged
    }
    
    fn get_convergence_info(&self) -> ConvergenceInfo {
        self.last_convergence.clone()
    }
    
    fn reset(&mut self) {
        self.current_time = 0.0;
        self.last_result = None;
        // Reset analyzers if they exist
        if self.dc_analyzer.is_some() {
            self.dc_analyzer = Some(NonlinearDcAnalysis::new(self.circuit.clone()));
        }
        if self.engine.is_some() {
            self.engine = Some(SimulationEngine::new(self.circuit.clone()));
        }
    }
}

impl Default for SpiceAdapter {
    fn default() -> Self {
        Self::new()
    }
}