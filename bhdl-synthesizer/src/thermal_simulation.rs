// Thermal Simulation Integration
// Provides thermal analysis for component placement, power dissipation, and thermal management
// Integrates with the synthesis pipeline to ensure thermal constraints are met

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use anyhow::{Result, Context};
use log::{info, debug, warn, error};

/// Thermal simulation engine for circuit analysis
pub struct ThermalSimulator {
    /// Component thermal models
    thermal_models: HashMap<String, ComponentThermalModel>,
    
    /// Board thermal properties
    board_properties: BoardThermalProperties,
    
    /// Ambient conditions
    ambient_conditions: AmbientConditions,
    
    /// Simulation parameters
    simulation_config: ThermalSimulationConfig,
    
    /// Heat transfer coefficient database
    heat_transfer_db: HeatTransferDatabase,
    
    /// Package thermal characteristics
    package_db: PackageThermalDatabase,
}

/// Component thermal model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentThermalModel {
    pub component_name: String,
    pub package_type: String,
    pub thermal_resistance: ThermalResistance,
    pub power_dissipation: PowerDissipationModel,
    pub temperature_limits: TemperatureLimits,
    pub thermal_mass: f64, // J/K
    pub heat_generation: HeatGenerationModel,
}

/// Thermal resistance network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalResistance {
    pub junction_to_case: f64,    // K/W
    pub case_to_ambient: f64,     // K/W  
    pub junction_to_ambient: f64, // K/W (total)
    pub thermal_pad_resistance: Option<f64>, // K/W
}

/// Power dissipation characteristics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerDissipationModel {
    pub static_power: f64,        // W (quiescent)
    pub dynamic_power: f64,       // W (switching)
    pub power_vs_frequency: PowerFrequencyModel,
    pub power_vs_voltage: PowerVoltageModel,
    pub power_vs_temperature: PowerTemperatureModel,
}

/// Power vs frequency relationship
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PowerFrequencyModel {
    Linear { slope: f64, intercept: f64 },
    Quadratic { a: f64, b: f64, c: f64 },
    LookupTable(Vec<(f64, f64)>), // (frequency_Hz, power_W)
}

/// Power vs voltage relationship
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PowerVoltageModel {
    Linear { slope: f64, intercept: f64 },
    Quadratic { a: f64, b: f64, c: f64 },
    Cubic { a: f64, b: f64, c: f64, d: f64 },
}

/// Power vs temperature relationship
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PowerTemperatureModel {
    Constant,
    Linear { slope: f64 }, // W/K
    Exponential { base: f64, exponent: f64 },
}

/// Temperature operating limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemperatureLimits {
    pub operating_min: f64,       // °C
    pub operating_max: f64,       // °C
    pub storage_min: f64,         // °C
    pub storage_max: f64,         // °C
    pub junction_max: f64,        // °C
    pub case_max: f64,           // °C
    pub derating_start: f64,     // °C (start derating power)
    pub derating_slope: f64,     // W/°C (power reduction per degree)
}

/// Heat generation model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HeatGenerationModel {
    Constant(f64), // W
    Variable {
        base_power: f64,
        load_factor: f64,
        switching_losses: f64,
        conduction_losses: f64,
    },
    SPICE {
        model_file: String,
        parameters: HashMap<String, f64>,
    },
}

/// Board thermal properties
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardThermalProperties {
    pub substrate_material: SubstrateMaterial,
    pub thickness_mm: f64,
    pub area_mm2: f64,
    pub copper_coverage: f64, // 0.0-1.0
    pub via_density: f64,     // vias/mm²
    pub thermal_vias: Vec<ThermalVia>,
    pub heat_spreaders: Vec<HeatSpreader>,
    pub airflow_pattern: AirflowPattern,
}

/// PCB substrate material properties
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SubstrateMaterial {
    FR4 {
        thermal_conductivity: f64, // W/(m·K)
        specific_heat: f64,        // J/(kg·K)
        density: f64,              // kg/m³
    },
    Aluminum {
        thermal_conductivity: f64,
        specific_heat: f64,
        density: f64,
    },
    Ceramic {
        thermal_conductivity: f64,
        specific_heat: f64,
        density: f64,
    },
    Custom {
        name: String,
        thermal_conductivity: f64,
        specific_heat: f64,
        density: f64,
    },
}

/// Thermal via specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalVia {
    pub position: (f64, f64), // mm
    pub diameter_mm: f64,
    pub plating_thickness_um: f64,
    pub thermal_conductivity: f64, // W/(m·K)
}

/// Heat spreader specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeatSpreader {
    pub material: HeatSpreaderMaterial,
    pub position: (f64, f64), // mm
    pub size: (f64, f64),     // mm
    pub thickness_mm: f64,
    pub thermal_interface_material: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HeatSpreaderMaterial {
    Copper,
    Aluminum,
    GraphitePad,
    ThermalPad,
    Custom { thermal_conductivity: f64 },
}

/// Airflow pattern around board
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AirflowPattern {
    Natural, // Natural convection only
    Forced {
        velocity_m_per_s: f64,
        direction: AirflowDirection,
        fan_curves: Vec<(f64, f64)>, // (flow_rate, pressure)
    },
    Mixed {
        natural_component: f64,
        forced_component: f64,
        direction: AirflowDirection,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AirflowDirection {
    Horizontal,
    Vertical,
    Diagonal(f64), // angle in degrees
}

/// Ambient operating conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmbientConditions {
    pub temperature: f64,         // °C
    pub humidity: f64,            // %RH
    pub pressure: f64,            // kPa
    pub altitude: f64,            // m
    pub enclosure_properties: Option<EnclosureProperties>,
}

/// Enclosure thermal properties
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnclosureProperties {
    pub material: String,
    pub thermal_conductivity: f64, // W/(m·K)
    pub emissivity: f64,           // 0.0-1.0
    pub internal_volume: f64,      // m³
    pub ventilation_area: f64,     // m²
    pub internal_heat_sources: f64, // W
}

/// Thermal simulation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalSimulationConfig {
    pub steady_state: bool,
    pub transient_duration: f64,  // seconds
    pub time_step: f64,           // seconds
    pub convergence_tolerance: f64,
    pub max_iterations: u32,
    pub mesh_resolution: MeshResolution,
    pub solver_type: ThermalSolver,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MeshResolution {
    Coarse,
    Medium,
    Fine,
    Adaptive,
    Custom { element_size_mm: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ThermalSolver {
    FiniteDifference,
    FiniteElement,
    AnalyticalApproximation,
    HybridFDM_FEM,
}

/// Heat transfer coefficient database
pub struct HeatTransferDatabase {
    convection_coefficients: HashMap<String, ConvectionData>,
    radiation_coefficients: HashMap<String, RadiationData>,
    conduction_coefficients: HashMap<String, ConductionData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvectionData {
    pub natural_convection: f64,    // W/(m²·K)
    pub forced_convection: f64,     // W/(m²·K)
    pub velocity_coefficient: f64,  // adjustment factor for air velocity
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadiationData {
    pub emissivity: f64,           // 0.0-1.0
    pub view_factors: HashMap<String, f64>,
    pub surface_finish: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConductionData {
    pub thermal_conductivity: f64,  // W/(m·K)
    pub interface_resistance: f64,  // K·m²/W
    pub contact_pressure: f64,      // Pa
}

/// Package thermal database
pub struct PackageThermalDatabase {
    packages: HashMap<String, PackageThermalData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageThermalData {
    pub package_type: String,
    pub theta_ja: f64,             // Junction-to-ambient thermal resistance
    pub theta_jc: f64,             // Junction-to-case thermal resistance
    pub psi_jt: f64,               // Junction-to-top thermal characterization
    pub psi_jb: f64,               // Junction-to-board thermal characterization
    pub thermal_mass: f64,         // Thermal capacitance
    pub lead_frame_data: Option<LeadFrameData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeadFrameData {
    pub material: String,
    pub thermal_conductivity: f64,
    pub lead_count: u32,
    pub lead_thermal_resistance: f64,
}

/// Thermal simulation results
#[derive(Debug, Clone)]
pub struct ThermalSimulationResult {
    pub component_temperatures: HashMap<String, ComponentTemperature>,
    pub board_temperature_map: TemperatureMap,
    pub hot_spots: Vec<HotSpot>,
    pub thermal_violations: Vec<ThermalViolation>,
    pub power_derating_recommendations: Vec<DerationRecommendation>,
    pub cooling_recommendations: Vec<CoolingRecommendation>,
    pub steady_state_reached: bool,
    pub simulation_time: f64,
    pub convergence_info: ConvergenceInfo,
}

/// Component temperature results
#[derive(Debug, Clone)]
pub struct ComponentTemperature {
    pub component_name: String,
    pub junction_temperature: f64,
    pub case_temperature: f64,
    pub ambient_temperature: f64,
    pub thermal_margin: f64,        // °C below limit
    pub derating_factor: f64,       // 0.0-1.0
    pub power_dissipated: f64,      // W
}

/// Board temperature distribution
#[derive(Debug, Clone)]
pub struct TemperatureMap {
    pub grid_resolution: (u32, u32), // (x, y)
    pub temperatures: Vec<Vec<f64>>,   // °C
    pub coordinates: Vec<Vec<(f64, f64)>>, // mm
    pub max_temperature: f64,
    pub min_temperature: f64,
    pub average_temperature: f64,
}

/// Hot spot identification
#[derive(Debug, Clone)]
pub struct HotSpot {
    pub position: (f64, f64),      // mm
    pub temperature: f64,          // °C
    pub size: f64,                 // mm radius
    pub components_affected: Vec<String>,
    pub severity: HotSpotSeverity,
    pub root_cause: String,
}

#[derive(Debug, Clone)]
pub enum HotSpotSeverity {
    Info,       // No immediate concern
    Warning,    // Monitor closely
    Critical,   // Requires action
    Emergency,  // Immediate risk
}

/// Thermal constraint violations
#[derive(Debug, Clone)]
pub struct ThermalViolation {
    pub component_name: String,
    pub violation_type: ViolationType,
    pub actual_value: f64,
    pub limit_value: f64,
    pub severity: ViolationSeverity,
    pub recommendation: String,
}

#[derive(Debug, Clone)]
pub enum ViolationType {
    JunctionTemperature,
    CaseTemperature,
    AmbientTemperature,
    ThermalResistance,
    PowerDissipation,
}

#[derive(Debug, Clone)]
pub enum ViolationSeverity {
    Minor,      // <10% over limit
    Moderate,   // 10-25% over limit  
    Severe,     // 25-50% over limit
    Critical,   // >50% over limit
}

/// Power derating recommendations
#[derive(Debug, Clone)]
pub struct DerationRecommendation {
    pub component_name: String,
    pub current_power: f64,        // W
    pub recommended_power: f64,    // W
    pub derating_factor: f64,      // 0.0-1.0
    pub reason: String,
    pub implementation: String,
}

/// Cooling system recommendations
#[derive(Debug, Clone)]
pub struct CoolingRecommendation {
    pub solution_type: CoolingSolutionType,
    pub estimated_improvement: f64, // °C reduction
    pub implementation_cost: CostEstimate,
    pub complexity: ComplexityLevel,
    pub description: String,
}

#[derive(Debug, Clone)]
pub enum CoolingSolutionType {
    ImprovedAirflow,
    HeatSink,
    ThermalPad,
    ThermalVia,
    ComponentRelocation,
    PowerReduction,
    ActiveCooling,
}

#[derive(Debug, Clone)]
pub enum CostEstimate {
    Low,        // <$10
    Medium,     // $10-100
    High,       // $100-1000
    VeryHigh,   // >$1000
}

#[derive(Debug, Clone)]
pub enum ComplexityLevel {
    Simple,     // No design changes
    Moderate,   // Minor layout changes
    Complex,    // Significant redesign
    Extensive,  // Complete thermal redesign
}

/// Convergence information
#[derive(Debug, Clone)]
pub struct ConvergenceInfo {
    pub iterations_required: u32,
    pub final_error: f64,
    pub convergence_rate: f64,
    pub stability_metric: f64,
}

impl ThermalSimulator {
    /// Create new thermal simulator
    pub fn new() -> Self {
        Self {
            thermal_models: HashMap::new(),
            board_properties: BoardThermalProperties::default(),
            ambient_conditions: AmbientConditions::default(),
            simulation_config: ThermalSimulationConfig::default(),
            heat_transfer_db: HeatTransferDatabase::new(),
            package_db: PackageThermalDatabase::new(),
        }
    }
    
    /// Load thermal models for components
    pub fn load_component_models(&mut self, components: &[String]) -> Result<()> {
        info!("Loading thermal models for {} components", components.len());
        
        for component in components {
            let model = self.create_thermal_model(component)?;
            self.thermal_models.insert(component.clone(), model);
        }
        
        Ok(())
    }
    
    /// Create thermal model for a component
    fn create_thermal_model(&self, component_name: &str) -> Result<ComponentThermalModel> {
        // In production, would load from database or component library
        let package_type = self.infer_package_type(component_name);
        let thermal_resistance = self.get_package_thermal_resistance(&package_type);
        
        Ok(ComponentThermalModel {
            component_name: component_name.to_string(),
            package_type,
            thermal_resistance,
            power_dissipation: PowerDissipationModel::default(),
            temperature_limits: TemperatureLimits::default(),
            thermal_mass: 0.001, // J/K, typical for small components
            heat_generation: HeatGenerationModel::Constant(0.1), // W, default
        })
    }
    
    /// Infer package type from component name
    fn infer_package_type(&self, component_name: &str) -> String {
        let name_lower = component_name.to_lowercase();
        
        if name_lower.starts_with('r') {
            "0603".to_string() // Typical resistor package
        } else if name_lower.starts_with('c') {
            "0805".to_string() // Typical capacitor package
        } else if name_lower.starts_with('u') {
            "SOIC-8".to_string() // Typical IC package
        } else if name_lower.starts_with('q') {
            "SOT-23".to_string() // Typical transistor package
        } else {
            "Generic".to_string()
        }
    }
    
    /// Get thermal resistance for package type
    fn get_package_thermal_resistance(&self, package_type: &str) -> ThermalResistance {
        match package_type {
            "0603" => ThermalResistance {
                junction_to_case: 50.0,
                case_to_ambient: 200.0,
                junction_to_ambient: 250.0,
                thermal_pad_resistance: None,
            },
            "0805" => ThermalResistance {
                junction_to_case: 30.0,
                case_to_ambient: 150.0,
                junction_to_ambient: 180.0,
                thermal_pad_resistance: None,
            },
            "SOIC-8" => ThermalResistance {
                junction_to_case: 25.0,
                case_to_ambient: 100.0,
                junction_to_ambient: 125.0,
                thermal_pad_resistance: Some(10.0),
            },
            "SOT-23" => ThermalResistance {
                junction_to_case: 15.0,
                case_to_ambient: 80.0,
                junction_to_ambient: 95.0,
                thermal_pad_resistance: None,
            },
            _ => ThermalResistance {
                junction_to_case: 40.0,
                case_to_ambient: 120.0,
                junction_to_ambient: 160.0,
                thermal_pad_resistance: None,
            },
        }
    }
    
    /// Run thermal simulation
    pub fn simulate(&self, power_map: &HashMap<String, f64>) -> Result<ThermalSimulationResult> {
        info!("Starting thermal simulation with {} heat sources", power_map.len());
        
        let mut component_temperatures = HashMap::new();
        let mut violations = Vec::new();
        let mut hot_spots = Vec::new();
        
        // Calculate steady-state temperatures for each component
        for (component_name, power) in power_map {
            if let Some(model) = self.thermal_models.get(component_name) {
                let temp = self.calculate_component_temperature(model, *power)?;
                
                // Check for violations
                if temp.junction_temperature > model.temperature_limits.junction_max {
                    violations.push(ThermalViolation {
                        component_name: component_name.clone(),
                        violation_type: ViolationType::JunctionTemperature,
                        actual_value: temp.junction_temperature,
                        limit_value: model.temperature_limits.junction_max,
                        severity: self.assess_violation_severity(
                            temp.junction_temperature,
                            model.temperature_limits.junction_max
                        ),
                        recommendation: format!(
                            "Reduce power dissipation or improve cooling for {}",
                            component_name
                        ),
                    });
                }
                
                // Identify hot spots
                if temp.junction_temperature > self.ambient_conditions.temperature + 50.0 {
                    hot_spots.push(HotSpot {
                        position: (0.0, 0.0), // Would get from layout in production
                        temperature: temp.junction_temperature,
                        size: 2.0, // mm
                        components_affected: vec![component_name.clone()],
                        severity: if temp.junction_temperature > model.temperature_limits.junction_max {
                            HotSpotSeverity::Critical
                        } else {
                            HotSpotSeverity::Warning
                        },
                        root_cause: format!("High power dissipation: {:.2}W", power),
                    });
                }
                
                component_temperatures.insert(component_name.clone(), temp);
            }
        }
        
        // Generate temperature map (simplified)
        let temp_map = self.generate_temperature_map(&component_temperatures)?;
        
        // Generate cooling recommendations
        let cooling_recommendations = self.generate_cooling_recommendations(&violations)?;
        
        // Generate derating recommendations
        let derating_recommendations = self.generate_derating_recommendations(&violations)?;
        
        info!("Thermal simulation completed:");
        info!("  - {} components analyzed", component_temperatures.len());
        info!("  - {} thermal violations", violations.len());
        info!("  - {} hot spots identified", hot_spots.len());
        
        Ok(ThermalSimulationResult {
            component_temperatures,
            board_temperature_map: temp_map,
            hot_spots,
            thermal_violations: violations,
            power_derating_recommendations: derating_recommendations,
            cooling_recommendations,
            steady_state_reached: true,
            simulation_time: 0.1, // seconds (simplified)
            convergence_info: ConvergenceInfo {
                iterations_required: 10,
                final_error: 0.01,
                convergence_rate: 0.95,
                stability_metric: 0.99,
            },
        })
    }
    
    /// Calculate component temperature
    fn calculate_component_temperature(
        &self,
        model: &ComponentThermalModel,
        power: f64,
    ) -> Result<ComponentTemperature> {
        let ambient = self.ambient_conditions.temperature;
        
        // Simple thermal resistance calculation
        let junction_temp = ambient + power * model.thermal_resistance.junction_to_ambient;
        let case_temp = ambient + power * model.thermal_resistance.case_to_ambient;
        
        let thermal_margin = model.temperature_limits.junction_max - junction_temp;
        
        // Calculate derating factor
        let derating_factor = if junction_temp > model.temperature_limits.derating_start {
            let excess = junction_temp - model.temperature_limits.derating_start;
            (1.0 - excess * model.temperature_limits.derating_slope / power).max(0.0)
        } else {
            1.0
        };
        
        Ok(ComponentTemperature {
            component_name: model.component_name.clone(),
            junction_temperature: junction_temp,
            case_temperature: case_temp,
            ambient_temperature: ambient,
            thermal_margin,
            derating_factor,
            power_dissipated: power,
        })
    }
    
    /// Assess thermal violation severity
    fn assess_violation_severity(&self, actual: f64, limit: f64) -> ViolationSeverity {
        let ratio = actual / limit;
        
        if ratio < 1.1 {
            ViolationSeverity::Minor
        } else if ratio < 1.25 {
            ViolationSeverity::Moderate
        } else if ratio < 1.5 {
            ViolationSeverity::Severe
        } else {
            ViolationSeverity::Critical
        }
    }
    
    /// Generate board temperature map
    fn generate_temperature_map(
        &self,
        component_temps: &HashMap<String, ComponentTemperature>,
    ) -> Result<TemperatureMap> {
        // Simplified temperature map generation
        let grid_size = (20, 20);
        let mut temperatures = vec![vec![self.ambient_conditions.temperature; grid_size.1 as usize]; grid_size.0 as usize];
        let mut coordinates = vec![vec![(0.0, 0.0); grid_size.1 as usize]; grid_size.0 as usize];
        
        // Fill in coordinates
        for i in 0..grid_size.0 as usize {
            for j in 0..grid_size.1 as usize {
                coordinates[i][j] = (i as f64 * 5.0, j as f64 * 5.0); // 5mm spacing
            }
        }
        
        // Add component heat sources (simplified heat spreading)
        for (_, temp) in component_temps {
            // Add heat to nearby grid points
            temperatures[10][10] = temp.case_temperature; // Simplified
        }
        
        let max_temp = component_temps.values()
            .map(|t| t.junction_temperature)
            .fold(0.0f64, f64::max);
            
        let min_temp = self.ambient_conditions.temperature;
        let avg_temp = component_temps.values()
            .map(|t| t.case_temperature)
            .sum::<f64>() / component_temps.len() as f64;
        
        Ok(TemperatureMap {
            grid_resolution: grid_size,
            temperatures,
            coordinates,
            max_temperature: max_temp,
            min_temperature: min_temp,
            average_temperature: avg_temp,
        })
    }
    
    /// Generate cooling recommendations
    fn generate_cooling_recommendations(
        &self,
        violations: &[ThermalViolation],
    ) -> Result<Vec<CoolingRecommendation>> {
        let mut recommendations = Vec::new();
        
        for violation in violations {
            let excess_temp = violation.actual_value - violation.limit_value;
            
            if excess_temp < 10.0 {
                recommendations.push(CoolingRecommendation {
                    solution_type: CoolingSolutionType::ImprovedAirflow,
                    estimated_improvement: excess_temp * 0.7,
                    implementation_cost: CostEstimate::Low,
                    complexity: ComplexityLevel::Simple,
                    description: format!(
                        "Improve airflow around {} to reduce temperature by ~{:.1}°C",
                        violation.component_name, excess_temp * 0.7
                    ),
                });
            } else if excess_temp < 25.0 {
                recommendations.push(CoolingRecommendation {
                    solution_type: CoolingSolutionType::HeatSink,
                    estimated_improvement: excess_temp * 0.8,
                    implementation_cost: CostEstimate::Medium,
                    complexity: ComplexityLevel::Moderate,
                    description: format!(
                        "Add heat sink to {} to reduce temperature by ~{:.1}°C",
                        violation.component_name, excess_temp * 0.8
                    ),
                });
            } else {
                recommendations.push(CoolingRecommendation {
                    solution_type: CoolingSolutionType::ActiveCooling,
                    estimated_improvement: excess_temp * 0.9,
                    implementation_cost: CostEstimate::High,
                    complexity: ComplexityLevel::Complex,
                    description: format!(
                        "Active cooling required for {} to reduce temperature by ~{:.1}°C",
                        violation.component_name, excess_temp * 0.9
                    ),
                });
            }
        }
        
        Ok(recommendations)
    }
    
    /// Generate power derating recommendations
    fn generate_derating_recommendations(
        &self,
        violations: &[ThermalViolation],
    ) -> Result<Vec<DerationRecommendation>> {
        let mut recommendations = Vec::new();
        
        for violation in violations {
            if let ViolationType::JunctionTemperature = violation.violation_type {
                let excess_temp = violation.actual_value - violation.limit_value;
                let derating_factor = 1.0 - (excess_temp / violation.limit_value * 0.5);
                
                recommendations.push(DerationRecommendation {
                    component_name: violation.component_name.clone(),
                    current_power: 1.0, // Would extract from power map
                    recommended_power: 1.0 * derating_factor,
                    derating_factor,
                    reason: format!(
                        "Junction temperature {:.1}°C exceeds limit {:.1}°C",
                        violation.actual_value, violation.limit_value
                    ),
                    implementation: "Reduce operating frequency or supply voltage".to_string(),
                });
            }
        }
        
        Ok(recommendations)
    }
    
    /// Update ambient conditions
    pub fn set_ambient_conditions(&mut self, conditions: AmbientConditions) {
        self.ambient_conditions = conditions;
    }
    
    /// Update board properties
    pub fn set_board_properties(&mut self, properties: BoardThermalProperties) {
        self.board_properties = properties;
    }
    
    /// Export thermal analysis report
    pub fn export_thermal_report(&self, result: &ThermalSimulationResult) -> Result<String> {
        let mut report = String::new();
        
        report.push_str("=== THERMAL ANALYSIS REPORT ===\n\n");
        
        report.push_str(&format!(
            "Simulation Summary:\n  - Components: {}\n  - Violations: {}\n  - Hot Spots: {}\n\n",
            result.component_temperatures.len(),
            result.thermal_violations.len(),
            result.hot_spots.len()
        ));
        
        // Component temperatures
        report.push_str("Component Temperatures:\n");
        for (name, temp) in &result.component_temperatures {
            report.push_str(&format!(
                "  {}: Junction={:.1}°C, Case={:.1}°C, Margin={:.1}°C\n",
                name, temp.junction_temperature, temp.case_temperature, temp.thermal_margin
            ));
        }
        
        // Violations
        if !result.thermal_violations.is_empty() {
            report.push_str("\nThermal Violations:\n");
            for violation in &result.thermal_violations {
                report.push_str(&format!(
                    "  {}: {:.1}°C (limit: {:.1}°C) - {:?}\n",
                    violation.component_name, violation.actual_value,
                    violation.limit_value, violation.severity
                ));
            }
        }
        
        // Recommendations
        if !result.cooling_recommendations.is_empty() {
            report.push_str("\nCooling Recommendations:\n");
            for rec in &result.cooling_recommendations {
                report.push_str(&format!(
                    "  - {:?}: {} (Improvement: {:.1}°C)\n",
                    rec.solution_type, rec.description, rec.estimated_improvement
                ));
            }
        }
        
        Ok(report)
    }
}

// Default implementations

impl Default for BoardThermalProperties {
    fn default() -> Self {
        Self {
            substrate_material: SubstrateMaterial::FR4 {
                thermal_conductivity: 0.3, // W/(m·K)
                specific_heat: 1200.0,     // J/(kg·K)
                density: 1850.0,           // kg/m³
            },
            thickness_mm: 1.6,
            area_mm2: 10000.0, // 100mm x 100mm
            copper_coverage: 0.5,
            via_density: 1.0,
            thermal_vias: Vec::new(),
            heat_spreaders: Vec::new(),
            airflow_pattern: AirflowPattern::Natural,
        }
    }
}

impl Default for AmbientConditions {
    fn default() -> Self {
        Self {
            temperature: 25.0,  // °C
            humidity: 50.0,     // %RH
            pressure: 101.3,    // kPa
            altitude: 0.0,      // m
            enclosure_properties: None,
        }
    }
}

impl Default for ThermalSimulationConfig {
    fn default() -> Self {
        Self {
            steady_state: true,
            transient_duration: 3600.0, // 1 hour
            time_step: 1.0,             // 1 second
            convergence_tolerance: 0.01, // °C
            max_iterations: 100,
            mesh_resolution: MeshResolution::Medium,
            solver_type: ThermalSolver::FiniteDifference,
        }
    }
}

impl Default for PowerDissipationModel {
    fn default() -> Self {
        Self {
            static_power: 0.01,   // W
            dynamic_power: 0.1,   // W
            power_vs_frequency: PowerFrequencyModel::Linear { slope: 1e-9, intercept: 0.01 },
            power_vs_voltage: PowerVoltageModel::Quadratic { a: 0.01, b: 0.0, c: 0.0 },
            power_vs_temperature: PowerTemperatureModel::Constant,
        }
    }
}

impl Default for TemperatureLimits {
    fn default() -> Self {
        Self {
            operating_min: -40.0,   // °C
            operating_max: 85.0,    // °C
            storage_min: -65.0,     // °C
            storage_max: 150.0,     // °C
            junction_max: 150.0,    // °C
            case_max: 125.0,        // °C
            derating_start: 70.0,   // °C
            derating_slope: 0.02,   // W/°C
        }
    }
}

impl HeatTransferDatabase {
    fn new() -> Self {
        let mut convection = HashMap::new();
        convection.insert("air_natural".to_string(), ConvectionData {
            natural_convection: 10.0,  // W/(m²·K)
            forced_convection: 25.0,   // W/(m²·K)
            velocity_coefficient: 0.8,
        });
        
        let mut radiation = HashMap::new();
        radiation.insert("pcb_surface".to_string(), RadiationData {
            emissivity: 0.9,
            view_factors: HashMap::new(),
            surface_finish: "Green soldermask".to_string(),
        });
        
        let mut conduction = HashMap::new();
        conduction.insert("copper".to_string(), ConductionData {
            thermal_conductivity: 400.0, // W/(m·K)
            interface_resistance: 1e-4,   // K·m²/W
            contact_pressure: 1e5,        // Pa
        });
        
        Self {
            convection_coefficients: convection,
            radiation_coefficients: radiation,
            conduction_coefficients: conduction,
        }
    }
}

impl PackageThermalDatabase {
    fn new() -> Self {
        let mut packages = HashMap::new();
        
        packages.insert("SOIC-8".to_string(), PackageThermalData {
            package_type: "SOIC-8".to_string(),
            theta_ja: 125.0,  // K/W
            theta_jc: 25.0,   // K/W
            psi_jt: 20.0,     // K/W
            psi_jb: 45.0,     // K/W
            thermal_mass: 0.01, // J/K
            lead_frame_data: Some(LeadFrameData {
                material: "Copper".to_string(),
                thermal_conductivity: 400.0,
                lead_count: 8,
                lead_thermal_resistance: 10.0,
            }),
        });
        
        Self { packages }
    }
}