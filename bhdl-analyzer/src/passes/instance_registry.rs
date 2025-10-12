//! Pass 1.25: Early Component Instance Registry (Phase 2: Scalability + Hierarchical)
//!
//! This pass runs after Pass 1 (scope building) but before Pass 1.5 (power domain expansion).
//! It scans the AST for component and module instances and builds registries that can be queried
//! for wildcard expansion.
//!
//! The registry enables wildcard patterns like `sensors[*].VCC` to be expanded into
//! concrete connections to all matching component instances.
//!
//! Phase 3 adds hierarchical support, enabling patterns like `sensor_board[*].sensor.VCC`
//! to expand across module instance boundaries.

use bhdl_ast::{SourceFile, AstNode, Board, Module, ComponentInst, Item};
use std::collections::HashMap;

/// Registry of component and module instances found in the AST
#[derive(Debug, Clone)]
pub struct InstanceRegistry {
    /// Map from instance name to instance information
    /// Example: "sensor_0" -> InstanceInfo { kind: Component, type_name: "TempSensor", ... }
    instances: HashMap<String, InstanceInfo>,
    /// Map from module type names to their definitions
    /// Example: "SensorModule" -> contents of the module
    module_definitions: HashMap<String, ModuleContents>,
}

/// Information about an instance (component or module)
#[derive(Debug, Clone)]
pub struct InstanceInfo {
    /// The type name (e.g., "TempSensor" or "SensorModule")
    pub type_name: String,
    /// Whether this is an array instance (e.g., sensor[0])
    pub is_array_element: bool,
    /// What kind of instance this is
    pub kind: InstanceKind,
}

/// Kind of instance
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceKind {
    /// A component instance (leaf node)
    Component,
    /// A module instance (contains other instances)
    Module,
}

/// Contents of a module definition
#[derive(Debug, Clone)]
pub struct ModuleContents {
    /// Component instances inside this module
    /// Example: "sensor" -> InstanceInfo
    pub components: HashMap<String, InstanceInfo>,
    /// Module instances inside this module (for nested modules)
    pub modules: HashMap<String, InstanceInfo>,
}

impl InstanceRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            instances: HashMap::new(),
            module_definitions: HashMap::new(),
        }
    }

    /// Register a component instance
    pub fn register(&mut self, instance_name: String, type_name: String, is_array_element: bool) {
        self.instances.insert(
            instance_name,
            InstanceInfo {
                type_name,
                is_array_element,
                kind: InstanceKind::Component,
            },
        );
    }

    /// Register a module instance
    pub fn register_module(&mut self, instance_name: String, module_type: String, is_array_element: bool) {
        self.instances.insert(
            instance_name,
            InstanceInfo {
                type_name: module_type,
                is_array_element,
                kind: InstanceKind::Module,
            },
        );
    }

    /// Register a module definition (what's inside the module)
    pub fn register_module_definition(&mut self, module_type: String, contents: ModuleContents) {
        self.module_definitions.insert(module_type, contents);
    }

    /// Get module contents by type name
    pub fn get_module_contents(&self, module_type: &str) -> Option<&ModuleContents> {
        self.module_definitions.get(module_type)
    }

    /// Get all instance names
    pub fn get_instance_names(&self) -> Vec<&String> {
        self.instances.keys().collect()
    }

    /// Get instance info by name
    pub fn get_instance(&self, name: &str) -> Option<&InstanceInfo> {
        self.instances.get(name)
    }

    /// Find all instances matching a wildcard pattern
    /// Returns instance names that match the base component name pattern
    pub fn find_wildcard_matches(&self, base_name: &str) -> Vec<String> {
        let mut matches = Vec::new();

        for (instance_name, _info) in &self.instances {
            if is_wildcard_match(instance_name, base_name) {
                matches.push(instance_name.clone());
            }
        }

        // Sort for consistent ordering
        matches.sort();
        matches
    }

    /// Expand a hierarchical wildcard path like "sensor_board[*].sensor.VCC"
    /// Returns a list of fully-qualified hierarchical paths
    pub fn expand_hierarchical_wildcard(&self, path: &str) -> Vec<String> {
        // Split the path by dots: ["sensor_board[*]", "sensor", "VCC"]
        let parts: Vec<&str> = path.split('.').collect();

        if parts.len() < 2 {
            // Not a hierarchical path, no expansion needed
            return vec![path.to_string()];
        }

        // The first part may contain a wildcard
        let first_part = parts[0];
        let remaining_parts = &parts[1..];

        // Check if first part has a wildcard
        if first_part.contains("[*]") {
            // Extract base name from wildcard pattern
            let base_name = first_part.replace("[*]", "");

            // Find all matching instances
            let matches = self.find_wildcard_matches(&base_name);

            // For each match, recursively expand the rest of the path
            let mut result = Vec::new();
            for instance_name in matches {
                // Get the instance info to check if it's a module
                if let Some(info) = self.get_instance(&instance_name) {
                    if info.kind == InstanceKind::Module {
                        // Recursively expand through the module
                        let sub_paths = self.expand_through_module(
                            &instance_name,
                            &info.type_name,
                            remaining_parts
                        );
                        result.extend(sub_paths);
                    } else {
                        // It's a component, just concatenate the rest
                        let full_path = format!("{}.{}", instance_name, remaining_parts.join("."));
                        result.push(full_path);
                    }
                }
            }
            result
        } else {
            // No wildcard in first part, but still hierarchical
            // Check if it's a module instance
            if let Some(info) = self.get_instance(first_part) {
                if info.kind == InstanceKind::Module {
                    self.expand_through_module(first_part, &info.type_name, remaining_parts)
                } else {
                    // Component instance, just return the full path
                    vec![path.to_string()]
                }
            } else {
                // Instance not found
                vec![]
            }
        }
    }

    /// Expand a path through a module instance
    /// Example: expand "sensor_board_0" + ["sensor", "VCC"] -> "sensor_board_0.sensor.VCC"
    fn expand_through_module(&self, module_instance: &str, module_type: &str, remaining_parts: &[&str]) -> Vec<String> {
        if remaining_parts.is_empty() {
            return vec![module_instance.to_string()];
        }

        // Get the module contents
        let module_contents = match self.get_module_contents(module_type) {
            Some(contents) => contents,
            None => return vec![], // Module definition not found
        };

        let next_part = remaining_parts[0];
        let rest = &remaining_parts[1..];

        // Check if next part has a wildcard
        if next_part.contains("[*]") || next_part == "*" || next_part.starts_with('*') {
            // Expand wildcard within the module
            let mut result = Vec::new();

            // If it's a bare *, match all components
            if next_part == "*" {
                for (comp_name, _) in &module_contents.components {
                    let full_path = if rest.is_empty() {
                        format!("{}.{}", module_instance, comp_name)
                    } else {
                        format!("{}.{}.{}", module_instance, comp_name, rest.join("."))
                    };
                    result.push(full_path);
                }
            } else if next_part.starts_with('*') {
                // Pattern like "*sensor" - suffix match
                let suffix = &next_part[1..]; // Remove leading *
                for (comp_name, _) in &module_contents.components {
                    if comp_name.ends_with(suffix) {
                        let full_path = if rest.is_empty() {
                            format!("{}.{}", module_instance, comp_name)
                        } else {
                            format!("{}.{}.{}", module_instance, comp_name, rest.join("."))
                        };
                        result.push(full_path);
                    }
                }
            } else {
                // Pattern like "sensor[*]"
                let base_name = next_part.replace("[*]", "");
                for (comp_name, _) in &module_contents.components {
                    if is_wildcard_match(comp_name, &base_name) {
                        let full_path = if rest.is_empty() {
                            format!("{}.{}", module_instance, comp_name)
                        } else {
                            format!("{}.{}.{}", module_instance, comp_name, rest.join("."))
                        };
                        result.push(full_path);
                    }
                }
            }
            result
        } else {
            // Direct name, check if it exists in module
            if module_contents.components.contains_key(next_part) {
                let full_path = if rest.is_empty() {
                    format!("{}.{}", module_instance, next_part)
                } else {
                    format!("{}.{}.{}", module_instance, next_part, rest.join("."))
                };
                vec![full_path]
            } else {
                // Check if it's a nested module
                if let Some(sub_module_info) = module_contents.modules.get(next_part) {
                    // Recursively expand through nested module
                    self.expand_through_module(
                        &format!("{}.{}", module_instance, next_part),
                        &sub_module_info.type_name,
                        rest
                    )
                } else {
                    vec![] // Not found
                }
            }
        }
    }

    /// Get the total number of registered instances
    pub fn len(&self) -> usize {
        self.instances.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// Find similar base names using fuzzy matching
    /// Returns a list of base names that are similar to the query, sorted by similarity
    pub fn find_similar_base_names(&self, query: &str, max_distance: usize) -> Vec<(String, usize)> {
        let mut candidates: Vec<(String, usize)> = Vec::new();

        // Extract all unique base names from instances
        let base_names = self.extract_unique_base_names();

        // Calculate Levenshtein distance for each base name
        for base_name in base_names {
            let distance = levenshtein_distance(query, &base_name);
            if distance <= max_distance {
                candidates.push((base_name, distance));
            }
        }

        // Sort by distance (most similar first)
        candidates.sort_by_key(|(_, dist)| *dist);
        candidates
    }

    /// Extract unique base names from all instances
    /// e.g., ["sensor_0", "sensor_1", "led0"] -> ["sensor", "led"]
    fn extract_unique_base_names(&self) -> Vec<String> {
        let mut base_names = std::collections::HashSet::new();

        for instance_name in self.instances.keys() {
            let base = extract_base_name(instance_name);
            base_names.insert(base);
        }

        base_names.into_iter().collect()
    }
}

/// Build the instance registry by scanning the AST
pub fn build_instance_registry(source_file: &SourceFile) -> InstanceRegistry {
    let mut registry = InstanceRegistry::new();

    // First pass: Scan all module definitions
    for item in source_file.items() {
        if let Some(module) = Module::cast(item.syntax().clone()) {
            scan_module_definition(&module, &mut registry);
        }
    }

    // Second pass: Scan all boards for component and module instances
    for item in source_file.items() {
        if let Some(board) = Board::cast(item.syntax().clone()) {
            scan_board_instances(&board, &mut registry);
        }
    }

    println!("Pass 1.25: Registered {} component/module instances", registry.len());
    println!("Pass 1.25: Registered {} module definitions", registry.module_definitions.len());
    registry
}

/// Scan a module definition and register its contents
fn scan_module_definition(module: &Module, registry: &mut InstanceRegistry) {
    use bhdl_ast::HasName;

    let module_name = match module.name() {
        Some(name) => name.text().to_string(),
        None => return,
    };

    let mut contents = ModuleContents {
        components: HashMap::new(),
        modules: HashMap::new(),
    };

    // Scan component instances inside the module
    for component_inst in module.component_instances() {
        if let (Some(inst_name), Some(type_name)) = (
            extract_instance_name(&component_inst),
            extract_component_type(&component_inst)
        ) {
            let is_array = inst_name.contains('[') || inst_name.contains('_');
            contents.components.insert(
                inst_name,
                InstanceInfo {
                    type_name,
                    is_array_element: is_array,
                    kind: InstanceKind::Component,
                },
            );
        }
    }

    // Scan module instances inside the module (for nested modules)
    for module_inst in module.module_instances() {
        use bhdl_ast::HasName;
        if let (Some(inst_name), Some(mod_type)) = (
            module_inst.name().map(|t| t.text().to_string()),
            module_inst.module_type().map(|t| t.text().to_string())
        ) {
            let is_array = inst_name.contains('[') || inst_name.contains('_');
            contents.modules.insert(
                inst_name,
                InstanceInfo {
                    type_name: mod_type,
                    is_array_element: is_array,
                    kind: InstanceKind::Module,
                },
            );
        }
    }

    println!("  Registered module definition: {} ({} components, {} modules)",
        module_name, contents.components.len(), contents.modules.len());

    registry.register_module_definition(module_name, contents);
}

/// Scan a board for component and module instances
fn scan_board_instances(board: &Board, registry: &mut InstanceRegistry) {
    // In BHDL v2.0, component and module instantiations use identical syntax.
    // We need to check the type name against registered module definitions
    // to determine whether each instance is a component or module.

    // Iterate through all component instances in the board
    for component_inst in board.component_instances() {
        // Extract type name to check if it's a known module
        let type_name = extract_component_type(&component_inst);
        let is_module = type_name.as_ref().map(|t| registry.module_definitions.contains_key(t)).unwrap_or(false);

        if is_module {
            // It's actually a module instance
            if let (Some(inst_name), Some(mod_type)) = (
                extract_instance_name(&component_inst),
                type_name
            ) {
                let is_array = inst_name.contains('[') || inst_name.contains('_');
                registry.register_module(inst_name.clone(), mod_type.clone(), is_array);
                println!("  Registered module instance: {} : {}", inst_name, mod_type);
            }
        } else {
            // It's a regular component
            register_component_instance(&component_inst, registry);
        }
    }

    // Scan generate blocks for generated instances
    for generate_block in board.generate_blocks() {
        scan_generate_block(&generate_block, registry);
    }
}

/// Register a module instance
fn register_module_instance(inst: &bhdl_ast::ModuleInst, registry: &mut InstanceRegistry) {
    use bhdl_ast::HasName;

    let instance_name = inst.name().map(|t| t.text().to_string());
    let module_type = inst.module_type().map(|t| t.text().to_string());

    if let (Some(name), Some(type_name)) = (instance_name, module_type) {
        let is_array = name.contains('[') || name.contains('_');
        registry.register_module(name.clone(), type_name.clone(), is_array);
        println!("  Registered module instance: {} : {}", name, type_name);
    }
}

/// Scan a generate block for component instances created by for loops
fn scan_generate_block(generate_block: &bhdl_ast::GenerateBlock, registry: &mut InstanceRegistry) {
    use bhdl_ast::AstNode;

    // Process all for loop generates
    for for_loop in generate_block.for_loops() {
        scan_for_loop_generate(&for_loop, registry);
    }

    // TODO: Handle if generates (conditional instance creation)
}

/// Scan a for loop generate and register all generated instances
fn scan_for_loop_generate(for_loop: &bhdl_ast::ForLoopGenerate, registry: &mut InstanceRegistry) {
    use bhdl_ast::AstNode;

    // Extract loop variable name
    let loop_var = match for_loop.loop_var() {
        Some(var) => var,
        None => {
            println!("  Warning: For loop generate missing loop variable");
            return;
        }
    };

    // Extract range bounds (e.g., (0, 7) from "0..7")
    let (start, end) = match for_loop.range_bounds() {
        Some((s, e)) => (s, e),
        None => {
            println!("  Warning: For loop generate missing range");
            return;
        }
    };

    println!("  Found generate for loop: {} in {}..{}", loop_var, start, end);

    // Iterate through all child nodes to find component instances
    // They may be direct children or wrapped in CONNECTION_STMT nodes
    for child in for_loop.syntax().children() {
        // Check for direct component instances
        if let Some(component_inst) = bhdl_ast::ComponentInst::cast(child.clone()) {
            process_generate_instance(&component_inst, &loop_var, start, end, registry);
        }

        // Check for component instances inside CONNECTION_STMT
        if child.kind() == bhdl_ast::SyntaxKind::CONNECTION_STMT {
            // CONNECTION_STMT has NET_REF : COMPONENT_INST structure
            // The NET_REF contains the instance name (like sensor[i])
            // The COMPONENT_INST contains the type

            let mut instance_name_template = None;
            let mut component_type = None;

            for stmt_child in child.children() {
                // Extract instance name from NET_REF
                if stmt_child.kind() == bhdl_ast::SyntaxKind::NET_REF {
                    instance_name_template = extract_instance_name_from_net_ref(&stmt_child, &loop_var);
                }

                // Extract component type from COMPONENT_INST
                if let Some(comp_inst) = bhdl_ast::ComponentInst::cast(stmt_child) {
                    // For CONNECTION_STMT, the component type is the first IDENT in COMPONENT_INST
                    component_type = extract_component_type_simple(&comp_inst);
                }
            }

            // Register the instances if we found both parts
            if let (Some(name_template), Some(comp_type)) = (instance_name_template, component_type) {
                if name_template.contains(&loop_var) {
                    // Generate instances for each iteration
                    for i in start..=end {
                        let instance_name = name_template.replace(&loop_var, &i.to_string());
                        let is_array = instance_name.contains('[') || instance_name.contains('_');

                        registry.register(instance_name.clone(), comp_type.clone(), is_array);
                        println!("    Generated instance: {} : {}", instance_name, comp_type);
                    }
                }
            }
        }
    }
}

/// Extract instance name from a NET_REF node (like sensor[i])
fn extract_instance_name_from_net_ref(net_ref: &bhdl_ast::SyntaxNode<bhdl_ast::BhdlLanguage>, loop_var: &str) -> Option<String> {
    use bhdl_ast::AstNode;

    let mut base_name = None;
    let mut has_loop_var = false;

    // Look for IDENT (base name like "sensor")
    for element in net_ref.children_with_tokens() {
        if let Some(token) = element.as_token() {
            if token.kind() == bhdl_ast::SyntaxKind::IDENT {
                base_name = Some(token.text().to_string());
            }
        }
    }

    // Check if there's a BUS_SUFFIX with the loop variable
    for child in net_ref.children() {
        if child.kind() == bhdl_ast::SyntaxKind::BUS_SUFFIX {
            // Look for IDENT_REF containing loop variable
            // Need to descend into IDENT_REF node
            for sub_child in child.children() {
                if sub_child.kind() == bhdl_ast::SyntaxKind::IDENT_REF {
                    for element in sub_child.children_with_tokens() {
                        if let Some(token) = element.as_token() {
                            if token.kind() == bhdl_ast::SyntaxKind::IDENT && token.text() == loop_var {
                                has_loop_var = true;
                            }
                        }
                    }
                }
            }
        }
    }

    // Construct the template name
    if let Some(base) = base_name {
        if has_loop_var {
            Some(format!("{}[{}]", base, loop_var))
        } else {
            Some(base)
        }
    } else {
        None
    }
}

/// Process a component instance found in a generate block
fn process_generate_instance(
    component_inst: &bhdl_ast::ComponentInst,
    loop_var: &str,
    start: i32,
    end: i32,
    registry: &mut InstanceRegistry,
) {
    let name_template = match extract_instance_name(component_inst) {
        Some(name) => name,
        None => return,
    };

    let component_type = match extract_component_type(component_inst) {
        Some(typ) => typ,
        None => return,
    };

    if name_template.contains(loop_var) {
        // Generate instances for each iteration
        for i in start..=end {
            let instance_name = name_template.replace(loop_var, &i.to_string());
            let is_array = instance_name.contains('[') || instance_name.contains('_');

            registry.register(instance_name.clone(), component_type.clone(), is_array);
            println!("    Generated instance: {} : {}", instance_name, component_type);
        }
    } else {
        // Instance name doesn't use loop variable, register as-is
        let is_array = name_template.contains('[') || name_template.contains('_');
        registry.register(name_template.clone(), component_type.clone(), is_array);
        println!("    Registered static instance in generate block: {} : {}", name_template, component_type);
    }
}

/// Register a component instance
fn register_component_instance(inst: &ComponentInst, registry: &mut InstanceRegistry) {
    // Get instance name (handle prefixed with "instance_name:")
    let instance_name = extract_instance_name(inst);

    // Get component type
    let component_type = extract_component_type(inst);

    if let (Some(name), Some(type_name)) = (instance_name, component_type) {
        let is_array = name.contains('[') || name.contains('_');
        registry.register(name.clone(), type_name.clone(), is_array);
        println!("  Registered instance: {} : {}", name, type_name);
    }
}

/// Extract instance name from component instantiation
/// Handles syntax like: "sensor_0: TempSensor();"
fn extract_instance_name(inst: &ComponentInst) -> Option<String> {
    // Look for the instance name before the colon
    // The syntax tree structure is: COMPONENT_INST -> IDENT (name) -> COLON -> ...
    inst.syntax()
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == bhdl_ast::SyntaxKind::IDENT)
        .map(|t| t.text().to_string())
}

/// Extract component type from component instantiation in CONNECTION_STMT
/// For syntax like CONNECTION_STMT: "sensor[i]: TempSensor();"
/// The COMPONENT_INST only contains: "TempSensor()"
fn extract_component_type_simple(inst: &ComponentInst) -> Option<String> {
    // Simply get the first IDENT, which is the component type
    inst.syntax()
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == bhdl_ast::SyntaxKind::IDENT)
        .map(|t| t.text().to_string())
}

/// Extract component type from component instantiation
/// Handles syntax like: "sensor_0: TempSensor();"
fn extract_component_type(inst: &ComponentInst) -> Option<String> {
    // Look for the component type (second identifier after the colon)
    let mut found_colon = false;
    for element in inst.syntax().children_with_tokens() {
        if let Some(token) = element.as_token() {
            if token.kind() == bhdl_ast::SyntaxKind::COLON {
                found_colon = true;
            } else if found_colon && token.kind() == bhdl_ast::SyntaxKind::IDENT {
                return Some(token.text().to_string());
            }
        }
    }
    None
}

/// Check if an instance name matches a wildcard pattern
/// Supports patterns like:
/// - "sensor[0]", "sensor[1]" match base "sensor"
/// - "sensor_0", "sensor_1" match base "sensor"
/// - "sensor0", "sensor1" match base "sensor"
fn is_wildcard_match(instance_name: &str, base_name: &str) -> bool {
    // Exact prefix match with array notation: sensor[0], sensor[1]
    if instance_name.starts_with(base_name) {
        let remainder = &instance_name[base_name.len()..];

        // Check for array notation: [0], [1]
        if remainder.starts_with('[') {
            return true;
        }

        // Check for underscore separator: _0, _1
        if remainder.starts_with('_') && remainder.len() > 1 {
            return remainder[1..].chars().all(|c| c.is_ascii_digit());
        }

        // Check for direct number: 0, 1, 2
        if !remainder.is_empty() && remainder.chars().all(|c| c.is_ascii_digit()) {
            return true;
        }
    }

    false
}

/// Extract base name from an instance name
/// Examples:
/// - "sensor_0" -> "sensor"
/// - "sensor[1]" -> "sensor"
/// - "led0" -> "led"
fn extract_base_name(instance_name: &str) -> String {
    // Check for array notation first: sensor[0]
    if let Some(bracket_pos) = instance_name.find('[') {
        return instance_name[..bracket_pos].to_string();
    }

    // Check for underscore separator: sensor_0
    if let Some(underscore_pos) = instance_name.rfind('_') {
        let after_underscore = &instance_name[underscore_pos + 1..];
        if !after_underscore.is_empty() && after_underscore.chars().all(|c| c.is_ascii_digit()) {
            return instance_name[..underscore_pos].to_string();
        }
    }

    // Check for trailing digits: sensor0
    let non_digit_end = instance_name
        .char_indices()
        .rev()
        .find(|(_, c)| !c.is_ascii_digit())
        .map(|(i, _)| i + 1)
        .unwrap_or(0);

    if non_digit_end < instance_name.len() {
        return instance_name[..non_digit_end].to_string();
    }

    // No pattern found, return as-is
    instance_name.to_string()
}

/// Calculate Levenshtein distance between two strings
/// This is the minimum number of single-character edits (insertions, deletions, substitutions)
/// needed to transform one string into another.
fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let len1 = s1.len();
    let len2 = s2.len();

    // Create a matrix to store distances
    let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

    // Initialize first row and column
    for i in 0..=len1 {
        matrix[i][0] = i;
    }
    for j in 0..=len2 {
        matrix[0][j] = j;
    }

    // Fill in the matrix
    for (i, c1) in s1.chars().enumerate() {
        for (j, c2) in s2.chars().enumerate() {
            let cost = if c1 == c2 { 0 } else { 1 };
            matrix[i + 1][j + 1] = std::cmp::min(
                std::cmp::min(
                    matrix[i][j + 1] + 1,     // deletion
                    matrix[i + 1][j] + 1,     // insertion
                ),
                matrix[i][j] + cost,          // substitution
            );
        }
    }

    matrix[len1][len2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wildcard_match_array_notation() {
        assert!(is_wildcard_match("sensor[0]", "sensor"));
        assert!(is_wildcard_match("sensor[1]", "sensor"));
        assert!(is_wildcard_match("sensor[99]", "sensor"));
        assert!(!is_wildcard_match("temperature[0]", "sensor"));
    }

    #[test]
    fn test_wildcard_match_underscore() {
        assert!(is_wildcard_match("sensor_0", "sensor"));
        assert!(is_wildcard_match("sensor_1", "sensor"));
        assert!(is_wildcard_match("sensor_99", "sensor"));
        assert!(!is_wildcard_match("sensor_", "sensor"));
        assert!(!is_wildcard_match("sensor_abc", "sensor"));
    }

    #[test]
    fn test_wildcard_match_direct_number() {
        assert!(is_wildcard_match("sensor0", "sensor"));
        assert!(is_wildcard_match("sensor1", "sensor"));
        assert!(is_wildcard_match("sensor99", "sensor"));
        assert!(!is_wildcard_match("sensorA", "sensor"));
    }

    #[test]
    fn test_wildcard_no_match() {
        assert!(!is_wildcard_match("temperature", "sensor"));
        assert!(!is_wildcard_match("sensor", "sensor")); // Exact match, not pattern
        assert!(!is_wildcard_match("sensors", "sensor")); // Different base
    }
}
