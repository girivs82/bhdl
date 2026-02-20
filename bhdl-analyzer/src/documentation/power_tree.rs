//! Power tree hierarchy generator

use super::{DocumentationContext, DocumentationError};
use std::collections::HashMap;

/// Generate power tree showing voltage conversion hierarchy
pub fn generate_power_tree(context: &DocumentationContext) -> Result<String, DocumentationError> {
    let mut output = String::new();

    output.push_str("## Power Tree\n\n");
    output.push_str("```\n");

    // Build tree structure from connections
    let tree = build_power_tree(context)?;

    // Render tree
    render_tree_node(&tree, &mut output, 0)?;

    output.push_str("```\n\n");

    Ok(output)
}

/// Power tree node representing a domain or component
#[derive(Debug, Clone)]
struct PowerTreeNode {
    name: String,
    node_type: NodeType,
    voltage: Option<String>,
    current: Option<f64>,
    children: Vec<PowerTreeNode>,
    component_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeType {
    PowerDomain,
    Converter,
    LoadGroup,
}

/// Build power tree structure from expansion data
fn build_power_tree(context: &DocumentationContext) -> Result<PowerTreeNode, DocumentationError> {
    // Group connections by source domain
    let mut domain_children: HashMap<String, Vec<String>> = HashMap::new();

    for conn in &context.expansion.connections {
        domain_children
            .entry(conn.source_net.clone())
            .or_insert_with(Vec::new)
            .push(conn.component.clone());
    }

    // Find root domains (those not derived from others)
    let root_domains = find_root_domains(context);

    // Build tree starting from root
    let mut root_children = Vec::new();
    for domain_name in root_domains {
        let node = build_domain_node(domain_name, context, &domain_children)?;
        root_children.push(node);
    }

    Ok(PowerTreeNode {
        name: "Power Distribution".to_string(),
        node_type: NodeType::PowerDomain,
        voltage: None,
        current: None,
        children: root_children,
        component_count: 0,
    })
}

/// Find root power domains (not derived from others)
fn find_root_domains(context: &DocumentationContext) -> Vec<String> {
    let mut domains: Vec<String> = context
        .expansion
        .connections
        .iter()
        .map(|c| c.source_net.clone())
        .collect();

    domains.sort();
    domains.dedup();
    domains
}

/// Build tree node for a power domain
fn build_domain_node(
    domain_name: String,
    context: &DocumentationContext,
    domain_children: &HashMap<String, Vec<String>>,
) -> Result<PowerTreeNode, DocumentationError> {
    // Count components in this domain
    let component_count = domain_children
        .get(&domain_name)
        .map(|v| v.len())
        .unwrap_or(0);

    // Calculate total current draw if metadata available
    let total_current = if let Some(components) = domain_children.get(&domain_name) {
        components
            .iter()
            .filter_map(|comp| {
                context
                    .component_metadata
                    .get(comp)
                    .and_then(|m| m.typical_current)
            })
            .sum::<f64>()
    } else {
        0.0
    };

    let current = if total_current > 0.0 {
        Some(total_current)
    } else {
        None
    };

    // Get voltage from domain attributes (future enhancement)
    let voltage = None; // TODO: Extract from power domain declaration

    Ok(PowerTreeNode {
        name: domain_name,
        node_type: NodeType::PowerDomain,
        voltage,
        current,
        children: Vec::new(),
        component_count,
    })
}

/// Render tree node with proper indentation
fn render_tree_node(
    node: &PowerTreeNode,
    output: &mut String,
    depth: usize,
) -> Result<(), DocumentationError> {
    let indent = "  ".repeat(depth);
    let prefix = if depth == 0 {
        "".to_string()
    } else {
        "├─ ".to_string()
    };

    // Format node info
    let mut info_parts = vec![node.name.clone()];

    if let Some(voltage) = &node.voltage {
        info_parts.push(format!("({})", voltage));
    }

    if let Some(current) = node.current {
        info_parts.push(format!("[{:.0} mA]", current * 1000.0));
    }

    if node.component_count > 0 {
        info_parts.push(format!("→ {} components", node.component_count));
    }

    output.push_str(&format!("{}{}{}\n", indent, prefix, info_parts.join(" ")));

    // Render children
    for child in &node.children {
        render_tree_node(child, output, depth + 1)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_root_domains() {
        // Test with empty context
        let context = DocumentationContext::new(
            crate::passes::power_domain_expansion::PowerDomainExpansion {
                connections: Vec::new(),
                decoupling_caps: Vec::new(),
                diagnostics: Vec::new(),
            },
            super::super::DocumentationOptions::default(),
        );

        let roots = find_root_domains(&context);
        assert_eq!(roots.len(), 0);
    }

    #[test]
    fn test_render_simple_node() {
        let node = PowerTreeNode {
            name: "VCC".to_string(),
            node_type: NodeType::PowerDomain,
            voltage: Some("5V".to_string()),
            current: Some(0.1),
            children: Vec::new(),
            component_count: 3,
        };

        let mut output = String::new();
        render_tree_node(&node, &mut output, 0).unwrap();

        assert!(output.contains("VCC"));
        assert!(output.contains("5V"));
        assert!(output.contains("100 mA"));
        assert!(output.contains("3 components"));
    }
}
