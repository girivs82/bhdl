//! Intelligent caching system for supplier data with rate limiting
//! 
//! This module provides multi-level caching to minimize API calls:
//! 1. In-memory LRU cache for hot data (minutes)
//! 2. SQLite persistent cache for warm data (hours/days)  
//! 3. Intelligent refresh scheduling based on volatility

use anyhow::Result;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use lru::LruCache;
use chrono::{DateTime, Utc, Duration as ChronoDuration};
use rusqlite::{Connection, params};

use crate::types::{SupplierInfo, ComponentId};

/// Cache entry with metadata
#[derive(Debug, Clone)]
pub struct CachedSupplierData {
    pub data: Vec<SupplierInfo>,
    pub cached_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub access_count: u32,
    pub volatility_score: f64, // 0.0 = stable, 1.0 = highly volatile
}

/// Rate limiting configuration per supplier
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub requests_per_minute: u32,
    pub requests_per_hour: u32,
    pub requests_per_day: u32,
    pub burst_allowance: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 10,
            requests_per_hour: 100,
            requests_per_day: 1000,
            burst_allowance: 5,
        }
    }
}

/// Rate limiting tracker
#[derive(Debug)]
struct RateLimiter {
    requests_minute: Vec<Instant>,
    requests_hour: Vec<Instant>,
    requests_day: Vec<Instant>,
    config: RateLimitConfig,
}

impl RateLimiter {
    fn new(config: RateLimitConfig) -> Self {
        Self {
            requests_minute: Vec::new(),
            requests_hour: Vec::new(),
            requests_day: Vec::new(),
            config,
        }
    }

    /// Check if request is allowed and update counters
    fn can_make_request(&mut self) -> bool {
        let now = Instant::now();
        
        // Clean old requests
        self.clean_old_requests(now);
        
        // Check limits
        if self.requests_minute.len() >= self.config.requests_per_minute as usize {
            return false;
        }
        if self.requests_hour.len() >= self.config.requests_per_hour as usize {
            return false;
        }
        if self.requests_day.len() >= self.config.requests_per_day as usize {
            return false;
        }
        
        // Record request
        self.requests_minute.push(now);
        self.requests_hour.push(now);
        self.requests_day.push(now);
        
        true
    }
    
    fn clean_old_requests(&mut self, now: Instant) {
        let minute_ago = now - Duration::from_secs(60);
        let hour_ago = now - Duration::from_secs(3600);
        let day_ago = now - Duration::from_secs(86400);
        
        self.requests_minute.retain(|&t| t > minute_ago);
        self.requests_hour.retain(|&t| t > hour_ago);
        self.requests_day.retain(|&t| t > day_ago);
    }
    
    /// Get time until next request is allowed
    fn time_until_next_request(&mut self) -> Option<Duration> {
        let now = Instant::now();
        self.clean_old_requests(now);
        
        if self.requests_minute.len() >= self.config.requests_per_minute as usize {
            if let Some(oldest) = self.requests_minute.first() {
                return Some(Duration::from_secs(60) - (now - *oldest));
            }
        }
        
        None
    }
}

/// Multi-level caching system with intelligent refresh
pub struct SupplierDataCache {
    // In-memory LRU cache (fastest, volatile)
    memory_cache: Arc<RwLock<LruCache<String, CachedSupplierData>>>,
    
    // Rate limiters per supplier
    rate_limiters: Arc<Mutex<HashMap<String, RateLimiter>>>,
    
    // Database connection for persistent cache
    db_path: String,
    
    // Cache configuration
    memory_cache_size: usize,
    default_cache_duration: ChronoDuration,
    max_cache_duration: ChronoDuration,
}

impl SupplierDataCache {
    /// Create new cache with specified configuration
    pub fn new(db_path: String, memory_cache_size: usize) -> Result<Self> {
        let cache = Self {
            memory_cache: Arc::new(RwLock::new(LruCache::new(
                std::num::NonZeroUsize::new(memory_cache_size).unwrap()
            ))),
            rate_limiters: Arc::new(Mutex::new(HashMap::new())),
            db_path,
            memory_cache_size,
            default_cache_duration: ChronoDuration::hours(4),
            max_cache_duration: ChronoDuration::days(1),
        };
        
        cache.init_persistent_cache()?;
        Ok(cache)
    }
    
    /// Initialize persistent cache tables
    fn init_persistent_cache(&self) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;
        
        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS supplier_cache (
                cache_key TEXT PRIMARY KEY,
                supplier_name TEXT NOT NULL,
                component_query TEXT NOT NULL,
                data_json TEXT NOT NULL,
                cached_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                access_count INTEGER NOT NULL DEFAULT 0,
                volatility_score REAL NOT NULL DEFAULT 0.5,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            )"#,
            [],
        )?;
        
        // Index for cleanup queries
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_supplier_cache_expires ON supplier_cache(expires_at)",
            [],
        )?;
        
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_supplier_cache_supplier ON supplier_cache(supplier_name)",
            [],
        )?;
        
        Ok(())
    }
    
    /// Generate cache key for supplier query
    fn cache_key(supplier: &str, query: &str) -> String {
        format!("{}:{}", supplier, query)
    }
    
    /// Check if request is allowed by rate limiter
    pub async fn check_rate_limit(&self, supplier: &str) -> Result<bool> {
        let mut limiters = self.rate_limiters.lock().unwrap();
        let limiter = limiters.entry(supplier.to_string())
            .or_insert_with(|| RateLimiter::new(self.get_rate_limit_config(supplier)));
        
        Ok(limiter.can_make_request())
    }
    
    /// Get time until next request is allowed
    pub async fn time_until_next_request(&self, supplier: &str) -> Option<Duration> {
        let mut limiters = self.rate_limiters.lock().unwrap();
        if let Some(limiter) = limiters.get_mut(supplier) {
            limiter.time_until_next_request()
        } else {
            None
        }
    }
    
    /// Get rate limit configuration for supplier
    fn get_rate_limit_config(&self, supplier: &str) -> RateLimitConfig {
        match supplier.to_lowercase().as_str() {
            "digikey" => RateLimitConfig {
                requests_per_minute: 8,  // Conservative limit
                requests_per_hour: 90,   // Leave buffer
                requests_per_day: 900,   // Leave buffer for 1000/day limit
                burst_allowance: 3,
            },
            "nexar" => RateLimitConfig {
                requests_per_minute: 5,
                requests_per_hour: 50,
                requests_per_day: 950,   // Buffer for 1000/month limit
                burst_allowance: 2,
            },
            _ => RateLimitConfig::default(),
        }
    }
    
    /// Get cached data if available and not expired
    pub async fn get(&self, supplier: &str, query: &str) -> Result<Option<Vec<SupplierInfo>>> {
        let cache_key = Self::cache_key(supplier, query);
        
        // 1. Check memory cache first
        {
            let mut cache = self.memory_cache.write().await;
            if let Some(cached) = cache.get(&cache_key) {
                if cached.expires_at > Utc::now() {
                    // Update access count
                    let mut updated = cached.clone();
                    updated.access_count += 1;
                    cache.put(cache_key.clone(), updated.clone());
                    
                    return Ok(Some(updated.data));
                } else {
                    // Remove expired entry
                    cache.pop(&cache_key);
                }
            }
        }
        
        // 2. Check persistent cache
        self.get_from_persistent_cache(&cache_key, supplier, query).await
    }
    
    /// Get data from persistent SQLite cache
    async fn get_from_persistent_cache(
        &self,
        cache_key: &str,
        supplier: &str,
        query: &str,
    ) -> Result<Option<Vec<SupplierInfo>>> {
        let conn = Connection::open(&self.db_path)?;
        
        let mut stmt = conn.prepare(
            "SELECT data_json, cached_at, expires_at, access_count, volatility_score 
             FROM supplier_cache 
             WHERE cache_key = ? AND expires_at > ?"
        )?;
        
        let now_timestamp = Utc::now().timestamp();
        let mut rows = stmt.query_map(params![cache_key, now_timestamp], |row| {
            let data_json: String = row.get(0)?;
            let cached_at: i64 = row.get(1)?;
            let expires_at: i64 = row.get(2)?;
            let access_count: u32 = row.get(3)?;
            let volatility_score: f64 = row.get(4)?;
            
            Ok((data_json, cached_at, expires_at, access_count, volatility_score))
        })?;
        
        if let Some(row) = rows.next() {
            let (data_json, cached_at, expires_at, access_count, volatility_score) = row?;
            
            // Deserialize data
            let data: Vec<SupplierInfo> = serde_json::from_str(&data_json)?;
            
            // Create cached entry
            let cached = CachedSupplierData {
                data: data.clone(),
                cached_at: DateTime::from_timestamp(cached_at, 0).unwrap_or_else(Utc::now),
                expires_at: DateTime::from_timestamp(expires_at, 0).unwrap_or_else(Utc::now),
                access_count: access_count + 1,
                volatility_score,
            };
            
            // Update access count in database
            conn.execute(
                "UPDATE supplier_cache SET access_count = access_count + 1 WHERE cache_key = ?",
                params![cache_key],
            )?;
            
            // Store in memory cache for faster future access
            {
                let mut cache = self.memory_cache.write().await;
                cache.put(cache_key.to_string(), cached);
            }
            
            return Ok(Some(data));
        }
        
        Ok(None)
    }
    
    /// Store data in cache with intelligent expiration
    pub async fn put(
        &self,
        supplier: &str,
        query: &str,
        data: Vec<SupplierInfo>,
    ) -> Result<()> {
        let cache_key = Self::cache_key(supplier, query);
        let now = Utc::now();
        
        // Calculate volatility score based on data characteristics
        let volatility_score = self.calculate_volatility_score(&data);
        
        // Calculate cache duration based on volatility
        let cache_duration = self.calculate_cache_duration(volatility_score);
        let expires_at = now + cache_duration;
        
        let cached = CachedSupplierData {
            data: data.clone(),
            cached_at: now,
            expires_at,
            access_count: 1,
            volatility_score,
        };
        
        // Store in memory cache
        {
            let mut cache = self.memory_cache.write().await;
            cache.put(cache_key.clone(), cached.clone());
        }
        
        // Store in persistent cache
        self.store_in_persistent_cache(&cache_key, supplier, query, &cached).await?;
        
        Ok(())
    }
    
    /// Calculate volatility score based on data characteristics
    fn calculate_volatility_score(&self, data: &[SupplierInfo]) -> f64 {
        if data.is_empty() {
            return 0.5; // Default
        }
        
        let mut volatility = 0.0;
        let mut factors = 0;
        
        for supplier_info in data {
            // High stock = more stable
            if supplier_info.availability > 10000 {
                volatility += 0.1;
            } else if supplier_info.availability > 1000 {
                volatility += 0.3;
            } else {
                volatility += 0.8;
            }
            factors += 1;
            
            // Multiple price breaks = more stable
            if supplier_info.price_breaks.len() > 3 {
                volatility += 0.1;
            } else {
                volatility += 0.3;
            }
            factors += 1;
            
            // Known suppliers = more stable
            match supplier_info.supplier_name.as_str() {
                "DigiKey" | "Mouser" | "Newark" => volatility += 0.1,
                _ => volatility += 0.4,
            }
            factors += 1;
        }
        
        if factors > 0 {
            volatility / factors as f64
        } else {
            0.5
        }
    }
    
    /// Calculate cache duration based on volatility
    fn calculate_cache_duration(&self, volatility_score: f64) -> ChronoDuration {
        // High volatility = shorter cache duration
        let base_hours = if volatility_score > 0.7 {
            1 // Very volatile: 1 hour
        } else if volatility_score > 0.5 {
            4 // Moderate: 4 hours  
        } else if volatility_score > 0.3 {
            12 // Stable: 12 hours
        } else {
            24 // Very stable: 24 hours
        };
        
        ChronoDuration::hours(base_hours).min(self.max_cache_duration)
    }
    
    /// Store data in persistent cache
    async fn store_in_persistent_cache(
        &self,
        cache_key: &str,
        supplier: &str,
        query: &str,
        cached: &CachedSupplierData,
    ) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;
        let data_json = serde_json::to_string(&cached.data)?;
        
        conn.execute(
            r#"INSERT OR REPLACE INTO supplier_cache 
               (cache_key, supplier_name, component_query, data_json, cached_at, expires_at, access_count, volatility_score)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
            params![
                cache_key,
                supplier,
                query,
                data_json,
                cached.cached_at.timestamp(),
                cached.expires_at.timestamp(),
                cached.access_count,
                cached.volatility_score,
            ],
        )?;
        
        Ok(())
    }
    
    /// Clean expired entries from cache
    pub async fn cleanup_expired(&self) -> Result<usize> {
        let conn = Connection::open(&self.db_path)?;
        let now_timestamp = Utc::now().timestamp();
        
        let deleted = conn.execute(
            "DELETE FROM supplier_cache WHERE expires_at < ?",
            params![now_timestamp],
        )?;
        
        Ok(deleted)
    }
    
    /// Get cache statistics
    pub async fn get_stats(&self) -> Result<CacheStats> {
        let conn = Connection::open(&self.db_path)?;
        
        let total_entries: i64 = conn.query_row(
            "SELECT COUNT(*) FROM supplier_cache",
            [],
            |row| row.get(0),
        ).unwrap_or(0);
        
        let expired_entries: i64 = conn.query_row(
            "SELECT COUNT(*) FROM supplier_cache WHERE expires_at < ?",
            params![Utc::now().timestamp()],
            |row| row.get(0),
        ).unwrap_or(0);
        
        let memory_entries = {
            let cache = self.memory_cache.read().await;
            cache.len()
        };
        
        Ok(CacheStats {
            total_persistent_entries: total_entries as usize,
            expired_entries: expired_entries as usize,
            memory_entries,
            memory_cache_capacity: self.memory_cache_size,
        })
    }
}

/// Cache performance statistics
#[derive(Debug)]
pub struct CacheStats {
    pub total_persistent_entries: usize,
    pub expired_entries: usize,
    pub memory_entries: usize,
    pub memory_cache_capacity: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempfile;
    use std::io::Write;
    
    #[tokio::test]
    async fn test_cache_basic_operations() {
        let temp_path = "/tmp/test_supplier_cache.db";
        std::fs::remove_file(temp_path).ok();
        
        let cache = SupplierDataCache::new(temp_path.to_string(), 100).unwrap();
        
        // Test empty cache
        let result = cache.get("DigiKey", "LM358").await.unwrap();
        assert!(result.is_none());
        
        // Test storing and retrieving
        let test_data = vec![SupplierInfo {
            supplier_name: "DigiKey".to_string(),
            supplier_part_number: "TEST-123".to_string(),
            manufacturer_part_number: "LM358".to_string(),
            manufacturer: "TI".to_string(),
            availability: 1000,
            lead_time_days: Some(1),
            moq: 1,
            price_breaks: vec![],
            datasheet_url: None,
            last_updated: Utc::now(),
        }];
        
        cache.put("DigiKey", "LM358", test_data.clone()).await.unwrap();
        
        let retrieved = cache.get("DigiKey", "LM358").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().len(), 1);
        
        std::fs::remove_file(temp_path).ok();
    }
    
    #[tokio::test]
    async fn test_rate_limiting() {
        let temp_path = "/tmp/test_rate_limit.db";
        std::fs::remove_file(temp_path).ok();
        
        let cache = SupplierDataCache::new(temp_path.to_string(), 100).unwrap();
        
        // First request should be allowed
        assert!(cache.check_rate_limit("DigiKey").await.unwrap());
        
        // Multiple rapid requests should eventually be rate limited
        let mut allowed_count = 0;
        for _ in 0..20 {
            if cache.check_rate_limit("DigiKey").await.unwrap() {
                allowed_count += 1;
            }
        }
        
        // Should not allow all 20 requests
        assert!(allowed_count < 20);
        
        std::fs::remove_file(temp_path).ok();
    }
}