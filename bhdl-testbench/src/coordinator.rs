//! Testbench coordinator that runs simulations with existing engines

use std::collections::HashMap;
use std::path::Path;
use bhdl_netlist::{Netlist, ConnectionPoint, NetId};
use bhdl_spice::{Circuit, AdaptiveCircuitSolver, ComponentModel, ElectricalLimits};
use bhdl_sim::{SimulationCoordinator, SimulationContext};
use bhdl_analyzer::flow_tracking::FlowTracker;
use bhdl_stdlib::{StdlibReader, get_default_stdlib_path};

use crate::{
    Result, TestbenchError, SignalRef, 
    testbench::{Testbench, SolverType},
    waveform::{WaveformCapture, WaveformFormat},
    stimulus::StimulusGenerator,
    verification::VerificationEngine,
    fault_injection::{FaultInjector, FaultScenario, FaultAnalysisResult, StressViolation},
};

/// Main testbench runner
pub struct TestbenchRunner {
    testbench: Testbench,
    netlist: Netlist,
    waveform_capture: WaveformCapture,
    stimulus_gen: StimulusGenerator,
    verification: VerificationEngine,
    fault_injector: FaultInjector,
    
    // Signal value storage
    signal_values: HashMap<SignalRef, f64>,
    
    // Simulation engines
    spice_solver: Option<SpiceSolverWrapper>,
    behavioral_coordinator: Option<SimulationCoordinator>,
    
    // Fault injection state
    active_fault_scenario: Option<String>,
}

struct SpiceSolverWrapper {
    circuit: Circuit,
    solver: AdaptiveCircuitSolver,
    signal_mapping: HashMap<SignalRef, NodeMapping>,
}

#[derive(Debug)]
enum NodeMapping {
    Voltage(String), // Node name
    Current(String), // Branch name
}

impl TestbenchRunner {
    pub fn new(
        testbench: Testbench,
        netlist: Netlist,
        flow_tracker: Option<FlowTracker>,
    ) -> Result<Self> {
        Self::new_with_analysis(testbench, netlist, flow_tracker, None)
    }
    
    pub fn new_with_analysis(
        testbench: Testbench,
        netlist: Netlist,
        flow_tracker: Option<FlowTracker>,
        analysis_result: Option<bhdl_analyzer::AnalysisResult>,
    ) -> Result<Self> {
        // Create waveform capture
        let waveform_capture = WaveformCapture::new(&testbench.scopes)?;
        
        // Create stimulus generator
        let stimulus_gen = StimulusGenerator::new(&testbench.stimuli);
        
        // Create verification engine
        let verification = VerificationEngine::new(
            &testbench.assertions,
            &testbench.measurements,
        )?;
        
        // Create appropriate simulation engine
        let (spice_solver, behavioral_coordinator) = match &testbench.simulation_config.solver_type {
            SolverType::SpiceAdaptive | SolverType::SpiceFixed => {
                let wrapper = Self::create_spice_solver(&netlist, analysis_result.as_ref())?;
                (Some(wrapper), None)
            }
            SolverType::Behavioral => {
                let coordinator = Self::create_behavioral_coordinator(&netlist, flow_tracker)?;
                (None, Some(coordinator))
            }
            SolverType::MixedSignal { .. } => {
                let spice = Self::create_spice_solver(&netlist, analysis_result.as_ref())?;
                let behavioral = Self::create_behavioral_coordinator(&netlist, flow_tracker)?;
                (Some(spice), Some(behavioral))
            }
        };
        
        // Initialize fault injector with standard scenarios
        let mut fault_injector = FaultInjector::new();
        for scenario in FaultInjector::create_standard_scenarios() {
            fault_injector.add_scenario(scenario);
        }
        
        Ok(Self {
            testbench,
            netlist,
            waveform_capture,
            stimulus_gen,
            verification,
            fault_injector,
            signal_values: HashMap::new(),
            spice_solver,
            behavioral_coordinator,
            active_fault_scenario: None,
        })
    }
    
    fn create_spice_solver(netlist: &Netlist, analysis_result: Option<&bhdl_analyzer::AnalysisResult>) -> Result<SpiceSolverWrapper> {
        // Convert netlist to SPICE circuit
        let mut circuit = Circuit::new();
        let mut signal_mapping = HashMap::new();
        
        // Load stdlib components
        let mut stdlib_reader = StdlibReader::new(get_default_stdlib_path());
        stdlib_reader.load_all_components()
            .map_err(|e| TestbenchError::ConfigError(format!("Failed to load stdlib: {}", e)))?;
        
        // First, ensure we have a ground node
        let mut has_ground = false;
        
        // Check analysis data for power and ground symbols
        if let Some(analysis_data) = &netlist.analysis_data {
            for (symbol_name, symbol_info) in &analysis_data.symbol_data {
                match symbol_info.symbol_type {
                    bhdl_common::analysis_interface::SymbolType::Ground => {
                        // Add ONLY the SPICE ground node (SPICE requires node "0" for ground)
                        circuit.add_node("0".to_string(), None);
                        has_ground = true;
                        
                        // Map BHDL ground signal reference to SPICE ground node
                        signal_mapping.insert(
                            SignalRef::Net(symbol_name.clone()),
                            NodeMapping::Voltage("0".to_string()),
                        );
                    }
                    bhdl_common::analysis_interface::SymbolType::Power => {
                        // We'll add voltage sources for power nodes later
                        ()
                    }
                    _ => ()
                }
            }
        }
        
        // If no ground found, create a default one
        if !has_ground {
            circuit.add_node("0".to_string(), None);
        }
        
        // Add nodes
        for (net_id, net) in &netlist.nets {
            let node_name = net.name.clone().unwrap_or_else(|| format!("net_{:?}", net_id));
            
            // Check if this is a ground net from power analysis
            let is_ground_net = if let Some(analysis_result) = analysis_result {
                let is_ground = analysis_result.power_analysis.domains.get(&node_name)
                    .map(|domain| domain.voltage <= 0.0)
                    .unwrap_or(false);
                if node_name.contains("GND") || node_name.contains("gnd") {
                    println!("DEBUG: Checking if '{}' is ground net: {}", node_name, is_ground);
                    if !is_ground {
                        println!("DEBUG: Available domains: {:?}", analysis_result.power_analysis.domains.keys().collect::<Vec<_>>());
                    }
                }
                is_ground
            } else {
                false
            };
            
            // Skip if already added as ground or if it's a ground domain
            if node_name == "0" || is_ground_net {
                // Map ground nets to SPICE ground node
                if is_ground_net {
                    if let Some(name) = &net.name {
                        // Add mapping with @ prefix for consistency with testbench
                        let signal_ref = SignalRef::Net(format!("@{}", name));
                        if !signal_mapping.contains_key(&signal_ref) {
                            signal_mapping.insert(
                                signal_ref,
                                NodeMapping::Voltage("0".to_string()),
                            );
                        }
                        
                        // Also add without @ prefix for backward compatibility
                        if !signal_mapping.contains_key(&SignalRef::Net(name.clone())) {
                            signal_mapping.insert(
                                SignalRef::Net(name.clone()),
                                NodeMapping::Voltage("0".to_string()),
                            );
                        }
                    }
                }
                continue;
            }
            
            circuit.add_node(node_name.clone(), None);
            
            // Map net signal reference
            if let Some(name) = &net.name {
                // Add mapping with @ prefix for consistency with testbench
                let signal_ref = SignalRef::Net(format!("@{}", name));
                if !signal_mapping.contains_key(&signal_ref) {
                    signal_mapping.insert(
                        signal_ref,
                        NodeMapping::Voltage(node_name.clone()),
                    );
                }
                
                // Also add without @ prefix for backward compatibility
                if !signal_mapping.contains_key(&SignalRef::Net(name.clone())) {
                    signal_mapping.insert(
                        SignalRef::Net(name.clone()),
                        NodeMapping::Voltage(node_name.clone()),
                    );
                }
            }
        }
        
        // Add components as branches
        println!("Processing {} instances", netlist.instances.len());
        for (inst_id, instance) in &netlist.instances {
            // Skip power and ground instances - they're handled as voltage sources
            if let Some(module) = netlist.modules.get(instance.definition) {
                if module.name == "Power" || module.name == "Ground" {
                    println!("  Skipping power/ground instance: {}", instance.name);
                    continue;
                }
            }
            let comp_name = instance.name.clone();
            println!("  Instance: {} (type: {:?})", comp_name, instance.definition);
            
            // Get the module definition
            if let Some(module) = netlist.modules.get(instance.definition) {
                println!("    Module found: {}", module.name);
                // Find all nets connected to this instance
                // We need to look for PinInstance connections, not InstancePort
                let mut connections = Vec::new();
                
                // Find all pin instances for this component instance
                for (pin_inst_id, pin_inst) in &netlist.pin_instances {
                    if pin_inst.instance == inst_id {
                        // Find which net this pin is connected to
                        if let Some(net_id) = pin_inst.net {
                            if let Some(pin) = netlist.pins.get(pin_inst.pin_def) {
                                connections.push((pin.name.clone(), net_id));
                            }
                        }
                    }
                }
                
                println!("    Connections found: {}", connections.len());
                
                // For two-terminal components (resistors, capacitors, etc.)
                if connections.len() >= 2 {
                    println!("    Processing as two-terminal component");
                    // Get the first two connections
                    let (_pin1_name, net1_id) = connections[0].clone();
                    let (_pin2_name, net2_id) = &connections[1];
                    
                    // Get net names - mapping ground nets to SPICE ground "0"
                    let get_spice_node_name = |net_id: &NetId| -> String {
                        if let Some(net) = netlist.get_net(*net_id) {
                            let net_name = net.name.clone().unwrap_or_else(|| format!("net_{:?}", net_id));
                            
                            // Check if this is a ground net
                            if net_name == "GND" || net_name == "gnd" {
                                return "0".to_string();
                            }
                            
                            // Also check power analysis for ground domains
                            if let Some(analysis_result) = analysis_result {
                                if let Some(domain) = analysis_result.power_analysis.domains.get(&net_name) {
                                    if domain.voltage <= 0.0 {
                                        return "0".to_string();
                                    }
                                }
                            }
                            
                            net_name
                        } else {
                            format!("net_{:?}", net_id)
                        }
                    };
                    
                    let node1 = get_spice_node_name(&net1_id);
                    let node2 = get_spice_node_name(net2_id);
                    
                    // Create component model based on module type
                    let model = if let Some(component_def) = stdlib_reader.get_component(&module.name) {
                        match component_def.module_name.as_str() {
                            "Resistor" | "Res" => {
                                let value = instance.attributes.get("value")
                                    .and_then(|v| parse_value_with_units(v))
                                    .unwrap_or_else(|| {
                                        // Try to get from stdlib attributes
                                        component_def.attributes.get("resistance")
                                            .and_then(|v| parse_value_with_units(v))
                                            .unwrap_or(1000.0)
                                    });
                                
                                let tolerance = component_def.attributes.get("tolerance")
                                    .and_then(|v| parse_value_with_units(v))
                                    .unwrap_or(5.0); // 5% default
                                
                                let max_power = component_def.attributes.get("max_power")
                                    .and_then(|v| parse_value_with_units(v));
                                
                                ComponentModel::Resistor { 
                                    resistance: value,
                                    tolerance,
                                    limits: ElectricalLimits {
                                        max_power,
                                        ..Default::default()
                                    }
                                }
                            }
                            "Capacitor" | "Cap" => {
                                let value = instance.attributes.get("value")
                                    .and_then(|v| parse_value_with_units(v))
                                    .unwrap_or_else(|| {
                                        component_def.attributes.get("capacitance")
                                            .and_then(|v| parse_value_with_units(v))
                                            .unwrap_or(1e-6)
                                    });
                                
                                let esr = component_def.attributes.get("esr")
                                    .and_then(|v| parse_value_with_units(v));
                                
                                let max_voltage = component_def.attributes.get("max_voltage")
                                    .and_then(|v| parse_value_with_units(v));
                                
                                ComponentModel::Capacitor { 
                                    capacitance: value,
                                    esr,
                                    limits: ElectricalLimits {
                                        max_voltage,
                                        ..Default::default()
                                    }
                                }
                            }
                            "LED" => {
                                // Get LED color from instance attributes
                                let color = instance.attributes.get("color")
                                    .cloned()
                                    .unwrap_or_else(|| "red".to_string());
                                
                                let forward_voltage = component_def.attributes.get("forward_voltage")
                                    .and_then(|v| parse_value_with_units(v))
                                    .unwrap_or(2.0);
                                
                                let forward_current = component_def.attributes.get("forward_current")
                                    .and_then(|v| parse_value_with_units(v))
                                    .unwrap_or(0.02); // 20mA
                                
                                let dynamic_resistance = component_def.attributes.get("dynamic_resistance")
                                    .and_then(|v| parse_value_with_units(v))
                                    .unwrap_or(10.0);
                                
                                let max_current = component_def.attributes.get("max_current")
                                    .and_then(|v| parse_value_with_units(v));
                                
                                ComponentModel::LED {
                                    color,
                                    forward_voltage,
                                    forward_current,
                                    dynamic_resistance,
                                    saturation_current: Some(3.96e-19),  // Typical value for LED
                                    emission_coefficient: Some(1.8),      // Typical LED ideality factor
                                    thermal_voltage: Some(0.026),         // Room temperature Vt
                                    limits: ElectricalLimits {
                                        max_current,
                                        ..Default::default()
                                    }
                                }
                            }
                            "Diode" => {
                                let forward_voltage = component_def.attributes.get("forward_voltage")
                                    .and_then(|v| parse_value_with_units(v))
                                    .unwrap_or(0.7);
                                
                                let forward_resistance = component_def.attributes.get("dynamic_resistance")
                                    .and_then(|v| parse_value_with_units(v))
                                    .unwrap_or(1.0);
                                
                                let reverse_current = component_def.attributes.get("reverse_current")
                                    .and_then(|v| parse_value_with_units(v))
                                    .unwrap_or(1e-9);
                                
                                let saturation_current = component_def.attributes.get("spice_is")
                                    .and_then(|v| v.parse::<f64>().ok());
                                
                                let emission_coefficient = component_def.attributes.get("spice_n")
                                    .and_then(|v| v.parse::<f64>().ok());
                                
                                let max_reverse_voltage = component_def.attributes.get("max_reverse_voltage")
                                    .and_then(|v| parse_value_with_units(v));
                                
                                ComponentModel::Diode {
                                    forward_voltage,
                                    forward_resistance,
                                    reverse_current,
                                    saturation_current,
                                    emission_coefficient,
                                    limits: ElectricalLimits {
                                        max_voltage: max_reverse_voltage,
                                        ..Default::default()
                                    }
                                }
                            }
                            _ => {
                                // Check if it has SPICE model attributes
                                if component_def.attributes.contains_key("spice_model") {
                                    // TODO: Create appropriate model based on spice_type
                                    ComponentModel::Resistor { 
                                        resistance: 1000.0,
                                        tolerance: 5.0,
                                        limits: ElectricalLimits::default()
                                    }
                                } else {
                                    // Default to resistor
                                    ComponentModel::Resistor { 
                                        resistance: 1000.0,
                                        tolerance: 5.0,
                                        limits: ElectricalLimits::default()
                                    }
                                }
                            }
                        }
                    } else {
                        // Component not in stdlib, make educated guess based on name
                        if module.name.contains("Res") {
                            let value = instance.attributes.get("value")
                                .and_then(|v| parse_value_with_units(v))
                                .unwrap_or(1000.0);
                            ComponentModel::Resistor { 
                                resistance: value,
                                tolerance: 5.0,
                                limits: ElectricalLimits::default()
                            }
                        } else if module.name.contains("Cap") {
                            let value = instance.attributes.get("value")
                                .and_then(|v| parse_value_with_units(v))
                                .unwrap_or(1e-6);
                            ComponentModel::Capacitor { 
                                capacitance: value,
                                esr: None,
                                limits: ElectricalLimits::default()
                            }
                        } else {
                            ComponentModel::Resistor { 
                                resistance: 1000.0,
                                tolerance: 5.0,
                                limits: ElectricalLimits::default()
                            }
                        }
                    };
                    
                    // Add branch
                    let component_type = match &model {
                        ComponentModel::Resistor { .. } => "Resistor",
                        ComponentModel::Capacitor { .. } => "Capacitor",
                        ComponentModel::Inductor { .. } => "Inductor",
                        ComponentModel::Diode { .. } => "Diode",
                        ComponentModel::LED { .. } => "LED",
                        _ => "Unknown",
                    };
                    
                    let value = match &model {
                        ComponentModel::Resistor { resistance, .. } => *resistance,
                        ComponentModel::Capacitor { capacitance, .. } => *capacitance,
                        ComponentModel::Inductor { inductance, .. } => *inductance,
                        _ => 0.0,
                    };
                    
                    let _branch_idx = circuit.add_branch(
                        comp_name.clone(),
                        &node1,
                        &node2,
                        component_type.to_string(),
                        value,
                        Some(inst_id),
                    );
                    
                    // TODO: Component models need to be set on the solver, not the circuit
                    
                    // Map current signal reference
                    signal_mapping.insert(
                        SignalRef::Current(instance.name.clone()),
                        NodeMapping::Current(comp_name),
                    );
                }
            }
        }
        
        // Add voltage sources for power nodes
        if let Some(analysis_data) = &netlist.analysis_data {
            let mut vsource_counter = 0;
            for (symbol_name, symbol_info) in &analysis_data.symbol_data {
                if matches!(symbol_info.symbol_type, bhdl_common::analysis_interface::SymbolType::Power) {
                    // Extract voltage from parameters
                    let voltage = symbol_info.parameters.get("voltage")
                        .and_then(|v| parse_value_with_units(v))
                        .unwrap_or(5.0); // Default to 5V if not specified
                    
                    // Create voltage source from power node to ground
                    let vsource_name = format!("V{}", vsource_counter);
                    vsource_counter += 1;
                    
                    let _branch_idx = circuit.add_branch(
                        vsource_name.clone(),
                        symbol_name,  // positive node (power)
                        "0",          // negative node (ground)
                        "VoltageSource".to_string(),
                        voltage,
                        None,
                    );
                    
                    // Map the voltage source current (for monitoring)
                    signal_mapping.insert(
                        SignalRef::Current(vsource_name.clone()),
                        NodeMapping::Current(vsource_name),
                    );
                }
            }
        }
        
        // Debug: Print circuit information
        println!("=== SPICE Circuit Debug ===");
        println!("Netlist nets: {}", netlist.nets.len());
        for (net_id, net) in &netlist.nets {
            println!("  Net {:?}: {} - connections: {}", 
                net_id, net.name.as_ref().unwrap_or(&"unnamed".to_string()), net.connections.len());
            for conn in &net.connections {
                match conn {
                    ConnectionPoint::PinInstance(pin_inst_id) => {
                        if let Some(pin_inst) = netlist.pin_instances.get(*pin_inst_id) {
                            if let Some(inst) = netlist.instances.get(pin_inst.instance) {
                                if let Some(pin) = netlist.pins.get(pin_inst.pin_def) {
                                    println!("    -> PinInstance: {}.{}", inst.name, pin.name);
                                }
                            }
                        }
                    }
                    ConnectionPoint::InstancePort(inst_id, port_id) => {
                        println!("    -> InstancePort: {:?}.{:?}", inst_id, port_id);
                    }
                    _ => {
                        println!("    -> Other: {:?}", conn);
                    }
                }
            }
        }
        println!("Circuit nodes: {}", circuit.nodes().count());
        for (idx, node) in circuit.nodes() {
            println!("  Node {:?}: {} (ground: {})", idx, node.name, node.is_ground);
        }
        println!("Circuit branches: {}", circuit.branches().count());
        for (idx, branch) in circuit.branches() {
            println!("  Branch {:?}: {} - Type: {}, Value: {}", 
                idx, branch.name, branch.component_type, branch.value);
        }
        
        // Add voltage source models for power supplies - use analyzer's power domains
        // IMPORTANT: Add voltage sources to circuit BEFORE creating solver
        if let Some(analysis_result) = analysis_result {
            println!("DEBUG: Processing power analysis with {} domains", analysis_result.power_analysis.domains.len());
            for (domain_name, domain) in &analysis_result.power_analysis.domains {
                println!("  Domain: {} - Voltage: {}V @ {}A", domain_name, domain.voltage, domain.max_current);
            }
            
            let mut vsource_counter = 0;
            for (domain_name, domain) in &analysis_result.power_analysis.domains {
                // Skip ground domains (0V)
                if domain.voltage > 0.0 {
                    let vsource_name = format!("V{}", vsource_counter);
                    
                    // Add the circuit branch first
                    circuit.add_branch(
                        vsource_name.clone(),
                        domain_name, // VCC node
                        "0",         // Ground node
                        "VoltageSource".to_string(),
                        domain.voltage,
                        None,
                    );
                    println!("  Added VoltageSource branch: {} ({} -> 0) = {}V", vsource_name, domain_name, domain.voltage);
                    vsource_counter += 1;
                }
            }
        }
        
        // Debug: Print circuit branches after adding voltage sources
        println!("Circuit branches after voltage sources: {}", circuit.branches().count());
        for (idx, branch) in circuit.branches() {
            println!("  Branch {:?}: {} - Type: {}, Value: {}", 
                idx, branch.name, branch.component_type, branch.value);
        }
        
        // Create solver AFTER adding all voltage sources to circuit
        let mut solver = AdaptiveCircuitSolver::new(circuit.clone());
        
        // Add component models based on module types
        println!("Adding component models to solver...");
        
        // Add voltage source component models to solver
        if let Some(analysis_result) = analysis_result {
            let mut vsource_counter = 0;
            for (domain_name, domain) in &analysis_result.power_analysis.domains {
                // Skip ground domains (0V)
                if domain.voltage > 0.0 {
                    let vsource_name = format!("V{}", vsource_counter);
                    
                    // Add the component model to the solver
                    solver.add_model(vsource_name.clone(), ComponentModel::VoltageSource {
                        voltage: domain.voltage,
                        internal_resistance: Some(0.1), // Small internal resistance
                    });
                    println!("  Added VoltageSource model: {} = {}V", vsource_name, domain.voltage);
                    vsource_counter += 1;
                }
            }
        }
        
        // Add models for regular components
        for (inst_id, instance) in &netlist.instances {
            let comp_name = instance.name.clone();
            
            println!("DEBUG: Instance '{}' attributes: {:?}", comp_name, instance.attributes);
            
            if let Some(module) = netlist.modules.get(instance.definition) {
                let model = if let Some(component_def) = stdlib_reader.get_component(&module.name) {
                    match component_def.module_name.as_str() {
                        "Resistor" | "Res" => {
                            let resistance = instance.attributes.get("value")
                                .and_then(|v| parse_value_with_units(v))
                                .unwrap_or_else(|| {
                                    component_def.attributes.get("resistance")
                                        .and_then(|v| parse_value_with_units(v))
                                        .unwrap_or(1000.0)
                                });
                            
                            let tolerance = component_def.attributes.get("tolerance")
                                .and_then(|v| parse_value_with_units(v))
                                .unwrap_or(5.0);
                            
                            Some(ComponentModel::Resistor { 
                                resistance,
                                tolerance,
                                limits: ElectricalLimits::default()
                            })
                        }
                        "LED" => {
                            let color = instance.attributes.get("color")
                                .cloned()
                                .unwrap_or_else(|| "red".to_string());
                            
                            let forward_voltage = component_def.attributes.get("forward_voltage")
                                .and_then(|v| parse_value_with_units(v))
                                .unwrap_or(2.0);
                            
                            let forward_current = component_def.attributes.get("forward_current")
                                .and_then(|v| parse_value_with_units(v))
                                .unwrap_or(0.02);
                            
                            let dynamic_resistance = component_def.attributes.get("dynamic_resistance")
                                .and_then(|v| parse_value_with_units(v))
                                .unwrap_or(10.0);
                            
                            println!("DEBUG: LED model parameters: color={}, forward_voltage={}, forward_current={}, dynamic_resistance={}", 
                                     color, forward_voltage, forward_current, dynamic_resistance);
                            
                            Some(ComponentModel::LED {
                                color,
                                forward_voltage,
                                forward_current,
                                dynamic_resistance,
                                saturation_current: Some(3.96e-19),  // Typical value for LED
                                emission_coefficient: Some(1.8),      // Typical LED ideality factor
                                thermal_voltage: Some(0.026),         // Room temperature Vt
                                limits: ElectricalLimits::default()
                            })
                        }
                        "Capacitor" | "Cap" => {
                            let capacitance = instance.attributes.get("value")
                                .and_then(|v| parse_value_with_units(v))
                                .unwrap_or_else(|| {
                                    component_def.attributes.get("capacitance")
                                        .and_then(|v| parse_value_with_units(v))
                                        .unwrap_or(1e-6)
                                });
                            
                            let esr = component_def.attributes.get("esr")
                                .and_then(|v| parse_value_with_units(v));
                            
                            Some(ComponentModel::Capacitor { 
                                capacitance,
                                esr,
                                limits: ElectricalLimits::default()
                            })
                        }
                        _ => None
                    }
                } else {
                    // Fallback for components not in stdlib
                    if module.name.contains("Res") {
                        let resistance = instance.attributes.get("value")
                            .and_then(|v| parse_value_with_units(v))
                            .unwrap_or(1000.0);
                        Some(ComponentModel::Resistor { 
                            resistance,
                            tolerance: 5.0,
                            limits: ElectricalLimits::default()
                        })
                    } else if module.name.contains("LED") {
                        Some(ComponentModel::LED {
                            color: "red".to_string(),
                            forward_voltage: 2.0,
                            forward_current: 0.02,
                            dynamic_resistance: 10.0,
                            saturation_current: Some(3.96e-19),  // Typical value for LED
                            emission_coefficient: Some(1.8),      // Typical LED ideality factor
                            thermal_voltage: Some(0.026),         // Room temperature Vt
                            limits: ElectricalLimits::default()
                        })
                    } else {
                        None
                    }
                };
                
                if let Some(model) = model {
                    solver.add_model(comp_name.clone(), model);
                    println!("  Added component model: {} ({})", comp_name, module.name);
                }
            }
        }
        
        // Set proper convergence parameters like the working standalone tests
        solver.set_convergence(100, 1e-6);
        println!("Set convergence: 100 iterations, 1e-6 tolerance");
        
        Ok(SpiceSolverWrapper {
            circuit,
            solver,
            signal_mapping,
        })
    }
    
    fn create_behavioral_coordinator(
        netlist: &Netlist,
        flow_tracker: Option<FlowTracker>,
    ) -> Result<SimulationCoordinator> {
        let flow_tracker = flow_tracker.ok_or_else(|| {
            TestbenchError::ConfigError("Flow tracker required for behavioral simulation".to_string())
        })?;
        
        // TODO: SimulationCoordinator needs ownership of netlist
        // For now, return an error as we can't clone the netlist
        Err(TestbenchError::ConfigError(
            "Behavioral simulation requires netlist ownership - not yet implemented".to_string()
        ))
    }
    
    pub fn add_waveform_output(&mut self, format: WaveformFormat, path: &Path) -> Result<()> {
        self.waveform_capture.add_writer(format, path)
    }
    
    pub fn run(&mut self) -> Result<TestbenchResults> {
        let duration = self.testbench.simulation_config.duration.as_seconds();
        let timestep = self.testbench.simulation_config.timestep.as_seconds();
        
        let mut current_time = 0.0;
        let mut violations = Vec::new();
        
        // Main simulation loop
        let mut step_count = 0;
        while current_time <= duration {
            step_count += 1;
            if step_count % 100 == 0 || step_count <= 10 {
                println!("Simulation step {} at time {:.6}s", step_count, current_time);
            }
            
            // Apply stimuli
            let stimuli = self.stimulus_gen.get_values(current_time);
            self.apply_stimuli(&stimuli)?;
            
            // Step simulation
            match &self.testbench.simulation_config.solver_type {
                SolverType::SpiceAdaptive | SolverType::SpiceFixed => {
                    self.step_spice(timestep)?;
                }
                SolverType::Behavioral => {
                    self.step_behavioral(current_time, timestep)?;
                }
                SolverType::MixedSignal { .. } => {
                    self.step_mixed_signal(current_time, timestep)?;
                }
            }
            
            // Capture waveforms
            self.waveform_capture.capture(current_time, &self.signal_values)?;
            
            // Check assertions
            let new_violations = self.verification.check(current_time, &self.signal_values)?;
            violations.extend(new_violations);
            
            // Update measurements
            self.verification.update_measurements(current_time, &self.signal_values)?;
            
            current_time += timestep;
        }
        
        // Finalize
        self.waveform_capture.finalize()?;
        let measurements = self.verification.get_final_measurements();
        
        Ok(TestbenchResults {
            passed: violations.is_empty(),
            violations,
            measurements,
            simulation_time: duration,
        })
    }
    
    fn apply_stimuli(&mut self, stimuli: &HashMap<SignalRef, f64>) -> Result<()> {
        // Apply to SPICE solver
        if let Some(spice) = &mut self.spice_solver {
            for (signal, value) in stimuli {
                // Find the corresponding voltage source in the circuit
                match signal {
                    SignalRef::Net(net_name) => {
                        // Find voltage source connected to this net
                        // Voltage sources are named V0, V1, etc. for power nets
                        let mut branch_to_update = None;
                        for (edge_idx, branch) in spice.circuit.branches() {
                            if branch.component_type == "VoltageSource" {
                                // Check if this source is connected to the target net
                                if let Some((n1, _n2)) = spice.circuit.branch_nodes(edge_idx) {
                                    if let Some(node1) = spice.circuit.get_node_by_id(n1) {
                                        if node1.name == *net_name || node1.name == format!("@{}", net_name) {
                                            branch_to_update = Some(branch.name.clone());
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        
                        // Update the voltage source value
                        if let Some(branch_name) = branch_to_update {
                            if let Some((_, branch_mut)) = spice.circuit.get_branch_mut(&branch_name) {
                                branch_mut.value = *value;
                            }
                        }
                    }
                    _ => {
                        // Other signal types not supported for stimulus yet
                    }
                }
            }
        }
        
        // Store in signal values
        for (signal, value) in stimuli {
            self.signal_values.insert(signal.clone(), *value);
        }
        
        Ok(())
    }
    
    fn step_spice(&mut self, _timestep: f64) -> Result<()> {
        if let Some(spice) = &mut self.spice_solver {
            // Debug: Print signal mapping (only first time)
            static mut FIRST_RUN: bool = true;
            unsafe {
                if FIRST_RUN {
                    println!("=== Signal Mapping Debug ===");
                    for (signal_ref, mapping) in &spice.signal_mapping {
                        println!("  {:?} -> {:?}", signal_ref, mapping);
                    }
                    FIRST_RUN = false;
                }
            }
            
            // Run SPICE analysis
            print!("Running SPICE analysis...");
            match spice.solver.analyze() {
                Ok(result) => {
                    println!(" SUCCESS");
                    println!("=== SPICE Analysis Results ===");
                    println!("Node voltages: {} entries", result.node_voltages.len());
                    for (node_idx, voltage) in &result.node_voltages {
                        if let Some(node_name) = spice.circuit.get_node_name(*node_idx) {
                            println!("  Node {} (idx {:?}): {:.6}V", node_name, node_idx, voltage);
                        }
                    }
                    println!("Branch currents: {} entries", result.branch_currents.len());
                    for (branch_idx, current) in &result.branch_currents {
                        // Find branch name
                        for (edge_idx, branch) in spice.circuit.branches() {
                            if edge_idx == *branch_idx {
                                println!("  Branch {} (idx {:?}): {:.6}A", branch.name, branch_idx, current);
                                break;
                            }
                        }
                    }
                    
                    // Create signal value map for this timestep
                    let mut signal_values = HashMap::new();
                    
                    // Extract node voltages
                    for (node_idx, voltage) in &result.node_voltages {
                        // Get node name from circuit
                        if let Some(node_name) = spice.circuit.get_node_name(*node_idx) {
                            // Find corresponding signal reference in mapping
                            for (signal_ref, mapping) in &spice.signal_mapping {
                                if let NodeMapping::Voltage(mapped_node) = mapping {
                                    if mapped_node == node_name {
                                        println!("  Mapping voltage: {:?} = {:.6}V", signal_ref, voltage);
                                        signal_values.insert(signal_ref.clone(), *voltage);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    
                    // Extract branch currents
                    for (branch_idx, current) in &result.branch_currents {
                        // Get branch from circuit
                        for (edge_idx, branch) in spice.circuit.branches() {
                            if edge_idx == *branch_idx {
                                let branch_name = &branch.name;
                                // Find corresponding signal reference in mapping
                                for (signal_ref, mapping) in &spice.signal_mapping {
                                    if let NodeMapping::Current(mapped_branch) = mapping {
                                        if mapped_branch == branch_name {
                                            println!("  Mapping current: {:?} = {:.6}A", signal_ref, current);
                                            signal_values.insert(signal_ref.clone(), *current);
                                            break;
                                        }
                                    }
                                }
                                break;
                            }
                        }
                    }
                    
                    println!("Signal values extracted: {} entries", signal_values.len());
                    for (signal_ref, value) in &signal_values {
                        println!("    {:?} = {:.6}", signal_ref, value);
                    }
                    
                    // Update signal values
                    self.signal_values.extend(signal_values);
                }
                Err(e) => return Err(TestbenchError::SpiceError(e)),
            }
        }
        Ok(())
    }
    
    fn step_behavioral(&mut self, current_time: f64, timestep: f64) -> Result<()> {
        if let Some(coordinator) = &mut self.behavioral_coordinator {
            let context = SimulationContext {
                start_time: current_time,
                end_time: current_time + timestep,
                time_step: timestep,
                debug: false,
            };
            
            match coordinator.simulate(&context) {
                Ok(_result) => {
                    // Extract signal values
                    // TODO: Implement signal extraction from behavioral sim
                }
                Err(e) => return Err(TestbenchError::Other(e.into())),
            }
        }
        Ok(())
    }
    
    fn step_mixed_signal(&mut self, current_time: f64, timestep: f64) -> Result<()> {
        // Step both simulators and synchronize
        self.step_spice(timestep)?;
        self.step_behavioral(current_time, timestep)?;
        
        // TODO: Implement synchronization between domains
        
        Ok(())
    }
    
    // ==================== Fault Injection Methods ====================
    
    /// Add a custom fault scenario
    pub fn add_fault_scenario(&mut self, scenario: FaultScenario) {
        self.fault_injector.add_scenario(scenario);
    }
    
    /// Run simulation with a fault scenario
    pub fn run_with_fault(&mut self, scenario_name: &str) -> Result<FaultAnalysisResult> {
        println!("=== Running Fault Scenario: {} ===", scenario_name);
        
        // Clone the scenario to avoid borrow issues
        let scenario = {
            self.fault_injector.load_scenario(scenario_name)
                .map_err(|e| TestbenchError::Other(e))?;
            
            // Get the scenario and clone it
            self.fault_injector.scenarios.get(scenario_name)
                .ok_or_else(|| TestbenchError::Other(anyhow::anyhow!("Failed to get scenario")))?
                .clone()
        };
        
        // Store baseline values by running without faults
        println!("Running baseline simulation...");
        self.active_fault_scenario = None;
        
        // Ensure fault injector has no active faults for baseline
        // Note: active_faults is cleared when load_scenario is called
        
        let _baseline_results = self.run()?;
        let baseline_values = self.signal_values.clone();
        
        println!("Baseline simulation complete. Signal count: {}", baseline_values.len());
        
        // Apply faults to SPICE models
        if let Some(spice) = &mut self.spice_solver {
            println!("Applying faults to SPICE models...");
            // First, make sure fault injector has loaded the scenario
            self.fault_injector.load_scenario(scenario_name)
                .map_err(|e| TestbenchError::Other(e))?;
            
            for (component_name, _) in &scenario.faults {
                // Apply fault to the solver's models
                if let Some(model) = spice.solver.get_model_mut(component_name) {
                    // Debug: print model before fault
                    println!("  Model {} before fault: {:?}", component_name, model);
                    
                    self.fault_injector.apply_to_component_model(component_name, model)
                        .map_err(|e| TestbenchError::Other(e))?;
                    
                    // Debug: print model after fault
                    println!("  Model {} after fault: {:?}", component_name, model);
                    println!("  Applied fault to {}", component_name);
                }
            }
        }
        
        // Run faulted simulation
        println!("Running faulted simulation...");
        self.active_fault_scenario = Some(scenario_name.to_string());
        let faulted_results = self.run()?;
        let faulted_values = self.signal_values.clone();
        
        // Analyze stress violations
        let stress_violations = self.analyze_stress_violations(&scenario, &faulted_values)?;
        
        // Check for cascade failures (simplified for now)
        let cascade_failures = Vec::new(); // TODO: Implement cascade detection
        
        // Check if protections triggered
        let protection_triggered = Vec::new(); // TODO: Implement protection detection
        
        // Determine if safety passed
        let safety_passed = stress_violations.is_empty() && 
                           faulted_results.violations.is_empty();
        
        Ok(FaultAnalysisResult {
            scenario_name: scenario_name.to_string(),
            baseline_values,
            faulted_values,
            stress_violations,
            cascade_failures,
            protection_triggered,
            safety_passed,
        })
    }
    
    /// Run multiple fault scenarios
    pub fn run_fault_campaign(&mut self, scenario_names: Vec<&str>) -> Result<Vec<FaultAnalysisResult>> {
        let mut results = Vec::new();
        
        for scenario_name in scenario_names {
            match self.run_with_fault(scenario_name) {
                Ok(result) => {
                    println!("\n{}", result.generate_report());
                    results.push(result);
                }
                Err(e) => {
                    eprintln!("Failed to run scenario '{}': {}", scenario_name, e);
                }
            }
        }
        
        Ok(results)
    }
    
    /// Analyze stress violations based on expected behavior
    fn analyze_stress_violations(
        &self,
        scenario: &FaultScenario,
        values: &HashMap<SignalRef, f64>,
    ) -> Result<Vec<StressViolation>> {
        let mut violations = Vec::new();
        
        if let Some(expected) = &scenario.expected_behavior {
            for (component, limits) in &expected.max_stress {
                // Check current limit
                if let Some(max_current) = limits.max_current {
                    let signal = SignalRef::Current(component.clone());
                    if let Some(&current) = values.get(&signal) {
                        if current.abs() > max_current {
                            violations.push(StressViolation {
                                component: component.clone(),
                                stress_type: "Current".to_string(),
                                actual_value: current.abs(),
                                limit_value: max_current,
                                severity: if current.abs() > max_current * 1.5 {
                                    "CRITICAL".to_string()
                                } else {
                                    "WARNING".to_string()
                                },
                            });
                        }
                    }
                }
                
                // Check voltage limit
                if let Some(max_voltage) = limits.max_voltage {
                    let signal = SignalRef::Voltage(component.clone());
                    if let Some(&voltage) = values.get(&signal) {
                        if voltage.abs() > max_voltage {
                            violations.push(StressViolation {
                                component: component.clone(),
                                stress_type: "Voltage".to_string(),
                                actual_value: voltage.abs(),
                                limit_value: max_voltage,
                                severity: "WARNING".to_string(),
                            });
                        }
                    }
                }
                
                // Check power limit
                if let Some(max_power) = limits.max_power {
                    // Calculate power from voltage and current if available
                    let current_signal = SignalRef::Current(component.clone());
                    let voltage_signal = SignalRef::Voltage(component.clone());
                    
                    if let (Some(&current), Some(&voltage)) = 
                        (values.get(&current_signal), values.get(&voltage_signal)) {
                        let power = (current * voltage).abs();
                        if power > max_power {
                            violations.push(StressViolation {
                                component: component.clone(),
                                stress_type: "Power".to_string(),
                                actual_value: power,
                                limit_value: max_power,
                                severity: if power > max_power * 2.0 {
                                    "CRITICAL".to_string()
                                } else {
                                    "WARNING".to_string()
                                },
                            });
                        }
                    }
                }
            }
        }
        
        Ok(violations)
    }
}

/// Results from testbench execution
#[derive(Debug)]
pub struct TestbenchResults {
    pub passed: bool,
    pub violations: Vec<AssertionViolation>,
    pub measurements: HashMap<String, f64>,
    pub simulation_time: f64,
}

#[derive(Debug, Clone)]
pub struct AssertionViolation {
    pub time: f64,
    pub assertion_name: String,
    pub message: String,
    pub severity: crate::testbench::Severity,
}

/// Parse a value string with units (e.g., "4.7k", "10uF", "330")
pub(crate) fn parse_value_with_units(value_str: &str) -> Option<f64> {
    let value_str = value_str.trim();
    
    // Find where the number ends and the unit begins
    let mut num_end = value_str.len();
    for (i, ch) in value_str.char_indices() {
        if ch.is_alphabetic() && i > 0 {
            num_end = i;
            break;
        }
    }
    
    let (num_part, unit_part) = value_str.split_at(num_end);
    let base_value: f64 = num_part.parse().ok()?;
    
    // Apply unit multiplier
    let multiplier = match unit_part {
        "k" | "K" => 1e3,
        "M" => 1e6,
        "G" => 1e9,
        "m" => 1e-3,
        "u" | "µ" => 1e-6,
        "n" => 1e-9,
        "p" => 1e-12,
        "" => 1.0,
        _ => 1.0, // Unknown unit, assume no multiplier
    };
    
    Some(base_value * multiplier)
}