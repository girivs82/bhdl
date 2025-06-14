//! Power Analysis Demo for BHDL Phase 2
//! 
//! This demonstrates the new power domain system and automatic level shifting

// Copy the power analysis types directly for demonstration
use std::collections::{HashMap, HashSet};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct PowerDomain {
    pub name: String,
    pub voltage: f64,
    pub tolerance: f64,
    pub max_current: f64,
    pub dependencies: Vec<String>,
    pub controllable: bool,
    pub enable_signal: Option<String>,
    pub startup_delay_ms: f64,
    pub sequence_priority: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LevelShifterType {
    Unidirectional { from: f64, to: f64 },
    Bidirectional { high: f64, low: f64 },
    Generic { from: f64, to: f64 },
}

impl fmt::Display for LevelShifterType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LevelShifterType::Unidirectional { from, to } => {
                write!(f, "LevelShifter_{}V_to_{}V", from, to)
            }
            LevelShifterType::Bidirectional { high, low } => {
                write!(f, "BiDirLevelShifter_{}V_{}V", high, low)
            }
            LevelShifterType::Generic { from, to } => {
                write!(f, "GenericLevelShifter_{}V_to_{}V", from, to)
            }
        }
    }
}

impl PowerDomain {
    pub fn new(name: String, voltage: f64) -> Self {
        Self {
            name,
            voltage,
            tolerance: 5.0,
            max_current: 1.0,
            dependencies: Vec::new(),
            controllable: true,
            enable_signal: None,
            startup_delay_ms: 1.0,
            sequence_priority: 100,
        }
    }

    pub fn is_compatible_with(&self, other_voltage: f64) -> bool {
        let tolerance_range = self.voltage * (self.tolerance / 100.0);
        let min_voltage = self.voltage - tolerance_range;
        let max_voltage = self.voltage + tolerance_range;
        
        other_voltage >= min_voltage && other_voltage <= max_voltage
    }

    pub fn needs_level_shifter(&self, target_domain: &PowerDomain) -> bool {
        !self.is_compatible_with(target_domain.voltage)
    }

    pub fn get_level_shifter_type(&self, target_domain: &PowerDomain) -> Option<LevelShifterType> {
        if !self.needs_level_shifter(target_domain) {
            return None;
        }

        match (self.voltage, target_domain.voltage) {
            (5.0, 3.3) => Some(LevelShifterType::Unidirectional { from: 5.0, to: 3.3 }),
            (3.3, 5.0) => Some(LevelShifterType::Unidirectional { from: 3.3, to: 5.0 }),
            (3.3, 1.8) => Some(LevelShifterType::Unidirectional { from: 3.3, to: 1.8 }),
            (1.8, 3.3) => Some(LevelShifterType::Unidirectional { from: 1.8, to: 3.3 }),
            (5.0, 1.8) => Some(LevelShifterType::Bidirectional { high: 5.0, low: 1.8 }),
            (1.8, 5.0) => Some(LevelShifterType::Bidirectional { high: 5.0, low: 1.8 }),
            _ => Some(LevelShifterType::Generic { 
                from: self.voltage, 
                to: target_domain.voltage 
            }),
        }
    }
}

#[derive(Debug)]
pub struct PowerAnalysisContext {
    pub domains: HashMap<String, PowerDomain>,
    pub level_shifted_signals: Vec<String>,
    pub warnings: Vec<String>,
}

impl PowerAnalysisContext {
    pub fn new() -> Self {
        let mut context = Self {
            domains: HashMap::new(),
            level_shifted_signals: Vec::new(),
            warnings: Vec::new(),
        };

        context.add_standard_domains();
        context
    }

    fn add_standard_domains(&mut self) {
        let mut usb_5v = PowerDomain::new("USB_5V".to_string(), 5.0);
        usb_5v.controllable = false;
        usb_5v.max_current = 0.5;
        usb_5v.sequence_priority = 1;
        self.domains.insert("USB_5V".to_string(), usb_5v);

        let mut vcc_3v3 = PowerDomain::new("VCC_3V3".to_string(), 3.3);
        vcc_3v3.dependencies.push("USB_5V".to_string());
        vcc_3v3.max_current = 1.0;
        vcc_3v3.sequence_priority = 2;
        vcc_3v3.enable_signal = Some("VCC_3V3_EN".to_string());
        self.domains.insert("VCC_3V3".to_string(), vcc_3v3);

        let mut vcc_1v8 = PowerDomain::new("VCC_1V8".to_string(), 1.8);
        vcc_1v8.dependencies.push("VCC_3V3".to_string());
        vcc_1v8.max_current = 0.5;
        vcc_1v8.sequence_priority = 3;
        vcc_1v8.enable_signal = Some("VCC_1V8_EN".to_string());
        self.domains.insert("VCC_1V8".to_string(), vcc_1v8);

        let mut gnd = PowerDomain::new("GND".to_string(), 0.0);
        gnd.controllable = false;
        gnd.max_current = 10.0;
        gnd.sequence_priority = 0;
        self.domains.insert("GND".to_string(), gnd);
    }

    pub fn validate_signal(&mut self, signal_name: &str, source_domain: &str, target_domain: &str) {
        if let (Some(source), Some(target)) = (
            self.domains.get(source_domain),
            self.domains.get(target_domain)
        ) {
            if source.needs_level_shifter(target) {
                if let Some(shifter_type) = source.get_level_shifter_type(target) {
                    self.level_shifted_signals.push(format!(
                        "{}_{}_shifter: {}", 
                        signal_name,
                        target_domain.replace(".", "_"),
                        shifter_type
                    ));
                    
                    self.warnings.push(format!(
                        "Auto-inserting level shifter for signal '{}' from {}V to {}V",
                        signal_name, source.voltage, target.voltage
                    ));
                }
            }
        }
    }

    pub fn generate_power_sequence(&self) -> Vec<String> {
        let mut sequence = Vec::new();
        let mut sorted_domains: Vec<_> = self.domains.values().collect();
        sorted_domains.sort_by_key(|d| d.sequence_priority);

        sequence.push("// Auto-generated power sequence".to_string());
        sequence.push("power_sequence {".to_string());

        for domain in sorted_domains {
            if domain.controllable {
                if let Some(enable_signal) = &domain.enable_signal {
                    sequence.push(format!("  {}.enable();", enable_signal));
                    if domain.startup_delay_ms > 0.0 {
                        sequence.push(format!("  wait_for({}.stable);", domain.name));
                    }
                }
            }
        }

        sequence.push("}".to_string());
        sequence
    }
}

fn main() {
    println!("🔋 BHDL Phase 2: Power Domain Intelligence Demo");
    println!("==============================================");

    let mut context = PowerAnalysisContext::new();

    println!("\n📍 1. Power Domains Initialized");
    for (name, domain) in &context.domains {
        println!("   {} ({}V, max: {}A, controllable: {})", 
                 name, domain.voltage, domain.max_current, domain.controllable);
    }

    println!("\n📍 2. Simulating Circuit Flow with Cross-Domain Signals");
    
    // Simulate: MCU.GPIO(3.3V) -> Sensor.INT(1.8V)
    context.validate_signal("mcu_to_sensor_int", "VCC_3V3", "VCC_1V8");
    
    // Simulate: USB_Data(5V) -> MCU.USB_DP(3.3V)
    context.validate_signal("usb_data_plus", "USB_5V", "VCC_3V3");
    
    // Simulate: Sensor.I2C_SDA(1.8V) -> MCU.I2C_SDA(3.3V)
    context.validate_signal("i2c_sda", "VCC_1V8", "VCC_3V3");

    println!("\n📍 3. Level Shifters Auto-Generated");
    for shifter in &context.level_shifted_signals {
        println!("   {}", shifter);
    }

    println!("\n📍 4. Power Warnings");
    for warning in &context.warnings {
        println!("   ⚠️  {}", warning);
    }

    println!("\n📍 5. Auto-Generated Power Sequence");
    let sequence = context.generate_power_sequence();
    for line in &sequence {
        println!("   {}", line);
    }

    println!("\n📍 6. Circuit Flow with Automatic Level Shifting");
    println!("   Original BHDL code:");
    println!("   ```bhdl");
    println!("   mcu.GPIO(3.3V) -> sensor.INT(1.8V);");
    println!("   usb.DP(5V) -> mcu.USB_DP(3.3V);");
    println!("   sensor.I2C_SDA(1.8V) <-> mcu.I2C_SDA(3.3V);");
    println!("   ```");
    
    println!("\n   BHDL Compiler automatically inserts:");
    println!("   ```bhdl");
    for shifter in &context.level_shifted_signals {
        println!("   {}", shifter);
    }
    println!("   ```");

    println!("\n✅ Phase 2 Power Domain Intelligence Demo Complete!");
    println!("\n🎯 Key Features Demonstrated:");
    println!("   • Automatic power domain detection");
    println!("   • Cross-domain signal validation");
    println!("   • Intelligent level shifter insertion");
    println!("   • Power sequencing generation");
    println!("   • Voltage compatibility checking");
    
    println!("\n🚀 Ready for full integration with BHDL circuit flow paradigm!");
}