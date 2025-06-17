//! Module cache for compiled component libraries

use super::*;
use std::fs;
use std::time::SystemTime;
use anyhow::Result;
use serde::{Serialize, Deserialize};

/// Cached module representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedModule {
    pub module: ComponentModule,
    pub source_hash: String,
    pub compile_time: SystemTime,
}

/// Module cache manager
pub struct ModuleCache {
    cache_dir: PathBuf,
}

impl ModuleCache {
    pub fn new() -> Result<Self> {
        let cache_dir = Self::get_cache_dir()?;
        fs::create_dir_all(&cache_dir)?;
        
        Ok(Self { cache_dir })
    }
    
    /// Get the cache directory
    fn get_cache_dir() -> Result<PathBuf> {
        // Try environment variable first
        if let Ok(dir) = std::env::var("BHDL_CACHE_DIR") {
            return Ok(PathBuf::from(dir));
        }
        
        // Use system cache directory
        if let Some(cache_dir) = dirs::cache_dir() {
            return Ok(cache_dir.join("bhdl"));
        }
        
        // Fallback to temp directory
        Ok(std::env::temp_dir().join("bhdl-cache"))
    }
    
    /// Get cached module if valid
    pub fn get_cached_module(
        &self,
        source_path: &Path,
        current_hash: &str,
    ) -> Result<Option<CachedModule>> {
        let cache_path = self.get_cache_path(source_path);
        
        if !cache_path.exists() {
            return Ok(None);
        }
        
        // Read cached module
        let data = fs::read(&cache_path)?;
        let cached: CachedModule = bincode::deserialize(&data)?;
        
        // Check if cache is valid
        if cached.source_hash == current_hash {
            // Check if source file hasn't been modified
            if let Ok(metadata) = fs::metadata(source_path) {
                if let Ok(modified) = metadata.modified() {
                    if modified <= cached.compile_time {
                        return Ok(Some(cached));
                    }
                }
            }
        }
        
        Ok(None)
    }
    
    /// Cache a compiled module
    pub fn cache_module(
        &self,
        source_path: &Path,
        module: &ComponentModule,
        source_hash: String,
    ) -> Result<()> {
        let cache_path = self.get_cache_path(source_path);
        
        // Ensure parent directory exists
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let cached = CachedModule {
            module: module.clone(),
            source_hash,
            compile_time: SystemTime::now(),
        };
        
        let data = bincode::serialize(&cached)?;
        fs::write(cache_path, data)?;
        
        Ok(())
    }
    
    /// Get cache file path for a source file
    fn get_cache_path(&self, source_path: &Path) -> PathBuf {
        // Create a unique cache key from the source path
        let cache_key = format!("{:x}", md5::compute(source_path.to_string_lossy().as_bytes()));
        self.cache_dir.join(format!("{}.bhdlc", cache_key))
    }
    
    /// Clear all cached modules
    pub fn clear_cache(&self) -> Result<()> {
        for entry in fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            if entry.path().extension().and_then(|s| s.to_str()) == Some("bhdlc") {
                fs::remove_file(entry.path())?;
            }
        }
        Ok(())
    }
    
    /// Get cache statistics
    pub fn get_stats(&self) -> Result<CacheStats> {
        let mut stats = CacheStats::default();
        
        for entry in fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            if entry.path().extension().and_then(|s| s.to_str()) == Some("bhdlc") {
                stats.total_files += 1;
                if let Ok(metadata) = entry.metadata() {
                    stats.total_size += metadata.len();
                }
            }
        }
        
        Ok(stats)
    }
}

#[derive(Debug, Default)]
pub struct CacheStats {
    pub total_files: usize,
    pub total_size: u64,
}

// Add dependencies to Cargo.toml:
// - dirs = "5.0"
// - bincode = "1.3"
// - md5 = "0.7"