//! Testbench compiler to convert AST to runtime structures

use anyhow::Result;
use std::collections::HashMap;

use bhdl_ast::{
    TestbenchDef, SimulationBlock, ScopeDef, StimulusBlock, VerifyBlock, 
    MeasureBlock, SignalRef, TimeSpec, AstNode, HasName
};
use bhdl_parser::{SyntaxKind, BhdlLanguage};

use crate::testbench::{
    Testbench, SimulationConfig, SolverType, Scope, CaptureMode,
    Stimulus, Waveform, Assertion, AssertionCondition,
    TimeConstraint, Measurement
};
use crate::{SignalRef as RuntimeSignalRef, TimeSpec as RuntimeTimeSpec, TimeUnit};

/// Compile a testbench from AST to runtime structure
pub fn compile_testbench(ast: &TestbenchDef) -> Result<Testbench> {
    let name = ast.name()
        .ok_or_else(|| anyhow::anyhow!("Testbench missing name"))?
        .text()
        .to_string();
        
    let target_board = ast.target_board()
        .ok_or_else(|| anyhow::anyhow!("Testbench missing target board"))?
        .text()
        .to_string();
        
    let simulation_config = ast.simulation_block()
        .map(|block| compile_simulation_config(&block))
        .transpose()?
        .unwrap_or_default();
        
    let scopes = ast.scopes()
        .map(|scope| compile_scope(&scope))
        .collect::<Result<Vec<_>>>()?;
        
    let stimuli = ast.stimulus_block()
        .map(|block| compile_stimuli(&block))
        .transpose()?
        .unwrap_or_default();
        
    let assertions = ast.verify_block()
        .map(|block| compile_assertions(&block))
        .transpose()?
        .unwrap_or_default();
    
    println!("DEBUG: Compiled {} assertions", assertions.len());
        
    let measurements = ast.measure_block()
        .map(|block| compile_measurements(&block))
        .transpose()?
        .unwrap_or_default();
        
    Ok(Testbench {
        name,
        target_board,
        simulation_config,
        scopes,
        stimuli,
        assertions,
        measurements,
    })
}

fn compile_simulation_config(block: &SimulationBlock) -> Result<SimulationConfig> {
    let duration = block.duration()
        .map(|ts| compile_time_spec(&ts))
        .transpose()?
        .unwrap_or(RuntimeTimeSpec { value: 10.0, unit: TimeUnit::Milliseconds });
        
    let timestep = block.timestep()
        .map(|ts| compile_time_spec(&ts))
        .transpose()?
        .unwrap_or(RuntimeTimeSpec { value: 10.0, unit: TimeUnit::Microseconds });
        
    let temperature = block.temperature()
        .and_then(|node| {
            // Try to extract temperature value
            node.children_with_tokens()
                .filter_map(|e| e.into_token())
                .find(|t| t.kind() == SyntaxKind::NUMBER)
                .and_then(|t| t.text().parse::<f64>().ok())
        })
        .unwrap_or(25.0);
        
    let solver_type = block.solver()
        .map(|token| {
            match token.text().as_ref() {
                "spice" => SolverType::SpiceAdaptive,
                "behavioral" => SolverType::Behavioral,
                "mixed" => SolverType::MixedSignal { 
                    analog_timestep: RuntimeTimeSpec { value: 1.0, unit: TimeUnit::Microseconds },
                    digital_timestep: RuntimeTimeSpec { value: 10.0, unit: TimeUnit::Nanoseconds },
                },
                _ => SolverType::SpiceAdaptive,
            }
        })
        .unwrap_or(SolverType::SpiceAdaptive);
        
    Ok(SimulationConfig {
        duration,
        timestep,
        solver_type,
        temperature,
        save_matrices: false,
    })
}

fn compile_scope(scope: &ScopeDef) -> Result<Scope> {
    let name = scope.name()
        .map(|t| {
            // Remove quotes from string literal
            let text = t.text();
            text.trim_matches('"').to_string()
        })
        .unwrap_or_else(|| "default".to_string());
        
    let signals = scope.signals()
        .map(|sig| compile_signal_ref(&sig))
        .collect::<Result<Vec<_>>>()?;
        
    let capture_mode = scope.capture_mode()
        .map(|mode| {
            // Parse capture mode from AST
            // For now, default to continuous
            CaptureMode::Continuous
        })
        .unwrap_or(CaptureMode::Continuous);
        
    Ok(Scope {
        name,
        signals,
        capture_mode,
        trigger: None,
        output_file: None,
    })
}

fn compile_signal_ref(signal: &SignalRef) -> Result<RuntimeSignalRef> {
    // Extract signal reference from AST
    let text = signal.syntax().text().to_string();
    
    if text.starts_with('@') {
        // Net reference - keep the @ prefix for proper signal mapping
        Ok(RuntimeSignalRef::Net(text.to_string()))
    } else if let Some(dot_pos) = text.find('.') {
        // Component pin or property
        let component = text[..dot_pos].to_string();
        let pin = text[dot_pos + 1..].to_string();
        
        if pin == "current" {
            Ok(RuntimeSignalRef::Current(component))
        } else if pin == "voltage" {
            Ok(RuntimeSignalRef::Voltage(component))
        } else if pin == "power" {
            Ok(RuntimeSignalRef::Power(component))
        } else {
            Ok(RuntimeSignalRef::Pin { 
                instance: component, 
                pin 
            })
        }
    } else {
        // Assume it's a net without @
        Ok(RuntimeSignalRef::Net(text))
    }
}

fn compile_stimuli(block: &StimulusBlock) -> Result<Vec<Stimulus>> {
    block.assignments()
        .map(|assign| {
            let signal = assign.signal()
                .ok_or_else(|| anyhow::anyhow!("Stimulus missing signal reference"))?;
            let signal_ref = compile_signal_ref(&signal)?;
            
            let waveform_expr = assign.waveform()
                .ok_or_else(|| anyhow::anyhow!("Stimulus missing waveform"))?;
                
            // Parse waveform type from AST
            let waveform = parse_waveform_expr(&waveform_expr)?;
            
            Ok(Stimulus {
                target: signal_ref,
                waveform,
            })
        })
        .collect()
}

fn compile_assertions(block: &VerifyBlock) -> Result<Vec<Assertion>> {
    block.assertions()
        .enumerate()
        .map(|(i, assertion)| {
            // Extract the assertion expression
            let syntax = assertion.syntax();
            let mut expr_text = String::new();
            let mut found_assert = false;
            
            for child in syntax.children_with_tokens() {
                match child {
                    rowan::NodeOrToken::Token(token) => {
                        if token.kind() == SyntaxKind::IDENT && token.text() == "assert" {
                            found_assert = true;
                        } else if found_assert && token.kind() != SyntaxKind::IDENT && token.text() != "message" {
                            expr_text.push_str(token.text());
                        }
                    }
                    rowan::NodeOrToken::Node(node) => {
                        if found_assert {
                            expr_text.push_str(&node.text().to_string());
                        }
                    }
                }
            }
            
            // Extract message if present
            let message = assertion.message()
                .map(|t| {
                    let text = t.text();
                    text.trim_matches('"').to_string()
                })
                .unwrap_or_else(|| format!("Assertion {} failed", i));
            
            // Parse the condition from the expression text
            let condition = parse_assertion_condition_from_text(&expr_text)?;
            let time_constraint = parse_time_constraint_from_text(&expr_text)?;
                
            Ok(Assertion {
                name: message.clone(),
                condition,
                time_constraint,
                severity: crate::testbench::Severity::Error,
                message,
            })
        })
        .collect()
}

fn compile_measurements(block: &MeasureBlock) -> Result<HashMap<String, Measurement>> {
    block.measurements()
        .map(|measurement| {
            let name = measurement.name()
                .map(|t| t.text().to_string())
                .unwrap_or_else(|| "unnamed".to_string());
                
            // Parse the measurement expression from the AST
            // The structure is: NAME = EXPR
            let measurement_type = parse_measurement_expression(&measurement)?;
            
            let meas = Measurement {
                name: name.clone(),
                measurement_type,
            };
            
            Ok((name, meas))
        })
        .collect()
}

/// Parse measurement expression to determine type and signal
fn parse_measurement_expression(measurement: &bhdl_ast::testbench::Measurement) -> Result<crate::testbench::MeasurementType> {
    // Find the expression after the = sign
    let syntax = measurement.syntax();
    let mut found_eq = false;
    
    for child in syntax.children_with_tokens() {
        match child {
            rowan::NodeOrToken::Token(token) => {
                if token.kind() == SyntaxKind::EQ {
                    found_eq = true;
                }
            }
            rowan::NodeOrToken::Node(node) => {
                if found_eq {
                    // This should be the expression node
                    return parse_measurement_type_from_expr(&node);
                }
            }
        }
    }
    
    Err(anyhow::anyhow!("No expression found in measurement"))
}

/// Parse the expression to determine measurement type
fn parse_measurement_type_from_expr(expr: &rowan::SyntaxNode<BhdlLanguage>) -> Result<crate::testbench::MeasurementType> {
    let text = expr.text().to_string();
    
    // Simple heuristics for now - can be expanded later
    if text.contains(".current") {
        // Extract component name from "component.current"
        if let Some(dot_pos) = text.find(".current") {
            let component = text[..dot_pos].trim().to_string();
            return Ok(crate::testbench::MeasurementType::Average {
                signal: crate::SignalRef::Current(component)
            });
        }
    } else if text.contains(".voltage") {
        // Extract component name from "component.voltage"
        if let Some(dot_pos) = text.find(".voltage") {
            let component = text[..dot_pos].trim().to_string();
            return Ok(crate::testbench::MeasurementType::Average {
                signal: crate::SignalRef::Voltage(component)
            });
        }
    } else if text.contains(".power") {
        // Extract component name from "component.power"
        if let Some(dot_pos) = text.find(".power") {
            let component = text[..dot_pos].trim().to_string();
            return Ok(crate::testbench::MeasurementType::Average {
                signal: crate::SignalRef::Power(component)
            });
        }
    } else if text.starts_with("rms(") && text.ends_with(")") {
        // RMS measurement: rms(signal)
        let inner = &text[4..text.len()-1];
        let signal = compile_signal_ref_from_text(inner)?;
        return Ok(crate::testbench::MeasurementType::RMS { signal });
    } else if text.starts_with("peak_to_peak(") && text.ends_with(")") {
        // Peak-to-peak measurement: peak_to_peak(signal)
        let inner = &text[13..text.len()-1];
        let signal = compile_signal_ref_from_text(inner)?;
        return Ok(crate::testbench::MeasurementType::PeakToPeak { 
            signal,
            window: None,
        });
    } else if text.starts_with("integral(") && text.ends_with(")") {
        // Integral measurement: integral(expression)
        let inner = &text[9..text.len()-1];
        return Ok(crate::testbench::MeasurementType::Integral { 
            expression: inner.to_string() 
        });
    }
    
    // Default to average measurement of the expression as a signal ref
    let signal = compile_signal_ref_from_text(&text)?;
    Ok(crate::testbench::MeasurementType::Average { signal })
}

/// Compile signal reference from text
fn compile_signal_ref_from_text(text: &str) -> Result<RuntimeSignalRef> {
    let text = text.trim();
    
    if text.starts_with('@') {
        // Net reference - keep the @ prefix for proper signal mapping
        Ok(RuntimeSignalRef::Net(text.to_string()))
    } else if let Some(dot_pos) = text.find('.') {
        // Component pin or property
        let component = text[..dot_pos].to_string();
        let pin = text[dot_pos + 1..].to_string();
        
        if pin == "current" {
            Ok(RuntimeSignalRef::Current(component))
        } else if pin == "voltage" {
            Ok(RuntimeSignalRef::Voltage(component))
        } else if pin == "power" {
            Ok(RuntimeSignalRef::Power(component))
        } else {
            Ok(RuntimeSignalRef::Pin { 
                instance: component, 
                pin 
            })
        }
    } else {
        // Assume it's a net without @
        Ok(RuntimeSignalRef::Net(text.to_string()))
    }
}

fn parse_waveform_expr(expr: &bhdl_ast::testbench::WaveformExpr) -> Result<Waveform> {
    // Get the text of the waveform expression
    let text = expr.syntax().text().to_string();
    
    // Parse function call patterns
    if text.starts_with("ramp(") && text.ends_with(")") {
        // Extract parameters from ramp(from: 0V, to: 5V, duration: 1ms)
        let params_text = &text[5..text.len()-1];
        let mut from_value = 0.0;
        let mut to_value = 0.0;
        let mut duration = RuntimeTimeSpec { value: 1.0, unit: TimeUnit::Milliseconds };
        
        // Simple parsing of key-value pairs
        for param in params_text.split(',') {
            let param = param.trim();
            if let Some(colon_pos) = param.find(':') {
                let key = param[..colon_pos].trim();
                let value_str = param[colon_pos+1..].trim();
                
                match key {
                    "from" => from_value = parse_voltage_value(value_str).unwrap_or(0.0),
                    "to" => to_value = parse_voltage_value(value_str).unwrap_or(0.0),
                    "duration" => duration = parse_time_value(value_str).unwrap_or(duration),
                    _ => {}
                }
            }
        }
        
        Ok(Waveform::Ramp {
            start_value: from_value,
            end_value: to_value,
            duration,
        })
    } else if text.starts_with("pulse(") && text.ends_with(")") {
        // Parse pulse waveform
        // pulse(low: 0V, high: 5V, delay: 100us, width: 500us, period: 1ms)
        let params_text = &text[6..text.len()-1];
        let mut low = 0.0;
        let mut high = 5.0;
        let mut delay = RuntimeTimeSpec { value: 0.0, unit: TimeUnit::Microseconds };
        let mut width = RuntimeTimeSpec { value: 500.0, unit: TimeUnit::Microseconds };
        let mut period = RuntimeTimeSpec { value: 1.0, unit: TimeUnit::Milliseconds };
        
        for param in params_text.split(',') {
            let param = param.trim();
            if let Some(colon_pos) = param.find(':') {
                let key = param[..colon_pos].trim();
                let value_str = param[colon_pos+1..].trim();
                
                match key {
                    "low" => low = parse_voltage_value(value_str).unwrap_or(0.0),
                    "high" => high = parse_voltage_value(value_str).unwrap_or(5.0),
                    "delay" => delay = parse_time_value(value_str).unwrap_or(delay),
                    "width" => width = parse_time_value(value_str).unwrap_or(width),
                    "period" => period = parse_time_value(value_str).unwrap_or(period),
                    _ => {}
                }
            }
        }
        
        Ok(Waveform::Pulse { low, high, delay, width, period })
    } else {
        // Try to parse as a constant value
        if let Some(value) = parse_voltage_value(&text) {
            Ok(Waveform::Constant(value))
        } else {
            Err(anyhow::anyhow!("Unsupported waveform type: {}", text))
        }
    }
}

fn parse_voltage_value(text: &str) -> Option<f64> {
    let text = text.trim();
    
    // Handle various unit suffixes
    let (num_text, unit_multiplier) = if text.ends_with("mA") {
        (&text[..text.len()-2], 0.001)
    } else if text.ends_with("uA") || text.ends_with("µA") {
        (&text[..text.len()-2], 0.000001)
    } else if text.ends_with("A") {
        (&text[..text.len()-1], 1.0)
    } else if text.ends_with("mV") {
        (&text[..text.len()-2], 0.001)
    } else if text.ends_with("V") {
        (&text[..text.len()-1], 1.0)
    } else {
        (text, 1.0)
    };
    
    // Parse the numeric part
    if let Ok(value) = num_text.parse::<f64>() {
        Some(value * unit_multiplier)
    } else {
        // Try parsing with unit prefixes
        crate::coordinator::parse_value_with_units(text)
    }
}

fn parse_time_value(text: &str) -> Option<RuntimeTimeSpec> {
    let text = text.trim();
    
    // Find where number ends and unit begins
    let mut num_end = text.len();
    for (i, ch) in text.char_indices() {
        if ch.is_alphabetic() && i > 0 {
            num_end = i;
            break;
        }
    }
    
    let (num_part, unit_part) = text.split_at(num_end);
    let value: f64 = num_part.parse().ok()?;
    
    let unit = match unit_part {
        "s" => TimeUnit::Seconds,
        "ms" => TimeUnit::Milliseconds,
        "us" | "µs" => TimeUnit::Microseconds,
        "ns" => TimeUnit::Nanoseconds,
        "ps" => TimeUnit::Picoseconds,
        _ => TimeUnit::Microseconds,
    };
    
    Some(RuntimeTimeSpec { value, unit })
}

fn compile_time_spec(spec: &TimeSpec) -> Result<RuntimeTimeSpec> {
    let value = spec.number()
        .and_then(|t| t.text().parse::<f64>().ok())
        .unwrap_or(0.0);
        
    let unit = spec.unit()
        .map(|t| {
            match t.text().as_ref() {
                "s" => TimeUnit::Seconds,
                "ms" => TimeUnit::Milliseconds,
                "us" | "µs" => TimeUnit::Microseconds,
                "ns" => TimeUnit::Nanoseconds,
                "ps" => TimeUnit::Picoseconds,
                _ => TimeUnit::Microseconds,
            }
        })
        .unwrap_or(TimeUnit::Microseconds);
        
    Ok(RuntimeTimeSpec { value, unit })
}

/// Parse assertion condition from text
fn parse_assertion_condition_from_text(text: &str) -> Result<AssertionCondition> {
    let text = text.trim();
    
    // Check for "signal in min..max" pattern
    if text.contains(" in ") && text.contains("..") {
        let parts: Vec<&str> = text.split(" in ").collect();
        if parts.len() == 2 {
            let signal_text = parts[0].trim();
            let range_text = parts[1].trim();
            
            // Parse signal reference
            let signal = compile_signal_ref_from_text(signal_text)?;
            
            // Parse range (min..max)
            if let Some(dot_pos) = range_text.find("..") {
                let min_text = &range_text[..dot_pos];
                let max_text = &range_text[dot_pos + 2..];
                
                // Remove "always" or other time constraints from max_text
                let max_text = max_text.split_whitespace().next().unwrap_or(max_text);
                
                let min = parse_voltage_value(min_text).unwrap_or(0.0);
                let max = parse_voltage_value(max_text).unwrap_or(0.0);
                
                return Ok(AssertionCondition::SignalInRange { signal, min, max });
            }
        }
    }
    
    // Check for "signal == value +/- tolerance" pattern
    if text.contains("==") && text.contains("+/-") {
        let parts: Vec<&str> = text.split("==").collect();
        if parts.len() == 2 {
            let signal_text = parts[0].trim();
            let value_tol_text = parts[1].trim();
            
            // Parse signal reference
            let signal = compile_signal_ref_from_text(signal_text)?;
            
            // Parse "value +/- tolerance"
            if let Some(tol_pos) = value_tol_text.find("+/-") {
                let value_text = &value_tol_text[..tol_pos].trim();
                let tol_text = &value_tol_text[tol_pos + 3..].trim();
                
                // Remove time constraints
                let tol_text = tol_text.split_whitespace().next().unwrap_or(tol_text);
                
                let value = parse_voltage_value(value_text).unwrap_or(0.0);
                let tolerance = parse_voltage_value(tol_text).unwrap_or(0.0);
                
                return Ok(AssertionCondition::SignalEquals { signal, value, tolerance });
            }
        }
    }
    
    // Check for "signal < value" pattern
    if text.contains(" < ") {
        let parts: Vec<&str> = text.split(" < ").collect();
        if parts.len() == 2 {
            let signal_text = parts[0].trim();
            let value_text = parts[1].trim();
            
            // Parse signal reference
            let signal = compile_signal_ref_from_text(signal_text)?;
            
            // Remove time constraints
            let value_text = value_text.split_whitespace().next().unwrap_or(value_text);
            let max = parse_voltage_value(value_text).unwrap_or(0.0);
            
            return Ok(AssertionCondition::SignalInRange { 
                signal, 
                min: f64::NEG_INFINITY, 
                max 
            });
        }
    }
    
    // Default to expression
    Ok(AssertionCondition::Expression(text.to_string()))
}

/// Parse time constraint from text
fn parse_time_constraint_from_text(text: &str) -> Result<TimeConstraint> {
    let text = text.trim();
    
    if text.contains(" always") {
        Ok(TimeConstraint::Always)
    } else if text.contains(" after ") {
        // Extract time after "after" keyword
        if let Some(after_pos) = text.find(" after ") {
            let after_text = &text[after_pos + 7..];
            // Take first word/token after "after"
            let time_text = after_text.split_whitespace().next().unwrap_or("");
            if let Some(time_spec) = parse_time_value(time_text) {
                return Ok(TimeConstraint::After(time_spec));
            }
        }
        Ok(TimeConstraint::Always)
    } else {
        Ok(TimeConstraint::Always)
    }
}