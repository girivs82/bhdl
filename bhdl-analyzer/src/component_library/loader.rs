//! Library loader for BHDL component libraries

use super::*;
use std::fs;
use anyhow::{Result, Context};
use bhdl_ast::{AstNode, SourceFile};

pub struct LibraryLoader;

impl LibraryLoader {
    /// Load a component library from a directory
    pub fn load_library(path: &Path) -> Result<ComponentLibrary> {
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
        
        // Load all .bhdl files recursively
        Self::load_modules_recursive(path, &mut library)?;
        
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
        let content = fs::read_to_string(path)?;
        
        // Parse using BHDL parser
        let parsed = bhdl_parser::parse(&content);
        
        if !parsed.errors().is_empty() {
            anyhow::bail!("Parse errors in {:?}: {:?}", path, parsed.errors());
        }
        
        // Extract module definitions
        let modules = Self::extract_modules_from_ast(&parsed.syntax(), path)?;
        
        Ok(modules)
    }
    
    /// Extract module definitions from parsed AST
    fn extract_modules_from_ast(
        syntax: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
        source_file: &Path,
    ) -> Result<Vec<ComponentModule>> {
        let mut modules = Vec::new();
        
        // Get the source file AST
        if let Some(_source) = SourceFile::cast(syntax.clone()) {
            // TODO: Implement proper module extraction from AST
            // For now, use simplified extraction based on syntax patterns
            
            for child in syntax.children() {
                if child.kind() == bhdl_parser::SyntaxKind::MODULE_DEF {
                    if let Some(module) = Self::parse_module_definition(&child, source_file)? {
                        modules.push(module);
                    }
                }
            }
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
        
        // Create basic module structure
        // TODO: Properly parse parameters, pins, metadata, etc.
        let module = ComponentModule {
            name,
            source_file: source_file.to_path_buf(),
            parameters: Vec::new(),
            pins: Vec::new(),
            metadata: ComponentMetadata::default(),
            conditionals: Vec::new(),
        };
        
        Ok(Some(module))
    }
}

// Add toml dependency to Cargo.toml for manifest parsing