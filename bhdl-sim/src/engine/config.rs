//! Simulation configuration

use serde::{Deserialize, Serialize};

/// Main simulation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationConfig {
    /// Time step in seconds
    pub time_step: f64,
    
    /// Maximum simulation time in seconds
    pub max_time: f64,
    
    /// Convergence threshold for iterative solvers
    pub convergence_threshold: f64,
    
    /// Maximum iterations for convergence
    pub max_iterations: usize,
    
    /// Output configuration
    pub output_config: OutputConfig,
    
    /// Performance tuning
    pub performance: PerformanceConfig,
    
    /// Enable debug mode
    pub debug: bool,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            time_step: 1e-6, // 1 microsecond
            max_time: 1.0,    // 1 second
            convergence_threshold: 1e-6,
            max_iterations: 100,
            output_config: OutputConfig::default(),
            performance: PerformanceConfig::default(),
            debug: false,
        }
    }
}

/// Output configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    /// Enable waveform capture
    pub capture_waveforms: bool,
    
    /// Signals to capture (empty = all)
    pub capture_signals: Vec<String>,
    
    /// Output format
    pub format: OutputFormat,
    
    /// Sample rate for waveform capture (samples per second)
    pub sample_rate: f64,
    
    /// Compression level (0-9)
    pub compression: u8,
    
    /// Output directory
    pub output_dir: String,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            capture_waveforms: true,
            capture_signals: Vec::new(),
            format: OutputFormat::Vcd,
            sample_rate: 1e6, // 1 MHz
            compression: 0,
            output_dir: "simulation_output".to_string(),
        }
    }
}

/// Output format options
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum OutputFormat {
    /// Value Change Dump format
    Vcd,
    
    /// Fast Signal Trace format
    Fst,
    
    /// CSV format
    Csv,
    
    /// JSON format
    Json,
    
    /// Binary format
    Binary,
}

/// Performance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Enable parallel evaluation
    pub parallel_evaluation: bool,
    
    /// Cache expression evaluation results
    pub cache_expressions: bool,
    
    /// Batch size for parallel processing
    pub batch_size: usize,
    
    /// Number of worker threads (0 = auto)
    pub num_threads: usize,
    
    /// Memory limit in bytes (0 = unlimited)
    pub memory_limit: usize,
    
    /// Enable incremental evaluation
    pub incremental: bool,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            parallel_evaluation: true,
            cache_expressions: true,
            batch_size: 1000,
            num_threads: 0, // Auto-detect
            memory_limit: 0, // Unlimited
            incremental: true,
        }
    }
}

impl SimulationConfig {
    /// Create a fast configuration for testing
    pub fn fast() -> Self {
        Self {
            time_step: 1e-3, // 1ms - coarser for speed
            max_time: 0.1,   // 100ms
            convergence_threshold: 1e-3,
            max_iterations: 50,
            output_config: OutputConfig {
                capture_waveforms: false,
                ..Default::default()
            },
            performance: PerformanceConfig {
                cache_expressions: false, // Less memory usage
                batch_size: 100,
                ..Default::default()
            },
            debug: false,
        }
    }
    
    /// Create a precise configuration for accuracy
    pub fn precise() -> Self {
        Self {
            time_step: 1e-9, // 1ns - very fine
            max_time: 1.0,
            convergence_threshold: 1e-12,
            max_iterations: 1000,
            output_config: OutputConfig {
                sample_rate: 1e9, // 1 GHz sampling
                ..Default::default()
            },
            performance: PerformanceConfig {
                incremental: false, // Full evaluation each step
                ..Default::default()
            },
            debug: false,
        }
    }
    
    /// Create a debug configuration
    pub fn debug() -> Self {
        Self {
            debug: true,
            output_config: OutputConfig {
                compression: 0, // No compression for readability
                ..Default::default()
            },
            performance: PerformanceConfig {
                parallel_evaluation: false, // Easier to debug
                cache_expressions: false,
                ..Default::default()
            },
            ..Default::default()
        }
    }
    
    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.time_step <= 0.0 {
            return Err("Time step must be positive".to_string());
        }
        
        if self.max_time <= 0.0 {
            return Err("Max time must be positive".to_string());
        }
        
        if self.convergence_threshold <= 0.0 {
            return Err("Convergence threshold must be positive".to_string());
        }
        
        if self.max_iterations == 0 {
            return Err("Max iterations must be at least 1".to_string());
        }
        
        if self.output_config.sample_rate <= 0.0 {
            return Err("Sample rate must be positive".to_string());
        }
        
        if self.output_config.compression > 9 {
            return Err("Compression level must be 0-9".to_string());
        }
        
        if self.performance.batch_size == 0 {
            return Err("Batch size must be at least 1".to_string());
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_config() {
        let config = SimulationConfig::default();
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_fast_config() {
        let config = SimulationConfig::fast();
        assert!(config.validate().is_ok());
        assert!(!config.output_config.capture_waveforms);
    }
    
    #[test]
    fn test_precise_config() {
        let config = SimulationConfig::precise();
        assert!(config.validate().is_ok());
        assert_eq!(config.time_step, 1e-9);
    }
    
    #[test]
    fn test_validation() {
        let mut config = SimulationConfig::default();
        
        config.time_step = -1.0;
        assert!(config.validate().is_err());
        
        config.time_step = 1e-6;
        config.max_iterations = 0;
        assert!(config.validate().is_err());
    }
}