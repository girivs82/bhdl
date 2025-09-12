// Intent-Aware Component Generator
// Generates components with appropriate design intents based on synthesis context

use std::collections::HashMap;
use anyhow::{Result, Context};
use bhdl_analyzer::types::AnalysisResult;
use bhdl_netlist::{Netlist, InstanceId, ModuleId};
use crate::synthesis_knowledge::{
    SynthesisKnowledgeEngine, SynthesizedComponent, ComponentValue, 
    SynthesisComponent, VirtualPinExpansion, MandatoryComponent
};

/// Intent-aware generator that creates components with design context
pub struct IntentAwareGenerator {
    synthesis_engine: SynthesisKnowledgeEngine,
    intent_templates: HashMap<String, IntentTemplate>,
    reference_counter: HashMap<String, usize>,
}

/// Template for generating intent strings with context
#[derive(Debug, Clone)]
pub struct IntentTemplate {
    pub intent_name: String,
    pub parameters: Vec<IntentParameter>,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct IntentParameter {
    pub name: String,
    pub source: IntentParameterSource,
    pub units: Option<String>,
    pub format: Option<String>,
}

#[derive(Debug, Clone)]
pub enum IntentParameterSource {
    UserParameter(String),           // "vout" from user
    CalculatedValue(String),         // "ripple_current" from calculation
    SimulationResult(String),        // "actual_current" from simulation
    ComponentSpecification(String),  // "switching_frequency" from component datasheet
    DesignTarget(String),           // "efficiency_target" from design goals
}

impl IntentAwareGenerator {
    pub fn new() -> Self {
        let mut generator = Self {
            synthesis_engine: SynthesisKnowledgeEngine::new(),
            intent_templates: HashMap::new(),
            reference_counter: HashMap::new(),
        };
        
        generator.initialize_intent_templates();
        generator
    }
    
    /// Initialize standard intent templates for different component functions
    fn initialize_intent_templates(&mut self) {
        // Power filtering intents
        self.intent_templates.insert("power_filtering".to_string(), IntentTemplate {
            intent_name: "power_filtering".to_string(),
            parameters: vec![
                IntentParameter {
                    name: "ripple_target".to_string(),
                    source: IntentParameterSource::DesignTarget("30%".to_string()),
                    units: Some("%".to_string()),
                    format: None,
                },
                IntentParameter {
                    name: "frequency".to_string(),
                    source: IntentParameterSource::ComponentSpecification("switching_frequency".to_string()),
                    units: Some("Hz".to_string()),
                    format: Some("engineering".to_string()),
                },
                IntentParameter {
                    name: "efficiency_priority".to_string(),
                    source: IntentParameterSource::DesignTarget("high".to_string()),
                    units: None,
                    format: None,
                }
            ],
            description: "Filter switching ripple from power supply".to_string(),
        });
        
        // Bootstrap timing intents
        self.intent_templates.insert("bootstrap_timing".to_string(), IntentTemplate {
            intent_name: "bootstrap_timing".to_string(),
            parameters: vec![
                IntentParameter {
                    name: "rise_time".to_string(),
                    source: IntentParameterSource::ComponentSpecification("gate_rise_time".to_string()),
                    units: Some("s".to_string()),
                    format: Some("engineering".to_string()),
                },
                IntentParameter {
                    name: "hold_time".to_string(),
                    source: IntentParameterSource::CalculatedValue("hold_time".to_string()),
                    units: Some("s".to_string()),
                    format: Some("engineering".to_string()),
                },
                IntentParameter {
                    name: "switching_freq".to_string(),
                    source: IntentParameterSource::ComponentSpecification("switching_frequency".to_string()),
                    units: Some("Hz".to_string()),
                    format: Some("engineering".to_string()),
                }
            ],
            description: "Provide bootstrap supply for high-side gate driver".to_string(),
        });
        
        // Feedback control intents
        self.intent_templates.insert("feedback_control".to_string(), IntentTemplate {
            intent_name: "feedback_control".to_string(),
            parameters: vec![
                IntentParameter {
                    name: "target_voltage".to_string(),
                    source: IntentParameterSource::UserParameter("vout".to_string()),
                    units: Some("V".to_string()),
                    format: None,
                },
                IntentParameter {
                    name: "accuracy".to_string(),
                    source: IntentParameterSource::ComponentSpecification("reference_accuracy".to_string()),
                    units: Some("%".to_string()),
                    format: None,
                },
                IntentParameter {
                    name: "loop_bandwidth".to_string(),
                    source: IntentParameterSource::DesignTarget("10kHz".to_string()),
                    units: Some("Hz".to_string()),
                    format: Some("engineering".to_string()),
                }
            ],
            description: "Regulate output voltage through feedback control".to_string(),
        });
        
        // Input protection intents
        self.intent_templates.insert("input_protection".to_string(), IntentTemplate {
            intent_name: "input_protection".to_string(),
            parameters: vec![
                IntentParameter {
                    name: "reverse_current".to_string(),
                    source: IntentParameterSource::DesignTarget("block".to_string()),
                    units: None,
                    format: None,
                },
                IntentParameter {
                    name: "efficiency_loss".to_string(),
                    source: IntentParameterSource::DesignTarget("minimize".to_string()),
                    units: None,
                    format: None,
                },
                IntentParameter {
                    name: "forward_voltage".to_string(),
                    source: IntentParameterSource::ComponentSpecification("forward_voltage".to_string()),
                    units: Some("V".to_string()),
                    format: None,
                }
            ],
            description: "Protect against reverse current and minimize conduction losses".to_string(),
        });
        
        // Power decoupling intents
        self.intent_templates.insert("power_decoupling".to_string(), IntentTemplate {
            intent_name: "power_decoupling".to_string(),
            parameters: vec![
                IntentParameter {
                    name: "esr_target".to_string(),
                    source: IntentParameterSource::DesignTarget("low".to_string()),
                    units: None,
                    format: None,
                },
                IntentParameter {
                    name: "ripple_reduction".to_string(),
                    source: IntentParameterSource::DesignTarget("80%".to_string()),
                    units: Some("%".to_string()),
                    format: None,
                },
                IntentParameter {
                    name: "frequency_range".to_string(),
                    source: IntentParameterSource::DesignTarget("[1kHz, 10MHz]".to_string()),
                    units: Some("Hz".to_string()),
                    format: None,
                }
            ],
            description: "Decouple power supply noise across frequency spectrum".to_string(),
        });
        
        // Soft-start timing intents
        self.intent_templates.insert("soft_start_timing".to_string(), IntentTemplate {
            intent_name: "soft_start_timing".to_string(),
            parameters: vec![
                IntentParameter {
                    name: "ramp_rate".to_string(),
                    source: IntentParameterSource::CalculatedValue("voltage_ramp_rate".to_string()),
                    units: Some("V/s".to_string()),
                    format: Some("engineering".to_string()),
                },
                IntentParameter {
                    name: "startup_time".to_string(),
                    source: IntentParameterSource::DesignTarget("5ms".to_string()),
                    units: Some("s".to_string()),
                    format: Some("engineering".to_string()),
                },
                IntentParameter {
                    name: "inrush_limit".to_string(),
                    source: IntentParameterSource::DesignTarget("prevent".to_string()),
                    units: None,
                    format: None,
                }
            ],
            description: "Control startup sequence to prevent inrush current".to_string(),
        });
    }
    
    /// Load synthesis knowledge for a component
    pub fn load_component_knowledge(&mut self, component_type: &str, stdlib_path: &str) -> Result<()> {
        self.synthesis_engine.load_component_knowledge(component_type, stdlib_path)
    }
    
    /// Generate components with intents for a given component instance
    pub fn synthesize_component_with_intents(
        &mut self,
        component_name: &str,
        component_type: &str,
        user_parameters: &HashMap<String, String>,
        analysis: &AnalysisResult,
        netlist: &mut Netlist,
    ) -> Result<Vec<IntentAwareComponent>> {
        
        println!("🎯 Intent-aware synthesis for {} ({})", component_name, component_type);
        
        // Get synthesized components from knowledge engine
        let synthesized_components = self.synthesis_engine.synthesize_component_requirements(
            component_name,
            component_type,
            user_parameters,
            Some(analysis),
        )?;
        
        let mut intent_aware_components = Vec::new();
        
        // Add intents to each synthesized component
        for synth_comp in synthesized_components {
            let intent_aware = self.create_intent_aware_component(
                &synth_comp,
                user_parameters,
                analysis,
                component_name,
            )?;
            
            println!("  📋 Generated: {} ({}) for {}", 
                    intent_aware.reference_designator,
                    intent_aware.component_type,
                    intent_aware.intent);
            
            intent_aware_components.push(intent_aware);
        }
        
        Ok(intent_aware_components)
    }
    
    /// Create an intent-aware component with full context
    fn create_intent_aware_component(
        &mut self,
        synth_comp: &SynthesizedComponent,
        user_parameters: &HashMap<String, String>,
        analysis: &AnalysisResult,
        parent_component: &str,
    ) -> Result<IntentAwareComponent> {
        
        // Generate unique reference designator
        let ref_des = self.generate_reference_designator(&synth_comp.component_type, parent_component)?;
        
        // Generate detailed intent with context
        let detailed_intent = self.generate_detailed_intent(
            &synth_comp.intent,
            user_parameters,
            analysis,
            synth_comp,
        )?;
        
        // Create comprehensive attributes including simulation data
        let mut attributes = synth_comp.attributes.clone();
        attributes.insert("synthesis_parent".to_string(), parent_component.to_string());
        attributes.insert("synthesis_method".to_string(), "intent_aware".to_string());
        attributes.insert("design_intent".to_string(), detailed_intent.clone());
        
        // Add simulation-derived attributes if available
        if let Some(operating_conditions) = self.extract_operating_conditions(analysis, &synth_comp.component_type) {
            for (key, value) in operating_conditions {
                attributes.insert(format!("sim_{}", key), value);
            }
        }
        
        Ok(IntentAwareComponent {
            reference_designator: ref_des,
            component_type: synth_comp.component_type.clone(),
            value: synth_comp.value.clone(),
            specifications: synth_comp.specifications.clone(),
            intent: detailed_intent,
            connections: synth_comp.connections.clone(),
            attributes,
            placement_info: synth_comp.placement_info.clone(),
            synthesis_context: SynthesisContext {
                parent_component: parent_component.to_string(),
                generation_reason: synth_comp.intent.clone(),
                design_parameters: user_parameters.clone(),
            },
        })
    }
    
    /// Generate detailed intent string with parameter substitution
    fn generate_detailed_intent(
        &self,
        intent_name: &str,
        user_parameters: &HashMap<String, String>,
        analysis: &AnalysisResult,
        synth_comp: &SynthesizedComponent,
    ) -> Result<String> {
        
        if let Some(template) = self.intent_templates.get(intent_name) {
            let mut intent_params = Vec::new();
            
            for param in &template.parameters {
                let value = self.resolve_intent_parameter(
                    param,
                    user_parameters,
                    analysis,
                    synth_comp,
                )?;
                
                intent_params.push(format!("{}: {}", param.name, value));
            }
            
            Ok(format!("{}({})", intent_name, intent_params.join(", ")))
        } else {
            // Fallback for unknown intent templates
            Ok(format!("{}(synthesis_generated: true)", intent_name))
        }
    }
    
    /// Resolve intent parameter value from various sources
    fn resolve_intent_parameter(
        &self,
        param: &IntentParameter,
        user_parameters: &HashMap<String, String>,
        analysis: &AnalysisResult,
        synth_comp: &SynthesizedComponent,
    ) -> Result<String> {
        
        let raw_value = match &param.source {
            IntentParameterSource::UserParameter(param_name) => {
                user_parameters.get(param_name).cloned()
                    .unwrap_or_else(|| "unknown".to_string())
            },
            IntentParameterSource::CalculatedValue(calc_name) => {
                self.get_calculated_value(calc_name, user_parameters)?
            },
            IntentParameterSource::SimulationResult(sim_param) => {
                self.get_simulation_value(sim_param, analysis)?
            },
            IntentParameterSource::ComponentSpecification(spec_name) => {
                self.get_component_specification(spec_name, &synth_comp.component_type)?
            },
            IntentParameterSource::DesignTarget(target) => {
                target.clone()
            }
        };
        
        // Apply formatting if specified
        let formatted_value = if let Some(format_type) = &param.format {
            self.apply_formatting(&raw_value, format_type)?
        } else {
            raw_value
        };
        
        // Add units if specified
        if let Some(units) = &param.units {
            Ok(formatted_value + units)
        } else {
            Ok(formatted_value)
        }
    }
    
    /// Get calculated value for intent parameter
    fn get_calculated_value(&self, calc_name: &str, user_parameters: &HashMap<String, String>) -> Result<String> {
        match calc_name {
            "ripple_current" => {
                // ΔI = 0.3 × Iout
                if let Some(iout) = user_parameters.get("output_current") {
                    let iout_val: f64 = iout.parse().unwrap_or(2.0);
                    Ok(format!("{:.1}A", 0.3 * iout_val))
                } else {
                    Ok("0.6A".to_string()) // Default for 2A output
                }
            },
            "hold_time" => {
                // Bootstrap hold time ≈ 2µs for typical switching
                Ok("2µs".to_string())
            },
            "voltage_ramp_rate" => {
                // Calculate based on soft-start capacitor and target voltage
                if let Some(vout) = user_parameters.get("vout") {
                    let vout_val: f64 = vout.replace("V", "").parse().unwrap_or(5.0);
                    Ok(format!("{:.1}V/ms", vout_val / 5.0)) // 5ms startup
                } else {
                    Ok("1V/ms".to_string())
                }
            },
            _ => Ok("unknown".to_string())
        }
    }
    
    /// Get simulation value for intent parameter
    fn get_simulation_value(&self, sim_param: &str, analysis: &AnalysisResult) -> Result<String> {
        // Extract from simulation data if available
        if let Some(ref dc_analysis) = analysis.simulation_data.dc_analysis {
            match sim_param {
                "actual_current" => {
                    // Get actual current from simulation
                    if let Some(current) = dc_analysis.branch_currents.values().next() {
                        Ok(format!("{:.3}A", current))
                    } else {
                        Ok("unknown".to_string())
                    }
                },
                "actual_voltage" => {
                    // Get actual voltage from simulation
                    if let Some(voltage) = dc_analysis.node_voltages.values().next() {
                        Ok(format!("{:.2}V", voltage))
                    } else {
                        Ok("unknown".to_string())
                    }
                },
                _ => Ok("unknown".to_string())
            }
        } else {
            Ok("no_simulation_data".to_string())
        }
    }
    
    /// Get component specification value
    fn get_component_specification(&self, spec_name: &str, component_type: &str) -> Result<String> {
        // Return known specifications for components
        match (component_type, spec_name) {
            ("TPS54331", "switching_frequency") => Ok("570kHz".to_string()),
            ("TPS54331", "reference_accuracy") => Ok("1%".to_string()),
            ("TPS54331", "gate_rise_time") => Ok("50ns".to_string()),
            ("SS34", "forward_voltage") => Ok("0.5V".to_string()),
            _ => Ok("unknown".to_string())
        }
    }
    
    /// Apply formatting to parameter values
    fn apply_formatting(&self, value: &str, format_type: &str) -> Result<String> {
        match format_type {
            "engineering" => {
                // Convert to engineering notation (k, M, µ, n, p)
                if let Ok(val) = value.replace("Hz", "").replace("s", "").parse::<f64>() {
                    if val >= 1e6 {
                        Ok(format!("{:.1}M", val / 1e6))
                    } else if val >= 1e3 {
                        Ok(format!("{:.1}k", val / 1e3))
                    } else if val >= 1.0 {
                        Ok(format!("{:.1}", val))
                    } else if val >= 1e-6 {
                        Ok(format!("{:.1}µ", val * 1e6))
                    } else if val >= 1e-9 {
                        Ok(format!("{:.1}n", val * 1e9))
                    } else {
                        Ok(format!("{:.1}p", val * 1e12))
                    }
                } else {
                    Ok(value.to_string())
                }
            },
            _ => Ok(value.to_string())
        }
    }
    
    /// Generate unique reference designator
    fn generate_reference_designator(&mut self, component_type: &str, parent: &str) -> Result<String> {
        let prefix = match component_type.to_lowercase().as_str() {
            "cap" | "capacitor" => "C",
            "res" | "resistor" => "R", 
            "inductor" => "L",
            "ss34" | "diode" => "D",
            _ => "U"
        };
        
        let key = format!("{}_{}", parent, prefix);
        let count = self.reference_counter.entry(key).or_insert(0);
        *count += 1;
        
        Ok(format!("{}{}", prefix, count))
    }
    
    /// Extract operating conditions from simulation data
    fn extract_operating_conditions(&self, analysis: &AnalysisResult, component_type: &str) -> Option<HashMap<String, String>> {
        if let Some(ref dc_analysis) = analysis.simulation_data.dc_analysis {
            let mut conditions = HashMap::new();
            
            // Add generic operating conditions
            if let Some((name, voltage)) = dc_analysis.node_voltages.iter().next() {
                conditions.insert("operating_voltage".to_string(), format!("{:.2}V", voltage));
            }
            if let Some((name, current)) = dc_analysis.branch_currents.iter().next() {
                conditions.insert("operating_current".to_string(), format!("{:.3}A", current));
            }
            
            Some(conditions)
        } else {
            None
        }
    }
}

/// Intent-aware component with full synthesis context
#[derive(Debug, Clone)]
pub struct IntentAwareComponent {
    pub reference_designator: String,
    pub component_type: String,
    pub value: String,
    pub specifications: HashMap<String, String>,
    pub intent: String,
    pub connections: Vec<crate::synthesis_knowledge::SynthesisConnection>,
    pub attributes: HashMap<String, String>,
    pub placement_info: Option<crate::synthesis_knowledge::PlacementConstraints>,
    pub synthesis_context: SynthesisContext,
}

/// Context information about how component was synthesized
#[derive(Debug, Clone)]
pub struct SynthesisContext {
    pub parent_component: String,
    pub generation_reason: String,
    pub design_parameters: HashMap<String, String>,
}