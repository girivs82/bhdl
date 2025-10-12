//! Power Domain Expansion Pass (Phase 1: Scalability)
//!
//! Expands power_domain blocks into explicit connections and component instantiations.
//! This pass runs after Pass 1 (scope building) but before Pass 2 (reference checking),
//! so that the expanded connections can be validated.

use bhdl_ast::{PowerDomain, SourceDefinition, DistributionPinList, DecouplingRule, CapSpec, Expr};
use bhdl_ast::{AstNode, Board, SourceFile};
use bhdl_ast::items::PatternType;
use crate::passes::InstanceRegistry;
use crate::types::Diagnostic;
use rowan::TextRange;

/// Result of power domain expansion
#[derive(Debug, Clone)]
pub struct PowerDomainExpansion {
    /// Expanded connections from power domain to loads
    pub connections: Vec<ExpandedConnection>,
    /// Generated decoupling capacitors
    pub decoupling_caps: Vec<DecouplingCapacitor>,
    /// Diagnostics generated during expansion
    pub diagnostics: Vec<Diagnostic>,
}

/// Expanded connection from source to load
#[derive(Debug, Clone)]
pub struct ExpandedConnection {
    /// Source net name (e.g., "VCC_3V3")
    pub source_net: String,
    /// Component instance name
    pub component: String,
    /// Pin name on the component
    pub pin: String,
}

/// Generated decoupling capacitor
#[derive(Debug, Clone)]
pub struct DecouplingCapacitor {
    /// Generated instance name (e.g., "C_DECOUP_1")
    pub instance_name: String,
    /// Capacitance value expression
    pub value: String,
    /// Placement constraint (near which component)
    pub near_component: Option<String>,
    /// Is this distributed (not near a specific component)
    pub is_distributed: bool,
    /// Power domain this capacitor belongs to
    pub domain: String,
}

impl PowerDomainExpansion {
    /// Create a new empty expansion result
    pub fn new() -> Self {
        Self {
            connections: Vec::new(),
            decoupling_caps: Vec::new(),
            diagnostics: Vec::new(),
        }
    }
}

/// Expand all power domains in a source file
pub fn expand_power_domains(
    source_file: &SourceFile,
    instance_registry: &InstanceRegistry,
) -> PowerDomainExpansion {
    let mut expansion = PowerDomainExpansion::new();

    // Find all boards in the source file
    for item in source_file.items() {
        if let Some(board) = Board::cast(item.syntax().clone()) {
            expand_board_power_domains(&board, instance_registry, &mut expansion);
        }
    }

    expansion
}

/// Expand power domains in a single board
fn expand_board_power_domains(board: &Board, instance_registry: &InstanceRegistry, expansion: &mut PowerDomainExpansion) {
    // Process each power domain
    for power_domain in board.power_domains() {
        expand_single_power_domain(&power_domain, instance_registry, expansion);
    }
}

/// Expand a single power domain
fn expand_single_power_domain(domain: &PowerDomain, instance_registry: &InstanceRegistry, expansion: &mut PowerDomainExpansion) {
    // Get the net name
    let net_name = match domain.net_name() {
        Some(name) => name,
        None => {
            expansion.diagnostics.push(Diagnostic {
                message: "Power domain missing net name".to_string(),
                range: TextRange::empty(rowan::TextSize::from(0)),
            });
            return;
        }
    };

    println!("Expanding power domain: @{}", net_name);

    // Validate voltage and current specifications
    if domain.voltage().is_none() {
        expansion.diagnostics.push(Diagnostic {
            message: format!("Power domain @{} missing voltage specification", net_name),
            range: TextRange::empty(rowan::TextSize::from(0)),
        });
    }

    if domain.current().is_none() {
        expansion.diagnostics.push(Diagnostic {
            message: format!("Power domain @{} missing current specification", net_name),
            range: TextRange::empty(rowan::TextSize::from(0)),
        });
    }

    // Expand distribution block (create connections from domain to all loads)
    if let Some(distribution_block) = domain.distribution_block() {
        expand_distribution(&net_name, &distribution_block, instance_registry, expansion);
    }

    // Expand decoupling block (generate capacitor instances)
    if let Some(decoupling_block) = domain.decoupling_block() {
        expand_decoupling(&net_name, &decoupling_block, expansion);
    }

    // TODO: Validate constraints
}

/// Expand distribution block into explicit connections
fn expand_distribution(
    net_name: &str,
    distribution_block: &bhdl_ast::DistributionBlock,
    instance_registry: &InstanceRegistry,
    expansion: &mut PowerDomainExpansion,
) {
    for pin_list in distribution_block.pin_lists() {
        expand_pin_list(net_name, &pin_list, instance_registry, expansion);
    }
}

/// Expand a single pin list (may include wildcards or ranges or hierarchical paths)
fn expand_pin_list(
    net_name: &str,
    pin_list: &DistributionPinList,
    instance_registry: &InstanceRegistry,
    expansion: &mut PowerDomainExpansion,
) {
    // Phase 3: Check for hierarchical path (more than 2 segments)
    // Examples:
    //   fpga.VCCO[0..7] -> 2 segments (simple)
    //   sensor_board[*].sensor.VCC -> 3 segments (hierarchical)
    if pin_list.is_hierarchical() {
        let full_path = pin_list.full_path();
        expand_hierarchical_path(net_name, &full_path, instance_registry, expansion);
        return;
    }

    // For non-hierarchical paths, extract component and pin name
    let component = match pin_list.component() {
        Some(c) => c,
        None => {
            expansion.diagnostics.push(Diagnostic {
                message: "Distribution pin list missing component name".to_string(),
                range: TextRange::empty(rowan::TextSize::from(0)),
            });
            return;
        }
    };

    let pin_name = match pin_list.pin_name() {
        Some(p) => p,
        None => {
            expansion.diagnostics.push(Diagnostic {
                message: format!("Distribution pin list for {} missing pin name", component),
                range: TextRange::empty(rowan::TextSize::from(0)),
            });
            return;
        }
    };

    // Check if this is a simple pin reference (no brackets at all)
    let full_path = pin_list.full_path();
    if !full_path.contains('[') {
        // Simple pin reference without any patterns: mcu.VCC
        expansion.connections.push(ExpandedConnection {
            source_net: net_name.to_string(),
            component,
            pin: pin_name,
        });
        println!("  Added connection: @{} -> {}.{}", net_name, expansion.connections.last().unwrap().component, expansion.connections.last().unwrap().pin);
        return;
    }

    // Phase 4: Advanced Pattern Matching - Use pattern_type() for all bracket patterns
    let pattern = pin_list.pattern_params();

    match pattern.pattern_type {
        PatternType::Wildcard => {
            // Expand wildcard [*] by finding all matching component instances
            expand_wildcard_instances(net_name, &component, &pin_name, instance_registry, expansion);
        }

        PatternType::SimpleRange(start, end) => {
            // Simple range: [0..7]
            for i in start..=end {
                expansion.connections.push(ExpandedConnection {
                    source_net: net_name.to_string(),
                    component: component.clone(),
                    pin: format!("{}[{}]", pin_name, i),
                });
            }
            println!(
                "  Expanded range {}.{}[{}..{}] -> {} connections",
                component, pin_name, start, end, (end - start + 1)
            );
        }

        PatternType::SteppedRange(start, end, step) => {
            // Stepped range: [0..15:3]
            let mut i = start;
            let mut count = 0;
            while i <= end {
                expansion.connections.push(ExpandedConnection {
                    source_net: net_name.to_string(),
                    component: component.clone(),
                    pin: format!("{}[{}]", pin_name, i),
                });
                i += step;
                count += 1;
            }
            println!(
                "  Expanded stepped range {}.{}[{}..{}:{}] -> {} connections",
                component, pin_name, start, end, step, count
            );
        }

        PatternType::ExplicitList(indices) => {
            // Explicit list: [0,5,10,15]
            for i in &indices {
                expansion.connections.push(ExpandedConnection {
                    source_net: net_name.to_string(),
                    component: component.clone(),
                    pin: format!("{}[{}]", pin_name, i),
                });
            }
            println!(
                "  Expanded explicit list {}.{}[{:?}] -> {} connections",
                component, pin_name, indices, indices.len()
            );
        }

        PatternType::EvenKeyword => {
            // Even keyword: [even] - match all instances with even indices
            let matches = instance_registry.find_wildcard_matches(&component);
            let mut even_matches = Vec::new();

            for instance_name in matches {
                if let Some(index) = extract_index_from_name(&instance_name) {
                    if index % 2 == 0 {
                        expansion.connections.push(ExpandedConnection {
                            source_net: net_name.to_string(),
                            component: instance_name.clone(),
                            pin: pin_name.clone(),
                        });
                        even_matches.push(instance_name);
                    }
                }
            }

            if even_matches.is_empty() {
                expansion.diagnostics.push(Diagnostic {
                    message: format!(
                        "Even pattern {}[even].{} found no matching instances with even indices",
                        component, pin_name
                    ),
                    range: TextRange::empty(rowan::TextSize::from(0)),
                });
            } else {
                println!(
                    "  Expanded even pattern {}[even].{} -> {} connections",
                    component, pin_name, even_matches.len()
                );
            }
        }

        PatternType::OddKeyword => {
            // Odd keyword: [odd] - match all instances with odd indices
            let matches = instance_registry.find_wildcard_matches(&component);
            let mut odd_matches = Vec::new();

            for instance_name in matches {
                if let Some(index) = extract_index_from_name(&instance_name) {
                    if index % 2 == 1 {
                        expansion.connections.push(ExpandedConnection {
                            source_net: net_name.to_string(),
                            component: instance_name.clone(),
                            pin: pin_name.clone(),
                        });
                        odd_matches.push(instance_name);
                    }
                }
            }

            if odd_matches.is_empty() {
                expansion.diagnostics.push(Diagnostic {
                    message: format!(
                        "Odd pattern {}[odd].{} found no matching instances with odd indices",
                        component, pin_name
                    ),
                    range: TextRange::empty(rowan::TextSize::from(0)),
                });
            } else {
                println!(
                    "  Expanded odd pattern {}[odd].{} -> {} connections",
                    component, pin_name, odd_matches.len()
                );
            }
        }
    }
}

/// Extract a number from an expression (for range expansion)
fn extract_number_from_expr(expr: &Expr) -> Option<i32> {
    // Get the text representation of the expression
    let text = expr.syntax().text().to_string();
    // Try to parse as integer
    text.trim().parse().ok()
}

/// Extract numeric index from instance name (Phase 4: Advanced Patterns)
/// Examples:
///   "sensor_0" -> Some(0)
///   "sensor[5]" -> Some(5)
///   "sensor7" -> Some(7)
///   "sensor_a" -> None
fn extract_index_from_name(name: &str) -> Option<i32> {
    // Try array notation first: sensor[5]
    if let Some(start) = name.find('[') {
        if let Some(end) = name.find(']') {
            let index_str = &name[start+1..end];
            return index_str.parse().ok();
        }
    }

    // Try underscore notation: sensor_0
    if let Some(pos) = name.rfind('_') {
        let suffix = &name[pos+1..];
        if suffix.chars().all(|c| c.is_numeric()) && !suffix.is_empty() {
            return suffix.parse().ok();
        }
    }

    // Try trailing digits: sensor0
    let digits: String = name.chars()
        .rev()
        .take_while(|c| c.is_numeric())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    if !digits.is_empty() {
        return digits.parse().ok();
    }

    None
}

/// Expand hierarchical path (Phase 3: Hierarchical Wildcards)
/// Handles paths like "sensor_board[*].sensor.VCC" or "array.*sensor.VCC"
fn expand_hierarchical_path(
    net_name: &str,
    full_path: &str,
    instance_registry: &InstanceRegistry,
    expansion: &mut PowerDomainExpansion,
) {
    println!("  Expanding hierarchical path: {}", full_path);

    // Use the instance registry's hierarchical expansion
    let expanded_paths = instance_registry.expand_hierarchical_wildcard(full_path);

    if expanded_paths.is_empty() {
        expansion.diagnostics.push(Diagnostic {
            message: format!("Hierarchical path '{}' found no matching instances", full_path),
            range: TextRange::empty(rowan::TextSize::from(0)),
        });
        return;
    }

    // Each expanded path is in the format "module_instance.component_instance.pin"
    // We need to split it to get component and pin
    for path in &expanded_paths {
        // Split by last dot to get component path and pin name
        if let Some(last_dot) = path.rfind('.') {
            let component_path = &path[..last_dot];
            let pin = &path[last_dot + 1..];

            expansion.connections.push(ExpandedConnection {
                source_net: net_name.to_string(),
                component: component_path.to_string(),
                pin: pin.to_string(),
            });
        }
    }

    println!("  Expanded hierarchical path to {} connection(s)", expanded_paths.len());
}

/// Expand wildcard instances (Phase 2: Scalability)
/// Finds all component instances matching the base name pattern using the instance registry
fn expand_wildcard_instances(
    net_name: &str,
    base_component: &str,
    pin_name: &str,
    instance_registry: &InstanceRegistry,
    expansion: &mut PowerDomainExpansion,
) {
    println!("  Expanding wildcard: {}[*].{}", base_component, pin_name);

    // Use registry to find matching instances
    let matches = instance_registry.find_wildcard_matches(base_component);

    if matches.is_empty() {
        // Generate helpful error message with suggestions
        let error_msg = generate_wildcard_error_message(base_component, instance_registry);

        expansion.diagnostics.push(Diagnostic {
            message: error_msg,
            range: TextRange::empty(rowan::TextSize::from(0)),
        });
        return;
    }

    // Create connections for each matching instance
    for instance_name in &matches {
        expansion.connections.push(ExpandedConnection {
            source_net: net_name.to_string(),
            component: instance_name.clone(),
            pin: pin_name.to_string(),
        });
    }

    println!("  Expanded wildcard to {} instance(s): {}", matches.len(), matches.join(", "));
}

/// Generate a helpful error message for failed wildcard expansion
/// Includes suggestions for similar instance names using fuzzy matching
fn generate_wildcard_error_message(base_component: &str, instance_registry: &InstanceRegistry) -> String {
    let mut message = format!("Wildcard expansion for '{}[*]' found no matching instances", base_component);

    // Try to find similar base names (max edit distance of 2)
    let similar = instance_registry.find_similar_base_names(base_component, 2);

    if !similar.is_empty() {
        // Get the most similar match
        let (best_match, _distance) = &similar[0];

        // Check if this match actually has instances
        let instances = instance_registry.find_wildcard_matches(best_match);

        if !instances.is_empty() {
            message.push_str(&format!("\n  Help: Did you mean '{}'? (found {} instance{}: {})",
                best_match,
                instances.len(),
                if instances.len() == 1 { "" } else { "s" },
                instances.join(", ")
            ));
        } else if similar.len() > 1 {
            // Try the next best match
            let (second_match, _) = &similar[1];
            let instances = instance_registry.find_wildcard_matches(second_match);

            if !instances.is_empty() {
                message.push_str(&format!("\n  Help: Did you mean '{}'? (found {} instance{}: {})",
                    second_match,
                    instances.len(),
                    if instances.len() == 1 { "" } else { "s" },
                    instances.join(", ")
                ));
            }
        }
    }

    // If no similar names found, list all available base names
    if similar.is_empty() || similar.iter().all(|(name, _)| instance_registry.find_wildcard_matches(name).is_empty()) {
        let all_names = instance_registry.get_instance_names();
        if !all_names.is_empty() {
            let sample_count = std::cmp::min(5, all_names.len());
            let sample: Vec<_> = all_names.iter().take(sample_count).map(|s| s.as_str()).collect();
            message.push_str(&format!("\n  Available instances: {}{}",
                sample.join(", "),
                if all_names.len() > sample_count {
                    format!(" (and {} more)", all_names.len() - sample_count)
                } else {
                    String::new()
                }
            ));
        }
    }

    message
}

/// Check if a symbol name matches a wildcard pattern
/// Supports patterns like:
/// - "sensor[0]" matches base "sensor"
/// - "sensor_0" matches base "sensor"
/// - "sensor0" matches base "sensor"
fn is_wildcard_match(symbol_name: &str, base_name: &str) -> bool {
    // Exact prefix match with array notation: sensor[0], sensor[1]
    if symbol_name.starts_with(base_name) && symbol_name[base_name.len()..].starts_with('[') {
        return true;
    }

    // Prefix match with underscore separator: sensor_0, sensor_1
    if symbol_name.starts_with(base_name) && symbol_name.len() > base_name.len() {
        let remainder = &symbol_name[base_name.len()..];
        if remainder.starts_with('_') && remainder[1..].chars().all(|c| c.is_numeric()) {
            return true;
        }
    }

    // Prefix match with direct number: sensor0, sensor1
    if symbol_name.starts_with(base_name) && symbol_name.len() > base_name.len() {
        let remainder = &symbol_name[base_name.len()..];
        if remainder.chars().all(|c| c.is_numeric()) {
            return true;
        }
    }

    false
}

/// Expand decoupling block into capacitor instances
fn expand_decoupling(
    net_name: &str,
    decoupling_block: &bhdl_ast::DecouplingBlock,
    expansion: &mut PowerDomainExpansion,
) {
    let mut cap_counter = expansion.decoupling_caps.len();

    for rule in decoupling_block.rules() {
        expand_decoupling_rule(&rule, net_name, &mut cap_counter, expansion);
    }
}

/// Expand a single decoupling rule
fn expand_decoupling_rule(
    rule: &DecouplingRule,
    net_name: &str,
    cap_counter: &mut usize,
    expansion: &mut PowerDomainExpansion,
) {
    // Get placement information
    let (near_component, is_distributed) = if rule.is_near() {
        (rule.near_component(), false)
    } else if rule.is_distributed() {
        (None, true)
    } else {
        expansion.diagnostics.push(Diagnostic {
            message: "Decoupling rule must be either 'near' or 'distributed'".to_string(),
            range: TextRange::empty(rowan::TextSize::from(0)),
        });
        return;
    };

    let has_each = rule.has_each();

    // Process each capacitor specification
    for cap_spec in rule.cap_specs() {
        expand_cap_spec(
            &cap_spec,
            &near_component,
            is_distributed,
            has_each,
            net_name,
            cap_counter,
            expansion,
        );
    }
}

/// Expand a single capacitor specification
fn expand_cap_spec(
    cap_spec: &CapSpec,
    near_component: &Option<String>,
    is_distributed: bool,
    _has_each: bool,
    net_name: &str,
    cap_counter: &mut usize,
    expansion: &mut PowerDomainExpansion,
) {
    // Extract capacitance value
    let value_str = match cap_spec.value() {
        Some(expr) => expr.syntax().text().to_string(),
        None => {
            expansion.diagnostics.push(Diagnostic {
                message: "Capacitor specification missing value".to_string(),
                range: TextRange::empty(rowan::TextSize::from(0)),
            });
            return;
        }
    };

    // Extract count
    let count = match cap_spec.count() {
        Some(expr) => extract_number_from_expr(&expr).unwrap_or(1),
        None => 1,
    };

    // Generate capacitor instances
    for _ in 0..count {
        *cap_counter += 1;
        let instance_name = format!("C_DECOUP_{}", cap_counter);

        expansion.decoupling_caps.push(DecouplingCapacitor {
            instance_name: instance_name.clone(),
            value: value_str.clone(),
            near_component: near_component.clone(),
            is_distributed,
            domain: net_name.to_string(),
        });

        if is_distributed {
            println!("  Generated distributed cap: {} = {}", instance_name, value_str);
        } else if let Some(ref comp) = near_component {
            println!("  Generated cap: {} = {} (near {})", instance_name, value_str, comp);
        }
    }
}
