//! BHDL Component Library
//! 
//! This crate provides the component library infrastructure for BHDL,
//! enabling the integration of real-world electronic components with
//! supply chain intelligence and manufacturing data.

// Public API modules
pub mod database;
pub mod cache;
pub mod kicad;
pub mod supplier;
pub mod synthesis;
pub mod search;
pub mod types;
pub mod config;

// Re-export key types for convenience
pub use types::{
    Component, ComponentId, SupplierData, SynthesisResult, ComponentOption,
    ElectricalSpec, PinDefinition, ComponentSymbol, ComponentFootprint
};

pub use database::ComponentDatabase;
pub use cache::ComponentCache;
pub use synthesis::{ComponentSynthesizer, SynthesisEngine, TwoStageSynthesizer, TwoStageConfig};
pub use search::ComponentSearchEngine;

/// Main component library API
pub struct ComponentLibrary {
    database: ComponentDatabase,
    cache: ComponentCache,
    synthesizer: ComponentSynthesizer,
    search_engine: ComponentSearchEngine,
}

impl ComponentLibrary {
    /// Create a new component library instance
    pub async fn new(db_path: &std::path::Path) -> anyhow::Result<Self> {
        let database = ComponentDatabase::new(db_path).await?;
        let cache = ComponentCache::new();
        let synthesizer = ComponentSynthesizer::new();
        let search_engine = ComponentSearchEngine::new(database.clone());

        Ok(Self {
            database,
            cache,
            synthesizer,
            search_engine,
        })
    }

    /// Search for components matching query
    pub async fn search_components(&self, query: &str) -> anyhow::Result<Vec<Component>> {
        // Try cache first
        if let Some(cached_results) = self.cache.get_search_results(query) {
            return Ok(cached_results);
        }

        // Search database directly (bypass search engine for now)
        let results = self.database.search_components(query).await?;

        // Cache results
        self.cache.cache_search_results(query.to_string(), results.clone());

        Ok(results)
    }

    /// Get component by ID with caching
    pub async fn get_component(&self, id: ComponentId) -> anyhow::Result<Option<Component>> {
        // Try cache first
        if let Some(cached_component) = self.cache.get_component(id).await {
            return Ok(Some(cached_component));
        }

        // Query database
        if let Some(component) = self.database.get_component(id).await? {
            // Cache for next time
            self.cache.cache_component(id, component.clone()).await;
            Ok(Some(component))
        } else {
            Ok(None)
        }
    }

    /// Get component symbol SVG with caching
    pub async fn get_component_symbol(&self, component_id: ComponentId) -> anyhow::Result<Option<String>> {
        // Try cache first
        if let Some(cached_svg) = self.cache.get_symbol_svg(component_id).await {
            return Ok(Some(cached_svg));
        }

        // Query database
        if let Some(svg) = self.database.get_symbol_svg(component_id).await? {
            // Cache for next time
            self.cache.cache_symbol_svg(component_id, svg.clone()).await;
            Ok(Some(svg))
        } else {
            Ok(None)
        }
    }

    /// Synthesize a component from generic requirements
    pub async fn synthesize_component(
        &self,
        component_type: &str,
        requirements: &types::ComponentRequirements,
    ) -> anyhow::Result<SynthesisResult> {
        self.synthesizer.synthesize(component_type, requirements, &self.database).await
    }

    /// Insert a component into the database
    pub async fn insert_component(&self, component: &Component) -> anyhow::Result<ComponentId> {
        self.database.insert_component(component).await
    }

    /// Get component statistics
    pub async fn get_stats(&self) -> anyhow::Result<database::ComponentStats> {
        self.database.get_component_stats().await
    }

    /// Get cache statistics
    pub fn get_cache_stats(&self) -> cache::CacheStats {
        self.cache.get_stats()
    }

    /// Get cache sizes
    pub async fn get_cache_sizes(&self) -> cache::CacheSizes {
        self.cache.get_cache_sizes().await
    }

    /// Get supplier data for a component
    pub async fn get_supplier_data(&self, component_id: ComponentId) -> anyhow::Result<Option<SupplierData>> {
        self.database.get_supplier_data(component_id).await
    }

    /// Update supplier data for a component
    pub async fn update_supplier_data(&self, supplier_data: &SupplierData) -> anyhow::Result<()> {
        self.database.upsert_supplier_data(supplier_data).await
    }

    /// Find components by electrical specifications
    pub async fn find_components_by_specs(
        &self,
        category: &types::ComponentCategory,
        specs: &[(String, f64, f64)],
    ) -> anyhow::Result<Vec<Component>> {
        self.database.find_components_by_specs(category, specs).await
    }

    /// Get components by category
    pub async fn get_components_by_category(&self, category: &types::ComponentCategory) -> anyhow::Result<Vec<Component>> {
        self.database.get_components_by_category(category).await
    }

    /// Get reference to the database (for synthesis engine)
    pub fn get_database(&self) -> &ComponentDatabase {
        &self.database
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_component_library_creation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        let _library = ComponentLibrary::new(&db_path).await.unwrap();
        // Basic creation test - more detailed tests in integration tests
    }
}