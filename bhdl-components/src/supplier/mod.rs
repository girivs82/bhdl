//! Supplier Data Integration
//! 
//! Manages integration with component suppliers and distributors to provide
//! real-time availability, pricing, and supply chain information.

pub mod trustedparts;
pub mod nexar;
pub mod digikey;
pub mod multi_backend;
pub mod cache;

use anyhow::Result;
use log::{info, warn, error};
use std::collections::HashMap;
use chrono::{DateTime, Utc, Duration};

use crate::types::{ComponentId, SupplierData};
use crate::database::ComponentDatabase;
use trustedparts::{TrustedPartsClient, TrustedPartsConfig};

/// Supplier integration service that manages data from multiple suppliers
pub struct SupplierService {
    /// TrustedParts API client
    trustedparts_client: TrustedPartsClient,
    /// Database for caching supplier data
    database: ComponentDatabase,
    /// Cache refresh interval (hours)
    refresh_interval_hours: i64,
}

impl SupplierService {
    /// Create a new supplier service
    pub async fn new(
        database_path: &std::path::Path,
        trustedparts_config: TrustedPartsConfig,
    ) -> Result<Self> {
        let trustedparts_client = TrustedPartsClient::new(trustedparts_config)?;
        let database = ComponentDatabase::new(database_path).await?;
        
        Ok(Self {
            trustedparts_client,
            database,
            refresh_interval_hours: 24, // Default to 24 hour refresh
        })
    }

    /// Set the cache refresh interval
    pub fn set_refresh_interval_hours(&mut self, hours: i64) {
        self.refresh_interval_hours = hours;
    }

    /// Update supplier data for a specific component
    pub async fn update_component_supplier_data(&self, component_id: ComponentId, part_number: &str) -> Result<()> {
        info!("Updating supplier data for component {} ({})", component_id, part_number);

        // Search TrustedParts for the component
        let search_results = self.trustedparts_client.search_part(part_number).await?;
        
        if search_results.is_empty() {
            warn!("No supplier data found for part number: {}", part_number);
            return Ok(());
        }

        // Get detailed information for the best match
        let best_match = &search_results[0]; // Take the first result as best match
        let details = self.trustedparts_client
            .get_component_details(&best_match.uid)
            .await
            .ok(); // Convert to Option, don't fail if details unavailable

        // Convert to our internal format
        let supplier_data = self.trustedparts_client
            .convert_to_supplier_data(component_id, best_match, details.as_ref());

        // Store in database
        self.database.upsert_supplier_data(&supplier_data).await?;
        
        info!("Updated supplier data for {} with {} suppliers", 
              part_number, supplier_data.suppliers.len());

        Ok(())
    }

    /// Update supplier data for multiple components
    pub async fn update_multiple_components(&self, components: &[(ComponentId, String)]) -> Result<SupplierUpdateResult> {
        let mut result = SupplierUpdateResult::new();
        
        info!("Updating supplier data for {} components", components.len());

        for (component_id, part_number) in components {
            match self.update_component_supplier_data(*component_id, part_number).await {
                Ok(_) => {
                    result.successful_updates += 1;
                }
                Err(e) => {
                    error!("Failed to update supplier data for {}: {}", part_number, e);
                    result.failed_updates += 1;
                    result.errors.push(format!("{}: {}", part_number, e));
                }
            }

            // Add small delay to avoid overwhelming the API
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        info!("Supplier update complete: {} successful, {} failed", 
              result.successful_updates, result.failed_updates);

        Ok(result)
    }

    /// Get cached supplier data for a component
    pub async fn get_supplier_data(&self, component_id: ComponentId) -> Result<Option<SupplierData>> {
        self.database.get_supplier_data(component_id).await
    }

    /// Check if supplier data needs refreshing
    pub async fn needs_refresh(&self, component_id: ComponentId) -> Result<bool> {
        if let Some(supplier_data) = self.get_supplier_data(component_id).await? {
            let age = Utc::now() - supplier_data.last_updated;
            Ok(age > Duration::hours(self.refresh_interval_hours))
        } else {
            Ok(true) // No data means we need to fetch it
        }
    }

    /// Check if supplier data is fresh within the given hours
    pub async fn is_data_fresh(&self, component_id: ComponentId, max_age_hours: i64) -> Result<bool> {
        if let Some(supplier_data) = self.get_supplier_data(component_id).await? {
            let age = Utc::now() - supplier_data.last_updated;
            Ok(age <= Duration::hours(max_age_hours))
        } else {
            Ok(false) // No data means it's not fresh
        }
    }

    /// Refresh supplier data if needed
    pub async fn refresh_if_needed(&self, component_id: ComponentId, part_number: &str) -> Result<bool> {
        if self.needs_refresh(component_id).await? {
            self.update_component_supplier_data(component_id, part_number).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get supplier data, refreshing if needed
    pub async fn get_fresh_supplier_data(&self, component_id: ComponentId, part_number: &str) -> Result<Option<SupplierData>> {
        // Refresh if needed
        self.refresh_if_needed(component_id, part_number).await?;
        
        // Return cached data
        self.get_supplier_data(component_id).await
    }

    /// Find components with stale supplier data
    pub async fn find_stale_components(&self) -> Result<Vec<ComponentId>> {
        let cutoff_time = Utc::now() - Duration::hours(self.refresh_interval_hours);
        self.database.find_components_with_stale_supplier_data(cutoff_time).await
    }

    /// Bulk refresh all stale supplier data
    pub async fn refresh_stale_data(&self) -> Result<SupplierUpdateResult> {
        let stale_components = self.find_stale_components().await?;
        
        if stale_components.is_empty() {
            info!("No stale supplier data found");
            return Ok(SupplierUpdateResult::new());
        }

        info!("Found {} components with stale supplier data", stale_components.len());

        // Get component part numbers for the stale components
        let mut components_to_update = Vec::new();
        for component_id in stale_components {
            if let Some(component) = self.database.get_component(component_id).await? {
                if let Some(part_number) = component.part_number {
                    components_to_update.push((component_id, part_number));
                } else if !component.name.is_empty() {
                    // Fall back to using component name if no part number
                    components_to_update.push((component_id, component.name));
                }
            }
        }

        self.update_multiple_components(&components_to_update).await
    }

    /// Get supplier statistics
    pub async fn get_supplier_stats(&self) -> Result<SupplierStats> {
        let total_components = self.database.count_components().await?;
        let components_with_supplier_data = self.database.count_components_with_supplier_data().await?;
        let stale_components = self.find_stale_components().await?.len();

        Ok(SupplierStats {
            total_components,
            components_with_supplier_data,
            components_without_supplier_data: total_components - components_with_supplier_data,
            stale_components,
            cache_coverage_percent: if total_components > 0 {
                (components_with_supplier_data as f64 / total_components as f64) * 100.0
            } else {
                0.0
            },
        })
    }
}

/// Result of supplier data update operations
#[derive(Debug, Clone)]
pub struct SupplierUpdateResult {
    pub successful_updates: u32,
    pub failed_updates: u32,
    pub errors: Vec<String>,
}

impl SupplierUpdateResult {
    pub fn new() -> Self {
        Self {
            successful_updates: 0,
            failed_updates: 0,
            errors: Vec::new(),
        }
    }

    pub fn total_processed(&self) -> u32 {
        self.successful_updates + self.failed_updates
    }

    pub fn success_rate(&self) -> f64 {
        if self.total_processed() > 0 {
            self.successful_updates as f64 / self.total_processed() as f64
        } else {
            0.0
        }
    }
}

/// Statistics about supplier data coverage
#[derive(Debug, Clone)]
pub struct SupplierStats {
    pub total_components: u32,
    pub components_with_supplier_data: u32,
    pub components_without_supplier_data: u32,
    pub stale_components: usize,
    pub cache_coverage_percent: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_supplier_service_creation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        let config = TrustedPartsConfig::default();
        let service = SupplierService::new(&db_path, config).await;
        
        assert!(service.is_ok());
    }

    #[test]
    fn test_supplier_update_result() {
        let mut result = SupplierUpdateResult::new();
        assert_eq!(result.total_processed(), 0);
        assert_eq!(result.success_rate(), 0.0);
        
        result.successful_updates = 8;
        result.failed_updates = 2;
        assert_eq!(result.total_processed(), 10);
        assert_eq!(result.success_rate(), 0.8);
    }
}