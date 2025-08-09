//! Circuit topology analysis for pattern detection

use crate::Circuit;
use petgraph::graph::NodeIndex;
use std::collections::{HashMap, HashSet, VecDeque};

pub mod patterns;
pub use patterns::CircuitPattern;

/// Analyzes circuit topology to identify patterns that benefit from specialized solving strategies
pub struct TopologyAnalyzer {
    circuit: Circuit,
    component_types: HashMap<String, ComponentType>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ComponentType {
    Linear,
    Nonlinear,
    Source,
}

impl TopologyAnalyzer {
    pub fn new(circuit: &Circuit) -> Self {
        let mut analyzer = Self {
            circuit: circuit.clone(),
            component_types: HashMap::new(),
        };
        analyzer.classify_components();
        analyzer
    }
    
    /// Classify components as linear, nonlinear, or source
    fn classify_components(&mut self) {
        for (_, branch) in self.circuit.branches() {
            let comp_type = match branch.component_type.as_str() {
                "Resistor" => ComponentType::Linear,
                "LED" | "Diode" => ComponentType::Nonlinear,
                "VoltageSource" | "CurrentSource" => ComponentType::Source,
                _ => ComponentType::Linear,
            };
            self.component_types.insert(branch.name.clone(), comp_type);
        }
    }
    
    /// Detect all circuit patterns
    pub fn detect_patterns(&self) -> Vec<CircuitPattern> {
        let mut patterns = Vec::new();
        
        // Detect series nonlinear chains
        patterns.extend(self.detect_series_nonlinear());
        
        // Detect parallel arrays
        patterns.extend(self.detect_parallel_arrays());
        
        // Detect symmetric structures
        patterns.extend(self.detect_symmetry());
        
        // Detect hierarchical blocks
        patterns.extend(self.detect_hierarchical());
        
        patterns
    }
    
    /// Detect series chains of nonlinear components
    fn detect_series_nonlinear(&self) -> Vec<CircuitPattern> {
        let mut patterns = Vec::new();
        let mut visited = HashSet::new();
        
        // Find all paths from voltage sources to ground
        for (_, branch) in self.circuit.branches() {
            if branch.component_type == "VoltageSource" && !visited.contains(&branch.name) {
                // Trace paths from this source
                if let Some(paths) = self.trace_paths_from_source(&branch.name) {
                    for path in paths {
                        let nonlinear_components: Vec<String> = path.iter()
                            .filter(|comp| {
                                self.component_types.get(*comp)
                                    .map(|t| *t == ComponentType::Nonlinear)
                                    .unwrap_or(false)
                            })
                            .cloned()
                            .collect();
                        
                        if nonlinear_components.len() >= 2 {
                            patterns.push(CircuitPattern::SeriesNonlinear {
                                components: nonlinear_components.clone(),
                                count: nonlinear_components.len(),
                            });
                            
                            // Mark as visited
                            for comp in &nonlinear_components {
                                visited.insert(comp.clone());
                            }
                        }
                    }
                }
            }
        }
        patterns
    }
    
    /// Detect parallel arrays of similar components
    fn detect_parallel_arrays(&self) -> Vec<CircuitPattern> {
        let mut patterns = Vec::new();
        let mut visited = HashSet::new();
        
        // Group components by their endpoints
        let mut parallel_groups: HashMap<(NodeIndex, NodeIndex), Vec<String>> = HashMap::new();
        
        for (idx, branch) in self.circuit.branches() {
            let (from, to) = self.circuit.graph.edge_endpoints(idx).unwrap();
            let key = if from < to { (from, to) } else { (to, from) };
            parallel_groups.entry(key).or_insert_with(Vec::new).push(branch.name.clone());
        }
        
        // Check each group for parallel arrays
        for (_, components) in parallel_groups {
            if components.len() >= 2 {
                // Check if all components are the same type
                let types: HashSet<_> = components.iter()
                    .filter_map(|c| self.component_types.get(c))
                    .collect();
                
                if types.len() == 1 && !components.iter().any(|c| visited.contains(c)) {
                    let matched = self.check_if_matched(&components);
                    
                    patterns.push(CircuitPattern::ParallelArray {
                        components: components.clone(),
                        matched,
                    });
                    
                    for comp in &components {
                        visited.insert(comp.clone());
                    }
                }
            }
        }
        
        patterns
    }
    
    /// Detect symmetric circuit structures
    fn detect_symmetry(&self) -> Vec<CircuitPattern> {
        // Simplified implementation - detect bridge circuits and differential pairs
        let mut patterns = Vec::new();
        
        // Look for bridge structures (4 components forming a diamond)
        // This is a placeholder - real implementation would be more sophisticated
        
        patterns
    }
    
    /// Detect hierarchical/modular structures
    fn detect_hierarchical(&self) -> Vec<CircuitPattern> {
        // Detect weakly coupled subcircuits
        let mut patterns = Vec::new();
        
        // Placeholder - would use graph partitioning algorithms
        
        patterns
    }
    
    /// Trace paths from a voltage source to ground
    fn trace_paths_from_source(&self, source_name: &str) -> Option<Vec<Vec<String>>> {
        // Find the voltage source branch
        let (source_idx, _source_branch) = self.circuit.branches()
            .find(|(_, b)| b.name == source_name)?;
        
        // Get the endpoints of the voltage source
        let (pos_node, neg_node) = self.circuit.graph.edge_endpoints(source_idx)?;
        
        // For voltage sources, we want to trace from the positive terminal
        // But we need to find which is positive and which is ground/negative
        // In typical circuits, GND has a lower index or is marked as ground
        let start_node = if self.circuit.get_node_by_id(neg_node)
            .map(|n| n.is_ground)
            .unwrap_or(false) {
            pos_node  // neg is ground, so pos is the start
        } else if self.circuit.get_node_by_id(pos_node)
            .map(|n| n.is_ground)
            .unwrap_or(false) {
            neg_node  // pos is ground, so neg is the start (unlikely)
        } else {
            // Neither is explicitly ground, use convention that higher node is positive
            if pos_node > neg_node { pos_node } else { neg_node }
        };
        
        let ground_node = if start_node == pos_node { neg_node } else { pos_node };
        
        // Simple path tracing - find all components between positive terminal and ground
        let mut paths = Vec::new();
        let mut current_path = Vec::new();
        
        // DFS to find paths (simplified - assumes series circuit)
        self.trace_from_node(start_node, ground_node, &mut current_path, &mut paths);
        
        if paths.is_empty() {
            None
        } else {
            Some(paths)
        }
    }
    
    /// Helper for DFS path tracing (iterative to avoid stack overflow)
    fn trace_from_node(&self, start: NodeIndex, target: NodeIndex, 
                       _path: &mut Vec<String>, paths: &mut Vec<Vec<String>>) {
        // Use BFS to find a simple path
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        let mut parent: HashMap<NodeIndex, (NodeIndex, String)> = HashMap::new();
        
        queue.push_back(start);
        visited.insert(start);
        
        while let Some(current) = queue.pop_front() {
            if current == target {
                // Reconstruct path
                let mut path = Vec::new();
                let mut node = current;
                
                while let Some((prev_node, branch_name)) = parent.get(&node) {
                    path.push(branch_name.clone());
                    node = *prev_node;
                    if node == start {
                        break;
                    }
                }
                
                path.reverse();
                if !path.is_empty() {
                    paths.push(path);
                }
                return; // Found one path is enough for now
            }
            
            // Explore neighbors
            for (idx, branch) in self.circuit.branches() {
                if branch.component_type == "VoltageSource" {
                    continue;
                }
                
                if let Some((from, to)) = self.circuit.graph.edge_endpoints(idx) {
                    let next_node = if from == current && !visited.contains(&to) { 
                        Some(to) 
                    } else if to == current && !visited.contains(&from) { 
                        Some(from) 
                    } else { 
                        None 
                    };
                    
                    if let Some(next) = next_node {
                        visited.insert(next);
                        parent.insert(next, (current, branch.name.clone()));
                        queue.push_back(next);
                    }
                }
            }
        }
    }
    
    /// Check if components in a parallel array are matched (similar parameters)
    fn check_if_matched(&self, components: &[String]) -> bool {
        // In a real implementation, this would check component parameters
        // For now, assume matched if they have the same type
        true
    }
    
    // Get branch nodes helper
    // This would be implemented properly with the Circuit struct
}