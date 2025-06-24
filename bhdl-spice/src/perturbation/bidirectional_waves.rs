/// Bidirectional wave propagation solver
/// 
/// This implementation models circuits as networks of transmission lines
/// where waves propagate in both directions, similar to waves at a shore.
/// Each component has a characteristic impedance that determines how
/// waves are reflected and transmitted.

use std::collections::HashMap;

/// Wave traveling along a connection
#[derive(Debug, Clone, Copy, Default)]
pub struct Wave {
    /// Voltage wave amplitude
    pub voltage: f64,
    /// Current wave amplitude
    pub current: f64,
}

impl Wave {
    pub fn new(voltage: f64, current: f64) -> Self {
        Self { voltage, current }
    }
    
    /// Power carried by the wave
    pub fn power(&self) -> f64 {
        self.voltage * self.current
    }
    
    /// Characteristic impedance of the wave
    pub fn impedance(&self) -> f64 {
        if self.current.abs() > 1e-12 {
            self.voltage / self.current
        } else {
            1e12 // Very high impedance
        }
    }
}

/// Port where waves enter and leave
#[derive(Debug)]
pub struct WavePort {
    /// Port ID
    pub id: usize,
    /// Incident wave (coming into the port)
    pub incident: Wave,
    /// Reflected wave (going out of the port)
    pub reflected: Wave,
    /// Port voltage (incident + reflected)
    pub voltage: f64,
    /// Port current (incident - reflected for proper sign)
    pub current: f64,
    /// Connected ports
    pub connections: Vec<usize>,
}

impl WavePort {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            incident: Wave::default(),
            reflected: Wave::default(),
            voltage: 0.0,
            current: 0.0,
            connections: Vec::new(),
        }
    }
    
    /// Update port voltage and current from waves
    pub fn update_from_waves(&mut self) {
        self.voltage = self.incident.voltage + self.reflected.voltage;
        self.current = self.incident.current - self.reflected.current;
    }
    
    /// Calculate waves from voltage and current
    pub fn update_waves(&mut self, z0: f64) {
        // Using transmission line theory:
        // V = V+ + V-  (incident + reflected voltage)
        // I = (V+ - V-) / Z0  (incident - reflected current)
        // Solving: V+ = (V + Z0*I) / 2
        //         V- = (V - Z0*I) / 2
        let v_plus = (self.voltage + z0 * self.current) / 2.0;
        let v_minus = (self.voltage - z0 * self.current) / 2.0;
        
        self.incident.voltage = v_plus;
        self.incident.current = v_plus / z0;
        self.reflected.voltage = v_minus;
        self.reflected.current = v_minus / z0;
    }
}

/// Wave-based component model
pub trait WaveModel: Send + Sync {
    /// Get port impedances
    fn port_impedances(&self) -> Vec<f64>;
    
    /// Process incident waves and produce reflected waves
    /// This is where the component's physics is implemented
    fn scatter(&mut self, incident_waves: &[Wave], dt: f64) -> Vec<Wave>;
    
    /// Update internal state
    fn update_state(&mut self, dt: f64);
    
    /// Get current internal state for monitoring
    fn get_state(&self) -> ComponentState;
    
    /// Reset to initial conditions
    fn reset(&mut self);
}

/// Component state for monitoring
#[derive(Debug, Clone, Copy)]
pub struct ComponentState {
    pub voltage: f64,
    pub current: f64,
    pub power: f64,
    pub energy: f64,
}

/// Resistor wave model
pub struct WaveResistor {
    resistance: f64,
    state: ComponentState,
}

impl WaveResistor {
    pub fn new(resistance: f64) -> Self {
        Self {
            resistance,
            state: ComponentState {
                voltage: 0.0,
                current: 0.0,
                power: 0.0,
                energy: 0.0,
            },
        }
    }
}

impl WaveModel for WaveResistor {
    fn port_impedances(&self) -> Vec<f64> {
        vec![self.resistance, self.resistance]
    }
    
    fn scatter(&mut self, incident_waves: &[Wave], _dt: f64) -> Vec<Wave> {
        // For a resistor, scattering matrix relates to impedance matching
        // Reflection coefficient: Γ = (Z - Z0) / (Z + Z0)
        // For matched impedance (Z = Z0), Γ = 0 (no reflection)
        
        let wave1 = incident_waves[0];
        let wave2 = incident_waves[1];
        
        // Total voltage and current through resistor
        self.state.voltage = wave1.voltage - wave2.voltage;
        self.state.current = self.state.voltage / self.resistance;
        self.state.power = self.state.voltage * self.state.current;
        
        // For now, assume matched impedance (no reflections)
        // In reality, would calculate based on port impedances
        vec![
            Wave::new(0.0, 0.0), // No reflection at port 1
            Wave::new(0.0, 0.0), // No reflection at port 2
        ]
    }
    
    fn update_state(&mut self, dt: f64) {
        self.state.energy += self.state.power * dt;
    }
    
    fn get_state(&self) -> ComponentState {
        self.state
    }
    
    fn reset(&mut self) {
        self.state = ComponentState {
            voltage: 0.0,
            current: 0.0,
            power: 0.0,
            energy: 0.0,
        };
    }
}

/// Capacitor wave model
pub struct WaveCapacitor {
    capacitance: f64,
    charge: f64,
    state: ComponentState,
}

impl WaveCapacitor {
    pub fn new(capacitance: f64) -> Self {
        Self {
            capacitance,
            charge: 0.0,
            state: ComponentState {
                voltage: 0.0,
                current: 0.0,
                power: 0.0,
                energy: 0.0,
            },
        }
    }
}

impl WaveModel for WaveCapacitor {
    fn port_impedances(&self) -> Vec<f64> {
        // Capacitor impedance depends on frequency
        // For time-domain, use incremental impedance
        vec![1e6, 1e6] // High impedance at DC
    }
    
    fn scatter(&mut self, incident_waves: &[Wave], dt: f64) -> Vec<Wave> {
        let wave1 = incident_waves[0];
        let wave2 = incident_waves[1];
        
        // Voltage across capacitor
        let v_new = wave1.voltage - wave2.voltage;
        let dv = v_new - self.state.voltage;
        
        // Current through capacitor: I = C * dV/dt
        self.state.current = self.capacitance * dv / dt;
        self.state.voltage = v_new;
        self.state.power = self.state.voltage * self.state.current;
        
        // Reflection based on impedance mismatch
        // Capacitor reflects most of the wave (high impedance)
        let reflection_coeff = 0.9;
        
        vec![
            Wave::new(wave1.voltage * reflection_coeff, -wave1.current * reflection_coeff),
            Wave::new(wave2.voltage * reflection_coeff, -wave2.current * reflection_coeff),
        ]
    }
    
    fn update_state(&mut self, dt: f64) {
        self.charge += self.state.current * dt;
        self.state.voltage = self.charge / self.capacitance;
        self.state.energy = 0.5 * self.capacitance * self.state.voltage * self.state.voltage;
    }
    
    fn get_state(&self) -> ComponentState {
        self.state
    }
    
    fn reset(&mut self) {
        self.charge = 0.0;
        self.state = ComponentState {
            voltage: 0.0,
            current: 0.0,
            power: 0.0,
            energy: 0.0,
        };
    }
}

/// Inductor wave model
pub struct WaveInductor {
    inductance: f64,
    flux: f64,
    state: ComponentState,
}

impl WaveInductor {
    pub fn new(inductance: f64) -> Self {
        Self {
            inductance,
            flux: 0.0,
            state: ComponentState {
                voltage: 0.0,
                current: 0.0,
                power: 0.0,
                energy: 0.0,
            },
        }
    }
}

impl WaveModel for WaveInductor {
    fn port_impedances(&self) -> Vec<f64> {
        // Inductor has high impedance at high frequencies
        vec![1e-3, 1e-3] // Low impedance at DC
    }
    
    fn scatter(&mut self, incident_waves: &[Wave], dt: f64) -> Vec<Wave> {
        let wave1 = incident_waves[0];
        let wave2 = incident_waves[1];
        
        // Voltage across inductor
        self.state.voltage = wave1.voltage - wave2.voltage;
        
        // Current through inductor: V = L * dI/dt => dI = V * dt / L
        let di = self.state.voltage * dt / self.inductance;
        self.state.current += di;
        self.state.power = self.state.voltage * self.state.current;
        
        // Inductor reflects with opposite sign (low impedance)
        let reflection_coeff = -0.9;
        
        vec![
            Wave::new(wave1.voltage * reflection_coeff, -wave1.current * reflection_coeff),
            Wave::new(wave2.voltage * reflection_coeff, -wave2.current * reflection_coeff),
        ]
    }
    
    fn update_state(&mut self, dt: f64) {
        self.flux += self.state.voltage * dt;
        self.state.current = self.flux / self.inductance;
        self.state.energy = 0.5 * self.inductance * self.state.current * self.state.current;
    }
    
    fn get_state(&self) -> ComponentState {
        self.state
    }
    
    fn reset(&mut self) {
        self.flux = 0.0;
        self.state = ComponentState {
            voltage: 0.0,
            current: 0.0,
            power: 0.0,
            energy: 0.0,
        };
    }
}

/// Voltage source wave model
pub struct WaveVoltageSource {
    voltage: f64,
    internal_resistance: f64,
    state: ComponentState,
}

impl WaveVoltageSource {
    pub fn new(voltage: f64) -> Self {
        Self {
            voltage,
            internal_resistance: 0.01, // 10mΩ internal resistance
            state: ComponentState {
                voltage: 0.0,
                current: 0.0,
                power: 0.0,
                energy: 0.0,
            },
        }
    }
    
    pub fn set_voltage(&mut self, voltage: f64) {
        self.voltage = voltage;
    }
}

impl WaveModel for WaveVoltageSource {
    fn port_impedances(&self) -> Vec<f64> {
        vec![self.internal_resistance, self.internal_resistance]
    }
    
    fn scatter(&mut self, incident_waves: &[Wave], _dt: f64) -> Vec<Wave> {
        // Voltage source injects waves to maintain voltage
        let wave_to_inject = Wave::new(self.voltage / 2.0, self.voltage / (2.0 * self.internal_resistance));
        
        // Calculate current drawn
        self.state.current = incident_waves[0].current + incident_waves[1].current;
        self.state.voltage = self.voltage - self.state.current * self.internal_resistance;
        self.state.power = self.state.voltage * self.state.current;
        
        vec![
            wave_to_inject,  // Inject wave at positive terminal
            Wave::new(0.0, 0.0), // Ground terminal
        ]
    }
    
    fn update_state(&mut self, dt: f64) {
        self.state.energy += self.state.power * dt;
    }
    
    fn get_state(&self) -> ComponentState {
        self.state
    }
    
    fn reset(&mut self) {
        self.state = ComponentState {
            voltage: self.voltage,
            current: 0.0,
            power: 0.0,
            energy: 0.0,
        };
    }
}

/// Connection between ports with wave propagation
#[derive(Debug, Clone)]
pub struct WaveGuide {
    /// Source port
    pub port1: usize,
    /// Destination port  
    pub port2: usize,
    /// Characteristic impedance
    pub z0: f64,
    /// Propagation delay (for transmission line effects)
    pub delay: f64,
    /// Forward traveling wave (port1 to port2)
    pub forward_wave: Wave,
    /// Backward traveling wave (port2 to port1)
    pub backward_wave: Wave,
}

impl WaveGuide {
    pub fn new(port1: usize, port2: usize, z0: f64) -> Self {
        Self {
            port1,
            port2,
            z0,
            delay: 0.0, // Instantaneous for now
            forward_wave: Wave::default(),
            backward_wave: Wave::default(),
        }
    }
}

/// Bidirectional wave circuit
pub struct WaveCircuit {
    /// Ports in the circuit
    pub ports: HashMap<usize, WavePort>,
    /// Components (each component connects to multiple ports)
    pub components: Vec<(Box<dyn WaveModel>, Vec<usize>)>,
    /// Wave guides between ports
    pub waveguides: Vec<WaveGuide>,
    /// Simulation time
    pub time: f64,
    /// Convergence tolerance
    pub tolerance: f64,
    /// Voltage sources for easy updates (component index -> voltage)
    pub voltage_sources: HashMap<usize, f64>,
}

impl WaveCircuit {
    pub fn new() -> Self {
        Self {
            ports: HashMap::new(),
            components: Vec::new(),
            waveguides: Vec::new(),
            time: 0.0,
            tolerance: 1e-6,
            voltage_sources: HashMap::new(),
        }
    }
    
    /// Add a port
    pub fn add_port(&mut self, id: usize) -> usize {
        self.ports.insert(id, WavePort::new(id));
        id
    }
    
    /// Add a component with its connected ports
    pub fn add_component(&mut self, component: Box<dyn WaveModel>, ports: Vec<usize>) -> usize {
        // Ensure all ports exist
        for &port_id in &ports {
            self.ports.entry(port_id).or_insert_with(|| WavePort::new(port_id));
        }
        let comp_idx = self.components.len();
        self.components.push((component, ports));
        comp_idx
    }
    
    /// Add a voltage source component
    pub fn add_voltage_source(&mut self, voltage: f64, ports: Vec<usize>) -> usize {
        let comp_idx = self.add_component(Box::new(WaveVoltageSource::new(voltage)), ports);
        self.voltage_sources.insert(comp_idx, voltage);
        comp_idx
    }
    
    /// Update voltage source value
    pub fn set_voltage_source(&mut self, comp_idx: usize, voltage: f64) {
        if let Some(&old_voltage) = self.voltage_sources.get(&comp_idx) {
            self.voltage_sources.insert(comp_idx, voltage);
            // This will be applied in the next step
        }
    }
    
    /// Connect two ports with a waveguide
    pub fn connect_ports(&mut self, port1: usize, port2: usize, z0: f64) {
        self.waveguides.push(WaveGuide::new(port1, port2, z0));
        
        // Update port connections
        if let Some(p1) = self.ports.get_mut(&port1) {
            p1.connections.push(port2);
        }
        if let Some(p2) = self.ports.get_mut(&port2) {
            p2.connections.push(port1);
        }
    }
    
    /// Single time step with proper bidirectional wave propagation
    pub fn step(&mut self, dt: f64) -> bool {
        // First, update any voltage sources
        for (&comp_idx, &voltage) in &self.voltage_sources {
            if let Some((component, _)) = self.components.get_mut(comp_idx) {
                // This is a bit hacky but works for our PoC
                // In production, we'd use trait object downcasting
                // For now, recreate the voltage source
                if comp_idx < self.components.len() {
                    let ports = self.components[comp_idx].1.clone();
                    self.components[comp_idx] = (Box::new(WaveVoltageSource::new(voltage)), ports);
                }
            }
        }
        
        let max_iterations = 50;
        let mut converged = false;
        
        for iteration in 0..max_iterations {
            let mut max_change: f64 = 0.0;
            
            // Step 1: Clear all incident waves at ports
            for port in self.ports.values_mut() {
                port.incident = Wave::default();
            }
            
            // Step 2: Propagate waves through waveguides bidirectionally
            for guide in &mut self.waveguides {
                if let (Some(port1), Some(port2)) = 
                    (self.ports.get(&guide.port1), self.ports.get(&guide.port2)) {
                    
                    // Calculate transmission line waves
                    // V1+ = (V1 + Z0*I1) / 2, V1- = (V1 - Z0*I1) / 2
                    let v1_plus = (port1.voltage + guide.z0 * port1.current) / 2.0;
                    let v1_minus = (port1.voltage - guide.z0 * port1.current) / 2.0;
                    
                    let v2_plus = (port2.voltage + guide.z0 * port2.current) / 2.0;
                    let v2_minus = (port2.voltage - guide.z0 * port2.current) / 2.0;
                    
                    // Forward wave from port1 to port2
                    guide.forward_wave = Wave::new(v1_plus, v1_plus / guide.z0);
                    
                    // Backward wave from port2 to port1
                    guide.backward_wave = Wave::new(v2_plus, v2_plus / guide.z0);
                }
            }
            
            // Step 3: Sum incident waves at each port (superposition)
            for guide in &self.waveguides {
                // Add forward wave to port2's incident
                if let Some(port2) = self.ports.get_mut(&guide.port2) {
                    port2.incident.voltage += guide.forward_wave.voltage;
                    port2.incident.current += guide.forward_wave.current;
                }
                
                // Add backward wave to port1's incident
                if let Some(port1) = self.ports.get_mut(&guide.port1) {
                    port1.incident.voltage += guide.backward_wave.voltage;
                    port1.incident.current += guide.backward_wave.current;
                }
            }
            
            // Step 4: Process components (scattering)
            for (comp_idx, (component, port_ids)) in self.components.iter_mut().enumerate() {
                // Gather incident waves at component ports
                let incident_waves: Vec<Wave> = port_ids.iter()
                    .map(|&id| self.ports.get(&id).map(|p| p.incident).unwrap_or_default())
                    .collect();
                
                // Calculate reflected waves based on component physics
                let reflected_waves = component.scatter(&incident_waves, dt);
                
                // Update port states
                for (i, &port_id) in port_ids.iter().enumerate() {
                    if let Some(port) = self.ports.get_mut(&port_id) {
                        let old_voltage = port.voltage;
                        
                        // Set reflected wave
                        port.reflected = reflected_waves.get(i).copied().unwrap_or_default();
                        
                        // Update port voltage and current from waves
                        port.update_from_waves();
                        
                        // Track convergence
                        let change = (port.voltage - old_voltage).abs();
                        max_change = max_change.max(change);
                    }
                }
            }
            
            // Check convergence
            if max_change < self.tolerance {
                converged = true;
                break;
            }
        }
        
        // Update component states after convergence
        for (component, _) in &mut self.components {
            component.update_state(dt);
        }
        
        self.time += dt;
        converged
    }
    
    /// Get voltage at a port
    pub fn get_port_voltage(&self, port_id: usize) -> f64 {
        self.ports.get(&port_id).map(|p| p.voltage).unwrap_or(0.0)
    }
    
    /// Get component state
    pub fn get_component_state(&self, comp_idx: usize) -> Option<ComponentState> {
        self.components.get(comp_idx).map(|(c, _)| c.get_state())
    }
    
    /// Reset circuit
    pub fn reset(&mut self) {
        for port in self.ports.values_mut() {
            port.incident = Wave::default();
            port.reflected = Wave::default();
            port.voltage = 0.0;
            port.current = 0.0;
        }
        for (component, _) in &mut self.components {
            component.reset();
        }
        self.time = 0.0;
    }
}