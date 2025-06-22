use std::io::{Write, BufWriter};
use std::fs::File;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use crate::circuit::PinValue;
use crate::propagation::LogicLevel;
use crate::error::{SimulationResult, SimulationError};
use super::waveform::WaveformCapture;

// Helper macros for IO operations
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

/// Output format trait
pub trait OutputFormat {
    fn write_header(&mut self, signals: &[String]) -> SimulationResult<()>;
    fn write_time_step(&mut self, time: f64, values: &HashMap<String, PinValue>) -> SimulationResult<()>;
    fn write_traces(&mut self, capture: &WaveformCapture) -> SimulationResult<()>;
    fn finish(self) -> SimulationResult<()>;
}

/// CSV output writer
pub struct CsvWriter {
    writer: BufWriter<File>,
    signals: Vec<String>,
    delimiter: char,
    write_headers: bool,
}

impl CsvWriter {
    pub fn new(path: &str, delimiter: char, write_headers: bool) -> SimulationResult<Self> {
        let file = File::create(path)
            .map_err(|e| SimulationError::IoError(format!("Failed to create CSV file: {}", e)))?;
        
        Ok(Self {
            writer: BufWriter::new(file),
            signals: Vec::new(),
            delimiter,
            write_headers,
        })
    }

    fn format_value(&self, value: &PinValue) -> String {
        if value.is_digital() {
            match value.logic_level {
                Some(LogicLevel::Low) => "0".to_string(),
                Some(LogicLevel::High) => "1".to_string(),
                Some(LogicLevel::Unknown) => "X".to_string(),
                Some(LogicLevel::HighZ) => "Z".to_string(),
                None => "U".to_string(),
            }
        } else if value.is_analog() {
            format!("{:.6}", value.voltage)
        } else {
            "U".to_string()
        }
    }
}

impl OutputFormat for CsvWriter {
    fn write_header(&mut self, signals: &[String]) -> SimulationResult<()> {
        self.signals = signals.to_vec();
        
        if self.write_headers {
            write_io!(self.writer, "Time")?;
            for signal in signals {
                write_io!(self.writer, "{}{}", self.delimiter, signal)?;
            }
            writeln_io!(self.writer)?;
        }
        
        Ok(())
    }

    fn write_time_step(&mut self, time: f64, values: &HashMap<String, PinValue>) -> SimulationResult<()> {
        write_io!(self.writer, "{:.9}", time)?;
        
        for signal in &self.signals {
            write_io!(self.writer, "{}", self.delimiter)?;
            if let Some(value) = values.get(signal) {
                write_io!(self.writer, "{}", self.format_value(value))?;
            } else {
                write_io!(self.writer, "U")?;
            }
        }
        writeln_io!(self.writer)?;
        
        Ok(())
    }

    fn write_traces(&mut self, capture: &WaveformCapture) -> SimulationResult<()> {
        // Get all signals and sort them
        let mut signals: Vec<String> = capture.get_all_signals().keys().cloned().collect();
        signals.sort();
        
        self.write_header(&signals)?;
        
        // Collect all unique time points - convert to integer nanoseconds for ordering
        let mut all_times = std::collections::BTreeSet::new();
        for trace in capture.get_all_signals().values() {
            for point in &trace.points {
                // Convert to integer nanoseconds to avoid f64 ordering issues
                all_times.insert((point.time * 1e9) as i64);
            }
        }
        
        // Write data for each time point
        for time_ns in all_times {
            let time = time_ns as f64 / 1e9; // Convert back to seconds
            let mut values = HashMap::new();
            for signal in &signals {
                if let Some(trace) = capture.get_signal(signal) {
                    if let Some(value) = trace.get_value_at(time) {
                        values.insert(signal.clone(), value.clone());
                    }
                }
            }
            self.write_time_step(time, &values)?;
        }
        
        Ok(())
    }

    fn finish(mut self) -> SimulationResult<()> {
        self.writer.flush()
            .map_err(|e| SimulationError::IoError(format!("Failed to flush CSV file: {}", e)))?;
        Ok(())
    }
}

/// JSON data structures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSimulationData {
    pub metadata: JsonMetadata,
    pub signals: Vec<JsonSignal>,
    pub time_points: Vec<JsonTimePoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonMetadata {
    pub version: String,
    pub timestamp: String,
    pub time_unit: String,
    pub simulator: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSignal {
    pub name: String,
    pub signal_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonTimePoint {
    pub time: f64,
    pub values: HashMap<String, JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonValue {
    Digital(String),
    Analog(f64),
    Bus(Vec<String>),
}

/// JSON output writer
pub struct JsonWriter {
    path: String,
    data: JsonSimulationData,
    pretty: bool,
}

impl JsonWriter {
    pub fn new(path: &str, pretty: bool) -> SimulationResult<Self> {
        Ok(Self {
            path: path.to_string(),
            data: JsonSimulationData {
                metadata: JsonMetadata {
                    version: "1.0".to_string(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    time_unit: "s".to_string(),
                    simulator: "BHDL Simulator".to_string(),
                    comment: None,
                },
                signals: Vec::new(),
                time_points: Vec::new(),
            },
            pretty,
        })
    }

    pub fn set_metadata(&mut self, metadata: JsonMetadata) {
        self.data.metadata = metadata;
    }

    fn format_value(&self, value: &PinValue) -> JsonValue {
        if value.is_digital() {
            let s = match value.logic_level {
                Some(LogicLevel::Low) => "0",
                Some(LogicLevel::High) => "1",
                Some(LogicLevel::Unknown) => "X",
                Some(LogicLevel::HighZ) => "Z",
                None => "U",
            };
            JsonValue::Digital(s.to_string())
        } else if value.is_analog() {
            JsonValue::Analog(value.voltage)
        } else {
            JsonValue::Digital("U".to_string())
        }
    }
}

impl OutputFormat for JsonWriter {
    fn write_header(&mut self, signals: &[String]) -> SimulationResult<()> {
        self.data.signals.clear();
        
        for signal in signals {
            self.data.signals.push(JsonSignal {
                name: signal.clone(),
                signal_type: "wire".to_string(),
                width: None,
                metadata: HashMap::new(),
            });
        }
        
        Ok(())
    }

    fn write_time_step(&mut self, time: f64, values: &HashMap<String, PinValue>) -> SimulationResult<()> {
        let mut json_values = HashMap::new();
        
        for (signal, value) in values {
            json_values.insert(signal.clone(), self.format_value(value));
        }
        
        self.data.time_points.push(JsonTimePoint {
            time,
            values: json_values,
        });
        
        Ok(())
    }

    fn write_traces(&mut self, capture: &WaveformCapture) -> SimulationResult<()> {
        // Clear existing data
        self.data.signals.clear();
        self.data.time_points.clear();
        
        // Add all signals with metadata
        for (name, trace) in capture.get_all_signals() {
            let signal_type = trace.metadata.get("type")
                .cloned()
                .unwrap_or_else(|| "wire".to_string());
            
            let width = trace.metadata.get("width")
                .and_then(|w| w.parse::<u32>().ok());
            
            self.data.signals.push(JsonSignal {
                name: name.clone(),
                signal_type,
                width,
                metadata: trace.metadata.clone(),
            });
        }
        
        // Sort signals by name
        self.data.signals.sort_by(|a, b| a.name.cmp(&b.name));
        
        // Collect all unique time points - convert to integer nanoseconds for ordering
        let mut all_times = std::collections::BTreeSet::new();
        for trace in capture.get_all_signals().values() {
            for point in &trace.points {
                // Convert to integer nanoseconds to avoid f64 ordering issues
                all_times.insert((point.time * 1e9) as i64);
            }
        }
        
        // Write data for each time point
        for time_ns in all_times {
            let time = time_ns as f64 / 1e9; // Convert back to seconds
            let mut values = HashMap::new();
            
            for signal in &self.data.signals {
                if let Some(trace) = capture.get_signal(&signal.name) {
                    if let Some(value) = trace.get_value_at(time) {
                        values.insert(signal.name.clone(), self.format_value(value));
                    }
                }
            }
            
            if !values.is_empty() {
                self.data.time_points.push(JsonTimePoint {
                    time,
                    values,
                });
            }
        }
        
        Ok(())
    }

    fn finish(self) -> SimulationResult<()> {
        let file = File::create(&self.path)
            .map_err(|e| SimulationError::IoError(format!("Failed to create JSON file: {}", e)))?;
        
        if self.pretty {
            serde_json::to_writer_pretty(file, &self.data)
        } else {
            serde_json::to_writer(file, &self.data)
        }
        .map_err(|e| SimulationError::IoError(format!("Failed to write JSON: {}", e)))?;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_csv_writer() {
        let dir = tempdir().unwrap();
        let csv_path = dir.path().join("test.csv");
        
        let mut writer = CsvWriter::new(csv_path.to_str().unwrap(), ',', true).unwrap();
        
        // Write header
        writer.write_header(&["clk".to_string(), "data".to_string()]).unwrap();
        
        // Write some time steps
        let mut values = HashMap::new();
        values.insert("clk".to_string(), PinValue::digital(LogicLevel::Low));
        values.insert("data".to_string(), PinValue::digital(LogicLevel::Unknown));
        writer.write_time_step(0.0, &values).unwrap();
        
        values.insert("clk".to_string(), PinValue::digital(LogicLevel::High));
        values.insert("data".to_string(), PinValue::digital(LogicLevel::Low));
        writer.write_time_step(1e-9, &values).unwrap();
        
        writer.finish().unwrap();
        
        // Verify content
        let content = std::fs::read_to_string(&csv_path).unwrap();
        assert!(content.contains("Time,clk,data"));
        assert!(content.contains("0.000000000,0,X"));
        assert!(content.contains("0.000000001,1,0"));
    }

    #[test]
    fn test_json_writer() {
        let dir = tempdir().unwrap();
        let json_path = dir.path().join("test.json");
        
        let mut writer = JsonWriter::new(json_path.to_str().unwrap(), true).unwrap();
        
        // Set custom metadata
        writer.data.metadata.comment = Some("Test simulation".to_string());
        
        // Write header
        writer.write_header(&["clk".to_string(), "voltage".to_string()]).unwrap();
        
        // Write time steps
        let mut values = HashMap::new();
        values.insert("clk".to_string(), PinValue::digital(LogicLevel::Low));
        values.insert("voltage".to_string(), PinValue::analog(0.0));
        writer.write_time_step(0.0, &values).unwrap();
        
        values.insert("clk".to_string(), PinValue::digital(LogicLevel::High));
        values.insert("voltage".to_string(), PinValue::analog(3.3));
        writer.write_time_step(1e-9, &values).unwrap();
        
        writer.finish().unwrap();
        
        // Verify content
        let content = std::fs::read_to_string(&json_path).unwrap();
        let parsed: JsonSimulationData = serde_json::from_str(&content).unwrap();
        
        assert_eq!(parsed.metadata.comment, Some("Test simulation".to_string()));
        assert_eq!(parsed.signals.len(), 2);
        assert_eq!(parsed.time_points.len(), 2);
        assert_eq!(parsed.time_points[0].time, 0.0);
        assert_eq!(parsed.time_points[1].time, 1e-9);
    }

    #[test]
    fn test_waveform_to_formats() {
        let dir = tempdir().unwrap();
        
        // Create waveform capture
        let mut capture = WaveformCapture::new(1000);
        capture.register_signal("clk", HashMap::from([("type".to_string(), "clock".to_string())]));
        capture.register_signal("data", HashMap::from([
            ("type".to_string(), "bus".to_string()),
            ("width".to_string(), "8".to_string()),
        ]));
        
        // Add some data
        capture.capture_value("clk", 0.0, PinValue::digital(LogicLevel::Low)).unwrap();
        capture.capture_value("clk", 5e-9, PinValue::digital(LogicLevel::High)).unwrap();
        capture.capture_value("data", 0.0, PinValue::analog(0.0)).unwrap();
        capture.capture_value("data", 10e-9, PinValue::analog(1.8)).unwrap();
        
        // Write to CSV
        let csv_path = dir.path().join("waveform.csv");
        let mut csv_writer = CsvWriter::new(csv_path.to_str().unwrap(), ',', true).unwrap();
        csv_writer.write_traces(&capture).unwrap();
        csv_writer.finish().unwrap();
        
        // Write to JSON
        let json_path = dir.path().join("waveform.json");
        let mut json_writer = JsonWriter::new(json_path.to_str().unwrap(), true).unwrap();
        json_writer.write_traces(&capture).unwrap();
        json_writer.finish().unwrap();
        
        // Verify files exist and have content
        assert!(csv_path.exists());
        assert!(json_path.exists());
        
        let csv_content = std::fs::read_to_string(&csv_path).unwrap();
        assert!(csv_content.contains("clk"));
        assert!(csv_content.contains("data"));
        
        let json_content = std::fs::read_to_string(&json_path).unwrap();
        let json_data: JsonSimulationData = serde_json::from_str(&json_content).unwrap();
        assert_eq!(json_data.signals.len(), 2);
        assert!(json_data.signals.iter().any(|s| s.name == "data" && s.width == Some(8)));
    }
}