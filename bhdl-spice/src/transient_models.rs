//! Companion models for transient analysis
//! 
//! This module implements companion models for reactive components (capacitors and inductors)
//! using the trapezoidal integration method for time-domain analysis.

use crate::ComponentModel;

/// Companion model for capacitors using trapezoidal integration
/// 
/// The capacitor equation: i = C * dv/dt
/// Using trapezoidal integration: i(n) = 2C/h * (v(n) - v(n-1)) - i(n-1)
/// This can be modeled as: i(n) = Geq * v(n) + Ieq
/// where:
///   Geq = 2C/h (equivalent conductance)
///   Ieq = -2C/h * v(n-1) - i(n-1) (equivalent current source)
#[derive(Debug, Clone)]
pub struct CapacitorCompanion {
    pub capacitance: f64,
    pub conductance: f64,  // Geq = 2C/h
    pub current_source: f64, // Ieq
    pub v_prev: f64,        // Previous voltage
    pub i_prev: f64,        // Previous current
}

impl CapacitorCompanion {
    /// Create a new capacitor companion model
    pub fn new(capacitance: f64, time_step: f64) -> Self {
        Self {
            capacitance,
            conductance: 2.0 * capacitance / time_step,
            current_source: 0.0,
            v_prev: 0.0,
            i_prev: 0.0,
        }
    }
    
    /// Update the companion model after a time step
    pub fn update(&mut self, v_new: f64, time_step: f64) {
        // Calculate new current
        let i_new = self.conductance * (v_new - self.v_prev) - self.i_prev;
        
        // Update companion model parameters for next iteration
        self.conductance = 2.0 * self.capacitance / time_step;
        self.current_source = -self.conductance * v_new - i_new;
        
        // Store current values as previous for next step
        self.v_prev = v_new;
        self.i_prev = i_new;
    }
    
    /// Get the Norton equivalent conductance
    pub fn get_conductance(&self) -> f64 {
        self.conductance
    }
    
    /// Get the Norton equivalent current source
    pub fn get_current_source(&self) -> f64 {
        self.current_source
    }
}

/// Companion model for inductors using trapezoidal integration
/// 
/// The inductor equation: v = L * di/dt
/// Using trapezoidal integration: v(n) = 2L/h * (i(n) - i(n-1)) - v(n-1)
/// This can be modeled as: v(n) = Req * i(n) + Veq
/// Or in Norton form: i(n) = v(n)/Req - Veq/Req
/// where:
///   Req = 2L/h (equivalent resistance)
///   Veq = -2L/h * i(n-1) - v(n-1) (equivalent voltage source)
#[derive(Debug, Clone)]
pub struct InductorCompanion {
    pub inductance: f64,
    pub resistance: f64,    // Req = 2L/h
    pub voltage_source: f64, // Veq
    pub i_prev: f64,        // Previous current
    pub v_prev: f64,        // Previous voltage
}

impl InductorCompanion {
    /// Create a new inductor companion model
    pub fn new(inductance: f64, time_step: f64) -> Self {
        Self {
            inductance,
            resistance: 2.0 * inductance / time_step,
            voltage_source: 0.0,
            i_prev: 0.0,
            v_prev: 0.0,
        }
    }
    
    /// Update the companion model after a time step
    pub fn update(&mut self, i_new: f64, time_step: f64) {
        // Calculate new voltage
        let v_new = self.resistance * (i_new - self.i_prev) - self.v_prev;
        
        // Update companion model parameters for next iteration
        self.resistance = 2.0 * self.inductance / time_step;
        self.voltage_source = -self.resistance * i_new - v_new;
        
        // Store current values as previous for next step
        self.i_prev = i_new;
        self.v_prev = v_new;
    }
    
    /// Get the Norton equivalent conductance (1/Req)
    pub fn get_conductance(&self) -> f64 {
        1.0 / self.resistance
    }
    
    /// Get the Norton equivalent current source (-Veq/Req)
    pub fn get_current_source(&self) -> f64 {
        -self.voltage_source / self.resistance
    }
}

/// Companion model for nonlinear devices (LEDs, diodes) with linearization
/// 
/// For exponential devices, we linearize around the operating point:
/// i = Is * (exp(v/Vt) - 1) ≈ i_op + g_d * (v - v_op)
/// where g_d = di/dv at operating point
#[derive(Debug, Clone)]
pub struct NonlinearCompanion {
    pub device_type: String,
    pub operating_voltage: f64,
    pub operating_current: f64,
    pub dynamic_conductance: f64,
    pub equivalent_current: f64,
}

impl NonlinearCompanion {
    /// Create companion model for LED
    pub fn for_led(
        model: &ComponentModel,
        v_op: f64,
        i_op: f64,
    ) -> Self {
        if let ComponentModel::LED { 
            saturation_current, 
            emission_coefficient, 
            thermal_voltage, 
            .. 
        } = model {
            let is = saturation_current.unwrap_or(1e-14);
            let n = emission_coefficient.unwrap_or(2.0);
            let vt = thermal_voltage.unwrap_or(0.026);
            
            // Calculate dynamic conductance at operating point
            let g_d = if v_op > 0.1 {
                // In forward bias, use exponential derivative
                i_op / (n * vt)
            } else {
                // Near zero or reverse bias, use small conductance
                is / (n * vt)
            };
            
            // Norton equivalent: I_eq = I_op - G_d * V_op
            let i_eq = i_op - g_d * v_op;
            
            Self {
                device_type: "LED".to_string(),
                operating_voltage: v_op,
                operating_current: i_op,
                dynamic_conductance: g_d,
                equivalent_current: i_eq,
            }
        } else {
            panic!("Expected LED model");
        }
    }
    
    /// Create companion model for diode
    pub fn for_diode(
        model: &ComponentModel,
        v_op: f64,
        i_op: f64,
    ) -> Self {
        if let ComponentModel::Diode { 
            saturation_current, 
            emission_coefficient, 
            .. 
        } = model {
            let is = saturation_current.unwrap_or(1e-12);
            let n = emission_coefficient.unwrap_or(1.0);
            let vt = 0.026; // thermal voltage
            
            // Calculate dynamic conductance at operating point
            let g_d = if v_op > 0.1 {
                i_op / (n * vt)
            } else {
                is / (n * vt)
            };
            
            // Norton equivalent
            let i_eq = i_op - g_d * v_op;
            
            Self {
                device_type: "Diode".to_string(),
                operating_voltage: v_op,
                operating_current: i_op,
                dynamic_conductance: g_d,
                equivalent_current: i_eq,
            }
        } else {
            panic!("Expected Diode model");
        }
    }
    
    /// Update the linearization point
    pub fn update(&mut self, v_new: f64, i_new: f64, model: &ComponentModel) {
        self.operating_voltage = v_new;
        self.operating_current = i_new;
        
        // Recalculate dynamic conductance
        match model {
            ComponentModel::LED { 
                emission_coefficient, 
                thermal_voltage, 
                saturation_current,
                .. 
            } => {
                let n = emission_coefficient.unwrap_or(2.0);
                let vt = thermal_voltage.unwrap_or(0.026);
                let is = saturation_current.unwrap_or(1e-14);
                
                self.dynamic_conductance = if v_new > 0.1 {
                    i_new / (n * vt)
                } else {
                    is / (n * vt)
                };
            }
            ComponentModel::Diode { 
                emission_coefficient, 
                saturation_current,
                .. 
            } => {
                let n = emission_coefficient.unwrap_or(1.0);
                let vt = 0.026;
                let is = saturation_current.unwrap_or(1e-12);
                
                self.dynamic_conductance = if v_new > 0.1 {
                    i_new / (n * vt)
                } else {
                    is / (n * vt)
                };
            }
            _ => {}
        }
        
        // Update Norton equivalent current
        self.equivalent_current = i_new - self.dynamic_conductance * v_new;
    }
}

/// Time-varying voltage source for transient analysis
#[derive(Debug, Clone)]
pub enum TransientSource {
    /// DC voltage
    DC { voltage: f64 },
    
    /// Step voltage: transitions from v1 to v2 at time t_step
    Step { 
        v1: f64, 
        v2: f64, 
        t_step: f64 
    },
    
    /// Pulse train
    Pulse {
        v_low: f64,
        v_high: f64,
        t_rise: f64,
        t_fall: f64,
        t_on: f64,
        t_period: f64,
        t_delay: f64,
    },
    
    /// Sinusoidal source
    Sine {
        v_offset: f64,
        v_amplitude: f64,
        frequency: f64,
        phase: f64,
        t_delay: f64,
    },
    
    /// Piecewise linear
    PWL {
        time_points: Vec<f64>,
        voltage_points: Vec<f64>,
    },
}

impl TransientSource {
    /// Get voltage at given time
    pub fn voltage_at_time(&self, t: f64) -> f64 {
        match self {
            TransientSource::DC { voltage } => *voltage,
            
            TransientSource::Step { v1, v2, t_step } => {
                if t < *t_step {
                    *v1
                } else {
                    *v2
                }
            }
            
            TransientSource::Pulse { 
                v_low, v_high, t_rise, t_fall, 
                t_on, t_period, t_delay 
            } => {
                if t < *t_delay {
                    return *v_low;
                }
                
                let t_rel = (t - t_delay) % t_period;
                
                if t_rel < *t_rise {
                    // Rising edge
                    v_low + (v_high - v_low) * (t_rel / t_rise)
                } else if t_rel < t_rise + t_on {
                    // On period
                    *v_high
                } else if t_rel < t_rise + t_on + t_fall {
                    // Falling edge
                    let t_fall_rel = t_rel - t_rise - t_on;
                    v_high - (v_high - v_low) * (t_fall_rel / t_fall)
                } else {
                    // Off period
                    *v_low
                }
            }
            
            TransientSource::Sine { 
                v_offset, v_amplitude, frequency, phase, t_delay 
            } => {
                if t < *t_delay {
                    *v_offset
                } else {
                    let omega = 2.0 * std::f64::consts::PI * frequency;
                    v_offset + v_amplitude * (omega * (t - t_delay) + phase).sin()
                }
            }
            
            TransientSource::PWL { time_points, voltage_points } => {
                if time_points.is_empty() {
                    return 0.0;
                }
                
                // Find the segment
                if t <= time_points[0] {
                    voltage_points[0]
                } else if t >= *time_points.last().unwrap() {
                    *voltage_points.last().unwrap()
                } else {
                    // Linear interpolation
                    for i in 1..time_points.len() {
                        if t <= time_points[i] {
                            let t1 = time_points[i-1];
                            let t2 = time_points[i];
                            let v1 = voltage_points[i-1];
                            let v2 = voltage_points[i];
                            
                            return v1 + (v2 - v1) * (t - t1) / (t2 - t1);
                        }
                    }
                    *voltage_points.last().unwrap()
                }
            }
        }
    }
}