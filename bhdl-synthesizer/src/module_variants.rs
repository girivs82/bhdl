//! Module variant management for parameter-based deduplication
//!
//! This module handles creating unique variants of modules based on their
//! parameter values, ensuring proper deduplication while maintaining
//! distinct implementations for different parameter combinations.

use std::collections::HashMap;
use anyhow::Result;
use bhdl_ast::{AstNode, ModuleInst, HasName};
use bhdl_analyzer::types::AnalysisResult;
use bhdl_netlist::{Netlist, ModuleId, ModuleKind};
use log::{debug, info};

/// Key for identifying unique module variants
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModuleVariantKey {
    /// Base module name (e.g., "RC_Filter")
    pub base_name: String,
    /// Sorted parameter key-value pairs for consistent hashing
    pub parameters: Vec<(String, String)>,
}

impl ModuleVariantKey {
    /// Create a new variant key from module name and parameters
    pub fn new(base_name: String, mut parameters: Vec<(String, String)>) -> Self {
        // Sort parameters for consistent hashing
        parameters.sort_by(|a, b| a.0.cmp(&b.0));
        Self {
            base_name,
            parameters,
        }
    }
    
    /// Generate a unique variant name (e.g., "RC_Filter_1k6_100n")
    pub fn variant_name(&self) -> String {
        if self.parameters.is_empty() {
            return self.base_name.clone();
        }
        
        // Create a suffix from parameter values
        let suffix = self.parameters.iter()
            .map(|(_, value)| {
                // Simplify value for naming (remove units, replace dots)
                value.replace(".", "_")
                    .replace(" ", "")
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '_')
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("_");
        
        format!("{}_{}", self.base_name, suffix)
    }
}

/// Module variant manager for deduplication
pub struct ModuleVariantManager {
    /// Map from variant key to module ID
    variants: HashMap<ModuleVariantKey, ModuleId>,
    /// Map from base module name to its definition in the AST
    module_definitions: HashMap<String, bhdl_ast::Module>,
}

impl ModuleVariantManager {
    pub fn new() -> Self {
        Self {
            variants: HashMap::new(),
            module_definitions: HashMap::new(),
        }
    }
    
    /// Register a module definition from the AST
    pub fn register_module_definition(&mut self, module: &bhdl_ast::Module) {
        if let Some(name) = module.name() {
            let module_name = name.text().to_string();
            self.module_definitions.insert(module_name, module.clone());
            debug!("Registered module definition: {}", name.text());
        }
    }
    
    /// Get or create a module variant for the given instance
    pub fn get_or_create_variant(
        &mut self,
        module_inst: &ModuleInst,
        netlist: &mut Netlist,
        analysis: &AnalysisResult,
    ) -> Result<ModuleId> {
        // Extract module type and parameters
        let module_type = module_inst.module_type()
            .map(|t| t.text().to_string())
            .ok_or_else(|| anyhow::anyhow!("Module instance missing type"))?;
        
        // Extract parameter values from the instance
        let parameters = self.extract_instance_parameters(module_inst, analysis)?;
        
        // Create variant key
        let variant_key = ModuleVariantKey::new(module_type.clone(), parameters);
        
        // Check if variant already exists
        if let Some(&module_id) = self.variants.get(&variant_key) {
            debug!("Reusing existing variant: {}", variant_key.variant_name());
            return Ok(module_id);
        }
        
        // Create new variant
        let variant_name = variant_key.variant_name();
        info!("Creating new module variant: {}", variant_name);
        
        // Create the module in the netlist
        let module_id = netlist.add_module(variant_name.clone(), ModuleKind::Module);
        
        // Copy pins from base module definition
        if let Some(base_module) = self.module_definitions.get(&module_type) {
            self.copy_module_interface(base_module, module_id, netlist)?;
        }
        
        // Store parameter values as module attributes
        for (param_name, param_value) in &variant_key.parameters {
            netlist.modules.get_mut(module_id)
                .map(|m| m.attributes.insert(param_name.clone(), param_value.clone()));
        }
        
        // Register the variant
        self.variants.insert(variant_key, module_id);
        
        Ok(module_id)
    }
    
    /// Extract parameter values from a module instance
    fn extract_instance_parameters(
        &self,
        module_inst: &ModuleInst,
        _analysis: &AnalysisResult,
    ) -> Result<Vec<(String, String)>> {
        let mut parameters = Vec::new();
        
        // Get parameters from the instance's param list
        if let Some(param_list) = module_inst.param_list() {
            for param_assign in param_list.params() {
                if let Some(name) = param_assign.name() {
                    let param_name = name.text().to_string();
                    
                    // Get the parameter value as a string
                    let param_value = if let Some(value) = param_assign.value() {
                        value.syntax().text().to_string().trim().to_string()
                    } else {
                        continue;
                    };
                    
                    parameters.push((param_name, param_value));
                }
            }
        }
        
        // If no parameters specified, we might need to get defaults from the module definition
        if parameters.is_empty() {
            if let Some(module_type) = module_inst.module_type() {
                let type_name = module_type.text().to_string();
                if let Some(base_module) = self.module_definitions.get(&type_name) {
                    parameters = self.extract_default_parameters(base_module)?;
                }
            }
        }
        
        Ok(parameters)
    }
    
    /// Extract default parameter values from module definition
    fn extract_default_parameters(&self, module: &bhdl_ast::Module) -> Result<Vec<(String, String)>> {
        let mut parameters = Vec::new();
        
        if let Some(param_list) = module.param_list() {
            // For module definitions, use param_defs() to get ModuleParam items
            for param_def in param_list.param_defs() {
                if let Some(name) = param_def.name() {
                    let param_name = name.text().to_string();
                    
                    // Get default value if present
                    if let Some(default_value) = param_def.default_value() {
                        let value_text = default_value.syntax().text().to_string().trim().to_string();
                        parameters.push((param_name, value_text));
                    }
                }
            }
        }
        
        Ok(parameters)
    }
    
    /// Copy module interface (pins) from base definition to variant
    fn copy_module_interface(
        &self,
        base_module: &bhdl_ast::Module,
        variant_id: ModuleId,
        netlist: &mut Netlist,
    ) -> Result<()> {
        use bhdl_ast::SyntaxKind;
        use bhdl_netlist::types::{PortDirection, PinDirection, PinType};
        
        // Extract pins from the base module
        for child in base_module.syntax().children() {
            if child.kind() == SyntaxKind::PIN_DECL {
                if let Some(pin_decl) = bhdl_ast::common::PinDecl::cast(child) {
                    if let Some(pin_name) = pin_decl.name() {
                        let pin_name_str = pin_name.text().to_string();
                        
                        // Determine pin type and direction
                        let decl_text = pin_decl.syntax().text().to_string();
                        let pin_type = if decl_text.contains("power") {
                            PinType::Power
                        } else if decl_text.contains("ground") {
                            PinType::Ground
                        } else {
                            PinType::Signal
                        };
                        
                        let direction = if decl_text.contains(" in") && !decl_text.contains("inout") {
                            PinDirection::In
                        } else if decl_text.contains(" out") && !decl_text.contains("inout") {
                            PinDirection::Out
                        } else {
                            PinDirection::InOut
                        };
                        
                        // Add port and pin to the variant
                        netlist.add_port(
                            variant_id,
                            pin_name_str.clone(),
                            PortDirection::InOut,
                            None
                        );
                        
                        netlist.add_pin(
                            variant_id,
                            pin_name_str,
                            direction,
                            pin_type
                        );
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Get all created variants for reporting
    pub fn get_variants(&self) -> &HashMap<ModuleVariantKey, ModuleId> {
        &self.variants
    }
}