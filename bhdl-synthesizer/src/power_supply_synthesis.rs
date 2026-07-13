//! Power-supply IC supporting-component synthesis for `NetlistGenerator`.
//!
//! When the netlist contains a power-supply IC (buck regulator,
//! LDO, charge pump, …) this module synthesises the *supporting
//! components* it needs to be operational: input/output decoupling
//! caps, inductor, feedback resistors, compensation network, etc.
//! The IC's required passives come from `component_calculator`'s
//! `CalculatedComponent` records, sized against the source's
//! `power VIN = X V; power VOUT = Y V;` declarations.
//!
//! Methods extracted from `lib.rs` on 2026-05-26 to keep the
//! main pipeline compile unit small. The cluster is a clean
//! contiguous group (lines 793..1081 of pre-split lib.rs) with
//! exactly one live caller (`generate_supporting_component_
//! connections`) and three dead helpers retained for symmetry —
//! a future tidy pass can remove the unused entries.
//!
//! Visibility: methods are `pub(crate)` so the main pipeline can
//! invoke them. Not `pub` — supporting-component synthesis is an
//! internal pipeline phase, not part of the synthesizer's
//! external surface.

use super::*;

impl NetlistGenerator {

    /// Extract power specifications from analysis results for a given IC
    pub(crate) async fn extract_power_specifications(&self, analysis: &AnalysisResult, ic_type: &str) -> Result<Option<PowerSupplySpec>> {
        // Extract power domain information from the analysis
        // Look for power declarations like "power VIN = 24V @ 3A;" and "power VOUT = 12V @ 2.5A;"
        
        debug!("Extracting power specifications for IC: {}", ic_type);
        
        let mut input_voltage = None;
        let mut output_voltage = None;
        let mut output_current = None;
        
        // Search through all symbols for power domains (both global scope and definition scopes)
        let mut all_symbols = analysis.global_scope.get_symbols().clone();
        
        // Add symbols from all definition scopes (power domains are stored in board definition scope)
        for (_node_ptr, scope) in bhdl_analyzer::definition_scopes_sorted(&analysis.definition_scopes) {
            for (name, symbol) in scope.get_symbols() {
                all_symbols.insert(name.clone(), symbol.clone());
            }
            
            // Check if this scope has nets separately (nets have their own namespace)
            for (name, symbol) in scope.get_nets() {
                all_symbols.insert(name.clone(), symbol.clone());
            }
        }
        
        debug!("Searching for power domains in {} symbols", all_symbols.len());
        
        for (name, symbol) in &all_symbols {
            if matches!(symbol.kind, bhdl_analyzer::symbol_table::SymbolKind::Net) {
                if let Some(ref net_attribute) = symbol.net_attributes {
                    // Check for input power domain (VIN, VCC, etc.)
                    let name_upper = name.to_uppercase();
                    if name_upper.contains("VIN") || name_upper.contains("INPUT") {
                        if let Some(voltage) = net_attribute.voltage() {
                            input_voltage = Some(voltage);
                            debug!("Found input voltage: {}V", voltage);
                        }
                    }
                    // Check for output power domain (VOUT, etc.)
                    else if name_upper.contains("VOUT") || name_upper.contains("OUTPUT") {
                        if let Some(voltage) = net_attribute.voltage() {
                            output_voltage = Some(voltage);
                            debug!("Found output voltage: {}V", voltage);
                        }
                        if let Some(current) = net_attribute.max_current() {
                            output_current = Some(current);
                            debug!("Found output current: {}A", current);
                        }
                    }
                }
            }
        }
        
        // Create power specification if we have the required information
        if let (Some(v_in), Some(v_out), Some(i_out)) = (input_voltage, output_voltage, output_current) {
            let power_spec = PowerSupplySpec {
                input_voltage: v_in,
                output_voltage: v_out,
                output_current: i_out,
                switching_frequency: self.get_default_switching_frequency(ic_type),
                ripple_spec: 0.100,      // 100mVpp (conservative spec)
                transient_spec: 100.0,   // 100µs (conservative spec)
                efficiency_target: self.get_default_efficiency(ic_type),
            };
            
            Ok(Some(power_spec))
        } else {
            debug!("Insufficient power domain information - VIN: {:?}, VOUT: {:?}, IOUT: {:?}", 
                   input_voltage, output_voltage, output_current);
            Ok(None)
        }
    }

    /// Get default switching frequency for a given IC type
    pub(crate) fn get_default_switching_frequency(&self, ic_type: &str) -> f64 {
        let type_lower = ic_type.to_lowercase();
        
        if type_lower.contains("tps543") {
            400_000.0  // 400kHz for TPS54331
        } else if type_lower.contains("lm2596") {
            150_000.0  // 150kHz for LM2596
        } else {
            500_000.0  // 500kHz default for modern switchers
        }
    }

    /// Get default efficiency for a given IC type
    pub(crate) fn get_default_efficiency(&self, ic_type: &str) -> f64 {
        let type_lower = ic_type.to_lowercase();
        
        if type_lower.contains("tps543") {
            0.91  // 91% for TPS54331
        } else if type_lower.contains("lm2596") {
            0.85  // 85% for LM2596 
        } else if type_lower.contains("7805") || type_lower.contains("lm317") {
            0.60  // 60% for linear regulators (poor efficiency)
        } else {
            0.88  // 88% default for modern switchers
        }
    }

    /// Add a calculated component to the netlist
    pub(crate) fn add_calculated_component_to_netlist(&mut self, component: &crate::component_calculator::CalculatedComponent, ic_name: &str) -> Result<()> {
        // Create module for the component type
        let component_type = format!("{:?}", component.component_type); // Convert enum to string
        let module_id = self.get_or_create_module(&component_type, ModuleKind::Component)?;
        
        // Create instance with calculated reference designator
        let instance_name = format!("{}_{}", ic_name, component.reference); // e.g., "U1_C1"
        
        if let Some(instance_id) = self.netlist.add_instance(instance_name.clone(), module_id) {
            debug!("Created calculated component: {} -> {:?} ({})", instance_name, instance_id, component.value);
            
            // Store the instance ID for later connection generation
            self.supporting_component_instances.insert(instance_name.clone(), instance_id);
            
            // Add component metadata as annotations
            // The visualizer can read these annotations to understand component values and purposes
            // TODO: Add proper metadata storage to netlist when available
            
        } else {
            warn!("Failed to create instance for calculated component: {}", instance_name);
        }
        
        Ok(())
    }

    /// Generate connections for supporting components from virtual pin expansion
    pub(crate) fn generate_supporting_component_connections(&mut self, analysis: &AnalysisResult) -> Result<()> {
        if self.supporting_component_instances.is_empty() {
            info!("No supporting components to connect");
            return Ok(()); // No supporting components to connect
        }
        
        info!("Generating connections for {} supporting components", self.supporting_component_instances.len());
        for (name, id) in &self.supporting_component_instances {
            info!("  Supporting component: {} -> {:?}", name, id);
        }
        
        // Group components by the IC they belong to.
        // Prefer vpin_parent attribute; fall back to name prefix for legacy components.
        let mut components_by_ic: HashMap<String, Vec<(String, InstanceId)>> = HashMap::new();
        for (name, id) in &self.supporting_component_instances {
            let ic_prefix = self.netlist.instances.get(*id)
                .and_then(|inst| inst.attributes.get("vpin_parent").cloned())
                .or_else(|| name.split('_').next().map(|s| s.to_string()));
            if let Some(prefix) = ic_prefix {
                components_by_ic.entry(prefix)
                    .or_insert_with(Vec::new)
                    .push((name.clone(), *id));
            }
        }

        // Create connections for each IC's supporting components
        for (ic_name, components) in components_by_ic {
            info!("Creating connections for IC {} supporting components ({} components)", ic_name, components.len());

            // Find relevant nets - we need SW, VOUT, GND, FB nets
            let sw_net = self.find_or_create_net(&format!("{}_SW", ic_name), NetClass::Signal);
            let vout_net = self.find_or_create_net("VOUT", NetClass::Power { voltage: 5.0, current: None }); // TODO: Get actual voltage
            let gnd_net = self.find_or_create_net("GND", NetClass::Ground);
            let fb_net = self.find_or_create_net(&format!("{}_FB", ic_name), NetClass::Signal);

            for (comp_name, comp_id) in components {
                // Get the module for this component to create pins
                let module_id = if let Some(inst) = self.netlist.instances.get(comp_id) {
                    inst.definition
                } else {
                    continue;
                };

                // Determine component role from attributes, then fall back to name parsing.
                let (comp_class, comp_role) = {
                    let inst = self.netlist.instances.get(comp_id);
                    let class = inst.and_then(|i| i.attributes.get("component_class").cloned());
                    let role = inst.and_then(|i| i.attributes.get("vpin_role").cloned());
                    (class.unwrap_or_default(), role.unwrap_or_default())
                };

                // Classify by component_class + vpin_role, fall back to name heuristic
                let is_inductor = comp_class == "inductor" || comp_name.starts_with("L");
                let is_capacitor = comp_class == "capacitor" || comp_name.starts_with("C");
                let is_resistor = comp_class == "resistor" || comp_name.starts_with("R");
                let is_diode = comp_class == "diode" || comp_name.starts_with("D");

                if is_inductor && comp_role != "shunt" {
                    // Inductor (series): SW -> IN, OUT -> VOUT
                    info!("Connecting inductor {}", comp_name);
                    let pin1 = self.get_or_create_pin(module_id, "IN", PinDirection::In);
                    let pin2 = self.get_or_create_pin(module_id, "OUT", PinDirection::Out);

                    let pin_inst1 = self.netlist.pin_instances.insert_with_key(|id| PinInstance {
                        id,
                        pin_def: pin1,
                        instance: comp_id,
                        net: Some(sw_net),
                        connection_name: None,
                    });
                    let pin_inst2 = self.netlist.pin_instances.insert_with_key(|id| PinInstance {
                        id,
                        pin_def: pin2,
                        instance: comp_id,
                        net: Some(vout_net),
                        connection_name: None,
                    });

                    self.netlist.connect(sw_net, ConnectionPoint::PinInstance(pin_inst1)).ok();
                    self.netlist.connect(vout_net, ConnectionPoint::PinInstance(pin_inst2)).ok();
                } else if is_capacitor {
                    // Capacitor: VOUT -> pin 1, pin 2 -> GND
                    info!("Connecting capacitor {}", comp_name);
                    let pin1 = self.get_or_create_pin(module_id, "1", PinDirection::In);
                    let pin2 = self.get_or_create_pin(module_id, "2", PinDirection::In);

                    let pin_inst1 = self.netlist.pin_instances.insert_with_key(|id| PinInstance {
                        id,
                        pin_def: pin1,
                        instance: comp_id,
                        net: Some(vout_net),
                        connection_name: None,
                    });
                    let pin_inst2 = self.netlist.pin_instances.insert_with_key(|id| PinInstance {
                        id,
                        pin_def: pin2,
                        instance: comp_id,
                        net: Some(gnd_net),
                        connection_name: None,
                    });

                    self.netlist.connect(vout_net, ConnectionPoint::PinInstance(pin_inst1)).ok();
                    self.netlist.connect(gnd_net, ConnectionPoint::PinInstance(pin_inst2)).ok();
                } else if is_resistor {
                    // Resistor: default VOUT -> pin 1, pin 2 -> GND
                    // (feedback divider wiring would need more context)
                    info!("Connecting resistor {}", comp_name);
                    let pin1 = self.get_or_create_pin(module_id, "1", PinDirection::In);
                    let pin2 = self.get_or_create_pin(module_id, "2", PinDirection::Out);

                    let pin_inst1 = self.netlist.pin_instances.insert_with_key(|id| PinInstance {
                        id,
                        pin_def: pin1,
                        instance: comp_id,
                        net: Some(vout_net),
                        connection_name: None,
                    });
                    let pin_inst2 = self.netlist.pin_instances.insert_with_key(|id| PinInstance {
                        id,
                        pin_def: pin2,
                        instance: comp_id,
                        net: Some(gnd_net),
                        connection_name: None,
                    });

                    self.netlist.connect(vout_net, ConnectionPoint::PinInstance(pin_inst1)).ok();
                    self.netlist.connect(gnd_net, ConnectionPoint::PinInstance(pin_inst2)).ok();
                } else if is_diode {
                    // Diode: GND -> D.A (anode), D.K (cathode) -> SW
                    info!("Connecting diode {}", comp_name);
                    let pin_a = self.get_or_create_pin(module_id, "A", PinDirection::In);
                    let pin_k = self.get_or_create_pin(module_id, "K", PinDirection::Out);

                    let pin_inst_a = self.netlist.pin_instances.insert_with_key(|id| PinInstance {
                        id,
                        pin_def: pin_a,
                        instance: comp_id,
                        net: Some(gnd_net),
                        connection_name: None,
                    });
                    let pin_inst_k = self.netlist.pin_instances.insert_with_key(|id| PinInstance {
                        id,
                        pin_def: pin_k,
                        instance: comp_id,
                        net: Some(sw_net),
                        connection_name: None,
                    });

                    self.netlist.connect(gnd_net, ConnectionPoint::PinInstance(pin_inst_a)).ok();
                    self.netlist.connect(sw_net, ConnectionPoint::PinInstance(pin_inst_k)).ok();
                } else {
                    info!("Unknown supporting component type for {} (class={}, role={})", comp_name, comp_class, comp_role);
                }
            }
        }
        
        Ok(())
    }
    
}
