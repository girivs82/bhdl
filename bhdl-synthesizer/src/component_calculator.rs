// Component Value Calculation Engine
// Calculates real component values using engineering formulas and design rules

use std::collections::HashMap;
use serde::{Serialize, Deserialize};

/// Calculated component with engineering rationale
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalculatedComponent {
    /// Component reference designator (e.g., "C1", "R2", "L1")
    pub reference: String,
    /// Component type (capacitor, resistor, inductor, etc.)
    pub component_type: ComponentType,
    /// Calculated value with units
    pub value: String,
    /// Voltage/power rating
    pub rating: String,
    /// Package/tolerance specifications
    pub package: String,
    /// Engineering justification for the value
    pub purpose: String,
    /// Mathematical calculation used
    pub calculation: String,
    /// Placement guidance
    pub placement: String,
    /// Design intent category
    pub intent: ComponentIntent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ComponentType {
    Capacitor,
    Resistor,
    Inductor,
    Diode,
    LED,
    Fuse,
    Crystal,
    Connector,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComponentIntent {
    InputFiltering,
    OutputFiltering,
    Decoupling,
    FeedbackControl,
    Protection,
    Indication,
    EnergyStorage,
    NoiseReduction,
    Compensation,
    CurrentLimiting,
}

/// Power supply design parameters
#[derive(Debug, Clone)]
pub struct PowerSupplySpec {
    pub input_voltage: f64,      // V
    pub output_voltage: f64,     // V
    pub output_current: f64,     // A
    pub switching_frequency: f64, // Hz
    pub ripple_spec: f64,        // V (peak-to-peak)
    pub transient_spec: f64,     // µs (settling time)
    pub efficiency_target: f64,  // 0.0 to 1.0
}

pub struct ComponentCalculator {
    design_rules: DesignRules,
}

#[derive(Debug, Clone)]
pub struct DesignRules {
    // Capacitor design rules
    pub cap_voltage_derating: f64,      // 0.8 = 80% voltage derating
    pub cap_ripple_current_derating: f64, // 0.7 = 70% ripple current derating
    
    // Inductor design rules
    pub inductor_saturation_derating: f64, // 0.8 = 80% saturation current derating
    pub inductor_ripple_ratio: f64,      // 0.3 = 30% current ripple
    
    // Resistor design rules
    pub resistor_power_derating: f64,    // 0.5 = 50% power derating
    pub feedback_reference_voltage: f64, // 0.8V typical for buck controllers
    
    // Thermal design rules
    pub ambient_temperature: f64,       // °C
    pub max_junction_temperature: f64,  // °C
}

impl Default for DesignRules {
    fn default() -> Self {
        Self {
            cap_voltage_derating: 0.8,
            cap_ripple_current_derating: 0.7,
            inductor_saturation_derating: 0.8,
            inductor_ripple_ratio: 0.3,
            resistor_power_derating: 0.5,
            feedback_reference_voltage: 0.8,
            ambient_temperature: 25.0,
            max_junction_temperature: 125.0,
        }
    }
}

impl ComponentCalculator {
    pub fn new() -> Self {
        Self {
            design_rules: DesignRules::default(),
        }
    }
    
    /// Calculate all supporting components for a buck converter
    pub fn calculate_buck_converter_components(&self, spec: &PowerSupplySpec, ic_name: &str) -> Vec<CalculatedComponent> {
        let mut components = Vec::new();
        let mut ref_counter = ComponentReferenceCounter::new();
        
        // Calculate input stage components
        components.extend(self.calculate_input_stage(spec, &mut ref_counter));
        
        // Calculate switching stage components  
        components.extend(self.calculate_switching_stage(spec, &mut ref_counter));
        
        // Calculate output stage components
        components.extend(self.calculate_output_stage(spec, &mut ref_counter));
        
        // Calculate feedback network
        components.extend(self.calculate_feedback_network(spec, &mut ref_counter));
        
        // Calculate protection and indication
        components.extend(self.calculate_protection_circuits(spec, &mut ref_counter));
        
        components
    }
    
    /// Calculate input filtering components
    fn calculate_input_stage(&self, spec: &PowerSupplySpec, refs: &mut ComponentReferenceCounter) -> Vec<CalculatedComponent> {
        let mut components = Vec::new();
        
        // Input ceramic capacitor for high-frequency filtering
        let input_ceramic_value = self.calculate_input_ceramic_capacitor(spec);
        let input_ceramic_voltage = (spec.input_voltage / self.design_rules.cap_voltage_derating).ceil() as u32;
        let ceramic_voltage_rating = self.standard_voltage_rating(input_ceramic_voltage);
        
        components.push(CalculatedComponent {
            reference: refs.next_capacitor(),
            component_type: ComponentType::Capacitor,
            value: format!("{:.1}µF", input_ceramic_value * 1e6),
            rating: format!("{}V", ceramic_voltage_rating),
            package: "X7R ceramic, 1206".to_string(),
            purpose: "High-frequency input filtering and switching noise reduction".to_string(),
            calculation: format!("C = I_ripple / (f_sw × ΔV_ripple) = {}A / ({}kHz × 0.1V) = {:.1}µF", 
                               spec.output_current * 0.1, spec.switching_frequency / 1000.0, input_ceramic_value * 1e6),
            placement: "Close to IC VIN pin, minimize loop area".to_string(),
            intent: ComponentIntent::InputFiltering,
        });
        
        // Input bulk electrolytic capacitor
        let bulk_cap_value = self.calculate_input_bulk_capacitor(spec);
        
        components.push(CalculatedComponent {
            reference: refs.next_capacitor(),
            component_type: ComponentType::Capacitor, 
            value: format!("{:.0}µF", bulk_cap_value * 1e6),
            rating: format!("{}V", ceramic_voltage_rating),
            package: "Low-ESR electrolytic".to_string(),
            purpose: "Bulk energy storage and low-frequency ripple filtering".to_string(),
            calculation: format!("C_bulk = I_load × t_holdup / ΔV_sag = {}A × 100µs / 0.5V = {:.0}µF", 
                               spec.output_current, bulk_cap_value * 1e6),
            placement: "Input power entry point, shared with multiple converters".to_string(),
            intent: ComponentIntent::EnergyStorage,
        });
        
        // Input protection TVS diode
        let tvs_voltage = (spec.input_voltage * 1.2).ceil() as u32;
        
        components.push(CalculatedComponent {
            reference: refs.next_diode(),
            component_type: ComponentType::Diode,
            value: format!("{}V TVS", tvs_voltage),
            rating: format!("1.5A peak", ),
            package: "DO-214AC (SMA)".to_string(),
            purpose: "Input overvoltage protection".to_string(),
            calculation: format!("V_clamp = V_in_max × 1.2 = {}V × 1.2 = {}V", 
                               spec.input_voltage, tvs_voltage),
            placement: "Across input power, close to connector".to_string(),
            intent: ComponentIntent::Protection,
        });
        
        components
    }
    
    /// Calculate switching stage inductor
    fn calculate_switching_stage(&self, spec: &PowerSupplySpec, refs: &mut ComponentReferenceCounter) -> Vec<CalculatedComponent> {
        let mut components = Vec::new();
        
        // Power inductor calculation using buck converter formula
        let inductance = self.calculate_power_inductor(spec);
        let inductor_current_rating = spec.output_current / self.design_rules.inductor_saturation_derating;
        
        components.push(CalculatedComponent {
            reference: refs.next_inductor(),
            component_type: ComponentType::Inductor,
            value: format!("{:.1}µH", inductance * 1e6),
            rating: format!("{:.1}A saturation", inductor_current_rating),
            package: "Power inductor, shielded".to_string(),
            purpose: "Energy storage for buck conversion".to_string(),
            calculation: format!("L = (V_in - V_out) × D / (ΔI × f_sw) = ({:.1}V - {:.1}V) × {:.3} / ({:.2}A × {:.0}kHz) = {:.1}µH",
                               spec.input_voltage, spec.output_voltage, 
                               spec.output_voltage / spec.input_voltage, // Duty cycle
                               spec.output_current * self.design_rules.inductor_ripple_ratio,
                               spec.switching_frequency / 1000.0, inductance * 1e6),
            placement: "SW pin to output, short wide traces, away from sensitive signals".to_string(),
            intent: ComponentIntent::EnergyStorage,
        });
        
        components
    }
    
    /// Calculate output filtering components
    fn calculate_output_stage(&self, spec: &PowerSupplySpec, refs: &mut ComponentReferenceCounter) -> Vec<CalculatedComponent> {
        let mut components = Vec::new();
        
        // Output ceramic capacitor for ripple reduction
        let output_ceramic = self.calculate_output_ceramic_capacitor(spec);
        let output_voltage_rating = self.standard_voltage_rating((spec.output_voltage * 2.0).ceil() as u32);
        
        components.push(CalculatedComponent {
            reference: refs.next_capacitor(),
            component_type: ComponentType::Capacitor,
            value: format!("{:.0}µF", output_ceramic * 1e6),
            rating: format!("{}V", output_voltage_rating),
            package: "X7R ceramic, 1210".to_string(),
            purpose: "Output ripple reduction and high-frequency noise filtering".to_string(),
            calculation: format!("C = ΔI / (8 × f_sw × ΔV_ripple) = {:.2}A / (8 × {}kHz × {}mV) = {:.0}µF",
                               spec.output_current * self.design_rules.inductor_ripple_ratio,
                               spec.switching_frequency / 1000.0, spec.ripple_spec * 1000.0,
                               output_ceramic * 1e6),
            placement: "Close to load connection point".to_string(),
            intent: ComponentIntent::OutputFiltering,
        });
        
        // Output bulk electrolytic for transient response
        let output_bulk = self.calculate_output_bulk_capacitor(spec);
        
        components.push(CalculatedComponent {
            reference: refs.next_capacitor(),
            component_type: ComponentType::Capacitor,
            value: format!("{:.0}µF", output_bulk * 1e6),
            rating: format!("{}V", output_voltage_rating),
            package: "Low-ESR electrolytic".to_string(),
            purpose: "Transient response and bulk energy storage".to_string(),
            calculation: format!("C_bulk = I_step × t_settle / ΔV_droop = {}A × {}µs / 100mV = {:.0}µF",
                               spec.output_current, spec.transient_spec, output_bulk * 1e6),
            placement: "Load-side bulk storage, minimize ESR path to load".to_string(),
            intent: ComponentIntent::EnergyStorage,
        });
        
        components
    }
    
    /// Calculate feedback resistor network
    fn calculate_feedback_network(&self, spec: &PowerSupplySpec, refs: &mut ComponentReferenceCounter) -> Vec<CalculatedComponent> {
        let mut components = Vec::new();
        
        // Standard feedback resistor values (E96 series preferred)
        let r1_value = 10000.0; // 10kΩ standard upper resistor
        let r2_value = r1_value / ((spec.output_voltage / self.design_rules.feedback_reference_voltage) - 1.0);
        let r2_standard = self.standard_resistor_value(r2_value);
        
        components.push(CalculatedComponent {
            reference: refs.next_resistor(),
            component_type: ComponentType::Resistor,
            value: format!("{:.0}kΩ", r1_value / 1000.0),
            rating: "1/8W".to_string(),
            package: "1% precision, 0805".to_string(),
            purpose: "Feedback voltage divider upper resistor".to_string(),
            calculation: "Standard value for feedback networks, optimizes noise vs. power".to_string(),
            placement: "Close to FB pin, short traces".to_string(),
            intent: ComponentIntent::FeedbackControl,
        });
        
        components.push(CalculatedComponent {
            reference: refs.next_resistor(),
            component_type: ComponentType::Resistor,
            value: format!("{:.2}kΩ", r2_standard / 1000.0),
            rating: "1/8W".to_string(),
            package: "1% precision, 0805".to_string(),
            purpose: "Feedback voltage divider lower resistor".to_string(),
            calculation: format!("R2 = R1 / ((V_out/V_ref) - 1) = {}kΩ / (({:.1}V/{:.1}V) - 1) = {:.2}kΩ",
                               r1_value / 1000.0, spec.output_voltage, 
                               self.design_rules.feedback_reference_voltage, r2_standard / 1000.0),
            placement: "FB pin to ground, close to R1".to_string(),
            intent: ComponentIntent::FeedbackControl,
        });
        
        // Compensation capacitor for stability
        let comp_cap_value = 10e-12; // 10pF typical
        
        components.push(CalculatedComponent {
            reference: refs.next_capacitor(),
            component_type: ComponentType::Capacitor,
            value: format!("{:.0}pF", comp_cap_value * 1e12),
            rating: format!("{}V", self.standard_voltage_rating((spec.output_voltage * 2.0).ceil() as u32)),
            package: "NP0 ceramic, 0805".to_string(),
            purpose: "Loop compensation and high-frequency stability".to_string(),
            calculation: "Empirical value for Type II compensation, provides zero at f_z = 1/(2πRC)".to_string(),
            placement: "Parallel with R1, minimize parasitic inductance".to_string(),
            intent: ComponentIntent::Compensation,
        });
        
        components
    }
    
    /// Calculate protection and indication circuits
    fn calculate_protection_circuits(&self, spec: &PowerSupplySpec, refs: &mut ComponentReferenceCounter) -> Vec<CalculatedComponent> {
        let mut components = Vec::new();
        
        // Power indicator LED with current limiting resistor
        let led_forward_voltage = 2.1; // Green LED typical
        let led_current = 0.002; // 2mA
        let led_resistor = (spec.output_voltage - led_forward_voltage) / led_current;
        let led_resistor_standard = self.standard_resistor_value(led_resistor);
        let resistor_power = led_current * led_current * led_resistor_standard;
        let resistor_rating = if resistor_power > 0.125 { "1/4W" } else { "1/8W" };
        
        components.push(CalculatedComponent {
            reference: refs.next_resistor(),
            component_type: ComponentType::Resistor,
            value: format!("{:.1}kΩ", led_resistor_standard / 1000.0),
            rating: resistor_rating.to_string(),
            package: "5% tolerance, 0805".to_string(),
            purpose: "LED current limiting".to_string(),
            calculation: format!("R = (V_supply - V_led) / I_led = ({:.1}V - {:.1}V) / {}mA = {:.1}kΩ",
                               spec.output_voltage, led_forward_voltage, led_current * 1000.0, 
                               led_resistor_standard / 1000.0),
            placement: "Series with LED, any convenient location".to_string(),
            intent: ComponentIntent::CurrentLimiting,
        });
        
        components.push(CalculatedComponent {
            reference: refs.next_led(),
            component_type: ComponentType::LED,
            value: "Green".to_string(),
            rating: "2mA, 2.1V forward".to_string(),
            package: "0805 SMD".to_string(),
            purpose: "Power status indication".to_string(),
            calculation: "Standard green LED for visual power confirmation".to_string(),
            placement: "Visible location, consider light pipe if enclosure blocks view".to_string(),
            intent: ComponentIntent::Indication,
        });
        
        components
    }
    
    // Component value calculation methods
    
    fn calculate_input_ceramic_capacitor(&self, spec: &PowerSupplySpec) -> f64 {
        // Input ceramic cap sized for switching current ripple
        // I_ripple ≈ I_out × 0.1 (10% of output current)  
        // C = I_ripple / (f_sw × ΔV_ripple)
        let ripple_current = spec.output_current * 0.1;
        let ripple_voltage = 0.1; // 100mV ripple allowance
        let capacitance = ripple_current / (spec.switching_frequency * ripple_voltage);
        
        // Round up to nearest standard value
        self.standard_capacitor_value(capacitance)
    }
    
    fn calculate_input_bulk_capacitor(&self, spec: &PowerSupplySpec) -> f64 {
        // Bulk cap sized for holdup time during input voltage sag
        // C = I × t / ΔV, where t = 100µs typical, ΔV = 0.5V allowable sag
        let holdup_time = 100e-6; // 100µs
        let voltage_sag = 0.5; // 0.5V allowable
        let capacitance = spec.output_current * holdup_time / voltage_sag;
        
        self.standard_capacitor_value(capacitance)
    }
    
    fn calculate_power_inductor(&self, spec: &PowerSupplySpec) -> f64 {
        // Buck converter inductor: L = (V_in - V_out) × D / (ΔI × f_sw)
        let duty_cycle = spec.output_voltage / spec.input_voltage;
        let ripple_current = spec.output_current * self.design_rules.inductor_ripple_ratio;
        let inductance = (spec.input_voltage - spec.output_voltage) * duty_cycle / 
                        (ripple_current * spec.switching_frequency);
        
        self.standard_inductor_value(inductance)
    }
    
    fn calculate_output_ceramic_capacitor(&self, spec: &PowerSupplySpec) -> f64 {
        // Output ripple: ΔV = ΔI / (8 × f_sw × C)
        // Rearrange: C = ΔI / (8 × f_sw × ΔV)
        let ripple_current = spec.output_current * self.design_rules.inductor_ripple_ratio;
        let capacitance = ripple_current / (8.0 * spec.switching_frequency * spec.ripple_spec);
        
        self.standard_capacitor_value(capacitance)
    }
    
    fn calculate_output_bulk_capacitor(&self, spec: &PowerSupplySpec) -> f64 {
        // Transient response: C = I_step × t_settle / ΔV_droop
        let settling_time = spec.transient_spec * 1e-6; // Convert µs to seconds
        let voltage_droop = 0.1; // 100mV allowable droop
        let capacitance = spec.output_current * settling_time / voltage_droop;
        
        self.standard_capacitor_value(capacitance)
    }
    
    // Standard value lookup methods
    
    fn standard_capacitor_value(&self, calculated: f64) -> f64 {
        // E12 series capacitor values in µF
        let e12_values = [1.0, 1.2, 1.5, 1.8, 2.2, 2.7, 3.3, 3.9, 4.7, 5.6, 6.8, 8.2];
        let mut decades = 1e-12;
        
        loop {
            for &value in &e12_values {
                let standard_value = value * decades;
                if standard_value >= calculated {
                    return standard_value;
                }
            }
            decades *= 10.0;
            if decades > 1.0 {
                break;
            }
        }
        
        calculated // Fallback
    }
    
    fn standard_inductor_value(&self, calculated: f64) -> f64 {
        // E12 series inductor values
        let e12_values = [1.0, 1.2, 1.5, 1.8, 2.2, 2.7, 3.3, 3.9, 4.7, 5.6, 6.8, 8.2];
        let mut decades = 1e-9;
        
        loop {
            for &value in &e12_values {
                let standard_value = value * decades;
                if standard_value >= calculated {
                    return standard_value;
                }
            }
            decades *= 10.0;
            if decades > 1e-3 {
                break;
            }
        }
        
        calculated // Fallback
    }
    
    fn standard_resistor_value(&self, calculated: f64) -> f64 {
        // E96 series resistor values (1% tolerance)
        let e96_values = [1.00, 1.02, 1.05, 1.07, 1.10, 1.13, 1.15, 1.18, 1.21, 1.24, 1.27, 1.30,
                         1.33, 1.37, 1.40, 1.43, 1.47, 1.50, 1.54, 1.58, 1.62, 1.65, 1.69, 1.74,
                         1.78, 1.82, 1.87, 1.91, 1.96, 2.00, 2.05, 2.10, 2.15, 2.21, 2.26, 2.32,
                         2.37, 2.43, 2.49, 2.55, 2.61, 2.67, 2.74, 2.80, 2.87, 2.94, 3.01, 3.09,
                         3.16, 3.24, 3.32, 3.40, 3.48, 3.57, 3.65, 3.74, 3.83, 3.92, 4.02, 4.12,
                         4.22, 4.32, 4.42, 4.53, 4.64, 4.75, 4.87, 4.99, 5.11, 5.23, 5.36, 5.49,
                         5.62, 5.76, 5.90, 6.04, 6.19, 6.34, 6.49, 6.65, 6.81, 6.98, 7.15, 7.32,
                         7.50, 7.68, 7.87, 8.06, 8.25, 8.45, 8.66, 8.87, 9.09, 9.31, 9.53, 9.76];
        
        let mut decades = 1.0;
        
        // Find appropriate decade
        while calculated >= decades * 10.0 {
            decades *= 10.0;
        }
        while calculated < decades {
            decades /= 10.0;
        }
        
        // Find closest E96 value in this decade
        for &value in &e96_values {
            let standard_value = value * decades;
            if standard_value >= calculated {
                return standard_value;
            }
        }
        
        // Return first value in next decade
        e96_values[0] * decades * 10.0
    }
    
    fn standard_voltage_rating(&self, min_voltage: u32) -> u32 {
        // Standard voltage ratings for capacitors
        let standard_voltages = [6, 10, 16, 25, 35, 50, 63, 100, 160, 250, 400, 630];
        
        for &voltage in &standard_voltages {
            if voltage >= min_voltage {
                return voltage;
            }
        }
        
        min_voltage // Fallback
    }
}

/// Helper to generate component reference designators
struct ComponentReferenceCounter {
    capacitor_count: u32,
    resistor_count: u32,
    inductor_count: u32,
    diode_count: u32,
    led_count: u32,
}

impl ComponentReferenceCounter {
    fn new() -> Self {
        Self {
            capacitor_count: 0,
            resistor_count: 0,
            inductor_count: 0,
            diode_count: 0,
            led_count: 0,
        }
    }
    
    fn next_capacitor(&mut self) -> String {
        self.capacitor_count += 1;
        format!("C{}", self.capacitor_count)
    }
    
    fn next_resistor(&mut self) -> String {
        self.resistor_count += 1;
        format!("R{}", self.resistor_count)
    }
    
    fn next_inductor(&mut self) -> String {
        self.inductor_count += 1;
        format!("L{}", self.inductor_count)
    }
    
    fn next_diode(&mut self) -> String {
        self.diode_count += 1;
        format!("D{}", self.diode_count)
    }
    
    fn next_led(&mut self) -> String {
        self.led_count += 1;
        format!("LED{}", self.led_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_buck_converter_calculation() {
        let calc = ComponentCalculator::new();
        let spec = PowerSupplySpec {
            input_voltage: 12.0,
            output_voltage: 5.0,
            output_current: 1.5,
            switching_frequency: 300_000.0, // 300kHz
            ripple_spec: 0.020, // 20mVpp
            transient_spec: 50.0, // 50µs
            efficiency_target: 0.92,
        };
        
        let components = calc.calculate_buck_converter_components(&spec, "TPS54331");
        
        assert!(!components.is_empty());
        println!("Generated {} components", components.len());
        
        for component in &components {
            println!("{}: {} {}", component.reference, component.value, component.rating);
        }
    }
}