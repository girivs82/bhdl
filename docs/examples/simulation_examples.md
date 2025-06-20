# BHDL Simulation Examples

## Real-World Validation Scenarios

### 1. Buck Converter Inductor Current Measurement

**Problem**: Board designers need to ensure inductor current stays within limits and doesn't saturate the core.

**Traditional Approach**: Build prototype, measure with oscilloscope, potentially damage components if design is wrong.

**BHDL Simulation Approach**:

```bhdl
testbench InductorValidation for BuckConverter {
    simulation {
        type: transient;
        duration: 5ms;
    }
    
    measure {
        IL_PEAK: max(current(L1));
        IL_RMS: rms(current(L1));
        IL_RIPPLE: peak_to_peak(current(L1));
    }
    
    assert {
        "Inductor below saturation": IL_PEAK < 3.5A;
        "RMS current within thermal limits": IL_RMS < 2.5A;
    }
    
    plot {
        waveform("inductor_current.svg") {
            current(L1);
            3.5A style="red,dashed";  // Saturation limit
            title: "Inductor Current vs Time";
        }
    }
}
```

**Benefits**: 
- Know exact peak current before building
- Size inductor correctly
- Verify CCM/DCM operation
- Check thermal limits

### 2. LED Driver Current Regulation

**Problem**: Ensure LED current is regulated properly across temperature and input voltage variations.

```bhdl
testbench LEDDriverValidation for LEDDriver {
    simulation {
        type: parameter_sweep;
        
        sweep {
            parameter: [VIN, temperature];
            VIN: [9V, 12V, 15V];
            temperature: [-20°C, 25°C, 85°C];
        }
    }
    
    measure {
        LED_CURRENT: current(LED1);
        LED_POWER: power(LED1);
    }
    
    assert {
        "LED current regulation ±5%": 
            LED_CURRENT in range(342mA, 358mA);  // 350mA ±5%
    }
    
    plot {
        heatmap("led_current_vs_conditions.svg") {
            x: VIN;
            y: temperature;
            z: LED_CURRENT;
            title: "LED Current vs Input Voltage and Temperature";
        }
    }
}
```

### 3. Power Supply Transient Response

**Problem**: Verify output voltage stays within spec during load transients.

```bhdl
testbench LoadTransientTest for PowerSupply {
    stimulus {
        // Sudden load change (CPU waking up)
        @1ms: load(VOUT) = 0.1A;
        @2ms: load(VOUT) = 2.5A;  // Step to full load
    }
    
    measure {
        VOUT_DIP: min(voltage(VOUT)) after 2ms;
        RECOVERY_TIME: settling_time(voltage(VOUT), 3.3V, 1%);
        OVERSHOOT: max(voltage(VOUT)) - 3.3V after 2ms;
    }
    
    assert {
        "Voltage dip < 100mV": (3.3V - VOUT_DIP) < 100mV;
        "Recovery < 50µs": RECOVERY_TIME < 50µs;
        "No overshoot": OVERSHOOT < 50mV;
    }
}
```

### 4. EMI Pre-Compliance Check

**Problem**: Estimate conducted emissions before EMC testing.

```bhdl
testbench EMIPreCompliance for SwitchingSupply {
    simulation {
        type: ac;
        start: 150kHz;  // CISPR 22 start frequency
        stop: 30MHz;
        points: 1000;
    }
    
    measure {
        // Conducted emissions on input
        LISN_VOLTAGE: voltage(LISN_50ohm);
        EMISSIONS_DBV: 20*log10(LISN_VOLTAGE/1µV);
    }
    
    plot {
        spectrum("conducted_emissions.svg") {
            EMISSIONS_DBV;
            cispr22_class_b_limit();  // Reference limit line
            title: "Estimated Conducted Emissions";
            ylabel: "dBµV";
        }
    }
}
```

### 5. Thermal Derating Analysis

**Problem**: Verify components stay within limits at high ambient temperature.

```bhdl
testbench ThermalAnalysis for PowerSupply {
    simulation {
        type: dc;
        temperature: sweep(25°C, 85°C, 5°C);
    }
    
    measure {
        // Power dissipation in key components
        R1_POWER: power(R1);
        Q1_POWER: power(Q1);
        TOTAL_POWER: power(total);
        
        // Estimated junction temperatures
        Q1_TJ: temperature + Q1_POWER * 62°C/W;  // RθJA
    }
    
    assert {
        "MOSFET junction temp": Q1_TJ < 150°C;
        "Resistor power derating": R1_POWER < 0.25W * (1 - (temperature-70°C)/80°C);
    }
}
```

### 6. Battery Life Estimation

**Problem**: Predict battery life under different operating conditions.

```bhdl
testbench BatteryLifeEstimation for PortableDevice {
    simulation {
        type: transient;
        duration: 1hour;
        
        // Define usage profile
        profile {
            repeat(60s) {
                @0s: mode = "active";     // 10s active
                @10s: mode = "idle";      // 40s idle  
                @50s: mode = "sleep";     // 10s sleep
            }
        }
    }
    
    measure {
        I_ACTIVE: mean(current(VBAT)) when mode == "active";
        I_IDLE: mean(current(VBAT)) when mode == "idle";
        I_SLEEP: mean(current(VBAT)) when mode == "sleep";
        
        // Average current over profile
        I_AVERAGE: mean(current(VBAT));
        
        // Battery life with 2000mAh battery
        BATTERY_LIFE: 2000mAh / I_AVERAGE;
    }
    
    report {
        "Active Current": I_ACTIVE;
        "Idle Current": I_IDLE;
        "Sleep Current": I_SLEEP;
        "Average Current": I_AVERAGE;
        "Estimated Battery Life": BATTERY_LIFE;
    }
}
```

## Benefits of Simulation

1. **Cost Savings**
   - Find issues before PCB fabrication
   - Reduce number of prototype iterations
   - Avoid component damage during testing

2. **Design Optimization**
   - Find optimal component values
   - Trade-off analysis (efficiency vs cost)
   - Margin analysis

3. **Documentation**
   - Simulation results as design validation
   - Waveforms for design reviews
   - Pass/fail criteria documented

4. **Risk Reduction**
   - Verify operation at extremes
   - Component stress analysis
   - What-if scenarios

5. **Time Savings**
   - Parallel validation of multiple designs
   - Automated regression testing
   - No need to wait for prototype assembly