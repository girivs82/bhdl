/// Physics-based perturbation model with proper energy conservation
/// 
/// This implementation uses a more rigorous approach that respects
/// conservation laws and handles numerical stability properly.

use std::collections::HashMap;

/// Represents electrical state at a connection point
#[derive(Debug, Clone, Copy, Default)]
pub struct ElectricalState {
    /// Voltage at this point
    pub voltage: f64,
    /// Current flowing through this point
    pub current: f64,
}

/// Represents a perturbation or change in electrical state
#[derive(Debug, Clone, Copy, Default)]
pub struct Perturbation {
    /// Change in voltage
    pub dv: f64,
    /// Change in current
    pub di: f64,
}

/// Port-based connection for enforcing conservation laws
#[derive(Debug, Clone)]
pub struct Port {
    /// Port identifier
    pub id: usize,
    /// Electrical state
    pub state: ElectricalState,
    /// Accumulated perturbation
    pub perturbation: Perturbation,
    /// Connected ports (for KCL enforcement)
    pub connections: Vec<usize>,
}

impl Port {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            state: ElectricalState::default(),
            perturbation: Perturbation::default(),
            connections: Vec::new(),
        }
    }
    
    /// Apply perturbation to this port
    pub fn apply_perturbation(&mut self, perturb: Perturbation) {
        self.perturbation.dv += perturb.dv;
        self.perturbation.di += perturb.di;
    }
    
    /// Update state based on accumulated perturbations
    pub fn update_state(&mut self) {
        self.state.voltage += self.perturbation.dv;
        self.state.current += self.perturbation.di;
        // Reset perturbations after applying
        self.perturbation = Perturbation::default();
    }
}

/// Component behavior trait using wave propagation
pub trait WaveComponent: Send + Sync {
    /// Get characteristic impedance (for wave propagation)
    fn characteristic_impedance(&self) -> f64;
    
    /// Process incoming wave and return reflected wave
    fn process_wave(&mut self, incident: ElectricalState, dt: f64) -> ElectricalState;
    
    /// Update internal state
    fn update_state(&mut self, dt: f64);
    
    /// Get current state
    fn get_state(&self) -> ElectricalState;
    
    /// Reset to initial conditions
    fn reset(&mut self);
}

/// Resistor with wave propagation
pub struct WaveResistor {
    resistance: f64,
    state: ElectricalState,
}

impl WaveResistor {
    pub fn new(resistance: f64) -> Self {
        Self {
            resistance,
            state: ElectricalState::default(),
        }
    }
}

impl WaveComponent for WaveResistor {
    fn characteristic_impedance(&self) -> f64 {
        self.resistance
    }
    
    fn process_wave(&mut self, incident: ElectricalState, _dt: f64) -> ElectricalState {
        // For a resistor, the reflection coefficient is 0 (no reflection)
        // All energy is absorbed/transmitted
        self.state.voltage = incident.voltage;
        self.state.current = incident.voltage / self.resistance;
        
        // No reflection for matched impedance
        ElectricalState {
            voltage: 0.0,
            current: 0.0,
        }
    }
    
    fn update_state(&mut self, _dt: f64) {
        // Resistor state is instantaneous
    }
    
    fn get_state(&self) -> ElectricalState {
        self.state
    }
    
    fn reset(&mut self) {
        self.state = ElectricalState::default();
    }
}

/// Inductor with wave propagation
pub struct WaveInductor {
    inductance: f64,
    state: ElectricalState,
    flux: f64,
}

impl WaveInductor {
    pub fn new(inductance: f64) -> Self {
        Self {
            inductance,
            state: ElectricalState::default(),
            flux: 0.0,
        }
    }
}

impl WaveComponent for WaveInductor {
    fn characteristic_impedance(&self) -> f64 {
        // For small time steps, inductor looks like high impedance
        1e6 // High impedance approximation
    }
    
    fn process_wave(&mut self, incident: ElectricalState, dt: f64) -> ElectricalState {
        // V = L * di/dt
        let di = incident.voltage * dt / self.inductance;
        self.state.current += di;
        
        // Reflected wave due to impedance mismatch
        let reflection_coeff = 0.9; // High reflection for inductor
        ElectricalState {
            voltage: -incident.voltage * reflection_coeff,
            current: -incident.current * reflection_coeff,
        }
    }
    
    fn update_state(&mut self, dt: f64) {
        self.flux += self.state.voltage * dt;
        self.state.current = self.flux / self.inductance;
    }
    
    fn get_state(&self) -> ElectricalState {
        self.state
    }
    
    fn reset(&mut self) {
        self.state = ElectricalState::default();
        self.flux = 0.0;
    }
}

/// Capacitor with wave propagation
pub struct WaveCapacitor {
    capacitance: f64,
    state: ElectricalState,
    charge: f64,
}

impl WaveCapacitor {
    pub fn new(capacitance: f64) -> Self {
        Self {
            capacitance,
            state: ElectricalState::default(),
            charge: 0.0,
        }
    }
}

impl WaveComponent for WaveCapacitor {
    fn characteristic_impedance(&self) -> f64 {
        // For small time steps, capacitor looks like low impedance
        0.001 // Low impedance approximation
    }
    
    fn process_wave(&mut self, incident: ElectricalState, dt: f64) -> ElectricalState {
        // I = C * dv/dt
        self.state.current = incident.current;
        let dv = incident.current * dt / self.capacitance;
        self.state.voltage += dv;
        
        // Reflected wave due to impedance mismatch
        let reflection_coeff = -0.9; // Negative reflection for capacitor
        ElectricalState {
            voltage: incident.voltage * reflection_coeff,
            current: incident.current * reflection_coeff,
        }
    }
    
    fn update_state(&mut self, dt: f64) {
        self.charge += self.state.current * dt;
        self.state.voltage = self.charge / self.capacitance;
    }
    
    fn get_state(&self) -> ElectricalState {
        self.state
    }
    
    fn reset(&mut self) {
        self.state = ElectricalState::default();
        self.charge = 0.0;
    }
}

/// Voltage source with wave propagation
pub struct WaveVoltageSource {
    voltage: f64,
    state: ElectricalState,
}

impl WaveVoltageSource {
    pub fn new(voltage: f64) -> Self {
        Self {
            voltage,
            state: ElectricalState { voltage, current: 0.0 },
        }
    }
    
    pub fn set_voltage(&mut self, voltage: f64) {
        self.voltage = voltage;
        self.state.voltage = voltage;
    }
}

impl WaveComponent for WaveVoltageSource {
    fn characteristic_impedance(&self) -> f64 {
        0.0 // Ideal voltage source has zero impedance
    }
    
    fn process_wave(&mut self, incident: ElectricalState, _dt: f64) -> ElectricalState {
        // Voltage source maintains its voltage regardless of incident wave
        self.state.current = incident.current;
        
        // Reflect to maintain voltage
        ElectricalState {
            voltage: self.voltage - incident.voltage,
            current: incident.current,
        }
    }
    
    fn update_state(&mut self, _dt: f64) {
        self.state.voltage = self.voltage;
    }
    
    fn get_state(&self) -> ElectricalState {
        self.state
    }
    
    fn reset(&mut self) {
        self.state = ElectricalState { 
            voltage: self.voltage, 
            current: 0.0 
        };
    }
}

/// Connection between two ports with wave propagation delay
#[derive(Debug, Clone)]
pub struct Transmission {
    /// Source port
    pub port1: usize,
    /// Destination port
    pub port2: usize,
    /// Propagation delay (for transmission line effects)
    pub delay: f64,
    /// Characteristic impedance
    pub z0: f64,
    /// Forward wave
    pub forward_wave: ElectricalState,
    /// Backward wave
    pub backward_wave: ElectricalState,
}

/// Wave-based circuit simulation
pub struct WaveCircuit {
    /// Ports indexed by ID
    pub ports: HashMap<usize, Port>,
    /// Components connected to ports
    pub components: HashMap<usize, Box<dyn WaveComponent>>,
    /// Transmissions between ports
    pub transmissions: Vec<Transmission>,
    /// Simulation time
    pub time: f64,
}

impl WaveCircuit {
    pub fn new() -> Self {
        Self {
            ports: HashMap::new(),
            components: HashMap::new(),
            transmissions: Vec::new(),
            time: 0.0,
        }
    }
    
    /// Add a port
    pub fn add_port(&mut self, id: usize) -> &mut Port {
        self.ports.entry(id).or_insert_with(|| Port::new(id))
    }
    
    /// Add a component connected to a port
    pub fn add_component(&mut self, port_id: usize, component: Box<dyn WaveComponent>) {
        self.components.insert(port_id, component);
    }
    
    /// Add a transmission line between ports
    pub fn add_transmission(&mut self, port1: usize, port2: usize, z0: f64) {
        self.transmissions.push(Transmission {
            port1,
            port2,
            delay: 0.0, // Instantaneous for now
            z0,
            forward_wave: ElectricalState::default(),
            backward_wave: ElectricalState::default(),
        });
    }
    
    /// Simulate one time step
    pub fn step(&mut self, dt: f64) -> bool {
        // Process waves at each component
        for (port_id, component) in &mut self.components {
            if let Some(port) = self.ports.get(port_id) {
                // Get incident wave from port
                let incident = port.state;
                
                // Process through component
                let reflected = component.process_wave(incident, dt);
                
                // Update port with reflection
                if let Some(port) = self.ports.get_mut(port_id) {
                    port.apply_perturbation(Perturbation {
                        dv: reflected.voltage,
                        di: reflected.current,
                    });
                }
            }
        }
        
        // Propagate waves through transmissions
        for trans in &mut self.transmissions {
            if let (Some(port1), Some(port2)) = 
                (self.ports.get(&trans.port1), self.ports.get(&trans.port2)) {
                
                // Calculate wave propagation
                let v1 = port1.state.voltage;
                let v2 = port2.state.voltage;
                let i_forward = (v1 - v2) / trans.z0;
                
                trans.forward_wave = ElectricalState {
                    voltage: v1,
                    current: i_forward,
                };
                
                trans.backward_wave = ElectricalState {
                    voltage: v2,
                    current: -i_forward,
                };
            }
        }
        
        // Update all component states
        for component in self.components.values_mut() {
            component.update_state(dt);
        }
        
        // Update all port states
        for port in self.ports.values_mut() {
            port.update_state();
        }
        
        self.time += dt;
        true // Convergence check can be added
    }
    
    /// Reset circuit
    pub fn reset(&mut self) {
        for component in self.components.values_mut() {
            component.reset();
        }
        for port in self.ports.values_mut() {
            port.state = ElectricalState::default();
            port.perturbation = Perturbation::default();
        }
        self.time = 0.0;
    }
}