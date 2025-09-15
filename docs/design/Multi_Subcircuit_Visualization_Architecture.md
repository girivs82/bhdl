# Multi-Subcircuit Visualization Architecture

## Overview

This document outlines the architecture for extending the BHDL visualizer to handle complex PCB designs with multiple interconnected subcircuits (power management, SoCs, communication interfaces, analog frontends, etc.).

## Current State

✅ **Completed**: Generic single-circuit visualizer working with:
- Component role detection from SPICE analysis
- Database metadata integration  
- Signal flow-based placement
- No hardcoded coordinates
- Professional SVG output

## Vision: Multi-Subcircuit Support

Enable visualization of complete PCB designs with multiple functional blocks:
- **Power Management**: Buck/boost converters, LDOs, power sequencing
- **Processing**: MCUs, SoCs, FPGAs with supporting components
- **Communication**: Ethernet, USB, wireless modules with matching networks
- **Analog**: ADCs, DACs, amplifiers with precision references
- **Interface**: Connectors, protection circuits, level shifters

## Architecture Components

### 1. Subcircuit Detection & Grouping

```rust
pub struct SubcircuitDetector {
    analysis_engine: TopologyAnalyzer,
    clustering_rules: ClusteringRuleSet,
}

impl SubcircuitDetector {
    /// Identify functional blocks from netlist topology
    pub fn identify_subcircuits(&self, netlist: &Netlist) -> Vec<Subcircuit> {
        // 1. IC-centric clustering (components connected to same IC)
        // 2. Power domain grouping (components on same voltage rail)
        // 3. Signal path tracing (differential pairs, clock distribution)
        // 4. Component role analysis (from SPICE topology detection)
    }
}

pub struct Subcircuit {
    subcircuit_type: SubcircuitType,
    primary_ic: Option<InstanceId>,
    supporting_components: Vec<InstanceId>,
    power_connections: Vec<NetId>,
    signal_connections: Vec<NetId>,
    placement_constraints: PlacementConstraints,
}
```

### 2. Subcircuit-Specific Layout Rules

Each subcircuit type has specialized placement and routing knowledge:

```rust
pub enum SubcircuitType {
    PowerRegulator {
        topology: PowerTopology, // Buck, Boost, LDO, etc.
        input_filter_placement: FilterPlacement,
        output_filter_placement: FilterPlacement,
        feedback_network_style: FeedbackStyle,
        switching_node_isolation: bool,
    },
    
    ProcessingUnit {
        architecture: ProcessorArchitecture, // MCU, SoC, FPGA
        power_sequencing_required: bool,
        io_bank_grouping: IoBankStrategy,
        crystal_placement: CrystalPlacement,
        decoupling_strategy: DecouplingStrategy,
    },
    
    CommunicationInterface {
        protocol: CommProtocol, // Ethernet, USB, PCIe, etc.
        differential_pair_matching: bool,
        emi_filtering_required: bool,
        isolation_required: bool,
        impedance_control: ImpedanceRequirements,
    },
    
    AnalogFrontend {
        signal_chain: AnalogChain, // ADC, DAC, Amplifier, etc.
        precision_requirements: PrecisionLevel,
        isolation_requirements: IsolationLevel,
        reference_distribution: ReferenceStrategy,
        sensitive_signal_shielding: bool,
    },
    
    InterfaceProtection {
        protection_type: ProtectionType, // ESD, overvoltage, etc.
        signal_types: Vec<SignalType>,
        connector_placement: ConnectorStrategy,
    },
}

pub struct FilterPlacement {
    position: RelativePosition, // Left, Right, Above, Below
    grouping: GroupingStyle,    // Vertical stack, horizontal array
    spacing: SpacingRequirement,
    alignment: AlignmentStrategy,
}

pub enum RelativePosition {
    LeftOfIC { distance: f64, alignment: Alignment },
    RightOfIC { distance: f64, alignment: Alignment },
    AboveIC { distance: f64, alignment: Alignment },
    BelowIC { distance: f64, alignment: Alignment },
}
```

### 3. Hierarchical Layout Engine

```rust
pub struct HierarchicalLayoutEngine {
    subcircuit_detector: SubcircuitDetector,
    subcircuit_layouters: HashMap<SubcircuitType, Box<dyn SubcircuitLayouter>>,
    arrangement_strategy: ArrangementStrategy,
    inter_subcircuit_router: InterSubcircuitRouter,
}

impl HierarchicalLayoutEngine {
    pub fn generate_layout(&mut self, netlist: &Netlist) -> CircuitLayout {
        // Phase 1: Subcircuit Detection
        let subcircuits = self.subcircuit_detector.identify_subcircuits(netlist);
        
        // Phase 2: Individual Subcircuit Layout
        let mut subcircuit_layouts = Vec::new();
        for subcircuit in subcircuits {
            let layouter = self.get_specialized_layouter(&subcircuit.subcircuit_type);
            let layout = layouter.layout_subcircuit(&subcircuit, netlist);
            subcircuit_layouts.push(SubcircuitLayout {
                subcircuit_id: subcircuit.id,
                internal_layout: layout,
                bounding_box: layout.get_bounding_box(),
                connection_points: self.extract_connection_points(&layout),
            });
        }
        
        // Phase 3: Subcircuit Arrangement
        let arranged_layout = self.arrangement_strategy
            .arrange_subcircuits(subcircuit_layouts, netlist);
        
        // Phase 4: Inter-Subcircuit Routing
        self.inter_subcircuit_router
            .route_connections(arranged_layout, netlist)
    }
}

trait SubcircuitLayouter {
    fn layout_subcircuit(&self, subcircuit: &Subcircuit, netlist: &Netlist) -> CircuitLayout;
    fn get_connection_points(&self, layout: &CircuitLayout) -> Vec<ConnectionPoint>;
    fn get_placement_constraints(&self) -> PlacementConstraints;
}
```

### 4. Signal Flow-Based Arrangement

```rust
pub struct SignalFlowArrangement {
    flow_analyzer: SignalFlowAnalyzer,
    placement_optimizer: PlacementOptimizer,
}

impl ArrangementStrategy for SignalFlowArrangement {
    fn arrange_subcircuits(&self, subcircuits: Vec<SubcircuitLayout>, netlist: &Netlist) -> ArrangedLayout {
        // 1. Analyze signal flow between subcircuits
        let flow_graph = self.flow_analyzer.build_flow_graph(&subcircuits, netlist);
        
        // 2. Determine optimal arrangement based on flow
        let arrangement = match flow_graph.primary_flow_direction() {
            FlowDirection::LeftToRight => self.arrange_horizontal(subcircuits, flow_graph),
            FlowDirection::TopToBottom => self.arrange_vertical(subcircuits, flow_graph),
            FlowDirection::Hierarchical => self.arrange_hierarchical(subcircuits, flow_graph),
        };
        
        // 3. Optimize placement to minimize wire lengths
        self.placement_optimizer.optimize_placement(arrangement)
    }
}

pub enum FlowDirection {
    LeftToRight,    // Power → Processing → Output (typical)
    TopToBottom,    // Power distribution from top
    Hierarchical,   // Complex multi-level flows
}
```

### 5. Specialized Subcircuit Layouters

#### Power Regulator Layouter
```rust
pub struct PowerRegulatorLayouter {
    topology_rules: PowerTopologyRules,
    thermal_considerations: ThermalRules,
    emi_rules: EMIRules,
}

impl SubcircuitLayouter for PowerRegulatorLayouter {
    fn layout_subcircuit(&self, subcircuit: &Subcircuit, netlist: &Netlist) -> CircuitLayout {
        // 1. Identify power topology (buck, boost, LDO)
        // 2. Place switching IC at center
        // 3. Position input caps to left (bulk + ceramic)
        // 4. Position output caps to right (bulk + ceramic)
        // 5. Route switching node with minimal loop area
        // 6. Place feedback network below IC
        // 7. Add thermal vias for high-current paths
    }
}
```

#### SoC/MCU Layouter
```rust
pub struct ProcessingUnitLayouter {
    pin_function_analyzer: PinFunctionAnalyzer,
    power_sequencing_rules: PowerSequencingRules,
    io_grouping_strategy: IoGroupingStrategy,
}

impl SubcircuitLayouter for ProcessingUnitLayouter {
    fn layout_subcircuit(&self, subcircuit: &Subcircuit, netlist: &Netlist) -> CircuitLayout {
        // 1. Place main IC at center
        // 2. Group power pins and place decoupling caps nearby
        // 3. Group I/O pins by function (SPI, I2C, GPIO banks)
        // 4. Place crystal near oscillator pins
        // 5. Route power with proper plane usage
        // 6. Maintain proper spacing for high-speed signals
    }
}
```

#### Ethernet Interface Layouter
```rust
pub struct EthernetLayouter {
    differential_pair_rules: DifferentialPairRules,
    magnetics_placement: MagneticsRules,
    emi_filtering: EMIFilterRules,
}

impl SubcircuitLayouter for EthernetLayouter {
    fn layout_subcircuit(&self, subcircuit: &Subcircuit, netlist: &Netlist) -> CircuitLayout {
        // 1. Place Ethernet IC
        // 2. Position magnetics with proper isolation
        // 3. Route differential pairs with length matching
        // 4. Place EMI filters inline with signal path
        // 5. Add common mode chokes
        // 6. Position connector with proper ESD protection
    }
}
```

### 6. Inter-Subcircuit Routing

```rust
pub struct InterSubcircuitRouter {
    routing_strategy: RoutingStrategy,
    constraint_solver: RoutingConstraintSolver,
}

impl InterSubcircuitRouter {
    pub fn route_connections(&mut self, layout: ArrangedLayout, netlist: &Netlist) -> CircuitLayout {
        // 1. Identify nets that span multiple subcircuits
        let inter_subcircuit_nets = self.find_inter_subcircuit_nets(&layout, netlist);
        
        // 2. Route power distribution first (thick traces)
        self.route_power_distribution(inter_subcircuit_nets.power_nets, &layout);
        
        // 3. Route high-speed signals (controlled impedance)
        self.route_high_speed_signals(inter_subcircuit_nets.high_speed, &layout);
        
        // 4. Route remaining signals
        self.route_remaining_signals(inter_subcircuit_nets.other, &layout);
        
        // 5. Add power planes and ground planes
        self.add_power_ground_planes(&layout);
    }
}
```

## Implementation Phases

### Phase 1: Enhanced Single-Subcircuit Visualization (Current Focus)
- ✅ Generic netlist visualizer working
- 🔄 **Perfect component symbol rendering**
- 🔄 **Improve wire routing (orthogonal, proper pin connections)**
- 🔄 **Add more component types (inductors, crystals, connectors)**
- 🔄 **Better spacing and alignment algorithms**

### Phase 2: Subcircuit Detection
- Implement topology analysis for IC-centric grouping
- Add power domain detection
- Create component role clustering algorithms
- Build subcircuit classification engine

### Phase 3: Specialized Subcircuit Layouters
- Power regulator layouter with switching topology awareness
- SoC/MCU layouter with pin function grouping
- Communication interface layouters (Ethernet, USB, etc.)
- Analog frontend layouter with precision requirements

### Phase 4: Multi-Subcircuit Arrangement
- Signal flow analysis between subcircuits
- Arrangement optimization algorithms
- Inter-subcircuit routing engine
- Power plane and ground plane generation

### Phase 5: Advanced Features
- Design rule checking (DRC) integration
- Thermal analysis integration
- EMI/EMC considerations
- Manufacturing constraints

## Integration with Existing BHDL Pipeline

This architecture builds on our existing success:

```
Parser → AST → Analyzer → Synthesizer → Visualizer
                ↓            ↓           ↓
           Symbol Table  Component    Generic
           + Analysis    Database     Netlist
           Results       + Metadata   Visualizer
                                         ↓
                                   Multi-Subcircuit
                                   Hierarchical
                                   Layout Engine
```

The multi-subcircuit engine will:
- **Leverage existing metadata** from analyzer and synthesizer
- **Use the same component database** for consistency
- **Build on the generic visualizer** as the foundation
- **Add hierarchical intelligence** without breaking existing functionality

## Benefits

1. **Professional PCB Layout Quality**: Automated placement following industry best practices
2. **Scalable Architecture**: Handles simple circuits to complex multi-board systems  
3. **Extensible Design**: Easy to add new subcircuit types and layout rules
4. **Metadata-Driven**: Uses rich analysis data already in the pipeline
5. **Design Rule Awareness**: Incorporates electrical, thermal, and manufacturing constraints

This architecture positions BHDL as a complete solution for electronic design automation, from circuit specification to professional PCB layout generation.