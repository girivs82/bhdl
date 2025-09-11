# Intelligent Passive Component Synthesis Implementation Plan

## Overview

This document details the implementation of intelligent passive component synthesis in BHDL, enabling virtual pins to automatically select appropriate resistor and capacitor parameters based on voltage domains, current requirements, and design intent.

## Problem Statement

### Current Limitations
- Virtual pin expansion uses generic `module_id` placeholders
- No voltage-aware passive component selection
- Missing integration with stdlib electrical parameters
- Components selected without considering power dissipation or voltage ratings
- Potential for overspecified (expensive) or underspecified (unsafe) components

### Real-World Requirements
- **Resistor Power Ratings**: 0402 (62.5mW) → 2512 (2W) based on I²R calculations
- **Capacitor Voltage Ratings**: 6.3V → 100V+ based on operating voltage + safety margin
- **Package Selection**: Driven by power dissipation and voltage requirements
- **Dielectric Selection**: C0G for precision, X7R for general use, X5R for high capacitance

## Solution Architecture

### 1. Intent-Driven Parameter Extraction

The system extracts component requirements from three sources:

#### A. Power Domain Analysis
```rust
// From analyzer Pass 5 - Power Analysis
VCC_3V3 -> 3.3V @ 500mA
VCC_5V -> 5V @ 1A  
VCC_12V -> 12V @ 2A
```

#### B. Intent Parameters
```bhdl
for input_protection(12V, 2A)        // Voltage: 12V, Current: 2A
for signal_amplification(3dB, 1MHz)  // Low power, high frequency
for power_output_protection(800mA, 5V) // Medium power, 5V
```

#### C. Component Context
```rust
// Virtual pin context provides functional requirements
pin VOUT: virtual power out;     // Power delivery requirements
pin SIGNAL_OUT: virtual signal out; // Signal integrity requirements
```

### 2. Intelligent Component Selection Engine

#### A. Power Calculation Engine
```rust
pub struct PassiveComponentCalculator {
    safety_factors: SafetyFactors,
    temperature_derating: f64,
}

impl PassiveComponentCalculator {
    /// Calculate required resistor power rating with safety margins
    pub fn calculate_resistor_power_rating(
        &self,
        resistance: f64,
        current: f64,
        voltage: f64,
    ) -> PowerRating {
        let power_dissipated = current * current * resistance; // I²R
        let derated_power = power_dissipated / self.safety_factors.power_derating; // 70% derating
        self.select_next_standard_power_rating(derated_power)
    }
    
    /// Calculate required capacitor voltage rating with safety margins
    pub fn calculate_capacitor_voltage_rating(
        &self,
        operating_voltage: f64,
    ) -> VoltageRating {
        let safety_voltage = operating_voltage * self.safety_factors.voltage_safety_margin; // 2x margin
        self.select_next_standard_voltage_rating(safety_voltage)
    }
}
```

#### B. Package Selection Logic
```rust
pub struct PackageSelector;

impl PackageSelector {
    /// Select appropriate package size based on power and voltage
    pub fn select_resistor_package(
        power_rating: PowerRating,
        voltage_rating: VoltageRating,
    ) -> PackageSize {
        match (power_rating, voltage_rating) {
            (PowerRating::P62mW, VoltageRating::V50) => PackageSize::_0402,
            (PowerRating::P100mW, VoltageRating::V75) => PackageSize::_0603,
            (PowerRating::P125mW, VoltageRating::V150) => PackageSize::_0805,
            (PowerRating::P250mW, VoltageRating::V200) => PackageSize::_1206,
            (PowerRating::P500mW, _) => PackageSize::_2010,
            (PowerRating::P1W, _) => PackageSize::_2512,
            _ => PackageSize::THT, // Through-hole for high power
        }
    }
    
    /// Select capacitor package and dielectric based on requirements
    pub fn select_capacitor_specs(
        capacitance: f64,
        voltage_rating: VoltageRating,
        frequency_req: Option<f64>,
    ) -> (PackageSize, DielectricType) {
        let dielectric = match (capacitance, frequency_req) {
            (c, Some(f)) if c <= 100e-12 && f > 1e6 => DielectricType::C0G, // Ultra-stable
            (c, _) if c <= 10e-9 => DielectricType::X7R, // General purpose
            (c, _) if c <= 10e-6 => DielectricType::X5R, // High capacitance
            _ => DielectricType::Y5V, // Very high capacitance
        };
        
        let package = match (capacitance, voltage_rating) {
            (c, v) if c <= 100e-12 && v <= VoltageRating::V50 => PackageSize::_0402,
            (c, v) if c <= 1e-9 && v <= VoltageRating::V50 => PackageSize::_0603,
            (c, v) if c <= 100e-9 && v <= VoltageRating::V50 => PackageSize::_0805,
            (c, v) if v > VoltageRating::V50 => PackageSize::_1210, // High voltage
            _ => PackageSize::_1206, // Default
        };
        
        (package, dielectric)
    }
}
```

### 3. Enhanced Virtual Pin Expansion

#### A. Updated Module Variants Implementation
```rust
// Enhanced bhdl-synthesizer/src/module_variants.rs

impl ModuleVariantGenerator {
    /// Expand virtual pin with intelligent passive component selection
    fn expand_virtual_pin_with_intelligent_components(
        &mut self,
        netlist: &mut Netlist,
        module_id: ModuleId,
        pin_name: &str,
        pin_type: PinType,
        pin_direction: PinDirection,
        context: &VirtualPinContext,
        intent: &bhdl_common::IntentCall,
    ) -> Result<()> {
        
        // Extract requirements from context and intent
        let voltage_domain = context.voltage_domain;
        let current_requirement = self.extract_current_from_intent(intent);
        let frequency_requirement = self.extract_frequency_from_intent(intent);
        
        // Calculate component specifications
        let calculator = PassiveComponentCalculator::new();
        let package_selector = PackageSelector::new();
        
        match (pin_type, pin_direction) {
            (PinType::Power, PinDirection::Out) => {
                self.synthesize_power_output_components(
                    netlist, module_id, pin_name,
                    voltage_domain, current_requirement, &calculator, &package_selector
                )
            },
            (PinType::Signal, PinDirection::Out) => {
                self.synthesize_signal_output_components(
                    netlist, module_id, pin_name,
                    frequency_requirement, &calculator, &package_selector
                )
            },
            _ => Ok(()),
        }
    }
    
    /// Synthesize power output components with proper ratings
    fn synthesize_power_output_components(
        &mut self,
        netlist: &mut Netlist,
        module_id: ModuleId,
        pin_name: &str,
        voltage: f64,
        current: f64,
        calculator: &PassiveComponentCalculator,
        package_selector: &PackageSelector,
    ) -> Result<()> {
        
        // 1. Decoupling capacitor selection
        let cap_voltage_rating = calculator.calculate_capacitor_voltage_rating(voltage);
        let (cap_package, cap_dielectric) = package_selector.select_capacitor_specs(
            100e-9, // 100nF typical
            cap_voltage_rating,
            None, // No specific frequency requirement
        );
        
        let decoupling_cap_module = self.create_or_get_capacitor_module(
            100e-9, // 100nF
            cap_voltage_rating,
            cap_package,
            cap_dielectric,
        );
        
        let decoupling_cap_instance = netlist.add_instance(
            format!("{}_decoup", pin_name),
            decoupling_cap_module
        ).ok_or_else(|| anyhow::anyhow!("Failed to create decoupling capacitor"))?;
        
        // 2. Current limiting resistor selection
        let current_limit_resistance = voltage / (current * 0.1); // 10% current limit
        let resistor_power_rating = calculator.calculate_resistor_power_rating(
            current_limit_resistance,
            current * 0.1,
            voltage,
        );
        let resistor_package = package_selector.select_resistor_package(
            resistor_power_rating,
            calculator.calculate_resistor_voltage_rating(voltage),
        );
        
        let current_limiter_module = self.create_or_get_resistor_module(
            current_limit_resistance,
            resistor_power_rating,
            resistor_package,
        );
        
        let current_limiter_instance = netlist.add_instance(
            format!("{}_ilimit", pin_name),
            current_limiter_module
        ).ok_or_else(|| anyhow::anyhow!("Failed to create current limiter"))?;
        
        // 3. Create nets and connections
        // ... (implement proper net creation and connection logic)
        
        info!("Created power output chain for '{}': {}V/{}A, cap: {:?}/{:?}, resistor: {:.1}Ω/{:?}", 
              pin_name, voltage, current, cap_package, cap_dielectric, 
              current_limit_resistance, resistor_package);
        
        Ok(())
    }
}
```

#### B. Component Module Creation
```rust
impl ModuleVariantGenerator {
    /// Create or retrieve a resistor module with specific parameters
    fn create_or_get_resistor_module(
        &mut self,
        resistance: f64,
        power_rating: PowerRating,
        package: PackageSize,
    ) -> ModuleId {
        // Check if module already exists
        let module_signature = format!("Resistor_{}Ω_{:?}_{:?}", resistance, power_rating, package);
        
        if let Some(existing_module) = self.component_module_cache.get(&module_signature) {
            return *existing_module;
        }
        
        // Create new resistor module with stdlib parameters
        let resistor_params = self.get_stdlib_resistor_params(package);
        let module_id = self.create_resistor_module_from_stdlib(
            module_signature.clone(),
            resistance,
            resistor_params,
        );
        
        // Cache for reuse
        self.component_module_cache.insert(module_signature, module_id);
        module_id
    }
    
    /// Get stdlib resistor parameters for package size
    fn get_stdlib_resistor_params(&self, package: PackageSize) -> &ResistorParams {
        match package {
            PackageSize::_0402 => &RESISTOR_0402_PARAMS,
            PackageSize::_0603 => &RESISTOR_0603_PARAMS,
            PackageSize::_0805 => &RESISTOR_0805_PARAMS,
            PackageSize::_1206 => &RESISTOR_1206_PARAMS,
            _ => &RESISTOR_0805_PARAMS, // Default
        }
    }
}
```

### 4. Stdlib Integration

#### A. Enhanced Electrical Parameters
```bhdl
// bhdl-stdlib/electrical_params.bhdl additions

// Package-specific resistor parameter templates
const RESISTOR_PACKAGES: map<string, ResistorParams> = {
    "0402": {
        power_rating: 62.5mW,
        max_voltage: 50V,
        tolerance: 5%,
        temp_coefficient: 100ppm,
        package_size: "0402"
    },
    "0603": {
        power_rating: 100mW,
        max_voltage: 75V,
        tolerance: 5%,
        temp_coefficient: 100ppm,
        package_size: "0603"
    },
    // ... more packages
};

// Capacitor dielectric specifications
const CAPACITOR_DIELECTRICS: map<string, CapacitorParams> = {
    "C0G": {
        temp_coefficient: "±30ppm/°C",
        max_capacitance: 100nF,
        voltage_linearity: "excellent",
        frequency_stable: true
    },
    "X7R": {
        temp_coefficient: "±15%",
        max_capacitance: 10μF,
        voltage_linearity: "good",
        frequency_stable: true
    },
    // ... more dielectrics
};
```

### 5. Implementation Phases

#### Phase 1: Core Calculation Engine (Week 1)
- [ ] Implement `PassiveComponentCalculator` 
- [ ] Implement `PackageSelector`
- [ ] Add safety factor configurations
- [ ] Unit tests for power/voltage calculations

#### Phase 2: Enhanced Virtual Pin Expansion (Week 2)
- [ ] Update `expand_virtual_pin_with_intent()` method
- [ ] Add `VirtualPinContext` with voltage/current extraction
- [ ] Implement component module creation and caching
- [ ] Integration tests with different voltage domains

#### Phase 3: Stdlib Integration (Week 3)
- [ ] Enhance electrical parameters with package specifications
- [ ] Create component parameter lookup functions
- [ ] Update component module templates
- [ ] Comprehensive testing with real circuits

#### Phase 4: Database Integration (Week 4)
- [ ] Map calculated specs to real components in database
- [ ] Implement component availability checking
- [ ] Add cost optimization for component selection
- [ ] Performance optimization and caching

### 6. Expected Results

#### Before (Current):
```
Virtual pin expansion → Generic module_id placeholders
All filters → Same generic recommendations
No voltage/power consideration
```

#### After (Intelligent):
```
3.3V/20mA signal → 0603 components, 6.3V caps, 125mW resistors
5V/800mA power → 1206 components, 16V caps, 500mW resistors  
12V/2A motor → 2010+ components, 35V caps, 1W+ resistors
```

### 7. Validation Strategy

#### A. Test Cases
1. **Low Power Digital (3.3V, <100mA)**: Should select 0603/0805, standard ratings
2. **Medium Power Analog (5V, 500mA)**: Should select 1206, higher power ratings  
3. **High Power Motor Drive (12V, 2A)**: Should select 2010+, high voltage/power ratings
4. **High Frequency Signal**: Should select C0G capacitors, low inductance packages

#### B. Safety Validation
- Verify all power ratings include 70% derating factor
- Verify all voltage ratings include 2x safety margin
- Check temperature derating for automotive/industrial applications

#### C. Cost Optimization
- Prefer standard values and common packages
- Avoid overspecification when possible
- Balance safety margins with cost constraints

## Conclusion

This implementation transforms BHDL from using generic component placeholders to intelligent, context-aware passive component synthesis. The system leverages:

- **Real electrical analysis** from power domain and current calculations
- **Design intent** from flow-based intent specifications  
- **Industry-standard safety practices** with proper derating factors
- **Comprehensive component knowledge** from stdlib electrical parameters

This represents a significant advancement in hardware description languages, enabling designers to specify intent while having the tools automatically select appropriate component specifications.

## Next Steps

1. **Immediate**: Implement Phase 1 calculation engines
2. **Short-term**: Integrate with existing virtual pin expansion system
3. **Medium-term**: Full stdlib integration and database mapping
4. **Long-term**: Machine learning for optimal component selection based on design history