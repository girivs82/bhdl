//! Configuration management for BHDL Components (simplified version)

use anyhow::Result;
use std::path::PathBuf;
use std::fs;

/// Simplified configuration that uses environment variables only
#[derive(Debug, Clone)]
pub struct SupplierConfig {
    pub nexar_client_id: Option<String>,
    pub nexar_client_secret: Option<String>,
    pub digikey_client_id: Option<String>,
    pub digikey_client_secret: Option<String>,
    pub default_backend: String,
}

impl SupplierConfig {
    /// Load configuration from environment variables
    pub fn load() -> Result<Self> {
        let config = Self {
            nexar_client_id: std::env::var("NEXAR_CLIENT_ID").ok(),
            nexar_client_secret: std::env::var("NEXAR_CLIENT_SECRET").ok(),
            digikey_client_id: std::env::var("DIGIKEY_CLIENT_ID").ok(),
            digikey_client_secret: std::env::var("DIGIKEY_CLIENT_SECRET").ok(),
            default_backend: std::env::var("BHDL_DEFAULT_BACKEND")
                .unwrap_or_else(|_| "auto".to_string()),
        };
        
        Ok(config)
    }
    
    /// Check if Nexar is configured
    pub fn has_nexar(&self) -> bool {
        self.nexar_client_id.is_some() && self.nexar_client_secret.is_some()
    }
    
    /// Check if DigiKey is configured
    pub fn has_digikey(&self) -> bool {
        self.digikey_client_id.is_some() && self.digikey_client_secret.is_some()
    }
    
    /// Create example configuration file
    pub fn create_example_config() -> Result<()> {
        let example_content = r#"# BHDL Components Supplier Configuration Example
# 
# This file shows how to configure API credentials for supplier backends.
# Copy this file to 'bhdl-supplier-config.toml' and update with your credentials.
# 
# Alternatively, you can set environment variables:

# Nexar API (Free tier: 1,000 calls/month)
# Get credentials at: https://nexar.com/api
# export NEXAR_CLIENT_ID="your_client_id"
# export NEXAR_CLIENT_SECRET="your_client_secret"

# DigiKey API (Free for registered developers)  
# Get credentials at: https://developer.digikey.com
# export DIGIKEY_CLIENT_ID="your_client_id"
# export DIGIKEY_CLIENT_SECRET="your_client_secret"

# Default backend (nexar, digikey, auto)
# export BHDL_DEFAULT_BACKEND="auto"

[nexar]
# client_id = "your_nexar_client_id"
# client_secret = "your_nexar_client_secret"
# enabled = true

[digikey]
# client_id = "your_digikey_client_id"  
# client_secret = "your_digikey_client_secret"
# enabled = true

[settings]
# default_backend = "auto"
# max_concurrent_requests = 3
"#;
        
        let example_path = PathBuf::from("bhdl-supplier-config.example.toml");
        fs::write(&example_path, example_content)?;
        
        Ok(())
    }
}

/// Convert to backend-specific configs
impl SupplierConfig {
    pub fn to_nexar_config(&self) -> Option<crate::supplier::nexar::NexarConfig> {
        if let (Some(client_id), Some(client_secret)) = (&self.nexar_client_id, &self.nexar_client_secret) {
            Some(crate::supplier::nexar::NexarConfig {
                client_id: client_id.clone(),
                client_secret: client_secret.clone(),
                ..Default::default()
            })
        } else {
            None
        }
    }
    
    pub fn to_digikey_config(&self) -> Option<crate::supplier::digikey::DigiKeyConfig> {
        if let (Some(client_id), Some(client_secret)) = (&self.digikey_client_id, &self.digikey_client_secret) {
            Some(crate::supplier::digikey::DigiKeyConfig {
                client_id: client_id.clone(),
                client_secret: client_secret.clone(),
                ..Default::default()
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_config_load() {
        let config = SupplierConfig::load().unwrap();
        // Should not fail even without environment variables
        assert_eq!(config.default_backend, "auto");
    }
}