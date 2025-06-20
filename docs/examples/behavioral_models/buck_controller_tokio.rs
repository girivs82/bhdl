// Example behavioral model for a buck converter controller
// This would live in a separate crate or module that implements the BehavioralModel trait

use async_trait::async_trait;
use std::collections::HashMap;

// From bhdl-pli crate
use bhdl_pli::{BehavioralModel, ModelError, PinDefinition};

/// Buck converter controller with soft-start and current limiting
pub struct BuckController {
    // Configuration (from attributes)
    vout_target: f64,
    soft_start_time: f64,
    switching_freq: f64,
    current_limit: f64,
    
    // Controller parameters
    kp: f64,
    ki: f64,
    
    // Internal state
    vref_ramped: f64,
    integrator: f64,
    soft_start_timer: f64,
    enabled: bool,
    in_current_limit: bool,
    
    // Pin mappings (set during initialization)
    pin_indices: HashMap<String, usize>,
}

impl BuckController {
    pub fn new() -> Self {
        Self {
            // Default configuration
            vout_target: 3.3,
            soft_start_time: 0.010,  // 10ms
            switching_freq: 500_000.0,  // 500kHz
            current_limit: 2.0,  // 2A
            
            // PI controller gains
            kp: 0.1,
            ki: 10.0,
            
            // Initial state
            vref_ramped: 0.0,
            integrator: 0.0,
            soft_start_timer: 0.0,
            enabled: false,
            in_current_limit: false,
            
            pin_indices: HashMap::new(),
        }
    }
    
    pub fn with_config(mut self, config: HashMap<String, f64>) -> Self {
        // Override defaults with provided config
        if let Some(&v) = config.get("vout_target") {
            self.vout_target = v;
        }
        if let Some(&v) = config.get("soft_start_time") {
            self.soft_start_time = v;
        }
        if let Some(&v) = config.get("switching_freq") {
            self.switching_freq = v;
        }
        if let Some(&v) = config.get("current_limit") {
            self.current_limit = v;
        }
        if let Some(&v) = config.get("kp") {
            self.kp = v;
        }
        if let Some(&v) = config.get("ki") {
            self.ki = v;
        }
        self
    }
}

#[async_trait]
impl BehavioralModel for BuckController {
    async fn initialize(&mut self, pins: Vec<PinDefinition>) -> Result<(), ModelError> {
        // Verify we have all required pins
        let required_inputs = ["VIN", "ENABLE", "FB", "I_SENSE"];
        let required_outputs = ["PWM", "PGOOD"];
        
        for (idx, pin) in pins.iter().enumerate() {
            self.pin_indices.insert(pin.name.clone(), idx);
        }
        
        // Check all required pins exist
        for pin_name in required_inputs {
            if !self.pin_indices.contains_key(pin_name) {
                return Err(ModelError::MissingPin(pin_name.to_string()));
            }
        }
        
        for pin_name in required_outputs {
            if !self.pin_indices.contains_key(pin_name) {
                return Err(ModelError::MissingPin(pin_name.to_string()));
            }
        }
        
        Ok(())
    }
    
    async fn step(&mut self, dt: f64, inputs: HashMap<String, f64>) 
        -> Result<HashMap<String, f64>, ModelError> 
    {
        // Read inputs
        let vin = inputs.get("VIN").copied().unwrap_or(0.0);
        let enable = inputs.get("ENABLE").copied().unwrap_or(0.0) > 0.5;
        let vfb = inputs.get("FB").copied().unwrap_or(0.0);
        let i_sense = inputs.get("I_SENSE").copied().unwrap_or(0.0);
        
        // State machine
        if enable && !self.enabled {
            // Rising edge of enable - start soft-start
            self.enabled = true;
            self.soft_start_timer = 0.0;
            self.vref_ramped = 0.0;
            self.integrator = 0.0;
        } else if !enable && self.enabled {
            // Falling edge of enable - shutdown
            self.enabled = false;
            self.vref_ramped = 0.0;
            self.integrator = 0.0;
        }
        
        let mut pwm_duty = 0.0;
        let mut pgood = false;
        
        if self.enabled {
            // Soft-start ramp
            if self.soft_start_timer < self.soft_start_time {
                self.soft_start_timer += dt;
                self.vref_ramped = self.vout_target * 
                    (self.soft_start_timer / self.soft_start_time).min(1.0);
            } else {
                self.vref_ramped = self.vout_target;
            }
            
            // Current limit check
            self.in_current_limit = i_sense > self.current_limit;
            
            if !self.in_current_limit {
                // Normal PI control
                let error = self.vref_ramped - vfb;
                
                // Proportional term
                let p_term = self.kp * error;
                
                // Integral term with anti-windup
                self.integrator += self.ki * error * dt;
                self.integrator = self.integrator.clamp(-0.5, 0.5);
                
                // Calculate duty cycle
                pwm_duty = (p_term + self.integrator + vfb / vin).clamp(0.0, 0.95);
                
                // Power good detection
                pgood = self.soft_start_timer >= self.soft_start_time && 
                       error.abs() < 0.1 && 
                       !self.in_current_limit;
            } else {
                // Current limit mode - reduce duty cycle
                pwm_duty *= 0.9;  // Simple current limit response
                pgood = false;
            }
        }
        
        // Return outputs
        let mut outputs = HashMap::new();
        outputs.insert("PWM".to_string(), pwm_duty);
        outputs.insert("PGOOD".to_string(), if pgood { 1.0 } else { 0.0 });
        
        Ok(outputs)
    }
    
    async fn step_batch(
        &mut self, 
        dt: f64, 
        count: usize,
        inputs: HashMap<String, Vec<f64>>
    ) -> Result<HashMap<String, Vec<f64>>, ModelError> {
        // Optimized batch processing
        let mut pwm_out = Vec::with_capacity(count);
        let mut pgood_out = Vec::with_capacity(count);
        
        // Pre-fetch input vectors
        let vin_vec = inputs.get("VIN").ok_or(ModelError::MissingInput("VIN".into()))?;
        let enable_vec = inputs.get("ENABLE").ok_or(ModelError::MissingInput("ENABLE".into()))?;
        let fb_vec = inputs.get("FB").ok_or(ModelError::MissingInput("FB".into()))?;
        let i_sense_vec = inputs.get("I_SENSE").ok_or(ModelError::MissingInput("I_SENSE".into()))?;
        
        for i in 0..count {
            // Create input map for single step
            let step_inputs = HashMap::from([
                ("VIN".to_string(), vin_vec[i]),
                ("ENABLE".to_string(), enable_vec[i]),
                ("FB".to_string(), fb_vec[i]),
                ("I_SENSE".to_string(), i_sense_vec[i]),
            ]);
            
            // Run single step
            let outputs = self.step(dt, step_inputs).await?;
            
            // Collect outputs
            pwm_out.push(outputs["PWM"]);
            pgood_out.push(outputs["PGOOD"]);
        }
        
        Ok(HashMap::from([
            ("PWM".to_string(), pwm_out),
            ("PGOOD".to_string(), pgood_out),
        ]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_buck_soft_start() {
        let mut controller = BuckController::new();
        
        // Initialize pins
        let pins = vec![
            PinDefinition { name: "VIN".into(), direction: PinDirection::Input },
            PinDefinition { name: "ENABLE".into(), direction: PinDirection::Input },
            PinDefinition { name: "FB".into(), direction: PinDirection::Input },
            PinDefinition { name: "I_SENSE".into(), direction: PinDirection::Input },
            PinDefinition { name: "PWM".into(), direction: PinDirection::Output },
            PinDefinition { name: "PGOOD".into(), direction: PinDirection::Output },
        ];
        
        controller.initialize(pins).await.unwrap();
        
        // Test soft-start behavior
        let dt = 0.0001;  // 100us timestep
        let mut time = 0.0;
        
        while time < 0.020 {  // 20ms simulation
            let inputs = HashMap::from([
                ("VIN".to_string(), 12.0),
                ("ENABLE".to_string(), 1.0),
                ("FB".to_string(), controller.vref_ramped * 0.9),  // 90% of target
                ("I_SENSE".to_string(), 0.5),  // 500mA
            ]);
            
            let outputs = controller.step(dt, inputs).await.unwrap();
            
            // During soft-start, PWM should gradually increase
            if time < 0.010 {
                assert!(outputs["PWM"] < 0.8);
                assert_eq!(outputs["PGOOD"], 0.0);
            } else {
                // After soft-start, should regulate
                assert!(outputs["PWM"] > 0.2);
                assert_eq!(outputs["PGOOD"], 1.0);
            }
            
            time += dt;
        }
    }
}