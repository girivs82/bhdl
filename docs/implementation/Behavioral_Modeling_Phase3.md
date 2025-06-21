# BHDL Behavioral Modeling System - Phase 3 Implementation

## Overview

Phase 3 of the BHDL simulation engine implements a comprehensive behavioral modeling system that allows components to be simulated with realistic electrical behavior. This system supports analog, digital, and mixed-signal models with proper state management and signal propagation.

## Architecture

### Component Model Interface

The core `BehavioralModel` trait provides a unified interface for all component models:

```rust
pub trait BehavioralModel: Send + Sync {
    fn name(&self) -> &str;
    fn model_type(&self) -> ModelType;
    fn ports(&self) -> &[ModelPort];
    fn initialize(&mut self, parameters: &HashMap<String, f64>) -> SimulationResult<()>;
    fn update(&mut self, inputs: &HashMap<String, PinValue>, time: f64, dt: f64) 
        -> SimulationResult<HashMap<String, PinValue>>;
    fn get_state(&self) -> HashMap<String, f64>;
    fn reset(&mut self);
}
```

### Model Types

1. **Analog Models**: Continuous-time electrical behavior
   - Resistor: V = I × R
   - Capacitor: I = C × dV/dt
   - Inductor: V = L × dI/dt
   - Voltage Source: Fixed or time-varying voltage

2. **Digital Models**: Discrete logic behavior
   - Logic Gates: NOT, AND, OR, NAND, NOR, XOR
   - Sequential Logic: D Flip-Flop, JK Flip-Flop, Latches
   - Timing: Propagation delays, setup/hold times

3. **Mixed-Signal Models**: Bridge analog and digital domains
   - ADC: Analog-to-Digital Converter with resolution and conversion time
   - DAC: Digital-to-Analog Converter with settling time
   - Comparators: Analog input to digital output with hysteresis

## Implementation Details

### Analog Behavior Framework

The `AnalogBehavior` trait provides a structured way to implement analog component behavior:

```rust
pub trait AnalogBehavior {
    fn calculate(&mut self, inputs: &AnalogInputs, state: &AnalogState, dt: f64) -> AnalogOutputs;
    fn update_state(&mut self, state: &mut AnalogState, inputs: &AnalogInputs, dt: f64);
}
```

Key features:
- Voltage and current calculation
- Impedance modeling
- State variable management (e.g., capacitor voltage, inductor current)
- Time-step based integration

### Digital Behavior Framework

The `DigitalBehavior` trait handles discrete logic:

```rust
pub trait DigitalBehavior {
    fn calculate(&mut self, inputs: &DigitalInputs, state: &DigitalState) -> DigitalOutputs;
    fn update_state(&mut self, state: &mut DigitalState, inputs: &DigitalInputs);
    fn propagation_delay(&self, output: &str) -> f64;
}
```

Key features:
- Logic level calculation (High, Low, Unknown, HighZ)
- Drive strength modeling
- Propagation delay handling
- Sequential state management

### Mixed-Signal Interface

The `MixedSignalInterface` trait provides domain conversion:

```rust
pub trait MixedSignalInterface {
    fn analog_to_digital(&self, voltage: f64, threshold_high: f64, threshold_low: f64) -> LogicLevel;
    fn digital_to_analog(&self, level: LogicLevel, vdd: f64, vss: f64) -> f64;
    fn port_domain(&self, port: &str) -> SignalDomain;
}
```

## Example Models

### Resistor Model

```rust
impl AnalogBehavior for ResistorBehavior {
    fn calculate(&mut self, inputs: &AnalogInputs, _state: &AnalogState, _dt: f64) -> AnalogOutputs {
        let mut outputs = AnalogOutputs::default();
        
        if let (Some(&v1), Some(&v2)) = (inputs.voltages.get("1"), inputs.voltages.get("2")) {
            let voltage_diff = v1 - v2;
            let current = voltage_diff / self.resistance;
            
            outputs.currents.insert("1".to_string(), -current);
            outputs.currents.insert("2".to_string(), current);
            outputs.impedances.insert("1".to_string(), self.resistance);
            outputs.impedances.insert("2".to_string(), self.resistance);
        }
        
        outputs
    }
}
```

### D Flip-Flop Model

```rust
impl DigitalBehavior for DFlipFlopBehavior {
    fn calculate(&mut self, inputs: &DigitalInputs, state: &DigitalState) -> DigitalOutputs {
        let clk = inputs.levels.get("CLK").copied().unwrap_or(LogicLevel::Unknown);
        let d = inputs.levels.get("D").copied().unwrap_or(LogicLevel::Unknown);
        
        // Detect rising edge
        let rising_edge = match (self.last_clk, clk) {
            (Some(LogicLevel::Low), LogicLevel::High) => true,
            _ => false,
        };
        
        // Output D on rising edge, otherwise stored Q
        let q = if rising_edge && d != LogicLevel::Unknown {
            d
        } else {
            state.registers.get("Q").copied().unwrap_or(LogicLevel::Low)
        };
        
        outputs.levels.insert("Q".to_string(), q);
        outputs.levels.insert("Q_BAR".to_string(), !q);
    }
}
```

### ADC Model

```rust
impl BehavioralModel for AdcModel {
    fn update(&mut self, inputs: &HashMap<String, PinValue>, time: f64, _dt: f64) 
        -> SimulationResult<HashMap<String, PinValue>> {
        let vin = inputs.get("VIN").map(|p| p.voltage).unwrap_or(0.0);
        let start = inputs.get("START").and_then(|p| p.logic_level).unwrap_or(LogicLevel::Low);
        
        // Start conversion on rising edge
        if start == LogicLevel::High && !self.converting {
            self.converting = true;
            self.conversion_start_time = time;
            self.last_analog_value = vin;
        }
        
        // Check if conversion complete
        if self.converting && (time - self.conversion_start_time) >= self.conversion_time {
            self.converting = false;
            
            // Convert to digital
            let normalized = (self.last_analog_value - vref_low) / (vref_high - vref_low);
            let digital_value = (normalized.clamp(0.0, 1.0) * ((1 << self.resolution) - 1) as f64) as u32;
            
            // Output digital bits
            for i in 0..self.resolution {
                let bit = (digital_value >> i) & 1;
                outputs.insert(format!("D{}", i), digital_pin_value(bit == 1));
            }
        }
    }
}
```

## Model Registry

The `ModelRegistry` manages all behavioral models in the simulation:

```rust
pub struct ModelRegistry {
    models: HashMap<InstanceId, Box<dyn BehavioralModel>>,
}

impl ModelRegistry {
    pub fn register(&mut self, instance: InstanceId, model: Box<dyn BehavioralModel>);
    pub fn update_all(&mut self, inputs: &HashMap<InstanceId, HashMap<String, PinValue>>, 
                      time: f64, dt: f64) -> HashMap<InstanceId, HashMap<String, PinValue>>;
}
```

## Model Library

A factory system creates standard models:

```rust
pub struct ModelLibrary {
    factories: HashMap<String, Box<dyn ModelFactory>>,
}

impl ModelLibrary {
    pub fn create_model(&self, model_type: &str, params: &HashMap<String, f64>) 
        -> Option<Box<dyn BehavioralModel>>;
}
```

Standard models include:
- Passive components: R, C, L
- Logic gates: NOT, AND, OR, NAND, NOR, XOR, XNOR
- Sequential logic: DFF, JKFF, Latch
- Sources: DC voltage, current, signal generator
- Converters: ADC, DAC

## Integration with Simulation Engine

The behavioral models integrate with the simulation engine through:

1. **Circuit Loader**: Maps netlist instances to behavioral models
2. **State Manager**: Maintains model states across time steps
3. **Propagation Engine**: Distributes pin values to/from models
4. **Time Manager**: Coordinates model updates with simulation time

## Testing

Comprehensive test coverage includes:
- Unit tests for each model type
- Integration tests with model registry
- Timing accuracy tests
- State preservation tests
- Mixed-signal conversion tests

## Performance Considerations

1. **Model Caching**: Reuse model instances where possible
2. **Parallel Updates**: Models can be updated in parallel (thread-safe)
3. **Lazy Evaluation**: Only update models with changed inputs
4. **State Snapshots**: Efficient state save/restore for time stepping

## Future Enhancements

1. **SPICE Model Import**: Parse SPICE models into behavioral models
2. **User-Defined Models**: Allow custom model definitions in BHDL
3. **Parameter Sweeps**: Built-in support for parameter variation
4. **Model Validation**: Automatic checking of model parameters
5. **Thermal Modeling**: Temperature effects on component behavior

## Usage Example

```rust
// Create model library
let mut library = ModelLibrary::new();
library.register_standard_models();

// Create model registry
let mut registry = ModelRegistry::new();

// Register component models
for (instance_id, component) in netlist.instances() {
    if let Some(model) = library.create_model(&component.model_type, &component.parameters) {
        registry.register(instance_id, model);
    }
}

// During simulation
let outputs = registry.update_all(&pin_inputs, sim_time, dt)?;
```

## Conclusion

Phase 3 provides a robust foundation for component behavioral modeling in BHDL simulation. The system is extensible, efficient, and accurately models real-world component behavior across analog, digital, and mixed-signal domains.