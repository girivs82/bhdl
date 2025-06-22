//! Test checkpoint and restore functionality

use bhdl_sim::{
    SimulationEngine, SimulationConfig,
    checkpoint::{CheckpointManager, CheckpointFormat, RestoreManager, RestoreOptions},
};
use bhdl_netlist::Netlist;
use tempfile::TempDir;

#[test]
fn test_checkpoint_creation() {
    // Create temp directory for checkpoints
    let temp_dir = TempDir::new().unwrap();
    let checkpoint_dir = temp_dir.path().to_string_lossy().to_string();
    
    // Create dummy netlist
    let netlist = Netlist::new();
    
    // Create simulation engine
    let config = SimulationConfig::default();
    let mut engine = SimulationEngine::new(config, netlist, "test_circuit".to_string()).unwrap();
    
    // Create checkpoint manager
    let mut checkpoint_manager = CheckpointManager::new(checkpoint_dir.clone());
    
    // Create checkpoint
    let path = checkpoint_manager.create_checkpoint(
        &engine,
        CheckpointFormat::Json,
        Some("Test checkpoint".to_string()),
    ).unwrap();
    
    // Verify file exists
    assert!(std::path::Path::new(&path).exists());
    
    // Get checkpoint info
    let info = checkpoint_manager.get_checkpoint_info(&path).unwrap();
    assert_eq!(info.circuit_name, "test_circuit");
    assert_eq!(info.sim_time, 0.0);
}

#[test]
fn test_checkpoint_restore() {
    // Create temp directory
    let temp_dir = TempDir::new().unwrap();
    let checkpoint_dir = temp_dir.path().to_string_lossy().to_string();
    
    // Create initial engine
    let netlist = Netlist::new();
    let config = SimulationConfig::default();
    let mut engine = SimulationEngine::new(config.clone(), netlist, "test_circuit".to_string()).unwrap();
    
    // Advance simulation time
    engine.time_manager.advance();
    engine.time_manager.advance();
    let original_time = engine.current_time();
    
    // Create checkpoint
    let mut checkpoint_manager = CheckpointManager::new(checkpoint_dir);
    let checkpoint_path = checkpoint_manager.create_checkpoint(
        &engine,
        CheckpointFormat::Binary,
        None,
    ).unwrap();
    
    // Create new engine
    let new_netlist = Netlist::new();
    let mut new_engine = SimulationEngine::new(config, new_netlist, "test_circuit".to_string()).unwrap();
    assert_eq!(new_engine.current_time(), 0.0);
    
    // Restore from checkpoint
    let mut restore_manager = RestoreManager::new(RestoreOptions::default());
    let report = restore_manager.restore(&mut new_engine, &checkpoint_path).unwrap();
    
    assert!(report.success);
    assert_eq!(report.restored_time, original_time);
    assert_eq!(new_engine.current_time(), original_time);
}

#[test]
fn test_auto_checkpoint() {
    let temp_dir = TempDir::new().unwrap();
    let checkpoint_dir = temp_dir.path().to_string_lossy().to_string();
    
    // Create engine with checkpoint manager
    let netlist = Netlist::new();
    let config = SimulationConfig::default();
    let engine = SimulationEngine::new(config, netlist, "test_circuit".to_string()).unwrap();
    
    // Set up auto-checkpoint
    let checkpoint_manager = CheckpointManager::new(checkpoint_dir)
        .with_auto_interval(0.001) // Checkpoint every 1ms
        .with_max_checkpoints(5);
    
    let engine = engine.with_checkpoint_manager(checkpoint_manager);
    
    // Verify checkpoint manager is configured
    assert!(engine.checkpoint_manager.is_some());
}

#[test]
fn test_compressed_checkpoint() {
    let temp_dir = TempDir::new().unwrap();
    let checkpoint_dir = temp_dir.path().to_string_lossy().to_string();
    
    let netlist = Netlist::new();
    let config = SimulationConfig::default();
    let mut engine = SimulationEngine::new(config, netlist, "test_circuit".to_string()).unwrap();
    
    let mut checkpoint_manager = CheckpointManager::new(checkpoint_dir);
    
    // Create compressed checkpoint
    let path = checkpoint_manager.create_checkpoint(
        &engine,
        CheckpointFormat::CompressedBinary,
        Some("Compressed test".to_string()),
    ).unwrap();
    
    // Verify file exists and has .bcpz extension
    assert!(path.ends_with(".bcpz"));
    assert!(std::path::Path::new(&path).exists());
    
    // Verify we can read it back
    let info = checkpoint_manager.get_checkpoint_info(&path).unwrap();
    assert_eq!(info.circuit_name, "test_circuit");
}