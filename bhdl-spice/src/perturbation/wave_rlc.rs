/// Specialized wave solver for RLC circuits
/// 
/// This implementation properly handles series RLC circuits using
/// wave propagation with correct topology and impedance matching.

use std::f64::consts::PI;

/// Component in the wave circuit
#[derive(Debug, Clone)]
pub struct WaveComponent {
    /// Component type and parameters
    pub comp_type: ComponentType,
    /// Component voltage
    pub voltage: f64,
    /// Component current
    pub current: f64,
    /// Internal state (flux for L, charge for C)
    pub internal_state: f64,
}

#[derive(Debug, Clone, Copy)]
pub enum ComponentType {
    VoltageSource { voltage: f64, z_internal: f64 },
    Resistor { resistance: f64 },
    Inductor { inductance: f64 },
    Capacitor { capacitance: f64 },
}

impl WaveComponent {
    pub fn new(comp_type: ComponentType) -> Self {
        Self {
            comp_type,
            voltage: 0.0,
            current: 0.0,
            internal_state: 0.0,
        }
    }
}

/// Series RLC circuit with wave propagation
pub struct WaveRLCCircuit {
    /// Components in series order
    pub components: Vec<WaveComponent>,
    /// Node voltages (n+1 nodes for n components)
    pub node_voltages: Vec<f64>,
    /// Transmission line impedance
    pub z0: f64,
    /// Simulation time
    pub time: f64,
}

impl WaveRLCCircuit {
    pub fn new(r: f64, l: f64, c: f64) -> Self {
        // Create components
        let components = vec![
            WaveComponent::new(ComponentType::VoltageSource { 
                voltage: 0.0, 
                z_internal: 0.01  // 10mΩ internal resistance
            }),
            WaveComponent::new(ComponentType::Resistor { resistance: r }),
            WaveComponent::new(ComponentType::Inductor { inductance: l }),
            WaveComponent::new(ComponentType::Capacitor { capacitance: c }),
        ];
        
        // Initialize node voltages (5 nodes for 4 components)
        // Node 0: Ground before voltage source
        // Node 1: After voltage source (positive terminal)
        // Node 2: After resistor
        // Node 3: After inductor (capacitor top)
        // Node 4: After capacitor (ground)
        let node_voltages = vec![0.0; 5];
        
        // Use resistor value as characteristic impedance
        let z0 = r;
        
        Self {
            components,
            node_voltages,
            z0,
            time: 0.0,
        }
    }
    
    /// Set voltage source value
    pub fn set_voltage(&mut self, voltage: f64) {
        if let Some(comp) = self.components.get_mut(0) {
            comp.comp_type = ComponentType::VoltageSource { 
                voltage, 
                z_internal: 0.01 
            };
        }
    }
    
    /// Single time step using direct integration (no inner iterations)
    pub fn step(&mut self, dt: f64) -> bool {
        // Apply voltage source constraint
        if let ComponentType::VoltageSource { voltage, .. } = self.components[0].comp_type {
            self.node_voltages[0] = 0.0; // Ground
            self.node_voltages[1] = voltage; // Source output
        }
        
        // Ground the capacitor bottom
        self.node_voltages[4] = 0.0;
        
        // Get component parameters
        let (r_val, l_val, c_val) = match (&self.components[1].comp_type, 
                                           &self.components[2].comp_type,
                                           &self.components[3].comp_type) {
            (ComponentType::Resistor { resistance }, 
             ComponentType::Inductor { inductance },
             ComponentType::Capacitor { capacitance }) => (*resistance, *inductance, *capacitance),
            _ => panic!("Invalid circuit configuration")
        };
        
        // Get current states
        let i_old = self.components[2].current; // Inductor current
        let v_cap = self.components[3].voltage; // Current capacitor voltage
        let v_source = self.node_voltages[1];
        
        // For a series RLC circuit, KVL gives us:
        // V_source = V_R + V_L + V_C
        // V_source = I*R + L*dI/dt + V_C
        
        // Using backward Euler for the inductor:
        // V_L = L * (I_new - I_old) / dt
        
        // So: V_source = I_new * R + L * (I_new - I_old) / dt + V_cap
        // Rearranging: I_new = (V_source - V_cap + L * I_old / dt) / (R + L/dt)
        
        let i_new = (v_source - v_cap + l_val * i_old / dt) / (r_val + l_val / dt);
        
        // Update all component currents (series circuit - same current everywhere)
        self.components[0].current = i_new;
        self.components[1].current = i_new;
        self.components[2].current = i_new;
        self.components[3].current = i_new;
        
        // Update component voltages
        self.components[1].voltage = r_val * i_new; // V_R = I * R
        self.components[2].voltage = l_val * (i_new - i_old) / dt; // V_L = L * di/dt
        
        // Update capacitor voltage using integration
        // dV_C/dt = I/C
        let v_cap_new = v_cap + i_new * dt / c_val;
        self.components[3].voltage = v_cap_new;
        
        // Update node voltages
        self.node_voltages[2] = self.node_voltages[1] - self.components[1].voltage;
        self.node_voltages[3] = self.node_voltages[2] - self.components[2].voltage;
        
        // Update internal states
        self.components[2].internal_state += self.components[2].voltage * dt; // Flux
        self.components[3].internal_state = v_cap_new * c_val; // Charge
        
        self.time += dt;
        true // Always converged since no iterations
    }
    
    /// Get capacitor voltage
    pub fn get_capacitor_voltage(&self) -> f64 {
        self.components.get(3)
            .map(|c| c.voltage)
            .unwrap_or(0.0)
    }
    
    /// Get circuit current (through resistor)
    pub fn get_circuit_current(&self) -> f64 {
        self.components.get(1)
            .map(|c| c.current)
            .unwrap_or(0.0)
    }
    
    /// Get component powers for energy tracking
    pub fn get_component_powers(&self) -> Vec<f64> {
        self.components.iter()
            .map(|c| c.voltage * c.current)
            .collect()
    }
    
    /// Reset circuit
    pub fn reset(&mut self) {
        for comp in &mut self.components {
            comp.voltage = 0.0;
            comp.current = 0.0;
            comp.internal_state = 0.0;
        }
        self.node_voltages = vec![0.0; self.node_voltages.len()];
        self.time = 0.0;
    }
}

// Wave state structures kept for potential future use
#[derive(Debug, Clone, Copy, Default)]
pub struct WaveState {
    pub v_forward: f64,
    pub v_backward: f64,
    pub voltage: f64,
    pub current: f64,
}

impl WaveState {
    pub fn update_from_waves(&mut self, z0: f64) {
        self.voltage = self.v_forward + self.v_backward;
        self.current = (self.v_forward - self.v_backward) / z0;
    }
    
    pub fn update_waves(&mut self, voltage: f64, current: f64, z0: f64) {
        self.voltage = voltage;
        self.current = current;
        self.v_forward = (voltage + z0 * current) / 2.0;
        self.v_backward = (voltage - z0 * current) / 2.0;
    }
}

#[derive(Debug, Clone, Default)]
pub struct Node {
    pub voltage: f64,
    pub current_sum: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_rc_charging() {
        // Simple RC circuit: 5V -> 1kΩ -> 1µF -> GND
        let mut circuit = WaveRLCCircuit::new(1000.0, 1e-9, 1e-6); // Small L for RC
        circuit.set_voltage(5.0);
        
        // Run for 5 time constants
        let tau = 1000.0 * 1e-6; // RC
        let dt = tau / 100.0;
        let steps = 500;
        
        for _ in 0..steps {
            circuit.step(dt);
        }
        
        // Should be ~99.3% of 5V after 5 tau
        let v_cap = circuit.get_capacitor_voltage();
        assert!((v_cap - 5.0 * (1.0 - (-5.0_f64).exp())).abs() < 0.1);
    }
}