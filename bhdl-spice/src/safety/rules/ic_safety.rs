//! Safety rules for integrated circuits

use crate::circuit::{Circuit, ComponentId, NodeId};
use crate::safety::{SafetyViolation, Severity, CircuitLocation, DamageEstimate};
use std::time::Duration;

/// Helper function for 2-pin voltage regulator safety check
fn check_voltage_regulator_safety_2pin(
    circuit: &Circuit,
    comp_id: ComponentId,
    component: &crate::circuit::Component,
    vin_node: NodeId,
    vout_node: NodeId,
    gnd_node: NodeId,
) -> Vec<SafetyViolation> {
    let mut violations = Vec::new();
    
    let vin = circuit.get_node_by_id(vin_node)
        .and_then(|n| n.voltage)
        .unwrap_or(0.0);
    let vout = circuit.get_node_by_id(vout_node)
        .and_then(|n| n.voltage)
        .unwrap_or(0.0);
    let gnd = circuit.get_node_by_id(gnd_node)
        .and_then(|n| n.voltage)
        .unwrap_or(0.0);
    
    // Calculate actual voltages relative to ground
    let vin_actual = vin - gnd;
    let vout_actual = vout - gnd;
    
    // Extract parameters from component
    let vout_nom = component.get_parameter("vout_nominal").unwrap_or(5.0);
    let dropout = component.get_parameter("dropout").unwrap_or(2.0);
    let vin_max = component.get_parameter("vin_max").unwrap_or(35.0);
    let vin_min = vout_nom + dropout;
    let iout_max = component.get_parameter("iout_max").unwrap_or(1.0);
    let power_max = component.get_parameter("power_max").unwrap_or(15.0);
    let tj_max = component.get_parameter("tj_max").unwrap_or(125.0);
    let rth_ja = component.get_parameter("rth_ja").unwrap_or(65.0);
    
    // Check 1: Input voltage range
    if vin_actual < vin_min {
        violations.push(SafetyViolation {
            rule_name: "Voltage Regulator Input Too Low".to_string(),
            severity: if vin_actual < vin_min - 0.5 {
                Severity::Error
            } else {
                Severity::Warning
            },
            location: CircuitLocation {
                nodes: vec![vin_node, vout_node, gnd_node],
                components: vec![comp_id],
                nets: vec![],
                description: format!("{} voltage regulator", component.name()),
            },
            message: format!(
                "{} requires minimum {}V input for {}V output ({}V dropout), but input is only {}V",
                component.name(), vin_min, vout_nom, dropout, vin_actual
            ),
            technical_details: format!(
                "Vin={:.1}V, Vin_min={:.1}V, Vout_nom={:.1}V, Dropout={:.1}V",
                vin_actual, vin_min, vout_nom, dropout
            ),
            user_impact: "Regulator cannot maintain stable output voltage".to_string(),
            estimated_damage: None,
        });
    }
    
    if vin_actual > vin_max {
        violations.push(SafetyViolation {
            rule_name: "Voltage Regulator Input Exceeds Maximum".to_string(),
            severity: Severity::Critical,
            location: CircuitLocation {
                nodes: vec![vin_node, vout_node, gnd_node],
                components: vec![comp_id],
                nets: vec![],
                description: format!("{} voltage regulator", component.name()),
            },
            message: format!(
                "{} has maximum input voltage of {}V, but input is {}V",
                component.name(), vin_max, vin_actual
            ),
            technical_details: format!(
                "Vin={:.1}V, Vin_max={:.1}V",
                vin_actual, vin_max
            ),
            user_impact: "Regulator will be damaged by excessive input voltage".to_string(),
            estimated_damage: Some(DamageEstimate {
                failure_mode: "Thermal runaway and internal breakdown".to_string(),
                time_to_failure: Some(Duration::from_secs(1)),
                affected_components: vec![component.name().to_string()],
                estimated_cost: Some(5.0),
            }),
        });
    }
    
    // Check 2: Output current
    let iout = component.current.unwrap_or(0.0).abs();
    if iout > iout_max {
        violations.push(SafetyViolation {
            rule_name: "Voltage Regulator Current Exceeds Rating".to_string(),
            severity: if iout > iout_max * 1.2 {
                Severity::Error
            } else {
                Severity::Warning
            },
            location: CircuitLocation {
                nodes: vec![vin_node, vout_node, gnd_node],
                components: vec![comp_id],
                nets: vec![],
                description: format!("{} voltage regulator", component.name()),
            },
            message: format!(
                "{} is rated for {}A maximum, but output current is {}A",
                component.name(), iout_max, iout
            ),
            technical_details: format!(
                "Iout={:.3}A, Iout_max={:.3}A, Overload={:.0}%",
                iout, iout_max, (iout / iout_max - 1.0) * 100.0
            ),
            user_impact: "Regulator may overheat and shut down".to_string(),
            estimated_damage: if iout > iout_max * 1.5 {
                Some(DamageEstimate {
                    failure_mode: "Thermal damage".to_string(),
                    time_to_failure: Some(Duration::from_secs(60)),
                    affected_components: vec![component.name().to_string()],
                    estimated_cost: Some(5.0),
                })
            } else {
                None
            },
        });
    }
    
    // Check 3: Power dissipation
    let iq = component.get_parameter("iq").unwrap_or(0.005);
    let power = (vin_actual - vout_actual) * iout + vin_actual * iq;
    
    if power > power_max {
        violations.push(SafetyViolation {
            rule_name: "Voltage Regulator Power Exceeds Rating".to_string(),
            severity: Severity::Error,
            location: CircuitLocation {
                nodes: vec![vin_node, vout_node, gnd_node],
                components: vec![comp_id],
                nets: vec![],
                description: format!("{} voltage regulator", component.name()),
            },
            message: format!(
                "{} dissipating {:.1}W, exceeds {:.1}W maximum",
                component.name(), power, power_max
            ),
            technical_details: format!(
                "Power={:.1}W, Power_max={:.1}W, Vin={:.1}V, Vout={:.1}V, Iout={:.3}A",
                power, power_max, vin_actual, vout_actual, iout
            ),
            user_impact: "Regulator will overheat without proper cooling".to_string(),
            estimated_damage: Some(DamageEstimate {
                failure_mode: "Thermal shutdown or damage".to_string(),
                time_to_failure: Some(Duration::from_secs(300)),
                affected_components: vec![component.name().to_string()],
                estimated_cost: Some(5.0),
            }),
        });
    }
    
    // Check 4: Junction temperature
    let tamb = 25.0; // Assume room temperature
    let tj = tamb + power * rth_ja;
    
    if tj > tj_max {
        violations.push(SafetyViolation {
            rule_name: "Voltage Regulator Junction Temperature Exceeded".to_string(),
            severity: Severity::Critical,
            location: CircuitLocation {
                nodes: vec![vin_node, vout_node, gnd_node],
                components: vec![comp_id],
                nets: vec![],
                description: format!("{} voltage regulator", component.name()),
            },
            message: format!(
                "{} junction temperature {:.0}°C exceeds {:.0}°C maximum",
                component.name(), tj, tj_max
            ),
            technical_details: format!(
                "Tj={:.0}°C, Tj_max={:.0}°C, Power={:.1}W, Rth_ja={:.1}°C/W, Required_Rth<{:.1}°C/W",
                tj, tj_max, power, rth_ja, (tj_max - tamb) / power
            ),
            user_impact: "Regulator will fail due to excessive temperature".to_string(),
            estimated_damage: Some(DamageEstimate {
                failure_mode: "Permanent thermal damage".to_string(),
                time_to_failure: Some(Duration::from_secs(10)),
                affected_components: vec![component.name().to_string()],
                estimated_cost: Some(5.0),
            }),
        });
    } else if tj > tj_max - 25.0 {
        violations.push(SafetyViolation {
            rule_name: "Voltage Regulator Running Hot".to_string(),
            severity: Severity::Warning,
            location: CircuitLocation {
                nodes: vec![vin_node, vout_node, gnd_node],
                components: vec![comp_id],
                nets: vec![],
                description: format!("{} voltage regulator", component.name()),
            },
            message: format!(
                "{} junction temperature {:.0}°C approaching {:.0}°C limit",
                component.name(), tj, tj_max
            ),
            technical_details: format!(
                "Tj={:.0}°C, Tj_max={:.0}°C, Margin={:.0}°C",
                tj, tj_max, tj_max - tj
            ),
            user_impact: "Regulator reliability may be reduced".to_string(),
            estimated_damage: None,
        });
    }
    
    // Check 5: Output capacitor presence (for stability)
    let has_output_cap = check_output_capacitor(circuit, vout_node.index(), gnd_node.index());
    if !has_output_cap {
        violations.push(SafetyViolation {
            rule_name: "Voltage Regulator Missing Output Capacitor".to_string(),
            severity: Severity::Warning,
            location: CircuitLocation {
                nodes: vec![vout_node, gnd_node],
                components: vec![comp_id],
                nets: vec![],
                description: format!("{} output", component.name()),
            },
            message: format!(
                "{} requires output capacitor for stability",
                component.name()
            ),
            technical_details: "Linear regulators require output capacitance for loop stability".to_string(),
            user_impact: "Regulator may oscillate or have poor transient response".to_string(),
            estimated_damage: None,
        });
    }
    
    violations
}

/// Check voltage regulator safety constraints
pub fn check_voltage_regulator_safety(
    circuit: &Circuit,
    comp_id: ComponentId,
) -> Vec<SafetyViolation> {
    let mut violations = Vec::new();
    
    let component = match circuit.get_component(comp_id) {
        Some(comp) => comp,
        None => return violations,
    };
    
    // Only check voltage regulators
    if component.component_type() != "VoltageRegulator" {
        return violations;
    }
    
    // Get component nodes
    let nodes = component.nodes();
    if nodes.len() < 3 {
        // For now, handle 2-terminal voltage regulators as a special case
        // In a real circuit, voltage regulators have 3 terminals (IN, OUT, GND)
        // but our simplified model only tracks IN->OUT
        if nodes.len() == 2 {
            // Assume ground is node 0 for testing
            let vin_node = nodes[0];
            let vout_node = nodes[1];
            let gnd_node = NodeId::new(0); // Assume GND is node 0
            
            return check_voltage_regulator_safety_2pin(
                circuit, comp_id, component, vin_node, vout_node, gnd_node
            );
        }
        return violations;
    }
    
    // Get node voltages (IN, OUT, GND)
    let vin_node = nodes[0];
    let vout_node = nodes[1];
    let gnd_node = nodes[2];
    
    let vin = circuit.get_node_by_id(vin_node)
        .and_then(|n| n.voltage)
        .unwrap_or(0.0);
    let vout = circuit.get_node_by_id(vout_node)
        .and_then(|n| n.voltage)
        .unwrap_or(0.0);
    let gnd = circuit.get_node_by_id(gnd_node)
        .and_then(|n| n.voltage)
        .unwrap_or(0.0);
    
    // Calculate actual voltages relative to ground
    let vin_actual = vin - gnd;
    let vout_actual = vout - gnd;
    
    // Extract parameters from component
    let vout_nom = component.get_parameter("vout_nominal").unwrap_or(5.0);
    let dropout = component.get_parameter("dropout").unwrap_or(2.0);
    let vin_max = component.get_parameter("vin_max").unwrap_or(35.0);
    let vin_min = vout_nom + dropout;
    let iout_max = component.get_parameter("iout_max").unwrap_or(1.0);
    let power_max = component.get_parameter("power_max").unwrap_or(15.0);
    let tj_max = component.get_parameter("tj_max").unwrap_or(125.0);
    let rth_ja = component.get_parameter("rth_ja").unwrap_or(65.0);
    
    // Check 1: Input voltage range
    if vin_actual < vin_min {
        violations.push(SafetyViolation {
            rule_name: "Voltage Regulator Input Too Low".to_string(),
            severity: if vin_actual < vin_min - 0.5 {
                Severity::Error
            } else {
                Severity::Warning
            },
            location: CircuitLocation {
                nodes: vec![vin_node, vout_node, gnd_node],
                components: vec![comp_id],
                nets: vec![],
                description: format!("{} voltage regulator", component.name()),
            },
            message: format!(
                "{} requires minimum {}V input for {}V output ({}V dropout), but input is only {}V",
                component.name(), vin_min, vout_nom, dropout, vin_actual
            ),
            technical_details: format!(
                "Vin={:.1}V, Vin_min={:.1}V, Vout_nom={:.1}V, Dropout={:.1}V",
                vin_actual, vin_min, vout_nom, dropout
            ),
            user_impact: "Regulator cannot maintain stable output voltage".to_string(),
            estimated_damage: None,
        });
    }
    
    if vin_actual > vin_max {
        violations.push(SafetyViolation {
            rule_name: "Voltage Regulator Input Exceeds Maximum".to_string(),
            severity: Severity::Critical,
            location: CircuitLocation {
                nodes: vec![vin_node, vout_node, gnd_node],
                components: vec![comp_id],
                nets: vec![],
                description: format!("{} voltage regulator", component.name()),
            },
            message: format!(
                "{} has maximum input voltage of {}V, but input is {}V",
                component.name(), vin_max, vin_actual
            ),
            technical_details: format!(
                "Vin={:.1}V, Vin_max={:.1}V",
                vin_actual, vin_max
            ),
            user_impact: "Regulator will be damaged by excessive input voltage".to_string(),
            estimated_damage: Some(DamageEstimate {
                failure_mode: "Thermal runaway and internal breakdown".to_string(),
                time_to_failure: Some(Duration::from_secs(1)),
                affected_components: vec![component.name().to_string()],
                estimated_cost: Some(5.0),
            }),
        });
    }
    
    // Check 2: Output current
    let iout = component.current.unwrap_or(0.0).abs();
    if iout > iout_max {
        violations.push(SafetyViolation {
            rule_name: "Voltage Regulator Current Exceeds Rating".to_string(),
            severity: if iout > iout_max * 1.2 {
                Severity::Error
            } else {
                Severity::Warning
            },
            location: CircuitLocation {
                nodes: vec![vin_node, vout_node, gnd_node],
                components: vec![comp_id],
                nets: vec![],
                description: format!("{} voltage regulator", component.name()),
            },
            message: format!(
                "{} is rated for {}A maximum, but output current is {}A",
                component.name(), iout_max, iout
            ),
            technical_details: format!(
                "Iout={:.3}A, Iout_max={:.3}A, Overload={:.0}%",
                iout, iout_max, (iout / iout_max - 1.0) * 100.0
            ),
            user_impact: "Regulator may overheat and shut down".to_string(),
            estimated_damage: if iout > iout_max * 1.5 {
                Some(DamageEstimate {
                    failure_mode: "Thermal damage".to_string(),
                    time_to_failure: Some(Duration::from_secs(60)),
                    affected_components: vec![component.name().to_string()],
                    estimated_cost: Some(5.0),
                })
            } else {
                None
            },
        });
    }
    
    // Check 3: Power dissipation
    let iq = component.get_parameter("iq").unwrap_or(0.005);
    let power = (vin_actual - vout_actual) * iout + vin_actual * iq;
    
    if power > power_max {
        violations.push(SafetyViolation {
            rule_name: "Voltage Regulator Power Exceeds Rating".to_string(),
            severity: Severity::Error,
            location: CircuitLocation {
                nodes: vec![vin_node, vout_node, gnd_node],
                components: vec![comp_id],
                nets: vec![],
                description: format!("{} voltage regulator", component.name()),
            },
            message: format!(
                "{} dissipating {:.1}W, exceeds {:.1}W maximum",
                component.name(), power, power_max
            ),
            technical_details: format!(
                "Power={:.1}W, Power_max={:.1}W, Vin={:.1}V, Vout={:.1}V, Iout={:.3}A",
                power, power_max, vin_actual, vout_actual, iout
            ),
            user_impact: "Regulator will overheat without proper cooling".to_string(),
            estimated_damage: Some(DamageEstimate {
                failure_mode: "Thermal shutdown or damage".to_string(),
                time_to_failure: Some(Duration::from_secs(300)),
                affected_components: vec![component.name().to_string()],
                estimated_cost: Some(5.0),
            }),
        });
    }
    
    // Check 4: Junction temperature
    let tamb = 25.0; // Assume room temperature
    let tj = tamb + power * rth_ja;
    
    if tj > tj_max {
        violations.push(SafetyViolation {
            rule_name: "Voltage Regulator Junction Temperature Exceeded".to_string(),
            severity: Severity::Critical,
            location: CircuitLocation {
                nodes: vec![vin_node, vout_node, gnd_node],
                components: vec![comp_id],
                nets: vec![],
                description: format!("{} voltage regulator", component.name()),
            },
            message: format!(
                "{} junction temperature {:.0}°C exceeds {:.0}°C maximum",
                component.name(), tj, tj_max
            ),
            technical_details: format!(
                "Tj={:.0}°C, Tj_max={:.0}°C, Power={:.1}W, Rth_ja={:.1}°C/W, Required_Rth<{:.1}°C/W",
                tj, tj_max, power, rth_ja, (tj_max - tamb) / power
            ),
            user_impact: "Regulator will fail due to excessive temperature".to_string(),
            estimated_damage: Some(DamageEstimate {
                failure_mode: "Permanent thermal damage".to_string(),
                time_to_failure: Some(Duration::from_secs(10)),
                affected_components: vec![component.name().to_string()],
                estimated_cost: Some(5.0),
            }),
        });
    } else if tj > tj_max - 25.0 {
        violations.push(SafetyViolation {
            rule_name: "Voltage Regulator Running Hot".to_string(),
            severity: Severity::Warning,
            location: CircuitLocation {
                nodes: vec![vin_node, vout_node, gnd_node],
                components: vec![comp_id],
                nets: vec![],
                description: format!("{} voltage regulator", component.name()),
            },
            message: format!(
                "{} junction temperature {:.0}°C approaching {:.0}°C limit",
                component.name(), tj, tj_max
            ),
            technical_details: format!(
                "Tj={:.0}°C, Tj_max={:.0}°C, Margin={:.0}°C",
                tj, tj_max, tj_max - tj
            ),
            user_impact: "Regulator reliability may be reduced".to_string(),
            estimated_damage: None,
        });
    }
    
    // Check 5: Output capacitor presence (for stability)
    let has_output_cap = check_output_capacitor(circuit, vout_node.index(), gnd_node.index());
    if !has_output_cap {
        violations.push(SafetyViolation {
            rule_name: "Voltage Regulator Missing Output Capacitor".to_string(),
            severity: Severity::Warning,
            location: CircuitLocation {
                nodes: vec![vout_node, gnd_node],
                components: vec![comp_id],
                nets: vec![],
                description: format!("{} output", component.name()),
            },
            message: format!(
                "{} requires output capacitor for stability",
                component.name()
            ),
            technical_details: "Linear regulators require output capacitance for loop stability".to_string(),
            user_impact: "Regulator may oscillate or have poor transient response".to_string(),
            estimated_damage: None,
        });
    }
    
    // Check 6: For adjustable regulators, check minimum load
    if let Some(iload_min) = component.get_parameter("iload_min") {
        if iload_min > 0.0 && iout < iload_min {
            violations.push(SafetyViolation {
                rule_name: "Adjustable Regulator Below Minimum Load".to_string(),
                severity: Severity::Warning,
                location: CircuitLocation {
                    nodes: vec![vout_node, gnd_node],
                    components: vec![comp_id],
                    nets: vec![],
                    description: format!("{} output", component.name()),
                },
                message: format!(
                    "{} requires minimum {}mA load, but only {}mA detected",
                    component.name(), iload_min * 1000.0, iout * 1000.0
                ),
                technical_details: format!(
                    "Iout={:.3}A, Iload_min={:.3}A, Suggested bleeder={}Ω",
                    iout, iload_min, (vout_nom / iload_min) as i32
                ),
                user_impact: "Regulator output may drift or become unstable".to_string(),
                estimated_damage: None,
            });
        }
    }
    
    violations
}

/// Check if there's a capacitor between two nodes
fn check_output_capacitor(circuit: &Circuit, node1: usize, node2: usize) -> bool {
    use crate::circuit::NodeId;
    
    // Look for capacitors connected between the two nodes
    let node1_id = NodeId::new(node1);
    let node2_id = NodeId::new(node2);
    
    for (_, component) in circuit.branches() {
        let nodes = component.nodes();
        if component.component_type() == "Capacitor" {
            if (nodes.contains(&node1_id) && nodes.contains(&node2_id)) {
                return true;
            }
        }
    }
    false
}

/// Check thermal derating for ICs
pub fn calculate_thermal_derating(
    power: f64,
    rth_ja: f64,
    tamb: f64,
    tj_max: f64,
) -> f64 {
    let tj = tamb + power * rth_ja;
    let margin = tj_max - tj;
    
    if margin <= 0.0 {
        0.0 // No safe operating area
    } else if margin < 25.0 {
        margin / 25.0 // Linear derating in last 25°C
    } else {
        1.0 // Full rating available
    }
}

/// Suggest heatsink requirements
pub fn suggest_heatsink(
    power: f64,
    tj_max: f64,
    tamb: f64,
    rth_ja_package: f64,
) -> Option<f64> {
    let required_rth = (tj_max - 25.0 - tamb) / power; // 25°C margin
    
    if required_rth < rth_ja_package {
        Some(required_rth)
    } else {
        None // No heatsink needed
    }
}