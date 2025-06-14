//! Component specification matching logic
//!
//! The ComponentMatcher finds candidate components that match given requirements
//! through sophisticated matching algorithms that consider electrical specifications,
//! physical constraints, and application context.

use std::collections::HashMap;
use anyhow::{Result, Context};
use log::{debug, info, warn};

use crate::types::*;
use crate::database::ComponentDatabase;

/// Component matcher that finds candidates based on requirements
pub struct ComponentMatcher {
    tolerance_margin: f64,
    voltage_safety_factor: f64,
    power_safety_factor: f64,
    version: String,
}

impl ComponentMatcher {
    /// Create a new component matcher with default tolerances
    pub fn new() -> Self {
        Self {
            tolerance_margin: 0.1,   // 10% tolerance margin for fuzzy matching
            voltage_safety_factor: 1.2, // 20% voltage safety margin
            power_safety_factor: 1.5,   // 50% power safety margin
            version: "1.0.0".to_string(),
        }
    }

    /// Create a matcher with custom tolerance settings
    pub fn with_tolerances(
        tolerance_margin: f64,
        voltage_safety_factor: f64,
        power_safety_factor: f64,
    ) -> Self {
        Self {
            tolerance_margin,
            voltage_safety_factor,
            power_safety_factor,
            version: "1.0.0".to_string(),
        }
    }

    /// Get the matcher version
    pub fn get_version(&self) -> String {
        self.version.clone()
    }

    /// Find components that match the given requirements
    pub async fn find_matching_components(
        &self,
        component_type: &str,
        requirements: &ComponentRequirements,
        database: &ComponentDatabase,
        max_results: usize,
    ) -> Result<Vec<Component>> {
        info!("Finding components matching type: {}", component_type);

        // Determine component category from type string
        let category = self.parse_component_category(component_type)?;
        
        // Get all components of the specified category
        let candidates = database.get_components_by_category(&category).await
            .context("Failed to get components by category")?;

        info!("Found {} candidates of category {:?}", candidates.len(), category);

        // Filter candidates based on requirements
        let mut matching_components = Vec::new();
        
        for component in candidates {
            if self.matches_requirements(&component, requirements) {
                matching_components.push(component);
            }
        }

        info!("Filtered to {} matching components", matching_components.len());

        // Rank by match quality and return top results
        let mut ranked_components = self.rank_by_match_quality(matching_components, requirements);
        ranked_components.truncate(max_results);

        Ok(ranked_components)
    }

    /// Find components using electrical specification search
    pub async fn find_by_electrical_specs(
        &self,
        category: &ComponentCategory,
        spec_requirements: &[(String, f64, f64)], // (spec_name, min_value, max_value)
        database: &ComponentDatabase,
        max_results: usize,
    ) -> Result<Vec<Component>> {
        debug!("Finding components by electrical specs for category {:?}", category);

        let candidates = database.find_components_by_specs(category, spec_requirements).await
            .context("Failed to find components by specs")?;

        debug!("Found {} candidates matching electrical specs", candidates.len());

        // Return top results (already filtered by database query)
        Ok(candidates.into_iter().take(max_results).collect())
    }

    /// Find component candidates for two-stage synthesis
    pub async fn find_component_candidates(
        &self,
        requirements: &ComponentRequirements,
        database: &ComponentDatabase,
    ) -> Result<Vec<Component>> {
        debug!("Finding component candidates for two-stage synthesis");

        // Determine component category from requirements
        let category = self.infer_component_category(requirements);
        
        // Build specification filters
        let spec_filters = self.build_specification_filters(requirements);
        
        // Query database for matching components
        let candidates = if spec_filters.is_empty() {
            // No specific electrical specs, get all components of category
            database.get_components_by_category(&category).await?
        } else {
            // Use electrical spec filtering
            database.find_components_by_specs(&category, &spec_filters).await?
        };
        
        debug!("Found {} candidates before requirements filtering", candidates.len());
        
        // Filter by additional requirements
        let filtered_candidates = self.filter_by_requirements(&candidates, requirements)?;
        
        debug!("Filtered to {} candidates after requirements check", filtered_candidates.len());
        
        Ok(filtered_candidates)
    }

    /// Parse component type string into category
    fn parse_component_category(&self, component_type: &str) -> Result<ComponentCategory> {
        let lower_type = component_type.to_lowercase();
        
        match lower_type.as_str() {
            "resistor" | "res" | "r" => Ok(ComponentCategory::Resistor),
            "capacitor" | "cap" | "c" => Ok(ComponentCategory::Capacitor),
            "inductor" | "ind" | "l" => Ok(ComponentCategory::Inductor),
            "diode" | "d" => Ok(ComponentCategory::Diode),
            "transistor" | "trans" | "q" => Ok(ComponentCategory::Transistor),
            "ic" | "chip" | "integrated_circuit" => Ok(ComponentCategory::IC),
            "connector" | "conn" | "j" => Ok(ComponentCategory::Connector),
            "crystal" | "xtal" | "y" => Ok(ComponentCategory::Crystal),
            "led" => Ok(ComponentCategory::LED),
            "switch" | "sw" => Ok(ComponentCategory::Switch),
            "relay" | "k" => Ok(ComponentCategory::Relay),
            "transformer" | "t" => Ok(ComponentCategory::Transformer),
            "fuse" | "f" => Ok(ComponentCategory::Fuse),
            _ => Ok(ComponentCategory::Other(component_type.to_string())),
        }
    }

    /// Check if a component matches the given requirements
    fn matches_requirements(&self, component: &Component, requirements: &ComponentRequirements) -> bool {
        // Check electrical specifications
        if !self.matches_electrical_specs(component, requirements) {
            return false;
        }

        // Check physical requirements
        if !self.matches_physical_requirements(component, requirements) {
            return false;
        }

        // Check application context
        if !self.matches_application_context(component, requirements) {
            return false;
        }

        true
    }

    /// Check if component matches electrical specifications
    fn matches_electrical_specs(&self, component: &Component, requirements: &ComponentRequirements) -> bool {
        // Check resistance
        if let Some(required_resistance) = requirements.resistance {
            if let Some(resistance_spec) = component.get_electrical_spec("resistance") {
                if !self.matches_value_with_tolerance(
                    resistance_spec.spec_value,
                    required_resistance,
                    requirements.tolerance
                ) {
                    return false;
                }
            } else {
                // No resistance spec found, but one is required
                return false;
            }
        }

        // Check capacitance
        if let Some(required_capacitance) = requirements.capacitance {
            if let Some(capacitance_spec) = component.get_electrical_spec("capacitance") {
                if !self.matches_value_with_tolerance(
                    capacitance_spec.spec_value,
                    required_capacitance,
                    requirements.tolerance
                ) {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Check inductance
        if let Some(required_inductance) = requirements.inductance {
            if let Some(inductance_spec) = component.get_electrical_spec("inductance") {
                if !self.matches_value_with_tolerance(
                    inductance_spec.spec_value,
                    required_inductance,
                    requirements.tolerance
                ) {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Check voltage rating (must be >= required with safety margin)
        if let Some(required_voltage) = requirements.voltage_rating {
            if let Some(voltage_spec) = component.get_electrical_spec("voltage_rating") {
                if voltage_spec.spec_value < required_voltage * self.voltage_safety_factor {
                    return false;
                }
            }
        }

        // Check current rating
        if let Some(required_current) = requirements.current_rating {
            if let Some(current_spec) = component.get_electrical_spec("current_rating") {
                if current_spec.spec_value < required_current * 1.1 { // 10% safety margin
                    return false;
                }
            }
        }

        // Check power rating (must be >= required with safety margin)
        if let Some(required_power) = requirements.power_rating {
            if let Some(power_spec) = component.get_electrical_spec("power_rating") {
                if power_spec.spec_value < required_power * self.power_safety_factor {
                    return false;
                }
            }
        }

        // Check frequency rating
        if let Some(required_frequency) = requirements.frequency {
            if let Some(frequency_spec) = component.get_electrical_spec("frequency") {
                if frequency_spec.spec_value < required_frequency {
                    return false;
                }
            }
        }

        true
    }

    /// Check if component matches physical requirements
    fn matches_physical_requirements(&self, component: &Component, requirements: &ComponentRequirements) -> bool {
        // Check package type
        if let Some(required_package) = &requirements.package_type {
            if let Some(component_package) = &component.package_type {
                if !self.matches_package_type(component_package, required_package) {
                    return false;
                }
            } else {
                // No package info, but one is required
                return false;
            }
        }

        // Check temperature range
        if let Some((min_temp, max_temp)) = requirements.temperature_range {
            // Look for temperature-related specs
            if let Some(temp_spec) = component.get_electrical_spec("operating_temperature") {
                if let (Some(spec_min), Some(spec_max)) = (temp_spec.min_value, temp_spec.max_value) {
                    if spec_min > min_temp || spec_max < max_temp {
                        return false;
                    }
                }
            }
        }

        true
    }

    /// Check if component matches application context
    fn matches_application_context(&self, component: &Component, requirements: &ComponentRequirements) -> bool {
        // This is a sophisticated matching based on application context
        // For now, we'll implement basic matching logic
        
        match requirements.application {
            ComponentApplication::PowerSupply => {
                // Power supply components should have good power handling
                if let Some(power_spec) = component.get_electrical_spec("power_rating") {
                    power_spec.spec_value >= 0.1 // At least 0.1W for power supply apps
                } else {
                    true // No power spec, assume OK
                }
            }
            ComponentApplication::RFCircuit => {
                // RF components should have frequency specs
                component.get_electrical_spec("frequency").is_some() ||
                component.get_electrical_spec("bandwidth").is_some()
            }
            ComponentApplication::DigitalLogic => {
                // Digital logic components should have reasonable voltage ratings
                if let Some(voltage_spec) = component.get_electrical_spec("voltage_rating") {
                    voltage_spec.spec_value >= 3.0 && voltage_spec.spec_value <= 12.0
                } else {
                    true
                }
            }
            ComponentApplication::PowerManagement => {
                // Power management components need good power handling
                if let Some(power_spec) = component.get_electrical_spec("power_rating") {
                    power_spec.spec_value >= 0.5 // At least 0.5W
                } else {
                    false // Power spec required for power management
                }
            }
            _ => true, // Other applications are less restrictive
        }
    }

    /// Check if a value matches with tolerance
    fn matches_value_with_tolerance(
        &self,
        actual_value: f64,
        required_value: f64,
        tolerance: Option<f64>,
    ) -> bool {
        let tolerance = tolerance.unwrap_or(0.2); // Default 20% tolerance
        let min_value = required_value * (1.0 - tolerance - self.tolerance_margin);
        let max_value = required_value * (1.0 + tolerance + self.tolerance_margin);
        
        actual_value >= min_value && actual_value <= max_value
    }

    /// Check if package types match (with fuzzy matching)
    fn matches_package_type(&self, component_package: &str, required_package: &str) -> bool {
        let comp_pkg = component_package.to_lowercase();
        let req_pkg = required_package.to_lowercase();
        
        // Exact match
        if comp_pkg == req_pkg {
            return true;
        }
        
        // Common package equivalents
        let package_equivalents = HashMap::from([
            ("0603", vec!["0603", "1608", "1608m"]),
            ("0805", vec!["0805", "2012", "2012m"]),
            ("1206", vec!["1206", "3216", "3216m"]),
            ("sot23", vec!["sot23", "sot-23", "sot_23"]),
            ("soic8", vec!["soic8", "soic-8", "so8"]),
            ("dip8", vec!["dip8", "dip-8", "pdip8"]),
        ]);
        
        // Check equivalents
        for (standard, equivalents) in package_equivalents {
            if req_pkg.contains(standard) {
                return equivalents.iter().any(|equiv| comp_pkg.contains(equiv));
            }
        }
        
        // Fuzzy matching for similar package names
        if self.fuzzy_match_package(&comp_pkg, &req_pkg) {
            return true;
        }
        
        false
    }

    /// Fuzzy match package types
    fn fuzzy_match_package(&self, pkg1: &str, pkg2: &str) -> bool {
        // Simple fuzzy matching based on common substrings
        let pkg1_parts: Vec<&str> = pkg1.split(&['-', '_', ' '][..]).collect();
        let pkg2_parts: Vec<&str> = pkg2.split(&['-', '_', ' '][..]).collect();
        
        // Check if any significant part matches
        for part1 in &pkg1_parts {
            for part2 in &pkg2_parts {
                if part1.len() >= 3 && part2.len() >= 3 && part1 == part2 {
                    return true;
                }
            }
        }
        
        false
    }

    /// Rank components by match quality
    fn rank_by_match_quality(
        &self,
        mut components: Vec<Component>,
        requirements: &ComponentRequirements,
    ) -> Vec<Component> {
        // Calculate match scores for each component
        let mut scored_components: Vec<(Component, f64)> = components
            .into_iter()
            .map(|component| {
                let score = self.calculate_match_score(&component, requirements);
                (component, score)
            })
            .collect();

        // Sort by score (highest first)
        scored_components.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Return sorted components
        scored_components.into_iter().map(|(component, _)| component).collect()
    }

    /// Calculate match quality score for a component
    fn calculate_match_score(&self, component: &Component, requirements: &ComponentRequirements) -> f64 {
        let mut score = 0.0;
        let mut total_weight = 0.0;

        // Electrical specification matches (weight: 0.6)
        let electrical_score = self.calculate_electrical_match_score(component, requirements);
        score += electrical_score * 0.6;
        total_weight += 0.6;

        // Physical match score (weight: 0.2)
        let physical_score = self.calculate_physical_match_score(component, requirements);
        score += physical_score * 0.2;
        total_weight += 0.2;

        // Application context score (weight: 0.1)
        let application_score = self.calculate_application_match_score(component, requirements);
        score += application_score * 0.1;
        total_weight += 0.1;

        // Availability and quality indicators (weight: 0.1)
        let quality_score = self.calculate_quality_score(component);
        score += quality_score * 0.1;
        total_weight += 0.1;

        if total_weight > 0.0 {
            score / total_weight
        } else {
            0.0
        }
    }

    /// Calculate electrical specification match score
    fn calculate_electrical_match_score(&self, component: &Component, requirements: &ComponentRequirements) -> f64 {
        let mut score = 0.0;
        let mut checks = 0;

        // Score based on how close values are to requirements
        if let Some(required_resistance) = requirements.resistance {
            if let Some(resistance_spec) = component.get_electrical_spec("resistance") {
                let ratio = (resistance_spec.spec_value / required_resistance).min(required_resistance / resistance_spec.spec_value);
                score += ratio;
                checks += 1;
            }
        }

        if let Some(required_capacitance) = requirements.capacitance {
            if let Some(capacitance_spec) = component.get_electrical_spec("capacitance") {
                let ratio = (capacitance_spec.spec_value / required_capacitance).min(required_capacitance / capacitance_spec.spec_value);
                score += ratio;
                checks += 1;
            }
        }

        // Voltage rating score (higher is better)
        if let Some(required_voltage) = requirements.voltage_rating {
            if let Some(voltage_spec) = component.get_electrical_spec("voltage_rating") {
                let ratio = if voltage_spec.spec_value >= required_voltage {
                    (required_voltage / voltage_spec.spec_value).max(0.5) // Bonus for meeting requirement
                } else {
                    0.0 // Penalty for not meeting requirement
                };
                score += ratio;
                checks += 1;
            }
        }

        if checks > 0 {
            score / checks as f64
        } else {
            0.5 // Neutral score if no electrical specs to check
        }
    }

    /// Calculate physical match score
    fn calculate_physical_match_score(&self, component: &Component, requirements: &ComponentRequirements) -> f64 {
        let mut score = 0.0;
        let mut checks = 0;

        // Package type match
        if let Some(required_package) = &requirements.package_type {
            if let Some(component_package) = &component.package_type {
                if self.matches_package_type(component_package, required_package) {
                    score += 1.0;
                }
            }
            checks += 1;
        }

        // Temperature range match
        if let Some((min_temp, max_temp)) = requirements.temperature_range {
            if let Some(temp_spec) = component.get_electrical_spec("operating_temperature") {
                if let (Some(spec_min), Some(spec_max)) = (temp_spec.min_value, temp_spec.max_value) {
                    if spec_min <= min_temp && spec_max >= max_temp {
                        score += 1.0;
                    } else {
                        // Partial credit for overlap
                        let overlap = (spec_max.min(max_temp) - spec_min.max(min_temp)) / (max_temp - min_temp);
                        score += overlap.max(0.0);
                    }
                }
            }
            checks += 1;
        }

        if checks > 0 {
            score / checks as f64
        } else {
            1.0 // Perfect score if no physical requirements
        }
    }

    /// Calculate application context match score
    fn calculate_application_match_score(&self, component: &Component, requirements: &ComponentRequirements) -> f64 {
        // This would be more sophisticated in a real implementation
        // For now, return 1.0 if it matches application context
        if self.matches_application_context(component, requirements) {
            1.0
        } else {
            0.5 // Partial credit
        }
    }

    /// Calculate component quality score
    fn calculate_quality_score(&self, component: &Component) -> f64 {
        let mut score: f64 = 0.5; // Base score

        // Bonus for having manufacturer info
        if component.manufacturer.is_some() {
            score += 0.1;
        }

        // Bonus for having part number
        if component.part_number.is_some() {
            score += 0.1;
        }

        // Bonus for having datasheet
        if component.datasheet_url.is_some() {
            score += 0.1;
        }

        // Bonus for having detailed electrical specs
        if component.electrical_specs.len() >= 3 {
            score += 0.1;
        }

        // Bonus for having symbol and footprint
        if component.symbol.is_some() {
            score += 0.05;
        }
        if component.footprint.is_some() {
            score += 0.05;
        }

        score.min(1.0)
    }

    /// Infer component category from requirements
    fn infer_component_category(&self, requirements: &ComponentRequirements) -> ComponentCategory {
        if requirements.resistance.is_some() {
            ComponentCategory::Resistor
        } else if requirements.capacitance.is_some() {
            ComponentCategory::Capacitor
        } else if requirements.inductance.is_some() {
            ComponentCategory::Inductor
        } else {
            // Default to IC for complex requirements
            ComponentCategory::IC
        }
    }

    /// Build specification filters for database query
    fn build_specification_filters(&self, requirements: &ComponentRequirements) -> Vec<(String, f64, f64)> {
        let mut filters = Vec::new();

        if let Some(resistance) = requirements.resistance {
            let tolerance = requirements.tolerance.unwrap_or(0.05);
            let min_val = resistance * (1.0 - tolerance);
            let max_val = resistance * (1.0 + tolerance);
            filters.push(("resistance".to_string(), min_val, max_val));
        }

        if let Some(capacitance) = requirements.capacitance {
            let tolerance = requirements.tolerance.unwrap_or(0.20); // 20% default for caps
            let min_val = capacitance * (1.0 - tolerance);
            let max_val = capacitance * (1.0 + tolerance);
            filters.push(("capacitance".to_string(), min_val, max_val));
        }

        if let Some(inductance) = requirements.inductance {
            let tolerance = requirements.tolerance.unwrap_or(0.20);
            let min_val = inductance * (1.0 - tolerance);
            let max_val = inductance * (1.0 + tolerance);
            filters.push(("inductance".to_string(), min_val, max_val));
        }

        if let Some(voltage_rating) = requirements.voltage_rating {
            // Voltage rating should be at least 20% higher than required
            let min_voltage = voltage_rating * self.voltage_safety_factor;
            filters.push(("voltage_rating".to_string(), min_voltage, f64::MAX));
        }

        if let Some(power_rating) = requirements.power_rating {
            // Power rating should be at least 50% higher than required
            let min_power = power_rating * self.power_safety_factor;
            filters.push(("power_rating".to_string(), min_power, f64::MAX));
        }

        filters
    }

    /// Filter components by additional requirements
    fn filter_by_requirements(&self, components: &[Component], requirements: &ComponentRequirements) -> Result<Vec<Component>> {
        let mut filtered = Vec::new();

        for component in components {
            if self.matches_requirements(component, requirements) {
                filtered.push(component.clone());
            }
        }

        Ok(filtered)
    }
}

impl Default for ComponentMatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_category_parsing() {
        let matcher = ComponentMatcher::new();
        
        assert!(matches!(matcher.parse_component_category("resistor").unwrap(), ComponentCategory::Resistor));
        assert!(matches!(matcher.parse_component_category("cap").unwrap(), ComponentCategory::Capacitor));
        assert!(matches!(matcher.parse_component_category("IC").unwrap(), ComponentCategory::IC));
        assert!(matches!(matcher.parse_component_category("unknown").unwrap(), ComponentCategory::Other(_)));
    }

    #[test]
    fn test_value_tolerance_matching() {
        let matcher = ComponentMatcher::new();
        
        // Test exact match
        assert!(matcher.matches_value_with_tolerance(1000.0, 1000.0, Some(0.05)));
        
        // Test within tolerance
        assert!(matcher.matches_value_with_tolerance(1050.0, 1000.0, Some(0.05)));
        assert!(matcher.matches_value_with_tolerance(950.0, 1000.0, Some(0.05)));
        
        // Test outside tolerance
        assert!(!matcher.matches_value_with_tolerance(1200.0, 1000.0, Some(0.05)));
        assert!(!matcher.matches_value_with_tolerance(800.0, 1000.0, Some(0.05)));
    }

    #[test]
    fn test_package_type_matching() {
        let matcher = ComponentMatcher::new();
        
        // Test exact match
        assert!(matcher.matches_package_type("0603", "0603"));
        
        // Test case insensitive
        assert!(matcher.matches_package_type("SOT23", "sot23"));
        
        // Test equivalent packages
        assert!(matcher.matches_package_type("1608", "0603"));
        assert!(matcher.matches_package_type("SOT-23", "sot23"));
    }

    #[test]
    fn test_match_score_calculation() {
        let matcher = ComponentMatcher::new();
        let requirements = ComponentRequirements::resistor(1000.0, 0.25, 0.05, 1);
        
        let component = Component {
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
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        
        let score = matcher.calculate_match_score(&component, &requirements);
        assert!(score > 0.5); // Should get a decent score
    }
}