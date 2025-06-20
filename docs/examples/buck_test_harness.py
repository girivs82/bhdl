#!/usr/bin/env python3
"""
Test harness for buck converter PLI behavioral model
Shows Python's advantages for complex test scenarios
"""

import numpy as np
import matplotlib.pyplot as plt
from scipy import signal
import pandas as pd
from typing import Dict, List, Tuple
import json

from bhdl_pli import TestHarness, Measurement, Assertion

class BuckTestHarness(TestHarness):
    """
    Advanced test harness showing Python capabilities:
    - Complex measurements using scipy
    - Statistical analysis
    - Advanced plotting
    - Machine learning for anomaly detection (if needed)
    """
    
    def __init__(self):
        super().__init__()
        self.measurements = {}
        self.test_results = []
        
    def setup(self):
        """Configure test environment"""
        # Can load test configurations from files
        with open('buck_test_config.json', 'r') as f:
            self.config = json.load(f)
            
        # Set up data logging
        self.data_logger = pd.DataFrame()
        
    def run_load_transient_test(self):
        """Complex load transient test with analysis"""
        # Define load profile
        load_profile = [
            (0.0, 3.3),      # 1A load
            (0.005, 6.6),    # 0.5A load (step down)
            (0.010, 2.2),    # 1.5A load (step up)
            (0.015, 3.3),    # Back to 1A
        ]
        
        # Apply load profile
        for time, resistance in load_profile:
            self.schedule_event(time, lambda r=resistance: self.set_load(r))
            
        # Run simulation
        self.run_until(0.020)
        
        # Get waveform data
        waveforms = self.get_model_data('waveform_data')
        
        # Analyze transient response
        vout_data = np.array(waveforms['vout_actual'])
        time_data = vout_data[:, 0]
        voltage_data = vout_data[:, 1]
        
        # Find transient events
        transients = self._detect_transients(time_data, voltage_data)
        
        # Measure each transient
        for i, (t_start, t_end) in enumerate(transients):
            # Extract transient window
            mask = (time_data >= t_start) & (time_data <= t_end)
            t_window = time_data[mask]
            v_window = voltage_data[mask]
            
            # Calculate metrics
            undershoot = 3.3 - np.min(v_window)
            overshoot = np.max(v_window) - 3.3
            settling_idx = self._find_settling_time(v_window, 3.3, tolerance=0.01)
            settling_time = t_window[settling_idx] - t_start if settling_idx else None
            
            self.measurements[f'transient_{i}'] = {
                'undershoot_v': undershoot,
                'overshoot_v': overshoot,
                'settling_time_s': settling_time,
                'peak_deviation_v': max(abs(undershoot), abs(overshoot))
            }
            
            # Assert requirements
            self.add_assertion(
                f"Transient {i} undershoot < 100mV",
                undershoot < 0.1
            )
            self.add_assertion(
                f"Transient {i} settling < 100µs",
                settling_time < 100e-6 if settling_time else False
            )
            
    def run_frequency_response_test(self):
        """Measure loop frequency response using injection"""
        # This demonstrates Python's signal processing capabilities
        
        # Generate chirp signal for injection
        duration = 0.01  # 10ms
        f_start = 100    # 100Hz
        f_end = 100e3    # 100kHz
        
        t = np.linspace(0, duration, int(duration * 1e6))  # 1MHz sampling
        chirp = 0.01 * signal.chirp(t, f_start, duration, f_end, method='logarithmic')
        
        # Inject into reference voltage
        for i, (time, amplitude) in enumerate(zip(t, chirp)):
            self.schedule_event(time, lambda a=amplitude: self.inject_disturbance(a))
            
        # Run and collect data
        self.run_until(duration)
        
        # Get response data
        vout_data = np.array(self.get_model_data('waveform_data')['vout_actual'])
        
        # Calculate frequency response using FFT
        f, Pxx_in = signal.periodogram(chirp, 1e6)
        _, Pxx_out = signal.periodogram(vout_data[:, 1], 1e6)
        
        # Transfer function magnitude
        H_mag = np.sqrt(Pxx_out / (Pxx_in + 1e-10))
        H_db = 20 * np.log10(H_mag + 1e-10)
        
        # Find key frequencies
        crossover_idx = np.argmin(np.abs(H_db))
        crossover_freq = f[crossover_idx]
        
        # Phase margin estimation (simplified)
        phase_at_crossover = -180 * crossover_freq / (2 * f_end)  # Approximation
        phase_margin = 180 + phase_at_crossover
        
        self.measurements['frequency_response'] = {
            'crossover_frequency_hz': crossover_freq,
            'phase_margin_deg': phase_margin,
            'gain_margin_db': -H_db[crossover_idx]
        }
        
        # Generate Bode plot
        self._generate_bode_plot(f, H_db, "buck_bode_plot.png")
        
    def run_efficiency_sweep(self):
        """Sweep load current and measure efficiency"""
        # This shows parameter sweeping capabilities
        
        load_currents = np.linspace(0.1, 2.0, 20)
        efficiencies = []
        
        for i_load in load_currents:
            # Set load
            r_load = 3.3 / i_load
            self.set_load(r_load)
            
            # Wait for steady state
            self.run_for(0.005)  # 5ms
            
            # Measure efficiency
            measurements = self.get_model_data('measurements')
            eff = measurements.get('efficiency_est', 0)
            efficiencies.append(eff)
            
        # Fit efficiency curve
        # Could use polynomial fit or more complex models
        poly_coeffs = np.polyfit(load_currents, efficiencies, 3)
        eff_fitted = np.polyval(poly_coeffs, load_currents)
        
        # Find peak efficiency point
        peak_idx = np.argmax(eff_fitted)
        peak_efficiency = eff_fitted[peak_idx]
        peak_current = load_currents[peak_idx]
        
        self.measurements['efficiency_sweep'] = {
            'peak_efficiency_percent': peak_efficiency,
            'peak_efficiency_current_a': peak_current,
            'light_load_eff': efficiencies[0],
            'full_load_eff': efficiencies[-1],
            'efficiency_coefficients': poly_coeffs.tolist()
        }
        
        # Plot efficiency curve
        self._plot_efficiency_curve(load_currents, efficiencies, eff_fitted)
        
    def run_monte_carlo_analysis(self):
        """Run Monte Carlo with component variations"""
        # This demonstrates statistical analysis capabilities
        
        n_runs = 100
        results = []
        
        # Component variations (gaussian distribution)
        nominal_values = {
            'l_value': 4.7e-6,
            'cout_value': 100e-6,
            'cout_esr': 20e-3
        }
        
        tolerances = {
            'l_value': 0.20,      # ±20%
            'cout_value': 0.10,   # ±10%  
            'cout_esr': 0.30      # ±30%
        }
        
        for run in range(n_runs):
            # Generate random component values
            params = {}
            for param, nominal in nominal_values.items():
                tolerance = tolerances[param]
                # Log-normal distribution for components
                params[param] = nominal * np.random.lognormal(0, tolerance/3)
                
            # Update model parameters
            self.update_model_params(params)
            
            # Run standard test
            self.reset_model()
            self.run_until(0.010)  # 10ms
            
            # Collect results
            measurements = self.get_model_data('measurements')
            results.append({
                'run': run,
                'vout_ripple': measurements.get('vout_ripple', 0),
                'efficiency': measurements.get('efficiency_est', 0),
                'il_ripple': measurements.get('il_ripple', 0),
                **params
            })
            
        # Convert to DataFrame for analysis
        df = pd.DataFrame(results)
        
        # Statistical analysis
        self.measurements['monte_carlo'] = {
            'vout_ripple_mean': df['vout_ripple'].mean(),
            'vout_ripple_std': df['vout_ripple'].std(),
            'vout_ripple_95pct': df['vout_ripple'].quantile(0.95),
            'efficiency_mean': df['efficiency'].mean(),
            'efficiency_min': df['efficiency'].min(),
            'yield_estimate': (df['vout_ripple'] < 0.050).mean() * 100  # % meeting 50mV spec
        }
        
        # Correlation analysis
        correlations = df[['vout_ripple', 'l_value', 'cout_value', 'cout_esr']].corr()
        
        # Generate plots
        self._plot_monte_carlo_results(df)
        
    def run_thermal_stress_test(self):
        """Test thermal behavior under stress conditions"""
        # Ambient temperature profile
        temp_profile = [
            (0.000, 25),   # Room temp
            (0.010, 85),   # Hot
            (0.020, 125),  # Very hot
            (0.030, 25),   # Cool down
        ]
        
        # Apply temperature profile
        for time, temp in temp_profile:
            self.schedule_event(time, lambda t=temp: self.set_ambient_temp(t))
            
        # Also apply high load
        self.set_load(2.2)  # 1.5A continuous
        
        # Run test
        self.run_until(0.040)
        
        # Analyze thermal behavior
        junction_temp_data = self.get_model_data('waveform_data')['junction_temp']
        
        # Find maximum temperature
        max_temp = max(t[1] for t in junction_temp_data)
        
        # Check for thermal shutdown
        state_data = self.get_model_data('waveform_data')['controller_state']
        thermal_shutdown_occurred = any(s[1] == 'FAULT' for s in state_data)
        
        self.measurements['thermal_stress'] = {
            'max_junction_temp_c': max_temp,
            'thermal_shutdown': thermal_shutdown_occurred,
            'thermal_margin_c': 150 - max_temp
        }
        
    def _detect_transients(self, time: np.ndarray, voltage: np.ndarray) -> List[Tuple[float, float]]:
        """Detect transient events in voltage waveform"""
        # Calculate derivative to find sudden changes
        dv_dt = np.gradient(voltage, time)
        
        # Threshold for transient detection
        threshold = np.std(dv_dt) * 3
        
        # Find transient starts
        transient_mask = np.abs(dv_dt) > threshold
        transient_starts = np.where(np.diff(transient_mask.astype(int)) == 1)[0]
        
        # Find transient windows (until settled)
        transients = []
        for start_idx in transient_starts:
            # Find when it settles (derivative small again)
            end_idx = start_idx + np.argmax(
                np.abs(dv_dt[start_idx:]) < threshold/10
            )
            
            if end_idx > start_idx:
                transients.append((time[start_idx], time[min(end_idx, len(time)-1)]))
                
        return transients
    
    def _find_settling_time(self, voltage: np.ndarray, target: float, tolerance: float) -> Optional[int]:
        """Find index where voltage settles within tolerance of target"""
        settled_mask = np.abs(voltage - target) < tolerance * target
        
        # Need to be settled for at least 10 consecutive samples
        for i in range(len(settled_mask) - 10):
            if all(settled_mask[i:i+10]):
                return i
                
        return None
    
    def _generate_bode_plot(self, freq: np.ndarray, mag_db: np.ndarray, filename: str):
        """Generate Bode magnitude plot"""
        plt.figure(figsize=(10, 6))
        plt.semilogx(freq[1:], mag_db[1:])  # Skip DC
        plt.grid(True, which="both", ls="-", alpha=0.3)
        plt.xlabel('Frequency (Hz)')
        plt.ylabel('Magnitude (dB)')
        plt.title('Buck Converter Loop Response')
        plt.axhline(y=0, color='r', linestyle='--', alpha=0.5)
        plt.savefig(filename)
        plt.close()
        
    def _plot_efficiency_curve(self, currents: np.ndarray, measured: List[float], fitted: np.ndarray):
        """Plot efficiency vs load current"""
        plt.figure(figsize=(10, 6))
        plt.plot(currents, measured, 'o', label='Measured')
        plt.plot(currents, fitted, '-', label='Fitted')
        plt.xlabel('Load Current (A)')
        plt.ylabel('Efficiency (%)')
        plt.title('Buck Converter Efficiency')
        plt.grid(True, alpha=0.3)
        plt.legend()
        plt.savefig('buck_efficiency_curve.png')
        plt.close()
        
    def _plot_monte_carlo_results(self, df: pd.DataFrame):
        """Generate Monte Carlo analysis plots"""
        fig, axes = plt.subplots(2, 2, figsize=(12, 10))
        
        # Ripple histogram
        axes[0, 0].hist(df['vout_ripple'] * 1000, bins=30, alpha=0.7)
        axes[0, 0].axvline(x=50, color='r', linestyle='--', label='50mV limit')
        axes[0, 0].set_xlabel('Output Ripple (mV)')
        axes[0, 0].set_ylabel('Count')
        axes[0, 0].set_title('Output Ripple Distribution')
        axes[0, 0].legend()
        
        # Efficiency histogram
        axes[0, 1].hist(df['efficiency'], bins=30, alpha=0.7)
        axes[0, 1].set_xlabel('Efficiency (%)')
        axes[0, 1].set_ylabel('Count')
        axes[0, 1].set_title('Efficiency Distribution')
        
        # Correlation heatmap
        corr_data = df[['vout_ripple', 'l_value', 'cout_esr']].corr()
        im = axes[1, 0].imshow(corr_data, cmap='coolwarm', aspect='auto', vmin=-1, vmax=1)
        axes[1, 0].set_xticks(range(len(corr_data.columns)))
        axes[1, 0].set_yticks(range(len(corr_data.columns)))
        axes[1, 0].set_xticklabels(corr_data.columns, rotation=45)
        axes[1, 0].set_yticklabels(corr_data.columns)
        axes[1, 0].set_title('Parameter Correlations')
        plt.colorbar(im, ax=axes[1, 0])
        
        # Scatter plot of ripple vs ESR
        axes[1, 1].scatter(df['cout_esr'] * 1000, df['vout_ripple'] * 1000, alpha=0.5)
        axes[1, 1].set_xlabel('Output Cap ESR (mΩ)')
        axes[1, 1].set_ylabel('Output Ripple (mV)')
        axes[1, 1].set_title('Ripple vs ESR')
        
        plt.tight_layout()
        plt.savefig('buck_monte_carlo_analysis.png')
        plt.close()
        
    def generate_report(self):
        """Generate comprehensive test report"""
        report = {
            'test_summary': {
                'total_assertions': len(self.assertions),
                'passed': sum(1 for a in self.assertions if a.passed),
                'failed': sum(1 for a in self.assertions if not a.passed)
            },
            'measurements': self.measurements,
            'failed_assertions': [
                {'name': a.name, 'message': a.message} 
                for a in self.assertions if not a.passed
            ]
        }
        
        # Save as JSON
        with open('buck_test_report.json', 'w') as f:
            json.dump(report, f, indent=2)
            
        # Generate HTML report with plots
        self._generate_html_report(report)
        
    def _generate_html_report(self, report: Dict):
        """Generate HTML report with embedded plots"""
        html = f"""
        <html>
        <head>
            <title>Buck Converter Test Report</title>
            <style>
                body {{ font-family: Arial, sans-serif; margin: 20px; }}
                .pass {{ color: green; }}
                .fail {{ color: red; }}
                table {{ border-collapse: collapse; }}
                td, th {{ border: 1px solid #ddd; padding: 8px; }}
            </style>
        </head>
        <body>
            <h1>Buck Converter Behavioral Model Test Report</h1>
            
            <h2>Summary</h2>
            <p>Total Assertions: {report['test_summary']['total_assertions']}</p>
            <p class="pass">Passed: {report['test_summary']['passed']}</p>
            <p class="fail">Failed: {report['test_summary']['failed']}</p>
            
            <h2>Key Measurements</h2>
            <table>
                <tr><th>Measurement</th><th>Value</th></tr>
        """
        
        # Add measurements to table
        for category, values in report['measurements'].items():
            if isinstance(values, dict):
                for key, value in values.items():
                    if isinstance(value, (int, float)):
                        html += f"<tr><td>{category}.{key}</td><td>{value:.4g}</td></tr>"
                        
        html += """
            </table>
            
            <h2>Plots</h2>
            <img src="buck_bode_plot.png" alt="Bode Plot">
            <img src="buck_efficiency_curve.png" alt="Efficiency Curve">
            <img src="buck_monte_carlo_analysis.png" alt="Monte Carlo Analysis">
            
        </body>
        </html>
        """
        
        with open('buck_test_report.html', 'w') as f:
            f.write(html)

# Main test execution
if __name__ == "__main__":
    harness = BuckTestHarness()
    harness.setup()
    
    # Run all tests
    harness.run_load_transient_test()
    harness.run_frequency_response_test()
    harness.run_efficiency_sweep()
    harness.run_monte_carlo_analysis()
    harness.run_thermal_stress_test()
    
    # Generate final report
    harness.generate_report()