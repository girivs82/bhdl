//! Bill of Materials (BOM) generator

use super::{DocumentationContext, DocumentationError};
use std::collections::HashMap;

/// Generate Bill of Materials for power domain components
pub fn generate_bom(context: &DocumentationContext) -> Result<String, DocumentationError> {
    let mut output = String::new();

    output.push_str("## Bill of Materials\n\n");

    // Collect and group components
    let bom_items = collect_bom_items(context);

    // Generate decoupling capacitor BOM
    if !bom_items.capacitors.is_empty() {
        output.push_str("### Decoupling Capacitors\n\n");
        output.push_str("| Ref Des | Value | Quantity | Type | Voltage | Placement |\n");
        output.push_str("|---------|-------|----------|------|---------|-----------|\\n");

        for (value, items) in &bom_items.capacitors {
            let total_qty: usize = items.iter().map(|i| i.quantity).sum();
            let placement = if items[0].is_distributed {
                "Distributed"
            } else {
                "Near-component"
            };

            output.push_str(&format!(
                "| {} | {} | {} | Ceramic | {} | {} |\n",
                format_ref_des_range(&items),
                value,
                total_qty,
                estimate_voltage_rating(value),
                placement
            ));
        }

        output.push_str("\n");

        // Summary
        let total_caps: usize = bom_items.capacitors.values().flatten().map(|i| i.quantity).sum();
        let unique_values = bom_items.capacitors.len();
        output.push_str(&format!(
            "**Summary**: {} capacitors total, {} unique values\n\n",
            total_caps, unique_values
        ));
    }

    // Generate other components (future enhancement)
    // TODO: Add sections for regulators, protection devices, etc.

    Ok(output)
}

/// BOM item structure
#[derive(Debug, Clone)]
struct BomItem {
    value: String,
    quantity: usize,
    ref_designators: Vec<String>,
    is_distributed: bool,
    domain: String,
}

/// Collected BOM items by category
#[derive(Debug, Default)]
struct BomItems {
    capacitors: HashMap<String, Vec<BomItem>>,
    regulators: Vec<BomItem>,
    protection: Vec<BomItem>,
}

/// Collect and organize BOM items from expansion data
fn collect_bom_items(context: &DocumentationContext) -> BomItems {
    let mut bom = BomItems::default();

    // Process decoupling capacitors
    let mut cap_groups: HashMap<String, Vec<&crate::passes::power_domain_expansion::DecouplingCapacitor>> = HashMap::new();

    for cap in &context.expansion.decoupling_caps {
        cap_groups
            .entry(cap.value.clone())
            .or_insert_with(Vec::new)
            .push(cap);
    }

    // Create BOM items for each capacitor value
    for (value, caps) in cap_groups {
        let items = group_capacitors_by_placement(value.clone(), &caps);
        bom.capacitors.insert(value, items);
    }

    bom
}

/// Group capacitors by placement type
fn group_capacitors_by_placement(
    value: String,
    caps: &[&crate::passes::power_domain_expansion::DecouplingCapacitor],
) -> Vec<BomItem> {
    let mut items = Vec::new();

    // Group by distributed vs. near-component
    let distributed: Vec<_> = caps.iter().filter(|c| c.is_distributed).collect();
    let near_component: Vec<_> = caps.iter().filter(|c| !c.is_distributed).collect();

    if !distributed.is_empty() {
        items.push(BomItem {
            value: value.clone(),
            quantity: distributed.len(),
            ref_designators: generate_ref_designators("C", distributed.len()),
            is_distributed: true,
            domain: distributed[0].domain.clone(),
        });
    }

    if !near_component.is_empty() {
        items.push(BomItem {
            value: value.clone(),
            quantity: near_component.len(),
            ref_designators: generate_ref_designators("C", near_component.len()),
            is_distributed: false,
            domain: near_component[0].domain.clone(),
        });
    }

    items
}

/// Generate reference designators (C1, C2, C3, ...)
fn generate_ref_designators(prefix: &str, count: usize) -> Vec<String> {
    (1..=count)
        .map(|i| format!("{}{}", prefix, i))
        .collect()
}

/// Format reference designator range (e.g., "C1-C10")
fn format_ref_des_range(items: &[BomItem]) -> String {
    let all_refs: Vec<_> = items
        .iter()
        .flat_map(|i| i.ref_designators.iter())
        .collect();

    if all_refs.is_empty() {
        return "-".to_string();
    }

    if all_refs.len() == 1 {
        return all_refs[0].clone();
    }

    // Simple range format
    format!("{}-{}", all_refs[0], all_refs[all_refs.len() - 1])
}

/// Estimate voltage rating based on capacitance value
fn estimate_voltage_rating(value: &str) -> &'static str {
    // Parse value to determine typical voltage rating
    let value_lower = value.to_lowercase();

    if value_lower.contains("µf") || value_lower.contains("uf") {
        // Larger capacitors typically 6.3V-16V
        "16V"
    } else if value_lower.contains("nf") {
        // Smaller caps can be 25V-50V
        "25V"
    } else {
        // Default
        "16V"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_ref_designators() {
        let refs = generate_ref_designators("C", 5);
        assert_eq!(refs.len(), 5);
        assert_eq!(refs[0], "C1");
        assert_eq!(refs[4], "C5");
    }

    #[test]
    fn test_format_ref_des_range() {
        let items = vec![BomItem {
            value: "100nF".to_string(),
            quantity: 10,
            ref_designators: vec!["C1".to_string(), "C2".to_string(), "C10".to_string()],
            is_distributed: false,
            domain: "VCC".to_string(),
        }];

        let range = format_ref_des_range(&items);
        assert!(range.contains("C1"));
        assert!(range.contains("C10"));
    }

    #[test]
    fn test_estimate_voltage_rating() {
        assert_eq!(estimate_voltage_rating("100nF"), "25V");
        assert_eq!(estimate_voltage_rating("10µF"), "16V");
        assert_eq!(estimate_voltage_rating("10uF"), "16V");
    }
}
