//! Power domain analysis for BHDL circuit flow paradigm
//!
//! This module implements intelligent power management including:
//! - Power domain type system with voltage/current tracking
//! - Automatic level shifter insertion between domains
//! - Power sequencing logic generation
//! - Cross-domain signal validation

use crate::types::{AnalysisResult, SourceLocation};
use bhdl_ast::{SyntaxKind, BhdlLanguage, SyntaxNode};
use rowan::ast::AstNode;
use std::collections::{HashMap, HashSet};
use std::fmt;

/// Power domain information
#[derive(Debug, Clone, PartialEq)]
pub struct PowerDomain {
    /// Domain name (e.g., "VCC_3V3", "VCC_1V8", "USB_5V")
    pub name: String,
    /// Nominal voltage in volts
    pub voltage: f64,
    /// Voltage tolerance (±percentage)
    pub tolerance: f64,
    /// Maximum current capability in amperes
    pub max_current: f64,
    /// Power sequencing dependencies
    pub dependencies: Vec<String>,
    /// Whether this domain is always-on or can be controlled
    pub controllable: bool,
    /// Enable signal name for controllable domains
    pub enable_signal: Option<String>,
    /// Startup delay in milliseconds
    pub startup_delay_ms: f64,
    /// Power-on sequence priority (lower = earlier)
    pub sequence_priority: u32,
}

impl PowerDomain {
    /// Create a new power domain
    pub fn new(name: String, voltage: f64) -> Self {
        Self {
            name,
            voltage,
            tolerance: 5.0, // 5% default tolerance
            max_current: 1.0, // 1A default max current
            dependencies: Vec::new(),
            controllable: true,
            enable_signal: None,
            startup_delay_ms: 1.0, // 1ms default delay
            sequence_priority: 100, // Default priority
        }
    }

    /// Check if this domain is compatible with another voltage
    pub fn is_compatible_with(&self, other_voltage: f64) -> bool {
        let tolerance_range = self.voltage * (self.tolerance / 100.0);
        let min_voltage = self.voltage - tolerance_range;
        let max_voltage = self.voltage + tolerance_range;
        
        other_voltage >= min_voltage && other_voltage <= max_voltage
    }

    /// Check if level shifting is needed to connect to another domain
    pub fn needs_level_shifter(&self, target_domain: &PowerDomain) -> bool {
        !self.is_compatible_with(target_domain.voltage)
    }

    /// Get the appropriate level shifter type for connecting to target domain
    pub fn get_level_shifter_type(&self, target_domain: &PowerDomain) -> Option<LevelShifterType> {
        if !self.needs_level_shifter(target_domain) {
            return None;
        }

        match (self.voltage, target_domain.voltage) {
            // Common voltage domain translations
            (5.0, 3.3) => Some(LevelShifterType::Unidirectional { from: 5.0, to: 3.3 }),
            (3.3, 5.0) => Some(LevelShifterType::Unidirectional { from: 3.3, to: 5.0 }),
            (3.3, 1.8) => Some(LevelShifterType::Unidirectional { from: 3.3, to: 1.8 }),
            (1.8, 3.3) => Some(LevelShifterType::Unidirectional { from: 1.8, to: 3.3 }),
            (5.0, 1.8) => Some(LevelShifterType::Bidirectional { high: 5.0, low: 1.8 }),
            (1.8, 5.0) => Some(LevelShifterType::Bidirectional { high: 5.0, low: 1.8 }),
            _ => Some(LevelShifterType::Generic { 
                from: self.voltage, 
                to: target_domain.voltage 
            }),
        }
    }
}

/// Types of level shifters that can be automatically inserted
#[derive(Debug, Clone, PartialEq)]
pub enum LevelShifterType {
    /// Unidirectional level shifter
    Unidirectional { from: f64, to: f64 },
    /// Bidirectional level shifter
    Bidirectional { high: f64, low: f64 },
    /// Generic level shifter for unusual voltage combinations
    Generic { from: f64, to: f64 },
}

impl fmt::Display for LevelShifterType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LevelShifterType::Unidirectional { from, to } => {
                write!(f, "LevelShifter_{}V_to_{}V", from, to)
            }
            LevelShifterType::Bidirectional { high, low } => {
                write!(f, "BiDirLevelShifter_{}V_{}V", high, low)
            }
            LevelShifterType::Generic { from, to } => {
                write!(f, "GenericLevelShifter_{}V_to_{}V", from, to)
            }
        }
    }
}

/// Signal that needs level shifting
#[derive(Debug, Clone)]
pub struct LevelShiftedSignal {
    pub signal_name: String,
    pub source_domain: String,
    pub target_domain: String,
    pub shifter_type: LevelShifterType,
    pub location: SourceLocation,
}

/// Power sequencing step
#[derive(Debug, Clone)]
pub struct PowerSequenceStep {
    pub domain_name: String,
    pub action: PowerAction,
    pub delay_ms: f64,
    pub condition: Option<String>,
}

/// Power control actions
#[derive(Debug, Clone, PartialEq)]
pub enum PowerAction {
    Enable,
    Disable,
    WaitForStable,
    CheckVoltage,
}

/// Power analysis context
#[derive(Debug)]
pub struct PowerAnalysisContext {
    /// All power domains in the design
    pub domains: HashMap<String, PowerDomain>,
    /// Signals that need level shifting
    pub level_shifted_signals: Vec<LevelShiftedSignal>,
    /// Generated power sequence
    pub power_sequence: Vec<PowerSequenceStep>,
    /// Domain assignments for components
    pub component_domains: HashMap<String, String>,
    /// Analysis errors
    pub errors: Vec<PowerAnalysisError>,
    /// Analysis warnings
    pub warnings: Vec<String>,
}

/// Power analysis error types
#[derive(Debug, Clone)]
pub enum PowerAnalysisError {
    /// Unknown power domain referenced
    UnknownDomain {
        domain_name: String,
        location: SourceLocation,
    },
    /// Voltage incompatibility
    VoltageIncompatibility {
        signal: String,
        source_voltage: f64,
        target_voltage: f64,
        location: SourceLocation,
    },
    /// Circular power dependency
    CircularDependency {
        domains: Vec<String>,
        location: SourceLocation,
    },
    /// Insufficient current capability
    InsufficientCurrent {
        domain: String,
        required: f64,
        available: f64,
        location: SourceLocation,
    },
    /// Invalid power sequence
    InvalidSequence {
        message: String,
        location: SourceLocation,
    },
}

impl fmt::Display for PowerAnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PowerAnalysisError::UnknownDomain { domain_name, location } => {
                write!(f, "Unknown power domain '{}' at {}:{}", 
                       domain_name, location.line, location.column)
            }
            PowerAnalysisError::VoltageIncompatibility { signal, source_voltage, target_voltage, location } => {
                write!(f, "Voltage incompatibility for signal '{}': {}V -> {}V at {}:{}", 
                       signal, source_voltage, target_voltage, location.line, location.column)
            }
            PowerAnalysisError::CircularDependency { domains, location } => {
                write!(f, "Circular power dependency: {} at {}:{}", 
                       domains.join(" -> "), location.line, location.column)
            }
            PowerAnalysisError::InsufficientCurrent { domain, required, available, location } => {
                write!(f, "Insufficient current in domain '{}': required {}A, available {}A at {}:{}", 
                       domain, required, available, location.line, location.column)
            }
            PowerAnalysisError::InvalidSequence { message, location } => {
                write!(f, "Invalid power sequence: {} at {}:{}", 
                       message, location.line, location.column)
            }
        }
    }
}

impl PowerAnalysisContext {
    /// Create a new power analysis context
    pub fn new() -> Self {
        let mut context = Self {
            domains: HashMap::new(),
            level_shifted_signals: Vec::new(),
            power_sequence: Vec::new(),
            component_domains: HashMap::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
        };

        // Add common power domains
        context.add_standard_domains();
        context
    }

    /// Add standard power domains commonly used in designs
    fn add_standard_domains(&mut self) {
        // 5V USB power
        let mut usb_5v = PowerDomain::new("USB_5V".to_string(), 5.0);
        usb_5v.controllable = false; // Always-on when USB connected
        usb_5v.max_current = 0.5; // 500mA typical USB limit
        usb_5v.sequence_priority = 1; // First to come up
        self.domains.insert("USB_5V".to_string(), usb_5v);

        // 3.3V main rail
        let mut vcc_3v3 = PowerDomain::new("VCC_3V3".to_string(), 3.3);
        vcc_3v3.dependencies.push("USB_5V".to_string());
        vcc_3v3.max_current = 1.0; // 1A capability
        vcc_3v3.sequence_priority = 2;
        vcc_3v3.enable_signal = Some("VCC_3V3_EN".to_string());
        self.domains.insert("VCC_3V3".to_string(), vcc_3v3);

        // 1.8V low power rail
        let mut vcc_1v8 = PowerDomain::new("VCC_1V8".to_string(), 1.8);
        vcc_1v8.dependencies.push("VCC_3V3".to_string());
        vcc_1v8.max_current = 0.5; // 500mA capability
        vcc_1v8.sequence_priority = 3;
        vcc_1v8.enable_signal = Some("VCC_1V8_EN".to_string());
        self.domains.insert("VCC_1V8".to_string(), vcc_1v8);

        // Digital ground
        let mut gnd = PowerDomain::new("GND".to_string(), 0.0);
        gnd.controllable = false; // Always present
        gnd.max_current = 10.0; // High current capability
        gnd.sequence_priority = 0; // Always first
        self.domains.insert("GND".to_string(), gnd);
    }

    /// Add a custom power domain
    pub fn add_domain(&mut self, domain: PowerDomain) {
        self.domains.insert(domain.name.clone(), domain);
    }

    /// Get a power domain by name
    pub fn get_domain(&self, name: &str) -> Option<&PowerDomain> {
        self.domains.get(name)
    }

    /// Add a signal that needs level shifting
    pub fn add_level_shifted_signal(&mut self, signal: LevelShiftedSignal) {
        self.level_shifted_signals.push(signal);
    }

    /// Assign a component to a power domain
    pub fn assign_component_domain(&mut self, component: String, domain: String) {
        self.component_domains.insert(component, domain);
    }

    /// Check if two domains are voltage compatible
    pub fn are_domains_compatible(&self, domain1: &str, domain2: &str) -> bool {
        if let (Some(d1), Some(d2)) = (self.get_domain(domain1), self.get_domain(domain2)) {
            d1.is_compatible_with(d2.voltage)
        } else {
            false
        }
    }

    /// Generate power sequence based on domain dependencies
    pub fn generate_power_sequence(&mut self) -> Result<(), PowerAnalysisError> {
        // Check for circular dependencies
        self.check_circular_dependencies()?;

        // Sort domains by sequence priority
        let mut sorted_domains: Vec<_> = self.domains.values().collect();
        sorted_domains.sort_by_key(|d| d.sequence_priority);

        self.power_sequence.clear();

        // Generate enable sequence
        for domain in sorted_domains {
            if domain.controllable {
                // Add enable step
                self.power_sequence.push(PowerSequenceStep {
                    domain_name: domain.name.clone(),
                    action: PowerAction::Enable,
                    delay_ms: 0.0,
                    condition: None,
                });

                // Add delay if needed
                if domain.startup_delay_ms > 0.0 {
                    self.power_sequence.push(PowerSequenceStep {
                        domain_name: domain.name.clone(),
                        action: PowerAction::WaitForStable,
                        delay_ms: domain.startup_delay_ms,
                        condition: Some(format!("{}.stable", domain.name)),
                    });
                }
            }
        }

        Ok(())
    }

    /// Check for circular dependencies in power domains
    fn check_circular_dependencies(&self) -> Result<(), PowerAnalysisError> {
        for domain in self.domains.values() {
            let mut visited = HashSet::new();
            let mut path = Vec::new();
            if self.has_circular_dependency(&domain.name, &mut visited, &mut path) {
                return Err(PowerAnalysisError::CircularDependency {
                    domains: path,
                    location: SourceLocation::unknown(),
                });
            }
        }
        Ok(())
    }

    /// Recursive helper for circular dependency detection
    fn has_circular_dependency(
        &self,
        domain_name: &str,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> bool {
        if path.contains(&domain_name.to_string()) {
            path.push(domain_name.to_string());
            return true;
        }

        if visited.contains(domain_name) {
            return false;
        }

        visited.insert(domain_name.to_string());
        path.push(domain_name.to_string());

        if let Some(domain) = self.get_domain(domain_name) {
            for dep in &domain.dependencies {
                if self.has_circular_dependency(dep, visited, path) {
                    return true;
                }
            }
        }

        path.pop();
        false
    }

    /// Validate signal compatibility between domains
    pub fn validate_signal_compatibility(
        &mut self,
        signal_name: &str,
        source_domain: &str,
        target_domain: &str,
        location: SourceLocation,
    ) -> Result<(), PowerAnalysisError> {
        let source = self.get_domain(source_domain)
            .ok_or_else(|| PowerAnalysisError::UnknownDomain {
                domain_name: source_domain.to_string(),
                location: location.clone(),
            })?;

        let target = self.get_domain(target_domain)
            .ok_or_else(|| PowerAnalysisError::UnknownDomain {
                domain_name: target_domain.to_string(),
                location: location.clone(),
            })?;

        if source.needs_level_shifter(target) {
            // Clone the voltage values to avoid borrowing issues
            let source_voltage = source.voltage;
            let target_voltage = target.voltage;
            
            // Add level shifter requirement
            if let Some(shifter_type) = source.get_level_shifter_type(target) {
                self.add_level_shifted_signal(LevelShiftedSignal {
                    signal_name: signal_name.to_string(),
                    source_domain: source_domain.to_string(),
                    target_domain: target_domain.to_string(),
                    shifter_type,
                    location: location.clone(),
                });

                self.warnings.push(format!(
                    "Auto-inserting level shifter for signal '{}' from {}V to {}V",
                    signal_name, source_voltage, target_voltage
                ));
            } else {
                return Err(PowerAnalysisError::VoltageIncompatibility {
                    signal: signal_name.to_string(),
                    source_voltage,
                    target_voltage,
                    location,
                });
            }
        }

        Ok(())
    }

    /// Generate BHDL code for level shifters
    pub fn generate_level_shifter_code(&self) -> String {
        let mut code = String::new();
        
        if !self.level_shifted_signals.is_empty() {
            code.push_str("// Auto-generated level shifters\n");
            
            for signal in &self.level_shifted_signals {
                code.push_str(&format!(
                    "{}_{}_shifter: {} {{ \n",
                    signal.signal_name,
                    signal.target_domain.replace(".", "_"),
                    signal.shifter_type
                ));
                code.push_str(&format!(
                    "  // Level shift {} from {} to {}\n",
                    signal.signal_name, signal.source_domain, signal.target_domain
                ));
                code.push_str("};\n\n");
            }
        }

        code
    }

    /// Generate BHDL code for power sequencing
    pub fn generate_power_sequence_code(&self) -> String {
        let mut code = String::new();
        
        if !self.power_sequence.is_empty() {
            code.push_str("// Auto-generated power sequence\n");
            code.push_str("power_sequence {\n");
            
            for step in &self.power_sequence {
                match step.action {
                    PowerAction::Enable => {
                        if let Some(enable_signal) = self.domains.get(&step.domain_name)
                            .and_then(|d| d.enable_signal.as_ref()) {
                            code.push_str(&format!("  {}.enable();\n", enable_signal));
                        }
                    }
                    PowerAction::WaitForStable => {
                        if let Some(condition) = &step.condition {
                            code.push_str(&format!("  wait_for({});\n", condition));
                        } else {
                            code.push_str(&format!("  delay({}ms);\n", step.delay_ms));
                        }
                    }
                    _ => {}
                }
            }
            
            code.push_str("}\n\n");
        }

        code
    }

    /// Add an error to the analysis
    pub fn add_error(&mut self, error: PowerAnalysisError) {
        self.errors.push(error);
    }

    /// Add a warning to the analysis
    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    /// Check if there are any errors
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Perform power analysis on a syntax tree
pub fn analyze_power_domains(syntax: &SyntaxNode<BhdlLanguage>) -> PowerAnalysisContext {
    let mut context = PowerAnalysisContext::new();
    
    // Walk the syntax tree and identify power-related constructs
    visit_node_for_power_analysis(syntax, &mut context);
    
    // Generate power sequence
    if let Err(error) = context.generate_power_sequence() {
        context.add_error(error);
    }
    
    context
}

/// Visit nodes in the syntax tree for power analysis
fn visit_node_for_power_analysis(node: &SyntaxNode<BhdlLanguage>, context: &mut PowerAnalysisContext) {
    match node.kind() {
        SyntaxKind::COMPONENT_INST => {
            analyze_component_power_requirements(node, context);
        }
        SyntaxKind::FLOW_EXPR => {
            analyze_flow_power_domains(node, context);
        }
        SyntaxKind::BINARY_EXPR => {
            analyze_signal_connections(node, context);
        }
        _ => {}
    }

    // Recursively visit children
    for child in node.children() {
        visit_node_for_power_analysis(&child, context);
    }
}

/// Analyze power requirements for component instantiation
fn analyze_component_power_requirements(node: &SyntaxNode<BhdlLanguage>, context: &mut PowerAnalysisContext) {
    // Extract component type and parameters
    if let Some(ident_token) = node.first_token() {
        let component_type = ident_token.text();
        
        // Infer power domain based on component type
        let power_domain = match component_type {
            "STM32H7" | "STM32F4" => "VCC_3V3",
            "ESP32" => "VCC_3V3",
            "TPS63070" => "USB_5V", // Buck-boost converter
            "AMS1117" => "USB_5V",  // Linear regulator
            "SensorIC" => "VCC_1V8", // Low power sensor
            _ => "VCC_3V3", // Default to 3.3V
        };
        
        context.assign_component_domain(component_type.to_string(), power_domain.to_string());
    }
}

/// Analyze power domains in flow expressions
fn analyze_flow_power_domains(_node: &SyntaxNode<BhdlLanguage>, _context: &mut PowerAnalysisContext) {
    // TODO: Implement flow-specific power analysis
    // This would track power flow through the circuit
}

/// Analyze signal connections for power compatibility
fn analyze_signal_connections(_node: &SyntaxNode<BhdlLanguage>, _context: &mut PowerAnalysisContext) {
    // TODO: Implement signal connection power analysis
    // This would check voltage compatibility between connected pins
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_domain_compatibility() {
        let domain_3v3 = PowerDomain::new("VCC_3V3".to_string(), 3.3);
        let domain_5v = PowerDomain::new("USB_5V".to_string(), 5.0);
        
        assert!(domain_3v3.is_compatible_with(3.3));
        assert!(domain_3v3.is_compatible_with(3.2)); // Within tolerance
        assert!(!domain_3v3.is_compatible_with(5.0));
        assert!(domain_3v3.needs_level_shifter(&domain_5v));
    }

    #[test]
    fn test_level_shifter_selection() {
        let domain_3v3 = PowerDomain::new("VCC_3V3".to_string(), 3.3);
        let domain_5v = PowerDomain::new("USB_5V".to_string(), 5.0);
        
        let shifter = domain_3v3.get_level_shifter_type(&domain_5v);
        assert!(shifter.is_some());
        
        if let Some(LevelShifterType::Unidirectional { from, to }) = shifter {
            assert_eq!(from, 3.3);
            assert_eq!(to, 5.0);
        }
    }

    #[test]
    fn test_power_analysis_context() {
        let mut context = PowerAnalysisContext::new();
        
        // Should have standard domains
        assert!(context.get_domain("USB_5V").is_some());
        assert!(context.get_domain("VCC_3V3").is_some());
        assert!(context.get_domain("VCC_1V8").is_some());
        assert!(context.get_domain("GND").is_some());
        
        // Test domain compatibility
        assert!(!context.are_domains_compatible("USB_5V", "VCC_1V8"));
        assert!(context.are_domains_compatible("VCC_3V3", "VCC_3V3"));
    }

    #[test]
    fn test_power_sequence_generation() {
        let mut context = PowerAnalysisContext::new();
        
        // Should generate sequence without errors
        assert!(context.generate_power_sequence().is_ok());
        assert!(!context.power_sequence.is_empty());
        
        // Verify sequence order (USB_5V should be first)
        let first_controllable = context.power_sequence.iter()
            .find(|step| step.action == PowerAction::Enable);
        
        // Since USB_5V is not controllable, first should be VCC_3V3
        if let Some(step) = first_controllable {
            assert_eq!(step.domain_name, "VCC_3V3");
        }
    }

    #[test]
    fn test_circular_dependency_detection() {
        let mut context = PowerAnalysisContext::new();
        
        // Create circular dependency
        let mut domain_a = PowerDomain::new("A".to_string(), 3.3);
        domain_a.dependencies.push("B".to_string());
        
        let mut domain_b = PowerDomain::new("B".to_string(), 1.8);
        domain_b.dependencies.push("A".to_string());
        
        context.add_domain(domain_a);
        context.add_domain(domain_b);
        
        // Should detect circular dependency
        assert!(context.generate_power_sequence().is_err());
    }
}