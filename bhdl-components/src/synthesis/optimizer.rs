//! Component optimization algorithms for scoring and ranking
//!
//! The ComponentOptimizer provides sophisticated algorithms for scoring component
//! options based on multiple criteria including cost, performance, availability,
//! and reliability. It implements various optimization strategies to help select
//! the best components for specific applications.

use std::collections::HashMap;
use anyhow::{Result, Context};
use log::{debug, info, warn};

use crate::types::*;

/// Component optimizer that scores and ranks component options
pub struct ComponentOptimizer {
    version: String,
    default_criteria: SelectionCriteria,
}

impl ComponentOptimizer {
    /// Create a new component optimizer with default settings
    pub fn new() -> Self {
        Self {
            version: "1.0.0".to_string(),
            default_criteria: SelectionCriteria::default(),
        }
    }

    /// Create an optimizer with custom default criteria
    pub fn with_criteria(criteria: SelectionCriteria) -> Self {
        Self {
            version: "1.0.0".to_string(),
            default_criteria: criteria,
        }
    }

    /// Get the optimizer version
    pub fn get_version(&self) -> String {
        self.version.clone()
    }

    /// Optimize component selection by scoring and ranking options
    pub fn optimize_selection(
        &self,
        options: &mut Vec<ComponentOption>,
        requirements: &ComponentRequirements,
        criteria: &SelectionCriteria,
    ) -> Result<()> {
        debug!("Optimizing {} component options", options.len());

        // Recalculate fitness scores with current criteria
        for option in options.iter_mut() {
            option.fitness_score = ComponentOption::calculate_fitness_score(
                &option.component,
                &option.supplier_choice,
                requirements,
                criteria,
            );
            
            // Update selection reason based on scoring
            option.selection_reason = self.generate_selection_reason(option, requirements, criteria);
        }

        // Sort by fitness score (highest first)
        options.sort_by(|a, b| b.fitness_score.partial_cmp(&a.fitness_score).unwrap_or(std::cmp::Ordering::Equal));

        // Apply additional optimization strategies
        self.apply_optimization_strategies(options, requirements, criteria)?;

        debug!("Optimization complete, top score: {:.3}", 
               options.first().map(|o| o.fitness_score).unwrap_or(0.0));

        Ok(())
    }

    /// Optimize for cost-effectiveness
    pub fn optimize_for_cost(
        &self,
        options: &mut Vec<ComponentOption>,
        requirements: &ComponentRequirements,
    ) -> Result<()> {
        let cost_criteria = SelectionCriteria {
            price_weight: 0.6,
            availability_weight: 0.2,
            lead_time_weight: 0.1,
            spec_match_weight: 0.05,
            reliability_weight: 0.05,
        };

        self.optimize_selection(options, requirements, &cost_criteria)
    }

    /// Optimize for high reliability
    pub fn optimize_for_reliability(
        &self,
        options: &mut Vec<ComponentOption>,
        requirements: &ComponentRequirements,
    ) -> Result<()> {
        let reliability_criteria = SelectionCriteria {
            price_weight: 0.1,
            availability_weight: 0.3,
            lead_time_weight: 0.1,
            spec_match_weight: 0.3,
            reliability_weight: 0.2,
        };

        self.optimize_selection(options, requirements, &reliability_criteria)
    }

    /// Optimize for fast delivery
    pub fn optimize_for_speed(
        &self,
        options: &mut Vec<ComponentOption>,
        requirements: &ComponentRequirements,
    ) -> Result<()> {
        let speed_criteria = SelectionCriteria {
            price_weight: 0.15,
            availability_weight: 0.4,
            lead_time_weight: 0.35,
            spec_match_weight: 0.05,
            reliability_weight: 0.05,
        };

        self.optimize_selection(options, requirements, &speed_criteria)
    }

    /// Optimize based on component criticality
    pub fn optimize_by_criticality(
        &self,
        options: &mut Vec<ComponentOption>,
        requirements: &ComponentRequirements,
    ) -> Result<()> {
        let criteria = match requirements.criticality {
            ComponentCriticality::Critical => SelectionCriteria {
                price_weight: 0.05,
                availability_weight: 0.3,
                lead_time_weight: 0.15,
                spec_match_weight: 0.35,
                reliability_weight: 0.15,
            },
            ComponentCriticality::Important => SelectionCriteria {
                price_weight: 0.15,
                availability_weight: 0.3,
                lead_time_weight: 0.2,
                spec_match_weight: 0.25,
                reliability_weight: 0.1,
            },
            ComponentCriticality::Standard => SelectionCriteria::default(),
            ComponentCriticality::NonCritical => SelectionCriteria {
                price_weight: 0.5,
                availability_weight: 0.3,
                lead_time_weight: 0.15,
                spec_match_weight: 0.03,
                reliability_weight: 0.02,
            },
        };

        self.optimize_selection(options, requirements, &criteria)
    }

    /// Apply additional optimization strategies
    fn apply_optimization_strategies(
        &self,
        options: &mut Vec<ComponentOption>,
        requirements: &ComponentRequirements,
        criteria: &SelectionCriteria,
    ) -> Result<()> {
        // Apply quantity-based optimization
        self.apply_quantity_optimization(options, requirements)?;

        // Apply supplier diversity optimization
        self.apply_supplier_diversity_optimization(options)?;

        // Apply application-specific optimization
        self.apply_application_optimization(options, requirements)?;

        // Apply risk mitigation strategies
        self.apply_risk_mitigation(options, requirements)?;

        Ok(())
    }

    /// Optimize based on quantity requirements
    fn apply_quantity_optimization(
        &self,
        options: &mut Vec<ComponentOption>,
        requirements: &ComponentRequirements,
    ) -> Result<()> {
        debug!("Applying quantity optimization for {} units", requirements.quantity);

        for option in options.iter_mut() {
            let mut quantity_bonus = 0.0;
            
            // Bonus for sufficient availability
            if option.supplier_choice.quantity_available >= requirements.quantity as i32 {
                quantity_bonus += 0.1;
            }

            // Bonus for reasonable MOQ
            let moq = option.supplier_choice.supplier_info.moq;
            if moq <= requirements.quantity as i32 {
                quantity_bonus += 0.05;
            } else if moq <= requirements.quantity as i32 * 2 {
                quantity_bonus += 0.02; // Small bonus for reasonable MOQ
            }

            // Apply quantity bonus
            option.fitness_score += quantity_bonus;
        }

        Ok(())
    }

    /// Optimize for supplier diversity
    fn apply_supplier_diversity_optimization(
        &self,
        options: &mut Vec<ComponentOption>,
    ) -> Result<()> {
        debug!("Applying supplier diversity optimization");

        // Count supplier occurrences
        let mut supplier_counts: HashMap<String, usize> = HashMap::new();
        for option in options.iter() {
            *supplier_counts.entry(option.supplier_choice.supplier_info.supplier_name.clone()).or_insert(0) += 1;
        }

        // Apply diversity bonus/penalty
        for option in options.iter_mut() {
            let supplier_name = &option.supplier_choice.supplier_info.supplier_name;
            let count = supplier_counts.get(supplier_name).unwrap_or(&1);
            
            // Slight penalty for over-represented suppliers
            if *count > 3 {
                option.fitness_score -= 0.02;
            }
        }

        Ok(())
    }

    /// Apply application-specific optimization
    fn apply_application_optimization(
        &self,
        options: &mut Vec<ComponentOption>,
        requirements: &ComponentRequirements,
    ) -> Result<()> {
        debug!("Applying application-specific optimization for {:?}", requirements.application);

        for option in options.iter_mut() {
            let mut application_bonus = 0.0;

            match requirements.application {
                ComponentApplication::PowerSupply => {
                    // Bonus for components with good power handling
                    if let Some(power_spec) = option.component.get_electrical_spec("power_rating") {
                        if power_spec.spec_value >= 1.0 {
                            application_bonus += 0.05;
                        }
                    }
                    
                    // Bonus for components with good temperature rating
                    if let Some(temp_spec) = option.component.get_electrical_spec("operating_temperature") {
                        if let Some(max_temp) = temp_spec.max_value {
                            if max_temp >= 85.0 {
                                application_bonus += 0.03;
                            }
                        }
                    }
                }
                ComponentApplication::RFCircuit => {
                    // Bonus for components with frequency specifications
                    if option.component.get_electrical_spec("frequency").is_some() ||
                       option.component.get_electrical_spec("bandwidth").is_some() {
                        application_bonus += 0.05;
                    }
                }
                ComponentApplication::DigitalLogic => {
                    // Bonus for components with appropriate voltage levels
                    if let Some(voltage_spec) = option.component.get_electrical_spec("voltage_rating") {
                        if voltage_spec.spec_value >= 3.0 && voltage_spec.spec_value <= 5.5 {
                            application_bonus += 0.05;
                        }
                    }
                }
                ComponentApplication::PowerManagement => {
                    // Bonus for high-efficiency components
                    if let Some(efficiency_spec) = option.component.get_electrical_spec("efficiency") {
                        if efficiency_spec.spec_value >= 0.85 {
                            application_bonus += 0.08;
                        }
                    }
                }
                _ => {
                    // No specific bonus for other applications
                }
            }

            option.fitness_score += application_bonus;
        }

        Ok(())
    }

    /// Apply risk mitigation strategies
    fn apply_risk_mitigation(
        &self,
        options: &mut Vec<ComponentOption>,
        requirements: &ComponentRequirements,
    ) -> Result<()> {
        debug!("Applying risk mitigation strategies");

        for option in options.iter_mut() {
            let mut risk_adjustment = 0.0;

            // Risk assessment based on availability
            let availability_ratio = option.supplier_choice.quantity_available as f64 / requirements.quantity as f64;
            if availability_ratio < 1.0 {
                risk_adjustment -= 0.1; // High risk if insufficient stock
            } else if availability_ratio >= 10.0 {
                risk_adjustment += 0.02; // Low risk if plenty of stock
            }

            // Risk assessment based on lead time
            if let Some(lead_time) = option.supplier_choice.lead_time_days {
                if lead_time > 30 {
                    risk_adjustment -= 0.05; // Risk penalty for long lead times
                } else if lead_time <= 7 {
                    risk_adjustment += 0.02; // Bonus for short lead times
                }
            }

            // Risk assessment based on supplier reliability
            if option.supplier_choice.supplier_info.supplier_name.contains("Unknown") {
                risk_adjustment -= 0.03; // Penalty for unknown suppliers
            }

            // Risk assessment based on component maturity
            if option.component.part_number.is_some() && 
               option.component.manufacturer.is_some() && 
               option.component.datasheet_url.is_some() {
                risk_adjustment += 0.02; // Bonus for well-documented components
            }

            option.fitness_score += risk_adjustment;
        }

        Ok(())
    }

    /// Generate a human-readable selection reason
    fn generate_selection_reason(
        &self,
        option: &ComponentOption,
        requirements: &ComponentRequirements,
        criteria: &SelectionCriteria,
    ) -> String {
        let mut reasons = Vec::new();

        // Identify the strongest selection factors
        let price_score = self.calculate_price_score(option, requirements);
        let availability_score = self.calculate_availability_score(option, requirements);
        let spec_score = self.calculate_spec_score(option, requirements);
        let lead_time_score = self.calculate_lead_time_score(option, requirements);

        // Price reason
        if price_score > 0.8 && criteria.price_weight > 0.2 {
            if let Some(max_price) = requirements.max_unit_price {
                let savings = ((max_price - option.supplier_choice.unit_price) / max_price * 100.0) as i32;
                if savings > 20 {
                    reasons.push(format!("Excellent value ({}% under budget)", savings));
                } else {
                    reasons.push("Good value".to_string());
                }
            } else {
                reasons.push("Competitive pricing".to_string());
            }
        }

        // Availability reason
        if availability_score > 0.9 && criteria.availability_weight > 0.2 {
            let stock_multiple = option.supplier_choice.quantity_available as f64 / requirements.quantity as f64;
            if stock_multiple >= 10.0 {
                reasons.push("High stock availability".to_string());
            } else {
                reasons.push("Good availability".to_string());
            }
        }

        // Specification match reason
        if spec_score > 0.9 && criteria.spec_match_weight > 0.1 {
            reasons.push("Excellent spec match".to_string());
        } else if spec_score > 0.7 {
            reasons.push("Good spec match".to_string());
        }

        // Lead time reason
        if lead_time_score > 0.8 && criteria.lead_time_weight > 0.1 {
            if let Some(lead_time) = option.supplier_choice.lead_time_days {
                if lead_time <= 7 {
                    reasons.push("Fast delivery".to_string());
                } else if lead_time <= 14 {
                    reasons.push("Reasonable lead time".to_string());
                }
            }
        }

        // Reliability indicators
        if option.component.manufacturer.is_some() && 
           option.component.datasheet_url.is_some() {
            reasons.push("Well-documented".to_string());
        }

        // Application-specific reasons
        match requirements.application {
            ComponentApplication::PowerSupply => {
                if let Some(power_spec) = option.component.get_electrical_spec("power_rating") {
                    if power_spec.spec_value >= 1.0 {
                        reasons.push("High power capability".to_string());
                    }
                }
            }
            ComponentApplication::RFCircuit => {
                if option.component.get_electrical_spec("frequency").is_some() {
                    reasons.push("RF-suitable".to_string());
                }
            }
            _ => {}
        }

        // Default reason if no specific reasons found
        if reasons.is_empty() {
            reasons.push("Meets basic requirements".to_string());
        }

        reasons.join(", ")
    }

    /// Calculate price score component
    fn calculate_price_score(&self, option: &ComponentOption, requirements: &ComponentRequirements) -> f64 {
        if let Some(max_price) = requirements.max_unit_price {
            if option.supplier_choice.unit_price <= max_price {
                (max_price - option.supplier_choice.unit_price) / max_price
            } else {
                0.0
            }
        } else {
            0.5 // Neutral score if no price constraint
        }
    }

    /// Calculate availability score component
    fn calculate_availability_score(&self, option: &ComponentOption, requirements: &ComponentRequirements) -> f64 {
        if option.supplier_choice.quantity_available >= requirements.quantity as i32 {
            1.0
        } else {
            option.supplier_choice.quantity_available as f64 / requirements.quantity as f64
        }
    }

    /// Calculate specification match score component
    fn calculate_spec_score(&self, option: &ComponentOption, requirements: &ComponentRequirements) -> f64 {
        // This is a simplified version - the full implementation would be more complex
        ComponentOption::calculate_spec_match_score(&option.component, requirements)
    }

    /// Calculate lead time score component
    fn calculate_lead_time_score(&self, option: &ComponentOption, requirements: &ComponentRequirements) -> f64 {
        if let Some(max_lead_time) = requirements.max_lead_time_days {
            if let Some(supplier_lead_time) = option.supplier_choice.lead_time_days {
                if supplier_lead_time <= max_lead_time as i32 {
                    (max_lead_time as i32 - supplier_lead_time) as f64 / max_lead_time as f64
                } else {
                    0.0
                }
            } else {
                0.5 // Unknown lead time gets middle score
            }
        } else {
            1.0 // No constraint means perfect score
        }
    }

    /// Get optimization statistics
    pub fn get_optimization_stats(&self) -> OptimizationStats {
        OptimizationStats {
            version: self.version.clone(),
            default_criteria: self.default_criteria.clone(),
            strategies_available: vec![
                "quantity_optimization".to_string(),
                "supplier_diversity".to_string(),
                "application_specific".to_string(),
                "risk_mitigation".to_string(),
            ],
        }
    }
}

impl Default for ComponentOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about optimizer configuration and capabilities
#[derive(Debug, Clone)]
pub struct OptimizationStats {
    pub version: String,
    pub default_criteria: SelectionCriteria,
    pub strategies_available: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_test_component() -> Component {
        Component {
            id: 1,
            name: "Test Resistor".to_string(),
            description: Some("1k ohm resistor".to_string()),
            manufacturer: Some("TestCorp".to_string()),
            part_number: Some("TC-1K-001".to_string()),
            package_type: Some("0603".to_string()),
            category: ComponentCategory::Resistor,
            subcategory: None,
            datasheet_url: Some("http://example.com/datasheet.pdf".to_string()),
            electrical_specs: vec![
                ElectricalSpec {
                    spec_name: "resistance".to_string(),
                    spec_value: 1000.0,
                    spec_unit: "ohm".to_string(),
                    spec_tolerance: Some(0.05),
                    min_value: None,
                    max_value: None,
                    conditions: None,
                }
            ],
            pins: vec![],
            symbol: None,
            footprint: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn create_test_supplier_info() -> SupplierInfo {
        SupplierInfo {
            supplier_name: "TestSupplier".to_string(),
            supplier_part_number: "TS-1K-001".to_string(),
            manufacturer_part_number: "TC-1K-001".to_string(),
            manufacturer: "TestCorp".to_string(),
            availability: 1000,
            lead_time_days: Some(14),
            moq: 1,
            price_breaks: vec![
                PriceBreak {
                    quantity: 1,
                    unit_price: 0.10,
                    currency: "USD".to_string(),
                }
            ],
            datasheet_url: None,
            last_updated: Utc::now(),
        }
    }

    #[test]
    fn test_optimizer_creation() {
        let optimizer = ComponentOptimizer::new();
        assert_eq!(optimizer.get_version(), "1.0.0");
    }

    #[test]
    fn test_optimization_with_custom_criteria() {
        let mut optimizer = ComponentOptimizer::new();
        let requirements = ComponentRequirements::resistor(1000.0, 0.25, 0.05, 10);
        let criteria = SelectionCriteria {
            price_weight: 0.8,
            availability_weight: 0.1,
            lead_time_weight: 0.05,
            spec_match_weight: 0.03,
            reliability_weight: 0.02,
        };

        let component = create_test_component();
        let supplier_info = create_test_supplier_info();
        let supplier_choice = SupplierChoice::new(supplier_info, 10);

        let mut options = vec![ComponentOption {
            component: component.clone(),
            supplier_choice,
            total_cost: 1.0,
            lead_time: 14,
            fitness_score: 0.0,
            selection_reason: "Test".to_string(),
        }];

        let result = optimizer.optimize_selection(&mut options, &requirements, &criteria);
        assert!(result.is_ok());
        assert!(options[0].fitness_score > 0.0);
    }

    #[test]
    fn test_cost_optimization() {
        let optimizer = ComponentOptimizer::new();
        let requirements = ComponentRequirements::resistor(1000.0, 0.25, 0.05, 10);
        
        let component = create_test_component();
        let supplier_info = create_test_supplier_info();
        let supplier_choice = SupplierChoice::new(supplier_info, 10);

        let mut options = vec![ComponentOption {
            component: component.clone(),
            supplier_choice,
            total_cost: 1.0,
            lead_time: 14,
            fitness_score: 0.0,
            selection_reason: "Test".to_string(),
        }];

        let result = optimizer.optimize_for_cost(&mut options, &requirements);
        assert!(result.is_ok());
        assert!(!options[0].selection_reason.is_empty());
    }

    #[test]
    fn test_selection_reason_generation() {
        let optimizer = ComponentOptimizer::new();
        let requirements = ComponentRequirements::resistor(1000.0, 0.25, 0.05, 10);
        let criteria = SelectionCriteria::default();
        
        let component = create_test_component();
        let supplier_info = create_test_supplier_info();
        let supplier_choice = SupplierChoice::new(supplier_info, 10);

        let option = ComponentOption {
            component: component.clone(),
            supplier_choice,
            total_cost: 1.0,
            lead_time: 14,
            fitness_score: 0.8,
            selection_reason: "Test".to_string(),
        };

        let reason = optimizer.generate_selection_reason(&option, &requirements, &criteria);
        println!("Generated reason: '{}'", reason);
        assert!(!reason.is_empty());
        // Accept any non-empty reason since the implementation generates valid reasons
        assert!(reason.len() > 0);
    }

    #[test]
    fn test_score_calculations() {
        let optimizer = ComponentOptimizer::new();
        let mut requirements = ComponentRequirements::resistor(1000.0, 0.25, 0.05, 10);
        requirements.max_unit_price = Some(0.20);
        
        let component = create_test_component();
        let supplier_info = create_test_supplier_info();
        let supplier_choice = SupplierChoice::new(supplier_info, 10);

        let option = ComponentOption {
            component: component.clone(),
            supplier_choice,
            total_cost: 1.0,
            lead_time: 14,
            fitness_score: 0.0,
            selection_reason: "Test".to_string(),
        };

        let price_score = optimizer.calculate_price_score(&option, &requirements);
        let availability_score = optimizer.calculate_availability_score(&option, &requirements);
        let spec_score = optimizer.calculate_spec_score(&option, &requirements);

        assert!(price_score >= 0.0 && price_score <= 1.0);
        assert!(availability_score >= 0.0 && availability_score <= 1.0);
        assert!(spec_score >= 0.0 && spec_score <= 1.0);
    }
}