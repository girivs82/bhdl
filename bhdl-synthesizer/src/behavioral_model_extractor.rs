// Behavioral Model Extractor
// Extracts behavioral models and optimization requirements from component definitions

use anyhow::{Result, Context};
use bhdl_ast::{SourceFile, Entity, HasName};
use bhdl_analyzer::AnalysisResult;
use bhdl_simulation::{ModelMetadata, SimulationLevel};
use rowan::ast::AstNode;
use std::collections::HashMap;
use std::time::Duration;
use log::{info, debug, warn};

/// Extracts behavioral models and optimization requirements from components
pub struct BehavioralModelExtractor {
    models: Vec<ModelMetadata>,
    optimization_requirements: HashMap<String, OptimizationRequirements>,
}

/// Optimization requirements extracted from @optimization_strategy
#[derive(Debug, Clone)]
pub struct OptimizationRequirements {
    pub target_efficiency: Option<f64>,
    pub min_phase_margin: Option<f64>,
    pub max_crossover_freq: Option<f64>,
    pub max_output_ripple: Option<f64>,
    pub objectives: Vec<String>,
    pub constraints: Vec<String>,
    pub search_method: String,
}

impl BehavioralModelExtractor {
    pub fn new() -> Self {
        Self {
            models: Vec::new(),
            optimization_requirements: HashMap::new(),
        }
    }
    
    /// Extract behavioral models from AST and analysis
    pub fn extract_from_ast(&mut self, ast: &SourceFile, analysis: &AnalysisResult) -> Result<()> {
        // Walk through all entities in the AST
        for entity in ast.entities() {
            // Get entity name from the name token
            let entity_name = if let Some(name_token) = entity.name() {
                name_token.text().to_string()
            } else {
                continue; // Skip entities without names
            };

            debug!("Extracting behavioral models from entity: {}", entity_name);

            // Extract behavioral models
            self.extract_entity_behavioral_models(&entity, &entity_name)?;

            // Extract optimization requirements
            self.extract_optimization_requirements(&entity, &entity_name)?;
        }
        
        // Also check imported modules from the symbol table
        for (name, symbol) in analysis.global_scope.get_symbols() {
            if symbol.kind == bhdl_analyzer::symbol_table::SymbolKind::Entity {
                // Check if this entity has behavioral models in the library
                if let Some(models) = self.load_library_models(name) {
                    self.models.extend(models);
                }
            }
        }
        
        info!("Extracted {} behavioral models and {} optimization requirements",
              self.models.len(), self.optimization_requirements.len());
        
        Ok(())
    }
    
    /// Extract behavioral models from an entity
    fn extract_entity_behavioral_models(&mut self, entity: &Entity, module_name: &str) -> Result<()> {
        // Look for @behavioral_model annotations
        let module_text = entity.syntax().text().to_string();
        
        // Parse @behavioral_model blocks (simplified - real implementation would use proper AST)
        if module_text.contains("@behavioral_model") {
            // Extract analytical model
            if let Some(analytical) = self.parse_behavioral_model(&module_text, "analytical") {
                self.models.push(ModelMetadata {
                    name: format!("{}_analytical", module_name),
                    level: SimulationLevel::Analytical,
                    typical_runtime: analytical.runtime,
                    accuracy: analytical.accuracy,
                    properties: analytical.properties,
                });
            }
            
            // Extract state-space model
            if let Some(state_space) = self.parse_behavioral_model(&module_text, "state_space") {
                self.models.push(ModelMetadata {
                    name: format!("{}_state_space", module_name),
                    level: SimulationLevel::Behavioral,
                    typical_runtime: state_space.runtime,
                    accuracy: state_space.accuracy,
                    properties: state_space.properties,
                });
            }
            
            // Extract switching model
            if let Some(switching) = self.parse_behavioral_model(&module_text, "switching") {
                self.models.push(ModelMetadata {
                    name: format!("{}_switching", module_name),
                    level: SimulationLevel::SwitchingSimple,
                    typical_runtime: switching.runtime,
                    accuracy: switching.accuracy,
                    properties: switching.properties,
                });
            }
        }
        
        Ok(())
    }
    
    /// Extract optimization requirements from @optimization_strategy
    fn extract_optimization_requirements(&mut self, entity: &Entity, module_name: &str) -> Result<()> {
        let module_text = entity.syntax().text().to_string();
        
        if module_text.contains("@optimization_strategy") {
            let mut requirements = OptimizationRequirements {
                target_efficiency: None,
                min_phase_margin: None,
                max_crossover_freq: None,
                max_output_ripple: None,
                objectives: Vec::new(),
                constraints: Vec::new(),
                search_method: "grid_search".to_string(),
            };
            
            // Parse target_efficiency
            if let Some(efficiency) = self.extract_value(&module_text, "target_efficiency:") {
                requirements.target_efficiency = Some(efficiency);
                debug!("Module {} specifies target_efficiency: {}", module_name, efficiency);
            }
            
            // Parse min_phase_margin
            if let Some(phase_margin) = self.extract_value(&module_text, "min_phase_margin:") {
                requirements.min_phase_margin = Some(phase_margin);
                debug!("Module {} specifies min_phase_margin: {}", module_name, phase_margin);
            }
            
            // Parse objectives
            if let Some(objectives) = self.extract_list(&module_text, "objectives:") {
                requirements.objectives = objectives;
            }
            
            // Parse constraints
            if let Some(constraints) = self.extract_list(&module_text, "constraints:") {
                requirements.constraints = constraints;
            }
            
            // Parse search method
            if let Some(method) = self.extract_string(&module_text, "search_method:") {
                requirements.search_method = method;
            }
            
            self.optimization_requirements.insert(module_name.to_string(), requirements);
        }
        
        Ok(())
    }
    
    /// Load behavioral models from the component library
    fn load_library_models(&self, component_name: &str) -> Option<Vec<ModelMetadata>> {
        // Check if this is a known power converter type
        if component_name.contains("Buck") || component_name.contains("buck") {
            // Return pre-defined models for buck converter
            // In real implementation, this would load from the stdlib files
            Some(vec![
                ModelMetadata {
                    name: format!("{}_analytical", component_name),
                    level: SimulationLevel::Analytical,
                    typical_runtime: Duration::from_millis(1),
                    accuracy: 0.75,
                    properties: HashMap::from([
                        ("model_type".to_string(), "buck_analytical".to_string()),
                        ("from_library".to_string(), "true".to_string()),
                    ]),
                },
                ModelMetadata {
                    name: format!("{}_behavioral", component_name),
                    level: SimulationLevel::Behavioral,
                    typical_runtime: Duration::from_millis(100),
                    accuracy: 0.90,
                    properties: HashMap::from([
                        ("model_type".to_string(), "state_space".to_string()),
                        ("from_library".to_string(), "true".to_string()),
                    ]),
                },
            ])
        } else {
            None
        }
    }
    
    /// Parse a behavioral model block (simplified parser)
    fn parse_behavioral_model(&self, text: &str, model_name: &str) -> Option<BehavioralModel> {
        // This is a simplified parser - real implementation would use proper AST parsing
        if let Some(start) = text.find(&format!("@behavioral_model {}", model_name)) {
            let block = &text[start..];
            
            let mut model = BehavioralModel {
                runtime: Duration::from_millis(1),
                accuracy: 0.8,
                properties: HashMap::new(),
            };
            
            // Extract runtime
            if let Some(runtime_ms) = self.extract_duration(block, "runtime:") {
                model.runtime = runtime_ms;
            }
            
            // Extract accuracy
            if let Some(accuracy) = self.extract_value(block, "accuracy:") {
                model.accuracy = accuracy;
            }
            
            // Extract model_type
            if let Some(model_type) = self.extract_string(block, "model_type:") {
                model.properties.insert("model_type".to_string(), model_type);
            }
            
            Some(model)
        } else {
            None
        }
    }
    
    /// Extract a numeric value from text
    fn extract_value(&self, text: &str, key: &str) -> Option<f64> {
        if let Some(pos) = text.find(key) {
            let after_key = &text[pos + key.len()..];
            let value_str = after_key.split(&[',', '\n', '}'][..])
                .next()?
                .trim();
            value_str.parse().ok()
        } else {
            None
        }
    }
    
    /// Extract a string value from text
    fn extract_string(&self, text: &str, key: &str) -> Option<String> {
        if let Some(pos) = text.find(key) {
            let after_key = &text[pos + key.len()..];
            let value = after_key.split(&[',', '\n', '}'][..])
                .next()?
                .trim()
                .trim_matches('"');
            Some(value.to_string())
        } else {
            None
        }
    }
    
    /// Extract a list from text
    fn extract_list(&self, text: &str, key: &str) -> Option<Vec<String>> {
        if let Some(pos) = text.find(key) {
            let after_key = &text[pos + key.len()..];
            if let Some(bracket_start) = after_key.find('[') {
                if let Some(bracket_end) = after_key.find(']') {
                    let list_content = &after_key[bracket_start + 1..bracket_end];
                    let items: Vec<String> = list_content
                        .split(',')
                        .map(|s| s.trim().trim_matches('"').to_string())
                        .collect();
                    return Some(items);
                }
            }
        }
        None
    }
    
    /// Extract duration from text
    fn extract_duration(&self, text: &str, key: &str) -> Option<Duration> {
        if let Some(pos) = text.find(key) {
            let after_key = &text[pos + key.len()..];
            let duration_str = after_key.split(&[',', '\n', '}'][..])
                .next()?
                .trim();
            
            if duration_str.ends_with("ms") {
                let ms = duration_str.trim_end_matches("ms").parse::<u64>().ok()?;
                Some(Duration::from_millis(ms))
            } else if duration_str.ends_with('s') {
                let s = duration_str.trim_end_matches('s').parse::<u64>().ok()?;
                Some(Duration::from_secs(s))
            } else {
                None
            }
        } else {
            None
        }
    }
    
    /// Get extracted models
    pub fn get_models(&self) -> &[ModelMetadata] {
        &self.models
    }
    
    /// Get optimization requirements for a component
    pub fn get_requirements(&self, component_name: &str) -> Option<&OptimizationRequirements> {
        self.optimization_requirements.get(component_name)
    }
}

/// Internal representation of a behavioral model
struct BehavioralModel {
    runtime: Duration,
    accuracy: f64,
    properties: HashMap<String, String>,
}