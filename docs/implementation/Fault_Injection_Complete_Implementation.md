# Fault Injection Complete Implementation Plan

## Current State Analysis

### What's Working
1. Basic fault injection framework exists in `bhdl-testbench/src/fault_injection.rs`
2. FaultInjector can modify component models (resistance, capacitance, etc.)
3. TestbenchRunner has `run_with_fault()` and `run_fault_campaign()` methods
4. Standard fault scenarios are defined (resistor_short, drift, LED_open, aging)

### What's Broken
1. **SPICE Circuit Building Issue**
   - Components aren't being added to SPICE circuit in coordinator
   - Node voltages show only 0.01V (initialization value)
   - No branch currents are generated
   - This causes fault injection to fail since components don't exist in SPICE

2. **Component Model Mapping**
   - LED models aren't being recognized in adaptive solver
   - The coordinator skips components during SPICE conversion
   - Database component mapping may be interfering

### Debug Output Shows
```
Processing 2 instances
  Instance: R1 (type: ModuleId(1v1))
    Module found: [But then nothing happens - no SPICE component created]
```

## Implementation Fixes Needed

### 1. Fix SPICE Circuit Building (Priority: HIGH)

**Location**: `bhdl-testbench/src/coordinator.rs`, lines 225-400

**Issue**: The code finds instances but doesn't create SPICE components

**Fix Required**:
```rust
// Around line 296, after getting module type
match module.name.as_str() {
    "Resistor" | "Res" | "R" => {
        // Get resistance value from instance attributes
        let resistance = instance.attributes.get("value")
            .and_then(|v| parse_value_with_units(v))
            .unwrap_or(1000.0);
        
        // Create SPICE branch
        let branch_id = circuit.add_branch(
            comp_name.clone(),
            node1,
            node2,
            BranchType::Resistor,
            resistance
        );
        
        // Also ensure model is in solver
        let model = ComponentModel::Resistor {
            resistance,
            tolerance: 5.0,
            limits: ElectricalLimits::default()
        };
        
        // Store in models map for later use
        models.insert(comp_name.clone(), model);
    }
    "LED" => {
        // Similar for LED...
    }
}
```

### 2. Fix Model Storage Architecture

**Issue**: Models are stored in solver but not accessible during fault injection

**Solution**: Store models at SpiceSolverWrapper level
```rust
struct SpiceSolverWrapper {
    circuit: Circuit,
    solver: AdaptiveCircuitSolver,
    signal_mapping: HashMap<SignalRef, NodeMapping>,
    component_models: HashMap<String, ComponentModel>, // Add this
}
```

### 3. Complete Test Example

**File**: `bhdl-testbench/src/bin/test_fault_injection.rs`

**Current Issue**: Testbench parsing fails on power measurement

**Fix**: Already applied - removed `R1.power` measurement

## Enhanced Fault Injection Architecture

### Component-Defined Fault Behaviors

**New Requirement**: Components (especially ICs) need to define their own internal fault propagation

**Example**: If an IC's reset pin shorts to ground:
- All outputs go high-impedance
- Internal state machines freeze
- Communication buses (SPI/I2C) become inactive
- Downstream components lose communication

### Proposed Architecture

#### 1. Component Fault Behavior Trait
```rust
// In bhdl-common/src/fault_behavior.rs
pub trait ComponentFaultBehavior {
    /// Define how this component responds to various fault conditions
    fn fault_response(&self, fault: &FaultCondition) -> FaultResponse;
    
    /// Define internal fault propagation rules
    fn propagation_rules(&self) -> Vec<FaultPropagationRule>;
    
    /// Check if component is in failed state
    fn is_failed(&self) -> bool;
    
    /// Get current fault state of all pins/internals
    fn fault_state(&self) -> ComponentFaultState;
}
```

#### 2. BHDL Syntax Extension
```bhdl
entity STM32F103 {
    // Regular pins...
    
    fault_behavior {
        rule reset_pin_held_low {
            trigger: pin_short_to_ground(NRST);
            effects: [
                all_outputs -> high_z,
                internal_state -> frozen,
                SPI_MOSI -> high_z,
                SPI_SCK -> high_z,
                assert_failure("MCU held in reset", critical)
            ];
        }
        
        rule brownout_detection {
            trigger: supply_voltage(VDD) < 2.0V;
            effects: [
                internal_reset,
                all_outputs -> undefined,
                flash_corruption possible
            ];
        }
    }
}
```

#### 3. Mixed-Signal Fault Propagation

**Key Innovation**: Faults propagate between electrical (SPICE) and behavioral domains

```rust
pub struct FaultEventBus {
    electrical_faults: Vec<ElectricalFault>,
    behavioral_faults: Vec<BehavioralFault>,
    propagation_queue: VecDeque<PropagatedFault>,
    affected_components: HashSet<ComponentId>,
}

impl MixedSignalInterface for FaultAwareInterface {
    fn on_electrical_fault(&mut self, fault: ElectricalFault) -> Result<()> {
        // Component checks its fault rules
        let response = self.component.fault_response(&fault);
        
        // Queue behavioral effects
        for effect in response.effects {
            self.fault_bus.queue_behavioral_fault(effect);
        }
        
        Ok(())
    }
}
```

### Test Scenarios to Implement

#### 1. Basic LED Circuit with R1 Short
- Demonstrates overcurrent through LED
- Shows cascade to LED failure
- Validates safety analysis integration

#### 2. MCU Reset Pin Fault
```bhdl
testbench TB_MCU_Reset_Fault {
    faults {
        scenario "reset_stuck_low" {
            at 10ms: short_to_ground(MCU1.NRST);
            
            expect_behavioral {
                MCU1.state == "held_in_reset";
                all_pins(MCU1, output) == high_z;
                SPI_BUS.active == false;
            }
            
            expect_cascade {
                SENSOR1.error == "no_spi_clock" after 50ms;
            }
        }
    }
}
```

#### 3. Power Supply Cascade
```bhdl
testbench TB_Power_Cascade {
    faults {
        scenario "regulator_thermal_shutdown" {
            progressive: REG1.load_current from 1A to 2A over 100ms;
            
            expect {
                REG1.junction_temp > 150C within 200ms;
                REG1.thermal_shutdown == true;
                @3V3 < 0.5V after shutdown;
                all_components_on(@3V3).powered == false;
            }
        }
    }
}
```

## Implementation Timeline

### Phase 1: Fix Current Issues (1-2 days)
1. Fix SPICE circuit building in coordinator
2. Ensure component models are accessible for fault injection
3. Complete basic R1 short demo

### Phase 2: Core Infrastructure (1 week)
1. Implement ComponentFaultBehavior trait
2. Create fault event bus
3. Add fault state to component models

### Phase 3: Parser Support (1 week)
1. Add fault_behavior block to BHDL grammar
2. Parse fault rules and effects
3. Store in component definitions

### Phase 4: Simulation Integration (1 week)
1. Integrate fault bus with coordinators
2. Implement cross-domain propagation
3. Add behavioral assertions

### Phase 5: Standard Components (1 week)
1. Add fault behaviors to voltage regulators
2. Add MCU fault models
3. Add power device failures

### Phase 6: Analysis & Reporting (1 week)
1. Enhance safety analyzer
2. Generate FMEA reports
3. Create fault tree visualizations

## Key Files to Modify

1. **Immediate Fixes**
   - `/bhdl-testbench/src/coordinator.rs` - Fix SPICE building
   - `/bhdl-testbench/src/fault_injection.rs` - Add model access
   - `/bhdl-spice/src/adaptive_solver.rs` - Ensure LED recognition

2. **New Infrastructure**
   - `/bhdl-common/src/fault_behavior.rs` - New trait
   - `/bhdl-parser/src/blocks.rs` - Parse fault_behavior
   - `/bhdl-sim/src/fault_propagation.rs` - Event bus

3. **Component Updates**
   - `/bhdl-stdlib/regulators/lm7805.bhdl` - Add fault behavior
   - `/bhdl-stdlib/ics/mcu_generic.bhdl` - MCU fault model

## Success Criteria

1. Basic R1 short scenario shows correct overcurrent
2. Component fault behaviors parse and store correctly
3. MCU reset fault propagates to behavioral effects
4. Safety analyzer detects cascade failures
5. FMEA reports include behavioral fault paths

## Notes for Resuming

When resuming this work:
1. Start with fixing coordinator.rs SPICE building
2. Use debug prints to verify components are added
3. Check that models are stored and accessible
4. Verify current/voltage extraction works
5. Then proceed to enhanced architecture

The key insight is that components need to define their own failure modes because external electrical simulation cannot predict internal IC behavior during faults.