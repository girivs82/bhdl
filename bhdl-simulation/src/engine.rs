// Core Simulation Engine
// Orchestrates component-embedded simulation and optimization

use std::collections::HashMap;
use std::time::{Duration, Instant};
use thiserror::Error;
// AST types will be used when integrated with synthesizer
// use bhdl_ast::{ComponentDef, ModuleDef, SourceFile, AstNode};
use bhdl_parser::{parse, SyntaxKind};
use nalgebra::{DMatrix, DVector};
use lru::LruCache;
use std::num::NonZeroUsize;

#[derive(Error, Debug)]
pub enum SimulationError {
    #[error("No behavioral models found for component {0}")]
    NoModelsFound(String),
    #[error("Model {0} not found")]
    ModelNotFound(String),
    #[error("Optimization failed to converge: {0}")]
    OptimizationFailed(String),
    #[error("Simulation error: {0}")]
    SimulationFailed(String),
    #[error("Parse error: {0}")]
    ParseError(String),
}

pub type Result<T> = std::result::Result<T, SimulationError>;

/// Different levels of simulation models
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulationLevel {
    Analytical = 0,      // Pure equations, milliseconds
    Behavioral = 1,      // Averaged models, seconds  
    SwitchingSimple = 2, // Simplified switching, minutes
    SwitchingFull = 3,   // Full SPICE, hours
}

/// Model metadata extracted from behavioral model annotations
#[derive(Debug, Clone)]
pub struct ModelMetadata {
    pub name: String,
    pub level: SimulationLevel,
    pub typical_runtime: Duration,
    pub accuracy: f64,
    pub properties: HashMap<String, String>,
}

/// Design parameters for optimization
#[derive(Debug, Clone)]
pub struct DesignParameters {
    pub values: HashMap<String, f64>,
}

impl DesignParameters {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }
    
    pub fn set(&mut self, name: &str, value: f64) {
        self.values.insert(name.to_string(), value);
    }
    
    pub fn get(&self, name: &str) -> Option<f64> {
        self.values.get(name).copied()
    }
}

/// Simulation results
#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub metrics: HashMap<String, f64>,
    pub success: bool,
    pub runtime: Duration,
}

/// Optimization result
#[derive(Debug, Clone)]
pub struct OptimizationResult {
    pub final_design: DesignParameters,
    pub best_score: f64,
    pub iterations: usize,
    pub total_runtime: Duration,
    pub convergence_reason: String,
}

/// Main simulation engine
#[derive(Clone)]
pub struct SimulationEngine {
    cache: LruCache<String, SimulationResult>,
    cache_hits: usize,
    cache_misses: usize,
}

impl SimulationEngine {
    pub fn new() -> Self {
        Self {
            cache: LruCache::new(NonZeroUsize::new(1000).unwrap()),
            cache_hits: 0,
            cache_misses: 0,
        }
    }
    
    /// Extract behavioral models from a parsed component
    pub fn extract_behavioral_models(&self, source: &str) -> Result<Vec<ModelMetadata>> {
        let parse_result = parse(source);
        let syntax = parse_result.syntax();
        
        let mut models = Vec::new();
        self.find_behavioral_models(&syntax, &mut models);
        
        if models.is_empty() {
            return Err(SimulationError::NoModelsFound("component".to_string()));
        }
        
        Ok(models)
    }
    
    /// Recursively find behavioral model nodes in syntax tree
    fn find_behavioral_models(&self, node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, models: &mut Vec<ModelMetadata>) {
        if node.kind() == SyntaxKind::BEHAVIORAL_MODEL {
            if let Some(model) = self.parse_behavioral_model_node(node) {
                models.push(model);
            }
        }
        
        for child in node.children() {
            self.find_behavioral_models(&child, models);
        }
    }
    
    /// Parse a behavioral model node into metadata
    fn parse_behavioral_model_node(&self, node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>) -> Option<ModelMetadata> {
        let mut model_name = String::new();
        let mut properties = HashMap::new();
        
        // Extract model name and properties
        for child in node.children() {
            match child.kind() {
                SyntaxKind::IDENT => {
                    if model_name.is_empty() {
                        model_name = child.text().to_string();
                    }
                }
                SyntaxKind::MODEL_PROPERTY => {
                    if let Some((key, value)) = self.parse_model_property(&child) {
                        properties.insert(key, value);
                    }
                }
                _ => {}
            }
        }
        
        if model_name.is_empty() {
            return None;
        }
        
        // Determine simulation level from model type
        let level = match properties.get("model_type").map(|s| s.as_str()) {
            Some("equations") | Some("analytical") => SimulationLevel::Analytical,
            Some("state_space") | Some("averaged") => SimulationLevel::Behavioral,
            Some("behavioral_switching") | Some("switching") => SimulationLevel::SwitchingSimple,
            Some("spice") | Some("full_spice") => SimulationLevel::SwitchingFull,
            _ => SimulationLevel::Analytical, // Default
        };
        
        // Parse runtime if available
        let runtime = properties
            .get("runtime")
            .and_then(|s| self.parse_duration(s))
            .unwrap_or_else(|| match level {
                SimulationLevel::Analytical => Duration::from_millis(1),
                SimulationLevel::Behavioral => Duration::from_millis(100),
                SimulationLevel::SwitchingSimple => Duration::from_secs(10),
                SimulationLevel::SwitchingFull => Duration::from_secs(600),
            });
        
        // Parse accuracy if available
        let accuracy = properties
            .get("accuracy")
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| match level {
                SimulationLevel::Analytical => 0.7,
                SimulationLevel::Behavioral => 0.9,
                SimulationLevel::SwitchingSimple => 0.95,
                SimulationLevel::SwitchingFull => 0.99,
            });
        
        Some(ModelMetadata {
            name: model_name,
            level,
            typical_runtime: runtime,
            accuracy,
            properties,
        })
    }
    
    /// Parse a model property (key: value)
    fn parse_model_property(&self, node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>) -> Option<(String, String)> {
        let mut key = String::new();
        let mut value = String::new();
        
        for child in node.children() {
            match child.kind() {
                SyntaxKind::IDENT => {
                    if key.is_empty() {
                        key = child.text().to_string();
                    }
                }
                SyntaxKind::VALUE => {
                    value = child.text().to_string().trim_matches('"').to_string();
                }
                _ => {
                    // For complex expressions, just take the text
                    let text = child.text().to_string();
                    let text = text.trim();
                    if !text.is_empty() && value.is_empty() {
                        value = text.to_string();
                    }
                }
            }
        }
        
        if key.is_empty() || value.is_empty() {
            return None;
        }
        
        Some((key, value))
    }
    
    /// Parse duration string (e.g., "1ms", "100ms", "10s")
    fn parse_duration(&self, s: &str) -> Option<Duration> {
        let s = s.trim();
        if let Some(ms) = s.strip_suffix("ms") {
            ms.parse::<u64>().ok().map(Duration::from_millis)
        } else if let Some(s_str) = s.strip_suffix('s') {
            s_str.parse::<u64>().ok().map(Duration::from_secs)
        } else {
            None
        }
    }
    
    /// Select the best model for a given simulation requirement
    pub fn select_model<'a>(
        &self,
        models: &'a [ModelMetadata],
        time_budget: Option<Duration>,
        accuracy_requirement: f64,
    ) -> Option<&'a ModelMetadata> {
        let mut candidates: Vec<&ModelMetadata> = models
            .iter()
            .filter(|m| m.accuracy >= accuracy_requirement)
            .collect();
        
        if candidates.is_empty() {
            // If no model meets accuracy requirement, pick the most accurate
            return models.iter().max_by(|a, b| a.accuracy.partial_cmp(&b.accuracy).unwrap());
        }
        
        // If time budget is specified, prefer faster models
        if let Some(budget) = time_budget {
            candidates.retain(|m| m.typical_runtime <= budget);
            if candidates.is_empty() {
                // If no model fits time budget, pick the fastest
                return models.iter().min_by_key(|m| m.typical_runtime);
            }
        }
        
        // Among valid candidates, prefer higher accuracy
        candidates
            .into_iter()
            .max_by(|a, b| a.accuracy.partial_cmp(&b.accuracy).unwrap())
    }
    
    /// Run simulation with a specific model and parameters
    pub fn simulate(
        &mut self,
        model: &ModelMetadata,
        parameters: &DesignParameters,
    ) -> Result<SimulationResult> {
        let start_time = Instant::now();
        
        // Check cache first
        let cache_key = format!("{}:{:?}", model.name, parameters.values);
        if let Some(cached) = self.cache.get(&cache_key) {
            self.cache_hits += 1;
            return Ok(cached.clone());
        }
        
        self.cache_misses += 1;
        
        // Run simulation based on model level
        let result = match model.level {
            SimulationLevel::Analytical => self.simulate_analytical(model, parameters)?,
            SimulationLevel::Behavioral => self.simulate_behavioral(model, parameters)?,
            SimulationLevel::SwitchingSimple => self.simulate_switching_simple(model, parameters)?,
            SimulationLevel::SwitchingFull => self.simulate_switching_full(model, parameters)?,
        };
        
        let runtime = start_time.elapsed();
        let sim_result = SimulationResult {
            metrics: result,
            success: true,
            runtime,
        };
        
        // Cache the result
        self.cache.put(cache_key, sim_result.clone());
        
        Ok(sim_result)
    }
    
    /// Simulate using analytical equations
    fn simulate_analytical(
        &self,
        model: &ModelMetadata,
        parameters: &DesignParameters,
    ) -> Result<HashMap<String, f64>> {
        let mut results = HashMap::new();
        
        // Example: Buck converter analytical model
        if let Some(vin) = parameters.get("vin_nom") {
            if let Some(vout) = parameters.get("vout") {
                if let Some(iout) = parameters.get("iout_max") {
                    if let Some(fsw) = parameters.get("f_sw") {
                        // L_min calculation from model property
                        if model.properties.contains_key("L_min") {
                            let l_min = (vin - vout) * vout / (vin * 0.3 * iout * fsw) * 1e-6; // Convert to µH
                            results.insert("L_min".to_string(), l_min);
                        }
                        
                        // C_min calculation
                        if model.properties.contains_key("C_min") {
                            let c_min = (0.3 * iout) / (8.0 * fsw * 0.05) * 1e6; // Convert to µF
                            results.insert("C_min".to_string(), c_min);
                        }
                        
                        // Efficiency estimate (simplified)
                        let efficiency = 0.85 + 0.1 * (vout / vin) - 0.05 * (iout / 3.0);
                        results.insert("efficiency".to_string(), efficiency.max(0.7).min(0.95));
                    }
                }
            }
        }
        
        Ok(results)
    }
    
    /// Simulate using behavioral/averaged models
    fn simulate_behavioral(
        &self,
        _model: &ModelMetadata,
        parameters: &DesignParameters,
    ) -> Result<HashMap<String, f64>> {
        let mut results = HashMap::new();
        
        // Example: State-space model for control loop analysis
        if let Some(l) = parameters.get("L") {
            if let Some(c) = parameters.get("C") {
                if let Some(r_load) = parameters.get("R_load") {
                    // Simplified transfer function analysis
                    let omega_0 = 1.0 / (l * c).sqrt();
                    let _q_factor = r_load * (c / l).sqrt();
                    
                    // Phase margin estimation (simplified)
                    let phase_margin = 60.0 - 10.0 * (omega_0 / 10000.0).log10();
                    results.insert("phase_margin".to_string(), phase_margin.max(0.0).min(90.0));
                    
                    // Crossover frequency
                    let crossover = omega_0 / (2.0 * std::f64::consts::PI);
                    results.insert("crossover_frequency".to_string(), crossover);
                    
                    // Stability check
                    let stable = if phase_margin > 45.0 { 1.0 } else { 0.0 };
                    results.insert("stable".to_string(), stable);
                }
            }
        }
        
        Ok(results)
    }
    
    /// Simulate using simplified switching models
    fn simulate_switching_simple(
        &self,
        _model: &ModelMetadata,
        parameters: &DesignParameters,
    ) -> Result<HashMap<String, f64>> {
        let mut results = HashMap::new();
        
        // Placeholder for switching simulation
        if let Some(l) = parameters.get("L") {
            if let Some(c) = parameters.get("C") {
                // Ripple calculations
                let current_ripple = 0.1 * l; // Simplified
                let voltage_ripple = current_ripple / (8.0 * 500e3 * c); // At 500kHz
                
                results.insert("current_ripple".to_string(), current_ripple);
                results.insert("voltage_ripple".to_string(), voltage_ripple);
                
                // Efficiency with switching losses
                let switching_losses = 0.05; // 5% switching losses
                let efficiency = 0.95 - switching_losses;
                results.insert("efficiency".to_string(), efficiency);
            }
        }
        
        Ok(results)
    }
    
    /// Simulate using full SPICE models (placeholder)
    fn simulate_switching_full(
        &self,
        _model: &ModelMetadata,
        _parameters: &DesignParameters,
    ) -> Result<HashMap<String, f64>> {
        // Placeholder - would integrate with actual SPICE engine
        let mut results = HashMap::new();
        results.insert("efficiency".to_string(), 0.92);
        results.insert("voltage_ripple".to_string(), 0.025);
        results.insert("thd".to_string(), 0.001);
        Ok(results)
    }
    
    /// Get cache statistics
    pub fn cache_stats(&self) -> (usize, usize, f64) {
        let total = self.cache_hits + self.cache_misses;
        let hit_rate = if total > 0 {
            self.cache_hits as f64 / total as f64
        } else {
            0.0
        };
        (self.cache_hits, self.cache_misses, hit_rate)
    }
}

impl Default for SimulationEngine {
    fn default() -> Self {
        Self::new()
    }
}