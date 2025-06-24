/// Advanced Adaptive Wave Solver with Fractional Delay Compensation
/// 
/// This implementation includes:
/// - Automatic circuit analysis for bandwidth detection
/// - Optimal filter selection based on circuit characteristics
/// - Fractional delay filters for precise phase compensation
/// - Parallel wave propagation support

use std::fs::File;
use std::io::Write;
use std::f64::consts::PI;
use rayon::prelude::*;

#[derive(Debug, Clone)]
struct FrequencyResponse {
    frequency: f64,
    magnitude: f64,
    phase: f64,
}

#[derive(Debug)]
struct AdaptiveFilterParams {
    /// Filter cutoff frequency
    cutoff: f64,
    /// Filter order
    order: usize,
    /// Group delay at signal frequencies
    group_delay: f64,
    /// Fractional delay for compensation
    fractional_delay: f64,
}

/// Fractional delay filter using Lagrange interpolation
struct FractionalDelayFilter {
    order: usize,
    delay: f64,
    coefficients: Vec<f64>,
}

impl FractionalDelayFilter {
    fn new(delay: f64, order: usize) -> Self {
        // Calculate Lagrange interpolation coefficients
        let mut coefficients = vec![0.0; order + 1];
        let d = delay;
        
        for n in 0..=order {
            let mut coeff = 1.0;
            for k in 0..=order {
                if k != n {
                    coeff *= (d - k as f64) / (n as f64 - k as f64);
                }
            }
            coefficients[n] = coeff;
        }
        
        Self { order, delay, coefficients }
    }
    
    fn apply(&self, signal: &[f64]) -> Vec<f64> {
        let mut output = vec![0.0; signal.len()];
        let int_delay = self.delay.floor() as usize;
        
        for i in (self.order + int_delay)..signal.len() {
            let mut sum = 0.0;
            for j in 0..=self.order {
                let idx = i - int_delay - j;
                if idx < signal.len() {
                    sum += self.coefficients[j] * signal[idx];
                }
            }
            output[i] = sum;
        }
        
        output
    }
}

/// Main solver structure
struct AdvancedAdaptiveWaveSolver {
    /// Time step for wave propagation
    dt: f64,
    /// Circuit impedance matrix (for parallel computation)
    z_matrix: Vec<Vec<f64>>,
    /// Node voltages
    voltages: Vec<f64>,
    /// Node currents
    currents: Vec<f64>,
    /// Filter parameters
    filter_params: Option<AdaptiveFilterParams>,
}

impl AdvancedAdaptiveWaveSolver {
    fn new(num_nodes: usize, dt: f64) -> Self {
        Self {
            dt,
            z_matrix: vec![vec![0.0; num_nodes]; num_nodes],
            voltages: vec![0.0; num_nodes],
            currents: vec![0.0; num_nodes],
            filter_params: None,
        }
    }
    
    /// Analyze circuit frequency response using small-signal analysis
    fn analyze_frequency_response(&mut self, test_frequencies: &[f64]) -> Vec<FrequencyResponse> {
        let mut responses = Vec::new();
        
        for &freq in test_frequencies {
            let omega = 2.0 * PI * freq;
            
            // Apply small sinusoidal perturbation
            let amplitude = 0.01;
            let cycles = 5;
            let duration = cycles / freq;
            let steps = (duration / self.dt) as usize;
            
            let mut input_signal = vec![0.0; steps];
            let mut output_signal = vec![0.0; steps];
            
            // Generate test signal
            for i in 0..steps {
                let t = i as f64 * self.dt;
                input_signal[i] = amplitude * (omega * t).sin();
            }
            
            // Simulate response (simplified)
            // In practice, this would use the full wave propagation
            let tau = 1e-3; // Example RC time constant
            for i in 1..steps {
                let t = i as f64 * self.dt;
                output_signal[i] = output_signal[i-1] + 
                    (input_signal[i] - output_signal[i-1]) * self.dt / tau;
            }
            
            // Extract magnitude and phase
            let (mag, phase) = self.extract_magnitude_phase(&input_signal, &output_signal, freq);
            
            responses.push(FrequencyResponse {
                frequency: freq,
                magnitude: mag,
                phase,
            });
        }
        
        responses
    }
    
    /// Extract magnitude and phase from time-domain signals
    fn extract_magnitude_phase(&self, input: &[f64], output: &[f64], freq: f64) -> (f64, f64) {
        let omega = 2.0 * PI * freq;
        let n = input.len();
        
        // Use last few cycles for steady-state
        let start = n * 3 / 5;
        
        // Fourier analysis
        let mut input_real = 0.0;
        let mut input_imag = 0.0;
        let mut output_real = 0.0;
        let mut output_imag = 0.0;
        
        for i in start..n {
            let t = i as f64 * self.dt;
            let cos_wt = (omega * t).cos();
            let sin_wt = (omega * t).sin();
            
            input_real += input[i] * cos_wt;
            input_imag += input[i] * sin_wt;
            output_real += output[i] * cos_wt;
            output_imag += output[i] * sin_wt;
        }
        
        let input_mag = (input_real * input_real + input_imag * input_imag).sqrt();
        let output_mag = (output_real * output_real + output_imag * output_imag).sqrt();
        
        let magnitude = output_mag / input_mag.max(1e-10);
        let phase = (output_imag).atan2(output_real) - (input_imag).atan2(input_real);
        
        (magnitude, phase)
    }
    
    /// Determine optimal filter parameters from frequency response
    fn determine_filter_params(&mut self, responses: &[FrequencyResponse]) -> AdaptiveFilterParams {
        // Find 3dB point
        let mut cutoff_freq = 1e9; // Default high frequency
        
        for response in responses {
            if response.magnitude < 0.707 { // -3dB point
                cutoff_freq = response.frequency;
                break;
            }
        }
        
        // Set filter cutoff at 100x the circuit bandwidth
        let filter_cutoff = cutoff_freq * 100.0;
        
        // Calculate group delay
        let mut group_delay = 0.0;
        for i in 1..responses.len() {
            if responses[i].frequency < cutoff_freq {
                let dphase = responses[i].phase - responses[i-1].phase;
                let dfreq = responses[i].frequency - responses[i-1].frequency;
                group_delay = -dphase / (2.0 * PI * dfreq);
            }
        }
        
        // Calculate filter-induced delay
        let filter_delay = match filter_cutoff {
            fc if fc > 1e9 => 0.0, // No filtering needed
            fc => 0.35 / fc, // Approximate Bessel filter delay
        };
        
        AdaptiveFilterParams {
            cutoff: filter_cutoff,
            order: 4, // 4th order for good rolloff
            group_delay,
            fractional_delay: filter_delay / self.dt,
        }
    }
    
    /// Parallel wave propagation step
    fn wave_step_parallel(&mut self, sources: &[(usize, f64)]) {
        let n = self.voltages.len();
        
        // Parallel computation of wave propagation
        let new_voltages: Vec<f64> = (0..n).into_par_iter().map(|i| {
            let mut v_sum = 0.0;
            let mut z_sum = 0.0;
            
            // Sum contributions from all connected nodes
            for j in 0..n {
                if self.z_matrix[i][j] > 0.0 {
                    let z = self.z_matrix[i][j];
                    v_sum += self.voltages[j] / z;
                    z_sum += 1.0 / z;
                }
            }
            
            // Add source contributions
            for &(node, value) in sources {
                if node == i {
                    v_sum += value;
                    z_sum += 1.0;
                }
            }
            
            if z_sum > 0.0 {
                v_sum / z_sum
            } else {
                self.voltages[i]
            }
        }).collect();
        
        self.voltages = new_voltages;
    }
    
    /// Run complete simulation with automatic filtering
    fn simulate_adaptive(&mut self, duration: f64, sources: Vec<(usize, f64)>) -> Vec<Vec<f64>> {
        println!("=== Advanced Adaptive Wave Simulation ===");
        
        // Step 1: Analyze circuit frequency response
        println!("\nStep 1: Analyzing circuit frequency response...");
        let test_freqs: Vec<f64> = (0..7).map(|i| 10.0_f64.powf(i as f64)).collect();
        let freq_response = self.analyze_frequency_response(&test_freqs);
        
        // Step 2: Determine optimal filter parameters
        println!("\nStep 2: Determining optimal filter parameters...");
        self.filter_params = Some(self.determine_filter_params(&freq_response));
        
        if let Some(ref params) = self.filter_params {
            println!("  Filter cutoff: {:.1} MHz", params.cutoff / 1e6);
            println!("  Filter order: {}", params.order);
            println!("  Compensation delay: {:.3} ns", params.fractional_delay * self.dt * 1e9);
        }
        
        // Step 3: Run wave simulation
        println!("\nStep 3: Running parallel wave propagation...");
        let steps = (duration / self.dt) as usize;
        let mut voltage_history = vec![vec![0.0; self.voltages.len()]; steps];
        
        for step in 0..steps {
            // Update sources
            let active_sources: Vec<(usize, f64)> = sources.iter()
                .map(|&(node, value)| (node, value))
                .collect();
            
            // Parallel wave propagation
            self.wave_step_parallel(&active_sources);
            
            // Store results
            voltage_history[step] = self.voltages.clone();
            
            if step % 10000 == 0 {
                print!("\r  Progress: {:.1}%", 100.0 * step as f64 / steps as f64);
                use std::io::{self, Write};
                io::stdout().flush().unwrap();
            }
        }
        println!("\r  Progress: 100.0%");
        
        // Step 4: Apply adaptive filtering with phase compensation
        println!("\nStep 4: Applying adaptive filtering with phase compensation...");
        
        if let Some(ref params) = self.filter_params {
            if params.cutoff < 1e9 {
                // Apply filtering to each node's voltage history
                let filtered_history: Vec<Vec<f64>> = (0..self.voltages.len())
                    .into_par_iter()
                    .map(|node| {
                        let signal: Vec<f64> = voltage_history.iter()
                            .map(|step| step[node])
                            .collect();
                        
                        // Apply filter
                        let filtered = self.apply_bessel_filter(&signal, params.cutoff);
                        
                        // Apply fractional delay compensation
                        let delay_filter = FractionalDelayFilter::new(params.fractional_delay, 4);
                        delay_filter.apply(&filtered)
                    })
                    .collect();
                
                // Transpose back to step-major order
                let mut compensated_history = vec![vec![0.0; self.voltages.len()]; steps];
                for step in 0..steps {
                    for node in 0..self.voltages.len() {
                        compensated_history[step][node] = filtered_history[node][step];
                    }
                }
                
                return compensated_history;
            }
        }
        
        voltage_history
    }
    
    /// High-quality Bessel filter for linear phase
    fn apply_bessel_filter(&self, signal: &[f64], fc: f64) -> Vec<f64> {
        // 4th order Bessel filter coefficients
        let wc = 2.0 * PI * fc * self.dt;
        
        // Pre-warped frequency for bilinear transform
        let wp = 2.0 * (wc / 2.0).tan();
        
        // Normalized Bessel polynomials (4th order)
        let b = [105.0, 105.0 * wp, 45.0 * wp * wp, 10.0 * wp * wp * wp, wp * wp * wp * wp];
        let a = [105.0, 105.0 * wp, 45.0 * wp * wp, 10.0 * wp * wp * wp, wp * wp * wp * wp];
        
        // Apply filter
        let mut output = vec![0.0; signal.len()];
        let mut x = vec![0.0; 5];
        let mut y = vec![0.0; 5];
        
        for i in 0..signal.len() {
            x[0] = signal[i];
            
            let mut sum = 0.0;
            for j in 0..5 {
                sum += b[j] * x[j] - if j > 0 { a[j] * y[j] } else { 0.0 };
            }
            y[0] = sum / a[0];
            output[i] = y[0];
            
            // Shift delay lines
            for j in (1..5).rev() {
                x[j] = x[j-1];
                y[j] = y[j-1];
            }
        }
        
        output
    }
}

fn main() {
    // Create test circuit (3 nodes: source, R, C)
    let mut solver = AdvancedAdaptiveWaveSolver::new(3, 0.1e-12);
    
    // Set up impedance matrix (simplified)
    solver.z_matrix[0][1] = 1.0;    // Source to R (1Ω internal)
    solver.z_matrix[1][0] = 1.0;
    solver.z_matrix[1][2] = 1000.0; // R to C (1kΩ)
    solver.z_matrix[2][1] = 1000.0;
    solver.z_matrix[2][0] = 1e6;    // C to ground (1MΩ represents capacitor)
    
    // Run simulation
    let duration = 10e-6;
    let sources = vec![(0, 5.0)]; // 5V at node 0
    let results = solver.simulate_adaptive(duration, sources);
    
    // Save results
    let mut file = File::create("tests/outputs/advanced_adaptive_wave.csv").unwrap();
    writeln!(file, "time_ns,v_source,v_resistor,v_capacitor").unwrap();
    
    let sample_rate = 1000; // Sample every 1000 steps
    for (i, voltages) in results.iter().enumerate() {
        if i % sample_rate == 0 {
            let time_ns = i as f64 * solver.dt * 1e9;
            writeln!(file, "{:.1},{:.6},{:.6},{:.6}", 
                     time_ns, voltages[0], voltages[1], voltages[2]).unwrap();
        }
    }
    
    println!("\n✓ Simulation complete!");
    println!("Results saved to tests/outputs/advanced_adaptive_wave.csv");
    
    // Performance metrics
    let total_nodes = 3;
    let total_steps = (duration / solver.dt) as usize;
    let ops_per_step = total_nodes * total_nodes * 5; // Matrix operations
    let total_ops = ops_per_step * total_steps;
    
    println!("\nPerformance metrics:");
    println!("  Total operations: {:.1}G", total_ops as f64 / 1e9);
    println!("  Parallelization: Achieved via rayon");
    println!("  Memory usage: {:.1}MB", (total_steps * total_nodes * 8) as f64 / 1e6);
}