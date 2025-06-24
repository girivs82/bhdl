/// Comprehensive Semiconductor Device Test
/// 
/// Tests hybrid (80%) vs smart damping on circuits with different semiconductor types:
/// 1. BJT amplifier circuits (npn/pnp)
/// 2. MOSFET switching circuits (nmos/pmos) 
/// 3. OPAMP-based circuits with feedback
/// 4. Mixed semiconductor circuits
/// 
/// Goal: Validate whether 80% approach is universal across all semiconductor types

use nalgebra::{DMatrix, DVector};
use std::time::Instant;

// Enhanced element types for comprehensive testing
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ElementType {
    Resistor,
    VoltageSource,
    CurrentSource,
    Diode,
    BJT,      // Bipolar Junction Transistor
    MOSFET,   // Metal-Oxide-Semiconductor FET
    OPAMP,    // Operational Amplifier
}

pub trait Element: Send + Sync {
    fn element_type(&self) -> ElementType;
    fn conductance(&self) -> f64 { 0.0 }
    fn is_nonlinear(&self) -> bool { false }
    fn current_at_voltage(&self, v: f64) -> f64;
    fn conductance_at_voltage(&self, v: f64) -> f64;
    fn get_voltage(&self) -> f64;
    fn set_voltage(&mut self, v: f64);
    fn get_terminals(&self) -> usize { 2 } // Default 2-terminal
}

// Basic passive components
pub struct Resistor {
    resistance: f64,
    voltage: f64,
}

impl Resistor {
    pub fn new(r: f64) -> Self {
        Self { resistance: r, voltage: 0.0 }
    }
}

impl Element for Resistor {
    fn element_type(&self) -> ElementType { ElementType::Resistor }
    fn conductance(&self) -> f64 { 1.0 / self.resistance }
    fn current_at_voltage(&self, v: f64) -> f64 { v / self.resistance }
    fn conductance_at_voltage(&self, _v: f64) -> f64 { 1.0 / self.resistance }
    fn get_voltage(&self) -> f64 { self.voltage }
    fn set_voltage(&mut self, v: f64) { self.voltage = v; }
}

pub struct VoltageSource {
    voltage: f64,
}

impl VoltageSource {
    pub fn new(v: f64) -> Self {
        Self { voltage: v }
    }
}

impl Element for VoltageSource {
    fn element_type(&self) -> ElementType { ElementType::VoltageSource }
    fn current_at_voltage(&self, _v: f64) -> f64 { 0.0 }
    fn conductance_at_voltage(&self, _v: f64) -> f64 { 0.0 }
    fn get_voltage(&self) -> f64 { self.voltage }
    fn set_voltage(&mut self, v: f64) { self.voltage = v; }
}

// Diode (for comparison)
pub struct Diode {
    is: f64,
    vt: f64,
    voltage: f64,
}

impl Diode {
    pub fn new(is: f64, vt: f64) -> Self {
        Self { is, vt, voltage: 0.0 }
    }
}

impl Element for Diode {
    fn element_type(&self) -> ElementType { ElementType::Diode }
    fn is_nonlinear(&self) -> bool { true }
    
    fn current_at_voltage(&self, v: f64) -> f64 {
        const MAX_EXP: f64 = 50.0;
        let v_norm = v / self.vt;
        
        if v_norm > MAX_EXP {
            let i_max = self.is * (MAX_EXP.exp() - 1.0);
            let g_max = (self.is / self.vt) * MAX_EXP.exp();
            i_max + g_max * (v - MAX_EXP * self.vt)
        } else if v_norm < -5.0 {
            -self.is
        } else {
            self.is * (v_norm.exp() - 1.0)
        }
    }
    
    fn conductance_at_voltage(&self, v: f64) -> f64 {
        const MAX_EXP: f64 = 50.0;
        const MIN_G: f64 = 1e-14;
        let v_norm = v / self.vt;
        
        if v_norm > MAX_EXP {
            (self.is / self.vt) * MAX_EXP.exp()
        } else if v_norm < -5.0 {
            MIN_G
        } else {
            ((self.is / self.vt) * v_norm.exp()).max(MIN_G)
        }
    }
    
    fn get_voltage(&self) -> f64 { self.voltage }
    fn set_voltage(&mut self, v: f64) { self.voltage = v; }
}

// BJT (Bipolar Junction Transistor) - Simplified Ebers-Moll model
pub struct BJT {
    is: f64,        // Saturation current
    beta: f64,      // Current gain (β)
    vt: f64,        // Thermal voltage
    bjt_type: BJTType,
    vbe: f64,       // Base-emitter voltage
    vbc: f64,       // Base-collector voltage  
}

#[derive(Clone, Copy)]
pub enum BJTType {
    NPN,
    PNP,
}

impl BJT {
    pub fn new(is: f64, beta: f64, vt: f64, bjt_type: BJTType) -> Self {
        Self { is, beta, vt, bjt_type, vbe: 0.0, vbc: 0.0 }
    }
    
    // Simplified BJT model - collector current
    fn collector_current(&self, vbe: f64, vbc: f64) -> f64 {
        let polarity = match self.bjt_type {
            BJTType::NPN => 1.0,
            BJTType::PNP => -1.0,
        };
        
        // Forward current (B-E junction)
        let if_curr = self.is * ((polarity * vbe / self.vt).exp() - 1.0);
        
        // Reverse current (B-C junction) 
        let ir_curr = self.is * ((polarity * vbc / self.vt).exp() - 1.0);
        
        // Collector current (simplified)
        polarity * (if_curr - ir_curr / (self.beta + 1.0))
    }
    
    // Base current
    fn base_current(&self, vbe: f64, vbc: f64) -> f64 {
        let ic = self.collector_current(vbe, vbc);
        let polarity = match self.bjt_type {
            BJTType::NPN => 1.0,
            BJTType::PNP => -1.0,
        };
        
        polarity * ic / self.beta
    }
}

impl Element for BJT {
    fn element_type(&self) -> ElementType { ElementType::BJT }
    fn is_nonlinear(&self) -> bool { true }
    fn get_terminals(&self) -> usize { 3 } // 3-terminal device
    
    // For 2-terminal interface, use B-E junction
    fn current_at_voltage(&self, v: f64) -> f64 {
        let vbe = v;
        let vbc = 0.0; // Assume C-B not reverse biased
        self.base_current(vbe, vbc)
    }
    
    fn conductance_at_voltage(&self, v: f64) -> f64 {
        let vbe = v;
        let polarity = match self.bjt_type {
            BJTType::NPN => 1.0,
            BJTType::PNP => -1.0,
        };
        
        // Transconductance
        let gm = (polarity * self.is / self.vt) * (polarity * vbe / self.vt).exp() / self.beta;
        gm.max(1e-14)
    }
    
    fn get_voltage(&self) -> f64 { self.vbe }
    fn set_voltage(&mut self, v: f64) { self.vbe = v; }
}

// MOSFET - Simplified square-law model
pub struct MOSFET {
    kp: f64,        // Process transconductance parameter
    vth: f64,       // Threshold voltage  
    lambda: f64,    // Channel length modulation
    mosfet_type: MOSFETType,
    vgs: f64,       // Gate-source voltage
    vds: f64,       // Drain-source voltage
}

#[derive(Clone, Copy)]
pub enum MOSFETType {
    NMOS,
    PMOS,
}

impl MOSFET {
    pub fn new(kp: f64, vth: f64, lambda: f64, mosfet_type: MOSFETType) -> Self {
        Self { kp, vth, lambda, mosfet_type, vgs: 0.0, vds: 0.0 }
    }
    
    // MOSFET drain current
    fn drain_current(&self, vgs: f64, vds: f64) -> f64 {
        let polarity = match self.mosfet_type {
            MOSFETType::NMOS => 1.0,
            MOSFETType::PMOS => -1.0,
        };
        
        let vgs_eff = polarity * vgs;
        let vds_eff = polarity * vds;
        let vth_eff = polarity * self.vth;
        
        if vgs_eff <= vth_eff {
            // Cutoff region
            0.0
        } else if vds_eff <= (vgs_eff - vth_eff) {
            // Linear region
            polarity * self.kp * ((vgs_eff - vth_eff) * vds_eff - 0.5 * vds_eff * vds_eff) * (1.0 + self.lambda * vds_eff)
        } else {
            // Saturation region
            polarity * 0.5 * self.kp * (vgs_eff - vth_eff).powi(2) * (1.0 + self.lambda * vds_eff)
        }
    }
}

impl Element for MOSFET {
    fn element_type(&self) -> ElementType { ElementType::MOSFET }
    fn is_nonlinear(&self) -> bool { true }
    fn get_terminals(&self) -> usize { 3 } // 3-terminal device
    
    // For 2-terminal interface, use G-S junction
    fn current_at_voltage(&self, v: f64) -> f64 {
        let vgs = v;
        let vds = 1.0; // Assume some drain voltage
        self.drain_current(vgs, vds)
    }
    
    fn conductance_at_voltage(&self, v: f64) -> f64 {
        let vgs = v;
        let polarity = match self.mosfet_type {
            MOSFETType::NMOS => 1.0,
            MOSFETType::PMOS => -1.0,
        };
        
        let vgs_eff = polarity * vgs;
        let vth_eff = polarity * self.vth;
        
        if vgs_eff <= vth_eff {
            1e-14 // Very small conductance in cutoff
        } else {
            // Transconductance in active region
            let gm = polarity * self.kp * (vgs_eff - vth_eff);
            gm.max(1e-14)
        }
    }
    
    fn get_voltage(&self) -> f64 { self.vgs }
    fn set_voltage(&mut self, v: f64) { self.vgs = v; }
}

// OPAMP - Simplified high-gain model
pub struct OPAMP {
    gain: f64,      // Open-loop gain
    vsat_pos: f64,  // Positive saturation voltage
    vsat_neg: f64,  // Negative saturation voltage
    vin_diff: f64,  // Differential input voltage
    vout: f64,      // Output voltage
}

impl OPAMP {
    pub fn new(gain: f64, vsat_pos: f64, vsat_neg: f64) -> Self {
        Self { gain, vsat_pos, vsat_neg, vin_diff: 0.0, vout: 0.0 }
    }
    
    // OPAMP output voltage
    fn output_voltage(&self, vin_diff: f64) -> f64 {
        let ideal_out = self.gain * vin_diff;
        
        // Saturation limits
        if ideal_out > self.vsat_pos {
            self.vsat_pos
        } else if ideal_out < self.vsat_neg {
            self.vsat_neg
        } else {
            ideal_out
        }
    }
}

impl Element for OPAMP {
    fn element_type(&self) -> ElementType { ElementType::OPAMP }
    fn is_nonlinear(&self) -> bool { true }
    fn get_terminals(&self) -> usize { 3 } // Simplified 3-terminal (differential input + output)
    
    // For 2-terminal interface, use differential input
    fn current_at_voltage(&self, v: f64) -> f64 {
        // OPAMP has very high input impedance, minimal current
        let vin_diff = v;
        vin_diff * 1e-12 // Very small input current
    }
    
    fn conductance_at_voltage(&self, _v: f64) -> f64 {
        1e-12 // Very high input impedance
    }
    
    fn get_voltage(&self) -> f64 { self.vin_diff }
    fn set_voltage(&mut self, v: f64) { self.vin_diff = v; }
}

// Hybrid solver (reusing from previous test)
pub struct HybridLogGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
}

impl HybridLogGradientSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        self.elements.push(element);
        self.elements.len() - 1
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    pub fn solve(&mut self) -> (Vec<f64>, f64, usize, f64) {
        let start = Instant::now();
        
        // Setup
        let mut vsource_count = 0;
        for elem in &self.elements {
            if elem.element_type() == ElementType::VoltageSource {
                vsource_count += 1;
            }
        }
        self.source_currents = vec![0.0; vsource_count];
        
        let mut total_iterations = 0;
        
        // Find voltage sources
        let mut vsources = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.element_type() == ElementType::VoltageSource {
                vsources.push((i, elem.get_voltage()));
            }
        }
        
        // Phase 1: Fast ramping (0-80%)
        let mut ramp = 0.0;
        let phase1_end = 0.8;
        let phase1_step = 0.05;
        
        while ramp < phase1_end {
            ramp = f64::min(ramp + phase1_step, phase1_end);
            
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp);
            }
            
            self.solve_at_ramp_fast(&mut total_iterations);
        }
        
        // Phase 2: Accurate convergence (80-100%)
        let phase2_step = 0.01;
        
        while ramp < 0.999 {
            ramp = f64::min(ramp + phase2_step, 1.0);
            
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp);
            }
            
            self.solve_at_ramp_accurate(&mut total_iterations);
        }
        
        // Get nonlinear device voltages
        let mut device_voltages = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.is_nonlinear() {
                for &(idx, pos, neg) in &self.connections {
                    if idx == i {
                        device_voltages.push(self.node_voltages[pos] - self.node_voltages[neg]);
                    }
                }
            }
        }
        
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        (device_voltages, self.source_currents.get(0).copied().unwrap_or(0.0).abs(), total_iterations, elapsed)
    }
    
    fn solve_at_ramp_fast(&mut self, total_iterations: &mut usize) {
        let max_iter = 30;
        let tol = 1e-8;
        let damping = 0.5;
        
        for _ in 0..max_iter {
            *total_iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                for i in 0..n {
                    let delta = x[i] - old_v[i+1];
                    self.node_voltages[i+1] = old_v[i+1] + damping * delta;
                    max_change = max_change.max(delta.abs());
                }
                
                for i in 0..self.source_currents.len() {
                    self.source_currents[i] = x[n + i];
                }
                
                for &(elem_idx, pos, neg) in &self.connections {
                    let v = self.node_voltages[pos] - self.node_voltages[neg];
                    self.elements[elem_idx].set_voltage(v);
                }
                
                if max_change < tol {
                    return;
                }
            } else {
                return;
            }
        }
    }
    
    fn solve_at_ramp_accurate(&mut self, total_iterations: &mut usize) {
        let max_iter = 100;
        let tol = 1e-12;
        let damping = 0.7;
        
        for _ in 0..max_iter {
            *total_iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                for i in 0..n {
                    let delta = x[i] - old_v[i+1];
                    self.node_voltages[i+1] = old_v[i+1] + damping * delta;
                    max_change = max_change.max(delta.abs());
                }
                
                for i in 0..self.source_currents.len() {
                    self.source_currents[i] = x[n + i];
                }
                
                for &(elem_idx, pos, neg) in &self.connections {
                    let v = self.node_voltages[pos] - self.node_voltages[neg];
                    self.elements[elem_idx].set_voltage(v);
                }
                
                if max_change < tol {
                    return;
                }
            } else {
                return;
            }
        }
    }
    
    fn build_mna_system(&self) -> (DMatrix<f64>, DVector<f64>) {
        let n = self.num_nodes - 1;
        let m = self.source_currents.len();
        
        let size = n + m;
        let mut a = DMatrix::zeros(size, size);
        let mut b = DVector::zeros(size);
        
        let gmin = 1e-12;
        for i in 0..n {
            a[(i, i)] = gmin;
        }
        
        let mut vs_idx = 0;
        
        for &(elem_idx, pos, neg) in &self.connections {
            let elem = &self.elements[elem_idx];
            
            match elem.element_type() {
                ElementType::VoltageSource => {
                    let row = n + vs_idx;
                    
                    if pos > 0 {
                        a[(pos-1, row)] = 1.0;
                        a[(row, pos-1)] = 1.0;
                    }
                    if neg > 0 {
                        a[(neg-1, row)] = -1.0;
                        a[(row, neg-1)] = -1.0;
                    }
                    
                    b[row] = elem.get_voltage();
                    vs_idx += 1;
                }
                _ => {
                    let v_elem = self.node_voltages[pos] - self.node_voltages[neg];
                    let g = elem.conductance_at_voltage(v_elem);
                    let i_elem = elem.current_at_voltage(v_elem);
                    let i_norton = i_elem - g * v_elem;
                    
                    if pos > 0 {
                        a[(pos-1, pos-1)] += g;
                        b[pos-1] -= i_norton;
                    }
                    if neg > 0 {
                        a[(neg-1, neg-1)] += g;
                        b[neg-1] += i_norton;
                    }
                    if pos > 0 && neg > 0 {
                        a[(pos-1, neg-1)] -= g;
                        a[(neg-1, pos-1)] -= g;
                    }
                }
            }
        }
        
        (a, b)
    }
}

// Smart damping solver (simplified version)
pub struct SmartDampingSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    damping_factor: f64,
    adaptive_step: f64,
}

impl SmartDampingSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            damping_factor: 0.3,
            adaptive_step: 0.01,
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        self.elements.push(element);
        self.elements.len() - 1
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    pub fn solve(&mut self) -> (Vec<f64>, f64, usize, f64) {
        let start = Instant::now();
        
        // Setup
        let mut vsource_count = 0;
        for elem in &self.elements {
            if elem.element_type() == ElementType::VoltageSource {
                vsource_count += 1;
            }
        }
        self.source_currents = vec![0.0; vsource_count];
        
        let mut total_iterations = 0;
        
        // Find voltage sources
        let mut vsources = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.element_type() == ElementType::VoltageSource {
                vsources.push((i, elem.get_voltage()));
            }
        }
        
        // Smart damping ramp
        let mut ramp = 0.0;
        
        while ramp < 0.999 {
            ramp = f64::min(ramp + self.adaptive_step, 1.0);
            
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp);
            }
            
            self.solve_with_smart_damping(&mut total_iterations);
        }
        
        // Get nonlinear device voltages
        let mut device_voltages = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.is_nonlinear() {
                for &(idx, pos, neg) in &self.connections {
                    if idx == i {
                        device_voltages.push(self.node_voltages[pos] - self.node_voltages[neg]);
                    }
                }
            }
        }
        
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        (device_voltages, self.source_currents.get(0).copied().unwrap_or(0.0).abs(), total_iterations, elapsed)
    }
    
    fn solve_with_smart_damping(&mut self, total_iterations: &mut usize) {
        let max_iter = 100;
        let tol = 1e-12;
        
        for _ in 0..max_iter {
            *total_iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                for i in 0..n {
                    let delta = x[i] - old_v[i+1];
                    self.node_voltages[i+1] = old_v[i+1] + self.damping_factor * delta;
                    max_change = max_change.max(delta.abs());
                }
                
                for i in 0..self.source_currents.len() {
                    self.source_currents[i] = x[n + i];
                }
                
                for &(elem_idx, pos, neg) in &self.connections {
                    let v = self.node_voltages[pos] - self.node_voltages[neg];
                    self.elements[elem_idx].set_voltage(v);
                }
                
                if max_change < tol {
                    return;
                }
            } else {
                return;
            }
        }
    }
    
    fn build_mna_system(&self) -> (DMatrix<f64>, DVector<f64>) {
        let n = self.num_nodes - 1;
        let m = self.source_currents.len();
        
        let size = n + m;
        let mut a = DMatrix::zeros(size, size);
        let mut b = DVector::zeros(size);
        
        let gmin = 1e-12;
        for i in 0..n {
            a[(i, i)] = gmin;
        }
        
        let mut vs_idx = 0;
        
        for &(elem_idx, pos, neg) in &self.connections {
            let elem = &self.elements[elem_idx];
            
            match elem.element_type() {
                ElementType::VoltageSource => {
                    let row = n + vs_idx;
                    
                    if pos > 0 {
                        a[(pos-1, row)] = 1.0;
                        a[(row, pos-1)] = 1.0;
                    }
                    if neg > 0 {
                        a[(neg-1, row)] = -1.0;
                        a[(row, neg-1)] = -1.0;
                    }
                    
                    b[row] = elem.get_voltage();
                    vs_idx += 1;
                }
                _ => {
                    let v_elem = self.node_voltages[pos] - self.node_voltages[neg];
                    let g = elem.conductance_at_voltage(v_elem);
                    let i_elem = elem.current_at_voltage(v_elem);
                    let i_norton = i_elem - g * v_elem;
                    
                    if pos > 0 {
                        a[(pos-1, pos-1)] += g;
                        b[pos-1] -= i_norton;
                    }
                    if neg > 0 {
                        a[(neg-1, neg-1)] += g;
                        b[neg-1] += i_norton;
                    }
                    if pos > 0 && neg > 0 {
                        a[(pos-1, neg-1)] -= g;
                        a[(neg-1, pos-1)] -= g;
                    }
                }
            }
        }
        
        (a, b)
    }
}

fn main() {
    println!("=== COMPREHENSIVE SEMICONDUCTOR DEVICE TEST ===");
    println!("Testing hybrid (80%) vs smart damping on all semiconductor types\n");
    
    // Test Case 1: BJT Amplifier
    println!("🔧 TEST 1: BJT AMPLIFIER CIRCUIT");
    println!("   Circuit: NPN BJT common-emitter amplifier");
    test_bjt_amplifier();
    
    // Test Case 2: MOSFET Switching
    println!("\n🔧 TEST 2: MOSFET SWITCHING CIRCUIT");
    println!("   Circuit: NMOS switching with load");
    test_mosfet_switching();
    
    // Test Case 3: OPAMP Buffer
    println!("\n🔧 TEST 3: OPAMP BUFFER CIRCUIT");
    println!("   Circuit: OPAMP voltage follower with feedback");
    test_opamp_buffer();
    
    // Test Case 4: Mixed Semiconductor Circuit
    println!("\n🔧 TEST 4: MIXED SEMICONDUCTOR CIRCUIT");
    println!("   Circuit: BJT + MOSFET + Diode combination");
    test_mixed_semiconductors();
    
    // Test Case 5: All Device Types
    println!("\n🔧 TEST 5: ALL DEVICE TYPES CIRCUIT");
    println!("   Circuit: BJT + MOSFET + OPAMP + Diode");
    test_all_device_types();
}

fn test_bjt_amplifier() {
    println!("  Testing NPN BJT common-emitter amplifier...");
    
    // Create hybrid solver
    let mut hybrid = HybridLogGradientSolver::new(5);
    
    let vcc = hybrid.add_element(Box::new(VoltageSource::new(12.0)));
    let vin = hybrid.add_element(Box::new(VoltageSource::new(0.7))); // Base voltage
    let rc = hybrid.add_element(Box::new(Resistor::new(2000.0))); // Collector resistor
    let rb = hybrid.add_element(Box::new(Resistor::new(100000.0))); // Base resistor
    let re = hybrid.add_element(Box::new(Resistor::new(1000.0))); // Emitter resistor
    let bjt = hybrid.add_element(Box::new(BJT::new(1e-14, 100.0, 0.026, BJTType::NPN)));
    
    // Connect BJT amplifier
    hybrid.connect(vcc, 1, 0);    // VCC
    hybrid.connect(vin, 2, 0);    // Input voltage
    hybrid.connect(rb, 2, 3);     // Base resistor
    hybrid.connect(rc, 1, 4);     // Collector resistor  
    hybrid.connect(re, 3, 0);     // Emitter resistor
    hybrid.connect(bjt, 3, 0);    // BJT (base-emitter)
    
    let (device_voltages_hybrid, _current_hybrid, iters_hybrid, time_hybrid) = hybrid.solve();
    
    // Create smart damping solver
    let mut smart = SmartDampingSolver::new(5);
    
    let vcc = smart.add_element(Box::new(VoltageSource::new(12.0)));
    let vin = smart.add_element(Box::new(VoltageSource::new(0.7)));
    let rc = smart.add_element(Box::new(Resistor::new(2000.0)));
    let rb = smart.add_element(Box::new(Resistor::new(100000.0)));
    let re = smart.add_element(Box::new(Resistor::new(1000.0)));
    let bjt = smart.add_element(Box::new(BJT::new(1e-14, 100.0, 0.026, BJTType::NPN)));
    
    smart.connect(vcc, 1, 0);
    smart.connect(vin, 2, 0);
    smart.connect(rb, 2, 3);
    smart.connect(rc, 1, 4);
    smart.connect(re, 3, 0);
    smart.connect(bjt, 3, 0);
    
    let (device_voltages_smart, _current_smart, iters_smart, time_smart) = smart.solve();
    
    println!("    Hybrid (80%):   {} devices, {:.1}ms, {} iters", 
             device_voltages_hybrid.len(), time_hybrid, iters_hybrid);
    println!("    Smart Damping:  {} devices, {:.1}ms, {} iters", 
             device_voltages_smart.len(), time_smart, iters_smart);
    
    if time_smart > 0.0 {
        println!("    Speed ratio: {:.1}x", time_smart / time_hybrid);
    }
    
    // Show BJT voltages
    if !device_voltages_hybrid.is_empty() && !device_voltages_smart.is_empty() {
        println!("    BJT VBE (Hybrid): {:.3}V", device_voltages_hybrid[0]);
        println!("    BJT VBE (Smart):  {:.3}V", device_voltages_smart[0]);
    }
}

fn test_mosfet_switching() {
    println!("  Testing NMOS switching circuit...");
    
    // Create hybrid solver
    let mut hybrid = HybridLogGradientSolver::new(5);
    
    let vdd = hybrid.add_element(Box::new(VoltageSource::new(5.0)));
    let vgate = hybrid.add_element(Box::new(VoltageSource::new(3.3))); // Gate voltage
    let rload = hybrid.add_element(Box::new(Resistor::new(1000.0))); // Load resistor
    let rgate = hybrid.add_element(Box::new(Resistor::new(10000.0))); // Gate resistor
    let mosfet = hybrid.add_element(Box::new(MOSFET::new(100e-6, 1.0, 0.01, MOSFETType::NMOS)));
    
    // Connect MOSFET switching circuit
    hybrid.connect(vdd, 1, 0);      // VDD
    hybrid.connect(vgate, 2, 0);    // Gate voltage
    hybrid.connect(rgate, 2, 3);    // Gate resistor
    hybrid.connect(rload, 1, 4);    // Load resistor
    hybrid.connect(mosfet, 3, 0);   // MOSFET (gate-source)
    
    let (device_voltages_hybrid, _current_hybrid, iters_hybrid, time_hybrid) = hybrid.solve();
    
    // Create smart damping solver
    let mut smart = SmartDampingSolver::new(5);
    
    let vdd = smart.add_element(Box::new(VoltageSource::new(5.0)));
    let vgate = smart.add_element(Box::new(VoltageSource::new(3.3)));
    let rload = smart.add_element(Box::new(Resistor::new(1000.0)));
    let rgate = smart.add_element(Box::new(Resistor::new(10000.0)));
    let mosfet = smart.add_element(Box::new(MOSFET::new(100e-6, 1.0, 0.01, MOSFETType::NMOS)));
    
    smart.connect(vdd, 1, 0);
    smart.connect(vgate, 2, 0);
    smart.connect(rgate, 2, 3);
    smart.connect(rload, 1, 4);
    smart.connect(mosfet, 3, 0);
    
    let (device_voltages_smart, _current_smart, iters_smart, time_smart) = smart.solve();
    
    println!("    Hybrid (80%):   {} devices, {:.1}ms, {} iters", 
             device_voltages_hybrid.len(), time_hybrid, iters_hybrid);
    println!("    Smart Damping:  {} devices, {:.1}ms, {} iters", 
             device_voltages_smart.len(), time_smart, iters_smart);
    
    if time_smart > 0.0 {
        println!("    Speed ratio: {:.1}x", time_smart / time_hybrid);
    }
    
    // Show MOSFET voltages
    if !device_voltages_hybrid.is_empty() && !device_voltages_smart.is_empty() {
        println!("    MOSFET VGS (Hybrid): {:.3}V", device_voltages_hybrid[0]);
        println!("    MOSFET VGS (Smart):  {:.3}V", device_voltages_smart[0]);
    }
}

fn test_opamp_buffer() {
    println!("  Testing OPAMP voltage follower...");
    
    // Create hybrid solver
    let mut hybrid = HybridLogGradientSolver::new(5);
    
    let vin = hybrid.add_element(Box::new(VoltageSource::new(2.5)));
    let vcc = hybrid.add_element(Box::new(VoltageSource::new(5.0)));
    let rfb = hybrid.add_element(Box::new(Resistor::new(1000.0))); // Feedback resistor
    let rload = hybrid.add_element(Box::new(Resistor::new(10000.0))); // Load resistor
    let opamp = hybrid.add_element(Box::new(OPAMP::new(100000.0, 4.8, 0.2)));
    
    // Connect OPAMP buffer
    hybrid.connect(vin, 1, 0);      // Input voltage
    hybrid.connect(vcc, 2, 0);      // Supply voltage
    hybrid.connect(rfb, 3, 1);      // Feedback resistor (output to input)
    hybrid.connect(rload, 3, 0);    // Load resistor
    hybrid.connect(opamp, 1, 0);    // OPAMP (differential input)
    
    let (device_voltages_hybrid, _current_hybrid, iters_hybrid, time_hybrid) = hybrid.solve();
    
    // Create smart damping solver
    let mut smart = SmartDampingSolver::new(5);
    
    let vin = smart.add_element(Box::new(VoltageSource::new(2.5)));
    let vcc = smart.add_element(Box::new(VoltageSource::new(5.0)));
    let rfb = smart.add_element(Box::new(Resistor::new(1000.0)));
    let rload = smart.add_element(Box::new(Resistor::new(10000.0)));
    let opamp = smart.add_element(Box::new(OPAMP::new(100000.0, 4.8, 0.2)));
    
    smart.connect(vin, 1, 0);
    smart.connect(vcc, 2, 0);
    smart.connect(rfb, 3, 1);
    smart.connect(rload, 3, 0);
    smart.connect(opamp, 1, 0);
    
    let (device_voltages_smart, _current_smart, iters_smart, time_smart) = smart.solve();
    
    println!("    Hybrid (80%):   {} devices, {:.1}ms, {} iters", 
             device_voltages_hybrid.len(), time_hybrid, iters_hybrid);
    println!("    Smart Damping:  {} devices, {:.1}ms, {} iters", 
             device_voltages_smart.len(), time_smart, iters_smart);
    
    if time_smart > 0.0 {
        println!("    Speed ratio: {:.1}x", time_smart / time_hybrid);
    }
    
    // Show OPAMP voltages
    if !device_voltages_hybrid.is_empty() && !device_voltages_smart.is_empty() {
        println!("    OPAMP Vin (Hybrid): {:.3}V", device_voltages_hybrid[0]);
        println!("    OPAMP Vin (Smart):  {:.3}V", device_voltages_smart[0]);
    }
}

fn test_mixed_semiconductors() {
    println!("  Testing mixed BJT + MOSFET + Diode circuit...");
    
    // Create hybrid solver
    let mut hybrid = HybridLogGradientSolver::new(6);
    
    let vcc = hybrid.add_element(Box::new(VoltageSource::new(12.0)));
    let vin = hybrid.add_element(Box::new(VoltageSource::new(1.0)));
    let r1 = hybrid.add_element(Box::new(Resistor::new(10000.0)));
    let r2 = hybrid.add_element(Box::new(Resistor::new(2000.0)));
    let r3 = hybrid.add_element(Box::new(Resistor::new(1000.0)));
    let bjt = hybrid.add_element(Box::new(BJT::new(1e-14, 100.0, 0.026, BJTType::NPN)));
    let mosfet = hybrid.add_element(Box::new(MOSFET::new(100e-6, 2.0, 0.01, MOSFETType::NMOS)));
    let diode = hybrid.add_element(Box::new(Diode::new(1e-12, 0.026)));
    
    // Connect mixed circuit
    hybrid.connect(vcc, 1, 0);      // VCC
    hybrid.connect(vin, 2, 0);      // Input
    hybrid.connect(r1, 2, 3);       // BJT base resistor
    hybrid.connect(r2, 1, 4);       // BJT collector resistor
    hybrid.connect(r3, 4, 5);       // MOSFET load resistor
    hybrid.connect(bjt, 3, 0);      // BJT
    hybrid.connect(mosfet, 4, 0);   // MOSFET
    hybrid.connect(diode, 5, 0);    // Diode
    
    let (device_voltages_hybrid, _current_hybrid, iters_hybrid, time_hybrid) = hybrid.solve();
    
    // Create smart damping solver
    let mut smart = SmartDampingSolver::new(6);
    
    let vcc = smart.add_element(Box::new(VoltageSource::new(12.0)));
    let vin = smart.add_element(Box::new(VoltageSource::new(1.0)));
    let r1 = smart.add_element(Box::new(Resistor::new(10000.0)));
    let r2 = smart.add_element(Box::new(Resistor::new(2000.0)));
    let r3 = smart.add_element(Box::new(Resistor::new(1000.0)));
    let bjt = smart.add_element(Box::new(BJT::new(1e-14, 100.0, 0.026, BJTType::NPN)));
    let mosfet = smart.add_element(Box::new(MOSFET::new(100e-6, 2.0, 0.01, MOSFETType::NMOS)));
    let diode = smart.add_element(Box::new(Diode::new(1e-12, 0.026)));
    
    smart.connect(vcc, 1, 0);
    smart.connect(vin, 2, 0);
    smart.connect(r1, 2, 3);
    smart.connect(r2, 1, 4);
    smart.connect(r3, 4, 5);
    smart.connect(bjt, 3, 0);
    smart.connect(mosfet, 4, 0);
    smart.connect(diode, 5, 0);
    
    let (device_voltages_smart, _current_smart, iters_smart, time_smart) = smart.solve();
    
    println!("    Hybrid (80%):   {} devices, {:.1}ms, {} iters", 
             device_voltages_hybrid.len(), time_hybrid, iters_hybrid);
    println!("    Smart Damping:  {} devices, {:.1}ms, {} iters", 
             device_voltages_smart.len(), time_smart, iters_smart);
    
    if time_smart > 0.0 {
        println!("    Speed ratio: {:.1}x", time_smart / time_hybrid);
    }
    
    // Show all device voltages
    println!("    Device voltages (Hybrid):   {:?}", 
             device_voltages_hybrid.iter().map(|&v| format!("{:.3}V", v)).collect::<Vec<_>>());
    println!("    Device voltages (Smart):    {:?}", 
             device_voltages_smart.iter().map(|&v| format!("{:.3}V", v)).collect::<Vec<_>>());
}

fn test_all_device_types() {
    println!("  Testing circuit with all semiconductor types...");
    
    // Create hybrid solver
    let mut hybrid = HybridLogGradientSolver::new(7);
    
    let vcc = hybrid.add_element(Box::new(VoltageSource::new(15.0)));
    let vin = hybrid.add_element(Box::new(VoltageSource::new(2.0)));
    let r1 = hybrid.add_element(Box::new(Resistor::new(10000.0)));
    let r2 = hybrid.add_element(Box::new(Resistor::new(5000.0)));
    let r3 = hybrid.add_element(Box::new(Resistor::new(2000.0)));
    let r4 = hybrid.add_element(Box::new(Resistor::new(1000.0)));
    let bjt = hybrid.add_element(Box::new(BJT::new(1e-14, 80.0, 0.026, BJTType::NPN)));
    let mosfet = hybrid.add_element(Box::new(MOSFET::new(200e-6, 1.5, 0.02, MOSFETType::NMOS)));
    let opamp = hybrid.add_element(Box::new(OPAMP::new(200000.0, 14.0, 1.0)));
    let diode = hybrid.add_element(Box::new(Diode::new(1e-12, 0.026)));
    
    // Connect comprehensive circuit
    hybrid.connect(vcc, 1, 0);      // VCC
    hybrid.connect(vin, 2, 0);      // Input
    hybrid.connect(r1, 2, 3);       // Input resistor
    hybrid.connect(r2, 1, 4);       // BJT collector resistor
    hybrid.connect(r3, 4, 5);       // MOSFET drain resistor
    hybrid.connect(r4, 5, 6);       // OPAMP load resistor
    hybrid.connect(bjt, 3, 0);      // BJT
    hybrid.connect(mosfet, 4, 0);   // MOSFET
    hybrid.connect(opamp, 5, 0);    // OPAMP
    hybrid.connect(diode, 6, 0);    // Diode
    
    let (device_voltages_hybrid, _current_hybrid, iters_hybrid, time_hybrid) = hybrid.solve();
    
    // Create smart damping solver
    let mut smart = SmartDampingSolver::new(7);
    
    let vcc = smart.add_element(Box::new(VoltageSource::new(15.0)));
    let vin = smart.add_element(Box::new(VoltageSource::new(2.0)));
    let r1 = smart.add_element(Box::new(Resistor::new(10000.0)));
    let r2 = smart.add_element(Box::new(Resistor::new(5000.0)));
    let r3 = smart.add_element(Box::new(Resistor::new(2000.0)));
    let r4 = smart.add_element(Box::new(Resistor::new(1000.0)));
    let bjt = smart.add_element(Box::new(BJT::new(1e-14, 80.0, 0.026, BJTType::NPN)));
    let mosfet = smart.add_element(Box::new(MOSFET::new(200e-6, 1.5, 0.02, MOSFETType::NMOS)));
    let opamp = smart.add_element(Box::new(OPAMP::new(200000.0, 14.0, 1.0)));
    let diode = smart.add_element(Box::new(Diode::new(1e-12, 0.026)));
    
    smart.connect(vcc, 1, 0);
    smart.connect(vin, 2, 0);
    smart.connect(r1, 2, 3);
    smart.connect(r2, 1, 4);
    smart.connect(r3, 4, 5);
    smart.connect(r4, 5, 6);
    smart.connect(bjt, 3, 0);
    smart.connect(mosfet, 4, 0);
    smart.connect(opamp, 5, 0);
    smart.connect(diode, 6, 0);
    
    let (device_voltages_smart, _current_smart, iters_smart, time_smart) = smart.solve();
    
    println!("    Hybrid (80%):   {} devices, {:.1}ms, {} iters", 
             device_voltages_hybrid.len(), time_hybrid, iters_hybrid);
    println!("    Smart Damping:  {} devices, {:.1}ms, {} iters", 
             device_voltages_smart.len(), time_smart, iters_smart);
    
    if time_smart > 0.0 {
        println!("    Speed ratio: {:.1}x", time_smart / time_hybrid);
    }
    
    // Show all device voltages with labels
    if device_voltages_hybrid.len() >= 4 && device_voltages_smart.len() >= 4 {
        println!("    BJT VBE     (Hybrid/Smart): {:.3}V / {:.3}V", 
                 device_voltages_hybrid[0], device_voltages_smart[0]);
        println!("    MOSFET VGS  (Hybrid/Smart): {:.3}V / {:.3}V", 
                 device_voltages_hybrid[1], device_voltages_smart[1]);
        println!("    OPAMP Vin   (Hybrid/Smart): {:.3}V / {:.3}V", 
                 device_voltages_hybrid[2], device_voltages_smart[2]);
        println!("    Diode Vd    (Hybrid/Smart): {:.3}V / {:.3}V", 
                 device_voltages_hybrid[3], device_voltages_smart[3]);
    }
}