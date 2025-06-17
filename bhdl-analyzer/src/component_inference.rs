//! Component inference engine for BHDL circuit flow paradigm
//!
//! This module implements intelligent component selection and parameter inference
//! based on circuit requirements and electrical constraints.

use crate::types::SourceLocation;
use std::collections::HashMap;
use std::fmt;

/// Component parameter that can be inferred
#[derive(Debug, Clone, PartialEq)]
pub struct InferredParameter {
    pub name: String,
    pub value: ParameterValue,
    pub confidence: f64, // 0.0 to 1.0
    pub reasoning: String,
}

/// Parameter value types
#[derive(Debug, Clone, PartialEq)]
pub enum ParameterValue {
    Resistance(f64),       // Ohms
    Capacitance(f64),      // Farads
    Inductance(f64),       // Henrys
    Voltage(f64),          // Volts
    Current(f64),          // Amperes
    Frequency(f64),        // Hertz
    Power(f64),            // Watts
    String(String),        // Text values
    Integer(i64),          // Numeric values
    Real(f64),             // Floating point values
    Boolean(bool),         // True/false
}

impl fmt::Display for ParameterValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParameterValue::Resistance(r) => write!(f, "{}Ω", format_electrical_value(*r)),
            ParameterValue::Capacitance(c) => write!(f, "{}F", format_electrical_value(*c)),
            ParameterValue::Inductance(l) => write!(f, "{}H", format_electrical_value(*l)),
            ParameterValue::Voltage(v) => write!(f, "{}V", v),
            ParameterValue::Current(i) => write!(f, "{}A", format_electrical_value(*i)),
            ParameterValue::Frequency(freq) => write!(f, "{}Hz", format_electrical_value(*freq)),
            ParameterValue::Power(p) => write!(f, "{}W", format_electrical_value(*p)),
            ParameterValue::String(s) => write!(f, "\"{}\"", s),
            ParameterValue::Integer(i) => write!(f, "{}", i),
            ParameterValue::Real(r) => write!(f, "{}", r),
            ParameterValue::Boolean(b) => write!(f, "{}", b),
        }
    }
}

/// Format electrical values with appropriate prefixes (k, M, μ, n, p)
fn format_electrical_value(value: f64) -> String {
    if value >= 1e6 {
        format!("{:.1}M", value / 1e6)
    } else if value >= 1e3 {
        format!("{:.1}k", value / 1e3)
    } else if value >= 1.0 {
        format!("{:.1}", value)
    } else if value >= 1e-3 {
        format!("{:.1}m", value * 1e3)
    } else if value >= 1e-6 {
        format!("{:.1}μ", value * 1e6)
    } else if value >= 1e-9 {
        format!("{:.1}n", value * 1e9)
    } else {
        format!("{:.1}p", value * 1e12)
    }
}

/// Circuit requirements for component inference
#[derive(Debug, Clone, Default)]
pub struct CircuitRequirements {
    /// Supply voltage
    pub supply_voltage: Option<f64>,
    /// Load current
    pub load_current: Option<f64>,
    /// Required current
    pub required_current: Option<f64>,
    /// Operating frequency
    pub frequency: Option<f64>,
    /// Power dissipation constraints
    pub max_power: Option<f64>,
    /// Temperature range
    pub temperature_range: Option<(f64, f64)>,
    /// Tolerance requirements
    pub tolerance: Option<f64>,
    /// Package constraints
    pub package_constraint: Option<String>,
}

/// Inferred component suggestion
#[derive(Debug, Clone)]
pub struct ComponentSuggestion {
    pub component_type: String,
    pub instance_name: Option<String>,  // Instance name from source (e.g., "D1", "U1")
    pub part_number: Option<String>,
    pub parameters: Vec<InferredParameter>,
    pub reasoning: String,
    pub confidence: f64,
    pub alternatives: Vec<String>,
}

/// Component inference context
#[derive(Debug)]
pub struct ComponentInferenceContext {
    /// Component database
    pub component_db: HashMap<String, ComponentTypeInfo>,
    /// Inference results
    pub inferred_components: Vec<ComponentSuggestion>,
    /// Inference warnings
    pub warnings: Vec<String>,
    /// Design rules
    pub design_rules: DesignRules,
    /// Module resolver for component libraries
    pub module_resolver: Option<crate::component_library::ModuleResolver>,
}

/// Component type information for inference
#[derive(Debug, Clone)]
pub struct ComponentTypeInfo {
    pub name: String,
    pub category: ComponentCategory,
    pub parameters: Vec<ParameterConstraint>,
    pub electrical_model: Option<ElectricalModel>,
    pub inference_rules: Vec<InferenceRule>,
}

/// Component categories for classification
#[derive(Debug, Clone, PartialEq)]
pub enum ComponentCategory {
    Passive,      // R, L, C
    Active,       // Transistors, diodes
    IC,           // Integrated circuits
    Connector,    // Headers, connectors
    Mechanical,   // Switches, relays
    Power,        // Regulators, converters
    Crystal,      // Crystals, oscillators
    Protection,   // Fuses, TVS diodes, protection devices
    PowerManagement, // Voltage regulators, DC-DC converters
}

/// Parameter constraints for inference
#[derive(Debug, Clone)]
pub struct ParameterConstraint {
    pub name: String,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub preferred_values: Vec<f64>, // E-series values for passives
    pub tolerance_options: Vec<f64>,
}

/// Electrical model for component behavior
#[derive(Debug, Clone)]
pub struct ElectricalModel {
    pub model_type: ModelType,
    pub parameters: HashMap<String, f64>,
}

#[derive(Debug, Clone)]
pub enum ModelType {
    Resistor,
    Capacitor,
    Inductor,
    Diode,
    BJT,
    MOSFET,
    OpAmp,
}

/// Inference rules for automatic parameter calculation
#[derive(Debug, Clone)]
pub struct InferenceRule {
    pub rule_type: InferenceRuleType,
    pub condition: String,
    pub formula: String,
    pub confidence: f64,
}

#[derive(Debug, Clone)]
pub enum InferenceRuleType {
    ResistorFromOhmsLaw,    // R = V / I
    CapacitorFromTimeConstant, // C = τ / R
    LEDCurrentLimiting,     // R = (Vcc - Vf) / If
    DecouplingCapacitor,    // C based on current and ripple
    PullUpResistor,         // R based on logic levels and speed
    CrystalLoadCapacitor,   // C based on crystal specs
    FuseFromMaxCurrent,     // I_fuse = I_max * 1.25
    TVSFromSupplyVoltage,   // V_tvs = V_supply * 1.2
    ElectrolyticFromRipple, // C = I_ripple / (f * V_ripple)
}

/// Design rules for component selection
#[derive(Debug, Clone)]
pub struct DesignRules {
    pub preferred_tolerances: Vec<f64>,
    pub preferred_packages: Vec<String>,
    pub power_derating_factor: f64,
    pub temperature_derating: f64,
    pub voltage_derating_factor: f64,
}

impl Default for DesignRules {
    fn default() -> Self {
        Self {
            preferred_tolerances: vec![1.0, 5.0, 10.0], // 1%, 5%, 10%
            preferred_packages: vec!["0603".to_string(), "0805".to_string()],
            power_derating_factor: 0.7, // 70% power derating
            temperature_derating: 0.8,   // 80% at max temp
            voltage_derating_factor: 0.8, // 80% voltage derating
        }
    }
}

impl ComponentInferenceContext {
    /// Create a new component inference context
    pub fn new() -> Self {
        let mut context = Self {
            component_db: HashMap::new(),
            inferred_components: Vec::new(),
            warnings: Vec::new(),
            design_rules: DesignRules::default(),
            module_resolver: None,
        };
        
        context.populate_component_database();
        context
    }
    
    /// Set the module resolver
    pub fn set_module_resolver(&mut self, resolver: crate::component_library::ModuleResolver) {
        self.module_resolver = Some(resolver);
    }
    
    /// Populate the component database with standard components
    fn populate_component_database(&mut self) {
        // Resistor
        let resistor_info = ComponentTypeInfo {
            name: "Res".to_string(),
            category: ComponentCategory::Passive,
            parameters: vec![
                ParameterConstraint {
                    name: "value".to_string(),
                    min_value: Some(1.0),
                    max_value: Some(10e6),
                    preferred_values: generate_e_series_values(12), // E12 series
                    tolerance_options: vec![1.0, 5.0, 10.0],
                }
            ],
            electrical_model: Some(ElectricalModel {
                model_type: ModelType::Resistor,
                parameters: HashMap::new(),
            }),
            inference_rules: vec![
                InferenceRule {
                    rule_type: InferenceRuleType::ResistorFromOhmsLaw,
                    condition: "voltage and current known".to_string(),
                    formula: "R = V / I".to_string(),
                    confidence: 0.9,
                },
                InferenceRule {
                    rule_type: InferenceRuleType::LEDCurrentLimiting,
                    condition: "LED in circuit".to_string(),
                    formula: "R = (Vcc - Vf) / If".to_string(),
                    confidence: 0.95,
                },
                InferenceRule {
                    rule_type: InferenceRuleType::PullUpResistor,
                    condition: "digital input pin".to_string(),
                    formula: "R = Vcc / (Vol * 10)".to_string(),
                    confidence: 0.8,
                },
            ],
        };
        self.component_db.insert("Res".to_string(), resistor_info);
        
        // Capacitor
        let capacitor_info = ComponentTypeInfo {
            name: "Cap".to_string(),
            category: ComponentCategory::Passive,
            parameters: vec![
                ParameterConstraint {
                    name: "value".to_string(),
                    min_value: Some(1e-12),
                    max_value: Some(1e-3),
                    preferred_values: generate_capacitor_values(),
                    tolerance_options: vec![5.0, 10.0, 20.0],
                }
            ],
            electrical_model: Some(ElectricalModel {
                model_type: ModelType::Capacitor,
                parameters: HashMap::new(),
            }),
            inference_rules: vec![
                InferenceRule {
                    rule_type: InferenceRuleType::CapacitorFromTimeConstant,
                    condition: "RC time constant needed".to_string(),
                    formula: "C = τ / R".to_string(),
                    confidence: 0.85,
                },
                InferenceRule {
                    rule_type: InferenceRuleType::DecouplingCapacitor,
                    condition: "power supply decoupling".to_string(),
                    formula: "C = I * dt / dV".to_string(),
                    confidence: 0.9,
                },
            ],
        };
        self.component_db.insert("Cap".to_string(), capacitor_info);
        
        // LED
        let led_info = ComponentTypeInfo {
            name: "LED".to_string(),
            category: ComponentCategory::Active,
            parameters: vec![
                ParameterConstraint {
                    name: "color".to_string(),
                    min_value: None,
                    max_value: None,
                    preferred_values: vec![],
                    tolerance_options: vec![],
                }
            ],
            electrical_model: Some(ElectricalModel {
                model_type: ModelType::Diode,
                parameters: [
                    ("Vf_red".to_string(), 2.0),
                    ("Vf_green".to_string(), 2.2),
                    ("Vf_blue".to_string(), 3.2),
                    ("If_typical".to_string(), 0.02), // 20mA
                ].iter().cloned().collect(),
            }),
            inference_rules: vec![],
        };
        self.component_db.insert("LED".to_string(), led_info);
        
        // Fuse
        let fuse_info = ComponentTypeInfo {
            name: "Fuse".to_string(),
            category: ComponentCategory::Protection,
            parameters: vec![
                ParameterConstraint {
                    name: "current_rating".to_string(),
                    min_value: Some(0.1),
                    max_value: Some(20.0),
                    preferred_values: vec![0.5, 1.0, 2.0, 3.0, 5.0, 10.0],
                    tolerance_options: vec![],
                }
            ],
            electrical_model: None,
            inference_rules: vec![
                InferenceRule {
                    rule_type: InferenceRuleType::FuseFromMaxCurrent,
                    condition: "maximum circuit current known".to_string(),
                    formula: "I_fuse = I_max * 1.25".to_string(),
                    confidence: 0.9,
                },
            ],
        };
        self.component_db.insert("Fuse".to_string(), fuse_info);
        
        // TVS Diode
        let tvs_info = ComponentTypeInfo {
            name: "TVSDiode".to_string(),
            category: ComponentCategory::Protection,
            parameters: vec![
                ParameterConstraint {
                    name: "voltage".to_string(),
                    min_value: Some(3.3),
                    max_value: Some(600.0),
                    preferred_values: vec![5.0, 12.0, 15.0, 24.0, 36.0, 48.0],
                    tolerance_options: vec![5.0, 10.0],
                }
            ],
            electrical_model: None,
            inference_rules: vec![
                InferenceRule {
                    rule_type: InferenceRuleType::TVSFromSupplyVoltage,
                    condition: "supply voltage known".to_string(),
                    formula: "V_tvs = V_supply * 1.2".to_string(),
                    confidence: 0.85,
                },
            ],
        };
        self.component_db.insert("TVSDiode".to_string(), tvs_info);
        
        // ElectrolyticCap (already handled by Cap, but add specific entry)
        let electrolytic_info = ComponentTypeInfo {
            name: "ElectrolyticCap".to_string(),
            category: ComponentCategory::Passive,
            parameters: vec![
                ParameterConstraint {
                    name: "value".to_string(),
                    min_value: Some(0.1e-6),
                    max_value: Some(10000e-6),
                    preferred_values: vec![1e-6, 10e-6, 22e-6, 47e-6, 100e-6, 220e-6, 470e-6, 1000e-6],
                    tolerance_options: vec![20.0],
                },
                ParameterConstraint {
                    name: "voltage".to_string(),
                    min_value: Some(6.3),
                    max_value: Some(450.0),
                    preferred_values: vec![6.3, 10.0, 16.0, 25.0, 35.0, 50.0, 63.0],
                    tolerance_options: vec![],
                }
            ],
            electrical_model: Some(ElectricalModel {
                model_type: ModelType::Capacitor,
                parameters: HashMap::new(),
            }),
            inference_rules: vec![
                InferenceRule {
                    rule_type: InferenceRuleType::ElectrolyticFromRipple,
                    condition: "ripple current and voltage known".to_string(),
                    formula: "C = I_ripple / (f * V_ripple)".to_string(),
                    confidence: 0.8,
                },
            ],
        };
        self.component_db.insert("ElectrolyticCap".to_string(), electrolytic_info);
        
        // LM7805 Linear Regulator
        let lm7805_info = ComponentTypeInfo {
            name: "LM7805".to_string(),
            category: ComponentCategory::PowerManagement,
            parameters: vec![],  // No parameters needed for instantiation
            electrical_model: None,
            inference_rules: vec![],
        };
        self.component_db.insert("LM7805".to_string(), lm7805_info);
        
        // TestPoint
        let testpoint_info = ComponentTypeInfo {
            name: "TestPoint".to_string(),
            category: ComponentCategory::Connector,
            parameters: vec![],
            electrical_model: None,
            inference_rules: vec![],
        };
        self.component_db.insert("TestPoint".to_string(), testpoint_info);
    }
    
    /// Infer component parameters based on circuit context
    pub fn infer_component_parameters(
        &mut self,
        component_type: &str,
        requirements: &CircuitRequirements,
        circuit_context: &CircuitContext,
    ) -> Option<ComponentSuggestion> {
        // First, try to resolve through component library if available
        if let Some(resolver) = &mut self.module_resolver {
            if let Ok(module) = resolver.resolve(component_type) {
                return self.create_suggestion_from_module(component_type, &module, requirements, circuit_context);
            }
        }
        
        // Fallback to hardcoded inference if we have the component in our database
        if let Some(component_info) = self.component_db.get(component_type).cloned() {
            match component_type {
                "Res" => return self.infer_resistor_parameters(requirements, circuit_context),
                "Cap" => return self.infer_capacitor_parameters(requirements, circuit_context),
                "LED" => return self.infer_led_parameters(requirements, circuit_context),
                _ => {
                    // For known components without specific inference, create generic suggestion
                    return self.create_generic_suggestion(component_type, &component_info, requirements, circuit_context);
                }
            }
        }
        
        // Check if this looks like an IC part number (alphanumeric, possibly with digits)
        if component_type.chars().any(|c| c.is_digit(10)) || 
           component_type.starts_with("LM") || 
           component_type.starts_with("NE") ||
           component_type.starts_with("TL") ||
           component_type.starts_with("MAX") ||
           component_type.starts_with("STM") {
            // This is likely an IC - no inference needed, just pass through
            return Some(ComponentSuggestion {
                component_type: component_type.to_string(),
                instance_name: None,
                part_number: Some(component_type.to_string()),
                parameters: vec![],
                reasoning: "IC component - using exact part number".to_string(),
                confidence: 1.0,
                alternatives: vec![],
            });
        }
        
        // For unknown components, create a generic component suggestion
        self.create_unknown_component_suggestion(component_type, requirements, circuit_context)
    }
    
    /// Infer resistor parameters
    fn infer_resistor_parameters(
        &mut self,
        requirements: &CircuitRequirements,
        context: &CircuitContext,
    ) -> Option<ComponentSuggestion> {
        let mut parameters = Vec::new();
        let mut reasoning = String::new();
        let mut confidence = 0.8;
        
        // Check if this is for LED current limiting
        if context.has_led_in_series {
            if let (Some(supply_v), Some(led_color)) = (requirements.supply_voltage, &context.led_color) {
                // Try to get LED parameters from module resolver (stdlib)
                let (vf, if_target) = if let Some(resolver) = &mut self.module_resolver {
                    // Try to resolve LED module to get parameters
                    if let Ok(led_module) = resolver.resolve("LED") {
                        // Extract forward voltage and current from electrical specs
                        let vf = led_module.metadata.electrical_specs.get(&format!("{}_forward_voltage", led_color))
                            .and_then(|v| v.parse::<f64>().ok())
                            .unwrap_or_else(|| match led_color.as_str() {
                                "red" => 2.0,
                                "green" => 2.2,
                                "blue" => 3.2,
                                "yellow" => 2.1,
                                "white" => 3.3,
                                _ => 2.0,
                            });
                        let if_target = led_module.metadata.electrical_specs.get(&format!("{}_forward_current", led_color))
                            .and_then(|v| v.parse::<f64>().ok())
                            .unwrap_or(0.020);
                        (vf, if_target)
                    } else {
                        // Fallback to defaults if LED module not found
                        let vf = match led_color.as_str() {
                            "red" => 2.0,
                            "green" => 2.2,
                            "blue" => 3.2,
                            "yellow" => 2.1,
                            "white" => 3.3,
                            _ => 2.0,
                        };
                        (vf, 0.020)
                    }
                } else {
                    // No module resolver, use defaults
                    let vf = match led_color.as_str() {
                        "red" => 2.0,
                        "green" => 2.2,
                        "blue" => 3.2,
                        "yellow" => 2.1,
                        "white" => 3.3,
                        _ => 2.0,
                    };
                    (vf, 0.020)
                };
                let resistance = (supply_v - vf) / if_target;
                let resistance_standard = find_nearest_e_series_value(resistance, 12);
                
                parameters.push(InferredParameter {
                    name: "value".to_string(),
                    value: ParameterValue::Resistance(resistance_standard),
                    confidence: 0.95,
                    reasoning: format!("Current limiting for {} LED: R = ({:.1}V - {:.1}V) / {:.3}A = {:.0}Ω", 
                                     led_color, supply_v, vf, if_target, resistance),
                });
                
                reasoning = format!("LED current limiting resistor");
                confidence = 0.95;
                
                // Check if voltage drop is too small
                let voltage_drop = supply_v - vf;
                if voltage_drop < 0.5 {
                    self.warnings.push(format!(
                        "Very small voltage drop ({:.1}V) across current limiting resistor for {} LED. Consider using a higher supply voltage or different LED color.",
                        voltage_drop, led_color
                    ));
                }
                
                // Calculate power dissipation
                let power = voltage_drop * if_target;
                if power > 0.25 * self.design_rules.power_derating_factor {
                    self.warnings.push(format!(
                        "High power dissipation ({:.2}W) in current limiting resistor. Consider higher power rating.",
                        power
                    ));
                }
            }
        }
        // Check if this is for pull-up
        else if context.is_pullup {
            if let Some(supply_v) = requirements.supply_voltage {
                // For pull-up resistors, typical values are 1k-100kΩ
                // Higher values for lower power, lower values for faster switching
                let resistance = if context.high_speed_signal {
                    1000.0 // 1kΩ for high speed
                } else {
                    10000.0 // 10kΩ for normal operation
                };
                
                let resistance_standard = find_nearest_e_series_value(resistance, 12);
                
                parameters.push(InferredParameter {
                    name: "value".to_string(),
                    value: ParameterValue::Resistance(resistance_standard),
                    confidence: 0.8,
                    reasoning: format!("Pull-up resistor for {}V logic", supply_v),
                });
                
                reasoning = "Digital pull-up resistor".to_string();
                confidence = 0.8;
            }
        }
        // Ohm's law inference
        else if let (Some(voltage), Some(current)) = (requirements.supply_voltage, requirements.required_current) {
            let resistance = voltage / current;
            let resistance_standard = find_nearest_e_series_value(resistance, 12);
            
            parameters.push(InferredParameter {
                name: "value".to_string(),
                value: ParameterValue::Resistance(resistance_standard),
                confidence: 0.9,
                reasoning: format!("Ohm's law: R = {:.1}V / {:.3}A = {:.0}Ω", voltage, current, resistance),
            });
            
            reasoning = "Resistance from voltage and current requirements".to_string();
            confidence = 0.9;
        }
        
        if !parameters.is_empty() {
            Some(ComponentSuggestion {
                component_type: "Res".to_string(),
                instance_name: None,
                part_number: None,
                parameters,
                reasoning,
                confidence,
                alternatives: vec![
                    "Consider 1% tolerance for precision applications".to_string(),
                    "Use 0.5W or higher power rating for safety".to_string(),
                ],
            })
        } else {
            None
        }
    }
    
    /// Infer capacitor parameters  
    fn infer_capacitor_parameters(
        &mut self,
        requirements: &CircuitRequirements,
        context: &CircuitContext,
    ) -> Option<ComponentSuggestion> {
        let mut parameters = Vec::new();
        let mut reasoning = String::new();
        let mut confidence = 0.7;
        
        // Decoupling capacitor
        if context.is_decoupling {
            let capacitance = if context.high_frequency {
                100e-9 // 100nF for high frequency decoupling
            } else {
                10e-6 // 10µF for bulk decoupling
            };
            
            parameters.push(InferredParameter {
                name: "value".to_string(),
                value: ParameterValue::Capacitance(capacitance),
                confidence: 0.85,
                reasoning: if context.high_frequency {
                    "High frequency decoupling capacitor".to_string()
                } else {
                    "Bulk decoupling capacitor".to_string()
                },
            });
            
            reasoning = "Power supply decoupling".to_string();
            confidence = 0.85;
        }
        // Crystal load capacitor
        else if context.is_crystal_load {
            let capacitance = 22e-12; // 22pF typical
            
            parameters.push(InferredParameter {
                name: "value".to_string(),
                value: ParameterValue::Capacitance(capacitance),
                confidence: 0.9,
                reasoning: "Crystal load capacitor".to_string(),
            });
            
            reasoning = "Crystal oscillator load capacitor".to_string();
            confidence = 0.9;
        }
        
        if !parameters.is_empty() {
            Some(ComponentSuggestion {
                component_type: "Cap".to_string(),
                instance_name: None,
                part_number: None,
                parameters,
                reasoning,
                confidence,
                alternatives: vec![
                    "Consider ceramic (X7R) for decoupling".to_string(),
                    "Use electrolytic for bulk capacitance".to_string(),
                ],
            })
        } else {
            // For generic capacitors without specific context, return a generic suggestion
            parameters.push(InferredParameter {
                name: "value".to_string(),
                value: ParameterValue::Capacitance(100e-9), // Default 100nF
                confidence: 0.5,
                reasoning: "Generic capacitor - value may need adjustment".to_string(),
            });
            
            Some(ComponentSuggestion {
                component_type: "Cap".to_string(),
                instance_name: None,
                part_number: None,
                parameters,
                reasoning: "Generic capacitor".to_string(),
                confidence: 0.5,
                alternatives: vec![
                    "Consider specific value based on circuit requirements".to_string(),
                    "Use ceramic (X7R/X5R) for general purpose".to_string(),
                ],
            })
        }
    }
    
    /// Infer LED parameters
    fn infer_led_parameters(
        &mut self,
        _requirements: &CircuitRequirements,
        context: &CircuitContext,
    ) -> Option<ComponentSuggestion> {
        let mut parameters = Vec::new();
        
        // Infer color if not specified
        let color = context.led_color.clone().unwrap_or_else(|| {
            if context.is_status_indicator {
                "green".to_string()
            } else if context.is_error_indicator {
                "red".to_string()
            } else {
                "red".to_string() // Default
            }
        });
        
        parameters.push(InferredParameter {
            name: "color".to_string(),
            value: ParameterValue::String(color.clone()),
            confidence: 0.7,
            reasoning: if context.is_status_indicator {
                "Green for status indication".to_string()
            } else if context.is_error_indicator {
                "Red for error indication".to_string()
            } else {
                "Default color selection".to_string()
            },
        });
        
        Some(ComponentSuggestion {
            component_type: "LED".to_string(),
            instance_name: None,
            part_number: None,
            parameters,
            reasoning: "LED color inference based on application".to_string(),
            confidence: 0.7,
            alternatives: vec![
                "Consider RGB LED for multiple states".to_string(),
                "Use high-brightness LED for visibility".to_string(),
            ],
        })
    }
    
    /// Create a generic suggestion for known components without specific inference
    fn create_generic_suggestion(
        &mut self,
        component_type: &str,
        component_info: &ComponentTypeInfo,
        _requirements: &CircuitRequirements,
        _circuit_context: &CircuitContext,
    ) -> Option<ComponentSuggestion> {
        let mut parameters = Vec::new();
        
        // Add default parameters from component info
        for constraint in &component_info.parameters {
            if let Some(default) = constraint.preferred_values.first() {
                parameters.push(InferredParameter {
                    name: constraint.name.clone(),
                    value: match component_info.category {
                        ComponentCategory::Passive => {
                            match component_type {
                                "Res" => ParameterValue::Resistance(*default),
                                "Cap" => ParameterValue::Capacitance(*default),
                                "Ind" => ParameterValue::Inductance(*default),
                                _ => ParameterValue::Real(*default),
                            }
                        }
                        _ => ParameterValue::Real(*default),
                    },
                    confidence: 0.5,
                    reasoning: "Default value for component type".to_string(),
                });
            }
        }
        
        Some(ComponentSuggestion {
            component_type: component_type.to_string(),
            instance_name: None,
            part_number: None,
            parameters,
            reasoning: format!("Generic {} component", component_type),
            confidence: 0.5,
            alternatives: vec![],
        })
    }
    
    /// Create a suggestion for unknown components
    fn create_unknown_component_suggestion(
        &mut self,
        component_type: &str,
        _requirements: &CircuitRequirements,
        _circuit_context: &CircuitContext,
    ) -> Option<ComponentSuggestion> {
        self.warnings.push(format!(
            "Unknown component type '{}' - no inference rules available",
            component_type
        ));
        
        Some(ComponentSuggestion {
            component_type: component_type.to_string(),
            instance_name: None,
            part_number: None,
            parameters: vec![],
            reasoning: format!("Unknown component type '{}'", component_type),
            confidence: 0.1,
            alternatives: vec![
                "Check component library for module definition".to_string(),
                "Verify component type name is correct".to_string(),
            ],
        })
    }
    
    /// Add an inferred component
    pub fn add_inferred_component(&mut self, suggestion: ComponentSuggestion) {
        self.inferred_components.push(suggestion);
    }
    
    /// Get all inferred components
    pub fn get_inferred_components(&self) -> &Vec<ComponentSuggestion> {
        &self.inferred_components
    }
    
    /// Create a suggestion from a resolved component module
    fn create_suggestion_from_module(
        &mut self,
        component_type: &str,
        module: &crate::component_library::ComponentModule,
        requirements: &CircuitRequirements,
        circuit_context: &CircuitContext,
    ) -> Option<ComponentSuggestion> {
        let mut parameters = Vec::new();
        
        // Extract metadata-based parameters
        if let Some(component_class) = &module.metadata.component_class {
            parameters.push(InferredParameter {
                name: "component_class".to_string(),
                value: ParameterValue::String(component_class.clone()),
                confidence: 1.0,
                reasoning: "From component module definition".to_string(),
            });
        }
        
        // Add package information
        if let Some(default_package) = &module.metadata.default_package {
            parameters.push(InferredParameter {
                name: "package".to_string(),
                value: ParameterValue::String(default_package.clone()),
                confidence: 0.9,
                reasoning: "Default package from module".to_string(),
            });
        }
        
        // Add electrical specs
        for (key, value) in &module.metadata.electrical_specs {
            parameters.push(InferredParameter {
                name: key.clone(),
                value: ParameterValue::String(value.clone()),
                confidence: 0.95,
                reasoning: format!("Electrical spec from module: {}", key),
            });
        }
        
        // Create the suggestion
        Some(ComponentSuggestion {
            component_type: component_type.to_string(),
            instance_name: None,
            part_number: module.metadata.db_component_id.clone(),
            parameters,
            reasoning: format!("Resolved from component library module: {}", module.name),
            confidence: 0.95,
            alternatives: module.metadata.packages.clone(),
        })
    }
    
    /// Generate BHDL code for inferred components
    pub fn generate_inferred_component_code(&self) -> String {
        let mut code = String::new();
        
        if !self.inferred_components.is_empty() {
            code.push_str("// Auto-inferred component parameters\n");
            
            for suggestion in &self.inferred_components {
                code.push_str(&format!("// {}\n", suggestion.reasoning));
                code.push_str(&format!("// Confidence: {:.0}%\n", suggestion.confidence * 100.0));
                
                let params: Vec<String> = suggestion.parameters.iter()
                    .map(|p| format!("{} = {}", p.name, p.value))
                    .collect();
                
                if !params.is_empty() {
                    code.push_str(&format!("{}({})\n", suggestion.component_type, params.join(", ")));
                } else {
                    code.push_str(&format!("{}()\n", suggestion.component_type));
                }
                
                code.push('\n');
            }
        }
        
        code
    }
}

/// Circuit context for component inference
#[derive(Debug, Clone, Default)]
pub struct CircuitContext {
    pub has_led_in_series: bool,
    pub led_color: Option<String>,
    pub is_pullup: bool,
    pub is_decoupling: bool,
    pub is_crystal_load: bool,
    pub high_speed_signal: bool,
    pub high_frequency: bool,
    pub is_status_indicator: bool,
    pub is_error_indicator: bool,
}

/// Generate E-series values (E12, E24, etc.)
fn generate_e_series_values(series: u32) -> Vec<f64> {
    let base_values = match series {
        12 => vec![1.0, 1.2, 1.5, 1.8, 2.2, 2.7, 3.3, 3.9, 4.7, 5.6, 6.8, 8.2],
        24 => vec![1.0, 1.1, 1.2, 1.3, 1.5, 1.6, 1.8, 2.0, 2.2, 2.4, 2.7, 3.0, 
                   3.3, 3.6, 3.9, 4.3, 4.7, 5.1, 5.6, 6.2, 6.8, 7.5, 8.2, 9.1],
        _ => vec![1.0, 1.5, 2.2, 3.3, 4.7, 6.8], // E6 fallback
    };
    
    let mut values = Vec::new();
    for decade in 0..8 { // Cover 1Ω to 10MΩ
        let multiplier = 10_f64.powi(decade);
        for &base in &base_values {
            values.push(base * multiplier);
        }
    }
    
    values
}

/// Generate standard capacitor values
fn generate_capacitor_values() -> Vec<f64> {
    let base_values = vec![1.0, 1.5, 2.2, 3.3, 4.7, 6.8];
    let mut values = Vec::new();
    
    // From 1pF to 1mF
    for decade in -12..=-3 {
        let multiplier = 10_f64.powi(decade);
        for &base in &base_values {
            values.push(base * multiplier);
        }
    }
    
    values
}

/// Find the nearest E-series value
fn find_nearest_e_series_value(target: f64, series: u32) -> f64 {
    let values = generate_e_series_values(series);
    
    values.into_iter()
        .min_by(|a, b| (a - target).abs().partial_cmp(&(b - target).abs()).unwrap())
        .unwrap_or(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_inference_context() {
        let context = ComponentInferenceContext::new();
        
        assert!(context.component_db.contains_key("Res"));
        assert!(context.component_db.contains_key("Cap"));
        assert!(context.component_db.contains_key("LED"));
    }

    #[test]
    fn test_resistor_inference_led_current_limiting() {
        let mut context = ComponentInferenceContext::new();
        
        let requirements = CircuitRequirements {
            supply_voltage: Some(5.0),
            required_current: None,
            load_current: None,
            frequency: None,
            max_power: None,
            temperature_range: None,
            tolerance: None,
            package_constraint: None,
        };
        
        let circuit_context = CircuitContext {
            has_led_in_series: true,
            led_color: Some("red".to_string()),
            ..Default::default()
        };
        
        let suggestion = context.infer_component_parameters("Res", &requirements, &circuit_context);
        assert!(suggestion.is_some());
        
        let suggestion = suggestion.unwrap();
        assert_eq!(suggestion.component_type, "Res");
        assert_eq!(suggestion.parameters.len(), 1);
        assert_eq!(suggestion.parameters[0].name, "value");
        
        if let ParameterValue::Resistance(r) = suggestion.parameters[0].value {
            assert!(r > 100.0 && r < 200.0); // Should be around 150Ω for red LED
        }
    }

    #[test]
    fn test_capacitor_inference_decoupling() {
        let mut context = ComponentInferenceContext::new();
        
        let requirements = CircuitRequirements {
            supply_voltage: Some(3.3),
            ..Default::default()
        };
        
        let circuit_context = CircuitContext {
            is_decoupling: true,
            high_frequency: true,
            ..Default::default()
        };
        
        let suggestion = context.infer_component_parameters("Cap", &requirements, &circuit_context);
        assert!(suggestion.is_some());
        
        let suggestion = suggestion.unwrap();
        assert_eq!(suggestion.component_type, "Cap");
        assert!(suggestion.confidence > 0.8);
    }

    #[test]
    fn test_e_series_values() {
        let e12_values = generate_e_series_values(12);
        assert!(e12_values.contains(&1.0));
        assert!(e12_values.contains(&4.7));
        assert!(e12_values.contains(&47.0)); // 4.7 * 10
        
        let nearest = find_nearest_e_series_value(4.5, 12);
        assert_eq!(nearest, 4.7);
    }

    #[test]
    fn test_electrical_value_formatting() {
        assert_eq!(format_electrical_value(1000.0), "1.0k");
        assert_eq!(format_electrical_value(1500000.0), "1.5M");
        assert_eq!(format_electrical_value(0.001), "1.0m");
        assert_eq!(format_electrical_value(0.000001), "1.0μ");
    }
}