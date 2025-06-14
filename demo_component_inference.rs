//! Component Inference Engine Demo for BHDL Phase 2
//! 
//! This demonstrates intelligent component parameter calculation
//! based on circuit requirements and electrical constraints.

use std::collections::HashMap;

// Copy the component inference types for demonstration
#[derive(Debug, Clone, PartialEq)]
pub struct InferredParameter {
    pub name: String,
    pub value: ParameterValue,
    pub confidence: f64,
    pub reasoning: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParameterValue {
    Resistance(f64),
    Capacitance(f64),
    Voltage(f64),
    Current(f64),
    String(String),
}

impl std::fmt::Display for ParameterValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParameterValue::Resistance(r) => write!(f, "{}Ω", format_electrical_value(*r)),
            ParameterValue::Capacitance(c) => write!(f, "{}F", format_electrical_value(*c)),
            ParameterValue::Voltage(v) => write!(f, "{}V", v),
            ParameterValue::Current(i) => write!(f, "{}A", format_electrical_value(*i)),
            ParameterValue::String(s) => write!(f, "\"{}\"", s),
        }
    }
}

fn format_electrical_value(value: f64) -> String {
    if value >= 1e6 {
        format!("{:.1}M", value / 1e6)
    } else if value >= 1e3 {
        format!("{:.1}k", value / 1e3)
    } else if value >= 1.0 {
        format!("{:.1}", value)
    } else if value >= 1e-3 {
        format!("{:.1}m", value * 1e3)
    } else if value >= 1e-6 {
        format!("{:.1}μ", value * 1e6)
    } else if value >= 1e-9 {
        format!("{:.1}n", value * 1e9)
    } else {
        format!("{:.1}p", value * 1e12)
    }
}

#[derive(Debug, Clone)]
pub struct ComponentSuggestion {
    pub component_type: String,
    pub parameters: Vec<InferredParameter>,
    pub reasoning: String,
    pub confidence: f64,
    pub alternatives: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CircuitRequirements {
    pub supply_voltage: Option<f64>,
    pub required_current: Option<f64>,
    pub frequency: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct CircuitContext {
    pub has_led_in_series: bool,
    pub led_color: Option<String>,
    pub is_pullup: bool,
    pub is_decoupling: bool,
    pub high_frequency: bool,
    pub is_status_indicator: bool,
}

pub struct ComponentInferenceEngine {
    pub inferred_components: Vec<ComponentSuggestion>,
}

impl ComponentInferenceEngine {
    pub fn new() -> Self {
        Self {
            inferred_components: Vec::new(),
        }
    }

    pub fn infer_resistor_for_led(
        &mut self,
        supply_voltage: f64,
        led_color: &str,
    ) -> ComponentSuggestion {
        let vf = match led_color {
            "red" => 2.0,
            "green" => 2.2,
            "blue" => 3.2,
            "yellow" => 2.1,
            "white" => 3.3,
            _ => 2.0,
        };
        
        let if_target = 0.02; // 20mA typical
        let resistance = (supply_voltage - vf) / if_target;
        let resistance_standard = find_nearest_e_series_value(resistance);
        
        let parameter = InferredParameter {
            name: "value".to_string(),
            value: ParameterValue::Resistance(resistance_standard),
            confidence: 0.95,
            reasoning: format!(
                "LED current limiting: R = ({:.1}V - {:.1}V) / {:.3}A = {:.0}Ω",
                supply_voltage, vf, if_target, resistance
            ),
        };
        
        ComponentSuggestion {
            component_type: "Res".to_string(),
            parameters: vec![parameter],
            reasoning: format!("Current limiting resistor for {} LED", led_color),
            confidence: 0.95,
            alternatives: vec![
                "Consider 1% tolerance for precise current".to_string(),
                "Use 0.5W rating for safety margin".to_string(),
            ],
        }
    }
    
    pub fn infer_pullup_resistor(&mut self, supply_voltage: f64, high_speed: bool) -> ComponentSuggestion {
        let resistance = if high_speed {
            1000.0 // 1kΩ for high speed
        } else {
            10000.0 // 10kΩ for normal operation
        };
        
        let resistance_standard = find_nearest_e_series_value(resistance);
        
        let parameter = InferredParameter {
            name: "value".to_string(),
            value: ParameterValue::Resistance(resistance_standard),
            confidence: 0.85,
            reasoning: format!(
                "Pull-up resistor for {}V logic, {} speed",
                supply_voltage,
                if high_speed { "high" } else { "normal" }
            ),
        };
        
        ComponentSuggestion {
            component_type: "Res".to_string(),
            parameters: vec![parameter],
            reasoning: "Digital pull-up resistor".to_string(),
            confidence: 0.85,
            alternatives: vec![
                "Lower value for faster switching".to_string(),
                "Higher value for lower power consumption".to_string(),
            ],
        }
    }
    
    pub fn infer_decoupling_capacitor(&mut self, high_frequency: bool) -> ComponentSuggestion {
        let capacitance = if high_frequency {
            100e-9 // 100nF for high frequency
        } else {
            10e-6 // 10µF for bulk decoupling
        };
        
        let parameter = InferredParameter {
            name: "value".to_string(),
            value: ParameterValue::Capacitance(capacitance),
            confidence: 0.9,
            reasoning: format!(
                "{} decoupling capacitor",
                if high_frequency { "High frequency" } else { "Bulk" }
            ),
        };
        
        ComponentSuggestion {
            component_type: "Cap".to_string(),
            parameters: vec![parameter],
            reasoning: "Power supply decoupling".to_string(),
            confidence: 0.9,
            alternatives: vec![
                "Use ceramic (X7R) for decoupling".to_string(),
                "Place close to power pins".to_string(),
            ],
        }
    }
    
    pub fn infer_led_color(&mut self, context: &CircuitContext) -> ComponentSuggestion {
        let color = if context.is_status_indicator {
            "green"
        } else {
            "red"
        };
        
        let parameter = InferredParameter {
            name: "color".to_string(),
            value: ParameterValue::String(color.to_string()),
            confidence: 0.8,
            reasoning: format!(
                "{} for {} indication",
                color,
                if context.is_status_indicator { "status" } else { "general" }
            ),
        };
        
        ComponentSuggestion {
            component_type: "LED".to_string(),
            parameters: vec![parameter],
            reasoning: "LED color based on application".to_string(),
            confidence: 0.8,
            alternatives: vec![
                "Consider RGB LED for multiple states".to_string(),
                "Use high-brightness for visibility".to_string(),
            ],
        }
    }
    
    pub fn add_suggestion(&mut self, suggestion: ComponentSuggestion) {
        self.inferred_components.push(suggestion);
    }
    
    pub fn generate_code(&self) -> String {
        let mut code = String::new();
        
        if !self.inferred_components.is_empty() {
            code.push_str("// Auto-inferred component parameters\n");
            
            for suggestion in &self.inferred_components {
                code.push_str(&format!("// {}\n", suggestion.reasoning));
                code.push_str(&format!("// Confidence: {:.0}%\n", suggestion.confidence * 100.0));
                
                let params: Vec<String> = suggestion.parameters.iter()
                    .map(|p| format!("{} = {}", p.name, p.value))
                    .collect();
                
                code.push_str(&format!("{}({})\n", suggestion.component_type, params.join(", ")));
                code.push('\n');
            }
        }
        
        code
    }
}

fn find_nearest_e_series_value(target: f64) -> f64 {
    let e12_base = vec![1.0, 1.2, 1.5, 1.8, 2.2, 2.7, 3.3, 3.9, 4.7, 5.6, 6.8, 8.2];
    let mut values = Vec::new();
    
    // Generate E12 values from 1Ω to 10MΩ
    for decade in 0..8 {
        let multiplier = 10_f64.powi(decade);
        for &base in &e12_base {
            values.push(base * multiplier);
        }
    }
    
    values.into_iter()
        .min_by(|a, b| (a - target).abs().partial_cmp(&(b - target).abs()).unwrap())
        .unwrap_or(target)
}

fn main() {
    println!("🧠 BHDL Phase 2: Component Inference Engine Demo");
    println!("===============================================");

    let mut engine = ComponentInferenceEngine::new();

    println!("\n📍 1. LED Current Limiting Resistor Inference");
    println!("   Circuit: VCC(5V) -> Res(??) -> LED(red) -> GND");
    
    let led_resistor = engine.infer_resistor_for_led(5.0, "red");
    engine.add_suggestion(led_resistor);
    
    if let ParameterValue::Resistance(r) = &engine.inferred_components[0].parameters[0].value {
        println!("   Inferred: R = {} (for 20mA LED current)", format_electrical_value(*r));
        println!("   Reasoning: {}", engine.inferred_components[0].parameters[0].reasoning);
    }

    println!("\n📍 2. Pull-up Resistor Inference");
    println!("   Circuit: VCC(3.3V) -> Res(??) -> MCU.INPUT_PIN");
    
    let pullup_resistor = engine.infer_pullup_resistor(3.3, false);
    engine.add_suggestion(pullup_resistor);
    
    if let ParameterValue::Resistance(r) = &engine.inferred_components[1].parameters[0].value {
        println!("   Inferred: R = {} (for digital pull-up)", format_electrical_value(*r));
    }

    println!("\n📍 3. High-Speed Pull-up Resistor");
    println!("   Circuit: VCC(3.3V) -> Res(??) -> MCU.I2C_SCL (400kHz)");
    
    let fast_pullup = engine.infer_pullup_resistor(3.3, true);
    engine.add_suggestion(fast_pullup);
    
    if let ParameterValue::Resistance(r) = &engine.inferred_components[2].parameters[0].value {
        println!("   Inferred: R = {} (for high-speed operation)", format_electrical_value(*r));
    }

    println!("\n📍 4. Decoupling Capacitor Inference");
    println!("   Circuit: VCC -> Cap(??) -> GND (near MCU power pin)");
    
    let decoupling_cap = engine.infer_decoupling_capacitor(true);
    engine.add_suggestion(decoupling_cap);
    
    if let ParameterValue::Capacitance(c) = &engine.inferred_components[3].parameters[0].value {
        println!("   Inferred: C = {} (high-frequency decoupling)", format_electrical_value(*c));
    }

    println!("\n📍 5. Bulk Decoupling Capacitor");
    println!("   Circuit: VCC -> Cap(??) -> GND (bulk supply filtering)");
    
    let bulk_cap = engine.infer_decoupling_capacitor(false);
    engine.add_suggestion(bulk_cap);
    
    if let ParameterValue::Capacitance(c) = &engine.inferred_components[4].parameters[0].value {
        println!("   Inferred: C = {} (bulk filtering)", format_electrical_value(*c));
    }

    println!("\n📍 6. LED Color Inference");
    println!("   Circuit: Status indicator LED");
    
    let context = CircuitContext {
        is_status_indicator: true,
        ..Default::default()
    };
    let led_color = engine.infer_led_color(&context);
    engine.add_suggestion(led_color);
    
    if let ParameterValue::String(color) = &engine.inferred_components[5].parameters[0].value {
        println!("   Inferred: color = {} (for status indication)", color);
    }

    println!("\n📍 7. Complete Circuit with Inferred Components");
    println!("   Original BHDL code:");
    println!("   ```bhdl");
    println!("   VCC -> Res().1 -> LED(red).A -> GND;");
    println!("   VCC -> Res().1 -> mcu.RESET;");
    println!("   VCC -> Cap().1 -> GND;");
    println!("   ```");
    
    println!("\n   After component inference:");
    println!("   ```bhdl");
    println!("   VCC -> Res(value = 150Ω).1 -> LED(red).A -> GND;");
    println!("   VCC -> Res(value = 10kΩ).1 -> mcu.RESET;");
    println!("   VCC -> Cap(value = 100nF).1 -> GND;");
    println!("   ```");

    println!("\n📍 8. Generated BHDL Code");
    let generated_code = engine.generate_code();
    print!("{}", generated_code);

    println!("📍 9. Design Insights");
    for (i, suggestion) in engine.inferred_components.iter().enumerate() {
        println!("   Component {}: {} (Confidence: {:.0}%)", 
                 i + 1, suggestion.reasoning, suggestion.confidence * 100.0);
        for alt in &suggestion.alternatives {
            println!("     • {}", alt);
        }
    }

    println!("\n✅ Component Inference Engine Demo Complete!");
    println!("\n🎯 Key Features Demonstrated:");
    println!("   • Automatic resistor calculation using Ohm's law");
    println!("   • LED current limiting resistor sizing");
    println!("   • Pull-up resistor value selection");
    println!("   • Decoupling capacitor sizing");
    println!("   • E-series standard value selection");
    println!("   • Context-aware component parameter inference");
    println!("   • Confidence scoring and alternatives");
    
    println!("\n🚀 Ready for integration with BHDL circuit flow!");
}