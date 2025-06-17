//! Main semantic visualizer that orchestrates pattern-based layout

use std::collections::HashMap;
use anyhow::{Result, Context};
use log::{debug, info, warn};

use bhdl_netlist::{Netlist, InstanceId, NetId};
use bhdl_synthesizer::DatabaseComponentInstance;

use crate::semantic_layout::{
    PatternDetector, ComponentClassifier, CircuitPattern, ComponentRole,
    get_layout_rules,
};
use crate::types::{
    Point, Component, Net, CircuitLayout, RoutingSegment, Orientation,
};
// use crate::symbols::SymbolLibrary;
use crate::svg::SvgRenderer;

/// Main semantic visualizer that creates intelligent circuit layouts
pub struct SemanticVisualizer {
    netlist: Netlist,
    component_instances: Vec<DatabaseComponentInstance>,
    // symbol_library: SymbolLibrary,
}

impl SemanticVisualizer {
    /// Create a new semantic visualizer
    pub fn new(
        netlist: Netlist,
        component_instances: Vec<DatabaseComponentInstance>,
    ) -> Self {
        Self {
            netlist,
            component_instances,
            // symbol_library: SymbolLibrary::new(),
        }
    }
    
    /// Generate a semantic-aware circuit layout
    pub fn generate_layout(&self) -> Result<CircuitLayout> {
        info!("🎨 Starting semantic circuit layout generation");
        
        // Phase 1: Detect circuit patterns
        let pattern_detector = PatternDetector::new(&self.netlist);
        let patterns = pattern_detector.detect_patterns();
        info!("Detected {} circuit patterns", patterns.len());
        
        // Phase 2: Classify components by role
        let classifier = ComponentClassifier::new(&self.netlist);
        let mut component_roles: HashMap<InstanceId, ComponentRole> = HashMap::new();
        
        for (instance_id, _) in self.netlist.instances.iter() {
            let role = classifier.classify(instance_id);
            component_roles.insert(instance_id, role);
            debug!("Component {:?} classified as {:?}", instance_id, component_roles[&instance_id]);
        }
        
        // Phase 3: Apply layout rules for each pattern
        let mut layout = CircuitLayout::new();
        let mut placed_components: HashMap<InstanceId, bool> = HashMap::new();
        
        for pattern in &patterns {
            info!("Applying layout rules for pattern: {:?}", pattern);
            self.layout_pattern(pattern, &component_roles, &mut layout, &mut placed_components)?;
        }
        
        // Phase 3b: Place any remaining unplaced components
        let unplaced: Vec<_> = self.netlist.instances.keys()
            .filter(|id| !placed_components.contains_key(id))
            .collect();
        
        if !unplaced.is_empty() {
            info!("Placing {} unplaced components", unplaced.len());
            self.layout_generic(&unplaced, &get_layout_rules(&CircuitPattern::Generic { components: vec![] }), 
                               &mut layout, &mut placed_components)?;
        }
        
        // Phase 4: Route connections
        self.route_connections(&mut layout)?;
        
        // Phase 5: Finalize layout
        layout.update_bounding_box();
        
        info!("✅ Layout generation complete: {} components, {} nets", 
              layout.components.len(), layout.nets.len());
        
        Ok(layout)
    }
    
    /// Apply layout rules for a specific pattern
    fn layout_pattern(
        &self,
        pattern: &CircuitPattern,
        component_roles: &HashMap<InstanceId, ComponentRole>,
        layout: &mut CircuitLayout,
        placed: &mut HashMap<InstanceId, bool>,
    ) -> Result<()> {
        let rules = get_layout_rules(pattern);
        
        match pattern {
            CircuitPattern::LinearRegulator { regulator, input_caps, output_caps, output_load, .. } => {
                info!("Laying out linear regulator pattern with {} input caps, {} output caps, {} output load", 
                      input_caps.len(), output_caps.len(), output_load.len());
                self.layout_linear_regulator(
                    *regulator,
                    input_caps,
                    output_caps,
                    output_load,
                    &rules,
                    layout,
                    placed,
                )?;
            }
            CircuitPattern::Generic { components } => {
                self.layout_generic(components, &rules, layout, placed)?;
            }
            _ => {
                warn!("Layout not implemented for pattern: {:?}", pattern);
            }
        }
        
        Ok(())
    }
    
    /// Layout a linear regulator pattern
    fn layout_linear_regulator(
        &self,
        regulator_id: InstanceId,
        input_caps: &[InstanceId],
        output_caps: &[InstanceId],
        output_load: &[InstanceId],
        rules: &crate::semantic_layout::LayoutRules,
        layout: &mut CircuitLayout,
        placed: &mut HashMap<InstanceId, bool>,
    ) -> Result<()> {
        info!("Laying out linear regulator pattern");
        
        // Place regulator at center
        let regulator_pos = Point::new(0.0, 0.0);
        self.place_component(regulator_id, regulator_pos, Orientation::Normal, layout, placed)?;
        
        // Place input capacitors to the left (scaled up 2x)
        let mut y_offset = -(input_caps.len() as f64 * 60.0) / 2.0;
        for cap_id in input_caps {
            let cap_pos = Point::new(-200.0, y_offset);
            self.place_component(*cap_id, cap_pos, Orientation::Rotate90, layout, placed)?;
            y_offset += 120.0;
        }
        
        // Place output capacitors to the right (scaled up 2x)
        let mut y_offset = -(output_caps.len() as f64 * 60.0) / 2.0;
        for cap_id in output_caps {
            let cap_pos = Point::new(200.0, y_offset);
            self.place_component(*cap_id, cap_pos, Orientation::Rotate90, layout, placed)?;
            y_offset += 120.0;
        }
        
        // Place output load components (LED + resistor) further to the right
        let mut y_offset = -(output_load.len() as f64 * 80.0) / 2.0;
        for (i, load_id) in output_load.iter().enumerate() {
            let x_pos = if i % 2 == 0 { 350.0 } else { 450.0 }; // Alternate between resistor and LED positions
            let load_pos = Point::new(x_pos, y_offset);
            
            // Check if it's a resistor or LED to orient properly
            if let Some(instance) = self.netlist.instances.get(*load_id) {
                if let Some(module) = self.netlist.modules.get(instance.definition) {
                    let orientation = if module.name.to_lowercase().contains("res") {
                        Orientation::Normal // Horizontal for resistors
                    } else {
                        Orientation::Rotate90 // Vertical for LEDs
                    };
                    self.place_component(*load_id, load_pos, orientation, layout, placed)?;
                }
            }
            
            if i % 2 == 1 { // After placing LED, move to next row
                y_offset += 160.0;
            }
        }
        
        Ok(())
    }
    
    /// Layout generic components in a grid
    fn layout_generic(
        &self,
        components: &[InstanceId],
        rules: &crate::semantic_layout::LayoutRules,
        layout: &mut CircuitLayout,
        placed: &mut HashMap<InstanceId, bool>,
    ) -> Result<()> {
        info!("Laying out {} components in generic pattern", components.len());
        
        let cols = 2; // Default grid columns
        let spacing = 150.0;
        let start_x = -300.0; // Start to the left to avoid overlap
        let start_y = 100.0;  // Start below the main circuit
        
        for (i, component_id) in components.iter().enumerate() {
            if placed.contains_key(component_id) {
                continue; // Already placed
            }
            
            let row = i / cols;
            let col = i % cols;
            
            let x = start_x + (col as f64 * spacing);
            let y = start_y + (row as f64 * spacing);
            
            self.place_component(*component_id, Point::new(x, y), Orientation::Normal, layout, placed)?;
        }
        
        Ok(())
    }
    
    /// Place a single component
    fn place_component(
        &self,
        instance_id: InstanceId,
        position: Point,
        orientation: Orientation,
        layout: &mut CircuitLayout,
        placed: &mut HashMap<InstanceId, bool>,
    ) -> Result<()> {
        let instance = self.netlist.instances.get(instance_id)
            .ok_or_else(|| anyhow::anyhow!("Instance {:?} not found", instance_id))?;
        
        let module = self.netlist.modules.get(instance.definition)
            .ok_or_else(|| anyhow::anyhow!("Module {:?} not found", instance.definition))?;
        
        // Create component with proper symbol
        let mut component = Component::new(instance_id, position)
            .with_label(instance.name.clone());
        component.rotation = orientation.rotation_degrees();
        
        // Get symbol from library or database
        if let Some(db_component) = self.find_database_component(instance_id) {
            debug!("Found database component for {}: svg_data length = {}", instance.name, db_component.svg_data.len());
            // Use database component symbol if available
            if !db_component.svg_data.is_empty() {
                component = component.with_svg(db_component.svg_data.clone());
                debug!("Using database symbol for {}", instance.name);
            } else {
                debug!("Database component {} has no SVG data", instance.name);
            }
        } else {
            debug!("No database component found for {}", instance.name);
        }
        
        // Set component size based on type
        component = self.set_component_size(component, &module.name);
        
        // Add pin positions
        self.add_pin_positions(&mut component, instance_id)?;
        
        layout.add_component(component);
        placed.insert(instance_id, true);
        
        Ok(())
    }
    
    /// Find database component instance for a netlist instance
    fn find_database_component(&self, instance_id: InstanceId) -> Option<&DatabaseComponentInstance> {
        let instance = self.netlist.instances.get(instance_id)?;
        self.component_instances.iter()
            .find(|db_comp| db_comp.instance_name == instance.name)
    }
    
    /// Set component size based on type (scaled up 2x)
    fn set_component_size(&self, component: Component, module_name: &str) -> Component {
        let name_lower = module_name.to_lowercase();
        
        if name_lower.contains("7805") || name_lower.contains("regulator") {
            component.with_size(160.0, 120.0)
        } else if name_lower.contains("cap") {
            component.with_size(60.0, 100.0)
        } else if name_lower.contains("res") {
            component.with_size(120.0, 40.0)
        } else if name_lower.contains("led") {
            component.with_size(60.0, 60.0)
        } else {
            component.with_size(120.0, 80.0)
        }
    }
    
    /// Add pin positions to component
    fn add_pin_positions(&self, component: &mut Component, instance_id: InstanceId) -> Result<()> {
        let instance = self.netlist.instances.get(instance_id)
            .ok_or_else(|| anyhow::anyhow!("Instance not found"))?;
        
        // Try to get pin positions from database component
        if let Some(db_component) = self.find_database_component(instance_id) {
            // Use actual pin positions from database (scaled up 10x to match SVG scale)
            for pin in &db_component.pins {
                let x = pin.x_position * 10.0;
                let y = pin.y_position * 10.0;
                let pin_name = pin.pin_name.as_ref()
                    .unwrap_or(&pin.pin_number)
                    .clone();
                component.pins.insert(pin_name, Point::new(x, y));
            }
            
            // Also add pin mappings for BHDL names
            for (bhdl_name, db_pin_number) in &db_component.pin_mapping {
                // Find the actual pin position for this number
                if let Some(pin) = db_component.pins.iter()
                    .find(|p| &p.pin_number == db_pin_number) {
                    let x = pin.x_position * 10.0;
                    let y = pin.y_position * 10.0;
                    component.pins.insert(bhdl_name.clone(), Point::new(x, y));
                }
            }
        } else {
            // Fallback to hardcoded positions based on component type
            let module = self.netlist.modules.get(instance.definition)
                .ok_or_else(|| anyhow::anyhow!("Module not found"))?;
            
            let name_lower = module.name.to_lowercase();
            
            if name_lower.contains("7805") {
                // LM7805 regulator pins
                component.pins.insert("IN".to_string(), Point::new(-40.0, 0.0));
                component.pins.insert("GND".to_string(), Point::new(0.0, 30.0));
                component.pins.insert("OUT".to_string(), Point::new(40.0, 0.0));
            } else if name_lower.contains("cap") {
                // Capacitor pins (vertical orientation, scaled up)
                component.pins.insert("pos".to_string(), Point::new(0.0, -50.0));
                component.pins.insert("neg".to_string(), Point::new(0.0, 50.0));
                component.pins.insert("1".to_string(), Point::new(0.0, -50.0));
                component.pins.insert("2".to_string(), Point::new(0.0, 50.0));
            } else if name_lower.contains("res") {
                // Resistor pins (horizontal)
                component.pins.insert("1".to_string(), Point::new(-30.0, 0.0));
                component.pins.insert("2".to_string(), Point::new(30.0, 0.0));
            } else if name_lower.contains("led") {
                // LED pins
                component.pins.insert("A".to_string(), Point::new(0.0, -15.0));
                component.pins.insert("K".to_string(), Point::new(0.0, 15.0));
            }
        }
        
        Ok(())
    }
    
    /// Route connections between components using Manhattan routing
    fn route_connections(&self, layout: &mut CircuitLayout) -> Result<()> {
        use crate::manhattan_router::{ManhattanRouter, RoutingTopology, Axis};
        
        info!("Routing connections for {} nets", self.netlist.nets.len());
        
        // Create router with obstacles from component bodies
        let mut router = ManhattanRouter::new(10.0); // 10 unit grid
        
        // Add component bodies as obstacles
        for component in &layout.components {
            let bbox = component.bounding_box();
            router.add_obstacle(
                Point::new(bbox.min_x, bbox.min_y),
                Point::new(bbox.max_x, bbox.max_y),
            );
        }
        
        // Route each net
        for (net_id, net_data) in self.netlist.nets.iter() {
            let mut net = Net::new(net_id, net_data.name.clone());
            
            // Collect all connection points for this net
            let mut connection_points = Vec::new();
            for connection in &net_data.connections {
                if let Some(point) = self.get_connection_point(connection, layout) {
                    connection_points.push(point);
                }
            }
            
            if connection_points.len() >= 2 {
                // Determine routing topology based on net type
                let topology = match &net_data.net_class {
                    bhdl_netlist::NetClass::Ground => {
                        // Star ground topology
                        let center = self.calculate_star_center(&connection_points);
                        RoutingTopology::Star { center }
                    }
                    bhdl_netlist::NetClass::Power(_) => {
                        // Bus topology for power distribution
                        RoutingTopology::Bus { main_axis: Axis::Horizontal }
                    }
                    _ => {
                        // Point-to-point for signals
                        RoutingTopology::PointToPoint
                    }
                };
                
                // Generate routing segments
                let segments = router.route_multi(&connection_points, topology);
                for segment in segments {
                    net.add_routing_segment(segment);
                }
                
                // Store connection points for visualization
                net.connection_points = connection_points;
            }
            
            layout.add_net(net);
        }
        
        Ok(())
    }
    
    /// Calculate optimal star center for ground connections
    fn calculate_star_center(&self, points: &[Point]) -> Point {
        if points.is_empty() {
            return Point::new(0.0, 0.0);
        }
        
        // Calculate centroid
        let sum_x: f64 = points.iter().map(|p| p.x).sum();
        let sum_y: f64 = points.iter().map(|p| p.y).sum();
        let count = points.len() as f64;
        
        // Offset down for ground star
        Point::new(sum_x / count, sum_y / count + 50.0)
    }
    
    /// Get physical connection point for a netlist connection
    fn get_connection_point(
        &self,
        connection: &bhdl_netlist::ConnectionPoint,
        layout: &CircuitLayout,
    ) -> Option<Point> {
        use bhdl_netlist::ConnectionPoint;
        
        match connection {
            ConnectionPoint::PinInstance(pin_inst_id) => {
                // Get pin instance details
                let pin_inst = self.netlist.pin_instances.get(*pin_inst_id)?;
                let pin = self.netlist.pins.get(pin_inst.pin_def)?;
                let component = layout.get_component_by_instance(pin_inst.instance)?;
                
                // Get world position of pin
                component.get_pin_world_position(&pin.name)
            }
            _ => None, // Handle other connection types as needed
        }
    }
    
}

/// Generate SVG visualization from circuit layout
pub fn generate_svg(layout: &CircuitLayout, output_path: &str) -> Result<()> {
    let renderer = SvgRenderer::new();
    let svg_content = renderer.render(layout)?;
    
    std::fs::write(output_path, svg_content)
        .context("Failed to write SVG file")?;
    
    info!("📄 SVG written to: {}", output_path);
    Ok(())
}