// Development and debugging intent functions

use bhdl_common::{
    IntentFunction, IntentParam, IntentResult, IntentValue,
    SimMode, SynthesisHint, ValidationRule, ToolScope,
    ParamMetadata, ParamType
};

/// Debug-only intent to opt out of production requirements
pub struct DebugOnlyIntent;

impl IntentFunction for DebugOnlyIntent {
    fn name(&self) -> &str {
        "debug_only"
    }

    fn resolve(&self, _params: &[IntentParam]) -> Result<IntentResult, String> {
        // Debug-only signals can be simulated with minimal requirements
        // They're exempt from production validation rules
        Ok(IntentResult {
            sim_mode: SimMode::PureDigital, // Simplest simulation mode
            synthesis_hints: vec![
                SynthesisHint::Custom("DEBUG ONLY - not for production".to_string()),
                SynthesisHint::Custom("Omit from production builds".to_string()),
                SynthesisHint::Custom("No component placement needed".to_string()),
            ],
            validation_rules: vec![
                ValidationRule {
                    condition: "not_in_production_build".to_string(),
                    error_message: "debug_only() signals must not appear in production builds".to_string(),
                }
            ],
            tool_scope: ToolScope::SimulationOnly, // Only for simulation, not synthesis
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![]  // No parameters
    }
}
