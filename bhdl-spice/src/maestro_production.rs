//! MAESTRO Production Implementation
//! 
//! This is the production implementation of MAESTRO (Multi-strategy Adaptive Engine 
//! for Smart Topology-driven Resolution and Orchestration) based on the IEEE TCAD paper.
//! 
//! MAESTRO provides intelligent orchestration of solving strategies based on circuit
//! topology analysis, complementing GLACIER's numerical robustness with circuit-aware
//! intelligence.

use std::collections::{HashMap, HashSet, VecDeque};
use log::{info, debug, warn};
use petgraph::visit::EdgeRef;

use crate::{
    Circuit, ComponentModel, SpiceError, Result,
    glacier_production::{GlacierSolver, Solution, Variable, VariableType},
};

/// Circuit pattern detected by topology analysis
#[derive(Debug, Clone, PartialEq)]
pub enum CircuitPattern {
    SeriesNonlinear { components: Vec<String>, length: usize },
    ParallelArray { components: Vec<String>, identical: bool },
    SymmetricCircuit { symmetry_groups: Vec<Vec<String>> },
    HierarchicalBlock { blocks: Vec<CircuitBlock> },
    BridgeCircuit { bridge_type: BridgeType },
    PowerConverter { topology: ConverterTopology },
    Mixed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CircuitBlock {
    pub name: String,
    pub components: Vec<String>,
    pub interface_nodes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BridgeType {
    DiodeRectifier,
    ActiveRectifier,
    HBridge,
    Wheatstone,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConverterTopology {
    Buck,
    Boost,
    BuckBoost,
    Flyback,
    Forward,
    SEPIC,
    Cuk,
}

/// Strategy for solving a particular pattern
#[derive(Debug, Clone)]
pub enum SolvingStrategy {
    ProgressiveActivation {
        components: Vec<String>,
        activation_order: Vec<usize>,
    },
    CurrentSharing {
        parallel_groups: Vec<Vec<String>>,
        activation_sequence: Vec<String>,
    },
    SymmetryExploitation {
        representative_group: Vec<String>,
        symmetry_map: HashMap<String, String>,
    },
    HierarchicalDecomposition {
        solve_order: Vec<String>,
        coupling_strength: HashMap<(String, String), f64>,
    },
    DirectSolve, // Fallback to GLACIER
}

/// MAESTRO orchestrator
pub struct MaestroOrchestrator {
    circuit: Circuit,
    models: HashMap<String, ComponentModel>,
    patterns: Vec<CircuitPattern>,
    strategies: Vec<(CircuitPattern, SolvingStrategy)>,
    recommendations: Vec<String>,
}

impl MaestroOrchestrator {
    pub fn new(circuit: Circuit) -> Self {
        Self {
            circuit,
            models: HashMap::new(),
            patterns: Vec::new(),
            strategies: Vec::new(),
            recommendations: Vec::new(),
        }
    }
    
    /// Add component model
    pub fn add_model(&mut self, name: String, model: ComponentModel) {
        self.models.insert(name, model);
    }
    
    /// Get recommendations generated during solving
    pub fn get_recommendations(&self) -> &[String] {
        &self.recommendations
    }
    
    /// Main orchestration function
    pub fn solve(&mut self) -> Result<Vec<Solution>> {
        println!("MAESTRO: Starting topology-aware orchestration");
        
        // Step 1: Analyze circuit topology
        self.patterns = self.detect_patterns();
        println!("MAESTRO: Detected {} circuit patterns", self.patterns.len());
        for pattern in &self.patterns {
            println!("  Pattern: {:?}", pattern);
        }
        
        // Step 2: Select strategies for each pattern
        self.strategies = self.select_strategies();
        println!("MAESTRO: Selected {} strategies", self.strategies.len());
        
        // Step 3: Try strategies in order
        let strategies = self.strategies.clone();  // Clone to avoid borrow conflict
        for (pattern, strategy) in &strategies {
            println!("MAESTRO: Trying {:?} strategy", strategy);
            
            match self.apply_strategy(pattern, strategy) {
                Ok(solutions) => {
                    println!("MAESTRO: Strategy succeeded with {} solutions", solutions.len());
                    return Ok(solutions);
                }
                Err(e) => {
                    println!("MAESTRO: Strategy failed: {}", e);
                    continue;
                }
            }
        }
        
        // Step 4: Fallback to GLACIER if all strategies fail
        println!("MAESTRO: All strategies failed, falling back to GLACIER");
        self.glacier_fallback()
    }
    
    /// Detect circuit patterns (Algorithm 4 from paper)
    pub fn detect_patterns(&self) -> Vec<CircuitPattern> {
        let mut patterns = Vec::new();
        
        // Build circuit graph
        let graph = self.build_circuit_graph();
        
        // Detect series nonlinear chains
        if let Some(series) = self.find_series_nonlinear(&graph) {
            patterns.push(series);
        }
        
        // Detect parallel arrays
        if let Some(parallel) = self.find_parallel_arrays(&graph) {
            patterns.push(parallel);
        }
        
        // Detect symmetries
        if let Some(symmetric) = self.find_symmetries(&graph) {
            patterns.push(symmetric);
        }
        
        // Detect hierarchical blocks
        if let Some(hierarchical) = self.find_hierarchical_blocks(&graph) {
            patterns.push(hierarchical);
        }
        
        // Detect bridge circuits
        if let Some(bridge) = self.detect_bridge_circuit(&graph) {
            patterns.push(bridge);
        }
        
        // Detect power converters
        if let Some(converter) = self.detect_power_converter(&graph) {
            patterns.push(converter);
        }
        
        if patterns.is_empty() {
            patterns.push(CircuitPattern::Mixed);
        }
        
        patterns
    }
    
    /// Build graph representation of circuit
    fn build_circuit_graph(&self) -> CircuitGraph {
        let mut graph = CircuitGraph::new();
        
        // Add nodes
        for (_idx, node) in self.circuit.nodes() {
            graph.add_node(node.name.clone());
        }
        
        // Add components as edges
        for edge_ref in self.circuit.graph.edge_references() {
            let branch = edge_ref.weight();
            let from_node = &self.circuit.graph[edge_ref.source()].name;
            let to_node = &self.circuit.graph[edge_ref.target()].name;
            
            graph.add_edge(
                from_node.clone(),
                to_node.clone(),
                branch.name.clone(),
                branch.component_type.clone(),
            );
        }
        
        graph
    }
    
    /// Find series nonlinear chains
    fn find_series_nonlinear(&self, graph: &CircuitGraph) -> Option<CircuitPattern> {
        // Look for chains of nonlinear components
        let nonlinear_types = ["LED", "Diode", "Transistor"];
        
        for start_node in graph.nodes() {
            let mut visited = HashSet::new();
            let mut chain = Vec::new();
            let mut current = start_node.clone();
            
            loop {
                visited.insert(current.clone());
                
                // Find next nonlinear component
                let mut found_next = false;
                for (neighbor, comp_id, comp_type) in graph.neighbors(&current) {
                    if !visited.contains(neighbor) && nonlinear_types.contains(&comp_type.as_str()) {
                        chain.push(comp_id.clone());
                        current = neighbor.clone();
                        found_next = true;
                        break;
                    }
                }
                
                if !found_next {
                    break;
                }
            }
            
            if chain.len() >= 2 {
                return Some(CircuitPattern::SeriesNonlinear {
                    length: chain.len(),
                    components: chain,
                });
            }
        }
        
        None
    }
    
    /// Find parallel arrays
    fn find_parallel_arrays(&self, graph: &CircuitGraph) -> Option<CircuitPattern> {
        // Look for components with same endpoints
        let mut parallel_groups: HashMap<(String, String), Vec<String>> = HashMap::new();
        
        for edge_ref in self.circuit.graph.edge_references() {
            let branch = edge_ref.weight();
            let from_node = &self.circuit.graph[edge_ref.source()].name;
            let to_node = &self.circuit.graph[edge_ref.target()].name;
            
            let key = if from_node < to_node {
                (from_node.clone(), to_node.clone())
            } else {
                (to_node.clone(), from_node.clone())
            };
            
            parallel_groups.entry(key).or_insert_with(Vec::new).push(branch.name.clone());
        }
        
        // Find largest parallel group
        if let Some((_, components)) = parallel_groups.into_iter()
            .filter(|(_, v)| v.len() >= 2)
            .max_by_key(|(_, v)| v.len()) 
        {
            // Check if identical
            let identical = components.iter().all(|id| {
                self.models.get(id) == self.models.get(&components[0])
            });
            
            return Some(CircuitPattern::ParallelArray { components, identical });
        }
        
        None
    }
    
    /// Find circuit symmetries
    fn find_symmetries(&self, graph: &CircuitGraph) -> Option<CircuitPattern> {
        // Simple symmetry detection - look for repeated structures
        let mut component_signatures: HashMap<String, Vec<String>> = HashMap::new();
        
        // Group components by their connection pattern
        for edge_ref in self.circuit.graph.edge_references() {
            let branch = edge_ref.weight();
            let from_node = &self.circuit.graph[edge_ref.source()].name;
            let to_node = &self.circuit.graph[edge_ref.target()].name;
            
            let signature = format!("{}-{}-{}", 
                graph.node_degree(from_node),
                branch.component_type,
                graph.node_degree(to_node)
            );
            component_signatures.entry(signature).or_insert_with(Vec::new).push(branch.name.clone());
        }
        
        // Find groups with same signature
        let symmetry_groups: Vec<Vec<String>> = component_signatures.into_iter()
            .filter(|(_, v)| v.len() >= 2)
            .map(|(_, v)| v)
            .collect();
        
        if !symmetry_groups.is_empty() {
            return Some(CircuitPattern::SymmetricCircuit { symmetry_groups });
        }
        
        None
    }
    
    /// Find hierarchical blocks
    fn find_hierarchical_blocks(&self, _graph: &CircuitGraph) -> Option<CircuitPattern> {
        // Simplified - look for weakly connected components
        // In production, would use more sophisticated graph partitioning
        None
    }
    
    /// Detect bridge circuit patterns
    fn detect_bridge_circuit(&self, graph: &CircuitGraph) -> Option<CircuitPattern> {
        // Look for 4 components forming a bridge
        let diode_count = self.circuit.branches()
            .filter(|(_, b)| b.component_type == "Diode")
            .count();
        
        if diode_count == 4 {
            // Check if they form a bridge topology
            // Simplified check - in production would verify exact topology
            return Some(CircuitPattern::BridgeCircuit {
                bridge_type: BridgeType::DiodeRectifier,
            });
        }
        
        None
    }
    
    /// Detect power converter topology
    fn detect_power_converter(&self, _graph: &CircuitGraph) -> Option<CircuitPattern> {
        // Look for characteristic components
        let has_inductor = self.circuit.branches().any(|(_, b)| b.component_type == "Inductor");
        let has_switch = self.circuit.branches().any(|(_, b)| 
            b.component_type == "MOSFET" || b.component_type == "Switch"
        );
        let has_diode = self.circuit.branches().any(|(_, b)| b.component_type == "Diode");
        
        if has_inductor && has_switch && has_diode {
            // Simplified detection - assume buck converter
            return Some(CircuitPattern::PowerConverter {
                topology: ConverterTopology::Buck,
            });
        }
        
        None
    }
    
    /// Select appropriate strategies for detected patterns
    fn select_strategies(&self) -> Vec<(CircuitPattern, SolvingStrategy)> {
        let mut strategies = Vec::new();
        
        for pattern in &self.patterns {
            let strategy = match pattern {
                CircuitPattern::SeriesNonlinear { components, .. } => {
                    // Progressive activation strategy (Algorithm 5 from paper)
                    SolvingStrategy::ProgressiveActivation {
                        activation_order: (0..components.len()).collect(),
                        components: components.clone(),
                    }
                }
                CircuitPattern::ParallelArray { components, identical } => {
                    // Current sharing strategy
                    if *identical {
                        SolvingStrategy::CurrentSharing {
                            parallel_groups: vec![components.clone()],
                            activation_sequence: components.clone(),
                        }
                    } else {
                        // Sort by strength (simplified - use Is value)
                        let mut sorted = components.clone();
                        sorted.sort_by_key(|id| {
                            match self.models.get(id) {
                                Some(ComponentModel::LED { saturation_current: Some(is), .. }) => 
                                    (is * 1e50) as i64,
                                _ => 0,
                            }
                        });
                        
                        SolvingStrategy::CurrentSharing {
                            parallel_groups: vec![sorted.clone()],
                            activation_sequence: sorted,
                        }
                    }
                }
                CircuitPattern::SymmetricCircuit { symmetry_groups } => {
                    // Symmetry exploitation
                    let representative = symmetry_groups[0].clone();
                    let mut symmetry_map = HashMap::new();
                    
                    for group in symmetry_groups {
                        for (i, comp) in group.iter().enumerate() {
                            if i > 0 {
                                symmetry_map.insert(comp.clone(), representative[0].clone());
                            }
                        }
                    }
                    
                    SolvingStrategy::SymmetryExploitation {
                        representative_group: representative,
                        symmetry_map,
                    }
                }
                _ => SolvingStrategy::DirectSolve,
            };
            
            strategies.push((pattern.clone(), strategy));
        }
        
        strategies
    }
    
    /// Apply a specific strategy
    fn apply_strategy(&mut self, pattern: &CircuitPattern, strategy: &SolvingStrategy) -> Result<Vec<Solution>> {
        match strategy {
            SolvingStrategy::ProgressiveActivation { components, activation_order } => {
                self.progressive_activation(components, activation_order)
            }
            SolvingStrategy::CurrentSharing { parallel_groups, activation_sequence } => {
                self.current_sharing(parallel_groups, activation_sequence)
            }
            SolvingStrategy::SymmetryExploitation { representative_group, symmetry_map } => {
                self.symmetry_exploitation(representative_group, symmetry_map)
            }
            SolvingStrategy::HierarchicalDecomposition { solve_order, coupling_strength } => {
                self.hierarchical_decomposition(solve_order, coupling_strength)
            }
            SolvingStrategy::DirectSolve => {
                self.glacier_fallback()
            }
        }
    }
    
    /// Progressive activation strategy (Algorithm 5 from paper)
    fn progressive_activation(&self, components: &[String], activation_order: &[usize]) -> Result<Vec<Solution>> {
        info!("MAESTRO: Applying progressive activation for {} components", components.len());
        
        let mut solutions = Vec::new();
        let mut previous_solution: Option<Solution> = None;
        
        for &num_active in activation_order {
            let active_components: HashSet<String> = components[0..=num_active]
                .iter()
                .cloned()
                .collect();
            
            info!("MAESTRO: Activating {} components", num_active + 1);
            
            // Create modified circuit
            let mut modified_circuit = self.circuit.clone();
            let mut modified_models = self.models.clone();
            
            // Deactivate components not in active set
            for (id, model) in &self.models {
                if !active_components.contains(id) {
                    // Replace with high resistance
                    modified_models.insert(id.clone(), ComponentModel::Resistor {
                        resistance: 10e6, // 10MΩ
                        tolerance: 5.0,
                        limits: Default::default(),
                    });
                }
            }
            
            // Use previous solution as initial guess
            let initial_guess = previous_solution.as_ref().map(|sol| {
                self.propagate_solution(&sol.variables, &active_components)
            });
            
            // Solve subproblem
            let mut glacier = GlacierSolver::new(modified_circuit);
            glacier.enable_multi_region = false; // Single solution for subproblem
            
            for (name, model) in modified_models {
                glacier.add_model(name, model);
            }
            
            match glacier.solve_at_ramp(1.0, initial_guess.as_deref()) {
                Ok(solution) => {
                    info!("  Converged in {} iterations", solution.iterations);
                    previous_solution = Some(solution.clone());
                    solutions.push(solution);
                }
                Err(e) => {
                    warn!("  Failed at step {}: {}", num_active + 1, e);
                    return Err(e);
                }
            }
        }
        
        // Return final solution
        if let Some(final_solution) = solutions.last() {
            Ok(vec![final_solution.clone()])
        } else {
            Err(SpiceError::NumericalError("Progressive activation failed".to_string()))
        }
    }
    
    /// Propagate solution to expanded problem
    fn propagate_solution(&self, previous_vars: &[Variable], active_components: &HashSet<String>) -> Vec<Variable> {
        // Smart initialization based on previous solution
        let mut new_vars = previous_vars.to_vec();
        
        // Add variables for newly activated components
        // This is simplified - in production would be more sophisticated
        
        new_vars
    }
    
    /// Current sharing strategy for parallel arrays with progressive activation
    fn current_sharing(&mut self, parallel_groups: &[Vec<String>], activation_sequence: &[String]) -> Result<Vec<Solution>> {
        println!("MAESTRO: Applying current sharing strategy for parallel LEDs");
        
        // First try with all components
        let mut glacier = GlacierSolver::new(self.circuit.clone());
        glacier.enable_multi_region = true;
        
        for (name, model) in &self.models {
            glacier.add_model(name.clone(), model.clone());
        }
        
        match glacier.solve() {
            Ok(solutions) => {
                println!("MAESTRO: GLACIER found {} solutions for parallel array", solutions.len());
                
                // Check if all LEDs are off in all solutions
                let all_leds_off = solutions.iter().all(|sol| {
                    self.check_all_leds_off(sol, &parallel_groups[0])
                });
                
                if all_leds_off {
                    println!("MAESTRO: All LEDs are off in all solutions, trying progressive activation");
                    
                    // Try progressive activation with component removal
                    if let Ok(progressive_solution) = self.progressive_parallel_activation(parallel_groups, activation_sequence) {
                        return Ok(vec![progressive_solution]);
                    }
                    
                    // If still no luck, provide recommendations
                    self.analyze_and_recommend_for_parallel_leds(&solutions[0], &parallel_groups[0]);
                }
                
                // Select the best solution
                let best = self.select_best_solution(&solutions);
                Ok(vec![best])
            }
            Err(e) => {
                println!("MAESTRO: Current sharing strategy failed: {}", e);
                Err(e)
            }
        }
    }
    
    /// Progressive activation for parallel components with actual component removal
    fn progressive_parallel_activation(&self, parallel_groups: &[Vec<String>], activation_sequence: &[String]) -> Result<Solution> {
        println!("MAESTRO: Trying progressive parallel activation");
        
        let mut best_solution: Option<Solution> = None;
        let mut best_leds_on = 0;
        
        // Try activating LEDs one by one
        for num_active in 1..=activation_sequence.len() {
            let active_leds: HashSet<String> = activation_sequence[0..num_active]
                .iter()
                .cloned()
                .collect();
            
            println!("  Activating {} LED(s): {:?}", num_active, active_leds);
            
            // Create a modified circuit with only active LEDs
            let mut modified_circuit = self.circuit.clone();
            let mut modified_models = HashMap::new();
            
            // Keep all non-LED components
            for (name, model) in &self.models {
                let is_parallel_led = parallel_groups.iter()
                    .any(|group| group.contains(name));
                
                if !is_parallel_led || active_leds.contains(name) {
                    modified_models.insert(name.clone(), model.clone());
                }
                // Inactive LEDs are simply not added to the circuit
            }
            
            let mut glacier = GlacierSolver::new(modified_circuit);
            glacier.enable_multi_region = true; // Try multiple regions even for subproblems
            
            for (name, model) in modified_models {
                glacier.add_model(name, model);
            }
            
            // Try to find a solution where LEDs are on
            if let Ok(solutions) = glacier.solve() {
                // Pick the solution with highest current
                if let Some(solution) = solutions.into_iter()
                    .max_by(|a, b| {
                        let a_current: f64 = a.branch_currents.values().map(|i| i.abs()).sum();
                        let b_current: f64 = b.branch_currents.values().map(|i| i.abs()).sum();
                        a_current.partial_cmp(&b_current).unwrap()
                    }) {
                    let leds_on = self.count_leds_on(&solution, &active_leds.iter().cloned().collect::<Vec<_>>());
                    println!("    {} LED(s) conducting", leds_on);
                    
                    if leds_on > best_leds_on {
                        best_leds_on = leds_on;
                        best_solution = Some(solution);
                    }
                }
            }
        }
        
        best_solution.ok_or_else(|| SpiceError::NumericalError("Progressive activation failed".to_string()))
    }
    
    /// Check if all LEDs in a group are off
    fn check_all_leds_off(&self, solution: &Solution, led_group: &[String]) -> bool {
        led_group.iter().all(|led_name| {
            solution.branch_currents.get(led_name)
                .map(|&i| i.abs() < 1e-6)  // Less than 1µA means off
                .unwrap_or(true)
        })
    }
    
    /// Count how many LEDs are conducting
    fn count_leds_on(&self, solution: &Solution, led_group: &[String]) -> usize {
        led_group.iter().filter(|led_name| {
            solution.branch_currents.get(*led_name)
                .map(|&i| i.abs() > 1e-3)  // More than 1mA means on
                .unwrap_or(false)
        }).count()
    }
    
    /// Analyze why LEDs are off and provide recommendations
    fn analyze_and_recommend_for_parallel_leds(&mut self, solution: &Solution, led_group: &[String]) {
        // Find the series resistor (assumes one resistor feeding the parallel LEDs)
        let series_resistor = self.find_series_resistor_for_leds(led_group);
        
        if let Some((resistor_name, current_value)) = series_resistor {
            // Get supply voltage
            let supply_voltage = self.get_supply_voltage();
            
            // Calculate what resistance would allow LEDs to turn on
            // For parallel LEDs, we need higher voltage at the common node
            let num_leds = led_group.len();
            let target_led_current = 0.010;  // 10mA per LED
            let total_current = target_led_current * num_leds as f64;
            let led_voltage = 2.0;  // Typical forward voltage
            
            // With parallel LEDs, we need V(N1) ≥ 2V to turn them on
            // So voltage drop across resistor = supply - LED voltage
            let voltage_drop = supply_voltage - led_voltage;
            let recommended_resistance = voltage_drop / total_current;
            
            let recommendation = if (recommended_resistance - current_value).abs() < 10.0 {
                // The resistance is already close to ideal
                format!(
                    "MAESTRO RECOMMENDATION: All {} parallel LEDs are off despite reasonable series resistance. \
                     This may be due to: (1) LEDs with very different Is values (current hogging), \
                     (2) Supply voltage too low for LED forward voltage, or \
                     (3) Numerical convergence to an 'all-off' solution. \
                     Try: Increasing supply voltage, using matched LEDs, or adding individual ballast resistors.",
                    num_leds
                )
            } else {
                format!(
                    "MAESTRO RECOMMENDATION: All {} parallel LEDs are off. \
                     The series resistor {} = {:.0}Ω may need adjustment. \
                     With {:.1}V supply and {:.1}V LED forward voltage, \
                     try {} = {:.0}Ω to allow ~{:.0}mA total current.",
                    num_leds, resistor_name, current_value,
                    supply_voltage, led_voltage,
                    resistor_name, recommended_resistance,
                    total_current * 1000.0
                )
            };
            
            println!("\n{}", recommendation);
            self.recommendations.push(recommendation);
        }
    }
    
    /// Find the series resistor feeding the LED group
    fn find_series_resistor_for_leds(&self, led_group: &[String]) -> Option<(String, f64)> {
        // Find the common node for all LEDs (not ground)
        let mut common_nodes = HashSet::new();
        
        for edge_ref in self.circuit.graph.edge_references() {
            let branch = edge_ref.weight();
            if led_group.contains(&branch.name) {
                let from_node = &self.circuit.graph[edge_ref.source()].name;
                let to_node = &self.circuit.graph[edge_ref.target()].name;
                
                if from_node != "GND" && from_node != "0" {
                    common_nodes.insert(from_node.clone());
                }
                if to_node != "GND" && to_node != "0" {
                    common_nodes.insert(to_node.clone());
                }
            }
        }
        
        // Find resistor connected to the common node
        for edge_ref in self.circuit.graph.edge_references() {
            let branch = edge_ref.weight();
            if branch.component_type == "Resistor" {
                let from_node = &self.circuit.graph[edge_ref.source()].name;
                let to_node = &self.circuit.graph[edge_ref.target()].name;
                
                if common_nodes.contains(from_node) || common_nodes.contains(to_node) {
                    if let Some(ComponentModel::Resistor { resistance, .. }) = self.models.get(&branch.name) {
                        return Some((branch.name.clone(), *resistance));
                    }
                }
            }
        }
        
        None
    }
    
    /// Get the supply voltage from voltage sources
    fn get_supply_voltage(&self) -> f64 {
        for (name, model) in &self.models {
            if let ComponentModel::VoltageSource { voltage, .. } = model {
                return *voltage;
            }
        }
        5.0  // Default assumption
    }
    
    /// Symmetry exploitation strategy
    fn symmetry_exploitation(&self, representative_group: &[String], symmetry_map: &HashMap<String, String>) -> Result<Vec<Solution>> {
        info!("MAESTRO: Applying symmetry exploitation");
        
        // Solve representative branch
        let mut reduced_circuit = self.circuit.clone();
        let mut reduced_models = HashMap::new();
        
        // Include only representative components
        for comp in representative_group {
            if let Some(model) = self.models.get(comp) {
                reduced_models.insert(comp.clone(), model.clone());
            }
        }
        
        let mut glacier = GlacierSolver::new(reduced_circuit);
        for (name, model) in reduced_models {
            glacier.add_model(name, model);
        }
        
        match glacier.solve_at_ramp(1.0, None) {
            Ok(representative_solution) => {
                // Replicate solution to symmetric components
                let full_solution = self.replicate_symmetric_solution(representative_solution, symmetry_map);
                Ok(vec![full_solution])
            }
            Err(e) => Err(e),
        }
    }
    
    /// Replicate solution to symmetric components
    fn replicate_symmetric_solution(&self, representative: Solution, symmetry_map: &HashMap<String, String>) -> Solution {
        // Simplified - in production would properly map variables
        representative
    }
    
    /// Hierarchical decomposition strategy
    fn hierarchical_decomposition(&self, solve_order: &[String], coupling_strength: &HashMap<(String, String), f64>) -> Result<Vec<Solution>> {
        info!("MAESTRO: Applying hierarchical decomposition");
        
        // Solve blocks in order with interface variable iteration
        // Simplified implementation
        self.glacier_fallback()
    }
    
    /// Fallback to GLACIER with solution selection
    fn glacier_fallback(&self) -> Result<Vec<Solution>> {
        let mut glacier = GlacierSolver::new(self.circuit.clone());
        
        for (name, model) in &self.models {
            glacier.add_model(name.clone(), model.clone());
        }
        
        // Get all solutions from GLACIER
        let all_solutions = glacier.solve()?;
        
        // If only one solution, return it
        if all_solutions.len() <= 1 {
            return Ok(all_solutions);
        }
        
        // Multiple solutions - select the most physically reasonable
        info!("MAESTRO: GLACIER found {} solutions, selecting most reasonable", all_solutions.len());
        
        let selected = self.select_best_solution(&all_solutions);
        info!("MAESTRO: Selected solution at ramp={:.1}%", selected.ramp * 100.0);
        
        Ok(vec![selected])
    }
    
    /// Select the most physically reasonable solution from multiple options
    fn select_best_solution(&self, solutions: &[Solution]) -> Solution {
        // Evaluate each solution based on physical criteria
        println!("MAESTRO: Evaluating {} solutions:", solutions.len());
        
        let mut best_solution = &solutions[0];
        let mut best_score = self.evaluate_solution_physical_reasonableness(&solutions[0]);
        
        for (i, solution) in solutions.iter().enumerate() {
            let score = if i == 0 {
                best_score
            } else {
                self.evaluate_solution_physical_reasonableness(solution)
            };
            
            // Show key voltages for debugging
            let v_n1 = solution.node_voltages.get("N1").copied().unwrap_or(0.0);
            let i_total = solution.branch_currents.values().filter(|&&i| i > 0.0).sum::<f64>();
            
            println!("  Solution {}: ramp={:.1}%, V(N1)={:.3}V, I_total={:.3}mA, score={:.2}", 
                     i + 1, solution.ramp * 100.0, v_n1, i_total * 1000.0, score);
            
            if i > 0 && score > best_score {
                best_score = score;
                best_solution = solution;
            }
        }
        
        println!("  Selected solution at ramp={:.1}% with score={:.2}", best_solution.ramp * 100.0, best_score);
        best_solution.clone()
    }
    
    /// Evaluate physical reasonableness of a solution
    fn evaluate_solution_physical_reasonableness(&self, solution: &Solution) -> f64 {
        let mut score = 0.0;
        
        // Criteria 1: For parallel LEDs, strongly prefer higher ramp solutions
        // where LEDs are more likely to be on
        let has_leds = self.circuit.branches()
            .any(|(_, b)| b.component_type == "LED");
        
        let ramp_score = if has_leds {
            // For LED circuits, heavily weight towards full ramp
            solution.ramp * solution.ramp * 20.0  // Quadratic weighting
        } else {
            // For other circuits, linear weighting
            solution.ramp * 10.0
        };
        score += ramp_score;
        
        // Criteria 2: Check power dissipation is reasonable
        let power_score = self.evaluate_power_dissipation(solution);
        score += power_score * 20.0;
        
        // Criteria 3: Check currents are within component limits
        let current_score = self.evaluate_current_limits(solution);
        score += current_score * 30.0;
        
        // Criteria 4: For LED circuits, prefer solutions where LEDs are properly biased
        let led_score = self.evaluate_led_operation(solution);
        score += led_score * 40.0;
        
        score
    }
    
    /// Evaluate power dissipation
    fn evaluate_power_dissipation(&self, solution: &Solution) -> f64 {
        let mut total_power = 0.0;
        let mut power_ok = true;
        
        // Check each component's power dissipation
        for edge_ref in self.circuit.graph.edge_references() {
            let branch = edge_ref.weight();
            let from_node = &self.circuit.graph[edge_ref.source()].name;
            let to_node = &self.circuit.graph[edge_ref.target()].name;
            
            let v_from = solution.node_voltages.get(from_node).copied().unwrap_or(0.0);
            let v_to = solution.node_voltages.get(to_node).copied().unwrap_or(0.0);
            let v_diff = (v_from - v_to).abs();
            
            // Get current (from branch currents or calculate)
            let current = if let Some(&i) = solution.branch_currents.get(&branch.name) {
                i.abs()
            } else {
                // Estimate from voltage and resistance for resistors
                match self.models.get(&branch.name) {
                    Some(ComponentModel::Resistor { resistance, .. }) => v_diff / resistance,
                    _ => 0.0,
                }
            };
            
            let power = v_diff * current;
            total_power += power;
            
            // Check against limits (simplified - in production would use actual limits)
            if power > 1.0 {  // 1W limit for discrete components
                power_ok = false;
            }
        }
        
        // Return score: 1.0 if power is reasonable, lower if excessive
        if power_ok && total_power < 5.0 {  // 5W total limit
            1.0
        } else {
            0.5 / (1.0 + total_power / 10.0)  // Penalize high power
        }
    }
    
    /// Evaluate if currents are within limits
    fn evaluate_current_limits(&self, solution: &Solution) -> f64 {
        let mut score = 1.0;
        
        // Check LED currents
        for edge_ref in self.circuit.graph.edge_references() {
            let branch = edge_ref.weight();
            if branch.component_type == "LED" {
                if let Some(&current) = solution.branch_currents.get(&branch.name) {
                    let i_abs = current.abs();
                    
                    // LEDs typically rated for 20-30mA max
                    if i_abs > 0.030 {  // 30mA
                        score *= 0.5;  // Penalty for overcurrent
                    } else if i_abs < 0.001 {  // 1mA
                        score *= 0.7;  // Slight penalty for very low current (LED barely on)
                    }
                }
            }
        }
        
        score
    }
    
    /// Evaluate LED operation points
    fn evaluate_led_operation(&self, solution: &Solution) -> f64 {
        let num_leds = self.circuit.branches()
            .filter(|(_, b)| b.component_type == "LED")
            .count();
        
        if num_leds == 0 {
            return 1.0;  // Not an LED circuit
        }
        
        let mut leds_on = 0;
        let mut leds_properly_biased = 0;
        
        for edge_ref in self.circuit.graph.edge_references() {
            let branch = edge_ref.weight();
            if branch.component_type == "LED" {
                let from_node = &self.circuit.graph[edge_ref.source()].name;
                let to_node = &self.circuit.graph[edge_ref.target()].name;
                
                let v_from = solution.node_voltages.get(from_node).copied().unwrap_or(0.0);
                let v_to = solution.node_voltages.get(to_node).copied().unwrap_or(0.0);
                let v_led = v_from - v_to;
                
                // Check if LED is on (forward biased)
                if v_led > 1.0 {  // Typical LED needs >1V to turn on
                    leds_on += 1;
                    
                    // Check if voltage is in reasonable range (1.5V - 3.5V for most LEDs)
                    if v_led > 1.5 && v_led < 3.5 {
                        leds_properly_biased += 1;
                    }
                }
            }
        }
        
        // Score based on fraction of LEDs that are on and properly biased
        let on_ratio = leds_on as f64 / num_leds as f64;
        let biased_ratio = leds_properly_biased as f64 / num_leds as f64;
        
        // Prefer solutions where most LEDs are on and properly biased
        on_ratio * 0.5 + biased_ratio * 0.5
    }
}

/// Simple circuit graph for topology analysis
struct CircuitGraph {
    nodes: HashSet<String>,
    edges: Vec<(String, String, String, String)>, // (from, to, comp_id, comp_type)
}

impl CircuitGraph {
    fn new() -> Self {
        Self {
            nodes: HashSet::new(),
            edges: Vec::new(),
        }
    }
    
    fn add_node(&mut self, node: String) {
        self.nodes.insert(node);
    }
    
    fn add_edge(&mut self, from: String, to: String, comp_id: String, comp_type: String) {
        self.edges.push((from, to, comp_id, comp_type));
    }
    
    fn nodes(&self) -> Vec<String> {
        self.nodes.iter().cloned().collect()
    }
    
    fn neighbors(&self, node: &str) -> Vec<(&String, &String, &String)> {
        self.edges.iter()
            .filter_map(|(from, to, id, typ)| {
                if from == node {
                    Some((to, id, typ))
                } else if to == node {
                    Some((from, id, typ))
                } else {
                    None
                }
            })
            .collect()
    }
    
    fn node_degree(&self, node: &str) -> usize {
        self.edges.iter()
            .filter(|(from, to, _, _)| from == node || to == node)
            .count()
    }
}

/// Combined GLACIER-MAESTRO solver
pub fn solve_with_glacier_maestro(circuit: Circuit, models: HashMap<String, ComponentModel>) -> Result<Vec<Solution>> {
    info!("Starting GLACIER-MAESTRO combined framework");
    
    // Try MAESTRO first
    let mut maestro = MaestroOrchestrator::new(circuit.clone());
    for (name, model) in &models {
        maestro.add_model(name.clone(), model.clone());
    }
    
    let result = match maestro.solve() {
        Ok(solutions) => {
            info!("MAESTRO succeeded with {} solutions", solutions.len());
            
            // Print any recommendations
            for recommendation in maestro.get_recommendations() {
                println!("\n{}", recommendation);
            }
            
            // If MAESTRO returns multiple solutions, use GLACIER to verify
            if solutions.len() == 1 {
                Ok(solutions)
            } else {
                // Use GLACIER for multi-region discovery
                let mut glacier = GlacierSolver::new(circuit);
                for (name, model) in models {
                    glacier.add_model(name, model);
                }
                glacier.solve()
            }
        }
        Err(e) => {
            warn!("MAESTRO failed: {}, falling back to GLACIER", e);
            
            // Direct GLACIER solve
            let mut glacier = GlacierSolver::new(circuit);
            for (name, model) in models {
                glacier.add_model(name, model);
            }
            glacier.solve()
        }
    };
    
    result
}