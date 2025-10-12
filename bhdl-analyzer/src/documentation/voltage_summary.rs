//! Voltage domain summary generator

use super::{DocumentationContext, DocumentationError};
use std::collections::HashMap;

/// Generate voltage domain summary table
pub fn generate_voltage_summary(context: &DocumentationContext) -> Result<String, DocumentationError> {
    let mut output = String::new();

    output.push_str("## Voltage Domain Summary\n\n");

    // Group connections by domain
    let mut domain_stats: HashMap<String, DomainStats> = HashMap::new();

    for conn in &context.expansion.connections {
        let stats = domain_stats.entry(conn.source_net.clone()).or_insert_with(DomainStats::default);
        stats.connection_count += 1;
        stats.components.insert(conn.component.clone());
    }

    // Group decoupling caps by domain
    for cap in &context.expansion.decoupling_caps {
        let stats = domain_stats.entry(cap.domain.clone()).or_insert_with(DomainStats::default);
        stats.decoupling_count += 1;
    }

    // Create table
    output.push_str("| Domain | Connections | Components | Decoupling | Total Capacitance |\n");
    output.push_str("|--------|-------------|------------|------------|-------------------|\n");

    let mut total_connections = 0;
    let mut total_components = 0;
    let mut total_decoupling = 0;

    // Sort domains for consistent output
    let mut sorted_domains: Vec<_> = domain_stats.keys().collect();
    sorted_domains.sort();

    for domain_name in sorted_domains {
        let stats = &domain_stats[domain_name];

        // Calculate total capacitance for this domain
        let total_cap = context.expansion.decoupling_caps
            .iter()
            .filter(|c| &c.domain == domain_name)
            .map(|c| parse_capacitance_value(&c.value))
            .sum::<f64>();

        let cap_str = format_capacitance(total_cap);

        output.push_str(&format!(
            "| @{} | {} | {} | {} caps | {} |\n",
            domain_name,
            stats.connection_count,
            stats.components.len(),
            stats.decoupling_count,
            cap_str
        ));

        total_connections += stats.connection_count;
        total_components += stats.components.len();
        total_decoupling += stats.decoupling_count;
    }

    // Add totals row
    output.push_str(&format!(
        "| **Total** | **{}** | **{}** | **{} caps** | - |\n",
        total_connections,
        total_components,
        total_decoupling
    ));

    output.push_str("\n");

    // Summary statistics
    output.push_str("### Statistics\n\n");
    output.push_str(&format!("- **Power Domains**: {}\n", domain_stats.len()));
    output.push_str(&format!("- **Total Connections**: {}\n", total_connections));
    output.push_str(&format!("- **Unique Components**: {}\n", total_components));
    output.push_str(&format!("- **Decoupling Capacitors**: {}\n", total_decoupling));

    Ok(output)
}

#[derive(Debug, Default)]
struct DomainStats {
    connection_count: usize,
    components: std::collections::HashSet<String>,
    decoupling_count: usize,
}

/// Parse capacitance value from string (e.g., "100nF", "10µF")
fn parse_capacitance_value(value_str: &str) -> f64 {
    let value_str = value_str.trim();

    // Extract number and unit
    let mut number_part = String::new();
    let mut unit_part = String::new();
    let mut in_number = true;

    for ch in value_str.chars() {
        if ch.is_numeric() || ch == '.' {
            number_part.push(ch);
        } else {
            in_number = false;
            unit_part.push(ch);
        }
    }

    let base_value: f64 = number_part.parse().unwrap_or(0.0);
    let unit = unit_part.trim();

    // Convert to Farads
    match unit {
        "pF" => base_value * 1e-12,
        "nF" => base_value * 1e-9,
        "µF" | "uF" => base_value * 1e-6,
        "mF" => base_value * 1e-3,
        "F" => base_value,
        _ => base_value * 1e-9, // Default to nF
    }
}

/// Format capacitance value in human-readable form
fn format_capacitance(farads: f64) -> String {
    if farads >= 1e-3 {
        format!("{:.2} mF", farads * 1e3)
    } else if farads >= 1e-6 {
        format!("{:.1} µF", farads * 1e6)
    } else if farads >= 1e-9 {
        format!("{:.0} nF", farads * 1e9)
    } else {
        format!("{:.0} pF", farads * 1e12)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_capacitance() {
        assert_eq!(parse_capacitance_value("100nF"), 100e-9);
        assert_eq!(parse_capacitance_value("10µF"), 10e-6);
        assert_eq!(parse_capacitance_value("1mF"), 1e-3);
    }

    #[test]
    fn test_format_capacitance() {
        assert_eq!(format_capacitance(100e-9), "100 nF");
        assert_eq!(format_capacitance(10e-6), "10.0 µF");
        assert_eq!(format_capacitance(1e-3), "1.00 mF");
    }
}
