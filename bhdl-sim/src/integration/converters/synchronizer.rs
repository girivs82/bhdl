//! Domain synchronization for mixed-signal simulation
//! 
//! Coordinates time progression between event-driven digital and 
//! time-stepped analog domains.

use std::collections::BTreeSet;
use std::ops::Bound::{Excluded, Unbounded};
use bhdl_netlist::NetId;
use ordered_float::OrderedFloat;

/// Domain synchronization coordinator
#[derive(Debug)]
pub struct DomainSynchronizer {
    /// Current simulation time
    current_time: f64,
    /// Analog domain timestep
    analog_timestep: f64,
    /// Next scheduled digital event time
    next_digital_event_time: Option<f64>,
    /// Forced synchronization points (e.g., for measurements)
    sync_points: BTreeSet<OrderedFloat<f64>>,
    /// Convergence tolerance for coupled systems
    convergence_tolerance: f64,
    /// Maximum iterations for convergence
    max_iterations: usize,
    /// Minimum timestep allowed
    min_timestep: f64,
    /// Maximum timestep allowed
    max_timestep: f64,
    /// Activity tracking for adaptive stepping
    recent_activity: ActivityTracker,
}

/// Result of synchronization query
#[derive(Debug, Clone)]
pub struct SyncResult {
    /// Next time point to simulate
    pub next_time: f64,
    /// Type of synchronization needed
    pub sync_type: SyncType,
    /// Whether iterative solving is required
    pub requires_iteration: bool,
}

/// Type of synchronization at a time point
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SyncType {
    /// Only analog domain needs update
    AnalogStep,
    /// Only digital domain has event
    DigitalEvent,
    /// Both domains need update
    Synchronized,
}

/// Tracks simulation activity for adaptive stepping
#[derive(Debug)]
struct ActivityTracker {
    /// Recent digital events per unit time
    digital_event_rate: f64,
    /// Recent analog voltage change rate
    analog_change_rate: f64,
    /// Nets with high activity
    active_nets: Vec<NetId>,
    /// Time window for rate calculation
    window_size: f64,
    /// Event count in current window
    event_count: usize,
    /// Window start time
    window_start: f64,
    /// Whether at least one full measurement window has completed
    completed_window: bool,
}

impl DomainSynchronizer {
    /// Create a new synchronizer with default settings
    pub fn new() -> Self {
        Self {
            current_time: 0.0,
            analog_timestep: 1e-9,  // 1ns default
            next_digital_event_time: None,
            sync_points: BTreeSet::new(),
            convergence_tolerance: 1e-6,
            max_iterations: 10,
            min_timestep: 1e-12,     // 1ps
            max_timestep: 1e-6,      // 1us
            recent_activity: ActivityTracker::new(),
        }
    }
    
    /// Get the next synchronization point
    pub fn get_next_sync_point(&mut self) -> SyncResult {
        let next_analog = self.current_time + self.analog_timestep;
        let next_digital = self.next_digital_event_time.unwrap_or(f64::INFINITY);
        
        // Check for forced sync points
        let next_forced = self.sync_points
            .range((Excluded(OrderedFloat(self.current_time)), Unbounded))
            .next()
            .map(|&OrderedFloat(t)| t)
            .unwrap_or(f64::INFINITY);
        
        // Find earliest time
        let next_time = next_analog.min(next_digital).min(next_forced);
        
        // Determine sync type
        let at_analog = (next_time - next_analog).abs() < 1e-15;
        let at_digital = (next_time - next_digital).abs() < 1e-15;
        let sync_type = match (at_analog, at_digital) {
            (true, true) => SyncType::Synchronized,
            (true, false) => SyncType::AnalogStep,
            (false, true) => SyncType::DigitalEvent,
            // Neither domain naturally lands here, so this is a forced sync
            // point (e.g. a measurement): both domains must be brought to it.
            (false, false) => SyncType::Synchronized,
        };
        
        // Check if iteration is needed (coupling between domains)
        let requires_iteration = self.check_coupling(sync_type);
        
        SyncResult {
            next_time,
            sync_type,
            requires_iteration,
        }
    }
    
    /// Advance time to the specified point
    pub fn advance_time(&mut self, time: f64) {
        assert!(time >= self.current_time,
                "Cannot advance time backwards: {} -> {}", self.current_time, time);

        let dt = time - self.current_time;
        self.current_time = time;

        // Update activity tracking
        self.recent_activity.update(time);

        // Adapt timestep based on activity
        self.adapt_analog_timestep(dt);
        
        // Remove passed sync points
        self.sync_points.retain(|&OrderedFloat(t)| t > time);
    }
    
    /// Set next digital event time
    pub fn set_next_digital_event(&mut self, time: Option<f64>) {
        if let Some(t) = time {
            assert!(t > self.current_time, 
                    "Digital event must be in future: {} <= {}", t, self.current_time);
        }
        self.next_digital_event_time = time;
    }
    
    /// Add a forced synchronization point
    pub fn add_sync_point(&mut self, time: f64) {
        if time > self.current_time {
            self.sync_points.insert(OrderedFloat(time));
        }
    }
    
    /// Register a digital event for activity tracking
    pub fn register_digital_event(&mut self, net: NetId) {
        self.recent_activity.register_event(net);
    }
    
    /// Register analog activity
    pub fn register_analog_change(&mut self, change_rate: f64) {
        self.recent_activity.register_analog_change(change_rate);
    }
    
    /// Get current analog timestep
    pub fn get_analog_timestep(&self) -> f64 {
        self.analog_timestep
    }
    
    /// Set convergence parameters
    pub fn set_convergence_params(&mut self, tolerance: f64, max_iterations: usize) {
        self.convergence_tolerance = tolerance;
        self.max_iterations = max_iterations;
    }
    
    /// Check if domains are coupled at this sync type
    fn check_coupling(&self, sync_type: SyncType) -> bool {
        match sync_type {
            SyncType::Synchronized => true,  // Always iterate when both update
            SyncType::AnalogStep | SyncType::DigitalEvent => {
                // Check if there are active converters at domain boundaries
                !self.recent_activity.active_nets.is_empty()
            }
        }
    }
    
    /// Adapt analog timestep based on activity
    ///
    /// `dt` is the amount of simulation time covered by the advance that
    /// triggered this adaptation.
    fn adapt_analog_timestep(&mut self, dt: f64) {
        // Calculate desired timestep based on activity
        let activity_factor = self.recent_activity.get_activity_factor();

        // High activity -> smaller timestep
        let desired_timestep = if !self.recent_activity.has_measurement() {
            // No activity data recorded yet: "no information" is not the
            // same as "measured low activity", so keep the current timestep.
            self.analog_timestep
        } else if activity_factor > 0.8 {
            self.analog_timestep * 0.5
        } else if activity_factor > 0.5 {
            self.analog_timestep * 0.8
        } else if activity_factor < 0.2 {
            // Low activity -> relax the timestep: one 1.5x growth step per
            // activity window covered by this advance, so long quiet
            // advances relax proportionally more.
            let quiet_windows = (dt / self.recent_activity.window_size).max(1.0);
            self.analog_timestep * 1.5_f64.powf(quiet_windows)
        } else {
            self.analog_timestep
        };
        
        // Clamp to allowed range
        self.analog_timestep = desired_timestep
            .max(self.min_timestep)
            .min(self.max_timestep);
        
        // If next digital event is soon, reduce timestep
        if let Some(next_event) = self.next_digital_event_time {
            let time_to_event = next_event - self.current_time;
            if time_to_event < self.analog_timestep * 2.0 {
                self.analog_timestep = (time_to_event / 2.0).max(self.min_timestep);
            }
        }
    }
}

impl ActivityTracker {
    fn new() -> Self {
        Self {
            digital_event_rate: 0.0,
            analog_change_rate: 0.0,
            active_nets: Vec::new(),
            window_size: 100e-9,  // 100ns window
            event_count: 0,
            window_start: 0.0,
            completed_window: false,
        }
    }

    fn update(&mut self, current_time: f64) {
        let elapsed = current_time - self.window_start;
        // Check if we need to start a new window
        if elapsed >= self.window_size {
            // Calculate rate for the completed window. Events recorded in it
            // only count as *recent* activity if we haven't advanced more
            // than another full window past them; beyond that the recent
            // past was quiet.
            self.digital_event_rate = if elapsed >= 2.0 * self.window_size {
                0.0
            } else {
                self.event_count as f64 / elapsed
            };
            self.completed_window = true;

            // Start new window
            self.window_start = current_time;
            self.event_count = 0;
            self.active_nets.clear();
        } else {
            // In-progress window: expose activity immediately instead of
            // only at window rollover. Rating over the full window span is
            // conservative; take the max so a fresh rollover measurement is
            // not instantly discarded by a near-empty new window.
            let in_progress_rate = self.event_count as f64 / self.window_size;
            self.digital_event_rate = self.digital_event_rate.max(in_progress_rate);
        }
    }

    /// Whether any activity data has been recorded at all
    fn has_measurement(&self) -> bool {
        self.completed_window || self.event_count > 0 || self.analog_change_rate > 0.0
    }
    
    fn register_event(&mut self, net: NetId) {
        self.event_count += 1;
        if !self.active_nets.contains(&net) {
            self.active_nets.push(net);
        }
    }
    
    fn register_analog_change(&mut self, change_rate: f64) {
        // Exponential moving average
        self.analog_change_rate = 0.9 * self.analog_change_rate + 0.1 * change_rate;
    }
    
    fn get_activity_factor(&self) -> f64 {
        // Normalize event rate (assume 1MHz is high activity)
        let digital_factor = (self.digital_event_rate / 1e6).min(1.0);
        
        // Normalize analog change rate (assume 1V/ns is high activity)  
        let analog_factor = (self.analog_change_rate / 1e9).min(1.0);
        
        // Combined activity factor
        digital_factor.max(analog_factor)
    }
}

impl Default for DomainSynchronizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_synchronization() {
        let mut sync = DomainSynchronizer::new();
        
        // Initially, only analog steps
        let result = sync.get_next_sync_point();
        assert_eq!(result.sync_type, SyncType::AnalogStep);
        assert_eq!(result.next_time, 1e-9);
        
        // Add a digital event
        sync.advance_time(1e-9);
        sync.set_next_digital_event(Some(5e-9));
        
        // Next few steps should be analog
        let result = sync.get_next_sync_point();
        assert_eq!(result.sync_type, SyncType::AnalogStep);
        assert_eq!(result.next_time, 2e-9);
        
        // Eventually hit the digital event. Near the event the analog
        // timestep is refined (see test_timestep_near_event), so we approach
        // it with shrinking analog steps before the event itself fires.
        sync.advance_time(4e-9);
        let mut fired = false;
        for _ in 0..50 {
            let result = sync.get_next_sync_point();
            assert!(result.next_time <= 5e-9,
                    "sync point overshot digital event: {}", result.next_time);
            if result.sync_type == SyncType::DigitalEvent {
                assert_eq!(result.next_time, 5e-9);
                fired = true;
                break;
            }
            assert_eq!(result.sync_type, SyncType::AnalogStep);
            sync.advance_time(result.next_time);
        }
        assert!(fired, "digital event at 5ns never fired");
    }
    
    #[test]
    fn test_synchronized_events() {
        let mut sync = DomainSynchronizer::new();
        
        // Set digital event at exact analog step time
        sync.set_next_digital_event(Some(1e-9));
        
        let result = sync.get_next_sync_point();
        assert_eq!(result.sync_type, SyncType::Synchronized);
        assert_eq!(result.next_time, 1e-9);
        assert!(result.requires_iteration);
    }
    
    #[test]
    fn test_forced_sync_points() {
        let mut sync = DomainSynchronizer::new();
        
        // Add forced sync at 0.5ns
        sync.add_sync_point(0.5e-9);
        
        let result = sync.get_next_sync_point();
        assert_eq!(result.next_time, 0.5e-9);
    }
    
    #[test]
    fn test_adaptive_timestep() {
        let mut sync = DomainSynchronizer::new();
        
        // Register high activity
        // Create a test netlist and net
        let mut netlist = bhdl_netlist::Netlist::new();
        let test_net = netlist.add_net(Some("test".to_string()));
        
        for _ in 0..10 {
            sync.register_digital_event(test_net);
        }
        sync.advance_time(10e-9);
        
        // Timestep should reduce
        assert!(sync.get_analog_timestep() < 1e-9);
        
        // Low activity period
        sync.advance_time(200e-9);
        
        // Timestep should increase
        assert!(sync.get_analog_timestep() > 1e-9);
    }
    
    #[test]
    fn test_timestep_near_event() {
        let mut sync = DomainSynchronizer::new();
        
        // Set event at 1.5ns
        sync.set_next_digital_event(Some(1.5e-9));
        
        // Advance close to event
        sync.advance_time(1.2e-9);
        
        // Timestep should be reduced
        assert!(sync.get_analog_timestep() < 0.3e-9);
    }
}