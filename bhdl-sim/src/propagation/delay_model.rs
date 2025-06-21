//! Pin-to-pin delay modeling

use std::collections::HashMap;

/// Models propagation delays in the circuit
pub struct DelayModel {
    /// Fixed delays between pins
    fixed_delays: HashMap<(String, String), f64>,
    
    /// Load-dependent delay coefficients
    load_coefficients: HashMap<String, LoadDelayCoeff>,
    
    /// Temperature coefficient (delay change per degree C)
    temp_coefficient: f64,
    
    /// Current temperature
    temperature: f64,
    
    /// Metrics
    metrics: DelayMetrics,
}

/// Propagation delay information
#[derive(Debug, Clone)]
pub struct PropagationDelay {
    pub from_pin: String,
    pub to_pin: String,
    pub nominal_delay: f64,
    pub actual_delay: f64,
    pub factors: DelayFactors,
}

/// Factors affecting delay
#[derive(Debug, Clone)]
pub struct DelayFactors {
    pub load_factor: f64,
    pub temp_factor: f64,
    pub voltage_factor: f64,
}

/// Load-dependent delay coefficients
#[derive(Debug, Clone)]
struct LoadDelayCoeff {
    /// Base delay at zero load
    base_delay: f64,
    /// Delay per unit load capacitance
    load_coefficient: f64,
}

/// Delay calculation metrics
#[derive(Debug, Default)]
struct DelayMetrics {
    calculations_performed: usize,
    total_delay_ps: f64,
    calculation_time_ms: f64,
}

impl DelayModel {
    /// Create a new delay model
    pub fn new() -> Self {
        Self {
            fixed_delays: HashMap::new(),
            load_coefficients: HashMap::new(),
            temp_coefficient: 0.002, // 0.2% per degree C
            temperature: 25.0, // Room temperature
            metrics: DelayMetrics::default(),
        }
    }
    
    /// Set temperature
    pub fn set_temperature(&mut self, temp: f64) {
        self.temperature = temp;
    }
    
    /// Add fixed delay between pins
    pub fn add_fixed_delay(&mut self, from: String, to: String, delay: f64) {
        self.fixed_delays.insert((from, to), delay);
    }
    
    /// Add load-dependent delay model
    pub fn add_load_delay(&mut self, pin: String, base: f64, coeff: f64) {
        self.load_coefficients.insert(pin, LoadDelayCoeff {
            base_delay: base,
            load_coefficient: coeff,
        });
    }
    
    /// Calculate propagation delay
    pub fn calculate_delay(
        &mut self,
        from_pin: &str,
        to_pin: &str,
        load_capacitance: f64,
        supply_voltage: f64,
        nominal_voltage: f64,
    ) -> PropagationDelay {
        let start = std::time::Instant::now();
        self.metrics.calculations_performed += 1;
        
        // Get base delay
        let nominal_delay = self.get_nominal_delay(from_pin, to_pin);
        
        // Calculate factors
        let load_factor = self.calculate_load_factor(to_pin, load_capacitance);
        let temp_factor = self.calculate_temp_factor();
        let voltage_factor = self.calculate_voltage_factor(supply_voltage, nominal_voltage);
        
        // Calculate actual delay
        let actual_delay = nominal_delay * load_factor * temp_factor * voltage_factor;
        
        self.metrics.total_delay_ps += actual_delay * 1e12; // Convert to ps
        self.metrics.calculation_time_ms += start.elapsed().as_secs_f64() * 1000.0;
        
        PropagationDelay {
            from_pin: from_pin.to_string(),
            to_pin: to_pin.to_string(),
            nominal_delay,
            actual_delay,
            factors: DelayFactors {
                load_factor,
                temp_factor,
                voltage_factor,
            },
        }
    }
    
    /// Get nominal delay between pins
    fn get_nominal_delay(&self, from: &str, to: &str) -> f64 {
        // Check for fixed delay
        if let Some(&delay) = self.fixed_delays.get(&(from.to_string(), to.to_string())) {
            return delay;
        }
        
        // Check for load-dependent delay
        if let Some(coeff) = self.load_coefficients.get(to) {
            return coeff.base_delay;
        }
        
        // Default delay (1ns)
        1e-9
    }
    
    /// Calculate load-dependent delay factor
    fn calculate_load_factor(&self, pin: &str, load_cap: f64) -> f64 {
        if let Some(coeff) = self.load_coefficients.get(pin) {
            1.0 + (coeff.load_coefficient * load_cap * 1e12) // pF conversion
        } else {
            1.0
        }
    }
    
    /// Calculate temperature-dependent delay factor
    fn calculate_temp_factor(&self) -> f64 {
        1.0 + self.temp_coefficient * (self.temperature - 25.0)
    }
    
    /// Calculate voltage-dependent delay factor
    fn calculate_voltage_factor(&self, actual: f64, nominal: f64) -> f64 {
        if actual > 0.0 && nominal > 0.0 {
            // Delay inversely proportional to voltage
            nominal / actual
        } else {
            1.0
        }
    }
    
    /// Create RC delay model
    pub fn rc_delay(resistance: f64, capacitance: f64) -> f64 {
        0.69 * resistance * capacitance // 0.69 = ln(2) for 50% threshold
    }
    
    /// Create transmission line delay
    pub fn transmission_line_delay(length: f64, velocity_factor: f64) -> f64 {
        // Delay = length / (c * velocity_factor)
        // c = 3e8 m/s
        length / (3e8 * velocity_factor)
    }
    
    /// Get average delay
    pub fn average_delay(&self) -> f64 {
        if self.metrics.calculations_performed > 0 {
            self.metrics.total_delay_ps / self.metrics.calculations_performed as f64
        } else {
            0.0
        }
    }
    
    /// Get metrics
    pub fn metrics(&self) -> &DelayMetrics {
        &self.metrics
    }
    
    /// Reset metrics
    pub fn reset_metrics(&mut self) {
        self.metrics = DelayMetrics::default();
    }
}

impl Default for DelayModel {
    fn default() -> Self {
        Self::new()
    }
}

/// Delay distribution for statistical analysis
#[derive(Debug, Clone)]
pub struct DelayDistribution {
    pub min: f64,
    pub typical: f64,
    pub max: f64,
    pub sigma: f64,
}

impl DelayDistribution {
    /// Create uniform distribution
    pub fn uniform(typical: f64, variation: f64) -> Self {
        Self {
            min: typical * (1.0 - variation),
            typical,
            max: typical * (1.0 + variation),
            sigma: typical * variation / 3.0, // Approximate
        }
    }
    
    /// Sample from distribution (using typical for now)
    pub fn sample(&self) -> f64 {
        // TODO: Implement proper statistical sampling
        self.typical
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_fixed_delay() {
        let mut model = DelayModel::new();
        model.add_fixed_delay("U1.OUT".to_string(), "U2.IN".to_string(), 5e-9);
        
        let delay = model.calculate_delay("U1.OUT", "U2.IN", 0.0, 5.0, 5.0);
        assert_eq!(delay.nominal_delay, 5e-9);
        assert_eq!(delay.actual_delay, 5e-9); // No modifying factors
    }
    
    #[test]
    fn test_load_dependent_delay() {
        let mut model = DelayModel::new();
        model.add_load_delay("U1.OUT".to_string(), 1e-9, 0.1); // 1ns + 0.1ns/pF
        
        let delay = model.calculate_delay("U1.IN", "U1.OUT", 10e-12, 5.0, 5.0); // 10pF load
        // Load factor = 1.0 + 0.1 * 10 = 2.0 (10pF load with 0.1ns/pF coefficient)
        assert!((delay.factors.load_factor - 2.0).abs() < 0.001);
    }
    
    #[test]
    fn test_temperature_effects() {
        let mut model = DelayModel::new();
        model.add_fixed_delay("A".to_string(), "B".to_string(), 1e-9);
        
        // Room temperature
        model.set_temperature(25.0);
        let delay_25 = model.calculate_delay("A", "B", 0.0, 5.0, 5.0);
        
        // High temperature
        model.set_temperature(85.0);
        let delay_85 = model.calculate_delay("A", "B", 0.0, 5.0, 5.0);
        
        // Should be 12% slower at 85C (60 degrees * 0.2%)
        assert!((delay_85.actual_delay / delay_25.actual_delay - 1.12).abs() < 0.001);
    }
    
    #[test]
    fn test_voltage_effects() {
        let mut model = DelayModel::new();
        model.add_fixed_delay("A".to_string(), "B".to_string(), 1e-9);
        
        // Lower voltage = slower
        let delay_low = model.calculate_delay("A", "B", 0.0, 4.5, 5.0);
        assert!((delay_low.factors.voltage_factor - 5.0/4.5).abs() < 0.001);
    }
    
    #[test]
    fn test_rc_delay() {
        // 1kΩ, 1nF RC circuit
        let delay = DelayModel::rc_delay(1000.0, 1e-9);
        assert!((delay - 0.69e-6).abs() < 1e-9); // Should be 0.69μs
    }
}