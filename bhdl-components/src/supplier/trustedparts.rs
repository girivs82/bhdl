//! TrustedParts API Integration
//! 
//! Provides integration with TrustedParts.com API for real-time component 
//! availability, pricing, and supply chain data aggregation.

use anyhow::{Result, Context};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use log::{debug, warn, error};

use crate::types::{ComponentId, SupplierData, SupplierInfo, PriceBreak};

/// TrustedParts API client configuration
#[derive(Debug, Clone)]
pub struct TrustedPartsConfig {
    /// API base URL
    pub base_url: String,
    /// API key for authentication
    pub api_key: Option<String>,
    /// Request timeout in seconds
    pub timeout_seconds: u64,
    /// Maximum retries for failed requests
    pub max_retries: u32,
}

impl Default for TrustedPartsConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.trustedparts.com".to_string(),
            api_key: None,
            timeout_seconds: 30,
            max_retries: 3,
        }
    }
}

/// TrustedParts API client
pub struct TrustedPartsClient {
    client: Client,
    config: TrustedPartsConfig,
}

impl TrustedPartsClient {
    /// Create a new TrustedParts API client
    pub fn new(config: TrustedPartsConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_seconds))
            .user_agent("BHDL-Components/0.1.0")
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client, config })
    }

    /// Search for components by part number
    pub async fn search_part(&self, part_number: &str) -> Result<Vec<TrustedPartsComponent>> {
        let url = format!("{}/v1/search", self.config.base_url);
        
        let mut params = vec![
            ("q", part_number),
            ("format", "json"),
            ("limit", "50"),
        ];

        // Add API key if available
        let api_key_param;
        if let Some(ref api_key) = self.config.api_key {
            api_key_param = ("apikey", api_key.as_str());
            params.push(api_key_param);
        }

        debug!("Searching TrustedParts for: {}", part_number);

        let response = self.client
            .get(&url)
            .query(&params)
            .send()
            .await
            .context("Failed to send request to TrustedParts")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "TrustedParts API error {}: {}", 
                status, 
                error_text
            ));
        }

        let search_response: TrustedPartsSearchResponse = response
            .json()
            .await
            .context("Failed to parse TrustedParts response")?;

        debug!("Found {} results for {}", search_response.results.len(), part_number);
        Ok(search_response.results)
    }

    /// Get detailed component information including suppliers and pricing
    pub async fn get_component_details(&self, uid: &str) -> Result<TrustedPartsComponentDetails> {
        let url = format!("{}/v1/parts/{}", self.config.base_url, uid);
        
        let mut params = vec![("format", "json")];

        // Add API key if available
        let api_key_param;
        if let Some(ref api_key) = self.config.api_key {
            api_key_param = ("apikey", api_key.as_str());
            params.push(api_key_param);
        }

        debug!("Getting component details for UID: {}", uid);

        let response = self.client
            .get(&url)
            .query(&params)
            .send()
            .await
            .context("Failed to get component details from TrustedParts")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "TrustedParts API error {}: {}", 
                status, 
                error_text
            ));
        }

        let details: TrustedPartsComponentDetails = response
            .json()
            .await
            .context("Failed to parse component details")?;

        debug!("Retrieved details for {}", details.mpn);
        Ok(details)
    }

    /// Convert TrustedParts data to our internal supplier data format
    pub fn convert_to_supplier_data(&self, 
        component_id: ComponentId, 
        tp_component: &TrustedPartsComponent,
        tp_details: Option<&TrustedPartsComponentDetails>
    ) -> SupplierData {
        let mut suppliers = Vec::new();

        // Process supplier offers if we have detailed data
        if let Some(details) = tp_details {
            for offer in &details.offers {
                let price_breaks = offer.prices.iter().map(|price| {
                    PriceBreak {
                        quantity: price.quantity,
                        unit_price: price.price,
                        currency: price.currency.clone().unwrap_or_else(|| "USD".to_string()),
                    }
                }).collect();

                let supplier_info = SupplierInfo {
                    supplier_name: offer.company.name.clone(),
                    supplier_part_number: offer.sku.clone(),
                    manufacturer_part_number: tp_component.mpn.clone(),
                    manufacturer: tp_component.manufacturer.clone(),
                    availability: offer.inventory_level.unwrap_or(0),
                    lead_time_days: offer.factory_lead_days,
                    moq: offer.moq.unwrap_or(1),
                    price_breaks,
                    datasheet_url: tp_component.datasheet_url.clone(),
                    last_updated: Utc::now(),
                };

                suppliers.push(supplier_info);
            }
        } else {
            // Create basic supplier info from search result only
            let supplier_info = SupplierInfo {
                supplier_name: "TrustedParts".to_string(),
                supplier_part_number: tp_component.mpn.clone(),
                manufacturer_part_number: tp_component.mpn.clone(),
                manufacturer: tp_component.manufacturer.clone(),
                availability: 0, // Unknown without details
                lead_time_days: None,
                moq: 1,
                price_breaks: Vec::new(),
                datasheet_url: tp_component.datasheet_url.clone(),
                last_updated: Utc::now(),
            };

            suppliers.push(supplier_info);
        }

        SupplierData {
            component_id,
            suppliers,
            last_updated: Utc::now(),
        }
    }
}

// TrustedParts API Response Types

#[derive(Debug, Deserialize)]
pub struct TrustedPartsSearchResponse {
    pub results: Vec<TrustedPartsComponent>,
    pub total_results: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct TrustedPartsComponent {
    pub uid: String,
    pub mpn: String,
    pub manufacturer: String,
    pub description: Option<String>,
    pub datasheet_url: Option<String>,
    pub image_url: Option<String>,
    pub lifecycle_status: Option<String>,
    pub rohs_status: Option<String>,
    pub categories: Option<Vec<String>>,
    pub parameters: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
pub struct TrustedPartsComponentDetails {
    pub uid: String,
    pub mpn: String,
    pub manufacturer: String,
    pub description: Option<String>,
    pub datasheet_url: Option<String>,
    pub offers: Vec<TrustedPartsOffer>,
    pub parameters: Option<HashMap<String, String>>,
    pub compliance: Option<TrustedPartsCompliance>,
}

#[derive(Debug, Deserialize)]
pub struct TrustedPartsOffer {
    pub company: TrustedPartsCompany,
    pub sku: String,
    pub inventory_level: Option<i32>,
    pub moq: Option<i32>,
    pub factory_lead_days: Option<i32>,
    pub prices: Vec<TrustedPartsPrice>,
    pub packaging: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct TrustedPartsCompany {
    pub name: String,
    pub display_flag: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TrustedPartsPrice {
    pub quantity: i32,
    pub price: f64,
    pub currency: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TrustedPartsCompliance {
    pub rohs: Option<String>,
    pub reach: Option<String>,
    pub halogen_free: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;

    #[tokio::test]
    async fn test_trustedparts_client_creation() {
        let config = TrustedPartsConfig::default();
        let client = TrustedPartsClient::new(config);
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_supplier_data_conversion() {
        let client = TrustedPartsClient::new(TrustedPartsConfig::default()).unwrap();
        
        let tp_component = TrustedPartsComponent {
            uid: "test-uid".to_string(),
            mpn: "LM358N".to_string(),
            manufacturer: "Texas Instruments".to_string(),
            description: Some("Dual Op-Amp".to_string()),
            datasheet_url: Some("https://example.com/datasheet.pdf".to_string()),
            image_url: None,
            lifecycle_status: Some("Active".to_string()),
            rohs_status: Some("Compliant".to_string()),
            categories: None,
            parameters: None,
        };

        let supplier_data = client.convert_to_supplier_data(1, &tp_component, None);
        
        assert_eq!(supplier_data.component_id, 1);
        assert_eq!(supplier_data.suppliers.len(), 1);
        assert_eq!(supplier_data.suppliers[0].manufacturer_part_number, "LM358N");
        assert_eq!(supplier_data.suppliers[0].manufacturer, "Texas Instruments");
    }
}