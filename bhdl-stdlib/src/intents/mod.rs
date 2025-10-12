// BHDL Standard Library Intent Functions
// Provides standard intent functions for common use cases

pub mod timing;
pub mod protection;
pub mod signal_processing;
pub mod analog;
pub mod digital;
pub mod measurement;
pub mod development;

use bhdl_common::{IntentRegistry, IntentFunction};

/// Register all standard intent functions
pub fn register_stdlib_intents(registry: &mut IntentRegistry) {
    // Register timing intents
    registry.register(Box::new(timing::DelayIntent));
    registry.register(Box::new(timing::DebounceIntent));
    registry.register(Box::new(timing::PulseStretchIntent));

    // Register protection intents
    registry.register(Box::new(protection::InputProtectionIntent));
    registry.register(Box::new(protection::OvervoltageProtectionIntent));

    // Register signal processing intents
    registry.register(Box::new(signal_processing::AntiAliasIntent));
    registry.register(Box::new(signal_processing::LowNoiseIntent));
    registry.register(Box::new(signal_processing::NoiseFilteringIntent));

    // Register analog intents
    registry.register(Box::new(analog::CurrentLimitingIntent));
    registry.register(Box::new(analog::LevelShiftingIntent));
    registry.register(Box::new(analog::VoltageDivisionIntent));
    registry.register(Box::new(analog::SignalAmplificationIntent));

    // Register digital intents
    registry.register(Box::new(digital::SignalBufferingIntent));

    // Register measurement intents
    registry.register(Box::new(measurement::PrecisionMeasurementIntent));
    registry.register(Box::new(measurement::ControlLoopIntent));

    // Register development intents
    registry.register(Box::new(development::DebugOnlyIntent));
}