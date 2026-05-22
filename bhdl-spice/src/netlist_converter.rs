//! Netlist to SPICE Circuit Converter
//! 
//! This module provides enhanced conversion from BHDL netlists to SPICE circuits,
//! leveraging the component model extraction system for accurate models.

use std::collections::HashMap;
use anyhow::{Result, Context};
use log::{debug, info, warn};

use crate::{
    Circuit, ComponentModelExtractor, ExtractedModel,
    circuit::{
        META_PARENT_INSTANCE, META_DECOMPOSITION_ROLE, META_COMPONENT_CLASS,
        META_RDS_ON, META_F_SW, META_T_SW, META_I_QUIESCENT,
        META_TOLERANCE, META_POWER_RATING, META_ESR, META_VOLTAGE_RATING, META_DCR,
        META_SATURATION_CURRENT, META_EMISSION_COEFFICIENT, META_THERMAL_VOLTAGE,
        META_FORWARD_VOLTAGE, META_FORWARD_CURRENT,
        META_MAX_CURRENT, META_MAX_VOLTAGE, META_MAX_POWER, META_TEMP_MIN, META_TEMP_MAX,
        META_VARIANT,
    },
    model_factory::SpiceModelFactory,
    models::SpiceModel,
};
use bhdl_netlist::{
    Netlist, NetId, InstanceId,
    ConnectionPoint, ModuleKind, ModuleDefinition,
};

/// Pin connection info for an instance, including pin type and direction
struct PinNetInfo {
    net_id: NetId,
    net_name: String,
    pin_name: String,
    pin_type: bhdl_netlist::PinType,
    pin_direction: bhdl_netlist::PinDirection,
}

/// Enhanced netlist converter with proper SPICE model creation
pub struct NetlistToSpiceConverter {
    /// Component model extractor
    model_extractor: ComponentModelExtractor,
    /// Model factory for creating SPICE models
    model_factory: SpiceModelFactory,
    /// Cached models for instances
    instance_models: HashMap<InstanceId, Box<dyn SpiceModel>>,
    /// Symbol table data from analyzer (if available)
    symbol_table: HashMap<String, HashMap<String, String>>,
    /// Component registry for type lookup
    component_registry: crate::component_registry::ComponentRegistry,
}

impl NetlistToSpiceConverter {
    /// Create new converter
    pub fn new() -> Self {
        Self {
            model_extractor: ComponentModelExtractor::new(),
            model_factory: SpiceModelFactory::new(),
            instance_models: HashMap::new(),
            symbol_table: HashMap::new(),
            component_registry: crate::component_registry::ComponentRegistry::new(),
        }
    }
    
    /// Set symbol table data from analyzer
    pub fn set_symbol_table(&mut self, symbol_table: HashMap<String, HashMap<String, String>>) {
        self.symbol_table = symbol_table;
    }
    
    /// Convert BHDL netlist to SPICE circuit with proper models
    pub fn convert(&mut self, netlist: &Netlist) -> Result<Circuit> {
        let mut circuit = Circuit::new();

        info!("Converting netlist to SPICE circuit with {} instances", netlist.instances.len());

        // Step 1: Add all nets as nodes
        self.add_nets_as_nodes(&mut circuit, netlist)?;

        // Step 2: Process regulators first (they define voltage sources on output nets)
        let mut vsource_nodes: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut deferred: Vec<(InstanceId, &bhdl_netlist::Instance)> = Vec::new();

        for (instance_id, instance) in &netlist.instances {
            let module = netlist.modules.get(instance.definition);
            let is_regulator = module.map(|m| {
                self.component_registry.get_component_type(&m.name, &instance.attributes)
                    == Some(crate::components::ComponentType::VoltageRegulator)
            }).unwrap_or(false);

            if is_regulator {
                // Use pin types from stdlib to identify regulator output
                let pin_info = Self::get_pin_net_info(netlist, instance_id);
                let reg_output_net = pin_info.iter()
                    .find(|p| p.pin_type == bhdl_netlist::PinType::Power
                           && p.pin_direction == bhdl_netlist::PinDirection::Out)
                    .map(|p| p.net_name.clone());

                info!("Regulator pre-scan for {}: output_net={:?} (from pin types: {:?})",
                      instance.name, reg_output_net,
                      pin_info.iter().map(|p| format!("{}:{:?}/{:?}→{}", p.pin_name, p.pin_type, p.pin_direction, p.net_name)).collect::<Vec<_>>());

                match self.process_instance(&mut circuit, netlist, instance_id, instance) {
                    Ok(_) => {
                        info!("Successfully processed regulator: {}", instance.name);
                        if let Some(output_net) = reg_output_net {
                            vsource_nodes.insert(output_net.clone());
                            info!("Regulator {} drives output net: {}", instance.name, output_net);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to process regulator {}: {}", instance.name, e);
                    }
                }
            } else {
                deferred.push((instance_id, instance));
            }
        }

        // Step 2b: Propagate vsource_nodes through inductors from virtual pin
        //          expansion.  After expansion, a buck's SW node is in vsource_nodes
        //          but the original VOUT net (connected via inductor) is not.  We
        //          need to mark those too so power symbols don't create redundant
        //          voltage sources.
        for (instance_id, instance) in &netlist.instances {
            let module = netlist.modules.get(instance.definition);
            let is_inductor = module.map(|m| {
                m.name.contains("Inductor") || m.name.contains("Ind")
                    || instance.attributes.get("component_class")
                        .map(|c| c == "inductor")
                        .unwrap_or(false)
            }).unwrap_or(false);

            if is_inductor {
                let pin_nets = Self::get_pin_net_info(netlist, instance_id);
                let net_names: Vec<String> = pin_nets.iter().map(|p| p.net_name.clone()).collect();
                // If one end is already in vsource_nodes, add the other end too
                for i in 0..net_names.len() {
                    if vsource_nodes.contains(&net_names[i]) {
                        for j in 0..net_names.len() {
                            if i != j && !vsource_nodes.contains(&net_names[j]) {
                                info!("Inductor {} propagates voltage-source-driven status: {} → {}",
                                      instance.name, net_names[i], net_names[j]);
                                vsource_nodes.insert(net_names[j].clone());
                            }
                        }
                    }
                }
            }
        }

        // Step 3: Process remaining instances, skipping power symbols whose nets are
        //         already driven by a regulator voltage source
        for (instance_id, instance) in deferred {
            let module = netlist.modules.get(instance.definition);
            let is_power_symbol = module.map(|m| Self::is_power_symbol(&m.name)).unwrap_or(false);

            if is_power_symbol {
                let connected = self.get_connected_nets(netlist, instance_id).unwrap_or_default();
                if let Some((net_id, net_name)) = connected.first() {
                    // Skip if net already driven by a regulator voltage source
                    if vsource_nodes.contains(net_name) {
                        info!("Skipping power symbol {} — net '{}' already driven by regulator",
                              instance.name, net_name);
                        continue;
                    }
                    // Skip if net is floating (only the power symbol is connected, no load)
                    let net_connection_count = netlist.nets.get(*net_id)
                        .map(|n| n.connections.len())
                        .unwrap_or(0);
                    if net_connection_count <= 1 {
                        info!("Skipping power symbol {} — net '{}' is floating ({} connections)",
                              instance.name, net_name, net_connection_count);
                        continue;
                    }
                }
            }

            match self.process_instance(&mut circuit, netlist, instance_id, instance) {
                Ok(_) => info!("Successfully processed instance: {}", instance.name),
                Err(e) => {
                    warn!("Failed to process instance {}: {}", instance.name, e);
                }
            }
        }

        info!("Created SPICE circuit with {} nodes and {} components",
             circuit.nodes().count(), circuit.branches().count());

        Ok(circuit)
    }
    
    /// Add nets as circuit nodes, skipping floating nets (those with < 2 connections)
    fn add_nets_as_nodes(&self, circuit: &mut Circuit, netlist: &Netlist) -> Result<()> {
        for (net_id, net) in &netlist.nets {
            // Skip nets with fewer than 2 connections — they're floating and would
            // create underconstrained equations in MNA. They'll be added lazily if
            // a branch references them later.
            if net.connections.len() < 2 {
                let name = net.name.as_deref().unwrap_or("unnamed");
                debug!("Skipping floating net '{}' ({} connections)", name, net.connections.len());
                continue;
            }
            let name = net.name.clone()
                .unwrap_or_else(|| format!("net_{:?}", net_id));
            circuit.add_node(name, Some(net_id));
        }
        Ok(())
    }
    
    /// Parse voltage from a power symbol name like "+12V", "+3V3", "+5V"
    fn parse_power_symbol_voltage(name: &str) -> Option<f64> {
        let trimmed = name.trim_start_matches('+');

        // Handle "3V3" style: replace 'V' with '.' → "3.3"
        // This covers: "3V3" → 3.3, "1V8" → 1.8, "2V5" → 2.5
        if trimmed.contains('V') {
            let normalized = trimmed.replace('V', ".");
            // Strip trailing '.' from cases like "12V" → "12."
            let normalized = normalized.trim_end_matches('.');
            if let Ok(v) = normalized.parse::<f64>() {
                return Some(v);
            }
        }

        // Try parsing directly (e.g., just a number)
        if let Ok(v) = trimmed.parse::<f64>() {
            return Some(v);
        }

        None
    }

    /// Check if a module name represents a power symbol
    fn is_power_symbol(module_name: &str) -> bool {
        module_name.starts_with('+') && module_name.len() > 1
    }

    /// Check if a module name represents a ground symbol
    fn is_ground_symbol(module_name: &str) -> bool {
        let lower = module_name.to_lowercase();
        lower == "gnd" || lower == "ground" || lower == "vss" || lower == "0"
    }

    /// Process a single instance and create SPICE model
    fn process_instance(
        &mut self,
        circuit: &mut Circuit,
        netlist: &Netlist,
        instance_id: InstanceId,
        instance: &bhdl_netlist::Instance,
    ) -> Result<()> {
        let module = netlist.modules.get(instance.definition)
            .ok_or_else(|| anyhow::anyhow!("Module not found for instance {}", instance.name))?;

        info!("Processing instance {} with module kind {:?}, module name: {}",
              instance.name, module.kind, module.name);

        // Get connected nets for this instance
        let connected_nets = self.get_connected_nets(netlist, instance_id)?;

        info!("Instance {} has {} connected nets: {:?}",
              instance.name, connected_nets.len(),
              connected_nets.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>());

        // Special handling: power symbols (+12V, +5V, +3V3, etc.)
        // These are 1-pin components that represent voltage sources to GND
        if Self::is_power_symbol(&module.name) {
            if let Some(voltage) = Self::parse_power_symbol_voltage(&module.name) {
                if let Some((_, net_name)) = connected_nets.first() {
                    // Ensure GND node exists
                    circuit.add_node("GND".to_string(), None);
                    // Add voltage source from net to GND
                    circuit.add_branch(
                        instance.name.clone(),
                        net_name,
                        "GND",
                        "VoltageSource".to_string(),
                        voltage,
                        Some(instance_id),
                    );
                    info!("Added power symbol {} as VoltageSource {}V: {} -> GND",
                          instance.name, voltage, net_name);
                }
            } else {
                warn!("Could not parse voltage from power symbol: {}", module.name);
            }
            return Ok(());
        }

        // Special handling: ground symbols
        // GND instances just mark their connected net as ground — the node is already added
        if Self::is_ground_symbol(&module.name) {
            debug!("Skipping ground symbol instance: {} (ground node already exists)", instance.name);
            return Ok(());
        }

        // Extract model based on available information
        let extracted_model = self.extract_model_for_instance(
            &instance.name,
            module,
            &instance.attributes,
        )?;

        // Create SPICE model
        let spice_model = self.model_extractor.create_spice_model(&extracted_model)?;

        // Handle different component types
        match module.kind {
            ModuleKind::PhysicalComponent | ModuleKind::Component => {
                self.add_physical_component(
                    circuit,
                    netlist,
                    &instance.name,
                    instance_id,
                    &connected_nets,
                    extracted_model,
                )?;
            }
            ModuleKind::Interface => {
                // Interfaces might represent connectors or test points
                if module.name.to_lowercase().contains("test") ||
                   module.name.to_lowercase().contains("point") ||
                   module.name.to_lowercase().contains("connector") {
                    self.add_physical_component(
                        circuit,
                        netlist,
                        &instance.name,
                        instance_id,
                        &connected_nets,
                        extracted_model,
                    )?;
                } else {
                    debug!("Skipping interface module: {}", instance.name);
                }
            }
            ModuleKind::Module => {
                // For now, skip logical modules
                debug!("Skipping logical module: {}", instance.name);
            }
            _ => {
                debug!("Skipping module kind {:?} for {}", module.kind, instance.name);
            }
        }

        // Cache the model
        self.instance_models.insert(instance_id, spice_model);

        Ok(())
    }
    
    /// Extract model for an instance
    fn extract_model_for_instance(
        &mut self,
        instance_name: &str,
        module: &ModuleDefinition,
        attributes: &HashMap<String, String>,
    ) -> Result<ExtractedModel> {
        // First try symbol table if available
        if let Some(symbol_data) = self.symbol_table.get(instance_name) {
            if let Ok(model) = self.model_extractor.extract_from_symbol_table(instance_name, symbol_data) {
                return Ok(model);
            }
        }
        
        // Then try user attributes
        if !attributes.is_empty() {
            if let Ok(model) = self.model_extractor.extract_from_user_attributes(instance_name, attributes) {
                return Ok(model);
            }
        }
        
        // Build extraction data from module and instance attributes
        let mut extraction_data = HashMap::new();
        
        // Add instance name
        extraction_data.insert("name".to_string(), instance_name.to_string());
        
        // Add module name for registry lookup
        extraction_data.insert("type".to_string(), module.name.clone());
        
        // Add instance attributes (these override module attributes)
        for (key, value) in attributes {
            extraction_data.insert(key.clone(), value.clone());
        }
        
        // Add module attributes
        for (key, value) in &module.attributes {
            extraction_data.insert(key.clone(), value.clone());
        }
        
        // Try to extract from the combined data
        self.model_extractor.extract_from_data(extraction_data)
    }
    
    /// Get nets connected to an instance
    fn get_connected_nets(
        &self,
        netlist: &Netlist,
        instance_id: InstanceId,
    ) -> Result<Vec<(NetId, String)>> {
        let mut connected_nets = Vec::new();

        // Look at pin instances for this instance
        for (_pin_inst_id, pin_inst) in &netlist.pin_instances {
            if pin_inst.instance == instance_id {
                if let Some(net_id) = pin_inst.net {
                    if let Some(net) = netlist.nets.get(net_id) {
                        let net_name = net.name.clone()
                            .unwrap_or_else(|| format!("net_{:?}", net_id));

                        // Add the net if not already present
                        if !connected_nets.iter().any(|(id, _)| *id == net_id) {
                            connected_nets.push((net_id, net_name));
                        }
                    }
                } else {
                    // Log unconnected pins for debugging
                    let pin_name = netlist.pins.get(pin_inst.pin_def)
                        .map(|p| p.name.as_str())
                        .unwrap_or("?");
                    debug!("Pin {} of instance {:?} has no net assigned", pin_name, instance_id);
                }
            }
        }

        // Also check net connections pointing to this instance's pins
        for (_net_id, net) in &netlist.nets {
            for conn in &net.connections {
                match conn {
                    ConnectionPoint::PinInstance(pi_id) => {
                        if let Some(pi) = netlist.pin_instances.get(*pi_id) {
                            if pi.instance == instance_id {
                                let net_name = net.name.clone()
                                    .unwrap_or_else(|| format!("net_{:?}", _net_id));
                                if !connected_nets.iter().any(|(id, _)| *id == _net_id) {
                                    connected_nets.push((_net_id, net_name));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        debug!("Instance {:?} has {} connected nets", instance_id, connected_nets.len());

        Ok(connected_nets)
    }
    
    /// Map ComponentType to SPICE equation system string
    fn component_type_to_spice_string(ct: &crate::components::ComponentType) -> String {
        use crate::components::ComponentType;
        match ct {
            ComponentType::Resistor => "Resistor".to_string(),
            ComponentType::Capacitor => "Capacitor".to_string(),
            ComponentType::Inductor => "Inductor".to_string(),
            ComponentType::Diode => "Diode".to_string(),
            ComponentType::LED => "LED".to_string(),
            ComponentType::VoltageSource => "VoltageSource".to_string(),
            ComponentType::CurrentSource => "CurrentSource".to_string(),
            ComponentType::VoltageRegulator => "VoltageRegulator".to_string(),
            ComponentType::BJT => "BJT".to_string(),
            ComponentType::MOSFET => "MOSFET".to_string(),
            ComponentType::OpAmp => "OpAmp".to_string(),
            ComponentType::Other(s) => s.clone(),
        }
    }

    /// Get connected nets with full pin metadata for an instance.
    /// Uses both pin_inst.net and net.connections for completeness.
    fn get_pin_net_info(
        netlist: &Netlist,
        instance_id: InstanceId,
    ) -> Vec<PinNetInfo> {
        let mut result = Vec::new();
        let mut seen_pins: std::collections::HashSet<bhdl_netlist::PinId> = std::collections::HashSet::new();

        // Pass 1: pin_inst.net (direct assignment)
        for (_pi_id, pin_inst) in &netlist.pin_instances {
            if pin_inst.instance == instance_id {
                if let Some(net_id) = pin_inst.net {
                    if let (Some(net), Some(pin_def)) = (netlist.nets.get(net_id), netlist.pins.get(pin_inst.pin_def)) {
                        seen_pins.insert(pin_inst.pin_def);
                        let net_name = net.name.clone().unwrap_or_else(|| format!("net_{:?}", net_id));
                        result.push(PinNetInfo {
                            net_id,
                            net_name,
                            pin_name: pin_def.name.clone(),
                            pin_type: pin_def.pin_type,
                            pin_direction: pin_def.direction,
                        });
                    }
                }
            }
        }

        // Pass 2: net.connections → PinInstance (fills gaps where pin_inst.net is null)
        for (net_id, net) in &netlist.nets {
            for conn in &net.connections {
                if let ConnectionPoint::PinInstance(pi_id) = conn {
                    if let Some(pi) = netlist.pin_instances.get(*pi_id) {
                        if pi.instance == instance_id && !seen_pins.contains(&pi.pin_def) {
                            if let Some(pin_def) = netlist.pins.get(pi.pin_def) {
                                seen_pins.insert(pi.pin_def);
                                let net_name = net.name.clone().unwrap_or_else(|| format!("net_{:?}", net_id));
                                result.push(PinNetInfo {
                                    net_id,
                                    net_name,
                                    pin_name: pin_def.name.clone(),
                                    pin_type: pin_def.pin_type,
                                    pin_direction: pin_def.direction,
                                });
                            }
                        }
                    }
                }
            }
        }

        result
    }

    /// Add a physical component to the circuit
    fn add_physical_component(
        &self,
        circuit: &mut Circuit,
        netlist: &Netlist,
        instance_name: &str,
        instance_id: InstanceId,
        connected_nets: &[(NetId, String)],
        extracted_model: ExtractedModel,
    ) -> Result<()> {
        use crate::components::ComponentType;

        let spice_type = Self::component_type_to_spice_string(&extracted_model.component_type);

        // Special handling for voltage regulators (3-terminal)
        // Use pin types from stdlib: power-in = VIN, power-out = VOUT, ground = GND
        if extracted_model.component_type == ComponentType::VoltageRegulator {
            let vout_voltage = extracted_model.parameters.get("vout").copied()
                .or(extracted_model.parameters.get("output_voltage").copied())
                .or(extracted_model.parameters.get("voltage").copied())
                .unwrap_or(5.0);

            let pin_info = Self::get_pin_net_info(netlist, instance_id);

            // Classify pins by their type and direction from stdlib definitions
            let vin_net = pin_info.iter()
                .find(|p| p.pin_type == bhdl_netlist::PinType::Power
                       && (p.pin_direction == bhdl_netlist::PinDirection::In
                           || p.pin_direction == bhdl_netlist::PinDirection::InOut))
                .map(|p| p.net_name.clone());
            let vout_net = pin_info.iter()
                .find(|p| p.pin_type == bhdl_netlist::PinType::Power
                       && p.pin_direction == bhdl_netlist::PinDirection::Out)
                .map(|p| p.net_name.clone());
            let gnd_node_name = pin_info.iter()
                .find(|p| p.pin_type == bhdl_netlist::PinType::Ground)
                .map(|p| p.net_name.clone())
                .unwrap_or_else(|| "GND".to_string());

            circuit.add_node(gnd_node_name.clone(), None);

            info!("Regulator {} decomposition (from pin types): VIN={:?}, VOUT={:?}, GND={}, vout_voltage={}V",
                  instance_name, vin_net, vout_net, gnd_node_name, vout_voltage);

            // Determine regulator type and extract loss model parameters
            let component_class = extracted_model.attributes.get("component_class")
                .cloned()
                .unwrap_or_default();
            let is_switching = component_class == "switching_regulator";

            // Helper: read a parameter from extracted_model (f64) or attributes (string)
            let read_param = |name: &str, default: f64| -> f64 {
                extracted_model.parameters.get(name).copied()
                    .or_else(|| extracted_model.attributes.get(name)
                        .and_then(|s| s.parse::<f64>().ok()))
                    .unwrap_or(default)
            };

            // Add voltage source on output: VOUT → GND = vout_voltage
            if let Some(vout_name) = &vout_net {
                let mut vout_meta = HashMap::new();
                vout_meta.insert(META_PARENT_INSTANCE.to_string(), instance_name.to_string());
                vout_meta.insert(META_DECOMPOSITION_ROLE.to_string(), "vout".to_string());
                if !component_class.is_empty() {
                    vout_meta.insert(META_COMPONENT_CLASS.to_string(), component_class.clone());
                }
                // Store device loss model parameters for post-simulation power
                // computation from physics, not lumped estimates.
                // I_quiescent applies to both linear and switching regulators.
                vout_meta.insert(META_I_QUIESCENT.to_string(), read_param("i_quiescent", 5e-3).to_string());
                if is_switching {
                    vout_meta.insert(META_RDS_ON.to_string(), read_param("rds_on", 0.2).to_string());
                    vout_meta.insert(META_F_SW.to_string(), read_param("f_sw", 500e3).to_string());
                    vout_meta.insert(META_T_SW.to_string(), read_param("t_sw", 80e-9).to_string());
                }
                circuit.add_branch_with_metadata(
                    format!("{}_vout", instance_name),
                    vout_name,
                    &gnd_node_name,
                    "VoltageSource".to_string(),
                    vout_voltage,
                    Some(instance_id),
                    vout_meta,
                );
                info!("Added regulator {} output as VoltageSource {}V: {} -> {}",
                      instance_name, vout_voltage, vout_name, gnd_node_name);
            }

            // Model the regulator's internal VIN→VOUT dropout path as a high resistance.
            // This provides necessary circuit connectivity for MNA solver stability
            // but must NOT create a significant parallel current path — the voltage
            // source at VOUT already sets the output; this resistor only ensures the
            // VIN node has a DC path to the rest of the circuit.
            // Use 10kΩ: high enough that parasitic current is negligible (~0.7mA at 7V drop)
            // but low enough for MNA numerical stability.
            if let (Some(vin_name), Some(vout_name)) = (&vin_net, &vout_net) {
                let internal_resistance = 10_000.0; // 10kΩ — connectivity only, not a power path
                let mut dropout_meta = HashMap::new();
                dropout_meta.insert(META_PARENT_INSTANCE.to_string(), instance_name.to_string());
                dropout_meta.insert(META_DECOMPOSITION_ROLE.to_string(), "dropout".to_string());
                circuit.add_branch_with_metadata(
                    format!("{}_dropout", instance_name),
                    vin_name,
                    vout_name,
                    "Resistor".to_string(),
                    internal_resistance,
                    Some(instance_id),
                    dropout_meta,
                );
                info!("Added regulator {} dropout path ({:.1}Ω): {} -> {}",
                      instance_name, internal_resistance, vin_name, vout_name);
            }

            return Ok(());
        }

        // For 2-terminal components
        if connected_nets.len() >= 2 {
            // Get primary value from extracted model
            let value = self.get_primary_value(&extracted_model);

            // For diode-type components, use pin metadata to determine anode/cathode
            // ordering. SPICE convention: node1 = anode, node2 = cathode.
            let (node1, node2) = if matches!(extracted_model.component_type,
                ComponentType::Diode | ComponentType::LED)
            {
                let pin_info = Self::get_pin_net_info(netlist, instance_id);
                let anode_net = pin_info.iter()
                    .find(|p| {
                        let name = p.pin_name.to_uppercase();
                        name == "A" || name == "ANODE"
                            || p.pin_direction == bhdl_netlist::PinDirection::In
                    })
                    .map(|p| p.net_name.clone());
                let cathode_net = pin_info.iter()
                    .find(|p| {
                        let name = p.pin_name.to_uppercase();
                        name == "K" || name == "CATHODE"
                            || p.pin_direction == bhdl_netlist::PinDirection::Out
                    })
                    .map(|p| p.net_name.clone());

                match (anode_net, cathode_net) {
                    (Some(anode), Some(cathode)) => {
                        info!("Diode {} pin-aware ordering: anode={}, cathode={}",
                              instance_name, anode, cathode);
                        (anode, cathode)
                    }
                    _ => {
                        // Fallback to connection order if pin metadata is ambiguous
                        warn!("Diode {} has ambiguous pin metadata, using connection order", instance_name);
                        (connected_nets[0].1.clone(), connected_nets[1].1.clone())
                    }
                }
            } else {
                // Symmetric components (resistors, capacitors): order doesn't matter
                (connected_nets[0].1.clone(), connected_nets[1].1.clone())
            };

            info!("Adding component {} ({}): {} -> {}, value={}",
                  instance_name, spice_type, node1, node2, value);

            let metadata = build_branch_metadata(&extracted_model);
            circuit.add_branch_with_metadata(
                instance_name.to_string(),
                &node1,
                &node2,
                spice_type,
                value,
                Some(instance_id),
                metadata,
            );
        } else if connected_nets.len() == 1 {
            // Single-pin components (like test points)
            debug!("Single-pin component {}: connected to {}",
                   instance_name, connected_nets[0].1);
        } else {
            warn!("Component {} has no connections", instance_name);
        }

        Ok(())
    }
    
    /// Infer component type from module name
    fn infer_component_type(&self, module_name: &str) -> String {
        let lower = module_name.to_lowercase();
        if lower.contains("voltage") && lower.contains("source") {
            "voltage_source".to_string()
        } else if lower.contains("res") || lower.starts_with('r') {
            "resistor".to_string()
        } else if lower.contains("cap") || lower.starts_with('c') {
            "capacitor".to_string()
        } else if lower.contains("ind") || lower.starts_with('l') {
            "inductor".to_string()
        } else if lower.contains("led") {
            "led".to_string()
        } else if lower.contains("diode") || lower.starts_with('d') {
            "diode".to_string()
        } else if lower.starts_with('v') {
            "voltage_source".to_string()
        } else {
            module_name.to_string()
        }
    }
    
    /// Extract value from component name (e.g., "Res_10k" -> "10k")
    fn extract_value_from_name(&self, name: &str) -> Option<String> {
        // Look for patterns like _10k, _100n, etc.
        if let Some(underscore_pos) = name.rfind('_') {
            let value_part = &name[underscore_pos + 1..];
            // Check if it looks like a value
            if value_part.chars().next()?.is_numeric() {
                return Some(value_part.to_string());
            }
        }
        
        // Look for embedded values like "R10k" or "C100n"
        if name.len() > 1 {
            let without_prefix = &name[1..];
            if without_prefix.chars().next().map(|c| c.is_numeric()).unwrap_or(false) {
                return Some(without_prefix.to_string());
            }
        }
        
        None
    }
    
    /// Get primary value from extracted model
    fn get_primary_value(&self, model: &ExtractedModel) -> f64 {
        use crate::components::ComponentType;
        
        let value = match model.component_type {
            ComponentType::Resistor => {
                model.parameters.get("resistance").copied().unwrap_or(1e3)
            }
            ComponentType::Capacitor => {
                model.parameters.get("capacitance").copied().unwrap_or(1e-6)
            }
            ComponentType::Inductor => {
                model.parameters.get("inductance").copied().unwrap_or(1e-6)
            }
            ComponentType::VoltageSource => {
                model.parameters.get("voltage").copied().unwrap_or(5.0)
            }
            _ => 1.0,
        };
        
        debug!("Primary value for {:?}: {}", model.component_type, value);
        value
    }
}

impl Default for NetlistToSpiceConverter {
    fn default() -> Self {
        Self::new()
    }
}

/// Enhanced Circuit::from_netlist using the converter
impl Circuit {
    /// Create SPICE circuit from BHDL netlist with proper models
    pub fn from_netlist_with_models(netlist: &Netlist) -> Result<Self> {
        let mut converter = NetlistToSpiceConverter::new();
        converter.convert(netlist)
    }
}

/// Project a synthesizer-produced `ExtractedModel` onto the per-branch metadata
/// HashMap consumed downstream by `stdlib_model_loader`.
///
/// This is the actual "stdlib drives SPICE behavior" bridge: stdlib `.bhdl`
/// attributes (e.g. `attribute esr = 0.05;`) reach the synthesizer, which
/// produces an `ExtractedModel` with `parameters` (numeric) and `attributes`
/// (string). This function maps the well-known names from those collections
/// onto stable `META_*` keys, which the model loader then reads to populate
/// `ComponentModel::*` variants.
///
/// Unknown attributes are intentionally not passed through — metadata is a
/// solver contract, not a free-form bag. New SPICE-relevant attributes should
/// be added to `circuit.rs` (the `META_*` constants) and to this function in
/// the same change.
fn build_branch_metadata(model: &ExtractedModel) -> HashMap<String, String> {
    let mut meta = HashMap::new();

    // Component class is the only string-typed key worth pulling through
    // verbatim today; everything else is numeric.
    if let Some(class) = model.attributes.get("component_class") {
        meta.insert(META_COMPONENT_CLASS.to_string(), class.clone());
    }

    // Read a numeric attribute. Prefer `parameters` (typed `f64`); fall back to
    // `attributes` only if the string parses cleanly as `f64`. This matches
    // the convention already established by the regulator path's `read_param`.
    let put_num = |meta: &mut HashMap<String, String>, src: &str, dst: &str| {
        if let Some(v) = model.parameters.get(src) {
            meta.insert(dst.to_string(), v.to_string());
        } else if let Some(s) = model.attributes.get(src) {
            if s.parse::<f64>().is_ok() {
                meta.insert(dst.to_string(), s.clone());
            }
        }
    };
    let put_str = |meta: &mut HashMap<String, String>, src: &str, dst: &str| {
        if let Some(s) = model.attributes.get(src) {
            meta.insert(dst.to_string(), s.clone());
        }
    };

    // Passive & semiconductor parameters that affect AC/transient solves
    // or that downstream safety analyses read out of branch metadata.
    put_num(&mut meta, "tolerance",            META_TOLERANCE);
    put_num(&mut meta, "power_rating",         META_POWER_RATING);
    put_num(&mut meta, "esr",                  META_ESR);
    put_num(&mut meta, "voltage_rating",       META_VOLTAGE_RATING);
    put_num(&mut meta, "dcr",                  META_DCR);
    put_num(&mut meta, "saturation_current",   META_SATURATION_CURRENT);
    put_num(&mut meta, "emission_coefficient", META_EMISSION_COEFFICIENT);
    put_num(&mut meta, "thermal_voltage",      META_THERMAL_VOLTAGE);
    put_num(&mut meta, "forward_voltage",      META_FORWARD_VOLTAGE);
    put_num(&mut meta, "forward_current",      META_FORWARD_CURRENT);
    put_num(&mut meta, "max_current",          META_MAX_CURRENT);
    put_num(&mut meta, "max_voltage",          META_MAX_VOLTAGE);
    put_num(&mut meta, "max_power",            META_MAX_POWER);
    put_num(&mut meta, "temp_min",             META_TEMP_MIN);
    put_num(&mut meta, "temp_max",             META_TEMP_MAX);

    // Free-form variant tag (LED `color`, future diode/transistor families).
    // The loader uses this to dispatch into fallback Rust LUTs only when the
    // numeric attributes above are not present.
    put_str(&mut meta, "color",   META_VARIANT);
    put_str(&mut meta, "variant", META_VARIANT);

    meta
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_value_extraction() {
        let converter = NetlistToSpiceConverter::new();
        
        assert_eq!(converter.extract_value_from_name("Res_10k"), Some("10k".to_string()));
        assert_eq!(converter.extract_value_from_name("Cap_100nF"), Some("100nF".to_string()));
        assert_eq!(converter.extract_value_from_name("R10k"), Some("10k".to_string()));
        assert_eq!(converter.extract_value_from_name("C100n"), Some("100n".to_string()));
        assert_eq!(converter.extract_value_from_name("LED"), None);
    }
    
    #[test]
    fn test_component_type_inference() {
        let converter = NetlistToSpiceConverter::new();
        
        assert_eq!(converter.infer_component_type("Resistor"), "resistor");
        assert_eq!(converter.infer_component_type("Res_10k"), "resistor");
        assert_eq!(converter.infer_component_type("Cap_100n"), "capacitor");
        assert_eq!(converter.infer_component_type("LED_Red"), "led");
        assert_eq!(converter.infer_component_type("Diode_1N4148"), "diode");
    }
}