// Package Selection Engine
// Selects appropriate component packages based on power, voltage, and application requirements

use crate::passive_component_calculator::{
    PowerRating, VoltageRating, PackageSize, DielectricType
};

/// Component specifications combining electrical and physical requirements
#[derive(Debug, Clone)]
pub struct ComponentSpec {
    pub package: PackageSize,
    pub power_rating: PowerRating,
    pub voltage_rating: VoltageRating,
}

/// Capacitor specifications including dielectric selection
#[derive(Debug, Clone)]
pub struct CapacitorSpec {
    pub package: PackageSize,
    pub voltage_rating: VoltageRating,
    pub dielectric: DielectricType,
    pub capacitance: f64,
    pub tolerance: f64, // Percentage
}

/// Resistor specifications with full electrical parameters
#[derive(Debug, Clone)]
pub struct ResistorSpec {
    pub package: PackageSize,
    pub power_rating: PowerRating,
    pub voltage_rating: VoltageRating,
    pub resistance: f64,
    pub tolerance: f64, // Percentage
    pub temp_coefficient: f64, // ppm/°C
}

/// Application requirements for component selection
#[derive(Debug, Clone)]
pub struct ApplicationRequirements {
    /// Operating frequency (Hz) - affects dielectric selection
    pub frequency: Option<f64>,
    
    /// Temperature range - affects package and derating
    pub temperature_range: Option<(f64, f64)>, // (min, max) in °C
    
    /// Size constraints - prefers smaller packages when possible
    pub size_constraint: SizeConstraint,
    
    /// Cost sensitivity - affects component selection strategy
    pub cost_sensitivity: CostSensitivity,
    
    /// Precision requirements - affects tolerance and stability
    pub precision_requirement: PrecisionRequirement,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizeConstraint {
    Minimal,    // Use smallest possible package
    Standard,   // Use standard packages (0603-1206)
    Relaxed,    // Size not critical
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CostSensitivity {
    Critical,   // Minimize cost, use common values
    Standard,   // Balance cost and performance
    Premium,    // Performance over cost
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrecisionRequirement {
    Low,        // ±10-20% tolerance acceptable
    Standard,   // ±5% tolerance
    High,       // ±1% tolerance
    Precision,  // ±0.1% tolerance
}

impl Default for ApplicationRequirements {
    fn default() -> Self {
        Self {
            frequency: None,
            temperature_range: Some((0.0, 85.0)), // Commercial temperature range
            size_constraint: SizeConstraint::Standard,
            cost_sensitivity: CostSensitivity::Standard,
            precision_requirement: PrecisionRequirement::Standard,
        }
    }
}

/// Main package selection engine
pub struct PackageSelector;

impl PackageSelector {
    /// Create new package selector
    pub fn new() -> Self {
        Self
    }
    
    /// Select appropriate resistor specification
    pub fn select_resistor_spec(
        &self,
        resistance: f64,
        power_rating: PowerRating,
        voltage_rating: VoltageRating,
        requirements: &ApplicationRequirements,
    ) -> ResistorSpec {
        let package = self.select_resistor_package(power_rating, voltage_rating, requirements);
        let tolerance = self.select_resistor_tolerance(requirements.precision_requirement);
        let temp_coefficient = self.select_resistor_temp_coefficient(requirements.precision_requirement);
        
        ResistorSpec {
            package,
            power_rating,
            voltage_rating,
            resistance,
            tolerance,
            temp_coefficient,
        }
    }
    
    /// Select appropriate capacitor specification  
    pub fn select_capacitor_spec(
        &self,
        capacitance: f64,
        voltage_rating: VoltageRating,
        requirements: &ApplicationRequirements,
    ) -> CapacitorSpec {
        let dielectric = self.select_capacitor_dielectric(capacitance, requirements);
        let package = self.select_capacitor_package(capacitance, voltage_rating, dielectric, requirements);
        let tolerance = self.select_capacitor_tolerance(dielectric, requirements.precision_requirement);
        
        CapacitorSpec {
            package,
            voltage_rating,
            dielectric,
            capacitance,
            tolerance,
        }
    }
    
    /// Select resistor package based on power and voltage requirements
    fn select_resistor_package(
        &self,
        power_rating: PowerRating,
        voltage_rating: VoltageRating,
        requirements: &ApplicationRequirements,
    ) -> PackageSize {
        // Start with power-based package selection
        let power_package = match power_rating {
            PowerRating::P62mW => PackageSize::_0402,
            PowerRating::P100mW => PackageSize::_0603,
            PowerRating::P125mW => PackageSize::_0805,
            PowerRating::P250mW => PackageSize::_1206,
            PowerRating::P500mW => PackageSize::_2010,
            PowerRating::P1W | PowerRating::P2W => PackageSize::_2512,
            PowerRating::P5W | PowerRating::P10W => PackageSize::THT,
        };
        
        // Check voltage requirements - high voltage may need larger package
        let voltage_package = match voltage_rating {
            VoltageRating::V6_3 | VoltageRating::V10 | VoltageRating::V16 => power_package,
            VoltageRating::V25 | VoltageRating::V35 => {
                // 25V+ may need at least 0805 for creepage
                match power_package {
                    PackageSize::_0402 | PackageSize::_0603 => PackageSize::_0805,
                    other => other,
                }
            },
            VoltageRating::V50 | VoltageRating::V63 => {
                // 50V+ may need at least 1206 for creepage
                match power_package {
                    PackageSize::_0402 | PackageSize::_0603 | PackageSize::_0805 => PackageSize::_1206,
                    other => other,
                }
            },
            VoltageRating::V100 | VoltageRating::V200 => {
                // 100V+ may need 2010+ for safety
                match power_package {
                    PackageSize::_0402 | PackageSize::_0603 | PackageSize::_0805 | 
                    PackageSize::_1206 | PackageSize::_1210 => PackageSize::_2010,
                    other => other,
                }
            },
            VoltageRating::V400 | VoltageRating::V630 => PackageSize::THT, // High voltage needs THT
        };
        
        // Apply size constraints
        match requirements.size_constraint {
            SizeConstraint::Minimal => {
                // Try to use smallest possible package while meeting electrical requirements
                voltage_package
            },
            SizeConstraint::Standard => {
                // Prefer standard packages (0603-1206) unless requirements force otherwise
                match voltage_package {
                    PackageSize::_0402 => PackageSize::_0603, // Avoid very small
                    PackageSize::_2010 | PackageSize::_2512 => {
                        // Check if 1206 can meet power requirements
                        if power_rating <= PowerRating::P250mW {
                            PackageSize::_1206
                        } else {
                            voltage_package
                        }
                    },
                    other => other,
                }
            },
            SizeConstraint::Relaxed => {
                // Use larger packages for better thermal performance
                match voltage_package {
                    PackageSize::_0402 | PackageSize::_0603 => PackageSize::_0805,
                    PackageSize::_0805 => PackageSize::_1206,
                    other => other,
                }
            },
        }
    }
    
    /// Select capacitor package based on capacitance, voltage, and dielectric
    fn select_capacitor_package(
        &self,
        capacitance: f64,
        voltage_rating: VoltageRating,
        dielectric: DielectricType,
        requirements: &ApplicationRequirements,
    ) -> PackageSize {
        // Base package selection on capacitance and voltage
        let base_package = match (capacitance, voltage_rating) {
            // Very small capacitance (pF range)
            (c, v) if c <= 100e-12 && v <= VoltageRating::V50 => PackageSize::_0402,
            (c, v) if c <= 100e-12 && v <= VoltageRating::V100 => PackageSize::_0603,
            
            // Small capacitance (nF range)
            (c, v) if c <= 1e-9 && v <= VoltageRating::V50 => PackageSize::_0603,
            (c, v) if c <= 1e-9 && v <= VoltageRating::V100 => PackageSize::_0805,
            (c, v) if c <= 10e-9 && v <= VoltageRating::V50 => PackageSize::_0805,
            (c, v) if c <= 10e-9 && v <= VoltageRating::V100 => PackageSize::_1206,
            (c, v) if c <= 100e-9 && v <= VoltageRating::V50 => PackageSize::_1206,
            (c, v) if c <= 100e-9 && v <= VoltageRating::V100 => PackageSize::_1210,
            
            // Medium capacitance (μF range)
            (c, v) if c <= 1e-6 && v <= VoltageRating::V25 => PackageSize::_1206,
            (c, v) if c <= 1e-6 && v <= VoltageRating::V50 => PackageSize::_1210,
            (c, v) if c <= 10e-6 && v <= VoltageRating::V25 => PackageSize::_1210,
            (c, v) if c <= 10e-6 && v <= VoltageRating::V50 => PackageSize::_2010, // High capacitance
            
            // High voltage or high capacitance
            (c, v) if v >= VoltageRating::V100 => PackageSize::_1210, // High voltage needs larger package
            (c, _) if c >= 10e-6 => PackageSize::_1210, // High capacitance needs larger package
            
            // Default case
            _ => PackageSize::_0805,
        };
        
        // Adjust for dielectric limitations
        let dielectric_package = match dielectric {
            DielectricType::C0G => {
                // C0G has capacitance density limitations
                if capacitance > 10e-9 {
                    // Large C0G caps need bigger packages
                    match base_package {
                        PackageSize::_0402 | PackageSize::_0603 => PackageSize::_0805,
                        PackageSize::_0805 => PackageSize::_1206,
                        other => other,
                    }
                } else {
                    base_package
                }
            },
            DielectricType::X7R => base_package, // Good density, use base selection
            DielectricType::X5R | DielectricType::Y5V => {
                // High-K dielectrics can use smaller packages for same capacitance
                match base_package {
                    PackageSize::_1210 => PackageSize::_1206,
                    PackageSize::_1206 => PackageSize::_0805,
                    other => other,
                }
            },
        };
        
        // Apply size constraints
        match requirements.size_constraint {
            SizeConstraint::Minimal => dielectric_package,
            SizeConstraint::Standard => {
                // Avoid very small packages for manufacturability
                match dielectric_package {
                    PackageSize::_0402 => PackageSize::_0603,
                    other => other,
                }
            },
            SizeConstraint::Relaxed => {
                // Use larger packages for better thermal and mechanical stability
                match dielectric_package {
                    PackageSize::_0402 | PackageSize::_0603 => PackageSize::_0805,
                    other => other,
                }
            },
        }
    }
    
    /// Select capacitor dielectric based on capacitance and application
    fn select_capacitor_dielectric(
        &self,
        capacitance: f64,
        requirements: &ApplicationRequirements,
    ) -> DielectricType {
        // Check precision requirements first
        if requirements.precision_requirement == PrecisionRequirement::Precision {
            if capacitance <= DielectricType::C0G.max_capacitance() {
                return DielectricType::C0G; // Best stability for precision
            }
        }
        
        // Check frequency requirements
        if let Some(frequency) = requirements.frequency {
            if frequency > 10e6 && capacitance <= DielectricType::C0G.max_capacitance() {
                return DielectricType::C0G; // Best for high frequency
            } else if frequency > 1e6 && capacitance <= DielectricType::X7R.max_capacitance() {
                return DielectricType::X7R; // Good for moderate high frequency
            }
        }
        
        // Select based on capacitance range and cost sensitivity
        match (capacitance, requirements.cost_sensitivity) {
            // Very low capacitance - use C0G for stability
            (c, _) if c <= 100e-12 => DielectricType::C0G,
            
            // Low capacitance - prefer C0G or X7R
            (c, CostSensitivity::Premium) if c <= 10e-9 => DielectricType::C0G,
            (c, _) if c <= 10e-9 => DielectricType::X7R,
            
            // Medium capacitance - X7R is typical choice
            (c, _) if c <= 1e-6 => DielectricType::X7R,
            
            // High capacitance - X5R for better density
            (c, CostSensitivity::Critical) if c <= 100e-6 => DielectricType::Y5V, // Cost-critical
            (c, _) if c <= 10e-6 => DielectricType::X5R,
            
            // Very high capacitance - Y5V for maximum density
            _ => DielectricType::Y5V,
        }
    }
    
    /// Select resistor tolerance based on precision requirements
    fn select_resistor_tolerance(&self, precision: PrecisionRequirement) -> f64 {
        match precision {
            PrecisionRequirement::Low => 10.0,        // ±10%
            PrecisionRequirement::Standard => 5.0,    // ±5%
            PrecisionRequirement::High => 1.0,        // ±1%
            PrecisionRequirement::Precision => 0.1,   // ±0.1%
        }
    }
    
    /// Select resistor temperature coefficient based on precision requirements
    fn select_resistor_temp_coefficient(&self, precision: PrecisionRequirement) -> f64 {
        match precision {
            PrecisionRequirement::Low => 200.0,       // ±200 ppm/°C
            PrecisionRequirement::Standard => 100.0,  // ±100 ppm/°C
            PrecisionRequirement::High => 50.0,       // ±50 ppm/°C
            PrecisionRequirement::Precision => 25.0,  // ±25 ppm/°C
        }
    }
    
    /// Select capacitor tolerance based on dielectric and precision requirements
    fn select_capacitor_tolerance(&self, dielectric: DielectricType, precision: PrecisionRequirement) -> f64 {
        match (dielectric, precision) {
            // C0G can achieve tight tolerances
            (DielectricType::C0G, PrecisionRequirement::Precision) => 1.0,  // ±1%
            (DielectricType::C0G, PrecisionRequirement::High) => 2.0,       // ±2%
            (DielectricType::C0G, _) => 5.0,                                // ±5%
            
            // X7R standard tolerances
            (DielectricType::X7R, PrecisionRequirement::High | PrecisionRequirement::Precision) => 10.0, // ±10%
            (DielectricType::X7R, _) => 20.0,                               // ±20%
            
            // X5R and Y5V typically looser tolerances
            (DielectricType::X5R | DielectricType::Y5V, _) => 20.0,         // ±20%
        }
    }
    
    /// Check if package can physically accommodate the required capacitance and voltage
    pub fn validate_capacitor_feasibility(
        &self,
        capacitance: f64,
        voltage_rating: VoltageRating,
        package: PackageSize,
        dielectric: DielectricType,
    ) -> bool {
        // Simplified feasibility check - in reality would use detailed manufacturer data
        match (package, dielectric) {
            // 0402 limitations
            (PackageSize::_0402, DielectricType::C0G) => {
                capacitance <= 1e-9 && voltage_rating <= VoltageRating::V50
            },
            (PackageSize::_0402, DielectricType::X7R) => {
                capacitance <= 100e-9 && voltage_rating <= VoltageRating::V25
            },
            
            // 0603 limitations  
            (PackageSize::_0603, DielectricType::C0G) => {
                capacitance <= 10e-9 && voltage_rating <= VoltageRating::V100
            },
            (PackageSize::_0603, DielectricType::X7R) => {
                capacitance <= 1e-6 && voltage_rating <= VoltageRating::V50
            },
            
            // Larger packages are generally feasible for most requirements
            _ => true,
        }
    }
    
    /// Estimate relative cost factor for component selection
    pub fn estimate_cost_factor(
        &self,
        package: PackageSize,
        tolerance: f64,
        dielectric: Option<DielectricType>,
    ) -> f64 {
        let mut cost_factor = 1.0;
        
        // Package size affects cost
        cost_factor *= match package {
            PackageSize::_0201 => 2.0,  // Very small is expensive
            PackageSize::_0402 => 1.5,  // Small premium
            PackageSize::_0603 | PackageSize::_0805 => 1.0, // Standard cost
            PackageSize::_1206 => 1.2,  // Slightly more expensive
            PackageSize::_1210 | PackageSize::_2010 | PackageSize::_2512 => 1.5, // Larger premium
            PackageSize::THT => 0.8,    // Through-hole can be cheaper
        };
        
        // Tolerance affects cost (tighter tolerance = higher cost)
        cost_factor *= if tolerance <= 1.0 {
            3.0  // Precision tolerance very expensive
        } else if tolerance <= 5.0 {
            1.5  // Tight tolerance moderately expensive
        } else {
            1.0  // Standard tolerance
        };
        
        // Dielectric affects cost
        if let Some(dielectric) = dielectric {
            cost_factor *= match dielectric {
                DielectricType::C0G => 2.0,  // Premium dielectric
                DielectricType::X7R => 1.0,  // Standard cost
                DielectricType::X5R => 0.9,  // Slightly cheaper
                DielectricType::Y5V => 0.8,  // Cheapest
            };
        }
        
        cost_factor
    }
}

impl Default for PackageSelector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_resistor_package_selection() {
        let selector = PackageSelector::new();
        let requirements = ApplicationRequirements::default();
        
        // Low power should select small package
        let spec = selector.select_resistor_spec(
            1000.0,
            PowerRating::P125mW,
            VoltageRating::V50,
            &requirements
        );
        assert_eq!(spec.package, PackageSize::_1206); // V50 requires larger package for creepage
        
        // High power should select large package
        let spec = selector.select_resistor_spec(
            100.0,
            PowerRating::P1W,
            VoltageRating::V50,
            &requirements
        );
        assert_eq!(spec.package, PackageSize::_2512);
    }
    
    #[test]
    fn test_capacitor_dielectric_selection() {
        let selector = PackageSelector::new();
        
        // Small capacitance should prefer C0G for precision
        let mut requirements = ApplicationRequirements::default();
        requirements.precision_requirement = PrecisionRequirement::High;
        
        let spec = selector.select_capacitor_spec(
            10e-12, // 10pF
            VoltageRating::V50,
            &requirements
        );
        assert_eq!(spec.dielectric, DielectricType::C0G);
        
        // Large capacitance should use X5R or Y5V
        let spec = selector.select_capacitor_spec(
            10e-6, // 10μF
            VoltageRating::V25,
            &requirements
        );
        assert!(matches!(spec.dielectric, DielectricType::X5R | DielectricType::Y5V));
    }
    
    #[test]
    fn test_high_voltage_package_upgrade() {
        let selector = PackageSelector::new();
        let requirements = ApplicationRequirements::default();
        
        // High voltage should upgrade package size for safety
        let spec = selector.select_resistor_spec(
            1000.0,
            PowerRating::P125mW, // Would normally be 0805
            VoltageRating::V100,  // High voltage upgrades to larger package
            &requirements
        );
        
        // Should upgrade to at least 1206 for high voltage
        assert!(matches!(spec.package, PackageSize::_1206 | PackageSize::_2010 | PackageSize::_2512 | PackageSize::THT));
    }
    
    #[test]
    fn test_cost_sensitivity() {
        let selector = PackageSelector::new();
        
        // Cost-critical should prefer Y5V for large capacitors
        let mut requirements = ApplicationRequirements::default();
        requirements.cost_sensitivity = CostSensitivity::Critical;
        
        let spec = selector.select_capacitor_spec(
            47e-6, // 47μF
            VoltageRating::V16,
            &requirements
        );
        assert_eq!(spec.dielectric, DielectricType::Y5V);
        
        // Premium should prefer better dielectrics
        requirements.cost_sensitivity = CostSensitivity::Premium;
        let spec = selector.select_capacitor_spec(
            1e-9, // 1nF
            VoltageRating::V50,
            &requirements
        );
        assert_eq!(spec.dielectric, DielectricType::C0G);
    }
}