use std::collections::HashMap;
use std::path::Path;
use std::fs;
use anyhow::{Result, Context};
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode, Module, ImportStmt, HasName, PinDecl};
use log::info;

/// Handles loading and parsing imported modules from BHDL files
pub struct ImportLoader {
    /// Cache of loaded modules from imports
    /// Key is the module name, value is the parsed Module AST
    loaded_modules: HashMap<String, Module>,
    
    /// Base path for resolving relative imports
    base_path: String,
}

impl ImportLoader {
    pub fn new(base_path: impl Into<String>) -> Self {
        Self {
            loaded_modules: HashMap::new(),
            base_path: base_path.into(),
        }
    }
    
    /// Process imports from a source file
    pub fn process_imports(&mut self, source_file: &SourceFile) -> Result<()> {
        info!("Processing imports from source file");
        
        // Iterate through all imports in the file
        for import in source_file.imports() {
            if let Some(path) = import.path() {
                info!("Found import: {}", path);
                
                // Get the imported names
                let imported_names = import.imported_names();
                if imported_names.is_empty() {
                    info!("  Simple import (no destructuring)");
                } else {
                    info!("  Importing: {:?}", imported_names);
                    
                    // Load the file and extract the requested modules
                    self.load_from_path(&path, &imported_names)?;
                }
            }
        }
        
        Ok(())
    }
    
    /// Load modules from a file path
    fn load_from_path(&mut self, import_path: &str, module_names: &[String]) -> Result<()> {
        // Resolve the path relative to the base path
        let full_path = if import_path.starts_with("../") || import_path.starts_with("./") {
            // Relative path - resolve from base
            Path::new(&self.base_path).join(import_path)
        } else {
            // Absolute or stdlib path
            Path::new(import_path).to_path_buf()
        };
        
        println!("IMPORT_LOADER: Loading modules {:?} from: {}", module_names, full_path.display());
        
        // Read and parse the file
        let content = fs::read_to_string(&full_path)
            .with_context(|| format!("Failed to read import file: {}", full_path.display()))?;
        
        let parse_result = parse(&content);
        if !parse_result.errors().is_empty() {
            for error in parse_result.errors() {
                log::warn!("Parse error in imported file {}: {}", full_path.display(), error.message);
            }
        }
        
        let syntax = parse_result.syntax();
        let source_file = SourceFile::cast(syntax)
            .ok_or_else(|| anyhow::anyhow!("Failed to cast imported file to SourceFile"))?;
        
        // Extract the requested modules
        for module in source_file.modules() {
            if let Some(name) = module.name() {
                let module_name = name.text().to_string();
                
                // Check if this module was requested
                if module_names.contains(&module_name) {
                    println!("IMPORT_LOADER: Found requested module: {}", module_name);
                    self.loaded_modules.insert(module_name, module.clone());
                } else {
                    println!("IMPORT_LOADER: Skipping module {} (not requested)", module_name);
                }
            }
        }
        
        Ok(())
    }
    
    /// Get a loaded module by name
    pub fn get_module(&self, name: &str) -> Option<&Module> {
        self.loaded_modules.get(name)
    }
    
    /// Get all loaded modules
    pub fn loaded_modules(&self) -> &HashMap<String, Module> {
        &self.loaded_modules
    }
    
    /// Check if a module has virtual pins
    pub fn module_has_virtual_pins(&self, module_name: &str) -> bool {
        if let Some(module) = self.get_module(module_name) {
            // Check if any pins are marked as virtual
            for pin in module.pins() {
                let pin_text = pin.syntax().text().to_string();
                if pin_text.contains("virtual") {
                    return true;
                }
            }
        }
        false
    }
    
    /// Get virtual pins for a module
    pub fn get_virtual_pins(&self, module_name: &str) -> Vec<String> {
        let mut virtual_pins = Vec::new();
        
        if let Some(module) = self.get_module(module_name) {
            for pin in module.pins() {
                let pin_text = pin.syntax().text().to_string();
                if pin_text.contains("virtual") {
                    if let Some(name) = pin.name() {
                        virtual_pins.push(name.text().to_string());
                    }
                }
            }
        }
        
        virtual_pins
    }
}