//! Component Model Extractor
//! 
//! This module extracts SPICE models from various sources:
//! - Symbol table (analyzer results)
//! - Component database (KiCad symbols)
//! - BHDL stdlib definitions
//! - User-specified attributes

use std::collections::HashMap;
use anyhow::{Result, Context};
use log::{debug, info, warn};

use crate::{
    models::*,
    model_factory::SpiceModelFactory,
    components::{ComponentModel, ComponentType},
};

/// Sources for component model data
#[derive(Debug, Clone)]
pub enum ModelSource {
    /// From BHDL analyzer symbol table
    SymbolTable,
    /// From component database (KiCad)
    Database,
    /// From BHDL stdlib
    Stdlib,
    /// User-specified in BHDL
    UserDefined,
    /// Inferred from circuit context
    Inferred,
}

/// Extracted model information
#[derive(Debug, Clone)]
pub struct ExtractedModel {
    /// Component name/identifier
    pub name: String,
    /// Model source
    pub source: ModelSource,
    /// Component type
    pub component_type: ComponentType,
    /// SPICE model parameters
    pub parameters: HashMap<String, f64>,
    /// Additional attributes
    pub attributes: HashMap<String, String>,
    /// Confidence level (0.0 - 1.0)
    pub confidence: f64,
}

/// Component model extractor that combines multiple sources
pub struct ComponentModelExtractor {
    /// SPICE model factory
    model_factory: SpiceModelFactory,
    /// Cached models
    model_cache: HashMap<String, ExtractedModel>,
}

impl ComponentModelExtractor {
    /// Create new model extractor
    pub fn new() -> Self {
        Self {
            model_factory: SpiceModelFactory::new(),
            model_cache: HashMap::new(),
        }
    }
    
    /// Extract model from symbol table entry
    pub fn extract_from_symbol_table(
        &mut self,
        symbol_name: &str,
        symbol_data: &HashMap<String, String>,
    ) -> Result<ExtractedModel> {
        debug!("Extracting model for '{}' from symbol table", symbol_name);
        debug!("Symbol data: {:?}", symbol_data);
        
        // Determine component type from symbol data
        let component_type = self.determine_component_type(symbol_name, symbol_data)?;
        debug!("Determined component type: {:?}", component_type);
        
        // Extract parameters based on type
        let parameters = self.extract_parameters(&component_type, symbol_data)?;
        debug!("Extracted parameters: {:?}", parameters);
        
        // Extract attributes
        let attributes = self.extract_attributes(symbol_data);
        
        let model = ExtractedModel {
            name: symbol_name.to_string(),
            source: ModelSource::SymbolTable,
            component_type,
            parameters,
            attributes,
            confidence: 0.9, // High confidence from symbol table
        };
        
        // Cache the model
        self.model_cache.insert(symbol_name.to_string(), model.clone());
        
        Ok(model)
    }
    
    /// Extract model from component database
    pub async fn extract_from_database(
        &mut self,
        component_id: i64,
        database_path: &str,
    ) -> Result<ExtractedModel> {
        // This would connect to the actual database
        // For now, return a placeholder
        warn!("Database extraction not yet implemented for component {}", component_id);
        
        Err(anyhow::anyhow!("Database extraction not implemented"))
    }
    
    /// Extract model from BHDL stdlib
    pub fn extract_from_stdlib(
        &mut self,
        component_name: &str,
        stdlib_data: &HashMap<String, serde_json::Value>,
    ) -> Result<ExtractedModel> {
        debug!("Extracting model for '{}' from stdlib", component_name);
        
        // Parse stdlib JSON data
        let component_type = stdlib_data.get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing component type in stdlib"))?;
        
        let component_type = self.parse_component_type(component_type)?;
        
        // Extract parameters from stdlib
        let mut parameters = HashMap::new();
        if let Some(params) = stdlib_data.get("parameters").and_then(|v| v.as_object()) {
            for (key, value) in params {
                if let Some(num) = value.as_f64() {
                    parameters.insert(key.clone(), num);
                }
            }
        }
        
        // Extract attributes
        let mut attributes = HashMap::new();
        if let Some(attrs) = stdlib_data.get("attributes").and_then(|v| v.as_object()) {
            for (key, value) in attrs {
                if let Some(str_val) = value.as_str() {
                    attributes.insert(key.clone(), str_val.to_string());
                }
            }
        }
        
        let model = ExtractedModel {
            name: component_name.to_string(),
            source: ModelSource::Stdlib,
            component_type,
            parameters,
            attributes,
            confidence: 0.95, // Very high confidence from stdlib
        };
        
        self.model_cache.insert(component_name.to_string(), model.clone());
        
        Ok(model)
    }
    
    /// Extract model from user-defined attributes
    pub fn extract_from_user_attributes(
        &mut self,
        component_name: &str,
        user_attrs: &HashMap<String, String>,
    ) -> Result<ExtractedModel> {
        debug!("Extracting model for '{}' from user attributes", component_name);
        
        // Check for spice_model attribute
        let model_type = user_attrs.get("spice_model")
            .ok_or_else(|| anyhow::anyhow!("No spice_model attribute found"))?;
        
        let component_type = self.parse_component_type(model_type)?;
        
        // Extract SPICE parameters (spice_* attributes)
        let parameters = self.extract_spice_parameters(user_attrs)?;
        
        // All attributes are passed through
        let attributes = user_attrs.clone();
        
        let model = ExtractedModel {
            name: component_name.to_string(),
            source: ModelSource::UserDefined,
            component_type,
            parameters,
            attributes,
            confidence: 1.0, // Full confidence in user-specified values
        };
        
        self.model_cache.insert(component_name.to_string(), model.clone());
        
        Ok(model)
    }
    
    /// Infer model from circuit context
    pub fn infer_from_context(
        &mut self,
        component_name: &str,
        connections: &[String],
        nearby_components: &[String],
    ) -> Result<ExtractedModel> {
        debug!("Inferring model for '{}' from circuit context", component_name);
        
        // Simple heuristics based on connections
        let component_type = if connections.iter().any(|c| c.contains("VCC") || c.contains("GND")) {
            if nearby_components.iter().any(|c| c.contains("LED")) {
                ComponentType::Resistor // Likely current limiting
            } else {
                ComponentType::Capacitor // Likely bypass cap
            }
        } else {
            ComponentType::Resistor // Default guess
        };
        
        // Inferred parameters are estimates
        let mut parameters = HashMap::new();
        match &component_type {
            ComponentType::Resistor => {
                parameters.insert("resistance".to_string(), 10e3); // 10k default
            }
            ComponentType::Capacitor => {
                parameters.insert("capacitance".to_string(), 100e-9); // 100nF default
            }
            _ => {}
        }
        
        let model = ExtractedModel {
            name: component_name.to_string(),
            source: ModelSource::Inferred,
            component_type,
            parameters,
            attributes: HashMap::new(),
            confidence: 0.3, // Low confidence for inferred models
        };
        
        self.model_cache.insert(component_name.to_string(), model.clone());
        
        Ok(model)
    }
    
    /// Create SPICE model from extracted data
    pub fn create_spice_model(&self, extracted: &ExtractedModel) -> Result<Box<dyn SpiceModel>> {
        info!("Creating SPICE model for '{}' ({:?})", extracted.name, extracted.component_type);
        
        // Use model factory with extracted parameters
        let model = self.model_factory.create_from_attributes(
            &extracted.name,
            &extracted.attributes,
        ).or_else(|| {
            // Fallback to creating from type and parameters
            self.create_model_from_type(&extracted.name, &extracted.component_type, &extracted.parameters)
        }).ok_or_else(|| anyhow::anyhow!("Failed to create SPICE model for {}", extracted.name))?;
        
        Ok(model)
    }
    
    /// Determine component type from name and attributes
    fn determine_component_type(
        &self,
        name: &str,
        data: &HashMap<String, String>,
    ) -> Result<ComponentType> {
        // Check explicit type attribute
        if let Some(type_str) = data.get("component_type") {
            return self.parse_component_type(type_str);
        }
        
        // Infer from name patterns
        let lower_name = name.to_lowercase();
        if lower_name.starts_with('r') || lower_name.contains("res") {
            Ok(ComponentType::Resistor)
        } else if lower_name.starts_with('c') || lower_name.contains("cap") {
            Ok(ComponentType::Capacitor)
        } else if lower_name.starts_with('l') || lower_name.contains("ind") {
            Ok(ComponentType::Inductor)
        } else if lower_name.starts_with('d') {
            if lower_name.contains("led") {
                Ok(ComponentType::LED)
            } else {
                Ok(ComponentType::Diode)
            }
        } else if lower_name.starts_with('q') {
            Ok(ComponentType::BJT)
        } else if lower_name.starts_with('m') {
            Ok(ComponentType::MOSFET)
        } else if lower_name.starts_with('u') {
            if lower_name.contains("reg") {
                Ok(ComponentType::VoltageRegulator)
            } else {
                Ok(ComponentType::OpAmp)
            }
        } else {
            Err(anyhow::anyhow!("Cannot determine component type for '{}'", name))
        }
    }
    
    /// Parse component type from string
    fn parse_component_type(&self, type_str: &str) -> Result<ComponentType> {
        match type_str.to_lowercase().as_str() {
            "resistor" | "res" => Ok(ComponentType::Resistor),
            "capacitor" | "cap" => Ok(ComponentType::Capacitor),
            "inductor" | "ind" => Ok(ComponentType::Inductor),
            "diode" => Ok(ComponentType::Diode),
            "led" => Ok(ComponentType::LED),
            "bjt" | "transistor" => Ok(ComponentType::BJT),
            "mosfet" | "fet" => Ok(ComponentType::MOSFET),
            "opamp" | "op-amp" => Ok(ComponentType::OpAmp),
            "voltage_regulator" | "regulator" => Ok(ComponentType::VoltageRegulator),
            _ => Err(anyhow::anyhow!("Unknown component type: {}", type_str))
        }
    }
    
    /// Extract parameters based on component type
    fn extract_parameters(
        &self,
        component_type: &ComponentType,
        data: &HashMap<String, String>,
    ) -> Result<HashMap<String, f64>> {
        let mut parameters = HashMap::new();
        
        match component_type {
            ComponentType::Resistor => {
                if let Some(val) = data.get("value").and_then(|v| self.parse_value(v)) {
                    parameters.insert("resistance".to_string(), val);
                }
                if let Some(pwr) = data.get("power").and_then(|v| self.parse_value(v)) {
                    parameters.insert("power_rating".to_string(), pwr);
                }
            }
            ComponentType::Capacitor => {
                if let Some(val_str) = data.get("value") {
                    debug!("Parsing capacitor value: '{}'", val_str);
                    if let Some(val) = self.parse_value(val_str) {
                        debug!("Parsed capacitance: {}", val);
                        parameters.insert("capacitance".to_string(), val);
                    } else {
                        warn!("Failed to parse capacitor value: '{}'", val_str);
                    }
                }
                if let Some(v) = data.get("voltage").and_then(|v| self.parse_value(v)) {
                    parameters.insert("voltage_rating".to_string(), v);
                }
            }
            ComponentType::Inductor => {
                if let Some(val) = data.get("value").and_then(|v| self.parse_value(v)) {
                    parameters.insert("inductance".to_string(), val);
                }
                if let Some(i) = data.get("current").and_then(|v| self.parse_value(v)) {
                    parameters.insert("current_rating".to_string(), i);
                }
            }
            ComponentType::LED => {
                if let Some(vf) = data.get("forward_voltage").and_then(|v| self.parse_value(v)) {
                    parameters.insert("forward_voltage".to_string(), vf);
                }
                if let Some(imax) = data.get("max_current").and_then(|v| self.parse_value(v)) {
                    parameters.insert("max_current".to_string(), imax);
                }
            }
            _ => {
                // Generic parameter extraction
                for (key, value) in data {
                    if let Some(num) = self.parse_value(value) {
                        parameters.insert(key.clone(), num);
                    }
                }
            }
        }
        
        Ok(parameters)
    }
    
    /// Extract non-numeric attributes
    fn extract_attributes(&self, data: &HashMap<String, String>) -> HashMap<String, String> {
        let mut attributes = HashMap::new();
        
        for (key, value) in data {
            // Skip numeric values
            if self.parse_value(value).is_none() {
                attributes.insert(key.clone(), value.clone());
            }
        }
        
        attributes
    }
    
    /// Extract SPICE-specific parameters (spice_* prefix)
    fn extract_spice_parameters(&self, attrs: &HashMap<String, String>) -> Result<HashMap<String, f64>> {
        let mut parameters = HashMap::new();
        
        for (key, value) in attrs {
            if key.starts_with("spice_") && !key.ends_with("_model") {
                let param_name = key.strip_prefix("spice_").unwrap();
                if let Some(num) = self.parse_value(value) {
                    parameters.insert(param_name.to_string(), num);
                }
            }
        }
        
        Ok(parameters)
    }
    
    /// Parse numeric value with units
    fn parse_value(&self, value_str: &str) -> Option<f64> {
        // Use the public parse_value function from model_factory
        crate::model_factory::parse_value(value_str)
    }
    
    /// Create model from type and parameters
    fn create_model_from_type(
        &self,
        name: &str,
        component_type: &ComponentType,
        parameters: &HashMap<String, f64>,
    ) -> Option<Box<dyn SpiceModel>> {
        self.model_factory.create_from_bhdl(
            name,
            &format!("{:?}", component_type),
            parameters,
        )
    }
}

impl Default for ComponentModelExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_extract_from_symbol_table() {
        let mut extractor = ComponentModelExtractor::new();
        
        let mut symbol_data = HashMap::new();
        symbol_data.insert("component_type".to_string(), "resistor".to_string());
        symbol_data.insert("value".to_string(), "10k".to_string());
        symbol_data.insert("power".to_string(), "0.25W".to_string());
        
        let model = extractor.extract_from_symbol_table("R1", &symbol_data).unwrap();
        
        assert_eq!(model.name, "R1");
        assert!(matches!(model.source, ModelSource::SymbolTable));
        assert!(matches!(model.component_type, ComponentType::Resistor));
        assert_eq!(model.parameters["resistance"], 10e3);
        assert_eq!(model.parameters["power_rating"], 0.25);
    }
    
    #[test]
    fn test_infer_from_context() {
        let mut extractor = ComponentModelExtractor::new();
        
        let connections = vec!["VCC".to_string(), "LED1.A".to_string()];
        let nearby = vec!["LED1".to_string()];
        
        let model = extractor.infer_from_context("R1", &connections, &nearby).unwrap();
        
        assert!(matches!(model.component_type, ComponentType::Resistor));
        assert_eq!(model.confidence, 0.3);
    }
}