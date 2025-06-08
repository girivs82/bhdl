use std::collections::HashMap;
use std::fs::File;
use std::io::{Write, Result as IoResult};
use bhdl_netlist::{InstanceId, PinId, Netlist, PortDirection, NetId, ConnectionPoint};
use crate::layout::types::{Point, ComponentLayout, NetLayout, BoundingBox, LayoutResult, PlacementNode};
use crate::layout::semantic::{SemanticLayoutEngine, CircuitPattern, SemanticAnalyzer, SemanticLayoutConstraints};
use crate::layout::placement::PlacementEngine;
use crate::layout::routing::RoutingEngine;
use crate::layout::utils::is_point_inside_rotated_rect;
use crate::symbols::get_symbol_dimensions;
use crate::LayoutHints;
use crate::maze_router::{Grid, GridCellState};
use crate::global_router::CoarseGridGraph;
use rand::{self, Rng};

// Constants for Grid Routing
const GRID_RESOLUTION: f64 = 0.5;
const COARSE_GRID_TARGET_DIM: usize = 20;

pub struct LayoutEngine<'net> {
    netlist: &'net Netlist,
    hints: LayoutHints,
    component_layouts: HashMap<InstanceId, ComponentLayout>,
    nets_layout: HashMap<NetId, NetLayout>,
    bounding_box: BoundingBox,
    global_paths: HashMap<NetId, Vec<(usize, usize)>>,
    coarse_grid: Option<CoarseGridGraph>,
}

impl<'net> LayoutEngine<'net> {
    pub fn new(netlist: &'net Netlist) -> Self {
        LayoutEngine {
            netlist,
            hints: LayoutHints::default(),
            component_layouts: HashMap::new(),
            nets_layout: HashMap::new(),
            bounding_box: BoundingBox::new(),
            global_paths: HashMap::new(),
            coarse_grid: None,
        }
    }

    pub fn run(&mut self) {
        println!("Starting layout generation...");
        
        // Initialize component layouts and positions using semantic analysis
        let mut positions = HashMap::new();
        let mut rng = rand::thread_rng();
        for (instance_id, _) in self.netlist.instances.iter() {
            let x: f64 = rng.gen_range(-200.0..200.0);
            let y: f64 = rng.gen_range(-200.0..200.0);
            positions.insert(instance_id, Point::new(x, y));
        }

        // Apply semantic analysis and pre-placement
        let mut semantic_engine = SemanticLayoutEngine::new(self.netlist);
        semantic_engine.apply_semantic_placement(&mut positions);

        // Run layered placement
        let placement_engine = PlacementEngine::new(self.netlist);
        placement_engine.run_layered_placement(&mut positions);

        // Generate component layouts from positions
        self.generate_layout_result(positions);

        // Calculate bounding box
        self.calculate_final_bounding_box();

        // Create coarse grid for global routing
        let coarse_grid = self.create_coarse_grid();
        
        // Run global routing
        let global_paths = {
            let mut temp_nets_layout = HashMap::new();
            let routing_engine = RoutingEngine::new(
                self.netlist,
                &self.component_layouts,
                &mut temp_nets_layout,
                &self.bounding_box,
            );
            routing_engine.run_global_routing(&coarse_grid)
        };
        self.global_paths = global_paths;

        // Run detailed routing
        let grid_width_world = (self.bounding_box.max_x - self.bounding_box.min_x) + 10.0 * GRID_RESOLUTION;
        let grid_height_world = (self.bounding_box.max_y - self.bounding_box.min_y) + 10.0 * GRID_RESOLUTION;
        let mut initial_grid = Grid::new(
            Point::new(self.bounding_box.min_x, self.bounding_box.min_y),
            grid_width_world,
            grid_height_world,
            GRID_RESOLUTION
        );
        
        {
            let mut routing_engine = RoutingEngine::new(
                self.netlist,
                &self.component_layouts,
                &mut self.nets_layout,
                &self.bounding_box,
            );
            routing_engine.run_detailed_routing(&mut initial_grid, &self.global_paths, &coarse_grid);
        }

        self.coarse_grid = Some(coarse_grid);

        // Final bounding box calculation including nets
        self.calculate_final_bounding_box();

        println!("Layout generation completed!");
    }

    pub fn run_with_semantic_analysis(&mut self) -> LayoutResult {
        let mut positions = HashMap::new();
        
        // Initialize positions randomly
        let mut rng = rand::thread_rng();
        for (instance_id, _) in self.netlist.instances.iter() {
            let x: f64 = rng.gen_range(-200.0..200.0);
            let y: f64 = rng.gen_range(-200.0..200.0);
            positions.insert(instance_id, Point::new(x, y));
        }

        // Apply semantic analysis and pre-placement
        let mut semantic_engine = SemanticLayoutEngine::new(self.netlist);
        semantic_engine.apply_semantic_placement(&mut positions);

        // Apply final semantic constraints
        semantic_engine.apply_global_constraints(&mut positions);

        // Generate layouts with rotation information from semantic analysis
        let component_rotations = semantic_engine.get_component_rotations();
        let final_positions = positions.clone();
        self.generate_layout_result_with_rotations(positions, component_rotations);

        // Calculate bounding box AFTER generating component layouts
        self.calculate_final_bounding_box();

        // Create coarse grid for global routing
        let coarse_grid = self.create_coarse_grid();
        
        // Run global routing
        let global_paths = {
            let mut temp_nets_layout = HashMap::new();
            let routing_engine = RoutingEngine::new(
                self.netlist,
                &self.component_layouts,
                &mut temp_nets_layout,
                &self.bounding_box,
            );
            routing_engine.run_global_routing(&coarse_grid)
        };
        self.global_paths = global_paths;

        // Run detailed routing
        let grid_width_world = (self.bounding_box.max_x - self.bounding_box.min_x) + 10.0 * GRID_RESOLUTION;
        let grid_height_world = (self.bounding_box.max_y - self.bounding_box.min_y) + 10.0 * GRID_RESOLUTION;
        let mut initial_grid = Grid::new(
            Point::new(self.bounding_box.min_x, self.bounding_box.min_y),
            grid_width_world,
            grid_height_world,
            GRID_RESOLUTION
        );
        
        {
            let mut routing_engine = RoutingEngine::new(
                self.netlist,
                &self.component_layouts,
                &mut self.nets_layout,
                &self.bounding_box,
            );
            routing_engine.run_detailed_routing(&mut initial_grid, &self.global_paths, &coarse_grid);
        }

        self.coarse_grid = Some(coarse_grid);

        // Final bounding box calculation including nets
        self.calculate_final_bounding_box();

        println!("Semantic layout generation completed!");

        // Generate the result
        LayoutResult {
            component_positions: final_positions,
        }
    }

    fn create_coarse_grid(&self) -> CoarseGridGraph {
        let world_width = (self.bounding_box.max_x - self.bounding_box.min_x) + 10.0 * GRID_RESOLUTION;
        let world_height = (self.bounding_box.max_y - self.bounding_box.min_y) + 10.0 * GRID_RESOLUTION;
        
        CoarseGridGraph::new(
            Point::new(self.bounding_box.min_x, self.bounding_box.min_y),
            world_width,
            world_height
        )
    }

    fn is_gnd_component(&self, inst_id: InstanceId) -> bool {
        if let Some(instance) = self.netlist.instances.get(inst_id) {
            if let Some(def) = self.netlist.modules.get(instance.definition) {
                return def.name.to_lowercase().contains("ground") || def.name.to_lowercase().contains("gnd");
            }
        }
        false
    }

    fn is_power_component(&self, inst_id: InstanceId) -> bool {
        if let Some(instance) = self.netlist.instances.get(inst_id) {
            if let Some(def) = self.netlist.modules.get(instance.definition) {
                let name_lower = def.name.to_lowercase();
                return name_lower.contains("vcc") || name_lower.contains("vdd") || 
                       name_lower.contains("power") || name_lower.contains("supply");
            }
        }
        false
    }

    fn calculate_relative_pin_positions(&self, instance_id: InstanceId) -> HashMap<PinId, Point> {
        let mut pin_positions = HashMap::new();
        
        if let Some(instance) = self.netlist.instances.get(instance_id) {
            if let Some(module_def) = self.netlist.modules.get(instance.definition) {
                // Get the proper symbol dimensions and pin locations
                let (_, _, symbol_pin_locations) = get_symbol_dimensions(module_def, self.netlist);
                
                // Map from pin names back to pin IDs
                for pin_id in &module_def.pins {
                    if let Some(pin) = self.netlist.pins.get(*pin_id) {
                        if let Some(position) = symbol_pin_locations.get(&pin.name) {
                            pin_positions.insert(*pin_id, *position);
                        }
                    }
                }
            }
        }
        
        pin_positions
    }

    fn get_pin_direction(&self, _instance_id: InstanceId, _pin_id: PinId) -> Option<PortDirection> {
        // TODO: Implement proper pin direction lookup with new API
        // For now, return None since pins don't have direction - ports do
        None
    }

    fn generate_layout_result(&mut self, positions: HashMap<InstanceId, Point>) {
        for (instance_id, position) in positions {
            if let Some(instance) = self.netlist.instances.get(instance_id) {
                if let Some(module_def) = self.netlist.modules.get(instance.definition) {
                    let (width, height, _) = get_symbol_dimensions(module_def, self.netlist);
                    let relative_pin_locations = self.calculate_relative_pin_positions(instance_id);
                    
                    let layout = ComponentLayout {
                        center_x: position.x,
                        center_y: position.y,
                        rotation: 0.0,
                        relative_pin_locations,
                        width,
                        height,
                    };
                    
                    self.component_layouts.insert(instance_id, layout);
                }
            }
        }
    }

    fn generate_layout_result_with_rotations(&mut self, positions: HashMap<InstanceId, Point>, component_rotations: &HashMap<InstanceId, f64>) {
        for (instance_id, position) in positions {
            if let Some(instance) = self.netlist.instances.get(instance_id) {
                if let Some(module_def) = self.netlist.modules.get(instance.definition) {
                    let (width, height, _) = get_symbol_dimensions(module_def, self.netlist);
                    let relative_pin_locations = self.calculate_relative_pin_positions(instance_id);
                    
                    // Get rotation from semantic analyzer, or default to 0.0
                    let rotation = component_rotations.get(&instance_id).copied().unwrap_or(0.0);
                    
                    let layout = ComponentLayout {
                        center_x: position.x,
                        center_y: position.y,
                        rotation,
                        relative_pin_locations,
                        width,
                        height,
                    };
                    
                    self.component_layouts.insert(instance_id, layout);
                }
            }
        }
    }

    fn get_world_pin_pos(&self, inst_id: InstanceId, pin_id: PinId) -> Option<Point> {
        if let Some(layout) = self.component_layouts.get(&inst_id) {
            if let Some(rel_pos) = layout.relative_pin_locations.get(&pin_id) {
                let angle = layout.rotation.to_radians();
                let cos_a = angle.cos();
                let sin_a = angle.sin();
                let world_x = layout.center_x + rel_pos.x * cos_a - rel_pos.y * sin_a;
                let world_y = layout.center_y + rel_pos.x * sin_a + rel_pos.y * cos_a;
                return Some(Point::new(world_x, world_y));
            }
        }
        None
    }

    fn calculate_final_bounding_box(&mut self) {
        self.bounding_box = BoundingBox::new();
        
        for (_, layout) in &self.component_layouts {
            self.bounding_box.update_with_component(layout);
        }
        
        for (_, net_layout) in &self.nets_layout {
            self.bounding_box.update_with_net(net_layout);
        }
        
        self.bounding_box.add_padding(50.0);
    }

    pub fn generate_debug_output(&self, filename: &str) -> IoResult<()> {
        let mut file = File::create(filename)?;
        
        writeln!(file, "Layout Debug Information")?;
        writeln!(file, "========================")?;
        writeln!(file)?;
        
        writeln!(file, "Component Layouts:")?;
        for (instance_id, layout) in &self.component_layouts {
            writeln!(file, "  Instance {:?}: center=({:.2}, {:.2}), size=({:.2}, {:.2}), rotation={:.2}°", 
                instance_id, layout.center_x, layout.center_y, layout.width, layout.height, layout.rotation)?;
        }
        
        writeln!(file)?;
        writeln!(file, "Net Layouts:")?;
        for (net_id, layout) in &self.nets_layout {
            writeln!(file, "  Net {:?}: {} segments", net_id, layout.segments.len())?;
        }
        
        writeln!(file)?;
        writeln!(file, "Bounding Box: ({:.2}, {:.2}) to ({:.2}, {:.2})",
            self.bounding_box.min_x, self.bounding_box.min_y,
            self.bounding_box.max_x, self.bounding_box.max_y)?;
        
        Ok(())
    }

    pub fn get_layouts_and_debug(&self) -> (
        &HashMap<InstanceId, ComponentLayout>,
        &HashMap<NetId, NetLayout>,
        &BoundingBox,
        Option<&CoarseGridGraph>,
        &HashMap<NetId, Vec<(usize, usize)>>,
    ) {
        (&self.component_layouts, &self.nets_layout, &self.bounding_box, self.coarse_grid.as_ref(), &self.global_paths)
    }

    fn find_net_id_connecting(&self, inst1_id_target: InstanceId, inst2_id_target: InstanceId) -> Option<NetId> {
        for (net_id, net) in &self.netlist.nets {
            let mut found_inst1 = false;
            let mut found_inst2 = false;
            
            for connection in &net.connections {
                match connection {
                    ConnectionPoint::InstancePort(inst_id, _) | 
                    ConnectionPoint::InstancePin(inst_id, _) => {
                        if *inst_id == inst1_id_target {
                            found_inst1 = true;
                        }
                        if *inst_id == inst2_id_target {
                            found_inst2 = true;
                        }
                    }
                    ConnectionPoint::ModulePort(_) => {
                        // Skip module ports for this check
                    }
                }
            }
            
            if found_inst1 && found_inst2 {
                return Some(net_id);
            }
        }
        None
    }

    fn get_instance_id_by_name_iter(&self, name_to_find: &str) -> Option<InstanceId> {
        for (instance_id, instance) in &self.netlist.instances {
            if instance.name == name_to_find {
                return Some(instance_id);
            }
        }
        None
    }
} 