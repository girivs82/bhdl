/// Professional schematic placement rules extracted from best practices
/// These rules encode the intelligence that made hardcoded layouts look good

use std::collections::HashMap;

/// Placement rules that encode professional schematic layout principles
#[derive(Debug, Clone)]
pub struct PlacementRules {
    pub signal_flow_direction: SignalFlow,
    pub component_spacing: ComponentSpacing,
    pub grouping_rules: Vec<GroupingRule>,
    pub alignment_rules: Vec<AlignmentRule>,
    pub routing_channels: RoutingChannels,
}

#[derive(Debug, Clone)]
pub enum SignalFlow {
    LeftToRight,  // Most common: input -> processing -> output
    TopToBottom,  // Alternative: power at top, ground at bottom
    Radial,       // Center IC with peripherals around
}

#[derive(Debug, Clone)]
pub struct ComponentSpacing {
    pub min_component_spacing: f64,      // Minimum space between components
    pub routing_channel_width: f64,      // Space reserved for wires
    pub power_rail_offset: f64,          // Offset for power/ground rails
    pub group_spacing: f64,              // Extra space between functional groups
}

#[derive(Debug, Clone)]
pub struct GroupingRule {
    pub name: String,
    pub condition: GroupingCondition,
    pub placement: GroupPlacement,
}

#[derive(Debug, Clone)]
pub enum GroupingCondition {
    /// Components connected to same net
    SameNet(String),
    /// Components of same type  
    SameType(String),
    /// Components in same functional role
    SameRole(String),
    /// Components within electrical distance
    ElectricalProximity { max_hops: usize },
}

#[derive(Debug, Clone)]
pub enum GroupPlacement {
    /// Stack vertically
    VerticalStack { spacing: f64 },
    /// Arrange horizontally
    HorizontalArray { spacing: f64 },
    /// Place in grid
    Grid { rows: usize, cols: usize },
    /// Cluster around a point
    Cluster { radius: f64 },
}

#[derive(Debug, Clone)]
pub struct AlignmentRule {
    pub name: String,
    pub components: AlignmentTarget,
    pub alignment: AlignmentType,
}

#[derive(Debug, Clone)]
pub enum AlignmentTarget {
    /// All components of a type
    ComponentType(String),
    /// Components with specific role
    ComponentRole(String),
    /// Components in a net
    NetMembers(String),
    /// Specific component list
    ComponentList(Vec<String>),
}

#[derive(Debug, Clone)]
pub enum AlignmentType {
    /// Align along horizontal line
    Horizontal(f64),  // y-coordinate
    /// Align along vertical line
    Vertical(f64),    // x-coordinate
    /// Align to grid
    Grid { x_pitch: f64, y_pitch: f64 },
    /// Align centers
    CenterAlign,
}

#[derive(Debug, Clone)]
pub struct RoutingChannels {
    pub horizontal_channels: Vec<f64>,  // y-coordinates reserved for horizontal wires
    pub vertical_channels: Vec<f64>,    // x-coordinates reserved for vertical wires
    pub channel_width: f64,             // Width of each channel
    pub wire_spacing: f64,              // Minimum spacing between wires
}

/// Buck converter specific placement rules
pub fn buck_converter_rules() -> PlacementRules {
    PlacementRules {
        signal_flow_direction: SignalFlow::LeftToRight,
        
        component_spacing: ComponentSpacing {
            min_component_spacing: 30.0,
            routing_channel_width: 40.0,
            power_rail_offset: 50.0,
            group_spacing: 60.0,
        },
        
        grouping_rules: vec![
            GroupingRule {
                name: "Input Capacitors".to_string(),
                condition: GroupingCondition::SameNet("VIN".to_string()),
                placement: GroupPlacement::VerticalStack { spacing: 20.0 },
            },
            GroupingRule {
                name: "Output Capacitors".to_string(),
                condition: GroupingCondition::SameNet("VOUT".to_string()),
                placement: GroupPlacement::VerticalStack { spacing: 20.0 },
            },
            GroupingRule {
                name: "Feedback Network".to_string(),
                condition: GroupingCondition::SameRole("Feedback".to_string()),
                placement: GroupPlacement::VerticalStack { spacing: 15.0 },
            },
        ],
        
        alignment_rules: vec![
            AlignmentRule {
                name: "Power Stage Alignment".to_string(),
                components: AlignmentTarget::ComponentList(vec![
                    "IC".to_string(),
                    "Inductor".to_string(),
                ]),
                alignment: AlignmentType::Horizontal(300.0),
            },
            AlignmentRule {
                name: "Capacitor Alignment".to_string(),
                components: AlignmentTarget::ComponentType("Capacitor".to_string()),
                alignment: AlignmentType::Grid { 
                    x_pitch: 80.0, 
                    y_pitch: 60.0 
                },
            },
        ],
        
        routing_channels: RoutingChannels {
            horizontal_channels: vec![250.0, 300.0, 350.0],  // Power rails
            vertical_channels: vec![],  // Filled dynamically
            channel_width: 10.0,
            wire_spacing: 5.0,
        },
    }
}

/// Linear regulator specific placement rules
pub fn linear_regulator_rules() -> PlacementRules {
    PlacementRules {
        signal_flow_direction: SignalFlow::LeftToRight,
        
        component_spacing: ComponentSpacing {
            min_component_spacing: 40.0,
            routing_channel_width: 30.0,
            power_rail_offset: 40.0,
            group_spacing: 50.0,
        },
        
        grouping_rules: vec![
            GroupingRule {
                name: "Input Filter".to_string(),
                condition: GroupingCondition::ElectricalProximity { max_hops: 1 },
                placement: GroupPlacement::Cluster { radius: 50.0 },
            },
            GroupingRule {
                name: "Output Filter".to_string(),
                condition: GroupingCondition::ElectricalProximity { max_hops: 1 },
                placement: GroupPlacement::Cluster { radius: 50.0 },
            },
        ],
        
        alignment_rules: vec![
            AlignmentRule {
                name: "Main Path".to_string(),
                components: AlignmentTarget::ComponentRole("PowerPath".to_string()),
                alignment: AlignmentType::Horizontal(300.0),
            },
        ],
        
        routing_channels: RoutingChannels {
            horizontal_channels: vec![260.0, 300.0, 340.0],
            vertical_channels: vec![],
            channel_width: 8.0,
            wire_spacing: 4.0,
        },
    }
}

/// Generic placement rules for unknown topologies
pub fn generic_rules() -> PlacementRules {
    PlacementRules {
        signal_flow_direction: SignalFlow::LeftToRight,
        
        component_spacing: ComponentSpacing {
            min_component_spacing: 40.0,
            routing_channel_width: 40.0,
            power_rail_offset: 60.0,
            group_spacing: 80.0,
        },
        
        grouping_rules: vec![
            GroupingRule {
                name: "Same Type Components".to_string(),
                condition: GroupingCondition::SameType("Any".to_string()),
                placement: GroupPlacement::HorizontalArray { spacing: 30.0 },
            },
        ],
        
        alignment_rules: vec![
            AlignmentRule {
                name: "Grid Alignment".to_string(),
                components: AlignmentTarget::ComponentType("All".to_string()),
                alignment: AlignmentType::Grid { 
                    x_pitch: 100.0, 
                    y_pitch: 100.0 
                },
            },
        ],
        
        routing_channels: RoutingChannels {
            horizontal_channels: vec![],
            vertical_channels: vec![],
            channel_width: 10.0,
            wire_spacing: 5.0,
        },
    }
}

/// Placement stages that follow professional schematic creation workflow
#[derive(Debug)]
pub enum PlacementStage {
    /// Identify circuit topology
    TopologyIdentification,
    /// Place main components (IC, major actives)
    MainComponentPlacement,
    /// Place support components (passives)
    SupportComponentPlacement,
    /// Group related components
    ComponentGrouping,
    /// Align components to grid/lines
    ComponentAlignment,
    /// Create routing channels
    RoutingChannelCreation,
    /// Final optimization
    Optimization,
}

impl PlacementRules {
    /// Score a placement based on how well it follows the rules
    pub fn score_placement(&self, components: &HashMap<String, (f64, f64)>) -> f64 {
        let mut score = 100.0;
        
        // Check minimum spacing
        for (name1, pos1) in components {
            for (name2, pos2) in components {
                if name1 != name2 {
                    let dist = ((pos1.0 - pos2.0).powi(2) + (pos1.1 - pos2.1).powi(2)).sqrt();
                    if dist < self.component_spacing.min_component_spacing {
                        score -= 10.0;  // Penalty for too close
                    }
                }
            }
        }
        
        // Check alignment rules
        for rule in &self.alignment_rules {
            // Check if aligned components are actually aligned
            // This is simplified - real implementation would be more sophisticated
            score += 5.0;  // Bonus for following alignment
        }
        
        // Check grouping rules
        for rule in &self.grouping_rules {
            // Check if grouped components are actually grouped
            score += 5.0;  // Bonus for proper grouping
        }
        
        score
    }
}