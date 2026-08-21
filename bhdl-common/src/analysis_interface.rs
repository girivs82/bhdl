//! Shared interfaces for analysis results
//! 
//! This module defines interfaces that allow different parts of the toolchain
//! to share analysis data without creating circular dependencies.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::pin_metadata::ModulePinMetadata;

/// Interface for accessing module definitions from analysis results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDefinitionInfo {
    pub name: String,
    pub pins: ModulePinMetadata,
    pub parameters: HashMap<String, String>,
}

/// Simplified interface for accessing analysis results
/// This avoids circular dependencies by providing only the data needed
/// by downstream tools like SPICE
pub trait AnalysisResultInterface {
    /// Get module definitions with their pin metadata
    fn get_module_definitions(&self) -> HashMap<String, ModuleDefinitionInfo>;
    
    /// Get the symbol table data needed for component mapping
    fn get_symbol_data(&self) -> HashMap<String, SymbolInfo>;
}

/// Simplified symbol information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInfo {
    pub symbol_type: SymbolType,
    pub module_type: Option<String>,
    pub parameters: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolType {
    Module,
    Instance,
    Net,
    Power,
    Ground,
    Constant,
}

/// Container for passing analysis data to SPICE and storing analysis results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisData {
    pub module_definitions: HashMap<String, ModuleDefinitionInfo>,
    pub symbol_data: HashMap<String, SymbolInfo>,
    /// Per-instance analysis results (instance_id -> analysis data)
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub instance_analysis: HashMap<String, InstanceAnalysisData>,
    /// Decap-synthesis reports (`decouple` statements), one per
    /// statement — what was chosen, why, and what was verified.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub decap_reports: Vec<DecapReport>,
}

/// One `decouple` statement's synthesis record: the machine-readable
/// form of "what did the mask force, what margin was added, and what
/// was verified" — printed by the CLI report section and archived
/// with the netlist's analysis data.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DecapReport {
    /// "<instance>.<domain>"
    pub target: String,
    /// rail net the network hangs on
    pub net: String,
    /// library path as written in the statement
    pub lib: String,
    pub mask_breakpoints: usize,
    /// mask derating headroom applied to every check
    pub z_margin_pct: f64,
    pub candidates_usable: usize,
    /// skipped library entities with the stated reason
    pub candidates_skipped: Vec<String>,
    /// greedy selections in commit order
    pub steps: Vec<DecapStep>,
    /// margin instances added (instance + entity), N+1 per non-bulk value
    pub margin_added: Vec<String>,
    /// bulk values exempt from margin, stated
    pub bulk_exempt: Vec<String>,
    /// single-open fault sweeps that passed vs the derated mask
    pub opens_verified: usize,
    /// bulk caps whose open was NOT verified (margin-exempt), stated
    pub opens_bulk_exempt: usize,
    /// final worst |Z|/mask against the derated mask
    pub final_ratio: f64,
    pub final_freq_hz: f64,
    /// true when the caps already existed (elaborated/hand-carried
    /// input) and synthesis skipped
    pub already_present: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DecapStep {
    pub instance: String,
    pub entity: String,
    pub value: String,
    /// worst |Z|/mask AFTER committing this part
    pub ratio_after: f64,
    pub freq_hz: f64,
}

impl AnalysisData {
    pub fn new() -> Self {
        Self {
            module_definitions: HashMap::new(),
            symbol_data: HashMap::new(),
            instance_analysis: HashMap::new(),
            decap_reports: Vec::new(),
        }
    }
}

impl Default for AnalysisData {
    fn default() -> Self {
        Self::new()
    }
}

/// Analysis-specific data that can be attached to netlist instances
/// This is different from AnalysisData above - this is per-instance analysis results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceAnalysisData {
    /// SPICE-specific component type (e.g., "resistor", "capacitor")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spice_type: Option<String>,
    
    /// Component role detected by analysis (e.g., "input_filter", "bypass_capacitor")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_role: Option<String>,
    
    /// Electrical parameters extracted from analysis
    #[serde(skip_serializing_if = "Option::is_none")]
    pub electrical_params: Option<ElectricalParams>,
    
    /// Safety analysis results
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_info: Option<SafetyInfo>,
    
    /// Generic extension map for future analysis types
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub extensions: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElectricalParams {
    /// Nominal value (resistance, capacitance, etc.)
    pub value: Option<f64>,
    
    /// Tolerance percentage
    pub tolerance: Option<f64>,
    
    /// Power rating in watts
    pub power_rating: Option<f64>,
    
    /// Voltage rating in volts
    pub voltage_rating: Option<f64>,
    
    /// Current rating in amperes
    pub current_rating: Option<f64>,
    
    /// Additional parameters
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub extra: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyInfo {
    /// Maximum safe voltage
    pub max_voltage: Option<f64>,
    
    /// Maximum safe current
    pub max_current: Option<f64>,
    
    /// Actual operating voltage (from DC analysis)
    pub operating_voltage: Option<f64>,
    
    /// Actual operating current (from DC analysis)
    pub operating_current: Option<f64>,
    
    /// Safety margin percentage
    pub safety_margin: Option<f64>,
    
    /// Any safety violations detected
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub violations: Vec<SafetyViolation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyViolation {
    pub severity: String, // "warning", "error", "critical"
    pub message: String,
    pub recommendation: Option<String>,
}

impl Default for InstanceAnalysisData {
    fn default() -> Self {
        Self {
            spice_type: None,
            component_role: None,
            electrical_params: None,
            safety_info: None,
            extensions: HashMap::new(),
        }
    }
}