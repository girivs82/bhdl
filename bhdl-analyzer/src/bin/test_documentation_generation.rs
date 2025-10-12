//! Test documentation generation for power domains

use bhdl_analyzer::documentation::{
    generate_documentation, DocumentationOptions, OutputFormat,
};
use bhdl_analyzer::passes::power_domain_expansion::{
    PowerDomainExpansion, ExpandedConnection, DecouplingCapacitor,
};

fn main() {
    println!("=== Power Domain Documentation Generation Test ===\n");

    // Create sample power domain expansion data
    let expansion = create_sample_expansion();

    // Generate documentation with all sections
    let options = DocumentationOptions {
        format: OutputFormat::Markdown,
        include_power_tree: true,
        include_bom: true,
        include_budget: true,
        include_connections: true,
        include_summary: true,
        show_patterns: true,
    };

    match generate_documentation(&expansion, options) {
        Ok(doc) => {
            println!("{}", doc);
            println!("\n=== Documentation Generation Complete ===");
        }
        Err(e) => {
            eprintln!("Error generating documentation: {}", e);
            std::process::exit(1);
        }
    }
}

/// Create sample power domain expansion for testing
fn create_sample_expansion() -> PowerDomainExpansion {
    let mut expansion = PowerDomainExpansion::new();

    // VCC_3V3 domain connections
    for i in 0..8 {
        expansion.connections.push(ExpandedConnection {
            source_net: "VCC_3V3".to_string(),
            component: format!("sensor_{}", i),
            pin: "VCC".to_string(),
        });
    }

    // Add MCU connection
    expansion.connections.push(ExpandedConnection {
        source_net: "VCC_3V3".to_string(),
        component: "mcu".to_string(),
        pin: "VDDA".to_string(),
    });

    // VCC_5V domain connections
    expansion.connections.push(ExpandedConnection {
        source_net: "VCC_5V".to_string(),
        component: "motor_driver".to_string(),
        pin: "VCC".to_string(),
    });

    for i in 0..4 {
        expansion.connections.push(ExpandedConnection {
            source_net: "VCC_5V".to_string(),
            component: format!("led_{}", i),
            pin: "A".to_string(),
        });
    }

    // Decoupling capacitors for VCC_3V3
    expansion.decoupling_caps.push(DecouplingCapacitor {
        instance_name: "C_DECOUP_1".to_string(),
        value: "100nF".to_string(),
        near_component: Some("mcu".to_string()),
        is_distributed: false,
        domain: "VCC_3V3".to_string(),
    });

    expansion.decoupling_caps.push(DecouplingCapacitor {
        instance_name: "C_DECOUP_2".to_string(),
        value: "10µF".to_string(),
        near_component: Some("mcu".to_string()),
        is_distributed: false,
        domain: "VCC_3V3".to_string(),
    });

    // Distributed capacitors for VCC_3V3
    for i in 0..4 {
        expansion.decoupling_caps.push(DecouplingCapacitor {
            instance_name: format!("C_DECOUP_{}", i + 3),
            value: "100nF".to_string(),
            near_component: None,
            is_distributed: true,
            domain: "VCC_3V3".to_string(),
        });
    }

    // Decoupling capacitors for VCC_5V
    expansion.decoupling_caps.push(DecouplingCapacitor {
        instance_name: "C_DECOUP_7".to_string(),
        value: "220µF".to_string(),
        near_component: Some("motor_driver".to_string()),
        is_distributed: false,
        domain: "VCC_5V".to_string(),
    });

    expansion.decoupling_caps.push(DecouplingCapacitor {
        instance_name: "C_DECOUP_8".to_string(),
        value: "100nF".to_string(),
        near_component: Some("motor_driver".to_string()),
        is_distributed: false,
        domain: "VCC_5V".to_string(),
    });

    expansion
}
