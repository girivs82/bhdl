//! Nexar API client for component data (successor to Octopart API)
//! 
//! Nexar provides a GraphQL API with a free tier of 1,000 calls/month
//! for individual developers, making it accessible for personal projects.

use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use reqwest::{Client, header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE}};
use log::{debug, info, warn};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

use crate::types::{SupplierInfo, PriceBreak};

/// Configuration for Nexar API
#[derive(Debug, Clone)]
pub struct NexarConfig {
    pub api_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub timeout_seconds: u64,
    pub max_retries: u32,
}

impl Default for NexarConfig {
    fn default() -> Self {
        Self {
            api_url: "https://api.nexar.com/graphql".to_string(),
            client_id: std::env::var("NEXAR_CLIENT_ID").unwrap_or_default(),
            client_secret: std::env::var("NEXAR_CLIENT_SECRET").unwrap_or_default(),
            timeout_seconds: 30,
            max_retries: 3,
        }
    }
}

/// Nexar API client
pub struct NexarClient {
    client: Client,
    config: NexarConfig,
    access_token: Option<String>,
    token_expires_at: Option<DateTime<Utc>>,
}

/// GraphQL query for component search
#[derive(Debug, Serialize)]
struct GraphQLQuery {
    query: String,
    variables: serde_json::Value,
}

/// Authentication response
#[derive(Debug, Deserialize)]
struct AuthResponse {
    access_token: String,
    expires_in: i64,
    token_type: String,
}

/// Nexar component search response
#[derive(Debug, Deserialize)]
struct NexarSearchResponse {
    data: Option<NexarSearchData>,
    errors: Option<Vec<GraphQLError>>,
}

#[derive(Debug, Deserialize)]
struct NexarSearchData {
    #[serde(rename = "supSearchMpn")]
    sup_search_mpn: NexarSearchResults,
}

#[derive(Debug, Deserialize)]
struct NexarSearchResults {
    results: Vec<NexarPart>,
}

#[derive(Debug, Deserialize)]
struct NexarPart {
    part: NexarPartDetails,
}

#[derive(Debug, Deserialize)]
struct NexarPartDetails {
    mpn: String,
    manufacturer: NexarManufacturer,
    #[serde(rename = "shortDescription")]
    short_description: Option<String>,
    #[serde(rename = "medianPrice1000")]
    median_price_1000: Option<NexarPrice>,
    sellers: Vec<NexarSeller>,
}

#[derive(Debug, Deserialize)]
struct NexarManufacturer {
    name: String,
}

#[derive(Debug, Deserialize)]
struct NexarPrice {
    price: f64,
    currency: String,
}

#[derive(Debug, Deserialize)]
struct NexarSeller {
    company: NexarCompany,
    offers: Vec<NexarOffer>,
}

#[derive(Debug, Deserialize)]
struct NexarCompany {
    name: String,
}

#[derive(Debug, Deserialize)]
struct NexarOffer {
    #[serde(rename = "inventoryLevel")]
    inventory_level: i32,
    #[serde(rename = "moq")]
    moq: i32,
    prices: Vec<NexarPriceBreak>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

#[derive(Debug, Deserialize)]
struct NexarPriceBreak {
    quantity: i32,
    price: f64,
    currency: String,
}

#[derive(Debug, Deserialize)]
struct GraphQLError {
    message: String,
    locations: Option<Vec<serde_json::Value>>,
    path: Option<Vec<serde_json::Value>>,
}

impl NexarClient {
    /// Create a new Nexar client
    pub fn new(config: NexarConfig) -> Result<Self> {
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
        debug!("Authenticating with Nexar API");

        let auth_url = "https://identity.nexar.com/connect/token";
        let params = [
            ("grant_type", "client_credentials"),
            ("client_id", &self.config.client_id),
            ("client_secret", &self.config.client_secret),
            ("scope", "supply.domain"),
        ];

        let response = self.client
            .post(auth_url)
            .form(&params)
            .send()
            .await
            .context("Failed to send authentication request")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Authentication failed: {}", error_text));
        }

        let auth_response: AuthResponse = response
            .json()
            .await
            .context("Failed to parse authentication response")?;

        self.access_token = Some(auth_response.access_token);
        self.token_expires_at = Some(Utc::now() + chrono::Duration::seconds(auth_response.expires_in - 60)); // 60s buffer

        info!("Successfully authenticated with Nexar API");
        Ok(())
    }

    /// Check if access token is valid and refresh if needed
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

    /// Search for components by manufacturer part number
    pub async fn search_components(&mut self, part_numbers: &[String]) -> Result<Vec<SupplierInfo>> {
        self.ensure_authenticated().await?;

        if part_numbers.is_empty() {
            return Ok(Vec::new());
        }

        // Nexar GraphQL query for component search
        let query = r#"
            query SearchComponents($queries: [SupSearchMpnQuery!]!) {
                supSearchMpn(queries: $queries) {
                    results {
                        part {
                            mpn
                            manufacturer {
                                name
                            }
                            shortDescription
                            medianPrice1000 {
                                price
                                currency
                            }
                            sellers(limit: 5) {
                                company {
                                    name
                                }
                                offers {
                                    inventoryLevel
                                    moq
                                    prices {
                                        quantity
                                        price
                                        currency
                                    }
                                    updatedAt
                                }
                            }
                        }
                    }
                }
            }
        "#;

        // Build queries for each part number
        let queries: Vec<serde_json::Value> = part_numbers
            .iter()
            .map(|mpn| serde_json::json!({"mpn": mpn, "limit": 3}))
            .collect();

        let variables = serde_json::json!({
            "queries": queries
        });

        let graphql_query = GraphQLQuery {
            query: query.to_string(),
            variables,
        };

        debug!("Searching for {} components via Nexar API", part_numbers.len());

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        
        if let Some(token) = &self.access_token {
            headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", token))?);
        }

        let response = self.client
            .post(&self.config.api_url)
            .headers(headers)
            .json(&graphql_query)
            .send()
            .await
            .context("Failed to send search request")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Search request failed: {}", error_text));
        }

        let search_response: NexarSearchResponse = response
            .json()
            .await
            .context("Failed to parse search response")?;

        if let Some(errors) = search_response.errors {
            warn!("GraphQL errors in response: {:?}", errors);
        }

        let mut supplier_infos = Vec::new();

        if let Some(data) = search_response.data {
            for part_result in data.sup_search_mpn.results {
                let part = part_result.part;
                
                // Convert each seller to a SupplierInfo
                for seller in part.sellers {
                    for offer in seller.offers {
                        let price_breaks: Vec<PriceBreak> = offer.prices
                            .into_iter()
                            .map(|price| PriceBreak {
                                quantity: price.quantity,
                                unit_price: price.price,
                                currency: price.currency,
                            })
                            .collect();

                        let supplier_info = SupplierInfo {
                            supplier_name: seller.company.name.clone(),
                            supplier_part_number: part.mpn.clone(),
                            manufacturer_part_number: part.mpn.clone(),
                            manufacturer: part.manufacturer.name.clone(),
                            availability: offer.inventory_level,
                            lead_time_days: None, // Nexar doesn't provide lead time in this query
                            moq: offer.moq,
                            price_breaks,
                            datasheet_url: None, // Would need separate query
                            last_updated: chrono::DateTime::parse_from_rfc3339(&offer.updated_at)
                                .map(|dt| dt.with_timezone(&Utc))
                                .unwrap_or_else(|_| Utc::now()),
                        };

                        supplier_infos.push(supplier_info);
                    }
                }
            }
        }

        info!("Found {} supplier offers via Nexar API", supplier_infos.len());
        Ok(supplier_infos)
    }

    /// Get detailed component information
    pub async fn get_component_details(&mut self, manufacturer_part_number: &str) -> Result<Option<SupplierInfo>> {
        let results = self.search_components(&[manufacturer_part_number.to_string()]).await?;
        Ok(results.into_iter().next())
    }

    /// Check API health and quota
    pub async fn check_health(&mut self) -> Result<ApiHealthInfo> {
        self.ensure_authenticated().await?;

        // Simple query to check API health
        let query = r#"
            query HealthCheck {
                supSearchMpn(queries: [{mpn: "LM358", limit: 1}]) {
                    results {
                        part {
                            mpn
                        }
                    }
                }
            }
        "#;

        let graphql_query = GraphQLQuery {
            query: query.to_string(),
            variables: serde_json::json!({}),
        };

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        
        if let Some(token) = &self.access_token {
            headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", token))?);
        }

        let start_time = std::time::Instant::now();
        
        let response = self.client
            .post(&self.config.api_url)
            .headers(headers)
            .json(&graphql_query)
            .send()
            .await
            .context("Failed to send health check request")?;

        let response_time = start_time.elapsed();
        let is_healthy = response.status().is_success();

        Ok(ApiHealthInfo {
            is_healthy,
            response_time_ms: response_time.as_millis() as u64,
            quota_remaining: None, // Nexar doesn't expose quota in response headers
            quota_limit: Some(1000), // Free tier limit
            rate_limit_remaining: None,
        })
    }
}

/// API health information
#[derive(Debug)]
pub struct ApiHealthInfo {
    pub is_healthy: bool,
    pub response_time_ms: u64,
    pub quota_remaining: Option<u32>,
    pub quota_limit: Option<u32>,
    pub rate_limit_remaining: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nexar_config_from_env() {
        std::env::set_var("NEXAR_CLIENT_ID", "test_client");
        std::env::set_var("NEXAR_CLIENT_SECRET", "test_secret");
        
        let config = NexarConfig::default();
        assert_eq!(config.client_id, "test_client");
        assert_eq!(config.client_secret, "test_secret");
    }

    #[tokio::test]
    async fn test_nexar_client_creation() {
        let config = NexarConfig {
            client_id: "test".to_string(),
            client_secret: "test".to_string(),
            ..Default::default()
        };
        
        let client = NexarClient::new(config);
        assert!(client.is_ok());
    }
}