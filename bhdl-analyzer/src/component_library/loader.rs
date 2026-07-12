//! Library loader for BHDL component libraries

use super::*;
use std::fs;
use anyhow::{Result, Context};
use bhdl_ast::{AstNode, SourceFile};

pub struct LibraryLoader;

impl LibraryLoader {
    /// Load a component library from a directory
    pub fn load_library(path: &Path) -> Result<ComponentLibrary> {
        log::debug!("Loading library from {:?}", path);
        
        // Load manifest
        let manifest_path = path.join("manifest.toml");
        let manifest = Self::load_manifest(&manifest_path)?;
        
        // Parse version
        let version = Version::parse(&manifest.library.version)?;
        
        let mut library = ComponentLibrary {
            name: manifest.library.name.clone(),
            version,
            path: path.to_path_buf(),
            modules: HashMap::new(),
            manifest,
        };
        
        log::debug!("Loading modules recursively from {:?}", path);
        // Load all .bhdl files recursively
        Self::load_modules_recursive(path, &mut library)?;
        
        log::debug!("Library loaded with {} modules", library.modules.len());
        Ok(library)
    }
    
    /// Load manifest.toml
    fn load_manifest(path: &Path) -> Result<LibraryManifest> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read manifest: {:?}", path))?;
        
        toml::from_str(&content)
            .with_context(|| format!("Failed to parse manifest: {:?}", path))
    }
    
    /// Recursively load .bhdl module files
    fn load_modules_recursive(dir: &Path, library: &mut ComponentLibrary) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_dir() {
                // Recurse into subdirectories
                Self::load_modules_recursive(&path, library)?;
            } else if path.extension().and_then(|s| s.to_str()) == Some("bhdl") {
                // Skip index.bhdl as it's just exports
                if path.file_name().and_then(|s| s.to_str()) == Some("index.bhdl") {
                    continue;
                }
                
                // Load module file
                if let Ok(modules) = Self::load_module_file(&path) {
                    for module in modules {
                        library.modules.insert(module.name.clone(), module);
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Load and parse a .bhdl module file
    fn load_module_file(path: &Path) -> Result<Vec<ComponentModule>> {
        log::debug!("Loading module file: {:?}", path);
        let content = fs::read_to_string(path)?;
        
        // Parse using BHDL parser
        let parsed = bhdl_parser::parse(&content);
        
        if !parsed.errors().is_empty() {
            anyhow::bail!("Parse errors in {:?}: {:?}", path, parsed.errors());
        }
        
        // Extract module definitions
        let modules = Self::extract_modules_from_ast(&parsed.syntax(), path)?;
        
        log::debug!("Extracted {} modules from {:?}", modules.len(), path.file_name());
        Ok(modules)
    }
    
    /// Extract module definitions from parsed AST
    fn extract_modules_from_ast(
        syntax: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
        source_file: &Path,
    ) -> Result<Vec<ComponentModule>> {
        log::debug!("Extracting modules from AST for {:?}", source_file);
        let mut modules = Vec::new();
        
        // Get the source file AST
        if let Some(_source) = SourceFile::cast(syntax.clone()) {
            // TODO: Implement proper module extraction from AST
            // For now, use simplified extraction based on syntax patterns
            
            let mut child_count = 0;
            for child in syntax.children() {
                child_count += 1;
                log::debug!("Processing child {} of kind {:?}", child_count, child.kind());
                if child.kind() == bhdl_parser::SyntaxKind::ENTITY_DEF {
                    log::debug!("Found ENTITY_DEF, parsing...");
                    match Self::parse_module_definition(&child, source_file) {
                        Ok(Some(module)) => {
                            log::debug!("Successfully parsed module: {}", module.name);
                            modules.push(module);
                        }
                        Ok(None) => log::debug!("parse_module_definition returned None"),
                        Err(e) => log::debug!("Error parsing module: {:?}", e),
                    }
                }
            }
            log::debug!("Processed {} children", child_count);
        }
        
        Ok(modules)
    }
    
    /// Parse a module definition node
    fn parse_module_definition(
        node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
        source_file: &Path,
    ) -> Result<Option<ComponentModule>> {
        // Extract module name
        let name = node.children_with_tokens()
            .find_map(|child| {
                if let rowan::NodeOrToken::Token(token) = child {
                    if token.kind() == bhdl_parser::SyntaxKind::IDENT {
                        Some(token.text().to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            });
        
        let name = match name {
            Some(n) => n,
            None => return Ok(None),
        };
        
        // Parse the entity's `attribute name = value;` declarations into the
        // module metadata. This is what makes the stdlib entity the "datasheet":
        // downstream analyses (e.g. LED current-limit resistor sizing) read the
        // REAL declared values (forward_voltage, forward_current, …) instead of
        // falling back to fabricated defaults (Real-Data Policy). Without this,
        // `electrical_specs` was always empty and every consumer guessed.
        let mut metadata = ComponentMetadata::default();
        // Resolve attribute values that REFERENCE a constructor param or const
        // to that param's default — e.g. `attribute tolerance = tolerance` →
        // 5%, `attribute voltage_rating = voltage` → 50V — instead of storing
        // the raw reference text. Storing the literal token (e.g. "tolerance")
        // would later read as a bogus string spec and poison the part-selection
        // grade gate. `extract_module_attributes_resolved` keeps plain literals
        // untouched and drops un-defaulted references, so the per-attribute
        // `unwrap_or(raw)` below preserves the old behaviour everywhere except
        // the references it can now resolve.
        // This pass is per-entity, so a param reference can only resolve to
        // the param's DEFAULT here; `attr_param_refs` records which attrs are
        // param-bound so per-instance consumers re-resolve them against the
        // instance's own constructor arguments.
        let entity_ast = bhdl_ast::Entity::cast(node.clone());
        let resolved = entity_ast
            .as_ref()
            .map(crate::attribute_extraction::extract_module_attributes_resolved)
            .unwrap_or_default();
        metadata.attr_param_refs = entity_ast
            .as_ref()
            .map(crate::attribute_extraction::extract_module_attribute_param_refs)
            .unwrap_or_default();
        for attr in node.descendants().filter_map(bhdl_ast::AttributeDecl::cast) {
            let (Some(name_tok), Some(value_expr)) = (attr.name(), attr.value()) else {
                continue;
            };
            let attr_name = name_tok.text().to_string();
            // Raw value text, with any surrounding quotes/whitespace stripped
            // ("2.0V", "20mA", "synchronous_buck", "Device:LED"); the fallback
            // for plain literals not present in the resolved map.
            let raw = value_expr.syntax().text().to_string().trim().trim_matches('"').to_string();
            let value = resolved.get(&attr_name).cloned().unwrap_or(raw);
            match attr_name.as_str() {
                "component_class" => metadata.component_class = Some(value.clone()),
                "kicad_symbol" => metadata.kicad_symbol = Some(value.clone()),
                _ => {}
            }
            metadata.electrical_specs.insert(attr_name, value);
        }
        // TODO: parse parameters/pins from the AST too (still stubbed).

        let module = ComponentModule {
            name,
            source_file: source_file.to_path_buf(),
            parameters: Vec::new(),
            pins: Vec::new(),
            metadata,
            conditionals: Vec::new(),
        };

        Ok(Some(module))
    }
}

// Add toml dependency to Cargo.toml for manifest parsing