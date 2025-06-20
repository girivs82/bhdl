#!/usr/bin/env python3
"""
Buck Converter Behavioral Model for BHDL PLI
Full realistic implementation showing Python advantages
"""

import numpy as np
from enum import Enum
from dataclasses import dataclass
from typing import Dict, Any, Optional
import logging

# Assume we have bhdl_pli package
from bhdl_pli import BehavioralModel, Port, PortType

class ControllerState(Enum):
    """Buck controller operating states"""
    OFF = "OFF"
    SOFT_START = "SOFT_START" 
    REGULATION = "REGULATION"
    HICCUP = "HICCUP"
    FAULT = "FAULT"

@dataclass
class PIController:
    """PID controller with anti-windup"""
    kp: float = 0.1
    ki: float = 0.01
    kd: float = 0.001
    
    error_integral: float = 0.0
    last_error: float = 0.0
    integral_limit: float = 1.0
    
    def update(self, error: float, dt: float) -> float:
        """Update PID controller and return output"""
        # Proportional
        p_term = self.kp * error
        
        # Integral with anti-windup
        self.error_integral += error * dt
        self.error_integral = np.clip(self.error_integral, 
                                     -self.integral_limit, 
                                     self.integral_limit)
        i_term = self.ki * self.error_integral
        
        # Derivative with filtering
        if dt > 0:
            derivative = (error - self.last_error) / dt
            # Simple low-pass filter on derivative
            alpha = 0.1
            filtered_derivative = alpha * derivative + (1-alpha) * (self.last_error / dt)
            d_term = self.kd * filtered_derivative
        else:
            d_term = 0
            
        self.last_error = error
        
        return p_term + i_term + d_term
    
    def reset(self):
        """Reset controller state"""
        self.error_integral = 0.0
        self.last_error = 0.0

class BuckControllerBehavioral(BehavioralModel):
    """
    Behavioral model of buck converter controller with full features:
    - Soft start
    - PID regulation
    - Current limiting
    - Thermal protection
    - Hiccup mode OCP
    - Frequency synchronization
    """
    
    def __init__(self, params: Dict[str, Any]):
        """Initialize with parameters from BHDL"""
        super().__init__()
        
        # Parameters from BHDL
        self.vin_nom = params.get('vin_nom', 12.0)
        self.vout_target = params.get('vout_target', 3.3)
        self.iout_max = params.get('iout_max', 2.0)
        self.fsw = params.get('fsw', 500e3)
        self.l_value = params.get('l_value', 4.7e-6)
        self.cout_value = params.get('cout_value', 100e-6)
        self.cout_esr = params.get('cout_esr', 20e-3)
        
        # Control parameters
        self.soft_start_time = 10e-3
        self.current_limit = self.iout_max * 1.5
        self.thermal_limit = 150.0  # °C
        
        # State variables
        self.state = ControllerState.OFF
        self.soft_start_timer = 0.0
        self.hiccup_timer = 0.0
        self.overcurrent_count = 0
        self.thermal_shutdown = False
        
        # Electrical state
        self.inductor_current = 0.0
        self.vout_actual = 0.0
        self.capacitor_voltage = 0.0
        self.switch_node = 0.0
        self.junction_temp = 25.0
        
        # Control state
        self.duty_cycle = 0.0
        self.vref_internal = 0.0
        self.pid = PIController()
        
        # Port mapping
        self.ports = {
            'VIN': Port(PortType.POWER_IN),
            'VOUT': Port(PortType.POWER_OUT),
            'ENABLE': Port(PortType.DIGITAL_IN),
            'PGOOD': Port(PortType.DIGITAL_OUT_OD),
            'FREQ_SET': Port(PortType.ANALOG_IN, optional=True)
        }
        
        # Monitoring data
        self.monitor_data = {
            'controller_state': [],
            'duty_cycle': [],
            'inductor_current': [],
            'junction_temp': [],
            'vout_actual': [],
            'switch_node': []
        }
        
        # Logging
        self.logger = logging.getLogger(__name__)
        
    def step(self, time: float, dt: float) -> None:
        """Execute one simulation timestep"""
        # Read inputs
        vin = self.read_port('VIN')
        enable = self.read_port('ENABLE')
        vout_measured = self.read_port('VOUT')
        
        # State machine logic
        if self.state == ControllerState.OFF:
            self._handle_off_state(vin, enable)
            
        elif self.state == ControllerState.SOFT_START:
            self._handle_soft_start_state(dt)
            
        elif self.state == ControllerState.REGULATION:
            self._handle_regulation_state(dt)
            
        elif self.state == ControllerState.HICCUP:
            self._handle_hiccup_state(dt, enable)
            
        elif self.state == ControllerState.FAULT:
            self._handle_fault_state(enable)
        
        # Update electrical model
        self._update_electrical_model(vin, dt)
        
        # Update thermal model
        self._update_thermal_model(dt)
        
        # Write outputs
        self.write_port('VOUT', self.vout_actual + self.capacitor_voltage * self.cout_esr)
        
        # PGOOD logic
        if self.state == ControllerState.REGULATION:
            if 0.95 * self.vout_target <= self.vout_actual <= 1.05 * self.vout_target:
                self.write_port('PGOOD', None)  # High-Z for open drain
            else:
                self.write_port('PGOOD', 0)
        else:
            self.write_port('PGOOD', 0)
        
        # Store monitoring data
        self._update_monitoring(time)
        
    def _handle_off_state(self, vin: float, enable: bool):
        """Handle OFF state logic"""
        self.duty_cycle = 0.0
        self.vref_internal = 0.0
        self.pid.reset()
        
        if enable and vin > 4.5:  # UVLO at 4.5V
            self.logger.info("Starting soft-start sequence")
            self.state = ControllerState.SOFT_START
            self.soft_start_timer = 0.0
            self.overcurrent_count = 0
            
    def _handle_soft_start_state(self, dt: float):
        """Handle soft-start state with ramp"""
        # Ramp reference voltage
        self.soft_start_timer += dt
        progress = min(self.soft_start_timer / self.soft_start_time, 1.0)
        self.vref_internal = self.vout_target * progress
        
        # PID control
        error = self.vref_internal - self.vout_actual
        self.duty_cycle = self.pid.update(error, dt)
        self.duty_cycle = np.clip(self.duty_cycle, 0, 0.9)
        
        # Check for overcurrent
        if self.inductor_current > self.current_limit:
            self.overcurrent_count += 1
            if self.overcurrent_count > 10:
                self.logger.warning("Overcurrent during soft-start, entering hiccup mode")
                self.state = ControllerState.HICCUP
                self.hiccup_timer = 0.0
        else:
            self.overcurrent_count = max(0, self.overcurrent_count - 1)
        
        # Transition to regulation
        if self.vout_actual > 0.9 * self.vout_target:
            self.logger.info("Soft-start complete, entering regulation")
            self.state = ControllerState.REGULATION
            
    def _handle_regulation_state(self, dt: float):
        """Handle normal regulation with full PID"""
        # PID control
        error = self.vout_target - self.vout_actual
        self.duty_cycle = self.pid.update(error, dt)
        
        # Feedforward term
        vin = self.read_port('VIN')
        duty_ff = self.vout_target / vin if vin > 0 else 0
        
        # Combined control with feedforward
        self.duty_cycle = 0.8 * self.duty_cycle + 0.2 * duty_ff
        self.duty_cycle = np.clip(self.duty_cycle, 0, 0.95)
        
        # Protection monitoring
        if self.inductor_current > self.current_limit:
            self.overcurrent_count += 1
            if self.overcurrent_count > 10:
                self.logger.error("Overcurrent fault, entering hiccup mode")
                self.state = ControllerState.HICCUP
                self.hiccup_timer = 0.0
        else:
            self.overcurrent_count = max(0, self.overcurrent_count - 1)
        
        # Thermal protection
        if self.junction_temp > self.thermal_limit:
            self.logger.error(f"Thermal shutdown at {self.junction_temp:.1f}°C")
            self.state = ControllerState.FAULT
            self.thermal_shutdown = True
            
    def _handle_hiccup_state(self, dt: float, enable: bool):
        """Handle hiccup mode for overcurrent recovery"""
        self.duty_cycle = 0.0
        self.hiccup_timer += dt
        
        if self.hiccup_timer > 0.1:  # 100ms hiccup period
            if enable:
                self.logger.info("Attempting restart after hiccup")
                self.state = ControllerState.SOFT_START
                self.soft_start_timer = 0.0
                self.overcurrent_count = 0
            else:
                self.state = ControllerState.OFF
                
    def _handle_fault_state(self, enable: bool):
        """Handle latched fault state"""
        self.duty_cycle = 0.0
        
        # Only clear fault when disabled
        if not enable:
            self.logger.info("Clearing fault state")
            self.state = ControllerState.OFF
            self.thermal_shutdown = False
            
    def _update_electrical_model(self, vin: float, dt: float):
        """Update inductor current and output voltage"""
        # Switch node voltage (ideal switch for now)
        self.switch_node = self.duty_cycle * vin
        
        # Inductor current dynamics: V = L * di/dt
        v_inductor = self.switch_node - self.vout_actual
        di_dt = v_inductor / self.l_value
        self.inductor_current += di_dt * dt
        
        # DCM clamp (no reverse current through diode)
        self.inductor_current = max(0.0, self.inductor_current)
        
        # Estimate load current (from BHDL electrical sim)
        vout_port = self.read_port('VOUT')
        if vout_port > 0.1:
            load_current = self.vout_actual / (vout_port / self.inductor_current) if self.inductor_current > 0 else 0
        else:
            load_current = 0
        
        # Capacitor current
        i_capacitor = self.inductor_current - load_current
        
        # Output voltage dynamics: I = C * dv/dt
        dv_dt = i_capacitor / self.cout_value
        self.vout_actual += dv_dt * dt
        self.vout_actual = max(0.0, self.vout_actual)  # No negative voltage
        
        # ESR effect (voltage across ESR)
        self.capacitor_voltage = i_capacitor * self.cout_esr
        
    def _update_thermal_model(self, dt: float):
        """Simple thermal model"""
        # Power losses
        p_switch = self.inductor_current**2 * 0.05  # Rds_on = 50mΩ
        p_inductor = self.inductor_current**2 * 0.02  # DCR = 20mΩ
        p_total = p_switch + p_inductor
        
        # Thermal dynamics with time constant
        thermal_resistance = 40.0  # °C/W
        thermal_capacitance = 0.1  # J/°C
        
        temp_rise_ss = p_total * thermal_resistance
        tau = thermal_resistance * thermal_capacitance
        
        # First-order thermal response
        self.junction_temp += (temp_rise_ss - (self.junction_temp - 25.0)) * dt / tau
        
    def _update_monitoring(self, time: float):
        """Store data for waveform generation"""
        self.monitor_data['controller_state'].append((time, self.state.value))
        self.monitor_data['duty_cycle'].append((time, self.duty_cycle))
        self.monitor_data['inductor_current'].append((time, self.inductor_current))
        self.monitor_data['junction_temp'].append((time, self.junction_temp))
        self.monitor_data['vout_actual'].append((time, self.vout_actual))
        self.monitor_data['switch_node'].append((time, self.switch_node))
        
    def get_waveform_data(self) -> Dict[str, Any]:
        """Return monitoring data for plotting"""
        return self.monitor_data
    
    def get_measurements(self) -> Dict[str, float]:
        """Return key measurements for testbench"""
        # Calculate statistics from stored data
        if len(self.monitor_data['inductor_current']) > 100:
            il_data = np.array([d[1] for d in self.monitor_data['inductor_current'][-100:]])
            vout_data = np.array([d[1] for d in self.monitor_data['vout_actual'][-100:]])
            
            return {
                'il_peak': np.max(il_data),
                'il_valley': np.min(il_data),
                'il_ripple': np.max(il_data) - np.min(il_data),
                'il_avg': np.mean(il_data),
                'vout_ripple': np.max(vout_data) - np.min(vout_data),
                'efficiency_est': self._estimate_efficiency(),
                'max_temp': max(d[1] for d in self.monitor_data['junction_temp']),
                'final_duty': self.duty_cycle
            }
        return {}
    
    def _estimate_efficiency(self) -> float:
        """Estimate converter efficiency"""
        if self.inductor_current > 0:
            # Simple loss model
            p_switch = self.inductor_current**2 * 0.05
            p_inductor = self.inductor_current**2 * 0.02
            p_out = self.vout_actual * self.inductor_current * 0.9  # Assume 90% goes to load
            p_in = p_out + p_switch + p_inductor
            
            return (p_out / p_in * 100) if p_in > 0 else 0
        return 0