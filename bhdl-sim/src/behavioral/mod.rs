//! Behavioral modeling for components

pub mod component_model;
pub mod analog_model;
pub mod digital_model;
pub mod mixed_signal;
pub mod model_library;

pub use component_model::{BehavioralModel, ModelType, ModelPort};
pub use analog_model::{AnalogBehavior, AnalogState};
pub use digital_model::{DigitalBehavior, DigitalState};
pub use mixed_signal::{MixedSignalInterface, SignalDomain};
pub use model_library::{ModelLibrary, ModelFactory};