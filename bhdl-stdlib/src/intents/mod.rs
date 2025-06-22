// BHDL Standard Library Intent Functions
// Provides standard intent functions for common use cases

pub mod timing;
pub mod protection;
pub mod signal_processing;

use bhdl_common::{IntentRegistry, IntentFunction};

/// Register all standard intent functions
pub fn register_stdlib_intents(registry: &mut IntentRegistry) {
    // Register timing intents
    registry.register(Box::new(timing::DelayIntent));
    registry.register(Box::new(timing::DebounceIntent));
    
    // Register protection intents
    registry.register(Box::new(protection::InputProtectionIntent));
    registry.register(Box::new(protection::OvervoltageProtectionIntent));
    
    // Register signal processing intents
    registry.register(Box::new(signal_processing::AntiAliasIntent));
    registry.register(Box::new(signal_processing::LowNoiseIntent));
}