use std::io::{Write, BufWriter};
use std::fs::File;
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use crate::circuit::PinValue;
use crate::propagation::{LogicLevel, DriveStrength};
use crate::error::{SimulationResult, SimulationError};
use super::waveform::{WaveformCapture, SignalTrace};

// Helper macro for IO operations
macro_rules! write_io {
    ($writer:expr, $($arg:tt)*) => {
        write!($writer, $($arg)*).map_err(|e| SimulationError::IoError(format!("Write failed: {}", e)))
    };
}

macro_rules! writeln_io {
    ($writer:expr) => {
        writeln!($writer).map_err(|e| SimulationError::IoError(format!("Write failed: {}", e)))
    };
    ($writer:expr, $($arg:tt)*) => {
        writeln!($writer, $($arg)*).map_err(|e| SimulationError::IoError(format!("Write failed: {}", e)))
    };
}

/// VCD file configuration
#[derive(Debug, Clone)]
pub struct VcdConfig {
    pub timescale: String,
    pub date: DateTime<Utc>,
    pub version: String,
    pub comment: Option<String>,
}

impl Default for VcdConfig {
    fn default() -> Self {
        Self {
            timescale: "1ns".to_string(),
            date: Utc::now(),
            version: "BHDL Simulator 1.0".to_string(),
            comment: None,
        }
    }
}

/// VCD file writer
pub struct VcdWriter {
    writer: BufWriter<File>,
    config: VcdConfig,
    signal_ids: HashMap<String, String>,
    next_id: u32,
    current_time: f64,
    time_scale_factor: f64,
}

impl VcdWriter {
    pub fn new(path: &str, config: VcdConfig) -> SimulationResult<Self> {
        let file = File::create(path)
            .map_err(|e| SimulationError::IoError(format!("Failed to create VCD file: {}", e)))?;
        
        let time_scale_factor = Self::parse_timescale(&config.timescale)?;
        
        Ok(Self {
            writer: BufWriter::new(file),
            config,
            signal_ids: HashMap::new(),
            next_id: 33, // Start with '!' in ASCII
            current_time: -1.0,
            time_scale_factor,
        })
    }

    fn parse_timescale(timescale: &str) -> SimulationResult<f64> {
        // Parse timescale like "1ns", "100ps", "1us"
        let (value_str, unit) = timescale.split_at(
            timescale.find(|c: char| c.is_alphabetic())
                .ok_or_else(|| SimulationError::ConfigError("Invalid timescale format".to_string()))?
        );
        
        let value: f64 = value_str.parse()
            .map_err(|_| SimulationError::ConfigError("Invalid timescale value".to_string()))?;
        
        let unit_factor = match unit {
            "s" => 1.0,
            "ms" => 1e-3,
            "us" => 1e-6,
            "ns" => 1e-9,
            "ps" => 1e-12,
            "fs" => 1e-15,
            _ => return Err(SimulationError::ConfigError(format!("Unknown timescale unit: {}", unit))),
        };
        
        Ok(value * unit_factor)
    }

    pub fn write_header(&mut self, capture: &WaveformCapture) -> SimulationResult<()> {
        // Write header section
        writeln_io!(self.writer, "$date")?;
        writeln_io!(self.writer, "   {}", self.config.date.format("%a %b %d %H:%M:%S %Y"))?;
        writeln_io!(self.writer, "$end")?;
        
        writeln_io!(self.writer, "$version")?;
        writeln_io!(self.writer, "   {}", self.config.version)?;
        writeln_io!(self.writer, "$end")?;
        
        if let Some(comment) = &self.config.comment {
            writeln_io!(self.writer, "$comment")?;
            writeln_io!(self.writer, "   {}", comment)?;
            writeln_io!(self.writer, "$end")?;
        }
        
        writeln_io!(self.writer, "$timescale {}{} $end", 
            self.config.timescale.chars().take_while(|c| c.is_numeric() || *c == '.').collect::<String>(),
            self.config.timescale.chars().skip_while(|c| c.is_numeric() || *c == '.').collect::<String>()
        )?;
        
        // Write variable definitions
        writeln_io!(self.writer, "$scope module top $end")?;
        
        let hierarchy = capture.get_hierarchy();
        self.write_hierarchy("top", capture, hierarchy)?;
        
        writeln_io!(self.writer, "$upscope $end")?;
        writeln_io!(self.writer, "$enddefinitions $end")?;
        
        Ok(())
    }

    fn write_hierarchy(&mut self, scope: &str, capture: &WaveformCapture, hierarchy: &std::collections::BTreeMap<String, Vec<String>>) -> SimulationResult<()> {
        // Write signals in current scope
        for (path, trace) in capture.get_all_signals() {
            if self.is_in_scope(path, scope) {
                let signal_name = self.get_signal_name(path, scope);
                let id = self.allocate_id();
                self.signal_ids.insert(path.clone(), id.clone());
                
                let width = trace.metadata.get("width")
                    .and_then(|w| w.parse::<u32>().ok())
                    .unwrap_or(1);
                
                let var_type = trace.metadata.get("type")
                    .map(|t| t.as_str())
                    .unwrap_or("wire");
                
                writeln_io!(self.writer, "$var {} {} {} {} $end", var_type, width, id, signal_name)?;
            }
        }
        
        // Write subscopes
        if let Some(children) = hierarchy.get(scope) {
            // Get unique module names
            let mut modules = std::collections::HashSet::new();
            for child in children {
                if let Some(module_name) = self.get_module_name(child, scope) {
                    modules.insert(module_name);
                }
            }
            
            for module in modules {
                let full_scope = if scope == "top" {
                    format!("top.{}", module)
                } else {
                    format!("{}.{}", scope, module)
                };
                writeln_io!(self.writer, "$scope module {} $end", module)?;
                self.write_hierarchy(&full_scope, capture, hierarchy)?;
                writeln_io!(self.writer, "$upscope $end")?;
            }
        }
        
        Ok(())
    }

    fn is_in_scope(&self, path: &str, scope: &str) -> bool {
        if scope == "top" {
            !path.contains('.')
        } else {
            path.starts_with(&format!("{}.", scope)) && 
            path[scope.len() + 1..].chars().filter(|&c| c == '.').count() == 0
        }
    }

    fn get_signal_name(&self, path: &str, scope: &str) -> String {
        if scope == "top" {
            path.to_string()
        } else {
            path[scope.len() + 1..].to_string()
        }
    }

    fn get_module_name(&self, path: &str, scope: &str) -> Option<String> {
        let relative = if scope == "top" {
            path
        } else {
            &path[scope.len() + 1..]
        };
        
        relative.split('.').next().map(|s| s.to_string())
    }

    fn allocate_id(&mut self) -> String {
        let id = self.next_id;
        self.next_id += 1;
        
        // Convert to VCD identifier (printable ASCII)
        if id < 127 {
            (id as u8 as char).to_string()
        } else {
            // Multi-character identifier
            let mut result = String::new();
            let mut n = id - 127;
            while n > 0 {
                result.push((33 + (n % 94)) as u8 as char);
                n /= 94;
            }
            result
        }
    }

    pub fn write_initial_values(&mut self, capture: &WaveformCapture) -> SimulationResult<()> {
        writeln_io!(self.writer, "$dumpvars")?;
        
        for (path, trace) in capture.get_all_signals() {
            if let Some(id) = self.signal_ids.get(path).cloned() {
                if let Some(first_point) = trace.points.first() {
                    self.write_value_change(&id, &first_point.value)?;
                } else {
                    // Write unknown value
                    writeln_io!(self.writer, "x{}", id)?;
                }
            }
        }
        
        writeln_io!(self.writer, "$end")?;
        Ok(())
    }

    pub fn write_time_step(&mut self, time: f64, changes: &HashMap<String, PinValue>) -> SimulationResult<()> {
        // Convert time to VCD time units
        let vcd_time = (time / self.time_scale_factor).round() as u64;
        
        if vcd_time as f64 != self.current_time {
            writeln_io!(self.writer, "#{}", vcd_time)?;
            self.current_time = vcd_time as f64;
        }
        
        for (path, value) in changes {
            if let Some(id) = self.signal_ids.get(path).cloned() {
                self.write_value_change(&id, value)?;
            }
        }
        
        Ok(())
    }

    pub fn write_trace(&mut self, trace: &SignalTrace) -> SimulationResult<()> {
        let id = self.signal_ids.get(&trace.name)
            .ok_or_else(|| SimulationError::ProbeError(format!("Signal {} not registered", trace.name)))?
            .clone();
        
        for point in &trace.points {
            let vcd_time = (point.time / self.time_scale_factor).round() as u64;
            
            if vcd_time as f64 != self.current_time {
                writeln_io!(self.writer, "#{}", vcd_time)?;
                self.current_time = vcd_time as f64;
            }
            
            self.write_value_change(&id, &point.value)?;
        }
        
        Ok(())
    }

    fn write_value_change(&mut self, id: &str, value: &PinValue) -> SimulationResult<()> {
        match value {
            _ if value.is_digital() => {
                let logic_char = match value.logic_level {
                    Some(LogicLevel::Low) => '0',
                    Some(LogicLevel::High) => '1',
                    Some(LogicLevel::Unknown) => 'x',
                    Some(LogicLevel::HighZ) => 'z',
                    None => 'x',
                };
                
                // For strong/weak drive, use extended VCD format
                match value.drive_strength {
                    DriveStrength::Strong => writeln_io!(self.writer, "{}{}", logic_char, id)?,
                    DriveStrength::Weak => writeln_io!(self.writer, "{}{}W", logic_char, id)?,
                    DriveStrength::None => writeln_io!(self.writer, "{}{}", logic_char, id)?,
                }
            }
            _ if value.is_analog() => {
                // VCD doesn't directly support analog, use real format
                writeln_io!(self.writer, "r{} {}", value.voltage, id)?;
            }
            _ => {
                // Unknown/uninitialized
                writeln_io!(self.writer, "x{}", id)?;
            }
        }
        
        Ok(())
    }

    pub fn write_all_traces(&mut self, capture: &WaveformCapture) -> SimulationResult<()> {
        // Collect all time points
        let mut all_times = std::collections::BTreeSet::new();
        for trace in capture.get_all_signals().values() {
            for point in &trace.points {
                all_times.insert((point.time / self.time_scale_factor).round() as u64);
            }
        }
        
        // Write values at each time point
        for vcd_time in all_times {
            writeln_io!(self.writer, "#{}", vcd_time)?;
            self.current_time = vcd_time as f64;
            
            let real_time = vcd_time as f64 * self.time_scale_factor;
            
            for (path, trace) in capture.get_all_signals() {
                if let Some(id) = self.signal_ids.get(path).cloned() {
                    if let Some(value) = trace.get_value_at(real_time) {
                        self.write_value_change(&id, value)?;
                    }
                }
            }
        }
        
        Ok(())
    }

    pub fn finish(mut self) -> SimulationResult<()> {
        self.writer.flush()
            .map_err(|e| SimulationError::IoError(format!("Failed to flush VCD file: {}", e)))?;
        Ok(())
    }
}

impl Write for VcdWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.writer.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_vcd_writer() {
        let dir = tempdir().unwrap();
        let vcd_path = dir.path().join("test.vcd");
        
        // Create capture and add signals
        let mut capture = WaveformCapture::new(1000);
        capture.register_signal("clk", HashMap::from([("type".to_string(), "wire".to_string())]));
        capture.register_signal("reset", HashMap::from([("type".to_string(), "wire".to_string())]));
        capture.register_signal("data", HashMap::from([
            ("type".to_string(), "wire".to_string()),
            ("width".to_string(), "8".to_string()),
        ]));
        
        // Add some values
        capture.capture_value("clk", 0.0, PinValue::digital(LogicLevel::Low)).unwrap();
        capture.capture_value("clk", 5e-9, PinValue::digital(LogicLevel::High)).unwrap();
        capture.capture_value("clk", 10e-9, PinValue::digital(LogicLevel::Low)).unwrap();
        capture.capture_value("reset", 0.0, PinValue::digital(LogicLevel::High)).unwrap();
        capture.capture_value("reset", 20e-9, PinValue::digital(LogicLevel::Low)).unwrap();
        
        // Write VCD
        let config = VcdConfig::default();
        let mut writer = VcdWriter::new(vcd_path.to_str().unwrap(), config).unwrap();
        writer.write_header(&capture).unwrap();
        writer.write_initial_values(&capture).unwrap();
        writer.write_all_traces(&capture).unwrap();
        writer.finish().unwrap();
        
        // Verify file was created and contains expected content
        let content = fs::read_to_string(&vcd_path).unwrap();
        assert!(content.contains("$timescale"));
        assert!(content.contains("$var wire 1"));
        assert!(content.contains("#0"));
        assert!(content.contains("#5"));
        assert!(content.contains("#10"));
        assert!(content.contains("#20"));
    }

    #[test]
    fn test_timescale_parsing() {
        assert_eq!(VcdWriter::parse_timescale("1ns").unwrap(), 1e-9);
        assert_eq!(VcdWriter::parse_timescale("100ps").unwrap(), 100e-12);
        assert_eq!(VcdWriter::parse_timescale("1us").unwrap(), 1e-6);
        assert_eq!(VcdWriter::parse_timescale("10ms").unwrap(), 10e-3);
        assert!(VcdWriter::parse_timescale("1xs").is_err());
        assert!(VcdWriter::parse_timescale("ns").is_err());
    }
}