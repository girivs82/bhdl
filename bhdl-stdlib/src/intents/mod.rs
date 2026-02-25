// BHDL Standard Library Intent Functions
// Provides standard intent functions for common use cases

pub mod timing;
pub mod protection;
pub mod signal_processing;
pub mod analog;
pub mod digital;
pub mod measurement;
pub mod development;
pub mod safety;
pub mod power_management;
pub mod digital_timing;
pub mod advanced;
pub mod specialized;

use bhdl_common::{IntentRegistry, IntentFunction, OutputFilteringIntent,
    InputFilteringIntent, RegulationIntent, LoadingIntent};

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

    // Register safety intents
    registry.register(Box::new(safety::AutomotiveSafetyIntent));
    registry.register(Box::new(safety::IndustrialControlIntent));
    registry.register(Box::new(safety::MedicalSafetyIntent));
    registry.register(Box::new(safety::EsdProtectionIntent));

    // Register power management intents
    registry.register(Box::new(power_management::PowerSequencingIntent));
    registry.register(Box::new(power_management::VoltageMonitoringIntent));
    registry.register(Box::new(power_management::PowerGoodSignalIntent));
    registry.register(Box::new(power_management::InrushLimitingIntent));

    // Register digital timing intents
    registry.register(Box::new(digital_timing::ClockDistributionIntent));
    registry.register(Box::new(digital_timing::ResetGenerationIntent));
    registry.register(Box::new(digital_timing::BootSequencingIntent));

    // Register advanced feature intents
    registry.register(Box::new(advanced::SignalIntegrityIntent));
    registry.register(Box::new(advanced::EmiFilteringIntent));
    registry.register(Box::new(advanced::IsolationIntent));
    registry.register(Box::new(advanced::ThermalManagementIntent));

    // Register output filtering intent (ripple-aware cap bank generation)
    registry.register(Box::new(OutputFilteringIntent));

    // Register power stage intents (staged power flow)
    registry.register(Box::new(InputFilteringIntent));
    registry.register(Box::new(RegulationIntent));
    registry.register(Box::new(LoadingIntent));

    // Register specialized intents
    registry.register(Box::new(specialized::VoltageRegulationIntent));
    registry.register(Box::new(specialized::CurrentSensingIntent));
    registry.register(Box::new(specialized::CommunicationInterfaceIntent));
    registry.register(Box::new(specialized::WatchdogMonitoringIntent));
    registry.register(Box::new(specialized::PowerOptimizationIntent));
    registry.register(Box::new(specialized::TestPointIntent));
    registry.register(Box::new(specialized::RedundancyIntent));
}