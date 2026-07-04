use anyhow::Result;
use reqwest::{Client, header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE}};
use serde_json::{Value, json};
use std::env;

#[derive(serde::Deserialize)]
struct DigiKeyAuthResponse {
    access_token: String,
    expires_in: i64,
    token_type: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file
    dotenv::dotenv().ok();
    
    let client_id = env::var("DIGIKEY_CLIENT_ID")
        .expect("DIGIKEY_CLIENT_ID environment variable not set");
    let client_secret = env::var("DIGIKEY_CLIENT_SECRET")
        .expect("DIGIKEY_CLIENT_SECRET environment variable not set");
    
    println!("🔑 Testing DigiKey API with credentials:");
    println!("   Client ID: {}...", &client_id[0..10]);
    println!("   Client Secret: {}...", &client_secret[0..10]);
    
    let client = Client::new();
    
    // Step 1: Authenticate
    println!("\n🔐 Step 1: Authenticating with DigiKey...");
    
    let auth_url = "https://api.digikey.com/v1/oauth2/token";
    let params = [
        ("client_id", client_id.as_str()),
        ("client_secret", client_secret.as_str()),
        ("grant_type", "client_credentials"),
    ];

    let auth_response = client
        .post(auth_url)
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .form(&params)
        .send()
        .await?;

    println!("Auth response status: {}", auth_response.status());

    if !auth_response.status().is_success() {
        let error_text = auth_response.text().await?;
        println!("❌ Authentication failed: {}", error_text);
        return Ok(());
    }

    let auth_data: DigiKeyAuthResponse = auth_response.json().await?;
    println!("✅ Authentication successful!");
    println!("   Token type: {}", auth_data.token_type);
    println!("   Expires in: {} seconds", auth_data.expires_in);
    
    // Step 2: Test component search
    println!("\n🔍 Step 2: Searching for component 'LM358'...");
    
    let search_request = json!({
        "Keywords": "LM358",
        "RecordCount": 5,
        "RecordStartPosition": 0,
        "Sort": {
            "Option": "SortByUnitPrice",
            "Direction": "Ascending"
        }
    });

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", auth_data.access_token))?);
    headers.insert("X-DIGIKEY-Client-Id", HeaderValue::from_str(&client_id)?);

    let search_url = "https://api.digikey.com/products/v4/search/keyword";
    
    let search_response = client
        .post(search_url)
        .headers(headers)
        .json(&search_request)
        .send()
        .await?;

    println!("Search response status: {}", search_response.status());

    if !search_response.status().is_success() {
        let error_text = search_response.text().await?;
        println!("❌ Search request failed: {}", error_text);
        return Ok(());
    }

    let search_data: Value = search_response.json().await?;
    
    // Parse the results
    if let Some(products) = search_data.get("Products") {
        if let Some(products_array) = products.as_array() {
            println!("✅ Search successful! Found {} results", products_array.len());
            
            for (i, product) in products_array.iter().enumerate().take(3) {
                let mpn = product.get("ManufacturerProductNumber").and_then(|v| v.as_str()).unwrap_or("Unknown");
                let manufacturer = product.get("Manufacturer")
                    .and_then(|m| m.get("Name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown");
                let description = product.get("Description")
                    .and_then(|d| d.get("ProductDescription"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("No description");
                let detailed_description = product.get("Description")
                    .and_then(|d| d.get("DetailedDescription"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("No detailed description");
                
                println!("\n📦 Result {}: {} by {}", i + 1, mpn, manufacturer);
                println!("   📝 {}", description);
                println!("   📋 {}", detailed_description);
                
                if let Some(qty_available) = product.get("QuantityAvailable").and_then(|v| v.as_i64()) {
                    println!("   📦 Stock: {}", qty_available);
                }
                
                if let Some(unit_price) = product.get("UnitPrice").and_then(|v| v.as_f64()) {
                    println!("   💰 Unit price: ${:.4}", unit_price);
                }
                
                // Show package variations with pricing
                if let Some(variations) = product.get("ProductVariations") {
                    if let Some(variations_array) = variations.as_array() {
                        println!("   📦 Package options:");
                        for variation in variations_array.iter().take(2) {
                            if let Some(package) = variation.get("PackageType").and_then(|p| p.get("Name")).and_then(|v| v.as_str()) {
                                if let Some(digikey_pn) = variation.get("DigiKeyProductNumber").and_then(|v| v.as_str()) {
                                    if let Some(moq) = variation.get("MinimumOrderQuantity").and_then(|v| v.as_i64()) {
                                        if let Some(qty_available) = variation.get("QuantityAvailableforPackageType").and_then(|v| v.as_i64()) {
                                            println!("     • {} ({}): {} in stock, MOQ: {}", package, digikey_pn, qty_available, moq);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            println!("❌ No products array found in response");
        }
    } else {
        println!("❌ No products found in response");
        println!("Response: {}", serde_json::to_string_pretty(&search_data)?);
    }
    
    println!("\n🎉 DigiKey API test completed successfully!");
    println!("💡 Your credentials are working and can access supplier data!");
    
    Ok(())
}