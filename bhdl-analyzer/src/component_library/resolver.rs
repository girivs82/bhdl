//! Module resolver for component instantiation
//! Handles search order: project → user libs → stdlib

use super::*;
use std::sync::Arc;
use anyhow::Result;

/// Resolves component module references
#[derive(Debug)]
pub struct ModuleResolver {
    /// Search paths in priority order
    search_paths: Vec<LibraryPath>,
    /// Resolved module cache
    cache: HashMap<String, Arc<ComponentModule>>,
    /// Standard library path
    stdlib_path: Option<PathBuf>,
}

impl ModuleResolver {
    pub fn new() -> Self {
        let stdlib_path = Self::find_stdlib_path();
        
        Self {
            search_paths: Vec::new(),
            cache: HashMap::new(),
            stdlib_path,
        }
    }
    
    /// Find the BHDL standard library path
    fn find_stdlib_path() -> Option<PathBuf> {
        // Try relative to executable
        if let Ok(exe_path) = std::env::current_exe() {
            let stdlib = exe_path
                .parent()?
                .parent()?
                .join("bhdl-stdlib");
            if stdlib.exists() {
                return Some(stdlib);
            }
        }
        
        // Try relative to current directory
        let cwd_stdlib = PathBuf::from("bhdl-stdlib");
        if cwd_stdlib.exists() {
            return Some(cwd_stdlib);
        }
        
        // Try environment variable
        if let Ok(path) = std::env::var("BHDL_STDLIB_PATH") {
            let stdlib = PathBuf::from(path);
            if stdlib.exists() {
                return Some(stdlib);
            }
        }
        
        None
    }
    
    /// Add a user library path with given precedence
    pub fn add_library_path(&mut self, path: PathBuf, precedence: u32) {
        self.search_paths.push(LibraryPath {
            path,
            precedence,
            library: None,
        });
        
        // Sort by precedence (lower number = higher priority)
        self.search_paths.sort_by_key(|p| p.precedence);
    }
    
    /// Add project-local modules (highest precedence)
    pub fn add_project_path(&mut self, path: PathBuf) {
        self.add_library_path(path, 0);
    }
    
    /// Initialize with standard library
    pub fn init_stdlib(&mut self) -> Result<()> {
        if let Some(stdlib_path) = &self.stdlib_path {
            self.add_library_path(stdlib_path.clone(), 1000); // Low priority
            Ok(())
        } else {
            anyhow::bail!("BHDL standard library not found")
        }
    }
    
    /// Resolve a module by name
    pub fn resolve(&mut self, module_name: &str) -> Result<Arc<ComponentModule>> {
        // Check cache first
        if let Some(cached) = self.cache.get(module_name) {
            return Ok(cached.clone());
        }
        
        // Search through paths in order
        for lib_path in &mut self.search_paths {
            // Load library if not already loaded
            if lib_path.library.is_none() {
                if let Ok(library) = LibraryLoader::load_library(&lib_path.path) {
                    lib_path.library = Some(library);
                }
            }
            
            // Search in library
            if let Some(library) = &lib_path.library {
                if let Some(module) = library.modules.get(module_name) {
                    let module_arc = Arc::new(module.clone());
                    self.cache.insert(module_name.to_string(), module_arc.clone());
                    return Ok(module_arc);
                }
            }
        }
        
        // Not found
        anyhow::bail!("Module '{}' not found in any library", module_name)
    }
    
    /// Resolve with parameter validation
    pub fn resolve_instantiation(
        &mut self,
        module_name: &str,
        parameters: &HashMap<String, String>,
    ) -> Result<(Arc<ComponentModule>, HashMap<String, String>)> {
        let module = self.resolve(module_name)?;
        
        // Validate parameters
        let mut resolved_params = HashMap::new();
        
        for param_def in &module.parameters {
            if let Some(value) = parameters.get(&param_def.name) {
                // TODO: Validate parameter type
                resolved_params.insert(param_def.name.clone(), value.clone());
            } else if let Some(default) = &param_def.default_value {
                resolved_params.insert(param_def.name.clone(), default.clone());
            } else {
                anyhow::bail!(
                    "Missing required parameter '{}' for module '{}'",
                    param_def.name,
                    module_name
                );
            }
        }
        
        // Check for unknown parameters
        for (param_name, _) in parameters {
            if !module.parameters.iter().any(|p| &p.name == param_name) {
                anyhow::bail!(
                    "Unknown parameter '{}' for module '{}'",
                    param_name,
                    module_name
                );
            }
        }
        
        Ok((module, resolved_params))
    }
    
    /// Get all available modules for autocomplete
    pub fn list_available_modules(&mut self) -> Vec<String> {
        let mut modules = Vec::new();
        
        for lib_path in &mut self.search_paths {
            // Load library if needed
            if lib_path.library.is_none() {
                if let Ok(library) = LibraryLoader::load_library(&lib_path.path) {
                    lib_path.library = Some(library);
                }
            }
            
            if let Some(library) = &lib_path.library {
                modules.extend(library.modules.keys().cloned());
            }
        }
        
        // Remove duplicates (keep first occurrence)
        let mut seen = std::collections::HashSet::new();
        modules.retain(|m| seen.insert(m.clone()));
        
        modules
    }
}