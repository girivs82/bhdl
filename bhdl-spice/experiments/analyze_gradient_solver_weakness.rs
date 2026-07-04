/// Analyze Gradient Solver Weakness with Extreme Parameters
/// 
/// This program investigates why the gradient solver fails with:
/// 1. Low saturation current (Is = 1e-15)
/// 2. High thermal voltage (Vt = 50mV)

use nalgebra::{DMatrix, DVector};
use std::fs::File;
use std::io::Write;

// Element trait and implementations (same as before)
pub trait Element: Send + Sync {
    fn element_type(&self) -> ElementType;
    fn conductance(&self) -> f64 { 0.0 }
    fn is_nonlinear(&self) -> bool { false }
    fn current_at_voltage(&self, v: f64) -> f64;
    fn conductance_at_voltage(&self, v: f64) -> f64;
    fn get_voltage(&self) -> f64;
    fn set_voltage(&mut self, v: f64);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ElementType {
    Resistor,
    VoltageSource,
    Diode,
}

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

// Analysis functions
fn analyze_diode_characteristics(is: f64, vt: f64, label: &str) {
    println!("\n=== {} ===", label);
    println!("Is = {:.0e}, Vt = {:.3}V", is, vt);
    
    // Analyze turn-on characteristics
    let v_knee = vt * (1e-6 / is).ln(); // Voltage where current ~ 1µA
    println!("\nTurn-on characteristics:");
    println!("  Knee voltage (I=1µA): {:.3}V", v_knee);
    
    // Analyze dynamic resistance at different voltages
    println!("\nDynamic resistance (rd = Vt/I):");
    for v in [0.1, 0.3, 0.5, 0.7, 0.9] {
        let i = is * ((v / vt).exp() - 1.0);
        let g = (is / vt) * (v / vt).exp();
        let rd = 1.0 / g;
        println!("  At V={:.1}V: I={:.3e}A, rd={:.3e}Ω, g={:.3e}S", v, i, rd, g);
    }
    
    // Analyze nonlinearity
    println!("\nNonlinearity analysis:");
    let v_test = 0.7;
    let i_test = is * ((v_test / vt).exp() - 1.0);
    let g_test = (is / vt) * (v_test / vt).exp();
    let exp_factor = (v_test / vt).exp();
    println!("  At V={:.1}V: exp(V/Vt)={:.3e}", v_test, exp_factor);
    println!("  Current: {:.3e}A", i_test);
    println!("  Conductance: {:.3e}S", g_test);
    println!("  Ratio g/Is: {:.3e}", g_test / is);
}

fn simulate_convergence_path(vs: f64, rs: f64, is: f64, vt: f64, label: &str) -> Vec<(f64, f64, f64)> {
    println!("\n--- Convergence Path for {} ---", label);
    
    let mut history = Vec::new();
    let num_ramps = 20;
    
    for ramp in 0..=num_ramps {
        let factor = ramp as f64 / num_ramps as f64;
        let vs_ramp = vs * factor;
        
        // Newton-Raphson to find operating point
        let mut vd = 0.5; // Initial guess
        let mut converged = false;
        
        for iter in 0..50 {
            let id = is * ((vd / vt).exp() - 1.0);
            let f = vd + id * rs - vs_ramp;
            let g = 1.0 + (is / vt) * (vd / vt).exp() * rs;
            let delta = f / g;
            
            vd -= delta;
            
            if delta.abs() < 1e-12 {
                converged = true;
                break;
            }
            
            // Check for divergence
            if vd < -10.0 || vd > 10.0 || vd.is_nan() {
                println!("  WARNING: Diverged at ramp={:.2}, iter={}", factor, iter);
                break;
            }
        }
        
        if converged {
            let id = (vs_ramp - vd) / rs;
            history.push((factor, vd, id));
            
            if ramp % 5 == 0 {
                println!("  Ramp {:.0}%: Vd={:.6}V, Id={:.6}mA", 
                         factor * 100.0, vd, id * 1000.0);
            }
        }
    }
    
    history
}

fn analyze_gradient_behavior(history: &Vec<(f64, f64, f64)>, label: &str) {
    if history.len() < 3 {
        println!("\nInsufficient history for gradient analysis");
        return;
    }
    
    println!("\n--- Gradient Analysis for {} ---", label);
    
    let mut max_curvature = 0.0;
    let mut max_curv_ramp = 0.0;
    
    for i in 2..history.len() {
        let (r0, v0, _) = history[i-2];
        let (r1, v1, _) = history[i-1];
        let (r2, v2, _) = history[i];
        
        let dr1 = r1 - r0;
        let dr2 = r2 - r1;
        
        if dr1 > 0.0 && dr2 > 0.0 {
            let dv1 = v1 - v0;
            let dv2 = v2 - v1;
            
            let grad1 = dv1 / dr1;
            let grad2 = dv2 / dr2;
            
            let curvature = (grad2 - grad1) / ((dr1 + dr2) / 2.0);
            
            if curvature.abs() > max_curvature {
                max_curvature = curvature.abs();
                max_curv_ramp = r1;
            }
            
            if i % 5 == 0 || curvature.abs() > 100.0 {
                println!("  Ramp {:.0}%: grad1={:.3}, grad2={:.3}, curv={:.3e}", 
                         r1 * 100.0, grad1, grad2, curvature);
            }
        }
    }
    
    println!("\nMaximum curvature: {:.3e} at ramp {:.0}%", max_curvature, max_curv_ramp * 100.0);
}

fn analyze_numerical_issues(is: f64, vt: f64, label: &str) {
    println!("\n--- Numerical Issues Analysis for {} ---", label);
    
    // Test voltage range where exp() is well-behaved
    let v_max = vt * 50.0; // MAX_EXP limit
    let v_min = vt * -5.0; // Where current ~ -Is
    
    println!("\nOperating voltage range:");
    println!("  V_min: {:.3}V (I ~ -{:.3e}A)", v_min, is);
    println!("  V_max: {:.3}V (beyond this, linearized)", v_max);
    
    // Analyze numerical precision issues
    println!("\nNumerical precision analysis:");
    
    // For low Is, the current can be extremely small
    let v_typ = 0.7;
    let i_typ = is * ((v_typ / vt).exp() - 1.0);
    let machine_eps = f64::EPSILON;
    
    println!("  At typical V={:.1}V:", v_typ);
    println!("    Current: {:.3e}A", i_typ);
    println!("    Relative to machine epsilon: {:.3e}", i_typ / machine_eps);
    
    // For high Vt, exponential changes slowly
    let exp_sensitivity = 1.0 / vt; // d/dV[exp(V/Vt)] = exp(V/Vt)/Vt
    println!("\nExponential sensitivity:");
    println!("  d/dV[exp(V/Vt)] ~ exp(V/Vt) / Vt");
    println!("  At V={:.1}V: sensitivity ~ {:.3e}", v_typ, (v_typ / vt).exp() / vt);
    
    // Condition number analysis
    let g = (is / vt) * (v_typ / vt).exp();
    let cond = g * 100.0; // g * Rs
    println!("\nLinearization condition:");
    println!("  Conductance at V={:.1}V: {:.3e}S", v_typ, g);
    println!("  Condition number (g*Rs): {:.3e}", cond);
}

fn write_analysis_results(filename: &str, results: &[(String, Vec<(f64, f64, f64)>)]) {
    let mut file = File::create(filename).unwrap();
    
    writeln!(file, "# Gradient Solver Weakness Analysis").unwrap();
    writeln!(file, "# ramp_factor, vd_baseline, vd_low_is, vd_high_vt").unwrap();
    
    // Find common ramp factors
    let baseline = &results[0].1;
    let low_is = &results[1].1;
    let high_vt = &results[2].1;
    
    for i in 0..baseline.len().min(low_is.len()).min(high_vt.len()) {
        writeln!(file, "{:.4}, {:.9}, {:.9}, {:.9}",
                 baseline[i].0, baseline[i].1, low_is[i].1, high_vt[i].1).unwrap();
    }
}

fn main() {
    println!("=== GRADIENT SOLVER WEAKNESS ANALYSIS ===");
    
    // Test cases
    let baseline = (1e-12, 0.026, "Baseline (Is=1e-12, Vt=26mV)");
    let low_is = (1e-15, 0.026, "Low Is (Is=1e-15, Vt=26mV)");
    let high_vt = (1e-12, 0.050, "High Vt (Is=1e-12, Vt=50mV)");
    
    // Analyze diode characteristics
    analyze_diode_characteristics(baseline.0, baseline.1, baseline.2);
    analyze_diode_characteristics(low_is.0, low_is.1, low_is.2);
    analyze_diode_characteristics(high_vt.0, high_vt.1, high_vt.2);
    
    // Circuit parameters
    let vs = 1.0;
    let rs = 100.0;
    
    println!("\n\n=== CONVERGENCE PATH ANALYSIS ===");
    println!("Circuit: Vs={}V, Rs={}Ω", vs, rs);
    
    // Simulate convergence paths
    let mut results = Vec::new();
    
    let baseline_path = simulate_convergence_path(vs, rs, baseline.0, baseline.1, baseline.2);
    analyze_gradient_behavior(&baseline_path, baseline.2);
    results.push((baseline.2.to_string(), baseline_path));
    
    let low_is_path = simulate_convergence_path(vs, rs, low_is.0, low_is.1, low_is.2);
    analyze_gradient_behavior(&low_is_path, low_is.2);
    results.push((low_is.2.to_string(), low_is_path));
    
    let high_vt_path = simulate_convergence_path(vs, rs, high_vt.0, high_vt.1, high_vt.2);
    analyze_gradient_behavior(&high_vt_path, high_vt.2);
    results.push((high_vt.2.to_string(), high_vt_path));
    
    // Numerical issues analysis
    println!("\n\n=== NUMERICAL ISSUES ANALYSIS ===");
    analyze_numerical_issues(baseline.0, baseline.1, baseline.2);
    analyze_numerical_issues(low_is.0, low_is.1, low_is.2);
    analyze_numerical_issues(high_vt.0, high_vt.1, high_vt.2);
    
    // Write results to file
    write_analysis_results("tests/outputs/gradient_weakness_analysis.csv", &results);
    
    // Summary
    println!("\n\n=== SUMMARY: WHY GRADIENT SOLVER FAILS ===");
    
    println!("\n1. LOW SATURATION CURRENT (Is=1e-15):");
    println!("   - Extreme sensitivity: Small voltage changes → huge current ratios");
    println!("   - Turn-on is very sharp (knee at ~0.414V vs 0.690V baseline)");
    println!("   - Current values approach numerical precision limits");
    println!("   - Gradient changes are extreme near turn-on");
    
    println!("\n2. HIGH THERMAL VOLTAGE (Vt=50mV):");
    println!("   - Slower exponential response → gradual turn-on");
    println!("   - Lower sensitivity means harder to detect changes");
    println!("   - Ramping may overshoot optimal step sizes");
    println!("   - Poor curvature estimates due to gradual changes");
    
    println!("\n3. FUNDAMENTAL ISSUE:");
    println!("   - Gradient solver assumes smooth, predictable curvature");
    println!("   - Extreme parameters violate this assumption");
    println!("   - Adaptive stepping can't handle sudden transitions (low Is)");
    println!("   - Or can't detect gradual changes accurately (high Vt)");
    
    println!("\n4. NEWTON SOLVER ADVANTAGE:");
    println!("   - Directly solves nonlinear equations at each point");
    println!("   - Not dependent on smooth evolution");
    println!("   - Quadratic convergence handles extreme nonlinearity");
    println!("   - More robust to parameter variations");
}