//! Database module for component storage and retrieval

pub mod schema;
pub mod queries;
pub mod migrations;

use rusqlite::{Connection, Result as SqliteResult};
use std::path::Path;
use std::sync::{Arc, Mutex};
use crate::types::*;

/// Main database interface for component data
#[derive(Clone)]
pub struct ComponentDatabase {
    connection: Arc<Mutex<Connection>>,
}

impl ComponentDatabase {
    /// Create a new database connection and initialize schema
    pub async fn new(db_path: &Path) -> anyhow::Result<Self> {
        let connection = Connection::open(db_path)?;
        
        let db = Self {
            connection: Arc::new(Mutex::new(connection)),
        };
        db.initialize_schema().await?;
        
        Ok(db)
    }

    /// Initialize database schema
    async fn initialize_schema(&self) -> anyhow::Result<()> {
        let mut conn = self.connection.lock().unwrap();
        migrations::run_migrations(&mut *conn)?;
        Ok(())
    }

    /// Get component by ID
    pub async fn get_component(&self, id: ComponentId) -> anyhow::Result<Option<Component>> {
        let conn = self.connection.lock().unwrap();
        queries::get_component_by_id(&*conn, id)
    }

    /// Get component ID by name
    pub async fn get_component_id_by_name(&self, name: &str) -> anyhow::Result<ComponentId> {
        let conn = self.connection.lock().unwrap();
        queries::get_component_id_by_name(&*conn, name)
    }

    /// Search components by name/description
    pub async fn search_components(&self, query: &str) -> anyhow::Result<Vec<Component>> {
        let conn = self.connection.lock().unwrap();
        queries::search_components(&*conn, query)
    }
    
    /// Advanced search with custom WHERE clause and parameters
    pub async fn search_components_advanced(&self, where_clause: &str, params: &[String]) -> anyhow::Result<Vec<Component>> {
        let conn = self.connection.lock().unwrap();
        queries::search_components_advanced(&*conn, where_clause, params)
    }
    
    /// Get all components in the database
    pub async fn get_all_components(&self) -> anyhow::Result<Vec<Component>> {
        let conn = self.connection.lock().unwrap();
        queries::get_all_components(&*conn)
    }

    /// Get component symbol SVG
    pub async fn get_symbol_svg(&self, component_id: ComponentId) -> anyhow::Result<Option<String>> {
        let conn = self.connection.lock().unwrap();
        queries::get_symbol_svg(&*conn, component_id)
    }

    /// Insert a new component
    pub async fn insert_component(&self, component: &Component) -> anyhow::Result<ComponentId> {
        let mut conn = self.connection.lock().unwrap();
        queries::insert_component(&mut *conn, component)
    }

    /// Update component
    pub async fn update_component(&self, component: &Component) -> anyhow::Result<()> {
        let mut conn = self.connection.lock().unwrap();
        queries::update_component(&mut *conn, component)
    }

    /// Delete component
    pub async fn delete_component(&self, id: ComponentId) -> anyhow::Result<()> {
        let mut conn = self.connection.lock().unwrap();
        queries::delete_component(&mut *conn, id)
    }

    /// Get supplier data for component
    pub async fn get_supplier_data(&self, component_id: ComponentId) -> anyhow::Result<Option<SupplierData>> {
        let conn = self.connection.lock().unwrap();
        queries::get_supplier_data(&*conn, component_id)
    }

    /// Insert or update supplier data
    pub async fn upsert_supplier_data(&self, supplier_data: &SupplierData) -> anyhow::Result<()> {
        let mut conn = self.connection.lock().unwrap();
        queries::upsert_supplier_data(&mut *conn, supplier_data)
    }

    /// Find components by electrical specifications
    pub async fn find_components_by_specs(
        &self,
        category: &ComponentCategory,
        specs: &[(String, f64, f64)], // (spec_name, min_value, max_value)
    ) -> anyhow::Result<Vec<Component>> {
        let conn = self.connection.lock().unwrap();
        queries::find_components_by_specs(&*conn, category, specs)
    }

    /// Get all components of a specific category
    pub async fn get_components_by_category(&self, category: &ComponentCategory) -> anyhow::Result<Vec<Component>> {
        let conn = self.connection.lock().unwrap();
        queries::get_components_by_category(&*conn, category)
    }

    /// Get component count statistics
    pub async fn get_component_stats(&self) -> anyhow::Result<ComponentStats> {
        let conn = self.connection.lock().unwrap();
        queries::get_component_stats(&*conn)
    }

    /// Count components with supplier data
    pub async fn count_components_with_supplier_data(&self) -> anyhow::Result<u32> {
        let conn = self.connection.lock().unwrap();
        queries::count_components_with_supplier_data(&*conn)
    }

    /// Find components with stale supplier data
    pub async fn find_components_with_stale_supplier_data(&self, cutoff_time: chrono::DateTime<chrono::Utc>) -> anyhow::Result<Vec<ComponentId>> {
        let conn = self.connection.lock().unwrap();
        queries::find_components_with_stale_supplier_data(&*conn, cutoff_time)
    }

    /// Count total components
    pub async fn count_components(&self) -> anyhow::Result<u32> {
        let conn = self.connection.lock().unwrap();
        queries::count_components(&*conn)
    }

    /// Insert component footprint
    pub async fn insert_component_footprint(&self, component_id: ComponentId, footprint: &ComponentFootprint) -> anyhow::Result<()> {
        let mut conn = self.connection.lock().unwrap();
        queries::insert_component_footprint(&mut *conn, component_id, footprint)
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