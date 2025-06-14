//! Multi-level caching system for component data

pub mod multi_level;
pub mod preloader;

use lru::LruCache;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock as AsyncRwLock;
use std::sync::RwLock;
use std::time::Instant;

use crate::types::{Component, ComponentId};

/// Main component cache with multiple levels
pub struct ComponentCache {
    // L1: Hot components (most recently/frequently used)
    hot_cache: Arc<AsyncRwLock<LruCache<ComponentId, Component>>>,
    
    // L2: Symbol SVG cache (larger, less frequently evicted)
    symbol_cache: Arc<AsyncRwLock<LruCache<ComponentId, String>>>,
    
    // L3: Search results cache (time-based expiry)
    search_cache: Arc<RwLock<HashMap<String, (Vec<Component>, Instant)>>>,
    
    // Cache statistics
    stats: Arc<RwLock<CacheStats>>,
}

/// Cache performance statistics
#[derive(Debug, Default, Clone)]
pub struct CacheStats {
    pub component_hits: u64,
    pub component_misses: u64,
    pub symbol_hits: u64,
    pub symbol_misses: u64,
    pub search_hits: u64,
    pub search_misses: u64,
}

impl CacheStats {
    pub fn component_hit_rate(&self) -> f64 {
        let total = self.component_hits + self.component_misses;
        if total == 0 {
            0.0
        } else {
            self.component_hits as f64 / total as f64
        }
    }
    
    pub fn symbol_hit_rate(&self) -> f64 {
        let total = self.symbol_hits + self.symbol_misses;
        if total == 0 {
            0.0
        } else {
            self.symbol_hits as f64 / total as f64
        }
    }
    
    pub fn search_hit_rate(&self) -> f64 {
        let total = self.search_hits + self.search_misses;
        if total == 0 {
            0.0
        } else {
            self.search_hits as f64 / total as f64
        }
    }
}

impl ComponentCache {
    /// Create a new component cache
    pub fn new() -> Self {
        Self {
            hot_cache: Arc::new(AsyncRwLock::new(LruCache::new(
                std::num::NonZeroUsize::new(1000).unwrap()
            ))),
            symbol_cache: Arc::new(AsyncRwLock::new(LruCache::new(
                std::num::NonZeroUsize::new(5000).unwrap()
            ))),
            search_cache: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(CacheStats::default())),
        }
    }
    
    /// Get component from cache
    pub async fn get_component(&self, id: ComponentId) -> Option<Component> {
        let component = self.hot_cache.read().await.peek(&id).cloned();
        
        // Update stats
        {
            let mut stats = self.stats.write().unwrap();
            if component.is_some() {
                stats.component_hits += 1;
            } else {
                stats.component_misses += 1;
            }
        }
        
        component
    }
    
    /// Cache a component
    pub async fn cache_component(&self, id: ComponentId, component: Component) {
        self.hot_cache.write().await.put(id, component);
    }
    
    /// Get symbol SVG from cache
    pub async fn get_symbol_svg(&self, component_id: ComponentId) -> Option<String> {
        let svg = self.symbol_cache.read().await.peek(&component_id).cloned();
        
        // Update stats
        {
            let mut stats = self.stats.write().unwrap();
            if svg.is_some() {
                stats.symbol_hits += 1;
            } else {
                stats.symbol_misses += 1;
            }
        }
        
        svg
    }
    
    /// Cache symbol SVG
    pub async fn cache_symbol_svg(&self, component_id: ComponentId, svg: String) {
        self.symbol_cache.write().await.put(component_id, svg);
    }
    
    /// Get search results from cache
    pub fn get_search_results(&self, query: &str) -> Option<Vec<Component>> {
        let cache = self.search_cache.read().unwrap();
        let result = if let Some((results, timestamp)) = cache.get(query) {
            // Expire after 5 minutes
            if timestamp.elapsed().as_secs() < 300 {
                Some(results.clone())
            } else {
                None
            }
        } else {
            None
        };
        
        // Update stats
        {
            let mut stats = self.stats.write().unwrap();
            if result.is_some() {
                stats.search_hits += 1;
            } else {
                stats.search_misses += 1;
            }
        }
        
        result
    }
    
    /// Cache search results
    pub fn cache_search_results(&self, query: String, results: Vec<Component>) {
        let mut cache = self.search_cache.write().unwrap();
        cache.insert(query, (results, Instant::now()));
        
        // Cleanup old entries (simple approach)
        if cache.len() > 1000 {
            cache.retain(|_, (_, timestamp)| timestamp.elapsed().as_secs() < 300);
        }
    }
    
    /// Clear all caches
    pub async fn clear_all(&self) {
        self.hot_cache.write().await.clear();
        self.symbol_cache.write().await.clear();
        self.search_cache.write().unwrap().clear();
    }
    
    /// Get cache statistics
    pub fn get_stats(&self) -> CacheStats {
        self.stats.read().unwrap().clone()
    }
    
    /// Get cache sizes
    pub async fn get_cache_sizes(&self) -> CacheSizes {
        let hot_len = self.hot_cache.read().await.len();
        let symbol_len = self.symbol_cache.read().await.len();
        let search_len = self.search_cache.read().unwrap().len();
        
        CacheSizes {
            hot_cache_size: hot_len,
            symbol_cache_size: symbol_len,
            search_cache_size: search_len,
        }
    }
    
    /// Preload common components
    pub async fn preload_common_components(&self, components: Vec<Component>) {
        let mut hot_cache = self.hot_cache.write().await;
        for component in components {
            hot_cache.put(component.id, component);
        }
    }
    
    /// Evict least recently used entries to make room
    pub async fn evict_lru(&self, count: usize) {
        let mut hot_cache = self.hot_cache.write().await;
        for _ in 0..count {
            if hot_cache.pop_lru().is_none() {
                break;
            }
        }
    }
}

#[derive(Debug)]
pub struct CacheSizes {
    pub hot_cache_size: usize,
    pub symbol_cache_size: usize,
    pub search_cache_size: usize,
}

impl Default for ComponentCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ComponentCategory, ElectricalSpec};

    fn create_test_component(id: ComponentId, name: &str) -> Component {
        Component {
            id,
            name: name.to_string(),
            description: Some("Test component".to_string()),
            manufacturer: Some("Test Mfg".to_string()),
            part_number: Some(format!("TEST-{}", id)),
            package_type: Some("0805".to_string()),
            category: ComponentCategory::Resistor,
            subcategory: None,
            datasheet_url: None,
            electrical_specs: vec![
                ElectricalSpec {
                    spec_name: "resistance".to_string(),
                    spec_value: 1000.0,
                    spec_unit: "Ω".to_string(),
                    spec_tolerance: Some(0.05),
                    min_value: Some(950.0),
                    max_value: Some(1050.0),
                    conditions: None,
                }
            ],
            pins: vec![],
            symbol: None,
            footprint: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_component_caching() {
        let cache = ComponentCache::new();
        let component = create_test_component(1, "Test Resistor");
        
        // Should be empty initially
        assert!(cache.get_component(1).await.is_none());
        
        // Cache the component
        cache.cache_component(1, component.clone()).await;
        
        // Should now be available
        let cached = cache.get_component(1).await.unwrap();
        assert_eq!(cached.name, component.name);
        
        // Check stats
        let stats = cache.get_stats();
        assert_eq!(stats.component_hits, 1);
        assert_eq!(stats.component_misses, 1);
    }

    #[tokio::test]
    async fn test_symbol_caching() {
        let cache = ComponentCache::new();
        let svg_data = "<svg>test</svg>".to_string();
        
        // Should be empty initially
        assert!(cache.get_symbol_svg(1).await.is_none());
        
        // Cache the SVG
        cache.cache_symbol_svg(1, svg_data.clone()).await;
        
        // Should now be available
        let cached_svg = cache.get_symbol_svg(1).await.unwrap();
        assert_eq!(cached_svg, svg_data);
    }

    #[test]
    fn test_search_caching() {
        let cache = ComponentCache::new();
        let query = "resistor";
        let results = vec![create_test_component(1, "Test Resistor")];
        
        // Should be empty initially
        assert!(cache.get_search_results(query).is_none());
        
        // Cache the results
        cache.cache_search_results(query.to_string(), results.clone());
        
        // Should now be available
        let cached_results = cache.get_search_results(query).unwrap();
        assert_eq!(cached_results.len(), results.len());
        assert_eq!(cached_results[0].name, results[0].name);
    }

    #[tokio::test]
    async fn test_cache_sizes() {
        let cache = ComponentCache::new();
        let component = create_test_component(1, "Test");
        let svg = "<svg>test</svg>".to_string();
        
        // Cache some data
        cache.cache_component(1, component).await;
        cache.cache_symbol_svg(1, svg).await;
        cache.cache_search_results("test".to_string(), vec![]);
        
        let sizes = cache.get_cache_sizes().await;
        assert_eq!(sizes.hot_cache_size, 1);
        assert_eq!(sizes.symbol_cache_size, 1);
        assert_eq!(sizes.search_cache_size, 1);
    }

    #[tokio::test]
    async fn test_clear_all() {
        let cache = ComponentCache::new();
        let component = create_test_component(1, "Test");
        
        // Cache some data
        cache.cache_component(1, component).await;
        cache.cache_symbol_svg(1, "<svg>test</svg>".to_string()).await;
        cache.cache_search_results("test".to_string(), vec![]);
        
        // Clear all caches
        cache.clear_all().await;
        
        // Should be empty
        let sizes = cache.get_cache_sizes().await;
        assert_eq!(sizes.hot_cache_size, 0);
        assert_eq!(sizes.symbol_cache_size, 0);
        assert_eq!(sizes.search_cache_size, 0);
    }
}