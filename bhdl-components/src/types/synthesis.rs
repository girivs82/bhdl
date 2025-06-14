//! Component synthesis and selection data structures

use serde::{Deserialize, Serialize};
use super::{Component, SupplierChoice};

/// Requirements for component synthesis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentRequirements {
    // Electrical requirements
    pub resistance: Option<f64>,
    pub capacitance: Option<f64>,
    pub inductance: Option<f64>,
    pub voltage_rating: Option<f64>,
    pub current_rating: Option<f64>,
    pub power_rating: Option<f64>,
    pub tolerance: Option<f64>,
    pub frequency: Option<f64>,
    
    // Physical requirements
    pub package_type: Option<String>,
    pub temperature_range: Option<(f64, f64)>, // (min, max) in Celsius
    
    // Supply chain requirements
    pub quantity: u32,
    pub max_unit_price: Option<f64>,
    pub max_lead_time_days: Option<u32>,
    pub preferred_suppliers: Vec<String>,
    
    // Design context
    pub application: ComponentApplication,
    pub criticality: ComponentCriticality,
}

/// Application context for component selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComponentApplication {
    PowerSupply,
    SignalProcessing,
    DigitalLogic,
    AnalogCircuit,
    RFCircuit,
    PowerManagement,
    Interface,
    Protection,
    Timing,
    Filtering,
    Other(String),
}

/// Component criticality for reliability requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComponentCriticality {
    Critical,    // Safety-critical, high reliability required
    Important,   // Important for functionality
    Standard,    // Standard reliability requirements
    NonCritical, // Cost-optimized selection acceptable
}

/// Result of component synthesis
#[derive(Debug, Clone)]
pub struct SynthesisResult {
    pub recommended: Option<ComponentOption>,
    pub alternatives: Vec<ComponentOption>,
    pub synthesis_notes: Vec<String>,
    pub confidence: f64, // 0.0 to 1.0
}

/// A component option with supply chain information
#[derive(Debug, Clone)]
pub struct ComponentOption {
    pub component: Component,
    pub supplier_choice: SupplierChoice,
    pub total_cost: f64,
    pub lead_time: u32,
    pub fitness_score: f64, // How well it meets requirements
    pub selection_reason: String,
}

/// Component selection criteria for scoring
#[derive(Debug, Clone)]
pub struct SelectionCriteria {
    pub price_weight: f64,        // 0.0 to 1.0
    pub availability_weight: f64, // 0.0 to 1.0
    pub lead_time_weight: f64,    // 0.0 to 1.0
    pub spec_match_weight: f64,   // 0.0 to 1.0
    pub reliability_weight: f64,  // 0.0 to 1.0
}

impl Default for SelectionCriteria {
    fn default() -> Self {
        Self {
            price_weight: 0.3,
            availability_weight: 0.3,
            lead_time_weight: 0.2,
            spec_match_weight: 0.15,
            reliability_weight: 0.05,
        }
    }
}

impl SelectionCriteria {
    /// Create selection criteria based on component requirements
    pub fn from_requirements(requirements: &ComponentRequirements) -> Self {
        match requirements.criticality {
            ComponentCriticality::Critical => Self {
                price_weight: 0.1,
                availability_weight: 0.3,
                lead_time_weight: 0.2,
                spec_match_weight: 0.3,
                reliability_weight: 0.1,
            },
            ComponentCriticality::Important => Self {
                price_weight: 0.2,
                availability_weight: 0.3,
                lead_time_weight: 0.2,
                spec_match_weight: 0.25,
                reliability_weight: 0.05,
            },
            ComponentCriticality::Standard => Self::default(),
            ComponentCriticality::NonCritical => Self {
                price_weight: 0.5,
                availability_weight: 0.2,
                lead_time_weight: 0.1,
                spec_match_weight: 0.15,
                reliability_weight: 0.05,
            },
        }
    }
}

impl ComponentRequirements {
    /// Create requirements for a resistor
    pub fn resistor(resistance: f64, power_rating: f64, tolerance: f64, quantity: u32) -> Self {
        Self {
            resistance: Some(resistance),
            power_rating: Some(power_rating),
            tolerance: Some(tolerance),
            quantity,
            application: ComponentApplication::Other("Resistor".to_string()),
            criticality: ComponentCriticality::Standard,
            ..Default::default()
        }
    }

    /// Create requirements for a capacitor
    pub fn capacitor(capacitance: f64, voltage_rating: f64, tolerance: f64, quantity: u32) -> Self {
        Self {
            capacitance: Some(capacitance),
            voltage_rating: Some(voltage_rating),
            tolerance: Some(tolerance),
            quantity,
            application: ComponentApplication::Other("Capacitor".to_string()),
            criticality: ComponentCriticality::Standard,
            ..Default::default()
        }
    }

    /// Create requirements for an inductor
    pub fn inductor(inductance: f64, current_rating: f64, tolerance: f64, quantity: u32) -> Self {
        Self {
            inductance: Some(inductance),
            current_rating: Some(current_rating),
            tolerance: Some(tolerance),
            quantity,
            application: ComponentApplication::Other("Inductor".to_string()),
            criticality: ComponentCriticality::Standard,
            ..Default::default()
        }
    }
}

impl Default for ComponentRequirements {
    fn default() -> Self {
        Self {
            resistance: None,
            capacitance: None,
            inductance: None,
            voltage_rating: None,
            current_rating: None,
            power_rating: None,
            tolerance: None,
            frequency: None,
            package_type: None,
            temperature_range: None,
            quantity: 1,
            max_unit_price: None,
            max_lead_time_days: None,
            preferred_suppliers: Vec::new(),
            application: ComponentApplication::Other("Generic".to_string()),
            criticality: ComponentCriticality::Standard,
        }
    }
}

impl ComponentOption {
    /// Calculate fitness score based on how well the component meets requirements
    pub fn calculate_fitness_score(
        component: &Component,
        supplier_choice: &SupplierChoice,
        requirements: &ComponentRequirements,
        criteria: &SelectionCriteria,
    ) -> f64 {
        let mut score = 0.0;
        let mut total_weight = 0.0;

        // Price score (normalized)
        if let Some(max_price) = requirements.max_unit_price {
            let price_score = if supplier_choice.unit_price <= max_price {
                (max_price - supplier_choice.unit_price) / max_price
            } else {
                0.0 // Exceeds budget
            };
            score += price_score * criteria.price_weight;
            total_weight += criteria.price_weight;
        }

        // Availability score
        let availability_score = if supplier_choice.quantity_available >= requirements.quantity as i32 {
            1.0
        } else {
            supplier_choice.quantity_available as f64 / requirements.quantity as f64
        };
        score += availability_score * criteria.availability_weight;
        total_weight += criteria.availability_weight;

        // Lead time score
        if let Some(max_lead_time) = requirements.max_lead_time_days {
            let lead_time_score = if let Some(supplier_lead_time) = supplier_choice.lead_time_days {
                if supplier_lead_time <= max_lead_time as i32 {
                    (max_lead_time as i32 - supplier_lead_time) as f64 / max_lead_time as f64
                } else {
                    0.0 // Exceeds maximum lead time
                }
            } else {
                0.5 // Unknown lead time gets middle score
            };
            score += lead_time_score * criteria.lead_time_weight;
            total_weight += criteria.lead_time_weight;
        }

        // Specification match score
        let spec_score = Self::calculate_spec_match_score(component, requirements);
        score += spec_score * criteria.spec_match_weight;
        total_weight += criteria.spec_match_weight;

        // Normalize score
        if total_weight > 0.0 {
            score / total_weight
        } else {
            0.0
        }
    }

    /// Calculate how well component specs match requirements
    pub fn calculate_spec_match_score(component: &Component, requirements: &ComponentRequirements) -> f64 {
        let mut matches = 0;
        let mut total_checks = 0;

        // Check resistance match
        if let Some(required_resistance) = requirements.resistance {
            total_checks += 1;
            if let Some(resistance_spec) = component.get_electrical_spec("resistance") {
                let tolerance = requirements.tolerance.unwrap_or(0.05); // 5% default
                let min_value = required_resistance * (1.0 - tolerance);
                let max_value = required_resistance * (1.0 + tolerance);
                if resistance_spec.spec_value >= min_value && resistance_spec.spec_value <= max_value {
                    matches += 1;
                }
            }
        }

        // Check capacitance match
        if let Some(required_capacitance) = requirements.capacitance {
            total_checks += 1;
            if let Some(capacitance_spec) = component.get_electrical_spec("capacitance") {
                let tolerance = requirements.tolerance.unwrap_or(0.20); // 20% default for caps
                let min_value = required_capacitance * (1.0 - tolerance);
                let max_value = required_capacitance * (1.0 + tolerance);
                if capacitance_spec.spec_value >= min_value && capacitance_spec.spec_value <= max_value {
                    matches += 1;
                }
            }
        }

        // Check voltage rating
        if let Some(required_voltage) = requirements.voltage_rating {
            total_checks += 1;
            if let Some(voltage_spec) = component.get_electrical_spec("voltage_rating") {
                // Voltage rating should be >= required (with safety margin)
                if voltage_spec.spec_value >= required_voltage * 1.2 {
                    matches += 1;
                }
            }
        }

        // Check power rating
        if let Some(required_power) = requirements.power_rating {
            total_checks += 1;
            if let Some(power_spec) = component.get_electrical_spec("power_rating") {
                // Power rating should be >= required (with safety margin)
                if power_spec.spec_value >= required_power * 1.5 {
                    matches += 1;
                }
            }
        }

        if total_checks > 0 {
            matches as f64 / total_checks as f64
        } else {
            1.0 // No specific requirements to check
        }
    }
}

impl SynthesisResult {
    /// Create a new synthesis result
    pub fn new() -> Self {
        Self {
            recommended: None,
            alternatives: Vec::new(),
            synthesis_notes: Vec::new(),
            confidence: 0.0,
        }
    }

    /// Add a component option to alternatives
    pub fn add_alternative(&mut self, option: ComponentOption) {
        self.alternatives.push(option);
        self.alternatives.sort_by(|a, b| b.fitness_score.partial_cmp(&a.fitness_score).unwrap());
    }

    /// Set the recommended option (typically the best alternative)
    pub fn set_recommended(&mut self) {
        if let Some(best_option) = self.alternatives.first() {
            self.recommended = Some(best_option.clone());
            self.confidence = best_option.fitness_score;
        }
    }

    /// Add a synthesis note
    pub fn add_note(&mut self, note: String) {
        self.synthesis_notes.push(note);
    }

    /// Check if synthesis was successful
    pub fn is_successful(&self) -> bool {
        self.recommended.is_some() && self.confidence > 0.5
    }
}