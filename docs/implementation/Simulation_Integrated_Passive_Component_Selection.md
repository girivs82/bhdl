# Simulation-Integrated Passive Component Selection

## Current State Analysis

### ✅ What We Have Available

From the `AnalysisResult` structure, we have access to:

1. **Power Analysis Data** (`PowerAnalysisContext`)
   - Power domain voltages and current capabilities
   - Power flow analysis between domains
   - Level shifter requirements

2. **Component Inference Data** (`ComponentInferenceContext`)
   - SPICE-resolved component values
   - Circuit context analysis (LED current limiting, voltage dividers, etc.)
   - Component constraints and electrical limits

3. **Flow Tracking Data** (`FlowTracker`)
   - Signal flow paths with design intent
   - Intent resolution to simulation modes
   - Hierarchical intent propagation

4. **Safety Analysis Data** (`SafetyAnalysisResult`)
   - DC operating point analysis
   - Safety violations and warnings
   - Electrical stress analysis

5. **SPICE Integration** (`bhdl-spice`)
   - Newton-Raphson nonlinear DC analysis
   - Component role detection through topology
   - Current and voltage calculations at operating points

### 🚧 What We're Missing

Currently, our passive component calculator uses **static calculations** based on:
- Design intent parameters (voltage, current from `for` statements)
- Power domain definitions
- Safety factor tables

But we're **not leveraging**:
- **Actual simulated currents and voltages** from SPICE DC analysis
- **Real power dissipation calculations** from simulation results
- **Frequency response data** from AC analysis
- **Transient behavior** for ripple current calculations

## Enhanced Integration Architecture

### 1. Simulation-Driven Parameter Extraction

```rust
/// Enhanced context that includes simulation results
#[derive(Debug, Clone)]
pub struct SimulationAugmentedContext {
    /// Static design requirements
    pub design_context: VirtualPinContext,
    
    /// DC operating point from SPICE analysis
    pub dc_operating_point: Option<DcOperatingPoint>,
    
    /// AC frequency response (for filter design)
    pub frequency_response: Option<FrequencyResponse>,
    
    /// Transient analysis results (for ripple calculations)
    pub transient_results: Option<TransientAnalysis>,
    
    /// Component stress analysis from safety checker
    pub stress_analysis: Option<ComponentStressAnalysis>,
}

#[derive(Debug, Clone)]
pub struct DcOperatingPoint {
    /// Actual simulated voltage at this node
    pub node_voltage: f64,
    
    /// Actual current through connected components
    pub branch_currents: HashMap<String, f64>,
    
    /// Power dissipation in connected components
    pub power_dissipation: HashMap<String, f64>,
    
    /// Operating temperature from thermal analysis
    pub operating_temperature: Option<f64>,
}
```

### 2. Enhanced Calculation Engine

```rust
impl PassiveComponentCalculator {
    /// Calculate resistor specifications using SPICE simulation results
    pub fn calculate_resistor_spec_from_simulation(
        &self,
        component_name: &str,
        simulation_context: &SimulationAugmentedContext,
        requirements: &ApplicationRequirements,
    ) -> Result<ResistorSpec> {
        
        // Use actual simulated current instead of estimated
        let actual_current = simulation_context
            .dc_operating_point
            .as_ref()
            .and_then(|dc| dc.branch_currents.get(component_name))
            .copied()
            .unwrap_or_else(|| {
                // Fallback to design intent calculation
                self.estimate_current_from_intent(&simulation_context.design_context)
            });
            
        // Use actual simulated power dissipation
        let actual_power = simulation_context
            .dc_operating_point
            .as_ref()
            .and_then(|dc| dc.power_dissipation.get(component_name))
            .copied()
            .unwrap_or_else(|| {
                // Fallback to I²R calculation
                let resistance = self.estimate_resistance(&simulation_context.design_context);
                actual_current * actual_current * resistance
            });
            
        // Account for actual operating temperature
        let temp_derating = if let Some(temp) = simulation_context
            .dc_operating_point
            .as_ref()
            .and_then(|dc| dc.operating_temperature) {
            self.calculate_temperature_derating(temp)
        } else {
            1.0 // No additional derating
        };
        
        // Calculate power rating with simulation-based data
        let required_power = actual_power / (self.safety_factors.power_derating * temp_derating);
        let power_rating = self.select_next_standard_power_rating(required_power);
        
        // Enhanced voltage rating based on actual node voltages
        let actual_voltage = simulation_context
            .dc_operating_point
            .as_ref()
            .map(|dc| dc.node_voltage)
            .unwrap_or(simulation_context.design_context.voltage_domain);
            
        let voltage_rating = self.calculate_resistor_voltage_rating(actual_voltage);
        
        Ok(ResistorSpec {
            resistance: self.calculate_resistance_from_simulation(&simulation_context),
            power_rating,
            voltage_rating,
            // ... other fields
        })
    }
    
    /// Calculate capacitor ESR requirements from ripple current simulation
    pub fn calculate_capacitor_esr_from_transient(
        &self,
        transient: &TransientAnalysis,
        max_temp_rise: f64,
    ) -> f64 {
        // Extract RMS ripple current from transient simulation
        let ripple_current_rms = transient.calculate_rms_ripple_current();
        
        // Use actual ripple current instead of estimated
        self.calculate_capacitor_esr_requirement(ripple_current_rms, max_temp_rise)
    }
}
```

### 3. SPICE Analysis Integration Points

#### A. DC Operating Point Integration
```rust
// In bhdl-synthesizer/src/module_variants.rs

impl ModuleVariantGenerator {
    async fn expand_virtual_pin_with_simulation_data(
        &mut self,
        netlist: &mut Netlist,
        module_id: ModuleId,
        pin_name: &str,
        analysis_result: &AnalysisResult,
        intent: &bhdl_common::IntentCall,
    ) -> Result<()> {
        
        // Extract simulation results if available
        let simulation_context = self.extract_simulation_context(pin_name, analysis_result)?;
        
        // Use enhanced calculator with simulation data
        let calculator = PassiveComponentCalculator::new();
        let selector = PackageSelector::new();
        
        match simulation_context {
            Some(sim_ctx) => {
                // Use actual simulation results
                self.synthesize_components_from_simulation(
                    netlist, module_id, pin_name, &sim_ctx, &calculator, &selector
                ).await
            },
            None => {
                // Fallback to design intent calculations
                self.synthesize_components_from_intent(
                    netlist, module_id, pin_name, intent, &calculator, &selector
                ).await
            }
        }
    }
    
    fn extract_simulation_context(
        &self,
        pin_name: &str,
        analysis_result: &AnalysisResult,
    ) -> Result<Option<SimulationAugmentedContext>> {
        
        // Extract DC operating point from SPICE analysis
        let dc_op = self.extract_dc_operating_point(pin_name, analysis_result)?;
        
        // Extract power dissipation from component inference
        let power_data = analysis_result.component_inference
            .get_power_dissipation_data(pin_name);
            
        // Extract safety analysis results
        let stress_analysis = analysis_result.safety_analysis
            .get_component_stress(pin_name);
            
        if dc_op.is_some() || power_data.is_some() || stress_analysis.is_some() {
            Ok(Some(SimulationAugmentedContext {
                design_context: self.extract_design_context(pin_name, analysis_result)?,
                dc_operating_point: dc_op,
                frequency_response: None, // TODO: Add AC analysis
                transient_results: None,  // TODO: Add transient analysis
                stress_analysis,
            }))
        } else {
            Ok(None)
        }
    }
}
```

#### B. Component Inference Integration
```rust
// Enhanced integration with existing SPICE component inference

impl ModuleVariantGenerator {
    /// Use component inference results to select passive components
    fn select_components_from_inference(
        &self,
        component_inference: &ComponentInferenceContext,
        pin_name: &str,
    ) -> Result<Vec<ComponentSpec>> {
        
        let mut components = Vec::new();
        
        // Check if SPICE analysis resolved component values
        if let Some(inferred_components) = component_inference.get_inferred_components(pin_name) {
            for component in inferred_components {
                match &component.circuit_context {
                    CircuitContext::LEDCurrentLimit { led_spec, supply_voltage, .. } => {
                        // Use SPICE-calculated resistance value
                        let resistance = component.calculated_value.unwrap_or(1000.0);
                        let current = (supply_voltage - led_spec.forward_voltage) / resistance;
                        
                        // Calculate actual power from SPICE results
                        let power_rating = self.calculator.calculate_resistor_power_rating(
                            resistance, current
                        );
                        
                        components.push(ComponentSpec::Resistor(ResistorSpec {
                            resistance,
                            power_rating,
                            // ... etc
                        }));
                    },
                    CircuitContext::FilterCapacitor { ripple_voltage, ripple_frequency, .. } => {
                        // Use SPICE-calculated capacitance
                        let capacitance = component.calculated_value.unwrap_or(100e-9);
                        
                        // Calculate ESR requirements from ripple analysis
                        let esr_requirement = self.calculator.calculate_esr_from_ripple(
                            *ripple_voltage, *ripple_frequency
                        );
                        
                        components.push(ComponentSpec::Capacitor(CapacitorSpec {
                            capacitance,
                            esr_max: Some(esr_requirement),
                            // ... etc
                        }));
                    },
                    _ => {
                        // Handle other circuit contexts
                    }
                }
            }
        }
        
        Ok(components)
    }
}
```

### 4. Integration with Safety Analysis

```rust
impl PassiveComponentCalculator {
    /// Use safety analysis results for enhanced derating
    pub fn calculate_safety_enhanced_rating(
        &self,
        base_requirement: f64,
        safety_analysis: &ComponentStressAnalysis,
        component_type: ComponentType,
    ) -> PowerRating {
        
        let mut derating_factor = self.safety_factors.power_derating;
        
        // Additional derating based on safety violations
        if safety_analysis.has_voltage_stress() {
            derating_factor *= 0.8; // 20% additional derating for voltage stress
        }
        
        if safety_analysis.has_thermal_stress() {
            derating_factor *= 0.7; // 30% additional derating for thermal stress
        }
        
        if safety_analysis.has_current_density_issues() {
            derating_factor *= 0.9; // 10% additional derating for current density
        }
        
        let derated_requirement = base_requirement / derating_factor;
        self.select_next_standard_power_rating(derated_requirement)
    }
}
```

### 5. Frequency-Aware Component Selection

```rust
impl PassiveComponentCalculator {
    /// Select capacitor based on frequency response simulation
    pub fn select_capacitor_from_frequency_response(
        &self,
        frequency_response: &FrequencyResponse,
        target_impedance: f64,
        frequency_range: (f64, f64),
    ) -> CapacitorSpec {
        
        // Find required capacitance from impedance at target frequency
        let target_freq = (frequency_range.0 * frequency_range.1).sqrt(); // Geometric mean
        let required_capacitance = 1.0 / (2.0 * std::f64::consts::PI * target_freq * target_impedance);
        
        // Check if capacitor performance meets requirements across frequency range
        let mut selected_dielectric = DielectricType::X7R;
        
        if frequency_range.1 > 10e6 {
            // High frequency requirements - check for dielectric losses
            if frequency_response.has_low_esr_requirement() {
                selected_dielectric = DielectricType::C0G;
            }
        }
        
        // Select based on actual simulated performance
        CapacitorSpec {
            capacitance: required_capacitance,
            dielectric: selected_dielectric,
            esr_max: Some(frequency_response.calculate_max_esr()),
            // ... other fields from simulation
        }
    }
}
```

## Implementation Phases

### Phase 1: DC Operating Point Integration ✅ (Ready to implement)
- Extract actual node voltages from SPICE DC analysis
- Use real current measurements instead of estimates
- Calculate power dissipation from simulation results
- Integrate with existing component inference data

### Phase 2: Safety Analysis Enhanced Derating ✅ (Ready to implement)  
- Use safety violation flags for additional derating
- Implement thermal stress-based component selection
- Add current density analysis for trace/via sizing

### Phase 3: AC Analysis Integration 🔄 (Requires AC analysis implementation)
- Frequency response-based filter component selection
- ESR requirements from ripple current analysis
- Bandwidth and phase margin considerations

### Phase 4: Transient Analysis Integration 🔄 (Requires transient analysis)
- Peak current calculations for power components
- Ripple current analysis for capacitor ESR
- Thermal cycling analysis for reliability

## Expected Improvements

### Before (Current Static Calculation):
```
5V domain @ 1A intent → 250mW resistor, 10V capacitor
```

### After (Simulation-Integrated):
```
5V domain @ 743mA actual simulated → 187mW actual → 375mW derated → 500mW selected
Safety violation detected → Additional 20% derating → 625mW → 1W selected
Temperature: 67°C → Thermal derating factor → 1W confirmed
ESR requirement from ripple: 0.2Ω → Low-ESR capacitor selected
```

## Next Steps

1. **Immediate**: Extend `extract_simulation_context()` in `module_variants.rs`
2. **Short-term**: Add DC operating point extraction from `AnalysisResult`
3. **Medium-term**: Integrate safety analysis enhanced derating
4. **Long-term**: Add AC and transient analysis integration

This creates a **simulation-driven passive component selection system** that uses actual electrical analysis instead of just design estimates - a major advancement in hardware synthesis automation! 🚀