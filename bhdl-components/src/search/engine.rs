//! Advanced component search engine implementation

use std::collections::HashMap;
use anyhow::{Result, Context};
use log::{debug, info};

use crate::types::{Component, ComponentCategory, ElectricalSpec};
use crate::database::ComponentDatabase;

/// Advanced search engine for components with multiple search strategies
pub struct SearchEngine {
    database: ComponentDatabase,
}

/// Search query with multiple filters and criteria
#[derive(Debug, Clone)]
pub struct SearchQuery {
    /// Text search across name, description, manufacturer, part number
    pub text: Option<String>,
    /// Filter by component category
    pub category: Option<ComponentCategory>,
    /// Filter by manufacturer name
    pub manufacturer: Option<String>,
    /// Filter by package type
    pub package_type: Option<String>,
    /// Electrical specification requirements
    pub electrical_specs: Vec<SpecificationFilter>,
    /// Maximum number of results to return
    pub limit: Option<usize>,
    /// Minimum relevance score threshold (0.0 to 1.0)
    pub min_relevance: Option<f64>,
}

/// Filter for electrical specifications with range support
#[derive(Debug, Clone)]
pub struct SpecificationFilter {
    /// Specification name (e.g., "resistance", "voltage", "current")
    pub spec_name: String,
    /// Minimum value (inclusive)
    pub min_value: Option<f64>,
    /// Maximum value (inclusive)
    pub max_value: Option<f64>,
    /// Exact value match
    pub exact_value: Option<f64>,
    /// Unit for the specification
    pub unit: Option<String>,
    /// Tolerance requirement
    pub max_tolerance: Option<f64>,
}

/// Search result with relevance scoring
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub component: Component,
    pub relevance_score: f64,
    pub match_reasons: Vec<String>,
}

impl SearchEngine {
    /// Create a new search engine
    pub fn new(database: ComponentDatabase) -> Self {
        Self { database }
    }
    
    /// Execute a search query with advanced filtering and ranking
    pub async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        info!("Executing search query: {:?}", query);
        
        // Phase 1: Get initial candidates from database
        let candidates = self.get_candidates(query).await
            .context("Failed to retrieve search candidates")?;
        
        debug!("Found {} initial candidates", candidates.len());
        
        // Phase 2: Score and rank candidates
        let mut results = self.score_candidates(&candidates, query)?;
        
        // Phase 3: Apply relevance threshold
        if let Some(min_relevance) = query.min_relevance {
            results.retain(|r| r.relevance_score >= min_relevance);
        }
        
        // Phase 4: Sort by relevance score (highest first)
        results.sort_by(|a, b| b.relevance_score.partial_cmp(&a.relevance_score).unwrap());
        
        // Phase 5: Apply result limit
        if let Some(limit) = query.limit {
            results.truncate(limit);
        }
        
        info!("Returning {} search results", results.len());
        Ok(results)
    }
    
    /// Get initial candidates from database using SQL queries
    async fn get_candidates(&self, query: &SearchQuery) -> Result<Vec<Component>> {
        // Build SQL conditions based on query
        let mut conditions = Vec::new();
        let mut params = Vec::new();
        
        // Category filter
        if let Some(ref category) = query.category {
            conditions.push("category = ?".to_string());
            params.push(category.as_str().to_string());
        }
        
        // Manufacturer filter
        if let Some(ref manufacturer) = query.manufacturer {
            conditions.push("manufacturer LIKE ?".to_string());
            params.push(format!("%{}%", manufacturer));
        }
        
        // Package type filter
        if let Some(ref package_type) = query.package_type {
            conditions.push("package_type LIKE ?".to_string());
            params.push(format!("%{}%", package_type));
        }
        
        // Text search across multiple fields
        if let Some(ref text) = query.text {
            conditions.push(
                "(name LIKE ? OR description LIKE ? OR part_number LIKE ? OR manufacturer LIKE ?)".to_string()
            );
            let search_term = format!("%{}%", text);
            params.extend(vec![search_term.clone(), search_term.clone(), search_term.clone(), search_term]);
        }
        
        // Execute database query
        let components = if conditions.is_empty() {
            self.database.get_all_components().await?
        } else {
            let where_clause = conditions.join(" AND ");
            self.database.search_components_advanced(&where_clause, &params).await?
        };
        
        Ok(components)
    }
    
    /// Score candidates based on relevance to search query
    fn score_candidates(&self, candidates: &[Component], query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let mut results = Vec::new();
        
        for component in candidates {
            let (score, reasons) = self.calculate_relevance_score(component, query);
            
            results.push(SearchResult {
                component: component.clone(),
                relevance_score: score,
                match_reasons: reasons,
            });
        }
        
        Ok(results)
    }
    
    /// Calculate relevance score for a component (0.0 to 1.0)
    fn calculate_relevance_score(&self, component: &Component, query: &SearchQuery) -> (f64, Vec<String>) {
        let mut total_score = 0.0;
        let mut max_score = 0.0;
        let mut reasons = Vec::new();
        
        // Text matching score (weight: 0.4)
        if let Some(ref search_text) = query.text {
            let (text_score, text_reasons) = self.calculate_text_score(component, search_text);
            total_score += text_score * 0.4;
            max_score += 0.4;
            reasons.extend(text_reasons);
        }
        
        // Category exact match (weight: 0.2)
        if let Some(ref category) = query.category {
            if component.category.as_str() == category.as_str() {
                total_score += 0.2;
                reasons.push(format!("Category match: {}", category.as_str()));
            }
            max_score += 0.2;
        }
        
        // Electrical specifications matching (weight: 0.3)
        if !query.electrical_specs.is_empty() {
            let (spec_score, spec_reasons) = self.calculate_spec_score(component, &query.electrical_specs);
            total_score += spec_score * 0.3;
            max_score += 0.3;
            reasons.extend(spec_reasons);
        }
        
        // Package type matching (weight: 0.1)
        if let Some(ref package) = query.package_type {
            if let Some(ref comp_package) = component.package_type {
                if comp_package.to_lowercase().contains(&package.to_lowercase()) {
                    total_score += 0.1;
                    reasons.push(format!("Package type match: {}", comp_package));
                }
            }
            max_score += 0.1;
        }
        
        // Normalize score (avoid division by zero)
        let final_score = if max_score > 0.0 {
            total_score / max_score
        } else {
            1.0 // If no filters, all components have perfect score
        };
        
        (final_score.min(1.0), reasons)
    }
    
    /// Calculate text matching score using fuzzy string matching
    fn calculate_text_score(&self, component: &Component, search_text: &str) -> (f64, Vec<String>) {
        let search_lower = search_text.to_lowercase();
        let mut score = 0.0;
        let mut reasons = Vec::new();
        
        // Exact name match gets highest score
        if component.name.to_lowercase() == search_lower {
            score = 1.0;
            reasons.push("Exact name match".to_string());
            return (score, reasons);
        }
        
        // Partial matches with different weights
        if component.name.to_lowercase().contains(&search_lower) {
            score += 0.8;
            reasons.push(format!("Name contains '{}'", search_text));
        }
        
        if let Some(ref part_number) = component.part_number {
            if part_number.to_lowercase().contains(&search_lower) {
                score += 0.6;
                reasons.push(format!("Part number contains '{}'", search_text));
            }
        }
        
        if let Some(ref description) = component.description {
            if description.to_lowercase().contains(&search_lower) {
                score += 0.4;
                reasons.push(format!("Description contains '{}'", search_text));
            }
        }
        
        if let Some(ref manufacturer) = component.manufacturer {
            if manufacturer.to_lowercase().contains(&search_lower) {
                score += 0.3;
                reasons.push(format!("Manufacturer contains '{}'", search_text));
            }
        }
        
        (score.min(1.0), reasons)
    }
    
    /// Calculate electrical specification matching score
    fn calculate_spec_score(&self, component: &Component, filters: &[SpecificationFilter]) -> (f64, Vec<String>) {
        if filters.is_empty() {
            return (1.0, vec![]);
        }
        
        let mut matched_specs = 0;
        let mut reasons = Vec::new();
        
        for filter in filters {
            if self.matches_spec_filter(component, filter) {
                matched_specs += 1;
                reasons.push(format!("Matches {} specification", filter.spec_name));
            }
        }
        
        let score = matched_specs as f64 / filters.len() as f64;
        (score, reasons)
    }
    
    /// Check if component matches a specification filter
    fn matches_spec_filter(&self, component: &Component, filter: &SpecificationFilter) -> bool {
        for spec in &component.electrical_specs {
            // Check if this is the spec we're looking for
            if spec.spec_name.to_lowercase() != filter.spec_name.to_lowercase() {
                continue;
            }
            
            // Check unit compatibility if specified
            if let Some(ref required_unit) = filter.unit {
                if spec.spec_unit.to_lowercase() != required_unit.to_lowercase() {
                    continue;
                }
            }
            
            // Check exact value match
            if let Some(exact_value) = filter.exact_value {
                if (spec.spec_value - exact_value).abs() < 1e-9 {
                    return true;
                }
                continue;
            }
            
            // Check range constraints
            let mut matches_range = true;
            
            if let Some(min_val) = filter.min_value {
                if spec.spec_value < min_val {
                    matches_range = false;
                }
            }
            
            if let Some(max_val) = filter.max_value {
                if spec.spec_value > max_val {
                    matches_range = false;
                }
            }
            
            // Check tolerance requirement
            if let Some(max_tolerance) = filter.max_tolerance {
                if let Some(tolerance) = spec.spec_tolerance {
                    if tolerance > max_tolerance {
                        matches_range = false;
                    }
                }
            }
            
            if matches_range {
                return true;
            }
        }
        
        false
    }
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            text: None,
            category: None,
            manufacturer: None,
            package_type: None,
            electrical_specs: Vec::new(),
            limit: Some(50),
            min_relevance: Some(0.1),
        }
    }
}