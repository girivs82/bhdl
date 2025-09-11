//! Main safety analyzer

use crate::{
    violations::{SafetyViolation, Severity, ViolationType, ComponentLocation},
    reports::SafetyReport,
    circuit_converter,
};
use bhdl_netlist::Netlist;
use bhdl_spice::{
    DcAnalysis, ComponentModel, ElectricalLimits,
    SafetyAnalysisEngine, SafetyConfig as SpiceSafetyConfig,
};
use std::collections::HashMap;
use anyhow::{Result, Context};
use log::{info, warn, debug};

/// Configuration for safety analysis
#[derive(Debug, Clone)]
pub struct SafetyConfig {
    /// Enable DC analysis
    pub run_dc_analysis: bool,
    /// Voltage derating factor (0.8 = use 80% of max)
    pub voltage_derating: f64,
    /// Current derating factor (0.7 = use 70% of max)
    pub current_derating: f64,
    /// Power derating factor (0.5 = use 50% of max)
    pub power_derating: f64,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            run_dc_analysis: true,
            voltage_derating: 0.8,
            current_derating: 0.7,
            power_derating: 0.5,
        }
    }
}

/// Main safety analyzer
pub struct SafetyAnalyzer {
    config: SafetyConfig,
}

impl SafetyAnalyzer {
    /// Create a new safety analyzer
    pub fn new(config: SafetyConfig) -> Self {
        Self { config }
    }
    
    /// Create with default config
    pub fn default() -> Self {
        Self::new(SafetyConfig::default())
    }
    
    /// Analyze a netlist for safety violations
    pub fn analyze(&self, netlist: &Netlist) -> Result<SafetyReport> {
        info!("Starting safety analysis for netlist with {} instances", netlist.instances.len());
        
        let mut violations = Vec::new();
        
        // Run DC analysis if enabled
        if self.config.run_dc_analysis {
            match self.run_dc_safety_analysis(netlist) {
                Ok(dc_violations) => {
                    info!("DC analysis found {} violations", dc_violations.len());
                    violations.extend(dc_violations);
                }
                Err(e) => {
                    warn!("DC analysis failed: {}", e);
                    violations.push(SafetyViolation::new(
                        Severity::Warning,
                        ViolationType::MissingProtection {
                            component: "Circuit".to_string(),
                            protection_type: "DC Analysis".to_string(),
                        },
                        format!("DC analysis failed: {}", e),
                        "Unable to verify electrical safety through simulation".to_string(),
                    ));
                }
            }
        }
        
        // Run heuristic checks that don't need DC analysis
        violations.extend(self.run_heuristic_checks(netlist));
        
        Ok(SafetyReport::from_violations(violations))
    }
    
    /// Run DC-based safety analysis
    fn run_dc_safety_analysis(&self, netlist: &Netlist) -> Result<Vec<SafetyViolation>> {
        // Extract power information
        let mut power_info = HashMap::new();
        
        // Look for power nets (this is a heuristic - ideally we'd have metadata)
        for (_id, net) in &netlist.nets {
            if let Some(name) = &net.name {
                match name.as_str() {
                    "VCC" | "VDD" => power_info.insert(name.clone(), 5.0),
                    "3V3" | "3.3V" => power_info.insert(name.clone(), 3.3),
                    "12V" => power_info.insert(name.clone(), 12.0),
                    "24V" => power_info.insert(name.clone(), 24.0),
                    _ => None,
                };
            }
        }
        
        if power_info.is_empty() {
            // Try to find VCC at least
            if netlist.nets.values().any(|net| net.name.as_ref() == Some(&"VCC".to_string())) {
                power_info.insert("VCC".to_string(), 5.0);
            }
        }
        
        debug!("Found {} power nets", power_info.len());
        
        // Convert to SPICE circuit
        let circuit = circuit_converter::netlist_to_circuit_with_power(netlist, &power_info)
            .map_err(|e| anyhow::anyhow!("Failed to convert netlist to circuit: {}", e))?;
        
        // Create DC analysis
        let mut dc_analysis = DcAnalysis::new(circuit.clone());
        
        // Add voltage source models
        for (power_net, voltage) in &power_info {
            let source_name = format!("V_{}", power_net);
            dc_analysis.add_model(source_name, ComponentModel::VoltageSource {
                voltage: *voltage,
                internal_resistance: None,
            });
        }
        
        // Add component models
        let models_added = self.add_component_models(&mut dc_analysis, netlist);
        debug!("Added {} component models", models_added);
        
        // Run DC analysis
        let dc_result = dc_analysis.analyze()
            .context("DC analysis failed")?;
        
        // Run SPICE safety analysis
        let spice_config = SpiceSafetyConfig {
            derating_factors: bhdl_spice::safety::engine::DeratingFactors {
                voltage: self.config.voltage_derating,
                current: self.config.current_derating,
                power: self.config.power_derating,
                temperature: 0.8,
            },
            ..Default::default()
        };
        
        let engine = SafetyAnalysisEngine::new(spice_config);
        let safety_result = engine.analyze(dc_analysis.circuit(), Some(&dc_result));
        
        // Convert SPICE violations to our format
        let violations = self.convert_spice_violations(&safety_result, netlist);
        
        Ok(violations)
    }
    
    /// Add component models from netlist
    fn add_component_models(&self, dc_analysis: &mut DcAnalysis, netlist: &Netlist) -> usize {
        let mut count = 0;
        
        for (_id, instance) in &netlist.instances {
            if let Some(module) = netlist.modules.get(instance.definition) {
                let model = self.create_component_model(&module.name, &instance.name);
                
                if let Some(model) = model {
                    dc_analysis.add_model(instance.name.clone(), model);
                    count += 1;
                }
            }
        }
        
        count
    }
    
    /// Create a component model
    fn create_component_model(&self, module_name: &str, instance_name: &str) -> Option<ComponentModel> {
        // TODO: Query component database if available
        
        // Use defaults based on type
        match module_name {
            "Res" | "Resistor" => {
                Some(ComponentModel::Resistor {
                    resistance: 1000.0, // Default 1k
                    tolerance: 5.0,
                    limits: ElectricalLimits {
                        max_power: Some(0.25), // 1/4W default
                        ..Default::default()
                    },
                })
            }
            "LED" => {
                Some(ComponentModel::LED {
                    color: "red".to_string(),
                    forward_voltage: 2.0,
                    forward_current: 0.02,
                    dynamic_resistance: 10.0,
                    saturation_current: Some(1e-12),
                    emission_coefficient: Some(2.0),
                    thermal_voltage: Some(0.026),
                    limits: ElectricalLimits {
                        max_current: Some(0.030),
                        max_voltage: Some(3.3),
                        max_power: Some(0.1),
                        ..Default::default()
                    },
                })
            }
            "Cap" | "Capacitor" => {
                Some(ComponentModel::Capacitor {
                    capacitance: 1e-6, // 1µF default
                    esr: None, // ESR not specified
                    limits: ElectricalLimits {
                        max_voltage: Some(50.0), // 50V default
                        ..Default::default()
                    },
                })
            }
            _ => {
                debug!("No model for component type: {} ({})", module_name, instance_name);
                None
            }
        }
    }
    
    /// Convert SPICE violations to our format
    fn convert_spice_violations(
        &self, 
        spice_result: &bhdl_spice::safety::SafetyAnalysisResult,
        _netlist: &Netlist,
    ) -> Vec<SafetyViolation> {
        let mut violations = Vec::new();
        
        for spice_violation in &spice_result.violations {
            let severity = match spice_violation.severity {
                bhdl_spice::Severity::Info => Severity::Info,
                bhdl_spice::Severity::Warning => Severity::Warning,
                bhdl_spice::Severity::Error => Severity::Error,
                bhdl_spice::Severity::Critical => Severity::Critical,
            };
            
            // Extract component info
            let component_name = if let Some(_comp_id) = spice_violation.location.components.first() {
                spice_violation.location.description.clone()
            } else {
                "Unknown".to_string()
            };
            
            // Determine violation type from message
            let violation_type = if spice_violation.message.contains("current") && spice_violation.message.contains("exceeds") {
                // Try to parse values from technical details
                ViolationType::Overcurrent {
                    actual: 0.0, // Would parse from message
                    limit: 0.0,
                    component: component_name.clone(),
                }
            } else if spice_violation.message.contains("voltage") && spice_violation.message.contains("exceeds") {
                ViolationType::Overvoltage {
                    actual: 0.0,
                    limit: 0.0,
                    component: component_name.clone(),
                }
            } else {
                ViolationType::MissingProtection {
                    component: component_name.clone(),
                    protection_type: "Unknown".to_string(),
                }
            };
            
            let mut violation = SafetyViolation::new(
                severity,
                violation_type,
                spice_violation.message.clone(),
                spice_violation.technical_details.clone(),
            );
            
            // Add suggested fix if available
            if let Some((_, modification)) = spice_result.suggested_fixes.iter()
                .find(|(v, _)| v.message == spice_violation.message) {
                let fix = format!("{:?}", modification); // Simple for now
                violation = violation.with_fix(fix);
            }
            
            // Add location
            violation = violation.with_location(ComponentLocation {
                instance_name: component_name,
                component_type: "Unknown".to_string(), // Would look up in netlist
                nets: Vec::new(),
            });
            
            violations.push(violation);
        }
        
        violations
    }
    
    /// Run heuristic safety checks that don't require DC analysis
    fn run_heuristic_checks(&self, netlist: &Netlist) -> Vec<SafetyViolation> {
        let mut violations = Vec::new();
        
        // Check 1: Direct LED connections to power
        for (_id, instance) in &netlist.instances {
            if let Some(module) = netlist.modules.get(instance.definition) {
                if module.name == "LED" {
                    // Check if LED is directly connected to a power net
                    if self.is_directly_connected_to_power(instance, netlist) {
                        violations.push(SafetyViolation::new(
                            Severity::Error,
                            ViolationType::MissingProtection {
                                component: instance.name.clone(),
                                protection_type: "Current Limiting".to_string(),
                            },
                            format!("LED {} appears to be directly connected to power without current limiting", instance.name),
                            "LEDs require current limiting resistors to prevent damage".to_string(),
                        ).with_fix("Add a 220Ω-470Ω resistor in series with the LED".to_string()));
                    }
                }
            }
        }
        
        violations
    }
    
    /// Check if a component is directly connected to power
    fn is_directly_connected_to_power(&self, _instance: &bhdl_netlist::Instance, netlist: &Netlist) -> bool {
        // This is a heuristic - check if any net connected to this instance
        // is named like a power net
        for (_net_id, net) in &netlist.nets {
            if let Some(name) = &net.name {
                if name.contains("VCC") || name.contains("VDD") || name.ends_with("V") {
                    // Check if this instance is connected to this net
                    // (This is simplified - would need proper connection checking)
                    return true;
                }
            }
        }
        false
    }
}