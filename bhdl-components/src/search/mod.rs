//! Advanced component search module with multi-criteria filtering and relevance scoring

pub mod engine;
pub mod filters;

// Re-export key types
pub use engine::{SearchEngine, SearchQuery, SearchResult, SpecificationFilter};

use crate::types::Component;
use crate::database::ComponentDatabase;

/// High-level component search engine wrapper
pub struct ComponentSearchEngine {
    engine: SearchEngine,
}

impl ComponentSearchEngine {
    /// Create a new search engine wrapper
    pub fn new(database: ComponentDatabase) -> Self {
        Self {
            engine: SearchEngine::new(database),
        }
    }
    
    /// Simple text search interface for backwards compatibility
    pub async fn search(&self, query_text: &str) -> anyhow::Result<Vec<Component>> {
        let query = SearchQuery {
            text: Some(query_text.to_string()),
            ..Default::default()
        };
        
        let results = self.engine.search(&query).await?;
        Ok(results.into_iter().map(|r| r.component).collect())
    }
    
    /// Advanced search with full query capabilities
    pub async fn advanced_search(&self, query: &SearchQuery) -> anyhow::Result<Vec<SearchResult>> {
        self.engine.search(query).await
    }
    
    /// Search for components by category
    pub async fn search_by_category(&self, category: crate::types::ComponentCategory) -> anyhow::Result<Vec<Component>> {
        let query = SearchQuery {
            category: Some(category),
            ..Default::default()
        };
        
        let results = self.engine.search(&query).await?;
        Ok(results.into_iter().map(|r| r.component).collect())
    }
    
    /// Search for components with specific electrical specifications
    pub async fn search_by_specs(&self, spec_filters: Vec<SpecificationFilter>) -> anyhow::Result<Vec<Component>> {
        let query = SearchQuery {
            electrical_specs: spec_filters,
            ..Default::default()
        };
        
        let results = self.engine.search(&query).await?;
        Ok(results.into_iter().map(|r| r.component).collect())
    }
}