# Automatic Validation in BHDL

## How Context-Aware Validation Works

When BHDL synthesizes a circuit, it automatically performs validation based on what it understands about the circuit. No manual testbench required for standard checks.

## Example: Buck Converter Automatic Validation

### User writes simple circuit:

```bhdl
board PowerSupply {
    power VIN = 12V @ 2A;
    power VOUT = 3.3V @ 1.5A;
    ground GND;
    
    // Buck converter using behavioral model
    buck: BuckConverterComplete(
        vin_nominal = 12V,
        vout = 3.3V,
        iout_max = 1.5A,
        fsw = 500kHz
    );
    
    VIN -> buck.VIN;
    buck.VOUT -> VOUT;
    buck.GND -> GND;
    
    // Or component-level implementation
    VIN -> L1: Inductor(4.7µH, current=2A).1;
    L1.2 -> sw_node -> D1: SchottkyDiode(SS34).cathode;
    D1.anode -> GND;
    sw_node -> C1: Cap(47µF, 16V, esr=50mΩ).1;
    C1.2 -> GND;
    sw_node -> VOUT;
}
```

### BHDL automatically validates during synthesis:

```rust
// In synthesizer
impl AutomaticValidator {
    fn validate_buck_converter(&self, circuit: &Circuit) -> ValidationReport {
        let mut report = ValidationReport::new();
        
        // 1. Detect this is a buck converter
        if let Some(buck) = self.detect_buck_topology(circuit) {
            
            // 2. Extract key parameters
            let vin = buck.input_voltage();
            let vout = buck.output_voltage();
            let iout_max = buck.max_output_current();
            let fsw = buck.switching_frequency();
            let l_value = buck.inductor.value;
            
            // 3. Calculate and validate ripple current
            let ripple_current = vout * (1.0 - vout/vin) / (l_value * fsw);
            let ripple_percent = ripple_current / iout_max * 100.0;
            
            if ripple_percent > 40.0 {
                report.add_warning(
                    "High inductor ripple current: {:.0}%\n\
                     Consider increasing inductor to {:.1}µH",
                    ripple_percent,
                    calculate_inductor_for_ripple(30.0)
                );
            }
            
            // 4. Validate inductor saturation
            let peak_current = iout_max + ripple_current / 2.0;
            if peak_current > buck.inductor.current_rating {
                report.add_error(
                    "Inductor saturation risk:\n\
                     Peak current: {:.2}A\n\
                     Inductor rating: {:.2}A\n\
                     Recommend inductor rated for {:.2}A",
                    peak_current,
                    buck.inductor.current_rating,
                    peak_current * 1.2
                );
            }
            
            // 5. Input capacitor RMS current
            let cin_rms = iout_max * sqrt(duty_cycle * (1.0 - duty_cycle));
            if let Some(cin) = buck.input_capacitor {
                if cin_rms > cin.ripple_current_rating {
                    report.add_error(
                        "Input capacitor RMS current exceeded:\n\
                         Required: {:.2}A\n\
                         Rating: {:.2}A",
                        cin_rms,
                        cin.ripple_current_rating
                    );
                }
            }
            
            // 6. Output capacitor ESR for stability
            if let Some(cout) = buck.output_capacitor {
                let esr_zero = 1.0 / (2.0 * PI * cout.esr * cout.value);
                let crossover_target = fsw / 10.0;
                
                if esr_zero > crossover_target * 2.0 {
                    report.add_warning(
                        "Output capacitor ESR may cause instability:\n\
                         ESR zero at {:.1}kHz\n\
                         Consider paralleling ceramic capacitors",
                        esr_zero / 1000.0
                    );
                }
            }
            
            // 7. MOSFET stress
            if let Some(mosfet) = buck.switching_mosfet {
                let vds_max = vin * 1.2; // 20% margin for spikes
                if vds_max > mosfet.vds_rating * 0.8 {
                    report.add_warning(
                        "MOSFET Vds close to limit:\n\
                         Expected max: {:.1}V\n\
                         Rating: {:.1}V\n\
                         Derate to 80% = {:.1}V",
                        vds_max,
                        mosfet.vds_rating,
                        mosfet.vds_rating * 0.8
                    );
                }
            }
            
            // 8. Thermal check
            let efficiency = self.estimate_efficiency(buck);
            let power_loss = (1.0 - efficiency) * vout * iout_max;
            let temp_rise = power_loss * buck.thermal_resistance;
            
            if temp_rise > 40.0 {
                report.add_warning(
                    "High temperature rise: {:.0}°C\n\
                     Power loss: {:.1}W\n\
                     Consider heat sinking or airflow",
                    temp_rise,
                    power_loss
                );
            }
            
            // 9. EMI pre-check
            let dv_dt = vin / (mosfet.rise_time);
            let emi_risk = self.estimate_emi_level(dv_dt, peak_current);
            
            if emi_risk > EMI_THRESHOLD {
                report.suggest(
                    "EMI mitigation suggested:\n\
                     - Add snubber: R={:.0}Ω, C={:.0}pF\n\
                     - Use shielded inductor\n\
                     - Add input filter",
                    calculate_snubber_r(fsw),
                    calculate_snubber_c(fsw)
                );
            }
        }
        
        report
    }
}
```

### Report generated automatically:

```
BHDL Synthesis Report - PowerSupply
===================================

Circuit Type: Buck Converter (12V -> 3.3V @ 1.5A)

✓ PASSED: Basic connectivity
✓ PASSED: Power domain consistency  
⚠ WARNING: High inductor ripple current (42%)
✗ ERROR: Input capacitor RMS rating exceeded

Detailed Analysis:
-----------------

Component Stress Analysis:
- L1: Peak current 1.82A (91% of 2A rating) ⚠
- C1: RMS current 0.75A (150% of 0.5A rating) ✗
- D1: Peak reverse voltage 12V (40% of 30V rating) ✓

Thermal Analysis:
- Estimated efficiency: 89%
- Power dissipation: 0.54W
- Junction temp rise: 33°C @ 25°C ambient ✓

Stability Analysis:
- Crossover frequency: ~50kHz (estimated)
- Phase margin: >45° (estimated) ✓
- Output cap ESR zero: 64kHz ✓

EMI Risk Assessment:
- dI/dt: 18A/µs (MEDIUM risk)
- dV/dt: 120V/µs (HIGH risk)
- Suggested: Add 10Ω/100pF snubber

Recommendations:
1. Increase L1 to 6.8µH to reduce ripple to 30%
2. Use low-ESR capacitor rated for 1A RMS ripple
3. Consider RC snubber for EMI reduction

Component Suggestions from Database:
- L1: Wurth 744325068 (6.8µH, 3.2A, 22mΩ)
- C1: Panasonic EEH-ZC1E470P (47µF, 25V, 1.4A RMS)
- Snubber: C0G 100pF + thick film 10Ω

Simulation Available:
Run 'bhdl simulate PowerSupply' for detailed waveforms
```

## Automatic Validation for Other Topologies

### LED Driver

```bhdl
board LEDDriver {
    power VIN = 12V;
    VIN -> led_driver: LEDDriverBehavioral(current=350mA);
    led_driver.LED+ -> LED1: LED(white, 3W).A;
    LED1.K -> led_driver.LED-;
}
```

Automatically validates:
- LED forward voltage vs available headroom
- Thermal derating of LED current
- Current sense resistor power rating
- PWM dimming frequency vs output cap

### Linear Regulator

```bhdl
board LinearSupply {
    power VIN = 5V;
    VIN -> reg: LM1117(3.3V).IN;
    reg.OUT -> VOUT;
    // caps...
}
```

Automatically validates:
- Dropout voltage at max current
- Power dissipation and thermal rise
- Input/output capacitor requirements
- Stability with ESR range

### Motor Driver

```bhdl
board MotorController {
    driver: DRV8825();
    driver.OUT1A -> motor.A+;
    driver.OUT1B -> motor.A-;
    // ...
}
```

Automatically validates:
- Motor current vs driver rating
- Back-EMF voltage vs driver abs max
- Thermal dissipation at stall current
- Current sense resistor value

## Benefits of Automatic Validation

1. **No Forgotten Checks**
   - System ensures all standard validations run
   - Catches common mistakes automatically

2. **Knowledge Built-In**
   - Best practices encoded in validators
   - Application notes integrated

3. **Component Database Integration**
   - Suggests real parts that meet requirements
   - Checks against actual specifications

4. **Progressive Enhancement**
   - Basic validation automatic
   - Add testbenches for specific scenarios
   - Override defaults when needed

5. **Documentation**
   - Validation report part of design docs
   - Traceable design decisions
   - Clear recommendations

## Custom Testbenches Still Available

For specific scenarios, users can still write testbenches:

```bhdl
// Test specific startup sequence
testbench CustomStartup for PowerSupply {
    scenario {
        // Your specific test case
        @0ms: VIN = 0V;
        @1ms: VIN = ramp_to(12V, 100ms);
        @2ms: enable = high;
    }
    
    verify {
        // Your specific requirements
        startup_time < 5ms;
        no_output_glitch;
    }
}
```

But for standard validation, it's automatic!