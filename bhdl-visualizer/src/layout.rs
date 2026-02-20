//! Layout engine for positioning circuit components

use std::collections::HashMap;
use anyhow::{Result, Context};
use log::{debug, info};

use bhdl_netlist::{Netlist, InstanceId, NetId};
use bhdl_synthesizer::{DatabaseComponentInstance, component_mapping::ComponentCategory};
use bhdl_analyzer::types::AnalysisResult;
use crate::types::{Point, BoundingBox, CircuitLayout, Component, Net, RoutingSegment};
use crate::symbols::SymbolManager;

/// Layout configuration
#[derive(Debug, Clone)]
pub struct LayoutConfig {
    /// Grid spacing for component placement
    pub grid_spacing: f64,
    /// Minimum spacing between components
    pub component_spacing: f64,
    /// Placement algorithm to use
    pub placement_algorithm: PlacementAlgorithm,
    /// Routing algorithm to use
    pub routing_algorithm: RoutingAlgorithm,
    /// Include grid background
    pub show_grid: bool,
    /// Canvas margins
    pub margins: f64,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            grid_spacing: 500.0,
            component_spacing: 4000.0,  // 4000 units spacing for ~1500-unit IC symbols (1000x scale) = 2500 unit gap
            placement_algorithm: PlacementAlgorithm::Semantic,
            routing_algorithm: RoutingAlgorithm::Manhattan,  // Use improved Manhattan routing with bus-like multi-point nets
            show_grid: true,
            margins: 500.0,
        }
    }
}

/// Component placement algorithms
#[derive(Debug, Clone, PartialEq)]
pub enum PlacementAlgorithm {
    /// Smart placement based on circuit semantics
    Semantic,
    /// Simple grid-based placement
    Grid,
    /// Force-directed placement
    ForceDirected,
}

/// Routing algorithms
#[derive(Debug, Clone, PartialEq)]
pub enum RoutingAlgorithm {
    /// Manhattan routing (orthogonal)
    Manhattan,
    /// Direct line routing
    Direct,
    /// Smart routing with obstacle avoidance
    Smart,
}

/// Layout engine for positioning components and routing connections
pub struct LayoutEngine {
    config: LayoutConfig,
    symbol_manager: SymbolManager,
}

impl LayoutEngine {
    /// Create a new layout engine with configuration
    pub fn new(config: LayoutConfig) -> Self {
        Self {
            config,
            symbol_manager: SymbolManager::new(),
        }
    }
    
    /// Layout a complete circuit from netlist and database components using semantic analysis
    pub async fn layout_circuit(
        &mut self,
        netlist: &Netlist,
        components: &[DatabaseComponentInstance],
        analysis_result: Option<&AnalysisResult>,
    ) -> Result<CircuitLayout> {
        info!("Starting circuit layout with {} components", components.len());
        
        let mut layout = CircuitLayout::new();
        layout.grid_spacing = self.config.grid_spacing;
        
        // Phase 1: Component placement using semantic analysis
        let positioned_components = self.place_components(netlist, components, analysis_result).await
            .context("Failed to place components")?;
        
        // Add components to layout
        for component in positioned_components {
            layout.add_component(component);
        }
        
        // Phase 2: Net routing
        let routed_nets = self.route_nets(netlist, &layout, components).await
            .context("Failed to route nets")?;
        
        // Add nets to layout
        for net in routed_nets {
            layout.add_net(net);
        }
        
        // Phase 3: Finalize layout
        layout.update_bounding_box();
        
        info!("Circuit layout complete: {} components, {} nets, bbox: {:.1}x{:.1}", 
              layout.components.len(), 
              layout.nets.len(),
              layout.bounding_box.width(),
              layout.bounding_box.height());
        
        Ok(layout)
    }
    
    /// Place components using semantic analysis and configured algorithm
    async fn place_components(
        &mut self,
        netlist: &Netlist,
        components: &[DatabaseComponentInstance],
        analysis_result: Option<&AnalysisResult>,
    ) -> Result<Vec<Component>> {
        match self.config.placement_algorithm {
            PlacementAlgorithm::Semantic => self.semantic_placement(netlist, components, analysis_result).await,
            PlacementAlgorithm::Grid => self.grid_placement(netlist, components).await,
            PlacementAlgorithm::ForceDirected => self.force_directed_placement(netlist, components).await,
        }
    }
    
    /// Semantic placement using BHDL analyzer semantic metadata
    async fn semantic_placement(
        &mut self,
        netlist: &Netlist,
        components: &[DatabaseComponentInstance],
        analysis_result: Option<&AnalysisResult>,
    ) -> Result<Vec<Component>> {
        debug!("Using semantic placement algorithm with BHDL analysis metadata");
        
        // Create a map from instance names to database components
        let mut component_map = HashMap::new();
        for comp in components {
            component_map.insert(comp.instance_name.clone(), comp);
        }
        
        // Use semantic metadata from BHDL analyzer if available
        let placement_strategy = if let Some(analysis) = analysis_result {
            self.determine_placement_strategy_from_analysis(analysis, components)
        } else {
            // Fallback to simple analysis if no semantic metadata
            debug!("No BHDL analysis metadata available, falling back to component analysis");
            self.analyze_circuit_pattern(components)
        };
        
        debug!("Selected placement strategy: {:?}", placement_strategy);

        let components = match placement_strategy {
            CircuitPattern::IntentDriven => {
                // Use intent-driven layout with flow tracker
                self.place_intent_driven_circuit(netlist, &component_map, analysis_result.unwrap()).await?
            }
            CircuitPattern::LinearRegulator => {
                self.place_power_circuit(netlist, &component_map).await?
            }
            CircuitPattern::OpAmpCircuit => {
                self.place_opamp_circuit(netlist, &component_map).await?
            }
            CircuitPattern::Generic => {
                self.place_generic_circuit(netlist, &component_map).await?
            }
        };

        Ok(components)
    }
    
    /// Determine placement strategy from BHDL analyzer semantic metadata
    fn determine_placement_strategy_from_analysis(
        &self,
        analysis: &AnalysisResult,
        components: &[DatabaseComponentInstance],
    ) -> CircuitPattern {
        debug!("Analyzing circuit pattern from BHDL semantic metadata");

        // PRIORITY 1: Check for flow tracker with intents (NEW!)
        if let Some(ref flow_tracker) = analysis.flow_tracker {
            let flow_paths = flow_tracker.get_flow_paths();

            // Count how many flows have intents
            let flows_with_intents: Vec<_> = flow_paths.iter()
                .filter(|flow| flow.intent.is_some())
                .collect();

            if !flows_with_intents.is_empty() {
                debug!("Found {} flow paths with intents, using intent-driven layout",
                       flows_with_intents.len());

                // Log intent categories for debugging
                for flow in &flows_with_intents {
                    if let Some(ref intent) = flow.intent {
                        debug!("  Flow {} has intent: {} with {} components",
                               flow.id, intent.name, flow.components.len());
                    }
                }

                return CircuitPattern::IntentDriven;
            }
        }

        // PRIORITY 2: Check power analysis for power-related circuits
        if !analysis.power_analysis.domains.is_empty() {
            debug!("Found power domains in analysis, treating as power circuit");
            return CircuitPattern::LinearRegulator;
        }

        // PRIORITY 3: Check component inference for circuit type hints
        for suggestion in &analysis.component_inference.inferred_components {
            if suggestion.component_type.contains("regulator") ||
               suggestion.component_type.contains("power") ||
               suggestion.component_type.contains("supply") {
                debug!("Found power-related component inference: {}", suggestion.component_type);
                return CircuitPattern::LinearRegulator;
            }

            if suggestion.component_type.contains("amplifier") ||
               suggestion.component_type.contains("opamp") {
                debug!("Found amplifier component inference: {}", suggestion.component_type);
                return CircuitPattern::OpAmpCircuit;
            }
        }

        // Fallback to component-based analysis
        debug!("No specific circuit type found in analysis, falling back to component analysis");
        self.analyze_circuit_pattern(components)
    }
    
    /// Analyze the type of circuit to optimize placement using netlist semantic data
    fn analyze_circuit_pattern(&self, components: &[DatabaseComponentInstance]) -> CircuitPattern {
        let mut has_regulator = false;
        let mut has_voltage_source = false;
        let mut has_opamp = false;
        let mut has_passives = false;
        let mut has_power_management = false;
        
        for component in components {
            match component.bhdl_type.as_str() {
                // Explicit voltage regulators
                "LM7805" | "LM317" | "LM1117" | "AMS1117" => has_regulator = true,
                // Voltage sources (power input)
                "VoltageSource" | "PowerSupply" | "Battery" => has_voltage_source = true,
                // Op-amps and amplifiers
                t if t.contains("opamp") || t.contains("amplifier") || t.contains("LM358") || t.contains("TL074") => has_opamp = true,
                // Passive components
                "Resistor" | "Capacitor" | "Inductor" => has_passives = true,
                // Ground and power management
                "Ground" | "PowerFlag" => has_power_management = true,
                _ => {}
            }
            
            // Check component category for additional hints
            match component.category {
                ComponentCategory::PowerRegulator => has_regulator = true,
                ComponentCategory::PassiveResistor | ComponentCategory::PassiveCapacitor => has_passives = true,
                _ => {}
            }
        }
        
        // Enhanced pattern detection based on semantic circuit analysis
        if has_regulator && has_passives {
            CircuitPattern::LinearRegulator
        } else if (has_voltage_source || has_power_management) && has_passives {
            // Voltage divider, power supply, or other power-related circuit
            // Treat as linear regulator layout for proper power flow visualization
            CircuitPattern::LinearRegulator
        } else if has_opamp && has_passives {
            CircuitPattern::OpAmpCircuit
        } else {
            CircuitPattern::Generic
        }
    }
    
    /// Place power circuits (regulators, voltage dividers, power supplies) with optimal layout
    async fn place_power_circuit(
        &mut self,
        netlist: &Netlist,
        component_map: &HashMap<String, &DatabaseComponentInstance>,
    ) -> Result<Vec<Component>> {
        debug!("Placing power circuit with semantic layout");

        let mut components = Vec::new();
        let spacing = self.config.component_spacing;

        // FIXED: Create type-based lookup map
        let mut type_map: HashMap<String, &DatabaseComponentInstance> = HashMap::new();
        for db_comp in component_map.values() {
            type_map.insert(db_comp.bhdl_type.clone(), db_comp);
        }

        // Classify instances by their module type for proper power flow layout
        let mut power_sources = Vec::new();
        let mut power_components = Vec::new(); // Regulators, converters
        let mut passive_components = Vec::new(); // R, L, C
        let mut load_components = Vec::new(); // LEDs, other loads
        let mut ground_components = Vec::new();

        // FIXED: Iterate over netlist instances and classify by module type
        for (instance_id, instance) in &netlist.instances {
            let module_def = match netlist.get_module(instance.definition) {
                Some(def) => def,
                None => {
                    debug!("WARNING: Module definition not found for instance {}, skipping", instance.name);
                    continue;
                }
            };

            let db_component = match type_map.get(&module_def.name) {
                Some(comp) => comp,
                None => {
                    debug!("WARNING: Database component not found for type {}, skipping instance {}",
                           module_def.name, instance.name);
                    continue;
                }
            };

            match db_component.bhdl_type.as_str() {
                "VoltageSource" | "PowerSupply" | "Battery" => power_sources.push((instance_id, instance, db_component)),
                "LM7805" | "LM317" | "LM1117" | "AMS1117" => power_components.push((instance_id, instance, db_component)),
                "Resistor" | "Capacitor" | "Inductor" | "Res" | "Cap" => passive_components.push((instance_id, instance, db_component)),
                "LED" => load_components.push((instance_id, instance, db_component)),
                "Ground" | "PowerFlag" => ground_components.push((instance_id, instance, db_component)),
                _ => passive_components.push((instance_id, instance, db_component)), // Default to passive
            }
        }

        debug!("Power circuit components: {} sources, {} power, {} passives, {} loads, {} grounds",
               power_sources.len(), power_components.len(), passive_components.len(),
               load_components.len(), ground_components.len());

        // Power flow layout: Sources (left) -> Power components (center) -> Loads (right)
        // Passives and grounds placed strategically
        let mut x_position = 0.0;
        let y_center = 0.0;

        // Phase 1: Place power sources on the left
        for (i, (instance_id, _instance, db_component)) in power_sources.iter().enumerate() {
            let y_offset = (i as f64 - power_sources.len() as f64 / 2.0) * spacing * 0.4;
            let component = self.symbol_manager.create_component(
                *instance_id,
                db_component,
                Point::new(x_position - spacing, y_center + y_offset),  // Compact: 1x spacing left
                0.0,
            ).await?;
            components.push(component);
        }

        // Phase 2: Place power management components in center
        for (i, (instance_id, _instance, db_component)) in power_components.iter().enumerate() {
            let y_offset = (i as f64 - power_components.len() as f64 / 2.0) * spacing * 0.4;
            let component = self.symbol_manager.create_component(
                *instance_id,
                db_component,
                Point::new(x_position, y_center + y_offset),
                0.0,
            ).await?;
            components.push(component);
        }

        // Phase 3: Place loads on the right
        for (i, (instance_id, _instance, db_component)) in load_components.iter().enumerate() {
            let y_offset = (i as f64 - load_components.len() as f64 / 2.0) * spacing * 0.4;
            let component = self.symbol_manager.create_component(
                *instance_id,
                db_component,
                Point::new(x_position + spacing, y_center + y_offset),  // Compact: 1x spacing right
                0.0,
            ).await?;
            components.push(component);
        }

        // Phase 4: Place passive components between power stages (more compact)
        let mut passive_x = x_position - spacing * 0.5;
        let mut passive_y = y_center + spacing * 0.6;
        for (instance_id, _instance, db_component) in &passive_components {
            let component = self.symbol_manager.create_component(
                *instance_id,
                db_component,
                Point::new(passive_x, passive_y),
                0.0,
            ).await?;
            components.push(component);

            // Alternate positions for multiple passives
            passive_x += spacing * 0.6;  // Compact horizontal spacing
            if passive_x > x_position + spacing * 0.5 {
                passive_x = x_position - spacing * 0.5;
                passive_y += spacing * 0.6;  // Compact vertical spacing
            }
        }

        // Phase 5: Place ground components at the bottom (more compact)
        for (i, (instance_id, _instance, db_component)) in ground_components.iter().enumerate() {
            let x_offset = (i as f64 - ground_components.len() as f64 / 2.0) * spacing * 0.4;
            let component = self.symbol_manager.create_component(
                *instance_id,
                db_component,
                Point::new(x_position + x_offset, y_center + spacing * 0.8),  // Closer bottom spacing
                0.0,
            ).await?;
            components.push(component);
        }

        debug!("Placed {} components in power circuit layout", components.len());
        Ok(components)
    }
    
    /// Place op-amp circuit
    async fn place_opamp_circuit(
        &mut self,
        netlist: &Netlist,
        component_map: &HashMap<String, &DatabaseComponentInstance>,
    ) -> Result<Vec<Component>> {
        debug!("Placing op-amp circuit");
        // For now, fall back to grid placement
        self.grid_placement_from_map(netlist, component_map).await
    }
    
    /// Place generic circuit with simple layout
    async fn place_generic_circuit(
        &mut self,
        netlist: &Netlist,
        component_map: &HashMap<String, &DatabaseComponentInstance>,
    ) -> Result<Vec<Component>> {
        debug!("Placing generic circuit");
        self.grid_placement_from_map(netlist, component_map).await
    }

    /// Place circuit using intent-driven spatial zones (NEW!)
    async fn place_intent_driven_circuit(
        &mut self,
        netlist: &Netlist,
        component_map: &HashMap<String, &DatabaseComponentInstance>,
        analysis: &AnalysisResult,
    ) -> Result<Vec<Component>> {
        debug!("Placing circuit with intent-driven layout");

        let flow_tracker = analysis.flow_tracker.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Flow tracker not available for intent-driven layout"))?;

        // Build component-to-zone mapping based on intents
        let mut component_zones: HashMap<String, SpatialZone> = HashMap::new();

        for flow_path in flow_tracker.get_flow_paths() {
            if let Some(ref intent) = flow_path.intent {
                let zone = map_intent_to_zone(&intent.name);

                debug!("Intent '{}' mapped to zone {:?}", intent.name, zone);

                // Assign all components in this flow to this zone
                for component_name in &flow_path.components {
                    // If component already has a zone, keep the first one
                    // (priority: first intent wins)
                    component_zones.entry(component_name.clone()).or_insert(zone);

                    debug!("  Component '{}' assigned to zone {:?}", component_name, zone);
                }
            }
        }

        // Define zone base positions (schematic-style layout)
        let spacing = self.config.component_spacing;
        let zone_positions = HashMap::from([
            (SpatialZone::Left, Point::new(-spacing * 3.0, 0.0)),
            (SpatialZone::Top, Point::new(0.0, -spacing * 2.0)),
            (SpatialZone::Center, Point::new(0.0, 0.0)),
            (SpatialZone::Right, Point::new(spacing * 3.0, 0.0)),
            (SpatialZone::Bottom, Point::new(0.0, spacing * 2.0)),
        ]);

        // FIXED: Create type-based lookup map instead of instance-based
        // This allows multiple instances of the same type to share database info
        let mut type_map: HashMap<String, &DatabaseComponentInstance> = HashMap::new();
        for db_comp in component_map.values() {
            type_map.insert(db_comp.bhdl_type.clone(), db_comp);
        }

        // Count instances per zone for layout within zones
        // IMPORTANT: Only count components that have database entries (component_map)
        let mut zone_counts: HashMap<SpatialZone, usize> = HashMap::new();
        for instance_name in component_map.keys() {
            let zone = component_zones.get(instance_name).copied().unwrap_or(SpatialZone::Center);
            *zone_counts.entry(zone).or_insert(0) += 1;
        }

        // Track placement index within each zone
        let mut zone_indices: HashMap<SpatialZone, usize> = HashMap::new();

        let mut components = Vec::new();

        // FIXED: Iterate over database components (component_map), NOT all netlist instances
        // This ensures we only place components that have SVG data for visualization
        for (instance_name, db_component) in component_map.iter() {
            // Find the instance_id from netlist for this component
            let instance_id = match netlist.instances.iter()
                .find(|(_, inst)| &inst.name == instance_name)
                .map(|(id, _)| id) {
                Some(id) => id,
                None => {
                    debug!("WARNING: Instance '{}' not found in netlist, skipping", instance_name);
                    continue;
                }
            };

            // Get the instance from netlist to access its properties
            let instance = match netlist.get_instance(instance_id) {
                Some(inst) => inst,
                None => {
                    debug!("WARNING: Instance {:?} not found in netlist", instance_id);
                    continue;
                }
            };

            // Get module definition to find component type
            let module_def = match netlist.get_module(instance.definition) {
                Some(def) => def,
                None => {
                    debug!("WARNING: Module definition not found for instance {}, skipping", instance.name);
                    continue;
                }
            };

            // Note: db_component is already provided by the iterator, no need to look it up

            // Determine zone for this component (default to Center if no intent)
            let zone = component_zones.get(&instance.name).copied().unwrap_or(SpatialZone::Center);

            // Get base position for this zone
            let base_pos = zone_positions[&zone];

            // Calculate position within zone to avoid overlapping
            let zone_index = zone_indices.entry(zone).or_insert(0);
            let zone_count = zone_counts.get(&zone).copied().unwrap_or(1);

            let position = match zone {
                SpatialZone::Left | SpatialZone::Right => {
                    // Stack vertically in left/right zones
                    let y_offset = (*zone_index as f64 - zone_count as f64 / 2.0) * spacing * 0.8;
                    Point::new(base_pos.x, base_pos.y + y_offset)
                }
                SpatialZone::Top | SpatialZone::Bottom => {
                    // Spread horizontally in top/bottom zones
                    let x_offset = (*zone_index as f64 - zone_count as f64 / 2.0) * spacing * 0.8;
                    Point::new(base_pos.x + x_offset, base_pos.y)
                }
                SpatialZone::Center => {
                    // Grid layout in center zone
                    let center_cols = (zone_count as f64).sqrt().ceil() as usize;
                    let row = *zone_index / center_cols;
                    let col = *zone_index % center_cols;
                    Point::new(
                        base_pos.x + (col as f64 - center_cols as f64 / 2.0) * spacing * 0.6,
                        base_pos.y + (row as f64 - (zone_count as f64 / center_cols as f64) / 2.0) * spacing * 0.6,
                    )
                }
            };

            *zone_index += 1;

            let component = self.symbol_manager.create_component(
                instance_id,
                db_component,
                position,
                0.0,
            ).await?;

            debug!("Placed component '{}' (type: {}) at ({:.1}, {:.1}) in zone {:?}",
                   instance.name, module_def.name, position.x, position.y, zone);

            components.push(component);
        }

        debug!("Intent-driven placement complete: {} components placed across {} zones",
               components.len(), zone_counts.len());

        Ok(components)
    }

    /// Grid-based placement algorithm
    async fn grid_placement(
        &mut self,
        netlist: &Netlist,
        components: &[DatabaseComponentInstance],
    ) -> Result<Vec<Component>> {
        debug!("Using grid placement algorithm");
        
        let component_map: HashMap<String, &DatabaseComponentInstance> = 
            components.iter().map(|c| (c.instance_name.clone(), c)).collect();
        
        self.grid_placement_from_map(netlist, &component_map).await
    }
    
    /// Grid placement implementation
    async fn grid_placement_from_map(
        &mut self,
        netlist: &Netlist,
        component_map: &HashMap<String, &DatabaseComponentInstance>,
    ) -> Result<Vec<Component>> {
        let mut components = Vec::new();
        let spacing = self.config.component_spacing;
        let instance_count = netlist.instances.len();
        let cols = (instance_count as f64).sqrt().ceil() as i32;

        // FIXED: Create type-based lookup map
        let mut type_map: HashMap<String, &DatabaseComponentInstance> = HashMap::new();
        for db_comp in component_map.values() {
            type_map.insert(db_comp.bhdl_type.clone(), db_comp);
        }

        // FIXED: Iterate over netlist instances, not database components
        for (i, (instance_id, instance)) in netlist.instances.iter().enumerate() {
            let row = i as i32 / cols;
            let col = i as i32 % cols;

            let x = col as f64 * spacing;
            let y = row as f64 * spacing;

            // Get module definition to find component type
            let module_def = match netlist.get_module(instance.definition) {
                Some(def) => def,
                None => {
                    debug!("WARNING: Module definition not found for instance {}, skipping", instance.name);
                    continue;
                }
            };

            // Look up database component by module type
            let db_component = match type_map.get(&module_def.name) {
                Some(comp) => comp,
                None => {
                    debug!("WARNING: Database component not found for type {}, skipping instance {}",
                           module_def.name, instance.name);
                    continue;
                }
            };

            let component = self.symbol_manager.create_component(
                instance_id,
                db_component,
                Point::new(x, y),
                0.0,
            ).await?;
            components.push(component);
        }

        Ok(components)
    }
    
    /// Force-directed placement algorithm (simplified)
    async fn force_directed_placement(
        &mut self,
        netlist: &Netlist,
        components: &[DatabaseComponentInstance],
    ) -> Result<Vec<Component>> {
        debug!("Using force-directed placement algorithm");
        // For now, fall back to grid placement
        // A full implementation would simulate forces between connected components
        self.grid_placement(netlist, components).await
    }
    
    /// Route nets using the configured algorithm
    async fn route_nets(
        &mut self,
        netlist: &Netlist,
        layout: &CircuitLayout,
        db_components: &[DatabaseComponentInstance],
    ) -> Result<Vec<Net>> {
        debug!("Routing {} nets", netlist.nets.len());
        
        let mut routed_nets = Vec::new();
        
        for (net_id, netlist_net) in &netlist.nets {
            debug!("Routing net: {:?} ({:?})", netlist_net.name, net_id);

            // Determine net type from netlist classification
            let net_type = match &netlist_net.net_class {
                bhdl_netlist::types::NetClass::Power(_) => crate::types::NetType::Power,
                bhdl_netlist::types::NetClass::Ground => crate::types::NetType::Ground,
                _ => crate::types::NetType::Signal,
            };

            let mut net = Net::with_type(net_id, netlist_net.name.clone(), net_type);

            // Collect connection points for this net
            debug!("  Net has {} connections in netlist", netlist_net.connections.len());
            let mut connection_points = Vec::new();
            for connection in &netlist_net.connections {
                debug!("  Connection type: {:?}", connection);

                // Extract instance_id and pin_id from either connection type
                let (instance_id, pin_id) = match connection {
                    bhdl_netlist::ConnectionPoint::InstancePin(inst_id, p_id) => {
                        debug!("  InstancePin: instance_id={:?}, pin_id={:?}", inst_id, p_id);
                        (*inst_id, *p_id)
                    }
                    bhdl_netlist::ConnectionPoint::PinInstance(pin_inst_id) => {
                        debug!("  PinInstance: pin_inst_id={:?}", pin_inst_id);
                        if let Some(pin_instance) = netlist.get_pin_instance(*pin_inst_id) {
                            debug!("    Resolved to instance_id={:?}, pin_def={:?}",
                                   pin_instance.instance, pin_instance.pin_def);
                            (pin_instance.instance, pin_instance.pin_def)
                        } else {
                            debug!("    WARNING: PinInstance {:?} not found in netlist", pin_inst_id);
                            continue;
                        }
                    }
                    _ => {
                        debug!("  Skipping non-pin connection: {:?}", connection);
                        continue;
                    }
                };

                if let Some(component) = layout.get_component_by_instance(instance_id) {
                    debug!("    Found component at ({}, {})", component.position.x, component.position.y);
                    debug!("    Component pins available: {:?}", component.pins.keys().collect::<Vec<_>>());

                    if let Some(pin) = netlist.get_pin(pin_id) {
                        debug!("    Pin from netlist: name='{}', id={:?}", pin.name, pin.id);

                        // Try to resolve the pin name using multiple strategies
                        let resolved_pin = self.resolve_pin_name(
                            component,
                            &pin.name,
                            netlist,
                            instance_id,
                            db_components
                        );

                        if let Some(pin_name) = resolved_pin {
                            if let Some(pin_pos) = component.get_pin_world_position(&pin_name) {
                                debug!("    ✓ Found pin position: ({}, {}) using resolved name '{}'",
                                       pin_pos.x, pin_pos.y, pin_name);
                                connection_points.push(pin_pos);
                            } else {
                                debug!("    WARNING: Resolved pin '{}' has no position", pin_name);
                            }
                        } else {
                            debug!("    WARNING: Could not resolve pin '{}'", pin.name);
                            debug!("             Available pins: {:?}", component.pins.keys().collect::<Vec<_>>());
                        }
                    } else {
                        debug!("    WARNING: Pin ID {:?} not found in netlist", pin_id);
                    }
                } else {
                    debug!("    WARNING: Component for instance {:?} not found in layout", instance_id);
                }
            }
            
            debug!("  Total connection points found: {}", connection_points.len());
            
            // Route between connection points
            if connection_points.len() >= 2 {
                let routing_segments = match self.config.routing_algorithm {
                    RoutingAlgorithm::Manhattan => self.route_manhattan(&connection_points),
                    RoutingAlgorithm::Direct => self.route_direct(&connection_points),
                    RoutingAlgorithm::Smart => self.route_smart(&connection_points, layout),
                };
                
                debug!("  Generated {} routing segments", routing_segments.len());
                
                for point in &connection_points {
                    net.add_connection_point(*point);
                }
                
                for segment in routing_segments {
                    net.add_routing_segment(segment);
                }
            } else {
                debug!("  Skipping net - insufficient connection points (need at least 2, found {})", connection_points.len());
            }
            
            routed_nets.push(net);
        }
        
        debug!("Routing complete: {} nets routed", routed_nets.len());
        Ok(routed_nets)
    }
    
    /// Manhattan (orthogonal) routing with improved multi-point handling
    fn route_manhattan(&self, points: &[Point]) -> Vec<RoutingSegment> {
        let mut segments = Vec::new();

        if points.is_empty() {
            return segments;
        }

        // For 2-point nets, use smart L-shaped routing
        if points.len() == 2 {
            let start = points[0];
            let end = points[1];

            // Determine if we should go horizontal-first or vertical-first
            let dx = (end.x - start.x).abs();
            let dy = (end.y - start.y).abs();

            // If points are already aligned, just draw a straight line
            if dx < 0.1 {
                // Vertical line
                segments.push(RoutingSegment::line(start, end));
            } else if dy < 0.1 {
                // Horizontal line
                segments.push(RoutingSegment::line(start, end));
            } else {
                // Need L-shaped routing
                // Add small offset from pins to avoid overlapping with component
                let pin_offset = 10.0;

                // Choose routing based on relative positions
                if dx > dy * 1.5 {
                    // Horizontal distance is significantly larger - go horizontal first
                    let start_offset = if end.x > start.x {
                        Point::new(start.x + pin_offset, start.y)
                    } else {
                        Point::new(start.x - pin_offset, start.y)
                    };

                    let intermediate = Point::new(end.x, start_offset.y);

                    // Start to offset
                    if (start_offset.x - start.x).abs() > 0.1 {
                        segments.push(RoutingSegment::line(start, start_offset));
                    }
                    // Horizontal segment
                    segments.push(RoutingSegment::line(start_offset, intermediate));
                    // Vertical segment
                    segments.push(RoutingSegment::line(intermediate, end));
                } else {
                    // Vertical distance is larger or similar - go vertical first
                    let start_offset = if end.y > start.y {
                        Point::new(start.x, start.y + pin_offset)
                    } else {
                        Point::new(start.x, start.y - pin_offset)
                    };

                    let intermediate = Point::new(start_offset.x, end.y);

                    // Start to offset
                    if (start_offset.y - start.y).abs() > 0.1 {
                        segments.push(RoutingSegment::line(start, start_offset));
                    }
                    // Vertical segment
                    segments.push(RoutingSegment::line(start_offset, intermediate));
                    // Horizontal segment
                    segments.push(RoutingSegment::line(intermediate, end));
                }
            }
        } else if points.len() > 2 {
            // For multi-point nets, use sequential routing instead of star topology
            // This creates a bus-like routing pattern that minimizes crossings

            // Sort points by x-coordinate to create left-to-right routing
            let mut sorted_points = points.to_vec();
            sorted_points.sort_by(|a, b| {
                a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal)
            });

            // Find a horizontal routing channel at the average Y position
            let avg_y = sorted_points.iter().map(|p| p.y).sum::<f64>() / sorted_points.len() as f64;

            // Route from left to right along the horizontal channel
            for (i, point) in sorted_points.iter().enumerate() {
                // Connect this point to the horizontal channel
                if (point.y - avg_y).abs() > 0.1 {
                    // Vertical segment from point to channel
                    let channel_point = Point::new(point.x, avg_y);
                    segments.push(RoutingSegment::line(*point, channel_point));
                }

                // Connect to the next point along the horizontal channel (if not last)
                if i < sorted_points.len() - 1 {
                    let next_point = sorted_points[i + 1];
                    if (point.x - next_point.x).abs() > 0.1 {
                        // Horizontal segment along the channel
                        let start_channel = Point::new(point.x, avg_y);
                        let end_channel = Point::new(next_point.x, avg_y);
                        segments.push(RoutingSegment::line(start_channel, end_channel));
                    }
                }
            }
        }

        segments
    }
    
    /// Direct line routing
    fn route_direct(&self, points: &[Point]) -> Vec<RoutingSegment> {
        let mut segments = Vec::new();
        
        for i in 1..points.len() {
            segments.push(RoutingSegment::line(points[i - 1], points[i]));
        }
        
        segments
    }
    
    /// Smart routing with obstacle avoidance (simplified)
    fn route_smart(&self, points: &[Point], layout: &CircuitLayout) -> Vec<RoutingSegment> {
        // For now, use Manhattan routing
        // A full implementation would avoid component bounding boxes
        self.route_manhattan(points)
    }
    
    /// Find instance ID in netlist by name
    fn find_instance_id(&self, netlist: &Netlist, instance_name: &str) -> Result<InstanceId> {
        for (instance_id, instance) in &netlist.instances {
            if instance.name == instance_name {
                return Ok(instance_id);
            }
        }
        Err(anyhow::anyhow!("Instance not found in netlist: {}", instance_name))
    }

    /// Resolve netlist pin name to component pin name using multiple strategies
    fn resolve_pin_name(
        &self,
        component: &Component,
        netlist_pin_name: &str,
        netlist: &Netlist,
        instance_id: InstanceId,
        db_components: &[DatabaseComponentInstance],
    ) -> Option<String> {
        // Strategy 1: Direct match - netlist pin name exists in component pins
        if component.pins.contains_key(netlist_pin_name) {
            debug!("      → Strategy 1 (direct match): '{}' found", netlist_pin_name);
            return Some(netlist_pin_name.to_string());
        }

        // Strategy 2: Use pin_mapping from database component
        // Get instance name from netlist to find corresponding database component
        if let Some(instance) = netlist.get_instance(instance_id) {
            if let Some(db_comp) = db_components.iter().find(|c| c.instance_name == instance.name) {
                // Check if pin_mapping has this netlist pin name
                if let Some(db_pin_number) = db_comp.pin_mapping.get(netlist_pin_name) {
                    // Now check if component has this database pin number
                    if component.pins.contains_key(db_pin_number) {
                        debug!("      → Strategy 2 (pin_mapping): '{}' → '{}'",
                               netlist_pin_name, db_pin_number);
                        return Some(db_pin_number.clone());
                    }
                }

                // Strategy 3: Try matching against database pin names
                // Some pins might use the pin name from the database directly
                for db_pin in &db_comp.pins {
                    if let Some(ref pin_name) = db_pin.pin_name {
                        // Check if netlist pin matches database pin name
                        if pin_name == netlist_pin_name {
                            // Return the pin number which is used as key in component.pins
                            if component.pins.contains_key(&db_pin.pin_number) {
                                debug!("      → Strategy 3 (DB pin name): '{}' → '{}'",
                                       netlist_pin_name, db_pin.pin_number);
                                return Some(db_pin.pin_number.clone());
                            }
                        }
                    }
                }
            }
        }

        // Strategy 4: Try case-insensitive match
        let netlist_pin_lower = netlist_pin_name.to_lowercase();
        for component_pin in component.pins.keys() {
            if component_pin.to_lowercase() == netlist_pin_lower {
                debug!("      → Strategy 4 (case-insensitive): '{}' → '{}'",
                       netlist_pin_name, component_pin);
                return Some(component_pin.clone());
            }
        }

        // Strategy 5: Try common aliases
        let aliases = match netlist_pin_name {
            "VCC" | "VDD" | "V+" | "VOUT" | "OUT" => vec!["1", "3", "VO", "OUT", "OUTPUT"],
            "GND" | "VSS" | "V-" => vec!["2", "GND", "GROUND"],
            "VIN" | "IN" | "INPUT" => vec!["1", "VI", "IN", "INPUT"],
            "A" | "ANODE" | "+" => vec!["1", "A", "ANODE", "+"],
            "K" | "CATHODE" | "-" => vec!["2", "K", "CATHODE", "-"],
            _ => vec![],
        };

        for alias in aliases {
            if component.pins.contains_key(alias) {
                debug!("      → Strategy 5 (alias): '{}' → '{}'", netlist_pin_name, alias);
                return Some(alias.to_string());
            }
        }

        // Strategy 6: If netlist pin is a number, try it directly
        if netlist_pin_name.chars().all(|c| c.is_numeric()) {
            if component.pins.contains_key(netlist_pin_name) {
                debug!("      → Strategy 6 (numeric): '{}'", netlist_pin_name);
                return Some(netlist_pin_name.to_string());
            }
        }

        // Strategy 7: Handle letter-prefixed numbers (A1->1, A2->2, K1->1, etc.)
        // If netlist pin is numeric, try component pins with letter prefix + number
        if netlist_pin_name.chars().all(|c| c.is_numeric()) {
            for component_pin in component.pins.keys() {
                // Check if component pin starts with a letter and ends with the netlist pin number
                if component_pin.len() > 1 {
                    let first_char = component_pin.chars().next().unwrap();
                    if first_char.is_alphabetic() {
                        let number_part = &component_pin[1..];
                        if number_part == netlist_pin_name {
                            debug!("      → Strategy 7 (letter-prefix): '{}' → '{}'",
                                   netlist_pin_name, component_pin);
                            return Some(component_pin.clone());
                        }
                    }
                }
            }
        }

        debug!("      → All strategies failed for pin '{}'", netlist_pin_name);
        None
    }
}

/// Circuit pattern types for semantic placement
#[derive(Debug, Clone, PartialEq)]
enum CircuitPattern {
    LinearRegulator,
    OpAmpCircuit,
    Generic,
    IntentDriven, // New pattern for intent-based layout
}

/// Spatial zones for intent-driven layout
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SpatialZone {
    Left,      // Input protection, buffering
    Top,       // Power management, regulation
    Center,    // Signal processing, filtering
    Right,     // Output buffering, distribution
    Bottom,    // Measurement, current sensing
}

/// Map intent function names to spatial zones for layout
fn map_intent_to_zone(intent_name: &str) -> SpatialZone {
    // Map standard BHDL intent functions to schematic zones
    // Based on common schematic conventions:
    // - Inputs on left
    // - Power on top
    // - Processing in center
    // - Outputs on right
    // - Measurement on bottom

    match intent_name {
        // Input-related intents → Left zone
        "input_protection" | "input_buffering" | "esd_protection" |
        "overvoltage_protection" | "input_filtering" | "signal_input_protection" |
        "bidirectional_protection" => SpatialZone::Left,

        // Power-related intents → Top zone
        "power_sequencing" | "voltage_regulation" | "power_output_protection" |
        "current_limiting" | "power_management" | "soft_start" |
        "inrush_limiting" | "power_conditioning" | "ground_protection" => SpatialZone::Top,

        // Signal processing intents → Center zone
        "noise_filtering" | "signal_processing" | "analog_filtering" |
        "digital_filtering" | "signal_conditioning" | "anti_alias" |
        "fast_response" | "slow_response" | "pulse_stretch" |
        "debounce" | "glitch_immunity" | "signal_amplification" |
        "level_shifting" => SpatialZone::Center,

        // Output-related intents → Right zone
        "output_buffering" | "signal_distribution" | "output_protection" |
        "signal_output_protection" | "drive_strength" | "fan_out" => SpatialZone::Right,

        // Measurement intents → Bottom zone
        "current_sensing" | "voltage_monitoring" | "precision_measurement" |
        "data_logging" | "measurement" | "fault_detection" => SpatialZone::Bottom,

        // Default to center for unknown intents
        _ => {
            debug!("Unknown intent '{}', defaulting to Center zone", intent_name);
            SpatialZone::Center
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_netlist::{Netlist, ModuleKind};
    
    #[tokio::test]
    async fn test_layout_engine_creation() {
        let config = LayoutConfig::default();
        let engine = LayoutEngine::new(config);
        
        assert_eq!(engine.config.placement_algorithm, PlacementAlgorithm::Semantic);
        assert_eq!(engine.config.routing_algorithm, RoutingAlgorithm::Manhattan);
    }
    
    #[tokio::test]
    async fn test_grid_placement() {
        let mut engine = LayoutEngine::new(LayoutConfig::default());
        let mut netlist = Netlist::new();
        
        // Create simple netlist
        let module_id = netlist.add_module("TestModule".to_string(), ModuleKind::PhysicalComponent);
        let instance_id = netlist.add_instance("R1".to_string(), module_id).unwrap();
        
        // Create test component
        let components = vec![create_test_component("R1", "Resistor")];
        
        let placed = engine.grid_placement(&netlist, &components).await.unwrap();
        
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].instance_id, instance_id);
    }
    
    #[test]
    fn test_circuit_pattern_analysis() {
        let engine = LayoutEngine::new(LayoutConfig::default());
        
        let regulator_components = vec![
            create_test_component("U1", "LM7805"),
            create_test_component("C1", "Capacitor"),
        ];
        
        let pattern = engine.analyze_circuit_pattern(&regulator_components);
        assert_eq!(pattern, CircuitPattern::LinearRegulator);
    }
    
    #[test]
    fn test_manhattan_routing() {
        let engine = LayoutEngine::new(LayoutConfig::default());
        let points = vec![
            Point::new(0.0, 0.0),
            Point::new(100.0, 50.0),
        ];
        
        let segments = engine.route_manhattan(&points);
        
        // Should create L-shaped routing
        assert!(segments.len() >= 1);
    }
    
    fn create_test_component(name: &str, bhdl_type: &str) -> DatabaseComponentInstance {
        use bhdl_synthesizer::component_mapping::ComponentCategory;
        use std::collections::HashMap;
        
        DatabaseComponentInstance {
            instance_name: name.to_string(),
            bhdl_type: bhdl_type.to_string(),
            component_id: 1,
            component_name: format!("{}_TEST", bhdl_type),
            component_description: Some("Test component".to_string()),
            svg_data: String::new(),
            pin_mapping: HashMap::new(),
            category: ComponentCategory::Unknown,
            electrical_specs: vec![],
            pins: vec![],
        }
    }
}