//! Component Library System for BHDL
//! 
//! Provides module-based component definitions with:
//! - Parameterized modules (Res(10k), Cap(100n))
//! - Component metadata and database linking
//! - Version management
//! - User library precedence over stdlib

use std::path::{Path, PathBuf};
use std::collections::HashMap;
use anyhow::Result;
use serde::{Serialize, Deserialize};

pub mod resolver;
pub mod loader;
pub mod cache;

pub use resolver::ModuleResolver;
pub use loader::LibraryLoader;
pub use cache::ModuleCache;

/// Component library containing module definitions
#[derive(Debug, Clone)]
pub struct ComponentLibrary {
    pub name: String,
    pub version: Version,
    pub path: PathBuf,
    pub modules: HashMap<String, ComponentModule>,
    pub manifest: LibraryManifest,
}

/// Library manifest (from manifest.toml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryManifest {
    pub library: LibraryInfo,
    pub components: ComponentsInfo,
    pub compatibility: CompatibilityInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryInfo {
    pub name: String,
    pub version: String,
    pub authors: Vec<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentsInfo {
    pub categories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityInfo {
    #[serde(rename = "bhdl-version")]
    pub bhdl_version: String,
}

/// Semantic version for libraries
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    pub fn parse(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            anyhow::bail!("Invalid version format: {}", s);
        }
        
        Ok(Version {
            major: parts[0].parse()?,
            minor: parts[1].parse()?,
            patch: parts[2].parse()?,
        })
    }
}

/// A component module definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentModule {
    pub name: String,
    pub source_file: PathBuf,
    pub parameters: Vec<ModuleParameter>,
    pub pins: Vec<PinDefinition>,
    pub metadata: ComponentMetadata,
    pub conditionals: Vec<ConditionalBlock>,
}

/// Module parameter definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleParameter {
    pub name: String,
    pub param_type: ParameterType,
    pub default_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParameterType {
    Resistance,
    Capacitance,
    Inductance,
    Voltage,
    Current,
    String,
    Package,
}

/// Pin definition in a module
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinDefinition {
    pub name: String,
    pub pin_type: PinType,
    pub electrical_type: Option<ElectricalType>,
    pub conditional: Option<String>,  // Condition for this pin
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PinType {
    Passive,
    PowerInput,
    PowerOutput(Option<f64>),  // Optional fixed voltage
    Ground,
    Input,
    Output,
    Bidirectional,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ElectricalType {
    Power,
    Ground,
    Digital,
    Analog,
    HighSpeed,
}

/// Component metadata from @ attributes
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComponentMetadata {
    pub component_class: Option<String>,
    pub kicad_symbol: Option<String>,
    pub packages: Vec<String>,
    pub default_package: Option<String>,
    pub db_component_id: Option<String>,
    pub electrical_specs: HashMap<String, String>,
}

/// Conditional blocks in modules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalBlock {
    pub condition: String,
    pub pins: Vec<PinDefinition>,
    pub metadata: HashMap<String, String>,
}

/// Library search path with precedence
#[derive(Debug, Clone)]
pub struct LibraryPath {
    pub path: PathBuf,
    pub precedence: u32,  // Lower = higher priority
    pub library: Option<ComponentLibrary>,
}

impl LibraryPath {
    pub fn find_module(&self, name: &str) -> Result<Option<ComponentModule>> {
        // TODO: Implement module search
        Ok(None)
    }
}