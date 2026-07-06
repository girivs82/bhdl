//! Power Domain Documentation Generator
//!
//! This module provides automatic documentation generation for power domains,
//! including power trees, BOMs, budgets, and connection summaries.

pub mod context;
pub mod connection_summary;
pub mod voltage_summary;
pub mod power_tree;
pub mod bom_generator;
pub mod budget_analyzer;
pub mod formatters;

pub use context::{DocumentationContext, DocumentationOptions, OutputFormat, ComponentMetadata};
pub use connection_summary::generate_connection_summary;
pub use voltage_summary::generate_voltage_summary;
pub use power_tree::generate_power_tree;
pub use bom_generator::generate_bom;
pub use budget_analyzer::generate_budget_analysis;

use crate::passes::power_domain_expansion::PowerDomainExpansion;
use std::error::Error;
use std::fmt;

/// Error type for documentation generation
#[derive(Debug)]
pub enum DocumentationError {
    /// Missing required data
    MissingData(String),
    /// Invalid configuration
    InvalidConfig(String),
    /// Formatting error
    FormattingError(String),
}

impl fmt::Display for DocumentationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DocumentationError::MissingData(msg) => write!(f, "Missing data: {}", msg),
            DocumentationError::InvalidConfig(msg) => write!(f, "Invalid configuration: {}", msg),
            DocumentationError::FormattingError(msg) => write!(f, "Formatting error: {}", msg),
        }
    }
}

impl Error for DocumentationError {}

/// Generate complete documentation for power domains
pub fn generate_documentation(
    expansion: &PowerDomainExpansion,
    options: DocumentationOptions,
) -> Result<String, DocumentationError> {
    let context = DocumentationContext::new(expansion.clone(), options);

    let mut output = String::new();

    // Title
    output.push_str("# Power Domain Documentation\n\n");
    output.push_str(&format!("**Generated**: {}\n\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S")));

    // Generate requested sections
    if context.options.include_summary {
        output.push_str("---\n\n");
        output.push_str(&generate_voltage_summary(&context)?);
        output.push_str("\n\n");
    }

    if context.options.include_power_tree {
        output.push_str("---\n\n");
        output.push_str(&generate_power_tree(&context)?);
        output.push_str("\n\n");
    }

    if context.options.include_budget {
        output.push_str("---\n\n");
        output.push_str(&generate_budget_analysis(&context)?);
        output.push_str("\n\n");
    }

    if context.options.include_bom {
        output.push_str("---\n\n");
        output.push_str(&generate_bom(&context)?);
        output.push_str("\n\n");
    }

    if context.options.include_connections {
        output.push_str("---\n\n");
        output.push_str(&generate_connection_summary(&context)?);
        output.push_str("\n\n");
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_documentation_generation() {
        // Create a minimal power domain expansion
        let expansion = PowerDomainExpansion::new();

        let options = DocumentationOptions::default();
        let result = generate_documentation(&expansion, options);

        assert!(result.is_ok());
        let doc = result.unwrap();
        assert!(doc.contains("Power Domain Documentation"));
    }
}
