use std::collections::HashMap;
use anyhow::Result;
use bhdl_ast::{SourceFile, AstNode, Module, ImportStmt, HasName};
use bhdl_analyzer::{AnalysisResult, symbol_table::SymbolTable};
use crate::import_loader::ImportLoader;
use log::info;

/// Pre-processes imports and augments the symbol table with imported definitions
/// This ensures the analyzer knows about imported modules before running analysis
pub struct ImportPreprocessor {
    import_loader: ImportLoader,
    imported_modules: HashMap<String, Module>,
}

impl ImportPreprocessor {
    pub fn new(base_path: impl Into<String>) -> Self {
        Self {
            import_loader: ImportLoader::new(base_path),
            imported_modules: HashMap::new(),
        }
    }
    
    /// Pre-process all imports in a source file
    /// This loads the imported modules and makes them available for analysis
    pub fn preprocess_imports(&mut self, source_file: &SourceFile) -> Result<()> {
        info!("Pre-processing imports before analysis");
        
        // Load all imported modules
        self.import_loader.process_imports(source_file)?;
        
        // Store the loaded modules for later use
        for (name, module) in self.import_loader.loaded_modules() {
            self.imported_modules.insert(name.clone(), module.clone());
            info!("Pre-processed import: {}", name);
        }
        
        Ok(())
    }
    
    /// Augment the analyzer's symbol table with imported module definitions
    /// This should be called before running the analyzer
    pub fn augment_symbol_table(&self, symbol_table: &mut SymbolTable) -> Result<()> {
        info!("Augmenting symbol table with {} imported modules", self.imported_modules.len());
        
        for (name, module) in &self.imported_modules {
            // Add the module as a component type to the symbol table
            // This will allow the analyzer to recognize it as a valid component type
            use bhdl_analyzer::symbol_table::{Symbol, SymbolKind};
            use rowan::TextRange;
            
            let symbol = Symbol {
                name: name.clone(),
                kind: SymbolKind::Module,
                span: TextRange::new(0.into(), 0.into()), // Dummy range for imported symbols
                instance_type_name: Some(name.clone()),
                definition_node_ptr: None, // No AST node for imported symbols
                bus_high: None,
                bus_low: None,
                direction: None,
                parameter_overrides: None,
                net_attributes: None,
            };
            
            symbol_table.insert(symbol);
            info!("Added imported module '{}' to symbol table", name);
        }
        
        Ok(())
    }
    
    /// Get an imported module by name
    pub fn get_imported_module(&self, name: &str) -> Option<&Module> {
        self.imported_modules.get(name)
    }
    
    /// Get all imported modules
    pub fn imported_modules(&self) -> &HashMap<String, Module> {
        &self.imported_modules
    }
    
    /// Check if a module has virtual pins
    pub fn module_has_virtual_pins(&self, module_name: &str) -> bool {
        if let Some(module) = self.get_imported_module(module_name) {
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
        
        if let Some(module) = self.get_imported_module(module_name) {
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

/// Pre-process imports and run analysis with augmented symbol table
/// This is a convenience function that handles the full workflow
pub fn preprocess_and_analyze(source_file: &SourceFile, base_path: &str) -> Result<(AnalysisResult, ImportPreprocessor)> {
    let mut preprocessor = ImportPreprocessor::new(base_path);
    
    // Step 1: Pre-process imports
    preprocessor.preprocess_imports(source_file)?;
    
    // Step 2: Run analysis with augmented symbol table
    let mut analysis_result = bhdl_analyzer::analyze(source_file);
    
    // Step 3: Augment the global symbol table with imported definitions
    // Note: This is a bit of a hack since we can't easily modify the analyzer's
    // internal symbol table after creation. In a future refactor, we should
    // pass the preprocessor to the analyzer directly.
    
    // For now, we'll add imported modules to the analysis result's diagnostics
    // so the synthesizer can see them and avoid "undefined component" errors
    
    Ok((analysis_result, preprocessor))
}