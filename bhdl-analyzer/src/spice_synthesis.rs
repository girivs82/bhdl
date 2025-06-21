//! SPICE-driven component synthesis and validation
//! 
//! This module implements the dual role of SPICE:
//! 1. Advisory: Validate user-specified values
//! 2. Generative: Calculate missing values

use std::collections::HashMap;
use anyhow::{Result, Context as _};
use log::{info, warn};
use bhdl_ast::{SourceFile, SyntaxNode, SyntaxKind};
// use bhdl_spice::{Circuit, ComponentModel, NonlinearDcAnalysis, ElectricalLimits}; // Commented out to avoid cyclic dependency
use crate::component_inference::{ComponentSuggestion, InferredParameter, ParameterValue};

/// Represents a component that needs value resolution
#[derive(Debug, Clone)]
pub struct UnresolvedComponent {
    pub instance_name: String,
    pub component_type: String,
    pub ast_node: bhdl_ast::SyntaxNode<bhdl_ast::BhdlLanguage>,
    pub is_value_specified: bool,
    pub specified_value: Option<f64>,
    pub constraints: ComponentConstraints,
    pub circuit_context: CircuitContext,
}

/// Constraints on component values
#[derive(Debug, Clone, Default)]
pub struct ComponentConstraints {
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub power_rating: Option<f64>,
    pub tolerance: Option<f64>,
    pub preferred_values: Option<Vec<f64>>,
}

/// Circuit context for component resolution
#[derive(Debug, Clone)]
pub enum CircuitContext {
    Unknown,
    LEDCurrentLimit {
        led_name: String,
        led_spec: LEDSpec,
        supply_voltage: f64,
    },
    VoltageDivider {
        target_ratio: f64,
        load_current: Option<f64>,
    },
    PullUpResistor {
        logic_high: f64,
        logic_low: f64,
        sink_current: f64,
    },
    DecouplingCapacitor {
        frequency: f64,
        ripple_current: f64,
    },
}

#[derive(Debug, Clone)]
pub struct LEDSpec {
    pub color: String,
    pub forward_voltage: f64,
    pub target_current: f64,
    pub max_current: f64,
}

/// Result of component value resolution
#[derive(Debug)]
pub struct ResolutionReport {
    pub resolutions: Vec<Resolution>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug)]
pub struct Resolution {
    pub component: String,
    pub parameter: String,
    pub calculated_value: f64,
    pub selected_value: f64,
    pub reasoning: String,
}

/// Result of circuit validation
#[derive(Debug)]
pub struct ValidationReport {
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
}

#[derive(Debug)]
pub struct ValidationError {
    pub component: String,
    pub error_type: ErrorType,
    pub message: String,
    pub severity: Severity,
    pub suggestion: Option<String>,
}

#[derive(Debug)]
pub struct ValidationWarning {
    pub component: String,
    pub message: String,
}

#[derive(Debug)]
pub enum ErrorType {
    Overcurrent,
    Overvoltage,
    Overpower,
    Thermal,
    Efficiency,
}

#[derive(Debug)]
pub enum Severity {
    Warning,
    Error,
    Critical,
}

/// Main SPICE synthesis engine
pub struct SpiceSynthesis {
    // circuit: Circuit,  // Commented out to avoid cyclic dependency
    components: Vec<UnresolvedComponent>,
    // models: HashMap<String, ComponentModel>,  // Commented out to avoid cyclic dependency
}

impl SpiceSynthesis {
    pub fn new() -> Self {
        Self {
            // circuit: Circuit::new(),  // Commented out to avoid cyclic dependency
            components: Vec::new(),
            // models: HashMap::new(),  // Commented out to avoid cyclic dependency
        }
    }
    
    /// Add an unresolved component
    pub fn add_component(&mut self, component: UnresolvedComponent) {
        self.components.push(component);
    }
    
    /// Add a SPICE model - commented out to avoid cyclic dependency
    // pub fn add_model(&mut self, name: String, model: ComponentModel) {
    //     self.models.insert(name, model);
    // }
    
    /// Resolve all component values
    pub fn resolve_components(&mut self) -> Result<ResolutionReport> {
        let mut report = ResolutionReport {
            resolutions: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        };
        
        // Process components that need value calculation
        for component in &self.components {
            if !component.is_value_specified {
                match self.resolve_component_value(component) {
                    Ok(resolution) => {
                        info!("Resolved {}: {} = {}", 
                            component.instance_name, 
                            resolution.parameter, 
                            resolution.selected_value
                        );
                        report.resolutions.push(resolution);
                    }
                    Err(e) => {
                        report.errors.push(format!(
                            "Failed to resolve {}: {}", 
                            component.instance_name, e
                        ));
                    }
                }
            }
        }
        
        Ok(report)
    }
    
    /// Resolve a single component value based on circuit context
    fn resolve_component_value(&self, component: &UnresolvedComponent) -> Result<Resolution> {
        match &component.circuit_context {
            CircuitContext::LEDCurrentLimit { led_spec, supply_voltage, .. } => {
                self.resolve_led_resistor(component, led_spec, *supply_voltage)
            }
            CircuitContext::VoltageDivider { target_ratio, load_current } => {
                self.resolve_voltage_divider(component, *target_ratio, *load_current)
            }
            _ => Err(anyhow::anyhow!(
                "Cannot resolve {} in context {:?}", 
                component.component_type, component.circuit_context
            ))
        }
    }
    
    /// Calculate LED current limiting resistor
    fn resolve_led_resistor(
        &self,
        component: &UnresolvedComponent,
        led_spec: &LEDSpec,
        supply_voltage: f64,
    ) -> Result<Resolution> {
        // Basic calculation: R = (Vsupply - Vf) / If
        let r_calculated = (supply_voltage - led_spec.forward_voltage) / led_spec.target_current;
        
        // Round to nearest E12 series value
        let r_standard = find_nearest_e12_value(r_calculated);
        
        // Verify the standard value is safe
        let actual_current = (supply_voltage - led_spec.forward_voltage) / r_standard;
        let mut final_value = r_standard;
        
        if actual_current > led_spec.max_current {
            // Need higher resistance
            final_value = find_next_higher_e12_value(r_calculated);
            warn!("Adjusted resistor value up to {} to ensure LED safety", final_value);
        }
        
        // Check power rating if specified
        if let Some(power_limit) = component.constraints.power_rating {
            let power_dissipated = actual_current * actual_current * final_value;
            if power_dissipated > power_limit * 0.7 {  // 70% derating
                warn!("Resistor power {:.3}W is close to limit {:.3}W", 
                    power_dissipated, power_limit);
            }
        }
        
        Ok(Resolution {
            component: component.instance_name.clone(),
            parameter: "value".to_string(),
            calculated_value: r_calculated,
            selected_value: final_value,
            reasoning: format!(
                "LED current limiting: ({:.1}V - {:.1}V) / {:.3}A = {:.0}Ω, selected E12: {:.0}Ω",
                supply_voltage, led_spec.forward_voltage, led_spec.target_current, 
                r_calculated, final_value
            ),
        })
    }
    
    /// Calculate voltage divider resistor values
    fn resolve_voltage_divider(
        &self,
        component: &UnresolvedComponent,
        target_ratio: f64,
        load_current: Option<f64>,
    ) -> Result<Resolution> {
        // For voltage dividers, we typically want the current through the divider
        // to be 10x the load current for good regulation
        let divider_current = load_current.unwrap_or(0.001) * 10.0;
        
        // This is a simplified calculation - in practice we'd need both R1 and R2
        let r_total = 12.0 / divider_current;  // Assuming 12V supply
        let r1 = r_total * (1.0 - target_ratio);
        
        let r_standard = find_nearest_e12_value(r1);
        
        Ok(Resolution {
            component: component.instance_name.clone(),
            parameter: "value".to_string(),
            calculated_value: r1,
            selected_value: r_standard,
            reasoning: format!(
                "Voltage divider for {:.1}% ratio: {:.0}Ω",
                target_ratio * 100.0, r_standard
            ),
        })
    }
    
    /// Validate all components with specified values
    pub fn validate_circuit(&mut self) -> Result<ValidationReport> {
        let mut report = ValidationReport {
            errors: Vec::new(),
            warnings: Vec::new(),
        };
        
        // Run DC analysis - commented out to avoid cyclic dependency
        // let mut analysis = NonlinearDcAnalysis::new(self.circuit.clone());
        // for (name, model) in &self.models {
        //     analysis.add_model(name.clone(), model.clone());
        // }
        
        // let dc_result = analysis.analyze()
        //     .context("Failed to run DC analysis for validation")?;
        let dc_result = (); // Placeholder
        
        // Check each component
        for component in &self.components {
            if component.is_value_specified {
                self.validate_component(component, &dc_result, &mut report)?;
            }
        }
        
        Ok(report)
    }
    
    /// Validate a single component
    fn validate_component(
        &self,
        component: &UnresolvedComponent,
        _dc_result: &(), // AnalysisResult is in bhdl_spice - avoid cyclic dependency
        report: &mut ValidationReport,
    ) -> Result<()> {
        match &component.circuit_context {
            CircuitContext::LEDCurrentLimit { led_name, led_spec, .. } => {
                // Find LED current in results
                // This is simplified - real implementation would trace through circuit
                // TODO: Get current from SPICE results when available
                if let Some(&current) = Some(&0.015f64) { // Placeholder 15mA
                    let current = current.abs();
                    
                    if current > led_spec.max_current {
                        report.errors.push(ValidationError {
                            component: component.instance_name.clone(),
                            error_type: ErrorType::Overcurrent,
                            message: format!(
                                "LED current {:.1}mA exceeds maximum {:.1}mA",
                                current * 1000.0,
                                led_spec.max_current * 1000.0
                            ),
                            severity: Severity::Critical,
                            suggestion: Some(format!(
                                "Increase resistance to at least {:.0}Ω",
                                component.specified_value.unwrap_or(0.0) * 
                                (current / led_spec.max_current)
                            )),
                        });
                    } else if (current - led_spec.target_current).abs() / led_spec.target_current > 0.2 {
                        report.warnings.push(ValidationWarning {
                            component: led_name.clone(),
                            message: format!(
                                "LED current {:.1}mA differs from target {:.1}mA by more than 20%",
                                current * 1000.0,
                                led_spec.target_current * 1000.0
                            ),
                        });
                    }
                }
            }
            _ => {}
        }
        
        Ok(())
    }
}

// Helper functions for E-series values
fn find_nearest_e12_value(target: f64) -> f64 {
    let e12_base = vec![1.0, 1.2, 1.5, 1.8, 2.2, 2.7, 3.3, 3.9, 4.7, 5.6, 6.8, 8.2];
    
    // Find the decade multiplier
    let decades = (target.log10().floor()) as i32;
    let multiplier = 10_f64.powi(decades);
    let normalized = target / multiplier;
    
    // Find nearest E12 value
    let nearest = e12_base.iter()
        .min_by(|&&a, &&b| {
            (a - normalized).abs().partial_cmp(&(b - normalized).abs()).unwrap()
        })
        .unwrap();
    
    nearest * multiplier
}

fn find_next_higher_e12_value(target: f64) -> f64 {
    let e12_base = vec![1.0, 1.2, 1.5, 1.8, 2.2, 2.7, 3.3, 3.9, 4.7, 5.6, 6.8, 8.2];
    
    // Find the decade multiplier
    let decades = (target.log10().floor()) as i32;
    let multiplier = 10_f64.powi(decades);
    let normalized = target / multiplier;
    
    // Find next higher E12 value
    let next_higher = e12_base.iter()
        .find(|&&v| v > normalized)
        .unwrap_or(&10.0);  // Next decade
    
    if *next_higher == 10.0 {
        1.0 * multiplier * 10.0
    } else {
        next_higher * multiplier
    }
}

impl ResolutionReport {
    pub fn to_component_suggestions(&self) -> Vec<ComponentSuggestion> {
        self.resolutions.iter().map(|res| {
            ComponentSuggestion {
                component_type: "Res".to_string(),  // For now, assuming resistors
                instance_name: Some(res.component.clone()),
                part_number: None,
                parameters: vec![
                    InferredParameter {
                        name: res.parameter.clone(),
                        value: ParameterValue::Resistance(res.selected_value),
                        confidence: 0.95,
                        reasoning: res.reasoning.clone(),
                    }
                ],
                reasoning: res.reasoning.clone(),
                confidence: 0.95,
                alternatives: vec![],
                parameter_overrides: HashMap::new(),
            }
        }).collect()
    }
}