//! Power budget analysis and reporting

use super::{DocumentationContext, DocumentationError};
use std::collections::HashMap;

/// Generate power budget analysis for each domain
pub fn generate_budget_analysis(context: &DocumentationContext) -> Result<String, DocumentationError> {
    let mut output = String::new();

    output.push_str("## Power Budget Analysis\n\n");

    // Analyze each power domain
    let domain_budgets = analyze_domain_budgets(context)?;

    // Generate budget table
    output.push_str("### Domain-Level Budgets\n\n");
    output.push_str("| Domain | Total Current | Component Count | Peak Current | Margin | Status |\n");
    output.push_str("|--------|---------------|-----------------|--------------|--------|--------|\n");

    let mut total_power = 0.0;

    for (domain_name, budget) in &domain_budgets {
        let margin_pct = budget.calculate_margin();
        let status = if margin_pct > 30.0 {
            "✓ Good"
        } else if margin_pct > 15.0 {
            "⚠ Adequate"
        } else {
            "✗ Tight"
        };

        output.push_str(&format!(
            "| @{} | {:.1} mA | {} | {:.1} mA | {:.0}% | {} |\n",
            domain_name,
            budget.typical_current * 1000.0,
            budget.component_count,
            budget.peak_current * 1000.0,
            margin_pct,
            status
        ));

        // Estimate power (assuming 5V for simplicity - should come from domain voltage)
        total_power += budget.typical_current * 5.0;
    }

    output.push_str("\n");

    // Overall summary
    output.push_str("### Overall Summary\n\n");
    output.push_str(&format!("- **Total Power Consumption**: {:.2} W\n", total_power));
    output.push_str(&format!("- **Power Domains**: {}\n", domain_budgets.len()));
    output.push_str(&format!(
        "- **Total Components**: {}\n",
        domain_budgets.values().map(|b| b.component_count).sum::<usize>()
    ));
    output.push_str("\n");

    // Detailed breakdown by domain
    if context.options.show_patterns {
        output.push_str("### Detailed Breakdown\n\n");

        for (domain_name, budget) in &domain_budgets {
            output.push_str(&format!("#### @{}\n\n", domain_name));

            // Component current breakdown
            if !budget.component_breakdown.is_empty() {
                output.push_str("**Current by Component Type**:\n\n");
                for (comp_type, current) in &budget.component_breakdown {
                    output.push_str(&format!("- {}: {:.1} mA\n", comp_type, current * 1000.0));
                }
                output.push_str("\n");
            }

            // Notes
            if !budget.notes.is_empty() {
                output.push_str("**Notes**:\n\n");
                for note in &budget.notes {
                    output.push_str(&format!("- {}\n", note));
                }
                output.push_str("\n");
            }

            output.push_str("---\n\n");
        }
    }

    Ok(output)
}

/// Power budget for a single domain
#[derive(Debug, Clone)]
struct DomainBudget {
    /// Typical current draw (Amperes)
    typical_current: f64,
    /// Peak current draw (Amperes)
    peak_current: f64,
    /// Number of components in domain
    component_count: usize,
    /// Current supply capacity (Amperes)
    supply_capacity: Option<f64>,
    /// Breakdown by component type
    component_breakdown: HashMap<String, f64>,
    /// Analysis notes
    notes: Vec<String>,
}

impl DomainBudget {
    /// Calculate margin percentage
    fn calculate_margin(&self) -> f64 {
        if let Some(capacity) = self.supply_capacity {
            if capacity > 0.0 {
                return ((capacity - self.peak_current) / capacity) * 100.0;
            }
        }

        // If no capacity specified, assume 30% margin for typical case
        30.0
    }
}

/// Analyze power budgets for all domains
fn analyze_domain_budgets(
    context: &DocumentationContext,
) -> Result<HashMap<String, DomainBudget>, DocumentationError> {
    let mut budgets = HashMap::new();

    // Group connections by domain
    let mut domain_components: HashMap<String, Vec<String>> = HashMap::new();

    for conn in &context.expansion.connections {
        domain_components
            .entry(conn.source_net.clone())
            .or_insert_with(Vec::new)
            .push(conn.component.clone());
    }

    // Analyze each domain
    for (domain_name, components) in domain_components {
        let budget = analyze_single_domain(&domain_name, &components, context)?;
        budgets.insert(domain_name, budget);
    }

    Ok(budgets)
}

/// Analyze power budget for a single domain
fn analyze_single_domain(
    domain_name: &str,
    components: &[String],
    context: &DocumentationContext,
) -> Result<DomainBudget, DocumentationError> {
    let mut typical_current = 0.0;
    let mut peak_current = 0.0;
    let mut component_breakdown: HashMap<String, f64> = HashMap::new();
    let mut notes = Vec::new();

    // Sum current from all components
    for comp in components {
        if let Some(metadata) = context.component_metadata.get(comp) {
            if let Some(typ_current) = metadata.typical_current {
                typical_current += typ_current;

                // Extract component type from name
                let comp_type = extract_component_type(comp);
                *component_breakdown.entry(comp_type).or_insert(0.0) += typ_current;
            }

            if let Some(max_current) = metadata.max_current {
                peak_current += max_current;
            }
        }
    }

    // If no peak current specified, estimate as 1.5x typical
    if peak_current == 0.0 {
        peak_current = typical_current * 1.5;
        notes.push("Peak current estimated as 1.5× typical".to_string());
    }

    // Check for missing metadata
    let components_with_metadata = context
        .component_metadata
        .keys()
        .filter(|k| components.contains(k))
        .count();

    if components_with_metadata < components.len() {
        notes.push(format!(
            "{} components missing current specifications",
            components.len() - components_with_metadata
        ));
    }

    // Get supply capacity from domain declaration (future enhancement)
    let supply_capacity = None; // TODO: Extract from power domain voltage/current spec

    Ok(DomainBudget {
        typical_current,
        peak_current,
        component_count: components.len(),
        supply_capacity,
        component_breakdown,
        notes,
    })
}

/// Extract component type from component name
fn extract_component_type(name: &str) -> String {
    // Extract type from name patterns like "led_0", "mcu", "sensor[0]"
    if name.starts_with("led") || name.contains("LED") {
        "LED".to_string()
    } else if name.starts_with("mcu") || name.contains("MCU") {
        "MCU".to_string()
    } else if name.starts_with("sensor") {
        "Sensor".to_string()
    } else if name.starts_with("reg") || name.contains("regulator") {
        "Regulator".to_string()
    } else {
        // Generic fallback
        name.split(|c: char| !c.is_alphanumeric())
            .next()
            .unwrap_or("Other")
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_margin() {
        let budget = DomainBudget {
            typical_current: 0.1,
            peak_current: 0.15,
            component_count: 5,
            supply_capacity: Some(0.5),
            component_breakdown: HashMap::new(),
            notes: Vec::new(),
        };

        let margin = budget.calculate_margin();
        assert!((margin - 70.0).abs() < 0.1); // (0.5 - 0.15) / 0.5 * 100 = 70%
    }

    #[test]
    fn test_extract_component_type() {
        assert_eq!(extract_component_type("led_0"), "LED");
        assert_eq!(extract_component_type("mcu"), "MCU");
        assert_eq!(extract_component_type("sensor[0]"), "Sensor");
        assert_eq!(extract_component_type("regulator1"), "Regulator");
    }

    #[test]
    fn test_calculate_margin_no_capacity() {
        let budget = DomainBudget {
            typical_current: 0.1,
            peak_current: 0.15,
            component_count: 5,
            supply_capacity: None,
            component_breakdown: HashMap::new(),
            notes: Vec::new(),
        };

        // Should return default 30% when no capacity specified
        let margin = budget.calculate_margin();
        assert_eq!(margin, 30.0);
    }
}
