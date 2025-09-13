// Waveform module - minimal stub for compilation

use crate::Result;

#[derive(Debug, Clone, Copy)]
pub enum WaveformFormat {
    VCD,
    FST,
    CSV,
}

#[derive(Debug, Clone)]
pub struct WaveformWriter {
    format: WaveformFormat,
}