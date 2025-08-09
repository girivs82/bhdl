//! Intelligent SPICE Engine - Separating circuit understanding from numerical solving
//! 
//! This module implements a revolutionary approach where the SPICE engine contains
//! intelligence about circuit patterns and solving strategies, while keeping the
//! numerical solver simple and fast.

pub mod patterns;
pub mod strategies;
pub mod topology_analyzer;
pub mod orchestrator;

use crate::{Circuit, GlacierSolver, Result, AnalysisResult};
use bhdl_netlist::Netlist;
use bhdl_analyzer::flow_tracking::FlowPath;
use std::collections::HashMap;

/// Context provided by the synthesizer containing high-level circuit information
#[derive(Debug, Clone)]
pub struct SynthesizerContext {
    /// Module boundaries and hierarchy
    pub module_instances: HashMap<String, ModuleInfo>,
    
    /// Flow paths with semantic meaning
    pub flow_paths: Vec<FlowPath>,
    
    /// Component roles from instantiation context
    pub component_roles: HashMap<String, SemanticRole>,
    
    /// Net attributes from declarations
    pub net_attributes: HashMap<String, NetAttribute>,
}

/// Information about a module instance
#[derive(Debug, Clone)]
pub struct ModuleInfo {
    pub name: String,
    pub instance_name: String,
    pub parameters: HashMap<String, String>,
    pub component_ids: Vec<String>,
}

/// Semantic role of a component derived from context
#[derive(Debug, Clone, PartialEq)]
pub enum SemanticRole {
    PowerSupply,
    Indicator,
    Protection,
    Signal,
    Control,
    Unknown,
}

/// Attributes of a net from BHDL declarations
#[derive(Debug, Clone)]
pub enum NetAttribute {
    PowerDomain { voltage: f64, current: f64 },
    GroundDomain,
    Signal { bandwidth: Option<f64> },
    Control,
}

/// The main intelligent SPICE engine
pub struct IntelligentSpiceEngine {
    /// Underlying numerical solver
    solver: GlacierSolver,
    
    /// Topology analyzer for pattern recognition
    analyzer: topology_analyzer::TopologyAnalyzer,
    
    /// Available solving strategies
    strategies: Vec<Box<dyn strategies::SolvingStrategy>>,
    
    /// Orchestrator for strategy execution
    orchestrator: orchestrator::Orchestrator,
    
    /// Performance tracking
    performance_tracker: Option<PerformanceTracker>,
}

/// Tracks performance of different strategies
#[derive(Default)]
pub struct PerformanceTracker {
    /// Success rates for pattern-strategy combinations
    pub success_rates: HashMap<(String, String), f64>,
    
    /// Average solving times
    pub average_times: HashMap<(String, String), std::time::Duration>,
    
    /// Total attempts
    pub attempts: HashMap<(String, String), usize>,
}

impl IntelligentSpiceEngine {
    /// Create a new intelligent SPICE engine
    pub fn new(circuit: Circuit) -> Self {
        use strategies::*;
        
        let solver = GlacierSolver::new(circuit);
        let analyzer = topology_analyzer::TopologyAnalyzer::new();
        
        // Register all available strategies
        let strategies: Vec<Box<dyn SolvingStrategy>> = vec![
            Box::new(progressive::ProgressiveTurnOnStrategy::new()),
            Box::new(current_sharing::CurrentSharingStrategy::new()),
            Box::new(symmetry::SymmetryEnforcingStrategy::new()),
            Box::new(state_enum::StateEnumerationStrategy::new()),
            Box::new(DefaultStrategy::new()),
        ];
        
        let orchestrator = orchestrator::Orchestrator::new();
        
        Self {
            solver,
            analyzer,
            strategies,
            orchestrator,
            performance_tracker: Some(PerformanceTracker::default()),
        }
    }
    
    /// Enable or disable performance tracking
    pub fn set_performance_tracking(&mut self, enabled: bool) {
        if enabled && self.performance_tracker.is_none() {
            self.performance_tracker = Some(PerformanceTracker::default());
        } else if !enabled {
            self.performance_tracker = None;
        }
    }
    
    /// Main entry point: solve circuit with intelligent pattern recognition
    pub fn solve(&mut self, context: Option<&SynthesizerContext>) -> Result<Vec<AnalysisResult>> {
        // Step 1: Analyze topology to identify patterns
        let patterns = if let Some(ctx) = context {
            self.analyzer.identify_with_context(self.solver.get_circuit(), ctx)
        } else {
            self.analyzer.identify_patterns(self.solver.get_circuit())
        };
        
        if patterns.is_empty() {
            // No special patterns detected, use default solver
            return self.solver.analyze_simple();
        }
        
        // Step 2: Select appropriate strategies
        let selected_strategy_indices: Vec<usize> = patterns.iter().enumerate().map(|(_i, pattern)| {
            // Find the best strategy for this pattern
            let mut best_idx = self.strategies.len() - 1; // Default strategy
            let mut best_confidence = 0.0;
            
            for (idx, strategy) in self.strategies.iter().enumerate() {
                if strategy.applicable(pattern) {
                    let confidence = strategy.confidence(pattern);
                    if confidence > best_confidence {
                        best_confidence = confidence;
                        best_idx = idx;
                    }
                }
            }
            best_idx
        }).collect();
        
        // Step 3: Execute strategies via orchestrator
        let start_time = std::time::Instant::now();
        let selected_strategies: Vec<&dyn strategies::SolvingStrategy> = 
            selected_strategy_indices.iter()
                .map(|&idx| self.strategies[idx].as_ref())
                .collect();
        
        let result = self.orchestrator.execute(
            &mut self.solver,
            &selected_strategies,
            &patterns,
            context,
        );
        
        // Step 4: Track performance if enabled
        if let Some(tracker) = &mut self.performance_tracker {
            if let Ok(_) = &result {
                for (pattern, &strategy_idx) in patterns.iter().zip(selected_strategy_indices.iter()) {
                    let strategy_name = self.strategies[strategy_idx].name();
                    let key = (pattern.name(), strategy_name.to_string());
                    
                    let attempts = tracker.attempts.entry(key.clone()).or_insert(0);
                    *attempts += 1;
                    
                    let success_count = (tracker.success_rates.get(&key).unwrap_or(&0.0) 
                        * (*attempts - 1) as f64 + 1.0) / *attempts as f64;
                    tracker.success_rates.insert(key.clone(), success_count);
                    
                    tracker.average_times.insert(key, start_time.elapsed());
                }
            }
        }
        
        result
    }
    
    /// Select strategies based on patterns and context
    fn select_strategies(
        &self,
        patterns: &[patterns::CircuitPattern],
        context: Option<&SynthesizerContext>,
    ) -> Vec<&dyn strategies::SolvingStrategy> {
        use patterns::CircuitPattern;
        
        patterns.iter().map(|pattern| {
            // Check if we have intent information to guide selection
            if let Some(ctx) = context {
                if let Some(strategy) = self.select_with_intent(pattern, ctx) {
                    return strategy;
                }
            }
            
            // Fall back to pattern-based selection
            self.select_by_pattern(pattern)
        }).collect()
    }
    
    /// Select strategy based on intent information
    fn select_with_intent(
        &self,
        pattern: &patterns::CircuitPattern,
        context: &SynthesizerContext,
    ) -> Option<&dyn strategies::SolvingStrategy> {
        use patterns::CircuitPattern;
        
        // Find relevant flow path with intent
        let relevant_flow = context.flow_paths.iter()
            .find(|flow| flow.intent.is_some() && 
                  pattern.involves_components(&flow.components));
        
        if let Some(flow) = relevant_flow {
            if let Some(intent) = &flow.intent {
                // Convert params to HashMap for strategy
                let mut params_map = HashMap::new();
                for param in &intent.params {
                    match param {
                        bhdl_common::IntentParam::Named(name, value) => {
                            let value_str = match value {
                                bhdl_common::IntentValue::String(s) => s.clone(),
                                bhdl_common::IntentValue::Number(n, unit) => {
                                    if let Some(u) = unit {
                                        format!("{}{}", n, u)
                                    } else {
                                        n.to_string()
                                    }
                                },
                                bhdl_common::IntentValue::Boolean(b) => b.to_string(),
                                bhdl_common::IntentValue::Identifier(id) => id.clone(),
                            };
                            params_map.insert(name.clone(), value_str);
                        },
                        bhdl_common::IntentParam::Positional(_) => {
                            // Skip positional params for now
                        }
                    }
                }
                
                // Match intent to strategy
                for strategy in &self.strategies {
                    if strategy.matches_intent(&intent.name, &params_map) {
                        return Some(strategy.as_ref());
                    }
                }
            }
        }
        
        None
    }
    
    /// Select strategy based on pattern alone
    fn select_by_pattern(&self, pattern: &patterns::CircuitPattern) -> &dyn strategies::SolvingStrategy {
        // Find strategy with highest confidence for this pattern
        let mut best_strategy = None;
        let mut best_confidence = 0.0;
        
        for strategy in &self.strategies {
            if strategy.applicable(pattern) {
                let confidence = strategy.confidence(pattern);
                if confidence > best_confidence {
                    best_confidence = confidence;
                    best_strategy = Some(strategy.as_ref());
                }
            }
        }
        
        best_strategy.unwrap_or_else(|| {
            // Default strategy as fallback
            self.strategies.last().unwrap().as_ref()
        })
    }
    
    /// Add a model to the underlying solver
    pub fn add_model(&mut self, name: String, model: crate::ComponentModel) {
        self.solver.add_model(name, model);
    }
    
    /// Get performance statistics
    pub fn performance_stats(&self) -> Option<&PerformanceTracker> {
        self.performance_tracker.as_ref()
    }
}