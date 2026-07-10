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
    models::{
        SpiceModel, ModelType,
        resistor::{ResistorModel, ResistorParams},
        capacitor::{CapacitorModel, CapacitorParams},
        inductor::{InductorModel, InductorParams},
        diode::{DiodeModel, DiodeParams},
        bjt::{BjtModel, BjtParams},
        mosfet::{MosfetModel, MosfetParams},
        opamp::{OpAmpModel, OpAmpParams},
    },
    model_factory::SpiceModelFactory,
    components::{ComponentModel, ComponentType},
    component_registry::ComponentRegistry,
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
    /// Component registry for type mappings
    component_registry: ComponentRegistry,
    /// Cached models
    model_cache: HashMap<String, ExtractedModel>,
}

impl ComponentModelExtractor {
    /// Create new model extractor
    pub fn new() -> Self {
        Self {
            model_factory: SpiceModelFactory::new(),
            component_registry: ComponentRegistry::new(),
            model_cache: HashMap::new(),
        }
    }
    
    /// Extract model from data map (simplified entry point)
    pub fn extract_from_data(&mut self, mut data: HashMap<String, String>) -> Result<ExtractedModel> {
        let name = data.get("name").cloned()
            .or_else(|| data.get("instance_name").cloned())
            .unwrap_or_else(|| "unknown".to_string());
        
        let module_type = data.get("type").cloned()
            .or_else(|| data.get("module_type").cloned())
            .ok_or_else(|| anyhow::anyhow!("No module type specified"))?;
        
        debug!("Extracting model for '{}' of type '{}'", name, module_type);

        // Get component type from registry
        let component_type = self.component_registry.get_component_type(&module_type, &data)
            .ok_or_else(|| anyhow::anyhow!("Unknown component type: {}", module_type))?;
        
        // Real-Data Policy (docs/spec/Real_Data_Policy.md): NO fabricated
        // default parameters. The model is built solely from values the entity
        // declares; a component that does not declare the parameters its SPICE
        // model needs is a HARD ERROR — never silently simulated on a guessed
        // 1kΩ / 1nF / 0.7V / Koren-nominal value. (The previous code seeded a
        // per-model default here, then overrode it; the seed is removed.)
        let mut parameters = HashMap::new();
        let component_class = data.get("component_class")
            .cloned()
            .unwrap_or_else(|| module_type.clone());
        let spice_model = self
            .component_registry
            .get_spice_model(&component_class)
            .map(|s| s.to_string());

        // Build parameters ONLY from entity-supplied values.
        //
        // Iterate in SORTED key order — `data` is a HashMap and several
        // distinct keys write the SAME parameter (a Fuse carries both
        // `resistance = 10mΩ` and a `value` of "1A" that the Resistor arm
        // maps to resistance), so hash-order iteration made the winner
        // random per run. The generic `value` arm is deferred below the
        // loop and only fills a parameter no explicit key already set:
        // an explicit attribute name always beats the generic value.
        let mut sorted_keys: Vec<&String> = data.keys().collect();
        sorted_keys.sort();
        for key in sorted_keys {
            let value = &data[key];
            if let Some(num_value) = self.parse_value(value) {
                // Map common attribute names to parameter names
                match key.as_str() {
                    "resistance" | "r" => { parameters.insert("resistance".to_string(), num_value); }
                    "capacitance" | "c" => { parameters.insert("capacitance".to_string(), num_value); }
                    "inductance" | "l" => { parameters.insert("inductance".to_string(), num_value); }
                    "voltage" | "v" => { parameters.insert("voltage".to_string(), num_value); }
                    "output_voltage" => { parameters.insert("output_voltage".to_string(), num_value); }
                    "vout" => { parameters.insert("vout".to_string(), num_value); }
                    "current" | "i" => { parameters.insert("current".to_string(), num_value); }
                    "power" | "p" => { parameters.insert("power_rating".to_string(), num_value); }
                    // Diode / LED operating point (datasheet). Without these
                    // arms the required-parameter gate below could NEVER be
                    // satisfied for diodes/LEDs — the declared value sat in
                    // `data` but was never mapped into `parameters`.
                    "forward_voltage" | "vf" => { parameters.insert("forward_voltage".to_string(), num_value); }
                    "forward_current" => { parameters.insert("forward_current".to_string(), num_value); }
                    "saturation_current" | "is" => { parameters.insert("saturation_current".to_string(), num_value); }
                    // Regulator loss-model constants (datasheet). Mapping them
                    // here makes them UNIT-AWARE (parse_value handles `3.4mA`,
                    // `500kHz`, `90mΩ`) — the decomposition's req_param reads
                    // parameters first, so stdlib entities can declare these
                    // idiomatically with units.
                    "i_quiescent" | "quiescent_current" => { parameters.insert("i_quiescent".to_string(), num_value); }
                    "rds_on" => { parameters.insert("rds_on".to_string(), num_value); }
                    "switching_frequency" | "f_sw" => { parameters.insert("f_sw".to_string(), num_value); }
                    "t_sw" => { parameters.insert("t_sw".to_string(), num_value); }
                    "dropout" | "dropout_voltage" => { parameters.insert("dropout".to_string(), num_value); }
                    "efficiency" => { parameters.insert("efficiency".to_string(), num_value); }
                    "feedback_voltage" => { parameters.insert("feedback_voltage".to_string(), num_value); }
                    // Koren triode parameters.
                    "mu" => { parameters.insert("mu".to_string(), num_value); }
                    "ex" => { parameters.insert("ex".to_string(), num_value); }
                    "kg1" => { parameters.insert("kg1".to_string(), num_value); }
                    "kp" => { parameters.insert("kp".to_string(), num_value); }
                    "kvb" => { parameters.insert("kvb".to_string(), num_value); }
                    _ => {}
                }
            }
        }
        // Generic `value` → the type's primary parameter, only when no
        // explicit attribute (resistance/capacitance/inductance) set it.
        if let Some(num_value) = data.get("value").and_then(|v| self.parse_value(v)) {
            let primary = match &component_type {
                ComponentType::Resistor => Some("resistance"),
                ComponentType::Capacitor => Some("capacitance"),
                ComponentType::Inductor => Some("inductance"),
                _ => None,
            };
            if let Some(p) = primary {
                parameters.entry(p.to_string()).or_insert(num_value);
            }
        }

        // Hard-gate (Real-Data Policy): every parameter the SPICE model needs
        // must be a real, entity-declared value. Any missing ⇒ hard error,
        // naming the component and the missing parameter(s). No fabrication.
        if let Some(sm) = spice_model.as_deref() {
            let required: &[&str] = match sm {
                "resistor" => &["resistance"],
                "capacitor" => &["capacitance"],
                "inductor" => &["inductance"],
                "diode" | "led" => &["forward_voltage"],
                "voltage_source" => &["voltage"],
                "triode" => &["mu", "ex", "kg1", "kp", "kvb"],
                _ => &[],
            };
            let missing: Vec<&str> = required
                .iter()
                .copied()
                .filter(|k| !parameters.contains_key(*k))
                .collect();
            if !missing.is_empty() {
                return Err(anyhow::anyhow!(
                    "component '{}' ({}) is missing real SPICE parameter(s) {:?} — \
                     Real-Data Policy: no fabricated defaults; declare them on the \
                     stdlib entity (the datasheet)",
                    name,
                    sm,
                    missing
                ));
            }
        }

        Ok(ExtractedModel {
            name,
            source: ModelSource::Stdlib,
            component_type,
            parameters,
            attributes: data,
            confidence: 0.9,
        })
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
        let mut parameters = self.extract_spice_parameters(user_attrs)?;

        // The resolved component value lives in the `value` attribute
        // (e.g. "22.1kΩ", "4.7uH", "100nF"). extract_spice_parameters only
        // pulls spice_*-prefixed keys, so without this the primary parameter
        // (resistance/capacitance/inductance) is never populated and the
        // converter falls back to its 1kΩ/1µF default — which makes a feedback
        // divider solve as two equal resistors (equal power, FB = VOUT/2).
        // Map `value` onto the type's primary parameter, the same way
        // extract_from_data does.
        if let Some(val) = user_attrs.get("value").and_then(|v| self.parse_value(v)) {
            match component_type {
                ComponentType::Resistor => { parameters.insert("resistance".to_string(), val); }
                ComponentType::Capacitor => { parameters.insert("capacitance".to_string(), val); }
                ComponentType::Inductor => { parameters.insert("inductance".to_string(), val); }
                ComponentType::VoltageSource => { parameters.insert("voltage".to_string(), val); }
                _ => {}
            }
        }

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
        
        // Create simple models based on component type and parameters
        
        let model: Box<dyn SpiceModel> = match &extracted.component_type {
            ComponentType::Resistor => {
                let resistance = extracted.parameters.get("resistance").copied().unwrap_or(1000.0);
                Box::new(ResistorModel::from_value(&extracted.name, resistance, "generic"))
            }
            ComponentType::Capacitor => {
                let capacitance = extracted.parameters.get("capacitance").copied().unwrap_or(1e-9);
                Box::new(CapacitorModel::from_value(&extracted.name, capacitance, "generic", 50.0))
            }
            ComponentType::Inductor => {
                let inductance = extracted.parameters.get("inductance").copied().unwrap_or(1e-6);
                Box::new(InductorModel::from_value(&extracted.name, inductance, "generic", 1.0))
            }
            ComponentType::Diode => {
                let vf = extracted.parameters.get("forward_voltage").copied().unwrap_or(0.7);
                let mut params = DiodeParams::default();
                params.vj = vf;
                Box::new(DiodeModel::new(extracted.name.clone(), params))
            }
            ComponentType::LED => {
                let vf = extracted.parameters.get("forward_voltage").copied().unwrap_or(2.0);
                let mut params = DiodeParams::default();
                params.vj = vf;
                params.is = 1e-12; // Typical LED saturation current
                Box::new(DiodeModel::new(extracted.name.clone(), params))
            }
            ComponentType::VoltageSource => {
                // For now, model as very low resistance
                // TODO: Implement proper voltage source model
                Box::new(ResistorModel::from_value(&extracted.name, 0.001, "voltage_source"))
            }
            ComponentType::CurrentSource => {
                // For now, model as very high resistance  
                // TODO: Implement proper current source model
                Box::new(ResistorModel::from_value(&extracted.name, 1e9, "current_source"))
            }
            ComponentType::VoltageRegulator => {
                // For now, model as low resistance to simulate regulated output
                // TODO: Use proper voltage regulator model
                Box::new(ResistorModel::from_value(&extracted.name, 0.1, "voltage_regulator"))
            }
            ComponentType::BJT => {
                // Create simple BJT model
                let mut params = BjtParams::default();
                params.bf = extracted.parameters.get("beta").copied().unwrap_or(100.0);
                Box::new(BjtModel::new(extracted.name.clone(), params))
            }
            ComponentType::MOSFET => {
                // Create simple MOSFET model
                let mut params = MosfetParams::default();
                params.vto = extracted.parameters.get("vth").copied().unwrap_or(2.0);
                Box::new(MosfetModel::new(extracted.name.clone(), params))
            }
            ComponentType::OpAmp => {
                // Create simple op-amp model with default parameters
                Box::new(OpAmpModel::new(extracted.name.clone(), OpAmpParams::default()))
            }
            ComponentType::Triode => {
                // The triode is emitted as a multi-terminal Circuit device
                // (DeviceKind::Triode), not via the SpiceModel path — this
                // cached placeholder is never consulted by the solver.
                Box::new(ResistorModel::from_value(&extracted.name, 1e9, "triode"))
            }
            ComponentType::Other(type_name) => {
                match type_name.as_str() {
                    "ground" => {
                        // Ground is modeled as very low resistance to a reference
                        Box::new(ResistorModel::from_value(&extracted.name, 0.001, "ground"))
                    }
                    "test_point" => {
                        // Test point is a high impedance resistor
                        Box::new(ResistorModel::from_value(&extracted.name, 1e9, "probe"))
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unsupported component type: {}", type_name));
                    }
                }
            }
        };
        
        Ok(model)
    }
    
    /// Determine component type from name and attributes
    fn determine_component_type(
        &self,
        name: &str,
        data: &HashMap<String, String>,
    ) -> Result<ComponentType> {
        // Use component registry to determine type
        if let Some(component_type) = self.component_registry.get_component_type(name, data) {
            Ok(component_type)
        } else {
            // Check explicit type attribute
            if let Some(type_str) = data.get("component_type") {
                self.parse_component_type(type_str)
            } else {
                Err(anyhow::anyhow!("Cannot determine component type for '{}' - not in registry", name))
            }
        }
    }
    
    /// Parse component type from string
    fn parse_component_type(&self, type_str: &str) -> Result<ComponentType> {
        // Create attributes map with component_class for registry lookup
        let mut attrs = HashMap::new();
        attrs.insert("component_class".to_string(), type_str.to_string());

        // Use registry to parse
        self.component_registry.get_component_type(type_str, &attrs)
            .ok_or_else(|| anyhow::anyhow!("Unknown component type: {}", type_str))
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