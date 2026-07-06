//! Power domain analysis for BHDL circuit flow paradigm
//!
//! This module implements intelligent power management including:
//! - Power domain type system with voltage/current tracking
//! - Automatic level shifter insertion between domains
//! - Power sequencing logic generation
//! - Cross-domain signal validation

use crate::types::SourceLocation;
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
    /// The per-rail load budget actually declared via `power X = V @ I`, or
    /// `None` when omitted. Distinct from `max_current` (a 1.0A estimate
    /// default). Real-Data Policy: an absent budget stays `None` (UNCHECKED
    /// downstream), never a fabricated value.
    pub declared_current: Option<f64>,
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
            declared_current: None, // Real-Data: only set when source declares `@ I`
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

/// A board-level boundary port (ports doctrine: power pins are not magic —
/// every board-level external connection is a top-level port). Recorded for
/// BOTH the explicit `port NAME: type dir [= spec];` form and the
/// `power X = V @ I;` / `ground X;` sugar, which desugars here: one record
/// shape, one lowering path into the netlist's Port objects.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardPortInfo {
    /// Port name == the boundary net's name.
    pub name: String,
    /// power | ground | signal.
    pub kind: BoardPortKind,
    /// Declared direction, or the type default (power → in,
    /// ground/signal → inout).
    pub direction: BoardPortDir,
    /// Declared rail voltage from the `= V @ I` spec (power ports).
    pub voltage: Option<f64>,
    /// Declared rail budget (`@ I`), None when the source omits it
    /// (Real-Data: never a fabricated default).
    pub current: Option<f64>,
    /// true for the explicit `port` spelling, false for the sugar.
    pub explicit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardPortKind {
    Power,
    Ground,
    Signal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardPortDir {
    In,
    Out,
    InOut,
}

/// Power analysis context
#[derive(Debug)]
pub struct PowerAnalysisContext {
    /// All power domains in the design
    pub domains: HashMap<String, PowerDomain>,
    /// Board-level boundary ports (explicit `port` decls + desugared
    /// power/ground decls), in source order.
    pub board_ports: Vec<BoardPortInfo>,
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
    /// Create a new power analysis context with standard domains pre-populated
    pub fn new() -> Self {
        let mut ctx = Self {
            domains: HashMap::new(),
            board_ports: Vec::new(),
            level_shifted_signals: Vec::new(),
            power_sequence: Vec::new(),
            component_domains: HashMap::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
        };
        // No standard-domain seeding. Power domains exist iff the
        // source declares one (`power FOO = X V;` or
        // `ground FOO;`) — they're net-name annotations referring
        // to actual on-board nets fed by real connectors or
        // regulators, not abstract floating "rails." Phantom
        // pre-population would create domains the source never
        // declared, which the synthesizer then materialises as
        // Instance records that don't correspond to any physical
        // component. See `load_power_domains_from_symbols` (this
        // file) for the source-driven population path. Discovery
        // documented during Phase H/I of the KiCad import work —
        // ambient seeding was creating phantom GND/USB_5V/
        // VCC_3V3/VCC_1V8 entries on schematics that declared
        // only their actual rails.
        ctx
    }

    /// Add a custom power domain
    pub fn add_domain(&mut self, domain: PowerDomain) {
        self.domains.insert(domain.name.clone(), domain);
    }
    
    /// Add a ground domain to the context
    pub fn add_ground_domain(&mut self, name: String) {
        // Ground domains are already added as 0V power domains
        // This method is for any special ground tracking if needed
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

/// Load power domains from symbol table
fn load_power_domains_from_symbols(context: &mut PowerAnalysisContext, symbol_table: &crate::symbol_table::SymbolTable) {
    use crate::symbol_table::SymbolKind;
    use crate::net_attributes::NetAttribute;
    
    // Check all nets in the symbol table
    for (name, symbol) in symbol_table.get_nets() {
        if symbol.kind == SymbolKind::Net {
            if let Some(net_attr) = &symbol.net_attributes {
                match net_attr {
                    NetAttribute::PowerDomain { voltage, max_current, declared_current, tolerance, controllable, enable_signal, startup_delay_ms, sequence_priority, dependencies, .. } => {
                        let mut domain = PowerDomain::new(name.clone(), *voltage);
                        domain.max_current = *max_current;
                        domain.declared_current = *declared_current;
                        domain.tolerance = *tolerance;
                        domain.controllable = *controllable;
                        domain.enable_signal = enable_signal.clone();
                        domain.startup_delay_ms = *startup_delay_ms;
                        domain.sequence_priority = *sequence_priority;
                        domain.dependencies = dependencies.clone();
                        context.add_domain(domain);
                    }
                    NetAttribute::GroundDomain => {
                        let domain = PowerDomain::new(name.clone(), 0.0);
                        context.add_domain(domain);
                    }
                    _ => {}
                }
            }
        }
    }
    
    // Note: child scope traversal is now handled by ScopeRegistry parent-chain lookup.
    // Power domain symbols are in the global scope nets namespace, so no recursion needed.
}

/// Perform power analysis on a syntax tree with symbol table
pub fn analyze_power_domains(
    syntax: &SyntaxNode<BhdlLanguage>, 
    global_scope: &crate::symbol_table::SymbolTable,
    definition_scopes: &std::collections::HashMap<rowan::ast::SyntaxNodePtr<BhdlLanguage>, crate::symbol_table::SymbolTable>
) -> PowerAnalysisContext {
    let mut context = PowerAnalysisContext::new();
    
    // Load power domains from symbol table (global scope)
    load_power_domains_from_symbols(&mut context, global_scope);
    
    // Load power domains from definition scopes (board scopes)
    for (_, scope) in definition_scopes {
        load_power_domains_from_symbols(&mut context, scope);
    }
    
    // First pass: identify power domains and initial connections
    visit_node_for_power_analysis(syntax, &mut context);
    
    // Second pass: propagate power domains through the circuit
    propagate_power_domains_in_circuit(syntax, &mut context);
    
    // Generate power sequence
    if let Err(error) = context.generate_power_sequence() {
        context.add_error(error);
    }
    
    context
}

/// Visit nodes in the syntax tree for power analysis
fn visit_node_for_power_analysis(node: &SyntaxNode<BhdlLanguage>, context: &mut PowerAnalysisContext) {
    match node.kind() {
        // Power and ground declarations are now loaded from symbol table.
        // Their BOUNDARY-PORT record, however, is made here: the sugar
        // desugars to a board port, the explicit `port` form records
        // directly — one record shape (ports doctrine).
        SyntaxKind::POWER_DECL
        | SyntaxKind::GROUND_DECL
        | SyntaxKind::PORT_DECL
        | SyntaxKind::POWER_DOMAIN_DEF => {
            record_board_port(node, context);
        }
        SyntaxKind::COMPONENT_INST => {
            analyze_component_power_requirements(node, context);
        }
        SyntaxKind::FLOW_EXPR => {
            analyze_flow_power_domains(node, context);
        }
        SyntaxKind::CONNECTION_STMT => {
            analyze_connection_power_domains(node, context);
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

/// Record a board-level boundary port from either spelling.
///
/// Board scope only: `power`/`ground` declarations inside entity expansion
/// blocks are internal to the generated circuit, not board boundaries.
fn record_board_port(node: &SyntaxNode<BhdlLanguage>, context: &mut PowerAnalysisContext) {
    use bhdl_ast::{AstNode, BoardPortDecl, BoardPortDirection, BoardPortType, GroundDecl, PowerDecl};

    if !node.ancestors().any(|a| a.kind() == SyntaxKind::BOARD_DEF) {
        return;
    }

    let info = match node.kind() {
        SyntaxKind::PORT_DECL => {
            let Some(decl) = BoardPortDecl::cast(node.clone()) else { return };
            let Some(name) = decl.name() else { return };
            let kind = match decl.port_type() {
                Some(BoardPortType::Power) => BoardPortKind::Power,
                Some(BoardPortType::Ground) => BoardPortKind::Ground,
                Some(BoardPortType::Signal) => BoardPortKind::Signal,
                None => return,
            };
            let direction = match decl.direction() {
                Some(BoardPortDirection::In) => BoardPortDir::In,
                Some(BoardPortDirection::Out) => BoardPortDir::Out,
                Some(BoardPortDirection::InOut) => BoardPortDir::InOut,
                // Type defaults: a power port is supplied from outside;
                // ground and signal are bidirectional unless said otherwise.
                None => match kind {
                    BoardPortKind::Power => BoardPortDir::In,
                    _ => BoardPortDir::InOut,
                },
            };
            BoardPortInfo {
                name: name.text().to_string(),
                kind,
                direction,
                voltage: decl.voltage().and_then(|v| parse_electrical_value(&v)),
                current: decl.current().and_then(|c| parse_electrical_value(&c)),
                explicit: true,
            }
        }
        SyntaxKind::POWER_DECL => {
            let Some(decl) = PowerDecl::cast(node.clone()) else { return };
            let Some(name) = decl.name() else { return };
            BoardPortInfo {
                name: name.text().to_string(),
                kind: BoardPortKind::Power,
                direction: BoardPortDir::In,
                voltage: decl.voltage().and_then(|v| parse_electrical_value(&v)),
                current: decl.current().and_then(|c| parse_electrical_value(&c)),
                explicit: false,
            }
        }
        SyntaxKind::GROUND_DECL => {
            let Some(decl) = GroundDecl::cast(node.clone()) else { return };
            let Some(name) = decl.name() else { return };
            BoardPortInfo {
                name: name.text().to_string(),
                kind: BoardPortKind::Ground,
                direction: BoardPortDir::InOut,
                voltage: None,
                current: None,
                explicit: false,
            }
        }
        // `power_domain @VCC = 5V @ 10A { sources {...} ... }` — the
        // scalability spelling of a declared rail. Direction comes from the
        // declaration itself: a non-empty `sources { }` block names the
        // on-board generator (rail is power-OUT); an empty/absent block
        // means power arrives from outside (power-IN boundary).
        SyntaxKind::POWER_DOMAIN_DEF => {
            let Some(decl) = bhdl_ast::items::PowerDomain::cast(node.clone()) else { return };
            let Some(name) = decl.net_name() else { return };
            let generated = decl
                .sources_block()
                .map(|sb| sb.syntax().children().next().is_some())
                .unwrap_or(false);
            // Token scan of the header: `= <NUMBER><unit> @ <NUMBER><unit> {`.
            let mut found_eq = false;
            let mut past_at = false;
            let mut voltage: Option<f64> = None;
            let mut current: Option<f64> = None;
            let mut pending_num: Option<String> = None;
            for token in node
                .children_with_tokens()
                .filter_map(|el| el.into_token())
            {
                match token.kind() {
                    SyntaxKind::EQ => found_eq = true,
                    SyntaxKind::L_BRACE => break,
                    SyntaxKind::AT if found_eq => {
                        past_at = true;
                        pending_num = None;
                    }
                    SyntaxKind::NUMBER if found_eq => pending_num = Some(token.text().to_string()),
                    SyntaxKind::UNIT_IDENTIFIER | SyntaxKind::IDENT if pending_num.is_some() => {
                        let txt = format!("{}{}", pending_num.take().unwrap(), token.text());
                        let v = parse_electrical_value(&txt);
                        if past_at {
                            current = current.or(v);
                        } else {
                            voltage = voltage.or(v);
                        }
                    }
                    _ => {}
                }
            }
            BoardPortInfo {
                name,
                kind: BoardPortKind::Power,
                direction: if generated { BoardPortDir::Out } else { BoardPortDir::In },
                voltage,
                current,
                explicit: false,
            }
        }
        _ => return,
    };

    // One port per boundary net name — a redeclaration is the same boundary.
    if !context.board_ports.iter().any(|p| p.name == info.name) {
        context.board_ports.push(info);
    }
}

/// Analyze a power declaration node
fn analyze_power_declaration(node: &SyntaxNode<BhdlLanguage>, context: &mut PowerAnalysisContext) {
    use bhdl_ast::{PowerDecl, AstNode};
    
    if let Some(power_decl) = PowerDecl::cast(node.clone()) {
        if let Some(name_token) = power_decl.name() {
            let name = name_token.text().to_string();
            
            // Parse voltage value
            let voltage = power_decl.voltage()
                .and_then(|v| parse_electrical_value(&v))
                .unwrap_or(0.0);
                
            // Parse current value. Real-Data Policy: keep the genuinely-declared
            // budget (`@ I`) as an Option; `max_current` keeps the 1.0A estimate
            // default only for the trace-width/validation consumers.
            let declared_current = power_decl.current()
                .and_then(|c| parse_electrical_value(&c));
            let current = declared_current.unwrap_or(1.0);

            // Create power domain
            let mut domain = PowerDomain::new(name.clone(), voltage);
            domain.max_current = current;
            domain.declared_current = declared_current;
            
            // Add to context
            context.add_domain(domain);
        }
    }
}

/// Analyze a ground declaration node
fn analyze_ground_declaration(node: &SyntaxNode<BhdlLanguage>, context: &mut PowerAnalysisContext) {
    use bhdl_ast::{GroundDecl, AstNode};
    
    if let Some(ground_decl) = GroundDecl::cast(node.clone()) {
        if let Some(name_token) = ground_decl.name() {
            let name = name_token.text().to_string();
            
            // Ground is a special 0V power domain
            let domain = PowerDomain::new(name.clone(), 0.0);
            
            // Add to context
            context.add_domain(domain);
            context.add_ground_domain(name_token.text().to_string());
        }
    }
}

/// Parse electrical values with units (e.g., "5V", "1A", "100mA")
fn parse_electrical_value(value_str: &str) -> Option<f64> {
    let value_str = value_str.trim();
    
    // Find where the number ends and unit begins
    let num_end = value_str.chars()
        .position(|c| c.is_alphabetic())
        .unwrap_or(value_str.len());
    
    let (num_part, unit_part) = value_str.split_at(num_end);
    
    // Parse the numeric part
    let mut value = num_part.parse::<f64>().ok()?;
    
    // Apply unit multiplier
    match unit_part {
        "mV" => value *= 0.001,
        "V" => {}, // Base unit
        "kV" => value *= 1000.0,
        "uA" | "µA" => value *= 0.000001,
        "mA" => value *= 0.001,
        "A" => {}, // Base unit
        _ => {}, // Unknown unit, assume base
    }
    
    Some(value)
}

/// Analyze power requirements for component instantiation
fn analyze_component_power_requirements(node: &SyntaxNode<BhdlLanguage>, context: &mut PowerAnalysisContext) {
    // This function is called during the first pass to identify component instances
    // We should NOT assign domains here - that happens during power propagation
    // For now, just note the component exists
    
    // Extract component instance name if this is a named component
    // Format: name: ComponentType(params)
    if let Some(parent) = node.parent() {
        if parent.kind() == SyntaxKind::COMPONENT_INST {
            // Try to get the instance name from a preceding identifier
            let mut instance_name = None;
            
            // Look for pattern like "R1: Res(10k)"
            if let Some(prev_token) = parent.first_token() {
                // The first token might be the instance name
                let text = prev_token.text();
                if text != ":" && !text.is_empty() {
                    instance_name = Some(text.to_string());
                }
            }
            
            // If we found an instance name, we can track it
            // But don't assign a domain yet - that happens during connection analysis
            if let Some(name) = instance_name {
                // Just note that this component exists
                // The domain will be assigned when we trace power connections
                println!("Power Analysis: Found component instance '{}'", name);
            }
        }
    }
}

/// Analyze power domains in connection statements
fn analyze_connection_power_domains(node: &SyntaxNode<BhdlLanguage>, context: &mut PowerAnalysisContext) {
    use bhdl_ast::v2_statements::ConnectionStmt;
    
    if let Some(conn_stmt) = ConnectionStmt::cast(node.clone()) {
        // Get the full connection text
        let conn_text = conn_stmt.text();
        
        // Parse chained connections: VCC -> R1 -> LED1 -> GND
        let parts: Vec<&str> = conn_text.split("->").map(|s| s.trim()).collect();
        
        if parts.len() >= 2 {
            // Process each connection pair
            for i in 0..parts.len() - 1 {
                let lhs_full = parts[i];
                let rhs_full = parts[i + 1];
                
                // Clean up the parts (remove semicolon, extract component names)
                let lhs = lhs_full.trim_end_matches(';').trim();
                let rhs = rhs_full.trim_end_matches(';').trim();
                
                // Extract component name from LHS (handle pin references and @ prefix)
                let (lhs_component, lhs_is_net) = if lhs.starts_with('@') {
                    // This is a net reference
                    let name = lhs[1..].trim();
                    if let Some(dot_pos) = name.find('.') {
                        (name[..dot_pos].trim(), true)
                    } else {
                        (name, true)
                    }
                } else if let Some(dot_pos) = lhs.find('.') {
                    (lhs[..dot_pos].trim(), false)
                } else {
                    (lhs, false)
                };
                
                // Extract target identifier from RHS (handle named handles and @ prefix)
                let (rhs_target, rhs_is_net) = if rhs.starts_with('@') {
                    // This is a net reference
                    let name = rhs[1..].trim();
                    if let Some(dot_pos) = name.find('.') {
                        (name[..dot_pos].trim(), true)
                    } else {
                        (name, true)
                    }
                } else if let Some(colon_pos) = rhs.find(':') {
                    (rhs[..colon_pos].trim(), false)
                } else if let Some(dot_pos) = rhs.find('.') {
                    (rhs[..dot_pos].trim(), false)
                } else {
                    (rhs, false)
                };
                
                println!("Power Analysis: Processing connection '{}{}' -> '{}{}'", 
                    if lhs_is_net { "@" } else { "" }, lhs_component,
                    if rhs_is_net { "@" } else { "" }, rhs_target);
                
                // Check if LHS is a power domain (only if it has @ prefix)
                if lhs_is_net && context.domains.contains_key(lhs_component) {
                    // Get the source domain - could be the component itself or its assigned domain
                    let source_domain = if let Some(assigned_domain) = context.component_domains.get(lhs_component) {
                        assigned_domain.clone()
                    } else {
                        lhs_component.to_string()
                    };
                    
                    // Propagate power domain based on electrical characteristics
                    if let Some(source_power) = context.domains.get(&source_domain).cloned() {
                        // Component instantiation (D1: LED(red).A) should NOT create a power domain
                        // Only low-impedance power distribution components should propagate domains
                        if rhs.contains(':') {
                            // This is a component instantiation, just assign it to the source domain
                            if rhs_target != "GND" {
                                context.assign_component_domain(rhs_target.to_string(), source_domain.clone());
                                println!("Power Analysis: Assigned component '{}' to power domain '{}'", rhs_target, source_domain);
                            }
                        } else if rhs_is_net && rhs_target != "GND" && !context.domains.contains_key(rhs_target) {
                            // Only create derived domain for @ net references that could be power nets
                            // In the future, check component impedance characteristics
                            println!("Power Analysis: Checking if '@{}' should be a power domain", rhs_target);
                            
                            // For now, only propagate if it looks like a power net name
                            if rhs_target.contains("VCC") || rhs_target.contains("VDD") || 
                               rhs_target.contains("PWR") || rhs_target.ends_with("V") {
                                let mut derived_domain = source_power.clone();
                                derived_domain.name = rhs_target.to_string();
                                println!("Power Analysis: Creating derived power domain '{}' from '{}'", rhs_target, source_domain);
                                context.domains.insert(rhs_target.to_string(), derived_domain);
                            }
                        }
                    }
                }
            }
        }
        
        // Also try the normal parsing path in case it works for some connections
        if let Some(expr) = conn_stmt.expr() {
            analyze_flow_power_domains(&expr, context);
        }
    }
}

/// Analyze power domains in flow expressions
fn analyze_flow_power_domains(node: &SyntaxNode<BhdlLanguage>, context: &mut PowerAnalysisContext) {
    // Implement flow-specific power analysis
    // This tracks power flow through the circuit and propagates power domains
    
    use bhdl_ast::{flow::{FlowExpr, FlowElement}, expr::BinaryExpr};
    
    // Handle FlowExpr nodes
    if let Some(flow_expr) = FlowExpr::cast(node.clone()) {
        // Extract the flow elements
        let elements: Vec<_> = flow_expr.elements().collect();
        
        // If the first element is a power domain, propagate it through the flow
        if let Some(first_elem) = elements.first() {
            match first_elem {
                FlowElement::Identifier(ident) => {
                    let source_name = ident.text().to_string();
                    
                    // Check if this is a power domain (only if it has @ prefix)
                    // Note: FlowElement::Identifier doesn't include @ in the text
                    // So we need to check the actual syntax
                    if context.domains.contains_key(&source_name) {
                        // This is a power flow! Propagate the domain through passive components
                        propagate_power_domain_through_flow(&elements, &source_name, context);
                    }
                }
                _ => {} // Other flow elements don't start power flows
            }
        }
    }
    // Handle BINARY_EXPR nodes (connection statements)
    else if let Some(binary_expr) = BinaryExpr::cast(node.clone()) {
        // Check if this is a flow operator (->)
        if let Some(op) = binary_expr.op() {
            if op == SyntaxKind::ARROW {
                // Extract source and target from binary expression
                if let (Some(lhs), Some(rhs)) = (binary_expr.lhs(), binary_expr.rhs()) {
                    let lhs_text = lhs.syntax().text().to_string().trim().to_string();
                    let rhs_text = rhs.syntax().text().to_string().trim().to_string();
                    
                    println!("Power Analysis: Processing connection '{}' -> '{}'", lhs_text, rhs_text);
                    
                    // Case 1: Direct power domain connections (@VIN -> something)
                    // Only process if LHS has @ prefix
                    let lhs_is_net_ref = lhs_text.starts_with('@');
                    let lhs_net_name = if lhs_is_net_ref { &lhs_text[1..] } else { &lhs_text };
                    
                    if lhs_is_net_ref && context.domains.contains_key(lhs_net_name) {
                        println!("Power Analysis: Found power connection from domain '@{}'", lhs_net_name);
                        println!("Power Analysis: RHS text = '{}'", rhs_text);
                        
                        // Extract the target identifier from RHS
                        // Due to parser structure, for named handles like "fuse: Fuse(1A).1",
                        // the RHS only contains "fuse" and the colon+instantiation are siblings
                        let target_name = rhs_text.trim().to_string();
                        
                        // Only create derived power domain if target looks like a power net
                        // Don't create domains for component handles (will be checked elsewhere)
                        if let Some(source_power) = context.domains.get(lhs_net_name).cloned() {
                            // Check if target should be a power domain
                            if target_name.contains("VCC") || target_name.contains("VDD") || 
                               target_name.contains("PWR") || target_name.ends_with("V") ||
                               target_name == "GND" {
                                let mut derived_domain = source_power.clone();
                                derived_domain.name = target_name.clone();
                                println!("Power Analysis: Creating derived power domain '{}' from '{}'", target_name, lhs_text);
                                context.domains.insert(target_name, derived_domain);
                            } else {
                                println!("Power Analysis: '{}' doesn't look like a power domain, skipping", target_name);
                            }
                        }
                    }
                    
                    // Case 2: Power propagation through components (component.pin -> net)
                    // Check if LHS is a pin reference from a power-propagating component
                    if let Some(dot_pos) = lhs_text.find('.') {
                        let component_name = lhs_text[..dot_pos].trim();
                        
                        // Check if this component is connected to a power domain
                        // Look for the component in our tracked power connections
                        if context.domains.contains_key(component_name) {
                            println!("Power Analysis: Found power propagation from component '{}'", component_name);
                            
                            // Extract target net name from RHS
                            // Due to parser structure, the RHS text already contains just the identifier
                            let target_name = rhs_text.trim().to_string();
                            
                            // Propagate power domain
                            if let Some(source_power) = context.domains.get(component_name).cloned() {
                                let mut derived_domain = source_power.clone();
                                derived_domain.name = target_name.clone();
                                println!("Power Analysis: Propagating power domain to '{}' from '{}'", target_name, component_name);
                                context.domains.insert(target_name, derived_domain);
                            }
                        } else {
                            println!("Power Analysis: Component '{}' not found in power domains (LHS: '{}')", component_name, lhs_text);
                        }
                    }
                }
            }
        }
    }
}

/// Propagate power domain through a flow expression
fn propagate_power_domain_through_flow(
    elements: &[bhdl_ast::flow::FlowElement],
    source_domain: &str,
    context: &mut PowerAnalysisContext,
) {
    use bhdl_ast::flow::FlowElement;
    
    // Track the current power domain as we traverse the flow
    let mut current_domain = source_domain.to_string();
    
    for element in elements {
        match element {
            FlowElement::Identifier(token) => {
                let net_name = token.text().to_string();
                
                // If this identifier is not already a power domain, mark it as derived
                if !context.domains.contains_key(&net_name) {
                    // Create a derived power domain
                    if let Some(source_power) = context.domains.get(&current_domain).cloned() {
                        let mut derived_domain = source_power.clone();
                        derived_domain.name = net_name.clone();
                        // Mark as derived by adding a note
                        context.domains.insert(net_name.clone(), derived_domain);
                    }
                }
                
                // Update current domain for next iteration
                current_domain = net_name;
            }
            FlowElement::ComponentInstantiation(comp_inst) => {
                // Extract component type name
                let instance_name = if let Some(comp_type) = comp_inst.component_type() {
                    comp_type.text().to_string()
                } else {
                    // Fallback: use syntax text
                    comp_inst.syntax().text().to_string()
                };
                
                // Assign this component to the current power domain
                context.assign_component_domain(instance_name.clone(), current_domain.clone());
                println!("Power Analysis: Assigned component '{}' to power domain '{}'", instance_name, current_domain);
                
                // Check if this is a passive component that propagates power
                if let Some(comp_type) = comp_inst.component_type() {
                    let type_name = comp_type.text().to_string();
                    
                    // Passive components that propagate power domains
                    let power_propagating_components = [
                        "Fuse", "Inductor", "L", "Ferrite", "Res", "R",
                        "TVSDiode", "Diode", "D", "SchottkyDiode"
                    ];
                    
                    if power_propagating_components.contains(&type_name.as_str()) {
                        // This component propagates the power domain
                        // Create a domain for the component itself
                        if let Some(source_power) = context.domains.get(&current_domain).cloned() {
                            let mut component_domain = source_power.clone();
                            component_domain.name = instance_name.clone();
                            context.domains.insert(instance_name.clone(), component_domain);
                            // Update current domain to this component
                            current_domain = instance_name;
                        }
                    } else {
                        // Active component or transformer - power domain may change
                        // For now, we stop propagation at active components
                        break;
                    }
                }
            }
            _ => {
                // Other flow elements don't affect power propagation
            }
        }
    }
}

/// Analyze signal connections for power compatibility
fn analyze_signal_connections(_node: &SyntaxNode<BhdlLanguage>, _context: &mut PowerAnalysisContext) {
    // TODO: Implement signal connection power analysis
    // This would check voltage compatibility between connected pins
}

/// Second pass: propagate power domains through the circuit
fn propagate_power_domains_in_circuit(syntax: &SyntaxNode<BhdlLanguage>, context: &mut PowerAnalysisContext) {
    
    
    // Build a map of all connections in the circuit
    let mut connections: Vec<(String, String)> = Vec::new();
    
    // Collect all connections
    visit_connections_for_propagation(syntax, &mut connections);
    
    println!("Power Analysis: Found {} connections to analyze", connections.len());
    
    // Keep propagating until no new domains are created
    let mut changed = true;
    let mut iterations = 0;
    const MAX_ITERATIONS: usize = 10; // Prevent infinite loops
    
    while changed && iterations < MAX_ITERATIONS {
        changed = false;
        iterations += 1;
        
        for (source, target) in &connections {
            // Check if source is a power domain or connected to one
            let source_domain = if context.domains.contains_key(source) {
                Some(source.clone())
            } else {
                // Check if source is a pin from a component with power
                if let Some(dot_pos) = source.find('.') {
                    let component = &source[..dot_pos];
                    if context.domains.contains_key(component) {
                        Some(component.to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            
            if let Some(domain_name) = source_domain {
                // Check if target already has a power domain
                if !context.domains.contains_key(target) {
                    // Check if this is a power-propagating connection
                    if is_power_propagating_connection(source, target) {
                        // Create derived power domain
                        if let Some(source_power) = context.domains.get(&domain_name).cloned() {
                            let mut derived_domain = source_power.clone();
                            derived_domain.name = target.clone();
                            println!("Power Analysis: Propagating power domain to '{}' from '{}'", target, domain_name);
                            context.domains.insert(target.clone(), derived_domain);
                            
                            // Also assign the component to its source domain
                            if target.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) {
                                context.assign_component_domain(target.clone(), domain_name.clone());
                                println!("Power Analysis: Assigned component '{}' to domain '{}'", target, domain_name);
                            }
                            
                            changed = true;
                        }
                    }
                }
            }
        }
    }
    
    println!("Power Analysis: Domain propagation completed in {} iterations", iterations);
}

/// Visit all connections and extract source/target pairs
fn visit_connections_for_propagation(node: &SyntaxNode<BhdlLanguage>, connections: &mut Vec<(String, String)>) {
    use bhdl_ast::v2_statements::ConnectionStmt;
    
    if let Some(conn_stmt) = ConnectionStmt::cast(node.clone()) {
        // Parse the connection statement text directly to handle named handles
        let stmt_text = conn_stmt.syntax().text().to_string();
        
        // Extract LHS and RHS from the arrow operator
        if let Some(arrow_pos) = stmt_text.find("->") {
            let lhs = stmt_text[..arrow_pos].trim();
            let rhs_with_semi = stmt_text[arrow_pos + 2..].trim();
            
            // Remove trailing semicolon
            let rhs_full = rhs_with_semi.trim_end_matches(';').trim();
            
            // For named handles like "fuse: Fuse(1A).1", extract just the name
            let rhs = if let Some(colon_pos) = rhs_full.find(':') {
                rhs_full[..colon_pos].trim()
            } else {
                rhs_full
            };
            
            connections.push((lhs.to_string(), rhs.to_string()));
        }
    }
    
    // Recursively visit children
    for child in node.children() {
        visit_connections_for_propagation(&child, connections);
    }
}

/// Check if a connection should propagate power domains
fn is_power_propagating_connection(source: &str, target: &str) -> bool {
    // Power propagates through:
    // 1. Direct connections (no component involved)
    // 2. Connections through passive components
    
    // Check if source is a pin reference
    if source.contains('.') {
        // This is a pin reference, check the pin number
        if let Some(dot_pos) = source.rfind('.') {
            let pin = &source[dot_pos + 1..];
            // Common power output pins
            if pin == "2" || pin == "OUT" || pin == "+" || pin == "-" {
                return true;
            }
        }
    }
    
    // Only propagate to identifiers that look like power net names
    // Don't propagate to arbitrary component handles
    if !target.contains('.') {
        // Check if target looks like a power net
        target.contains("VCC") || target.contains("VDD") || 
        target.contains("PWR") || target.ends_with("V") ||
        target == "GND"
    } else {
        false
    }
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

    /// Helper: synthesize the same set of domains the old
    /// `add_standard_domains` seeded, but from explicit
    /// construction (matching what `power FOO = X V;` source
    /// declarations now create). Used by tests below that
    /// exercise sequencing logic on a USB→3V3→1V8 topology.
    fn populate_standard_topology(context: &mut PowerAnalysisContext) {
        let mut usb_5v = PowerDomain::new("USB_5V".to_string(), 5.0);
        usb_5v.controllable = false;
        usb_5v.max_current = 0.5;
        usb_5v.sequence_priority = 1;
        context.add_domain(usb_5v);

        let mut vcc_3v3 = PowerDomain::new("VCC_3V3".to_string(), 3.3);
        vcc_3v3.dependencies.push("USB_5V".to_string());
        vcc_3v3.max_current = 1.0;
        vcc_3v3.sequence_priority = 2;
        vcc_3v3.enable_signal = Some("VCC_3V3_EN".to_string());
        context.add_domain(vcc_3v3);

        let mut vcc_1v8 = PowerDomain::new("VCC_1V8".to_string(), 1.8);
        vcc_1v8.dependencies.push("VCC_3V3".to_string());
        vcc_1v8.max_current = 0.5;
        vcc_1v8.sequence_priority = 3;
        vcc_1v8.enable_signal = Some("VCC_1V8_EN".to_string());
        context.add_domain(vcc_1v8);

        let mut gnd = PowerDomain::new("GND".to_string(), 0.0);
        gnd.controllable = false;
        gnd.max_current = 10.0;
        gnd.sequence_priority = 0;
        context.add_domain(gnd);
    }

    #[test]
    fn test_power_analysis_context() {
        // A freshly-constructed context now starts EMPTY — power
        // domains come from source declarations, not from a
        // hardcoded default set. Verify the empty initial state
        // and that domains added explicitly are visible.
        let mut context = PowerAnalysisContext::new();
        assert!(context.get_domain("USB_5V").is_none(),
            "fresh context should have no ambient domains");
        assert!(context.get_domain("GND").is_none(),
            "fresh context should have no ambient ground");

        populate_standard_topology(&mut context);
        assert!(context.get_domain("USB_5V").is_some());
        assert!(context.get_domain("VCC_3V3").is_some());
        assert!(context.get_domain("VCC_1V8").is_some());
        assert!(context.get_domain("GND").is_some());

        // Domain compatibility checks (unchanged semantics).
        assert!(!context.are_domains_compatible("USB_5V", "VCC_1V8"));
        assert!(context.are_domains_compatible("VCC_3V3", "VCC_3V3"));
    }

    #[test]
    fn test_power_sequence_generation() {
        let mut context = PowerAnalysisContext::new();
        populate_standard_topology(&mut context);

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