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
}

impl Default for MonomorphizationResult {
    fn default() -> Self {
        Self {
            specializations: HashMap::new(),
            name_to_key: HashMap::new(),
            generic_modules: HashMap::new(),
            diagnostics: Vec::new(),
            iterations: 0,
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
) -> MonomorphizationResult {
    let mut result = MonomorphizationResult::new();

    // Step 1: Collect generic module definitions from the global scope
    collect_generic_modules(scope_registry, &mut result);

    if result.generic_modules.is_empty() {
        println!("Pass 2.5: No generic entities found, skipping monomorphization");
        return result;
    }

    println!(
        "Pass 2.5: Found {} generic entity/entities: {:?}",
        result.generic_modules.len(),
        result.generic_modules.keys().collect::<Vec<_>>()
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
}
