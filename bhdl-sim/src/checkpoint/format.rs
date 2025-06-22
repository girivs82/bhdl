//! Checkpoint file format definitions

use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use bhdl_netlist::{InstanceId, NetId};
use crate::circuit::{PinValue, ComponentState};
use crate::time::TimeStep;

/// Checkpoint format version
pub const CHECKPOINT_VERSION: u32 = 1;

/// Magic bytes for checkpoint files
pub const CHECKPOINT_MAGIC: &[u8] = b"BHDLCHKP";

/// Checkpoint file format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointFormat {
    /// Binary format (fast, compact)
    Binary,
    /// JSON format (human readable)
    Json,
    /// Compressed binary format
    CompressedBinary,
}

impl CheckpointFormat {
    /// Get file extension
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Binary => "bcp",
            Self::Json => "json",
            Self::CompressedBinary => "bcpz",
        }
    }
    
    /// Check if format is compressed
    pub fn is_compressed(&self) -> bool {
        matches!(self, Self::CompressedBinary)
    }
}

/// Checkpoint header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointHeader {
    /// Version number
    pub version: u32,
    /// Timestamp when created
    pub timestamp: u64,
    /// Simulation time
    pub sim_time: f64,
    /// Total steps executed
    pub total_steps: u64,
    /// Circuit name
    pub circuit_name: String,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl CheckpointHeader {
    /// Create a new header
    pub fn new(sim_time: f64, total_steps: u64, circuit_name: String) -> Self {
        Self {
            version: CHECKPOINT_VERSION,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            sim_time,
            total_steps,
            circuit_name,
            metadata: HashMap::new(),
        }
    }
    
    /// Add metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

/// Checkpoint data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointData {
    /// Header information
    pub header: CheckpointHeader,
    /// Time state
    pub time_state: TimeState,
    /// Circuit state
    pub circuit_state: CircuitState,
    /// Component states
    pub component_states: HashMap<InstanceId, ComponentState>,
    /// Simulation statistics
    pub statistics: SimulationStats,
    /// Event queue snapshot
    pub event_queue: EventQueueSnapshot,
}

/// Time manager state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeState {
    /// Current simulation time
    pub current_time: f64,
    /// Current time step
    pub time_step: TimeStep,
    /// Total steps executed
    pub total_steps: u64,
    /// Time step history
    pub step_history: Vec<f64>,
}

/// Circuit state snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitState {
    /// Pin values
    pub pin_values: HashMap<(InstanceId, String), PinValue>,
    /// Net voltages
    pub net_voltages: HashMap<NetId, f64>,
    /// Attribute values
    pub attributes: HashMap<String, f64>,
}

/// Simulation statistics snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationStats {
    /// Total evaluations
    pub total_evaluations: u64,
    /// Convergence failures
    pub convergence_failures: u64,
    /// Average time step
    pub avg_time_step: f64,
    /// Peak memory usage
    pub peak_memory: usize,
}

/// Event queue snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventQueueSnapshot {
    /// Number of pending events
    pub pending_count: usize,
    /// Next event time
    pub next_event_time: Option<f64>,
    /// Event types in queue
    pub event_types: Vec<String>,
}

/// Checkpoint metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointMetadata {
    /// File path
    pub path: String,
    /// Format
    pub format: String,
    /// Size in bytes
    pub size: usize,
    /// Creation time
    pub created: u64,
    /// Description
    pub description: Option<String>,
}

impl CheckpointMetadata {
    /// Create from file
    pub fn from_file(path: &str, format: CheckpointFormat) -> std::io::Result<Self> {
        let metadata = std::fs::metadata(path)?;
        
        Ok(Self {
            path: path.to_string(),
            format: format!("{:?}", format),
            size: metadata.len() as usize,
            created: metadata.modified()?
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            description: None,
        })
    }
}