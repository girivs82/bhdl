#!/usr/bin/env python3
"""Plot RLC circuit response from perturbation simulation"""

import pandas as pd
import matplotlib.pyplot as plt
import numpy as np

# Read the CSV file
df = pd.read_csv('tests/outputs/perturbation_rlc_response.csv')

# Create figure with subplots
fig, (ax1, ax2) = plt.subplots(2, 1, figsize=(10, 8), sharex=True)

# Plot voltages
ax1.plot(df['time_ms'], df['v_source'], 'k-', label='V_source', linewidth=2)
ax1.plot(df['time_ms'], df['v_c'], 'b-', label='V_capacitor')
ax1.plot(df['time_ms'], df['v_r'], 'r-', label='V_resistor')
ax1.plot(df['time_ms'], df['v_l'], 'g-', label='V_inductor')
ax1.set_ylabel('Voltage (V)')
ax1.set_title('RLC Circuit Step Response - Perturbation Model')
ax1.legend()
ax1.grid(True, alpha=0.3)

# Plot current
ax2.plot(df['time_ms'], df['i_circuit'] * 1000, 'r-', label='Circuit current')
ax2.set_xlabel('Time (ms)')
ax2.set_ylabel('Current (mA)')
ax2.legend()
ax2.grid(True, alpha=0.3)

# Add theoretical response for comparison
# For series RLC: ω₀ = 1/√(LC), ζ = R/2 * √(C/L)
L = 10e-3  # 10mH
C = 100e-6  # 100µF
R = 100  # 100Ω
omega_0 = 1 / np.sqrt(L * C)
zeta = R / 2 * np.sqrt(C / L)

print(f"Natural frequency: {omega_0/(2*np.pi):.1f} Hz")
print(f"Damping ratio: {zeta:.3f}")
print(f"Circuit is: ", end="")
if zeta < 1:
    print("Underdamped")
    omega_d = omega_0 * np.sqrt(1 - zeta**2)
    print(f"Damped frequency: {omega_d/(2*np.pi):.1f} Hz")
elif zeta == 1:
    print("Critically damped")
else:
    print("Overdamped")

plt.tight_layout()
plt.savefig('tests/outputs/perturbation_rlc_response.png', dpi=150)
plt.show()