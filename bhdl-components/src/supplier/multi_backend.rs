//! Multi-backend supplier service that works with multiple APIs
//! 
//! This service provides a unified interface to multiple component databases
//! that are accessible to individual developers, with automatic fallback
//! and intelligent backend selection.

use anyhow::{Result, Context};
use log::{debug, info, warn, error};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::{self, Display};
use tokio::time::{timeout, Duration};

use crate::types::{SupplierInfo, SupplierData, ComponentId};
use super::{
    nexar::{NexarClient, NexarConfig},
    digikey::{DigiKeyClient, DigiKeyConfig},
    cache::{SupplierDataCache, CacheStats},
};

/// Configuration for multi-backend supplier service
#[derive(Debug, Clone)]
pub struct MultiBackendConfig {
    pub nexar: Option<NexarConfig>,
    pub digikey: Option<DigiKeyConfig>,
    pub preferred_backends: Vec<SupplierBackend>,
    pub max_concurrent_requests: usize,
    pub request_timeout_seconds: u64,
    pub fallback_enabled: bool,
}

impl Default for MultiBackendConfig {
    fn default() -> Self {
        Self {
            nexar: Some(NexarConfig::default()),
            digikey: Some(DigiKeyConfig::default()),
            preferred_backends: vec![
                SupplierBackend::Nexar,  // Free tier: 1000 calls/month
                SupplierBackend::DigiKey, // Free with registration
            ],
            max_concurrent_requests: 3,
            request_timeout_seconds: 30,
            fallback_enabled: true,
        }
    }
}

/// Available supplier backends
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SupplierBackend {
    Nexar,
    DigiKey,
    // Future: Mouser, Arrow, etc.
}

impl std::fmt::Display for SupplierBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SupplierBackend::Nexar => write!(f, "Nexar"),
            SupplierBackend::DigiKey => write!(f, "DigiKey"),
        }
    }
}

/// Multi-backend supplier service
pub struct MultiBackendSupplierService {
    nexar_client: Option<NexarClient>,
    digikey_client: Option<DigiKeyClient>,
    config: MultiBackendConfig,
    backend_health: HashMap<SupplierBackend, BackendHealth>,
    cache: SupplierDataCache,
}

/// Backend health status
#[derive(Debug, Clone)]
struct BackendHealth {
    is_available: bool,
    last_success: Option<chrono::DateTime<chrono::Utc>>,
    consecutive_failures: u32,
    avg_response_time_ms: u64,
}

impl Default for BackendHealth {
    fn default() -> Self {
        Self {
            is_available: true,
            last_success: None,
            consecutive_failures: 0,
            avg_response_time_ms: 1000,
        }
    }
}

/// Search result from a backend
#[derive(Debug, Clone)]
struct BackendSearchResult {
    backend: SupplierBackend,
    suppliers: Vec<SupplierInfo>,
    response_time_ms: u64,
    error: Option<String>,
}

impl MultiBackendSupplierService {
    /// Create a new multi-backend supplier service
    pub async fn new(config: MultiBackendConfig, cache_db_path: String) -> Result<Self> {
        let nexar_client = if let Some(nexar_config) = &config.nexar {
            match NexarClient::new(nexar_config.clone()) {
                Ok(client) => {
                    info!("Nexar client initialized");
                    Some(client)
                }
                Err(e) => {
                    warn!("Failed to initialize Nexar client: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let digikey_client = if let Some(digikey_config) = &config.digikey {
            match DigiKeyClient::new(digikey_config.clone()) {
                Ok(client) => {
                    info!("DigiKey client initialized");
                    Some(client)
                }
                Err(e) => {
                    warn!("Failed to initialize DigiKey client: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let mut backend_health = HashMap::new();
        if nexar_client.is_some() {
            backend_health.insert(SupplierBackend::Nexar, BackendHealth::default());
        }
        if digikey_client.is_some() {
            backend_health.insert(SupplierBackend::DigiKey, BackendHealth::default());
        }

        // Initialize cache
        let cache = SupplierDataCache::new(cache_db_path, 1000)?;

        Ok(Self {
            nexar_client,
            digikey_client,
            config,
            backend_health,
            cache,
        })
    }

    /// Search for component supplier data across all backends
    pub async fn search_component_suppliers(&mut self, part_numbers: &[String]) -> Result<SupplierData> {
        if part_numbers.is_empty() {
            return Ok(SupplierData {
                component_id: 0, // Will be set by caller
                suppliers: Vec::new(),
                last_updated: chrono::Utc::now(),
            });
        }

        // Check cache first for each part number
        let mut all_suppliers = Vec::new();
        let mut uncached_parts = Vec::new();

        for part_number in part_numbers {
            let cache_key = format!("multi:{}", part_number);
            match self.cache.get("multi", &cache_key).await? {
                Some(cached_suppliers) => {
                    info!("Cache hit for part {}", part_number);
                    all_suppliers.extend(cached_suppliers);
                }
                None => {
                    uncached_parts.push(part_number.clone());
                }
            }
        }

        // If we found everything in cache, return early
        if uncached_parts.is_empty() {
            info!("All {} parts found in cache", part_numbers.len());
            return Ok(SupplierData {
                component_id: 0,
                suppliers: all_suppliers,
                last_updated: chrono::Utc::now(),
            });
        }

        info!("Searching for {} uncached parts across {} backends", 
              uncached_parts.len(), self.available_backends().len());

        // Get ordered list of backends to try
        let backends_to_try = self.get_ordered_backends();
        let mut successful_backends = Vec::new();

        // Search uncached parts with rate limiting
        for backend in backends_to_try {
            // Check rate limits before making requests
            if !self.cache.check_rate_limit(&backend.to_string()).await? {
                warn!("Rate limit exceeded for backend {}, skipping", backend);
                continue;
            }

            match self.search_with_backend(backend, &uncached_parts).await {
                Ok(result) => {
                    if !result.suppliers.is_empty() {
                        let supplier_count = result.suppliers.len();
                        
                        // Cache the results per part number 
                        for part_number in &uncached_parts {
                            let part_suppliers: Vec<_> = result.suppliers.iter()
                                .filter(|s| s.manufacturer_part_number.contains(part_number))
                                .cloned()
                                .collect();
                            
                            if !part_suppliers.is_empty() {
                                let cache_key = format!("multi:{}", part_number);
                                self.cache.put(&backend.to_string(), &cache_key, part_suppliers).await?;
                            }
                        }
                        
                        all_suppliers.extend(result.suppliers);
                        successful_backends.push(backend);
                        self.update_backend_health(backend, true, result.response_time_ms);
                        
                        info!("Backend {} returned {} suppliers", 
                              backend, supplier_count);
                    } else {
                        debug!("Backend {} returned no results", backend);
                    }
                }
                Err(e) => {
                    warn!("Backend {} failed: {}", backend, e);
                    self.update_backend_health(backend, false, 0);
                    
                    if !self.config.fallback_enabled {
                        break;
                    }
                }
            }
            
            // If we have results and fallback is disabled, stop here
            if !all_suppliers.is_empty() && !self.config.fallback_enabled {
                break;
            }
        }

        // Deduplicate suppliers by manufacturer part number + supplier name
        all_suppliers = self.deduplicate_suppliers(all_suppliers);

        info!("Total {} unique suppliers found from {} backends", 
              all_suppliers.len(), successful_backends.len());

        Ok(SupplierData {
            component_id: 0, // Will be set by caller
            suppliers: all_suppliers,
            last_updated: chrono::Utc::now(),
        })
    }

    /// Search with a specific backend
    async fn search_with_backend(
        &mut self, 
        backend: SupplierBackend, 
        part_numbers: &[String]
    ) -> Result<BackendSearchResult> {
        let start_time = std::time::Instant::now();
        
        let result = timeout(
            Duration::from_secs(self.config.request_timeout_seconds),
            self.execute_backend_search(backend, part_numbers)
        ).await;

        let response_time_ms = start_time.elapsed().as_millis() as u64;

        match result {
            Ok(Ok(suppliers)) => Ok(BackendSearchResult {
                backend,
                suppliers,
                response_time_ms,
                error: None,
            }),
            Ok(Err(e)) => Err(anyhow::anyhow!("Backend {} error: {}", backend, e)),
            Err(_) => Err(anyhow::anyhow!("Backend {} timed out", backend)),
        }
    }

    /// Execute search on specific backend
    async fn execute_backend_search(
        &mut self, 
        backend: SupplierBackend, 
        part_numbers: &[String]
    ) -> Result<Vec<SupplierInfo>> {
        match backend {
            SupplierBackend::Nexar => {
                if let Some(client) = &mut self.nexar_client {
                    client.search_components(part_numbers).await
                } else {
                    Err(anyhow::anyhow!("Nexar client not available"))
                }
            }
            SupplierBackend::DigiKey => {
                if let Some(client) = &mut self.digikey_client {
                    client.search_components(part_numbers).await
                } else {
                    Err(anyhow::anyhow!("DigiKey client not available"))
                }
            }
        }
    }

    /// Get ordered list of backends to try
    fn get_ordered_backends(&self) -> Vec<SupplierBackend> {
        let mut backends = Vec::new();
        
        // Start with preferred backends that are available and healthy
        for &backend in &self.config.preferred_backends {
            if self.is_backend_healthy(backend) {
                backends.push(backend);
            }
        }
        
        // Add any remaining available backends
        for &backend in &[SupplierBackend::Nexar, SupplierBackend::DigiKey] {
            if !backends.contains(&backend) && self.is_backend_available(backend) {
                backends.push(backend);
            }
        }
        
        backends
    }

    /// Check if backend is available
    fn is_backend_available(&self, backend: SupplierBackend) -> bool {
        match backend {
            SupplierBackend::Nexar => self.nexar_client.is_some(),
            SupplierBackend::DigiKey => self.digikey_client.is_some(),
        }
    }

    /// Check if backend is healthy
    fn is_backend_healthy(&self, backend: SupplierBackend) -> bool {
        if !self.is_backend_available(backend) {
            return false;
        }
        
        if let Some(health) = self.backend_health.get(&backend) {
            health.is_available && health.consecutive_failures < 3
        } else {
            true
        }
    }

    /// Update backend health status
    fn update_backend_health(&mut self, backend: SupplierBackend, success: bool, response_time_ms: u64) {
        let health = self.backend_health.entry(backend).or_insert_with(BackendHealth::default);
        
        if success {
            health.is_available = true;
            health.last_success = Some(chrono::Utc::now());
            health.consecutive_failures = 0;
            
            // Update average response time (simple moving average)
            health.avg_response_time_ms = (health.avg_response_time_ms + response_time_ms) / 2;
        } else {
            health.consecutive_failures += 1;
            
            // Mark as unavailable after 3 consecutive failures
            if health.consecutive_failures >= 3 {
                health.is_available = false;
                warn!("Backend {} marked as unavailable after {} failures", backend, health.consecutive_failures);
            }
        }
    }

    /// Deduplicate suppliers by manufacturer part number and supplier name
    fn deduplicate_suppliers(&self, suppliers: Vec<SupplierInfo>) -> Vec<SupplierInfo> {
        let mut unique_suppliers = HashMap::new();
        
        for supplier in suppliers {
            let key = format!("{}:{}", supplier.manufacturer_part_number, supplier.supplier_name);
            
            // Keep the supplier with the most recent data or better price
            if let Some(existing) = unique_suppliers.get(&key) {
                let existing_supplier: &SupplierInfo = existing;
                if supplier.last_updated > existing_supplier.last_updated {
                    unique_suppliers.insert(key, supplier);
                }
            } else {
                unique_suppliers.insert(key, supplier);
            }
        }
        
        unique_suppliers.into_values().collect()
    }

    /// Get cache statistics
    pub async fn get_cache_stats(&self) -> Result<CacheStats> {
        self.cache.get_stats().await
    }

    /// Clear expired cache entries
    pub async fn cleanup_cache(&self) -> Result<usize> {
        self.cache.cleanup_expired().await
    }

    /// Check if more requests are allowed for a backend
    pub async fn can_make_request(&self, backend: SupplierBackend) -> Result<bool> {
        self.cache.check_rate_limit(&backend.to_string()).await
    }

    /// Get time until next request is allowed for a backend
    pub async fn time_until_next_request(&self, backend: SupplierBackend) -> Option<std::time::Duration> {
        self.cache.time_until_next_request(&backend.to_string()).await
    }

    /// Get available backends
    pub fn available_backends(&self) -> Vec<SupplierBackend> {
        let mut backends = Vec::new();
        
        if self.nexar_client.is_some() {
            backends.push(SupplierBackend::Nexar);
        }
        if self.digikey_client.is_some() {
            backends.push(SupplierBackend::DigiKey);
        }
        
        backends
    }

    /// Get backend health status
    pub fn get_backend_health(&self) -> &HashMap<SupplierBackend, BackendHealth> {
        &self.backend_health
    }

    /// Check health of all backends
    pub async fn check_all_backends_health(&mut self) -> Result<HashMap<SupplierBackend, super::nexar::ApiHealthInfo>> {
        let mut health_info = HashMap::new();
        
        if let Some(client) = &mut self.nexar_client {
            match client.check_health().await {
                Ok(info) => {
                    health_info.insert(SupplierBackend::Nexar, info);
                }
                Err(e) => {
                    error!("Nexar health check failed: {}", e);
                }
            }
        }
        
        if let Some(client) = &mut self.digikey_client {
            match client.check_health().await {
                Ok(info) => {
                    health_info.insert(SupplierBackend::DigiKey, info);
                }
                Err(e) => {
                    error!("DigiKey health check failed: {}", e);
                }
            }
        }
        
        Ok(health_info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_multi_backend_service_creation() {
        let config = MultiBackendConfig {
            nexar: None, // Disable to avoid requiring credentials
            digikey: None,
            ..Default::default()
        };
        
        let service = MultiBackendSupplierService::new(config, ":memory:".to_string()).await;
        assert!(service.is_ok());
    }

    #[test]
    fn test_backend_display() {
        assert_eq!(SupplierBackend::Nexar.to_string(), "Nexar");
        assert_eq!(SupplierBackend::DigiKey.to_string(), "DigiKey");
    }

    #[test]
    fn test_supplier_deduplication() {
        // This would require creating a service instance and test suppliers
        // Left as integration test for now
    }
}