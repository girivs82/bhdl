/// Intelligent component placer that uses rules and signal flow analysis
/// This is the core intelligence that makes layouts professional

use std::collections::HashMap;
use crate::placement_rules::{PlacementRules, SignalFlow, GroupingRule, AlignmentRule};
use crate::signal_flow_analyzer::{SignalFlowAnalysis, ComponentRole, StageType};

#[derive(Debug, Clone)]
pub struct ComponentPlacement {
    pub name: String,
    pub position: (f64, f64),
    pub size: (f64, f64),
    pub rotation: f64,
}

pub struct IntelligentPlacer {
    rules: PlacementRules,
    flow_analysis: SignalFlowAnalysis,
    components: HashMap<String, ComponentInfo>,
    placements: HashMap<String, ComponentPlacement>,
    canvas_width: f64,
    canvas_height: f64,
}

#[derive(Debug, Clone)]
struct ComponentInfo {
    name: String,
    component_type: String,
    size: (f64, f64),
    role: ComponentRole,
    stage: Option<usize>,
}

impl IntelligentPlacer {
    pub fn new(rules: PlacementRules, flow_analysis: SignalFlowAnalysis) -> Self {
        Self {
            rules,
            flow_analysis,
            components: HashMap::new(),
            placements: HashMap::new(),
            canvas_width: 1000.0,
            canvas_height: 600.0,
        }
    }
    
    /// Add component to be placed
    pub fn add_component(&mut self, name: String, component_type: String) {
        let size = self.get_component_size(&component_type);
        let role = self.flow_analysis.component_roles
            .get(&name)
            .cloned()
            .unwrap_or(ComponentRole::Supporting);
        
        // Find which stage this component belongs to
        let stage = self.flow_analysis.signal_stages
            .iter()
            .find(|s| s.components.contains(&name))
            .map(|s| s.stage_num);
        
        self.components.insert(name.clone(), ComponentInfo {
            name,
            component_type,
            size,
            role,
            stage,
        });
    }
    
    /// Get component size based on type and role
    fn get_component_size(&self, component_type: &str) -> (f64, f64) {
        match component_type {
            "IC" => (80.0, 60.0),
            "Capacitor" => (20.0, 40.0),
            "Resistor" => (50.0, 15.0),
            "Inductor" => (50.0, 30.0),
            "Diode" => (30.0, 20.0),
            _ => (30.0, 30.0),
        }
    }
    
    /// Execute the placement algorithm
    pub fn place_components(&mut self) -> HashMap<String, ComponentPlacement> {
        // Stage 1: Place components along signal flow
        self.place_by_signal_flow();
        
        // Stage 2: Apply grouping rules
        self.apply_grouping_rules();
        
        // Stage 3: Apply alignment rules
        self.apply_alignment_rules();
        
        // Stage 4: Optimize placement
        self.optimize_placement();
        
        // Stage 5: Center and scale to canvas
        self.fit_to_canvas();
        
        self.placements.clone()
    }
    
    /// Place components based on signal flow stages
    fn place_by_signal_flow(&mut self) {
        let num_stages = self.flow_analysis.signal_stages.len();
        if num_stages == 0 {
            self.place_fallback();
            return;
        }
        
        // Calculate stage positions based on flow direction
        let stage_positions = match self.rules.signal_flow_direction {
            SignalFlow::LeftToRight => {
                // Distribute stages horizontally
                let stage_width = self.canvas_width / (num_stages as f64 + 1.0);
                (0..num_stages)
                    .map(|i| ((i as f64 + 1.0) * stage_width, self.canvas_height / 2.0))
                    .collect::<Vec<_>>()
            }
            SignalFlow::TopToBottom => {
                // Distribute stages vertically
                let stage_height = self.canvas_height / (num_stages as f64 + 1.0);
                (0..num_stages)
                    .map(|i| (self.canvas_width / 2.0, (i as f64 + 1.0) * stage_height))
                    .collect::<Vec<_>>()
            }
            SignalFlow::Radial => {
                // Place stages in a circle
                let center = (self.canvas_width / 2.0, self.canvas_height / 2.0);
                let radius = self.canvas_width.min(self.canvas_height) / 3.0;
                (0..num_stages)
                    .map(|i| {
                        let angle = (i as f64) * 2.0 * std::f64::consts::PI / (num_stages as f64);
                        (
                            center.0 + radius * angle.cos(),
                            center.1 + radius * angle.sin()
                        )
                    })
                    .collect::<Vec<_>>()
            }
        };
        
        // Place components within each stage
        for (stage_idx, stage) in self.flow_analysis.signal_stages.iter().enumerate() {
            let (stage_x, stage_y) = stage_positions[stage_idx];
            let components_in_stage = &stage.components;
            let num_components = components_in_stage.len();
            
            if num_components == 0 {
                continue;
            }
            
            // Arrange components within stage based on stage type
            match stage.stage_type {
                StageType::Input | StageType::Output => {
                    // Stack vertically for filter capacitors
                    let spacing = self.rules.component_spacing.min_component_spacing;
                    let total_height = (num_components as f64) * spacing;
                    let start_y = stage_y - total_height / 2.0;
                    
                    for (i, comp_name) in components_in_stage.iter().enumerate() {
                        let y = start_y + (i as f64) * spacing;
                        self.placements.insert(comp_name.clone(), ComponentPlacement {
                            name: comp_name.clone(),
                            position: (stage_x, y),
                            size: self.components[comp_name].size,
                            rotation: 0.0,
                        });
                    }
                }
                StageType::PowerConversion => {
                    // Place IC centrally, others around it
                    let ic_components: Vec<_> = components_in_stage.iter()
                        .filter(|c| self.components.get(*c)
                            .map_or(false, |info| info.component_type == "IC"))
                        .collect();
                    
                    let other_components: Vec<_> = components_in_stage.iter()
                        .filter(|c| !ic_components.contains(c))
                        .collect();
                    
                    // Place IC at stage position
                    if let Some(ic_name) = ic_components.first() {
                        self.placements.insert((*ic_name).clone(), ComponentPlacement {
                            name: (*ic_name).clone(),
                            position: (stage_x, stage_y),
                            size: self.components[*ic_name].size,
                            rotation: 0.0,
                        });
                    }
                    
                    // Place others around IC
                    for (i, comp_name) in other_components.iter().enumerate() {
                        let offset = self.rules.component_spacing.routing_channel_width;
                        let x = stage_x + offset * ((i % 2) as f64 * 2.0 - 1.0);
                        let y = stage_y + offset * ((i / 2) as f64);
                        
                        self.placements.insert((*comp_name).clone(), ComponentPlacement {
                            name: (*comp_name).clone(),
                            position: (x, y),
                            size: self.components[*comp_name].size,
                            rotation: 0.0,
                        });
                    }
                }
                StageType::Feedback => {
                    // Place feedback resistors in divider configuration
                    let spacing = self.rules.component_spacing.min_component_spacing;
                    for (i, comp_name) in components_in_stage.iter().enumerate() {
                        let y = stage_y + (i as f64) * spacing;
                        self.placements.insert(comp_name.clone(), ComponentPlacement {
                            name: comp_name.clone(),
                            position: (stage_x, y),
                            size: self.components[comp_name].size,
                            rotation: 0.0,
                        });
                    }
                }
                _ => {
                    // Generic placement
                    let spacing = self.rules.component_spacing.min_component_spacing;
                    for (i, comp_name) in components_in_stage.iter().enumerate() {
                        let x = stage_x + (i as f64) * spacing;
                        self.placements.insert(comp_name.clone(), ComponentPlacement {
                            name: comp_name.clone(),
                            position: (x, stage_y),
                            size: self.components[comp_name].size,
                            rotation: 0.0,
                        });
                    }
                }
            }
        }
    }
    
    /// Fallback placement when no signal flow is detected
    fn place_fallback(&mut self) {
        let mut x = 100.0;
        let mut y = 100.0;
        
        for (name, info) in &self.components {
            self.placements.insert(name.clone(), ComponentPlacement {
                name: name.clone(),
                position: (x, y),
                size: info.size,
                rotation: 0.0,
            });
            
            x += info.size.0 + self.rules.component_spacing.min_component_spacing;
            if x > self.canvas_width - 100.0 {
                x = 100.0;
                y += 100.0;
            }
        }
    }
    
    /// Apply grouping rules to cluster related components
    fn apply_grouping_rules(&mut self) {
        let rules = self.rules.grouping_rules.clone();
        for rule in rules {
            let components_to_group = self.find_components_for_grouping(&rule);
            if !components_to_group.is_empty() {
                self.group_components(components_to_group, &rule);
            }
        }
    }
    
    /// Find components that match grouping condition
    fn find_components_for_grouping(&self, rule: &GroupingRule) -> Vec<String> {
        use crate::placement_rules::GroupingCondition;
        
        match &rule.condition {
            GroupingCondition::SameType(comp_type) => {
                self.components.iter()
                    .filter(|(_, info)| info.component_type == *comp_type || comp_type == "Any")
                    .map(|(name, _)| name.clone())
                    .collect()
            }
            GroupingCondition::SameRole(role_name) => {
                self.components.iter()
                    .filter(|(_, info)| format!("{:?}", info.role) == *role_name)
                    .map(|(name, _)| name.clone())
                    .collect()
            }
            _ => Vec::new(),
        }
    }
    
    /// Group components according to placement rule
    fn group_components(&mut self, components: Vec<String>, rule: &GroupingRule) {
        use crate::placement_rules::GroupPlacement;
        
        if components.is_empty() {
            return;
        }
        
        // Calculate group center from current positions
        let mut center_x = 0.0;
        let mut center_y = 0.0;
        let mut count = 0;
        
        for comp_name in &components {
            if let Some(placement) = self.placements.get(comp_name) {
                center_x += placement.position.0;
                center_y += placement.position.1;
                count += 1;
            }
        }
        
        if count > 0 {
            center_x /= count as f64;
            center_y /= count as f64;
        }
        
        // Apply group placement
        match &rule.placement {
            GroupPlacement::VerticalStack { spacing } => {
                let total_height = components.len() as f64 * spacing;
                let start_y = center_y - total_height / 2.0;
                
                for (i, comp_name) in components.iter().enumerate() {
                    if let Some(placement) = self.placements.get_mut(comp_name) {
                        placement.position.1 = start_y + i as f64 * spacing;
                    }
                }
            }
            GroupPlacement::HorizontalArray { spacing } => {
                let total_width = components.len() as f64 * spacing;
                let start_x = center_x - total_width / 2.0;
                
                for (i, comp_name) in components.iter().enumerate() {
                    if let Some(placement) = self.placements.get_mut(comp_name) {
                        placement.position.0 = start_x + i as f64 * spacing;
                    }
                }
            }
            GroupPlacement::Cluster { radius } => {
                let angle_step = 2.0 * std::f64::consts::PI / components.len() as f64;
                
                for (i, comp_name) in components.iter().enumerate() {
                    let angle = i as f64 * angle_step;
                    if let Some(placement) = self.placements.get_mut(comp_name) {
                        placement.position.0 = center_x + radius * angle.cos();
                        placement.position.1 = center_y + radius * angle.sin();
                    }
                }
            }
            _ => {}
        }
    }
    
    /// Apply alignment rules to create clean lines
    fn apply_alignment_rules(&mut self) {
        let rules = self.rules.alignment_rules.clone();
        for rule in rules {
            let components_to_align = self.find_components_for_alignment(&rule);
            if !components_to_align.is_empty() {
                self.align_components(components_to_align, &rule);
            }
        }
    }
    
    /// Find components that match alignment target
    fn find_components_for_alignment(&self, rule: &AlignmentRule) -> Vec<String> {
        use crate::placement_rules::AlignmentTarget;
        
        match &rule.components {
            AlignmentTarget::ComponentType(comp_type) => {
                self.components.iter()
                    .filter(|(_, info)| info.component_type == *comp_type || comp_type == "All")
                    .map(|(name, _)| name.clone())
                    .collect()
            }
            AlignmentTarget::ComponentList(list) => list.clone(),
            _ => Vec::new(),
        }
    }
    
    /// Align components according to rule
    fn align_components(&mut self, components: Vec<String>, rule: &AlignmentRule) {
        use crate::placement_rules::AlignmentType;
        
        match &rule.alignment {
            AlignmentType::Horizontal(y) => {
                for comp_name in components {
                    if let Some(placement) = self.placements.get_mut(&comp_name) {
                        placement.position.1 = *y;
                    }
                }
            }
            AlignmentType::Vertical(x) => {
                for comp_name in components {
                    if let Some(placement) = self.placements.get_mut(&comp_name) {
                        placement.position.0 = *x;
                    }
                }
            }
            AlignmentType::Grid { x_pitch, y_pitch } => {
                for comp_name in components {
                    if let Some(placement) = self.placements.get_mut(&comp_name) {
                        // Snap to nearest grid point
                        placement.position.0 = (placement.position.0 / x_pitch).round() * x_pitch;
                        placement.position.1 = (placement.position.1 / y_pitch).round() * y_pitch;
                    }
                }
            }
            AlignmentType::CenterAlign => {
                // Calculate average position
                let mut avg_x = 0.0;
                let mut avg_y = 0.0;
                let mut count = 0;
                
                for comp_name in &components {
                    if let Some(placement) = self.placements.get(comp_name) {
                        avg_x += placement.position.0;
                        avg_y += placement.position.1;
                        count += 1;
                    }
                }
                
                if count > 0 {
                    avg_x /= count as f64;
                    avg_y /= count as f64;
                    
                    // Align all to average
                    for comp_name in components {
                        if let Some(placement) = self.placements.get_mut(&comp_name) {
                            placement.position.0 = avg_x;
                            placement.position.1 = avg_y;
                        }
                    }
                }
            }
        }
    }
    
    /// Optimize placement to avoid overlaps and improve aesthetics
    fn optimize_placement(&mut self) {
        // Simple force-directed optimization
        let iterations = 10;
        
        for _ in 0..iterations {
            let mut forces: HashMap<String, (f64, f64)> = HashMap::new();
            
            // Calculate repulsive forces between all components
            for (name1, place1) in &self.placements {
                let mut force_x = 0.0;
                let mut force_y = 0.0;
                
                for (name2, place2) in &self.placements {
                    if name1 != name2 {
                        let dx = place2.position.0 - place1.position.0;
                        let dy = place2.position.1 - place1.position.1;
                        let dist = (dx * dx + dy * dy).sqrt();
                        
                        if dist < self.rules.component_spacing.min_component_spacing * 2.0 && dist > 0.0 {
                            // Repulsive force
                            let force = self.rules.component_spacing.min_component_spacing / dist;
                            force_x -= force * dx / dist;
                            force_y -= force * dy / dist;
                        }
                    }
                }
                
                forces.insert(name1.clone(), (force_x, force_y));
            }
            
            // Apply forces with damping
            let damping = 0.1;
            for (name, (fx, fy)) in forces {
                if let Some(placement) = self.placements.get_mut(&name) {
                    placement.position.0 += fx * damping;
                    placement.position.1 += fy * damping;
                }
            }
        }
    }
    
    /// Scale and center placement to fit canvas
    fn fit_to_canvas(&mut self) {
        if self.placements.is_empty() {
            return;
        }
        
        // Find bounds
        let mut min_x = f64::MAX;
        let mut max_x = f64::MIN;
        let mut min_y = f64::MAX;
        let mut max_y = f64::MIN;
        
        for placement in self.placements.values() {
            min_x = min_x.min(placement.position.0 - placement.size.0 / 2.0);
            max_x = max_x.max(placement.position.0 + placement.size.0 / 2.0);
            min_y = min_y.min(placement.position.1 - placement.size.1 / 2.0);
            max_y = max_y.max(placement.position.1 + placement.size.1 / 2.0);
        }
        
        // Calculate scale and offset
        let margin = 50.0;
        let width = max_x - min_x;
        let height = max_y - min_y;
        
        let scale_x = (self.canvas_width - 2.0 * margin) / width;
        let scale_y = (self.canvas_height - 2.0 * margin) / height;
        let scale = scale_x.min(scale_y).min(1.5);  // Don't scale up too much
        
        let offset_x = (self.canvas_width - width * scale) / 2.0 - min_x * scale;
        let offset_y = (self.canvas_height - height * scale) / 2.0 - min_y * scale;
        
        // Apply transformation
        for placement in self.placements.values_mut() {
            placement.position.0 = placement.position.0 * scale + offset_x;
            placement.position.1 = placement.position.1 * scale + offset_y;
            placement.size.0 *= scale;
            placement.size.1 *= scale;
        }
    }
}