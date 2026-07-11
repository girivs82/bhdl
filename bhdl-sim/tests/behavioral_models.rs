//! Tests for behavioral modeling with expressions and when blocks

use bhdl_sim::evaluation::{
    SimulationAttributeEvaluator,
    WhenBlockProcessor,
    expression_parser::ExpressionParser,
};
use bhdl_sim::circuit::{CircuitState, CircuitTopology};
use bhdl_sim::engine::time::TimeManager;
use bhdl_analyzer::attribute_analysis::{AttributeAnalysisResult, WhenBlockInfo};
use bhdl_analyzer::expression_evaluator::RuntimeValue;
use std::collections::{HashMap, HashSet};

#[test]
fn test_voltage_controlled_oscillator() {
    println!("\n=== Voltage Controlled Oscillator (VCO) ===");
    
    // Create circuit state
    let topology = CircuitTopology {
        instance_modules: HashMap::new(),
        net_connections: HashMap::new(),
    };
    let mut circuit_state = CircuitState::new(topology);
    
    // Initialize VCO attributes
    circuit_state.update_attribute("control_voltage", RuntimeValue::Real(2.5));
    circuit_state.update_attribute("base_frequency", RuntimeValue::Real(1e6)); // 1 MHz
    circuit_state.update_attribute("frequency", RuntimeValue::Real(1e6));
    circuit_state.update_attribute("output", RuntimeValue::Real(0.0));
    
    // Expression texts for VCO behavior.
    let mut expression_texts = HashMap::new();
    expression_texts.insert(
        "frequency".to_string(),
        "base_frequency * (1.0 + 0.1 * control_voltage)".to_string()
    );
    expression_texts.insert(
        "output".to_string(),
        "sin(2 * pi * frequency * t)".to_string()
    );
    
    // Create evaluator
    let analysis = AttributeAnalysisResult {
        attributes: HashMap::new(),
        dependencies: HashMap::new(),
        evaluation_order: vec!["frequency".to_string(), "output".to_string()],
        circular_dependencies: vec![],
        mutable_attributes: HashSet::new(),
    };
    
    let mut evaluator = SimulationAttributeEvaluator::with_expressions(analysis, expression_texts);
    
    // Test at different times
    let mut time_manager = TimeManager::new(1e-9); // 1ns time step
    
    println!("VCO output over time:");
    for i in 0..10 {
        let time = i as f64 * 100e-9; // 100ns intervals
        time_manager.set_time(time).unwrap();
        
        // Evaluate expressions
        let attr_ids = vec![
            bhdl_sim::evaluation::scheduler::AttributeId("frequency".to_string()),
            bhdl_sim::evaluation::scheduler::AttributeId("output".to_string()),
        ];
        
        evaluator.evaluate_batch(&attr_ids, &mut circuit_state, &time_manager).unwrap();

        let freq = circuit_state.get_attribute("frequency").unwrap().clone();
        let output = circuit_state.get_attribute("output").unwrap().clone();

        // frequency = 1e6 * (1.0 + 0.1 * 2.5) = 1.25 MHz
        match freq {
            RuntimeValue::Real(f) => assert!((f - 1.25e6).abs() < 1e-3, "freq={}", f),
            other => panic!("expected Real frequency, got {:?}", other),
        }
        // output = sin(2*pi*frequency*t). evaluate_batch is two-pass (all
        // attributes read pre-batch state), so output uses the frequency
        // from the PREVIOUS batch: 1 MHz (initial) on the first iteration,
        // 1.25 MHz afterwards.
        let expected_freq = if i == 0 { 1e6 } else { 1.25e6 };
        let expected = (2.0 * std::f64::consts::PI * expected_freq * time).sin();
        match output {
            RuntimeValue::Real(v) => {
                assert!((v - expected).abs() < 1e-9, "t={} output={} expected={}", time, v, expected)
            }
            other => panic!("expected Real output, got {:?}", other),
        }

        println!("  t={:.0}ns: freq={:?} Hz, output={:?}",
                time * 1e9, freq, output);
    }
}

#[test]
fn test_pwm_generator() {
    println!("\n=== PWM Generator with When Blocks ===");
    
    // Create circuit state
    let topology = CircuitTopology {
        instance_modules: HashMap::new(),
        net_connections: HashMap::new(),
    };
    let mut circuit_state = CircuitState::new(topology);
    
    // Initialize PWM attributes
    circuit_state.update_attribute("duty_cycle", RuntimeValue::Real(0.75)); // 75% duty
    circuit_state.update_attribute("period", RuntimeValue::Real(1e-6)); // 1µs period
    circuit_state.update_attribute("counter", RuntimeValue::Real(0.0));
    circuit_state.update_attribute("output", RuntimeValue::Real(0.0));
    
    // Create when blocks for PWM logic
    let when_blocks = vec![
        WhenBlockInfo {
            condition: "counter < duty_cycle * period".to_string(),
            assignments: {
                let mut map = HashMap::new();
                map.insert("output".to_string(), "5.0".to_string()); // High
                map
            },
        },
        WhenBlockInfo {
            condition: "counter >= duty_cycle * period".to_string(),
            assignments: {
                let mut map = HashMap::new();
                map.insert("output".to_string(), "0.0".to_string()); // Low
                map
            },
        },
    ];
    
    let mut processor = WhenBlockProcessor::new(when_blocks);
    let time_manager = TimeManager::new(100e-9); // 100ns steps
    
    println!("PWM output pattern:");
    for i in 0..20 {
        let counter_val = (i as f64 * 100e-9) % 1e-6; // Wrap at period
        circuit_state.update_attribute("counter", RuntimeValue::Real(counter_val));
        
        // Process when blocks
        processor.process_all(&mut circuit_state, &time_manager).unwrap();
        
        let output = circuit_state.get_attribute("output").unwrap();
        let bar = if let RuntimeValue::Real(v) = output {
            if *v > 2.5 { "████" } else { "    " }
        } else { "????" };
        
        println!("  t={:4.0}ns: {} output={:?}", i * 100, bar, output);
    }
}

#[test]
fn test_temperature_sensor_model() {
    println!("\n=== Temperature Sensor Behavioral Model ===");
    
    // Create circuit state
    let topology = CircuitTopology {
        instance_modules: HashMap::new(),
        net_connections: HashMap::new(),
    };
    let mut circuit_state = CircuitState::new(topology);
    
    // Initialize sensor attributes
    circuit_state.update_attribute("temperature", RuntimeValue::Real(25.0)); // 25°C
    circuit_state.update_attribute("sensitivity", RuntimeValue::Real(10e-3)); // 10mV/°C
    circuit_state.update_attribute("offset", RuntimeValue::Real(0.5)); // 0.5V at 0°C
    circuit_state.update_attribute("noise_amplitude", RuntimeValue::Real(1e-3)); // 1mV noise
    circuit_state.update_attribute("output_voltage", RuntimeValue::Real(0.0));
    
    // Expression for temperature sensor: linear transfer function plus a
    // small sinusoidal noise term.
    let mut expression_texts = HashMap::new();
    expression_texts.insert(
        "output_voltage".to_string(),
        "offset + sensitivity * temperature + noise_amplitude * sin(1000 * t)".to_string()
    );
    
    let analysis = AttributeAnalysisResult {
        attributes: HashMap::new(),
        dependencies: HashMap::new(),
        evaluation_order: vec!["output_voltage".to_string()],
        circular_dependencies: vec![],
        mutable_attributes: HashSet::new(),
    };
    
    let mut evaluator = SimulationAttributeEvaluator::with_expressions(analysis, expression_texts);
    let mut time_manager = TimeManager::new(1e-6);
    
    // Test at different temperatures
    let test_temps = vec![0.0, 25.0, 50.0, 75.0, 100.0];
    
    println!("Temperature sensor response:");
    for (i, temp) in test_temps.into_iter().enumerate() {
        circuit_state.update_attribute("temperature", RuntimeValue::Real(temp));

        time_manager.advance_by(1e-3).unwrap();
        let t = (i as f64 + 1.0) * 1e-3;

        let attr_ids = vec![
            bhdl_sim::evaluation::scheduler::AttributeId("output_voltage".to_string()),
        ];

        evaluator.evaluate_batch(&attr_ids, &mut circuit_state, &time_manager).unwrap();

        // V_out = offset + sensitivity * T + noise = 0.5 + 0.01 * T + 1mV * sin(1000 t)
        let expected = 0.5 + 10e-3 * temp + 1e-3 * (1000.0 * t).sin();
        match circuit_state.get_attribute("output_voltage") {
            Some(RuntimeValue::Real(v)) => {
                assert!((v - expected).abs() < 1e-9,
                        "T={}: expected {}V, got {}V", temp, expected, v);
                println!("  T={:3.0}°C: {:.3}V", temp, v);
            }
            other => panic!("expected Real output_voltage, got {:?}", other),
        }
    }
}

#[test]
fn test_digital_filter_behavioral() {
    println!("\n=== Digital Filter Behavioral Model ===");
    
    // Create circuit state for a simple moving average filter
    let topology = CircuitTopology {
        instance_modules: HashMap::new(),
        net_connections: HashMap::new(),
    };
    let mut circuit_state = CircuitState::new(topology);
    
    // Initialize filter state
    circuit_state.update_attribute("input", RuntimeValue::Real(0.0));
    circuit_state.update_attribute("sample1", RuntimeValue::Real(0.0));
    circuit_state.update_attribute("sample2", RuntimeValue::Real(0.0));
    circuit_state.update_attribute("sample3", RuntimeValue::Real(0.0));
    circuit_state.update_attribute("output", RuntimeValue::Real(0.0));
    
    // Expression for 3-tap moving average
    let mut expression_texts = HashMap::new();
    expression_texts.insert(
        "output".to_string(),
        "(input + sample1 + sample2 + sample3) / 4.0".to_string()
    );
    
    // When blocks to shift samples
    let when_blocks = vec![
        WhenBlockInfo {
            condition: "true".to_string(), // Always execute
            assignments: {
                let mut map = HashMap::new();
                map.insert("sample3".to_string(), "sample2".to_string());
                map.insert("sample2".to_string(), "sample1".to_string());
                map.insert("sample1".to_string(), "input".to_string());
                map
            },
        },
    ];
    
    let mut processor = WhenBlockProcessor::new(when_blocks);
    let time_manager = TimeManager::new(1e-6);
    
    // Test with step input
    println!("Moving average filter response to step input:");
    for i in 0..8 {
        // Step input at i=2
        let input_val = if i >= 2 { 1.0 } else { 0.0 };
        circuit_state.update_attribute("input", RuntimeValue::Real(input_val));
        
        // Process filter
        processor.process_all(&mut circuit_state, &time_manager).unwrap();
        
        // Note: In a real implementation, we'd evaluate the output expression here
        // For now, manually calculate
        let s1 = circuit_state.get_attribute("sample1").unwrap();
        let s2 = circuit_state.get_attribute("sample2").unwrap();
        let s3 = circuit_state.get_attribute("sample3").unwrap();
        
        println!("  Step {}: input={:.1}, samples=[{:?}, {:?}, {:?}]",
                i, input_val, s1, s2, s3);
    }
}

#[test]
fn test_expression_parser_advanced() {
    println!("\n=== Advanced Expression Parsing ===");
    
    let mut parser = ExpressionParser::new();
    
    // Test various expression types
    let test_cases = vec![
        ("Simple arithmetic", "2 + 3 * 4"),
        ("Parentheses", "(2 + 3) * 4"),
        ("Function calls", "sin(2 * pi * t)"),
        ("Nested functions", "pow(sin(x), 2) + pow(cos(x), 2)"),
        ("Comparisons", "voltage > 3.3"),
        ("Logical operations", "enable && (voltage > 3.3 || current < 0.1)"),
    ];
    
    println!("Testing expression parser:");
    for (desc, expr) in test_cases {
        match parser.parse(expr) {
            Ok(_) => println!("  ✓ {}: '{}'", desc, expr),
            Err(e) => println!("  ✗ {}: '{}' - Error: {}", desc, expr, e),
        }
    }
    
    // Test cache performance
    let stats = parser.stats();
    println!("\nParser statistics:");
    println!("  - Total parses: {}", stats.parse_count);
    println!("  - Cache hits: {}", stats.cache_hits);
    println!("  - Cache size: {}", stats.cache_size);
}