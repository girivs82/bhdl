//! Checkpoint creation and management

use std::path::Path;
use std::fs::File;
use std::io::{Write, BufWriter};
use flate2::write::GzEncoder;
use flate2::Compression;
use crate::error::{SimulationResult, SimulationError};
use crate::engine::SimulationEngine;
use crate::time::TimeStep;
use super::format::*;

/// Checkpoint manager
pub struct CheckpointManager {
    /// Checkpoint directory
    checkpoint_dir: String,
    /// Maximum checkpoints to keep
    max_checkpoints: usize,
    /// Auto-checkpoint interval (in simulation time)
    auto_interval: Option<f64>,
    /// Last checkpoint time
    last_checkpoint_time: f64,
    /// Checkpoint history
    history: Vec<CheckpointMetadata>,
}

impl CheckpointManager {
    /// Create a new checkpoint manager
    pub fn new(checkpoint_dir: String) -> Self {
        Self {
            checkpoint_dir,
            max_checkpoints: 10,
            auto_interval: None,
            last_checkpoint_time: 0.0,
            history: Vec::new(),
        }
    }
    
    /// Set maximum checkpoints to keep
    pub fn with_max_checkpoints(mut self, max: usize) -> Self {
        self.max_checkpoints = max;
        self
    }
    
    /// Set auto-checkpoint interval
    pub fn with_auto_interval(mut self, interval: f64) -> Self {
        self.auto_interval = Some(interval);
        self
    }
    
    /// Check if auto-checkpoint is needed
    pub fn should_checkpoint(&self, current_time: f64) -> bool {
        if let Some(interval) = self.auto_interval {
            current_time - self.last_checkpoint_time >= interval
        } else {
            false
        }
    }
    
    /// Create a checkpoint
    pub fn create_checkpoint(
        &mut self,
        engine: &SimulationEngine,
        format: CheckpointFormat,
        description: Option<String>,
    ) -> SimulationResult<String> {
        // Ensure checkpoint directory exists
        std::fs::create_dir_all(&self.checkpoint_dir)
            .map_err(|e| SimulationError::IoError(e.to_string()))?;
        
        // Generate filename
        let filename = self.generate_filename(engine.current_time(), format);
        let path = Path::new(&self.checkpoint_dir).join(&filename);
        
        // Create checkpoint data
        let data = self.collect_checkpoint_data(engine)?;
        
        // Write checkpoint
        match format {
            CheckpointFormat::Binary => self.write_binary(&path, &data)?,
            CheckpointFormat::Json => self.write_json(&path, &data)?,
            CheckpointFormat::CompressedBinary => self.write_compressed(&path, &data)?,
        }
        
        // Update metadata
        let mut metadata = CheckpointMetadata::from_file(path.to_str().unwrap(), format)
            .map_err(|e| SimulationError::IoError(e.to_string()))?;
        metadata.description = description;
        
        // Add to history and manage retention
        self.history.push(metadata);
        self.last_checkpoint_time = engine.current_time();
        self.cleanup_old_checkpoints()?;
        
        Ok(path.to_string_lossy().to_string())
    }
    
    /// Collect checkpoint data from engine
    fn collect_checkpoint_data(&self, engine: &SimulationEngine) -> SimulationResult<CheckpointData> {
        // Get time state
        let time_state = TimeState {
            current_time: engine.current_time(),
            time_step: engine.time_manager.current_step().clone(),
            total_steps: engine.total_steps(),
            step_history: engine.time_manager.step_history().to_vec(),
        };
        
        // Get circuit state
        let circuit_state = CircuitState {
            pin_values: engine.circuit_state.get_all_pin_values(),
            net_voltages: engine.circuit_state.get_all_net_voltages(),
            attributes: engine.circuit_state.get_all_attributes_f64(),
        };
        
        // Get component states
        let component_states = engine.circuit_state.get_all_component_states();
        
        // Get statistics
        let stats = engine.stats_collector.get_summary();
        let statistics = SimulationStats {
            total_evaluations: stats.total_evaluations,
            convergence_failures: stats.convergence_failures,
            avg_time_step: stats.avg_time_step,
            peak_memory: stats.peak_memory_mb as usize * 1024 * 1024,
        };
        
        // Get event queue info
        let event_stats = engine.event_dispatcher.statistics();
        let event_queue = EventQueueSnapshot {
            pending_count: event_stats.queue_size,
            next_event_time: None, // Would need to expose from queue
            event_types: Vec::new(), // Would need to collect from queue
        };
        
        // Create header
        let header = CheckpointHeader::new(
            engine.current_time(),
            engine.total_steps(),
            engine.circuit_name().to_string(),
        );
        
        Ok(CheckpointData {
            header,
            time_state,
            circuit_state,
            component_states,
            statistics,
            event_queue,
        })
    }
    
    /// Generate checkpoint filename
    fn generate_filename(&self, sim_time: f64, format: CheckpointFormat) -> String {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        
        format!(
            "checkpoint_t{:.6}_ts{}.{}",
            sim_time,
            timestamp,
            format.extension()
        )
    }
    
    /// Write binary checkpoint
    fn write_binary(&self, path: &Path, data: &CheckpointData) -> SimulationResult<()> {
        let file = File::create(path)
            .map_err(|e| SimulationError::IoError(e.to_string()))?;
        let mut writer = BufWriter::new(file);
        
        // Write magic bytes
        writer.write_all(CHECKPOINT_MAGIC)
            .map_err(|e| SimulationError::IoError(e.to_string()))?;
        
        // Write data
        bincode::serialize_into(writer, data)
            .map_err(|e| SimulationError::IoError(e.to_string()))?;
        
        Ok(())
    }
    
    /// Write JSON checkpoint
    fn write_json(&self, path: &Path, data: &CheckpointData) -> SimulationResult<()> {
        let file = File::create(path)
            .map_err(|e| SimulationError::IoError(e.to_string()))?;
        let writer = BufWriter::new(file);
        
        serde_json::to_writer_pretty(writer, data)
            .map_err(|e| SimulationError::IoError(e.to_string()))?;
        
        Ok(())
    }
    
    /// Write compressed binary checkpoint
    fn write_compressed(&self, path: &Path, data: &CheckpointData) -> SimulationResult<()> {
        let file = File::create(path)
            .map_err(|e| SimulationError::IoError(e.to_string()))?;
        let writer = BufWriter::new(file);
        let mut encoder = GzEncoder::new(writer, Compression::default());
        
        // Write magic bytes
        encoder.write_all(CHECKPOINT_MAGIC)
            .map_err(|e| SimulationError::IoError(e.to_string()))?;
        
        // Write data
        bincode::serialize_into(&mut encoder, data)
            .map_err(|e| SimulationError::IoError(e.to_string()))?;
        
        encoder.finish()
            .map_err(|e| SimulationError::IoError(e.to_string()))?;
        
        Ok(())
    }
    
    /// Clean up old checkpoints
    fn cleanup_old_checkpoints(&mut self) -> SimulationResult<()> {
        if self.history.len() <= self.max_checkpoints {
            return Ok(());
        }
        
        // Sort by creation time
        self.history.sort_by_key(|m| m.created);
        
        // Remove oldest checkpoints
        let to_remove = self.history.len() - self.max_checkpoints;
        for i in 0..to_remove {
            let path = &self.history[i].path;
            std::fs::remove_file(path)
                .map_err(|e| SimulationError::IoError(
                    format!("Failed to remove old checkpoint: {}", e)
                ))?;
        }
        
        // Update history
        self.history.drain(0..to_remove);
        
        Ok(())
    }
    
    /// List available checkpoints
    pub fn list_checkpoints(&self) -> &[CheckpointMetadata] {
        &self.history
    }
    
    /// Get checkpoint info
    pub fn get_checkpoint_info(&self, path: &str) -> SimulationResult<CheckpointHeader> {
        // Determine format from extension
        let format = if path.ends_with(".json") {
            CheckpointFormat::Json
        } else if path.ends_with(".bcpz") {
            CheckpointFormat::CompressedBinary
        } else {
            CheckpointFormat::Binary
        };
        
        // Read header based on format
        match format {
            CheckpointFormat::Json => {
                let file = File::open(path)
                    .map_err(|e| SimulationError::IoError(e.to_string()))?;
                let data: CheckpointData = serde_json::from_reader(file)
                    .map_err(|e| SimulationError::IoError(e.to_string()))?;
                Ok(data.header)
            }
            CheckpointFormat::Binary => {
                let file = File::open(path)
                    .map_err(|e| SimulationError::IoError(e.to_string()))?;
                let mut reader = std::io::BufReader::new(file);
                
                // Skip magic bytes
                let mut magic = [0u8; 8];
                std::io::Read::read_exact(&mut reader, &mut magic)
                    .map_err(|e| SimulationError::IoError(e.to_string()))?;
                
                let data: CheckpointData = bincode::deserialize_from(reader)
                    .map_err(|e| SimulationError::IoError(e.to_string()))?;
                Ok(data.header)
            }
            CheckpointFormat::CompressedBinary => {
                let file = File::open(path)
                    .map_err(|e| SimulationError::IoError(e.to_string()))?;
                let decoder = flate2::read::GzDecoder::new(file);
                let mut reader = std::io::BufReader::new(decoder);
                
                // Skip magic bytes
                let mut magic = [0u8; 8];
                std::io::Read::read_exact(&mut reader, &mut magic)
                    .map_err(|e| SimulationError::IoError(e.to_string()))?;
                
                let data: CheckpointData = bincode::deserialize_from(reader)
                    .map_err(|e| SimulationError::IoError(e.to_string()))?;
                Ok(data.header)
            }
        }
    }
}

/// Checkpoint handle for managing single checkpoint
pub struct Checkpoint {
    /// File path
    path: String,
    /// Format
    format: CheckpointFormat,
    /// Loaded data
    data: Option<CheckpointData>,
}

impl Checkpoint {
    /// Load checkpoint from file
    pub fn load(path: &str) -> SimulationResult<Self> {
        // Determine format from extension
        let format = if path.ends_with(".json") {
            CheckpointFormat::Json
        } else if path.ends_with(".bcpz") {
            CheckpointFormat::CompressedBinary
        } else {
            CheckpointFormat::Binary
        };
        
        Ok(Self {
            path: path.to_string(),
            format,
            data: None,
        })
    }
    
    /// Get checkpoint data
    pub fn data(&mut self) -> SimulationResult<&CheckpointData> {
        if self.data.is_none() {
            self.data = Some(self.read_data()?);
        }
        Ok(self.data.as_ref().unwrap())
    }
    
    /// Read checkpoint data
    fn read_data(&self) -> SimulationResult<CheckpointData> {
        match self.format {
            CheckpointFormat::Json => {
                let file = File::open(&self.path)
                    .map_err(|e| SimulationError::IoError(e.to_string()))?;
                serde_json::from_reader(file)
                    .map_err(|e| SimulationError::IoError(e.to_string()))
            }
            CheckpointFormat::Binary => {
                let file = File::open(&self.path)
                    .map_err(|e| SimulationError::IoError(e.to_string()))?;
                let mut reader = std::io::BufReader::new(file);
                
                // Skip magic bytes
                let mut magic = [0u8; 8];
                std::io::Read::read_exact(&mut reader, &mut magic)
                    .map_err(|e| SimulationError::IoError(e.to_string()))?;
                
                bincode::deserialize_from(reader)
                    .map_err(|e| SimulationError::IoError(e.to_string()))
            }
            CheckpointFormat::CompressedBinary => {
                let file = File::open(&self.path)
                    .map_err(|e| SimulationError::IoError(e.to_string()))?;
                let decoder = flate2::read::GzDecoder::new(file);
                let mut reader = std::io::BufReader::new(decoder);
                
                // Skip magic bytes
                let mut magic = [0u8; 8];
                std::io::Read::read_exact(&mut reader, &mut magic)
                    .map_err(|e| SimulationError::IoError(e.to_string()))?;
                
                bincode::deserialize_from(reader)
                    .map_err(|e| SimulationError::IoError(e.to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_checkpoint_filename_generation() {
        let temp_dir = TempDir::new().unwrap();
        let manager = CheckpointManager::new(temp_dir.path().to_string_lossy().to_string());
        
        let filename = manager.generate_filename(1.5, CheckpointFormat::Binary);
        assert!(filename.starts_with("checkpoint_t1.500000_"));
        assert!(filename.ends_with(".bcp"));
        
        let filename = manager.generate_filename(2.0, CheckpointFormat::Json);
        assert!(filename.ends_with(".json"));
    }
    
    #[test]
    fn test_checkpoint_metadata() {
        let header = CheckpointHeader::new(1.0, 1000, "test_circuit".to_string())
            .with_metadata("user".to_string(), "test".to_string());
        
        assert_eq!(header.sim_time, 1.0);
        assert_eq!(header.total_steps, 1000);
        assert_eq!(header.circuit_name, "test_circuit");
        assert_eq!(header.metadata.get("user"), Some(&"test".to_string()));
    }
}