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
            grid_spacing: 20.0,
            component_spacing: 40.0,
            placement_algorithm: PlacementAlgorithm::Semantic,
            routing_algorithm: RoutingAlgorithm::Manhattan,
            show_grid: true,
            margins: 50.0,
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
        let routed_nets = self.route_nets(netlist, &layout).await
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
        
        // Check power analysis for power-related circuits
        if !analysis.power_analysis.domains.is_empty() {
            debug!("Found power domains in analysis, treating as power circuit");
            return CircuitPattern::LinearRegulator;
        }
        
        // Check component inference for circuit type hints
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
        
        // Classify components by type for proper power flow layout
        let mut power_sources = Vec::new();
        let mut power_components = Vec::new(); // Regulators, converters
        let mut passive_components = Vec::new(); // R, L, C
        let mut load_components = Vec::new(); // LEDs, other loads
        let mut ground_components = Vec::new();
        
        for (name, db_component) in component_map {
            match db_component.bhdl_type.as_str() {
                "VoltageSource" | "PowerSupply" | "Battery" => power_sources.push((name, db_component)),
                "LM7805" | "LM317" | "LM1117" | "AMS1117" => power_components.push((name, db_component)),
                "Resistor" | "Capacitor" | "Inductor" => passive_components.push((name, db_component)),
                "LED" => load_components.push((name, db_component)),
                "Ground" | "PowerFlag" => ground_components.push((name, db_component)),
                _ => passive_components.push((name, db_component)), // Default to passive
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
        for (i, (name, db_component)) in power_sources.iter().enumerate() {
            let instance_id = self.find_instance_id(netlist, name)?;
            let y_offset = (i as f64 - power_sources.len() as f64 / 2.0) * spacing * 0.5;
            let component = self.symbol_manager.create_component(
                instance_id,
                db_component,
                Point::new(x_position - spacing * 2.0, y_center + y_offset),
                0.0,
            ).await?;
            components.push(component);
        }
        
        // Phase 2: Place power management components in center
        for (i, (name, db_component)) in power_components.iter().enumerate() {
            let instance_id = self.find_instance_id(netlist, name)?;
            let y_offset = (i as f64 - power_components.len() as f64 / 2.0) * spacing * 0.5;
            let component = self.symbol_manager.create_component(
                instance_id,
                db_component,
                Point::new(x_position, y_center + y_offset),
                0.0,
            ).await?;
            components.push(component);
        }
        
        // Phase 3: Place loads on the right
        for (i, (name, db_component)) in load_components.iter().enumerate() {
            let instance_id = self.find_instance_id(netlist, name)?;
            let y_offset = (i as f64 - load_components.len() as f64 / 2.0) * spacing * 0.5;
            let component = self.symbol_manager.create_component(
                instance_id,
                db_component,
                Point::new(x_position + spacing * 2.0, y_center + y_offset),
                0.0,
            ).await?;
            components.push(component);
        }
        
        // Phase 4: Place passive components between power stages
        let mut passive_x = x_position - spacing;
        let mut passive_y = y_center + spacing;
        for (name, db_component) in &passive_components {
            let instance_id = self.find_instance_id(netlist, name)?;
            let component = self.symbol_manager.create_component(
                instance_id,
                db_component,
                Point::new(passive_x, passive_y),
                0.0,
            ).await?;
            components.push(component);
            
            // Alternate positions for multiple passives - ensure proper spacing
            passive_x += spacing;  // Use full spacing instead of half
            if passive_x > x_position + spacing {
                passive_x = x_position - spacing;
                passive_y += spacing * 0.8;  // Slightly less vertical spacing
            }
        }
        
        // Phase 5: Place ground components at the bottom
        for (i, (name, db_component)) in ground_components.iter().enumerate() {
            let instance_id = self.find_instance_id(netlist, name)?;
            let x_offset = (i as f64 - ground_components.len() as f64 / 2.0) * spacing * 0.5;
            let component = self.symbol_manager.create_component(
                instance_id,
                db_component,
                Point::new(x_position + x_offset, y_center + spacing * 1.5),
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
        let cols = (component_map.len() as f64).sqrt().ceil() as i32;
        
        for (i, (name, db_component)) in component_map.iter().enumerate() {
            let row = i as i32 / cols;
            let col = i as i32 % cols;
            
            let x = col as f64 * spacing;
            let y = row as f64 * spacing;
            
            let instance_id = self.find_instance_id(netlist, name)?;
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
    ) -> Result<Vec<Net>> {
        debug!("Routing {} nets", netlist.nets.len());
        
        let mut routed_nets = Vec::new();
        
        for (net_id, netlist_net) in &netlist.nets {
            let mut net = Net::new(net_id, netlist_net.name.clone());
            
            // Collect connection points for this net
            let mut connection_points = Vec::new();
            for connection in &netlist_net.connections {
                if let bhdl_netlist::ConnectionPoint::InstancePin(instance_id, pin_id) = connection {
                    if let Some(component) = layout.get_component_by_instance(*instance_id) {
                        if let Some(pin) = netlist.get_pin(*pin_id) {
                            if let Some(pin_pos) = component.get_pin_world_position(&pin.name) {
                                connection_points.push(pin_pos);
                            }
                        }
                    }
                }
            }
            
            // Route between connection points
            if connection_points.len() >= 2 {
                let routing_segments = match self.config.routing_algorithm {
                    RoutingAlgorithm::Manhattan => self.route_manhattan(&connection_points),
                    RoutingAlgorithm::Direct => self.route_direct(&connection_points),
                    RoutingAlgorithm::Smart => self.route_smart(&connection_points, layout),
                };
                
                for point in &connection_points {
                    net.add_connection_point(*point);
                }
                
                for segment in routing_segments {
                    net.add_routing_segment(segment);
                }
            }
            
            routed_nets.push(net);
        }
        
        Ok(routed_nets)
    }
    
    /// Manhattan (orthogonal) routing
    fn route_manhattan(&self, points: &[Point]) -> Vec<RoutingSegment> {
        let mut segments = Vec::new();
        
        for i in 1..points.len() {
            let start = points[i - 1];
            let end = points[i];
            
            // Create L-shaped routing (horizontal then vertical)
            let intermediate = Point::new(end.x, start.y);
            
            // Horizontal segment
            if (start.x - intermediate.x).abs() > 0.1 {
                segments.push(RoutingSegment::line(start, intermediate));
            }
            
            // Vertical segment
            if (intermediate.y - end.y).abs() > 0.1 {
                segments.push(RoutingSegment::line(intermediate, end));
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
}

/// Circuit pattern types for semantic placement
#[derive(Debug, Clone, PartialEq)]
enum CircuitPattern {
    LinearRegulator,
    OpAmpCircuit,
    Generic,
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