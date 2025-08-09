//! Waveform capture and output

use std::collections::HashMap;
use std::fs::File;
use std::io::{Write, BufWriter};
use std::path::Path;
use indexmap::IndexMap;
use vcd;

use crate::{SignalRef, Result, TestbenchError};
use crate::testbench::{Scope, CaptureMode, TriggerCondition, TriggerType};

/// Waveform capture system
pub struct WaveformCapture {
    scopes: Vec<ActiveScope>,
    writers: Vec<Box<dyn WaveformWriter>>,
    signal_map: HashMap<SignalRef, SignalId>,
    next_signal_id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SignalId(u32);

struct ActiveScope {
    config: Scope,
    buffers: HashMap<SignalRef, SignalBuffer>,
    trigger_state: TriggerState,
    is_capturing: bool,
}

#[derive(Debug)]
enum TriggerState {
    Waiting,
    PreTrigger { samples: Vec<(f64, HashMap<SignalRef, f64>)> },
    Triggered { end_time: f64 },
    Complete,
}

struct SignalBuffer {
    timestamps: Vec<f64>,
    values: Vec<f64>,
    last_value: Option<f64>,
    last_timestamp: Option<f64>,
}

impl WaveformCapture {
    pub fn new(scopes: &[Scope]) -> Result<Self> {
        let mut capture = Self {
            scopes: Vec::new(),
            writers: Vec::new(),
            signal_map: HashMap::new(),
            next_signal_id: 1,
        };
        
        // Create active scopes
        for scope in scopes {
            let mut buffers = HashMap::new();
            for signal in &scope.signals {
                buffers.insert(signal.clone(), SignalBuffer::new());
                
                // Assign signal IDs
                if !capture.signal_map.contains_key(signal) {
                    capture.signal_map.insert(signal.clone(), SignalId(capture.next_signal_id));
                    capture.next_signal_id += 1;
                }
            }
            
            let active = ActiveScope {
                config: scope.clone(),
                buffers,
                trigger_state: TriggerState::Waiting,
                is_capturing: matches!(scope.capture_mode, CaptureMode::Continuous),
            };
            
            capture.scopes.push(active);
        }
        
        Ok(capture)
    }
    
    pub fn add_writer(&mut self, format: WaveformFormat, output_path: &Path) -> Result<()> {
        let writer: Box<dyn WaveformWriter> = match format {
            WaveformFormat::VCD => Box::new(VcdWriter::new(output_path)?),
            WaveformFormat::CSV => Box::new(CsvWriter::new(output_path)?),
            WaveformFormat::JSON => Box::new(JsonWriter::new(output_path)?),
        };
        
        self.writers.push(writer);
        Ok(())
    }
    
    pub fn capture(&mut self, time: f64, values: &HashMap<SignalRef, f64>) -> Result<()> {
        println!("=== WaveformCapture::capture at time {:.6} ===", time);
        println!("Values provided: {} entries", values.len());
        for (signal, value) in values {
            println!("  {:?} = {:.6}", signal, value);
        }
        
        for scope in &mut self.scopes {
            scope.capture(time, values)?;
        }
        
        // Write to all output formats
        for writer in &mut self.writers {
            writer.write_timepoint(time, values)?;
        }
        
        Ok(())
    }
    
    pub fn finalize(&mut self) -> Result<()> {
        for writer in &mut self.writers {
            writer.finalize()?;
        }
        Ok(())
    }
}

impl ActiveScope {
    fn capture(&mut self, time: f64, values: &HashMap<SignalRef, f64>) -> Result<()> {
        // Check trigger conditions
        self.update_trigger_state(time, values)?;
        
        // Capture based on mode and state
        if self.should_capture(time, values) {
            for signal in &self.config.signals {
                if let Some(value) = values.get(signal) {
                    if let Some(buffer) = self.buffers.get_mut(signal) {
                        buffer.add_sample(time, *value, &self.config.capture_mode)?;
                    }
                }
            }
        }
        
        Ok(())
    }
    
    fn update_trigger_state(&mut self, time: f64, values: &HashMap<SignalRef, f64>) -> Result<()> {
        if let Some(trigger) = &self.config.trigger {
            match &mut self.trigger_state {
                TriggerState::Waiting => {
                    if self.check_trigger_condition(trigger, values) {
                        self.trigger_state = TriggerState::Triggered { 
                            end_time: time + 1.0 // TODO: Get from config
                        };
                        self.is_capturing = true;
                    }
                }
                TriggerState::Triggered { end_time } => {
                    if time >= *end_time {
                        self.trigger_state = TriggerState::Complete;
                        self.is_capturing = false;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    
    fn check_trigger_condition(&self, trigger: &TriggerCondition, values: &HashMap<SignalRef, f64>) -> bool {
        if let Some(value) = values.get(&trigger.signal) {
            match &trigger.condition {
                TriggerType::Above { value: threshold } => *value > *threshold,
                TriggerType::Below { value: threshold } => *value < *threshold,
                TriggerType::InRange { min, max } => *value >= *min && *value <= *max,
                _ => false, // Rising/Falling need previous value
            }
        } else {
            false
        }
    }
    
    fn should_capture(&self, _time: f64, _values: &HashMap<SignalRef, f64>) -> bool {
        self.is_capturing
    }
}

impl SignalBuffer {
    fn new() -> Self {
        Self {
            timestamps: Vec::new(),
            values: Vec::new(),
            last_value: None,
            last_timestamp: None,
        }
    }
    
    fn add_sample(&mut self, time: f64, value: f64, mode: &CaptureMode) -> Result<()> {
        match mode {
            CaptureMode::Continuous => {
                self.timestamps.push(time);
                self.values.push(value);
            }
            CaptureMode::OnChange { threshold } => {
                if let Some(last) = self.last_value {
                    if (value - last).abs() >= *threshold {
                        self.timestamps.push(time);
                        self.values.push(value);
                        self.last_value = Some(value);
                    }
                } else {
                    // First sample
                    self.timestamps.push(time);
                    self.values.push(value);
                    self.last_value = Some(value);
                }
            }
            _ => {
                // Other modes handled at scope level
                self.timestamps.push(time);
                self.values.push(value);
            }
        }
        
        self.last_timestamp = Some(time);
        Ok(())
    }
}

/// Waveform output format
#[derive(Debug, Clone, Copy)]
pub enum WaveformFormat {
    VCD,
    CSV,
    JSON,
}

/// Trait for waveform writers
pub trait WaveformWriter {
    fn write_timepoint(&mut self, time: f64, values: &HashMap<SignalRef, f64>) -> Result<()>;
    fn finalize(&mut self) -> Result<()>;
}

/// VCD (Value Change Dump) writer
struct VcdWriter {
    writer: BufWriter<File>,
    signal_vars: HashMap<SignalRef, String>,
    last_values: HashMap<SignalRef, f64>,
}

impl VcdWriter {
    fn new(path: &Path) -> Result<Self> {
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);
        
        // Write VCD header
        writeln!(writer, "$date")?;
        writeln!(writer, "   {}", chrono::Local::now())?;
        writeln!(writer, "$end")?;
        writeln!(writer, "$version")?;
        writeln!(writer, "   BHDL Testbench 0.1.0")?;
        writeln!(writer, "$end")?;
        writeln!(writer, "$timescale 1ns $end")?;
        
        Ok(Self {
            writer,
            signal_vars: HashMap::new(),
            last_values: HashMap::new(),
        })
    }
    
    fn write_signal_def(&mut self, signal: &SignalRef, var_code: &str) -> Result<()> {
        writeln!(self.writer, "$var wire 64 {} {} $end", var_code, signal.to_string())?;
        self.signal_vars.insert(signal.clone(), var_code.to_string());
        Ok(())
    }
}

impl WaveformWriter for VcdWriter {
    fn write_timepoint(&mut self, time: f64, values: &HashMap<SignalRef, f64>) -> Result<()> {
        // Convert time to nanoseconds
        let time_ns = (time * 1e9) as u64;
        writeln!(self.writer, "#{}", time_ns)?;
        
        // Write changed values
        for (signal, value) in values {
            if let Some(last) = self.last_values.get(signal) {
                if (*last - value).abs() < f64::EPSILON {
                    continue; // No change
                }
            }
            
            // Get or create variable code
            let var_code = if let Some(code) = self.signal_vars.get(signal) {
                code.clone()
            } else {
                // New signal, add definition
                let code = format!("v{}", self.signal_vars.len());
                self.write_signal_def(signal, &code)?;
                code
            };
            
            // Write value change
            writeln!(self.writer, "r{:.6} {}", value, var_code)?;
            self.last_values.insert(signal.clone(), *value);
        }
        
        Ok(())
    }
    
    fn finalize(&mut self) -> Result<()> {
        self.writer.flush()?;
        Ok(())
    }
}

/// CSV writer
struct CsvWriter {
    writer: BufWriter<File>,
    signals: IndexMap<SignalRef, usize>,
    first_write: bool,
}

impl CsvWriter {
    fn new(path: &Path) -> Result<Self> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        
        Ok(Self {
            writer,
            signals: IndexMap::new(),
            first_write: true,
        })
    }
}

impl WaveformWriter for CsvWriter {
    fn write_timepoint(&mut self, time: f64, values: &HashMap<SignalRef, f64>) -> Result<()> {
        // Update signal list
        for signal in values.keys() {
            if !self.signals.contains_key(signal) {
                self.signals.insert(signal.clone(), self.signals.len());
            }
        }
        
        // Write header on first write
        if self.first_write {
            write!(self.writer, "time")?;
            for signal in self.signals.keys() {
                write!(self.writer, ",{}", signal.to_string())?;
            }
            writeln!(self.writer)?;
            self.first_write = false;
        }
        
        // Write data row
        write!(self.writer, "{}", time)?;
        for signal in self.signals.keys() {
            if let Some(value) = values.get(signal) {
                write!(self.writer, ",{}", value)?;
            } else {
                write!(self.writer, ",")?; // Empty cell
            }
        }
        writeln!(self.writer)?;
        
        Ok(())
    }
    
    fn finalize(&mut self) -> Result<()> {
        self.writer.flush()?;
        Ok(())
    }
}

/// JSON writer
struct JsonWriter {
    path: std::path::PathBuf,
    data: Vec<serde_json::Value>,
}

impl JsonWriter {
    fn new(path: &Path) -> Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
            data: Vec::new(),
        })
    }
}

impl WaveformWriter for JsonWriter {
    fn write_timepoint(&mut self, time: f64, values: &HashMap<SignalRef, f64>) -> Result<()> {
        let mut point = serde_json::Map::new();
        point.insert("time".to_string(), serde_json::Value::from(time));
        
        for (signal, value) in values {
            point.insert(signal.to_string(), serde_json::Value::from(*value));
        }
        
        self.data.push(serde_json::Value::Object(point));
        Ok(())
    }
    
    fn finalize(&mut self) -> Result<()> {
        let file = File::create(&self.path)?;
        serde_json::to_writer_pretty(file, &self.data)?;
        Ok(())
    }
}