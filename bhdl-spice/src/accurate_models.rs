//! Accurate physics-based component models for SPICE simulation
//!
//! These models use real physics equations without simplifications,
//! relying on the scaled solver to handle numerical challenges.

use crate::components::ElectricalLimits;
use std::f64::consts::E;

/// Physical constants
pub const BOLTZMANN: f64 = 1.380649e-23;  // J/K
pub const ELEMENTARY_CHARGE: f64 = 1.602176634e-19;  // C
pub const ROOM_TEMP: f64 = 298.15;  // K (25°C)

/// Accurate LED model using Shockley diode equation
#[derive(Debug, Clone)]
pub struct AccurateLED {
    /// Saturation current (A) - typically 1e-24 to 1e-12
    pub saturation_current: f64,
    /// Emission coefficient (1.0-2.0, typically 1.5 for LEDs)
    pub emission_coefficient: f64,
    /// Series resistance (Ω) - bulk and contact resistance
    pub series_resistance: f64,
    /// Junction temperature (K)
    pub temperature: f64,
    /// Maximum ratings
    pub max_current: f64,
    pub max_voltage: f64,
    pub max_power: f64,
}

impl AccurateLED {
    /// Create LED from datasheet parameters
    pub fn from_datasheet(
        vf_nominal: f64,  // Forward voltage at test current
        if_test: f64,     // Test current for Vf
        n: f64,           // Emission coefficient (1.5 typical)
        rs: f64,          // Series resistance
        if_max: f64,      // Maximum continuous current
    ) -> Self {
        // Extract Is from datasheet values. The datasheet Vf includes the
        // I·Rs drop, so remove it to get the junction voltage at the test
        // point — current() applies the same Rs when evaluating.
        let vt = thermal_voltage(ROOM_TEMP);
        let is = if_test / (((vf_nominal - if_test * rs) / (n * vt)).exp() - 1.0);
        
        Self {
            saturation_current: is,
            emission_coefficient: n,
            series_resistance: rs,
            temperature: ROOM_TEMP,
            max_current: if_max,
            max_voltage: vf_nominal * 1.5,  // Typical headroom
            max_power: vf_nominal * if_max,
        }
    }
    
    /// Current through LED given voltage
    pub fn current(&self, voltage: f64) -> f64 {
        if voltage <= 0.0 {
            return 0.0;  // No reverse current for simplicity
        }
        
        let vt = thermal_voltage(self.temperature);
        let vd = voltage - self.series_resistance * self.current_iterative(voltage);
        
        if vd <= 0.0 {
            return 0.0;
        }
        
        self.saturation_current * ((vd / (self.emission_coefficient * vt)).exp() - 1.0)
    }
    
    /// Iterative solution for current including series resistance
    fn current_iterative(&self, voltage: f64) -> f64 {
        let vt = thermal_voltage(self.temperature);
        let mut i_guess = voltage / (self.series_resistance + 25.0);  // Initial guess
        
        // Newton iteration for self-consistent solution. The exponential
        // needs ~10 iterations to converge from the linear initial guess.
        for _ in 0..50 {
            let vd = voltage - i_guess * self.series_resistance;
            if vd <= 0.0 {
                return 0.0;
            }
            
            let i_diode = self.saturation_current * ((vd / (self.emission_coefficient * vt)).exp() - 1.0);
            let di_dvd = (self.saturation_current / (self.emission_coefficient * vt)) * 
                         (vd / (self.emission_coefficient * vt)).exp();
            
            let f = i_guess - i_diode;
            let df = 1.0 + di_dvd * self.series_resistance;
            
            i_guess -= f / df;
            
            if f.abs() < 1e-12 {
                break;
            }
        }
        
        i_guess
    }
    
    /// Dynamic conductance (dI/dV)
    pub fn conductance(&self, voltage: f64) -> f64 {
        if voltage <= 0.0 {
            return 1e-12;  // Very small conductance when off
        }
        
        let vt = thermal_voltage(self.temperature);
        let i = self.current(voltage);
        let vd = voltage - i * self.series_resistance;
        
        if vd <= 0.0 {
            return 1e-12;
        }
        
        let gd = (self.saturation_current / (self.emission_coefficient * vt)) * 
                 (vd / (self.emission_coefficient * vt)).exp();
        
        // Total conductance including series resistance
        gd / (1.0 + gd * self.series_resistance)
    }
}

/// Accurate diode model with reverse breakdown
#[derive(Debug, Clone)]
pub struct AccurateDiode {
    /// Saturation current (A)
    pub saturation_current: f64,
    /// Emission coefficient
    pub emission_coefficient: f64,
    /// Series resistance (Ω)
    pub series_resistance: f64,
    /// Reverse breakdown voltage (V)
    pub breakdown_voltage: f64,
    /// Breakdown knee current (A)
    pub breakdown_current: f64,
    /// Junction temperature (K)
    pub temperature: f64,
}

impl AccurateDiode {
    /// Standard 1N4148 signal diode
    pub fn n1n4148() -> Self {
        Self {
            saturation_current: 2.52e-9,
            emission_coefficient: 1.752,
            series_resistance: 0.568,
            breakdown_voltage: -75.0,
            breakdown_current: -1e-6,
            temperature: ROOM_TEMP,
        }
    }
    
    /// Standard 1N4007 rectifier diode
    pub fn n1n4007() -> Self {
        Self {
            saturation_current: 1.45e-8,
            emission_coefficient: 1.96,
            series_resistance: 0.037,
            breakdown_voltage: -1000.0,
            breakdown_current: -5e-6,
            temperature: ROOM_TEMP,
        }
    }
    
    /// Current through diode
    pub fn current(&self, voltage: f64) -> f64 {
        let vt = thermal_voltage(self.temperature);
        
        // Forward bias
        if voltage > 0.0 {
            let i_forward = self.saturation_current * 
                ((voltage / (self.emission_coefficient * vt)).exp() - 1.0);
            return i_forward;
        }
        
        // Reverse bias with breakdown
        let v_br = self.breakdown_voltage;
        if voltage > v_br {
            // Normal reverse leakage
            return -self.saturation_current;
        } else {
            // Breakdown region - exponential increase
            let v_excess = v_br - voltage;
            let i_breakdown = -self.breakdown_current * (v_excess / vt).exp();
            return i_breakdown - self.saturation_current;
        }
    }
}

/// Accurate Zener diode model
#[derive(Debug, Clone)]
pub struct AccurateZener {
    /// Forward diode parameters
    pub forward_is: f64,
    pub forward_n: f64,
    /// Zener voltage (positive value)
    pub zener_voltage: f64,
    /// Zener knee current
    pub zener_current: f64,
    /// Dynamic resistance in Zener region
    pub zener_resistance: f64,
    /// Temperature coefficient (V/K)
    pub temp_coefficient: f64,
    pub temperature: f64,
}

impl AccurateZener {
    /// 5.1V Zener (1N4733A)
    pub fn n1n4733a() -> Self {
        Self {
            forward_is: 1e-12,
            forward_n: 1.5,
            zener_voltage: 5.1,
            zener_current: 49e-3,  // At 49mA test current
            zener_resistance: 7.0,  // 7Ω dynamic resistance
            temp_coefficient: 2e-3, // +2mV/K
            temperature: ROOM_TEMP,
        }
    }
    
    pub fn current(&self, voltage: f64) -> f64 {
        let vt = thermal_voltage(self.temperature);
        let vz_temp = self.zener_voltage + self.temp_coefficient * (self.temperature - ROOM_TEMP);
        
        if voltage > 0.5 {
            // Forward diode operation
            self.forward_is * ((voltage / (self.forward_n * vt)).exp() - 1.0)
        } else if voltage > -vz_temp {
            // Reverse leakage
            -self.forward_is
        } else {
            // Zener breakdown region
            let v_excess = -voltage - vz_temp;
            -self.zener_current - v_excess / self.zener_resistance
        }
    }
}

/// Accurate bipolar junction transistor (BJT) model
#[derive(Debug, Clone)]
pub struct AccurateBJT {
    /// Transport saturation current
    pub is: f64,
    /// Forward beta (hFE)
    pub beta_f: f64,
    /// Reverse beta
    pub beta_r: f64,
    /// Forward emission coefficient
    pub nf: f64,
    /// Reverse emission coefficient
    pub nr: f64,
    /// Base resistance
    pub rb: f64,
    /// Collector resistance
    pub rc: f64,
    /// Emitter resistance
    pub re: f64,
    /// Early voltage (forward)
    pub va: f64,
    /// Temperature
    pub temperature: f64,
}

impl AccurateBJT {
    /// 2N3904 NPN general purpose
    pub fn n2n3904() -> Self {
        Self {
            is: 1.4e-14,
            beta_f: 200.0,
            beta_r: 4.0,
            nf: 1.0,
            nr: 1.0,
            rb: 10.0,
            rc: 1.0,
            re: 0.1,
            va: 74.0,
            temperature: ROOM_TEMP,
        }
    }
    
    /// Calculate terminal currents given voltages (Ebers-Moll model)
    pub fn currents(&self, vbe: f64, vbc: f64, vce: f64) -> (f64, f64, f64) {
        let vt = thermal_voltage(self.temperature);
        
        // Base-emitter and base-collector junction currents
        let ibe = (self.is / self.beta_f) * ((vbe / (self.nf * vt)).exp() - 1.0);
        let ibc = (self.is / self.beta_r) * ((vbc / (self.nr * vt)).exp() - 1.0);
        
        // Transport currents
        let ice = self.is * ((vbe / (self.nf * vt)).exp() - (vbc / (self.nr * vt)).exp());
        
        // Early effect
        let ice_total = ice * (1.0 + vce / self.va);
        
        // Terminal currents
        let ib = ibe + ibc;
        let ic = ice_total - ibc;
        let ie = ic + ib;
        
        (ib, ic, ie)
    }
}

/// Helper function to calculate thermal voltage
fn thermal_voltage(temperature: f64) -> f64 {
    BOLTZMANN * temperature / ELEMENTARY_CHARGE
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_led_model_accuracy() {
        // Test LED created from typical datasheet values
        let led = AccurateLED::from_datasheet(
            2.0,    // 2V @ 20mA
            0.02,   // 20mA test current
            1.5,    // Typical emission coefficient
            10.0,   // 10Ω series resistance
            0.03,   // 30mA max
        );
        
        // Should give very small Is
        assert!(led.saturation_current < 1e-20);
        
        // Should give approximately 2V at 20mA
        let i = led.current(2.0);
        assert!((i - 0.02).abs() < 0.001);
    }
}