// Data capture and output modules

pub mod waveform;
pub mod vcd;
pub mod probe;
pub mod formats;
pub mod simple;

// Export simple version
pub mod manager {
    pub use super::simple::Manager;
}

pub use waveform::{WaveformCapture, SignalTrace, TimePoint};
pub use vcd::{VcdWriter, VcdConfig};
pub use probe::{ProbeManager, Probe, ProbeType};
pub use formats::{OutputFormat, CsvWriter, JsonWriter};