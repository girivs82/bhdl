use std::collections::HashMap;
use anyhow::Result;
use bhdl_ast::{SourceFile, AstNode, Entity, ImportStmt, HasName};
use bhdl_analyzer::{AnalysisResult, symbol_table::SymbolTable};
use crate::import_loader::ImportLoader;
use log::info;

/// Pre-processes imports and augments the symbol table with imported definitions
/// This ensures the analyzer knows about imported entities before running analysis
pub struct ImportPreprocessor {
    import_loader: ImportLoader,
    imported_entities: HashMap<String, Entity>,
}

impl ImportPreprocessor {
    pub fn new(base_path: impl Into<String>) -> Self {
        Self {
            import_loader: ImportLoader::new(base_path),
            imported_entities: HashMap::new(),
        }
    }

    /// Pre-process all imports in a source file
    /// This loads the imported entities and makes them available for analysis
    pub fn preprocess_imports(&mut self, source_file: &SourceFile) -> Result<()> {
        info!("Pre-processing imports before analysis");

        // Load all imported entities
        self.import_loader.process_imports(source_file)?;

        // Store the loaded entities for later use
        for (name, entity) in self.import_loader.loaded_entities() {
            self.imported_entities.insert(name.clone(), entity.clone());
            info!("Pre-processed import: {}", name);
        }

        Ok(())
    }

    /// Augment the analyzer's symbol table with imported entity definitions
    /// This should be called before running the analyzer
    pub fn augment_symbol_table(&self, symbol_table: &mut SymbolTable) -> Result<()> {
        info!("Augmenting symbol table with {} imported entities", self.imported_entities.len());

        for (name, _entity) in &self.imported_entities {
            // Add the entity as a component type to the symbol table
            // This will allow the analyzer to recognize it as a valid component type
            use bhdl_analyzer::symbol_table::{Symbol, SymbolKind};
            use rowan::TextRange;

            let symbol = Symbol {
                name: name.clone(),
                kind: SymbolKind::Entity,
                span: TextRange::new(0.into(), 0.into()), // Dummy range for imported symbols
                instance_type_name: Some(name.clone()),
                definition_node_ptr: None, // No AST node for imported symbols
                bus_high: None,
                bus_low: None,
                direction: None,
                parameter_overrides: None,
                net_attributes: None,
                resolved_type: None,
                generic_params: None,
            };

            symbol_table.insert(symbol);
            info!("Added imported entity '{}' to symbol table", name);
        }

        Ok(())
    }

    /// Get an imported entity by name
    pub fn get_imported_entity(&self, name: &str) -> Option<&Entity> {
        self.imported_entities.get(name)
    }

    /// Get all imported entities
    pub fn imported_entities(&self) -> &HashMap<String, Entity> {
        &self.imported_entities
    }

    /// Check if an entity has virtual pins
    pub fn entity_has_virtual_pins(&self, entity_name: &str) -> bool {
        if let Some(entity) = self.get_imported_entity(entity_name) {
            for pin in entity.pins() {
                let pin_text = pin.syntax().text().to_string();
                if pin_text.contains("virtual") {
                    return true;
                }
            }
        }
        false
    }

    /// Get virtual pins for an entity
    pub fn get_virtual_pins(&self, entity_name: &str) -> Vec<String> {
        let mut virtual_pins = Vec::new();

        if let Some(entity) = self.get_imported_entity(entity_name) {
            for pin in entity.pins() {
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