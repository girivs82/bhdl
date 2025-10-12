//! Connection summary generator

use super::{DocumentationContext, DocumentationError};
use std::collections::HashMap;

/// Generate detailed connection summary per domain
pub fn generate_connection_summary(context: &DocumentationContext) -> Result<String, DocumentationError> {
    let mut output = String::new();

    output.push_str("## Power Domain Connections\n\n");

    // Group connections by domain
    let mut domain_connections: HashMap<String, Vec<&crate::passes::power_domain_expansion::ExpandedConnection>> = HashMap::new();

    for conn in &context.expansion.connections {
        domain_connections.entry(conn.source_net.clone())
            .or_insert_with(Vec::new)
            .push(conn);
    }

    // Group decoupling by domain
    let mut domain_decoupling: HashMap<String, Vec<&crate::passes::power_domain_expansion::DecouplingCapacitor>> = HashMap::new();

    for cap in &context.expansion.decoupling_caps {
        domain_decoupling.entry(cap.domain.clone())
            .or_insert_with(Vec::new)
            .push(cap);
    }

    // Sort domains for consistent output
    let mut sorted_domains: Vec<_> = domain_connections.keys().collect();
    sorted_domains.sort();

    for domain_name in sorted_domains {
        let connections = &domain_connections[domain_name];

        output.push_str(&format!("### @{}\n\n", domain_name));

        // Connection list
        output.push_str(&format!("**Connections** ({} total):\n\n", connections.len()));

        // Analyze connection patterns
        let patterns = analyze_connection_patterns(connections);

        if !patterns.is_empty() && context.options.show_patterns {
            output.push_str("*Pattern Expansion:*\n");
            for (pattern, count) in &patterns {
                output.push_str(&format!("- {}: {} connections\n", pattern, count));
            }
            output.push_str("\n");
        }

        // List all connections (limit to first 20 for readability)
        let show_limit = 20;
        for (i, conn) in connections.iter().enumerate() {
            if i < show_limit {
                output.push_str(&format!("- {} → {}.{}\n", domain_name, conn.component, conn.pin));
            } else {
                output.push_str(&format!("- ... and {} more connections\n", connections.len() - show_limit));
                break;
            }
        }

        output.push_str("\n");

        // Decoupling summary
        if let Some(decoupling) = domain_decoupling.get(domain_name) {
            output.push_str(&format!("**Decoupling** ({} capacitors):\n\n", decoupling.len()));

            // Group by placement type
            let near_caps: Vec<_> = decoupling.iter().filter(|c| !c.is_distributed).collect();
            let dist_caps: Vec<_> = decoupling.iter().filter(|c| c.is_distributed).collect();

            if !near_caps.is_empty() {
                output.push_str("*Near-component placement:*\n");
                let near_summary = summarize_capacitors(&near_caps);
                for (value, count) in near_summary {
                    output.push_str(&format!("- {}× {}\n", count, value));
                }
                output.push_str("\n");
            }

            if !dist_caps.is_empty() {
                output.push_str("*Distributed placement:*\n");
                let dist_summary = summarize_capacitors(&dist_caps);
                for (value, count) in dist_summary {
                    output.push_str(&format!("- {}× {}\n", count, value));
                }
                output.push_str("\n");
            }
        }

        output.push_str("---\n\n");
    }

    Ok(output)
}

/// Analyze connection patterns to identify wildcards, ranges, etc.
fn analyze_connection_patterns(connections: &[&crate::passes::power_domain_expansion::ExpandedConnection]) -> Vec<(String, usize)> {
    let mut patterns = Vec::new();
    let mut component_pins: HashMap<String, Vec<String>> = HashMap::new();

    // Group by component
    for conn in connections {
        component_pins.entry(conn.component.clone())
            .or_insert_with(Vec::new)
            .push(conn.pin.clone());
    }

    // Detect patterns
    for (component, pins) in &component_pins {
        if pins.len() > 1 {
            // Check if it's a range pattern
            if is_range_pattern(component) {
                patterns.push((format!("{}[*] (range)", base_name(component)), pins.len()));
            } else if pins.len() > 1 {
                // Multiple pins on same component
                patterns.push((format!("{}.* ({} pins)", component, pins.len()), pins.len()));
            }
        }
    }

    patterns
}

/// Check if component name suggests a range pattern
fn is_range_pattern(name: &str) -> bool {
    name.contains('[') || name.contains('_') && name.chars().last().map_or(false, |c| c.is_numeric())
}

/// Extract base name from component
fn base_name(name: &str) -> String {
    if let Some(pos) = name.find('[') {
        name[..pos].to_string()
    } else if let Some(pos) = name.rfind('_') {
        if name[pos+1..].chars().all(|c| c.is_numeric()) {
            name[..pos].to_string()
        } else {
            name.to_string()
        }
    } else {
        name.to_string()
    }
}

/// Summarize capacitors by value
fn summarize_capacitors(caps: &[&&crate::passes::power_domain_expansion::DecouplingCapacitor]) -> Vec<(String, usize)> {
    let mut value_counts: HashMap<String, usize> = HashMap::new();

    for cap in caps {
        *value_counts.entry(cap.value.clone()).or_insert(0) += 1;
    }

    let mut result: Vec<_> = value_counts.into_iter().collect();
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_range_pattern() {
        assert!(is_range_pattern("sensor[0]"));
        assert!(is_range_pattern("sensor_0"));
        assert!(!is_range_pattern("mcu"));
    }

    #[test]
    fn test_base_name() {
        assert_eq!(base_name("sensor[0]"), "sensor");
        assert_eq!(base_name("sensor_0"), "sensor");
        assert_eq!(base_name("mcu"), "mcu");
    }
}
