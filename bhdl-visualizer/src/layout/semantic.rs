use std::collections::{HashMap, HashSet};
use bhdl_netlist::{InstanceId, NetId, Netlist, ConnectionPoint, PinId};
use crate::layout::types::{Point, LayoutResult};

#[derive(Debug, Clone)]
pub enum CircuitPattern {
    PowerRegulator {
        regulator: InstanceId,
        input_caps: Vec<InstanceId>,
        output_caps: Vec<InstanceId>,
        feedback: Vec<InstanceId>,
    },
    OpAmpStage {
        op_amp: InstanceId,
        input_network: Vec<InstanceId>,
        feedback_network: Vec<InstanceId>,
        output_network: Vec<InstanceId>,
    },
    MicrocontrollerCore {
        mcu: InstanceId,
        power_components: Vec<InstanceId>,
        decoupling_caps: Vec<InstanceId>,
        crystal: Option<InstanceId>,
        reset_circuit: Vec<InstanceId>,
    },
    DifferentialPair {
        positive_net: NetId,
        negative_net: NetId,
        components: Vec<InstanceId>,
    },
    PowerDistribution {
        power_nets: Vec<NetId>,
        ground_nets: Vec<NetId>,
        components: Vec<InstanceId>,
    },
    SignalChain {
        chain: Vec<InstanceId>,
        signal_path: Vec<NetId>,
    },
}

#[derive(Debug, Clone)]
pub struct SemanticLayoutConstraints {
    pub power_rails_vertical: bool,
    pub signal_flow_left_to_right: bool,
    pub group_by_function: bool,
    pub differential_pairs_symmetric: bool,
    pub decoupling_caps_near_power_pins: bool,
}

impl Default for SemanticLayoutConstraints {
    fn default() -> Self {
        SemanticLayoutConstraints {
            power_rails_vertical: true,
            signal_flow_left_to_right: true,
            group_by_function: true,
            differential_pairs_symmetric: true,
            decoupling_caps_near_power_pins: true,
        }
    }
}

pub struct SemanticAnalyzer<'a> {
    netlist: &'a Netlist,
    patterns: Vec<CircuitPattern>,
    constraints: SemanticLayoutConstraints,
}

impl<'a> SemanticAnalyzer<'a> {
    pub fn new(netlist: &'a Netlist) -> Self {
        SemanticAnalyzer {
            netlist,
            patterns: Vec::new(),
            constraints: SemanticLayoutConstraints::default(),
        }
    }

    pub fn analyze_patterns(&mut self) {
        self.find_power_regulators();
        self.find_op_amp_stages();
        self.find_microcontroller_cores();
        self.find_differential_pairs();
        self.find_power_distribution();
        self.find_signal_chains();
        
        println!("Found {} circuit patterns", self.patterns.len());
    }

    fn find_power_regulators(&mut self) {
        for (instance_id, instance) in &self.netlist.instances {
            if let Some(def) = self.netlist.modules.get(instance.definition) {
                if self.is_voltage_regulator(&def.name) {
                    let input_caps = self.find_connected_capacitors(instance_id, "input");
                    let output_caps = self.find_connected_capacitors(instance_id, "output");
                    println!("Found voltage regulator: {} with {} input caps, {} output caps", 
                             instance.name, input_caps.len(), output_caps.len());
                    self.patterns.push(CircuitPattern::PowerRegulator {
                        regulator: instance_id,
                        input_caps,
                        output_caps,
                        feedback: self.find_feedback_components(instance_id),
                    });
                }
            }
        }
    }

    fn find_op_amp_stages(&mut self) {
        for (instance_id, instance) in &self.netlist.instances {
            if let Some(def) = self.netlist.modules.get(instance.definition) {
                if self.is_op_amp(&def.name) {
                    self.patterns.push(CircuitPattern::OpAmpStage {
                        op_amp: instance_id,
                        input_network: self.find_input_components(instance_id),
                        feedback_network: self.find_feedback_components(instance_id),
                        output_network: self.find_output_components(instance_id),
                    });
                }
            }
        }
    }

    fn find_microcontroller_cores(&mut self) {
        for (instance_id, instance) in &self.netlist.instances {
            if let Some(def) = self.netlist.modules.get(instance.definition) {
                if self.is_microcontroller(&def.name) {
                    self.patterns.push(CircuitPattern::MicrocontrollerCore {
                        mcu: instance_id,
                        power_components: self.find_power_components_for_mcu(instance_id),
                        decoupling_caps: self.find_decoupling_caps_for_mcu(instance_id),
                        crystal: self.find_crystal_for_mcu(instance_id),
                        reset_circuit: self.find_reset_circuit_for_mcu(instance_id),
                    });
                }
            }
        }
    }

    fn find_differential_pairs(&mut self) {
        for (net_id, net) in &self.netlist.nets {
            if let Some(net_name) = &net.name {
                if let Some(pair_net_id) = self.find_differential_pair_net(net_name, net_id) {
                    let components = self.find_components_on_differential_pair(net_id, pair_net_id);
                    self.patterns.push(CircuitPattern::DifferentialPair {
                        positive_net: net_id,
                        negative_net: pair_net_id,
                        components,
                    });
                }
            }
        }
    }

    fn find_power_distribution(&mut self) {
        let mut power_nets = Vec::new();
        let mut ground_nets = Vec::new();
        let mut components = Vec::new();

        for (net_id, net) in &self.netlist.nets {
            if let Some(net_name) = &net.name {
                if self.is_power_net(net_name) {
                    power_nets.push(net_id);
                    components.extend(self.find_components_on_net(net_id));
                } else if self.is_ground_net(net_name) {
                    ground_nets.push(net_id);
                    components.extend(self.find_components_on_net(net_id));
                }
            }
        }

        if !power_nets.is_empty() || !ground_nets.is_empty() {
            components.sort();
            components.dedup();
            self.patterns.push(CircuitPattern::PowerDistribution {
                power_nets,
                ground_nets,
                components,
            });
        }
    }

    fn find_signal_chains(&mut self) {
        let mut visited = HashSet::new();
        
        for (instance_id, _) in &self.netlist.instances {
            if !visited.contains(&instance_id) {
                if let Some((chain, signal_path)) = self.build_series_chain(instance_id, &mut visited) {
                    if chain.len() > 1 {
                        println!("Found signal chain with {} components", chain.len());
                        self.patterns.push(CircuitPattern::SignalChain { chain, signal_path });
                    }
                }
            }
        }
    }

    fn build_series_chain(&self, start_instance: InstanceId, visited: &mut HashSet<InstanceId>) -> Option<(Vec<InstanceId>, Vec<NetId>)> {
        let mut chain = vec![start_instance];
        let mut signal_path = Vec::new();
        let mut current_instance = start_instance;
        visited.insert(start_instance);

        loop {
            if let Some((next_instance, connecting_net)) = self.find_next_in_series(current_instance, visited) {
                chain.push(next_instance);
                signal_path.push(connecting_net);
                visited.insert(next_instance);
                current_instance = next_instance;
            } else {
                break;
            }
        }

        if chain.len() > 1 {
            Some((chain, signal_path))
        } else {
            None
        }
    }

    fn find_next_in_series(&self, instance_id: InstanceId, visited: &HashSet<InstanceId>) -> Option<(InstanceId, NetId)> {
        let nets_connected = self.find_nets_connected_to_instance(instance_id);
        
        for net_id in nets_connected {
            if !self.is_series_connection(instance_id, instance_id, net_id) {
                continue;
            }
            
            let instances_on_net = self.find_instances_on_net(net_id);
            
            for other_instance in instances_on_net {
                if other_instance != instance_id && !visited.contains(&other_instance) {
                    return Some((other_instance, net_id));
                }
            }
        }
        
        None
    }

    fn is_series_connection(&self, _inst1: InstanceId, _inst2: InstanceId, net_id: NetId) -> bool {
        if let Some(net) = self.netlist.nets.get(net_id) {
            if let Some(net_name) = &net.name {
                if net_name.to_lowercase().contains("vcc") || 
                   net_name.to_lowercase().contains("vdd") || 
                   net_name.to_lowercase().contains("gnd") || 
                   net_name.to_lowercase().contains("ground") {
                    return false;
                }
            }
        }
        true
    }

    fn find_nets_connected_to_instance(&self, instance_id: InstanceId) -> Vec<NetId> {
        let mut nets = Vec::new();
        
        for (net_id, net) in &self.netlist.nets {
            for connection in &net.connections {
                if let (Some(inst_id), _) = self.extract_instance_pin(connection) {
                    if inst_id == instance_id {
                        nets.push(net_id);
                        break;
                    }
                }
            }
        }
        
        nets
    }

    fn find_instances_on_net(&self, net_id: NetId) -> Vec<InstanceId> {
        let mut instances = Vec::new();
        
        if let Some(net) = self.netlist.nets.get(net_id) {
            for connection in &net.connections {
                if let (Some(inst_id), _) = self.extract_instance_pin(connection) {
                    instances.push(inst_id);
                }
            }
        }
        
        instances
    }

    fn is_voltage_regulator(&self, name: &str) -> bool {
        let name_lower = name.to_lowercase();
        name_lower.contains("regulator") || name_lower.contains("reg") || name_lower.contains("ldo")
    }

    fn is_op_amp(&self, name: &str) -> bool {
        let name_lower = name.to_lowercase();
        name_lower.contains("opamp") || name_lower.contains("op_amp") || name_lower.contains("amplifier")
    }

    fn is_microcontroller(&self, name: &str) -> bool {
        let name_lower = name.to_lowercase();
        name_lower.contains("mcu") || name_lower.contains("microcontroller") || name_lower.contains("processor")
    }

    fn is_power_net(&self, name: &str) -> bool {
        let name_lower = name.to_lowercase();
        name_lower.contains("vcc") || name_lower.contains("vdd") || name_lower.contains("power")
    }

    fn is_ground_net(&self, name: &str) -> bool {
        let name_lower = name.to_lowercase();
        name_lower.contains("gnd") || name_lower.contains("ground") || name_lower.contains("vss")
    }

    fn find_connected_capacitors(&self, regulator_id: InstanceId, context: &str) -> Vec<InstanceId> {
        let mut capacitors = Vec::new();
        
        // Find nets connected to the regulator
        let connected_nets = self.find_nets_connected_to_instance(regulator_id);
        
        for net_id in connected_nets {
            if let Some(net) = self.netlist.nets.get(net_id) {
                // Determine if this net is input or output based on context and net name
                let is_target_net = if context == "input" {
                    // Input nets typically have names like VIN, VCC_IN, etc.
                    if let Some(net_name) = &net.name {
                        net_name.to_lowercase().contains("vin") || 
                        net_name.to_lowercase().contains("input") ||
                        (net_name.to_lowercase().contains("vcc") && !net_name.to_lowercase().contains("out"))
                    } else {
                        false
                    }
                } else if context == "output" {
                    // Output nets typically have names like VOUT, VCC_OUT, etc.
                    if let Some(net_name) = &net.name {
                        net_name.to_lowercase().contains("vout") || 
                        net_name.to_lowercase().contains("output") ||
                        (net_name.to_lowercase().contains("vcc") && net_name.to_lowercase().contains("out"))
                    } else {
                        false
                    }
                } else {
                    false
                };
                
                if is_target_net {
                    // Find capacitors connected to this net
                    for connection in &net.connections {
                        if let (Some(inst_id), _) = self.extract_instance_pin(connection) {
                            if inst_id != regulator_id {
                                // Check if this instance is a capacitor
                                if let Some(instance) = self.netlist.instances.get(inst_id) {
                                    if let Some(def) = self.netlist.modules.get(instance.definition) {
                                        if def.name.to_lowercase().contains("capacitor") {
                                            capacitors.push(inst_id);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        capacitors
    }

    fn find_feedback_components(&self, _instance_id: InstanceId) -> Vec<InstanceId> {
        Vec::new() // Placeholder implementation
    }

    fn find_input_components(&self, _instance_id: InstanceId) -> Vec<InstanceId> {
        Vec::new()
    }

    fn find_output_components(&self, _instance_id: InstanceId) -> Vec<InstanceId> {
        Vec::new()
    }

    fn find_power_components_for_mcu(&self, _instance_id: InstanceId) -> Vec<InstanceId> {
        Vec::new()
    }

    fn find_decoupling_caps_for_mcu(&self, _instance_id: InstanceId) -> Vec<InstanceId> {
        Vec::new()
    }

    fn find_crystal_for_mcu(&self, _instance_id: InstanceId) -> Option<InstanceId> {
        None
    }

    fn find_reset_circuit_for_mcu(&self, _instance_id: InstanceId) -> Vec<InstanceId> {
        Vec::new()
    }

    fn find_differential_pair_net(&self, name: &str, _current_net: NetId) -> Option<NetId> {
        // Look for complementary differential pair names
        let pair_name = if name.ends_with("_P") || name.ends_with("_p") {
            name.replace("_P", "_N").replace("_p", "_n")
        } else if name.ends_with("_N") || name.ends_with("_n") {
            name.replace("_N", "_P").replace("_n", "_p")
        } else if name.ends_with("+") {
            name.replace("+", "-")
        } else if name.ends_with("-") {
            name.replace("-", "+")
        } else {
            return None;
        };

        self.find_net_by_name(&pair_name)
    }

    fn find_net_by_name(&self, name: &str) -> Option<NetId> {
        for (net_id, net) in &self.netlist.nets {
            if let Some(net_name) = &net.name {
                if net_name == name {
                    return Some(net_id);
                }
            }
        }
        None
    }

    fn find_components_on_differential_pair(&self, net1: NetId, net2: NetId) -> Vec<InstanceId> {
        let mut components = HashSet::new();
        
        for &net_id in &[net1, net2] {
            if let Some(net) = self.netlist.nets.get(net_id) {
                for connection in &net.connections {
                    if let (Some(inst_id), _) = self.extract_instance_pin(connection) {
                        components.insert(inst_id);
                    }
                }
            }
        }
        
        components.into_iter().collect()
    }

    fn find_components_on_net(&self, net_id: NetId) -> Vec<InstanceId> {
        let mut components = Vec::new();
        
        if let Some(net) = self.netlist.nets.get(net_id) {
            for connection in &net.connections {
                if let (Some(inst_id), _) = self.extract_instance_pin(connection) {
                    components.push(inst_id);
                }
            }
        }
        
        components
    }

    pub fn get_patterns(&self) -> &[CircuitPattern] {
        &self.patterns
    }

    fn extract_instance_pin(&self, connection: &ConnectionPoint) -> (Option<InstanceId>, Option<PinId>) {
        match connection {
            ConnectionPoint::InstancePin(inst_id, pin_id) => (Some(*inst_id), Some(*pin_id)),
            ConnectionPoint::InstancePort(inst_id, _port_id) => {
                // For now, we'll need to map ports to pins somehow
                // This is a temporary solution - proper port/pin mapping needed
                (Some(*inst_id), None)
            }
            ConnectionPoint::ModulePort(_) => (None, None),
        }
    }

}

pub struct SemanticLayoutEngine<'a> {
    analyzer: SemanticAnalyzer<'a>,
    constraints: SemanticLayoutConstraints,
    component_rotations: HashMap<InstanceId, f64>,
}

impl<'a> SemanticLayoutEngine<'a> {
    pub fn new(netlist: &'a Netlist) -> Self {
        let mut analyzer = SemanticAnalyzer::new(netlist);
        analyzer.analyze_patterns();
        
        SemanticLayoutEngine {
            analyzer,
            constraints: SemanticLayoutConstraints::default(),
            component_rotations: HashMap::new(),
        }
    }

    pub fn apply_semantic_placement(&mut self, positions: &mut HashMap<InstanceId, Point>) {
        println!("Applying semantic placement for {} patterns", self.analyzer.patterns.len());
        
        // Clone patterns to avoid borrowing issues
        let patterns = self.analyzer.patterns.clone();
        for pattern in &patterns {
            self.apply_pattern_layout(pattern, positions);
        }
        
        // Second pass: optimize ground placement now that all other components are positioned
        self.optimize_ground_placements_final_pass(positions);
    }

    fn apply_pattern_layout(&mut self, pattern: &CircuitPattern, positions: &mut HashMap<InstanceId, Point>) {
        match pattern {
            CircuitPattern::PowerRegulator { regulator, input_caps, output_caps, feedback } => {
                self.layout_power_regulator(*regulator, input_caps, output_caps, feedback, positions);
            }
            CircuitPattern::OpAmpStage { op_amp, input_network, feedback_network, output_network } => {
                self.layout_op_amp_stage(*op_amp, input_network, feedback_network, output_network, positions);
            }
            CircuitPattern::MicrocontrollerCore { mcu, power_components, decoupling_caps, crystal, reset_circuit } => {
                self.layout_microcontroller_core(*mcu, power_components, decoupling_caps, crystal, reset_circuit, positions);
            }
            CircuitPattern::DifferentialPair { positive_net, negative_net, components } => {
                self.layout_differential_pair(*positive_net, *negative_net, components, positions);
            }
            CircuitPattern::PowerDistribution { power_nets, ground_nets, components } => {
                self.layout_power_distribution(power_nets, ground_nets, components, positions);
            }
            CircuitPattern::SignalChain { chain, signal_path } => {
                self.layout_signal_chain(chain, signal_path, positions);
            }
        }
    }

    fn layout_power_regulator(
        &mut self,
        regulator: InstanceId,
        input_caps: &[InstanceId],
        output_caps: &[InstanceId],
        feedback: &[InstanceId],
        positions: &mut HashMap<InstanceId, Point>
    ) {
        // Constraint-based layout: define relationships, not absolute positions
        let regulator_center = Point::new(0.0, 0.0); // Start at origin
        
        // Place the regulator at the logical center
        positions.insert(regulator, regulator_center);
        self.component_rotations.insert(regulator, 0.0);
        
        // Calculate spacing based on component pin geometry
        let regulator_pin_offset = self.estimate_pin_offset_for_component(regulator, 
            self.find_pin_by_name(regulator, "VIN").unwrap_or_default());
        let cap_pin_offset = if let Some(&cap_id) = input_caps.first() {
            self.estimate_pin_offset_for_component(cap_id, 
                self.find_pin_by_name(cap_id, "1").unwrap_or_default())
        } else {
            Point::new(20.0, 0.0) // Default capacitor pin offset
        };
        
        // Calculate spacing to align pins perfectly
        let horizontal_spacing = regulator_pin_offset.x.abs() + cap_pin_offset.x.abs() + 20.0; // 20.0 minimum clearance
        let pin_alignment_offset = 20.0; // For 90° rotated caps, this aligns pin 1 with regulator pins
        
        // Place input capacitors to the left with perfect pin alignment
        for (i, &cap_id) in input_caps.iter().enumerate() {
            let cap_x = regulator_center.x - horizontal_spacing - (i as f64 * 60.0);
            let cap_y = regulator_center.y + pin_alignment_offset; // Align cap pin 1 with regulator pin
            
            positions.insert(cap_id, Point::new(cap_x, cap_y));
            self.component_rotations.insert(cap_id, 90.0); // Rotate for vertical pin orientation
        }
        
        // Place output capacitors to the right with perfect pin alignment
        for (i, &cap_id) in output_caps.iter().enumerate() {
            let cap_x = regulator_center.x + horizontal_spacing + (i as f64 * 60.0);
            let cap_y = regulator_center.y + pin_alignment_offset; // Align cap pin 1 with regulator pin
            
            positions.insert(cap_id, Point::new(cap_x, cap_y));
            self.component_rotations.insert(cap_id, 90.0); // Rotate for vertical pin orientation
        }
        
        // Place feedback components below the regulator with proportional spacing
        for (i, &fb_id) in feedback.iter().enumerate() {
            let fb_x = regulator_center.x + (i as f64 * 40.0);
            let fb_y = regulator_center.y + 60.0;
            positions.insert(fb_id, Point::new(fb_x, fb_y));
            self.component_rotations.insert(fb_id, 0.0);
        }
        
        // Place power symbols with constraint-based positioning
        if let Some(vin_id) = self.find_power_source_instance() {
            // Align VIN symbol with input capacitor X position for straight vertical routing
            let vin_x = if let Some(&cap_id) = input_caps.first() {
                positions.get(&cap_id).map(|p| p.x).unwrap_or(regulator_center.x - horizontal_spacing)
            } else {
                regulator_center.x - horizontal_spacing
            };
            let vin_y = regulator_center.y - 80.0; // Above circuit with good clearance
            positions.insert(vin_id, Point::new(vin_x, vin_y));
            self.component_rotations.insert(vin_id, 0.0);
        }
        
        if let Some(vout_id) = self.find_power_output_instance() {
            // Align VOUT symbol with output capacitor X position for straight vertical routing
            let vout_x = if let Some(&cap_id) = output_caps.first() {
                positions.get(&cap_id).map(|p| p.x).unwrap_or(regulator_center.x + horizontal_spacing)
            } else {
                regulator_center.x + horizontal_spacing
            };
            let vout_y = regulator_center.y - 80.0; // Above circuit with good clearance
            positions.insert(vout_id, Point::new(vout_x, vout_y));
            self.component_rotations.insert(vout_id, 0.0);
        }
        
        // Place ground symbol below with center alignment for optimal routing
        if let Some(ground_id) = self.find_ground_instance() {
            let ground_x = regulator_center.x; // Center align with regulator
            let ground_y = regulator_center.y + 100.0; // Below all components
            positions.insert(ground_id, Point::new(ground_x, ground_y));
            self.component_rotations.insert(ground_id, 0.0);
        }
    }

    fn layout_op_amp_stage(
        &self,
        op_amp: InstanceId,
        input_network: &[InstanceId],
        feedback_network: &[InstanceId],
        output_network: &[InstanceId],
        positions: &mut HashMap<InstanceId, Point>
    ) {
        // Place op-amp at center
        positions.insert(op_amp, Point::new(0.0, 0.0));

        // Place input network to the left
        for (i, &comp_id) in input_network.iter().enumerate() {
            positions.insert(comp_id, Point::new(-80.0, i as f64 * 30.0 - 15.0));
        }

        // Place feedback network above
        for (i, &comp_id) in feedback_network.iter().enumerate() {
            positions.insert(comp_id, Point::new(i as f64 * 40.0 - 20.0, 60.0));
        }

        // Place output network to the right
        for (i, &comp_id) in output_network.iter().enumerate() {
            positions.insert(comp_id, Point::new(80.0, i as f64 * 30.0 - 15.0));
        }
    }

    fn layout_microcontroller_core(
        &self,
        mcu: InstanceId,
        power_components: &[InstanceId],
        decoupling_caps: &[InstanceId],
        crystal: &Option<InstanceId>,
        reset_circuit: &[InstanceId],
        positions: &mut HashMap<InstanceId, Point>
    ) {
        // Place MCU at center
        positions.insert(mcu, Point::new(0.0, 0.0));

        // Place power components above
        for (i, &comp_id) in power_components.iter().enumerate() {
            positions.insert(comp_id, Point::new(i as f64 * 30.0 - 15.0, 80.0));
        }

        // Place decoupling caps around MCU
        let radius = 40.0;
        for (i, &cap_id) in decoupling_caps.iter().enumerate() {
            let angle = (i as f64) * 2.0 * std::f64::consts::PI / decoupling_caps.len() as f64;
            let x = radius * angle.cos();
            let y = radius * angle.sin();
            positions.insert(cap_id, Point::new(x, y));
        }

        // Place crystal to the right
        if let Some(crystal_id) = crystal {
            positions.insert(*crystal_id, Point::new(60.0, 0.0));
        }

        // Place reset circuit below
        for (i, &comp_id) in reset_circuit.iter().enumerate() {
            positions.insert(comp_id, Point::new(i as f64 * 25.0 - 12.5, -60.0));
        }
    }

    fn layout_differential_pair(
        &self,
        _positive_net: NetId,
        _negative_net: NetId,
        components: &[InstanceId],
        positions: &mut HashMap<InstanceId, Point>
    ) {
        // Place components symmetrically for differential pairs
        let center_y = 0.0;
        let spacing = 20.0;
        
        for (i, &comp_id) in components.iter().enumerate() {
            let y_offset = if i % 2 == 0 { spacing } else { -spacing };
            let x = i as f64 * 40.0;
            positions.insert(comp_id, Point::new(x, center_y + y_offset));
        }
    }

    fn layout_power_distribution(
        &self,
        power_nets: &[NetId],
        ground_nets: &[NetId],
        components: &[InstanceId],
        positions: &mut HashMap<InstanceId, Point>
    ) {
        // Optimized ground placement: position ground symbols near their connections
        for &comp_id in components {
            if let Some(instance) = self.analyzer.netlist.instances.get(comp_id) {
                if let Some(module) = self.analyzer.netlist.modules.get(instance.definition) {
                    if module.name.to_lowercase().contains("ground") || module.name.to_lowercase().contains("gnd") {
                        // Only optimize ground position if it hasn't been positioned by PowerRegulator pattern
                        if !positions.contains_key(&comp_id) {
                            // This is a ground symbol - find optimal position based on connections
                            let optimal_pos = self.find_optimal_ground_position(comp_id, positions);
                            positions.insert(comp_id, optimal_pos);
                        }
                    } else {
                        // For non-ground components, only position them if they haven't been positioned already
                        // This prevents PowerDistribution from overriding PowerRegulator or SignalChain positioning
                        if !positions.contains_key(&comp_id) {
                            // Check if this is a power symbol that should be positioned by PowerRegulator
                            let is_power_symbol = if let Some(instance) = self.analyzer.netlist.instances.get(comp_id) {
                                if let Some(module) = self.analyzer.netlist.modules.get(instance.definition) {
                                    let name_lower = module.name.to_lowercase();
                                    name_lower.contains("vcc") || name_lower.contains("vdd") || 
                                    name_lower.contains("power") || name_lower.contains("vin") || 
                                    name_lower.contains("vout")
                                } else { false }
                            } else { false };
                            
                            // Skip positioning power symbols - let PowerRegulator handle them
                            if !is_power_symbol {
                                                            // Non-ground power components use grid layout with dynamic spacing
                            let component_count = components.len() as f64;
                            let grid_spacing = 50.0 + (component_count * 10.0); // Dynamic spacing based on component count
                                let start_x = -(components.len() as f64) * grid_spacing / 2.0;
                                let index = components.iter().position(|&id| id == comp_id).unwrap_or(0);
                                let x = start_x + (index as f64) * grid_spacing;
                                let y = if power_nets.len() > ground_nets.len() { 220.0 } else { 380.0 }; // Move further from signal chain
                                positions.insert(comp_id, Point::new(x, y));
                            }
                        }
                    }
                }
            }
        }
    }
    
    fn find_optimal_ground_position(&self, ground_instance_id: InstanceId, existing_positions: &HashMap<InstanceId, Point>) -> Point {
        // Find all pin positions that connect to this ground symbol
        let connected_pin_positions = self.find_ground_connection_pin_positions(ground_instance_id, existing_positions);
        

        
        if connected_pin_positions.is_empty() {
            // No connections found, use default position
            return Point::new(0.0, 360.0);
        }
        
        // Generate candidate positions for ground symbol and evaluate routing cost for each
        let candidate_positions = self.generate_ground_placement_candidates(&connected_pin_positions);
        

        
        // Find the position with minimum routing cost
        let mut best_position = Point::new(0.0, 360.0);
        let mut min_cost = f64::INFINITY;
        
        for candidate in candidate_positions.iter() {
            let cost = self.calculate_routing_cost(&connected_pin_positions, candidate);
            if cost < min_cost {
                min_cost = cost;
                best_position = *candidate;
            }
        }
        

        best_position
    }
    
    fn find_ground_connection_pin_positions(&self, ground_instance_id: InstanceId, existing_positions: &HashMap<InstanceId, Point>) -> Vec<Point> {
        let mut pin_positions = Vec::new();
        
        // Find nets that the ground symbol is connected to
        for (_net_id, net) in &self.analyzer.netlist.nets {
            let ground_connected = net.connections.iter().any(|conn| {
                if let ConnectionPoint::InstancePin(inst_id, _) = conn {
                    *inst_id == ground_instance_id
                } else {
                    false
                }
            });
            
            if ground_connected {
                // Find actual pin positions of other components on this net
                for conn in &net.connections {
                    if let ConnectionPoint::InstancePin(inst_id, pin_id) = conn {
                        if *inst_id != ground_instance_id {
                            if let Some(pin_pos) = self.calculate_world_pin_position(*inst_id, *pin_id, existing_positions) {
                                pin_positions.push(pin_pos);
                            }
                        }
                    }
                }
            }
        }
        
        pin_positions
    }
    
    fn calculate_world_pin_position(&self, instance_id: InstanceId, pin_id: PinId, positions: &HashMap<InstanceId, Point>) -> Option<Point> {
        // Get component position
        let component_pos = positions.get(&instance_id)?;
        
        // Get pin offset within component (this would ideally come from component definition)
        // For now, use simple heuristics based on pin name/index
        let pin_offset = self.estimate_pin_offset_for_component(instance_id, pin_id);
        
        Some(Point::new(component_pos.x + pin_offset.x, component_pos.y + pin_offset.y))
    }
    
    fn estimate_pin_offset_for_component(&self, instance_id: InstanceId, pin_id: PinId) -> Point {
        // Get component type to estimate pin positions
        if let Some(instance) = self.analyzer.netlist.instances.get(instance_id) {
            if let Some(module) = self.analyzer.netlist.modules.get(instance.definition) {
                if let Some(pin) = self.analyzer.netlist.pins.get(pin_id) {
                    // Enhanced pin position estimation based on component type and pin name
                    let component_type = self.get_component_type(instance_id);
                    match component_type {
                        ComponentType::Resistor | ComponentType::Capacitor => {
                            // Horizontal 2-pin components with pins at ±20 from center
                            match pin.name.as_str() {
                                "1" => Point::new(-20.0, 0.0),  // Left pin
                                "2" => Point::new(20.0, 0.0),   // Right pin
                                _ => Point::new(0.0, 0.0),
                            }
                        }
                        ComponentType::Ground => {
                            // Ground symbols have pin at top of symbol
                            Point::new(0.0, -10.0)  // Pin above symbol center
                        }
                        ComponentType::MOSFET => {
                            // MOSFET typical pin layout: Gate(left), Drain(top), Source(bottom)
                            match pin.name.to_lowercase().as_str() {
                                "d" | "drain" => Point::new(0.0, -15.0),   // Drain at top
                                "s" | "source" => Point::new(0.0, 15.0),   // Source at bottom  
                                "g" | "gate" => Point::new(-20.0, 0.0),    // Gate at left
                                "1" => Point::new(-20.0, 0.0),  // Assume pin 1 is gate
                                "2" => Point::new(0.0, -15.0),  // Assume pin 2 is drain
                                "3" => Point::new(0.0, 15.0),   // Assume pin 3 is source
                                _ => Point::new(0.0, 0.0),
                            }
                        }
                        ComponentType::VoltageRegulator => {
                            // Voltage regulator typical layout: Input(left), Output(right), Ground(bottom)
                            match pin.name.to_lowercase().as_str() {
                                "vin" | "in" | "input" | "1" => Point::new(-20.0, 0.0),  // Input at left
                                "vout" | "out" | "output" | "2" => Point::new(20.0, 0.0), // Output at right
                                "gnd" | "ground" | "3" => Point::new(0.0, 15.0),          // Ground at bottom
                                _ => Point::new(0.0, 0.0),
                            }
                        }
                        ComponentType::OpAmp => {
                            // Op-amp typical layout: +IN(left top), -IN(left bottom), OUT(right)
                            match pin.name.to_lowercase().as_str() {
                                "in+" | "inp" | "noninv" | "1" => Point::new(-20.0, -10.0), // +IN at left top
                                "in-" | "inn" | "inv" | "2" => Point::new(-20.0, 10.0),     // -IN at left bottom
                                "out" | "output" | "3" => Point::new(20.0, 0.0),            // OUT at right
                                "vcc" | "vdd" | "4" => Point::new(0.0, -15.0),              // Power at top
                                "vss" | "gnd" | "5" => Point::new(0.0, 15.0),               // Ground at bottom
                                _ => Point::new(0.0, 0.0),
                            }
                        }
                        ComponentType::Generic => {
                            // Generic component - try to infer from pin name
                            let pin_name = pin.name.to_lowercase(); 
                            if pin_name.contains("in") || pin_name == "1" {
                                Point::new(-20.0, 0.0)  // Input typically on left
                            } else if pin_name.contains("out") || pin_name == "2" {
                                Point::new(20.0, 0.0)   // Output typically on right
                            } else if pin_name.contains("gnd") || pin_name.contains("ground") {
                                Point::new(0.0, 15.0)   // Ground typically at bottom
                            } else if pin_name.contains("vcc") || pin_name.contains("vdd") {
                                Point::new(0.0, -15.0)  // Power typically at top
                            } else {
                                Point::new(0.0, 0.0)    // Default to center
                            }
                        }
                    }
                } else {
                    Point::new(0.0, 0.0)
                }
            } else {
                Point::new(0.0, 0.0)
            }
        } else {
            Point::new(0.0, 0.0)
        }
    }
    
    fn generate_ground_placement_candidates(&self, connected_pins: &[Point]) -> Vec<Point> {
        if connected_pins.is_empty() {
            return vec![Point::new(0.0, 360.0)];
        }
        
        let mut candidates = Vec::new();
        
        // Define canvas boundaries (with some margin for component size)
        let canvas_margin = 30.0;  // Margin to ensure component is fully visible
        let min_canvas_x = canvas_margin;
        let max_canvas_x = 800.0 - canvas_margin;  // SVG width is 800
        let min_canvas_y = canvas_margin;
        let max_canvas_y = 600.0 - canvas_margin;  // SVG height is 600
        
        // Calculate bounding box of connected pins
        let min_x = connected_pins.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
        let max_x = connected_pins.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
        let min_y = connected_pins.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
        let max_y = connected_pins.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
        
        let clearance = 50.0;  // Minimum clearance from other components
        
        // For ground symbols: prefer placement BELOW connected components
        // Ground pins are at the TOP of the symbol, so wires come DOWN to ground
        for pin in connected_pins {
            // Priority candidates: BELOW the connected pins (conventional ground placement)
            candidates.push(Point::new(pin.x, pin.y + clearance));           // Directly below
            candidates.push(Point::new(pin.x + clearance/2.0, pin.y + clearance));  // Below-right
            candidates.push(Point::new(pin.x - clearance/2.0, pin.y + clearance));  // Below-left
            candidates.push(Point::new(pin.x, pin.y + clearance * 1.5));     // Further below
            
            // Secondary candidates: to the sides (same level)
            candidates.push(Point::new(pin.x + clearance, pin.y));          // Right  
            candidates.push(Point::new(pin.x - clearance, pin.y));          // Left
            
            // Avoid placing ground ABOVE components (unconventional and causes routing through symbol)
            // Only add above candidates if below options don't work
            if pin.y + clearance > max_canvas_y {
                candidates.push(Point::new(pin.x, pin.y - clearance));      // Above (fallback only)
            }
        }
        
        // Add candidates based on bounding box and individual pin positions
        let center_x = (min_x + max_x) / 2.0;
        let center_y = (min_y + max_y) / 2.0;
        
        // Generate candidates near each individual connected pin for optimal routing
        for pin in connected_pins {
            // Primary: directly below each connected pin (best for straight-down routing)
            candidates.push(Point::new(pin.x, pin.y + clearance));
            candidates.push(Point::new(pin.x, pin.y + clearance * 1.5));
            
            // Secondary: slightly offset from each pin (for avoiding overlap)
            candidates.push(Point::new(pin.x + clearance/3.0, pin.y + clearance));
            candidates.push(Point::new(pin.x - clearance/3.0, pin.y + clearance));
        }
        
        // Cluster-based candidates (lower priority than individual pin placement)
        // Primary: below the component cluster  
        candidates.push(Point::new(center_x, max_y + clearance));           // Below center
        candidates.push(Point::new(center_x, max_y + clearance * 1.5));     // Further below
        
        // Secondary: to the sides  
        candidates.push(Point::new(max_x + clearance, center_y));           // Right of center
        candidates.push(Point::new(min_x - clearance, center_y));           // Left of center
        
        // Tertiary: above (only if below doesn't fit)
        if max_y + clearance > max_canvas_y {
            candidates.push(Point::new(center_x, min_y - clearance));       // Above center (fallback)
        }
        
        // Filter candidates to ensure they're within canvas bounds
        candidates.into_iter()
            .filter(|candidate| {
                candidate.x >= min_canvas_x && candidate.x <= max_canvas_x &&
                candidate.y >= min_canvas_y && candidate.y <= max_canvas_y
            })
            .collect()
    }
    
    fn calculate_routing_cost(&self, connected_pins: &[Point], ground_position: &Point) -> f64 {
        let mut total_cost = 0.0;
        
        for pin in connected_pins {
            // Calculate cost for routing from this pin to ground position
            let route_cost = self.calculate_single_route_cost(pin, ground_position);
            total_cost += route_cost;
        }
        
        total_cost
    }
    
    fn calculate_single_route_cost(&self, from: &Point, to: &Point) -> f64 {
        let dx = (to.x - from.x).abs();
        let dy = (to.y - from.y).abs();
        
        // Cost factors
        let length_cost = dx + dy;  // Manhattan distance
        let bend_cost = if dx > 1.0 && dy > 1.0 { 20.0 } else { 0.0 };  // Penalty for L-shapes vs straight lines
        
        // Canvas boundary penalty - heavily penalize routes that would go off-screen
        let canvas_margin = 30.0;
        let mut boundary_penalty = 0.0;
        
        // Check if any part of the routing path would go outside canvas bounds
        if to.x < canvas_margin || to.x > (800.0 - canvas_margin) ||
           to.y < canvas_margin || to.y > (600.0 - canvas_margin) {
            boundary_penalty += 1000.0;  // Heavy penalty for off-screen placement
        }
        
        // Additional penalty if the routing path itself would go off-screen
        // For L-shaped routing, check intermediate points
        if dx > 1.0 && dy > 1.0 {
            // Check the two possible L-shaped paths
            let intermediate1 = Point::new(from.x, to.y);  // Vertical first, then horizontal
            let intermediate2 = Point::new(to.x, from.y);  // Horizontal first, then vertical
            
            if intermediate1.x < canvas_margin || intermediate1.x > (800.0 - canvas_margin) ||
               intermediate1.y < canvas_margin || intermediate1.y > (600.0 - canvas_margin) {
                boundary_penalty += 500.0;
            }
            
            if intermediate2.x < canvas_margin || intermediate2.x > (800.0 - canvas_margin) ||
               intermediate2.y < canvas_margin || intermediate2.y > (600.0 - canvas_margin) {
                boundary_penalty += 500.0;
            }
        }
        
        // Ground-specific routing penalties
        let mut ground_penalty = 0.0;
        
        // The 'to' point represents the ground pin, which is at the TOP of the ground symbol
        // Ground symbol center is at 'to + Point::new(0.0, 10.0)' (pin is -10 offset)
        let ground_symbol_center = Point::new(to.x, to.y + 10.0);
        
        // Penalty for routing that goes "through" the ground symbol
        // If from.y > to.y, the wire is coming from below and going up to the ground pin
        // This means routing THROUGH the ground symbol, which is visually incorrect
        if from.y > ground_symbol_center.y {
            ground_penalty += 200.0;  // Heavy penalty for routing through the symbol
        }
        
        // Bonus for conventional ground placement (ground below the component)
        if to.y > from.y {
            ground_penalty -= 50.0;  // Bonus for placing ground below the connected component
        }
        
        // Strong bonus for straight down routing (most conventional and efficient)
        if dx < 5.0 && to.y > from.y {
            ground_penalty -= 75.0;  // Strong bonus for straight down to ground
        }
        
        // Additional bonus for minimal horizontal offset (reduces wire length and bends)
        if dx < 20.0 && to.y > from.y {
            ground_penalty -= 30.0;  // Bonus for nearby horizontal placement
        }
        
        length_cost + bend_cost + boundary_penalty + ground_penalty
    }
    


    fn layout_signal_chain(
        &self,
        chain: &[InstanceId],
        _signal_path: &[NetId],
        positions: &mut HashMap<InstanceId, Point>
    ) {
        
        // Check if any components in this chain are part of a power regulator pattern
        let mut regulator_components = HashSet::new();
        for pattern in &self.analyzer.patterns {
            if let CircuitPattern::PowerRegulator { regulator, input_caps, output_caps, feedback } = pattern {
                regulator_components.insert(*regulator);
                regulator_components.extend(input_caps);
                regulator_components.extend(output_caps);
                regulator_components.extend(feedback);
            }
        }
        
        // Also exclude power symbols that have been positioned by power regulator layout
        let power_source = self.find_power_source_instance();
        let power_output = self.find_power_output_instance();
        let ground_instance = self.find_ground_instance();
        
        if let Some(ps) = power_source {
            regulator_components.insert(ps);
        }
        if let Some(po) = power_output {
            regulator_components.insert(po);
        }
        if let Some(gi) = ground_instance {
            regulator_components.insert(gi);
        }
        
        // Only position components that are NOT part of a power regulator pattern
        let mut non_regulator_chain = Vec::new();
        for &instance_id in chain {
            if !regulator_components.contains(&instance_id) {
                non_regulator_chain.push(instance_id);
            }
        }
        
        if non_regulator_chain.is_empty() {
            return; // All components are part of power regulator, don't override
        }
        
        // Place non-regulator components in a horizontal line from left to right with much larger spacing
        let spacing = 200.0; // Much larger spacing to prevent overlaps and routing conflicts
        let start_x = -(non_regulator_chain.len() as f64 - 1.0) * spacing / 2.0;
        let y = 300.0; // Fixed Y position for horizontal alignment
        
        for (i, &instance_id) in non_regulator_chain.iter().enumerate() {
            let x = start_x + (i as f64) * spacing;
            positions.insert(instance_id, Point::new(x, y));
            
            // Check if this is a ground component and position it below
            if let Some(instance) = self.analyzer.netlist.instances.get(instance_id) {
                if let Some(def) = self.analyzer.netlist.modules.get(instance.definition) {
                    if def.name.to_lowercase().contains("ground") || def.name.to_lowercase().contains("gnd") {
                        // Position ground components below the signal chain
                        positions.insert(instance_id, Point::new(x, y + 60.0));
                    }
                }
            }
        }
    }

    fn optimize_ground_placements_final_pass(&self, positions: &mut HashMap<InstanceId, Point>) {
        // Find all components that could benefit from pin-aware optimization
        let mut optimizable_components = Vec::new();
        
        for (comp_id, _) in positions.iter() {
            if let Some(instance) = self.analyzer.netlist.instances.get(*comp_id) {
                if let Some(module) = self.analyzer.netlist.modules.get(instance.definition) {
                    let name = module.name.to_lowercase();
                    // Identify components that have specific pin positioning requirements
                    // BUT exclude voltage regulators and ground symbols which are already positioned by PowerRegulator pattern
                    if (name.contains("mosfet") || name.contains("transistor") ||
                        name.contains("opamp")) && 
                       !name.contains("regulator") && 
                       !name.contains("ground") && !name.contains("gnd") {
                        optimizable_components.push(*comp_id);
                    }
                }
            }
        }
        
        // Re-optimizing pin-aware components in final pass
        
        // Re-optimize each component's position based on current component positions and pin layout
        for comp_id in optimizable_components {
            let optimal_pos = self.find_optimal_pin_aware_position(comp_id, positions);
            positions.insert(comp_id, optimal_pos);
        }
    }

    // ============ GENERIC PIN-AWARE PLACEMENT FUNCTIONS ============
    
    fn find_optimal_pin_aware_position(&self, target_instance_id: InstanceId, existing_positions: &HashMap<InstanceId, Point>) -> Point {
        // Find all pin positions that connect to this component
        let connected_pin_positions = self.find_component_connection_pin_positions(target_instance_id, existing_positions);
        
        if connected_pin_positions.is_empty() {
            // No connections found, use current position if it's reasonable, otherwise skip optimization
            if let Some(current_pos) = existing_positions.get(&target_instance_id) {
                // If the current position is not at the origin (0,0), keep it
                if current_pos.x.abs() > 1.0 || current_pos.y.abs() > 1.0 {
                    return *current_pos;
                }
            }
            // Only use default if no reasonable position exists
            return Point::new(0.0, 360.0);
        }
        
        // Get component type to understand its pin layout
        let component_type = self.get_component_type(target_instance_id);
        
        // Generate candidate positions based on component pin layout and connections
        let candidate_positions = self.generate_pin_aware_placement_candidates(
            &connected_pin_positions, 
            target_instance_id,
            &component_type
        );
        
        // Find the position with minimum routing cost
        let mut best_position = existing_positions.get(&target_instance_id)
            .cloned()
            .unwrap_or(Point::new(0.0, 360.0));
        let mut min_cost = f64::INFINITY;
        
        for candidate in candidate_positions.iter() {
            let cost = self.calculate_pin_aware_routing_cost(
                &connected_pin_positions, 
                candidate, 
                target_instance_id,
                &component_type
            );
            if cost < min_cost {
                min_cost = cost;
                best_position = *candidate;
            }
        }

        best_position
    }
    
    fn find_component_connection_pin_positions(&self, target_instance_id: InstanceId, existing_positions: &HashMap<InstanceId, Point>) -> Vec<Point> {
        let mut pin_positions = Vec::new();
        
        // Find nets that the target component is connected to
        for (_net_id, net) in &self.analyzer.netlist.nets {
            let target_connected = net.connections.iter().any(|conn| {
                if let ConnectionPoint::InstancePin(inst_id, _) = conn {
                    *inst_id == target_instance_id
                } else {
                    false
                }
            });
            
            if target_connected {
                // Find actual pin positions of other components on this net
                for conn in &net.connections {
                    if let ConnectionPoint::InstancePin(inst_id, pin_id) = conn {
                        if *inst_id != target_instance_id {
                            if let Some(pin_pos) = self.calculate_world_pin_position(*inst_id, *pin_id, existing_positions) {
                                pin_positions.push(pin_pos);
                            }
                        }
                    }
                }
            }
        }
        
        pin_positions
    }
    
    fn get_component_type(&self, instance_id: InstanceId) -> ComponentType {
        if let Some(instance) = self.analyzer.netlist.instances.get(instance_id) {
            if let Some(module) = self.analyzer.netlist.modules.get(instance.definition) {
                let name = module.name.to_lowercase();
                if name.contains("ground") || name.contains("gnd") {
                    return ComponentType::Ground;
                } else if name.contains("mosfet") || name.contains("transistor") {
                    return ComponentType::MOSFET;
                } else if name.contains("regulator") {
                    return ComponentType::VoltageRegulator;
                } else if name.contains("opamp") {
                    return ComponentType::OpAmp;
                } else if name.contains("resistor") {
                    return ComponentType::Resistor;
                } else if name.contains("capacitor") {
                    return ComponentType::Capacitor;
                }
            }
        }
        ComponentType::Generic
    }
    
    fn generate_pin_aware_placement_candidates(&self, connected_pins: &[Point], target_instance_id: InstanceId, component_type: &ComponentType) -> Vec<Point> {
        if connected_pins.is_empty() {
            return vec![Point::new(0.0, 360.0)];
        }
        
        let mut candidates = Vec::new();
        let clearance = 50.0;
        let canvas_margin = 30.0;
        let max_canvas_x = 800.0 - canvas_margin;
        let max_canvas_y = 600.0 - canvas_margin;
        let min_canvas_x = canvas_margin;
        let min_canvas_y = canvas_margin;
        
        // Get the pin directions for this component type
        let pin_directions = self.get_component_pin_directions(target_instance_id, component_type);
        
        for pin in connected_pins {
            // Generate candidates based on component pin directions
            for direction in &pin_directions {
                match direction {
                    PinDirection::Top => {
                        // Pin is at top of symbol, so place component BELOW connected components
                        candidates.push(Point::new(pin.x, pin.y + clearance));
                        candidates.push(Point::new(pin.x, pin.y + clearance * 1.5));
                        candidates.push(Point::new(pin.x + clearance/3.0, pin.y + clearance));
                        candidates.push(Point::new(pin.x - clearance/3.0, pin.y + clearance));
                    }
                    PinDirection::Bottom => {
                        // Pin is at bottom of symbol, so place component ABOVE connected components
                        candidates.push(Point::new(pin.x, pin.y - clearance));
                        candidates.push(Point::new(pin.x, pin.y - clearance * 1.5));
                        candidates.push(Point::new(pin.x + clearance/3.0, pin.y - clearance));
                        candidates.push(Point::new(pin.x - clearance/3.0, pin.y - clearance));
                    }
                    PinDirection::Left => {
                        // Pin is at left of symbol, so place component to the RIGHT of connected components
                        candidates.push(Point::new(pin.x + clearance, pin.y));
                        candidates.push(Point::new(pin.x + clearance * 1.5, pin.y));
                        candidates.push(Point::new(pin.x + clearance, pin.y + clearance/3.0));
                        candidates.push(Point::new(pin.x + clearance, pin.y - clearance/3.0));
                    }
                    PinDirection::Right => {
                        // Pin is at right of symbol, so place component to the LEFT of connected components
                        candidates.push(Point::new(pin.x - clearance, pin.y));
                        candidates.push(Point::new(pin.x - clearance * 1.5, pin.y));
                        candidates.push(Point::new(pin.x - clearance, pin.y + clearance/3.0));
                        candidates.push(Point::new(pin.x - clearance, pin.y - clearance/3.0));
                    }
                }
            }
        }
        
        // Filter candidates to ensure they're within canvas bounds
        candidates.into_iter()
            .filter(|candidate| {
                candidate.x >= min_canvas_x && candidate.x <= max_canvas_x &&
                candidate.y >= min_canvas_y && candidate.y <= max_canvas_y
            })
            .collect()
    }
    
    fn get_component_pin_directions(&self, instance_id: InstanceId, component_type: &ComponentType) -> Vec<PinDirection> {
        match component_type {
            ComponentType::Ground => vec![PinDirection::Top], // Ground pin typically at top of symbol
            ComponentType::MOSFET => vec![PinDirection::Top, PinDirection::Bottom], // Drain at top, source at bottom
            ComponentType::VoltageRegulator => vec![PinDirection::Left, PinDirection::Right], // Input left, output right
            ComponentType::OpAmp => vec![PinDirection::Left, PinDirection::Right], // Inputs left, output right
            ComponentType::Resistor | ComponentType::Capacitor => vec![PinDirection::Left, PinDirection::Right], // Horizontal 2-pin
            ComponentType::Generic => vec![PinDirection::Top, PinDirection::Bottom, PinDirection::Left, PinDirection::Right], // All directions
        }
    }
    
    fn calculate_pin_aware_routing_cost(&self, connected_pins: &[Point], target_position: &Point, target_instance_id: InstanceId, component_type: &ComponentType) -> f64 {
        let mut total_cost = 0.0;
        
        for pin in connected_pins {
            // Calculate cost for routing from this pin to target component position
            let route_cost = self.calculate_component_route_cost(pin, target_position, target_instance_id, component_type);
            total_cost += route_cost;
        }
        
        total_cost
    }
    
    fn calculate_component_route_cost(&self, from: &Point, to: &Point, target_instance_id: InstanceId, component_type: &ComponentType) -> f64 {
        let dx = (to.x - from.x).abs();
        let dy = (to.y - from.y).abs();
        
        // Base routing cost
        let length_cost = dx + dy;  // Manhattan distance
        let bend_cost = if dx > 1.0 && dy > 1.0 { 20.0 } else { 0.0 };
        
        // Canvas boundary penalty
        let canvas_margin = 30.0;
        let mut boundary_penalty = 0.0;
        if to.x < canvas_margin || to.x > (800.0 - canvas_margin) ||
           to.y < canvas_margin || to.y > (600.0 - canvas_margin) {
            boundary_penalty += 1000.0;
        }
        
        // Component-specific routing bonuses/penalties
        let mut component_penalty = 0.0;
        match component_type {
            ComponentType::Ground => {
                // Ground symbols: prefer placing below connected components (conventional)
                if to.y > from.y {
                    component_penalty -= 50.0; // Bonus for conventional ground placement
                }
                if dx < 5.0 && to.y > from.y {
                    component_penalty -= 75.0; // Strong bonus for straight down to ground
                }
            }
            ComponentType::MOSFET => {
                // MOSFETs: prefer alignment for clean power flow
                if dy < 5.0 {
                    component_penalty -= 30.0; // Bonus for horizontal alignment
                }
            }
            ComponentType::VoltageRegulator => {
                // Voltage regulators: prefer left-to-right signal flow
                if to.x > from.x {
                    component_penalty -= 40.0; // Bonus for left-to-right placement
                }
            }
            ComponentType::OpAmp => {
                // Op-amps: prefer left-to-right signal flow
                if to.x > from.x {
                    component_penalty -= 40.0; // Bonus for left-to-right placement  
                }
            }
            _ => {
                // Generic components: prefer straight connections
                if dx < 5.0 || dy < 5.0 {
                    component_penalty -= 20.0; // Bonus for straight connections
                }
            }
        }
        
        length_cost + bend_cost + boundary_penalty + component_penalty
    }

    fn find_power_source_instance(&self) -> Option<InstanceId> {
        for (instance_id, instance) in &self.analyzer.netlist.instances {
            if let Some(def) = self.analyzer.netlist.modules.get(instance.definition) {
                let module_name = def.name.to_lowercase();
                if (module_name.contains("power") || module_name == "power") && !module_name.contains("regulator") {
                    // Check if this is likely an input power source based on connections
                    if self.is_power_input_instance(instance_id) {
                        return Some(instance_id);
                    }
                }
            }
        }
        None
    }

    fn find_power_output_instance(&self) -> Option<InstanceId> {
        for (instance_id, instance) in &self.analyzer.netlist.instances {
            if let Some(def) = self.analyzer.netlist.modules.get(instance.definition) {
                let module_name = def.name.to_lowercase();
                if (module_name.contains("power") || module_name == "power") && !module_name.contains("regulator") {
                    // Check if this is likely an output power rail based on connections
                    if !self.is_power_input_instance(instance_id) {
                        return Some(instance_id);
                    }
                }
            }
        }
        None
    }

    fn find_ground_instance(&self) -> Option<InstanceId> {
        for (instance_id, instance) in &self.analyzer.netlist.instances {
            if let Some(def) = self.analyzer.netlist.modules.get(instance.definition) {
                if def.name.to_lowercase().contains("ground") || def.name.to_lowercase().contains("gnd") {
                    return Some(instance_id);
                }
            }
        }
        None
    }

    fn is_power_input_instance(&self, instance_id: InstanceId) -> bool {
        // Check nets connected to this instance to determine if it's input or output power
        let connected_nets = self.analyzer.find_nets_connected_to_instance(instance_id);
        
        for net_id in connected_nets {
            if let Some(net) = self.analyzer.netlist.nets.get(net_id) {
                if let Some(net_name) = &net.name {
                    // VIN, VCC, VDD are typically input power
                    if net_name.to_lowercase().contains("vin") || 
                       net_name.to_lowercase().contains("vcc") ||
                       net_name.to_lowercase().contains("vdd") {
                        return true;
                    }
                    // VOUT is typically output power
                    if net_name.to_lowercase().contains("vout") {
                        return false;
                    }
                }
            }
        }
        
        // Default: assume first power instance found is input
        true
    }

    pub fn apply_global_constraints(&self, positions: &mut HashMap<InstanceId, Point>) {
        // Apply global semantic constraints
        if self.constraints.power_rails_vertical {
            // Align power components vertically
        }
        
        if self.constraints.signal_flow_left_to_right {
            // Ensure signal flow goes from left to right
        }
    }

    pub fn get_component_rotations(&self) -> &HashMap<InstanceId, f64> {
        &self.component_rotations
    }

    fn find_pin_by_name(&self, instance_id: InstanceId, pin_name: &str) -> Option<PinId> {
        if let Some(instance) = self.analyzer.netlist.instances.get(instance_id) {
            if let Some(module) = self.analyzer.netlist.modules.get(instance.definition) {
                for &pin_id in &module.pins {
                    if let Some(pin) = self.analyzer.netlist.pins.get(pin_id) {
                        if pin.name == pin_name {
                            return Some(pin_id);
                        }
                    }
                }
            }
        }
        None
    }
}

// ============ COMPONENT TYPE DEFINITIONS ============

#[derive(Debug, Clone, PartialEq)]
enum ComponentType {
    Ground,
    MOSFET,
    VoltageRegulator,
    OpAmp,
    Resistor,
    Capacitor,
    Generic,
}

#[derive(Debug, Clone, PartialEq)]
enum PinDirection {
    Top,
    Bottom,
    Left,
    Right,
} 