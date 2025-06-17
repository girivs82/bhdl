#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! edition = "2021"
//! [dependencies]
//! reqwest = { version = "0.11", features = ["json"] }
//! serde_json = "1.0"
//! serde = { version = "1.0", features = ["derive"] }
//! tokio = { version = "1.0", features = ["full"] }
//! dotenv = "0.15"
//! ```
//! Simple test script to verify Nexar API connectivity

use reqwest::{Client, header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE}};
use serde_json::{Value, json};
use std::env;

#[derive(serde::Deserialize)]
struct AuthResponse {
    access_token: String,
    expires_in: i64,
    token_type: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables from .env file
    dotenv::dotenv().ok();
    
    let client_id = env::var("NEXAR_CLIENT_ID")
        .expect("NEXAR_CLIENT_ID environment variable not set");
    let client_secret = env::var("NEXAR_CLIENT_SECRET")
        .expect("NEXAR_CLIENT_SECRET environment variable not set");
    
    println!("🔑 Testing Nexar API with credentials:");
    println!("   Client ID: {}...", &client_id[0..10]);
    println!("   Client Secret: {}...", &client_secret[0..10]);
    
    let client = Client::new();
    
    // Step 1: Authenticate
    println!("\n🔐 Step 1: Authenticating with Nexar...");
    
    let auth_url = "https://identity.nexar.com/connect/token";
    let params = [
        ("grant_type", "client_credentials"),
        ("client_id", &client_id),
        ("client_secret", &client_secret),
        ("scope", "supply.domain"),
    ];

    let auth_response = client
        .post(auth_url)
        .form(&params)
        .send()
        .await?;

    if !auth_response.status().is_success() {
        let error_text = auth_response.text().await?;
        panic!("❌ Authentication failed: {}", error_text);
    }

    let auth_data: AuthResponse = auth_response.json().await?;
    println!("✅ Authentication successful!");
    println!("   Token type: {}", auth_data.token_type);
    println!("   Expires in: {} seconds", auth_data.expires_in);
    
    // Step 2: Test component search
    println!("\n🔍 Step 2: Searching for component 'LM358'...");
    
    let graphql_url = "https://api.nexar.com/graphql";
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
                        sellers(limit: 3) {
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

    let variables = json!({
        "queries": [{"mpn": "LM358", "limit": 3}]
    });

    let graphql_query = json!({
        "query": query,
        "variables": variables
    });

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", auth_data.access_token))?);

    let search_response = client
        .post(graphql_url)
        .headers(headers)
        .json(&graphql_query)
        .send()
        .await?;

    if !search_response.status().is_success() {
        let error_text = search_response.text().await?;
        panic!("❌ Search request failed: {}", error_text);
    }

    let search_data: Value = search_response.json().await?;
    
    // Check for GraphQL errors
    if let Some(errors) = search_data.get("errors") {
        println!("⚠️  GraphQL errors: {}", serde_json::to_string_pretty(errors)?);
    }
    
    if let Some(data) = search_data.get("data") {
        if let Some(search_results) = data.get("supSearchMpn") {
            if let Some(results) = search_results.get("results") {
                let empty_vec = vec![];
                let results_array = results.as_array().unwrap_or(&empty_vec);
                println!("✅ Search successful! Found {} results", results_array.len());
                
                for (i, result) in results_array.iter().enumerate() {
                    if let Some(part) = result.get("part") {
                        let mpn = part.get("mpn").and_then(|v| v.as_str()).unwrap_or("Unknown");
                        let manufacturer = part.get("manufacturer")
                            .and_then(|m| m.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown");
                        let description = part.get("shortDescription")
                            .and_then(|v| v.as_str())
                            .unwrap_or("No description");
                        
                        println!("\n📦 Result {}: {} by {}", i + 1, mpn, manufacturer);
                        println!("   📝 {}", description);
                        
                        if let Some(median_price) = part.get("medianPrice1000") {
                            if let Some(price) = median_price.get("price").and_then(|v| v.as_f64()) {
                                if let Some(currency) = median_price.get("currency").and_then(|v| v.as_str()) {
                                    println!("   💰 Median price (1000+): {:.4} {}", price, currency);
                                }
                            }
                        }
                        
                        if let Some(sellers) = part.get("sellers") {
                            if let Some(sellers_array) = sellers.as_array() {
                                println!("   🏪 {} seller(s) found:", sellers_array.len());
                                
                                for seller in sellers_array.iter().take(2) {
                                    if let Some(company) = seller.get("company").and_then(|c| c.get("name")).and_then(|v| v.as_str()) {
                                        println!("     • {}", company);
                                        
                                        if let Some(offers) = seller.get("offers") {
                                            if let Some(offers_array) = offers.as_array() {
                                                for offer in offers_array.iter().take(1) {
                                                    if let Some(stock) = offer.get("inventoryLevel").and_then(|v| v.as_i64()) {
                                                        println!("       📦 Stock: {}", stock);
                                                    }
                                                    if let Some(moq) = offer.get("moq").and_then(|v| v.as_i64()) {
                                                        println!("       📊 MOQ: {}", moq);
                                                    }
                                                    
                                                    if let Some(prices) = offer.get("prices") {
                                                        if let Some(prices_array) = prices.as_array() {
                                                            if let Some(first_price) = prices_array.first() {
                                                                if let (Some(qty), Some(price), Some(currency)) = (
                                                                    first_price.get("quantity").and_then(|v| v.as_i64()),
                                                                    first_price.get("price").and_then(|v| v.as_f64()),
                                                                    first_price.get("currency").and_then(|v| v.as_str())
                                                                ) {
                                                                    println!("       💰 {}+ units: {:.4} {}", qty, price, currency);
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                println!("❌ No results found in response");
            }
        } else {
            println!("❌ No search data found in response");
        }
    } else {
        println!("❌ No data found in response");
        println!("Response: {}", serde_json::to_string_pretty(&search_data)?);
    }
    
    println!("\n🎉 Nexar API test completed successfully!");
    println!("💡 Your credentials are working and can access supplier data!");
    
    Ok(())
}