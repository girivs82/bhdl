//! Pass 2.5: Monomorphization Pipeline
//!
//! Resolves generic entity instantiations by creating specialized copies
//! with concrete parameter values. Uses a fixed-point algorithm:
//!
//! 1. **Collect**: Build registry of generic entity definitions
//! 2. **Scan**: Find all instantiations with concrete parameters
//! 3. **Specialize**: Clone entity, substitute params, generate mangled name
//! 4. **Deduplicate**: Identical specializations share a single entry
//! 5. **Iterate**: Repeat until no new specializations are discovered
//!
//! ```bhdl
//! entity BuckConverter<V_IN: voltage, V_OUT: voltage>(duty: percentage)
//!     where V_IN >= 4.5V, V_OUT < V_IN
//! { ... }
//!
//! board MyBoard {
//!     psu: BuckConverter<12V, 3.3V>(50%);  // → BuckConverter_12V_3V3
//! }
//! ```

use std::collections::{BTreeMap, HashMap, HashSet};
use bhdl_common::{ConstValue, GenericParam};
use crate::scope_registry::{ScopeId, ScopeRegistry};
use crate::symbol_table::{Symbol, SymbolKind};
use crate::types::Diagnostic;

/// Deterministic key for deduplicating specializations.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SpecializationKey {
    /// Original generic module name.
    pub module_name: String,
    /// Concrete parameter values in deterministic order.
    pub params: BTreeMap<String, ConstValueKey>,
}

/// A hashable wrapper around `ConstValue` for use in specialization keys.
/// We convert floats to their bit representation for deterministic hashing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConstValueKey {
    Integer(i64),
    Float(u64), // f64 bits
    Bool(bool),
    String(String),
    Voltage(u64),
    Current(u64),
    Resistance(u64),
    Capacitance(u64),
    Inductance(u64),
    Power(u64),
    Frequency(u64),
    Time(u64),
}

impl ConstValueKey {
    pub fn from_const_value(cv: &ConstValue) -> Self {
        match cv {
            ConstValue::Integer(n) => ConstValueKey::Integer(*n),
            ConstValue::Float(f) => ConstValueKey::Float(f.to_bits()),
            ConstValue::Bool(b) => ConstValueKey::Bool(*b),
            ConstValue::String(s) => ConstValueKey::String(s.clone()),
            ConstValue::Voltage(v) => ConstValueKey::Voltage(v.to_bits()),
            ConstValue::Current(a) => ConstValueKey::Current(a.to_bits()),
            ConstValue::Resistance(r) => ConstValueKey::Resistance(r.to_bits()),
            ConstValue::Capacitance(c) => ConstValueKey::Capacitance(c.to_bits()),
            ConstValue::Inductance(l) => ConstValueKey::Inductance(l.to_bits()),
            ConstValue::Power(w) => ConstValueKey::Power(w.to_bits()),
            ConstValue::Frequency(hz) => ConstValueKey::Frequency(hz.to_bits()),
            ConstValue::Time(t) => ConstValueKey::Time(t.to_bits()),
        }
    }
}

/// A specialized (monomorphized) version of a generic module.
#[derive(Debug, Clone)]
pub struct SpecializedModule {
    /// The mangled name for this specialization (e.g., "BuckConverter_12V_3V3_2A").
    pub mangled_name: String,
    /// The original generic module name.
    pub original_name: String,
    /// Concrete parameter values used for this specialization.
    pub concrete_params: BTreeMap<String, ConstValue>,
    /// Whether constraint checks passed for this specialization.
    pub constraints_satisfied: bool,
    /// Any constraint violation messages.
    pub constraint_errors: Vec<String>,
    /// Pins excluded by `when` conditions evaluating to false.
    pub excluded_pins: HashSet<String>,
    /// Resolved bus sizes for parameterized array pins (pin_name → concrete size).
    pub resolved_bus_sizes: HashMap<String, i64>,
}

/// Registry of generic module definitions.
#[derive(Debug, Clone)]
pub struct GenericModuleDef {
    /// Module name.
    pub name: String,
    /// Generic parameter declarations.
    pub params: Vec<GenericParam>,
    /// The scope ID where this module was defined.
    pub scope_id: ScopeId,
}

/// An alias that maps to a generic entity with concrete type arguments.
/// e.g., `alias LM7805 = LinearRegulator<5V>;` → AliasSpecialization { alias_name: "LM7805", target_entity: "LinearRegulator", type_arg_texts: ["5V"] }
#[derive(Debug, Clone)]
pub struct AliasSpecialization {
    /// The alias name (e.g., "LM7805")
    pub alias_name: String,
    /// The target generic entity name (e.g., "LinearRegulator")
    pub target_entity: String,
    /// Raw text of each type argument (e.g., ["5V", "3.3V"])
    pub type_arg_texts: Vec<String>,
    /// Resolved concrete parameter values (populated during monomorphization)
    /// Maps generic param name → concrete ConstValue
    pub concrete_params: BTreeMap<String, ConstValue>,
}

/// Result of the monomorphization pass.
#[derive(Debug, Clone)]
pub struct MonomorphizationResult {
    /// All specialized modules, keyed by their specialization key.
    pub specializations: HashMap<SpecializationKey, SpecializedModule>,
    /// Map from mangled name back to specialization key.
    pub name_to_key: HashMap<String, SpecializationKey>,
    /// Generic module definitions found.
    pub generic_modules: HashMap<String, GenericModuleDef>,
    /// Diagnostics generated during monomorphization.
    pub diagnostics: Vec<Diagnostic>,
    /// Number of fixed-point iterations performed.
    pub iterations: usize,
    /// Alias specializations collected from Pass 1 (alias Name = Generic<args>)
    pub alias_specializations: Vec<AliasSpecialization>,
}

impl Default for MonomorphizationResult {
    fn default() -> Self {
        Self {
            specializations: HashMap::new(),
            name_to_key: HashMap::new(),
            generic_modules: HashMap::new(),
            diagnostics: Vec::new(),
            iterations: 0,
            alias_specializations: Vec::new(),
        }
    }
}

impl MonomorphizationResult {
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a specialized module by its mangled name.
    pub fn get_by_mangled_name(&self, name: &str) -> Option<&SpecializedModule> {
        self.name_to_key
            .get(name)
            .and_then(|key| self.specializations.get(key))
    }

    /// Check if a module name is a generic module.
    pub fn is_generic(&self, name: &str) -> bool {
        self.generic_modules.contains_key(name)
    }

    /// Get the mangled name for a specialization, if it exists.
    pub fn mangled_name_for(
        &self,
        module_name: &str,
        params: &BTreeMap<String, ConstValue>,
    ) -> Option<&str> {
        let key = SpecializationKey {
            module_name: module_name.to_string(),
            params: params
                .iter()
                .map(|(k, v)| (k.clone(), ConstValueKey::from_const_value(v)))
                .collect(),
        };
        self.specializations.get(&key).map(|s| s.mangled_name.as_str())
    }
}

/// Run the monomorphization pass.
///
/// This scans the scope registry for:
/// 1. Module definitions with generic parameters → builds generic registry
/// 2. Instances whose type matches a generic module → creates specializations
///
/// Constraint checking is performed on each specialization.
pub fn run_monomorphization(
    scope_registry: &ScopeRegistry,
    resolved_constants: &crate::types::ResolvedConstants,
    alias_specializations: Vec<AliasSpecialization>,
) -> MonomorphizationResult {
    let mut result = MonomorphizationResult::new();
    result.alias_specializations = alias_specializations;

    // Step 1: Collect generic module definitions from the global scope
    collect_generic_modules(scope_registry, &mut result);

    if result.generic_modules.is_empty() {
        println!("Pass 2.5: No generic entities found, skipping monomorphization");
        return result;
    }

    println!(
        "Pass 2.5: Found {} generic entity/entities: {:?}",
        result.generic_modules.len(),
        {
            // Sorted — generic_modules is a HashMap; keep the line stable
            // run-to-run.
            let mut names: Vec<_> = result.generic_modules.keys().collect();
            names.sort();
            names
        }
    );

    // Step 2: Fixed-point iteration
    let max_iterations = 16; // Safety limit
    for iteration in 0..max_iterations {
        let before = result.specializations.len();

        // Scan all scopes for instances of generic modules
        scan_for_generic_instantiations(scope_registry, resolved_constants, &mut result);

        let after = result.specializations.len();
        result.iterations = iteration + 1;

        println!(
            "Pass 2.5: Iteration {} — {} new specialization(s) (total: {})",
            iteration + 1,
            after - before,
            after
        );

        if after == before {
            // Fixed point reached
            break;
        }
    }

    // Step 3: Register specialized modules in the global scope
    // (This is done by the caller, since we don't mutate the scope registry here)

    // Step 3: Process alias specializations
    process_alias_specializations(scope_registry, &mut result);

    println!(
        "Pass 2.5: Monomorphization complete. {} specialization(s) in {} iteration(s)",
        result.specializations.len(),
        result.iterations
    );

    result
}

/// Collect generic entity definitions from the scope registry.
fn collect_generic_modules(
    scope_registry: &ScopeRegistry,
    result: &mut MonomorphizationResult,
) {
    let global = scope_registry.global_scope();

    for sym in global.iter() {
        if sym.kind == SymbolKind::Entity || sym.kind == SymbolKind::Component {
            if let Some(ref gp) = sym.generic_params {
                if !gp.is_empty() {
                    // Find the scope ID for this entity's definition
                    let scope_id = if let Some(ref node_ptr) = sym.definition_node_ptr {
                        scope_registry
                            .scope_id_for_node(node_ptr)
                            .unwrap_or(scope_registry.global_id())
                    } else {
                        scope_registry.global_id()
                    };

                    result.generic_modules.insert(
                        sym.name.clone(),
                        GenericModuleDef {
                            name: sym.name.clone(),
                            params: gp.clone(),
                            scope_id,
                        },
                    );
                }
            }
        }
    }
}

/// Scan all scopes for instantiations of generic entities.
fn scan_for_generic_instantiations(
    scope_registry: &ScopeRegistry,
    resolved_constants: &crate::types::ResolvedConstants,
    result: &mut MonomorphizationResult,
) {
    // Collect generic entity names for lookup
    let generic_names: HashSet<String> = result.generic_modules.keys().cloned().collect();

    // Walk all scopes looking for Instance symbols whose type is a generic entity
    for scope_entry in scope_registry.iter() {
        for sym in scope_entry.table.iter() {
            if sym.kind == SymbolKind::Instance {
                if let Some(ref type_name) = sym.instance_type_name {
                    if generic_names.contains(type_name) {
                        // This is an instantiation of a generic entity.
                        // Try to resolve concrete parameter values.
                        try_specialize(type_name, sym, resolved_constants, result);
                    }
                }
            }
        }
    }
}

/// Attempt to create a specialization for a generic entity instantiation.
fn try_specialize(
    generic_name: &str,
    instance_sym: &Symbol,
    resolved_constants: &crate::types::ResolvedConstants,
    result: &mut MonomorphizationResult,
) {
    let generic_def = match result.generic_modules.get(generic_name) {
        Some(def) => def.clone(),
        None => return,
    };

    // Resolve concrete parameter values from the instance's parameter overrides
    let mut concrete_params = BTreeMap::new();

    if let Some(ref overrides) = instance_sym.parameter_overrides {
        for param in &generic_def.params {
            if let Some(node_ptr) = overrides.get(&param.name) {
                // Look up the resolved constant value using the AST node pointer
                if let Some(val) = resolved_constants.get(node_ptr) {
                    concrete_params.insert(param.name.clone(), val.clone());
                } else if let Some(ref default) = param.default {
                    concrete_params.insert(param.name.clone(), default.clone());
                }
            } else if let Some(ref default) = param.default {
                // Use default value
                concrete_params.insert(param.name.clone(), default.clone());
            }
        }
    } else {
        // No overrides — use defaults for all parameters
        for param in &generic_def.params {
            if let Some(ref default) = param.default {
                concrete_params.insert(param.name.clone(), default.clone());
            }
        }
    }

    // If we couldn't resolve all parameters, skip
    if concrete_params.len() < generic_def.params.len() {
        // Not all parameters resolved — skip silently (may be resolved in a later pass)
        return;
    }

    // Build specialization key
    let key = SpecializationKey {
        module_name: generic_name.to_string(),
        params: concrete_params
            .iter()
            .map(|(k, v)| (k.clone(), ConstValueKey::from_const_value(v)))
            .collect(),
    };

    // Check for deduplication
    if result.specializations.contains_key(&key) {
        return; // Already specialized
    }

    // Generate mangled name
    let mangled = generate_mangled_name(generic_name, &concrete_params);

    // Check constraints
    let mut constraint_errors = Vec::new();
    for param in &generic_def.params {
        for constraint in &param.constraints {
            let resolve = |name: &str| -> Option<ConstValue> {
                concrete_params.get(name).cloned()
            };
            if let Err(msg) = constraint.check(&resolve) {
                constraint_errors.push(format!(
                    "Constraint violated for '{}' in specialization '{}': {}",
                    param.name, mangled, msg
                ));
            }
        }
    }

    let constraints_satisfied = constraint_errors.is_empty();

    if !constraints_satisfied {
        // Add diagnostics for constraint violations
        for err_msg in &constraint_errors {
            result.diagnostics.push(Diagnostic::new(
                err_msg.clone(),
                instance_sym.span,
            ));
        }
    }

    let specialized = SpecializedModule {
        mangled_name: mangled.clone(),
        original_name: generic_name.to_string(),
        concrete_params,
        constraints_satisfied,
        constraint_errors,
        excluded_pins: HashSet::new(),
        resolved_bus_sizes: HashMap::new(),
    };

    result.name_to_key.insert(mangled, key.clone());
    result.specializations.insert(key, specialized);
}

/// Generate a human-readable mangled name for a specialization.
///
/// Examples:
/// - `BuckConverter<12V, 3.3V, 2A>` → `BuckConverter_12V_3V3_2A`
/// - `Filter<1kHz>` → `Filter_1kHz`
fn generate_mangled_name(
    module_name: &str,
    params: &BTreeMap<String, ConstValue>,
) -> String {
    let mut parts = vec![module_name.to_string()];

    for (_name, value) in params {
        let part = mangle_value(value);
        parts.push(part);
    }

    parts.join("_")
}

/// Convert a ConstValue to a mangled name fragment.
fn mangle_value(value: &ConstValue) -> String {
    match value {
        ConstValue::Integer(n) => format!("{}", n),
        ConstValue::Float(f) => format!("{}", f)
            .replace('.', "p")
            .replace('-', "neg"),
        ConstValue::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        ConstValue::String(s) => s.replace(' ', "_"),
        ConstValue::Voltage(v) => format_si_value(*v, "V"),
        ConstValue::Current(a) => format_si_value(*a, "A"),
        ConstValue::Resistance(r) => format_si_value(*r, "R"),
        ConstValue::Capacitance(c) => format_si_value(*c, "F"),
        ConstValue::Inductance(l) => format_si_value(*l, "H"),
        ConstValue::Power(w) => format_si_value(*w, "W"),
        ConstValue::Frequency(hz) => format_si_value(*hz, "Hz"),
        ConstValue::Time(t) => format_si_value(*t, "s"),
    }
}

/// Format a value with SI prefix for readable mangled names.
fn format_si_value(value: f64, unit: &str) -> String {
    let (scaled, prefix) = if value >= 1e6 {
        (value / 1e6, "M")
    } else if value >= 1e3 {
        (value / 1e3, "k")
    } else if value >= 1.0 {
        (value, "")
    } else if value >= 1e-3 {
        (value * 1e3, "m")
    } else if value >= 1e-6 {
        (value * 1e6, "u")
    } else if value >= 1e-9 {
        (value * 1e9, "n")
    } else if value >= 1e-12 {
        (value * 1e12, "p")
    } else {
        (value, "")
    };

    // Format to remove trailing zeros
    let num_str = if (scaled - scaled.round()).abs() < 1e-9 {
        format!("{}", scaled as i64)
    } else {
        format!("{:.1}", scaled)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    };

    format!("{}{}{}", num_str, prefix, unit)
        .replace('.', "p")
        .replace('-', "neg")
}

/// Process alias specializations collected during Pass 1.
///
/// For each `alias LM7805 = LinearRegulator<5V>;`, resolve the type arg text
/// to a ConstValue using the generic entity's param types, then create a
/// `SpecializedModule` entry so the synthesizer can access concrete params.
/// Also resolves pin specialization: conditional pins (`when`) and parameterized bus sizes.
fn process_alias_specializations(
    scope_registry: &ScopeRegistry,
    result: &mut MonomorphizationResult,
) {
    if result.alias_specializations.is_empty() {
        return;
    }

    let alias_specs: Vec<AliasSpecialization> = result.alias_specializations.clone();

    for alias in &alias_specs {
        let generic_def = match result.generic_modules.get(&alias.target_entity) {
            Some(def) => def.clone(),
            None => {
                println!(
                    "Pass 2.5: Alias '{}' targets '{}' which is not a generic entity, skipping",
                    alias.alias_name, alias.target_entity
                );
                continue;
            }
        };

        // Match type args positionally to generic params
        let mut concrete_params = BTreeMap::new();
        for (i, param) in generic_def.params.iter().enumerate() {
            if let Some(arg_text) = alias.type_arg_texts.get(i) {
                // Parse the type arg text into a ConstValue based on the param type
                if let Some(cv) = parse_type_arg_text(arg_text, &param.param_type) {
                    concrete_params.insert(param.name.clone(), cv);
                } else {
                    println!(
                        "Pass 2.5: Could not parse type arg '{}' for param '{}' in alias '{}'",
                        arg_text, param.name, alias.alias_name
                    );
                }
            } else if let Some(ref default) = param.default {
                concrete_params.insert(param.name.clone(), default.clone());
            }
        }

        if concrete_params.len() < generic_def.params.len() {
            println!(
                "Pass 2.5: Not all params resolved for alias '{}', skipping",
                alias.alias_name
            );
            continue;
        }

        // Build specialization key
        let key = SpecializationKey {
            module_name: alias.target_entity.clone(),
            params: concrete_params
                .iter()
                .map(|(k, v)| (k.clone(), ConstValueKey::from_const_value(v)))
                .collect(),
        };

        // Check constraints
        let mut constraint_errors = Vec::new();
        for param in &generic_def.params {
            for constraint in &param.constraints {
                let resolve = |name: &str| -> Option<ConstValue> {
                    concrete_params.get(name).cloned()
                };
                if let Err(msg) = constraint.check(&resolve) {
                    constraint_errors.push(format!(
                        "Constraint violated for '{}' in alias '{}': {}",
                        param.name, alias.alias_name, msg
                    ));
                }
            }
        }

        let constraints_satisfied = constraint_errors.is_empty();
        if !constraints_satisfied {
            for err_msg in &constraint_errors {
                result.diagnostics.push(Diagnostic::new(
                    err_msg.clone(),
                    rowan::TextRange::new(0.into(), 0.into()),
                ));
            }
        }

        // Resolve pin specialization by scanning the generic entity's scope for pin symbols
        let (excluded_pins, resolved_bus_sizes) = resolve_pin_specialization(
            scope_registry,
            &generic_def,
            &concrete_params,
        );

        // Use the alias name directly (not a mangled name)
        let specialized = SpecializedModule {
            mangled_name: alias.alias_name.clone(),
            original_name: alias.target_entity.clone(),
            concrete_params: concrete_params.clone(),
            constraints_satisfied,
            constraint_errors,
            excluded_pins: excluded_pins.clone(),
            resolved_bus_sizes: resolved_bus_sizes.clone(),
        };

        if !result.specializations.contains_key(&key) {
            result.name_to_key.insert(alias.alias_name.clone(), key.clone());
            result.specializations.insert(key, specialized);
            let mut pin_info = String::new();
            if !excluded_pins.is_empty() {
                pin_info.push_str(&format!(", excluded_pins: {:?}", excluded_pins));
            }
            if !resolved_bus_sizes.is_empty() {
                pin_info.push_str(&format!(", bus_sizes: {:?}", resolved_bus_sizes));
            }
            println!(
                "Pass 2.5: Created alias specialization '{}' → '{}' with params {:?}{}",
                alias.alias_name,
                alias.target_entity,
                concrete_params.keys().collect::<Vec<_>>(),
                pin_info
            );
        }
    }

    // Update concrete_params on alias_specializations for downstream use
    // Include both explicitly provided type args AND defaults
    for alias in &mut result.alias_specializations {
        if let Some(generic_def) = result.generic_modules.get(&alias.target_entity) {
            for (i, param) in generic_def.params.iter().enumerate() {
                if let Some(arg_text) = alias.type_arg_texts.get(i) {
                    if let Some(cv) = parse_type_arg_text(arg_text, &param.param_type) {
                        alias.concrete_params.insert(param.name.clone(), cv);
                    }
                } else if let Some(ref default) = param.default {
                    // Fill in default values for params not explicitly provided
                    alias.concrete_params.entry(param.name.clone())
                        .or_insert_with(|| default.clone());
                }
            }
        }
    }
}

/// Parse a type argument text (e.g., "5V", "3.3V", "100") into a ConstValue
/// based on the expected generic param type.
fn parse_type_arg_text(
    text: &str,
    param_type: &bhdl_common::GenericParamType,
) -> Option<ConstValue> {
    let text = text.trim();
    match param_type {
        bhdl_common::GenericParamType::Const(bhdl_type) => {
            match bhdl_type {
                bhdl_common::BhdlType::Voltage(_) => {
                    parse_value_with_unit(text, "V").map(ConstValue::Voltage)
                }
                bhdl_common::BhdlType::Current(_) => {
                    parse_value_with_unit(text, "A").map(ConstValue::Current)
                }
                bhdl_common::BhdlType::Resistance(_) => {
                    parse_value_with_unit(text, "Ω").or_else(|| parse_value_with_unit(text, "ohm"))
                        .map(ConstValue::Resistance)
                }
                bhdl_common::BhdlType::Capacitance => {
                    parse_value_with_unit(text, "F").map(ConstValue::Capacitance)
                }
                bhdl_common::BhdlType::Frequency => {
                    parse_value_with_unit(text, "Hz").map(ConstValue::Frequency)
                }
                bhdl_common::BhdlType::Power => {
                    parse_value_with_unit(text, "W").map(ConstValue::Power)
                }
                bhdl_common::BhdlType::Time => {
                    parse_value_with_unit(text, "s").map(ConstValue::Time)
                }
                bhdl_common::BhdlType::Bool => {
                    match text {
                        "true" => Some(ConstValue::Bool(true)),
                        "false" => Some(ConstValue::Bool(false)),
                        _ => None,
                    }
                }
                bhdl_common::BhdlType::Integer => {
                    text.parse::<i64>().ok().map(ConstValue::Integer)
                }
                _ => {
                    // Try plain number
                    text.parse::<f64>().ok().map(ConstValue::Float)
                }
            }
        }
        bhdl_common::GenericParamType::Type | bhdl_common::GenericParamType::TypeBounded(_) => {
            Some(ConstValue::String(text.to_string()))
        }
    }
}

/// Parse a value with optional SI prefix and unit suffix.
/// e.g., "5V" → 5.0, "3.3V" → 3.3, "100mA" → 0.1, "10kΩ" → 10000.0
fn parse_value_with_unit(text: &str, _expected_unit: &str) -> Option<f64> {
    // Strip any unit suffixes and parse
    let text = text.trim();

    // Try to split into numeric part and unit part
    let split_pos = text.find(|c: char| c.is_alphabetic() || c == 'Ω' || c == 'µ' || c == 'μ');

    let (num_str, unit_str) = if let Some(pos) = split_pos {
        (&text[..pos], &text[pos..])
    } else {
        (text, "")
    };

    let base_value = num_str.parse::<f64>().ok()?;

    // Apply SI prefix multiplier
    let multiplier = match unit_str {
        // Voltage
        "V" => 1.0,
        "mV" => 0.001,
        "kV" => 1000.0,
        "µV" | "μV" | "uV" => 1e-6,
        // Current
        "A" => 1.0,
        "mA" => 0.001,
        "µA" | "μA" | "uA" => 1e-6,
        // Resistance
        "Ω" | "ohm" => 1.0,
        "kΩ" | "kohm" | "kOhm" => 1000.0,
        "MΩ" | "Mohm" | "MOhm" => 1e6,
        "mΩ" | "mohm" => 0.001,
        // Capacitance
        "F" => 1.0,
        "mF" => 0.001,
        "µF" | "μF" | "uF" => 1e-6,
        "nF" => 1e-9,
        "pF" => 1e-12,
        // Frequency
        "Hz" => 1.0,
        "kHz" => 1000.0,
        "MHz" => 1e6,
        "GHz" => 1e9,
        // Power
        "W" => 1.0,
        "mW" => 0.001,
        // Time
        "s" => 1.0,
        "ms" => 0.001,
        "µs" | "μs" | "us" => 1e-6,
        "ns" => 1e-9,
        // No unit
        "" => 1.0,
        _ => 1.0,
    };

    Some(base_value * multiplier)
}

/// Resolve pin specialization for a concrete set of parameters.
///
/// Scans the generic entity's scope for pin symbols with:
/// - `when_condition`: If condition references a bool param that is false, exclude the pin.
/// - `bus_size_param`: If it references an integer param, resolve the concrete bus size.
fn resolve_pin_specialization(
    scope_registry: &ScopeRegistry,
    generic_def: &GenericModuleDef,
    concrete_params: &BTreeMap<String, ConstValue>,
) -> (HashSet<String>, HashMap<String, i64>) {
    let mut excluded_pins = HashSet::new();
    let mut resolved_bus_sizes = HashMap::new();

    // Find the scope for this generic entity definition
    let scope = scope_registry.table(generic_def.scope_id);

    // Scan all symbols in the entity's scope for pins
    for sym in scope.iter() {
        if sym.kind != SymbolKind::Pin && sym.kind != SymbolKind::VirtualPin {
            continue;
        }

        // Check when_condition for conditional pins
        if let Some(ref condition) = sym.when_condition {
            let condition = condition.trim();
            // Handle negated conditions: `!PARAM_NAME`
            let (is_negated, param_name) = if let Some(stripped) = condition.strip_prefix('!') {
                (true, stripped.trim())
            } else {
                (false, condition)
            };

            if let Some(cv) = concrete_params.get(param_name) {
                let is_true = match cv {
                    ConstValue::Bool(b) => *b,
                    ConstValue::Integer(n) => *n != 0,
                    _ => true, // Non-bool/int params are considered truthy
                };
                let include = if is_negated { !is_true } else { is_true };
                if !include {
                    excluded_pins.insert(sym.name.clone());
                    println!(
                        "Pass 2.5: Pin '{}' excluded (when {} = {:?})",
                        sym.name, condition, cv
                    );
                }
            }
        }

        // Check bus_size_param for parameterized array pins
        if let Some(ref param_name) = sym.bus_size_param {
            if let Some(cv) = concrete_params.get(param_name) {
                let size = match cv {
                    ConstValue::Integer(n) => *n,
                    ConstValue::Float(f) => *f as i64,
                    _ => continue,
                };
                resolved_bus_sizes.insert(sym.name.clone(), size);
                println!(
                    "Pass 2.5: Pin '{}' bus size resolved to {} (from param '{}')",
                    sym.name, size, param_name
                );
            }
        }
    }

    (excluded_pins, resolved_bus_sizes)
}

/// Register specialized entities into the scope registry.
/// Called after monomorphization to make specialized names available for later passes.
pub fn register_specializations(
    scope_registry: &mut ScopeRegistry,
    mono_result: &MonomorphizationResult,
) {
    use rowan::TextRange;

    for (_key, spec) in &mono_result.specializations {
        if !spec.constraints_satisfied {
            continue; // Don't register invalid specializations
        }

        // Check if already registered
        if scope_registry.lookup_global(&spec.mangled_name).is_some() {
            continue;
        }

        let sym = Symbol {
            name: spec.mangled_name.clone(),
            kind: SymbolKind::Entity,
            span: TextRange::new(0.into(), 0.into()),
            instance_type_name: Some(spec.original_name.clone()),
            definition_node_ptr: None,
            bus_high: None,
            bus_low: None,
            direction: None,
            parameter_overrides: None,
            net_attributes: None,
            resolved_type: None,
            generic_params: None,
            when_condition: None,
            bus_size_param: None,
        };

        scope_registry.global_scope_mut().insert(sym);
        println!(
            "Pass 2.5: Registered specialization '{}' (from '{}')",
            spec.mangled_name, spec.original_name
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mangled_name_generation() {
        let mut params = BTreeMap::new();
        params.insert("V_IN".to_string(), ConstValue::Voltage(12.0));
        params.insert("V_OUT".to_string(), ConstValue::Voltage(3.3));

        let name = generate_mangled_name("BuckConverter", &params);
        assert_eq!(name, "BuckConverter_12V_3p3V");
    }

    #[test]
    fn test_mangled_name_with_current() {
        let mut params = BTreeMap::new();
        params.insert("I_MAX".to_string(), ConstValue::Current(2.0));

        let name = generate_mangled_name("Regulator", &params);
        assert_eq!(name, "Regulator_2A");
    }

    #[test]
    fn test_mangled_name_milliamps() {
        let mut params = BTreeMap::new();
        params.insert("I".to_string(), ConstValue::Current(0.02)); // 20mA

        let name = generate_mangled_name("LED_Driver", &params);
        assert_eq!(name, "LED_Driver_20mA");
    }

    #[test]
    fn test_specialization_key_dedup() {
        let key1 = SpecializationKey {
            module_name: "Mod".to_string(),
            params: {
                let mut m = BTreeMap::new();
                m.insert("V".to_string(), ConstValueKey::Voltage(12.0f64.to_bits()));
                m
            },
        };
        let key2 = SpecializationKey {
            module_name: "Mod".to_string(),
            params: {
                let mut m = BTreeMap::new();
                m.insert("V".to_string(), ConstValueKey::Voltage(12.0f64.to_bits()));
                m
            },
        };
        assert_eq!(key1, key2);

        let key3 = SpecializationKey {
            module_name: "Mod".to_string(),
            params: {
                let mut m = BTreeMap::new();
                m.insert("V".to_string(), ConstValueKey::Voltage(5.0f64.to_bits()));
                m
            },
        };
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_specialization_with_constraints() {
        let mut result = MonomorphizationResult::new();

        // Register a generic module with constraints
        result.generic_modules.insert(
            "Buck".to_string(),
            GenericModuleDef {
                name: "Buck".to_string(),
                params: vec![
                    GenericParam {
                        name: "V_IN".to_string(),
                        param_type: bhdl_common::GenericParamType::Const(
                            bhdl_common::BhdlType::Voltage(None),
                        ),
                        constraints: vec![bhdl_common::Constraint::GreaterEqual(
                            bhdl_common::ConstraintExpr::Param("V_IN".to_string()),
                            bhdl_common::ConstraintExpr::Literal(ConstValue::Voltage(4.5)),
                        )],
                        default: None,
                    },
                    GenericParam {
                        name: "V_OUT".to_string(),
                        param_type: bhdl_common::GenericParamType::Const(
                            bhdl_common::BhdlType::Voltage(None),
                        ),
                        constraints: vec![bhdl_common::Constraint::LessThan(
                            bhdl_common::ConstraintExpr::Param("V_OUT".to_string()),
                            bhdl_common::ConstraintExpr::Param("V_IN".to_string()),
                        )],
                        default: None,
                    },
                ],
                scope_id: ScopeId(0),
            },
        );

        // Create a valid specialization
        let mut concrete = BTreeMap::new();
        concrete.insert("V_IN".to_string(), ConstValue::Voltage(12.0));
        concrete.insert("V_OUT".to_string(), ConstValue::Voltage(3.3));

        let key = SpecializationKey {
            module_name: "Buck".to_string(),
            params: concrete
                .iter()
                .map(|(k, v)| (k.clone(), ConstValueKey::from_const_value(v)))
                .collect(),
        };

        // Manually run constraint checks
        let generic_def = result.generic_modules.get("Buck").unwrap();
        let mut errors = Vec::new();
        for param in &generic_def.params {
            for constraint in &param.constraints {
                let resolve = |name: &str| -> Option<ConstValue> {
                    concrete.get(name).cloned()
                };
                if let Err(msg) = constraint.check(&resolve) {
                    errors.push(msg);
                }
            }
        }
        assert!(errors.is_empty(), "Constraints should be satisfied: {:?}", errors);

        let spec = SpecializedModule {
            mangled_name: generate_mangled_name("Buck", &concrete),
            original_name: "Buck".to_string(),
            concrete_params: concrete,
            constraints_satisfied: true,
            constraint_errors: vec![],
            excluded_pins: HashSet::new(),
            resolved_bus_sizes: HashMap::new(),
        };

        result.name_to_key.insert(spec.mangled_name.clone(), key.clone());
        result.specializations.insert(key, spec);

        assert_eq!(result.specializations.len(), 1);
        assert!(result.get_by_mangled_name("Buck_12V_3p3V").is_some());
    }

    #[test]
    fn test_constraint_violation_detected() {
        let generic_def = GenericModuleDef {
            name: "Buck".to_string(),
            params: vec![GenericParam {
                name: "V_IN".to_string(),
                param_type: bhdl_common::GenericParamType::Const(
                    bhdl_common::BhdlType::Voltage(None),
                ),
                constraints: vec![bhdl_common::Constraint::GreaterEqual(
                    bhdl_common::ConstraintExpr::Param("V_IN".to_string()),
                    bhdl_common::ConstraintExpr::Literal(ConstValue::Voltage(4.5)),
                )],
                default: None,
            }],
            scope_id: ScopeId(0),
        };

        // Try with V_IN = 3.0V (violates >= 4.5V)
        let mut concrete = BTreeMap::new();
        concrete.insert("V_IN".to_string(), ConstValue::Voltage(3.0));

        let mut errors = Vec::new();
        for param in &generic_def.params {
            for constraint in &param.constraints {
                let resolve = |name: &str| -> Option<ConstValue> {
                    concrete.get(name).cloned()
                };
                if let Err(msg) = constraint.check(&resolve) {
                    errors.push(msg);
                }
            }
        }
        assert!(!errors.is_empty(), "Should detect constraint violation");
    }

    #[test]
    fn test_si_prefix_formatting() {
        assert_eq!(format_si_value(12.0, "V"), "12V");
        assert_eq!(format_si_value(3.3, "V"), "3p3V");
        assert_eq!(format_si_value(0.02, "A"), "20mA");
        assert_eq!(format_si_value(1000.0, "R"), "1kR");
        assert_eq!(format_si_value(0.0000001, "F"), "100nF");
        assert_eq!(format_si_value(1000000.0, "Hz"), "1MHz");
    }

    #[test]
    fn test_parse_type_arg_bool() {
        let bool_type = bhdl_common::GenericParamType::Const(bhdl_common::BhdlType::Bool);
        assert_eq!(
            parse_type_arg_text("true", &bool_type),
            Some(ConstValue::Bool(true))
        );
        assert_eq!(
            parse_type_arg_text("false", &bool_type),
            Some(ConstValue::Bool(false))
        );
        assert_eq!(parse_type_arg_text("maybe", &bool_type), None);
    }

    #[test]
    fn test_parse_type_arg_integer() {
        let int_type = bhdl_common::GenericParamType::Const(bhdl_common::BhdlType::Integer);
        assert_eq!(
            parse_type_arg_text("4", &int_type),
            Some(ConstValue::Integer(4))
        );
        assert_eq!(
            parse_type_arg_text("1", &int_type),
            Some(ConstValue::Integer(1))
        );
        assert_eq!(parse_type_arg_text("abc", &int_type), None);
    }

    #[test]
    fn test_pin_exclusion_when_false() {
        use crate::scope_registry::{ScopeKind};

        let mut registry = ScopeRegistry::new();
        let entity_scope = registry.alloc_child(registry.global_id(), ScopeKind::Entity);

        // Add pins: VI, VO, GND (unconditional), EN (when HAS_EN)
        let dummy_range = rowan::TextRange::new(0.into(), 0.into());
        for pin_name in &["VI", "VO", "GND"] {
            registry.table_mut(entity_scope).insert(Symbol {
                name: pin_name.to_string(),
                kind: SymbolKind::Pin,
                span: dummy_range,
                instance_type_name: None,
                definition_node_ptr: None,
                bus_high: None,
                bus_low: None,
                direction: None,
                parameter_overrides: None,
                net_attributes: None,
                resolved_type: None,
                generic_params: None,
                when_condition: None,
                bus_size_param: None,
            });
        }
        registry.table_mut(entity_scope).insert(Symbol {
            name: "EN".to_string(),
            kind: SymbolKind::Pin,
            span: dummy_range,
            instance_type_name: None,
            definition_node_ptr: None,
            bus_high: None,
            bus_low: None,
            direction: None,
            parameter_overrides: None,
            net_attributes: None,
            resolved_type: None,
            generic_params: None,
            when_condition: Some("HAS_EN".to_string()),
            bus_size_param: None,
        });

        let generic_def = GenericModuleDef {
            name: "LinearRegulator".to_string(),
            params: vec![
                GenericParam {
                    name: "V_OUT".to_string(),
                    param_type: bhdl_common::GenericParamType::Const(bhdl_common::BhdlType::Voltage(None)),
                    constraints: vec![],
                    default: None,
                },
                GenericParam {
                    name: "HAS_EN".to_string(),
                    param_type: bhdl_common::GenericParamType::Const(bhdl_common::BhdlType::Bool),
                    constraints: vec![],
                    default: Some(ConstValue::Bool(false)),
                },
            ],
            scope_id: entity_scope,
        };

        // Test with HAS_EN = false → EN should be excluded
        let mut params_no_en = BTreeMap::new();
        params_no_en.insert("V_OUT".to_string(), ConstValue::Voltage(5.0));
        params_no_en.insert("HAS_EN".to_string(), ConstValue::Bool(false));
        let (excluded, _bus_sizes) = resolve_pin_specialization(&registry, &generic_def, &params_no_en);
        assert!(excluded.contains("EN"), "EN should be excluded when HAS_EN=false");
        assert_eq!(excluded.len(), 1, "Only EN should be excluded");

        // Test with HAS_EN = true → EN should be included
        let mut params_with_en = BTreeMap::new();
        params_with_en.insert("V_OUT".to_string(), ConstValue::Voltage(3.3));
        params_with_en.insert("HAS_EN".to_string(), ConstValue::Bool(true));
        let (excluded, _bus_sizes) = resolve_pin_specialization(&registry, &generic_def, &params_with_en);
        assert!(excluded.is_empty(), "No pins should be excluded when HAS_EN=true");
    }

    #[test]
    fn test_bus_size_resolution() {
        use crate::scope_registry::ScopeKind;

        let mut registry = ScopeRegistry::new();
        let entity_scope = registry.alloc_child(registry.global_id(), ScopeKind::Entity);

        let dummy_range = rowan::TextRange::new(0.into(), 0.into());
        // Add VCC and GND (unconditional, no bus)
        for pin_name in &["VCC", "GND"] {
            registry.table_mut(entity_scope).insert(Symbol {
                name: pin_name.to_string(),
                kind: SymbolKind::Pin,
                span: dummy_range,
                instance_type_name: None,
                definition_node_ptr: None,
                bus_high: None,
                bus_low: None,
                direction: None,
                parameter_overrides: None,
                net_attributes: None,
                resolved_type: None,
                generic_params: None,
                when_condition: None,
                bus_size_param: None,
            });
        }
        // Add INP, INM, OUT with bus_size_param = "CHANNELS"
        for pin_name in &["INP", "INM", "OUT"] {
            registry.table_mut(entity_scope).insert(Symbol {
                name: pin_name.to_string(),
                kind: SymbolKind::Pin,
                span: dummy_range,
                instance_type_name: None,
                definition_node_ptr: None,
                bus_high: None,
                bus_low: None,
                direction: None,
                parameter_overrides: None,
                net_attributes: None,
                resolved_type: None,
                generic_params: None,
                when_condition: None,
                bus_size_param: Some("CHANNELS".to_string()),
            });
        }

        let generic_def = GenericModuleDef {
            name: "OpAmp".to_string(),
            params: vec![GenericParam {
                name: "CHANNELS".to_string(),
                param_type: bhdl_common::GenericParamType::Const(bhdl_common::BhdlType::Integer),
                constraints: vec![],
                default: Some(ConstValue::Integer(1)),
            }],
            scope_id: entity_scope,
        };

        // Test with CHANNELS = 2
        let mut params = BTreeMap::new();
        params.insert("CHANNELS".to_string(), ConstValue::Integer(2));
        let (_excluded, bus_sizes) = resolve_pin_specialization(&registry, &generic_def, &params);
        assert_eq!(bus_sizes.get("INP"), Some(&2));
        assert_eq!(bus_sizes.get("INM"), Some(&2));
        assert_eq!(bus_sizes.get("OUT"), Some(&2));
        assert_eq!(bus_sizes.get("VCC"), None);

        // Test with CHANNELS = 4
        let mut params4 = BTreeMap::new();
        params4.insert("CHANNELS".to_string(), ConstValue::Integer(4));
        let (_excluded, bus_sizes4) = resolve_pin_specialization(&registry, &generic_def, &params4);
        assert_eq!(bus_sizes4.get("INP"), Some(&4));
        assert_eq!(bus_sizes4.get("OUT"), Some(&4));
    }
}
