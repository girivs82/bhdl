#!/usr/bin/env python3
"""Verify RLC simulation results without external dependencies"""

import csv
import math

def read_csv(filename):
    """Read CSV file and return data"""
    data = {'time_ms': [], 'v_c': [], 'i_circuit': []}
    with open(filename, 'r') as f:
        reader = csv.DictReader(f)
        for row in reader:
            data['time_ms'].append(float(row['time_ms']))
            data['v_c'].append(float(row['v_c']))
            data['i_circuit'].append(float(row['i_circuit']))
    return data

def calculate_theoretical_response(t, R, L, C, V_step):
    """Calculate theoretical overdamped RLC response"""
    omega_0 = 1 / math.sqrt(L * C)
    zeta = R / 2 * math.sqrt(C / L)
    
    if t < 0.001:  # Before step
        return 0.0
    
    t_adj = t - 0.001  # Adjust for step at 1ms
    
    if zeta > 1:  # Overdamped
        sqrt_term = math.sqrt(zeta**2 - 1)
        r1 = -omega_0 * (zeta - sqrt_term)
        r2 = -omega_0 * (zeta + sqrt_term)
        v_cap = V_step * (1 - (r2/(r2-r1)) * math.exp(r1 * t_adj) + 
                         (r1/(r2-r1)) * math.exp(r2 * t_adj))
    else:
        # For now, just handle overdamped case
        v_cap = V_step * (1 - math.exp(-omega_0 * t_adj))
    
    return v_cap

# Read simulation results
print("Reading simulation results...")
data = read_csv('tests/outputs/stable_rlc_response.csv')

# Circuit parameters
R = 50.0    # Ohms
L = 10e-3   # Henries  
C = 100e-6  # Farads
V_step = 5.0  # Volts

# Natural frequency and damping
omega_0 = 1 / math.sqrt(L * C)
zeta = R / 2 * math.sqrt(C / L)

print(f"\nCircuit Analysis:")
print(f"  R = {R} Ω")
print(f"  L = {L*1000} mH")
print(f"  C = {C*1e6} µF")
print(f"  Natural frequency: {omega_0/(2*math.pi):.1f} Hz")
print(f"  Damping ratio ζ = {zeta:.3f}")
print(f"  System is: {'OVERDAMPED' if zeta > 1 else 'UNDERDAMPED' if zeta < 1 else 'CRITICALLY DAMPED'}")

# Check key points in the response
print(f"\nKey Time Points:")
print(f"{'Time (ms)':<12} {'V_cap (sim)':<12} {'V_cap (theory)':<15} {'Error (%)':<10}")
print("-" * 50)

# Time constants for overdamped system
tau1 = 1 / (omega_0 * (zeta - math.sqrt(zeta**2 - 1)))
tau2 = 1 / (omega_0 * (zeta + math.sqrt(zeta**2 - 1)))
print(f"\nTime constants: τ1 = {tau1*1000:.1f} ms, τ2 = {tau2*1000:.1f} ms")

# Check at specific times
check_times = [0.0, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0]  # ms
for check_time in check_times:
    # Find closest time in data
    idx = min(range(len(data['time_ms'])), 
              key=lambda i: abs(data['time_ms'][i] - check_time))
    
    sim_time = data['time_ms'][idx]
    sim_voltage = data['v_c'][idx]
    theory_voltage = calculate_theoretical_response(sim_time/1000, R, L, C, V_step)
    
    if theory_voltage > 0:
        error = (sim_voltage - theory_voltage) / theory_voltage * 100
    else:
        error = 0.0
    
    print(f"{sim_time:<12.1f} {sim_voltage:<12.3f} {theory_voltage:<15.3f} {error:<10.2f}")

# Check steady state
final_voltage = data['v_c'][-1]
final_current = data['i_circuit'][-1]
steady_state_error = abs(final_voltage - V_step)

print(f"\nSteady State Analysis:")
print(f"  Final capacitor voltage: {final_voltage:.3f} V")
print(f"  Final circuit current: {final_current*1e6:.1f} µA")
print(f"  Steady state error: {steady_state_error*1000:.1f} mV ({steady_state_error/V_step*100:.2f}%)")

# Calculate RMS error over the simulation
sum_sq_error = 0
count = 0
for i in range(len(data['time_ms'])):
    if data['time_ms'][i] >= 1.0:  # After step
        theory = calculate_theoretical_response(data['time_ms'][i]/1000, R, L, C, V_step)
        error = (data['v_c'][i] - theory)
        sum_sq_error += error**2
        count += 1

rms_error = math.sqrt(sum_sq_error / count) if count > 0 else 0
rms_error_percent = rms_error / V_step * 100

print(f"\nAccuracy Summary:")
print(f"  RMS error: {rms_error*1000:.2f} mV ({rms_error_percent:.3f}%)")
print(f"  {'✓' if rms_error_percent < 1.0 else '⚠'} Results are {'excellent' if rms_error_percent < 0.1 else 'very good' if rms_error_percent < 1.0 else 'acceptable'}")

# Write verification results
with open('tests/outputs/verification_results.txt', 'w') as f:
    f.write(f"RLC Circuit Simulation Verification\n")
    f.write(f"===================================\n\n")
    f.write(f"Circuit: R={R}Ω, L={L*1000}mH, C={C*1e6}µF\n")
    f.write(f"Damping ratio: {zeta:.3f} (overdamped)\n")
    f.write(f"RMS error: {rms_error_percent:.3f}%\n")
    f.write(f"Steady state error: {steady_state_error/V_step*100:.3f}%\n")
    f.write(f"Status: PASS\n" if rms_error_percent < 1.0 else f"Status: NEEDS IMPROVEMENT\n")

print(f"\nVerification complete. Results written to tests/outputs/verification_results.txt")