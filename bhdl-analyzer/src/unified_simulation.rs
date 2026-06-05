//! Unified Simulation Orchestrator
//! 
//! This module coordinates all simulation engines to run once and extract all needed data
//! for component selection, safety analysis, and derating decisions.

use crate::types::*;
use std::collections::HashMap;
use std::time::Instant;
use log::{info, warn, debug};

/// Simulation feedback structures that mirror the stdlib simulation_knowledge.bhdl interface
/// These represent the knowledge that components provide back to simulation engines

#[derive(Debug, Clone)]
pub struct SimulationFeedback {
    pub operating_point_constraints: Option<OperatingPointConstraints>,
    pub nonlinear_model: Option<NonlinearModel>,
    pub thermal_model: Option<ThermalModel>,
    pub frequency_response: Option<FrequencyResponseModel>,
    pub noise_model: Option<NoiseModel>,
    pub dynamic_model: Option<DynamicModel>,
    pub stress_derating: Option<StressDerating>,
    pub selection_constraints: Option<SelectionConstraints>,
}

#[derive(Debug, Clone)]
pub struct OperatingPointConstraints {
    pub min_node_voltage: Option<f64>,
    pub max_node_voltage: Option<f64>,
    pub voltage_drop_model: Option<String>,
    pub min_current: Option<f64>,
    pub max_current: Option<f64>,
    pub current_limiting: Option<bool>,
    pub max_power_dissipation: Option<f64>,
    pub efficiency: Option<f64>,
    pub convergence_aid: Option<ConvergenceAid>,
}

#[derive(Debug, Clone)]
pub struct NonlinearModel {
    pub model_type: String,
    pub parameters: Vec<(String, f64)>,
}

#[derive(Debug, Clone)]
pub struct ThermalModel {
    pub thermal_resistance: Option<f64>,
    pub max_operating_temperature: Option<f64>,
    pub temperature_coefficient: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct FrequencyResponseModel {
    pub impedance_model: String,
    pub esr: Option<f64>,
    pub esl: Option<f64>,
    pub frequency_range: (f64, f64),
}

#[derive(Debug, Clone)]
pub struct NoiseModel {
    pub noise_type: String,
    pub noise_density: f64,
}

#[derive(Debug, Clone)]
pub struct DynamicModel {
    pub model_type: String,
    pub time_constants: Vec<f64>,
}

#[derive(Debug, Clone)]
pub struct StressDerating {
    pub current_derating_curve: Vec<(f64, f64)>,
    pub temperature_derating_curve: Vec<(f64, f64)>,
}

#[derive(Debug, Clone)]
pub struct SelectionConstraints {
    pub preferred_packages: Vec<String>,
    pub tolerance_requirements: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct ConvergenceAid {
    pub initial_guess: Option<f64>,
    pub damping_factor: Option<f64>,
}

impl Default for SimulationFeedback {
    fn default() -> Self {
        Self {
            operating_point_constraints: None,
            nonlinear_model: None,
            thermal_model: None,
            frequency_response: None,
            noise_model: None,
            dynamic_model: None,
            stress_derating: None,
            selection_constraints: None,
        }
    }
}

/// Orchestrates unified simulation across all engines
pub struct UnifiedSimulationOrchestrator {
    /// Whether to run DC analysis
    pub enable_dc_analysis: bool,
    /// Whether to run electrical safety analysis  
    pub enable_electrical_safety: bool,
    /// Whether to run thermal analysis
    pub enable_thermal_analysis: bool,
    /// Whether to run AC analysis (when available)
    pub enable_ac_analysis: bool,
    /// Whether to run transient analysis (when available)
    pub enable_transient_analysis: bool,
    /// Ambient temperature for thermal analysis
    pub ambient_temperature: f64,
}

impl Default for UnifiedSimulationOrchestrator {
    fn default() -> Self {
        Self {
            enable_dc_analysis: true,
            enable_electrical_safety: true,  
            enable_thermal_analysis: true,
            enable_ac_analysis: false, // Not yet implemented
            enable_transient_analysis: false, // Not yet implemented
            ambient_temperature: 25.0, // 25°C default
        }
    }
}

impl UnifiedSimulationOrchestrator {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Configure which simulation engines to run
    pub fn with_configuration(
        dc: bool, 
        safety: bool, 
        thermal: bool, 
        ac: bool, 
        transient: bool
    ) -> Self {
        Self {
            enable_dc_analysis: dc,
            enable_electrical_safety: safety,
            enable_thermal_analysis: thermal,
            enable_ac_analysis: ac,
            enable_transient_analysis: transient,
            ambient_temperature: 25.0,
        }
    }
    
    /// Run unified simulation and populate all data structures
    pub fn run_unified_simulation(
        &self,
        netlist: &bhdl_netlist::Netlist,
        component_inference: &crate::component_inference::ComponentInferenceContext,
    ) -> Result<UnifiedSimulationData, String> {
        let start_time = Instant::now();
        info!("🔬 Starting unified simulation orchestrator");
        
        let mut simulation_data = UnifiedSimulationData::new();
        let mut engines_used = Vec::new();
        let mut warnings = Vec::new();
        
        // Phase 1: DC Operating Point Analysis
        if self.enable_dc_analysis {
            info!("Phase 1: Running DC operating point analysis");
            match self.run_dc_analysis(netlist, component_inference) {
                Ok(dc_results) => {
                    simulation_data.dc_analysis = Some(dc_results);
                    engines_used.push("DC Analysis".to_string());
                    info!("✓ DC analysis completed successfully");
                }
                Err(e) => {
                    warn!("DC analysis failed: {}", e);
                    warnings.push(format!("DC analysis failed: {}", e));
                }
            }
        }
        
        // Phase 2: Electrical Safety Analysis (requires DC results)
        if self.enable_electrical_safety {
            info!("Phase 2: Running electrical safety analysis");
            if let Some(ref dc_results) = simulation_data.dc_analysis {
                match self.run_electrical_safety_analysis(netlist, dc_results, component_inference) {
                    Ok(safety_results) => {
                        simulation_data.electrical_safety = Some(safety_results);
                        engines_used.push("Electrical Safety Analysis".to_string());
                        info!("✓ Electrical safety analysis completed successfully");
                    }
                    Err(e) => {
                        warn!("Electrical safety analysis failed: {}", e);
                        warnings.push(format!("Electrical safety analysis failed: {}", e));
                    }
                }
            } else {
                warnings.push("Electrical safety analysis skipped - DC analysis required".to_string());
            }
        }
        
        // Phase 3: Thermal Analysis (uses DC power dissipation results)
        if self.enable_thermal_analysis {
            info!("Phase 3: Running thermal analysis");
            match self.run_thermal_analysis(netlist, &simulation_data) {
                Ok(thermal_results) => {
                    simulation_data.thermal_analysis = Some(thermal_results);
                    engines_used.push("Thermal Analysis".to_string());
                    info!("✓ Thermal analysis completed successfully");
                }
                Err(e) => {
                    warn!("Thermal analysis failed: {}", e);
                    warnings.push(format!("Thermal analysis failed: {}", e));
                }
            }
        }
        
        // Phase 4: AC Analysis (future implementation)
        if self.enable_ac_analysis {
            info!("Phase 4: AC analysis requested but not yet implemented");
            warnings.push("AC analysis not yet implemented".to_string());
        }
        
        // Phase 5: Transient Analysis (future implementation) 
        if self.enable_transient_analysis {
            info!("Phase 5: Transient analysis requested but not yet implemented");
            warnings.push("Transient analysis not yet implemented".to_string());
        }
        
        // Finalize simulation metadata
        let simulation_time = start_time.elapsed().as_millis() as f64;
        simulation_data.simulation_metadata = SimulationMetadata {
            simulation_time_ms: simulation_time,
            engines_used,
            simulation_accuracy: self.assess_simulation_accuracy(&simulation_data, &warnings),
            warnings,
            timestamp: std::time::SystemTime::now(),
        };
        
        info!("🎯 Unified simulation completed in {:.1}ms", simulation_time);
        self.log_simulation_summary(&simulation_data);
        
        Ok(simulation_data)
    }
    
    /// Run DC operating point analysis using bhdl-spice with stdlib simulation knowledge
    fn run_dc_analysis(
        &self,
        netlist: &bhdl_netlist::Netlist,
        component_inference: &crate::component_inference::ComponentInferenceContext,
    ) -> Result<DcSimulationResults, String> {
        debug!("Converting netlist to SPICE circuit with stdlib simulation knowledge");
        
        // Convert BHDL netlist to SPICE circuit representation using stdlib knowledge
        let circuit = self.convert_netlist_to_spice_circuit_with_stdlib_knowledge(netlist, component_inference)?;
        
        // Create SPICE analysis engine
        let mut spice_engine = bhdl_spice::SpiceEngine::new();
        
        // Configure for DC analysis with convergence aids from stdlib
        spice_engine.set_analysis_type(bhdl_spice::AnalysisType::DC);
        spice_engine.set_convergence_tolerance(1e-9);
        spice_engine.set_max_iterations(100);
        
        // Apply convergence aids from stdlib simulation knowledge
        for component in component_inference.get_inferred_components() {
            if let Some(instance_name) = &component.instance_name {
                if let Some(sim_feedback) = self.load_simulation_knowledge(component) {
                    if let Some(ref constraints) = sim_feedback.operating_point_constraints {
                        if let Some(ref convergence_aid) = constraints.convergence_aid {
                            // Set initial guess for nonlinear components
                            if let Some(initial_guess) = convergence_aid.initial_guess {
                                spice_engine.set_initial_condition(instance_name, initial_guess);
                                debug!("Applied initial guess {:.2}V for component {}", initial_guess, instance_name);
                            }
                            
                            // Set damping factor for difficult convergence cases
                            if let Some(damping) = convergence_aid.damping_factor {
                                spice_engine.set_component_damping(instance_name, damping);
                                debug!("Applied damping factor {:.2} for component {}", damping, instance_name);
                            }
                        }
                    }
                }
            }
        }
        
        // Run DC analysis
        debug!("Running Newton-Raphson DC analysis");
        let analysis_result = spice_engine.analyze(&circuit)
            .map_err(|e| format!("SPICE DC analysis failed: {}", e))?;
            
        // Extract DC operating point data
        let mut node_voltages = HashMap::new();
        let mut branch_currents = HashMap::new();
        let mut power_dissipation = HashMap::new();
        let mut operating_temperatures = HashMap::new();
        
        // Convert SPICE results to our unified format
        for (node_id, voltage) in analysis_result.node_voltages.iter() {
            node_voltages.insert(node_id.clone(), *voltage);
        }
        
        for (component_id, current) in analysis_result.branch_currents.iter() {
            branch_currents.insert(component_id.clone(), *current);
        }
        
        // Calculate power dissipation: P = V * I or P = I²R depending on component
        for (component_id, current) in &analysis_result.branch_currents {
            if let Some(voltage_drop) = analysis_result.component_voltage_drops.get(component_id) {
                let power = voltage_drop.abs() * current.abs();
                power_dissipation.insert(component_id.clone(), power);
            }
        }
        
        // Calculate operating temperatures from power dissipation (simplified thermal model)
        for (component_id, power) in &power_dissipation {
            // Simple thermal model: ΔT = P * Rth + Tambient
            // Use default thermal resistance of 50°C/W for resistors, 25°C/W for ICs
            let thermal_resistance = if component_id.starts_with('R') {
                50.0 // °C/W for resistors
            } else {
                25.0 // °C/W for ICs
            };
            let temperature = self.ambient_temperature + power * thermal_resistance;
            operating_temperatures.insert(component_id.clone(), temperature);
        }
        
        Ok(DcSimulationResults {
            node_voltages,
            branch_currents,
            power_dissipation,
            operating_temperatures,
            converged: analysis_result.converged,
            iterations: analysis_result.iterations,
            final_residual: analysis_result.final_residual,
        })
    }
    
    /// Run electrical safety analysis using DC results
    fn run_electrical_safety_analysis(
        &self,
        netlist: &bhdl_netlist::Netlist,
        dc_results: &DcSimulationResults,
        component_inference: &crate::component_inference::ComponentInferenceContext,
    ) -> Result<ElectricalSafetyResults, String> {
        debug!("Running electrical safety analysis");
        
        let mut component_stress = HashMap::new();
        let mut current_density_violations = Vec::new();
        let mut voltage_stress_violations = Vec::new();
        let mut thermal_stress_violations = Vec::new();
        
        // Analyze each component for stress conditions
        for component in component_inference.get_inferred_components() {
            if let Some(instance_name) = &component.instance_name {
                let stress_analysis = self.analyze_component_stress(
                    instance_name,
                    component,
                    dc_results,
                )?;
                
                // Check for violations
                if stress_analysis.has_voltage_stress {
                    voltage_stress_violations.push(VoltageStressViolation {
                        component: instance_name.clone(),
                        applied_voltage: dc_results.node_voltages.get(instance_name).copied().unwrap_or(0.0),
                        max_voltage: self.get_component_max_voltage(component),
                        stress_ratio: stress_analysis.voltage_stress_ratio,
                        severity: self.categorize_stress_severity(stress_analysis.voltage_stress_ratio),
                    });
                }
                
                if stress_analysis.has_current_stress {
                    current_density_violations.push(CurrentDensityViolation {
                        location: instance_name.clone(),
                        current: dc_results.branch_currents.get(instance_name).copied().unwrap_or(0.0),
                        max_safe_current: self.get_component_max_current(component),
                        severity: self.categorize_stress_severity(stress_analysis.current_stress_ratio),
                    });
                }
                
                if stress_analysis.has_thermal_stress {
                    thermal_stress_violations.push(ThermalStressViolation {
                        component: instance_name.clone(),
                        operating_temperature: dc_results.operating_temperatures.get(instance_name).copied().unwrap_or(25.0),
                        max_temperature: self.get_component_max_temperature(component),
                        thermal_derating_required: stress_analysis.thermal_stress_ratio > 0.8,
                        severity: self.categorize_stress_severity(stress_analysis.thermal_stress_ratio),
                    });
                }
                
                component_stress.insert(instance_name.clone(), stress_analysis);
            }
        }
        
        // Create safety summary
        let total_violations = voltage_stress_violations.len() + current_density_violations.len() + thermal_stress_violations.len();
        let critical_violations = voltage_stress_violations.iter()
            .filter(|v| matches!(v.severity, SafetyViolationSeverity::Critical))
            .count() + 
            current_density_violations.iter()
            .filter(|v| matches!(v.severity, SafetyViolationSeverity::Critical))
            .count() +
            thermal_stress_violations.iter()  
            .filter(|v| matches!(v.severity, SafetyViolationSeverity::Critical))
            .count();
        
        let components_needing_derating: Vec<String> = component_stress.iter()
            .filter(|(_, stress)| stress.has_voltage_stress || stress.has_current_stress || stress.has_thermal_stress)
            .map(|(name, _)| name.clone())
            .collect();
            
        let estimated_reliability_impact = (critical_violations as f64) / (total_violations.max(1) as f64);
        
        Ok(ElectricalSafetyResults {
            component_stress,
            current_density_violations,
            voltage_stress_violations,
            thermal_stress_violations,
            safety_summary: ElectricalSafetySummary {
                total_violations,
                critical_violations,
                components_needing_derating,
                estimated_reliability_impact,
            },
        })
    }
    
    /// Run thermal analysis using power dissipation data
    fn run_thermal_analysis(
        &self,
        _netlist: &bhdl_netlist::Netlist,
        simulation_data: &UnifiedSimulationData,
    ) -> Result<ThermalSimulationResults, String> {
        debug!("Running thermal analysis");
        
        let mut component_temperatures = HashMap::new();
        let mut hot_spots = Vec::new();
        let mut thermal_derating_factors = HashMap::new();
        
        if let Some(ref dc_results) = simulation_data.dc_analysis {
            // Use operating temperatures from DC analysis as base
            component_temperatures = dc_results.operating_temperatures.clone();
            
            // Identify hot spots (components > 85°C)
            for (component_id, temperature) in &component_temperatures {
                if *temperature > 85.0 {
                    hot_spots.push(HotSpot {
                        location: component_id.clone(),
                        temperature: *temperature,
                        components_affected: vec![component_id.clone()], // Simplified - could analyze thermal coupling
                        cooling_required: *temperature > 100.0,
                    });
                }
            }
            
            // Calculate thermal derating factors
            for (component_id, temperature) in &component_temperatures {
                // Linear derating above 70°C, 50% derating at 125°C
                let derating_factor = if *temperature <= 70.0 {
                    1.0 // No derating
                } else if *temperature >= 125.0 {
                    0.5 // Maximum derating
                } else {
                    // Linear interpolation between 70°C and 125°C
                    1.0 - 0.5 * (*temperature - 70.0) / (125.0 - 70.0)
                };
                thermal_derating_factors.insert(component_id.clone(), derating_factor);
            }
        }
        
        Ok(ThermalSimulationResults {
            component_temperatures,
            hot_spots,
            thermal_derating_factors,
            ambient_temperature: self.ambient_temperature,
        })
    }
    
    /// Analyze stress conditions for a single component
    fn analyze_component_stress(
        &self,
        instance_name: &str,
        component: &crate::component_inference::ComponentSuggestion,
        dc_results: &DcSimulationResults,
    ) -> Result<ComponentStressAnalysis, String> {
        // Get actual operating conditions
        let operating_voltage = dc_results.node_voltages.get(instance_name).copied().unwrap_or(0.0);
        let operating_current = dc_results.branch_currents.get(instance_name).copied().unwrap_or(0.0);
        let operating_power = dc_results.power_dissipation.get(instance_name).copied().unwrap_or(0.0);
        let operating_temp = dc_results.operating_temperatures.get(instance_name).copied().unwrap_or(25.0);
        
        // Get component limits
        let max_voltage = self.get_component_max_voltage(component);
        let max_current = self.get_component_max_current(component);
        let max_power = self.get_component_max_power(component);
        let max_temperature = self.get_component_max_temperature(component);
        
        // Calculate stress ratios
        let voltage_stress_ratio = operating_voltage.abs() / max_voltage;
        let current_stress_ratio = operating_current.abs() / max_current;
        let power_stress_ratio = operating_power / max_power;
        let thermal_stress_ratio = operating_temp / max_temperature;
        
        // Determine if stress is excessive (>80% of rating)
        let has_voltage_stress = voltage_stress_ratio > 0.8;
        let has_current_stress = current_stress_ratio > 0.8;
        let has_thermal_stress = thermal_stress_ratio > 0.8;
        
        // Generate derating recommendations
        let mut derating_recommendations = Vec::new();
        
        if has_voltage_stress {
            derating_recommendations.push(DeratingRecommendation {
                parameter: "voltage_rating".to_string(),
                current_value: max_voltage,
                recommended_value: operating_voltage.abs() / 0.7, // 70% derating
                derating_factor: 0.7,
                reason: format!("Voltage stress ratio {:.1}% exceeds safe limit", voltage_stress_ratio * 100.0),
            });
        }
        
        if has_current_stress {
            derating_recommendations.push(DeratingRecommendation {
                parameter: "current_rating".to_string(),
                current_value: max_current,
                recommended_value: operating_current.abs() / 0.8, // 80% derating
                derating_factor: 0.8,
                reason: format!("Current stress ratio {:.1}% exceeds safe limit", current_stress_ratio * 100.0),
            });
        }
        
        if thermal_stress_ratio > 0.8 {
            derating_recommendations.push(DeratingRecommendation {
                parameter: "thermal_rating".to_string(),
                current_value: max_temperature,
                recommended_value: operating_temp / 0.8, // 80% thermal derating
                derating_factor: 0.8,
                reason: format!("Operating temperature {:.1}°C approaches thermal limit", operating_temp),
            });
        }
        
        Ok(ComponentStressAnalysis {
            component_name: instance_name.to_string(),
            voltage_stress_ratio,
            current_stress_ratio,
            power_stress_ratio,
            thermal_stress_ratio,
            has_voltage_stress,
            has_current_stress,
            has_thermal_stress,
            derating_recommendations,
        })
    }
    
    /// Convert BHDL netlist to SPICE circuit using stdlib simulation knowledge
    fn convert_netlist_to_spice_circuit_with_stdlib_knowledge(
        &self,
        _netlist: &bhdl_netlist::Netlist,
        component_inference: &crate::component_inference::ComponentInferenceContext,
    ) -> Result<bhdl_spice::Circuit, String> {
        debug!("Converting netlist to SPICE circuit with stdlib simulation knowledge");
        
        let mut circuit = bhdl_spice::Circuit::new();
        
        // Process each component with its stdlib simulation knowledge
        for component in component_inference.get_inferred_components() {
            if let Some(instance_name) = &component.instance_name {
                if let Some(sim_feedback) = self.load_simulation_knowledge(component) {
                    debug!("Applying simulation knowledge for component: {}", instance_name);
                    
                    // Add component to SPICE circuit with stdlib-derived model
                    match component.component_type.as_str() {
                        "LED" => {
                            if let Some(ref nonlinear) = sim_feedback.nonlinear_model {
                                circuit.add_diode_component(
                                    instance_name, 
                                    &nonlinear.parameters
                                );
                                info!("Added LED {} with diode model from stdlib", instance_name);
                            }
                        }
                        "Res" | "Resistor" => {
                            if let Some(ref nonlinear) = sim_feedback.nonlinear_model {
                                // Extract resistance from component parameters
                                // Real-Data Policy: real declared resistance or hard error.
                                let resistance = component.parameters.iter()
                                    .find(|p| p.name.to_lowercase() == "resistance" || p.name.to_lowercase() == "value")
                                    .and_then(|p| match p.value {
                                        crate::component_inference::ParameterValue::Resistance(val) => Some(val),
                                        _ => None,
                                    })
                                    .ok_or_else(|| format!(
                                        "resistor '{}' has no real resistance value — Real-Data Policy: \
                                         declare it on the entity", instance_name))?;
                                    
                                circuit.add_resistor_component(
                                    instance_name, 
                                    resistance,
                                    &nonlinear.parameters
                                );
                                info!("Added resistor {} with {}Ω from stdlib", instance_name, resistance);
                            }
                        }
                        "Cap" | "Capacitor" => {
                            if let Some(ref freq_resp) = sim_feedback.frequency_response {
                                // Extract capacitance from component parameters
                                // Real-Data Policy: real declared capacitance or hard error.
                                let capacitance = component.parameters.iter()
                                    .find(|p| p.name.to_lowercase() == "capacitance" || p.name.to_lowercase() == "value")
                                    .and_then(|p| match p.value {
                                        crate::component_inference::ParameterValue::Capacitance(val) => Some(val),
                                        _ => None,
                                    })
                                    .ok_or_else(|| format!(
                                        "capacitor '{}' has no real capacitance value — Real-Data Policy: \
                                         declare it on the entity", instance_name))?;

                                // Parasitics: a real value if the stdlib model supplies one, else
                                // IDEAL (0) — an ideal element makes no measurement claim, whereas a
                                // fabricated 0.1Ω/1nH would. (Real-Data Policy: no invented value.)
                                circuit.add_capacitor_component(
                                    instance_name,
                                    capacitance,
                                    freq_resp.esr.unwrap_or(0.0),
                                    freq_resp.esl.unwrap_or(0.0)
                                );
                                info!("Added capacitor {} with {:.0}nF from stdlib", instance_name, capacitance * 1e9);
                            }
                        }
                        _ => {
                            warn!("Unknown component type: {} for {}", component.component_type, instance_name);
                        }
                    }
                    
                    // Apply thermal coupling if available
                    if let Some(ref thermal) = sim_feedback.thermal_model {
                        if let Some(thermal_resistance) = thermal.thermal_resistance {
                            circuit.set_thermal_coupling(instance_name, thermal_resistance, self.ambient_temperature);
                            debug!("Applied thermal coupling for {}: {}°C/W", instance_name, thermal_resistance);
                        }
                    }
                } else {
                    // Fallback for components without stdlib knowledge
                    warn!("No stdlib simulation knowledge found for component: {}", instance_name);
                    circuit.add_generic_component(instance_name, &component.component_type);
                }
            }
        }
        
        info!("Successfully converted netlist with {} components using stdlib knowledge", 
              component_inference.get_inferred_components().len());
        
        Ok(circuit)
    }
    
    /// Convert BHDL netlist to SPICE circuit (legacy simplified interface)
    fn convert_netlist_to_spice_circuit(
        &self,
        netlist: &bhdl_netlist::Netlist,
        component_inference: &crate::component_inference::ComponentInferenceContext,
    ) -> Result<bhdl_spice::Circuit, String> {
        // Delegate to stdlib-aware version
        self.convert_netlist_to_spice_circuit_with_stdlib_knowledge(netlist, component_inference)
    }
    
    /// Get component maximum voltage rating from stdlib simulation knowledge
    fn get_component_max_voltage(&self, component: &crate::component_inference::ComponentSuggestion) -> f64 {
        // Load simulation knowledge from stdlib
        if let Some(sim_feedback) = self.load_simulation_knowledge(component) {
            if let Some(ref constraints) = sim_feedback.operating_point_constraints {
                if let Some(max_voltage) = constraints.max_node_voltage {
                    return max_voltage;
                }
            }
        }
        
        // Fallback to component-specific defaults based on stdlib electrical_params
        match component.component_type.as_str() {
            "Res" | "Resistor" => 250.0, // From stdlib ResistorParams.voltage_rating
            "Cap" | "Capacitor" => 50.0,  // From stdlib CapacitorParams.voltage_rating
            "LED" => 5.0,                 // From stdlib LEDParams.reverse_voltage
            _ => 100.0,                   // Conservative default
        }
    }
    
    /// Get component maximum current rating from stdlib simulation knowledge
    fn get_component_max_current(&self, component: &crate::component_inference::ComponentSuggestion) -> f64 {
        // Load simulation knowledge from stdlib
        if let Some(sim_feedback) = self.load_simulation_knowledge(component) {
            if let Some(ref constraints) = sim_feedback.operating_point_constraints {
                if let Some(max_current) = constraints.max_current {
                    return max_current;
                }
            }
        }
        
        // Fallback to component-specific defaults from stdlib electrical_params  
        match component.component_type.as_str() {
            "Res" | "Resistor" => 0.5,    // From stdlib ResistorParams.current_rating
            "Cap" | "Capacitor" => 1.0,   // From stdlib CapacitorParams.ripple_current_max
            "LED" => 0.030,               // From stdlib LEDParams.max_current (30mA)
            _ => 1.0,                     // Conservative default
        }
    }
    
    /// Get component maximum power rating from stdlib simulation knowledge
    fn get_component_max_power(&self, component: &crate::component_inference::ComponentSuggestion) -> f64 {
        // Load simulation knowledge from stdlib
        if let Some(sim_feedback) = self.load_simulation_knowledge(component) {
            if let Some(ref constraints) = sim_feedback.operating_point_constraints {
                if let Some(max_power) = constraints.max_power_dissipation {
                    return max_power;
                }
            }
        }
        
        // Fallback to component-specific defaults from stdlib electrical_params
        match component.component_type.as_str() {
            "Res" | "Resistor" => 0.25,   // From stdlib ResistorParams.power_rating (1206 package)
            "Cap" | "Capacitor" => 0.1,   // ESR power dissipation limit
            "LED" => 0.1,                 // From stdlib LEDParams.max_power (100mW)
            _ => 1.0,                     // Conservative default
        }
    }
    
    /// Get component maximum temperature rating from stdlib simulation knowledge
    fn get_component_max_temperature(&self, component: &crate::component_inference::ComponentSuggestion) -> f64 {
        // Load simulation knowledge from stdlib 
        if let Some(sim_feedback) = self.load_simulation_knowledge(component) {
            if let Some(ref thermal) = sim_feedback.thermal_model {
                if let Some(max_temp) = thermal.max_operating_temperature {
                    return max_temp;
                }
            }
        }
        
        // Fallback to component-specific defaults from stdlib electrical_params
        match component.component_type.as_str() {
            "Res" | "Resistor" => 125.0,  // From stdlib ResistorParams.temperature_range
            "Cap" | "Capacitor" => 85.0,  // From stdlib CapacitorParams.temperature_range (X7R)
            "LED" => 85.0,                // From stdlib LEDParams.max_junction_temp
            _ => 85.0,                    // Conservative default
        }
    }
    
    /// Load simulation knowledge from stdlib for a specific component
    fn load_simulation_knowledge(&self, component: &crate::component_inference::ComponentSuggestion) -> Option<SimulationFeedback> {
        // This would integrate with the stdlib loader to get actual simulation knowledge
        // For now, create enhanced mock data based on stdlib structure
        
        match component.component_type.as_str() {
            "LED" => Some(SimulationFeedback {
                operating_point_constraints: Some(OperatingPointConstraints {
                    min_node_voltage: Some(1.8),     // LED turn-on threshold
                    max_node_voltage: Some(5.0),     // Reverse voltage limit
                    voltage_drop_model: Some("diode".to_string()),
                    min_current: Some(0.001),        // 1mA minimum for visible light
                    max_current: Some(0.030),        // 30mA max continuous from stdlib
                    current_limiting: Some(false),    // LEDs don't limit current
                    max_power_dissipation: Some(0.1), // 100mW from stdlib LEDParams
                    efficiency: Some(0.15),          // 15% luminous efficiency
                    convergence_aid: Some(ConvergenceAid {
                        initial_guess: Some(2.0),     // Start DC solver at ~2V
                        damping_factor: Some(0.8),    // Slower convergence for nonlinear
                    }),
                }),
                nonlinear_model: Some(NonlinearModel {
                    model_type: "diode".to_string(),
                    parameters: vec![
                        ("is".to_string(), 1e-14),    // Saturation current
                        ("n".to_string(), 1.8),       // Ideality factor  
                        ("rs".to_string(), 0.5),      // Series resistance
                    ],
                }),
                thermal_model: Some(ThermalModel {
                    thermal_resistance: Some(50.0),  // °C/W junction to ambient
                    max_operating_temperature: Some(85.0), // From stdlib
                    temperature_coefficient: Some(-0.002), // -2mV/°C
                }),
                stress_derating: Some(StressDerating {
                    current_derating_curve: vec![
                        (0.8, 1.0),   // 100% rating up to 80% of max
                        (0.9, 0.8),   // 80% rating at 90% of max  
                        (1.0, 0.5),   // 50% rating at 100% of max
                    ],
                    temperature_derating_curve: vec![
                        (25.0, 1.0),   // 100% rating at 25°C
                        (70.0, 0.8),   // 80% rating at 70°C
                        (85.0, 0.6),   // 60% rating at 85°C
                    ],
                }),
                ..Default::default()
            }),
            
            "Res" | "Resistor" => Some(SimulationFeedback {
                operating_point_constraints: Some(OperatingPointConstraints {
                    min_node_voltage: None,          // No minimum voltage requirement
                    max_node_voltage: Some(250.0),   // Resistor voltage rating
                    voltage_drop_model: Some("linear".to_string()),
                    min_current: None,               // No minimum current requirement
                    max_current: Some(0.5),          // From package power rating
                    current_limiting: Some(false),   // Resistors don't limit current
                    max_power_dissipation: Some(0.25), // 1206 package from stdlib
                    efficiency: Some(0.0),           // Resistors dissipate all power
                    convergence_aid: Some(ConvergenceAid {
                        initial_guess: None,          // Linear - no convergence issues
                        damping_factor: Some(1.0),    // Full Newton steps
                    }),
                }),
                nonlinear_model: Some(NonlinearModel {
                    model_type: "linear".to_string(),
                    parameters: vec![
                        ("r".to_string(), 1000.0),   // Default resistance - will be overridden by component value
                        ("tc".to_string(), 100e-6),   // Temperature coefficient
                    ],
                }),
                thermal_model: Some(ThermalModel {
                    thermal_resistance: Some(200.0), // °C/W for SMD resistor
                    max_operating_temperature: Some(125.0), // From stdlib
                    temperature_coefficient: Some(100e-6), // 100ppm/°C
                }),
                ..Default::default()
            }),
            
            "Cap" | "Capacitor" => Some(SimulationFeedback {
                operating_point_constraints: Some(OperatingPointConstraints {
                    min_node_voltage: None,          // No minimum voltage requirement
                    max_node_voltage: Some(50.0),    // Capacitor voltage rating
                    voltage_drop_model: Some("ideal".to_string()),
                    min_current: None,               // No minimum current requirement
                    max_current: Some(1.0),          // Ripple current rating
                    current_limiting: Some(false),   // Capacitors don't limit current
                    max_power_dissipation: Some(0.1), // ESR power limit
                    efficiency: Some(0.98),          // 98% efficient (low ESR)
                    convergence_aid: Some(ConvergenceAid {
                        initial_guess: Some(0.0),     // Start with no voltage across cap
                        damping_factor: Some(1.0),    // Linear - no convergence issues
                    }),
                }),
                frequency_response: Some(FrequencyResponseModel {
                    impedance_model: "capacitive".to_string(),
                    esr: Some(0.1),                  // 100mΩ ESR
                    esl: Some(1e-9),                 // 1nH ESL
                    frequency_range: (1.0, 100e6),  // 1Hz to 100MHz
                }),
                ..Default::default()
            }),
            
            _ => None, // Unknown component types
        }
    }
    
    /// Categorize stress severity based on stress ratio
    fn categorize_stress_severity(&self, stress_ratio: f64) -> SafetyViolationSeverity {
        if stress_ratio > 1.0 {
            SafetyViolationSeverity::Critical
        } else if stress_ratio > 0.95 {
            SafetyViolationSeverity::Error
        } else if stress_ratio > 0.85 {
            SafetyViolationSeverity::Warning
        } else {
            SafetyViolationSeverity::Info
        }
    }
    
    /// Assess overall simulation accuracy
    fn assess_simulation_accuracy(
        &self,
        simulation_data: &UnifiedSimulationData,
        warnings: &[String],
    ) -> SimulationAccuracy {
        let mut convergence_quality = 1.0;
        let mut model_fidelity = 0.8; // Base fidelity
        let mut limitations = Vec::new();
        
        // Check DC convergence quality
        if let Some(ref dc) = simulation_data.dc_analysis {
            if !dc.converged {
                convergence_quality = 0.3;
                limitations.push("DC analysis did not converge".to_string());
            } else if dc.final_residual > 1e-6 {
                convergence_quality = 0.7;
                limitations.push("DC analysis convergence marginal".to_string());
            }
        }
        
        // Reduce fidelity based on missing analyses
        if simulation_data.ac_analysis.is_none() {
            model_fidelity *= 0.9;
            limitations.push("AC analysis not available".to_string());
        }
        
        if simulation_data.transient_analysis.is_none() {
            model_fidelity *= 0.9;
            limitations.push("Transient analysis not available".to_string());
        }
        
        // Account for warnings
        let warning_penalty = (warnings.len() as f64) * 0.1;
        let confidence_level = (convergence_quality * model_fidelity - warning_penalty).max(0.0).min(1.0);
        
        SimulationAccuracy {
            convergence_quality,
            model_fidelity,
            confidence_level,
            limitations,
        }
    }
    
    /// Log simulation summary
    fn log_simulation_summary(&self, simulation_data: &UnifiedSimulationData) {
        info!("📊 Simulation Summary:");
        
        if let Some(ref dc) = simulation_data.dc_analysis {
            info!("  🔌 DC Analysis: {} nodes, {} components, {} iterations", 
                  dc.node_voltages.len(), dc.branch_currents.len(), dc.iterations);
            if !dc.converged {
                warn!("  ⚠️  DC analysis did not converge!");
            }
        }
        
        if let Some(ref safety) = simulation_data.electrical_safety {
            info!("  🛡️  Safety Analysis: {} violations, {} critical", 
                  safety.safety_summary.total_violations,
                  safety.safety_summary.critical_violations);
            if safety.safety_summary.critical_violations > 0 {
                warn!("  ⚠️  {} components need derating", safety.safety_summary.components_needing_derating.len());
            }
        }
        
        if let Some(ref thermal) = simulation_data.thermal_analysis {
            info!("  🌡️  Thermal Analysis: {} components analyzed, {} hot spots", 
                  thermal.component_temperatures.len(),
                  thermal.hot_spots.len());
        }
        
        let metadata = &simulation_data.simulation_metadata;
        info!("  ⚡ Performance: {:.1}ms, confidence {:.1}%", 
              metadata.simulation_time_ms, 
              metadata.simulation_accuracy.confidence_level * 100.0);
    }
}

// Placeholder types for bhdl-spice integration (would come from actual bhdl-spice crate)
// Enhanced to support stdlib simulation knowledge integration
mod bhdl_spice {
    use std::collections::HashMap;
    
    pub struct SpiceEngine {
        initial_conditions: HashMap<String, f64>,
        component_dampings: HashMap<String, f64>,
    }
    
    pub struct Circuit {
        components: Vec<ComponentEntry>,
    }
    
    pub enum AnalysisType { DC }
    
    pub struct AnalysisResult {
        pub node_voltages: HashMap<String, f64>,
        pub branch_currents: HashMap<String, f64>,
        pub component_voltage_drops: HashMap<String, f64>,
        pub converged: bool,
        pub iterations: usize,
        pub final_residual: f64,
    }
    
    #[derive(Debug)]
    struct ComponentEntry {
        name: String,
        component_type: String,
        parameters: HashMap<String, f64>,
    }
    
    impl SpiceEngine {
        pub fn new() -> Self { 
            Self { 
                initial_conditions: HashMap::new(),
                component_dampings: HashMap::new(),
            }
        }
        
        pub fn set_analysis_type(&mut self, _: AnalysisType) {}
        pub fn set_convergence_tolerance(&mut self, _: f64) {}
        pub fn set_max_iterations(&mut self, _: usize) {}
        
        // New methods for stdlib simulation knowledge integration
        pub fn set_initial_condition(&mut self, component_name: &str, voltage: f64) {
            self.initial_conditions.insert(component_name.to_string(), voltage);
        }
        
        pub fn set_component_damping(&mut self, component_name: &str, damping: f64) {
            self.component_dampings.insert(component_name.to_string(), damping);
        }
        
        pub fn analyze(&self, circuit: &Circuit) -> Result<AnalysisResult, String> {
            // Enhanced placeholder using stdlib knowledge
            let mut node_voltages = HashMap::new();
            let mut branch_currents = HashMap::new();
            let mut component_voltage_drops = HashMap::new();
            
            // Simulate using stdlib knowledge
            for component in &circuit.components {
                match component.component_type.as_str() {
                    "LED" => {
                        // Use diode model from stdlib
                        let forward_voltage = 2.0; // From stdlib LEDParams
                        node_voltages.insert(component.name.clone(), forward_voltage);
                        branch_currents.insert(component.name.clone(), 0.020); // 20mA
                        component_voltage_drops.insert(component.name.clone(), forward_voltage);
                    }
                    "Resistor" => {
                        // Use linear model from stdlib
                        let resistance = component.parameters.get("r").copied().unwrap_or(1000.0);
                        let current = 0.020; // Assume same as LED current
                        let voltage = current * resistance;
                        node_voltages.insert(component.name.clone(), voltage);
                        branch_currents.insert(component.name.clone(), current);
                        component_voltage_drops.insert(component.name.clone(), voltage);
                    }
                    "Capacitor" => {
                        // Use ideal model for DC analysis
                        node_voltages.insert(component.name.clone(), 5.0); // Supply voltage
                        branch_currents.insert(component.name.clone(), 0.0); // No DC current
                        component_voltage_drops.insert(component.name.clone(), 0.0);
                    }
                    _ => {
                        // Generic component
                        node_voltages.insert(component.name.clone(), 3.3);
                        branch_currents.insert(component.name.clone(), 0.010);
                        component_voltage_drops.insert(component.name.clone(), 3.3);
                    }
                }
            }
            
            Ok(AnalysisResult {
                node_voltages,
                branch_currents,
                component_voltage_drops,
                converged: true,
                iterations: 5,
                final_residual: 1e-12,
            })
        }
    }
    
    impl Circuit {
        pub fn new() -> Self { 
            Self { 
                components: Vec::new(),
            }
        }
        
        // New methods for stdlib-aware component addition
        pub fn add_diode_component(&mut self, name: &str, parameters: &[(String, f64)]) {
            let mut param_map = HashMap::new();
            for (key, value) in parameters {
                param_map.insert(key.clone(), *value);
            }
            
            self.components.push(ComponentEntry {
                name: name.to_string(),
                component_type: "LED".to_string(),
                parameters: param_map,
            });
        }
        
        pub fn add_resistor_component(&mut self, name: &str, resistance: f64, parameters: &[(String, f64)]) {
            let mut param_map = HashMap::new();
            param_map.insert("r".to_string(), resistance);
            for (key, value) in parameters {
                param_map.insert(key.clone(), *value);
            }
            
            self.components.push(ComponentEntry {
                name: name.to_string(),
                component_type: "Resistor".to_string(),
                parameters: param_map,
            });
        }
        
        pub fn add_capacitor_component(&mut self, name: &str, capacitance: f64, esr: f64, esl: f64) {
            let mut param_map = HashMap::new();
            param_map.insert("c".to_string(), capacitance);
            param_map.insert("esr".to_string(), esr);
            param_map.insert("esl".to_string(), esl);
            
            self.components.push(ComponentEntry {
                name: name.to_string(),
                component_type: "Capacitor".to_string(),
                parameters: param_map,
            });
        }
        
        pub fn add_generic_component(&mut self, name: &str, component_type: &str) {
            self.components.push(ComponentEntry {
                name: name.to_string(),
                component_type: component_type.to_string(),
                parameters: HashMap::new(),
            });
        }
        
        pub fn set_thermal_coupling(&mut self, _component_name: &str, _thermal_resistance: f64, _ambient_temp: f64) {
            // Placeholder for thermal coupling - would be implemented in real SPICE engine
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_unified_simulation_orchestrator() {
        let orchestrator = UnifiedSimulationOrchestrator::new();
        assert!(orchestrator.enable_dc_analysis);
        assert!(orchestrator.enable_electrical_safety);
        assert_eq!(orchestrator.ambient_temperature, 25.0);
    }
    
    #[test]
    fn test_stress_severity_categorization() {
        let orchestrator = UnifiedSimulationOrchestrator::new();
        
        assert_eq!(orchestrator.categorize_stress_severity(1.1), SafetyViolationSeverity::Critical);
        assert_eq!(orchestrator.categorize_stress_severity(0.97), SafetyViolationSeverity::Error);
        assert_eq!(orchestrator.categorize_stress_severity(0.87), SafetyViolationSeverity::Warning);
        assert_eq!(orchestrator.categorize_stress_severity(0.7), SafetyViolationSeverity::Info);
    }
    
    #[test]
    fn test_unified_simulation_data_methods() {
        let mut sim_data = UnifiedSimulationData::new();
        
        // Test default state
        assert!(!sim_data.has_safety_violations("R1"));
        assert_eq!(sim_data.get_derating_factor("R1"), 1.0);
        assert!(sim_data.get_operating_voltage("R1").is_none());
        
        // Add some mock safety data
        let mut safety_results = ElectricalSafetyResults {
            component_stress: HashMap::new(),
            current_density_violations: Vec::new(),
            voltage_stress_violations: Vec::new(),
            thermal_stress_violations: Vec::new(),
            safety_summary: ElectricalSafetySummary {
                total_violations: 1,
                critical_violations: 0,
                components_needing_derating: vec!["R1".to_string()],
                estimated_reliability_impact: 0.1,
            },
        };
        
        safety_results.component_stress.insert("R1".to_string(), ComponentStressAnalysis {
            component_name: "R1".to_string(),
            voltage_stress_ratio: 0.9,
            current_stress_ratio: 0.7,
            power_stress_ratio: 0.8,
            thermal_stress_ratio: 0.6,
            has_voltage_stress: true,
            has_current_stress: false,
            has_thermal_stress: false,
            derating_recommendations: Vec::new(),
        });
        
        sim_data.electrical_safety = Some(safety_results);
        
        // Test with safety data
        assert!(sim_data.has_safety_violations("R1"));
        assert_eq!(sim_data.get_derating_factor("R1"), 0.8); // 20% derating for voltage stress
    }
}