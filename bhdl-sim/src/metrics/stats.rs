//! Simulation statistics structures

use std::collections::HashMap;
use std::time::Duration;
use bhdl_netlist::{InstanceId, NetId, ModuleId};

/// Overall simulation statistics
#[derive(Debug, Default, Clone)]
pub struct SimulationStats {
    /// Total simulation time
    pub total_time: f64,
    /// Total number of time steps
    pub total_steps: u64,
    /// Total number of events
    pub total_events: u64,
    /// Number of convergence failures
    pub convergence_failures: u64,
    /// Maximum convergence iterations
    pub max_convergence_iterations: usize,
    /// Total evaluation count
    pub total_evaluations: u64,
    /// Total propagation count
    pub total_propagations: u64,
    /// Performance statistics
    pub performance: PerformanceStats,
    /// Component statistics
    pub components: HashMap<InstanceId, ComponentStats>,
    /// Net statistics
    pub nets: HashMap<NetId, NetStats>,
    /// Module statistics
    pub modules: HashMap<ModuleId, ModuleStats>,
}

/// Performance-related statistics
#[derive(Debug, Default, Clone)]
pub struct PerformanceStats {
    /// Wall clock time
    pub wall_time: Duration,
    /// CPU time spent in evaluation
    pub evaluation_time: Duration,
    /// CPU time spent in propagation
    pub propagation_time: Duration,
    /// CPU time spent in output
    pub output_time: Duration,
    /// Peak memory usage
    pub peak_memory_mb: f64,
    /// Average memory usage
    pub avg_memory_mb: f64,
    /// Simulation speed (sim time / wall time)
    pub simulation_speed: f64,
}

/// Per-component statistics
#[derive(Debug, Default, Clone)]
pub struct ComponentStats {
    /// Component name
    pub name: String,
    /// Module type
    pub module_type: String,
    /// Number of evaluations
    pub evaluation_count: u64,
    /// Total evaluation time
    pub evaluation_time: Duration,
    /// Average evaluation time
    pub avg_evaluation_time: Duration,
    /// Number of errors
    pub error_count: u64,
    /// Pin change count
    pub pin_changes: HashMap<String, u64>,
    /// Power consumption (if tracked)
    pub power_consumption: Option<f64>,
}

/// Per-net statistics
#[derive(Debug, Default, Clone)]
pub struct NetStats {
    /// Net name
    pub name: Option<String>,
    /// Number of value changes
    pub change_count: u64,
    /// Number of conflicts
    pub conflict_count: u64,
    /// Average voltage
    pub avg_voltage: f64,
    /// Peak current
    pub peak_current: f64,
    /// Connected components
    pub connection_count: usize,
}

/// Per-module type statistics
#[derive(Debug, Default, Clone)]
pub struct ModuleStats {
    /// Module name
    pub name: String,
    /// Number of instances
    pub instance_count: usize,
    /// Total evaluations across all instances
    pub total_evaluations: u64,
    /// Total errors across all instances
    pub total_errors: u64,
    /// Average evaluation time
    pub avg_evaluation_time: Duration,
}

impl SimulationStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update component statistics
    pub fn update_component(&mut self, id: InstanceId, name: String, module_type: String) {
        self.components.entry(id).or_insert_with(|| ComponentStats {
            name,
            module_type,
            ..Default::default()
        });
    }

    /// Record component evaluation
    pub fn record_evaluation(&mut self, id: InstanceId, duration: Duration) {
        self.total_evaluations += 1;
        
        if let Some(stats) = self.components.get_mut(&id) {
            stats.evaluation_count += 1;
            stats.evaluation_time += duration;
            stats.avg_evaluation_time = stats.evaluation_time / stats.evaluation_count as u32;
        }
    }

    /// Record pin change
    pub fn record_pin_change(&mut self, instance_id: InstanceId, pin_name: &str) {
        if let Some(stats) = self.components.get_mut(&instance_id) {
            *stats.pin_changes.entry(pin_name.to_string()).or_insert(0) += 1;
        }
    }

    /// Record net change
    pub fn record_net_change(&mut self, net_id: NetId) {
        self.total_events += 1;
        self.nets.entry(net_id).or_default().change_count += 1;
    }

    /// Record net conflict
    pub fn record_net_conflict(&mut self, net_id: NetId) {
        self.nets.entry(net_id).or_default().conflict_count += 1;
    }

    /// Update net statistics
    pub fn update_net_stats(&mut self, net_id: NetId, voltage: f64, current: f64) {
        let stats = self.nets.entry(net_id).or_default();
        
        // Update average voltage (simple running average)
        if stats.change_count > 0 {
            stats.avg_voltage = (stats.avg_voltage * (stats.change_count - 1) as f64 + voltage) 
                / stats.change_count as f64;
        } else {
            stats.avg_voltage = voltage;
        }
        
        // Update peak current
        if current.abs() > stats.peak_current {
            stats.peak_current = current.abs();
        }
    }

    /// Calculate module statistics
    pub fn calculate_module_stats(&mut self) {
        self.modules.clear();
        
        // Group by module type
        for (_, comp_stats) in &self.components {
            let module_stats = self.modules
                .entry(ModuleId::default()) // This is a simplification
                .or_insert_with(|| ModuleStats {
                    name: comp_stats.module_type.clone(),
                    ..Default::default()
                });
            
            module_stats.instance_count += 1;
            module_stats.total_evaluations += comp_stats.evaluation_count;
            module_stats.total_errors += comp_stats.error_count;
        }
        
        // Calculate averages
        for stats in self.modules.values_mut() {
            if stats.instance_count > 0 && stats.total_evaluations > 0 {
                stats.avg_evaluation_time = self.performance.evaluation_time / stats.total_evaluations as u32;
            }
        }
    }

    /// Generate summary
    pub fn summary(&self) -> String {
        let mut summary = String::new();
        
        summary.push_str(&format!("=== Simulation Statistics ===\n"));
        summary.push_str(&format!("Total Time: {:.9}s\n", self.total_time));
        summary.push_str(&format!("Total Steps: {}\n", self.total_steps));
        summary.push_str(&format!("Total Events: {}\n", self.total_events));
        summary.push_str(&format!("Total Evaluations: {}\n", self.total_evaluations));
        
        if self.total_steps > 0 {
            summary.push_str(&format!("Avg Step Time: {:.3}µs\n", 
                self.performance.wall_time.as_micros() as f64 / self.total_steps as f64));
        }
        
        if self.convergence_failures > 0 {
            summary.push_str(&format!("Convergence Failures: {}\n", self.convergence_failures));
        }
        
        summary.push_str(&format!("\n=== Performance ===\n"));
        summary.push_str(&format!("Wall Time: {:.3}s\n", self.performance.wall_time.as_secs_f64()));
        summary.push_str(&format!("Simulation Speed: {:.2}x\n", self.performance.simulation_speed));
        summary.push_str(&format!("Peak Memory: {:.1} MB\n", self.performance.peak_memory_mb));
        
        summary.push_str(&format!("\n=== Top Components by Evaluations ===\n"));
        let mut components: Vec<_> = self.components.values().collect();
        components.sort_by_key(|c| std::cmp::Reverse(c.evaluation_count));
        
        for (i, comp) in components.iter().take(5).enumerate() {
            summary.push_str(&format!("{}: {} ({}) - {} evals\n", 
                i + 1, comp.name, comp.module_type, comp.evaluation_count));
        }
        
        summary.push_str(&format!("\n=== Top Nets by Changes ===\n"));
        let mut nets: Vec<_> = self.nets.iter().collect();
        nets.sort_by_key(|(_, n)| std::cmp::Reverse(n.change_count));
        
        for (i, (_, net)) in nets.iter().take(5).enumerate() {
            let name = net.name.as_ref()
                .map(|s| s.as_str())
                .unwrap_or("<unnamed>");
            summary.push_str(&format!("{}: {} - {} changes\n", 
                i + 1, name, net.change_count));
        }
        
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_stats() {
        let mut stats = SimulationStats::new();
        let instance_id = InstanceId::default();
        
        stats.update_component(instance_id, "cpu".to_string(), "ARM_M4".to_string());
        stats.record_evaluation(instance_id, Duration::from_micros(10));
        stats.record_evaluation(instance_id, Duration::from_micros(20));
        
        let comp_stats = stats.components.get(&instance_id).unwrap();
        assert_eq!(comp_stats.evaluation_count, 2);
        assert_eq!(comp_stats.avg_evaluation_time, Duration::from_micros(15));
    }

    #[test]
    fn test_net_stats() {
        let mut stats = SimulationStats::new();
        let net_id = NetId::default();
        
        stats.record_net_change(net_id);
        stats.update_net_stats(net_id, 3.3, 0.1);
        stats.record_net_change(net_id);
        stats.update_net_stats(net_id, 5.0, 0.2);
        
        let net_stats = stats.nets.get(&net_id).unwrap();
        assert_eq!(net_stats.change_count, 2);
        assert!((net_stats.avg_voltage - 4.15).abs() < 0.01);
        assert_eq!(net_stats.peak_current, 0.2);
    }

    #[test]
    fn test_summary_generation() {
        let mut stats = SimulationStats::new();
        stats.total_time = 1e-6;
        stats.total_steps = 1000;
        stats.total_events = 500;
        stats.performance.wall_time = Duration::from_millis(100);
        stats.performance.simulation_speed = 10.0;
        stats.performance.peak_memory_mb = 256.0;
        
        let summary = stats.summary();
        assert!(summary.contains("Total Time: 0.000001000s"));
        assert!(summary.contains("Total Steps: 1000"));
        assert!(summary.contains("Simulation Speed: 10.00x"));
    }
}