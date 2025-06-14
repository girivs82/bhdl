//! Core synthesis engine for converting component requirements to real components
//!
//! The SynthesisEngine orchestrates the entire component synthesis process:
//! 1. Validates requirements and determines search strategy
//! 2. Finds candidate components using the matcher
//! 3. Retrieves supplier data for each candidate
//! 4. Scores and ranks components using the optimizer
//! 5. Returns ranked ComponentOption results

use std::collections::HashMap;
use anyhow::{Result, Context};
use log::{debug, info, warn, error};

use crate::types::*;
use crate::database::ComponentDatabase;
use crate::supplier::SupplierService;
use super::matcher::ComponentMatcher;
use super::optimizer::ComponentOptimizer;

/// Core synthesis engine that orchestrates component selection
pub struct SynthesisEngine {
    matcher: ComponentMatcher,
    optimizer: ComponentOptimizer,
    max_candidates: usize,
    max_alternatives: usize,
}

impl SynthesisEngine {
    /// Create a new synthesis engine with default settings
    pub fn new() -> Self {
        Self {
            matcher: ComponentMatcher::new(),
            optimizer: ComponentOptimizer::new(),
            max_candidates: 50,    // Maximum candidates to consider
            max_alternatives: 10,  // Maximum alternatives to return
        }
    }

    /// Create a synthesis engine with custom settings
    pub fn with_limits(max_candidates: usize, max_alternatives: usize) -> Self {
        Self {
            matcher: ComponentMatcher::new(),
            optimizer: ComponentOptimizer::new(),
            max_candidates,
            max_alternatives,
        }
    }

    /// Set the maximum number of candidates to consider
    pub fn set_max_candidates(&mut self, max_candidates: usize) {
        self.max_candidates = max_candidates;
    }

    /// Set the maximum number of alternatives to return
    pub fn set_max_alternatives(&mut self, max_alternatives: usize) {
        self.max_alternatives = max_alternatives;
    }

    /// Main synthesis method - converts requirements to component selections
    pub async fn synthesize_component(
        &self,
        component_type: &str,
        requirements: &ComponentRequirements,
        database: &ComponentDatabase,
    ) -> Result<SynthesisResult> {
        info!("Starting component synthesis for {} with requirements: {:?}", 
              component_type, requirements);

        // Validate requirements
        if let Err(e) = self.validate_requirements(requirements) {
            error!("Invalid requirements: {}", e);
            let mut result = SynthesisResult::new();
            result.add_note(format!("Invalid requirements: {}", e));
            return Ok(result);
        }

        // Find candidate components
        let candidates = self.find_candidates(component_type, requirements, database).await
            .context("Failed to find candidate components")?;

        if candidates.is_empty() {
            warn!("No candidates found for {} with given requirements", component_type);
            let mut result = SynthesisResult::new();
            result.add_note("No matching components found in database.".to_string());
            return Ok(result);
        }

        info!("Found {} candidates for {}", candidates.len(), component_type);

        // Score and rank candidates
        let ranked_options = self.score_and_rank_candidates(
            &candidates, requirements, &SelectionCriteria::default()
        ).await?;

        // Build synthesis result
        let mut result = SynthesisResult::new();
        
        // Add alternatives (limited by max_alternatives)
        for option in ranked_options.into_iter().take(self.max_alternatives) {
            result.add_alternative(option);
        }

        // Set the recommended option (best scoring alternative)
        result.set_recommended();

        // Add synthesis notes
        result.add_note(format!("Evaluated {} candidates", candidates.len()));
        result.add_note(format!("Component type: {}", component_type));
        
        if let Some(recommended) = &result.recommended {
            result.add_note(format!("Recommended: {} (score: {:.2})", 
                                  recommended.component.name, recommended.fitness_score));
        }

        info!("Synthesis complete for {}: {} alternatives, confidence: {:.2}", 
              component_type, result.alternatives.len(), result.confidence);

        Ok(result)
    }

    /// Synthesize component with custom selection criteria
    pub async fn synthesize_with_criteria(
        &self,
        component_type: &str,
        requirements: &ComponentRequirements,
        criteria: &SelectionCriteria,
        database: &ComponentDatabase,
    ) -> Result<SynthesisResult> {
        info!("Starting component synthesis with custom criteria for {}", component_type);

        // Validate requirements
        if let Err(e) = self.validate_requirements(requirements) {
            error!("Invalid requirements: {}", e);
            let mut result = SynthesisResult::new();
            result.add_note(format!("Invalid requirements: {}", e));
            return Ok(result);
        }

        // Find candidate components
        let candidates = self.find_candidates(component_type, requirements, database).await
            .context("Failed to find candidate components")?;

        if candidates.is_empty() {
            warn!("No candidates found for {} with given requirements", component_type);
            let mut result = SynthesisResult::new();
            result.add_note("No matching components found in database.".to_string());
            return Ok(result);
        }

        // Score and rank candidates with custom criteria
        let ranked_options = self.score_and_rank_candidates(
            &candidates, requirements, criteria
        ).await?;

        // Build synthesis result
        let mut result = SynthesisResult::new();
        
        for option in ranked_options.into_iter().take(self.max_alternatives) {
            result.add_alternative(option);
        }

        result.set_recommended();
        result.add_note(format!("Evaluated {} candidates with custom criteria", candidates.len()));

        Ok(result)
    }

    /// Synthesize component with supplier integration
    pub async fn synthesize_with_supplier_data(
        &self,
        component_type: &str,
        requirements: &ComponentRequirements,
        database: &ComponentDatabase,
        supplier_service: &SupplierService,
    ) -> Result<SynthesisResult> {
        info!("Starting component synthesis with supplier integration for {}", component_type);

        // Validate requirements
        if let Err(e) = self.validate_requirements(requirements) {
            error!("Invalid requirements: {}", e);
            let mut result = SynthesisResult::new();
            result.add_note(format!("Invalid requirements: {}", e));
            return Ok(result);
        }

        // Find candidate components
        let candidates = self.find_candidates(component_type, requirements, database).await
            .context("Failed to find candidate components")?;

        if candidates.is_empty() {
            warn!("No candidates found for {} with given requirements", component_type);
            let mut result = SynthesisResult::new();
            result.add_note("No matching components found in database.".to_string());
            return Ok(result);
        }

        // Get supplier data for candidates
        let candidates_with_supplier_data = self.enrich_with_supplier_data(
            candidates, supplier_service, requirements
        ).await?;

        // Score and rank candidates
        let ranked_options = self.score_and_rank_candidates(
            &candidates_with_supplier_data, requirements, &SelectionCriteria::default()
        ).await?;

        // Build synthesis result
        let mut result = SynthesisResult::new();
        
        for option in ranked_options.into_iter().take(self.max_alternatives) {
            result.add_alternative(option);
        }

        result.set_recommended();
        result.add_note(format!("Evaluated {} candidates with supplier data", candidates_with_supplier_data.len()));

        Ok(result)
    }

    /// Validate component requirements
    fn validate_requirements(&self, requirements: &ComponentRequirements) -> Result<()> {
        // Check for contradictory requirements
        if let Some(max_price) = requirements.max_unit_price {
            if max_price <= 0.0 {
                return Err(anyhow::anyhow!("Maximum unit price must be positive"));
            }
        }

        if let Some(max_lead_time) = requirements.max_lead_time_days {
            if max_lead_time == 0 {
                return Err(anyhow::anyhow!("Maximum lead time must be positive"));
            }
        }

        if requirements.quantity == 0 {
            return Err(anyhow::anyhow!("Quantity must be at least 1"));
        }

        // Check for reasonable electrical specifications
        if let Some(resistance) = requirements.resistance {
            if resistance <= 0.0 {
                return Err(anyhow::anyhow!("Resistance must be positive"));
            }
        }

        if let Some(capacitance) = requirements.capacitance {
            if capacitance <= 0.0 {
                return Err(anyhow::anyhow!("Capacitance must be positive"));
            }
        }

        if let Some(voltage) = requirements.voltage_rating {
            if voltage <= 0.0 {
                return Err(anyhow::anyhow!("Voltage rating must be positive"));
            }
        }

        if let Some(power) = requirements.power_rating {
            if power <= 0.0 {
                return Err(anyhow::anyhow!("Power rating must be positive"));
            }
        }

        Ok(())
    }

    /// Find candidate components that match requirements
    async fn find_candidates(
        &self,
        component_type: &str,
        requirements: &ComponentRequirements,
        database: &ComponentDatabase,
    ) -> Result<Vec<Component>> {
        debug!("Finding candidates for component type: {}", component_type);

        // Use the matcher to find candidates
        let candidates = self.matcher.find_matching_components(
            component_type, requirements, database, self.max_candidates
        ).await?;

        debug!("Found {} candidates", candidates.len());
        Ok(candidates)
    }

    /// Score and rank candidates using the optimizer
    async fn score_and_rank_candidates(
        &self,
        candidates: &[Component],
        requirements: &ComponentRequirements,
        criteria: &SelectionCriteria,
    ) -> Result<Vec<ComponentOption>> {
        debug!("Scoring and ranking {} candidates", candidates.len());

        // Create dummy supplier choices for components without supplier data
        let mut options = Vec::new();
        for component in candidates {
            let dummy_supplier = self.create_dummy_supplier_choice(component, requirements);
            let fitness_score = ComponentOption::calculate_fitness_score(
                component, &dummy_supplier, requirements, criteria
            );
            
            options.push(ComponentOption {
                component: component.clone(),
                supplier_choice: dummy_supplier,
                total_cost: 0.0, // Will be calculated by supplier choice
                lead_time: 0,
                fitness_score,
                selection_reason: "Based on specification match".to_string(),
            });
        }

        // Use optimizer to rank options
        self.optimizer.optimize_selection(&mut options, requirements, criteria)?;

        debug!("Ranking complete, top score: {:.2}", 
               options.first().map(|o| o.fitness_score).unwrap_or(0.0));

        Ok(options)
    }

    /// Enrich candidate components with supplier data
    async fn enrich_with_supplier_data(
        &self,
        candidates: Vec<Component>,
        supplier_service: &SupplierService,
        requirements: &ComponentRequirements,
    ) -> Result<Vec<Component>> {
        debug!("Enriching {} candidates with supplier data", candidates.len());

        let mut enriched_candidates = Vec::new();

        for component in candidates {
            // Try to get supplier data
            match supplier_service.get_supplier_data(component.id).await {
                Ok(Some(_supplier_data)) => {
                    // Component has supplier data, include it
                    enriched_candidates.push(component);
                }
                Ok(None) => {
                    // No supplier data, but still include if we're not strict about it
                    debug!("No supplier data for component {}, including anyway", component.id);
                    enriched_candidates.push(component);
                }
                Err(e) => {
                    warn!("Error getting supplier data for component {}: {}", component.id, e);
                    // Include anyway to not lose candidates
                    enriched_candidates.push(component);
                }
            }
        }

        debug!("Enriched {} candidates with supplier data", enriched_candidates.len());
        Ok(enriched_candidates)
    }

    /// Create a dummy supplier choice for components without supplier data
    fn create_dummy_supplier_choice(
        &self,
        component: &Component,
        requirements: &ComponentRequirements,
    ) -> SupplierChoice {
        let dummy_supplier_info = SupplierInfo {
            supplier_name: "Unknown".to_string(),
            supplier_part_number: component.part_number.clone().unwrap_or_default(),
            manufacturer_part_number: component.part_number.clone().unwrap_or_default(),
            manufacturer: component.manufacturer.clone().unwrap_or("Unknown".to_string()),
            availability: requirements.quantity as i32,
            lead_time_days: Some(14), // Default 2 weeks
            moq: 1,
            price_breaks: vec![PriceBreak {
                quantity: 1,
                unit_price: 1.0, // Default price
                currency: "USD".to_string(),
            }],
            datasheet_url: component.datasheet_url.clone(),
            last_updated: chrono::Utc::now(),
        };

        SupplierChoice::new(dummy_supplier_info, requirements.quantity as i32)
    }

    /// Get synthesis statistics
    pub fn get_synthesis_stats(&self) -> SynthesisStats {
        SynthesisStats {
            max_candidates: self.max_candidates,
            max_alternatives: self.max_alternatives,
            matcher_version: self.matcher.get_version(),
            optimizer_version: self.optimizer.get_version(),
        }
    }
}

impl Default for SynthesisEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the synthesis engine configuration
#[derive(Debug, Clone)]
pub struct SynthesisStats {
    pub max_candidates: usize,
    pub max_alternatives: usize,
    pub matcher_version: String,
    pub optimizer_version: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_synthesis_engine_creation() {
        let engine = SynthesisEngine::new();
        assert_eq!(engine.max_candidates, 50);
        assert_eq!(engine.max_alternatives, 10);
    }

    #[tokio::test]
    async fn test_synthesis_engine_with_limits() {
        let engine = SynthesisEngine::with_limits(20, 5);
        assert_eq!(engine.max_candidates, 20);
        assert_eq!(engine.max_alternatives, 5);
    }

    #[tokio::test]
    async fn test_requirements_validation() {
        let engine = SynthesisEngine::new();
        
        // Valid requirements
        let valid_req = ComponentRequirements::resistor(1000.0, 0.25, 0.05, 1);
        assert!(engine.validate_requirements(&valid_req).is_ok());
        
        // Invalid requirements - zero quantity
        let mut invalid_req = valid_req.clone();
        invalid_req.quantity = 0;
        assert!(engine.validate_requirements(&invalid_req).is_err());
        
        // Invalid requirements - negative price
        let mut invalid_req = valid_req.clone();
        invalid_req.max_unit_price = Some(-1.0);
        assert!(engine.validate_requirements(&invalid_req).is_err());
    }

    #[tokio::test]
    async fn test_dummy_supplier_choice() {
        let engine = SynthesisEngine::new();
        let requirements = ComponentRequirements::resistor(1000.0, 0.25, 0.05, 10);
        
        let component = Component {
            id: 1,
            name: "Test Resistor".to_string(),
            description: Some("1k ohm resistor".to_string()),
            manufacturer: Some("TestCorp".to_string()),
            part_number: Some("TC-1K-001".to_string()),
            package_type: Some("0603".to_string()),
            category: ComponentCategory::Resistor,
            subcategory: None,
            datasheet_url: None,
            electrical_specs: vec![],
            pins: vec![],
            symbol: None,
            footprint: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        
        let supplier_choice = engine.create_dummy_supplier_choice(&component, &requirements);
        assert_eq!(supplier_choice.quantity_available, 10);
        assert_eq!(supplier_choice.supplier_info.manufacturer, "TestCorp");
    }
}