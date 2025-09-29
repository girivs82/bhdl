/// Signal flow analyzer that extracts topology information from netlist metadata
/// Uses rich metadata instead of inferring from naming patterns

use std::collections::{HashMap, HashSet, VecDeque};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct SignalFlowAnalysis {
    pub input_nets: Vec<String>,
    pub output_nets: Vec<String>,
    pub power_path: Vec<String>,  // Component sequence in power path
    pub signal_stages: Vec<Stage>,
    pub component_roles: HashMap<String, ComponentRole>,
}

#[derive(Debug, Clone)]
pub struct Stage {
    pub stage_num: usize,
    pub components: Vec<String>,
    pub stage_type: StageType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StageType {
    Input,
    PowerConversion,
    Filtering,
    Regulation,
    Protection,
    Output,
    Feedback,
    Support,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ComponentRole {
    InputFilter,
    PowerConverter,  // IC doing conversion
    EnergyStorage,   // Inductor/transformer
    OutputFilter,
    FeedbackNetwork,
    Protection,
    Decoupling,
    Supporting,
}

pub struct SignalFlowAnalyzer {
    pub components: HashMap<String, ComponentInfo>,
    nets: HashMap<String, Vec<String>>,  // net -> connected components
    connections: HashMap<String, Vec<String>>,  // component -> connected components
}

#[derive(Debug, Clone)]
pub struct ComponentInfo {
    pub name: String,
    pub component_type: String,
    pub pins: Vec<PinInfo>,
}

#[derive(Debug, Clone)]
struct PinInfo {
    name: String,
    net: Option<String>,
    pin_type: PinType,
}

#[derive(Debug, Clone, PartialEq)]
enum PinType {
    PowerIn,
    PowerOut,
    Ground,
    Signal,
    Unknown,
}

impl SignalFlowAnalyzer {
    pub fn new() -> Self {
        Self {
            components: HashMap::new(),
            nets: HashMap::new(),
            connections: HashMap::new(),
        }
    }
    
    /// Create analyzer from netlist JSON, using rich metadata
    pub fn from_netlist(netlist: &Value) -> Self {
        let mut analyzer = Self::new();
        analyzer.parse_netlist_metadata(netlist);
        analyzer
    }
    
    /// Parse netlist metadata to extract components, nets, and connectivity
    fn parse_netlist_metadata(&mut self, netlist: &Value) {
        // Extract pin definitions for proper pin type identification
        let mut pin_type_map: HashMap<String, PinType> = HashMap::new();
        if let Some(pins) = netlist["pins"].as_array() {
            for pin in pins {
                if let Some(pin_value) = pin.get("value") {
                    if !pin_value.is_null() {
                        if let (Some(pin_name), Some(pin_type_str)) = (
                            pin_value["name"].as_str(),
                            pin_value["pin_type"].as_str()
                        ) {
                            let pin_type = match pin_type_str {
                                "Power" => PinType::PowerIn,
                                "Ground" => PinType::Ground,
                                "Signal" => PinType::Signal,
                                _ => PinType::Unknown,
                            };
                            pin_type_map.insert(pin_name.to_string(), pin_type);
                        }
                    }
                }
            }
        }
        
        // Extract instances to build component list
        if let Some(instances) = netlist["instances"].as_array() {
            for instance in instances {
                if let Some(inst_value) = instance.get("value") {
                    if !inst_value.is_null() {
                        if let Some(name) = inst_value["name"].as_str() {
                            // Determine component type from module definitions
                            let component_type = if let Some(def_ref) = inst_value["definition"].as_object() {
                                if let Some(idx) = def_ref["idx"].as_u64() {
                                    self.get_module_name_by_index(netlist, idx as usize)
                                        .unwrap_or_else(|| "Unknown".to_string())
                                } else {
                                    "Unknown".to_string()
                                }
                            } else {
                                "Unknown".to_string()
                            };
                            
                            // Build pin info for this component
                            let pins = self.get_component_pins(netlist, name, &pin_type_map);
                            
                            self.components.insert(name.to_string(), ComponentInfo {
                                name: name.to_string(),
                                component_type,
                                pins,
                            });
                        }
                    }
                }
            }
        }
        
        // Extract nets and build connectivity graph
        if let Some(nets) = netlist["nets"].as_array() {
            for net in nets {
                if let Some(net_value) = net.get("value") {
                    if !net_value.is_null() {
                        if let Some(net_name) = net_value["name"].as_str() {
                            let connected_components = self.get_connected_components(netlist, net_value);
                            self.add_net(net_name.to_string(), connected_components);
                        }
                    }
                }
            }
        }
    }
    
    /// Get module name by index from modules array
    fn get_module_name_by_index(&self, netlist: &Value, index: usize) -> Option<String> {
        if let Some(modules) = netlist["modules"].as_array() {
            if let Some(module) = modules.get(index) {
                if let Some(module_value) = module.get("value") {
                    if !module_value.is_null() {
                        return module_value["name"].as_str().map(|s| s.to_string());
                    }
                }
            }
        }
        None
    }
    
    /// Get component pins with proper types from pin_instances
    fn get_component_pins(&self, netlist: &Value, component_name: &str, pin_type_map: &HashMap<String, PinType>) -> Vec<PinInfo> {
        let mut pins = Vec::new();
        
        if let Some(pin_instances) = netlist["pin_instances"].as_array() {
            for pin_inst in pin_instances {
                if let Some(pin_value) = pin_inst.get("value") {
                    if !pin_value.is_null() {
                        // Check if this pin belongs to our component
                        if let Some(instance_ref) = pin_value["instance"].as_object() {
                            if let Some(inst_idx) = instance_ref["idx"].as_u64() {
                                if let Some(inst_name) = self.get_instance_name_by_index(netlist, inst_idx as usize) {
                                    if inst_name == component_name {
                                        // Get pin name and net
                                        if let Some(pin_def_ref) = pin_value["pin_def"].as_object() {
                                            if let Some(pin_def_idx) = pin_def_ref["idx"].as_u64() {
                                                if let Some(pin_name) = self.get_pin_name_by_index(netlist, pin_def_idx as usize) {
                                                    let net_name = if let Some(net_ref) = pin_value["net"].as_object() {
                                                        if let Some(net_idx) = net_ref["idx"].as_u64() {
                                                            self.get_net_name_by_index(netlist, net_idx as usize)
                                                        } else { None }
                                                    } else { None };
                                                    
                                                    let pin_type = pin_type_map.get(&pin_name)
                                                        .cloned()
                                                        .unwrap_or(PinType::Unknown);
                                                    
                                                    pins.push(PinInfo {
                                                        name: pin_name,
                                                        net: net_name,
                                                        pin_type,
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        pins
    }
    
    /// Get instance name by index
    fn get_instance_name_by_index(&self, netlist: &Value, index: usize) -> Option<String> {
        if let Some(instances) = netlist["instances"].as_array() {
            if let Some(instance) = instances.get(index) {
                if let Some(inst_value) = instance.get("value") {
                    if !inst_value.is_null() {
                        return inst_value["name"].as_str().map(|s| s.to_string());
                    }
                }
            }
        }
        None
    }
    
    /// Get pin name by index
    fn get_pin_name_by_index(&self, netlist: &Value, index: usize) -> Option<String> {
        if let Some(pins) = netlist["pins"].as_array() {
            if let Some(pin) = pins.get(index) {
                if let Some(pin_value) = pin.get("value") {
                    if !pin_value.is_null() {
                        return pin_value["name"].as_str().map(|s| s.to_string());
                    }
                }
            }
        }
        None
    }
    
    /// Get net name by index
    fn get_net_name_by_index(&self, netlist: &Value, index: usize) -> Option<String> {
        if let Some(nets) = netlist["nets"].as_array() {
            if let Some(net) = nets.get(index) {
                if let Some(net_value) = net.get("value") {
                    if !net_value.is_null() {
                        return net_value["name"].as_str().map(|s| s.to_string());
                    }
                }
            }
        }
        None
    }
    
    /// Get components connected to a net
    fn get_connected_components(&self, netlist: &Value, net_value: &Value) -> Vec<String> {
        let mut components = Vec::new();
        
        if let Some(connections) = net_value["connections"].as_array() {
            for connection in connections {
                if let Some(pin_inst_ref) = connection["PinInstance"].as_object() {
                    if let Some(pin_inst_idx) = pin_inst_ref["idx"].as_u64() {
                        // Find the component that owns this pin instance
                        if let Some(component_name) = self.get_component_for_pin_instance(netlist, pin_inst_idx as usize) {
                            if !components.contains(&component_name) {
                                components.push(component_name);
                            }
                        }
                    }
                }
            }
        }
        
        components
    }
    
    /// Get component name that owns a pin instance
    fn get_component_for_pin_instance(&self, netlist: &Value, pin_inst_idx: usize) -> Option<String> {
        if let Some(pin_instances) = netlist["pin_instances"].as_array() {
            if let Some(pin_inst) = pin_instances.get(pin_inst_idx) {
                if let Some(pin_value) = pin_inst.get("value") {
                    if !pin_value.is_null() {
                        if let Some(instance_ref) = pin_value["instance"].as_object() {
                            if let Some(inst_idx) = instance_ref["idx"].as_u64() {
                                return self.get_instance_name_by_index(netlist, inst_idx as usize);
                            }
                        }
                    }
                }
            }
        }
        None
    }
    
    /// Add component information
    pub fn add_component(&mut self, name: String, component_type: String, pins: Vec<(String, Option<String>)>) {
        let pin_infos = pins.into_iter().map(|(pin_name, net)| {
            let pin_type = self.classify_pin(&component_type, &pin_name);
            PinInfo { name: pin_name, net, pin_type }
        }).collect();
        
        self.components.insert(name.clone(), ComponentInfo {
            name: name.clone(),
            component_type,
            pins: pin_infos,
        });
    }
    
    /// Add net information
    pub fn add_net(&mut self, net_name: String, connected_components: Vec<String>) {
        // Build bidirectional connection graph before moving
        for i in 0..connected_components.len() {
            for j in i+1..connected_components.len() {
                let comp1 = connected_components[i].clone();
                let comp2 = connected_components[j].clone();
                
                self.connections.entry(comp1.clone())
                    .or_insert_with(Vec::new)
                    .push(comp2.clone());
                    
                self.connections.entry(comp2)
                    .or_insert_with(Vec::new)
                    .push(comp1);
            }
        }
        
        // Now insert the net
        self.nets.insert(net_name, connected_components);
    }
    
    /// Analyze the circuit and determine signal flow
    pub fn analyze(&self) -> SignalFlowAnalysis {
        // Identify input and output nets
        let (input_nets, output_nets) = self.identify_io_nets();
        
        // Trace power path from input to output
        let power_path = self.trace_power_path(&input_nets, &output_nets);
        
        // Identify component stages
        let signal_stages = self.identify_stages(&power_path);
        
        // Assign roles to components
        let component_roles = self.assign_component_roles(&signal_stages);
        
        SignalFlowAnalysis {
            input_nets,
            output_nets,
            power_path,
            signal_stages,
            component_roles,
        }
    }
    
    /// Analyze the circuit using netlist metadata
    pub fn analyze_with_metadata(&self, netlist: &Value) -> SignalFlowAnalysis {
        // Identify input and output nets using metadata
        let (input_nets, output_nets) = self.identify_io_nets_from_metadata(netlist);
        
        // Trace power path from input to output
        let power_path = self.trace_power_path(&input_nets, &output_nets);
        
        // Identify component stages
        let signal_stages = self.identify_stages(&power_path);
        
        // Assign roles to components using metadata
        let component_roles = self.assign_component_roles_from_metadata(netlist);
        
        SignalFlowAnalysis {
            input_nets,
            output_nets,
            power_path,
            signal_stages,
            component_roles,
        }
    }
    
    /// Identify input and output nets based on net classes from metadata
    fn identify_io_nets(&self) -> (Vec<String>, Vec<String>) {
        let mut input_nets = Vec::new();
        let mut output_nets = Vec::new();
        
        for (net_name, _) in &self.nets {
            // Use naming patterns as primary method
            if net_name.contains("VIN") || net_name.contains("INPUT") || 
               net_name.contains("VCC") || net_name.starts_with("V+") {
                input_nets.push(net_name.clone());
            }
            // Common output net patterns
            else if net_name.contains("VOUT") || net_name.contains("OUTPUT") ||
                    net_name.ends_with("_OUT") {
                output_nets.push(net_name.clone());
            }
        }
        
        // If no clear input/output, use heuristics
        if input_nets.is_empty() || output_nets.is_empty() {
            self.identify_io_by_topology(&mut input_nets, &mut output_nets);
        }
        
        (input_nets, output_nets)
    }
    
    /// Identify input and output nets using netlist metadata
    fn identify_io_nets_from_metadata(&self, netlist: &Value) -> (Vec<String>, Vec<String>) {
        let mut input_nets = Vec::new();
        let mut output_nets = Vec::new();
        
        if let Some(nets) = netlist["nets"].as_array() {
            for net in nets {
                if let Some(net_value) = net.get("value") {
                    if !net_value.is_null() {
                        if let Some(net_name) = net_value["name"].as_str() {
                            // Check net class for power identification
                            if let Some(net_class) = net_value["net_class"].as_object() {
                                if let Some(power_value) = net_class["Power"].as_f64() {
                                    // This is a power net - likely input
                                    if net_name.contains("VIN") || net_name.contains("INPUT") {
                                        input_nets.push(net_name.to_string());
                                    }
                                }
                            } else if let Some(net_class_str) = net_value["net_class"].as_str() {
                                if net_class_str == "Signal" {
                                    // Signal nets could be output
                                    if net_name.contains("VOUT") || net_name.contains("OUTPUT") {
                                        output_nets.push(net_name.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Fallback to naming patterns if metadata doesn't provide clear info
        if input_nets.is_empty() || output_nets.is_empty() {
            return self.identify_io_nets();
        }
        
        (input_nets, output_nets)
    }
    
    /// Use topology analysis to identify input/output when names aren't clear
    fn identify_io_by_topology(&self, input_nets: &mut Vec<String>, output_nets: &mut Vec<String>) {
        // Find IC components
        let ics: Vec<_> = self.components.iter()
            .filter(|(_, info)| info.component_type == "IC")
            .collect();
        
        if let Some((ic_name, ic_info)) = ics.first() {
            // Input nets are typically connected to IC input pins
            for pin in &ic_info.pins {
                if pin.pin_type == PinType::PowerIn {
                    if let Some(net) = &pin.net {
                        if !input_nets.contains(net) {
                            input_nets.push(net.clone());
                        }
                    }
                }
                // Output nets from IC output pins
                else if pin.pin_type == PinType::PowerOut {
                    if let Some(net) = &pin.net {
                        if !output_nets.contains(net) {
                            output_nets.push(net.clone());
                        }
                    }
                }
            }
        }
    }
    
    /// Trace the main power path through the circuit
    fn trace_power_path(&self, input_nets: &[String], output_nets: &[String]) -> Vec<String> {
        let mut path = Vec::new();
        
        // Find components connected to input
        let mut current_components = HashSet::new();
        for net in input_nets {
            if let Some(components) = self.nets.get(net) {
                current_components.extend(components.clone());
            }
        }
        
        // BFS to find path to output
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        
        // Start from input-connected components
        for comp in current_components {
            queue.push_back((comp.clone(), vec![comp.clone()]));
        }
        
        while let Some((current, current_path)) = queue.pop_front() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current.clone());
            
            // Check if we reached output
            if self.is_connected_to_output(&current, output_nets) {
                path = current_path;
                break;
            }
            
            // Explore connections
            if let Some(connected) = self.connections.get(&current) {
                for next in connected {
                    if !visited.contains(next) {
                        let mut new_path = current_path.clone();
                        new_path.push(next.clone());
                        queue.push_back((next.clone(), new_path));
                    }
                }
            }
        }
        
        // Filter to only include significant components (ICs, inductors, etc.)
        path.into_iter()
            .filter(|comp| self.is_significant_component(comp))
            .collect()
    }
    
    /// Check if component is connected to output nets
    fn is_connected_to_output(&self, component: &str, output_nets: &[String]) -> bool {
        if let Some(info) = self.components.get(component) {
            for pin in &info.pins {
                if let Some(net) = &pin.net {
                    if output_nets.contains(net) {
                        return true;
                    }
                }
            }
        }
        false
    }
    
    /// Determine if component is significant for power path
    fn is_significant_component(&self, component: &str) -> bool {
        if let Some(info) = self.components.get(component) {
            matches!(info.component_type.as_str(), 
                "IC" | "TPS54302" | "Inductor" | "Transformer" | "Diode" | "MOSFET" | "Transistor")
        } else {
            false
        }
    }
    
    /// Identify functional stages in the circuit
    fn identify_stages(&self, power_path: &[String]) -> Vec<Stage> {
        let mut stages = Vec::new();
        let mut stage_num = 0;
        
        // Input stage - components connected to input nets
        let mut input_stage_components = Vec::new();
        for (comp_name, info) in &self.components {
            if self.is_input_stage_component(info) {
                input_stage_components.push(comp_name.clone());
            }
        }
        if !input_stage_components.is_empty() {
            stages.push(Stage {
                stage_num,
                components: input_stage_components,
                stage_type: StageType::Input,
            });
            stage_num += 1;
        }
        
        // Power conversion stage - IC and directly connected components
        if !power_path.is_empty() {
            stages.push(Stage {
                stage_num,
                components: power_path.to_vec(),
                stage_type: StageType::PowerConversion,
            });
            stage_num += 1;
        }
        
        // Output stage
        let mut output_stage_components = Vec::new();
        for (comp_name, info) in &self.components {
            if self.is_output_stage_component(info) {
                output_stage_components.push(comp_name.clone());
            }
        }
        if !output_stage_components.is_empty() {
            stages.push(Stage {
                stage_num,
                components: output_stage_components,
                stage_type: StageType::Output,
            });
            stage_num += 1;
        }
        
        // Feedback stage
        let feedback_components = self.identify_feedback_components();
        if !feedback_components.is_empty() {
            stages.push(Stage {
                stage_num,
                components: feedback_components,
                stage_type: StageType::Feedback,
            });
        }
        
        stages
    }
    
    /// Check if component is part of input stage
    fn is_input_stage_component(&self, info: &ComponentInfo) -> bool {
        // Input capacitors
        if info.component_type == "Capacitor" {
            for pin in &info.pins {
                if let Some(net) = &pin.net {
                    if net.contains("VIN") || net.contains("INPUT") {
                        return true;
                    }
                }
            }
        }
        false
    }
    
    /// Check if component is part of output stage
    fn is_output_stage_component(&self, info: &ComponentInfo) -> bool {
        // Output capacitors
        if info.component_type == "Capacitor" {
            for pin in &info.pins {
                if let Some(net) = &pin.net {
                    if net.contains("VOUT") || net.contains("OUTPUT") {
                        return true;
                    }
                }
            }
        }
        false
    }
    
    /// Identify feedback network components
    fn identify_feedback_components(&self) -> Vec<String> {
        let mut feedback = Vec::new();
        
        for (comp_name, info) in &self.components {
            // Resistors connected to FB pin
            if info.component_type == "Resistor" {
                for pin in &info.pins {
                    if let Some(net) = &pin.net {
                        if net.contains("FB") || net.contains("FEEDBACK") {
                            feedback.push(comp_name.clone());
                            break;
                        }
                    }
                }
            }
        }
        
        feedback
    }
    
    /// Assign roles to components using netlist metadata
    fn assign_component_roles_from_metadata(&self, netlist: &Value) -> HashMap<String, ComponentRole> {
        let mut roles = HashMap::new();
        
        // First, use the analysis_data if available
        if let Some(analysis_data) = netlist["analysis_data"].as_object() {
            if let Some(instance_analysis) = analysis_data["instance_analysis"].as_object() {
                for (comp_name, analysis) in instance_analysis {
                    if let Some(component_role_str) = analysis["component_role"].as_str() {
                        let role = match component_role_str {
                            "InputFilter" => ComponentRole::InputFilter,
                            "OutputFilter" => ComponentRole::OutputFilter,
                            "OutputStabilization" => ComponentRole::OutputFilter, // Map to OutputFilter
                            "Decoupling" => ComponentRole::Decoupling,
                            "PowerConverter" => ComponentRole::PowerConverter,
                            "EnergyStorage" => ComponentRole::EnergyStorage,
                            "Protection" => ComponentRole::Protection,
                            "FeedbackNetwork" => ComponentRole::FeedbackNetwork,
                            _ => ComponentRole::Supporting,
                        };
                        roles.insert(comp_name.clone(), role);
                    }
                }
            }
        }
        
        // For components not covered by analysis_data, fall back to type-based assignment
        for (comp_name, comp_info) in &self.components {
            if !roles.contains_key(comp_name) {
                let role = match comp_info.component_type.as_str() {
                    "TPS54302" | "IC" => ComponentRole::PowerConverter,
                    "Inductor" => ComponentRole::EnergyStorage,
                    "Capacitor" => {
                        // Try to infer from connections
                        if self.is_connected_to_input_net(comp_name) {
                            ComponentRole::InputFilter
                        } else if self.is_connected_to_output_net(comp_name) {
                            ComponentRole::OutputFilter
                        } else {
                            ComponentRole::Decoupling
                        }
                    }
                    "Resistor" => {
                        // Check if connected to feedback nets
                        if self.is_connected_to_feedback_net(comp_name) {
                            ComponentRole::FeedbackNetwork
                        } else {
                            ComponentRole::Supporting
                        }
                    }
                    "Diode" => ComponentRole::Protection,
                    _ => ComponentRole::Supporting,
                };
                roles.insert(comp_name.clone(), role);
            }
        }
        
        roles
    }
    
    /// Assign roles to components based on stages and connections (fallback method)
    fn assign_component_roles(&self, stages: &[Stage]) -> HashMap<String, ComponentRole> {
        let mut roles = HashMap::new();
        
        for stage in stages {
            let role = match stage.stage_type {
                StageType::Input => ComponentRole::InputFilter,
                StageType::PowerConversion => ComponentRole::PowerConverter,
                StageType::Output => ComponentRole::OutputFilter,
                StageType::Feedback => ComponentRole::FeedbackNetwork,
                StageType::Protection => ComponentRole::Protection,
                _ => ComponentRole::Supporting,
            };
            
            for comp in &stage.components {
                // Special case for specific component types
                if let Some(info) = self.components.get(comp) {
                    let specific_role = match info.component_type.as_str() {
                        "Inductor" => ComponentRole::EnergyStorage,
                        "Diode" if stage.stage_type == StageType::PowerConversion => ComponentRole::Protection,
                        _ => role.clone(),
                    };
                    roles.insert(comp.clone(), specific_role);
                } else {
                    roles.insert(comp.clone(), role.clone());
                }
            }
        }
        
        // Identify decoupling capacitors (small caps near ICs)
        for (comp_name, info) in &self.components {
            if info.component_type == "Capacitor" && !roles.contains_key(comp_name) {
                // Check if connected to IC power pins
                let is_decoupling = info.pins.iter().any(|pin| {
                    pin.net.as_ref().map_or(false, |net| {
                        net.contains("VCC") || net.contains("VDD") || 
                        (net.contains("GND") && self.is_near_ic(comp_name))
                    })
                });
                
                if is_decoupling {
                    roles.insert(comp_name.clone(), ComponentRole::Decoupling);
                }
            }
        }
        
        roles
    }
    
    /// Internal power path tracing for use in fallback methods
    fn trace_power_path_internal(&self) -> Vec<String> {
        let (input_nets, output_nets) = self.identify_io_nets();
        self.trace_power_path(&input_nets, &output_nets)
    }
    
    /// Check if component is electrically near an IC
    fn is_near_ic(&self, component: &str) -> bool {
        // Check if directly connected to an IC
        if let Some(connected) = self.connections.get(component) {
            connected.iter().any(|comp| {
                self.components.get(comp)
                    .map_or(false, |info| info.component_type == "IC")
            })
        } else {
            false
        }
    }
    
    /// Check if component is connected to input nets
    fn is_connected_to_input_net(&self, component: &str) -> bool {
        if let Some(comp_info) = self.components.get(component) {
            comp_info.pins.iter().any(|pin| {
                pin.net.as_ref().map_or(false, |net| {
                    net.contains("VIN") || net.contains("INPUT")
                })
            })
        } else {
            false
        }
    }
    
    /// Check if component is connected to output nets
    fn is_connected_to_output_net(&self, component: &str) -> bool {
        if let Some(comp_info) = self.components.get(component) {
            comp_info.pins.iter().any(|pin| {
                pin.net.as_ref().map_or(false, |net| {
                    net.contains("VOUT") || net.contains("OUTPUT")
                })
            })
        } else {
            false
        }
    }
    
    /// Check if component is connected to feedback nets
    fn is_connected_to_feedback_net(&self, component: &str) -> bool {
        if let Some(comp_info) = self.components.get(component) {
            comp_info.pins.iter().any(|pin| {
                pin.net.as_ref().map_or(false, |net| {
                    net.contains("FB") || net.contains("FEEDBACK")
                })
            })
        } else {
            false
        }
    }
    
    /// Classify pin type based on component type and pin name
    fn classify_pin(&self, component_type: &str, pin_name: &str) -> PinType {
        match component_type {
            "IC" => {
                match pin_name.to_uppercase().as_str() {
                    "VIN" | "VCC" | "VDD" | "V+" => PinType::PowerIn,
                    "VOUT" | "SW" | "OUT" => PinType::PowerOut,
                    "GND" | "VSS" | "V-" => PinType::Ground,
                    _ => PinType::Signal,
                }
            }
            _ => PinType::Unknown,
        }
    }
}