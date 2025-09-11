/// Redundancy analysis for safety-critical systems
/// 
/// This module analyzes component redundancy and its impact on safety metrics.
/// It considers various redundancy schemes (1oo2, 2oo3, etc.) and calculates
/// the effective failure rates considering redundancy.

use std::collections::HashMap;
use super::requirement_hierarchy::{RequirementHierarchy, ImplementationDetails};
use super::fmea_analysis::{FMEAAnalysis, FailureType};

/// Redundancy configuration for a function
#[derive(Debug, Clone)]
pub struct RedundancyConfig {
    pub requirement_id: String,
    pub components: Vec<String>,
    pub redundancy_type: RedundancyType,
    pub common_cause_factor: f64,  // Beta factor for CCF
}

/// Types of redundancy configurations
#[derive(Debug, Clone, PartialEq)]
pub enum RedundancyType {
    /// No redundancy (single channel)
    Single,
    /// 1-out-of-2 (one component must work)
    OneOutOfTwo,
    /// 2-out-of-2 (both must work - series)
    TwoOutOfTwo,
    /// 2-out-of-3 (majority voting)
    TwoOutOfThree,
    /// 1-out-of-3 (any one must work)
    OneOutOfThree,
    /// N-out-of-M general case
    NOutOfM { n: usize, m: usize },
}

impl RedundancyType {
    /// Parse redundancy type from component count
    pub fn from_component_count(count: usize, requirement_id: &str) -> Self {
        // Infer redundancy type from requirement ID and component count
        match count {
            1 => RedundancyType::Single,
            2 => {
                // Default to 1oo2 for dual redundancy (parallel)
                // Components are redundant if any one can fulfill the requirement
                RedundancyType::OneOutOfTwo
            }
            3 => {
                // Default to 2oo3 for triple redundancy (voting)
                RedundancyType::TwoOutOfThree
            }
            _ => RedundancyType::NOutOfM { n: (count + 1) / 2, m: count }, // Majority voting
        }
    }
    
    /// Calculate effective failure rate considering redundancy
    pub fn calculate_effective_failure_rate(&self, base_failure_rate: f64, ccf: f64) -> f64 {
        match self {
            RedundancyType::Single => base_failure_rate,
            
            RedundancyType::OneOutOfTwo => {
                // Parallel redundancy: both must fail for system to fail
                // Only one needs to work (1oo2)
                // Effective rate is much lower due to redundancy
                let p = base_failure_rate / 1e9;  // Convert FIT to probability
                let p_both_fail = p * p * (1.0 - ccf) + p * ccf;  // Independent + CCF
                p_both_fail * 1e9  // Convert back to FIT
            }
            
            RedundancyType::TwoOutOfTwo => {
                // Series: both must work (not a safety redundancy case)
                // Either component failing causes system failure
                // This is worse than single component, so we shouldn't use it
                2.0 * base_failure_rate  // Approximation for low failure rates
            }
            
            RedundancyType::TwoOutOfThree => {
                // 2oo3 voting: at least 2 must work
                // P_sys = 3*P^2*(1-P) + P^3 + P_ccf
                let p = base_failure_rate / 1e9;  // Convert FIT to probability
                let independent = 3.0 * p * p * (1.0 - p) + p * p * p;
                let common_cause = base_failure_rate * ccf;
                independent * 1e9 + common_cause  // Convert back to FIT
            }
            
            RedundancyType::OneOutOfThree => {
                // All three must fail
                let p = base_failure_rate / 1e9;
                let independent = p * p * p;
                let common_cause = base_failure_rate * ccf;
                independent * 1e9 + common_cause
            }
            
            RedundancyType::NOutOfM { n, m } => {
                // General N-out-of-M calculation
                // This is a simplified approximation
                let p = base_failure_rate / 1e9;
                let k = *m - *n + 1;  // Number of components that must fail
                let failure_prob = p.powi(k as i32);
                failure_prob * 1e9 + base_failure_rate * ccf
            }
        }
    }
    
    /// Get redundancy description string
    pub fn description(&self) -> String {
        match self {
            RedundancyType::Single => "No redundancy".to_string(),
            RedundancyType::OneOutOfTwo => "1oo2 (Parallel redundancy)".to_string(),
            RedundancyType::TwoOutOfTwo => "2oo2 (Series redundancy)".to_string(),
            RedundancyType::TwoOutOfThree => "2oo3 (Triple modular redundancy)".to_string(),
            RedundancyType::OneOutOfThree => "1oo3 (Triple parallel redundancy)".to_string(),
            RedundancyType::NOutOfM { n, m } => format!("{}oo{} redundancy", n, m),
        }
    }
}

/// Analyze redundancy in the system
pub struct RedundancyAnalyzer {
    pub redundancy_configs: HashMap<String, RedundancyConfig>,
    pub effective_failure_rates: HashMap<String, f64>,
}

impl RedundancyAnalyzer {
    pub fn new() -> Self {
        Self {
            redundancy_configs: HashMap::new(),
            effective_failure_rates: HashMap::new(),
        }
    }
    
    /// Analyze redundancy from requirement hierarchy
    pub fn analyze_from_hierarchy(&mut self, hierarchy: &RequirementHierarchy) {
        for (req_id, requirement) in &hierarchy.requirements {
            if let ImplementationDetails::ByComponents(components) = &requirement.implemented_by {
                if !components.is_empty() {
                    let redundancy_type = RedundancyType::from_component_count(
                        components.len(), 
                        req_id
                    );
                    
                    let config = RedundancyConfig {
                        requirement_id: req_id.clone(),
                        components: components.clone(),
                        redundancy_type,
                        common_cause_factor: 0.02,  // Default 2% CCF
                    };
                    
                    self.redundancy_configs.insert(req_id.clone(), config);
                }
            }
        }
    }
    
    /// Apply redundancy analysis to FMEA
    pub fn apply_to_fmea(&self, fmea: &mut FMEAAnalysis) {
        // Group components by their requirements
        let mut component_to_reqs: HashMap<String, Vec<String>> = HashMap::new();
        
        for (req_id, config) in &self.redundancy_configs {
            for comp in &config.components {
                component_to_reqs.entry(comp.clone())
                    .or_insert_with(Vec::new)
                    .push(req_id.clone());
            }
        }
        
        // Update failure modes based on redundancy
        for (comp_id, component) in &mut fmea.components {
            if let Some(req_ids) = component_to_reqs.get(comp_id) {
                for req_id in req_ids {
                    if let Some(config) = self.redundancy_configs.get(req_id) {
                        // If component is part of redundant configuration
                        if config.redundancy_type != RedundancyType::Single {
                            // Update failure types: SPF becomes MPF due to redundancy
                            for mode in &mut component.failure_modes {
                                if mode.failure_type == FailureType::SinglePointFault {
                                    mode.failure_type = FailureType::MultiPointFault;
                                    
                                    // Adjust failure rate based on redundancy
                                    let effective_rate = config.redundancy_type
                                        .calculate_effective_failure_rate(
                                            mode.failure_rate,
                                            config.common_cause_factor
                                        );
                                    
                                    // Store original rate and update with effective
                                    mode.residual_failure_rate = Some(effective_rate);
                                }
                            }
                            
                            // Add redundancy as a safety mechanism
                            if !component.safety_mechanisms.iter()
                                .any(|m| m.description.contains("redundancy")) {
                                component.safety_mechanisms.push(
                                    super::fmea_analysis::SafetyMechanism {
                                        id: format!("SM_{}_REDUNDANCY", comp_id.to_uppercase()),
                                        description: config.redundancy_type.description(),
                                        mechanism_type: super::fmea_analysis::SafetyMechanismType::HardwareRedundancy,
                                        coverage: match config.redundancy_type {
                                            RedundancyType::TwoOutOfThree => 0.99,
                                            RedundancyType::OneOutOfTwo => 0.95,
                                            _ => 0.90,
                                        },
                                        targets: vec!["hardware_failure".to_string()],
                                    }
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    
    /// Calculate redundancy-adjusted SPFM
    pub fn calculate_adjusted_spfm(&self, fmea: &FMEAAnalysis) -> f64 {
        let mut total_failure_rate = 0.0;
        let mut spf_residual_rate = 0.0;
        
        for component in fmea.components.values() {
            if !component.is_safety_relevant {
                continue;
            }
            
            for mode in &component.failure_modes {
                // Use residual rate if available (redundancy-adjusted)
                let effective_rate = mode.residual_failure_rate
                    .unwrap_or(mode.failure_rate);
                
                total_failure_rate += effective_rate;
                
                if mode.failure_type == FailureType::SinglePointFault {
                    // This should be rare after redundancy analysis
                    let residual = effective_rate * (1.0 - mode.diagnostic_coverage);
                    spf_residual_rate += residual;
                }
            }
        }
        
        if total_failure_rate > 0.0 {
            1.0 - (spf_residual_rate / total_failure_rate)
        } else {
            1.0
        }
    }
    
    /// Generate redundancy report
    pub fn generate_report(&self) -> RedundancyReport {
        let mut report = RedundancyReport {
            total_functions: self.redundancy_configs.len(),
            redundant_functions: 0,
            single_channel_functions: 0,
            highest_redundancy: RedundancyType::Single,
            configurations: Vec::new(),
        };
        
        for config in self.redundancy_configs.values() {
            if config.redundancy_type != RedundancyType::Single {
                report.redundant_functions += 1;
            } else {
                report.single_channel_functions += 1;
            }
            
            report.configurations.push(config.clone());
        }
        
        // Find highest redundancy level
        if let Some(config) = self.redundancy_configs.values()
            .max_by_key(|c| c.components.len()) {
            report.highest_redundancy = config.redundancy_type.clone();
        }
        
        report
    }
}

/// Redundancy analysis report
#[derive(Debug)]
pub struct RedundancyReport {
    pub total_functions: usize,
    pub redundant_functions: usize,
    pub single_channel_functions: usize,
    pub highest_redundancy: RedundancyType,
    pub configurations: Vec<RedundancyConfig>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_redundancy_type_calculation() {
        // Test 1oo2 redundancy
        let oot = RedundancyType::OneOutOfTwo;
        let effective = oot.calculate_effective_failure_rate(100.0, 0.02);
        assert!(effective < 100.0);  // Should be much less than single failure
        
        // Test 2oo3 redundancy
        let toot = RedundancyType::TwoOutOfThree;
        let effective = toot.calculate_effective_failure_rate(100.0, 0.02);
        assert!(effective < 50.0);  // Should provide significant improvement
    }
}