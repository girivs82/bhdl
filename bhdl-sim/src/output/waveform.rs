use std::collections::{HashMap, BTreeMap};
use crate::circuit::PinValue;
use crate::error::{SimulationResult, SimulationError};

/// A single time point in the waveform
#[derive(Debug, Clone)]
pub struct TimePoint {
    pub time: f64,
    pub value: PinValue,
}

/// Signal trace containing all captured values over time
#[derive(Debug, Clone)]
pub struct SignalTrace {
    /// Signal name/path
    pub name: String,
    /// Sorted by time
    pub points: Vec<TimePoint>,
    /// Signal metadata
    pub metadata: HashMap<String, String>,
}

impl SignalTrace {
    pub fn new(name: String) -> Self {
        Self {
            name,
            points: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn add_point(&mut self, time: f64, value: PinValue) {
        // Only add if value changed or it's the first point
        if self.points.is_empty() || self.points.last().unwrap().value != value {
            self.points.push(TimePoint { time, value });
        }
    }
    
    pub fn add_point_force(&mut self, time: f64, value: PinValue) {
        // Always add the point (used for periodic capture)
        self.points.push(TimePoint { time, value });
    }

    pub fn get_value_at(&self, time: f64) -> Option<&PinValue> {
        // Binary search for the time point
        match self.points.binary_search_by(|p| p.time.partial_cmp(&time).unwrap()) {
            Ok(idx) => Some(&self.points[idx].value),
            Err(idx) => {
                if idx == 0 {
                    None // Before first sample
                } else {
                    Some(&self.points[idx - 1].value) // Return previous value
                }
            }
        }
    }

    pub fn time_range(&self) -> Option<(f64, f64)> {
        if self.points.is_empty() {
            None
        } else {
            Some((self.points.first().unwrap().time, self.points.last().unwrap().time))
        }
    }
}

/// Waveform capture system
pub struct WaveformCapture {
    /// All captured signals indexed by full path
    signals: HashMap<String, SignalTrace>,
    /// Signals organized by hierarchy
    hierarchy: BTreeMap<String, Vec<String>>,
    /// Maximum number of points per signal (for memory management)
    max_points_per_signal: usize,
    /// Whether to capture all changes or sample at intervals
    capture_mode: CaptureMode,
    /// Sampling interval for periodic capture
    sample_interval: f64,
    /// Last sample time for periodic capture
    last_sample_time: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CaptureMode {
    /// Capture every value change
    AllChanges,
    /// Sample at regular intervals
    Periodic,
    /// Capture on specific events
    EventDriven,
}

impl WaveformCapture {
    pub fn new(max_points_per_signal: usize) -> Self {
        Self {
            signals: HashMap::new(),
            hierarchy: BTreeMap::new(),
            max_points_per_signal,
            capture_mode: CaptureMode::AllChanges,
            sample_interval: 1e-9, // 1ns default
            last_sample_time: 0.0,
        }
    }

    pub fn set_capture_mode(&mut self, mode: CaptureMode, sample_interval: Option<f64>) {
        self.capture_mode = mode;
        if let Some(interval) = sample_interval {
            self.sample_interval = interval;
        }
    }

    pub fn register_signal(&mut self, path: &str, metadata: HashMap<String, String>) {
        let mut trace = SignalTrace::new(path.to_string());
        trace.metadata = metadata;
        self.signals.insert(path.to_string(), trace);
        
        // Update hierarchy
        let parts: Vec<&str> = path.split('.').collect();
        if parts.len() > 1 {
            let parent = parts[..parts.len()-1].join(".");
            self.hierarchy.entry(parent).or_insert_with(Vec::new).push(path.to_string());
        }
    }

    pub fn capture_value(&mut self, path: &str, time: f64, value: PinValue) -> SimulationResult<()> {
        // Check if we should capture based on mode
        match self.capture_mode {
            CaptureMode::AllChanges => {
                // Always capture
            }
            CaptureMode::Periodic => {
                // Always capture the first sample
                if self.last_sample_time == 0.0 && time == 0.0 {
                    // First sample - don't skip
                } else if time < self.last_sample_time + self.sample_interval {
                    return Ok(()); // Skip this sample
                }
                self.last_sample_time = time;
            }
            CaptureMode::EventDriven => {
                // Handled externally
                return Ok(());
            }
        }

        // Capture the value
        let trace = self.signals.get_mut(path)
            .ok_or_else(|| SimulationError::ProbeError(format!("Signal {} not registered", path)))?;
        
        // Use force capture for periodic mode to capture even if value hasn't changed
        match self.capture_mode {
            CaptureMode::Periodic => trace.add_point_force(time, value),
            _ => trace.add_point(time, value),
        }
        
        // Check memory limit after adding and compress if needed
        if trace.points.len() > self.max_points_per_signal {
            Self::compress_trace_static(trace);
        }
        
        Ok(())
    }

    pub fn capture_event(&mut self, path: &str, time: f64, value: PinValue) -> SimulationResult<()> {
        if self.capture_mode != CaptureMode::EventDriven {
            return Ok(());
        }
        
        let trace = self.signals.get_mut(path)
            .ok_or_else(|| SimulationError::ProbeError(format!("Signal {} not registered", path)))?;
        
        trace.add_point(time, value);
        Ok(())
    }

    fn compress_trace_static(trace: &mut SignalTrace) {
        if trace.points.len() < 3 {
            return;
        }

        // More aggressive compression: keep every other change
        let mut compressed = Vec::new();
        compressed.push(trace.points[0].clone());
        
        let mut last_kept_value = &trace.points[0].value;
        let mut skip_next = false;
        
        for i in 1..trace.points.len() - 1 {
            let curr = &trace.points[i];
            
            // If value changed from last kept value
            if curr.value != *last_kept_value {
                if !skip_next {
                    compressed.push(curr.clone());
                    last_kept_value = &curr.value;
                }
                skip_next = !skip_next; // Alternate keeping changes
            }
        }
        
        // Always keep last point if it's different from the last kept
        let last = trace.points.last().unwrap();
        if last.value != *last_kept_value {
            compressed.push(last.clone());
        }
        
        trace.points = compressed;
    }

    pub fn get_signal(&self, path: &str) -> Option<&SignalTrace> {
        self.signals.get(path)
    }

    pub fn get_all_signals(&self) -> &HashMap<String, SignalTrace> {
        &self.signals
    }

    pub fn get_hierarchy(&self) -> &BTreeMap<String, Vec<String>> {
        &self.hierarchy
    }

    pub fn clear(&mut self) {
        for trace in self.signals.values_mut() {
            trace.points.clear();
        }
        self.last_sample_time = 0.0;
    }

    pub fn time_range(&self) -> Option<(f64, f64)> {
        let mut min_time = f64::MAX;
        let mut max_time = f64::MIN;
        let mut has_data = false;

        for trace in self.signals.values() {
            if let Some((t_min, t_max)) = trace.time_range() {
                min_time = min_time.min(t_min);
                max_time = max_time.max(t_max);
                has_data = true;
            }
        }

        if has_data {
            Some((min_time, max_time))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::propagation::LogicLevel;

    #[test]
    fn test_signal_trace() {
        let mut trace = SignalTrace::new("test.signal".to_string());
        
        // Add points
        trace.add_point(0.0, PinValue::digital(LogicLevel::Low));
        trace.add_point(1e-9, PinValue::digital(LogicLevel::High));
        trace.add_point(2e-9, PinValue::digital(LogicLevel::High)); // Should not be added
        trace.add_point(3e-9, PinValue::digital(LogicLevel::Low));
        
        // Check only changed values are stored
        assert_eq!(trace.points.len(), 3);
        
        // Test value lookup
        assert_eq!(trace.get_value_at(0.5e-9).unwrap().logic_level.unwrap(), LogicLevel::Low);
        assert_eq!(trace.get_value_at(1.5e-9).unwrap().logic_level.unwrap(), LogicLevel::High);
        assert_eq!(trace.get_value_at(3.5e-9).unwrap().logic_level.unwrap(), LogicLevel::Low);
        
        // Test time range
        let (min, max) = trace.time_range().unwrap();
        assert_eq!(min, 0.0);
        assert_eq!(max, 3e-9);
    }

    #[test]
    fn test_waveform_capture() {
        let mut capture = WaveformCapture::new(1000);
        
        // Register signals
        capture.register_signal("top.clk", HashMap::new());
        capture.register_signal("top.reset", HashMap::new());
        capture.register_signal("top.cpu.data", HashMap::from([
            ("width".to_string(), "8".to_string()),
            ("type".to_string(), "bus".to_string()),
        ]));
        
        // Capture values
        capture.capture_value("top.clk", 0.0, PinValue::digital(LogicLevel::Low)).unwrap();
        capture.capture_value("top.clk", 1e-9, PinValue::digital(LogicLevel::High)).unwrap();
        capture.capture_value("top.reset", 0.0, PinValue::digital(LogicLevel::High)).unwrap();
        capture.capture_value("top.reset", 5e-9, PinValue::digital(LogicLevel::Low)).unwrap();
        
        // Check hierarchy
        let hierarchy = capture.get_hierarchy();
        assert!(hierarchy.contains_key("top"));
        assert!(hierarchy.contains_key("top.cpu"));
        
        // Check signals
        let clk_trace = capture.get_signal("top.clk").unwrap();
        assert_eq!(clk_trace.points.len(), 2);
        
        // Test time range
        let (min, max) = capture.time_range().unwrap();
        assert_eq!(min, 0.0);
        assert_eq!(max, 5e-9);
    }

    #[test]
    fn test_compression() {
        let mut capture = WaveformCapture::new(5); // Small limit to trigger compression
        capture.register_signal("test", HashMap::new());
        
        // Add many points
        for i in 0..10 {
            let value = if i % 3 == 0 { LogicLevel::High } else { LogicLevel::Low };
            capture.capture_value("test", i as f64 * 1e-9, PinValue::digital(value)).unwrap();
        }
        
        // Check that compression happened
        let trace = capture.get_signal("test").unwrap();
        assert!(trace.points.len() <= capture.max_points_per_signal);
    }

    #[test]
    fn test_periodic_capture() {
        let mut capture = WaveformCapture::new(1000);
        capture.set_capture_mode(CaptureMode::Periodic, Some(5e-9));
        capture.register_signal("test", HashMap::new());
        
        // Try to capture at various times
        for i in 0..20 {
            capture.capture_value("test", i as f64 * 1e-9, PinValue::digital(LogicLevel::High)).unwrap();
        }
        
        // Should only have captured at 0, 5, 10, 15 ns
        let trace = capture.get_signal("test").unwrap();
        assert_eq!(trace.points.len(), 4);
        assert!((trace.points[0].time - 0.0).abs() < 1e-15);
        assert!((trace.points[1].time - 5e-9).abs() < 1e-15);
        assert!((trace.points[2].time - 10e-9).abs() < 1e-15);
        assert!((trace.points[3].time - 15e-9).abs() < 1e-15);
    }
}