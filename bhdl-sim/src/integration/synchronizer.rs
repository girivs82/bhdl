//! Mixed-signal synchronization logic
//! 
//! Coordinates time stepping between analog and digital domains to ensure
//! accurate simulation while maintaining efficiency.

use std::collections::{HashMap, BTreeSet};
use crate::error::{SimulationResult, SimulationError};
use bhdl_netlist::NetId;
use ordered_float::OrderedFloat;

/// Synchronization strategy for mixed-signal simulation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStrategy {
    /// Lock-step: both domains advance together
    LockStep,
    /// Event-driven: domains sync only at interface events
    EventDriven,
    /// Adaptive: dynamically choose based on activity
    Adaptive,
}

/// Synchronization point in time
#[derive(Debug, Clone)]
pub struct SyncPoint {
    /// Time of synchronization
    pub time: f64,
    /// Reason for sync
    pub reason: SyncReason,
    /// Nets involved
    pub nets: Vec<NetId>,
}

/// Reason for synchronization
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncReason {
    /// Regular interval sync
    Periodic,
    /// Digital event crossing to analog
    DigitalEvent,
    /// Analog threshold crossing
    AnalogThreshold,
    /// Convergence requirement
    Convergence,
    /// User-defined breakpoint
    Breakpoint,
}

/// Mixed-signal synchronizer
pub struct MixedSignalSynchronizer {
    /// Current synchronization strategy
    strategy: SyncStrategy,
    
    /// Next scheduled sync points
    sync_queue: BTreeSet<OrderedFloat<f64>>,
    
    /// Sync history for debugging
    sync_history: Vec<SyncPoint>,
    
    /// Digital event times
    digital_events: BTreeSet<OrderedFloat<f64>>,
    
    /// Analog critical times (e.g., zero crossings)
    analog_events: BTreeSet<OrderedFloat<f64>>,
    
    /// Interface nets that require synchronization
    interface_nets: Vec<NetId>,
    
    /// Configuration
    config: SyncConfig,
    
    /// Performance metrics
    metrics: SyncMetrics,
}

/// Synchronization configuration
#[derive(Debug, Clone)]
pub struct SyncConfig {
    /// Maximum time between syncs (for periodic strategy)
    pub max_sync_interval: f64,
    
    /// Minimum time between syncs (to avoid thrashing)
    pub min_sync_interval: f64,
    
    /// Threshold for analog changes that trigger sync
    pub analog_change_threshold: f64,
    
    /// Whether to sync on all digital events
    pub sync_all_digital_events: bool,
    
    /// Maximum lookahead for event prediction
    pub max_lookahead: f64,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            max_sync_interval: 1e-6,      // 1 microsecond
            min_sync_interval: 1e-12,     // 1 picosecond
            analog_change_threshold: 0.1,  // 100mV change
            sync_all_digital_events: true,
            max_lookahead: 1e-3,          // 1 millisecond
        }
    }
}

/// Synchronization metrics
#[derive(Debug, Default)]
struct SyncMetrics {
    total_syncs: usize,
    digital_event_syncs: usize,
    analog_event_syncs: usize,
    periodic_syncs: usize,
    rejected_syncs: usize,
    avg_sync_interval: f64,
}

impl MixedSignalSynchronizer {
    /// Create a new synchronizer
    pub fn new(strategy: SyncStrategy, interface_nets: Vec<NetId>) -> Self {
        Self {
            strategy,
            sync_queue: BTreeSet::new(),
            sync_history: Vec::new(),
            digital_events: BTreeSet::new(),
            analog_events: BTreeSet::new(),
            interface_nets,
            config: SyncConfig::default(),
            metrics: SyncMetrics::default(),
        }
    }
    
    /// Create with custom configuration
    pub fn with_config(mut self, config: SyncConfig) -> Self {
        self.config = config;
        self
    }
    
    /// Register a digital event time
    pub fn add_digital_event(&mut self, time: f64) {
        if time > 0.0 && time.is_finite() {
            self.digital_events.insert(OrderedFloat(time));
            
            // Add to sync queue if using appropriate strategy
            match self.strategy {
                SyncStrategy::LockStep | SyncStrategy::EventDriven => {
                    self.schedule_sync(time, SyncReason::DigitalEvent);
                }
                SyncStrategy::Adaptive => {
                    // Decide based on current activity
                    if self.should_sync_on_digital_event() {
                        self.schedule_sync(time, SyncReason::DigitalEvent);
                    }
                }
            }
        }
    }
    
    /// Register an analog critical time
    pub fn add_analog_event(&mut self, time: f64, net: NetId) {
        if time > 0.0 && time.is_finite() {
            self.analog_events.insert(OrderedFloat(time));
            
            // Always sync on analog thresholds for interface nets
            if self.interface_nets.contains(&net) {
                self.schedule_sync(time, SyncReason::AnalogThreshold);
            }
        }
    }
    
    /// Get the next synchronization time
    pub fn next_sync_time(&self, current_time: f64) -> Option<f64> {
        // Find next time after current
        self.sync_queue
            .range(OrderedFloat(current_time)..)
            .next()
            .map(|t| t.0)
    }
    
    /// Check if synchronization is needed at current time
    pub fn needs_sync(&self, current_time: f64, last_sync_time: f64) -> bool {
        // Check minimum interval
        if current_time - last_sync_time < self.config.min_sync_interval {
            return false;
        }
        
        // Check if we're at a sync point
        if self.sync_queue.contains(&OrderedFloat(current_time)) {
            return true;
        }
        
        // Check maximum interval for periodic sync
        if current_time - last_sync_time >= self.config.max_sync_interval {
            return true;
        }
        
        false
    }
    
    /// Perform synchronization
    pub fn synchronize(
        &mut self,
        current_time: f64,
        analog_values: &HashMap<NetId, f64>,
        digital_values: &HashMap<NetId, bool>,
    ) -> SimulationResult<SyncResult> {
        let start = std::time::Instant::now();
        
        // Remove this sync point from queue
        self.sync_queue.remove(&OrderedFloat(current_time));
        
        // Determine sync reason
        let reason = if self.digital_events.contains(&OrderedFloat(current_time)) {
            self.metrics.digital_event_syncs += 1;
            SyncReason::DigitalEvent
        } else if self.analog_events.contains(&OrderedFloat(current_time)) {
            self.metrics.analog_event_syncs += 1;
            SyncReason::AnalogThreshold
        } else {
            self.metrics.periodic_syncs += 1;
            SyncReason::Periodic
        };
        
        // Find nets that need value exchange
        let mut changed_nets = Vec::new();
        for &net in &self.interface_nets {
            // Check if values differ significantly
            if let (Some(&analog_v), Some(&digital_v)) = 
                (analog_values.get(&net), digital_values.get(&net)) {
                let digital_voltage = if digital_v { 5.0 } else { 0.0 };
                if (analog_v - digital_voltage).abs() > self.config.analog_change_threshold {
                    changed_nets.push(net);
                }
            }
        }
        
        // Record sync point
        self.sync_history.push(SyncPoint {
            time: current_time,
            reason: reason.clone(),
            nets: changed_nets.clone(),
        });
        
        // Update metrics
        self.metrics.total_syncs += 1;
        if self.sync_history.len() > 1 {
            let intervals: Vec<f64> = self.sync_history.windows(2)
                .map(|w| w[1].time - w[0].time)
                .collect();
            self.metrics.avg_sync_interval = 
                intervals.iter().sum::<f64>() / intervals.len() as f64;
        }
        
        // Schedule next periodic sync if needed
        if self.strategy == SyncStrategy::LockStep {
            self.schedule_sync(
                current_time + self.config.max_sync_interval,
                SyncReason::Periodic
            );
        }
        
        Ok(SyncResult {
            nets_updated: changed_nets,
            sync_time: start.elapsed().as_secs_f64(),
            next_sync: self.next_sync_time(current_time),
        })
    }
    
    /// Schedule a sync point
    fn schedule_sync(&mut self, time: f64, _reason: SyncReason) {
        if time > 0.0 && time.is_finite() {
            self.sync_queue.insert(OrderedFloat(time));
        }
    }
    
    /// Decide if we should sync on digital event (for adaptive strategy)
    fn should_sync_on_digital_event(&self) -> bool {
        // Simple heuristic: sync if we haven't synced recently
        if let Some(last_sync) = self.sync_history.last() {
            let time_since_sync = self.digital_events.iter()
                .next_back()
                .map(|t| t.0 - last_sync.time)
                .unwrap_or(0.0);
            
            time_since_sync > self.config.max_sync_interval * 0.5
        } else {
            true // Always sync on first event
        }
    }
    
    /// Get synchronization metrics
    pub fn metrics(&self) -> String {
        format!(
            "Synchronization Metrics:\n\
             - Total syncs: {}\n\
             - Digital event syncs: {}\n\
             - Analog event syncs: {}\n\
             - Periodic syncs: {}\n\
             - Rejected syncs: {}\n\
             - Average sync interval: {:.3e}s",
            self.metrics.total_syncs,
            self.metrics.digital_event_syncs,
            self.metrics.analog_event_syncs,
            self.metrics.periodic_syncs,
            self.metrics.rejected_syncs,
            self.metrics.avg_sync_interval
        )
    }
    
    /// Clear event queues (call after sync)
    pub fn clear_past_events(&mut self, current_time: f64) {
        // Remove events before current time
        self.digital_events = self.digital_events
            .split_off(&OrderedFloat(current_time));
        self.analog_events = self.analog_events
            .split_off(&OrderedFloat(current_time));
        
        // Keep sync history bounded
        if self.sync_history.len() > 1000 {
            self.sync_history.drain(0..500);
        }
    }
}

/// Result of synchronization
#[derive(Debug)]
pub struct SyncResult {
    /// Nets that had values updated
    pub nets_updated: Vec<NetId>,
    /// Time taken for synchronization
    pub sync_time: f64,
    /// Next scheduled sync time
    pub next_sync: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_lockstep_sync() {
        let interface_nets = vec![NetId::default()];
        let mut sync = MixedSignalSynchronizer::new(
            SyncStrategy::LockStep,
            interface_nets
        );
        
        // Should schedule periodic syncs
        assert!(sync.needs_sync(1e-6, 0.0));
        assert!(!sync.needs_sync(1e-12, 0.0)); // Too soon
    }
    
    #[test]
    fn test_event_driven_sync() {
        let interface_nets = vec![NetId::default()];
        let mut sync = MixedSignalSynchronizer::new(
            SyncStrategy::EventDriven,
            interface_nets
        );
        
        // Add digital event
        sync.add_digital_event(1e-6);
        assert_eq!(sync.next_sync_time(0.0), Some(1e-6));
        
        // Add analog event
        sync.add_analog_event(5e-7, NetId::default());
        assert_eq!(sync.next_sync_time(0.0), Some(5e-7));
    }
    
    #[test]
    fn test_sync_metrics() {
        let interface_nets = vec![NetId::default()];
        let mut sync = MixedSignalSynchronizer::new(
            SyncStrategy::LockStep,
            interface_nets
        );
        
        let mut analog_values = HashMap::new();
        analog_values.insert(NetId::default(), 2.5);
        
        let mut digital_values = HashMap::new();
        digital_values.insert(NetId::default(), true);
        
        // Perform sync
        let result = sync.synchronize(1e-6, &analog_values, &digital_values).unwrap();
        assert!(!result.nets_updated.is_empty());
        
        // Check metrics
        assert!(sync.metrics().contains("Total syncs: 1"));
    }
}