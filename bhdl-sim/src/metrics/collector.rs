//! Metrics collection system

use std::collections::HashMap;
use std::time::{Duration, Instant};
use bhdl_netlist::{InstanceId, NetId};

/// Type of metric being collected
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MetricType {
    /// Time-based metrics
    SimulationTime,
    StepTime,
    EvaluationTime,
    PropagationTime,
    
    /// Count-based metrics
    TotalSteps,
    TotalEvents,
    ConvergenceIterations,
    ErrorCount,
    
    /// Component metrics
    ComponentEvaluations(InstanceId),
    ComponentErrors(InstanceId),
    
    /// Net metrics
    NetChanges(NetId),
    NetConflicts(NetId),
    
    /// Performance metrics
    MemoryUsage,
    CpuUsage,
    
    /// Custom metrics
    Custom(String),
}

/// Value of a metric
#[derive(Debug, Clone)]
pub enum MetricValue {
    /// Integer value
    Integer(i64),
    /// Floating point value
    Real(f64),
    /// Duration value
    Duration(Duration),
    /// Text value
    Text(String),
    /// Boolean value
    Boolean(bool),
    /// List of values
    List(Vec<MetricValue>),
}

/// Time series data point
#[derive(Debug, Clone)]
pub struct TimeSeriesPoint {
    pub time: f64,
    pub value: MetricValue,
}

/// Metrics collector
pub struct MetricsCollector {
    /// Current metric values
    metrics: HashMap<MetricType, MetricValue>,
    /// Time series data
    time_series: HashMap<MetricType, Vec<TimeSeriesPoint>>,
    /// Performance timers
    timers: HashMap<String, Instant>,
    /// Counters
    counters: HashMap<MetricType, i64>,
    /// Configuration
    enabled: bool,
    time_series_enabled: bool,
    max_time_series_points: usize,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            metrics: HashMap::new(),
            time_series: HashMap::new(),
            timers: HashMap::new(),
            counters: HashMap::new(),
            enabled: true,
            time_series_enabled: true,
            max_time_series_points: 10000,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_time_series_enabled(&mut self, enabled: bool) {
        self.time_series_enabled = enabled;
    }

    /// Record a metric value
    pub fn record(&mut self, metric_type: MetricType, value: MetricValue) {
        if !self.enabled {
            return;
        }

        self.metrics.insert(metric_type.clone(), value.clone());
        
        // Update counter if applicable
        if let MetricValue::Integer(count) = &value {
            self.counters.insert(metric_type.clone(), *count);
        }
    }

    /// Record a time series value
    pub fn record_time_series(&mut self, metric_type: MetricType, time: f64, value: MetricValue) {
        if !self.enabled || !self.time_series_enabled {
            return;
        }

        let series = self.time_series.entry(metric_type).or_insert_with(Vec::new);
        series.push(TimeSeriesPoint { time, value });
        
        // Limit series size
        if series.len() > self.max_time_series_points {
            // Remove oldest 10%
            let remove_count = self.max_time_series_points / 10;
            series.drain(0..remove_count);
        }
    }

    /// Increment a counter
    pub fn increment(&mut self, metric_type: MetricType) {
        if !self.enabled {
            return;
        }

        let count = self.counters.entry(metric_type.clone()).or_insert(0);
        *count += 1;
        self.metrics.insert(metric_type, MetricValue::Integer(*count));
    }

    /// Add to a counter
    pub fn add(&mut self, metric_type: MetricType, amount: i64) {
        if !self.enabled {
            return;
        }

        let count = self.counters.entry(metric_type.clone()).or_insert(0);
        *count += amount;
        self.metrics.insert(metric_type, MetricValue::Integer(*count));
    }

    /// Start a timer
    pub fn start_timer(&mut self, name: &str) {
        if !self.enabled {
            return;
        }

        self.timers.insert(name.to_string(), Instant::now());
    }

    /// Stop a timer and record the duration
    pub fn stop_timer(&mut self, name: &str) -> Option<Duration> {
        if !self.enabled {
            return None;
        }

        if let Some(start) = self.timers.remove(name) {
            let duration = start.elapsed();
            let metric_type = MetricType::Custom(format!("timer.{}", name));
            self.record(metric_type, MetricValue::Duration(duration));
            Some(duration)
        } else {
            None
        }
    }

    /// Get a metric value
    pub fn get(&self, metric_type: &MetricType) -> Option<&MetricValue> {
        self.metrics.get(metric_type)
    }

    /// Get a counter value
    pub fn get_counter(&self, metric_type: &MetricType) -> i64 {
        self.counters.get(metric_type).copied().unwrap_or(0)
    }

    /// Get time series data
    pub fn get_time_series(&self, metric_type: &MetricType) -> Option<&Vec<TimeSeriesPoint>> {
        self.time_series.get(metric_type)
    }

    /// Get all metrics
    pub fn get_all_metrics(&self) -> &HashMap<MetricType, MetricValue> {
        &self.metrics
    }

    /// Clear all metrics
    pub fn clear(&mut self) {
        self.metrics.clear();
        self.time_series.clear();
        self.timers.clear();
        self.counters.clear();
    }

    /// Calculate derived metrics
    pub fn calculate_derived_metrics(&mut self) {
        // Average step time
        if let Some(total_time) = self.get(&MetricType::SimulationTime) {
            if let Some(total_steps) = self.get(&MetricType::TotalSteps) {
                if let (MetricValue::Real(time), MetricValue::Integer(steps)) = (total_time, total_steps) {
                    if *steps > 0 {
                        let avg_step_time = time / (*steps as f64);
                        self.record(
                            MetricType::Custom("avg_step_time".to_string()),
                            MetricValue::Real(avg_step_time),
                        );
                    }
                }
            }
        }

        // Event rate
        if let Some(total_time) = self.get(&MetricType::SimulationTime) {
            if let Some(total_events) = self.get(&MetricType::TotalEvents) {
                if let (MetricValue::Real(time), MetricValue::Integer(events)) = (total_time, total_events) {
                    if *time > 0.0 {
                        let event_rate = (*events as f64) / time;
                        self.record(
                            MetricType::Custom("event_rate".to_string()),
                            MetricValue::Real(event_rate),
                        );
                    }
                }
            }
        }
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter_metrics() {
        let mut collector = MetricsCollector::new();
        
        collector.increment(MetricType::TotalSteps);
        collector.increment(MetricType::TotalSteps);
        collector.add(MetricType::TotalEvents, 5);
        
        assert_eq!(collector.get_counter(&MetricType::TotalSteps), 2);
        assert_eq!(collector.get_counter(&MetricType::TotalEvents), 5);
    }

    #[test]
    fn test_timer_metrics() {
        let mut collector = MetricsCollector::new();
        
        collector.start_timer("test");
        std::thread::sleep(Duration::from_millis(10));
        let duration = collector.stop_timer("test").unwrap();
        
        assert!(duration.as_millis() >= 10);
        
        let metric = collector.get(&MetricType::Custom("timer.test".to_string())).unwrap();
        assert!(matches!(metric, MetricValue::Duration(_)));
    }

    #[test]
    fn test_time_series() {
        let mut collector = MetricsCollector::new();
        
        for i in 0..5 {
            collector.record_time_series(
                MetricType::SimulationTime,
                i as f64,
                MetricValue::Real(i as f64 * 1e-9),
            );
        }
        
        let series = collector.get_time_series(&MetricType::SimulationTime).unwrap();
        assert_eq!(series.len(), 5);
        assert_eq!(series[0].time, 0.0);
    }

    #[test]
    fn test_derived_metrics() {
        let mut collector = MetricsCollector::new();
        
        collector.record(MetricType::SimulationTime, MetricValue::Real(1.0));
        collector.record(MetricType::TotalSteps, MetricValue::Integer(100));
        collector.record(MetricType::TotalEvents, MetricValue::Integer(50));
        
        collector.calculate_derived_metrics();
        
        let avg_step_time = collector.get(&MetricType::Custom("avg_step_time".to_string())).unwrap();
        if let MetricValue::Real(time) = avg_step_time {
            assert_eq!(*time, 0.01);
        }
        
        let event_rate = collector.get(&MetricType::Custom("event_rate".to_string())).unwrap();
        if let MetricValue::Real(rate) = event_rate {
            assert_eq!(*rate, 50.0);
        }
    }
}