//! Runtime model execution engine
//! 
//! This module provides runtime interpretation of component models defined in bhdl-stdlib.
//! Models are executed based on stdlib attributes rather than hardcoded logic.

use std::collections::HashMap;
use nalgebra::{DMatrix, DVector};
use anyhow::{Result, Context};
use bhdl_stdlib::{StdlibReader, StdlibComponentDefinition};
use bhdl_common::{ExpressionEvaluator, Value};

use crate::ComponentModel;
use crate::equation_engine::EquationEngine;

/// Context for model execution - provides access to circuit state and stamping
pub struct ModelExecutionContext<'a> {
    /// Jacobian matrix for stamping conductances
    pub jacobian: &'a mut DMatrix<f64>,
    /// Residual vector for stamping currents  
    pub residual: &'a mut DVector<f64>,
    /// Current solution vector (node voltages)
    pub x: &'a DVector<f64>,
    /// Node indices for this component (None = ground)
    pub n1_idx: Option<usize>,
    pub n2_idx: Option<usize>,
    /// Voltage difference across component (v1 - v2)
    pub v_diff: f64,
}

impl<'a> ModelExecutionContext<'a> {
    /// Get voltage at node 1 (input/positive terminal)
    pub fn get_v1(&self) -> f64 {
        self.n1_idx.map(|i| self.x[i]).unwrap_or(0.0)
    }
    
    /// Get voltage at node 2 (output/negative terminal)  
    pub fn get_v2(&self) -> f64 {
        self.n2_idx.map(|i| self.x[i]).unwrap_or(0.0)
    }
    
    /// Stamp linear element (conductance + current) into system matrices
    /// For Newton-Raphson: current is the Norton equivalent current source
    pub fn stamp_linear_element(&mut self, conductance: f64, current: f64) {
        // Stamp conductance in Jacobian
        if let Some(i1) = self.n1_idx {
            self.jacobian[(i1, i1)] += conductance;
            self.residual[i1] -= current;  // Note: negative for correct MNA formulation
        }
        if let Some(i2) = self.n2_idx {
            self.jacobian[(i2, i2)] += conductance;
            self.residual[i2] += current;  // Note: positive for correct MNA formulation
        }
        if let (Some(i1), Some(i2)) = (self.n1_idx, self.n2_idx) {
            self.jacobian[(i1, i2)] -= conductance;
            self.jacobian[(i2, i1)] -= conductance;
        }
    }
    
    /// Stamp nonlinear element using companion model approach
    /// For a nonlinear element, we use the companion model:
    /// i = g*v + i_eq where g = di/dv at operating point and i_eq = i - g*v
    pub fn stamp_nonlinear_element(&mut self, di_dv: f64, i_actual: f64) {
        // Calculate equivalent current source value
        // i_eq = i_actual - di_dv * v_diff
        let i_eq = i_actual - di_dv * self.v_diff;
        
        // Stamp as linear element with current source
        self.stamp_linear_element(di_dv, i_eq);
    }
}

/// Runtime component model execution engine
pub struct RuntimeModelEngine {
    /// Stdlib reader for component definitions
    stdlib_reader: StdlibReader,
    /// Cache of parsed component attributes for performance
    attribute_cache: HashMap<String, HashMap<String, String>>,
    /// Expression evaluator for BHDL expressions
    expression_evaluator: ExpressionEvaluator,
    /// Cache of loaded module contents
    module_content_cache: HashMap<String, String>,
    /// Equation engine for evaluating stdlib-defined equations
    equation_engine: EquationEngine,
}

impl RuntimeModelEngine {
    /// Create new runtime model engine
    pub fn new() -> Result<Self> {
        let stdlib_path = bhdl_stdlib::get_default_stdlib_path();
        let mut stdlib_reader = StdlibReader::new(stdlib_path);
        
        // Load all stdlib components
        stdlib_reader.load_all_components()
            .context("Failed to load stdlib components")?;
            
        Ok(Self {
            stdlib_reader,
            attribute_cache: HashMap::new(),
            expression_evaluator: ExpressionEvaluator::new(),
            module_content_cache: HashMap::new(),
            equation_engine: EquationEngine::new(),
        })
    }
    
    /// Execute component model based on stdlib definition or fallback to hardcoded
    pub fn execute_component_model(
        &mut self,
        component_name: &str,
        ctx: &mut ModelExecutionContext,
    ) -> Result<()> {
        // Try to find component in stdlib first
        let component_def_opt = self.stdlib_reader.get_component(component_name).cloned();
        if let Some(component_def) = component_def_opt {
            self.execute_stdlib_model(&component_def, ctx)
        } else {
            // Fallback to hardcoded model inference based on component name patterns
            self.execute_inferred_hardcoded_model(component_name, ctx)
        }
    }
    
    /// Execute component model with provided parameters (from ComponentModel enum)
    pub fn execute_component_model_with_params(
        &mut self,
        component_name: &str,
        model: &ComponentModel,
        ctx: &mut ModelExecutionContext,
    ) -> Result<()> {
        // For now, delegate to the appropriate model execution based on type
        match model {
            ComponentModel::Resistor { resistance, .. } => {
                self.execute_resistor_model(*resistance, ctx)
            }
            ComponentModel::LED { forward_voltage, forward_current, saturation_current, emission_coefficient, thermal_voltage, .. } => {
                self.execute_led_model_with_params(*forward_voltage, *forward_current, *saturation_current, *emission_coefficient, *thermal_voltage, ctx)
            }
            ComponentModel::Diode { saturation_current, emission_coefficient, .. } => {
                let is = saturation_current.unwrap_or(1e-12);
                let n = emission_coefficient.unwrap_or(1.0);
                self.execute_diode_model(is, n, ctx)
            }
            ComponentModel::Capacitor { capacitance, .. } => {
                // For DC analysis, capacitors are open circuits
                let r_open = 1e12; // 1TΩ
                let g = 1.0 / r_open;
                ctx.stamp_linear_element(g, 0.0);
                Ok(())
            }
            ComponentModel::Inductor { inductance, .. } => {
                // For DC analysis, inductors are short circuits
                let r_short = 1e-3; // 1mΩ
                let g = 1.0 / r_short;
                ctx.stamp_linear_element(g, 0.0);
                Ok(())
            }
            ComponentModel::CurrentSource { current, .. } => {
                self.execute_current_source_model(*current, ctx)
            }
            ComponentModel::VoltageRegulator { output_voltage, dropout_voltage, quiescent_current, .. } => {
                self.execute_voltage_regulator_model(
                    *output_voltage,
                    *dropout_voltage,
                    *quiescent_current,
                    ctx
                )
            }
            _ => {
                // For other models, try stdlib lookup
                self.execute_component_model(component_name, ctx)
            }
        }
    }
    
    /// Execute component model based on stdlib component definition
    fn execute_stdlib_model(
        &mut self,
        component_def: &StdlibComponentDefinition,
        ctx: &mut ModelExecutionContext,
    ) -> Result<()> {
        // First check if component has equation-based model
        if component_def.attributes.contains_key("spice_equation_i") {
            return self.execute_equation_based_model(component_def, ctx);
        }
        
        // Load the module if not already loaded
        // Check for 'params' which is what the attributes reference
        if !self.expression_evaluator.has_symbol("params") {
            self.load_module_for_evaluation(&component_def.module_name)?;
        }
        
        // Get the SPICE model type from stdlib attributes
        let spice_model = component_def.attributes.get("spice_model")
            .ok_or_else(|| anyhow::anyhow!("Component {} missing spice_model attribute", component_def.module_name))?;
        
        // Remove quotes if present
        let model_type = spice_model.trim().trim_matches('"');
            
        match model_type {
            "resistor" => self.execute_stdlib_resistor_model(component_def, ctx),
            "diode" => self.execute_stdlib_diode_model(component_def, ctx),
            "voltage_regulator" => self.execute_stdlib_voltage_regulator_model(component_def, ctx),
            "current_source" => self.execute_stdlib_current_source_model(component_def, ctx),
            "capacitor" => self.execute_stdlib_capacitor_model(component_def, ctx),
            _ => {
                // Unknown SPICE model type - try to infer from attributes
                self.execute_inferred_model(component_def, ctx)
            }
        }
    }
    
    /// Execute resistor model - simple linear
    fn execute_resistor_model(&self, resistance: f64, ctx: &mut ModelExecutionContext) -> Result<()> {
        let g = 1.0 / resistance;
        // For a linear resistor, Norton current = 0 (i = g*v, so i_norton = i - g*v = 0)
        ctx.stamp_linear_element(g, 0.0);
        Ok(())
    }
    
    /// Execute LED model with SPICE parameters from stdlib
    /// Pure Shockley equation: I = Is * (exp(V/(n*Vt)) - 1)
    fn execute_led_model_with_params(
        &self, 
        forward_voltage: f64, 
        forward_current: f64,
        saturation_current: Option<f64>,
        emission_coefficient: Option<f64>,
        thermal_voltage: Option<f64>,
        ctx: &mut ModelExecutionContext
    ) -> Result<()> {
        // Use stdlib parameters with realistic defaults if not provided
        let vt = thermal_voltage.unwrap_or(0.026);     // 26mV at room temperature
        let n = emission_coefficient.unwrap_or(2.0);   // Typical LED emission coefficient
        let is = saturation_current.unwrap_or(3.96e-19);  // Realistic saturation current (Vf=2V @ 20mA)
        
        // Pure Shockley equation - no voltage shifting
        let v = ctx.v_diff;
        
        // Calculate current and conductance using same limits as diode model
        const MAX_EXP: f64 = 50.0;
        const MIN_G: f64 = 1e-14;
        let v_norm = v / (n * vt);
        
        let i_actual = if v_norm > MAX_EXP {
            // Limit exponential to prevent overflow
            let i_max = is * (MAX_EXP.exp() - 1.0);
            let g_max = (is / (n * vt)) * MAX_EXP.exp();
            i_max + g_max * (v - MAX_EXP * n * vt)
        } else if v_norm < -5.0 {
            // Deep reverse bias
            -is
        } else {
            // Normal exponential region
            is * (v_norm.exp() - 1.0)
        };
        
        let di_dv = if v_norm > MAX_EXP {
            (is / (n * vt)) * MAX_EXP.exp()
        } else if v_norm < -5.0 {
            MIN_G
        } else {
            ((is / (n * vt)) * v_norm.exp()).max(MIN_G)
        };
        
        // Debug: print LED voltage on first few calls
        static mut DEBUG_COUNT: usize = 0;
        unsafe {
            if DEBUG_COUNT < 10 {
                eprintln!("LED stamp: V={:.3}V, I={:.2e}A, g={:.2e}S, Is={:.2e}A", 
                         v, i_actual, di_dv, is);
                DEBUG_COUNT += 1;
            }
        }
        
        // Use the nonlinear stamping method which handles Norton conversion correctly
        ctx.stamp_nonlinear_element(di_dv, i_actual);
        
        Ok(())
    }
    
    /// Execute LED model - compatibility wrapper for backward compatibility
    fn execute_led_model(&self, forward_voltage: f64, forward_current: f64, ctx: &mut ModelExecutionContext) -> Result<()> {
        // Use defaults for SPICE parameters
        self.execute_led_model_with_params(
            forward_voltage, 
            forward_current, 
            Some(3.96e-19),  // Default saturation current - realistic value
            Some(2.0),    // Default emission coefficient
            Some(0.026),  // Default thermal voltage
            ctx
        )
    }
    
    /// Execute diode model - Shockley equation
    fn execute_diode_model(&self, saturation_current: f64, emission_coefficient: f64, ctx: &mut ModelExecutionContext) -> Result<()> {
        let vt = 0.026;
        let n = emission_coefficient;
        let is = saturation_current;
        
        // Calculate current and conductance using same limits as reference
        const MAX_EXP: f64 = 50.0;
        const MIN_G: f64 = 1e-14;
        let v_norm = ctx.v_diff / (n * vt);
        
        let i_actual = if v_norm > MAX_EXP {
            // Limit exponential to prevent overflow
            let i_max = is * (MAX_EXP.exp() - 1.0);
            let g_max = (is / (n * vt)) * MAX_EXP.exp();
            i_max + g_max * (ctx.v_diff - MAX_EXP * n * vt)
        } else if v_norm < -5.0 {
            // Deep reverse bias
            -is
        } else {
            // Normal exponential region
            is * (v_norm.exp() - 1.0)
        };
        
        let di_dv = if v_norm > MAX_EXP {
            (is / (n * vt)) * MAX_EXP.exp()
        } else if v_norm < -5.0 {
            MIN_G
        } else {
            ((is / (n * vt)) * v_norm.exp()).max(MIN_G)
        };
        
        // Use nonlinear stamping
        ctx.stamp_nonlinear_element(di_dv, i_actual);
        
        Ok(())
    }
    
    /// Execute current source model
    fn execute_current_source_model(&self, current: f64, ctx: &mut ModelExecutionContext) -> Result<()> {
        if let Some(i1) = ctx.n1_idx {
            ctx.residual[i1] -= current;
        }
        if let Some(i2) = ctx.n2_idx {
            ctx.residual[i2] += current;
        }
        Ok(())
    }
    
    /// Execute voltage regulator model with adaptive gain (moved from adaptive_solver.rs)
    fn execute_voltage_regulator_model(
        &self, 
        output_voltage: f64, 
        dropout_voltage: f64, 
        quiescent_current: f64, 
        ctx: &mut ModelExecutionContext
    ) -> Result<()> {
        // This is the same adaptive algorithm, but now in the runtime engine
        let v_in = ctx.get_v1();
        let v_out = ctx.get_v2();
        
        let vin_min = output_voltage + dropout_voltage;
        
        if v_in >= vin_min {
            // Regulated mode: Auto-adaptive feedback gain
            let voltage_error = v_out - output_voltage;
            
            // Adaptive gain calculation based on circuit physics
            let headroom = v_in - vin_min;
            let max_headroom = 20.0;
            let headroom_factor = (headroom / max_headroom).min(1.0).max(0.1);
            
            // Base transconductance of typical linear regulator
            let base_transconductance = 1.0; // 1 S base value
            
            // Error-adaptive scaling
            let error_magnitude = voltage_error.abs();
            let error_scaling = if error_magnitude > 1.0 {
                // Large error: reduce gain to prevent oscillation
                1.0 / (1.0 + error_magnitude)
            } else if error_magnitude < 0.01 {
                // Small error: increase gain for precision
                1.0 + (0.01 - error_magnitude) * 10.0
            } else {
                // Medium error: nominal gain
                1.0
            };
            
            let adaptive_gain = base_transconductance * headroom_factor * error_scaling;
            let control_current = -adaptive_gain * voltage_error + quiescent_current;
            
            ctx.stamp_linear_element(adaptive_gain, control_current);
        } else {
            // Dropout mode: resistive behavior
            let dropout_resistance = 1.0 + (vin_min - v_in) * 10.0;
            let g = 1.0 / dropout_resistance.max(0.1);
            let i = g * ctx.v_diff + quiescent_current;
            ctx.stamp_linear_element(g, i);
        }
        
        Ok(())
    }
    
    /// Execute resistor model based on stdlib attributes
    fn execute_stdlib_resistor_model(
        &self,
        component_def: &StdlibComponentDefinition,
        ctx: &mut ModelExecutionContext,
    ) -> Result<()> {
        // Extract resistance value from stdlib attributes
        let resistance = self.parse_resistance_value(component_def)?;
        
        let g = 1.0 / resistance;
        // For a linear resistor, Norton current = 0 (i = g*v, so i_norton = i - g*v = 0)
        ctx.stamp_linear_element(g, 0.0);
        Ok(())
    }
    
    /// Execute diode/LED model based on stdlib attributes
    fn execute_stdlib_diode_model(
        &self,
        component_def: &StdlibComponentDefinition,
        ctx: &mut ModelExecutionContext,
    ) -> Result<()> {
        // Check if this is an LED (special diode type)
        let spice_type = component_def.attributes.get("spice_type");
        let is_led = spice_type.map_or(false, |t| t == "led");
        
        if is_led {
            self.execute_stdlib_led_model(component_def, ctx)
        } else {
            self.execute_stdlib_generic_diode_model(component_def, ctx)
        }
    }
    
    /// Execute LED model based on stdlib attributes
    fn execute_stdlib_led_model(
        &self,
        component_def: &StdlibComponentDefinition,
        ctx: &mut ModelExecutionContext,
    ) -> Result<()> {
        // Extract LED parameters from stdlib
        let forward_voltage = self.parse_voltage_value(component_def, "spice_vj")?;
        let forward_current = self.parse_current_value(component_def, "forward_current")?;
        
        // Use the same shifted exponential model as the hardcoded version
        self.execute_led_model_with_params(forward_voltage, forward_current, None, None, None, ctx)
    }
    
    /// Execute generic diode model based on stdlib attributes
    fn execute_stdlib_generic_diode_model(
        &self,
        component_def: &StdlibComponentDefinition,
        ctx: &mut ModelExecutionContext,
    ) -> Result<()> {
        // Extract diode parameters from stdlib
        let saturation_current = component_def.attributes.get("spice_is")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(1e-12);
        let emission_coefficient = component_def.attributes.get("spice_n")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(1.0);
        
        let vt = 0.026;
        
        if ctx.v_diff > 0.0 {
            let exp_term = (ctx.v_diff / (emission_coefficient * vt)).min(40.0).exp();
            let i = saturation_current * (exp_term - 1.0);
            let di_dv = (saturation_current / (emission_coefficient * vt)) * exp_term;
            ctx.stamp_linear_element(di_dv, i);
        } else {
            let i = -saturation_current;
            let di_dv = 1e-12;
            ctx.stamp_linear_element(di_dv, i);
        }
        
        Ok(())
    }
    
    /// Execute voltage regulator model based on stdlib attributes (same adaptive algorithm)
    fn execute_stdlib_voltage_regulator_model(
        &self,
        component_def: &StdlibComponentDefinition,
        ctx: &mut ModelExecutionContext,
    ) -> Result<()> {
        // Extract regulator parameters from stdlib
        let output_voltage = self.parse_voltage_value(component_def, "spice_output_voltage")?;
        let dropout_voltage = self.parse_voltage_value(component_def, "spice_dropout_voltage")?;
        let quiescent_current = self.parse_current_value(component_def, "spice_quiescent_current")?;
        
        // Use the same adaptive algorithm as before, but driven by stdlib data
        let v_in = ctx.get_v1();
        let v_out = ctx.get_v2();
        
        let vin_min = output_voltage + dropout_voltage;
        
        if v_in >= vin_min {
            // Regulated mode: Auto-adaptive feedback gain
            let voltage_error = v_out - output_voltage;
            
            // Same adaptive gain calculation as before
            let headroom = v_in - vin_min;
            let max_headroom = 20.0;
            let headroom_factor = (headroom / max_headroom).min(1.0).max(0.1);
            
            let base_transconductance = 1.0;
            
            let error_magnitude = voltage_error.abs();
            let error_scaling = if error_magnitude > 1.0 {
                1.0 / (1.0 + error_magnitude)
            } else if error_magnitude < 0.01 {
                1.0 + (0.01 - error_magnitude) * 10.0
            } else {
                1.0
            };
            
            let adaptive_gain = base_transconductance * headroom_factor * error_scaling;
            let control_current = -adaptive_gain * voltage_error + quiescent_current;
            
            ctx.stamp_linear_element(adaptive_gain, control_current);
        } else {
            // Dropout mode: resistive behavior
            let dropout_resistance = 1.0 + (vin_min - v_in) * 10.0;
            let g = 1.0 / dropout_resistance.max(0.1);
            let i = g * ctx.v_diff + quiescent_current;
            ctx.stamp_linear_element(g, i);
        }
        
        Ok(())
    }
    
    /// Execute current source model based on stdlib attributes
    fn execute_stdlib_current_source_model(
        &self,
        component_def: &StdlibComponentDefinition,
        ctx: &mut ModelExecutionContext,
    ) -> Result<()> {
        let current = self.parse_current_value(component_def, "current")?;
        
        if let Some(i1) = ctx.n1_idx {
            ctx.residual[i1] -= current;
        }
        if let Some(i2) = ctx.n2_idx {
            ctx.residual[i2] += current;
        }
        Ok(())
    }
    
    /// Execute capacitor model based on stdlib attributes
    /// For DC analysis, capacitors are open circuits (very high resistance)
    fn execute_stdlib_capacitor_model(
        &self,
        _component_def: &StdlibComponentDefinition,
        ctx: &mut ModelExecutionContext,
    ) -> Result<()> {
        // In DC analysis, capacitors act as open circuits
        // Use a very high resistance to approximate this
        let r_open = 1e12; // 1TΩ - effectively open circuit
        let g = 1.0 / r_open;
        // For a linear element, Norton current = 0
        ctx.stamp_linear_element(g, 0.0);
        Ok(())
    }
    
    /// Execute model with inferred behavior from attributes
    fn execute_inferred_model(
        &self,
        component_def: &StdlibComponentDefinition,
        ctx: &mut ModelExecutionContext,
    ) -> Result<()> {
        // Try to infer model type from component class
        if let Some(component_class) = component_def.attributes.get("component_class") {
            match component_class.as_str() {
                "resistor" => self.execute_stdlib_resistor_model(component_def, ctx),
                "led" => self.execute_stdlib_led_model(component_def, ctx),
                "diode" => self.execute_stdlib_generic_diode_model(component_def, ctx),
                "capacitor" => self.execute_stdlib_capacitor_model(component_def, ctx),
                "voltage_regulator" => self.execute_stdlib_voltage_regulator_model(component_def, ctx),
                _ => self.execute_default_model(&component_def.module_name, ctx)
            }
        } else {
            self.execute_default_model(&component_def.module_name, ctx)
        }
    }
    
    /// Execute model based on hardcoded name patterns for backward compatibility
    fn execute_inferred_hardcoded_model(
        &self,
        component_name: &str,
        ctx: &mut ModelExecutionContext,
    ) -> Result<()> {
        // Infer component type from naming patterns
        let component_type = component_name.chars().next().unwrap_or('?');
        
        match component_type {
            'R' => {
                // Resistor (R1, R2, etc.) - use default 1kΩ
                let resistance = 1000.0;
                let g = 1.0 / resistance;
                // For a linear resistor, Norton current = 0
                ctx.stamp_linear_element(g, 0.0);
                Ok(())
            },
            'D' => {
                // Diode/LED (D1, D2, etc.) - use generic LED model
                self.execute_led_model(2.0, 0.02, ctx) // 2V, 20mA typical
            },
            'V' => {
                // Voltage source (V1, V2, etc.) - this should be handled separately
                eprintln!("Warning: Voltage source '{}' should be handled by voltage source stamping", component_name);
                Ok(())
            },
            'I' => {
                // Current source (I1, I2, etc.)
                let current = 0.001; // 1mA default
                self.execute_current_source_model(current, ctx)
            },
            'C' => {
                // Capacitor (C1, C2, etc.) - open circuit in DC
                let r_open = 1e12; // 1TΩ
                let g = 1.0 / r_open;
                // For a linear element, Norton current = 0
                ctx.stamp_linear_element(g, 0.0);
                Ok(())
            },
            _ => {
                // Unknown component - use default
                self.execute_default_model(component_name, ctx)
            }
        }
    }
    
    /// Default model for unknown components
    fn execute_default_model(
        &self,
        component_name: &str,
        ctx: &mut ModelExecutionContext,
    ) -> Result<()> {
        // Default to high resistance for unknown components
        eprintln!("Warning: Unknown component type '{}', using default high resistance model", component_name);
        let resistance = 1e9; // 1GΩ default
        let g = 1.0 / resistance;
        // For a linear element, Norton current = 0
        ctx.stamp_linear_element(g, 0.0);
        Ok(())
    }
    
    /// Get component definition from stdlib by name
    pub fn get_stdlib_component(&self, name: &str) -> Option<&StdlibComponentDefinition> {
        self.stdlib_reader.get_component(name)
    }
    
    /// Execute equation-based model from stdlib attributes
    fn execute_equation_based_model(
        &mut self,
        component_def: &StdlibComponentDefinition,
        ctx: &mut ModelExecutionContext,
    ) -> Result<()> {
        // Note: In the current implementation, attributes are stored as strings
        // even if they were parsed from expressions. The stdlib reader extracts
        // the text representation of the expression.
        // TODO: In future, we could store the parsed AST directly in attributes
        
        // Parse equations from attributes
        if let Some(i_eq) = component_def.attributes.get("spice_equation_i") {
            // The attribute value contains the expression text
            self.equation_engine.parse_equation("i", i_eq.trim())?;
        } else {
            return Err(anyhow::anyhow!("Component {} has no spice_equation_i", component_def.module_name));
        }
        
        if let Some(di_dv_eq) = component_def.attributes.get("spice_equation_di_dv") {
            self.equation_engine.parse_equation("di_dv", di_dv_eq.trim())?;
        } else {
            return Err(anyhow::anyhow!("Component {} has no spice_equation_di_dv", component_def.module_name));
        }
        
        // Build variable bindings
        let mut vars = HashMap::new();
        vars.insert("v_diff".to_string(), ctx.v_diff);
        vars.insert("v1".to_string(), ctx.get_v1());
        vars.insert("v2".to_string(), ctx.get_v2());
        
        // Add all component attributes as variables
        for (key, value) in &component_def.attributes {
            // Try to parse as numeric value
            if let Ok(num_val) = self.parse_numeric_value(value) {
                vars.insert(key.clone(), num_val);
            }
        }
        
        // Evaluate equations
        let current = self.equation_engine.evaluate("i", &vars)?;
        let conductance = self.equation_engine.evaluate("di_dv", &vars)?;
        
        // Stamp into circuit matrix
        ctx.stamp_linear_element(conductance, current);
        
        Ok(())
    }
    
    /// Parse numeric value from attribute string (handles units)
    fn parse_numeric_value(&self, value_str: &str) -> Result<f64> {
        let trimmed = value_str.trim().trim_matches('"');
        
        // Try direct parse first
        if let Ok(val) = trimmed.parse::<f64>() {
            return Ok(val);
        }
        
        // Try to parse with units
        // Find where the unit starts
        let mut numeric_end = trimmed.len();
        for (i, ch) in trimmed.char_indices() {
            if ch.is_alphabetic() || ch == '°' || ch == 'Ω' || ch == 'µ' {
                numeric_end = i;
                break;
            }
        }
        
        let numeric_part = &trimmed[..numeric_end];
        let unit_part = &trimmed[numeric_end..];
        
        let base_value = numeric_part.parse::<f64>()?;
        
        // Apply unit multiplier
        let multiplier = match unit_part {
            "p" | "pF" | "pH" | "pA" | "pV" | "pW" => 1e-12,
            "n" | "nF" | "nH" | "nA" | "nV" | "nW" | "ns" => 1e-9,
            "u" | "µ" | "uF" | "µF" | "uH" | "µH" | "uA" | "µA" | "uV" | "µV" | "us" | "µs" => 1e-6,
            "m" | "mF" | "mH" | "mA" | "mV" | "mW" | "ms" | "mΩ" => 1e-3,
            "k" | "kΩ" | "kHz" | "kV" | "kA" | "kW" => 1e3,
            "M" | "MΩ" | "MHz" | "MV" | "MA" | "MW" => 1e6,
            "G" | "GΩ" | "GHz" | "GV" | "GA" | "GW" => 1e9,
            _ => 1.0,
        };
        
        Ok(base_value * multiplier)
    }
    
    /// Parse resistance value from component attributes
    fn parse_resistance_value(&self, component_def: &StdlibComponentDefinition) -> Result<f64> {
        // Try various resistance attribute names
        if let Some(value) = component_def.attributes.get("spice_resistance") {
            // Try to parse the value, if it's an expression like "params.resistance", use default
            match self.parse_electrical_value(value, "Ω") {
                Ok(val) => Ok(val),
                Err(_) => {
                    // Default to 1kΩ for resistors when we can't parse the expression
                    Ok(1000.0)
                }
            }
        } else if let Some(value) = component_def.attributes.get("resistance") {
            match self.parse_electrical_value(value, "Ω") {
                Ok(val) => Ok(val),
                Err(_) => Ok(1000.0)
            }
        } else if let Some(value) = component_def.attributes.get("dc_resistance") {
            match self.parse_electrical_value(value, "Ω") {
                Ok(val) => Ok(val),
                Err(_) => Ok(1000.0)
            }
        } else {
            // Default resistor value
            Ok(1000.0)
        }
    }
    
    /// Parse voltage value from component attributes
    fn parse_voltage_value(&self, component_def: &StdlibComponentDefinition, attr_name: &str) -> Result<f64> {
        if let Some(value) = component_def.attributes.get(attr_name) {
            self.parse_electrical_value(value, "V")
        } else {
            Err(anyhow::anyhow!("No {} value found for component {}", attr_name, component_def.module_name))
        }
    }
    
    /// Parse current value from component attributes
    fn parse_current_value(&self, component_def: &StdlibComponentDefinition, attr_name: &str) -> Result<f64> {
        if let Some(value) = component_def.attributes.get(attr_name) {
            self.parse_electrical_value(value, "A")
        } else {
            Err(anyhow::anyhow!("No {} value found for component {}", attr_name, component_def.module_name))
        }
    }
    
    /// Parse electrical value with unit handling
    fn parse_electrical_value(&self, value_str: &str, expected_unit: &str) -> Result<f64> {
        // Handle various electrical unit formats
        let value_str = value_str.trim();
        
        // Check if this is a BHDL expression reference (e.g., "params.output_voltage")
        if value_str.contains("params.") || value_str.contains("_PARAMS.") || value_str.contains('.') {
            // Evaluate the expression using the expression evaluator
            let local_symbols = HashMap::new(); // Could be populated with local context if needed
            match self.expression_evaluator.evaluate_string(value_str, &local_symbols) {
                Ok(Value::Number(n)) => return Ok(n),
                Ok(_) => return Err(anyhow::anyhow!("Expression {} did not evaluate to a number", value_str)),
                Err(_) => {
                    // Fall through to try parsing as literal
                }
            }
        }
        
        // Try using the expression evaluator's electrical value parser
        self.expression_evaluator.parse_electrical_value(value_str)
    }
    
    
    /// Load a module for expression evaluation
    fn load_module_for_evaluation(&mut self, module_name: &str) -> Result<()> {
        // First check if already loaded
        if self.module_content_cache.contains_key(module_name) {
            let content = self.module_content_cache.get(module_name).unwrap().clone();
            self.expression_evaluator.load_module(module_name, &content)?;
            return Ok(());
        }
        
        // Try to find the module file path
        let stdlib_path = bhdl_stdlib::get_default_stdlib_path();
        let possible_paths = vec![
            format!("{}/regulators/{}.bhdl", stdlib_path, module_name.to_lowercase()),
            format!("{}/passives/{}.bhdl", stdlib_path, module_name.to_lowercase()),
            format!("{}/power/{}.bhdl", stdlib_path, module_name.to_lowercase()),
            format!("{}/connectors/{}.bhdl", stdlib_path, module_name.to_lowercase()),
        ];
        
        for path in &possible_paths {
            if let Ok(content) = std::fs::read_to_string(&path) {
                self.module_content_cache.insert(module_name.to_string(), content.clone());
                self.expression_evaluator.load_module(module_name, &content)?;
                return Ok(());
            }
        }
        
        // If not found in standard paths, return error
        Err(anyhow::anyhow!("Module {} not found in stdlib paths", module_name))
    }
}

impl Default for RuntimeModelEngine {
    fn default() -> Self {
        Self::new().expect("Failed to create runtime model engine")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::{DMatrix, DVector};
    
    #[test]
    fn test_runtime_model_engine_creation() {
        let result = RuntimeModelEngine::new();
        assert!(result.is_ok());
    }
    
    #[test] 
    fn test_resistor_model_execution() {
        let engine = RuntimeModelEngine::new().unwrap();
        
        let mut jacobian = DMatrix::zeros(2, 2);
        let mut residual = DVector::zeros(2);
        let x = DVector::from_vec(vec![1.0, 0.0]);
        
        let mut ctx = ModelExecutionContext {
            jacobian: &mut jacobian,
            residual: &mut residual,
            x: &x,
            n1_idx: Some(0),
            n2_idx: Some(1),
            v_diff: 1.0,
        };
        
        let result = engine.execute_resistor_model(1000.0, &mut ctx);
        assert!(result.is_ok());
        
        // Check that conductance was stamped
        assert_eq!(jacobian[(0, 0)], 0.001);
        assert_eq!(jacobian[(1, 1)], 0.001);
        assert_eq!(jacobian[(0, 1)], -0.001);
        assert_eq!(jacobian[(1, 0)], -0.001);
        
        // A linear resistor has zero Norton current (i = g*v, so
        // i_norton = i - g*v = 0) and contributes nothing to the residual.
        assert_eq!(residual[0], 0.0);
        assert_eq!(residual[1], 0.0);
    }
}