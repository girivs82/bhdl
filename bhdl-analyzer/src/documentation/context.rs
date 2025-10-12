//! Documentation context and configuration

use crate::passes::power_domain_expansion::PowerDomainExpansion;
use std::collections::HashMap;

/// Component metadata for documentation
#[derive(Debug, Clone)]
pub struct ComponentMetadata {
    /// Typical current draw (A)
    pub typical_current: Option<f64>,
    /// Maximum current draw (A)
    pub max_current: Option<f64>,
    /// Component description
    pub description: Option<String>,
}

/// Output format for documentation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Markdown format
    Markdown,
    /// ASCII table format
    AsciiTable,
    /// HTML format
    Html,
}

/// Documentation generation options
#[derive(Debug, Clone)]
pub struct DocumentationOptions {
    /// Output format
    pub format: OutputFormat,
    /// Include power tree diagram
    pub include_power_tree: bool,
    /// Include BOM
    pub include_bom: bool,
    /// Include power budget analysis
    pub include_budget: bool,
    /// Include detailed connection summary
    pub include_connections: bool,
    /// Include voltage domain summary
    pub include_summary: bool,
    /// Show pattern expansion details
    pub show_patterns: bool,
}

impl Default for DocumentationOptions {
    fn default() -> Self {
        Self {
            format: OutputFormat::Markdown,
            include_power_tree: true,
            include_bom: true,
            include_budget: true,
            include_connections: true,
            include_summary: true,
            show_patterns: true,
        }
    }
}

/// Documentation generation context
#[derive(Debug, Clone)]
pub struct DocumentationContext {
    /// Power domain expansion data
    pub expansion: PowerDomainExpansion,
    /// Component metadata
    pub component_metadata: HashMap<String, ComponentMetadata>,
    /// Generation options
    pub options: DocumentationOptions,
}

impl DocumentationContext {
    /// Create a new documentation context
    pub fn new(expansion: PowerDomainExpansion, options: DocumentationOptions) -> Self {
        Self {
            expansion,
            component_metadata: HashMap::new(),
            options,
        }
    }

    /// Add component metadata
    pub fn add_component_metadata(&mut self, name: String, metadata: ComponentMetadata) {
        self.component_metadata.insert(name, metadata);
    }

    /// Get component metadata
    pub fn get_component_metadata(&self, name: &str) -> Option<&ComponentMetadata> {
        self.component_metadata.get(name)
    }
}
