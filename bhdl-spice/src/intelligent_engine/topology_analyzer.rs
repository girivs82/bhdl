//! Topology analyzer for identifying circuit patterns

use crate::Circuit;
use super::patterns::{CircuitPattern, PatternMatcher, ShareType, LoadType};
use super::SynthesizerContext;
use std::collections::{HashMap, HashSet};
use petgraph::visit::EdgeRef;

/// Main topology analyzer that identifies problematic circuit patterns
pub struct TopologyAnalyzer {
    /// Registered pattern matchers
    matchers: Vec<Box<dyn PatternMatcher>>,
}

impl TopologyAnalyzer {
    pub fn new() -> Self {
        let matchers: Vec<Box<dyn PatternMatcher>> = vec![
            Box::new(SeriesNonlinearMatcher::new()),
            Box::new(ParallelDeviceMatcher::new()),
            Box::new(ProtectionCircuitMatcher::new()),
            Box::new(HighGainFeedbackMatcher::new()),
        ];
        
        Self { matchers }
    }
    
    /// Identify patterns without synthesizer context
    pub fn identify_patterns(&self, circuit: &Circuit) -> Vec<CircuitPattern> {
        let mut patterns = Vec::new();
        
        for matcher in &self.matchers {
            patterns.extend(matcher.identify(circuit));
        }
        
        // Sort by severity (most severe first)
        patterns.sort_by_key(|p| std::cmp::Reverse(p.severity()));
        
        patterns
    }
    
    /// Identify patterns with synthesizer context for better accuracy
    pub fn identify_with_context(
        &self, 
        circuit: &Circuit,
        context: &SynthesizerContext,
    ) -> Vec<CircuitPattern> {
        let mut patterns = self.identify_patterns(circuit);
        
        // Enhance patterns with context information
        self.enhance_with_flow_intents(&mut patterns, context);
        self.enhance_with_module_info(&mut patterns, context);
        
        patterns
    }
    
    /// Use flow intents to enhance pattern detection
    fn enhance_with_flow_intents(
        &self,
        patterns: &mut Vec<CircuitPattern>,
        context: &SynthesizerContext,
    ) {
        // Look for series patterns with sequential intent
        for flow in &context.flow_paths {
            if let Some(intent) = &flow.intent {
                if intent.name == "sequential_indication" {
                    // Check if any series pattern matches this flow
                    for pattern in patterns.iter_mut() {
                        if let CircuitPattern::SeriesNonlinear { 
                            components, order_matters, ..
                        } = pattern {
                            if flow.components.iter().all(|c| components.contains(c)) {
                                *order_matters = true;
                            }
                        }
                    }
                }
            }
        }
    }
    
    /// Use module boundaries to enhance pattern detection
    fn enhance_with_module_info(
        &self,
        patterns: &mut Vec<CircuitPattern>,
        context: &SynthesizerContext,
    ) {
        // Module names can hint at circuit function
        for (_id, module) in &context.module_instances {
            match module.name.as_str() {
                "BuckConverter" | "StepDown" => {
                    // Add switching converter pattern if not already detected
                    if !patterns.iter().any(|p| matches!(p, CircuitPattern::SwitchingConverter { .. })) {
                        patterns.push(CircuitPattern::SwitchingConverter {
                            topology: super::patterns::ConverterType::Buck,
                            switches: module.component_ids.clone(),
                            control_type: super::patterns::ControlType::VoltageMode,
                        });
                    }
                },
                "BridgeRectifier" | "FullBridge" => {
                    // Look for 4 diodes in this module
                    let diodes: Vec<String> = module.component_ids.iter()
                        .filter(|id| self.is_diode_component(id))
                        .cloned()
                        .collect();
                    
                    if diodes.len() == 4 {
                        patterns.push(CircuitPattern::BridgeRectifier {
                            diodes: diodes.try_into().unwrap(),
                            load_type: LoadType::Unknown,
                        });
                    }
                },
                _ => {}
            }
        }
    }
    
    fn is_diode_component(&self, _id: &str) -> bool {
        // TODO: Check actual component model
        false
    }
}

/// Matcher for series nonlinear elements (LEDs, diodes)
struct SeriesNonlinearMatcher;

impl SeriesNonlinearMatcher {
    fn new() -> Self {
        Self
    }
    
    fn find_series_chains(&self, circuit: &Circuit) -> Vec<Vec<String>> {
        let mut chains = Vec::new();
        let mut visited = HashSet::new();
        
        // Find all nonlinear components
        let nonlinear_components: Vec<String> = circuit.branches()
            .filter_map(|(idx, branch)| {
                // Check if this is a nonlinear component based on type
                match branch.component_type.as_str() {
                    "LED" | "Diode" => Some(branch.name.clone()),
                    _ => None,
                }
            })
            .collect();
        
        // For each nonlinear component, try to build a chain
        for comp in &nonlinear_components {
            if visited.contains(comp) {
                continue;
            }
            
            let chain = self.build_chain_from(comp, circuit, &nonlinear_components, &mut visited);
            if chain.len() > 1 {
                chains.push(chain);
            }
        }
        
        chains
    }
    
    fn build_chain_from(
        &self,
        start: &str,
        circuit: &Circuit,
        nonlinear_components: &[String],
        visited: &mut HashSet<String>,
    ) -> Vec<String> {
        let mut chain = vec![start.to_string()];
        visited.insert(start.to_string());
        
        // Find connected nonlinear components
        let mut current = start;
        loop {
            if let Some(next_comp) = self.find_next_in_chain(current, circuit, nonlinear_components, visited) {
                chain.push(next_comp.clone());
                visited.insert(next_comp.clone());
                current = &chain[chain.len() - 1];
            } else {
                break;
            }
        }
        
        chain
    }
    
    fn find_next_in_chain(
        &self,
        current: &str,
        circuit: &Circuit,
        nonlinear_components: &[String],
        visited: &HashSet<String>,
    ) -> Option<String> {
        // Get the component branch
        let (edge_idx, branch) = circuit.get_branch(current)?;
        
        // Get the nodes this component connects
        let endpoints = circuit.graph.edge_endpoints(edge_idx)?;
        
        // For LEDs/diodes, we assume node order is anode -> cathode
        // Check what's connected to the cathode (second node)
        let cathode_node = endpoints.1;
        
        // Find other components connected to this node
        for edge in circuit.graph.edges(cathode_node) {
            let other_branch = edge.weight();
            if other_branch.name == current || 
               visited.contains(&other_branch.name) || 
               !nonlinear_components.contains(&other_branch.name) {
                continue;
            }
            
            // Check if this component's anode connects to our cathode
            // (i.e., it's the source node of the edge)
            if edge.source() == cathode_node {
                return Some(other_branch.name.clone());
            }
        }
        
        None
    }
}

impl PatternMatcher for SeriesNonlinearMatcher {
    fn identify(&self, circuit: &Circuit) -> Vec<CircuitPattern> {
        let chains = self.find_series_chains(circuit);
        
        chains.into_iter().map(|components| {
            // Check if all components are identical type
            let first_type = circuit.get_branch(&components[0])
                .map(|(_, branch)| match branch.component_type.as_str() {
                    "LED" => "LED",
                    "Diode" => "Diode",
                    _ => "Unknown",
                })
                .unwrap_or("Unknown");
            
            let identical = components.iter().all(|comp| {
                circuit.get_branch(comp)
                    .map(|(_, branch)| match branch.component_type.as_str() {
                        "LED" => "LED" == first_type,
                        "Diode" => "Diode" == first_type,
                        _ => false,
                    })
                    .unwrap_or(false)
            });
            
            CircuitPattern::SeriesNonlinear {
                count: components.len(),
                component_type: first_type.to_string(),
                components,
                identical,
                order_matters: false, // Will be enhanced by context
            }
        }).collect()
    }
    
    fn confidence(&self) -> f64 {
        0.9 // High confidence for series detection
    }
}

/// Matcher for parallel current-sharing devices
struct ParallelDeviceMatcher;

impl ParallelDeviceMatcher {
    fn new() -> Self {
        Self
    }
    
    fn find_parallel_groups(&self, circuit: &Circuit) -> Vec<Vec<String>> {
        let mut groups = Vec::new();
        let mut visited = HashSet::new();
        
        // Find all active devices (MOSFETs, BJTs)
        let active_devices: Vec<String> = circuit.branches()
            .filter_map(|(idx, branch)| {
                match branch.component_type.as_str() {
                    "MOSFET" | "BJT" | "FET" | "Transistor" => Some(branch.name.clone()),
                    _ => None,
                }
            })
            .collect();
        
        // Group devices with same drain/collector and source/emitter connections
        for device in &active_devices {
            if visited.contains(device) {
                continue;
            }
            
            let group = self.find_parallel_to(device, circuit, &active_devices, &mut visited);
            if group.len() > 1 {
                groups.push(group);
            }
        }
        
        groups
    }
    
    fn find_parallel_to(
        &self,
        device: &str,
        circuit: &Circuit,
        active_devices: &[String],
        visited: &mut HashSet<String>,
    ) -> Vec<String> {
        let mut group = vec![device.to_string()];
        visited.insert(device.to_string());
        
        let (edge_idx, branch) = match circuit.get_branch(device) {
            Some(b) => b,
            None => return group,
        };
        
        // Get the nodes this device connects
        let endpoints = match circuit.graph.edge_endpoints(edge_idx) {
            Some(e) => e,
            None => return group,
        };
        
        // Find other devices with same endpoints
        for other in active_devices {
            if visited.contains(other) {
                continue;
            }
            
            if let Some((other_idx, other_branch)) = circuit.get_branch(other) {
                if let Some(other_endpoints) = circuit.graph.edge_endpoints(other_idx) {
                    // Check if endpoints match (in either order)
                    if (other_endpoints.0 == endpoints.0 && other_endpoints.1 == endpoints.1) ||
                       (other_endpoints.0 == endpoints.1 && other_endpoints.1 == endpoints.0) {
                        group.push(other.clone());
                        visited.insert(other.clone());
                    }
                }
            }
        }
        
        group
    }
}

impl PatternMatcher for ParallelDeviceMatcher {
    fn identify(&self, circuit: &Circuit) -> Vec<CircuitPattern> {
        let groups = self.find_parallel_groups(circuit);
        
        groups.into_iter().map(|components| {
            let device_type = circuit.get_branch(&components[0])
                .map(|(_, branch)| branch.component_type.as_str())
                .unwrap_or("Unknown");
            
            CircuitPattern::ParallelDevices {
                count: components.len(),
                device_type: device_type.to_string(),
                components,
                expected_sharing: ShareType::Unknown,
            }
        }).collect()
    }
    
    fn confidence(&self) -> f64 {
        0.85
    }
}

/// Matcher for protection circuits
struct ProtectionCircuitMatcher;

impl ProtectionCircuitMatcher {
    fn new() -> Self {
        Self
    }
}

impl PatternMatcher for ProtectionCircuitMatcher {
    fn identify(&self, circuit: &Circuit) -> Vec<CircuitPattern> {
        let mut patterns = Vec::new();
        
        // Find TVS diodes and Zener diodes
        for (idx, branch) in circuit.branches() {
            match branch.component_type.as_str() {
                "TVSDiode" => {
                    // Find the protected net (connected to cathode)
                    if let Some(endpoints) = circuit.graph.edge_endpoints(idx) {
                        // Assume cathode is the second node
                        let protected_node = &circuit.graph[endpoints.1];
                        patterns.push(CircuitPattern::ProtectionCircuit {
                            protection_device: branch.name.clone(),
                            protected_net: protected_node.name.clone(),
                            clamp_voltage: branch.value, // Voltage is stored as value
                        });
                    }
                },
                "Diode" | "ZenerDiode" => {
                    // Check if this is a Zener by voltage value
                    if branch.value > 2.0 && branch.value < 50.0 {
                        if let Some(endpoints) = circuit.graph.edge_endpoints(idx) {
                            let protected_node = &circuit.graph[endpoints.1];
                            patterns.push(CircuitPattern::ProtectionCircuit {
                                protection_device: branch.name.clone(),
                                protected_net: protected_node.name.clone(),
                                clamp_voltage: branch.value,
                            });
                        }
                    }
                },
                _ => {}
            }
        }
        
        patterns
    }
    
    fn confidence(&self) -> f64 {
        0.95 // High confidence for protection devices
    }
}

/// Matcher for high-gain feedback loops
struct HighGainFeedbackMatcher;

impl HighGainFeedbackMatcher {
    fn new() -> Self {
        Self
    }
}

impl PatternMatcher for HighGainFeedbackMatcher {
    fn identify(&self, circuit: &Circuit) -> Vec<CircuitPattern> {
        let mut patterns = Vec::new();
        
        // Find op-amps and check for feedback
        for (idx, branch) in circuit.branches() {
            if branch.component_type == "OpAmp" {
                // For op-amps, we need to check for feedback paths
                // This is simplified - in reality would need more sophisticated analysis
                if let Some(endpoints) = circuit.graph.edge_endpoints(idx) {
                    // Find components that might form feedback
                    let feedback_components = self.find_feedback_components(circuit, &branch.name);
                    
                    if !feedback_components.is_empty() {
                        patterns.push(CircuitPattern::HighGainFeedback {
                            forward_gain: branch.value, // Gain stored as value
                            feedback_components,
                            loop_type: super::patterns::FeedbackType::Negative,
                        });
                    }
                }
            }
        }
        
        patterns
    }
    
    fn confidence(&self) -> f64 {
        0.8
    }
}

impl HighGainFeedbackMatcher {
    fn find_feedback_components(
        &self,
        circuit: &Circuit,
        opamp_name: &str,
    ) -> Vec<String> {
        // Simple heuristic - find resistors and capacitors that might form feedback
        let mut feedback_components = Vec::new();
        
        // In a real implementation, we would trace the actual feedback path
        // For now, just find passive components that might be in feedback
        for (idx, branch) in circuit.branches() {
            if branch.name != opamp_name && 
               (branch.component_type == "Resistor" || 
                branch.component_type == "Res" ||
                branch.component_type == "Capacitor" ||
                branch.component_type == "Cap") {
                // Simplified: assume components near the op-amp might be feedback
                feedback_components.push(branch.name.clone());
                
                // Limit to reasonable number
                if feedback_components.len() >= 4 {
                    break;
                }
            }
        }
        
        feedback_components
    }
}