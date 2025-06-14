//! Two-stage synthesis engine optimized for API limitations and data volatility

use anyhow::Result;
use log::{info, warn, debug};
use std::collections::HashMap;

use crate::types::*;
use crate::database::ComponentDatabase;
use crate::supplier::{SupplierService, multi_backend::MultiBackendSupplierService};
use super::matcher::ComponentMatcher;
use super::optimizer::ComponentOptimizer;

/// Configuration for two-stage synthesis
#[derive(Debug, Clone)]
pub struct TwoStageConfig {
    /// Maximum candidates to evaluate in stage 1 (spec-based)
    pub max_stage1_candidates: usize,
    /// Maximum candidates to query suppliers for in stage 2  
    pub max_stage2_candidates: usize,
    /// Enable/disable stage 2 supplier lookup
    pub enable_supplier_lookup: bool,
    /// Cache duration for supplier data (hours)
    pub supplier_cache_hours: i64,
    /// Minimum spec match score to proceed to stage 2
    pub min_spec_score_threshold: f64,
}

impl Default for TwoStageConfig {
    fn default() -> Self {
        Self {
            max_stage1_candidates: 100,    // Cast wide net initially
            max_stage2_candidates: 20,     // Practical API limit
            enable_supplier_lookup: false, // Default to spec-only mode
            supplier_cache_hours: 4,       // Short-term caching
            min_spec_score_threshold: 0.6, // Only check supply for good matches
        }
    }
}

/// Two-stage synthesis engine
pub struct TwoStageSynthesizer {
    matcher: ComponentMatcher,
    optimizer: ComponentOptimizer,
    config: TwoStageConfig,
}

impl TwoStageSynthesizer {
    pub fn new(config: TwoStageConfig) -> Self {
        Self {
            matcher: ComponentMatcher::new(),
            optimizer: ComponentOptimizer::new(),
            config,
        }
    }

    /// Execute two-stage synthesis
    pub async fn synthesize(
        &self,
        component_type: &str,
        requirements: &ComponentRequirements,
        database: &ComponentDatabase,
        supplier_service: Option<&SupplierService>,
    ) -> Result<SynthesisResult> {
        info!("Starting two-stage synthesis for {}", component_type);
        
        // Stage 1: Spec-based selection from local database
        let stage1_candidates = self.stage1_spec_selection(requirements, database).await?;
        
        if stage1_candidates.is_empty() {
            warn!("No candidates found in stage 1 spec selection");
            return Ok(SynthesisResult {
                recommended: None,
                alternatives: Vec::new(),
                synthesis_notes: vec!["No components found matching specifications".to_string()],
                confidence: 0.0,
            });
        }

        info!("Stage 1 found {} candidates", stage1_candidates.len());

        // Stage 2: Live supplier lookup for shortlisted candidates (optional)
        let final_options = if self.config.enable_supplier_lookup && supplier_service.is_some() {
            self.stage2_supplier_lookup(requirements, &stage1_candidates, supplier_service.unwrap()).await?
        } else {
            // Convert stage1 candidates to component options without supplier data
            self.create_spec_only_options(requirements, &stage1_candidates)?
        };

        // Create final synthesis result
        self.create_synthesis_result(final_options, stage1_candidates.len())
    }

    /// Stage 1: Fast spec-based selection from local database
    async fn stage1_spec_selection(
        &self,
        requirements: &ComponentRequirements,
        database: &ComponentDatabase,
    ) -> Result<Vec<ComponentCandidate>> {
        debug!("Stage 1: Spec-based selection");
        
        // Find components matching electrical/physical specs
        let components = self.matcher.find_component_candidates(requirements, database).await?;
        
        // Score based on specifications only (no supplier data)
        let mut candidates: Vec<ComponentCandidate> = components
            .into_iter()
            .map(|comp| {
                let spec_score = self.calculate_spec_score(&comp, requirements);
                let reputation_score = self.calculate_reputation_score(&comp);
                ComponentCandidate {
                    component: comp,
                    spec_score,
                    reputation_score,
                    combined_score: spec_score * 0.8 + reputation_score * 0.2,
                }
            })
            .collect();

        // Sort by combined score and limit candidates
        candidates.sort_by(|a, b| b.combined_score.partial_cmp(&a.combined_score).unwrap_or(std::cmp::Ordering::Equal));
        candidates.truncate(self.config.max_stage1_candidates);

        info!("Stage 1 selected {} candidates (max: {})", 
              candidates.len(), self.config.max_stage1_candidates);

        Ok(candidates)
    }

    /// Stage 2: Live supplier lookup for shortlisted candidates
    async fn stage2_supplier_lookup(
        &self,
        requirements: &ComponentRequirements,
        candidates: &[ComponentCandidate],
        supplier_service: &SupplierService,
    ) -> Result<Vec<ComponentOption>> {
        debug!("Stage 2: Supplier lookup");
        
        // Filter candidates that meet minimum spec threshold
        let qualified_candidates: Vec<&ComponentCandidate> = candidates
            .iter()
            .filter(|c| c.spec_score >= self.config.min_spec_score_threshold)
            .take(self.config.max_stage2_candidates)
            .collect();

        info!("Stage 2 querying {} candidates (threshold: {:.2})", 
              qualified_candidates.len(), self.config.min_spec_score_threshold);

        let mut options = Vec::new();
        let mut api_calls = 0;

        for candidate in qualified_candidates {
            // Check cache first
            if let Some(cached_data) = self.get_cached_supplier_data(&candidate.component, supplier_service).await? {
                if let Some(option) = self.create_option_from_supplier_data(&candidate.component, &cached_data, requirements)? {
                    options.push(option);
                }
                continue;
            }

            // Rate limiting: respect API constraints
            if api_calls >= self.config.max_stage2_candidates {
                warn!("Reached API call limit, using remaining candidates without supplier data");
                break;
            }

            // Live API lookup
            match self.lookup_live_supplier_data(&candidate.component, supplier_service).await {
                Ok(Some(supplier_data)) => {
                    if let Some(option) = self.create_option_from_supplier_data(&candidate.component, &supplier_data, requirements)? {
                        options.push(option);
                    }
                    api_calls += 1;
                }
                Ok(None) => {
                    // No supplier data available, create spec-only option
                    options.push(self.create_spec_only_option(&candidate.component, requirements)?);
                }
                Err(e) => {
                    warn!("Failed to fetch supplier data for {}: {}", candidate.component.name, e);
                    // Fallback to spec-only option
                    options.push(self.create_spec_only_option(&candidate.component, requirements)?);
                }
            }
        }

        info!("Stage 2 completed with {} API calls", api_calls);

        // Sort final options by fitness score
        options.sort_by(|a, b| b.fitness_score.partial_cmp(&a.fitness_score).unwrap_or(std::cmp::Ordering::Equal));

        Ok(options)
    }

    /// Create component options without supplier data (spec-only mode)
    fn create_spec_only_options(
        &self,
        requirements: &ComponentRequirements,
        candidates: &[ComponentCandidate],
    ) -> Result<Vec<ComponentOption>> {
        let mut options = Vec::new();
        
        for candidate in candidates.iter().take(self.config.max_stage2_candidates) {
            options.push(self.create_spec_only_option(&candidate.component, requirements)?);
        }

        Ok(options)
    }

    /// Create a component option without supplier data
    fn create_spec_only_option(
        &self,
        component: &Component,
        requirements: &ComponentRequirements,
    ) -> Result<ComponentOption> {
        // Create a placeholder supplier choice
        let placeholder_supplier = SupplierInfo {
            supplier_name: "Unknown".to_string(),
            supplier_part_number: component.part_number.clone().unwrap_or_else(|| "N/A".to_string()),
            manufacturer_part_number: component.part_number.clone().unwrap_or_else(|| "N/A".to_string()),
            manufacturer: component.manufacturer.clone().unwrap_or_else(|| "Unknown".to_string()),
            availability: 0,
            lead_time_days: None,
            moq: 1,
            price_breaks: vec![],
            datasheet_url: component.datasheet_url.clone(),
            last_updated: chrono::Utc::now(),
        };

        let supplier_choice = SupplierChoice::new(placeholder_supplier, requirements.quantity as i32);
        
        let fitness_score = self.calculate_spec_score(component, requirements);
        
        Ok(ComponentOption {
            component: component.clone(),
            supplier_choice,
            total_cost: 0.0, // Unknown without supplier data
            lead_time: 0,
            fitness_score,
            selection_reason: format!("Spec match: {:.1}% (no supplier data)", fitness_score * 100.0),
        })
    }

    /// Calculate specification match score (0.0 to 1.0)
    fn calculate_spec_score(&self, component: &Component, requirements: &ComponentRequirements) -> f64 {
        // Use the existing spec matching logic from ComponentOption
        ComponentOption::calculate_spec_match_score(component, requirements)
    }

    /// Calculate manufacturer/component reputation score
    fn calculate_reputation_score(&self, component: &Component) -> f64 {
        let mut score: f64 = 0.5; // Base score

        // Manufacturer reputation
        if let Some(manufacturer) = &component.manufacturer {
            match manufacturer.as_str() {
                "TI" | "Texas Instruments" => score += 0.3,
                "Analog Devices" | "ADI" => score += 0.3,
                "Linear Technology" | "Maxim" => score += 0.25,
                "Vishay" | "Yageo" | "Murata" => score += 0.2,
                _ => score += 0.1,
            }
        }

        // Documentation completeness
        if component.datasheet_url.is_some() { score += 0.1; }
        if !component.electrical_specs.is_empty() { score += 0.1; }

        score.min(1.0)
    }

    /// Check cache for supplier data
    async fn get_cached_supplier_data(
        &self,
        component: &Component,
        supplier_service: &SupplierService,
    ) -> Result<Option<SupplierData>> {
        // Check if we have recent data (within cache window)
        if supplier_service.is_data_fresh(component.id, self.config.supplier_cache_hours).await? {
            supplier_service.get_supplier_data(component.id).await
        } else {
            Ok(None)
        }
    }

    /// Perform live supplier data lookup
    async fn lookup_live_supplier_data(
        &self,
        component: &Component,
        supplier_service: &SupplierService,
    ) -> Result<Option<SupplierData>> {
        let part_number = component.part_number.as_deref()
            .or_else(|| Some(&component.name))
            .unwrap_or("unknown");

        supplier_service.update_component_supplier_data(component.id, part_number).await?;
        supplier_service.get_supplier_data(component.id).await
    }

    /// Create component option from supplier data
    fn create_option_from_supplier_data(
        &self,
        component: &Component,
        supplier_data: &SupplierData,
        requirements: &ComponentRequirements,
    ) -> Result<Option<ComponentOption>> {
        let supplier_choices = supplier_data.get_best_suppliers(requirements.quantity as i32, 1);
        
        if let Some(supplier_choice) = supplier_choices.first() {
            let criteria = SelectionCriteria::from_requirements(requirements);
            let fitness_score = ComponentOption::calculate_fitness_score(
                component,
                supplier_choice,
                requirements,
                &criteria,
            );

            let total_cost = supplier_choice.total_price;
            let lead_time = supplier_choice.lead_time_days.unwrap_or(0) as u32;
            let selection_reason = self.generate_selection_reason(component, supplier_choice, fitness_score);

            Ok(Some(ComponentOption {
                component: component.clone(),
                supplier_choice: supplier_choice.clone(),
                total_cost,
                lead_time,
                fitness_score,
                selection_reason,
            }))
        } else {
            Ok(None)
        }
    }

    /// Generate human-readable selection reason
    fn generate_selection_reason(&self, component: &Component, supplier_choice: &SupplierChoice, score: f64) -> String {
        let mut reasons = Vec::new();

        if score > 0.8 {
            reasons.push("excellent match".to_string());
        } else if score > 0.6 {
            reasons.push("good match".to_string());
        } else {
            reasons.push("acceptable match".to_string());
        }

        if supplier_choice.quantity_available > 1000 {
            reasons.push("high availability".to_string());
        }

        if let Some(lead_time) = supplier_choice.lead_time_days {
            if lead_time <= 7 {
                reasons.push("short lead time".to_string());
            }
        }

        if supplier_choice.unit_price < 1.0 {
            reasons.push("low cost".to_string());
        }

        reasons.join(", ")
    }

    /// Create final synthesis result
    fn create_synthesis_result(
        &self,
        mut options: Vec<ComponentOption>,
        total_candidates: usize,
    ) -> Result<SynthesisResult> {
        let mut result = SynthesisResult::new();
        
        // Add all options as alternatives
        for option in options {
            result.add_alternative(option);
        }
        
        // Set recommended option
        result.set_recommended();
        
        // Add synthesis notes
        result.add_note(format!("Evaluated {} candidates from database", total_candidates));
        
        if self.config.enable_supplier_lookup {
            result.add_note(format!("Queried live supplier data for top {} candidates", 
                                  self.config.max_stage2_candidates));
        } else {
            result.add_note("Spec-only mode: no supplier data queried".to_string());
        }
        
        if let Some(recommended) = &result.recommended {
            result.add_note(format!("Recommended: {} (score: {:.2})", 
                recommended.component.name, recommended.fitness_score));
        }

        Ok(result)
    }
}

/// Intermediate candidate from stage 1
#[derive(Debug, Clone)]
struct ComponentCandidate {
    component: Component,
    spec_score: f64,
    reputation_score: f64,
    combined_score: f64,
}