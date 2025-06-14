//! Database module for component storage and retrieval

pub mod schema;
pub mod queries;
pub mod migrations;

use rusqlite::{Connection, Result as SqliteResult};
use std::path::Path;
use crate::types::*;

/// Main database interface for component data
pub struct ComponentDatabase {
    connection: Connection,
}

impl ComponentDatabase {
    /// Create a new database connection and initialize schema
    pub async fn new(db_path: &Path) -> anyhow::Result<Self> {
        let connection = Connection::open(db_path)?;
        
        let mut db = Self { connection };
        db.initialize_schema().await?;
        
        Ok(db)
    }

    /// Initialize database schema
    async fn initialize_schema(&mut self) -> anyhow::Result<()> {
        migrations::run_migrations(&mut self.connection)?;
        Ok(())
    }

    /// Get component by ID
    pub async fn get_component(&self, id: ComponentId) -> anyhow::Result<Option<Component>> {
        queries::get_component_by_id(&self.connection, id)
    }

    /// Search components by name/description
    pub async fn search_components(&self, query: &str) -> anyhow::Result<Vec<Component>> {
        queries::search_components(&self.connection, query)
    }

    /// Get component symbol SVG
    pub async fn get_symbol_svg(&self, component_id: ComponentId) -> anyhow::Result<Option<String>> {
        queries::get_symbol_svg(&self.connection, component_id)
    }

    /// Insert a new component
    pub async fn insert_component(&self, component: &Component) -> anyhow::Result<ComponentId> {
        queries::insert_component(&self.connection, component)
    }

    /// Update component
    pub async fn update_component(&self, component: &Component) -> anyhow::Result<()> {
        queries::update_component(&self.connection, component)
    }

    /// Delete component
    pub async fn delete_component(&self, id: ComponentId) -> anyhow::Result<()> {
        queries::delete_component(&self.connection, id)
    }

    /// Get supplier data for component
    pub async fn get_supplier_data(&self, component_id: ComponentId) -> anyhow::Result<Vec<SupplierData>> {
        queries::get_supplier_data(&self.connection, component_id)
    }

    /// Insert or update supplier data
    pub async fn upsert_supplier_data(&self, supplier_data: &SupplierData) -> anyhow::Result<()> {
        queries::upsert_supplier_data(&self.connection, supplier_data)
    }

    /// Find components by electrical specifications
    pub async fn find_components_by_specs(
        &self,
        category: &ComponentCategory,
        specs: &[(String, f64, f64)], // (spec_name, min_value, max_value)
    ) -> anyhow::Result<Vec<Component>> {
        queries::find_components_by_specs(&self.connection, category, specs)
    }

    /// Get all components of a specific category
    pub async fn get_components_by_category(&self, category: &ComponentCategory) -> anyhow::Result<Vec<Component>> {
        queries::get_components_by_category(&self.connection, category)
    }

    /// Get component count statistics
    pub async fn get_component_stats(&self) -> anyhow::Result<ComponentStats> {
        queries::get_component_stats(&self.connection)
    }
}

/// Database statistics
#[derive(Debug)]
pub struct ComponentStats {
    pub total_components: u32,
    pub components_with_symbols: u32,
    pub components_with_supplier_data: u32,
    pub categories: std::collections::HashMap<String, u32>,
}

// Re-export for convenience
pub use schema::*;
pub use queries::*;
pub use migrations::*;