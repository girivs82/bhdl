// BHDL Intent System Core Types
// Defines the intent system data structures and simulation modes

use std::collections::HashMap;

/// Simulation mode determined by intent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SimMode {
    /// Pure digital simulation with boolean logic
    PureDigital,
    /// Digital simulation with timing information
    DigitalWithTiming,
    /// Mixed analog/digital simulation
    MixedSignal,
    /// Full analog simulation required
    AnalogRequired,
}

/// Intent parameter value
#[derive(Debug, Clone, PartialEq)]
pub enum IntentValue {
    Number(f64, Option<String>), // value and optional unit
    String(String),
    Boolean(bool),
    Identifier(String),
}

/// Intent function call with parameters
#[derive(Debug, Clone)]
pub struct IntentCall {
    pub name: String,
    pub params: Vec<IntentParam>,
}

/// Intent parameter (can be positional or named)
#[derive(Debug, Clone)]
pub enum IntentParam {
    Positional(IntentValue),
    Named(String, IntentValue),
}

/// Result of intent resolution
#[derive(Debug, Clone)]
pub struct IntentResult {
    pub sim_mode: SimMode,
    pub synthesis_hints: Vec<SynthesisHint>,
    pub validation_rules: Vec<ValidationRule>,
    pub tool_scope: ToolScope,
}

/// Hints for synthesis tools
#[derive(Debug, Clone, PartialEq)]
pub enum SynthesisHint {
    BufferChain,
    RCNetwork,
    ActiveDelay,
    DigitalFilter,
    AnalogFilter,
    Custom(String),
}

/// Validation rules from intent
#[derive(Debug, Clone)]
pub struct ValidationRule {
    pub condition: String,
    pub error_message: String,
}

/// Which tools should respect this intent
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolScope {
    All,
    SimulationOnly,
    SynthesisOnly,
    AnalysisOnly,
}

/// Intent function definition (from stdlib)
pub trait IntentFunction {
    /// Name of the intent function
    fn name(&self) -> &str;
    
    /// Resolve the intent to simulation mode and other properties
    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String>;
    
    /// Get parameter metadata for validation
    fn param_metadata(&self) -> Vec<ParamMetadata>;
}

/// Parameter metadata for intent functions
#[derive(Debug, Clone)]
pub struct ParamMetadata {
    pub name: String,
    pub param_type: ParamType,
    pub required: bool,
    pub default_value: Option<IntentValue>,
}

/// Parameter type for validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamType {
    Duration,
    Frequency,
    Voltage,
    Current,
    Component,
    String,
    Number,
    Boolean,
}

/// Registry of available intent functions
pub struct IntentRegistry {
    functions: HashMap<String, Box<dyn IntentFunction>>,
}

impl std::fmt::Debug for IntentRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IntentRegistry")
            .field("registered_functions", &self.functions.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl IntentRegistry {
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
        }
    }
    
    /// Register an intent function
    pub fn register(&mut self, function: Box<dyn IntentFunction>) {
        self.functions.insert(function.name().to_string(), function);
    }
    
    /// Resolve an intent call
    pub fn resolve(&self, call: &IntentCall) -> Result<IntentResult, String> {
        match self.functions.get(&call.name) {
            Some(function) => function.resolve(&call.params),
            None => Err(format!("Unknown intent function: {}", call.name)),
        }
    }
    
    /// Get all registered intent names
    pub fn registered_intents(&self) -> Vec<&str> {
        self.functions.keys().map(|s| s.as_str()).collect()
    }

    /// Get an intent function by name
    pub fn get(&self, name: &str) -> Option<&dyn IntentFunction> {
        self.functions.get(name).map(|b| &**b as &dyn IntentFunction)
    }
}

/// Helper to parse intent value from string
impl IntentValue {
    pub fn parse_with_unit(s: &str) -> Self {
        // Simple parser - in real implementation would be more robust
        if let Ok(num) = s.parse::<f64>() {
            IntentValue::Number(num, None)
        } else if s.starts_with('"') && s.ends_with('"') {
            IntentValue::String(s[1..s.len()-1].to_string())
        } else if s == "true" || s == "false" {
            IntentValue::Boolean(s == "true")
        } else {
            // Try to parse number with unit
            let parts: Vec<&str> = s.split_whitespace().collect();
            if parts.len() == 2 {
                if let Ok(num) = parts[0].parse::<f64>() {
                    IntentValue::Number(num, Some(parts[1].to_string()))
                } else {
                    IntentValue::Identifier(s.to_string())
                }
            } else {
                IntentValue::Identifier(s.to_string())
            }
        }
    }
}

impl Default for IntentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Built-in intent functions ────────────────────────────────────────────

/// `input_filtering` intent — bulk or precision filtering on a power rail input.
///
/// Parameters:
///   - `bulk` (boolean, optional): true for bulk electrolytic-style filtering
///   - `max_esr` (resistance, optional): maximum ESR for filter capacitor
pub struct InputFilteringIntent;

impl IntentFunction for InputFilteringIntent {
    fn name(&self) -> &str {
        "input_filtering"
    }

    fn resolve(&self, _params: &[IntentParam]) -> Result<IntentResult, String> {
        Ok(IntentResult {
            sim_mode: SimMode::MixedSignal,
            synthesis_hints: vec![
                SynthesisHint::Custom("bulk_decoupling_capacitor".to_string()),
            ],
            validation_rules: Vec::new(),
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "bulk".to_string(),
                param_type: ParamType::Boolean,
                required: false,
                default_value: Some(IntentValue::Boolean(false)),
            },
            ParamMetadata {
                name: "max_esr".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: None,
            },
        ]
    }
}

/// `regulation` intent — marks a component as performing voltage regulation on a rail.
///
/// Parameters:
///   - `soft_start` (duration, optional): soft-start ramp time
///   - `dropout` (voltage, optional): regulator dropout voltage
pub struct RegulationIntent;

impl IntentFunction for RegulationIntent {
    fn name(&self) -> &str {
        "regulation"
    }

    fn resolve(&self, _params: &[IntentParam]) -> Result<IntentResult, String> {
        Ok(IntentResult {
            sim_mode: SimMode::MixedSignal,
            synthesis_hints: vec![
                SynthesisHint::Custom("voltage_regulation".to_string()),
            ],
            validation_rules: Vec::new(),
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "soft_start".to_string(),
                param_type: ParamType::Duration,
                required: false,
                default_value: None,
            },
            ParamMetadata {
                name: "dropout".to_string(),
                param_type: ParamType::Voltage,
                required: false,
                default_value: None,
            },
        ]
    }
}

/// `loading` intent — marks a component as a load on a power rail.
///
/// Parameters:
///   - `purpose` (string, optional): description of the load's role
pub struct LoadingIntent;

impl IntentFunction for LoadingIntent {
    fn name(&self) -> &str {
        "loading"
    }

    fn resolve(&self, _params: &[IntentParam]) -> Result<IntentResult, String> {
        Ok(IntentResult {
            sim_mode: SimMode::PureDigital,
            synthesis_hints: Vec::new(),
            validation_rules: Vec::new(),
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "purpose".to_string(),
                param_type: ParamType::String,
                required: false,
                default_value: None,
            },
        ]
    }
}

/// `output_filtering` intent — drives multi-tier capacitor bank generation
/// for switching regulator outputs.
///
/// Parameters:
///   - `max_ripple` (voltage, required): Maximum peak-to-peak output ripple
///   - `bandwidth` (frequency, optional): Target filter bandwidth
pub struct OutputFilteringIntent;

impl IntentFunction for OutputFilteringIntent {
    fn name(&self) -> &str {
        "output_filtering"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Validate max_ripple is present
        let has_max_ripple = params.iter().any(|p| match p {
            IntentParam::Named(name, _) => name == "max_ripple",
            IntentParam::Positional(_) => true, // first positional = max_ripple
        });

        if !has_max_ripple {
            return Err("output_filtering requires 'max_ripple' parameter".to_string());
        }

        Ok(IntentResult {
            sim_mode: SimMode::MixedSignal,
            synthesis_hints: vec![
                SynthesisHint::Custom("multi_tier_cap_bank".to_string()),
            ],
            validation_rules: vec![
                ValidationRule {
                    condition: "max_ripple > 0".to_string(),
                    error_message: "max_ripple must be positive".to_string(),
                },
            ],
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "max_ripple".to_string(),
                param_type: ParamType::Voltage,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "bandwidth".to_string(),
                param_type: ParamType::Frequency,
                required: false,
                default_value: None,
            },
        ]
    }
}