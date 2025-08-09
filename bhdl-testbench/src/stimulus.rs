//! Stimulus generation for testbenches

use std::collections::HashMap;
use crate::{SignalRef, TimeSpec};
use crate::testbench::{Stimulus, Waveform};

/// Stimulus generator that produces signal values over time
pub struct StimulusGenerator {
    stimuli: Vec<Stimulus>,
}

impl StimulusGenerator {
    pub fn new(stimuli: &[Stimulus]) -> Self {
        Self {
            stimuli: stimuli.to_vec(),
        }
    }
    
    /// Get stimulus values at a given time
    pub fn get_values(&self, time: f64) -> HashMap<SignalRef, f64> {
        let mut values = HashMap::new();
        
        for stimulus in &self.stimuli {
            let value = self.evaluate_waveform(&stimulus.waveform, time);
            values.insert(stimulus.target.clone(), value);
        }
        
        values
    }
    
    fn evaluate_waveform(&self, waveform: &Waveform, time: f64) -> f64 {
        match waveform {
            Waveform::Constant(value) => *value,
            
            Waveform::Ramp { start_value, end_value, duration } => {
                let duration_sec = duration.as_seconds();
                if time <= 0.0 {
                    *start_value
                } else if time >= duration_sec {
                    *end_value
                } else {
                    let progress = time / duration_sec;
                    start_value + (end_value - start_value) * progress
                }
            }
            
            Waveform::Steps(steps) => {
                // Find the active step
                let mut current_value = 0.0;
                for (step_time, value) in steps {
                    if time >= step_time.as_seconds() {
                        current_value = *value;
                    } else {
                        break;
                    }
                }
                current_value
            }
            
            Waveform::Sine { amplitude, frequency, offset, phase } => {
                offset + amplitude * (2.0 * std::f64::consts::PI * frequency * time + phase).sin()
            }
            
            Waveform::Pulse { low, high, delay, width, period } => {
                let delay_sec = delay.as_seconds();
                let width_sec = width.as_seconds();
                let period_sec = period.as_seconds();
                
                if time < delay_sec {
                    *low
                } else {
                    let t_rel = (time - delay_sec) % period_sec;
                    if t_rel < width_sec {
                        *high
                    } else {
                        *low
                    }
                }
            }
        }
    }
}