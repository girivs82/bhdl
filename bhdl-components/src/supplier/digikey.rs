//! DigiKey API client for component data
//! 
//! DigiKey provides free API access for developers with reasonable rate limits.
//! Requires registration but no business credentials needed.

use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use reqwest::{Client, header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE}};
use log::{debug, info, warn};
use chrono::{DateTime, Utc};

use crate::types::{SupplierInfo, PriceBreak};

/// Configuration for DigiKey API
#[derive(Debug, Clone)]
pub struct DigiKeyConfig {
    pub api_url: String,
    pub auth_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub timeout_seconds: u64,
    pub max_retries: u32,
}

impl Default for DigiKeyConfig {
    fn default() -> Self {
        Self {
            api_url: "https://api.digikey.com".to_string(),
            auth_url: "https://api.digikey.com/v1/oauth2/token".to_string(),
            client_id: std::env::var("DIGIKEY_CLIENT_ID").unwrap_or_default(),
            client_secret: std::env::var("DIGIKEY_CLIENT_SECRET").unwrap_or_default(),
            timeout_seconds: 30,
            max_retries: 3,
        }
    }
}

/// DigiKey API client
pub struct DigiKeyClient {
    client: Client,
    config: DigiKeyConfig,
    access_token: Option<String>,
    token_expires_at: Option<DateTime<Utc>>,
}

/// DigiKey authentication response
#[derive(Debug, Deserialize)]
struct DigiKeyAuthResponse {
    access_token: String,
    expires_in: i64,
    token_type: String,
}

/// DigiKey product search request
#[derive(Debug, Serialize)]
struct DigiKeySearchRequest {
    #[serde(rename = "Keywords")]
    keywords: String,
    #[serde(rename = "RecordCount")]
    record_count: i32,
    #[serde(rename = "RecordStartPosition")]
    record_start_position: i32,
    #[serde(rename = "Sort")]
    sort: DigiKeySort,
}

#[derive(Debug, Serialize)]
struct DigiKeySort {
    #[serde(rename = "Option")]
    option: String,
    #[serde(rename = "Direction")]
    direction: String,
}

/// DigiKey search response
#[derive(Debug, Deserialize)]
struct DigiKeySearchResponse {
    #[serde(rename = "ProductsCount")]
    products_count: i32,
    #[serde(rename = "Products")]
    products: Vec<DigiKeyProduct>,
}

/// DigiKey product information
#[derive(Debug, Deserialize)]
struct DigiKeyProduct {
    #[serde(rename = "DigiKeyPartNumber")]
    digi_key_part_number: String,
    #[serde(rename = "ManufacturerPartNumber")]
    manufacturer_part_number: String,
    #[serde(rename = "Manufacturer")]
    manufacturer: DigiKeyManufacturer,
    #[serde(rename = "ProductDescription")]
    product_description: String,
    #[serde(rename = "UnitPrice")]
    unit_price: Option<f64>,
    #[serde(rename = "StandardPricing")]
    standard_pricing: Vec<DigiKeyPricing>,
    #[serde(rename = "QuantityAvailable")]
    quantity_available: i32,
    #[serde(rename = "MinimumOrderQuantity")]
    minimum_order_quantity: i32,
    #[serde(rename = "NonStock")]
    non_stock: bool,
    #[serde(rename = "ProductUrl")]
    product_url: String,
    #[serde(rename = "PrimaryDatasheet")]
    primary_datasheet: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DigiKeyManufacturer {
    #[serde(rename = "Name")]
    name: String,
}

#[derive(Debug, Deserialize)]
struct DigiKeyPricing {
    #[serde(rename = "BreakQuantity")]
    break_quantity: i32,
    #[serde(rename = "UnitPrice")]
    unit_price: f64,
}

impl DigiKeyClient {
    /// Create a new DigiKey client
    pub fn new(config: DigiKeyConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_seconds))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            config,
            access_token: None,
            token_expires_at: None,
        })
    }

    /// Authenticate and get access token
    async fn authenticate(&mut self) -> Result<()> {
        debug!("Authenticating with DigiKey API");

        let grant_type = "client_credentials".to_string();
        let params = [
            ("client_id", &self.config.client_id),
            ("client_secret", &self.config.client_secret),
            ("grant_type", &grant_type),
        ];

        let response = self.client
            .post(&self.config.auth_url)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .form(&params)
            .send()
            .await
            .context("Failed to send authentication request")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("DigiKey authentication failed: {}", error_text));
        }

        let auth_response: DigiKeyAuthResponse = response
            .json()
            .await
            .context("Failed to parse DigiKey authentication response")?;

        self.access_token = Some(auth_response.access_token);
        self.token_expires_at = Some(Utc::now() + chrono::Duration::seconds(auth_response.expires_in - 60));

        info!("Successfully authenticated with DigiKey API");
        Ok(())
    }

    /// Ensure we have a valid access token
    async fn ensure_authenticated(&mut self) -> Result<()> {
        if let Some(expires_at) = self.token_expires_at {
            if Utc::now() >= expires_at {
                self.access_token = None;
                self.token_expires_at = None;
            }
        }

        if self.access_token.is_none() {
            self.authenticate().await?;
        }

        Ok(())
    }

    /// Search for components by part number
    pub async fn search_components(&mut self, part_numbers: &[String]) -> Result<Vec<SupplierInfo>> {
        self.ensure_authenticated().await?;

        if part_numbers.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_supplier_infos = Vec::new();

        // Search each part number individually (DigiKey doesn't support batch search)
        for part_number in part_numbers {
            match self.search_single_component(part_number).await {
                Ok(mut infos) => all_supplier_infos.append(&mut infos),
                Err(e) => {
                    warn!("Failed to search for component '{}': {}", part_number, e);
                    continue;
                }
            }
        }

        info!("Found {} supplier offers via DigiKey API", all_supplier_infos.len());
        Ok(all_supplier_infos)
    }

    /// Search for a single component
    async fn search_single_component(&mut self, part_number: &str) -> Result<Vec<SupplierInfo>> {
        let search_request = DigiKeySearchRequest {
            keywords: part_number.to_string(),
            record_count: 10, // Limit results to avoid quota exhaustion
            record_start_position: 0,
            sort: DigiKeySort {
                option: "SortByUnitPrice".to_string(),
                direction: "Ascending".to_string(),
            },
        };

        debug!("Searching DigiKey for component: {}", part_number);

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        
        if let Some(token) = &self.access_token {
            headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", token))?);
        }

        let search_url = format!("{}/Search/v3/Products/Keyword", self.config.api_url);
        
        let response = self.client
            .post(&search_url)
            .headers(headers)
            .json(&search_request)
            .send()
            .await
            .context("Failed to send DigiKey search request")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("DigiKey search failed: {}", error_text));
        }

        let search_response: DigiKeySearchResponse = response
            .json()
            .await
            .context("Failed to parse DigiKey search response")?;

        let mut supplier_infos = Vec::new();

        for product in search_response.products {
            // Convert DigiKey pricing to our format
            let price_breaks: Vec<PriceBreak> = product.standard_pricing
                .into_iter()
                .map(|pricing| PriceBreak {
                    quantity: pricing.break_quantity,
                    unit_price: pricing.unit_price,
                    currency: "USD".to_string(), // DigiKey prices are in USD
                })
                .collect();

            let supplier_info = SupplierInfo {
                supplier_name: "DigiKey".to_string(),
                supplier_part_number: product.digi_key_part_number,
                manufacturer_part_number: product.manufacturer_part_number,
                manufacturer: product.manufacturer.name,
                availability: product.quantity_available,
                lead_time_days: if product.non_stock { Some(14) } else { Some(1) }, // Estimate
                moq: product.minimum_order_quantity,
                price_breaks,
                datasheet_url: product.primary_datasheet,
                last_updated: Utc::now(),
            };

            supplier_infos.push(supplier_info);
        }

        Ok(supplier_infos)
    }

    /// Get detailed component information
    pub async fn get_component_details(&mut self, manufacturer_part_number: &str) -> Result<Option<SupplierInfo>> {
        let results = self.search_components(&[manufacturer_part_number.to_string()]).await?;
        
        // Return the best match (first result, sorted by price)
        Ok(results.into_iter().next())
    }

    /// Check API health and quota
    pub async fn check_health(&mut self) -> Result<super::nexar::ApiHealthInfo> {
        self.ensure_authenticated().await?;

        // Simple search to check API health
        let start_time = std::time::Instant::now();
        
        let health_check_result = self.search_single_component("LM358").await;
        
        let response_time = start_time.elapsed();
        let is_healthy = health_check_result.is_ok();

        Ok(super::nexar::ApiHealthInfo {
            is_healthy,
            response_time_ms: response_time.as_millis() as u64,
            quota_remaining: None, // DigiKey doesn't expose quota in response headers
            quota_limit: None, // Varies by plan
            rate_limit_remaining: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_digikey_config_from_env() {
        std::env::set_var("DIGIKEY_CLIENT_ID", "test_client");
        std::env::set_var("DIGIKEY_CLIENT_SECRET", "test_secret");
        
        let config = DigiKeyConfig::default();
        assert_eq!(config.client_id, "test_client");
        assert_eq!(config.client_secret, "test_secret");
    }

    #[tokio::test]
    async fn test_digikey_client_creation() {
        let config = DigiKeyConfig {
            client_id: "test".to_string(),
            client_secret: "test".to_string(),
            ..Default::default()
        };
        
        let client = DigiKeyClient::new(config);
        assert!(client.is_ok());
    }
}