// ML-Based Component Selection Optimization
// Uses machine learning techniques to select optimal components based on:
// - Historical design data
// - Performance metrics
// - Cost optimization
// - Reliability statistics

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use anyhow::{Result, Context};
use log::{info, debug, warn};

/// ML model for component selection
pub struct MLComponentSelector {
    /// Trained model weights for different component categories
    model_weights: HashMap<ComponentCategory, ModelWeights>,
    
    /// Historical design database
    design_history: DesignHistoryDatabase,
    
    /// Component performance database
    performance_db: ComponentPerformanceDB,
    
    /// Learning parameters
    learning_config: LearningConfig,
    
    /// Feature extractors for different component types
    feature_extractors: HashMap<String, Box<dyn FeatureExtractor>>,
}

/// Component categories for ML models
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum ComponentCategory {
    Resistor,
    Capacitor,
    Inductor,
    Diode,
    Transistor,
    IC,
    Connector,
    Crystal,
    PowerSupply,
}

/// Trained model weights
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelWeights {
    /// Feature importance weights
    feature_weights: Vec<f64>,
    
    /// Bias terms
    bias: Vec<f64>,
    
    /// Decision boundaries
    decision_boundaries: Vec<DecisionBoundary>,
    
    /// Model accuracy metrics
    accuracy: f64,
    precision: f64,
    recall: f64,
}

/// Decision boundary for classification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionBoundary {
    feature_index: usize,
    threshold: f64,
    weight: f64,
}

/// Historical design database
pub struct DesignHistoryDatabase {
    /// Past designs indexed by design ID
    designs: HashMap<String, HistoricalDesign>,
    
    /// Component usage statistics
    usage_stats: HashMap<String, UsageStatistics>,
    
    /// Success metrics for designs
    success_metrics: HashMap<String, DesignSuccessMetrics>,
}

/// Historical design record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalDesign {
    pub design_id: String,
    pub date: String,
    pub components: Vec<ComponentSelection>,
    pub performance_metrics: PerformanceMetrics,
    pub cost: f64,
    pub reliability_score: f64,
    pub field_failure_rate: Option<f64>,
}

/// Component selection in a design
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentSelection {
    pub component_type: String,
    pub part_number: String,
    pub manufacturer: String,
    pub parameters: HashMap<String, f64>,
    pub cost: f64,
    pub availability: String,
    pub lead_time_days: u32,
}

/// Component usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStatistics {
    pub total_uses: u32,
    pub success_rate: f64,
    pub average_lifetime_hours: f64,
    pub failure_modes: Vec<String>,
    pub preferred_applications: Vec<String>,
}

/// Design success metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignSuccessMetrics {
    pub performance_score: f64,
    pub reliability_score: f64,
    pub cost_effectiveness: f64,
    pub manufacturing_yield: f64,
    pub field_return_rate: f64,
}

/// Performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub efficiency: f64,
    pub thermal_performance: f64,
    pub noise_level: f64,
    pub response_time: f64,
    pub bandwidth: f64,
}

/// Component performance database
pub struct ComponentPerformanceDB {
    /// Performance data by part number
    performance_data: HashMap<String, ComponentPerformance>,
    
    /// Benchmark results
    benchmarks: HashMap<String, BenchmarkResult>,
    
    /// Reliability data
    reliability_data: HashMap<String, ReliabilityData>,
}

/// Component performance data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentPerformance {
    pub part_number: String,
    pub typical_values: HashMap<String, f64>,
    pub min_values: HashMap<String, f64>,
    pub max_values: HashMap<String, f64>,
    pub temperature_coefficients: HashMap<String, f64>,
    pub aging_characteristics: HashMap<String, f64>,
}

/// Benchmark results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub test_conditions: HashMap<String, String>,
    pub measured_values: HashMap<String, f64>,
    pub pass_fail: bool,
    pub notes: String,
}

/// Reliability data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReliabilityData {
    pub mtbf_hours: f64,
    pub fit_rate: f64,
    pub activation_energy: f64,
    pub failure_modes: Vec<FailureMode>,
}

/// Failure mode information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureMode {
    pub mode: String,
    pub probability: f64,
    pub severity: u8,
    pub detection_difficulty: u8,
    pub rpn: u32, // Risk Priority Number
}

/// Learning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningConfig {
    pub learning_rate: f64,
    pub regularization: f64,
    pub batch_size: usize,
    pub epochs: u32,
    pub validation_split: f64,
    pub early_stopping_patience: u32,
}

/// Feature extractor trait
pub trait FeatureExtractor: Send + Sync {
    /// Extract features from component requirements
    fn extract_features(&self, requirements: &ComponentRequirements) -> Vec<f64>;
    
    /// Get feature names for interpretability
    fn get_feature_names(&self) -> Vec<String>;
}

/// Component requirements for selection
#[derive(Debug, Clone)]
pub struct ComponentRequirements {
    pub component_type: ComponentCategory,
    pub electrical_specs: HashMap<String, f64>,
    pub environmental_conditions: EnvironmentalConditions,
    pub cost_target: Option<f64>,
    pub size_constraints: Option<SizeConstraints>,
    pub reliability_requirements: Option<ReliabilityRequirements>,
}

/// Environmental conditions
#[derive(Debug, Clone)]
pub struct EnvironmentalConditions {
    pub temperature_range: (f64, f64),
    pub humidity_range: (f64, f64),
    pub vibration_level: String,
    pub altitude_max: f64,
    pub chemical_exposure: Vec<String>,
}

/// Size constraints
#[derive(Debug, Clone)]
pub struct SizeConstraints {
    pub max_height_mm: f64,
    pub max_area_mm2: f64,
    pub package_preference: Vec<String>,
}

/// Reliability requirements
#[derive(Debug, Clone)]
pub struct ReliabilityRequirements {
    pub min_mtbf_hours: f64,
    pub max_failure_rate: f64,
    pub required_lifetime_hours: f64,
    pub safety_critical: bool,
}

/// ML prediction result
#[derive(Debug, Clone)]
pub struct MLPrediction {
    pub recommended_components: Vec<ComponentRecommendation>,
    pub confidence_scores: Vec<f64>,
    pub feature_importance: HashMap<String, f64>,
    pub predicted_performance: PerformanceMetrics,
    pub risk_assessment: RiskAssessment,
}

/// Component recommendation
#[derive(Debug, Clone)]
pub struct ComponentRecommendation {
    pub part_number: String,
    pub manufacturer: String,
    pub score: f64,
    pub reasons: Vec<String>,
    pub alternatives: Vec<String>,
    pub trade_offs: Vec<TradeOff>,
}

/// Trade-off analysis
#[derive(Debug, Clone)]
pub struct TradeOff {
    pub parameter: String,
    pub this_value: f64,
    pub alternative_value: f64,
    pub impact: String,
}

/// Risk assessment
#[derive(Debug, Clone)]
pub struct RiskAssessment {
    pub overall_risk: RiskLevel,
    pub risk_factors: Vec<RiskFactor>,
    pub mitigation_strategies: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone)]
pub struct RiskFactor {
    pub factor: String,
    pub severity: f64,
    pub probability: f64,
    pub mitigation: String,
}

impl MLComponentSelector {
    /// Create new ML component selector
    pub fn new() -> Self {
        Self {
            model_weights: Self::initialize_models(),
            design_history: DesignHistoryDatabase::new(),
            performance_db: ComponentPerformanceDB::new(),
            learning_config: LearningConfig::default(),
            feature_extractors: Self::initialize_extractors(),
        }
    }
    
    /// Initialize pre-trained models
    fn initialize_models() -> HashMap<ComponentCategory, ModelWeights> {
        let mut models = HashMap::new();
        
        // Initialize with pre-trained weights for each category
        // In production, these would be loaded from trained model files
        for category in [
            ComponentCategory::Resistor,
            ComponentCategory::Capacitor,
            ComponentCategory::Inductor,
            ComponentCategory::Diode,
            ComponentCategory::Transistor,
            ComponentCategory::IC,
            ComponentCategory::Connector,
            ComponentCategory::Crystal,
            ComponentCategory::PowerSupply,
        ] {
            models.insert(category, ModelWeights::default());
        }
        
        models
    }
    
    /// Initialize feature extractors
    fn initialize_extractors() -> HashMap<String, Box<dyn FeatureExtractor>> {
        HashMap::new()
        // In production, would initialize specific extractors for each component type
    }
    
    /// Select optimal component using ML
    pub fn select_component(
        &self,
        requirements: &ComponentRequirements,
        context: &DesignContext,
    ) -> Result<MLPrediction> {
        info!("ML component selection for {:?}", requirements.component_type);
        
        // Extract features from requirements
        let features = self.extract_features(requirements, context)?;
        
        // Get model for component category
        let model = self.model_weights.get(&requirements.component_type)
            .ok_or_else(|| anyhow::anyhow!("No model for component type"))?;
        
        // Run inference
        let predictions = self.run_inference(&features, model)?;
        
        // Extract values before moving predictions
        let confidence_scores = predictions.confidence_scores.clone();
        let predicted_performance = predictions.performance.clone();
        
        // Rank components based on predictions
        let recommendations = self.rank_components(predictions, requirements)?;
        
        // Assess risks
        let risk_assessment = self.assess_risks(&recommendations, requirements)?;
        
        // Calculate feature importance
        let feature_importance = self.calculate_feature_importance(&features, model)?;
        
        Ok(MLPrediction {
            recommended_components: recommendations,
            confidence_scores,
            feature_importance,
            predicted_performance,
            risk_assessment,
        })
    }
    
    /// Extract features from requirements
    fn extract_features(
        &self,
        requirements: &ComponentRequirements,
        context: &DesignContext,
    ) -> Result<Features> {
        let mut features = Features::new();
        
        // Electrical features
        for (param, value) in &requirements.electrical_specs {
            features.add_numerical(param.clone(), *value);
        }
        
        // Environmental features
        features.add_numerical("temp_min".to_string(), requirements.environmental_conditions.temperature_range.0);
        features.add_numerical("temp_max".to_string(), requirements.environmental_conditions.temperature_range.1);
        
        // Context features
        features.add_categorical("application".to_string(), context.application_type.clone());
        features.add_numerical("production_volume".to_string(), context.production_volume as f64);
        
        // Historical features
        if let Some(history) = self.get_similar_designs(requirements) {
            features.add_numerical("historical_success_rate".to_string(), history.success_rate);
            features.add_numerical("historical_mtbf".to_string(), history.average_mtbf);
        }
        
        Ok(features)
    }
    
    /// Run ML inference
    fn run_inference(
        &self,
        features: &Features,
        model: &ModelWeights,
    ) -> Result<InferenceResult> {
        // Simple neural network forward pass
        let input_vector = features.to_vector();
        let mut activations = input_vector.clone();
        
        // Apply weights and bias
        for i in 0..activations.len() {
            if i < model.feature_weights.len() {
                activations[i] *= model.feature_weights[i];
            }
            if i < model.bias.len() {
                activations[i] += model.bias[i];
            }
        }
        
        // Apply activation function (ReLU)
        for a in &mut activations {
            *a = a.max(0.0);
        }
        
        // Apply decision boundaries
        let mut scores = Vec::new();
        for boundary in &model.decision_boundaries {
            if boundary.feature_index < activations.len() {
                let score = if activations[boundary.feature_index] > boundary.threshold {
                    boundary.weight
                } else {
                    0.0
                };
                scores.push(score);
            }
        }
        
        // Calculate confidence
        let total_score: f64 = scores.iter().sum();
        let confidence = (total_score / scores.len() as f64).min(1.0).max(0.0);
        
        Ok(InferenceResult {
            scores,
            confidence_scores: vec![confidence],
            performance: PerformanceMetrics {
                efficiency: 0.85 + confidence * 0.1,
                thermal_performance: 0.80 + confidence * 0.15,
                noise_level: 20.0 - confidence * 5.0,
                response_time: 1.0 - confidence * 0.5,
                bandwidth: 100.0 + confidence * 50.0,
            },
        })
    }
    
    /// Rank components based on ML predictions
    fn rank_components(
        &self,
        predictions: InferenceResult,
        requirements: &ComponentRequirements,
    ) -> Result<Vec<ComponentRecommendation>> {
        let mut recommendations = Vec::new();
        
        // Query performance database for matching components
        let candidates = self.performance_db.query_components(requirements)?;
        
        // Score each candidate
        for candidate in candidates {
            let score = self.score_component(&candidate, &predictions, requirements)?;
            
            recommendations.push(ComponentRecommendation {
                part_number: candidate.part_number.clone(),
                manufacturer: candidate.manufacturer.clone(),
                score,
                reasons: self.generate_reasons(&candidate, requirements)?,
                alternatives: self.find_alternatives(&candidate)?,
                trade_offs: self.analyze_trade_offs(&candidate, requirements)?,
            });
        }
        
        // Sort by score
        recommendations.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        
        // Keep top 5
        recommendations.truncate(5);
        
        Ok(recommendations)
    }
    
    /// Score a component
    fn score_component(
        &self,
        candidate: &ComponentCandidate,
        predictions: &InferenceResult,
        requirements: &ComponentRequirements,
    ) -> Result<f64> {
        let mut score = 0.0;
        
        // Electrical specification match
        for (param, required_value) in &requirements.electrical_specs {
            if let Some(actual_value) = candidate.specifications.get(param) {
                let match_score = 1.0 - (actual_value - required_value).abs() / required_value.abs();
                score += match_score.max(0.0) * 0.3;
            }
        }
        
        // Historical performance
        if let Some(history) = self.design_history.get_component_history(&candidate.part_number) {
            score += history.success_rate * 0.2;
        }
        
        // Cost consideration
        if let Some(cost_target) = requirements.cost_target {
            let cost_score = 1.0 - (candidate.cost - cost_target).abs() / cost_target;
            score += cost_score.max(0.0) * 0.2;
        }
        
        // ML prediction weight
        score += predictions.confidence_scores[0] * 0.3;
        
        Ok(score.min(1.0))
    }
    
    /// Generate reasons for recommendation
    fn generate_reasons(
        &self,
        candidate: &ComponentCandidate,
        requirements: &ComponentRequirements,
    ) -> Result<Vec<String>> {
        let mut reasons = Vec::new();
        
        // Check specification matches
        for (param, required_value) in &requirements.electrical_specs {
            if let Some(actual_value) = candidate.specifications.get(param) {
                if (actual_value - required_value).abs() / required_value.abs() < 0.1 {
                    reasons.push(format!("Excellent {} match", param));
                }
            }
        }
        
        // Check historical success
        if let Some(history) = self.design_history.get_component_history(&candidate.part_number) {
            if history.success_rate > 0.9 {
                reasons.push(format!("High historical success rate: {:.0}%", history.success_rate * 100.0));
            }
        }
        
        // Check availability
        if candidate.availability == "In Stock" {
            reasons.push("Readily available".to_string());
        }
        
        Ok(reasons)
    }
    
    /// Find alternative components
    fn find_alternatives(&self, component: &ComponentCandidate) -> Result<Vec<String>> {
        // In production, would query database for similar components
        Ok(vec![
            format!("{}_ALT1", component.part_number),
            format!("{}_ALT2", component.part_number),
        ])
    }
    
    /// Analyze trade-offs
    fn analyze_trade_offs(
        &self,
        candidate: &ComponentCandidate,
        requirements: &ComponentRequirements,
    ) -> Result<Vec<TradeOff>> {
        let mut trade_offs = Vec::new();
        
        // Cost vs performance trade-off
        if let Some(cost_target) = requirements.cost_target {
            if candidate.cost > cost_target {
                trade_offs.push(TradeOff {
                    parameter: "Cost".to_string(),
                    this_value: candidate.cost,
                    alternative_value: cost_target,
                    impact: "Higher cost but better performance".to_string(),
                });
            }
        }
        
        Ok(trade_offs)
    }
    
    /// Assess risks
    fn assess_risks(
        &self,
        recommendations: &[ComponentRecommendation],
        requirements: &ComponentRequirements,
    ) -> Result<RiskAssessment> {
        let mut risk_factors = Vec::new();
        
        // Check for single source risks
        let unique_manufacturers: std::collections::HashSet<_> = 
            recommendations.iter().map(|r| &r.manufacturer).collect();
        
        if unique_manufacturers.len() == 1 {
            risk_factors.push(RiskFactor {
                factor: "Single source supplier".to_string(),
                severity: 0.7,
                probability: 0.3,
                mitigation: "Qualify alternative suppliers".to_string(),
            });
        }
        
        // Check for obsolescence risk
        for rec in recommendations {
            if let Some(lifecycle) = self.performance_db.get_lifecycle_status(&rec.part_number) {
                if lifecycle == "LastTimeBuy" || lifecycle == "Obsolete" {
                    risk_factors.push(RiskFactor {
                        factor: format!("Component {} nearing end-of-life", rec.part_number),
                        severity: 0.8,
                        probability: 0.9,
                        mitigation: "Design in alternative component".to_string(),
                    });
                }
            }
        }
        
        // Determine overall risk level
        let max_risk = risk_factors.iter()
            .map(|r| r.severity * r.probability)
            .fold(0.0, f64::max);
        
        let overall_risk = if max_risk > 0.7 {
            RiskLevel::High
        } else if max_risk > 0.4 {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        };
        
        Ok(RiskAssessment {
            overall_risk,
            risk_factors,
            mitigation_strategies: vec![
                "Maintain safety stock".to_string(),
                "Qualify multiple suppliers".to_string(),
                "Regular design reviews".to_string(),
            ],
        })
    }
    
    /// Calculate feature importance
    fn calculate_feature_importance(
        &self,
        features: &Features,
        model: &ModelWeights,
    ) -> Result<HashMap<String, f64>> {
        let mut importance = HashMap::new();
        
        // Simple importance based on weight magnitude
        for (i, feature_name) in features.get_names().iter().enumerate() {
            if i < model.feature_weights.len() {
                importance.insert(
                    feature_name.clone(),
                    model.feature_weights[i].abs(),
                );
            }
        }
        
        // Normalize
        let total: f64 = importance.values().sum();
        if total > 0.0 {
            for value in importance.values_mut() {
                *value /= total;
            }
        }
        
        Ok(importance)
    }
    
    /// Get similar historical designs
    fn get_similar_designs(&self, requirements: &ComponentRequirements) -> Option<HistoricalStats> {
        // In production, would query design history database
        Some(HistoricalStats {
            success_rate: 0.85,
            average_mtbf: 50000.0,
        })
    }
    
    /// Train model on new data
    pub fn train(&mut self, training_data: &TrainingData) -> Result<TrainingMetrics> {
        info!("Training ML model with {} samples", training_data.samples.len());
        
        // Split data into training and validation
        let split_index = (training_data.samples.len() as f64 * 
                          (1.0 - self.learning_config.validation_split)) as usize;
        
        let (train_samples, val_samples) = training_data.samples.split_at(split_index);
        
        // Training loop
        let mut best_loss = f64::MAX;
        let mut patience_counter = 0;
        
        for epoch in 0..self.learning_config.epochs {
            // Mini-batch gradient descent
            let mut epoch_loss = 0.0;
            
            for batch in train_samples.chunks(self.learning_config.batch_size) {
                let batch_loss = self.train_batch(batch)?;
                epoch_loss += batch_loss;
            }
            
            epoch_loss /= train_samples.len() as f64;
            
            // Validation
            let val_loss = self.validate(val_samples)?;
            
            debug!("Epoch {}: train_loss={:.4}, val_loss={:.4}", epoch, epoch_loss, val_loss);
            
            // Early stopping
            if val_loss < best_loss {
                best_loss = val_loss;
                patience_counter = 0;
            } else {
                patience_counter += 1;
                if patience_counter >= self.learning_config.early_stopping_patience {
                    info!("Early stopping at epoch {}", epoch);
                    break;
                }
            }
        }
        
        Ok(TrainingMetrics {
            final_loss: best_loss,
            accuracy: self.calculate_accuracy(val_samples)?,
            epochs_trained: self.learning_config.epochs,
        })
    }
    
    /// Train on a batch
    fn train_batch(&mut self, batch: &[TrainingSample]) -> Result<f64> {
        // Simplified training - in production would use proper backpropagation
        let mut total_loss = 0.0;
        
        for sample in batch {
            let features = Features::from_vector(sample.features.clone());
            let prediction = self.run_inference(&features, 
                &self.model_weights[&sample.component_category])?;
            
            // Calculate loss
            let loss = (prediction.confidence_scores[0] - sample.label).powi(2);
            total_loss += loss;
            
            // Update weights (simplified gradient descent)
            if let Some(model) = self.model_weights.get_mut(&sample.component_category) {
                for i in 0..model.feature_weights.len() {
                    if i < sample.features.len() {
                        let gradient = 2.0 * (prediction.confidence_scores[0] - sample.label) 
                                      * sample.features[i];
                        model.feature_weights[i] -= self.learning_config.learning_rate * gradient;
                        
                        // L2 regularization
                        model.feature_weights[i] *= 1.0 - self.learning_config.regularization;
                    }
                }
            }
        }
        
        Ok(total_loss / batch.len() as f64)
    }
    
    /// Validate on validation set
    fn validate(&self, samples: &[TrainingSample]) -> Result<f64> {
        let mut total_loss = 0.0;
        
        for sample in samples {
            let features = Features::from_vector(sample.features.clone());
            let prediction = self.run_inference(&features, 
                &self.model_weights[&sample.component_category])?;
            let loss = (prediction.confidence_scores[0] - sample.label).powi(2);
            total_loss += loss;
        }
        
        Ok(total_loss / samples.len() as f64)
    }
    
    /// Calculate accuracy
    fn calculate_accuracy(&self, samples: &[TrainingSample]) -> Result<f64> {
        let mut correct = 0;
        
        for sample in samples {
            let features = Features::from_vector(sample.features.clone());
            let prediction = self.run_inference(&features, 
                &self.model_weights[&sample.component_category])?;
            
            // Binary classification threshold
            let predicted_class = if prediction.confidence_scores[0] > 0.5 { 1.0 } else { 0.0 };
            let actual_class = if sample.label > 0.5 { 1.0 } else { 0.0 };
            
            if predicted_class == actual_class {
                correct += 1;
            }
        }
        
        Ok(correct as f64 / samples.len() as f64)
    }
}

// Helper structures

#[derive(Debug, Clone)]
pub struct Features {
    numerical: HashMap<String, f64>,
    categorical: HashMap<String, String>,
}

impl Features {
    fn new() -> Self {
        Self {
            numerical: HashMap::new(),
            categorical: HashMap::new(),
        }
    }
    
    fn add_numerical(&mut self, name: String, value: f64) {
        self.numerical.insert(name, value);
    }
    
    fn add_categorical(&mut self, name: String, value: String) {
        self.categorical.insert(name, value);
    }
    
    fn to_vector(&self) -> Vec<f64> {
        let mut vec = Vec::new();
        
        // Add numerical features
        for (_, value) in &self.numerical {
            vec.push(*value);
        }
        
        // One-hot encode categorical features
        for (_, value) in &self.categorical {
            // Simple hash-based encoding for demo
            let hash = value.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32)) as f64;
            vec.push(hash / 1000.0); // Normalize
        }
        
        vec
    }
    
    fn from_vector(vec: Vec<f64>) -> Self {
        let mut features = Self::new();
        for (i, val) in vec.iter().enumerate() {
            features.add_numerical(format!("feature_{}", i), *val);
        }
        features
    }
    
    fn get_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        names.extend(self.numerical.keys().cloned());
        names.extend(self.categorical.keys().cloned());
        names
    }
}

#[derive(Debug, Clone)]
pub struct DesignContext {
    pub application_type: String,
    pub production_volume: u32,
    pub target_cost: f64,
    pub regulatory_requirements: Vec<String>,
}

#[derive(Debug, Clone)]
struct InferenceResult {
    scores: Vec<f64>,
    confidence_scores: Vec<f64>,
    performance: PerformanceMetrics,
}

#[derive(Debug, Clone)]
struct ComponentCandidate {
    part_number: String,
    manufacturer: String,
    specifications: HashMap<String, f64>,
    cost: f64,
    availability: String,
}

#[derive(Debug, Clone)]
struct HistoricalStats {
    success_rate: f64,
    average_mtbf: f64,
}

#[derive(Debug, Clone)]
pub struct TrainingData {
    samples: Vec<TrainingSample>,
}

#[derive(Debug, Clone)]
struct TrainingSample {
    features: Vec<f64>,
    label: f64,
    component_category: ComponentCategory,
}

#[derive(Debug, Clone)]
pub struct TrainingMetrics {
    final_loss: f64,
    accuracy: f64,
    epochs_trained: u32,
}

// Implementations for helper structs

impl DesignHistoryDatabase {
    fn new() -> Self {
        Self {
            designs: HashMap::new(),
            usage_stats: HashMap::new(),
            success_metrics: HashMap::new(),
        }
    }
    
    fn get_component_history(&self, part_number: &str) -> Option<&UsageStatistics> {
        self.usage_stats.get(part_number)
    }
}

impl ComponentPerformanceDB {
    fn new() -> Self {
        Self {
            performance_data: HashMap::new(),
            benchmarks: HashMap::new(),
            reliability_data: HashMap::new(),
        }
    }
    
    fn query_components(&self, requirements: &ComponentRequirements) -> Result<Vec<ComponentCandidate>> {
        // In production, would query actual database
        Ok(vec![
            ComponentCandidate {
                part_number: "COMP-001".to_string(),
                manufacturer: "TechCorp".to_string(),
                specifications: requirements.electrical_specs.clone(),
                cost: 1.50,
                availability: "In Stock".to_string(),
            },
            ComponentCandidate {
                part_number: "COMP-002".to_string(),
                manufacturer: "ElectroCo".to_string(),
                specifications: requirements.electrical_specs.clone(),
                cost: 1.25,
                availability: "In Stock".to_string(),
            },
        ])
    }
    
    fn get_lifecycle_status(&self, part_number: &str) -> Option<String> {
        // In production, would query actual database
        Some("Active".to_string())
    }
}

impl Default for ModelWeights {
    fn default() -> Self {
        Self {
            feature_weights: vec![0.1; 10],
            bias: vec![0.0; 10],
            decision_boundaries: vec![
                DecisionBoundary {
                    feature_index: 0,
                    threshold: 0.5,
                    weight: 1.0,
                },
            ],
            accuracy: 0.85,
            precision: 0.80,
            recall: 0.82,
        }
    }
}

impl Default for LearningConfig {
    fn default() -> Self {
        Self {
            learning_rate: 0.001,
            regularization: 0.0001,
            batch_size: 32,
            epochs: 100,
            validation_split: 0.2,
            early_stopping_patience: 10,
        }
    }
}