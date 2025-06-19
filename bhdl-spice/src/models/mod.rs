//! SPICE model definitions and traits
//! 
//! This module provides sophisticated SPICE models for accurate circuit simulation

use std::collections::HashMap;
use serde::{Serialize, Deserialize};

pub mod resistor;
pub mod capacitor;
pub mod inductor;
pub mod diode;
pub mod bjt;
pub mod mosfet;
pub mod opamp;
pub mod voltage_regulator;

pub use resistor::{ResistorModel, ResistorParams};
pub use capacitor::{CapacitorModel, CapacitorParams};
pub use inductor::{InductorModel, InductorParams};
pub use diode::{DiodeModel, DiodeParams};
pub use bjt::{BjtModel, BjtParams, BjtType};
pub use mosfet::{MosfetModel, MosfetParams, MosfetType};
pub use opamp::{OpAmpModel, OpAmpParams};
pub use voltage_regulator::{VoltageRegulatorModel, VoltageRegulatorParams, RegulatorType};

/// Temperature in Celsius (default: 27°C)
pub const DEFAULT_TEMPERATURE: f64 = 27.0;

/// Boltzmann constant (J/K)
pub const BOLTZMANN: f64 = 1.380649e-23;

/// Elementary charge (C)
pub const ELEMENTARY_CHARGE: f64 = 1.602176634e-19;

/// Thermal voltage at room temperature (V)
pub const VT_ROOM: f64 = 0.025875; // kT/q at 27°C

/// Calculate thermal voltage at given temperature
pub fn thermal_voltage(temp_celsius: f64) -> f64 {
    let temp_kelvin = temp_celsius + 273.15;
    (BOLTZMANN * temp_kelvin) / ELEMENTARY_CHARGE
}

/// Base trait for all SPICE models
pub trait SpiceModel: Send + Sync {
    /// Get model name
    fn name(&self) -> &str;
    
    /// Get model type identifier
    fn model_type(&self) -> ModelType;
    
    /// Calculate current through device given terminal voltages
    fn current(&self, voltages: &[f64], temp: f64) -> f64;
    
    /// Calculate conductance (di/dv) for Newton-Raphson
    fn conductance(&self, voltages: &[f64], temp: f64) -> Vec<f64>;
    
    /// Get number of terminals
    fn num_terminals(&self) -> usize;
    
    /// Check if model is nonlinear
    fn is_nonlinear(&self) -> bool;
    
    /// Get model parameters as key-value pairs
    fn parameters(&self) -> HashMap<String, f64>;
    
    /// Update model parameter by name
    fn set_parameter(&mut self, name: &str, value: f64) -> Result<(), String>;
}

/// Model type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelType {
    Resistor,
    Capacitor,
    Inductor,
    Diode,
    BJT,
    MOSFET,
    OpAmp,
    VoltageRegulator,
    VoltageSource,
    CurrentSource,
}

/// Model level for complexity/accuracy tradeoff
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelLevel {
    /// Simple linear or piecewise-linear models
    Basic,
    /// Standard SPICE models (Level 1)
    Standard,
    /// Advanced models with more effects (Level 2+)
    Advanced,
    /// Full physics-based models
    Complete,
}

/// Common model parameters shared across devices
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommonParams {
    /// Nominal temperature for parameters (°C)
    pub tnom: f64,
    /// Temperature coefficient 1 (%/°C)
    pub tc1: f64,
    /// Temperature coefficient 2 (%/°C²)
    pub tc2: f64,
    /// Area factor for scaled devices
    pub area: f64,
    /// Number of parallel devices
    pub m: f64,
}

impl Default for CommonParams {
    fn default() -> Self {
        Self {
            tnom: DEFAULT_TEMPERATURE,
            tc1: 0.0,
            tc2: 0.0,
            area: 1.0,
            m: 1.0,
        }
    }
}

/// Model library for storing and retrieving models
pub struct ModelLibrary {
    models: HashMap<String, Box<dyn SpiceModel>>,
    type_defaults: HashMap<ModelType, Box<dyn SpiceModel>>,
}

impl ModelLibrary {
    /// Create new model library with defaults
    pub fn new() -> Self {
        let mut lib = Self {
            models: HashMap::new(),
            type_defaults: HashMap::new(),
        };
        lib.load_defaults();
        lib
    }
    
    /// Load default models for each type
    fn load_defaults(&mut self) {
        // Add default models for each type
        self.type_defaults.insert(
            ModelType::Resistor,
            Box::new(ResistorModel::default())
        );
        self.type_defaults.insert(
            ModelType::Capacitor,
            Box::new(CapacitorModel::default())
        );
        self.type_defaults.insert(
            ModelType::Inductor,
            Box::new(InductorModel::default())
        );
        self.type_defaults.insert(
            ModelType::Diode,
            Box::new(DiodeModel::default())
        );
        self.type_defaults.insert(
            ModelType::BJT,
            Box::new(BjtModel::default())
        );
        self.type_defaults.insert(
            ModelType::MOSFET,
            Box::new(MosfetModel::default())
        );
        self.type_defaults.insert(
            ModelType::OpAmp,
            Box::new(OpAmpModel::default())
        );
    }
    
    /// Add a model to the library
    pub fn add_model(&mut self, name: String, model: Box<dyn SpiceModel>) {
        self.models.insert(name, model);
    }
    
    /// Get model by name with fallback to type default
    pub fn get_model(&self, name: &str, model_type: ModelType) -> Option<&dyn SpiceModel> {
        self.models.get(name)
            .map(|m| m.as_ref())
            .or_else(|| self.type_defaults.get(&model_type).map(|m| m.as_ref()))
    }
    
    /// Get mutable model by name
    pub fn get_model_mut(&mut self, name: &str) -> Option<&mut (dyn SpiceModel + 'static)> {
        self.models.get_mut(name).map(|m| m.as_mut())
    }
}

/// Helper function to clamp values for numerical stability
pub fn clamp_exp(x: f64, max: f64) -> f64 {
    x.min(max).max(-max)
}

/// Helper for safe division preventing divide by zero
pub fn safe_divide(num: f64, den: f64, default: f64) -> f64 {
    if den.abs() < 1e-30 {
        default
    } else {
        num / den
    }
}