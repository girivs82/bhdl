use std::collections::HashMap;
use std::path::Path;
use std::fs;
use anyhow::{Result, Context};
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode, Entity, ImportStmt, HasName, PinDecl, Item};
use log::info;

/// Handles loading and parsing imported modules from BHDL files
pub struct ImportLoader {
    /// Cache of loaded modules from imports
    /// Key is the entity name, value is the parsed Entity AST
    loaded_entities: HashMap<String, Entity>,
    /// Generic arguments captured from alias specializations
    /// (`alias LM7805 = LinearRegulator<5V>;` → "LM7805" → ["5V"]). The
    /// attribute stamper substitutes these for the target entity's generic
    /// parameter names so e.g. `attribute output_voltage = V_OUT` resolves.
    alias_generic_args: HashMap<String, Vec<String>>,
    
    /// Cache of full source file ASTs for cross-import resolution
    /// Key is the file path, value is the parsed SourceFile AST
    loaded_source_files: HashMap<String, SourceFile>,
    
    /// Base path for resolving relative imports
    base_path: String,

    /// Optional Cargo-style library resolver. When set, non-relative
    /// imports (`<namespace>/<rel>.bhdl`) resolve through it — against
    /// the project manifest's declared libraries + the search path
    /// (`-I` / `$BHDL_LIB_PATH`). When unset, non-relative imports fall
    /// back to the legacy literal-path-from-cwd behaviour (keeps
    /// stdlib-only boards and existing tests working with no manifest).
    /// See `docs/spec/Library_Resolution.md`.
    resolver: Option<bhdl_common::library::LibraryResolver>,
}

impl ImportLoader {
    pub fn new(base_path: impl Into<String>) -> Self {
        Self {
            loaded_entities: HashMap::new(),
            alias_generic_args: HashMap::new(),
            loaded_source_files: HashMap::new(),
            base_path: base_path.into(),
            resolver: None,
        }
    }

    /// Update the base path for resolving relative imports
    pub fn set_base_path(&mut self, base_path: impl Into<String>) {
        self.base_path = base_path.into();
    }

    /// Install the Cargo-style library resolver for namespaced imports.
    pub fn set_resolver(&mut self, resolver: bhdl_common::library::LibraryResolver) {
        self.resolver = Some(resolver);
    }
    
    /// Process imports from a source file
    pub fn process_imports(&mut self, source_file: &SourceFile) -> Result<()> {
        info!("Processing imports from source file with base path: {}", self.base_path);
        
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
    
    /// Load entities from a file path
    fn load_from_path(&mut self, import_path: &str, module_names: &[String]) -> Result<()> {
        // Resolve the path. `./` and `../` stay file-relative to the
        // base path. Non-relative `<namespace>/<rel>.bhdl` imports go
        // through the Cargo-style resolver when one is installed
        // (declared-library + search-path resolution); otherwise they
        // fall back to the legacy literal-path-from-cwd behaviour.
        let full_path = if import_path.starts_with("../") || import_path.starts_with("./") {
            Path::new(&self.base_path).join(import_path)
        } else if let Some(resolver) = &self.resolver {
            resolver
                .resolve_import(import_path)
                .map_err(|e| anyhow::anyhow!("{}", e))?
        } else {
            // No resolver: shared search order — importing file's dir,
            // input dir, -I roots, $BHDL_LIB_PATH, then the legacy
            // literal-path-from-cwd fallback.
            bhdl_common::import_search::resolve_relative(
                import_path,
                Path::new(&self.base_path),
            )
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
        
        // Store the full source file AST for cross-import resolution
        let file_path = full_path.to_string_lossy().to_string();
        self.loaded_source_files.insert(file_path.clone(), source_file.clone());
        info!("Stored source file AST for: {}", file_path);
        
        // First, extract all entities from the file
        let mut file_modules = HashMap::new();
        for entity in source_file.entities() {
            if let Some(name) = entity.name() {
                let module_name = name.text().to_string();
                file_modules.insert(module_name.clone(), entity.clone());
            }
        }
        
        // Then, process aliases to find what modules map to requested names
        // Look for alias statements in the source file by checking all children
        // (source_file.items() might not include aliases)
        for child in source_file.syntax().children() {
            if child.kind() == bhdl_parser::SyntaxKind::ALIAS {
                // Parse the alias by extracting the relevant tokens
                let mut alias_name = String::new();
                let mut target_name = String::new();
                let mut found_eq = false;
                
                for token in child.children_with_tokens() {
                    if let Some(t) = token.as_token() {
                        match t.kind() {
                            bhdl_parser::SyntaxKind::IDENT => {
                                if !found_eq && alias_name.is_empty() {
                                    alias_name = t.text().to_string();
                                } else if found_eq && target_name.is_empty() {
                                    target_name = t.text().to_string();
                                }
                            },
                            bhdl_parser::SyntaxKind::EQ => {
                                found_eq = true;
                            },
                            _ => {}
                        }
                    }
                }
                
                if !alias_name.is_empty() && !target_name.is_empty() {
                    println!("IMPORT_LOADER: Parsed alias: {} -> {}", alias_name, target_name);
                    
                    // If the alias name is requested, load the target module
                    if module_names.contains(&alias_name) {
                        if let Some(target_entity) = file_modules.get(&target_name) {
                            println!("IMPORT_LOADER: Found alias {} -> {} (LOADING)", alias_name, target_name);
                            // Capture the specialization's generic arguments
                            // (the text between < and > on the alias RHS).
                            let alias_text = child.text().to_string();
                            if let (Some(lt), Some(gt)) = (alias_text.find('<'), alias_text.rfind('>')) {
                                if lt < gt {
                                    let args: Vec<String> = alias_text[lt + 1..gt]
                                        .split(',')
                                        .map(|a| a.trim().to_string())
                                        .filter(|a| !a.is_empty())
                                        .collect();
                                    if !args.is_empty() {
                                        println!("IMPORT_LOADER: Alias {} generic args: {:?}", alias_name, args);
                                        self.alias_generic_args.insert(alias_name.clone(), args);
                                    }
                                }
                            }
                            self.loaded_entities.insert(alias_name, target_entity.clone());
                        } else {
                            println!("IMPORT_LOADER: Target entity {} not found for alias {}", target_name, alias_name);
                        }
                    } else {
                        println!("IMPORT_LOADER: Alias {} not in requested modules {:?}", alias_name, module_names);
                    }
                }
            }
        }
        
        // Finally, load directly requested entities (not aliases)
        for entity in source_file.entities() {
            if let Some(name) = entity.name() {
                let entity_name = name.text().to_string();

                // Check if this entity was requested directly
                if module_names.contains(&entity_name) {
                    println!("IMPORT_LOADER: Found requested entity: {}", entity_name);
                    self.loaded_entities.insert(entity_name, entity.clone());
                } else {
                    println!("IMPORT_LOADER: Skipping entity {} (not requested)", entity_name);
                }
            }
        }
        
        Ok(())
    }
    
    /// Get a loaded module by name
    pub fn get_entity(&self, name: &str) -> Option<&Entity> {
        self.loaded_entities.get(name)
    }

    /// Generic arguments of an alias specialization, if `name` was loaded via
    /// `alias <name> = <Target><args…>;`.
    pub fn get_alias_generic_args(&self, name: &str) -> Option<&Vec<String>> {
        self.alias_generic_args.get(name)
    }
    
    /// Get all loaded modules
    pub fn loaded_entities(&self) -> &HashMap<String, Entity> {
        &self.loaded_entities
    }
    
    /// Get all loaded source file ASTs for cross-import resolution
    pub fn loaded_source_files(&self) -> &HashMap<String, SourceFile> {
        &self.loaded_source_files
    }
    
    /// Try to find the source file containing a specific text range
    /// This is used for resolving SyntaxNodePtr across imports
    pub fn find_source_file_for_range(&self, range: rowan::TextRange) -> Option<&SourceFile> {
        // Check each loaded source file to see if it contains this range
        for (_, source_file) in &self.loaded_source_files {
            let file_range = source_file.syntax().text_range();
            if file_range.contains_range(range) {
                return Some(source_file);
            }
        }
        None
    }
    
    /// Check if a module has virtual pins
    pub fn entity_has_virtual_pins(&self, module_name: &str) -> bool {
        if let Some(module) = self.get_entity(module_name) {
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
        
        if let Some(module) = self.get_entity(module_name) {
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