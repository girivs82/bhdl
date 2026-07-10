// Passive Component Calculation Engine
// Provides intelligent calculation of resistor and capacitor specifications
// based on voltage domains, current requirements, and safety factors

use std::fmt;

/// Standard power ratings for resistors (in watts)
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum PowerRating {
    P62mW = 62,      // 1/16W (0402)
    P100mW = 100,    // 1/8W (0603)
    P125mW = 125,    // 1/8W+ (0805)
    P250mW = 250,    // 1/4W (1206)
    P500mW = 500,    // 1/2W (2010)
    P1W = 1000,      // 1W (2512)
    P2W = 2000,      // 2W (2512)
    P5W = 5000,      // 5W (THT)
    P10W = 10000,    // 10W (THT)
}

impl PowerRating {
    /// Get power rating as watts
    pub fn as_watts(self) -> f64 {
        (self as u32) as f64 / 1000.0
    }
    
    /// Get next higher standard power rating
    pub fn next_higher(self) -> Self {
        match self {
            PowerRating::P62mW => PowerRating::P100mW,
            PowerRating::P100mW => PowerRating::P125mW,
            PowerRating::P125mW => PowerRating::P250mW,
            PowerRating::P250mW => PowerRating::P500mW,
            PowerRating::P500mW => PowerRating::P1W,
            PowerRating::P1W => PowerRating::P2W,
            PowerRating::P2W => PowerRating::P5W,
            PowerRating::P5W => PowerRating::P10W,
            PowerRating::P10W => PowerRating::P10W, // Max
        }
    }
}

impl fmt::Display for PowerRating {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PowerRating::P62mW => write!(f, "62.5mW"),
            PowerRating::P100mW => write!(f, "100mW"),
            PowerRating::P125mW => write!(f, "125mW"),
            PowerRating::P250mW => write!(f, "250mW"),
            PowerRating::P500mW => write!(f, "500mW"),
            PowerRating::P1W => write!(f, "1W"),
            PowerRating::P2W => write!(f, "2W"),
            PowerRating::P5W => write!(f, "5W"),
            PowerRating::P10W => write!(f, "10W"),
        }
    }
}

/// Standard voltage ratings for components (in volts)
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum VoltageRating {
    V6_3 = 63,       // 6.3V (low voltage)
    V10 = 100,       // 10V
    V16 = 160,       // 16V
    V25 = 250,       // 25V
    V35 = 350,       // 35V
    V50 = 500,       // 50V
    V63 = 630,       // 63V
    V100 = 1000,     // 100V
    V200 = 2000,     // 200V
    V400 = 4000,     // 400V
    V630 = 6300,     // 630V (high voltage)
}

impl VoltageRating {
    /// Get voltage rating as volts
    pub fn as_volts(self) -> f64 {
        (self as u32) as f64 / 100.0
    }
    
    /// Get next higher standard voltage rating
    pub fn next_higher(self) -> Self {
        match self {
            VoltageRating::V6_3 => VoltageRating::V10,
            VoltageRating::V10 => VoltageRating::V16,
            VoltageRating::V16 => VoltageRating::V25,
            VoltageRating::V25 => VoltageRating::V35,
            VoltageRating::V35 => VoltageRating::V50,
            VoltageRating::V50 => VoltageRating::V63,
            VoltageRating::V63 => VoltageRating::V100,
            VoltageRating::V100 => VoltageRating::V200,
            VoltageRating::V200 => VoltageRating::V400,
            VoltageRating::V400 => VoltageRating::V630,
            VoltageRating::V630 => VoltageRating::V630, // Max
        }
    }
}

impl fmt::Display for VoltageRating {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VoltageRating::V6_3 => write!(f, "6.3V"),
            VoltageRating::V10 => write!(f, "10V"),
            VoltageRating::V16 => write!(f, "16V"),
            VoltageRating::V25 => write!(f, "25V"),
            VoltageRating::V35 => write!(f, "35V"),
            VoltageRating::V50 => write!(f, "50V"),
            VoltageRating::V63 => write!(f, "63V"),
            VoltageRating::V100 => write!(f, "100V"),
            VoltageRating::V200 => write!(f, "200V"),
            VoltageRating::V400 => write!(f, "400V"),
            VoltageRating::V630 => write!(f, "630V"),
        }
    }
}

/// Package sizes for surface mount components
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PackageSize {
    _0201,    // 0.6mm x 0.3mm (ultra-miniature)
    _0402,    // 1.0mm x 0.5mm (miniature)
    _0603,    // 1.6mm x 0.8mm (small)
    _0805,    // 2.0mm x 1.25mm (standard)
    _1206,    // 3.2mm x 1.6mm (medium)
    _1210,    // 3.2mm x 2.5mm (medium-high voltage)
    _2010,    // 5.0mm x 2.5mm (high power)
    _2512,    // 6.4mm x 3.2mm (very high power)
    THT,      // Through-hole technology
}

impl fmt::Display for PackageSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackageSize::_0201 => write!(f, "0201"),
            PackageSize::_0402 => write!(f, "0402"),
            PackageSize::_0603 => write!(f, "0603"),
            PackageSize::_0805 => write!(f, "0805"),
            PackageSize::_1206 => write!(f, "1206"),
            PackageSize::_1210 => write!(f, "1210"),
            PackageSize::_2010 => write!(f, "2010"),
            PackageSize::_2512 => write!(f, "2512"),
            PackageSize::THT => write!(f, "THT"),
        }
    }
}

/// Capacitor dielectric types with their characteristics
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DielectricType {
    C0G,    // Ultra-stable, low capacitance, precision applications
    X7R,    // Good stability, general purpose
    X5R,    // Medium stability, higher capacitance
    Y5V,    // Poor stability, very high capacitance, cost-sensitive
}

impl DielectricType {
    /// Get typical temperature coefficient
    pub fn temp_coefficient(&self) -> &'static str {
        match self {
            DielectricType::C0G => "±30ppm/°C",
            DielectricType::X7R => "±15%",
            DielectricType::X5R => "±22%", 
            DielectricType::Y5V => "+22%/-82%",
        }
    }
    
    /// Get maximum recommended capacitance for this dielectric
    pub fn max_capacitance(&self) -> f64 {
        match self {
            DielectricType::C0G => 100e-9,   // 100nF
            DielectricType::X7R => 10e-6,    // 10μF
            DielectricType::X5R => 100e-6,   // 100μF
            DielectricType::Y5V => 1000e-6,  // 1mF
        }
    }
    
    /// Check if suitable for high frequency applications
    pub fn is_frequency_stable(&self) -> bool {
        matches!(self, DielectricType::C0G | DielectricType::X7R)
    }
}

impl fmt::Display for DielectricType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DielectricType::C0G => write!(f, "C0G/NP0"),
            DielectricType::X7R => write!(f, "X7R"),
            DielectricType::X5R => write!(f, "X5R"),
            DielectricType::Y5V => write!(f, "Y5V"),
        }
    }
}

/// Safety factors for component derating
#[derive(Debug, Clone)]
pub struct SafetyFactors {
    /// Power derating factor (e.g., 0.7 = 70% of max power)
    pub power_derating: f64,
    
    /// Voltage safety margin (e.g., 2.0 = use component rated 2x operating voltage)
    pub voltage_safety_margin: f64,
    
    /// Temperature derating for harsh environments
    pub temperature_derating: f64,
    
    /// Current safety margin for resistors
    pub current_safety_margin: f64,
}

impl Default for SafetyFactors {
    fn default() -> Self {
        Self {
            power_derating: 0.7,        // 70% power derating
            voltage_safety_margin: 2.0, // 2x voltage safety margin
            temperature_derating: 0.9,   // 10% temperature derating
            current_safety_margin: 1.2, // 20% current safety margin
        }
    }
}

impl SafetyFactors {
    /// Create safety factors for automotive applications
    pub fn automotive() -> Self {
        Self {
            power_derating: 0.6,        // More conservative for automotive
            voltage_safety_margin: 2.5, // Higher voltage margins
            temperature_derating: 0.8,   // Account for engine heat
            current_safety_margin: 1.3, // Higher current margins
        }
    }
    
    /// Create safety factors for industrial applications
    pub fn industrial() -> Self {
        Self {
            power_derating: 0.65,       // Industrial-grade derating
            voltage_safety_margin: 2.2, // Industrial voltage margins
            temperature_derating: 0.85,  // Wide temperature range
            current_safety_margin: 1.25,// Industrial current margins
        }
    }
}

/// Main passive component calculation engine
pub struct PassiveComponentCalculator {
    safety_factors: SafetyFactors,
}

impl PassiveComponentCalculator {
    /// Create new calculator with default safety factors
    pub fn new() -> Self {
        Self {
            safety_factors: SafetyFactors::default(),
        }
    }
    
    /// Create calculator with custom safety factors
    pub fn with_safety_factors(safety_factors: SafetyFactors) -> Self {
        Self { safety_factors }
    }
    
    /// Calculate required resistor power rating with safety margins
    /// Uses I²R power dissipation calculation
    pub fn calculate_resistor_power_rating(
        &self,
        resistance: f64,
        current: f64,
    ) -> PowerRating {
        let power_dissipated = current * current * resistance; // I²R
        let derated_power = power_dissipated / self.safety_factors.power_derating;
        self.select_next_standard_power_rating(derated_power)
    }
    
    /// Calculate required resistor power rating from voltage and resistance
    /// Uses V²/R power dissipation calculation  
    pub fn calculate_resistor_power_rating_from_voltage(
        &self,
        resistance: f64,
        voltage: f64,
    ) -> PowerRating {
        let power_dissipated = voltage * voltage / resistance; // V²/R
        let derated_power = power_dissipated / self.safety_factors.power_derating;
        self.select_next_standard_power_rating(derated_power)
    }
    
    /// Calculate required capacitor voltage rating with safety margins
    pub fn calculate_capacitor_voltage_rating(
        &self,
        operating_voltage: f64,
    ) -> VoltageRating {
        let safety_voltage = operating_voltage * self.safety_factors.voltage_safety_margin;
        self.select_next_standard_voltage_rating(safety_voltage)
    }
    
    /// Calculate resistor voltage rating (typically not critical but good practice)
    pub fn calculate_resistor_voltage_rating(
        &self,
        operating_voltage: f64,
    ) -> VoltageRating {
        // Resistors typically need less voltage derating than capacitors
        let safety_voltage = operating_voltage * 1.5; // 1.5x margin for resistors
        self.select_next_standard_voltage_rating(safety_voltage)
    }
    
    /// Select next higher standard power rating
    fn select_next_standard_power_rating(&self, required_power: f64) -> PowerRating {
        if required_power <= 0.0625 {
            PowerRating::P62mW
        } else if required_power <= 0.100 {
            PowerRating::P100mW
        } else if required_power <= 0.125 {
            PowerRating::P125mW
        } else if required_power <= 0.250 {
            PowerRating::P250mW
        } else if required_power <= 0.500 {
            PowerRating::P500mW
        } else if required_power <= 1.0 {
            PowerRating::P1W
        } else if required_power <= 2.0 {
            PowerRating::P2W
        } else if required_power <= 5.0 {
            PowerRating::P5W
        } else {
            PowerRating::P10W
        }
    }
    
    /// Select next higher standard voltage rating
    fn select_next_standard_voltage_rating(&self, required_voltage: f64) -> VoltageRating {
        if required_voltage <= 6.3 {
            VoltageRating::V6_3
        } else if required_voltage <= 10.0 {
            VoltageRating::V10
        } else if required_voltage <= 16.0 {
            VoltageRating::V16
        } else if required_voltage <= 25.0 {
            VoltageRating::V25
        } else if required_voltage <= 35.0 {
            VoltageRating::V35
        } else if required_voltage <= 50.0 {
            VoltageRating::V50
        } else if required_voltage <= 63.0 {
            VoltageRating::V63
        } else if required_voltage <= 100.0 {
            VoltageRating::V100
        } else if required_voltage <= 200.0 {
            VoltageRating::V200
        } else if required_voltage <= 400.0 {
            VoltageRating::V400
        } else {
            VoltageRating::V630
        }
    }
    
    /// Calculate current through resistor for given voltage
    pub fn calculate_resistor_current(&self, voltage: f64, resistance: f64) -> f64 {
        voltage / resistance // Ohm's law: I = V/R
    }
    
    /// Calculate resistance for desired current limiting
    pub fn calculate_current_limiting_resistance(&self, voltage: f64, max_current: f64) -> f64 {
        let safe_current = max_current / self.safety_factors.current_safety_margin;
        voltage / safe_current // R = V/I
    }
    
    /// Calculate capacitor ESR requirements for ripple current
    pub fn calculate_capacitor_esr_requirement(
        &self,
        ripple_current: f64,
        max_temp_rise: f64, // °C
    ) -> f64 {
        // ESR limit based on I²R heating: ESR < ΔT / (I²rms * Rth)
        // Assuming typical thermal resistance of 100°C/W for SMD capacitors
        let thermal_resistance = 100.0; // °C/W
        max_temp_rise / (ripple_current * ripple_current * thermal_resistance)
    }
    
    // ==================== SIMULATION INTEGRATION METHODS ====================
    
    /// Enhanced resistor calculation using actual simulation data instead of estimates
    pub fn calculate_resistor_spec_from_simulation(
        &self,
        component_name: &str,
        analysis_result: &bhdl_analyzer::AnalysisResult,
        design_intent: Option<&bhdl_common::IntentCall>,
    ) -> Result<(PowerRating, VoltageRating, f64), Box<dyn std::error::Error>> {
        
        // Use unified simulation data - single source of truth for all simulation results
        let simulation_data = &analysis_result.simulation_data;
        
        // Extract actual operating conditions from unified simulation data
        let actual_current = simulation_data.get_operating_current(component_name)
            .unwrap_or_else(|| {
                // Fallback: estimate from design intent or context
                self.estimate_current_from_intent(design_intent, analysis_result)
            });
            
        let actual_voltage = simulation_data.get_operating_voltage(component_name)
            .unwrap_or_else(|| {
                // Fallback: get from power domain analysis
                self.estimate_voltage_from_power_domains(analysis_result)
            });
            
        let actual_power = simulation_data.get_power_dissipation(component_name)
            .unwrap_or_else(|| {
                // Fallback: calculate I²R from simulated values
                if actual_current > 0.0 {
                    actual_current * actual_current * (actual_voltage / actual_current)
                } else {
                    0.0
                }
            });
            
        // Get comprehensive derating factor from unified simulation (includes all safety analysis)
        let simulation_derating_factor = simulation_data.get_derating_factor(component_name);
        let enhanced_safety_factors = self.get_safety_enhanced_factors(analysis_result);
        
        // Calculate power rating using actual simulated power with comprehensive derating
        let total_power_derating = enhanced_safety_factors.power_derating * simulation_derating_factor;
        let required_power = actual_power / total_power_derating;
        let power_rating = self.select_next_standard_power_rating(required_power);
        
        // Calculate voltage rating using actual simulated voltage
        let required_voltage = actual_voltage * enhanced_safety_factors.voltage_safety_margin;
        let voltage_rating = self.select_next_standard_voltage_rating(required_voltage);
        
        // Calculate optimal resistance from simulation results
        let optimal_resistance = actual_voltage / actual_current;
        
        Ok((power_rating, voltage_rating, optimal_resistance))
    }
    
    /// Enhanced capacitor calculation using simulation data for ripple current analysis
    pub fn calculate_capacitor_spec_from_simulation(
        &self,
        component_name: &str,
        analysis_result: &bhdl_analyzer::AnalysisResult,
        design_intent: Option<&bhdl_common::IntentCall>,
    ) -> Result<(VoltageRating, DielectricType, f64), Box<dyn std::error::Error>> {
        
        // Use unified simulation data for all capacitor analysis
        let simulation_data = &analysis_result.simulation_data;
        
        // Extract ripple current from unified transient analysis (if available)
        let ripple_current = if let Some(ref transient) = simulation_data.transient_analysis {
            transient.ripple_currents.get(component_name).copied().unwrap_or_else(|| {
                // Fallback: estimate from power analysis
                self.estimate_ripple_from_power_analysis(analysis_result)
            })
        } else {
            // Fallback: estimate from power analysis
            self.estimate_ripple_from_power_analysis(analysis_result)  
        };
            
        // Extract actual operating voltage from unified simulation
        let operating_voltage = simulation_data.get_operating_voltage(component_name)
            .unwrap_or_else(|| {
                // Fallback: get from power domain analysis
                self.estimate_voltage_from_power_domains(analysis_result)
            });
            
        // Extract frequency requirements from unified AC analysis (if available)
        let frequency_range = if let Some(ref ac) = simulation_data.ac_analysis {
            if let Some(bandwidth) = ac.bandwidth.get(component_name) {
                (1.0, *bandwidth)
            } else {
                self.extract_frequency_requirements(analysis_result, design_intent)
            }
        } else {
            self.extract_frequency_requirements(analysis_result, design_intent)
        };
        
        // Get comprehensive derating from unified simulation
        let simulation_derating_factor = simulation_data.get_derating_factor(component_name);
        let enhanced_safety_factors = self.get_safety_enhanced_factors(analysis_result);
        
        // Calculate voltage rating with comprehensive simulation-based derating
        let total_voltage_derating = enhanced_safety_factors.voltage_safety_margin / simulation_derating_factor;
        let required_voltage = operating_voltage * total_voltage_derating;
        let voltage_rating = self.select_next_standard_voltage_rating(required_voltage);
        
        // Select dielectric type based on unified simulation thermal analysis
        let dielectric = if let Some(ref thermal) = simulation_data.thermal_analysis {
            if let Some(operating_temp) = thermal.component_temperatures.get(component_name) {
                if *operating_temp > 125.0 || frequency_range.1 > 10e6 {
                    // Ultra-high temperature or high frequency - use most stable dielectric
                    DielectricType::C0G
                } else if *operating_temp > 85.0 || frequency_range.1 > 1e6 {
                    // High temperature or medium frequency - use X7R
                    DielectricType::X7R
                } else {
                    // Normal conditions - use X7R for good balance
                    DielectricType::X7R
                }
            } else if frequency_range.1 > 10e6 {
                // No thermal data but high frequency
                DielectricType::C0G
            } else {
                DielectricType::X7R
            }
        } else {
            // No thermal analysis - use frequency-based selection
            if frequency_range.1 > 10e6 {
                DielectricType::C0G
            } else {
                DielectricType::X7R
            }
        };
        
        // Calculate ESR requirements from actual ripple current
        let max_esr = self.calculate_capacitor_esr_requirement(ripple_current, 10.0); // 10°C max temp rise
        
        Ok((voltage_rating, dielectric, max_esr))
    }
    
    /// Extract actual current from SPICE DC operating point analysis
    fn extract_actual_current_from_spice(
        &self,
        component_name: &str,
        analysis_result: &bhdl_analyzer::AnalysisResult,
    ) -> Option<f64> {
        // Check if we have component inference results with current data
        let inferred_components = analysis_result.component_inference.get_inferred_components();
        
        for component in inferred_components {
            if let Some(instance_name) = &component.instance_name {
                if instance_name.contains(component_name) {
                    // Look for current parameters in the inferred parameters
                    for param in &component.parameters {
                        if param.name == "current" || param.name == "calculated_current" {
                            match &param.value {
                                bhdl_analyzer::component_inference::ParameterValue::Current(current) => {
                                    return Some(*current);
                                },
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        
        None
    }
    
    /// Extract actual voltage from SPICE DC analysis
    fn extract_actual_voltage_from_spice(
        &self,
        component_name: &str,
        analysis_result: &bhdl_analyzer::AnalysisResult,
    ) -> Option<f64> {
        // Check power domain analysis for actual voltages
        let power_domains = &analysis_result.power_analysis.domains;
        
        for (domain_name, domain_info) in power_domains {
            if component_name.contains(&domain_name.to_lowercase()) {
                return Some(domain_info.voltage);
            }
        }
        
        None
    }
    
    /// Extract actual power dissipation from SPICE calculations
    fn extract_actual_power_from_spice(
        &self,
        component_name: &str,
        analysis_result: &bhdl_analyzer::AnalysisResult,
    ) -> Option<f64> {
        // For now, calculate from current and voltage if available
        let current = self.extract_actual_current_from_spice(component_name, analysis_result)?;
        let voltage = self.extract_actual_voltage_from_spice(component_name, analysis_result)?;
        
        // P = I²R, but we need R. For now, estimate typical values
        // This would be enhanced with actual SPICE netlist analysis
        let estimated_resistance = voltage / current;
        Some(current * current * estimated_resistance)
    }
    
    /// Get safety factors enhanced by safety analysis violations
    fn get_safety_enhanced_factors(
        &self,
        analysis_result: &bhdl_analyzer::AnalysisResult,
    ) -> SafetyFactors {
        let mut enhanced_factors = self.safety_factors.clone();
        
        // Apply additional derating based on safety diagnostics
        for diagnostic in &analysis_result.safety_analysis.diagnostics {
            // For now, apply conservative derating for any safety diagnostic
            enhanced_factors.power_derating *= 0.8; // 20% additional derating
            enhanced_factors.voltage_safety_margin *= 1.2; // 20% higher voltage margin
        }
        
        enhanced_factors
    }
    
    /// Estimate current from design intent if SPICE data not available
    fn estimate_current_from_intent(
        &self,
        design_intent: Option<&bhdl_common::IntentCall>,
        analysis_result: &bhdl_analyzer::AnalysisResult,
    ) -> f64 {
        if let Some(intent) = design_intent {
            // Extract current from intent parameters
            for param in &intent.params {
                if let bhdl_common::IntentParam::Named(name, value) = param {
                    if name == "current_limit" || name == "max_current" {
                        if let bhdl_common::IntentValue::Number(current, Some(unit)) = value {
                            if unit == "A" || unit == "mA" {
                                let current_amps = if unit == "mA" { current / 1000.0 } else { *current };
                                return current_amps;
                            }
                        }
                    }
                }
            }
        }
        
        // Fallback: estimate from component inference
        0.020 // 20mA default
    }
    
    /// Estimate voltage from power domain analysis
    fn estimate_voltage_from_power_domains(&self, analysis_result: &bhdl_analyzer::AnalysisResult) -> f64 {
        let power_domains = &analysis_result.power_analysis.domains;

        // Conservative default: the highest domain voltage (name-tie-broken so
        // the estimate is deterministic — domains is a HashMap).
        power_domains
            .iter()
            .max_by(|(an, a), (bn, b)| {
                a.voltage
                    .partial_cmp(&b.voltage)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| an.cmp(bn))
            })
            .map(|(_, domain_info)| domain_info.voltage)
            .unwrap_or(5.0) // 5V default
    }
    
    /// Extract ripple current from transient analysis (placeholder)
    fn extract_ripple_current_from_transient(
        &self,
        _component_name: &str,
        _analysis_result: &bhdl_analyzer::AnalysisResult,
    ) -> Option<f64> {
        // TODO: Implement when transient analysis is available
        None
    }
    
    /// Estimate ripple current from power analysis
    fn estimate_ripple_from_power_analysis(&self, analysis_result: &bhdl_analyzer::AnalysisResult) -> f64 {
        let power_domains = &analysis_result.power_analysis.domains;

        // Estimate ripple as 10% of the largest domain's max current
        // (name-tie-broken so the estimate is deterministic — domains is a
        // HashMap).
        power_domains
            .iter()
            .max_by(|(an, a), (bn, b)| {
                a.max_current
                    .partial_cmp(&b.max_current)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| an.cmp(bn))
            })
            .map(|(_, domain_info)| domain_info.max_current * 0.1)
            .unwrap_or(0.010) // 10mA default ripple
    }
    
    /// Extract frequency requirements from analysis and intent
    fn extract_frequency_requirements(
        &self,
        _analysis_result: &bhdl_analyzer::AnalysisResult,
        design_intent: Option<&bhdl_common::IntentCall>,
    ) -> (f64, f64) {
        if let Some(intent) = design_intent {
            // Check for frequency-related intents
            match intent.name.as_str() {
                "noise_filtering" | "anti_alias" => (1.0, 100e3), // Audio range
                "fast_response" | "precision_measurement" => (1.0, 10e6), // High frequency
                "signal_buffering" | "output_buffering" => (1.0, 1e6), // Digital range
                _ => (1.0, 100e3), // Default range
            }
        } else {
            (1.0, 100e3) // Default: DC to 100kHz
        }
    }
}

impl Default for PassiveComponentCalculator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_resistor_power_calculation() {
        let calculator = PassiveComponentCalculator::new();
        
        // Test low power: 10mA through 1kΩ = 100mW
        let power_rating = calculator.calculate_resistor_power_rating(1000.0, 0.010);
        assert_eq!(power_rating, PowerRating::P250mW); // Next higher from 100mW/0.7 ≈ 143mW
        
        // Test high power: 100mA through 100Ω = 1W  
        let power_rating = calculator.calculate_resistor_power_rating(100.0, 0.100);
        assert_eq!(power_rating, PowerRating::P2W); // Next higher from 1W/0.7 ≈ 1.43W
    }
    
    #[test]
    fn test_capacitor_voltage_calculation() {
        let calculator = PassiveComponentCalculator::new();
        
        // Test 3.3V operation -> should select 10V rating (3.3V * 2 = 6.6V)
        let voltage_rating = calculator.calculate_capacitor_voltage_rating(3.3);
        assert_eq!(voltage_rating, VoltageRating::V10);
        
        // Test 5V operation -> should select 10V rating (5V * 2 = 10V)
        let voltage_rating = calculator.calculate_capacitor_voltage_rating(5.0);
        assert_eq!(voltage_rating, VoltageRating::V10);
        
        // Test 12V operation -> should select 25V rating (12V * 2 = 24V)
        let voltage_rating = calculator.calculate_capacitor_voltage_rating(12.0);
        assert_eq!(voltage_rating, VoltageRating::V25);
    }
    
    #[test]
    fn test_current_limiting_resistance() {
        let calculator = PassiveComponentCalculator::new();
        
        // Test 5V with 100mA limit -> should calculate ~60Ω (with safety margin)
        let resistance = calculator.calculate_current_limiting_resistance(5.0, 0.100);
        assert!((resistance - 60.0).abs() < 5.0); // Allow 5Ω tolerance
    }
    
    #[test]
    fn test_automotive_safety_factors() {
        let automotive_calc = PassiveComponentCalculator::with_safety_factors(SafetyFactors::automotive());
        let standard_calc = PassiveComponentCalculator::new();
        
        // Automotive should be more conservative
        let auto_power = automotive_calc.calculate_resistor_power_rating(1000.0, 0.010);
        let std_power = standard_calc.calculate_resistor_power_rating(1000.0, 0.010);
        
        assert!(auto_power >= std_power); // Automotive should select same or higher rating
    }
    
    #[test]
    fn test_power_rating_progression() {
        let mut power = PowerRating::P62mW;
        let expected_progression = [
            PowerRating::P62mW,
            PowerRating::P100mW,
            PowerRating::P125mW,
            PowerRating::P250mW,
            PowerRating::P500mW,
            PowerRating::P1W,
            PowerRating::P2W,
            PowerRating::P5W,
            PowerRating::P10W,
        ];
        
        for (i, expected) in expected_progression.iter().enumerate() {
            assert_eq!(power, *expected, "Mismatch at index {}", i);
            if i < expected_progression.len() - 1 {
                power = power.next_higher();
            }
        }
    }
}